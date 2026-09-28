# gen2 知识图谱规格（R-C · t3）

> 状态：**只读规格**。本单未改任何代码（`git status --porcelain` 只有本文件）。
> 读数时间基准：**2026-09-27T13:58Z（本地 2026-09-27T21:58+08:00）**，活库 `C:\Users\19410\.ruagent\data\ruagent.db`（文件 mtime 2026-09-27T21:15:21 本地），守护进程 pid **79984** 在 `127.0.0.1:8787`（`GET /api/v1/health` → `{"status":"ok"}`）。
> 本单**没有**调用 `GET /api/v1/recall`（它会写 `recall_log`）。用到的 HTTP 只有 `GET /api/v1/graph/*` 与 `GET /api/v1/health`（只读）。
> 消费方：I-C（实现，crates/graph + daemon/distill.rs）、V-C（独立验证）、RV-C（判决）。

## 0 一句话结论

图本身是对的、索引是好的、双时态的表结构是业界最领先那一档（Graphiti 同款）；**坏的是接线与语义**：生产召回走的是 strict 短语腿（`api.rs:2524`），在 23 条真实查询上非空 **1/23**；**0/67** 的边有来源；**66/67** 的边所谓「事件时间」就是写入时刻；**20/63** 实体是孤点；关系词表 **24/35** 是一次性的；抽取质量**没有任何 gold 读数**，而失败率只活在日志里（同窗口 22 次尝试里 21 次失败）。因此本代的重点不是「加更多图算法」，而是 ①**把种子解析接进召回**、②**让每条边带来源与真时间**、③**把抽取质量做成可测的量**；多跳检索（HippoRAG 式传播）排在其后，社区摘要层（GraphRAG 式全局问答案）再后。

### 0.1 读数纪律（本文件每行都遵守）

| 项 | 约定 |
| --- | --- |
| 对象集 | 每个读数都写明「数的是哪些行」，例如「`entity_edges` 全部 67 行」而不是「图的边」 |
| 采样面 | 真实查询帧 = `recall_log` 里出现过的**全部不同查询**（现场 23 条），不是构造查询；自查询帧 = 全部 63 个实体名 |
| 判据 | 每条都写成可机械复算的式子；判据不成立时写「不成立」，不成立就**不写进规格** |
| 时间窗 | 每个读数带自己的窗口。`distill_log` 的窗口（09-13..09-26）与 `daemon.log` 的窗口（09-26T04:52:45Z..09-27T13:48:48Z）**不同**，两者不得合并读（mem-core 2026-09-27 明确要求） |
| 复现 | A 节每行给命令；F 节是完整复现器（单文件、只读、无第三方依赖） |
| 旧值 | 引用旧读数必带时间点与来源（closure §1 / t247 / t250 的行号） |

### 0.2 现场读数与旧读数的差异（先声明，避免误读）

| 项 | 旧读数（来源） | 本单现场读数 | 差异解释 |
| --- | --- | --- | --- |
| entities | **50**（任务单给的 2026-09-27 读数；closure §1 记 63） | **63** | 活库在自变（closure §7.2）；本单所有实体相关读数以 63 为分母 |
| 实体腿在召回 | **574/574 `entities=0`**（closure §1 行 31，t247） | **646/651 `entities=0`**（5 行为非 0） | 同一缺陷仍在；5 行非 0 全在 2026-09-26 且都是字面 `ruagent` 查询（strict 腿唯一能命中的形状） |
| 实体自查询 | **63/63 命中**（closure §1 行 33，t247） | **63/63 命中**（复刻 strict 腿） | 未变；仍证明「索引没坏、构造坏了」 |
| strict/loose 修复 | t250 修了查询构造，6 类矩阵 `autohotkey-v2 strict 0 → loose 1`（closure §6.1 行 142） | strict 0 → loose 1 **复现成立** | t250 修的是 **loose 入口**；**生产召回没用它**（见 A9） |

---

## A 基线表

每行：对象集 · 采样面 · 复现命令 · 读数 · 时间窗。
`P` = `python <F节脚本>`（脚本存到 `%TEMP%\graph-baseline.py`）；`S` = 只读 SQL；`H` = 只读 HTTP GET。

| # | 对象集 | 采样面 | 复现命令 | 读数（现场） | 时间窗 |
| --- | --- | --- | --- | --- | --- |
| A1 | 图上全部节点/边计数 | `entities` 全表 63 行 / `entity_edges` 全表 67 行 | `P`（A1 counts） | entities **63**；edges 总 **67**、当前有效 **60**、已失效 **7**（10.4%）；悬空边 **0**；自环 **0**；有 kind **63/63**；有 summary **62/63** | 至 2026-09-27T13:58Z |
| A2 | 边的来源字段 | `entity_edges` 全表 67 行 | `S`: `SELECT COUNT(*) FROM entity_edges WHERE source_episode IS NOT NULL` | **0/67 = 0%**。对照：`memories.source_episode` 非空 **162/163 = 99.4%** | 同上 |
| A3 | episodes 与「蒸馏来源」的可用性 | `episodes` 全表 17 行 | `S`: `SELECT kind,COUNT(*) FROM episodes GROUP BY 1` | `mcp_write` **16**、`manual` **1**、`run_turn` **0**。**不可判定**：ep17 的 content 是 `legacy distilled memories: the [distilled] body prefix was r…`（2026-09-26T21:09:10Z），而盘上有 `ruagent.db.before-t229-cleanup-20260926-150802`（1.36 MB）⇒ 156 条指向 ep17 的记忆是 **t347 的回填**，不是 distill 写的；活库因此**不能**用来判断「distill 的 run_turn episode 路径是否可用」。判据必须另开干净库（见 C·G3） | 同上 |
| A4 | distill 的产出记账 | `distill_log` 全表 33 行 | `S`: `SELECT COUNT(*), SUM(memories_written),SUM(entities_written),SUM(relations_written) FROM distill_log` | 33 行；三者全 0 **18/33 = 54.5%**；entities=0 **21/33 = 63.6%**；relations=0 **26/33 = 78.8%**；累计写入 mem **85** / ent **69** / rel **39**；agent 全是 `dsh` | **窗口 2026-09-13T17:06:33Z .. 2026-09-26T10:28:22Z** |
| A5 | distill 的失败率（日志侧，窗口与 A4 **不同**） | `daemon.log` 全部 360 行；其中含 `distill` 的 22 行 | `S`: `Select-String daemon.log -Pattern "distill"` | 22 行 Try 中 **1 行成功**（`auto-distilled … memories=9 entities=8`，10:28:22.967Z）+ **21 行 WARN `auto-distill failed … error=Query returned no rows`** = **95.5% 失败**。同一窗口内 `distill_log` **只有 1 行**（`ruagent:5213faf6e69d4fbf`，mem 9 / ent 8 / rel 7）⇒ **库里只记 1/22 = 4.5% 的尝试** | **窗口 2026-09-26T04:52:45Z .. 2026-09-27T13:48:48Z**（= 当前 daemon.log 文件首行..末行；日志在 5 MB 轮转） |
| A6 | 失败根因（只读核对） | A5 的 21 个失败 session key | `S`: `SELECT COUNT(*) FROM sessions WHERE key=?` | 21 个 key **现在全部**在 `sessions` 里（现场逐个查了 6 个 + 33 个 distill_log key 全部命中）⇒ 失败不是「会话不存在」，而是**索引落地与自动蒸馏的竞态**；失败串来自 `distill.rs:497（t85 勘误：原 256；锚 load_messages 定义）`（`load_messages` 的 `session not indexed` 路径，实际是 `QueryReturnedNoRows`） | 同上 |
| A7 | 关系词表 | `entity_edges` 67 行的 `relation` | `P`（A2） | 不同关系名 **35**；只出现 1 次的 **24（68.6%）**；当前有效边前五：`uses 12`、`supports 4`、`orchestrates 4`、`integrates_runtime 3`、`borrows_design_from 3`。长名字例：`uses_model_for_permission_gatekeeping`、`uses_as_production_database` | 至 2026-09-27T13:58Z |
| A8 | 时序有效性是否携带信息 | `entity_edges` 67 行 | `P`（A3） | `abs(valid_at − created_at)`：**66/67 ≤ 1 秒**（中位 0.000），只有 **1 行 > 60 秒**（最大 41920.26 s）。`invalid_at = expired_at` 的 **0/7**。⇒ 所谓双时态的事件时间轴在 98.5% 的边上**等于写入时刻**；7 条被失效的边里有同一事实的中英改写各一条（edge 18 英文 → edge 33 中文 → edge 66 现行） | 同上 |
| A9 | 实体消解 / 别名 | `entities` 63 行的 `name` | `P`（A4） | 判据：去尾部括号后小写归一相等 = **3 组**（`Agent Client Protocol (ACP)` ↔ `Agent Client Protocol`；`Model Context Protocol (MCP)` ↔ `Model Context Protocol`；`DeepSeek Harness (dsh)` ↔ `DeepSeek Harness`）；再做 token 子集 = **+1 对**（`Graphiti` ⊂ `Graphiti / Zep`）。**冗余实体 4 个 / 63 = 6.3%**，且 `dsh`(#58)/`acpx`(#41)/`dsh-kanban`(#59) 这一族无法用同一判据自动裁决（需要别名表或人工未决队列） | 同上 |
| A10 | 拓扑与可达 | 当前有效边 60 条构成的**无向**图；63 个节点 | `P`（A5） | 连通块 **23** 个（大小 `[35,4,4,1×20]`）；最大块 **35/63 = 55.6%**；孤立实体 **20/63 = 31.7%**。每节点可达数：hops=1 均值 **1.5** / 中位 **1**；hops=2 **9.5 / 4**；hops=3 **11.3 / 6**。最大块内 3 跳覆盖对 **686/1190 = 57.6%**。枢纽 `ruagent`(#1)：当前事件边 **33/60 = 55.0%**，不同邻居 **22** | 同上 |
| A11 | 实体腿：自查询对照 | 全部 63 个实体名各查自己（复刻 strict 腿） | `P`（A6） | `strict` 命中 **63/63**；`loose` 也 **63/63** ⇒ 索引与解析对「名字本身」没坏 | 同上 |
| A12 | 实体腿：真实查询帧（**本单的核心读数**） | `recall_log` 全部**不同**查询 23 条，1 条 = 1 个样本（不按调用次数加权） | `P`（A6） | `strict` 非空 **1/23 = 4.3%**（唯一命中是字面 `ruagent`）；`loose`（t250 的入口，生产未接）非空 **13/23 = 56.5%**；**纯文本重合上界**（任一 ≥2 字 run 出现在名字/摘要里，或名字的 run 出现在查询里）非空 **13/23 = 56.5%** ⇒ 剩下 **10/23 指向图里根本不存在的对象**（`kettle`、`kettle material`、`部署流水线`、`wiki`、`deploy`、`deploy pipeline`、`panel`、`轮询 自动通知 用户偏好`、`T97B rollback fail`、`中文交流 简洁 直接 表达习惯`）。逐查询读数见 F 节输出 | 同上（查询来自 2026-09-16..2026-09-27 的 651 次调用） |
| A13 | 实体腿：生产侧读数 | `recall_log` 全表 651 行 | `P`（A7）+ `S` | 651 行里 `entities>0` 的 **5 行 = 0.77%**（平均 0.0246），全部在 **2026-09-26**，查询全是字面 `ruagent`；**最后一行**（id 651，2026-09-27T13:23:02Z，`ruagent memory`）`entities=0`。逐日：09-16..09-25 每天 0 行非 0；09-26 有 5 行 | **窗口 2026-09-16T13:05:24Z .. 2026-09-27T13:23:02Z** |
| A14 | 生产召回帧的采样偏差 | 651 行 / 23 条不同查询 | `S` | `kettle` **303** 次 + `autohotkey-v2` **297** 次 = **600/651 = 92.2%**（closure §1 行 28 记 96.5%，那是探针簇更早的窗口）⇒ 生产帧**也**被两个探针主导；A12 用「不同查询」去重只能部分去偏，必须在读数里写明 | 同上 |
| A15 | live HTTP 端点（GET，只读） | 5 个端点各一次 | `H`（见 F 节 `http-probe` 小节） | `GET /graph/entities?limit=100` → 63；`GET /graph/edges?limit=5` → `total=60`；`GET /graph/search?q=ruagent` → 10 条 + `match=exact`（多数是 **summary** 命中，不是名字命中）；`?q=autohotkey-v2` → 1 条 + `match=candidate`；`?q=kettle` → **0 条 + `match=none`**；`GET /graph/entity/1/neighbors?hops=2` → 24（depth1 22 / depth2 2）；`?hops=4` → 32（depth1 22 / depth2 2 / depth3 7 / depth4 1）；`GET /graph/entity/1/facts` → 每条 fact 的 `source_episode` 全为 `null` | 2026-09-27T13:5x Z |
| A16 | 图 crate 现有测试基线 | `cargo test -p ruagent-graph`（3 个单测 + 2 个集成测试文件） | `$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"; $env:CARGO_TARGET_DIR=$env:TEMP\ruagent-rc; cargo test -p ruagent-graph` | `bi_temporal_supersession_and_as_of_queries` ok、`multi_hop_traversal` ok、`entity_search` ok、`two_char_ascii_queries_degrade_instead_of_erroring` ok、`cjk_substring_needs_like` ok、`query_shape_matrix` ok、`hyphen_phrase_requires_adjacency` ok ⇒ **7 passed / 0 failed**（0.04 s + 0.04 s） | 2026-09-27T21:5x +08:00 |
| A17 | 生产召回只用了 strict 腿（代码判据） | `crates/daemon/src/api.rs:2524` | `Select-String crates\daemon\src\api.rs -Pattern "search_entities"` | 召回处理器调 `ruagent_graph::search_entities`（strict）；`search_entities_loose` 只在面板搜索端点（`api.rs:776`）被调 ⇒ **t250 修的入口没有被召回接上**。这条与 A12 的 `strict 1/23` 是同一件事的两个面 | 至 2026-09-27T13:58Z |

### A.18 已知缺陷逐条复核（题面 5 条；成立才写进规格）

| 题面缺陷 | 复核结论 | 证据 |
| --- | --- | --- |
| ① 实体腿曾在召回里恒为 0，而「用实体自己的名字查自己」63/63 命中 ⇒ 问题在查询构造而非索引 | **成立**（且比原描述更窄） | A11 `63/63`；A12 `strict 1/23`、A13 生产 `5/651`；A17 召回走 strict。**但**「查询构造」只解释 12/23 的缺口：纯文本解析上界 **13/23**，另 **10/23 的查询对象压根不在图里**（A12）⇒ 这一条必须与 ⑤ 一起读，否则会把抽取覆盖问题误诊成检索问题 |
| ② 无多跳/传播式检索（只见 1 跳邻居；无 PPR 式图扩散） | **部分成立** | `neighbors` 有递归 CTE，端点默认 2 跳、上限 4（`api.rs:1161`），A15 的 `hops=4` 真返回 depth 1..4 ⇒ 多跳**能力在**；但 **`ruagent_graph::neighbors` 在生产召回里没有任何调用点**（`Select-String ruagent_graph::` 全仓只有 `api.rs` 的图页面端点与 `distill.rs`），召回只做 `current_facts`（1 跳）。所以正确表述是「无多跳**检索**」，不是「无多跳」 |
| ③ 无社区/主题摘要层 ⇒ 无法回答全局性问题 | **成立** | `crates/store/src/migrations/*.sql` 里没有 community/topic 表；`ruagent_graph` 无社区 API；A10 的 23 个连通块（最大 35/63、孤立 20）说明即使现在做社区，也有 20 个节点没有社区 |
| ④ 实体消解与别名规则没有命名判据（「同一个实体」没有对象集） | **成立** | `upsert_entity` 的判据只有 `norm()` = `trim().to_lowercase()`（`graph/src/lib.rs:38-40`），唯一索引 `idx_entities_norm`；没有别名表、没有对象集定义。A9 用一条机械判据量出 **4 个冗余实体**，并暴露判据边界（`dsh` 一族裁决不了） |
| ⑤ 抽取质量没有人工 gold（precision/recall 未知），distill 的失败率被日志层掩盖 | **两部分都成立** | 无 gold：`distill_log` 只有三类计数列（`0008_memory_embeddings.sql:9-16`），没有任何质量列；失败被掩盖：现场窗口内库 **1 行** vs 日志 **22 次尝试（21 失败）**（A4/A5，窗口不同但同在 09-26）。注意方向：失败**在日志里**、不在库里；`distill_log` 的 54.5% 全 0 是**已去重或真没抽到**，**不能**当成失败率（closure §1.1 已自纠过这一点） |

---

## B SOTA 对照（8 条一手来源，逐条 采纳/不采纳 + 理由 + 代价）

| # | 来源（一手，带 URL） | 采纳/不采纳 | 理由 | 代价 |
| --- | --- | --- | --- | --- |
| B1 | GraphRAG — Edge et al., *From Local to Global: A Graph RAG Approach to Query-Focused Summarization*，<https://arxiv.org/abs/2404.16130> | **部分采纳**：采纳「层次社区 + 每社区摘要 + 全局 map-reduce」这一**结构**，作为 P1；不采纳它的**索引期全量 LLM 摘要**形态 | 采纳的结构解决一个我们真实缺的能力（全局性问题）；A10 说明我们的图有 23 个连通块、最大块 55.6%，正好是社区划分的输入。不采纳索引期形态的理由是成本形态与我们相反：我们是单用户本地库、每次 distill 只有 1 次 ACP 往返预算 | 每社区一次 LLM 调用 + 一次社区划分（Leiden/Louvain 在 SQLite 邻接上纯 Rust 可做，无新依赖）；社区摘要需要**失效策略**（图变了摘要就旧了），否则添一个会说谎的层 |
| B2 | LazyGraphRAG（Microsoft Research，2024-11-25；2025-06-06 编者按），<https://www.microsoft.com/en-us/research/blog/lazygraphrag-setting-a-new-standard-for-quality-and-cost/> | **采纳**（这是 B1 的修正形态）：采纳「延迟 LLM 使用 + 单一成本旋钮（relevance test budget）」与「LLM 成对判据：comprehensiveness / diversity / empowerment」 | 该页给出的一手读数正是我们需要的形式：索引成本 = vector RAG、且为**完整 GraphRAG 的 0.1%**；全局质量的**查询成本低 700 倍以上**；用 C2 的 **4% 查询成本**即显著优于全部对照条件。它同时把评测协议写清了（100 条合成查询、LLM 成对比较、三指标）⇒ 我们不必自创判据 | 查询期 LLM 调用 ⇒ 延迟上升，需要显式 budget（我们建议每查询 ≤2 次 relevance test、≤1 次摘要生成）；「4%/700×」是 AP 新闻语料上的读数，**不能**直接搬到 63 实体的图，必须自己复测（C·G8 的标定读数即为此） |
| B3 | HippoRAG — Gutiérrez et al.，<https://arxiv.org/abs/2405.14831>；HippoRAG 2 — *From RAG to Memory*，<https://arxiv.org/abs/2502.14802> | **采纳原则，不采纳实现**：采纳「个性化传播（PPR）是多跳召回的正确打分模型」；不采纳「全图 PPR 矩阵 + 每次查询重建邻接」 | 我们的图是 **63 节点 / 60 边**（A1/A10），线性代数版 PPR 的收益远小于它的常数与内存；但 PPR 的**排序思想**（离种子越近、度越低、边越罕见 ⇒ 越该被取出）刚好治我们的病：枢纽 `ruagent` 独占 55% 的边（A10），不减权则任何路径都穿过它 | 有界的 beam 搜索会漏；漏多少有读数：hops=3 对覆盖仅 **57.6%**，hops=1 平均只到 **1.5** 个邻居 ⇒ 必须把「截断」当一等输出（D 节的 `truncated_by`），否则读者会把「没到」当成「不存在」 |
| B4 | LightRAG — Guo et al.，<https://arxiv.org/abs/2410.05779> | **采纳概念**：采纳「双层检索」= 低层**实体关键词**（名字/别名）+ 高层**主题关键词**（社区主题）；不采纳它的专用 KV/向量库栈（见 A.18 ③：社区层今天不存在） | 我们已有 SQLite + FTS5 + LanceDB，不需要再引一层存储；而「低层 + 高层」正好对应我们的两个种子来源：实体名/别名（今天就是坏的，A12）与社区主题（今天不存在，A19③） | 高层关键词要等社区层（P1）才有东西可用 ⇒ 双层里的高层在第一阶段只能是单层，规格里必须写明「第一阶段只有低层，这不是完成态」 |
| B5 | Zep / Graphiti — Rasmussen et al.，*Zep: A Temporal Knowledge Graph Architecture for Agent Memory*，<https://arxiv.org/abs/2501.13956> | **部分采纳**：采纳双时态（今天已有）+ 「矛盾的边失效而不删除」（今天已有）；**新增采纳**两件：①事件时间必须**来自抽取**而不是 `now()`；②冲突判定**不能只看 (src,dst,relation) 三元组**。不采纳完整 Graphiti 的 LLM 边去重/失效判定（每次写入一次 LLM 调用） | 我们现状是 A8：66/67 的边「事件时间 = 写入时刻」，等于双时态只有一根轴；A9 的取样里同一事实被写成 3 行（英→中→现行），说明三元组级 supersession 能兜住「同关系改写」，但**兜不住**同义不同关系的表述 | 不做语义冲突判定 ⇒ 同一事实会留下多条并行边（已实测 3 行 1 事实）；代价是在召回侧必须做去重（按 fact 文本或 entity 对聚合），否则边数虚高、证据里出现重复事实 |
| B6 | OpenSanctions Pairs — Smith et al.，<https://arxiv.org/abs/2603.11051> | **采纳结论，不采纳主路径**：采纳「成对匹配接近天花板，注意力应转向 blocking / clustering / uncertainty-aware review」；不采纳「用 LLM 当主匹配器」 | 该文一手读数：规则基线 **91.3% F1**、GPT-4o **99.0% F1**、本地 `DeepSeek-R1-Distill-Qwen-14B` **98.2% F1**；并明确两类失败**互补**（规则过匹配、LLM 跨文字系统转写弱）。我们的消解面只有 4 个冗余对（A9）⇒ 规则 + 别名表 + 未决队列足够，把 LLM 留给定不了的少数对最划算 | 未决对需要一个人工/LLM 复核入口（今天没有：`entities` 只有 upsert/delete，`api.rs:1070/1130`）；不做 blocking ⇒ 每对新实体与全部旧实体比对，63 实体下没问题，≥1 万时需要 blocking |
| B7 | Text2KGBench — Mihindukulasooriya et al., ISWC 2023，<https://arxiv.org/abs/2308.02357> | **采纳评测协议**：采纳它把评测分成**三族**——事实抽取 / 本体一致性（concepts, relations, domain-range）/ 幻觉；不采纳它的 Wikidata/DBpedia 本体 | 我们有 A7 的读数：35 个关系名里 **24 个是一次性的**，说明**没有本体**这件事已经被量出来了；三族指标正好把「词表外的关系名」「kind 不在集合里」「事实在 transcript 里找不到支撑」分开量。它是**唯一**给出 7 个可机械复算指标的公开基准 | 需要人工 gold（本规格里唯一无法自动化的成本）：建议 **20 个会话**的手工标注（实体/关系/时间），落在 `docs/design/reviews/gold/`；标注成本估算 ~2 小时/人 |
| B8 | GraphRAG auto-tuning（Microsoft Research），<https://www.microsoft.com/en-us/research/blog/graphrag-auto-tuning-provides-rapid-adaptation-to-new-domains/> | **采纳**：采纳「抽取 prompt 与本体必须按领域调优，抽取对 prompt 高度敏感」，故 D3 的 prompt 改动必须带版本指纹 | 该页一手读数：同一语料上 auto-tuned prompt 抽出 **4896 entities / 8210 relationships / 1027 communities**，显著多于默认 prompt ⇒ 「换了 prompt，产出量会变」不是猜测而是实测现象。这正支持 mem-core 的闸门 3（成本可归因） | 需要一次调优跑批（20 会话 × 2 个 prompt 版本）；prompt 指纹要进 `distill_log`（一次 schema 变更，交 I-SCHEMA） |

---

## C 目标表

每行五要素：**metric / baseline / target / 复现命令 / 标定读数**。target 一律可证伪；标定读数是「为什么这个 target 不是拍脑袋」的现场依据。
`P` = F 节脚本；`T` = `$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"; $env:CARGO_TARGET_DIR="$env:TEMP\ruagent-ic"; cargo test -p ruagent-graph <额外参数>`。

### G1 实体腿不再恒空（resolution 优先，最高优先级）
- **metric**：真实查询帧上「实体腿非空」的比例（帧 = `recall_log` 的全部不同查询，现场 N=23）
- **baseline**：**1/23 = 4.3%**（strict，生产路径）；生产加权读数 **5/651 = 0.77%**（A12/A13）
- **target**：≥ **13/23 = 56.5%**；生产上非探针查询的 `entities>0` 行比例 ≥ 50%
- **复现命令**：`python %TEMP%\graph-baseline.py`（A6 / A7 两段）
- **标定读数**：`loose` 今天**已经**是 13/23（t250 修过入口，生产没接），而纯文本重合上界也是 **13/23** ⇒ 目标是「**把已有入口接进召回**」，不是「把上界抬到 23」。剩余 10/23 由 G7 负责（那些查询指向的对象不在图里）

### G2 每个种子必须说得出是哪条腿找到的
- **metric**：`SeedLeg` 四条腿（`ExactName` / `NameToken`（含别名表）/ `SummaryFts` / `VectorNearest`）各自的命中计数；每条腿在 23 条真实查询上的非空计数
- **baseline**：无（今天只有 strict/loose 两个黑箱；`match` 字段只存在于面板端点 `api.rs:784`，召回与注入都看不到）
- **target**：`resolve_seeds` 对 23 条真实查询 + 63 个实体名给出逐条四元组读数；每条腿的计数（含 0）都出现在证据里；`cargo test -p ruagent-graph --test seed-resolution` 覆盖 23 条
- **复现命令**：`T --test seed-resolution`
- **标定读数**：A12 的逐查询 strict/loose/textual 三列已是同一形状的读数（F 节输出）⇒ 目标是把这张表搬进 crate 里、由测试钉住，而不是新造读数形状

### G3 每条新边带来源（episode）
- **metric**：新写入的 `entity_edges` 行里 `source_episode IS NOT NULL` 的比例
- **baseline**：**0/67 = 0%**（A2）。**活库不可作判据**：`run_turn` episode = 0，156 条记忆指向的 ep17 是 t347 回填的 `legacy distilled memories`（A3）⇒ 活库读数**不可判定**
- **target**：在**临时 root 的新库**上跑一次 distill，本次抽取写入的每条边 `source_episode` = 本次 episode id（**100%**）；`record_episode` 失败时保持 **NULL**（不许 0/-1/legacy 回填，mem-core D1(a)(b)）；活库历史行**不追溯**
- **复现命令**：新库路径上 ~~`cargo test -p ruagent-graph --test provenance-fresh-db`~~ ⇒ **勘误（t38，见附录 H.2）**：该 target **不存在**（全树 0 命中）；真实目标 `T --test temporal`（`the_old_writer_leaves_the_source_unknown_and_the_new_one_records_it`）+ `T --test extraction-gold`（边来源与本体），实跑 exit 0；再 `S`: `SELECT COUNT(*) FROM entity_edges WHERE source_episode IS NULL`
- **标定读数**：memories 侧 162/163 有来源这件事**看起来**是正例，但 A3 证明其中 156 条是回填 ⇒ 「99.4% 有来源」这条旧读数**不能**用来给图边担保；这正是 §7.136「新库上验证过的字段不能替历史行背书」的同族

### G4 时间语义（事件时间与记录时间分开）
- **metric**：① 边的 `event_time_source` 分布（`extracted` / `recorded`）；② 时点查询（`facts_as_of`）在 gold 上的正确率
- **baseline**：**66/67（98.5%）** 的 `|valid_at − created_at| ≤ 1s`（A8）⇒ 事件时间轴目前不含信息；`invalid_at = expired_at` **0/7**；gold 不存在
- **target**：新抽取的边 100% 带 `event_time_source` ✅（已判：写入侧两态都在读数里）；`extracted` 比例 ≥ **30%** —— **deferred（t38 裁决）**：需一次真实重蒸馏，且下限**挂在 G7 的 gold 会话构成上** ⇒ **「30% 在 gold 会话落地前不判」**，并入 G7 的数据待办（H.3 第 3 条）；gold 6 条时点查询 **6/6** ✅（已判，见下）；`valid_at` 未知时写 NULL 语义（`TemporalStatus::RecordedAtOnly`）而不是 `now()` ✅
- **复现命令**：`python %TEMP%\graph-baseline.py`（A3 段）+ ~~`T --test as-of-gold`~~ ⇒ **勘误（t38，见附录 H.2）**：真实目标 `T --test temporal`（`as_of_gold_returns_what_was_true_then`），实跑读数 `as_of ruagent at … -> MATCH` ×6
- **标定读数**：1/67 的边有真事件时间（41920 s 那一条）就是现存唯一正例 ⇒ 「≥30%」在改造前无法达到、改造后必须复测，不能声称一次到位

### G5 多跳检索（有界、可归因、带路径证据）
- **metric**：单查询返回的 `paths` / `facts` 条数；gold 20 查询上的 **path-recall@12**；`truncated_by` 的分布
- **baseline**：生产 = `current_facts(strict 命中)`（0 跳扩展）：每查询 facts **中位 0 / 均值 2.22**（strict 种子）；即使换成 loose 种子，1 跳均值也只有 **13.22**、中位仍 **0** ⇒ 分布双峰
- **target**：默认 `hops=2`（硬上界 3）、`beam=8`：`paths ≤ 12`、`facts ≤ 24`；gold path-recall@12 **≥ 0.60**；每查询 facts 中位 **≥ 3**；每次返回都给出 `truncated_by`（撞哪个预算）
- **复现命令**：`T --test multihop-gold`（gold 路径集）+ `python %TEMP%\graph-baseline.py`（A5 段）
- **标定读数**：hops=1 平均可达 **1.5** 邻居、hops=2 **9.5**、hops=3 **11.3**（中位 1/4/6）；hops=3 对覆盖 **686/1190 = 57.6%**；枢纽占 **55%** 的边 ⇒ ① 3 跳是「多数可达，不是全可达」，必须报截断；② 不做 `rel_idf × degree_norm` 归一，所有路径都会穿过 `ruagent`，证据退化成「一切都与 ruagent 有关」

### G6 消解与别名（判据先行）
- **metric**：冗余对象对数（判据 = 去尾部括号后小写归一相等 **或** token 子集），以及别名表命中次数
- **baseline**：**4 对**（3 组括号同名 + `Graphiti ⊂ Graphiti / Zep`）/ 63 实体（A9）
- **target**：冗余 **0**；`entity_aliases` 存在且解析时**别名命中合并为同一对象**；无法用判据裁决的对（`dsh`(#58) / `acpx`(#41) / `dsh-kanban`(#59) 一族）进**未决队列**并在读数里出现（不许静默丢弃）
- **复现命令**：`T --test resolution` + `python %TEMP%\graph-baseline.py`（A4 段）
- **标定读数**：A4 的判据是机械的（探针可逐对复算），4 对是**现场**的实数；判据边界（`dsh` 一族）已用样本写明，因此 target=0 只对「判据内的对」成立，判据外的对由未决队列负责

### G7 抽取质量 gold（本代唯一无法自动化的一项）
- **metric**：三族（Text2KGBench 协议）：① 实体/关系 precision & recall；② 本体一致性（`kind` ∈ 受控集合、`relation` ∈ 受控词表、src/dst 指向已声明实体）；③ 幻觉率（fact 文本在 transcript 里找不到支撑）
- **baseline**：**无 gold**。可用的代理读数：关系名 **24/35 单例（68.6%）**（A7，**对象集 = 全部 67 条边**；同口径换成 **current 60 条边** 是 **26/35**，多出 `owns`、`runs_on` —— t38 更正：两个对象集必须分开写，见 §C·G7 target 与 RV-C2-6）；`distill_log` 三者全 0 **18/33 = 54.5%**（A4，注意这不是失败率）
- **target**（**t38 逐行裁决：本代能判的已判，不能判的显式 `deferred` —— 标 deferred 的四项仍是「未达成」，不是达标**，依据与前置见附录 H.3）：
  - 实体 P ≥ **0.85** / R ≥ **0.70** —— **deferred**（需人工标注的工程实体族 gold；本代 gold 三族是边/关系名/实体对，无实体族、无会话语料）
  - 关系 P ≥ **0.80** —— **已达标并关闭**（t27：`45/60 = 0.7500 → 43/47 = 0.9149`，同一份标注未改；见 `gen2-graph-repair.md` §2）
  - 关系 R ≥ **0.60** —— **deferred**（活库 **0/67** 历史边有 `source_episode` ⇒ 无可回溯语料；前置是一次真实重蒸馏跑批）
  - 非法 `kind`/`relation` = **0** —— **部分落地**：关系名侧已「拒收/归一」（非关系名不落盘并计数，t27 §2）；`kind` 侧受控集合仍在 E7/D3 的 prompt 闸门内
  - 幻觉率 ≤ **0.10** —— **deferred**（**机制半边已在 fixture 上被测**，含「分不清改写与发明」的已知假阳性；缺的是语料，不是实现）
  - gold 文件落 `docs/design/reviews/gold/`（**勘误：真实位置是 `crates/graph/tests/gold/`，见附录 H**）
- **复现命令**：~~`python docs/design/reviews/gold/score.py --gold docs/design/reviews/gold/*.json --pred <dump>`（脚本由 I-C 落，gold 由人工标注）~~ ⇒ **勘误（见附录 H）**：真实复现是测试本身 `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-graph --test extraction-gold -Nocapture`（gold 由人工标注，判据与 gold 同源在 `crates/graph/tests/gold/`；**不落 `score.py`**）
- **标定读数**：A7 的 35/24 与 A12 的「10/23 查询对象不在图里」是两条**独立**证据，都指向抽取覆盖/本体缺失 ⇒ 在 gold 落地前，任何「抽取质量提升了」的说法都不可证伪，规格里因此把 G7 列为**阻断 G1 剩余部分与 G8 的前提**

### G8 社区/主题摘要层（全局性问题）
- **metric**：**结构侧（t38 降为前置条件，不再是 target）**：分区数、`covered == non_isolated`、`split_by_modularity`、重建幂等、摘要惰性；**质量侧（本代唯一可失败 target）**：GraphRAG 三指标成对判决胜率 ≥ **60%** vs 无社区层的对照组 —— **deferred（t38 裁决）**：需 LLM 判决跑批（协议照 B2），见附录 H.3 第 4 条
- **baseline**：**0**（无 community 表；A10：23 块、最大 35/63、孤立 20）
- **target**：~~覆盖 ≥ **90%** 的非孤立实体~~ —— **t38 更正 RVC-9/RV-C2-7：这条按构造必真**（`covered` 的定义就是非孤立实体数，断言 `covered == non_isolated` 再断言比率 ≥0.90，永远不可能失败 ⇒ **无信息量，降为前置条件**）。取而代之的**可被证伪**结构指标（t38 立，已进测试）：**最大社区 ≤ 50% 的非孤立实体**（今天 14/43 = 32.6%）与 **孤立实体比例 ≤ 40%**（今天 20/63 = 31.7%）—— 两者都能红；全局查询注入 ≤ **2** 条社区摘要（注入面 owner 在 INT）；三指标胜率 ≥ **60%** **deferred**
- **复现命令**：~~`T --test communities`~~ ⇒ **勘误（t38，见附录 H.2）**：该 target **不存在**；真实目标 `T --test extraction-gold`（社区结构断言与读数），实跑：`level 0: communities=9 covered=43 non_isolated=43 split_by_modularity=1`、`partition: 9 communities, sizes [14, 5, 3, 3, 2, 6, 2, 4, 4]`；判决脚本（LLM 成对比较，协议照 B2）**deferred**
- **标定读数**：孤立实体 **20/63 = 31.7%** 必须先补边（或先承认覆盖率分母只有 43）——这是「先量分母再定 target」的一次练习；B2 的 4%/700× 是 AP 语料读数，本地必须自测

### G9 distill 三态可观测（失败不再只活在日志里）
- **metric**：同一窗口内 `distill_log` 行数 ÷ `daemon.log` 里的 distill 尝试数；`status ∈ {ok, empty, failed}` 的分布 **（t85 勘误 D-4：迁移 `0024_distill_attempts.sql` 已把 `distill_log` 重建为「一次尝试一行」⇒ 行数**就是**尝试数，旧表「一 session 一行」的前提不存在；见文末 t85 勘误段）**
- **baseline**：**库 1 行 / 日志 22 次尝试**（21 失败 = 95.5%），差值 **21**（A4/A5，**同一窗口 09-26T04:52:45Z..09-27T13:48:48Z**）
- **target**：同窗口差值 **0**；每个失败行带 `failure_reason`；`empty`（抽到空数组）与 `failed`（agent 报错 / 会话未索引 / 超时）可区分；`failed` 行**绝不**创建 episode（mem-core D4 硬约束）
- **复现命令**：`Select-String $env:USERPROFILE\.ruagent\logs\daemon.log -Pattern "distill"` 与 `S`: `SELECT status,COUNT(*) FROM distill_log GROUP BY 1` **（t85 勘误 D-4：这条 SQL 在 0024 之后读的是**尝试**级行 —— 与 `daemon.log` 的尝试数应当**相等**；旧的「差值 21」只在 0024 之前可复现）**
- **标定读数**：21 个失败 key 现在**全部**在 `sessions` 里（A6）⇒ 根因是竞态而非数据缺失；`Query returned no rows` 来自 `distill.rs:256`。两个窗口不同这件事必须写在读数里（mem-core 2026-09-27 明确要求）

### G10 图证据真的进过注入（mem-core 要的第三层证据）
- **metric**：`transcripts/` 里 `context_injected` 事件中含 `<graph>` 块的条数
- **baseline**：**0**（closure §1 行 40，t259：79 个注入事件里含 knowledge/wiki/entity 块的 = 0）
- **target**：≥ **1** 个运行路径事件里出现 `<graph>`，且块内每条路径能回溯到 `edge_id`（`events` 与 `transcripts` 两侧一致）
- **复现命令**：`Select-String $env:USERPROFILE\.ruagent\transcripts\*.jsonl -Pattern "<graph>"`
- **标定读数**：`tag_rank` 对**未知** tag 返回 **6**（t85 勘误 D-3：此处原写 **5**、坐标原写 `:268`；今天 `crates/memory/src/inject.rs:326` 是 `fn tag_rank`、`:334` 是 `_ => 6`；详见文末 t85 勘误段）⇒ 不把 `graph` 加进词表的话，图块会被**第一个丢弃**。mem-core 2026-09-27 已同意把位置定在 `knowledge(2)` 与 `wiki(4)` 之间（graph=3），并标注这是**策略变更**（`wiki`/`project_context` 的 rank 各 +1）

---

## D 冻结接口

**冻结语义**：I-C 可以**加**字段、可以改私有实现与 SQL，**不可以**改下面已列出的名字/类型/语义；`neighbors` / `current_facts` / `facts_as_of` / `list_entities` / `list_edges` / `search_entities` / `upsert_entity` / `add_fact` / `delete_entity` **签名一律不动**（t250 已把 `search_entities` 冻成接口，`api.rs:4602` 的测试钉着它）。
位置：`crates/graph/src/lib.rs`。

```rust
/// 一次图检索的预算与时间视角。字段全部有 Default；调用方只覆盖它关心的。
/// 冻结常量：DEFAULT_HOPS=2、MAX_HOPS=3、DEFAULT_BEAM=8、DEFAULT_MAX_PATHS=12、
/// DEFAULT_MAX_FACTS=24、DAMPING=0.5。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct GraphQuery {
    /// 原始查询串（用户/任务文本）。种子解析在这一层完成，不外包给调用方。
    pub text: String,
    /// 回溯跳数，超过 MAX_HOPS 时按 MAX_HOPS 处理**并记 truncated_by=Hops**。
    pub hops: u32,
    /// 每一跳保留的 frontier 节点数上限。
    pub beam: u32,
    /// 返回路径条数上限。
    pub max_paths: u32,
    /// 返回边（事实）条数上限。
    pub max_facts: u32,
    /// 双时态视角：「当时为真」。None = 现在。RFC3339。
    pub as_of: Option<String>,
    /// 是否允许把已失效的边作为**历史**证据（默认 false）。
    /// true 时每条这样的边必须带 TemporalStatus::Superseded。
    pub include_superseded: bool,
}

/// 种子的来源判据：每个种子必须说出是哪条腿找到的。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SeedLeg {
    ExactName,     // norm_name == norm(query)
    NameToken,     // 查询与名字有 token / 子串重合（含别名表命中，见 AliasTable 优先）
    AliasTable,    // entity_aliases 命中
    SummaryFts,    // entities_fts 命中 summary
    VectorNearest, // 嵌入近邻；无嵌入器时**不出现**（不是 0，是不出现）
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SeedHit {
    pub entity: Entity,
    pub leg: SeedLeg,
    pub score: f32,
}

/// 一条边在某个时间视角下的状态。**必须显式区分「真事件时间」与「写入时刻」。**
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum TemporalStatus {
    Current,                                        // invalid_at IS NULL 且 valid_at 是抽取出来的
    RecordedAtOnly,                                 // valid_at 只是写入时刻（今天 66/67 的边都是这一态）
    TrueAsOf { valid_at: String, invalid_at: String }, // 给了 as_of 且当时为真、现在已失效
    Superseded { invalid_at: String },              // 已失效（仅 include_superseded=true）
}

/// 来源：抽取它的那次蒸馏。三段都可能是 None —— 「未知」是事实，不许回填。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct EdgeSource {
    pub episode: Option<i64>,      // entity_edges.source_episode（外键 episodes(id)，不许哨兵）
    pub session_key: Option<String>, // episodes.source_run
    pub recorded_at: String,       // entity_edges.created_at（T'：记录时间）
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct EvidenceEdge {
    pub edge_id: i64,
    pub src: i64,
    pub dst: i64,
    pub src_name: String,
    pub dst_name: String,
    pub relation: String,
    pub fact_text: String,
    pub valid_at: String,          // T：抽取到的事件时间；无则等于 recorded_at 且 temporal=RecordedAtOnly
    pub invalid_at: Option<String>,
    pub temporal: TemporalStatus,
    pub source: EdgeSource,
}

/// 「为什么留下这一条」的机械判据：V-C 必须能逐因子复算 score。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PathRationale {
    pub seed_leg: SeedLeg,
    /// score = factors[0] × Π factors[1..]；factors[0] = 种子分，其余逐边权重。
    pub factors: Vec<f32>,
}

/// 一条证据路径：hops+1 个节点、hops 条边，逐跳有序。
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct EvidencePath {
    pub hops: u32,
    pub score: f32,
    pub nodes: Vec<Entity>,        // nodes[0] 是种子
    pub edges: Vec<EvidenceEdge>,  // edges[i] 连接 nodes[i] → nodes[i+1]
    pub why: PathRationale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TruncationBudget { Hops, Beam, MaxPaths, MaxFacts }

/// 空结果必须说得出为什么空 —— 不许静默（本仓纪律 3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EmptyReason {
    NoSeed,             // 查询里没有任何可解析到图上对象的词
    NoEdgeFromSeeds,    // 种子存在但没有任何有效边（孤立实体，今天 20/63）
    AsOfBeforeAnyFact,  // as_of 早于所有 valid_at
    HopsCap,            // 到 MAX_HOPS 时 frontier 仍非空
    BeamCap,            // 有候选被 beam 截掉
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct RetrievalStats {
    pub seeds_by_leg: Vec<(SeedLeg, u32)>, // 每条腿的计数，0 也要出现
    pub frontier_sizes: Vec<u32>,          // 每跳之后的 frontier 大小
    pub paths_considered: u32,
    pub paths_emitted: u32,
    pub truncated_by: Option<TruncationBudget>,
    pub empty_reason: Option<EmptyReason>, // 非空时必须是 None，不许两个都为 Some
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct GraphEvidence {
    pub seeds: Vec<SeedHit>,
    pub paths: Vec<EvidencePath>,
    pub stats: RetrievalStats,
}

/// 冻结入口：**唯一**的多跳图检索入口。
pub async fn retrieve(db: &Db, q: &GraphQuery) -> Result<GraphEvidence, DbError>;

/// 种子解析单独可测（V-C 拿它量 G1 那条指标）。
pub async fn resolve_seeds(db: &Db, text: &str, limit: u32) -> Result<Vec<SeedHit>, DbError>;

/// 社区层（P1）。None = 还没建过社区 ⇒ 调用方必须显示「未建」，不是「空」。
pub async fn communities(db: &Db, level: u32) -> Result<Option<Vec<Community>>, DbError>;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Community {
    pub id: i64,
    pub level: u32,
    pub parent: Option<i64>,
    pub summary: Option<String>,
    pub entity_ids: Vec<i64>,
}
```

### D.1 打分与排序（冻结；V-C 逐因子复算）

```
score(path) = factors[0] × Π_i factors[1+i]
factors[0]              = 种子分（ExactName 1.0 > AliasTable 0.9 > NameToken 0.7 > SummaryFts 0.5 > VectorNearest = 该腿归一化分）
hop_weight(e)           = DAMPING × rel_idf(e.relation) × degree_norm(e)
DAMPING                 = 0.5
rel_idf(r)              = 1 / (1 + ln(1 + n_r))            n_r = 当前有效边中 relation = r 的条数
degree_norm(e)          = 1 / (1 + ln(1 + deg(src) + deg(dst)))   deg = 当前有效边的入+出度数
确定性 tie-break        = (score 降序, hops 升序, 节点 id 序列字典序升序)
```
理由：`rel_idf` 用**数据**而不是手写的「hub 关系清单」压掉 `uses`（现场 12/60）这类泛关系；`degree_norm` 压掉 `ruagent`（现场 33/60 = 55%）这类枢纽；二者都可从库里复算，因此可证伪。

### D.2 证据形状 → 注入（消费面契约）

一条 `EvidencePath` 渲染成**一行**，喂 `ruagent_memory::inject::ContextItem`：

```rust
ContextItem::dated(
    TAG_GRAPH,                       // 新常量，mem-core 2026-09-27 同意：rank 3（knowledge 与 wiki 之间）
    format!("{} →{}→ {} →{}→ {} [{}] (source: {})", /* 名字·关系·名字 逐跳; temporal 标签; episode/session */),
    /* 该路径 edges 中最大的 valid_at */,
)
```
硬要求：**路径 / 跳数 / 时序 / 来源四件事必须在同一行里可读**；`n0 -rel-> n1 -rel-> n2 [valid_at=…, state=…] (session=…)`。注入块的可读性向量 = `paths` 条数（≤ `max_paths`），不是 facts 条数（同一路径里的边不重复计）。

### D.3 冻结边界（不许顺手改的）

| 冻结物 | 原因 |
| --- | --- |
| `search_entities` 的名字/签名/语义（`lib.rs:309`） | t250 已冻；`api.rs:4602` 的测试与面板 strict 腿都依赖它 |
| `neighbors` / `current_facts` / `facts_as_of` / `list_entities` / `list_edges` | 面板图页面 4 个端点在用（A15），改语义=改面板读数 |
| `Entity` / `Edge` 的**已列字段** | `serde::Serialize` 直接出 JSON 给面板；加字段可以，改名/改类型不行 |
| `add_fact` 的参数个数与顺序 | `distill.rs:520` 与 `api.rs:1095/4690` 三处调用点；要加 `event_time_source` 请用新函数或新参数**尾部追加**并在规格里登记 |
| `upsert_entity` 的 `norm()` 判据 | 别名是**新表**，不是把 `norm()` 改聪明（改 `norm()` 会静默合并已有行） |

---

## E 实现清单

优先级：**P0** = 没有它本代目标不成立；**P1** = 目标需要但可后置；**P2** = 加分项。

| # | 项 | 优先 | inScope（谁的什么文件） | 需要的 schema 变更（→ I-SCHEMA） |
| --- | --- | --- | --- | --- |
| E1 | 种子解析：`resolve_seeds` 四腿（`ExactName`/`NameToken`/`AliasTable`/`SummaryFts`），strict 与 loose 的现有形状**保留**为其中两条 | P0 | `crates/graph/src/lib.rs`（graph，本人）；`crates/graph/tests/seed-resolution.rs`（新） | 无（`entity_aliases` 由 E4 提供；无别名表时 `AliasTable` 腿计数为 0 且必须出现在读数里） |
| E2 | `retrieve` 多跳 + `GraphEvidence`/`EvidencePath`/`RetrievalStats`（D 节签名与排序） | P0 | `crates/graph/src/lib.rs`；`crates/graph/tests/multihop.rs` + `multihop-gold.rs`（新） | 可选索引 `entity_edges(src, invalid_at)` 已有近似（`idx_edges_src`）；新增 `event_time_source` 列见 E3 |
| E3 | 时间语义：`event_time_source`、`TemporalStatus::RecordedAtOnly`、抽取 `valid_at` | P0 | `crates/graph/src/lib.rs`；`crates/daemon/src/distill.rs`（**与 mem-core 共管**，见 E9） | `ALTER TABLE entity_edges ADD COLUMN event_time_source TEXT NOT NULL DEFAULT 'recorded'`；`ADD COLUMN fact_hash TEXT` |
| E4 | 消解与别名：`entity_aliases` 表 + 解析时合并 + 未决队列 | P0 | `crates/graph/src/lib.rs`；`crates/graph/tests/resolution.rs`（新） | `CREATE TABLE entity_aliases(id, entity_id, alias, norm_alias UNIQUE, source, created_at)`；`CREATE TABLE resolution_pending(entity_a, entity_b, reason, created_at, PRIMARY KEY(entity_a,entity_b))` |
| E5 | 边的来源：`EdgeSource` 三段；`write_graph` 传 episode | P0 | `crates/graph/src/lib.rs`；`crates/daemon/src/distill.rs`（共管，E9） | 不需要（`source_episode` 列已在 `0005_graph.sql:34`）；可选 `entity_sources(entity_id, episode_id)` 记录**实体**的来源 |
| E6 | 抽取质量 gold + 三族评分脚本 | P0（阻断 G7/G1 剩余部分） | `docs/design/reviews/gold/**`（gold 数据 + `score.py`）；`crates/graph/tests/extraction-gold.rs` 只读 gold 做回归 | 无 |
| E7 | `distill_log` 三态（`ok`/`empty`/`failed`）+ `failure_reason` + **prompt 指纹** | P0 | `crates/daemon/src/distill.rs`（共管，E9） | `ALTER TABLE distill_log ADD COLUMN status TEXT NOT NULL DEFAULT 'ok'`；`ADD COLUMN failure_reason TEXT`；`ADD COLUMN prompt_hash TEXT` |
| E8 | 社区/主题摘要层（Leiden/Louvain 划分；摘要**延迟**生成，不在索引期全量调用 LLM） | P1 | `crates/graph/src/lib.rs`；`crates/graph/tests/communities.rs`（新） | `CREATE TABLE communities(id, level, parent_id, summary, built_at)`；`CREATE TABLE community_entities(community_id, entity_id, weight, PRIMARY KEY(community_id,entity_id))` |
| E9 | PPR 式传播排序（beam → damped propagation，`factors` 仍是同一条公式） | P2 | `crates/graph/src/lib.rs` | 无 |
| E10 | 向量种子腿（`SeedLeg::VectorNearest`） | P2 | `crates/graph/src/lib.rs`（**注意**：`entities` 今天没有嵌入列，需要 I-SCHEMA 加列或复用 knowledge 侧的嵌入器） | `ALTER TABLE entities ADD COLUMN embedding BLOB` / `ADD COLUMN embedder TEXT` |

### E.9 依赖登记：需要别人改的接线点（**不在**我的 inScope，写清给对应 owner）

| 依赖 | 文件（owner） | 需要的最小改动 | 依据 |
| --- | --- | --- | --- |
| DEP-1 | `crates/daemon/src/api.rs`（integ） | 召回处理器 `api.rs:2524` 从 `search_entities` 换成 `resolve_seeds` + `retrieve`（即 G1 的实际落地）；面板搜索端点保留 strict→loose 两段与 `match` 字段不动 | A12/A17：生产 strict **1/23**、loose **13/23** |
| DEP-2 | `crates/memory/src/inject.rs`（mem-core） | 新增 `TAG_GRAPH`；`tag_rank` 排 **3**（`knowledge(2)` 与 `wiki(4)` 之间），`wiki`/`project_context` 各 +1 | mem-core 2026-09-27 已同意；G10 的标定读数（未知 tag → 5，被首先丢弃） |
| DEP-3 | `crates/store/src/migrations/*.sql`（recall / I-SCHEMA） | E3/E4/E7/E8/E10 的列与表（上面 5 行）；`recall_log` 增 `graph_entities` / `graph_paths` 两列以承载 G1/G5 的生产读数 | A13 今天只能从 `entities` 一列读实体腿；多跳**没有任何生产读数位** |
| DEP-4 | `crates/daemon/src/distill.rs`（**graph 与 mem-core 共管**） | 见 E.10 下 5 条；每条都已发给 mem-core 并收到答复 | 任务单硬要求：「任何改变记忆写入行为的改动都必须先发消息给 mem-core 并登记」 |
| DEP-5 | `crates/knowledge/src/store.rs`（recall / I-A） | 只读：`fts::terms/match_all/match_any_prefix/like_patterns` 继续作为**唯一**分词判据被 graph 复用（`knowledge/src/store.rs:488` 已写着「so this crate and the graph crate cannot drift」）。**不新增**分词实现 | 本单 A12 的 loose 复刻直接照抄这 4 个函数，两者必须同源 |

### E.10 distill.rs 的行为变更登记（附发消息证据）

**证据**：`agent_teams_send_message → mem-core`，message id **`69cba3ba-835d-45bd-98ce-b929a23cb052`**（2026-09-27 本会话，主题「distill.rs 行为变更登记（按『任何改变记忆写入行为的改动必须先通知你』的纪律）」）。mem-core 已回复逐条决议：

| 编号 | 变更（`crates/daemon/src/distill.rs`） | mem-core 决议 | 我采纳的约束 |
| --- | --- | --- | --- |
| D1 | `write_graph` 的 `add_fact`（520-528）把 `source_episode` 从 `None` 改成共用 episode id | **同意** | (a) 必须是 `record_episode` 返回的真 id，不许哨兵（`-1` 已被 SQLite 787 拒，§7.137）；(b) `record_episode` 失败时保持 **NULL**，不回填 |
| D2 | `write_memories`（380-493）返回值加 `episode: Option<i64>`（**具名字段**，不是三元组），**不移动** episode 创建位置 | **同意**，含两条要求 | (a) 用 struct 而非元组；(b) 不许把 episode 创建挪到 `write_graph` 之后；(c) memories 为空时仍建 episode 且 `kind=run_turn` |
| D3 | `EXTRACTION_PROMPT`（16-36）加 `relations[].valid_at`、`entities[].aliases[]`、受控 `kind`/`relation` | **有条件同意**（三条闸门） | 闸门1 `memories[]` 那一段**逐字节不动**（新字段只加在 `relations[]`/`entities[]`）；闸门2 `valid_at` 未知必须 **null**，prompt 写明「绝不回填当前时刻」；闸门3 `distill_log` 记 **prompt 指纹**。若闸门3 落不了地 ⇒ 改走「第二次抽取调用」（+1 ACP 往返/会话） |
| D4 | 失败路径第一次写 `distill_log`（`status`/`failure_reason`）；`log_outcome`（216-237）扩列 | **同意**，并升为契约 | 失败行**绝不**创建 episode；面板「已蒸馏」布尔只允许来源于 `episodes.kind='run_turn'`（mem-core 的 `distilled = EXISTS(episode e JOIN memories m ON m.source_episode=e.id WHERE e.kind='run_turn')`） |
| D5 | 抽到空数组时写 `status='empty'`，与「被去重跳过」区分 | **同意** | `empty` 需与 D3 闸门3 的 prompt 指纹一起读，才能回答「是抽取变严了，还是会话真没内容」 |

**边界声明（写进规格，避免下游误读）**：D1/D2/D4/D5 都只动 `distill.rs` 与 `entity_edges`/`distill_log` 的写入，**不改 `memories` 行的语义**；D3 是本清单里唯一可能影响记忆产出的改动，因此它带着三条闸门。`builtin_extraction_prompt()`（`distill.rs:883（t85 勘误：原 582；锚 builtin_extraction_prompt 定义）`）被面板设置页**只读展示** ⇒ 改 prompt 等于改用户看到的文本，面板文案归 I-D/ integ 负责（不阻断）。

### E.11 与 I-A / I-B / I-D / I-SCHEMA 的边界（逐项：不冲突）

| 区域 | owner | 我方 inScope 路径 | 交集检查 |
| --- | --- | --- | --- |
| I-SCHEMA（`crates/store`：migrations、`fts.rs`、repo traits） | recall | 无（我不写 `crates/store`） | **不冲突**：我只**提需求**（DEP-3），落库由 I-SCHEMA 做；`fts.rs` 我只读复用（DEP-5） |
| I-A（`crates/knowledge`） | recall | 无 | **不冲突**：`knowledge` 不触碰 `entities`/`entity_edges`（`Select-String -Path crates\knowledge\src -Pattern "entities" -Pattern "graph"` 只命中注释与 `store.rs:488` 的同源说明） |
| I-B（`crates/memory`） | mem-core | 无 | **不冲突**：我只提 `TAG_GRAPH`（DEP-2）与 distill 的 episode 依赖（D1/D2）；`inject.rs` 的写者是 mem-core |
| I-D（`crates/daemon/wiki.rs`） | wiki | 无 | **不冲突**：`wiki.rs` 不引用 `ruagent_graph`（全仓 `ruagent_graph::` 的调用点只有 `api.rs` 与 `distill.rs`） |
| `crates/daemon/src/api.rs`（图页面 + 召回） | integ | 无 | **不冲突但需接线**：DEP-1；图页面端点我**不改**，召回接线由 integ 做 |
| `crates/daemon/src/distill.rs` | **graph 与 mem-core 共管** | 我改（E3/E5/E7） | 共管 ⇒ 每次改动前发消息、规格登记（E.10 已做） |
| `panel/` | 集成/UI | 无（本单 out of scope） | **不冲突**：面板图页的读数是另一个单的对象 |

---

## F 复现（附录）

### F.1 复现器（自包含、只读、无第三方依赖）

存成 `%TEMP%\graph-baseline.py`，`python %TEMP%\graph-baseline.py`。**只开 `?mode=ro`，不写任何表**。

```python
# graph-baseline.py — R-C (t3) 基线复现器。只读活库（mode=ro），不写任何东西。
# 用法: python graph-baseline.py            # 默认 ~/.ruagent/data/ruagent.db
#       python graph-baseline.py <db-path>
# 依赖: python3 标准库(sqlite3)。不需要 /api/v1/recall。
import sys, os, re, json, sqlite3, collections
from datetime import datetime

DB = sys.argv[1] if len(sys.argv) > 1 else os.path.expanduser("~/.ruagent/data/ruagent.db")
URI = "file:" + DB.replace("\\", "/") + "?mode=ro"
con = sqlite3.connect(URI, uri=True)
c = con.cursor()
ONE = lambda q, a=(): c.execute(q, a).fetchone()[0]
OUT = collections.OrderedDict()

def section(name):
    OUT[name] = collections.OrderedDict()
    print("\n== " + name + " ==")
    return OUT[name]

# ---------- A1 计数 ----------
s = section("A1 counts")
s["entities"] = ONE("SELECT COUNT(*) FROM entities")
s["edges_total"] = ONE("SELECT COUNT(*) FROM entity_edges")
s["edges_current"] = ONE("SELECT COUNT(*) FROM entity_edges WHERE invalid_at IS NULL")
s["edges_invalidated"] = ONE("SELECT COUNT(*) FROM entity_edges WHERE invalid_at IS NOT NULL")
s["edges_with_source_episode"] = ONE("SELECT COUNT(*) FROM entity_edges WHERE source_episode IS NOT NULL")
s["edges_dangling"] = ONE("SELECT COUNT(*) FROM entity_edges e LEFT JOIN entities x ON x.id=e.src LEFT JOIN entities y ON y.id=e.dst WHERE x.id IS NULL OR y.id IS NULL")
s["edges_self_loop"] = ONE("SELECT COUNT(*) FROM entity_edges WHERE src=dst")
s["entities_with_kind"] = ONE("SELECT COUNT(*) FROM entities WHERE COALESCE(kind,'')<>''")
s["entities_with_summary"] = ONE("SELECT COUNT(*) FROM entities WHERE COALESCE(summary,'')<>''")
s["orphan_entities"] = ONE("SELECT COUNT(*) FROM entities e WHERE NOT EXISTS (SELECT 1 FROM entity_edges x WHERE x.src=e.id OR x.dst=e.id)")
s["memories_total"] = ONE("SELECT COUNT(*) FROM memories")
s["memories_with_episode"] = ONE("SELECT COUNT(*) FROM memories WHERE source_episode IS NOT NULL")
s["episodes"] = ONE("SELECT COUNT(*) FROM episodes")
s["sessions_indexed"] = ONE("SELECT COUNT(*) FROM sessions")
s["distill_log_rows"] = ONE("SELECT COUNT(*) FROM distill_log")
s["distill_log_zero_output"] = ONE("SELECT COUNT(*) FROM distill_log WHERE memories_written=0 AND entities_written=0 AND relations_written=0")
s["distill_log_zero_entities"] = ONE("SELECT COUNT(*) FROM distill_log WHERE entities_written=0")
s["distill_log_window"] = ONE("SELECT MIN(distilled_at)||' .. '||MAX(distilled_at) FROM distill_log")
s["recall_log_rows"] = ONE("SELECT COUNT(*) FROM recall_log")
s["recall_log_distinct_queries"] = ONE("SELECT COUNT(DISTINCT query) FROM recall_log")
s["recall_log_entity_nonempty"] = ONE("SELECT SUM(CASE WHEN entities>0 THEN 1 ELSE 0 END) FROM recall_log")
s["recall_log_window"] = ONE("SELECT MIN(ts)||' .. '||MAX(ts) FROM recall_log")
print(json.dumps(OUT["A1 counts"], ensure_ascii=False, indent=1))

# ---------- A2 关系词表 ----------
s = section("A2 relation vocabulary")
s["distinct_relations"] = ONE("SELECT COUNT(DISTINCT relation) FROM entity_edges")
s["singleton_relations"] = ONE("SELECT COUNT(*) FROM (SELECT relation FROM entity_edges GROUP BY relation HAVING COUNT(*)=1)")
s["top_relations"] = c.execute("SELECT relation, COUNT(*) FROM entity_edges WHERE invalid_at IS NULL GROUP BY 1 ORDER BY 2 DESC LIMIT 5").fetchall()
print(s["distinct_relations"], "distinct /", s["singleton_relations"], "singletons", s["top_relations"])

# ---------- A3 时序 ----------
s = section("A3 temporal")
def ts(x):
    if x is None: return None
    try: return datetime.fromisoformat(re.sub(r"(\+\d{2}:\d{2})$", "", x).replace("Z", "+00:00"))
    except Exception: return None
d = []
for v, cr in c.execute("SELECT valid_at, created_at FROM entity_edges"):
    a, b = ts(v), ts(cr)
    if a and b: d.append(abs((a - b).total_seconds()))
d.sort()
s["n"] = len(d)
s["delta_le_1s"] = sum(1 for x in d if x <= 1)
s["delta_gt_60s"] = sum(1 for x in d if x > 60)
s["delta_max"] = d[-1] if d else None
s["invalid_eq_expired"] = ONE("SELECT SUM(CASE WHEN invalid_at=expired_at THEN 1 ELSE 0 END) FROM entity_edges WHERE invalid_at IS NOT NULL")
print(s)

# ---------- A4 消解 ----------
s = section("A4 alias/resolution")
ents = c.execute("SELECT id,name,norm_name FROM entities ORDER BY id").fetchall()
def base(n):
    return re.sub(r"\s*[\(（][^)）]*[\)）]\s*$", "", n.strip().lower()).strip()
g = collections.defaultdict(list)
for i, n, nn in ents: g[base(n)].append((i, n))
grouped = {k: v for k, v in g.items() if len(v) > 1}
keys = list(g.keys())
subset = [(a, b) for a in keys for b in keys if a != b and len(a) >= 3 and set(a.split()) <= set(b.split()) and len(a) < len(b)]
s["entities"] = len(ents)
s["same_object_groups_after_parenthesis_strip"] = {k: v for k, v in grouped.items()}
s["token_subset_pairs"] = subset
s["redundant_entities"] = sum(len(v) - 1 for v in grouped.values()) + len(subset)
print("groups:", list(grouped), "subset pairs:", subset, "redundant:", s["redundant_entities"])

# ---------- A5 拓扑 / 可达 ----------
s = section("A5 topology")
adj = collections.defaultdict(set)
for src, dst in c.execute("SELECT src,dst FROM entity_edges WHERE invalid_at IS NULL"):
    adj[src].add(dst); adj[dst].add(src)
ids = [r[0] for r in c.execute("SELECT id FROM entities")]
def bfs(start, hops):
    dist = {start: 0}; fr = [start]
    for h in range(1, hops + 1):
        nxt = []
        for x in fr:
            for y in adj[x]:
                if y not in dist: dist[y] = h; nxt.append(y)
        fr = nxt
        if not fr: break
    return dist
for h in (1, 2, 3):
    sizes = sorted(len(bfs(i, h)) - 1 for i in ids)
    s[f"reach_hops{h}_mean"] = round(sum(sizes) / len(sizes), 2)
    s[f"reach_hops{h}_median"] = sizes[len(sizes) // 2]
seen, comps = set(), []
for i in ids:
    if i in seen: continue
    st = [i]; comp = []; seen.add(i)
    while st:
        x = st.pop(); comp.append(x)
        for y in adj[x]:
            if y not in seen: seen.add(y); st.append(y)
    comps.append(comp)
comps.sort(key=len, reverse=True)
s["components"] = len(comps)
s["component_sizes"] = [len(x) for x in comps[:6]]
big = comps[0]
cov = sum(1 for i in big for j in big if i != j and bfs(i, 3).get(j, 99) <= 3)
s["pairs_within_3_hops"] = f"{cov}/{len(big)*(len(big)-1)}"
s["max_incident_edges"] = ONE("""SELECT MAX(d) FROM (SELECT e.id,
      (SELECT COUNT(*) FROM entity_edges x WHERE (x.src=e.id OR x.dst=e.id) AND x.invalid_at IS NULL) d FROM entities e)""")
s["max_distinct_neighbors"] = max((len(adj[i]) for i in ids), default=0)
s["max_degree_entity"] = c.execute("""SELECT id,name FROM entities e ORDER BY
      (SELECT COUNT(*) FROM entity_edges x WHERE (x.src=e.id OR x.dst=e.id) AND x.invalid_at IS NULL) DESC LIMIT 1""").fetchone()
s["hub_edge_share"] = round(100.0 * s["max_incident_edges"] / max(1, ONE("SELECT COUNT(*) FROM entity_edges WHERE invalid_at IS NULL")), 1)
print("components:", s["components"], s["component_sizes"], "3-hop pair coverage:", s["pairs_within_3_hops"],
      "| max incident edges:", s["max_incident_edges"], "max distinct neighbors:", s["max_distinct_neighbors"],
      "hub:", s["max_degree_entity"], s["hub_edge_share"], "% of current edges")

# ---------- A6 实体腿：strict / loose / 文本上界 ----------
MIN_RECALL_ASCII = 3
def terms(q):
    res, cur = [], ""
    for ch in q:
        if ch.isalnum(): cur += ch
        else:
            if cur: res.append(cur.lower())
            cur = ""
    if cur: res.append(cur.lower())
    o = []
    for t in res:
        if t not in o: o.append(t)
    return o
def recallable(t): return not (len(t) < MIN_RECALL_ASCII and t.isascii())
def qq(t): return '"' + t.replace('"', '""') + '"'
def fts(expr, limit=10):
    if not expr: return []
    try:
        c.execute("SELECT e.id FROM entities_fts f JOIN entities e ON e.id=f.rowid WHERE entities_fts MATCH ? ORDER BY rank LIMIT ?", (expr, limit))
        return [r[0] for r in c.fetchall()]
    except sqlite3.OperationalError:
        return []
def strict_ids(q, limit=10):
    return fts(" ".join(qq(t.replace('"', '""')) for t in q.split()), limit)
def loose_ids(q, limit=10):
    t = terms(q)
    if not t: return []
    out, seen = [], set()
    for i in fts(" ".join(qq(x) for x in t)):
        if i not in seen: seen.add(i); out.append(i)
    if not out:
        for i in fts(" OR ".join(qq(x) + "*" for x in t if recallable(x))):
            if i not in seen: seen.add(i); out.append(i)
    for x in t:
        if not recallable(x) or len(out) >= limit: continue
        pat = "%" + x.replace("\\", "\\\\").replace("%", "\\%").replace("_", "\\_") + "%"
        c.execute("SELECT id FROM entities WHERE name LIKE ? ESCAPE '\\' OR summary LIKE ? ESCAPE '\\' ORDER BY id LIMIT ?", (pat, pat, limit))
        for (i,) in c.fetchall():
            if i not in seen: seen.add(i); out.append(i)
    return out
def textual_seeds(query, minlen=2):
    def runs(s2):
        res, cur = [], ""
        for ch in s2:
            if ch.isalnum(): cur += ch
            else:
                if len(cur) >= minlen: res.append(cur.lower())
                cur = ""
        if len(cur) >= minlen: res.append(cur.lower())
        return res
    qr = runs(query)
    hay_stack = [(r[0], r[1], (r[1] or "") + " " + (r[2] or ""))
                 for r in c.execute("SELECT id, name, COALESCE(summary,'') FROM entities")]
    return [i for i, n, hay in hay_stack
            if any(r in hay.lower() for r in qr) or any(r in query.lower() for r in runs(n))]

s = section("A6 entity leg on the real query frame")
s["frame"] = "recall_log 的全部不同查询（采样面 = 用户真实查询，不是构造查询）"
qs = [r[0] for r in c.execute("SELECT query, COUNT(*) FROM recall_log GROUP BY query ORDER BY COUNT(*) DESC").fetchall()]
names = [(r[0], r[1]) for r in c.execute("SELECT id,name FROM entities ORDER BY id")]
self_strict = sum(1 for i, n in names if i in strict_ids(n))
s["self_query_strict"] = f"{self_strict}/{len(names)}"
s["frame_size"] = len(qs)
s["strict_nonempty"] = sum(1 for q in qs if strict_ids(q))
s["loose_nonempty"] = sum(1 for q in qs if loose_ids(q))
s["textual_overlap_nonempty"] = sum(1 for q in qs if textual_seeds(q))
s["per_query"] = [(q, len(strict_ids(q)), len(loose_ids(q)), len(textual_seeds(q))) for q in qs]
print("self-query strict:", s["self_query_strict"], "| frame:", s["frame_size"],
      "strict nonempty:", s["strict_nonempty"], "loose:", s["loose_nonempty"], "textual:", s["textual_overlap_nonempty"])
for row in s["per_query"]: print("   strict=%-3d loose=%-3d textual=%-3d %r" % (row[1], row[2], row[3], row[0]))

# ---------- A7 召回实体腿的生产读数（recall_log） ----------
s = section("A7 recall_log entity leg")
s["rows"] = ONE("SELECT COUNT(*) FROM recall_log")
s["entity_nonempty_rows"] = ONE("SELECT SUM(CASE WHEN entities>0 THEN 1 ELSE 0 END) FROM recall_log")
s["per_day"] = c.execute("SELECT substr(ts,1,10), COUNT(*), SUM(CASE WHEN entities>0 THEN 1 ELSE 0 END) FROM recall_log GROUP BY 1 ORDER BY 1").fetchall()
s["last_row"] = c.execute("SELECT id, ts, query, memories, knowledge, wiki, entities, source FROM recall_log ORDER BY id DESC LIMIT 1").fetchone()
print("rows:", s["rows"], "entities>0:", s["entity_nonempty_rows"], "last:", s["last_row"])
print("per day:", s["per_day"])

con.close()
print("\n[OK] read-only; nothing written.")
```

**A6 段的现场输出（2026-09-27T13:58Z）**：

```
self-query strict: 63/63 | frame: 23 strict nonempty: 1 loose: 13 textual: 13
   strict=0   loose=0   textual=0   'kettle'
   strict=0   loose=1   textual=1   'autohotkey-v2'
   strict=0   loose=0   textual=0   'kettle material'
   strict=10  loose=10  textual=14  'ruagent'
   strict=0   loose=1   textual=1   'autohotkey 改键'
   strict=0   loose=0   textual=0   '部署流水线'
   strict=0   loose=0   textual=0   'wiki'
   strict=0   loose=0   textual=0   'deploy'
   strict=0   loose=1   textual=7   't99 control perturbation delete me'
   strict=0   loose=2   textual=2   'postgres 连接池'
   strict=0   loose=1   textual=1   '项目'
   strict=0   loose=0   textual=0   '轮询 自动通知 用户偏好'
   strict=0   loose=6   textual=6   '记忆'
   strict=0   loose=2   textual=2   '知识库'
   strict=0   loose=0   textual=0   '中文交流 简洁 直接 表达习惯'
   strict=0   loose=10  textual=14  'ruagent 本地优先 多智能体平台'
   strict=0   loose=10  textual=15  'ruagent memory'
   strict=0   loose=0   textual=0   'panel'
   strict=0   loose=0   textual=0   'deploy pipeline'
   strict=0   loose=6   textual=6   'Windows 11 运行环境 操作系统'
   strict=0   loose=2   textual=2   'T97C generating 任务'
   strict=0   loose=0   textual=0   'T97B rollback fail'
   strict=0   loose=2   textual=2   'T97 MARK TWO doctor probe kettle chrome'
```

### F.2 HTTP 只读复现（`GET` only；**不得**用 `/api/v1/recall`）

```powershell
$b = "http://127.0.0.1:8787"
(Invoke-WebRequest "$b/api/v1/health" -UseBasicParsing).Content
(Invoke-WebRequest "$b/api/v1/graph/entities?limit=100" -UseBasicParsing).Content.Length
(Invoke-WebRequest "$b/api/v1/graph/edges?limit=5" -UseBasicParsing).Content      # total=60
(Invoke-WebRequest "$b/api/v1/graph/search?q=ruagent" -UseBasicParsing).Content   # match=exact, 10 条（多为 summary 命中）
(Invoke-WebRequest "$b/api/v1/graph/search?q=autohotkey-v2" -UseBasicParsing).Content  # match=candidate, 1 条
(Invoke-WebRequest "$b/api/v1/graph/search?q=kettle" -UseBasicParsing).Content    # match=none, 0 条
(Invoke-WebRequest "$b/api/v1/graph/entity/1/neighbors?hops=2" -UseBasicParsing).Content
(Invoke-WebRequest "$b/api/v1/graph/entity/1/neighbors?hops=4" -UseBasicParsing).Content
(Invoke-WebRequest "$b/api/v1/graph/entity/1/facts" -UseBasicParsing).Content     # source_episode 全 null
```

### F.3 单元测试基线

```powershell
$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"
$env:CARGO_TARGET_DIR="$env:TEMP\ruagent-rc"     # 绝不用 D:/rust_cache（真 daemon 占着那里的 ruagent.exe）
cargo test -p ruagent-graph
```

---

## G 未测 / 不可判定清单（不许静默跳过）

| # | 项 | 状态 | 原因 / 需要什么 |
| --- | --- | --- | --- |
| G-1 | distill 的 `run_turn` episode 路径是否可用 | **不可判定** | A3：活库 `run_turn`=0，但 156 条记忆指向 t347 回填的 `legacy` manual episode，且盘上有 t229 清理前的备份 ⇒ 活库无法回答。**需要**：临时 root 新库上跑一次 distill（C·G3 的判据） |
| G-2 | distill 的「LLM 抽取失败」与「真没内容」的区分 | **未测** | `distill_log` 没有 status 列（`0008:9-16`），成功路径才写行；日志只区分 WARN/INFO。**需要** C·G9 的 schema |
| G-3 | 抽取 precision/recall（人工 gold） | **未测** | 无 gold 数据。**需要** 20 会话人工标注（C·G7）；在此之前任何抽取质量声明不可证伪 |
| G-4 | 多跳检索的 gold path-recall | **未测** | 需要 gold 路径集（与 G-3 同一份标注可复用）。**需要** C·G5 |
| G-5 | 嵌入种子腿（`VectorNearest`） | **未测** | `entities` 表**没有** embedding 列（`0005_graph.sql` 只有 name/kind/summary）；本单未启用任何嵌入器 |
| G-6 | 20 个孤立实体的来源（抽取出来的，还是面板/CLI 手建的） | **未测** | 需要把 `entities.created_at` 与 `distill_log.distilled_at` 对窗；本单只量了「有 20 个」（A10），没有量来源。**不写进规格**，只登记 |
| G-7 | 面板 `/graph` 页面的读数（实体表、画布、检查器） | **out of scope** | 本单 out of scope 含 `panel/`；且本单的判据是库与 HTTP，不是 DOM |
| G-8 | 社区摘要的质量 | **未测** | 社区层尚不存在（A.18 ③）；C·G8 的判据要等 P1 |
| G-9 | `entity_edges` 的历史行是否该回溯补 `source_episode` | **决定：不回溯** | 理由：`source_episode` 的语义是「指向派生它的 episode」，历史行的 episode 已在 t229 清理里消失 ⇒ 补任何值都是编造（§7.77 同族）。因此 C·G3 的判据明确写「只对新写入的边」 |
| G-10 | 本单没有跑全 workspace 的 `clippy`/`fmt` | **有意跳过** | 本单只产出一个 markdown 文件，不改任何 Rust 代码；全 workspace 的 fmt/clippy 是同伴在途编辑的对象（纪律 10）。`cargo test -p ruagent-graph` 已现场跑过（A16） |

---

## 附录 H 勘误（t27 · 2026-09-28 追加，**只追加不删原文**）

RV-C（`docs/design/reviews/gen2-graph-review.md` §4 RVC-6(b)）指出：本规格 §C·G7 与 §E6 点名的 gold 交付面 **`docs/design/reviews/gold/`**（含 `score.py`）在仓库里**不存在**，全仓也没有 `score.py`；I-C 实际把 gold 落在 **`crates/graph/tests/gold/`**（6 个 JSON），并用 `cargo test` 而不是 python 脚本复现。上面 §C·G7 的复现命令因此**不可执行**。

**裁决与落点（两个真相源只能选一个，t27 选「规格勘误」这条路，不补 `score.py`）**：

1. **gold 的真实位置**：`crates/graph/tests/gold/` —— `live_graph_snapshot.json`（活库快照，63 实体 / 67 边）、`edges.json`（60 条 current 边的人工三值标注）、`ontology.json`（35 个关系名）、`resolution.json`（24 对实体）、`multihop.json`（20 组可达对 + 5 组越界负例）、`as_of.json`（6 条时点查询）。
2. **复现方式**：不是 `python …/score.py`，而是**测试本身**（读 gold 并把读数打印出来；gold 覆盖率由测试守护：新增未标注的边/关系名会让测试变红）：
   ```powershell
   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-graph --test extraction-gold -Nocapture
   ```
   改前的读数命令与三条 gold 族读数见 `docs/design/reviews/gen2-graph-impl.md` §1 G7；改后（写入侧去重 + 非关系名拒收）见 `docs/design/reviews/gen2-graph-repair.md`。
3. **不落地 `score.py` 的理由**：一个 python 评分器会成为 gold 判据的**第二个实现**（且它读的字段、算的 precision 口径都可能与 `extraction-gold.rs` 分叉）。本代的原则是配置与判据单一真相源；用测试替代脚本后，「gold 与判据同源」由编译器保证，而不是靠两处人肉同步。若后续要做跨实现评分（例如与 Text2KGBench 的协议对接），应新增一个**读同一份 gold JSON** 的工具，并把本附录再勘误一次，而不是并行维护。
4. **对象集与目标的对应关系（RVC-6(a)，与 RV-C 一致）**：§C·G7 的 target 写「**20 个 gold 会话**上实体 P ≥0.85 / R ≥0.70」，而现有 gold 的三族是**边 / 关系名 / 实体对**，**没有实体族**，也没有会话语料 —— 所以**实体族目标在现有 gold 上没有对应物**：已在 t27 报告里登记为 `not_measured`（原因=无 gold、无会话语料），**不判过也不判败**，并列为下一代数据待办（需要 20 段手工标注的会话，而不是活库快照）。

### H.2 §C 三行复现命令的勘误（G3/G4/G8，t38 追加，**只追加不删原文**）

RV-C2-8 指出：§C 里还有三条复现命令指向**不存在**的测试 target。三条都保留原文（见上面的删除线），此处给出**真实存在**的 target 与**实跑读数**（本节的每条命令都在 2026-09-28 的这棵树上跑过，exit 0）：

| 目标 | 规格原文命令 | 为什么不能用 | 真实 target（`crates/graph/tests/`） | 实跑读数 |
| --- | --- | --- | --- | --- |
| **G3** 每条新边带来源 | `--test provenance-fresh-db`（L109） | 全树 **0 命中**（该文件不存在） | `--test temporal`（`the_old_writer_leaves_the_source_unknown_and_the_new_one_records_it`：旧写入器留 NULL、新写入器记 `event_time_source`）+ `--test extraction-gold`（边/本体族） | `test the_old_writer_leaves_the_source_unknown_and_the_new_one_records_it ... ok`，5 passed / 0 failed |
| **G4** 时间语义 + 时点查询 | `--test as-of-gold`（L116） | 全树 **0 命中** | `--test temporal`（`as_of_gold_returns_what_was_true_then` + t27 的三写法仪器 + `TrueAsOf` 断言） | `as_of ruagent at … -> MATCH` **6/6**（8/8、17/17、9/9、17/17、10/10、17/17 条自有边，`orphan edges []`），5 passed / 0 failed |
| **G8** 社区层 | `--test communities`（L144） | 文件不存在（`communities` 只命中函数/符号名） | `--test extraction-gold`（社区结构：分区数、`covered == non_isolated`、幂等、摘要惰性 + t38 新增的两条可证伪指标） | `level 0: communities=9 covered=43 non_isolated=43 split_by_modularity=1`；`partition: 9 communities, sizes [14, 5, 3, 3, 2, 6, 2, 4, 4]`；`summaries written: 1 of 9` |

**为什么只改指针、不改测试**（RV-C2-8 的 requiredFix 就是这句）：这三个能力**都已经有测试**，只是名字与规格写的不一致；新增三个空壳 target 会让「测试数量」变成装饰。真实命令一律是 `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-graph --test <真实名>`。

### H.3 四条不可测目标的显式 `deferred` 裁决（t38 追加）

**裁决来源与性质**：captain 2026-09-28 裁决 —— 这四条按**可测量性**显式 `deferred`（closure §7.138 的形状），**不是判据放宽**。**它们仍然是「未达成」**：`deferred` 表示「本代不具备测量它的数据/跑批条件」，**不得**被下游读成达标或通过。逐条按 captain 要求的四要素登记：

| # | 目标原文位置 | 为什么本代测不了（证据） | 下一代的前置 | owner 角色 |
| --- | --- | --- | --- | --- |
| 1 | §C·G7 target「20 个 gold 会话上 实体 P ≥0.85 / R ≥0.70」 | 现有 gold 三族是边/关系名/实体对，**没有实体族**、**没有会话语料**（`crates/graph/tests/gold/` 六个 JSON 无实体族字段）；活库快照不能替代手工标注会话 | **人工标注**的工程实体族 gold（与召回侧「人工分级 gold」同类数据前置） | 人工标注（需具名成员/任务；角色=数据标注） |
| 2 | §C·G7 target「关系 R ≥0.60；幻觉率 ≤0.10」 | 活库 **0/67** 历史边有 `source_episode` ⇒ 无可回溯 transcript 可比对；幻觉率的**机制**半边已在 fixture 上被测（含「分不清改写与发明」的已知假阳性，见 `gen2-graph-repair.md` §5） | **一次真实重蒸馏跑批**，产出带 episode 的边集与可回溯语料 | 跑批（角色=重蒸馏 + 标注） |
| 3 | §C·G4 target「`extracted` 比例 ≥30%（活库）」 | 需一次真实重蒸馏；且 30% 的下限**挂在第 1、2 条的 gold 会话构成上**（规格原文自己这么写） | 同第 1、2 条；**在这之前不判 30%** | 同第 2 条（并入 G7 的数据待办） |
| 4 | §C·G8 target「三指标成对判决 ≥60%」 | 需要 LLM 成对判决**跑批**（协议照 B2）；本代没有该 harness，也没有带标签的对照组 | 评测 harness + 对照组（无社区层 vs 有社区层） | 评测（角色=跑批） |

**替代性收获（不是这四条的替代，而是它们的机制半边）**：G8 的结构侧在本代**已经可判**并已升级为**前置条件** + 两条**可被证伪**的新指标（H.3 第 4 条 / §C·G8），所以「社区层有没有结构」这件事没有停在 `deferred` 里；G7 的关系 P 也已在本代达标关闭。**deferred 剩下的恰是没有数据/没有跑批的那部分。**

---

## t85 坐标勘误（2026-09-29）

| 行 | 旧引用（逐字） | 新引用 | 锚词 / 依据 |
| --- | --- | --- | --- |
| L46 | `distill.rs:256` | `distill.rs:497` | `load_messages` 定义行 |
| L394 | `distill.rs:582` | `distill.rs:883` | `builtin_extraction_prompt` 定义行 |

**同批的语义勘误（不是坐标）见下两节**：D-3（`tag_rank` 未知 tag 5→6）与 D-4（G9 相对迁移 0024）。

**未收口（SUSPECT/UNVERIFIED）**：`api.rs:776/784/1070/1095/1161/2524/4602`、`distill.rs:256/520`、`store.rs:488`、`inject.rs:268`、`lib.rs:309`（**缺 crate 名**）。其中 `api.rs:2524` 现在指的是**已被 DEP-1 改变**的召回路径（t80 D-7），owner：graph 规格属主。

### t85 勘误 D-3：`tag_rank` 对未知 tag 返回 **6**，不是 5（2026-09-29）

**原文（逐字保留）**：`- **标定读数**：\`tag_rank\` 对**未知** tag 返回 **5**（\`crates/memory/src/inject.rs:268\`）⇒ 不把 \`graph\` 加进词表的话，图块会被**第一个丢弃**。`

**更正**：`tag_rank` 的兜底分支今天是 `crates/memory/src/inject.rs:334 _ => 6`（`profile=0 memories=1 knowledge=2 graph=3 wiki=4 project=5 **unknown=6**`；`gen2-integration-contract.md` §3.2 的读数也是 6）。**方向不变**（未登记 tag 会被**最后**丢弃），但按 5 去推 `graph` 的插入位置会算错一格。旧坐标 `inject.rs:268` 也已漂（定义在 `:326`）。

### t85 勘误 D-4：G9 的判据落后于迁移 0024（2026-09-29）

**原文（逐字保留）**：
`- **metric**：同一窗口内 \`distill_log\` 行数 ÷ \`daemon.log\` 里的 distill 尝试数；\`status ∈ {ok, empty, failed}\` 的分布`
`- **复现命令**：… \`SELECT status,COUNT(*) FROM distill_log GROUP BY 1\``

**更正**：仓库现在有 **25** 个迁移（最大 `0025_query_eval_gold_unique.sql`），其中 **`crates/store/src/migrations/0024_distill_attempts.sql`** 把 `distill_log` **重建**为「**一次尝试一行**」：`CREATE TABLE distill_log_attempts (…)` → `ALTER TABLE distill_log_attempts RENAME TO distill_log` → `CREATE INDEX idx_distill_log_session ON distill_log(session_key, id)` → `CREATE INDEX … status`。因此在 **0024 之后**：
* 「同一窗口内 `distill_log` **行数** ÷ `daemon.log` 尝试数」这个比值不再度量「一次 session 记了几次尝试」——行数现在**就是**尝试数（旧读法之所以能看出「库 1 行 / 日志 22 次」，是因为旧表按 `session_key` 一行一 session，**那个前提已被 0024 删除**）；
* 旧 baseline「**库 1 行 / 日志 22 次尝试**（差值 21）」只在 **0024 之前**的库上可复现；在 0024 之后的库上，正确的读数形式是「`distill_log` 行数 == `daemon.log` 尝试数（差值 0）」+ `status` 分布 + `failure_reason`；
* **判据本身（差值 0 / `empty` 与 `failed` 可区分 / `failed` 不建 episode）不变**，变的是它**所依据的数据模型**。

**交回 finding（不在本单改）**：`gen2-integration-contract.md` §3.1 的落地表止于 `0023`，还没有 0024 的形状变更行 —— owner：contract（t64）。
