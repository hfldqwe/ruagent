# gen3 审计（只读）— 存储层本体（`crates/store/**`）

> 任务：t94。范围：`crates/store/**`（**只读**；我的唯一写入 = 本报告）。
> 审计者：verify（独立验证员，不是这一层的作者）。
> 纪律：**没有写活库**；**没有起真守护进程**（pid 79984 全程未触碰：`alive StartTime=2026/9/27 5:35:37`）；实验全部在**我自己的临时 root**（`%TEMP%\ruagent-t94`，端口 8818，pid 78232/53400/94080，逐个停过）与我自己的 DB 副本上做；临时文件已清（§6 有残留读数）。
> 归因：并发波次里工作；引用行号都在本轮读过。**两处任务前提我先用读数更正**（§2 S7），不用"看起来没有"下结论。

## 0 结论

**这一层最硬的问题不在并发，而在"版本账本"和"未知值兜底"。**

1. **S1（high）迁移脚本不可重放：25 个脚本里 20 个在重放时报错，而一旦某一行版本记录丢失，守护进程就完全起不来。** 我实测：在**我自己的** root 上删掉 `schema_migrations` 里最大那行（v25）后重启 ⇒ `health=False`，stderr 逐字 `Error: opening database … sqlite error: index ux_query_eval_gold_set_query already exists`。逐个版本的普查：**20/25 重放会 raise**，只有 v7/v13/v14/v16/v17 因为用了 `IF NOT EXISTS` 而幸免（DDL 普查：25 个文件里只有 7 个用过 `IF NOT EXISTS`）。
2. **S2（high/medium）版本表的"洞"会被静默前填。** 我删掉 v3（保留 v25）后重启：`rows=24`、v3 仍然缺失、守护进程健康、日志里没有任何提示 ⇒ 那个迁移建的东西**永远不会被建**，而"账本里有 v25"让一切都看起来正常。（根因：`migrations.rs:67` 用 `MAX(version)`，`:74` 跳过一切 `<= current`。）
3. **S3（medium）五个"文本 → 枚举"的映射用 `_ => Default` 兜底**（`lib.rs:564/582/608/630`）：未知文本变成 `Pending`/`DependsOn`/`Queued`/`EndTurn` —— 一个读不出来的 run 状态显示成"排队中"，一个读不出来的停止原因显示成"正常结束"。

## 1 方法与可复现仪器

| 仪器 | 命令 | 用途 |
| --- | --- | --- |
| 静态清点 | `python %TEMP%\t94-census.py`（30 个文件、actor/迁移/兜底/文件层/测试守卫） | 全部 finding 的源码侧 |
| **迁移实验** | `powershell -File %TEMP%\t94-run.ps1`（我自己的 root `%TEMP%\ruagent-t94`、端口 8818；EX1 新库 → EX2 挖洞 → EX3 删最大行） | S1/S2 的运行时读数 |
| **重放普查** | `python %TEMP%\t94-idem2.py`（每个版本一份 DB 副本，逐个 `executescript`） | S1 的 20/25 |
| CI 守卫 | `sed -n '101,110p' .github/workflows/ci.yml` | S5 |
| 用法普查 | `grep -c '\.call(' vs '\.call_flat('`（crates/cli 全仓） | S5 的 199 vs 48 |
| 残留读数 | `Get-ChildItem $env:TEMP` 下匹配 `ruagent*/ra-*/pw-*/vint*` 的目录合计 | S8 |

## 2 findings（10 条：high 1 / medium 4 / low-medium 1 / low 4）

### S1（high · 迁移可恢复性）**20/25 个迁移不可重放；丢一行版本记录 = 守护进程完全起不来**

- **file:line**：`crates/store/src/migrations.rs:59-96`（`apply` / `apply_one`）；`crates/store/src/sqlite.rs:50`（迁移在 `Db::open` 里跑，失败 ⇒ `open` 失败 ⇒ 进程退出）；最典型的一个脚本 `crates/store/src/migrations/0025_query_eval_gold_unique.sql`（裸 `CREATE UNIQUE INDEX`）。
- **复现**：
  ```powershell
  # 我做的三步（%TEMP%\ruagent-t94 是我自己的 root，端口 8818）
  # EX1 起一次（建库）→ EX2 删 v3 → EX2 再起 → EX3 删最大行(v25) → EX3 再起
  python %TEMP%\t94-idem2.py     # 逐版本重放普查
  ```
- **证据读数**：
  ```
  EX1 fresh: schema_migrations rows=25 MAX=25 MIN=1 contiguous=1  PRAGMA user_version=0
  EX3 after restart: health=False
      m3.err: Error: opening database
              Caused by: 0: sqlite error: index ux_query_eval_gold_set_query already exists
                         1: index ux_query_eval_gold_set_query already exists
  EX3 after the failed start: schema_migrations rows=23 MAX=24   ← 库没被改，下一轮会以同一错误再失败
  重放普查: re-appliable WITHOUT raising = 5 -> [7,13,14,16,17] ; RAISES = 20
      v1 table tasks already exists · v3 duplicate column name: result · v12 duplicate column name: selected_by
      v19 table query_eval_sets already exists · v24 error in view distill_recorded_outcomes: no such table: main.distill_log
      v25 index ux_query_eval_gold_set_query already exists …
  DDL 普查: 25 个文件里 if_not_exists 出现 0 次的有 18 个（0019/0020/0021/0022/0023/0024/0025 全是 0）
  ```
- **为什么重要**：`schema_migrations` 只是**账本**，schema 对象才是事实。这个设计把"账本 vs 事实"的不一致变成**产品整体不可用**（`Db::open` 失败 ⇒ 守护进程退出 ⇒ HTTP/MCP/注入/面板全灭），而错误信息只点名一个索引，没有任何恢复指引。可达路径包括：恢复一份备份、合并两台的 DB、`VACUUM INTO` 后手工整理、跨版本回滚后重开。**注意这与"每迁移一个事务"（`apply_one`）不冲突**：事务保证了"不半吊子"，但没有保证"可重放"，而恢复场景需要的正是后者。
- **可证伪的修复判据**：① 每个迁移脚本必须可重放（`IF NOT EXISTS` / 守卫式 `ALTER`；`0024` 的 view 重建要 `DROP VIEW IF EXISTS` 先）；② runner 在应用前**核对账本与事实**（例如版本行丢了但对象在 ⇒ 明确报"accounting 与 schema 不一致，按 X 修复"而不是抛 SQL 错）；③ 判据：对任意一个版本执行"删除该行 + 重启"，守护进程必须仍能启动（今天 20/25 不能）。
- **顺带一条同族证据**：`crates/store/src/sqlite.rs:152-158` 的测试注释写 *"Reopen: migrations are idempotent, data survives"*，但它测的是"25 行版本记录都在时的 no-op"——**它证明的是版本过滤器跳过了所有迁移，不是迁移本身可重放**。这就是为什么上表 20/25 从未被发现。
- **owner**：store owner。

### S2（high/medium · 静默前填）`MAX(version)` 假设账本连续：中间的洞永远不被补，也没有任何提示

- **file:line**：`crates/store/src/migrations.rs:66-70`（`SELECT COALESCE(MAX(version),0) FROM schema_migrations`）、`:72-79`（`if version <= current { continue }`）。
- **复现**：`powershell -File %TEMP%\t94-run.ps1` 的 EX2 段（`DELETE FROM schema_migrations WHERE version=3` 后重启）。
- **证据读数**：
  ```
  EX2 punched hole: rows=24 MAX=25
  EX2 after restart: health=True, rows=24, version 3 present=0
  重启后的日志里与 migration/error 相关的行: 只有一条无关的 WARN（chat option catalog refresh failed）
  ```
- **为什么重要**：`MAX=25` ⇒ `3 <= 25` ⇒ **v3 被"视为已应用"**。如果丢的是中间某行（恢复、手工整理、导入时的部分复制），那个迁移建的表/列/索引就**永远不存在**，而所有后续代码会以"远因不可见"的方式失败（例如"no such column"出现在某个不相关的端点）。这一条与 S1 方向相反（S1 是硬失败、S2 是静默通过），两者共用同一个根因：**账本被当成事实**。
- **可证伪的修复判据**：应用前断言账本连续（`COUNT(*) == MAX(version)` 且 `MIN=1`），不连续就**明确拒绝启动并列出缺失版本**（或按 `SELECT version` 的补集重放）。判据：挖掉任一中间版本行后重启，必须出现可读的"缺 vN"错误（今天：静默通过，且该迁移永不补）。
- **owner**：store owner。

### S3（medium · 未知值兜底）五个映射把读不出来的文本静默变成**合法状态**

- **file:line**：`crates/store/src/lib.rs:564`（`_ => TaskStatus::Pending`）、`:582`（`_ => EdgeKind::DependsOn`）、`:608`（`_ => RunStatus::Queued`）、`:630`（`_ => StopReason::EndTurn`）；对照 `:527` 的 `_ => None`（那是 `(Option, Option)` 配对，属正当用法）。
- **复现**：`sed -n '558,632p' crates/store/src/lib.rs`；运行时：向 `tasks`/`runs` 写一行未知状态文本（或在未来版本写的库上用旧二进制读），然后 `GET /api/v1/tasks`。
- **证据读数**（源码逐字，四个 from_str 都是**不可失败签名** `fn f(s: &str) -> T`）：
  ```
  fn task_status_from_str(s: &str) -> TaskStatus { match s { "in_progress"=>…, "blocked"=>…, "done"=>…, "cancelled"=>…, _ => TaskStatus::Pending } }
  fn edge_kind_from_str(s: &str) -> EdgeKind { … _ => EdgeKind::DependsOn }
  fn run_status_from_str(s: &str) -> RunStatus { … _ => RunStatus::Queued }
  fn stop_reason_from_str(s: &str) -> StopReason { … _ => StopReason::EndTurn }
  ```
- **为什么重要**：四者的兜底值都是"看起来正常"的状态：未知的 run 状态 → **Queued**（看起来在排队等调度）、未知任务状态 → **Pending**（看起来还没开始）、未知停止原因 → **EndTurn**（**失败看起来像正常结束**）、未知边类型 → **DependsOn**（图语义被改写）。这是本代反复吃的"不可判定被一个合法值掩盖"，而且它落在**所有区域之下**：面板、HTTP、MCP、注入全都会照着这个被改写的状态做判断。
- **可证伪的修复判据**：`*_from_str` 改成可失败（`Option<T>`/`Result`）或在 `_` 分支 `tracing::warn!` + 计数，并且**至少一个测试**断言"未知文本不会静默变成某个状态"。判据：往 `tasks.status` 插一个 `'donee'`，读回时要么报错/报未知，要么有一条 WARN —— 今天两者都没有（读到 `Pending`）。
- **owner**：store owner。

### S4（medium · 失败被伪装）删除路径三处把 SQL 失败变成"不存在"/"已删除"，其中一处**推翻了它自己注释里的不变量**

- **file:line**：`crates/store/src/lib.rs:743-750`（`delete_chat`：`conn.execute("DELETE FROM chats …").unwrap_or(0)`，doc `:739-740` 承诺 `false` = "id 不存在"）；`:766-793`（`delete_session`：`:767-771` `query_row(…).ok()` ⇒ `NotFound`；**`:781-784` `let _ = conn.execute("INSERT OR REPLACE INTO session_deletions …")`**；`:791` `let _ = conn.execute("DELETE FROM session_archives …")`）。
- **复现**：`sed -n '738,796p' crates/store/src/lib.rs`。
- **证据读数**：三处逐字：
  ```rust
  let n = conn.execute("DELETE FROM chats WHERE id = ?1", [&id]).unwrap_or(0);   // SQL 失败 => n=0 => Ok(false) "id 不存在"
  let source: Option<String> = conn.query_row("SELECT source FROM sessions …").ok();   // SQL 失败 => None => NotFound
  // Tombstone first: if the process dies between the two statements the session is still hidden …
  let _ = conn.execute("INSERT OR REPLACE INTO session_deletions (key, deleted_at) VALUES (?1, ?2)", …);
  let n = conn.execute("DELETE FROM sessions WHERE key = ?1", [&key]).unwrap_or(0);
  let _ = conn.execute("DELETE FROM session_archives WHERE key = ?1", [&key]);
  ```
- **为什么重要**：① `Ok(false)` 有两种含义（"不存在"与"删失败"），调用方与测试都只能看到一个布尔；② `NotFound` 同样吞掉 SQL 失败；③ **tombstone 写失败被丢弃，而会话行照删、返回值照 `Deleted`** —— 注释（`:778-780`）说这个顺序是为了"进程死掉也不会让会话重新出现"，但**写失败**这一路径让"没有 tombstone 的删除"回 `Deleted`，索引器会把它重新建回来（正是注释要防的那件事）；④ 档案行的删除失败同样被丢 ⇒ 留下孤儿 `session_archives` 行而调用方以为删干净了。
- **可证伪的修复判据**：这三处改用 `call_flat`，让 `Ok(false)`/`NotFound` 只表示真的没有行；tombstone 写失败必须**中止**删除并返回错误（它是不变量的承载者，不能 best-effort）。判据：让 tombstone 的 INSERT 失败（例如临时加一个冲突触发器），返回值必须是错误而不是 `Deleted`（今天：`Deleted`）。
- **owner**：store owner。

### S5（medium · 测量面）CI 的 nested-Result 守卫只覆盖**一种文本形状**，而 `call` 仍是 4:1 的主流用法

- **file:line**：`.github/workflows/ci.yml:101-110`（守卫本体，**在已提交版本里**——与 t65 的 evidence 步骤不同）；被它保护的契约在 `crates/store/src/sqlite.rs:68-79`（`call` 的 NOTE）与 `:94-109`（`call_flat`）。
- **复现**：
  ```bash
  sed -n '101,110p' .github/workflows/ci.yml
  grep -rn --include='*.rs' -c '\.call('   crates cli | awk -F: '{s+=$2} END {print s}'   # 199
  grep -rn --include='*.rs' -c '\.call_flat\(' crates cli | awk -F: '{s+=$2} END {print s}' # 48
  ```
- **证据读数**：守卫 = `grep -rn -A8 '\.call(' crates cli | grep -E '\.await[[:space:]]*\.(is_err|is_ok)\(\)'` ⇒ 它只抓"`.call(` 之后 8 行内出现 `.await.is_err()` / `.await.is_ok()`"这一种形状。而 `sqlite.rs:76-77` 自己点名的失败形状有 **≥4 种**：`if let Err(e) = call(..)`、`.await.is_err()`、`let _ = call(..)`、以及"只对 `?` 作用于外层、内层 `Result` 被直接丢弃"。这几种**都不会**被这条 grep 命中。用法普查：**`call` 199 处 vs `call_flat` 48 处**（t324/t327 之后新写法仍占少数）。
- **为什么重要**：守卫的存在感大于它的覆盖面——CI 打印 "no nested-Result misjudgement found"，而它审计的是一个**子集**；`sqlite.rs:98-102` 的注释说得很重（"t324 found 17 places where that nesting made a failure invisible"），守卫却是文本级的窄网。
- **可证伪的修复判据**：① 把 `call` 变成非 `pub`（或 `#[deprecated]`）⇒ 形状不可表达，比 grep 强；② 若只能用 grep，扩到四种形状并在 CI 里对**新增**的 `call(` 计数设上限（ratchet）。判据：写一行 `db.call(|c| c.execute("UPDATE …", [])).await?;`（忽略内层 Result），CI 必须红 —— 今天不红。
- **owner**：store owner + CI owner。

### S6（medium · 漂移无检查）迁移登记表是**手写** `include_str!` 列表，版本号来自**数组下标**

- **file:line**：`crates/store/src/migrations.rs:8-37`（`MIGRATIONS` + `SCHEMA_VERSION = MIGRATIONS.len()`）、`:72-73`（`let version = (i + 1) as i64;`）。
- **复现**：`grep -rn 'read_dir\|\.sql"' crates/store/src/*.rs`（只有 `include_str!` 行，没有任何目录核对）；`ls crates/store/src/migrations/*.sql | wc -l`（25）对照 `grep -c 'include_str!' crates/store/src/migrations.rs`（25）。
- **证据读数**：25 个文件 ↔ 25 个条目 ↔ EX1 实测 `rows=25, MIN=1, MAX=25`（**今天一致**）；但一致性**没有任何机器检查**，而版本号不是文件名里的数字（`0025_…` ⇒ 数组第 25 个），是"位置"。⇒ ① 往目录丢一个 `.sql` 却不加进数组 = **文件存在、永不应用**（静默）；② 在中间插入/重排 ⇒ 已迁移的库因为 `version <= current` **跳过新脚本**（静默），或把**另一个脚本**当成已应用的版本号（错位）。
- **为什么重要**：这是"事件存在 ≠ 那件事发生过"的文件级版本：目录里有 26 个迁移，账本里只有 25 个应用记录，没有任何东西会红。
- **可证伪的修复判据**：一条测试（或 build script）断言"`migrations/*.sql` 排序后的名字集合 == `include_str!` 列表"且"文件名的数字 == 下标+1"。判据：往目录放一个 `0026_x.sql` 不动数组，测试必须红（今天绿）。
- **owner**：store owner。

### S7（low-medium · 前提更正）任务给的两条前提在这棵树上不成立（用读数说）

- **`PRAGMA user_version` 本仓不用**：EX1 实测 `pragma_user_version = 0`（fresh root，25 个迁移全应用后仍是 0）；`crates/store` 里 `user_version` grep 命中 **0**。版本来自 `schema_migrations(version INTEGER PRIMARY KEY, applied_at)`（`migrations.rs:61-64`）。⇒ "user_version 与文件名不一致时怎么办"这个问题应当改写成"`schema_migrations` 与 `migrations/*.sql` 不一致时怎么办"（= S1/S2/S6）。
- **`crates/store` 里没有 LanceDB 实现**：`crates/store/src` 的 30 个文件 = `lib.rs`（仓储）/`sqlite.rs`（actor）/`migrations.rs` + 25 个 `.sql`/`fts.rs`/`transcript.rs`（JSONL）。全 crate `lancedb::|LanceDb` 命中 **0**；真正依赖 `lancedb 0.38` 的是 **`crates/knowledge`**（其 Cargo.toml description 逐字 "…LanceDB + FTS hybrid retrieval"）。⇒ 这一层实际是"SQLite + JSONL 两种实现 + trait"，"三实现一致性"应审的是 `crates/knowledge` 的向量层与 `crates/store` 的 SQL 层**跨 crate 的语义**（例如"找不到/空结果/写失败"三种结局在两侧是否同义），本单只做到 SQL/JSONL 两侧。
- **为什么写进 findings**：审计的第一个动作是核对象集；把不存在的东西写进范围，后面每一条都会歪。
- **可证伪的修复判据**：任务/契约里把"三实现"改成"SQLite + JSONL（`crates/store`）与 LanceDB（`crates/knowledge`）"；判据：`crates/store/Cargo.toml` 的 description 不再声称 LanceDB（它今天逐字写着 "SQLite/LanceDB/JSONL implementations"）。
- **owner**：captain（契约/对象集）+ store（description）。

### S8（low · 残留）临时目录残留实测：**store 自己的仪器留下 ~298 MB**；全 `%TEMP%` 匹配目录合计 **78.4 GB**

- **file:line**：`crates/store/src/migrations.rs:1177-1182`（`%TEMP%\ra-t36-empty` 的建/删模式）、`:770`/`:897`（两个 `#[ignore]` 活库仪器）、`crates/store/src/lib.rs:1285`（第三个）、`crates/store/src/sqlite.rs:143-159`（`ruagent-db-test-<pid>` 的建/删）。
- **复现**：
  ```powershell
  Get-ChildItem $env:TEMP -Directory | Where-Object { $_.Name -match '^(ruagent|ra-|pw-|vint)' } |
    ForEach-Object { [pscustomobject]@{ N=$_.Name; B=(Get-ChildItem $_.FullName -Recurse -File | Measure-Object Length -Sum).Sum } }
  ```
- **证据读数**（我这一轮）：
  ```
  匹配目录 598 个，合计 78,399,064,107 B ≈ 78.4 GB
    其中 %TEMP%\ruagent-team-target = 77,437,952,973 B (72.12 GB)   ← 全队共享 cargo 构建树（合法但巨大）
  store crate 自己的残留：
    ra-t36-target            272,763,150 B   ← 一个 CARGO_TARGET_DIR（t36 仪器的构建树）
    ra-t36-pre                24,903,753 B
    ra-t36-empty                 438,272 B
    （另：ra-t65-nomock 286,483,968 B 是 daemon 测试的 root；ruagent-graph-fixture-* 各 ~4.6 MB 是 graph 的夹具）
  我的收尾：删掉我自己的 root 后，属于我的目录 = 0
  ```
- **为什么重要**：`sqlite.rs:143-159` 与 `transcript.rs:86-117` 的测试**在成功路径上会删**自己的临时目录 ✓，但 `ra-t36-*` 与 `ra-t65-*` 这类**跨进程/被 `#[ignore]` 的仪器**把几百 MB 留在 `%TEMP%` 且不再回收；在"90 GB 级泄漏史"的背景下，这条读数应该进账本（谁留的、能不能清）。
- **可证伪的修复判据**：`ra-t36-*`/`ra-t65-*` 这类固定名字的目录要么由一个 `cleanup` 入口回收，要么在仪器开头 `remove_dir_all`（今天 instrument 只在部分路径删）；判据：跑完一次 `--include-ignored` 后 `%TEMP%` 里这些前缀的目录数不增长（今天会增长）。
- **owner**：store owner + 仪器 owner。

### S9（low · 取消语义）`Db::call` 里 `let _ = tx.send(f(conn))`：调用方放弃后**写仍会执行**，结果被丢弃

- **file:line**：`crates/store/src/sqlite.rs:85-92`（尤其 `:88` `let _ = tx.send(f(conn));`）。
- **复现**：`sed -n '80,95p' crates/store/src/sqlite.rs`。
- **证据读数**：闭包在被取到队列里之后**一定会执行**（`op(&mut conn)`，`:59-61`），而把结果送回调用方的 `tx.send` 失败被 `let _` 丢弃 ⇒ 若调用方已经不再等待（请求被取消/超时、任务被 abort），**这个写照样落库**，没有任何人知道它发生了。反面（`DbError::Closed`）只覆盖"队列关闭"，不覆盖"接收方消失"。
- **为什么重要**：HTTP 层的请求取消（客户端断开）不会阻止已经入队的写；这对"删除/归档/纠偏"这类操作是可接受的默认，但**没有任何地方写明**这是有意的（对比 `unbounded_channel` 也没有背压）。本条是"报文级"读数：我读的是代码，没有构造取消场景（§5）。
- **可证伪的修复判据**：`call` 的 doc 明确写"已入队的写不可取消"，或在闭包执行前检查 `tx.is_closed()` 并跳过（可以省掉一次无意义的写）。判据：取消接收方后，日志/账本上能看出那次写是"照做了"还是"被跳过"。
- **owner**：store owner。

### S10（low · 失败不留痕）`Db::open` 迁移失败后，磁盘上留着一个**半初始化且不可用**的 DB，且不会自我修复/隔离

- **file:line**：`crates/store/src/sqlite.rs:31-52`（`open` → `create_dir_all(parent)` → `Connection::open(path)` → `configure_and_spawn`（其中 `:50` 跑迁移））。
- **复现**：`powershell -File %TEMP%\t94-run.ps1` 的 EX3 段（删掉最大版本行后重启）。
- **证据读数**：EX3 启动失败后，同一个 DB 文件仍在 `%TEMP%\ruagent-t94\data\ruagent.db`，`schema_migrations rows=23 MAX=24` —— **状态没变，所以下一次启动会以同一个错误再失败**（错误是同一个脚本对同一状态的确定性结果；我只观测到一次失败，重复观测未做，见 §5）。
- **为什么重要**：产品在这一刻是"全部不可用 + 需要人手工修 24 行账本"，而 `ruagent doctor`/启动路径里没有"检测账本与 schema 不一致并给出修复命令"的入口。
- **可证伪的修复判据**：`open` 失败时打印可执行的修复指引（或提供 `ruagent db-doctor --repair-accounting`）；判据：人为制造 S1 的状态后，启动输出里能直接看到"怎么做"（今天只有 SQL 错误）。
- **owner**：store owner + cli owner。

## 3 正面读数（查过、**不是**缺陷）

1. **每迁移一个事务、且版本行与 DDL 同事务**（`migrations.rs:87-96`：`BEGIN → execute_batch(script) → INSERT version → COMMIT`）⇒ 不存在"应用了但没记账"的半吊子状态 ✓（问题在**可重放**与**账本校验**，不在原子性）。
2. **`call` 与 `call_flat` 的双入口 + 一条证明嵌套语义的测试**：`sqlite.rs:166-192` 在**同一条失败语句**上验证 `call` 外层 `Ok`/内层 `Err` 而 `call_flat` 返回 `Err(DbError::Sqlite)` ✓ —— 这是本代少见的"把陷阱写成测试"。
3. **连接配置**：`busy_timeout=5s`、`journal_mode=WAL`、`synchronous=NORMAL`、`foreign_keys=ON`（`sqlite.rs:46-49`），单一命名写线程（`ruagent-db-writer`），句柄全丢后best-effort `wal_checkpoint(TRUNCATE)`（`:62-63`）✓。
4. **并发写测试真的并发**：`sqlite.rs:117-139` 起 32 个 tokio 任务并发插入再断言 `len()==32` ✓。
5. **JSONL 侧对"撕裂行"有测试**：`transcript.rs:111-117` 写一个被截断的 JSON 行并断言读取行为 ✓（`remove_dir_all(...).ok()` 只出现在测试清理里）。
6. **`DbError` 二分语义清楚**：`Sqlite`/`Closed`/`Io`（`sqlite.rs:10-18`），`Closed` 的文案是 "database writer is shut down" ✓。
7. **本 crate 没有静默跳过**：`#[ignore]` 3 处（`lib.rs:1285`、`migrations.rs:770/897`）都带 env 变量说明，且用 `expect` 大声失败；t70 的全仓 `#[ignore]` 清单把这 3 个列为"declared not covered" ✓。
8. **账本与磁盘今天一致**：EX1 实测 25 行、MIN=1、MAX=25、连续 ✓（S6 说的是"没有检查"，不是"已经错了"）。

## 4 未验证猜想（**不是** finding）

- **G1** S1 的**可达性**我只证了"状态可被造成 + 后果严重"，没证"正常运维会走到这里"。真实路径候选：备份恢复、两库合并、`VACUUM INTO` 后手工整理、跨版本回滚。取读数的办法：在临时 root 上模拟"把 WAL 删掉后打开"与"只复制 .db 不复制 -wal"，看哪种能造出"对象在、账本行不在"。
- **G2** S3 的**版本偏斜**场景（未来版本写入新状态文本、旧二进制读）我没有实际构造；我用的是源码级读数（四个 `_` 分支）。
- **G3** S4 的失效注入（让 tombstone 的 INSERT 失败）我没有做；我只读了 `let _` 的形状与注释里的不变量。
- **G4** S9 的取消场景（接收方 drop 后写是否照做）我**没有**构造实验，只有代码读数。
- **G5** `ruagent-db-test-<pid>` / `ruagent-tr-*` 在**测试 panic** 时是否残留：我只做了"现在磁盘上有没有"的读数（没有），没做"panic 中途"的读数。

## 5 未覆盖范围（每条写原因）

| 未覆盖 | 原因 |
| --- | --- |
| `crates/knowledge` 的 LanceDB 向量层（trait 语义一致性、锁、损坏、清理） | **不在本单 inScope**（本单 `crates/store/**`）；而且它不属 store crate（§2 S7 已更正前提）。建议单独立项（t94 的结论里已写明它才是"第三实现"）。 |
| `cargo test -p ruagent-store` 的独立重跑 | 本单的读数全部来自我自己的进程与 DB 副本；store 的测试面已被 t20 的 `test --workspace`（exit 0 / 55 target）与 t70 的 `#[ignore]` 清点覆盖，我没有为它再占一次编译锁（并发波次在跑）。 |
| 事务嵌套 | store **没有**公开嵌套事务 API（`apply_one` 用 `conn.transaction()`，业务侧用 `call(conn)` 手写语句）⇒ "嵌套事务"这条前提在本 crate 里没有对应物；我按对象集实数处理，没有硬造场景。 |
| 跨进程/多连接的锁竞争 | 本 crate 的设计就是"单连接 + 单写线程"，多进程场景（两个守护进程同 root）我没有测（`busy_timeout` 与 WAL 的行为属于 SQLite 语义，不是本仓字节）。 |
| S1/S2 的**修复后**对照读数 | 我的 inScope 只有报告，不能改 `crates/store`；"修好后应仍能启动"是可证伪判据，不是我给出的读数。 |
| S9/G4/S10 的重复观测与失效注入 | 见 §4，逐条写了取数办法。 |

## 6 我这一轮的操作与收尾（残留读数）

- **我起的进程（全部记录、全部已停）**：临时 daemon pid **78232**（EX1/m1）、**53400**（EX2/m2）、**94080**（EX3/m3，**启动失败**），同一 root `%TEMP%\ruagent-t94`、端口 **8818**；`stop` 后 `alive=False` ×3。**真守护进程 pid 79984 全程未触碰**（每步后 `alive StartTime=2026/9/27 5:35:37`）。
- **我没有写活库**：所有 SQL 手术都在我自己 root 的 `data\ruagent.db` 与我自己的副本上。
- **我写入的文件**：只有本报告 `docs/design/reviews/gen3-audit-store.md`；`crates/store/**` 零字节改动。
- **收尾残留读数**：删掉 `%TEMP%\ruagent-t94` 后，属于我的目录 = **0**；`%TEMP%` 匹配前缀的目录数 598 → 598（我只清理自己的）；**不是我的、仍然留在盘上的**：`ra-t36-target` 272.8 MB、`ra-t36-pre` 24.9 MB、`ra-t36-empty` 0.44 MB、`ra-t65-nomock` 286.5 MB、`ruagent-graph-fixture-*` 若干（各 ~4.6 MB）—— 见 S8。
- 仪器（`%TEMP%`）：`t94-census.py`、`t94-run.ps1`、`t94-idem2.py`、`t94-harness.log`。
