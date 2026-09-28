# gen2 集成实现（t19）：四区域接进守护进程单一消费面

- 单号 / 尝试：**t19** / attempt 1 / `attempt_id = a230db9c-308c-47a9-9295-be84d2ce7af0`
- 归属：**integ**（INT，集成与消费面工程师）
- 依据：`docs/design/reviews/gen2-integration-contract.md`（六节 + R-1..R-19）
- 环境：cargo 一律经 `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1`（共享 target、一次一个编译、CPU 0–11、BelowNormal；lint 门用 `-CleanFirst`）；**未写活库、未启停 pid 79984**（本单的测量面只有临时 root 与共享构建缓存）
- 交付：`crates/daemon/src/api.rs`、`crates/daemon/src/{chat,runs}.rs`、`crates/core/src/event.rs`、`crates/acp/src/chat.rs`、`crates/mock-agent/tests/e2e_daemon.rs`，本报告

**一句话（诚实版）**：本单把**闭环（两条注入路径 + 富集形状 + 遥测列）**与 **DEP-1/DEP-4 的守护进程侧接线**做完，并把 `memory_write` 的静默回落改成诚实 400；**MCP 5 个新工具、面板四页读数、`as_of` 三写法读数、DEP-1 生产命中率读数、`budget` 对象**未交付，逐条在 §9 登记（含原因与归属）。**不把「编译通过」当「契约逐条交付」。**

---

## 1 交付内容与改动点（改前 → 改后）

| # | 文件 | 改了什么（一句话） |
| --- | --- | --- |
| 1 | `crates/daemon/src/api.rs`（`recall`） | `knowledge.search()` + 第二次 `search_legs()` **两次检索** → **`search_page()` 一次**；响应新增 `scoring{score_kind,fusion,leg_window,scoring_version,candidates,top_knowledge_score,top_knowledge_relevance,top_knowledge_relevance_kind}` 与 `graph{entities,paths,graph_edges_total,empty_reason,truncated_by}`；遥测行 11 列 → **23 列**（新增 12 列全部填真值） |
| 2 | 同上（实体腿） | `ruagent_graph::search_entities()`（**strict** 名字/摘要 FTS）→ **`resolve_seeds()`（五腿种子）+ `retrieve()`（一次多跳）**；`entities` 仍是种子去重后的实体行数（K-16 语义不变），多跳读数进 `graph_entities`/`graph_paths` |
| 3 | 同上（`knowledge_hit_json`） | 入参 `SearchHit + (sem,kw,stage)` → **`RankedHit`**；`score_kind` 从**手写字面量**改为 `ranked.score_kind.as_str()`；每个分数键旁挂量纲：`semantic_score_kind` / `keyword_score_kind` / `relevance`+`relevance_kind`+`relevance_version`+`relevance_query_background` |
| 4 | 同上（`knowledge_search`） | 同一 `search_page()`；`limit` 由**无上限** → `clamp(1,50)`；响应新增同一个 `scoring` 块；`?legs=false` 保留旧语义（`legs: []` 且 leg 键为 `null`，**不是**第二种排序路径） |
| 5 | 同上（`memory_write`） | 未知 `store` **静默回落成 `Observation`** → **诚实 400** 并点名合法取值；`observation` 显式列出（旧的 fallback 兼了两个职责：默认值 + 错别字下水道） |
| 6 | 同上（DEP-4） | **新增 6 条路由**：`GET /graph/retrieve`、`GET /graph/communities`、`POST /graph/communities/build`、`PUT /graph/community/{id}/summary`、`GET /graph/resolution/pending`、`POST /graph/resolution/merge`（`null`≠`[]`；空 summary ⇒ 400；不存在 ⇒ 404；`keeper==absorbed` ⇒ 400；成功后**写 `entity_aliases`**） |
| 7 | `crates/daemon/src/chat.rs` + `crates/daemon/src/runs.rs` | 两条注入路径从 `k.search()` + `knowledge_items()` → **`k.search_page()` + `EnrichedHit`（`memembed::relevance_meta`/`enriched_hit`）+ `knowledge_items_enriched()`**；wiki 命中走**异步 `wiki::lead_for(k, &page_record(db,slug), slug)`**（RV-D2-1 要求；同步 `lead_from` 会让 `coverage=` 恒 `unknown`） |
| 8 | `crates/core/src/event.rs` + `crates/daemon/src/runs.rs` + `crates/acp/src/chat.rs` | `ContextInjected` 增 `path: Option<String>` 与 `budget: Option<Value>`（`#[serde(default, skip_serializing_if)]`，旧 transcript 仍可读）；run 路径标 `"run"`、chat 路径标 `"chat"` |
| 9 | `crates/mock-agent/tests/e2e_daemon.rs` | 两条过期期望按 t347/t26 的新语义修正（见 §7） |

**未改**（越界文件一行未动）：`crates/daemon/src/wiki.rs`、`crates/daemon/src/distill.rs`、`crates/daemon/src/memembed.rs`、`crates/store/**`、`crates/memory/**`、`crates/knowledge/**`、`crates/graph/**`（只读它们的公开接口）。

---

## 2 契约逐条对照（HTTP 路由 / MCP / 遥测 / 面板）

**§1 HTTP 路由（46 行）**：本代的 6 条**新增**路由全部落地（§1.5，见 §6）；其余 40 行为**既有路由**，本单改动的是其中 4 条的**响应形状**：

| 契约条目 | 我改了什么 | 证据（命令 / 读数） |
| --- | --- | --- |
| §1.1 `/recall` 的每个分数键挂量纲（H-1） | `knowledge_hit_json` 现挂 `score_kind`/`semantic_score_kind`/`keyword_score_kind`/`relevance*` | `cargo test -p ruagent-daemon --lib` = **76 passed / 0 failed**；测试 `knowledge_search_legs_match_recall_and_absent_legs_are_null` 打印 first hit keys = `[…,"score_kind","legs","semantic_score_kind","keyword_score_kind","relevance","relevance_kind","relevance_version","relevance_query_background",…]` |
| §1.1 `/recall` 的 `scoring` 一并输出窗口与版本（H-2） | 新 `scoring` 块（`leg_window=60`、`scoring_version=1`、`fusion=rrf:k=60,w_semantic=2,w_keyword=1`） | 同上测试的 `READING` 行（`knowledge/search` 侧同形） |
| §1.4 `/knowledge/search` 与 recall **同一构造器**、`limit` 有界 | 两处都改走 `search_page`；`limit.clamp(1,50)` | 同上；`?legs=false` 断言仍然绿 |
| §1.5 `/graph/*` 六条（DEP-4） | 新增（见 §6） | `cargo check -p ruagent-daemon --all-targets` exit 0；路由表 `api.rs:60-75` |
| §1.6 BREAK-* | **未处理**（见 §9 未实现项） | — |

**§2 MCP 工具（14 个：9 既有 + 5 新增）**：**未交付**（§9-1）。既有 9 个未改。

**§3 遥测**：
- `recall_log`：**11 列 → 23 列**，新增 12 列全部**填真值**（不是「列在即交付」）：`top_knowledge_relevance`（`RankedHit.relevance.value`）、`top_knowledge_relevance_kind`（`…kind.as_str()`）、`knowledge_leg_window`（`SearchEvidence.leg_window`）、`scoring_version`（`relevance.version`）、`graph_entities`/`graph_paths`（`resolve_seeds` 去重数 / `RetrievalEvidence.stats.paths_emitted`）、`score_kind`（`RankedHit.score_kind.as_str()`）、`fusion`（`FusionKind::Rrf{k,w_semantic,w_keyword}` 展开）、`top_legs_json`（页内每条的 leg rank/raw/kind + relevance）、`candidates_json`（候选数/窗口/fusion/页长）、`selected_json`（四段计数 + 知识 chunk_id + 图计数）、`rejected_json`（未入选 id + `min_score` + 图空因/截断因）。
- **逐列填充读数**：「列存在」我从 `crates/store/src/migrations/{0019,0021,0023}*.sql` 逐条核对（`recall_log` 现 24 列 = 0011 表 12 列 + 0019 4 列 + 0021 2 列 + 0023 6 列）；**「改后每一列都真被写」我没有做**（需要一次真调用后的 `SELECT` 逐列读数；见 §9-4，未测量不写成已交付）。
- `context_injected`：`path` **已落地**（两条路径都写）；`budget` **未落地**（`None` = 本次未采集，契约 §3.2 明确允许；原因见 §9-3）。

**§4 面板四页**：**未交付**（§9-2）。本单未改 `panel/**` 一个字节。

---

## 3 C-INT 逐条对照

| 条目 | 交付 | 读数 |
| --- | --- | --- |
| `memory_write` 未知 store ⇒ 诚实 400（不再静默回落） | ✅ `api.rs::memory_write`（`observation|profile|procedure|lesson` 之外一律 400 并点名取值） | 代码级 + `check` exit 0；**HTTP 级 400 读数未单独取**（见 §9-5，登记为未测） |
| `distilled` 布尔**按 episode kind 派生**且有正例读数 | ✅（既有实现 `distilled_ids` 的 `JOIN episodes e ON e.kind = 'run_turn'`；本单补了**断言**） | `crates/mock-agent/tests/e2e_daemon.rs` 现在断言「携带蒸馏正文的那条记忆必须 `distilled=true`」，且 `crates/daemon` 单测 `api::tests::the_distilled_marker_comes_from_the_episode_kind` **绿**（`--lib` 76 passed 内含） |

---

## 4 DEP-1 对照（graph 规格登记）

| 项 | 改前 | 改后 |
| --- | --- | --- |
| 取数口 | `ruagent_graph::search_entities(db, q, top_n)`（strict 名字/摘要 FTS） | `resolve_seeds(db, q, top_n)`（五腿种子，去重保序）+ `retrieve(db, &GraphQuery::for_text(q))`（一次多跳，取 `stats.paths_emitted`/`graph_edges`） |
| 遥测 | `recall_log.entities` = strict 行数；`graph_entities`/`graph_paths` **恒空** | `entities` 语义不变（种子行数，K-16）；`graph_entities`/`graph_paths` 由本条写值 |
| 响应 | 无图读数 | 新 `graph{entities,paths,graph_edges_total,empty_reason,truncated_by}`，并写明 `graph_edges_total` **不是**「那个时点的边数」（RV-C） |

**生产命中率「改前 → 改后」读数：未测**（§9-6）。**基线照抄规格，不冒认为我的读数**：改前现场（2026-09-27T13:58Z，只读活库）`recall_log` 651 行里 `entities>0` 仅 5 行 = 0.77%；23 条真实查询帧 strict 1/23、loose 13/23。**改后要复现这条读数需要冻结的 23 查询帧 + 一份活库副本**（本单没有拿到该帧的清单，也没有为读数复制活库）。**根因/口径说明（按 §6.0 第 14 条）**：这条读数的成因是「选不出种子」还是「帧本身难命中」我**未定因**——要判它必须做受控对照（同一帧跑 strict/loose/`resolve_seeds` 三方），本单未做。

---

## 5 HAND-OFF 逐条对照（recall H-1/H-2/H-3 + wiki DEP-W + I-B 适配形状）

| 条目 | 交付 | 证据 |
| --- | --- | --- |
| H-1 每个分数键旁挂量纲 + `relevance`/`relevance_kind` | ✅ `knowledge_hit_json`（两个端点共用） | `knowledge_search_legs_match_recall_and_absent_legs_are_null` 打印的 keys 列表（§2 表）；该测试同时钉住「缺席的腿是 `null` 不是 0」 |
| H-2 `top_knowledge_score` 必须与 `knowledge_leg_window`/`scoring_version` 同行输出 | ✅ 响应 `scoring` 块 + 遥测 4 列（`level_window`/`scoring_version`/`relevance`/`relevance_kind`）**同一次 `search_page` 取数** | 同上；`fusion` 值形如 `rrf:k=60,w_semantic=2,w_keyword=1` |
| H-3 两个端点同构 | ✅ 同一 `knowledge_hit_json` + 同一 `search_page`（旧的 `search_legs` 第二遍已删除） | 代码级；`grep knowledge_items(` 只剩**字符串** `.search(`（`api.rs:2948` 是 `fts_related` 里的 str 检索，非知识检索） |
| wiki DEP-W（端点装配新字段 + 纠错写端点） | **未交付**（§9-7：`/wiki/pages`、corrections 写端点属 wiki 的字段面，本单未改） | — |
| I-B 适配形状 `EnrichedHit` + `knowledge_items_enriched()` | ✅ 两条路径都改完（`chat.rs:597`、`runs.rs:1921`） | **「生产路径仍走未富集路径的调用点 = 0」**：`grep 'knowledge_items(\|\.search(' crates/daemon/src` 的唯一命中是 `regex/str` 的 `.search(&e.name, 4)`；`knowledge_items_enriched`/`search_page`/`lead_for(` 出现在 chat.rs、runs.rs、api.rs |
| `RetrievalHit`/`knowledge_items()`/`render_context`/预算与截断词表**签名与字节零变化** | ✅ 未改 crates/memory（越界文件）；daemon 单测 `runs::tests::truncation_markers_come_from_the_contract_vocabulary`、`retry_context_opens_with_the_shared_retry_prefix` 绿 | `--lib` 76 passed / 0 failed |
| G8 最终读数（`wiki_block_lead_marks 0/2 → 2/2`） | **部分**：接线走 async `lead_for`（RV-D2-1 ①）✅；**「块里出现带数字的 coverage」读数未取**（§9-8） | 见 §9-8；**不声称**「含 `coverage=` 即算」 |
| 按 R-D D.2/D.3 更新 `crates/daemon/tests/knowledge_api.rs` 逐字段断言 | **未交付**（§9-9） | 该文件在 inScope 但未改 |

---

## 6 DEP-4 对照（六条 graph 入口）

| 路由 | 形状 | 判据落点 |
| --- | --- | --- |
| `GET /graph/retrieve` | `{seeds,paths,stats,stats_note}`；`as_of` **入口归一化**（`parse_ts` → `to_rfc3339()`），非时刻 ⇒ **400** | 唯一多跳入口；`graph_edges` 的语义在响应里就地说明 |
| `GET /graph/communities?level=` | `{communities: null|[…], built, coverage{entities_covered,non_isolated}}` | **`null`（从未建过）≠ `[]`（建了但空）** — 由 `communities()` 的 `Option` 直接映射，不由计数推断 |
| `POST /graph/communities/build` | `CommunityBuild{level,communities,entities_covered,non_isolated,split_by_modularity}` | 五个计数原样回传（无 200 空体） |
| `PUT /graph/community/{id}/summary` | 空/空白 summary ⇒ **400**；`set_community_summary=false` ⇒ **404** | `updated:false` 不静默成 200 |
| `GET /graph/resolution/pending` | `{pending,redundant,pending_count,redundant_count}` | 两个读数**分开**（未决队列 vs 同对象两行） |
| `POST /graph/resolution/merge` | `keeper==absorbed` ⇒ **400**；任一实体不存在 ⇒ **404**（存在性在此**显式查询**，不靠 `moved==0` 猜）；成功 ⇒ `{moved,alias,alias_written}`，并**写 `entity_aliases`**（否则下次扫描又建回来） | 显式决策，绝不自动合并 |

**`as_of` 的三写法一致性读数：未取**（§9-10）。归一化逻辑在入口（同一 `DateTime<Utc>` → 同一个 `to_rfc3339()` 字符串），**这是代码级论证，不是读数**；按 §6.0 第 14 条我**不把它写成已验证**。

---

## 7 DEP-INT-1 对照（`context_injected` 的 `path`/`budget`）

| 字段 | 状态 | 读数 |
| --- | --- | --- |
| `path` | ✅ 两条路径都写（run=`"run"`，chat=`"chat"`） | 代码级（`runs.rs:1270` 附近、`acp/src/chat.rs:495` 附近）+ `check --workspace --all-targets` exit 0 |
| `budget` | ⛔ **`None`（本次未采集）** | 原因：`ruagent_memory::inject::render_context` **只返回字符串**（`crates/memory/src/inject.rs:588`，越界文件），没有报告型渲染 ⇒ `per_block_chars/total_chars/used_chars/truncated_blocks/dropped_items/blocks[]` 只能靠**渲染文本反推**，而反推 `blocks[]` 是把「看起来对」当读数。契约 §3.2 允许 `null = 本次未采集`，因此**登记为未实现项**（§9-3）而不是填一个全 0 对象 |
| 「含 `path` 的注入事件数 / 总注入事件数」 | **未取**（§9-11） | 需要一次端到端跑（临时 root + 临时端口）后数 transcript；本单没有起守护进程 |

**改前 → 改后**：改前该事件**只有 `render`**（80 个历史 `context_injected` 全无 path/budget，契约 §3.2 基线）；改后**每个新事件都带 `path`**，`budget` 仍缺（如实登记）。

---

## 8 两条过期期望的修正（`crates/mock-agent/tests/e2e_daemon.rs`）

| 位置 | 改前 | 改后 | 理由 |
| --- | --- | --- | --- |
| `:2166`（现 ~2166-2191） | `m["content"] == "[distilled] 用户喜欢深色主题"` | 断言「存在携带该正文的行」**且**「其中至少一条 `distilled=true`」 | t347 移除了正文前缀（provenance 是**字段**）；顺带把 C-INT 的「派生布尔」变成**正例读数** |
| `:2182` | `query_row(SELECT entities_written …)` 与 `assert_eq!(Some(0))` | `SELECT COUNT(*)` ≥ 1（**至少一次尝试**） | t26 起蒸馏**按次记账**，同一 session 可能多行；拿「碰巧返回的那一行」当本次读数就是第 14 条要防的那种断言 |

---

## 9 未实现项与理由（逐条，不静默）

| # | 未实现项 | 理由 / 归属 | 需要什么才能闭合 |
| --- | --- | --- | --- |
| 1 | **MCP 5 个新只读工具**（`memory_forget_report`/`graph_search`/`graph_retrieve`/`wiki_pages`/`wiki_links`） | 本单预算用尽；`crates/mcp/src/**` 在 inScope 但未改 | 一个独立小单（约 5 个 tool 定义 + registry 注册 + roundtrip 测试） |
| 2 | **面板四页读数**（Memory/Knowledge 量纲显示、Wiki/Graph 页新字段与纠错入口） | 未改 `panel/**`；`npm run build` 绿但**没有 DOM 级读数**（没改就没有可读的东西） | 面板单：按契约 §4 收敛读数（不重设计视觉）+ e2e |
| 3 | **`context_injected.budget` 对象** | `render_context` 无报告型返回值（`crates/memory/src/inject.rs:588`，**mem-core 的文件**）；INT 不许改它 | mem-core 提供 `render_context_report`（或等价）；INT 只做装配 |
| 4 | **`recall_log` 逐列「有值/恒空/不适用」填充读数** | 需要一次真调用后对 24 列逐列 `SELECT`；本单没有起进程 | 一条端到端脚本（临时 root + 一次 `/recall`）后逐列读 |
| 5 | **`memory_write` 未知 store 的 HTTP 400 读数** | 未写 HTTP 级用例 | `crates/daemon/tests/`（inScope）加一条 400 用例 |
| 6 | **DEP-1 生产命中率「改前→改后」** | 需要冻结的 23 查询帧 + 活库副本；本单没有该帧 | 从 t3/t7 报告取帧清单，做 strict/loose/`resolve_seeds` 三方受控对照 |
| 7 | **wiki DEP-W**（`/wiki/pages` 新字段、纠错写端点装配） | 未改（wiki 的字段面在本单范围里，但预算用尽） | 小单：装配 + `knowledge_api.rs` 断言 |
| 8 | **G8「带数字的 coverage」读数** | 接线已按 RV-D2-1 走 async `lead_for`；读数需要一次带 wiki 页的端到端跑 | 端到端（临时 root，写一页 wiki，跑一次 chat/run，读 render 里的 `coverage=<数字>`） |
| 9 | **`crates/daemon/tests/knowledge_api.rs` 逐字段断言更新** | 未改（在 inScope） | 与 #6/#8 同一次 |
| 10 | **`as_of` 三写法逐字节一致读数** | 归一化在入口（代码级），但没有 3 次 HTTP 调用读数 | 端到端 3 次 `GET /graph/retrieve?as_of=…`（规范形/`Z`/`+08:00`）比对 `paths` 字节 |
| 11 | **「含 path/budget 的注入事件数 / 总注入事件数」** | 需要端到端跑 | 与 #8 同一次 |
| 12 | **§1.6 的 BREAK-* 项** | 契约里登记为破坏性变更的条目，本单未逐条处理 | 逐条小单 |

**对下一代的建议（含仍未闭环的部分）**：① 把 #1/#2（MCP + 面板）与 #7/#9（wiki 字段面 + 断言）分成三张小单——它们的接线形状都已冻结，缺的只是预算；② `budget` 是**唯一需要跨区改动**的一项：请 mem-core 先给报告型渲染，INT 再装配（否则只能永远 `null`）；③ 端到端读数（#4/#6/#8/#10/#11）应由 V-INT 用**一个临时 root 脚本**一次跑完，避免每单各起一次进程；④ **闭环的下一层**是「实体块」——本单已让图种子里程碑进入 `recall` 的响应与遥测，但**注入块里还没有 `graph` 段**（`tag_rank` 已给 graph rank 3 的词表位），这是下一代最值得做的一步。

---

## 10 verify 命令读数（两件证据：覆盖的 target + 本次重查的包）

```
$ powershell … scripts/cargo-team.ps1 test --workspace
  （见本报告 §10.1，含每一批 test result 行与「哪些 target 真的跑了」）

$ powershell … scripts/cargo-team.ps1 clippy --workspace --all-targets -DenyWarnings -CleanFirst ruagent-daemon
  （见 §10.2，含 clean -p ruagent-daemon (forced re-check) + Checking <crate> 行）

$ cd panel && npm run build
  ✓ built in 7.84s — build-panel: dist updated (55 assets, index.html swapped by rename) — exit 0
```

### 10.1 `test --workspace`（第一遍：**失败**，已修）
第一遍在 `-p ruagent-daemon --lib` 停下：`api::tests::knowledge_search_legs_match_recall_and_absent_legs_are_null` **FAILED**（`75 passed; 1 failed`）。失败正文就是 READINGS 里 `?legs=false` 那一格：我把 `legs` 置成了 `null`，而既有断言要求 `[]`（键的类型不随 opt-out 改变）。**修法**：`?legs=false` 下 `legs` 保持**空数组**、只有 leg 的**值**为 `null`。
**第二遍**：`test -p ruagent-daemon --lib` = **76 passed / 0 failed**（exit 0 / 45.9s）。**第三遍（完整）**：`test --workspace` = **exit 0 / 151.8s**，见 §10.3 的 target 清点。**注意编译面**：§10.1 的第二遍只覆盖 daemon 的 lib + lib test；「整个 workspace 绿」这一句只能由第三遍支撑。

### 10.2 `clippy --workspace --all-targets -DenyWarnings -CleanFirst ruagent-daemon`（**exit 101**，两条都在**别人的文件**里）

```
[cargo-team] clean -p ruagent-daemon (forced re-check)
[cargo-team] exit=101 elapsed=13.4s
    Removed 2414 files, 7.2GiB total
    Checking ruagent-core … store … orchestrator … acp … graph … knowledge … memory
    Checking ruagent-daemon v0.1.0 … cli … mock-agent
error: items after a test module  --> cli\src\main.rs:809:1
error: this boolean expression can be simplified  --> crates\mock-agent\tests\wiki_pipeline.rs:1225:9
```

| # | 文件 : 行 | lint | 归属（为什么不是我改） |
| --- | --- | --- | --- |
| 1 | `cli/src/main.rs:809` | `clippy::items_after_test_module`（`mod tests` 之后还有 `fn client()` 等 9 项） | **`cli/` 不在本单 inScope**（我的范围 = daemon 的 6 个 src 文件 + daemon/tests + mock-agent/tests/e2e_daemon.rs + mcp/src + orchestrator/src + core/src + acp/src/chat.rs + panel） |
| 2 | `crates/mock-agent/tests/wiki_pipeline.rs:1225` | `clippy::nonminimal_bool`（`!(coverage == Some(1.0) && !uncited.is_empty())`） | **该文件归 wiki（I-D）** —— 我的范围只有同目录的 `e2e_daemon.rs` |

**我自己的面是干净的**：这一轮 clippy 把 core / store / orchestrator / acp / graph / knowledge / memory / daemon / cli / mock-agent 全部 `Checking` 过（`-CleanFirst` 强制重查），**daemon 的 lib 与全部 test target 没有报任何诊断**；两条错误都在上面点名的文件里。按纪律 #10 我只报坐标、不代改。

### 10.3 覆盖率与「哪些 target 真的跑了」（§6.0 第 1–2 条）

`test --workspace`（第三遍）**没有在第一个失败 target 停住**，因为它一条都没有失败；stderr 的 target 清点 = `ruagent`(main) · `ruagent-acp` · `ruagent-core` · **`ruagent-daemon --lib`(76)** · **`--test injection_e2e`(0 passed / 7 ignored)** · **`--test knowledge_api`(5)** · `--test smoke` · `ruagent-graph`(lib + multihop-gold 7 + resolution 3 + seed-resolution 5 + temporal 5 + live-after(3 ignored) + extraction-gold 7 + entity-query-shapes 3 + empty-recall-pattern 1 + fixture 1 + fixture-ownership 1) · `ruagent-knowledge`(lib 35 + score-kind-wire 2 + retrieval-legs 9 + retrieval-quality 3 + cjk 3 + residual-scan 6 + gold-seed-idempotence 3 + retrieval-gold-copy/live(2 ignored)) · `ruagent-mcp`(lib 10 + roundtrip 1) · `ruagent-memory`(lib 59) · `ruagent-mock-agent`(lib 4 + **`--test e2e_daemon` 23** + integration + judge 2 + registry_api 2 + mcp_health 1 + chat_experience 5 + **wiki_pipeline 15**) · `ruagent-orchestrator` 4 · `ruagent-policy` 7 · `ruagent-store`(32 + 3 ignored) · 全部 Doc-tests。

**⇒ 本单的「test --workspace 通过」覆盖了上列每一个 target**（不是「第一个失败前的那些」）。第一遍的红已按 §10.1 修好并重取。

### 10.4 三条 verify 的最终读数

| 命令 | 结果 |
| --- | --- |
| `scripts/cargo-team.ps1 test --workspace` | **exit 0 / 151.8s**（覆盖 §10.3 的全部 target；含 daemon 76、e2e_daemon 23、wiki_pipeline 15、store 32） |
| `scripts/cargo-team.ps1 clippy --workspace --all-targets -DenyWarnings -CleanFirst ruagent-daemon` | **exit 101**，两条诊断在 `cli/src/main.rs:809`（不在 inScope）与 `crates/mock-agent/tests/wiki_pipeline.rs:1225`（I-D）；**我自己的文件零诊断**（本轮 `Checking` 过 daemon + 其全部 test target） |
| `cd panel; npm run build` | **exit 0 / 7.84s**，`build-panel: dist updated (55 assets, index.html swapped by rename)` |

---

## 11 纪律回执（§6.0 第 1–15 条自检）

| 条 | 本单的落地 |
| --- | --- |
| 1–5 编译面 | 每次改动后 `check -p ruagent-daemon --all-targets`；§10.3 明确写出「哪些 target 真的跑了」；`--all-targets` 的 *all* 不当作保证 |
| 6–10 lint 假绿/竞态 | lint 门用 `-CleanFirst ruagent-daemon`（单窗口），读数含 `clean -p` + 本次 `Checking` |
| 11 空跑绿 | 本单的测试都是**真跑**；`?legs=false` 失败是**真红**，且已定因（断言形状不符，不是缓存） |
| 12 跨 crate 签名广播 | 本单改了 `RunEvent::ContextInjected`（**core 的公开枚举**）⇒ 受影响面 = `crates/acp`、`crates/daemon`、任何反序列化 transcript 的消费方；`#[serde(default)]` 保持旧数据可读；**在本报告与给 captain 的消息里点名**（§7 与消息天） |
| 13 同形多处替换 | 本单没有「先删后改」的中间态（`search_page` 的三处调用点与 `ContextInjected` 的三处构造点都是同一批编辑内完成，且每批后立即 `check`） |
| 14 根因是假设 | §4（DEP-1 命中率）与 §9-4/§9-8 都写成**未测/未定因**，没有把代码论证写成读数 |
| 15 陈旧产物 / 读到别人的数据 | 本单所有读数都注明「在最终字节上重取」；临时 root 用后删除（本单未创建新 root：测试自己管理临时目录，共享缓存未被我 `clean` 以外的方式改动） |

---

## 12 复现命令

```powershell
# ① 编译面
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 check -p ruagent-daemon --all-targets
# ② 闭环相关单测（含 legs/量纲/蒸馏布尔）
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-daemon --lib
# ③ 全 workspace（第一遍若红，按 §6.0 报名与目标）
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test --workspace
# ④ lint 门（强制重查）
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 clippy --workspace --all-targets -DenyWarnings -CleanFirst ruagent-daemon
# ⑤ 面板
cd panel; npm run build
```

---

## 13 t45 补完（本轮追加，不改 §1–§12 的既有结论）

### 13.1 已交付 + 读数

| # | 项 | 读数 |
| --- | --- | --- |
| 1 | **`cli/src/main.rs:809`（`items_after_test_module`）** | 在 `mod tests` 上加**作用域限定的 `#[allow(clippy::items_after_test_module)]` + 理由注释**（移动 ~500 行到文件尾部是「无人拥有的文件里的大重排」，diff 远大于它消掉的 lint）。核对：`clippy -p ruagent --all-targets -DenyWarnings` ⇒ **exit 0 / 7.4s**（同时满足新第 16 条：改的是 cli crate，就跑 cli 的门） |
| 2 | **workspace lint 门转绿** | `clippy --workspace --all-targets -DenyWarnings -CleanFirst ruagent-daemon` ⇒ **exit 0 / 12.2s**；两件证据：`clean -p ruagent-daemon (forced re-check)` + `Removed 1338 files, 6.1GiB total`，且本轮 `Checking` 了 daemon / mock-agent / mcp / cli（覆盖 = `--all-targets` 的 lib + lib test + 全部 test target）。另一条 `crates/mock-agent/tests/wiki_pipeline.rs:1225` 由 **t44（wiki）** 落地后一并转绿 |
| 2b | **`test --workspace`（本轮最终字节）** | **`TEST_EXIT=0`**；日志 `%TEMP%\t45-workspace-test.log` 里 **44 条 `test result: ok`**、**0 条 `FAILED`/`panicked`**，覆盖 daemon 76 · e2e_daemon 23 · wiki_pipeline 15 · store 32/3 ignored · memory 59 · mcp 10+1 · knowledge 35+9+3+6+3 · graph 全套 · mock-agent 其余 · 全部 doc-tests（「哪些 target 真的跑了」按逐条 `test result` 行清点，不是「首个失败前的那些」） |
| 3 | **五条跨区读数**（临时 root + 临时端口，一次跑完；**未写活库**） | 见 §13.2：②④⑤ **一手读数**；① 受控语料读数（生产帧未对照）；③ 未测（理由 §13.3-①） |
| 4 | **§6.0 第 16 条 + R-20** | **inScope 跨两个 crate ⇒ 门禁必须覆盖两个 crate**（三实例：`distill.rs` 属 daemon、`wiki_pipeline.rs` 属 mock-agent、`cli/` 无人拥有）；已写入契约 §6.0 与修订记录 R-20 |

### 13.2 五条读数的实际结果

探针（一次临时 root 进程：**pid 80360**，`127.0.0.1:8799`，root `%TEMP%\ruagent-t45-probe`，`RUAGENT_EMBEDDER=hash` + mock agent `--behavior echo`）：ingest 一份文档 → 建 3 个实体 + 1 条带 `valid_at` 的事实 → 3 次 `/recall`（写 `recall_log`）→ `as_of` 三写法 → 一次 chat（产出 `context_injected`）→ 直读临时 DB。

| 读数 | 结果 |
| --- | --- |
| **② `as_of` 三写法一致性** | `2026-09-01T00:00:00+00:00` / `…Z` / `2026-09-01T08:00:00+08:00` ⇒ 响应长度**都是 1053**、SHA256 前 16 位**都是 `4D-54-17-A7-20-6`**、`paths=1`/`seeds=1` ⇒ **`as_of_all_equal=True`（逐字节相同）**；非时刻串 ⇒ **HTTP 400** |
| **④ 含 `path`/`budget` 的注入事件计数** | `transcripts=1`、`context_injected=1`、**`with_path=1`**、**`with_budget=0`** ⇒ `path` 1/1；`budget` 0/1（`None` = 本次未采集，见 §13.3-③） |
| **⑤ `recall_log` 逐列填充（24 列 / 3 行）** | **有值 22 列**（`id, ts, query, strategy, top_n, memories, knowledge, wiki, entities, top_knowledge_score, top_knowledge_relevance, top_knowledge_relevance_kind, knowledge_leg_window, scoring_version, graph_entities, graph_paths, score_kind, fusion, top_legs_json, candidates_json, selected_json, rejected_json`，各 `non_null=3`）；**恒空 2 列**：`top_memory_score` 0/3（探针没种记忆 ⇒ 记忆腿为空 ⇒ **不适用**）与 `source` 0/3（调用方**未声明**来源，`NULL = unknown` 是设计 ⇒ **不适用**） |
| **① 实体腿（DEP-1）受控语料** | `PROBE45ALIASONLY`（只在实体名）⇒ `graph.entities=1`；**`PROBE45FACT`（只在 `fact_text`）⇒ `graph.entities=0`**；`ruagent runs on Kylin` ⇒ `graph.entities=2, graph.paths=1`。同响应 `score_kind=rrf_rank`、`leg_window=60`、`scoring_version=1`、`top_knowledge_relevance=0` + `…kind=calibrated` ⇒ **H-2「分数与窗口/版本同行」在 HTTP 面成立** |
| **③ G8 带数字 coverage** | **未测**（理由 §13.3-①） |

### 13.3 仍未交付 / not_measured（逐条，含原因与 owner）

| # | 项 | 状态 | 原因 / 下一步 |
| --- | --- | --- | --- |
| ① | **G8「带数字的 coverage」** | **not_measured** | 要让 wiki 构建**落地**一页（`wiki_pages.cite_coverage` 有记录值）需要一个**脚本化**的 mock agent：构建会调 agent 生成页面，`--behavior echo` 的正文**过不了引用门** ⇒ 页面 `failed` ⇒ 没有记录值 ⇒ 恒 `unknown`。**下一步**：用 `wiki_pipeline` 同款的脚本化应答起一次临时 root，再跑一次 chat 注入读 `coverage=<数字>`。**本轮不声称**「含 `coverage=` 即算」 |
| ② | **DEP-1 生产 23 查询帧三方对照** | **not_measured** | 冻结帧在 t3/t7 报告里，且 1/23 vs 13/23 是**活库**读数；复现需 `VACUUM INTO` 活库副本并按帧对齐语料。本轮给的是**受控语料**读数；其中「**只在 `fact_text` 里出现的词 ⇒ 0 实体**」是一条给 graph 的发现。**按 §6.0 第 14 条：成因未定因** |
| ③ | **`budget` 对象** | **not_measured（契约 §3.2 允许的处置）** | `render_context` 只返回字符串（`crates/memory/src/inject.rs:588`，mem-core 的文件）；`per_block_chars/total_chars` 来自 `InjectionBudget::default()`、`used_chars` 可算，但 **`blocks[]` 只能从渲染文本反推** —— 那就是把「看起来对」当读数。**处置**：`budget: None` = 本次未采集（读数 `with_budget=0`）。**owner**：mem-core 给报告型渲染后装配 |
| ④ | **MCP 5 个新工具** | **未交付** | 本单预算用尽（先做 lint 门 + 五条读数）。**owner**：下一张 INT 单；形状已在契约 §2 冻结（`memory_forget_report`/`graph_search`/`graph_retrieve`/`wiki_pages`/`wiki_links`） |
| ⑤ | **面板四页 DOM 级读数** | **未交付** | 未改 `panel/**`；`npm run build` 绿（55 assets）但**没有改动就没有可读的页**。**owner**：下一张面板单 |
| ⑥ | **wiki DEP-W + `knowledge_api.rs` 逐字段断言** | **未交付** | 未改（文件在 inScope）。**owner**：与 ⑤ 同一张单（同属「消费面字段装配」） |
| ⑦ | **§1.6 BREAK-* 逐条处置** | **部分** | 已处置：`RunEvent::ContextInjected` 用 `#[serde(default)]` 保旧 transcript 可读（**新事件**读数是 §13.2 ④；**旧事件的反序列化读数我没有取** ⇒ 该子项仍 not_measured）。其余 BREAK-* 未逐条处置（随各自区域单） |

### 13.4 本轮纪律回执（§6.0 第 1–16 条）

- **第 1–5 条**：读数以**逐 target** 形式给出（§10.3 + §13.2）；本轮两条门都绿，不存在「首个失败 target 挡住其余」。
- **第 6–10 条**：workspace clippy 用 `-CleanFirst ruagent-daemon`，证据含 `Removed 1338 files, 6.1GiB` + 本轮 `Checking <crate>` 行。
- **第 11/14 条**：§13.3-①② 写成 **not_measured / 未定因**，没有把推理当读数。
- **第 12 条**：core 枚举形状变更已在 t19 及本报告广播（受影响面 acp/daemon/transcript 消费方）。
- **第 15 条 ③**：探针 root **已删除**（`root_cleaned=True`），pid **80360 已停**（`pid_alive=False`）。
- **第 16 条**：本单改了 `cli/` ⇒ 额外跑了 `clippy -p ruagent --all-targets -DenyWarnings`（exit 0）⇒ **被改文件所在 crate ⊆ 被跑过的 `-p <crate>` 集合**成立。
- **未触碰**：pid 79984 全程只读未启停；**未写活库**（探针只写自己的临时 root）。

---

## 14 t46 补完（第 2 轮，本轮追加）

### 14.1 MCP 5 个新工具：**已实现并注册**（`crates/mcp/src/lib.rs`），4/5 调用读数 + **1 条暴露了缺失的 HTTP 路由**

在 `#[tool_router] impl PlatformTools` 里新增 5 个只读工具（薄代理到同名 HTTP 路由，**不另起实现**）：`memory_forget_report`（`GET /api/v1/forget-report?content_hash=`）· `graph_search`（`GET /api/v1/graph/search?q=&limit=`）· `graph_retrieve`（`GET /api/v1/graph/retrieve?q=&hops=&as_of=&include_superseded=`）· `wiki_pages`（`GET /api/v1/knowledge/wiki/pages`）· `wiki_links`（`GET /api/v1/knowledge/wiki/links`）；新增 3 个参数结构（`HashParams` / `GraphSearchParams` / `GraphRetrieveParams`）。

**读数（MCP stdio 驱动，临时 root + 临时端口；daemon pid 81632、mcp 子进程 exit 0）**：

| 读数 | 值 |
| --- | --- |
| `tools/list` | **14 个** = 9 个既有 + `graph_retrieve, graph_search, memory_forget_report, wiki_links, wiki_pages` |
| `graph_retrieve` 的 JSON Schema | `required=["query"]`；`hops{integer,null}` / `as_of{string,null}` / `include_superseded{boolean,null}` —— 与声明一致 |
| `memory_forget_report` 的 Schema | `required=["content_hash"]`，描述「64 hex chars」 |
| `graph_search{q:"ruagent"}` | `{"entities":[{"id":1,"name":"ruagent","kind":"project",…}],"match":"exact"}` |
| `graph_retrieve{q:"ruagent runs on Kylin",hops:2}` | `{"seeds":[{"entity":{"id":1,…},"leg":"name_token","score":0.7},{"entity":{"id":2,"name":"Kylin",…}}],…}` |
| `wiki_pages{}` | `{"pages": []}` |
| `wiki_links{}` | `{"nodes":[],"edges":[],"broken":[],"orphans":[],"wanted":[],"unreachable":[],"self_links":[],"degrees":[],"readings_at":"2026-09-27T19:34:42Z"}` |
| **`memory_forget_report{content_hash:"0"×64}`** | **`daemon call failed: error decoding response body`** |

**这一条失败暴露的是守护进程侧缺一条路由（真 finding）**：`grep 'forget' crates/daemon/src/*.rs` ⇒ **daemon 里没有 `/api/v1/forget-report`，也没有任何 forget 路由**；该能力目前只在 `crates/memory` 的报告函数里。契约 §1 的 A-10 锚点假定它存在 ⇒ **HTTP 面缺这一条**，工具注册成功、调用必失败。**修法很小**（`crates/daemon/src/api.rs` 加一个 handler，正是本单 inScope），但本轮预算用尽 ⇒ **登记为 finding INT46-1，不冒充绿**。

**§6.0 第 15 条 ① 的现场实例**：同一探针**第一次**跑出 `tools/list` = **9 个**（没有我的 5 个）—— 因为 `debug\ruagent.exe` 是 t45 的**旧二进制**；`build -p ruagent` 重编后同一脚本才读出 **14 个**。**绿和红都可能来自陈旧产物**；这次是「红来自旧二进制」。

### 14.2 本轮未交付（逐条：原因 + owner）

| # | 项（t45 §13.3 的编号） | 状态与原因 |
| --- | --- | --- |
| 1 | **面板四页 DOM 读数（+ wiki DEP-W + `knowledge_api.rs` 断言，按 captain 指示合单）** | **未交付**：本轮预算先用在与 MCP + 探针上。**owner**：下一轮同一张「消费面字段装配」单；DOM 读数走 repo 的 `panel/e2e` 入口 |
| 2 | **G8 数字 coverage** | **未交付，原因已定**：需要**脚本化** mock agent（`--behavior echo` 的页过不了引用门 ⇒ 无记录值）。**owner**：INT 下一轮，复用 `crates/mock-agent/tests/wiki_pipeline.rs` 的脚本化应答形状；产出前**不许**用「含 `coverage=` 即算」 |
| 3 | **DEP-1 生产帧三方对照** | **未交付（未做，不是不可测）**：需要 `VACUUM INTO` 活库副本 + t3/t7 的 23 帧清单。**owner**：INT 下一轮（captain 已裁定这是实现者读数） |
| 4 | **旧 transcript 反序列化读数** | **未交付**：计划在 `crates/daemon/tests/` 写一条旧形状 `{"type":"context_injected","render":"X"}` 的反序列化断言（读 `path=None`/`budget=None`）。**owner**：INT 下一轮（**最易补的一条**） |
| 5 | **§1.6 其余 BREAK-*** | **未逐条处置**（同 t45 登记；owner 随各自区域单） |

### 14.3 对 t45 §13.2 ① 观察的**更正**（`PROBE45FACT` ⇒ 0 不是缺陷）

**先解释再更正**（按 captain 要求，也按 §6.0 第 14 条）：`SeedLeg` 的**五个腿**是 `ExactName / AliasTable / NameToken / SummaryFts / VectorNearest`（`crates/graph/src/retrieve.rs:144-156` 的 `as_str`/`score` 两个 match 逐字列出，`:164-166` 的 `ALL` 同序），**其中没有「事实文本」腿** —— 种子来源是实体名、别名表、名字 token、摘要 FTS、向量近邻。因此「只在某条边的 `fact_text` 里出现的 `PROBE45FACT` ⇒ `graph.entities=0`」是**正确行为**：它既不是实体名，也不是五条腿里任何一条。

**⇒ 我撤回 t45 报告 §13.2 ① 里「（给 graph 的发现）」这半句**（原文保留，按追加式修订在此更正）：把正确行为登记成缺陷，与「构造性达标」是同一枚硬币的两面 —— **两者都是失真**。

### 14.4 本轮门禁读数

- `test --workspace` = **exit 0**（日志 `%TEMP%\t46-gates.log`；逐 target 清点见 §10.3 的同一形状，本轮无 FAILED）
- `clippy --workspace --all-targets -DenyWarnings -CleanFirst ruagent-daemon` = **exit 0**（含 `clean -p ruagent-daemon (forced re-check)` + 本轮 `Checking <crate>` 行）
- `cd panel && npm run build` = **exit 0**
- `check -p ruagent-mcp --all-targets` = exit 0（本单改动区的编译面）

### 14.5 纪律回执（本轮）

- **§6.0 第 15 条 ①**：本轮的「9 个工具」正是**旧二进制**造成的红；`build` 重编后同脚本得 14 个 —— 已在 §14.1 登记为现场实例。
- **§6.0 第 14 条**：§14.3 的 `PROBE45FACT` 做了**反做式核对**（读 `SeedLeg` 的全部腿，而不是解释现象）：现象**不是**缺陷 ⇒ **撤回**此前的登记。
- **§6.0 第 12 条**：本单改了 `crates/mcp/src/lib.rs` 的公开工具面（新增 5 个 tool 与 3 个参数结构）—— 影响面 = MCP 客户端（可见 tool 数 9 → 14）与契约 §2 的对照表；**未改任何既有工具的签名**。
- **临时 root**：`%TEMP%\ruagent-t46-mcp` **已删除**，daemon pid **81632 已停**（`daemon_alive=False`）、mcp 子进程 exit 0；`t46-mcp-{in,out,err}` 已删。
- **pid 79984** 未触碰；**未写活库**。

---

## 15 t47 补完（第 3 轮：P1 分级）

### 15.1 P1① INT46-1：`/api/v1/forget-report` 路由**已补**（`crates/daemon/src/api.rs`），工具**真的可用**

- **改动**：路由 `.route("/api/v1/forget-report", get(forget_report))` + handler（`content_hash` 必须 **64 位十六进制**，否则 **400**；调用 `ruagent_memory::lifecycle::forget_report`，**不加法** —— 报告的读数就是 memory 报告的读数；`total` 在**一处**派生）。
- **成功读数（HTTP，临时 root + 端口 8802，daemon pid 37468）**：`GET /api/v1/forget-report?content_hash=6206…3944` ⇒ **200**，体 = `{"content_hash":"6206…3944","local":[4 面],"external":[6 面],"total":10}`；`local` 中 `memories` = `readout{hits:1,truncated:false,total:1}` + `sample[{"key":"memory/1 (user)",…}]`、`memories_fts` = `hits:1`、`derived` = 0、`episodes` = `not_available{…}`；`external` 6 条全 `not_available` 且**各自点名 owner**（I-A/recall、I-D/wiki）。
- **MCP 工具读数（stdio，`memory_forget_report`）**：真哈希 ⇒ **成功**（同形）；**从未写过的 `aaaa…`（64 位）** ⇒ 成功、各面 `hits:0`、`sample` 空；**非 64 位串** ⇒ **HTTP 400**。⇒ 该工具从「注册成功但必然失败」变成「**可用 + 边界诚实**」。
- **我改掉了自己的一条错说法（§6.0 第 14 条）**：handler 注释原写「未知哈希 ⇒ `total == 0`」—— **实测 `total: 10`**（`total` 是**报告携带的面读数个数**，不是残留计数，写入/未写入哈希都一样）⇒ 注释按实测改正，并把「怎么读未知哈希」（看 `local[].status.readout.hits` 全 0 + `sample` 空）写进注释。

### 15.2 P1② 旧 transcript 反序列化用例：**已交付**

新增 `crates/daemon/tests/event_compat.rs`（2 条用例）：
- `an_old_context_injected_event_still_deserializes`：喂**旧形状** `{"type":"context_injected","render":"…old bytes…"}`（无 `path`/`budget`）⇒ **反序列化成功**、`render` 字节保真、`path == None`、`budget == None`（**未知，不是编出来的 `"run"`**）。
- `a_new_context_injected_event_still_reads_its_values`：新形状 ⇒ `path == Some("chat")`、`budget.is_some()` —— 专抓「字段存在但没人读」。
- **读数**：`test -p ruagent-daemon --test event_compat` ⇒ **`2 passed; 0 failed`**（`Running tests\event_compat.rs`）。`#[serde(default)]` 的证据是这次**反序列化**，不是属性本身。

### 15.3 P1③ G8 带数字 coverage：**本轮未产出**（显式登记，不近似冒充）

| 项 | 内容 |
| --- | --- |
| 状态 | **未产出**（**不是**「已证明不可产」） |
| 机理（已定位） | wiki 构建要**落地**一页 ⇒ 需要 agent 生成**过引用门**的正文；`--behavior echo` 的回显过不了 ⇒ 页面 `failed` ⇒ `wiki_pages.cite_coverage` **没有记录值** ⇒ 渲染里恒 `unknown`（t45/t46 两次实测） |
| 需要的形状（入口已探明） | `ruagent-mock-agent --behavior scripted --replies replies.json`，`ScriptedReply` 按 **marker** 命中（`crates/mock-agent/src/lib.rs:146-159`）；应答正文需带 wiki 构建要求的锚点/引用形状 |
| 本轮为何没做 | 预算：P1① 补路由 + 重建（第 15 条）+ 探针 + 旧 transcript 用例 + 三条门禁已占满；**未用近似形状充当读数** |
| owner | INT 下一轮（或 wiki 单）：读 `ScriptedReply` 字段与 wiki 构建的页 JSON 形状 ⇒ 起一次临时 root 建一页，再跑一次 chat 注入读 `coverage=<数字>` |

### 15.4 P2 / P3（如实记录，未完成）

| 项 | 状态 | 原因 + owner |
| --- | --- | --- |
| **P2 面板四页 DOM + wiki DEP-W + `knowledge_api.rs` 断言** | **未完成** | 本轮预算全部用于 P1；`npm run build` 绿但无 DOM 读数。**owner**：下一轮「消费面字段装配」单（panel + wiki 端点字段 + knowledge_api 断言，走 repo e2e 入口） |
| **P3 DEP-1 的 23 帧三方对照** | **未完成** | 需 `VACUUM INTO` 活库只读副本 + t3/t7 冻结帧清单。**owner**：下一轮（实现者读数） |
| **P3 §1.6 其余 BREAK-*** | **未逐条处置** | 同 t45/t46 登记；owner 随各自区域单 |

### 15.5 门禁读数（最终字节）

- `test --workspace` = **`TEST_EXIT=0`**（日志 `%TEMP%\t47-gates.log`；**全部 `test result: ok`、0 条真失败**。附一条读数工件：我先用 `Select-String 'FAILED'` 数出 58 条 —— PowerShell 默认**大小写不敏感**，把 38 条「0 failed」行也算进去了；逐 target 清点含 daemon 76 · e2e_daemon 23 · wiki_pipeline 15 · **event_compat 2** · store 32/3 ignored · memory 59 · mcp 10+1 · knowledge/graph 全套 · doc-tests）
- `clippy --workspace --all-targets -DenyWarnings -CleanFirst ruagent-daemon` = **exit 0 / 10.9s**；两件证据 = `clean -p ruagent-daemon (forced re-check)` + `Removed 1566 files, 6.2GiB total` + 本轮 `Checking ruagent-daemon / ruagent-mock-agent / ruagent-mcp / ruagent(cli)`
- `cd panel && npm run build` = **exit 0 / 5.53s**（55 assets）
- 探针收尾：`daemon_alive=False`、`cleaned=True`（root `%TEMP%\ruagent-t47-forget` 已删、pid 37468 已停、`t47-in/out` 已删）；**pid 79984 未触碰**；**未写活库**

### 15.6 本轮结束时仍未交付项（一句话清单，供下一轮直接接续）

1. **P1③ G8 带数字 coverage**（脚本化 mock：`ScriptedReply` 形状 + wiki 页 JSON 形状）—— 本代最后一个「构造性达标」风险点；
2. **P2 面板四页 DOM 读数 + wiki DEP-W + `knowledge_api.rs` 断言**（合单）；
3. **P3 DEP-1 的 23 帧三方对照**（`VACUUM INTO` 副本 + 帧清单）；
4. **P3 §1.6 其余 BREAK-*** 逐条处置。

---

## 16 t48 补完（第 4 轮：P1 = G8 唯一 blocker）

### 16.1 G8：**仍未产出**，但入口被推进到「只差引用锚点的识别细节」（附可直接复用的失败证据）

**做成的部分（脚本化 mock 真的驱动了 wiki 构建）**：临时 root + `--behavior scripted --replies replies.json`（`ScriptedReply{marker, reply}`；两个 marker 取自分文提示词首行 —— **`WIKI PLANNER`**、**`WIKI PAGE WRITER`**，见 `crates/daemon/src/wiki.rs:1301/1333`），ingest 一份文档后 `POST /api/v1/knowledge/wiki/build {scope:"all", agent:"wiki"}`。**两段脚本都命中了**（证据在临时 DB 行里）：

| 表 | 读数（临时 root 的 `data/ruagent.db`） |
| --- | --- |
| `wiki_builds` | `['1','all','done','0','wiki','1','0','1',…]` ⇒ `pages_planned=1`、`pages_written=0`、`pages_failed=1` |
| `wiki_build_pages` | `['1','deploy-pipeline','create','failed', 'citation check failed (1 problems); uncited section [运行时间]: section \`运行时间\` of page \`deploy-pipeline\` has no citation anchor']` |

⇒ **planner 回复被接受**（页 `deploy-pipeline` 被计划出来）、**writer 回复也被接受**（有正文），**只卡在引用门**：我给的锚点行 `<!-- cite: t48-doc#0 -->` **没有被识别成 section 的锚点**。

**G8 要的两个数（读数）**：`GET /api/v1/knowledge/wiki/pages` ⇒ `{"pages":[]}`（页没落地，`wiki_pages` 无行 ⇒ `cite_coverage` 无记录值）；注入侧 `context_injected=1`，`coverage=` 的**大小写敏感**匹配（.NET `[regex]::Matches(render,'coverage=[^\s\)]*')`，默认区分大小写）⇒ **0 条**、`coverage_with_digits_count=0`。**⇒ 不达标，也不用近似形状冒充。**

**本轮排掉的一个假设**：先怀疑 PowerShell `-Encoding utf8` 的 **BOM** 让 `serde_json` 解析失败 ⇒ 幂等重跑并改用 `UTF8Encoding($false)` 写 **BOM-free** 的 replies.json ⇒ **两次行为完全一致** ⇒ **BOM 不是原因**。（该重跑还暴露：`Get-Content -Encoding Byte` 在 PowerShell 7 上不可用 ⇒ 我的字节检查**根本没跑到**，「BOM 假设」原本就缺证据 —— 如实记下这个缺席的读数。）

**状态（按本单要求写）**：**未产出**（**不是**「不可产」）。这是本代**最后一个构造性达标风险点**，**由 RV-INT（t21）带着该缺口判整代**。**下一步确切入口**：① 读 `crates/daemon/src/wiki.rs` 的引用解析与 section 切分（`cite:` 行的位置/格式、chunk id 形状 —— 我的 `#0` 未被识别）；② 用 `wiki_build_pages.error` 作负控（本单已给一份可直接复用的失败行文本）；③ 落地后 `wiki_pages.cite_coverage` 会有记录值，再跑一次 chat 读注入块里的 `coverage=<数字>`。

### 16.2 P2 / P3（未完成，如实记录）

| 项 | 状态 | 原因 + owner |
| --- | --- | --- |
| **P2 面板四页 DOM + wiki DEP-W + `knowledge_api.rs` 断言** | **未完成** | 本轮预算全部用于 G8 的两次探针（含一次诊断重跑）与三条门禁；`npm run build` 绿但无 DOM 读数。**owner**：下一轮「消费面字段装配」单 |
| **P3 DEP-1 的 23 帧三方对照** | **未完成** | 需 `VACUUM INTO` 活库只读副本 + t3/t7 冻结帧清单。**owner**：下一轮 |
| **P3 §1.6 其余 BREAK-*** | **未逐条处置** | 同 t45–t47 登记；owner 随各自区域单 |

### 16.3 门禁读数（最终字节；计数一律用**大小写敏感**判据，第 17 条）

- `test --workspace` = **`TEST_EXIT=0`**；`Select-String -CaseSensitive`：`test result: ok` = **55 行**、`FAILED` = **0 行**；`(\d+) passed` 求和 = **386 passed**（独立支撑：退出码 0）。
- `clippy --workspace --all-targets -DenyWarnings -CleanFirst ruagent-daemon` = **exit 0 / 11.2s**；两件证据 = `clean -p ruagent-daemon (forced re-check)` + `Removed 890 files, 4.4GiB total` + 本轮 `Checking ruagent-daemon / ruagent-mock-agent / ruagent-mcp / ruagent(cli)`。
- `cd panel && npm run build` = **exit 0 / 6.00s**（55 assets）。
- 清理：两个临时 root `ruagent-t48-g8` / `ruagent-t48-g8b` **均已删除**（`cleaned=True`）；探针 daemon pid **76632 / 85496 已停**；**pid 79984 未触碰**（`StartTime = 09/27/2026 05:35:37`）；**未写活库**。本轮**未改产品代码**（只写报告 + 临时文件）⇒ 第 15 条不触发（无需重编）。

### 16.4 本轮结束时仍未交付项（一句话清单 + 下一轮入口）

1. **G8 带数字 coverage** —— 入口：读 `crates/daemon/src/wiki.rs` 的 `cite:` 锚点解析形状，用本单的失败行文本作负控；**唯一 blocker**；
2. **P2 面板四页 DOM + wiki DEP-W + `knowledge_api.rs` 断言** —— 入口：`panel/e2e` + `/api/v1/knowledge/wiki/{pages,links}` 字段；
3. **P3 DEP-1 23 帧三方对照** —— 入口：`VACUUM INTO` 副本 + t3/t7 帧清单；
4. **P3 §1.6 其余 BREAK-*** —— 随各自区域单。

---

## 17 t50：G8 **已产出**（带数字 coverage，两侧对照）—— 本代最后一个构造性达标风险点关闭

### 17.1 被接受的锚点确切形状（只读取证，附行号）

| 问题 | 答案（证据） |
| --- | --- |
| **格式** | `<!-- cite: <document>#<chunk_id> -->` —— `const CITE_PREFIX = "<!-- cite:"` / `CITE_SUFFIX = "-->"`（`crates/daemon/src/wiki.rs:27-28`；定义注释 `:23-26`） |
| **位置** | **一行一个、HTML 注释形式**，写在它所支撑的 **H2 section 末尾**；解析器 `citations_in(body)`（`wiki.rs:531-535`）**逐 section** 收集，返回 `(section_title, document, chunk_id: i64)` ⇒ 锚点是对**某一个 H2** 的声明，不是对整页；**代码围栏内跳过** |
| **chunk_id 形状** | **`i64`**，正例用 **1 起的小整数**：`deploy-guide#1` / `tea-doc#2` / `src-alpha#1`（`crates/mock-agent/tests/wiki_pipeline.rs:256-260`，`ALPHA_BODY_V1` 注释写「chunk 1 exists after `put_source`」）⇒ 我 t48 用的 **`#0` 指不到任何 chunk**，该 section 于是被算作「没有锚点」（`wiki_build_pages.error` = `uncited section [运行时间]: … has no citation anchor`） |
| **是否必须引用真实存在的 chunk** | **是**（本轮的 A/B 就是它：同一份正文，只把 `#0` 换成 ingest 出的 `#1`，页就落地） |
| **豁免** | `## 来源` 与 `## 相关*` 不需要锚点（写手提示词 `wiki.rs:1338-1344`） |

**真实通过的页面正文样例（本轮过门的那一张，探针原文）**：

```
# 部署流水线

部署流水线是把代码从仓库送上生产环境的一条自动化链路，由 scripts/deploy.sh 驱动，相关约定见 [[tea-notes]] 与 [[runbook]]。

## 运行时间

部署流水线在每周二运行，从仓库根目录启动，合并之后执行。T48COVERAGE 标记。
<!-- cite: deploy-guide#1 -->

## 步骤

先构建，再跑测试，最后发布；任何一步失败都会终止整条流水线。
<!-- cite: deploy-guide#1 -->

## 来源

- deploy-guide
```

### 17.2 产出读数（脚本化 mock；临时 root + 端口 8806，daemon pid 65960）

方法同前（`--behavior scripted --replies replies.json`，marker `WIKI PLANNER` / `WIKI PAGE WRITER`），**先在临时 root ingest 源文档 `deploy-guide`，再引用它真实存在的 chunk `#1`（没有伪造 id、没有绕过 gate）**：

| 读数 | 值 |
| --- | --- |
| `wiki_builds` | `[(1, 'done', 1, 1, 0)]` ⇒ **`pages_planned=1`、`pages_written=1`、`pages_failed=0`** |
| `wiki_build_pages` | `[(1, 'deploy-pipeline', 'written', None)]` ⇒ **`status=written`、`error` 空**（与 t48 的 `failed` + `citation check failed …` 构成正/负对照） |
| **`wiki_pages.cite_coverage`** | **`1.0`（有记录值，非 NULL）**；同行 `sections=2`、`cited_sections=2`、`links_out=2`、`links_out_broken=2`、`self_links=0` |
| `GET /api/v1/knowledge/wiki/pages` | `{"slug":"deploy-pipeline",…,"freshness":"fresh","citations":2,"cite_coverage":1.0,"uncited_sections":[],"links_out":2,"links_out_broken":2,"self_links":0}` |
| **注入块** | `context_injected=1`；**大小写敏感**匹配 `[regex]::Matches(render,'coverage=[0-9][^ \r\n\)]*')` ⇒ **`coverage=1.00`**，`coverage_with_digits_count=1` |

**⇒ G8 达标**：构建侧 `cite_coverage=1.0`（记录值）与注入侧 `coverage=1.00`（**带数字**，不是 `unknown`）**两侧对上**。本代最后一个「构造性达标」风险点**关闭**。

### 17.3 本轮的一条读数教训（下一代纪律素材）

同一探针**第一次**跑出 `pages_planned=0`、`wiki_builds` 仍是 `running` —— 因为我**在构建完成前就读了 DB**。**该构建是异步的**（`POST …/wiki/build` 立刻返回 `status:"running"`）：**异步面的读数必须先轮询到终态**（本轮改成等 `wiki_builds.status` 到 `done`/`failed`，1 轮即到 `done`）。t48 那次读到的 `failed` 行之所以真实，是因为它在 `/wiki/pages` 与 chat 之后才读 —— **「早读」不是总是安全**。

### 17.4 门禁读数（最终字节；计数一律**大小写敏感**）

- `test --workspace` = **`TEST_EXIT=0`**；`Select-String -CaseSensitive`：`test result: ok` = **55 行**、`FAILED` = **0 行**；`(\d+) passed` 求和 = **386**。
- `clippy --workspace --all-targets -DenyWarnings -CleanFirst ruagent-daemon` = **exit 0 / 12.2s**；两件证据 = `clean -p ruagent-daemon (forced re-check)` + `Removed 890 files, 4.4GiB total` + 本轮 `Checking ruagent-daemon / ruagent-mock-agent / ruagent-mcp / ruagent(cli)`。
- `cd panel && npm run build` = **exit 0 / 7.03s**（55 assets）。
- 清理：临时 root `ruagent-t50-g8` **已删除**（`cleaned=True`）、探针 daemon pid **65960 已停**；**pid 79984 未触碰**（`StartTime = 09/27/2026 05:35:37`）；**未写活库**。**本单未改产品代码**（inScope 只有本报告与临时探针文件）⇒ 第 15 条不触发。
- **G8 关闭，本单无遗留 blocker**：面板四页 / wiki DEP-W / `knowledge_api.rs` 断言 / DEP-1 帧对照 / §1.6 其余 BREAK-* 已按 captain 裁决转 t51 与后续。

---

## 18 t53：声称面收口（面板类型检查入门禁 + 两处「文档事实错」的复核 + #18/#19）

### 18.1 面板类型检查：**已接进门禁**，并给了负控（红 → 绿）

- **改前**：`panel/package.json` 的 `build` = `node scripts/build-panel.mjs`（**只有 vite 构建，不含 `tsc`**）；同一个文件里**早就有一个 `check` 脚本**（`tsc -b --noEmit && tsc -p e2e/tsconfig.json --noEmit`），但它**从不在任何 verify 命令里** ⇒ **一个 TS 类型错误能静默通过**；而 `AGENTS.md` 那时写的是「`npm run build` # tsc + vite build」（**文档的系统声称 ≠ 实际门禁**）。
- **改后**：`"build": "tsc -b --noEmit && tsc -p e2e/tsconfig.json --noEmit && node scripts/build-panel.mjs"` ⇒ **`cd panel && npm run build` 这条既有 verify 命令现在真的覆盖类型检查**（复用既有入口，不新增第五条命令）。
- **负控读数（本单实测）**：

  | 步骤 | 读数 |
  | --- | --- |
  | 在 `panel/vite.config.ts` 末尾注入 `const t53TypeError: number = "not a number";` | `npm run build` ⇒ **`vite.config.ts(47,7): error TS2322: Type 'string' is not assignable to type 'number'.`** + `TS6133`（未使用）⇒ **`NEGCTL_BUILD_EXIT=1`（红）** |
  | `git checkout -- vite.config.ts` 撤销（`git diff --stat -- vite.config.ts` = 空） | `npm run build` ⇒ `✓ built in 7.57s` + `dist updated (55 assets)` ⇒ **`GREEN_BUILD_EXIT=0`（绿）** |

  ⇒ **门禁真的挡得住类型错误**（不是「脚本文档说含 tsc」而已）。负控用的文件是 **inScope 的 `panel/vite.config.ts`**（`panel/src/` 未碰）。

### 18.2 契约 §1.5 的两处「事实错」：复核结果**两条都不成立**

按「先复核再更正」办（§6.0 第 14 条）：

| 声称 | 复核读数 | 结论 |
| --- | --- | --- |
| 「契约把图端点创建写成复数 `/graph/entities`」 | 契约 `L92` 原文 = `POST /api/v1/graph/entity`（**单数**）；路由表 `api.rs:53` = `.route("/api/v1/graph/entity", post(graph_create_entity))`，`api.rs:49` = `.route("/api/v1/graph/entities", get(graph_entities))` ⇒ 复数**只有 GET**（复数 POST 确实 405） | **契约本来就对**；该「错误」**不在契约里** |
| 「`WikiLinks.degrees` 被写成 map」 | 契约 `L78` 原文 = `degrees:[{slug,links_in,links_out,links_out_broken}]`（**对象数组**）；`grep degrees` 全表命中 3 处（L78/L267/L427）**都是数组形状** | **契约本来就对** |

**处置**：在 §1.5 就地加一行 **「读法注」**（单数 POST + 复数只有 GET + `degrees` 是数组），修订记录记 **R-22b**，明确写「这两条声称不成立；若 wiki 看到的是**另一份文档**里的错字，请点名文件（owner：captain/wiki 指路）」。**把正确的文档登记成错误，与把错误的当正确一样是失真** —— 按本代纪律如实回报，而不为了「完成更正」去改一处本来正确的地方。

### 18.3 §6.0 第 18 / 19 条已入契约

- **第 18 条「异步面的读数必须先轮询到终态」**：实例 = t50 的 `POST …/wiki/build` 立即返回 `running`、早读得 `pages_planned=0`；改轮询后 `pages_written=1` / `cite_coverage=1.0`；**并写明反面**：t48 的 `failed` 是真的，只因读得够晚 ⇒ **「早读恰好读到终态」是运气不是方法**。
- **第 19 条「门禁必须写明它不覆盖什么」**：实例 = `panel/` 不属任何 Rust 包 + build 不含 `tsc`；**纪律 = 声称面必须与被测面一致**（本单处置见 §18.1）。

### 18.4 门禁读数（最终字节；计数一律大小写敏感）

- `test --workspace` = **`TEST_EXIT=0`**；`Select-String -CaseSensitive`：`test result: ok` = **55 行**、`FAILED` = **0 行**、`(\d+) passed` 求和 = **387**。（**注**：t50 同一命令求和是 **386**，差 1 —— **未定因**，不编解释；两个读数都如实记录。）
- `clippy --workspace --all-targets -DenyWarnings -CleanFirst ruagent-daemon` = **exit 0 / 10.8s**；两件证据 = `clean -p ruagent-daemon (forced re-check)` + `Removed 1164 files, 5.8GiB total` + 本轮 `Checking ruagent-daemon / ruagent-mock-agent / ruagent-mcp / ruagent(cli)`。
- `cd panel && npm run build` = **exit 0**（55 assets；**含类型检查**，负控见 §18.1）。
- **pid 79984 未触碰**（`StartTime = 09/27/2026 05:35:37`）；**未写活库**；本单不需要起进程（无临时 root）。

---

## 19 t55：更正「面板类型检查何时入门禁」的错误说法（并补上 t53 丢失的记录）

### 19.1 先取证：包装脚本一直做类型检查

`panel/scripts/build-panel.mjs`（t319 写、wiki 未改）：

| 行 | 内容 |
| --- | --- |
| L25-31 | `run(cmd)` = `spawnSync(cmd, {shell:true})`；`r.status !== 0` ⇒ `console.error("build-panel: \`${cmd}\` exited ${r.status}; dist untouched")` + `process.exit(...)` |
| L33 / L34 / L35 | `node e2e/i18n-check.mjs` / `npx tsc -b` / `npx tsc -p e2e/tsconfig.json --noEmit` |
| L39-42 | 故意失败钩子 `PANEL_BUILD_FORCE_FAIL=1`（在 vite 之前、任何写 `dist` 之前退出） |
| L44 | `npx vite build --outDir dist-staging --emptyOutDir` |
| L46-58 | **唯一**动 `dist` 的部分（staging → 拷 assets → 写 `index.html.tmp` 再 rename → 清孤儿） |

⇒ **`cd panel && npm run build` 一直覆盖类型检查**。t53 §18《改前》的前提（「原本只有 vite、不含 tsc ⇒ 类型错误能静默通过」）**不成立**；t54 写进 `AGENTS.md` 的「its type check only reached that command in t53」**也不成立**。§18.1 的那段历史陈述**以本节为准**（§18.1 的负控读数仍为真）。

### 19.2 只针对 wrapper 的负控（把 t53 的前缀临时移除）

| 步骤 | 读数 |
| --- | --- |
| `build` 临时改回 `node scripts/build-panel.mjs`（t319 状态）+ 注入 `const t55TypeError: number = "not a number";` | `vite.config.ts(47,7): error TS2322` + `TS6133` + **`build-panel: \`npx tsc -b\` exited 1; dist untouched`** ⇒ **`WRAPPER_ONLY_NEGCTL_EXIT=1`（红）** |
| `dist/index.html` 的 SHA256（失败前后） | 同为 **`98F8B0D3…`** ⇒ **`dist_untouched=True`** |
| `git checkout -- vite.config.ts` 后 | `✓ built in 7.38s` + `dist updated (55 assets)` ⇒ **`WRAPPER_ONLY_GREEN_EXIT=0`（绿）** |

⇒ t53 的负控读数**是真的**，但它证明的是「**这条命令能挡住类型错误**」，**不是**「它以前挡不住」—— **现象先测、解释后写（第 14 条）**。

### 19.3 处置：撤掉 t53 的重复前缀，把真实历史写回

- **`panel/package.json`**：`build` 回到 `"node scripts/build-panel.mjs"`（t53 的两步与 wrapper 内部**重复**，每轮多跑两次 `tsc`）。它是**被跟踪**文件（`git ls-files -v` = `H`）⇒ **`git diff --numstat -- panel/package.json` 为空**，这条空 diff **是证据**。
- **`AGENTS.md`**（被跟踪，`git diff --numstat` = **`30 2`**）：命令注释改成真实历史（一直覆盖类型检查，wrapper 内 L33/L34/L35/L44）；「不覆盖什么」段保留，并把它对 `check` 脚本的说法改成准确表述（**没人按名字调用它**是关于调用者的陈述，不是关于门禁覆盖面的陈述）；**no Rust gate protects the panel** 整段保留。
- **契约**：第 19 条实例按带日期记录更正（**R-23a**：旧文字逐字引用 + 为什么错 + 真实现状）、**R-23b** 记撤销、**第 19 条本体保留**；**R-23d** 加第 21 条。

### 19.4 第 20 / 21 条 + 一次「缺证据」的发现（本单最有价值的那半）

- **第 20 条**「判断一条命令做了什么，要读它真正执行的东西，不要只读入口名字」：`"build": "node scripts/build-panel.mjs"` 看起来只打包，实际跑 `i18n-check + tsc ×2 + vite`（`build-panel.mjs:33-44`）⇒ **同一错误被传播两次**（t53 进契约、t54 进 `AGENTS.md`）；**错误的说法比错误的读数更长寿**。
- **第 21 条**（captain 转 t56）「**没有 diff ≠ 没改**」：本单按它复核了自己 —— `AGENTS.md` / `panel/package.json` **被跟踪**（`H`）⇒ diff 可作证据；`gen2-integration-contract.md` / `gen2-integration-impl.md` **未跟踪**（`git ls-files -v` 无输出）⇒ 只能用**内容级读数**。
- **内容级读数抓出一处缺证据**：契约里 `R-22` 关键字计数 = **0** ⇒ **t53 那条修订记录当时根本没落盘**（编辑被「文件未读」拒绝后我漏了重试）⇒ 本单补上 R-22 并注明「补记（t55）」。after-state 留档：契约 **688 行**、2546 个数字串；`AGENTS.md` 55 个数字串；**未跟踪文件的 before-state 不可得 —— 这本身就是第 21 条的例证**。

### 19.5 门禁读数（最终字节）

- `cd panel && npm run build` = **exit 0 / 10.07s**（55 assets）。
- `test --workspace` = **`TEST_EXIT=0`**；大小写敏感：`test result: ok` **55 行**、`FAILED` **0 行**、`(\d+) passed` 求和 = **390**（t50 386 / t53 387 / 本单 390 —— 同窗口 mem-core 正在落地，**未定因**）。
- `clippy --workspace --all-targets -DenyWarnings -CleanFirst ruagent-daemon` = **exit 0 / 12.9s**，本轮 `Checking ruagent-daemon / ruagent-mock-agent / ruagent / ruagent-mcp`。
- **pid 79984 未触碰**（`StartTime = 09/27/2026 05:35:37`）；**未写活库**；无临时 root。
