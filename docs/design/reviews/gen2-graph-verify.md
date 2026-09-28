# gen2 知识图谱独立验证报告（V-C · t13）

> 被验对象：I-C（t9）对 `docs/design/reviews/gen2-graph-spec.md` C 节目标的达成情况。
> 本文件只写**我自己跑出来的数**；作者的数是引用的，每条都带「作者读数 → 我的独立读数 → 差异」。
> 验证者：verify（独立验证员），**未改任何被验代码**；inScope 只有本文件。
> 全队编译纪律（captain 2026-09-28）：每一次 cargo 都走 `scripts/cargo-team.ps1`（单一编译 / 共享 target / CPU 0-11 / BelowNormal）。**这是对任务单里 `CARGO_TARGET_DIR=$env:TEMP\ruagent-verify-vc` 一行的有意替换**（captain 明令「不要再自己设 `CARGO_TARGET_DIR`」，因为 5 个并发编译把内存吃光了）：新树读数走脚本的共享 target，旧树对比单独用 `-TargetDir "$env:TEMP\ruagent-cmp-vc"`（closure §7.1，两棵树绝不共享）。两条路径都记在 §6。

## 0 一句话

C 节 10 条目标：**G1/G3/G4/G6/G7（含如实未达标）/G8（结构侧）/G9 的读数我复现到与作者一致**；**G2 的「5 条腿含 0 都出现」成立，但真库 `summary_fts` 计数不一致（我 46 / 作者 39，F-0b）**；**G5 的作者读数整条不可复现**（同 code + 同 gold，我跑三次都得 0.7500/0.8000，作者写 0.7000/0.7500，截断分布与 facts 中位也不同，F-0）；**G10 未接线（not_measured）**，作者已如实登记。
另有一条**冻结接口输入面上的正确性缺陷**（`as_of` 文本比较：同一瞬间的 `Z`/带偏移写法给出不同证据，真实库上 16/67 条边判反，F-1）、一条**断言层弱于声称层**的洞（「失败行绝不创建 episode」只覆盖 `write_memories` 之前的失败，F-2），以及两条对象集/判据口径登记（F-3/F-4）。**没有 blocker / high。**

## 1 对象集 / 采样面 / 时间窗（读数三件套）

| # | 对象集 | 采样面 | 复现命令 | 读数 | 时间点 |
| --- | --- | --- | --- | --- | --- |
| S1 | 活库（只读） | `~/.ruagent/data/ruagent.db`（`mode=ro` URI，只读连接；**从未写入**） | `python %TEMP%\vc-probe1.py` | entities **63**、`entity_edges` **67**（current 60 / superseded 7）、`recall_log` **651**、不同查询 **23**、非孤立实体 **43**；**gen2 表在活库里不存在**（`entity_aliases`/`communities`/`resolution_pending` 全无）⇒ 活库仍是迁移前 schema | 2026-09-28T00:51+08:00；活库文件 mtime **2026-09-27T21:15:21.078+08:00**（整个验证期间未变） |
| S2 | 活库**副本**（我自己的） | `VACUUM INTO` → `%TEMP%\vc-live-probe.db`（14,188,544 B）；`Db::open` 在其上跑迁移 | 见 §4 / §3.7 | 副本上迁移 0019–0023 生效后才出现新列/新表 | 2026-09-28T00:51:48+08:00 |
| S3 | 作者冻结快照 | `crates/graph/tests/gold/live_graph_snapshot.json` | `python %TEMP%\vc-probe1.py` | entities **63** / edges **67**（current **60**）⇒ 与 S1 逐位一致 | 快照文件 mtime 2026-09-27T23:21:53+08:00 |
| S4 | 被验代码树 | 工作树 HEAD `0a39e5b801`（`git archive` 导出为「旧树」，见 §6） | `git status --porcelain` / `Get-FileHash` | 被验文件 SHA256/16 见 §9；**读数的左边界 = 我的哈希快照 2026-09-28T00:49:24+08:00** | 同上 |
| S5 | 真守护进程 | pid 79984 @ 127.0.0.1:8787 | 未启停、未写 `~/.ruagent`、**未调用 `GET /api/v1/recall`** | 只读健康检查 HTTP 200（在 V-C 之前的准备阶段） | — |

**归因（captain F5 纪律，并发波次）**：全树 `git status --porcelain` 本次输出 26 项，其中 `crates/store/**`（t6/recall）、`crates/knowledge/**`（I-A）、`crates/memory/**`（I-B）、`crates/daemon/src/wiki.rs`（I-D）都有同伴在途编辑 ⇒ **只当背景读数记录，不据此写任何 finding**。本单被验对象的归因只用「文件 mtime 落在被验窗口内 + 作者 `changedPaths`」：

| 文件 | mtime（+08:00） | SHA256/16（我核对时） |
| --- | --- | --- |
| `crates/graph/src/retrieve.rs` | 2026-09-27T23:28:06 | `7306185BB6929561` |
| `crates/graph/src/community.rs` | 2026-09-27T23:33:53 | `6F3EA5E9B9A06B3C` |
| `crates/graph/src/lib.rs` | 2026-09-27T23:33:58 | `DD2EBC23B34EC131` |
| `crates/graph/Cargo.toml` | 2026-09-27T23:22:10 | `C624BED10106E31D` |
| `crates/daemon/src/distill.rs` | 2026-09-28T00:08:09 | `A912C17FC05D17E6` |
| `crates/graph/tests/{fixture,multihop-gold,resolution,seed-resolution,temporal,extraction-gold}.rs` | 23:25–23:39 | 见 §9 |
| `crates/graph/tests/gold/*.json`（6 个） | 23:21–23:33 | 见 §9 |
| 作者报告 `gen2-graph-impl.md` | 2026-09-28T00:47:15 | — |

全部被验文件 mtime **早于**我的读数窗口（00:50–01:0x），且与作者 `changedPaths` 一致 ⇒ 本报告读数归因于 I-C。收尾时我会重新哈希一次，若有同伴在被验窗口后改动，将在 §7 注明「核对时哈希」。

## 2 待验目标表（C 节 10 条；期望值只写在规格/规格引用处，探针从函数取）

| 目标 | 规格 target（C 节） | 作者读数（引用 gen2-graph-impl.md） | 我的独立读数 | 差异 |
| --- | --- | --- | --- | --- |
| G1 实体腿不再恒空 | ≥13/23（真实查询帧）；生产端到端 not_measured | 13/23 = 56.5%；生产 not_measured | **13/23**（`old(strict) 1 -> new 13`）；生产端到端确实未接线 | 一致 |
| G2 逐腿归因 | 5 条腿含 0 都出现在证据里 | {exact_name 1, name_token 8, summary_fts 39, alias_table 0, vector_nearest 0} | 5 条腿（含 0）都出现；但 `summary_fts` 我读到 **46**，作者 39（F-0b） | 计数不一致（腿的**形状**一致） |
| G3 每条新边带来源 | 本次抽取的边 100% `source_episode` = 本次 episode | 2/2 边带 `Some(1)` + 16 位 fact_hash | **2/2**，`[("runs_on", Some(1), Some("extracted"), Some("3b86510571381087")), ("uses", Some(1), Some("recorded"), Some("330893524acef598"))]` | 一致 |
| G4 事件时间分离 | 新边 100% 带 `event_time_source`；extracted ≥30% not_measured；as-of gold 6/6 | as-of **6/6**；extracted 比例 not_measured | **6/6 MATCH**（6 条逐条打印）；`extracted=Current / recorded=RecordedAtOnly / old(NULL)=RecordedAtOnly` | 一致 |
| G5 多跳检索 | path-recall@12 ≥0.60（默认 hops=2）；facts 中位 ≥3；每次截断都报 | hop2 **0.7000** / hop3 0.7500；facts 中位 **7**；cuts {Hops 4, MaxPaths 15, None 1} | hop2 **0.7500** / hop3 **0.8000**；facts 中位 **9**（min 3 max 12）；cuts **{None 1, Hops 6, MaxPaths 13}** | **不一致（F-0）**：同 code + 同 gold，**三次**独立复跑都得 0.7500/0.8000 |
| G6 消解与别名 | 24 对 gold 上 P/R；真库冗余 0 | P 1.0000 / R 1.0000（TP7 FP0 FN0 TN16，弃权 5，declared 1）；真库 7→0（5 次合并，边 67 不变） | P **1.0000** / R **1.0000**（同 TP/FP/FN/TN/弃权/declared）；真库冗余对 = **7**（我用**自己的判据**独立枚举，见 §5.3），merge 后 0、边数不变 | 一致 |
| G7 抽取质量 gold | 关系 P ≥0.80 | **0.7500（未达标，已登记）**；ontology 29/35 controlled；unsupported 0 | **0.7500**（45/60）严格、**0.9333**（56/60）容忍重复；duplicate 0.1833、mislabel 0.0667、unsupported **0**；ontology controlled 29 / ad_hoc 6 | 一致（含「未达标」这一判决） |
| G8 社区层 | 覆盖 ≥90% 非孤立实体；三指标判决 | 9 社区、43/43 = 100%、split_by_modularity=1；质量 not_measured | **9 社区、covered 43 / non_isolated 43、split_by_modularity=1**；分区 sizes [14,5,3,3,2,6,2,4,4]；摘要惰性 1/9；三指标仍 not_measured | 一致（但该判据不证伪，见 F-4） |
| G9 distill 三态 | 库行数 = 尝试数；失败带 reason；failed 不建 episode | 三态并存 [(empty,1),(failed,1),(ok,1)]；failed 不覆盖 ok；「库行数=尝试数」到不了（PK=session_key） | **完全同**：三态分别 1 行；`after a failed attempt on a session that had succeeded: ("ok", None, 3)`；`a first-attempt failure: ("failed", Some("Query returned no rows"), Some("hash-b"))` | 一致（但「failed 不建 episode」的断言层弱于声称层，见 F-2） |
| G10 图证据进注入 | transcripts 里出现 `<graph>` 且可回溯 edge_id | not_measured（owner = INT/t19 + mem-core） | **not_measured**：生产注入路径不在 `crates/graph`；我的读数只到 `GraphEvidence::lines()`（一行含 hops/valid_at/state/edges/source，见 §4） | 一致 |

## 3 独立读数表（命令 → 我看到的原始行）

全部命令经 `scripts/cargo-team.ps1`，一次一个编译；`-Nocapture` 由脚本翻译为 `-- --nocapture`。

### 3.1 门禁（contract 的 Verify）

```
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-graph
→ exit=0：lib 3 + empty-recall-pattern 1 + entity-query-shapes 3 + extraction-gold 6 + multihop-gold 5
        + resolution 3 + seed-resolution 5 + temporal 3 = 29 passed / 0 failed；live-after 3 ignored
        （doc-tests 0）。耗时 2.0 s（增量）。
```

### 3.2 G1/G2（`--test seed-resolution -Nocapture`）

```
self-name resolution: old(strict) 63/63 -> new 63/63 legs={ExactName}     ← 无回归
non-empty: old(strict) 1 -> new 13                                        ← G1 target 13/23 达标
（逐查询表：23 行 strict/resolved 计数）
seeds_by_leg for "ruagent": [(ExactName, 1), (AliasTable, 0), (NameToken, 0), (SummaryFts, 11), (VectorNearest, 0)]
```

我在**真实帧**上的逐腿总量（我的探针，§4 第 2 节打印的 `legs_by_count`，两处读数一致）：
```
FRAME n=23 strict_nonempty=1 new_nonempty=13
legs_by_count={"alias_table": 0, "exact_name": 1, "name_token": 8, "summary_fts": 46, "vector_nearest": 0}
```
⇒ 与作者 §1 G2 的 `{exact_name 1, name_token 8, summary_fts 39}` **在 `summary_fts` 上不一致（46 ≠ 39）**；5 条腿都出现（含 0）这一条成立。差异归因见 **F-0b**（我在新副本与「跑过 5 次 merge 的副本」上都是 46，所以不是对象集造成的）。

### 3.3 G5（`--test multihop-gold -Nocapture`）

```
gold reachable n=20 | old(1-hop current_facts) reached 0 | new hops=2 reached 15 | new hops=3 reached 16
truncated_by distribution over the gold seeds: {"None": 1, "Some(Hops)": 6, "Some(MaxPaths)": 13}
facts per query: median 9 min 3 max 12
path-recall@12: hops=2 0.7500 hops=3 0.8000
tight budgets -> paths=2 facts=2 truncated_by=Some(Beam) frontiers=[2, 0]
beyond-cap negatives checked: 5
isolated seed: seeds=1 reason=Some(NoEdgeFromSeeds)   nonce query: seeds=0 reason=Some(NoSeed)
as-of 2000: edges_reachable=60 reason=Some(AsOfBeforeAnyFact)
```
两次独立复跑 + 本表这一次 = **三次**（同一次会话、同 code、同 gold）读数逐位相同 ⇒ 0.7500 稳定，不是闪烁。

### 3.4 G4（`--test temporal -Nocapture`）

```
as_of ruagent at 2026-09-13T18:34:20.791485300+00:00: own 8/8 edges,  12 seeds, orphan edges [] -> MATCH
as_of ruagent at 2026-09-13T18:38:15.350633200+00:00: own 17/17 edges, 12 seeds, orphan edges [] -> MATCH
as_of ruagent at 2026-09-13T18:34:20.792034600+00:00: own 9/9 edges,  12 seeds, orphan edges [] -> MATCH
as_of ruagent at 2026-09-13T18:38:15.351211800+00:00: own 17/17 edges, 12 seeds, orphan edges [] -> MATCH
as_of ruagent at 2026-09-13T18:34:20.792301700+00:00: own 10/10 edges, 12 seeds, orphan edges [] -> MATCH
as_of ruagent at 2026-09-13T18:38:15.350897100+00:00: own 17/17 edges, 12 seeds, orphan edges [] -> MATCH
temporal states: extracted=Some("Current") recorded=Some("RecordedAtOnly") old(null)=Some("RecordedAtOnly")
superseded edge 18 state = Superseded { invalid_at: "2026-09-13T18:38:15.350633200+00:00" }
```

### 3.5 G6（`--test resolution -Nocapture`）

```
resolution gold n=24 | merge decisions: TP=7 FP=0 FN=0 TN=16 | abstain(queue)=5 | declared-alias-only=1 | precision=1.0000 recall=1.0000
BEFORE (norm_name equality): 0/8 true pairs merged
BEFORE: the judge finds 7 redundant pairs（副本）
AFTER: merged 5 pairs, redundant pairs = 0, edges still 67     ← live-after 的同一读数（§3.7）
```
口径说明：`abstain=5` 与 `TN=16` **不是相加关系**（弃权对同时计入 TN）；24 = TP 7 + TN 16 + declared 1。作者报告把两者并列写出，读者容易误算成 28，但判据本身无误（我读了 `resolution.rs:44-67` 的计数逻辑并逐行复算）。

### 3.6 G7/G8 结构侧（`--test extraction-gold -Nocapture`）

```
edges n=60 supported=45 duplicate=11 mislabeled=4 unsupported=0
relation-level precision (strict) = 0.7500 | endpoint-level (duplicates tolerated) = 0.9333 | duplicate rate = 0.1833 | mislabel rate = 0.0667
gold covers 60 current edges and 35 relation names
ontology: 35 relations, controlled=29 ad_hoc=6 (0.1714), singleton-based names=24
level 0: communities=9 covered=43 non_isolated=43 split_by_modularity=1
partition: 9 communities, sizes [14, 5, 3, 3, 2, 6, 2, 4, 4], split_by_modularity=1
level 1 id -> children: [Some(10) × 9]      communities_of(#1) = [(1, 0), (10, 1)]
summaries written: 1 of 9
support(verbatim) = true | support(paraphrase) = false | support(invented) = false
```

### 3.2b 真实帧（活的 23 条不同查询，`live-after` 的三个 `#[ignore]` 读数，**每条测试各用一份新副本**）

```
REAL FRAME n=23 | strict non-empty 1 -> new 13 | queries with >1 seed: 7
seeds by leg (all queries): {"exact_name": 1, "name_token": 8, "summary_fts": 46, "alias_table": 0, "vector_nearest": 0}
（逐查询 23 行 strict/seeds/legs 全部打印）
```

**差异（F-0b）**：作者 §1 G2 写 `summary_fts 39`，我在**新副本**上是 **46**。差异不是随机的，见 F-0b 的仪器实验：`live-after` 的三条测试**共用同一个 `RUAGENT_GRAPH_LIVE_COPY` 文件**，而 `redundant_pairs_...` 会在该文件上合并 5 个实体、`community_coverage_...` 会写社区表；因此**在同一个副本上二次运行**时对象集已经变了（63→58 实体、+5 别名）。我按「一条测试一份新副本」跑，读数才是原始对象集上的数。

### 3.7 活库副本三条（`--test live-after -- --ignored --nocapture`，每条测试各用一份新副本）

```
[the_real_query_frame_through_the_new_legs]     REAL FRAME n=23 | strict non-empty 1 -> new 13 | queries with >1 seed: 7
[redundant_pairs_on_the_real_graph_go_to_zero]  REAL: the judge finds 7 redundant pairs:
      #2/#21 ACP==Agent Client Protocol (ACP)；#2/#43 ACP==Agent Client Protocol；#21/#43；#22/#44 Model Context Protocol (MCP)==Model Context Protocol；
      #27/#42 DeepSeek Harness (dsh)==DeepSeek Harness；#27/#58 DeepSeek Harness (dsh)==dsh；#35/#45 Graphiti / Zep==Graphiti
      REAL: merged 5 pairs -> redundant 0 | entities 58 | edges 67 (unchanged) | aliases 5
[community_coverage_and_multihop_on_the_real_graph]  REAL communities: 9 communities, covered 43/43 non-isolated, split_by_modularity=1
      REAL retrieve(ruagent): seeds=12 paths=12 facts=11 truncated_by=Some(MaxPaths) frontiers=[8, 7]
      first evidence line: ruagent -exposes-> Model Context Protocol (MCP) [hops=1 valid_at=2026-09-13T18:34:20.790281200+00:00 state=recorded_at_only edges=13 source=-]
```

判读：`retrieve("ruagent")` 的默认读数是 12 路径 / 11 条不同事实 / MaxPaths / frontiers=[8,7]，与作者 §1 G5 末行**逐位一致**；真库冗余 7 → 0 与我的独立判据（§5.3）**逐对一致**；社区 9 个、覆盖 43/43 与作者一致。

### 3.8 G3/G4 写侧/G9/C1（`--test -p ruagent-daemon --lib distill -Nocapture`）

```
13 passed / 0 failed / 0 ignored (58 filtered out)
READING confidence(用户可能偏好简体中文。) = 0.4
READING confidence(用户使用双屏显示器。) = 0.8
READING edges: [("runs_on", Some(1), Some("extracted"), Some("3b86510571381087")), ("uses", Some(1), Some("recorded"), Some("330893524acef598"))]
READING after a failed attempt on a session that had succeeded: ("ok", None, 3)
READING a first-attempt failure: ("failed", Some("Query returned no rows"), Some("hash-b"))
READING distill_log by status: [(Some("empty"), 1), (Some("failed"), 1), (Some("ok"), 1)]
READING t350: run_turn row 1 marked=true | manual row 2 marked=false
READING polarity pair: verdict=Refused("a polarity token is in the difference") cosine(hash-embedder)=0.5000
```

## 4 多跳证据形状复核（我自己构造的问题 + 我对每条边的独立复算）

探针：`%TEMP%\vc-probe\newtree\src\evidence.rs`（**我写的**，不在仓库里；只依赖 `ruagent-graph` / `ruagent-store` 的公开 API + `rusqlite`）。对象集 = 我自己的活库副本 `%TEMP%\vc-probe-live2.db`（VACUUM INTO，读源只开 `?mode=ro`）。四个我自选的问题：`"银河麒麟 V10"`（CJK 名）、`"python-build-standalone"`（长连字符名）、`"Podman"`（小连通块）、`"ruagent"`（枢纽）。

**形状：每条路径都自带「怎么连起来的」**（原文摘录，`hops/state/edges/source` 与逐跳关系、`valid_at` 在同一行）：

```
ruagent -exposes-> Model Context Protocol (MCP) [hops=1 valid_at=2026-09-13T18:34:20.790281200+00:00 state=recorded_at_only edges=13 source=-]

question "Podman": seeds=2 paths=6 facts=4 truncated_by=Some(Hops) frontier=[3, 3]
  path score=0.003159 hops=2 nodes=["Podman", "QEMU", "WSL Ubuntu-22.04"]
     e9  Podman -uses-> QEMU            valid_at=2026-09-13T17:06:33.252097500+00:00 invalid_at=None state=RecordedAtOnly src_ep=None session=None rec=…252099500
     e10 WSL Ubuntu-22.04 -uses-> QEMU  valid_at=2026-09-13T17:06:33.252221500+00:00 invalid_at=None state=RecordedAtOnly src_ep=None session=None rec=…252222900
     why: leg=ExactName seed_factor=1.0000 factors=[1.0, 0.058775, 0.053748913] product=0.00315909 score=0.00315909
```

**我在探针里对每条返回边做的独立复算/复核（不是只读一个分数）**：

| 复核项 | 我的做法 | 结果（四个问题上逐条一致） |
| --- | --- | --- |
| 路径几何 | `hops == edges.len() == nodes.len()-1`、无重复节点、`edges[i]` 真连接 `nodes[i]/nodes[i+1]` | `bad_geom=0` |
| 边真的存在且一致 | 用 `edge_id` 回查 SQLite（src/dst/relation/valid_at/invalid_at/created_at/source_episode）逐字段比对证据里的字段 | `bad_rows=0` |
| 分数可复算（D.1 逐因子） | 我自己从库里数 `rel_count`/`degree`，按 `0.5 × 1/(1+ln(1+n_r)) × 1/(1+ln(1+deg_src+deg_dst))` 重算每个因子，再乘起来 | `bad_score=0`；每个因子的 `got` 与 `mine` **逐位相同**（如 `0.113169 / 0.113169`、`0.04760989 / 0.04760989`） |
| 时序 | 我自己根据同行 `invalid_at`/`event_time_source` 独立判 `current / recorded_at_only / superseded`，与 `TemporalStatus` 比对 | `bad_state=0`（**注**：第一次跑出 `bad_state>0` 是我探针自己的字符串比较写错——`Debug` 打的是 `RecordedAtOnly` 而我按 snake_case 比；修掉后归零。这条错记在我头上，不是被验对象的缺陷） |
| 无效/过期边被排除 | 默认走查里是否有 `invalid_at != None` 的边；`include_superseded=true` 时它们是否出现且带 `Superseded` | 默认 **false**（无）；历史模式返回 3 条，全部 `Superseded { invalid_at }`（edge 28 / 48 / 59） |
| 确定性 | 同一问题连跑两次，逐字段比较 seeds/paths/stats | 四个问题全部 `deterministic: true` |
| 预算 | 三组预算（12/24/8、3/2/2、1/1/1） | `paths ≤ max_paths`、`facts ≤ max_facts`、`hops ≤ 2`（默认），且 `truncated_by` 每次都被报出（`MaxPaths`/`Beam`） |
| 来源 | 活库历史行的 `source_episode` 全为 `None`、`session=None` | 证据里如实显示 `src_ep=None session=None`，**没有回填**（G3 的「未知就是未知」在形状上成立） |

**证据形状之外的两位读数**（同样来自探针，用于 §7 的 O-2）：`graph_edges` 在 `as_of` 模式与默认模式下都是 **60**（当前边总数），故事件视角下不能拿它当「那个时点有多少边」。

**形状支持结论吗？** 支持：给我一条路径就能回答「为什么连起来」（逐跳关系 + 两个端点名）、「什么时候为真」（`valid_at` + `state`，并且 `as_of` 模式下会给 `TrueAsOf{valid_at,invalid_at}`）、「谁说的」（`source(session/episode)`，今天为未知时显式 `None`）、以及「为什么只有这些」（`truncated_by` + `frontier_sizes` + `empty_reason`）。唯一与「足以让人判断」有距离的是：`#12 built_on` 这类**关系名与事实文本不一致**的边会被原样端上来（G7 的 mislabeled 就是它），即形状合格、语义质量由 G7 那条读数负责——这正是作者把 G7 单列一条目标的原因。

## 5 抽取 / 消解的独立 gold 复核（不看作者结论，另造判据/另做标注）

### 5.1 作者 gold 的对象集是否真的覆盖
- `edges.json`：**60 条 current 边全部标注**（我按快照逐 id 比对：60/60），标签 45/11/4/0。
- `ontology.json`：**35 个关系名全部标注**（29 controlled / 6 ad_hoc）。
- `resolution.json`：24 对；`same=true` 8 对（7 mechanical + 1 declared）。
- 快照与活库逐位一致（S1 vs S3 的 63/67/60），所以 gold 是对**真库**的读数，不是对玩具图。

### 5.2 我自己的多跳 gold 复核（独立 BFS，Python 标准库，不看作者的分类）
对 `multihop.json` 全部 25 条，我在冻结快照上自建邻接（只走 current 边）做 BFS，逐条比对「gold 的 hops/class/节点序列」：
- 20 条 `reachable`：BFS 距离 = 2 或 3，**与 gold 的 hops 逐条相同**，节点序列逐边都是真实邻接；
- 5 条 `beyond_cap`：BFS 距离 4/5/6，**都 > 3**；
- **独立复算出的不一致条目 = 0**。

### 5.3 我自己的冗余判据（不抄作者的 judge）
我在 63 个实体名上实现了我自己的判据（去一个尾部括号得主干 + 括号内容 + token 子集 + 首字母缩写），得到冗余对 **恰好 7 对**：
`#2 ACP ≈ #21 Agent Client Protocol (ACP)`、`#2 ≈ #43 Agent Client Protocol`、`#21 ≈ #43`、`#22 Model Context Protocol (MCP) ≈ #44 Model Context Protocol`、`#27 DeepSeek Harness (dsh) ≈ #42 DeepSeek Harness`、`#27 ≈ #58 dsh`、`#35 Graphiti / Zep ≈ #45 Graphiti`。
其中 `#2/#21/#43` 是 3-团（需 2 次合并）⇒ **7 对 = 5 次合并**，与作者「合并 5 次 → 0，边数 67 不变」完全吻合。规格 A9 的「4 对」用的是更弱判据（只按空白 token 与括号主干），作者已在 §1 G6 写明这个基线变化，我确认该变化是真的。

### 5.4 我对作者 15 条非 supported 标注的独立重判（人工重读事实文本）
- 4 条 mislabeled：`#3 shares_kernel_with`（文本说的是共享**测试思路**）、`#12 built_on`（文本说的是 ruagent **是** ACP 客户端）、`#45 must_never_drop`（是约束不是关系）、`#47 planned_queryable_store`（是计划不是现状）⇒ **我逐条同意**。
- 11 条 duplicate：逐对读出「伙伴边」的事实文本（`#31↔#61`、`#38↔#16`、`#39↔#14`、`#40↔#15`、`#52/#53/#55↔#49`、`#54/#56↔#50`、`#62↔#14`、`#63↔#15`）⇒ **11/11 确实是同一事实的中英改写或反向重复，我同意**。
- 我反过来找「被标 supported 但应当被降级」的边：**没有找到**；只有一条边界项 `#21 benchmarked_against`（文本是「用户要求面板与后端对齐 Multica 的成熟度」），我判「关系名成立但读数偏宽」，不足以降级。
⇒ 结论：**G7 的 gold 判据在我的独立重判下不需要修改**；P=0.7500 是真实读数，未达标也真实。同时也确认：若把 duplicate 记为「事实为真但冗余」，同一对象集上的宽容读数是 **0.9333**，达标与否完全取决于这条口径（作者**没有**放宽口径，值得记一笔）。

## 6 可证伪性复核（两棵树 + 构建树证明）

**两棵树怎么来的（链接/构建树证明）**

| | 新树（被验） | 旧树（改动前） |
| --- | --- | --- |
| 来源 | 工作树 | `git archive --format=tar -o %TEMP%\vc-old.tar HEAD` ⇒ `%TEMP%\vc-old`；`HEAD = 0a39e5b801ea30cb8189cd6d10c4480ddfeedbfe`（I-C 的全部改动都还是未提交的工作树状态） |
| target | `%TEMP%\ruagent-team-target`（全队共享，captain 2026-09-28 纪律） | `%TEMP%\ruagent-cmp-vc`（**独立**，closure §7.1：两棵树绝不共享 target） |
| 树内容证明 | `crates/graph/src/{lib,retrieve,community}.rs` + `tests/{fixture,multihop-gold,resolution,seed-resolution,temporal,extraction-gold}.rs` + `tests/gold/*` | `crates/graph/src/` **只有 `lib.rs`**；`crates/graph/tests/` **只有** `empty-recall-pattern.rs`、`entity-query-shapes.rs`；`crates/store/src/migrations/` **18 个**（新树 23 个）；`event_time_source` 在旧迁移里 **0 次** |
| 旧树门禁 | `cargo test -p ruagent-graph` = 29 passed / 3 ignored | `cargo test -p ruagent-graph` = **7 passed / 0 failed**（`entity_search` / `bi_temporal_supersession_and_as_of_queries` / `multi_hop_traversal` / `two_char_ascii_queries_degrade_instead_of_erroring` / `query_shape_matrix` / `hyphen_phrase_requires_adjacency` / `cjk_substring_needs_like`）——与规格 A16 的基线逐条一致，且**新判据的测试文件在旧树里根本不存在** |

**新判据在旧代码上失败（同一条命令、同一份活库副本、同一个探针源码）**

探针 `%TEMP%\vc-probe\{newtree,oldtree}\src\oldlegs.rs`（**只用**冻结的 `search_entities` / `current_facts` / `Db`，所以两棵树都能编）：

```
旧树（-TargetDir %TEMP%\ruagent-cmp-vc）：
  OLD G1: strict non-empty 1/23 (new target was >=13)          ← 目标失败
  OLD G5: 0/20 gold reachable pairs (new target was >=12 of 20) ← 目标失败（20 条逐条 false）
新树（同一个源码、同一个二进制入口）：
  OLD G1: 1/23   /  OLD G5: 0/20                                ← 与旧树逐位相同（证明这两条旧腿确实没被改）
新腿（同一次会话、同一份副本）：
  resolve_seeds 13/23（§3.2b REAL FRAME）/ retrieve hops=2 15/20（§3.3）
```

⇒ 结论：G1 的 ≥13/23 与 G5 的 ≥12/20 **在改动前的代码上都是失败的**，且失败是「旧函数本身跑出来」的，不是回忆出来的旧值。这条复核里我另外确认：`search_entities`、`current_facts`、`facts_as_of`、`neighbors`、`list_entities`、`list_edges`、`upsert_entity`、`add_fact`、`delete_entity` 的函数体在 `git diff HEAD -- crates/graph/src/lib.rs` 里的**删除行数 = 1**（只有 `fn norm` → `pub(crate) fn norm` 这一行可见性变更），其余 1009 行都是纯新增（`+` 行长 524 在 `search_entities_loose` 之后）。

### 6.1 我的探针与 #4「多跳证据形状」的差别

§4 的读数来自 `newtree/src/evidence.rs`（**新树**、我自己的问题、我对每条边的独立复算）；§6 的读数来自 `oldlegs.rs`（两棵树都能编的旧腿）。两个探针都不在仓库里（放在 `%TEMP%`），仓库内只新增本报告一个文件（inScope 纪律）。

## 7 findings

**没有 blocker / high。** 被验的 C 节目标里，G1–G4、G6–G9 的读数在我自己的构建树上可复现；G5 的目标（≥0.60）也达标，但作者报告里的**具体数字不可复现**（F-0）。以下按严重度排列，每条都给「我跑出来的证据 → 影响 → 建议的修法」，**我没有替作者改任何代码**。

### F-0（medium · 报告准确性）G5 的作者读数不可复现，且我复跑三次都稳定在另一个数

- 证据（同 code、同 gold、同 fixture，我跑三次全同）：`path-recall@12: hops=2 0.7500 hops=3 0.8000`、`facts per query: median 9 min 3 max 12`、`truncated_by {... "Some(Hops)": 6, "Some(MaxPaths)": 13}`。
- 作者 §1 G5 写：`0.7000 / 0.7500`、median `7`、cuts `{Hops 4, MaxPaths 15, None 1}`。
- 影响：目标（≥0.60）两版都达标，**判决不变**；但「读数必须可复现」这条纪律下，作者报告里的 G5 行不能用——下游（RV-C / repair）若拿 0.70 当基线，会把一个并不存在的差距当成待修项。（方向上是作者**低估**了自己的实现。）
- 我的归因：被验文件 mtime 全部早于我的读数窗口（§1），`store/src/fts.rs`（22:58）也早于作者报告（00:47），所以**不是并发波次造成的漂移**；最可能是报告引用了 `retrieve.rs` 23:28 定稿之前那一次运行的数字（`multihop-gold.rs` 23:25、gold 23:21，均早于 23:28）。
- 建议：RV-C 以我的 0.7500/0.8000（或让作者重跑一次）为准更新报告；不要动 target。

### F-0b（low · 对象集纪律）真库逐腿计数 `summary_fts`：作者 39，我 **46**（新副本与改写过的副本都是 46）

- 证据（三条读数，同一命令）：**新副本** → `{"exact_name": 1, "name_token": 8, "summary_fts": 46}`；**复用**「跑过 5 次 `merge_entities` 的副本」（63→58 实体、+5 别名）→ **同样 46**。
- 我原本的假设（作者复用了同一个副本、对象集已被合并）**被这次实验否掉**：合并后的副本也给我 46。⇒ 39 与 46 的差异**不是**对象集造成的，和 F-0 同族（作者引用了更早一次运行的数）。
- 仍然值得记下的纪律（对下一个人有用）：`live-after` 的三条测试**共用一个 `RUAGENT_GRAPH_LIVE_COPY` 文件**，其中 `redundant_pairs_...` 会真合并、`community_coverage_...` 会真写社区表；在同一副本上第二次运行，读到的就不是原始图（实体 63→58、别名 0→5、社区表非空）。本单的做法是**一条测试一份新副本**（§3.7 的三行都写明）。
- 建议：`live-after.rs` 每条测试自带 `VACUUM INTO`，或把环境变量改成「目录 + 测试名」；报告里注明每条读数的副本来源。

### F-1（medium · 正确性，冻结接口之外）`as_of` 用**字符串比较**判时间：同一瞬间的不同 RFC3339 写法给出不同证据

- 位置：`crates/graph/src/retrieve.rs`（`temporal_status` 里 `valid_at <= t && invalid_at > t`，以及 `retrieve` 的 `true_then` 过滤）。
- 证据（真实行，`vc-live-probe.db` 的 67 条边，我用 Python 复刻同一条规则并与**瞬间算术**逐边对比）：

| as_of（三者是**同一个瞬间** 2026-09-13T18:38:15.000Z） | 代码判「当时为真」的边数 | 正确边数 | 对称差 |
| --- | --- | --- | --- |
| `2026-09-13T18:38:15.000000000+00:00`（规范形） | 30 | 30 | **0** |
| `2026-09-13T18:38:15Z`（`Z` 形） | 38 | 30 | **16 条**（漏 4 条真边 18/19/20/28，多 12 条当时还不成立的边 33–42…） |
| `2026-09-14T02:38:15+08:00`（带偏移） | 43 | 30 | **21 条** |

  逐边最小例：edge 18 `invalid_at=2026-09-13T18:38:15.350633200+00:00`，`as_of=…18:38:15Z` ⇒ 代码 `"." < "Z"` 判 `invalid_at <= as_of`，把一条**当时为真**的边判成已失效；同时 edge 33（`valid_at=…18:38:15.350633200+00:00`）被判定「当时已成立」。
- 影响：`GraphQuery::as_of` 是 D 节冻结签名（`Option<String>`，RFC3339），而 DEP-4 计划把它接到 HTTP `GET /api/v1/graph/retrieve?…&as_of=…`（owner=INT/t19）。HTTP 查询串里最自然的写法就是 `…Z`。⇒ 上线后同一瞬间的不同写法会给出不同证据，且**静默**（不报错，只是少/多证据）。
- 为什么作者的 gold 抓不到：`tests/gold/as_of.json` 的 6 条查询**全部**是「纳秒 + `+00:00`」的规范形（我从文件里逐条读出），恰好是文本比较唯一正确的形态 ⇒ 该 gold 对这条缺陷是盲的。
- 继承性说明（对作者公平）：旧的冻结函数 `facts_as_of` 在 SQL 里也是字符串比较（`valid_at <= ?2 AND (invalid_at IS NULL OR invalid_at > ?2)`），所以这不是 I-C 引入的**回归**，而是 I-C 把它带到了新的冻结入口上并写进了「V-C 逐因子复算」的那条路径。
- 建议修法（不替作者改）：入口处把 `as_of` 归一化（解析成 `chrono::DateTime<Utc>` 再比较，或统一 `to_rfc3339_opts(SecondsFormat::Nanos)` 规范化后比较），并给 gold 补 3 条**同瞬间不同写法**的查询（`Z` / 带偏移 / 无小数）作为负例。

### F-2（low · 断言层弱于声称层）「失败行绝不创建 episode」只对 `write_memories` **之前**的失败成立

- 声称：作者 §1 G9 / 规格 E.10 D4「失败行**绝不**创建 episode」。
- 我读到的顺序（`crates/daemon/src/distill.rs`）：`distill_once` → `ask_agent` → `parse_extraction` → `write_memories()`（**episode 在这里创建**，line 507-527）→ `write_graph()`（line 273）→ 返回；任何一步 `Err` 都由 `distill()` 的 `Err` 分支写成 `status='failed'`（line 216-243）。⇒ `write_graph` 或 `write_memories` 内部后半段失败时，**episode 已经存在**，而 `distill_log` 是 `failed`。
- 测试的实际覆盖面：`the_log_records_three_states_and_a_failure_keeps_a_success` 直接调 `log_outcome(...)` 断言「episode 数不变」（line 1208-1213），**没有**让 `distill()` 在 `write_memories` 之后失败。⇒ 断言为真，但断言的对象是日志写入器，不是被声称的不变量。
- 影响：低（徽章方向是「写了记忆 + 记了失败」，不会把「什么都没做」冒充成功；即使 `write_graph` 失败，memories 也确实写进去了）。但**「绝不」这个词目前没有覆盖完整的失败面**，且这条不变量是 mem-core 的 D4 契约。
- 另注：mem-core 的 D2(b) 明确要求「不许把 episode 创建挪到 `write_graph` 之后」，因此 D2(b) 与 D4 存在张力；要在 `write_graph` 失败时保住 D4，只能在 Err 路径上回滚/删除本次 episode，或把 D4 的措辞收窄成「在任何写入发生前的失败不创建 episode」。
- 建议：把措辞与测试对齐（二选一），并给 `distill()` 加一条「`write_graph` 失败后 episode/日志 的一致性」测试。

### F-3（low · 对象集口径）`24/35 单例` 与 gold 的 60 条边不是同一个对象集

- 我的独立计数：**全部 67 条边**上 35 个关系名、**24 个只出现一次**；**当前 60 条边**上同样 35 个名字、但**26 个只出现一次**（多出 `owns`、`runs_on`）。
- 作者 §1 G7 写「本体覆盖不足 24/35 单例 + 6/35 ad_hoc」，紧邻它的 gold（`edges.json`）是**60 条 current 边**的对象集 ⇒ 同一段里两个对象集。测试本身没错（`extraction-gold.rs` 用 `snapshot.edges` 全 67 条，并把它标成「the measurement the spec quotes」），错的是报告把两个数并排放。
- 影响：低（结论方向不变：68.6% vs 74.3% 的「一次性关系名」都很高）。建议在报告里把两个对象集分开写。

### F-4（low · 判据可证伪性）G8 的「覆盖 ≥90% 非孤立实体」按构造必然成立

- 证据：`build_communities` 把**每一个**非孤立实体放进某个社区（`crates/graph/src/community.rs:200-231`），测试自己还断言 `assert_eq!(build.entities_covered, build.non_isolated)` ⇒ 覆盖率恒为 1.0000，`≥0.90` 这条 target 不可能失败；我实测 43/43。
- 影响：低。G8 真正有信息量的读数是「分区数/大小/`split_by_modularity`（=1）」「重建幂等」「摘要惰性」，而**质量侧（GraphRAG 三指标）作者已如实记 not_measured**。建议把 G8 的 target 改成「质量侧成对判决胜率」（已有协议 B2），把结构侧降为前置条件。

### 观察（不进 findings，留给评审裁决）

- O-1 `SeedBucket::offer` 的同腿比较方向是**升序**（`crates/graph/src/retrieve.rs:341`：`(leg, f32_key(score)) < (old.leg, f32_key(old.score))`），而文档说「on a tie keep the higher score」。当前每条腿传入的 score 都是该腿常量，所以**今天无影响**；一旦某条腿开始传真实分（如 E10 的向量腿归一化分），同一腿内会保留**较低**分。建议改成 `>` 或补一条同腿不同分的测试。
- O-2 `RetrievalStats.graph_edges` 的文档说「the whole graph at this time view」，实现给的是**当前边总数**（`index.total_current`，与 `as_of` 无关）。今天只是文档与语义不严，读数使用者需知道它不能用来判断「那个时点有多少边」。
- O-3 `resolution` 报告的计数口径：`abstain(queue)=5` 与 `TN=16` 是**重叠**计数（弃权的负例既算弃权也算 TN），24 = TP7 + TN16 + declared1。作者把两者并列写，容易读成 28；建议加一句口径。
- O-4 G8 的 `community_coverage` 用 `COUNT(*)` 统计 `community_entities`；同层重复归属会把它算大。测试断言了「同层不重复」，所以今天成立；读侧若被别处复用需注意。
- O-5 我独立重判 60 条边时，只有 `#21 ruagent -benchmarked_against-> Multica`（文本是「用户要求面板与后端对齐 Multica 的成熟度」）处于「关系名成立但读数偏宽」的边界；**不足以降级**，写在这里供 RV-C 判断判据是否要收紧（若收紧，`supported` 会变 44/60 = 0.7333，**判定不变**）。

## 8 未测项（写明原因，不静默跳过）

| 项 | 状态 | 原因（谁才能测） |
| --- | --- | --- |
| G1 生产端到端（召回响应里的实体腿） | **not_measured** | DEP-1 未接线：`api.rs` 的召回实体腿仍是 `search_entities`（我读代码确认 `ruagent_graph::` 的调用点仍只有图页面端点与 `distill.rs`）；owner = INT/t19。我的读数只到腿级 13/23（两条路径都测了：冻结帧 13/23、真帧 13/23）。 |
| G4 `extracted ≥30%`（活库） | **not_measured** | 需要一次真实重蒸馏（agent 调用）。我在活库副本上看到的是 `event_time_source` 全 NULL（迁移后不回填）；写侧语义由 `--lib distill` 的 3 条读数覆盖。 |
| G7 关系 recall / 幻觉率（活库） | **not_measured** | 0/67 历史边有 `source_episode` ⇒ 没有 transcript 可比对；机制在 fixture 上被测（verbatim true / paraphrase false / invented false），且**词法检查分不清改写与发明**这一点被测试自己记下来。 |
| G8 质量侧三指标（comprehensiveness/diversity/empowerment） | **not_measured** | 需要一次 LLM 成对判决跑批；结构侧我测到 9 社区 / 43-43 / split=1 / 幂等 / 摘要惰性。 |
| G9 「库行数 = 日志尝试数」 | **到不了（已登记）** | `distill_log` PK = `session_key`（`0008_memory_embeddings.sql`）；作者选择「失败不覆盖成功」。按次计数需要 I-SCHEMA-2 的表，不在本单。 |
| G10 `<graph>` 进注入 | **not_measured** | 产生点在 `api.rs`/`runs.rs`（INT/t19）与 `memory::inject` 的 tag 词表（mem-core）。我**没有**启停真守护进程、没有写 `~/.ruagent`、也没有用 `GET /api/v1/recall` 取任何读数（任务禁止：它会写 `recall_log`）。 |
| E10 向量种子腿 | **未实现（如实登记）** | graph 没有嵌入器；`VectorNearest` 恒为 0 且**出现在 `seeds_by_leg` 里**（我逐数字确认 0，不是消失）。 |
| E5 `entity_sources` 表 | **不做** | t6 未建（无消费者）；我在迁移后的副本上确认该表不存在，而 G3 由 `entity_edges.source_episode` 满足。 |
| 「不破坏产品」重放（面板旧端点） | **未测** | 图页面 4 个端点在 `api.rs`（不在本单 inScope，也无 HTTP 读数需求）；我用 `git diff` 证明冻结函数的函数体零改动（§6.1）替代了行为重放，但这**不是**端到端重放。 |
| 多人并发下的读数稳定性 | **部分测** | 我在并发波次里跑（期间 knowledge/mock-agent 都在构建），同一命令三次读数逐位相同（§3.3）；但「并发写入活库时 as_of/检索的稳定性」这一类需要真守护进程，未测。 |

## 9 附录：读数用的文件哈希（收尾复核）

**收尾复核（2026-09-28T01:03:17+08:00）**：`crates/graph/**` 全部 20 个被验文件的 mtime 与 SHA256/16 与 §1 的核对时**逐位相同**（`retrieve.rs 7306185BB6929561`、`community.rs 6F3EA5E9B9A06B3C`、`lib.rs DD2EBC23B34EC131`、`Cargo.toml C624BED10106E31D`、`tests/*` 与 `tests/gold/*` 全同）⇒ §2–§6 的图读数绑定在同一份字节上。活库 mtime 仍是 **2026-09-27T21:15:21.078+08:00**（14,946,304 B）⇒ 我的整个验证过程**没有写活库**（只读 `?mode=ro` 抓副本）。

**一处被验窗口后的他人改动（必须声明，按 captain F5 纪律）**：

| 文件 | 我读数时（00:49:24 快照） | 收尾核对时（01:03:17） | 影响 |
| --- | --- | --- | --- |
| `crates/daemon/src/distill.rs` | mtime **00:08:09**、SHA256/16 **`A912C17FC05D17E6`** | mtime **01:02:25**、SHA256/16 **`86307A99826E4E1A`** | §3.8/G3/G4 写侧/G9/C1 的读数属于 `A912C17F` 那一版；**01:02:25 之后有人改了它**（同批还新增了 `crates/store/src/migrations/0024_distill_attempts.sql` 与 `crates/daemon/tests/injection_e2e.rs` 的改动），我没有对新的字节重跑，也不对它们负责（那是别人的在途单）。F-2 的结论按 `A912C17F` 的字节成立。 |
| `crates/graph/**` | — | 无变化 | — |

**背景读数（只作背景，不写 finding）**：收尾时 `git status --porcelain -- crates panel` = **47 项**（crates/store 6、crates/knowledge 8、crates/memory 7、crates/daemon 5、crates/graph 11、其余为 mock-agent/panel 等），其中新增的 `0024_distill_attempts.sql` 与 `distill.rs` 的 01:02 改动都不是我的产物、也不是被验对象。**这是并发波次**：本单从不拿全树 porcelain 当越界证据（归因只用 mtime + 作者 `changedPaths`，见 §1）。

**收尾哈希全文**（`<相对路径>|<mtime ISO>|<SHA256前16>`，2026-09-28T01:03:17+08:00）：

```
crates/daemon/src/distill.rs|2026-09-28T01:02:25.0075220+08:00|86307A99826E4E1A   ← 读数时是 00:08:09 / A912C17FC05D17E6
crates/graph/Cargo.toml|2026-09-27T23:22:10.1555607+08:00|C624BED10106E31D
crates/graph/src/community.rs|2026-09-27T23:33:53.9812757+08:00|6F3EA5E9B9A06B3C
crates/graph/src/lib.rs|2026-09-27T23:33:58.3864739+08:00|DD2EBC23B34EC131
crates/graph/src/retrieve.rs|2026-09-27T23:28:06.3752177+08:00|7306185BB6929561
crates/graph/tests/empty-recall-pattern.rs|2026-09-26T22:49:09.7315184+08:00|1F49AC5BFCDD80E5
crates/graph/tests/entity-query-shapes.rs|2026-09-26T18:56:56.4076766+08:00|9DC62C282D92F1D6
crates/graph/tests/extraction-gold.rs|2026-09-27T23:32:37.4256814+08:00|BEAC9FF333F03E20
crates/graph/tests/fixture.rs|2026-09-27T23:25:04.6001193+08:00|6E4584294150BAC4
crates/graph/tests/gold/as_of.json|2026-09-27T23:27:42.2949728+08:00|5B9FCD2346FC5FFC
crates/graph/tests/gold/edges.json|2026-09-27T23:33:31.2647237+08:00|7E4550C42FEFA81F
crates/graph/tests/gold/live_graph_snapshot.json|2026-09-27T23:21:53.5397871+08:00|3A8315953DFB4AF2
crates/graph/tests/gold/multihop.json|2026-09-27T23:21:53.5417873+08:00|F42A8C77836A3644
crates/graph/tests/gold/ontology.json|2026-09-27T23:33:31.2637253+08:00|908290045ACEA5F1
crates/graph/tests/gold/resolution.json|2026-09-27T23:33:31.2627171+08:00|89328403FD782FC0
crates/graph/tests/live-after.rs|2026-09-28T00:08:13.1097616+08:00|16694D19713D21EC
crates/graph/tests/multihop-gold.rs|2026-09-27T23:25:04.7148971+08:00|67BA5B3A41451B2D
crates/graph/tests/resolution.rs|2026-09-27T23:33:27.4550340+08:00|684D26DF615638AD
crates/graph/tests/seed-resolution.rs|2026-09-27T23:39:43.4601367+08:00|A8A05D92A16212F2
crates/graph/tests/temporal.rs|2026-09-27T23:38:09.9326924+08:00|353EE6B6B468ED1D
```

**复现命令汇总**（都能直接跑；`-Nocapture` 与 `-TargetDir` 是 `scripts/cargo-team.ps1` 的开关）：

```powershell
$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"
# 门禁
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-graph
# 逐目标带读数
... test -p ruagent-graph --test seed-resolution -Nocapture      # G1/G2
... test -p ruagent-graph --test multihop-gold -Nocapture        # G5
... test -p ruagent-graph --test temporal        -Nocapture      # G4
... test -p ruagent-graph --test resolution      -Nocapture      # G6
... test -p ruagent-graph --test extraction-gold -Nocapture      # G7/G8 结构
... test -p ruagent-daemon --lib distill         -Nocapture      # G3/G9/C1
# 活库副本（每条测试一份新副本；注意 --nocapture 要写在 `--` 列表里）
python -c "import sqlite3,os;con=sqlite3.connect('file:'+os.path.expanduser('~/.ruagent/data/ruagent.db').replace('\\','/')+'?mode=ro',uri=True);con.execute('VACUUM INTO ?',(os.path.join(os.environ['TEMP'],'vc-x.db'),));con.close()"
$env:RUAGENT_GRAPH_LIVE_COPY="$env:TEMP\vc-x.db"
... test -p ruagent-graph --test live-after -- --ignored --nocapture --test-threads=1 <测试名>
# 两棵树（旧树 = git archive HEAD；独立 target dir）
git archive --format=tar -o $env:TEMP\vc-old.tar HEAD; mkdir $env:TEMP\vc-old; tar -xf $env:TEMP\vc-old.tar -C $env:TEMP\vc-old
cd $env:TEMP\vc-old; ... -File <repo>\scripts\cargo-team.ps1 test -p ruagent-graph -TargetDir "$env:TEMP\ruagent-cmp-vc"
# 我的探针（在 %TEMP%，不在仓库里）
$env:VC_LIVE="$env:TEMP\vc-probe-live2.db"
... -File scripts\cargo-team.ps1 run --manifest-path "$env:TEMP\vc-probe\newtree\Cargo.toml" --bin evidence
... -File scripts\cargo-team.ps1 run --manifest-path "$env:TEMP\vc-probe\newtree\Cargo.toml" --bin oldlegs
... -File <repo>\scripts\cargo-team.ps1 run --manifest-path "$env:TEMP\vc-probe\oldtree\Cargo.toml" --bin oldlegs -TargetDir "$env:TEMP\ruagent-cmp-vc"
# 背景
git status --porcelain -- crates panel
```

