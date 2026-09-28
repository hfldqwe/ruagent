# gen3 规格 vs 实现 漂移审计（t80 · 只读）

**对象**：`gen2-recall-spec.md` / `gen2-memory-spec.md` / `gen2-graph-spec.md` / `gen2-wiki-spec.md` / `gen2-integration-contract.md` 里**可读数**的冻结断言，在**当前工作树字节**上逐条取一次读数。
**做法**：① 先做一次机械扫描 —— 把五份文档里**全部 `crates/….rs:NNN` 冻结坐标**抽出来（80 处），逐条检查「该坐标当前行是否还含引用句里的锚词」，得到 55 个候选；② 对高价值候选做**语义复核**（该断言今天是否还成立，新坐标在哪）；③ 抽查 D 节冻结接口 / 附录勘误 / 契约 §1–§6 锚点的能力面（表、列、路由、MCP 工具、响应键名）；④ 逐条问 §6.0 的 22 条纪律「今天还有没有守卫」。
**纪律**：**只读**（本报告是唯一写入；四份规格 + 契约 + `crates/` + `panel/` 一行未改）。**未写活库**、**未启停 pid 79984**、**未用 `/api/v1/recall` 取证**。时间窗 2026-09-29T0x:xx+08:00。

---

## 0 一句话

**冻结断言本身大多还成立，坏掉的是它们指向的坐标**：80 处 `file:line` 引用里，我机械扫出 55 处「当前行已不含引用句锚词」，逐条语义复核确认至少 8 处**真漂移**（最远的一处偏了 **+1699 行**）。这类漂移不会让测试变红，只会让**下一位按坐标去核的人读到无关代码**并据此下结论 —— 正是本单要防的「照着它错下去」。另有一条**「计划被读成交付声明」**（t59 的 F6 —— **归因已于 §6 R-1 更正：那是 captain 的待办清单，不是交付声明；t59 = failed 且它自己把 F6 列在未交付，现已并入 t83**）和一条**语义断言过期**（graph 规格的「未知 tag ⇒ 5」，今天是 6）。

---

## 1 findings（按严重度排序；每条：规格位置 · 断言原文 · 当前读数 · 判定 · 可证伪修复判据 · 建议 owner）

### D-1（high）纠错端点对未知 slug 仍落盘（**判定不变**；归因已更正 → §6 R-1）
* **规格/契约位置**：`gen2-integration-contract.md:74`（§1.4 wiki 表）与 §6.2 **N-3**（拒绝面）+ 我 t21 的 **INT-F7**（已派给 t57/t59）。
* **断言原文（派单口径）**：captain 2026-09-29 落点清单：「**t59**（… + **F6** 纠错端点未知 slug 拒绝 + F7 §1.5 读法注）」。**归因更正（§6 R-1）**：那份清单是**待办/计划**，不是交付声明；权威来源是**任务状态 + 该任务自己的报告**（t59 = failed / 部分交付，它自己把 F6 列在「未交付」），F6 现归 **t83**。
* **当前读数（我的，行号级）**：`crates/daemon/src/api.rs:1128-1162 wiki_add_correction` **只校验 `kind`**（`:1133 CorrectionKind::parse`，未知 kind ⇒ 400）；`:1151` 直接 `crate::wiki::add_correction(db, &c)`。`crates/daemon/src/wiki.rs:1220-1242 add_correction` **只 bail 空 reason（`:1222`）与空 author（`:1225`）**，随后 `:1240 INSERT INTO wiki_corrections (slug, kind, reason, author, at)` —— **没有任何 slug 存在性检查**（`wiki_corrections` 也没有指向页的 FK）。⇒ 未知 slug 仍**落盘**（与 t20 的 F6 实测一致）。
* **判定**：**不再成立 / 未落地**（~~派单声称已交付~~ ⇒ **归因更正见 §6 R-1**：t59 自己的报告与任务状态都把 F6 列为**未交付**，字节与「未交付」一致；**现状判定不变**）。
* **可证伪修复判据**：`POST /api/v1/knowledge/wiki/pages/<不存在的 slug>/corrections` body `{"kind":"note","reason":"x","author":"human"}` ⇒ **404 或 400**，且 `SELECT COUNT(*) FROM wiki_corrections` **前后不变**；改前该用例必须红。
* **建议 owner**：**integ（t59 的属主）**；请 captain 核「是在途未落盘，还是报告与盘不一致」。

### D-2（medium）冻结坐标大面积漂移（80 处引用，55 处可疑，至少 8 处已确认）
* **规格位置**：五份文档**全部** `crates/….rs:NNN` 引用（共 80 处；分布：契约 40 · memory 18 · recall 15 · wiki 5 · graph 2）。
* **断言原文（举三条我复核过的）**：
  * `gen2-recall-spec.md:77`「`crates/knowledge/src/store.rs:610` 与 `:659`：`let leg_k = limit.max(10) as usize;`」→ **当前 `store.rs:610` 是空行**；该字面量现在只活在 `store.rs:160` 的注释（实测基线）里，真值是 `store.rs:165 pub const LEG_WINDOW: usize = 60;`
  * `gen2-recall-spec.md:404`「`crates/daemon/src/wiki.rs:1482`（`recall_stubs(hits: &[SearchHit], ..)`）」→ **`recall_stubs` 现在在 `wiki.rs:3181`**（偏 **+1699 行**）
  * `gen2-recall-spec.md:405`「`crates/daemon/src/memembed.rs:310`（`ruagent_knowledge::rrf(&[sem_ids, kw_ids], RRF_K)`）」→ **现在在 `memembed.rs:581`**（偏 +271）
  * 另有我已复核的：`api.rs:3003`（`"score_kind": "rrf_rank"`）→ **现在 `:2973`/`:2985`**；`api.rs:2466`（`recall` 处理器）→ **`:2889` 是新的 `resolve_seeds` 调用**；`crates/core/src/event.rs:121`（`ContextInjected` 形状）→ **现在 `:133`**；`crates/daemon/tests/knowledge_api.rs:478`（`links_out` 逐字段钉住）→ **现在 `:480`**；`runs.rs:1270`（`budget: None`）→ **现在 `:1278`**（`acp/chat.rs:497` → `:505`，daemon `chat.rs:613` → `:627`）。
* **判定**：**坐标层不再成立**（断言文本多数仍成立，位置全变）。这是本审计里**量最大**的漂移类。
* **可证伪修复判据**：写一条可脚本化的检查（我用的形状）：对每条引用取引用句里的锚词（标识符/字面量），断言「当前该行含任一锚词」，否则红；修法 = 更新坐标**或**把坐标换成符号名（`store.rs LEG_WINDOW`），并在勘误里注明。修完后该脚本 0 命中。
* **建议 owner**：**各规格属主（recall / memory / graph / wiki / integ 各改自己那一份）**；这条只改文档，不动代码。

### D-3（medium）`gen2-graph-spec.md:165` 的标定读数「未知 tag ⇒ 5」已过期（今天是 6）
* **规格位置**：`gen2-graph-spec.md:165`。
* **断言原文**：「**标定读数**：`tag_rank` 对**未知** tag 返回 **5**（`crates/memory/src/inject.rs:268`）⇒ 不把 `graph` 加进词表的话，图块会被**第一个丢弃**」。
* **当前读数**：`crates/memory/src/inject.rs:334 _ => 6`（`tag_rank` 的兜底分支）；`gen2-integration-contract.md:203` 与我在 RV-B r2 的独立读数（`profile=0, memories=1, knowledge=2, graph=3, wiki=4, project=5, unknown=6`）都是 **6**。引用坐标 `inject.rs:268` 现在也不是该分支。
* **判定**：**不再成立**（数值过期 + 坐标漂移）。结论方向（未登记 tag 会被**最后**丢弃）不变，但数值会让读者算错排序。
* **可证伪修复判据**：规格该行改为 **6** 并附断言 `tag_rank("not-a-tag") == 6`（或直接引 `inject.rs` 的行号与函数名）；改前若有人按 5 推断 `graph` 的位置就会与实测不符。
* **建议 owner**：**graph（mem-core 复核）**。

### D-4（medium）`gen2-graph-spec.md:154/157` 的 G9 判据落后于迁移 0024（`distill_log` 已重建为「一行=一次尝试」）
* **规格位置**：`gen2-graph-spec.md:154`（metric）与 `:157`（复现命令）。
* **断言原文**：metric =「同一窗口内 `distill_log` 行数 ÷ `daemon.log` 里的 distill 尝试数」；复现 =「`SELECT status,COUNT(*) FROM distill_log GROUP BY 1`」。
* **当前读数**：仓库里**已有 25 个迁移**（我实测 `crates/store/src/migrations/` 25 个文件，最大 `0025_query_eval_gold_unique.sql`），其中 **`0024_distill_attempts.sql`** 的语句是 `CREATE TABLE distill_log_attempts (…)` → `ALTER TABLE distill_log_attempts RENAME TO distill_log` → `CREATE INDEX idx_distill_log_session ON distill_log(session_key, id)` → `CREATE INDEX idx_distill_log_status …` ⇒ **`distill_log` 现在是「一次尝试一行」**，不再是「一个 session 一行」。规格与「行数 < 尝试数」的旧限制说明因此对形状的描述**落后一代**；契约 §3.1 的落地表也止于 `0023`、没有 0024 的形状变更行。
* **判定**：**不再成立（形状/判据口径过期）**。这是任务点 1 要的「二次漂移」：勘误本身没跟上后来落地的 0024。
* **可证伪修复判据**：规格 §G9 与契约 §3.1 补一行「0024 起 `distill_log` 一行=一次尝试（`idx_distill_log_session(session_key,id)`）」，并给「temp root 上 `attempts == rows`」读数（我 t32/t28 实测过 `attempts=4 rows=4 rows-attempts=0`，可作为既有读数引用）。
* **建议 owner**：**graph + I-SCHEMA（t6 的迁移属主）**。

### D-5（medium）§6.0 第 11 条的守卫形状只覆盖了一部分：仍有 **7 处裸 `#[ignore]`**
* **规格位置**：`gen2-integration-contract.md:349-353`（第 11 条：**要求 `#[ignore = "…needs <ENV>…"]` + 体首 `expect`**，「不存在一个不测量也能通过的状态」）。
* **当前读数（我实测）**：全 `crates/**` 里 `#[ignore = "` 形状 **16 处**（合格形态），**裸 `#[ignore]` 7 处**：`crates/daemon/tests/injection_e2e.rs:96` · `crates/graph/tests/live-after.rs:3` · `crates/knowledge/tests/retrieval-gold-copy.rs:34` · `crates/knowledge/tests/retrieval-gold-live.rs:26` · `crates/store/src/lib.rs:1281` · `crates/store/src/migrations.rs:766` · `crates/store/src/migrations.rs:894`。
* **判定**：**部分不成立** —— 这 7 处在「显式 `--ignored` 运行但缺 env」时**可能静默通过**（正是第 11 条要消灭的状态）。
* **可证伪修复判据**：把 7 处改成带原因的 `#[ignore = "needs <ENV>"]` + 体首 `expect/env_or_skip_that_fails`；判据 = 显式 `--ignored` 且缺 env 时**必须 FAILED**（不是 `ok. 0 passed … filtered out`）。
* **建议 owner**：各文件属主（daemon / graph / knowledge / store）。

### D-6（medium）契约 §6.3 的「`<graph>` ⇒ not_measured」将被 t63 作废（**在飞**，请随单改规格）
* **规格位置**：`gen2-integration-contract.md:480`（§6.3 第 2 行）与 `:203`（§3.2 tag 词表含 `graph`）、§6.1 **A-13⑤**（`TAG_GRAPH` rank=3）。
* **断言原文**：「`<graph>` 在生产 transcript 里出现（G10）｜需要一次真实注入运行｜若 A-2 未跑到含 `graph` 的路径 ⇒ `not_measured`」。
* **当前读数**：**t63 在飞**（captain 已给读数：`context_injected` 5 事件 ⇒ `with_graph=3`、负控干净）。我 t21 的独立读数（当时）是：`TAG_GRAPH` 在生产代码里**只在** `crates/memory/src/inject.rs:186`（定义）与 `:331`（rank）出现，生产者侧唯一 tag 默认是 `crates/daemon/src/memembed.rs:300 .unwrap_or(TAG_RELEVANT_MEMORIES)` ⇒ 当时**无构造点**。
* **判定**：**今天仍成立**（块尚未可达），但 **t63 落地即作废**：应改为「块可达性已闭合 + `GRAPH_PATHS` 取值依据 = 下一代项 + owner」，并**不许**把未达成写成 `not_measured`。
* **可证伪修复判据**：t63 落地后，契约 §6.3 该行删除/改写为 deferred + owner；判据 = 「有构造点」的读数（`with_graph>0` + 负控 → 0）。
* **建议 owner**：**integ（随 t63 改契约）**。

### D-7（low）`gen2-graph-spec.md:57`（A17）「生产召回只用了 strict 腿」列为基线，但今天已被 DEP-1 改变且坐标漂移
* **规格位置**：`gen2-graph-spec.md:57`（A 节基线表 A17）。
* **断言原文**：「生产召回只用了 strict 腿（代码判据）｜`crates/daemon/src/api.rs:2524`｜`Select-String … -Pattern "search_entities"`」。
* **当前读数**：召回路径现在是 `api.rs:2889 ruagent_graph::resolve_seeds(state.mgr.db(), &q.q, top_n)`；`api.rs:2524` 已不含该调用（该行的 `search_entities` 命中在 `:803`/`:811`，属另一处理器；`:5397` 是测试）。⇒ 基线行**作为历史**仍对，作为**现状**已错。
* **判定**：**不再成立（现状层）/ 基线层需加时间戳标记**。
* **可证伪修复判据**：该行加「**已被 DEP-1（t19）改变：生产召回走 `resolve_seeds`**」并更新坐标；判据 = 按该行去 grep 能命中召回处理器的调用点。
* **建议 owner**：**graph**。

### D-8（low）`gen2-memory-spec.md:1262` 点名的 doc 注释**今天仍在**（未落地，不是漂移）
* **规格位置**：`gen2-memory-spec.md:1248/1262`。
* **断言原文**：「请求结构体 doc 注释把**语法**词表读成**可写**词表｜`crates/daemon/src/api.rs:4234`「/// user \| global \| project:<x> \| agent:<x>」｜改为指向 `MemoryStore::allowed_kinds` 渲染出的矩阵（或至少写明「可写性取决…」）」。
* **当前读数**：`api.rs:4278 /// user | global | project:<x> | agent:<x>` **仍在**（坐标已漂 +44 行；`:4307` 的错误消息里也仍是同一串）。⇒ 这条 finding **未修**。
* **判定**：**仍成立**（未修）—— 列在这里是因为它已被 t59（命名空间矩阵）触及过一轮，读者容易以为一起修了。
* **可证伪修复判据**：注释改为引用 `allowed_kinds` 的矩阵或写明可写性；判据 = `Select-String 'user \| global \| project:<x>'` 在请求结构体注释处 0 命中。
* **建议 owner**：**integ / mem-core（t59 的属主）**。

### D-9（low）契约 §3.2 的 `budget` 与 §6.1/§6.2 的自相矛盾（t60 在飞）
* **规格位置**：`gen2-integration-contract.md:198`（§3.2 `budget` 可为 `null` = 未采集）与 `:449`（§6.1 A-2 要求 budget 读数）、`:473`（§6.2 N-6）。
* **当前读数（我实测）**：`crates/daemon/src/runs.rs:1278 budget: None` 与 `crates/daemon/src/chat.rs:505 budget: None`（两条注入路径都仍是 None；无 `BudgetReport`/`render_context_report` 符号）⇒ **t60 尚未落地**，且记忆/图规格里以 `budget: None` 为**现状**的表行**今天仍准确**（`gen2-memory-spec.md:1356/1357`）。
* **判定**：**成立**（两份规格的现状描述都对），但契约内部**两条判据互相矛盾**（§3.2 允许恒 null vs §6.1/§6.2 要求预算读数）—— 这是**规格自身的缺陷**，t60 落地时应同时修掉。
* **可证伪修复判据**：t60 把「采集到 budget」写成契约的**硬要求**（或把 A-2/N-6 的预算半边显式 deferred + 具名 owner），并给「`budget` 非 null 的事件数 > 0 + 一次预算绑定 ⇒ `dropped_items>0`」读数。
* **建议 owner**：**mem-core（t60）+ integ（改契约文字）**。

### D-10（low）§6.0 第 12/13/14/15②/17/18/21/22 条**只活在文档里**（无脚本/CI 守卫）
* **规格位置**：`gen2-integration-contract.md:355-404`（第 12–21 条）与 `:712`（R-24a 新增第 22 条）。
* **当前读数（我核到的守卫面）**：`.github/workflows/ci.yml` 跑 `cargo fmt --all --check` / `clippy --workspace --all-targets -- -D warnings` / `test`（且文件头注释里就有「空跑」守卫），`e2e.yml` 覆盖面板与 e2e，`.github/workflows/scripts/{check-workflow-refs.sh,test-evidence.sh}` 存在；`scripts/cargo-team.ps1` 提供 `-CleanFirst/-DenyWarnings/-DryRun/-TargetDir/-NoLock`（第 5/6/7/10 条的**工具化**守卫）。⇒ 第 1–11、16、19、20 条**有对应守卫**（CI / 脚本）；**第 12、13、14、15②、17、18、21、22 条没有自动守卫**（全靠人与报告纪律；第 14 条本代只靠 RV-C r3 的一次正例示范）。
* **判定**：**无守卫（登记，不当成已执行）**。
* **可证伪修复判据**：能给守卫的给守卫（例：第 17 条 → CI 里禁用 `Select-String 'FAILED'` 形状，或提供 `-CaseSensitive` 包装；第 21 条 → 报告模板加「该文件是否被 git 跟踪」一格）；给不出的写明「本代只能靠评审抽检」。
* **建议 owner**：**captain（流程）+ CI 属主**。

### D-11（low，前瞻）在飞修复单会让哪些规格文字作废（供决定是否随单改规格）
| 在飞/已落 | 会作废的规格文字 | 今天的状态（我实测） |
| --- | --- | --- |
| **t59（已落 F1）** | 契约 §3.1 L178「读出必须逐列透出」（这条**已由实现满足**，规格无需改） | 见 §2「已核对成立」表 |
| **t60（未落）** | 契约 §3.2 的「`budget` 可恒 null」+ memory-spec:1356/1357 的现状行 | `runs.rs:1278`/`chat.rs:505` 仍 `budget: None` |
| **t63（未落）** | 契约 §6.3 的 `<graph> ⇒ not_measured` | 生产仍无 `TAG_GRAPH` 构造点 |
| **t64/t72–t77（未落）** | 会改 `prompt_hash`/`/stats`/面板命名空间等，**其对应规格行（recall-spec H-1/H-2、memory-spec §9、契约 A-11）落地后需要同步** | `api.rs:403 stats` 仍只回 `{"agents"}`；`prompt_hash` 在 `api.rs` **0 命中**（我实测） |
* **判定**：**前瞻登记**（不是当前漂移）。建议：**规格随单改**由该单属主负责，并在报告里点明「本条作废了哪一行」。

---

## 2 已核对**成立**的冻结断言（同样的读数纪律，写下来免得下一轮重做）

| # | 规格位置 | 断言 | 当前读数 |
| --- | --- | --- | --- |
| P-1 | `gen2-recall-spec.md` BREAK-REC-1 / 契约 §1.6 L110 | 腿窗口 = `LEG_WINDOW=60`，`limit.max(10)` 退役 | `crates/knowledge/src/store.rs:165 pub const LEG_WINDOW: usize = 60;`；旧式 `limit.max(10)` 只剩 `:160` 的实测基线注释 ⇒ **成立** |
| P-2 | 契约 §3.1 L178「读出必须逐列透出」 | `/recall/log` 逐列透出 | `api.rs:3379` 起的 SELECT 已含 24 列（含 `CAST(score_kind…)`/`CAST(top_legs_json…)` 等）⇒ **列数成立（t59 落地）**；~~响应键 `"rows"` 在 `api.rs:3455`~~ ⇒ **本行后半句已被 §6 R-2 更正**：`:3455` 的 `"rows"` 属**另一个计数响应**（同响应含 `max_rows`/`policy`/`rows_without_source`），`/recall/log` 的真实键是 **`api.rs:3472 "log": rows`** ⇒ 键名与契约 §1.2 L52 的 `rows:[…]` **仍不一致（未收口，已并入 t83）** |
| P-3 | 契约 §2（14 个工具闭集） | MCP 恰好 14 个工具 | `crates/mcp/src/lib.rs` 里 14 个 `async fn` 工具名逐一对应（memory_search/write/recall/get · knowledge_search/ingest/expand · graph_entity/search/retrieve · wiki_pages/links · memory_forget_report · list_tasks）⇒ **成立**（注意：用 `#[tool` 数属性会得到 16，含 router/handler 属性，**不是**工具数） |
| P-4 | 契约 §1.5 L99 / A-14⑧ | `facts?at=` 必须归一化、非时刻报错 | `api.rs:1290 let dt = ruagent_graph::parse_ts(at)` → `:1292 facts_as_of(…, &dt.to_rfc3339())` ⇒ **成立（t57 落地）** |
| P-5 | `gen2-graph-spec.md` G7 / 我 RV-C r3 | 关系级 precision = 43/47 = 0.9149 | 我 RV-C r3 的独立运行：`written 47 / duplicate 11 / refused 2`、`43/47 = 0.9149`、本体 35→29 ⇒ **成立** |
| P-6 | 契约 §3.2 的 tag 词表 / A-13⑤ | `TAG_GRAPH` rank = 3 | `crates/memory/src/inject.rs:331 TAG_GRAPH => 3`（`:1081` 断言同值）⇒ **成立** |
| P-7 | 契约 BREAK-WIKI-1 / wiki-spec L311 | `links_out` 不含自链（现场 `cooking-pasta` 3→2） | `crates/daemon/tests/knowledge_api.rs:480 assert_eq!(a["links_out"], 2)` + `:426/:484` 的解释注释 ⇒ **成立**（我的引用坐标 `:478` 已漂到 `:480`，见 D-2） |

---

## 3 22 条纪律的守卫现状（逐条问「还有守卫吗」）

| 条 | 主题 | 今天的守卫 | 判 |
| --- | --- | --- | --- |
| 1–5 | 编译面覆盖 / 首个失败 target 停住 | **CI**（`.github/workflows/ci.yml`：fmt/clippy/test）+ 报告的「覆盖哪些 target」纪律 | 有（人 + CI 各半） |
| 6–10 | clippy 假绿（缓存命中 / 两条证据 / `-CleanFirst` 持锁窗口） | **工具**：`scripts/cargo-team.ps1` 的 `-CleanFirst` / `-DenyWarnings` / `-DryRun` / 持锁单窗口 | **有（工具化，本代最好的形态）** |
| 11 | 判据必须让漏测自己红（空跑绿 / `#[ignore]` 形状） | 部分：CI 文件头有「空跑」守卫；但 **7 处裸 `#[ignore]`** | **部分（D-5）** |
| 12 | 公开签名/形状变更先广播 | 无自动守卫（靠报告与消息） | 只活在文档 |
| 13 | 同形多处替换必须一次改完 | 无自动守卫（靠「改完立刻 check」） | 只活在文档 |
| 14 | 根因必须受控反做 | 无自动守卫（本代 1 次正例：RV-C r3） | 只活在文档 |
| 15 | 红也可能来自旧二进制 / 读别人的数据 / 临时 root 收尾 | 夹具有 Drop 清理，但 t42 实测 `Drop` 失败 + 725 个泄漏目录 ⇒ **无守卫** | 只活在文档 |
| 16 | inScope 跨 crate ⇒ 门禁覆盖两个 crate | **CI 的 `--workspace --all-targets`** 覆盖；单 crate 声明靠人 | 有（CI） |
| 17 | 量词工具默认行为（`Select-String` 大小写） | 无守卫 | 只活在文档 |
| 18 | 异步面先轮询终态 | 无守卫 | 只活在文档 |
| 19 | 门禁必须写明不覆盖什么 | 部分（`AGENTS.md` 与契约都写了；无断言） | 部分 |
| 20 | 判断命令做了什么要读脚本 | 无守卫（本代 t55 已更正过两次） | 只活在文档 |
| 21 | 「没有 diff」≠「没改」（先问是否被跟踪） | 无守卫 | 只活在文档 |
| 22 | 跨 target 的 `passed` 求和不是稳定读数 | 无守卫（R-24a 新增） | 只活在文档 |

---

## 4 未覆盖范围（如实写）

1. **坐标引用只做了 8 处语义复核**：80 处里 55 处是机械候选，我逐条语义确认了 8 处（D-2 列出的那些）+ 3 处证伪（P-1/P-4/P-7）；**其余候选未逐条复核**（工具给出的「锚词不匹配」在引用句跨行、写的是范围（`:4008-4058`）、或锚词是中文时会有假阳性 —— 我看到的候选里这类占多数）。
2. **C 节目标的标定读数未重取**：活库分布（`top_memory_score` 分桶、`recall_log` 651 行历史）、wiki 真库 `stale` 正例、社区质量三指标等 —— 本轮**只做静态核对**，没有起临时 root、没有读活库（只读纪律允许，但本单目标是规格 vs 实现，不重复 V-INT 的端到端）。
3. **`panel/` 未审**（t51/t59 的面板读数我未复跑 e2e）。
4. **第 12/13/14/15/17/18/20/21 条未逐条实测**「有没有人真的违反」，只判断了「有没有守卫」。
5. **在飞单的最终字节未核**：t60/t63/t64/t72–t77 我按下单前的 HEAD 读；它们落地后 D-6/D-9/D-11 需要重判。
6. **`0025_query_eval_gold_unique.sql` 的语句未逐行读**（只确认它存在、且不改 `recall_log` 形状）。

---

## 5 建议的处理顺序（给 captain 的排单参考）

1. **D-1** 先核（声称 vs 盘上字节不一致，会误导后续验收）→ 若在途未落盘，请在 t59 的报告上更正声称。
2. **D-4 / D-3**（会改变判据口径的语义漂移）→ 随 graph 下一单改规格。
3. **D-2**（坐标漂移，量最大但每条都便宜）→ 建议**一次性脚本化勘误**：五份规格各改自己那份，附我给的检查形状。
4. **D-5**（7 处裸 `#[ignore]`）→ 各属主顺手改。
5. **D-6 / D-9 / D-11**（随 t63/t60/t62 落地同步改规格文字）。
6. **D-10**（只活在文档的 8 条纪律）→ 建议给能自动化的两条（17/21）加守卫，其余在契约里写明「靠评审抽检」。

---

## 6 修订记录（追加式；只更正，不改写历史结论 —— 原文本节内逐字引用）

### R-1（2026-09-29，t80 之后 captain 复核）D-1 的**归因**更正：那份清单是「计划」，不是「交付声明」
* **原文（逐字引用，保留不改）**：
  * §0：「另有一条**交付声称与盘上字节不一致**（t59 的 F6）」
  * D-1 标题：「t59 声称的「纠错端点拒绝未知 slug」在盘上不成立」
  * D-1 判定：「**不再成立 / 未落地**（**派单声称已交付**，字节未变）」
  * §5-1：「D-1 先核（**声称 vs 盘上字节不一致**，会误导后续验收）→ 若在途未落盘，请在 t59 的报告上更正声称」
* **captain 复核（2026-09-29）**：他给的落点清单写的是**待办/计划**（「t59 将做 R-1..R-4 + F1 + F6 + F7」），**不是**「t59 已交付」。**权威来源 = 任务状态 + 该任务自己的报告**：**t59 = failed（部分交付）**，其 task output 明确把 **F6 列在「未交付」** 里（交付的只有 F1 的「只增列」那半：24 键、22 非 null）。
* **更正后的表述**：D-1 的 **判定不变**（未知 slug 仍落盘），但归因从「声称与字节不一致」改为「**我的前提把计划读成了交付声明**（captain 已认这是他表述造成的歧义，后续清单会显式标注计划/交付）」。
* **F6 的现状（captain 裁决）**：已并入 **t83（owner=integ）**，判据写成两半 —— ① `author` 空/缺 ⇒ 400 且不落盘（**已成立，不许回归**）；② **未知 slug ⇒ 拒绝且不落盘**（待修）；取证用「**非空 `author` + 0 页的根**」这个受控形状。我给的现状证据（`api.rs:1128-1162` 只校验 kind · `wiki.rs:1220-1242` 只 bail 空 `reason`/`author` · `wiki_corrections` 无指向页的 FK）已转给属主。

### R-2（2026-09-29，t80 之后 captain 复核 + 我自己的复读）§2 **P-2 的响应键**更正：键是 `log`，不是 `rows`
* **原文（逐字引用，保留不改）**：§2 P-2 「……响应键 `"rows"` 在 `api.rs:3455` ⇒ **成立（t59 落地）**」。
* **我自己的复核（当前字节，逐行）**：
  * `crates/daemon/src/api.rs:3472` ⇒ **`"log": rows`** —— 这才是 `GET /api/v1/recall/log` 的响应键；其上方 `:3465-3468` 的代码注释自己写着「The key stays `log` FOR NOW (t59/F1 handed back): the contract §1.2 row says `rows`, but consumers outside this unit's inScope read `log` … `Awaited<ReturnType<typeof api.recallLog>>["log"]`」。
  * `api.rs:3455` 的 `"rows": r.get::<_, i64>(0)?` 属**另一个响应**（同一对象里还有 `:3453 "policy"` · `:3454 "max_rows"` · `:3458 "rows_without_source"`）—— 我把两个响应认成了一个，**这是我的误读**。
  * 两个消费方印证：`crates/daemon/tests/knowledge_api.rs:737 = log["log"]`、`panel/src/views/Memory.tsx:82 = Awaited<ReturnType<typeof api.recallLog>>["log"]`。
  * 契约侧：§1.2 **L52** 的响应 schema 写的是 `{rows:[{ts,query,…}]}`，且契约**自己的 R-24c（L714）**已登记「与实现不一致的是**实现侧**（handler 的键名），修法归 F1」。
* **更正后的判定（P-2 → 部分成立）**：**列数成立**（`api.rs:3379` 起的 SELECT 确为 24 列，t59 的 F1 只增列那半落地）；**键名不一致仍未收口** —— 实现是 `log`、契约是 `rows`，归 **t83**（captain 裁决：**按契约改成 `rows`，且 `api.rs` + `crates/daemon/tests/**` + `panel/src/views/Memory.tsx:82` 三处同一次改**，因为只改 api.rs 必红 —— integ 上一轮实测 `TEST_EXIT=101`）。
* **教训（我方）**：「键在、值在」不等于「键名对」；机械核对行号时必须**确认那行属于哪个响应/函数**（我这次把计数端点与列表端点混了）。这与我 D-2 里主张的「坐标要能被脚本断言」同源：这次是我自己的坐标核对不够严。

### R-3（2026-09-29，登记 captain 点名的后续交付）
* **D-5 / D-10 的下一步**：captain 要我在**下一轮派单**里把「哪几条纪律**只活在文档**」列成**可派单的最小集合**，每条给一个**可执行的守卫形状**，由他决定给哪几条加机械门禁。按本轮读数，候选最小集合 = §3 表里判「只活在文档」的 **12（广播形状变更）· 13（同形多处替换一次改完）· 14（根因受控反做）· 15②（读别人的输入）· 17（量词工具默认行为）· 18（异步面先轮询终态）· 20（读命令真正执行的东西）· 21（未跟踪文件的 diff 不作证据）· 22（跨 target 求和）**；我已在 §3 里给 17/21 写出可机械化的形状（示例：17 → CI 里禁用裸 `Select-String 'FAILED'` 形状或提供大小写敏感包装；21 → 报告模板固定一格「该文件是否被 `git ls-files` 跟踪」），其余待派单时逐条补形状。
* **本轮其余 finding（D-2/D-3/D-4/D-5/D-10/D-11）**：captain 已全部收下并登记，无更正。
