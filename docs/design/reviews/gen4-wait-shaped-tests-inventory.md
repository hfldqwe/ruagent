# 全仓盘点：「测试侧等待」这一族（只读审计）

**任务**：`t157`（kind = work，**只读**）· **日期**：2026-10-04 · **写入**：仅本文件（`crates/**`、`panel/**`、`.github/**`、`scripts/**`、`tools/**` **零改动**）

**审计所读的字节（必须说明）**：本盘点读的是**当前工作树**，其中 `crates/daemon/src/chat.rs`（t145 的修复）、`crates/daemon/src/distill.rs` + `crates/daemon/src/registry.rs`（t154 的修复）**带有未提交改动** ⇒ 表里 `delete_removes_the_row_and_stops_a_live_run_first`、`message_count_is_real_before_the_index_catches_up` 读到的是**修后**形状（它们的修前形状见各自报告）；其余文件为 HEAD 内容。

**结论一句话**：这一族在全仓**不是散落的**，而是**高度集中在「轮询 helper + 固定上限」这一个形状**上，而其中**绝大多数已经是健康的**（等一个可观测事实、耗尽时**响亮报错**）。真正需要处置的只有 **4 处**：2 处「裸 sleep 后做否定断言」（`chat.rs` 的 `the_unattended_plan_decides_and_the_default_spends_nothing`）、1 处「helper 耗尽时**静默**返回、失败信息失去成因」（`chat.rs::wait_for_rows`）、1 处「清理重试静默吞掉失败」（`store/tests/ledger_reopen.rs` 的 `Drop`）。**没有发现任何「产品侧缺信号」的新成员**（④ 类为空，理由见 §6）。

---

## 1. 检索命令与命中计数（可重跑）

```powershell
# 形状检索（全仓，Rust 源）
grep -rn --include=*.rs -F -- 'sleep('        crates   # 43
grep -rn --include=*.rs -F -- 'from_millis'   crates   # 42
grep -rn --include=*.rs -F -- 'for _ in 0..'  crates   # 20
grep -rn --include=*.rs -F -- 'timeout('      crates   # 16
grep -rn --include=*.rs -F -- 'Instant::now()' crates  # 21
grep -rn --include=*.rs -F -- 'recv_timeout'  crates   #  1
grep -rn --include=*.rs -F -- 'wait_until'    crates   #  0（本仓没有这个名字的 helper）
# 旁证
grep -rn --include=*.rs -F -- '#[ignore'      crates   # 24（= 16 真属性 + 8 散文，见下）
grep -rn -F -- 'waitForTimeout' panel/e2e              # 19  （未逐条读，见 §7）
grep -rn -F -- 'Start-Sleep'    scripts                #  3  （未逐条读，见 §7）
```

**合并扫描（我用的形状正则，含 `sleep(` / `from_millis` / `for _ in 0..` / `timeout(` / `Instant::now()` / `recv_timeout` / `wait_until` / `poll`）**：原始命中 **153** 行；其中落在**测试上下文**（`*/tests/*.rs` 或 `#[cfg(test)]` 内的 `mod *_tests`）的是下面各表列出的那些；**其余命中属产品代码**（见 §7③）。

**⚠️ 一条纪律（本轮亲身踩到，captain 已入账 C51）**：`grep -rn "#\[ignore"`（不加 `-F`）会因 `[` 被当成括号表达式而**报错**（`Unmatched [ …`），此时它**仍然返回 0 计数** ⇒ **0 是一个假读数**。同一命令加 `-F` 后得 **24**。⇒ 报告里的计数**只用 `-F` 的固定串检索**。

### 1.1 「计数为 0」的判据（C51 的正确形态，含本单的实测控制组）

一个计数要能当证据，**同一个调用形状**必须先通过三关（缺一不可）：

```powershell
$files = Get-ChildItem crates -Recurse -Filter *.rs      # 注意：-Path 'crates/**/*.rs' 不递归 ⇒ 会得到一个自信的 0
# 1) 调用方式真的在跑（正对照：一个必然存在的串必须非零）
($files | Select-String -Pattern 'fn '                        | Measure-Object).Count   # 2171 ✓
# 2) 负对照（不可能的串必须为 0）
($files | Select-String -Pattern '@@no-such-string-anywhere@@' | Measure-Object).Count   #    0 ✓
# 3) 目标串；并且**转义要与匹配模式一致**（regex vs -SimpleMatch 是两个不同的字面量）
($files | Select-String -Pattern '#\[ignore' | Measure-Object).Count                     #   24   ← regex
($files | Select-String -Pattern '#[ignore' -SimpleMatch | Measure-Object).Count         #   24   ← 字面量（不带 `\`）
```

**本单在 C51 上的实测两例（都是「自信的 0」，但成因不同 —— 这个区分很重要）**：
1. **工具不在**：`rg -n -F -- '#[ignore' crates` 在本机 **`rg` 不在 PATH**（`Get-Command rg` = False）⇒ **三条计数全为空**；若只跑目标串，得到的就是一个**自信的 0**。**正对照 `fn ` 同样为空 ⇒ 立刻暴露**（这正是正对照的价值）。
2. **规格错，不是仪器错**：`Select-String '#\[ignore' -SimpleMatch` = **0** —— 字面量模式下 `\` 是**模式的一部分**，源码里不存在 `#\[ignore` 这个串 ⇒ **这个 0 是诚实的**（我给的串确实不在文件里）。⇒ 纪律：**换匹配模式就要换转义**；同一个「0」在「工具没跑」与「我要的串确实不在」之间，靠**正对照 + 模式一致性**区分。

### 1.2 `#[ignore]` 的 24 vs 16：**captain 已结清（本单不再背）**

```
24 命中 = 16 条【真属性】（`^\s*#\[ignore`）+ 8 条【散文里提到 #[ignore] 的字面量】
cargo test --workspace -- --ignored --list ⇒ 16     ← 与 t144 的清账逐字一致
```
⇒ **CI 的忽略门禁没有缺口**。本单独立复核（两种工具/两种模式交叉）：**属性形 = 16**、**任意形 = 24**、**差 = 8 散文**，与 captain 的结论一致。（散文 8 处由 captain 点名：`retrieval-gold-live.rs:26`、`injection_e2e.rs:84,:96`、`migrations.rs:1164,:1294`、`retrieval-gold-copy.rs:34`、`store/src/lib.rs:1286`、`graph/tests/live-after.rs:3`；我未逐处复核这些**散文**坐标。）

## 2. 分类判据（先把边界写清，避免「混类」）

| 类 | 判据（我实际使用的） |
| --- | --- |
| **① 等一个可观测事实到终态** | 退出条件是一个**产品给出的事实**（产物字段/行/事件/文件），且**预算只是上界**：耗尽时**响亮失败**（`panic!`/`assert!`/`expect`）。 |
| **② 固定猜测预算** | **判定依赖墙钟或迭代数**，而不是依赖事实：典型是「裸 `sleep(N)` 之后断言」，或耗尽后**不检查**就直接断言别的东西。 |
| **③ 等待体/清理里断言空转或静默吞掉** | 等不到就**走过去**（无失败），或**吞掉错误**（`let _ =`、`if let Ok` 无 else）；断言恒真。 |
| **④ 产品侧缺信号** | 产品**根本没有**可观测的完成信号，测试只能靠时间猜 ⇒ **必须**写成产品侧 finding。 |

**边界说明（不混类的关键）**：`for _ in 0..N { … }` **本身不是** ② —— 只有「**判定**依赖 N」时才是。若退出是事实、N 只是上界、耗尽会响亮报错 ⇒ **①**（我把它记为「①+预算未论证」，并在建议处置里点名该数字）。反之，`store/tests/migration_write_lock.rs` 里 `waited >= 300ms` 这种**阈值被写在注释里论证过**的，不算 ②。

## 3. 表 A —— **有前置红证 / 已知会红**（排序第 1 组；处置：已修，只登记）

| # | 坐标（符号优先） | 在等什么 | 类 | C47 前置红证 | C48 生产可解释 | 处置 |
| --- | --- | --- | --- | --- | --- | --- |
| A1 | `chat::generating_tests::delete_removes_the_row_and_stops_a_live_run_first`（`chat.rs`，本次读到的**修后**形状：`TEARDOWN_BOUND=5s` + `try_recv` + 只数 `AgentMessageChunk`） | 运行**自己的**终局信号 `Stopped`/`Error`，或会话 sender 掉落（`Closed`） | ① | **有**：修前 **4/10 红**（10 占用进程 + 私有 `-NoLock` 冷构建） | 能（红是**投递延迟**，非产品缺陷） | **已修**（t145），只登记；**注入行为已换成不会自然竞争的 `slowreply`** |
| A2 | `chat::generating_tests::message_count_is_real_before_the_index_catches_up`（`chat.rs`，修后：`USER_MESSAGE_BOUND` deadline + 响亮报错） | 索引/`history()` 报出的那个字段变成用户可见 | ① | **有**：修前 **2/3 红**（诊断运行） | 能（自增长预算 vs 墙钟） | **已修**（t149），只登记 |
| A3 | `distill::t329_tests::*`（6 条）+ `registry::tests::*`（2 条） | —（**不是等待**：读的是 `write_memories(…).await` 的**同步返回值**与同步文件 I/O） | **不属于本族** | 无（0/4） | — | **本族除外**：t154 查明是**夹具陈旧**（`pid + seq` + pid 重用 ⇒ 打开上一轮旧库）。**本表保留它，免得下一位再把它当本族**；处置归 t154 |

⇒ **本族在全仓的「已知会红」样本只有 A1/A2 两条，且都已修。**

## 4. 表 B —— **判据侧确定可修**（排序第 2 组；处置：立单 / 现在就修）

| # | 坐标（符号优先） | 在等什么 | 类 | C47 | C48 | 建议处置 |
| --- | --- | --- | --- | --- | --- | --- |
| B1 | `chat::generating_tests::the_unattended_plan_decides_and_the_default_spends_nothing`（`chat.rs`，两处**裸** `sleep(300ms)` 后 `assert!(rows.is_empty())` / `assert_eq!(rows.len(), 1)`） | 「什么都没发生」—— **否定性事实**，且**判定直接依赖这个 300ms** | ② | 无 | 能（延迟本身） | **立单**：把「等待」换成**正面可观测**（该用例 (2) 已经用 `wait_for_rows` 做到了；(1)/(3) 可断言「同一窗口内 `rows` 不变」的**双向**证据，或直接对「不产出行」的那条路径做单元级断言），不要再用「睡一会儿证明没事」 |
| B2 | `chat::generating_tests::wait_for_rows`（`chat.rs`，`for _ in 0..60 { sleep(50ms) }`，**耗尽后直接 `rows(db).await` 返回**） | 后台蒸馏写进 `distill_log` 的那一行 | ①（**但耗尽不响亮**） | 无 | 能 | **立单（小）**：耗尽时 `panic!("wait_for_rows: wanted {want}, got {n} …")` —— 现在失败信息是「len 0」，与「真的没写」不可分 |
| B3 | `store::tests::ledger_reopen` 的 `TempDir::drop`（`store/tests/ledger_reopen.rs`，`for _ in 0..20 { remove_dir_all → Err(_) => sleep(50ms) }`） | Windows 释放文件句柄（清理重试） | ③（**静默吞掉**：20 次都失败也无声） | 无 | 能（句柄延迟） | **立单（小）**：最后一次失败要**留声**（`eprintln!` 或 panic）—— 现在它正是本代抱怨过的「泄漏 root」的隐藏源 |
| B4 | `daemon/tests/injection_e2e.rs` 的 `drive_run` / `drive_chat` / `chat_event_carries_the_injection_before_the_reply`（`for _ in 0..600 { sleep(100ms) }` = **60s**）与 `a_retried_run_carries_the_shared_render`（`0..80 { sleep(250ms) }` = **20s**） | 转录 JSONL 里出现 `context_injected` **且** 回显非空；或运行到达终态 | ①（预算未论证，但耗尽**响亮**：`unwrap_or_else(|| panic!(…))` / `assert!`） | 无 | 能 | **只登记**（形状正确）；若要收紧：把 600×100ms 的**数字**换成「等运行自己的终态事件（`Stopped`/`Error`）后再读转录」，预算只留作上界 |
| B5 | `chat::generating_tests::generating_tracks_the_run_while_active_tracks_the_lifetime`（`chat.rs`，`for _ in 0..200 { sleep(50ms) }` = 10s，退出条件 `!e.generating`，耗尽 `expect("the turn never finished")`） | 产品字段 `generating` 变 false | ①（预算未论证） | 无 | 能 | **只登记**（形状正确）。**review 在 t150 点名它**：把 200×50ms 换成**产品自己的终局事件**（正是 A1/t145 用的那一对）就能去掉这个 10s 上限 |
| B6 | `chat::option_catalog_tests::persisted`（`chat.rs`，`for _ in 0..100 { sleep(20ms) }`，退出=行出现，耗尽 `assert_eq!(stored, Some(1), …)`） | 被 spawn 的**表写入**落地的那一行（注释已说明「写是 spawned 的，所以等行而不是抢跑」） | ① | 无 | 能 | **只登记**（形状正确，注释也把因果写清了） |

## 5. 表 C —— **未解释**（排序第 3 组）

| # | 项 | 为什么未解释 | 处置 |
| --- | --- | --- | --- |
| C1 | t154 的 **F2 触发时刻**（哪一次 pid 重用、哪个测试先跑） | 遗留目录已被「先清再建」中和，事后无法回放当时的 pid 映射 | **留成未解释项**（已在 `gen4-flaky-distill-registry-repair.md` §8 同样登记）：**不许**用绿读数收口 |
| C2 | ~~`#[ignore]` 属性命中 **24** 与任务单引用 t144 的「**16 条**」不一致~~ | **已结清（captain，2026-10-04）**：24 = **16 条真属性** + **8 条散文里提到 `#[ignore]` 的字面量**；`cargo test --workspace -- --ignored --list` = **16** ⇒ **CI 忽略门禁无缺口** | **关闭**；本单独立复核（两种工具 × 两种模式）得属性形 = **16**、任意形 = **24**、差 = 8 ⇒ 与结论一致。本条**不再作为未解释项**；检索纪律见 §1.1（C51） |

## 6. 表 D —— **仅形状可疑、无红证**（排序第 4 组；处置：只登记，**不据此宣布任何东西已修好**）

以下都属 **①**（等可观测事实 + 耗尽响亮），只是**预算数字未被论证**，或**本身就是产品给的可观测信号**：

| 坐标（符号优先） | 等什么 | 预算 | 备注 |
| --- | --- | --- | --- |
| `mock_agent` 的 `chat_experience.rs::poll_json`（`0..100`×100ms，耗尽 `panic!("timed out waiting for: {what}")`） | 传入谓词为真（HTTP 字段） | 10s | ①，**失败信息带 `what`** ✓ |
| `mock-agent/tests/e2e_daemon.rs::poll_until`（deadline 10s，`assert!(now < deadline, "timed out waiting for {url}, last: {v}")`） | HTTP 谓词 | 10s | ①，**带最后一次值** ✓ 最完整的一例 |
| `mock-agent/tests/e2e_daemon.rs::poll_transcript`（`ceiling` 参数由调用方给，如 10s） | 转录里出现某行 | 由调用方给 | ①，同一形状 ✓ |
| `mock-agent/tests/judge.rs::wait_task`（`0..80`×125ms，耗尽 `panic!("task condition not reached in time")`） | 任务状态谓词 | 10s | ① |
| `mock-agent/tests/wiki_pipeline.rs::wait_build`（`0..240`×250ms，耗尽 `panic!("build {id} did not finish in time")`） | wiki build 的 `status ∈ {done,failed}` | 60s | ①，命名的失败信息 ✓ |
| `mock-agent/tests/mcp_health.rs::health_check_reports_and_gates_injection`（`0..50`×200ms，退出=`report.contains("mcp=")`，随后 `assert!(report.contains("mcp=good"), …)`） | 转录里的 `mcp=` 报告 | 10s | ①（耗尽靠断言现形），**断言打印 report** ✓ |
| `mock-agent/tests/e2e_daemon.rs::orphan_sweep_finds_and_kills_children`（`0..50`×20ms，耗尽 `assert!(alive, …)`） | 子进程真的活着（`process_started_at_ms`） | 1s | ① |
| `mock-agent/tests/integration.rs::drive`（deadline 500ms + `timeout(100ms, recv)` 排空事件流） | 通道排空（`Ok(None)` = 所有 sender 掉了） | 500ms | ①**但语义要读准**：这里不是等某个事实，而是**排空**；deadline 到点是「流还在」——**断言没跟上**（本函数只返回 `events`），所以**不要在它身上挂「必须收全」的断言**（现在没有） |
| `store/tests/migration_write_lock.rs::the_migration_transaction_holds_the_write_lock`（`recv_timeout(10s)` 等持有者信号；`waited >= 300ms` 带注释论证） | 竞争写者已持锁 / 迁移确实等过锁 | 10s + 400ms 试验性持有 | **①，且是范例**：阈值**被注释论证**（「inside the 400ms hold」），并明写「`opened` 先判，否则本测试测得的是空」 |
| `daemon/tests/smoke.rs::smoke`（`.timeout(Duration::from_secs(300))`） | SSE 流结束 | 300s | ①（reqwest 的超时，子进程/流边界） |
| `mock-agent/tests/chat_experience.rs` 的 8 处 `poll_json(...)` 调用点（`option_catalog_probes_once_then_persists`、`chat_history_and_runtime_switch_idempotent`、`role_option_defaults_apply`、`run_options_apply`、`chat_close_drops_parked_asks`） | 各自的谓词 | 10s | ①（复用 `poll_json`） |
| `mock-agent/tests/e2e_daemon.rs` 的 ~18 处 `poll_until/poll_transcript` 调用点（`permission_parks_…`、`crashed_agent_marks_run_failed`、`fanout_*`、`land_selected_worktree`、`cancel_parks_then_cancels`、`harness_concurrency_gates_and_queueing`、`failed_run_one_click_retry`、`chat_stop_*`、`role_*`、`chat_runs_in_its_project_cwd` …） | 各自谓词 | 10s | ①（复用两个 helper） |
| `chat.rs` 的 A1/A2/B2/B5/B6（见上表） | — | — | 已在 A/B 表里逐条处置 |

**④（产品侧缺信号）在本轮盘点里为空**：我逐条读过的等待里，**没有一处**是「产品没给出可观测信号、测试只能靠时间猜」——等 HTTP 字段/行/事件/文件都有对应字段；**唯一曾经像 ④ 的 t154 那 8 条，最后查明是夹具陈旧**（表 A3）。⇒ **本轮不收 ④ 类 finding**；若将来出现，判据是：**把等待换成「事实」时找不到任何产品字段可读**。

## 7. 我【没有】覆盖的（点名，不静默省略）

**① `#[ignore]`（**属性 16 条**）**：本盘点**没有**逐条读它们的处境（任务单说 t144 的 manifest 已说明各自处境）。**计数差异已结清**：`#[ignore` 任意形 = **24** = **16 条真属性 + 8 条散文提及**，`-- --ignored --list` = **16**（captain 结清；本单两工具交叉复核一致）⇒ **不再是未解释项**，见 §1.1/§1.2 与表 C2。**仍未逐条读**这一点保持登记。

**② `panel/e2e/**`**：`grep -F 'waitForTimeout'` = **19** 处，`waitFor(` = 0；`scripts/**` 的 `Start-Sleep` = **3** 处。这些**都不在 crates 的检索面内**（本单 out of scope），**我一行都没读** ⇒ 它们既可能含本族成员，也可能是等 UI 事实；**需要 panel/scripts 属主另做一张同形表**。

**③ 产品代码里的等待（非测试侧，按类排除但点名）**：`chat.rs` 的 `MODELS_WAIT` 系列（`switch_model` / `set_option` / `wait_options` / `apply_role_defaults` 各一处 `tokio::time::timeout(MODELS_WAIT, …)`）、`spawn_idle_reaper` 的 `sleep(60s)`、`chat.rs:161/974` 的 `Instant::now()`（时间戳）、以及 `store/tests/migration_write_lock.rs:59` 的 `busy_timeout(5s)`（配置，不是等待）。**它们是产品行为，不是测试判据** ⇒ 不属本族；**但 `MODELS_WAIT` 是「产品用墙钟代替终态」的一处**，若有人要为「面板开关到底生效没」立判据，这里是**该提供可观测信号的候选**（**我只点名，未评估**）。

**④ 形状正则的假阳性（点名，免得下一位重跑时误算）**：`graph/tests/live-after.rs:305 for _ in 0..30` 与 `graph/tests/resolution.rs:126 for _ in 0..20` 是**合并有限对的「有界工作循环」**（`else { break }` = 没有剩余可合并对），**不是等待**；`graph/tests/multihop-gold.rs:214 for _ in 0..6` 是**重复 6 次同一查询证明逐字节确定性**（幂等性证据，不是等待）；`knowledge/tests/retrieval-quality.rs:394 for _ in 0..indent` 是**字符串缩进**；`retrieval-quality.rs:402` / `retrieval-gold-live.rs:309` / `store/src/lib.rs:1300` 的 `Instant::now()` 是**耗时测量**（读数值），不是等待。

**⑤ 我读不出「它在等什么」的地方**：`mock-agent/tests/integration.rs::drive` 的 500ms 排空（语义是「排空」而不是「等某事实」，见 §6 表 D 备注）与 `daemon/tests/smoke.rs` 的 300s 超时（等的是「流结束」还是「子进程退出」，**我未追到那一条流的产生者**）—— 这两处**我标注为「语义未定」**，不硬塞进某类。

## 8. 建议处置汇总（谁该动手）

| 组 | 成员 | 建议 |
| --- | --- | --- |
| 已修，只登记 | A1（t145）、A2（t149） | 无需动作；不要再为它们重跑大批次（C47：它们的**前置红证已经存在**） |
| 立单（小而值） | **B1**（两处裸 `sleep(300ms)` 后做否定断言）、**B2**（`wait_for_rows` 耗尽不响亮）、**B3**（`ledger_reopen` 的 `Drop` 静默吞错） | 由 `crates/daemon/src/chat.rs`、`crates/store/tests/**` 属主立单；**B1 是本盘点里最该修的一条**（它是 ② 类里唯一「判定依赖预算」的） |
| 只登记（形状正确，预算未论证） | B4/B5/B6 + 表 D 的全部 `poll_*` helper | 若某条真红过，先补**前置红证**（C47），再谈「0/N 绿」；**不要**因为「形状像」就重写健康 helper |
| 产品侧候选（未评估） | `MODELS_WAIT` 系列（`chat.rs`） | 若将来要为「模型/选项切换生效」立判据，先让它**报出可观测终态**，而不是让测试等墙钟 |
| 不做 | `%TEMP%` 清理、`#[ignore]` 逐条读、`panel/e2e`、`scripts` | 前者见 t154 §9.2；后三者见 §7（需各自属主/另立单） |

**本单未改任何代码、未跑任何测试**（只读审计；契约的 Verify 为空）。检索命令与计数见 §1，可原样重跑。
