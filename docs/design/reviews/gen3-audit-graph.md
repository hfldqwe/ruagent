# gen3 只读审计：图谱 + 蒸馏路径的待完善点（t67）

> **性质**：只读审计。未改 `crates/graph/**`、`crates/daemon/src/distill.rs` 或任何其他代码；唯一写入 = 本文件。
> **审计对象**：`crates/graph/src/{lib.rs,retrieve.rs,community.rs}`（1240 + 1064 + 357 行）+ `crates/daemon/src/distill.rs`（1763 行，产品代码 = 1–946）。
> **日期**：2026-09-28。工作树 = t43/t49 之后的当前树。
> **证据口径**：全部读数来自**对活库副本的只读探测**（`VACUUM INTO` 到 `%TEMP%`，源以 `mode=ro` 打开）与**已有测试**；**没有**调用真守护进程（pid 79984）、**没有**写活库、**没有**改任何测试或产品代码。活库读数挂在 2026-09-28（`entities 63 / entity_edges 67 / current 60 / episodes 17 / schema v18`）。
> **探测产物**（保留供复核）：`%TEMP%\t67-live.db`（活库副本）、`%TEMP%\t67-v23.db`（副本 + 0019–0024 迁移 ⇒ 真实 schema）、`%TEMP%\t67-amp-{1,10,100}.db`、`%TEMP%\t67-f-comm.db`、`%TEMP%\t67-g-merge.db`。每条 finding 的复现命令都从这些产物出发。

## 0 摘要（按严重度 × 影响面，12 条）

| # | 严重度 | 一句话 | 位置 | 类别 |
| --- | --- | --- | --- | --- |
| 1 | **高** | 蒸馏失败后**图里留下半成品**：实体与别名已提交、边为 0、episode 标记 failed ⇒ 「失败不留痕」不成立 | `distill.rs:779–862`、`335–355` | 原子性 |
| 2 | **高** | `build_communities` 的「原子替换」**不是原子的**（注释与实现相反）：注入失败后旧分区没了、新分区只写了一半 | `community.rs:207–231` | 原子性 + 注释不实 |
| 3 | 中高 | `facts_as_of` 用**字符串**比较时刻（RVC-1 已在检索侧修过，此处仍在）：**同一时刻三种写法 = 三个答案**（38/30/43 ÷ 67 条边） | `lib.rs:230–248` | 正确性 |
| 4 | 中高 | 每次 `retrieve()` **读出整张边表**（无 WHERE、无 LIMIT、带 LEFT JOIN）+ 三张全量 HashMap ⇒ 成本与查询无关、只与库大小有关 | `retrieve.rs:786–792`、`815–845` | 性能（扩展性） |
| 5 | 中 | `merge_entities` 把两个**相连**实体合并会造出**自环** `src=dst`（活库现在 0 个自环） | `lib.rs:1123–1130` | 正确性 |
| 6 | 中 | `merge_entities` 8 条语句**无事务**；注释称「不会半应用」——被证伪 | `lib.rs:1110–1157` | 原子性 + 注释不实 |
| 7 | 中 | 错误被吞成「没找到 / 没做成」：`upsert_entity_with_aliases` 把**任何**查询错误当 no-match ⇒ 静默造重复实体；`void_episode` 把补偿失败吞成 `false` | `lib.rs:946–960`、`distill.rs:432–455` | 错误路径 |
| 8 | 中 | `build_evidence` **N+1**：每个节点 id 一次 `query_row` | `retrieve.rs:1049–1062` | 性能 |
| 9 | 中 | `resolve_seeds` 在 per-probe / per-pattern **循环里 `prepare`**，全 crate **无语句缓存**（`prepare_cached` 0 处） | `retrieve.rs:496–506`、`559–568` | 性能 |
| 10 | 中 | `resolve_seeds` 的两条腿是**全表扫**（`instr()` / `name LIKE`），每条 recall 都扫 | `retrieve.rs:528–532`、`560–563` | 性能（扩展性） |
| 11 | 低中 | `redundant_pairs` 对**全部实体做 O(N²)** 比较，且挂在 HTTP 面板路径上 | `lib.rs:1082–1101`、`api.rs:1442` | 性能 |
| 12 | 低 | 公开但无人调用的 API：`base_name`（**0 个调用者**）、`communities_of`/`fact_count`（仅测试） | `lib.rs:749`、`community.rs:352`、`retrieve.rs:301` | 声明 vs 实际 |

**四条被证伪 / 已核对无问题的假设**见 §14（含「OR 条件没走索引」「hops 无界」「产物路径有 unwrap」「log_outcome 违反 0024」四条 —— 前两条是我先怀疑、后被读数推翻的）。**未能覆盖的范围**见 §15。**未验证猜想**见 §16。

---

## 1 蒸馏失败后图里留下半成品（高）

* **位置**：`crates/daemon/src/distill.rs:779–862`（`write_graph` 逐实体 `upsert_entity_with_aliases` → 逐关系 `upsert_fact`，**没有任何事务**）、`335–355`（`write_extraction` 的补偿只 `void_episode`）。
* **复现**：
  ```powershell
  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-daemon --lib distill -- a_graph_write_failure -Nocapture
  # 然后读它留下的真实产物（只读）：
  python -c "import glob,os,sqlite3;d=sorted(glob.glob(os.path.join(os.environ['TEMP'],'ruagent-t329-t27-void-*')),key=os.path.getmtime)[-1];c=sqlite3.connect('file:'+os.path.join(d,'ruagent.db').replace(chr(92),'/')+'?mode=ro',uri=True);print(c.execute('SELECT id,name FROM entities').fetchall(), c.execute('SELECT COUNT(*) FROM entity_edges').fetchone(), c.execute('SELECT id,kind FROM episodes').fetchall())"
  ```
* **证据读数**（2026-09-28，我这次运行留下的产物 `%TEMP%\ruagent-t329-t27-void-29008-1\ruagent.db`）：
  ```
  entities: 2 -> [(1,'ruagent'), (2,'麒麟 V10')]      ← 失败的那次抽取创建的，已提交
  entity_edges: 0                                     ← 边一条都没写进去（触发器拦截）
  episodes: [(1,'run_turn_failed')]                   ← RVC-3 的补偿按设计工作
  episode meta: {"voided_by":"write_graph","reason":"adding relation runs_on"}
  entity_aliases: 1 [(2,'银河麒麟','extraction')]      ← 别名也留下了
  memories: 1 | with source_episode: 1
  ```
* **为什么重要**：`void_episode` 让**徽章**变诚实了（这是 RVC-3 的成果），但**图本身不诚实**：一次被记录为 `failed` 的蒸馏在 `entities` 里留下 2 行、在 `entity_aliases` 里留下 1 行，而 `entity_edges` 是 0 ⇒ 库里存在「没有任何事实、却有一条别名、且溯源指向一个 failed episode」的实体。G9/D4 的契约是「每次尝试都有一行日志」；**图侧缺的对称契约是「失败的尝试不改变图」**。重跑能救（`upsert_entity_with_aliases` 会按 `norm_name` 命中），所以不是无限膨胀，但「失败即无痕」这条不变量现在是假的。
* **可证伪的修复判据**：把 `write_graph` 的实体+别名+关系写进**一个事务**（store 已具备该能力：`crates/store/src/lib.rs:838–845` 在 `backfill_chunk_grams` 里就是 `let tx = conn.transaction()?; … tx.commit()?;`，**不需要新 API**）。修好后同一条命令留下的产物必须显示：**`entities` 数量与尝试前一致（失败尝试新增 0 行）、`entity_aliases` 新增 0 行、`entity_edges` 仍为 0、episode 仍是 `run_turn_failed`**；并且 `crates/daemon/src/distill.rs` 里 `write_graph` 的事务覆盖率可被断言（例如注入失败后逐表对比）。
* **建议 owner**：graph（I-C）——`distill.rs` 在我名下；若修法要动 store 的事务封装，另请 store 侧确认（当前不需要）。

## 2 `build_communities` 的「原子替换」不是原子的（高）

* **位置**：`crates/graph/src/community.rs:207–231`（注释在 207–208：「Replace this level atomically: a half-rebuilt partition would be read as evidence.」）。
* **复现**：把活库副本迁到真实 schema，跑 `community.rs:209–231` 的**同一串语句**并注入一次失败；`isolation_level=None`（= rusqlite 的 autocommit 语义，store 的 `call/call_flat` **从不** `BEGIN`）。
  ```python
  # 见 §17 脚本 F：DELETE community_entities / DELETE communities / 逐社区 INSERT + 逐成员 INSERT
  # 注入：CREATE TRIGGER t67_boom BEFORE INSERT ON community_entities WHEN NEW.entity_id=30 BEGIN SELECT RAISE(ABORT,'t67 injected'); END;
  ```
* **证据读数**（`%TEMP%\t67-f-comm.db`，真实 DDL 来自 0021）：
  ```
  before: 3 communities / 9 members  (每社区 3 成员)
  注入失败: "t67 injected"
  after : 3 communities / 6 members  per-community members=[(1,3),(2,3),(3,0)]
          parent_id IS NULL 的 level-0 行: 3
  ```
  ⇒ 旧分区被删（前两条 DELETE 已提交），新分区只写了一半（第 3 个社区 **0 个成员**、层级挂载还没发生）。**注释描述的状态就是实际发生的状态。**
* **为什么重要**：`communities`/`community_entities` 是 G8 的**证据载体**（`covered == non_isolated`、分区数、成员互斥、层级挂载都是断言面）。一个「半重建但已提交」的分区会**通过一部分断言**（分区数 3 、成员不重叠），只在与 `entities`/`non_isolated` 对账时暴露 —— 也就是它可能被读成合法证据。而 `covered == non_isolated` 是 **G8 结构侧断言**，一旦库里有这种状态，断言会红在一个与根因无关的地方（同族于 t36 的「另一条测试的断言以不相干的理由变红」）。
* **可证伪的修复判据**：同一个注入实验里，调用返回 `Err` **且** `communities`/`community_entities` 两张表在 level=0 上**逐行等于调用前**（`SELECT * FROM communities WHERE level=0 ORDER BY id` 与成员集合都相同）；正常路径读数不变（`--test extraction-gold` 的 `communities=9 covered=43 non_isolated=43`、`sizes [14,5,3,3,2,6,2,4,4]`）。
* **建议 owner**：graph（I-C）。修法同样只需 `conn.transaction()`（同 §1 的先例）。

## 3 `facts_as_of` 用字符串比较时刻：同一时刻三种写法 = 三个答案（中高）

* **位置**：`crates/graph/src/lib.rs:230–248`，SQL 在 233–240：
  ```sql
  ... WHERE (src = ?1 OR dst = ?1)
        AND valid_at <= ?2
        AND (invalid_at IS NULL OR invalid_at > ?2)
  ```
  `at` 是 `&str`，直接进绑定参数 ⇒ **文本序**。对照：检索侧 `retrieve.rs:736–750` 已经在入口 `parse_ts`，并把非时刻**报错**（RVC-1 的修法）。
* **复现**：见 §17 脚本 C（对 `%TEMP%\t67-live.db` 只读）：取 **同一个时刻** 的三种写法，跑**上面那段 SQL 原文**，与「按解析后的时刻比较」对照。
  ```powershell
  # 三种写法（同一瞬间 2026-09-13T18:38:15Z）：
  #   "2026-09-13T18:38:15Z"  |  "2026-09-13T18:38:15+00:00"  |  "2026-09-14T02:38:15+08:00"
  ```
* **证据读数**（活库副本，67 条边；正确解析 = 30 条「当时为真」）：
  ```
  字符串比较   "…15Z"                  -> 38 条 | 与解析正确值差 16 条
  字符串比较   "…15+00:00"（无小数）    -> 30 条 | 差 0 条   ← 巧合：活库 nearly 全是 +00:00 且带小数
  字符串比较   "…15+08:00"（同一时刻）  -> 43 条 | 差 21 条
  三种写法两两差异：Z vs +00:00 = 16 条；Z vs +08:00 = 5 条；+00:00 vs +08:00 = 21 条
  按 API 的调用形状（src=?1 OR dst=?1）逐实体：6 / 63 个实体在两种写法下返回**不同的条数**（例：实体 52 → 1 条 vs 5 条）
  ```
  活库 `valid_at` 形态：`+hh:mm` 66 条、**1 条是纯日期 `2026-09-13`**（无非零偏移混用，所以今天「恰好」多数场合对得上）。
* **为什么重要**：① 这是 RVC-1 判过的**同一类缺陷**，只在检索侧修了，`facts_as_of` 是**公开 API 且是产品路径**（`api.rs:1288–1292`）；② HTTP 侧今天**碰巧被掩盖**：`api.rs:1290` 先 `parse_ts` 再 `to_rfc3339()`（t57 修的是「非时刻返回 400」），所以绝大多数请求都被规范化成 `+00:00`；③ 但 (a) 同一时刻的 `Z` 写法在活库上就错 **16/67**，(b) 活库已经存在非规范值（`'2026-09-13'`），(c) 任何**其他调用者**（测试、MCP、将来的面板）直接调这个公开函数就会拿到错误答案 —— 判据属于函数，不属于某个 handler。
* **可证伪的修复判据**：`facts_as_of` 内部 `parse_ts(at)`，非时刻返回 `Err`（与 `retrieve` 同形）；修好后**同一个时刻的三种写法必须返回逐行相同的边集**（上表的 16 / 21 / 5 → **0**），且 `--test temporal` 的 `as_of … -> MATCH` 6/6 与 `orphan edges []` 不变。
* **建议 owner**：graph（I-C）。

## 4 每次 `retrieve()` 读出整张边表（中高，扩展性）

* **位置**：`crates/graph/src/retrieve.rs:786–792`（SQL）+ `815–845`（三张全量 HashMap）。
  ```sql
  SELECT e.id, e.src, …, e.event_time_source
  FROM entity_edges e LEFT JOIN episodes ep ON ep.id = e.source_episode
  ORDER BY e.id                      -- 没有 WHERE、没有 LIMIT
  ```
* **复现**：`EXPLAIN QUERY PLAN` + 放大探测（见 §17 脚本 D/E，副本 `%TEMP%\t67-v23.db`）。
* **证据读数**：
  ```
  EXPLAIN: "SCAN e ; SEARCH ep USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN"
  放大（只增 entity_edges 行数，查询不变）:
    x1    edges=67    rows_back=67    sql=0.0002s
    x10   edges=670   rows_back=670   sql=0.0025s
    x100  edges=6700  rows_back=6700  sql=0.0220s
  ```
  之后 Rust 侧对**全部**返回行建 `rel_count`/`degree`/`incident` 三张表，并在 walk 的内层对每个候选路径做 3 次 `Vec` clone（`893–898`）。活库今天只有 67 条边，所以**今天不是延迟问题**（这就是为什么严重度写「中高」而不是「高」）。
* **为什么重要**：函数文档（`721–725`）写「Bounded by construction: hops ≤ MAX_HOPS, at most beam paths per hop, at most max_paths paths…」—— **输出**确实是界的，**输入**不是：成本 = O(全库边数)，与查询、与预算都无关。图谱一旦长到几万条边，每次召回（含 `--test` 的每次 gold 查询）都要为整库付一遍。库里已经有 `neighbors()` 的**递归 CTE 先例**（`lib.rs:256–270`），说明「按种子局部取边」在本仓是已实现的能力。
* **可证伪的修复判据**：给 `retrieve` 加一个「读了多少边」的计数（`stats.graph_edges` 已经是 `total_current`，但它记的是全库——这正是可用的判据钩子）。修好后：**同一个小查询在 x100 的库上读到的行数不得随之增长**（例如「与 seed 邻域相关」的边数），且 `--test multihop-gold` 的三行读数（`0.7500/0.8000`、中位 `9`、`{None 1, Some(Hops) 6, Some(MaxPaths) 13}`）逐位不变。
* **建议 owner**：graph（I-C）。

## 5 `merge_entities` 合并相连实体 → 自环（中）

* **位置**：`crates/graph/src/lib.rs:1123–1130`（两条 UPDATE 把 `src`/`dst` 都改到 keeper，**没有**处理 `src = dst`）。
* **复现**：活库副本上跑**真实的 8 条语句序列**（§17 脚本 G），选一条真实边 `edge 1: src=1 dst=2`（`keeper=1, absorbed=2`）。
* **证据读数**：
  ```
  活库当前自环数: 0           （合并会把它从 0 变成 >0）
  活库中「互相指向」的实体对: 4 条边参与 reciprocal pairs
  跑完那串语句后自环数: 1     （合并把 edge 1 变成 1 -> 1）
  ```
* **为什么重要**：① 自环是**语义污染**：`current_facts(1)` 会返回一条「1 与 1 有关系」的事实，社区/度数统计把它算进去；② 更关键的一致性后果：`merge_entities` 删掉 absorbed 的 `community_entities`（`1143–1146`）但**不把它转移给 keeper**，于是 `covered` 比 `non_isolated` 少 1 ⇒ **G8 的 `covered == non_isolated` 结构断言在下一次 `build_communities` 之前一直是假的**；③ 重跑合并不是幂等的补丁（第二次调用 `keeper==absorbed` 才早退，但已被删的行不会回来）。
* **可证伪的修复判据**：合并后断言 `SELECT COUNT(*) FROM entity_edges WHERE src = dst` **仍为 0**（要么删除这类边、要么把它们按「两面变一面」显式折叠并记录），且 `community_coverage` 的 `(covered, non_isolated)` 在合并后**仍然相等**（把 absorbed 的成员关系交给 keeper 或触发一次重建）；`redundant_pairs` 里该对消失。`--test extraction-gold` 的 G6/G8 读数不变。
* **建议 owner**：graph（I-C）。

## 6 `merge_entities` 8 条语句无事务，注释称「不会半应用」（中）

* **位置**：`crates/graph/src/lib.rs:1110–1157`；被证伪的注释在 `1137–1138`：「Aliases follow their entity, or the delete below would break the FK (foreign_keys=ON) and the merge would half-apply.」
* **复现**：同一串语句，在**最后一条**（`DELETE FROM entities`）注入失败（§17 脚本 G）。
* **证据读数**（`%TEMP%\t67-g-merge.db`）：
  ```
  注入: "t67 injected at the last statement"
  after: absorbed 行仍在=1 | 它的名字已进 entity_aliases=1 | keeper 的关联边 40 -> 40（边被改指，语义已变）
  ```
  ⇒ 边与别名已经按「合并完成」的样子改完，**实体行还在** —— 正是注释说不会发生的那件事。FK 的**顺序**只保证不触发外键错误，**不提供原子性**。
* **为什么重要**：合并是**人工/agent 决策路径**（`api.rs:1497` 是产品端点）。中途失败后库处于「两边都在、别名已指向 keeper、边已重指」的状态：后续解析可能把 absorbed 名字解析成 keeper，同时 absorbed 行还在 ⇒ 同一对象两个 id 并存（就是 G6 要消灭的冗余），且**没有任何日志记录这次半合并**。
* **可证伪的修复判据**：注入失败后，`entities / entity_edges / entity_aliases / community_entities / resolution_pending` 五张表**逐行等于调用前**，调用返回 `Err`；正常路径读数不变（`--test live-after` 的 `merged 5 pairs -> redundant 0 | entities 58 | edges 67 | aliases 5`）。
* **建议 owner**：graph（I-C）。

## 7 错误被吞成「没找到 / 没做成」（中）

* **位置 A**：`crates/graph/src/lib.rs:946–960` —— `if let Ok(id) = conn.query_row("SELECT id FROM entities WHERE norm_name = ?1", …) { return Ok(Some(id)); }`（别名查询 953–959 同形）。
  **`query_row` 的 `Err` 有两种：`QueryReturnedNoRows`（期望）与真正的 DB 错误（I/O、关闭、schema 漂移）**；这里把两者**都**当作「没匹配」。
* **位置 B**：`crates/daemon/src/distill.rs:432–455` —— `void_episode` 把补偿 UPDATE 的失败吞成 `false`：`.map(…).unwrap_or(false)`，函数签名是 `bool`，调用者只能把 `false` 写进 `tracing::warn!`，**没有任何一层能对失败做出反应**。
* **复现**：静态站点核查（命令见 §17 脚本 H）—— grep 出这两处吞错误的形状与其所有同类站点；再用 `%TEMP%\t67-live.db` 的**镜像**演示语义（在副本上 `DROP TABLE entity_aliases` 模拟 schema 错误，同样的两条 SQL 在「吞」与「不吞」两种写法下的结论）。
* **证据读数**：`lib.rs` 里 `if let Ok(...) = conn.query_row(...)` 形态 **2 处**（946、953）；`void_episode` **1 处** `unwrap_or(false)`；镜像演示：同一 schema 错误下「吞」写法得出「无匹配 ⇒ 新建实体」，而正确写法是 `Err` ⇒ 一次瞬时 DB 错误就会**静默制造一个重复实体**（G6 的目标正是消灭它），且事后无法从日志区分「真的没有」与「查询失败」。
* **为什么重要**：这是**没有 unwrap、但比 unwrap 更坏**的一类：`unwrap` 至少会响；这里把「我不知道」当成「没有」写进了图。第 3 条「隐藏的错误路径」正是本代反复抓到的一族（「未测/失败不是通过」的图侧版本）。
* **可证伪的修复判据**：只把 `QueryReturnedNoRows` 当 no-match，其余 `Err` 一律上抛；`void_episode` 返回 `Result<bool>` 或至少在 `false` 时把 DB 错误带进返回值。判据读数：**注入一次 DB 错误后，`upsert_entity_with_aliases` 必须返回 `Err`（而不是 `Created`）**，且 `entities` 行数不增加；RVC-3 那条测试在注入补偿失败时必须能看到 `Err` 而不是 `false`。
* **建议 owner**：graph（I-C）——A/B 都在我的两个文件里。

## 8 `build_evidence` 是 N+1：每个节点一次查询（中）

* **位置**：`crates/graph/src/retrieve.rs:1049–1062` —— 语句只 prepare 一次（好），但循环里**每个 id 一次 `query_row`**：
  ```rust
  let mut stmt = conn.prepare("SELECT id, name, kind, summary FROM entities WHERE id = ?1")?;
  for id in &ids { let e = stmt.query_row([id], …)?; names.insert(*id, e); }
  ```
* **复现**：`--test live-after -- --include-ignored --nocapture` 拿到该调用的规模（`REAL retrieve(ruagent): seeds=12 paths=12 facts=11 …`），再对照 `1040–1044` 的 `ids` 构成（每条路径的全部节点去重）。
* **证据读数**：活库那一次召回 `paths=12`，每条路径 2–3 个节点 ⇒ **约 24–36 次语句执行**，而它们要的东西用一条 `WHERE id IN (…)`（或与边一起 join）就能拿到。活库 63 个实体，所以代价现在很小；这是**形状**问题（第 2 条同族：调用成本随返回规模走 N 次往返）。
* **为什么重要**：N+1 与 §4 叠加：先全表读边，再对每个节点往返一次。两者都在**每次召回**上，而召回是本代要推到 SOTA 的那条路。
* **可证伪的修复判据**：实现成 1 条语句；判据读数 = 「语句执行次数」计数（可临时插桩或用 `sqlite3_stmt_status`）从 ~N 降到 1，且 evidence JSON 逐字节不变（`--test multihop-gold` 的 6 次运行仍 `byte-identical: true`、长度 `13696`）。
* **建议 owner**：graph（I-C）。

## 9 `resolve_seeds` 在循环里 `prepare`，全 crate 无语句缓存（中）

* **位置**：`crates/graph/src/retrieve.rs:496–506`（**每个 alias probe** 一次 `conn.prepare`）、`559–568`（**每个 LIKE pattern** 一次）。`prepare_cached` 在 `crates/graph`、`crates/store`、`distill.rs` 里的出现次数 = **0**。
* **复现**：`Select-String -Path crates\graph\src\*.rs -Pattern "prepare_cached"`（0 命中）+ 读上面两段循环；probe / pattern 数量来自 store 的 `terms()` / `han_bigrams()` / `like_patterns()`，即**随查询文本变化**。
* **证据读数**：一条召回的语句编译次数 = `1（exact）+ |probes|（alias）+ 1 + 1 + |patterns|（LIKE）+ 1（summary FTS）+ 1（全库边读）+ N（build_evidence）` ——前两项与后一项都随输入变化。活库一次 CJK 查询的 `name_token` 腿命中 8 个种子、`summary_fts` 46 个（`live-after` 读数），说明这些腿确实在跑。
* **为什么重要**：SQLite 的 `prepare` 会编译 SQL（含 FTS/LIKE 的语句构造），而调用发生在**每次召回**；`prepare_cached` 是 rusqlite 为这个场景提供的现成机制，本仓一处没用。属于「每次调用重复编译的 SQL」这一类的**原样命中**。
* **可证伪的修复判据**：循环内改用 `prepare_cached`（或将语句提到循环外、或把多条 probe 合成一条 `IN`），并给出可测读数：一次召回的语句准备次数变为**常数**（不随 probe/pattern 数量变化）；召回读数（`seeds by leg`、`paths`、`facts`）逐位不变。
* **建议 owner**：graph（I-C）。

## 10 `resolve_seeds` 的两条腿是全表扫（中，扩展性）

* **位置**：`crates/graph/src/retrieve.rs:528–532`（`WHERE instr(lower(?1), norm_name) > 0 ORDER BY length(norm_name) DESC, id LIMIT ?2`）、`560–563`（`WHERE name LIKE ?1 ESCAPE '\' ORDER BY id LIMIT ?2`）。
* **复现**：`EXPLAIN QUERY PLAN`（§17 脚本 D，`%TEMP%\t67-v23.db`）。
* **证据读数**：
  ```
  leg 3b instr() : "SCAN entities ; USE TEMP B-TREE FOR ORDER BY"
  leg 3c name LIKE: "SCAN entities"
  ```
  对照：`facts_as_of` 与 `list_entities` 的 OR 条件**都走了索引**（`MULTI-INDEX OR` + `idx_edges_src`/`idx_edges_dst`），说明「这里扫表」不是我在猜——同一个库上别的语句有计划可用。
* **为什么重要**：`instr` 与 `LIKE '%…%'` **原理上**无法用普通 B-tree 索引，所以这是**设计代价**而不是笔误；但代价挂在**每次 recall 的种子解析**上，且随实体数线性增长（活库 63 个实体，扫描成本不可见；10⁵ 实体时它是主项）。第 2 条（性能）里「无界遍历/全表扫」的原样命中。
* **可证伪的修复判据**：给出「扫描行数不随表增长」的替代路径（例如把 `norm_name` 的 n-gram 也进 FTS、或用 `entities_fts` + 精确复核；store 侧已有 `chunks_fts_cjk` 的先例），判据读数 = 同样的查询在 x100 实体库上的**扫描行数**不增长，而 `seeds by leg` 三条腿的活库读数（`exact_name 1 / name_token 8 / summary_fts 46`）不变。
* **建议 owner**：graph（I-C）+ store（若需要新索引或新虚拟表）。

## 11 `redundant_pairs` 对全部实体 O(N²)，且在产品端点上（低中）

* **位置**：`crates/graph/src/lib.rs:1082–1101`（先 `SELECT id, name FROM entities`，再对每一对调用 `judge_against`）；调用者 `crates/daemon/src/api.rs:1442`。
* **复现**：`--test live-after -- --include-ignored --nocapture`（该仪器直接调 `redundant_pairs` 并打印 `REAL: the judge finds 7 redundant pairs`），配合活库 `entities 63`。
* **证据读数**：63 个实体 ⇒ **1,953 次 `judge_against`**（`63×62/2`）每次调用；返回 7 对。增长是**平方**：10k 实体 ⇒ ~5×10⁷ 次判据调用。`entities` 行数每增加 10 倍，比较次数增加 100 倍。
* **为什么重要**：它是**面板端点**（不是离线批处理），也就是说代价挂在交互路径上；而它的用途（G6 的质量读数）本可以走「别名/首 token 索引 + 有界候选集」——仓里已有这种形状的先例（`memembed::judge_merge` 的 top_k=3 有界候选）。活库规模下无所谓，所以是低中。
* **可证伪的修复判据**：候选集有界（例如按 `norm_name` 的阻塞键分组，只比较同桶），判据读数 = **比较次数随实体数亚平方增长**（给一个计数），而读数不变（活库仍 7 对；`live-after` 的冗余仪器仍 `merged 5 pairs -> redundant 0`）。
* **建议 owner**：graph（I-C）。

## 12 公开但无人调用 / 只在测试里被调用（低）

* **位置与证据**（普查命令：§17 脚本 I，扫 `crates/**` 全部 `.rs`，排除注释）：
  | 函数 | 位置 | 非测试调用点 |
  | --- | --- | --- |
  | `base_name` | `lib.rs:749` | **0 个**（连测试都没有）⇒ 死代码 |
  | `communities_of` | `community.rs:352` | 仅 `extraction-gold.rs:74,75`（测试）+ `lib.rs:20`（re-export 名单） |
  | `fact_count` | `retrieve.rs:301` | 仅 `live-after.rs`/`multihop-gold.rs`（测试） |
  | `add_fact_with_source` | `lib.rs:519` | 仅 crate 内 `upsert_fact`（`lib.rs:732`）⇒ `pub` 但只服务内部 |
* **为什么重要**：本代已抓到一族「声称的能力 vs 实际」。这里三条**不是**谎报（它们确实存在、也确实被测试用），但 `base_name` 是完全死的、另两条是「对外公开、实际只有测试/内部在用」——**公开面比真实消费面大**，而下一代要收敛成单一消费面（HTTP/MCP），这个差额就是噪声来源。把它列出来是为了让「删掉/收窄可见性」成为一个有读数可依据的决定，而不是口味。
* **可证伪的修复判据**：判决后重跑普查：被保留者必须出现**至少一个非测试生产调用点**（或降为 `pub(crate)`），被删除者从普查里消失；`cargo clippy --all-targets -D warnings` 与 `test -p ruagent-graph` 全绿（特别是 `dead_code` 类 lint 不再需要手动忽略）。
* **建议 owner**：graph（I-C）。

---

## 14 已核对无问题 / 被我自己的读数推翻的假设（写在前面，避免下一个人重复怀疑）

1. **产品路径上没有 `unwrap/expect/panic`**：`crates/graph/src/{lib.rs,retrieve.rs,community.rs}` 与 `distill.rs` 的全部 `unwrap/panic` 都在 `#[cfg(test)]` 之后（`lib.rs` 测试模块从 1172 行开始；`distill.rs` 产品代码止于 946，947/1010 起是两个测试模块）。`community.rs`、`retrieve.rs` **零** unwrap。命令：§17 脚本 A。
2. **`log_outcome` 没有违反 0024**：`0024_distill_attempts.sql` 明文警告「`crates/daemon/src/distill.rs:327-343` 的 `ON CONFLICT(session_key)` 必须在同一集成窗口改成普通 INSERT」，而今天是 `distill.rs:472–489` 的**普通 INSERT** ⇒ 迁移那条警告已被兑现。命令与读数：`Select-String -Path crates\daemon\src\distill.rs -Pattern "ON CONFLICT"` → **1 处命中，且只在文档注释里**（`distill.rs:312`，是**引述那个被删掉的子句**的说明文字），**SQL 语句里 0 处**。（第一稿我在这里写了「无命中」，复核时改掉：注释也算命中，判据是「语句里 0 处」。）
3. **`list_entities` 的 `OR` 不是全表扫（我的假设被推翻）**：EXPLAIN 给的是 `MULTI-INDEX OR` + `SEARCH x USING INDEX idx_edges_src/dst` ⇒ SQLite 会把 OR 拆成两次索引查找。**不列为 finding。**
4. **`neighbors` 的 `hops` 在产品路径上有界（假设被推翻）**：`api.rs:1260` 是 `q.hops.unwrap_or(2).min(4)`；递归 CTE 自身也有 `walk.depth < ?2`（`lib.rs:264`）。库函数不设上限，但唯一的调用者设了。**不列为 finding。**
5. **`facts_as_of` / `list_entities` 的计划是好的**（都走索引）——第 3 条的缺陷是**比较语义**，不是计划。别把两者混为一谈。
6. **`community.rs::local_moving` 是有界的**：`for _ in 0..MAX_PASSES`（`community.rs:109`）+ 严格增益（`> best + EPS`，140/144）⇒ 不会振荡、不会无限循环。
7. **并发交错不是风险**：所有 DB 访问经单一写者 actor（`Db::call*`），闭包按序执行；因此「一个闭包内多条语句」在**并发**下是自洽的，问题只在**失败/崩溃**时的原子性（§1/§2/§6 因此都按「注入失败」取证，而不是按「并发」）。

## 15 我没能覆盖的范围（必读）

* **`crates/store`**：只按「有没有事务」读了 `sqlite.rs`/`lib.rs`，**没有审计**它的仓储、FTS、迁移（除 0024 的那条警告）。第 6/10 条的修法可能要它配合，但它的缺陷不在本单范围。
* **`crates/daemon` 的其他文件**：`api.rs` 只在有 finding 指向时读（`1288–1292`、`1260`、`1442`、`1497`）；`memembed.rs`（`judge_merge`）与 wiki/memory 侧**未审**。
* **`crates/memory`**：`write_memories` 的语义只在 `distill.rs` 的调用点读；「失败尝试的 memory 该不该留」是 mem-core 的判据，本单只报了图侧。
* **规模/性能**：所有放大实验都是**副本上的合成放大**（×10/×100 行），**没有**生产规模数据，也没有端到端 Rust 计时（我只测了 SQL 段与行数）；第 4/10/11 条的「扩展性」结论是**按计划与增长形状**推的，不是按实测延迟。
* **并发压力**：没有在真写者并发下跑（第 14.7 条只证明交错不可能，未做压力验证）。
* **MCP/面板消费面**：普查覆盖 `crates/**` + `cli/`，**不含** `panel/`（TS）与运行时的 HTTP/MCP 注册（第 12 条的「无人调用」= 在这些 Rust crate 内无人调用）。
* **真守护进程**：未启动、未调用、未读其 `/api/v1/recall`（按纪律）；因此「今天线上的实际行为」只由活库副本 + 测试推断。
* **`gold` 数据与判据**：未复核任何 gold 数值/阈值（那是 RV 的面）；本单只提到读数用于「修后不变」的判据。

## 16 未验证猜想（不许当 finding 用）

* `resolve_seeds` 的 alias probe 数在实际查询上可能远大于我的直觉（`probes` = norm(text) + terms）——**没有**测出一条真实查询各自的 probe/pattern 数量，因此第 9 条的「语句数」是**结构式**而非实测计数。
* `build_communities` 的失败概率（我注入的是人为触发；真实世界有没有触发点未知）；`covered != non_isolated` 窗口的实际时长取决于重建频率。
* `merge_entities` 自环在真实库上出现过没有（活库 0 个自环，但活库**从未**跑过合并的相邻实体对；t9/t27 的 5 次合并是否产生过自环**未查**）。
* 第 4 条改成种子邻域后，召回质量会不会变（局部取边可能改变并列路径的进入顺序）：**必须**用「读数逐位不变」的判据守住，否则就是换个方式改读数。
* `fact_hash`/`normalize_fact` 与 `expired_at`/`created_at` 的语义我只扫过，没有逐条对着 bi-temporal 契约核（`expired_at` 在 `retrieve` 里根本没被读，但我没确认那是刻意还是遗漏）。

## 17 复现脚本（本报告的全部读数都由这些产生）

所有脚本对 `%TEMP%` 的副本操作，源库只以 `mode=ro` 打开；`isolation_level=None` 是**刻意的**——它等于 rusqlite 在 `Db::call*` 里的 autocommit 语义。

* **A** 产品路径 unwrap/panic 普查：对每个文件取最后一个 `#[cfg(test)]` 行号，统计其前的 `unwrap|expect|panic!|unreachable!`。
* **C** `facts_as_of` 的字符串比较（§3）：
  ```python
  SQL="SELECT id FROM entity_edges WHERE (src=?1 OR dst=?1) AND valid_at<=?2 AND (invalid_at IS NULL OR invalid_at>?2)"
  # 三种写法：'2026-09-13T18:38:15Z' / '2026-09-13T18:38:15+00:00' / '2026-09-14T02:38:15+08:00'
  # 正确值：用 datetime.fromisoformat 解析后比较；再把 SQL 换成一参数版本（去掉实体条件）对全表计数
  ```
* **D** 计划：`EXPLAIN QUERY PLAN <code 里的 SQL 原文>`（§4/§10）。
* **E** 放大：把副本 `entities_edges` 复制 N-1 次（**先读出基础行再 `executemany`**，id 加偏移；注意**不要**用 `INSERT … SELECT … FROM 同一张表` 里的自引用子查询——第一次这么写把 `MAX(id)` 与新增行互相喂，脚本死循环，这是我踩过的坑）。
* **F** `build_communities` 半重建（§2）：真实 DDL（把 0019–0024 迁到副本）+ 预置 level-0 分区 + `BEFORE INSERT ON community_entities` 触发器注入失败 + 逐条执行 `community.rs:209–231` 的语句顺序。
* **G** `merge_entities` 半合并/自环（§5/§6）：同法注入在最后一条 `DELETE FROM entities`，随后查 `src=dst` 与 absorbed 行是否还在。
* **H** 吞错误站点普查（§7）：grep `if let Ok\(.*\) = conn.query_row` 与 `unwrap_or\(false\)`。
* **I** 公开 API 普查（§12）：扫 `crates/**`，对每个 `pub (async) fn` 统计「crate 内非测试 / crate 内测试 / crate 外」三类调用点。

---

## 18 本单的边界（自证）

* 唯一写入：本文件（`docs/design/reviews/gen3-audit-graph.md`）。
* 未改：`crates/graph/**`、`crates/daemon/src/distill.rs`、任何测试、任何判据文本、任何 gold。
* 未写活库（源库以 `SQLITE_OPEN_READ_ONLY` 语义的 URI 打开，只 `VACUUM INTO` 到 `%TEMP%`）；未启停 pid 79984。
* 发现即登记，**没有顺手修**：每条都给了「可证伪的修复判据」，由后续实现单决定 owner 与读数。
