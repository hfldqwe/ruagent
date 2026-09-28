# RV-SCHEMA 评审（t23）：t6 / I-SCHEMA 的十条验收与两处替代形式的判决

> **结论**：**pass**。t6 的十条验收**逐条达成**，每条都有**我自己的**读数；两处 captain 已接受的替代形式，我不只看到「东西在」，而是**复现了它们的必要性**（真实外键父表上 `DROP TABLE` 中止 + 事务内 `PRAGMA foreign_keys=OFF` 是 no-op；规格那版触发器在唯一填充路径上把 FTS5 索引写坏），并**逐格测了 UPDATE 触发器的拒绝面**。
> **评审员**：review（独立评审员，**不是** t6 的作者，`crates/` 一行未改；本单只写本文件）。
> **被评审对象**：t6 / I-SCHEMA —— `crates/store/src/migrations/0019..0023_*.sql`（5 个新文件）+ `migrations.rs` / `fts.rs` / `lib.rs` + 作者报告 `docs/design/reviews/gen2-store-impl.md`；工作树 `0a39e5b8`（未提交）。
> **时间窗**：2026-09-27T23:19 → 23:45 +08:00。t6 的完成时刻（team.json `updatedAt = 1790521498077`）= **2026-09-27T23:04:58+08:00**（归因判据用）。
> **我的构建树**：`CARGO_TARGET_DIR=$env:TEMP\ruagent-review-vschema`（不与他人共用）。
> **我的副本**：`%TEMP%\rv23-live-copy-A.db`（`VACUUM INTO`，从 `mode=ro` 连接造；**绝不**指向 `~/.ruagent`）。
> **环境纪律**：真守护进程 **pid 79984 未启停**；`~/.ruagent/data/ruagent.db` 全程只读，**sha256 前后逐位相同**（`51539f88c4f901b148cd0ab05c232de84baff2f8f6d9db54b3366494b3c50aa1`，mtime `2026-09-27T21:15:21`，size 14,946,304 B）；未使用 `GET /api/v1/recall`（本单没碰 HTTP）。

---

## 0 一句话

t6 交付的是一个**加法迁移**：18 → 23 个迁移、12 张新表、1 张新虚拟表、1 个新视图、24 个新列、6 个新触发器、8 个新索引，外加 `Db::backfill_chunk_grams()` 与三个 `pub` 导出。我在**自己的 target dir、自己用 Python 从 `.sql` 建的库、自己的活库副本**上把十条验收逐条重做，全部成立。两处 captain 替代形式**成立且必要**——它们不是「换个写法」，而是避免一次产品级停摆（每次 dry-run 构建被拒）与一次索引损坏（`database disk image is malformed`）。

**findings：7 条，全部 low/观察，无 medium / high / blocker。** 其中唯一一条「读数错了」的（RV23-1：触发器改前基数写成 8，实为 7）不落在验收第 7 条点名的三项读数上，不影响任何一条验收的判决。

---

## 1 我自己的读数

### 1.1 契约命令（我自己的 target dir）

| 命令 | 我的读数 | exit |
| --- | --- | --- |
| `cargo test -p ruagent-store` | `test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured`（+ doc-tests 0）；二进制 `ruagent_store-4d07e171ff807ebb.exe` | **0** |
| `cargo clippy -p ruagent-store --all-targets -- -D warnings` | `Finished dev profile … in 5.02s`，**零 warning / 零 error** | **0** |
| `git status --porcelain -- crates/store` | ` M fts.rs` ` M lib.rs` ` M migrations.rs` + `?? 0019..0023`（8 项；加上报告文件 = t6 `changedPaths` 的全部九项，**多一项都没有**） | 0 |
| `git status --porcelain -- panel` | **空**（`panel/` 至今零改动） | 0 |
| `git diff --stat HEAD -- crates/store/src/migrations` | **空** | 0 |
| `git diff --name-only HEAD -- crates/store/src/migrations` | **空**（历史 0001–0018 一字未改） | 0 |
| `git diff --numstat HEAD -- crates/store/src/fts.rs` | **+207 / −2**，两行删除恰好是注释行与 `const`→`pub const` ⇒ **四个冻结函数（`terms`/`match_all`/`match_any_prefix`/`like_patterns`）逐字节未动的独立论证**（只有 2 处删除且都被解释掉，函数体不可能被改过） | 0 |
| `Select-String -Pattern '#\[ignore\|#\[should_panic' crates/store/src` | **无命中** ⇒「0 ignored」是结构性的 | — |

### 1.2 假通过面：**我亲自验了**（t23 的验收要求之一）

**不设 `RUAGENT_T6_LIVE_COPY`**（我先显式清掉环境变量，并打印了 `is set? False`）：

```
running 2 tests
test migrations::tests::live_copy_keeps_history_nullable ... [t6] NOT MEASURED: set RUAGENT_T6_LIVE_COPY=… ok
test tests::live_copy_bigram_backfill_at_scale ...           [t6] NOT MEASURED: set RUAGENT_T6_LIVE_COPY=… ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 30 filtered out
EXIT=0        # [t6] 行数 = 2
```

⇒ **绿 ≠ 测到**：两个测试只打印 `[t6] NOT MEASURED: …` 就 `ok`，退出码 0。所以判据落在「**有没有打出 `[t6]` 读数行**」，而不是退出码。下面所有 live 读数都来自**我自己 `VACUUM INTO` 的副本**。

**指向我自己的副本**（`RUAGENT_T6_LIVE_COPY=%TEMP%\rv23-live-copy-A.db`）：

```
[t6] live copy: wiki_builds rows=8 unfinished_plans=0
[t6] live copy: memories.access_count NULL 163/163
[t6] live copy: memories.last_used_at NULL 163/163
[t6] live copy: memories.valid_from NULL 163/163
[t6] live copy: distill_log.status NULL 33/33
[t6] live copy: entity_edges.event_time_source NULL 67/67
[t6] live copy: chunks.grams NULL 10765/10765 (all-NULL = freshly migrated, none-NULL = backfilled)
[t6] live copy: recall_log.knowledge_leg_window NULL 651/651
test migrations::tests::live_copy_keeps_history_nullable ... ok        # 8 行 [t6] 读数
```

**回填两遍（同一副本，`tests::live_copy_bigram_backfill_at_scale`）**：

```
1st: [t6] live copy: chunks=10765 grams_missing_before=10765 filled=10765 elapsed=1486ms (0.14 ms/row)
     [t6] live copy: bigram-index vs LIKE equivalence on 86 derived 2-char Han substrings -> 0 disagreement(s) []
2nd: [t6] live copy: chunks=10765 grams_missing_before=0 filled=0 elapsed=38ms (38.27 ms/row)
     [t6] live copy: bigram-index vs LIKE equivalence on 86 derived 2-char Han substrings -> 0 disagreement(s) []
```

⇒ 「一次填满 + 第二次 0」= 幂等，**我自己跑出来的**（fill 行数 10765 与作者/V-SCHEMA 逐位一致；耗时是负载差）。`filled=0` 那行的 `38.27 ms/row` 是除零产物 ⇒ **RV23-2**。

### 1.3 我自建库（只用 `.sql` 文件，**不经过 crate**，Python `sqlite3` 3.50.4）

| 对象集 | 改前（0001..0018） | 改后（0001..0023） | 差 |
| --- | --- | --- | --- |
| `MAX(schema_migrations.version)` | **18** | **23** | +5 |
| 表 / 视图 | 42 / 0 | **59 / 1**（`wiki_builds_unfinished_plans`） | +17 / +1 |
| 触发器 | **7**（`chunks_ad/ai` · `entities_ad/ai/au` · `memories_ad/ai`） | **13**（+`chunks_cjk_ai/au_fill/au_change/ad` + `wiki_builds_plan_pairing_ins/upd`） | **+6** |
| 具名索引（`sql IS NOT NULL`） | **10** | **18** | **+8** |
| 12 张新表 | — | **12/12 存在** | — |
| `chunks_fts_cjk` | — | 存在（external content over `chunks`） | — |
| `recall_log` 列数 | 12 | **24** | +12 |
| 24 个新增列 `notnull=0 且 dflt_value IS NULL` | — | **24/24，0 例外** | — |

表中 24 列按表分布**逐位核对**：`recall_log` 12 · `memories` 4 · `distill_log` 3 · `entity_edges` 2 · `entities` 2 · `chunks` 1。

### 1.4 归因：t6 自己的改动 vs 同伴的在途编辑（验收第 1 条）

t6 的完成时刻是 **23:04:58**。文件 mtime（不随时间漂移的判据）：

| 时段 | 文件 |
| --- | --- |
| **t6 窗内 22:50:59 → 23:01:37** | `0019` 22:50:59 · `0021` 22:51:29 · `0022` 22:51:58 · `0020` 22:54:28 · `0023` 22:56:55 · `migrations.rs` 22:58:20 · `lib.rs` 22:58:23 · `fts.rs` 22:58:27 · 报告 23:01:37 |
| **t6 之后 23:07:59 → 23:21:24** | `memory/src/dedupe.rs` · `write.rs` · `query.rs` · `lib.rs` · `inject.rs` · `lifecycle.rs` · `graph/src/lib.rs` · `knowledge/src/{rrf,store,lib}.rs` · `daemon/src/{distill,wiki,memembed}.rs` |

⇒ 今天 `git status --porcelain -- crates/knowledge crates/memory crates/graph crates/daemon panel` **非空**，但**没有一行能归到 t6**：全部晚于 23:04:58，是 t7–t10 的在途编辑（`panel/` 连他们都没碰）。t6 的改动面 = `crates/store` 三文件 + 5 个新迁移 + 报告，与 `changedPaths` 逐项相同。**验收第 1 条成立**（附 RV23-5 的口径建议）。

---

## 2 两处替代形式的判决（**必须成立，否则判失败**）

captain 已接受这两处替代，所以它们**不是失败理由**；但 t23 要求我复核它们**真的成立**，包括「视图非 0 行 / 触发器的拒绝面与声明不符」这类不成立形状。我的判决：**两处都成立，而且我复现的是必要性**。

### 2.1 第 8 条：「两个 CHECK」→ 2 个触发器 + 1 个视图（要求 0 行）

**(a) 视图是真读数，不是恒真。** 在**真实历史**的副本上（`wiki_builds` 8 行、`wiki_build_pages` 41 行子行）：迁移后视图 = **0 行**；而我把一条刚插入的 plan-only 行（`planned_only`/`dry_run=1`/`finished_at` 空，正是 `wiki.rs:1642` 的形状）插进去后视图 **0 → 1**，跑完 `wiki.rs:1845` 那一条 `finish_dry_run`（只补 `finished_at`）后回 **0**。⇒ 「要求 0 行」这个判据**能被证伪**，而且这条 0→1→0 本身就是「那个 CHECK 不可能存在」的证明。

**(b) 拒绝面与声明逐格一致**（我自己的四格转移矩阵，每格**各自新建一行**再装触发器；第一次跑时我复用了被修好的行，读数错，已改正）：

| 旧状态 | → 新状态 | 我的读数 | 声明 |
| --- | --- | --- | --- |
| `planned`/1（合规） | `done`/1（不合规） | **REJECTED** `wiki_builds: plan-only status and dry_run must agree` | 拦 ✔ |
| `planned`/1 | `planned`/0 | **REJECTED**（同上） | 拦 ✔ |
| `done`/1（已坏） | `done`/0（修回合规） | **ACCEPTED** | 不冻结历史行 ✔ |
| `done`/1 | `planned`/1（修回合规） | **ACCEPTED** | ✔ |
| `done`/1 | `done`/1（其它字段） | **ACCEPTED** | ✔ |
| `planned_only`/1 | `planned`/1（词汇归一） | **ACCEPTED** | ✔ |
| `planned`/1 | `done`/1（先插一条坏行再修） | **REJECTED** | ✔ |
| `dry_run=NULL` 的插入 | — | REJECTED 的是 `NOT NULL constraint failed: wiki_builds.dry_run`（与配对无关，且 `0010` 本来就 NOT NULL） | 无冲突 ✔ |

产品侧语句重放（crate 测试 `wiki_builds_invariant_accepts_every_product_write_and_rejects_bad_pairs`，我跑绿；另用**我自己抄的 raw SQL** 复跑）：干跑插入、真跑插入、`finish_dry_run`、`finish_build`、`fail_build`、通用字段写**全部 ACCEPTED**；三个该被拒的写**全部 REJECTED**。⇒ **不会拒掉任何一条产品写**，拒绝面恰好是声明的那一面。

**(c) 必要性：表重建在这条路径上真的跑不了**（我在**我自己迁移过的副本**上复现，子行 41 行）：

```
PRAGMA foreign_keys=1 | wiki_builds 8 行 | wiki_build_pages 41 行
plain DROP TABLE wiki_builds                    -> IntegrityError: FOREIGN KEY constraint failed
BEGIN; PRAGMA foreign_keys=OFF -> 值仍 = 1        （事务内 no-op）
in-transaction DROP TABLE wiki_builds           -> IntegrityError: FOREIGN KEY constraint failed
```

**(d) 回填是回填、不是发明**（我逐行比对源库与副本）：id 7,8 `planned_only` → `planned`；id 1,4 的 `finished_at` **恰好等于 `started_at`**（`2026-09-14T18:52:56.430373700+00:00` / `19:25:44.102165+00:00`），id 2,3,5,6 的 `finished_at` 一个字节未动。迁移文件里还留有**回填前 8 行的逐行现场值**（我读到的源库值与注释逐行一致）。

**(e) 零消费者改动**：`panel/` 空；t6 时间窗内 `crates/daemon` 无任何文件被写（§1.4）。

⇒ **判决：替代形式成立**（且是必要的），不作为失败理由。

### 2.2 第 9 条：「3 个触发器」→ 4 个（`au_fill` 只插 / `au_change` 先删后插）

**必要性我自己复现了**：我用**规格草案那版**（单个 `AFTER UPDATE OF grams`，无 NULL 守卫）建库，走 `grams` 唯一的填充路径：

```
insert（grams 仍 NULL）        -> MATCH "研磨" = 0
NULL -> 值 的回填（UPDATE）     -> DatabaseError: database disk image is malformed
此后 MATCH "研磨"               -> 0      （该行永久不入索引）
```

而**交付版**（4 个触发器）的四步（插 / 填 / 改值 / 删）由 crate 测试 `cjk_bigram_index_follows_a_null_to_value_backfill` 走通 —— 我跑绿，且它每步用 **MATCH**（external-content 表的普通 `SELECT` 读的是 `chunks`，不看索引），改值那步断言**旧值 0 命中、新值 1 命中**（重复投递会让同一 rowid 出现两次）。我自己的自建库也确认 4 个触发器具名在位（`chunks_cjk_ai/au_fill/au_change/ad`）。

⇒ **判决：替代形式成立且必要**，不作为失败理由。

---

## 3 十条验收逐条判决

判据**只对齐 t6 契约里的十条**，不引入规格外的新要求。

| # | 验收（压缩） | 我的独立读数 | 判决 |
| --- | --- | --- | --- |
| A1 | 只改 `crates/store/**` + 报告；`git status` 证明无越界 | store 的 status = 恰为 `changedPaths` 九项；`panel/` 空；越界路径的最早 mtime 是 23:07:59，**晚于** t6 完成（23:04:58）与报告落盘（23:01:37） | **达成** |
| A2 | 四份规格的每条 store 请求都落地或有证据 no-op（逐条引规格） | §3 表 38 行 = A7+B9+C10+D8+E4（我数的）；**对象面独立核对**：从四份规格里解析出的每个 `CREATE TABLE`/`CREATE VIRTUAL TABLE`/`CREATE INDEX`/`ADD COLUMN` 名字**逐个在我自建库里存在**（`query_eval_sets/gold/runs`、`chunks_fts_cjk`、`idx_query_eval_gold_set`、`chunks.grams`、4 个 recall_log 列、`communities`、`community_entities`、`entity_aliases`、`resolution_pending`、`distill_log.status`、`entities.embedding`、`entity_edges.event_time_source`、`wiki_pages/citations/graph_readings/corrections` + 3 个 wiki 索引）；**6 条可证伪 no-op 我自己复现**（见下） | **达成** |
| A3 | 只做加法，不改历史迁移 | `git diff --name-only HEAD -- crates/store/src/migrations` **空**；`0001–0018` 在 status 里既无 `M` 也无 `??`；`MIGRATIONS` 只在 0018 之后追加 5 行 | **达成** |
| A4 | 新 schema 在全新库可应用，有 count / `table_info` 读数 | §1.3 的整张表：我用**只含 `.sql` 的独立建库**得到 23 / 59 / 1 / 13 / 18，12 表 12⁄12，`recall_log` 24 列，24 列 notnull/dflt 逐列读出 | **达成** |
| A5 | 历史行新列保持 NULL；NULL ≠ user；说明渲染层如何区分 unknown | 副本上 **13 列**逐列 `NULL = 总数` 且 `notnull=0 / dflt=NULL`（§4 表）；自建库 24⁄24；报告 §5 给出 `unknown (pre-NNNN)` 字面量与「不得回落成 `0`/`''`/`ok`/`human`」的清单；**既有反例 `lib.rs:189`（`selected_by` NULL → `"human"`）t6 未改而登记为未修**（§4 末，活库 **16/21** 行受影响） | **达成** |
| A6 | `cargo test -p ruagent-store` 与 clippy 通过 | 32 passed / 0 failed / 0 ignored（exit 0）；clippy 零诊断（exit 0）；`#[ignore]`/`#[should_panic]` 无命中 | **达成** |
| A7 | 报告给出改前 → 改后读数（迁移数、表/列清单、测试读数） | 18 → 23（我自建库：18 版 `MAX=18` / 23 版 `MAX=23`）· 12 表 + 1 虚表 + 1 视图 + 24 列 · **改后 32 passed 是我自己跑的**；「改前 20 passed」这一侧我**没有**在纯净 HEAD 树上重跑（那是 V-SCHEMA/t22 的读数，我引用它并标明来源）；**同表里「新触发器 改前 8」是错的（实为 7，两个独立读数）→ RV23-1**，Delta `+6` 正确 | **达成**（附 RV23-1） |
| A8 | wiki DDL-1..7 走路线 B：保留 `dry_run` + 两个 CHECK + legacy 回填，零消费者改动；路线 A 记为后续 | 替代形式**成立**（§2.1）：`dry_run` 列在；2 个触发器 + 1 个视图（要求 0 行，真历史 0/8 且 0→1→0 可证伪）；拒绝面逐格与声明一致；legacy 回填逐行核对；`panel/` 与 `daemon/` 在窗内零改动；路线 A（DDL-5/N-5）报告记为后续 | **达成**（替代形式） |
| A9 | recall 0019 按其规格落地（3 表 + 4 列 + `chunks.grams` + `chunks_fts_cjk` + 3 触发器），只 ADD 不重解释 `top_knowledge_score` | 3 表 + `idx_query_eval_gold_set` 在；4 个 recall_log 列在（可空无默认）；`chunks.grams` 在、`chunks_fts_cjk` 在；触发器 4 个且**必要性我复现**（§2.2）；`top_knowledge_score` 未动：**源库与副本 max 均 `0.032786883413791656`、非空均 651、列位均第 10** | **达成**（替代形式） |
| A10 | R-E K-2 的 6 列用追加迁移（建议 `0023_recall_telemetry.sql`）；列落地前 A-3b 记 not_measured | `0023_recall_telemetry.sql` 存在，文件内**恰好 6 条** `ALTER TABLE recall_log ADD COLUMN`（`score_kind`/`fusion`/`top_legs_json`/`candidates_json`/`selected_json`/`rejected_json`）；`recall_log` 24 列（源库 12 列，**六列一个都不存在**）；六列可空无默认，历史行 NULL | **达成**（附 RV23-4 口径说明） |

### 3.1 A2 的 6 条可证伪 no-op（我自己复现）

| 声明的 no-op | 我的独立读数 | 成立？ |
| --- | --- | --- |
| `memory_diffs.op` 无 CHECK ⇒ 新 op 值不需要迁移 | 自建库 DDL：`op TEXT NOT NULL, -- insert \| supersede \| skip_dedupe \| reject`，**表 DDL 内无 CHECK** | ✔ |
| `episodes.kind` 无 CHECK | `kind TEXT NOT NULL`（注释列 4 个值），无 CHECK | ✔ |
| `wiki_build_pages.status` 无 CHECK（新取值无需迁移） | `status TEXT NOT NULL`，无 CHECK；活库 `pending` 22/41（我读到 41 行子行） | ✔ |
| 不建 `entity_sources` | 自建库里 `entity_sources` 不存在；`entity_edges.source_episode` 在 `0005` | ✔ |
| 不建 `(src, invalid_at)` 索引，已存在超集 | `idx_edges_src ON entity_edges(src, relation, invalid_at)` **确实存在** | ✔ |
| `residual_scan` / `document_path` 不是 store 的对象（归 I-A） | 全仓 grep：`impl Knowledge` 在 `crates/knowledge/src/store.rs` 与 `files.rs`，且这两个函数**现在就在那里**（`store.rs` 里 `document_path` / `residual_scan`）—— 即 t6 把归属判给 I-A 是对的，落点也确实不在 store | ✔ |

> 诚实声明：我**没有**把四份规格（合计 2600+ 行）的 38 条逐条重读。我实测的是**对象面**（规格点名的 DDL 名字 vs 自建库逐字存在）、**6 条可证伪 no-op**、以及 no-op 的**符号存在性**（A-6/D-8 那一类）。这与 V-SCHEMA 的 U-3 是同一档覆盖，我在 §6 记为未测项，不冒充全量。

---

## 4 NULL 纪律与 `selected_by` 登记

**13 个「有历史行」的新列，在**我自己的副本**上逐列读数**（`notnull` / `dflt` 直接来自 `PRAGMA table_info`）：

| 表.列 | NULL / 总数 | notnull | dflt |
| --- | --- | --- | --- |
| `memories.access_count` | **163/163** | 0 | NULL |
| `memories.last_used_at` | **163/163** | 0 | NULL |
| `memories.valid_from` | **163/163** | 0 | NULL |
| `memories.valid_to` | **163/163** | 0 | NULL |
| `distill_log.status` | **33/33** | 0 | NULL |
| `distill_log.failure_reason` | **33/33** | 0 | NULL |
| `distill_log.prompt_hash` | **33/33** | 0 | NULL |
| `entity_edges.event_time_source` | **67/67** | 0 | NULL |
| `entity_edges.fact_hash` | **67/67** | 0 | NULL |
| `chunks.grams` | **10765/10765**（迁移后未回填）/ 0（回填后） | 0 | NULL |
| `recall_log.knowledge_leg_window` | **651/651** | 0 | NULL |
| `entities.embedding` | **63/63** | 0 | NULL |
| `entities.embedder` | **63/63** | 0 | NULL |

任务单点名的四组（`access_count` / `event_time_source` / `distill_log.status` / `valid_from|valid_to`）**全部 all-NULL 且 nullable 无默认**；另外 24 个新增列在自建库上 **24/24** 都是 `notnull=0 且 dflt=NULL`（我另断言「任何新增列带 DEFAULT」= 空集）。⇒ **「没把没测当通过」**：NULL 纪律不是靠新表（新表无历史）撑的，是靠有历史的 6 张表 13 列逐一读出来的。

**`selected_by` 那条登记是不是诚实的？—— 是。** 三条独立读数：

1. 代码仍在：`crates/store/src/lib.rs:189` `by.unwrap_or_else(|| "human".into())`，其上注释 `// NULL selected_by = a selection made before migration 0012.`（**正是**验收第 5 条点名的 `NULL ≠ user`）。
2. **t6 没有顺手改它**：`git diff -U0 HEAD -- crates/store/src/lib.rs` 的 hunk 只有三处（`@@ -797,0 +798,56 @@`、`@@ -1071,2 +1127 @@`、`@@ -1094,0 +1150,247 @@`）—— 全在 798 之后，**与 189 行无关**。
3. 报告把它登记为**未改**：§5 末（「在仓里发现的一处既有违反（登记，本单不改）」）+ §8 的 N-4「未测/未改（登记）」，并写明改它要动 `orchestrator`/`runs` 消费方（不在 t6 的 inScope）。
4. 我补一个**爆炸半径**读数（报告没给）：活库 `tasks` **16/21** 行的 `selected_by IS NULL` ⇒ 这处渲染回落今天会让 16 行历史被读成 `"human"`。**它不是本单缺陷**（验收第 5 条要的是「新列的 NULL 语义 + 说明渲染层区分方式」），但它是一条真实的、有量的数据错误，值得单独一单（finding RV23-6）。

---

## 5 findings

> 全部 **low / 观察**，无 medium 及以上。我**没有**改任何被评审代码（不在本单 inScope，也不该替作者修）。

**RV23-1（low，记账）报告 §1「新触发器 | 8 | +6」的改前基数应为 7。**
· 复现：`python %TEMP%\rv23_p2_fresh.py head`（只用 `0001..0018` 建库 → 触发器 **7** 个）与**活库直读**（`schema_migrations` max=18 的源库：`SELECT COUNT(*) FROM sqlite_master WHERE type='trigger'` = **7**，具名 `chunks_ad/ai`、`entities_ad/ai/au`、`memories_ad/ai`）。
· 期望/实际：报告写 8；实际 7。**Delta `+6` 正确**（7 → 13，我自建库两侧都读了）。
· 影响：`docs/design/reviews/gen2-store-impl.md` §1 表里一个改前基数。验收第 7 条点名的三项读数（迁移数 18→23、表/列清单、测试 20→32）**都不受影响**，故不构成 needs_revision。建议改成 7，或改写为「+6（13 总数）」。

**RV23-2（low，仪器）`filled=0` 时打印的 `ms/row` 是无意义的除零产物。**
· 复现：`RUAGENT_T6_LIVE_COPY=%TEMP%\rv23-live-copy-A.db cargo test -p ruagent-store --lib tests::live_copy_bigram_backfill_at_scale -- --exact --nocapture`（**跑两遍**）→ 第二遍打出 `filled=0 elapsed=38ms (38.27 ms/row)`。
· 根因：`crates/store/src/lib.rs:1313` `elapsed.as_secs_f64() * 1000.0 / filled.max(1) as f64`。
· 影响：一个读数行会让人以为第二遍做了很慢的工作（真相是它一行都没处理，而这**正是幂等的证据**）。建议 `filled == 0` 时打印 `n/a`。

**RV23-3（low，采样面措辞）「86 条自语料派生的 2 字汉字子串」实际来自**单个 chunk**。**
· 复现：读 `crates/store/src/lib.rs:1338-1350` —— `if seen.len() >= 25 { break; }` 位于**外层 `for text in rows`** 内、**内层 token 循环之后**，所以第一个合格 chunk 能一次灌进全部 86 个 token 再退出（86 > 25 就是这么来的）。
· 影响：**等价性结论不受影响**（我用副本独立跑：索引 vs `LIKE` 在 86 子串上 0 处不一致；我另做 2 字抽样 `并发/方法/查询/日志` = 138/138、612/612、259/259、205/205 **逐位相等**），但「自语料的采样」这个措辞比实际抽样面更强。建议把上限判定移进内层，或改写措辞。

**RV23-4（观察，口径继承）「R-E 14 列」标签与可枚举的 12 个新列不符（t6 照抄契约，不是它发明的）。**
· 复现：我数 `recall_log` 的新列 = 0019 的 4 + 0021 的 2 + 0023 的 6 = **12**；`recall_log` 总数 **24 = 原 12 + 新 12**（源库 12 列，六列一个都不存在）。
· 影响：验收第 10 条点名的**六列逐个在位**，A10 成立；标签冲突属契约侧（V-SCHEMA 的 U-6 已登记）。我不引入新要求，只登记。

**RV23-5（观察，边界归因）t6 契约里的那条「无越界」命令今天**跑不出空**，判据应换成 mtime。**
· 复现：`git status --porcelain -- crates/knowledge crates/memory crates/graph crates/daemon panel` 现在非空，但每一行的 mtime 都 ≥ 23:07:59，**晚于** t6 完成（23:04:58）与报告（23:01:37）；`panel/` 至今为空。
· 影响：无缺陷。提示后续验证/评审单改用「哪些路径的 mtime 落在被评审单的时间窗内」这种**不随时间漂移**的判据（我本单就是这么判 A1 的）。

**RV23-6（观察，既有违反已诚实登记，但有一条真实的数据错误待单）`lib.rs:189` 的 `selected_by` NULL → `"human"` 仍未修。**
· 复现：读 `crates/store/src/lib.rs:186-190`；`git diff -U0 HEAD -- crates/store/src/lib.rs` 证明 t6 未触碰该行；报告 §5 末 + N-4 登记为「未改」；活库 `tasks` **16/21** 行 `selected_by IS NULL`。
· 影响：**登记诚实**（没有被当成已修），故不判 t6 失败；但它是「NULL ≠ user」的一处真实违反，建议单独一单（消费方 `orchestrator`/`runs` 不在 t6 inScope）。

**RV23-7（观察，可证伪边界）1 字汉 query 与 `LIKE` **按设计不等价**，不得把「0 处不一致」读成「bigram 腿可替代 LIKE 腿」。**
· 复现：在我回填后的副本上 `MATCH` vs `LIKE`：`方` 0/1748、`件` 0/1831、`日` 21/333、`变` 1/648、`茶` 0/6；而 2 字子串逐位相等（见 RV23-3）。
· 原因：`han_bigrams` 对长度 1 的汉字 run 存**该字自身**，长 run 里的字**只以 bigram 存在** ⇒ 1 字命中是 `LIKE` 的**子集**。
· 影响：无缺陷（`LIKE` 阶段仍是完备路径）。登记它是给 **I-A/t7** 的注意项：keyword 腿的四段降级链里，1 字汉 query 仍只能靠 LIKE。

---

## 6 未测 / 不判定（不许静默跳过）

| # | 项 | 状态 | 原因 | 谁解 |
| --- | --- | --- | --- | --- |
| U-1 | 迁移在**真守护进程下一次启动**时应用是否成功 | **未测** | 不许启停 pid 79984；我只在副本上走了等价路径（同一 `apply` 代码 + 同量级真实数据 651/10765/163/33/67 行） | INT/t19 重启窗口（V-INT/t20） |
| U-2 | 四份规格 38 条**逐条**重导 | **部分测** | 我实测对象面（规格点名的 DDL 名字 vs 自建库）、6 条可证伪 no-op、no-op 的符号存在性；**未逐行重读 2600+ 行规格** | 若需全量，另开一单 |
| U-3 | `0023` 六列的**值语义**（`score_kind="rrf_rank"`、`rejected_json.graph_no_seed` 等） | **未测（列在 ≠ 值在）** | 写入侧在 `api.rs`（INT/t19）；本单只判「列存在、可空、无默认、历史 NULL」 | INT/t19 + V-INT/t20 |
| U-4 | `chunks.grams` 的**磁盘增量**与回填耗时口径 | **未测** | 耗时我读了（1486 ms / 0.14 ms/row）但它是负载相关的；磁盘增量与语料相关 | I-A/t7 / V-A（R-A §G7 口径） |
| U-5 | 路线 A（删 `dry_run`） | **不采纳（规格 P1-later）** | 需要 §2.1(c) 的表重建 | 后续 |
| U-6 | `wiki_build_pages` 22 行 `pending` 的清理 | **不在迁移里（规格明文）** | 报告 D-5 引原文；我在副本上确认 41 行子行、22 行 pending | I-D/t10 |
| U-7 | 真库上 `fail_build` 是否**曾经**命中干跑行 | **未测** | 库里 8 行状态里没有 `failed` 的干跑行，但这不等于「从未发生」；新触发器落地后该路径在代码上不可达（`start_build_inner` 干跑分支在 `wiki.rs:774` 就 return） | 无 |

---

## 7 复现命令（我实际跑的）

```powershell
# 0) 环境（我自己的 target dir；绝不用 D:/rust_cache）
$env:PATH = "$env:USERPROFILE\.protoc\bin;$env:PATH"
$env:CARGO_TARGET_DIR = "$env:TEMP\ruagent-review-vschema"
cd C:\Users\19410\Documents\ai\ruagent

# 1) 契约命令
cargo test -p ruagent-store                                        # 32 passed / 0 failed / 0 ignored (exit 0)
cargo clippy -p ruagent-store --all-targets -- -D warnings         # Finished，零诊断 (exit 0)
git status --porcelain -- crates/store                             # M fts.rs lib.rs migrations.rs + ?? 0019..0023
git status --porcelain -- panel                                    # 空
git diff --name-only HEAD -- crates/store/src/migrations           # 空（历史迁移一字未改）
git diff --numstat HEAD -- crates/store/src/fts.rs                 # 207 2

# 2) 假通过面（先清掉环境变量）
Remove-Item Env:RUAGENT_T6_LIVE_COPY -EA 0
cargo test -p ruagent-store --lib live_copy -- --nocapture --test-threads=1   # 2 行 [t6] NOT MEASURED + ok (exit 0)

# 3) 我自己的副本（只读源库 -> VACUUM INTO 到 %TEMP%）
python %TEMP%\rv23_p1_copy.py          # 源库 sha256/mtime/行数/新对象 0/14 + 造副本 A

# 4) live 读数（指向我自己的副本，绝不指向 ~/.ruagent）
$env:RUAGENT_T6_LIVE_COPY = "$env:TEMP\rv23-live-copy-A.db"
cargo test -p ruagent-store --lib migrations::tests::live_copy_keeps_history_nullable -- --exact --nocapture  # 8 行读数
cargo test -p ruagent-store --lib tests::live_copy_bigram_backfill_at_scale -- --exact --nocapture            # 跑两遍看幂等

# 5) 我自建的库（只用 .sql，不经过 crate）与替代形式复核
python %TEMP%\rv23_p2_fresh.py head    # 0001..0018: 42 表 / 7 触发器 / 10 索引
python %TEMP%\rv23_p2_fresh.py         # 0001..0023: 59/1/13/18, 12 表, 24 列 notnull=0 且 dflt=NULL
python %TEMP%\rv23_p3_copy.py          # 副本：13 列 NULL 读数 + PRAGMA notnull/dflt + top_knowledge_score 对照
python %TEMP%\rv23_p4_rejection.py     # 拒绝面 + 回填出处（finished_at == started_at）
python %TEMP%\rv23_p5_specobjects.py   # 规格点名的 DDL 名字 vs 自建库
python %TEMP%\rv23_p6_matrix.py        # UPDATE 触发器的六格转移矩阵
python %TEMP%\rv23_p7_necessity.py     # 必要性：FK 中止 / 事务内 pragma no-op / 规格版触发器 malformed
python %TEMP%\rv23_p8_noop.py          # 6 条 no-op + 活库触发器 7 + selected_by 16/21
python %TEMP%\rv23_p9_boundary.py      # 1 字 vs 2 字汉 query 的索引/LIKE 边界
```

---

## 8 纪律回执

* **只写本单 inScope**：本文件 `docs/design/reviews/gen2-schema-review.md`。`crates/` 与 `panel/` **一行未改**（评审员不是作者；findings 只登记，不代修）。
* **读数三件套**：每条读数带**对象集**（我自建的 18 版/23 版库 · 我的副本 A · 只读源库 651/10765/163/33/67/63/8 行）、**采样面**（13 列 NULL、24 列 notnull/dflt、6 条 no-op、六格转移矩阵、1/2 字汉 query）、**可证伪判据**（视图 0→1→0；坏写 REJECTED / 好写 ACCEPTED；规格版触发器报错）。期望值只写一处（本报告 §3/§4），探针从库与文件现场取，**没有复制常量**。
* **建议的满意度判决**：`pass`。十条验收逐条达成，两处 captain 替代形式我复核为**成立且必要**，findings 全部 low/观察，**无 medium 及以上**。
* **未测一律写明**（§6，7 项）。
* **真守护进程 pid 79984 未启停**；`~/.ruagent/data/ruagent.db` 全程 `mode=ro`，所有探针跑完后 **sha256 与 mtime 逐位相同**；未使用 `GET /api/v1/recall`。
* **编译**：所有 `cargo` 命令都在自己的 target dir 下完成（clippy 零诊断），**没有**对任何 Rust 文件的改动。
* **同伴在途编辑**：`crates/memory/**`、`crates/graph/**`、`crates/daemon/**`、`crates/knowledge/**` 在复核期间有改动（文件名与 mtime 已在 §1.4 列出）；**未归因给 t6，也未替同伴改动**。
