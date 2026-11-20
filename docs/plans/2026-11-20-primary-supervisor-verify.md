# 单一入口 + 主智能体监督：独立验证报告

日期：2026-11-20 · 验证者：`verifier`（与三名实现者无共享上下文）
剧本：`docs/plans/2026-11-20-primary-supervisor-acceptance.md` · 规格：`docs/plans/2026-11-20-primary-supervisor-design.md`
基线：`d30931b`（`d30931be34c0bafb368a9c0ac954d3362aaac584`）+ 工作树未提交改动

## 0 这份报告的读数属于哪一份字节

所有读数都在**同一份工作树字节**上取得，取读数前后各查一次 `git status --porcelain` / `git diff --stat`，
两次完全相同（16 个文件 `+2888 / −100`，另有 8 个未跟踪文件）。会话开始与结束时都一致，所以本报告不需要分版本引用：

| 文件 | blob id（`git hash-object`） | mtime |
| --- | --- | --- |
| `crates/daemon/src/api.rs` | `e453e08cb56fb3ad8765ed59863cd66d3b22a15c` | 11-20 22:52:40 |
| `crates/daemon/src/runs.rs` | `8d485d81e2867d7265de0f984f7341b3cb58dd39` | 11-20 22:36:23 |
| `crates/daemon/src/chat.rs` | `ffeeb447be180bca08c480a49fa757c1d0253ae7` | 11-20 21:42:43 |
| `crates/daemon/src/lib.rs` | `b68d4fa240f895c99edf897fc6be070184904b30` | 11-20 22:36:30 |
| `crates/store/src/migrations.rs` | `c77e8a29c8e7042a31253e93b18f811b5ca302e8` | 11-20 21:42:43 |
| `crates/daemon/tests/supervise_wakeup.rs` | `b204928cfdb1c92b316ab425bdb3b5a8f6b52f3e` | 11-20 22:31 |

**纪律声明**：只读产品代码；唯一写入是这份报告。没有提交、没有 push、没有打 tag。
没有碰 `8787` 上的常驻实例（`pid 49800`，`D:\rust_cache\debug\ruagent.exe --root C:\Users\19410\.ruagent`）：
本报告里每一次 HTTP 都打在**我自己起的私有 daemon**（私有 root + `127.0.0.1:8791` / `8793`），收尾按 PID 停掉、端口已确认无人监听。

私有 fixture（我自己写的，不是实现者的）：
`%TEMP%\psverify-v1\root`（沿用常驻实例之外的私有 root）、四个 mock agent —
`boom-a` / `boom-b`（`--behavior crash`，必然失败）、`talker`（`echo`）、`slowpoke`（`slowreply`，10s）；
后期为了走到面板的 dash 分支又加了一个 role 型 `helper`。二进制是把本树 `cargo build -p ruagent --bin ruagent`
的产物**复制**到私有目录再起，避免锁住共享 target。

---

## 1 总判定

用户要的形态是「我只需要和一个主 agent 对话，它自己分发/监督/汇报」。按这条量：

| # | 断言 | 判定 | 我的读数 |
| --- | --- | --- | --- |
| A1 | 迁移可重放、数字跟上 | **达标** | `0032_runs_origin_chat.sql:19`；`migrations.rs:40` 进数组；我按名字跑 `cargo test -p ruagent-store --lib origin` → `the_origin_chat_column_upgrades_an_old_database_and_reapplies_cleanly ... ok`（1 passed）；私有 root 首启日志 `applied schema migration version=32` |
| A2 | 终态映射穷举 | **达标** | `runs.rs:1929` `task_status_for` 是全 8 变体的 `match`（新变体不编译）；`WaitingPermission → None`；活实例 4 次失败/中断后 task 都是 `pending` |
| A3 | 聚合判据只有一处 | **达标（附一条注）** | 产品代码里「簇内还有非终态」的扫描**只有 1 处**：`runs.rs:1990`。注：`watchdog.rs:363` 的 `inflight()` 是**同一个问题的第二个读者**（问「这个 task 还有活着的 run 吗」），它不做唤醒判定，但两份代码问同一个问题 |
| A4 | `notify` 走 `send_prompt` 且不绕 `generating` | **达标** | `chat.rs:420` → `chat.rs:449` `deliver_or_queue`：`is_generating()` 为真就**排队**，否则 `send_prompt`；平台标记 `chat.rs:440` 复用 `USER_TEXT_SENTINEL`。活实例：chat 里出现平台事件后**真的跑了一轮**（`agent_message_chunk` + `stopped`），且 `user_text` 为空 |
| A5 | watchdog 默认关闭且挂 capability | **达标（行为未测）** | `policy/src/lib.rs:253-257` `#[serde(default)] enabled: bool`；`capability.rs:457` 行 `tier = Llm`、`default_enabled = false`；活实例 `GET /api/v1/capabilities` 里 `watchdog` 行 `enabled:false, configured:"legacy", new:true`，两个 root 的日志里 `watchdog` 一行都没有（循环没起）。**真开一轮没有验** |
| B1 | 失败→自愈一次→仍失败→chat 多一条平台事件 | **达标** | 私有实例上全链跑通：2 个 run（第 2 个 `self_heal_of` 指向第 1 个）、chat 里**恰好 1 条** `user_message`，且它的 `user_text` 为空、正文说出两次失败原因 |
| B1 反例 | 不带 `origin_chat_id` → chat 一条都不多 | **达标** | 同一流程去掉 origin：chat 里 `user_message` = **0**，run 只有 1 个（没有自愈），task 仍被回写成 `pending` |
| B2 | 中间终态不打扰，全部终态恰好一条 | **达标** | 快的那条 t+1.0s 失败时 chat = 0 条（慢的那条实测仍 `running`）；全部终态后 chat = **恰好 1 条**（簇里 3 个 run：完成 1、失败 1、自愈失败 1） |
| B3 | run 失败后 task 不是 `in_progress` | **达标** | `GET /tasks/{id}` → `task.status = pending`，两条 run 各自带 `error` |
| C1 | `GET /api/v1/runs` 存在、过滤正确、未知 status → 400 | **达标** | 200 + 信封 `total/limit/offset/truncated/runs`；`?status=failed` 只回 failed、`?status=completed` 只回 completed；`?status=complete` → **400** `unknown status ...`；`?since=not-a-time` → 400 |
| C2 | `agent` 是名字不是 id | **达标** | 每行 `agent: "boom-a" / "boom-b" / "slowpoke"`，`agent_id` 另给 registry id |
| C3 | 空结果 = 空数组 + 正常信封；面板说「还没有」 | **达标** | `?status=cancelled` → `runs: []`、`total: 0`、信封完整；空 root 的浏览器读数里两块写的是「还没有完成过的运行。」「还没有失败过的运行。」 |
| C4 | 有数据时数字对得上 | **达标（附一条注）** | 有数据 root：面板「最近完成」1 行、「最近失败」5 行，与 `GET /runs?status=completed&limit=5`(total 1)、`?status=failed&limit=5`(total 6, 回 5) 逐行同行序一致。注：**首次运行分支（FirstRun）里这两个块根本不在页面上**（见 §2 C4） |
| D | 四条门禁 | **达标** | `fmt` exit 0；`clippy --workspace --all-targets -DenyWarnings` 强制重读 exit 0（12 条 `Checking`，0 warning/0 error）；`test --workspace` exit 0，**ok 行 87 / FAILED 行 0 / passed 合计 952**（ignored 22）；`panel npm run build` exit 0 |

**对「一个入口」的总体判读：**

1. **P0（看板不骗人）：成立。** 四条活实例读数（B1 的两次失败、B1 反例的单次失败、R1 的重启中断、R2 的重启中断）都落在 `pending`，没有一条留在 `in_progress`。
2. **P1（回执读数层）：成立。** 路由、过滤、400、名字、空态五项都在活实例上验过；面板两块与 API 逐行对得上。
3. **P2（唤醒闭环）：主体成立，但有一条会伤到人的缺口。** 「谁派的活谁被叫醒、一次、说得出原因」在一条私有链路上是真的（B1/B2/B3）。
   **缺口在重启路径**：daemon 重启会把被中断的 chat 簇**再起一轮 run（花 token）**，而那一轮的汇报会因为「chat 没有活会话」被**丢弃**（只留一条 WARN）。也就是说：重启既花了钱，又没能把话说出口。
4. **P3（开始自己找活）：结构达标，行为未测。** 默认关闭是实测的（两个 root 都没有 watchdog 日志行、capability 行 `enabled:false`）；「真的到点起一轮」我一次都没在活实例上看到。

所以「用户只跟一个主 agent 说话」这条目标：**分发与汇报的地基已经在了，监督还差最后一环——平台在用户不在时说的话，只有在那个会话仍然活着的时候才到得了。**

---

## 2 A / B / C / D 逐条

### A1 迁移可重放 + 数字跟上 —— 达标

**断言**：`runs` 有 `origin_chat_id`，迁移可重放，且「数字」（头版本 + 重放测试）跟着走。

**我怎么验的**
1. 读迁移脚本与数组：`crates/store/src/migrations/0032_runs_origin_chat.sql:19` = `ALTER TABLE runs ADD COLUMN origin_chat_id TEXT;`，`migrations.rs:40` 把它 `include_str!` 进 `MIGRATIONS`。
2. 按名字跑那一条**重放**测试（不是看别人的读数）：
   `scripts/cargo-team.ps1 test -p ruagent-store --lib origin`
3. 让一个**全新私有 root** 从零建库，读 daemon 自己的迁移日志。
4. 读那条测试的实现（`migrations.rs` 新加的 `the_origin_chat_column_upgrades_an_old_database_and_reapplies_cleanly`）：它用 `apply_one` 逐条建到 31、写一行「前 0032 时代的 run」、再 `apply()` 升到 32，断言列可空无默认、历史行读 `NULL`、行数不变；然后删掉 `schema_migrations` 里 ≥32 的行**重放**，断言 schema 快照完全一致；最后用**同一句 ALTER 不加守卫**做负控，断言它确实报错。

**读数**
- `test migrations::tests::the_origin_chat_column_upgrades_an_old_database_and_reapplies_cleanly ... ok` / `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 50 filtered out`
- 私有 root 首启：`INFO ruagent_store::migrations: applied schema migration version=32`（1..=32 全打了一遍）
- `ruagent-store --lib` 整包（同一次 `test --workspace` 里）：`ok. 48 passed; 0 failed; 3 ignored`

**判读**
数字确实跟上了：头版本 31 → **32**，且旧的那条（覆盖 0031 的）把「头字面量」让给了新测试，改成断言「0032 排在 0031 之后」——这是「头字面量跟着最新一步走」的正确写法，不是把数字钉在原地。

**一处剧本偏差（记下来，不算实现问题）**：剧本写「原来是 25/25，加了就是 26/26」。本树的实际头在加这条迁移前是 **31**，加完是 **32**；25/26 是设计文档写作时的旧数字。我按树的实际数字验的，A1 的实质要求（可重放 + 头字面量跟着走）成立。

### A2 终态映射穷举 —— 达标

**断言**：`Failed` / `Cancelled` / `Interrupted` 都回写 task；`WaitingPermission` 被当成非终态；不是 `if completed`。

**我怎么验的**：读 `runs.rs:1929` 起的 `task_status_for`，以及唯一调用点 `runs.rs:1969` `finish_cluster`。

**读数**（结构）
- `task_status_for` 是 `match status` 覆盖全部 8 个 `RunStatus`：
  `Queued | Spawning | Running | WaitingPermission => None`；
  `Completed => Some(TaskStatus::Done)`；
  `Failed | Interrupted | Cancelled => Some(TaskStatus::Pending)`。
- `finish_cluster` 用 `filter_map(task_status_for)` 收 `targets`，再比 `targets.len() != runs.len()`，不等就 WARN 然后**不动** task —— 也就是「映射不全」这件事不会静默变成某个默认值。
- `WaitingPermission` 被判非终态：映射返回 `None`，`is_terminal()` 在 core 里也是 false。脚本要的这条成立。
- 另外 `supervise` 在收尾前给「非 Completed 且 error 为空」的终态补一句原因（`runs.rs:1892`），所以「说得出为什么没成」有载体。

**读数**（活实例，4 次独立观测）

| 场景 | run 终态 | `GET /tasks/{id}` → task.status |
| --- | --- | --- |
| B1：失败 + 自愈失败 | failed, failed | **pending** |
| B1 反例：单次失败、无 origin | failed | **pending** |
| R1：被重启打断（chat 派发） | interrupted, failed(自愈) | **pending** |
| R2：被重启打断（面板派发） | interrupted | **pending** |

**判读**：穷举是真的穷举（编译器兜底），不是把 `if completed` 换成 `if !completed`；四条活读数里没有一条留在 `in_progress`。

### A3 聚合判据只有一处 —— 达标，附一条注

**断言**：「同簇还有非终态」这个判断全仓只应出现一次（两处 = 两个真相）。

**我怎么验的**：grep 产品代码（排除 `*/tests/*`）里所有「非终态扫描」的写法，再逐个看命中点的用途。

**读数**
```
crates\daemon\src\runs.rs:1990:  if let Some(live) = runs.iter().find(|r| !r.status.is_terminal()) {
```
只有这一处。它就在 `finish_cluster` 里，同时决定两件事（P0 回写要不要做、P2 要不要唤醒），并且**在它之后**才有「有失败 → 先自愈一次」的分支。风暴抑制靠它，没有计时器：
`finish_cluster` 里没有任何 `tokio::time` / 最小间隔，这一点我专门查过。

**注（我认为该记下来）**：`crates/daemon/src/watchdog.rs:363` 的 `inflight(db, task_id)` 问的是**同一个问题**的字面形式——「这个 task 还有活着的 run 吗」（用 `list_runs_by_status` 走 4 个非终态 + `r.task_id == task_id` 过滤）。它不参与唤醒判定（只用来跳过本轮 tick），所以**唤醒判据仍然只有一处**；但「簇还活着吗」这个事实的读法现在有两份（`runs.rs:1990` 与 `watchdog.rs:363`），且第二个是 P3 那份改动引入的。把 `inflight` 改成 `finish_cluster` 内的一个纯读函数（或反过来）是后续该做的小合并；它现在没造成行为分叉（两个都用了 `is_terminal`/`ACTIVE`，`watchdog` 里还有一条 `debug_assert` 钉住 ACTIVE 是 `is_active` 的补集）。

另外两处疑似但不成立：`api.rs:4070` 的 `finished:` 用的是 core 的 `is_terminal()`（同一个谓词，不是第二份判据）；`lib.rs:701` 的重启回写**故意**调 `finish_cluster` 而不是自己判（每个 `task_id` 用 `BTreeSet` 去重后调一次）。

### A4 `notify` 走 `send_prompt`、不绕 `generating` —— 达标

**断言**：`ChatManager::notify` 内部复用 `send_prompt`，带 `generating` 检查，「正在飞」时**排队**不是丢弃，且唤醒消息带平台事件标记。

**我怎么验的**：读 `chat.rs` 新增的 `notify` / `deliver_or_queue` / `platform_event`，再在活实例上看一次真实的唤醒长什么样。

**读数**（结构）
- `chat.rs:420 pub async fn notify(&self, chat_id: RunId, text: String) -> Result<()>`：chat 不在 live 表里时**区分「历史上存在但没活会话」与「已被删除」**，都只 WARN 不重试、返回 `Ok(())`（不把失败抛给 supervise）。
- `chat.rs:449 async fn deliver_or_queue(...)`：**唯一**决定「现在发 / 先排队」的地方——
  `if chat.is_generating() { queue.push_back(text); WARN(队列在内存里，重启会丢) } else { chat.send_prompt(db, embedder, platform_event(&text)) }`。
  它走的就是 `send_prompt`（`chat.rs:168`），而 `send_prompt` 自己会 `generating.store(true)`（`chat.rs` 内 `// Set BEFORE the prompt is queued` 那段）。所以**没有绕过**。
- 排队的补发口在「一轮结束」的那一处：监听到 `RunEvent::Stopped` / `RunEvent::Error` 时清 `generating` 并 `drain_one_notification`（`chat.rs` 里那个 forwarder 任务），没有独立计时器。
- 平台标记：`chat.rs:440 fn platform_event(text) -> format!("{text}{USER_TEXT_SENTINEL}")`，`USER_TEXT_SENTINEL` 定义在 `chat.rs:113`，读取方是 `sessions.rs:935 fn user_text`（切在最后一个标记之后）。这与 `distill.rs` 的 `platform_prompt()` 是同一个标记，不是新造的第二套约定。

**读数**（活实例，B1 的 chat 转录原文，`<root>/data/transcripts/run-<chat-id>.jsonl`）
```
seq=0 type=state_changed status=running
seq=1 type=user_message            ← 平台事件（user_text 切完为空）
seq=2 type=agent_message_chunk text=echo: 【平台事件】你派出去的活没能做成…
seq=3 type=usage_update
seq=4 type=stopped
```
- `user_text(raw)` = **空串**（标记之后没有用户文字）→ 不是用户消息。
- 但它**真的驱动了一轮**：`agent_message_chunk` + `stopped` 都在，也就是说这条汇报不是「静默追加一条记录」，而是「真的回了一句」（echo agent 的回话就是它收到的那段文本）。

**判读**：A4 的三条都成立。补一条实测到的失败模式（P2.4 要求的）：重启后 chat 没有活会话时，`notify` 走的是「记一条、吞掉、不重试」那条分支——我在 R1 里亲眼看到那条 WARN（见 §3）。

### A5 watchdog 默认关闭且挂 capability —— 达标（行为未测）

**断言**：`[watchdog]` 默认 false；capability plane 上有一行；不是只靠 policy 一行。

**我怎么验的**：读 policy 默认值、capability 注册表与闸门函数；再在全新私有 root（没有 `[watchdog]`）上看 `GET /api/v1/capabilities` 与 daemon 日志。

**读数**
- `crates/policy/src/lib.rs:253-257`：`pub struct WatchdogConfig { #[serde(default)] pub enabled: bool, ... }`——缺省 false；`round()`（`:326`）在 `!enabled` 时直接 `Ok(None)`，**连校验都不做**。
- `crates/daemon/src/capability.rs:457`：注册行 `id: CapabilityId::Watchdog, tier: Tier::Llm, default_enabled: false, new_in_this_increment: true`；`capability.rs:134` 的字符串 id 是 `"watchdog"`；`capability.rs` 的 `specs().len() == CapabilityId::ALL.len()` 断言同步到 12。
- 闸门本体：`watchdog.rs:274 gate_refusal(plane)` → `plane.gate(CapabilityId::Watchdog, true)`；被挡时返回**一句带补救写法的话**，不是 bool。
- `config.rs:918` 的 `DEFAULT_POLICY_TOML` 里 `[watchdog]` 与 `[capabilities.watchdog]` 都是**注释形态**。
- 活实例（空 root，`[watchdog]` 不存在）：`GET /api/v1/capabilities` → `table_present=false, rows=12`，`watchdog` 行原文：
  `{"id":"watchdog","tier":"llm","default_enabled":false,"enabled":false,"configured":"legacy","new":true,…}`
- 两个私有 root 的 daemon 日志里 grep `watchdog` = **0 行**（没有循环日志、没有「blocked」日志）→ 默认配置下这个特性根本没起。

**判读**：「默认关闭」在结构上和活实例上都是真的；「挂 capability」在注册表与闸门函数上是真的。**「真到点起一轮」与「capability 关着时它确实不发」只有 crate 自己的单测**（`watchdog::tests::*` 与 `capability_defaults`），在我这次 `test --workspace` 里全部通过（`ruagent-daemon --lib` 290 passed、`capability_defaults` 5 passed），但那**不是活实例读数**——见 §5。

### B1 失败 → 自愈 → 仍失败 → chat 里真的多一条平台事件 —— 达标

**我怎么验的**（私有 daemon，`127.0.0.1:8791`，私有 root）
1. `POST /api/v1/chat {"agent":"talker"}` → chat id `01a1f986-52a0-716f-…`
2. `POST /api/v1/tasks` → task `01a1f986-52d3-…`
3. `POST /api/v1/tasks/{id}/runs {"agent":"boom-a","prompt":"fail please","origin_chat_id":"<chat>"}` → run `01a1f986-52d8-…`（响应里 `origin_chat_id` 已带上）
4. 轮询 `GET /api/v1/runs/{id}` 到终态；再等簇里出现第 2 个 run（自愈）
5. 读 chat 转录文件（`chat_events` 重放的就是这个文件，daemon 没有 `GET /chat/{id}/messages`）

**读数**
- run 1：`status=failed`，`error` = `ACP protocol error: Incoming transport closed: {"reason": "incoming_transport_closed", "method": "session/prompt"}`，`self_heal_of=null`
- run 2：`status=failed`，`self_heal_of="01a1f986-52d8-…"`（= run 1），`origin_chat_id` 同 chat，agent 换成了另一个 registry id
- run 2 自己的转录 `seq=0`：
  `type=routed rationale=self-heal (the cluster's one automatic retry) of failed run 01a1f986-52d8-718f-a456-2b9590d26309, on a different agent`
  → 脚本要的「有一条 run 事件记录了自愈尝试」在这里（用的是既有 `Routed` 事件，没有新事件类型）
- chat 里 `user_message` 总数 = **1**，且 `user_text` = **空**；正文（原文照抄）：
```
【平台事件】你派出去的活没能做成（这是平台自动汇报，不是用户说的话）。

任务：B1 verify: fail then heal then report
  · boom-a — Failed：ACP protocol error: Incoming transport closed: { "reason": "incoming_transport_closed", "method": "session/prompt" }
  · boom-b（自愈重试） — Failed：ACP protocol error: Incoming transport closed: { "reason": "incoming_transport_closed", "method": "session/prompt" }

我试了 2 次（含 1 次自愈重试），最后一次失败原因是：ACP protocol error: Incoming transport closed: { "reason": "incoming_transport_closed", "method": "session/prompt" }
```
- task：`status=pending`；两条 run 都带 `error`

**判读**：脚本的四条要求全部满足——① 有自愈留痕（`Routed` 事件 + 正文「含 1 次自愈重试」）；② chat 里是**平台事件**（`user_text` 为空，标记是仓库既有那个）；③ 说得出失败原因（逐条 agent + 最后原因）；④ task 不是 `in_progress`。而且第 5 条我额外看的：这一轮**真的被 agent 回答过**（`agent_message_chunk` + `stopped`），不是静默记录。

### B1 反例：不带 `origin_chat_id` —— 达标

**我怎么验的**：同一流程，body 里**不**放 `origin_chat_id`，另开一个 chat 观察。

**读数**
- run 的 `origin_chat_id` = `null`；`status=failed`
- task：`status=pending`，**runs 只有 1 个**（没有自愈）
- 该 chat：`user_message` = **0**，平台事件 = **0**
- 之前那个被派发的 chat（有 origin 的）仍然是 1 条，没有被污染

**判读**：「面板/脚本派的活不唤醒人」成立，而且顺手读出一条实现细节：**自愈只发生在有 `origin_chat_id` 的簇上**（`finish_cluster` 里 origin 的 `let Some(...) else { return }` 在自愈分支**之前**）。这与规格 §P2.3 第 1 步一致（「origin 为空 → 什么都不做」），实现比规格更严：P0 的 task 回写**仍然做了**（规格那句「什么都不做」如果照字面读会把回写也跳掉）。这一点我认为是好的，但值得写进设计文档，否则下一个读者会以为无 origin 的失败不回写。

### B2 中间结果不打扰、全部终态恰好一条 —— 达标

**我怎么验的**：同一个 task 上先派慢的（`slowpoke`，10s）再派快的（`boom-a`，必失败），两条都带同一个 `origin_chat_id`；在**快的终态那一刻**读 chat，并同时读慢的那条的状态。

**读数**
```
t+0.1s dispatched   slow=01a1f986-cfc9-…  fast=01a1f986-cfcd-…
t+1.0s fast terminal: status=failed
t+1.0s slow status=running            ← 簇里确实还有活的
t+1.0s chat3 messages=0 platform=0    ← 中间终态没有打扰
t+3.0s (2s later) slow status=running chat3 messages=0
…等全部终态（含自愈）…
runs in the cluster: 3
  completed self_heal_of=[]     origin=<chat3>
  failed    self_heal_of=[]     origin=<chat3>
  failed    self_heal_of=<fast> origin=<chat3>
task3.status=pending
chat3 user_message events: 1 | platform events: 1
```
那唯一一条的正文（原文照抄，节选）：
```
【平台事件】你派出去的活没能做成（这是平台自动汇报，不是用户说的话）。
任务：B2 verify: two attempts, one report
  · slowpoke — 成功：（没有输出）
  · boom-a — Failed：ACP protocol error: Incoming transport closed…
  · boom-b（自愈重试） — Failed：ACP protocol error: Incoming transport closed…
我试了 3 次（含 1 次自愈重试），最后一次失败原因是：ACP protocol error: Incoming transport closed…
```

**判读**：绿是**结构绿**，不是碰巧——见 §3 特别问题 1 的读数（自愈 run 继承了 origin，所以它自己的终态也走「同簇聚合」那条路，而不是走「无归属 → 不唤醒」那条路）。簇里 3 个 run 只产生 1 条汇报，说明「每 run 唤醒一次」没有被写出来，也没有计时器掺进来。

### B3 失败时看板不再骗人 —— 达标

**读数**：B1 的 task `status=pending`（不是 `in_progress`），两条 run 各自 `error` 非空；
B1 反例、R1、R2 同样都是 `pending`。
`GET /tasks/{id}` 的 `runs[]` 就是「上一次为什么没成」的读法（`error` 字段 + `updated_at`）。

**判读**：看板不再显示「进行中」。**一点观察**：R1（重启）里 task 曾经短时间回到 `in_progress`——因为自愈起的那一轮**真的又开工了**，那时候「进行中」是真话，不是骗人。等自愈也终态后回到 `pending`。

### C1 `GET /api/v1/runs` 存在、过滤正确、未知 status → 400 —— 达标

**读数**
- `keys: total,limit,offset,truncated,runs`；`total=6 limit=200 offset=0 truncated=False returned=6`
- `?status=failed` → `total=5`，行内 status 全部 `failed`；`?status=completed` → `total=1`，全部 `completed`
- `?status=complete`（typo）→ **HTTP 400**，body：`unknown status `complete` (queued|spawning|running|waiting_permission|completed|failed|cancelled|interrupted)`
- `?since=not-a-time` → **HTTP 400**：``since `not-a-time` is neither unix milliseconds nor an RFC3339 instant``
- `?since=<now 的 unix ms>` → `total=0`（过滤生效，不是「忽略参数」）

**判读**：typo 不会看起来像成功。实现里 `parse_run_status`（`api.rs` 内）明确**不用** store 的 `run_status_from_str`（那个把不认识的值读成 `Queued`），理由写在注释里，我读到的行为与之一致。

### C2 `agent` 是名字不是 id —— 达标

**读数**（同一份响应逐行）
```
task_title=B2 verify: two attempts, one report | agent=boom-b   | agent_id=01a1f985-eace-765a-… | status=failed
task_title=B2 verify: two attempts, one report | agent=slowpoke | agent_id=01a1f985-eace-765a-… | status=completed
task_title=B1 verify: fail then heal then report | agent=boom-b | agent_id=01a1f985-eace-… | status=failed
```
**判读**：`agent` 是可读名字，`agent_id` 另给（设计里说「名字 + 可链接的 id」两者都要），与 `task_dispatch` 的既有先例一致。

### C3 空态 = 空数组 + 正常信封 + 面板「还没有」 —— 达标

**读数（API）**：`?status=cancelled` → `total=0 returned=0 type=Object[] truncated=False keys=total,limit,offset,truncated,runs`

**读数（面板，真浏览器）**：另起一个**空 root**（`%TEMP%\psverify-v1\root-empty`，`127.0.0.1:8793`，`GET /runs → total=0`），用 `agent-browser` 打开 `#home`，页面正文原文：
```
最近完成
查看全部
还没有完成过的运行。
最近失败
查看全部
还没有失败过的运行。
```
**判读**：空态是**句子**，不是 `0`；而且渲染得出来（不是只有 i18n 词条里存在）。构建产物里这两个字符串也确认在 `panel/dist/assets/index-*.js` 里（`Home-*.js` 里没有，因为字典在 index chunk）。

### C4 有数据时面板数字对得上 `GET /api/v1/runs` —— 达标（附一条注）

**读数（同一时刻的两侧）**

| 面板（浏览器正文） | API |
| --- | --- |
| 最近完成：1 行 `B2 verify: two attempts, one report · slowpoke · 9 分钟前` | `?status=completed&limit=5` → `total=1`，1 行，`B2 verify… / slowpoke` |
| 最近失败：5 行（R1/boom-a、B2/boom-b、B2/boom-a、B1-neg/boom-a、B1/boom-b），每行都带失败原因片段 | `?status=failed&limit=5` → `total=6`，回 5 行，顺序与面板逐行相同 |

**判读**：两块的行、行序、agent 名字、错误摘要都与 API 同一份数据；有数据时不是「0」，没数据时是「还没有」。

**注（这条我认为该报给 lead）**：`Home.tsx` 有**两条落地分支**，`firstRun` 分支（`Home.tsx` 内 `const firstRun = settled && dash.failed.length === 0 && (dash.agents === 0 || (dash.sessions !== null && dash.sessions.length === 0))`）**不渲染这两个块**。我最初用私有 root（只有 4 个 runtime 型 agent、0 个 role、0 个 session）打开面板，看到的就是 `FirstRun` 那一屏：「欢迎回来 / 从这里开始」——`最近完成 / 最近失败` **一个字都没有**。为了让 dash 分支出现，我加了一个 role 型 agent 并喂了一个 session 才看到（`roles=1, sessions=1`）。
所以：「全新安装 / 没有 role、没有 session 的 root」上，用户看不到回执这两块。它不算说谎（那种状态下确实没东西可报），但它意味着 **P1 的可见性在首次运行形态下不存在**。是否要把两块搬进 `FirstRun` 分支，是产品决定，不是实现遗漏——但验证者应该让你知道它现在不在那儿。

### D 门禁（每一条都带读数）

| # | 命令 | 读数 |
| --- | --- | --- |
| D1 | `cargo fmt --all --check` | **exit 0**，无任何输出（0 个 diff site） |
| D2 | `scripts/cargo-team.ps1 clippy --workspace --all-targets -DenyWarnings` | 第一次调用 **exit 0 但 elapsed=1.3s、没有任何 `Checking` 行 = 缓存绿，不算读数**；随后 `-CleanFirst ruagent-core -CleanFirst ruagent-policy -CleanFirst ruagent-store -CleanFirst ruagent-mcp -CleanFirst ruagent-daemon` 强制重读：**exit 0，elapsed=23.3s，12 条 `Checking`（含 `Checking ruagent-daemon v0.1.9`），`: warning:` 0 行、`: error:` 0 行** |
| D3 | `scripts/cargo-team.ps1 test --workspace` | **exit 0**；**`test result: ok` 行 = 87**；**`FAILED` 行 = 0**；**`passed` 求和 = 952**（`ignored` 求和 = 22，elapsed=168.8s） |
| D4 | `cd panel && npm run build` | **exit 0**；wrapper 依次跑了 `i18n-check`（`VERDICT: PASS`，zh=1035 / en=1035，unresolved refs 0）、`tsc -b`、`tsc -p e2e/tsconfig.json --noEmit`，然后 vite `✓ 1853 modules transformed`、`built in 9.97s`、`dist updated (55 assets)` |

D3 的 target 级佐证（同一次运行里我按 target 抓的行）：
`ruagent-store --lib ok. 48 passed` / `ruagent-daemon --lib ok. 290 passed; 1 ignored` / `ruagent-policy --lib ok. 13 passed` /
`supervise_wakeup ok. 4 passed` / `capabilities ok. 12 passed` / `capability_defaults ok. 5 passed` / `e2e_daemon ok. 25 passed`。

**判读**：四条门禁都是绿的，且 D2 那条我是**强制重读**之后才认的（缓存绿在这台机器上是常态，见 `scripts/cargo-team.ps1` 自己的说明）。

---

## 3 两个特别问题

### 3.1 自愈 run 有没有继承 `origin_chat_id`（B2 的绿是结构还是碰巧）

**结论：继承了，B2 的绿是结构成立，不是碰巧。**

**代码**：`runs.rs:1062`，`retry_run_inner` 给新 attempt 的 `RunLaunch` 写
`origin_chat_id: old.origin_chat_id.clone()`（注释：`a heal retry answers to the SAME conversation`）。

**活实例读数**（B1 / B2 / R1 三处都单独看过）
```
B1  run1 failed    origin=01a1f986-52a0-…  self_heal_of=null
B1  run2 failed    origin=01a1f986-52a0-…  self_heal_of=01a1f986-52d8-…   ← 同一个 chat
B2  heal run       origin=01a1f986-cf9a-…  self_heal_of=01a1f986-cfcd-…
R1  heal run       origin=01a1f987-744a-…  self_heal_of=<被中断的那个>
```
**为什么这条重要（我特意读了一遍失败会怎样）**：`finish_cluster` 决定「叫醒谁」时读的是 `runs.last()` 的 origin（`runs` 按 `created_at ASC` 排，`last()` = 最新那一条 = 自愈那一轮）。如果自愈 run **没有**继承 origin，那么自愈一轮结束时 `runs.last().origin_chat_id` 会是 `None` → 直接 `return` → **一条汇报都不会发出**。B2 的「恰好一条」就会变成「零条」，而 B1 连自愈都无法触发（没有 origin 连自愈分支都进不去，见 B1 反例）。所以 B2 的绿依赖这条继承，继承是**显式写的**，不是偶然。

### 3.2 重启路径：会不会回写 task、会不会再花 token

**结论：会回写（task 不再是 `in_progress`）；而且如果这个簇是 chat 派发的，它会自己再起一轮 run（花 token）——并且那一轮的汇报会因为「没有活会话」被丢弃。**

**代码**：`lib.rs` 把启动清扫拆成两半——`interrupt_live_runs`（`lib.rs:896`，只把非终态 run 标 `Interrupted`）与 `finalize_interrupted_clusters`（`lib.rs:701`，每个 distinct `task_id` 调一次 `runs::finish_cluster`，`BTreeSet` 去重）。`finish_cluster` 里自愈的前置条件是「簇全终态 + 有 origin + 簇内还没自愈过」，`Interrupted` 是 `is_retryable()`，所以重启会走进去。

**实测 R1：chat 派发、跑到一半被硬杀（按 PID `Stop-Process -Force`）**
```
before kill : run status=running        task status=in_progress
after  boot : task4.status=in_progress  runs=2
   run 01a1f987-7488-… status=interrupted  self_heal_of=[]                  origin=<chat4>
   run 01a1f987-e16c-… status=running      self_heal_of=01a1f987-7488-…     origin=<chat4>
boot log    : WARN runs marked interrupted by restart count=1 tasks=1
              WARN self-heal: started the cluster's one automatic retry; the report waits for it
                   healed_run=01a1f987-7488-… heal_run=01a1f987-e16c-…
                   from_agent=slowpoke to_agent=boom-a
              INFO cluster write-back attempted for the runs the restart interrupted tasks=1 runs=1
final       : task4.status=pending   runs=2 (interrupted + failed)
chat4       : user_message events = 0   platform events = 0
log         : WARN platform event dropped: the chat exists in history but has no live session
                   (closed or idle-reaped); a report cannot start a turn, and this is NOT retried
                   chat_id=01a1f987-744a-…
```
**实测 R2：面板派发（origin 为空）、跑到一半被硬杀**
```
before kill : run status=running  task=in_progress
after  boot : task5.status=pending   runs=1
   run 01a1f988-393c-… status=interrupted  self_heal_of=[]  error="daemon restarted mid-run"
```
**判读（这条直接打到钱包）**
1. **回写成立**：两条路径（chat 派发 / 面板派发）的 task 最终都离开 `in_progress`（`pending`），R1 里中途那一段 `in_progress` 是因为自愈真的又开工了，属真话。
2. **重启会花 token，实测**：chat 派发的簇被重启打断后，boot 会**自己起一个新的 run**（R1：`runs=2`，第二个是 `self_heal_of` 指向被中断那个、换了 agent、`origin` 继承）。R2 证明这条花钱路径**只对 chat 派发的簇成立**：面板派发的只回写、不重起。
3. **而且这一次的汇报是丢的**：那一轮跑完时，这个 chat 在重启后的进程里**没有活会话**（重启不会恢复会话），`notify` 走到「历史上存在但没有活会话」那条分支，只 WARN、不重试。所以 R1 花了钱、自愈了、也组好了汇报文案，最后**一句话没送到用户眼前**。规格 §6 预见了「`generating` 是内存态、重启即丢排队中的唤醒」，但这里是更前面的一环：**连 chat 本身都不是活的**。这不是实现者悄悄漏掉的（`chat.rs` 的 `notify` 分支注释写得很清楚，日志也说了），但它和「你肯定要向用户汇报」这句用户原话之间有一个洞：**重启之后，平台说的话没人收。**
4. 补救方向（不属于验收，只是指出形状）：要么在重启后把「待汇报」落成一条持久记录（P1 的回执列表天然是那个地方），要么允许 `notify` 对一个「历史存在但没活会话」的 chat **发一条一次性的系统通知/未读标记**，而不是只 WARN。现在的状态是「钱花了，话没说」。

---

## 4 与实现者读数不一致的地方

实现者只留下了一份报告（`docs/plans/2026-11-20-p3-watchdog-report.md`；`supervise` / `receipts` 两份没有留下报告）。逐条比对：

1. **`test --workspace` 的红绿**
   - 他写（§⑥）：`exit=101`，`test result: ok` **86** 行 / `FAILED` **1** 行 / `passed` 合计 **951**；唯一红是
     `api::tests::the_topology_dispatches_carry_the_origin_onto_every_run_they_create`，
     原因是夹具用裸命令名 `mock-agent`，本机 PATH 上没有。
   - 我读到（当前字节，`api.rs` blob `e453e08`）：**exit 0 / ok 87 行 / FAILED 0 行 / passed 952**，
     那条测试**通过**（`ruagent-daemon --lib` 290 passed）。
   - 差异在哪：那条测试的夹具**已经改过**——现在用 `std::env::current_exe()` 的绝对路径当 agent 的 `command`，注释里就写着「裸名字是机器的属性，不是被测代码的属性」。
     `api.rs` 的 mtime 是 **22:52:40**，他的报告是 **22:44:02**，也就是说他的红属于**更早的字节**。
     我的读数 +1 ok 行 / +1 passed 正好等于「那个 target 从 `289 passed; 1 failed` 变成 `290 passed`」。
   - 结论：**他的红已过期，不是当前树的状态**；他报告 §⑦ 第 13 条「属于 `receipts` 的夹具形状问题」也随之失效。我这份读数是当前字节的。
2. **「重启会花 token」** —— 他写「已由 lead 拍板接受」，我实测**确实会**（R1），并补上他没写的那一半：**那一轮的汇报会因为没有活会话而丢失**（§3.2 第 3 点）。他的报告把损失说成「`generating` 是内存态、排队中的唤醒会丢」；实测的损失比这句更彻底——丢的不是队列，是**整个会话不存在**。
3. **A3 那条「只有一处」** —— 他的报告没提；我读到 `watchdog.rs:363 inflight()` 是同一问题的第二个读者（不参与唤醒判定）。这不是他报告里的错，是他没覆盖到的一个点，我记在 A3 的注里。

没有发现「他说绿我读到红」的项目；也没有发现他声称做了而我读到没做的项目。反向的一条我记一下：他报告 §⑦ 第 1 条自陈「端到端没有测试」，而 `crates/daemon/tests/supervise_wakeup.rs`（`supervise` 写的，4 passed）确实端到端覆盖了 P2 的链路——两份报告之间没有对齐，但不影响结论。

---

## 5 我没能确认的（推断不是读数）

1. **watchdog 真的到点起了一轮**：一次都没验。我起的两个私有 root 都没写 `[watchdog] enabled = true`，所以「定时器 → `start_run` → 真 agent」这条链在我手上只有代码 + crate 单测（`watchdog::tests::*`，在我这次 `test --workspace` 里通过），**没有活实例读数**。同样地，「capability 关着时它确实不发一轮」也只有单测（`the_watchdog_capability_is_the_cost_gate_…`）与代码，不是活读数。
2. **`notify` 在「正在飞」时排队、并在同一轮结束后补发**：只有 crate 单测（`a_platform_event_arriving_mid_turn_is_queued_and_delivered_when_the_turn_ends`，在我这次 workspace run 里通过）。活实例里我没能制造出「平台事件恰好撞上一轮在飞」的时序（我的 chat 在被唤醒时都是 idle 的），所以我只验到「idle → 立即发」这一半。
3. **pipeline / judge 的 origin 传递与聚合**：只有代码 + `api.rs` 的单测。B2 我验的是「同一 task 上两个 run 的簇聚合」，没用 `/pipeline`（pipeline 的每一步是独立子 task，报告是**逐步**的，规格自己也这么写）。
4. **面板英文语种**：两块的回执文案我只在 `zh` 下用浏览器读过；`en` 侧只有构建期的 `i18n-check`（zd/en 键集一致、`unresolved refs=0`）这一条机械读数。
5. **`ignored` 的 22 条**：我报了求和，但没有逐条核对 ignored 清单是否与被约束的「`#[ignore]` 必须逐个登记」契约一致（那是 CI 的另一道门，不在剧本里）。
6. **R1 之后用户还能不能看到那条被丢的报告**：我观测到它是**被丢弃且不重试**（WARN 原文如此）；我没有再验证「用户重启后自己打开那个会话、平台会不会补发」——从代码读是不会（没有持久待发队列），但这是读代码，不是读数。
7. **迁移整体重放**：我按名字跑了 **0032 这一条**的重放测试，并读到 `ruagent-store --lib` 48 passed（既有测试覆盖其余步骤）；我没有手工把 32 条脚本逐条重放一遍。
8. **`origin_chat_id` 是否被校验**：我没有测「传一个不存在的 chat id」会怎样（代码读起来是不校验，事后唤醒时才判「没有这个 chat」）。这也属于读代码。
9. **我删掉了什么**：私有 root、私有二进制、`%TEMP%\psverify-v1` 下的一切都按具体路径删了。本报告里的引文都是**原文照抄**（对话转录、run 行、日志行），删掉之后无法再从源文件复读；四个小日志（`%TEMP%\verify-test-workspace.log` / `verify-clippy.log` / `verify-clippy-forced.log` / `verify-panel-build.log` / `verify-build.log`）我**保留**了，因为报告直接引用了它们的行，删了就成了不可复查的读数。这属于「引用了就别删」与「临时目录要清」之间我选的前者，请按此判读。

---

## 6 收尾核对

- 只读产品代码：是。唯一写入是这份报告（加上 §5 第 9 点说明保留的那几个日志）。
- 没有提交 / push / tag：是（`git status` 与开始时相比只多出本文件）。
- 没有碰 8787：是。全程只对 `127.0.0.1:8791` / `8793`（我自己的两个私有 daemon）发 HTTP；收尾前 `Get-CimInstance` 确认只剩常驻的 `pid 49800`。
- 私有 daemon 按 PID 停：`35120`、`13528`；停完 `8791` / `8793` 均已无人监听。
- 私有 mock 子进程：收尾时 `Name='ruagent-mock-agent.exe'` 查询结果为**空**，没有孤儿。
- 浏览器会话：`agent-browser close --all`（`✓ Closed session: psverify`）。
