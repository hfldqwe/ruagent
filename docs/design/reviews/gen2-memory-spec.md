# gen2 记忆规格（R-B / t2，mem-core）

- 任务：t2 R-B —— 记忆这一代的可执行规格（去重/合并判据、置信度校准、衰减与巩固、遗忘与可审计）
- attempt：`1b0036fb-bf50-4d9e-8562-71c1740322cf`
- owner：mem-core（`crates/memory` 含注入契约 `inject.rs`）
- 本单只写这一个文件：`docs/design/reviews/gen2-memory-spec.md`（无代码改动）
- 供：I-B（实现）· V-B（独立验证）· RV-B（独立评审）
- 现场读数时间窗：**2026-09-27T21:28:58 → 22:01:50 +08:00**；活库 `ruagent.db` mtime **2026-09-27T21:15:21.078 +08:00**（只读 `mode=ro`）

读数纪律（本规格自用）：每条读数都带「对象集 + 采样面 + 复现命令 + 时间窗」；旧值一律标注时间与来源；**分布类读数（标记覆盖率等）在数据落地前不当结论**（closure §7.140）；未取到的读数写进 A.12，不写成 0。禁止用 `GET /api/v1/recall` 取证（它会写 `recall_log`）。

复现装置：本文件自带 SQL 与两个探针的完整代码（附录 §A1/§A2），因此不依赖任何 `%TEMP%` 里的脚本。全部读数只用两种命令：`cargo test` 与 `python -c`（python 3，stdlib `sqlite3`）。

**修订记录（追加式：只标注来源，不静默改数、不删原文）**

- **R-1（captain 裁决，2026-09-27）**：`crates/daemon/src/memembed.rs` 归 **mem-core / I-B(t8)**（t8 的 inScope 已 amend 加入 `crates/daemon/src/memembed.rs` 与本文件；t19 的 inScope 已把 memembed.rs 移除 ⇒ 同一文件只留一个写者，closure §5）。`crates/daemon/src/api.rs` 保持 **INT / t19** 单写者。
- **R-2（captain 裁决，2026-09-27）**：本规格中**属于 api.rs 的条目从 C 节移出 → 新建 `## C-INT` 节**，owner=**INT/t19**，验收在 **t20/t21**。涉及两条：`memory_write` 未知 store 必须诚实 400；`distilled` 布尔按 episode kind 派生。**C 节原行保留在原位并标注指向 C-INT**（未删、未改数）。
- **R-3（captain 裁决，2026-09-27）**：**not_measured 规则批准**；V-B(t12) 的依赖为 **[t8, t9]**（I-C 落地后才跑）⇒ 规格里依赖 distill 路径的 C 目标**逐条标注 jointly-delivered + 责任半边**（mem-core 半边 / graph-I-C 半边），失败落在 distill 那半边时由 captain 路由给 graph/repair。
- **R-4（captain 裁决，2026-09-27）**：跨存储残余面清点**批准发消息**给 recall（t1）与 wiki（t4），注明「只读查询、只要求读数、不需要改文件」。
- **R-5**：A.10 里被本单读数推翻的 closure §6.1 前提（「chat 注入预算 700 字节」「0/54 有截断标记」）**保留在原位并标注复核结论**，见 A.10 的对照表 —— 该表就是本条规则的落地。
- **R-6（captain 裁决，2026-09-27）**：等待期的有界活 = 把 C5 的 `forget_report` **形状草案**写进本文件新增一节（`## C5-D`），标注 **non-landing / 待 I-SCHEMA 定表后再冻结**；**只写类型与语义，不写实现、不预填任何数字判据、不碰 `crates/`**；与 I-SCHEMA 表结构强耦合的部分**只以「待定项」列出**。recall/wiki 的回信到达后（它们会直接唤醒我），按 §7.130（预期/读数分开）收敛该形状，并**逐条引用它们给的读数 + 时间点**。
- **R-6 的收敛 B（2026-09-27，recall 第二次回信后；追加，前文不动）**：C5-D 的**形状升到 v2**（v1.5 保留在其上方）——`origin` 从 `&'static str` 变 `ResidualOrigin{Db,File,Both}`（取值集合与 I-A 逐字相同）、每个面的读数从裸 `usize` 变 `ResidualCount{hits,truncated,total:Option<usize>}`、新增 `LocalResidual`；新增**语义第四条**（`truncated` 判据必须是 `limit+1`；`total` 只在显式 `exact_total=true` 时才有值；**`None ≠ 0`**）；I-A 侧的最终签名（`residual_scan(needle, limit, exact_total) -> ResidualPage`）抄录在 v2 里仅作对照，本规格不依赖它；**代价读数**两列对照（recall 的 **10.47 ms** vs 我的 **2026-09-27T22:23:50+08:00** 复跑 median **6.80 ms**、`LIMIT 51` 6.69 ms）已记入 C5-D。漂移自检（字段名 + `ResidualOrigin` 取值集合）归 **t7 的测试**（I-A 的 D.4）；**我承诺不放宽取值集合、不改字段语义，除非先告诉 recall**。U-4（wiki）仍 `Unknown`（**此判断在写下时成立；wiki 回信后由下面的「收敛 C」取代，原文保留不删**）。
- **R-8（t8 实现后的接口状态，2026-09-27）**：本规格的 C-local 条目已在 `crates/memory/**` + `crates/daemon/src/memembed.rs` 落地，读数与命令见 **`docs/design/reviews/gen2-memory-impl.md`**。三处**接口事实**必须在这里登记（细节在实现报告 §3/§4）：
  1. **D.4 的标记位置改为行首前缀**（见该节的 D.4 修订块，原因：后缀会被 `per_block` 截断）；
  2. **跨区字段用适配形状**：`RetrievalHit` 未加字段（加字段会让 `chat.rs`/`runs.rs` 编译不过，而它们是 integ 的、本单不许动），改为 `EnrichedHit { hit, lead: Option<WikiLeadMeta>, relevance: Option<RelevanceMeta> }` + `knowledge_items_enriched()`；`knowledge_items()` 保留为薄包装，**既有调用点与字节零变化**。这是**上报 captain 的偏差**（§7.138），不是静默降级；
  3. 新增常量 `usage::DECAY_OVERFETCH=3`：注入每组读 `limit×3` 行后按 decay 排序再截断 —— 否则衰减只能重排窗口、不能改变窗口内容（C3 的行为变化，已登记）。
  另：`tag_rank` 值域变为 0..6（含 `graph=3`），`TAG_GRAPH`/`LOW_CONFIDENCE_MARK`/`WIKI_LEAD_HINT`/`EnrichedHit`/`WikiLeadMeta`/`RelevanceMeta` 为新增公开 API；`render_context`/`InjectionBudget`/截断词表/`RetrievalHit` **签名与字节未变**。
- **R-9（t24 / R-O1 修复：`WikiLeadMeta.edited` 由 `bool` 更正为 `Option<bool>`，2026-09-28）**：R-8 的第 2 条把跨区字段落成适配形状后，集成期发现一处**三态被二值字段掩盖**：生产端 `wiki::WikiLead.edited` 是 `Option<bool>`（`Some(true)` 手改 / `Some(false)` 未手改 / **`None` = 这一面看不到 DB**，见 R-D D.2/D.5），而 `inject::WikiLeadMeta.edited` 当初按 D.7 的字面写成 `bool` ⇒ 映射只能把 `None` 压成 `false`（一个确定的「没有编辑过」）。captain 裁决走方案 ①（字段保真三态），不选 ②（再加 `edited_unknown: bool` 会让同一事实有两个真相源）也不选 ③（整条 lead 不注入 = 信息损失）。
  - **改后**：`pub edited: Option<bool>`；渲染 `Some(true)→edited=true`、`Some(false)→edited=false`、`None→edited=unknown`（与同结构体的 `stale` 第三态同一词表）；测试 `an_unknown_edited_state_never_renders_as_false` 钉住「`None` 时**不许**出现字符串 `edited=false`」。
  - **改前红读数**（临时导出树 `%TEMP%\ruagent-t24-before`，把 `inject.rs` 退回旧 `bool` 形状后跑同一判据）：**2 failed / 0 passed**，且 `None` 与 `Some(false)` 的 `<wiki>` 渲染**逐字节相同**（都 `edited=false`）。
  - **单一映射点**（`crates/daemon/src/memembed.rs`，t8 已交付）：`lead_meta(&wiki::WikiLead)` / `relevance_meta(&knowledge::RelevanceScore)` / `enriched_hit(&SearchHit, Option<&RelevanceScore>, Option<WikiLeadMeta>)`；三态字段由 `lead_meta` 原样透传，t19 不需要再判断。
  - 报告：`docs/design/reviews/gen2-memory-repair-o1.md`（含三态三读数、golden 未变证据、以及一条我自查纠正的**假读数**：两棵源码树共用 `CARGO_TARGET_DIR` 会串扰构建产物）。
- **R-6 的更正（2026-09-27，wiki 证伪后；撤回不覆盖、原文保留）**：我在「收敛 C」里给「4 页 vs 5 页」配的解释（**时间差**：相隔 58 分钟、库/树已变）是**错的**，已按 §7.18 **显式撤回**。证伪来自 wiki（其 **2026-09-27T22:34:26.792102+08:00** 同一对象同一时刻的复核）：两读数**逐位相容**，差别是**口径差** —— `wiki docs = 5 / 26 chunk`（含自动生成的 `index`）vs `page docs = 4 / 25 chunk`（不含 index）；且 22:34 与 21:33 逐位相同 ⇒ **库/树没有变**。我的独立复核（**2026-09-27T22:37:08.669435+08:00**）一致：5 行/26 chunk，去掉 `wiki/index`（1 chunk）= 4 页/25 chunk；`wiki_builds` 8 行（done 4 · planned 2 · planned_only 2）。更正后的写法是**单行**：`wiki 文档 = 5（4 页 + index）/ 26 chunk；wiki 页 = 4 / 25 chunk（排除 index.md）`，时间窗 `21:33:17 与 22:34:26 两次读数逐位相同`。同时收下 wiki 的两条追加：**U-8 裁决为 bug**（孤儿哈希：产品路径对账，目标 0 行，不放迁移 §7.75）；**对象集拆两行**（B-09 = 25 chunk 的页内容；B-09b = 4 chunk 的来源），并引其 **R-D D.5 的新鲜度第三态**（`freshness: fresh|stale|unknown` + `PageFreshness.stale: Option<bool>`）。**方法教训写进本文件**：凡「两个读数不一样」，**先列对象集与口径差，再谈时间差**。
- **R-6 的收敛 C（2026-09-27，wiki 回信后；追加，前文不动）**：U-4 **已收敛** —— wiki 页是「**磁盘真相源 + DB 影子**」（`scan` 会删掉文件已不在的影子行，`files.rs:290-309`）；`ResidualSurface` 的单一 `WikiPage` 被**四个载体**取代（`WikiChunk` 逐字 / `WikiPageHash` **只有哈希、答不了 substring** / `WikiPlan` 计划文本 / `WikiBuildPage` 无正文），`WikiPage` 原文保留为历史；新增 **key 约定表**（`wiki/<slug>#c<id>` 可 expand、`build/<id>` 只能走 wiki builds API、`hash:<sha>` 不是可取回对象）；新增**语义第五条**（哈希命中不得计为内容残余）与**第六条**（wiki 面「没有残余」只在一个扫描周期内成立）；新增待定项 **U-7**（报告要不要带有效期/是否重读文件）、**U-8**（孤儿哈希：实测 `doctor-probe` 有哈希行、无 document 行、无文件）。读数两列对照记入 C5-D「已到的读数（wiki）」：其 **21:33** 现场 4 页/25 chunk vs 我 **2026-09-27T22:31:08.59+08:00** 复核 5 页/26 chunk —— **不同但可解释**（相隔 58 分钟，库/树已变，§7.68），两个读数各带时刻、不合并。**〔该句已被 wiki 于 22:34 证伪（口径差，不是时间差）—— 原文保留，更正见下面的「R-6 的更正」〕**
- **R-6 的收敛 A（2026-09-27，recall 第一次回信后；标签在加入「收敛 B」时补为 A，正文未改）**：C5-D 的 U-3 **已收敛** —— knowledge 侧被明确为**五个派生物面**（磁盘 md / `documents` 行 / `chunks.content` / `chunks_fts` / LanceDB 向量），`ResidualHit` 按 recall 的要求加 `origin`（`db|file|both`）与 `path` 两字段；I-A **承诺 t7 之后**提供 `residual_scan(needle, limit)` 与 `document_path(name)`（**尚未存在**，t7 前只允许记 `NotAvailable`）。U-4（wiki 面）**仍待回信**。全部读数逐条引用了 recall 的消息（其自报时间点 **22:1x**，未到分钟位，不补精度）与**我的独立复核**（**2026-09-27T22:16:52+08:00**），见 C5-D「已到的读数」。
- **R-7（captain 裁决，2026-09-27）**：**不动 t2 的终态记录**（terminal results 不可变；追认会给出「终态可补写」的错先例），**记录以本文件的撤回块为准**（证伪者 + 时间点 + 替代写法 + 原文一字未删 = §7.18 要的形状）；并且按 §7.109，那条错解释**没有驱动任何动作**（不是目标，无下游据此排期或改判据）⇒ 只需纠正记录，记录已在本文件。captain 另采纳本文件自加的那条规则为本代纪律：**两个读数不一样时，先列对象集与口径差，再谈时间差**。
  - 给 V-B / RV-B 的读者：**若你在别处（例如 t2 的终态 output）看到「4 页 vs 5 页 = 时间差/库树在动」的说法，它已被证伪；本文件的更正后单行读数才是权威** —— `wiki 文档 = 5（4 页 + index）/ 26 chunk`；`wiki 页 = 4 / 25 chunk（排除 index.md）`；时间窗 `21:33:17 与 22:34:26 逐位相同`。

---

## A 基线表

### A.1 测试基线（记忆 crate 自带判据）

| 项 | 内容 |
| --- | --- |
| 对象集 | `ruagent-memory` crate 的全部测试（单测 + doc-test） |
| 采样面 | 全量（28 个 test fn 全跑，无 filter、无 ignored） |
| 复现命令 | `$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"; $env:CARGO_TARGET_DIR="$env:TEMP\ruagent-rb"; cargo test -p ruagent-memory` |
| 读数 | `running 28 tests` → **28 passed; 0 failed; 0 ignored**；doc-tests 0；`Finished test profile ... in 44.63s`（首轮编译）+ 运行 13.81s |
| 时间窗 | 起 2026-09-27T21:28:58.2687399+08:00（job 起）→ 完成 21:30 前后 |
| 覆盖内容 | 注入契约 10 条（含 proptest `never_exceeds_total` 与 golden `golden_render`）· dedupe 4 条 · lifecycle 10 条 · write/query/namespace 各 1–2 条 |
| 局限 | 全部是 crate 级内存库判据，**没有一条读活库**；crate 测试通过 ≠ 活体行为正确 |

### A.2 活库状态（memories）

| 项 | 内容 |
| --- | --- |
| 对象集 | `~/.ruagent/data/ruagent.db` 的 `memories` 表 |
| 采样面 | 全表 163 行，无 where |
| 复现命令 | `python -c "import sqlite3;c=sqlite3.connect('file:C:/Users/19410/.ruagent/data/ruagent.db?mode=ro',uri=True);print(c.execute('SELECT COUNT(*),SUM(superseded_at IS NULL AND deleted_at IS NULL),SUM(superseded_at IS NOT NULL AND deleted_at IS NULL),SUM(deleted_at IS NOT NULL),SUM(supersedes IS NOT NULL) FROM memories').fetchall())"` |
| 读数 | total **163** · live **157** · superseded-only **6** · tombstoned **0** · `supersedes` 链 **5** |
| 时间窗 | DB mtime 2026-09-27T21:15:21.078+08:00；读数 21:39:58+08:00 |

live 行的 store × namespace 分布（同一命令，`GROUP BY store, namespace`；live=157）：

| store | namespace | 行数 |
| --- | --- | --- |
| lesson | global | 38 |
| lesson | project:distill | 1 |
| lesson | project:ruagent | 1 |
| observation | project:distill | 3 |
| observation | project:qpl | 1 |
| observation | project:ruagent | 33 |
| observation | user | 12 |
| procedure | global | 21 |
| procedure | project:mouse-buttons-ahk | 1 |
| procedure | project:python-3.10.9-offline | 2 |
| procedure | project:ruagent | 4 |
| profile | user | 40 |

### A.3 置信度现状（`confidence`）

| 项 | 内容 |
| --- | --- |
| 对象集 | live 157 行的 `confidence` |
| 采样面 | live 全量（`superseded_at IS NULL AND deleted_at IS NULL`） |
| 复现命令 | `python -c "... ;print(c.execute('SELECT confidence,COUNT(*) FROM memories WHERE superseded_at IS NULL AND deleted_at IS NULL GROUP BY confidence ORDER BY confidence').fetchall())"` |
| 读数 | 0.5×4 · 0.6×5 · 0.7×1 · 0.75×2 · 0.8×78 · 0.9×39 · 0.95×5 · 1.0×23；**distinct=8**；min 0.5 max 1.0 avg **0.8436**；**<0.5 = 0 行** |
| 时间窗 | 读数 21:39:58+08:00 |

写入侧（只读代码，非读数）：`api.rs:3730`（HTTP `memory_write`）固定 **0.9**；`crates/mcp/src/lib.rs:85-90`（MCP `memory_write`）**没有 confidence 参数**；`distill.rs:467` 取抽取值 `m.confidence.unwrap_or(0.8).clamp(0.5, 1.0)`。⇒ 三条生产写入路径里，**两条把 <0.5 变成不可达**（0.9 常量 / 无参数），第三条的下限被 `clamp(0.5,_)` 卡死。任务单里「写入侧固定 0.9」的措辞不够准：今天 0.8 是众数（78/157=49.7%，来自 distill 的默认值），但**结论成立且更强** —— `<0.5` 在任何生产路径上都不可达。

消费侧（只读代码）：`render_context`（`inject.rs:350-415`）**不渲染 confidence**，`ContextItem`（`inject.rs:69-78`）没有该字段 ⇒ 注入给 agent 的记忆**完全不携带置信度**。面板 `panel/src/views/Memory.tsx:486-497`（t253）已从「只在 <0.5 时渲染」改成**恒渲染两位小数**，`<0.5` 时追加「低」标记 ⇒ 该标记在 157 行 live 上 **0 个正例**。

### A.4 向量与 embedder

| 项 | 内容 |
| --- | --- |
| 对象集 | live 157 行的 `embedder` / `embedding` |
| 采样面 | live 全量 |
| 复现命令 | `python -c "... ;print(c.execute('SELECT embedder,COUNT(*),SUM(embedding IS NULL),LENGTH(embedding)/4 FROM memories WHERE superseded_at IS NULL AND deleted_at IS NULL GROUP BY embedder,LENGTH(embedding)/4').fetchall())"` |
| 读数 | **157/157** = `fastembed:multilingual-e5-small`；`embedding IS NULL` = **0**；维度 **384**（`LENGTH(embedding)/4`） |
| 时间窗 | 读数 21:39:58+08:00 |

⇒ 单一向量空间、无混代（`memembed::semantic_search` 会跳过长度不符的行，今天没有这种行）。这是「余弦可以作为候选集生成器」的前提。

### A.5 来源标记与「蒸馏」布尔（§7.140 复核）

| 项 | 内容 |
| --- | --- |
| 对象集 | `memories.source_episode` × `episodes.kind` |
| 采样面 | `memories` 全表 163 行 + live 157 行 |
| 复现命令 | `python -c "... ;print(c.execute('SELECT SUM(source_episode IS NULL),SUM(source_episode IS NOT NULL),COUNT(DISTINCT source_episode) FROM memories').fetchall());print(c.execute('SELECT kind,COUNT(*) FROM episodes GROUP BY kind').fetchall())"` |
| 读数（全表） | `source_episode` 非空 **162/163 = 99.4%**（与 closure §7.140 在 2026-09-26 记的 162/163 一致）；被引用的 episode 只有 **7 个**；其中 **156 行**指向**同一条**迁移建的 `manual` episode（`episodes.source_run='ruagent:legacy-distilled-prefix'`） |
| 读数（live） | `source_episode` 非空 **156/157**（唯一 NULL 在 `observation`）；按 kind 计：`manual`×1 → **152 行**，`mcp_write`×4 → 4 行 |
| 读数（episodes 表） | `mcp_write` **16** · `manual` **1** · **`run_turn` 0** |
| 时间窗 | 读数 21:39:58+08:00 |

**⇒ 两个标记都不是「会话蒸馏」的标记**：
1. `source_episode IS NOT NULL`：live 覆盖率 99.4%，其中 152/157 指向同一条合成 episode ⇒ 信息量≈0（§7.140 的结论在现场复现成立）。
2. 面板/API 真正的布尔（`api.rs:3794-3831 distilled_ids`）按 `episodes.kind='run_turn'` 派生 ⇒ 因为 `run_turn` episode **= 0**，今天 **157 行 live 里 0 个正例**。标记是精确的，但**当前没有任何正样本**：不能据此说它「好」或「坏」，只能说它未落地（§7.140 的纪律）。
3. 0 个 `run_turn` episode 的原因可查：**列本身从 0004 迁移起就在**（`0004_memory.sql:25`），变的是**写入侧** —— t347 之前 `distill.rs` 一律传 `source_episode: None`（closure §7.64 在 2026-09-26 记下这一点），t347 起才写；而 `distill_log` 最后一次成功是 **2026-09-26T10:28:22Z**，早于 t347 的迁移 episode（ingested_at 2026-09-26T21:09:10Z）⇒ **t347 之后没有跑过一次成功的会话蒸馏**，那 156 行是迁移按内容前缀回填的，不是 distill 写出来的。

### A.6 生命周期现状（supersede / delete / restore / purge）

| 项 | 内容 |
| --- | --- |
| 对象集 | `memory_diffs` 全表 + `memories.deleted_at/superseded_at` |
| 采样面 | `memory_diffs` 237 行全量 |
| 复现命令 | `python -c "... ;print(c.execute('SELECT op,COUNT(*),MIN(ts),MAX(ts) FROM memory_diffs GROUP BY op ORDER BY COUNT(*) DESC').fetchall())"` |
| 读数 | insert **181** · purge **23** · reject **15** · skip_dedupe **10** · supersede **5** · delete **2** · restore **1**（合计 237；窗 2026-09-12T15:13:49Z → 2026-09-26T14:59:24Z） |
| 现存量 | tombstoned **0**（今天没有软删除行）；23 次 purge 的 reason 全是「purged a live row (hard delete)」，**没有一次是清 tombstone** |
| delete/restore 的窗 | delete 2 次：2026-09-26T13:59:20.998Z / 13:59:23.941Z；restore 1 次：13:59:24.441Z |
| 时间窗 | 读数 21:39:58+08:00 |

⇒ 「可审计删除」的**机制**在（可区分 NotFound / AlreadyDeleted / NotDeleted，且每次都落 `memory_diffs`），但**只被 2 次 delete + 1 次 restore 演练过**，且都在同 4 秒内（很可能同一次测试）。

### A.7 蒸馏与召回日志

| 项 | 内容 |
| --- | --- |
| 对象集 | `distill_log` / `recall_log` |
| 采样面 | 两表全量（`distill_log` 无重复键，`INSERT OR REPLACE` 每会话一行） |
| 复现命令 | `python -c "... ;print(c.execute('SELECT COUNT(*),MIN(distilled_at),MAX(distilled_at),SUM(memories_written),SUM(entities_written),SUM(relations_written),COUNT(DISTINCT agent) FROM distill_log').fetchall());print(c.execute('SELECT COUNT(*),MIN(ts),MAX(ts) FROM recall_log').fetchall())"` |
| 读数（distill_log） | **33 行**，窗 2026-09-13T17:06:33Z → **2026-09-26T10:28:22Z**，Σmemories_written=**85**，Σentities=69，Σrelations=39，agent 只有 `dsh`（33/33） |
| 读数（recall_log） | **651 行**，窗 2026-09-16T13:05:24Z → 2026-09-27T13:23:02Z；`source` 为 NULL 的 624 行，有名 source 27 行（probe/t290-live/… ） |
| 时间窗 | 读数 21:39:58+08:00 |
| 旧值对照 | closure §7.140/§1 在 **2026-09-26** 记 `recall_log=650`、`distill_log=32`；今天 651/33。**recall_log 会因任何人的召回调用增长**（最后一行 13:23:02Z 就是 16 分钟前别人写进去的），引它必须带时间点 |
| 局限 | `distill_log` **只记成功**：失败（今天 daemon.log 里的 `auto-distill failed`）一行都不进库 ⇒ 库里读不到失败率。图已在 t3 R-C 登记此条（D4），我同意其方向，见 E.7 |

**「无 consolidate / reflection」复核：成立，但要拆成两句**（否则会读成同一个数）：
1. **有**会话级抽取（`distill.rs`：extract → dedupe/supersede → write），历史上写过 85 行记忆 —— 所以不能说「会话结束什么都不产出」。
2. **没有**跨会话/跨行的巩固：代码里没有第二条流水线（无「按主题聚合多行旧记忆、产出一条更高层结论」的入口），`memories` 也没有任何「使用/巩固」状态列（A.8）⇒ 记忆只能被同样的措辞**替换**，不能被**综合**。

### A.8 记忆表有没有「使用/衰减/巩固」状态（`memories` 列普查）

| 项 | 内容 |
| --- | --- |
| 对象集 | `memories` 的列名集合 |
| 采样面 | `pragma_table_info('memories')` 全列（14 列） |
| 复现命令 | `python -c "... ;print(c.execute(\"SELECT COUNT(*) FROM pragma_table_info('memories') WHERE name IN ('access_count','last_used_at','decay','promotion','valid_from','valid_to','pinned')\").fetchall())"` |
| 读数 | **0 / 7**（14 列 = id, store, namespace, content, content_hash, confidence, source_episode, supersedes, superseded_at, created_at, updated_at, embedding, embedder, deleted_at） |
| 时间窗 | 读数 21:39:58+08:00 |

⇒ 今天没有「被读过几次 / 上次是什么时候用上」的落点，因此**衰减与巩固在数据模型上不可能**：读路径 `query.rs:49` 只能 `ORDER BY updated_at DESC`（写入时间），注入选择 `inject.rs:144-147` 也自述「the N most recent, not the N most relevant」。召回确实在发生（`recall_log` 651 行），但**没有任何一行被写回使用事实**。

### A.9 合并判据在真实语料上的读数（dedupe 校准）

| 项 | 内容 |
| --- | --- |
| 对象集 | live 157 行两两配对，**限同一 (store, namespace)**（与 `distill.rs:436-438` 的候选面一致：只有同 store+namespace 才可能被判合并） |
| 采样面 | 12 个分组、**2297 对**，全量无抽样 |
| 复现命令 | §A1 的探针（`ruagent_memory::dedupe::judge` 的逐行等价移植，**先用 crate 自带 22 条判据向量自检**） |
| 读数 | **Mergeable 0 · Same 0 · Refused 2297**；即今天活库上该判据的**触发率 0**；唯一观察到的触发是历史上 5 次 supersede（A.6） |
| 自检 | 移植版复现 crate 判据 **22/22**（四条中文改写两两 Mergeable；极性对/内容替换/星期替换 Refused；`进行` 位移 Mergeable；英文 filler Mergeable、内容替换 Refused）⇒ 这个端口可信，0 不是我写坏的 |
| 时间窗 | 读数 21:50:56+08:00 |
| 局限 | 「0/2297」**不能**单独读成「判据太窄」，因为 t329 之后写入侧已经用它合过（幂等）；它的正确读法是**「今天没有可合并的残留」与「它看不见真实改写」两件事在这一个数里分不开** —— 下面用余弦分布把第二件事单独量出来 |

**余弦分布（同一采样面，157 行 384 维 e5-small 向量）**：

| 读数 | 值 |
| --- | --- |
| 同 scope 配对余弦 | max **0.9884** · p99 **0.9525** · p95 **0.9177** · 中位 **0.8500** · min 0.7434 |
| `≥0.95` / `≥0.90` / `≥0.86` / `≥0.80` | 24 对（1.04%）/ 366 对（15.93%）/ 1038 对（45.19%）/ 1779 对（77.45%） |
| 每行「最好的邻居」余弦 | 中位 **0.9101**；128/157 行存在 ≥0.86 的邻居 |
| 复现命令 | §A2 探针 |
| 时间窗 | 读数 22:01:50+08:00 |

**两条必须一起读的结论**：
1. **绝对余弦阈值在这套语料+模型上不是等价判据**：阈值 0.86 会放行 **45.19%** 的全部配对，阈值 0.90 放行 15.93% —— 任何「相似度 ≥ θ 就合并」的写法都会大面积误并。
2. **反过来，判据看不见模型眼里的同义改写**：余弦最高的 6 对（0.9827–0.9884）逐字是同一件事实，例如
   - `#5 用户的语音输入法用 F6 作为启动/停止语音输入的快捷键。` ↔ `#23 用户的语音输入法以 F6 键作为启动/停止语音输入的触发键。`（cos **0.9884**）
   - `#145 讨厌轮询式交互，偏好状态变化时自动通知／事件推送。` ↔ `#149 用户讨厌轮询，偏好由系统主动推送的自动通知。`（cos **0.9861**）
   - `#19 xlwt 的版本属性是 __VERSION__ 而不是 __version__…` ↔ `#40 xlwt 的版本属性名是 __VERSION__ 而非 __version__…`（cos **0.9846**）

   而 `judge()` 对**全部 2297 对**都 Refused ⇒ 该判据在这批真改写上**召回 0**。

**候选集上界（为「有界的候选面」做标定）**：以 `cos ≥ 0.95` 的 24 对为代理正例，按「每行取余弦 top-K 邻居」的候选集，K=1 恢复 21/24、K=2 恢复 23/24、**K=3 恢复 24/24**（K≥3 不再增长）。
**这条读数的局限要写在脸上**：代理正例本身是按余弦定义的 ⇒ 「余弦能找回余弦」有循环性，它**只能**证明「有界候选面在近同义对上不丢东西」，**不能**证明决策规则对。决策规则要用带标签的语料另测（C2）。

### A.10 注入面现状（预算 / 块头 / 截断 / 丢块 / 生产者）

**契约常量（只读代码，`crates/memory/src/inject.rs`）**

| 项 | 值 | 位置 |
| --- | --- | --- |
| 预算单位 | **字符**（`chars().count()`，Unicode 标量值）——**不是字节** | `inject.rs:382,392,413` |
| `InjectionBudget::default()` | `per_block=1024` / `total=4096`（字符） | `inject.rs:15-24` |
| 块头/块尾 | `<{tag}>\n` … `</{tag}>\n` | `inject.rs:378-379` |
| 可见截断 | `… [+N chars truncated]`（`tail_truncated`，省略号后有**一个空格**）· `… [{what} truncated at {N} chars]`（`cut_at`） | `inject.rs:41-49` |
| 逐块截断 | 只对**块体**截，附 `… [+N chars truncated]`；`N = body.chars().count() - per_block` | `inject.rs:382-388` |
| 丢块 | 整块丢（**不部分发**），`dropped += group.len()` ⇒ 计的是**条目数**；通知 `<context_budget>\n… [+N items dropped: context budget reached]\n</context_budget>\n`，放不下时退化为 `… [+N dropped]` | `inject.rs:392-412` |
| tag 与 drop rank | `user_profile`(0) · `relevant_memories`(1) · `knowledge`(2) · `wiki`(3) · `project_context`(4) · 未知 tag(5) | `inject.rs:119-126,261-270` |
| 抽取条数 | `KNOWLEDGE_SOURCES=3` · `WIKI_PAGES=2` | `inject.rs:273-274` |
| 两条路径的选择参数 | `CHAT_SELECTION`: groups(profile/user 5, observation/user 5, observation/global 3【死组】, procedure/global 3, lesson/global 3) + `query_top_n=4, query_min_score=0.34`；`RUNS_SELECTION`: profile/user 5, observation/user 8, observation/project 8 + **`query_top_n=0`（无查询腿）** | `inject.rs:179-255` |
| 唯一选择规则 | `crates/daemon/src/memembed.rs:227-278 select_injection_memories`（两路径共调） | — |
| 唯一渲染器 | `render_context`（两路径都调 `render_context(&items, &InjectionBudget::default())`） | `chat.rs:581` · `runs.rs:1906` |

**盘上真实 render 普查（第三层读数）**

| 项 | 内容 |
| --- | --- |
| 对象集 | `~/.ruagent/data/transcripts/*.jsonl` 里所有 `event.type == "context_injected"` 的 `render` 字段 |
| 采样面 | 目录下**全部 147 个 `.jsonl`**（= 147 个 run transcript）；命中 80 个事件、分布在 78 个文件里 |
| 复现命令 | §A3 探针（python 逐行 JSON 解析；按 render 首字节形状分类） |
| 读数（按形状分类） | `tagged_contract_render` **19**（契约渲染：`<tag>` 块）· `legacy_memory_context_pre_t260` **35**（旧 `[memory context — …]` 措辞）· `role_retry_context` **24**（`[role — you are]`，重试/角色上下文，也走 ContextInjected）· 其它 **2** |
| tagged 类的读数 | 字符数 min **108** / 中位 **1660** / max **2458**（离 total=4096 还有 **1638** 余量）· >1024 字符 18/19 · >4096 字符 **0/19** · 带 `chars truncated` **18/19**（17 次 `+345`、1 次 `+841`）· 带丢块通知 **0/19** · 带 `<context_budget>` **0/19** · 出现 `<knowledge>` **1/19** · `<wiki>` **1/19** |
| 全 80 个事件 | 带截断标记 **25/80**（31.3%）；带丢块通知 **1/80**；带 `<context_budget>` **0/80** |
| 时间窗 | 事件 ts 2026-09-12T15:13:49Z → 2026-09-26T14:33:18Z；读数 21:47:33+08:00 / 21:49:30+08:00 |

**对 closure §6.1 两条旧读数的复核（每条都要么被推翻、要么仍在）**

| closure §6.1 在 2026-09-26 记的 | 今天复核 | 归因 |
| --- | --- | --- |
| 「chat 注入预算 **700 字节**（用 `entry.len()`）⇒ 115 行只发 4 条、静默丢 111 行」 | **代码已不存在**：`chat.rs:521-583` 现在构造 `ContextItem` 并调 `render_context(..., &InjectionBudget::default())`（1024 字符/块、4096 字符/总）。旧读数因此**不再成立**，它的对象（自写预算的 chat 构造器）已被 t260 删掉 | 行为变更，不是读数漂移 |
| 「chat 路径 **0/54** 有截断标记」 | **无法在今天的盘上复核**：147 个 transcript 全是 `run-*.jsonl`；本机 chat 会话的载体是 harness 自己的 `~/.dsh/sessions/**.jsonl.zstd`（非 ruagent 事件流），且 `data/transcripts/` 里没有任何 `chat-*.jsonl`。**记为未测（A.12）**，不写成 0 | 采样面缺口 |
| 「契约路径 24/25 有 `[+N chars truncated]`、0/25 有丢块通知」 | **仍在，但分布变了**：tagged 类 18/19 有截断标记、0/19 有丢块通知（与「0/25」同类）；全 80 事件里只有 1 次丢块通知 | 与今天一致 |

**收敛点现状（只读代码）**：两条路径已经在同一处汇合 —— 选择规则 `memembed::select_injection_memories`、渲染器 `render_context`、排序键 `inject::tag_rank`、丢块/截断词表 `tail_truncated/cut_at` 各只有一份。**残留差异是数据而不是代码**：chat 有查询腿（top4/0.34）而 runs 没有（`query_top_n=0`，`inject.rs:249-254`）；chat 读 procedure/lesson（global），runs 读 project 面而 chat 不能（因为 chat 钉在 cwd 而不是 project 名，`inject.rs:238-241` 自述这是 GAP）。

### A.11 任务单「已知缺陷」逐条复核

本表每行的**对象集** = 该条缺陷的执行路径（`crates/memory` 的判据/读路径或 `crates/daemon` 的消费路径，逐行在「证据」列给 file:line）；**采样面** = A.2–A.10 已给的现场读数（每条在证据里点名它引用的是哪一节）；**复现命令** = 该行证据给出的 file:line 只读核对 + 所引 A.x 节的命令；**读数** = 「复核结论」列；**时间窗** = 2026-09-27T21:28:58 → 22:01:50 +08:00（活库读数）与当前 HEAD 的工作树（代码核对）。

| # | 任务单原话 | 复核结论 | 证据（对象集 + 采样面 + 复现） |
| --- | --- | --- | --- |
| D-1 | confidence 写入侧固定 0.9 ⇒ 契约承诺的 `<0.5` 渲染在真实数据上永不触发 | **成立（措辞需精确化）** | 三条写路径：HTTP 固定 0.9（`api.rs:3730`）、MCP 无该参数（`mcp/src/lib.rs:85-90`）、distill `clamp(0.5,1.0)`（`distill.rs:467`）；live `<0.5 = 0/157`（A.3）。「契约承诺的 `<0.5` 渲染」今天落在**面板**（`Memory.tsx:494`），注入契约**根本不渲染 confidence**（`inject.rs:69-78,350-415`） |
| D-2 | 无 consolidate / reflection（会话结束不产出可复用结论） | **方向成立，须拆两句** | 有会话级抽取（`distill_log` 33 行/85 行记忆）；无跨行巩固（无第二条流水线；`memories` 无使用状态列 0/7，A.8）；且 t347 之后 `run_turn` episode = 0 ⇒ 今天的「蒸馏自会话」标记 0 正例（A.5） |
| D-3 | dedupe 阈值与合并判据没有证据门槛（「相似」没有命名对象集） | **成立，且比原话更严重** | `judge()` 的判据是**一份手写词表**（`IGNORABLE` 28 项 / `POLARITY` 21 项，`dedupe.rs:23-34`），没有任何语料测量支撑其覆盖；候选面是**同 store+namespace 的全部 live 行**（`distill.rs:436-438`，无 LIMIT，无阈值）；实测在真改写上召回 **0/2297**，而余弦 ≥0.86 放行 45.19%（A.9）。另：`distill.rs:371-372` 的 doc 注释仍写着「cosine >= 0.90」，**与代码（judge）不一致**（doc/code 漂移） |
| D-4 | `memory/list` 未知 store 静默降级成 Observation、namespace 默认 user（store=lesson → 0 行，而 counts 显示 38） | **list 已修，write 未修** | `memory_list` 走 `parse_store`（`api.rs:3029-3039`）⇒ 未知 store **400**；namespace 缺省 **None=不过滤**（`api.rs:3862-3865`），响应同时给 `total`/`matched`/`counts`。**但 `memory_write`（`api.rs:3705-3710`）仍把未知 store 静默当 Observation**，MCP 正是走这条（`mcp/src/lib.rs:81`）；另外 `query.rs:11-16 row_to_memory` 对未知 store 字符串也静默回落到 Observation |
| D-5 | episode 标记宽窄（§7.140）：面板要的是「这个 episode 是不是一次会话蒸馏」，应由 API 派生布尔 | **已实现（t350），但今天 0 正例** | `api.rs:3794-3852`：`DISTILLED_EPISODE_KIND="run_turn"` + `distilled_ids()` join + `with_distilled()` 输出布尔；`memory_list` 与 `memory_search` 都带上了。实测 `run_turn` episode **0** ⇒ 157 live 行 **0 正例**（A.5） |

**额外发现（不在任务单里，但会拦下游工作）**

| # | 发现 | 证据 / 影响 |
| --- | --- | --- |
| X-1 | 注入给 agent 的记忆**不带置信度**，而面板带 ⇒ 「低置信」这个质量信号只对**人**可见，对**agent**不可见 | `inject.rs:69-78,350-415` vs `Memory.tsx:492-497` |
| X-2 | 今天没有任何「记忆被用过」的事实被写回 ⇒ 召回无法自我改进（无反馈闭环） | `memories` 0/7 个使用类列；`recall_log` 651 行只有查询侧；`memory_diffs` 无 read/use op |
| X-3 | `memory_diffs` 的 op 词表是**写入侧**的一部分（`insert/supersede/skip_dedupe/reject/delete/restore/purge`），但**没有 `merge_judged`** ⇒ 合并**决策**（不是结果）无法审计 | `write.rs:144-153,171-195`；`lifecycle.rs:97-106,144-153,232-241` |
| X-4 | `distill_log` 只记成功（失败零行）⇒ 库里读不到蒸馏失败率 | `distill.rs:216-237`；图在 t3 R-C 的 D4 已登记，我同意（E.7） |
| X-5 | `pending`/`episodes` 的 `ref_time`（事件时间）与 `ingested_at`（事务时间）**双列存在**，但记忆中没有任何读路径用它（`memories` 只有 `created_at/updated_at`）⇒ 「双时态」在记忆侧是名义的 | `0004_memory.sql:10-11`；`memories` 列普查（A.8） |

### A.12 本单**未取到**的读数（写清原因，不写成 0）

**对象集** = 下列每一个「本来该测、但今天测不到」的面；**采样面** = 每行「采样面/为什么测不到」列给出的那次尝试；**读数** = 「判为未测」，不是 0；**时间窗** = 同上（21:28:58 → 22:01:50 +08:00）；**复现命令** = 每行的「复现命令」列（凡写 `不可复现` 的，原因是缺失的是**采样面本身**，不是命令）。

| 未测项 | 复现命令 | 采样面 / 为什么今天测不到 |
| --- | --- | --- |
| chat 路径的 render 普查（旧 0/54） | 不可复现（缺采样面） | 本机 chat 会话的载体是 harness 的 `~/.dsh/sessions/**.jsonl.zstd`，不是 ruagent 事件流；`data/transcripts/` 只有 `run-*.jsonl`。要测需解压 zstd 或跑一次真实 chat（后者会写活库/活 transcript，见纪律） |
| 注入 render 是否**被 agent 收到** | `cargo test -p ruagent-daemon --test injection_e2e`（本单未跑） | 事件只证明「发出了」（§7.24）；要第三层证据需要 mock-agent 回显（`crates/daemon/tests/injection_e2e.rs` 已有此形态，本单不跑） |
| 面板/API 的运行时表现（`distilled` 布尔、confidence 渲染） | 不可复现（越出 inScope） | 面板不是本单范围（out of scope: panel/），且本机 daemon 是别人的活体，不启停 |
| 活库 `counts` vs `matched` 的端到端差异 | `GET /api/v1/memory/list?store=lesson`（只读） | 本单只复核代码，不当读数（避免与别人的活体取证互相污染） |
| 「失败率」 | 不可复现（缺列/缺同一时间窗） | `distill_log` 不含失败行（X-4），daemon.log 的时间窗与 `distill_log` 不同 ⇒ 今天不可读；这正是 E.7 要补的 |
| 压测/规模读数 | 不可复现（缺样本量） | 157 行 live 的库上取规模读数没有意义（<1k 行）；本单不写 |

---

## B SOTA 对照（≥4 条一手来源；每条给 采纳/不采纳 + 理由 + 代价）

来源（全部为论文/vendor 一手文档；引用 URL）：
[S1] Mem0: Building Production-Ready AI Agents with Scalable Long-Term Memory — https://arxiv.org/abs/2504.19413
[S2] Zep: A Temporal Knowledge Graph Architecture for Agent Memory — https://arxiv.org/abs/2501.13956 ；双时态的措辞另见 Zep 一手文档 https://www.getzep.com/ai-agents/temporal-knowledge-graph/
[S3] MemGPT: Towards LLMs as Operating Systems — https://arxiv.org/abs/2310.08560 ；Letta Memory Blocks（vendor 一手）— https://www.letta.com/blog/memory-blocks/
[S4] Generative Agents: Interactive Simulacra of Human Behavior — https://arxiv.org/abs/2304.03442
[S5] MemoryBank: Enhancing Large Language Models with Long-Term Memory — https://arxiv.org/abs/2305.10250
[S6] GDPR Art. 17（right to erasure）— https://gdpr-info.eu/art-17-gdpr/ ；ICO 指引 — https://ico.org.uk/for-organisations/uk-gdpr-guidance-and-resources/individual-rights/individual-rights/right-to-erasure/

### B.1 [S1] Mem0：抽取 → 合并/更新 → 检索 的流水线

- 一手内容：Mem0 的定位是「dynamically **extracting, consolidating**, and retrieving salient information from ongoing conversations」；其图版本（Mem0g）用**两阶段 LLM 抽取**，并把每个新事实与**已检索到的相似既有事实**比对后决定 ADD/UPDATE/DELETE/NOOP。
- **采纳（部分）**：采纳「**每条新记忆的合并决策必须针对一个具名候选集**，且决策本身可读（ADD / UPDATE / SUPERSEDE / NOOP）」这一形状。这正是 D-3 缺的东西；落到我们的数据模型就是 `MergeVerdict` + `memory_diffs.op=merge_judged`（E.2/E.3）。
- **不采纳**：① 不把「向量库」当唯一后端（我们的读路径、审计与生命周期都建立在 SQL 行上，迁移成本远大于收益）；② 不采纳「每条事实一次 LLM 判定」的无条件形态 —— 我们每会话已经有一次抽取调用（+1 次 LLM 往返），再加「每条事实一次判定」的代价是**线性放大**，而今天 live 只有 157 行，收益无法证明。**代价（采纳那部分的）**：候选集要一次余弦扫描（157 行 × 384 维，今天毫秒级；行数上千后需要索引），且要在 `memory_diffs` 多写一种 op（面板/i18n 需同步）。

### B.2 [S2] Zep/Graphiti：双时态与冲突消解（时间有效性的第一手依据）

- 一手内容：Graphiti 维护**双时间线**（valid time = 事实在世界里为真的时间；ingestion/provenance time = 它被写入的时间），**被取代的事实是 invalidate（失效）而不是 delete（删除）**，并支持「按 as-of 时间查询当时的真值」。
- **采纳**：① 记忆行也要有**有效期**语义 —— 我们已有 `superseded_at`（= invalidated at）但没有 `valid_from`（= 事实开始为真的时间；今天只有 `created_at/updated_at`）。采纳为：给 `memories` 加 `valid_from`/`valid_to`（或明确用 `created_at`/`superseded_at` 作代理并在注入里**渲染日期**，这一点我们已经在做）；② 采纳「失效≠删除」——我们已经是这个形状（A.2：superseded 6 行仍在，tombstone 0）。
- **不采纳**：不采纳「每次写入都用 LLM 做冲突检测」；我们的写入是**高频、无人监督**的（MCP 工具 + distill），每次写入加一次 LLM 判定的代价与不确定性都超出记忆这一层的收益。冲突消解改由**合并判据 + 显式候选集**承担（B.1），LLM 只在**会话结束的巩固/反思**（B.4）里出现一次。
- **代价**：加 `valid_from/valid_to` 是一次 store 迁移（I-SCHEMA 面）+ 所有读路径要决定「按哪个时间排序/过滤」；今天没有任何 `valid_from` 数据，迁移只能回填 `created_at`（**这是回填，不是事实**，必须在迁移里自述，理由同 §7.137/§7.77）。

### B.3 [S3] MemGPT/Letta：分层记忆 + 自我编辑

- 一手内容：MemGPT 用「虚拟上下文管理」在快/慢存储层级之间搬数据；Letta 把它产品化为 **memory blocks**（可被 agent 用工具 `rethink_memory` 就地重写的具名块）。
- **采纳**：① 采纳「**有界 + 具名 + 可寻址的块**」这一形状 —— 我们已经有了（`TAG_*` + `render_context` 的 1024/4096 字符边界，A.10），把它写进 D 节当作**冻结契约**而不是实现细节；② 采纳「**巩固是一个显式动作**」：会话结束的巩固要有一个**具名入口**（E.4），而不是散在后台日志里。
- **不采纳**：不采纳「让 agent 在会话中直接自我编辑长期记忆块」。理由：注入面是**不受信内容**进入上下文的地方（wiki/knowledge/transcript 都可能含指令），给它写权限等于把 prompt injection 升级为持久化写入；今天的写入路径全部经过 `write_memory` 的命名空间治理（`namespace.rs`）与审计，这个性质我不想用「更强的自编辑」换掉。
- **代价**：显式巩固 = 每会话**至少一次额外的 LLM 往返**（今天 distill 已经是一次，巩固可能第二次或并入同一次回答），代价必须在 I-C 的预算里写出来，不能隐式。

### B.4 [S4] 生成式 agent 的 reflection：把经历综合成更高层结论

- 一手内容：记忆流上做检索分数 `α·recency + β·importance + γ·relevance`（recency 用**指数衰减**，importance 由 LLM 打 1–10，relevance 是余弦）；当最近记忆的累计 importance 超过阈值（论文里 **150**）就进入 reflection，生成「高层问题 → 洞见」并**把洞见写回记忆流（带引用）**。
- **采纳**：① **reflection 作为会话关闭的一道工序**：产出的是**带来源引用的更高层结论**，而不是又一条原始观察；② **recency 用指数衰减参与排序**（我们现在是 `updated_at DESC` 的平铺，A.8），与 B.5 的 reinforcing 结合。
- **不采纳**：不采纳「每条记忆都要一个 1–10 的 LLM importance 分」。理由：那是**每条**一次 LLM 判定，代价随记忆数线性增长；我们改用**可观测证据**（episode kind、被引用的次数、显式确认信号）推导 importance，不引入新的、无法校准的自评标量。
- **代价**：反思产物会**增加**行数（今天 157 行里已经有 99.4% 共享同一条 provenance，A.5）⇒ 反思必须自带 provenance 与「它是派生物」的标记，否则会把 A.5 那个「覆盖面 99% = 没有标记」的毛病复制一遍（§7.140）。

### B.5 [S5] 认知架构的 decay/promotion（MemoryBank 的艾宾浩斯遗忘曲线）

- 一手内容：MemoryBank 的 memory updater 依 **Ebbinghaus 遗忘曲线** `R = e^{-t/S}` 做记忆更新：保留率随时间下降，**被重复访问/复习会重置并抬高曲线**（S 增大）。生成式 agent 的 recency 项同样是**指数衰减**（B.4）。
- **采纳**：① **衰减是排序信号，不是删除动作**：`score = 融合分 + λ·e^{-Δt/S}`，`S` 随 `access_count` 增长；② 需要落 `access_count` / `last_used_at` 两列（今天 0/7，A.8）并在读路径写回。
- **不采纳**：不采纳「衰减到阈值就自动遗忘/删除」。理由：① 我们的语料是**用户的技术事实**（lesson/procedure），误删的代价是「答案证据永久消失」，而收益只是一个排序微调；② 立法面（B.6）要求删除**可解释、可审计**，自动遗忘无法给出一条人能读的理由。衰减只影响**没被用上的东西排后面**，删除仍必须走显式的 `delete/purge` + `memory_diffs`。
- **代价**：读路径要**写**（更新 access_count）⇒ 注入/召回会变成写操作，必须走单写者 actor（`store` 的 `Db::call`）并注意「取证即写库」的副作用：本单因此禁止用 `/api/v1/recall` 取证（纪律已列）。另需证明写回不改变**返回内容**（只改排序），否则读数会互相污染。

### B.6 [S6] 可审计删除（遗忘权）

- 一手内容：GDPR Art. 17 赋予数据主体「erasure without undue delay」的权利（含「数据不再必要」「撤回同意」「非法处理」等要件），并列出对法定义务/法律主张等例外；ICO 指引把它落到「组织必须能证明已响应」。
- **采纳**：**遗忘必须同时是（a）可执行的（内容真的消失）与（b）可证明的（审计里留下"谁在何时因何被擦除"但不留内容）**。我们今天的 `purge_memory` 已经是「真删行 + FTS 触发器跟随 + 审计只留 id/时间/reason」（`lifecycle.rs:182-247`），方向正确；缺口在**派生面**：`episodes.content` 里存着**原始 transcript**（会话原文可能含被要求移除的秘密），而 wiki/knowledge 文档是另一套存储 —— 今天没有一处代码能回答「为这条内容，还有哪些派生物存在」。
- **不采纳**：不采纳「软删除就算遗忘」（`deleted_at` 是行还在、内容还在，A.6 里 23 次 purge 全是对 live 行做的，说明我们的 purge 已经在承担真删）；也不采纳「审计也一起删」（那就没有可证明性）。
- **代价**：需要一次**跨存储的残余面搜索**（memories + memories_fts + episodes + knowledge/wiki 文档），这是本规格里**唯一跨出 crates/memory 的项**（与 I-A/I-D/I-SCHEMA 交界，见 E.6），且必须有「删除前/删除后」两次读数才有证明力。

---

## C 目标表（每条：metric + baseline + target + 复现命令 + 标定读数）

约定：`baseline` 一律取 A 节的现场读数；`标定读数` = 这条目标**今天已经量到的、能让目标不落空的那部分**（避免「改完才知道有没有东西可改」）。所有 target 都是**可证伪**的：给出「什么情况算失败」。

**两条边界（R-1/R-2/R-3 之后）**：
1. 本节（C）**只含 mem-core 拥有文件的条目**（`crates/memory/**` 与 `crates/daemon/src/memembed.rs`）。属于 `crates/daemon/src/api.rs` 的条目已移到 **`## C-INT`**（owner=INT/t19）；C4/C7 里那两行的**结论与读数保留在原位并标注**，但**执行与验收归 C-INT**。
2. 依赖 `crates/daemon/src/distill.rs`（I-C/t9）才能闭合的条目，逐条标了 **jointly-delivered** 与**责任半边**；I-C 未落地时，那半边判 **not_measured（原因：依赖未落地）**，不判过也不判败。

### C1 置信度可校准（`<0.5` 必须可达，且由具名信号决定）

| 项 | 内容 |
| --- | --- |
| metric | (a) live 行中 `confidence < 0.5` 的行数；(b) 写入侧 confidence 的取值是否由**具名信号**决定（同一条记忆的证据变差 ⇒ 值必须变） |
| baseline | `<0.5 = 0/157`；distinct=8；min=0.5；三条写路径：HTTP 常量 0.9 / MCP 无参数 / distill `clamp(0.5,1.0)`（A.3） |
| target | ① `crates/memory` 新增 `confidence` 模块，导出 `ConfidenceSignals` + `confidence(signals) -> f64`，**具名信号**至少三个：`user_confirmed`（1.0）、`unconfirmed`（0.8）、`hedged`（≤0.4）；② distill 不再 `clamp(0.5,_)`，改为把信号交给该函数（**依赖 I-C**，见 E.1）；③ 判据：固定 12 条事实的场景测试里，`hedged` 的那条 `<0.5` 且三个信号产出 **≥3 个不同值**；④ 注入面按 D.4 只在 `<0.5` 时追加可见标记 `[unverified]` |
| 复现命令 | `cargo test -p ruagent-memory confidence` + `python -c "... ;print(c.execute('SELECT COUNT(*) FROM memories WHERE deleted_at IS NULL AND superseded_at IS NULL AND confidence < 0.5').fetchall())"`（活库读数，改造后仍应允许为 0：**活库无正例不等于功能未落地**，正例由场景测试提供） |
| 标定读数 | 今天的分布（8 个值/0.5–1.0）证明「值域已经有人用」；而 `clamp(0.5,_)` 与常量 0.9 是**代码级铁证**：不先拆掉它们，任何 `<0.5` 的写法都是死代码 |
| 失败判据 | 若改完后 12 条场景里 distinct 仍 =1，或 `hedged` 那条 ≥0.5 ⇒ 目标未达成 |
| 交付与责任（R-3） | **jointly-delivered**。mem-core 半边 = `crates/memory/src/confidence.rs`（具名信号 → 值 + 单测）；graph / I-C 半边 = `distill.rs:467` 去掉 `clamp(0.5, 1.0)`、把抽取到的信号交给该函数（E.9-① 已登记）。**I-C 未落地时**：函数与判据可独立判过/判败；「活体上 `<0.5` 可达」判 **not_measured（原因：I-C 依赖未落地）**，不进 C1 的失败判据 |

### C2 合并/去重：有界候选集 + 具名判据 + 可审计决策

| 项 | 内容 |
| --- | --- |
| metric | (a) 合并决策是否有**具名候选集与决策记录**（`memory_diffs.op=merge_judged`：`candidates=N, rule=..., verdict=...`）；(b) 在**带标签语料**上的召回与误并；(c) 候选面是否**有界**（不再全表扫） |
| baseline | 判据 = 手写词表 `judge()`（28+21 项，无语料支撑）；候选面 = 同 store+namespace **全部** live 行（无 LIMIT/无阈值，`distill.rs:436-438`）；实测 live 2297 对 Mergeable **0**、Same **0**（A.9）；而余弦最高的 6 对（0.9827–0.9884）逐字是同一事实 ⇒ 真改写上召回 **0**；绝对余弦阈值不可用（≥0.86 放行 45.19%） |
| target | ① 候选集 = 同 (store,namespace) 内按余弦 **top-K，K=3**（标定见下）且有 `candidate_source=embedding`；② 决策 = `judge()` 判 Mergeable 时直接合并（高精度路径），否则**只在 `cos ≥ τ_scope` 时**允许一次具名判定并把结果写审计；`τ_scope` **必须由该 scope 的分布导出**（今天：中位 0.85 / p99 0.9525 ⇒ τ 取 p99 量级，不许写死 0.90）；③ 判据：带标签语料（≥20 正例 = 人工确认的同事实改写、≥20 负例 = 极性/数值/主体不同）上，**误并 = 0**，召回 ≥ 0.7；④ `memory_diffs` 增加 `merge_judged` op |
| 复现命令 | 探针 §A1/§A2（候选集与分布）+ `cargo test -p ruagent-memory dedupe` + `python -c "... ;print(c.execute(\"SELECT op,COUNT(*) FROM memory_diffs GROUP BY op\").fetchall())"` |
| 标定读数 | ① 移植判据复现 crate 向量 **22/22**（判据本身可信）；② top-K 候选集在「cos≥0.95 的 24 对」上 K=1→21/24、K=2→23/24、**K=3→24/24**（有界候选面不丢近同义对）；③ 余弦分布（中位 0.85 / p99 0.9525）给出 τ 的量级，证明「写死 0.90 会放行 15.93% 的配对」 |
| 失败判据 | 若错并 >0，或语料召回 <0.7，或候选面仍是无界扫描 ⇒ 未达成 |
| 交付与责任（R-3） | **jointly-delivered**。mem-core 半边 = `dedupe.rs` 的 `merge_decision`/`MergeConfig`/`MergeVerdict` + `query.rs` 的候选行查询 + `write.rs` 的 `merge_judged` 审计 + `memembed.rs` 的候选集余弦（R-1 后归我）；graph / I-C 半边 = `distill.rs:436-448`（候选行读取）与 `610-616`（`mergeable_target` 改调 `merge_decision`）。**I-C 未落地时**：判据、带宽、`merge_judged` 可独立判；「在 distill 写入路径上生效」判 **not_measured（原因：I-C 依赖未落地）**，且该半边未落地时本目标**不允许判过** |
| 待补（不让分布读数当结论） | 带标签语料今天**不存在**：它必须**先造出来再测**（≥40 对），本条在语料落地前只算「判据与带宽已定」，不算已验证（§7.140） |

### C3 衰减与巩固（使用状态 → 排序；不做自动删除）

| 项 | 内容 |
| --- | --- |
| metric | (a) `memories` 是否记录「被用上」（`access_count`,`last_used_at`）；(b) 排序是否含**指数衰减 + 强化**；(c) 注入内容是否**只受排序影响**（不改内容） |
| baseline | 使用类列 **0/7**；读路径 `ORDER BY updated_at DESC`（`query.rs:49`）；注入组选择自述「the N most recent, not the N most relevant」（`inject.rs:144-147`）；`recall_log` 651 行证明召回在发生，但**没有一行被写回** |
| target | ① 迁移加 `access_count INTEGER NOT NULL DEFAULT 0`、`last_used_at TEXT`（**I-SCHEMA**，见 E.3）；② 注入/召回命中后写回（单写者 actor）；③ 排序 `score = 融合分 + λ·e^{-Δt/S}`，`S = 1 + access_count`；④ 判据：两条内容相同、一条 `access_count=5,last_used_at=now`、一条 `access_count=0,last_used_at=now-30d` 的记忆，在固定查询下前者排在后者前；⑤ `access_count` 更新**不改变返回内容**（同一查询的 content 集合逐字节相同） |
| 复现命令 | `cargo test -p ruagent-memory ranking` + `python -c "... ;print(c.execute('SELECT COUNT(*),SUM(access_count>0) FROM memories').fetchall())"` |
| 标定读数 | 列普查 0/7 + `updated_at` 排序（A.8）是「今天不可能有衰减」的充分证据；`recall_log` 651 行是「事件密度足够，值得写回」的证据（粗仪器，仅说明量级） |
| 失败判据 | 若写回改变了返回内容（回归），或 `access_count` 在 unix 侧更新失败但排序照旧 ⇒ 未达成 |

### C4 会话关闭的巩固/反思（产出可复用结论 + 可归因）

| 项 | 内容 |
| --- | --- |
| metric | (a) 一次成功蒸馏是否产出 `kind='run_turn'` 的 episode 与指向它的记忆行；(b) 是否有一条**跨行巩固**入口（把多条旧记忆综合成一条带引用的结论）；(c) 产出率变化是否**可归因**（prompt 指纹） |
| baseline | `run_turn` episode **0**；live `distilled=true` **0/157**；`distill_log` 33 行/85 记忆，最后成功 2026-09-26T10:28:22Z；无跨行巩固流水线；`distill_log` 无 prompt 版本列 |
| target | ① 蒸馏成功后 `SELECT COUNT(*) FROM episodes WHERE kind='run_turn'` **>0**，且对应记忆行 `distilled=true` 有正例；② 新增巩固入口（具名函数 + 审计 op `consolidate`），产物必须带来源引用（episode id 或记忆 id 列表）；③ `distill_log` 记 prompt 指纹（E.7，**I-C/I-SCHEMA**）；④ 判据：在一条固定 transcript 上跑一次，读数为「1 个 run_turn episode + ≥1 行记忆 + 0 个无 provenance 的产出」 |
| 复现命令 | `python -c "... ;print(c.execute(\"SELECT kind,COUNT(*) FROM episodes GROUP BY kind\").fetchall())"` + `python -c "... ;print(c.execute(\"SELECT COUNT(*) FROM memories m JOIN episodes e ON e.id=m.source_episode WHERE e.kind='run_turn'\").fetchall())"` + 蒸馏端到端（I-C 提供） |
| 标定读数 | 「0 个 run_turn episode」+「`distilled` 布尔 0 正例」（A.5）是**空集上的读数**：它证明今天没有正例，**不证明**将来会有（§7.84）—— 所以 target 必须由一次真实蒸馏来闭合 |
| 失败判据 | 若蒸馏成功但 episode kind 不是 `run_turn`（标记错位），或产出记忆的 `source_episode IS NULL`（provenance 丢失）⇒ 未达成 |
| 交付与责任（R-3 + R-2） | **jointly-delivered**，且**含一条已移出项**。mem-core 半边 = `consolidate.rs` 的巩固入口 + 审计 `op='consolidate'` + `query.rs::session_memories`；graph / I-C 半边 = 蒸馏产出 `run_turn` episode（`distill.rs:385-405`）与 prompt 指纹（E.7）；**`distilled` 布尔那半边已移出 → `## C-INT` C-INT-2（owner=INT/t19，验收 t20/t21）**。I-C 未落地时：「入口 + 审计 + 幂等」可独立判；「一次真实蒸馏产出正例」判 **not_measured（原因：I-C 依赖未落地）** |

### C5 遗忘与可审计（内容真消失 + 审计可证明 + 残余面可回答）

| 项 | 内容 |
| --- | --- |
| metric | (a) `purge` 后内容在所有 memory 侧面上是否消失（`memories` / `memories_fts` / `embedding`）；(b) 审计是否只留 id+时间+reason（不留内容）；(c) 是否存在一个**残余面清点**读数（为一条内容列出全部派生面：episodes / knowledge / wiki） |
| baseline | purge 23 次全落在 live 行；tombstoned 0；`memory_diffs` 保留 id+op+reason（A.6）；`purge_memory` 真删行、FTS 由 `memories_ad` 触发器跟随、embedding 随行消失（`lifecycle.rs:182-247`）；**没有**残余面清点（`episodes.content` 仍是原始 transcript） |
| target | ① 判据：purge 一条内容后，`SELECT COUNT(*) FROM memories WHERE content_hash=<h>` = 0、`memories_fts MATCH <term>` = 0（同一 SQL 事务外读）；② 新增 `forget_report(content_or_id)` 只读读数：列出 `episodes`/knowledge/wiki 侧仍含该内容的对象（**只报，不自动删**）；③ 审计自述：`purge` 的 reason 里若清了 supersede 链，必须带上数量（今天已做，`lifecycle.rs:227-231`，保留） |
| 复现命令 | 临时 root 上 `cargo test -p ruagent-memory lifecycle`（禁止对活库写入）；活库只读 `python -c "... ;print(c.execute('SELECT COUNT(*) FROM memories WHERE deleted_at IS NOT NULL').fetchall())"` |
| 标定读数 | 今天 `deleted_at IS NOT NULL = 0` 且 `purge=23` ⇒ 「软删/真删两条路都通、且今天没有悬挂 tombstone」；审计 op 词表完整（7 种，A.6） |
| 失败判据 | 若 purge 后 FTS 仍命中，或审计里出现被删内容，或 `forget_report` 在**存在**派生面时返回空 ⇒ 未达成 |
| 残余面形状（R-6） | `target ②` 的**接口形状**写在 **`## C5-D`（non-landing）**：类型与语义已定，实现/schema 未定；**本节的验收不依赖它**（`target ①/③` 今天就能判）。U-3（knowledge 五面 + `ResidualHit.origin/path`）已按 recall 回信收敛；U-4（wiki 面）待回信 |

### C6 注入面：每条 render 都有「截断/丢块」的可读账目

| 项 | 内容 |
| --- | --- |
| metric | (a) tagged render 中带可见截断标记的比例；(b) 丢块是否总有计数（不许静默）；(c) 是否有 `<graph>` 块（图证据进注入，**依赖 I-C**）；(d) `total` 上界永不被打破 |
| baseline | tagged 19 个 render：18 带截断标记、**0 带丢块通知**、0 逼近 4096（max 2458/余量 1638）；全 80 事件带丢块通知 1；proptest `never_exceeds_total` 在 crate 级守着上界（A.1/A.10） |
| target | ① 契约层单测：任何 `render_context` 输出满足 `chars ≤ total`，且**丢失时必有计数**（已有，保留）；② 新增 `TAG_GRAPH` 与 `tag_rank` 位置（D.3），并要求一条**第三层读数**：某次运行的 `context_injected` 里出现 `<graph>`；③ 判据：`tagged` 类 render 里「有块被丢」的那次必须出现 `<context_budget>` 或 `… [+N dropped]`（今天 0 例 ⇒ 需要造一次预算绑定的运行） |
| 复现命令 | `cargo test -p ruagent-memory inject` + 探针 §A3（普查 + 分类） |
| 标定读数 | 19 个 tagged render 的字符分布（108/1660/2458）证明「今天预算从未逼近上界」⇒ 丢块通知的 0 例**不是缺陷的证据，而是没有样本**（粗仪器，A.10 已写） |
| 失败判据 | 若出现「render 超 4096」或「丢了块但无任何计数」⇒ 未达成（前者是硬违约） |
| 交付与责任（R-3） | **jointly-delivered**。mem-core 半边 = `inject.rs` 的 `TAG_GRAPH`/`tag_rank`/契约单测 + `memembed.rs` 把图证据转成 `ContextItem`（R-1 后归我）；graph / I-C 半边 = 图证据的**内容**（`crates/graph` + `distill.rs` 产出）。I-C 未落地时：①（上界/计数）与②的契约半边可独立判；②的「`context_injected` 里出现 `<graph>`」那条第三层读数判 **not_measured（原因：I-C 依赖未落地）** —— 契约在、生产者不在，不能算过 |

### C7 诚实失败：未知输入必须 400/报错，不许静默降级

| 项 | 内容 |
| --- | --- |
| metric | 未知 `store` 的写入/读取是否**拒绝**（而非回落 Observation），未知 namespace 是否拒绝 |
| baseline | `memory_list` 已 400（`api.rs:3029-3039`）；**`memory_write` 仍静默回落 Observation**（`api.rs:3705-3710`，MCP 走它）；`query.rs:11-16 row_to_memory` 对未知 store 字符串静默回落 |
| target | ① `memory_write` 与 MCP `memory_write` 对未知 store 一律 **400 + 有效值列表**（复用 `parse_store`）；② `row_to_memory` 返回 `Result`，未知 store 变成**显式错误**（或 `MemoryStore::try_parse`），调用方逐个处理（这是一次**破坏性 API 变更**，D.6 已列）；③ 判据：`store=bogus` 写 → 400，且 `memory_diffs` **不新增行**（拒绝不是一条静默写入） |
| 复现命令 | `cargo test -p ruagent-daemon api`（不是本单范围，登记给 I-B 的实现条目）+ 只读核对 `api.rs:3029-3039` |
| 标定读数 | `memory_diffs.reject = 15` 证明「拒绝会被审计」这条通路已经在用；`memory_list` 的 400 是同一形状的已完成先例 |
| 失败判据 | 若 `store=bogus` 返回 200 ⇒ 未达成 |
| 归属变更（R-1/R-2，2026-09-27） | **本行已拆分，原文保留不动**：属于 `crates/daemon/src/api.rs` 的那半边（`memory_write` 400 + MCP 透传）**移出 → `## C-INT` 的 C-INT-1（owner=INT/t19，验收 t20/t21）**；留在 C7 的只有 **mem-core 半边**：`crates/memory/src/query.rs` 的 `row_to_memory` 返回 `Result`（破坏性变更 B-6）。上面的 metric/baseline/target/标定读数**未改一个数**，它们对两半边都成立 |

---

## C-INT 集成面目标（owner=**INT / t19**；验收在 **t20 / t21**；本单不改 `api.rs` 一行）

**为什么单独成节（R-2，captain 裁决 2026-09-27）**：这两条的目标文件是 `crates/daemon/src/api.rs`，按 closure §5「一个文件一个写者」归 **INT/t19**；mem-core 只负责把它们**写清**（本节）并保证 C 节不再把它们算成自己的验收面。两行都是从 C 节**移出**的，C4/C7 的原行保留并加了指向本节的标注（未删、未改数）。

### C-INT-1 `memory_write` 未知 store 必须诚实 400（不许静默回落 Observation）

| 项 | 内容 |
| --- | --- |
| owner | INT / t19（目标文件 `crates/daemon/src/api.rs`；MCP 侧 `crates/mcp/src/lib.rs` 透传） |
| 来源 | 原 C7 的 api.rs 半边（R-2 移出）；任务单已知缺陷 D-4 的未修部分 |
| metric | 未知 `store` 的**写入**是否被拒绝（HTTP 与 MCP 两条入口一致），且拒绝时**不产生任何写入副作用** |
| baseline | `memory_write` 仍 `_ => MemoryStore::Observation`（`api.rs:3705-3710`）⇒ `store=bogus` 走通、落库为 observation；MCP `memory_write`（`crates/mcp/src/lib.rs:81-90`）**根本不传 confidence/store 校验**，直接吃这条回落。对照：`memory_list` 已经是 400（`api.rs:3029-3039 parse_store`） |
| target | ① `memory_write` 复用 `parse_store`，未知值 → **400 + 有效值列表**（`profile/observation/procedure/lesson`）；② MCP 工具把 4xx 原样透出（不许吞成 200）；③ 判据：`store=bogus` 写 → 4xx，且 `SELECT COUNT(*) FROM memory_diffs` **前后不变**（拒绝不是一次静默写入） |
| 复现命令 | `curl -s -o - -w '%{http_code}' -X POST <daemon>/api/v1/memory/write -H 'content-type: application/json' -d '{"store":"bogus","namespace":"user","content":"t2-cint-probe"}'`。**注意这条命令会写库** ⇒ 它不属于「只读取证」，必须由 t19 **在临时 root 上**做，不许对活库（pid 79984 / 127.0.0.1:8787）跑；本单只做只读核对：`api.rs:3029-3039` 与 `3705-3710` |
| 标定读数 | `memory_diffs.reject = 15`（A.6，21:39:58+08:00）⇒ 「拒绝会被审计」这条通路已经在用；`memory_list` 的 400 是同形状的**已完成先例** ⇒ 目标不是新机制，而是一次复用 |
| 失败判据 | 若 `store=bogus` 返回 200、或返回 4xx 但 `memory_diffs` 多了一行（说明仍写了一次）⇒ 未达成 |

### C-INT-2 `distilled` 布尔按 episode kind 派生

| 项 | 内容 |
| --- | --- |
| owner | INT / t19（目标文件 `crates/daemon/src/api.rs`：`distilled_ids`/`with_distilled`） |
| 来源 | 原 C4 的「`distilled=true` 有正例」那半边（R-2 移出）；任务单已知缺陷 D-5 |
| metric | 面板/接口拿到的 `distilled` 是否**只**由 `episodes.kind='run_turn'` 派生（不是「有没有 episode」，也不是「`distill_log` 里有没有这个 session」），且在数据落地后有可读的正例率 |
| baseline | 机制**已实现**（`api.rs:3794-3831`，t350）：`DISTILLED_EPISODE_KIND="run_turn"` + join + `with_distilled` 输出布尔；但 `run_turn` episode = **0**、`distill_log` 最后一次成功 2026-09-26T10:28:22Z ⇒ live 157 行 **0 正例**。`source_episode IS NOT NULL` 的 162/163 是**另一个更宽的标记**（156 行指向同一条迁移 episode），不能用它替代（§7.140） |
| target | ① 派生规则冻结（只允许 episode kind；禁止用 `distill_log.session_key` 反查 —— 那会把 §7.140 的宽标记引回来）；② 形状判据：造一次真实蒸馏（依赖 I-C/t9）后 ① `SELECT COUNT(*) FROM episodes WHERE kind='run_turn'` >0，② `SELECT COUNT(*) FROM memories m JOIN episodes e ON e.id=m.source_episode WHERE e.kind='run_turn'` = 该次蒸馏写入的记忆数，③ 接口对这些行 `distilled=true`、对那 156 行迁移行 `distilled=false`（**宽窄要在数据落地后重量一遍**，§7.140） |
| 复现命令 | `python -c "... ;print(c.execute(\"SELECT kind,COUNT(*) FROM episodes GROUP BY kind\").fetchall())"`（只读）+ 一次真实蒸馏 + `GET /api/v1/memory/list`（**由 t19/t20 在临时 root 取证**；本单不调活库 recall） |
| 标定读数 | 「`run_turn`=0、live 正例=0」（A.5，21:39:58+08:00）是**空集读数**：证明今天没有正例，**不证明**将来会有（§7.84）⇒ 所以判据必须由一次真实蒸馏闭合，不能靠读代码判过 |
| 失败判据 | 若正例仍为 0（I-C 未落地 ⇒ 判 **not_measured**，不是未达成），或那 156 行迁移行被标成 `distilled=true`（标记错位），或出现任何按 `distill_log` 反查的写法 ⇒ 未达成 |

---

## C5-D 残余面清点接口草案（`forget_report`，**non-landing**，R-6）

**状态**：本节是**形状草案**，不是冻结接口，也不进 I-B 的验收面。
- **non-landing**：I-SCHEMA(t6) 把表/列定下来之前**不写实现、不写迁移、不预填任何数字判据**；C5 的验收仍以「memory 侧三面为 0 + 审计自述」为准（那半边今天已可判）。
- **待冻结时机**：t6 定表 **且** recall/wiki 回信收敛形状之后，由 I-B 把它升级成 `[冻结]` 并在 D 节登记；在那之前它只表达「未知的形状」。
- 本节只有两段：**形状**（类型 + 语义）与**待定项**。SQL、索引、删除语义留到 t6/I-B 落地时写。
- **［R-6 收敛追加，2026-09-27，原文保留］** recall 回信到达后本节多了第三块「**已到的读数**」：因为 U-3 的「未知」已经变成了**带来源与时间点的读数**，把它压进「形状」或「待定项」都会让读数看起来像预期。原两段不动。

### 形状（类型与语义；纯形状，无实现）

**［收敛 B，2026-09-27，recall 第二次回信之后］** 下面的代码块是 **v1.5**（含 `origin`/`path`），**保留不动**；收敛后的 **v2** 紧随其后（`ResidualOrigin` 枚举、`ResidualCount`（`truncated`/`total`）、`LocalResidual`）。两块的差别在 v2 的抬头逐条列出。

```rust
// ── v1.5（保留；已被 v2 取代，但原文不删）──────────────────────────────
/// 一个「残余面」：某条内容在被遗忘后仍可能存在的派生位置。
/// 语义：只用于**报告**；任何调用方都不许据此自动删除（删除仍走 delete/purge + 审计）。
pub struct ResidualHit {
    /// 面名（闭集，见 ResidualSurface）。
    pub surface: ResidualSurface,
    /// 面内的稳定标识（memory id / episode id / document 名 / chunk_id / wiki slug / build id）。
    /// 语义：**标识，不是内容** —— 报告里永不复述被遗忘的文本。
    pub key: String,
    /// 这条读数是怎么来的：`file:line` 或 API 路径（出处要连坐标，§7.41）。
    pub source: &'static str,
    /// **[收敛，2026-09-27，recall 回信]** 这条命中的载体：`"db" | "file" | "both"`。
    /// 语义：知识库一条字节有 4 个派生物面（磁盘 md / `chunks.content` / `chunks_fts` / LanceDB 向量），
    /// 「删了文件但 DB 还在」这种半清状态只有这个字段能让它可见。
    pub origin: &'static str,
    /// **[收敛]** `origin` 含 `"file"` 时必填的磁盘路径。
    /// 语义：**不是新能力** —— `documents.source` 今天 934/934 非空，路径可反推。
    pub path: Option<String>,
}

pub enum ResidualSurface {
    Memories,      // 本 crate：memories 行（superseded / tombstoned 都算残余）
    MemoriesFts,   // 本 crate：memories_fts 的命中
    Episodes,      // 本 crate：episodes.content（原始 transcript）
    // ── 以下五类都是「知识库一条字节」的派生物面，由 recall 在 2026-09-27 回信里逐面点名；
    //    它们**不能合成一个面**，因为同一个 needle 在每一面的清点结果不同。
    KnowledgeFile,     // 外部（I-A）：<root>/knowledge/<name>.md —— 声明的真相源
    KnowledgeDocument, // 外部（I-A）：`documents` 行（整篇 sha256 在这里）
    KnowledgeChunk,    // 外部（I-A）：`chunks.content`（今天**没有** content_hash）
    KnowledgeChunkFts, // 外部（I-A）：`chunks_fts`（触发器维护的词面索引）
    KnowledgeVectors,  // 外部（I-A）：LanceDB `knowledge_chunks`（按 chunk id）
    // ── wiki 面（I-D）按载体强度拆开（［收敛 C，2026-09-27，wiki 回信］）──
    WikiChunk,     // 逐字正文（**DB 影子**：documents('wiki/<slug>') + chunks.content）—— 能真回答「这段内容还在不在」
    WikiPageHash,  // 只有整页 sha256（`wiki_page_hashes`）—— **答不了任何 substring 问题**（见语义第五条）
    WikiPlan,      // planner 的 plan 逐字文本（`wiki_builds.plan_json`）—— 说的是「计划里」而不是「页面里」
    WikiBuildPage, // `wiki_build_pages` 行 —— **无正文**，只能报 slug/action/status
    WikiPage,      // [v1.5 保留] 单一 wiki 面；已被上面四个载体取代（原文不删，保留为历史）
}

pub struct ForgetReport {
    /// 被问的对象：内容哈希（64 hex）——不是内容本身。
    pub content_hash: String,
    /// 本 crate 能直接回答的面，逐面若干条（**只报出现，不判定达标**）。
    pub local: Vec<ResidualHit>,
    /// 需要外部回答的面：每条要么是一个读数，要么是一条「未取得」的显式记录。
    pub external: Vec<ExternalResidual>,
}

/// 「问」的形状：只表达请求，**不代表对方已实现**（§7.130：预期必须标成预期）。
pub struct ExternalResidual {
    pub surface: ResidualSurface, // KnowledgeFile | KnowledgeDocument | KnowledgeChunk | KnowledgeChunkFts | KnowledgeVectors | WikiPage
    pub owner: &'static str,      // "I-A/recall" | "I-D/wiki"
    pub status: ExternalStatus,
    pub source: &'static str,     // 对方给的查询坐标，或 "未取得"
}

pub enum ExternalStatus {
    Readout(usize),             // 对方给了读数：该面上仍有几处命中（数，不是内容）
    NotAvailable(&'static str), // 未取得 + 原因（例如「t7 之后给」）
    Unknown,                    // 还没问过 / 还没回
}
```

```rust
// ── v2（收敛，2026-09-27；recall 第二次回信 + wiki 回信后）────────────────
// 与 v1.5 的差别，只有三处，都由 recall 的设计取舍驱动：
//   1) origin 从 &'static str 变成**枚举** ResidualOrigin —— 取值集合必须与 I-A 侧的
//      ResidualOrigin 逐字相同（Db|File|Both），因为两套类型各自定义、不跨 crate 依赖，
//      字段名与取值集合就是唯一能漂移的地方（I-A 已在 gen2-recall-spec.md 的 D.4
//      登记了一条漂移自检，归 t7 的测试；我这边承诺：**不先告诉 recall 就不放宽取值集合**）。
//   2) 每个面的读数不再是裸 usize，而是 ResidualCount —— 因为「命中几处」必须能说清
//      「这是全部还是被 limit 截断」。truncated 的判据**必须是 limit+1 多取一行**，
//      不能是 hits == limit（恰好等于 limit 时两者无法区分，那正是假通过再深一层）。
//   3) total 是 Option<usize>，因为 COUNT 要再扫一遍全表（代价读数见下）；
//      **None ≠ 0** —— 没要 exact_total 就不许把 None 读成「没有残余」。

/// 命中载体。**取值集合与 I-A 侧的 ResidualOrigin 必须逐字相同**（两个 crate 各自定义，不互相依赖）。
pub enum ResidualOrigin { Db, File, Both }

/// 一个面的读数。**没有这个结构，「清点完成」就只能靠猜。**
pub struct ResidualCount {
    pub hits: usize,          // 该面命中的条数（可能是被 limit 截断后的条数）
    pub truncated: bool,      // 判据 = limit+1 多取一行后仍有一条 ⇒ true（零额外扫描）
    pub total: Option<usize>, // 只有显式 exact_total=true 才 Some；None ≠ 0
}

/// 本 crate 的面（memories / memories_fts / episodes）。
pub struct LocalResidual {
    pub surface: ResidualSurface,
    pub count: ResidualCount,
    pub sample: Vec<ResidualHit>, // 至多 limit 条**标识**（永不含被忘文本）
    pub source: &'static str,     // 该读数的查询坐标（file:line 或 SQL）
}

pub struct ForgetReport {
    pub content_hash: String,        // 被问的对象：hash，不是内容
    pub local: Vec<LocalResidual>,
    pub external: Vec<ExternalResidual>,
}

/// 「问」的形状：只表达请求，**不代表对方已实现**（§7.130）。
pub struct ExternalResidual {
    pub surface: ResidualSurface, // KnowledgeFile | KnowledgeDocument | KnowledgeChunk | KnowledgeChunkFts | KnowledgeVectors | WikiChunk | WikiPageHash | WikiPlan | WikiBuildPage
    pub owner: &'static str,      // "I-A/recall" | "I-D/wiki"
    pub status: ExternalStatus,
    pub source: &'static str,     // 对方给的查询坐标，或 "未取得"
}

pub enum ExternalStatus {
    Readout(ResidualCount),     // 对方给了读数（含 truncated/total；total=None 表示没要 exact_total）
    NotAvailable(&'static str), // 未取得 + 原因（例如「I-A 承诺 t7 后提供」「只有哈希，需重读文件再哈希」）
    Unknown,                    // 还没问过 / 还没回
}

// ── I-A 侧已确认的形状（**对方的**，来自 recall 2026-09-27 回信；已落在
//    docs/design/reviews/gen2-recall-spec.md 的 D.2）。抄在这里只为让两套类型可对照，
//    本规格**不**依赖它、也不在 crates 里引用它。t7 之前它是承诺，不是能力。 ──
// pub async fn residual_scan(&self, needle: &str, limit: u32, exact_total: bool) -> Result<ResidualPage, KnowledgeError>;
// pub struct ResidualHit { document_id: i64, name: String, chunk_id: Option<i64>, origin: ResidualOrigin, path: Option<PathBuf> }
// pub struct ResidualPage { hits: Vec<ResidualHit>, truncated: bool, total: Option<usize> }
// pub fn document_path(&self, name: &str) -> std::path::PathBuf;   // <root>/knowledge/<name>.md

// ── wiki 面：按**载体强度**拆开（［收敛 C，2026-09-27，wiki 回信］）。
//    三条腿现在就能只读跑，但证据强度**不等价**，混在一个 `WikiPage` 里会产生假阳性：
//    (a) 逐字残余（强）：`chunks c JOIN documents d ON d.id=c.document_id
//         WHERE d.name LIKE 'wiki/%' AND c.content LIKE '%'||?1||'%'`
//    (b) 内容寻址（弱）：`wiki_page_hashes WHERE hash = ?1` —— **只有哈希相等**，
//        回答不了任何 substring 问题 ⇒ 它不是「内容残余」的证据（见语义第五条）
//    (c) 计划残余（强，但语义是「计划里」）：`wiki_builds WHERE plan_json LIKE '%'||?1||'%'`
//    另：`wiki_build_pages(build_id, slug, action, status, error)` **无正文** ⇒ 只能报 slug/action。
//    ──
```

**key 的约定（［收敛 C 追加］可取回性要看得出来 —— 三种 key 的取回能力不同）**

| key 形状 | 例子 | 怎么取回 |
| --- | --- | --- |
| `wiki/<slug>#c<chunk_id>` | `wiki/autohotkey-v2#c3` | 直接喂 `GET /api/v1/knowledge/expand/{chunk_id}` |
| `build/<id>` | `build/6` | **只能用** `GET /api/v1/knowledge/wiki/builds/{id}` 的 `plan` 字段（**不是** expand） |
| `hash:<sha256>` | `hash:db3d84ad85…` | 不是一条可取回的对象：要与**重读文件后的哈希**比对才成为证据（语义第五条） |

**语义三条（不写实现）**
1. `forget_report` 是**只读**：不发删除、不改任何行、不写 `memory_diffs`。
2. 报告里**永不出现被遗忘的文本**（只有 `content_hash` 与 `key`）—— 否则「可证明的遗忘」自己就成了新的泄露面。
3. `ExternalStatus` 是**三态而不是布尔**：`Readout / NotAvailable / Unknown` —— 「没有残余」与「没问过」必须可区分（§7.87 家族：永远通过的检查与永远失败的检查是同一类缺陷）。

**语义第四条（［收敛 B 追加，2026-09-27］recall 的设计取舍，我采纳并把它抬成契约）**
4. **「截断」与「总计」必须可辨，且默认不可得**：
   - `truncated` 的判据只能是 **`limit+1` 多取一行**，不许写成 `hits == limit`（恰好等于 limit 时两者同形 ⇒ 假通过再深一层）；
   - `total: Option<usize>` 只有在**显式 `exact_total=true`** 时才有值 ⇒ **`None ≠ 0`**：任何消费者（包括我自己的 `forget_report`）都不许把 `None` 读成「没有残余」。我的调用点若需要「确切的 0」，就必须显式请求 total；
   - 代价（读数见「已到的读数」）：**每一次 LIKE 扫描都是一次全表扫**；多要 total = 多扫一遍，而 `limit+1` 那条探针与单纯 `LIMIT` 同价。

**漂移自检（recall 的 D.4 登记，t7 的测试）**：两套 `ResidualOrigin` 的取值集合与 `ResidualHit` 的字段名必须一致（`Db|File|Both`）；**我承诺：不放宽取值集合、不改字段语义，除非先告诉 recall**。本规格 v2 的 `ResidualOrigin` 与 I-A 的逐字相同。

**语义第五条（［收敛 C 追加，2026-09-27］wiki 的证据强度取舍，我采纳并抬成契约）**
5. **同一段内容在不同载体上的证据强度不等价，报告里必须能看出来**：
   - 逐字载体（`…Chunk` / `…Plan`）= 真读数；
   - **哈希载体（`WikiPageHash`）不能计为「内容残余」**：它答不了 substring 问题 ⇒ 要么报 `NotAvailable("只有哈希，需重读文件再哈希")`，要么由调用方重读文件、重算哈希后再升级成逐字读数；
   - **`WikiBuildPage` 无正文** ⇒ 只能报 slug/action，不得当内容命中；
   - 把哈希命中当内容命中（或反过来）**就是假阳性与假阴性各一次**，而且都长得像「查过了」。
6. **wiki 面的 DB 影子会自己消失（［收敛 C］）**：`Knowledge::scan` 会删掉「文件已不在」的 file-backed 行（`crates/knowledge/src/files.rs:290-309`，`report.removed`）⇒ wiki 面的「没有残余」只在**一个扫描周期内**成立。因此 wiki 面的读数**必须带自己的时刻**，并把「影子可能已被扫描清掉」写进 `source` 或 `NotAvailable` 的原因里（见 U-7）。
   - **［wiki 回信追记］这条已成 I-D 规格里的一条判据**：wiki 把它采纳为「**新鲜度第三态**」的成因之一 —— `freshness = "unknown"` 必须存在（KB 里查不到来源 row 时），且**不许被静默算成 `false`**；其冻结形状见 **`gen2-wiki-spec.md` 的 D.5**：`WikiPageInfo.freshness: "fresh"|"stale"|"unknown"` + `PageFreshness.stale: Option<bool>`。本规格引用它时**只引这两行**，不重述其语义（避免两处口径漂移）。

### 待定项（把未知显式化；t6 定表 + 回信收敛前不冻结）

| # | 待定 | 为什么现在不能定 | 谁能定 |
| --- | --- | --- | --- |
| U-1 | `episodes` 是否要能按「来源记忆」反查（今天 `episodes` 只有 `content_hash UNIQUE`，一条 transcript 与一条记忆不是同一串字节，喂记忆的 hash 查不到对应的 episode） | 需要 I-SCHEMA 决定 `episodes` 是否加派生/来源列；今天只能用 hash 相等做**粗判**（会漏） | I-SCHEMA(t6) + I-B(t8) |
| U-2 | `MemoriesFts` 这一面的查询形态（FTS5 `MATCH` 要词面，不能喂 hash） | 取决于「谁在问」给的是 hash 还是原文；只给 hash 时这一面只能退回 id 集合的反查 | I-B(t8) |
| U-3 | **已收敛（2026-09-27，两次回信）**：knowledge 侧由 I-A 提供 `residual_scan(needle, limit, exact_total) -> ResidualPage` 与 `document_path(name)`（按**原文子串**匹配，不是 token）；**时间承诺：t7 之后给**（t1 是规格单、禁改代码）。今天能拿到的只有裸 SQL（`chunks.content LIKE`）与 `documents.content_hash`（整篇粒度）。第二次回信把形状定到「截断/总计是一等字段」⇒ 本节的 v2 与语义第四条即其收敛结果 | 见下「已到的读数」；**不要用 `search()` 的命中形状做清点**（它是有 LIMIT/阈值/分词预处理的**排序结果** ⇒ 会给出「前 8 条里没有 ⇒ 清干净了」这种假通过） | recall(t7) |
| U-4 | **已收敛（2026-09-27，wiki 回信）**：wiki 页**磁盘是真相源、DB 是影子**（`documents(name='wiki/<slug>')` + `chunks` + `chunks_fts` + LanceDB），三条只读腿的 SQL 见上；**页级出处只到文档粒度**（frontmatter `sources:`/`source_hashes:` + `build:`），**chunk 级出处全仓不存在**（wiki 全树 grep 的结论 + 我复核的表结构）。这一面按**载体强度**拆成四个 `ResidualSurface`（`WikiChunk`/`WikiPageHash`/`WikiPlan`/`WikiBuildPage`） | 见下「已到的读数（wiki）」；chunk 级出处是 **I-D 在 R-D 里定的「引用可验证」闸门**（它的 E 节会向 I-SCHEMA 提 DDL），**不是**我这边的目标 | wiki(t10) |
| U-7 | wiki 面的「没有残余」只在**一个扫描周期内**成立（DB 影子会被 `files.rs:290-309` 的 `report.removed` 清掉）⇒ 报告要不要带「有效期/时刻」字段，以及要不要**主动重读文件**代替影子 | 取决于 t10 落地后的扫描周期与 API 形状；今天只能把「时刻」写进 `source` | I-D(t10) + I-B(t8) |
| U-8 | 页哈希（`wiki_page_hashes`）今天与页面文件**不保证一一对应**（实测 `doctor-probe`：有哈希行、无 `documents` 行、无磁盘文件）⇒ 哈希面要不要单独成「孤儿哈希」一栏 | **已裁决（wiki，2026-09-27）：bug，不是允许态** —— 三条证据：① `clear_page_hash` 只在 delete 分支被调（`crates/daemon/src/wiki.rs:1024`）；② 「文件在别处消失」无人对账（`files.rs:290-309` 删 document 行，而 `wiki_page_hashes` 不参与扫描）；③ `wiki/doctor-probe` 在 `documents` 与磁盘都不存在而 hash 行仍在（非暂时不同步）。**收口归 I-D 的产品路径（其 E10，随构建跑），不放一次性迁移**（closure §7.75）。读数：现场 5 行里 **1 行泄漏 ⇒ 目标 0 行**（我 22:37:08 复核仍为 1） | I-D(t10) |
| U-5 | 报告的**阈值/达标线** | 本单不预填数字判据（R-6）：今天无正例数据，写数字就是把预期当读数（§7.130） | t6 之后的 I-B + V-B |
| U-6 | 要不要一个「五面汇总」的统一入口，还是每面一个只读查询 | 取决于单一消费面的设计（HTTP/MCP/面板） | integ(t5/t19) |

**与 C5 的关系**：C5 的验收**不依赖**本节 —— C5 的 `target ①/③` 今天就能判；本节只是把 `target ②`（残余面清点）的**未知显式化**，避免它在没有形状的情况下被当成「已具备」。U-3 已按 recall 的回信收敛（读数见下）；U-4 仍待 wiki 回信。

### 已到的读数（recall 回信，2026-09-27；**预期与读数分开**，§7.130）

**来源（引用别人的读数必须带来源与时间点，§7.68）**：`agent_teams_send_message` ← recall，主题「[recall → mem-core] 残余面清点」，时间点由发信人写作 **2026-09-27T22:1x+08:00** —— **只给到「1x」这一级，我不替它补精度**（§7.68：读数带的是它自己的时刻）。下表把「recall 给的」与「我复核的」分成两列；**我复核用的时刻 = 2026-09-27T22:16:52.357696+08:00**（只读 `mode=ro` + 只读文件列目录，同一台机器）。

| 读数（对象集 + 采样面） | recall 给的（22:1x） | 我的独立复核（22:16:52） | 一致性 |
| --- | --- | --- | --- |
| 磁盘 markdown 与 `documents` 行一一对应 | 934 文件 = 934 行 | `<root>/knowledge/**/*.md` = **934**；`documents` = **934**（`chunks` 10765 行、934 个不同 document_id） | ✓ 两列相同 |
| `documents.source` 非空（⇒ 路径可反推，不是新能力） | 934/934 | `SUM(source IS NULL)` = **0** | ✓ |
| `documents.content_hash`（整篇粒度） | 存在，非空 | `SUM(content_hash IS NULL)` = **0**；列 = `[id,name,source,content_hash,chunk_count,created_at]` | ✓ |
| `chunks` 有没有 hash 列 | 没有（列 = id, document_id, idx, content, span_start, span_end, section_id） | `pragma_table_info('chunks')` = 同样 7 列；**任何含 "hash" 的列 = 0 个** | ✓ ⇒ 「按 sha256 查只能到 document 粒度」成立 |
| `chunks_fts` 由触发器维护 | `0006_knowledge.sql:25-30` | 同文件 19-30 行：`chunks_fts`（fts5, content='chunks'）+ `chunks_ai`/`chunks_ad` 两个触发器 | ✓ |
| 子串腿的 SQL 今天存在 | `crates/knowledge/src/store.rs:575-605`（`fts_like`） | 同位置：`SELECT id FROM chunks WHERE content LIKE ? ESCAPE '\' ORDER BY id LIMIT ?`，**私有 fn、只回 id、带 `leg_k` 上界** | ✓（并补一条：**LIMIT 使它不能直接当清点器** —— 见下） |
| LanceDB 向量面存在 | `<root>/data/lancedb/knowledge_chunks` | 路径存在（`knowledge_chunks.lance`） | ✓ |

**「已存在」与「承诺提供」必须分开（按 recall 自己的要求）**：
- **今天已存在（实测）**：裸 SQL `chunks.content LIKE`（`store.rs:575-605`，**私有且带上界**）· `documents.content_hash`（整篇）· `documents.source` → 文件路径。
- **承诺提供，尚不存在**：`Knowledge::residual_scan(needle, limit)` 与 `Knowledge::document_path(name)` —— 由 I-A 在 **t7 之后**交付（recall 2026-09-27 消息确认）。因此在 t7 之前，knowledge 那五面只允许记 `ExternalStatus::NotAvailable("I-A 承诺 t7 后提供")`，**不许写成 `Readout(0)`**（那正是「没问过」被当成「没有残余」的假通过）。
- **两条方法层面的约束（recall 给的，我采纳）**：① **不要**用 `search()` 的命中形状做清点（排序结果 + LIMIT + 阈值 + 分词预处理 ⇒ 假通过）；② needle 必须是**原文子串**匹配，而 CJK 的 token 腿不可靠（recall 给 t1 的普查：2 字汉字查询 63.20% 只能靠 LIKE）⇒ 清点用 LIKE，不用 MATCH。

**代价读数：`exact_total` 到底贵在哪（recall 第二次回信，2026-09-27）**

| 读数（对象集 + 采样面） | recall 给的 | 我的独立复核（**2026-09-27T22:23:50.723492+08:00**） | 说明 |
| --- | --- | --- | --- |
| 一次 `content LIKE` 全表扫（`chunks` 10765 行） | 约 **10.47 ms**（其 t1 A8，采样面见 gen2-recall-spec） | `COUNT(*) ... LIKE '%# AutoHo%'`：5 次 = [6.78, 6.83, 6.61, 6.94, 6.80] ms ⇒ **min 6.61 / median 6.80** | 同一台机器、只读 `mode=ro`、热缓存；**时间读数是粗仪器**（§7.118）：两方相差约 1.5× 属可预期，量级一致即可，「10.47 vs 6.8」不该被当成矛盾 |
| `LIMIT 51`（即 `limit+1` 截断探针）是否更贵 | —（其设计主张：零额外成本） | 同 needle **median 6.69 ms**（min 6.63） | ✓ 与 `COUNT` 同价（差异在噪声内） ⇒ **`truncated` 的 `limit+1` 判据确实零额外扫描** |
| 无命中 needle 是否更便宜 | — | median **6.63 ms** | ✗ 不便宜：LIKE 无索引，命中与否都要扫完 ⇒ 结构结论与 needle 无关 |
| 结构结论（供 C5-D 语义第四条） | 「`COUNT(*)` 要再扫一遍全表」 | **成立**：每次 LIKE 扫描 ≈ 6.6–6.9 ms（本机此刻），`limit+1` 不额外扫，`exact_total=true` = 再加一次同价扫描 | `chunks_fts` 行数 10765 与 `chunks` 行数相同（触发器同步的旁证） |

### 已到的读数（wiki 回信，2026-09-27；**预期与读数分开**，§7.130）

**来源与时间点**：`agent_teams_send_message` ← wiki，自报时间窗 **2026-09-27 21:33–22:30 +08:00**（活库只读 + **临时 root 冷启动 daemon 取证**，真 daemon pid 79984 未触碰）。我的独立复核时刻 = **2026-09-27T22:31:08.593753+08:00**（只读 `mode=ro` + 只读列目录/读文件，未启停任何进程）。

| 读数（对象集 + 采样面） | wiki 给的（21:33 现场） | 我的独立复核（22:31:08） | 一致性 |
| --- | --- | --- | --- |
| 逐字残余腿：`documents.name LIKE 'wiki/%'` 的页数与 chunk 数 | **4 页 = 25 chunk** | **5 页 = 26 chunk**：`wiki/autohotkey-v2`(6) · `cooking-pasta`(7) · `gardening-roses`(6) · `index`(1) · `kubernetes-troubleshooting`(6) | **〔该行的「不同但可解释＝时间差」解释已被证伪 ⇒ 见紧随表格的撤回块；正确写法是口径差，两个读数逐位相容〕** |
| `wiki_page_hashes` 行数 | 5 行，其中 `doctor-probe` **既无文件也无 document 行** | 5 行；`doctor-probe`：`document_row=0`、`disk_file=False` ⇒ **复现**；另 4 个 slug 都有 document 行与磁盘文件 | ✓（这条正是 U-8） |
| 出处粒度 | 页 frontmatter `sources:` / `source_hashes:` + `build: <id>`；**chunk 级出处全仓不存在** | `<root>/knowledge/wiki/autohotkey-v2.md` 的 frontmatter 逐字：`sources: [ahk-notes]` + `source_hashes: {ahk-notes: 1f84ac51…}` + `status: generated`；`chunks`/`documents`/`wiki_*` 里**没有任何 citation/chunk_id 列**（`chunk_revisions` 的 `chunk_id` 是编辑史，不是引用） ⇒ **chunk 级出处确不存在** | ✓ |
| `wiki_build_pages` 无正文 | 只能报 slug/action | 列 = `[build_id, slug, action, status, error]`（41 行） ⇒ **确无正文** | ✓ |
| plan 腿 | `wiki_builds.plan_json` 逐字 | 列含 `plan_json`（8 个 build） ⇒ 腿存在 | ✓ |
| 磁盘是真相源、DB 是影子 | `Knowledge::scan` 删「文件已不见」的 file-backed 行（`files.rs:290-309`，`report.removed`） | 只读核对同位置：`if !present.contains(&name) { delete_document(id) }` ⇒ **成立** | ✓ ⇒ 语义第六条与 U-7 |

**wiki 明确说「不需要新代码」**：三条腿都是现成 SQL/API；它只建议改**我这一侧的 `ResidualHit` 形状**（拆载体、key 可取回、出处只到文档粒度）—— 这三条我已采纳（见 v2 的 wiki 部分、key 约定表、语义第五/六条）。

**［撤回，2026-09-27，§7.18：撤回要显式，不许静默覆盖］** 上表第一行原本写的「**不同但可解释：相隔约 58 分钟，期间库/树变了（§7.68 的形态）**」是**我写错的解释**，现予撤回。

- **证伪来自 wiki**（2026-09-27T22:34:26.792102+08:00，同一对象、同一时刻、只读复核）：`wiki docs = 5 / 26 chunk` 是**含自动生成的 `index`**；`page docs = 4 / 25 chunk` 是**不含 index** ⇒ 两份读数**逐位相容**；且它 22:34 的复核与 21:33 **完全相同**（相隔 61 分钟），`wiki_builds` 的 status 分布也逐位相同 ⇒ **库/树没有变**，不是 §7.68 那种「对一棵移动中的树的两次读数」。
- **我的独立复核**（**2026-09-27T22:37:08.669435+08:00**，只读 `mode=ro`）：`documents` 里 `wiki/*` = **5 行 / 26 chunk**；去掉 `wiki/index`（`chunk_count=1`）= **4 页 / 25 chunk**；`wiki_builds` = **8 行**（`done` 4 · `planned` 2 · `planned_only` 2）⇒ 与 wiki 的更正一致。
- **正确写法（单行，不并列两列、不再解释成时间差）**：`wiki 文档 = 5（4 页 + index）/ 26 chunk`；`wiki 页 = 4 / 25 chunk（排除 index.md）`；时间窗写 **`21:33:17 与 22:34:26 两次读数逐位相同`**。
- **错在哪（方法层面，留给下一位读者）**：我取数前**没有先定口径**（「`documents.name LIKE 'wiki/%'` 里那一行自动 index 算不算一页」），于是把一个**定义差**读成了**时间差**，还给它配了一个听起来合理的机制（「相隔 58 分钟，树在动」）。这是 §7.73（错位碎片）与 §7.109（一条判据写错的代价要看它会驱动什么）的形态：这个假解释**会驱动**别人相信活库在自变（closure §7.2 家族），从而给后续所有读数打折。**规则（本规格自加）：凡「两个读数不一样」，先列出两者的对象集与口径差，再谈时间差。**

**对象集必须拆两行（引 wiki 的 R-D A.0；我的复核一致）**

| 行 | 对象集 | 读数（2026-09-27T22:37:08.67+08:00，同一口径复核） |
| --- | --- | --- |
| **B-09** | 页**作为文档被切成几块** | **25 chunk**（4 页；含生成的 index 则 26） |
| **B-09b** | 这 4 页所依赖的**来源**有几块 | **4 chunk**：`ahk-notes` = 1 · `ops-handbook` = 3（4 个页面 frontmatter 的 `sources` 只出现这两个名字） |

两者差一个数量级；混用会得出「证据很多」的错觉 —— **真库上 4 页的全部内容由 4 个来源 chunk 支撑**。

---

## D 冻结接口（注入契约的确切类型/常量/签名；破坏性变更与两条路径的收敛点）

以下为 **I-B 必须逐字实现**的形状。标注 `[冻结]` = 不改签名与字节；`[新增]` = 加东西；`[破坏]` = 会改变现有调用方或 agent 可见字节。

### D.1 预算（单位是字符，不是字节）`[冻结]`

```rust
pub struct InjectionBudget { pub per_block: usize, pub total: usize }   // inject.rs:8-13
impl Default for InjectionBudget { ... per_block: 1024, total: 4096 ... } // inject.rs:15-24
```
- 语义：`per_block`/`total` 都是 **`chars().count()`**（Unicode 标量值），断言 `out.chars().count() <= total`（`inject.rs:413 debug_assert`）。
- `[破坏]` 任何把单位改回**字节**（或把 `total` 调小/调大）的改动都会改变 agent 看到的字节，属策略变更，必须单独成单 + 附活体 render 读数。

### D.2 条目与标签 `[冻结] + [新增 TAG_GRAPH]`

```rust
pub struct ContextItem { pub tag: &'static str, pub content: String, pub date: String } // inject.rs:69-78
impl ContextItem {
    pub fn dated(tag: &'static str, content: impl Into<String>, ts: &str) -> Self;   // date = ts 的 'T' 之前部分
    pub fn undated(tag: &'static str, content: impl Into<String>) -> Self;           // date = ""
}
pub const TAG_USER_PROFILE: &str = "user_profile";
pub const TAG_RELEVANT_MEMORIES: &str = "relevant_memories";
pub const TAG_KNOWLEDGE: &str = "knowledge";
pub const TAG_WIKI: &str = "wiki";
pub const TAG_PROJECT_CONTEXT: &str = "project_context";
```
- `[冻结]` 构造**只经两个构造器**（现有调用点只有 `chat.rs:550`、`runs.rs:1873`、`knowledge_items`）：因此**加字段不破坏调用方** —— D.4 的 `confidence` 就按这个前提加。
- `[新增]` `pub const TAG_GRAPH: &str = "graph";`（图证据块；由 I-C 产出内容，注入契约只定义 tag 与位置）。**采纳依据**：`graph` 是一行结构化事实（带主客体），比「生成页」（wiki）更像证据、比逐字来源（knowledge）更弱。
- `[破坏/策略]` `tag_rank` 重排：

```rust
pub fn tag_rank(tag: &str) -> u8   // 现在: user_profile 0 / relevant_memories 1 / knowledge 2 / wiki 3 / project_context 4 / _ 5
```
改为 `user_profile 0 · relevant_memories 1 · knowledge 2 · graph 3 · wiki 4 · project_context 5 · _ 6`。影响：预算绑定时**被先丢的块**变了；`inject.rs:756-759` 的链式断言必须一次改齐（它正是为这件事存在的）。

### D.3 可见截断与丢块计数 `[冻结]`（含一处必须在文档里写清的语义）

```rust
pub fn tail_truncated(dropped_chars: usize) -> String;   // "… [+{N} chars truncated]"
pub fn cut_at(what: &str, bound_chars: usize) -> String; // "… [{what} truncated at {N} chars]"
```
- 逐块截断：块体 `chars().count() > per_block` 时只发前 `per_block` 个字符，再追加 `… [+{body_chars - per_block} chars truncated]`。
- 丢块：整块丢，`dropped += group.len()` ⇒ **计的是条目（记忆/命中）数，不是块数**；通知 `<context_budget>\n… [+{N} items dropped: context budget reached]\n</context_budget>\n`，放不下时退化为 `… [+{N} dropped]`；通知本身也受 `total` 约束（硬上界优先）。
- `[冻结]` 词表只有这一个来源（t309）；daemon 侧生产者（重试尾、handoff）**必须**调这两个函数，不许各写一份字节。
- `[新增，可加但不可重定义]` 若需要「丢了几块」`block_dropped`，**只能新增一条通知**，不许把 `dropped` 的语义从条目改成块。

### D.4 置信度进注入面 `[新增，破坏性=否]`

```rust
pub struct ContextItem { tag, content, date, pub confidence: Option<f64> }  // 新字段
pub fn dated_with_confidence(tag, content, ts, confidence: Option<f64>) -> Self; // 新构造器
// 渲染规则（唯一）：confidence == Some(c) && c < 0.5 ⇒ 行尾追加 " [unverified]"
```
- 依据：C1 + B.6（「低质量信号只对人可见」是 X-1）；选择**只渲染低置信**而不是恒渲染数字：一个 token 的成本 + 可行动（「这条没被确认」），且不需要把未校准的数当概率读（面板的 `memoryConfidenceHint` 已经自述它未校准）。
- `[破坏性=否]`：所有现有构造点用构造器；`render_injection`（`MemoryForInjection` 路径）保持 `confidence: None` 行为，字节不变（`inject.rs:466-499` 的 golden 判据因此**不应**变红）。

**［D.4 修订，2026-09-27，t8 实现期；原文保留在上面，只改「位置」这一件事］** 标记从**行尾后缀**改成**行首前缀**：

```rust
pub const LOW_CONFIDENCE_MARK: &str = "[unverified]";
// 渲染规则（唯一，改后）：confidence == Some(c) && c < LOW_CONFIDENCE
//   ⇒ 在**行首**写 "{LOW_CONFIDENCE_MARK} "，再写日期与正文
// 例：`[unverified] [2026-09-27] the user might use xlwt`
```

- **为什么改**：本单自己的测试 `the_low_confidence_mark_survives_truncation` 把后缀版当场跑红 —— 一条 4000 字符的低置信行在 `per_block=100` 下被截断，**后缀连同标记一起被截掉**；也就是说「标记最重要」的那类行长（又长又没被确认）恰好是标记消失的那类。前缀在任何截断下都活着。
- 代价：行首多一个 token 的词（与后缀同价），且**行首**在视觉/注意力上更靠前（对 agent 更可靠）。
- 破坏性：agent 可见字节变化，但只对 `<0.5` 的行生效；今天活库 0 行 ⇒ 只能由场景测试造正例（与 C1 的 jointly-delivered 半边一致）。
- 位置**不再**由前缀/后缀之争决定：`LOW_CONFIDENCE_MARK` 是唯一常量，测试比对它而不是字面量。

### D.5 选择（哪些记忆进注入）`[冻结]`

```rust
pub enum MemoryScope { User, Global, Project }
pub struct MemoryGroup { pub tag: &'static str, pub store: MemoryStore, pub scope: MemoryScope, pub limit: u32 }
pub struct InjectionSelection { pub groups: &'static [MemoryGroup], pub query_top_n: u32, pub query_min_score: f32 }
pub const CHAT_SELECTION: InjectionSelection;   // ... + query_top_n 4 / query_min_score 0.34
pub const RUNS_SELECTION: InjectionSelection;   // ... + query_top_n 0 （无查询腿）
pub const KNOWLEDGE_SOURCES: usize = 3;
pub const WIKI_PAGES: usize = 2;
pub fn knowledge_items(hits: &[RetrievalHit], sources: usize, wiki: usize) -> Vec<ContextItem>;
```
- 唯一执行点是 `crates/daemon/src/memembed.rs:227-278 select_injection_memories`；`Project` scope 在 **chat 侧不可解析**（chat 钉 cwd）⇒ 该组被**跳过而不是猜**（`memembed.rs:242-245`）。
- `[破坏/可证中性]` chat 的 `observation/global` 组是**死组**（Observation 不能写 global，`lib.rs:54-67`；`inject.rs:194-204` 自述）：删它 = 少一次查询、结论不变，但仍是行为变更 ⇒ 与本代一起走 I-B，并保留 `the_two_presets_state_their_own_differences` 对「死组之所以死」的那条断言（改成对 `allows_namespace` 的断言）。
- `[冻结]` 两条路径的差异（chat 有查询腿、runs 无；chat 读 procedure/lesson、runs 读 project）**是参数不是漂移**：任何统一都必须另附「agent 看到什么变了」的读数。

#### D.5 追加（t31 / RV-B-8 裁决 (a)：适配形状进冻结面，2026-09-28；只追加，前文一字未动）

**裁决**：captain 选 **(a) 接受适配形状**（不选 (b) 字面字段）。理由与 t8 时相同：`RetrievalHit` 是公开字段结构体，`chat.rs`/`runs.rs` 用结构体字面量构造它，加字段会让这两个文件编译不过 —— 与「`cargo check -p ruagent-daemon --all-targets` 通过」不可兼得。因此**冻结面里没有 `RetrievalHit.lead` / `RetrievalHit.relevance` 这两个字段**，要用的形状是下面这个（`crates/memory/src/inject.rs` 拥有）：

```rust
pub struct EnrichedHit {
    pub hit: RetrievalHit,                    // 形状与字节未变
    pub lead: Option<WikiLeadMeta>,           // R-D D.7
    pub relevance: Option<RelevanceMeta>,     // R-A H-4
}
impl EnrichedHit {
    pub fn plain(hit: RetrievalHit) -> Self;  // 未富集路径：lead=None, relevance=None
    pub fn order_value(&self) -> f32;         // 有 relevance 用它，否则用 hit.score
}
pub fn knowledge_items_enriched(hits: &[EnrichedHit], sources: usize, wiki: usize) -> Vec<ContextItem>;
// 旧 API 一字未动，且是薄包装：
pub fn knowledge_items(hits: &[RetrievalHit], sources: usize, wiki: usize) -> Vec<ContextItem>;
```

**逐字节冻结的部分（不得改动语义或字节）**：`RetrievalHit` 的字段与语义 · `knowledge_items()` 的输出 · `render_context()` 的块头/标记/丢块文案与预算单位（字符）· `InjectionBudget{per_block:1024,total:4096}` · 上面 D.3 的四个可见标记函数（`tail_truncated` / `cut_at` / `items_dropped_notice` / `items_dropped_minimal`，t31 起后两个由 `render_context` 本身调用，探针直接调它们即可）。`knowledge_items()` 当前的调用点（`chat.rs:569`、`runs.rs:1893`）是**未富集路径**，t19 把它们改成 `knowledge_items_enriched`，判据是「未富集调用点 = 0」。

**唯一映射点（单一适配器，t19 只调不写）**：`crates/daemon/src/memembed.rs`
```rust
pub fn lead_meta(lead: &crate::wiki::WikiLead) -> ruagent_memory::inject::WikiLeadMeta;  // 三态原样透传；Citation → (document, chunk_id)
pub fn relevance_meta(value: f32, kind: &'static str, version: u32, query_background: Option<f32>)
    -> ruagent_memory::inject::RelevanceMeta;   // kind 用 ScoreKind::as_str()，不复写拼写
pub fn enriched_hit(hit: &ruagent_knowledge::SearchHit, relevance: Option<RelevanceMeta>,
                    lead: Option<WikiLeadMeta>) -> ruagent_memory::inject::EnrichedHit;
```
**调用点（构造点在 chat.rs / runs.rs，归 t19）**：命中 → `enriched_hit(&hit, relevance.map(...), lead)`，其中 `lead` 取自 I-D 的 `wiki::lead_for(kb, recorded_hash, slug)`（三参 async，三态保真；同步档 `lead_from` 的 `edited`/`stale_since` 会是 `None`），`relevance` 取自 I-A 的 `RankedHit.relevance`。**注入侧不渲染任何相似度数字**（有断言钉住），也不解析 wiki 的 `## 来源`（那份解析全仓只有 wiki 一处，见 `gen2-wiki-spec` 的对应修订）。

### D.6 渲染器与收敛点 `[冻结]`

```rust
pub fn render_context(items: &[ContextItem], budget: &InjectionBudget) -> String;  // 唯一渲染器
pub fn render_injection(memories: &[MemoryForInjection], budget: &InjectionBudget) -> String; // 记忆专用入口，保留
```
- 规则（全部已有测试）：按**首次出现顺序**合并同 tag 的条目为一个块 → 每块受 `per_block` 截断（可见标记）→ 整块超 `total` 则**整块丢 + 计数** → 空输入返回**空串**（不许占位符）。
- **两条注入路径的收敛点（现状即契约）**：① 选择 `memembed::select_injection_memories`；② 排序 `tag_rank`；③ 渲染 `render_context(&items, &InjectionBudget::default())`；④ 无条目时 chat 返回 `None`、runs 返回 `String::new()`（都是「不发空块」）。任何**第三个**生产者必须走这四步，不许自写预算/块头/截断。
- `[破坏]` 若 `ContextItem` 新增字段之外还要改 `render_context` 的**字节**（块头/标记文案/顺序），必须同时给出：crate 级 golden 更新 + 至少一条盘上 render 的 before/after（否则第三层证据缺失）。

### D.7 `score_kind` 与分数语义（API 面，冻结为「不混用」）`[冻结]`

| 分数 | 语义 | 出现处 |
| --- | --- | --- |
| `score` + `score_kind="rrf_rank"` | **RRF 排名分**（k=60），不是相似度；只在 `/api/v1/recall` 的 memories/knowledge 条目上 | `api.rs:2579,2591,3003`；`memembed.rs:184-187,310` |
| `semantic_score` | 该行的**余弦**（memory 腿）/ LanceDB 距离（knowledge 腿，越低越近） | `api.rs:2581-2582,2600-2601` |
| `keyword_score` | 该行的 **bm25**（**越负越好**） | `api.rs:2582,2600-2601` |
| 注入选择的 `score` | **原始余弦**（`query_min_score=0.34` 过滤），**不经过 RRF** | `memembed.rs:257-276` |

- `[冻结]` `"rrf_rank"` 是**字面量**；k=60 只写在文档/注释里。**发现一处漂移**：`memembed.rs:151-152` 的注释把这个字段写成 `(score_kind)` 并在同段描述「RRF, k = 60」，容易被读成 `"rrf_k60"` —— 由 I-B 或 I-D 统一（`api.rs` 是唯一产出点）。
- `[冻结]` **注入面不使用 RRF**：注入的查询腿是原始余弦阈值。把 RRF 分送进注入会把「排名分」当「相似度」用（`api.rs:2566-2570` 记录的正是这个误读）。两条消费面各说各的分数类型，不许互相代用。

### D.8 本代的破坏性变更清单（I-B 必须逐条实现并各自留读数）

| # | 变更 | 类型 | 谁可见 |
| --- | --- | --- | --- |
| B-1 | `tag_rank` 插入 `graph`（3），`wiki→4`、`project_context→5` | 策略（agent 可见） | 预算绑定时的丢块顺序 |
| B-2 | 删 chat 的 `observation/global` 死组 | 行为（可证中性） | 每次注入少一次 SQL；结论不变 |
| B-3 | `ContextItem` 新增 `confidence` 字段 | API（编译期） | 只用构造器的调用方**不受影响**；直接写结构体字面量的外部调用方会红 |
| B-4 | 渲染 `<0.5` 时的 `[unverified]` 后缀 | agent 可见（新字节） | 只在低置信行；今天 0 行 ⇒ 需要场景测试造正例 |
| B-5 | `MemoryWrite` 的 confidence 来源从常量/clamp 改为具名信号 | 行为 | 写入值分布变化（可读数：distinct 值、`<0.5` 计数） |
| B-6 | `query.rs::row_to_memory` 从「静默回落」改为返回错误 | API（破坏性） | 所有读路径调用方 |
| B-7 | `memory_write`（HTTP+MCP）未知 store → 400 | API（破坏性，方向是「更诚实」） | 现有 MCP 客户端若传过 `store=bogus`，以前静默成功，现在失败 |
| B-8 | `memories` 新增 `access_count`/`last_used_at`（+ 可选 `valid_from/valid_to`） | schema（I-SCHEMA） | 迁移 + 读路径排序 |
| B-9 | `memory_diffs` 新增 op `merge_judged` / `consolidate` | 数据（审计词表） | 面板/导出/测试的 op 白名单 |

**按 R-1/R-2 的责任归属（不改变上表的类型与可见面）**：B-1、B-2、B-3、B-4、B-5、B-6、B-9 归 **I-B(t8) / mem-core**（文件分别为 `inject.rs`、`inject.rs`、`inject.rs`、`inject.rs`、`crates/memory/src/write.rs`+`confidence.rs`、`crates/memory/src/query.rs`、`crates/memory/src/write.rs`）；**B-7 归 INT/t19**（`api.rs` + `crates/mcp/src/lib.rs`，验收 t20/t21，见 `## C-INT` C-INT-1）；**B-8 的迁移脚本归 I-SCHEMA/t6**，`crates/memory` 只消费新列。

---

## E 实现清单（优先级 + inScope + schema 变更 + 与其他代不冲突的证明）

约定：**inScope 只写路径**；`冲突面` 一列显式点名 I-SCHEMA(crates/store)、I-A(crates/knowledge)、I-C(crates/graph+distill.rs)、I-D(daemon/wiki.rs)，并说明重叠如何避免（或已登记）。凡需他人改行为，**已发消息**（E.9 附证据）。

### E.1 [P0] 置信度信号与写入侧去钳位

| 项 | 内容 |
| --- | --- |
| inScope | `crates/memory/src/confidence.rs`（新）· `crates/memory/src/lib.rs`（导出）· `crates/memory/src/write.rs`（`MemoryWrite` 携带信号或校验后的值） |
| 做什么 | `ConfidenceSignals { user_confirmed, user_corrected, hedged, unconfirmed, ... }` + `pub fn confidence(s: &ConfidenceSignals) -> f64`，阈值具名常量（如 `CONF_CONFIRMED=1.0` / `CONF_UNCONFIRMED=0.8` / `CONF_HEDGED=0.4`）；写路径**不再**静默 clamp（clamp 只留 `debug_assert` 与显式错误） |
| schema | 无 |
| 冲突面 | `crates/store` **不碰**；`distill.rs`（I-C）必须改成把信号传进来、删掉 `clamp(0.5,1.0)`（`distill.rs:467`）⇒ **已登记依赖**（E.9-① ）。`crates/knowledge` 不碰 |
| 验收 | C1 |

### E.2 [P0] 合并判据：有界候选面 + 具名判据 + 审计决策

| 项 | 内容 |
| --- | --- |
| inScope | `crates/memory/src/dedupe.rs`（判据与决策类型）· `crates/memory/src/query.rs`（新增「同 scope 候选行」只读查询）· `crates/memory/src/write.rs`（`merge_judged` 审计）· `crates/memory/src/lib.rs` |
| 做什么 | ① `pub struct MergeCandidate { id: i64, content: String, cosine: Option<f32> }`；② `pub enum MergeVerdict { Same, Merge { rule: &'static str }, New, Refused(&'static str) }`；③ `pub fn merge_decision(new: &str, candidates: &[MergeCandidate], cfg: &MergeConfig) -> MergeDecision`，`MergeConfig` 里 `top_k: u32 = 3`、`tau_scope: f32`（**由调用方注入的分布导出的值**，不是常量）；④ 判 Mergeable 直接合；否则仅当 `cosine ≥ tau_scope` 才允许「具名判定」（判定人由 I-C 的 LLM 或 I-B 的规则提供，二选一在 I-B 落地时定，并把 `rule` 写进审计）；⑤ 决策落 `memory_diffs`：`op='merge_judged'`，reason 形如 `candidates=3 rule=lexical_ignorable verdict=merge` |
| schema | **无**（`memory_diffs` 已有 `reason`/`op` 列，只是多一个 op 值 ⇒ 不需要迁移；若 I-SCHEMA 的 op 白名单是硬约束则登记） |
| 冲突面 | 候选行的**余弦计算**今天在 `crates/daemon/src/memembed.rs`（不在 I-SCHEMA/I-A/I-C/I-D 任一单里）⇒ 若要在 crate 内闭环，需要一次 `memembed` 的归属决定（**请 captain 指派**，见 E.9-②）；`distill.rs` 的 `mergeable_target`（I-C）改为调 `merge_decision`（**已登记依赖** E.9-①）；`crates/knowledge` 只用其 `Embedder` trait（只读依赖，不碰代码） |
| 验收 | C2（含带标签语料 ≥40 对） |

**E.2 追加（t31 / R31，2026-09-28；只追加，第 ⑤ 条原文的「形如」格式按下表扩写，前文一字未动）**

审计串（`memory_diffs.reason`，唯一写者 `dedupe::merge_audit_reason`）现在的完整形状是**键值序列**，旧键一个没丢：

```
candidates=<数字> candidate_source=<embedding|none> anchor=<id|none> escalated_from=<id|none>
over_tau=<id,id|none> polarity_refused=<id|none> rule=<规则名> verdict=<same|merge|needs_judgement|new|refused>
```

- `candidate_source=`：就是本表 target ① 要求的那一个字符串，值集 **{embedding, none}**（`dedupe::candidate_source`，单一来源）。`none` = 本次决策无可用向量、只有词法判据。
- `anchor=`：判定**所依据**的那条候选。多行达 `tau_scope` 时选**文本最接近**的（`dedupe::text_overlap` = 归一化 LCS 长度），tie-break 依次为余弦、id —— **不再盲目跟余弦最高**：活库 45.19% 的配对在 cos ≥ 0.86，余弦单靠自己分不出「改写」与「另一件事的邻居」。
- `escalated_from=`：余弦更高但**决策没跟**的那条（噪声邻居签名）；一致时为 `none`。
- `over_tau=`：所有被拒且达 `tau_scope` 的行（**≥τ 的行不许藏**）。
- `polarity_refused=`：因极性词被拒的行。**一个极性拒绝只否决它自己那一对**：它既不可合并也不可升级，但**不再终止整条决策**（旧实现遇到第一个极性拒绝就 `return`，会让一个邻居替其它行做决定——t31 修掉并各有测试）。没有更好的结果时，verdict 仍是 `Refused(极性)`。
- `MergeVerdict` 的**形状与语义未变**；新增的是 `pub struct MergeAudit { verdict, anchor, escalated_from, over_tau, polarity_refused, cosines_available }` 与 `pub fn merge_decision_audited(..) -> MergeAudit`；`merge_decision(..) -> MergeVerdict` 保留为薄包装（**签名未变**，I-C 的调用点不受影响）。

### E.3 [P1] 使用状态 → 衰减与巩固（排序，不删）

| 项 | 内容 |
| --- | --- |
| inScope | `crates/memory/src/query.rs`（排序表达式 + 命中写回）· `crates/memory/src/lifecycle.rs`（删除时不动使用状态）· `crates/memory/src/lib.rs`（`MemoryRow` 新字段） |
| 做什么 | ① 读路径 `score = base + lambda * exp(-delta_t / (1 + access_count))`（`base` 由调用方给：bm25/余弦融合分）；② 命中写回 `access_count` / `last_used_at`（单写者 actor，**只写这两列**）；③ 排序变化不得改变返回内容 |
| schema | `memories ADD COLUMN access_count INTEGER NOT NULL DEFAULT 0` + `ADD COLUMN last_used_at TEXT`（**I-SCHEMA 的执行面在 crates/store/src/migrations**）⇒ 迁移文件由 I-SCHEMA 出，我在报告里登记「需要 0019 迁移」；同批若采纳 B.2 的 `valid_from/valid_to` 一并申请 |
| 冲突面 | **必须与 I-SCHEMA 分文件**：迁移脚本归 I-SCHEMA，`crates/memory` 只消费新列；不碰 `crates/knowledge`/`crates/graph`；`wiki.rs`（I-D）不涉及 |
| 验收 | C3 |

### E.4 [P1] 会话关闭的巩固/反思入口

| 项 | 内容 |
| --- | --- |
| inScope | `crates/memory/src/consolidate.rs`（新：输入 = 一组记忆/行 + 一条结论文本 + 来源列表，输出 = 写入结果；**不含 LLM 调用**）· `crates/memory/src/lib.rs`· `crates/memory/src/query.rs`（`session_memories(session_key)`） |
| 做什么 | ① 巩固产物的最小形状：`content` + `sources: Vec<i64>`（记忆 id 或 episode id）+ `confidence`（走 E.1）+ 审计 `op='consolidate'`；② 巩固产物**必须可被注入**（tag 用 `TAG_RELEVANT_MEMORIES` 或新 tag，位置见 D.2）；③ 提供「一条结论不被同一批来源重复写入」的幂等键（内容哈希 + 来源集合指纹） |
| schema | 若来源列表要落库：`memories` 无该列 ⇒ 需要 **I-SCHEMA** 加表/列（例如 `memory_sources(memory_id, source_kind, source_id)`）；这是**新表**，必须在 I-SCHEMA 的任务里同步，避免与它的迁移编号冲突 |
| 冲突面 | LLM 调用在 `distill.rs`（I-C）；API 派生布尔在 `api.rs`（无主，见 E.9-②）。`wiki.rs`（I-D）不涉及；`crates/knowledge` 不涉及 |
| 验收 | C4 |

### E.5 [P1] 遗忘的可证明性（残余面清点）

| 项 | 内容 |
| --- | --- |
| inScope | `crates/memory/src/lifecycle.rs`（`purge` 的读数化：返回「内容哈希 + 已消失面」）· `crates/memory/src/query.rs`（按内容哈希的残余查询：`memories`/`memories_fts`/`episodes`） |
| 做什么 | ① `pub async fn forget_report(db, id_or_hash) -> ForgetReport { memories: usize, fts_hits: usize, episodes: usize, derived: External? }`；② `ForgetReport` 对**跨存储**面（knowledge 文档、wiki 页）只报「需要 I-A/I-D 提供」，**不在本 crate 里越界查询**；③ 判据：purge 后 memory 侧三面全 0 |
| schema | 无（`episodes.content` 已有） |
| 冲突面 | 跨存储的残余面清单 = **I-A（knowledge）/I-D（wiki）** 的接口：我只定义一个「问」的形状（E.9-③ 登记），不实现它们；`crates/store` 只读 |
| 验收 | C5 |

### E.6 [P1] 注入契约：`TAG_GRAPH` + `[unverified]` + 排序键

| 项 | 内容 |
| --- | --- |
| inScope | `crates/memory/src/inject.rs`（唯一文件） |
| 做什么 | D.2/D.3/D.4：`TAG_GRAPH`、`tag_rank` 重排、`ContextItem.confidence` + `dated_with_confidence` + `<0.5` 后缀、`the_two_presets_state_their_own_differences` 断言更新、chat 死组删除（B-2） |
| schema | 无 |
| 冲突面 | 图块的**内容**由 I-C 产出（`distill.rs`/`graph`），我只定义 tag/位置/契约；`memembed.rs` 负责把图证据转成 `ContextItem`（无主，E.9-②）；`crates/knowledge` 不碰（`RetrievalHit` 是本地类型，`inject.rs:276-289` 自述「刻意不依赖 knowledge crate」） |
| 验收 | C6 + `cargo test -p ruagent-memory` 全绿（golden 字节不变的部分不许变红） |

### E.7 [P2] 蒸馏可归因与三态日志（**依赖 I-C**）

| 项 | 内容 |
| --- | --- |
| inScope | 无（本代不由我实现）：`distill.rs`（I-C/t9）+ `distill_log` 的表（**I-SCHEMA/t6**）；`api.rs` 的 `distilled` 派生归 **INT/t19**（已移出到 `## C-INT` C-INT-2，R-2） |
| 做什么 | ① prompt 指纹入 `distill_log`；② 失败/空产出各写一行（status/failure_reason），**失败绝不创建 episode**（`run_turn` 是面板布尔的唯一来源，见 A.5／§7.140）；③ 徽章只允许由 `episodes.kind='run_turn'` 派生（执行面在 C-INT-2） |
| schema | `distill_log` 加 `status`/`failure_reason`/`prompt_fingerprint`（I-SCHEMA） |
| 冲突面 | 我**不改** `distill.rs`；已与 graph 双向确认（E.9-① 里 D1/D2/D4/D5 的答复；D3 的闸门） |
| 验收 | C4 的可归因部分（归 I-C 的单验） |

### E.8 [P0] 诚实失败（HTTP/MCP 的未知 store）

| 项 | 内容 |
| --- | --- |
| inScope | **拆分（R-1/R-2）**：① `crates/memory/src/query.rs` 的 `row_to_memory` → `Result`（**mem-core / I-B(t8)，是我的 inScope**）；② `crates/daemon/src/api.rs` 的 `memory_write` 400 与 `crates/mcp/src/lib.rs` 的 4xx 透传 → **INT / t19**（目标行 = `## C-INT` C-INT-1，验收 t20/t21） |
| 做什么 | C7 四条（C7 原表保留原读数 + 归属标注）；本项在我这半边只做 `row_to_memory` 的显式错误化（破坏性变更 B-6） |
| schema | 无 |
| 冲突面 | `api.rs` 是多人用过的高冲突文件（closure §7.62 记录过「同一文件两个写者」事故）⇒ 按 R-1 由 **INT/t19 独家写**，我**一行都不改**；我只改 `crates/memory/src/query.rs`，它与 `api.rs` 的接口是「返回值类型」而非同一文件 |
| 验收 | C7（我这半边）+ C-INT-1（t19/t20 那半边） |

### E.9 对 `distill.rs` 的依赖登记（含发消息的证据）

| # | 依赖 | 方向 | 证据 |
| --- | --- | --- | --- |
| ① | `distill.rs` 必须改 4 处行为：① 置信度去 clamp 并把信号交给 `crates/memory::confidence`（E.1）；② `mergeable_target` 改调 `merge_decision`（E.2）；③ `write_memories` 把 episode id 返回给 `write_graph`（graph 的 D2）；④ 失败/空产出写 `distill_log` 且失败不建 episode（E.7） | 我 → graph | **已发**：`agent_teams_send_message` → graph，主题「[mem-core / t2 R-B] 你的 D1–D5 逐条答复」，message id **8ba50a93-37bd-4ae1-886c-afca69a6da09**（2026-09-27 本会话内）。结论：D1/D2/D4/D5 同意（D2 附「返回具名字段、不许移动 episode 创建时机、空 memories 也要建 run_turn episode」三条），D3 有条件同意（记忆面字节不变 / valid_at 未知必须 null / distill_log 记 prompt 指纹，缺一条就走「第二次抽取调用」），D4 追加「失败行绝不创建 episode，徽章只许来自 `episodes.kind='run_turn'`」，TAG_GRAPH 采纳并冻结位置 |
| ② | `crates/daemon/src/memembed.rs`（候选集余弦、注入选择、图证据转 `ContextItem`）与 `crates/daemon/src/api.rs`（`memory_write` 的 400、`distilled` 派生）**不属于 I-SCHEMA/I-A/I-C/I-D 任一单** | 我 → captain | **已裁决（R-1/R-2，captain，2026-09-27）**：`memembed.rs` **归 mem-core / I-B(t8)**（t8 的 inScope 已 amend 加入它与本文件；t19 的 inScope 已移除它 ⇒ 单写者）；`api.rs` 保持 **INT/t19 单写者**，其两条目标已移出为 `## C-INT`（C-INT-1/C-INT-2，验收 t20/t21）。本规格据此改：C 节不含 api.rs 条目，E.2/E.6 的 memembed 半边进入我的 inScope |
| ③ | 跨存储残余面清单（knowledge 文档 / wiki 页） | 我 → recall(t1) + wiki(t4) | **已发（R-4 批准）**：`agent_teams_send_message` → recall，message id **18a9a07d-48dd-4ae7-b299-fa9e109de852**；→ wiki，message id **0c817b7d-471d-4347-9f54-30ada46c5d52**（2026-09-27 本会话内）。两封都写明「只读查询、只要求读数、不需要你们改文件」，并附了我这一侧的 `ResidualHit` 形状草案 + 三个具体问题（是否已存在这样的查询 / 他们认可的应答形状 / 正文落在 DB 还是磁盘），要求他们把「预期」与「读数」分开（§7.130）。**回信未到之前，C5 的残余面那半边是「形状已定、实现归 I-A/I-D」，不得当成已具备** |

### E.10 优先级排序（I-B 的执行顺序建议）

1. **P0**：E.1 置信度 · E.2 合并判据（两者都在我的 inScope 内闭环，且能立刻给出「改前→改后」读数；它们的 distill 半边按 R-3 标 jointly-delivered）。**E.8 已不再是我的单**（R-2：api.rs 半边 → `## C-INT`/t19；我只留 `query.rs` 的 `row_to_memory`，随 E.2 一起做掉）
2. **P1**：E.6 注入契约（`TAG_GRAPH`/`[unverified]`/排序键，含 `memembed.rs` —— R-1 后归我）→ E.3 使用状态（需 **I-SCHEMA/t6** 迁移）→ E.4 巩固（依赖 I-C/t9 才能闭合 C4）→ E.5 遗忘可证明性（残余面半边依赖 recall/wiki 的回信）
3. **P2**：E.7（I-C/t9 的单）与本文件的收尾（R-2 的 C-INT 已就位）

**依赖阻塞提醒**（§7.93 家族）：E.1 的「`<0.5` 可达」与 E.2 的「在 distill 写入路径上生效」都**无法只靠 I-B 闭合** —— 它们要 I-C 改 `distill.rs:467` 与 `611-616`。如果 I-C 未完成，V-B 必须把这两条判成 **not_measured（原因：依赖未落地）**，而不是判过或判败。

---

## 附录 A1 探针：`judge()` 的等价移植 + 活库两两判定（A.9 的复现装置）

```python
import sqlite3, itertools, collections
IGNORABLE=["偏好","进行","用户","我们","一种","这个","那个","就是","可以","请","的","了","呢","啊","吧","吗","我","会","要",
           "please","prefers","prefer","users","user","the","and","are","is","to","of","in","a","an"]
POLARITY=["不","别","没有","没","无需","无","禁止","避免","取消","停止","不再","从不","绝不",
          "never","not","no","without","avoid","stop","dont","don't","cannot","can't"]
def normalize(s): return "".join(c.lower() for c in s if c.isalnum())
def diff(a,b):
    x,y=list(a),list(b); n,m=len(x),len(y); dp=[[0]*(m+1) for _ in range(n+1)]
    for i in range(n-1,-1,-1):
        for j in range(m-1,-1,-1):
            dp[i][j]=dp[i+1][j+1]+1 if x[i]==y[j] else max(dp[i+1][j],dp[i][j+1])
    i=j=0; out=[]
    while i<n and j<m:
        if x[i]==y[j]: i+=1; j+=1
        elif dp[i+1][j]>=dp[i][j+1]: out.append(x[i]); i+=1
        else: j+=1
    while i<n: out.append(x[i]); i+=1
    return "".join(out)
def strip_ignorable(s):
    for w in IGNORABLE:
        while w in s: s=s.replace(w,"",1)
    return s
def contains_token(hay,tok):
    if any(c.isalpha() and c.isascii() for c in tok):
        return any(w==tok for w in "".join(c if c.isalnum() else " " for c in hay).split())
    return tok in hay
def judge(old,new):
    a,b=normalize(old),normalize(new)
    if not a and not b: return "Same"
    if a==b: return "Same"
    if not a or not b: return "Refused:empty"
    oo,on=diff(a,b),diff(b,a); changed=oo+on
    if any(contains_token(changed,p) for p in POLARITY): return "Refused:polarity"
    return "Mergeable" if not strip_ignorable(oo) and not strip_ignorable(on) else "Refused:content"
# 自检：必须复现 crate 的向量（22/22）——四条中文改写两两 Mergeable，极性/内容替换 Refused
four=["[distilled] 用户偏好使用简体中文交流。","[distilled] 用户使用简体中文交流。",
      "[distilled] 用户使用简体中文进行交流。","[distilled] 偏好使用简体中文交流。"]
assert all(judge(a,b)=="Same" for a in four for b in four if a==b)
assert all(judge(a,b)=="Mergeable" for a in four for b in four if a!=b)
assert judge("用户偏好简体中文","用户不使用简体中文")=="Refused:polarity"
assert judge("[distilled] 用户偏好简体中文交流。","[distilled] 用户偏好英文交流。")=="Refused:content"
c=sqlite3.connect("file:C:/Users/19410/.ruagent/data/ruagent.db?mode=ro",uri=True)
rows=c.execute("""SELECT id,store,namespace,content FROM memories
                  WHERE superseded_at IS NULL AND deleted_at IS NULL ORDER BY store,namespace,id""").fetchall()
g=collections.defaultdict(list)
for i,s,n,ct in rows: g[(s,n)].append((i,ct))
pairs=[(i1,i2) for v in g.values() for (i1,c1),(i2,c2) in itertools.combinations(v,2)]
verdicts=collections.Counter(judge(c1,c2) for v in g.values() for (i1,c1),(i2,c2) in itertools.combinations(v,2))
print("live rows=%d in-scope pairs=%d verdicts=%s"%(len(rows),len(pairs),dict(verdicts)))
```

## 附录 A2 探针：余弦分布 + top-K 候选召回（A.9 的复现装置）

```python
import sqlite3, struct, itertools, collections, statistics
con=sqlite3.connect("file:C:/Users/19410/.ruagent/data/ruagent.db?mode=ro",uri=True)
rows=con.execute("""SELECT id,store,namespace,embedding FROM memories
                    WHERE superseded_at IS NULL AND deleted_at IS NULL AND embedding IS NOT NULL""").fetchall()
vec=lambda b: struct.unpack("<%df"%(len(b)//4),b)
def cos(a,b):
    d=sum(x*y for x,y in zip(a,b)); na=sum(x*x for x in a)**.5; nb=sum(y*y for y in b)**.5
    return d/(na*nb) if na and nb else 0.0
g=collections.defaultdict(list)
for i,s,n,b in rows: g[(s,n)].append((i,vec(b)))
pairs=[]; neigh={}
for k,mem in sorted(g.items()):
    for (i1,v1),(i2,v2) in itertools.combinations(mem,2): pairs.append(cos(v1,v2))
    for i1,v1 in mem:
        neigh[i1]=[j for _,j in sorted(((cos(v1,v2),i2) for i2,v2 in mem if i2!=i1),reverse=True)]
pairs.sort(reverse=True)
print("pairs=%d max=%.4f p99=%.4f p95=%.4f median=%.4f min=%.4f"%(len(pairs),pairs[0],
      pairs[int(len(pairs)*.01)],pairs[int(len(pairs)*.05)],statistics.median(pairs),pairs[-1]))
for th in (.95,.90,.86,.80): print("cos>=%.2f: %d (%.2f%%)"%(th,sum(1 for c in pairs if c>=th),100*sum(1 for c in pairs if c>=th)/len(pairs)))
pos={(min(a,b),max(a,b)) for (s,n),mem in g.items() for (a,_,va),(b,_,vb) in itertools.combinations(mem,2) if cos(va,vb)>=.95}
for K in (1,2,3,5):
    rec=sum(1 for a,b in pos if b in neigh[a][:K] or a in neigh[b][:K])
    print("K=%d recovers %d/%d of cos>=0.95 pairs"%(K,rec,len(pos)))
```

## 附录 A3 探针：盘上 render 普查（A.10 的复现装置）

```python
import glob, json, collections, statistics
TAGS=["user_profile","relevant_memories","knowledge","wiki","project_context"]
def klass(r):
    if r.startswith("<") and any(("<"+t+">") in r for t in TAGS+["context_budget"]): return "tagged"
    if r.startswith("[memory context"): return "legacy_pre_t260"
    if r.startswith("[role") or "you are" in r[:80]: return "role_retry"
    return "other"
by=collections.defaultdict(list)
files=glob.glob("C:/Users/19410/.ruagent/data/transcripts/*.jsonl")
for p in files:
    for line in open(p,encoding="utf-8",errors="replace"):
        if "context_injected" not in line: continue
        try: o=json.loads(line)
        except Exception: continue
        ev=o.get("event") if isinstance(o,dict) else None
        if isinstance(ev,dict) and ev.get("type")=="context_injected": by[klass(ev.get("render") or "")].append(ev.get("render") or "")
print("files=%d events=%d"%(len(files),sum(len(v) for v in by.values())))
for k,v in by.items():
    ch=sorted(len(r) for r in v)
    print("%s n=%d chars min/med/max=%d/%d/%d truncated=%d dropped=%d budget_block=%d"%(
        k,len(v),ch[0],int(statistics.median(ch)),ch[-1],
        sum(1 for r in v if "chars truncated" in r),sum(1 for r in v if "dropped" in r),
        sum(1 for r in v if "<context_budget>" in r)))
```

---

## 附：本单自检

| 纪律 | 状态 |
| --- | --- |
| 只改 inScope（本文件） | 全文唯一的写动作 = 创建/编辑 `docs/design/reviews/gen2-memory-spec.md`（读入的文件全部只读） |
| 未改任何代码 | 是。`git status --porcelain -- crates panel` = **空**（2026-09-27T22:06:35+08:00）；全量 `git status --porcelain` = 本文件 + `docs/design/reviews/gen2-graph-spec.md`（**后者是 graph 的在途产物，不是我的**，我未触碰） |
| 未启停真 daemon（pid 79984 / 127.0.0.1:8787） | 是（只读 `mode=ro` 与只读文件扫描；未调 `/api/v1/recall`） |
| 读数带时间窗 | 是（A 节每行；旧值必带时间点与来源） |
| 分布读数不当结论 | 是（A.5/A.9/A.10 三处显式写出「未落地/无样本/循环性」的局限） |
| 未把「标记覆盖率」当结论 | 是（A.5 明确：0 正例只说未落地） |
| 对 distill.rs 的依赖已发消息 | 是（E.9-①，message id `8ba50a93-37bd-4ae1-886c-afca69a6da09`） |
| verify 命令 | `$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"; $env:CARGO_TARGET_DIR="$env:TEMP\ruagent-verify-rb"; cargo test -p ruagent-memory` → exit **0**，`28 passed; 0 failed; 0 ignored`（运行 2.69s），起 **2026-09-27T22:06:35.3419311+08:00** |
| verify 的 code 面 | `git status --porcelain -- crates panel` → 空输出，exit 0（2026-09-27T22:06:35+08:00） |

**R-1..R-5 之后的收尾（2026-09-27，同一文件追加）：**

| 项 | 状态 |
| --- | --- |
| C-INT 节 | 已新建（owner=INT/t19，验收 t20/t21），两条 = C-INT-1（`memory_write` 未知 store 400）· C-INT-2（`distilled` 布尔按 episode kind 派生）；C4/C7 原行**保留原位 + 标注指向**（R-2：只标注，未删、未改数） |
| jointly-delivered 标注 | C1/C2/C4/C6 各加「交付与责任（R-3）」行，写明 mem-core 半边 / graph-I-C 半边，以及 I-C 未落地时的 **not_measured** 判法 |
| §6.1 被推翻的前提 | **保留在 A.10 原位**并带复核结论（R-5） |
| 对 recall / wiki 的只读查询请求 | 已发（R-4）：message id `18a9a07d-48dd-4ae7-b299-fa9e109de852`（recall）· `0c817b7d-471d-4347-9f54-30ada46c5d52`（wiki） |
| 越界声明 | **本次收尾未触碰 `crates/`、`panel/`、`api.rs` 一行**；改动只有本文件（`git status --porcelain -- crates panel` 复读见下） |
| 与 t2/t8 的关系 | t2（R-B）已 completed（终态不可变）；本次收尾是 **t8(I-B) 的前置**（t8 的 inScope 含本文件），不改变 t2 的终态内容，只按 captain 裁决追加 |

**R-6 之后的追加（2026-09-27，同一文件；recall 回信到达后）：**

| 项 | 状态 |
| --- | --- |
| `## C5-D` 新节 | 已新建：**non-landing** 的 `forget_report` 形状（类型 + 语义三条）+ 待定项 U-1..U-6；**不写实现、不预填数字判据、未碰 `crates/`**（R-6） |
| U-3 收敛 | 已按 recall 回信收敛：knowledge 侧 = **五个派生物面**；`ResidualHit` 加 `origin`（`db|file|both`）与 `path`；清点用**原文子串 LIKE**，**不许**用 `search()` 的排序命中形状（会假通过） |
| 「已存在」vs「承诺提供」 | 已分开：裸 SQL（`store.rs:575-605`，私有+带 `leg_k` 上界）与 `documents.content_hash` = **今天已存在**；`residual_scan`/`document_path` = **I-A 承诺 t7 后提供**，t7 前只允许 `ExternalStatus::NotAvailable`，不许记 `Readout(0)` |
| 我的独立复核 | 2026-09-27T22:16:52+08:00 只读复跑（`documents` 934/`source` 非空 934/`chunks` 10765 且**无 hash 列**/`knowledge/**/*.md` 934/触发器 0006:19-30/LanceDB 路径存在）⇒ 与 recall 的 22:1x 读数**逐条一致** |
| U-4 仍待 | wiki 面（正文落 DB 还是磁盘、build→sources 出处）**未回信** ⇒ 保持 `Unknown`；回信到达后按同法收敛并引用其时间点 |

**收敛 B（2026-09-27，recall 第二次回信后追加）：**

| 项 | 状态 |
| --- | --- |
| 形状 v2 | `origin` → `ResidualOrigin{Db,File,Both}`（与 I-A 逐字相同）；每面读数 → `ResidualCount{hits,truncated,total:Option<usize>}`；新增 `LocalResidual`；**v1.5 原文保留在其上方** |
| 语义第四条 | `truncated` 判据必须是 **`limit+1`**（不是 `hits == limit`）；`total` 只在**显式 `exact_total=true`** 时才有值；**`None ≠ 0`** —— 我的调用点若需要「确切的 0」必须显式要 total |
| 代价读数 | recall **10.47 ms**（其 t1 A8）vs 我的复跑 **2026-09-27T22:23:50+08:00**：`COUNT(*) LIKE` median **6.80 ms**、`LIMIT 51` median **6.69 ms**、无命中 median **6.63 ms** ⇒ 结构结论成立（`limit+1` 零额外扫描、`exact_total` = 再一次同价全表扫）；两方数值差约 1.5× 属时间读数的粗仪器性质（§7.118），规格里已写明不该当矛盾 |
| 漂移自检 | 归 **I-A 的 D.4 / t7 测试**（比对字段名与 `ResidualOrigin` 取值集合）；**我的承诺写进规格**：不放宽取值集合、不改字段语义，除非先告诉 recall |

**收敛 C（2026-09-27，wiki 回信后追加）：**

| 项 | 状态 |
| --- | --- |
| wiki 载体拆分 | `WikiPage` 单一面 → **四个载体**：`WikiChunk`（逐字，DB 影子）· `WikiPageHash`（**只有哈希，答不了 substring**）· `WikiPlan`（`plan_json` 文本）· `WikiBuildPage`（无正文，只能报 slug/action）；`WikiPage` 原文保留为历史 |
| key 约定 | `wiki/<slug>#c<chunk_id>`（可喂 knowledge expand）· `build/<id>`（**只能**走 wiki builds API 的 plan）· `hash:<sha256>`（不是可取回对象，需重读文件再哈希） |
| 语义第五/六条 | ⑤ 哈希命中**不得**计为内容残余（否则假阳/假阴各一次，且都长得像「查过了」）；⑥ wiki 面的「没有残余」**只在一个扫描周期内**成立（`scan` 会删影子行，`files.rs:290-309`）⇒ 读数必须带时刻 |
| 我的独立复核 | **2026-09-27T22:31:08.59+08:00** 只读：5 页/26 chunk（wiki 21:33 报 4 页/25 chunk，**不同但可解释** —— 相隔 58 min，库/树已变，§7.68，两读数各带时刻不合并）**〔此解释已撤回：是口径差（含/不含生成的 index），两读数逐位相容 —— 见「收敛 C 的更正」〕** · `wiki_page_hashes` 5 行且 `doctor-probe` 无 document 行无文件（**复现**，→ U-8）· frontmatter `sources/source_hashes` 只是文档粒度、**chunk 级出处确不存在** · `wiki_build_pages` 列无正文 · `files.rs:290-309` 的 `report.removed` 逐字核对成立 |
| 新增待定项 | **U-7**（要不要带有效期/主动重读文件代替影子）· **U-8**（孤儿哈希：有哈希行、无 document 行、无文件 —— **已裁决为 bug**，目标 0 行，收口归 I-D 的产品路径） |
| **收敛 C 的更正（撤回）** | 已把「4 页 vs 5 页 = 时间差」**显式撤回**（§7.18），改为**口径差**：`wiki docs = 5（4 页 + index）/ 26 chunk`；`wiki 页 = 4 / 25 chunk（排除 index）`；时间窗 **`21:33:17 与 22:34:26 两次读数逐位相同`**；我的复核 **22:37:08.67+08:00** 一致。方法教训（写进规格）：**凡「两个读数不一样」，先列对象集与口径差，再谈时间差** |
| 对象集拆两行 | **B-09** = 页作为文档 25 chunk；**B-09b** = 4 页的**来源**只有 4 chunk（`ahk-notes` 1 + `ops-handbook` 3）；引 wiki 的 R-D A.0，我 22:37:08 复核一致 |
| I-D 的 DDL 请求（登记） | wiki 在 R-D 的 E.2 已把 `wiki_citations` 的 DDL 作为对 **I-SCHEMA 的请求**列出（迁移 `0019`，DDL-1/2）；**我不抢先定死形状** —— 它落地时按 DDL-1/2 收敛 `ResidualHit`/`key` 的出处部分 |
| 越界声明（C） | 仍只改本文件；`crates/daemon/src/wiki.rs`（I-D）· `api.rs`（INT/t19）· `distill.rs`（I-C）· `crates/store` 均未触碰；`git status --porcelain -- crates panel` 复读见上 |

---

## 一致性裁决 RV-C2-9：失败尝试的 episode 语义（t41 / mem-core，2026-09-28；**只追加，前文一字未删未改**）

**裁决**：captain 裁决（2026-09-28）**采纳 RVC-3 的实现**，并以本段**取代**下述两处旧措辞。**D2(b) 不变。**

### 1 冲突的对象集：哪三处字面互斥（原文引用，均保留原位）

| # | 原文位置 | 字面 |
| --- | --- | --- |
| ① | 本文件 **E.7「做什么」②**（第 953 行） | 「② 失败/空产出各写一行（status/failure_reason），**失败绝不创建 episode**」 |
| ② | 本文件 **E.9-① ④**（第 972 行） | 「④ 失败/空产出写 `distill_log` 且**失败不建 episode**（E.7）」；同处还记着我给 graph 的 D-答复：「D2 附『返回具名字段、**不许移动 episode 创建时机**、空 memories 也要建 run_turn episode』」「D4 追加『失败行绝不创建 episode，徽章只许来自 `episodes.kind='run_turn'`』」 |
| ③ | 同一答复的 **D2(b)** | 「**不许**把 episode 创建挪到 `write_graph` 之后」 |

**为什么 ①② 与 ③ 不能同时成立**：失败点是 `write_graph`（图写入这一步），而 ③ 要求 episode 必须在它**之前**创建 ⇒ 在失败路径上「已经有 episode」与「失败就不建 episode」在字面上**互相否定**。真正要守的不变量不是「失败没有 episode」，而是**第三条文**：`run_turn` 是徽章的唯一来源（A.5／§7.140）。

### 2 取代后的可同时成立表述（本节生效的判据文本）

> **失败尝试可以创建 episode，但必须在同一路径内把它标记为失败态并保留来源；唯一禁止的是让它以 `run_turn` 呈现为成功。**

落成三条可检验的子句：

1. **可以创建**：`write_memories` 内创建 episode 是**允许**的（这正是不违反 D2(b) 的唯一形状：创建时机不许后移）。
2. **必须标记**：失败时在**同一路径内**把它标为 `kind='run_turn_failed'`，并在 `meta` 写入 `{"voided_by":…,"reason":…}`（`COALESCE(meta, ?)`，**不覆盖**既有 meta）；标记带 `AND kind='run_turn'` 守卫 ⇒ 重复标记是 no-op，也不会误标其它 kind。
3. **必须保留来源**：`memories.source_episode` 仍指向该 episode（**不删行**，见 §3）。**唯一被禁止的行为**是让这次尝试以 `kind='run_turn'` **呈现为成功**。

### 3 「不删除」的机制依据（记忆侧事实，我这一侧已逐条核对）

| 事实 | 位置 | 含义 |
| --- | --- | --- |
| `source_episode INTEGER REFERENCES episodes(id)`，**无 `ON DELETE`** | `crates/store/src/migrations/0004_memory.sql:25`（`sqlite.rs:49` 设 `foreign_keys=ON`） | 删除被引用的 episode 会被 SQLite 拒绝；删行还会丢掉好记忆的来源 ⇒ **标记而不是删除** |
| 只在插入时写一次 | `crates/memory/src/write.rs:118-126` | `source_episode` 是记忆侧的溯源，不随 episode 状态变化 |
| 唯一另一处写它的语句只填空、永不清空 | `crates/memory/src/lifecycle.rs:681`（t347 前缀迁移 `COALESCE(source_episode, ?3)`） | kind 变更对记忆侧溯源**不可见** |
| `session_memories` 的 JOIN **不看 kind** | `crates/memory/src/query.rs:302-312` | C4 的读路径在标记前后**给出同一结果** |

### 4 徽章契约仍成立（本次修法的目的）＋ 通则（captain 2026-09-28 加强）

- 面板的 `distilled` 布尔**只**由 `episodes.kind='run_turn'` 派生（执行面 = `## C-INT` C-INT-2，`api.rs` 的 `DISTILLED_EPISODE_KIND`）⇒ 被标记的失败尝试**不会**显示成发生过。panel 侧对应注释见 `panel/src/api.ts:296-298`。
- **通则**：`run_turn_failed` **不是**唯一的非 `run_turn` 溯源 episode —— t347 的 `[distilled] ` 前缀迁移给 156 行历史记忆建了一条 `kind='manual'` 的 episode（`crates/memory/src/lifecycle.rs:907` 有断言，A.4 的读数表已记）。所以「**溯源 episode 存在 ≠ 蒸馏发生过**」是**通则**，失败路径只是它的一个实例。**任何「蒸馏发生过」的代理都必须按 `kind` 过滤。**

### 5 可复现读数（RVC-3 真实失败路径）

来源：graph 的 `docs/design/reviews/gen2-graph-repair.md` §1（t27/t38，2026-09-28），失败由 `entity_edges` 上的 `RAISE(ABORT)` 触发器触发；我**未复跑**他的路径，此处是**引用**而非我的读数：

```
episodes 1 -> 1 | kind=run_turn 1 -> 0 | episode kind="run_turn_failed"
meta=Some("{\"voided_by\":\"write_graph\",\"reason\":\"adding relation runs_on\"}")
memories keeping their provenance 1
```

读法：**行数不变**（补偿是标记，不是增删）· `run_turn` 计数**减少**（徽章不再认它）· 来源**保留 1**（可审计）。

### 6 风险登记（零代码）：`episode::episode_count` 是全表计数

- 事实：`crates/memory/src/episode.rs:86` 的 `pub async fn episode_count` = `SELECT COUNT(*) FROM episodes`，**不分 kind**。
- 调用点状态（本会话全仓扫描 `episode_count`，含 panel/TS 与测试）：**只有定义，没有任何调用点** ⇒ **可删除候选，留待下一代清理**（本单 inScope 只有规格 + 报告，**不删**）。
- 规则：**任何「蒸馏发生过」的代理都必须按 `kind` 过滤**；若哪天有人拿这个全表计数当代理，被标记的失败尝试就会被算成成功，直接违反 §4 的通则。**今天无害的唯一原因是它没有调用点**，不是因为它的形状正确。

---

## 命名空间写词表修订（t52 / mem-core，2026-09-28；**只追加，前文一字未删未改**）

### 1 被收口的 finding（wiki 在 t51 报的跨区 finding；原文逐字引用）

> 现场读数：`POST /api/v1/memory/write` 对 `namespace:"global"` 回 **HTTP 200 `{"outcome":"RejectedNamespace"}`**，而 `"user"` 正常 `Inserted(1)`；但 **R-B 规格的命名空间词表写的是 `user | global | project:<x> | agent:<x>`** ⇒ **声称支持的一个值实际被拒**。

### 2 事实（本人 2026-09-29 在**临时 root + 临时端口**上自测，两次调用，脚本与响应体见报告）

| (store, namespace) | HTTP | 响应体 |
| --- | --- | --- |
| `observation` × `global` | 200 | `{"outcome":"RejectedNamespace"}` |
| `observation` × `user` | 200 | `{"outcome":"Inserted(1)"}` |
| `procedure` × `global` | 200 | `{"outcome":"Inserted(2)"}` |
| `lesson` × `global` | 200 | `{"outcome":"Inserted(3)"}` |
| `observation` × `project:t52` | 200 | `{"outcome":"Inserted(4)"}` |
| `observation` × `agent:x` | 200 | `{"outcome":"Inserted(5)"}` |
| `profile` × `global` | 200 | `{"outcome":"RejectedNamespace"}` |
| `profile` × `user` | 200 | `{"outcome":"Inserted(6)"}` |
| `bogus` store | 400 | `unknown store \`bogus\` (observation \| profile \| procedure \| lesson)` |
| `bogus` namespace | 400 | `invalid namespace \`bogus\` (user \| global \| project:<x> \| agent:<x>)` |

**结论**：`user \| global \| project:<x> \| agent:<x>` 是**可解析（语法）词表**，不是**可写词表**；`global` 既不是「普遍支持」也不是「不支持」——它在拥有 global 作用域的两个 store（`procedure`/`lesson`）里**写入成功**，在 `observation`/`profile` 里被治理规则拒绝。写侧真正的词表是**矩阵**（下表）。本规格此前**没有**写这个矩阵（只在 §D.5 第 794 行记过 `observation/global` 是「死组」），这就是缺口的本体。

### 3 生效的写词表（**一个真相源**：`MemoryStore::allowed_kinds`，本次起为数据；`write_vocabulary()` 由它渲染）

```
profile×{user} | observation×{user,project:<x>,agent:<x>} | procedure×{project:<x>,global} | lesson×{project:<x>,global}
```

- **可写 8 格**：`profile×user` · `observation×{user,project:<x>,agent:<x>}` · `procedure×{project:<x>,global}` · `lesson×{project:<x>,global}`
- **拒绝 8 格**：`profile×{global,project:<x>,agent:<x>}` · `observation×global` · `procedure×{user,agent:<x>}` · `lesson×{user,agent:<x>}`
- 设计 §6.1 的治理表**未变**：`global` 仍是 procedure/lesson 的跨项目作用域，Observation 仍不写 global（本修订只把这条从「代码里的 `match`」提升为「规格里的数据」）。
- **`global` 的旧措辞说明**：`user | global | project:<x> | agent:<x>` 这串字面**不在本文件里**（本会话全仓核对）；它出现在 `crates/daemon/src/api.rs:4234`（请求结构体的 doc 注释）与 `crates/store/src/migrations/0004_memory.sql:21`（列注释），并由 wiki 的 finding 引为「R-B 规格」的说法。本修订**不删改那两处**（不在本单 inScope），只在此登记：**它们是语法词表，不可读作可写词表**；三处声明与面板矩阵的路由见 §5。

### 4 拒绝必须可辨（本次代码改动，`crates/memory`）

- `WriteOutcome::RejectedNamespace` 的 `Debug` 改为**手写**（该串是**线格式**：daemon 用 `format!("{outcome:?}")` 直接放进 HTTP body，CLI 打印它，MCP 透传，面板按**首个 token** 映射 i18n），现在形如：
  `RejectedNamespace (this store does not allow that namespace; write vocabulary = profile×{user} | observation×{user,project:<x>,agent:<x>} | procedure×{project:<x>,global} | lesson×{project:<x>,global}; the refused pair is recorded in the write audit as op='reject')`
- **前三个正例字面逐字节冻结**（`Inserted(7)` / `Superseded { old: 1, new: 2 }` / `SkippedDuplicate(7)`，各有断言），因为面板的 `outcomeKey` 取首 token、`mock-agent` 用 `contains("Inserted")`、CLI/MCP 直接展示。
- **「哪一对被拒」仍在审计里**（`memory_diffs.op='reject'`，`reason='store \`observation\` cannot write namespace \`global\`'`）：变体**故意不带字段**（`distill.rs:763` 按 `W::RejectedNamespace` 匹配，加字段会让它编译不过），所以 per-request 事实走审计、可写集合走响应。
- 未采纳的替代方案（登记，不悬空）：把 HTTP 面改成 **400** 或把 body 变成结构化字段（`{outcome, store, namespace, allowed}`）属于 `api.rs`（INT/t19）——本单不改，见 §5。

### 5 路由给其它属主的声明（本单只报不改；坐标与要求）

| 声明 | 位置 | 要求 |
| --- | --- | --- |
| 请求结构体 doc 注释把**语法**词表读成**可写**词表 | `crates/daemon/src/api.rs:4234`「/// user \| global \| project:<x> \| agent:<x>」 | 改为指向 `MemoryStore::allowed_kinds` 渲染出的矩阵（或至少写明「可写性取决于 store」） |
| 列注释同样 | `crates/store/src/migrations/0004_memory.sql:21` | 同上；它描述的是**列能存的值**，不是写侧允许的组合 |
| **面板矩阵错**（会直接引导用户选到被拒的组合） | `panel/src/views/Memory.tsx:35-40` `NAMESPACES = { profile:["user"], observation:["user","global"], procedure:["global"], lesson:["global"] }` | `observation` 不得出现 `global`；且四处都缺 `project:<x>`/`agent:<x>`。改为矩阵的真值（最好由 API 下发，见下） |
| 被拒写入**仍留下 episode** | `api.rs:4267-4273`：`record_episode(McpWrite)` 在 `write_memory` 的治理检查**之前** | 临时 root 实测：10 次请求里 8 次留下 `mcp_write` episode（含 2 次被拒）⇒ episode 数**不能**当「写入发生过」的代理（与 t41 §4 的通则同族） |
| 结构化拒绝 | `api.rs:4298` `format!("{outcome:?}")` | 建议后续把拒绝序列化为字段（`store`/`namespace`/`allowed`）或 400；本单已在 `crates/memory` 侧提供 `write_vocabulary()` / `allowed_kinds()` 作为唯一来源 |

**本修订的族别**：**「声称的词表 vs 实际的词表」**，与 t30「冻结字面量必须与序列化输出同源」同族 —— 两者的病都是**同一个事实有两个来源**（此处：语法词表与写矩阵；t30：字面量与序列化输出），治法也一样：**收敛到一个来源，并让断言（而非注释）守住它**。

---

## 图证据进注入面（t58 / F5 / G10，2026-09-28；**只追加，前文一字未删未改**）

### 1 现场（V-INT 的 F5 + 我自己的复读）

V-INT 在**含图证据的同一批 turn** 上读了 5 个 `context_injected`：块分布 = `knowledge 5/5 + wiki 5/5 + **graph 0/5**`（同根 `/recall` 读到 `graph.entities=1..5`、`graph.paths>0`，所以不是「没样本」）。我的复读（`Select-String`，2026-09-28）：`TAG_GRAPH|graph_items|graph_evidence_items|GraphEvidence` 的命中只落在 `crates/memory/src/inject.rs`（15 处）与 `crates/daemon/src/memembed.rs`（10 处，含本单新增的适配器与它的测试）；**`chat.rs`/`runs.rs` 命中 0 处**，`graph_evidence_items(` 的调用者只有它自己的定义与测试 ⇒ **生产的 `<graph>` 计数仍是 0/5**。

### 2 裁决（二选一）—— **选 (a)，并在 inScope 内做到边界**

- **inScope 内落地（本代已做）**：契约侧 `inject::graph_items(lines, limit)` + `inject::GRAPH_PATHS`；适配点侧 `memembed::graph_evidence_items(&GraphEvidence, limit)`。
- **inScope 外（本代不改，交回 captain）**：两个**生产构造点**在 `crates/daemon/src/chat.rs` 与 `crates/daemon/src/runs.rs`，是 t58 的 outOfScope，因此**本代结束时生产的 `<graph>` 仍是 0/5**。
- **G10 的判定：未达标（NOT not_measured）**。生产者已交付（`crates/graph::retrieve` + `GraphEvidence::lines()`），契约与适配器也已就位，**缺的只是一次 5 行的调用**；把它记成 not_measured 等于把「没人接线」写成「没有依赖」。**owner = 下一步入口（持有 `chat.rs`/`runs.rs` 写权的那张单）**。

### 3 本代落地的契约（冻结形状，只追加）

```rust
pub const GRAPH_PATHS: usize = 3;                       // 预算：一个数，不是调用点的魔法字面量
pub fn graph_items(lines: &[String], limit: usize) -> Vec<ContextItem>;   // 契约拥有 tag/块形/预算
```
- `crates/memory` **不依赖 `crates/graph`**（依赖方向：memory → core + store）⇒ 契约收的是**已渲染的文本行**，渲染本身归 `crates/graph` 的 `GraphEvidence::lines()`（一行内含 `hops / valid_at / state / edges / source`）。
- 不发明内容、不放占位符、不给行加日期：**空输入 ⇒ 空 Vec ⇒ 调用方不发块**（与 `knowledge_items` 同一条诚实失败规则）；**空白行被丢弃**（空行留在块里会被读成「有证据但内容缺失」）。
- 块位置/丢弃顺序不变：`tag_rank(graph)=3`（knowledge 2 与 wiki 4 之间，`D.2` 已冻结）；总预算按 `tag_rank` 从最后开始丢，因此 graph 比 wiki/project_context **更晚**被丢。

### 4 适配点（`crates/daemon/src/memembed.rs`，唯一转换处）

```rust
pub fn graph_evidence_items(e: &ruagent_graph::GraphEvidence, limit: usize)
    -> Vec<ruagent_memory::inject::ContextItem>;   // = inject::graph_items(&e.lines(), limit)
```
本单的三条读数（逐字见报告）：有路径 ⇒ `<graph>` 块；无路径 ⇒ `items=0` 且 `render=""`；10 行 + `limit=3` ⇒ 恰 3 行；`per_block=120` 与 `total=120` 两种绑定下**截断可见、块被丢有计数、`chars ≤ total` 成立**。

### 5 剩余工作（确切的下一步入口；本单不改这两处）

| 位置 | 插在哪 | 形状 |
| --- | --- | --- |
| `crates/daemon/src/chat.rs` | `knowledge_items_enriched(...)` 那段 `if let` **之后**（第 602 行 `}` 与第 604 行注释之间） | `if let Ok(evidence) = ruagent_graph::retrieve(db, &ruagent_graph::GraphQuery::for_text(query)).await { items.extend(crate::memembed::graph_evidence_items(&evidence, ruagent_memory::inject::GRAPH_PATHS)); }` |
| `crates/daemon/src/runs.rs` | 知识块 `if let` **之后**（第 1926 行 `}` 与第 1927 行注释之间） | 同上，query 换成 `run_query(task)` |

两处都**不需要新依赖**（daemon 已依赖 `ruagent_graph`），也**不需要新参数**（`db`/`query` 都在作用域内）。**接线后必须给 K-5 要求的盘上读数**（至少一条带 `<graph>` 的 `context_injected` 事件）；在此之前，**生产的 `<graph>` 计数是 0/5，G10 未达标**。

---

## 预算/丢块报告：`budget` 恒 null 的收口（t60 / INT-F6，2026-09-28；**只追加，前文一字未删未改**）

### 1 现场（V-INT t20 的读数 + 我自己的代码侧复读）

| 读数 | 来源/时间 | 值 |
| --- | --- | --- |
| `context_injected` 5 条事件里 `budget` 非 null 的条数 | V-INT t20/t21（`gen2-integration-verify.md:38`、`gen2-integration-review.md:83`） | **0/5（恒 null）**；`path` 5/5 |
| 为什么恒 null（代码侧，我今天复读） | `crates/daemon/src/runs.rs:1278` `budget: None,`；`crates/acp/src/chat.rs:505` `budget: None,` | 两处的注释自己写明：`render_context` **只返回字符串**，所以只能从渲染文本**反推** `blocks[]`；t19 §9-3 据此登记为 `not_measured`，而反推=把「看起来对」当读数 |

**它导致两条锚点不可判（不是「未跑到」，是数据从不产生）**：
- **A-2（§6.1）的预算半边**：「含 `budget` 的事件数 ÷ 总数」「`used_chars ≤ 4096`」「`blocks[]` 的 tag 集合与渲染一致」「造一次预算绑定 ⇒ `dropped_items>0`」——半条都读不出来。
- **N-6（§6.2）**：「渲染里出现丢块时，事件里 `budget.dropped_items>0`」——事件里没有这个字段。

### 2 契约自相矛盾（原文逐字引用）

- **§3.2 第 198 行**：`| budget [新增] | 对象 | 见下；**null = 本次未采集**（不许写全 0 假装采集了） |`
- **§6.1 A-2 第 449 行**：要求「各自给出『含 `budget`/`path` 的注入事件数 ÷ 总注入事件数』…`budget.used_chars ≤ 4096`、`blocks[]` 的 tag 集合与渲染一致；造一次预算绑定 ⇒ `dropped_items>0`」
- **§6.2 N-6 第 473 行**：「渲染里出现丢块时，事件里 `budget.dropped_items>0`」

⇒ §3.2 把「本次未采集」**允许为一个终态**，而 §6.1/§6.2 要求它**必然被采集**。二者今天不能同时为真 —— 与 **t41 裁过的同形冲突**（R-B 的 D4 允许 `None` vs D2(b) 要求必填）是同一族：**「允许缺省」与「要求取值」被写在同一份契约里，而没有任何东西裁决谁赢**。

### 3 裁决：**(a) 落地** —— 渲染侧给一份可序列化报告，`render_context` 字节逐字节不变

```rust
pub struct BlockReport { tag: &'static str, items: u32, chars: usize, truncated_chars: u32, dropped_items: u32 }
pub struct BudgetReport { per_block_chars: usize, total_chars: usize, used_chars: usize,
                          truncated_blocks: u32, dropped_items: u32, blocks: Vec<BlockReport>,
                          notes: Option<String> }   // §3.2 的可选字段，未设则不序列化
pub fn render_context_report(items: &[ContextItem], budget: &InjectionBudget) -> (String, BudgetReport);
pub fn render_context(items: &[ContextItem], budget: &InjectionBudget) -> String {
    render_context_report(items, budget).0      // 一个代码路径、一份账 ⇒ 报告与字节不可能分家
}
```
- **字节不变**：6 个规范夹具的渲染与**改前**逐字节相同（含 `… [+190 chars truncated]`、`… [+2 dropped]`、以及两个空渲染），由 `the_report_refactor_moved_no_byte` 钉住；原有的 `golden_render`、三条不变量测试与 `never_exceeds_total` 性质测试全绿；新增性质测试 `the_report_never_disagrees_with_the_render` 对**任意输入**断言报告与渲染一致（`used_chars == render.chars().count()`、无丢块时块字符和 == 渲染、截断计数 == 渲染里的标记数、`blocks[]` 的 tag 序列 == 渲染里的 tag 块序列）。
- **形状即契约 §3.2 的字段表**（逐字段断言，不是「加了 serde 就算」）：`per_block_chars/total_chars/used_chars/truncated_blocks/dropped_items/blocks[]`，`blocks[]={tag,items,chars,truncated_chars,dropped_items}`，`notes` 缺省省略。
- **负控/正控**：无丢块 ⇒ `dropped_items=0`、`truncated_blocks=0`（**0 是读数，不是 null**）；有丢块 ⇒ `>0`；只有截断的渲染 ⇒ `truncated_blocks=1` 且 `lost_anything()==true`。

### 4 剩余工作（inScope 之外；**未达成，不是 not_measured**）

两个**事件构造点**不在本单 inScope（`crates/daemon/src/runs.rs`、`crates/acp/src/chat.rs`）：

| 位置 | 现在 | 需要的形状 |
| --- | --- | --- |
| `crates/daemon/src/runs.rs:1270-1279`（run 路径） | `budget: None`（注释：契约没有报告型渲染） | `render_run_injection` 改回 `(String, BudgetReport)`（或让上游带出报告），事件处 `budget: Some(serde_json::to_value(&report)?)` |
| `crates/acp/src/chat.rs:496-505`（chat 路径） | `budget: None` | 同上；该适配器持有 `injection_context` 的返回，需要它把报告一起交出来 |

⇒ **A-2 的预算半边与 N-6 仍是「未达成」**：仪器已就位（本单），但从事件到仪器的那一段线还没接；**owner = 持这两个文件写权的那张单**。在此之前，任何「`budget` 已达标」的说法都是假的；反过来，把今天的状态写成 `not_measured` 也是假的 —— 它今天可判为**未达成**（有确切构造点、确切形状、确切 owner）。

### 5 一处诚实的限制（建议 §3.2 补一个字段，路由给 INT）

`blocks[]` 只列**发出去的**块（否则 A-2 的「tag 集合与渲染一致」在读侧会被判失败）。代价是：**丢的是哪个 tag**（§3.3 问题 2）在冻结字段里答不出来 —— 只能从渲染文本的 `<context_budget>` 通知看「丢了几条」。若 §6.1 A-2 / §3.3 Q2 要保留「哪个 tag 被丢」，§3.2 需要加一个 `dropped_tags: [tag]`（或把丢块以 `chars=0` 也列进 `blocks[]` 并明确 tag 集合的语义是「⊇」）。**本单不改契约**（不在 inScope），只登记这个二选一。
