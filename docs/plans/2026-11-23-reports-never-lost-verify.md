# 汇报永不丢 · 独立验证报告

日期：2026-11-25 · 验证者：`verifier2`（独立，不是实现者）
剧本：`docs/plans/2026-11-23-reports-never-lost-acceptance.md`
规格：`docs/plans/2026-11-23-reports-never-lost.md`
基线：`d563b03`（v0.1.10）+ 工作树（`outbox` 的 P4 + `heal-origin` 的 P5/P6 + lead 补的会话侧 P6）
机器：Windows，私有根 + 私有端口（8795/8796/8797），**未碰 8787 上的常驻实例**（收尾时确认 `8787` 仍在监听）。

**本报告里每个数字都是我自己取的**：要么我起私有 daemon 用 HTTP 打出来的，要么我用 `python sqlite3` 只读直读私有库，要么我自己跑的命令输出。没有引用任何人的读数（§6 里对实现者的引用只用于「比对分歧」，不用于支撑结论）。

---

## 0 总判定

| 项 | 判定 | 我自己的读数 / 文件:行 |
| --- | --- | --- |
| **P4 平台事件 outbox** | **达标（核心），但「一次补一条」不成立 —— 补发会自己连锁** | 落盘：`crates/daemon/src/chat.rs:1506` 之后的三个分支都会先 `enqueue_chat_report`；活读数：闭会话派活 → outbox `id=1 delivered_at=null attempts=0` → `resume` → 会话里 1 条平台事件、`delivered_at` 非空、`attempts=1`。**但** 3 条 pending 时 resume 返回 `pending=2`（只补 1 条），**1.1 s 后 3 条全部自己送到**（§5 F 节）。 |
| **P5 自愈世代** | **达标（有限定条件）** | 同一 task 派两次、两次都失败 → 4 个 run，`dispatch_epoch` = 0,0,1,1，`self_heal_of` 2 条，settle 15 s 后仍是 4 个 run（`crates/daemon/src/runs.rs:2302 heal_budget_spent`）。限定条件：**没有 origin 的 cluster 一次自愈都没有**（`runs.rs:2208` 在自愈之前 return）—— 我实测过一次次尝试：无 origin 派发 → 只 1 个 run。 |
| **P6 origin 自动注入** | **达标（三处都验了，含 lead 补的会话侧）** | run 侧：`env RUAGENT_CHAT_ID=<chat>` / `child=<chat>`；无 origin：`<unset>`；**会话自己的 agent 进程**：`env RUAGENT_CHAT_ID=<chat> pid=46804 child=<chat>`（`crates/daemon/src/chat.rs:1279` + `crates/acp/src/chat.rs:170`）；`task_dispatch` 缺省继承、显式优先、空串按缺席（`crates/mcp/src/lib.rs:704`），我用 `--nocapture` 自己跑了 `dispatch_origin` 两个用例并读到三条 wire 读数。 |
| **F（无人时是否反复花钱）** | **有：没有定时器/boot 会自己触发投递，但「一次 resume 或一次说话」之后，pending 会被一条一个 turn 地连发到清空，无人参与** | §5。上界 = 每 chat 50 条（cap），每条 = 一个完整会话 turn（真 harness 下即一次付费模型调用）。 |

---

## 1 A 节（结构面）

### A1 outbox 表存在、迁移可重放 —— 达标

- 迁移文件：`crates/store/src/migrations/0033_chat_reports.sql`（`CREATE TABLE IF NOT EXISTS chat_reports` + `CREATE INDEX IF NOT EXISTS idx_chat_reports_pending`）。
- 头号：`crates/store/src/migrations.rs:41` 把 0033 追加进 `MIGRATIONS`；`:45` `pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;`（所以 32 → 33 是跟着数组走的），字面量 33 的 pin 有 3 处：`crates/store/src/migrations.rs:1109`、`:1487`（新表测试内）、`:1676`。
- **我的活读数（全新库）**：私有 root 第一次启动，日志 `applied schema migration version=1 … version=33`；随后直读库：`max(version)=33`、`chat_reports` 表存在、索引存在、列顺序 = `['id','chat_id','run_id','text','created_at','delivered_at','attempts']`。
- **我的活读数（旧库升级，不是测试替身）**：我用仓库自己的 32 个迁移文件手工搭了一个 **version=32** 的库（建库后立即直读：`max version = 32`、`chat_reports` 计数 0），并在里面塞了一行 0033 之前形态的 `chats` 行；然后让 daemon 在这个库上启动 —— 日志只有一行 `applied schema migration version=33`，之后 `max(version)=33`、表/索引都在、**旧 chats 行还在**、`chat_reports` 行数 = 0（没有凭空造一条汇报）、旧 chat 在 `GET /api/v1/chats` 里读到 `pending_reports: 0`。
- 可重放：`test --workspace` 里 `the_chat_report_outbox_upgrades_an_old_database_and_reapplies_cleanly` 通过（含它自己的负控）。这条是「跑别人的测试」，不是我的探针 —— 但**旧库升级**那一半我上面用真库验过。

### A2 `text` 不带平台标记 —— 达标

- 代码：标记只在投递那一刻加 —— `crates/daemon/src/chat.rs:474 fn platform_event`，唯一调用点是 `chat.rs:530 chat.send_prompt(db, embedder, platform_event(&event.text))`；写库处 `crates/store/src/lib.rs:881 enqueue_chat_report` 存的是原样 `text`。
- **我的活读数（直读库）**：`select count(*) from chat_reports where instr(text, char(8291))>0` → **0**（U+2063 = `USER_TEXT_SENTINEL`）；全表 8 行，含 sentinel 的行数 = 0（前 3 行的逐行断言也都是 `False`）。
- 附带一条：**投递后的会话事件**里 sentinel 出现 2 次 —— 一次是 `platform_event()` 加的，一次是 transcript 自己的 `ruagent:user-text<sentinel>` 尾巴；最后一个 sentinel 之后只有空白，`sessions::user_text`（`rfind`）因此读到空用户正文，即「平台在说话」（我读 transcript 断言：`text after sentinel='ruagent:user-text\u2063\n\n'`，平台文字之后无用户文字）。

### A3 `delivered_at` 是唯一已投递判据 —— 达标

- 「还欠着」的谓词只出现在 `WHERE … delivered_at IS NULL`：`crates/store/src/lib.rs:900`（cap 计数）、`:909`（cap 删旧）、`:941`（pending 读）、`:972`（per-chat 计数）、`:990`（全表计数）、`:1010`（标记已投递时的幂等 guard）。
- `attempts` 只有自增一处：`crates/store/src/lib.rs:1025 UPDATE chat_reports SET attempts = attempts + 1`；全仓没有任何 SQL 拿 `attempts` 作谓词（我按 `attempts` grep 过 crates/**，命中的都是注释、测试与 distill 的另一个 `attempts` 概念）。
- **我的活读数（直读库）**：`attempts>0 AND delivered_at IS NULL` = **0**；`attempts=0 AND delivered_at IS NOT NULL` = **0**；被 cap 丢掉的行是 `DELETE` 掉的（表里只剩 50 行，不是标记）。

### A4 补发只有一个发起处 —— **不达标（两处）**

- `resume_chat` 之后：`crates/daemon/src/chat.rs:1192 self.deliver_one_pending_report(&chat)` → `chat.rs:1670`（读 pending，投递一条）。这是规格 §P4.3 说的那一处。
- **第二处**：turn 结束的 watcher `chat.rs:1392 drain_one_notification(...)`，它在内存队列为空时**回落到 outbox**：`chat.rs:579 db.chat_reports(&chat.id, true, 1)` → 投递一条。
- 两者都走 `deliver_or_queue`（`chat.rs:493`，判据仍只有一处：`generating`），所以「一轮只能有一条在飞」没被绕过；但「读 pending 并投递」的**发起处是 2**，而剧本 A4 要求 1。这不是命名问题，它有可观测后果 —— 见 F 节：第二个发起处把「一次一条」变成了「一条接一条直到清空」。
- 备注：`runs.rs:2208` 那段注释自己写了「同一个 hook / 一个判据」，规格 §P4.2 也写了「drain 成功 = 标 delivered_at」，所以第二处并非偷偷加的，但它**超出了规格 §P4.3 的「只在一处发起」**，剧本 A4 因此读红。

### A5 自愈世代按 `params.dispatch_epoch`，旧行走保守分支 —— 达标

- `crates/core/src/run.rs:159-179`：`dispatch_epoch: Option<u32>`，`#[serde(default, skip_serializing_if = "Option::is_none")]`（`runs.params` JSON 列，**无迁移**），doc 明确写了「旧行 `None` ⇒ 视为同一世代 ⇒ 不额外给预算」以及为什么朝保守方向。
- 判据只有一处：`crates/daemon/src/runs.rs:2302 fn heal_budget_spent(runs, ending)`；`None` 分支 = `runs.iter().any(self_heal_of.is_some())`（旧行为，不给新预算）；有 epoch 时只看同代。调用点唯一：`runs.rs:2231`。
- 世代产生：`runs.rs:1008 next_dispatch_epoch` = 该 task 的**非自愈** run 数（0 起）；fan-out 只算一次（`runs.rs:1162`）；自愈继承被治愈者（`runs.rs:1131 DispatchEpoch::Of(old.params.dispatch_epoch)`）；人类 retry = `New`。
- 旧行进代码的行为：`crates/daemon/tests/heal_generations.rs::a_pre_versioning_heal_row_still_counts_as_this_generation` 在我这次 `test --workspace` 里通过（这是跑别人的用例；我自己**没有**单独构造旧行探针 —— 见 §6）。

---

## 2 B 节（行为面）

### B1 落盘后补发（P4 主判据）—— 核心达标；「只补一条」只在 resume 返回那一刻成立

我怎么验的（私有 root-a / 8795）：
1. `POST /api/v1/chat {"agent":"talker"}` 建会话（live）→ `DELETE /api/v1/chat/{id}` 关掉（204，行留在 `chats`）；
2. `POST /api/v1/tasks` + `POST /api/v1/tasks/{id}/runs {"agent":"boom-a","origin_chat_id":<chat>}`，boom-a/boom-b 都是 `mock --behavior crash`，所以 cluster 必然失败；
3. 读 `GET /api/v1/chat/{id}/reports?pending=1`、`GET /api/v1/chats`；
4. `POST /api/v1/chat/{id}/resume`，再读会话 transcript（`data/transcripts/run-<chat>.jsonl`）与 outbox。

读数：
- cluster 收尾：2 个 run（`boom-a` failed + `boom-b` 自愈 failed），第二个的 `params.self_heal_of` 指向第一个，两者 `dispatch_epoch = 0`。
- **outbox（resume 前）**：`{"id":1, "delivered_at":null, "attempts":0, "pending":1}`；正文是集群失败汇报（"我试了 2 次（含 1 次自愈重试）…"），**`run_id` 是 null**（见 §6 的缺口）。
- `GET /api/v1/chats` 该行：`pending_reports: 1`、`active: false`。
- **resume 之后**：会话 transcript 里 `user_message` 且含「平台事件」的**正好 1 条**；outbox 同一行 `delivered_at="2026-11-25T13:28:04.553130900+00:00"`、`attempts=1`、`?pending=1` 返回 0 行；resume 响应 `pending_reports: 0`。
- **第③条（堆多条只补一条）**：另建 C2（关掉）→ 在**同一个 task 上先后 3 次派发**（每次都等上一簇收尾，因此是 3 个各自独立的 cluster → 3 条汇报）→ outbox 3 条 pending（id 2,3,4）→ resume 响应 `pending_reports=2`，即**此刻只补了 1 条**；但 1.1 s 后 `pending=0`、`delivered=3`、transcript 里平台事件 3 条。**「其余仍 pending」这一条按字面不成立**。

判读：P4 的主判据（不丢、resume 后真的出现在会话里、`delivered_at` 非空、`attempts>=1`）我全部测到。剧本第③条与规格 §P4.3 的意图（"不要在一个 turn 里连发 N 条……那会把主 agent 的话抢成一串独白"）在实现上是「一条一个 turn、自动连发」，人在不在场都一样 —— 见 F。

### B2 上限 50 丢最旧 —— 达标

- 一次填 53 条（**每个 task 一次派发** —— 一个 task 的多次派发属于同一个 cluster，只在整簇结束时汇报一次，这是产品语义，不是 bug；我第一次用一个 task 填 53 次只得到 1 条汇报，读数作废并重做）。
- 读数：`dispatched=53`（saturation 重试 41 次，mock harness 队列 8）→ `pending=50  rows_still_in_table=50`；保留 id **10…59**，被丢的是 **7,8,9**（= 最旧三条）；日志恰好 3 条点名 WARN：`report outbox FULL … report=7 cap=50` / `report=8` / `report=9`；`GET /api/v1/chats` 该行 `pending_reports: 50`。
- 顺带测到（不是要求项）：mock harness 饱和时自愈起不来，日志 `self-heal: the automatic retry could NOT be started … harness Mock is saturated`，于是那一簇只试 1 次就汇报 —— 行为是既有的，不是本次改动引入。

### B3 删除级联 —— 达标

- 删前：`rows=50 pending=50`；`DELETE /api/v1/chats/{id}` → 204；删后 API `rows=0 pending=0`；直读库 `rows for the deleted chat = 0`、`chats row = 0`。
- 代码位置：`crates/store/src/lib.rs:755` 起 `delete_chat` 在**同一个 writer 闭包**里先删 `chat_reports` 再删 `chats`。

### B4 重启不再「花钱买不到话」（P4.5）—— **达标：真的杀了 daemon 再重启**

这是我唯一一条「真进程重启」的读数，没有用「读代码推断」。

```
私有 root-b / 8796，独立 daemon 进程
1) 起 daemon，health=ok；POST /chat（talker，live）；POST /tasks + /tasks/{id}/runs
   {"agent":"slow-a", "origin_chat_id":<chat>}（slow-a = mock slowreply，握 10 s）
   读 run：status=running
2) Stop-Process -Id <daemon pid> -Force     （TerminateProcess，不是优雅退出）
   → health=none；<root>/data/daemon.exit 仍停在 "running pid=59912 since=2026-11-25T13:40:31.712Z"
     （= 被杀的形态：没有 exit 记录）
3) 同一 root、同一端口重新起 daemon（health=ok）
   日志：WARN "runs marked interrupted by restart count=1 tasks=1"
         WARN "self-heal: started this dispatch's one automatic retry … from_agent=slow-a to_agent=boom-a dispatch_epoch=Some(0)"
         INFO "cluster write-back attempted for the runs the restart interrupted tasks=1 runs=1"
   簇：2 个 run —— 原 run status=interrupted error="daemon restarted mid-run"，
        自愈 run status=failed（boom-a 崩）且 self_heal_of = 原 run
   outbox：1 条 pending（delivered=null, attempts=0）—— 重启后聊天没有活会话，汇报停在盘上
4) POST /chat/{id}/resume
   → 响应 pending_reports=0；transcript 里 1 条平台事件（正文含 "slow-a — Interrupted：daemon restarted mid-run"、
     "boom-a（自愈重试） — Failed…"、"我试了 2 次（含 1 次自愈重试）"）
   → outbox 该行 delivered_at 非空、attempts=1
```

判读：**既花了那一轮（boot 清扫真的起了自愈 run，`crates/daemon/src/lib.rs:981` 的 `true`），也收到了那句话（resume 后出现在会话里）**。这是 P4.5 的唯一有效证据形状，成立。

注意一处与实现者测试的差别：仓库里的 `a_restart_heals_the_cluster_and_the_report_arrives_when_the_conversation_does` 是**在进程内**调用 `interrupt_live_runs` + `finalize_interrupted_clusters` 模拟重启的（它自己在文档里也这么说）。它是有价值的，但它**不是**「真的重启」。上面这段是。

### B5 自愈世代 —— 达标（有前提）

第一次尝试作废：我派发时**没带 `origin_chat_id`**，两个 run 各自只留下 1 个 attempt、**没有任何自愈**。原因在读代码里找到了：`crates/daemon/src/runs.rs:2208` —— 簇的最后一个 run 没有 origin 就直接 `return`，第 (4) 段的自愈根本到不了。**P5 的判据只对「会话派发的活」可观察**。这既是我第一次读数的解释，也是一条应当写进报告的边界（实现者的报告与规格都没写）。

重做（带 origin，闭会话 C7）：
```
dispatch 1 → runs=2, heals=1   epoch: 0 (attempt) / 0 (self-heal of it)
dispatch 2 → runs=4, heals=2   epoch: 0,0,1,1
settle 15 s → runs=4, heals=2（没有第 3 个 attempt，即同代没有第二次自愈）
outbox：1 条，"我试了 4 次（含 2 次自愈重试）"
```
判读：两次人类派发、两次都失败 → **每代各一次自愈**；同代内不出现第二次。报告文案的「含 2 次自愈重试」也跟着代数走了（`runs.rs:2412` 起按 heal 行计数），不是写死 1。

### B6 origin 自动注入 —— 达标（三处 + 工具侧）

| 断言 | 我的读数 |
| --- | --- |
| 带 origin 的 run 的 agent 进程有它 | `run 01a212ed…`（envdump agent）：`result = "env RUAGENT_CHAT_ID=<01a212ed-8f53-…> pid=31912 child=<01a212ed-8f53-…>"`，status=completed |
| 无 origin 的 run 里**不存在**（不是空串） | 另一 run：`result = "env RUAGENT_CHAT_ID=<unset> pid=9932 child=<unset>"` |
| **会话自己的 agent 进程也有**（lead 补的那一处） | 用 envdump 作会话 agent：`POST /chat/{id}/messages` 后 transcript 里 `{"text":"env RUAGENT_CHAT_ID=<01a212ed-8f53-7081-a3ae-1e11a86dd340> pid=46804 child=<01a212ed-8f53-…>"}` —— 变量在、值是会话 id、子进程继承 |
| `task_dispatch` 缺省继承、显式优先、空串按缺席 | 我自己跑了 `cargo-team.ps1 test -p ruagent-mcp --test dispatch_origin -Nocapture`：`2 passed`；三条 wire/回复读数分别是「belongs to NO conversation…」（无环境下不带 `origin_chat_id`）、「belongs to conversation: 01a2…c001」（读环境）、「belongs to conversation: 01a2…c002」（显式覆盖环境）。用例里还有 `RUAGENT_CHAT_ID=`（空）→ 按缺席处理 |
| 「一个 run 一个进程」成立 | 带 origin 的 run pid=31912、无 origin 的 run pid=9932、会话进程 pid=46804：三个不同 pid |

---

## 3 C 节（读数面）

- **C1** `GET /api/v1/chats` 每行带 `pending_reports`，且与 outbox 实际一致：
  - 欠 1 条 → `1`（与 `?pending=1` 的 1 行一致）；
  - 被 cap 压到 50 → `50`（与表里 50 行一致）；
  - 已投递完 → `0`（是读数，不是 null）；删掉的会话不再出现在列表；
  - 代码里为 `None`（未知）的分支只有「outbox 计数读失败」一条（`crates/daemon/src/chat.rs:2430` 起），**我没有构造出读失败**，所以 `null` 这一态只有代码证据（§6）。
- **C2** `GET /api/v1/chat/{id}/reports?pending=1` 能查到未投递原文：读到 `text`/`created_at`/`delivered_at:null`/`attempts:0`；不加 `pending` 时同一会话在投递后仍能看到那一行（`delivered_at` 非空）。边界：路由把 id 解析成 `RunId`，所以非 UUID 形状的 chat id 会得 400 `invalid chat id`（我在旧库探针里用了一个非 UUID id，撞到了这条）。
- **C3** 面板未读数三态：`panel/src/views/Chat.tsx:67 export function pendingReportBadge(n)` —— `null/undefined → null`、`!isFinite(n) || n <= 0 → null`、否则 `Math.trunc(n)`；调用点 `Chat.tsx:2193`，渲染 `.chat-unread`（`panel/src/index.css` 新增），文案 `chat.pendingReports`（zh/en 都有）。**这一条是代码阅读 + `npm run build` 的类型检查，不是渲染读数**（没有浏览器、没有 DOM 测试；也没有单测钉住这个纯函数 —— 见 §6）。

---

## 4 D 节（门禁，带读数）

取读数时 `git status --porcelain` = 27 条（19 modified + 8 untracked，全是被验的改动）；收尾时 28 条，唯一新增是本报告。被验的 19+8 条前后一致（关键文件的 blob id 见下，门禁前后两次相同）。关键文件的 blob id（`git hash-object`，门禁前后两次一致）：

```
37b0b10caf14ec8f0282bf2ae64af65bd7397435  crates/daemon/src/chat.rs
1a113d3a05265b24fc8c3dd5c82f60564dc8b6d7  crates/daemon/src/runs.rs
a63ca5017f1ffb9cc8eb191a5c8d45ce4bf6558d  crates/daemon/src/lib.rs
9cbfdedf89b4a3b271199c98d1d177b45c492b35  crates/daemon/src/api.rs
218c7aa70751bf8ca97fa405c0fd77d2e156f9eb  crates/store/src/lib.rs
2a0f94ed7ac06e04563ee547505b45810864be60  crates/store/src/migrations.rs
0e23bcba93625b25dcfb1e94b564800698f50732  crates/core/src/run.rs
b277bb142e85dea805b4793af9d2b743c2d62ad0  crates/acp/src/chat.rs      <- lead 补的那一处
0cd46db6a52404855d6f9568ebff7cd2b9ce5fb4  crates/acp/src/run.rs
d12ce357e54aa677a566bae5862096ec5c15ac38  crates/mcp/src/lib.rs
544ce89081dfdcfd34f7a6631253e8ac8db7d816  panel/src/views/Chat.tsx
e30edabf765f098334590e17327af9c1b38866d7  crates/mock-agent/src/lib.rs
```

| 门禁 | 我的读数 |
| --- | --- |
| `cargo fmt --all --check` | **exit 0**，2 s，无输出（0 个 diff site） |
| `scripts/cargo-team.ps1 clippy --workspace --all-targets -DenyWarnings` | **exit 0**，`Finished dev profile in 3.28s`，**没有任何 `Checking` 行** —— 这是缓存命中的形状（AGENTS.md 专门警告过这条）。我的判读：`cargo` 的 fresh 检查通过说明**这批字节**已在同一 target dir 被 clippy 检过且干净；但我**没有**用 `-CleanFirst` 强制重读（那会清空队内共享 target dir、拖慢其他人），所以这一条我按「缓存干净的读数」报，不按「刚重新 lint 过」报。更强的同字节读数是下一行的 `test --workspace`（全量重编译并跑了 965 个测试）。 |
| `scripts/cargo-team.ps1 test --workspace` | **exit 0**，elapsed **225.1 s**；三元组：**ok 行 91 / FAILED 行 0 / passed 求和 965**（ignored 22，79 个 target）。注意口径：`Select-String` 默认**大小写不敏感**，所以朴素地数 `FAILED` 得 106（全是 `0 failed` 汇总行和测试名）；**大小写敏感**数 `FAILED` = 0，`test result: FAILED` = 0。四个新 target 全绿：`chat_reports_outbox` 4/4、`heal_generations` 2/2、`chat_origin_env` 2/2、`dispatch_origin` 2/2。 |
| `cd panel && npm run build` | **exit 0**，52 s；`i18n-check` + `tsc -b` + `tsc -p e2e --noEmit` + vite 依次通过，末尾 `build-panel: dist updated (55 assets …)`。 |

**读数属于哪份字节**：`test --workspace` 与 panel build 跑在上面那批 blob id 上（我前后各哈希一次，一致）。`clippy` 那一行是缓存读数（同上）。

---

## 5 F 节：补发会不会在用户不在场时反复花钱？—— **会，但不是「自己开始」，而是「一旦有人碰过就会自己跑完」**

**先回答「有没有一条路径不需要人打开会话就能触发投递」：没有。**

- 实测：root-a 上留了 1 条 pending（C7，`attempts=0`），**真杀 daemon 再重启**（同一 root），重启后 20 s 内那条行仍是 `pending=1 / attempts=0 / delivered_at=''` —— boot 不做投递，只做「把被打断的 run 标 interrupted + 让可自愈的簇自愈」，而自愈产出的汇报会**落盘**、不投递。
- 代码侧对得上：`chat_reports` 的读者只有 4 处 —— `resume_chat`（`chat.rs:1192`）、turn 结束的 drain（`chat.rs:1392`→`579`）、`GET /api/v1/chats` 的计数（`chat.rs:2430`）、`GET /chat/{id}/reports`（`api.rs:7518`）。没有定时器（`chat.rs` 里唯一的 60 s 循环是 idle reaper，它**关闭**会话；`watchdog.rs` 的定时器起的是 **run**，不是会话 turn），boot 不读 outbox。

**但它有一条自动连锁，触发一次就会一条接一条送到清空：**

- 机制：drain 在内存队列为空时回落到 outbox，取最旧一条 → `deliver_or_queue`（不 generating 就 `send_prompt`）→ **这条投递本身又是一个 turn** → 那个 turn 结束时 watcher 再 drain 下一条 → …… 直到该 chat 的 pending 清空。
- 我的读数（最硬的一条）：3 条 pending 的会话，`POST /resume` 返回 `pending_reports=2`（**此刻只补了 1 条**），
  - `t=142 ms`：`pending=2 delivered=1 platform_user_messages=1`
  - `t=1161 ms`：`pending=0 delivered=3 platform_user_messages=3`
  即 **1 秒内剩下两条全部自动送出，没有任何人再操作**。
- 频率与代价：**每条 = 一个完整的会话 turn**（`send_prompt`，带记忆/能力注入；真 harness 下就是一次付费模型调用 + 它的工具轮次）。连锁没有固定周期，它跟着 turn 的延迟走；条数上界 = 每 chat 的 **50**（cap），所以**一次 resume 最多可以自动烧掉 50 个 turn**，中间不需要任何人在场。
- 触发条件只有两个（都要「会话已经是活的」）：一次 `POST /resume`，或者用户/平台在该会话里说了任何一句话（那句话的 turn 结束就会开始 drain）。**它不会自己从零开始**。
- 另外一处放大风险（代码阅读，未实测）：`deliver_or_queue` 里 `mark_chat_report_delivered` 失败只 WARN，行仍 pending；如果这条写持续失败，链每轮都会把同一条再发一次，理论上永不收敛（`chat.rs:530-545`）。
- 与规格的关系：规格 §P4.3 写的是「补发在 resume 之后只发一条，其余等下次」；实现多了 turn-end 兜底，于是「等下次 resume」实际上变成「等下一个 turn」。**这是我认为本增量最需要 lead 裁决的一点**：它是「汇报一定送到」和「不在场不花钱」之间的取舍，当前实现选的是前者，而且没有上限开关。

---

## 6 与实现者读数不一致的地方

1. **`docs/plans/2026-11-23-p5-p6-report.md` §⑦a：「会话侧（`chat.rs` 的那个 agent 进程）没有注入」—— 现在已经有了。**
   他写「要补的两步」：`acp/src/chat.rs` 的 `ChatOptions` 加 `env`，`daemon/src/chat.rs::start_inner` 传 `CHAT_ID_ENV`。这两步现在都在：`crates/acp/src/chat.rs:170 pub env: Vec<(String, String)>`、`crates/daemon/src/chat.rs:1279 env: vec![(ruagent_core::CHAT_ID_ENV.to_string(), id.to_string())]`，并且新测试 `chat_origin_env::the_conversations_own_agent_process_carries_its_conversation` 在本次 `test --workspace` 里通过；我的真进程读数也见到了（`env RUAGENT_CHAT_ID=<chat> pid=46804 child=<chat>`）。差异原因是时间差（报告写在 lead 补这一处之前），不是错误读数；但**报告没有更新，照它读会得出「P6 只做了一半」的错误结论**。
   诊断这条时的字节差：他表里 11 个哈希，产品文件是 `core/src/{run,lib}.rs`、`acp/src/run.rs`、`daemon/src/runs.rs`、`mcp/src/lib.rs` —— **既没有 `crates/acp/src/chat.rs`，也没有 `daemon/src/chat.rs`**；我读到的 `acp/src/chat.rs` 是 `b277bb1…`（他的表里没有这一行）。

2. **他说「两件都成立，各有活读数」—— 成立，但少了一个前提条件：P5 只对「有 origin 的 cluster」生效。**
   `runs.rs:2208` 在自愈之前就 `return`（无 origin = 面板/CLI 派发，没人等，所以不花钱也不查世代）。他的两个测试都传了 `origin_chat_id`（`heal_generations.rs:269/301/422`），所以他的读数不假，但**从他的报告读不出「无 origin 的活一次自愈都没有」**，而这正是用户「重派」场景里最容易踩的边界（从面板重派 = 没有自愈；从会话重派 = 每代一次）。

3. **规格 vs 实现（不是实现者之间的分歧，是「做到的」和「要求的」的差）**：
   - 规格 A4/§P4.3「补发只在一处发起」：实现有 2 处（resume + turn-end 兜底的 outbox 读取），并因此产生自动连锁（§5）。剧本 A4 我判红。
   - 规格 §P4.1 的 `run_id`「哪次 run 促成的汇报」：**实现从不写它**。三处写库都传 `None`（`chat.rs:1514/1546/1576`），而 `wake_in_background` 手上就有 `run.id`（`runs.rs:2460` 起），只用于 WARN。我的读数：私有库里 8 行 outbox，`run_id IS NOT NULL` 的行数 = 0（`group by run_id` 只有 `(None, 8)`）；API 返回 `"run_id": null`。所以「这条汇报是哪个 run 促成的」今天查不到 —— 规格说它是可查的。
   - 规格 §P4.5 要求「谁改这一行，谁必须交那条测试」：测试在（`chat_reports_outbox::a_restart_heals_…`），但它用进程内函数模拟重启；**真重启的证据是我这次补的**（§2 B4）。

4. **迁移测试的 head pin 被挪走（合规但值得记）**：`the_origin_chat_column_…` 里原本 `SCHEMA_VERSION == 32` 的硬断言改成了 `MIGRATIONS.len() > v32 + 1`，33 的硬 pin 挪到新测试。这是规格 §6 要求的形状；副作用是「0032 的测试不再钉 head」，将来若有人把 0033 删掉而把 pin 留在别处，这个测试不会红。

5. **实现者的 `test --workspace` 报 964 passed / ok 91 / FAILED 0，我读到 965 / 91 / 0。** 差 1 正好是 lead 补的会话侧那条测试（他的报告里 `chat_origin_env` 只有 1 个用例，现在是 2 个）。这是一次**一致**的交叉验证，不是分歧，记在这里因为它正好证明了「报告读数属于哪份字节」这件事。

6. **`crates/daemon/src/watchdog.rs:7-8` 的文档句现在是假的**（读代码发现，非本次改动引入但被本次改动打破）：「`Chat::send_prompt`'s only caller in the whole workspace is the `chat_message` HTTP handler」。现在 `deliver_or_queue` 也调用 `send_prompt`（`chat.rs:530`）。这不影响功能，但它是「定时器不碰 agent」这条论证的原文，而 P4 之后补发本身就是一个能驱动 turn 的非打字路径。

---

## 7 我没能确认的（推断 ≠ 读数）

1. **`target/debug`/团队 target 的 clippy 是缓存读数**。我没有跑 `-CleanFirst`（会清空队内共享 target dir）。若要求「刚 lint 过」的读数，需要在声明的窗口里重跑。
2. **`pending_reports: null`（未知）这一态没有活读数**。代码路径是「outbox 计数读失败 → 全部 null」（`chat.rs:2430`），我没有办法让 SQLite 读失败而不破坏环境。C3 的 null 态同样只有代码 + 类型检查。
3. **面板的未读数徽标没有渲染读数**：没有浏览器、没有 DOM 测试，也没有单测钉 `pendingReportBadge` 的三态；`npm run build` 只证明它能编译、i18n 键配对。
4. **旧行进代码（`dispatch_epoch` 缺失）我只有「跑别人测试」的读数**，没有自己构造旧行探针（那需要往活动库里写行，会违反「只读产品代码」的纪律；我选择了不写）。
5. **并发派发的 epoch 竞态未测**（同一 task 几乎同时两次派发可能拿到同一个 epoch ⇒ 共享一个自愈预算）。实现者自己在报告 §⑦d 也标了未测。方向是保守的（少自愈，不多花钱）。
6. **agent → 平台 MCP server 这一跳仍是继承**：我在真进程上只测到「agent 进程有变量」和「agent 的**子进程**（`cmd /c echo %VAR%`）有变量」两层；MCP server 是 agent 自己 spawn 的，中间那一跳是 OS 继承，不是我的读数（`crates/mcp/tests/dispatch_origin.rs` 测的是同一个进程读环境后往 wire 上写）。
7. **真 harness（claude/codex 等）下的 turn 成本与延迟未测**，全部用 `mock-agent`。所以 F 节的「每条一个付费 turn」是**机制推断 + mock 计时**（3 条 1.1 s），不是账单读数。
8. **`8787` 上的常驻实例**：我只确认收尾时它仍在监听，没有对它做任何读写。
9. **迁移的「可重放」**：全新库（1→33）、手工 32 库（→33）我都实测了；「同一脚本在已应用库上再跑一次是 no-op」这一半只有仓库测试的读数（它自带负控）。

---

## 8 环境、工件与收尾（**没有删除任何被引用的东西**）

私有产物全部在 `C:\Users\19410\AppData\Local\Temp\rnl-verify\`（**保留，未删**）：

| 路径 | 内容 |
| --- | --- |
| `bin\ruagent.exe`、`bin\ruagent-mock-agent.exe` | `cargo rustc … -- -o <私有路径>` 构建的二进制（不覆盖常驻实例的 `D:\rust_cache\debug\ruagent.exe`） |
| `root-a\`（8795） | B1/B2/B3/B5/B6/C 的私有根；`logs\daemon.log`、`data\ruagent.db`、`data\transcripts\*` |
| `root-b\`（8796） | B4 的真杀/重启私有根；`logs\daemon.log` 含 `runs marked interrupted by restart` 与 boot 自愈行；`data\daemon.exit` 保留被杀形态 |
| `root-c\`（8797） | A1 的 v32 旧库升级探针 |
| `daemon.ps1`、`b2-cap2.ps1`、`b4-restart.ps1`、`dbread*.py`、`make-v32-db.py` | 我用的探针脚本（可复跑） |
| `test-workspace.txt`、`clippy.txt`、`panel-build.txt`、`mcp-dispatch-origin.txt`、`b4-out2.txt` | 各门禁与关键场景的原始输出 |

收尾方式：三个私有 daemon 都按 **PID** 停（`stopclean` → `<root>/data/daemon.stop`），各自写了 `exit reason=stop-request`（root-a/b/c 均为 `alive=False`）；没有按进程名或端口清扫；临时根与日志**保留**。`8787` 的常驻实例全程未碰。

纪律：只读产品代码（唯一写入是本报告），没有提交、没有 push、没有打 tag。






