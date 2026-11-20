# 异步入口审计：仓库侧的六项读数

> **性质**：只读审计。不改产品代码与配置，本文件是唯一写入。
> **字节**：所有行号都是 `HEAD = d30931b`（`d30931be34c0bafb368a9c0ac954d3362aaac584`，
> `2026-11-20 01:39:41 +0800`）的检出状态（`git status --porcelain` 为空）上的行号。
> 行号会漂，**符号比行号可靠**，所以引文后面都带符号名。
> **问题**：用户的原话是「我只需要和一个主 agent 对话，所有事情都能由这个主 agent 进行管理分发监督」。
> 本文件只回答「仓库离这个目标还差什么」，不设计完整方案。
> **未做的事**（纪律）：没有触碰 `8787` 上的常驻实例（任务书告知 pid 49800），没有起私有守护进程，没有跑
> `cargo test` / `npm run build`，没有改任何配置。所有读数来自源码字节 + 仓库里已有的测试断言。

---

## 1 一句话结论

**现状是「异步执行 + 拉取式交付」，不是「异步任务系统」。**

拆开说，由后面六节支撑：

- **执行层已经是异步的**，而且相当完整：`POST /api/v1/tasks/{id}/runs` 立刻返回一条
  `queued`/`spawning` 的 run 行，真正的推进在 `tokio::spawn` 出去的 supervisor 里；有每 harness
  的并发闸门与排队上限、有取消令牌、有 append-only 的 `transcripts/*.jsonl`、有终态广播、
  有重启后的 `Interrupted` 清扫。这一层不是「差一点」，是**已经有了**。
- **交付层是「拉」的**：没有「完成」的主动通知。用户切走页面再回来能看到结果，靠的是
  (a) `runs` 行 + transcript 文件，(b) 面板重挂载时重新 GET，(c) 打开某个 run 时 SSE 从 seq 0
  重放。**没有任何一条路径会把「活干完了」推给用户，更不会推回对话。**
- **`#inbox` 不是回执箱**：它的数据源是 `/api/v1/permissions`，条目是「某 run 的某个 tool_call
  在等人裁决」，存在进程内存里，run 一结束就被清掉。它是**阻塞队列的审批台**，不是异步回执的落地。
- **最关键的一条：主 agent 无法被唤醒。** `Chat::send_prompt` 在整个 workspace 里的唯一调用者
  就是 `chat_message` 这个 HTTP handler。也就是说，除了用户打字，**没有任何东西能把一条消息投进
  一个会话并驱动一轮**。这是「主 agent 分发与监督」缺的那块地基——不是缺定时器（定时器有四套，
  都在跑，都不通向 agent），是缺一个「内部可以唤醒会话」的入口。

所以准确的说法是：**异步执行、同步交付、无法唤醒。** 缺的不是执行地基，是回执与唤醒。

---

## 2 六项读数

### 2.1 run 的生命周期是不是异步的 —— 是，而且是两套都异步

**读数**

两条入口都立刻返回，都不阻塞到结束：

```
crates/daemon/src/api.rs:190      .route("/api/v1/chat/{id}/messages", post(chat_message))
crates/daemon/src/api.rs:188      .route("/api/v1/chat", post(chat_start).get(chat_list))
crates/daemon/src/api.rs:162      .route("/api/v1/tasks/{id}/runs", post(start_run))
```

对话路径：`POST /api/v1/chat` 返回 `200` + 身份（`api.rs:7051-7057`，`chat_start`）；
`POST /api/v1/chat/{id}/messages` 返回 **`202 ACCEPTED`**（`api.rs:7084-7087`，`chat_message`）：

```rust
    chat.send_prompt(state.mgr.db(), state.knowledge.embedder(), req.text)
        .await
        .map_err(|e| ApiError::bad_request(format!("{e:#}")))?;
    Ok(StatusCode::ACCEPTED)
```

`send_prompt` 本身只做三件事：拼注入上下文、写 `chats.updated_at`、**把命令投进 channel 就返回**
（`crates/daemon/src/chat.rs:252-271`，`Chat::send_prompt`）：

```rust
        self.generating
            .store(true, std::sync::atomic::Ordering::SeqCst);
        ...
        let sent = self.send(ChatCommand::Prompt {
            text,
            context,
            context_budget: ctx_budget,
        });
```

真正的一轮在 ACP 会话自己的任务里跑（`crates/acp/src/chat.rs:263` 起的 `tokio::spawn`，
事件在 `:508`/`:558`/`:602` 等处发到 `broadcast::Sender<RunEvent>`）。

run 路径：`start_run` 落库后 `tokio::spawn` 出去，然后**把落库时的那份 clone 直接返回**：

```rust
crates/daemon/src/runs.rs:763    let returned = run.clone();
crates/daemon/src/runs.rs:776    tokio::spawn(async move {
crates/daemon/src/runs.rs:926        Ok(returned)
crates/daemon/src/runs.rs:927    }
```

状态机是 8 态的，`queued` 是**新的 run 的初始态**（`crates/core/src/run.rs:13-30`，`RunStatus`；
`crates/core/src/run.rs:173-180`，`Run::new` → `status: RunStatus::Queued`）：

```
Queued → Spawning → Running → {Completed | Failed | Cancelled | Interrupted}
                 ↘ WaitingPermission（见下面的判读：这个态从来没有人写过）
```

迁移点在 `start_run`（`runs.rs:734` 设 `Queued`、`runs.rs:741` 设 `Spawning`）和 `supervise`
（`runs.rs:1484-1491` 发 `StateChanged{Spawning}`、`runs.rs:1602-1611` 在会话起来后置
`Running`、`runs.rs:1709/1734/1745` 置终态）。

事件写作两处落地：**JSONL 文件** + **进程内广播**，同一次写入产生两份（`runs.rs:1433` 建
`TranscriptWriter`，`:1807-1819` `append_event`）：

```rust
crates/daemon/src/runs.rs:1433    let mut transcript = TranscriptWriter::create(transcript_path(&transcripts_dir, &run.id))
crates/daemon/src/runs.rs:1816        if let Err(e) = transcript.append(event) {
crates/daemon/src/runs.rs:1817            tracing::warn!(run_id = %run.id, error = %e, "transcript append failed");
```

对话路径有一份对称的落地：`crates/daemon/src/chat.rs:1002-1029` 起一个 forwarder 任务，把会话
broadcast 里的每个事件 append 进**同一个 `transcripts/` 目录**下的 `run-<id>.jsonl`。

仓库自带的 e2e 测试直接断言了「POST 返回时活还没干完」，这是异步性的第一手证据
（`crates/mock-agent/tests/e2e_daemon.rs:1624/1632`）：

```rust
    assert_eq!(a["status"], "spawning", "free slot spawns immediately");
    ...
    assert_eq!(b["status"], "queued", "second run queues: {b}");
```

**判读**

- 「异步」这件事在仓库里是**既成事实**，不需要新建：返回 id、后台推进、可订阅、可取消、可重试
  的全套都在 `runs.rs` 里。
- 但两套异步的**成熟度不一样**：`runs` 有闸门（2.5）、有终态落库、有重启清扫；**chat 一个都没有**
  ——chat 不受闸门约束，`chats` 表没有 status 列（`crates/store/src/migrations/0013_chats.sql`
  只有 id/agent/runtime/model/title/created_at/updated_at，`0015_chats_cwd.sql` 加 cwd），
  守护进程重启后一轮被打断的对话**不留任何标记**，transcript 就停在那里。
- `RunStatus::WaitingPermission` 是个**空态**：整个 workspace 只有格式化/反序列化/清扫列表在读它，
  **没有任何写入点**（`grep WaitingPermission` 的全部 8 处命中：`acp/permission.rs:6` 是注释、
  `core/run.rs` 与 `store/lib.rs` 是映射、`daemon/lib.rs:522` 是重启清扫列表）。面板上看起来的
  「等待权限」是 `get_task` **在读取时 join 内存里的 pending map** 拼出来的
  （`api.rs:3314-3328`），不是行上的状态。这对「回执」有直接影响：**密码不在行里，行就无法承载
  「曾卡在权限上」这段历史。**

### 2.2 一个 run 的结果怎么回到用户眼前 —— 轮询 + SSE 重放；没有推送，也没有回流

**读数**

终态有明确的事件与广播。`supervise` 收尾处先落库、再置任务、最后广播
（`runs.rs:1794-1802`）：

```rust
    transcript.flush()?;
    db.update_run(&run).await?;
    if run.status == RunStatus::Completed {
        db.update_task_status(task_id, TaskStatus::Done).await?;
    }
    let _ = broadcast.send(StreamMsg::Finished {
        run_id: run.id,
        status: run.status,
    });
```

轮询端点有，而且是**纯读库**，没有长等待（`api.rs:3939-3953`，`get_run`：

```rust
    let run = state
        .mgr
        .db()
        .get_run(id)
        .await?
        .ok_or_else(|| ApiError::not_found("run not found"))?;
    Ok(Json(run))
```

MCP 侧同样保证不阻塞（`crates/mcp/src/lib.rs:706` 的 tool description 原文：
`"THIS TOOL DOES NOT WAIT: it is a single GET with no polling loop and no sleep"`，
实现 `:708-721` 只发一次 GET）。

SSE 有，而且是**先重放 transcript 再从 seq 继续**，所以「切走再回来」是设计内的
（`api.rs:3955-4008`，`run_events`）：

```rust
    let replay = ruagent_store::read_transcript(&path).unwrap_or_default();
    let terminal_at_start = run.status.is_terminal();
    ...
        for line in replay {
            last_seq = line.seq;
            let _ = tx.send(sse_data(&line));
        }
        if terminal_at_start {
            let _ = tx.send(sse_end(run.status));
            return;
        }
```

对话有对称的一条（`api.rs:7190-7219`，`chat_events`：先 `subscribe()` 再读文件，注释明确
「subscribe BEFORE reading the transcript」以避免读窗口丢事件；`StateChanged{completed}`
= 会话结束）。

面板**列表靠轮询、单条靠 SSE**（读数，逐处）：
`panel/src/views/Board.tsx:370` `setInterval(load, 3000)`；
`panel/src/views/TaskDetail.tsx:116` `setInterval(refresh, 2000)`；
`panel/src/views/Home.tsx:308` `setInterval(() => void load(), 10000)`；
`panel/src/views/Agents.tsx:1126` `usePoll(load, 2000)`（权限队列）；
`panel/src/App.tsx:357`（外壳的 `permissions` 轮询）；
SSE 只有两处：`panel/src/views/Chat.tsx:814` `new EventSource(\`/api/v1/chat/${id}/events\`)` 与
`panel/src/views/RunTimeline.tsx:180` `new EventSource(\`/api/v1/runs/${run.id}/events\`)`。

「回来后还能看到什么」的机制：
- **run**：`GET /api/v1/tasks/{id}` 一次返回 task + 该 task 的全部 runs（含 `result`、`status`、
  `error`、`waiting_permission` 拼装；`api.rs:3286-3328`，`get_task`）。
- **chat**：`GET /api/v1/chats` 的每行带 `active`/`generating`，但这两个值**只来自内存里的活会话
  注册表**（`crates/daemon/src/chat.rs:1896-1897`：
  `e.active = live.contains_key(&e.id); e.generating = live.get(&e.id).copied().unwrap_or(false);`），
  重启后必然 `false`，且没有「这一轮被中断」的第三态。
- **对话正文**：重开会话时走 `chat_events` 的 transcript 重放。

持久化载体只有两个：SQLite 的 `runs`/`tasks`/`chats` 行，和 `data/transcripts/*.jsonl`
（`RunEvent` 逐条，带 `ts` + `seq`）。**没有「已完成但未读」这类列**，也就没有「未读/已读」概念。

**判读**

- 「切走再回来还能看到结果」= **能**，而且不是靠运气：transcript 是 append-only 文件、
  run 行有 `result`、SSE 会重放。这一点比很多工具做得好。
- 但「结果回来找你」= **不能**。缺三样东西，每样都在代码里能指认：
  1. **没有 run 列表端点**。路由表里只有 `GET /api/v1/runs/{id}`（`api.rs:167`），
     **没有 `GET /api/v1/runs`**。所以「最近完成了什么」只能顺着 task 一个个点进去看。
  2. **没有完成计数/未读读数**。`Home.tsx` 的数（`Home.tsx:256-302`）是 agents/sessions/
     tasks/runs 总数/memories/docs 六项，没有「刚完成 N 个」。
  3. **结果不会回到对话**。`send_prompt` 的唯一调用者是 `chat_message`
     （全仓 `grep send_prompt\(` / `ChatCommand::Prompt`：`api.rs:7084`、`chat.rs:261`（自身）、
     `distill.rs:1514`（蒸馏器自己的临时会话）、以及测试）。**没有任何「run 完成后唤醒对话」的路径**，
     也没有「把 run 结果作为一条消息注入会话」的路径。
- 所以用户体感上的「每次要在不同窗口和多个 agent 交互」不是错觉：**每一个 run 都是一扇需要你回去
  看的窗**，而「谁干完了」这个信息在平台上不存在。

### 2.3 inbox 是什么 —— 权限审批台，不是异步回执

**读数**

路由与视图：`#inbox` → `Inbox()`（`panel/src/views/Agents.tsx:1107`，懒加载挂载在
`panel/src/App.tsx:602`）。数据源一行就说完（`panel/src/api.ts:1636-1637`）：

```ts
  pendingPermissions: () =>
    get<{ pending: PendingPermission[] }>("/api/v1/permissions").then((r) => r.pending),
```

裁决端点是 `POST /api/v1/permissions/{runId}:{toolCallId}`（`panel/src/api.ts:1642`）。
daemon 侧：路由在 `api.rs:240-241`，handler 极短（`api.rs:6844-6847`）：

```rust
async fn list_permissions(State(state): State<AppState>) -> Json<serde_json::Value> {
    let pending: Vec<PendingPermission> = state.mgr.pending_permissions();
    Json(serde_json::json!({ "pending": pending }))
}
```

条目的类型与粒度（`crates/daemon/src/runs.rs:110-128`，`PendingPermission` / `PendingMap`）：

```rust
pub struct PendingPermission {
    pub run_id: RunId,
    pub tool_call_id: String,
    pub title: String,
    pub raw_input: serde_json::Value,
    pub choices: Vec<ruagent_core::PermissionChoice>,
}
...
/// Map key: `<run_id>:<tool_call_id>`.
type PendingMap = Arc<Mutex<HashMap<String, PendingEntry>>>;
```

面板类型一致（`panel/src/api.ts:220-226`）。**粒度 = 一次 run 里的一个 tool_call 的一次裁决**
——不是消息，不是 run，不是 task。

生命周期决定它不可能是回执：
- 存在**进程内存**（`Arc<Mutex<HashMap<…>>>`），不在 SQLite；
- run 一到终态，该 run 的所有 parked ask 被**主动丢弃**（`runs.rs:1777-1783`：
  `p.retain(|k, _| !k.starts_with(&prefix));`，注释原文「drop them so the inbox stays honest」）；
- 会话关闭时也会被丢（`crates/daemon/src/lib.rs:701-704` 的 `set_ask_dropper`）。

设计意图写在视图契约里（`docs/design/views/view-inbox.md` §1）：

> **职责**：**权限收件箱**——智能体要做规则未覆盖的操作时停在这里等人裁决……
> 「快速」是因为正在跑的运行被阻塞在这上面。

仓库里的 `gen4-inbox-spine-repair.md`（`docs/design/reviews/gen4-inbox-spine-repair.md`）是一份
**纯 CSS 修复报告**（`.inbox-card` 的 3px 琥珀脊线被同权重规则盖掉），与「异步回执」无关——这条要
说清楚，免得被当成「inbox 已经承载过回执」。

**判读**

- inbox 是**唯一一个跨页有徽标、有「等你处理」语义（MASTER §3.2 白名单 W3）、且有 2s 轮询的地方**
  （`App.tsx:418-431` 的侧栏徽标 + `App.tsx:357` 的外壳轮询）。所以它是**最自然的改造宿主**，
  但它现在的内容物和「异步回执」是**两个概念**：
  - 权限：**阻塞中的**run 在等你，删掉就没了；
  - 回执：**已结束的**run 想告诉你它结束了，必须持久、必须可回溯。
- 直接把它扩成「权限 + 回执」有一个具体的技术障碍（不是口味问题）：条目现在**没有持久载体**，
  也没有「已读」语义。要承载回执，必须新增一张表或一列，并新增 resolve 语义（「知道了」而不是
  「允许/拒绝」）。这是 2.3 的读数能支撑的最硬的一条结论。

### 2.4 task 系统 vs run 系统 —— 同一件事的两层，`task_dispatch` 是真派活，但「认领」不存在

**读数**

**不是同一个东西，是两层。** `tasks` 是持久意图，`runs` 是一次执行尝试；一个 task 可以有 N 个 run。

存储层（`crates/store/src/migrations/0001_init.sql`）：

```sql
CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY, title TEXT NOT NULL, intent TEXT NOT NULL, status TEXT NOT NULL,
    creator TEXT NOT NULL, project TEXT, pinned_agent TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS runs (
    id TEXT PRIMARY KEY, task_id TEXT NOT NULL, agent TEXT NOT NULL, params TEXT NOT NULL,
    status TEXT NOT NULL, acp_session_id TEXT, workspace TEXT, ...
);
```

`runs.result` 是后来加的（`0003_m2_results.sql:4`：`ALTER TABLE runs ADD COLUMN result TEXT;`），
同一条迁移给 `tasks` 加了 `selected_run_id`。

状态集不同：`TaskStatus` 5 态 `Pending|InProgress|Blocked|Done|Cancelled`
（`crates/core/src/task.rs:10-20`），`RunStatus` 8 态（见 2.1）。

**`task_dispatch` 做了什么：真派给另一个 agent 跑，不是记账。** MCP 侧只有一次 HTTP
（`crates/mcp/src/lib.rs:670-703`），body 只装调用方真的给了的字段（`:683-689`）：

```rust
        let url = format!(
            "{}/api/v1/tasks/{}/runs",
            self.config.daemon_url, params.task_id
        );
```

这个端点在 daemon 里走完整链路：路由级联决定 agent（`api.rs:3547-3638`）→
`RunManager::start_run`（`runs.rs:672`）→ 落一条 run 行 → `tokio::spawn` 起 supervisor →
真起一个 agent 子进程（`supervise` 里的 `RunOptions{program, args, …}`，`runs.rs:1496-1508`）。
run 的 `params.agent` 记的是**注册表里的 card id**，所以 MCP 还要反向查名字
（`mcp/src/lib.rs:696-703` 的注释与 `describe_agent`）。

**「任务被哪个 agent 认领」——没有人认领，也没有认领这个概念。** 三条读数：
1. `0001_init.sql` 的**文件头注释自己写着**：`-- claim/lease columns arrive with the v2 task board.`
   而 HEAD 上没有那条迁移（`grep 'ALTER TABLE tasks'` 只有 `0003` 加 `selected_run_id`）。
2. `grep -i 'claim|assignee|owner_agent'` 在 `crates/*/src/*.rs` 里的命中**全部**是别的语义
   （记忆/图里「某条结论主张什么」），**没有一处**是任务认领。
3. 没有 worker 扫 pending task。`TaskStatus::Pending` 在 `crates/*/src/*.rs` 里的全部出现是：
   构造（`crates/core/src/task.rs:88`）、HTTP 状态解析（`api.rs:8222`）、启动 run 时的判断
   （`runs.rs:883`，把 task 置 InProgress）、`store` 的字符串映射与一条通用 SQL
   （`store/lib.rs:99/550/564`）、以及一处测试（`store/lib.rs:882`）。
   **没有循环、没有 tick、没有 poll 去「拿活」**。

**并发上限**在 `crates/policy/src/lib.rs:45-63`（`ConcurrencyLimits`）：

```rust
impl Default for ConcurrencyLimits {
    fn default() -> Self {
        Self { per_harness: 2, queue_per_harness: 8 }
    }
}
```

按 **harness kind** 计（`HarnessKind` 4 种：`ClaudeCode|OpenCode|Dsh|Mock`，
`crates/core/src/agent.rs:13-29`），所以理论上限是 4×2 = 8 个 run；但**实际按你装了哪几种 runtime
算**，只有一种 runtime 时就是 2 个并发 + 8 个排队。配置面是 `policy.toml` 的 `[concurrency]`
（`policy/src/lib.rs:183-196`）。

**失败重试：只有手动的、单次的。** `POST /api/v1/runs/{id}/retry`（`api.rs:170` / `:736-752` →
`runs.rs:934`），前置条件 `old.status.is_retryable()`（Failed|Interrupted|Cancelled，
`core/run.rs:82-87`），行为是**新建一个 run**（复用 workspace + 从旧 transcript 取崩溃上下文，
`runs.rs:1988` 起的 `retry_context`）。**没有自动重试、没有退避、没有 attempts 计数**
（对比：蒸馏有有界退避与账本，`distill.rs` 有 `retries`，run 没有）。pipeline 里也**不重试**：
某一步没 `Completed` 就断链并取消下游（`runs.rs:1117-1121`）。

补一条只有跑过才会发现的读数（**这是「监督」视角下最刺眼的一处**）：
`task` 只在 run **成功**时被回写为 `Done`（`runs.rs:1796-1798`），
`update_task_status` 的全部调用点只有 4 处（`grep update_task_status`）：
`api.rs:770`（人手动 PATCH）、`runs.rs:884`（启动时置 InProgress）、
`runs.rs:1797`（Completed → Done）、`runs.rs:1941`（pipeline 未启动的下游 → Cancelled）。
**run 失败/取消时，task 永远停在 `in_progress`** —— 看板上它会一直显示「进行中」。

**判读**

- 「task 与 run 是不是同一个东西」：不是。这层分离是对的，而且够用——一个 task 多 run 正好承载
  fanout / retry / judge 三种拓扑（`crates/orchestrator/src/lib.rs:13-21` 的 `Topology`：
  `Direct|FanOut|Pipeline`）。
- 「`task_dispatch` 是不是真派活」：**是**。它会起真进程、花真 token，MCP 的 description 自己也
  这么写（`mcp/src/lib.rs:668`：`"COST: THIS SPENDS THE USER'S MODEL TOKENS"`）。
- 「任务被谁认领」：**这个问题在今天的仓库里没有答案，因为概念不存在**。agent 不是「来抢活」的，
  是「被 daemon 路由指派」的。这决定了造「主 agent 分发」时的形状：**你要的不是任务板 + 抢单，
  而是一个能主动决定「派给谁」并承担结果的调度者**——今天的 `start_run` 已经是那个执行器，
  缺的是执行器上面的决策者与下面的回执。
- 「并发上限在哪 / 有没有重试」：闸门在 `RunManager.gates`（内存信号量），重试只有手动单次。
  **对「无人值守」来说这是两个明确的缺口**：满了会 400（不是排队到磁盘），失败了没人再试。

### 2.5 并发与后台的硬约束

**读数**

并发闸门（`runs.rs:161-194` 结构 + `:713-744` 用法）：

```rust
        let gate = self.gate(card.harness);
        let hold = match gate.sem.clone().try_acquire_owned() {
            Ok(permit) => GateHold::Free(permit),
            Err(_) => {
                if let Err(e) = reserve_slot(&gate, card.harness,
                    self.policy.concurrency.queue_per_harness,
                    self.policy.concurrency.per_harness) { … return Err(e); }
                run.status = RunStatus::Queued;
```

排队的 run **不占资源**：先 `insert_run`，等拿到 permit 才建 workspace、才做注入渲染
（`runs.rs:806-812` 的注释与顺序；测试 `e2e_daemon.rs:1635-1638` 断言 `workspace` 为 `null`）。
满队时**拒绝**并给一句可读的话（`runs.rs:186-193`：`"harness {kind:?} is saturated: {queue_cap} runs
already waiting ({running} running) — retry when the current runs finish"`）。

**队列不是持久队列**：它是 `tokio::sync::Semaphore` + 一个等待计数器，进程内存里。
守护进程一重启，`Queued` 行被改 `Interrupted`（下面），**不会重新入队**。

取消端点有两个，run 和 chat 分开：
- run：`POST /api/v1/runs/{id}/cancel`（`api.rs:169` / `:706-732`）→
  `RunManager::cancel_run`（`runs.rs:366-377`）只是 `token.cancel()`；真正动作在
  `supervise` 的 select 分支里（`runs.rs:1690-1727`）：`driver.abort()` + `Stopped{Cancelled}` +
  `StateChanged{Cancelled}`，并**用 `reclaim_run_child` 复核子进程真的死了**。
- chat：`POST /api/v1/chat/{id}/stop`（`api.rs:191` / `:7094-7108`）→ `ChatCommand::Stop`
  （`$/cancel_request`），会话留着可以继续下一轮。
- **MCP 没有 cancel 工具**（`crates/mcp/src/lib.rs` 里 `grep cancel` 只命中一处
  `client.cancel().await`，即 reqwest 的请求取消）。所以 agent 自己能派活，**不能收活**。

重启恢复（`crates/daemon/src/lib.rs:515-534`）：

```rust
    let mut recovered = 0usize;
    for status in [
        RunStatus::Queued, RunStatus::Spawning, RunStatus::Running, RunStatus::WaitingPermission,
    ] {
        for mut run in db.list_runs_by_status(status).await? {
            run.status = RunStatus::Interrupted;
            run.error = Some("daemon restarted mid-run".into());
```

孤儿清扫只在 **boot 时一次**，不是周期性的（`crates/daemon/src/orphans.rs:1-18` 文件头：
记录上一代 daemon 的 pid+启动时间，下次启动杀掉它残留的子进程；pid 复用有专门的判据）。
会话侧**没有对应的清扫**：`chats` 表没有 status 列（`0013_chats.sql` / `0015_chats_cwd.sql`），
重启后一轮被打断的对话不留标记，`active/generating` 回到 `false`（`chat.rs:1896-1897`）。
另有空闲回收：`IDLE_TIMEOUT = 60 * 60`（`chat.rs:592`，`spawn_idle_reaper` 在 `:1976`）。

**判读**

- 「同时能跑几个 run」：**默认 per runtime kind 2 个，最多排 8 个**，超了就是 400。
  对单用户够用；对「主 agent 一次分发给 5 个角色」**不够**（除非它们跨 kind，或改 `policy.toml`）。
- 「有没有队列」：有**闸门**，没有**持久队列**。这个区别对目标很关键：持久队列能在重启后继续，
  闸门不能。
- 「跑着的 run 能不能取消」：能，且做得干净（abort + 复核子进程，`runs.rs:1699-1707` 的注释
  就是为了这一条）。但**只有 HTTP 面有**，agent 侧没有工具。
- 「重启后未完成的 run 会怎样」：**变 `Interrupted`，然后没人管**。有 `retry` 端点可以人工恢复，
  没有自动再入队、没有自动重试。这是一条会直接打到「无人值守」的能力边界。

### 2.6 「主 agent 主动找活干」有没有地基 —— 有四套定时器，但**没有一套能驱动 agent**

**读数**

逐个查，四个机制，各自驱动什么：

| 机制 | 位置 | 周期 | 驱动什么 |
| --- | --- | --- | --- |
| `watch`（脚本） | `scripts/ruagent-daemon.ps1:262-284` | 每次被调用一次 | `GET /api/v1/health`，不 ok 就 `start` |
| `install-task`（计划任务） | `scripts/ruagent-daemon.ps1:285+` | 登录时 + **每 5 分钟**（`:356-357`，`:594` 回读校验 `PT5M`） | 用 `conhost.exe --headless` 跑上面的 `watch` |
| `audit.yml`（CI） | `.github/workflows/audit.yml:77-78` | **周一 03:00 UTC** | 起一台一次性 daemon + Chromium，跑 `design-audit.mjs --check` 判面板 DOM |
| MCP health（daemon 内） | `crates/daemon/src/mcphealth.rs:13`、`:114-123` | 每 **5 分钟** | ping 每个注册的 MCP server，写内存状态供注入筛选 |

`watch` 的本体就是一次健康检查（`scripts/ruagent-daemon.ps1:272-283`）：

```powershell
    if (Test-Health) {
      ...
      Add-Content -Path (Join-Path $logDir 'watchdog.log') -Value "$stamp ok pid=$ours"
      Write-Output 'ok (no action)'
    } else {
      ...
      & $PSCommandPath start -Root $Root -Exe $Exe -Addr $Addr
    }
```

`Test-Health` 一行（`:110-112`）：`Invoke-RestMethod -Uri $health`，`$health` = `…/api/v1/health`（`:88`）。

daemon 内还有两条周期循环，也都不是 agent：
知识库扫描 60s（`lib.rs:680-696`，`kb_scanner.scan()` + `knowledge_graph::sweep_if_enabled`），
运行时候选目录刷新（`lib.rs:723-729`，`chats.refresh_all_options`）。

**关键确认：这四套全都是「脚本/定时器调 HTTP 或 ping 进程」，没有一套是「一个 agent session 被定时
消息驱动」。** 具体证据两条：

1. 由 daemon **自己主动发起**（不是被某个 HTTP 请求触发）的 agent turn，在整个 workspace 里
   **只有一处**：蒸馏器
   （`crates/daemon/src/distill.rs:1441-1541`，`ask_agent_with_usage`：自己
   `start_chat`、自己 `session.send(ChatCommand::Prompt{…})`、权限自动 Cancel、5 分钟超时）。
   但它的触发点**不是定时器**，是 `ChatManager::close` 里的 `self.maybe_auto_distill(id);`
   （`chat.rs:1350-1364`）。也就是说：**蒸馏是「会话关闭时的一次副产物」，不是「定时主动找活」。**
2. `Chat::send_prompt` 的唯一外部调用者是 `chat_message`（2.2 已给 grep 结果）。daemon 里
   **不存在**任何「往某个 chat 投一条消息」的内部 API，因此没有任何定时器**能够**驱动一个会话
   ——不是没写，是缺那个入口。

**判读**

- 定时器的地基**有**，而且不止一套（计划任务 5 分钟、daemon 内 60s/5 分钟循环）。
  但它们驱动的是「进程活着」和「数据整理」，**没有一条通向 agent**。
- 蒸馏器给了一个**可以直接抄的技术先例**：在 daemon 内部起一个一次性 ACP 会话、发一轮、收结果、
  超时与权限都有既定做法。这是「主动找活」最省事的实现路径（见 §4 第 2 步）。
- 因此「要不要造新东西」的答案很明确：**要，但只造一个小东西**——一个「按周期/按事件发起一轮」
  的调度点，加上（更重要的）一个「让会话能被内部唤醒」的入口。后者才是硬骨头。

---

## 3 能力表

| 能力 | 有/部分/没有 | 证据（HEAD d30931b） | 缺的那一块具体是什么 |
| --- | --- | --- | --- |
| 后台 run | **有** | `runs.rs:776` `tokio::spawn` + `:926` `Ok(returned)`；`chat.rs:261` 只入队；`e2e_daemon.rs:1624/1632` 断言返回时仍 `spawning`/`queued` | 无（这一层是完整的） |
| 结果回流 | **部分** | 终态广播 `runs.rs:1799-1802`；轮询 `api.rs:3939`；SSE 重放 `api.rs:3977-3989`；面板轮询 `Board.tsx:370`/`TaskDetail.tsx:116` | **没有推送、没有未读、没有 run 列表端点**（路由表只有 `GET /runs/{id}`，`api.rs:167`）。结果只在「你回去看」的时候存在 |
| 跨会话持久 | **部分** | `runs` 行 + `0003_m2_results.sql` 的 `result`；`transcripts/*.jsonl`（`runs.rs:1433`）；`chats` 行（`0013`+`0015`） | run 侧够用；**chat 侧没有 status 列**，重启后中断的一轮不留标记（`chat.rs:1896-1897` 的 `active/generating` 只来自内存） |
| 任务派发 | **有**（派发）／**没有**（认领） | `mcp/src/lib.rs:670-703` → `POST /tasks/{id}/runs` → `runs.rs:672`；认领：`0001_init.sql` 头注释 `claim/lease columns arrive with the v2 task board`，且全仓无 claim 字段/无 pending 扫描 | **没有「谁来做」的协商**：daemon 路由指派，agent 不能抢单、不能拒单。`list_tasks` 能看到看板，但**没有** `run_list`/`task_runs` 工具，agent 看不到自己派出去的那一堆 run 各自怎样 |
| 并发控制 | **有**（run）／**没有**（chat） | `policy/src/lib.rs:56-63`（2 / 8）；闸门 `runs.rs:713-744`；满队拒绝 `runs.rs:186-193` | 闸门是**内存信号量、不是持久队列**：重启即丢。**chat 完全不受闸门约束**，开 10 个会话就是 10 个子进程 |
| 取消 | **部分** | run：`api.rs:169/706-732` → `runs.rs:370-377` → `1690-1727`（abort + 复核子进程）；chat：`api.rs:191/7094-7108` | **MCP 没有 cancel 工具**：agent 能派活不能收活。没有「按 task 取消全部 run」的聚合取消 |
| 重启恢复 | **部分** | `lib.rs:515-534` 非终态 → `Interrupted` + `error="daemon restarted mid-run"`；`orphans.rs:1-18` 杀残留子进程；手动 `retry` `api.rs:170`/`runs.rs:934` | **没有自动再入队/自动重试**；排队中的 run 直接变 `Interrupted`；chat 侧无清扫无标记；孤儿扫描只在 boot 一次（非周期） |
| 主动驱动（watchdog） | **部分** | `ruagent-daemon.ps1:262-284`（health 检查）、`:356-357`/`:594`（5 分钟计划任务）、`audit.yml:77-78`（周一 CI）、`mcphealth.rs:13/114-123`（5 分钟 ping）、`lib.rs:680-696`（60s 扫描） | **没有一条通向 agent**：四套机制全是「调 HTTP / ping 进程 / 扫数据」。缺 (a) 一个按周期发起一轮的调度点，(b) 一个让会话能被内部唤醒的入口（`send_prompt` 今天只有 `chat_message` 一个调用者） |
| **（附加）失败可见性** | **没有** | `update_task_status` 全部 4 个调用点：`api.rs:770`、`runs.rs:884`（→InProgress）、`:1797`（Completed→Done）、`:1941`（pipeline 下游→Cancelled） | **run 失败时 task 永远停在 `in_progress`**，看板显示「进行中」，没人知道它其实死了 |

---

## 4 离目标最近的三步（候选与代价，不是方案）

排序依据：**改动面 ÷ 解锁能力**。第 1 步几乎肯定要做；第 3 步是目标的真正终点，但代价最大，
需要你先决定要不要。

### 第 1 步：让「完成的 run」可见（回执的读数层）

**解锁**：用户不再需要「回去逐个窗口看」。这是「一个入口」的最低前提——**一个地方能看到全世界
刚刚发生了什么**。

**要动的地方**（量都很小）：
- `crates/daemon/src/api.rs`：新增 `GET /api/v1/runs`（带 `status` / `since` / `limit` 的过滤，
  复用文件里既有的 `page()` 信封，见 `list_tasks` 的写法 `api.rs:3250-3277`）。~60 行。
- `crates/store/src/lib.rs`：如果既有的 `list_runs_by_status` 不够用，补一个带 `updated_at` 过滤的
  reader。~30 行。
- `panel/src/views/Home.tsx`（或 `Board.tsx`）：一个「最近完成/失败」列表或计数。
  注意 `Home.tsx:247-302` 的六端点合并模式是现成的，照抄即可。~80-120 行。

**代价**：小。**不改存储、不动 chat、不动 run 推进。** 估算 150-250 行，含一条路由测试。
**它能单独成立**，不依赖第 2、3 步。

### 第 2 步：把 watchdog 从「存活」升级为「活干」（主动驱动的最小形态）

**解锁**：平台能在没人打字的时候发起工作。这是「主 agent 自己找活」的第一块砖。

**要动的地方**：
- `crates/policy/src/lib.rs`：`policy.toml` 加一个 `[watchdog]`（`enabled` / `interval` /
  `agent` / `prompt` 或 `task`）。~40 行含解析与默认关闭。
- `crates/daemon/src/lib.rs`：抄 `:680-696` 那条 60s 循环的格局，加一条 tick；到点调用
  `start_run`（**不是**去 POST 自己的 HTTP —— 那样等于把 daemon 当外部客户端，还会绕开闸门）。
  ~60 行。
- 幂等/去重：同一个 watchdog 上一轮没结束就跳过本轮（用 `list_runs_by_status` 判断即可）。~20 行。
- 要不要落成 task：建议落（`create_task` + `start_run`），这样它自动出现在看板和回执里——
  **免费复用第 1 步**。

**代价**：中偏小，约 150-250 行。**不动 chat、不动面板、不加迁移。**
**风险点（要提前定）**：它会自己花 token。建议挂在已有的 capability plane 上
（`crates/daemon/src/capability.rs` 的 `CapabilityId`，默认关闭），而不是只靠 `policy.toml` 一行。

### 第 3 步：让会话能被唤醒（「主 agent 监督」的真正闭环）

**解锁**：派出去的活干完了，**主 agent 自己知道**，并把结论带回对话。没有这一步，
「分发与监督」永远停在「分发 + 你自己去查」。

**要动的地方**（这是三步里唯一有结构性代价的）：
- **归属关系**：今天 `runs` 有 `task_id`，但**没有「这个 run 是哪个 chat 派的」**。
  要么给 `runs` 加一列 `origin_chat_id`（一条 migration + `RunParams` 一侧），
  要么新开一张 `run_origin(run_id, chat_id, dispatched_at)`。~1 条迁移 + 40 行。
- **唤醒入口**：把 `Chat::send_prompt` 从「handler 专用」变成「内部可调用」，并加并发约束
  （同一 chat 只能有一轮在飞——`generating` 已存在，`chat.rs:138`）。
  最省的做法是在 `ChatManager` 上加一个 `notify(id, text) -> Result<()>`，内部就是
  `send_prompt` + `generating` 检查。~50 行。
- **完成钩子**：在 `supervise` 收尾处（`runs.rs:1791-1802`，正好在 `update_run` 之后、
  `Finished` 广播之前）判断有没有 `origin_chat_id`，有就 spawn 一次 `notify`。~30 行。
- **风暴抑制**：fanout 一次 5 个 run 会唤醒 5 次。要么按 task 聚合（最后一个终态时唤醒一次），
  要么加最短间隔。~40 行。
- **文案边界**：唤醒消息必须是「平台事件」，不能被当成用户说的话——
  蒸馏那里已经有 `platform_prompt()` 这个先例（`distill.rs:1515`），照抄。

**代价**：大，约 350-600 行 + 1 条迁移 + 一轮「一个 chat 同时只允许一轮」的回归测试。
**它必须先定一个产品决策**：唤醒是「静默追加一条记录」还是「真的让主 agent 回一句」？
前者几乎无风险但价值有限（等于第 1 步加个筛选）；后者才是用户想要的，但它意味着
**平台会在用户不在的时候花 token 说话**。

**排序理由（为什么不是反过来）**：第 3 步的技术底座（`send_prompt`）薄得意外，但它的
**产品歧义最大**、且会引入第一写入（迁移）+ 一个新的花费路径；第 1、2 步都能单独交付、
都能被独立验证、都不改数据形状。所以按「最小改动 × 最大解锁」是 1 → 2 → 3。
如果用户的真实痛点是「我不想切窗口」，**第 3 步才是解药，第 1 步只是止痛片** ——
这一点需要你和用户对齐，不是我能替他决定的。

---

## 5 我没能确认的（照实写）

1. **没有在活实例上验证任何一条。** 按纪律没有碰 `8787`（任务书告知 pid 49800），也没有起私有守护进程
   （起它要 `cargo run` 或既有二进制，且首次启动会碰模型下载；proxy 环境已知有问题）。
   所以本文所有读数都是**源码字节 + 既有测试断言**，不是运行时观测。若要与运行时对账，
   需要在私有 root + 私有端口上重做一遍 2.1/2.2/2.5。
2. **`runs.updated_at` 的写入时机我只查了状态迁移处。** 我看到 `update_run` 在 `Spawning`/
   `Running`/终态处被调用（`runs.rs:879/1611/1795`），**没有**逐一确认事件流是否也刷新它。
   如果只在状态迁移时写，那第 1 步的 `since=` 语义要重新设计（用 `created_at` 还是新列）。
   这条我标注为「未查完」，不是「不存在」。
3. **面板是否还有别的「完成提示」散点，我没有逐页读完。** 我的 grep 覆盖了
   `EventSource|usePoll|setInterval|Notification|unread|badge`，没有覆盖 `document.title`、
   antd `message`/`notification` 的全部调用点，也没有读完 `Chat.tsx`（1500+ 行）。
   结论「没有推送」在 daemon 侧是硬的（缺端点、缺唤醒入口），面板侧是「grep 范围内没有」。
4. **`task` 状态在 run 失败后的最终形态**：我从「`update_task_status` 只有 4 个调用点」
   推出「失败时停在 `in_progress`」，**没有**找到一条测试直接断言它。这是推断，
   虽然推断链很短（写入点穷举）。
5. **重启后 chat 的「无标记」是从 DDL 推的**：`chats` 表没有 status 列（`0013`+`0015`），
   加上 `active/generating` 只读内存注册表（`chat.rs:1896-1897`）。没有测试断言
   「重启后不标 interrupted」——因为按现状也没什么可标的。
6. **`audit.yml` 我只读了头部注释与 `on:` 块**，没有确认那台一次性 daemon 有没有被复用到
   别的用途（比如它是否顺带跑了别的检查）。
7. **`#board` 的列与 run 状态的同步**：我只确认了「task 状态独立于 run 状态」这一半
   （`TaskDetail.tsx:144-147` 的注释自己就说「a task can be 「已完成」 while this is non-zero」），
   没有查 fanout 多 run 时 `selected_run_id` 与 task 状态的全部交互。
