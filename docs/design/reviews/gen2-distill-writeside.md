# distill 写入侧：一次尝试一行（t26 / R-2 · I-C）

> 任务：I-SCHEMA-2（t25，迁移 `0024_distill_attempts.sql`）把 `distill_log` 的主键从「每会话一行」改成「每次尝试一行」之后的**写入侧收口**。
> 改动面：`crates/daemon/src/distill.rs`（唯一代码文件）+ 本报告。没有动 `crates/store/`、`crates/mock-agent/tests/e2e_daemon.rs`、`crates/memory/`、`crates/knowledge/`、`panel/`。
> 环境：全部 cargo 走 `scripts/cargo-team.ps1`（共享 target、一次一个编译、CPU 0-11、BelowNormal）；真守护进程 pid 79984 只读、未启停、未写 `~/.ruagent`。

## 0 一句话

`distill_log` 的写入从「每会话一行 + `ON CONFLICT(session_key)` 覆盖」改成「**每次尝试一行**（普通 INSERT）」。改前：同一会话一次成功 + 一次失败 → 库里 **1 行**，且成功的 `memories_written` 被失败覆盖销毁；改后：**2 行**，各有自己的 `status`/`failure_reason`/`prompt_hash`。本窗口读数 **`distill_recorded_outcomes` 行数 − 日志尝试数 = 0**，`test -p ruagent-daemon --lib distill` **13 passed / 0 failed**。

## 1 迁移前后同一段写入（原文）

**改前**（`crates/daemon/src/distill.rs` 的 `log_outcome`，`ON CONFLICT(session_key)` 版本，逐字）：

```sql
INSERT INTO distill_log
    (session_key, distilled_at, memories_written, entities_written,
     relations_written, agent, status, failure_reason, prompt_hash)
VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
ON CONFLICT(session_key) DO UPDATE SET
    distilled_at = excluded.distilled_at,
    memories_written = excluded.memories_written,
    entities_written = excluded.entities_written,
    relations_written = excluded.relations_written,
    agent = excluded.agent,
    status = excluded.status,
    failure_reason = excluded.failure_reason,
    prompt_hash = excluded.prompt_hash
WHERE excluded.status <> 'failed'
   OR distill_log.status IS NULL
   OR distill_log.status = 'failed'
```

**改后**（现在文件里的原文，`crates/daemon/src/distill.rs:340-357`）：

```sql
INSERT INTO distill_log
    (session_key, distilled_at, memories_written, entities_written,
     relations_written, agent, status, failure_reason, prompt_hash)
VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
```

**为什么旧形状必须改（不是风格问题）**：0024 重建了该表，`session_key` 不再有唯一约束 ⇒ `ON CONFLICT(session_key)` **在 PREPARE 阶段**就报
`ON CONFLICT clause does not match any PRIMARY KEY or UNIQUE constraint`（SQLite：`Error code 1: SQL error or missing database`）。这不是推理：迁移落地后本单的 `the_log_records_three_states_and_a_failure_keeps_a_success` 当场变红，panic 行就是这句 `log_outcome(...).await.unwrap()`，错误文本逐字如上。

## 2 被删掉的那段保护为什么不再需要

旧谓词
```sql
WHERE excluded.status <> 'failed'
   OR distill_log.status IS NULL
   OR distill_log.status = 'failed'
```
存在的唯一理由是**键冲突**：既然 `session_key` 是主键，第二次尝试必然 UPDATE 第一行，于是必须用一个条件决定「这次 UPDATE 该不该发生」，否则一次失败会把「这个会话成功过」这个事实删掉。

改后冲突本身消失了：**两次尝试是两行**，「失败不覆盖成功」不再需要任何裁决 —— 成功的行还在原处（`id` 更小），失败的行是新的一行（`id` 更大）。旧谓词在这里不但多余，而且**它的语义在新形状下更弱**：它只能保住「该会话曾有成功」，保不住「成功时写了什么」（`memories_written`/`entities_written` 会被 `excluded.*` 覆盖）。行级事实保住的是全部字段，这正是可审计性要求的东西。因此**整段删除，不加替代保护**（没有 `WHERE`、没有 `DO NOTHING`、没有应用层判断）。

「最新一次尝试」需要单独读时用 `ORDER BY id DESC LIMIT 1`（`id` 是 `INTEGER PRIMARY KEY` = rowid，插入序单调）。

## 3 一处命名偏差（登记，不擅自改 schema）

契约写的是「带 `status` + `failure_reason` + `prompt_hash` + `attempted_at`」。0024 里**没有 `attempted_at` 列**：它保留的是原有且 `NOT NULL` 的 `distilled_at`（= 本行的写入时刻，也就是这次尝试的时刻）。因此本单把 **`attempted_at` 实现为 `distilled_at`**（每次尝试都写 `Utc::now().to_rfc3339()`），而不是新增列 —— 新增列要改 `crates/store`，不在本单 inScope，也没有必要。若评审要求列名严格叫 `attempted_at`，那是一张 I-SCHEMA 单，本单按上面的映射交付。

## 4 读数（改前 → 改后）

| | 改前 | 改后 |
| --- | --- | --- |
| 同一会话：1 次成功 + 1 次失败 | **1 行**（成功的 `memories_written` 被失败覆盖成 0；recall 在真副本上用直接 SQL 复现过） | **2 行** |
| 「库行数 − 日志尝试数」 | 恒为负（4 次尝试 → 1 行） | **0** |
| 失败率可从库里读 | 不能（只在 `daemon.log`；实测某窗口 22 次尝试 / 21 次失败 / 库里 1 行） | 能（按 `status` 分布；分母用视图） |

测试输出（`... test -p ruagent-daemon --lib distill -Nocapture`，**13 passed / 0 failed**）：

```
READING one session, two attempts: [(1, "ok", None, Some("hash-a")), (2, "failed", Some("Query returned no rows"), Some("hash-a"))]
READING the succeeded attempt after a later failure: ("ok", None, 3)
READING the newest attempt of that session: failed
READING a first-attempt failure: ("failed", Some("Query returned no rows"), Some("hash-b"))
READING distill_log by status: [(Some("empty"), 1), (Some("failed"), 2), (Some("ok"), 1)]
READING R-2: attempts=4 rows=4 recorded_outcomes=4 | rows - attempts = 0
```

逐条对应契约的验收项：
- **一次成功 + 一次失败 ⇒ 库里 2 行**：第一行读数（`id=1 ok` 与 `id=2 failed`），且成功行**没有** `failure_reason`、失败行带原因与它自己的 `prompt_hash`；
- **成功不被摧毁**：`SUM(memories_written)` 的见证是 `("ok", None, 3)` —— 该会话的成功行仍写着 3；
- **三态分布**：`[(empty,1),(failed,2),(ok,1)]`（4 次尝试 4 行，与 `attempts=4` 对齐）；
- **视图当分母**：`recorded_outcomes=4`；视图定义是 `status IS NOT NULL`，所以 33 行历史（`status` 全 NULL，0024 不按内容猜）不会进入分母 —— 用 `COUNT(*) FROM distill_log` 会把它们算进去，那正是 0020 拒绝 `DEFAULT 'ok'` 的同一个错误；
- **失败绝不创建 episode**：测试先 `write_memories` 造出 1 条 episode，再制造失败，断言 episode 数不变（t350 的徽章只认 `episodes.kind='run_turn'`）；
- **测试用临时 root**：`std::env::temp_dir()` + 进程内原子计数器（时间戳分辨率不足，早期版本因此撞过目录）。

## 5 门禁：通过，且附「覆盖了哪些 target」「确实重查了哪个 crate」两件证据

**先强制重编**（否则「绿」可能是缓存命中 —— 本代已被这种读数骗过）：

```
[cargo-team] cargo clean -p ruagent-daemon
     Removed 974 files, 3.5GiB total
```

1. `test -p ruagent-daemon --lib distill` → **exit 0 / 13 passed**，输出里有本次 `Compiling ruagent-store … knowledge → memory → graph → daemon`（30.5s，真重编）。
2. `check -p ruagent-daemon --all-targets --verbose` → **exit 0**；逐 target 的 `--crate-name` 清单（11 个）：`ruagent_core, ruagent_store, ruagent_acp, ruagent_orchestrator, ruagent_knowledge, ruagent_graph, ruagent_memory, ruagent_daemon, smoke, knowledge_api, injection_e2e`。
3. `clippy -p ruagent-daemon --all-targets -DenyWarnings --verbose` → **exit 0（Finished 8.68s）**，逐 unit 的 `Running clippy-driver … --crate-name …` 行给出**本次真的被 lint 的编译面**：
   - `--crate-name ruagent_daemon … crates\daemon\src\lib.rs --crate-type lib`（**lib**）
   - `--crate-name ruagent_daemon … crates\daemon\src\lib.rs --test`（**lib test** —— `-D warnings` 就是卡在这个 unit 上的那一类）
   - `--crate-name smoke … tests\smoke.rs --test`
   - `--crate-name knowledge_api … tests\knowledge_api.rs --test`
   - `--crate-name injection_e2e … tests\injection_e2e.rs --test`
   - 依赖链 6 个：`ruagent_core / ruagent_store / ruagent_acp / ruagent_orchestrator / ruagent_knowledge / ruagent_graph / ruagent_memory`

   即：**本代那三条误读的根因在这条读数里被排除了** —— 「首个失败的 target 挡住其后全部」不适用于这次（它 exit 0，且逐 unit 的 `Running clippy-driver` 行证明 lib、lib test、以及三个集成 test target 都被**真正重新 lint 过**）。
   注：`--verbose` 的逐 unit 行是**这次**才拿到的方法；上一轮（gen2-graph-impl.md §10 R-2b）我只声明了「这次调用 exit 0」，因为当时没拿到这批行 —— 这次补齐了第②件证据。

## 6 两个现成读法（写进 `log_outcome` 的文档注释，供后来者不再猜）

```sql
-- 1. 「这个会话试过几次、每次什么结果」：按尝试顺序，每行一次尝试
SELECT * FROM distill_log WHERE session_key = ?1 ORDER BY id;

-- 2. 「蒸馏失败率」：分母必须是有记载结果的行
SELECT status, COUNT(*) FROM distill_recorded_outcomes GROUP BY 1;
--    绝不要用 COUNT(*) FROM distill_log：33 行历史 status 全为 NULL
--    （0024 拒绝按内容猜，正是 0020 拒绝 DEFAULT 'ok' 的同一个理由）
```

## 7 跨区发现（只报告，不改别人的文件）

1. `crates/mock-agent/tests/e2e_daemon.rs:2182`（`distill_graph_toggle_controls_entity_extraction`）用
   `query_row("SELECT entities_written FROM distill_log WHERE session_key = ?1")` 读；多行时 SQLite 返回**最旧**一行（无 `ORDER BY` ⇒ rowid 序）。该会话只蒸馏一次时断言 `Some(0)` 仍成立、不会红，但语义应改成「该会话至少一次尝试」或 `ORDER BY id DESC LIMIT 1`。**captain 已把该文件并入 t19（integ）的 inScope**，本单未动。
2. `crates/daemon/tests/injection_e2e.rs:773` 的 `collapsible_if`（wiki 在 00:4x 报的）在本单读数里**不复现**：`--all-targets -DenyWarnings` exit 0，且 `injection_e2e` 这个 test target 确实被重新 lint 过（见 §5 第 3 条的逐 unit 行）。integ 已改过该文件。

## 8 未测 / 不声称

- **活库读数 not_measured**：真库仍是 v18（0024 未应用），这是正确的；本单所有读数都在**临时 root**上取。真副本升级（33 → 33 行、`status` 33/33 NULL、分母视图 2 of 35）由 t25/recall 在自己的报告里给出，本单不重复声称。
- **端到端（daemon 全链路触发一次蒸馏失败）not_measured**：需要 mock-agent 的 e2e 场景，归 t19；本单的证据面是 `distill_log` 写入路径本身。
- 未新增列、未改 schema、未改任何 store 文件。

## 9 复现命令

```powershell
cd C:\Users\19410\Documents\ai\ruagent
# 强制重编（否则可能是缓存命中）
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 clean -p ruagent-daemon
# 三条 verify（契约原文的等价形状）
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-daemon --lib distill -Nocapture
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 check -p ruagent-daemon --all-targets
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 clippy -p ruagent-daemon --all-targets -DenyWarnings
# 逐 target 的编译面证据（两件证据里的第②件）
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 clippy -p ruagent-daemon --all-targets -DenyWarnings --verbose
# 确认旧形状已不存在（SQL 语句形状应为 0 命中）
Select-String -Path crates\daemon\src\distill.rs -Pattern "ON CONFLICT"
Select-String -Path crates\daemon\src\distill.rs -Pattern "INSERT INTO distill_log"
```

实测（本单交付时）：
- `ON CONFLICT` → **1 命中，且只在文档注释里**（`distill.rs:312` 的 `/// \`ON CONFLICT(session_key)\` now fails at PREPARE time with …` —— 那是解释这条错误为什么不能再用的说明文字）；
- `ON CONFLICT\(session_key\) DO UPDATE` → **0 命中**（可执行的旧 SQL 已彻底不存在）；
- `INSERT INTO distill_log` → **1 命中**，就是 §1 里那段普通 INSERT。

（更正记录：本报告初稿在这里写「`ON CONFLICT\(session_key\)` → 0 命中」，那个 grep 模式恰好绕过了注释里的那处；实际是「SQL 形状 0、注释 1」。留在这里而不是直接改掉，因为「读数的口径决定结论」正是本单反复出现的那类错误。）
