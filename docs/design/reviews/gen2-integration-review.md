# RV-INT 评审（t21）：集成契约六节逐条判决

> **结论：`needs_revision`（任务置 failed）**。INT 这一代**真的把不少东西接上了**（下面 §1.3 逐条记名），但契约 §6 的锚点里还有**六条 medium 未达成**，其中两条是**契约原文点名要求**的（§3.1「读出必须**逐列透出**」、§1.5「`facts?at=` 接线时**同样要归一化输入**」），另有两条让 §6 的锚点**无法判**（`budget` 恒 null、`facts?at=` 静默 200）。按本单验收「**只有全部达成才 pass**」，我判 **needs_revision**。
> **评审员**：review（独立评审员，不是 INT 的作者）。inScope 只有本文件；`crates/`、`panel/` 一行未改。**真守护进程 pid 79984 未启停**，`~/.ruagent` 未写；所有 cargo 走 `scripts/cargo-team.ps1`（**未**自设 `CARGO_TARGET_DIR` —— 任务单里那一行是对全队纪律的有意替换）。
> **我的动手复核**：① 锚点命令 `test --workspace` 我自己重跑（**EXIT=0**）；② 面板我**自己**跑 `cd panel && npm run build`（**exit 0 / dist updated 55 assets**）并**逐字读**了两处标签与 i18n 文案；③ 六节里的每一条 finding 都给了**行号级**的代码读数，不是转述作者/V-INT。
> **输入**：`gen2-integration-contract.md`（157,918 B / 709 行）· `gen2-integration-impl.md`（t19）· `gen2-integration-verify.md`（t20 / V-INT）· 当前工作树字节（2026-09-29T0x:xx+08:00）。

---

## 1 我自己的读数

### 1.1 锚点命令（我跑的那一条）

```
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test --workspace
→ EXIT=0；44 条 `test result:` 行全部 `ok … 0 failed`（含 `32 passed; 3 ignored` 的 graph 面、
  `77 passed` 的 daemon 单测面、`62 passed`、`35 passed`、`23 passed` 等；
  另见 `0 passed; N ignored` 的活库形状仪器 —— 按契约 §6.0 第 11 条，它们**默认 ignored** 是正确的形状）
```
**声称面**（契约 §6.0 第 1/6/8 条）：这次调用 **exit=0**，`--workspace` 覆盖全部成员的**默认 target**；`--ignored` 的活库形状仪器**未跑**（我没有活库副本需求），因此我**不声称**那部分被覆盖。

### 1.2 面板（我自己跑 + 我自己读）

```
cd panel && npm run build   → exit 0 · "build-panel: dist updated (55 assets, index.html swapped by rename)" · 6.30s
```
* `panel/src/views/Memory.tsx:374-376` 用 `t("memory.topMemoryScore")`，文案（**我逐字读的**）：zh `"记忆腿余弦 {s}"` / en `"memory leg cosine {s}"`（`panel/src/i18n/memory.ts:47/127`），旁边 `knowledge.score` = `"名次分 {s}"` / `"rank score {s}"`（`knowledge.ts:24/88`），且 `knowledge.scoreHint` 明写「**它没有量纲，不是相似度**」。`memory.ts:45` 的注释把「记忆腿余弦 vs 知识腿无量纲 RRF 名次分」**写进了源码**。
* `panel/src/views/Wiki.tsx:192-206`：coverage **三态徽标**在（`cite_coverage == null` ⇒ `t("wiki.coverageUnknown")` = `"覆盖 unknown"` / `"coverage unknown"`，带 `wiki.coverageHint`，键**不省略**）。
* ⇒ **「面板还把名次分当相似度显示」这一条：不成立**（改前 `m 0.86` 裸值已换成带量纲的两腿标签）。

### 1.3 我复核到的**真的合上了**的部分（要记名，不然 pass/fail 会被读成一片灰）

| 项 | 我的行号级读数 |
| --- | --- |
| **遥测单源** | `INSERT INTO recall_log` 全仓**只出现一次**：`crates/daemon/src/api.rs:3196`（§3.1「写入点唯一」成立） |
| **两条注入路径共用契约** | daemon 侧 `chat.rs:597 knowledge_items_enriched(…)` + `:613 render_context(&items, &InjectionBudget::default())`；`runs.rs:1921 knowledge_items_enriched(…)` + `:1938 render_context(&items, &InjectionBudget::default())` ⇒ **同两个冻结入口/同一个预算**，不存在第三套自写渲染 |
| **MCP 闭集 14 个** | 5 个新工具都在：`crates/mcp/src/lib.rs:301 memory_forget_report` · `:324 graph_search` · `:348 graph_retrieve` · `:378 wiki_pages` · `:397 wiki_links`（t19 报告 §9-1 曾记「未交付」，**HEAD 上已补齐**） |
| **H-2 的 `scoring` 块** | `api.rs:3169 "leg_window"` + `:3170 "fusion"`（+ `scoring_version`，见 §3.1 写入侧）已在 `/recall` 响应里 |
| **§1.5 的六条 graph 路由** | `api.rs` 路由表 + `wiki_corrections`/`graph/*` 端点齐（t51 已逐字段读数；本轮我按 t20 的 A-14①–⑦ 读数采信并抽查路由） |
| **面板不再混量纲** | §1.2 |

---

## 2 契约六节逐条判决

| 节 | 判决 | 依据（我的读数） |
| --- | --- | --- |
| **§1 HTTP 路由表** | **未达成**（1 条 medium） | §1.5 L99 原文要求 `GET /graph/entity/{id}/facts?at=` **同样要归一化输入**；实测 `api.rs:1283 Some(at) => ruagent_graph::facts_as_of(db, id, at).await?` **直传** ⇒ 非时刻字符串静默 200（对照 `/graph/retrieve` 走 `parse_ts` ⇒ 400）⇒ **INT-F2** |
| **§2 MCP 工具表** | **达成** | 14 个闭集齐（5 个新工具行号见 §1.3）；`memory_write{store:"bogus"}` ⇒ 工具错误由 `test -p ruagent-mcp --test roundtrip` 覆盖（在我 exit=0 的 `--workspace` 面内） |
| **§3 遥测字段表** | **未达成**（2 条 medium） | 写入侧成立（`recall_log` 11→23 列填真值，§3.1 的「列在 ≠ 值在」t19 逐列读数 + t20 复核）；但 §3.1 **L178「`/api/v1/recall/log` 的读出必须逐列透出」未做到**（`api.rs:3369` 只 SELECT 11 列）⇒ **INT-F1**；§3.2 的 `budget` **恒 null**（t20: 0/5）⇒ §6 的 A-2/N-6 只完成一半 ⇒ **INT-F6**；`<graph>` 块无生产者 ⇒ **INT-F5** |
| **§4 面板面** | **达成** | 我自己 `npm run build` exit 0（55 assets）+ 两处文案行号级读数（§1.2）；四页 e2e 的我**没有重跑**（需浏览器；t51 报 4 passed，我标注为引用） |
| **§5 冲突裁决** | **达成** | H-2 的 `scoring` 块在（§1.3）；越界接口按 §5.3「区域提供库接口、INT 只接线」处置（`runs.rs`/`chat.rs` 调的是 mem-core 的冻结 API）；F-5（`ScoreKind` serde 漂移）已并入 recall 的 t29、跨 crate 钉按契约顺序等它落地——**契约自己写死的顺序**，不算 INT 的缺口 |
| **§6 验收锚点清单** | **未达成**（3 条锚点不可达/未满足） | A-11 ✗（`/distill` 无 `prompt_hash`、`/stats` 无 `embedder`/`scoring_version`）⇒ **INT-F3/INT-F4**；A-2 的预算半边与 N-6 ✗（`budget` 恒 null）⇒ **INT-F6**；A-14⑧ 只在一半入口成立 ⇒ **INT-F2**。A-1/A-3a/A-3b/A-4/A-5/A-7/A-13/A-14①–⑦ 由我的 `--workspace` exit=0 + 行号读数支撑 |

---

## 3 findings

### INT-F1（medium · 契约 §3.1 L178「读出必须逐列透出」未达成）
* **事实（我的读数）**：`GET /api/v1/recall/log`（`api.rs:3352 fn recall_log`）的 SQL 是 `SELECT ts, query, strategy, top_n, memories, knowledge, wiki, entities, …`（`:3369-3371`，共 11 列 + `source`），JSON 里**没有** `score_kind`/`fusion`/`top_legs_json`/`candidates_json`/`selected_json`/`rejected_json`/`graph_entities`/`graph_paths`/`knowledge_leg_window`/`scoring_version`/`top_knowledge_relevance`。写作侧 23 列都填了真值（t19/t20 读数），**读出面一条都没有**。
* **后果**：契约 §3.3 那 7 个问题（哪条腿找到的/分页是否是函数/丢弃规则/图是否参与/跨版本可比）从 HTTP 面**答不了**；面板与 MCP 只能靠别的端点补（MCP 也没有 recall_log 工具）⇒「列在、值在、读出不在」。
* **requiredFix**：把 §3.1 的 14 个新列（或至少 §3.3 需要的 10 个）逐列透出到 `/api/v1/recall/log`，并补一条锚点测试断言键集（缺键即红）。

### INT-F2（medium · 契约 §1.5 L99 明文要求的归一化只做了一半）
* **事实（我的读数）**：`/api/v1/graph/retrieve` 的 `as_of` 走 `parse_ts` ⇒ 非时刻 **400**、三写法逐字节一致（t20 已独立取读数，len 474 / sha `759cf2372f5bc019`）；但 `GET /api/v1/graph/entity/{id}/facts?at=` 在 `api.rs:1283` **把字符串直传** `facts_as_of` ⇒ 非时刻**静默 200** 且结果集随字符串变化（t20 F2 实测）。
* **后果**：同一个 `as_of`/`at` 语义在两个入口两套行为；契约同一行点名的「同样要归一化输入，并按 t27 的三种写法给一致性读数」未做到。
* **requiredFix**：`facts` 入口复用同一 `parse_ts` 归一化 + 400；补「三写法一致」读数（与 retrieve 共用一条测试或两条同形断言）。

### INT-F3（medium · 锚点 A-11 / F-6 / K-11 未达成）
* **事实（我的读数）**：`GET /api/v1/distill`（`api.rs:2632 distill_policy_get`）返回 `{…, "prompt": p.prompt, "builtin_prompt": …}`，**没有 `prompt_hash`**（另两处 `prompt` 出现在 `:2664 distill_policy_put` 的入参与默认值里）。契约 A-11 的判据是「`/distill` 有 `prompt_hash`；一次 `PUT /distill` 后 `prompt_hash` **必须变**」。
* **requiredFix**：`prompt_hash`（内容哈希）进 GET/PUT 响应；补「PUT 后哈希变化」的断言（改前无键 ⇒ 用例必须在改前红）。

### INT-F4（medium · 锚点 A-11 / F-8 未达成）
* **事实（我的读数）**：`api.rs:403-405 async fn stats` 的响应体只有 `{"agents": stats}`，**没有 `embedder`、没有 `scoring_version`**（`:571` 的 `embedder` 属另一个 handler，`:2725` 亦然）。契约 A-11 判据：「`/stats` 同时给 `embedder` 与当前 `scoring_version`」。
* **requiredFix**：`/stats` 加两项（`scoring_version` 的取值必须与 `recall_log.scoring_version` 同源，不许第二套）。

### INT-F5（medium · 闭环的 `<graph>` 块**没有构造点**，G10 不是「未测」而是不可达）
* **事实（我的读数）**：`TAG_GRAPH` 在**生产代码里只出现一次**：`crates/memory/src/inject.rs:186`（定义）与 `:331`（rank=3），其余全是测试（`:1059/:1060/:1076/:1081`）。注入项的 tag 生产者侧，daemon 唯一的 tag 取值是 `memembed.rs:300 .unwrap_or(TAG_RELEVANT_MEMORIES)`；`runs.rs:1921` / `chat.rs:597` 只 extend `knowledge_items_enriched`。⇒ **没有任何代码把图证据变成 `ContextItem{tag: "graph"}`**，所以 t20 的 5 次注入里 `graph` 块 **0/5** 不是采样偶然。
* **后果**：任务书要我判的「闭环是否真的合上（知识/wiki/**实体块**进入注入事件）」——**知识 5/5、wiki 5/5、实体/图 0/5** ⇒ 闭环只合上了两条腿。契约 §3.2 的词表列了 `graph`、A-13⑤ 钉了 rank=3，却没有生产者；§6.3 只允许「A-2 未跑到含 graph 的路径 ⇒ not_measured」，而这里是**路径不存在**。
* **requiredFix（二选一，都要落纸）**：(i) 接一条生产者（graph 证据 → `ContextItem{tag: TAG_GRAPH}`，走同一 `render_context`/预算），并把 G10 的读数重取；或 (ii) 在契约里把 `<graph>` 块显式登记为 **deferred（本代无构造点）+ owner**，并把 §6.3 那一行从「未跑到 ⇒ not_measured」改成「**无构造点 ⇒ deferred**」（不许停在 not_measured，否则下一位会以为只差一次运行）。

### INT-F6（medium · 锚点 A-2 的预算半边与 N-6 **不可达**）
* **事实**：t20 我采信的独立读数是 `context_injected` 5 条：`path` 5/5、`budget` **0/5**（恒 null）；t19 报告 §9-3 把 `budget` 记为 `not_measured`，理由是 `render_context` 只返回字符串（`crates/memory/src/inject.rs:588`）⇒ 采集需要跨区接口。契约 §3.2 确实写了「`null` = 本次未采集」，但 §6.1 A-2 要求「含 `budget` 的事件数 ÷ 总数、`used_chars ≤ 4096`、`blocks[]` 的 tag 集合与渲染一致、造一次预算绑定 ⇒ `dropped_items>0`」，§6.2 N-6 也要求「有丢块 ⇒ 事件里 `dropped_items>0`」。⇒ 这两条锚点今天**不可能判**（不是「未跑到」，是数据从不产生）。
* **requiredFix**：让渲染侧回一份可序列化的报告（`render_context_report(...) -> (String, BudgetReport)` 之类，mem-core 的冻结面内加一层、不改 `render_context` 字节），调用方把它写进事件；并在契约 §3.2 把「允许恒 null」与 A-2/N-6 的关系写清（二者今天互相矛盾）。

### INT-F7（low-medium · 纠错端点对不存在的 slug 不报错并落盘）
* **我的读数**：`api.rs:1128 async fn wiki_add_correction` 在 `:1130 Path(slug)` 之后**没有 slug 存在性检查**，走到 `:1152 Ok(id) => …` 直接写。t20 的 F6 已实测「不存在的 slug ⇒ 不报错并落盘」，与 t51 声称的「400 点名未知名」不符；契约 §1.4 的纠错面要求「拒收不落盘」（N-3 只覆盖空 `reason`）。
* **requiredFix**：写入前查 `wiki_pages`/页文件；不存在 ⇒ 404（或 400 并在体里点名 slug）；补负例测试（不存在的 slug ⇒ 无新行）。

### INT-F8（low · A-12 的字面条件未满足）
* **我的读数**：`crates/daemon/tests/knowledge_api.rs:132/153/630/685/722` 调 `GET /api/v1/recall`，而该文件里**没有**「本测试会写 `recall_log`」的注明（对照 `injection_e2e.rs:22/285` 明确写着「never on /api/v1/recall」）。A-12 的判据是「只允许出现在明确写着会写 `recall_log` 的测试里」。
* **为什么只是 low**：这些测试跑在临时 root（`start_test_daemon()`），**没有碰活库**；问题在**文献面**——下一位无法从文件本身判断这是允许的写还是越界的取证。
* **requiredFix**：在测试文件头（或每条调用上一行）补注明「本测试在临时 root 上写 recall_log」，或改成断言行数变化的显式形状。

### 观察（不进 findings）
* **O-1** V-INT 的 **F7（契约两处键名不一致：`POST /graph/entity` 单数、`degrees` 是对象数组）我认同为文档级 low**，且契约 §1.5 的「读法注（t53）」已经把它写清；未独立复核。
* **O-2** V-INT 的 **F8（面板 DOM 读数依赖根的数据形状）** 是**读数的条件**不是实现缺陷；我自己的面板读数是**源码 + i18n 字面量 + build 门**，不依赖根数据。
* **O-3** A-8 的「只允许两个文件」判据在**并发波次**里已不适用：我自己的 `git status --porcelain` 在 `crates/daemon/src/api.rs crates/daemon/tests crates/mcp panel` 上给 10 项（`api.rs`/`injection_e2e.rs` = INT 自己声明的 R-8c/R-8e 与本代 INT 波；`knowledge_api.rs`/`mcp/lib.rs`/`panel/**` = t50/t51/t56 波；两个 `??` = `event_compat.rs`/`consumption.e2e`）。按 captain 的 F5 纪律，整目录 porcelain **不是越界证据**，我不据此判 INT。
* **O-4** t19 报告**自己**把 MCP/面板/`as_of` 三写法读数记为「未交付/未取」（§9-1/§9-2/§9-10），这是**正确的诚实**；HEAD 上 MCP 与面板已由后续波次补齐 —— 我按 **HEAD 状态**判六节，并在表里注明「由哪个波次交付」。同理 t19 的 §13.3 已登记 `budget`，但契约 §6 的 A-2/N-6 仍不可判（F6）。

---

## 4 判据与纪律回执

* **判据来源**：只取契约 §1–§6 的原文行号（§1.5 L99、§3.1 L178、§3.2 L196-204、§3.3 L210-232、§6.1 A-2/A-11/A-14、§6.2 N-3/N-6、§6.3），**未引入契约外的新要求**；每条 finding 都指回契约行号。
* **动手复核**：`test --workspace` 我自己跑（EXIT=0）；面板我**自己**跑 build + 逐字读两处文案；六条 finding 全部有 `api.rs`/`inject.rs`/`mempembed` 行号级读数，未依赖作者或 V-INT 的转述。
* **未测/不判**：四页 e2e 我**没有重跑**（需浏览器，成本与风险都高）⇒ 标「引用 t51 的 4 passed，未独立复核」；`--ignored` 的活库形状仪器**未跑**；真库/真 pid 全程未碰。
* **并发纪律**：全仓 porcelain 在并发波次里不作越界证据（O-3）；我只在读数受影响时报坐标，不替同伴改。
* **范围**：本文件是唯一写入（inScope）；`crates/`、`panel/`、契约/实现/验证三份报告**一行未改**。
