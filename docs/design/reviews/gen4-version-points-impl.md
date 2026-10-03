# INT-F3/F4 版本读点：**未落地** —— 交接包（t126, attempt 2）

**状态：failed（交接，不是产品缺陷）** · 零源码改动：`git status --porcelain -- crates/daemon/src/api.rs crates/daemon/tests` = **空** ⇒ 树没有被留在半成品状态。

## 为什么停在这里（诚实的原因）

本 attempt 只完成了**开工具质**（够写补丁的坐标全部回读完毕），**没有**进入「改 + 编译 + 三个门 + 线上探针 + 报告」的循环。停手的原因是**预算**（本会话上下文耗尽），不是发现产品缺陷。**交接包把剩下的工作变成机械动作**（下面的坐标全部逐行回读过，不引用任何漂移行号）。

## 已复核的坐标（今天字节）

| 目标 | 事实 | 坐标 |
| --- | --- | --- |
| **F4 `/stats`** | handler 体只有三行：`let stats = state.mgr.db().agent_stats().await?;` + `Ok(Json(serde_json::json!({ "agents": stats })))` | `crates/daemon/src/api.rs` 的 `async fn stats`（回读：本 attempt 用 `Select-String 'async fn stats'` 定位后逐行打印） |
| ├ `embedder` 的来源 | `state.knowledge.embedder_name()` —— 同一 accessor 已在 `/knowledge/*` 用（返回 `&'static str`） | 定义 `crates/store/src/store.rs:625 pub fn embedder_name(&self) -> &'static str`（注意：该文件在 **store** crate）；api.rs 里既有用法可直接照抄 |
| └ `scoring_version` 的来源 | **不许字面量**：用单一真相源的常量 `SCORING_VERSION` | 定义 `crates/knowledge/src/store.rs:338 pub const SCORING_VERSION: u32 = 2;`，由 `crates/knowledge/src/lib.rs:21 pub use store::{…}` 再导出；**daemon 里已有 8 处 `SCORING_VERSION` 引用**（照抄其中一处的路径前缀即可，勿臆造） |
| **F3 `/distill`** | handler 体：`let p = state.chats.distill_policy_now();` 然后 6 个键（`auto/agent/language/prompt/graph/builtin_prompt`），`builtin_prompt` 来自 `crate::distill::builtin_extraction_prompt()` | `crates/daemon/src/api.rs` 的 `async fn distill_policy_get`（回读：逐行打印过 2947-2977） |
| ├ **有效 prompt 的本体** | `Distiller::compose_prompt()` = `extraction_prompt(self.language.as_deref(), self.prompt_override.as_deref())` | `crates/daemon/src/distill.rs:230 impl Distiller` / `:236 pub(crate) fn compose_prompt` |
| ├ **同源的关键** | `content_hash(&self.compose_prompt())` —— **同一个函数、同一对输入** | `crates/daemon/src/distill.rs:283`（生成 `prompt_hash` 并落 `distill_log`） |
| ├ **api.rs 能不能拿到同一段 prompt** | **能，但不是直接调 `extraction_prompt`**（它是**私有**的：`crates/daemon/src/distill.rs:1114 fn extraction_prompt(...)` 无 `pub`）。可以走 **`Distiller`**：`crates/daemon/src/distill.rs:201 pub struct Distiller`，且 `:210 pub language: Option<String>`、`:212 pub prompt_override: Option<String>`、`:215 pub graph: bool` 都是 **pub 字段**，`compose_prompt` 是 `pub(crate)` ⇒ 从 api.rs 构一个 `Distiller` 再调 `compose_prompt()` 是**可达**的（若还有其它必填字段，用 `..Default::default()`；若没有 `Default`，则需在 `distill.rs` 加一个 `pub(crate)` 访问器 —— **那超出本单 inScope，要先问 captain**） | 同上 |
| ├ 策略→Distiller 的既有映射 | `prompt_override: policy.prompt`（即 GET 返回的 `p.prompt` 就是 `prompt_override`） | `crates/daemon/src/chat.rs:1576` |
| └ 哈希函数 | `content_hash(content: &str) -> String` | `crates/memory/src/write.rs:79 pub fn content_hash` |

## 计划的补丁形状（两步，都是加法式）

1. `async fn stats`：`serde_json::json!({ "agents": stats, "embedder": state.knowledge.embedder_name(), "scoring_version": <knowledge 的 SCORING_VERSION 路径> })`，附注释点名两个来源与坐标。
2. `async fn distill_policy_get`：在 6 个键后追加 `"prompt_hash": ruagent_memory::write::content_hash(&<由 p 构造的 Distiller>.compose_prompt())`，注释写明「与 `distill.rs:283` 是同一函数同一输入 ⇒ 同源」。

## 还没取到的读数（交接给下一位必须补）

- **改前/改后原始 JSON**：`GET /api/v1/stats`（改前 = `{"agents":[]}`，t62 §2 已量）与 `GET /api/v1/distill`（改前 = 6 键，t62 §2 已量）的**改后**值 —— 需要**临时 root + 临时端口**起自己的守护进程（同树构建 **不要**用 `-TargetDir`，C31：实测独占全局锁 9m40s）。
- **三条能红用例 + 至少一条负控**（隔离树/临时副本变异，共享树零变异，给窗口起止 + 恢复后绿读数）：**未做**。
- 三个门（`test -p ruagent-daemon`、`clippy -p ruagent-daemon --all-targets -DenyWarnings`、`cargo fmt --all --check`）：**未跑**（因为没有改动可门）。

## 未测 / 不覆盖（第 19 条）

- **不覆盖** `/distill` 的 **PUT** 半边（`/api/v1/distill` 只有 GET；GET `/api/v1/distill/policy` 在当前守护进程上返回**非 JSON**，t62 §2 已记）—— 本单没有解释那条路径的真实形状。
- **不覆盖** `prompt_hash` 的**值是否随 `[distill] prompt` 改动而变**（那需要 PUT 半边，见上）；本单只设计到「与 `distill.rs:283` 同源」这一步。
- **不覆盖** `scoring_version` 的**语义正确性**（它是不是「今天该报的那个版本」），本单只把它从常量搬到 wire。

## 零改动自证

`git status --porcelain -- crates/daemon/src/api.rs crates/daemon/tests` ⇒ 空（t126 attempt 未写任何 Rust 字节）；本 attempt 除了本文件与两份新的型定义外没有产出。**active daemon**：全程未启停、未写活库；引用活守护进程时按 **C32**（端口归属 + health）而不是被记住的 pid。

---

# t127 落地（续 t126）：F4 先、F3 后

**日期**：2026-10-04 · **性质**：实现落盘（t126 的续做，不是重做）。F4 与 F3 都落在 `crates/daemon/src/api.rs`；新增 `crates/daemon/tests/version_points.rs`（4 条用例）。**`crates/daemon/src/distill.rs` 零字节改动**（见 §T127.5 的路径选择）。门跑两次：F4 落完先取其读数，再落 F3 取其读数。

## T127.1 F4 —— `/stats` 的 `embedder` / `scoring_version`（先落）

**改前（t62 §2，逐字）**：`{"agents":[]}`

**改后（本单最终字节，`GET /api/v1/stats`，逐字原始 JSON）**：

```
READING F4 /api/v1/stats: {"agents":[],"embedder":"hash-embedder","scoring_version":2}
```

**单一真相源（不许字面量）**：`"embedder"` 取自 `state.knowledge.embedder_name()`（`Knowledge::embedder_name`，`crates/knowledge/src/store.rs`，`pub fn embedder_name(&self) -> &'static str`；**同一 accessor 已被 `/api/v1/knowledge/documents` 读**）；`"scoring_version"` 取自 `ruagent_knowledge::SCORING_VERSION`（`crates/knowledge/src/store.rs`，由 `crates/knowledge/src/lib.rs` 再导出）。

**同源交叉对照（同一 accessor 供两个读点）**：用例从 `/api/v1/knowledge/documents` 读到同一个 `"embedder"` 并与 `/stats` 逐字比较 ⇒ 两边相等（断言通过）。

**负控（临时副本变异，共享树零变异）**：

```
READING F4 negative control: real=Some(("hash-embedder", 2)) renamed=None deleted=None literal=Some(("a-literal-not-the-live-embedder", 999))
```

⇒ 同一判据对「删键 / 改键名」返回 `None`（不可信），对「换字面量」给出与真值不同的对（`999` ≠ `2`）⇒ 三个读点各自都能红。**该负控是真实响应 JSON 的临时副本**，不是共享树变异。

## T127.2 F3 —— `/distill` 的 `prompt_hash`（后落）

**改前（t62 §2，逐字，键序）**：`[auto,agent,language,prompt,graph,builtin_prompt]`

**改后（本单最终字节，`GET /api/v1/distill`，逐字原始 JSON）**：

```
READING F3 /api/v1/distill: {"auto":false,"agent":null,"language":null,"prompt":"PROMPT-A","graph":true,"builtin_prompt":"<内置抽取 prompt，逐字未动>","prompt_hash":"6b252b7eab25ba01ba42b088a3b5b397e97cfc6e1b09f33628a88689a9ffbeee"}
```

**同源的证明（同函数、同输入）**：真实蒸馏的算法是 `ruagent_memory::write::content_hash(&self.compose_prompt())`（`crates/daemon/src/distill.rs`，`fn distill_plan` 内的 `prompt_hash` 绑定），而 `compose_prompt()` = `extraction_prompt(self.language, self.prompt_override)`（同文件 `impl Distiller`）。api.rs 用**同一个** `compose_prompt()` 在**同一对输入**上算：

| `Distiller` 字段 | 本单取值 | 来源（策略→字段的映射） |
| --- | --- | --- |
| `db` / `root` | `state.mgr.db().clone()` / `state.config.root.clone()` | 与 `api.rs` 既有构造（session-distill 路径）同形 |
| `embedder` | `Some(state.knowledge.embedder())` | 同既有构造；**不影响 compose_prompt** |
| `registry` | `state.mgr.registry_view()` | 同既有构造 |
| `language` | `p.language` | 策略字段 → `Distiller.language`（`crates/daemon/src/lib.rs` 启动时同一映射） |
| `prompt_override` | `p.prompt` | 策略字段 → `Distiller.prompt_override`（`crates/daemon/src/chat.rs`：`prompt_override: policy.prompt`） |
| `graph` | `p.graph` | 策略字段 → `Distiller.graph` |

**独立重算同一判据（用例复算 compose 的文档化形状）**：用例从 wire 读 `prompt`（或 `builtin_prompt`）并算 `content_hash(base + "\n\nTRANSCRIPT:\n")`：

```
READING F3 hashes: override daemon 6b252b7eab25ba01ba42b088a3b5b397e97cfc6e1b09f33628a88689a9ffbeee | same-policy second daemon 6b252b7eab25ba01ba42b088a3b5b397e97cfc6e1b09f33628a88689a9ffbeee | other prompt 6de27008a956e493b0f258b84738a1222cfc4ddd5d1d3a4f68055e37f99d7717 | default-prompt daemon 01789e40365a8992a348c5ddb89c0a3f409fec90b03f5a5e586abbb54d4eb8ba (expected 6b252b7eab25ba01ba42b088a3b5b397e97cfc6e1b09f33628a88689a9ffbeee / 01789e40365a8992a348c5ddb89c0a3f409fec90b03f5a5e586abbb54d4eb8ba)
```

⇒ **同输入 ⇒ 同 hash**（同一守护进程与第二个同策略守护进程都与独立重算相等）；**改一个字段 ⇒ 不同 hash**（`[distill] prompt = "PROMPT-B"` ⇒ `6de27008…`）；**内置 prompt ⇒ 另一个可复算值**（`01789e40…`，用例用 wire 上的 `builtin_prompt` 独立算出）。

**负控（同 F4 形状，临时副本变异）**：

```
READING F3 negative control: real=Some("6b252b7e…") renamed=None deleted=None literal=Some("0000000000000000…")
```

## T127.3 加法式（逐字对照）

| 读点 | 改前（t62 §2 逐字） | 改后 | 结论 |
| --- | --- | --- | --- |
| `/stats` | `{"agents":[]}` | `{"agents":[],"embedder":…,"scoring_version":2}` | `agents` 键**取值一字未动**、仍居首位；只追加两键（该行末尾多一个逗号，是追加的语法结果） |
| `/distill` | `[auto,agent,language,prompt,graph,builtin_prompt]` | 同上六键**逐个不变、顺序不变** + 末尾 `prompt_hash` | 键名与取值逐字相同；`builtin_prompt` 仍在原位 |

## T127.4 门禁（三个命令，最终字节）

| 命令 | 读数 |
| --- | --- |
| `test -p ruagent-daemon` | **ok-lines=11 · FAILED-lines=0 · panicked-lines=0 · exit=0**（`elapsed=48.7s`；含 `version_points` 的 `4 passed; 0 failed`） |
| `clippy -p ruagent-daemon --all-targets -DenyWarnings` | **exit=0**（`error-lines=0`，`elapsed=6.8s`）；**两件证据**：① 覆盖的 target = 12 个 unit 行：`capabilities`、`capability_defaults`、`event_compat`、`graph_ingest`、`injection_e2e`、`knowledge_api`、`recall_evidence_announced`、`ruagent_daemon`、`ruagent_extract`、`ruagent_mcp`、`smoke`、`version_points`；② 本次是**增量**重查（契约命令未带 `-CleanFirst`，如实记：不是全量重编） |
| `cargo fmt --all --check` | **exit=0**（`elapsed=1.0s`）。首次跑时只有我新增的 `crates/daemon/tests/version_points.rs` 漂移 3 处；只对该文件跑 `rustfmt` 后归零（**未动他人文件**），随后三条门在**改格式后的字节上重跑**（本表读数即取于此） |

## T127.5 路径选择：**不加访问器**（captain 裁决第 1 条）

`Distiller` 的字段（`crates/daemon/src/distill.rs` 的 `pub struct Distiller`）**全部是 `pub`**，`compose_prompt` 是 `pub(crate)` ⇒ 在 `api.rs` 里**可以直接构造并调用**，因此**不允许**为「让改动看起来更小」而加 `pub(crate)` 访问器（那会为了小改动牺牲同源）。**结论：`crates/daemon/src/distill.rs` 零字节改动**；另一条路（加访问器）不可行，因为它的前提「存在私有字段且无构造器」不成立。

## T127.6 本单不覆盖（第 19 条）

- **不覆盖** `/distill` 的 **PUT 半边**：本单只用 GET（`distill_policy_get`），PUT（`distill_policy_put`）与「返回非 JSON 的那条 policy 路径」（t62 §2 已记）本单未验证、未改动。
- **不覆盖** `prompt_hash` 在**真实蒸馏路径**上是否与 api 侧读到的一致：**未测**。原因：真实路径入口是 `Distiller::distill_plan`（session close / 手动路由），需要一次真实会话 + 脚本化 mock agent，并读 `distill_log.prompt_hash` 与同一策略下的 api 侧值对照；本单只证明两侧**同函数、同输入**（api 侧复用 `compose_prompt`）。**需要什么**：一条临时 root 的端到端探针（`--behavior scripted`）+ `distill_log` 行读取。
- **不覆盖** `scoring_version` 的语义正确性（它是否「今天该报的那个版本」）：本单只把它从单一来源搬到 wire。

## T127.7 修订记录（带日期，旧文字逐字引用）

- **2026-10-04 · 坐标更正（captain 在 t127 回读后指出，三处中一处是我自己推走的）**：
  - 交接原文「`pub fn embedder_name(&self)` 定义 `crates/store/src/store.rs:625`」→ **更正为 `crates/knowledge/src/store.rs` 的 `pub fn embedder_name(&self) -> &'static str`**（`crates/store/src/` 只有 `fts.rs` `lib.rs` `migrations.rs` `sqlite.rs` `transcript.rs`，**无 `store.rs`** ⇒ 路径错，不是行漂）。
  - 交接原文「`crates/knowledge/src/lib.rs:21`」→ **更正为 `:24`**（今天是 `SCORING_VERSION, ScoreKind, …`；`:21` 是 `pub use store::{` 的旧值 ⇒ 交接来的坐标是**声称**不是事实）。
  - 交接原文「already read by `/api/v1/knowledge/documents`, api.rs:648」→ **更正为 `api.rs` 中 `knowledge_documents` 里的同一 accessor**（行号已移；**成因是我自己在 F4 注释处插入 +16 行**，把下面被引用过的行号推走了 ⇒ 自伤型漂移）。
  - **引用方式（写进本单）**：① 能按符号引就按符号引（`state.knowledge.embedder_name()` / `ruagent_knowledge::SCORING_VERSION` / 函数名）；② 若必须给行号，在**最后一次编辑之后**回读；③ 从交接/文档抄来的坐标一律当**声称**处理，先读再写（§T127.1/§T127.2 的注释已按此改写）。
- **2026-10-04 · 负控口径**：本单**未在共享树上做任何变异**；两条负控都是**真实响应 JSON 的临时副本**（删键 / 改键名 / 换字面量）。**这不证明** HTTP handler 自身会红 —— 那需要在隔离树里改源码再构建；本会话预算不足，**未测**，需要一条隔离 worktree + 自带 `CARGO_TARGET_DIR` 的构建。
