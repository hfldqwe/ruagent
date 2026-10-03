# t132 修复：转录抽取补上**按字节的界** + `bounded.rs` 补上那条能红断言

> **性质**：**修复（repair）**，作者 = verify2。输入 = `t125` 攻击式审计的 **F1**（`docs/design/reviews/gen4-audit-extract.md` §4 表格第一行 + §2.3）。
> **结论**：`ExtractLimits` 新增 `max_input_bytes`（默认 **1 MiB**），转录窗口从"只界条数"变成"**条数 + 字节**都界"；被裁掉的输入**可区分**（`MemoryCandidates::bytes_skipped` + `truncated`），**没有把 F1 换成 F2**。实测：`memory_one_turn_repeat` 的 16 MB 档从 **1459 ms / 278 MB 峰值**落到 **95.5 ms / 17.8 MB**（1 MB 档是 93.6 ms），`bounded.rs` 从 8 条测试变 9 条，**新断言在隔离副本里被负控打红**（`8 passed; 1 failed`），共享树零变异。
> **窗口**：2026-10-04 00:22:42 – 00:3x（+08:00）。**HEAD 在窗口内从 `27b3893` 继续移动**，所以一切读数钉在**文件 SHA-256** 上（§0）。

---

## 0 采样面、pins、门禁读数

| | |
| --- | --- |
| 改的文件 | `crates/extract/src/lib.rs` · `crates/extract/src/memory.rs` · `crates/extract/src/text.rs` · `crates/extract/tests/bounded.rs` · `crates/daemon/src/extract_plane.rs`（**注释一处**） + 本报告。**`crates/daemon/src/api.rs`/`distill.rs` 一行未动**（t127 在途），`crates/mcp`/`crates/graph`/`panel`/`.github`/`scripts` 一行未动 |
| 改前 pins（用于 before 读数的那份字节） | `lib.rs 130AEABEBD8059ED` · `memory.rs 77A579AD7DF66B60` · `text.rs F2CB567678B9AB13` · `bounded.rs 3E9C829CE19712B7` · `extract_plane.rs 218EA73708D0FFA6`（`graph.rs`/`rules.rs` 未动：`D0FB53F0564B9762` / `BAB5489C7865AB33`） |
| **改后 pins（门禁跑的就是这份字节，收尾复查未变）** | `lib.rs C440F2D757C38A21` · `memory.rs 0FCA4AC71C7932FB` · `text.rs ADEF7745FEAB3D24` · `bounded.rs 168B59509E7FAC53` · `extract_plane.rs 0AA9AAC351E795E7` |
| 门禁（全部在**改后字节**上） | ① `cargo-team.ps1 test -p ruagent-extract` ⇒ **38 passed / 0 failed / exit 0**（6 lib + **9 bounded** + 8 graph-gold + 3 memory-gold + 12 requirement-markers；bounded 由 8 → 9）② `cargo-team.ps1 clippy -p ruagent-extract --all-targets -DenyWarnings` ⇒ **exit 0**（`Checking ruagent-extract` + `Finished`，排队 5×10s 后 2.6s）③ `cargo fmt --all --check` ⇒ **exit 0** |
| 探针 | `%TEMP%\t132-probe`：把**改前**与**改后**两套 `src/*.rs` 各用 rustc 直编成 rlib（无 cargo、不占团队锁），**同一份** `probe.rs` 链接两边 ⇒ before/after 是**同一把尺子**。`probe_budget.rs` 只能在改后编（它读新字段）。**与 t125 探针的一处测量修正**：输入在**计数开始之前**构造好，所以 `pass_peak_bytes` 是**抽取器自己的**工作集，不含调用方那份 16 MB 输入文本（t125 的探针把 `text.clone()` 放在被测闭包里，所以它那份 16 MiB 峰值里混着输入本身）——这一点会让两轮的绝对数字不完全可比，所以我**用同一份新探针重跑了改前字节**（下表左列） |

---

## 1 缝的选择（接受标准第 3 条）：选 **extract 侧 `ExtractLimits`**，daemon 侧**不裁字节**

**改动**：`ExtractLimits` 新增字段

```
lib.rs:390   pub max_input_bytes: usize,
lib.rs:410       max_input_bytes: 1024 * 1024,      // 默认 1 MiB
```

**依据（四条，按重要性）**：

1. **上限的单一真相源**：这个 crate 的契约是"limits 以参数进来，crate 从不读配置"（`lib.rs:109-112`），而**每一条已有的界**都住在 `ExtractLimits` 里（`max_per_input`/`min_score`/`max_text_bytes`/`max_content_bytes`/`max_entity_name_chars`/`max_turns`）。把转录的字节界放到 `turn_for_extraction` 里，就是让"这个抽取器受哪些界约束"这个问题有**两个答案**，而其中一个不在它自己的文档结构里。而且图谱路径**已经在 crate 内**按字节界输入（`graph.rs:42` 的 `window_text` + `max_text_bytes`）——两条路径现在同形。
2. **只有这一趟能"报"**：报告面是 `MemoryCandidates` 的计数器；daemon 的 `truncated` 谓词读的正是 `pass.truncated`（这正是 `extract_plane.rs:457-489` 记下的 t9 教训：**标志必须是这一趟自己的读数，不是调用点的重算**）。若在 daemon 侧裁字节，调用点就得自己再算一次"我裁了多少"——正是 t9 判过的那种重算，而且它裁掉的部分对 `truncated` 不可见，那就会把 F1 换成 F2。
3. **谁会写下这个输入**：今天的调用方是 daemon（读用户自己写的转录，天然无界），但 crate 是**公开 API**：测试、MCP 面、将来的调用方都在同一个口进来。界在 crate 里 ⇒ **每一个**调用方都受保护，而不是只有今天这一个。
4. **daemon 侧不需要裁字节，也不需要改逻辑**：daemon 的每一套 limits 都是从 `ExtractLimits::default()`/`graph_default()` 派生的（`session_limits` 在 `extract_plane.rs:155-161` 就是 `..ExtractLimits::default()`）⇒ 新字段**自动生效**，`truncated` 也**自动覆盖**它（`pass.truncated` 已包含字节损失）。`turn_for_extraction`（`extract_plane.rs:410-436`）继续干它该干的活——**过滤平台提示词**（语义过滤），它不做尺寸裁剪。**唯一一处 daemon 改动是注释**：把 `truncated` 的第三个来源写清楚（`extract_plane.rs:488-…`），谓词一行没动。

**默认值 1 MiB 的依据**：文档窗是 256 KiB（`max_text_bytes`），转录取它的 **4 倍**——一次会话承载对话双方；而 2000 轮 × ~500 B/轮的**真实上限量级**约 1 MB，所以默认值**不重调正常会话**（§2.4 实测 2000 轮 88,890 B ⇒ `bytes_skipped=0`）。它约束的是**病态单轮**。

---

## 2 改前/改后读数（接受标准第 1 条）

**同一份 `probe.rs`，同一测量形状，改前字节 vs 改后字节**（`ExtractLimits::default()`；`peak_over_input = pass_peak_bytes / input_bytes`）：

| 用例 | 档 | 改前 ms | 改后 ms | 改前峰值 B | 改后峰值 B | 改前 peak/input | 改后 peak/input |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `memory_one_turn_repeat` | 1 MB | **101.56** | **93.55** | 17,388,756 | **17,388,756** | 17.30 | 17.30 |
| | 4 MB | 414.99 | **97.75** | 69,555,412 | **17,757,356** | 17.30 | 4.42 |
| | **16 MB** | **1459.12** | **95.53** | 278,222,036 | **17,757,356** | 17.30 | **1.10** |
| `memory_one_turn_no_punct` | 1 MB | 13.06 | 12.07 | 18,874,484 | 18,874,484 | **18.00** | **18.00** |
| | 4 MB | 52.67 | **13.41** | 75,497,592 | **18,874,560** | 18.00 | 4.50 |
| | **16 MB** | **203.88** | **15.03** | 301,990,004 | **18,874,560** | 18.00 | **1.13** |
| `memory_han_one_turn_repeat`（**非 ASCII**） | 1 MB | 57.20 | 61.74 | 10,660,494 | 10,660,494 | 11.09 | 11.09 |
| | 4 MB | 247.77 | **61.35** | 42,641,976 | **11,534,334** | 11.09 | 3.00 |
| | **16 MB** | **1003.96** | **67.47** | 170,568,130 | **11,534,334** | 11.09 | **0.75** |

**判据（t125 给的）逐条对账**：
* **「16 MB 档的 ms 回落到与 1 MB 同量级」** ✅ `repeat`：93.55 → **95.53 ms**（1 MB 档 93.55，**1.02×**；改前同一对比是 **14.4×**）；`no_punct`：12.07 → 15.03 ms（1.25×，改前 15.6×）；Han：61.74 → 67.47 ms（1.09×，改前 17.6×）。**4 MB 档与 16 MB 档现在同值**（97.75 / 95.53）——因为两者的**实际读取量都等于预算**，与输入大小无关。
* **「峰值分配不再随输入增长」** ✅ `repeat` 17.39 → 17.76 → **17.76 MB**（1/4/16 MB，**平的**）；`no_punct` 18.87 → 18.87 → **18.87 MB**（平）；Han 10.66 → 11.53 → **11.53 MB**（平）。`peak/input` 因此从恒定的 17.30/18.00/11.09 掉到 **1.10 / 1.13 / 0.75**。
* **诚实的一条**：界去掉的是**增长**，不是**常数因子**。被读的那 1 MiB 文本仍要建 `char_indices` 表（**16 B/字符**，`text.rs:80`），所以绝对值仍是 ~17–19 MB（`peak/budget ≈ 17`）。这不是这条 finding 的对象（F1 是"无界"），但如果将来要把 ~17 MB 的绝对占用也压下去，那要改的是 `split_sentences` 的表（例如流式扫描），属于另一单。

**报告面读数（`probe_budget.rs`，改后）**——这就是"被裁掉的输入可区分"（接受标准第 4 条）：

```
T132|budget|default_max_input_bytes=1048576
T132|budget|one_turn_1MB|input=1004870|bytes_skipped=0|truncated=false|out=1|turns_skipped=0
T132|budget|one_turn_4MB|input=4019526|bytes_skipped=2970956|truncated=true|out=1|turns_skipped=0
T132|budget|one_turn_16MB|input=16078150|bytes_skipped=15029580|truncated=true|out=1|turns_skipped=0
T132|budget|three_turns|input=3375000|bytes_skipped=2326455|truncated=true|out=1|turns_skipped=0|origin_indices=["2"]|distinct=["以后统一用 后段CCCC 处理这件事。"]
T132|budget|zero_budget|input=23|bytes_skipped=23|truncated=true|out=0
T132|budget|realistic_2000_turns|input=88890|bytes_skipped=0|truncated=true|out=32
```

* `one_turn_4MB`：`bytes_skipped=2,970,956` 而 `input − 预算 = 2,970,950` ⇒ 多出的 **6 字节**是**句末标点回退**（见下）造成的，说明保留头**≤ 预算**且计数是"没读的字节"的确切值；16 MB 档保留头与 4 MB 档**完全一样**（两次相减都差 1,048,570）⇒ **与输入大小无关**。
* `three_turns`（3 × 1.125 MB = 3,375,000 B）：`bytes_skipped=2,326,455`、**`origin_indices=["2"]`、只剩最新那一轮的句子** ⇒ 预算按**最新优先的连续后缀**生效：老的两轮整轮被丢、**最新一轮与其原始 `origin.index`** 都保住（证据坐标没有被窗口平移污染）。
* `realistic_2000_turns`（88,890 B）：**`bytes_skipped=0`** ⇒ 正常会话**没有**被这次改动重调；这里的 `truncated=true` 来自 `max_per_input`（`out=32`），与字节界无关——正好说明两个损失共用一个标志、但计数各归各的。
* `zero_budget`（`max_input_bytes=0`）：`bytes_skipped=23/23`、`out=0`、**不 panic** ⇒ 退化值被处理且**如实报数**，不是静默空列表。

---

## 3 新断言 + 负控（接受标准第 2 条）

**新测试**：`crates/extract/tests/bounded.rs:377` `an_over_long_transcript_is_bounded_by_bytes_and_the_loss_is_reported`（bounded.rs 由 8 条 → **9 条**）。

它断言**三**件事，第三件是无法碰巧满足的：
1. 损失被报出：`truncated == true`（bounded.rs:420），且 `bytes_skipped == 输入 − 预算`（**精确算术**，bounded.rs:423-425）——构造上让预算**正好落在句末**（等长句 × 100），所以这个等式是精确的而不是区间；
2. 预算之外**什么都没被读**：句尾那枚 `独特标记ZZZ` 不得出现在候选里，而预算内的 `填充0000` 必须出现（bounded.rs:431-439）；
3. **尺度不变性**：同一前缀放进 **16×** 更大的轮次（`limit * 256` vs `limit * 16`），候选列表必须**逐字相同**（`format!("{:?}", …)` 相等，bounded.rs:450-455）——"输出不再随输入增长"被写成断言而不是承诺。

**负控（共享树零变异）**：
* **做法**：把**改后**字节复制到 `%TEMP%\t132-neg`（含 `Cargo.toml`，因为 purity 测试 `include_str!` 了它），用 rustc 直编成测试二进制；然后在**副本**里把预算抹掉——`byte_window(window, limits.max_input_bytes)` → `byte_window(window, usize::MAX)`（**就是"把上限调大"**，一行、无其它改动），再编一份，跑同一个测试。
* **窗口**：**00:25:05 – 00:25:10**（+08:00）。共享树在窗口内**没有被写过**（收尾 `git status --porcelain -- crates/extract` 只有我自己那 4 个改后文件，五个源文件哈希 = 改后 pins）。
* **读数（逐字）**：

```
Control A（未变异副本，同一份 bounded.rs 测试二进制）:
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.01s

Control B（把预算调成 usize::MAX）:
test an_over_long_transcript_is_bounded_by_bytes_and_the_loss_is_reported ... FAILED
test result: FAILED. 8 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.08s
```
  以及同一过滤器下两个二进制的对照：`bounded_a.exe => ok. 1 passed; 8 filtered out` / `bounded_b.exe => FAILED. 0 passed; 1 failed; 8 filtered out`。
* **红在哪一条断言（负控把原因也交出来了）**：panic 打出的结构是
  `… truncated: false, turns_skipped: 0, bytes_skipped: 0 }`
  并且候选列表里**出现了 `…content: "以后统一用 独特标记ZZZ 处理这件事。"…`** ⇒ 三条断言同时失效：损失没被报（1）、预算外的句子被抽出来了（2）、16× 的输入给出不同的候选（3）。**其余 8 条测试仍绿** ⇒ 这个红**专属于**新断言，不是把别的测试弄红了。
* **"恢复后绿读数"**：共享树从未进入红状态——`cargo-team.ps1 test -p ruagent-extract` 在**同一份改后字节**上 **38 passed / 0 failed**（§0），Control A 在副本上也 **9 passed**。

---

## 4 既有语义不变 + F1 没有变成 F2（接受标准第 4 条）

**（a）逐字对照改前/改后受影响的语义面**：

| 语义 | 改前 | 改后 | 结论 |
| --- | --- | --- | --- |
| `max_turns` 的含义 | `start = turns.len() - max_turns`；`turns_skipped = start` | `turns_skipped = turns.len() - max_turns`（**只算这一条窗**）；索引基址另用 `base = turns_skipped + first_kept`（memory.rs:75-90） | **含义不变**：`bounded.rs:249` 的 `turns_skipped == 197`（200 轮 / `max_turns=3`）仍绿；`three_turns` 读数里 `max_turns` 没切任何东西 ⇒ `turns_skipped=0`，被字节预算丢掉的轮次**没有**混进这个计数 |
| `max_content_bytes` 的含义 | 超长句 `continue`（丢弃） | **一行未动**（`memory.rs:229-231` 仍 `sentence.len() > max_content_bytes ⇒ continue`） | **含义不变**；`memory-gold.rs:145` 的 `max_content_bytes: 20` 用例仍绿 |
| `truncated` 的含义 | 只表示"去重后的表被 `max_per_input` 砍了" | 变成**两个损失共用一个标志**（`max_per_input` 的砍 + 输入字节的裁），与 `GraphCandidates::truncated` 本来就有的形状一致（`lib.rs:403-415` 是它的先例） | **是扩展不是改写**：小输入（所有 fixture ≤ 几 KB）走 `byte_window` 的快路径 ⇒ `bytes_skipped=0` ⇒ `truncated` 与改前**逐位相同**；gold 的 `assert!(!got.truncated)`（memory-gold.rs:101）仍绿 |
| `origin.index` | 原输入下标 | 原输入下标（`base + offset`） | **不变**；`three_turns` 读到 `origin_indices=["2"]`（最新那轮的原下标） |
| `graph`/document 路径 | `max_text_bytes` 开窗 | **一行未动** | 不变（`graph-gold` 8 条 + `bounded` 的图谱断言全绿） |

**（b）唯一一处被新字段迫改的既有测试**：`bounded.rs:345` 的退化 limits 全量字面量（唯一一个**不带 `..default()`** 的 `ExtractLimits` 字面量）加了 `max_input_bytes: 0` —— 这是**编译需要**，**期望值一个字节没改**（`max_turns: 0` ⇒ 窗口为空 ⇒ 该用例的 `candidates.is_empty()` / `turns_skipped == 4` 两条断言仍绿）。**我查过全仓还有没有别的全量字面量**：另一个在 `crates/daemon/src/knowledge_graph.rs:243`，但它带 `..ruagent_extract::ExtractLimits::graph_default()` ⇒ **不受影响**，所以**没有**去动一个 out-of-scope 的文件（这是选 extract 侧缝之前必须先验的可行性，结论：可行）。

**（c）F1 → F2 的自证（这是本单最需要自证的一条）**：新引入的裁切**不是静默的**——
* `MemoryCandidates::bytes_skipped`（`memory.rs:51`）精确计数"没读的字节"：整轮被丢的字节 + 边界轮被切掉的尾巴；
* `truncated = truncated || bytes_skipped > 0`（`memory.rs:162`）⇒ **任何字节裁切都会抬旗**；
* daemon 的谓词 `pass.truncated || pass.turns_skipped > 0`（`extract_plane.rs:474`，**未改**）因此自动覆盖它；
* **读数**：4/16 MB 档 `truncated=true` 且 `bytes_skipped` 精确（§2）；`zero_budget` 也报满 23/23 字节。
* 对照：`max_content_bytes` 的丢弃**今天仍是静默的**（那是 t125 的 **F2**，**不在本单 inScope/acceptance**，我没有顺手改它——改它要动 `MemoryCandidates` 的语义面与 gold 期望，属于另一单）。**所以本单没有把 F1 换成 F2，但 F2 仍然独立存在**，这一点必须留档而不是被"F1 已收口"盖过去。

**（d）新代码自身不能引入"半句当证据"**：裁切点先按字符边界回退，再**回退到最后一个句末**（`memory.rs:204-219` 的 `head_within` + `text.rs:109` 的 `last_sentence_end`），所以正常情况下 `split_sentences` 不会收到半句；没有句末标点时按字符边界切，并且**损失照样报数**（`bytes_skipped`），注释里写明了这个兜底。**句末判据只写一处**：`text.rs:98` 的 `splits_here` 被 `split_sentences`（`text.rs:83`）与 `last_sentence_end` 共用——两个函数必须对"句子在哪结束"有**同一个**答案，否则裁切点会落在两者不一致的地方。

---

## 5 门禁与验证命令（逐条）

| 命令 | 读数 |
| --- | --- |
| `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-extract` | **exit 0**；`6 passed`（lib）+ **`9 passed`（bounded，改前 8）** + `8 passed`（graph-gold）+ `3 passed`（memory-gold）+ `12 passed`（requirement-markers）+ `0 passed`（doctests）= **38 passed / 0 failed**；`[cargo-team] exit=0 elapsed=3.1s` |
| `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 clippy -p ruagent-extract --all-targets -DenyWarnings` | **exit 0**；`cargo clippy -p ruagent-extract --all-targets -- -D warnings` ⇒ `Checking ruagent-extract v0.1.0` + `Finished dev profile in 2.48s`（先排队 `waiting 10s`×5） |
| `cargo fmt --all --check` | **exit 0**（无输出） |

---

## 6 未测 / 未覆盖（接受标准第 5 条）

* **非 ASCII：探针覆盖了**（不问"是否覆盖"而直接给数）——`memory_han_one_turn_repeat` 1/4/16 MB：改前 57.20/247.77/**1003.96** ms、峰值 10.66/42.64/**170.57** MB（= 输入的 11.09 倍）；改后 61.74/**61.35**/**67.47** ms、峰值 10.66/**11.53**/**11.53** MB（平的，`peak/input` 11.09 → **0.75**）。**为什么 Han 的倍率比 ASCII 小**：`char_indices` 表是 **16 B/字符**，而一个汉字占 **3 字节**输入 ⇒ 对"输入字节"的放大是 ~5.3×，对 1 MiB **预算**的绝对占用则与脚本无关（~11 MB，因为预算里容纳的**字符数**更少）。
* **daemon 侧还有没有别的调用点把未裁剪文本喂进来**：greps 结果——**转录路径的生产调用点只有 `extract_plane.rs:456` 一处**（其余 6 处 `memory_candidates_with` 命中都在 `extract_plane.rs` 的**测试**里）；文档路径的生产调用点是 `knowledge_graph.rs:248` `graph_candidates(&text, &limits)`，它**本来就**受 `max_text_bytes` 开窗约束（`graph.rs:42`）。⇒ 没有第二个"未裁剪入口"。
* **没有被这次改动界住的**（要点名，否则会被读成"全界住了"）：**调用方那份已经加载好的转录 `String`**。daemon 从会话文件读出的整段文本在调用之前就已经分配好了（`turn_for_extraction` 还会为保留的轮次**克隆**一份，`extract_plane.rs:455`），本单界的是**抽取器的工作量与分配**，不是**调用方的常驻内存**。要界那一侧，得在会话读取/裁剪处动手（`crates/daemon/src/sessions.rs`，不在本单 inScope）。
* **`max_input_bytes` 今天无法从 capability 选项配置**：`ExtractLimits` 的每个字段都在 daemon 的 capability 层被"选项 → limits"映射（`extract_plane.rs:155-161` 只映射 `max_per_input`/`min_score`），而选项键的白名单在 `crates/daemon/src/capability.rs`（`OptionKey`）。⇒ 这个界今天**只有默认值**，operator 不能调。把 `max_input_bytes` 做成能力选项是**后续一单**（要动 `capability.rs`/`policy.toml` 文档/面板选项面，都不在本单 inScope）。**我把它当"未闭合的环"留在这里。**
* **没有 daemon 端到端读数**：本单**没有起过任何 daemon**（不需要：界在纯函数里）。因此"真实会话经过 HTTP 注入面后 `truncated`/`bytes_skipped` 长什么样"**未实测**（`extract_plane.rs` 的谓词读法是我读代码 + t9 的既有读数，不是新跑的 e2e）。
* **没有 RSS/多核读数**：只有探针的**分配计数**（`GlobalAlloc` 包装），不是峰值 RSS；单进程单线程，没有并发抽取的读数。
* **单一平台**：全部读数来自 Windows 本机（`rustc 1.95.0`）。
* **`max_turns` × `max_input_bytes` 的交互只测了 3 轮 1 例**（`three_turns`）：没有测"很多小轮 + 一个巨大轮"的混合（预期：小轮按后缀保留、巨大轮只留头，但**未实测**）。
* **没有覆盖**：`MAX_DISTINCT_TERMS`、`min_score`、图谱侧的界（本单没碰图谱路径）；t125 的 **F2**（`max_content_bytes` 的静默丢弃）**故意没动**（见 §4(c)）。

---

## 7 复现命令（照抄可用）

```powershell
# 0) pins：改前（用于 before 读数）/ 改后
cd C:\Users\19410\Documents\ai\ruagent
Get-ChildItem crates\extract\src\*.rs,crates\extract\tests\bounded.rs,crates\daemon\src\extract_plane.rs |
  ForEach-Object { "$($_.Name) $((Get-FileHash $_.FullName -Algorithm SHA256).Hash)" }

# 1) 门禁（走团队包装器，排队是正常的）
$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"; $env:NO_PROXY='127.0.0.1,localhost,::1'
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-extract
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 clippy -p ruagent-extract --all-targets -DenyWarnings
cargo fmt --all --check

# 2) before/after 探针（rustc 直编，不碰仓库 target / 团队锁）
$p="$env:TEMP\t132-probe"; Remove-Item -Recurse -Force $p -EA SilentlyContinue
New-Item -ItemType Directory -Force "$p\before\src","$p\after\src" | Out-Null
git stash list > $null   # 不要 stash 共享树！before 的来源是：
#   改前 = 本节 §0 里那五个 sha 对应的字节（我是开工时先复制出来才改的）
Copy-Item crates\extract\src\*.rs "$p\after\src\"
foreach ($w in @('before','after')) {
  rustc --edition 2024 --crate-name ruagent_extract --crate-type lib "$p\$w\src\lib.rs" --out-dir "$p\$w\lib" -C opt-level=3 -C debuginfo=0
  rustc --edition 2024 "$p\probe.rs" --extern ruagent_extract="$p\$w\lib\libruagent_extract.rlib" -o "$p\probe_$w.exe" -C opt-level=3
}
& "$p\probe_before.exe" before   # §2 左两列
& "$p\probe_after.exe" after     # §2 右两列
rustc --edition 2024 "$p\probe_budget.rs" --extern ruagent_extract="$p\after\lib\libruagent_extract.rlib" -o "$p\probe_budget.exe" -C opt-level=2
& "$p\probe_budget.exe"          # bytes_skipped / truncated / 后缀语义 / 退化值

# 3) 负控（隔离副本，共享树零变异）
$n="$env:TEMP\t132-neg"; New-Item -ItemType Directory -Force "$n\src","$n\tests" | Out-Null
Copy-Item crates\extract\src\*.rs "$n\src\"; Copy-Item crates\extract\tests\bounded.rs "$n\tests\"; Copy-Item crates\extract\Cargo.toml "$n\Cargo.toml"
rustc --edition 2024 --crate-name ruagent_extract --crate-type lib "$n\src\lib.rs" --out-dir "$n\lib" -C opt-level=1
rustc --edition 2024 --test "$n\tests\bounded.rs" --extern ruagent_extract="$n\lib\libruagent_extract.rlib" -o "$n\bounded_a.exe" -C opt-level=1
& "$n\bounded_a.exe"                                     # ⇒ 9 passed
(Get-Content "$n\src\memory.rs" -Raw).Replace('byte_window(window, limits.max_input_bytes)','byte_window(window, usize::MAX)') |
  Set-Content "$n\src\memory.rs" -NoNewline -Encoding utf8NoBOM     # 唯一变异：把上限调成无穷
rustc --edition 2024 --crate-name ruagent_extract --crate-type lib "$n\src\lib.rs" --out-dir "$n\lib_b" -C opt-level=1
rustc --edition 2024 --test "$n\tests\bounded.rs" --extern ruagent_extract="$n\lib_b\libruagent_extract.rlib" -o "$n\bounded_b.exe" -C opt-level=1
& "$n\bounded_b.exe"                                     # ⇒ FAILED. 8 passed; 1 failed
```

---

## 8 纪律回执

* **写入集合** = 本报告 + 五个 inScope 文件；`crates/daemon/src/api.rs`/`distill.rs`、`crates/mcp`、`crates/graph`、`panel`、`.github`、`scripts` **一行未改**（收尾 `git status --porcelain -- crates/extract` 只列出我自己那 4 个文件；`api.rs`/`mcp` 等的 `M` 状态是**别人**的在途改动，我没有碰）。
* **构建路径**：仓库构建**全部**走 `scripts/cargo-team.ps1`（`test` 与 `clippy` 都在锁上排过队，日志可见 `waiting 10s`×5）；**同树没有用 `-TargetDir`、没有设 `CARGO_TARGET_DIR`**。探针/负控用 rustc 直编 `%TEMP%` 里的副本（无 cargo、不占锁、不写仓库 target）——这是 t19/t93 允许的隔离形态。
* **变异负控**：只在 `%TEMP%\t132-neg` 里做，窗口 **00:25:05–00:25:10**，共享树零变异；开窗前看过锁与在跑进程（当时无锁文件，有同伴的 cargo/rustc 在跑，我的构建排队）。
* **活环境**：本单**没有起任何 daemon**，没有临时端口/root，`127.0.0.1:8787` 与活库 `~/.ruagent` **完全未触碰**（§6 已说明为什么不需要 daemon）。
* **坐标回读（C33）**：报告里每一条 `file:line` 都是**写完之后的字节**上回读的（`lib.rs:390/410`、`memory.rs:51/75/81/95/162/177/204/222`、`text.rs:83/98/109`、`bounded.rs:345/377/385/402/423/431`、`extract_plane.rs:474/488`），并在收尾复查时五个源文件的 SHA-256 与门禁跑的那份**逐字节相同**（§0 改后 pins）——**门禁认证的就是这些字节**。
* **`my_temp_leftovers`（收尾）**：`%TEMP%\t132-probe` 与 `%TEMP%\t132-neg` **已删**（只删这两个具体路径——`%TEMP%` 里还有 11 个**不是我的** `t125*` 条目，本单一个都没碰，t102 的越界教训已落实）；我起的进程 **0**；共享树之外留下的东西 **0**。
* **我不是自审吗**：本单是**修复**，作者就是我，所以它**需要独立验证/评审**（t125 的审计报告是输入，不是本单的通过证明）。报告只给读数与自证，不给自己发 pass 结论。
