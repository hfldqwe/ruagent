# gen2 集成契约（R-E / t5，integ）— 四区域消费面的单一契约

| 项 | 值 |
| --- | --- |
| 任务 | t5（requirements round 1），成员 `integ`，attempt 1，`attempt_id 7f0c98cb-eb0e-4b2f-bd3d-5418ed8ed81f` |
| 产物 | 本文件（**规格单：零代码改动**，`git status --porcelain -- crates panel` 为空） |
| owner | integ（集成与消费面唯一写者：`crates/daemon/src/{api,config,lib,chat,runs,sessions}.rs`、`crates/daemon/tests/`、`crates/mcp/`、`panel/`、`crates/orchestrator/`、`crates/core/`） |
| 输入（四份，逐份全文读完） | `docs/design/reviews/gen2-recall-spec.md` · `docs/design/reviews/gen2-memory-spec.md` · `docs/design/reviews/gen2-graph-spec.md` · `docs/design/reviews/gen2-wiki-spec.md` |
| 供谁读 | **INT（t19）实现** · **V-INT（t20）独立验证** · **RV-INT（t21）判决** |
| 引用读数的时间基准 | 四份规格自报：`gen2-recall-spec` 2026-09-27T21:28:58→22:23+08:00 · `gen2-memory-spec` 2026-09-27T21:28:58→22:37+08:00 · `gen2-graph-spec` 2026-09-27T13:58Z（本地 21:58+08:00）· `gen2-wiki-spec` 2026-09-27 21:28:55→22:47+08:00。**凡引旧值一律带时间点与来源**；活库在自变（`entities` 50→63、`recall_log` 650→651，同一天内），因此本契约的每条现场读数都写窗口 |
| 本单的取证纪律 | 只读核对代码与四份规格（`read`/`grep`/`glob`）；**未调用 `GET /api/v1/recall`**；未启停真守护进程 **pid 79984 @ 127.0.0.1:8787**；未写活库 |

**本契约的定位**：把四份区域规格的**消费面**（HTTP / MCP / 注入遥测 / 面板）收敛成**一份**可实现的接口表，冻结四份规格之间的矛盾与它们都没提、但接线时必须定的接口。**它不重新定义任何区域的算法**；凡某个能力属于某个区域，契约只写「由该区域提供库接口，INT 只做接线」。

**标注词表**

| 标注 | 含义 |
| --- | --- |
| `[冻结]` | 名字 / 类型 / 语义 / 线上字面量不许改；只许加 |
| `[新增]` | 只加不改；旧消费方零改动 |
| `[破坏]` | 会改变现有消费方（编译、行为或 agent 可见字节），必须显式登记 + 给迁移 |
| `[HANDOFF]` | 该改动**不在 INT 的 inScope**，必须由 captain 改派或由对应 owner 落地；INT 只接线 |

---

## 1 HTTP 路由表

**读法**：只列**四区域的消费面**；`/agents`、`/runtimes`、`/tasks`、`/runs`、`/chat`、`/sessions`、`/permissions`、`/skills`、`/mcp` 中与四区域无关的路由**不在本表**（本代零改动）。「消费的区域接口」一列写的是**库接口**，不是另一个 HTTP 端点；`I-A`=crates/knowledge（recall/t7）、`I-B`=crates/memory + daemon/memembed.rs（mem-core/t8，见 K-6）、`I-C`=crates/graph + daemon/distill.rs（graph/t9）、`I-D`=daemon/wiki.rs（wiki/t10）、`I-SCHEMA`=crates/store（recall/t6）。

### 1.1 记忆（memory）

| Method | Path | 请求 schema | 响应 schema | 消费的区域接口 | 只读/写 | 本次变更 |
| --- | --- | --- | --- | --- | --- | --- |
| GET | `/api/v1/memory/list` | query `store?`（`profile\|observation\|procedure\|lesson`，未知 ⇒ **400 + 有效值**）`namespace?`（省略=不过滤）`limit?`（默认 500，夹 1..5000） | `{memories:[MemoryRow+distilled:bool], total, matched, limit, store, namespace, counts}` | I-B `query::{all_memories,count_memories,store_counts}` + `api.rs::distilled_ids` | 只读 | `[冻结]` 形状；`distilled` 派生规则冻结为**只由** `episodes.kind='run_turn'`（C-INT-2），禁止按 `distill_log` 反查 |
| GET | `/api/v1/memory/search` | query `q`（必填）`limit?`=8 `legs?`（**共用 SearchQuery，对 memory 无意义**） | `{hits:[MemoryRow+distilled]}` | I-B `query::search_fts` | 只读 | `[新增]` `hits[i].confidence` 语义不变（A.3：live 8 个值、min 0.5、`<0.5`=0/157） |
| POST | `/api/v1/memory/write` | `{store, namespace, content, supersedes?:i64, confidence?:f64}` | `{outcome:"Inserted(id)"\|"Superseded{..}"\|…}` | I-B `write_memory` + `episode::record_episode` + `memembed::embed_row` | **写** | `[破坏]` **BREAK-MEM-1**：未知 `store` ⇒ **400 + 有效值列表**（今天静默回落 `Observation`，`api.rs:3705-3710`）。**顺序硬约束**：400 必须发生在 `record_episode` **之前**，否则「被拒绝的写入」仍留下 episode 行（负例锚点 A-11） |
| POST | `/api/v1/memory/write` | 同上 + `confidence?` | 同上 | I-B `crates/memory/src/confidence.rs`（C1） | **写** | `[新增]` `confidence` 透传：缺省时由 I-B 的**具名常量**决定（`CONF_UNCONFIRMED`），**不再在 api.rs 里写字面量 0.9**；范围 (0,1] 外 ⇒ 400。这解决 `gen2-memory-spec` A.3 的 D-1（三条写路径都让 `<0.5` 不可达） |
| POST | `/api/v1/memory/supersede` | `{id:i64, new_content:String}` | `{outcome:…}` | I-B `lifecycle::supersede` | 写 | 无（`memory_diffs.op=supersede` 已在用） |
| GET | `/api/v1/memory/{id}` | path `id`（不可解析 ⇒ 400；不存在 ⇒ **404**） | `{memory:MemoryRow}`（消费方取 `.memory`） | I-B `query::get` | 只读 | 无 |
| DELETE | `/api/v1/memory/{id}` | query `purge?`（true=真删，缺省=软删 tombstone） | `{outcome:…}` | I-B `lifecycle::{delete_memory,purge_memory}` | 写 | 无（`gen2-memory-spec` C5 的判据是「purge 后三面为 0」，端点已具备） |
| POST | `/api/v1/memory/{id}/restore` | path `id` | `{outcome:…}` | I-B `lifecycle::restore` | 写 | 无 |
| GET | `/api/v1/memory/diffs` | query `limit?` | `{diffs:[{id,memory_id,op,reason,at}]}` | I-B `memory_diffs` 读 | 只读 | `[新增]` **op 词表扩到 9 值**：`insert\|supersede\|skip_dedupe\|reject\|delete\|restore\|purge` **+`merge_judged`+`consolidate`**（`gen2-memory-spec` B-9）。消费方必须「未知 op 原样显示、不崩」 |
| POST | `/api/v1/memory/backfill-embeddings` | 空 body | `{…计数}` | I-B `memembed::embed_row` 批量 | 写 | 无 |
| POST | `/api/v1/memory/migrate-distilled-prefix` | 空 body | `{…计数}` | I-B/distill 一次性迁移 | 写 | 无（历史：它造了 156 行 `manual` episode，见 A.5） |
| GET | **`/api/v1/forget-report`** `[新增]` | query **二选一**：`content_hash`(64hex) 或 `memory_id`；`limit?`=50(1..200)；`exact_total?=false` | `{content_hash, local:[LocalResidual], external:[ExternalResidual], read_at}`（`gen2-memory-spec` §C5-D v2） | I-B `lifecycle::forget_report`（本 crate 三面）+ **I-A `residual_scan(needle,limit,exact_total)`** + **I-D 四个 wiki 载体**的只读腿 | **只读**（只报不删） | `[新增]` **本契约回答 `gen2-memory-spec` U-6（「要不要一个统一入口」）：要，且只有一个。** 语义冻结：`None ≠ 0`（`total:Option`）；`truncated` 判据必须是 `limit+1` 多取一行；哈希载体不得计为内容残余；报告里**永不出现被遗忘的文本** |

### 1.2 召回 + 遥测（recall）

| Method | Path | 请求 schema | 响应 schema | 消费的区域接口 | 只读/写 | 本次变更 |
| --- | --- | --- | --- | --- | --- | --- |
| GET | `/api/v1/recall` | query `q`（必填）`strategy?`=`aggressive\|conservative` `top_n?`=5（夹 1..20）`min_score?`=0.0 `source?`（`[a-z0-9_-]{1,32}`，非法 ⇒ 400；亦可走 header `x-ruagent-recall-source`） | `{strategy, source, memories:[…], memory_legs:{…}, knowledge:[…], wiki:[…], entities:[…], graph:{…}}` | I-B `memembed::recall_memories` + I-A `search_page`（新，一次拿腿+量纲）+ I-C `resolve_seeds`/`retrieve` + I-D `recall_stubs`/`leader` | **写（仅 `recall_log` 一行）** | `[破坏/策略]` **BREAK-REC-1**：`leg_k = limit.max(10)` 改为**恒定** `LEG_WINDOW=60`（`gen2-recall-spec` C5/B-2）⇒ 历史 `top_knowledge_score` **不可比**，必须同时写 `knowledge_leg_window`/`scoring_version`。`[新增]` 响应加 `knowledge[i].*_kind`/`relevance`/`fusion`/`leg_window`/`candidates`、顶层 `graph`、`memory_legs.leg_window`。`[新增]` 写库时多写 §3.1 的新列 |
| GET | `/api/v1/recall/log` | query `limit?`=50(1..200) `source?`（`unknown`=`source IS NULL` 的历史行） | `{rows:[{ts,query,strategy,top_n,memories,knowledge,wiki,entities,top_memory_score,top_knowledge_score,source,source_label, score_kind, knowledge_leg_window, scoring_version, fusion, top_knowledge_relevance, top_knowledge_relevance_kind, top_legs_json, selected_json, rejected_json, graph_entities, graph_paths}], retention:{policy,max_rows:5000,rows,min_ts,max_ts,unknown_source}}` | I-B/I-A 产出的遥测；`recall_log` 列由 I-SCHEMA 建 | 只读 | `[新增]` 新列**一律透出**；历史行这些列 = `null`（不是 0，不给历史行背书） |
| GET | `/api/v1/distill` | 无 | `{auto,agent,language,prompt,graph,builtin_prompt,prompt_hash}` | I-D? **不**；`distill.rs`（I-C）生效的 prompt + INT 的 `config::DistillEditor` | 只读 | `[新增]` **`prompt_hash`**（见 F-6）：`sha256(生效 prompt 文本)`，`null` 当且仅当无法确定生效值 |
| PUT | `/api/v1/distill` | `{auto:bool, agent?:String, language?:String, prompt?:String, graph?:bool\|null}` | `{ok:true, prompt_hash}` | 同上 | 写 | `[新增]` 响应加 `prompt_hash`；`agent` 未知/禁用仍 400（现状，保留） |

### 1.3 知识库（knowledge，I-A 面）

| Method | Path | 请求 schema | 响应 schema | 消费的区域接口 | 只读/写 | 本次变更 |
| --- | --- | --- | --- | --- | --- | --- |
| GET | `/api/v1/knowledge/search` | query `q`（必填）`limit?`=8 **夹 1..50（新）** `legs?`=true | `{hits:[KnowledgeHit], total, matched}`；`KnowledgeHit = {kind:"knowledge", chunk_id, document, score, score_kind:"rrf_rank", legs:[…], semantic_rank, semantic_score, keyword_rank, keyword_score, query_keyword_stage, content?, excerpt?, hint?, relevance?, relevance_kind?, semantic_score_kind?, keyword_score_kind?, fusion?, leg_window?, candidates?}` | I-A `Knowledge::search_page`（新）；旧 `search`+`search_legs` 两次调用删除 | 只读 | `[破坏/边界]` **BREAK-KB-1**：`limit` 夹到 1..50（今天无上界）。`[新增]` **加法键**（不嵌套、不改旧键名）：每个分数旁挂 `*_kind`，并给 `relevance`；`query_keyword_stage` 新增取值 `"bigram"`（K-9）。`[冻结]` `score` 的语义**永不改变**（仍是 RRF 名次分，`"rrf_rank"` 是字面量，`gen2-memory-spec` D.7） |
| POST | `/api/v1/knowledge/ingest` | `{name:String, content:String}` | `{chunks:i64, …}` | I-A `Knowledge::index_doc` | 写 | 无 |
| GET | `/api/v1/knowledge/documents` | 无 | `{documents:[…]}` | I-A `Knowledge::list_documents` | 只读 | 无 |
| GET | `/api/v1/knowledge/documents/{id}` | path `id` | `{chunks:[[id,content],…]}` | I-A `Knowledge::document_chunks` | 只读 | 无（**wiki 的锚校验就用它**，`gen2-wiki-spec` E.3） |
| DELETE | `/api/v1/knowledge/documents/{id}` | path `id`（不存在 ⇒ 404） | `{…}` | I-A `Knowledge::delete_document` | 写 | 无 |
| GET | `/api/v1/knowledge/raw/{*name}` | wildcard 路径 = 树内路径 | `text/plain`（**原始字节**；时间戳读数一律取原始字节，`gen2-wiki-spec` A.3 的仪器教训） | I-A `Knowledge::read_raw` | 只读 | 无 |
| PUT | `/api/v1/knowledge/raw/{*name}` | `{content:String}` | `{chunks:i64}` | I-A `Knowledge::write_raw` | 写 | 无 |
| POST | `/api/v1/knowledge/rebuild` | 空 | `{…scan 计数}` | I-A `Knowledge::scan` | 写 | 无 |
| PATCH | `/api/v1/knowledge/chunks/{id}` | `{content}` | `{…}` | I-A `Knowledge::edit_chunk` | 写 | 无 |
| GET | `/api/v1/knowledge/chunks/{id}/revisions` | path `id` | `{revisions:[…]}` | I-A `chunk_revisions` | 只读 | 无 |
| POST | `/api/v1/knowledge/revisions/{id}/rollback` | path `id` | `{…}` | I-A `rollback_revision` | 写 | 无 |
| GET | `/api/v1/knowledge/expand/{chunk_id}` | path `chunk_id` | `{document,section,chunk}` | I-A `parents_for` + chunk | 只读 | 无；**它是 wiki 锚的取回路径**，`anchor.retrieve` 的契约就是它（`gen2-wiki-spec` D.7） |

### 1.4 Wiki（I-D 面）

| Method | Path | 请求 schema | 响应 schema | 消费的区域接口 | 只读/写 | 本次变更 |
| --- | --- | --- | --- | --- | --- | --- |
| GET | `/api/v1/knowledge/wiki/pages` | 无 | `{pages:[WikiPageInfo]}`；新增 10 键：`citations:usize` `cite_coverage:f32` `uncited_sections:[String]` `stale_since?:String` `freshness:"fresh"\|"stale"\|"unknown"` `stale_sources:[String]` `built_at?:String` `links_out_broken:usize` `self_links:usize` `frozen_by?:String` | I-D `wiki::pages`（**单一新鲜度计算点** `freshness()`；度数取自 `link_graph`） | 只读 | `[破坏]` **BREAK-WIKI-1**：`links_out` 语义收窄（含断链、**不含自链**）⇒ 现场 `cooking-pasta` 3→2；`crates/daemon/tests/knowledge_api.rs:478` 的逐字段断言由 **INT** 改（K-8）。`[新增]` 其余 9 键 + `freshness` 绝不省略 |
| GET | `/api/v1/knowledge/wiki/links` | 无 | `{nodes,edges,broken,orphans, wanted:[{slug,demanders:[String],demand_count}], unreachable:[String], self_links:[WikiEdge], degrees:[{slug,links_in,links_out,links_out_broken}], readings_at}` | I-D `wiki::links` → 唯一纯函数 `link_graph` | 只读 | `[新增]` 5 键；**不变量**：`broken` 与 `wanted.slug` 逐位相等、`orphans ⊆ unreachable`、`pages[i].links_out == degrees[i].links_out` |
| GET | `/api/v1/knowledge/wiki/builds` | query `limit?` | `{builds:[WikiBuild]}` | I-D `wiki::builds` | 只读 | `[新增]` **线上 `status` 词表** = `planned\|planned_only\|running\|done\|failed`；`planned_only` 是**过渡值**（I-D 的 E3 落地前仍在产出，`wiki.rs:1838`），迁移 `0022` 已把 2/8 行 legacy `planned_only` 回填成 `planned`，并加配对触发器（K-7）。**消费规则**：按派生布尔判 plan-only（`dry_run===true`），**禁止**只按 `'planned_only'` 字面量过滤 —— 新行在 I-D(t10) 改 `DRY_RUN_STATUS` 前仍写 `planned_only` |
| GET | `/api/v1/knowledge/wiki/builds/{id}` | path `id`（不存在 ⇒ 404） | `{build:{…,plan:[WikiPagePlan]?}, pages:[{slug,action,status,error}]}` | I-D `wiki::build_get` | 只读 | 无（`build/<id>` 的唯一取回路径，见 `gen2-memory-spec` §C5-D 的 key 约定表） |
| POST | `/api/v1/knowledge/wiki/build` | `{scope?:"all"\|"changed"\|[name], dry_run?:bool, agent?:String, confirm_plan?:i64}` | `{build_id,status,agent,pages_planned,plan?,notes?}` | I-D `wiki::start_build` | 写 | `[破坏]` **BREAK-WIKI-2**：`{dry_run:true, confirm_plan:N}` **必须 400**（今天静默忽略、还建了一行，`gen2-wiki-spec` D1c）；只读计划不再写 `pending` 页行（G7c 22/41→0） |
| GET | **`/api/v1/knowledge/wiki/pages/{slug}/corrections`** `[新增]` | path `slug` | `{corrections:[{slug,kind,reason,author,at}], active?:{…}}` | I-D `wiki::{corrections,active_correction}` | 只读 | `[新增]` 纠错回路（G6）的审计面 |
| POST | **`/api/v1/knowledge/wiki/pages/{slug}/corrections`** `[新增]` | `{kind:"pin"\|"release"\|"note", reason:String, author:String}`（`reason`/`author` 空串 ⇒ **400**） | `{id:i64, …}` | I-D `wiki::add_correction` | 写 | `[新增]` **「记下为什么改」的强制点**；秒级落地：冻结页构建必须 `skipped` 且 `error` 含 `frozen: <reason>` |

### 1.5 知识图谱（I-C 面）

| Method | Path | 请求 schema | 响应 schema | 消费的区域接口 | 只读/写 | 本次变更 |
| --- | --- | --- | --- | --- | --- | --- |
| GET | `/api/v1/graph/entities` | query `limit?` | `{entities:[[Entity,degree]]}` | I-C `list_entities` | 只读 | 无 |
| GET | `/api/v1/graph/edges` | query `limit?` | `{edges:[Edge], total}` | I-C `list_edges` | 只读 | 无 |
| GET | `/api/v1/graph/search` | query `q` | `{entities:[Entity], match:"exact"\|"candidate"\|"none"}` | I-C `search_entities` →（空时）`search_entities_loose` | 只读 | `[冻结]` **面板搜索端点的两段式与 `match` 字段保持不动**（`gen2-graph-spec` DEP-1 明文）；`match` 必须是三态字符串，空查询是 `"none"` 不是回退 |
| POST | `/api/v1/graph/entity` | `{name, kind?, summary?}` | `{id}` | I-C `upsert_entity` | 写 | 无 |
| — | **§1.5 读法注（t53，2026-09-28）**：**创建是 `POST /api/v1/graph/entity`（单数）**（路由表 `api.rs:53`）；**没有** `POST /api/v1/graph/entities`（复数只有 `GET`，`api.rs:49` ⇒ 复数 `POST` 是 405）。t53 按要求复核：**这条「契约写错了」与「`degrees` 是 map」两条声称都不成立** —— 本行与 `/graph/entities` 那行本来就写对了，`wiki/links` 行的 `degrees` 原文就是**对象数组**。写在这里是为了**让下一位不再重复这件事** |
| DELETE | `/api/v1/graph/entity/{id}` | path `id`（不存在 ⇒ 404） | `{outcome:"deleted",id,edges_removed,facts_removed}` | I-C `delete_entity` | 写 | 无 |
| POST | `/api/v1/graph/fact` | `{src,dst,relation,fact_text?, source_episode?, …}` | `{id}` | I-C `add_fact` | 写 | `[新增]` 承载 `source_episode` + `event_time_source`（G3/G4）；未知/缺失来源 ⇒ **`null`，不许回填 `now()`、不许哨兵** |
| GET | `/api/v1/graph/entity/{id}` | path `id` | `{facts:[Edge]}` | I-C `current_facts` | 只读 | `[新增]` 每条 fact 加 `temporal`/`source{episode,session_key,recorded_at}`（D 节冻结形状） |
| GET | `/api/v1/graph/entity/{id}/neighbors` | query `hops?`（默认 2，上限 4） | `{neighbors,hops}` | I-C `neighbors` | 只读 | `[冻结]` 语义与上限不动（面板 4 个端点在用） |
| GET | `/api/v1/graph/entity/{id}/facts` | query `at?` | `{facts:[Edge]}` | I-C `facts_as_of` | 只读 | `[新增]` 返回的边必须带 `TemporalStatus` 四态之一 |
| GET | **`/api/v1/graph/retrieve`** `[新增]` | query `q`（必填）`hops?`=2（≤3）`beam?`=8 `as_of?`(RFC3339) `include_superseded?`=false `max_paths?`=12 `max_facts?`=24 | `{seeds:[SeedHit], paths:[EvidencePath], stats:{seeds_by_leg,frontier_sizes,paths_considered,paths_emitted,truncated_by,empty_reason}}` | I-C `retrieve`（`retrieve.rs:633`，**唯一多跳入口**） | 只读 | `[新增]`（captain 2026-09-28 转达 graph 的 DEP-4 形状）把 `retrieve` 暴露到消费面：这是 G1/G5 在**生产**上的读数位，也是面板 Graph 页与 `recall.graph` 段的数据源；与 DEP-1 同入口。**接线门禁：已解除（captain 2026-09-28，graph 的 t27 completed）** —— `as_of` 可以接线，但**必须按 t27 的形状交读数**：① **3 条「同一瞬间、三种写法（规范形 / `Z` / `+08:00`）结果逐字节一致」的读数**；② `as_of` **必须在入口归一化**；③ 非时刻字符串必须**报错**（400），不许静默按字符串比较。**另**：**`RetrievalStats.graph_edges` 不是「那个时点的边数」**（它与 `as_of` 无关，每次都是 60）⇒ 不许把它当时间切片读数（RV-C 的提醒）。`GET /graph/entity/{id}/facts?at=` 今天已存在，接线时同样要归一化输入，并按 t27 的三种写法给一致性读数 |
| GET | **`/api/v1/graph/communities`** `[新增]` | query `level?`=0 | `{communities:[Community]\|null, built:bool, coverage?:{entities_covered,non_isolated}}`；`Community = {id,level,parent,summary,entity_ids}` | I-C `communities(db, level) -> Option<Vec<Community>>`（`community.rs:271`）+ `community_coverage`（`:328`） | 只读 | `[新增]` **必须能区分「`None` = 从未建过」与「空数组」**：`communities:null`（或 `built:false`）⇒ 消费面显示「未建社区」，**不许**显示「0 个社区」（`gen2-graph-spec` D 节：`None` ⇒ 调用方必须显示「未建」，不是「空」） |
| POST | **`/api/v1/graph/communities/build`** `[新增]` | body `{level?:u32}`=0（或 query `level?`；两者都给且不一致 ⇒ 400） | `{level, communities, entities_covered, non_isolated, split_by_modularity}`（`CommunityBuild`，`community.rs:47`） | I-C `build_communities(db, level)`（`community.rs:172`） | **写**（重建 `communities`/`community_entities` 的本层分区） | `[新增]` 一次构建改写本层分区 ⇒ **必须回一份可证伪的计数**（`entities_covered`/`non_isolated` 是 G8 的分母来源）；不允许 200 空体 |
| PUT | **`/api/v1/graph/community/{id}/summary`** `[新增]` | path `id:i64`；body `{summary:String}`（空串 ⇒ 400） | `{updated:bool, id, summary}` | I-C `set_community_summary(db, id, &str) -> bool`（`community.rs:314`） | **写**（只改摘要文本，不动分区） | `[新增]` `updated=false`（id 不存在）⇒ **404**，不许 200 假装写了 |
| GET | **`/api/v1/graph/resolution/pending`** `[新增]` | 无 | `{pending:[{entity_a,entity_b,reason}], redundant:[{entity_a,entity_b,detail}]}` | I-C `pending_pairs`（`lib.rs:882`）+ `redundant_pairs`（`lib.rs:898`） | 只读 | `[新增]` 两条读数分开报：`pending` = 未决队列（已记录「我们没有合并它们」），`redundant` = 判据说同一对象但仍是两行（G6 的分子）。**合并前必须能看到它们** |
| POST | **`/api/v1/graph/resolution/merge`** `[新增]` | body `{keeper:i64, absorbed:i64}`（相等 ⇒ 400；任一不存在 ⇒ 404） | `{moved:i64, keeper, absorbed}` | I-C `merge_entities(db, keeper, absorbed) -> u32`（`lib.rs:927`） | **写**（边改指 + 被吸收名变别名 + 删被吸收行） | `[新增]` **显式、人工/agent 决策，绝不自动合并**（`lib.rs:921-926` 的注释即判据）；回应里给 `moved`，并**必须**同一次写下 `entity_aliases`（否则「下次扫描又把它建回来」） |

### 1.6 破坏性变更与迁移（逐条）

| ID | 变更 | 类型 | 谁会被打断 | 迁移路径 |
| --- | --- | --- | --- | --- |
| **BREAK-REC-1** | 腿窗口 `limit.max(10)` → `LEG_WINDOW=60` | 行为（排序会变） | 所有把 `top_knowledge_score` 画在同一曲线上的人；面板 recall log 列 | ① 一次改到位，**不许留两个窗口**；② 同一次改动写 `knowledge_leg_window=60` + `scoring_version`；③ 历史 651 行这 4+13 列全 `null`；④ 跨这次改动**禁止**新旧分数同图 |
| **BREAK-REC-2** | 每条命中挂 `*_kind`、加 `relevance`、加 `graph` 段 | 加法（消费方需容忍新键） | 未知键的严格反序列化 | 消费方按「未知 kind ⇒ 不渲染该数」处理；`"rrf_rank"` 字面量不动 |
| **BREAK-KB-1** | `/knowledge/search` 的 `limit` 夹 1..50 | 边界（今天无上界） | 传 `limit>50` 的调用方 | 与 `LEG_WINDOW=60` 配套，使 C5（分页不改排序）在 API 可达面上成立；返回体不报错，只截断到 50 并在 `total` 保持真实 |
| **BREAK-KB-2** | `KeywordStage` 加 `"bigram"` | 加值 | 穷尽 `match` 的消费方（**已核：全仓无**） | `api.rs` 用 `format!("{stage:?}").to_lowercase()` ⇒ 自动；面板/MCP 必须「未知值原样显示」 |
| **BREAK-MEM-1** | `memory_write` 未知 `store` ⇒ 400（HTTP + MCP） | 破坏（方向：更诚实） | 传过 `store=bogus` 的 MCP 客户端（以前静默成功） | 复用 `api.rs::parse_store`（`memory_list` 已是 400 的先例）；**400 必须在 `record_episode` 之前** |
| **BREAK-MEM-2** | `memory_diffs.op` 加 `merge_judged`/`consolidate` | 加值 | op 白名单 | 面板按未知 op 显示原字符串；导出/测试白名单同步 |
| **BREAK-WIKI-1** | `WikiPageInfo.links_out` 收窄（不含自链） | 行为（读数变） | `crates/daemon/tests/knowledge_api.rs:478`（逐字段钉住） | INT 改该测试（K-8）；现场 `cooking-pasta` 3→2、`self_links=1` |
| **BREAK-WIKI-2** | `{dry_run:true, confirm_plan:N}` ⇒ 400 | 行为（更诚实） | 面板「重放计划」按钮（今天它靠 200 拿新 build_id） | 面板改用「读 plan → confirm 后再独立发起构建」两条请求，不再合并两个语义 |
| **BREAK-WIKI-3** | `wiki_builds.status` 词表从 4 套收成 1 套（`planned_only` → `planned`） | 数据迁移 | 读 `status` 字面量的消费方（面板 244 行 `b.dry_run ? "(dry)"`） | 迁移 `0022_wiki_gen2.sql`（I-SCHEMA）回填 + 配对触发器（K-7）；过渡期两个值并存 ⇒ 面板**按 `dry_run===true` 判 plan-only**（不许按 `'planned_only'` 字面量过滤：2/8 行 legacy 已被回填成 `planned`，新行仍写 `planned_only`），`dry_run` 由「兼容字段」升为**判据本身** |
| **BREAK-GRAPH-1** | `recall.entities` 的**来源**从 `search_entities`（strict）换成 `resolve_seeds` | 行为（返回更多/更准） | 面板 recall log 的 `ent n` 读数含义 | `recall_log.entities` 保持「本节返回的实体行数」；新增 `graph_entities`（种子数）/`graph_paths`（路径数）承载多跳读数（K-2） |
| **BREAK-CORE-1** | `RunEvent::ContextInjected` 加可选 `budget`/`path` 字段 | 编译（构造点）/ JSON（向后兼容） | `runs.rs:1270`（INT ✔）、`crates/acp/src/chat.rs:497`（**已并入 t19 inScope**，见 DEP-INT-1） | 字段 `#[serde(default)]` + `Option`；旧 JSONL 读得进、旧行的 `budget=null` |

---

## 2 MCP 工具表

**读法**：`crates/mcp/src/lib.rs` 今天有 9 个工具（memory_search / memory_write / memory_recall / memory_get / knowledge_search / knowledge_ingest / knowledge_expand / graph_entity / list_tasks）。本表是**本代冻结的闭集（共 14 个）**：新增 **5 个只读工具**（`memory_forget_report` / `graph_search` / `graph_retrieve` / `wiki_pages` / `wiki_links`），**不新增任何写工具**。

| Tool | 入参 schema | 暴露的能力 | 写库 | 审计来源字段（写工具必填） | 本次变更 |
| --- | --- | --- | --- | --- | --- |
| `memory_search` | `{query:String, limit?:u32}` | memory | 否 | — | 无 |
| `memory_write` | `{store:String, namespace:String, content:String, supersedes?:i64, confidence?:f64}` | memory | **是**（`memories` 行 + `episodes` 行） | `episodes.kind='mcp_write'` + `memories.source_episode`（**非空**，`record_episode` 失败时如实 `null`）；`source` 声明为 `mcp` | `[新增]` `confidence?` 透传；`[破坏]` 未知 `store` ⇒ **工具错误**（4xx 原样透出，`error_for_status` 已具备）；`supersedes` 保持 |
| `memory_recall` | `{query:String, conservative?:bool, top_n?:u32}` | recall（memory+kb+wiki+graph） | **是（仅遥测）**：写 `recall_log` 一行 | **必须声明 `source=mcp_recall`**（今天不声明 ⇒ `source IS NULL`，读数无法归因） | `[新增]` 传 `source`；返回文本新增逐腿量纲标签、`wiki` 行的 `stale/coverage/anchors`（来自响应新键）、`graph` 路径行 |
| `memory_get` | `{id:i64}` | memory | 否 | — | 无 |
| **`memory_forget_report`** `[新增]` | `{content_hash:String}` | memory（残余面清点） | 否（只报不删） | — | `[新增]` 对接 §1.1 的 `/forget-report`；回答 `gen2-memory-spec` U-6 |
| `knowledge_search` | `{query:String, limit?:u32}` | knowledge | 否 | — | `[新增]` 输出带 `*_kind` 与逐腿名次；`limit` 同 BREAK-KB-1 |
| `knowledge_ingest` | `{name:String, content:String}` | knowledge | **是**（`documents` + `chunks` + FTS） | `documents.source` 非空（`raw/<name>`）；`chunk_revisions` 记改动 | 无 |
| `knowledge_expand` | `{chunk_id:i64}` | knowledge | 否 | — | 无（wiki 锚的取回腿） |
| `graph_entity` | `{id:i64}` | graph | 否 | — | `[新增]` 输出每条 fact 的 `temporal`/`source`（与 HTTP 面同形） |
| **`graph_search`** `[新增]` | `{q:String}` | graph | 否 | — | `[新增]` 对接 `/graph/search`，输出带 `match` 三态 |
| **`graph_retrieve`** `[新增]` | `{q:String, hops?:u32, as_of?:String, include_superseded?:bool}` | graph（多跳） | 否 | — | `[新增]` 对接 `/graph/retrieve`；`stats.empty_reason`/`truncated_by` **必须出现在输出里**（空结果不许静默）；`as_of` 与 HTTP 面同一条归一化规则（§1.5）；输出**不得**把 `stats.graph_edges` 说成「那个时点的边数」 |
| **`wiki_pages`** `[新增]` | `{stale_only?:bool}` | wiki | 否 | — | `[新增]` 输出 `freshness`（三态）/`cite_coverage`/`citations`/`stale_since`/`frozen_by` |
| **`wiki_links`** `[新增]` | `{}` | wiki | 否 | — | `[新增]` 输出 `wanted`（带需求方）与 `orphans`/`unreachable` 分开报 |
| `list_tasks` | `{status?:String}` | platform ops | 否 | — | 无 |

**冻结规则（本契约新增，见 F-5/F-6）**

1. **每个写工具必须能被审计归因到「谁在什么时候写了什么」**：`memory_write` 靠 `episodes(kind='mcp_write')` + `source_episode`；`knowledge_ingest` 靠 `documents.source` + `chunk_revisions`；`memory_recall` 虽不改记忆，但**改遥测**，靠 `recall_log.source='mcp_recall'`。
2. **本代不新增写工具的裁决与理由**：① wiki 的 `pin/release/note` 是**人的**纠正动作（`gen2-wiki-spec` B6：真正风险是「静默」，不是权限），agent 一 pin 就静默冻结一页 ⇒ 只开 HTTP/面板，不开 MCP；② wiki 构建（`wiki_build`）会写生成页，规格没有要求 agent 触发它，开它等于把 `gen2-memory-spec` B.3 拒绝的「agent 自我编辑」搬进 wiki；③ 图写入同理：`graph_fact` 今天**不是** MCP 工具，本代不补。
3. **4xx 纪律**：所有工具必须把 daemon 的 4xx **原样**变成工具错误（现状均走 `error_for_status`，不得吞成 200）；`memory_write` 的未知 `store` 是第一个会真正吃到 4xx 的写工具。
4. **MCP `source` 词表**：`mcp_recall`（唯一需要声明者）；`crates/mcp/src/lib.rs` 里的字面量是**唯一**来源，不许在调用点散写。

---

## 3 遥测字段表

**两个载体**：① `recall_log`（SQLite，一行 = 一次召回调用）；② `context_injected`（transcripts JSONL，一个事件 = 一次注入）。**它们描述的是两件事**，本契约不把它们合并（把预算混进 `recall_log` 会让「这次召回花了多少上下文」变成假读数的来源）。

### 3.1 `recall_log`（列名 / 类型 / 语义 / 写入点）

| 列 | 类型 | 语义 | 谁提供 | 历史行 |
| --- | --- | --- | --- | --- |
| `id` `ts` `query` `strategy` `top_n` | 既有 | 一行 = 一次调用；`query` 由写入侧截到 200 字符 | INT 写（`api.rs:2766`） | 有值 |
| `memories` `knowledge` `wiki` `entities` | 既有 INTEGER | **本节实际返回的行数**（不是候选数） | INT 写 | 有值 |
| `top_memory_score` | 既有 REAL | 记忆腿的**原始余弦** top-1（≥0.25/0.30 过滤前） | I-B `mem_legs.top_semantic_score` | 有值 |
| `top_knowledge_score` | 既有 REAL | 知识腿的**融合分 top-1**；`score_kind="rrf_rank"`，上界 2/61 ≈ 0.0328（现场 651 行 5 个取值，全部逐位等于名次和） | I-A / INT 写 | 有值 |
| `source` | 既有 TEXT（0018） | **谁声明了这次调用**；`NULL` = 未声明（历史） | INT `declared_source` | 有值/NULL |
| **`score_kind`** `[新增]` | TEXT | `top_knowledge_score` 的量纲字面量，**恒 `"rrf_rank"`**（K-4） | INT 写常量 | **NULL** |
| **`knowledge_leg_window`** `[新增]` | INTEGER | 这次调用的腿窗口；`LEG_WINDOW=60`。**历史行为 NULL ⇒ 与今天不可比** | I-A `SearchEvidence.leg_window` | **NULL** |
| **`scoring_version`** `[新增]` | INTEGER | 打分/窗口/模型的版本号：权重、窗口、embedder 任一变化 ⇒ +1；与 `knowledge_meta.embedder` 联读 | INT 常量 + I-A | **NULL** |
| **`fusion`** `[新增]` | TEXT | 融合式，例 `"rrf(k=60,w_sem=2,w_kw=1)"` | I-A `FusionKind` | **NULL** |
| **`top_knowledge_relevance`** / **`…_kind`** `[新增]` | REAL / TEXT | 标定相关分及其量纲（`"calibrated"`）；**只许展示/排序，禁止当 `min_score` 闸门**（C3） | I-A `RelevanceScore` | **NULL** |
| **`top_legs_json`** `[新增]` | TEXT | **逐腿名次与原始分**（针对 top-1 融合命中）：`[{"leg":"semantic","rank":1,"raw_score":0.1244,"kind":"semantic_l2sq"},{"leg":"keyword","rank":3,"raw_score":-4.75,"kind":"bm25"}]`；未命中的腿**不出现**（不是 `0`） | INT 从 `search_page` 组装 | **NULL** |
| **`candidates_json`** `[新增]` | TEXT | 每条腿进入融合的候选数 + 并集：`{"semantic":60,"keyword":60,"fused":84}` | INT 从 `SearchEvidence` 组装 | **NULL** |
| **`selected_json`** `[新增]` | TEXT | 本次**保留**的分节计数：`{"memories":5,"knowledge":5,"wiki":2,"entities":3,"graph_paths":4}` | INT | **NULL** |
| **`rejected_json`** `[新增]` | TEXT | 本次**丢弃**的分节计数：`{"memories_by_top_n":2,"knowledge_by_top_n":3,"knowledge_by_min_score":1,"wiki_by_top_n":0,"graph_no_seed":7}`（`min_score` 的丢弃今天被静默 `continue`，必须计数） | INT | **NULL** |
| **`graph_entities`** / **`graph_paths`** `[新增]` | INTEGER | 多跳图的**种子数** / **发出的路径数**；`graph_paths=0` 与「没接图」必须能分开（联读 `rejected_json.graph_no_seed`） | I-C `RetrievalStats` | **NULL** |

**写入点唯一**：`crates/daemon/src/api.rs::recall`（`INSERT INTO recall_log`）。`/api/v1/recall/log` 的读出必须**逐列透出**；`retention.max_rows = 5000`。

**落地进度（`git status` + 迁移文件读于 2026-09-27T22:53:52+08:00；**后续复核 2026-09-27T23:05:22+08:00**：I-SCHEMA 已追加 `0023_recall_telemetry.sql`（mtime 22:56:55），§3.1 的 14 列**全部落地**）**

| 列 | 落地状态 | 依据 |
| --- | --- | --- |
| `top_knowledge_relevance` · `…_kind` · `knowledge_leg_window` · `scoring_version` | **已落地** | `0019_recall_quality.sql` |
| `graph_entities` · `graph_paths` | **已落地** | `0021_graph_evidence.sql` |
| `score_kind` · `fusion` · `top_legs_json` · `candidates_json` · `selected_json` · `rejected_json` | **已落地（缺口已闭合）** | `0023_recall_telemetry.sql`（I-SCHEMA；列名与语义与本表逐字一致，全部可空无默认） |

**⇒ `recall_log` 现共 24 列**（原有 12 + 0019 的 4 + 0021 的 2 + 0023 的 6）。I-SCHEMA 的 `new_objects_exist_on_a_fresh_database` 把**完整列序逐位钉住**，因此本表不是唯一的「落地清单」来源 —— 多一列/少一列/次序变了都会在测试里红。**历史 651 行这 14 列一律 NULL**（读作 `unknown (pre-0019/0021/0023)`），**不许**写 0；`top_knowledge_score` 一个字节未改 ⇒ A-3a 的「历史行不可比」结论不变。

### 3.2 `context_injected`（transcripts JSONL 事件）

今天的形状只有 `{"event":{"type":"context_injected","render":"<tag>…</tag>"}}`（`crates/core/src/event.rs:121`）。本代在其上加**可选**字段（`#[serde(default)]`，旧 JSONL 向后兼容）：

| 字段 | 类型 | 语义 |
| --- | --- | --- |
| `render` | String（既有，**字节不变**） | agent 实际收到的块文本；预算是**字符**（`chars().count()`）不是字节（`gen2-memory-spec` D.1 冻结） |
| `path` `[新增]` | `"run"` \| `"chat"` | 哪条注入路径（两条路径的差异是**参数不是漂移**） |
| `budget` `[新增]` | 对象 | 见下；**`null` = 本次未采集**（不许写全 0 假装采集了） |
| `budget.per_block_chars` / `total_chars` | usize | `InjectionBudget::default()` = 1024 / 4096（字符） |
| `budget.used_chars` | usize | `render.chars().count()`；判据 `used_chars ≤ total_chars`（硬上界） |
| `budget.truncated_blocks` | u32 | 块体被截断的块数（`… [+N chars truncated]`） |
| `budget.dropped_items` | u32 | 被丢弃的**条目数**（不是块数；`render_context` 的 `dropped += group.len()` 语义，冻结） |
| `budget.blocks[]` | 数组 | 每块 `{tag,items,chars,truncated_chars,dropped_items}`；`tag` 用契约词表（`user_profile`/`relevant_memories`/`knowledge`/`graph`/`wiki`/`project_context`），未知 tag ⇒ rank 6 |
| `budget.notes` `[新增]` | 可选字符串 | 采集侧的自我说明（例：`"probe"`）；缺省省略 |

**基线（改前）**：盘上 147 个 transcript、80 个 `context_injected`，其中契约渲染 19 个（字符 108/1660/2458，18/19 有截断标记、**0/19 有丢块通知**、0/19 出现 `<context_budget>`）；全 80 事件仅 1 次丢块通知。**`0/19 丢块` 不是缺陷证据，是没有样本**（预算从未逼近 4096）——所以本表的字段必须有**造样本**的锚点（A-6）。

### 3.3 一条记录能回答哪些问题

**`recall_log` 一行（+ 联读 `knowledge_meta.embedder`）能回答：**

1. 这个查询的 top-1 知识命中是**哪条腿**找到的、在该腿里第几名、原始分多少（`top_legs_json` + `kind`）？
2. 这次排序**是不是分页参数的函数**？（`knowledge_leg_window` + `scoring_version`：窗口恒 60 ⇒ 「分页不改排序」在 API 可达面上成立）
3. 每条腿给了多少候选、融合并集多大（`candidates_json`）？
4. 各节**保留了多少 / 丢了多少，因为什么规则丢的**（`selected_json` / `rejected_json`）？
5. 图这一腿**有没有参与**：种子几条、路径几条、是「没种子」还是「有种子无路径」（`graph_entities`/`graph_paths` + `rejected_json.graph_no_seed`）？
6. 谁调用的（`source`）？无答案查询与可答查询**在展示分上是否仍然重叠**（`top_knowledge_score` 分布，今天重叠且触上界 2/61）？
7. 跨版本可比吗？（`scoring_version` + `embedder`：换模型/换权重必须 bump，禁止新旧同图）

**`context_injected` 一个事件能回答：**

1. 这次注入**发了哪些块、每块多少条、多少字符**（`blocks[]`）？
2. **有没有被截断/丢块，丢了几条、丢的是哪个 tag**（`truncated_blocks`/`dropped_items`/`blocks[].dropped_items`）？
3. 离 4096 还有多少余量（`used_chars/total_chars`）？
4. 是 run 还是 chat 路径（`path`），两条路径的块集合差异是什么？
5. 与 `recall_log` 按 `ts`+`query` 联读：**检索到的东西真的进了注入吗**（今天的第三层证据只有 `injection_e2e` 的 mock-agent 回显，盘上没有预算账目）？

**明确答不了的（写清，不许拿相邻字段顶替）：**

- 单条记录**答不了**「agent 是否真的读了/用了」这条内容（只有发出去的事实；`gen2-memory-spec` A.12 已把「被 agent 收到」列为第二层证据）。
- `recall_log` **答不了**注入预算（两个载体不同）；`context_injected` **答不了**检索质量（没有腿信息）。
- 今天 `top_knowledge_score` **答不了**「这次结果好不好」（无答案查询可与可答查询同分；C3 的判据在此必然失败）。

---

## 4 面板面（Memory / Knowledge / Wiki / Graph）

**总原则（本节的边界）**：**只收敛，不重设计视觉**。不新增导航路由、不新增页面、不改布局与配色；新增的只有「读数」（标签、数值、单位、三态文本）与「纠正入口」（已存在按钮的接线或规格要求的一对子资源按钮）。**所有新文案必须同时进 `panel/src/i18n/*.ts` 的 zh 与 en**（`npm run i18n:check` 会红，见 A-5）。

| 页面 | 新增/修改的读数 | 纠正入口 | 具体坐标（今天的形状 → 本代要求） |
| --- | --- | --- | --- |
| **Memory** | ① recall log 行：`m 0.86` / `k 0.016` **必须带量纲**（`m 0.86 cosine` / `k 0.016 rrf_rank`，或拆两行）② 新增 `legs/leg_window/scoring_version/selected/rejected/graph_*` 读数（读到 `null` ⇒ 显示「—」，显示 `unknown (pre-0018)`）③ `distilled` 徽章保持（今天 0 正例）④ confidence 保持恒显 + `<0.5` 的「低」标记 | `supersede` / 软删 / `purge=true` 真删 / `restore`（全部已有，本次只接线新读数） | `panel/src/views/Memory.tsx:367-384`（并排两数，本代必须带 kind；这是 `gen2-recall-spec` A7 的缺陷现场）· `:486-497`（confidence，保持）· `:480-484`（distilled，保持） |
| **Knowledge** | ① 每个分数旁带 `*_kind`：`score … rrf_rank`、`semantic_score … semantic_l2sq(距离)`、`keyword_score … bm25` ② 新增 `relevance`（带 kind，标「未标定完成、不是闸门」）③ 头部加 `fusion`/`leg_window`/`candidates` ④ `query_keyword_stage` 新增值 `bigram` 必须原样显示 | chunk 编辑 / 修订列表 / 回滚（全部已有） | `panel/src/views/Knowledge.tsx:411-475`（今天已把两条腿分开不并排：**保持**，只补 kind 与 relevance）；`panel/src/api.ts:194-222` 的 `KnowledgeHit` 类型补新键（全部可选，兼容旧 daemon） |
| **Wiki** | ① 页行加 `freshness` 三态（`unknown` 渲染 `?`，**不许**渲染成 `fresh`）、`cite_coverage`、`citations`、`stale_since`、`stale_sources`、`links_out_broken`、`self_links`、`frozen_by` ② `broken` 扁平名单 → `wanted`（带 `demanders`/`demand_count`，按需求数排序）+ `unreachable` 与 `orphans` **分开报** ③ 构建表：**plan-only 的判据是派生布尔 `dry_run === true`**（触发器保证它与 `status ∈ {planned, planned_only}` 一致），**禁止**按 `'planned_only'` 字面量过滤 —— 迁移 `0022` 已把 8 行里的 2 行 legacy `planned_only` 回填成 `planned`，而在 I-D(t10) 改 `DRY_RUN_STATUS` 之前新行仍写 `planned_only`；两个值并存时只看字面量会漏一半（recall 2026-09-27 实测确认）④ 图读数每次 `done` 构建一行（历史序列可读） | **新增**：`pin` / `release` / `note`（`reason`+`author` 必填，空 ⇒ 400） | `panel/src/views/Wiki.tsx:113/131/143-146/186-192/244` · `panel/src/api.ts:412-445`（类型补键）· `panel/e2e/wiki.spec.ts:84-87`（不得渲染 `source_hashes`，本代扩展为「不得把 `unknown` 渲染成 `fresh`」） |
| **Graph** | ① 多跳检索读数：`seeds_by_leg`（每条腿计数，0 也要出现）、`paths` 条数、`truncated_by`、`empty_reason` ② 每条边/事实加 `temporal` 四态与 `source`（`episode`/`session_key`/`recorded_at`）③ 社区层 `null` ⇒ 显示「未建社区」，0 ⇒ 「0 个社区」（两者**必须可区分**）④ 待裁决对（`resolution_pending`）只读列表 | 建实体 / 加事实 / 删实体（全部已有） | `panel/src/views/Graph.tsx`（实体/边列表与详情，`panel/src/api.ts:473-489` 的 `GraphEdge` 补 `temporal`/`source`——全部可选） |

**跨页一致性判据（可机械检查）**

1. 同一页在**任何两个消费面**（面板 / HTTP / MCP / 注入）对同一事实说法一致：`stale` 的三态、`hint` 的句子（`gen2-wiki-spec` D.7：`recall_stubs` 与 `lead_for` 用**同一个** `WIKI_LEAD_HINT` 常量）、`score_kind` 字面量。
2. **未知枚举值不许崩、不许被吞成 0**：`query_keyword_stage`、`memory_diffs.op`、`freshness`、`TemporalStatus`、`SeedLeg`、`CiteProblemKind` 全部按字符串显示或忽略。
3. **`null` 与 `0` 不许互相顶替**：`total:Option`、`top_knowledge_relevance`、`graph_communities=null`、历史行的新列 —— 面板一律显示 `—`/`unknown`。

---

## 5 冲突裁决

### 5.1 四份规格之间的冲突（逐条）

| ID | 冲突（谁怎么说） | 裁决 | 理由 |
| --- | --- | --- | --- |
| **K-1** | **迁移编号被四方同时认领**：`gen2-recall-spec` E10 要 `0019_recall_quality.sql`（gold 集 + recall_log 4 列 + `chunks.grams` + `chunks_fts_cjk`）；`gen2-wiki-spec` E.2 要 `0019_wiki_gen2.sql`（DDL-1..7），且它自己的 E1 行又写成 `0019_wiki_citations.sql`；`gen2-memory-spec` B-8 与 `gen2-graph-spec` DEP-3 也各自要列 | **编号的唯一分配者是 I-SCHEMA（t6, recall），每区一个连续号；INT 不写任何迁移。** 本契约核实时的**在途现场**（`git status` 读于 **2026-09-27T22:53:52+08:00**，文件 mtime 22:50:59/22:51:11/22:51:29/22:51:58）：`0019_recall_quality.sql`（recall：gold 三表 + `recall_log` 4 列 + `chunks.grams` + `chunks_fts_cjk`）/`0020_memory_semantics.sql`（memory：`memories.{access_count,last_used_at,valid_from,valid_to}`、`memory_sources`、`distill_log.{status,failure_reason,prompt_hash}`）/`0021_graph_evidence.sql`（graph：`entity_edges.{event_time_source,fact_hash}`、`entity_aliases`、`resolution_pending`、`communities`、`community_entities`、`entities.{embedding,embedder}`、`recall_log.{graph_entities,graph_paths}`）/`0022_wiki_gen2.sql`（wiki：`wiki_pages`、`wiki_citations`、`wiki_graph_readings`、`wiki_corrections` + 回填 + 配对触发器）/**`0023_recall_telemetry.sql`**（recall：`recall_log` 的 `score_kind`/`fusion`/`top_legs_json`/`candidates_json`/`selected_json`/`rejected_json`——本契约 §3.1 的缺口，追加于 2026-09-27T22:56:55+08:00，复核于 23:05:22+08:00）。**冻结这个分配**：规格正文里写的 `0019_wiki_gen2.sql` 等文件名**被本分配取代**，任何区域不得自行挑号或另立文件 | 编号是全局资源：四份规格各自把 0019 当成自己的私有号，落地时必然相撞。**分文件（每区一个连续号）+ 单一分配者**比「一个文件四段」更贴合本仓 `schema_migrations` 的既有机制；文件怎么切是 I-SCHEMA 的自由，**列名与语义由本契约冻结** |
| **K-2** | **`recall_log` 新列的数量与落地进度**：`gen2-recall-spec` E6 只建 4 列（relevance/relevance_kind/leg_window/scoring_version）并把值写入交给 INT；`gen2-graph-spec` DEP-3 又要 `graph_entities`/`graph_paths`；本契约 §3.1 还要 `score_kind`/`fusion`/`top_legs_json`/`candidates_json`/`selected_json`/`rejected_json` | **以 §3.1 为超集；缺口已闭合（I-SCHEMA 2026-09-27T22:56:55+08:00 追加 `0023_recall_telemetry.sql`，复核 23:05:22+08:00）**：14 列全部落地 = 0019 的 `top_knowledge_relevance`/`…_kind`/`knowledge_leg_window`/`scoring_version` + 0021 的 `graph_entities`/`graph_paths` + 0023 的 6 列（列名与语义与 §3.1 逐字一致，全部可空无默认）。`recall_log` 现共 **24 列**；`top_knowledge_score` 的解释**一个字节不改**；历史 651 行这 14 列全部 NULL | 「一条记录能回答哪些问题」是集成面唯一的可证伪标准；4 列答不了「哪条腿找到 top-1」「丢了多少、为什么丢」。**落地后 A-3b 从 `not_measured` 升级为可判**；写入侧（`api.rs` 那个静默 `continue` 变成 `rejected_json.knowledge_by_min_score` 计数）是 INT 的责任 —— **列在 ≠ 值在** |
| **K-3** | **`SearchLegs` 改名 vs 冻结**：`gen2-recall-spec` D.2 新增 `SearchEvidence` 并给 `SearchLegs` 别名（B-1 说这样破坏=0），但 D.2 的 `RankedHit` 结构是**嵌套**的（`semantic: Option<LegEvidence>`）；而 `api.rs` 今天读扁平 `.semantic/.keyword/.keyword_stage/.fused.len()`，面板读扁平 JSON 键 | **库侧**：实现 `SearchEvidence`（或别名）+ `search_page`，`api.rs` 改调一次 `search_page`（消掉 H-2 的第二次检索）。**JSON 侧**：保持**扁平键**（`semantic_rank`/`semantic_score`/`keyword_rank`/`keyword_score`/`query_keyword_stage`），只**加法**补 `*_kind`/`relevance`/`fusion`/`leg_window`/`candidates` | 库内结构可以嵌，线上契约不能嵌：面板 `Knowledge.tsx:446-471`、`api.ts:194-222`、`knowledge_api.rs` 都按扁平键读。嵌套会同时破坏三处消费方，而收益为零 |
| **K-4** | **同一字段名 4 种量纲**：`gen2-recall-spec` C6 要求「每个分数自带 kind」；`gen2-memory-spec` D.7 冻结 `"rrf_rank"` 字面量、且明文禁止「顺手把 `semantic_score` 改成相似度」 | **冻结语义 + 只加新键**：`score` 恒为 RRF 名次分（`score_kind="rrf_rank"`）；knowledge 的 `semantic_score` 恒为**距离**、memory 的 `semantic_score` 恒为**余弦**；新键 `semantic_score_kind`/`keyword_score_kind`/`relevance_kind` 表达量纲；面板并排时必须带 kind | 两处规格其实不冲突：一处要「可读量纲」，一处要「不许改名」。加键同时满足，改名只满足一处并制造静默错误 |
| **K-5** | **注入字节会不会变**：`gen2-graph-spec` D.2 要 `TAG_GRAPH` 且 rank=3（`gen2-memory-spec` D.2 已同意），这会改 `tag_rank`；`gen2-wiki-spec` D.7 要给 `<wiki>` 块加 `stale/coverage/anchors/hint` 行；而 `gen2-memory-spec` D.6 说「改 `render_context` 字节必须给 crate golden + 盘上 before/after」 | **两件都做，但必须成对交读数**：rank 冻结为 `user_profile 0 · relevant_memories 1 · knowledge 2 · graph 3 · wiki 4 · project_context 5 · unknown 6`；`<wiki>` 加的标记行必须与 HTTP 面的 `hint` 同句。**before 读数已存在**：19 个 tagged render（字符 108/1660/2458、18/19 截断、0/19 丢块）；after 必须给出至少一条带 `<graph>` 的盘上事件（`gen2-graph-spec` G10） | 这是策略变更（预算绑定时先丢谁变了），不是纯加法；不给 before/after 就无法区分「没丢」与「没样本」（今天的 0/19 丢块就是后者） |
| **K-6** | **`crates/daemon/src/memembed.rs` 的归属**：本契约的原始 inScope 列了它，而 captain 的 R-1 裁决把它给了 mem-core/I-B(t8)（t19 的 inScope 已 amend 移除） | **以 R-1 为准：INT 一行都不改 `memembed.rs`**。契约中凡涉及「候选集余弦 / 注入选择 / 图证据 → `ContextItem`」的操作，一律写成 **「I-B 提供 `select_injection_memories`、（图证据的）`graph_items`；INT 只调用」** | 一个文件一个写者（closure §5）；R-1 晚于 t19 的 inScope 定义，是 captain 的显式裁决 |
| **K-7** | **wiki plan-only 的两条路线**：`gen2-wiki-spec` E.2 路线 A（删 `dry_run` 列，完整单源，但必须改 `panel/src/api.ts:437` 与 `Wiki.tsx:244`）vs 路线 B（保留 `dry_run` + 回填 + 不变量，词汇 3→2）；规格自裁「先做路线 B」 | **确认路线 B 为本轮唯一路线**；**落地机制按 I-SCHEMA 的在途实现冻结**（`0022_wiki_gen2.sql`，读于 2026-09-27T22:53）：不变量由 **BEFORE INSERT/UPDATE 触发器** `wiki_builds_plan_pairing_{ins,upd}` 强制 + 视图 `wiki_builds_unfinished_plans` 提供那条不可表达的读数（规格写的 CHECK 在 SQLite 上做不出，impl 已显式登记该偏差）。**线上 `status` 词表 = `planned\|planned_only\|running\|done\|failed`**：`planned_only` 是**过渡值**（`wiki.rs:1838 DRY_RUN_STATUS` 仍产出，I-D 的 E3 改成 `planned`）。**消费规则冻结为「按派生布尔 `dry_run===true` 判 plan-only，禁止按 `'planned_only'` 字面量过滤」**：迁移 0022 已回填 2/8 行 legacy，而新行在 I-D 改动前仍写 `planned_only` —— 只看字面量会漏一半（recall 2026-09-27 实测）。路线 A 记为 P1，**本代不改面板** | 路线 A 的表重建在活动表（8 行 + 41 页行，`PRAGMA foreign_keys` 开着）上需要一次可复现自检，唯一收益是「同一事实不写两份」；触发器已经把「两份不一致」这个**可观测**缺陷消灭。过渡值必须写进契约，否则面板会在 I-D 落地前后表现不同而没人知道为什么 |
| **K-8** | **`links_out` 收窄是破坏性变更，而钉住旧形状的测试不在 I-D 的 inScope**：`gen2-wiki-spec` D.2 收窄语义（`cooking-pasta` 3→2），E.1 明说 `crates/daemon/tests/knowledge_api.rs` 由 I-INT 改（`knowledge_api.rs:478-506` 逐字段断言 `links_out==2`/`broken==["k8s"]`/`orphans==["orphan-page"]`） | **接受收窄，改测试归 INT**：断言改成「`pages[i].links_out == degrees[i].links_out`」「`broken` 与 `wanted.slug` 逐位相等」「自链出现在 `self_links`」「`orphans ⊆ unreachable`」 | 逐字段钉住旧形状的测试在语义变更后必须由**同一次改动**更新，否则要么假绿（测试没跟上）要么假红（实现正确） |
| **K-9** | **CJK bigram 变体与线上词表**：`gen2-recall-spec` D.3 B-8 加 `KeywordStage::Bigram`（已核全仓无穷尽 `match`，`api.rs` 自动输出 `"bigram"`）；但面板/MCP 消费的是字符串 | **线上词表冻结为 `empty\|precision\|prefix\|bigram\|substring`**；消费方必须「未知值原样显示」；面板为 `bigram` 补 i18n 键（zh+en） | 加变体是源码兼容，但**线上字符串词表是契约**：不冻结就会有人当成埋点值穷举 |
| **K-10** | **`retrieval-quality` 仪器已饱和（三项恰好 1.0000）与它的扩写位置**：`gen2-recall-spec` C8/E3 要扩写 `crates/knowledge/tests/**` 并新建 `query_eval_*` 三表（`crates/store`） | **INT 不写这两处**：契约只把「至少一个切片 baseline < 1.0」记为**前置条件**（V-INT 可用 `cargo test -p ruagent-knowledge --test retrieval-quality` 当只读证据引用），不作为 INT 的验收目标 | INT 的 inScope 不含 `crates/knowledge`（I-A 写）与 `crates/store`（I-SCHEMA 写）；越界写 = 两个写者 |
| **K-11** | **`distill_log.prompt_hash` 的计算点**：`gen2-graph-spec` D3 闸门 3 要求 prompt 指纹进 `distill_log`；prompt 的**生效值**由 INT 的 `/api/v1/distill` 与 I-C 的 `distill.rs` 共同决定，谁算？ | **一个计算点，INT 暴露**：`prompt_hash = sha256(生效 prompt 文本)`；`GET/PUT /api/v1/distill` 返回它（F-6）；I-C 写入 `distill_log.prompt_hash` 时**取同一个函数**，不许自己再哈希一遍 | 两处各算一次就是 doc/code 漂移的新现场（`gen2-memory-spec` D-3 的 `distill.rs:371` 注释漂移就是这么来的） |
| **K-12** | **注入面用不用 `relevance` 重排**：`gen2-recall-spec` H-4/B11 要注入侧用 `relevance` 而不是 RRF 名次分做重排；`gen2-memory-spec` D.7 冻结「注入面不使用 RRF；查询腿是原始余弦阈值（`query_min_score=0.34`）」；而 `relevance` 是 `[0,1]` 的标定分，与余弦阈值不同轴 | **`relevance` 只做同轴内的次级排序键；闸门仍是余弦**（本代）：注入的选择 = 余弦 top-N + `query_min_score` 过滤（D.7 冻结），命中之间可用 `relevance` 做**次序**调整；**禁止**把 `relevance` 与 `query_min_score` 混成同一个阈值 | `relevance` 的切点未标定（C3 明文禁止把 0.227 写成常量）；把未标定的量与已成型的余弦阈值混用，就是再造一次 A7 的量纲混用 |
| **K-13** | **两条「第三态」的写法不同**：`gen2-memory-spec` §C5-D 用 `ExternalStatus{Readout,NotAvailable,Unknown}`；`gen2-wiki-spec` D.5 用 `freshness:"fresh"|"stale"|"unknown"` + `PageFreshness.stale:Option<bool>` | **不是冲突，但要冻结映射**：wiki 面回答残余问题时，`freshness=="unknown"` ⇒ 对应的 carrier 记 `NotAvailable("wiki freshness unknown: <unknown_cause>")`，**不许**记 `Readout(0)`；「哈希载体」永远不升级成 `Readout` | 两套都是三态，但一个是「问过没有」，一个是「新不新」。不映射就会出现「读不到 ⇒ 报 0」这类假通过（语义第五/六条正是为它写的） |
| **K-14** | **空结果的可解释性在两侧强度不同**：`gen2-graph-spec` 冻结 `EmptyReason`/`TruncationBudget`（空必须说得出为什么空）；召回面对无答案查询**今天不可分**（`recall@1` 无答案最高分 > 可答最低分） | **采纳图的判据到召回面，但不许当闸门**：`recall.graph.stats.empty_reason`/`truncated_by` 必须透出；**禁止**用 `empty_reason`/`relevance` 做「这条查询有没有答案」的判定 | 图的判据是「为什么没到」，不是「该不该返回」；把它升级成闸门就会把 C3 的未标定量变成开关 |
| **K-15** | **wiki lead 的两个消费面措辞**：`gen2-wiki-spec` D.7 要 HTTP `recall_stubs` 的 `hint` 与注入面「说同一句话」，而今天注入面没有 | **一个常量两个消费面**：两处**逐字相同**的字面量 —— I-D 的 `wiki::LEAD_HINT`（`wiki.rs:2470`）与 I-B 的 `inject::WIKI_LEAD_HINT`（`inject.rs:364`），各自常量、**无跨 crate 依赖**（`crates/memory` 刻意不依赖 `ruagent_knowledge`/daemon）；`recall_stubs` 与 `lead_for` 的 `hint` 都取自 I-D 的那个；INT 只在 `chat.rs`/`runs.rs` 两个构造点把 lead 装进 `EnrichedHit`（**不再给 `RetrievalHit` 加字段**，见 DEP-INT-8）。**判据**：`Select-String` 两个 crate 里那句 hint 文本逐字相同，且注入块里的 `hint` 行与 HTTP stub 的 `hint` 字段相等（两处逐字相同的字面量 + 一条比对判据，而不是一个跨 crate 常量） | 同一个事实两个字符串 = 迟早漂移；常量（或两处逐字相同的字面量 + 一条比对判据）是唯一能让「说法一致」变成可 grep 的形式 |
| **K-16** | **`entities` 一列的两种含义**：`recall_log.entities` 今天 = `search_entities` 返回行数；改成 `resolve_seeds` 后它仍然存在，而多跳路径没有任何列 | **见 K-2 的裁决**：`entities` 语义不变（本节返回的实体行数）；多跳读数进 `graph_entities`/`graph_paths`；`recall` 响应新增顶层 `graph` 段 | 改旧列语义会让 651 行历史与新行不可同读；加列则新旧各说各的、可分别标注 |

### 5.2 四份规格都没提、但集成必须冻结的接口（本契约新增）

| ID | 冻结的接口 | 冻结值 | 理由 |
| --- | --- | --- | --- |
| **F-1** | **API 可达的 `limit` 上界** | `/knowledge/search` 的 `limit` 夹 **1..=50**；`/recall` 的 `top_n` 保持 1..20 且 `search_n=min(3*top_n,30)`；两者都 `≤ LEG_WINDOW=60` | 恒定腿窗口只有在「窗口 ≥ 任何 API 可达的 limit」时才保证 C5（分页不改排序）。今天 `knowledge_search` 无上界，`limit=500` 会让窗口小于请求 |
| **F-2** | **残余面的单一读入口** | `GET /api/v1/forget-report`（§1.1）+ MCP `memory_forget_report` | 回答 `gen2-memory-spec` U-6（明写「取决于单一消费面的设计（HTTP/MCP/面板）⇒ integ(t5/t19)」） |
| **F-3** | **wiki 纠错的两个端点** | `GET/POST /api/v1/knowledge/wiki/pages/{slug}/corrections` | `gen2-wiki-spec` D.6 只说「写入端点在 `api.rs`（I-INT）」，形状由本契约冻结（`reason`/`author` 必填，空 ⇒ 400） |
| **F-4** | **多跳图检索的暴露口** | `GET /api/v1/graph/retrieve` + MCP `graph_retrieve` | 四份规格都要求多跳进入消费面（G1/G5/G10），但没有任何一份说它从哪个端点出去；没有它就只能在测试里读 G5 |
| **F-5** | **MCP 写工具的审计来源字段** | `memory_write`→`episodes(kind='mcp_write')`+`source_episode`；`knowledge_ingest`→`documents.source`；`memory_recall`→`recall_log.source='mcp_recall'` | t5 的硬要求（写工具必须具备可审计来源字段）；今天 `memory_recall` 的 `source` 是 NULL |
| **F-6** | **`prompt_hash` 的单一计算点** | `sha256(生效 prompt)`，由 `/api/v1/distill` 的 GET/PUT 暴露，供 `distill_log` 复用 | K-11；也是「产出率变化可归因」的唯一机械判据 |
| **F-7** | **`recall_log.source` 的声明义务** | panel ⇒ `panel`；MCP ⇒ `mcp_recall`；harness/probe ⇒ `probe`；未声明 ⇒ NULL 且 `source_label="unknown (pre-0018)"` | 今天 651 行里 624 行 NULL；不补声明，「谁在用召回」永远答不了 |
| **F-8** | **`scoring_version` 与 embedder 的绑定** | 换 embedder / 换权重 / 换窗口 ⇒ `scoring_version` **+1**；`/api/v1/stats` 必须同时暴露 `embedder` 与当前 `scoring_version` | `gen2-recall-spec` B10：距离没有绝对零点、换模型后历史分不可比；没有版本号就没人知道曲线换了轴 |
| **F-9** | **错误纪律** | 未知枚举值 ⇒ **400 + 有效值列表**（不许回落）；不存在的资源 ⇒ **404**（不许 200 空）；拒绝**不得留下副作用**（`memory_write` 的 400 在 `record_episode` 之前） | 「诚实失败」是四份规格共同的第 7 条目标；回落与 200 空是同一类假通过 |
| **F-10** | **`context_injected` 的预算遥测字段** | §3.2 的 `path` + `budget{…}`（可选、`serde(default)`） | t5 的硬要求；四份规格都只说「注入预算」，没有一份规定它怎么被记账 |
| **F-11** | **未知值容忍** | 消费方（面板/MCP/CLI）对未知的 `score_kind`/`query_keyword_stage`/`op`/`freshness`/`TemporalStatus`/`SeedLeg`/`CiteProblemKind` **必须原样显示或忽略，不许崩、不许吞成 0** | 四份规格各自都在加枚举值（B-5/B-8/B-9、四态/五态）；不加这条，第一个新值就是线上事故 |
| **F-12** | **`crates/orchestrator` 本代零接口** | 无 | 已核：`crates/orchestrator/src` 里 `memory|recall|knowledge|graph|inject` 只命中一处**注释**（`lib.rs:76` 讲 handoff 文案）；没有接线点。写下来是为了让 V-INT 不必去那里找 |
| **F-13** | **评测集合规模的读法（V-A 实测 `query_eval_gold` 不幂等）** | 任何消费面/遥测/面板**不许用 `query_eval_gold` 的**表行数**表示集合规模**；必须 `COUNT(DISTINCT query)`（或按冻结常量）。读数形状：同一 seed 跑 **6 次** ⇒ 表 **132 行**而集合只有 **22 条**（V-A 实测 `(132, 90)`）；根因 = schema 无 `UNIQUE(set_id, query)` + seed 用 `INSERT OR IGNORE` ⇒ **幂等性 repair 归 recall**（captain 已派单） | 今天的 HTTP 面**没有**暴露 `query_eval_*`，所以这条是**预防性冻结**：任何人一旦把它接进读数，先按本行写 `COUNT(DISTINCT …)`。写进契约是因为「表行数」看起来永远比「集合大小」权威，而这里是反的 |

### 5.3 越界接口的改写（区域提供库接口，INT 只接线）

| ID | 需要的东西 | 谁提供（不在 INT inScope） | INT 只做什么 |
| --- | --- | --- | --- |
| **DEP-INT-1** | ~~`RunEvent::ContextInjected` 增可选 `path`/`budget` 字段的 chat 路径构造点：`crates/acp/src/chat.rs:497`~~ **已改派（captain 2026-09-27）**：`crates/acp` 原先全 DAG 无人拥有（真实缺口），现并入 **t19 的 inScope** | **INT（t19）自己**：`crates/core/src/event.rs`（字段定义）+ `crates/daemon/src/runs.rs:1270`（run 路径）+ `crates/acp/src/chat.rs:497`（chat 路径） | 三条都改；**两条路径都要交读数**：`含 budget/path 的注入事件数 / 总注入事件数`（run 与 chat 各一个比值，t19 验收项）⇒ 原先的 `not_measured（原因：acp 构造点未落地）` 回退项**作废** |
| **DEP-INT-2** | `InjectionBudget` 的报告函数（`render_context` 的账目） | **I-B（mem-core）**：建议 `render_context_report(items,budget) -> (String, InjectReport)`；若 I-B 只提供 `render()`，INT 不许自己再实现一份预算算术 | `runs.rs`/`chat.rs` 调用并把报告交给事件 |
| **DEP-INT-3** | `Knowledge::search_page` / `SearchEvidence`（含 `leg_window`/`fusion`/`candidates`） | **I-A（recall）** | `api.rs` 调它、把扁平键组装进响应与 `recall_log` |
| **DEP-INT-4** | `resolve_seeds` / `retrieve` / `communities` **已就绪**（`crates/graph/src/retrieve.rs:384/633`、`crates/graph/src/community.rs:172/271/314/328/352`、`crates/graph/src/lib.rs:882/898/927`） | **I-C（graph）** | `api.rs` 暴露 §1.5 的六条路由（`/graph/retrieve`、`/graph/communities`、`POST /graph/communities/build`、`PUT /graph/community/{id}/summary`、`/graph/resolution/pending`、`POST /graph/resolution/merge`），`recall` 里接 `resolve_seeds`（替换 strict 的 `search_entities`）。**graph 明确判断：不再开第二个消费面（不另开 CLI）** ⇒ 面板/CLI 只挑一个消费面，与「单一消费面」的收敛方向一致（captain 2026-09-28 转达） |
| **DEP-INT-5** | `wiki::pages/links/builds/...`（含新字段）与 `lead_for`/`add_correction` | **I-D（wiki）** | 端点透传、两个调用点填 `lead`、纠错端点接线 |
| **DEP-INT-6** | 迁移 0019 的全部 DDL + `recall_log` 新列 | **I-SCHEMA（t6, recall）** | **只写值**（`INSERT` 的列），不写迁移、不改列 |
| **DEP-INT-7** | `crates/daemon/src/memembed.rs`（注入选择、图证据 → `ContextItem`） | **I-B（R-1 裁决）** | **一行不改**，只调用 |
| **DEP-INT-8** | **I-B 已落地的适配形状（`RetrievalHit` 不加字段）**：`EnrichedHit { hit: RetrievalHit, lead: Option<WikiLeadMeta>, relevance: Option<RelevanceMeta> }` + `knowledge_items_enriched(&[EnrichedHit], sources, wiki)`；旧 API `knowledge_items(&[RetrievalHit], …)` 变成**薄包装**（`EnrichedHit::plain`）。**依据**：`RetrievalHit` 是公开字段结构体，`chat.rs:560-568` / `runs.rs:1884-1891` 用**结构体字面量**构造 ⇒ 加字段会让这两处编译不过（见 R-3a） | **I-B（mem-core）**：上述类型/函数 + 渲染/排序规则（`EnrichedHit::order_value()`）+ **单一映射点**（`crates/daemon/src/memembed.rs:648/669/683` 的 `lead_meta` / `relevance_meta` / `enriched_hit`，R-5）；**I-D（wiki）**：`lead_for(kb, recorded_hash, slug) -> Option<WikiLead>`（`wiki.rs:2780`）；**I-A（recall）**：`RankedHit.relevance` + `search_page`（`store.rs:245/1036`） | **INT 只做接线，且不许自己再写一份适配器**（I-B 已给单一映射点）：两个调用点（`chat.rs:558/569`、`runs.rs:1882/1893`）把 `k.search(...)` 换成 **`kb.search_page(...)`**，对每个 `RankedHit` 调 `memembed::enriched_hit(&rh.hit, relevance_meta(…), lead)`；**`relevance_meta` 按值接收**（R-8b），调用形状 = `memembed::relevance_meta(r.value, r.kind.as_str(), r.version, Some(r.query_background))` —— **`kind` 必须走 `ScoreKind::as_str()`，不许复写 `"calibrated"` 字面量**（R-A A7）；`lead` 仅在 `document` 以 `wiki/` 开头时用 `wiki::lead_for(kb, None, slug)` → `memembed::lead_meta`；最后 `knowledge_items_enriched(&enriched, KNOWLEDGE_SOURCES, WIKI_PAGES)`。**禁止**把 RRF 名次分 `hit.score` 当相关性（R-A H-4）。**`lead_for` 的第三参 `recorded_hash: Option<&str>` 是 I-D 相对 `gen2-wiki-spec` D.7 的加参** ⇒ 以实际签名为准（R-3c） |

---

## 6 验收锚点清单（V-INT / RV-INT 逐条可跑）

### 6.0 环境与临时 root 形状（每条 Rust 命令都以此开头）

```powershell
# 公共前置 1：protoc（绝不用 D:/rust_cache：真守护进程占着那里的 ruagent.exe）
$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"

# 公共前置 2（captain 2026-09-27/28 全队资源纪律，**取代**原先各自设 CARGO_TARGET_DIR 的写法）：
# 每一次 cargo 都走包装脚本；表里为可读性仍短写成 `cargo test …`，
# 实际执行 = 把同样的 cargo 子命令交给脚本：
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test --workspace
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 clippy --workspace --all-targets -- -D warnings
```

**资源纪律（四条，逐条都影响锚点怎么跑）**：① **一次只允许一个编译**（脚本持锁；别人在编时打印 `another team build is running (one compile at a time); waiting 10s ...` 并等待，最长 90 分钟 —— **看到等待不是卡死：不要重试、不要再起一条**）；② **全队共享一份 target 缓存**（`$env:TEMP\ruagent-team-target`），**不要再自己设 `CARGO_TARGET_DIR`**；③ 只用 CPU 0–11（脚本 `-Cpus` 默认 12，留 4 核给用户）；④ `BelowNormal` 优先级。

**编译面纪律（captain 2026-09-28；本代已出现两次的坑，读数是「跑过了」的证据，不是「应该有」的推断）**：`cargo test --workspace` 与 `clippy --all-targets` 都在**第一个失败的 target 就中止**，其后的 target **根本没被编译**。因此：

1. **报「通过」时必须写明这次编译面覆盖了哪些 target**（例：`-p ruagent-daemon --lib` / `--lib test` / `--test injection_e2e` / `--all-targets`）。否则「没跑到」会被读者当成「通过」。
2. **`clippy -p ruagent-daemon --all-targets -- -D warnings` 必须在 lib 与 lib-test 都绿之后再读一次** —— 前一个 target 的错误会挡住后面的 lint。
3. **反例（本代真实发生）**：一轮 daemon clippy 停在 `lib test`（`due to 2 previous errors`）⇒ `crates/daemon/tests/injection_e2e.rs` **从未被编译**，「清单里没有它」不等于「它已经修好」。
4. **最终 verify 分两遍跑并各自记读数**：先 `check`/`clippy` 把编译面弄绿，再跑 `test`；不要把 workspace 级命令的一次中止读成「其余都通过」。
5. **只有 `--all-targets`（或逐 target 全列）才能声称「覆盖了全部 target」**；`-p <某个 crate>` 永远看不到别的 crate 里的同族 lint（例：`distill.rs` 属 **daemon** crate ⇒ `-p ruagent-graph` 永远编不到它）。

**lint「通过」的第二件证据：本次确实重查了哪些 target（clippy 假绿的两半，captain 2026-09-28 转述 wiki(t10) 与 graph(t9) 的实测）**

`clippy … -- -D warnings` 的**尾参不进 cargo 的指纹**：先跑过一次**不带 `-D`** 的 clippy 之后，带 `-D` 的**同一条命令会命中缓存、一条都不重查、直接报成功**。wiki 的实测形状：`exit=0`、只花 **0.8s**、输出里**没有** `Checking ruagent-daemon`，而当时仓库里**确有 warning**；她最终可信的绿是在 `cargo clean -p ruagent-daemon` 之后取得的（**9.67s**，四个 crate 都被重查）。**graph 踩到的是同一失真的另一半**：它第一次跑 daemon gate 拿到 **0.77s / 0.9s 的 `exit=0`**（看着全绿），`touch` 自己改过的文件强制重编后才拿到真读数。因此：

6. **`exit=0` 且输出里有本次的 `Checking <crate>`，两条缺一不可** —— 只有退出码不算通过（0.8s 的 `exit=0` 是假绿的典型形状）。
7. **保险做法（可靠形态）**：`scripts/cargo-team.ps1 … -CleanFirst <crate>`（可重复）—— 它在**同一个持锁窗口内**先 `cargo clean -p <crate>` 再跑门，**中途放不进任何队友的构建**：
   ```powershell
   scripts/cargo-team.ps1 clippy -p ruagent-daemon --all-targets -DenyWarnings -CleanFirst ruagent-daemon
   ```
   **「先 clean、再单独跑门」写成两条独立命令是不可靠的**（第 10 条）：队内锁是**按 cargo 调用**持有的，两次调用之间会放锁，队友的一次普通 clippy 能把缓存又刷成「可命中」。**只删本包产物，不要整仓 `clean`**。`clean` 形状已用 `-DryRun` 验过 = `cargo clean -p ruagent-daemon`。
8. **总纪律（与第 1–5 条合并）**：报「lint / 测试通过」必须给**两件证据** —— **① 覆盖了哪些 target**（`--all-targets` 的 *all* 不是保证，首个失败的 target 就停住）；**② 本次确实重查了哪些 target**（**退出码不算，缓存命中的绿不算**）。
9. **改过哪个 crate，就先 `clean -p <crate>`（或至少 `touch` 该 crate 的文件）再跑 gate**；并尽量用 **`--verbose`** 看逐 target 的 `clippy-driver --crate-name …` 行，作为第 ② 件证据的**逐 target**形态。范例（**缺证据就收窄声称面**）：graph 那一轮**只声明「这次调用 exit 0」，不声称逐 target 证据** —— 因为它没拿到那批 `clippy-driver` 行；**这与「把没重查说成通过」是相反的做法，写进契约作为范例。**
10. **强制重查必须与门在同一个持锁窗口内（wiki t10 实测的竞态，captain 2026-09-28）**：wiki 先 `clean -p <crate>`、再单独跑门，**队友的一次普通 clippy 抢在两次调用之间**（锁是按 cargo 调用持有的）⇒ 缓存又被刷成可命中 ⇒ **1.44s 的假绿**；最后靠 `touch` 自己的源文件 + **紧邻**跑同一条门才拿到真读数。**结构性修法已下沉到工具**：用 `-CleanFirst <crate>`（第 7 条），不要用两条独立命令。
11. **判据必须让「漏测」自己红 —— 绿可能是空跑（recall t33 实测的 harness 事实，captain 2026-09-28）**：
    - **事实**：不做 `#[ignore]` 时，**连「请你测量」的那条命令也会空跑而绿** —— 一个包里没有 ignored 测试时，`cargo test … -- --ignored` 打印 `ok. 0 passed … N filtered out` 并 **`exit=0`**。
    - **纪律措辞**：**「谁来跑」不能当判据** —— 即使有人**专门**去跑测量命令，看到的也可能是一次**什么都没测**的绿。
    - **合格形状**：需要活库副本 / 外部环境的仪器用 **`#[ignore = "…needs <ENV>…"]` + 体首 `expect`** ⇒ 默认跑显示 ignored，**显式运行而缺 env 时必 FAILED**；**再加真实读数断言**（例：`rows==15 && no_answer==7`）⇒ 「有 env 但没测」也红。**一句话原则：不存在一个不测量也能通过的状态。**
    - **同族**：**断言从没被证明失败过，就还没被证明是断言** —— 新增的「防空转」断言本身也要在**真空输入**上实跑出红读数（recall 已另立 **t36** 做这件事）。
    - **本代「绿可能是假的」六种收尾**（第 1–11 条一族）：**① 缓存命中**（第 6/7/9/10 条）· **② 编译面缺口**（首个失败 target 就停 ⇒ 后面的从没编到，第 1–5 条）· **③ target 覆盖声称过宽**（`--all-targets` 的 *all* 不是保证、`Checking` 只到包级）· **④ 夹具/环境状态依赖**（缺 env 时仪器静默退化成空跑）· **⑤ 判据被削弱**（为让它变绿而放宽断言）· **⑥ 空跑绿**（本条）。**判据的设计目标 = 让前五种造成的漏测都表现为一次红。**
12. **公开签名 / 公开类型形状 / re-export 名单的变更，必须先广播给受影响属主（captain 2026-09-28；本代已出现多次）**：一个 crate 的 **re-export 名单**、一个公开类型的 **serde 形状**、一个函数的 **入参类型**，**都是对外的契约** —— 动了就要让别人能**提前**知道，不要让他们从一次红里猜。
    - **两个真实形状**：① graph 的 t27 门禁③④曾**因 wiki 改了接口、而 mem-core 的 `memembed.rs` 尚未适配**而红 —— graph 报给双方、**未替其改一行**，两人收敛后重跑全绿；② mem-core 的 t31 把 `merge_audit_reason` 从 `&MergeVerdict` 改成 `&MergeAudit`，而 `memembed.rs:529` 的调用点**没在同窗口跟上** ⇒ 卡住 graph 两条「真实路径」测试，RV-D 的契约命令也因此第一次 `exit=101`。
    - **纪律写法**：**落地前后都要把「新形状 + 全部受影响调用点」直接发给受影响属主**（一个文件一个写者 ⇒ **不能替对方改**），并在自己的报告里登记「**这一改动会让谁在哪个窗口变红**」。
    - **判据**：**不许让同伴从一次红里猜你在改什么**；跨 crate 签名改动的报告必须**点名**受影响文件与调用点（例：`memembed.rs:529`、`chat.rs:569`、`runs.rs:1893`）。
13. **同形多处替换必须整块一次改完再离开文件（本代已污染一次同伴读数；captain 2026-09-28，来源 = t37）**：
    - **形状**：t37 里我先删 `fn skip_missing_mock()`、再逐个改 7 处调用点；**wiki 的 t34 门禁恰好跑在两次编辑之间** ⇒ 读到 `crates/daemon/tests/injection_e2e.rs:685 cannot find function skip_missing_mock`（**exit=101**），她的门禁读数被**污染一次**（她的应对是正确的：**只报坐标、不代改**，另取一条 `-p ruagent-daemon --lib` 的收窄读数撑住自己的单子）。
    - **纪律写法**：同形多处替换（**改名 / 改签名 / 删函数 / 改调用点**）**要么一次性改完并自测到可编译，要么先加兼容层**（旧名转发 / `#[deprecated]` 包装 / 保留旧形状一轮）**再删旧形状**；**中间态不允许跨过别人的门禁窗口**。
    - **登记义务**：**在报告里登记这个编辑窗口**（哪个文件、什么错、**谁被污染**、何时恢复）—— 登记之后它是「**读数污染**」而不是「事故」（t37 报告 §7 的写法即范例）。
    - **可自动化的一条**：改完**立刻** `cargo check -p <crate> --all-targets`（**不是等门禁**）—— 中间态的红应当是**你自己的**，别让它先变成别人的。

**「被文档描述成存在、实际从未被启用」的开关（t37 的 `RUAGENT_REQUIRE_MOCK`，captain 要求单记）**：审计结论 = 全仓 **5 处提及**，定义侧只在 `crates/daemon/tests/injection_e2e.rs:90/:93/:94`，另 2 处是**文档描述**（`gen2-store-instrument-falsifiability.md:129/:158`、`docs/plans/2026-09-26-memory-knowledge-closure.md:484`），而**没有任何脚本 / CI / toml / 契约命令设置它** ⇒ 它**不是防线，只是「声称的能力」**（t37 已连根删除：留着会在 `-D warnings` 下因无引用而 `dead_code` 红；删掉比留着更诚实）。**审计规则**：任何「严格开关 / 守卫 / opt-in」都必须能被一条 grep 回答「**谁打开它**」；答案是「没有人」时，**要么接进一条门禁，要么删掉**。**同族变体**（本代全部出现过）：`WikiLeadMeta.edited: bool` 丢掉第三态（`crates/memory/src/inject.rs:350`）· `WIKI_LEAD_HINT` 双份字面量 · `ScoreKind` 的 serde 漂移 · 空跑绿 —— **判据必须是跑出来的读数，不是文档里的名字。**
14. **根因是假设，必须用受控反做验证（captain 2026-09-28；RV-C r3 的示范，成本很低、结论很硬）**：
    - **形状**：graph 在 t38 给 G5 归一化的根因写成「**排序 tie 改变了谁进 truncate**」；评审在导出树里**反做**被怀疑的那处改动（删掉 `retrieve.rs:931/:957` 的 `.then_with(|| edge_seq(…))`），**3/3 次**跑出与交付树**逐位相同**的读数 ⇒ **归因被推翻**；真因是「**读数没在交付那一版上重取**」（旧数字在反做后的代码上同样不可复现）。被复现出来的只有**不可复现性本身**（`6 runs byte-identical: false`）。
    - **纪律写法**：报告里写「根因是 X」必须能被一条**反做实验**回答 —— **把 X 改回去（或移除 X），症状是否仍在？** 症状**仍在** ⇒ X **不是**根因（改写成「症状未定因」或继续定位）；症状**消失** ⇒ X 是根因（并**把反做读数写进报告**）。
    - **豁免**：纯机械的直接因果（例：「该列不存在 ⇒ 查询报错」）不必做实验；**推断性的行为归因**（tie-break / 采样面 / 聚合口径 / 缓存 / 并发）**一律要做**。
    - **为什么**：一份持久报告里写错的**根因**会被下一代当真相读 —— 本代已栽过一次同类跟头（把 recall 共享树 `git stash` 的**症状**误归因成「re-export 名单漂移」，需专门发更正才摘干净）。**读数要可复现，根因要可反做。**
    - **与第 11 条同族**：都是「**不要相信没被检验过的断言**」—— 第 11 条是不相信**绿**（空跑），本条是不相信**解释**（根因）。
15. **红也可能是旧二进制 / 也可能是在读别人的数据（captain 2026-09-28；graph t42 的两条实测）**：
    - **① 缓存不只骗绿，也骗红**。graph 做**负控**实验时（注释掉「先删」让测试该红），恢复源码时 **mtime 被保留** ⇒ cargo **复用了负控那份坏二进制** ⇒ 看起来像「修法无效」。**纪律**：**恢复源码后必须 `touch` 或 `-CleanFirst` 再取值**；与第 6/11 条合并成总纪律 —— **绿和红都可能来自陈旧产物，报读数时必须说明你测的是哪一份字节**（本次是否重编、哪个 crate 被重查、构建树哈希/导出树来源）。
    - **② 「偶发红」可能是在读别人的数据**。现场（graph t42 §11，按正经 finding 处置）：夹具 `fixture.rs` 把快照插进 `{pid}-{seq}` 目录却**从不清理**（`Drop` 在连接仍开着时必然失败，实测 `os error 32`；开工时该 crate 名下 **725 个**泄漏目录）⇒ **进程号被回收**后新进程继承上一轮的实体行（id 1..63 已在）⇒ 撞 `UNIQUE(entities.id)` ⇒ 一次整套门禁 `3 passed; 2 failed`，而**单跑 12 次不复现**。**纪律**：**本 crate 会写输入的仪器必须独占其输入**；看到**不可复现的红**，先问「**它读的输入是不是别人留下的**」（临时目录 / 端口 / pid 复用 / 共享副本），不要先怀疑被测代码。**夹具要先删后建**，并补**带负控**的回归测试（禁用删除 ⇒ 红；恢复 ⇒ 绿，坏侧好侧都测到）。
    - **③ 临时 root 的清理义务**：`%TEMP%` 下有 **8,746 个** `ruagent-*` 目录共 **136.8 GB**（C: 可用 172 GB → 102 GB），captain 正在清（保护 `ruagent-team-target` 与 20 分钟内的新目录）。**本代任何测试若创建 root，必须自己收尾**（或落进会被下一轮清掉的固定前缀）。
16. **inScope 跨两个 crate 时，门禁必须覆盖两个 crate（captain 2026-09-28；本代第三个实例）**：
    - **三个实例**：① graph 的 `crates/daemon/src/distill.rs` 属 **daemon** crate ⇒ `-p ruagent-graph` **永远编不到它**；② wiki 的 `crates/mock-agent/tests/wiki_pipeline.rs` 属 **mock-agent** crate ⇒ t34 只跑 `-p ruagent-daemon` ⇒ 它**从未被 lint**，直到 t19 的 workspace 级门禁才露出来；③ `cli/` **此前无人拥有** ⇒ `cli/src/main.rs:809` 的 `items_after_test_module` 一直挂着，挡住的正是 INT 的 workspace clippy 门。
    - **纪律写法**：实现单的 inScope 若含**另一个 crate 的文件**，**verify 必须同时跑那个 crate 的 `-p <crate> --all-targets`**；只跑「主要那个 crate」的门禁，不能声称覆盖了跨 crate 的那部分。
    - **判据**：报告里「我改了哪些文件」与「哪些命令覆盖了它们」必须**一一对应**（被改文件所在 crate ⊆ 被跑过的 `-p <crate>` 集合）。
17. **量词工具本身的默认行为会造假读数（captain 2026-09-28；t47 实例）**：
    - **实例**：INT 用 `Select-String -Pattern 'FAILED'` 数失败，数出 **58 条「失败」** —— 实际是 PowerShell 的 `Select-String` **默认大小写不敏感**，把 **38 条 `test result: ok. 0 failed`** 也算进去了；当时「0 条真失败」这个结论是**别的东西**（包装脚本的 `TEST_EXIT=0`）**独立**支撑的。
    - **纪律写法**：**报计数类读数必须写明工具与关键开关**（大小写敏感与否、字面量 vs 正则、编码、行尾、是否把 `0 failed`/ignored/空跑行算进去）；**能用退出码或显式大小写敏感就不许用模糊匹配**。
    - **并且：结论不许只挂在一个计数上**（本例的正确形状 = 计数可疑 ⇒ 退出码独立支撑，两者并报）。同理适用于「N 个文件改了」「M 列有值」「K 条命中」一切由工具聚合出的数字。
    - **与第 6/11/14/15 条同族**：**判据、绿红、解释、量词 —— 四者都可能被陈旧产物或工具的默认行为造成假象**。
18. **异步面的读数必须先轮询到终态（captain 2026-09-28；t50 实例）**：
    - **实例**：`POST /api/v1/knowledge/wiki/build` **立即返回** `{"status":"running",…}`（构建是异步的）；t50 的探针**在终态前读 DB**，得到 `pages_planned=0` / `wiki_builds.status=running` —— 那**不是产品行为，是读数时机**。同一探针改成轮询 `wiki_builds.status` 到 `done`/`failed` 后，读数变成 `pages_written=1`、`cite_coverage=1.0`。
    - **纪律**：**任何异步面（构建 / 运行 / 蒸馏 / 索引）的读数，先轮询到终态再读**；报告里写明「等到什么状态、等了几轮」。
    - **反面同样是坑**：t48 那次读到的 `failed` 行**是真的**，只因为它读得**够晚** ⇒ **不能把「早读恰好读到终态」当安全习惯**（那是运气，不是方法）。
19. **门禁必须写明它不覆盖什么（captain 2026-09-28；第 16 条的姊妹条）**：
    - **实例（t51 登记 / t53 修正 / t55 再修正）**：**`panel/` 不属于任何 Rust 包** ⇒ 两条 Rust 门禁天然看不到它 —— **这一半成立，是第 19 条的本体**：Rust 门禁（`cargo fmt/clippy/test/check`）不覆盖面板的 TS。
      - ~~「而本仓 `npm run build` 原先只有 `node scripts/build-panel.mjs`（vite 构建，不含 `tsc`）⇒ 一个 TS 类型错误能静默通过」~~ —— **这句是错的（t55 更正）**：`panel/scripts/build-panel.mjs` **一直**在跑 `i18n-check`（L33）→ `npx tsc -b`（L34）→ `npx tsc -p e2e/tsconfig.json --noEmit`（L35）→（`PANEL_BUILD_FORCE_FAIL` 钩子 L39-42）→ `npx vite build`（L44）；`run()`（L25-31）**任一步非零即 `exit`** 并打印 `dist untouched`，`dist` **只在全部检查之后**才被写（L46-58）⇒ **面板门禁本来就覆盖类型检查**。
      - **真实现状**：`cd panel && npm run build` 覆盖类型检查（在 wrapper 内部）；`panel/package.json` 的 `check` 脚本**只是没人显式按名字调它**。t53 加的显式前置两步已**撤销**（与 wrapper 内部**重复**）。
    - **纪律**：**每条门禁都要写明覆盖边界**（覆盖哪些 crate / 语言 / target，**不覆盖**什么）；**声称面必须与被测面一致**（文档写「含 tsc」就必须真含，否则改文档或改脚本）。
    - **t53 的处置**：把类型检查接进 `panel/package.json` 的 `build`（`tsc -b --noEmit && tsc -p e2e/tsconfig.json --noEmit && node scripts/build-panel.mjs`）⇒ `cd panel && npm run build` 这条既有 verify 命令**真的**覆盖类型；附**负控**读数（故意错类型 ⇒ 红；撤销 ⇒ 绿）。
      **t55 的更正**：那次「处置」的前提是**错的**（类型检查本来就在 wrapper 里），而且让同两步 `tsc` **每轮跑两次** ⇒ **已撤销**，`build` 回到 `node scripts/build-panel.mjs`（`git diff -- panel/package.json` 为空）。t53 的**负控读数本身是真的**，它证明的是「这条命令能挡住类型错误」，**不是**「它以前挡不住」—— 现象先测、解释后写（第 14 条）。
20. **判断一条命令做了什么，要读它真正执行的东西（包装脚本 / 子进程），不要只读入口名字（captain 2026-09-28；t55 实例）**：
    - **实例**：`"build": "node scripts/build-panel.mjs"` **看起来**只是打包，而该 `.mjs` 里跑了 `i18n-check` + `tsc ×2` + `vite`（`panel/scripts/build-panel.mjs:33-44`）。t53 只读入口名就得出「原本不含 tsc ⇒ 类型错误能静默通过」的错误前提，t54 又把它写进 `AGENTS.md` ⇒ **同一个错误被传播两次**（一次进契约、一次进仓库说明）。
    - **纪律**：**报「某条命令覆盖什么」之前，先读它真正执行的东西**（脚本内容、`spawn` 的子命令、`pre`/`post` 钩子、CI 的 `run:` 块），报告里给**行号级引用**；**入口名字只是声称面**。
    - **同族**：与第 14 条（根因要可反做）、第 17 条（量词默认行为）同族 —— **别把「读起来像」当成「做起来是」**；而且 **错误的说法比错误的读数更长寿**（t53 的错误说法活到 t54，直到 wiki 撤回自己的 finding 时才被查出来）。
21. **「没有 diff」不等于「没改」（captain 2026-09-28 转 t56 实测；t55 又撞了一次）**：
    - **实例**：`docs/design/reviews/` 下同类报告**基本未被 git 跟踪**（`git ls-files -v` 无输出、`git status` 报 `??`、`git diff --numstat` 为空）⇒ **在这些文档上，「空 diff」只说明未跟踪，不说明未改**；谁拿 `git diff` 给这类文档的变化背书，那就是**假绿**（证据工具本身不适合该对象）。
    - **t55 现场**：`AGENTS.md` 与 `panel/package.json` 是**被跟踪**的（`H`）⇒ 对它们 `git diff --numstat` 分别是 `30 2` 与**空**，**这两条是证据**；而 `gen2-integration-contract.md` / `gen2-integration-impl.md` **未跟踪** ⇒ 只能用**内容级读数**（行数、关键字计数、哈希、数字序列）。正是靠内容级读数才发现 **t53 的 R-22 修订记录当时根本没落盘**。
    - **纪律**：**判定文件是否变化，先问「它被跟踪吗」（`git ls-files`）**；未跟踪/新增文件用**内容级读数**取证，并在报告里写明用的是哪种证据。
    - **附带**：改**措辞**也会顺手改**读数**（t56 的实例：一句「对 4 页逐位相等」改成「逐位相等」就**丢掉了采样面**）⇒ 改文档前后把**关键数字序列与不变量**抽出来逐位比对（t55：契约 2546 个数字串、`AGENTS.md` 55 个，作为 after-state 留档；**before-state 对未跟踪文件不可得，这本身就是第 21 条的证据**）。

**`Checking <crate>` 的覆盖面限制（wiki/graph 共同确认，第 9 条的补充）**：cargo 的 `Checking <crate>` 是**按包**打印的 ⇒ 它只能证明**这个包**被重查，**不能逐 target 枚举**。因此报告的**正确声称面**是「**这次调用 `exit=0` 且这个包被重查过**」；**不声称逐 target 证据**（`--verbose` 的 `clippy-driver --crate-name …` 行是更强的形态，**拿不到就不许声称**）。措辞照 wiki 的：**缺证据就收窄声称面**。

**本代唯一一处 workspace 级 lint 门是 t19 的 `cargo clippy --workspace --all-targets -- -D warnings`**，所以第 6–10 条主要落在 INT 身上。**自测的形状也要报**：像 `clean -p ruagent-daemon` ⇒ `exit=0 / 0.6s / Removed 918 files, 2.4GiB` + `clippy … --all-targets` ⇒ `exit=0 / 8.1s / Checking ruagent-daemon v0.1.0`（R-10c）那样，把「重查过」写成能复核的两行，而不是一句「已通过」；**t19 起改用 `-CleanFirst` 的单窗口形态**（脚本 v4，8,298 B / mtime 2026-09-28 1:40:18）。

**冻结字面量的 serde 漂移陷阱（F-5，captain 2026-09-28 转 V-A 的发现；只读取证）**：`crates/knowledge/src/store.rs:71` 的 `ScoreKind` 只有 `#[derive(…, serde::Serialize)]`、**没有 `rename_all`** ⇒ serde 实读 **`"RrfRank"`**，而 `as_str()` 与 daemon 手写 JSON 用的是**冻结字面量 `"rrf_rank"`**（`as_str()` 的五个取值：`rrf_rank`/`semantic_l2sq`/`bm25`/`cosine`/`calibrated`）。对照：同 crate 的 `ResidualOrigin` **有**一条漂移钉（`crates/knowledge/tests/residual-scan.rs:40` 逐字断言 serde 输出）。**因此**：

- **H-1 若直接 serde 序列化 `RankedHit`/`SearchPage`，量纲字段会静默换名**（`rrf_rank` → `RrfRank`）—— 这正是契约「量纲字段必须冻结字面量」（K-4/A-13 ②⑤）要防的事，但会从没人预料的入口发生。**在 `ScoreKind` 加上 `rename_all = "snake_case"`（或逐变体显式 rename）之前，HTTP 面必须继续用手写字面量**（`api.rs:3003/3006/3008` 行级恒 `"rrf_rank"`，V-A 独立复核为「作者没有多报」）。
- **需要的修法（captain 已裁决归属：(a) 但并入他已有的 `t29`，owner=recall，deps=t7；见 R-14）**：① 加 `rename_all = "snake_case"`（或逐变体显式 rename）使 serde 输出与 `as_str()` 的五个冻结字面量逐字一致；② 补一条**漂移钉**（形态照 `crates/knowledge/tests/residual-scan.rs:40` 对 `ResidualOrigin` 的钉）并给「改前 `"RrfRank"` → 改后 == `as_str()`」的读数；③ 本条即契约落点（我）。**F-5 与 F-1 同源（都出自 V-A 对 t7 的独立验证）、落点同在 `crates/knowledge/**`，故并入同一张 repair 单，不新开单**（一个成员同时只能拥有一张未完成单）。
- **跨 crate 钉（INT 侧，`crates/daemon/tests/**`）—— captain 已批，但顺序写死**：**不要先提交一条红测试**；等 t29 的 ① 落地后再加，断言 `serde_json::to_string(&ScoreKind::RrfRank) == "\"rrf_rank\""`。**两条钉不重复**：crate 内钉**定义**、跨 crate 钉**消费面**（R-14b）。
- **任何新增的 `*_kind` 键都必须取自 `ScoreKind::as_str()` 这个唯一来源**，不许再抄一遍字面量（R-A A7 / R-8b 同一条纪律）。

**脚本的尾巴等价开关（v3，2026-09-28T00:11:42；我逐个用 `-DryRun` 验过形状）**

| 想写的裸 cargo | 交给脚本的写法 | `-DryRun` 实测解析出的命令行 |
| --- | --- | --- |
| `cargo test … -- --nocapture` | `… cargo-team.ps1 test … -Nocapture` | `cargo test -p ruagent-daemon --test knowledge_api -- --nocapture` |
| `cargo clippy … -- -D warnings` | `… cargo-team.ps1 clippy … --all-targets -DenyWarnings`（裸 `--` 也能过） | `cargo clippy --workspace --all-targets -- -D warnings` |
| 两棵树对比 | `… -TargetDir "$env:TEMP\ruagent-cmp-<本单号>"` | `target=…\ruagent-cmp-t5`（独立目录，不共享） |
| **只验形状、不编译不取锁** | 任何命令末尾加 **`-DryRun`** | `[cargo-team] DRY RUN (nothing compiled, no lock taken)` + 最终命令行 + `target/jobs/cpus/priority` |

**`-DryRun` 是本代的形状自测纪律**：v2 的自测只跑过 `--version`，恰好绕过「`test` 被绑给第一个声明参数」的 bug（captain 2026-09-28 披露）；**凡新增/改动一条锚点命令，先用 `-DryRun` 看它解析出的 cargo 命令行与判据里写的是否一致，再真跑**。v3 不声明任何参数、自解析 `$args`，所以「同样的 cargo 子命令交给脚本」这个写法成立。

**两棵树对比**（`git worktree` / `git archive`）**必须**用 `-TargetDir "$env:TEMP\ruagent-cmp-<本单号>"` 单独目录（两棵树共享 target 会让 cargo 把第一棵树的 rlib 给第二棵，closure §7.1）；脚本另提供 `-NoLock`，**只**用于「故意的第二 target 目录」，不许拿它绕过「一次一个编译」。

**临时 root 形状**（`crates/daemon/tests/knowledge_api.rs:12` 的 `start_test_daemon()` 与 `crates/mcp/tests/roundtrip.rs:39` 是既有范例，INT 沿用）：

```
$env:TEMP\ruagent-<suite>-<pid>-<seq>\
  config\agents.toml    # [agent.mock] harness="mock" command="mock-agent" enabled=false
  config\mcp.toml       # [profile.default] servers = []
  config\policy.toml    # [permissions] default = "ask"
  data\ruagent.db       # 本次测试新建（迁移在这里跑，活库不受影响）
  knowledge\            # 本次测试写入的 md
  transcripts\          # context_injected 事件的落点
```

**真实进程（只在 A-9 用）**：`ruagent.exe serve --addr 127.0.0.1:<临时端口> --root <临时 root>`，**记下自己的 pid 到 `<root>\canary.pid`**，收尾只 `Stop-Process -Id <自己记录的 pid>`；**绝不**启停 pid 79984、绝不按名字/端口批量杀。

### 6.1 正向锚点

| # | 判据（契约条目） | 确切命令 | 期望读数（可证伪） |
| --- | --- | --- | --- |
| **A-1** | §1.1/§1.4/§1.5 的 HTTP 形状与破坏性变更 | `cargo test -p ruagent-daemon --test knowledge_api` | 全绿；其中：`links_out == degrees.links_out`（4 页逐位）；`freshness` 三态在场；`{dry_run:true,confirm_plan:N}` ⇒ **400**；`memory_diffs.op` 里的新值可读；`/knowledge/search` 的 `limit=500` ⇒ 实际返回 ≤50 |
| **A-2** | §3.2 注入遥测 + `<graph>`/`<wiki>` 块 + BREAK-CORE-1 | `cargo build -p ruagent-mock-agent` 然后 `cargo test -p ruagent-daemon --test injection_e2e` | **run 路径与 chat 路径都要交读数**（DEP-INT-1 已改派，acp/chat.rs:497 在 t19 inScope）：各自给出「含 `budget`/`path` 的注入事件数 ÷ 总注入事件数」；`path="run"` / `path="chat"` 各自出现；`budget.used_chars ≤ 4096`、`blocks[]` 的 tag 集合与渲染一致；造一次预算绑定 ⇒ `dropped_items>0` 且渲染里出现丢块通知。**前置**：mock-agent 未构建时该测试**打印 skip**（不许静默通过）⇒ 记 `not_measured` |
| **A-3a** | §3.1 **已落地列**真的被写 | `cargo test -p ruagent-daemon --test knowledge_api -- recall_log` | 新行：`source` 非空、`knowledge_leg_window=60`、`scoring_version` 非空、`graph_entities`/`graph_paths` 与响应 `graph.stats` 一致；**历史行这些列全 NULL**（不是 0） |
| **A-3b** | §3.1 的 6 列（`score_kind`/`fusion`/`top_legs_json`/`candidates_json`/`selected_json`/`rejected_json`）**既有列也有值** | 同 A-3a 的命令 + `python -c "import sqlite3;c=sqlite3.connect('<临时 root>/data/ruagent.db?mode=ro',uri=True);print([r[1] for r in c.execute('pragma table_info(recall_log)')])"` | **列已在（0023，24 列）⇒ 自 2026-09-27T23:05:22+08:00 起判可判，不再 `not_measured`**。判据：新行 `score_kind="rrf_rank"`、`fusion` 与 `scoring_version` 联读可比、`top_legs_json` 可解析且**不含未命中的腿**、`rejected_json.knowledge_by_min_score` **有计数**（改前是静默 `continue`）**列在 ≠ 值在**：这一条测的是 INT 的写入侧。**t19 报告按列给出填充读数**（口径，来源：captain 2026-09-27 的消息；**不改变任何列的语义**）：14 列每列一行「**有值 / 恒空 / 不适用 + 原因**」，**恒空又非不适用的一律登记为未实现项**（例：`rejected_json.knowledge_by_min_score` 今天恒空，因为 `api.rs` 的 `min_score` 过滤是静默 `continue`）；V-INT(t20) 独立复核。历史 651 行这 6 列全 NULL |
| **A-4** | §2 MCP 表 | `cargo test -p ruagent-mcp --test roundtrip` | 工具清单里出现 `graph_search`/`graph_retrieve`/`wiki_pages`/`wiki_links`/`memory_forget_report`；`memory_write{store:"bogus"}` ⇒ **工具错误**（不是 200 文本）；`memory_recall` 触发的调用在 `recall_log` 里 `source='mcp_recall'` |
| **A-5** | §4 面板类型/文案/构建 | `cd panel; npm run check; npm run i18n:check; npm run build` | `check` 0 错；`i18n:check` 0 错（zh/en 键集一致）；`build` 成功产出 `panel/dist/index.html` |
| **A-6** | §4 四页读数（只读 e2e） | `cd panel; npm run test:e2e -- --output=$env:TEMP\pw-int` | `recall.spec.ts`（Memory 的 recall log 行含量纲）、`knowledge.spec.ts`、`wiki.spec.ts`（含 `freshness` 三态、`wanted` 的 demanders）、`views.spec.ts` 全绿；`registry.spec.ts` **跳过**（`writeAccess()` 未开 ⇒ 它不写活库 config） |
| **A-7** | 全 workspace 不因本代变红 | `scripts/cargo-team.ps1 fmt --all -- --check`；`scripts/cargo-team.ps1 clippy --workspace --all-targets -- -D warnings`；`scripts/cargo-team.ps1 test --workspace`（前缀见 §6.0；**不要**自己设 `CARGO_TARGET_DIR`、**不要**并发两条 cargo） | 三条 exit 0。**同伴在途编辑导致的失败**：报告「哪个文件、什么错」，不替同伴改。**看到 `another team build is running … waiting 10s` 属于正常排队，等待即可** |
| **A-8** | 契约自身与「本单零代码改动」 | `Select-String -Path docs/design/reviews/gen2-integration-contract.md -Pattern 'gen2-(recall\|memory\|graph\|wiki)-spec' -Quiet`；`git status --porcelain -- crates/daemon/src/api.rs crates/daemon/src/config.rs crates/daemon/src/lib.rs crates/daemon/src/chat.rs crates/daemon/src/runs.rs crates/daemon/src/sessions.rs crates/daemon/tests crates/mcp crates/core crates/orchestrator crates/acp panel` | 第一条 `True`；第二条**在 t5 首次交付时为空**（2026-09-27T23:34:20+08:00）。**R-8c/R-8e 之后**该命令允许出现且只允许出现两个文件：`crates/daemon/src/api.rs`（`.repeat().take()` → `repeat_n`）与 `crates/daemon/tests/injection_e2e.rs`（let-chain）；判据 = `git diff -U0` 只含这两行。**其余**（`crates/store/**`=I-SCHEMA、`crates/daemon/src/{memembed,wiki,distill}.rs`=I-B/I-D/I-C、`crates/memory/**`、`crates/graph/**`）都不是 INT 的改动；整目录 porcelain 在并发波次里不是越界证据（同 captain 给 verify 的 F5 纪律） |
| **A-9** | 端到端手测（真实进程，临时 root/端口） | 见 6.0 的进程纪律 + `Invoke-WebRequest "http://127.0.0.1:<PORT>/api/v1/memory/write" -Method POST -ContentType application/json -Body '{"store":"bogus","namespace":"user","content":"t5-probe"}'` | HTTP **400**；且 `SELECT COUNT(*) FROM memory_diffs` 与 `SELECT COUNT(*) FROM episodes` **前后不变**（拒绝不留副作用，负例见 A-11） |
| **A-10** | 残余面入口（F-2） | 同上进程 + `Invoke-WebRequest "http://127.0.0.1:<PORT>/api/v1/forget-report?content_hash=<64hex>"` | 200 + `{local:[…],external:[…],read_at}`；**哈希载体的 carrier 不许报 `Readout`**；`total` 缺省为 `null`（不是 0）。`gen2-memory-spec` U-6 因此闭合 |
| **A-11** | 时序/可归因（F-6/F-8） | 同上进程 + `GET /api/v1/distill` 与 `GET /api/v1/stats` | `/distill` 有 `prompt_hash`；`/stats` 同时给 `embedder` 与当前 `scoring_version`；一次 `PUT /distill`（改 prompt）后 `prompt_hash` **必须变** |
| **A-12** | 只读取证（禁用项） | `Select-String -Path crates/daemon/tests/*.rs -Pattern '/api/v1/recall'` | 只允许出现在**明确写着会写 `recall_log`** 的测试里；契约/验证报告里出现「用 `/recall` 取证」一律判无效（真库 pid 79984 禁止触碰） |
| **A-13** | DEP-INT-8 的注入接线（I-B 的适配形状真的被用上） | `cargo test -p ruagent-daemon --test injection_e2e`（+ `cargo test -p ruagent-memory inject`） | ① 知识命中进注入时用的是 `RankedHit.relevance`（**不是** `hit.score`）：构造一条 `relevance` 与 `score` 排序相反的数据，注入块顺序必须跟 `relevance`（`EnrichedHit::order_value()`）；② wiki 命中进注入时 `lead` 非空 ⇒ 块里出现 marker 行（`stale=`/`coverage=`/`anchors=`）+ `hint` 行，且 `hint` 与 HTTP stub 的 `hint` **逐字相等**（K-15）；③ 低置信行是**前缀** `[unverified] `（`inject.rs:98 LOW_CONFIDENCE_MARK`，**不是行尾**：行尾会被 per_block 截断吃掉）；④ 不带 lead/relevance 时 render **字节与改前逐字相同**（golden 仍绿）；⑤ `TAG_GRAPH` 的 rank = 3（0..6 表见 §3.2）；⑥ **O-1 = 方案 ① 已落地（t24/mem-core，我 2026-09-27T23:42:56+08:00 独立核对；t24 已终态）**：`inject::WikiLeadMeta.edited: Option<bool>`（`None → edited=unknown`），测试 `an_unknown_edited_state_never_renders_as_false`（`inject.rs:1124`）断言 `edited=unknown` 出现且 `edited=false` **不**出现，`Some(false)`/`Some(true)` 各一读数 ⇒ **本子项可判**；用例在**改前**（`bool`）必须红。**编译面**：`test -p ruagent-daemon --test injection_e2e` 需要 daemon 的 `--lib`/`--lib test` 先绿，否则这个 test target **根本不会被编译**（§6.0 的编译面纪律）；报读数时必须写明覆盖到 `--test injection_e2e`；**若这条判据的读数来自 clippy，还须给「本次重查了 `Checking ruagent-daemon`」这一行**（§6.0 第 6–8 条，防假绿） |
| **A-14** | **§1.5 的六条 graph 路由**（DEP-4 形状，captain 2026-09-28 转达；t19 验收第 1 条「契约里每条路由都实现」自动覆盖） | `cargo test -p ruagent-daemon --test knowledge_api -- graph`；端到端（临时 root）：先 `GET /api/v1/graph/communities?level=0` 再 `POST /api/v1/graph/communities/build` 后重读 | ① 六条路由都在且形状与 §1.5 一致；② **「`None` = 从未建过」与「空数组」可区分**（未建库上必须是 `communities:null` / `built:false`，**不许**渲染成 0 个社区）；③ `POST /communities/build` 回 `CommunityBuild` 的五个计数（**不许** 200 空体），且 `entities_covered ≤ non_isolated`；④ `PUT /community/{id}/summary` 对不存在 id ⇒ **404**（`updated=false` 不许静默 200）；⑤ `GET /resolution/pending` 把 `pending`（未决队列）与 `redundant`（同对象两行）**分开报**；⑥ `POST /resolution/merge`：`keeper==absorbed` ⇒ **400**、任一不存在 ⇒ **404**、成功后 `moved>0` **且被吸收名进了 `entity_aliases`**（否则下次扫描又建回来）；⑦ `/graph/retrieve` 的 `stats.empty_reason`/`truncated_by` 必须透出（空结果不许静默）；⑧ **`as_of` 接线（门禁已于 t27 completed 后解除，captain 2026-09-28）**：给 **3 条「同一瞬间 × 三种写法（规范形 / `Z` / `+08:00`）结果逐字节一致」**的读数，入口归一化，非时刻字符串 ⇒ **400**；**不得**把 `stats.graph_edges` 说成「那个时点的边数」（它每次都是 60，与 `as_of` 无关）。**证据要求（§6.0 第 6–8 / 11 条）**：报读数时给两件证据 —— 覆盖的 target（含 `--test knowledge_api`）+ 本次 `Checking ruagent-daemon` 那一行；**若读数取自 clippy 面，先 `clean -p ruagent-daemon` 再跑**（防「尾参不进指纹」的假绿）；**不得用空跑（`0 passed … N filtered out`）充当测量**（第 11 条） |

### 6.2 负例锚点（坏侧必须真的红，否则两侧都没测到）

| # | 负例 | 命令 | 判据 |
| --- | --- | --- | --- |
| **N-1** | 未知 store 写入被拒 | `curl -s -o - -w '%{http_code}' -X POST http://127.0.0.1:<PORT>/api/v1/memory/write -H 'content-type: application/json' -d '{"store":"bogus","namespace":"user","content":"t5-neg"}'` | `400`；`memory_diffs`/`episodes` 行数不变（**改前**这条命令返回 200 且落库为 observation —— 这就是它有效的证明） |
| **N-2** | `dry_run` + `confirm_plan` 组合被拒 | `POST /api/v1/knowledge/wiki/build` body `{"dry_run":true,"confirm_plan":1}` | `400` 且错误信息**点名两个字段**；`wiki_builds` 不新增行（改前 200 + 新建一行） |
| **N-3** | 空 `reason` 的纠错被拒 | `POST /api/v1/knowledge/wiki/pages/<slug>/corrections` body `{"kind":"pin","reason":"","author":"human"}` | `400`；`wiki_corrections` 不新增行 |
| **N-4** | 分页不改排序（C5） | `python %TEMP%\ra_limit_probe.py`（`gen2-recall-spec` §F.3） | 冻结点：`limit ∈ {5,10,20,30}` 对同一组查询的 **top-1 文档名与分数逐条相同**；**改前**必须至少 2/13 不同（否则用例无效） |
| **N-5** | 每个分数都有 kind（C6） | `cargo test -p ruagent-daemon --test knowledge_api -- score_kind` | 断言存在：任何 `score`/`semantic_score`/`keyword_score`/`relevance` 旁都有同级 `*_kind`；**改前**缺 3 个 |
| **N-6** | 注入丢块必有计数 | A-2 的预算绑定运行 | 渲染里出现丢块时，事件里 `budget.dropped_items>0`；**改前**这类事件 80 个里只有 1 个有通知、`<context_budget>` 0 个 |

### 6.3 判 not_measured（不许写成 0）的项

| 项 | 为什么不许当 0 | 判法 |
| --- | --- | --- |
| chat 路径的预算遥测 | ~~构造点不在 INT inScope~~ | **已解除**：captain 2026-09-27 把 `crates/acp/src/chat.rs` 并入 t19 的 inScope（DEP-INT-1 已改派）⇒ chat 路径与 run 路径**都必须**给出读数（A-2），不再是 not_measured |
| `<graph>` 在生产 transcript 里出现（G10） | 需要一次真实注入运行 | 若 A-2 未跑到含 `graph` 的路径 ⇒ `not_measured`，run 路径的契约部分照常判 |
| 真库上的 wiki `stale` 正例 / 手改页 | 真库 0/4、从未发生（`gen2-wiki-spec` A.4） | `not_measured` + 原因；正例只在临时 root 的 canary 上取 |
| 分级 nDCG、多样性（MMR λ）、社区摘要胜率 | 前置语料/gold 不存在 | `not_measured` + 原因；**禁止**用二值退化 nDCG 冒充分级 nDCG |
| `recall_log` 新列在有历史行上的分布 | 历史行为 null | 报「历史 651 行全 NULL」这个事实，**不报 0** |

### 6.4 本契约自身的 verify 与纪律回执

| 项 | 状态 |
| --- | --- |
| 本单（t5 交付本身）只写 inScope | 交付物的写动作 = 创建/修订 `docs/design/reviews/gen2-integration-contract.md`；**t5 首次交付时**（2026-09-27T23:34:20+08:00）按文件列举的 INT 侧 `git status` = **空**。**此后 R-8c/R-8e 追加了两次 clippy 解除阻塞的最小修复**（见下一行），因此现在该命令会显示那两个文件 |
| INT 自己的代码改动（**R-8c/R-8e**，只有两处 clippy 修复） | `crates/daemon/src/api.rs:3814`（`repeat().take()` → `repeat_n()`，mem-core 报坐标、captain 裁归我）· `crates/daemon/tests/injection_e2e.rs:773`（`collapsible_if` → let-chain）。两处都是 `-D warnings` 的解除阻塞修复，无语义变化；**A-8 的判据因此改为「只允许这两个文件出现，且 diff 只含这两行」。** `crates/daemon/src/distill.rs:1098/1100`（`type_complexity`）**归 graph**（captain 2026-09-28 裁决），INT 不碰 |
| 同伴在途产物（**不是我改的**） | `crates/store/src/{fts.rs,lib.rs,migrations.rs}` + `migrations/0019..0023*.sql`（**I-SCHEMA/t6**）· `crates/daemon/src/memembed.rs`（**I-B/t8**，R-1 裁决归它）· `crates/daemon/src/wiki.rs`（**I-D/t10**）· `crates/daemon/src/distill.rs`（**I-C/t9 与 mem-core 共管**）· `crates/memory/**`、`crates/graph/**`（I-B/I-C）—— 本契约**读取**它们以对齐 K-1/K-2/K-7/DEP-INT-8，**未改动任何一行** |
| 未改代码 | 是（无 `crates/`、`panel/` 改动） |
| 未启停真守护进程 pid 79984 / 127.0.0.1:8787 | 是（只读代码与只读活库读数，全部引用四份规格的自报窗口；**未调用 `/api/v1/recall`**） |
| 契约内的路径 | 每条实现/测试路径都落在 INT 的 inScope（`crates/daemon/src/{api,config,lib,chat,runs,sessions}.rs`、`crates/daemon/tests/`、`crates/mcp/`、`panel/`、`crates/orchestrator/`、`crates/core/`）；越界项一律写成 §5.3 的「区域提供库接口，INT 只接线」 |
| 读数纪律 | 本文件所有「今天」的读数都来自四份规格并带时间点；活库在自变（entities 50→63、recall_log 650→651）已在表头声明；**在途迁移的读数带自己的观察时刻**（22:50:59–22:53:52+08:00） |

---

**附：修订记录（追加式；只标注来源与时刻，不改写历史结论）**

**R-1（2026-09-27T23:05:22+08:00，I-SCHEMA 回信后）** —— 来源：`agent_teams_send_message` ← recall（I-SCHEMA/t6），主题「编号确认：0023 归我，已落地」，消息 id `a8344184-076b-4fb7-af44-0548bf1bb308` 的回执；我的独立核对：`crates/store/src/migrations/0023_recall_telemetry.sql` 存在（mtime **2026-09-27T22:56:55**），六个 `ALTER TABLE recall_log ADD COLUMN` 逐字为 `score_kind`/`fusion`/`top_legs_json`/`candidates_json`/`selected_json`/`rejected_json`。

| # | 改了什么 | 原文（保留不改） | 依据 |
| --- | --- | --- | --- |
| R-1a | §3.1「落地进度」：缺口 6 列 → **全部落地**；补 `recall_log` 现共 **24 列**与「列序被 I-SCHEMA 的新库列序断言逐位钉住」 | 原文写「**缺口：6 列未落地** ⇒ 需 I-SCHEMA 追加迁移（建议 `0023_recall_telemetry.sql`）」 | 0023 已落地（22:56:55），编号按 I-SCHEMA 自己的分配（`0023_recall_telemetry.sql`），**不是**我原建议之外的号 |
| R-1b | **A-3b 从 `not_measured` 升级为可判**（判据不变，但「列不存在」这一 not_measured 原因消失；并把「列在 ≠ 值在」写清：这条测的是 INT 的写入侧） | 原文写「列存在前：判 `not_measured（原因：列不存在…）`」 | §3.1 落地进度 + 0023 的 DDL |
| R-1c | K-2 的裁决从「登记缺口」改为「缺口已闭合」，并保留原理由 | 原文写「**尚未落地** = …（6 列，需 I-SCHEMA 追加一个迁移，建议 …）」 | 同上 |
| R-1d | **plan-only 的消费规则收紧**（§1.4 / BREAK-WIKI-3 / K-7 / §4 Wiki）：从「两个值都当 plan-only」改为「按派生布尔 `dry_run===true` 判，**禁止**按 `'planned_only'` 字面量过滤」 | 原文写「消费方把两个值**都**当 plan-only；`dry_run` 降级为兼容字段」 | recall 的实测：0022 已把 8 行里 **2 行** legacy `planned_only` 回填成 `planned`，而 I-D(t10) 改 `DRY_RUN_STATUS`（`wiki.rs:1838`）之前**新行仍写 `planned_only`** ⇒ 只看字面量会漏一半；触发器保证 `dry_run` 与两个 plan-only 状态一致 |

**未改的结论（逐条复核后仍然成立）**：K-1 的编号分配（0019–0022，另加 0023 只属 recall）；K-3 的「库里可嵌、线上保持扁平键 + 只加 `*_kind`/`relevance`/`fusion`/`leg_window`/`candidates`」（recall 确认遵守，未动任何线上键名）；K-9 的 `"bigram"` 由 `api.rs:2967` 自动得到、零改动；A-3a 的「历史行不可比」（`top_knowledge_score` 一个字节未改）。

**R-2（2026-09-27，captain 回复后；只改「已改派/已接受」的事实，不改任何冻结语义）** —— 来源：`agent_teams_send_message` ← captain（收到于 2026-09-27T23:1x+08:00），三点：

| # | 改了什么 | 依据 |
| --- | --- | --- |
| R-2a | **DEP-INT-1 从 HANDOFF 改为已改派**：`crates/acp` 原先全 DAG 无人拥有（真实缺口），现并入 **t19 的 inScope**；BREAK-CORE-1 与 §6.3 的对应措辞同步 | captain：t19 的 amend 已加 `crates/acp/src/chat.rs`；§6.3 的 `not_measured` 回退项作废 |
| R-2b | **A-2 增加两条路径的比值式读数**：run 与 chat 各自给出「含 `budget`/`path` 的注入事件数 ÷ 总注入事件数」 | 同上（captain 明说这是 t19 新增的验收项） |
| R-2c | **A-3b 增加按列的填充口径**：14 列每列给「有值 / 恒空 / 不适用 + 原因」，恒空又非不适用 ⇒ 登记未实现项（**不改变列的语义**，故非契约抖动） | captain：按「列在 ≠ 值在」的读法钉进 t19 口径，「不再另 amend，以此消息为记录」 |

**执行前置（不属接口变更，但 V-INT 应知道）**：t19 的依赖已加 **t23（RV-SCHEMA）** 与 **t24（repair, O-1 的三态修复）** —— 集成**必须等这两道的任务终态 pass** 才能开工（captain 2026-09-27）。**「代码在盘上」与「任务已 pass」要分开读**：t24 的代码已落地（R-5a，A-13 ⑥ 因此可判），但 t19 的开工门槛仍是任务终态。本契约的 §3.1 列清单在 t19 开工时以 `schema_migrations` + I-SCHEMA 的列序断言为准，契约只冻结**列名与语义**。

**R-4（2026-09-27，captain 对 O-1 的裁决）** —— 来源：`agent_teams_send_message` ← captain。**裁决：走方案 ①** —— `inject::WikiLeadMeta.edited` 改为保真三态 `Option<bool>`；**不选** ②（`edited_unknown: bool` 会让同一事实有两个真相源、且二者可能不一致），**不选** ③（在该 lead 上不注入 = 信息损失）。captain 给的理由与本代已有的定型一致：R-B 的 `ExternalStatus{Readout|NotAvailable|Unknown}`、closure §6.4 的「NULL ≠ user」——**一个只能取二值的字段承载三态事实，缺陷在字段类型本身**。

| # | 影响 | 状态 |
| --- | --- | --- |
| R-4a | 交付归 **t24（repair, mem-core）**：判据写死了 O-1 的反例（`WikiLead{edited: None}` ⇒ `<wiki>` 块**不渲染** `edited=false`）+ `Some(false)`/`Some(true)` 各一读数 + 「无 lead / 无 relevance 时渲染逐字不变（golden 仍绿）」；「不出现该标记」还是「渲染为 `unknown`」由 t24 判，但必须在报告里写明理由（判据：**读者不能把「不知道」读成「没有编辑过」**） | 待 t24 |
| R-4b | **V-B(t12) 与 t19 现在都依赖 t24**；A-13 第 ⑥ 子项保持 `pending` 直到 t24 交回 | 依赖已挂 |
| R-4c | INT 的既有做法不变：**不落任何把 `None` 写死成 `false` 的代码**；t19 开工时 `WikiLeadMeta` 已是三态形状 | 已确认 |
| R-4d | 未改的确认：DEP-INT-8 / K-15 的接线（知识命中走 `search_page().hits[i].relevance`、禁止用 RRF 名次分当相关性；wiki 命中走 `lead_for(kb, recorded_hash, slug)`）与 t19 验收里「每个分数键旁挂 `*_kind`、hit 增 `relevance`/`relevance_kind`」一致；A-8 的**按文件列举**判据与 captain 发给 `verify` 的 F5 纪律（并发波次里整目录 porcelain 不是越界证据）是同一条 | 无变化 |

**R-5（2026-09-27T23:42:56+08:00，I-B 回信 + 我的只读核对；O-1 的交付落地）** —— 来源：`agent_teams_send_message` ← mem-core（「裁决：选 ①，我已落地（在你的消息到达前我也独立撞到同一个冲突，结论相同）」）。**独立核对**（只读）：`crates/memory/src/inject.rs` 的 `WikiLeadMeta.edited: Option<bool>`（三态注释 + 「CORRECTED at integration time (t8 + integ's O-1 / DEP-INT-8)」）· 测试 `an_unknown_edited_state_never_renders_as_false`（`:1124`，断言 `edited=unknown` 出现、`edited=false` 不出现，`Some(false)`/`Some(true)` 各一读数）· `crates/daemon/src/memembed.rs:648 lead_meta` / `:669 relevance_meta` / `:683 enriched_hit`（**全部 `pub`**）· `crates/knowledge/src/store.rs:182 RelevanceScore` / `:245 RankedHit.relevance` / `:1036 search_page` / `:1149 residual_scan` · `crates/daemon/src/wiki.rs:2451 WikiLead` / `:2470 LEAD_HINT` / `:2780 lead_for`。

| # | 改了什么 | 依据 / 影响 |
| --- | --- | --- |
| R-5a | **A-13 第 ⑥ 子项从 `pending` 改为可判**：`edited: Option<bool>` 已在盘上，`None → edited=unknown`，反例测试存在（改前 `bool` 上必须红） | 只读核对（上）；O-1 的判据未变 |
| R-5b | **DEP-INT-8 增「单一映射点」并加硬约束「INT 不许自己再写一份适配器」**：I-B 在**它自己的** `crates/daemon/src/memembed.rs` 提供 `lead_meta` / `relevance_meta` / `enriched_hit`；INT 的接线收敛为「`k.search` → `kb.search_page` + 调这三个 helper + `knowledge_items_enriched`」 | mem-core 的理由：避免 `chat.rs`/`runs.rs` 各写一份适配器（两份 = 迟早漂移）。`memembed.rs` 按 R-1 归 I-B ⇒ **INT 一行不改它**，只调用（与 K-6 一致） |
| R-5c | **登记「未富集调用点 = 2」这个可判读数**：`chat.rs:558/569` 与 `runs.rs:1882/1893`（`k.search(...)` + `knowledge_items(...)`）是**仅剩**的两处；改完必须为 **0** | 我独立核对两个文件：其余没有 `knowledge_items`/`search_page` 调用点 ⇒ 这条同时是 t19 的**完成判据**，不依赖任何未落地的东西 |
| R-5d | **登记无依赖阻塞**：`RelevanceScore`/`RankedHit.relevance`/`search_page`/`residual_scan`（I-A）与 `lead_for`/`WikiLead`/`LEAD_HINT`（I-D）**都已在盘上** ⇒ F-2（`forget-report` 的 knowledge 五面）与 DEP-INT-8 的接线在代码层面已无阻塞；t19 仍只等 **t23 + t24** 的**任务终态** | 只读核对（上）；「代码在盘上」≠「任务已 pass」，两者分开写 |
| R-5e | `usage::record_usage` 的写回在 `select_injection_memories`（I-B 的 `memembed.rs`）内部，**INT 不接线**；C3 的 overfetch 同理 | mem-core 明示；与我 K-6/DEP-INT-7 一致 |

**R-6（2026-09-28T00:0x+08:00，captain 全队资源纪律；改「锚点怎么跑」，不改任何接口）** —— 来源：`agent_teams_send_message` ← captain（机测：5 个 cargo 并发、两个 rustc 同时在各自 target 里编 `lancedb`、32.5 GB 内存只剩 3.77 GB、16/16 核满载；已按记录 PID 停掉在途编译）。我的只读核对：`scripts/cargo-team.ps1` 存在（4,370 B，mtime **2026-09-28 0:06:19**），参数 `[-Jobs N] [-Cpus N] [-TargetDir PATH] [-NoLock] <cargo args…>`，默认 `-Jobs 4 -Cpus 12`、共享 target `$env:TEMP\ruagent-team-target`、持锁最长 **90 分钟**、等待语 `another team build is running (one compile at a time); waiting 10s ...`、`BelowNormal`。

| # | 改了什么 | 依据 / 影响 |
| --- | --- | --- |
| R-6a | **§6.0 的公共前置重写**：删掉「`$env:CARGO_TARGET_DIR="$env:TEMP\ruagent-integ"`」，改为**每次 cargo 都走 `scripts/cargo-team.ps1`**（表里保留 `cargo …` 短写、前缀统一在 §6.0 给）；保留「绝不用 `D:/rust_cache`」的理由 | captain：**不要再自己设 `CARGO_TARGET_DIR`**；全队共享一份缓存（同一份依赖只编一次） |
| R-6b | **§6.0 增四条纪律**：一次一个编译（等锁最长 90 min，**看到 waiting 不要重试、不要再起一条**）· 共享 target · 只用 CPU 0–11 · BelowNormal；两棵树对比必须 `-TargetDir "$env:TEMP\ruagent-cmp-<本单号>"`（closure §7.1：共享 target 会让 cargo 把第一棵树的 rlib 给第二棵）；`-NoLock` 只用于故意的第二 target 目录 | captain 的通知逐条；脚本参数我核过 |
| R-6c | **A-7 的三条命令改写为脚本形式**，并写明「排队等待 == 正常，不是卡死」 | 同上 |
| R-6d | **未改**：A-1/A-2/A-3a/A-3b/A-4/A-13 表里的 `cargo test …` 短写（由 §6.0 的前缀统一），以及全部**判据文本**（改了跑法，没改测什么） | 契约抖动最小化：这一次只动「怎么跑」 |

**R-7（2026-09-28T00:1x+08:00，包装脚本 v3 + 形状自测教训）** —— 来源：`agent_teams_send_message` ← captain（「v3 不声明任何参数、自解析 `$args`，其余原样转发」；并披露 v2 的参数绑定 bug：`test` 会被绑给第一个声明参数，自测只测了 `--version` 恰好绕过）。我的只读核对 + `-DryRun` 实测（**不编译、不取锁**）：`scripts/cargo-team.ps1`（6,684 B，mtime **2026-09-28 0:11:42**）无 `[Parameter]`/`[int]`/`[string]`/`[switch]` 声明；三种形状解析结果 —— `test -p ruagent-daemon --test knowledge_api -Nocapture` → `cargo test … -- --nocapture`；`clippy --workspace --all-targets -DenyWarnings` → `cargo clippy … -- -D warnings`；`test -p ruagent-graph -TargetDir "$env:TEMP\ruagent-cmp-t5"` → `target=…\ruagent-cmp-t5`。

| # | 改了什么 | 依据 / 影响 |
| --- | --- | --- |
| R-7a | **§6.0 增「尾巴等价开关」表**：`-Nocapture` ≡ `-- --nocapture`、`-DenyWarnings` ≡ `-- -D warnings`（裸 `--` 也可）、`-TargetDir` ≡ 独立 target、`-DryRun` ≡ 只打印命令行（不编译不取锁） | captain 的三条尾巴等价 + 我逐形状 `-DryRun` 实测 |
| R-7b | **增形状自测纪律**：凡新增/改动锚点命令，先 `-DryRun` 核解析出的 cargo 命令行，再真跑（v2 的 `--version` 自测绕过 bug 就是反例） | captain 披露的教训 |
| R-7c | **§6.0 的「同样的 cargo 子命令交给脚本」写法确认成立**（v3 无声明参数）⇒ R-6a 的正文**不改**；仅补上尾巴开关与 `-DryRun` | captain 明示「无需再改契约，只要补两条尾巴的等价开关」 |
| R-7d | **未改**：全部判据文本、A-7 的三条命令、`-TargetDir`/`-NoLock` 的用法（`-TargetDir` 仍是两棵树对比的唯一正确出口，closure §7.1）；wiki 误杀他人包装进程（PID 69716 = graph 的 `daemon --lib distill`）与本契约无关，不记录为契约条目（captain 已披露并让 graph 重跑） | 契约抖动最小化 |

**R-3（2026-09-27，I-B(t8) 落地回信后；改「实际形状」，留一条开放项）** —— 来源：`agent_teams_send_message` ← mem-core（自报「t8 落地完成，注入契约有新增」），我的独立核对（只读）：`crates/memory/src/inject.rs:389` `EnrichedHit` · `:444` `knowledge_items_enriched` · `:423` `knowledge_items`（薄包装）· `:376` `RelevanceMeta` · `:350` `WikiLeadMeta` · `:364` `WIKI_LEAD_HINT` · `:98` `LOW_CONFIDENCE_MARK="[unverified]"`；`crates/daemon/src/wiki.rs:2470` `LEAD_HINT`（与前者逐字相同）· `:2780` `lead_for(kb, recorded_hash, slug)`；`crates/knowledge/src/store.rs:1036` `search_page` + `RankedHit{hit,score_kind,semantic,keyword,relevance}`。

| # | 改了什么 | 依据 / 影响 |
| --- | --- | --- |
| R-3a | **DEP-INT-8 从「`RetrievalHit.lead` 字段」改为实际适配形状**：`EnrichedHit`/`knowledge_items_enriched`，旧 API 变薄包装；INT 的接线改为「知识命中走 `search_page().hits[i].relevance`、wiki 命中走 `lead_for`」 | mem-core 的理由可证伪且我核过：`RetrievalHit` 是公开字段结构体，`chat.rs:560-568`/`runs.rs:1884-1891` 用结构体字面量构造 ⇒ 加字段会让这两处（当时不在 t8 的 inScope）编译不过。**语义与 R-D D.7 / R-A H-4 一致**，改动是**形状**不是能力 |
| R-3b | **K-15 改为「两处逐字相同的字面量 + 一条比对判据」**（`wiki::LEAD_HINT` 与 `inject::WIKI_LEAD_HINT` 已实测逐字相同） | 两个 crate 之间刻意没有依赖（`crates/memory` 不依赖 knowledge/daemon）⇒ 单一常量不可能，只能字面量相同 + 判据 |
| R-3c | **DEP-INT-5/8 记录 `lead_for` 的实际签名是 3 参**（`kb, recorded_hash: Option<&str>, slug`），`gen2-wiki-spec` D.7 冻结的 2 参版本以实际签名为准 | I-D 加 `recorded_hash` 是为了区分「文件被手改」与「来源漂移」；这是 I-D 相对自己规格的加参，**契约以落地签名为准**（消费面本来就是 `Option<WikiLead>`，加参不改消费语义） |

**R-3 留下的唯一开放项（要求 I-B 或 captain 裁决，**未**由我在契约里单方面定死）**

| 项 | 冲突 | 判据（可证伪） | 建议 |
| --- | --- | --- | --- |
| **O-1 `edited` 的第三态在适配形状里丢失** | `wiki::WikiLead.edited: Option<bool>`（**三态**：`Some(true)` 手改 / `Some(false)` 未改 / `None` 这一面看不到 DB）而 `inject::WikiLeadMeta.edited: bool` ⇒ 映射会把 `None` 压成 `false`，与 K-13 / F-11 的「**不可判定不许被 `false` 掩盖**」直接冲突（`gen2-wiki-spec` D.2/D.5 三态的同一原则） | 造一个 `WikiLead{edited: None}` ⇒ 注入块必须**不许**渲染 `edited=false`（渲染 `edited=unknown`、或省略该字段但保留 `stale`/`freshness` 的三态） | ~~① 首选：I-B 把 `WikiLeadMeta.edited` 改成 `Option<bool>`；② 次选：`WikiLeadMeta` 加 `edited_unknown: bool`；③ 最后：INT 不注入该 lead。~~ **已裁决：方案 ①（captain 2026-09-27）⇒ 派 t24（repair, mem-core）交付；本项的判据与理由原文保留，见 R-4** |

**R-8（2026-09-28T01:1x–01:3x+08:00；captain 转达 graph 的 DEP-4 形状 + 两条 clippy 归属裁决 + mem-core 的 t24 终态）** —— 来源：`agent_teams_send_message` ← captain（「graph 的 t9 交回，带一份 DEP-4 的精确形状，请写进契约的路由表」+「api.rs:3814 归你」+「distill.rs:1098/1100 归 graph」）与 ← mem-core（t24 终态 + `relevance_meta` 改为按值）。我的只读核对：`crates/graph/src/community.rs:172 build_communities` · `:271 communities -> Option<Vec<Community>>` · `:314 set_community_summary -> bool` · `:328 community_coverage` · `:352 communities_of`；`crates/graph/src/lib.rs:882 pending_pairs` · `:898 redundant_pairs` · `:927 merge_entities -> u32` · `:865 queue_pending`；`crates/graph/src/retrieve.rs:384 resolve_seeds` · `:633 retrieve`；`crates/daemon/src/memembed.rs:662 lead_meta` · `:685 relevance_meta(value,kind,version,query_background)` · `:702 enriched_hit(hit, Option<RelevanceMeta>, Option<WikiLeadMeta>)`。`api.rs` 今天**没有任何** `/graph/{retrieve,communities,community,resolution}` 路由 ⇒ 六条全新。

| # | 改了什么 | 依据 / 影响 |
| --- | --- | --- |
| R-8a | **§1.5 写入 graph 的六条 DEP-4 路由**：`GET /graph/retrieve?q=&hops=`（与 DEP-1 同入口）· `GET /graph/communities?level=`（**必须可区分 `None`=从未建过 与 空数组**）· `POST /graph/communities/build` · `PUT /graph/community/{id}/summary`（不存在 id ⇒ 404）· `GET /graph/resolution/pending`（`pending` 与 `redundant` 分开报）· `POST /graph/resolution/merge`（显式人工决策，绝不自动合并；成功后必须写 `entity_aliases`）；**DEP-INT-4 同步改为「库签名已就绪」** | captain 转达 graph 的精确形状；graph 明确**不再开第二个消费面（不另开 CLI）**⇒ 与「单一消费面」一致。t19 验收第 1 条「契约里每条路由都实现」由此自动覆盖，不需要新单或 amend |
| R-8b | **DEP-INT-8 的 `relevance_meta` 改为按值接收**，调用形状写死为 `memembed::relevance_meta(r.value, r.kind.as_str(), r.version, Some(r.query_background))`；`kind` 必须走 `ScoreKind::as_str()`、**不许复写 `"calibrated"` 字面量**（R-A A7） | mem-core 的理由：`ruagent_knowledge::RelevanceScore` 的 crate-root re-export 在 00:1x→01:0x 之间一度不在（01:0x 已恢复，`lib.rs:23-24`），它不想让自己的编译状态跟随别人的编辑波动 ⇒ 按值切断路径依赖 |
| R-8c | **`crates/daemon/src/api.rs:3814` 的 clippy 已由我修**：`.repeat("?").take(ids.len())` → `.repeat_n("?", ids.len())`（`clippy::manual_repeat_n` / `repeat().take()`） | captain 裁决「owner 自己修」（mem-core 只报坐标）。这是 `crates/daemon/src/api.rs` 的代码改动 ⇒ A-8/§6.4 的「本单零代码改动」措辞已同步修正 |
| R-8d | **t24 已终态**（mem-core 报）：`WikiLeadMeta.edited: Option<bool>` + `None → edited=unknown` + 断言不含 `edited=false`；mem-core 侧 `check -p ruagent-daemon --all-targets` exit=0 ⇒ A-13 ⑥ 保持「可判」 | mem-core 回信；我 23:42:56 的只读核对一致 |
| R-8e | **同一轮 clippy 又发现我 inScope 内第二处**：`crates/daemon/tests/injection_e2e.rs:773` 的 `collapsible_if`（嵌套 `if let`）已改为 let-chain —— 它挡的是 `-p ruagent-daemon --all-targets` 的 **test** target，不修就轮到 INT 自己 | 我跑 `clippy -p ruagent-daemon --all-targets -DenyWarnings`（经包装脚本）实测的完整输出：api.rs 那条在我修掉后消失，剩 distill.rs:1098/1100（graph）与 injection_e2e.rs:773（我） |
| R-8f | **`crates/daemon/src/distill.rs:1098/1100`（`type_complexity`）归 graph 的 repair**，INT 不碰；并登记 captain 的提醒：**`distill.rs` 属 daemon crate ⇒ `-p ruagent-graph` 之类的命令永远查不到它**（跨 crate 的 lint 归属陷阱） | captain 2026-09-28；写下来免得下一个人用 `-p ruagent-graph` 去核 distill.rs |

**R-9（2026-09-28T01:4x–01:5x+08:00；编译面纪律 + 我这两处修复的诚实状态）** —— 来源：`agent_teams_send_message` ← captain（三条更新：编译面纪律写进 §6.0 · `injection_e2e.rs:773` 归我 · `api.rs:3814` 已清、归 graph 的只剩 distill.rs）与 ← mem-core（同一条读数陷阱 + `lead_for` 三参 async、另有同步档 `lead_from`）。我的只读核对 + 经包装脚本的实测：

| # | 事实 / 改动 | 读数（带编译面） |
| --- | --- | --- |
| R-9a | **§6.0 新增「编译面纪律」五条**：第一个失败 target 就中止；报通过必须写明覆盖的 target；daemon clippy 要在 lib 与 lib-test 都绿之后再读一次；最终 verify 分两遍；只有 `--all-targets` 才能声称覆盖全部 | captain 的三条更新 + mem-core 的反例 |
| R-9b | **`crates/daemon/src/api.rs:3814` 已清**（`.repeat_n`）；我这轮 clippy 的完整输出里已无 api.rs | `clippy -p ruagent-daemon --all-targets -DenyWarnings`（经脚本）exit=101，**只剩 `distill.rs:1098/1100`**；mem-core 01:3x 独立复跑同结果 |
| R-9c | **`crates/daemon/tests/injection_e2e.rs:773` 的修复已编译通过，但它的 clippy 读数仍待 graph** —— 这是本条最重要的诚实点：上一条命令停在 `lib test`，`injection_e2e` **从未被 lint**，所以「清单里没有它」不构成它还坏或已好的证据 | 我用**换一个面**测：`check -p ruagent-daemon --all-targets`（经脚本）⇒ **exit=0 / 3.8s**，覆盖 `--lib`、`--lib test`、全部 test target（含 `injection_e2e`）⇒ 我的两处编辑**可编译**；**clippy 面的最终读数要等 graph 清掉 distill.rs 后重跑**（§6.0 第 2 条） |
| R-9d | **`lead_for` 与其同步档**（我独立核对）：`wiki.rs:2846 pub async fn lead_for(kb, recorded_hash: Option<&str>, slug)`；`wiki.rs:2817 pub fn lead_from(kb, slug)`（同步，`edited`/`stale_since` 会是 `None`，即第三态由调用方决定） | DEP-INT-8 以 `lead_for` 为准；`lead_from` 只是记录，INT 本轮不用它 |

**R-10（2026-09-28T01:5x–02:0x+08:00；clippy 假绿的仪器纪律 + 我按它交出的第一条真读数）** —— 来源：`agent_teams_send_message` ← captain（「请把 clippy 假绿写进 §6.0，这是本代最危险的一条仪器教训」；转述 wiki 在 t10 的实测：**尾参不进 cargo 指纹** ⇒ 先跑过一次不带 `-D` 的 clippy 之后，带 `-D` 的同一条命令命中缓存、**0.8s、exit=0、输出里没有 `Checking ruagent-daemon`**，而仓库里确有 warning；她的可信绿在 `clean -p ruagent-daemon` 之后取得，**9.67s**）。

| # | 改了什么 / 我交出的读数 | 证据 |
| --- | --- | --- |
| R-10a | **§6.0 新增第 6–8 条（lint 通过的第二件证据）**：⑥ `exit=0` **且** 输出里有本次 `Checking <crate>`，缺一不可（只有退出码不算）；⑦ 保险做法 = 先 `scripts/cargo-team.ps1 clean -p ruagent-daemon`（**只删本包产物，不整仓 clean**）再跑 clippy；⑧ 总纪律 = 报「lint/测试通过」必须同时给**覆盖的 target** 与**本次重查的 crate** 两件证据 | captain 的转述 + 我用 `-DryRun` 验过的 `clean` 形状（`cargo clean -p ruagent-daemon`） |
| R-10b | **A-13 / A-14 的跑法已带上这两条证据要求**（判据文本未动） | captain：「让 A-13/A-14 的跑法带上这两条证据要求，判据文本不用动」 |
| R-10c | **我按该纪律交出的第一条真读数（同时补上 R-9c 承诺的那条 clippy 面读数）** | `clean -p ruagent-daemon` ⇒ exit=0 / **0.6s** / `Removed 918 files, 2.4GiB total`；随后 `clippy -p ruagent-daemon --all-targets -DenyWarnings` ⇒ **exit=0 / 8.1s** / stderr 里有 **`Checking ruagent-daemon v0.1.0`** ⇒ 两件证据齐（target 面 = `--all-targets`，含 `--lib`/`--lib test`/`--test injection_e2e`；重查面 = 本次确实重查了 daemon）。**⇒ 我的两处 clippy 修复（`api.rs:3814`、`injection_e2e.rs:773`）在 lint 面通过；graph 的 distill.rs 修复也已落地**（0.8s 的假绿形状没有出现） |
| R-10d | **本代唯一一处 workspace 级 lint 门是 t19 的 `clippy --workspace --all-targets -- -D warnings`** ⇒ 这条纪律主要落在 INT 身上；t19 报读数时按 §6.0 第 8 条给两件证据 | captain 明示 |

**R-11（2026-09-28，captain「§6.0 再加第 9 条」；graph 的同一失真另一半，只改契约文本、未跑任何命令）** —— 来源：`agent_teams_send_message` ← captain。三处更新：

| # | 改了什么 | 依据 |
| --- | --- | --- |
| R-11a | **§6.0 新增第 9 条**：**改过哪个 crate，就先 `clean -p <crate>`（或至少 `touch` 该 crate 的文件）再跑 gate**；并尽量用 **`--verbose`** 看逐 target 的 `clippy-driver --crate-name …` 行，作为第 ② 件证据的**逐 target** 形态 | graph 首次跑 daemon gate 得到 **0.77s / 0.9s 的 `exit=0`**（看着全绿），`touch` 自己改过的文件强制重编后才拿到真读数 —— 与 wiki 的 0.8s 是同一失真的两半 |
| R-11b | **第 8 条改写为 captain 的措辞**：两件证据 = ① 覆盖了哪些 target（`--all-targets` 的 *all* 不是保证、首个失败 target 就停住）② 本次确实重查了哪些 target（**退出码不算，缓存命中的绿不算**） | 同上 |
| R-11c | **写入范例（缺证据就收窄声称面）**：graph 那一轮**只声明「这次调用 exit 0」，不声称逐 target 证据**（因为它没拿到那批 `clippy-driver` 行）—— 与「把没重查说成通过」相反的做法；并把我自己 R-10c 的读法（`clean 0.6s / Removed 918 files, 2.4GiB` + `clippy 8.1s / Checking ruagent-daemon`）作为「能复核的两行」的模板 | captain 明确要求把 graph 的做法作为范例写进去 |

**R-12（2026-09-28T01:4x–02:0x+08:00；§6.0 第 10 条 = 竞态 + 工具层结构性修法；只改契约文本）** —— 来源：`agent_teams_send_message` ← captain（「§6.0 第 10 条（wiki 实测的竞态）+ 工具层面的结构性修法」）。我的只读核对：`scripts/cargo-team.ps1` 现 **8,298 B / mtime 2026-09-28 1:40:18**，`-CleanFirst` 在 `:84` 解析成 `List[string]`、`:117` 打印 `[cargo-team] (under the same lock) cargo clean -p …`、`:155-157` 在同一锁窗口内先 clean 再跑门。

| # | 改了什么 | 依据 |
| --- | --- | --- |
| R-12a | **§6.0 新增第 10 条**：wiki 先 `clean -p`、再**单独**跑门 ⇒ 队友的普通 clippy 抢在**两次 cargo 调用之间**（锁是按调用持有的）⇒ 缓存又变可命中 ⇒ **1.44s 假绿**；最后靠 `touch` 源文件 + **紧邻**跑同一条门才取到真读数。**结论：强制重查必须与门在同一持锁窗口内。** | captain 转述 wiki 的实测（本代第三例假绿，形状与 wiki 0.8s / graph 0.77s 同族） |
| R-12b | **第 7 条改写为可靠形态**：`… -CleanFirst <crate>`（可重复）在**同一锁窗口**内 clean + 门，并给出的确切命令行；同时明确写下「两条独立命令本身不可靠」 | captain 已把该条从纪律**下沉到工具**（脚本 v4），并 `-DryRun` 验过解析 |
| R-12c | **新增「`Checking <crate>` 的覆盖面限制」段**：`Checking` 是**按包**打印 ⇒ 只能证明**这个包**被重查，**不能逐 target 枚举**；**正确声称面** = 「这次调用 `exit=0` 且这个包被重查过」，**不声称逐 target 证据**（`--verbose` 的 `clippy-driver --crate-name …` 行是更强形态，**拿不到就不许声称**）；措辞照 wiki：「缺证据就收窄声称面」 | wiki/graph 共同确认 + captain 要求写进 §6.0 |
| R-12d | **第 6–9 条保持**；末尾「主要落在 INT 身上」的句子改为**第 6–10 条**，并写明 t19 起改用 `-CleanFirst` 的单窗口形态 | captain：「第 6–9 条保持，这条作为第 10 条」 |

**R-13（2026-09-28，captain 转 V-A 的两条发现 + 两条门禁重申；只改契约文本）** —— 来源：`agent_teams_send_message` ← captain。我的只读核对：`crates/knowledge/src/store.rs:71` 的 `ScoreKind` 只有 `serde::Serialize`（**无 `rename_all`**），`as_str()` 五个字面量为 `rrf_rank`/`semantic_l2sq`/`bm25`/`cosine`/`calibrated`；同 crate 的 `ResidualOrigin` **有**漂移钉（`crates/knowledge/tests/residual-scan.rs:40`）。

| # | 改了什么 | 依据 / 归属 |
| --- | --- | --- |
| R-13a | **§6.0 增「冻结字面量的 serde 漂移陷阱（F-5）」段**：serde 实读 `"RrfRank"` vs 冻结字面量 `"rrf_rank"`；**在加 `rename_all` 之前，HTTP 面必须继续用手写字面量**（`api.rs:3003/3006/3008` 恒 `"rrf_rank"`）；任何新增 `*_kind` 键必须取自 `ScoreKind::as_str()` 这一唯一来源 | V-A 的发现 + 我的只读核对；契约落点（③）属我 |
| R-13b | **①②（加 `rename_all` + 漂移钉测试）的归属**：定义在 `crates/knowledge` ⇒ **I-A（recall）**；漂移钉可落在 `crates/knowledge/tests/**`（I-A）或在 `crates/daemon/tests/**` 做**跨 crate**钉（**后者在 INT 的 inScope，可随同一次改动加**）。**INT 在收到 amend 或路由前不写 `crates/knowledge`** | 一文件一写者（discipline #5）；已就此向 captain 提路由确认 |
| R-13c | **§5.2 增 F-13「评测集合规模的读法」**：不许用 `query_eval_gold` 的**表行数**表示集合规模，必须 `COUNT(DISTINCT query)` 或冻结常量（V-A 实测：6 次 seed ⇒ **132 行** vs 集合 **22 条**，`(132, 90)`；根因 = 无 `UNIQUE(set_id,query)` + `INSERT OR IGNORE`，幂等性 repair 归 recall） | captain；今天是**预防性**冻结（HTTP 面尚未暴露该表） |
| R-13d | **§1.5 的 `/graph/retrieve` 增接线门禁**：**`as_of` 在 graph 修完 RVC-1（t27）之前不接线**；`GET /graph/entity/{id}/facts?at=` 只透传、**在 RVC-1 落地前不得声称时序正确** | captain 重申 |
| R-13e | **重申两条门禁**：workspace clippy 用 `-CleanFirst ruagent-daemon`（或所需包）+ **两件证据**（§6.0 第 6–10 条）；`?as_of=` 等 t27 | captain |

**R-14（2026-09-28，captain 对 F-5 归属的裁决；只改契约文本）** —— 来源：`agent_teams_send_message` ← captain（「归属裁决：(a)，但不新开单 —— F-5 并进我已有的 t29（owner=recall，deps=t7）」）。我在 team 状态里核对到 **t29 已 claimed（repair r1，owner=recall）**，其标题为「闭合 V-A F-1：query_eval_gold 幂等性」⇒ F-1 与 F-5 同单、**先 F-1 后 F-5**。

| # | 改了什么 | 依据 |
| --- | --- | --- |
| R-14a | **F-5 的 ①② 归属落定**：并入 **t29**（recall），不新开单；理由 = F-5 与 F-1 **同源**（都出自 V-A 对 t7 的独立验证）、落点同在 `crates/knowledge/**`（t29 的 inScope 已覆盖 `store.rs`），且一个成员同时只能拥有一张未完成单。契约 §6.0 的 F-5 段已按此改写，并记下 t29 的验收（serde 输出与 `as_str()` 五字面量逐字一致 + 漂移钉 + 改前 `"RrfRank"` → 改后的读数） | captain 裁决 + 我的状态核对 |
| R-14b | **跨 crate 钉的顺序写死**：`crates/daemon/tests/**` 的钉**必须等 t29 的 ① 落地后**再加（**不许先提交红测试**），断言 `serde_json::to_string(&ScoreKind::RrfRank) == "\"rrf_rank\""`；**两条钉分工** = crate 内钉**定义**、跨 crate 钉**消费面** | captain 明确「做，但顺序照你说的」 |
| R-14c | **F-13 被确认为预防条**：今天 HTTP 面未暴露 `query_eval_gold`，但下一轮 C3 标定会踩 ⇒ 现在冻结口径（`COUNT(DISTINCT query)` 或冻结常量）是对的 | captain |
| R-14d | **两条门禁再次确认**：`as_of` 等 RVC-1（t27）；clippy 用 `-CleanFirst` + 两件证据 | captain |
| R-14e | **t19 的依赖在 team 状态里是 `t5,t15,t16,t28,t18,t23,t24`** —— 其中 `t5/t23/t24` 已完成，**`t15/t16/t18/t28` 仍未完成**（t14 V-D 也仍在跑）⇒ t19 **尚未 ready**。captain 说「继续 t19，不需要等我」，但按调度器的依赖门与「不抢未就绪的任务」，INT **等 t19 被派发**（不提前 claim）；本行写下来是为了让 V-INT 知道契约这一侧已经就绪 | team 状态（2026-09-28 本会话读数） |

**R-15（2026-09-28，captain「§6.0 再加一条纪律（第 11 条）：判据必须让漏测自己红」；只加纪律，判据文本未动）** —— 来源：`agent_teams_send_message` ← captain（转述 recall 在 **t33** 的 harness 实测）。

| # | 改了什么 | 依据 |
| --- | --- | --- |
| R-15a | **§6.0 新增第 11 条**：`cargo test … -- --ignored` 在无 ignored 测试的包里打印 `ok. 0 passed … N filtered out` 且 **`exit=0`** ⇒ **「谁来跑」不能当判据**；合格形状 = `#[ignore = "…needs <ENV>…"]` + 体首 `expect`（缺 env 显式运行必 FAILED）+ **真实读数断言**（`rows==15 && no_answer==7`）⇒「有 env 但没测」也红。**原则：不存在一个不测量也能通过的状态。** | recall t33 的实测（我未独立复现，按「转述」登记） |
| R-15b | **同族纪律写入**：**断言从没被证明失败过，就还没被证明是断言** —— 新增的防空转断言也要在**真空输入**上实跑出红读数（recall 另立 **t36**） | captain |
| R-15c | **写成「六种假绿」的收尾**（本代第 1–11 条一族）：① 缓存命中 · ② 编译面缺口（首个失败 target 就停）· ③ target 覆盖声称过宽（`--all-targets` 的 *all* 不是保证、`Checking` 只到包级）· ④ 夹具/环境状态依赖（缺 env 静默退化成空跑）· ⑤ 判据被削弱 · ⑥ **空跑绿**（本条）；**判据的设计目标 = 让前五种造成的漏测都表现为一次红** | captain 的族清单 + 我前十条的积累 |

**R-16（2026-09-28，captain：`as_of` 门禁解除 + §6.0 第 12 条；只加纪律/改跑法，判据文本未动）** —— 来源：`agent_teams_send_message` ← captain。

| # | 改了什么 | 依据 |
| --- | --- | --- |
| R-16a | **§1.5 的 `/graph/retrieve` 门禁由「不接线」改为「可接线 + 三条证据」**：① **3 条「同一瞬间 × 三种写法（规范形 / `Z` / `+08:00`）结果逐字节一致」**的读数；② `as_of` **入口归一化**；③ 非时刻字符串 ⇒ **400**（不许静默按字符串比）。MCP `graph_retrieve` 行同步。**R-13d/R-14d 的历史措辞保留不改**（那两行写的是当时有效的门禁），本条即解除点 | graph 的 **t27 已 completed**：三种瞬间 × 三种写法逐字节相同（10349 / 21268 / 39215 B 各三次一致），非时刻字符串现在报错 |
| R-16b | **`RetrievalStats.graph_edges` 的语义钉**：它**不是**「那个时点的边数」（与 `as_of` 无关，每次都是 60）⇒ 消费面不许把它当时间切片读数；已写进 §1.5 与 A-14 ⑧ | captain 转 RV-C 的提醒 |
| R-16c | **§6.0 新增第 12 条「公开签名 / 公开类型形状 / re-export 名单的变更必须先广播给受影响属主」**，含两个真实形状（graph t27 门禁③④因 wiki 接口改动 + mem-core `memembed.rs` 未适配而红；mem-core t31 把 `merge_audit_reason` 从 `&MergeVerdict` 改成 `&MergeAudit` 而 `memembed.rs:529` 未跟上 ⇒ 卡住 graph 两条真实路径测试、RV-D 契约命令首次 `exit=101`）；纪律 = 落地前后把「新形状 + 全部受影响调用点」发给属主、报告里登记「会让谁在哪个窗口变红」；**判据 = 不许让同伴从一次红里猜你在改什么** | captain；与「re-export 名单 / serde 形状 / 入参类型都是对外契约」同源 |
| R-16d | **A-14 的跑法补第 ⑧ 项与两条证据要求**（`as_of` 三写法读数、`graph_edges` 语义钉、第 11 条的空跑禁令） | captain 要求接线时给 3 条读数 |

**R-17（2026-09-28，t37 完成 + §6.0 第 13 条；只加纪律）** —— 来源：`agent_teams_send_message` ← captain（收下 t37，点名两处并要求加第 13 条）。t37 交付物：`crates/daemon/tests/injection_e2e.rs`（32 ins / 46 del，794 → 780 行）+ `docs/design/reviews/gen2-injection-instrument-repair.md`。

| # | 改了什么 | 依据 |
| --- | --- | --- |
| R-17a | **§6.0 新增第 13 条「同形多处替换必须整块一次改完再离开文件」**：形状 = t37 先删 `skip_missing_mock()`、再逐个改 7 处调用点，**wiki 的 t34 门禁跑在中间态** ⇒ `:685 cannot find function skip_missing_mock`（exit 101），她的门禁读数被污染一次；纪律 = **要么一次性改完并自测到可编译，要么先加兼容层再删旧形状**；**报告里登记该编辑窗口**（登记后是「读数污染」而非「事故」）；可自动化的一条 = 改完**立刻** `cargo check -p <crate> --all-targets`，别等门禁 | captain 2026-09-28；t37 报告 §7 即范例 |
| R-17b | **单记 `RUAGENT_REQUIRE_MOCK` 的审计结论**（新增一段）：「**被文档描述成存在、实际从未被启用**的严格开关」—— 全仓 5 处提及、定义侧只在 `injection_e2e.rs:90/:93/:94`、另 2 处是文档描述、**无人设置** ⇒ **不是防线，只是声称的能力**；**审计规则** = 任何严格开关/守卫/opt-in 都要能被一条 grep 回答「谁打开它」，答案是「没有人」时**要么接进门禁、要么删掉**；同族变体（`WikiLeadMeta.edited` 丢三态 · `WIKI_LEAD_HINT` 双份字面量 · `ScoreKind` serde 漂移 · 空跑绿）一并登记 | captain：「值得单记……与本代其它失真同源（声称的能力 vs 实际的能力）」 |
| R-17c | **t37 的其它读数按 captain 的照收记录**：三态（默认 `0 passed; 7 ignored` · 显式缺件 `FAILED. 0 passed; 7 failed` / **101** · 有件 `7 passed`）· diff 32 ins/46 del 且 rustfmt 干净 · 两件证据（`Removed 414 files` + 本次 `Checking ruagent-daemon`）· wiki 独立复跑同一条 clippy = exit 0 作独立复核 · **诚实项①** 的标注方式获认可（「没有一手读数就说没有一手读数」） | captain |
| R-17d | **`crates/acp/src/adapter.rs:160`**（门 = `resolve_program("dsh").is_none()`）那条同族项按**独立项留给 acp 侧**，本代记为下一代 | captain |

**R-18（2026-09-28，captain「§6.0 再加第 14 条：根因是假设，必须用受控反做验证」；只加纪律，判据文本未动）** —— 来源：`agent_teams_send_message` ← captain（转述 RV-C r3 对 graph t38 的示范）。

| # | 改了什么 | 依据 |
| --- | --- | --- |
| R-18a | **§6.0 新增第 14 条**：报告里写「根因是 X」必须能被一条**反做实验**回答 —— **把 X 改回去（或移除 X），症状是否仍在？** 仍在 ⇒ X **不是**根因（改写成「症状未定因」或继续定位）；消失 ⇒ X 是根因（**把反做读数写进报告**）。**豁免** = 纯机械直接因果（「列不存在 ⇒ 查询报错」）；**推断性行为归因**（tie-break / 采样面 / 聚合口径 / 缓存 / 并发）**一律要做** | RV-C r3：反做 `retrieve.rs:931/:957` 的 `.then_with(|| edge_seq(…))` 后 **3/3 次与交付树逐位相同** ⇒ 「tie 改变谁进 truncate」被推翻，真因 = **读数没在交付那一版上重取**；被复现的只有不可复现性本身 |
| R-18b | **写明为什么**：持久报告里写错的根因会被下一代当真相读（本代已栽过一次：把 `git stash` 的**症状**误归因成「re-export 名单漂移」，需专门更正才摘干净）；**读数要可复现，根因要可反做**；并标明与**第 11 条同族**（第 11 条不相信**绿**，本条不相信**解释**） | captain |
| R-18c | **对 t19 的直接约束（我自己的承诺）**：本单报告里凡写「根因」的推断性结论（例：注入调用点计数、实体腿命中率改前→改后、`top_knowledge_score` 不可比的成因、遥测列恒空的原因）**都要附反做或受控对照**；做不到的写成「**未定因**」 | 第 14 条对 INT 报告同样生效 |

**R-19（2026-09-28，captain「§6.0 再加第 15 条 + 记一条能解释偶发红的仪器发现」；只加纪律）** —— 来源：`agent_teams_send_message` ← captain（转述 graph t42 的两条实测）。

| # | 改了什么 | 依据 |
| --- | --- | --- |
| R-19a | **§6.0 新增第 15 条 ①「缓存不只骗绿，也骗红」**：graph 负控实验后**恢复源码但 mtime 被保留** ⇒ cargo 复用负控那份坏二进制 ⇒ 看起来像「修法无效」。纪律 = **恢复源码后必须 `touch` 或 `-CleanFirst` 再取值**；与第 6/11 条合并：**绿和红都可能来自陈旧产物，报读数必须说明你测的是哪一份字节**（是否重编 / 哪个 crate 被重查 / 构建树来源） | graph t42 |
| R-19b | **第 15 条 ②「偶发红可能是在读别人的数据」**：夹具 `{pid}-{seq}` 目录**从不清理**（`Drop` 因连接仍开着必失败，`os error 32`；该 crate 名下 **725 个**泄漏目录）⇒ pid 回收后新进程继承上一轮实体行（id 1..63 已在）⇒ `UNIQUE(entities.id)` ⇒ 整套门禁 `3 passed; 2 failed` 而单跑 12 次不复现。纪律 = **会写输入的仪器必须独占输入**；看到**不可复现的红**先问「输入是不是别人留下的」（临时目录/端口/pid 复用/共享副本），**不要先怀疑被测代码**；夹具**先删后建** + **带负控**的回归测试 | graph t42 §11 |
| R-19c | **第 15 条 ③「临时 root 的清理义务」**：`%TEMP%` 下 **8,746 个** `ruagent-*` 目录 / **136.8 GB**（C: 可用 172 GB → 102 GB），captain 正在清（保护 `ruagent-team-target` 与 20 分钟内的新目录）；**本代任何测试若创建 root 必须自己收尾**（或落在会被清掉的固定前缀） | captain；t37 已按此收尾（`ruagent-t37-nomock` 用后删除） |

**R-20（2026-09-28，captain 要求 §6.0 第 16 条；t45）** —— 来源：`agent_teams_send_message` ← captain（「inScope 跨两个 crate 时，门禁必须覆盖两个 crate」）。

| # | 改了什么 | 依据 |
| --- | --- | --- |
| R-20a | **§6.0 新增第 16 条**：**inScope 跨两个 crate 时，门禁必须覆盖两个 crate** —— 三个实例：① graph 的 `distill.rs` 属 daemon crate（`-p ruagent-graph` 永远编不到）② wiki 的 `crates/mock-agent/tests/wiki_pipeline.rs` 属 mock-agent crate（t34 只跑 `-p ruagent-daemon` ⇒ 从未被 lint）③ `cli/` 此前无人拥有。**纪律**：inScope 若含另一个 crate 的文件，verify 必须同时跑那个 crate 的 `-p <crate> --all-targets`；**判据** = 被改文件所在 crate ⊆ 被跑过的 `-p <crate>` 集合 | captain 2026-09-28 |
| R-20b | **`cli/` 收编进 INT 的 inScope（t45）**：`cli/src/main.rs:809` 的 `items_after_test_module` 由 INT 处置，修法 = 在 `mod tests` 上加**作用域限定的 `#[allow(clippy::items_after_test_module)]` + 理由注释**（把 ~500 行移到文件尾部是「无人拥有的文件里的大重排」，diff 远大于它消掉的 lint；注释里写明「`cli/` 有了属主后移动才是更好的修法」）。核对：`clippy -p ruagent --all-targets -DenyWarnings` ⇒ **exit 0**（本条同时满足第 16 条：改的是 cli crate，就跑 cli 的门） | t45；captain 已把该 lint 并入本单 inScope |

**R-21（2026-09-28，captain 要求 §6.0 第 17 条；t47 实例）**

| # | 改了什么 | 依据 |
| --- | --- | --- |
| R-21a | **§6.0 新增第 17 条「量词工具本身的默认行为会造假读数」**：实例 = t47 用 `Select-String 'FAILED'` 数出 **58 条「失败」**，实为 PowerShell 默认**大小写不敏感**，把 38 条 `0 failed` 行算进去；结论（0 真失败）当时由 `TEST_EXIT=0` **独立**支撑。**纪律**：计数类读数必须写明工具 + 关键开关（大小写、字面量/正则、编码、行尾）；能用退出码或显式大小写敏感就不用模糊匹配；**结论不许只挂在一个计数上**。与第 6/11/14/15 条同族：**判据、绿红、解释、量词 —— 四者都可能被陈旧产物或默认行为造成假象** | captain 2026-09-28；t47 报告 §15.5 |
| R-21b | **`forget-report` 在 HTTP 面上补全（INT46-1）**：`GET /api/v1/forget-report?content_hash=<64hex>` ⇒ 200 `{content_hash, local[], external[], total}`；**未知（从未写入）哈希 ⇒ 成功 + 各面 `hits:0` / `sample` 空**（不是 404、不是假数据）；非 64 位十六进制 ⇒ **400**。**并钉住一条读法**：`total` 是**报告携带的面读数个数**（本代 10），**不是残留计数** —— 写入与未写入哈希的 `total` 相同，因此它**不能**区分「写过/没写过」；要看看 `local[].status.readout.hits`。这条读法来自 INT 先写错注释（「未知 ⇒ `total==0`」）再实测（`total:10`）修正 | t47 报告 §15.1；captain 确认 |

**R-22（2026-09-28，t53）** —— 来源：captain 转 t51 的两类「声称面 ≠ 被测面」。（**补记（t55）**：这条 R-22 的修订记录在 t53 时**因编辑窗口失败而未落盘**，t55 用内容级读数发现并补上 —— 契约未被 git 跟踪，`git diff` 对它**不是**证据，见第 21 条。）

| # | 改了什么 | 依据 |
| --- | --- | --- |
| R-22a | **§6.0 新增第 18 条**（异步面读数先轮询到终态）与**第 19 条**（门禁必须写明不覆盖什么）；第 19 条当时记的处置（把 `tsc` 前置进 `build`）**已被 R-23b 撤销** | captain 2026-09-28；t50 / t51 / t53 |
| R-22b | **§1.5 加「读法注」并复核两处「文档事实错」**：复核结果 **两条声称都不成立** —— 创建在契约里本来就是 `POST /api/v1/graph/entity`（单数，与 `api.rs:53` 一致），复数只有 `GET`（`api.rs:49`）；`wiki/links` 行的 `degrees` 本来就是**对象数组** | t53 复核（全表 grep 命中 + 路由表原文） |

**R-23（2026-09-28，t55 更正 —— 「面板类型检查何时入门禁」的错误说法）**

| # | 改了什么 | 依据 |
| --- | --- | --- |
| R-23a | **更正第 19 条里的错误实例**。旧文字（逐字）：「而本仓 `npm run build` 原先只有 `node scripts/build-panel.mjs`（**vite 构建，不含 `tsc`**）⇒ **一个 TS 类型错误能静默通过**」。**为什么错**：`panel/scripts/build-panel.mjs` 一直跑 `i18n-check`(L33) → `tsc -b`(L34) → `tsc -p e2e/tsconfig.json --noEmit`(L35) → `vite`(L44)，`run()`(L25-31) 任一步非零即退出并打印 `dist untouched`，`dist` 只在检查之后才写(L46-58) ⇒ **门禁本来就覆盖类型检查**。**真实现状**：类型检查在 `cd panel && npm run build` 内部（wrapper）；`check` 脚本只是没人按名字调用。**第 19 条本体保留**（Rust 门禁不覆盖面板的 TS 仍成立） | t55 取证（行号）+ **只针对 wrapper 的负控**：`WRAPPER_ONLY_NEGCTL_EXIT=1`、`dist_index_sha` 前后同为 `98F8B0D3…`（`dist_untouched=True`）、撤销后 `exit=0` |

**R-24（2026-09-28，t57：V-INT 消费面 findings 的收口）**

| # | 改了什么 | 依据 |
| --- | --- | --- |
| R-24a | **§6.0 新增第 22 条**「跨 target 的 `passed` 求和不是稳定读数」—— 按 verify 在 t20 §10 交出的**实际做法**写成「**求和的三种失败方式 + 正确形状**」：① 求和会**扫进不属于本次的东西**（正则 `(\d+) passed` 扫的是报告者手上那份输出，可能含他自己额外跑的 `cargo test -p …` 与 doc-tests 的 `0 passed` 行）⇒ 同一命令在不同人手里得 386/387/390，漂移来自**求和范围**；② **行数会造假**（不区分大小写时 `0 failed` 被当成 `FAILED`，第 17 条实例）；③ **正确形状** = `test result: ok` / `FAILED` / `panicked` 的**行数**（`Select-String -CaseSensitive`）+ **包装脚本退出码** + **逐 target 的 `Running <target>` → 紧随其后的 `test result` 按序配对**；坚持给总数必须写明口径与范围。与第 16/17/19/21 条同族：**读数必须与被测对象同界，且边界要写出来** | captain 2026-09-28（转 verify 的 t20 §10 做法） |
| R-24b | **F2 落地**：`GET /graph/entity/{id}/facts?at=` 在**入口**归一化（`parse_ts` → `to_rfc3339()`），非时刻串 ⇒ **400**（t57 实测改前：`not-a-time` / `2026-13-45T99:99:99Z` / `now` **全部 200**）⇒ **A-14⑧ 现在两个入口都成立**（t27 只修了 `retrieve?as_of=`） | t57 读数（改前 200/200/200；改后 400） |
| R-24c | **F7 键名更正**：`/recall/log` 的契约行（§1.2 L52）**已经**写的是 `rows:[{…24 列…}]` ⇒ 与实现不一致的是**实现侧**（handler 的键名），修法归 F1（扩 SELECT + 对齐键名）；`communities/build` 的 `communities` 是**计数**而非列表 —— 在 §1.5 该行就地加**读法注**（旧文字逐字引用） | t57 复核（契约 L52 原文 + L101 原文） |
| R-23b | **撤销 t53 对 `panel/package.json` 的改动**（`build` 里的显式 `tsc ×2` 前缀）：与 wrapper 内部**重复**（每轮多跑两次 tsc），且当时前提是错的 ⇒ 回到 `"node scripts/build-panel.mjs"`；`panel/package.json` 是**被跟踪**文件 ⇒ `git diff --numstat` **为空**，这条空 diff 是**证据** | t55 决策 + 读数（wrapper-only 负控红 + `dist` 未动） |
| R-23c | **§6.0 新增第 20 条**「判断一条命令做了什么，要读它真正执行的东西，不要只读入口名字」；并记下**同一错误被传播两次**（t53 进契约、t54 进 `AGENTS.md`）⇒「**错误的说法比错误的读数更长寿**」 | captain 2026-09-28；t55 |
| R-23d | **§6.0 新增第 21 条**「没有 diff ≠ 没改」（未跟踪文件的空 diff 是假绿）—— 并据此**补上 t53 丢失的 R-22 记录**（契约未跟踪，`git diff` 对它不是证据） | captain 2026-09-28 转 t56 的实测 |
