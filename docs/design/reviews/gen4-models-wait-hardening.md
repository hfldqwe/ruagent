# `MODELS_WAIT` 加固：把「超时折成空 Vec」变成显式三态分支

**任务**：`t172`（kind = repair）· **日期**：2026-10-04 · **inScope 写入**：`crates/daemon/src/chat.rs`、本文件
**来源**：`t168`（只读评估，已 completed）。

> ## ⚠️ 本单是**加固（hardening）**，**不是缺陷修复**
> `t168` 已按 **C48** 逐站点证明**今天的行为可解释、不构成缺陷**（用户可见行为最坏是「不更新」，不是「错的判定」）。
> 本单的价值**只是**：把**一个静默替换**（超时 ⇒ 空 `Vec`，与「报告了空」同形）变成**显式分支**，让防线从**约定**变成**类型**。
> **没有任何用户可见行为被修复**，也没有任何「红 → 绿」被声称。

**最终字节**：`crates/daemon/src/chat.rs` sha256 = **`82F65A720C956D3F46462147BD0E9BCE561353F3DE7CCCC1C22AB8883AE4AFE5`**，git blob = **`e6798cad5c49914ceafc8647dee6e0cc4bd8a1ae`**（= 窗口前后锚，见 §4）。

---

## 1. 形状选择与理由（`Result<_, TimedOut>` 还是枚举？——**枚举**）

新增（`wait_options` 之前）：

```rust
#[derive(Debug, Clone, PartialEq)]
enum OptionsRead {
    NotReportedInTime,          // 等到 MODELS_WAIT 到点，运行时仍未报告 —— 超时，不是对选项的观测
    ReportedNone,               // 运行时报告了：它没有选项
    Reported(Vec<SessionOptionState>),  // 报告了这些选项（按构造**非空**）
}
```

**为什么不用 `Result<Vec<SessionOptionState>, TimedOut>`**：那会把**「报告了空」**和**「报告了选项」**挤进同一个 `Ok(Vec)`，调用方仍然必须**靠 `is_empty()` 去反推**三态 —— 而 `is_empty()` 反推**正是本单要消除的那个推断**（t168 §3：防线是约定不是类型）。枚举让**四种结局都有名字**（三态 + 超时），使 `record_probe` 的匹配**全备且显式**。
**额外好处（由构造保证的不变量）**：`Reported` 里**不可能为空**（空报告映射到 `ReportedNone`）⇒ 任何调用方**拿不到**一个「看起来像报告、其实是空」的值。

## 2. 改动清单（符号优先）

| 位置 | 改动 |
| --- | --- |
| `enum OptionsRead`（新，`wait_options` 之前） | 三态 + 超时，附文档说明「这是把会话通道本来就有的区分**做成全备投影**」 |
| `wait_options`（原 `chat.rs:1684-1691`） | 返回 `OptionsRead`；`match watch.borrow_and_update().clone() { None => NotReportedInTime, Some(v) if v.is_empty() => ReportedNone, Some(v) => Reported(v) }` —— **不再有 `unwrap_or_default()`** |
| `ChatManager::probe_options`（原 `:1322`） | `record_probe(runtime, state, updated_at)`：**按值**传入（不再 `&state`）；`probe_options` 其余逻辑不变 |
| `ChatManager::record_probe`（原 `:1339-1367`） | `state: OptionsRead` + **显式全匹配**：`NotReportedInTime \| ReportedNone` ⇒ 返回上一次（无上一次则返回空 `CachedOptions`）、**不 insert、不 `persist_options`**；`Reported(v)` ⇒ insert 缓存 + `persist_options`。**策略与 t192 逐字相同**，只是从「靠 `is_empty` 推断」变成**每态一个命名分支**；超时那一支另加一条 `tracing::debug!`（把「没报告」这件事**说出来**，而旧形状说不出来） |
| `apply_role_defaults` 超时分支（原 `:1703`） | WARN 措辞：`"role defaults: runtime reported no options"` ⇒ **`"role defaults: the runtime did NOT report its options within the bound (a timeout, not a report that it has none); skipping"`** + `wait = ?MODELS_WAIT` |
| `apply_role_defaults` 每项答复分支（原 `:1740`） | 措辞补齐：`"role default: the runtime did not answer within the bound (a timeout)"` + `wait = ?MODELS_WAIT`（原话「did not answer」本就准确，**补上界**使其与前者一致） |
| `apply_role_defaults` 的 `let Some(options) = … else { return }` | 该分支**原先完全静默**（不可达，但静默）⇒ 补一条 `tracing::debug!`（**这一条超出契约字面，我主动披露**：不动行为，只让「通道动了却没带值」不再是无声路径） |

**`MODELS_WAIT`（20s）未动**（契约要求）：它是「**到达类事实**」的上界，`options_watch()` 的到达时刻由运行时决定（spawn + 握手 + 目录请求，量级为秒；`OPTIONS_REFRESH = 6h` 是刷新节奏，**不是**这个等待）；挂住的运行时**无界** ⇒ 20s 是上界而非判定。实施过程中**没有出现必须动它的情况**。

## 3. 判据 1：「冷启动超时」与「报告了空」不再是同一个值 —— **读数**

**改前**（`t168` 已记）：`wait_options` 返回 `Vec`，`None`（未报告）经 `unwrap_or_default()` 变成 `Vec::new()`，与 `Some(vec![])`（报告了「没有」）**同形** ⇒ 判据 1 **不成立**。
**改后**：二者是**不同的枚举变体**（`NotReportedInTime` vs `ReportedNone`）⇒ 判据 1 **由构造成立**；并被 `:1810` 那条测试**钉住**（本单在其末尾新增第 5 段）：

```rust
assert_ne!(OptionsRead::NotReportedInTime, OptionsRead::ReportedNone,
           "a cold-start timeout and a report of none must not be the same value");
let timed_out = chats.record_probe("mock", OptionsRead::NotReportedInTime, 5);
assert_eq!(timed_out.options.first().and_then(|o| o.current.clone()), Some(Some("mock-max".to_string())),
           "a timeout must leave the last good catalog alone");
assert_eq!(cached("mock").map(|c| c.updated_at), Some(4),
           "a timeout must not restamp the cache with the time it gave up");
assert_eq!(stored, Some(1),                          // 表里仍是第 1 步那一行
           "a timeout must not persist over the stored catalog");
```
**诚实边界**：这是**类型级 + 单元级**的读数（变体可区分 + 超时不覆盖/不重打时间戳/不持久化），**不是**端到端的冷启动观测 —— 我**没有**跑真实运行时（见 §6）。`assert_ne!` 本身只证明两个变体不同；**实质读数**是后三条：超时走的是**另一个分支**，且**不动**已有知识。

## 4. 确定性红证（窗口，单点变异，共享树按字节恢复）

- **变异点（非 test 路径）**：`ChatManager::record_probe` 的 `NotReportedInTime | ReportedNone` 分支 —— 把「返回上一次 / 不落库」的收尾换成「把空目录 `insert` 进 `model_cache`」（模拟「空即是权威」这一错误策略）。
- **窗口**：`19:24:24 → 19:24:43`（**19 秒**），开窗前已宣告（含两个锚、非 test 路径、时长）。
- **读数**：**exit=101**，`panicked at crates\daemon\src\chat.rs:1962` —— 正是 `an_empty_probe_neither_overwrites_nor_persists_the_catalog` 的 `assert!(cached("cold-runtime").is_none(), "an empty probe on a cold runtime must not be cached as a catalog")` ⇒ **t192 那条策略是被测试真正钉住的**（不是「约定」）。
- **恢复（C49 + C52）**：
  - 开窗前 sha256 `82F65A72…` / blob `e6798cad…`；关窗后 **sha256 `82F65A72…` / blob `e6798cad…`（逐字相同）**；
  - 按 **C49** 刷新了 mtime 之后**才**跑门禁（§5）；
  - 按 **C52** 逐行读回恢复后的那一支：`1365| previous.unwrap_or(CachedOptions {` / `1366| options: Vec::new(),` / `1367| updated_at,` / `1368| })` ⇒ 与修后字节一致，无残留变异。
- **共享树零变异**：窗口内是**宣告过**的脏状态，19 秒关闭，按字节恢复，随后门禁读的是**恢复后的字节**（sha256/blob 与锚一致）。

## 5. 门禁（最终字节，退出码在任何管道之前取；**逐 target 计数**）

`scripts/cargo-team.ps1 test -p ruagent-daemon` ⇒ **exit=0**：

| target | 结果 |
| --- | --- |
| `unittests src/lib.rs` | **157 passed; 0 failed; 0 ignored**（含 `chat::generating_tests::an_empty_probe_neither_overwrites_nor_persists_the_catalog`，即 t192 那条） |
| `tests/capabilities.rs` | 11 passed |
| `tests/capability_defaults.rs` | 5 passed |
| `tests/event_compat.rs` | 2 passed |
| `tests/graph_ingest.rs` | 16 passed |
| `tests/injection_e2e.rs` | 0 passed / **8 ignored**（feature 门控，非本单） |
| `tests/knowledge_api.rs` | 16 passed |
| `tests/recall_evidence_announced.rs` | 3 passed |
| `tests/smoke.rs` | 0（feature 门控） |
| `tests/version_points.rs` | 5 passed |
| `Doc-tests ruagent_daemon` | 0 |
| **合计** | **215 passed · 0 failed · 8 ignored** |

`clippy -p ruagent-daemon --all-targets -DenyWarnings` ⇒ **exit=0**，`^error` = **0**。
`cargo fmt --all --check` ⇒ **exit=0**，零 `Diff in`。
最终字节：sha256 **`82F65A72…`**、blob **`e6798cad…`**。

**C47 口径（不许用绿读数收口）**：本处**无前置红证** —— 我**不**声称「修好了一条会红的 flake」，也**不**用 §5 的绿读数当「加固有效」的证据。本单的机制证据只有 §4 的红证（策略可被打破且测试会红）与 §3 的类型级读数；**行为正确性今天的结论来自 t168 的字节阅读，不来自本单的绿**。

## 6. 未覆盖（明写）

- **`spawn_idle_reaper` 的 `sleep(60s)`：只登记不改** —— 它是**周期节奏**（reaper 循环），不是等待某个终态。
- **`MODELS_WAIT` 的其它站点 S1/S2/S4/S5 本单不改**：S1 `switch_model`（超时 ⇒ **重启并应用模型**，结果正确只是慢）、S2 `set_option`（**显式 `Err`**）、S4/S5 `apply_role_defaults`（**显式 WARN**，本单只改措辞）—— 它们都不把超时折成值。
- **真实模型 / GUI 开关不测**：需要在场真实运行时；本单证据是**类型级 + 单元级 + 单点变异**，没有任何端到端观测。
- **`CachedOptions` / `api.rs` 那一层的残留仍未闭合**（t168 §4 残留①）：冷运行时**那一次 HTTP 响应**仍是「空列表 + 当前时间」的形状（`record_probe` 无既存目录时返回 `CachedOptions { options: Vec::new(), updated_at }`，见 `chat.rs` 该分支）。要闭合它得改 `CachedOptions` 的语义与 `api.rs:4811` 的响应形状 —— **`crates/daemon/src/api.rs` 不在本单 inScope**，故**如实留开**，不假装本轮解决了。
- **活守护进程（8787）未启停、未写其库**；`crates/daemon/tests/**`、`crates/acp/**`、`crates/store/**`、`panel/**`、`.github/**`、`scripts/**` **零改动**；收尾只按具体路径（`$env:TEMP\t172-fixed-chat.rs` 备份仍在，未用 glob 删任何东西）。

## 7. 建议的提交信息（契约要求：提交信息也要如实写「加固、非缺陷修复」）

```
harden(chat): make a cold-start option timeout a distinct variant, not an empty report

The session's option channel already distinguishes "not reported yet" (None)
from "reported none" (Some(vec![])); wait_options() projected both onto an
empty Vec, so a cold start slower than MODELS_WAIT reached record_probe()
looking exactly like a runtime that advertises nothing.

This is HARDENING, NOT A DEFECT FIX: t192's rule (an unreported probe may not
overwrite or persist the catalog) already made the ambiguous value
non-authoritative, so no user-visible behaviour changes here. What changes is
that the defence is now the type and a total match instead of a convention --
OptionsRead { NotReportedInTime | ReportedNone | Reported(Vec) } -- plus two
WARN messages that can now say "did not report within the bound" instead of
"reported no options". MODELS_WAIT is unchanged: 20s bounds an arrival fact
(the runtime decides when to report), a hung runtime is unbounded, and the
bound never becomes the user's verdict.
```
