# I-SCHEMA store 迁移落地（t6）

> 状态：**已落地并通过验证**。产物：`crates/store/**`（5 个新迁移 + `fts.rs` / `lib.rs` / `migrations.rs`）+ 本文件。
> 作者：recall（I-SCHEMA = `crates/store`，本仓 schema 的唯一真相源）。
> 时间窗：**2026-09-27T22:24 → 23:0x+08:00**。纪律回执：只改 `crates/store/**` 与本文件；`git status --porcelain -- crates/knowledge crates/memory crates/graph crates/daemon panel` = **空**；真守护进程 pid 79984 未启停，`~/.ruagent/data/ruagent.db` **只读**（`mode=ro`），且读数证明它没被动过（§7.3）；所有写操作发生在 `%TEMP%` 的**副本**上。

**一句话**：四份规格 + 集成契约一共提出 **38 条** store 侧请求（§3 逐条表），**落地 27 条、有证据判 no-op 11 条**，迁移数 **18 → 23**；其中 **5 条按规格原样落地会出错或会破坏产品**（4 类 NULL/DEFAULT 背书 + wiki 的两个 CHECK），逐条用读数改了并登记（§4）。

---

## 1 改前 → 改后读数

| 项 | 改前 | 改后 | 复现命令 |
| --- | --- | --- | --- |
| 迁移数 / `SCHEMA_VERSION` | **18** | **23** | `cargo test -p ruagent-store --lib migrations::tests::migrations_apply_idempotently` |
| 新增迁移文件 | — | `0019_recall_quality.sql` · `0020_memory_semantics.sql` · `0021_graph_evidence.sql` · `0022_wiki_gen2.sql` · `0023_recall_telemetry.sql` | `ls crates/store/src/migrations` |
| 新表 | — | **12**：`query_eval_sets` `query_eval_gold` `query_eval_runs` · `memory_sources` · `entity_aliases` `resolution_pending` `communities` `community_entities` · `wiki_pages` `wiki_citations` `wiki_graph_readings` `wiki_corrections` | `cargo test -p ruagent-store --lib migrations::tests::new_objects_exist_on_a_fresh_database`（从**空库**建出，逐表断言） |
| 新虚拟表 | `chunks_fts` `entities_fts` `memories_fts` | + **`chunks_fts_cjk`**（Han bigram 影子索引） | 同上 |
| 新视图 | — | **`wiki_builds_unfinished_plans`**（见 §4.2） | 同上 |
| `ALTER` 的新列 | — | **24** 列（`recall_log` **12** · `memories` 4 · `distill_log` 3 · `entity_edges` 2 · `entities` 2 · `chunks` 1 —— 六张表） | 同上（带 `notnull=0` / `dflt_value IS NULL` 的逐列断言） |
| 新触发器 | 8 | **+6**：`chunks_cjk_ai` `chunks_cjk_au_fill` `chunks_cjk_au_change` `chunks_cjk_ad` `wiki_builds_plan_pairing_ins` `wiki_builds_plan_pairing_upd` | 同上 |
| 新索引 | — | **+8**（另 `resolution_pending` 的复合 PK 自带一个隐式索引）：`idx_query_eval_gold_set` · `idx_memory_sources_source` · `idx_entity_aliases_entity` · `idx_communities_level` · `idx_community_entities_entity` · `idx_wiki_citations_chunk` · `idx_wiki_citations_doc` · `idx_wiki_corrections_slug` | 同上 |
| `cargo test -p ruagent-store` | **20 passed**（= 32 − 本单新增 12） | **32 passed / 0 failed** | `cargo test -p ruagent-store` |
| `cargo clippy -p ruagent-store --all-targets -- -D warnings` | 干净 | 干净 | 见下 |
| `migrations::apply` 的原子性 | 每条语句各自提交 ⇒ **半应用可能** | **一迁移一事务**，失败整体回滚且不写版本行 | `cargo test -p ruagent-store --lib migrations::tests::a_failing_migration_rolls_back_completely` |

### 1.1 在**活库副本**上跑出的读数（不是空库，不是合成夹具）

仪器：`VACUUM INTO` 从**只读**连接做一份一致副本 → 对它跑迁移 → 读数据。副本 = `%TEMP%\ruagent-t6-live-copy.db`，源 = `~/.ruagent/data/ruagent.db`（651 行 `recall_log`，`schema_migrations` max=18）。命令（§7.2）。

| 读数 | 值 | 说明 |
| --- | --- | --- |
| `wiki_builds` 行数 / `wiki_builds_unfinished_plans` | **8 / 0** | 0022 的回填把两处不变量违反（id 7,8 的 `planned_only`；id 1,4 的 `finished_at IS NULL`）都闭合了 |
| `wiki_builds` 里 `(status IN ('planned','planned_only')) <> (dry_run=1)` | **0 行** | 历史数据自查通过 |
| `memories.access_count` NULL | **163/163** | 历史行 = unknown，**不是 0** |
| `memories.last_used_at` NULL | **163/163** | 同上 |
| `memories.valid_from` NULL | **163/163** | 同上（**没有**回填 `created_at`，见 §4.3） |
| `distill_log.status` NULL | **33/33** | **不是 `'ok'`** —— 这正是 §4.3 拒收 `NOT NULL DEFAULT 'ok'` 的理由：33 行里有 18 行三者全 0 |
| `entity_edges.event_time_source` NULL | **67/67** | **不是 `'recorded'`**，见 §4.3 |
| `chunks.grams` NULL | **10765/10765**（新副本）→ **0/10765**（回填后） | 两种状态都合法；Partial 填充（唯一不可能的状态）由每批一事务保证 |
| `recall_log.knowledge_leg_window` NULL | **651/651** | 历史行 NULL ⇒ 与恒定窗口后的行**不可比**，这是 C5/B-2 要求的 |
| 全量回填 10765 行 | **1430 ms（0.13 ms/行）**，第二次调用 = **0** | 可续跑、幂等的实测 |
| **bigram 索引 vs LIKE 等价性** | **86 条自语料派生的 2 字汉字子串，0 处不一致** | 这是 CJK 那一半的端到端判据：索引找到的行 == `LIKE '%XY%'` 找到的行 |

### 1.2 契约命令的读数

```
cargo test -p ruagent-store                                  -> 32 passed; 0 failed
cargo clippy -p ruagent-store --all-targets -- -D warnings    -> Finished, no diagnostics
git status --porcelain -- crates/knowledge crates/memory crates/graph crates/daemon panel
                                                             -> （空）
```

（`CARGO_TARGET_DIR=$env:TEMP\ruagent-verify-schema`；本单实测过 `ruagent-schema` 与 `ruagent-verify-schema` 两个目录，结论一致。）

---

## 2 编号分配（K-1）与四份规格的请求对照

集成契约 K-1 把「编号唯一分配者」判给 I-SCHEMA。**四份规格全部写了 `0019`**（recall §E10「一个文件：0019」· memory §E.3「需要 0019 迁移」· graph DEP-3 · wiki §E.2 `0019_wiki_gen2.sql`）—— 这是一个**四方撞号**，必须由我一个人裁决。分配如下，**每区一个号，号随区连续**：

| 号 | 文件 | 来源规格 | 成立理由 |
| --- | --- | --- | --- |
| 0019 | `0019_recall_quality.sql` | R-A §E3/E4/E6/E10 | 四方里只有 recall 是「schema 的 owner 也是本代的执行者」，先落最靠前的那份 |
| 0020 | `0020_memory_semantics.sql` | R-B E.3/E.4/E.7 + B.2 | |
| 0021 | `0021_graph_evidence.sql` | R-C E3/E4/E5/E7/E8/E10 + DEP-3 | |
| 0022 | `0022_wiki_gen2.sql` | R-D E.2 DDL-1..7 | wiki 请求的文件名是 `0019_wiki_gen2.sql`，**按 K-1 由本分配取代** |
| 0023 | `0023_recall_telemetry.sql` | R-E（集成契约 §3.1 K-2） | **追加**：R-E 的请求在 0019 写完之后才到（2026-09-27T22:53:52+08:00），且问的是另一个问题（§3.5） |

**为什么 recall 拿到两个号（0019 与 0023）而不是把 6 列并进 0019**：0019 已被 R-E 在 22:53 的 `git status` 里观测并登记为「recall 的 4 列」，把它改成 10 列会让**别人已经引用过的产物**与观测不符（closure §7.9/7.16 那一族）。追加一个文件是唯一不修改已发布内容的做法，代价是「每区一个号」变成「每区一段连续号」—— 这一段现在只是 `{0019} ∪ {0023}`，登记在此，下次分配按 **0024** 起。

---

## 3 逐条清单：四份规格 + 集成契约的每一条 store 侧请求

图例：**落地** = 本单写了 schema/接口；**no-op** = 有证据说明不需要 store 改动（引原文小节）。

### 3.1 R-A `gen2-recall-spec`

| # | 请求（出处） | 处置 | 证据 / 落地位置 |
| --- | --- | --- | --- |
| A-1 | `query_eval_sets` / `query_eval_gold` / `query_eval_runs` 三表（§E3「这一项就是 schema 变更本身」；§E10 的 DDL 原文） | **落地** | 0019，逐字含 `judged_by`（`'title-derived'` vs `'human'` 不许混写）、`answerable`、`class` 与 `idx_query_eval_gold_set` |
| A-2 | `recall_log` 4 列 `top_knowledge_relevance` / `_kind` / `knowledge_leg_window` / `scoring_version`（§E6） | **落地** | 0019；历史行 NULL，**不重解释 `top_knowledge_score`** |
| A-3 | `chunks.grams` + `chunks_fts_cjk` + 3 触发器（§E4、§E10） | **落地（触发器 3 → 4）** | 0019；见 §4.1 |
| A-4 | `fts::han_bigrams` / `fts::match_bigrams` / `MIN_RECALL_ASCII` 改 `pub`（§D.2 第 6 块） | **落地** | `crates/store/src/fts.rs`；四个冻结函数**逐字节未动**（R-C DEP-5 要求，且 `frozen_forms` 的既有断言仍在跑） |
| A-5 | bigram 回填（§E4 ③ 把执行面放在 I-A 的 `with_embedder`） | **落地，但执行面改到 store** | `Db::backfill_chunk_grams()`（`lib.rs`）；见 §4.4。**I-A 的写入侧仍要填 `grams`**（新 chunk 的 INSERT），这一条没被替代 |
| A-6 | `residual_scan(needle, limit, exact_total)` / `document_path(name)`（§D.2 第 5 块，欠 I-B） | **no-op** | 两者都定义在 `impl Knowledge`（§D.2 原文），**不是 store 的对象**：`residual_scan` 读的是 `chunks.content`（`crates/knowledge` 已能读到），`document_path` 拼的是 `<root>/knowledge/<name>.md` 这个**盘上路径**，store 既不知道 root 也不碰文件系统。归 I-A(t7) |
| A-7 | D.1 的冻结清单（`SearchHit`/`LegHit`/`KeywordStage`/`rrf`/`Embedder`/`score_kind` 字面量） | **no-op（store 侧无对应物）** | 全部在 `crates/knowledge` 与 daemon；`crates/store` 只提供 `fts` 四函数与 `Db`，本单对它们**零改动**（§3.4 的 `fts.rs` 只做加法） |

### 3.2 R-B `gen2-memory-spec`

| # | 请求（出处） | 处置 | 证据 / 落地位置 |
| --- | --- | --- | --- |
| B-1 | `memories ADD COLUMN access_count INTEGER NOT NULL DEFAULT 0` + `last_used_at TEXT`（§E.3 / B-8） | **落地，类型按 NULL 规则改** | 0020；见 §4.3 第 1 条 |
| B-2 | `memories.valid_from` / `valid_to`（B.2「采纳」+ §E.3「同批若采纳 B.2 …一并申请」） | **落地，且不回填** | 0020；见 §4.3 第 2 条 |
| B-3 | `memory_sources(memory_id, source_kind, source_id)`（§E.4「若来源列表要落库」） | **落地** | 0020 + `idx_memory_sources_source`（残留面反查方向） |
| B-4 | `distill_log` 加 `status`/`failure_reason`/`prompt_fingerprint`（§E.7） | **落地，名字统一为 `prompt_hash`** | 0020；见 §4.3 第 3 条（graph 也提了同一张表，见 C-4） |
| B-5 | `memory_diffs` 新 op 值（`merge_judged` / `consolidate`）「若 op 白名单是硬约束则登记」（§C2 行 833） | **no-op** | **实测无 CHECK**：`0004_memory.sql:50-59` 的 `op` 是裸 `TEXT NOT NULL`；活库已有 **7 个**不同 op（insert/purge/reject/skip_dedupe/supersede/delete/restore），而列注释只列了 4 个 ⇒ 新 op 值不需要迁移。**注释漂移登记在此**，`0004` 是历史文件、不能改 |
| B-6 | `episodes` 是否加「来源」列以便按来源记忆反查（U-1） | **no-op（判定：不加列）** | U-1 自己写了今天只能「用 hash 相等做粗判（会漏）」。根因：`episodes.content_hash` 是**原始 transcript** 的 sha256，与记忆行的 `content_hash` 不是同一串字节，加列也救不了；而**真正的反查**已由 B-3 的 `memory_sources(source_kind='episode')` 提供。**这是本单对 U-1 的裁决，登记给 I-B** |
| B-7 | `episodes.kind` 新值（面板 `run_turn` 派生，§E.7 ②） | **no-op** | `0004_memory.sql:5-14` 的 `kind` 是裸 `TEXT NOT NULL`，无 CHECK |
| B-8 | E.5 遗忘可证明性「schema | 无（`episodes.content` 已有）」 | **no-op（与规格一致）** | 引原文即证据 |
| B-9 | D.7 `RetrievalHit.lead` / `TAG_GRAPH` / `tag_rank` | **no-op** | `crates/memory` 的类型与常量；store 不参与 |

### 3.3 R-C `gen2-graph-spec`

| # | 请求（出处） | 处置 | 证据 / 落地位置 |
| --- | --- | --- | --- |
| C-1 | `entity_edges ADD COLUMN event_time_source TEXT NOT NULL DEFAULT 'recorded'` + `fact_hash TEXT`（E3） | **落地，类型按 NULL 规则改** | 0021；见 §4.3 第 4 条 |
| C-2 | `entity_aliases(id, entity_id, alias, norm_alias UNIQUE, source, created_at)`（E4） | **落地** | 0021（+ `idx_entity_aliases_entity`） |
| C-3 | `resolution_pending(entity_a, entity_b, reason, created_at, PK(entity_a,entity_b))`（E4） | **落地** | 0021（逐字） |
| C-4 | `distill_log` 三态 + `failure_reason` + prompt 指纹（E7） | **落地（由 0020 满足）** | 同一张表、同一组列，**一列一名**；`prompt_fingerprint` 是 memory 的散文名，落在 `prompt_hash`（graph 给了 DDL，以 DDL 为准）。两方都已在 §4.3 第 3 条登记 |
| C-5 | `communities(id, level, parent_id, summary, built_at)` + `community_entities(community_id, entity_id, weight, PK)`（E8） | **落地** | 0021（+ `idx_communities_level`、`idx_community_entities_entity` 供「实体 → 它的社区」这一检索方向） |
| C-6 | `entities ADD COLUMN embedding BLOB` / `embedder TEXT`（E10） | **落地** | 0021；另附**成本登记**见 §4.5 |
| C-7 | `recall_log` 增 `graph_entities` / `graph_paths`（DEP-3） | **落地** | 0021 |
| C-8 | E5 可选 `entity_sources(entity_id, episode_id)` | **no-op（判定：不建）** | E5 自己标「可选」，且没有命名的消费者；它要服务的 G 目标是**边的来源**，而 `entity_edges.source_episode` 已在 `0005_graph.sql:34`。**一张没人读的表不是 schema，是一句「已覆盖」的声称**；I-C 真落地了消费者，一次加法迁移即可 |
| C-9 | E2「可选索引 `entity_edges(src, invalid_at)`」 | **no-op** | `0005_graph.sql:36-37` 已有 `idx_edges_src(src, relation, invalid_at)` 与 `idx_edges_dst`；再建 `(src, invalid_at)` 只是现有索引的前缀 |
| C-10 | E1/E6/E9「无 schema」 | **no-op（与规格一致）** | 引原文即证据 |

### 3.4 R-D `gen2-wiki-spec`（E.2 DDL-1..7）

| # | 请求 | 处置 | 证据 / 落地位置 |
| --- | --- | --- | --- |
| D-1 | DDL-1 `wiki_pages`（含 `stale_since` / `stale_sources_json` / `uncertain_markers`） | **落地** | 0022（逐字；唯一改动是把逐条注释保留下来） |
| D-2 | DDL-2 `wiki_citations` + 两个索引 | **落地** | 0022；**不加 FK 到 `wiki_pages`** —— 那会给 I-D 强加一个它的代码没有的写入顺序（规格原文也没有 FK）。这是**决定，不是遗漏**，写在文件里 |
| D-3 | DDL-3 回填 legacy 词汇 + `finished_at` | **落地** | 0022；**回填前的现场值逐行记在迁移注释里**（8 行 / id 7,8=`planned_only` / id 1,4=`finished_at NULL`），因为回填会改这些值而记录必须活下来（§7.77） |
| D-4 | DDL-4 路线 B：两个 CHECK + 回填，不删列 | **落地，但改成触发器 + 视图** | 见 §4.2（**本单最重要的偏差**） |
| D-5 | DDL-5 路线 B（不写 `pending` 页行；22 行 pending 由产品路径清理） | **no-op** | 规格原文：「现场 22 行 pending 的清理由产品路径完成，**不在迁移里**」。另：`wiki_build_pages.status` 是裸 `TEXT NOT NULL`（`0010_wiki.sql:25`），I-D 的新取值**不需要迁移** |
| D-6 | DDL-6 `wiki_graph_readings` | **落地** | 0022（逐字，`build_id` 为 PK ⇒ 每构建一行） |
| D-7 | DDL-7 `wiki_corrections` + 索引 | **落地** | 0022（`reason`/`author` 的非空 CHECK 逐字保留） |
| D-8 | §E.3「迁移 0019（DDL-1…7）、**`Db` 仓储方法**」 | **落地（文件号 = 0022）+ no-op（无新 Db 方法）** | 编号见 §2。仓储方法：I-D 自己的规格写「I-D 的每一次写都用现成的 `Db::call/call_flat`（`wiki.rs` 已有 `run_write` 封装）」⇒ **不需要新方法，一项都不加**（不加比加更符合它的规格） |

### 3.5 R-E `gen2-integration-contract`（K-1 / K-2 / §3.1）

| # | 请求 | 处置 | 证据 |
| --- | --- | --- | --- |
| E-1 | K-1 编号分配 | **落地** | §2；`0019..0023`，契约里的文件名 `0019_wiki_gen2.sql` 已被取代 |
| E-2 | §3.1 缺口 6 列 `score_kind` / `fusion` / `top_legs_json` / `candidates_json` / `selected_json` / `rejected_json` | **落地** | `0023_recall_telemetry.sql`，逐字对齐 §3.1 的语义（含「未命中的腿不出现」「`graph_no_seed` 让『没种子』与『有种子无路径』可分」）；`score_kind` 为**常量列**（`"rrf_rank"`），理由是读数者不该需要知道写它的代码是哪一代 —— 与 0019 的 `top_knowledge_relevance_kind` 同一个理由 |
| E-3 | §3.1「历史行全部 NULL；`top_knowledge_score` 的解释一个字节不改」 | **落地并实测** | 副本上 `recall_log.knowledge_leg_window` NULL 651/651；`top_knowledge_score` 未被任何迁移触碰（列清单断言里逐位核对） |
| E-4 | §3.1「14 列」 | **落地：14/14** | 8 列在 0019+0021，6 列在 0023；`new_objects_exist_on_a_fresh_database` 把 `recall_log` 的**完整列序**（**24 列**）钉住，所以「多一列/少一列/次序变了」都会红 |

---

## 4 三处偏差 + 一处执行面迁移（每条都带读数）

> 判据：**按规格原样落地会出错、会破坏产品、或会写入一个迁移无法支持的声称** ⇒ 改，并把理由与读数登记。**「规格说 A 我做 B 但没登记」是不允许的。**

### 4.1 A-3：`chunks_fts_cjk` 的触发器是 4 个而不是 3 个

- **规格**（R-A §E10）给了 3 个触发器，第三个是 `AFTER UPDATE OF grams`，体内先 `'delete'` 旧值再插入新值，**没有 NULL 守卫**。
- **问题**：`grams` 这一列**唯一的填充路径就是 `NULL → 值` 的 UPDATE**（回填）。在那个 UPDATE 上，`'delete'` 半支会对一行**从未被插入过索引**的记录执行 FTS5 的 `delete` 命令。
- **改法**：按 `old.grams` 是否存在把 UPDATE 路径拆成两支：
  `chunks_cjk_au_fill`（`WHEN old.grams IS NULL AND new.grams IS NOT NULL`：只插入）、`chunks_cjk_au_change`（两支都非空且不同：先删后插）。另有 `chunks_cjk_ai`（`WHEN new.grams IS NOT NULL`）与 `chunks_cjk_ad`（`WHEN old.grams IS NOT NULL`）。
- **判据（可证伪）**：`migrations::tests::cjk_bigram_index_follows_a_null_to_value_backfill` —— 断言恰好走过 NULL→值→改值→删除四步，每步用 **MATCH**（不是 `COUNT(*)`：external-content 表的普通 SELECT 读的是 `chunks`，不看索引）读索引；改值那步断言**旧值 0 命中、新值 1 命中**（重复投递会让同一 rowid 出现两次 ⇒ 判据能红）。

### 4.2 D-4：**wiki_builds 的不变量用触发器 + 视图落地，不做表重建，第二个 CHECK 不实现**（本单最重要的偏差）

规格（R-D DDL-4 路线 B）要求「两个 CHECK + 回填」。**两个都按约束落不了**，两条理由都是实测的：

1. **表重建在今天的 runner 里跑不了。** `wiki_builds` 是 `wiki_build_pages(build_id REFERENCES wiki_builds(id))` 的父表，现场有 **41 行子行**；`Db::configure_and_spawn` 设 `foreign_keys = ON`（`sqlite.rs:49`），所以 `DROP TABLE wiki_builds` 会执行一次隐式 DELETE 并**违反即时外键而中止**；标准的 12 步重建要求 `PRAGMA foreign_keys=OFF`，而该 pragma **在事务内是 no-op**，1.3 之前 `apply` 又根本没有事务。⇒ 「为一个能用另一种方式拿到的约束，去做一次失败后不可重跑的重建」不成立。（本单顺手把 `apply` 改成**一迁移一事务**，正是让重建**将来**有可能的前置条件。）
2. **第二个 CHECK 用约束根本表达不了，这是产品自己的写入顺序决定的。** `insert_build`（`wiki.rs:1642`）写 `status='planned_only'` 且 **`finished_at` 尚未设置**，`finish_dry_run`（`wiki.rs:1845`）在**另一条语句**里补 `finished_at`。INSERT 那一刻不变量「plan-only ⇒ finished_at 非空」**合法地为假** ⇒ CHECK 会拒绝每一次 dry-run 构建，触发器也不可能更好（触发器看不见未来）。
   所以这一半以**读数**形式交付：视图 `wiki_builds_unfinished_plans`，**要求值 = 0 行**（副本实测 0/8）。测试里还专门断言了「刚插入的 plan-only 行让视图 = 1」—— 那一条断言本身就是「CHECK 不可能存在」的证明。

**能强制的那一半照强制**：`(status ∈ {planned, planned_only}) = (dry_run = 1)` 由两个触发器守（INSERT 拒非法配对；UPDATE 只拦「合规行被改成不合规」，**不**冻结历史行 —— 否则那行就再也修不回合规）。

**第二个偏差点：接受的词汇是 `planned` 与 `planned_only` 两个值。** 今天的写入者仍发 `planned_only`（`DRY_RUN_STATUS`，`wiki.rs:1838`），I-D 的 E3 会改成 `planned`。若我按严格单值落，**本迁移与 t10 之间每一次 dry-run 构建都会被中止**。收窄成单值是「拿到 I-D 的代码改动后」的一条后续迁移，登记在此。

**「不会破坏产品」是怎么证的（不是推断）**：`wiki_builds_invariant_accepts_every_product_write_and_rejects_bad_pairs` 把 `wiki.rs` 的**每一条真实语句**逐一重放 —— `insert_build`×2 形状（dry 的 `planned_only`/1、real 的 `running`/0）、`finish_dry_run`、`finish_build`、`fail_build`、`update_build` 的通用字段写 —— 断言**全部通过**；然后断言三个**该被拒**的写（`done`+`dry_run=1`、`planned`+`dry_run=0`、把合规行改成 `done`）**全部被拒**。另：`fail_build` 在 dry-run 路径上不可达（`start_build_inner` 在 `wiki.rs:774` 就 return 了，`fail_build` 只在 `execute_build` 里），所以「dry 行被标记失败」这条路径不存在，触发器不会卡住它。

### 4.3 三个「`NOT NULL DEFAULT x` 会替历史行背书」的列类型改动

**本单统一的规则**（写在每个迁移文件的顶部，并有一条测试强制）：

> **给已有数据的表加列时，新列一律 NULLABLE 且无 DEFAULT，除非那些行对该列的值是真正可知的。NULL = 「unknown，因为这行早于这一列」** —— 即 `0018` 为 `recall_log.source` 立下的规则（「NULL is left NULL: it means UNKNOWN, not 'user'」）。

`ALTER TABLE … ADD COLUMN x NOT NULL DEFAULT v` **不只是**约束未来的写入：它会把 **v 回填进每一行历史**，而那是一句迁移无法支撑的、关于过去的声称。

| # | 规格原文 | 若照做会发生什么 | 本单的落法 |
| --- | --- | --- | --- |
| 1 | memory §E.3：`access_count INTEGER NOT NULL DEFAULT 0` | 163 行历史被盖上「用过 0 次」，可被读成**证据**说明从未使用；真相是当时没有计数器 | `access_count INTEGER`（可空）。读数：副本上 **163/163 NULL**。读侧用 `COALESCE(access_count, 0)` 做算术，但**「0」与「unknown」在存储层仍可分** |
| 2 | memory B.2 的代价注：`valid_from/valid_to` 的回填「只能回填 `created_at`（这是回填，不是事实）」 | 把「行被写入的时刻」当成「事实在世界里为真的时刻」，两个不同的量 | **不回填**：`valid_from`/`valid_to` 可空，读侧显式回落 `COALESCE(valid_from, created_at)`。读数：**163/163 NULL**。这使「这行有显式有效期」与「这行是在用代理」**可分** |
| 3 | graph §E3：`event_time_source TEXT NOT NULL DEFAULT 'recorded'` | 67 行里 **66 行**（`|valid_at−created_at| ≤ 1s`）被标成 `'recorded'` 是**对的**，**1 行是错的**；一个对 98.5% 正确、对剩下的静默错误的单一取值，比一个说「写这条边的抽取路径没有记录来源」的 NULL 更坏 | `event_time_source TEXT`（可空）。读数：**67/67 NULL**。graph 的 `TemporalStatus::RecordedAtOnly` 仍可由**测量** `|valid_at − created_at|` 派生 —— 列记录的是**写入者知道什么**，那正是测量无法重建的信息 |
| 4 | graph §E7：`distill_log.status TEXT NOT NULL DEFAULT 'ok'` | 33 行里 **18 行**三者全 0（mem-core 2026-09-27 读数）—— 正是这一列要区分的那些行 —— 会被全部断言为 33/33 成功，与本代的发现相反 | `status TEXT`（可空）。读数：**33/33 NULL** |

**名字统一（一列一名）**：memory §E.7 的散文名 `prompt_fingerprint` 与 graph §E7 的 DDL 名 `prompt_hash` 是同一件事。**以 DDL 为准 ⇒ `prompt_hash`**，登记在此，两个 owner 都不必再猜。

**新表不受此规则影响**（没有历史），所以 12 张新表与 9 个索引**逐字保留规格的 `NOT NULL` / `DEFAULT`**（`wiki_pages.verified DEFAULT 'unverified'`、`wiki_corrections` 的两个非空 CHECK 等）。测试按这条线分两半断言：新增列 `notnull=0 且 dflt_value IS NULL`，新表逐字存在。

### 4.4 A-5：bigram 回填的执行面从 I-A 移到 store（`Db::backfill_chunk_grams`）

- **规格**（R-A §E4 ③）把它放在 `Knowledge::with_embedder`（I-A 的 t7）。
- **改法**：在 store 提供 `Db::backfill_chunk_grams() -> Result<u64, DbError>`（分批 256、每批一事务、可续跑、幂等、**返回填充行数**）。I-A 的写入侧**仍必须**在新 chunk 的 INSERT 里填 `grams` —— 这一条没被替代。
- **理由**：**schema 与「让 schema 有用」的那一遍必须一起交付**，否则 CJK 腿会在「有人记得回填」之前静默索引空；把执行面留给下游正是 `§7.45`（一个假设被写进派单就变成别人的前提）的形状。而且它必须用 `fts::han_bigrams`（SQL 算不出来），store 正是唯一真相源所在。
- **判据**：副本上 **10765/10765 行、1430 ms（0.13 ms/行）**，第二次调用 **0**；一个「标点-only 的 chunk」被填成 `''` 而**不是 NULL**（否则循环会永远重新找到它 —— 这一条有专门的测试：`bigram_backfill_fills_the_index_and_terminates_on_empty_bigrams`，含 `grams=''` 计数 1 / `grams IS NULL` 计数 0 的读数）。

### 4.5 给 I-C 的成本登记（不是缺陷，是这一列带来的行为改变）

`entities_au`（`0005_graph.sql:43`）是 `AFTER UPDATE ON entities` 且**没有列过滤**，所以 E10 的向量种子腿一旦写 `embedding`，**每个被嵌入的实体会额外重写一次 `entities_fts` 行**（delete + insert）。那个触发器在历史迁移里，**不能在此收窄**。登记给 I-C：写 embedding 时按批更新，或接受这次 FTS 重写。

---

## 5 NULL 语义：渲染层如何区分「unknown（早于该列）」

**规则（本单交付的契约，供 I-B/I-C/I-D/I-INT/面板消费）**：

1. **新列在历史行上恒为 NULL**（§4.3 的表 + §1.1 的副本读数逐列证明了这一点）。
2. **NULL 不得被渲染成任何一个具体取值**：`0` / `''` / `'ok'` / `'human'` / `'unverified'` 都是禁止的降级方向。要一个数字就用 `COALESCE(x, 0)` **在读取点显式**这么做，并把「这是回落值」写出来。
3. **面向人的渲染用 `unknown (pre-NNNN)` 字面量**，把列名带到读者眼前；`0018` 对 `recall_log.source` 已经规定过这个形状，本代的列沿用 `pre-0019` / `pre-0020` / `pre-0021` / `pre-0022` / `pre-0023`。
4. **不许把这些行从列表里过滤掉**（`0018` 的原文：过滤会把历史变成 0 行，「看起来更整齐，实际静默地丢了样本」）。

**新增列 → 渲染层要处理的 unknown 清单**（供 I-INT 的 §3.1 透出与面板）：

| 列 | NULL 的含义 | 读侧要做的 |
| --- | --- | --- |
| `recall_log.top_knowledge_relevance` / `_kind` | 该行早于 0019，**没有**可信的相关性分 | 渲染 `unknown (pre-0019)`；**不得**回落成 0；**不得**与有值的行画在同一条曲线上 |
| `recall_log.knowledge_leg_window` | 该行是用 `limit.max(10)` 的窗口产生的 | 与恒定窗口的行**不可比**（R-A B-2 明文）。契约 A-3a 已要求 `recall_log` 端点透出该列 |
| `recall_log.scoring_version` | 未知代次 | 不同 version 不得共图 |
| `recall_log.graph_entities` / `graph_paths` | 图腿**未被尝试**（不是「返回 0 条」） | 与 `rejected_json.graph_no_seed` 联读；`0` 与 `NULL` 必须分开渲染 |
| `recall_log.score_kind` / `fusion` / `top_legs_json` / `candidates_json` / `selected_json` / `rejected_json` | 该行早于 0023 | `unknown (pre-0023)`；契约 A-3b 在列不存在时判 `not_measured`，本单落地后改为可判 |
| `chunks.grams` | 该 chunk 还没被企回填（**不是**「内容没有 bigram」） | bigram 腿只按索引查 ⇒ NULL 行自然不命中；**回填后**「内容无 bigram」的诚实表示是 `''` |
| `memories.access_count` / `last_used_at` | 不知道用过几次 / 何时用过 | 排序用 `COALESCE(access_count,0)`；**面板不得把 NULL 显示成 `0 次`** |
| `memories.valid_from` / `valid_to` | 该行没有显式有效期，只能回落 `created_at` / `superseded_at` | 回落是**读取点的显式动作**，不是存储层的填充 |
| `distill_log.status` / `failure_reason` / `prompt_hash` | 该次蒸馏的结局**未知**（33 行里 18 行三者全 0） | **不得**渲染成 `ok`；三态读的是 `ok`/`empty`/`failed`/`unknown(pre-0020)` **四态** |
| `entity_edges.event_time_source` / `fact_hash` | 写这条边的抽取路径没记录来源 / 没有事实指纹 | 与**测量**出来的 `RecordedAtOnly` 分开渲染（一个是记录，一个是推断） |
| `entities.embedding` / `embedder` | 该实体未嵌入 | 向量种子腿必须跳过它并**计数**，不许当成「相似度 0」 |
| `wiki_pages.*`（新表） | 不适用（无历史），但 `stale_since` 的 NULL = **从未观测到漂移** | 与 `stale=0` 一起读；`wiki_pages_unfinished` 之类的新视图同理 |

**在仓里发现的一处既有违反（登记，本单不改）**：`crates/store/src/lib.rs:189`

```rust
// NULL selected_by = a selection made before migration 0012.
Some((Some(run), by)) => Ok(Some((run.parse().map_err(conv)?, by.unwrap_or_else(|| "human".into())))),
```

它把 `0012` 之前的行的 `selected_by` **渲染成 `"human"`** —— 正是本验收条目点名的「NULL ≠ user」。**我没有顺手改**：它的返回类型是 `(RunId, String)`，改掉要动 `orchestrator`/`runs` 的消费方（不在本单 inScope），且会改一条已有行为，必须有自己的一次验证。**登记为待办**：把 `by` 变成 `Option<String>` 或在读取点渲染成 `unknown (pre-0012)`。

---

## 6 store 侧新增接口（除 schema 之外的「仓储与 FTS」）

| 接口 | 文件 | 判据 |
| --- | --- | --- |
| `pub const MIN_RECALL_ASCII: usize = 3`（原为私有） | `fts.rs` | 值在**编译期**钉住（`const _: () = assert!(MIN_RECALL_ASCII == 3);`，运行时断言常量是重言式且 clippy 会拒） |
| `pub fn han_bigrams(text: &str) -> String` | `fts.rs` | 汉字 run → 重叠 2 字 bigram；长度 1 的 run → 该字符；非汉字字母数字 run → **原样透传**（bigram 化 ASCII 会把它变成噪声，精确/前缀命中本来就是前两段的活）；其余 → 分隔符 |
| `pub fn match_bigrams(terms: &[String]) -> String` | `fts.rs` | 每个**汉字 term** 出一个它自己 bigram 的**短语**（有序 ⇒ 精确子串语义），OR 连接；**非汉字 term 跳过**（索引里没有它，OR 一个永不匹配的模式只会让这一腿看起来更大）；无汉字 term ⇒ 返回 `""`，调用方**不得**把空模式交给 MATCH |
| `Db::backfill_chunk_grams(&self) -> Result<u64, DbError>` | `lib.rs` | 见 §4.4 |
| `migrations::apply` 改为**一迁移一事务** + `pub const SCHEMA_VERSION: i64` | `migrations.rs` | 见 §1 与 §4.2 理由 1；`apply_one` 拆出来是为了**不用往 `MIGRATIONS` 里塞一条坏迁移**就能测回滚 |
| **冻结不动**：`fts::{terms, match_all, match_any_prefix, like_patterns}` | `fts.rs` | R-C DEP-5（graph 照抄这 4 个函数）与 R-A D.1 都要求逐字节不变；三个既有单测仍在跑（未改一字），本单的新测试**另加**并额外断言了 `match_all`/`match_any_prefix` 的输出没变 |

---

## 7 复现

### 7.1 契约命令

```powershell
$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"
$env:CARGO_TARGET_DIR="$env:TEMP\ruagent-verify-schema"
cargo test -p ruagent-store
cargo clippy -p ruagent-store --all-targets -- -D warnings
git status --porcelain -- crates/knowledge crates/memory crates/graph crates/daemon panel
```

### 7.2 从**空库**建出全部 schema 的读数

`cargo test -p ruagent-store --lib migrations::tests::new_objects_exist_on_a_fresh_database`：12 张新表 + 1 个视图逐表断言存在；`chunks` 与 `recall_log` 的**完整列序**（8 列 / **24 列**）逐位断言；**24 个新列**逐列断言 `notnull=0` 且无默认值（`recall_log` 12 · `chunks` 1 · `memories` 4 · `distill_log` 3 · `entity_edges` 2 · `entities` 2）。

### 7.3 活库副本上的读数（写的是副本，源库只读且已证明未被动过）

```powershell
# 1) 一致副本（VACUUM INTO 从只读连接，含 WAL 内容）
python -c "import sqlite3,os;src=os.path.join(os.environ['USERPROFILE'],'.ruagent','data','ruagent.db');dst=os.path.join(os.environ['TEMP'],'ruagent-t6-live-copy.db');c=sqlite3.connect('file:'+src.replace('\\','/')+'?mode=ro',uri=True);c.execute('VACUUM INTO ?',(dst,));print('copied',dst)"
# 2) 迁移后（**未回填**）的历史 NULL 读数
$env:RUAGENT_T6_LIVE_COPY="$env:TEMP\ruagent-t6-live-copy.db"
cargo test -p ruagent-store --lib migrations::tests::live_copy_keeps_history_nullable -- --exact --nocapture
# 3) 全量回填 + bigram 索引/LIKE 等价性
cargo test -p ruagent-store --lib tests::live_copy_bigram_backfill_at_scale -- --exact --nocapture
```

两个 live-copy 测试**没设环境变量时大声跳过**（打印 `[t6] NOT MEASURED: …` 与理由），绝不静默通过 —— 实测打印：
`[t6] NOT MEASURED: set RUAGENT_T6_LIVE_COPY=<a migrated copy of the live db> …`（两条）。

**源库未被碰过的判据**（`python %TEMP%\ra_t6_livecheck.py`，2026-09-27T22:58:53+08:00）：`schema_migrations` max 仍 = **18**；`recall_log` 仍 **12** 列；`chunks` 仍 **7** 列；`query_eval_sets`/`wiki_pages`/`memory_sources`/`entity_aliases`/`chunks_fts_cjk` 在源库**都不存在**（`[]`）；行数 934/10765/651/163/63/8；db mtime = `2026-09-27T21:15:21+08:00`（早于本单任何操作）。真守护进程 pid 79984 未被启停。

### 7.4 一次变异实验（判据确实能红）

`a_failing_migration_rolls_back_completely` 用一条**故意坏的**迁移（先 `CREATE TABLE` 再引用不存在的表）证明：报错、`t6_atomicity_probe` 表**不存在**、`schema_migrations` 行数不变。改前那版 runner 会把第一句留在盘上。

---

## 8 未测 / 不判定（不许静默跳过）

| # | 项 | 状态 | 原因 | 谁解 |
| --- | --- | --- | --- | --- |
| N-1 | 迁移在**真守护进程下一次启动**时应用是否成功 | **未测**（不可在本单证） | 不许启停 pid 79984；副本上等价路径已测（同一 `apply` 代码 + 同一量级数据） | INT/t19 的重启窗口（t20 的 A-3a 会覆盖） |
| N-2 | `chunks.grams` 的**磁盘增量** | **未测** | 副本回填后可量，但本单时间窗用来做等价性判据；且磁盘增量取决于语料 | I-A(t7)/V-A，口径写在 R-A §G7 |
| N-3 | 0004 的 `memory_diffs.op` 注释漂移（列 4 个、库里 7 个） | **不判定（登记）** | `0004` 是历史迁移文件，**不能改**；活库 7 个 op 值是读数不是规范 | captain 裁决是否值得一次「只改注释」的后续迁移 |
| N-4 | `lib.rs:189` 的 NULL → `"human"` | **未测/未改（登记）** | 改它要动 `orchestrator`/`runs` 的消费方（不在 inScope），且需要自己的验证 | 见 §5 末，建议单独一单 |
| N-5 | 路线 A（删 `dry_run` 列） | **不采纳（规格自己排在 P1-later）** | R-D 裁决「先做路线 B」；且它需要 4.2 理由 1 说的表重建 | 后续（拿到 I-D 的 E3 之后） |
| N-6 | `wiki_build_pages` 22 行 `pending` 的清理 | **不在迁移里（规格明文）** | R-D：由产品路径完成；迁移去猜哪些过期就是发明数据 | I-D(t10) |

---

## 9 交接（给下游）

| 给谁 | 内容 |
| --- | --- |
| **I-A / t7（recall，我自己）** | ① 写入侧要在 INSERT chunk 时填 `grams`（用 `fts::han_bigrams`）；② 调 `Db::backfill_chunk_grams()` 并把它返回的行数**打进输出**（0 与「没跑」必须可分）；③ keyword 腿的降级链用 `fts::match_bigrams` 加 bigram 段，`KeywordStage` 加 `Bigram` 变体（`api.rs:2967` 的 `format!("{:?}")` 会自动输出 `"bigram"`）；④ `SearchEvidence` + `search_page`（R-A D.2） |
| **I-B / t8（mem-core）** | ① `access_count`/`last_used_at`/`valid_from`/`valid_to` 可空、无默认，`COALESCE` 在读取点；② `memory_sources` 是 U-1 的答案；③ `distill_log.prompt_hash` 是统一名（不是 `prompt_fingerprint`）；④ `memory_diffs` 的新 op 值无需迁移 |
| **I-C / t9（graph）** | ① `event_time_source` 可空 ⇒ `RecordedAtOnly` 仍靠**测量**派生；② `distill_log` 三列已由 0020 落地；③ 不建 `entity_sources`（要就再说）；④ §4.5 的 `entities_au` 成本 |
| **I-D / t10（wiki）** | ① 编号 = **0022**（不是 0019）；② 不变量用触发器 + 视图，**没有 CHECK** ⇒ 请勿假设 DB 会拒绝 `finished_at IS NULL` 的 plan-only 行，那个不变量靠 `wiki_builds_unfinished_plans` = 0 行来守；③ 词汇收窄到单值是**后续**迁移，与本单**不冲突**（两个值都受支持） |
| **I-INT / t19（integ）** | ① K-2 的 6 列已落地（`0023`），A-3b 可从 `not_measured` 改为**可判**；② §5 的 unknown 清单是透出的口径（`null`，不是 0） |
| **verify / V-A** | 本单的可证伪面：`cargo test -p ruagent-store`（32）、clippy、`git status` 边界、§7.3 的副本读数（含 86 条子串 0 不一致）。**注意**：live-copy 两个测试**需要** `RUAGENT_T6_LIVE_COPY` 才有读数，不设会打印 `NOT MEASURED` 并**通过** —— 请把「有没有打出那两行 `[t6]` 读数」当判据，而不是只看绿 |
