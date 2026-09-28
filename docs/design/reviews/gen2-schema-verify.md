# V-SCHEMA 独立验证（t22）：t6 / I-SCHEMA 的十条验收、两处替代形式与「不破坏产品」重放

> **状态**：独立复核**完成**，结论 **pass**（6 条 low/观察 finding，**无 medium 及以上**）。
> **验证者**：verify2（独立验证员，**不是**被验对象的作者）。本单只写本文件；`crates/store` 一行未改。
> **时间窗**：2026-09-27T22:56 → 23:16 +08:00。
> **被验产物（工作树 0a39e5b8，未提交）**：`crates/store/src/migrations/0019..0023_*.sql`（5 个新文件）+ `migrations.rs` / `fts.rs` / `lib.rs` + 作者报告 `docs/design/reviews/gen2-store-impl.md`。
> **构建树**：`CARGO_TARGET_DIR=$env:TEMP\ruagent-verify-vschema`（当前工作树）与 `$env:TEMP\ruagent-verify-vschema-head`（`git archive HEAD` 导出到 `$env:TEMP\v22-head` 的纯净树，**不是** `git worktree`，不动 `.git`）。
> **环境纪律**：真守护进程 **pid 79984 未启停**；`~/.ruagent/data/ruagent.db` 全程 `mode=ro`，**sha256 与 mtime 前后逐位一致**（§2.6）；所有写操作发生在 `%TEMP%\v22-*.db` 副本上。

---

## 0 一句话

**t6 的十条验收全部独立复现成立**，含 captain 已接受的两处替代形式 —— 而且我复现的是它们的**必要性**（不是只看到「东西在」）：wiki 的 `DROP TABLE` 在 `foreign_keys=ON` 下被即时外键中止、`PRAGMA foreign_keys=OFF` 在事务内确实是 no-op、那个 CHECK 确实会拒掉**每一次** dry-run 构建；规格那版单个 `AFTER UPDATE OF grams` 触发器在**唯一**填充路径（NULL→值回填）上确实报 `database disk image is malformed`，而 4 触发器版在 填/改/删 四步上索引与 `LIKE` 逐位一致。差异只有 3 处**记账/口径**（§3：触发器「改前 8」实为 7；「14 列」实际枚举 12 列；`ms/row` 在 filled=0 时是无意义的除零产物），**不动摇任何一条验收**。

---

## 1 待验清单

### 1.1 t6 的十条验收（逐条，编号沿用任务单顺序）

| # | 验收（原文压缩） | 本单的独立判据 |
| --- | --- | --- |
| A1 | 只改 `crates/store/**` + 报告文件；`git status` 证明无越界 | `git status --porcelain`；`git diff --stat HEAD -- crates/store/src/migrations`；文件 mtime |
| A2 | 四份规格的每条 store 请求都落地或有证据 no-op | 12 表/1 视图/24 列/6 触发器/8 索引的**自建库**清点 + 6 条可证伪 no-op 逐条复现（§2.5） |
| A3 | 只做加法：不改历史迁移文件 | `git diff` 对 `.../migrations/` 为空；0001–0018 在 status 里既无 `M` 也无 `??` |
| A4 | 新 schema 在全新临时库可应用，有 count / `table_info` 读数 | 我用 Python 的 sqlite3 **独立**从 23 个 `.sql` 建库（不经过 crate）；`MAX(version)`、表/视图/触发器/索引计数、`PRAGMA table_info` |
| A5 | 历史行新列保持 NULL，NULL ≠ user，说明渲染层如何区分 | 副本 A 上 **13 个新列**逐列 NULL 计数 + `notnull/dflt_value` |
| A6 | `cargo test -p ruagent-store` 与 clippy 通过 | 我的 target dir 重跑；**另在纯净 HEAD 树上**重跑得到「改前」读数 |
| A7 | 报告给出改前→改后读数 | 18→23 与 20→32 两个基线我都自己重跑 |
| A8 | wiki DDL 路线 B（captain 替代：2 触发器 + 1 视图，要求 0 行） | 视图 0 行（真历史 8 行）+ 三处「不该过」的写全被拒 + **替代形式的必要性复现** |
| A9 | recall 0019（captain 替代：3→4 触发器） | 4 个触发器在 schema 里逐个具名 + NULL→值→改值→删除四步索引逐步走通 + 规格版触发器失败的复现 |
| A10 | R-E K-2 六列用追加迁移落地 | `0023` 六个 `ADD COLUMN`、六列在 `recall_log` **列尾**、可空无默认、源库（18 版 / 12 列）本来没有它们 |

### 1.2 captain 已接受的替代形式（**不得当失败理由**，但必须自己复现成立）

* ①「两个 CHECK」→ **2 个触发器 + 1 个视图（要求 0 行）**；
* ②「3 个触发器」→ **4 个**（`au_fill` 只插 / `au_change` 先删后插）。

### 1.3 我额外加的三条反「假通过」面（本单自己要求）

* F1：不设 `RUAGENT_T6_LIVE_COPY` 时，两个 live-copy 测试**打印 `[t6] NOT MEASURED` 后通过** ⇒ 判据是「有没有打出 `[t6]` 读数行」，不是退出码（§2.2）。
* F2：`视图 = 0 行` 这个判据在空库上**恒真** ⇒ 必须同时证明它**能变成非 0**（§4.2）。
* F3：「不破坏产品」不能只看作者的白名单 ⇒ 我从 `wiki.rs` 里**逐条抄出真实 SQL** 重放（§2.5、§4.3）。

---

## 2 独立读数（我自己的构建树、自己的 target dir、自己的副本）

### 2.1 契约命令

| 命令 | 我的读数 | exit |
| --- | --- | --- |
| `$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"; $env:CARGO_TARGET_DIR="$env:TEMP\ruagent-verify-vschema"; cargo test -p ruagent-store` | `test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured`（+ doc-tests 0）；`ruagent_store-4d07e171ff807ebb.exe` | **0** |
| 同上 clippy：`cargo clippy -p ruagent-store --all-targets -- -D warnings` | `Finished dev profile ... in 11.86s`，**零 warning / 零 error** | **0** |
| `git status --porcelain -- crates` | **非空**，但**没有一行是 store 之外的 t6 改动**（见 §2.6 与 finding F5） | 0 |
| `git diff --stat HEAD -- crates/store/src/migrations` | **空**（历史迁移一字未改） | 0 |
| `git status --porcelain -- crates/store/src/migrations` | 只有 5 个 `?? 0019..0023` | 0 |
| 「改前」基线：`$env:TEMP\v22-head`（`git archive HEAD` 导出）+ `CARGO_TARGET_DIR=$env:TEMP\ruagent-verify-vschema-head`，`cargo test -p ruagent-store` | `test result: ok. 20 passed; 0 failed`，且迁移目录只有 `0001..0018` | **0** |

⇒ 作者的「改前 20 → 改后 32」与「迁移 18 → 23」**我在两棵不同的构建树上各自重跑复现**，不是复述。
⇒ `crates/store/src/*.rs` 里**没有** `#[ignore]` / `#[should_panic]`（`Select-String` 无命中）⇒ 「0 ignored」是结构性的，不是碰巧。

### 2.2 假通过处理：两个 live-copy 测试（F1）

**不设环境变量的实跑**（`--nocapture --test-threads=1`，我的 target dir）：

```
running 2 tests
test migrations::tests::live_copy_keeps_history_nullable ... [t6] NOT MEASURED: set RUAGENT_T6_LIVE_COPY=<a migrated copy of the live db> to take the history-NULL reading (never point this at ~/.ruagent: the test APPLIES migrations, i.e. it writes)
ok
test tests::live_copy_bigram_backfill_at_scale ... [t6] NOT MEASURED: set RUAGENT_T6_LIVE_COPY=<a copy of the live db> for the at-scale backfill reading. The test APPLIES migrations and WRITES grams, so it must never be pointed at ~/.ruagent.
ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 30 filtered out; finished in 0.04s
```

⇒ **绿 ≠ 测到**：两行 `[t6] NOT MEASURED` 都在，退出码 0。作者在报告 §7.3 与交接里点名了这个形状，我**按「有没有打出 `[t6]` 读数」判**，并且下面的读数全部来自**我自己 `VACUUM INTO` 的副本**，从未指向 `~/.ruagent`。

**源库 → 副本（Python `sqlite3`，从只读连接，含 WAL）**：

| 副本 | 大小 | `schema_migrations` max | `recall_log` 行 | `chunks` 行 |
| --- | --- | --- | --- | --- |
| `%TEMP%\v22-live-copy-A.db` | 14,188,544 B | 18（副本） | 651 | 10765 |
| `%TEMP%\v22-live-copy-B.db` | 14,188,544 B | 18（副本） | 651 | 10765 |

### 2.3 副本 A：指向它跑 `live_copy_keeps_history_nullable`（我看到了 8 行 `[t6]` 读数）

```
[t6] live copy: wiki_builds rows=8 unfinished_plans=0
[t6] live copy: memories.access_count NULL 163/163
[t6] live copy: memories.last_used_at NULL 163/163
[t6] live copy: memories.valid_from NULL 163/163
[t6] live copy: distill_log.status NULL 33/33
[t6] live copy: entity_edges.event_time_source NULL 67/67
[t6] live copy: chunks.grams NULL 10765/10765 (all-NULL = freshly migrated, none-NULL = backfilled)
[t6] live copy: recall_log.knowledge_leg_window NULL 651/651
test migrations::tests::live_copy_keeps_history_nullable ... ok
```

### 2.4 副本 B：指向它跑 `live_copy_bigram_backfill_at_scale`（两次调用）

```
第一次： [t6] live copy: chunks=10765 grams_missing_before=10765 filled=10765 elapsed=2195ms (0.20 ms/row)
        [t6] live copy: bigram-index vs LIKE equivalence on 86 derived 2-char Han substrings -> 0 disagreement(s) []
第二次（同一副本）： chunks=10765 grams_missing_before=0 filled=0 elapsed=61ms (61.15 ms/row)
        [t6] live copy: bigram-index vs LIKE equivalence on 86 derived 2-char Han substrings -> 0 disagreement(s) []
```

⇒ **幂等**：第二次 `filled=0` 且 `grams IS NULL = 0`（我另用 Python 直读副本 B 确认）。
⇒ 耗时与作者不同（我 2195 ms / 0.20 ms/row，作者 1430 ms / 0.13 ms/row）——**同一台机器不同时刻的负载差**，不是读数冲突；行数读数（10765 / 0）逐位相同。
⇒ `filled=0` 时那个 `ms/row` 是 61.15 的**除零产物**（分母 `filled.max(1)`）—— 见 finding F3。

### 2.5 我自己写的探针（**不经过 crate**，Python `sqlite3` = SQLite 3.50.4，FTS5 可用）

探针脚本在 `%TEMP%\v22_p*.py`（本单只读仓内文件，脚本落在临时目录）。它们构成一条**第二条独立实现路径**：库我自己建、SQL 我自己抄、样本我自己采样。

**(a) 全新库：我只用 23 个 `.sql` 文件建库（`v22_p2_fresh.py`、`v22_p2c_legacy.py`）**

| 读数 | 值 |
| --- | --- |
| `.sql` 文件数 / `MAX(schema_migrations.version)` | **23 / 23** |
| 表 / 视图 / 触发器 / 具名索引 | 59 / 1（`wiki_builds_unfinished_plans`）/ 13 / 18 |
| 12 张新表 | **12/12** 存在 |
| 虚拟表 | `chunks_fts_cjk` 存在（external content；另生成 4 张 shadow 表） |
| 新增具名索引 | **10 → 18 = +8**，报告点名的 8 个逐个命中 |
| 新增触发器 | **7 → 13 = +6**（4 个 cjk + 2 个 wiki 配对） |
| `recall_log` 列数 / 列序 | **24**，与测试 pin 的顺序逐位一致 |
| 24 个新增列 `notnull=0 且 dflt_value IS NULL` | **24/24 成立，0 例外** |
| 被迁移**丢掉**的表 | `[]`（额外空表的唯一差是 `schema_migrations` 由 runner 单独建、我第一版脚本没建它——探针自身产物，已说明） |
| 24 列按表分布 | `recall_log` 12 · `memories` 4 · `distill_log` 3 · `entity_edges` 2 · `entities` 2 · `chunks` 1 = **24** |

**(b) `wiki.rs` 真实语句重放（我从 `crates/daemon/src/wiki.rs` 逐条抄，行号标在脚本里）**

| 语句 | 出处 | 结果 |
| --- | --- | --- |
| S1 `insert_build` 干跑（`planned_only`, `dry_run=1`, `finished_at` 空） | `wiki.rs:1642`（`:756` 调用） | **ACCEPTED** |
| S2 `insert_build` 真跑（`running`, `dry_run=0`） | `wiki.rs:1642`（`:805`/`:848` 调用） | **ACCEPTED** |
| S3 `finish_dry_run` 只补 `finished_at`（**另一条语句**） | `wiki.rs:1845` | **ACCEPTED** |
| S4 `finish_build` → `done` + 计数 + `finished_at` | `wiki.rs:1716` | **ACCEPTED** |
| S5 `fail_build` → `failed`（先插一条 `running` 行） | `wiki.rs:1762` | **ACCEPTED** |
| S6 `update_build` 通用字段写（`pages_planned`） | `wiki.rs:1750`（`:867` 调用） | **ACCEPTED** |
| S7 `insert_build_pages`（`status='pending'`） | `wiki.rs:1669` | **ACCEPTED** |
| S8 `set_page_status` | `wiki.rs:1740` | **ACCEPTED** |

**三个该被拒的写（显式判决，不是布尔翻转）**：

```
REJECTED  R1 INSERT status='done', dry_run=1         [wiki_builds: plan-only status and dry_run must agree]
REJECTED  R2 INSERT status='planned', dry_run=0      [同上]
REJECTED  R3 UPDATE 合规干跑行 -> status='done'       [同上]
==> all three rejected: True
```

**`fail_build` 会不会误伤干跑行（唯一可能被触发器中止的真实产品路径）**：不会。`fail_build` 的唯一产品调用点在 `wiki.rs:875`，位于 `start_build_inner` 的**非干跑**分支（干跑在 `:774` 就 `return Ok(...)`；`:1976` 那处在 `#[cfg(test)]` 里）⇒ 触发器不可能中止它。

**「已坏的 legacy 行仍然可修」（UPDATE 触发器只拦「合规→不合规」）**：把两个触发器临时摘掉、造一行 `status='done', dry_run=1` 再逐字重建触发器后：

```
ACCEPTED  repair: legacy 'done'/dry=1 -> 'planned'（修回合规）
REJECTED  freeze: 合规干跑行 -> 'done'（正是要拦的那一下）
```

⇒ 「不冻结历史缺陷、只保护合规行」这两半**都**测到了。

**(c) 0023 的六列 = `ADD COLUMN`（`v22_p34_copy.py`、`v22_p2_fresh.py`）**

* 源库（`schema_migrations=18`）`recall_log` **12 列**，**六列一个都不存在**；副本 A / 全新库 **24 列**。
* 六列位于列序**最末**、且顺序与 `0023` 文件里 6 条语句的先后**逐位一致**：`score_kind, fusion, top_legs_json, candidates_json, selected_json, rejected_json`。
* 六列 `notnull=0`、`dflt_value=NULL`；历史行 `knowledge_leg_window` NULL **651/651**。
* `0023_recall_telemetry.sql` 内恰好 **6** 条 `ALTER TABLE recall_log ADD COLUMN`（文件 50 行 / 3270 B）。
* 判据的强度说明：SQLite 在 `ALTER TABLE ADD COLUMN` 时会重写 `sqlite_master.sql`，所以**单看表的 DDL 文本区分不出「原生」与「加列」**；我把判据落在「**源库（18 版）根本没有这些列**」+「列序在最末」+「文件里就是 6 条 ADD COLUMN」三点上。

**(d) NULL 纪律：13 个新列在副本 A 上的逐列读数**

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
| `chunks.grams` | **10765/10765**（副本 A，未回填） | 0 | NULL |
| `recall_log.knowledge_leg_window` | **651/651** | 0 | NULL |
| `entities.embedding` | **63/63** | 0 | NULL |
| `entities.embedder` | **63/63** | 0 | NULL |

任务单点名的四组（`access_count` / `event_time_source` / `distill_log.status` / `valid_from|valid_to`）**全部 all-NULL、全部 nullable 无默认**。`non-NULL` 计数在每一组都是 **0**。规则引用的 `0018` 原文我也读了：`-- it from the request. NULL is left NULL: it means UNKNOWN, not "user".`

**(e) 我自己的 bigram 样本（**不复用作者样本**）**

* 采样面：`chunks` 全长按 `id` 排序，**每 37 行取一行**，在其中用**我自己的 Han 判据**（`0x3400-0x4DBF | 0x4E00-0x9FFF | 0xF900-0xFAFF`）取相邻 2 字窗口，取到 **60 个互异**子串为止（作者的是「第一个合格 chunk 的 86 个」，`lib.rs:1339-1354` 的 25 上限是在外层行循环里判的）。
* 我的样本前 12 个：`并发 发锁 方法 法区 条件 件查 查询 询员 员工 日志 志变 变动`。

| 判据 | 我的读数 |
| --- | --- |
| `MATCH '"XY"'`（索引） vs `content LIKE '%XY%'`（全表扫），**60 个子串** | **0 处不一致** |
| `MATCH` vs **存储列自身的 token**（`' '||grams||' '` 含 `" XY "`），同 60 个 | **0 处不一致**（索引与列不脱节） |
| 3 字查询（短语化 bigram，10 个样本） | **0 处不一致** |
| 语料外子串（`舙갋` / `鿿囍` / `鬻龘`） | 索引 **0**、LIKE **0**（负例两侧都为 0，判据不是恒真） |
| **1 字汉 query** | **按设计不等价**：索引 ≤ LIKE，例 `方` 0/1748、`件` 0/1831、`日` 21/333、`变` 1/648（1 字 run 存自身，长 run 里的字只以 bigram 存在）——见 finding F6 |

**(f) no-op 声明的独立复现（验收 A2 的可证伪子集）**

| 声明的 no-op | 我的独立读数 | 成立？ |
| --- | --- | --- |
| B-5 `memory_diffs.op` 无 CHECK ⇒ 新 op 值无需迁移 | 全新库 DDL 是裸 `op TEXT NOT NULL`；活库 **7 个**不同 op（`delete/insert/purge/reject/restore/skip_dedupe/supersede`），注释只列 4 个 | **成立** |
| B-7 `episodes.kind` 无 CHECK | DDL 裸 `kind TEXT NOT NULL` | **成立** |
| D-5 `wiki_build_pages.status` 无 CHECK（新取值无需迁移） | DDL 裸 `status TEXT NOT NULL`；活库 `pending` **22 行**（规格说清理归产品路径） | **成立** |
| C-9 不建 `entity_edges(src, invalid_at)`：已有超集 | `idx_edges_src ON entity_edges(src, relation, invalid_at)` 确实存在（另有 `idx_edges_dst`） | **成立** |
| C-8 不建 `entity_sources` | 全新库无此表；`entity_edges.source_episode` 已在 0005 | **成立** |
| A-6 `residual_scan`/`document_path` 不是 store 的对象 | `grep` 全仓：`impl Knowledge` 在 `crates/knowledge/src/store.rs:125` 与 `files.rs:187`；这两个函数名**尚未存在**（属 I-A/t7）。「不在 store」这个归属判断成立 | **成立** |

### 2.6 源库未被碰过（贯穿全单）

| 判据 | 探针 1（**任何副本之前**） | 探针 8（**全部探针 + 两次 live-copy 测试之后**） |
| --- | --- | --- |
| `sha256` | `51539f88c4f901b148cd0ab05c232de84baff2f8f6d9db54b3366494b3c50aa1` | **同一位逐字节相同** |
| `mtime` | `2026-09-27T21:15:21.078444` | **相同** |
| 大小 | 14,946,304 B | 相同 |
| `schema_migrations` max | **18**（18 行） | **18** |
| `recall_log` 列数 / 行数 | **12** / 651 | 12 / 651 |
| `chunks` 列数 / 行数 | **7** / 10765 | 7 / 10765 |
| 新对象（`query_eval_sets`/`wiki_pages`/`memory_sources`/`entity_aliases`/`chunks_fts_cjk`） | 全部不存在（0/5） | **仍然 0/5** |
| 其它行数 | `memories` 163 · `distill_log` 33 · `entity_edges` 67 · `entities` 63 · `wiki_builds` 8 · `wiki_build_pages` 41 | 相同 |

⇒ 「只读 + 写副本」不是声明，是**两个时间点的 sha256 相同**证明的；也顺带证明**把 `RUAGENT_T6_LIVE_COPY` 指向副本时测试确实没写源库**。

**活库 `wiki_builds` 8 行（迁移前，我只读抄下）**：`1,4 = planned/1/finished_at NULL`；`7,8 = planned_only/1/finished_at 已设`；`2,3,5,6 = done/0` —— 与 `0022` 注释里登记的回填前现场值**逐行一致**（8 行、id 7/8 是 `planned_only`、id 1/4 是 `finished_at NULL`）。
**迁移后（副本 A）**：8 行全部自洽，`wiki_builds_unfinished_plans` = **0 行**，配对违规 = **0 行**；id 1/4 的 `finished_at` 落回 `started_at`（`2026-09-14T18:52:56Z` / `19:25:44Z`），id 7/8 由 `planned_only` → `planned`。

### 2.7 「只改 store + 报告」在我复核时的形状（A1）

`git status --porcelain -- crates` **此刻非空**：`crates/memory/**`、`crates/graph/**`、`crates/daemon/src/distill.rs` 上有**其他同伴的在途编辑**（I-B/t8、I-C/t9）。这**不是** t6 的越界，证据：

* `crates/store` 的文件 mtime 全落在 t6 的时间窗内，且**没有更晚的触碰**：`0019 → 22:50:59`、`0023 → 22:56:55`、`migrations.rs → 22:58:20`、`lib.rs → 22:58:23`、`fts.rs → 22:58:27`、报告 `gen2-store-impl.md → 23:01:37`。
* `crates/knowledge` 干净；`crates/store/src/migrations/` 只有 5 个 `??`；`git diff --stat HEAD -- crates/store/src/migrations` **空**。
* 我**不**把 `crates/memory`/`graph`/`daemon` 的改动记到 t6 账上（纪律：全 workspace 的在途编辑只报告「哪个文件」，不替同伴改、也不归因）。
* 收尾时同伴名单还在变（又多了 `crates/daemon/src/wiki.rs`、`crates/daemon/src/memembed.rs`、`crates/knowledge/**`）。其中 `wiki.rs` 是**我的重放引用面**，我另做了文本级复核 ⇒ 见 **§10 addendum**。

---

## 3 差异表：「作者读数 → 我的独立读数 → 差异」

| 项 | 作者读数 | 我的独立读数 | 差异 |
| --- | --- | --- | --- |
| 迁移数 / `SCHEMA_VERSION` | 18 → **23** | 源库 18；全新库 23 个文件、`MAX(version)=23`；副本迁移后 23 | 无 |
| `cargo test -p ruagent-store` | 20 → **32 passed / 0 failed** | **32 passed / 0 failed / 0 ignored**（我的 target dir）；**纯净 HEAD 树 20 passed** | 无（两棵树各自的基线我都自己跑） |
| clippy `-D warnings` | 干净 | `Finished`，零诊断，exit 0 | 无 |
| 新表 12 | 12 | **12/12** + 虚拟表 + 1 视图 | 无 |
| 新虚拟表 `chunks_fts_cjk` | 有 | 有（另 4 张 shadow 表） | 无 |
| 新视图 | `wiki_builds_unfinished_plans` | 有，真历史 **0/8** | 无 |
| `ALTER` 新列 24（12/4/3/2/2/1） | 24 | **24**，按表分布逐位相同，`notnull=0` + 无默认 **24/24** | 无 |
| 新触发器「**8** → +6」 | 改前 **8**，+6 | 改前（0001–0018 自建库）**7**，改后 13，**+6** | **差 1**：`8` 应为 `7`；`+6` 正确 → finding **F1** |
| 新索引 +8 | +8 | 10 → 18，8 个具名逐个命中 | 无 |
| 副本 NULL 读数 | `163/163` · `33/33` · `67/67` · `651/651` | 同上，且扩到 **13 列**（另 `valid_to 163/163`、`fact_hash 67/67`、`failure_reason/prompt_hash 33/33`、`embedding/embedder 63/63`） | 无（只是更全） |
| 全量回填 | `10765` 行 / `1430 ms (0.13 ms/row)`，第二次 **0** | `10765` 行 / `2195 ms (0.20 ms/row)`，第二次 **0**（`grams IS NULL=0`） | 行数/幂等无差异；耗时是负载差。`filled=0` 时打印的 `61.15 ms/row` 是除零产物 → finding **F3** |
| bigram vs LIKE | 86 子串 → **0 不一致** | 作者的测试：86 → 0；**我自己分层采的 60 子串 → 0**；索引 vs 存储列 → 0；3 字 10 样本 → 0；负例 0/0 | 无（样本族不同而结论一致）。作者样本实际来自**单个 chunk** → finding **F4** |
| `0023` 六列 | 落地、可空无默认 | 六列在**列尾**、顺序吻合、源库（12 列）本无此六列、文件内恰好 6 条 `ADD COLUMN`、历史 651/651 NULL | 无 |
| 「R-E 14 列」 | 14/14 落地 | `recall_log` **24 列**（原 12 + 新 12）成立；但 §3.1 表里 `[新增]` 条目**数出来是 12**（4+2+6） | **口径差 2**：标签「14」与枚举不符，契约侧继承而来，**不动 schema** → finding **F2** |
| `top_knowledge_score` 未被重解释 | 一个字节未改 | 24 列序里该列原样在第 11 位；0018 的 NULL 规则文本原样 | 无 |
| 「不破坏产品」 | 重放 + 三个拒写 | 我**自己抄的** S1–S8 全 ACCEPTED；R1/R2/R3 全 REJECTED；legacy 可修/合规被护两半都测到；`fail_build` 干跑不可达 | 无 |
| 「历史迁移一字未改」 | 只追加 | `git diff --stat HEAD -- crates/store/src/migrations` **空**；status 无 `M` | 无 |
| 源库未被碰 | max 18 / mtime 21:15:21 / 12 列 | **sha256 前后逐位相同** + 上述全部读数 | 无（我多了 sha256 这一层） |
| 冻结四函数逐字节未动 | 未动 | `terms`/`match_all`/`match_any_prefix`/`like_patterns` + 私有 `quote`/`recallable` **逐字节相同**（`git show HEAD:...` 抽出函数体比对）；`fts.rs` numstat `207+/2-`（删的两行是注释行与 `const`→`pub const`） | 无 |
| 逐条清单 38 = 落地 27 + no-op 11 | 38 = 27 + 11 | 条目数 **38** 对得上；**D-8 一行同时算「落地（文件号）」与「no-op（不加 Db 方法）」**，所以「27」含一半重复计数 | 记账口径说明（不列入 finding，属可接受的混合处置登记） |

---

## 4 可证伪性复核（判据能不能红）

### 4.1 替代形式 ①（2 触发器 + 1 视图）的**必要性**，我自己复现了

在**真实历史**的副本上（`wiki_build_pages` **41** 行子行、`wiki_builds` 8 行）：

```
(a1) PRAGMA foreign_keys=ON 下 DROP TABLE wiki_builds
     -> IntegrityError: FOREIGN KEY constraint failed          （复现报告理由 (a)）
(a2) BEGIN; PRAGMA foreign_keys=OFF; -> SELECT PRAGMA foreign_keys 仍 = 1（事务内 no-op）
     DROP 仍然 FK 中止；ROLLBACK 后仍 = 1                       （复现 (a) 的「pragma 不可切换」）
(a3) 把「plan-only ⇒ finished_at 非空」写成 INSERT 时判据后跑 wiki.rs:1642 的原语句
     -> REJECTED: CHECK failed: plan-only => finished_at NOT NULL
     （⇒ 那个 CHECK 会拒掉每一次 dry-run 构建：captain 的理由 (b) 成立）
```

而**视图这个替代判据不是恒真**（F2）：故意插一行刚建的 plan-only 行，视图从 **0 → 1**，跑完 `finish_dry_run` 那一条语句后回 **0**。这正是「CHECK 不可能存在」的证明，也说明「要求 0 行」确实能被证伪。

### 4.2 替代形式 ②（4 触发器）的**必要性**，我自己复现了

全新库上先把填充路径走完整（**用 `MATCH` 读索引**，因为 external-content 表的普通 `SELECT` 读的是 `chunks`）：

```
[4 触发器，已交付] 插入 grams=NULL -> MATCH 咖啡 0 / integrity ok
                   NULL->值 回填   -> MATCH 咖啡 1 / integrity ok
                   改值            -> MATCH 咖啡 1、MATCH 研磨 0（旧值消失、无重复投递）/ integrity ok
                   删除            -> MATCH 咖啡 0 / integrity ok
[规格草案] 换成单个无 NULL 守卫的 AFTER UPDATE OF grams：
                   NULL->值 回填   -> DatabaseError: database disk image is malformed
                   并且此后 MATCH 水温 = 0（该行永久不在索引里）
```

⇒ 报告 §4.1 说的「'delete' 半支会对从未插入索引的记录执行 FTS5 delete」**逐字复现**；4 个触发器不是形式变化，是修一个会报错的路径。

### 4.3 其余判据的红/绿两侧

| 判据 | 好侧（绿） | 坏侧（红） |
| --- | --- | --- |
| live-copy 读数 | 指向我自己的副本 → 8 行 `[t6]` 读数 | 不设环境变量 → **只打印 NOT MEASURED**，退出码仍 0（判据不看退出码） |
| `wiki_builds` 配对不变量 | S1–S8 全过、`planned/1` 也被接受 | R1/R2/R3 全被拒（触发器消息具名） |
| 视图 0 行 | 迁移后真历史 0/8 | 刚插入的 plan-only 行 → 1 行 |
| NULL 纪律 | 13 列全 NULL | 若哪次 `ALTER` 带了 `DEFAULT`，`notnull/dflt` 与 NULL 计数会同时红（我两处都读） |
| 六列存在 | 副本/新库都在 | 源库（18 版）本来就没有 ⇒ 「存在」不是在源库上误判的 |
| 索引 vs LIKE 等价 | 60 + 86 + 10 个样本 0 不一致 | ①语料外子串两侧都 0；②1 字汉 query **明确不等价**（§2.5e）⇒ 判据未被过度声称 |
| 冻结函数 | 逐字节相同 | 若被改，`git show` 抽出的函数体会不同（我比的是函数体，不是文件） |
| 「改前」基数 | 纯净 HEAD 树 20 passed、18 个迁移 | 若「改前」不是 20/18，报告 §1 的改前→改后就不成立 |

---

## 5 逐条验收对照（t6 十条 × 我的读数）

| # | 我的独立判据 | 结论 |
| --- | --- | --- |
| A1 | store 文件 mtime 全在 t6 时间窗（22:50–22:58）且无更晚触碰；`crates/knowledge` 干净；store 改动 = 报告 `changedPaths` 那 9 项。**crates/memory / crates/graph / crates/daemon 有同伴在途编辑，不计入 t6** | **成立**（附 F5 口径说明） |
| A2 | 12 表 + 1 视图 + 24 列 + 6 触发器 + 8 索引的清点我自建库重做；6 条可证伪 no-op 逐条复现成立 | **成立**（38 = 27+11 的算术见 §3 末行说明） |
| A3 | `git diff --stat HEAD -- crates/store/src/migrations` 空；0001–0018 无 `M`/`??`；新文件只在 0019–0023 | **成立** |
| A4 | 只用 `.sql` 建库：23 文件 / `MAX=23` / 59 表 / 1 视图 / 13 触发器 / 18 索引；`table_info` 逐列读数 | **成立** |
| A5 | 13 列 NULL 计数 + `notnull=0` + `dflt_value=NULL`；`0018` 原文规则在库；渲染层区分方式在报告 §5（本单只判「NULL 与 0 在存储层可分」） | **成立** |
| A6 | 32 passed / 0 failed / 0 ignored，且**无 `#[ignore]`**；clippy 零诊断 | **成立** |
| A7 | 18→23、20→32 两个基线都在我自己的树上重跑 | **成立** |
| A8 | 视图 0/8；2 触发器在位；三条坏写全拒；S1–S8 全过；`fail_build` 干跑不可达；legacy 可修 | **成立**（替代形式，不作为失败理由） |
| A9 | 4 个 cjk 触发器具名在位；NULL→值→改值→删除四步索引正确；规格版触发器失败的复现 | **成立**（替代形式，不作为失败理由） |
| A10 | 六列在列尾、源库本无、6 条 `ADD COLUMN`、可空无默认、历史 651/651 NULL | **成立**（「14」标签口径见 F2） |

---

## 6 findings

> 全部为 **low / 观察**。**无 medium / high / blocker。** 我没有改任何被验代码（不在 inScope 里，也不该替作者修）。

**F1（low，记账）「新触发器 8 → +6」的「改前 8」实为 7。**
· 复现：`python %TEMP%\v22_p9_counts.py`（只用 `0001..0018` 建库后数触发器）
· 期望/实际：报告 §1 表写「新触发器 | 8 | +6」；实际 `0001..0018` 建出的库有 **7** 个触发器（`chunks_ad`/`chunks_ai`/`entities_ad`/`entities_ai`/`entities_au`/`memories_ad`/`memories_ai`），迁移后 **13** 个 ⇒ **增量 +6 正确**，改前基数多算了 1。活库（18 版）也是 7 个，两处互证。
· 影响：仅改前→改后表格的一个基数，不涉及 schema；验收 A2/A9 不受影响。

**F2（low，口径）「R-E 14 列」标签与可枚举的列数不符（12）。**
· 复现：`python %TEMP%\v22_p9_counts.py` + 读 `gen2-integration-contract.md:155-183`
· 期望/实际：契约与 t6 报告都写「14 列全部落地」，同时两处都把同一组枚举成 `0019 的 4 + 0021 的 2 + 0023 的 6`；我按 §3.1 表里 `[新增]` 条目数出来是 **12**（`top_knowledge_relevance`、`…_kind`、`knowledge_leg_window`、`scoring_version`、`graph_entities`、`graph_paths`、`score_kind`、`fusion`、`top_legs_json`、`candidates_json`、`selected_json`、`rejected_json`），而 `recall_log` 总数 **24 = 原 12 + 新 12** 与两份文档一致。
· 影响：**不动 schema**；验收 A10 点名的六列全在。属契约侧标签遗留，t6 照抄。建议在契约里把「14」改为「12」或补足缺的 2 列名（我没有权限改契约，故只登记）。

**F3（low，仪器）`filled=0` 时打印的 `ms/row` 是无意义的除零产物。**
· 复现：`RUAGENT_T6_LIVE_COPY=%TEMP%\v22-live-copy-B.db cargo test -p ruagent-store --lib tests::live_copy_bigram_backfill_at_scale -- --exact --nocapture`（第二次跑）
· 期望/实际：第二次输出 `filled=0 elapsed=61ms (61.15 ms/row)`。分母是 `filled.max(1)`（`lib.rs:1313`），所以「没有行要填」被渲染成「每行 61 ms」。`filled` 与 `elapsed` 本身都对。
· 影响：一个读数行会误导读者以为第二遍做了很慢的工作（真相是它什么都没做，且**这正是幂等的证据**）。建议 `filled==0` 时打印 `n/a`。

**F4（low，采样面）「86 条自语料派生的 2 字汉字子串」实际来自单个 chunk。**
· 复现：读 `lib.rs:1328-1354`；`seen.len() >= 25` 的判定在**外层 `for text in rows`** 之后，所以一个长 chunk 能一次贡献全部 86 个 token（此即 86 > 25 的原因），随后立刻 `break`。
· 期望/实际：措辞像是「语料级采样」，实际是「按 `id` 排序后第一个合格 chunk 的全部 bigram」。
· 影响：**结论仍然成立**——我自己按 `id` 每 37 行分层采样的 60 个子串也得 0 处不一致（§2.5e）。但若要「跨语料」的强度，采样循环应把上限判定放进内层。属措辞/采样面问题，不是等价性缺陷。

**F5（观察，边界归因）`git status --porcelain -- crates` 已不再是 t6 的越界证据。**
· 复现：`git status --porcelain -- crates`
· 实际：`crates/memory/**`、`crates/graph/**`、`crates/daemon/src/distill.rs` 有 I-B/I-C 的在途编辑。t6 契约里的那条命令在**它的时间点**为空是合理的，但在**今天**跑会让人误以为 t6 越界。
· 影响：无（我按文件 mtime + `changedPaths` 归因，见 §2.7）。提示后续验证单改用「哪些路径的 mtime 落在被验单的时间窗内」这种**不随时间漂移**的判据。

**F6（观察，可证伪边界）1 字汉 query **不等价**，且这是设计，不是缺陷。**
· 复现：`python %TEMP%\v22_p34_copy.py`
· 实际：`MATCH '"方"'` = 0 而 `LIKE '%方%'` = 1748；`件` 0/1831、`日` 21/333。原因：1 字 Han run 存自身，长 run 里的字只以 bigram 存在。
· 影响：无缺陷。登记它是为了**不允许**任何人把「0 处不一致」读成「bigram 腿能替代 LIKE 腿」：2 字与 3 字子串等价成立，1 字子串按设计是子集（LIKE 阶段仍是唯一完备路径）。

### 6.1 我**没有**发现的问题（正面确认）

* 视图是用读数而不是约束交付的这件事，**没有**削弱「不破坏产品」：S1–S8 全过 + R1–R3 全拒 + legacy 可修，两侧都测到。
* `0022` 的回填**没有**发明数据：它只把 `finished_at` 落回 `started_at`（干跑在计划生成那一刻即终结，`wiki.rs:767-773`），并在迁移注释里保留了回填前 8 行的逐行现场值——我读数与之逐行一致。
* 24 个新列**没有**任何一个带 `DEFAULT`（否则历史行会被背书）；新表则逐字保留规格的 `NOT NULL`/`DEFAULT`（`wiki_pages.verified DEFAULT 'unverified'`、`wiki_corrections` 的两个非空 CHECK）。
* 「一个迁移一事务」是真的：`migrations.rs` 的 `apply_one` 走 `conn.transaction()`，`a_failing_migration_rolls_back_completely` 用故意坏的迁移证明「第一句不留在盘上、版本行不写」——这条测试在我 32 passed 里跑着，且我读了它断言的三个量。
* `residual_scan`/`document_path` 记 no-op 并归 I-A 的判断，与规格原文（`impl Knowledge`）一致；`impl Knowledge` 确实在 `crates/knowledge`（`store.rs:125`、`files.rs:187`），只是这两个函数还没人写（I-A/t7 的活）。

---

## 7 未测项（不许静默跳过）

| # | 项 | 状态 | 原因 | 谁解 |
| --- | --- | --- | --- | --- |
| U-1 | 迁移在**真守护进程下一次启动**时应用是否成功 | **未测** | 不许启停 pid 79984；我仅在副本上走了等价路径（同一 `apply` + 同量级真实数据 651/10765/163/33/67 行） | INT/t19 的重启窗口（t20 的 A-3a） |
| U-2 | `chunks.grams` 的**磁盘增量** | **未测** | 需要语料相关的磁盘口径；本单的回填判据是「行数 + 幂等 + 等价性」 | I-A(t7) / V-A（R-A §G7 口径） |
| U-3 | 四份规格 **38 条请求逐条**重导 | **部分测** | 规格合计 2612 行；我复核的是**可证伪子集**：12 表/1 视图/24 列/6 触发器/8 索引的清点 + 6 条 no-op 的独立复现 + 冻结函数逐字节 + 编号分配（0019–0023 与 §2 一致）。**其余 32 条我一字未判**，本单不声称覆盖 | 若要全量，需另一单（且要读四份规格） |
| U-4 | `0023` 六列的**值语义**（`score_kind="rrf_rank"` 常量、`rejected_json.graph_no_seed` 等） | **未测（列在 ≠ 值在）** | 写入侧是 INT（`api.rs`），本单只判「列存在且可空无默认」；契约自己也写「列在 ≠ 值在」 | INT/t19 + V-INT/t20 |
| U-5 | `<root>/knowledge/<name>.md` 这类**盘上路径**语义（A-6） | **未测** | 属 I-A(t7) 的对象，不在 store，本单无判据 | I-A/t7 |
| U-6 | 「R-E 14 列」标签到底是 12 还是 14 | **未判（登记 F2）** | 要判它得先裁定契约 §3.1 的意图（是否还有 2 列未列入表）；我给出可枚举的 12 与总数 24 两个读数，把标签冲突留给契约 owner | 契约 owner / captain |
| U-7 | 真库上 `fail_build` 是否**曾经**命中过干跑行 | **未测** | 是历史行为问题：库里没有 `failed` 的干跑行（我读了 8 行状态），但这不等于「从未发生过」 | 无（新触发器落地后该路径在代码上不可达） |

---

## 8 复现命令（我一个不漏地用了这些）

```powershell
# 0) 环境
$env:PATH = "$env:USERPROFILE\.protoc\bin;$env:PATH"
$env:CARGO_TARGET_DIR = "$env:TEMP\ruagent-verify-vschema"      # 工作树
# 纯净 HEAD 树（证明「改前」）：git archive 不触 .git，不是 git worktree
$dst = "$env:TEMP\v22-head"; Remove-Item -Recurse -Force $dst -EA 0; New-Item -ItemType Directory $dst | Out-Null
git archive HEAD | tar -x -C $dst
$env:CARGO_TARGET_DIR = "$env:TEMP\ruagent-verify-vschema-head"  # 在 $dst 里跑

# 1) 契约命令
cargo test -p ruagent-store                                     # 32 passed (改前: 20 passed)
cargo clippy -p ruagent-store --all-targets -- -D warnings      # 干净
git status --porcelain -- crates
git diff --stat HEAD -- crates/store/src/migrations             # 空
git status --porcelain -- crates/store/src/migrations           # 仅 5 个 ??

# 2) 假通过面（不设环境变量 → 必须看到两行 NOT MEASURED）
cargo test -p ruagent-store --lib live_copy -- --nocapture --test-threads=1

# 3) 我自己的副本（绝不指向 ~/.ruagent）
python %TEMP%\v22_p1_copy.py          # VACUUM INTO 出 A/B 两份 + 源库 sha256 前后对照
python %TEMP%\v22_p2_fresh.py         # 只用 23 个 .sql 建库：清点 + S1..S8 重放
python %TEMP%\v22_p2b_verdicts.py     # R1/R2/R3 显式判决 + before/after 触发器索引数
python %TEMP%\v22_p2c_legacy.py       # legacy 可修 / 合规行被护 + 未丢表
python %TEMP%\v22_p34_copy.py         # 副本 A 的 13 列 NULL + 0023 六列 + 我的 bigram 样本
python %TEMP%\v22_p5_frozen.py        # 四个冻结函数逐字节比对
python %TEMP%\v22_p6_noop.py          # 6 条 no-op 独立复现
python %TEMP%\v22_p7_necessity.py     # 替代形式 ①② 的必要性（FK 中止 / 事务内 pragma no-op / 规格触发器报错）
python %TEMP%\v22_p7b_a3.py           # 只应用 0019..0023 的副本：CHECK 会拒 dry-run + 索引损坏读数
python %TEMP%\v22_p8_final.py         # 源库 sha256 终检 + 副本 B 版本 + store 文件 mtime
python %TEMP%\v22_p9_counts.py        # 0001..0018 自建库的触发器数 + 「14 列」枚举

# 4) live-copy 读数（指向我自己的副本）
$env:RUAGENT_T6_LIVE_COPY = "$env:TEMP\v22-live-copy-A.db"
cargo test -p ruagent-store --lib migrations::tests::live_copy_keeps_history_nullable -- --exact --nocapture
$env:RUAGENT_T6_LIVE_COPY = "$env:TEMP\v22-live-copy-B.db"
cargo test -p ruagent-store --lib tests::live_copy_bigram_backfill_at_scale -- --exact --nocapture   # 跑两遍看幂等
```

---

## 9 纪律回执

* **只写本单 inScope**：本文件 `docs/design/reviews/gen2-schema-verify.md`。`git status --porcelain` 里我这一轮**没有新增任何 crates/ 或 panel/ 的改动**；`crates/store` 一行未改（验证者不是作者，findings 只登记不代修）。收尾证据：`crates/store/src/**` 全部文件的 mtime **最新只到 22:58:27**（`fts.rs`），全部早于我任何一次操作（我的第一条命令在 22:56 之后，读数集中在 23:0x–23:1x）；这三个被改过的文件在我收尾时的 SHA256 前 16 位：`fts.rs 2655CB2B46048536`、`lib.rs CBF1195D929AF72B`、`migrations.rs 9568C164CB2EA97F`。
* **读数三件套**：每条读数带**对象集**（活库 651/10765/163/33/67 行；副本 A/B；全新自建库）、**采样面**（我的 bigram 样本：`id` 每 37 行走 1 行、我自己的 Han 判据、60 个互异 2 字子串；作者的 86 个另算）、**可证伪判据**（§4 逐条给红/绿两侧）。期望值只写一处（本报告 §3/§4），探针从库与文件取，**没有复制常量**（bigram 样本、NULL 计数、触发器数全部现场读）。
* **未测一律写明**（§7，7 项，含原因与归属）。
* **真守护进程 pid 79984 未启停；`~/.ruagent/data/ruagent.db` 只读且 sha256 前后一致**（§2.6）；取证未使用 `GET /api/v1/recall`（本单根本没碰 HTTP）。
* **编译**：所有 `cargo` 命令都在自己的 target dir 下完成，clippy 零诊断；**没有**精确单点替换式的代码改动（不需要：本单不编译任何新代码）。
* **同伴在途编辑**：`crates/memory/**`、`crates/graph/**`、`crates/daemon/src/distill.rs` 在复核期间有改动（文件名已列出），**未归因给 t6，也未替同伴改动**（F5）。
* **结论**：t6 十条验收**独立复现成立**，两处 captain 替代形式**成立且必要**，无 medium 及以上 finding ⇒ **pass**。

---

## 10 addendum：复核期间同伴改动了 `crates/daemon/src/wiki.rs`（我重放的 SQL 是否还成立）

我在 §2.5b 重放的 7 条产品 SQL 是**从工作树里抄的**，抄的时间约 23:0x。复核期间 I-D/t10 编辑了 `crates/daemon/src/wiki.rs`（`git diff --numstat` = **+230 / −4**，mtime 23:17:52），**行号整体下移**（我引用的 `1642/1716/1750/1762/1845` → 现在 `1871/1944/1978/1990/2073`）。所以必须回答：**我重放的文本还成立吗？**

判据：比**文本**（归一化空白后），不比行号。`python %TEMP%\v22_p10_addendum.py` 的读数：

| 语句 | HEAD | 现在 | 文本相同？ | 现在的行 |
| --- | --- | --- | --- | --- |
| `insert_build`（S1） | found | found | **IDENTICAL** | 1871 |
| `finish_build`（S2） | found | found | **IDENTICAL** | 1944 |
| `update_build`（S3） | found | found | **IDENTICAL** | 1978 |
| `fail_build`（S4） | found | found | **IDENTICAL** | 1990 |
| `finish_dry_run`（S5） | found | found | **IDENTICAL** | 2073 |
| `insert_build_pages`（S6） | found | found | **IDENTICAL** | 1897 |
| `set_page_status`（S7） | found | found | **IDENTICAL** | 1968 |

* `DRY_RUN_STATUS` 在 HEAD 与现在都是 `"planned_only"`。
* **t10 的这次编辑改动的行里，提到 `wiki_builds` 的有 0 行**（`git diff -U0` 过滤）⇒ 它没有动任何 `wiki_builds` 写入，也没有动 `wiki_builds` 的不变量面。
* ⇒ §2.5b/§4.3 的重放**对当前修订依然成立**；本报告正文引用的行号是 23:0x 那次读的，已在此给出新旧对照。
* 这条 addendum 本身就是一个**可证伪面**：谁把行号当判据（而不是把 SQL 文本当判据），在同伴持续编辑的工作树里就会得出错误的「重放已失效」结论。
