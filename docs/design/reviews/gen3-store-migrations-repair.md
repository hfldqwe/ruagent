# gen3 修复报告 — 迁移账本不再是「不可验证的事实」（t98）

> 上游：t94 审计 `docs/design/reviews/gen3-audit-store.md` 的 **S1（high）+ S2（high/medium）**（同一根因）+ **S7 声称面**，外加 captain 并入的 **t77 三项遗留**。
> 范围：`crates/store/src/`、`crates/store/Cargo.toml`、`crates/store/tests/`、本报告。**没有写活库**；**没有启停真守护进程 pid 79984**（全程 `alive StartTime=2026/9/27 5:35:37`）。
> 作者：verify（本单是**修复**，不是自评；独立评审请另起单）。

## 0 结论

**问题**（t94 读数）：迁移账本被当成事实。`MAX(version)` 让「中间的洞」被读成"之前全部应用过"（删 v3 ⇒ `rows=24`、v3 永不再应用、**health=True、日志无提示**）；而 25 个迁移里 **20 个不可重放**，所以一旦某行版本记录丢失但对象还在（恢复备份/合并库/手工整理），脚本重跑会直接炸掉启动（`index ux_query_eval_gold_set_query already exists`）。**一条测错了东西的绿测试**（`sqlite.rs:152` 的 *"Reopen: migrations are idempotent, data survives"*，实际只证明「25 行都在时的 no-op」）给这个状态盖了章。

**修完之后的读数**（全部我自己跑出来，见 §2）：

| 判据 | 今天（改后） |
| --- | --- |
| 25 个迁移可重放 | **25/25**（`every_migration_can_be_reapplied_without_changing_the_schema` 逐个重跑 25 个脚本，`sqlite_master` 快照**逐字节不变**；改前 20/25 会 raise） |
| 删 `version=MAX` 后重启 | **`health=True`**，账本回到 `rows=25 min=1 max=25`（改前：`health=False` + `index … already exists`） |
| 中间挖洞（删 v1..v24 任一） | **`health=False` + 逐字点名**：`schema ledger is not usable: schema_migrations is not contiguous: version(s) [3] are missing while version 25 is recorded (24 row(s) present). …`（改前：`health=True`、静默、那个迁移永不应用） |
| 挖洞后恢复该行 | **`health=True`**（红→绿两条读数都有） |
| 从零建库 | `health=True`、`GET /api/v1/health` 200、`GET /api/v1/tasks` 200、账本 25/1/25 |
| 旧库升级（合成 v18 + 遗留数据） | `health=True`、账本 18 → **25**、遗留 task/run 行数与 `selected_by='agent:judge'` **完好**、0019–0025 的新对象都在 |
| 门禁 | `test -p ruagent-store` **exit 0**（lib 40 passed / 0 failed / 3 ignored；集成 3 passed）；`clippy -p ruagent-store --all-targets -- -D warnings -CleanFirst ruagent-store` **exit 0 / 0 warnings** |

**顺带**：`crates/store/Cargo.toml` 的 description 不再声称 LanceDB（向量层在 `crates/knowledge`）；t77 的**恒真断言**改成了真的 before/after 比较并给了「破坏不变量 ⇒ 该断言失败」的读数；t77 的另两项经精确复查**本来就已经满足**（`#[ignore]` 裸用法 0 处；全仓同绑定自比较 0 处），详见 §1.6。

## 1 改了什么

### 1.1 账本连续性校验（S2）——`crates/store/src/migrations.rs:59`（`apply`）

`MAX(version)` 仍是"要不要跑"的判据，但在**动手之前**先核对账本本身：

```rust
let versions = ledger_versions(conn)?;                    // :125  升序读全部版本行
let current  = versions.last().copied().unwrap_or(0);
let known_max = current.min(SCHEMA_VERSION);              // 上界=本二进制所知的最高版本
let missing: Vec<i64> = (1..=known_max).filter(|v| !versions.contains(v)).collect();
if !missing.is_empty() { return Err(DbError::Ledger(format!(… "version(s) {missing:?} are missing while version {current} is recorded" …))); }
```

- **为什么上界要 `min(SCHEMA_VERSION)`**：一条**未来版本**的行（降级场景，例如 999）如果把 `current` 抬到 999，`1..=999` 会把 1..998 全部当成"洞"⇒ 降级路径被误拒。现在：未来版本 = **WARN 并继续**（`tracing::warn!` 点名 unknown 版本，见 `:98`），本版本范围内的缺行 = **拒绝启动**。这条是集成测试 `a_future_version_is_not_a_hole` 逼出来的（第一次跑就是红的，见 §6 事故 0）。
- **为什么拒绝而不是自动补**：中间缺行有两种可能（"行丢了"vs"对象被删了"），而**重放一个中间迁移是在更晚的迁移之上动手**——0024 会 DROP + RENAME `distill_log`。拒绝并点名，是这个位置唯一诚实的选择；恢复动作（补行或换库）写在错误信息里。
- 新错误变体：`crates/store/src/sqlite.rs:19-26` 的 `DbError::Ledger(String)`（`#[error("schema ledger is not usable: {0}")]`）。加变体是安全的：全仓对 `DbError` 只有构造与 `matches!`，没有穷举 `match`（读数：`grep DbError::(Closed|Sqlite|Io)|match .*DbError` 命中 7 处，全是构造/局部匹配）。

### 1.2 25 个迁移可重放（S1）——`apply_script` / `statements` / `parse_guard` / `statement_already_present`

两层，**探针式**，不是吞错：

1. **SQL 层**（14 个 `.sql`、**72 个语句**）：`CREATE TABLE|VIRTUAL TABLE|[UNIQUE] INDEX|VIEW|TRIGGER` 全部加 `IF NOT EXISTS`，`DROP TABLE|VIEW|INDEX` 加 `IF EXISTS`。列守卫之外，脚本里剩下的 `CREATE/DROP` **未守卫数 = 0**（普查见 §2.6）。
2. **runner 层**（SQLite 表达不了的部分）：
   - `statements()`（`:207`）按语句切分，**trigger 体保持完整**（`CREATE TRIGGER … BEGIN … END;` 直到 `END;` 才收）。
   - `parse_guard()`（`:253`）+ `statement_already_present()`（`:327`）：只认下面这些形状，**别的一律照跑**（真错误照抛）：
     - `CREATE TABLE/VIRTUAL TABLE/INDEX/UNIQUE INDEX/VIEW/TRIGGER [IF NOT EXISTS] <name>` ⇒ 对象已存在则跳过；
     - `ALTER TABLE <t> ADD [COLUMN] <c>` ⇒ `pragma_table_info` 里已有该列则跳过（**这是 SQLite 没有 `ADD COLUMN IF NOT EXISTS` 的那 ~36 条**）；
     - `DROP TABLE/VIEW/INDEX [IF EXISTS] <name>` ⇒ 对象不存在则跳过。
   - `rebuild_already_done()`（`:188`）：**0024 的整脚本前置探针**。0024 是 DROP + RENAME 的整表重建（`distill_log_attempts` → `distill_log`），逐语句守卫**无法**安全表达；探针是对象图上的事实——`idx_distill_log_session` 是 0024 在 RENAME **之后**才建的索引，它存在 ⇔ 重建已完成。为真则整脚本跳过。
3. **顺手修的一处真数据风险**：`crates/store/src/migrations/0012_task_selected_by.sql` 的回填原本是
   `UPDATE tasks SET selected_by='human' WHERE selected_run_id IS NOT NULL;` ⇒ 重放会把**后来**由 judge 写的 `agent:<name>` 出身改回 `human`（`store::set_selected_run` 两列一起写；`lib.rs:186` 明确 "NULL selected_by = 0012 之前的选择"）。现在补上 `AND selected_by IS NULL`（新库路径语义完全等价，因为 `ALTER TABLE` 刚加列时全为 NULL）。回归测试：`reapplying_0012_does_not_clobber_a_judges_provenance`（`:1835`）。

### 1.3 「不要静默」的机械保险——5 个新单元测试 + 1 个新集成测试文件（`:1699` 起）

| 测试 | 它替代的旧读法 |
| --- | --- |
| `every_migration_can_be_reapplied_without_changing_the_schema`（:1699） | 把「删任一版本行仍能启动」变成 CI 里可重复的读数：25 个脚本各重跑一次，`sqlite_master` 快照必须逐字节相同、账本行数必须不变 |
| `a_non_reappliable_statement_is_still_an_error`（:1734） | **守卫不能吞错**：认识的形状（重复 `CREATE TABLE`）跳过；**不认识的形状（`INSERT` 主键冲突）必须照抛** |
| `a_ledger_hole_is_refused_by_name`（:1762） | 挖洞 ⇒ `DbError::Ledger`、消息里出现 `3` 与 `contiguous`、**且什么都没被应用**（行数 = 24） |
| `an_intact_ledger_is_a_silent_no_op_and_a_lost_maximum_is_reapplied`（:1793） | 完好 ⇒ 静默 no-op；丢最高行 ⇒ 重放并恢复账本 |
| `statements_splits_trigger_bodies_whole_and_leaves_no_fragment`（:1866） | 切分器不许把 trigger 体切开/合并，且不留无终止符的碎片 |
| `the_guard_parses_the_shapes_it_claims_to`（:1897） | 守卫的解析面逐形状钉死（含"`INSERT` 必须保持未守卫"） |
| `crates/store/tests/ledger_reopen.rs`（新文件，3 个测试） | 用**守护进程实际走的入口** `Db::open`（`sqlite.rs:31`）验：丢最高行仍能开、挖洞按名拒绝 + 补回后能开、未来版本不算洞 |

### 1.4 t77 遗留 ①：恒真断言改成真的 before/after

`migrations.rs:1576-1588`（原 `:1548-1549`）：

```rust
// 改前（断言不可能是假的，而且正好在"证明别的断言是空洞的"那个测试里）
assert!(distill_rows >= distill_rows);        // after >= before, 0 >= 0
assert_eq!(distill_rows, distill_rows);       // after == before, 0 == 0

// 改后：两个真读数，围绕一次真的第二次 apply（就是那个副本）
let before = distill_rows;                       // 迁移后的输入上测一次
let after = { let mut pass = rusqlite::Connection::open(&path)…; apply(&mut pass)…;
              count_ok(&pass, "SELECT COUNT(*) FROM distill_log")… };   // 再跑一遍后测一次
assert!(upgrade_preserved_rows(before, after).is_ok(), "… {}", upgrade_preserved_rows(before, after).unwrap_err());
```

不变量被抽成纯谓词 `upgrade_preserved_rows(before, after)`（`:1650`）：丢行/凭空多行都返回 `Err`。它有两层可证伪性证明：

- **永久**：`the_upgrade_row_invariant_can_fail`（`:1664`）——`(0,0)`/`(3,3)` 通过，`(3,2)`/`(3,4)` 必须失败；
- **定时变异（capitan 要求的读数）**：把 `let before = distill_rows;` 改成 `+ 1`，同一条真断言**确实变红**（§2.5，窗口 2 **5.1 秒**）。

### 1.5 t77 遗留 ②③：精确复核，结果是"本来就已经满足"

- **`#[ignore]` 裸用法**：`crates/store/src` 共 **3 个** `#[ignore` 属性（`lib.rs:1290`、`migrations.rs:775`、`migrations.rs:904`），**全部带理由字符串且点名 env 变量**；**裸形式（`^\s*#\[ignore\]\s*$`）= 0**，全仓裸形式也 **= 0**。⇒ **无需改动**。为什么先前会数出"2 处裸"：`migrations.rs:771` 与 `:901` 的**文档散文**里写着 `` `#[ignore]`d ``（讲 t33 的历史），未锚定的模式（`#\[ignore`，我 t94 的普查用的就是它）会把它们当成属性 —— 记在 §6 教训里，锚定模式已写进本报告。
- **全仓同绑定自比较**：Python 真正则（ripgrep 不支持反向引用）扫 **95 个 `.rs`**：命中 **2** 处，**都在我新写的注释里**（`:1567-1568` 引用"改前长什么样"），**活断言 0 处** ⇒ 与 captain 的预期一致。

### 1.6 S7 声称面更正

```diff
-description = "Repository traits and SQLite/LanceDB/JSONL implementations"
+description = "SQLite repositories (single-writer actor + migrations) and JSONL transcripts; the LanceDB vector layer lives in ruagent-knowledge (t98)"
```

`crates/store/src` 里 LanceDB 引用 **0 处**；真正的向量层在 `crates/knowledge`（`lancedb 0.38`）。同时把「**本仓不使用 `PRAGMA user_version`**（fresh root 实测 `pragma_user_version = 0`，crates/store 里 grep 命中 0）」写在这里与 t94 报告里：版本在 `schema_migrations(version INTEGER PRIMARY KEY, applied_at)` 表里。

## 2 读数（判据逐条，全部自己跑）

### 2.1 S1：25/25 可重放（测试层，可重复）

`every_migration_can_be_reapplied_without_changing_the_schema`：`apply` 后对 25 个脚本逐个 `apply_script` → 无错；`sqlite_master` 快照（`type,name,sql` 排序）**逐字节相同**；账本行数不变。改前同一动作的逐版本普查是 `RAISES = 20`（只有 v7/v13/v14/v16/v17 因为顺手用了 `IF NOT EXISTS` 而幸存）。

### 2.2 S1：守护进程级（我自己的临时 root + 端口 8819）

```
EX-ZERO  from-zero: health=True  ledger rows=25 min=1 max=25
         GET /api/v1/health -> 200 (35 bytes)   GET /api/v1/tasks -> 200 (12 bytes)
EX-MAX   punched vMAX: rows=24 max=24
         after restart: health=True   ledger rows=25 min=1 max=25      <-- 判据命中
```

### 2.3 S2：挖洞点名 + 完好静默（含红→绿两条）

```
EX-HOLE  punched v3: rows=24 max=25
         after restart: health=False  exited=True
         stderr: schema ledger is not usable: schema_migrations is not contiguous:
                 version(s) [3] are missing while version 25 is recorded (24 row(s) present).
                 The objects those migrations create were never applied, and re-applying a middle
                 migration on top of later ones can rewrite tables, so this boot refuses instead of
                 guessing. Restore the lost row(s) (or the database file) and start again.
EX-HOLE-RECOVER restored v3: rows=25 max=25   after restart: health=True
```

**逐版本表（删 v、重启、记录、补回；25 次）**：

```
v1..v24  health=False names-v        (每个都点名它自己的缺失版本)
v25      health=True
totals: started=1 refused=24
```

**如何同时读 S1 与 S2 两句话**（验收里"S1 删任一版本行仍能启动"与"S2 挖洞必须点名"在这里方向相反，必须写明我选了什么）：**脚本层** 25/25 可重放（§2.1）；**账本层** 中间缺行**拒启动并点名**（`:3` 那条），只有**账本仍连续**的情况（去掉最高行）才走"重放 + 启动"。理由是中间重放可能在更晚的迁移之上改表（0024），而"行丢了/对象被删了"在账本里无法区分。**这是一个需要 captain 确认的设计取舍**，不是偷偷选一个：如果要求中间缺行也能自动补跑，正确做法是加一条"先探测该迁移对象是否缺失、缺则补跑"的**显式修复入口**（见 §5 未覆盖）。

### 2.4 从零建库 + 基于旧库升级（因为改了 `.sql`，两条都要给）

```
EX-OLD  synthesized: ruagent.db at version 18 (18 ledger rows, 76 objects)
        inserted 1 legacy task (selected_by=agent:judge) + 1 legacy run
        before: tasks=1 runs=1 distill_log=0 chunks=0
        upgrade: health=True   ledger rows=25 min=1 max=25
        after : tasks=1 runs=1 distill_log=0 chunks=0 recall_log=0
        new objects from 0019-0025: ux_query_eval_gold_set_query=True wiki_pages=True chunks_fts_cjk=True
                                    distill_log_attempts=False   <-- 正确：0024 把它 RENAME 成 distill_log
        legacy provenance: ('old-t1','legacy task','done','run-old','agent:judge')   <-- 完好
```

⇒ 旧库（v18，真实历史形状 + 遗留数据）升级到 25：**行数不变、出身不变、新对象齐全**；从零建库路径见 `EX-ZERO`。**今天的库不会因此坏掉**：脚本只在"对象不存在"时才动手，且 `IF NOT EXISTS`/`IF EXISTS` 在全新库上与原来逐字等价。

### 2.5 负控（这一单要求的两条 + 我加的两条）

1. **故意挖洞 ⇒ 必须红**：`EX-HOLE` = `health=False` + 逐字点名；恢复 ⇒ `EX-HOLE-RECOVER` = `health=True`。
2. **定时变异（恒真断言那条）**：把 `let before = distill_rows;` 变异成 `+ 1`，
   ```
   WINDOW START 2026-09-29T09:13:33      (第一次尝试，见 §6 事故 1：被编译锁拖长，未取到读数)
   MUTANT APPLIED 2026-09-29T09:25:12.946
   test migrations::tests::a_vacuum_input_satisfies_every_pre_t33_predicate ... FAILED
   panicked at crates\store\src\migrations.rs:1583:
     the historical half of the pre-t33 predicate set does NOT hold: lost 1 row(s): before=1 after=0
   test result: FAILED. 39 passed; 1 failed; 3 ignored     (其余 39 个照旧绿 ⇒ 红是这条断言引起的)
   RESTORED 2026-09-29T09:25:18.060  sha256=9C858522…  identical-to-pre=True
   ```
   **窗口 2 = 5.1 秒**；文件恢复后 sha256 与变异前**逐字节一致**（`9C85852254A9A5C44A423C24AE940727B665687E6C6DC044519A268E67BC8A58`）。
   **窗口已关闭：`crates/store/src/migrations.rs` 现在是可推状态**（见 §6 事故 1 的完整时间线与恢复证明）。
3. **守卫不许吞错**：`a_non_reappliable_statement_is_still_an_error` —— 重复 `CREATE TABLE` 被跳过（无错），`INSERT` 撞主键**必须报 `DbError::Sqlite`**。写这个测试时它第一次是**红的**，因为它暴露了我自己的一个误解：重复 `CREATE` 是"被守卫认识的形状"，根本不该报错；于是负控换成了不认识的 `INSERT`（这条过程记在 §6）。
4. **谓词可伪**：`the_upgrade_row_invariant_can_fail`（永久，见 §1.4）。

### 2.6 门禁（第 22 条形态：行数 + 退出码；第 16 条：`-CleanFirst` 两件证据）

| 命令 | 退出码 | 读数 |
| --- | --- | --- |
| `scripts/cargo-team.ps1 test -p ruagent-store` | **0** | 日志 **69 行**；3 个 target：lib **40 passed / 0 failed / 3 ignored**、`tests/ledger_reopen.rs` **3 passed / 0 failed**、doc-tests 0；日志里出现 `Compiling ruagent-store`（证明测的是当次字节） |
| `scripts/cargo-team.ps1 clippy -p ruagent-store --all-targets -DenyWarnings -CleanFirst ruagent-store` | **0** | 日志 **7 行**；`clean -p ruagent-store (forced re-check)` → `Removed 614 files, 98.6MiB` → `Checking ruagent-store` → `Finished … in 1.18s`；**`warning:` 行 0**（⇒ 新集成测试也在 all-targets 里被 lint 到） |

门禁时的 in-scope 文件 sha256（前 16 位）：`migrations.rs 9C85852254A9A5C4` · `sqlite.rs 39EB75C4BF42ED8B` · `Cargo.toml F11BAC97783F3CAF` · `tests/ledger_reopen.rs 371EAB75C7C2295D`。
（`Cargo.toml` 我顺手把工作副本的行尾统一成 LF：HEAD 该文件本来就是 LF，我的编辑把它变成了 CRLF，git 于是有 `CRLF will be replaced by LF` 警告。统一后 `git diff` 仍是**一行**：只有 description。）

### 2.7 改动清单（`git diff --stat`）

```
 crates/store/Cargo.toml                            |   2 +-
 crates/store/src/migrations.rs                     | 584 ++++++++++++++++++++-   (实现 + 10 个新测试)
 crates/store/src/sqlite.rs                         |   8 +                 (DbError::Ledger)
 crates/store/src/migrations/0001..0025.sql         | 16 个文件，+72/-72 行（IF [NOT] EXISTS / IF EXISTS）
 crates/store/tests/ledger_reopen.rs                | 新文件（3 个集成测试）
 18 files changed, 662 insertions(+), 82 deletions(-)
```

`.sql` 普查（改后）：`unguarded CREATE/DROP statements left: 0`；**没有任何对象名被两个不同迁移创建**（79 个对象 / 79 个槽位）⇒「对象已存在就跳过 CREATE」在全新库上不可能保留旧形状（这是这个守卫最大的风险点，专门量了）。

## 3 机制上的两条判断（供评审挑战）

1. **为什么用探针而不是"吞 already exists 错误"**：吞错会把"迁移写错了"也吃掉（例如 `CREATE TABLE taks` 拼错时对象不存在、语句照跑、建出垃圾表；而某个语句因别的原因失败却恰好报 "already exists" 就会被静默）。探针问的是**对象图的事实**，未认识形状一律照跑照抛，并由 `a_non_reappliable_statement_is_still_an_error` 钉住。
2. **为什么 0024 允许"整脚本跳过"**：它的重放语义是破坏性的（DROP 真表 + 从暂存表重拷），逐语句守卫必然要么留着 DROP（丢数据）要么跳过 CREATE（形状不对）；探针把"重建已完成"变成可判定的事实，并且这个事实是 0024 自己建的索引，不存在"猜"。

## 4 查过、**不是**缺陷

1. `0022` 的两条回填 `UPDATE`（`status='planned' WHERE dry_run=1 AND status='planned_only'`；`finished_at=COALESCE(finished_at,started_at) WHERE … IS NULL`）重放**幂等**（只碰遗留状态、COALESCE 保住已有值）。
2. `0025` 的去重 `DELETE … WHERE id NOT IN (SELECT MIN(id) …)` + `CREATE UNIQUE INDEX`：重放时唯一索引已存在 ⇒ DELETE 无重复可删 ⇒ 幂等。
3. 触发器体内的 `INSERT INTO …_fts …` 只在 trigger 定义里出现，不被 runner 当语句跑（切分器把 trigger 体整块交给 `execute_batch`）。
4. 每迁移一个事务的原子性未动（`apply_one`），且**账本行与 DDL 同事务** ⇒ "应用了但没记账"仍不可能。
5. `DbError` 加变体不破坏任何调用点（无穷举 match，7 处构造/局部匹配）。
6. `0004/0005/0019` 的 `CREATE VIRTUAL TABLE … USING fts5` 加 `IF NOT EXISTS` 后，重放不产生影子表残留（`every_migration_can_be_reapplied…` 的 `sqlite_master` 快照逐字节相同即此证据）。
7. `#[ignore]` 三项（`lib.rs:1290`、`migrations.rs:775`、`migrations.rs:904`）都带理由与 env 名；裸形式全仓 0。
8. 全仓同绑定自比较 0 处活断言（2 处命中都在注释里）。

## 5 不覆盖什么（第 19 条）

1. **LanceDB/文件层的迁移**：`crates/store` 里没有 LanceDB（S7 已更正声称面），知识库/向量层的 schema 演进、向量表重建、维度变更**不在本单**（应随 `crates/knowledge` 立单）。
2. **并发/多进程迁移**：本设计是"单连接 + 单写线程"，两个进程同时开同一个 root 的行为、`busy_timeout` 与 WAL 下的迁移竞争**没有测**（第二个进程会撞上同一张 `schema_migrations` 表的写锁）。
3. **已发布版本的回滚**：我测的是"库较新/较旧 + 本二进制"，**没有测**"新二进制写过之后回退到旧二进制"（旧二进制没有连续性校验，会照旧行为工作；这点只能是推理，不是读数）。
4. **中间缺行的自动修复**：现在**拒绝并点名**，没有"探测对象缺失 → 补跑该迁移"的修复入口（`ruagent db-doctor` 之类）。这是 §2.3 里那条设计取舍的后续。
5. **0024 的数据拷贝路径**：合成旧库的 `distill_log` 是 **0 行**，所以 0024 的 `INSERT … SELECT` 只被**结构性地**走过（对象齐全、`distill_log_attempts` 正确消失），**没有用"有 distill 行"的旧库**测它的行保真（t25 的 `#[ignore]` 仪器正是为这个场景准备的，需要一份真库副本）。
6. **`0012` 出身改写**在守护进程级未复现（只在单元测试 `reapplying_0012_does_not_clobber_a_judges_provenance` 里造了 judge 行 + 重放 0012）；DB 级场景需要一个"v11 库 + `agent:judge` 行"的合成库。
7. **迁移脚本的语义正确性**（列类型/索引是否够用）不在本单：本单只保证"可重放 + 账本可验证 + 不悄悄改数据"。
8. **`crates/store` 之外的迁移面**（daemon 的 `guard`、knowledge 的 lance 目录初始化）未审未改。

## 6 过程记录（两个事故 / 三条教训）+ 收尾

### 事故 0：连续性校验的第一版把"未来版本"当成洞
`a_future_version_is_not_a_hole` 第一次就红：`current=999` ⇒ `1..=999` 全被当成缺行 ⇒ 降级路径被拒。修法是把上界收成 `min(current, SCHEMA_VERSION)`，未来版本改为 WARN。**测试先红后绿**是它有效的证据。

### 事故 1：变异窗口被编译锁拖长 + 恢复后 cargo 复用变异二进制
- **窗口 1**：`WINDOW START 2026-09-29T09:13:33`，脚本在 `cargo-team.ps1` 的锁上等了十多分钟（日志里连续 `another team build is running … waiting 10s`），我的工具调用在 600 s 上限被掐掉，**恢复语句没跑到** ⇒ 文件在窗口里带着变异体约 10 分钟（我下一次调用立刻用字节备份恢复，sha256 与变异前一致）。**这不满足"1–2 分钟"的承诺**，如实记录：这 10 分钟里任何跑 `-p ruagent-store` 的人会看到 `a_vacuum_input_satisfies_every_pre_t33_predicate` 红，消息是 `lost 1 row(s): before=1 after=0`。
- **窗口 2**（批准并记录过的那次）：先在无 `cargo/rustc` 时再动手，并挂了一个**看门狗进程**（8 分钟后无条件回写备份）⇒ `MUTANT APPLIED 09:25:12.946 → RESTORED 09:25:18.060`（**5.1 s**），sha256 一致。
- **教训（第二个坑更值钱）**：`Copy-Item` 恢复会**保留备份的旧 mtime**，而 cargo 的指纹是 mtime 的 ⇒ 恢复后第一次 `test -p ruagent-store` **复用了变异体编译出来的测试二进制**（读数：`lost 1 row(s): before=1 after=0`，而源文件早已恢复、sha256 正确）。修法：`(Get-Item <file>).LastWriteTime = Get-Date` 强制重建，之后才是绿（日志里能看到 `Compiling ruagent-store`）。**只交"恢复后 sha256 一致"是不够的**，还得有一条"重建后绿"的读数 —— 这条教训适用于所有变异负控。
- 影响面：只污染**共享 target 里的 store 测试二进制**（约 5 分钟内任何 `-p ruagent-store` 会看到那条红）；源文件从未在窗口外带变异体，事后 `Compiling ruagent-store` + 绿已证明。

### 教训（普查类）
未锚定的 `#\[ignore` 会把**文档散文**数成属性（`migrations.rs:771`/`:901` 的 `` `#[ignore]`d ``）；精确形式是 `^\s*#\[ignore\]\s*$`。我 t94 的普查用了未锚定形式，capitan 的"2 处裸"很可能是这么来的 —— 已在本报告更正为 **0 处**。

### 收尾与残留读数

- **我起的进程**：临时 daemon 共 30 个（端口 8819），**全部记录、全部已停**（脚本逐个打印 `alive=False`）；真守护进程 **pid 79984 全程未触碰**（每步后 `alive StartTime=2026/9/27 5:35:37`）。另见过程中短暂出现的 pid 61276（09:26:36 起、非我记录，复查时已退出，未动它）。
- **我的临时目录**：`%TEMP%\ruagent-t98-daemon`、`%TEMP%\ruagent-t98-old`、`%TEMP%\ra-t36-empty`（store 测试自己每次重建的真空输入）、私有构建树 `%TEMP%\ruagent-t98-target`（**15.56 GB**，用完即删，见下方"清理后"读数）、`%TEMP%\ra-t36-empty`、`%TEMP%\ruagent-t98-*` 脚本与日志。
- **不写活库**：所有 SQL 手术都在我自己的 root / 合成库 / 副本上；`~/.ruagent` 未被写过。

（清理后读数见任务 output；本节保留清理前的对象集以便对照。）

## 7 评审裁决与后续登记（captain，t98 判通过）

**§2.3 的取舍：维持当前实现**——脚本层 25/25 可重放；**账本层中间缺行拒启动并点名**；只有账本仍连续（丢最高行）时才重放并启动。裁决理由（captain 采纳本报告两条并补一条）：

1. 中间重放是**在更晚的迁移之上动手**（`0024` 的 DROP+RENAME 是真表）⇒「重放中间那一步」不是恢复，而是**用旧步骤改新形状**；
2. 账本里「**行丢了**」与「**对象被删了**」**不可区分** ⇒ 点名拒绝是唯一诚实的行为；
3. 最高行缺失（账本仍连续）时重放并启动是安全的（本报告 §2.2 的读数）。

**若将来做显式修复入口（`ruagent db-doctor` 之类），captain 给的判据**（登记在此，供下一单直接引用）：

- 必须**点名它重放了什么**（版本号 + 迁移干了什么）；
- 必须**证明对象被恢复**（不只是写了一行账本）——即一条"对象存在/列存在"的读数，而不是"账本行存在"；
- **在无法证明时拒绝修复**（不许静默修）。

**变异窗口事故已登记为纪律实例**（与 t93/t81 两起并列，C22 同族）：窗口 1 因「被团队编译锁拖住 + 工具调用 600 s 上限被掐」让文件带变异体约 10 分钟。**可用形态**（窗口 2，判据级）：**只在没有 `cargo/rustc` 运行时动手** + **挂一个独立看门狗进程无条件回写**（不依赖同一个工具调用的返回）⇒ 5.1 秒完成。配套两条教训：① 恢复后必须有一条**重建后绿**的读数（`Copy-Item` 保留旧 mtime ⇒ cargo 复用变异二进制）；② 跨成员可见的红必须**先公告**并报窗口起止。

**本报告更正了 t94 的一条 finding**：「2 处裸 `#[ignore]`」实为 `migrations.rs:771/901` 的**文档散文** `` `#[ignore]`d ``，未锚定模式把散文当属性；锚定后 `crates/store` 与全仓裸形式均为 **0**。与 C24（`Select-String 'FAILED'` 大小写不敏感造成假红）同族：**未锚定的匹配会造出不存在的 finding**。
