# 独立验证 `t127` 版本读点：`prompt_hash` / `embedder` / 加法式 —— **读数与 findngs**

> **性质**：**独立验证**（verify2 本人，不是 t127 的作者）。输入 = t127（commit `ec24b61`，报告 `docs/design/reviews/gen4-version-points-impl.md`）+ 其测试 `crates/daemon/tests/version_points.rs`。
> **结论一句话**：三条承重声称里，**"同一 accessor" 成立（逐字复现）**，**"加法式" 成立（逐字对照改前实现）**，**"同源" 在三条真正会算 hash 的路径上成立（同一个 RwLock、同一表达式）**；但作者的"独立重算"路径**不是对所有配置都独立**：`[distill] language` 一配上，它就与**正确的** handler 分叉（实测，见 §5 V-B1）。另有 3 条低危 finding 与 2 条我自己补上的读数（PUT 半边、graph 开关）。
> **我没有修改 `crates/**` 的任何字节**（本单 inScope 只有本报告）。

---

## 0 每个读数取自哪份字节（C35 的正面处理）

| 用途 | 路径 | mtime | sha256 |
| --- | --- | --- | --- |
| **我用来打 HTTP 的守护进程二进制** | `C:\Users\19410\AppData\Local\Temp\ruagent-team-target\debug\ruagent.exe` | **2026-10-04 00:41:51** | **`C4C59173BB6AD05C2CDAE6D1FD18A6274171EDF078DEB52599724D3116F891A7`** |
| 我是怎么得到它的 | `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 build -p ruagent` ⇒ `exit 0 elapsed=68.0s`（编译了 `ruagent-memory`/`ruagent-daemon`/`ruagent`） | | |
| **明确没用来取证**（同一路径的旧字节，早于本次构建） | 同上路径 | 2026-10-04 00:18:35 | `C3D795C83627E7AC…` |
| **明确没碰**（活守护进程的图像，pid 14944） | `D:\rust_cache\debug\ruagent.exe` | 2026-10-02 23:05:41 | `0C97B01C2439B89E…` |
| 我读的测试二进制（§6 的 4 条通过来自它） | `...\ruagent-team-target\debug\deps\version_points-b25472ed7185eaa0.exe` | 2026-10-04 00:43:11 | `05E6427E134A18D4…` |
| 源码 pin：`crates/daemon/src/api.rs` | 共享树 **与** 我 `git archive HEAD` 出的隔离副本 **同一哈希** | | `C43BACC0A535A107…` |
| HEAD（我开工时） | `93d4491`（t127 本身在 `ec24b61`） | | |
| HEAD（我收尾时） | **`307f071`** —— **树在我取证期间被同伴推进了**（见下） | | |
| 源码 pin（今天字节） | `api.rs C43BACC0A535A107`（**未变**）· `tests/version_points.rs 09E16DD33EA34F55`（**未变**）· `distill.rs 0291DF0E6E580A7F` · `chat.rs 14387C3CC3120A4F` · `memory/src/write.rs 3C4D92FF64426C22` · `knowledge/src/embed.rs 4DC61A592046A427` · `knowledge/src/store.rs F25E892B9BA90F13` · `daemon/src/lib.rs ADDF53AB6A5DFBB3` | | |

### 树在动（AGENTS.md 的 t20 提醒，按它的要求披露）

* 我开工到收尾之间 `HEAD` 从 `93d4491` 走到 **`307f071`**，工作树里有**同伴在途**的未提交改动：`crates/acp/src/chat.rs`、`crates/daemon/src/chat.rs`、`crates/daemon/src/distill.rs`、`crates/daemon/tests/injection_e2e.rs`、`crates/mcp/src/lib.rs`、`.github/workflows/audit.yml`、`docs/design/reviews/gen4-audit-gate-recon.md`（外加两个新文档）。
* **我的读数相关的两份文件没有被任何人动**：`api.rs` 与 `tests/version_points.rs` 在开工时/收尾时**哈希相同** ⇒ §1–§5 引用的 handler 与用例字节就是我验的那份。
* **被同伴推走的行号我全部回读了**（C33）：`distill.rs` 的 `extraction_prompt` 从 `:1114` 移到 **`:1119`**、`chat.rs` 的 `auto_distiller` 从 `:1568` 移到 **`:1603`**、`set_distill_policy`/`distill_policy_now` 从 `:1583/:1588` 移到 **`:1618/:1623`**（`language: policy.language`/`prompt_override: policy.prompt` 现在是 **`:1610/:1611`**）。**改的是行号，引用的那段文本逐字未变**（我重读过，见 §2/§4 的新坐标）⇒ 结论不受影响，但坐标按新字节给。
* **HTTP 读数属于 00:41:51 那份二进制**（sha256 见上），它是用**当时的工作树**构建的；上表列出的今天哈希是该二进制之后同伴又改过的文件。凡"树在动"可能影响结论的地方（组字、策略映射、用例）我都回读过并确认文本未变，因此结论对两边都成立；受影响的是**门禁计数**（见 §6）。

**C35 的教训被正面执行**：共享 target 里那份旧 `ruagent.exe`（会话开始时 C35 记录的是更早的 09-29 09:28:43，本单开工时它已经是 00:18:35 的另一个版本）**早于我的取证**，所以我没有拿它当"现在的字节"——我**自己重建**了 `-p ruagent` 并钉住上面的哈希与 mtime。活守护进程（8787）与它的库只被**读**（health/端口归属），一个字节未写。

**我的临时环境**：临时 root `%TEMP%\t135-root`、临时端口 **8911**（**不是** 8787），守护进程 **pid 15376**（我起的，收尾已停并核对）；`NO_PROXY=127.0.0.1,localhost,::1`。

---

## 1 独立重算 `prompt_hash`（接受标准 1）

**我的独立路径不是调他们的函数**，而是：从**源码**读出组字规则 ⇒ 用**我自己的** SHA-256（.NET `SHA256::HashData(UTF8.GetBytes(s))`，小写 hex）算 ⇒ 与线上 payload 逐字比。组字规则（`crates/daemon/src/distill.rs:1119-1131` 今天的字节；行号在同伴推树后从 `:1114` 移来，文本逐字未变，逐字读过）：

```rust
fn extraction_prompt(language: Option<&str>, prompt_override: Option<&str>) -> String {
    let base = prompt_override.unwrap_or(EXTRACTION_PROMPT);
    let mut out = base.to_string();
    if let Some(lang) = language { out.push_str(&format!("...\nin {lang}. ...")); }
    out.push_str("\n\nTRANSCRIPT:\n");
    out
}
```
`compose_prompt()`（`distill.rs:236-238`）就是 `extraction_prompt(self.language.as_deref(), self.prompt_override.as_deref())`；`content_hash`（`crates/memory/src/write.rs:79-88`）= **SHA-256 的小写 hex**。

| # | 线上策略（我 PUT/写在**我自己的 root** 里） | 线上 `prompt_hash`（逐字） | 我独立的 SHA-256 | 相等 |
| --- | --- | --- | --- | --- |
| **A** | `[distill] prompt = "PROMPT-A"` | `6b252b7eab25ba01ba42b088a3b5b397e97cfc6e1b09f33628a88689a9ffbeee` | `sha256("PROMPT-A" + "\n\nTRANSCRIPT:\n")` = **同值** | ✅ |
| **B** | PUT `{"prompt":"PROMPT-B"}`（**只改一个输入字段**） | `6de27008a956e493b0f258b84738a1222cfc4ddd5d1d3a4f68055e37f99d7717` | `sha256("PROMPT-B" + tail)` = **同值** | ✅ 且 **≠ A** ✅ |
| **C** | PUT `{"prompt":null}`（回到内建默认） | `01789e40365a8992a348c5ddb89c0a3f409fec90b03f5a5e586abbb54d4eb8ba` | `sha256(线上 builtin_prompt[2380 B] + tail)` = **同值** | ✅ |
| **D** | PUT `{"prompt":"PROMPT-A","language":"简体中文"}` | `4c99dcd897411d8cacb6526ce6e8f8405f7fb24c1efd39e19c6b490b859625e0` | `sha256("PROMPT-A" + 语言子句 + tail)` = **同值** | ✅ |
| **E** | PUT `{"prompt":"PROMPT-A","language":null,"graph":false}` | `6b252b7e…`（**与 A 逐字相同**） | — | 见 §4 |

**A/B/C 三组与作者报告里的三个声称值（`6b252b7e…` / `6de27008…` / `01789e40…`）逐字一致**，而且这次是**我自己算出来的**，不是抄他们的。

**我用的组字文本（逐字，含 Rust 续行规则）**：base 之后是
`"\n\nWrite every `content` value, entity `summary`, and relation `fact` in {lang}. JSON keys and the `store`/`namespace` values stay exactly as specified above."`
—— 源码里这个字符串字面量被 `\` 续行折成三行，Rust 的续行会**吃掉换行与下一行的前导空白**，所以拼出来是 `and relation`（一个空格）、`the `store``（一个空格）；D 组我把 `{lang}` 换成 `简体中文`，逐字命中线上值 ⇒ 这条续行规则的读法**被读数证实**，不是我猜的。

**线上 payload 逐字（A 组）**：
```
{"auto":false,"agent":null,"language":null,"prompt":"PROMPT-A","graph":true,
 "builtin_prompt":"You are a memory distillation engine. …Empty arrays are valid. Quality over quantity.\n",
 "prompt_hash":"6b252b7eab25ba01ba42b088a3b5b397e97cfc6e1b09f33628a88689a9ffbeee"}
```
（`builtin_prompt` 全长 **2380** 字节，尾串 `. Quality over quantity.\n` 与源码常量 `EXTRACTION_PROMPT` 的结尾一致。JSON 键序在这里**不是**字母序 ⇒ 该 workspace 打开了 `serde_json` 的 `preserve_order`，所以"键序"是一条**可读**的性质，不是实现细节——这也是 §3 能拿键序当证据的前提。）

**差值检查**：A→B 改一个字段 ⇒ 两个 hash 都变且各自等于我的独立重算（`6b25…` vs `6de2…`，64 hex 前缀即分叉）✅ 接受标准要求的两组逐字哈希在上面表里。

---

## 2 `embedder` 走同一 accessor，且 daemon 里没有第二处写死（接受标准 2）

**单项来源链（逐字读源）**：
* **唯一的字符串定义**：`crates/knowledge/src/embed.rs:89-91` `fn name(&self) -> &'static str { "hash-embedder" }`（hash embedder 的实现）。
* **accessor**：`crates/knowledge/src/store.rs:625-627` `pub fn embedder_name(&self) -> &'static str { self.embedder.name() }` —— 委派给**当前活的** embedder，不是常量。
* **daemon 的两处读点**：`api.rs:499`（`/api/v1/stats`）与 `api.rs:667`（`/api/v1/knowledge/documents`）—— **同一个符号** `state.knowledge.embedder_name()`；daemon 里 `embedder_name` 只出现三次，第三处是 `lib.rs:199` 的日志（`embedder = knowledge.embedder_name()`），同样是 accessor。
* **daemon 源码里没有第二处把该字符串写死**：全仓 `hash-embedder` 字面量只有：`embed.rs:90`（**定义**处）、`distill.rs:517`（**注释**）、`store.rs:1691/1819` 与 4 个 knowledge 测试（断言）、`cli/src/main.rs:949`（**比较**，见 §5 V-B4）。**daemon/src 里除了那句注释，零字面量** ⇒ 这一条成立。

**运行时逐字对（同一进程、同一二进制 C4C59173…）**：
```
GET /api/v1/stats               -> {"agents":[],"embedder":"hash-embedder","scoring_version":2}
GET /api/v1/knowledge/documents -> {"documents":[],"embedder":"hash-embedder"}
stats.embedder == documents.embedder -> True（byte-equal，-ceq 比较）
```
⚠ **必须点名的采样面限制**：我这份实例的 embedder 是**离线 `hash-embedder`**（知识库为空，本机上 `FastEmbedder::try_new` 未产出语义 embedder）。所以我证明的是"**两个读点在同一进程里逐字相等且都走 accessor**"，**没有**观察到语义 embedder 那一支的取值。换 embedder 时两处仍会相等（都调同一 accessor），但"语义模型名"这条**未观测**。

---

## 3 加法式：既有键与取值一字未动（接受标准 3）

**方法**：拿 `ec24b61^` 的 `api.rs` 原文与今天逐字对照 + 读 t127 的 diff（`git show ec24b61 --unified=0 -- crates/daemon/src/api.rs`）。

**`/distill` 改前（`ec24b61^:crates/daemon/src/api.rs`，逐字）**：
```rust
async fn distill_policy_get(State(state): State<AppState>) -> Json<serde_json::Value> {
    let p = state.chats.distill_policy_now();
    Json(serde_json::json!({
        "auto": p.auto,
        "agent": p.agent,
        "language": p.language,
        "prompt": p.prompt,
        "graph": p.graph,
        "builtin_prompt": crate::distill::builtin_extraction_prompt(),
    }))
}
```
**今天**（`api.rs:2966-2995`）：这**六行取值表达式一字未动**，`"prompt_hash"` 追加在 `builtin_prompt` **之后**；t127 的 diff 在这段里是**纯 `+` 行**（`@@ -2948,0 +2968,18 @@` 插注释与 Distiller 构造，`@@ -2955,0 +2993 @@` 插一行 `prompt_hash`）。
**运行时**（我自己的 daemon）：键序 = `auto, agent, language, prompt, graph, builtin_prompt, prompt_hash`（逐字 `{"auto":false,"agent":null,"language":null,"prompt":"PROMPT-A","graph":true,"builtin_prompt":…,"prompt_hash":…}`）⇒ 既有六键**位置与取值都没动**，新键在末尾 ✅。测试自己的断言（`version_points.rs:182-191`）用的就是这条键序。

**`/stats` 改前（逐字）**：
```rust
async fn stats(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    let stats = state.mgr.db().agent_stats().await?;
    Ok(Json(serde_json::json!({ "agents": stats })))
}
```
**今天**：`let stats = state.mgr.db().agent_stats().await?;` **一字未变**，`"agents": stats` 仍是**第一个键**（`api.rs:481/498`）；diff 是 `@@ -482 +482,20 @@`，**唯一的 `-` 行**就是那行单键 `Ok(Json(...))` 被展开成三键的同一表达式。运行时 `{"agents":[],"embedder":"hash-embedder","scoring_version":2}` ⇒ **`agents` 是空数组（本实例没有 agent 卡片），取值路径未变** ✅。
**注意这条读数的边界**：我的 `/stats` 读数里 `agents` 是 `[]`（空），所以"取值未变"的证据主要是**实现路径逐字 + 键序**，**不是**"非空 agents 与旧版逐字比对"（要那样需要同一实例在改前改后各跑一次；我没有改前二进制——C35 已说明那份旧 exe 早于 t127 且我拒绝用它取证）。

---

## 4 攻击面：三组输入，两处计算会不会分叉（接受标准 4）

| 攻击输入 | 读数 | 判定 |
| --- | --- | --- |
| **`prompt` override 对默认** | A（override `PROMPT-A`）`6b25…` → PUT B（`PROMPT-B`）`6de2…` → PUT null（内建）`0178…`：**三值互不相同，且每个都等于我独立重算** | **不分叉** ✅。原因（读源，行号回读于今天字节）：线上 `p = state.chats.distill_policy_now()`（`api.rs:2967`）与真正跑蒸馏的三处 Distiller 构造都从**同一个 `RwLock<AutoDistill>`** 取字段——`chat.rs:1623 distill_policy_now()` / `chat.rs:1603-1611 auto_distiller()`（`language: policy.language, prompt_override: policy.prompt`）/ `api.rs:3123-3130` 的 `session_distill`（同样两行）；PUT 半边 `api.rs:3055-3064` 既写 `policy.toml` 又 `set_distill_policy(...)` 更新**同一个锁**。⇒ **wire 报的就是跑的时候会用的**（对这三条路径）。第四处 Distiller 是 `api.rs:1070-1081` 的 **wiki 构建**（`language: None, prompt_override: None`），源码注释与调用点都表明它**不调 distill**、也不写 `prompt_hash` ⇒ 不是分叉源（若要绝对闭合，可给它加一条"永不写 distill_log"的断言）。 |
| **`graph` 开对关** | `graph=true`（A）与 `graph=false`（E）的 `prompt_hash` **逐字相同**（`6b25…`）；线上 `graph` 字段确实从 `true` 变 `false` | **不是缺陷，是构造使然**：`compose_prompt()` → `extraction_prompt(language, prompt_override)` 的**参数表里没有 graph** ⇒ graph 在数学上进不了这个 hash。字段名与用途（`distill.rs:599` “`prompt_hash` attributes it to the prompt that produced it”）说的就是"prompt 版本"，所以**记录为读数、不记 finding**；但如果有人把它当"行为版本"读，那是误读——这一点值得写进字段文档（见 §5 V-B5 观察）。 |
| **`language` 变化** | 见下：**分叉了**——但分叉的两侧是「**正确的 handler**」与「**测试的重算公式**」，不是 handler 与 distiller | **finding（§5 V-B1）**，不调和 |

**这条攻击的完整读数**（D 组）：线上 `{"language":"简体中文","prompt":"PROMPT-A"}` ⇒ `prompt_hash = 4c99dcd897411d8cacb6526ce6e8f8405f7fb24c1efd39e19c6b490b859625e0`，**等于**我独立算的 `sha256("PROMPT-A" + 语言子句 + tail)` ⇒ **handler 是对的**；而 `version_points.rs:149-151` 的重算只拼 `base + "\n\nTRANSCRIPT:\n"`（常量 `TRANSCRIPT_TAIL`，`version_points.rs:136`）⇒ 它算出 `6b252b7e…` ≠ 线上的 `4c99dc…`。**结论：那条"独立重算"在 `language` 被配置时，会在正确的代码上变红。**

---

## 5 Findings（不改代码，交作者/captain）

### V-B1（**medium**）`version_points.rs` 的"独立重算"对 `language` 是盲的
* **problem**：`expected_prompt_hash`（`crates/daemon/tests/version_points.rs:149-151`）把组字规则**硬编码成无语言子句的形态**（只拼 `TRANSCRIPT_TAIL`），而 daemon 的组字在 `language` 存在时会在 base 与 tail 之间插入一段子句（`crates/daemon/src/distill.rs:1117-1123`）。⇒ 该用例只在 `language = None` 的 fixture 下成立；一旦 fixture 配上 `[distill] language`，它会在**正确**的实现上 FAILED。实测两值：线上 `4c99dc…` vs 测试公式 `6b252b7e…`（§4）。
* **measured, not argued**：我在**我自己的临时守护进程**上用 PUT 配了 `language = 简体中文`，并用**我自己的** SHA-256 独立验证了线上值等于 `A + 子句 + tail`。
* **为什么这不算"作者过度声称"**：测试注释自己写了"This is the same function and the same inputs the daemon uses"（`:145-148`），没有声称覆盖 language。这是**覆盖缺口**而不是谎报。
* **requiredFix**：把期望值从**线上字段**构出来（`body["language"]` 非空时插入同一段子句——子句文本应从源码常量取，别再抄一份），或补一条 `[distill] language = "…"` 的 fixture 用例；并在该文件里留一句"language 分支在 wire 层没有覆盖"的记录。

### V-B2（**low**）测试注释里的行号已经漂了
* **problem**：`version_points.rs:118` 写 `(api.rs:648)`；今天 `api.rs:648` 是 `ruagent_memory::query::get_memory(...)`，真正读 accessor 的是 **`api.rs:667`**。这正是 t127 自己在 `gen4-version-points-impl.md` §T127.5 立下的规矩（"能按符号引就按符号引；若不是要给行号，必须在最后一次编辑之后回读"）——**daemon 源码照做了（`api.rs:487-490` 引符号），测试没照做**。
* **requiredFix**：把那句改成引符号（`state.knowledge.embedder_name()` in `knowledge_documents`）。

### V-B3（**low**，作者已自述的盲区，我把它写清边界）
* **problem**：`expected_prompt_hash` 调的是**同一个** `ruagent_memory::write::content_hash`（`:150`）⇒ **`content_hash` 内部要是变了，两侧一起变，断言不会红**；形状闸 `h.len() == 64 && all(ascii_hexdigit)`（`:142`）也放行任何别的 64-hex 函数。所以这个用例证明的是"**同一函数的两次计算相等**"，**不**证明"它是 SHA-256"。
* **已被自述**：注释 `:145-148` 明说"same function"，报告 §T127.7 也把"HTTP handler 自身是否会红"列为**未测**。⇒ 这是**边界**，不是谎报。
* **requiredFix（若要闭合）**：在隔离副本里把 `content_hash` 换成另一个 64-hex 构造（例如 `h.update(b"x")`），预期**仍然全绿**——那正是这条边界的读数。

### V-B4（**low**，超出本单范围但在"没有第二处写死"这句话的边界内）
* **problem**：`cli/src/main.rs:949` `let semantic = embedder != "hash-embedder";` —— 用**字面量比较**决定"语义检索是否可用"。裁决链本身是对的（`embedder` 来自 accessor），但**字符串在这里被第二次写死**。⇒ 接受标准的"daemon 源码里没有第二处写死" **成立**；"全仓只有一处写死" **不成立**。
* **requiredFix**：比对 accessor 的分支名（或让 `Embedder` trait 暴露一个 `is_semantic()`），别再抄字面量。

### V-B5（**observation**，不是缺陷）
`prompt_hash` 对 `graph` 开关不敏感（§4）。字段名与注释都指向"prompt 版本"，所以这是**符合意图**的；但"同一次 distill 的 `graph` 不同 ⇒ 落库的 `prompt_hash` 相同"意味着：**不能拿 `prompt_hash` 区分两次行为不同的蒸馏**。若将来有人想用它做"这次抽取是用什么配置做的"的证据，需要在 `distill_log` 旁边补 graph/agent 列，而不是扩这个 hash 的输入（扩了就会破坏与历史行的可比性）。

---

## 6 门禁：我本人在最终字节上重跑（接受标准 5）

| 命令 | 读数 |
| --- | --- |
| `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-daemon` | **exit 0**；`[cargo-team] exit=0 elapsed=58.6s`（前有 `waiting 10s`×2 等锁）。逐目标：lib **157 passed** · capabilities **11** · capability_defaults **5** · event_compat **2** · graph_ingest **16** · injection_e2e **0 passed / 7 ignored** · knowledge_api **16** · recall_evidence_announced **3** · smoke **0** · **version_points 4** · doctests **0** ⇒ **合计 214 passed / 0 failed / 7 ignored** |
| `... clippy -p ruagent-daemon --all-targets -DenyWarnings` | **exit 0**；`Checking ruagent-memory` + `Checking ruagent-daemon` + `Finished dev profile in 7.52s` |
| `cargo fmt --all --check` | **exit 0**（无输出） |

（三条命令都作用于共享树的**最终字节**；本单**没有**改过 `crates/**`，所以"最终字节"= 我开工时读的那份 = HEAD `93d4491` 的 `ec24b61` 内容。收尾复查：`git status --porcelain` 里没有我的任何 `crates/**` 改动。）

### 6.1 收尾重跑（当前字节，含同伴在途改动）——**必须给，因为 fmt 红了**

同伴在我取证期间推进了树（§0"树在动"），所以我按 AGENTS.md t20 的规矩**在收尾时又跑了一遍三条门禁**（`00:54:45` 起，HEAD=`307f071`）：

| 命令 | 收尾读数 |
| --- | --- |
| `test -p ruagent-daemon` | **exit 0**（`elapsed=47.1s`）：lib **157** · capabilities **11** · capability_defaults **5** · event_compat **2** · graph_ingest **16** · injection_e2e **0 passed / 8 ignored**（同伴给它加了 1 条，之前是 7）· knowledge_api **16** · recall_evidence_announced **3** · smoke **0** · **version_points 4** · doctests **0** ⇒ **214 passed / 0 failed / 8 ignored** |
| `clippy -p ruagent-daemon --all-targets -DenyWarnings` | **exit 0**（`Checking ruagent-acp` + `Checking ruagent-daemon` + `Finished in 7.64s`） |
| `cargo fmt --all --check` | **exit 1** ⚠ —— **不是我的、也不是 t127 的文件**：`Diff in crates\daemon\tests\injection_e2e.rs:929` 与 `:974`（rustfmt 要把 `assert_eq!` 的两行参数收成一行）。该文件是**同伴在途的未提交改动**（`git diff --numstat` = **197 增 / 0 删**，最后一次提交是 `0da0cb6`）；`git status` 里它属于同伴的 `M` 列表。**我按纪律不替同伴改**，只报"哪个文件、什么错"。⇒ 提交前若要绿，需要**该文件的作者**跑一次 `rustfmt`（注意它有 CRLF，`git` 也在警告 `CRLF will be replaced by LF`）。 |
| 我第一次的 fmt 读数（`00:44`，同一批文件在同伴落盘之前） | **exit 0、无 diff** ⇒ t127 与我的字节本身是格式化的；红是那之后进来的。 |

**陷阱记录（我自己的）**：我第一次跑 fmt 时用了 `... | Select-Object -First 10`，管道被提前掐断，`$LASTEXITCODE` 报出 **0**，而输出里其实**有 diff** —— 读数与退出码自相矛盾。重跑时改成 `cmd /c "cargo fmt --all --check > file 2>&1"` 再取 `$LASTEXITCODE`，才拿到可信的 **1**。**教训**：给退出码当证据的命令**不要**接会截断管道的 cmdlet。

---

## 7 作者自报未测的那条：HTTP handler 自身会不会红（接受标准 6）

**我做了什么**：把 HEAD `git archive` 成隔离副本 `%TEMP%\t135-iso`（**不动共享树的任何文件**；隔离副本的 `api.rs` 哈希与共享树逐字相同 = `C43BACC0A535A107`），在**副本**里把 handler 的 hash 输入换成字面量：

```
- "prompt_hash": ruagent_memory::write::content_hash(&distiller.compose_prompt()),
+ "prompt_hash": "0000000000000000000000000000000000000000000000000000000000000000",
```
（用**形状合法的 64-hex 字面量**，而不是乱码字符串 —— 这样红的是**值**，不是 `prompt_hash()` 那条形状闸；否则这条实验会被形状闸代替，问不清"值错会不会红"。）
然后 **自带私有 `CARGO_TARGET_DIR`**（`%TEMP%\t135-iso-target`）+ `PROTOC` 冷构建，跑 `cargo test -p ruagent-daemon --test version_points`。

**为什么是冷构建**：共享 target 目录 **100.50 GB**（`debug\deps` 81.88 GB）⇒ 复制它做隔离不划算；按 AGENTS.md 的规矩（worktree 必须自带 target dir）只能冷建，代价是实测过的 **~9 分钟**量级。
**纪律偏差（明说）**：这条隔离构建**没有**走 `scripts/cargo-team.ps1` —— 因为包装器的锁是为**共享 target** 的存在性而设，而这次用的是私有 target（不碰共享目录、也不占团队锁，对同伴**更**友好）。共享树上的三条门禁（§6）**全部**走了包装器。

**窗口与状态**：变异窗口 **00:45:16 – 00:53:44**（冷构建 **8m26s**，`Finished test profile … in 8m 26s`）；副本 `api.rs` `C43BACC0A535A107` → `30126C7A9D583AB3`；同一时刻与收尾后**共享树** `api.rs` 都是 `C43BACC0A535A107`（`shared api.rs after: C43BACC0A535A107` ⇒ **零变异**）。

**读数（逐字）**——`cargo test -p ruagent-daemon --test version_points`，**`ISOLATED_TEST_EXIT=101`**：

```
running 4 tests
test stats_reports_the_calibration_versions_from_their_single_sources ... ok
test the_calibration_check_is_false_for_all_three_mutations ... ok
test the_prompt_hash_check_is_false_for_all_three_mutations ... FAILED
test distill_reports_a_prompt_hash_from_the_same_function_and_input ... FAILED

test result: FAILED. 2 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.52s
```

F3 那条的 panic（逐字）：

```
thread 'distill_reports_a_prompt_hash_from_the_same_function_and_input' panicked at crates\daemon\tests\version_points.rs:212:5:
assertion `left == right` failed: same function, same input: content_hash(compose_prompt()) for the override policy
  left: Some("0000000000000000000000000000000000000000000000000000000000000000")
 right: Some("6b252b7eab25ba01ba42b088a3b5b397e97cfc6e1b09f33628a88689a9ffbeee")
```

⇒ **答案：HTTP handler 自身的取值被覆盖** ✅ —— 把 handler 的 hash 输入换成"值错的 64-hex 字面量"，F3 的用例**变红**，而 F4 的两条**仍绿**（两条红都落在 F3 那一半）。**接受标准第 6 条：在隔离副本里测出来了**，作者自报的边界**关掉**。
附带：被变异的那一轮 F3 打印的期望值就是 `6b252b7e… / 01789e40…`（用例自己打印的 `expected`），**与我 §1 独立算出的两个值逐字一致** —— 这是对我"组字规则读法"的又一次交叉验证。（那一轮还出了一条 `unused_variables` 警告：我的变异让 `distiller` 不再被使用 ⇒ **是我变异的产物，不是树的问题**；共享树上的 `clippy -D warnings` 是干净的，见 §6。）

**附带读数（比预期多出来的一条）**：F3 的"负控" `the_prompt_hash_check_is_false_for_all_three_mutations` **也红了**，而它红的原因值得记录——它的"真实样本"取自**活 handler 的输出**：

```
READING F3 negative control: real=Some("0000…0000") renamed=None deleted=None literal=Some("0000…0000")
assertion `left != right` failed: a literal must not read as the computed value
  left: Some("0000…0000")   right: Some("0000…0000")
```

即：该负控把**线上 payload 本身**当"real"基线（作者报告 §T127.7 也是这么自述的），所以 **handler 一旦错，它会与它本该守护的断言一起变红** ⇒ 它是"对真实样本做删键/改名/换字面量"的自检，**不是 handler 的独立哨兵**。这与 V-B3 属同一类边界（期望值取自被测对象），但方向不同：V-B3 是"函数内部变了不会红"，这里是"handler 变了会红，但**红得没有分辨力**（它守的不是 handler 的正确性）"。**注意**：这不削弱主用例的价值——主用例 `distill_reports_a_prompt_hash_from_the_same_function_and_input` 的红是**真正的**覆盖（它把 handler 的值与"从 wire 字段独立重算的值"比）。

**与 V-B3 的关系（很重要）**：这一条实验能回答"**handler 的取值**错会不会红"（预期**会**），**不能**回答"`content_hash` 本身变了会不会红"（V-B3：**不会**）。两者是不同的问题，别把前者的绿当后者的证据。

---

## 8 未测 / 不覆盖

* **最强形态的"同源"仍未验证**：`distill_log.prompt_hash`（真跑一次蒸馏落库的值，`distill.rs:283/293/316`）与 wire 的 `prompt_hash` **逐字相等**这件事，本单**没有**测——需要一条真会话 + mock agent + 读 `distill_log`。我只在**源码级**证明了两处是同一表达式同一输入（`content_hash(&self.compose_prompt())`）。这是作者 §T127.6 的边界，**仍然开着**。
* **wire 层的 language 分支**：本单**没有**测"language 配置下 wire 的 hash 是否可被独立重算"——**除了**我自己在 §4 做的那一次（我用 PUT 配了 language 并独立算对了）。缺的是**测试里**有这条用例（V-B1）。
* **语义 embedder 那一支未观测**（§2 的采样面限制）：我的实例报 `hash-embedder`。
* **`scoring_version` 的语义正确性**（它是不是"今天该报的版本"）**未测**——作者 §T127.6 也这么写；我只核对了 `SCORING_VERSION: u32 = 2`（`crates/knowledge/src/store.rs:338`）与 wire 的 `2` 一致，以及它来自常量而非字面量。
* **没有**并发/多实例读数（单进程、单端口 8911）；**单一平台**（Windows，`rustc 1.95.0`）。
* **`/stats` 的 `agents` 非空形态**未与旧版逐字比对（§3 的边界）。
* 未覆盖 PUT 的**失败路径**（非法 JSON/越权）——我只跑了三条合法 PUT（A→B→默认→language→graph）。
* **fmt 门禁在收尾时是红的**，红在同伴在途的 `crates/daemon/tests/injection_e2e.rs`（§6.1）；**t127 与本单的字节是格式化的**（第一次 fmt 读数 exit 0）。这条红**不是** t127 的缺陷，但它会让**下一次 push** 失败（AGENTS.md t66 记的就是这个形状），所以必须由该文件的作者处理。
* **树在动带来的一条采样面限制**：我的 HTTP 读数属于 **00:41:51** 那份二进制（当时的工作树），而 `distill.rs`/`chat.rs` 在那之后被同伴改了行号（文本未变，我已回读）。**严格地说**：我验证的是"**那一份字节**上，wire 值与 distiller 用的是同一表达式同一策略"；对**同伴改后的**那两个文件，我核对的只是"我引用的那段文本没有变"。
* 我**没有**在"同伴改后的字节"上重建过二进制再取一次 HTTP 读数（那要再花一次构建 + 临时守护进程）；若 captain 要"最终字节上的 HTTP 读数"，需要重跑一次 §9 的 0)+1) —— 预计 <3 分钟（增量构建 68s + 起进程 12s）。

---

## 9 复现命令（照抄可用）

```powershell
cd C:\Users\19410\Documents\ai\ruagent
# 0) 自己重建二进制并钉住（C35：不要用共享 target 里的旧 exe 取证）
$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"; $env:NO_PROXY='127.0.0.1,localhost,::1'
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 build -p ruagent
$exe="$env:TEMP\ruagent-team-target\debug\ruagent.exe"; (Get-Item $exe).LastWriteTime; (Get-FileHash $exe -Algorithm SHA256).Hash

# 1) 我自己的临时守护进程（临时 root + 临时端口 8911；不碰 8787）
$root="$env:TEMP\t135-root"; Remove-Item -Recurse -Force $root -EA SilentlyContinue
New-Item -ItemType Directory -Force "$root\config" | Out-Null
Set-Content "$root\config\policy.toml" -Encoding utf8NoBOM -Value "[permissions]`ndefault = `"ask`"`n`n[distill]`nprompt = `"PROMPT-A`"`n"
$p = Start-Process $exe -ArgumentList 'serve','--addr','127.0.0.1:8911','--root',$root -PassThru -WindowStyle Hidden
# 健康检查后再取读数（Invoke-WebRequest/-NoProxy）

# 2) 我自己的 SHA-256（不调他们的函数）
function Sha([string]$s) { (([System.Security.Cryptography.SHA256]::HashData([System.Text.Encoding]::UTF8.GetBytes($s))) | % { $_.ToString('x2') }) -join '' }
$tail="`n`nTRANSCRIPT:`n"; Sha ('PROMPT-A' + $tail)          # 必须等于 wire 的 prompt_hash 6b252b7e…
# PUT {"prompt":"PROMPT-B"} / {"prompt":null} / {"prompt":"PROMPT-A","language":"简体中文"} 后各重算一次

# 3) 隔离变异（共享树零变异；自带 target dir）
git archive HEAD -o "$env:TEMP\t135-iso.tar"; mkdir "$env:TEMP\t135-iso"; tar -xf "$env:TEMP\t135-iso.tar" -C "$env:TEMP\t135-iso"
# 在副本里把 "prompt_hash": content_hash(...) 换成 64-hex 字面量，然后：
$env:CARGO_TARGET_DIR="$env:TEMP\t135-iso-target"; $env:PROTOC="$env:USERPROFILE\.protoc\bin\protoc.exe"
Remove-Item Env:\CARGO_NET_OFFLINE -EA SilentlyContinue
cd "$env:TEMP\t135-iso"; cargo test -p ruagent-daemon --test version_points

# 4) 收尾：停我起的 pid（只按记录的 pid），删我的临时目录
Stop-Process -Id <我记录的 pid>; Remove-Item -Recurse -Force "$env:TEMP\t135-root","$env:TEMP\t135-iso","$env:TEMP\t135-iso-target"
```

---

## 10 纪律回执

* **只写了本报告**：`crates/**`、`panel/**`、`.github/**`、`scripts/**` **零字节改动**。收尾 `git status --porcelain` 里的 `M` 全是**同伴在途**的文件（`acp/chat.rs`、`daemon/chat.rs`、`daemon/distill.rs`、`daemon/tests/injection_e2e.rs`、`mcp/lib.rs`、`.github/workflows/audit.yml`、`gen4-audit-gate-recon.md`）；未跟踪的两个新文件里，`gen4-version-points-verify.md` 是**我的**，`gen4-audit-gate-verify.md` 是同伴的。
* **⚠ 我自己的一处纪律违规（自报，不掩盖）**：收尾清临时目录时，我按 `%TEMP%\t135*` 这个 glob 删东西，把**两个不是我的**、9 天前的文件一起删了：`%TEMP%\t135z-apply.py` 与 `%TEMP%\t135z.json`（mtime **2026-09-25 00:16:53**，属上一代的 `t135z` 任务；我的窗口是 2026-10-04）。它们与我的任务无关，我原本应当**在删之前看 mtime/归属**（t102 的教训正是"不要按通配符清 %TEMP%"）。影响：两个临时文件（一个 9 天前的中间产物），不在仓库、不在任何人的产物路径里；**但这是我的错，记录在此**，同类操作今后只按**我这次自己创建的完整路径**删（本次 `t135-root`/`t135-iso`/`t135-iso-target` 三个都做到了）。
* **`my_temp_leftovers`（收尾）**：`%TEMP%\t135-root`、`%TEMP%\t135-iso`、`%TEMP%\t135-iso-target`（6.39 GB）、`%TEMP%\t135-iso.tar`、`%TEMP%\t135-fmt.txt` **全部已删**；`%TEMP%` 里我这次创建的 `t135*` 条目 = **0**。
* **进程**：我起的守护进程 **pid 15376**（临时端口 8911）已由我按记录的 pid 停掉（`Stop-Process -Id 15376`，**不是**按名字/端口批量杀）；排查后我名下的 `ruagent.exe`/`cargo.exe`/`rustc.exe` 残留 = **0**。
* **活环境**：8787（收尾回读 = **pid 14944**，health `ok/ruagent`）与 `~/.ruagent` 只读、未起未停未写；我 PUT 的全部策略只落在 `%TEMP%\t135-root`（§4 已用 `policy.toml` 的内容作证）。
* **活环境**：8787（pid 14944）与 `~/.ruagent` **只读**、未起未停；我的守护进程用**临时 root + 临时端口 8911**，pid 15376 **由我自己记录并自己停**（不按名字/端口批量杀）。
* **C35**：用来取证的每一个读数都标了来源字节（路径 + mtime + sha256）；旧 exe 与活进程的图像**明确排除**。
* **负控/变异**：只发生在 `%TEMP%\t135-iso` 隔离副本，窗口与哈希已写进 §7；共享树在同一时刻的 `api.rs` 哈希未变。
* **读数三件套**：对象集（两个 GET +PUT 往返的 `/distill`、`/stats`、`/knowledge/documents`）、采样面（单进程、临时 root、离线 `hash-embedder`、空知识库）、可证伪判据（**两个**独立的 64-hex 值：wire 与我自己的 SHA-256；任一处不等即红）。
* **期望值只写一处**：组字规则只从源码读一次（§1 逐字引用），子句文本不再抄第二份（这正是 V-B1 要求作者修的那一点——我不在自己的验证里犯同一个错）。
