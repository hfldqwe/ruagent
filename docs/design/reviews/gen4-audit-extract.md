# t125 只读攻击式审计：`crates/extract`（零 token 抽取层）

> **性质**：**只读攻击式审计**。写入集合 = 本报告一行；`crates/**`、`panel/**`、`.github/**`、`scripts/**` **一行未改**（收尾读数：`git status --porcelain -- crates/extract` **为空**，五个源文件 SHA-256 与开工时逐字节相同）。
> **结论一句话**：crate 的四项核心声称**成立且被机械证明**（无依赖 ⇒ 不可能有模型/网络调用；`max_per_input` 在 n 与 n+1 两侧都精确封顶且 `truncated` 能区分「正好 n」与「被砍在 n」；同输入跨进程逐字节相同；图谱路径有界）。**两条声称不成立或其范围被夸大**：**「bounded」在转录（记忆）路径上不约束工作量**（无按字节开窗，实测时间随输入线性增长、峰值分配 = 输入的 16 倍），**「候选带证据」缺位置坐标**（`split_sentences` 算出的 offset 被丢弃，且全仓**没有任何消费者**读 `origin`）。另有两处**机械证明的缺口**：gold fixture 不钉 `origin`/`source`（实测篡改被接受）、纯度扫描的文件清单是**手写常量**（隔离副本负控：树里有 `std::fs` 扫描仍判 clean）。**无 blocker/high；findings 8 条（medium 4 / low 4）。**

---

## 0 元数据：采样面、时间窗、工具、pins

| | |
| --- | --- |
| 采样面 | `crates/extract/{Cargo.toml,src/{lib,graph,memory,rules,text}.rs,tests/{bounded,memory-gold,graph-gold,requirement-markers}.rs,tests/gold/*.json}`；调用面 `crates/daemon/src/{extract_plane.rs,distill.rs,api.rs,capability.rs,config.rs}`（**只读**）；设计面 `docs/plans/capability-plugins-design.md` |
| 时间窗 | **2026-10-04 00:0x–00:18（+08:00；AST 读数见 §7 的 `Get-Date`，本报告写于 00:18:01）**。**HEAD 在侦察期间从 `873bb90` → `db46f6c` → `e8aaead` 连移三次（同伴在提交）**，所以我不把读数钉在提交号上，而钉在**文件哈希**上（下一条） |
| 文件 pins（SHA-256 前缀，复制后与仓库比对 = **True**） | `graph.rs D0FB53F0564B9762` · `lib.rs 130AEABEBD8059ED` · `memory.rs 77A579AD7DF66B60` · `rules.rs BAB5489C7865AB33` · `text.rs F2CB567678B9AB13` |
| 被判对象 | `ruagent-extract` crate（源码 ~2755 行，测试 ~1247 行） |
| 我的实测工具（**只读、且不碰仓库构建锁**） | ① 仓库入口测试：`powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-extract` ⇒ **37 passed / 0 failed / exit 0**（6 lib + 8 bounded + 8 graph-gold + 3 memory-gold + 12 requirement-markers + 0 doctests；两次运行：5.3s / 2.0s，第二次排队 10s×11 后拿到锁）② 攻击探针：把五个源文件**逐字节复制**到 `%TEMP%\t125-probe\src`，**用 rustc 直接编译**（`rustc --edition 2024 --crate-type lib`，无 cargo、无 `CARGO_TARGET_DIR`、不碰仓库 target 与团队锁），再编译 `probe.rs` 链接该 rlib |
| 探针的隔离声明 | 探针只写 `%TEMP%\t125-probe`；**唯一的变异**在**隔离副本**里做（见 §2.9 / §8），窗口 **2026-10-04 00:16:22–00:16:22**，共享树未参与。**注意 `%TEMP%` 里另有 11 个 `t125*` 条目不是我的**（`t125run1.log`…`t125c`，2026-09-24 / 10-01 的同伴残留），本单**只删自己那一个具体路径**、一个都没碰（t102 的越界教训） |

**读数的可信度分级**（我自己的工具也有 bug，先说清）：探针里的计数分配器在 `realloc` 路径上记账有缺陷，20 行 perf 里有 6 行的 `peak_bytes` 读到 `usize::MAX`（明显是哨兵溢出）——**这些行的内存数字作废**；可用的是 `peak = 16×输入` 的那几行（Ascii 无标点/控制字符）与**全部 ms 列**。这一点在 §2.3 逐行标注。

---

## 1 §1 声称表（谁声称了什么，逐条坐标）

后面每一条攻击都对着一张编号；坐标全部是我读过的原文位置。

| # | 声称 | 出处（坐标） |
| --- | --- | --- |
| C1 | `zero-token deterministic extraction`：抽取路径**零 token**，无模型、无网络 | `crates/extract/src/lib.rs:1-27`（"Zero-token deterministic extraction — the free tier"…"it never invokes an agent or a model"）· `crates/extract/Cargo.toml:11-19`（"There is deliberately no dependencies table: purity is this crate's defining property"）· `docs/plans/capability-plugins-design.md:10,126,148,193,1228`（`Free` = zero tokens） |
| C2 | 纯度的机械证明：扫描每个源文件 + 扫描清单 | `lib.rs:24-27`（"`tests/bounded.rs` proves that mechanically: it scans **every source file**"）· `tests/bounded.rs:14-20`（`SOURCES`，5 项**手写常量**）· `:31-51`（`FORBIDDEN` 19 项子串）· `:53-88`（断言体） |
| C3 | 确定性：同输入 ⇒ 逐字节相同，与迭代顺序/进程环境无关 | `lib.rs:29-36`（"no clock, no randomness, no environment read, and no state carried between calls"）· `graph.rs:19-21`（"every sort ends with a tie-break on the name"）· `tests/bounded.rs:120-151`（32 次 `format!("{:?}")` 比对） |
| C4 | 有界：输出被 `max_per_input` 封顶；每个 body ≤ `max_content_bytes` 否则**丢弃不截断**；名字 ≤ `max_entity_name_chars`；转录窗口 = 最后 `max_turns`；文档窗口 = 前 `max_text_bytes`；词表 ≤ `MAX_DISTINCT_TERMS`；**"Work is linear in the windowed input"** | `lib.rs:38-47`（原文）· `rules.rs:392`（`MAX_DISTINCT_TERMS = 4096`）· `tests/bounded.rs:294-354`（`pathological_inputs_stay_bounded`，文件名本身就是承诺） |
| C5 | 四个「齿」封顶：`max_per_input` / `max_docs_per_pass` / `min_score` / `weight`（本队任务单的措辞，设计里分三处） | `lib.rs:355-397`（`ExtractLimits` 与两个默认集）· `docs/plans/capability-plugins-design.md:248-264,316-319`（选项键与范围）· `:481-521`（policy.toml 样例）· `:1342-1354`（哪些 capability 声明哪些键）· `:1730-1733,1640-1662`（`max_docs_per_pass` = 摄取扫描的界）· `:626,1342`（`weight` = 召回腿融合权重） |
| C6 | 证据链：每个候选带 `origin`（来源坐标）+ `source`（调用方标签）+ 逐字文本 + `dedup_key`，供 applier 归因 | `lib.rs:97-105`（"Every candidate carries two things the applier needs and cannot recompute: where it came from (`origin`, plus `source` …) and its `dedup_key`"）· `:210-226`（`CandidateOrigin`）· `:243,306,335`（三个字段） |
| C7 | `min_score` 是 §9.3 的排名闸门 | `lib.rs:362-364`（"Graph candidates below this ranking are dropped"）· `graph.rs:163-167` · `tests/bounded.rs:273-291` |
| C8 | 文档开窗"回退到前一个段落边界，段落绝不会被抽一半" | `graph.rs:40-42`（调用点注释）· `graph.rs:221-238`（`window_text` 的文档注释："cut back to the previous paragraph boundary so a paragraph is never half-extracted. The cut never splits a character."） |
| C9 | gold fixture 把行为钉住（"a later algorithm change shows up as a diff instead of as a silent behaviour change"） | `tests/memory-gold.rs:1-35`（render = `rule|store|confidence|content`）· `tests/graph-gold.rs:1-69`（render_entity 6 段 / render_relation 7 段） |
| C10 | 默认抽取路由 | `crates/daemon/src/extract_plane.rs:61-78`（`Extractor::{Acp,Rules,Both}`，`"acp" => …` 注释与文案称 **the default** 于 `:75,:300`）· `:130-133`（`explicit`）· 设计 `:632-633`（两个 capability 的 tier = `free`、**enabled = off**） |

---

## 2 六项攻击：方式 + 读数

### 2.1 攻击「零 token」（C1）——**成立，且是结构性成立**

* **方式**：不读注释，读**能不能链接进来**：清单 + 锁定图的依赖边（这是最硬的一层：没有依赖就无从发起调用）。
* **读数**：
  * `crates/extract/Cargo.toml` **没有 `[dependencies]` 表**；只有 `[dev-dependencies] serde_json = "1"`。
  * `Cargo.lock` 里 `ruagent-extract` 的 `dependencies` = **只有 `serde_json` 一行**（即上面那条 dev-dep；dev-dep 不进库产物）。
  * 探针的纯度扫描（忠实复现 `bounded.rs:31-51` 的 19 项 `FORBIDDEN`）在五个源文件上 `scan_verdict_clean=true`；且 `std::process`/`std::net`/`unsafe`/`extern crate` 全在禁列 ⇒ **没有 FFI/无 unsafe 的路径可以绕过**。
  * **调用面（不只 crate 内部）**：daemon 的抽取入口是 `Extractor::{Acp, Rules, Both}`（`extract_plane.rs:61-78`）；`Rules` 臂就是 `ruagent_extract::memory_candidates_with`（`:456`）并把 `source: Some(ExtractSource::Rules)` 标出来（`:494`）；**没有任何中间层在调用前后发起模型调用**（`Rules` 臂的函数体 `:440-497` 里没有 `ask_agent`，模型调用只在 `AcpExtractor::ask`，`:519-527`）。
* **越界在哪（这是本条要交的东西）**：**默认值不是零 token**。`extract_plane.rs:75` 与 `:300` 把 `"acp"` 写成 **the default**，而零 token 的那一半要靠 capability `session_extract_rules`（设计 `:632-633`：tier = `free`，**enabled = off**）打开 ⇒ **「zero-token extraction」是"可用且需要显式打开"，不是"默认如此"**。一个只读 `AGENTS.md`/设计摘要的读者会把默认路径当成零 token。→ **F7**（claim-map，low）。
* **我试过但没查到**：`for` 循环里没有任何 `std::time`/环境读取（禁列覆盖）；没有 `include!`/`#[path]` 把外部文件拉进来（`lib.rs:114-117` 只有四个 `pub mod`，目录里正好五个文件）——这一条**今天**是干净的（见 §2.9 的负控说明"今天完整、将来会漏"）。

### 2.2 攻击「四个上限真的封顶吗」（C4/C5/C7）——**逐个给边界读数**

**方式**：探针直接调公开入口，取 `n` 与 `n+1` 两侧。所有读数出自 `probe.exe caps`。

| 齿 | 上限值 / 生效坐标 | n 侧读数 | n+1 侧读数 | 判定 |
| --- | --- | --- | --- | --- |
| `max_per_input`（记忆） | 默认 **32**（`lib.rs:378`），生效 `memory.rs:116-119` | `turns=5,cap=5 ⇒ len=5, truncated=false` | `turns=6 ⇒ len=5, truncated=true`；`turns=7 ⇒ len=5,truncated=true`；`turns=3/4 ⇒ 3/4,false` | ✅ **精确封顶，且 `truncated` 能把「正好 5」与「被砍在 5」分开** |
| `max_per_input`（图谱，实体+关系共享预算） | 默认 **96**（`lib.rs:391-396`），生效 `graph.rs:168-177` + `:205-211` | `terms=4(+1 标题)=5,cap=5 ⇒ entities=5,relations=0,truncated=false` | `terms=5 ⇒ 6 实体 ⇒ entities=5,truncated=true`；`terms=6 ⇒ truncated=true` | ✅ 同上（关系按 `budget = cap - entities` 拿剩下的，实体优先，`graph.rs:203-211`） |
| `min_score` | 默认 **0.0**（`lib.rs:379`），生效 `graph.rs:167`（`retain(e.score >= min_score)`） | `min_score=0.25` ⇒ 保留全部 5 个（含 4 个 0.25，**`>=` 是闭的**） | `min_score=0.2500001` ⇒ 只剩 `Demo`(0.5)；`0.3` ⇒ 同 | ⚠️ **齿有效但出厂值不咬**：`min_score=0.0` ⇒ 5 个实体全在（0.25 ≥ 0.0）→ **F6** |
| `max_content_bytes` | 默认 **400**，生效 `memory.rs:150-152`（`sentence.len() > cap ⇒ continue`） | 句子 46 字节，`cap=46 ⇒ candidates=1` | `cap=45 ⇒ candidates=0` | ✅ 边界精确（**且是按字节**，见 F8） |
| `max_entity_name_chars` | 默认 **60**，生效 `graph.rs:86,517,544,722` | 名字 60 **字符** ⇒ entities=1 | 61 字符 ⇒ entities=0 | ✅ 边界精确（**按字符**——与上一条的按字节不同轴） |
| `max_turns` | 默认 **2000**，生效 `memory.rs:57-58` | `turns=10, cap=3 ⇒ turns_skipped=7`，`origin.index` = **7,8,9（原输入下标，不是窗口内下标）** | — | ✅ 且**窗口平移没有污染证据坐标** |
| `max_text_bytes` | 默认 256 KiB（`lib.rs:380`），生效 `graph.rs:42,223-238` | 输入 200,000 B，`cap=64 KiB ⇒ bytes_skipped=134,464, truncated=true` | — | ⚠️ 字节数精确，但**段落承诺不成立**（§2.3 / F5） |
| `MAX_DISTINCT_TERMS` | **4096**（`rules.rs:392`），生效 `graph.rs:335` | — | 未单独取边界读数（见 §6 未覆盖） | ⏸ 未测 |
| `max_docs_per_pass` | **不在本 crate**：daemon 的摄取扫描（`capability.rs:434 max_docs_per_pass: Some(20)`、`api.rs:5184`、设计 `:1730-1733/1640-1662`） | — | — | ⏸ **未取读数**（要跑 daemon 摄取测试，超出本单范围）→ §6 |
| `weight` | **与抽取路径无关**：召回腿融合权重（设计 `:626,1342-1354`） | — | — | ⏸ 不在本 crate 的抽取路径上（claim-map 问题 → F7） |

**「负数/零/极大值/重复输入」这四项也测了**：
* **零**：`bounded.rs:340-354` 用全零 limits（含 `min_score=1.0`）断言输出为空、不 panic；我复现了全零图/记忆两侧（探针 caps 里的 `max_content_bytes` 与 `min_turns` 组合路径）⇒ **不 panic** ✅。
* **重复输入**：`"以后统一用 pnpm。"×50,000` ⇒ `candidates=1`（dedup 生效，`bounded.rs:314-327` 已钉）✅。
* **极大值**：`max_per_input` 从 5 升到 100（`probe det cap=100_no_drop`）⇒ 40 个候选全出、`truncated=false` ✅。
* **负数**：字段是 `usize`/`f32`，负数进不来；`min_score` 为负等价于 0.0（`score >= 负数` 恒真）⇒ 不产生越界。⚠️ **`min_score = f32::NAN` 会让 `retain` 全丢且 `truncated` 仍 false**——但设计把 `min_score` 校验为 **finite 且 0.0..=1.0**（设计 `:316`），daemon 无法产出 NaN ⇒ **不列为 finding，进 §4 猜想 G2**。

### 2.3 攻击「`bounded.rs` 这个名字在承诺有界」（C4）——**图谱侧成立；转录侧不约束工作量**

**方式**：探针用计数分配器量峰值分配 + 墙钟时间，输入按 8–16 倍递增。

| 输入类别 | 1 MB | 4 MB | 16 MB | 16×/1× 时间比 | 峰值分配读数 |
| --- | --- | --- | --- | --- | --- |
| **记忆**·单轮无标点（一整句） | 21.6 ms | 78.2 ms | 235.1 ms | 10.9× | **16 MiB / 64 MiB / 256 MiB = 输入的 16 倍**（可用读数） |
| **记忆**·单轮全命中规则句（去重后 1 条） | 137.0 ms | 513.1 ms | **1942.7 ms** | 14.2× | `usize::MAX`（**我的探针计数器缺陷，作废**） |
| **记忆**·全 CJK 句 | 67.9 ms | 293.6 ms | 1241.7 ms | 18.3× | 作废 |
| **记忆**·控制字符（`\x01\x02\x07\x1b`） | 20.4 ms | 49.2 ms | 212.1 ms | 10.4× | 16 MiB/64 MiB/256 MiB（可用） |
| **图谱**·普通文档（`\n\n` 段落） | 34.0 ms | 22.6 ms | 39.4 ms | **≈1×（平台）** | 作废 |
| **图谱**·深嵌套 `[[[[…]]]]`（单行无空行） | 6.9 ms | 5.0 ms | 4.8 ms | **≈1×** | 作废 |
| **图谱**·混排 1 行（中文+emoji+反引号+全角括号，~7.7 MB） | 133.8 ms | — | — | — | 作废 |

* **图谱侧：有界成立** ✅ 三个尺寸下时间**基本不动**（`graph_doc` 34/23/39 ms）、输出**恒为 3**（超出的 16 MB 被 `max_text_bytes` 切掉、`bytes_skipped` 报出）——这是 C4 的强证据。
* **转录侧：工作量的界只是「轮数」** ✗ `memory.rs:57-58` 按**条数**开窗（`max_turns`），**没有按字节的窗**；`memory.rs:147` 把**整轮文本**交给 `split_sentences`，而 `text.rs:78-83` 先 `text.char_indices().collect()` ⇒ **每字符 16 字节的表**（ASCII 下 = 输入的 16 倍峰值分配，实测 1/4/16 MB → 16/64/256 MiB）。时间对字节**线性**（14.2× for 16×），不是二次爆炸 ✅，但**线性 × 无界字节 = 无界**：单轮 1 GB 文本 ⇒ 约 19 s、约 16 GB 分配。`tests/bounded.rs:314-327` 那个 1.1 MB 单轮用例只断言**输出条数 = 1**，**没有任何时间/内存断言** ⇒ 文件名承诺的东西没有被测。→ **F1**。
* daemon 侧**没有**替我兜住：`turn_for_extraction`（`extract_plane.rs:410-436`）只做平台提示词裁剪，**没有字节上限**。
* **能造出无界就是 finding；这些我都试过了**：超长单句 ✅（无界工作量）、无句末标点 ✅（同）、巨量重复 ✅（**更慢**：121 ms/MB）、深嵌套括号 ✅（**很快，无爆炸**）、全 CJK ✅、混排 ✅、控制字符 ✅。**没有观察到任何二次/爆炸行为**（这是正读数）。

### 2.4 攻击「gold 测试能不能被假输出骗过」（C9）——**能，两处，已实测**

**方式**：在探针里**复现**两个 gold 文件的比较逻辑（`memory-gold.rs:15-35` 的 `render`、`:106-119` 的断言；`graph-gold.rs:10-69` 的 `render_entity`/`render_relation`、`:112-131` 的断言），把**真输出**与**篡改输出**各自渲染后比对 + 跑一遍 gold 自己的断言。**没有改仓库任何文件**。

| 篡改 | 渲染串相同？ | gold 自身断言接受？ | 结论 |
| --- | --- | --- | --- |
| 记忆：**每个候选 `origin.index - 1`** 且 `role` 翻成 `Assistant` | `true` | **`true`（通过！）** | ✗ **一条记错轮次/记错角色（= 用户还是助手）的输出能通过 gold** |
| 记忆：`source = "WRONG-SESSION"` | `true` | `true`（gold **从不**提 `source`） | ✗ 调用方的溯源标签不受 gold 约束 |
| 记忆：`origin.index + 1` | `true` | `false`（`:109-113` 的 `index < turns.len()` 兜住了它） | ✅ 这条被兜住（我把它一并列出，避免把"能骗过"说过头） |
| 图谱：**`origin.section = Some(99)` → `source = ""`** | `true` / `true` | **`true`（通过！）**；`!truncated`、`bytes_skipped==0`、关系端点检查全过 | ✗ **图谱侧连 `origin` 和 `source` 都不看**——而 `source` 正是 applier 写进行的文档/chunk 标签 |

* **为什么这是 finding 而不是"测试风格"**：`origin` 是 C6 声称的**证据坐标**（§2.6 又证明它没有消费者），gold fixture 是唯一钉住候选形状的机制，而它恰好**不钉**证据字段。
* **具体改法（可证伪的控制）**：把 `origin`/`source` 放进渲染串，例如记忆 `render` 改为 `format!("{}|{}|{}|{}|{:?}|{}", rule, store, confidence, content, origin, source)`、图谱 entity/relation 的 render 末尾各加 `|{:?}|{}`；改动后**同一个篡改必须变红**（今天它通过）。→ **F3**。

### 2.5 攻击「确定性/幂等，含跨进程」（C3）——**成立**

* **方式**：同一输入在进程内跑两次比对 `format!("{:?}")` 的 FNV-1a 指纹；再**把探针进程跑两次**比对指纹（跨进程）；再改输入顺序看输出。
* **读数**：
  * 进程内：`memory_in_process same=true hash=e6bccdbd51403179 n=32`、`graph_in_process same=true hash=0112152a6d458c01 n=41`。
  * **跨进程**：两趟独立的 `probe.exe det` 输出**同一个指纹**（`e6bccdbd51403179` / `0112152a6d458c01`）⇒ 逐字节相同 ✅。
  * **顺序敏感性（区分两种情形，别混为一谈）**：
    * 默认 `max_per_input=32` 而候选 40 个 ⇒ 反转输入后**存活集合不同**（`set_same=false`）——这是**上限的下游效应**（谁被留下取决于次序），不是不确定性。
    * 把上限抬到 100（无不丢）再反转 ⇒ **`set_same=true`、`order_same=false`、`truncated=false/false`** ✅：**事实集合与顺序无关，只有排列次序跟随出现次序**。
    * 一轮内两条同规则句子互换 ⇒ 输出次序跟随文本次序（`first=["green thread","work stealing"]` / `second=["work stealing","green thread"]`）——与 `memory.rs:100-103`（occurrence index 升序、稳定排序）**一致，属设计**。
  * 机制侧：五个源文件里**没有 `HashMap`/`HashSet`/`Instant`/`SystemTime`/`rand::`**（禁列覆盖），所有排序用 `sort_by`/`sort_by_key`（**稳定**）且 `total_cmp`（避免 NaN 造成的非全序），`graph.rs:157-162/936-944` 以 `(score, first, name/…, dst)` 收尾 ⇒ 与注释一致 ✅。→ **本条无 finding**。

### 2.6 攻击「候选带不带证据」（C6）——**带身份与逐字文本；缺位置坐标，且现有的那个也没人读**

* **方式**：打出三种候选的完整 `Debug`；再全仓 grep 谁**读**这些字段。
* **读数（原样）**：
  * `memory_candidate = MemoryCandidate { store: Observation, namespace: "user", content: "以后统一用工具0处理这件事。", confidence: Unconfirmed, rule: "user_preference", origin: Turn { index: 0, role: User }, source: "" }`
  * `graph_entity = EntityCandidate { name: "Heading", kind: Some("tool"), summary: Some("See [[Wiki Target]] and \`code_id\` here."), aliases: [], score: 0.5, rule: "heading_entity", origin: Document { section: Some(1) }, source: "knowledge/rust.md" }`
  * 关系：`fact` = 逐字句、`valid_at` = 只在句中写明 ISO 时间时才有（`lib.rs:328-332`，无时钟 ⇒ 不能伪造"现在是事件时间"）✅
  * `split_sentences_offsets=[(0, "第一句。"), (12, "第二句。"), (24, "第三句。")]` ⇒ **offset 存在且可取**
  * `candidate_has_offset_field=false` ⇒ **三个候选结构体都没有字节/字符区间字段**
  * 全仓 grep `.origin|CandidateOrigin`：**消费者为零**——命中的只有 extract 自己的定义/构造（`lib.rs:243/306/335`、`memory.rs:309`、`graph.rs:188/958`）与 daemon 在 **ACP 臂**构造 `CandidateOrigin::Document { section: None }`（`extract_plane.rs:561/575/590`）；`Rules` 臂是**原样透传**（`:491 memories: pass.candidates`），**没有任何地方读它**（`runs.rs:693` 是无关的 `original_prompt`）。
* **因此今天带的是**：`source`（调用方标签：session key / 文档或 chunk id）、`origin`（轮次下标+角色 / 章节下标）、逐字 `content`/`fact`/`summary`、`rule`、`confidence`、`dedup_key`。**缺的是**：指向原文的**位置**（`memory.rs:147` 把算好的 offset 丢掉：`for (_, sentence) in text::split_sentences(&turn.text)`；`graph.rs:55` 只拿它算章节，不写进候选），以及**任何消费者**把轮次/章节坐标带进存储。→ **F4**。
  * 补充（不是缺陷）：`source` 在 `memory_candidates()`（无 `_for` 的入口）里**必然为空**，且 `lib.rs:244-248` 明说"这一层不可能知道 session key，读点什么正是它不能做的" ✅ 诚实的空值。

### 2.7 攻击「默认路由与能力开关」（C10，§2.1 的延伸）

`extract_plane.rs:75` `"acp" => Ok(Extractor::Acp)`、`:300` 错误文案自称 `the default`；`session_extract_rules` / `knowledge_ingest_graph` 在设计表里 tier=`free`、**enabled=off**（设计 `:632-633`）⇒ 零 token 层"存在、可开、默认不开"。**这也解释了为什么 §2.3 的无界转录风险今天不是生产路径**（默认走 ACP，而 ACP 有 token 成本与自己的上限）——但一旦按设计打开 free tier，F1 就落到真实路径上。这条与 F1/F7 一起读。

### 2.8 攻击「`window_text` 的段落承诺」（C8）——**承诺与代码不符，已实测到中段切断**

* **读数**：200,000 字节、**通篇没有空行**的单段落文档，`max_text_bytes=64 KiB` ⇒ `bytes_skipped=134,464`、`truncated=true`、窗口正好是 65,536 字节。代码 `graph.rs:232-235`：找到 `"\n\n"` 就回退到段落边界，**找不到就用 `None => cut`（生切）** ⇒ 段落**被抽了一半**，与 `:221-222` 的注释直接矛盾。**字符边界安全** ✅（`:228-230` 逐字节回退到 `is_char_boundary`；我的 1/4/16 MB 全 CJK 输入无 panic，`graph.rs:340-354` 也钉了多字节边界）。
* **后果我试了但没造出来**：这条生切会让窗口末尾出现"半句话"，而 `lib.rs:41-43` 声称 body 永远"丢弃不截断"。我构造的单段落 200 KB 输入里，那个碎片**没有**变成候选（实体规则的 df≥2/df≥3 闸门与反引号配对把它挡了）⇒ **后果进 §4 猜想 G1，不列入 finding**；**矛盾本身**（注释 vs `None => cut`）列入 **F5（low）**。

### 2.9 攻击「纯度的机械证明」（C2）——**今天完整，将来会漏；隔离副本负控已打红**

* **读数（隔离副本，窗口 00:16:22–00:16:22）**：
  * 变异**前**：`scan_verdict_clean=true, files_listed=5, unscanned_source_files=0` ⇒ 五个源文件**全部**被扫（今天不缺）。
  * 在**副本**里加 `src/io_probe.rs`（含 `use std::fs;` 与一个真的 `std::fs::metadata` 调用）+ `lib.rs` 末尾加 `pub mod io_probe;` ⇒
    `scan_verdict_clean=true`（**纯度测试照样绿**）而 `unscanned_source_files=1|io_probe.rs(forbidden_hits=["std::fs"])`。
  * 该副本**能编译**（`mutated lib built: True`）⇒ 这是**真实的 I/O 可达路径**，不是纸面假设。
* **机制**：`bounded.rs:14-20` 的 `SOURCES` 是**手写 5 项常量**，而"哪些文件属于这个 crate"由 `lib.rs:114-117` 的模块声明决定——两者今天一致，将来不一致时**扫描会静默停止覆盖**（正是本队 `t93/t81` 那条纪律的同类：一个"没报错的绿灯"比红灯更危险）。→ **F9（low）**；改法：让扫描**枚举目录**（或断言 `src/*.rs` 数量 == `SOURCES.len()`），这样新增文件就会红。
* **纪律**：变异只在 `%TEMP%\t125-probe\src`（副本）里做；共享树**未参与**，收尾 `git status --porcelain -- crates/extract` 为空、五个源文件 SHA-256 与开工相同。

---

## 3 一次攻击之后，哪些声称**活着**（正读数，别只记 findigns）

1. **零 token 是结构性的**：没有 `[dependencies]` ⇒ 没有可链接的 HTTP/模型栈；禁列覆盖 `unsafe`/`extern crate` ⇒ 无可绕过的 FFI（§2.1）。
2. **`max_per_input` 是精确的齿**：记忆 4/5/6 → 4/5/5，`truncated=false/false/true`；图谱同（§2.2）。
3. **`truncated` 的语义做对了**：`exactly n` 与 `capped at n` 可区分（这正是 `extract_plane.rs:457-489` 记下的 t9 修复口径）。
4. **确定性是双份的**：进程内 + 跨进程指纹相同；稳定排序 + `total_cmp`；顺序只影响次序与（有上限时的）存活集合，不影响事实集合（§2.5）。
5. **图谱路径真有界**：1/4/16 MB 时间基本不动、输出恒为 3 条、`bytes_skipped` 报出截断（§2.3）。
6. **多字节安全**：全 CJK / emoji / 全角括号 / 深嵌套 1–16 MB 一律不 panic（且 `bounded.rs:399-458` 的表覆盖了 full-width 括号 panic 的回归形状）。
7. **事件时间不能伪造**：无时钟、`valid_at` 只在句子明写 ISO 时间时才有（`lib.rs:328-332` + 实测 `valid_at=-`/Some 两例）。
8. **自查意识**：crate 自己记了 6 条"与设计文本的偏离"（`lib.rs:49-95`），其中两条（`graph_default` 的 96、`wiki_link` 第五条规则）与设计确有张力但**写明了**——这比沉默偏离好得多。

---

## 4 findings（按分级；每条带坐标 + 读数 + 可证伪的证伪方式）

> 分级口径：**blocker/high 我一条都没给**——理由是：这个 crate 在它声称的核心（纯、确定、输出封顶、图谱有界、无 panic）上**实测成立**，而没有一条 finding 会把**错误的结论**写进记忆/图谱（F2 是"该抽的没抽"，F1 是"慢/吃内存"，F3/F9 是测试与证明的缺口，F4 是证据缺一维，都不是伪造事实）。凡我认为"可能伪造事实"的路径（半句当逐字事实）**我没能造出来**，按要求放进 §4 猜想而不是 finding。

| id | sev | 问题 | 坐标 | 读数 | 可证伪方式 |
| --- | --- | --- | --- | --- | --- |
| **F1** | medium | **转录路径没有按字节的界**：`max_turns` 只界条数，单轮文本整段进入 `split_sentences`，且先建 `char_indices()` 表（每字符 16 B）⇒ 时间随字节线性、峰值分配 = 输入 16 倍（ASCII），**无上限**。`tests/bounded.rs`（文件名即承诺）对此只断言"输出条数 == 1"，**没有时间/内存断言**；daemon 侧 `turn_for_extraction` 不按字节裁剪 | `memory.rs:57-58`、`memory.rs:147`、`text.rs:78-83`、`lib.rs:38-47`、`tests/bounded.rs:314-327`、`extract_plane.rs:410-436` | `memory_one_turn_repeat`：137 → 513 → **1943 ms**（1/4/16 MB，14.2×）；`memory_one_turn_no_punct` 峰值 **16/64/256 MiB = 16× 输入**；图谱同尺寸 34/23/39 ms（平台） | 给 `ExtractLimits` 加一个输入字节上限（或让 daemon 在 `turn_for_extraction` 裁字节）后重跑 `probe.exe perf`：`memory_*_16MB` 的 ms 应回落到与 1 MB 同量级、峰值分配不再随输入增长；今天它线性增长 |
| **F2** | medium | **`max_content_bytes` 的丢弃是静默的**：超长句 `continue`，`truncated`/任何计数都不动 ⇒ 调用方无法区分"句子太长被丢"、"没有规则命中"、"去重了"。这与本 crate 自己在 `extract_plane.rs:457-489` 立的规矩（"调用方必须能区分 exactly 32 与 capped at 32"）是同一种缺陷，只在**另一个位置**复发 | `memory.rs:150-152`、`graph.rs:60-64,889`、`lib.rs:403-415`（`truncated` 只覆盖两种损失）、`tests/memory-gold.rs:145-159`（断言会丢，但没断言会报） | 句子 46 字节，`cap=45` ⇒ `candidates=0` 且 `truncated=false`、`suppressed=0`、`turns_skipped=0` ⇒ **全零读数** | 把 `ExtractLimits{max_content_bytes: 45}` 下的 `MemoryCandidates` 打出来：今天五个计数器全是"没发生"；加一个 `dropped_long` 计数（或让 `truncated` 覆盖它）后同一输入应给出**非零**读数 |
| **F3** | medium | **gold fixture 不钉证据字段**：记忆侧渲染只比 `rule|store|confidence|content`（`origin` 只查"是 Turn 且 index < len"，`role` 与 `source` 完全不查）；图谱侧渲染 6/7 段里**没有 `origin`、没有 `source`** | `tests/memory-gold.rs:15-35,106-119`；`tests/graph-gold.rs:10-69,112-131` | 篡改 `origin.index-1` + `role` 翻转 ⇒ 渲染相同且 **gold 接受=true**；`source="WRONG-SESSION"` ⇒ 渲染相同、gold 从不提它；图谱 `section=Some(99)`+`source=""` ⇒ 两个渲染串都相同且 `!truncated/bytes_skipped==0/端点检查` 全过 ⇒ **接受=true**（另一条 `index+1` 被范围检查兜住，已如实列出） | 把 `origin`/`source` 加进两个 `render*` 的串（或每个 case 增 `expect_origin`）⇒ 同一篡改必须**变红**；今天它绿 |
| **F4** | medium | **证据链缺位置一维，且现有坐标无人消费**：`split_sentences` 返回 `(offset, sentence)`，`memory.rs` 直接丢弃、`graph.rs` 只用于章节计算，三个候选结构体都没有区间字段；全仓**零消费者**读 `origin`（只有定义、构造、以及 Rules 臂的原样透传）⇒ 一行记忆/图谱边无法回溯到**具体轮次/章节** | `memory.rs:147`、`graph.rs:55`、`text.rs:78`、`lib.rs:210-226,229-249,287-310,321-339`、`extract_plane.rs:491`（透传无消费） | `split_sentences_offsets=[(0,"第一句。"),(12,…),(24,…)]`；`candidate_has_offset_field=false`；grep `.origin` 命中**全是定义/构造**，无读取点 | 给候选加 `span: (usize, usize)`（或让 applier 把 `origin` 写进行）⇒ 同一输入的候选应能定位回原文区间；今天 `format!("{:?}", candidate)` 里没有任何能定位原文的字段 |
| **F5** | low | **`window_text` 的注释承诺与代码不符**：注释说"回退到前一个段落边界，段落绝不会被抽一半"，但找不到 `\n\n` 时 `None => cut` 就是生切 | `graph.rs:221-238`（注释 `:221-222`，兜底 `:232-235`）、调用点注释 `graph.rs:40-42` | 200,000 B 单段落文档、`cap=64 KiB` ⇒ `bytes_skipped=134,464`、窗口恰 65,536 B ⇒ **中间没有段落边界可用，窗口末尾落在段落内部** | 断言"窗口末尾要么是段落边界、要么 == 输入末尾"（即 `input[..end]` 以 `\n\n` 结束或 `end == input.len()`）：今天单段落输入必红；改成按句末标点回退或改注释即绿 |
| **F6** | low | **`min_score` 齿在出厂值上不咬**：默认 0.0，而图谱实体最低分 0.25（`intrinsic_score(df=1)`）⇒ 永不裁剪；设计里两个抽取 capability 只声明 `max_per_input`/`max_docs_per_pass`（没有 `min_score`）⇒ 生产路径上这个"齿"从未被设过 | `lib.rs:379`、`lib.rs:391-396`、`graph.rs:101,167`、设计 `:1344-1346`（声明键）、`:316`（范围校验） | 默认 limits ⇒ 5 个实体含 4 个 0.25 全在；`min_score=0.25` ⇒ 仍全在（`>=` 闭）；`0.2500001`/`0.3` ⇒ 只剩 0.5 那个 | 把默认改成 0.25 并重跑 `probe.exe caps`：`min_score|default` 那行应从 5 个实体变成 1 个；今天"默认=0.0"与"齿不咬"同时为真 |
| **F7** | low | **"四个齿"不在同一个 crate 里**（claim-map）：`max_per_input` 在 extract；`min_score` 在 extract（见 F6 为惰性）；`max_docs_per_pass` 在 **daemon 的摄取扫描**；`weight` 是**召回腿融合权重**，与抽取路径无关。而默认抽取路由是 **acp（花 token）**，零 token 层默认 **off** | `lib.rs:355-397`；`capability.rs:434`、`api.rs:5184`、设计 `:1730-1733,1640-1662`；设计 `:626,1342-1354`；`extract_plane.rs:75,300`；设计 `:632-633` | 逐条边界读数见 §2.2（能测的两条我测了，另外两条见 §6 未覆盖）；默认路由与开关的坐标见 §2.1/§2.7 | 审计"抽取层的上限"时按这四个名字 grep：两个命中在 `crates/extract`，两个命中的生效位置在 `crates/daemon` 与召回融合。要么在文档里写清归属，要么把 `weight` 从"抽取上限"的说法里去掉 |
| **F8** | low | **内容预算按字节、实体名预算按字符 ⇒ 脚本间不对称**：同一个 `max_content_bytes=400` 下，140 个 ASCII 字符（156 B）留着，140 个汉字（416 B）被丢 ⇒ 汉语的有效句长预算约为 ASCII 的 1/3（400 B ≈ 133 汉字） | `lib.rs:367-369`（字段名含 `_bytes` / `_chars`，命名是诚实的）、`memory.rs:150`、`graph.rs:517` | `script=ascii chars=140 bytes=156 cap=400 ⇒ candidates=1`；`script=han chars=140 bytes=416 cap=400 ⇒ candidates=0` | 用同一字符数、两种脚本各跑一次（探针 `caps` 已含）⇒ 今天一个留一个丢；若认为不应如此，需把该上限改成按字符（或按脚本换算），并同步两个字段名的语义 |
| **F9** | low | **纯度扫描的文件清单是手写常量**：`SOURCES` 固定 5 项，而"本 crate 有哪些文件"由 `lib.rs` 的模块声明决定；将来新增 `src/x.rs` + `pub mod x;`，扫描不再覆盖且**不会报错** | `tests/bounded.rs:14-20`、`lib.rs:114-117`、`lib.rs:24-27`（"scans every source file"） | 隔离副本负控：加 `src/io_probe.rs`（真 I/O）+ `pub mod io_probe;` ⇒ `scan_verdict_clean=true`（**绿**）而 `unscanned_source_files=1|io_probe.rs(std::fs)`；副本可编译 | 在**隔离副本**里重复上面的两步（同一命令见 §7）：今天绿；把扫描改成枚举 `src/*.rs`（或断言数量相等）后，同一变异必须**变红** |

---

## 5 §4 未验证猜想（**不混进 findings**）

* **G1（半句话变逐字证据）**：F5 的生切原则上让窗口末尾出现"半句话"，而关系/实体把所在句逐字写进 `fact`/`summary` ⇒ 理论上可能出现"半句当逐字事实"。**我没能造出来**：200,000 B 单段落输入里，碎片没有变成候选（实体规则的 df≥2 / Han df≥3 闸门与反引号配对挡住了它）。**下一步的精确配方**（谁想接着打就用这个）：让切点落在**成对定界符的中间**且让碎片仍满足某条规则的闸门——例如把 64 KiB 切点对准一个 `[[wiki]]` 链接的**闭合括号之后**、或对准一个 inline-code span 的**闭反引号之后**，同时让该 token 在前文已出现 ≥2 次（`proper_noun_phrase` 的 df≥2 很容易满足）⇒ 若如此能抽出**带半句 `summary`/`fact` 的候选**，则 F5 应从 low 升为 medium/high（因为它把"半句"写成了"逐字证据"）。
* **G2（NaN 静默清空）**：`min_score = f32::NAN` ⇒ `score >= NaN` 恒假 ⇒ 全部丢弃而 `truncated` 仍 false。**我没把它列成 finding**：设计把该选项校验为 finite 且 0.0..=1.0（设计 `:316`），daemon 产不出 NaN。若将来有调用方绕过校验，这条就会变成 F2 的同款（静默清空）。
* **G3（`MAX_DISTINCT_TERMS` 的取舍方向）**：`graph.rs:335` 在表满 4096 时停止收新词——我**没有**取到"第 4097 个词被丢"的读数，也没有确认它是否抬 `truncated`（代码读起来**不抬**，但我没测）。若确实不抬，它是 F2 的一个同款（静默丢弃）。
* **G4（`turns_skipped` 的语义面）**：它数的是**过滤后**（`turn_for_extraction` 之后）被窗口切掉的条数，而 daemon 注释（`extract_plane.rs:481-487`）也是这个口径 ⇒ 我**不认为**是缺陷，但"被平台提示词过滤掉的轮次"不出现在任何计数里，值得下游确认这不是有意的省略。
* **G5（探针自身缺陷）**：我的计数分配器在 `realloc` 路径记账有误，perf 表 20 行里 6 行的 `peak_bytes` 读到 `usize::MAX`。这些行的**内存**数字作废（ms 可用）；§2.3 只引用了读数为**精确 16×** 的两行。要拿到全量内存读数需要一个正确的分配器或 Peak RSS。

---

## 6 §5 未覆盖范围（明确写没审什么）

* **`max_docs_per_pass` 的边界读数**：它在 daemon 的摄取扫描里（`capability.rs:434`、`api.rs:5184`、设计 `:1730-1733`，设计自称的用例在 `:2048`：30 份文档 + 上限 20 ⇒ 第一轮 20、第二轮 10）。**要跑 daemon 摄取测试/起 daemon 才能取**，超出本单范围 ⇒ **未测**。
* **`weight`**：召回腿融合权重，与抽取路径无关 ⇒ 未审（只给归属坐标）。
* **daemon 侧抽取缝的其余行为**：并发（同一 session 同时两次抽取）、`ExtractBundle` 的合并（`extract_plane.rs:212-221`）、错误路径（`NoSourceEnabled`）、ACP 臂的 prompt/parse —— 只审了与六项声称直接相关的部分（默认路由、`Rules` 臂透传、`turn_for_extraction` 无字节裁剪）。
* **`requirement-markers.rs`（285 行 / 12 个用例）**：我只确认它**全绿**，**没有**逐条审它的 marker 精度（`contains_requirement` 的否定/静态/引用守卫只在 `memory.rs` 的注释里读过）。
* **`rules.rs` / `text.rs` 的其余内部**：标记表内容、`STOP_*` 词表、`norm`/`base_name`/`variants_of`/`iso_datetime` 的**语义正确性**（我只审了它们的确定性/边界/多字节安全性）⇒ 一个"标记精度"审计是另一份工作。
* **`src/text.rs` 的 6 个 lib 单元测试**与 `src/graph.rs` 内部未读段（我只读了 1-243 与关键调用点）：**未逐行审**。
* **图形写入侧 `ruagent_graph::apply_extraction`** 的身份/合并语义：只扫到 `source_episode`/`event_time_source` 两个字段 ⇒ **未审**。
* **e2e / HTTP 路由 / capability HTTP 面（`GET/PUT /api/v1/capabilities`、`POST /api/v1/knowledge/graph/ingest`）/ policy.toml 解析**：未审。
* **性能基准**：没有与基线对比的基准（只有我这次的 1/4/16 MB 相对读数）；没有 Peak RSS；没有多核/并发读数。
* **跨平台**：全部读数来自 Windows（本机）；未验证 Linux/macOS 上的行为。
* **fuzz**：没有做随机/覆盖引导的 fuzz —— `bounded.rs:399-458` 自己声明"是手工表，不是 fuzz 语料"，我同意这个自我描述，因此"任意文本不 panic"**未被本单证明**。

---

## 7 复现命令（照抄可用）

```powershell
# 0) 判对象的 pins（先钉字节，再谈读数）
cd C:\Users\19410\Documents\ai\ruagent
Get-ChildItem crates\extract\src\*.rs | ForEach-Object { "$($_.Name) $((Get-FileHash $_.FullName -Algorithm SHA256).Hash)" }
# 期望（本报告窗口）：graph D0FB53F0…  lib 130AEABE…  memory 77A579AD…  rules BAB5489C…  text F2CB5676…

# 1) 仓库入口的测试读数（走团队包装器，不要在共享树上设 CARGO_TARGET_DIR）
$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"; $env:NO_PROXY='127.0.0.1,localhost,::1'
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-extract
# ⇒ 37 passed / 0 failed / exit 0（6 lib + 8 bounded + 8 graph-gold + 3 memory-gold + 12 requirement-markers）

# 2) 攻击探针：逐字节复制 + rustc 直编（不碰仓库 target / 团队锁 / 共享树）
$p="$env:TEMP\t125-probe"; Remove-Item -Recurse -Force $p -EA SilentlyContinue
New-Item -ItemType Directory -Force "$p\src" | Out-Null; Copy-Item crates\extract\src\*.rs "$p\src\"
rustc --edition 2024 --crate-name ruagent_extract --crate-type lib "$p\src\lib.rs" --out-dir "$p\lib" -C opt-level=2
rustc --edition 2024 "$p\probe.rs" --extern ruagent_extract="$p\lib\libruagent_extract.rlib" -o "$p\probe.exe" -C opt-level=2
# probe.rs 见本报告附录（要点：计数分配器 + 公开入口直接调用），随后:
& "$p\probe.exe" caps      # 上限边界（§2.2）
& "$p\probe.exe" det       # 确定性与顺序（§2.5，跑两次比对 hash 即跨进程）
& "$p\probe.exe" perf      # 病态输入时间/分配（§2.3）
& "$p\probe.exe" evidence  # 候选字段与 offset（§2.6）
& "$p\probe.exe" gold      # gold 断言能否被骗（§2.4）
& "$p\probe.exe" purity "$p\src"   # 纯度扫描（§2.9）

# 3) F9 的隔离负控（只动副本；窗口自我宣告）
Set-Content "$p\src\io_probe.rs" "use std::fs;`npub fn leak() -> usize { std::fs::metadata(`".`").map(|m| m.len() as usize).unwrap_or(0) }" -Encoding utf8
Add-Content "$p\src\lib.rs" "`npub mod io_probe;"
& "$p\probe.exe" purity "$p\src"    # ⇒ scan_verdict_clean=true 且 unscanned_source_files=1|io_probe.rs(std::fs)
rustc --edition 2024 --crate-name ruagent_extract --crate-type lib "$p\src\lib.rs" --out-dir "$p\lib_mut"   # ⇒ 能编译（I/O 可达是真的）

# 4) 收尾：临时目录与进程自查
Remove-Item -Recurse -Force $p
Get-Process ruagent -EA SilentlyContinue | Select-Object Id   # 本单没起过 daemon
```

**为什么探针可以不走 `cargo-team.ps1`**：包装器管的是**仓库工作区的构建**（单飞锁 + 共享 target）。我的探针**不构建工作区**：用 rustc 直接编译 `%TEMP%` 里逐字节复制的五个文件（crate 零依赖 ⇒ 不需要任何 extern），产物、rlib 全在 `%TEMP%\t125-probe`，**不碰 `D:\rust_cache` / `%TEMP%\ruagent-team-target`，也不占团队锁**。唯一与团队构建的交集是第 1 步那次 `cargo test -p ruagent-extract`，它**走了包装器并排队等锁**（日志可见 `waiting 10s` × 11，拿到锁后 2.0s 完成）。隔离副本的变异窗口：**2026-10-04 00:16:22–00:16:22**。

---

## 8 纪律回执

* **只读**：写入集合 = 本报告。`crates/**`、`panel/**`、`.github/**`、`scripts/**` **未改**（`git status --porcelain -- crates/extract` 为空；五文件 SHA-256 与开工相同）。
* **构建路径**：所有仓库构建走 `scripts/cargo-team.ps1`（`test -p ruagent-extract`，两次都成功且**在锁上排队**，未绕锁、未设 `CARGO_TARGET_DIR`、未用 `-TargetDir`）；探针构建用 rustc 直编隔离副本（§7 说明），**不参与工作区构建**。
* **活环境**：本单**没有起过任何 daemon**（读的是源码 + 副本探针），因此 `127.0.0.1:8787` 与活库 `~/.ruagent` **完全没有被触碰**；没有临时端口、没有临时 root 需要清。**一处必须纠正的环境事实**：任务单上写的活 daemon pid **79984 在我开工时已经不存在**；8787 的持有者是 **pid 14944（ruagent，启动于 2026-10-02 23:05:45，即我这次窗口之前）**，收尾读数 `listening=True`。也就是说那台 daemon 在我审计期间被**别人**重启过（换过 pid）——**不是我做的**（我没有 `Stop-Process`/`Start-Process` 任何 ruagent 进程），但它意味着"79984"这个编号从此刻起不能再当作活 daemon 的判据。
* **变异测试**：只在 `%TEMP%\t125-probe\src`（隔离副本）里做，窗口 00:16:22–00:16:22，**共享树未参与**（符合 t93/t81 条款：共享树不会因为我变红）。
* **`my_temp_leftovers`（收尾）**：`%TEMP%\t125-probe`（含 src 副本、rlib、probe.exe）**已删除**；我起的 `ruagent` 进程 **0**；我在共享树上产生的其他残留 **0**（探针只读仓库、只写 `%TEMP%`）。
* **我是不是自审**：不是。`crates/extract` 不是我的产物（另一会话的 capability 一代代码），本单我没有改它一行；报告只给声称表、读数、findings 与未覆盖范围。
* **一处需要 captain 知道的邻近风险**：侦察期间 HEAD 从 `873bb90` → `db46f6c` → `e8aaead` 连移三次（同伴在提交），所以我把读数钉在**文件 SHA-256** 上（收尾复查五个 pin 仍逐字节相同）；若此后有人改这五个文件，本报告的所有读数按 pin 失效（符合"绿灯只对它跑过的字节负责"的纪律）。
