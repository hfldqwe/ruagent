# 图侧三处非原子写（t67 #1/#2/#6，同根因）：失败不留半成品（t81）

> **性质**：修复单。改了 3 个文件（`distill.rs` · `community.rs` · `lib.rs`）+ 本报告；**没改 `crates/store`**（理由见 §1.3）。
> **日期**：2026-09-29。所有读数取在**最终字节**的构建上（改完 → 跑 → 再读），注入失败一律读**产物**而不是推理。
> **来源**：审计 `docs/design/reviews/gen3-audit-graph.md` §1/#1、§2/#2、§6/#6 三条同根因；本单把它们合成**一种形状**同时修掉。

## 0 一句话

**三处多步写入现在各在一个事务里**：蒸馏的图半边、`build_communities` 的层级替换、`merge_entities` 的八条语句。注入失败后**产物与写入前逐项相同**（审计改前分别是「2 实体 + 1 别名残留」「3 社区 6 成员 + 3 行 NULL 父」「别名已移、行还在」），成功路径**照常写完**（三条负控）。

| # | 现场 | 审计改前读数（t67，注入失败后读产物） | 本单改后读数（注入失败后读产物） | 成功路径负控 |
| --- | --- | --- | --- | --- |
| 1 | `distill.rs:779-862`/`335-355` 蒸馏 | `entities 2 [(1,ruagent),(2,麒麟 V10)]` · `entity_edges 0` · `aliases 1` · `episodes [(1,run_turn_failed)]` | **`entities 0/0 · aliases 0/0 · edges 0/0`（0 残留）** | **`entities 2 aliases 1 edges 2`**（daemon 测试）／图级 `entities 2 aliases 2 relations 1` |
| 2 | `community.rs:207-231` 层级替换 | 3 社区/9 成员 → **3 社区/6 成员 `[(1,3),(2,3),(3,0)]` + 3 行 `parent_id NULL`** | **分区回到替换前**：`2 communities / members 6 / parent_id NULL 0`（我的夹具：2 社区×3 成员 + 一个 level-1 父） | **`rebuild succeeded -> 2 communities, 6 members, parent_id NULL 0`** |
| 3 | `lib.rs:1110-1157` `merge_entities` | 末条失败 ⇒ **absorbed 行仍在、别名已移** | **`entities 2/2 · edges 1->1 · aliases 1/1`（0 变化）** | **`moved 1 edge(s) · entities 1 · edges 1 · aliases 2 · keeper 关联边 1`** |

## 1 一种形状（三处同一根因，一处一处加是错的）

### 1.1 形状

```rust
db.call_flat(move |conn| {              // 一个闭包 = 一次 sole-writer 往返
    let tx = conn.transaction()?;       // 事务只开在一个闭包能拥有连接的地方
    …所有语句都用 &tx/&*tx…              // 既有的 per-item 函数改成吃 &Connection
    tx.commit()?;                       // 失败路径：`?` 直接返回 ⇒ Transaction Drop ⇒ 回滚
    Ok(outcome)
})
```

`Db::call_flat` 的闭包签名是 `FnOnce(&mut rusqlite::Connection) -> Result<T, rusqlite::Error>`（`store/src/sqlite.rs:103-109`），所以 `conn.transaction()` **在任何闭包内都可用** —— 审计说的「根因是 `call*` 在这些写入点没开事务」**成立**，而不是 `call*` 的语义缺能力。

### 1.2 为了让「整份抽取」也能在一个事务里：抽出 `*_in`(conn) 与一个聚合入口

原来 `upsert_entity_with_aliases` / `upsert_fact` / `add_alias` / `queue_pending` / `upsert_entity` / `add_fact_with_source` 各自 `db.call*` 一次，**互相之间不可能共享事务**。改法：

* 每个公开入口的语句抽成**吃 `&Connection` 的同步函数**（`upsert_entity_in` / `insert_fact_in` / `upsert_fact_in` / `resolve_entity_in` / `add_alias_in` / `queue_pending_in` / `create_and_alias_in`）；
* 每个公开入口变成**薄包装**：开事务 → 调 `*_in` → commit（**签名一个字没变**，既有调用者与测试不受影响）；
* 新增一个聚合入口 **`apply_extraction(db, entities, facts, source, source_episode) -> ExtractionWrite`**：**一个闭包、一个事务**里跑完 N 个实体（含别名、含判官/入队）与 M 条关系，并沿用**同一套** `resolve_entity_in`/`upsert_fact_in`（所以 G7 的写侧去重、别名解析、`NeedsJudgement` 不合并等规则**一条都没被绕过**）。
* `distill.rs::write_graph` 从「逐条 `await` 调用 graph API」变成「组两个 Vec → 一次 `apply_extraction`」，日志计数改从 `ExtractionWrite{entities,aliases,relations,duplicates,refused,pending}` 取，**读数语义不变**。

**副作用（顺带被修掉的同族缺陷）**：单条路径本身现在也是原子的 —— 一个实体的**别名集**不再可能写一半（审计 #7 提到的「别名逐条 await」）；`add_fact_with_source` 的「先作废旧事实、再插新事实」两条语句也同事务。

### 1.3 为什么**不是**交回 store finding（本单允许「交回」的那条）

* `conn.transaction()` 在 `call_flat` 的闭包里**可用且已被仓库自己用过**：`crates/store/src/lib.rs:838-845`（`backfill_chunk_grams`：`let tx = conn.transaction()?; … tx.commit()?;`）。
* 本单**没有改 `crates/store` 一个字节**（`git status` 里 store 只有本代先前任务的既有改动）。所以「根因在 `call*` 语义」这一判断**不成立**，不需要交回 finding。
* 唯一值得记的一条**形态建议**（不是 finding）：`call`/`call_flat` 的文档可以加一句「多步写入必须在这个闭包内自开事务，否则中间态会被读成证据」——今晚的教训值这一句；但它属于文档，不影响本单。

## 2 #1 蒸馏失败不留半成品

* **改前（审计，注入 `entity_edges` INSERT 触发器后读产物）**：`entities 2 [(1,'ruagent'),(2,'麒麟 V10')]` · `entity_edges 0` · `episodes [(1,'run_turn_failed')]` · `aliases 1` —— 「徒章」（episode）诚实，图不诚实。
* **改后（本次最终字节）**，`test -p ruagent-daemon --lib -Nocapture` 的两条打印行：
  ```
  READING RVC-3: episodes 1 -> 1 | kind=run_turn 1 -> 0 | episode kind="run_turn_failed"
    | meta=Some("{\"voided_by\":\"write_graph\",\"reason\":\"writing this extraction into the graph (all-or-nothing)\"}")
    | memories keeping their provenance 1
  READING t81 #1: failed graph write -> entities 0/0 aliases 0/0 edges 0/0 (before/after; 0 residue is the invariant)
  READING entities=2 aliases=1 edges=2 (success path, t81 #1 control)
  ```
  断言：`graph_after == graph_before`（三张表逐项相等）。**RVC-3 的契约一字未动**（episode 计数不变、`kind` 变 `run_turn_failed`、meta 带 `voided_by`、memory 保留溯源）。
* **负控（成功路径）**：同一次运行里的 `graph_edges_carry_the_episode_and_the_event_time_source` → `entities 2 aliases 1 edges 2`，且 `write_graph` 返回 `ent=2 rel=2`（说明改成一个事务后**写入条数与从前完全一致**）。
* **判据达成**：注入图写入失败 ⇒ 图三表与写入前**逐项相同（0 残留）** ⇒ ✅。

## 3 #2 `build_communities`：注释里的「atomically」现在是真的

* **改前（审计读数）**：注入失败后 **3 社区/9 成员 → 3 社区/6 成员 `[(1,3),(2,3),(3,0)]` + 3 行 `parent_id NULL`** —— 旧分区已删（前两条 DELETE 已提交）、新分区只写了一半。
* **改后**：`DELETE`(两张表) + 逐社区 INSERT + 逐成员 INSERT + 父挂载，全在一个 `tx` 里；失败时 `?` 返回 ⇒ 回滚。我的回归测试（2 组件×3 节点 + 先建 level-1 父）：
  ```
  READING t81 #2: injected failure -> sqlite error: t81 injected failure
    | partition before 2 communities/[3, 3] | after 2 communities (members 6, parent_id NULL 0)
  READING t81 #2 (control): rebuild succeeded -> 2 communities, 6 members, parent_id NULL 0
  ```
  断言：`after == before`（社区 id、`parent_id`、成员集合三样逐项相等）⇒ **回到替换前** ✅。
* **顺带修的一处编译期陷阱**：读边的 `stmt` 必须先于 `conn.transaction()` 出作用域（否则 `E0502`：`&mut conn` 与语句的不可变借用冲突）—— 我把读边放进块作用域并写了注释，避免下一个人再撞。
* **判据达成**：注入失败后仍是 9 成员量级（我的夹具是 6 成员）、**无 `parent_id NULL`** ⇒ ✅。

## 4 #6 `merge_entities`：八条语句同一事务

* **改前（审计读数）**：末条 `DELETE FROM entities` 注入失败 ⇒ **absorbed 行仍在=1、它的名字已进 `entity_aliases`=1、边已改指** —— 正是旧注释（`lib.rs:1137-1138`：「or the delete below would break the FK … and the merge would half-apply」）说不会发生的事。**FK 顺序只防外键错误，不提供原子性**；这一点已写进新注释。
* **改后**：
  ```
  READING t81 #6: injected failure at the last statement -> sqlite error: t81 injected failure
    | entities 2/2 | edges 1->1 | aliases 1/1 (before/after)
  READING t81 #6 (control): merge succeeded -> moved 1 edge(s) | entities 1 | edges 1 | aliases 2
    | keeper incident edges 1
  ```
  ⇒ **失败后 0 变化**（实体、当前边、别名三样都不动）✅；**成功后全部生效**（行删掉、边跟随、名字进别名表、关联边 1 条）✅。
* **仍开着、本单**不**碰的**：审计 #5（合并相连实体会产生自环、社区成员不转移）与 #7 的「错误被吞成误阴」（`if let Ok(..) = conn.query_row`）—— 它们是**另外的缺陷**（数据语义 / 错误路径），不属本单 triage；本单只把它们**留在原地**并在 §8 登记，免得被读成「一起修了」。

## 5 反做负控：**只有坏侧红、好侧绿才算两侧都测到**

做法（按 t93/t81 纪律：**先广播 → 限时 → 报窗口起止 + 恢复绿**）：把三处事务退化成 autocommit（`let tx: &rusqlite::Connection = &*conn;` + 去掉 `tx.commit()?;`，**提示符：反做态仍可编译**），跑我那三条新测试，然后按内存原文恢复并校验哈希。

* **反做态读数**（`test -p ruagent-graph --lib -- --nocapture`）：
  ```
  READING t81 #1: injected failure -> … | entities 0/2 | edges 0/0 | aliases 0/2   ← 2 实体 + 2 别名残留
  READING t81 #6: injected failure at the last statement -> … | entities 2/2 | edges 1->1 | aliases 1/2  ← 别名已移
  READING t81 #2: injected failure -> … | after 2 communities (members 5, parent_id NULL 2)            ← 半建分区
  test result: FAILED. 3 passed; 3 failed; … finished in 0.24s          exit 101
  ```
  ⇒ **三条断言都能因为「事务被拿掉」变红**，且打出的残留形态与审计改前记录**同族**（2 实体/2 别名、别名已移、半建分区 + NULL 父）。
* **恢复证据（逐字还原）**：反做前内存快照哈希与恢复后**完全一致** ——
  `lib.rs 56B92EE4055078DB`、`community.rs C608FE071325FA61`、`distill.rs 36269AFECB031895`（SHA-256 前 16 位）；脚本 `try/finally`，异常也会恢复；`RESTORED hashes … identical: True`。
* **窗口**：起 ~12 秒，**只影响我 3 条新测试**（不影响任何人的编译面）；恢复后立刻复跑并广播「回到绿」——`--lib` **ok-lines=1（6 passed）/ FAILED-lines=0 / panicked=0 / exit 0**、整条 `test -p ruagent-graph` **ok-lines=12 / FAILED-lines=0 / panicked=0 / exit 0**。
* **被负控覆盖的三个新测试**（名字与位置）：`crates/graph/src/lib.rs` 的 `tests::a_failed_extraction_write_leaves_no_partial_graph`、`tests::a_failed_merge_rolls_back_the_whole_group`；`crates/graph/src/community.rs` 的 `community::tests::a_failed_rebuild_leaves_the_previous_partition_intact`。（放在 `src/` 内的 `#[cfg(test)]`：本单 inScope 只列了这三个**源文件** + 报告，`crates/graph/tests/**` 不在其中。）

## 6 门禁：三个面分开写（第 6 条纪律）

| 面 | 命令 | 读数 |
| --- | --- | --- |
| **编译面（全树，含测试目标）** | `check --workspace --all-targets` | **exit 0**，`Finished dev profile … in 36.92s`，无 error/无 `-->` |
| **测试面（graph）** | `test -p ruagent-graph` | **ok-lines=12 · FAILED-lines=0 · panicked=0 · `exit=0`**（12 个 target 全绿；三条新测试逐个 `ok`） |
| **测试面（daemon lib）** | `test -p ruagent-daemon --lib` | **ok-lines=1（`80 passed; 0 failed`）· FAILED-lines=0 · panicked=0 · `exit=0`** |
| **crate 级门禁 · graph** | `clippy -p ruagent-graph --all-targets -DenyWarnings -CleanFirst` | **exit 0** · `-CleanFirst` 卸 **6518 files / 1.1 GiB**（证明真重读）· **14 个 unit 行**：`ruagent_core`、`ruagent_store`、`ruagent_graph`×2、`seed_resolution`、`resolution`、`extraction_gold`、`fixture_ownership`、`fixture`、`empty_recall_pattern`、`live_after`、`entity_query_shapes`、`multihop_gold`、`temporal` · **error-lines=0** · `Finished in 4.16s` |
| **crate 级门禁 · daemon** | `clippy -p ruagent-daemon --all-targets -DenyWarnings -CleanFirst` | **exit 0** · 卸 **862 files / 2.2 GiB** · **7 个 unit 行**（名字未采集，见下）· **error-lines=0** · `Finished in 9.33s` |

**两件证据的口径说明**：①「覆盖哪些 target」——graph 侧逐 unit 列出了 14 个（含 10 个集成测试目标）；daemon 侧只拿到 **7 个 unit 行**、**名字没采集**（我为了省一次构建锁等待没有再跑一遍 `--verbose`），这是**已知缺口**、如实写在这里而不是含糊过去。②「本次确实重查」——两条 clippy 都带 `-CleanFirst`，`Removed … files/GiB` 两行就是重读的证据；测试面另用 12 条 `test result` 行与三条新测试名作证。

**格式面（t66）**：`rustfmt --edition 2024 --check` 对**我改的三个文件**的改动行数 = **0/0/0**（`lib.rs`、`community.rs`、`distill.rs` 本来就 fmt-clean）⇒ 本单不会给下一次推送添 fmt 站点；**没有**对未触碰的文件跑 fmt。

## 7 过程发现（自我记账，不静默）

1. **我两次让共享树停在不可编译状态而没有广播**（第 23 条纪律的实例，captain 的 C15）：
   * 第一次 `lib.rs`：`E0277 EventTimeSource: Default`（我新加的 `ExtractFact` 派生了 `Default`）+ `E0308`（helper 返回元组、调用处按单值解构）—— 修完自己复核到 exit 0，但**期间 integ 花了 479.9s 的私有构建才发现产物不存在**；
   * 第二次 `lib.rs`：`E0061`（我的新测试 `add_fact` 少传两个参数，7 vs 5）—— 同样是修复前被别人的 workspace 读数撞到。
   * 我此后承诺并执行：**跨文件前先 `check --workspace --all-targets`**；本单后半段每次改动都先跑它（最后一次 exit 0、`Finished in 36.92s`）。
2. **一条仪器坑（写给自己也给下一个人）**：我第一版门禁行数用 `Select-String -Pattern 'FAILED'` 统计，PowerShell 默认**大小写不敏感**，于是把 12 行 `test result: ok. … 0 failed` 全算成了 `FAILED`（报出 `FAILED=15` 的假红）。凡「红/绿行数」必须用 `-CaseSensitive` 且锚定 `test result: FAILED`。
3. **反做负控的纪律升级**：先广播（哪些文件、哪些测试、预计时长）→ 限时（~12s）→ 报窗口起止 + 恢复绿 + 哈希一致。本次三样都做了。

## 8 本单**没有**做的（避免被读成「一起修了」）

* 审计 **#5**（合并相连实体会产生自环；absorbed 的社区成员不转移 ⇒ `covered != non_isolated` 直到下次重建）——**未动**。
* 审计 **#7**（`if let Ok(..) = conn.query_row` 把任何 DB 错误当 no-match；`void_episode` 把补偿失败吞成 `false`）——**未动**；其中「别名逐条写」的形式问题被 §1.2 的事务**顺带**解决，但**判据语义**没改。
* 审计 **#4 / #8 / #9 / #10 / #11 / #12**（全表读、N+1、循环内 `prepare`、全表扫、O(N²)、死 API）—— 均**未动**，属性能/清理单。
* `merge_entities` 的 `with_context(|| format!("resolving entity {name}"))` 级别错误上下文：改成一个聚合调用后，错误信息是底层 SQL 错误 + 一句 `writing this extraction into the graph (all-or-nothing)`；**逐项命名**的上下文不再有（可读性略降，换来原子性）。如实记在这里。

## 9 复现命令

```powershell
# 三条新测试（注入失败读产物 + 成功路径负控）
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-graph --lib -- --nocapture
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-daemon --lib -Nocapture

# 门禁三面
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 check --workspace --all-targets
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-graph
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 clippy -p ruagent-graph  --all-targets -DenyWarnings -CleanFirst
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 clippy -p ruagent-daemon --all-targets -DenyWarnings -CleanFirst
```
