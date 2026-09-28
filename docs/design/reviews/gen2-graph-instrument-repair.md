# 图侧活库仪器收口（t42）：会写其输入的仪器必须独占其输入

> 本单是**仪器形状修复**，不是读数修复：读数必须逐位不变，只有「能不能一次跑完」这件事变了。
> inScope：`crates/graph/tests/**` + 本文件。未动 `crates/graph/src/**`（产品代码）、未写活库、未启停 pid 79984。
> 全部命令走 `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 <args>`（共享 target、一次一个编译、只用 0–11 核）。

## 0 一句话

`crates/graph/tests/live-after.rs` 的三条 `#[ignore]` 活库仪器原先**共用一份副本**，而 `Db::open` 会跑迁移（写）⇒ `--include-ignored` 一次跑全部时**两条死在 `Db::open`**（改前 `1 passed / 2 failed`）。现在每条仪器**自己 `VACUUM INTO` 一份私有副本**（先删后建）、**只读打开源**、并**断言自己那份副本的对象集与所有权**⇒ 改后 **`3 passed / 0 failed`**，且三条**单独跑**与**一起跑**得到**逐位相同**的读数。

**并且**：本单在同一个 crate 里找到了同一条规则的第三个实例——**测试夹具本身也在写自己的输入**（`fixture.rs` 把快照插进一个 `{pid}-{seq}` 临时目录，却从不清理旧目录）⇒ 我的一次门禁运行出现 `3 passed; 2 failed`（单跑 12 次不复现）。修法相同（**创建前先删自己的目录**），并补了一条**负控**through 的回归测试（§11）。规则因此在本 crate 里有**三处**落地：三条活库仪器、夹具目录、以及「源只读」。

## 1 规则

> ### 会写其输入的仪器必须独占其输入。
> 更完整地说：**只要仪器会打开其输入并写它**（本 crate 的实例是 `Db::open` 的迁移 + `merge_entities` 的合并 + `build_communities` 的写入），它就必须自己复制一份、写自己的那份，并且**断言那份是自己的**。否则「一起跑」与「单独跑」不是同一个实验，读数不可复现。

## 2 两个先例（同一族的守卫，t36）

| 先例 | 位置 | 形状 |
| --- | --- | --- |
| store 侧 t6/t25 | `crates/store/src/migrations.rs:799-815` | 注释原文：「**T6 and T25 must point at SEPARATE copies.** The t25 instrument writes two ANNOTATED rows into `distill_log` for its probe session, so running the two with ONE copy leaves this test reading another test's writes -- and the history assertion further down ("every historical row must be NULL") then fails for a reason that has nothing to do with history.」守卫是一个断言：`probe_rows == 0`，失败时直接说明「point `RUAGENT_T6_LIVE_COPY` and `RUAGENT_T25_LIVE_COPY` at SEPARATE copies」 |
| knowledge 侧活库族 | `crates/knowledge/tests/retrieval-gold-copy.rs:41`、`retrieval-gold-live.rs:33` | `RUAGENT_IA_LIVE_COPY` 指向**一份被复制的 root**；其中一条会 `record a query_eval_runs row`（写）⇒ 同一族 |
| **本单（图侧）** | `crates/graph/tests/live-after.rs` | 见 §3：比先例更强一步 —— 不再依赖「人给两份副本」，而是**仪器自己派生**私有副本，所以「共享」在结构上不可能 |

**先例教训的通用形式**：**t36 的失败不是断言错了，是输入错了**，而错误输入的表现是**另一条测试的断言以不相干的理由变红**（history-NULL）。本单的改前表现同族：错的是「共用一份副本」，症状是**两条仪器在 `Db::open` 里崩**。

## 3 改前 → 改后

### 3.1 一次跑全部（`--include-ignored`）

| | 读数 | 证据 |
| --- | --- | --- |
| **改前**（t38 现场测到，2026-09-28，同一份活库副本） | **`1 passed; 2 failed`**，两条失败在 `crates/graph/tests/live-after.rs:29` 的 `Db::open(&path).unwrap()`（三条仪器共用一份副本，`Db::open` 的迁移与一条仪器的写入在并发下互相踩） | t38 报告 `gen2-graph-impl.md` §14.4 第 1 条（同一份副本、`--include-ignored`） |
| **改后**（本单，2026-09-28） | **`test result: ok. 3 passed; 0 failed`**（`--test live-after -- --include-ignored`） | 本节 §5 的原始输出 |

改前那个数字**无法在今天的树上重放**，因为改后的形状让「共享一份副本」不可能发生 —— 这不是丢失证据，而是本单的目的；改前的原文与命令保留在 §3.1 引用处。

### 3.2 三条单独跑仍各自通过（断言没有为「一起跑」变绿而削弱）

```
single: frame        -> test result: ok. 1 passed; 0 failed
single: redundancy   -> test result: ok. 1 passed; 0 failed
single: communities  -> test result: ok. 1 passed; 0 failed
```

三条的**断言一条没删、一条没放宽**：`communities` 仍断言 `covered/non_isolated >= 0.90`、`paths == 12`、`facts <= 24`、`hops <= 2`；`redundancy` 仍断言 `redundant == 0` 且 `edges == 67`；`frame` 仍断言 `new_nonempty >= 13` 且 `new_nonempty > old_nonempty`。**新增**的断言只有「对象集相等」「所有权标记专属」「副本仍是原始的」三类（§4）。

### 3.3 读数逐位不变（这是形状修复，不是读数修复）

| 仪器 | t9/t27/t38 已登记的读数 | t42 一起跑 | t42 单独跑 |
| --- | --- | --- | --- |
| frame | `REAL FRAME n=23 \| strict non-empty 1 -> new 13 \| queries with >1 seed: 7`；`seeds by leg (all queries): {"exact_name": 1, "name_token": 8, "summary_fts": 46}`（t38 §13.1 更正后的可引用值） | **逐字相同** | **逐字相同** |
| redundancy | `the judge finds 7 redundant pairs` → `merged 5 pairs, redundant pairs = 0, edges still 67, aliases 5`（t27 §1 RVC-6 / G6） | **逐字相同**（另含 `entities 58`） | **逐字相同** |
| communities | `9 communities, covered 43/43 non-isolated, split_by_modularity=1`（t9 §1 G8）；`retrieve(ruagent): paths=12`（t9 G5） | **逐字相同**（另含 `seeds=12 facts=11 truncated_by=Some(MaxPaths) frontiers=[8, 7]`） | **逐字相同** |

## 4 每条仪器断言自己那份副本（不只靠路径不同）

三条防线都落在 `own_copy(tag)` 与各仪器开头：

1. **路径独占**：目标是 `%TEMP%\ruagent-t42-live\live-<tag>-<pid>.db` —— `tag` 区分仪器、`pid` 区分并发运行 ⇒ 三条仪器**不可能**指向同一个文件。
2. **内容相等（迁移之前）**：复制完成后、`Db::open` 之前，用 rusqlite 直接读副本与源，断言**对象集指纹相等**：
   ```
   Fingerprint { entities: Some(63), edges: Some(67), aliases: None, recall_rows: Some(651), recall_queries: Some(23) }
   ```
   指纹的每个字段都是 `Option`：**源是被迁移落后的活库**（v18 没有 `entity_aliases` 表）⇒ 「表不存在」与「表存在但为空」是**两个不同的事实**，不能都写成 0。比较发生在迁移之前，所以「我这份就是源的内容」是**测出来的**而不是假设的。
3. **所有权标记**：`Db::open` 之后往自己副本写一行 `t42_owner(tag)`，再断言 ① 副本里读到的是**自己的 tag**，② **源里连 `t42_owner` 这张表都不存在**（`sqlite_master` 计数 = 0）。这是「我能写我的输入、而别人看不见」的直接证据，也是先例那条守卫的等价物。
   同一族的「陷阱播报」：`redundancy` 在写之前断言 `entity_aliases == 0`、`communities` 断言 `communities == 0`（失败信息直接说「你读的是别的仪器已经写过的副本」）—— 因为这两条仪器**写的就是这两处**。
4. **源只读**：复制用的连接是 `SQLITE_OPEN_READ_ONLY`（实测可用：`VACUUM INTO` 只读源、只写目标）⇒ 「源不会被改」是**连接属性**，不是注释里的承诺。这也顺手修掉了改前的一个副作用：旧形状里第一条跑起来的仪器会**把迁移写进共享的源**。

原始输出（一起跑，摘录）：
```
[t42] frame: own copy "...\ruagent-t42-live\live-frame-92824.db" | marker exclusive | fingerprint Fingerprint { entities: Some(63), edges: Some(67), aliases: None, recall_rows: Some(651), recall_queries: Some(23) }
[t42] frame instrument object set: recall_log rows=651 distinct_queries=23 entities=63
[t42] redundancy: own copy "...\live-redundancy-92824.db" | marker exclusive | fingerprint Fingerprint { entities: Some(63), edges: Some(67), aliases: None, recall_rows: Some(651), recall_queries: Some(23) }
[t42] redundancy instrument object set (before its own writes): entities=63 edges=67 aliases=0
[t42] communities: own copy "...\live-communities-92824.db" | marker exclusive | fingerprint Fingerprint { entities: Some(63), edges: Some(67), aliases: None, recall_rows: Some(651), recall_queries: Some(23) }
[t42] communities instrument object set (before its own writes): entities=63 communities=0
```
⇒ 三条仪器同一个 pid、三个不同的目标文件、三份指纹相同、三个标记互斥。

## 5 原始读数（可复现）

```powershell
# ① 一份源副本（VACUUM INTO：Copy-Item 会丢 WAL，不能用来复制活库）
python -c "import sqlite3,os;d=os.path.join(os.environ['TEMP'],'ruagent-t42-source.db');os.path.exists(d) and os.remove(d);c=sqlite3.connect(os.path.join(os.path.expanduser('~'),'.ruagent','data','ruagent.db'));c.execute(\"VACUUM INTO '%s'\"%d.replace('\\\\','/'));c.close()"
$env:RUAGENT_GRAPH_LIVE_COPY="$env:TEMP\ruagent-t42-source.db"

# ② 一次跑全部（三条一起）：改前 1 passed / 2 failed → 改后 3 passed / 0 failed
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-graph -- --include-ignored --nocapture

# ③ 三条单独跑（各自仍通过）
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-graph --test live-after the_real_query_frame -- --include-ignored --nocapture
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-graph --test live-after redundant_pairs -- --include-ignored --nocapture
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-graph --test live-after community_coverage -- --include-ignored --nocapture

# ④ 默认（不带 env）：三条 ignored，其余全绿
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-graph
```

## 6 `VACUUM INTO` 目标已存在（缺陷 ②）

现场实测（read-only 连接）：

| 情形 | 结果 |
| --- | --- |
| 源以 `mode=ro` 打开，目标不存在 | **成功**（14.3 MB）⇒ 源只读是可行的，不必为了复制而给源写权限 |
| 目标已存在 | `OperationalError: output file already exists`（**报错，不是覆盖**） |

修法：`own_copy` 在 `VACUUM INTO` 之前**先删目标**（`std::fs::remove_file`），并把这一步的失败信息写清（`removing the stale copy target {dst:?}`）；运行配方（§5 ①、`live-after.rs` 头注释）也统一成「先删后建」。

## 7 没有输入时的形状：响亮地红，不静默跳过

不带 `RUAGENT_GRAPH_LIVE_COPY` 跑 `--include-ignored`（诚实登记，不是失败）：

```
thread '...' panicked at crates\graph\tests\live-after.rs:39:57:
set RUAGENT_GRAPH_LIVE_COPY to a COPY of ~/.ruagent/data/ruagent.db (never the live file): NotPresent
test result: FAILED. 0 passed; 3 failed; 0 ignored
```

**这是有意的**：与 t36 的 `RUAGENT_T6_LIVE_COPY` / knowledge 的 `RUAGENT_IA_LIVE_COPY` 同形 —— 缺输入是「你让我量，但我量不了」，不是「没有数据 ⇒ 通过」。**改成「悄悄 return ⇒ 绿」会让 `--include-ignored` 在没读任何活库的情况下变绿（空转通过），那正是本代反复抓的假通过。** 所以本单**不**把它改成 skip；验收命令 ② 需要带那个 env 前缀（与契约里 `$env:PATH=…` 前缀同类），两种形状的读数都列在 §3.1/§7。

## 8 门禁与两件证据

| 命令 | 结果 | ①覆盖哪些 target | ②本次确实重查了哪个包 |
| --- | --- | --- | --- |
| `test -p ruagent-graph` | **exit 0**，**35 passed / 0 failed / 3 ignored**（不带 env 时三条活库仪器不跑） | **12 个 target**（逐 unit `--crate-name`：`ruagent_graph` lib + lib test、`entity_query_shapes`、`multihop_gold`、`fixture_ownership`、`resolution`、`seed_resolution`、`temporal`、`fixture`、`extraction_gold`、`live_after`、`empty_recall_pattern`） | 本轮改的就是 `live-after.rs` + `fixture.rs` + 新 target `fixture-ownership.rs`；`-CleanFirst ruagent-graph` 卸 **2384 files / 321.4MiB** 后 12 个 unit 逐个重编重跑 |
| `test -p ruagent-graph -- --include-ignored` | **exit 0**，`live-after` 目标 **3 passed / 0 failed**（其余 target 全绿，共 36 passed） | 同上 12 个；三条 `#[ignore]` 仪器本轮**真的跑了**（三条各自打印 `[t42] own copy …` 指纹行） | 同上；三条仪器各自的私有副本指纹 `entities 63 / edges 67 / recall 651 / distinct 23` 逐条列出 |
| `clippy -p ruagent-graph --all-targets -DenyWarnings -CleanFirst ruagent-graph` | **exit 0**，`-CleanFirst` 卸 1466 files / 259.0MiB，`Finished in 1.62s`，**零诊断** | 同批 **12 个** unit 逐个出现 `Running clippy-driver --crate-name …` | `-CleanFirst` 的 Removed 行 + 同一次运行内的 12 条 unit 行 |

**稳定性读数**（连续 5 次整套 `test -p ruagent-graph`）：**5/5 exit 0**，每次 12 条 `test result: ok.`。这一条是针对 §11 那个瞬时红的直接证据：夹具不再继承任何旧目录后，同一套命令连跑不再出现 `3 passed; 2 failed`。

## 9 改动清单

* `crates/graph/tests/live-after.rs`：新增 `source_path()` / `open_source_readonly()` / `Fingerprint` / `table_rows()` / `fingerprint_of()` / `own_copy(tag)` / `assert_pristine()`；三条仪器从 `open_copy()`（直接 `Db::open` 源）改成 `own_copy("<tag>")`；三条各自新增对象集/原始性断言；头注释重写（规则、改前症状、运行配方）。
* `crates/graph/tests/fixture.rs`：新增 `next_root()` 与 `from_snapshot_at(root, snapshot)`（把「拥有目录」这条规则放进可测的函数）；**创建前先删同名旧目录**；`Drop` 的注释写明它**只能尽力**（Drop 跑时连接还开着，Windows 会拒删）。
* `crates/graph/tests/fixture-ownership.rs`（新 target）：回归测试 `a_stale_fixture_root_is_emptied_not_inherited`（§11）。
* `docs/design/reviews/gen2-graph-instrument-repair.md`（本文件）。
* **未动**：`crates/graph/src/**`（产品代码一行未改）、`crates/graph/tests/` 里其他测试的断言、任何其他 crate、活库（只读打开源、只往自己副本写）、pid 79984。

## 10 仍然存在的、与本单无关的弱点（不静默）

* 三条仪器仍需要人先准备一份**源副本**（`RUAGENT_GRAPH_LIVE_COPY`）：本单让仪器独占**它的输入**，但没有让仪器**自己去找活库** —— 那需要读写 `~/.ruagent`，与本单的「不写活库」边界冲突，故意不做。
* 副本文件留在 `%TEMP%\ruagent-t42-live\`（每次运行按 pid 命名，便于事后核对；**先删后建**只针对同名的那一个）。若需要自动清理，应另立一条（并把「保留失败现场」的取舍写清楚）。

## 11 第三个实例：测试夹具也在写自己的输入（本单附带发现，**不在验收清单里**）

### 11.1 现象：一次门禁运行瞬时红

`test -p ruagent-graph --verbose -CleanFirst ruagent-graph`（同一天、同一次验证）出现：

```
test result: FAILED. 3 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.14s
```

失败的 target 是 `seed-resolution`（5 条里 2 条红）。随后：**单独跑 6 次、再 6 次（两个 5 测试 target 共 12 次）全部绿**；同一套命令再整跑 5 次也全绿（§8）。一个「偶发红」就是一条要解释的读数，不是噪声。

### 11.2 根因（测出来的）

1. `crates/graph/tests/fixture.rs` 的夹具把整份快照插进 `%TEMP%\ruagent-graph-fixture-{pid}-{seq}`，而 `{pid}` 会被操作系统**回收**；`from_snapshot` 只做 `create_dir_all`，**从不清理同名旧目录**。
2. `Drop` 里的 `remove_dir_all` **只能尽力**：它跑在字段析构之前 ⇒ `self.db` 还开着 ⇒ Windows 拒删（本单实测同一形状：`os error 32`「另一个程序正在使用此文件」）。证据：本单开工时 `%TEMP%` 下有 **725 个** `ruagent-graph-fixture-*` 目录。
3. 于是一个被回收的 pid + 相同 seq 会**继承上一轮的快照行**（实体 id 1..63 已在），rebuild 再插入同样 id ⇒ `UNIQUE(entities.id)` ⇒ 该 target 里**恰好若干条**测试红（与「2 of 5」相符：只有拿到那几个 seq 的测试受影响）。

### 11.3 修法（同一条规则）

`from_snapshot_at(root, snapshot)`：**创建前先删同名旧目录**（失败就响亮地 panic，绝不继承）。`Drop` 保持尽力并注明它为什么不可靠；`next_root()` 与 `from_snapshot_at()` 公开出来，使这条规则**可被测试**，而不是只写在注释里。

### 11.4 回归测试 + 负控（只有坏侧红、好侧绿才算两侧都测到）

`crates/graph/tests/fixture-ownership.rs::a_stale_fixture_root_is_emptied_not_inherited`：在一个 `{pid}` 目录里留下一个**没人持有**的陈旧 `graph.db`（28 字节的非数据库字节串 + 一个陈旧 `-wal`），然后要求「在同一个 root 上重建」得到**恰好 63 个实体、0 条继承行、数据库大小 > 1000 字节」：

```
t42 fixture ownership: stale root rebuilt -> entities 63 (expected 63), inherited rows 0 (expected 0), db size 4096 bytes (the stale 28-byte file is gone)
```

**负控**：把「先删」那一段临时注释掉再跑同一条测试 ⇒ **`FAILED`**，错误是 `Sqlite(SqliteFailure(NotADatabase)) "file is not a database"`（`Db::open` 打到陈旧字节上）⇒ 这条测试确实**能因为缺少修法而变红**，不是装饰。随后恢复源码并重跑 ⇒ 绿；源码完整性另行确认（`removing a stale fixture root` 仍在，无负控残留）。

**顺带一条仪器教训（写下来，因为它会骗人）**：恢复源码时我用了保留 mtime 的写法（`shutil.copyfile` + `move`），于是源码的 mtime **比上一次编译还旧** ⇒ cargo 认为无需重编，**用负控那个坏二进制跑出了第二次「红」**，看起来像「修法无效」。真相是缓存命中（我们这代一直被提醒「绿可能是缓存命中」，这次是**红也可能是旧二进制**）。处置：`touch` 源文件强制重编后复跑 ⇒ 绿。凡「恢复文件后再跑」的场合，都要显式 touch 或 `-CleanFirst`。
