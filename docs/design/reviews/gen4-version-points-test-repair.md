# t142 修复：让 `version_points` 的「独立重算」在 **`language`** 维上有判别力（收口 t135 的 V-B1）

> **性质**：**修复**（repair），作者 = verify2（**同时是 t135 的验证者**：这条 finding 是我自己提的，修复后它需要**独立评审**，我不给自己发 pass）。
> **输入**：t135 的 V-B1（medium）+ captain 对契约的精化（"它不是「失去判别力」，而是**在正确的代码上会变红**"）。
> **结论一句话**：把期望值从**源码里的组字规格**读出来（`include_str!("../src/distill.rs")` + 一个转义还原器），并把语言**取值**从线上的 `language` 字段取；于是配上 `language` 时两边逐字一致（`4c99dcd8…`），**改一个由隔离副本模拟的「distiller 忽略 language」⇒ 新用例红、旧用例仍绿** —— 这条轴现在有判别力了。
> **写入集合**：`crates/daemon/tests/version_points.rs`（唯一改动的源文件）+ 本报告。`crates/daemon/src/**`、`crates/knowledge/**`、`crates/mcp/**`、`panel/**`、`.github/**`、`scripts/**` **零字节改动**。

---

## 0 Pins / 每个读数取自哪份字节（C35）

| 项 | 值 |
| --- | --- |
| HEAD（开工） | `307f071` → （期间同伴连续提交）→ **`498170a`**（收尾） |
| 我改的文件 改前 | `git HEAD:crates/daemon/tests/version_points.rs` = **303 行 / 4 条测试 / 12406 字符**（= t127 的版本） |
| 我改的文件 改后 | **477 行 / 5 条测试 / 19337 字节**；`git diff --numstat` = **187 增 / 13 删** |
| 本单的读数来自 | **测试框架**（`cargo test`）——共享 target 里的测试二进制 `%TEMP%\ruagent-team-target\debug\deps\version_points-b25472ed7185eaa0.exe`（mtime **2026-10-04 01:01:07**，sha256 `59C7BD76CBAFBF749186A995…`），它 **postdates** 我改后文件的 mtime（**01:00:54**）⇒ 那次绿读数属于**最终字节**。**本单没有为读数重建过 `ruagent.exe`**（不需要：改的是测试，不是二进制行为）。⇒ C35 的"旧 exe"陷阱在本单不存在，但**隔离实验**用了一份私有 target 目录，见 §3 |
| 我的文件 改后 | sha256（前 24 hex）= **`48E81C4E29A476A554DFC8D2`**（隔离副本里的同名文件与它**逐字节相同** ⇒ §3 引用的行号对最终字节有效） |
| C33 坐标回读 | 两处 panic 坐标 `version_points.rs:303` / `:385` 都是 `assert_eq!(` 的**起始行**（消息字符串在 `:309` / `:388`），已在最终字节上回读确认 |
| 唯一一次隔离构建 | `%TEMP%\t142-iso-target`（私有，**不碰共享 target**）；冷建 **7m55s**（`Finished test profile … in 7m 55s`），第二次变异是增量 **20.36s** |
| 共享树零变异 | 隔离副本 `api.rs` 被变异过（`C43BACC0A535A107` → `30126C7A9D583AB3`）又被**还原**回 `C43BACC0A535A107`；共享树 `api.rs` 全程 `C43BACC0A535A107`；隔离副本 `distill.rs` `0291DF0E6E580A7F` → `B0AF5F9F1F3CBD7F`，共享树 `distill.rs` 全程 `0291DF0E6E580A7F` |

---

## 1 修法（二选一：我选「从线上的 `language` 字段构出」，并说明依据）

**V-B1 的精化事实**（t135 实测）：配上 `[distill] language = "简体中文"` 时，**线上** `prompt_hash = 4c99dcd897411d8cacb6526ce6e8f8405f7fb24c1efd39e19c6b490b859625e0`（我用**独立的** SHA-256 证明它就是 `content_hash(base + 语言子句 + tail)`，**即 handler 是对的**），而文件里那条"独立重算"只拼 `base + "\n\nTRANSCRIPT:\n"` ⇒ 算 `6b252b7eab25ba01ba42b088a3b5b397e97cfc6e1b09f33628a88689a9ffbeee` ⇒ **它会在正确代码上变红**。⇒ 修法必须让期望值随 `language` 走。

**选择的形状**：**语言取值从线上字段取，语言子句的文本从 `distill.rs` 源码取。**
* 为什么**不能**整个从线上取：wire 只报 `language`（取值）与 `prompt_hash`（组字结果的 hash），**它不报子句文本** ⇒ "从线上字段构出"只能覆盖*取值*那一半。
* 为什么子句文本**从源码读**而不是抄一份：抄一份就是**第二份规格**——正是 V-B1 的成因（一个"看起来独立、其实与规格有三处不同步可能"的副本）。t135 的 requiredFix 原话就是"子句文本从源码取，别抄第二份"。于是：

```rust
const DISTILL_SRC: &str = include_str!("../src/distill.rs");   // ← 规格的唯一来源

fn literal_after(marker: &str) -> &'static str { /* 从 marker 起，到第一个「未被转义」的 " 为止 */ }
fn unescape(raw: &str) -> String { /* \n \t \r \" \\ 以及「行尾 \」续行（吃掉换行+下一行前导空白） */ }
fn language_clause(language: &str) -> String { /* 形状断言 + {lang} 替换 */ }
fn transcript_tail() -> String { /* 同一来源读尾部 */ }

fn expected_prompt_hash(base: &str, language: Option<&str>) -> String {
    let clause = language.map(language_clause).unwrap_or_default();
    ruagent_memory::write::content_hash(&format!("{base}{clause}{}", transcript_tail()))
}
```
* **续行规则是规格的一部分**（源码里子句字面量折了三行）：Rust 的 `\`+行尾会**吃掉换行与下一行前导空白**，所以拼出来是 `and relation`（一个空格）。t135 我在 PowerShell 里手工套同一条规则算出的值**等于**线上值；这次由代码套同一条规则，同样的值 ⇒ 规则读法被两次独立证实。
* **坏输入必须响**：marker 找不到 ⇒ `panic!` 并给出"重读 `extraction_prompt` 再改这里、**不要**换成硬编码副本"的指令；子句形状不符/不含 `{lang}` ⇒ 断言失败。**不设静默回退**——静默回退正是 V-B1 的形状。
* **没选的另一条**（补一条 language fixture，子句硬编码在测试里）：更便宜，但它把"第二份规格"留在文件里 ⇒ 下次规格一动又会不同步。若评审认为 `include_str!` 太重，这是可接受的退路，**但需要在测试里写明"这是规格副本"**。
* 顺带把**旧的 `const TRANSCRIPT_TAIL`** 删掉了（改由 `transcript_tail()` 从源码读）：它和子句是**同一类**副本（只是当时恰好没漂），留着就是把同一个缺陷留一半。这让 diff 的 13 行删除里有 3 行是它（§5 逐条列出）。

---

## 2 新轴的逐字读数（共享树、测试框架、5 条测试全绿）

```
running 5 tests
READING F3 language axis: wire none=Some("6b252b7eab25ba01ba42b088a3b5b397e97cfc6e1b09f33628a88689a9ffbeee")
                              zh=Some("4c99dcd897411d8cacb6526ce6e8f8405f7fb24c1efd39e19c6b490b859625e0")
                              en=Some("80a88736b0fca0535e312b5fd4c12b67823ad8ad57ab0f9dc04a1652e4749a39")
| recomposed from the source's composition + the wire's language:
  none=6b252b7eab25ba01ba42b088a3b5b397e97cfc6e1b09f33628a88689a9ffbeee
  zh  =4c99dcd897411d8cacb6526ce6e8f8405f7fb24c1efd39e19c6b490b859625e0
  en  =80a88736b0fca0535e312b5fd4c12b67823ad8ad57ab0f9dc04a1652e4749a39
test the_recomposition_follows_the_language_clause_and_the_axis_has_teeth ... ok
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.49s
```
**两边各自的来路**（这是接受标准要的"两边的 hash 与它们各自来自哪条路径"）：

| 轴位 | 线上（路径：`GET /api/v1/distill` → `api.rs` 建 `Distiller{language: p.language, prompt_override: p.prompt}` → `compose_prompt()` → `extraction_prompt(language, override)` → `content_hash`） | 本文件的重算（路径：线上 `prompt` + 线上 `language` → `distill.rs` 源码里的子句/尾部 → `content_hash`） | 一致 |
| --- | --- | --- | --- |
| 无 language（对照） | `6b252b7e…fbeee` | `6b252b7e…fbeee`（子句为空 ⇒ 与 t135 的旧期望值相同） | ✅ |
| `language = 简体中文` | `4c99dcd8…9625e0` | `4c99dcd8…9625e0` | ✅ |
| `language = English` | `80a88736…4749a39`（`80a88736b0fca0535e312b5fd4c12b67823ad8ad57ab0f9dc04a1652e4749a39`） | 同值 | ✅ |

**牙齿**（同一组输入**只在 `language` 上不同**，hash 必须动）：`none ≠ zh`、`none ≠ en`、`zh ≠ en` 三条 `assert_ne!` 全绿。
**与 t135 的交叉验证**：`zh` 的线上值 `4c99dcd8…` 正是 t135 我用**独立 PowerShell SHA-256** 算出的值 ⇒ 两次、两条互不相干的实现路径给出同一个 64-hex ⇒ 该轴的"同源"是**读数**而不是承诺。
**没有动的旧读数**：F3 的 `6b252b7e…`（override）/ `6de27008…`（改一字段）/ `01789e40…`（内建默认）与 F4 的 `{"agents":[],"embedder":"hash-embedder","scoring_version":2}` 与 t135 逐字相同 ⇒ 本单**没有**改变 wire 契约（只加了测试的判别力）。

---

## 3 能红的证明（隔离副本；共享树零变异）

**隔离形态**：`git archive HEAD` → `%TEMP%\t142-iso`（**不动共享树的任何文件**）；把**改前**的测试文件带进去当**第二个测试目标** `tests/version_points_old.rs`（303 行 / 4 条测试，来自 `git HEAD`），把我的**改后**文件放成 `tests/version_points.rs`；`CARGO_TARGET_DIR=%TEMP%\t142-iso-target`（私有）+ `PROTOC`；`NO_PROXY=127.0.0.1,localhost,::1`。

### 3.1 变异 1（接受标准给的那个形状）：handler 的 hash 输入 → 形状合法的 64-hex 字面量

窗口 **01:01:31 → 01:09:27**（冷建 7m55s）。副本 `api.rs` `C43BACC0A535A107` → `30126C7A9D583AB3`；同一时刻共享树 `api.rs` 仍是 `C43BACC0A535A107`。
```
running 5 tests
test the_prompt_hash_check_is_false_for_all_three_mutations ... FAILED
test the_calibration_check_is_false_for_all_three_mutations ... ok
test stats_reports_the_calibration_versions_from_their_single_sources ... ok
test the_recomposition_follows_the_language_clause_and_the_axis_has_teeth ... FAILED
test distill_reports_a_prompt_hash_from_the_same_function_and_input ... FAILED
thread 'distill_reports_a_prompt_hash_from_the_same_function_and_input' panicked at crates\daemon\tests\version_points.rs:303:5:
assertion `left == right` failed: same function, same input: content_hash(compose_prompt()) for the override policy
  left: Some("0000000000000000000000000000000000000000000000000000000000000000")
 right: Some("6b252b7eab25ba01ba42b088a3b5b397e97cfc6e1b09f33628a88689a9ffbeee")
test result: FAILED. 2 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.51s
```
⇒ **主用例以契约给的那个形状变红**（`left Some("0000…") / right Some("6b252b7e…")`）✅；F4 的两条仍绿。

### 3.2 变异 2（V-B1 本身）：让 **distiller 侧忽略 `language`**，两处计算从此在这条轴上分叉

先把副本 `api.rs` **还原**（回到 `C43BACC0A535A107`），再只改一处：
```diff
-        extraction_prompt(self.language.as_deref(), self.prompt_override.as_deref())
+        extraction_prompt(None, self.prompt_override.as_deref())   // 变异（仅隔离副本）：distiller 侧忽略 language
```
窗口 **01:09:43 → 01:10:07**（**增量 20.36s**，私有 target 是热的）。副本 `distill.rs` `0291DF0E` → `B0AF5F9F`；共享树 `distill.rs` 全程 `0291DF0E`。**同一份变异下两个测试文件并排跑**（`--no-fail-fast`）：

```
Running tests\version_points.rs            (改后 = 本单的字节)
running 5 tests
test the_recomposition_follows_the_language_clause_and_the_axis_has_teeth ... FAILED
test result: FAILED. 4 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.51s
thread 'the_recomposition_follows_the_language_clause_and_the_axis_has_teeth' panicked at crates\daemon\tests\version_points.rs:385:5:
assertion `left == right` failed: same function, same input: the language clause must be inside the hash
  left: Some("6b252b7eab25ba01ba42b088a3b5b397e97cfc6e1b09f33628a88689a9ffbeee")   ← 线上（被变异：没有子句）
 right: Some("4c99dcd897411d8cacb6526ce6e8f8405f7fb24c1efd39e19c6b490b859625e0")   ← 重算（从源码读来的子句）

Running tests\version_points_old.rs        (改前 = t127 的 303 行字节)
running 4 tests
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.63s
```
⇒ **这就是 V-B1 的完整证明**：同一个"两处计算在 `language` 上分叉"的世界里，**改前的用例 4/4 全绿（它看不见）**，**改后的用例红在那条语言断言上（`left` = 无子句的线上值，`right` = 带子句的重算值）** ✅✅ —— 修的不是"断言更严"，而是**把原来空转的那条轴变成有判别力的轴**。
（附带：变异 2 下 F3 的主用例仍绿，正确 —— 它用的是**无 language** 的 daemon，与这条轴无关；这也说明新轴的失败**专属于**新断言。）

**共享树零变异**：两次变异都只写进 `%TEMP%\t142-iso`；共享树的 `api.rs`/`distill.rs` 在两次窗口前后哈希不变（§0 表）。**没有用宣告窗口**（不需要：隔离副本足够）。

---

## 4 轴清单：这份测试现在覆盖什么、没覆盖什么、为什么

**已覆盖（7 条轴）**
| # | 轴 | 判据（在文件里的位置） |
| --- | --- | --- |
| 1 | 同输入 ⇒ 同 hash（同一 daemon、又一台同策略 daemon） | 2 条 `assert_eq!`（保护中的两条，逐字未动） |
| 2 | 改一个字段（`[distill] prompt`）⇒ 不同 hash | 1 条 `assert_ne!`（保护中的第三条） |
| 3 | 重算 == 线上（**无 language** 的 override daemon 与 built-in daemon） | 2 条 `assert_eq!`（现在走**源码派生**的重算） |
| 4 | **`language` 轴：两边一致**（无/简体中文/English 三档） | **新**：3 条 `assert_eq!` |
| 5 | **`language` 轴：有牙齿**（三种语言两两不同） | **新**：3 条 `assert_ne!` |
| 6 | 键集合与键序的**加法式**（`/distill` 六键 + `prompt_hash`；`/stats` 三键） | 2 条键序断言 |
| 7 | `prompt_hash` 的形状闸（64 hex）与三变异负控；`/stats` 的 embedder 双读点相等 + `scoring_version` == 常量 | 4 条断言 + 2 条负控 |

**仍未覆盖（每条都写"为什么今天没覆盖"）**
| 轴 | 为什么今天没覆盖 |
| --- | --- |
| **`[distill] prompt` 在运行时经 `PUT /api/v1/distill` 改动** | **没构造**。文件只通过"用 PROMPT-B 启动一台 daemon"来体现 prompt 变化；PUT 是另一个面（会写 `policy.toml` + `set_distill_policy`）。t135 我在**临时守护进程**上手工跑过 A→B→默认 的 PUT 往返（读数在那里），但**没进这个文件**。价值：PUT 覆盖的是**plumbing**（同一个 `RwLock` 被更新），不是组字 —— 与 V-B1 那条"判别力"不同类。 |
| **`graph` 开/关** | **不属于本函数的契约**：`compose_prompt()` → `extraction_prompt(language, override)` 的参数表里没有 `graph` ⇒ 它**进不了**这个 hash。断言它会变成"钉住一个非性质"（t135 实测两者 hash 逐字相同）。⇒ 故意不测。 |
| **embedder 变化（hash-embedder ↔ 语义模型）** | **构造不出（在本测试环境里）**：需要真的 FastEmbedder/ONNX 可用；本机 `try_new` 失败 ⇒ 永远走离线 `hash-embedder`。文件现在证明的是"两个读点走**同一个 accessor**"（与具体 embedder 无关的结构性质），这比"钉一个模型名"更对。 |
| **`scoring_version` 的数值语义**（2 是否"今天该报的版本"） | **构造不出"外部真相"**：文件拿它与常量本身比 ⇒ 常量改了会一起动。要检出"常量错了"需要一个外部权威（发布记录/迁移表），不属于这条测试的契约。 |
| **真蒸馏落库的 `distill_log.prompt_hash` == 线上 `prompt_hash`** | **没构造**（成本）：要真会话 + mock agent 触发自动蒸馏。这是 t135 的 V-B6，**仍然开着**（本单 inScope 只有测试文件，够不到那条 e2e）。 |
| **`auto`/`agent` 键的**行为** | 只断言存在与键序，不断言取值 —— 它们的取值来自策略与 agent 卡片，属于别的测试面。 |

---

## 5 既有断言未被削弱（逐字对照）

`git diff --numstat` = **187 增 / 13 删**。**全部 13 行删除**逐字如下（证明没有任何保护中的断言被削弱）：

```
-/// The documented tail `extraction_prompt` appends after the base (the source of
-/// the composition is `crates/daemon/src/distill.rs`, `fn extraction_prompt`).
-const TRANSCRIPT_TAIL: &str = "\n\nTRANSCRIPT:\n";
-/// Recompute `prompt_hash` INDEPENDENTLY, from values read off the wire plus the
-/// documented composition: `content_hash(base + "\n\nTRANSCRIPT:\n")`. This is the
-/// same function and the same inputs the daemon uses — the daemon builds them
-/// from its live policy, the test from the wire's own `builtin_prompt`/`prompt`.
-fn expected_prompt_hash(base: &str) -> String {
-    ruagent_memory::write::content_hash(&format!("{base}{TRANSCRIPT_TAIL}"))
-        expected_prompt_hash(body["prompt"].as_str().unwrap()),
-        expected_prompt_hash(d2["builtin_prompt"].as_str().unwrap())
-        Some(expected_prompt_hash(body["prompt"].as_str().unwrap())),
-        Some(expected_prompt_hash(d2["builtin_prompt"].as_str().unwrap())),
```
**四条"保护中"的断言根本不出现在 diff 里**（既不在删除也不在新增），改前/改后逐字相同：

```rust
    assert_eq!(
        prompt_hash(&body),
        prompt_hash(&b2),
        "same input => same hash (a second daemon, same policy)"
    );
    assert_eq!(
        prompt_hash(&body),
        prompt_hash(&a2),
        "same input => same hash (same daemon)"
    );
    assert_ne!(
        prompt_hash(&body),
        prompt_hash(&c2),
        "changing `[distill] prompt` must change the hash"
    );
```
（第 4 条 = `assert_ne!` 的 `same daemon`… 精确说：三条断言 + 同一测试里对 `builtin_prompt` 的 `assert_eq!`；**它们引用的 `prompt_hash()` 谓词与 `get_json()` 也逐字未动**。得改的只有**重算辅助函数的签名**：`expected_prompt_hash(base)` → `(base, language)` —— 因为期望值必须随 `language` 走，这正是 V-B1 要求的；它的**调用点**从 1 行变 4 行是 rustfmt 折行，不是语义变化，语句消息逐字保留。）

**hunk 清单**（6 个）：`@@ -134,3 +134,84 @@`（源码派生机制，替换 `TRANSCRIPT_TAIL`）、`@@ -145,6 +226,13 @@`（辅助函数签名/文档）、`@@ -209,2 +297,5 @@`（F3 的 println）、`@@ -214 +305,4 @@` 与 `@@ -219 +313,4 @@`（两条重算断言加 `language` 实参）、`@@ -238,0 +336,77 @@`（新用例）。

---

## 6 门禁（我本人在最终字节上重跑）

| 命令 | 读数 |
| --- | --- |
| `scripts/cargo-team.ps1 test -p ruagent-daemon` | **exit 101** —— **我的目标全绿**：`version_points` **5 passed / 0 failed**（改前 4）；失败的**唯一**目标是 `--lib`，且失败在**同伴的文件**里：`chat::generating_tests::delete_removes_the_row_and_stops_a_live_run_first`，`panicked at crates\daemon\src\chat.rs:2046: assertion left == right failed: a deleted chat must not produce another message`（156 passed / 1 failed）。**两点把它定性为 load-sensitive flake，而不是回归**：① **同一份字节上只跑 `--lib` 的重跑是 `ok. 157 passed; 0 failed`（exit 0）**；② 那次红发生时，**我自己的隔离冷建（4 jobs）正在同一台机器上跑**（§3 窗口 01:01:31–01:09:27 与该次门禁重叠），而重跑时它已经结束 ⇒ 这条断言是"删掉的 chat 不得再产出消息"的**时序竞争**，与 prompt hash 无任何关系。③ **我的改动不可能够到它**：它在 `--lib` 目标，而我的字节只在 `tests/version_points.rs`（另一个测试二进制）。带 `--no-fail-fast` 的整套读数：**214 passed / 1 failed / 8 ignored**（lib 156+1F · capabilities 11 · capability_defaults 5 · event_compat 2 · graph_ingest 16 · injection_e2e 0+8ignored · knowledge_api 16 · recall_evidence_announced 3 · smoke 0 · **version_points 5** · doctests 0）。⇒ 本行按 gate 的**字面读数**记 **failed（101）**，成因与归属写在这里，**不替同伴修**（已立 finding V-B7）；**attempt 2 在最终字节上重跑为 exit 0，见 §6.1**。 |
| `scripts/cargo-team.ps1 clippy -p ruagent-daemon --all-targets -DenyWarnings` | **exit 0**（`Checking ruagent-daemon` + `Finished dev profile in …`） |
| `cargo fmt --all --check` | **exit 0**（我曾在本单早期读到过一次红，那是**同伴**当时在途的 `crates/daemon/tests/injection_e2e.rs`；他修好/提交后现在是 0 —— 与我这条 187/13 的改动无关） |

### 6.1 attempt 2：最终字节上的重跑 —— **三条门禁全绿**（收口 attempt 1 的 gate 红）

**先说那份红的归宿（captain 的决定）**：整包那次 `exit 101` 由 **captain 在空闲机器上复核为绿**（`cargo-team.ps1 test -p ruagent-daemon` ⇒ `tests\version_points.rs → ok. 5 passed; 0 failed`、`Doc-tests ruagent_daemon → ok. 0`、`[cargo-team] exit=0 elapsed=11.1s`）⇒ **§6 表里那条红是负载/时序敏感，不是本单的失败**；该 flake 已**单独立案 `t145`**（要求：**有界等待 + 显式同步**，**不许**把 sleep 调长或"重试到过为止"，并明确"若根因在产品侧（生成循环本身与删除不同步）⇒ 如实写成产品缺陷、不许只改测试掩盖"）。

**attempt 2 我自己的读数**（窗口 **01:22:04 → 01:36:24**；退出码在任何管道之前捕获，C40）：

| 命令 | 退出码 | 计数原文 |
| --- | --- | --- |
| `scripts/cargo-team.ps1 test -p ruagent-daemon` | **0**（`[cargo-team] exit=0 elapsed=11.7s`） | lib `157 passed; 0 failed` · capabilities `11/0` · capability_defaults `5/0` · event_compat `2/0` · graph_ingest `16/0` · injection_e2e `0 passed; 0 failed; 8 ignored` · knowledge_api `16/0` · recall_evidence_announced `3/0` · smoke `0` · **version_points `5 passed; 0 failed`** · doctests `0` ⇒ **215 passed / 0 failed / 8 ignored** |
| `scripts/cargo-team.ps1 clippy -p ruagent-daemon --all-targets -DenyWarnings` | **0**（`Finished dev profile in 2.90s`，无 error/warning 行） | — |
| `cargo fmt --all --check` | **0**（零 `Diff in`） | — |

**这次认证的字节**：`crates/daemon/tests/version_points.rs` = sha256（前 24 hex）**`48E81C4E29A476A554DFC8D2`** —— 与 attempt 1 认证的**同一个哈希**（未再改动），且 `git diff --exit-code HEAD -- crates/daemon/tests/version_points.rs` = **0**（已随 `e6673d8` 提交）⇒ 上面三绿**属于 §2/§3 那份字节**。

**对 flake 假设的一处诚实修正**：这次重跑时**同机有 3 个同伴的 cargo/rustc 在跑**（我得排队 **82 次 `waiting 10s`**，从 01:22:04 等到 01:35:21 才拿到构建锁），**整包仍然绿** ⇒ 触发条件**不是"存在别的构建"这么粗**。准确的说法是：**四次读数里只红过一次**（① attempt 1 的整包：红；② 同字节 `--lib` 单跑：绿；③ captain 空闲机整包：绿；④ attempt 2 整包、同机 3 个同伴构建：绿）⇒ 这是一个**稀有**的时序竞争，与 prompt hash / 本单改动无关；把它当成"负载一高必红"会是过度概括。

---

## 7 未测 / 不覆盖 / 残余风险

* **残余风险 1（源码派生的耦合）**：`include_str!("../src/distill.rs")` 让本测试**依赖源码文本**。marker `\n\nWrite every ` 在今天出现 **2 次**（`distill.rs:1124` 的真子句 + `:1159` 的单元测试文本），我的 `find` 取**第一处**；若将来有人在**更前面**插入同样的 marker，解析会取错 —— 但**形状断言**（以 `\n\n` 开头、以 `above.` 结尾、含 `{lang}`）会**当场 panic**，而不是给出一条错的期望值。这是有意的"坏输入要响"。同理尾部 marker 有 8 处含 `TRANSCRIPT:`，但只有真字面量是**转义形态**（`\n\nTRANSCRIPT:\n`）⇒ 命中唯一。
* **残余风险 2**：若 `extraction_prompt` 被**重命名/搬走**，本测试会 panic（附指令），需要人改测试 —— 这是设计选择（我宁可红，也不要静默丢掉这条轴）。
* **残余风险 3（flaky）**：`chat.rs:2046` 的那条偶发红**仍在树里**（已作为 finding 交给 captain；本单不替同伴修）。它会让后续任何 `test -p ruagent-daemon` 的读数偶发为红。
* **未覆盖**：§4 表格里的六条；其中 V-B6（真蒸馏落库值）是**最强形态的同源**，仍开着。**没有**在隔离副本里测"新版测试在**别的**轴上变红"（只测了契约要求的那两条）。**单一平台**（Windows / rustc 1.95.0），单进程。
* **本单没有重建 `ruagent.exe`**：因此**没有**新的 HTTP 级读数（也不需要：wire 值我沿用 t135 的读数，本单的新读数全部来自测试框架与隔离构建）。

---

## 8 复现命令

```powershell
cd C:\Users\19410\Documents\ai\ruagent
# 0) 快速信号：只跑我的目标，带读数
$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"; $env:NO_PROXY='127.0.0.1,localhost,::1'
rustfmt --edition 2024 crates/daemon/tests/version_points.rs        # 只格式化我改的文件
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-daemon --test version_points -- --nocapture

# 1) 隔离实验（共享树零变异；私有 target dir）
$iso="$env:TEMP\t142-iso"; Remove-Item -Recurse -Force $iso,"$env:TEMP\t142-iso-target" -EA SilentlyContinue
New-Item -ItemType Directory -Force $iso | Out-Null
git archive HEAD -o "$env:TEMP\t142-iso.tar"; tar -xf "$env:TEMP\t142-iso.tar" -C $iso; Remove-Item "$env:TEMP\t142-iso.tar"
git show HEAD:crates/daemon/tests/version_points.rs | Set-Content "$iso\crates\daemon\tests\version_points_old.rs" -Encoding utf8NoBOM
Copy-Item crates/daemon/tests/version_points.rs "$iso\crates\daemon\tests\version_points.rs" -Force
$env:CARGO_TARGET_DIR="$env:TEMP\t142-iso-target"; $env:PROTOC="$env:USERPROFILE\.protoc\bin\protoc.exe"
Remove-Item Env:\CARGO_NET_OFFLINE -EA SilentlyContinue; Set-Location $iso
# 1a) 变异 1：handler 的 hash 输入 -> 64-hex 字面量（在副本 api.rs 里）
cargo test -p ruagent-daemon --no-fail-fast --test version_points --test version_points_old
# 1b) 变异 2：还原 api.rs，改副本 distill.rs 的 compose_prompt 忽略 language，再增量重跑
cargo test -p ruagent-daemon --no-fail-fast --test version_points --test version_points_old

# 2) 收尾门禁（共享树）
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-daemon --no-fail-fast
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 clippy -p ruagent-daemon --all-targets -DenyWarnings
cmd /c "cargo fmt --all --check > `"$env:TEMP\t142-fmt.txt`" 2>&1"; Write-Output "FMT=$LASTEXITCODE"
Remove-Item -Recurse -Force "$env:TEMP\t142-iso","$env:TEMP\t142-iso-target","$env:TEMP\t142-fmt.txt"
```

---

## 9 纪律回执

* **写入集合** = 我改的 `crates/daemon/tests/version_points.rs` + 本报告；`crates/daemon/src/**`（只读，且隔离副本里的变异从不落到共享树）、`crates/knowledge/**`、`crates/mcp/**`、`panel/**`、`.github/**`、`scripts/**` **零字节改动**。
* **负控/变异**：全部在 `%TEMP%\t142-iso`（`git archive HEAD` 的导出）+ 私有 `CARGO_TARGET_DIR`；窗口 **01:01:31–01:09:27**（冷建）与 **01:09:43–01:10:07**（增量）；**共享树零变异**（`api.rs`/`distill.rs` 哈希在两次窗口前后不变，§0）。
* **构建路径**：共享树的门禁**全部**走 `scripts/cargo-team.ps1`，同树**没有**设 `CARGO_TARGET_DIR`/没有 `-TargetDir`；隔离副本用**私有** `CARGO_TARGET_DIR` 直跑 `cargo`（AGENTS.md 的 worktree 规矩），因此**不占团队锁**（对同伴更友好），这一点在此明说。
* **进程与临时目录**：**本单没有起过守护进程**（读数都来自测试框架），因此活环境（8787 = pid 14944，收尾回读 health `ok`）**连读都没读**（不需要 POB 探针）；`%TEMP%\t142-iso`、`%TEMP%\t142-iso-target`、`%TEMP%\t142-fmt.txt` **已删**。
  * **本单按 t135 的教训执行"先看归属、再删具体路径"**：收尾时 `%TEMP%` 里还有两个匹配 `t142*` 的条目 —— **`t142b` 与 `t142run`，mtime 都是 2026-10-02 20:14:39（比我开工早两天）** ⇒ **不是我的，我一个都没碰**（`my_temp_leftovers` 我的条目 = **0**；我删的三个路径全部是我本单创建的）。t135 那次的 glob 误删没有重演。
  * **attempt 2 的收尾**：本 attempt 只创建了 5 个门禁日志 `%TEMP%\t142b.1`…`%TEMP%\t142b.5`，**按五个具体文件名逐个删除**（删后复核：五个文件都不在了）；那两个**不是我**的目录 `t142b`/`t142run`（2026-10-02）**仍在、未触碰**。**顺带记一条工具陷阱**：`Get-ChildItem -Filter 't142b.*'` 会**误匹配**那个没有点的目录名 `t142b`（Windows 旧通配语义），所以这里认的是"删了哪五个具体路径"，不是"按模式还剩几个"。我名下 `cargo.exe`/`rustc.exe` = **0**；`ruagent.exe` 只有活的 **pid 14944**（8787 的持有者）。
* **读数三件套**：对象集（`version_points` 的 5 条测试 + 隔离副本里的老/新两份）；采样面（共享 target 的测试二进制，或私有 target 的隔离构建；单进程）；可证伪判据（**两个独立的 64-hex 值必须相等**，或**必须不等**；任一不符即红，且隔离副本里已实测会红）。
* **期望值只写一处**：组字规格**只从 `distill.rs` 源码读**（子句与尾部各一处），语言取值只从线上 `language` 读 ⇒ 文件里**不再有第二份规格**。
* **自我评审声明**：这条 finding 是我提的、修也是我做的 ⇒ **它需要独立评审**（我不给自己 pass）。
