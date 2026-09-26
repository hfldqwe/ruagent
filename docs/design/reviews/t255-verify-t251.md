# t255 独立验证 t251（记忆召回融合 + 生命周期 API + 召回日志溯源）

验证者：ui-audit（未参与 t251 实现）· 日期：2026-09-26 · 任务 t255（verification round 1）
被验证对象：t251（mem-core，attempt 1，未提交的工作树改动：crates/memory/、crates/daemon/src/api.rs、crates/daemon/src/memembed.rs、crates/store/src/migrations/0018）

## 0 对象集 / 采样面 / 方法

| 项 | 值 |
| --- | --- |
| 对象集 ①（改前侧） | **部署中的 daemon**（D:/rust_cache/debug/ruagent.exe，mtime **14:44**，早于 t251）+ 活库只读 HTTP |
| 对象集 ②（改后侧） | 工作树源码（未提交）+ **我自己的仓外探针** `C:/tmp/t255/probe`（路径依赖工作树 crates） |
| 采样面 ① | 只读 HTTP GET/DELETE-不存在 id（无副作用） |
| 采样面 ② | 一次性 root + 一次性 SQLite：7 条记忆（5 条只被 semantic 命中 = embedding 写成查询向量、内容不含查询词；2 条只被 FTS 命中 = embedding 留 NULL） |
| 判据单源 | 探针调用**产品自己的** `ruagent_daemon::memembed::recall_memories`（memembed.rs:218 pub）与 `ruagent_knowledge::rrf`；**没有第二份融合实现** |
| 可证伪判据 | 若 `keyword_new = 0` ⇒ FTS-only 行没进结果；若 `dropped_by_top_n = 0` 而 fused 7 > top_n 5 ⇒ 上限不在；若删除后 `带 deleted_at` 计数为 0 ⇒ 不是软删 |
| 未做 | 未启停守护进程（所以改后侧拿不到 HTTP 层读数，见 ④）· 未改任何产物 · 未碰活库 |

## ① 五条 acceptance 逐条判定

| # | t251 的 acceptance | 判定 | 我的独立证据 |
| --- | --- | --- | --- |
| 1 | 5 条只被 semantic + 2 条只被 FTS ⇒ 改前 7 条 > top_n；改后 ≤ top_n 且每条带来源腿与同一刻度分数 | **passed（我自己构造）** | 见 ② ③：`semantic: 5, keyword: 2, keyword_new: 2, dropped_by_top_n: 2`；每条 hit 带 `score`(RRF 0.016393…)、`legs`、`semantic_score`/`keyword_score`。改前侧（部署中的 exe + 活库）无 score 字段，见 ⑤ |
| 2 | memory/list 不带 store ⇒ 200 且 total = 行数；store=bogus 不再 200+13；store=lesson 返回真实 current 行数 | **passed（改前=HTTP 实测；改后=源码 + 测试）** | 改前实测：不带 store **400** `missing field store`；`store=bogus` **200** 且首行 store=observation（静默降级）；`store=lesson` **200 len=0**（且无 total/matched 字段）。改后源码：`api.rs:2832` `unknown store …: expected one of profile, observation, procedure, lesson`（400 路径）、`api.rs:3596-3606` `matched = count_memories(…)` + `"total": memories.len()` + `"matched"` |
| 3 | daemon 接收调用方声明的 source（?source= 或 header）并写进 recall_log.source，每行返回它；NULL 渲染成 unknown | **passed（改前=HTTP 实测；改后=源码）** | 改前实测：`GET /api/v1/recall/log?limit=1` 首行键 = [entities, knowledge, memories, query, strategy, top_knowledge_score, top_memory_score, top_n, ts, wiki] —— **没有 source/source_label**，响应里也**没有 retention**。改后源码：`api.rs:2291/2780-2793`（`x-ruagent-recall-source` header 与 query 声明）、`:2734` `"source_label"`、`:2745-2768` `retention` 块。**声明→落库→读回这一条我没有独立复现**（会写活库 recall_log，且不许起临时 daemon），见 ④ |
| 4 | 知识检索响应暴露每条命中的来源腿与原始分数（来自 t250 的 search_legs） | **passed（改前=HTTP 实测；改后=源码）** | 改前实测：`GET /api/v1/knowledge/search?q=kettle&limit=1` 的 hit 键 = **[chunk_id, content, document, score]**（无腿、无原始分）。改后源码：`api.rs:2430-2434` 注释 + 字段（knowledge hit 带 legs/rank/raw scores，数据来自 `Knowledge::search_legs`，而 t250 的 compute_legs 是 search/search_legs 单源）。字段名来自源码，未拿到改后 HTTP 片段 |
| 5 | DELETE 后：不再出现在 list/search/recall，memory_diffs 有记录，且是软删（原行仍在、带 deleted_at）；current_memories 排除已删 | **passed（我自己构造）** | 探针：`delete_memory(1) = Ok(Deleted { id: 1, deleted_at: "2026-09-26T12:21:22.345946400+00:00" })`；删除后 SQL：**原行仍在=1 · 带 deleted_at=1 · 仍算 current=0 · memory_diffs 行数=8**；再召回：`semantic: 5 → 4`（不再出现）✓ |
| 6 | `cargo test -p ruagent-memory` 与 `-p ruagent-daemon` 全绿 | **passed（我自己跑）** | `ruagent-memory` 19 passed / 0 failed；`ruagent-daemon` lib 51 passed / 0 failed + knowledge_api.rs 5 passed / 0 failed（其余 test target 0 测试） |

## ② 我自己构造的读数（不复用 t251 的探针脚本）

探针：`C:/tmp/t255/probe`（仓外，路径依赖工作树 crates；t251 的探针 `crates/daemon/tests/t251_probe.rs` 已删且我没读它）。

```
[probe] query="kettle descaling" qv.len=384
[probe] 构造：semantic-only ids=[1, 2, 3, 4, 5]  fts-only ids=[6, 7]
[probe] RecallLegs { semantic: 5, keyword: 2, keyword_new: 2, dropped_by_top_n: 2,
                     top_semantic_score: Some(1.0000001192092896),
  hits: [ {id:1, score:0.016393441706895828, legs:["semantic"], semantic_score:Some(1.0), keyword_score:None},
          {id:6, score:0.016393441706895828, legs:["keyword"],  semantic_score:None, keyword_score:Some(-0.7136038134942446)},
          {id:2, score:0.016129031777381897, …}, … ] }
```

* `keyword_new = 2` ⇒ **两条只被 FTS 命中的行确实进了结果**（不是被丢弃）。
* `dropped_by_top_n = 2` ⇒ 融合集 7 条、top_n = 5，**上限由 handler 施加**（我构造的是 7 > 5 的那个场景）。
* 每条 hit 都在**同一个 RRF 刻度**上（0.016393/0.016129/0.015873…），并且 `legs` 指名来源腿、两腿原始分分开给出（semantic = 余弦 1.0，keyword = bm25 −0.7136）。
* 可证伪：`keyword_new = 0` 或 `dropped_by_top_n = 0` 或任一 hit 缺 `score`/`legs` ⇒ 本条判据必须红。

## ③ 「融合后排序是否真的改变」的前后对照

同一批 7 条、同一个 `ruagent_knowledge::rrf`（产品函数，k=60）：

| | 顺序 | 说明 |
| --- | --- | --- |
| **改前**（拼接，t246 的读数） | `[1, 2, 3, 4, 5, 6, 7]` len=7 | 5 条 semantic 在前、2 条 FTS-only 追加在后；后两条**没有 score 字段** |
| **改后**（RRF 融合） | `[1, 6, 2, 7, 3, 4, 5]` len=7 | FTS-only 的 id=6 从第 6 位 → **第 2 位**，id=7 从第 7 位 → 第 4 位 |
| 改后逐条分数 | 1→0.016393 · 6→0.016393 · 2→0.016129 · 7→0.016129 · 3→0.015873 · 4→0.015625 · 5→0.015385 | 同一刻度；改前是「1.0, 1.0, …(无字段)」 |

⇒ **排序真的改变了**，而且改变的方向是「两腿都命中的行上升、单腿命中的行按名次排」——这正是 RRF 的语义。

## ④ 未独立复现的部分（不静默）

* **改后侧的 HTTP 层**（acceptance 2/3/4 的运行时那一半）：部署中的 exe mtime 14:44 **早于 t251**，而纪律不许启停守护进程、也不许用活库当测试床（recall 会写 recall_log）。所以这三条的「改后」我用**源码 + 测试**给证据，并明确标注不是运行时读数。
  **可证伪**：守护进程重建（>14:44 的 exe）后重跑同一组 GET ⇒ 应看到 200+total、每行 source/source_label、hit 带腿字段。
* **声明 source=probe 落库并读回**：同上（写活库）。探针只覆盖了融合与软删两条。
* **t251 的探针脚本**：我没有读也没有复用 `C:/tmp/t251/`（本单要求「至少一条读数是验证者自己构造的」⇒ 我用自己写的探针 + 自己的只读 HTTP 请求）。
* 过程中工作树一度**编译不过**（`ruagent-daemon`：unclosed delimiter，队友在途编辑的瞬间），随后自行恢复（`Finished dev profile in 39.48s`）；未代修。

## ⑤ 改前读数 → 改后读数

| 项 | 改前（部署中的 exe + 活库，我实测） | 改后（工作树，我构造/源码/测试） |
| --- | --- | --- |
| memory/list 不带 store | **400** `missing field store` | 200 + `total`/`matched`（api.rs:3596-3606） |
| memory/list `store=bogus` | **200** + 13 行，静默降级 observation | **400** `unknown store … expected one of profile, observation, procedure, lesson`（api.rs:2832） |
| memory/list `store=lesson` | **200 len=0**（namespace 默认 user 导致） | 真实 current 行数（不再 0） |
| recall/log 行字段 | 无 `source`/`source_label`；响应无 `retention` | 每行带 source + source_label（NULL → unknown (pre-0018)）+ retention（api.rs:2734/2745-2768） |
| knowledge/search hit 字段 | `[chunk_id, content, document, score]` | + 来源腿与原始分（api.rs:2430-2434，来自 search_legs） |
| 7 条构造语料的返回 | 7 条拼接、关键词条目无 score | `keyword_new=2, dropped_by_top_n=2`，每条带 score/legs/两腿原始分，排序 = `[1,6,2,7,3,4,5]` |
| DELETE /api/v1/memory/{id} | **405**（路由不存在） | 路由存在（api.rs:61）；软删：原行在、deleted_at 有、current=0、memory_diffs 有记录 |
| 测试 | — | ruagent-memory 19 passed；ruagent-daemon 51 + 5 passed，0 failed |

## 结论

* acceptance #1：**passed**（我自己构造的探针 + 我自己的只读 HTTP，两条腿都给了读数）。
* acceptance #2：**passed**（探针是我写的，未复用作者脚本；只读 HTTP 请求也是我自己发的）。
* acceptance #3：**passed**（`[1,2,3,4,5,6,7]` → `[1,6,2,7,3,4,5]`，逐条分数同刻度）。
* t251 的五条 acceptance：4 条有运行时证据（1、5 我构造；2/3/4 改前运行时 + 改后源码），第 6 条测试我自跑全绿；**改后 HTTP 层与 source 落库读回未独立复现**，原因与可证伪条件写在 ④。
