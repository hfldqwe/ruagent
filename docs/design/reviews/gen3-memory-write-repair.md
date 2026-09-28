# 记忆写侧静默失真修复：F1（软删除堵死重写）+ F2（supersedes 静默 no-op/说谎审计）（t72 / mem-core）

- 任务：t72（kind=repair round 1）；attempt `98db836b-ddbf-442e-b30c-c65550993c94`
- inScope：`crates/memory/src/write.rs` · `crates/memory/tests/`（新增）· 本报告；**未改** `crates/store` / `crates/daemon` / `crates/mcp` / `crates/acp`
- 现场：临时 root `%TEMP%\ruagent-t72e2e` + 临时端口 **18792**（loopback）+ 私有 target 目录（用后已删）；自起 daemon **PID 73948 已停**（`still_running=False`，无残留）；活库未写、pid 79984 未启停

## 0 取证与「编译面 / 测试面」分开（第 6 条 + captain 本轮口径）

| 面 | 命令 | 读数 |
| --- | --- | --- |
| **编译面（全 workspace）** | `scripts/cargo-team.ps1 check --workspace --all-targets` | **exit=0** · `Finished dev profile in 15.44s`（跑到 `Finished` ⇒ 所有 target 都编译过，**没有** fail-fast 截断） |
| 编译面（历史窗口，如实记录） | 同上（本轮中段） | exit=101，错误在**同伴文件**：`crates/graph/src/lib.rs:1322` `error[E0277]: the trait bound `EventTimeSource: Default` is not satisfied`；且输出只到 `Checking ruagent-graph / ruagent-memory / ruagent-mock-agent` 就停了（**fail-fast ⇒ 其余 crate 未覆盖**）。这是 captain 点名的在途编辑窗口；**已由所有者修复**（复跑 `check -p ruagent-graph` exit=0，随后 workspace 复跑 exit=0）。 |
| **测试面（只覆盖 memory）** | `scripts/cargo-team.ps1 test -p ruagent-memory` | **exit=0** · `test result:` 行 **3** 行，全部 `ok`（`71 passed; 0 failed` 库内 + `9 passed; 0 failed` 新集成测试 + `0 passed` doctest）· 大小写敏感的 `FAILED.` 命中 **0** · `panicked` 命中 **0** |
| 门禁（clippy） | `scripts/cargo-team.ps1 clippy -p ruagent-memory --all-targets -DenyWarnings -CleanFirst ruagent-memory` | **exit=0** · 本轮自己的 `Checking ruagent-memory v0.1.0`（同锁窗口 clean+gate ⇒ 真重查） |

**测试面只说「只覆盖 memory」**：`test -p ruagent-memory` 不覆盖 daemon/acp/panel，也不覆盖同伴在途的 target；workspace 的测试面本单没跑（且按 captain 口径，`test --workspace` 不加 `--no-fail-fast` 会在第一个失败 target 后中止 ⇒ 读它必须报「跑到哪些 target」）。

**一条命令级缺陷（路由，不是我的代码）**：t72 契约里的 verify 行原文是
`powershell ... scripts/cargo-team.ps1 clippy -p ruagent-memory --all-targets -DenyWarnings -CleanFirst`（**`-CleanFirst` 没有 crate 参数**）。照抄执行得到 **exit=101**，错误是 wrapper 的**参数解析**错（`error: "--package <SPEC>" requires a SPEC format value`），**不是任何诊断**：`scripts/cargo-team.ps1:83-84` 是 `if ($a -eq '-CleanFirst') { $i++; $cleanFirst.Add(...) }` ⇒ 它必须跟一个 crate 名（wrapper 自己的头注释 `:47` 也是这么示范的）。⇒ 谁照抄这一行，会把「wrapper 参数错」读成「门禁红」。建议把 t72 契约的 verify 行改成带 `ruagent-memory` 的形式。

## 1 F1：软删除的行堵死同内容重写 —— 修好，但**必须先说清 t69 给的修法不够**

**改前读数**（t69 审计的载体：临时 root 的 in-memory DB）

```
删 id=3 → 同内容重写 outcome=SkippedDuplicate(3)；current_rows_visible=0
```

**t69 给的修法（补 `deleted_at IS NULL`）是必要的，但不是充分的** —— 我照做后立刻撞到第二条拦路：`memories` 上的 **`UNIQUE (store, namespace, content_hash)`**（`crates/store/src/migrations/0004_memory.sql:30`）**把墓碑行也算在键里**，所以「同内容再写」在 SQL 层根本插不进去：

```
READING t72（中间态，仅补 deleted_at 过滤）:
  outcome=Err(Sqlite(SqliteFailure(… extended_code: 2067,
           Some("UNIQUE constraint failed: memories.store, memories.namespace, memories.content_hash"))))
  current_visible=false
```

即：静默的 `SkippedDuplicate` 变成了**响亮的 UNIQUE 错误**（比静默好，但事实仍然回不来）。

**落地的修法（`crates/memory/src/write.rs`，两步）**
1. 去重只看**当前行**：`… AND superseded_at IS NULL AND deleted_at IS NULL`（与读侧 `query.rs:62-64` 同一个谓词 —— 一个事实一个来源）；
2. 键被**墓碑**占住时**复活它**：`UPDATE memories SET deleted_at = NULL, confidence = ?, updated_at = ? WHERE id = ? AND deleted_at IS NOT NULL`（判据 = 受影响行数 1，且这个数进审计的 `reason`）。

**改后读数（三层都给）**

| 层 | 读数 |
| --- | --- |
| 集成测试（in-memory，公开 API） | `READING t72 F1 rewrite after delete: outcome=Ok(Inserted(1)) current_visible=true current_rows=1`；审计 `ops=["insert","delete","insert"]`，末行 `reason="revived soft-deleted row id=1: … UNIQUE across tombstones … cleared deleted_at (affected 1 row) instead of inserting a new row"` |
| **HTTP 端到端**（临时 root `%TEMP%\ruagent-t72e2e`，端口 18792） | `POST /memory/write` → `HTTP 200 {"outcome":"Inserted(1)"}`；`DELETE /memory/1` → `HTTP 200 {"outcome":"deleted","soft":true}`；**再写同内容** → `HTTP 200 {"outcome":"Inserted(1)"}`；`GET /memory/list` → `{"memories":[{"id":1,…,"deleted_at":null,…}],"total":1}` ⇒ **事实真的回来了，且只有一行** |
| 审计（HTTP `/memory/diffs`） | 见 §5 的逐行读数：`insert`(id=1) → `delete`(id=1,"soft delete") → `insert`(id=1, reason 说明是复活) → `supersede`(id=1→2) → `supersede_refused`(id=1) |

**负控（不许把去重一起关掉）**：`READING t72 F1 negative control: first=Inserted(1) duplicate=SkippedDuplicate(1) rows=1` ⇒ 未删除的同内容重写**仍然** `SkippedDuplicate`，且不产生第二行。

**一个没被覆盖的角落（新发现，已落地为响亮拒绝）**：如果占键的墓碑**本身也被 superseded**，复活它并不能让它变 current，而第二行又插不进去 ⇒ 我加了第三条路径 `Step::InsertRefused`：**拒绝 + 审计 `op="insert_refused"` + 明说原因**，绝不静默丢。读数：

```
READING t72 F1 corner: outcome=Err(… "insert refused: the same content is held by tombstone id=1 which is also superseded …")
  audit_ops=["insert","supersede","delete","insert_refused"]
  last_reason="… held by tombstone id=1, which is ALSO superseded: reviving it would not make it current and a second row cannot exist; nothing was written"
```

**这一族的设计岔口（路由给 store / 设计 owner）**：墓碑占键有两种自洽的语义，今天只有实现选了一种而**没有任何地方写下来**：
- **(i) 复活（本单所取）**：`(store, namespace, content_hash)` 是「这条记忆」的永久标识，删除是墓碑，再写=复活；
- **(ii) 让键变成部分唯一**（`UNIQUE … WHERE deleted_at IS NULL`，需要新迁移）：删除后再写=一条**全新的行**，墓碑留着当历史。
今天这两种都"能解释"0400 行的行为，所以**必须由设计/迁移裁决一个并写下**；我选 (i) 的原因写进了代码注释与本文档：它不需要迁移、不动 schema，而且与 `restore_memory` 的语义（清除 `deleted_at` 让行重新可见）一致。

## 2 F2：`supersedes` 静默 no-op + 说谎审计 —— 修好，判据是**受影响行数**

**改前读数**（t69 载体：临时 root 的 in-memory DB）

```
C3 跨 store：outcome=Superseded{old:1,new:2} · old.superseded_at=NULL（仍 current） · 审计 op="supersede" before=id=1 after=id=2
C11 指向已 superseded 的 id：outcome=Superseded{old:7,new:9} · 同一句审计
对照：不存在的 id ⇒ FOREIGN KEY constraint failed
```

**改法**：`UPDATE` 加上 `AND deleted_at IS NULL`（与 F1 同一个「current」谓词），并**按 `affected` 判定**；`affected == 0` 时用一条 `SELECT EXISTS` 区分两种命运：
- 行**不存在** ⇒ 不拦截，继续走到 INSERT，让 **FOREIGN KEY** 拒绝（**行为不变**，判据要求如此）；
- 行**存在**但不是本 store×namespace 的当前行 ⇒ `Step::SupersedeRefused`：**不写任何行**，审计写 `op="supersede_refused"`（理由里带 `0 rows superseded`），函数返回 `Err`。

**改后读数**

| 场景 | 读数（集成测试 / HTTP） |
| --- | --- |
| 跨 store（C3 的复现场景） | `outcome=Err(… "supersede refused: `1` is not a current row of store `procedure` namespace `project:other` …")` · `old_still_current=true` · `procedure_rows_written=0` · 审计 `["insert","supersede_refused"]`（**没有** `supersede`） |
| 指向已 superseded 的 id（C11） | `outcome=Err(…)` · `v3_written=false` · 审计 `["insert","supersede","supersede_refused"]` |
| 指向被软删除的行 | `outcome=Err(…)` · `successor_written=false` · 审计 `["insert","delete","supersede_refused"]` |
| 不存在的 id（**行为不变**） | `Err(… extended_code: 787, "FOREIGN KEY constraint failed")` · 未写任何行 |
| **HTTP 端到端** | 合法 `supersedes` ⇒ `HTTP 200 {"outcome":"Superseded { old: 1, new: 2 }"}`；跨 store ⇒ **`HTTP 500`** `sqlite error: supersede refused: …`；不存在的 id ⇒ **`HTTP 500`** `sqlite error: FOREIGN KEY constraint failed` |

**负控（正常的 supersede 必须照旧）**：`READING t72 F2 negative control: outcome=Superseded { old: 1, new: 2 } old_still_current=false audit=[("insert",…), ("supersede","id=1","id=2")]` ⇒ 旧行离开读面、审计**照写** `supersede` 且 before/after 齐全。

**为什么用 `Err` 而不是新的 `WriteOutcome` 变体**：`WriteOutcome` 的 `Debug` 是**线格式**（HTTP body / CLI / MCP 透传，面板按首 token 映射 i18n，t52 已冻结三个正例串）⇒ 新变体会在 API/面板侧出现一个**没有 i18n key 的结果**，而拒绝的正确归宿是「带原因的失败」。`DbError` 是 store 的公开类型（不在本单 inScope，不能加变体），所以拒绝以「约束形状 + 明确文案」抛出，与「不存在的 id」那条已有的 FK 约束错误同类。**更干净的形状**（`DbError` 加领域变体、或 API 面 400 + 结构化字段）已路由，见 §6。

## 3 审计：每个 op 都要有「受影响行数」的依据（本单新增的防回归）

| 决策 | 落库依据（读数） |
| --- | --- |
| `insert`（真插入） | `INSERT` 成功；HTTP 端到端 `after="id=1"`、列表可见 |
| `insert`（**复活**） | `UPDATE … WHERE id=? AND deleted_at IS NOT NULL` 受影响 **1** 行 ⇒ 同一行 id、`current_rows=1`；审计 `reason` 里逐字写下 `affected 1 row` |
| `supersede` | `UPDATE … WHERE id=? AND store=? AND namespace=? AND superseded_at IS NULL AND deleted_at IS NULL` 受影响 **1** 行（新增判据）；`affected=0` 分成「不存在（FK 拒绝）」与「非当前（拒绝+审计）」 |
| `skip_dedupe` | 去重 `SELECT` 命中一条**当前**行（无写入） |
| `reject` / `supersede_refused` / `insert_refused` | **不写任何行**，且审计行**必须**存在并写明原因（`reason` 非空、op 与 `supersede` 不同名） |
| HTTP 逐行读数 | `insert(id=1,reason=null)` → `delete(id=1,"soft delete")` → `insert(id=1, reason="revived … affected 1 row …")` → `supersede(1→2)` → `supersede_refused(id=1, reason="0 rows superseded: …")` |

**family 断言**（集成测试 `every_audit_op_matches_what_really_happened`）：一次会话里依次产生 insert / skip_dedupe / supersede / supersede_refused，断言 op 序列**逐字**如此、且拒绝行的 reason 含 `0 rows superseded` ⇒ 「事件存在 ≠ 那件事发生过」这一族在本路径上**有负控**。

**未覆盖的一处（路由）**：`lifecycle.rs:96-99` 的 `delete` 与 `:139-146` 的 `restore` 依据的是**同闭包内的一次预读**（`already.is_none()`），不是 `UPDATE` 的受影响行数。单写者 actor 让它们今天等价，但口径与 write 侧不一致（同一族，严重度 low）。

## 4 公开面：MCP 说明 vs 实际发生（一个事实一个来源）

MCP 工具说明逐字（`crates/mcp/src/lib.rs:75`）：
> 「Store a long-term memory about the user, a project, or a lesson learned. Stores: … Optionally pass `supersedes=<memory id>` to replace it.」

| 输入 | 说明读起来是什么 | 实际发生（本单读数） | 一致？ |
| --- | --- | --- | --- |
| 同 store×namespace 的**当前** id | replace it | `Superseded { old, new }`，旧行离开读面，审计 `supersede` | ✅ 一致 |
| 别的 store×namespace 的 id | （未说明） | `HTTP 500` + 「不是本 store×namespace 的当前行」+ 审计 `supersede_refused`；**没有任何行**被写 | ✅ **不再分叉**（说明没承诺它；实现现在响亮拒绝而不是假装成功） |
| 不存在的 id | （未说明） | `HTTP 500` + `FOREIGN KEY constraint failed` | ✅ 同上 |

⇒ **不需要改 MCP 文档来对齐**（它没有说不成立的话），所以本单**没有**改 `crates/mcp`。两处可以更好、但都不在我 inScope，已路由（§6）：① 说明里写明「只替换同一 store+namespace 的当前条目」；② `api.rs` 把这两类拒绝映成 **400 + 明确原因**（今天是 500，且消息带 `sqlite error:` 前缀 —— 见 §6 的读数）。

## 5 门禁与范围

- 门禁读数见 §0（测试面 3 行 `ok` / 0 `FAILED.` / 0 `panicked` + exit 0；clippy exit 0 + 真重查行；workspace 编译面 exit 0）。
- changedPaths：`crates/memory/src/write.rs`（三步判定 + 受影响行数 + 三条拒绝/复活路径 + 审计措辞）· `crates/memory/tests/write_governance.rs`（**新增**，9 条集成测试、逐条带 `READING t72` 读数）· 本报告。**没有改** `store`/`daemon`/`mcp`/`acp`/`panel`/`cli`，因此**第 16 条的跨 crate 门禁未触发**（无跨 crate 文件改动）。
- 端到端纪律：临时 root `%TEMP%\ruagent-t72e2e` + 端口 18792（loopback，未用 `--allow-remote`）；daemon PID **73948** 记录并停止（`still_running=False`）；收尾按命令行匹配自己的临时路径复查 **0 残留**；活库未写、pid 79984 未启停。
- 磁盘：私有 target 目录用后已删（`%TEMP%\ruagent-t52-target`、`%TEMP%\ruagent-audit-t69\target`），按 captain 本轮口径。

## 6 路由（本单不改；每条带读数与坐标）

| # | 位置 | 问题（读数） | 建议 |
| --- | --- | --- | --- |
| R-1 | `crates/daemon/src/api.rs`（`memory_write` 的 `?` 传播） | 跨 store 的 `supersedes` ⇒ **HTTP 500** `sqlite error: supersede refused: …`（调用方错误却报 5xx，且带 `sqlite error:` 内部前缀） | 把「supersede 拒绝 / FK 拒绝」映成 **400** + 明确原因（可与 t59 的拒绝串一起做） |
| R-2 | `crates/mcp/src/lib.rs:75` | 说明未写「只替换同一 store+namespace 的当前条目」 | 补一句（**不要**另抄词表；t59 已在处理同一行的命名空间声明） |
| R-3 | `crates/store/src/migrations/0004_memory.sql:30` + 设计 | `UNIQUE (store, namespace, content_hash)` **跨墓碑**生效：这是「删除后再写」只能复活、不能新增的根因 | 设计裁决 (i) 复活 / (ii) 部分唯一索引（需迁移），并把选定语义写进 schema 注释与设计文档 |
| R-4 | `crates/memory/src/lifecycle.rs:96-99,139-146` | `delete`/`restore` 按**预读**而非受影响行数决定 op | 与 write 侧同口径（low） |
| R-5 | t72 契约的 verify 行 | 裸 `-CleanFirst` ⇒ wrapper **exit=101 参数错**（不是诊断） | 改成 `-CleanFirst ruagent-memory`（本单实际用的就是它） |

## 7 未做 / 边界（无静默跳过）

- **没有**改 `store`（schema/迁移）、`api.rs`、`mcp`、`panel` —— 判据都在 §6 路由。
- **没有**跑 `test --workspace`（会编译并跑同伴在途的测试）；本单的测试面**只覆盖 memory**，已按 captain 口径写明，并按第 6 条把编译面与测试面分开写。
- **没有**在活库或真守护进程上做任何取证：端到端全部在 `%TEMP%\ruagent-t72e2e`。
- `WriteOutcome` 的公开枚举与 `Debug` **一字未改**（线格式冻结面），因此 HTTP body 的三种正例串与 t52 的拒绝串逐字不变 —— 本单新增的拒绝**只**出现在 `Err` 路径与 `memory_diffs.op` 里（新 op：`supersede_refused` / `insert_refused`）。
