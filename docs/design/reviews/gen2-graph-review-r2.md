# RV-C round 2 评审（t28）：t27 的六条 finding / G7 目标 / R-3 逐条判决 + C 节重判

> **结论：`needs_revision`（任务置 failed）**。
> **好消息先说清**：t27 声明的 **6 条 medium+ finding（RVC-1/3/4/5/6/10）+ G7 关系级 precision + captain 追加的 R-3 全部关闭**，而且**每一条都有我自己的复现读数**（§1、§5）——包括我自己在**导出树 + 独立 target dir** 里亲手构造的一个多跳问题。RVC-2（high，0.7500<0.80）现在由我实测 **43/47 = 0.9149**。
> **为什么仍不是 pass**：C 节里还有 **4 条目标属于「不可测 + 规格未具名 owner」** —— G7 的**实体族** P≥0.85/R≥0.70、G7 的**关系 recall**≥0.60 与**幻觉率**≤0.10、G4 的 `extracted ≥30%`、G8 的**三指标成对判决**≥60%。t27 把它们登记成 `not_measured` 并写了 owner **角色**（「下一代数据待办」/「评测 harness」/「跑批」），但规格与任务单里**没有一个具名成员或任务**接下它们；按本单契约的原文「**规格未具名 owner 却无法测量的目标，仍算未达成**」⇒ 判未达成（§3-B、§4 RV-C2-1..4）。另有 4 条 low 未闭（RVC-7/8/9 + 一条同族的**规格复现命令指向不存在的 target**）与 1 条一致性登记（RVC-3 的 `run_turn_failed` 与 D4 字面，§4 RV-C2-9）。
> **这不需要任何代码返工**：残余项的修复路径只有两条 —— **(i) captain 为那 4 条具名 owner/任务**，或 **(ii) 修订规格把它们显式 deferred**。任一条落地后本单可直接翻 `pass`，不需要再改一行 Rust。
> **评审员**：review（独立评审员，**不是** I-C 的作者；仓库里我只写本文件，`crates/` 与 `panel/` 一行未改；我自己的探针只存在于 `%TEMP%` 的导出树里）。
> **字节绑定**：`crates/graph/src/retrieve.rs`、`tests/{fixture,multihop-gold,temporal,extraction-gold,resolution,seed-resolution}.rs` 与 `crates/daemon/src/distill.rs` 的**当前工作树字节**（我全部现场重跑，见 §1）；`docs/design/reviews/gen2-graph-repair.md`（192 行 / 02:27）· `gen2-graph-impl.md`（02:17）· `gen2-graph-spec.md`（02:16，含附录 H）。
> **时间窗**：2026-09-28T02:1x → 02:5x +08:00。
> **纪律**：每一次 cargo 都走 `scripts/cargo-team.ps1`（单一编译 / 共享 target，**未**自设 `CARGO_TARGET_DIR`；任务单里那一行是对全队纪律的有意替换）。我的多跳探针跑在 `%TEMP%\rvg-tree`（工作树导出）+ **独立** `-TargetDir %TEMP%\ruagent-cmp-rvg`，不碰共享 target、不碰共享工作树。真守护进程 **pid 79984 未启停**；`~/.ruagent` 只读；**未用** `GET /api/v1/recall`。

---

## 1 我自己的读数

### 1.1 门禁（我自己的运行，共享 target）

```
scripts/cargo-team.ps1 test -p ruagent-graph          → exit 0
  11 个 target 全部绿：3 / 1 / 3 / 7 / 0 / 0(3 ignored) / 7 / 3 / 5 / 5 / 0 ⇒ 34 passed / 0 failed / 3 ignored
scripts/cargo-team.ps1 test -p ruagent-daemon --lib distill → exit 0，15 passed / 0 failed
```

`3 ignored` 与作者口径一致（`live-after` 的三条 opt-in 活库副本读数）。

### 1.2 我自己的多跳问题（**导出树 + 独立 target**，不是作者的 gold 种子）

`%TEMP%\rvg-tree`（工作树全量导出，1112 文件 / 88.5 MB）+ `%TEMP%\ruagent-cmp-rvg`（独立 target，冷编译 12.6 s）+ 我自己写的 `crates/graph/tests/rv-own-multihop.rs`：

```
RV OWN SEED (my rule: max live degree, ties by id) = id 1 "ruagent" degree 33
MY QUESTION: text="ruagent" hops=3 paths=12 facts=11 truncated_by=Some(MaxPaths)
  ...12 条路径（8 条 1 跳 + 4 条 2 跳）
  LINE ruagent -exposes-> Model Context Protocol (MCP) [hops=1 …]
  LINE ruagent <-owns- hfldqwe [hops=1 …]
  LINE ruagent -runs_on-> Windows 11 [hops=1 …]
  LINE ruagent -built_on-> Agent Client Protocol (ACP) <-implements- DeepSeek Harness (dsh) [hops=2 …]
DIRECTION: 16 hops cross-checked against stored rows, all equal
RENDER: 16 hops appear in 12 rendered line(s)
DETERMINISM: 1 distinct JSON blobs over 3 runs (15636 B)
MY BFS: reachable-within-3 = 32 ; >=4-hop = 3
NEGATIVE: "Podman" is 6 hops away by my BFS; claimed at hops=3: false
RVC-1 AS_OF: 1 distinct blobs over 3 spellings of the SAME instant (15636 B)
RVC-1 NON-INSTANT: Some("io error: as_of is not an instant: \"yesterday\" (accepted: RFC3339 with Z …)")
test rv_own_multihop_question ... ok（exit 0）
```

**形状判决（任务单要求的那一条）**：每一个返回的跳我都拿 `edge_id` 回查 `entity_edges` 的 `(src, dst)` 并**逐跳比对**（16/16 相等）；`lines()` 里同时出现 `-rel->` 与 `<-rel-` 两种箭头，**反向跳按存储方向渲染**（`ruagent <-owns- hfldqwe`）；同一问题 3 次序列化**逐字节相同**。⇒ G5 的证据形状与 D.2 契约在**我的**输入上成立。
**诚实边界**：我那次走法的 `truncated_by=Some(MaxPaths)`（12 条路径撞了上限），所以「6 跳的 `Podman` 没被声称」是在**被截断的走法**上的观察 —— 它是「没有凭空发明」的弱证据，不是「全走法可达性」的证明。

### 1.3 G7 / G8 / G5 / G6 / G2 的原始行（我跑出来的）

```
… --test extraction-gold -Nocapture
relation-level precision (strict) = 0.7500 | endpoint-level = 0.9333 | duplicate 0.1833 | mislabel 0.0667
G7 AFTER: replay of 60 frozen current edges -> written 47, duplicate 11, refused 2 | labels {"duplicate":2,"mislabeled":2,"supported":43}
G7 AFTER: relation-level precision (strict) = 43/47 = 0.9149  (BEFORE: 45/60 = 0.7500)
G7 ontology direction: distinct relation names 35 -> 29, object-specific (ad_hoc) 6 -> 3
level 0: communities=9 covered=43 non_isolated=43 split_by_modularity=1
partition: 9 communities, sizes [14,5,3,3,2,6,2,4,4], split_by_modularity=1

… --test multihop-gold -Nocapture
gold reachable n=20 | old(1-hop current_facts) reached 0 | new hops=2 reached 15 | new hops=3 reached 16
truncated_by distribution over the gold seeds: {"None": 1, "Some(Hops)": 6, "Some(MaxPaths)": 13}
facts per query: median 9 min 3 max 12
path-recall@12: hops=2 0.7500 hops=3 0.8000
the largest parallel-edge group in the fixture: [49, 52, 53, 55]
6 runs of the same query: lengths [13696 ×6] | byte-identical: true
ranking key: 12 paths -> (score,hops,nodes) distinct 8, + edge ids distinct 12
Q "微信": 4 paths, 4 reverse hops, all in the stored direction
direction check: 40 paths across 4 questions, 26 of their hops traverse an edge backwards, all rendered in the stored direction
beyond-cap negatives checked: 5

… --test resolution -Nocapture
resolution gold n=24 | TP=7 FP=0 FN=0 TN=16 | abstain(queue)=5 | declared-alias-only=1 | precision=1.0000 recall=1.0000
BEFORE (norm_name equality): 0/8 true pairs merged
BEFORE: the judge finds 7 redundant pairs → AFTER: merged 5 pairs, redundant pairs = 0, edges still 67, aliases 5

… --test seed-resolution -Nocapture
seeds_by_leg for "ruagent": [(ExactName,1),(AliasTable,0),(NameToken,0),(SummaryFts,11),(VectorNearest,0)]
self-name resolution: old(strict) 63/63 -> new 63/63 legs={ExactName}
```

### 1.4 RVC-1 / RVC-10 的仪器读数（`--test temporal -Nocapture`，我自己的运行）

```
instant 1: canonical | Z | +08:00 -> facts 9/9/9 | bytes 10349/10349/10349 | identical true
   old TEXT rule vs the parsed instant: canonical 0/67, Z-form 0/67, +08:00-form 27/67 judged backwards
instant 2: facts 21/21/21 | bytes 21268×3 | identical true   (old: 0/0/17 backwards)
instant 3: facts 43/43/43 | bytes 39215×3 | identical true   (old: 0/0/0  backwards)
as_of="not-a-time" -> Some("io error: as_of is not an instant: \"not-a-time\" …")
as_of gold: 6 条 (8/8, 17/17, 9/9, 17/17, 10/10, 17/17 edges) 全部 MATCH，orphan edges []
TrueAsOf: 1 of the 1 true-then edges came back; 1 of 9 rendered lines say true_as_of
```

⇒ 三个瞬间 × 三种写法**逐字节相同且非空集**（9/21/43 条不同事实）——这正是**能让旧缺陷变红**的仪器（同一跑里内联的旧文本规则在 `+08:00` 形上 27/67、17/67 判反）。

### 1.5 RVC-3 / R-3 / G9 的读数（`test -p ruagent-daemon --lib distill -Nocapture`，我自己的运行）

```
READING RVC-3: episodes 1 -> 1 | kind=run_turn 1 -> 0 | episode kind="run_turn_failed"
               | meta=Some("{\"voided_by\":\"write_graph\",\"reason\":\"adding relation runs_on\"}") | memories keeping their provenance 1
READING R-3: merge_judged rows 0 -> 2 | written=2 skipped=0 | live=1 superseded=1
READING edges: [("runs_on", Some(1), Some("extracted"), Some("3b86510571381087")), ("uses", Some(1), Some("recorded"), Some("330893524acef598"))]
READING distill_log by status: [(Some("empty"),1),(Some("failed"),2),(Some("ok"),1)]
READING R-2: attempts=4 rows=4 recorded_outcomes=4 | rows - attempts = 0
READING one session, two attempts: [(1,"ok",None,Some("hash-a")), (2,"failed",Some("Query returned no rows"),Some("hash-a"))]
READING the newest attempt of that session: failed
READING turns: session-start-only=0 missing-file=0 real-chat=1
```

---

## 2 C 节逐条重判

| G | 规格 target（压缩） | 我的独立读数 | 判决 |
| --- | --- | --- | --- |
| **G1** 实体腿不再恒空 | 真实帧 ≥13/23；生产 `entities>0` ≥50% | 腿级 13/23（round 1 我实测；本轮 `the_live_query_frame_is_within_the_new_legs_reach` **ok**，种子逻辑未被 t27 触及）；生产端仍是 `api.rs:2524` 的 `search_entities`（零 `resolve_seeds/retrieve` 调用点） | 腿级**达成**；生产端 **not_measured**，owner=INT/t19（DEP-1，excused） |
| **G2** 逐腿归因 | 5 条腿（含 0）都在证据里；23 条覆盖 | `[(ExactName,1),(AliasTable,0),(NameToken,0),(SummaryFts,11),(VectorNearest,0)]`（含 0）；23 条覆盖测试 **ok** | **达成**；作者报告里的聚合计数不可引用 ⇒ RV-C2-5 |
| **G3** 每条新边带来源 | 本次抽取 100%；失败保持 NULL；历史不追溯 | `edges: [("runs_on",Some(1),Some("extracted"),Some("3b86…")), ("uses",Some(1),Some("recorded"),Some("3308…"))]` ⇒ **2/2** 带 episode + 16 位 `fact_hash`；`after four phrasings … rows_with_source_episode=4`；活库仍 0/67（不追溯） | **达成** |
| **G4** 事件时间 ≠ 记录时间 | 新边 100% 带 `event_time_source`；`extracted ≥30%`；as-of gold 6/6 | 新边 `extracted`/`recorded` 两态都在读数里；as-of gold **6/6 MATCH**（我的运行，含 `TrueAsOf` 一例）；三种写法逐字节相同 + 非时刻报错 ⇒ **RVC-1 关闭、黄金不再对缺陷盲**；`extracted ≥30%`（活库）**无读数** | 写入侧/时点查询**达成**；`extracted ≥30%` **未达成**（§3-B、RV-C2-3） |
| **G5** 多跳检索 | recall@12 ≥0.60；facts 中位 ≥3；截断必报 | **0.7500(hops=2) / 0.8000(hops=3)**、facts 中位 **9**、`{None 1, Hops 6, MaxPaths 13}`、旧 1 跳 0/20、≥4 跳负例 5/5、**我自己的问题 12 路径/16 跳方向全对/3 次逐字节相同/我自己的 BFS 阴性** | **达成**；作者报告数字不可引用 ⇒ RV-C2-5 |
| **G6** 消解与别名 | 24 对 P/R；真库冗余 0；未决不静默 | `TP=7 FP=0 FN=0 TN=16 | abstain=5`、`P=1.0000 R=1.0000`；旧 `norm_name` 0/8；真库 7→**0**（合并 5、`edges 67` 不变、`aliases 5`）、`queued: #65 vs #37` | **达成** |
| **G7** 抽取质量 gold | 实体 P≥0.85/R≥0.70；关系 P≥0.80/R≥0.60；非法=0；幻觉≤0.10；gold 落 `docs/design/reviews/gold/` | **关系 P = 43/47 = 0.9149 ≥ 0.80（我的实测）**；本体 35→29 名、ad_hoc 6→3；`unsupported=0`；实体族 / 关系 recall / 幻觉率**无读数** | 关系 P **达成**（RVC-2 关闭）；**实体族未达成**（RV-C2-1）；**recall/幻觉率未达成**（RV-C2-2）；gold 落点由**规格附录 H** 勘误到 `crates/graph/tests/gold/`（RVC-6 关闭） |
| **G8** 社区层 | 覆盖 ≥90% 非孤立；全局注入 ≤2 条；三指标 ≥60% | `communities=9 covered=43 non_isolated=43 split=1`、sizes、幂等测试 ok；**判据按构造必真**（先 `assert_eq!(covered, non_isolated)` 再断言 ratio） | 结构侧**达成但判据不可证伪** ⇒ RV-C2-7；全局注入 not_measured（owner=INT/t19，excused）；**三指标未达成**（RV-C2-4） |
| **G9** distill 三态 | 同窗口差值 0；失败带 reason；empty/failed 可分；failed 不建 episode | `attempts=4 rows=4 recorded_outcomes=4 / rows-attempts=0`；三态 `[empty 1, failed 2, ok 1]`；`newest=failed` 不覆盖先前 `ok`；失败 reason `Query returned no rows`；`turns: real-chat=1`；RVC-3 修复后 **`kind=run_turn 1→0`**（徽章不撒谎）+ 来源保留 | **达成**（差额 0 由 t25/t26 落地后成立，我在**当前字节**复现）；**「失败绝不创建 episode」的字面**由 RV-C2-9 登记为一致性解释 |
| **G10** 图证据进注入 | ≥1 个运行路径出现 `<graph>` | 生产注入路径在 `api.rs`/`runs.rs`；`TAG_GRAPH`/rank 3 由 mem-core 落地（RV-B 已核） | **not_measured**，owner=INT/t19 + mem-core（excused；终判 t20/t21） |

---

## 3 owner 清单：excused 与「仍算未达成」的分界

### (A) 不判 I-C 失败（规格/任务单**具名** owner = INT/t19，接线后才能测）

| # | 目标/读数 | 具名 owner | 为什么只能等它 |
| --- | --- | --- | --- |
| A-1 | G1 生产端到端（召回响应实体腿 / `recall_log.entities>0` ≥50%） | **INT/t19**（规格 E.9 DEP-1） | 生产点是 `api.rs:2524`；`recall_log.graph_entities/graph_paths` 列已由 t6 备好但无人写 |
| A-2 | G10 `<graph>` 进注入 + G8「全局注入 ≤2 条摘要」 | **INT/t19**（生产者）+ mem-core（词表**已完成**） | 注入块在 `api.rs`/`runs.rs` 拼装；任务禁止启停 pid 79984、禁止用 `/api/v1/recall` 取证 |
| A-3 | G9 的**活库**同窗口差值 | 运维/跑批（活库迁到 0024 后跑一次真实 distill） | 活库仍是 18 版；机制已在临时 root 上由我复现（`attempts=4 rows=4`） |

### (B) 仍算**未达成**（不可测且规格**未具名** owner —— 只写了角色）

| # | 目标 | 为什么仍算未达成 | 谁才能解 |
| --- | --- | --- | --- |
| B-1 | G7 实体族 P≥0.85 / R≥0.70（「20 个 gold 会话」） | 现 gold 无实体族、无会话语料；附录 H 登记 owner=「下一代数据待办」（**角色**，不是成员/任务） | captain 具名 owner/任务，或修订规格 deferred |
| B-2 | G7 关系 recall ≥0.60、幻觉率 ≤0.10 | 需要带 transcript 的 gold 语料（历史边 0/67 有 episode） | 同上 |
| B-3 | G4 `extracted ≥30%`（活库） | 需要一次真实重蒸馏 + gold 会话里「文本真带时间」的比例（规格把下限挂在 G7 上） | 同上 |
| B-4 | G8 三指标成对判决 ≥60% | 需要 LLM 判决跑批；规格只写「判决脚本（协议照 B2）」，报告写 owner=「评测 harness」（**角色**） | 同上 |

---

## 4 findings

### RV-C2-1（medium · 目标未达成）G7 实体族 P≥0.85 / R≥0.70 无读数、无具名 owner
* **事实**：现有 gold 三族是边 / 关系名 / 实体对（`edges.json` / `ontology.json` / `resolution.json`），**没有实体族与会话语料**；t27 已把它登记为 `not_measured`（附录 H §4 + repair §5），这是**进步**（round 1 时它连登记都没有，是 RVC-6(a)）。但登记里的 owner 是「下一代数据待办（需人工标注 20 段会话）」——**角色，不是成员或任务**。
* **requiredFix（二选一，均不需要代码返工）**：(i) captain 为一个成员/任务具名，明确交付「20 段手工标注会话」；或 (ii) 修订规格 C·G7，把实体族一行显式标 `deferred（需人工标注会话，本代不判）`。若 captain 裁决既有登记已足够，本项即转 `not_measured(excused)`。

### RV-C2-2（medium · 目标未达成）G7 关系 recall ≥0.60 与幻觉率 ≤0.10 无读数、无具名 owner
* **事实**：`0/67` 历史边有 `source_episode` ⇒ 没有可回溯的 transcript 语料可比对；t27 登记原因与 B-1 同族，owner 仍是角色（「需重跑真实蒸馏 + 标注」）。
* **requiredFix**：同 RV-C2-1 的二选一；并注明**幻觉率的机制半边已在 fixture 上被测**（含「分不清改写与发明」的已知假阳性），缺的是语料而不是实现。

### RV-C2-3（medium · 目标未达成）G4 的 `extracted ≥30%`（活库）无读数、无具名 owner
* **事实**：写入侧已达（新边 100% 带 `event_time_source`，`extracted`/`recorded` 两态都在我的读数里），时点查询 6/6 且仪器已能证伪（§1.4）；只有活库比例没有读数 —— 它需要一次真实重蒸馏，而规格把 30% 的下限**挂在 G7 的 gold 会话构成上**。
* **requiredFix**：同 RV-C2-1；或把该半条并入 G7 的数据待办，明确「30% 在 gold 会话落地前不判」。

### RV-C2-4（medium · 目标未达成）G8 三指标成对判决 ≥60% 无读数、无具名 owner
* **事实**：需要 LLM 成对比较跑批（规格 B2 协议）；报告写 owner=「评测 harness」（角色）。
* **requiredFix**：同 RV-C2-1；或修订规格把 G8 的三指标显式 deferred，并把结构侧降为前置条件（见 RV-C2-7）。

### RV-C2-5（low · 报告准确性，RVC-7 仍开）作者报告的数字在当前字节上不可复现
* **改前→我的复现**：报告 §1 写 `path-recall@12 = 0.7000 / 0.7500`、`facts 中位 7`、`truncated_by {Hops 4, MaxPaths 15, None 1}`；我**今天在同一字节上**读到 **`0.7500 / 0.8000`、中位 9、`{None 1, Hops 6, MaxPaths 13}`**（round 1 的读数与今天逐位一致）。G2 的聚合计数同理（报告 `summary_fts 39`；round 1 我在同一份副本上读到 46；本轮我只复现了逐腿打印的形式，未重取聚合值，**如实标注**）。
* **requiredFix**：把报告里这三行标为**不可引用**并换成可复现读数（或由作者重跑一次）。**target 与判决不受影响**，方向上作者**低估**了自己的实现。

### RV-C2-6（low · 口径，RVC-8 仍开）「24/35 单例」与相邻 gold 不是同一对象集
* **事实**：`24/35` 是**全部 67 条边**上的单例关系名数；**current 60 条边**上是 `26/35`（多出 `owns`、`runs_on`）。报告 §1 与 `ontology.json`（60 条 current 边）紧邻书写，读者会把两个对象集读成同一个。
* **requiredFix**：报告里把两个对象集分开写（测试本身没错）。

### RV-C2-7（low · 判据可证伪性，RVC-9 仍开）G8 的「覆盖 ≥90% 非孤立实体」按构造必真
* **事实**：`extraction-gold.rs` 先 `assert_eq!(build.entities_covered, build.non_isolated)`，再断言 `ratio >= 0.90` ⇒ 这条 target 不可能失败（我实测 43/43）。规格 G8 的第一条 metric 因此**没有信息量**。
* **requiredFix**：把结构侧降为**前置条件**（断言 `covered == non_isolated` + 分区数 + `split_by_modularity` + 幂等 + 摘要惰性），G8 的**可失败 target** 落回质量侧三指标（RV-C2-4），或另立一条能被证伪的结构指标（如「最大社区 ≤ N」或「孤立实体比例 ≤ X」）。

### RV-C2-8（low · 同族于 RVC-6(b)）规格还有三条复现命令指向**不存在的测试 target**
* **事实**：规格 §C 的复现命令里，`--test provenance-fresh-db`（G3，L109）、`--test as-of-gold`（G4，L116）、`--test communities`（G8，L144）在仓库里**都不存在**（我全树搜：前两者 0 命中；`communities` 只命中函数名/符号，`crates/graph/tests/communities.rs` 不存在）。实际覆盖率在 `multihop-gold.rs`/`temporal.rs`/`extraction-gold.rs` 里。t27 只给 G7 加了**附录 H** 勘误，这三条没同步。
* **requiredFix**：把附录 H 的勘误范围扩到 G3/G4/G8（指向真实测试 target），或在 §C 三行上各加一行勘误指针。**不改测试**。

### RV-C2-9（low · 一致性登记）RVC-3 的关闭方式与 D4 的字面「失败行绝不创建 episode」不一致
* **事实**：D4 的原文是「失败行**绝不**创建 episode」；mem-core 的 D2(b) 同时要求「不许把 episode 创建挪到 `write_graph` 之后」。两条字面不能同时成立。t27 的落地是**创建后标记**：`episodes 1→1`、`kind='run_turn' 1→0`、`kind="run_turn_failed"` + `meta={voided_by,reason}` ⇒ **徽章契约成立**（`distilled` 只认 `kind='run_turn'`）、来源保留。我的判断：**采纳这个实现**（它同时满足 D2(b) 与 D4 的目的）。
* **requiredFix**：把它写成**一致性裁决**（conformance decision）而不是静默实现：规格 E.10/D4 补一句「失败尝试的 episode 被标记为 `kind='run_turn_failed'`（不删除，保 `memories.source_episode`），`distilled` 布尔只认 `kind='run_turn'`」，并让 mem-core 确认这一条（他给的契约是 D4）。

---

## 5 六条 finding + G7 + R-3 的关闭表（改前 → 我的复现 → 判）

| # | 改前（我 round 1 的读数） | 我的复现（本轮） | 判 |
| --- | --- | --- | --- |
| **RVC-1**(medium) `as_of` 文本比较 | `Z` 形 16/19 判反、`+08:00` 21 条判反；gold 全规范形 ⇒ 对该缺陷盲 | §1.4：3 瞬间 × 3 写法**逐字节相同**（10349/21268/39215 B，非空集 9/21/43）；非时刻 ⇒ `Err(io error: as_of is not an instant …)`；同一跑内联的旧规则仍能变红（27/67、17/67） | **关闭** |
| **RVC-2**(high) G7 关系 P 0.7500<0.80 | `45/60 = 0.7500` | §1.3：**43/47 = 0.9149**（`written 47, duplicate 11, refused 2`；`supported 45→43`），本体 35→29、ad_hoc 6→3 | **关闭**（并在 §2 记下代价：写出边 60→47） |
| **RVC-3**(medium) 失败留 episode | 不变量不成立（`write_memories` 内建 episode → `write_graph` 失败） | §1.5：`episodes 1→1`、`kind=run_turn 1→0`、`kind="run_turn_failed"` + meta、来源保留 | **关闭**（+ RV-C2-9 的一致性登记） |
| **RVC-4**(medium) 不可复现 | 6 次 6 份不同 JSON；并列边在 49/52/53 轮换 | §1.3：`6 runs … byte-identical: true`、`+edge ids distinct 12`、并列组 `[49,52,53,55]`；**我自己的问题 3 次 1 份 blob**（§1.2） | **关闭** |
| **RVC-5**(medium) 箭头写反 | `微信` 4 条路径全反向渲染成正向 | §1.3：`4 paths, 4 reverse hops, all in the stored direction`、40 路径/26 反向跳全对；§1.2：我 16 跳逐跳回查 `(src,dst)` 相等、渲染里两种箭头并存 | **关闭** |
| **RVC-6**(medium) 实体族无登记 + 规格命令指向不存在路径 | `docs/design/reviews/gold/`、`score.py` 不存在；实体族连 not_measured 都没有 | 规格**附录 H**：gold 真实位置 `crates/graph/tests/gold/`、复现方式 `--test extraction-gold`、并说明为何不补 `score.py`；实体族登记 `not_measured` | **关闭**（行为层；归属层见 RV-C2-1；**勘误范围只覆盖 G7** ⇒ RV-C2-8） |
| **RVC-10**(low) `TrueAsOf` 无断言 | `grep TrueAsOf tests/` = 0 | §1.4：`TrueAsOf: 1 of the 1 true-then edges … 1 of 9 rendered lines say true_as_of`，测试 `a_fact_true_then_and_superseded_now_is_reported_as_true_as_of` **ok** | **关闭** |
| **R-3**（captain 追加）`judge_merge` 接线 | 全仓无生产调用者、活库 `op='merge_judged'`=0 | §1.5：`merge_judged rows 0 -> 2 | live=1 superseded=1`；无 embedder 回落词法（`polarity pair … cosine(hash)=0.5000` 一例在读数里） | **关闭** |
| RVC-7/8/9 | 报告数字 / 口径 / 构造性判据 | 未动（§1.3 我重新复现了 G5 的数字；其余按 round 1 读数） | **仍开** ⇒ RV-C2-5/6/7 |

---

## 6 未测 / 不判定

| # | 项 | 状态 | 原因 |
| --- | --- | --- | --- |
| U-1 | 活库上的 `extracted ≥30%`、`entities>0` 生产比例 | **未测** | 需要真实重蒸馏与 api.rs 接线（A-1/A-3） |
| U-2 | G10 `<graph>` 的盘上出现 | **未测** | 生产注入路径在 t19；任务禁止启停 pid 79984 |
| U-3 | G9 的**活库**窗口差值 | **未测** | 活库 18 版；机制已在临时 root 由我复现 |
| U-4 | G2 的 23 条聚合逐腿计数（我本轮只复现了逐腿打印形式） | **部分** | round 1 我在副本上读到 `summary_fts 46`；本轮未重取聚合值（明确标注） |
| U-5 | 我自己的多跳阴性「6 跳不在 hops=3 结果里」的强度 | **弱证据** | 我那次走法 `truncated_by=MaxPaths` ⇒ 缺席与截断不可分（已在 §1.2 写明） |
| U-6 | `clippy -D warnings` 的独立复跑 | **未测** | 不在本单验收命令里（作者自述绿） |

---

## 7 复现命令（我实际跑的）

```powershell
$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"     # 全队包装脚本；未自设 CARGO_TARGET_DIR
scripts/cargo-team.ps1 test -p ruagent-graph                      # 34 passed / 3 ignored，exit 0
scripts/cargo-team.ps1 test -p ruagent-graph --test multihop-gold  -Nocapture
scripts/cargo-team.ps1 test -p ruagent-graph --test extraction-gold -Nocapture
scripts/cargo-team.ps1 test -p ruagent-graph --test temporal -Nocapture
scripts/cargo-team.ps1 test -p ruagent-graph --test resolution -Nocapture
scripts/cargo-team.ps1 test -p ruagent-graph --test seed-resolution -Nocapture
scripts/cargo-team.ps1 test -p ruagent-daemon --lib distill -Nocapture     # 15 passed
# 我自己的多跳问题（导出树 + 独立 target；共享工作树与共享 target 都没碰）
robocopy <repo> %TEMP%\rvg-tree /E /XD target .git node_modules panel\node_modules
#   写入 %TEMP%\rvg-tree\crates\graph\tests\rv-own-multihop.rs（我的种子规则 / 我自己的 BFS / 逐跳回查 / 两次确定性）
cd %TEMP%\rvg-tree; scripts\cargo-team.ps1 test -p ruagent-graph --test rv-own-multihop -TargetDir %TEMP%\ruagent-cmp-rvg -Nocapture
git status --porcelain -- crates panel    # 我唯一新增 docs/design/reviews/gen2-graph-review-r2.md
```

---

## 8 纪律回执

* **只写本单 inScope**：本文件 `docs/design/reviews/gen2-graph-review-r2.md`。仓库的 `crates/`、`panel/`、`docs/` 其他文件**一行未改**；我的探针只存在于 `%TEMP%\rvg-tree`。
* **读数三件套**：对象集（工作树当前字节 + 冻结快照 `live_graph_snapshot.json` 63 实体/67 边）· 采样面（11 个 graph target + 我的自定义问题 + 4 个 as-of 瞬间 × 3 写法 + 6 条 gold）· 可证伪判据（旧文本规则在同一跑内被判反 27/67、17/67；并列边 6 次逐字节相同；我的 BFS 阴性）。
* **期望值只写一处**（规格 §C 与本报告 §2），判据从代码与测试取，**没有复制常量**。
* **未测一律写明**（§6，6 项），其中 A 类（具名 owner=INT/t19 等）**不据此判 I-C 失败**，B 类按契约算未达成并进 findings。
* **真守护进程 pid 79984 未启停**；`~/.ruagent` 只读；**未用** `GET /api/v1/recall`；我自己的探针用了**独立 target dir**，共享 target 只用于契约命令。
* **并发声明**：本单期间队友在途改动（wiki 的 `WikiLead.cite_coverage: f32 → Option<f32>` + `has_anchors`/`anchored_sections`、mem-core 的 `memembed.rs` 适配）**不在我判据的对象里**；我只在读数受影响时报「哪个文件、什么错」，不替同伴改。
