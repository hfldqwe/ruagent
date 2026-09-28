# I-SCHEMA-2 `distill_log` 按次计数（t25 / R-C DEP-3）

> 状态：**已实现并通过验证**。产物：`crates/store/src/migrations/0024_distill_attempts.sql`（新）+ `crates/store/src/migrations.rs`（注册迁移、修一处过时文档、加 2 个测试）+ 本文件。
> 作者：recall（store 是 schema 的唯一真相源）。时间窗：**2026-09-28T00:58:47 → 01:1x+08:00**。
> inScope 遵守：本单只改 `crates/store/**` 与本报告；**写入侧 `crates/daemon/src/distill.rs` 一行未动**（归 graph 的 I-C 修复单）。

**一句话**：`distill_log` 的主键本来是 `session_key`，于是**每次蒸馏尝试都命中同一个键、`INSERT OR REPLACE` 把上一次的行删掉** —— 现场窗口里 22 次尝试 21 次失败，库里只留下 1 行，失败率只活在日志里。0024 把这张表**重建**成一行一次尝试（`id INTEGER PRIMARY KEY`，`session_key` 降为普通列 + 索引），历史 33 行**逐行保留且 `status` 仍为 NULL**（=「尝试未知」，不许猜），并加一条**视图**把「比率的分子/分母是什么」写进 schema：`distill_recorded_outcomes`。

---

## 1 逐条验收：改前 → 改后

| # | 验收项 | 改前读数 | 改后读数 | 判 |
| --- | --- | --- | --- | --- |
| 1 | 迁移**只追加**；`distill_log` 一行一次尝试；失败不得覆盖成功，反之亦然 | 同一 `session_key` 两次尝试**只留 1 行**（`INSERT OR REPLACE`） | 同一 `session_key` 两次尝试**2 行**；成功后再失败 → 2 行；失败后再成功 → 3 行且失败行仍在（双向都测） | **达标** |
| 2 | 旧行 NULL 语义保住；「会话级、尝试未知」与「本次失败」可分开；不许按内容猜 status | 33 行（无 `status` 列，活库在 v18） | 33 行**逐行保留**，`status IS NULL` = **33/33**，`id` 唯一；失败行 `status='failed'` 与 NULL 行**在同一个查询里可分辨**；视图把 NULL 行排除在分母之外 | **达标** |
| 3 | 独立副本 + 直接 SQL：一次成功 + 一次失败 → 读 **2 行**（改前 1 行）；失败行带 reason（可用时带指纹） | `ra-t25-before.db`（v18 副本）**rows = 1, memories_written = 0**（成功的行被失败覆盖销毁） | `ra-t25-after.db`（v18→v24 副本）**rows = 2**（`ok=1, failed=1`），`failure_reason = "prompt longer than the context window"`，`prompt_hash = "ph-bad"` | **达标** |
| 4 | 空库证据（`PRAGMA table_info` / 计数）；老库升级不丢行（n → ≥ n） | 空库/老库在 0008 的形状：6 列，PK = `session_key` | 空库：**10 列**、PK = **`id`**（逐列逐序断言）；老库副本：**33 → 33**（`after == before`，重建是逐行拷贝，差值即拷贝不完整）、`id` 全唯一 | **达标** |
| 5 | `test -p ruagent-store` / `clippy --all-targets -D warnings` | — | **34 passed / 0 failed**（t6 时 32 → +2 本单测试）；clippy **exit=0 零诊断**。两条读数都是**强制重编后重取**的（输出里可见 `Compiling`/`Checking ruagent-store`，见 §9.2），且 live-copy 测试已修成**可重跑**（§9.1） | **达标** |
| 6 | 只改 `crates/store/**` 与本报告；写入侧不归本单，schema 证据用直接 SQL | — | `git status --porcelain -- crates/store` 列出的 9 项里，**本单新增的只有 `0024_distill_attempts.sql`**；`migrations.rs` 的 `M` 是 t6 与 t25 共用的同一个在途文件（本单在其中只加注册行、测试与一处过时文档修正）；其余 6 项（`fts.rs`/`lib.rs`/0019–0023）全属 t6。`crates/daemon/src/distill.rs` **未被本单触碰**（只读了 296–365 行）；§3 的两条读数都是**直接 SQL**（python + store 自己的连接） | **达标** |

**确切命令**（全队包装脚本，不自设 `CARGO_TARGET_DIR`；`-DryRun` 先核形状）：
```powershell
$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-store
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 clippy -p ruagent-store --all-targets -DenyWarnings
$env:RUAGENT_T25_LIVE_COPY="$env:TEMP\ra-t25-after.db"       # v18 副本；测试会迁移并写入
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-store -Nocapture
git status --porcelain -- crates/store
```

---

## 2 缺陷的现场证据（不是推测）

- 任务书给的数字我复核了：`distill_log` **33 行**（2026-09-13..09-26），**三者全 0 的占 18/33**；同窗口 daemon.log **22 次尝试 21 次失败（95.5%）**。
- 2026-09-28T00:58:47+08:00 只读现场复核（`mode=ro`，未碰活库）：`count=33`、`session_key IS NULL` = **0**、`distinct session_key` = **33**、**PK = `session_key`**（`sqlite_autoindex_distill_log_1`）、`max(version)=18`（活库还没跑我的 0019–0023）。
- 机制：`session_key TEXT PRIMARY KEY` + `INSERT OR REPLACE` ⇒ **一次尝试只能留一行**。这不是「日志写得少」，是**键设计让历史不可共存**。

---

## 3 迁移设计：为什么这次可以重建表，以及重建会撞坏谁

**为什么必须重建**：SQLite 不能删主键，而主键就是缺陷本身。「这个键不再唯一」没有加法形式。t6 的 `apply` 已经把每条迁移放进**一个事务**（0024 因此是原子的），但事务解决的是「半应用」，不是「能不能重建」。

**三条前置条件，迁移文件写出之前先在活库上量过**（都在 §2 那条只读读数里）：
1. **没有任何东西引用它**：`foreign_keys` 0 条、视图 0 个、触发器 0 个提到 `distill_log` ⇒ `DROP TABLE` 不会因为依赖对象中止（0022 的 wiki 重建做不到正是因为 41 行子表）；
2. **数据可无损拷贝**：33 行、`session_key` NULL **0** 行、33 个互异键 ⇒ 新形状下不会有行冲突，而且新加的 `NOT NULL` 不会拒掉任何历史行（旧表 `TEXT PRIMARY KEY` 在 SQLite 里**允许 NULL** —— 这是本次顺手收紧的一处）；
3. **迁移是事务的**（t6）⇒ 不会留下半张表。

**0024 的形状**：
```sql
CREATE TABLE distill_log_attempts (
    id INTEGER PRIMARY KEY,           -- 每次尝试的身份（= rowid）
    session_key TEXT NOT NULL,        -- 不再是键；NOT NULL（每次尝试都有会话）
    distilled_at TEXT NOT NULL,
    memories_written INTEGER NOT NULL DEFAULT 0,   -- 原样保留
    entities_written INTEGER NOT NULL DEFAULT 0,
    relations_written INTEGER NOT NULL DEFAULT 0,
    agent TEXT,
    status TEXT, failure_reason TEXT, prompt_hash TEXT   -- 0020 加的，NULL 语义原样
);
INSERT INTO distill_log_attempts (...) SELECT ... FROM distill_log ORDER BY distilled_at, session_key;  -- id 可复现
DROP TABLE distill_log;
ALTER TABLE distill_log_attempts RENAME TO distill_log;
CREATE INDEX idx_distill_log_session ON distill_log(session_key, id);
CREATE INDEX idx_distill_log_status  ON distill_log(status) WHERE status IS NOT NULL;  -- 比率用
CREATE VIEW   distill_recorded_outcomes AS SELECT * FROM distill_log WHERE status IS NOT NULL;
```

**⚠ 这会撞坏谁（登记，不藏）**：`session_key` 不再唯一之后，带 `ON CONFLICT(session_key)` 的语句在 **PREPARE 阶段**就报
`ON CONFLICT clause does not match any PRIMARY KEY or UNIQUE constraint`。
全仓**只有一处**这样的语句：`crates/daemon/src/distill.rs:332`（`log_outcome`，graph 的 I-C 修复单）。我把**那条语句原文**（含 `WHERE excluded.status <> 'failed' OR ...` 谓词）抽出来在迁移后的副本上跑了一遍：

```
STALE WRITER FAILS AT PREPARE: ON CONFLICT clause does not match any PRIMARY KEY or UNIQUE constraint
REPLACEMENT (plain INSERT) rows for that session: 1
rate query: [('failed', 1), ('ok', 2)]
```

即：**换成普通 INSERT 就是按次计数**。我选择让它**大声失败**而不是继续悄悄塌陷成一行 —— 一个还在把六次尝试压成一行的旧写入者，正是本单要终结的东西。**这一条必须在同一个集成窗口里和 graph 的改动一起落地**（已发消息给 graph，见 §7）。

---

## 4 读数（对象集 / 采样面 / 判据）

### 4.1 空库（`fresh()`，合成）
```
columns(distill_log) = [id, session_key, distilled_at, memories_written, entities_written,
                        relations_written, agent, status, failure_reason, prompt_hash]   （逐列逐序断言）
PRAGMA table_info(distill_log) 的 pk 列 = ["id"]
sqlite_master 里有 distill_recorded_outcomes（视图存在且可查）
```
合成行为读数（`distill_log_keeps_one_row_per_attempt_and_legacy_rows_stay_unknown`）：
- 同一会话 成功→失败 = **2 行**；再追一次成功 = **3 行**，且 `status='failed'` 仍 **1 行**（失败没被后来的成功抹掉）；
- **同一秒内**两次尝试共存 = 5 行（旧键做不到：时间不参与键，除 `id` 外不需要任何东西唯一）；
- 失败行带 `failure_reason` 与 `prompt_hash`（能答「是哪个 prompt 失败」）；
- `id` 为 1..5（插入序，可复现）；
- 再插一行**不写 status** 的历史形状行：`distill_log` = 6 行，`status IS NULL` = 1，**`distill_recorded_outcomes` = 5 行**；按 status 分组 = `[("empty",2),("failed",1),("ok",2)]` —— **数据库现在能回答过去只有日志能回答的问题**；
- `session_key` 为 NULL 的插入被拒（NOT NULL 收紧有效）；
- **旧写入者的语句 prepare 必须失败**（断言错误信息含 `ON CONFLICT`）—— 这是把 §3 的隐患钉成判据，免得它被静默复活。

### 4.2 真实库副本的升级（`ra-t25-after.db` = 活库 v18 的 `VACUUM INTO` 副本；**第一轮**用 01:02:16+08:00 那份，**修订后**重做了一份全新的）

```
RUN 1（全新 v18 副本）
[t25] live copy BEFORE: rows=33 already-0024=false columns=["session_key","distilled_at",
      "memories_written","entities_written","relations_written","agent"] outcome-unknown=33/33
[t25] live copy AFTER migration: rows=33 outcome-unknown(NULL)=33 ids unique=true (1 = yes)
[t25] live copy AFTER two SQL attempts on one session:
      rows=2 ok=1 failed=1 failure_reason=Some("prompt longer than the context window") prompt_hash=Some("ph-bad")
[t25] live copy: recorded-outcome view = 2 of 35 rows (the rate's denominator)
test result: ok. 34 passed; 0 failed

RUN 2（**同一个副本**，它已经被 RUN 1 迁移过 —— 验证者会做的第二次跑）
[t25] live copy BEFORE: rows=35 already-0024=true columns=[10 列] outcome-unknown=33/35
[t25] live copy AFTER migration: rows=35 outcome-unknown(NULL)=33 ids unique=true (1 = yes)
[t25] live copy: cleared 2 scratch row(s) from a prior run
[t25] live copy AFTER two SQL attempts on one session:
      rows=2 ok=1 failed=1 failure_reason=Some("prompt longer than the context window") prompt_hash=Some("ph-bad")
[t25] live copy: recorded-outcome view = 2 of 37 rows (the rate's denominator)
test result: ok. 34 passed; 0 failed
```
- **升级不丢行**：33 → 33（`after == before` 断言：重建是逐行拷贝，不等就意味着拷贝不完整），并且**逐行**按 `(session_key, distilled_at)` 比对（`lost = 0`）—— 只比总数漏得掉「换了一行」；
- **历史行的 NULL 一个都没被动**：`outcome-unknown` 迁移前后相等（RUN 1：33/33 → 33；RUN 2：33/35 → 33）。两轮的分母不同是因为 RUN 1 自己写下的那 2 行**有标注**，这正是两轮读数的差别所在，不是矛盾；
- **两条尝试 = 2 行**，失败行带 reason 与指纹，两轮一致；
- **分母视图**：RUN 1 = 2 of 35、RUN 2 = 2 of 37（37 = 35 + 本次 2；上一轮的 2 行草稿由测试自己清掉）⇒ 比率不会把 33 行历史当成成功；
- 起始状态**写进读数**（`already-0024=false/true`）：这是 §9.1 那个缺陷的修法，好让「这份副本此前有没有被迁移过」不再靠猜。

### 4.3 改前读数（同一件直接 SQL 仪器，另一个独立副本 `ra-t25-before.db`，v18 原始形状）
```
BEFORE (pre-0024 schema, two INSERT OR REPLACE on one session): rows = 1 memories_written = 0
BEFORE: the success was DESTROYED -- the failure's row replaced it
```
这就是**可证伪证据**：同一段 SQL、同一个会话、同一份数据，改前留 1 行且成功的 `memories_written=7` 变成 0；改后留 2 行。两条读数都在 `%TEMP%` 副本上，活库**全程只读**（`mode=ro` + `VACUUM INTO`）。

---

## 5 读的口径（写进 schema，不靠口口相传）

| 问 | 查哪里 | 为什么 |
| --- | --- | --- |
| 「这次尝试成功了吗」 | `distill_log.status` | `ok` / `empty` / `failed`；**NULL = 没人记录过结果**（0024 之前的 33 行，或一个不写 status 的旧写入者），**不是成功** |
| 「蒸馏失败率是多少」 | `SELECT status, COUNT(*) FROM distill_recorded_outcomes GROUP BY 1` | 分母 = **有记载的结果**。把 NULL 折进「ok」会替没人观测过的成功背书 —— 这正是 0020 拒绝 `NOT NULL DEFAULT 'ok'` 的同一个理由 |
| 「这个会话试过几次」 | `SELECT * FROM distill_log WHERE session_key=? ORDER BY id` | 这就是旧键做不到的读 |
| 「是哪次/哪个 prompt 失败的」 | `id` / `prompt_hash` / `failure_reason` | 指纹在 0020 就有，本单让它**按次**可写 |

---

## 6 未解决 / 未测（不静默跳过）

| # | 项 | 状态 | 谁解 |
| --- | --- | --- | --- |
| N-1 | 写入侧仍是旧语句（`ON CONFLICT(session_key)`） | **已由 graph 落地**（2026-09-28 01:1x+08:00 回执）：`crates/daemon/src/distill.rs` 的 `log_outcome` 改成普通 INSERT，他那侧读数 `attempts=4 rows=4 recorded_outcomes=4`、`rows - attempts = 0`、「后来的失败之后成功那行仍在 `("ok", None, 3)`」、按 status 分组 `[(empty,1),(failed,2),(ok,1)]`；`clippy -p ruagent-daemon --all-targets -DenyWarnings` 也回 exit 0 | graph（完成） |
| N-2 | `crates/mock-agent/tests/e2e_daemon.rs:2182` 断言某会话「distill_log 有行且 entities=0」 | **未跑（不在本单 inScope，也不在 verify 命令里）**；graph 复核了语义：多行时**无 ORDER BY 取最旧一行**，该会话只蒸馏一次时断言仍成立（今天不会红），但建议改成「至少一次尝试」或 `ORDER BY id DESC LIMIT 1` | mock-agent 侧（需 captain 派单） |
| N-3 | 活库（`~/.ruagent`）**仍是 v18**，0024 未应用 ⇒ **活库的失败率今天仍不可读** | **未测（正确：不许碰活库）** | 集成时由守护进程重启应用 0019–0024 |
| N-4 | 「失败率」的完整读数 | **未测**：库里要有**新写入者写的** status 行才谈得上比率；本单只证明**分母口径可查**（副本上 2 行） | 集成后可由 integ/面板取 |
| N-5 | 是否有别处（HTTP/面板）假设「每会话 ≤1 行」 | **已查：没有**。全仓 `distill_log` 的读者只有 `crates/daemon/src/distill.rs`（含它自己的 verify 助手，按 status 分组）与那一个 mock-agent 断言；`panel/` 与 `api.rs` 今天都不读这张表 | 无需改动；若下一代加面板，遵守 §5 的口径 |

---

## 7 跨单交接（已发消息，不靠别人猜）

**给 graph（I-C 写入侧）**：0024 之后 `distill_log` 的形状是 `id / session_key(非唯一, NOT NULL, 有索引) / distilled_at / memories_written / entities_written / relations_written / agent / status / failure_reason / prompt_hash`。要改的**只有一处**：`crates/daemon/src/distill.rs:327-343` 的 `INSERT ... ON CONFLICT(session_key) DO UPDATE ...` → **普通 INSERT**（失败与成功各自一行，不再需要那段 `WHERE excluded.status <> 'failed' OR ...` 保护）。`status` 词表 `ok|empty|failed` 不变，`prompt_hash` 每次尝试都写。报 surface = `crates/daemon/src/distill.rs`。

**给 integ/t19**：注入/遥测面若要看失败率，用 `distill_recorded_outcomes`（或等价的 `WHERE status IS NOT NULL`），**不要**用 `COUNT(*) FROM distill_log` 当分母。

---

## 8 边界与复现

- **只追加**：`0001–0023` 一字未改；`MIGRATIONS` 数组尾部追加一项 = 版本号 24（`SCHEMA_VERSION` 随 `MIGRATIONS.len()` 自动变 24）。
- **`crates/store` 之外零改动**：`git status --porcelain -- crates/daemon crates/knowledge crates/memory crates/graph panel` 里的每一项都属于同伴在途的单（`daemon/src/{api,distill,memembed,wiki}.rs` = wiki/integ/graph/mem-core；`knowledge/**`、`memory/**`、`graph/**` 各有其主）。本单对 `crates/daemon/src/distill.rs` 只**读**了 296–365 行。
- **活库纪律**：`~/.ruagent/data/ruagent.db` 全程 `mode=ro` 只读；两份副本由 `VACUUM INTO` 生成在 `%TEMP%`（`ra-t25-before.db` 用于改前读数，`ra-t25-after.db` 由本单测试迁移并写入）。真守护进程 pid 79984 未启停。
- **测试写权限**：`live_copy_*` 测试会**迁移并写入**目标库，因此只在 `RUAGENT_T25_LIVE_COPY` 被显式指向副本时才跑；不设时它**打印 `NOT MEASURED` 并通过**（与 t6 同一形状）—— 验证者请把「有没有打出 `[t25] live copy BEFORE`」当判据，而不是只看绿。
- **本单自己的 lint 债**：首轮 4 处 `await`/类型标注编译错 + 无 clippy 告警。

---

## 9 我在本单自己抓出的两个读数缺陷（都改了，不留在原地）

### 9.1 `live_copy_*` 测试**不可重跑**（不是缓存问题，是测试对起始状态的假设）
第一版测试假定「副本是 pre-0024，所以每一行都是 outcome-unknown」，于是断言 `NULL 行数 == 总行数`、并且把草稿行直接 INSERT。**第一次跑在全新副本上通过，第二次跑同一个副本就失败** —— graph 提醒我「绿可能是缓存命中」之后，我 touch 了自己的文件强制重编再跑，撞出来的正是这个：

```
[t25] live copy BEFORE: rows=36 columns=["id","session_key",...]      ← 副本早已被迁移过
assertion `left == right` failed: every pre-existing row must still say 'outcome not recorded'
test result: FAILED. 33 passed; 1 failed
```
诊断：不是缓存、不是迁移，是**我的测试把「起始状态」当成了常量**。一次不能被重取的读数不算读数，而验证者重跑是常规动作。修法三条：
1. 起始状态**先量后断**：没有 `status` 列 = 全部未知；有 = 按 `status IS NULL` 计数；并把 `already-0024=true/false` 打进读数；
2. 「不丢行」改成**逐行**比对 `(session_key, distilled_at)`，不只比总数；
3. 草稿会话只清**它自己**那一个 `session_key`（`DELETE ... WHERE session_key='t25:schema-side-probe'`），从不触碰别人的行 ⇒ 读数在两轮里完全相同。**修完的实测是 §4.2 的 RUN 1 / RUN 2 两轮都 34 passed。**

### 9.2 「绿」可能是缓存命中 —— 本单的 gate 读数已**强制重编**重取
graph 同轮报了这个同族陷阱（他第一次跑 daemon 的 `-D warnings` 是 0.77s / 0.9s 的 exit 0）。本单的最终读数不是靠「上一次的编译产物」：
```
   Compiling ruagent-store v0.1.0 (C:\Users\19410\Documents\ai\ruagent\crates\store)   ← test
    Checking ruagent-store v0.1.0 (…)                                                  ← clippy
   test result: ok. 34 passed; 0 failed
   [cargo-team] exit=0 elapsed=2.7s / clippy exit=0
```
两条 gate 都是在 `LastWriteTime = now` 强制失效之后跑的，`Compiling`/`Checking ruagent-store` 两行就是「这次真的编了」的判据。另一条同族事实（本单也踩过）：**cargo 在第一个失败的 target 就停住** ⇒ `--all-targets` 的 "all" 不是保证；所以本单的 live-copy 读数用 `--test`/`--lib` 之外还单独 `-Nocapture` 跑过一遍，确认 `[t25]` 那几行真的出现在输出里。
