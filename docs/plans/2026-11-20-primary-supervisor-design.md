# 单一入口 + 主智能体监督：设计与分工

日期：2026-11-20 · 基线：`d30931b` · 状态：**已与用户对齐目标形态与唤醒语义，待实现**

## 0 用户要的是什么（原话，不要改写）

> 「我只需要和一个主 agent 对话，所有事情都能由这个主 agent 进行管理分发监督，我绝大多数情况只需要和一个主 agent 交互即可。因为我感觉每次在不同窗口和多个 agent 交互太累了。」

对形态的两次澄清，逐字保留：

1. **入口**：「聊天是可以作为唯一入口，但是也允许直接去面板发布任务或者直接和干活的 agent 对话」
   → 聊天是**默认**入口；面板发任务、直接找某个 agent 是**显式 override**，保留而不是废除。
   这与 `primary-agent.md` 已经写下的「一个答案 + 显式 `agent` 覆盖」是同一件事，所以本设计**不推翻**既有契约，只补它缺的那半截。

2. **唤醒**：「出问题优先自己解决，或者只是一个临时结果，那就直接继续监督干活的 agent 完成任务，如果是最终结果，还是需要汇报的。比如用户问你一个问题，这个问题子 agent 给出了答复或者解决了，那么你肯定要向用户汇报的」
   → 三条语义，必须同时成立：
   - **中间结果不打断用户**，主 agent 自己接着推进；
   - **失败先自愈**（自己解决），解决不了才开口；
   - **最终结果必须汇报**——而且是**真的回一句**，不是静默追加一条记录。

## 1 现状（读数来自 `docs/design/async-entry-audit.md`，基线 `d30931b`）

| 层 | 状态 | 证据 |
| --- | --- | --- |
| 执行（run 后台推进） | **完整** | `runs.rs:776` `tokio::spawn`，返回时仍 `spawning`；`e2e_daemon.rs:1624/1632` 断言此事 |
| 交付（结果回到眼前） | **拉取式** | 无推送、无未读、**无 `GET /api/v1/runs`**（路由表只有 `/runs/{id}`，`api.rs:167`） |
| 派发（主 agent 派活） | **有** | `mcp/src/lib.rs:670-703` → `POST /tasks/{id}/runs` |
| 认领（谁来做） | **没有** | `0001_init.sql` 头注释 `claim/lease … arrive with the v2 task board` |
| 唤醒（主 agent 被叫醒） | **没有** | `Chat::send_prompt` 全仓唯一调用者 `chat_message` ⇒ 任何定时器都驱动不了一轮 |
| 失败可见性 | **没有** | `update_task_status` 4 个调用点，只有成功写 `Done`（`runs.rs:1797`）⇒ run 死了 task 永远「进行中」 |

外部对照（`docs/design/superagent-mechanics.md` §6）：**分工必须由结构保证，不能靠提示词**——单 Session + subagent，中性提示下 5 次运行**一次都没委派**。所以本设计里的每一项都是结构，不是提示词要求。

## 2 核心概念：任务簇（cluster）

一个 **cluster** = 一次对话里为同一件事派出的一组 run。键有两层：

- `runs.task_id` —— 已有。同一个 task 的多次 dispatch（重试、换 agent、fanout 分支）天然同簇。
- `runs.origin_chat_id` —— **新增**。回答「这活是哪次对话派的」，今天不存在。

**聚合判定**（这是「中间/最终」的唯一判据，不许别处再判一次）：

```
簇内仍有非终态 run  → 中间结果，不唤醒（主 agent 继续监督）
簇内全部终态        → 最终结果，唤醒并汇报
簇内出现失败        → 先自愈（见 P2.3），自愈耗尽才唤醒
```

## 3 四件事

### P0 失败必须回写 task 状态（修的是「看板骗人」）

**问题**：`runs.rs:1791-1802` 的收尾只把 `Completed` 映射到 task `Done`。run 变成 `Failed`/`Cancelled`/`Interrupted` 时 task 停在 `InProgress`，看板显示「进行中」，人无从知道它死了。

**规格**：
- 终态映射必须穷举，不是 `if completed`：
  - `Completed` → task `Done`
  - `Failed` / `Interrupted` / `Cancelled` → task **回到 `Pending`**（可被再次 dispatch），并在 task 上留下「上一次为什么没成」的读数（复用既有 `error`/`result` 载体，不新造字段）
  - 同一 task 有多个 run 时（fanout），**只有全部终态才写 task 状态**；否则保持 `InProgress`（与 P2 的聚合判定同一把尺子）
- **不许静默**：回写失败（DB 错）要 `WARN` 并带上 task_id / run_id / 目标状态。

### P1 回执读数层（`GET /api/v1/runs`）

**规格**：
- `GET /api/v1/runs?status=&since=&limit=&offset=` —— 复用文件里既有的分页信封（照 `list_tasks`，`api.rs:3250-3277`）。
  - 默认按 `updated_at desc`；`since` 是 unix ms 或 RFC3339（二者都收，与仓库其它端点一致）。
  - 每行至少：`id`、`task_id`、`task_title`、`agent`（**名字**，不是 registry id——`task_dispatch` 的回复已经立了这个先例）、`status`、`finished`、`updated_at`、`error`（有才给）。
  - 未知 status 值 → 400（typo 不能看起来像成功）。
- 面板：`Home.tsx` 增加「最近完成 / 最近失败」两块（照 `Home.tsx:247-302` 的六端点合并模式）。**空态要说「还没有」，不是渲染成 0 条**（仓库已有 `known` 三态的先例：未知不等于零）。
- 不加迁移、不动 chat、不动 run 推进。

### P2 唤醒闭环（解药）

**P2.1 归属**
- `runs` 加 `origin_chat_id TEXT NULL`（一条 migration；**必须加入既有的 25/25 重放测试**，那条测试是硬的）。
- `POST /api/v1/tasks/{id}/runs` 与 MCP `task_dispatch` 允许带 `origin_chat_id`；不带 = `NULL` = 这是面板/外部派发的活，**不唤醒任何人**（面板派发的活本来就有人在看）。

**P2.2 唤醒入口**
- `ChatManager` 加 `notify(chat_id, text) -> Result<()>`：内部复用 `send_prompt`，**带 `generating` 检查**（同一 chat 同时只允许一轮在飞；`generating` 已存在，`chat.rs:138`）。若正在飞，**排队一条**而不是丢弃（丢弃 = 静默丢汇报，正是本设计要消灭的失败模式）。
- 唤醒消息必须带「平台事件」标记，不能被当成用户说的话——照 `distill.rs:1515` 的 `platform_prompt()` 先例。

**P2.3 完成钩子 + 聚合 + 自愈**
在 `supervise` 收尾（`runs.rs:1791-1802`，`update_run` 之后、`Finished` 广播之前）：
1. 若 `origin_chat_id` 为空 → 什么都不做（错了，见 P2.1）。
2. 查同簇（同 `task_id`）是否还有非终态 run → **有则返回**（中间结果，不唤醒）。
3. 全部终态后：
   - 全部成功 → 唤醒一次，内容 = 这次簇的结论摘要（哪个 agent、任务标题、关键结果片段）。
   - 有失败 → **先自愈**：对失败的 run 调既有 `retry_run`（`runs.rs:934`），上限 `N=1` 次且**换一个不同的 agent**（若注册表里有可用者；否则原 agent 重试）。自愈耗尽 → 唤醒并明说「我试了 N 次，最后一次失败原因是 …」。
   - 自愈本身必须**留痕**：写一条 run 事件（既有事件流），否则用户看不到「它自己试过」。
4. **风暴抑制**：一次 fanout 5 个 run 只能唤醒 1 次——由第 2 步的聚合天然保证，**不要再加计时器**（两个判据 = 两个真相）。

**P2.4 不许静默的失败模式**（每一条都要有测试或可读日志）
- `notify` 失败 → `WARN` 带 chat_id + run_id，且**不能让 supervise 崩**。
- 唤醒时 chat 已被删 → 记一条，吞掉，不重试。
- 自愈时注册表里没有别的 agent → 明说「只有一个可用 agent」，不假装换过。

### P3 watchdog 从「存活」升级为「发起一轮」

**规格**：
- `policy.toml` 加 `[watchdog]`：`enabled`（**默认 false**）/ `interval` / `agent` / `prompt` 或 `task`。
- 挂**既有 capability plane**（`crates/daemon/src/capability.rs` 的 `CapabilityId`），不是只靠 policy 一行——它会自己花 token。
- tick 直接调 `start_run`，**不要 POST 自己的 HTTP**（那是把 daemon 当外部客户端，且会绕开并发闸门）。
- 上一轮没结束就跳过本轮（用 `list_runs_by_status` 判，幂等）。
- 落成 task（`create_task` + `start_run`），这样它自动出现在 P1 的回执里——免费复用。

## 4 分工与文件所有权（并行，不许越界）

| 成员 | 范围 | **独占**文件 | **禁碰** |
| --- | --- | --- | --- |
| `supervise` | P0 + P2 | `crates/daemon/src/runs.rs`、`crates/daemon/src/chat.rs`、`crates/store/src/migrations.rs`、`crates/store/src/lib.rs` | `api.rs`、`panel/**`、`policy/**` |
| `receipts` | P1 | `crates/daemon/src/api.rs`（路由表 + 新 handler）、`panel/src/views/Home.tsx`、`panel/src/api.ts`、i18n 词条 | `runs.rs`、`migrations.rs`、`policy/**` |
| `watchdog` | P3 | `crates/policy/src/lib.rs`、`crates/daemon/src/lib.rs`、`crates/daemon/src/capability.rs` | `runs.rs`、`api.rs`、`panel/**` |

`store/src/lib.rs` 归 `supervise`（它要加列与簇查询）；`receipts` 若需要一个 run 列表 reader，**必须**把函数签名提给 `supervise` 由它加，或者用已有的 `list_runs_by_status` + 在 api 层过滤。**两个人在同一个文件里加函数 = 必冲突。**

## 5 门禁与验收（每个人都跑，不合并成一次）

```
cargo fmt --all --check
scripts/cargo-team.ps1 clippy --workspace --all-targets -DenyWarnings   # PROTOC 必须先设
scripts/cargo-team.ps1 test --workspace
cd panel && npm run build                                                # 含 i18n-check + tsc
```

- `PROTOC="$env:USERPROFILE\.protoc\bin\protoc.exe"`，`NO_PROXY='127.0.0.1,localhost,::1'`（本机系统代理是坏的）。
- 迁移必须过既有的「25/25 重放」测试；加一条就变 26，**数字要跟着改**。
- 面板改动必须**双语**（zh/en），i18n-check 会抓漏。
- **端到端必须有一条**：`POST /tasks/{id}/runs` 带 `origin_chat_id` → run 失败 → 自愈一次 → 仍失败 → **chat 里真的多了一条平台事件**。没有这条，「主 agent 会汇报」就是声称。

## 6 已知风险（不许在实现时重新发现）

- **唤醒会在用户不在时花 token**。这是用户明确要的（「你肯定要向用户汇报」），但必须由 P2.3 的聚合保证**不是每 run 一次**。
- **`generating` 是内存态**，daemon 重启即丢；排队中的唤醒会随进程消失。第一版接受这个损失，但必须在日志里说出来（`WARN`），不能装作没发生。
- **`RunStatus::WaitingPermission` 是空态**（全仓 8 处命中无一处写入）。本设计**不修**它，但 P0 的终态穷举必须**把它算作非终态**，否则聚合判定会提前唤醒。
