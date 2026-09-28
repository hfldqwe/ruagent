# gen2 记忆第二轮修缮（t31 / repair round 2，mem-core）

- 任务：t31 —— 按 `docs/design/reviews/gen2-memory-spec.md` 修缮记忆生命周期（去重/合并判据、巩固审计、衰减时钟、审计可读性），并交付两处跨区接口的落地说明
- attempt：`e4e69ce4-531e-4430-ae6d-f89fdbe54eb4`（round 2）
- 本单可写面（stored inScope）：`crates/memory/` · `crates/daemon/src/memembed.rs` · `docs/design/reviews/gen2-memory-spec.md` · **本报告**
- 未改动（outOfScope 实测为空）：`crates/store/` · `crates/knowledge/` · `crates/graph/` · `crates/daemon/src/distill.rs` · `crates/daemon/src/api.rs` · `crates/mock-agent/tests/e2e_daemon.rs` · `panel/`
- 环境：所有 cargo 走 `scripts/cargo-team.ps1`（单一飞行锁 + 团队共享 target）；真 daemon pid 79984 未启停、`~/.ruagent` 只读（`mode=ro`）、未用 `GET /api/v1/recall` 取证

---

## 0 载体约定（本报告与规格里凡涉及 0019+ 新列的读数一律按此写）

**为什么单独立这一节**：t8 报告里有一句话的**载体写错了** —— 「live 库 163 行的 `access_count` 仍全为 NULL」。活库根本没有这一列。这不是措辞问题：同一句话在两种载体上一个读作「列不存在」、另一个读作「列存在且值为 NULL」，读者会据此做相反的决定。评审（RV-B）把这条列为 finding，本节即其落地。

| 载体 | 定义 | 判据（只读） |
| --- | --- | --- |
| **活库(18 版)** | 真 daemon pid 79984 正在用的 `~/.ruagent/data/ruagent.db`（`file:...?mode=ro`）。迁移只到 **18** ⇒ **0019–0025 一律未应用** | `SELECT MAX(version) FROM schema_migrations` = **18** |
| **迁移后副本** | 应用了 0019+ 的任何库（含测试用的 `Db::open_in_memory()`、未来的重启后活库） | `MAX(version)` ≥ 19；`pragma_table_info('memories')` 含新列 |

**读数（2026-09-28，只读 `mode=ro`，探针 `%TEMP%\ruagent-rb\carrier_probe.py`）**

| 读数 | 活库(18 版) | 迁移后副本 |
| --- | --- | --- |
| `schema_migrations` max | **18** | ≥19（t6 报告 §3：0019–0023） |
| `memories` 列数 | **14**；`access_count` / `last_used_at` / `valid_from` **不存在** | 17；三列存在，历史行为 NULL（`0020` 的三态规则） |
| `memory_sources` 表 | **不存在** | 存在（`0020`） |
| `query_eval_sets` / `chunks_fts_cjk` | **不存在** | 存在（`0019` / t6） |
| `memory_diffs` 的 op | 7 个：delete 2 · insert 181 · purge 23 · reject 15 · restore 1 · skip_dedupe 10 · supersede 5（共 237 行）**没有** `merge_judged`/`consolidate` | 新 op 只在**新代码跑过**的库上出现（本报告所有新 op 读数都来自内存库） |
| `memories` 行数 | 163 | 同（迁移不改行数） |

**t8 报告那句话的更正（原文保留，additive）**：应读作「**迁移后副本**上 163 行历史行的 `access_count` 为 NULL（NULL = 计数器存在前，不是 0 次使用）；**活库(18 版)** 上该列不存在」。本次我把这条更正写在本报告里，并**没有改动 t8 的终态报告正文**（它是终态记录；改由本报告的更正块承担）。

---

## 1 一句话

第二轮把五处「声称」变成「可读出的事实」：① 使用衰减的**时钟**（`Δt = now − COALESCE(last_used_at, updated_at)`，此前该列只写不读）；② 巩固**动作**落到 `memory_diffs`（`op='consolidate'`，此前全仓不存在）；③ 合并判据的**带标签语料**（20+20，可复跑，误并 0）；④ 审计记**实际决定者**（`anchor=`/`escalated_from=`/`over_tau=`/`polarity_refused=`）并顺带修掉两个真缺陷；⑤ 全报告读数统一写明**载体**、D.3 截断词表收敛为四个公开函数。

## 2 逐条验收：改前 → 改后（含产出该数的命令）

### 2.1 Δt = now − COALESCE(last_used_at, updated_at)（RV-B-1）

- **改前**：`usage::decay_score(base, updated_at, now, access_count)` —— `last_used_at` **不是入参**；生产调用点 `memembed.rs` 传 `&m.updated_at`。⇒ **两条内容相同、写时间相同、使用时间不同的记忆，入参逐字相同 ⇒ 出值必然相同**。评审 RV-B-1 实测 `used=0.500000 unused=0.500000`。
- **改后**：`decay_score(base, updated_at, last_used_at: Option<&str>, now, access_count)`；时钟 = `last_used_at`（可解析）→ `updated_at`（可解析）→「当作很旧」。新增 `pub fn decay_clock(updated_at, last_used_at) -> Option<DateTime<Utc>>`，探针可直接打印**这个分是从哪个时刻算出来的**。
- **调用点同窗口改**：`memembed::usage_of` 现在 `SELECT id, access_count, last_used_at`，`select_injection_memories` 传 `last_used.as_deref()`。
- **读数**（`cargo test -p ruagent-memory`）：
  - `READING C3(Δt) used=5,last_used=now -> 0.500000 | used=0,last_used=now-30d -> 0.058660` ⇒ 严格先后成立（两者 `updated_at` 相同，只有使用时钟不同）。
  - `READING C3 NULL row score=0.0587 vs zero-use row 0.0587`；`decay_clock` 的三种输入（`Some` / `None` 回落 / 乱码回落）逐条断言。
- **证据强度诚实标注**：改前侧是**编译级缺席**（旧签名没有这个参数，所以「同入参必同出值」无法在旧代码上运行期跑红），加上评审的实测值。与 t8 的 C3/C5 同类，不冒充运行期 before-red。

### 2.2 巩固动作落审计 `op='consolidate'`（RV-B-2）

- **改前**：`op='consolidate'` **全仓不存在**（评审原文）；`record_consolidation` 只写 `memory_sources` ⇒ 巩固留下出处，没有「发生过一次巩固」的记录。
- **改后**：只有本次**真的插入了来源行**（`n > 0`）才写一条 `memory_diffs`：`op='consolidate'`、`after='id=<memory_id>'`、`reason='consolidate key=<64hex> sources=<给定条数> inserted=<n>'`；key 复用既有 `consolidation_key(content, sources)`（单一来源）。载体事实（content/store/namespace）在同函数内读，**不**给公开结构体 `ConsolidationWrite` 加字段（那会连带改 out-of-scope 的 `lifecycle.rs`）。
- **读数**（`consolidate::tests::consolidation_links_are_stored_once_and_queryable_both_ways`）：
  - `READING C4 first=2 second=0`（`memory_sources`，既有行为不变）
  - `READING C4 audit after first call: [("consolidate", Some("consolidate key=854f473955ac6ee3214a2f145ee75eaf725075bc143c91ce797d8688fdb0172a sources=2 inserted=2"))]`
  - `READING C4 audit rows: first call=1 after a repeat=1` ⇒ **第一次 1 行 / 第二次 +0 行**，与 `memory_sources` 同幂等。

### 2.3 ≥20 正例 + ≥20 负例的可复跑夹具（RV-B-3，C2 target ③）

- **夹具**：`crates/memory/src/dedupe.rs` 测试模块里的 `POSITIVE_PAIRS`（20 条人工确认的同事实改写）与 `NEGATIVE_PAIRS`（20 条：极性 6 / 数值 8 / 主体与对象 6）。
- **复跑命令**：`cargo test -p ruagent-memory the_labelled_fixture_has_zero_false_merges_and_a_measured_recall -- --nocapture`（无模型、无网络、不碰活库）。
- **对象集说明（必须与读数同行）**：每对带一个 **assign 的余弦**（正例 0.93–0.99 / 负例 0.55–0.94，按活库地标取），因为单测里没有 embedder ⇒ 本夹具测**给定相似度证据下的决策规则**，不是 embedder，也不能替代 I-A 的活库 gold 集。候选集 = 「决定性那对」单元素；候选集召回另由 R-B A.9（cos≥0.95 的 K=3 覆盖 24/24）测。
- **读数**：
  - `READING C2 lexical-only: recall=0/20 false_merge=0/20` ⇒ 词法判据单独**一条改写都不认**，也**一条负例都不误并**。
  - `READING C2 decision@0.95: recall=14/20 (=0.70) false_merge=0/20 negative_escalations=0/20 below_tau=2 polarity_gated=4`（**原始读数行；引它必须连同下面那行连写**）
  - **误并 = 0/20**（充分满足判据）。
  - **召回 = 14/20 = 0.70** —— **恰好等于规格 C2 target ③ 的「≥ 0.7」门槛，没有任何余量**（**连同下面那行连写**）。缺口逐条具名：2 条被我把 assign 余弦压在 τ 之下（0.94/0.93，设计上就该丢），4 条被**极性门**拦下（`xlwt __VERSION__`、`禁止写 ~/.ruagent`、`不能被修改`、`禁止警告`）——它们的 diff 里出现极性词（多在改写那一侧），按门的规则**不可合并也不可升级判定**。这是**策略代价的读数**，不是未知。
  - **[captain 裁决 2026-09-28] 这一行必须**永远连写**（可以只引它，不许只引其中的数字）**：
    > **召回 14/20 = 0.70（恰好触线、无余量；夹具由实现者自造；6 条未命中 = 2 条我刻意置于 τ 之下 + 4 条极性门；从夹具自身取 τ ⇒ 3/20）**
  - **[captain 裁决 2026-09-28] 极性行交不交给具名判定：本代不可以，保持 fail-closed。** 极性门让 4 条正例不合并，方向正确（**误并比漏并更糟**）；把极性行转给具名判定 = 用「未落地的 LLM 收口」换一个召回数字，是把语义变更伪装成召回改进。具名判定正是下一代 LLM 收口的落点，届时连同极性行一起评。
  - **[裁决前的问句，保留为记录]** 我原先请评审裁决「触线是否算达标」；captain 已裁：**按写下来的判据算达标，但不做临时加码（加码就是改判据），要求证据强度随行**（即上面那行连写）。**下一代第一优先**：一张**独立**的（先冻结、再由别人使用）≥20 正 / ≥20 负语料 —— 与 C3 需要人工分级 gold 属同一类数据前置。
  - `READING C2 decision@0.99: recall=3/20`：把 τ 取自**这个夹具自己**的 40 个 assign 余弦的 p99 ⇒ 断言 `recall_at_fixture_tau < recall`。**从你想合并的那批对里取阈值是循环的**，必须取 scope 的背景分布（规格 D.2 原话）。

### 2.4 `candidate_source` 写进决策/审计形状（一处字符串）

选「写进形状」（不选「在规格 D 节登记命名偏差」），因此来源只有一处陈述：`dedupe::candidate_source(cosines_available) -> "embedding" | "none"`（闭集两值），以**规格自己的键名** `candidate_source=` 进审计串。
读数：`candidates=3 candidate_source=embedding anchor=11 escalated_from=none over_tau=none polarity_refused=none rule=lexical_ignorable verdict=merge`（判定形状）与 `candidates=3 candidate_source=none …`（无向量）各一条断言。
规格侧同步：`gen2-memory-spec.md` 的 **E.2 追加块**把第 ⑤ 条的审计串扩写为完整键值序列（旧键一个没丢），并写明 `MergeVerdict` 形状未变、`merge_decision` 签名未变 —— 两处说的是同一件事，没有第二套说法。

### 2.5 审计必须记**实际决定者**，不是余弦最高的邻居（RV-B-low-②）

- **改前**：`NeedsJudgement` 的 `candidate` = 被拒候选里**余弦最高**的那条；审计串只有 `candidates=/rule=/verdict=` ⇒ 读者分不清「规则与相似度一致」和「相似度把决策拉到了邻居身上」。
- **改后**（一个写入者 `merge_audit_reason`，四个键）：
  - `anchor=` = 判定**所依据**的那条。多行达 τ 时选**文本最接近**的（`text_overlap` = 归一化 LCS 长度，公开可复现），tie-break 依次余弦、id —— 不再盲目跟余弦最高（活库 45.19% 的配对在 cos ≥ 0.86，余弦单靠自己分不出改写与「另一件事的邻居」）。
  - `escalated_from=` = **余弦更高但决策没跟**的那条（噪声邻居签名）。
  - `over_tau=` = **所有**被拒且达 τ 的行（≥τ 的行不许藏）。
  - `polarity_refused=` = 被极性门拒绝的行。
- **读数（噪声邻居排在最前）**：`overlap(noise)=5 overlap(paraphrase)=16` ⇒ `NeedsJudgement{candidate:9, cosine:0.97}`，`candidates=2 candidate_source=embedding anchor=9 escalated_from=7 over_tau=7,9 polarity_refused=none rule=embedding_scope verdict=needs_judgement` ⇒ **决策跟同事实那条（9），并明确点名它没跟的高余弦邻居（7）**。
- **顺带查出并同窗口修掉两个真缺陷（各有测试）**：
  1. **一个被拒邻居不得替其它行做决定**：旧循环在**第一个极性拒绝**处 `return`，于是一条 diff 里带极性词的邻居能让整条决策变成 `Refused`，连词法上可合并的另一行一起丢。修法：极性拒绝**记录并继续扫描**。读数（新测试 `a_polarity_refused_neighbour_does_not_block_a_merge_with_another_row`）：`Merge{candidate:6, rule="lexical_ignorable"} | … polarity_refused=4`（旧代码此处是 `Refused`）。**诚实标边界**：该修法**不提高**夹具召回（14/20 前后一致，那 4 条是为自己那一对而被拒的），它改的是**跨行**行为。
  2. **极性拒绝必须与其它结果并列记录**：`Refused(POLARITY_REFUSAL)` 仍是「没有更好结果时」的终态；`over_tau` 里永不含极性行（不可升级）。读数：`polarity at cosine 0.99: Refused(...) | … polarity_refused=7 …`。

### 2.6 D.3 词表：验证/评审脚本的判据来源（owner：后续验证单 V-INT 起）

- 词表收敛成**四个公开函数**（探针直接调，不扫自然语言关键字）：`inject::tail_truncated(n)`、`inject::cut_at(what, bound)`、`inject::items_dropped_notice(n)`、`inject::items_dropped_minimal(n)`；后两个**由 `render_context` 自己调用**（改前是两处内联字面量）⇒ 探针问到的字节与渲染器发出的字节不可能漂移。
- 契约测试 `the_visible_truncation_vocabulary_is_the_only_source_a_probe_needs` 读数：
  `"… [+7 chars truncated]"` · `"… [upstream result truncated at 500 chars]"` · `"<context_budget>\n… [+3 items dropped: context budget reached]\n</context_budget>\n"` · `"… [+3 dropped]"`。
- 规则：验证/评审脚本**必须**从这四个函数取判据，禁用 `Select-String 'truncated'` 这类自然语言扫盘 —— 那样的脚本会在文案不变、**单位从字符变成字节**时照样发绿。**执行与验收归 V-INT**（不在本单 inScope）。

### 2.7 载体写法统一

见 §0：新增载体约定 + 判据 + 读数表，并更正 t8 那句写错载体的原话（更正写在本报告，不改动 t8 终态正文）。本报告所有新 op / 新列读数都标了载体。

### 2.8 RV-B-8 裁决 (a)：适配形状进冻结面

captain 裁 **(a) 接受适配形状**。落地：`gen2-memory-spec.md` 的 **D.5 追加块**（只追加、前文一字未动）写明 `EnrichedHit{hit, lead: Option<WikiLeadMeta>, relevance: Option<RelevanceMeta>}` + `plain()` + `order_value()` + `knowledge_items_enriched(hits, sources, wiki)`；旧 `knowledge_items()` 是薄包装；**逐字节冻结面**（`RetrievalHit` 字段语义 · `knowledge_items()` 输出 · `render_context` 字节 · `InjectionBudget` · D.3 四函数）；唯一适配器 `memembed::{lead_meta, relevance_meta, enriched_hit}`（含 `wiki::lead_for(kb, record, slug)` 的三态保真与 `RankedHit.relevance` 的来源）；调用点 `chat.rs`/`runs.rs` 归 t19，判据「未富集调用点 = 0」。**不悬空。**

### 2.9 交叉涟漪：wiki 的 RV-D-1 让契约的 `cite_coverage` 变三态（同窗口收敛）

- wiki 在 t34 把 `wiki::WikiLead.cite_coverage` 从「构造上只能 0.0/1.0」改成**构建期记录值 + 三态**（`Option<f32>`），并新增 `has_anchors` / `anchored_sections`；我的适配器因此编不过（她报的坐标）。
- **我的收敛**（都在 inScope）：`inject::WikiLeadMeta.cite_coverage: f32 → Option<f32>`，渲染 `coverage=unknown`（与 `edited=unknown` **同一词表**，键不省略）；适配器**原样透传**（`None` 不许填成 0.0/1.0）。
- **读数**：`edited=None coverage=None -> … coverage=unknown`，且断言 `coverage=0.00`/`coverage=1.00` **不出现**；只有 `Some(0.0)`/`Some(1.0)` 才渲染那两个值。适配器侧：`None -> None`、`Some(0.5) -> Some(0.5)`。
- **证据强度标注**：改前侧是**编译级缺席**（`f32` 表达不了 unknown），加上 RV-D-1 的缺陷读数（分母分子同源 ⇒ 只能 0/1；无 `wiki_pages` 行报满分）由 wiki 在 t34 给出。
- 另一个涟漪：graph 报的 `memembed.rs:529`（`merge_audit_reason` 入参类型不一致）—— 我 t31 的中途态，已同窗口改用 `merge_decision_audited` 并回告读数。
- 第三个涟漪：wiki 用 `-CleanFirst` 强制重查发现 `memembed.rs:342` 的 `type_complexity`（我用 t31 扩宽的嵌套元组带出来的）⇒ 抽出模块内 `type UsageRow = (i64, Option<i64>, Option<String>);`，`clippy -DenyWarnings -CleanFirst ruagent-daemon` 复绿。

## 3 本单的验收读数（逐字执行 stored verify）

| 命令 | 读数 |
| --- | --- |
| `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-memory` | **exit=0** · `59 passed; 0 failed; 0 ignored`（3.19s）+ doctest 0/0。t8 的 53 → +6 条新测试 |
| `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 clippy -p ruagent-memory --all-targets -DenyWarnings -CleanFirst ruagent-memory` | **exit=0** · 输出含本轮 `Checking ruagent-memory v0.1.0`（同锁窗口内 clean+gate ⇒ 真重查，不是缓存假绿），`Finished in 1.32s`，零 lint |
| `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 check -p ruagent-daemon --all-targets` | **exit=0** · 输出含 `Checking ruagent-daemon v0.1.0` ⇒ 真编译，**含 lib-test target**（graph 的 RVC-3/R-3 与 wiki 契约 verify 所在面），`Finished in 6.33s` |
| （额外，因我先前那条假绿）`… clippy -p ruagent-daemon --all-targets -DenyWarnings -CleanFirst ruagent-daemon` | **exit=0** · 全依赖 + `ruagent-daemon` 逐包 `Checking`，`Finished in 9.08s`，零 finding |
| `git status --porcelain -- crates/store crates/knowledge crates/graph crates/daemon/src/api.rs crates/daemon/src/wiki.rs crates/daemon/src/distill.rs panel` | **非空但没有一条是我的改动**（共享工作树里是同伴的在途窗口）。第二重可复算证据：全仓扫描 `MergeAudit\|merge_decision_audited\|candidate_source\|text_overlap\|items_dropped_notice\|items_dropped_minimal\|decay_clock\|polarity_refused` ⇒ **64 处命中全部在 `crates/memory/src/{inject,dedupe,usage}.rs` 与 `crates/daemon/src/memembed.rs`**，out-of-scope 文件 0 匹配 |

**教训（我方，已改）**：`clippy … -- -D warnings` 的尾参不进 cargo 指纹，只要先前有一次**不带 `-D`** 的 clippy 编译过同一批 target，带 `-D` 的那次就命中缓存、一条不重查、直接报成功 ⇒ 我先前给 daemon 面的 「clippy 绿」是**假绿**，是 wiki 用 `-CleanFirst` 才暴露出来的。此后 daemon 面 clippy 一律带 `-CleanFirst`。

## 4 本单改动的文件

`crates/memory/src/usage.rs`（Δt + `decay_clock` + 严格先后测试）· `crates/memory/src/consolidate.rs`（审计行 + 幂等读数）· `crates/memory/src/dedupe.rs`（`MergeAudit`/`anchor`/`escalated_from`/`over_tau`/`polarity_refused`、`candidate_source`、`text_overlap`、20+20 夹具、噪声邻居与极性邻居测试）· `crates/memory/src/inject.rs`（D.3 四函数 + `cite_coverage` 三态渲染）· `crates/daemon/src/memembed.rs`（Δt 调用点、`usage_of` 两列、`judge_merge` 用 `merge_decision_audited`、`UsageRow`、`cite_coverage` 透传）· `docs/design/reviews/gen2-memory-spec.md`（D.5 追加 + E.2 追加）· 本报告。

## 5 未做 / 不在本单（原因与 owner 都写明，无静默跳过）

- **C2 在 distill 真路径上的读数**：graph 的 R-3 接线（他重跑后贴 `op='merge_judged'` 0→N 的读数）。
- **`<graph>` 第三层注入**：无生产端构造点（归 t19 的构造窗口）。
- **知识/wiki 残余面的活库读数**：依赖未落地（I-A/I-D 的残余面）；活库(18 版)也没有那些表。
- **任何活库写入**：本单全程未写用户数据（只读 `mode=ro`），未启停真 daemon。

---

## 6 裁决与修订记录（captain，2026-09-28；本节按「终态记录只允许带日期修订」的形状追加）

**修订 1 —— 判据 ③ 的裁法与强制限定词。** captain 裁：**按写下来的判据算达标**（判据是「≥20 正 / ≥20 负 + 误并 / 召回」，≥0.70 就是 ≥0.70），**不做「触线不算」的临时加码**——加码即改判据，是本代最忌的形状。但证据强度必须随行，§2.3 的那一行从此**永远连写**：

> **召回 14/20 = 0.70（恰好触线、无余量；夹具由实现者自造；6 条未命中 = 2 条我刻意置于 τ 之下 + 4 条极性门；从夹具自身取 τ ⇒ 3/20）**

任何只引「0.70」而不带这些限定的地方，都是把弱证据当强证据用（与 F-4「recall@1 只在作者那张面上成立」同一族）。

**修订 2 —— 极性行不交给具名判定（本代）。** 保持 **fail-closed**：极性门让 4 条正例不合并，方向正确（误并比漏并更糟）；把极性行转给具名判定等于用「未落地的 LLM 收口」换一个召回数字，那是把语义变更伪装成召回改进。具名判定是**下一代 LLM 收口的落点**，届时连同极性行一起评。

**修订 3 —— 契约两处不一致时的动作顺序（流程规则，我方失误的复盘）。** 本单 stored `inScope` 指定的报告是 `gen2-memory-repair-r2.md`，而界面显示的是 `gen2-memory-impl.md`；我先按界面版把本轮内容写进了 t8 的**终态报告**，随后按 stored 为准恢复。captain 裁定：
- **以 stored 契约（amend 后的记录）为唯一权威**；界面那处会写进本代收尾的账。
- 「逐字还原 + 残留自检 0 命中」是正确的恢复动作；但**第一步就不该**把本轮内容写进终态报告 —— **终态报告只允许「带日期修订记录」的更正，不允许追加本轮内容**（graph 的 t9 报告同办）。
- **下次遇到契约两处不一致，先报 captain 一句再动手。**

**修订 4 —— 下一代第一优先（交接项）。** 一张**独立**的带标签语料：**先冻结、再由别人使用**的 ≥20 正例 / ≥20 负例，与 C3 需要的人工分级 gold 属同一类**数据前置**；本单的 20+20 由实现者自造，只能证明「决策规则在给定相似度证据下可复算」，不能替代它。

**修订 5 —— 我自己那次假绿的定性（保留为纪律证据）。** `clippy … -- -D warnings` 的尾参不进 cargo 指纹 ⇒ 命中缓存、一条不重查即报成功；这与 wiki 用 `-CleanFirst` 查到 `memembed.rs:342` 的 `type_complexity`（正是我用 `last_used_at` 扩宽的嵌套元组带出来的）**恰好构成一对**：纪律不是形式，它真抓到了我新写的缺陷。此后 daemon 面 clippy 一律带 `-CleanFirst`（已执行，见 §3）。
