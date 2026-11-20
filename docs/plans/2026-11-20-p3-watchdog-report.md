# P3 watchdog：从「查存活」到「发起一轮」——实现报告

日期：2026-11-20 · 成员：`watchdog` · 基线：`d30931b` + 工作树改动（未提交）
规格：`docs/plans/2026-11-20-primary-supervisor-design.md` §P3
读数来源：`docs/design/async-entry-audit.md` §2.6 / §4 第 2 步

---

## ① 一句话结论

平台现在有了**第一条真正通向 agent 的定时器**：`[watchdog]` 每到点就在进程内调
`RunManager::start_run` 起一轮 run，落成一个 `TaskCreator::Schedule` 任务，因此它自动出现在
看板和 P1 的回执列表里，不需要第二套「watchdog 历史」。默认关闭，两把闸门（policy + capability
plane）都默认关。

---

## ② `[watchdog]` 配置形状与默认值

`crates/policy/src/lib.rs`（`WatchdogConfig` / `WatchdogRound` / `parse_watchdog_interval`）：

```toml
[watchdog]
enabled = false        # 默认 false —— 硬要求：它会自己花用户的 token
interval = "15m"       # 缺省 = 15 分钟（DEFAULT_WATCHDOG_INTERVAL）；单位必须写
agent = "dsh"          # 可选，不给就走既有 routing cascade
prompt = """..."""     # 与 task 二选一
# task = "01J..."      # 或每轮 dispatch 一个已存在的 task
```

| 字段 | 类型 | 缺省 | 语义 |
| --- | --- | --- | --- |
| `enabled` | bool | `false` | 关 = `round()` 返回 `Ok(None)`，什么都不校验、什么都不做 |
| `interval` | string | `"15m"` | `90s` / `15m` / `2h`；**单位必须写**，裸数字被拒；`0` 被拒 |
| `agent` | string? | `None` | 不在 registry / 被 disable → 报错，**不静默换人** |
| `prompt` | string? | `None` | 每轮的 prompt |
| `task` | string? | `None` | 每轮 dispatch 这个 task 的**当前 intent**（改了 task，下一轮就变） |

**为什么 `interval` 是带单位的字符串**：仓库里没有任何 duration 的配置先例（查过 `interval`
全仓只有 `tokio::time::interval` 一处），而裸数字在这里是**花钱的歧义**——`15` 到底是 15 秒还是
15 分钟，差 60 倍。所以 `parse_watchdog_interval` 拒绝裸数字并要求 `s`/`m`/`h`，`0s`/`0m` 单独
报「tight loop」错误。没有加「最小 5 秒」之类的地板：那是没人要求过的新策略。

**非法配置的处置**（`enabled = true` 时才校验）：
- `interval` 为 0 / 无单位 / 非数字 → `WatchdogConfigError::{ZeroInterval, IntervalWithoutUnit, IntervalNotANumber}`
- `prompt` 与 `task` 都缺（含 `prompt = "   "` 这种空白）→ `NoTarget`
- 两个都给 → `BothTargets`

三种都带**可读的拒绝理由**（错误文案本身就是补救说明），daemon 侧一律 **WARN 并整条关闭**，
绝不带半个配置运行；`enabled = false` 的 section 不校验（关着的开关不会误动作）。

---

## ③ capability plane 的落地方式（复用哪套、id 叫什么、怎么开）

**复用**：`crates/daemon/src/capability.rs` 既有的 `CapabilityId` + `SPECS` 注册表 +
`CapabilityPlane::gate`，一行都没新造开关机制。

- id：`CapabilityId::Watchdog` / 字符串 `"watchdog"`（**追加在 `ALL` 末尾**，既有 11 行的顺序
  与字符串一个没动）
- 行：`tier: Llm`、`default_enabled: false`（L4：会花 token 的必须默认关）、
  `new_in_this_increment: true`（该字段的操作含义就是「新 ⇒ 必须默认关」）、`options: &[]`
- 闸门表达式：`gate(Watchdog, legacy = true)`。`legacy` 恒为 true 是**构造性的**——`Watchdog`
  这个值只有 `[watchdog] enabled` 说了 yes 才会存在，所以 policy 的 `enabled` 就是它的
  pre-plane 开关（L1：没有 `[capabilities]` 表时闸门放行，所以「只写 `[watchdog] enabled = true`」
  就够）

**怎么开**（三种，效果等价）：

1. `policy.toml` 里什么都不加 `[capabilities]`，只写 `[watchdog] enabled = true`（legacy 放行）；
2. `policy.toml` 加
   ```toml
   [capabilities.watchdog]
   enabled = true
   ```
   —— 只要 `[capabilities]` 表**存在**，llm 档就按注册表默认值（关）走，必须显式打开这一行；
3. `PUT /api/v1/capabilities` 带 `confirm_cost: true`（llm 档的既有成本闸门，`capability.rs`
   现成的 `unconfirmed_llm_enable`），不用重启。

**「被能力闸门挡住」不是静默**：`gate_refusal()` 返回的是**一句话**而不是 bool，日志里逐字给出
`[capabilities.watchdog] enabled = true` 这段补救配置，并在闸门开/关**切换时**各说一次（不在
每个周期重复刷屏）。为什么必须说出来：`[capabilities]` 表因为任何**别的**原因存在时，每一行都
回到注册表默认值，`watchdog` 是 llm 档 = 默认关 —— 用户看到的会是一个「看起来开了却永远不响」
的定时器。

`DEFAULT_POLICY_TOML`（`crates/daemon/src/config.rs`）加了注释形态的 `[watchdog]` 与
`[capabilities.watchdog]` 示例，用户不用翻代码就知道字段长什么样；既有的
`default_policy_toml_ships_capabilities_commented_out` 断言不受影响（表仍是注释掉的）。

---

## ④ tick 的位置，以及它为什么拖不死主循环

**位置**：`crates/daemon/src/lib.rs` 里**独立的一条 `tokio::spawn` 循环**，摆在 MCP health
`spawn_loop` 之后、`api::router(state)` 之前（即 `state` 建好之后，因为它要拿
`state.chats.capabilities_handle()` 这一个共享 plane）。体量对照：KB 扫描那条是
`loop { ...; sleep(60s) }`，我这条同构，只是**周期来自 policy**、动作是 `start_run`。

**为什么独立成一条，不塞进 KB 扫描的循环体**：两个功能共用一个 `loop` 就共用一个 `?` 和一种
失败形态，以后没人知道该改哪条。

**为什么不会拖死 daemon**：

1. 循环体里**没有 `?`**：`tick()` 返回 `Result<Outcome, String>`，`run()` 用 `match` 接住，
   `Err` 只落一条 `WARN`（带原因）然后继续睡。这是这一单最容易写出来的 bug，被结构挡住了。
2. `tick()` 的每一步失败都变成 `String`（`task_for` / `inflight` / `start_run` 各自 `map_err`）。
3. `sleep` 在 `tick` **之前**：daemon 每次重启（launcher 的 `watch` 在健康检查失败时就会重启）
   都不会花掉一轮 —— 这是刻意的，代价是「开启后要等一个周期才看到第一轮」，写进日志与报告。
4. 幂等：用既有的 `list_runs_by_status` 走 4 个非终态（`Queued/Spawning/Running/
   WaitingPermission`），只看属于本轮 task 的 run；命中就跳过并**明确说**「跳过，因为上一轮还在
   跑」（带 run_id + status）。`WaitingPermission` 算非终态（设计 §6 的硬要求）。
5. 并发闸门没被绕开：走的是 `RunManager::start_run` 的进程内入口，按 harness 的信号量/排队
   上限照旧生效；HTTP 自调用那条路既会把自己当外部客户端，也会绕开它，所以没走。

**每轮谁来做**（`watchdog::target`）：
`[watchdog].agent` 指名 → 用它（不在 registry / 被禁用 = 报错，不换人）；不给 → 走既有 cascade：
task 的 pin → `routing.toml` 规则 → `routing.toml` default → `primary = true` 的卡 → 第一个
enabled 且不是权限 approver 的卡。

---

## ⑤ 给 `supervise` 的交接说明

**P3 本身不需要 `supervise` 改任何东西。** `start_run`
（`runs.rs`，当前签名 `self: &Arc<Self>, task: &Task, agent_name: &str, prompt: String,
mcp_servers: Vec<McpServer>, workspace_spec: WorkspaceSpec, launch: RunLaunch) -> Result<Run>`）
本来就是**可内部调用**的：没有 axum extractor，`Arc<RunManager>` 上直接调即可（autoref 到
`&Arc<Self>`），`RunLaunch { routed: Some(decision), ..Default::default() }` 照旧构造，
`origin_chat_id` / `self_heal_of` 两个新字段吃 `Default`，所以 P2.1/P2.3 落地后我这行代码不用改。

**但 P0 的启动清扫那一半需要过一次可见性**（已由 lead 落地）：`runs::finish_cluster` 原本是
module-private 的 `async fn`，`lib.rs` 的启动清扫要复用同一个判据，唯一改动是

```diff
-async fn finish_cluster(mgr: &Arc<RunManager>, task_id: ruagent_core::TaskId, run: &Run)
+pub(crate) async fn finish_cluster(mgr: &Arc<RunManager>, task_id: ruagent_core::TaskId, run: &Run)
```

行为零变化（签名/返回 `()`/内部判断全不动），只是让同 crate 的 `lib.rs` 能调它。**没有**任何已
公开的入口能做这件事（`runs.rs` 的 25 个 `pub fn` 里没有写回 task 状态或触发唤醒的），所以
「在 `lib.rs` 里重写一份」是被明确否掉的路：那是第二个判据、两个真相。

**唯一一条「遗留债」，是给 `api.rs`（`receipts`）的，不是给 `supervise` 的**：routing cascade 的
level 4/5（`primary = true` 的卡、跳过权限 approver 的兜底）在 `api.rs` 里是
`primary_decision` / `default_route_agent`，**是 private**。P3 需要同一套判断，而 `api.rs` 在
本增量归别人，所以 `watchdog::target` 现在**自带一份 level 4/5**（level 1–3 用的是
`ruagent_orchestrator::route`，与 `api.rs` 同一个调用，不是副本）。合并方式二选一，精确到签名：

- 最小改动：把 `api.rs` 里这两个函数提成 `pub(crate)`：
  ```rust
  pub(crate) fn primary_decision(
      cards: &[ruagent_core::AgentCard],
      approver: Option<ruagent_core::AgentId>,
  ) -> Option<(String, ruagent_core::RoutingDecision)>

  pub(crate) fn default_route_agent(
      cards: &[ruagent_core::AgentCard],
      approver: Option<ruagent_core::AgentId>,
  ) -> Option<ruagent_core::AgentCard>
  ```
  然后 `watchdog::target` 的 level 4/5 两段换成对它们的调用（约 −40 行）。
- 或者更彻底：把 `routing_decision_with` / `primary_decision` / `default_route_agent` /
  `with_default_rationale` 整体搬到 `crates/daemon/src/routing.rs`（新文件，谁都不占），
  `api.rs` 与 `watchdog.rs` 都只调这一份。**两个答案就是两个真相**，`AGENTS.md` 已经记过一次
  同样的形状（`McpConfig::spawn_command` vs `entry.command`）。

在那之前，两处的**排序**是一致的，且 `watchdog::target` 的 unit test 把这条排序逐级钉住了
（pin > rule > default > primary > 第一个 enabled 的非 approver）。

**给 P1（`receipts`）免费的好处，附一句提醒**：watchdog 的每一轮都是一个普通 run，`task_title`
来自它的 task（`watchdog: <prompt 首行>`），所以回执列表里会自然出现。`prompt` 模式**只创建一个
常驻 task**（不是每轮一个），理由是避免看板每 15 分钟多一行；每轮的历史在 run 上。

---

## ⑤b 启动清扫的回写（P0 的 boot 半边）

同一单里补掉的一处一致性漏洞（`supervise` 的最终报告指认，文件在我的范围）：

**缺口**：`lib.rs` 的启动清扫把非终态 run 改成 `Interrupted` 之后**不经过 `finish_cluster`**，
于是「重启那一刻正在跑的 task 永远停在 `InProgress`」—— P0 刚修好的「失败必须回写 task」被重启
路径绕过了。今天用户重启一次就能看到这个不一致。

**做法**（`lib.rs`，拆成两个可测的半步，顺序与真 supervisor 一致：先落 run，再判簇）：

```rust
// 1) 只改 run，返回被改的那些（只要 Db）
async fn interrupt_live_runs(db: &Db) -> Result<Vec<Run>>

// 2) 每个 DISTINCT task_id 调一次 runs::finish_cluster（只要 Arc<RunManager>）
async fn finalize_interrupted_clusters(mgr: &Arc<RunManager>, interrupted: &[Run])
```

- **聚合语义一致**：一次重启让同一个 task 的多个 run 同时变 `Interrupted` 时，只对那个 task
  调**一次** `finish_cluster`（`BTreeSet` 去重）—— 一次 fan-out 被重启杀掉不能按成员数回写一次
  一次。是否「全部终态」、映射到 `Done`/`Pending`、要不要自愈、要不要唤人，全部由 `finish_cluster`
  一个判据回答，我这里一个 status 都不看。
- **位置**：清扫的「标记」半步仍在原处（`mgr` 还不存在），「回写」半步挪到 `mgr` 建好**并且
  live capability plane 装好之后**（`mgr.set_capabilities(...)` 之后），这样从 boot 触发的自愈
  重试是在用户真实的 capability 配置下组装 prompt 的。
- **不许静默**：`finish_cluster` 内部对「读簇失败 / 找不到 task / 回写失败 / 唤醒失败」逐个
  `WARN` 且都带 `task_id` + `run_id`；我这一侧只加两条聚合日志（`WARN count=… tasks=…`、
  `INFO tasks=… runs=…`）。`interrupt_live_runs` 保留它原有的 `?`（数据库连 run 都标不死是
  boot 级故障），**但回写那一半没有任何 `?`** —— 一个 task 的回写失败绝不能让 daemon 起不来。
- **⚠ 重启会花钱，写完就接受**：`finish_cluster` 在「全部终态 + 有 `origin_chat_id` + 簇内没自愈
  过」时会自愈一次（偏好另一个 agent）。`Interrupted` 是 `is_retryable()`，所以**重启后走 chat
  派发过的活会自动重起一轮**；面板/CLI 派发的（`origin_chat_id == None`）只回写 `Pending`、
  不重试、不唤人。这是 lead 明确拍板的（用户原话「出问题优先自己解决」），而且这句话**写在
  `lib.rs` 调用 `finish_cluster` 紧邻的注释里**，不只写在报告里 —— 报告没人会在重启前读。
- **测试**（`lib.rs::restart_sweep_tests`，3 条，真 `Db` + 真 `RunManager`）：
  1. `a_restart_puts_an_interrupted_task_back_to_pending`：`running` run + `in_progress` task →
     标记半步后 run=`Interrupted` 而 task **仍是** `in_progress`（**把缺口本身断言下来**）→
     回写半步后 task=`Pending`，run 的 `error`（「为什么上次没成」）还在；
  2. `a_cluster_killed_by_a_restart_is_finalized_once`：3 个 run 同属一个 task → 标记 3 个 →
     回写后 task=`Pending`；再调一次仍是 `Pending`（幂等，不会第二次写）；
  3. `a_clean_restart_finalizes_nothing`：没有非终态 run → 什么都不动，`Done` 的 task 不受影响。

---


## ⑥ 门禁读数

全部在 Windows、`PROTOC=C:\Users\19410\.protoc\bin\protoc.exe`、
`NO_PROXY=127.0.0.1,localhost,::1` 下取得，走队内脚本（有编译锁，会排队）。

| 命令 | 读数 |
| --- | --- |
| `scripts/cargo-team.ps1 clippy --workspace --all-targets -DenyWarnings` | **exit=0**（最终字节，`elapsed=13.8s`） |
| `scripts/cargo-team.ps1 test --workspace --no-fail-fast` | **exit=101**：`test result: ok` **86** 行 / `FAILED` **1** 行；`passed` 合计 **951**，`failed` 合计 **1**，`ignored` 合计 **22**（87 个 test target，`--no-fail-fast` 保证没有一个 target 被跳过） |

唯一那条红**不是本单的**，也不在本单的文件里（上一轮是 `supervise_wakeup`，它现在绿了，
`4 passed`；这一轮换了另一个同样不在我范围里的）：

```
-p ruagent-daemon --lib   api::tests::the_topology_dispatches_carry_the_origin_onto_every_run_they_create
  assertion `left == right` failed: the additive field must not turn a working fan-out into a 4xx:
  resolving spawn command for `alpha`: invalid agent command
  `program `mock-agent` for agent `alpha` not found on PATH`    left: 500   right: 200
test result: FAILED. 289 passed; 1 failed; 1 ignored
```

这是 `api.rs` 的新测试（P2.1 `origin_chat_id` 的 topology 派发），它用**裸命令名** `mock-agent`
当 agent 的 command，而这台机器/CI 的 PATH 上没有 `mock-agent` 可执行文件 —— `e2e_daemon.rs`
的做法（`env!("CARGO_BIN_EXE_ruagent-mock-agent")` 复制一份、写绝对路径）才是能跑的形状。
属于 `receipts` 的测试夹具，我没有动。**注意它会让 `daemon --lib` 这个 target 整条红**，所以
`daemon --lib` 的绿读数要等它修好才能取到。

本单相关的 target 全绿：

| target | 读数 |
| --- | --- |
| `-p ruagent-policy`（4 条新增 watchdog 配置测试） | ok. 13 passed; 0 failed |
| `-p ruagent-daemon --lib` | 289 passed / **1 failed**（上面那条 `api::tests`，非本单）；本单的 8 条 `watchdog::tests::*` 与 3 条 `restart_sweep_tests::*` 逐条 ok |
| `--test capabilities` | ok. 12 passed; 0 failed |
| `--test capability_defaults` | ok. 5 passed; 0 failed |
| `--test knowledge_api` | ok. 16 passed; 0 failed |
| `--test supervise_wakeup`（上一轮那条红，`supervise` 已修） | ok. 4 passed; 0 failed |

**过程中修掉的两处「数字 pin」**（都是本单加第 12 个 capability 引起的，已按「断言关系而不是
数字」改写，下一个人加 capability 时它自己会红）：

1. `crates/daemon/src/capability.rs` `row_and_response_json_are_the_api_contract`：
   `Some(11)` → `Some(CapabilityId::ALL.len())`。
2. `crates/daemon/tests/knowledge_api.rs` `the_panel_sees_every_declared_key_and_a_write_does_not_materialize_defaults`：
   `Some(11)` → `Some(CapabilityId::ALL.len())`（该文件不在我的独占范围，成因是这批改动，按
   lead 的指派只改断言、不重排结构）。
3. `crates/daemon/tests/capability_defaults.rs`：`DESIGN_IDS` 加到 12，并把 `watchdog` 加进
   「默认配置不得启动任何新工作」与「llm 档默认关闭」两条断言 —— **默认关闭是硬要求**。那条断言
   的原文是 `assert!(!plane.enabled(id), "{} must be off in the default configuration", id.as_str())`，
   读的是**默认值**（`[capabilities]` 表 ABSENT ⇒ 注册表 `default_enabled: false`），
   **不是**「断言某个函数没被调用」。



---

## ⑦ 没做到 / 没覆盖的（照实写）

1. **端到端（真起一个 ACP agent 的 run）没有测试。** 我已有的三条必需测试都落在
   capability/DB 层的**单元级**：配置解析（policy crate）、幂等（真 `Db`，判定函数级）、
   闸门联动（`tick()` 级，断言「被挡住时一个 task/run 都不产生」）。**「一轮真的跑完并留下
   result」这条端到端没人测**：daemon 的集成测试（`tests/capabilities.rs` 等）都是手工拼
   `RunManager` + `api::router`，**不走 `serve_with_remote`**，所以我这条 tick 循环在测试里根本
   不会被 spawn；要覆盖它需要 mock-agent 的二进制路径（`CARGO_BIN_EXE_*` 只在本 package 的
   test target 里存在）或一个新的 dev-dependency + 一个能起 daemon 的 harness。这条我没做。
   替代证据：`a_blocked_tick_launches_nothing_and_a_permitted_tick_never_panics` 证明 tick 确实
   走到了 `start_run`（用一个不存在的 program 让 spawn 前置检查失败，错误作为 `String` 返回而
   不是 panic），并证明 task 半边已经落库。
   **最省的下一次补法**（已在纸上推过，未落地）：在 `crates/mock-agent/tests/` 新加一个
   test target（那里 `env!("CARGO_BIN_EXE_ruagent-mock-agent")` 可用），手工拼
   `Db + ChatManager + RunManager`（照 `e2e_daemon.rs:30-110`，但**不需要** HTTP 与
   `Knowledge`），然后 `Watchdog::from_config(..).tick()`：`--behavior echo` 跑完 → 断言
   run `Completed` 且 task `Done`；`--behavior slowreply`（10s，可取消）时立刻再
   `tick()` → 断言 `Outcome::Busy`，最后 `cancel_run` 收尾。
2. **`[watchdog]` 只在 boot 读一次**（与 `[distill]` 同款）。改 `prompt`/`interval`/`agent` 要重启；
   `[capabilities.watchdog]` 则是每轮读 live plane，`PUT` 立即生效。这个不对称是刻意的（config
   加载只有一处），但没有面板/接口去热改它。
3. **没有把「policy 开了但 capability 关了」加进 `GET /api/v1/capabilities` 的 `conflicts[]`。**
   那需要给 `capability::conflicts(distill_auto)` 加一个 `watchdog_enabled` 参数，而它的调用者
   `response()` 在 `api.rs`（本增量不归我）。现在的做法是在 daemon 日志里说（带补救配置），
   面板上还看不到这条冲突。
4. **`GET /api/v1/runs` 里 watchdog 的 run 和人的 run 无法一眼区分**，除了 `task_title` 前缀
   `watchdog: `。要把「谁派发的」变成机读字段是 `runs.origin_*` 那一列的地盘（P2），我没碰。
5. **每轮都在 `list_tasks(None)` 上扫一遍**（`prompt` 模式为了找常驻 task）。15 分钟一次、
   单用户规模下可以忽略，但它确实是 O(任务数)；正确的做法是 store 侧一个按 creator 查的
   reader（`store/**` 归 `supervise`，我没提需求）。
6. **`interval` 没有下限**（只拒 0）：`interval = "1s"` 是合法的，会每个 1 秒发起一轮。要不要
   加地板（例如 ≥10s）是**产品决定**，不是实现遗漏；我按「不发明没人要求的策略」处理。
7. **`workspace` 固定 `WorkspaceSpec::Fresh`**：watchdog 没有 `cwd`/`repo` 配置项，所以每轮都是
   一个全新隔离目录。想把 watchog 绑到某个仓库得先加配置字段（没做，规格没要求）。
8. **一轮卡在权限审批上会挡住后面所有轮**（幂等的直接后果）：run 停在 `WaitingPermission` 时
   `inflight` 命中，之后每轮都只打「上一轮还在跑」的 WARN，直到人裁决。这是刻意的（不叠加第二次
   花钱），但值得知道：watchdog 的「暂停」不是它自己的状态，而是审批台那条 ask。
9. **`crates/mcp/src/lib.rs` 的 `capability_set` 描述里写着「ENABLING an llm-tier capability
   (distill_session) …」**：现在 llm 档有两个 id 了（`distill_session` 与本单的 `watchdog`），
   那句示例只提了一个。属于文档漂移，但那个文件本增量有其他人在改，我没动 —— 交给后续一次小修。
10. **门禁**：`ruagent-daemon` 的编译读数一度被 `supervise` 正在改的 `runs.rs` 挡住
   （`retry_run_inner` 的 future 不 `Send`），那一刻我的文件已单独编译通过；最终读数见下。
11. **watchdog 的一轮不唤醒任何人**：`RunLaunch.origin_chat_id` 留 `None`（P2.1 的字段），因为
   这一轮没有任何对话派发它。P2.3 的唤醒是「谁派的活谁被叫醒」，而 watchdog 的活没有人派 —— 等
   主智能体成为前门之后，「watchdog 的结论要不要叫醒主 agent」才是可回答的问题（那需要的是
   `origin_chat_id` = 主 agent 的会话，属于 P2 的地盘）。现在这轮的结果只在看板/回执里。
12. **⚠ daemon 重启会花 token（已由 lead 拍板接受，写在代码注释里）**：启动清扫现在把被重启打断
   的簇交给 `finish_cluster`，而它会对「有 `origin_chat_id` 且簇内没自愈过」的失败簇**自愈一次**
   （偏好另一个 agent）。所以**重启会重起一轮走 chat 派发的活**；面板/CLI 派发的只回写 `Pending`、
   不重试、不唤人。理由与限制都已写在 `lib.rs` 调用 `finish_cluster` 紧邻的注释里（报告没人会在
   重启前读），详见 §5b。这一条的**残余风险**是：如果 `finish_cluster` 的自愈判据以后改成「任何
   终态都重试」，重启会变成一台自动重试机 —— 那时的改动必须重新评估这条。
13. **`api.rs` 的新测试 `the_topology_dispatches_carry_the_origin_onto_every_run_they_create`
   在 PATH 上没有 `mock-agent` 时必红**（裸命令名），属于 `receipts` 的夹具形状问题，我没动。
