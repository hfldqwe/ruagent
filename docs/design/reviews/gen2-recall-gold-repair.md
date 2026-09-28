# V-A F-1 闭合：`query_eval_gold` 的 seed 幂等性（t29）

> 状态：**已修复并通过验证**。产物：`crates/store/src/migrations/0025_query_eval_gold_unique.sql`（新）+ `crates/store/src/migrations.rs`（注册）+ `crates/knowledge/tests/common/mod.rs`（seed 文档/守卫/漂移报告）+ `crates/knowledge/tests/gold-seed-idempotence.rs`（新，可重取的前后对照仪器）+ 本文件。
> 作者：recall。时间窗：**2026-09-28T01:2x → 02:0x+08:00**。
> inScope 遵守：只改 `crates/store/`、`crates/knowledge/` 与本报告；**未写活库**（§5 有只读证据）。

**一句话**：`seed` 用 `INSERT OR IGNORE`，而 `INSERT OR IGNORE` 只能忽略**某个 UNIQUE 约束定义出来的**冲突 —— 0019 没给 `(set_id, query)` 任何唯一性，于是每跑一次仪器就往表里**再追加整份 22 条**：6 次之后 `COUNT(*)=132`、`SUM(answerable)=90`（V-A 的 F-1，本单用**真 seed 代码**独立复现）。集合没变大，**表**变大了。今天的 KPI 不受影响（判据读的是冻结常量 `GOLD`），但**表的大小是下一代 C3「≥60 条查询且 ≥20 条无答案」的判据前置** —— 按被灌水的行数读，就是给自己发一个从没达到的目标。0025 先按 `(set_id, query)` 去重、再建唯一索引；seed 加了一条**拒绝活库**的守卫；并留下一个**永续的前后对照仪器**（同一次运行里 PRE 侧显示缺陷、POST 侧显示修复）。

---

## 1 逐条验收：改前 → 改后

| # | 验收项 | 改前 | 改后 | 判 |
| --- | --- | --- | --- | --- |
| 1 | **改前读数独立复现**：无 `UNIQUE(set_id, query)`、`INSERT OR IGNORE` ⇒ 6 次后 `(132, 90)` | `COUNT(*)` = 22, 44, 66, 88, 110, **132**；`SUM(answerable)` = 15, 30, 45, 60, 75, **90**；`distinct(set_id,query)` 恒为 22；`duplicate_groups` 0→22 | 同一仪器 POST 侧：**22 × 6 次**，`SUM(answerable)` **15 × 6**，`duplicate_groups` 恒 0 | **达标** |
| 2 | **修法**：① 追加迁移加唯一性 + seed 真幂等；② seed 只写副本 | **两条都做了**：0025 去重 + `CREATE UNIQUE INDEX ux_query_eval_gold_set_query`；seed 加了「拒绝活库根」守卫（`pragma_database_list` 读连接真实打开的文件，不是读一个可以忘记传的参数）；6 次逐次序列 PRE `[22,44,66,88,110,132]` / POST `[22,22,22,22,22,22]` | **达标** |
| 3 | **活库影响（只读）** | 见 §5：`~/.ruagent` 在 v18，**`query_eval_gold` 整个表还不存在** ⇒ 0 行、0 重复组；清理语句写进 §5 但**未执行** | **达标** |
| 4 | **防假达标的下游口径** | 见 §7：给出「按表行数 vs 去重后」对照 —— 真副本上 **308 vs 22**（answerable 210 vs 15）；scratch 库上「会被读成 132 条查询、90 条可答」vs 22 | **达标** |
| 5 | `test -p ruagent-store`、`test -p ruagent-knowledge`、`clippy --all-targets -DenyWarnings -CleanFirst` | 见 §6：**34 passed** / **66 passed** / clippy `Checking ruagent-knowledge` 零诊断（`-CleanFirst` 生效） | **达标** |
| 6 | 只改 inScope、不写活库、不启停 79984、HF 用临时副本 | `git status` 见 §8；活库只读证据见 §5；HF_HOME 指向 `%TEMP%\ia-hf\hub` 副本 | **达标** |

**确切命令**
```powershell
$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-store
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-knowledge
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 clippy -p ruagent-knowledge --all-targets -DenyWarnings -CleanFirst ruagent-knowledge
# 前后对照仪器（scratch 根在 %TEMP%，自己擦自己）
$env:RUST_TEST_NOCAPTURE="1"
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-knowledge --test gold-seed-idempotence -Nocapture
# 真副本联调（会迁移并写副本）
$env:HF_HOME="$env:TEMP\ia-hf\hub"; $env:RUAGENT_IA_LIVE_COPY="$env:TEMP\ia-live"
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-knowledge --test retrieval-gold-copy -Nocapture
```

---

## 2 缺陷：现场读数（真 seed 代码 + 真 schema，6 次逐次）

**仪器**：`crates/knowledge/tests/gold-seed-idempotence.rs`。两侧只差**一个对象** —— 0025 加的那个唯一索引，PRE 侧按名字 `DROP INDEX`（而不是重写一遍 0019 的 `CREATE TABLE`：重写是**历史的重构**，会与历史漂移），POST 侧保留。同一段 seed 代码、同一份数据、同样 6 次。

```
[t29] frozen set: 22 queries, 15 answerable, 7 unanswerable

PRE  side: dropped 1 unique index(es) -- 0025's only object, so this side reproduces the pre-0025 schema
[t29] PRE  run 1: COUNT(*)=22  SUM(answerable)=15  distinct(set_id,query)=22  duplicate_groups=0
[t29] PRE  run 2: COUNT(*)=44  SUM(answerable)=30  distinct(set_id,query)=22  duplicate_groups=22
[t29] PRE  run 3: COUNT(*)=66  SUM(answerable)=45  distinct(set_id,query)=22  duplicate_groups=22
[t29] PRE  run 4: COUNT(*)=88  SUM(answerable)=60  distinct(set_id,query)=22  duplicate_groups=22
[t29] PRE  run 5: COUNT(*)=110 SUM(answerable)=75  distinct(set_id,query)=22  duplicate_groups=22
[t29] PRE  run 6: COUNT(*)=132 SUM(answerable)=90  distinct(set_id,query)=22  duplicate_groups=22

POST side: dropped 0 ... (keeps 0025's index)
[t29] POST run 1..6: COUNT(*)=22  SUM(answerable)=15  distinct(set_id,query)=22  duplicate_groups=0
[t29] PRE  sequence COUNT(*): [22, 44, 66, 88, 110, 132]
[t29] POST sequence COUNT(*): [22, 22, 22, 22, 22, 22]
[t29] PRE side BY TABLE ROWS: COUNT(*)=132 SUM(answerable)=90 -> would read as a 132-query set with 90 answerable
[t29] PRE side AFTER DEDUPLICATION: 22 queries in 22 duplicate groups -> the set that was actually frozen
test result: ok. 2 passed; 0 failed
```

- `(132, 90)` **逐位等于 V-A 的独立读数** ⇒ 两个独立复现指向同一件事，不是我的夹具碰巧。
- 同一份输出的最后两行就是**假达标的入口**：同一张表，按行数读是「132 条查询 / 90 条可答」，按去重集合读是「22 条 / 15 条可答」。
- 该仪器**可重取**：两个 scratch 根（`%TEMP%\ra-t29-pre` / `-post`）在开头被自己擦掉并打印（`wiped the previous scratch root ...`），所以第二次、第 N 次跑读数相同。

---

## 3 修法一：0025（去重 + 唯一索引），以及为什么**不用 upsert**

```sql
DELETE FROM query_eval_gold
WHERE id NOT IN (SELECT MIN(id) FROM query_eval_gold GROUP BY set_id, query);

CREATE UNIQUE INDEX ux_query_eval_gold_set_query ON query_eval_gold(set_id, query);
```

- **先删后建**：真库里这个表由 0019 在同一次迁移里创建（空表），所以第 1 步删 0 行；但**开发副本**上已经有 308 行 = 22 × 14（实测），第 1 步把它收敛回真正冻结的 22 行。索引若先建会直接失败。
- **保留 `id INTEGER PRIMARY KEY`**：以后给某一行换标签（例如人工复核后 `judged_by` → `'human'`）应当是 **UPDATE 那一行**，不是插第二行。一条被判定的查询就是一行，来源放在列里（0019 自己就是这么注释 `judged_by` 的），所以 `(set_id, query)` 就是自然键。
- **我特意没有把 seed 改成 upsert（这是有意的取舍，不是漏做）**：`OR IGNORE` **从不碰已存在的行**。而 `judged_by` / `note` 正是**人工判定**会落的地方（0019 的注释；R-A C3 等的就是 `judged_by='human'` 行，今天 0 行）。upsert 会把人类标签覆盖掉 —— 那会把语料里**唯一的人类判定**销毁。所以：已存在行若与冻结常量不一致，走**打印**（`[gold] drift: N of 22 ...`，本次两侧都是 `0 of 22`），**不覆盖**；`common/mod.rs` 的文档注释也改成了说明「幂等性来自 0025 的约束，不来自 `OR IGNORE` 自己」，并把 `(132, 90)` 这件事写进去。
- 边界：**没有任何东西指向 `query_eval_gold`** —— 在迁移到 25 的副本上查 `sqlite_master`，提到它的对象只有它自己和它的两个索引（`idx_query_eval_gold_set` + 0025 的 `ux_query_eval_gold_set_query`），没有视图、没有触发器、没有子表；它的两条外键都是**向外**的（→ `documents.gold_document_id`、→ `query_eval_sets.set_id`）。所以 `DELETE` 既不会被外键拦住，也不会留下孤儿引用。

## 4 修法二：seed 拒绝活库（守卫，不是注释）

```rust
fn refuse_live_root(conn: &rusqlite::Connection) {
    // 读连接**实际打开**的那个文件，而不是读一个调用方可能忘记传的参数
    let file: String = conn.query_row(
        "SELECT file FROM pragma_database_list WHERE name = 'main'", [], |r| r.get(0))...;
    ... if is_inside(&canonical(target), &canonical(home/.ruagent)) { panic!("refusing to seed ...") }
}
```
- 判断是**纯函数** `is_inside(target, root)`（组件级，所以 `.ruagent-other` 不会被误判成 `.ruagent` 之内），单测 4 个用例：活库本体 ✓ 拒、活库下任意文件 ✓ 拒、**兄弟目录** `.ruagent-other` ✗ 放行、`%TEMP%` 副本 ✗ 放行。
- panic 是有意的：这段代码跑在仪器里，它要防的事故（评测仪器去迁移并写用户的活库）必须以最响的方式停下。
- **未测（如实登记）**：守卫的 panic 分支**没有**对着真正的活库根执行过（那正是它要防的动作），只单测了它的判决函数与「活库之外一律放行」这一侧。这是唯一一处有意不实测的分支。

---

## 5 真副本联调 + 活库只读评估

### 5.1 真副本（`%TEMP%\ia-live`，t7 遗留、带着 14 次灌水）
```
BEFORE: version 23  COUNT(*)=308  SUM(answerable)=210  DISTINCT(set_id,query)=22  dup groups=22
（跑 retrieval-gold-copy：应用 0024+0025，然后重新 seed）
[gold] drift: 0 of 22 frozen queries disagree with the table
[t7-gold] gold set id=1 newly written=0 embedder=fastembed:multilingual-e5-small (dim 384)
[t7-gold] n=15 misses=0 recall@1=0.7333 recall@5=0.9333 recall@20=1.0000 MRR=0.8185 nDCG@10=0.8621
test result: ok. 1 passed; 0 failed
AFTER:  version 25  COUNT(*)=22   SUM(answerable)=15   DISTINCT(set_id,query)=22  dup groups=0
```
- 迁移把灌水的 308 行收敛成 22 行，`newly written=0`（**seed 现在真的没东西可写**）。
- **KPI 未被本单推动**：`recall@1 0.7333 / MRR 0.8185 / nDCG@10 0.8621` 与 t7 的读数**逐位相同** —— 修的是判据前置，不是指标（任务书的要求就是这条）。

### 5.2 活库（只读，`mode=ro`；2026-09-28T02:00:38+08:00）
```
size: 14946304  mtime: 2026-09-27T21:15:21.078444+08:00     ← 与 t7 引用点相同，未被写
schema_migrations max: 18
  table query_eval_gold: ABSENT (created by migration 0019, not yet applied)
  table query_eval_sets: ABSENT (created by migration 0019, not yet applied)
recall_log rows: 651                                          ← 与 2026-09-27 现场读数相同（写一次就会涨）
distill_log rows: 33
recorded daemon pid: 79984 | pidfile mtime: 2026-09-27T05:35:38.846250+08:00   ← 未启停
```
**结论**：活库里 `query_eval_gold` **还不存在**（v18），所以 `COUNT(*) = 0`、`COUNT(DISTINCT query) = 0`、**重复组 0** —— 活库**没有任何重复需要清理**。集成重启时会先由 0019 建表（空），再由 0025 去重（删 0 行）并建索引，因此**这次修复对活库的既有数据零改动**。

**清理语句（写给"手上有一张被灌水表"的读者，本单未执行、也不对活库执行）**：
```sql
-- 只读证据下的清理；0025 内部执行的正是这条（先删后建索引）
DELETE FROM query_eval_gold
WHERE id NOT IN (SELECT MIN(id) FROM query_eval_gold GROUP BY set_id, query);
-- 清理后自检：下面两个数必须相等，否则集合仍被灌水
SELECT COUNT(*) AS table_rows,
       (SELECT COUNT(*) FROM (SELECT DISTINCT set_id, query FROM query_eval_gold)) AS set_rows
FROM query_eval_gold;
```

---

## 6 验证读数（三条契约命令）

| 命令 | 读数 |
| --- | --- |
| `test -p ruagent-store` | `test result: ok. **34 passed**; 0 failed`（exit 0） |
| `test -p ruagent-knowledge` | `35 + 2 + 6 + 9 + 1 + 1 + 9 + 3 = **66 passed**; 0 failed`（exit 0）；两个活库仪器打印 **NOT MEASURED**（未设 env 时不许静默跳过），新仪器打印 `POST sequence COUNT(*): [22,22,22,22,22,22]` |
| `clippy -p ruagent-knowledge --all-targets -DenyWarnings -CleanFirst ruagent-knowledge` | `Checking ruagent-knowledge v0.1.0` + `Finished`，**零诊断**（exit 0） |

**「覆盖哪些 target + 本次确实重查了哪个包」两件证据**
1. `--all-targets` 对 `ruagent-knowledge` 覆盖的 target = `lib` + 7 个集成测试二进制：`retrieval-quality` · `retrieval-legs` · `retrieval-cjk` · `retrieval-gold-copy` · `retrieval-gold-live` · `residual-scan` · `gold-seed-idempotence`（与 §6 第二行 8 个 `test result` 逐一对上）。
2. `-CleanFirst ruagent-knowledge` 的输出里**有 `Checking ruagent-knowledge v0.1.0`** —— 该包这次**确实被重查**，不是命中上一轮的缓存指纹（这正是 graph 提醒的「假绿」家族之一，见 t25 报告 §9.2）。

---

## 7 下游口径（防假达标，写给下一代 C3 与 t19）

**规则**：C3 的前置「≥60 条查询且 ≥20 条无答案」**必须按去重后的集合读**，不能按表行数读。

```sql
-- 正确：集合规模 = 去重后的查询数
SELECT COUNT(*) AS n_queries,
       SUM(answerable) AS n_answerable,
       SUM(1 - answerable) AS n_unanswerable
FROM (SELECT DISTINCT set_id, query, answerable FROM query_eval_gold);
-- 错误（今天在带重复的表上会虚高；0025 之后新库不再有重复，但**读数口径不能靠"不会再发生"**）
SELECT COUNT(*), SUM(answerable) FROM query_eval_gold;
```

**两个口径的实测对照（同一张表、同一时刻）**
| 表 | 按表行数 | 去重后 |
| --- | --- | --- |
| `%TEMP%\ia-live`（t7 遗留） | **308** 行 / 210 可答 | **22** 条 / 15 可答 |
| scratch PRE 侧（6 次 seed） | **132** 行 / 90 可答 →「132 条查询的集合」 | **22** 条 / 15 可答 |
| `~/.ruagent`（v18，表未建） | 0 | 0 |

**为什么这条比缓存假绿更值得写进纪律**：它不是「读数采不到」，而是**读数的对象被自己的重复运行养大了**。即使 0025 之后新库不再产生重复，判据也**不许**建立在「这个 bug 不会再发生」之上 —— 口径必须自己去重。

---

## 8 边界、复现与未测

- **只追加**：`0001–0024` 一字未改；`MIGRATIONS` 尾部追加一项 = 版本 25（`SCHEMA_VERSION` = `MIGRATIONS.len()`）。
- **inScope**：本单新增 `crates/store/src/migrations/0025_query_eval_gold_unique.sql`、`crates/knowledge/tests/gold-seed-idempotence.rs`、本报告；修改 `crates/store/src/migrations.rs`（注册一行）、`crates/knowledge/tests/common/mod.rs`（seed 文档 + 守卫 + 漂移报告）。`git status` 里 `crates/store`/`crates/knowledge` 的其余条目属我 t6/t7 的在途工作；`crates/memory`、`crates/graph`、`crates/daemon`、`panel` 的改动**没有一行是我的**。
- **活库纪律**：全程 `mode=ro` 打开 + `VACUUM INTO` 副本（`%TEMP%\ra-t29-pre` / `-post` 是 scratch、`%TEMP%\ia-live` 是 t7 的副本）；未启停 pid 79984。**未使用 `GET /api/v1/recall`**。
- **HF 缓存**：真副本联调用 `HF_HOME=%TEMP%\ia-hf\hub`（我自己的临时副本），未指向 `~/.ruagent/models`。
- **未测（不静默跳过）**：
  1. 守卫的 panic 分支未对真活库根执行（见 §4，有意的）。
  2. 「活库升级后」的读数未取：活库仍 v18，0025 要等集成重启随 0019–0025 一起应用（那时去重删 0 行）。
  3. `judged_by='human'` 行仍 **0** 行 —— C3 的数据前置依旧缺失（t7 已按 deferred 报 captain），本单**不改变**这一点，只是让它的判据不再能被灌水。
  4. `query_eval_runs` 侧不受本单影响（未取新读数）。
