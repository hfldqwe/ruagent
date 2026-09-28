# RV-C2-9 一致性裁决：失败尝试的 episode 语义（t41 / mem-core）

- 任务：t41（kind=requirements，**零代码**）；attempt `b808cf68-3512-4f9a-b46f-816b84f99b07`
- inScope（stored）：`docs/design/reviews/gen2-memory-spec.md` · 本报告
- outOfScope（本单未触碰）：`crates/memory/` · `crates/daemon/` · `crates/graph/` · `crates/store/` · `panel/` · 以及 t8 的终态报告 `gen2-memory-impl.md`
- 环境：只读；未写活库、未启停 pid 79984、未用 `/api/v1/recall`

## 1 一句话

一条**跨规格字面冲突**（我的 D4「失败行绝不创建 episode」与我答复 graph 的 D2(b)「不许把 episode 创建挪到 `write_graph` 之后」**互相否定**）现按 captain 裁决**采纳实现**收口：失败尝试**可以**创建 episode，但必须在**同一路径内**标记为 `run_turn_failed`（`meta={voided_by,reason}`、不覆盖既有 meta）并**保留来源**；**唯一禁止的是**让它以 `run_turn` 呈现为成功。**D2(b) 不变**。规格里**只追加**一段，前文一字未删未改。

## 2 冲突的对象集（三处字面）

| # | 位置 | 字面 |
| --- | --- | --- |
| ① | 规格 E.7「做什么」②（第 953 行） | 「**失败绝不创建 episode**」 |
| ② | 规格 E.9-① ④（第 972 行） | 「失败/空产出写 `distill_log` 且**失败不建 episode**」 |
| ③ | 我给 graph 的 D2(b) | 「**不许**把 episode 创建挪到 `write_graph` 之后」 |

互斥的机理：失败点在 `write_graph`，而 ③ 要求 episode 在它**之前**创建 ⇒ 失败路径上「已有 episode」与「失败不建 episode」在字面上不能同真。真正的不变量是第三条：`run_turn` 是徽章的唯一来源（A.5／§7.140）。

## 3 落地的规格追加（第 1156 行起，`## 一致性裁决 RV-C2-9`）

- **§1 冲突对象集**（上表，原文引用、保留原位）
- **§2 取代后的表述**（本节生效的判据文本）：可以创建 / 必须标记（`AND kind='run_turn'` 守卫，重复标记 no-op）/ 必须保留来源；**唯一禁止**是以 `run_turn` 呈现成功
- **§3 「不删除」的机制依据**（记忆侧四条事实，见 §4 本文）
- **§4 徽章契约 + 通则**：只认 `kind='run_turn'`；`manual` 也是既有的非 `run_turn` 溯源 episode ⇒「溯源 episode 存在 ≠ 蒸馏发生过」是**通则**，失败路径只是实例
- **§5 可复现读数**（引用 graph t27/t38，标明非我的读数）
- **§6 风险登记**：`episode::episode_count` 全表计数、不分 kind

## 4 记忆侧事实（我这一侧核对，用于支撑「标记而非删除」）

| 事实 | 位置 | 含义 |
| --- | --- | --- |
| `source_episode INTEGER REFERENCES episodes(id)`，**无 `ON DELETE`** | `crates/store/src/migrations/0004_memory.sql:25`（`sqlite.rs:49` 设 `foreign_keys=ON`） | 删被引用的 episode 会被 SQLite 拒绝；删行还会丢好记忆的来源 |
| 只在插入时写一次 | `crates/memory/src/write.rs:118-126` | 溯源不随 episode 状态变化 |
| 另一处写入只填空、永不清空 | `crates/memory/src/lifecycle.rs:681`（t347 `COALESCE(source_episode, ?3)`） | kind 变更对溯源不可见 |
| `session_memories` 的 JOIN 不看 kind | `crates/memory/src/query.rs:302-312` | 标记前后 C4 读数不变 |
| t347 前缀迁移建的 `manual` episode 有断言 | `crates/memory/src/lifecycle.rs:907` | 「非 `run_turn` 溯源 episode」在记忆侧**早已存在** |

## 5 徽章契约与读数

- 契约：`distilled` 布尔**只**由 `episodes.kind='run_turn'` 派生（执行面 `## C-INT` C-INT-2 / `api.rs` 的 `DISTILLED_EPISODE_KIND`；panel 侧注释 `panel/src/api.ts:296-298`）。
- 读数（**引用** graph，非我复跑：`gen2-graph-repair.md` §1，2026-09-28，失败由 `entity_edges` 的 `RAISE(ABORT)` 触发器触发）：

```
episodes 1 -> 1 | kind=run_turn 1 -> 0 | episode kind="run_turn_failed"
meta=Some("{\"voided_by\":\"write_graph\",\"reason\":\"adding relation runs_on\"}")
memories keeping their provenance 1
```

读法：行数不变（补偿是标记）· `run_turn` 计数减少（徽章不认它）· 来源保留（可审计）。

## 6 `episode_count` 风险登记（零代码，**本单不删**）

- 事实：`crates/memory/src/episode.rs:86` = `SELECT COUNT(*) FROM episodes`，**不分 kind**。
- 调用点：全仓扫描（`.rs` 全部 crate + `panel/src/**.ts` + 测试）**只有定义、零调用点** ⇒ **可删除候选，留待下一代清理**（删除会越出 t41 inScope，故只登记）。
- 规则（写进规格 §6）：**任何「蒸馏发生过」的代理都必须按 `kind` 过滤**；今天它无害的唯一原因是**没有调用点**，不是它的形状正确。

## 7 与 graph t38 的 E.10/D4 确认

graph 来问过 E.10/D4 的一致性确认，我已**正面回答：同意**，并给出一处**措辞加强**（`manual` 也是既有的非 `run_turn` 溯源 episode ⇒ 徽章只认 `run_turn` 是**通则**而不是失败路径的例外）与一处**风险面**（`episode_count`）。规格措辞按 captain 要求**由本单（t41）落**，未让 graph 代改规格。

## 8 越界声明与自检

- 本单只改：`docs/design/reviews/gen2-memory-spec.md`（**只追加**一段，1156 行起）+ 本报告。
- 未改：`crates/memory/**`、`crates/daemon/**`、`crates/graph/**`、`crates/store/**`、`panel/**`、`gen2-memory-impl.md`（t8 终态报告，仍 260 行原状）。
- **未改任何判据文本**：D2(b) 原文、E.7/E.9 旧措辞、C 节各 target 的判据与数字全部保留原位；本段只**取代**两处旧措辞并给出可同时成立的表述。
- 复核命令（只读）：
  - `Select-String -Path docs\design\reviews\gen2-memory-spec.md -Pattern '一致性裁决 RV-C2-9|失败绝不创建 episode|失败不建 episode'` ⇒ 三处都在（前两处为原文、第三处为本段），证明「只追加、未删原文」。
  - `Select-String -Path crates\memory\src\episode.rs -Pattern 'episode_count'` + 全仓扫描 ⇒ 只有定义（§6 的事实）。
