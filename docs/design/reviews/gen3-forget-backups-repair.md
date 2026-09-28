# 遗忘的残余面补上「备份类」：`ResidualSurface::BackupFile` + C5 判据修订 + 只读检测读数（t75 / A-1）（mem-core）

- 任务：t75（kind=repair round 1）；attempt `a8674d4c-4f77-4f76-8c21-439ca4adfead`
- inScope：`crates/memory/src/lifecycle.rs` · `docs/design/reviews/gen2-memory-spec.md` · 本报告；**未改** `crates/daemon/src/wiki.rs`、`api.rs`、`mcp`、`store`
- **本单没有做任何破坏性动作**：没有删除/移动/加密任何用户数据、没有写活库、没有启停 pid 79984。检测全部是 `read_dir` / `metadata` / `read` / **`immutable=1` 只读打开**。
- 上游：t71 的 A-1（verify2 报「本轮最重要的一条」）+ captain 的规格裁决（备份属于遗忘的结论面）

## 0 一句话

`forget` 的结论面原先只覆盖**活库及其派生物**，于是「活库清干净」会被读成「可证明」——而**被清前的字节以明文躺在磁盘上**。本单把这一面加进枚举（`BackupFile`），让报告**检测并报告**它，并把该面写进 C5 判据（**只检测、只报告**；处置另立单）。

## 1 面清单：枚举 14 个面 × 本机读数

`ResidualSurface` 改前 13 个变体、报告实际发出 12 个面（local 4 + external 6）；改后 **14 个变体**、报告发出 **12 个面**（local 5 + external 7）。逐面读数：

| # | 面 | 谁回答 | 本机（2026-09-29 只读） | 报告里的形态 |
| --- | --- | --- | --- | --- |
| 1 | `Memories` | 本 crate | 活库 163 行；带 `[distilled]` 标记 **0** 行；tombstone **0** | `Readout`（本地查询） |
| 2 | `MemoriesFts` | 本 crate | 触发器跟随 memories | `Readout` |
| 3 | `Derived` | 本 crate | — | `Readout` |
| 4 | `Episodes` | 本 crate | — | `NotAvailable`（无 hash→transcript 列，R-B U-1） |
| 5 | **`BackupFile`（新）** | 本 crate（文件）+ daemon/wiki（请求） | **2 个候选**：`data/backups/memories-distilled-prefix-20260926T210910Z.txt` **35,843 B / 312 行**（本 crate 的转储）· `data/ruagent.db.before-t229-cleanup-20260926-150802` **1,359,872 B**（整库副本，**可只读打开**） | `Readout`（有 root）/ `NotAvailable`（无 root）+ 一条 `ExternalResidual`（wiki 侧） |
| 6 | `KnowledgeFile` | I-A | 未查询（owner 承诺 t7 后） | `NotAvailable`（请求） |
| 7 | `KnowledgeDocument` | I-A | 同上 | `NotAvailable`（请求） |
| 8 | `KnowledgeChunk` | I-A | 同上 | `NotAvailable`（请求） |
| 9 | `KnowledgeChunkFts` | I-A | 同上 | **无任何报告提及（finding A-2）** |
| 10 | `KnowledgeVectors` | I-A | 同上 | **无任何报告提及（finding A-2）** |
| 11 | `WikiChunk` | I-D/wiki | 未查询 | `NotAvailable`（请求） |
| 12 | `WikiPageHash` | I-D/wiki | 哈希面，答不了 substring | `NotAvailable`（请求） |
| 13 | `WikiPlan` | I-D/wiki | 未查询 | `NotAvailable`（请求） |
| 14 | `WikiBuildPage` | I-D/wiki | 无正文 | **无任何报告提及（finding A-2）** |

> 「未查询 / 承诺中」是**报告自己**给出的状态（`NotAvailable` + 原因），不是本报告的口径猜测 —— 见 §2 的 `READING t75 external surfaces` 与既有 `source` 字段。
> 本单**只**实现 `BackupFile` 的 memory 侧读数；其余面照旧由 owner 回答，**不改**它们的形状。

## 2 代码读数（逐条，全部可在测试里复现）

### 2.1 A-1 的完整现场被复现成一个测试（`the_pre_cleanup_bytes_are_reported_as_a_residual`）

```
READING t75 strip outcome: PrefixMigrationOutcome { scanned: 1, stripped: 1, …, backup: "<temp>/data/backups/memories-distilled-prefix-20260928T204847Z.txt" }
READING t75 after cleanup: live memories surface=Readout(ResidualCount { hits: 0, truncated: false, total: Some(0) })
                           backup surface=Readout(ResidualCount { hits: 2, truncated: false, total: Some(2) })
READING t75 carriers = ["memories-distilled-prefix-20260928T204847Z.txt", "ruagent.db.before-t75.db"]
READING t75 inventory memories-distilled-prefix-20260928T204847Z.txt: 46 B, 1 record(s) with this hash
READING t75 inventory ruagent.db.before-t75.db: 438272 B, 1 record(s) with this hash
```

读法：**活库面上 0 命中（内容真的被清理了），而备份面上 2 个载体** —— 这正是 A-1 说的「活库干净、明文还在」。两个载体分别走两条**独立**的检测路径：转储按本 crate 的长度前缀格式解析、逐条用 `crate::write::content_hash` 重算哈希（**一个事实一个来源**：不另写一份哈希实现）；整库副本按 SQLite 只读查询同一条 SQL。

### 2.2 检测面在**真实 root** 上的读数（`RUAGENT_T75_REAL_ROOT` 点名时才读；不点名会**响亮打印 SKIPPED**，不静默跳过）

```
READING t75 real root "C:\Users\19410\.ruagent" backup surface = Readout(ResidualCount { hits: 0, truncated: false, total: Some(0) })
READING t75 real candidates = 2
READING t75 real inventory memories-distilled-prefix-20260926T210910Z.txt: 35843 B, checked, this hash is NOT in it
READING t75 real inventory ruagent.db.before-t229-cleanup-20260926-150802: 1359872 B, checked, this hash is NOT in it
```

⇒ 验收点名的两个文件**都被检测到**，且 35,843 B 与 1,359,872 B 与任务单给的读数一致（转储另有 PowerShell 独立复核：`bytes=35843 newlines=312`）。用故意不存在的哈希查，两者都回「checked, NOT in it」⇒ **它们确实是「检查过的面」，不是「因为文件存在就算命中」**。

### 2.3 负控（关键）：文件存在 ≠ 残余

```
READING t75 negative control: Readout(ResidualCount { hits: 0, truncated: false, total: Some(0) }) sample=[]
READING t75 negative inventory memories-distilled-prefix-20260928T204847Z.txt: 34 B, checked, this hash is NOT in it
READING t75 negative inventory ruagent.db.before-t75-neg.db: 438272 B, checked, this hash is NOT in it
```

`total: Some(0)` 是**测到的零**（不是 `None`、也不是「跳过」）⇒ 判据「文件在就报」被负控堵住。

### 2.4 三条状态（`None ≠ 0`，本代通则）

```
READING t75 unrooted backup surface: NotAvailable("no filesystem root was given: `data/backups/*` and
  `data/*.before-*.db` cannot be reached by SQL; call forget_report_at(db, Some(root), hash)") source=n/a (no root supplied)
```

⇒ 无 root 时面**仍然出现**并点名能回答它的调用（改前：这一面**根本不在枚举里**，连 `NotAvailable` 都不报 —— A-1 的原形状）。第三条状态（候选读不出来 ⇒ `total: None`）见 §3。

## 3 三条**实测**出来的设计事实（比实现本身值钱）

1. **副本可能读不出来，且不许读成「里面没有」**。最初版本用朴素的 `SQLITE_OPEN_READ_ONLY` 打开整库副本，测试里副本的结构还留在 `-wal`（只 copy 了主文件）⇒ 检测器**如实报** `could not be checked: not a memories database: no such table: memories` 并把该面 `total` 置 **`None`（未测）**。⇒ 这条路径**不会**把「读不到的副本」读成「干净」。
2. **文件种类必须按字节判定，不能按扩展名**。本机的整库副本名是 `ruagent.db.before-t229-cleanup-20260926-150802`，**不以 `.db` 结尾** —— 第一版按 `.db` 结尾筛候选，**漏掉了它**（读数：`real candidates = 1`）。改为嗅探 SQLite 头（`SQLite format 3\0`）与本 crate 转储头之后 ⇒ `real candidates = 2`，副本被判为可读。
3. **`immutable=1` 不是优化，是正确性**：朴素只读打开一个 WAL 模式的副本会在**它旁边的目录里生成** `-shm`/`-wal`（本机实测：32768 B 的 `-shm` + 0 B 的 `-wal`）。**一个「遗忘报告」不能写它正在报告的那个目录** ⇒ 改为 `immutable=1`（URI）打开、把 `-shm`/`-wal` 从候选里排除，并规定：**副本旁边若有非空 `-wal`，该副本判为「未测」**（`immutable` 读不到 `-wal`，按主文件读会给出可能偏低的零）。

## 4 C5 判据更新（`docs/design/reviews/gen2-memory-spec.md`）

- 在 C5 表末**追加一行**指针（**表内旧文字一字未改**）：`〔2026-09-29 修订（t75）：target 的 (c) 范围必须包含「被遗忘字节的明文副本」——ResidualSurface::BackupFile 已落地〕`。
- 文末**追加**一节 `## t75 遗忘残余面：备份类（C5 修订，2026-09-29）`，内含：① **旧文字逐字引用**（`metric` / `target` / `失败判据` 三行原文）；② 修订 7 条（范围、新增面与按字节判种类、三条判据形态、只读约束、本代不做处置、wiki 侧登记、三个未覆盖面登记）；③ 本机反例读数（163 行 / 0 标记 / 0 tombstone vs 35,843 B / 1,359,872 B）。
- **没有**把未覆盖写成 `not_measured`：本单未覆盖的三项在规格与 §5 里都写成**已知缺口 + owner**，可判为「未达成」。

## 5 交回 / 路由（本单不改）

| # | 位置 | 发现 | 建议 |
| --- | --- | --- | --- |
| **A-2** | `lifecycle.rs` 的面清单 | 枚举里 `KnowledgeChunkFts` / `KnowledgeVectors` / `WikiBuildPage` **三个面无任何报告提及**（与 A-1 同形的「不列＝不在」） | 要么由各自 owner 给出读数并入报告，要么在规格里写明**为什么**不列；本单只在测试里点名（`READING t75 unmentioned enum variant: …`），**未**擅自加进报告（无查询就会变编读数） |
| **A-3** | `crates/daemon/src/wiki.rs:2317-2323` | wiki 的写前副本 `wiki-backups/{slug}.md` 是**同一个面**（明文副本），但 owner 不同、不在本单 inScope | 报告已用 `ExternalResidual{surface: BackupFile, owner: "daemon/wiki", source: "crates/daemon/src/wiki.rs:2317-2323"}` **请求**它；建议 owner 窗口补一条读数（本 root 下该目录尚未创建，见「未覆盖」） |
| **A-4** | `crates/daemon/src/api.rs:3326-3340`（`GET /api/v1/forget-report`） | 端点调的是无 root 的 `forget_report` ⇒ 从 HTTP/MCP 看，`BackupFile` 永远是 `NotAvailable`（面出现了，但读不到本机真实的备份） | 端点侧改调 `forget_report_at(db, Some(root), hash)`（一个参数）；`api.rs` 现被 t96 占着，交回 captain 排单 |
| **A-5** | 备份**保留**策略（尚未存在） | §3 的第 3 条：副本带不带 `-wal` 决定它能否被判据覆盖 | 另立单（本单 **禁止**破坏性动作）；登记为设计事实 |
| **A-6** | `~/.ruagent/data/` | **我的探针留下的残余**：第一次用朴素只读打开那份整库副本，在用户数据目录里生成了 `ruagent.db.before-t229-cleanup-20260926-150802-shm`（**32,768 B**）与 `…-wal`（**0 B**）。改前该目录只有 1,359,872 B 那一个文件（t75 开工时的只读列举为证） | **我没有删它们**（本单禁止任何破坏性动作，且那是用户数据目录）⇒ 请裁决：由谁在何时删除这两个 SQLite 临时文件（`-wal` 为 0 B、`-shm` 为 32,768 B 草稿；活库 daemon 不会打开那份副本）。修复后复跑确认：**这次运行没有再新增任何文件**（前后列举一致） |

## 6 门禁与纪律

| 面 | 命令 | 读数 |
| --- | --- | --- |
| 测试面 | `scripts/cargo-team.ps1 test -p ruagent-memory` | **exit=0**；`test result:` **3 行全 ok**（`79 passed; 0 failed`（本单 +5）· `9 passed; 0 failed` 集成 · `0 passed` doctest）；`FAILED.`=**0**；`panicked`=**0**。**只覆盖 memory**。 |
| 门禁 | `scripts/cargo-team.ps1 clippy -p ruagent-memory --all-targets -DenyWarnings -CleanFirst`（契约里的裸形式） | **exit=0**，`^warning` 行 **0**，`^error` 行 **0**；两件证据 = `clean -p ruagent-memory (forced re-check)` + `Checking ruagent-memory v0.1.0` |
| 编译面（附加） | `scripts/cargo-team.ps1 check -p ruagent-memory --all-targets` | **exit=0**，`^warning` 行 **0**，`^error` 行 **0**（captain 的 P0 要求：连警告也 0） |
| 格式 | `rustfmt --edition 2024 --check crates/memory/src/lifecycle.rs` | **exit=0**，0 行 diff（只对本单文件跑；未 `cargo fmt --all`） |
| 只读性 | 真实 root 读数前后两次列举 `~/.ruagent/data/*.before-*` | **完全一致**（改后运行没有新增文件）⇒ `immutable=1` 兑现了「报告不写它报告的目录」 |

**四轮 P0 修复的账**（captain 提出的 4 条警告，逐条真修，**没有** `#[allow(dead_code)]`）：
1. `BackupFileReading.path`（:605）⇒ **删字段**（调用方直接从 `PathBuf` 取，重复且没人读）；
2. `BackupScan.inventory`（:617）⇒ **真的用上**：`tracing::debug!(target: "ruagent_memory::forget", inventory = ?scan.inventory)` ⇒ 逐文件清单（名/大小/判定）进日志，运维在产线路径上能看出「查了哪些、哪个读不出来」；
3. `unused Result`（测试里的 `flush_for_copy`）⇒ **两层都显式处理**（`.await.expect(...).expect(...)`），**明确拒绝 `let _ =`**：checkpoint 失败必须是可见失败；
4. 顺手纠正我自己的测量口径：只看 `error` 而不看 `^warning`、且不看 `--all-targets` —— 现在三个面（test / check / clippy）都同时报 `^warning` 行数。

## 7 未覆盖（明确写出，不静默）

- **wiki 侧备份**（`wiki-backups/{slug}.md`）：本 root 下该目录**尚未创建**（`~/.ruagent` 顶层只有 `config/data/knowledge/logs/models/skills/workspaces/worktrees`），所以本机**没有**这条读数；面本身已用 `ExternalResidual` 请求，owner 见 A-3。
- **`KnowledgeChunkFts` / `KnowledgeVectors` / `WikiBuildPage`**：报告未提及（A-2），本单不加读数。
- **知识/wiki 五面的实际内容**：由 owner 回答，本单没有越界查询。
- **备份的处置**（删除/加密/保留）：**本单明确不做**，也不给建议值 —— 涉及用户数据，须单独裁决（A-5）。
