# t256 独立验证：t250 实体腿与知识腿查询构造修复

验证者：ui-audit（独立于作者）· 日期：2026-09-26 · 任务 t256（verification round 1）
被验证对象：提交 **2c2a750**「feat(retrieval): 实体腿回退构造 + 逐腿原始分数（t250）」

## 0. 对象集与采样面

| 项 | 值 |
| --- | --- |
| 改前树 | 提交 `2e1d7ed`（t250 的父提交），用 `git archive` 解到 `C:/tmp/t256/before` |
| 改后树 | 提交 `2c2a750`（t250），解到 `C:/tmp/t256/after` |
| 仪器 | `crates/knowledge/tests/retrieval-quality.rs`，两棵树**逐字节相同**（md5 `284067fb31117000308bbac60356fc2a`）⇒ 任何数字差异只能来自产物 |
| 实体腿仪器 | `crates/graph/tests/entity-query-shapes.rs`（11 查询 × 9 实体，内存 SQLite + 与活库相同的 FTS 触发器） |
| 逐腿仪器 | `crates/knowledge/tests/retrieval-legs.rs`（3 测试）· `crates/store/src/fts.rs` 单元测试（3 测试） |
| 采样面 | 临时 root（测试自建）+ `CARGO_TARGET_DIR=D:/rust_cache`；**未读也未写** `~/.ruagent` 真实数据 |
| 活库只读交叉验证 | `file:~/.ruagent/data/ruagent.db?mode=ro`（仅 t247 已登记的 recall_log 574 行，本次不重复） |

### 0.1 仪器可信性：先证明链接的是哪棵树，再读数字

共享 `CARGO_TARGET_DIR` 下用两棵不同的树跑同一个包，第一次得到的结论**是错的**：改后树的
`retrieval-legs` 报 `error[E0599]: no method named search_legs found for struct Knowledge`，
而 `-v` 显示它链接的是 `libruagent_knowledge-a54df93ae6f94ec5.rlib`——该 rlib 里
`grep -a -c search_legs` = **0**，即它是**没有 t250 代码的那棵树**的产物。

修正办法：给每棵树一个不同的包身份（临时树内 `version = "0.1.1" / "0.1.2"`），强制 cargo 为它生成
新的 metadata，然后**每次读数都验证链接身份**：

| 运行 | 链接的 rlib | 该 rlib 的 `search_legs` 计数 | 结论 |
| --- | --- | --- | --- |
| 改前树 harness | `libruagent_knowledge-c03582eef59816c9.rlib` | **0** | 确为 t250 之前的库 ✓ |
| 改后树 harness | `libruagent_knowledge-223f9d34da684965.rlib` | **6** | 确为 t250 之后的库 ✓ |

没有这一步，下面 ①②③ 的数字都不可信。**这条本身是对全队的告警**：凡是「同一 CARGO_TARGET_DIR +
两棵树对比」的读数，都可能悄悄读到另一棵树的 rlib。

## ① 实体腿命中矩阵（自己跑，未使用作者输出）

命令（改后树 = 仓库当前树，`crates/graph/` 未被在途改动碰过，md5 `90628a7c6a0c46aa60784a2d22cbfd34`）：
`cargo test -p ruagent-graph --test entity-query-shapes -- --nocapture`

改前 = `search_entities`（t250 声明签名未动，`git show --stat 2c2a750 -- crates/graph/src/lib.rs` = **+89/−0**，
即该函数一行未改 ⇒ 今天跑到的 strict 列就是改前行为）· 改后 = `search_entities_loose`。

| class | query | 改前(strict) | 改后(loose) | 期望命中 |
| --- | --- | --- | --- | --- |
| plain-word | skills | 2 | 2 | Skills CLI |
| hyphen-model | **autohotkey-v2** | **0** | **1** | AutoHotkey |
| hyphen-model | deepseek-v4-flash | 1 | 1 | deepseek-v4-flash |
| cjk-substring | **潜艇** | **0** | **1** | 蓝鲸潜艇 |
| cjk-substring | **麒麟** | **0** | **1** | 银河麒麟 V10 |
| cjk-phrase | 蓝鲸潜艇 | 1 | 1 | 蓝鲸潜艇 |
| cjk-phrase | 银河麒麟 | 1 | 1 | 银河麒麟 V10 |
| multiword-en | Skills CLI | 1 | 2 | Skills CLI |
| special | XButton2 | 2 | 2 | XButton2 |
| special | C++ | 1 | 1 | C++ |
| special | deploy.sh | 1 | 2 | deploy.sh |

3 passed / 0 failed（`hyphen_phrase_requires_adjacency` · `cjk_substring_needs_like` · `query_shape_matrix`）。
**可证伪判据**：三处 0→≥1 的格子若在改后仍为 0，矩阵断言必须红；「不存在 strict 有命中而 loose 为空的查询」
这条无回归断言在 11 条上成立。

## ② 黄金集 fused recall@5：改前 → 改后

| 指标 | 改前（2e1d7ed） | 改后（2c2a750） |
| --- | --- | --- |
| fused recall@1 | 0.6667 | 0.6667 |
| **fused recall@5** | **0.8667** | **0.8667** |
| fused MRR | 0.7500 | 0.7500 |
| n（可答查询） | 15 | 15 |

⇒ t250 的「融合行为不变」被独立复现（同一仪器逐字节相同 + 链接身份各自证明）。
**可证伪判据**：两次运行若链接到同一棵树的 rlib，本表即无效；上表的两个 rlib 已分别用符号探针证明。

## ③ 两个新接口：签名与实测行为

| 接口 | 签名（源码实测） | 实测行为 |
| --- | --- | --- |
| `ruagent_store::fts` | `terms(&str) -> Vec<String>` · `match_all(&[String]) -> String` · `match_any_prefix(&[String]) -> String` · `like_patterns(&[String]) -> Vec<String>` | 单元测试 3 passed：`autohotkey-v2`→`["autohotkey","v2"]`、整段汉字保持 1 个 term、`"a%b"`→`%a\%b%`、单字符 ASCII 不进 LIKE 而单字符汉字进 |
| `Knowledge::search_legs` | `pub async fn search_legs(&self, query: &str, limit: u32) -> Result<SearchLegs, KnowledgeError>`；`SearchLegs { semantic: Vec<LegHit>, keyword: Vec<LegHit>, fused: Vec<(i64,f32)> }`；`LegHit { chunk_id: i64, rank: usize, raw_score: f32 }` | `retrieval-legs` 3 passed：两腿各自带原始分且 rank 连续 · semantic 原始分有限且 ≥0（NAN 会红）· keyword 原始分有限且 ≤0（bm25）· `fused` 与 `search()` 的返回集合逐条一致 · 无词法命中时 keyword 腿为空而 semantic 仍返回 |
| `graph::search_entities_loose` | `pub async fn search_entities_loose(db: &Db, query: &str, limit: u32) -> Result<Vec<Entity>, DbError>` | 见 ① 的 loose 列 |

## ④ 逐腿列来自产物 + grep 证据

**(a) 逐腿列已从产物取**（工作区，`crates/knowledge/tests/retrieval-quality.rs:364`）：

```
crates/knowledge/tests/retrieval-quality.rs:364:  let legs = kb.search_legs(query, 5).await.unwrap();
crates/knowledge/tests/retrieval-quality.rs:499:  "semantic": "PRESENT since t250 -- read from Knowledge::search_legs, never reconstructed"
crates/knowledge/tests/retrieval-quality.rs:504:  "keyword":  "PRESENT since t250 -- read from Knowledge::search_legs; ..."
crates/knowledge/tests/retrieval-legs.rs: 7 处 k.search_legs(...)
```

**(b) 但「仓库里不存在任何在测试中重建检索腿的代码路径」这条不成立**：

```
crates/knowledge/tests/retrieval-quality.rs:268: fn legacy_match(query: &str) -> String
crates/knowledge/tests/retrieval-quality.rs:276: fn legacy_keyword_hits(conn: &rusqlite::Connection, query: &str, limit: usize) -> Vec<i64>
crates/knowledge/tests/retrieval-quality.rs:282:   .prepare("SELECT rowid FROM chunks_fts WHERE chunks_fts MATCH ?1 ORDER BY rank LIMIT ?2")
crates/knowledge/tests/retrieval-quality.rs:375:   let legacy = legacy_keyword_hits(&ro, query, 10);   // 第二条只读连接
crates/knowledge/tests/t261-live-reading.rs:10: fn legacy_match(query: &str) -> String
crates/knowledge/tests/t261-live-reading.rs:19/24: 同一构造 + chunks_fts MATCH
```

两个文件（含 t261 的新文件）在测试内**重建了改前的 keyword 腿**，用来产出 `fused_legacy_keyword_construction`
这条对照基线。两种读法都写下来，由 captain 裁决：

* 字面读法：这就是「测试中重建检索腿」⇒ 判据不成立（我按这个判据记 failed）。
* 意图读法（t245 裁决 ② 的原话是「仪器测量它自己那份检索逻辑的副本」）：它重建的是**改前的腿**当对照，
  而且被 `legacy_construction_is_the_t245_one` 钉死（:670-677），报告的逐腿列本身来自产物 ⇒ 判据成立。

**(c) 另一个必须记录的状态差**：这条判据在 **t250 的提交里并不成立**。我在改后树（2c2a750）跑同一 harness，
JSON 里 `absent_legs.semantic/keyword` 仍是 `ABSENT -- crates/knowledge does not expose per-leg rankings...`
（该文件未被 t250 触碰，`git show --stat 2c2a750` 无 retrieval-quality.rs）。工作区版本（md5
`f7d7cbbd1447653449dff7046ceb822b`，mtime 19:17:24，git status `M`）才改成 PRESENT——**它尚未提交**，
且是在我这次验证期间由别的成员改的（同一时段有别的进程在跑这个 harness）。

## 结论

| 判据 | 判定 |
| --- | --- |
| ① 改前/改后实体腿矩阵（自己跑） | **passed** |
| ② 黄金集 fused recall@5 前后两个数 | **passed**（0.8667 → 0.8667） |
| ③ 两个新接口签名与实测行为一致 | **passed** |
| ④ 逐腿列从产物取 ∧ 无测试内重建 ∧ grep 证据 | **failed**（(b) 字面不成立；(c) 提交态不成立、工作区态成立但未提交） |

t250 的**产品代码**三条声明全部被独立复现；④ 失败的是**仓库状态/口径**问题，需要 captain 裁决两件事：
①`legacy_keyword_hits` 这类对照基线算不算「测试中重建检索腿」；②工作区那份 harness 改动由谁提交。

## 未测 / 跳过（不静默）

* **活库端到端**：本次不跑 `~/.ruagent` 上的召回（会写 recall_log），实体腿在活库上的 0→≥1 只在 t250/t247
  的读数里，本次未复测；t250 已用「自查询 63/63」给出构造对名字本身没坏的证据，我没有第二份。
* **全 workspace 的 fmt/clippy/test**：未跑。工作区有大量在途改动（Cargo.lock、cli/、daemon/、memory/、
  store/migrations 等），跑全量会把别人的半成品算进我的结论。
* **t261-live-reading.rs** 只做了 grep，未运行（它属于 t261 的在途工作，不在本次对象集内）。
