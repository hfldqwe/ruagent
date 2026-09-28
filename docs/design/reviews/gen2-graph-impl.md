# gen2 知识图谱实现报告（I-C · t9）

> 交付物：`crates/graph/src/{lib,retrieve,community}.rs`、`crates/graph/tests/**`、`crates/daemon/src/distill.rs`、本文件。
> 规格：`docs/design/reviews/gen2-graph-spec.md`（R-C / t3）。store 侧由 I-SCHEMA / t6 落地（迁移 0019–0023 + `fts::han_bigrams`）。
> 环境：`$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"`；`$env:CARGO_TARGET_DIR=$env:TEMP\ruagent-ic`。真守护进程 pid **79984** 全程未动（未启停、未写 `~/.ruagent`、未用 `GET /api/v1/recall`）。
> 读数纪律：每条「改前」都由**旧代码路径在同一次测试运行里执行**得出（`search_entities` / `current_facts` / `upsert_entity` 的 `norm_name` 相等 / `add_fact` 都未改动），不是回忆出来的旧值。

## 0 一句话

C 节 10 条目标：**8 条达标**（G1 G2 G3 G4* G5 G6 G8* G9*）、**1 条部分达标并已登记**（G7：关系级 precision 0.75 < 0.80 目标）、**1 条 not_measured 且 owner 不在本单**（G10 注入）。带 * 的是「结构侧达标、需要在别处接线的读数仍为 not_measured」。
`cargo test -p ruagent-graph`：**29 passed / 0 failed / 3 ignored**（ignored = 需活库副本的 opt-in 读数，实测 3 passed）；`cargo clippy -p ruagent-graph --all-targets -- -D warnings`：**clean**；`cargo check -p ruagent-daemon --all-targets`：**exit 0**。全部经 `scripts/cargo-team.ps1`（见 §7 的写法）。

---

## 1 C 节逐条：改前 → 改后

### G1 实体腿不再恒空

| | 读数 | 命令 |
| --- | --- | --- |
| 改前 | 真实查询帧（`recall_log` 的 23 条不同查询，**采样面 = 用户真实查询**）：`strict` 非空 **1/23 = 4.3%**；生产加权 `entities>0` **5/651 = 0.77%** | 旧腿在同一测试里执行：`--test live-after` 的 `search_entities` 那一列 |
| 改后 | **13/23 = 56.5%** ✅ 目标 ≥13/23；生产端到端 **not_measured**（见 §4） | `$env:RUAGENT_GRAPH_LIVE_COPY=...;` 然后 `... test -p ruagent-graph --test live-after -Nocapture -- --ignored` |

冻结快照上的同一读数：`... test -p ruagent-graph --test seed-resolution -Nocapture` → `non-empty: old(strict) 1 -> new 13`（13/23，逐查询表打印；自查询对照 **63/63 → 63/63** 无回归）。

### G2 每个种子说得出是哪条腿

- **改前**：只有 strict / loose 两个黑箱；`match` 字段只存在于面板端点。
- **改后**：`SeedLeg::{ExactName, AliasTable, NameToken, SummaryFts, VectorNearest}`，`RetrievalStats.seeds_by_leg` **永远列出 5 条腿（含 0）**。
  真库全帧按腿计数（**t38 更正，见 §13.1**）：`{"exact_name": 1, "name_token": 8, "summary_fts": 46}`；~~`summary_fts: 39`~~ **标为不可引用**（同一份 23 查询全帧、同一份活库副本上，今天在三种形状下都读到 **46**，与 RV-C round 1 的独立读数一致）；`alias_table` 0（真图今天没有别名行）、`vector_nearest` 0（本 crate 未接嵌入器，**报告不出来**而不是悄悄消失）。
- 命令：`--test live-after`（`seeds by leg` 那一行）、`--test seed-resolution each_leg_reports_its_own_count_including_zero`。
- 抗噪证据：`zzqqxx-wvvvv-yyyyy` 这种非词查询 → `empty_reason = no_seed`（**不是**「图里没有这件事」）。

### G3 每条新边带来源（episode）

| | 读数 | 命令 |
| --- | --- | --- |
| 改前 | `entity_edges.source_episode` 非空 **0/67 = 0%**（对照 `memories` 162/163，且其中 156 条是 t347 回填的 legacy episode） | `python %TEMP%\graph-baseline.py`（t3 的 A2 段） |
| 改后 | `write_graph` 把本次蒸馏的 episode id 传给 `add_fact_with_source`：测试读数 **2/2 边都带 `source_episode = Some(1)`（本次 episode）**，且每条边带 16 位 `fact_hash` | `... test -p ruagent-daemon --lib distill -Nocapture` → `READING edges: [("runs_on", Some(1), Some("extracted"), Some("3b86510571381087")), ("uses", Some(1), Some("recorded"), Some("330893524acef598"))]` |

`record_episode` 失败时保持 **NULL**（mem-core D1(b) 的硬要求）：`add_fact_with_source` 的 `source_episode` 参数就是它拿到的值，没有任何哨兵/回填分支。活库历史行**不追溯**（§7.77 同族）。

### G4 时间语义（事件时间 ≠ 记录时间）

| | 读数 | 命令 |
| --- | --- | --- |
| 改前 | `abs(valid_at − created_at) ≤ 1s` 的边 **66/67 = 98.5%**；`event_time_source` 列不存在（迁移前）/ 全 NULL（迁移后不写）；`add_fact` 永远不写它 | `--test temporal the_old_writer_...`（旧写入器在同一测试里跑） |
| 改后 | 抽取侧 `add_fact_with_source`：有事件时间 → `extracted`，无 → `recorded`；`valid_at = None` **强制** `recorded`（`Extracted` 会是谎话）。检索侧 `TemporalStatus` 四态，测试打印：`extracted=Current`、`recorded=RecordedAtOnly`、**旧写入器(NULL)=RecordedAtOnly** | `... test -p ruagent-graph --test temporal -Nocapture` |

时点查询 gold（`tests/gold/as_of.json`，6 条人工查询，期望集由**独立实现** python BFS/窗口计算冻结）：**6/6 MATCH**。另外断言了「一跳返回的每条边都必须碰到某个种子」——这条不变量会抓住凭空多出来的跳。
目标里「新抽取的 `extracted` 比例 ≥30%」**not_measured**：需要一次真实重新蒸馏（本单没有 agent 调用），见 §4。

### G5 多跳检索（有界、可归因、带路径证据）

| | 读数 | 命令 |
| --- | --- | --- |
| 改前 | 召回只做 `current_facts(strict 命中)`＝0 跳扩展；对 gold 的 2/3 跳路径 **recall 0/20**；每查询 facts 中位 0 / 均值 2.22 | `--test multihop-gold gold_paths_recall_at_12`（`current_facts` 那一列） |
| 改后 | **path-recall@12 = 0.7500（默认 hops=2）**、0.8000（hops=3）✅ 目标 ≥0.60；facts/查询中位 **9** ✅ 目标 ≥3；20 条 gold 上 `truncated_by` = {Some(Hops) 6, Some(MaxPaths) 13, None 1}（**每次截断都报出来**）<br>~~0.7000 / 0.7500 · 中位 7 · {Hops 4, MaxPaths 15, None 1}~~ **标为不可引用（t38 更正，见 §13.2）** | `... test -p ruagent-graph --test multihop-gold -Nocapture` |

其余钉死的性质（`--test multihop-gold`）：
- 5 条 **distance ≥4** 的负例在 hops=3 下**必须找不到**（不能凭空发明路径）；
- 每条返回路径：`hops == edges.len() == nodes.len()-1`、无重复节点、`edges[i]` 真连接 `nodes[i]/nodes[i+1]`、**score == factors 的乘积**（读者可复算）；
- 预算硬上界：`max_paths=3, max_facts=2, beam=2` → 返回 2 条路径 / 2 条边 / `truncated_by = Beam`；
- 真库读数：`retrieve("ruagent")` 默认 → 12 路径 / 11 条不同事实 / `truncated_by=MaxPaths` / `frontier_sizes=[8,7]`（默认 hops=2 生效）。

### G6 消解与别名（判据先行）

| | 读数 | 命令 |
| --- | --- | --- |
| 改前 | 判据只有 `norm_name` 相等（`upsert_entity` 至今如此）：在 24 对人工 gold 上 **0/8 真对被合并**；真图里同一对象两行共存 | `--test resolution judge_scores_...`（`norm_name` 相等那一列） |
| 改后 | 24 对人工 gold：**precision 1.0000 / recall 1.0000**（TP=7 FP=0 FN=0 TN=16），**弃权 5 对**（进未决队列），1 对明确标注「只有声明的别名才能连」 | `... test -p ruagent-graph --test resolution -Nocapture` |
| 改后（真库） | 冗余对 **7 → 0**（合并 5 次）；`entity_edges` **67 → 67 不变**（合并搬边不丢边）；entities 63 → 58；新增 5 条别名 | `... test -p ruagent-graph --test live-after -Nocapture -- --ignored` |

**判据（最终版，写明在代码里）**：`variants(x)` = 去掉一个尾部括号后的主干 + 括号内内容；
`SAME` = 任一变体**相等**，或较短者 token 是较长者子集（两者都 ≥3 字符），或一方是另一方多词形式的**首字母缩写**（`ACP` ↔ `Agent Client Protocol`）；
`PENDING` = 用 **store 的同一个分词器**取首个 token 相同但不满足 SAME —— `dsh` / `dsh-kanban` 一族，交人裁决；
其余 `DIFFERENT`。
**判据变强带来的基线变化必须写明**：t3 规格里的基线是「4 对冗余」（当时用的是只按空白 token 的弱判据）；同一个图在最终判据下是 **7 对**。目标「0」不变，达标路径是 5 次合并。
未决队列：`resolution_pending`（`--test resolution upsert_with_aliases_merges_or_queues_never_guesses` 打印 `queued: #65 vs #37`）。

### G7 抽取质量 gold（**部分达标，已按 §7.138 上报，未自行放宽**）

命名对象集与人工 gold（`crates/graph/tests/gold/`，**不是**「看起来更好」）：
- `edges.json`：**全部 60 条当前边**，人工三值标注 `supported | mislabeled | duplicate`（判据写在文件里）；
- `ontology.json`：**全部 35 个关系名**，`controlled | ad_hoc`（判据：名字是否把一个**通用关系类型**说出来，还是把具体对象/用途焊进了名字）；
- `resolution.json`：**24 对实体**，`same` + `how(mechanical|declared)`。

| 指标 | 读数 | 目标 | 判定 |
| --- | --- | --- | --- |
| 关系级 precision（严格） | **0.7500**（45/60） | ≥0.80 | ❌ **未达标，已登记** |
| 端点级 precision（容忍重复） | **0.9333**（56/60） | — | — |
| 重复率 / 误标率 / 端点错（unsupported） | 0.1833（11） / 0.0667（4） / **0** | — | — |
| 本体一致性 | controlled **29/35 = 0.8286**，ad_hoc 6/35，单词表 24/35 | 非法 kind/relation = 0 | 词表外关系名仍在（见下） |
| 幻觉率 | **not_measured**（活库 0/67 边有 episode ⇒ 没有 transcript 可比对）；机制在 fixture 上被测：**发明的句子会被抓**，且**改写句也会被抓**（词法支撑分不清改写与发明——这个假阳性就是它不被当数字的原因） | ≤0.10 | not_measured |
| 关系 recall（活库） | **not_measured**：没有可追溯的 gold 语料（历史边无来源；要 gold 必须重跑一次真实蒸馏 + 标注） | ≥0.60 | not_measured |

差在哪（读数指出的具体原因，供下一轮）：**11 条重复**（同一事实以不同关系名/反方向各留一条，如 49/52/53 三条「dsh 驱动 deepseek-v4-flash」、14/62 与 15/63 各两条）+ **4 条误标**（`shares_kernel_with` 说的是「共享测试思路」不是共享内核；`built_on` 说的是「是 ACP 客户端」；`must_never_drop` 是约束不是关系；`planned_queryable_store` 说的是计划不是现状）+ **24/35 个关系名只出现一次**。
命令：`... test -p ruagent-graph --test extraction-gold -Nocapture`（`the_gold_object_sets_cover_every_row_they_claim_to` 保证：快照里新增任何一条边/关系名而没标注，测试就红）。

### G8 社区/主题摘要层（结构侧达标）

| | 读数 | 命令 |
| --- | --- | --- |
| 改前 | **0**（无表、无 API；23 个连通块、最大 35/63、孤立 20） | t3 的 A10 |
| 改后（真库） | **9 个社区，覆盖 43/43 = 100% 的非孤立实体** ✅ 目标 ≥90%；35 节点巨块被模块度切分（`split_by_modularity=1`）；层级 0 挂到层级 1 | `--test live-after -Nocapture -- --ignored` |
| 性质 | 重建幂等（两次 build 的划分逐成员相同）；同一层内实体不重复；build 不花任何 LLM 调用（summary 全 NULL），摘要可单条后写；未建过的层返回 **None**（不是「空」） | `--test extraction-gold a_level_is_unbuilt_until_it_is_built / the_partition_is_deterministic_... / a_summary_may_be_absent_...` |

**质量侧 not_measured**：`comprehensiveness / diversity / empowerment` 成对判决（≥60% 胜率）需要一次 LLM 判定跑批，本单不产生。owner：下一轮 I-C 或评测 harness owner；判据与协议已在 t3 的 B2 写好。

### G9 distill 三态可观测

| | 读数 | 命令 |
| --- | --- | --- |
| 改前 | 同窗口（`daemon.log` 09-26T04:52:45Z..09-27T13:48:48Z）**22 次尝试 / 库里 1 行**，21 次失败只在日志；`distill_log` 的 status/failure_reason/prompt_hash 三列由 t6 加了但**没人写** | t3 的 A4/A5 |
| 改后 | `distill()` 的**成功路径写 `ok` 或 `empty`**、**失败路径写 `failed` + `failure_reason` + `prompt_hash`**；失败行**不覆盖**已记录的 ok；失败**绝不创建 episode**（t350 的徽章按 `episodes.kind` 判）；三种状态在同一张表里并存 | `... test -p ruagent-daemon --lib distill -Nocapture` → `READING distill_log by status: [(Some("empty"), 1), (Some("failed"), 1), (Some("ok"), 1)]`；`READING after a failed attempt on a session that had succeeded: ("ok", None, 3)`；`READING a first-attempt failure: ("failed", Some("Query returned no rows"), Some("hash-b"))` → **13 passed / 0 failed** |

**目标「库行数 = 日志尝试数」已达**（原始状态是到不了：`distill_log` 的主键是 `session_key`，同一会话多次尝试只能留一行 ⇒ 差值恒为负）。I-SCHEMA-2/0024 重建为按次行 + 本单写入侧改成普通 INSERT 后，读数 `attempts=4 rows=4 recorded_outcomes=4 | rows - attempts = 0`（§10 R-2）。

### G10 图证据进注入（not_measured，owner 不在本单）

- **改前 0**；**改后 not_measured**：`<graph>` 块的产生点在 `crates/daemon/src/api.rs` / `runs.rs`（integ）与 tag 词表在 `crates/memory/src/inject.rs`（mem-core）。
- 我这侧已交付可消费的形状并钉死：`GraphEvidence::lines()` 每个路径**一行**内含 `hops / valid_at / state / edges(edge_id)/ source(session)`（`--test multihop-gold a_returned_path_says_how_it_got_there` 断言这些片段存在）。
- 位置已与 mem-core 约定（graph 排 knowledge 与 wiki 之间，rank 3）；`--test live-after` 打印的那行就是注入会拿到的原文。

---

## 2 冻结点与实现偏差

1. **D 节签名逐字实现**：`retrieve(&Db,&GraphQuery) -> Result<GraphEvidence,DbError>`、`resolve_seeds`、`communities`，以及 `SeedLeg / TemporalStatus / EdgeSource / EvidencePath / PathRationale / RetrievalStats / TruncationBudget / EmptyReason` 全部按规格的名字与类型落地。**加的只有 `Hash`**（`SeedLeg`，让调用方按腿索引计数）与 `CommunityBuild / communities_of / community_coverage`（E8 需要的读侧）。
2. **冻结的旧接口一字未动**：`search_entities`、`search_entities_loose`、`upsert_entity`、`add_fact`、`current_facts`、`facts_as_of`、`neighbors`、`list_entities`、`list_edges`、`delete_entity` 的函数体与签名保持原样（`git diff` 只新增）。`Edge`/`Entity` 的字段**没有**加也没有改——多跳需要的 `event_time_source`/`session_key` 走新的 `EvidenceEdge`，避免动面板 JSON。
3. **CJK 同源（DEP-5）**：本 crate **没有**任何 CJK 分词实现。用到的是 `ruagent_store::fts::{terms, match_any_prefix, like_patterns, han_bigrams, MIN_RECALL_ASCII}`；`entities_fts` 没有 bigram 列，所以 `match_bigrams` 在这里**用不上**（会造出一个永不匹配的短语），这一点写在 `retrieve.rs` 的文件头里。
   反查（实体名出现在查询里）用 `instr(lower(query), norm_name)` —— 不是分词，是子串判断，且沿用 store 的 `MIN_RECALL_ASCII` 下限。
4. **一个可选设计修正**：路径去重从「节点序列」改成「**边 id 序列**」。原因：同一对节点之间的两条不同关系是两条**不同证据**，按节点去重会静默丢一条（as-of gold 第一次跑就抓到了这个：期望 8 条边、只回 7 条）。这是 D 节未写明的实现细节，记在这里供 RV-C 判。
5. **`status` 的语义收口**：`empty` = 抽取回来三个数组都空；`ok` = 至少有一个非空（即使写库时全被去重跳过）。这与「被去重」区分开（D5），写进 `distill_once` 的注释。
6. **附带做了 mem-core 明确要求的一处记忆写入改动（C1）**：`distill.rs` 里 `m.confidence.unwrap_or(0.8).clamp(0.5, 1.0)` → `ruagent_memory::confidence::confidence(&signals)`（有值就走 `explicit`，无值 `unconfirmed`）。效果：抽取报的 `0.4` **原样入库**（测试读数 `confidence(用户可能偏好简体中文。) = 0.4`，`0.8` 走单源默认），`clamp` 曾让 `<0.5` 在任何生产路径上不可达（活库 0/157）。**没有**半接线 mem-core 的 `judge_merge`（见 §4 DEP-5），因为半接线会让今天能判的合并静默消失。
7. **D3 闸门 1 被测试钉住**：`the_memories_half_of_the_prompt_is_byte_identical` 断言 `memories[]` 那段 bullet 与 JSON 示例逐字节未动，同时断言新增字段与「valid_at 未知必须 null」那句在 prompt 里。改 prompt 的人不会悄悄改到记忆面。
6. **一句自我纠正**：我最初把「括号内的缩写」在 judge 里丢掉了（只留主干），gold 立刻抓到 recall 只有 0.5（漏 4/8）。修法是 `variants()` 保留括号内容 + 首字母缩写规则（recall 1.0）。**这是 gold 证明它自己有用的地方，不是事后补的解释。**
8. **交付后的两处收口（同一文件，不是新功能）**：① `log_outcome` 改成「一行一次尝试」（I-SCHEMA-2/0024 落地后，R-2 写入侧，见 §10 R-2）——原写法在迁移后会在 PREPARE 阶段报错，本单测试抓到了；② 测试里的 `Vec<(String, Option<i64>, Option<String>, Option<String>)>` 抽成 `type EdgeRow`（wiki/t10 报的 `clippy::type_complexity`，见 §10 R-2b）。两处都只动 `crates/daemon/src/distill.rs`，读数见 §10。

## 3 跨区接口与接线需求（不在我的 inScope，按纪律登记）

| # | 需要谁做什么 | 现状 | 依据 |
| --- | --- | --- | --- |
| DEP-1 | **integ / t19**：`api.rs:2524` 的召回实体腿从 `search_entities`（strict）换成 `resolve_seeds` + `retrieve`，并把 `graph_entities`/`graph_paths` 写进 `recall_log`（列已由 t6 加好） | 未接线 ⇒ **G1 的生产端到端读数 not_measured** | t3 A17；本报告 §1 G1 |
| DEP-2 | **mem-core**：`TAG_GRAPH` 常量 + `tag_rank` 排 3（knowledge 与 wiki 之间）。mem-core 2026-09-27 已回信同意（并在自己的 D 节登记为策略变更：wiki/project_context 各 +1） | 未落地 ⇒ **G10 not_measured** | 本报告 §1 G10；`crates/memory/src/inject.rs:261` |
| DEP-3 | **I-SCHEMA / recall**：`distill_log` 按**次**计数需新表或复合主键（原 PK=session_key）；`recall_log` 的 `graph_*` 两列已就位 | **已落地**（`0024_distill_attempts.sql`，owner=recall）；写入侧由本单收口（§10 R-2）⇒ G9 的「差值 0」已达成 | §7.138；本报告 §10 R-2 |
| DEP-4 | **integ**：面板/CLI 需要一个入口调 `merge_entities`（合并是决策，不该自动跑）与 `build_communities`（社区构建需要触发点） | 未落地 ⇒ G6 的「0」在真实运行中不会自己发生 | `crates/graph/src/lib.rs` 的 `merge_entities` 文档 |
| DEP-5 | **B 类接线（mem-core，本单已按他们要求做了一半）**：`mergeable_target` → `memembed::judge_merge` + 对 `MergeVerdict::NeedsJudgement` 用一次 LLM 判定收口；mem-core 说这条不落地时他们的 C2 判 not_measured | **未落地**（见 §4） | mem-core 2026-09-27 的来信 |

## 4 not_measured / 未达标（不静默，按 §7.138 上报）

| 项 | 状态 | 原因 / owner |
| --- | --- | --- |
| G1 生产端到端（recall 响应里的实体腿） | **not_measured** | 需要 DEP-1 接线；owner = INT/t19。我的读数是腿级（13/23） |
| G4 `extracted` 比例 ≥30%（活库） | **not_measured** | 需要一次真实重蒸馏（本单无 agent 调用）；抽取侧的写入语义已被单测钉死 |
| G7 关系级 precision ≥0.80 | **未达标（0.75）** | 11 重复 + 4 误标；修法是抽取 prompt/本体（我的 E7/D3 区）+ 写侧去重。**没有降低判据**，登记在此 |
| G7 关系 recall ≥0.60（活库） | **not_measured** | 无可追溯 gold 语料（0/67 边有 episode）；要 gold 必须重跑蒸馏 + 人工标注 |
| G7 幻觉率 ≤0.10 | **not_measured** | 同上；机制已实现并被 fixture 测到（含它分不清改写与发明的**已知假阳性**） |
| G8 三指标成对判决 ≥60% | **not_measured** | 需要 LLM 判定跑批；结构侧（覆盖 100%）已达标 |
| G9 「库行数 = 日志尝试数」 | **已达** | 原为到不了（`distill_log` PK = `session_key` ⇒ 同会话每次覆盖）；I-SCHEMA-2/0024 重建为按次行 + 本单写入侧改普通 INSERT ⇒ `rows - attempts = 0`（§10 R-2） |
| G10 `<graph>` 进注入 | **not_measured** | owner = INT/t19 + mem-core（DEP-2） |
| DEP-5（mem-core 要的 `judge_merge` 接线 + NeedsJudgement 的 LLM 收口） | **未落地** | 本单预算用尽；mem-core 已同意在未落地时把他们的 C2 判 not_measured。**没有半接线**（半接线会让今天能判的合并静默消失） |
| 实体向量种子腿（E10） | **未实现** | `entities.embedding/embedder` 列已由 t6 加好，但本 crate 没有嵌入器；`SeedLeg::VectorNearest` 永远为 0 且**出现在读数里** |
| `entity_sources` 表（E5 可选） | **不做** | t6 明确不建（无消费者）；「每条边有来源」由 `entity_edges.source_episode` 满足 |

## 5 反例：每条新判据在**旧代码**上失败（同一次运行里的旧腿）

| 判据 | 旧腿（未改动的旧函数） | 新腿 |
| --- | --- | --- |
| 真实查询帧非空 ≥13/23 | `search_entities`（strict）：**1/23** | `resolve_seeds`：**13/23** |
| gold 路径 recall ≥0.60 | `current_facts`（1 跳）：**0/20** | `retrieve`：**0.75**（hops=2；t38 更正，原写 0.70，见 §13.2） |
| 时点查询 6/6 | 旧路径没有 as-of 检索（`facts_as_of` 只有 0 跳、不返回路径）；`add_fact` 不写 `event_time_source` ⇒ 只能报 `RecordedAtOnly` | `retrieve(as_of=...)`：**6/6** |
| 消解 recall ≥0.70 | `norm_name` 相等：**0/8 真对** | judge：**7/7 mechanical 真对**（+1 对走别名表） |
| 真图冗余 0 | `norm_name` 相等判据下这 7 对全部共存 | 合并 5 次 → **0**，边数不变 |
| 注入可读 | 无 `GraphEvidence`（没有多跳证据可言） | `lines()` 一行含 hops/valid_at/state/edges/source |

## 6 仪器（会制造假读数，写下来给下一个人）

1. **复制 SQLite 文件必须连 WAL 一起**：`Copy-Item ruagent.db copy.db` 会丢掉 4.3 MB 的 `-wal`，副本的 `recall_log` 少到 **22 条不同查询**，于是 G1 读数变成 12/22（假的不达标）。正确工具是**只读连接上的 `VACUUM INTO`**（得到一致快照），之后读数是 **23 条 / 13 非空**。第一次跑出的 12 是我自己的探针错。
2. **临时目录不能只用时间戳**：`Utc::now()` 在 Windows 上分辨率约 0.5–15 ms，同一进程里两个测试会撞到同一个目录，第二个测试就报 `UNIQUE constraint failed: entities.id`。测试 fixture 改成进程内原子计数器。
3. **`#[ignore]` 的活库读数必须拒绝打开 `~/.ruagent`**：`live-after.rs` 在打开前断言路径里不含 `.ruagent`。
4. 本仓已有的两条仍然适用：验证要用独立 target dir（§7.139）；PowerShell 会把 ISO 时间串按本地偏移渲染（t4 的 A.3）——本单的时间比较全部在 SQL 字符串层面做，不经过 PowerShell 的 datetime。
5. **包装脚本要先 `-DryRun`**：全队改走 `scripts/cargo-team.ps1` 之后，它的第一个版本在 PowerShell 7 下把位置参数绑到了 `-Jobs`，于是我的调用**打印出 `cargo -CargoArgs System.Object[]` 并以 exit 101 结束**——看起来像「cargo 失败了」，实际上是包装脚本一个 cargo 参数都没传对。`-DryRun` 会打印将执行的**确切 cargo 命令行**，先看这一行再跑，就不会把「包装脚本没接线」读成「代码编不过」。
6. **一条被误杀的编译不是一条失败读数**：本单的一次 `test -p ruagent-daemon --lib distill` 进程被队友按「命令行含 cargo-team + 创建时刻」推断归属时误杀（当时它在**等锁**、没在编译）。结论写在这里：**等锁中的进程与正在编译的进程在外部看起来一样**，所以「进程消失 + 没有编译错误」既不是 pass 也不是 fail，只能重跑；判断编译结论只看 `test result:` / `exit=N` 这两行。

## 7 复现命令汇总

> 全队编译纪律（2026-09-27 captain）：**每一次 cargo 都走 `scripts/cargo-team.ps1`**（单一编译、共享 target、只占 CPU 0-11、BelowNormal）。等价写法：
> `-Nocapture` = `-- --nocapture`，`-DenyWarnings` = `-- -D warnings`，`-DryRun` 只打印命令行不取锁。

```powershell
# 全部 unit + integration (29 passed / 3 ignored)
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-graph

# 逐条目标的带读数输出
... -File scripts/cargo-team.ps1 test -p ruagent-graph --test seed-resolution -Nocapture   # G1 G2
... -File scripts/cargo-team.ps1 test -p ruagent-graph --test multihop-gold   -Nocapture   # G5
... -File scripts/cargo-team.ps1 test -p ruagent-graph --test temporal        -Nocapture   # G4
... -File scripts/cargo-team.ps1 test -p ruagent-graph --test resolution      -Nocapture   # G6
... -File scripts/cargo-team.ps1 test -p ruagent-graph --test extraction-gold -Nocapture   # G7 G8(结构)
... -File scripts/cargo-team.ps1 test -p ruagent-daemon --lib distill        -Nocapture   # G3 G4(写入侧) G9 + C1

# 活库「改后」读数（副本；从不打开 ~/.ruagent）
python -c "import sqlite3,os;con=sqlite3.connect('file:'+os.path.expanduser('~/.ruagent/data/ruagent.db').replace('\\','/')+'?mode=ro',uri=True);con.execute('VACUUM INTO ?',(os.path.join(os.environ['TEMP'],'ruagent-t9-live.db'),))"
$env:RUAGENT_GRAPH_LIVE_COPY="$env:TEMP\ruagent-t9-live.db"
... -File scripts/cargo-team.ps1 test -p ruagent-graph --test live-after -Nocapture -- --ignored --test-threads=1

# 门禁
... -File scripts/cargo-team.ps1 clippy -p ruagent-graph --all-targets -DenyWarnings
... -File scripts/cargo-team.ps1 clippy -p ruagent-daemon --all-targets -DenyWarnings   # 动过 distill.rs 就必须跑这条（见 §10 R-2b）
git status --porcelain -- crates/store crates/knowledge crates/memory panel
```

## 8 改动清单

新增：`crates/graph/src/retrieve.rs`（种子四条腿 + 有界多跳 + 证据形状）、`crates/graph/src/community.rs`（确定性划分 + 延迟摘要）、`crates/graph/tests/{seed-resolution,multihop-gold,resolution,temporal,extraction-gold,live-after,fixture}.rs`、`crates/graph/tests/gold/{live_graph_snapshot,multihop,resolution,ontology,edges,as_of}.json`。
修改：`crates/graph/src/lib.rs`（新写入器/判据/别名/合并，**旧接口一字未动**）、`crates/graph/Cargo.toml`（dev-dep `serde_json`，测试读 gold 用）、`crates/daemon/src/distill.rs`（D1–D5 + C1 的 confidence 单源 + 两处测试解构 + R-2 的 `log_outcome` 单行 INSERT + R-2b 的 `type EdgeRow`）。
### 8.1 派生文件申报（manifest 带出的 lock 变化，按 captain 2026-09-27 的规则：声明即可、不算越界）

`Cargo.lock` **不在任何人 inScope**，因此没有列入 `changedPaths`，但必须申报它由哪个 manifest 的哪一行带出、新依赖的用途、以及它是否已在 workspace 依赖里：

- 带出它的 manifest 行：`crates/graph/Cargo.toml` 的 `[dev-dependencies]` 新增 `serde_json = "1"`。
- 用途：`crates/graph/tests/**` 读 `tests/gold/*.json`（人工 gold 的判据与标注）用的解析器；**只用于 test target，不进 library 依赖**。
- 是否已在 workspace 依赖里：**在**（`serde_json` 早已是本 workspace 的依赖，锁文件里已有该包与版本），因此这次**没有新增任何第三方 crate**，只是把 graph 的依赖边加上。
- 一行 diff（`git diff -- Cargo.lock`，全文件只有这 2 行，其中只有第 1 行是本单的；第 2 行属队友在途改动）：
  `+ "serde_json",` —— 出现在 `[[package]] name = "ruagent-graph"` 的 `dependencies` 列表里（另一行同形状的 `+ "serde_json",` 在不同包名下，非本单产物）。

## 9 门禁读数（最后一次，全部走 `scripts/cargo-team.ps1`；含交付后收口的两处复测）

| 命令 | 结果 |
| --- | --- |
| `... test -p ruagent-graph` | **exit 0**：lib 3 + empty-recall-pattern 1 + entity-query-shapes 3 + extraction-gold 6 + multihop-gold 5 + resolution 3 + seed-resolution 5 + temporal 3 = **29 passed / 0 failed**；`live-after` **3 ignored**（需活库副本；带 `RUAGENT_GRAPH_LIVE_COPY` 跑过后 **3 passed**） |
| `... test -p ruagent-daemon --lib distill -Nocapture` | **exit 0**：**13 passed / 0 failed**（含本单新增的 4 条：provenance / 三态 / confidence / prompt 闸门） |
| `... clippy -p ruagent-graph --all-targets -DenyWarnings` | **exit 0**，clean |
| `cargo check -p ruagent-daemon --all-targets` | **exit 0**（早先 mem-core 报的两处测试解构错误已修；那条 `Knowledge` 的编译错误是队友在途状态，现在已绿） |
| `git status --porcelain -- crates/store crates/knowledge crates/memory panel` | **输出非空，但列出的每一条都是队友的在途改动**（t6 的 `crates/store/src/migrations/0019-0023`、recall 的 `crates/knowledge/tests/**`、mem-core 的 `crates/memory/**`）。本单**没有**改这些路径：我的改动集 = `crates/graph/**` + `crates/daemon/src/distill.rs` + 本报告（`git status --porcelain -- crates/graph crates/daemon/src/distill.rs` 只列我的文件）。按纪律：这是**第三方在途状态**，不是本单的越界，我也没有替他们改任何一个字节 |


## 10 修复清单（repair 单的边界，按 captain 2026-09-27 的裁决立）

### R-1 G7 关系级 precision 0.75 → ≥0.80（同一批文件，我的 inScope）
三个具体失败模式，每个都有读数与本单已落地的判据：
| 子项 | 读数 | 手段（我这边） |
| --- | --- | --- |
| R-1a 同事实重复边 11 条（不同关系名或反方向） | `edges.json` 的 11 条 duplicate：49/52/53、14/62、15/63、16/38、50/54/56、49/55 | 写侧以 `fact_hash`（本单已写）+ `(src,dst)` 归一化身份去重；对已失效/并行边做「同事实」判定（不能只按 relation 相等） |
| R-1b 误标 4 条 | 3 `shares_kernel_with`、12 `built_on`、45 `must_never_drop`、47 `planned_queryable_store` | 抽取 prompt 里把「关系名必须陈述事实文本所说的关系」写成约束；`must_never_drop`/`planned_queryable_store` 这类**不是关系**的名进禁用表 |
| R-1c 本体覆盖不足 **24/35 单例**（**全部 67 条边**的口径）+ 6/35 ad_hoc | `ontology.json`（**60 条 current 边**的口径；同口径下是 **26/35**：多出 `owns`、`runs_on`） | 关系词表收窄（受控集合 + 允许 `snake_case` 组合但必须由词表元素拼出），并把非法关系名从「接受」改成「拒绝/归一」 |
判据不放宽：目标仍是关系级 precision ≥0.80（30 条相关指标里低于 0.80 的那条必须变绿），gold 文件不改，只改被测代码。

### R-2 G9 写入侧（**已落地**：I-SCHEMA-2 先落地，写入侧由本单收口）
原状：`distill_log` 的 PK = `session_key`（`0008_memory_embeddings.sql:9`）⇒ 同一会话的多次尝试只能留一行，「库行数 = 日志尝试数」到不了。
I-SCHEMA-2（owner=recall，`crates/store/src/migrations/0024_distill_attempts.sql`）把该表重建为「一行一次尝试」（`id INTEGER PRIMARY KEY`，`session_key` 降为普通列 + 索引 `(session_key, id)`，视图 `distill_recorded_outcomes` 只取有结果的行），并**点名**本写入侧必须改：保留 `ON CONFLICT(session_key)` 的写入者在 **PREPARE 阶段**就报 `ON CONFLICT clause does not match any PRIMARY KEY or UNIQUE constraint`。本单实测就是这条错误（`the_log_records_three_states_and_a_failure_keeps_a_success` 变红），不是推断。
本单已改（`crates/daemon/src/distill.rs` 的 `log_outcome`）：
1. `ON CONFLICT(session_key) DO UPDATE … WHERE excluded.status <> 'failed' OR …` **整段换成普通 INSERT** —— 每次尝试一行；
2. `status ∈ {ok, empty, failed}` + `failure_reason` + `prompt_hash` 每次尝试都写；
3. 「失败不覆盖成功」从 SQL 的 WHERE 条件**升级为行级事实**（成功行仍在旁边）；
4. 失败**绝不**创建 episode（断言：先建 1 条 episode 再制造失败，episode 数不变）；
5. 读数（`... test -p ruagent-daemon --lib distill -Nocapture`，13 passed / 0 failed）：
   - `READING R-2: attempts=4 rows=4 recorded_outcomes=4 | rows - attempts = 0` ✅ **「库行数 − 日志尝试数 = 0」达成**（改前 4 次尝试只留 1 行）
   - `READING the succeeded attempt after a later failure: ("ok", None, 3)`、`READING the newest attempt of that session: failed`（成功行未被抹掉；最新尝试也能单独读到）
   - `READING distill_log by status: [(empty,1),(failed,2),(ok,1)]`（三态并存且计数 = 尝试数）
recall 给的两个读法已写进 `log_outcome` 的文档注释，供后续读者不再猜：某会话试过几次 = `SELECT * FROM distill_log WHERE session_key=? ORDER BY id`；失败率 = `SELECT status, COUNT(*) FROM distill_recorded_outcomes GROUP BY 1`（分母**必须**是有结果的行；`COUNT(*) FROM distill_log` 会把 33 行历史 NULL 算进分母 —— 那正是 0020 拒绝 `DEFAULT 'ok'` 的同一个错误）。
**跨区发现（不是我的文件，只报告不改）**：`crates/mock-agent/tests/e2e_daemon.rs:2182`（`distill_graph_toggle_controls_entity_extraction`）用 `SELECT entities_written FROM distill_log WHERE session_key = ?1` + `query_row` 读，多行时取**最旧**一行；该会话只蒸馏一次时断言仍成立，但语义建议改成「该会话至少一次尝试 / 取最新一行」（recall 已提，owner = mock-agent 侧）。

### R-2b `clippy::type_complexity`（wiki/t10 报的阻塞项，已修）
`crates/daemon/src/distill.rs` 的 lib-test target 里 `Vec<(String, Option<i64>, Option<String>, Option<String>)>` 出现两次（行类型 + 闭包返回类型）→ `clippy::type_complexity`。修法：抽成测试模块内的 `type EdgeRow = (…);`。
**为什么我自己的门禁没看到它**：我本单只跑过 `clippy -p ruagent-graph`，而这条 gate 是 `clippy -p ruagent-daemon --all-targets -- -D warnings`（wiki 的 verify 命令之一），`-D warnings` 对它那次调用里编译的**每个** target 生效。这是一条真实的读数盲区，记在这里：**改了 `crates/daemon/src/distill.rs` 就必须跑一次 daemon 的 `--all-targets -D warnings`**，不能只看 graph 那一侧。
修后读数：`... clippy -p ruagent-daemon --all-targets -DenyWarnings` → **exit 0**（`Checking ruagent-daemon v0.1.0` 后 `Finished`，无 error/warning）。注意一个读数陷阱（mem-core 与 wiki 都踩到、我复现了同族现象）：cargo 在**第一个失败的 target 就停住**，所以「`--all-targets` 绿」只对**这次真正被重新编译的那些 target** 成立；被缓存的目标不会重新报告。要看每个 target 是否真的被 lint，只能 `--verbose` 看 `clippy-driver --crate-name …` 的逐条调用（我用包装脚本 + `--verbose` 尝试枚举时没拿到那批行，所以本报告不声称逐 target 证据，只声明**这次调用 exit 0**）。

### R-3 DEP-5（mem-core 的 `judge_merge` 接线 + NeedsJudgement 的 LLM 收口）
`mergeable_target` 改为调 `memembed::judge_merge(db, embedder, store, namespace, content) -> (MergeVerdict, String)`，对 `NeedsJudgement` 用 distill 已有的那一次 ACP 往返收口（mem-core 的方向）。今天**未落地且没有半接线**。

### R-4 G4 `extracted ≥30%` 与 G7 关系 recall / 幻觉率
都是 not_measured，需要**一次真实重新蒸馏**（agent 跑批）而不是代码改动；跑批后 `distill_log.prompt_hash` + 边的 `event_time_source`/`source_episode` 就是这三个读数的取数面（本单已把它们写进库）。

## 11 DEP-4 精确定义（captain 要的：谁提供、签名、消费者）
**结论：不需要 graph 再暴露新库接口——库接口已经在了；缺的只是一层触发入口，而它属于 daemon 的 HTTP 面（owner = INT/t19）。** 理由：本仓纪律是「一个消费面」，`api.rs` 已有 `/api/v1/graph/*` 一族端点（`entities/edges/search/entity/{id}/neighbors/entity/{id}/facts`）；合并与建社区若由 graph 自己另开 CLI，就会造出第二个消费面（也正是本代要收敛掉的东西）。

已就绪的库签名（本单实现，无需改动）：
```rust
pub async fn build_communities(db: &Db, level: u32) -> Result<CommunityBuild, DbError>;   // CommunityBuild: Serialize
pub async fn communities(db: &Db, level: u32) -> Result<Option<Vec<Community>>, DbError>; // None = 从未建过
pub async fn set_community_summary(db: &Db, id: i64, summary: &str) -> Result<bool, DbError>;
pub async fn community_coverage(db: &Db, level: u32) -> Result<(u32, u32), DbError>;
pub async fn communities_of(db: &Db, entity: i64) -> Result<Vec<Community>, DbError>;
pub async fn pending_pairs(db: &Db) -> Result<Vec<(i64, i64, String)>, DbError>;
pub async fn merge_entities(db: &Db, keeper: i64, absorbed: i64) -> Result<u32, DbError>; // 返回搬动的边数
```
建议的 HTTP 入口（形状与上面一一对应，落在 `crates/daemon/src/api.rs`，owner=INT/t19）：
| 方法/路径 | 请求 | 响应 | 对应库函数 | 为什么必须显式 |
| --- | --- | --- | --- | --- |
| `POST /api/v1/graph/communities/build` | `{"level":0}` | `CommunityBuild` JSON | `build_communities` | 划分是覆盖式重建，不能随写入自动触发（延迟与幂等都要看得见） |
| `GET /api/v1/graph/communities?level=0` | — | `{"built":false}` 或 `[{id,level,parent,summary,entity_ids}]` | `communities` | 「没建过」与「建了但空」必须可区分（`None` vs 空数组） |
| `PUT /api/v1/graph/community/{id}/summary` | `{"summary":"…"}` | `{"updated":true}` | `set_community_summary` | 摘要是延迟写的（LazyGraphRAG），一次一条 |
| `GET /api/v1/graph/resolution/pending` | — | `[{entity_a,entity_b,reason}]` | `pending_pairs` | 未决队列要有人看得见，否则等于静默丢弃 |
| `POST /api/v1/graph/resolution/merge` | `{"keeper":N,"absorbed":M}` | `{"moved_edges":n}` | `merge_entities` | **合并是决策**：删行 + 别名 + 边重指，只允许人工/agent 显式调用 |
| `GET /api/v1/graph/retrieve?q=…&hops=2` | — | `GraphEvidence`（含 `lines()`） | `retrieve` | 与 DEP-1 是同一个入口：召回与面板应看同一份证据 |
消费者：面板图页面（`panel/`，另一个 owner）与 `ruagent doctor` / CLI。若 captain 更希望 CLI 视角，可选第二形状 `ruagent graph merge --keeper N --absorbed M`（`cli/`，同样不是我的 inScope）——**两者只能选一个**，否则又是两个消费面。

## 12 修订记录（t26/t27 · 2026-09-28 追加，**只追加，不删旧文字**）

按 captain 2026-09-28 统一的规则：**终态交付物不得被静默改写；允许更正，但必须在同一文件内追加一段带日期的修订记录，写明「改了哪几处、旧文字是什么、为什么」。** 本文件在 t9 置 completed 之后被改过两轮，逐处登记如下（旧文字**逐字引用**，因为「当时说过什么」是读者判断 RV-C 结论的前提）。

### 第 1 轮（t26 交付时，2026-09-28）——结构修正与三处状态更新

| # | 位置 | 旧文字（逐字） | 改成 | 为什么 |
| --- | --- | --- | --- | --- |
| 1.1 | §9 标题与门禁表的位置 | `## 9 门禁读数（最后一次，全部走 scripts/cargo-team.ps1）` 这个标题在我插入 §10 时被吞掉，**门禁表被挤到 §11 之后**（表头 `\| 命令 \| 结果 \|` 出现在文件最末） | 恢复 `## 9 门禁读数…` 标题，并把门禁表移回 §9 之下 | 结构错位会让读者以为门禁表属于 DEP-4 那一节；这是同一次交付里连续插入两节造成的，不是内容问题 |
| 1.2 | §1 G9 段末 | 「**目标「库行数 = 日志尝试数」在按次计数上仍到不了**：`distill_log` 的主键是 `session_key`（`0008_memory_embeddings.sql:9`），同一会话多次尝试只能留一行（当前实现选择：失败不覆盖成功）。按次行需要新表或复合主键 —— **这是 I-SCHEMA 的表，不在我的 inScope**，已登记（§3 DEP-3），按 §7.138 上报而不是自行放宽。」 | 「**目标「库行数 = 日志尝试数」已达**（原始状态是到不了：…）；I-SCHEMA-2/0024 重建为按次行 + 本单写入侧改成普通 INSERT 后，读数 `attempts=4 rows=4 recorded_outcomes=4 \| rows - attempts = 0`（§10 R-2）。」 | 0024（t25）与写入侧收口（t26）落地后，这条目标从「到不了」变成「已达」；旧文字保留在此处供 RV-C 对照 |
| 1.3 | §4 not_measured 表 · G9 行 | 「\| G9 「库行数 = 日志尝试数」 \| **到不了** \| `distill_log` PK = `session_key`（I-SCHEMA 的表）⇒ 按次计数需要 schema 变更；已登记 DEP-3 \|」 | 「\| G9 「库行数 = 日志尝试数」 \| **已达** \| 原为到不了（…）；I-SCHEMA-2/0024 重建为按次行 + 本单写入侧改普通 INSERT ⇒ `rows - attempts = 0`（§10 R-2） \|」 | 同上：状态变了，不能留在 not_measured 表里 |
| 1.4 | §3 DEP-3 行 | 「\| DEP-3 \| **I-SCHEMA / recall**：`distill_log` 若要按**次**计数需新表或复合主键（今天 PK=session_key）；`recall_log` 的 `graph_*` 两列已就位 \| 未落地 ⇒ G9 的「差值 0」到不了 \| §7.138；本报告 §1 G9 \|」 | 「\| DEP-3 \| … \| **已落地**（`0024_distill_attempts.sql`，owner=recall）；写入侧由本单收口（§10 R-2）⇒ G9 的「差值 0」已达成 \|」 | 依赖项已收敛，登记表必须跟着收敛，否则下一轮还会有人去追一个已完成的依赖 |
| 1.5 | §10 R-2 标题（重复标题） | 改写 §10 时留下过一行旧标题：`### R-2 G9 写入侧（**等 I-SCHEMA-2 落地后归我做**，captain 已立单 owner=recall）`，紧接在新标题之上 | 删除这一行重复标题，保留「已落地」版本 | 同一节出现两个标题（一个说「等」，一个说「已落地」）会自相矛盾 |
| 1.6 | §10 R-2 正文 + R-2b 新增 | 原文是「I-SCHEMA-2 落地后，写入侧要做且**只做**这些：1…5」（待办清单） | 改为「**已落地**」版本：5 条待办逐条标注落地方式 + 5 条读数；并新增 **R-2b**（`clippy::type_complexity`，wiki/t10 报的阻塞项、修法，以及「我本单只跑过 graph 侧 clippy」这个盲区） | t26 把 R-2 做完了；R-2b 是同一轮里修的阻塞项，属于同一批文件，必须留痕 |
| 1.7 | §2 实现偏差新增第 8 条 | 原 §2 只有 1–6 条 | 新增第 8 条：交付后的两处收口（`log_outcome` 单行 INSERT、测试里的 `type EdgeRow`） | 偏差清单必须包含「交付后修的」，否则读者会以为交付内容与清单一致 |

### 第 2 轮（t27 期间，2026-09-28）——新增 §8.1（派生文件申报）

| # | 位置 | 旧文字（逐字） | 改成 | 为什么 |
| --- | --- | --- | --- | --- |
| 2.1 | §8 末尾 | 「`Cargo.lock` 被 dev-dependency 间接改动（`serde_json` 已在锁文件里，仅 graph 包的依赖列表变化）。」 | 保留该句，并在其后新增 **§8.1 派生文件申报**：带出它的 manifest 行（`crates/graph/Cargo.toml` 的 `[dev-dependencies]` 新增 `serde_json = "1"`）、用途、是否已在 workspace 依赖里、以及**一行 diff**（`+ "serde_json",` 出现在 `[[package]] name = "ruagent-graph"` 的 dependencies 列表；同文件另一行同形状的属队友在途改动） | captain 的规则：manifest 带出的 lock 变化**声明即可、不算越界**，但必须给出**一行 diff**，不能只写「有 1 行变化」 |

### 本记录与被引用结论的关系

* §1/§3/§4/§10 的**数字与判据没有改过**：G7 仍是「未达标 0.7500、已登记、未放宽」（t27 的 repair 另立报告：`gen2-graph-repair.md`）；G1/G2/G5/G6/G8 的读数未改。改的只有 G9 的**状态**（到不了 → 已达）以及结构。
* 本记录**不修改**任何旧读数的口径。RV-C 报告引用「作者写 G9 到不了」时，对照的就是 1.2–1.4 三条的旧文字。

## 13 修订记录（t38 · 2026-09-28 追加，**只追加，不删旧文字**）

RV-C round 2（`gen2-graph-review-r2.md`）判 needs_revision 并开 **RV-C2-5（G5 的三行数字不可复现）/ RV-C2-6（两个对象集被写成一个）**。按同一规则（终态报告不得被静默改写：可以更正，但必须留「改了哪几处、旧文字是什么、为什么」），逐处登记。**§13 的三条更正都改变了数字方向：更正后的读数比我原来写的更好 —— 旧数在别的字节上可能是真的，但它不是交付那一版上的读数，所以按「不可引用」处置，而不是按「更保守所以安全」处置。**

### 13.1 腿计数（§1 G2）：`summary_fts 39` → **46**

* **旧文字（逐字）**：「真库全帧按腿计数：`{"exact_name": 1, "name_token": 8, "summary_fts": 39}`；`alias_table` 0（真图今天没有别名行）、`vector_nearest` 0（本 crate 未接嵌入器，**报告不出来**而不是悄悄消失）。」
* **改成**：`summary_fts: 46`，旧值加删除线并标 **不可引用**。
* **为什么**：RV-C round 1 在同一份副本上读到 46；t38 我在**同一对象集**（活库副本：`recall_log` 651 行 / **23** 条 distinct query、`entities` 63、current 边 60）上，用**三种形状**各跑一次（只跑帧测试 / 两个 ignored 测试并行 / 先合并再跑帧）都读到 **46**，且另外两条腿逐位相同（`exact_name 1`、`name_token 8`）⇒ **差异只在 `summary_fts` 这一条腿**，不在解析逻辑、不在对象集。旧值 39 与 46 相差 7 条，只可能来自「这条腿的输入或代码状态与今天不同」，而那一次运行用的是哪个中间态已不可考（那份副本已删）—— 这正是 §7 一类的交付纪律问题：**读数没有在交付那一版上重取**。可引用的读数是 **46**（命令见下）。
* **命令（可复现，需要一份活库副本）**：
  ```powershell
  python -c "import sqlite3,os;d=os.path.join(os.environ['TEMP'],'ruagent-live.db');c=sqlite3.connect(os.path.join(os.path.expanduser('~'),'.ruagent','data','ruagent.db'));c.execute(\"VACUUM INTO '%s'\"%d.replace('\\\\','/'));c.close()"
  $env:RUAGENT_GRAPH_LIVE_COPY="$env:TEMP\ruagent-live.db"
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-graph --test live-after the_real_query_frame -- --include-ignored --nocapture
  ```
  ⇒ `REAL FRAME n=23 | strict non-empty 1 -> new 13 | queries with >1 seed: 7`、`seeds by leg (all queries): {"exact_name": 1, "name_token": 8, "summary_fts": 46}`。
* **顺带发现的仪器缺陷（t38 新开，owner=我，下一代修）**：`live-after.rs` 的三条 `#[ignore]` 测试**共用同一份副本文件**，而 `Db::open` 会跑迁移（写）⇒ 用 `--include-ignored` 一次跑全部时，**两个测试在 `live-after.rs:29` 的 `Db::open().unwrap()` 上直接崩**（实测 `1 passed; 2 failed`；串行/单独跑则全过）。也就是说「一次跑完全部活库读数」这件事今天做不到；本节的 46 是用**单独跑帧测试**取的（三种形状都得到 46，所以这个数不受该缺陷影响）。

### 13.2 G5 三行（§1 G5、§7 汇总表）：`0.7000 / 0.7500`、中位 `7`、`{Hops 4, MaxPaths 15, None 1}` → **`0.7500 / 0.8000`、中位 `9`、`{Some(Hops) 6, Some(MaxPaths) 13, None 1}`**

* **旧文字（逐字）**：「**path-recall@12 = 0.7000（默认 hops=2）**、0.7500（hops=3）✅ 目标 ≥0.60；facts/查询中位 **7** ✅ 目标 ≥3；20 条 gold 上 `truncated_by` = {Hops 4, MaxPaths 15, None 1}（**每次截断都报出来**）」；另 §7 汇总表写「`retrieve`：**0.70**（hops=2）」。
* **改成**：`0.7500 / 0.8000`、中位 `9`、`{Some(Hops) 6, Some(MaxPaths) 13, None 1}`；旧值加删除线并标 **不可引用**。
* **为什么（根因，按 captain 要求的「哪一类失真」逐条排除）**：
  1. **不是采样面**：gold 的 20 条可达对是冻结的（`multihop.json`），两次读数的对象集相同（`gold reachable n=20` 两次都打印）。
  2. **不是聚合口径**：同一测试、同一打印行、同一 `max_paths=12` 预算。
  3. ~~**是路径选择顺序**：两次读数之间，`crates/graph/src/retrieve.rs` 上**唯一**改变「哪些路径进入候选/被截断」的改动是 **RVC-4 的 tie-break**（排序键末尾追加边 id 序列）。旧键在真数据的并列组 `[49,52,53,55]` 上不是全序 ⇒ 并列路径进入 `truncate(beam=8)` / `truncate(max_paths=12)` 的**顺序**由 `HashMap::into_values()` 决定 ⇒ 预算被任意一组并列路径吃掉。追加边 id 后顺序确定，**改变的恰好是「谁吃掉 MaxPaths 预算」**。~~ ⇒ **t43 更正：这条根因被受控反做推翻，见下。**
  4. ~~**方向自洽**：截断分布 `MaxPaths 15 → 13`、`Hops 4 → 6`（少两条路径在 MaxPaths 上被丢，多两条走到 hop 上限），recall `0.70 → 0.75`、`0.75 → 0.80`，facts 中位 `7 → 9` —— 每一个变化都是「更少的路径被任意丢弃」所预测的方向。~~ ⇒ **t43 更正：这条「方向自洽」是给一个错根因编的自洽性论证，随根因一并作废**（见 §15.2）。数字本身仍然成立，只是不能用它来解释新旧差。
  5. **可核对**：t38 的重跑（`... --test multihop-gold -Nocapture`）与 RV-C round 2 在同一天、同一字节上的独立读数**逐位一致**（`path-recall@12: hops=2 0.7500 hops=3 0.8000`、`facts per query: median 9 min 3 max 12`、`{"None": 1, "Some(Hops)": 6, "Some(MaxPaths)": 13}`）⇒ 新数值是**双方独立复现**过的。
  6. **真因（t43 更正）：读数没在交付那一版上重取。** 判据：把 RVC-4 **反做**（删掉 tie-break）后，**RVC-4 之前的数字一个也回不来**（见 §15 的受控反做对照）⇒ 新旧差不可能由 tie-break 造成，只能是「旧数字取自交付版之前的另一个代码状态」。与 §13.1 的 G2 `summary_fts 39` 是**同一类**失真。

* **仍然成立的弱点（不因更正而消失）**：20 条 gold 里 **13 条报 `truncated_by=Some(MaxPaths)`** ⇒ 这个 recall 是**预算下的读数**，不是检索质量的上限；「缺席」与「被预算截断」在 MaxPaths 那一档不可分（RV-C2 的 U-5 同判）。要对质量下结论需要 `max_paths` 放大后的对照读数，列为下一代（owner=我）。

### 13.3 单例关系名（§10 R-1c）：`24/35` 只有一个对象集 → **两个都写**

* **旧文字（逐字）**：「| R-1c 本体覆盖不足 24/35 单例 + 6/35 ad_hoc | `ontology.json` | …」
* **改成**：`24/35` 标明**对象集 = 全部 67 条边**；`ontology.json` 是 **current 60 条边**的口径，同口径下是 **26/35**（多出 `owns`、`runs_on`）。spec §C·G7 的 baseline 行同步标注（那次是 A7 的 67 条边口径）。
* **为什么**：两个数字都真，但相邻书写会让读者把 67 条边的读数挂到 60 条边的 gold 上。t38 用 `live_graph_snapshot.json` 直接复算：全部 67 条边 → 单例 **24/35**；`invalid_at IS NULL` 的 60 条 → **26/35**（差集恰为 `owns`、`runs_on`）。
* **口径规则（写在这里，供后续复用）**：凡引用关系/边计数，必须写「对象集 = 全部 N 条边（含失效）还是 current N 条」，二者不可互换。

### 本记录与被引用的结论的关系

* 三条更正**只提高** §1/§7/§10 读数的可复现性，**没有改任何 target、没有改判据、没有改 gold**。G7 的 0.7500 → 0.9149、G6 的 1.0000/1.0000、G8 的结构读数都不受影响。
* §12（t26/t27 的修订记录）与 §13 职责不同：§12 记「状态与结构」，§13 记「数字与口径」。两者都只追加。

## 14 t38（repair round 3）的收口登记

**为什么这一节在本文件里**：t38 的 inScope 只有 `gen2-graph-spec.md`、本文件、`crates/graph/tests/extraction-gold.rs`、`crates/daemon/src/distill.rs` 与「G3/G4/G8」；`gen2-graph-repair.md`（t27 的报告）**不在 t38 的 inScope 里**，所以 t38 的登记落在**本文件**（§14）与**规格附录 H.2/H.3**，而不是再去追加 repair 报告。RV-C round 3 若要逐条对照，三件分别在 §14.1/§14.2/§14.3。

RV-C round 2（`gen2-graph-review-r2.md`）判 **needs_revision**：t27 的六条 finding + G7 + R-3 全部**关闭**，但开了 9 条（4 条 medium = 四个不可测目标缺具名 owner；5 条 low = 报告准确性/口径/可证伪性/命令勘误/一致性登记）。t38 按 captain 2026-09-28 的裁决逐条收口。**本单零代码逻辑改动**（`distill.rs` 未动；唯一的代码改动是给 G8 加两条可证伪指标）。

### 14.1 §C 四条目标的显式 `deferred` 裁决（**仍是未达成，不是达标**）

落点 = `gen2-graph-spec.md` **附录 H.3**（并在 §C·G7/G4/G8 就地标注）。按 captain 要求的四要素（**目标原文位置 · 为什么本代测不了（证据）· 下一代的前置 · owner 角色**）：

| # | 目标 | 为什么测不了（证据） | 下一代前置 | owner 角色 |
| --- | --- | --- | --- | --- |
| 1 | G7 实体族 P≥0.85 / R≥0.70 | gold 三族是边/关系名/实体对，**无实体族、无会话语料** | **人工标注**的工程实体族 gold | 人工标注（**需具名成员/任务**） |
| 2 | G7 关系 R≥0.60 + 幻觉率 ≤0.10 | 活库 **0/67** 历史边有 `source_episode` ⇒ 无 transcript 可比对；幻觉率**机制半边已在 fixture 上被测**（含「分不清改写与发明」的已知假阳性） | **一次真实重蒸馏跑批**（产出带 episode 的边集与可回溯语料） | 重蒸馏跑批 + 标注 |
| 3 | G4 `extracted ≥30%`（活库） | 需真实重蒸馏；且下限**挂在第 1/2 条的 gold 会话构成上**（规格原文自述） | 同 1/2；**gold 落地前不判 30%** | 并入 G7 的数据待办 |
| 4 | G8 三指标成对判决 ≥60% | 需 LLM 成对判决**跑批**（协议照 B2），本代无 harness 与对照组 | 评测 harness + 对照组 | 评测跑批 |

**三处同时在位**：§C 就地标 `deferred` · 附录 H.3 表（四要素）· 本 §14.1。**并逐字写明「它们仍是未达成」**——不许被读成通过。**captain 的选项 (i) 依然开着**：只要为「20 段手工标注会话」具名一个成员/任务，第 1–3 条立刻从 deferred 变成可判（我没有替 captain 具名：角色 ≠ 成员）。**本代仍可判的**：G8 结构侧（已降为前置条件 + 两条可证伪指标，见 §14.4）；G7 的关系 **P** 已达标关闭（0.7500 → 0.9149）。

### 14.2 附录 H 的勘误范围扩到 G3/G4/G8（真实 target + 实跑，不改测试）

落点 = `gen2-graph-spec.md` **附录 H.2**（只追加；§C 三行的旧命令用删除线保留，各加一行勘误指针）：

| 目标 | 规格原命令（错，全树 0 命中/文件不存在） | 真实 target | 实跑读数（2026-09-28） |
| --- | --- | --- | --- |
| G3 | `--test provenance-fresh-db` | `--test temporal` + `--test extraction-gold` | `the_old_writer_leaves_the_source_unknown_and_the_new_one_records_it ... ok`，5 passed |
| G4 | `--test as-of-gold` | `--test temporal`（`as_of_gold_returns_what_was_true_then`） | `as_of ruagent at … -> MATCH` **6/6**（8/8、17/17、9/9、17/17、10/10、17/17 条自有边，`orphan edges []`） |
| G8 | `--test communities` | `--test extraction-gold` | `level 0: communities=9 covered=43 non_isolated=43 split_by_modularity=1`；`partition: 9 communities, sizes [14,5,3,3,2,6,2,4,4]`；`summaries written: 1 of 9` |

**不新增三个空壳 target**（否则「测试数量」变成装饰）：三个能力都已有测试，只是名字与规格不一致。

### 14.3 重新可引用的 G5 / G2 读数：两侧并排 + 根因

见 **§13.1（G2 `summary_fts` 39 → 46）与 §13.2（G5 的三行）**，两处都逐字引用了旧文字。汇总：

| 读数 | 原报告（**现标不可引用**） | t38 重取（**可引用**） | RV-C r2 独立读数 | 根因 |
| --- | --- | --- | --- | --- |
| G5 path-recall@12 | `0.7000 / 0.7500` | **`0.7500 / 0.8000`** | 一致 | **读数没在交付那一版上重取**（t43 更正，见 §15；旧写法「排序 tie」已被受控反做推翻） |
| G5 facts 中位 | `7` | **`9`**（min 3 / max 12） | 一致 | 同上 |
| G5 `truncated_by` | `{Hops 4, MaxPaths 15, None 1}` | **`{Some(Hops) 6, Some(MaxPaths) 13, None 1}`** | 一致 | 同上 |
| G2 腿计数 | `summary_fts: 39` | **`summary_fts: 46`**（另两腿 1/8 逐位相同） | round 1 也是 **46** | 差异**只在 FTS 这一条腿**；对象集（23 查询/651 行/63 实体/60 current 边）与解析逻辑两次相同 ⇒ **读数没在交付那一版上重取** |

**逐类排除**：G5 的两侧差异**不是采样面**（gold 20 条可达对冻结）、**不是聚合口径**（同一测试同一打印行），**也不是排序 tie**（t43 受控反做：把 RVC-4 反做后新数字**逐位不变**、旧数字**一个也回不来**，§15）⇒ 与 G2 **同一根因：读数没在交付那一版上重取**。**残留弱点**：13/20 条 gold 报 `Some(MaxPaths)` ⇒ 该 recall 是**预算下的读数**、不是质量上限（与 RV-C2 的 U-5 同判），需要 `max_paths` 放大后的对照读数（owner=我，下一代）。


### 14.4 RV-C2-7 的可证伪结构指标 + 两条仪器缺陷

**G8 的旧 target「覆盖 ≥90% 非孤立实体」按构造必真**（`covered` 的定义即是非孤立实体数）⇒ 结构侧**降为前置条件**（幂等、成员互斥、层级挂载、`covered == non_isolated`、`split_by_modularity`、摘要惰性 —— 断言全部保留），并新立**两条能红的指标**（`crates/graph/tests/extraction-gold.rs`，读数与阈值同一行打印）：

```
G8 falsifiable structure: largest community 14/43 = 0.3256 (target <= 0.50) | isolated 20/63 = 0.3175 (target <= 0.40)
```

**两条仪器缺陷（t38 新开，owner=我，下一代修）**：
1. **`live-after.rs` 的三条 `#[ignore]` 测试共用同一份副本文件**，而 `Db::open` 会跑迁移（写）⇒ `--include-ignored` 一次跑全部时**两个测试在 `live-after.rs:29` 的 `Db::open().unwrap()` 上直接崩**（实测 `1 passed; 2 failed`；单独跑全过）。所以「一次跑完所有活库读数」今天做不到；§13.1 的 46 是单独跑帧测试取的（三种形状都得到 46，不受该缺陷影响）。
2. 活库副本必须 `VACUUM INTO`，且**目标文件已存在就报错** ⇒ 复现脚本须先删目标（命令已写进 §13.1）。

### 14.5 RV-C2-9：E.10/D4 的一致性裁决（**已取得 mem-core 确认**）

**已确认**：草稿契约发给 mem-core（message `cd4f804d-7c18-4865-85e9-91f5a3bd7c8a`），**mem-core 回复「同意」**，并给出一处**措辞加强**：`run_turn_failed` **不是**唯一的非 `run_turn` 溯源 episode —— 记忆侧早有 `kind='manual'` 一例（t347 迁移为历史行新建，`crates/memory/src/lifecycle.rs:907` 有断言）⇒ **「溯源 episode 存在 ≠ 蒸馏发生过」是通则，徽章只认 `run_turn` 不是为失败路径开的例外**。

**记忆侧逐条查证（mem-core 给的依据）**：① 全仓**没有**生产代码读 `episodes.kind` 做判定；② 唯一的 kind 无关读路径 `query.rs:302 session_memories` 按 `e.source_run` 连接、**不看 kind** ⇒ 标记前后同一结果；③ `source_episode` 的外键**没有 `ON DELETE`**（NO ACTION + `foreign_keys=ON`）且引用 `episodes.id` 而非 kind ⇒ **改 kind 不可能让来源失效**（这正是「删除要么撞外键、要么丢来源」的机制版）。**风险面（主动报的，不是反例）**：`crate::episode::episode_count` 是**全表计数、不分 kind**，今天**没有生产调用点**；若哪天面板/派生布尔改用它当「蒸馏发生过」的代理，就会把被标记的失败算成成功 —— 已在裁决里点名。**并确认**我的 `... WHERE id = ?1 AND kind = 'run_turn'` guard 正确（重复标记 no-op，且不会把非 `run_turn` 的行标成失败）。

**落点与责任划分**：条款文字按 captain 裁决由 **mem-core 的 t41** 收口（我只请求确认、**未改 §E.10 一个字节**）；一致性责任 = mem-core 拥有 `memories.source_episode` 的写入与 `episodes.kind` 的取值集，graph/I-C 拥有失败路径的标记（`void_episode`）与「episodes 计数不变」的断言。**任一方要改「标记还是删除」「徽章读哪个 kind」之前，先改 E.10/D4 这一条。**

## 15 修订记录（t43 · 2026-09-28 追加，**只追加，不删旧文字**）：§13.2/§14.3 的 G5 **根因**更正

**触发**：RV-C round 3 的 **RV-C3-1（low，诊断准确性）**：t38 在 §13.2/§14.3 给 G5 归一化写的根因「**排序 tie**」被评审的受控反做实验**推翻**。本单只更正**根因**，**不动任何读数值**（G5 的新读数成立、`≥0.60` 达标、判据不受影响）。

### 15.1 改了哪几行

| # | 位置（本文件） | 处置 |
| --- | --- | --- |
| 1 | §13.2 第 3 条「是路径选择顺序」（原第 352 行） | 整条**删除线保留 + 标注「t43 更正：已被受控反做推翻」** |
| 2 | §13.2 第 4 条「方向自洽」（原第 353 行） | 同样删除线保留 + **标注「这是给一个错根因编的自洽性论证，随根因作废」** |
| 3 | §13.2 新增第 6 条 | **真因：读数没在交付那一版上重取**（判据 = 反做后旧数字一个也回不来） |
| 4 | §14.3 表 · G5 三行的「根因」列（原第 406–408 行） | 从「**排序 tie**：…」改为「**读数没在交付那一版上重取**（t43 更正，见 §15；旧写法『排序 tie』已被受控反做推翻）」 |
| 5 | §14.3 「逐类排除」段（原第 411 行） | 「**是排序 tie**」改为「**也不是排序 tie**（t43 受控反做：反做后新数字逐位不变、旧数字一个也回不来）」⇒ 与 G2 同一根因 |

### 15.2 旧文字（逐字引用）

* §13.2 第 3 条：「**是路径选择顺序**：两次读数之间，`crates/graph/src/retrieve.rs` 上**唯一**改变「哪些路径进入候选/被截断」的改动是 **RVC-4 的 tie-break**（排序键末尾追加边 id 序列）。旧键在真数据的并列组 `[49,52,53,55]` 上不是全序 ⇒ 并列路径进入 `truncate(beam=8)` / `truncate(max_paths=12)` 的**顺序**由 `HashMap::into_values()` 决定 ⇒ 预算被任意一组并列路径吃掉。追加边 id 后顺序确定，**改变的恰好是「谁吃掉 MaxPaths 预算」**。」
* §13.2 第 4 条：「**方向自洽**：截断分布 `MaxPaths 15 → 13`、`Hops 4 → 6`…recall `0.70 → 0.75`、`0.75 → 0.80`，facts 中位 `7 → 9` —— 每一个变化都是「更少的路径被任意丢弃」所预测的方向。」
* §14.3 G5 行「根因」列：「**排序 tie**：RVC-4 的 tie-break（排序键追加边 id）改变了并列路径进入 `truncate(beam/max_paths)` 的顺序 ⇒ 预算不再被任意一组并列路径吃掉。方向自洽：`MaxPaths 15→13`、`Hops 4→6`、recall +0.05 两级、中位 `7→9`」。
* §14.3 逐类排除段：「…**不是聚合口径**（同一测试同一打印行）、**是排序 tie**；…」

**为什么错**：这段根因把「方向上说得通」当成了证据（post-hoc 自洽），而**没有做反做**（把 RVC-4 拆掉看数字会不会回去）。反做一做就崩：拆掉 tie-break 后新数字**逐位不变**，旧数字**一个也回不来** ⇒ tie-break 与新旧差**无关**。

### 15.3 受控反做对照（RV-C r3 的实验 + 我 t43 的独立复跑）

**评审的形状**（`gen2-graph-review-r3`，逐字转述其读数）：导出副本 `%TEMP%\rvc3-tree` + 独立 target `%TEMP%\ruagent-cmp-rvc3`，删掉 `retrieve.rs:931/:957` 两处 `.then_with(|| edge_seq(a).cmp(&edge_seq(b)))`（**反做 RVC-4**），**3/3 次**仍是 `path-recall@12 0.7500/0.8000`、facts 中位 `9`、`{None 1, Some(Hops) 6, Some(MaxPaths) 13}` —— **与交付树逐位相同**；它复现出来的只有**不可复现性本身**（`6 runs byte-identical: false`，长度 `13756/13701/13680/13738/13719/13667`）。

**我的独立复跑**（t43，同样的形状，2026-09-28；副本 `%TEMP%\rvc3-tree`，robocopy 导出 1053 文件 / 80.7 MB，独立 `-TargetDir %TEMP%\ruagent-cmp-rvc3`；**共享工作树未动**，复跑后核对 `crates/graph/src/retrieve.rs` 仍是 2 处 tie-break）：

| 树 | 读数 |
| --- | --- |
| **交付树**（RVC-4 在位） | `6 runs of the same query: lengths [13696, 13696, 13696, 13696, 13696, 13696] \| byte-identical: true`；`path-recall@12: hops=2 0.7500 hops=3 0.8000`；`facts per query: median 9 min 3 max 12`；`{"None": 1, "Some(Hops)": 6, "Some(MaxPaths)": 13}`；`test result: ok. 7 passed` |
| **反做树**（两处 tie-break 均删）run 1 | `lengths [13709, 13731, 13716, 13687, 13640, 13729] \| byte-identical: false`；**gold 三行与交付树逐位相同**；`FAILED. 6 passed; 1 failed` |
| **反做树** run 2 | `lengths [13713, 13680, 13724, 13726, 13702, 13680] \| false`；gold 三行同 |
| **反做树** run 3 | `lengths [13661, 13703, 13737, 13693, 13721, 13675] \| false`；gold 三行同 |

**三点结论**：① **3/3 次** gold 三行与交付树**逐位相同** ⇒ 旧数字（`0.7000/0.7500`、中位 `7`、`{Hops 4, MaxPaths 15}`）**不是** tie-break 造成的，它们在反做后的代码上**同样不可复现** ⇒ 真因就是「**读数没在交付那一版上重取**」；② 反做复现出来的只有**不可复现性本身**（交付树 `13696 ×6` 恒定，反做树每次 6 个不同长度）；③ **额外收获（RVC-4 的负控）**：反做树上失败的那一条正是 t27 加的 `the_same_query_serializes_byte_identically_across_runs`（`6 passed; 1 failed`）⇒ **把 RVC-4 拆掉，它有且仅有它会红**，说明那条测试真的在守 RVC-4，不是装饰。
**一个操作细节也记下来（免得下一个人以为反做没做全）**：我第一次用带缩进的字面量替换，只删掉了 `:931`、漏了 `:957`（缩进不同）；**只删一处时 gold 三行也已经与交付树相同**，删满两处后 3/3 仍相同 ⇒ 结论对「删一处 / 删两处」都成立。

### 15.4 同一根因错误在别处是否也出现（grep）

命令：`Select-String -Path docs\design\reviews\gen2-graph-{impl,spec,repair}.md,gen2-graph-review-r2.md -Pattern "排序 tie|tie-break|tie break|并列路径"`

| 文件:行 | 内容 | 判定 |
| --- | --- | --- |
| `gen2-graph-impl.md:352`（旧） | 「**是路径选择顺序**…RVC-4 的 tie-break…」 | **错根因** ⇒ 本单已在 §13.2 划掉并更正 |
| `gen2-graph-impl.md:406–408`（旧） | §14.3 表 G5 三行「**排序 tie**…」 | **错根因** ⇒ 本单已更正为「读数没在交付那一版上重取」 |
| `gen2-graph-impl.md:411`（旧） | §14.3「…**是排序 tie**；…」 | **错根因** ⇒ 本单已更正 |
| `gen2-graph-spec.md:326` | 「确定性 tie-break = (score 降序, hops 升序, 节点 id 序列字典序升序)」 | **不是错根因**：这是规格对**接口**的定义（RVC-4 的修法本身就要求它），与「新旧数字差异从哪来」无关 ⇒ **不改**（且 spec 不在本单 inScope） |
| `gen2-graph-repair.md` | 无命中 | — |
| `gen2-graph-review-r2.md` | 无命中（RV-C2 只要求「重跑/标不可引用」，没要求根因） | — |

⇒ **根因错误只出现在本文件的三处**，且三处都已在 §13.2/§14.3 更正；规格里的 `tie-break` 是接口定义，属另一个语义，**保留**。

**读这张表时注意**：表里的行号是 **t43 更正之前**的行号。更正之后同一条 grep 会**额外命中本节的更正文字**（划掉的旧条、§14.3 的「也不是排序 tie」、§15 的引用）—— 那是**更正记录**在引用它，不是又出现了一次错根因。判据：**旧文字必须带删除线/引号且紧邻「t43 更正」，否则才算漏改**（本单复查结果：0 处漏改）。

### 15.5 没有改的东西（本单的边界）

* **读数值一个未改**：`0.7500 / 0.8000`、中位 `9`、`{Some(Hops) 6, Some(MaxPaths) 13, None 1}`、`summary_fts 46`、G7 的 `0.9149` 等全部保持原样；`≥0.60` 的判定不受影响。
* **§14.3 的残留弱点句保留**（评审明确肯定它）：「13/20 条 gold 报 `Some(MaxPaths)` ⇒ 该 recall 是**预算下的读数**、不是质量上限」。
* **`crates/**` 一行未动**（本单是记录更正）；反做只发生在 `%TEMP%` 的副本 + 独立 target 上，共享工作树与共享 target 未受影响。
* **教训（写给下一代）**：**读数要可复现，根因要可反做。** 一个「方向上自洽」的根因不是证据；能把它拆掉再跑一遍、看旧数字会不会回来，才是证据。本代已第二次栽在同类问题上（上一次是把 recall 共享树 `git stash` 的症状误归因成「re-export 名单漂移」）。

