# gen2 Wiki 实现报告（I-D）

> 单号 t10 · attempt `34f9f93a-0a1c-45ff-a8ae-e9b4b0523bbc` · 时间窗 2026-09-27 21:5x – 2026-09-28 04:0x（+08:00）
> 上游：`docs/design/reviews/gen2-wiki-spec.md`（t4，I-D 的可执行规格）· schema：`crates/store/src/migrations/0022_wiki_gen2.sql`（I-SCHEMA，t6）
> inScope（只有这两条路径）：`crates/daemon/src/wiki.rs`、`crates/mock-agent/tests/wiki_pipeline.rs`
> 交付：本报告 + 上述两个文件（+ 无仓库内其它改动）
>
> **修订记录 1（t10 交付后，captain 裁决 D7）**：`no content section` 由「超出冻结枚举的独立 bail」改为 `CiteProblemKind` 的**第 7 个具名变体**（`NoContentSection`），并配一条**只归因给它自己**的触发用例；D3 按裁决作为**破坏性变更**保留登记。这次修订落在 **V-D(t14) 开工之前**（当时 t14 仍是 `pending`），所以 V-D 验的就是这一版；读数由 13 passed 变为 **14 passed**。
> **修订记录 2（t34 修复单，RV-D 六条 finding）**：RV-D 判 `needs_revision`（G1 硬门的**读数半边**：`cite_coverage` 是构造性满分）。本修复把覆盖率的取数口换成**构建期记录值 + 第三态**（RV-D-1），并处理 RV-D-2（措辞）、RV-D-3（`stale_reasons`/`unknown_cause`）、RV-D-4（空表登记）、RV-D-5（`frozen_by` 口径）、RV-D-6（计数）。**写侧准入逻辑一字未改**。读数：`wiki_pipeline` **14 → 15 passed / 0 failed**（改前 4）；`daemon --lib` **72 → 76 passed / 0 failed**。逐条见 **§5bis**，新判据见 §8.3 R0。
> **修订记录 3（t56，2026-09-28，只有措辞，无判据/读数改动）**：§4 的单源强制点表里 `links_out` 一行的判据写成 `pages[i].links_out == links.degrees[slug].links_out` —— `degrees` 实际是 `Vec<PageDegree>`（可 `[i]` 索引的数组），`[slug]` 这种写法**读起来像 map**，而它**已经误导过一次**（t51 我按 map 形状写第一版断言、被测试当场打红；集成契约 L78 从头就是数组形状）。**旧文字逐字引用**：`` `the_two_graph_endpoints_cannot_disagree` 对每页断言 `pages[i].links_out == links.degrees[slug].links_out` 且 `links_in`/`links_out_broken` 同样逐位相等 ``；**新文字**：`degrees` **数组中该 slug 的那一行**（`PageDegree`；`degrees` 是 `Vec<PageDegree>`，**不是**以 slug 为键的 map）。**不改**任何断言、读数或代码。逐条分类见 `gen2-wiki-shorthand.md`。

**本文的读法**：§1 是一张「改前 → 改后」总表（G1–G9），§2 逐条讲机制，§3 是两侧证据与**旧代码反证**，§4 是单源强制点，§5 是**偏差与未落地登记**，**§5bis 是 t34 修复（RV-D 六条 finding）**，§6 是对 I-INT 的接线请求，§7 对注入契约的影响，§8 复现命令与纪律声明，§9 风险与不确定。

**结论（四行）**
1. **t4 的四条硬门（G1/G2/G3/G7）全部有可证伪判据且已在集成测试里跑到「坏侧真的红」**：**7 个**引证缺陷族逐条 `failed`（含专属于第 7 族的归因用例），`{"dry_run":true,"confirm_plan":N}` ⇒ 400，来源节缺失/不一致逐条点名 kind。**G1 的准入半边一直是真的**（V-D 自造 7 族 7/7 被拒、旧码 8/8 落地）；被 RV-D 推翻的是**读数**半边，已在 §5bis.1 修好并重读。
2. **读数**：`test -p ruagent-mock-agent --test wiki_pipeline` **4 passed →（t10）14 →（t34）15 passed / 0 failed**；`test -p ruagent-daemon --lib` **72 → 76 passed / 0 failed**；`check -p ruagent-daemon --all-targets` 绿；`clippy -p ruagent-daemon --all-targets -- -D warnings` 绿（**强制重查**后取得）。旧代码（gen2 之前的二进制，sha256 `CB8B55A0…`）上**同一 fixture 6/6 全部以 `written` 落地**（§3.4）。
3. **一条目标未达成**：G8（注入块标记 2/2）的生产侧数据已就绪，但消费端在 `crates/memory` + `chat.rs`/`runs.rs`，不在本单 inScope ⇒ 记 `not_measured` 并给出接线要求（§5-D1、§6）；**且这条的达标证据原本受 RV-D-1 污染**，修好后必须重读（§5bis.3）。其余偏差与半落地逐条列在 §5。
4. **契约 verify 三条**：`test -p ruagent-mock-agent --test wiki_pipeline` ✅、`clippy -p ruagent-daemon --all-targets -- -D warnings` ✅（**强制重查后**取得，§8.3 R3）、`git status --porcelain -- crates/store crates/knowledge crates/memory crates/graph panel` = 同伴的在途编辑、**无一条来自本单**（§8.3 R4）。

---

## 1. G1–G9：改前 → 改后

`before` 全部来自 t4 规格 A.1 的现场读数（只读探针，21:33–22:37 +08:00，对象集与采样面见该表），**不是我这一单重新采的**；`after` 分两类：**可判据**（测试里断言，见 §3）和**结构性**（单源强制点，见 §4）。凡本代拿不到读数的，写 `not_measured` 并给原因，不写 0。

| G | metric | before（现场，+08:00） | after（本代） | 状态 |
| --- | --- | --- | --- | --- |
| **G1** | `cite_coverage = cited_content_sections / content_sections` | **0/10 = 0.00**（4 页；B-08：`PageMeta` 里 chunk 级锚 0 处）+ 活体反例 B-21（22:29:50） | **准入**：任一 content section 无锚 ⇒ 该页 `failed`；`PageMeta.citations`/`verified` 由 daemon 序列化写入页文件。**读数**：覆盖率取**构建期记录值**（`report.coverage` → `wiki_pages.cite_coverage`），且只在页面**仍 fresh** 时敢报，否则 `null`（unknown）—— t10 首版这条读数是**构造性满分**，见下注 | ⚠️→✅ **t10 首版读数受 RV-D-1 污染，本修复已换实现**：`dry_run_plan_then_confirmed_build_lands_pages`（fresh 页 `1.0` + 不变量）+ `an_unresolvable_anchor_cannot_keep_reporting_full_coverage`（锚失效后 `null` + `uncited` 非空）+ `the_citation_gate_fails_a_page_whose_claims_cannot_be_traced` |
| **G2** | 三条独立比率 | 2a 缺 `## 来源` **1/1 = 1.00**（B-21）；2b 来源节与 frontmatter 不一致 **0/4**；2c 未引用节 **10/10** | **7 个**缺陷族逐条 `failed` 且 `error` 点名 kind：`uncited section` / `missing sources section` / `sources section mismatch` / `dangling citation` / `unaligned citation` / **`no content section`** / `unknown chunk`（不可判定 ≠ 通过） | ✅ 判据（6 例 × 2 侧 + `a_page_with_nothing_to_cite_fails_by_its_own_name` 对第 7 族的**归因**用例 + 单测 `cite_problem_kinds_have_distinct_words` 保证 7 个词互不相同；**旧代码 6/6 全部落地**，见 §3.4） |
| **G3** | `stale_persistence`、`freshness_consistency` | `stale_since` 字段不存在（**not_measured**）；四面不一致 **1/1**（B-11：API 说 `stale=true`，`index.md` 无标记） | `stale_since` 由「下一次构建」写入并只写一次；`stale_sources` 给名字；`index.md` 出 `⚠️ 源已更新`；`/pages`、`/links`、`index.md` 三面同源（§4）；第三态 = 「KB 里还没有可解析的证据」时 `freshness="unknown"`（**不是 `fresh`**），且 D.2 的冻结不变量 `stale == (freshness=="stale")` 成立 | ✅ 判据（`a_page_that_stays_stale_records_since_and_says_why` + `an_unanswerable_freshness_reading_is_unknown_never_fresh`；`/pages` 与 `index.md` 逐位一致） |
| **G4** | `invalidation_action_rate` | **not_measured**（真库 0 个 stale 页 ⇒ 分母 0，不能写 0/4） | stale 且未冻结的页**不允许以 `keep` 结束**：计划说 `keep` 也被执行器改写为 `update` 并重建，行里记的是**实际执行的动作** `action='update', status='written'`；重建后 `stale=false` | ✅ 判据（`a_stale_page_is_repaired_even_when_the_plan_says_keep`：同一页先 `keep` 后重建 + `built_at` 变化） |
| **G5** | 四条图读数 | 5a `graph_inconsistency` **2/4**（B-14）；5b `wanted_attribution` **0/5**（B-15）；5c 持久化 **0**（无表）；5d 自链被静默丢弃（B-15/16） | 5a = 0（`WikiPageInfo.links_out` 直接取 `LinkGraph.degrees`，§4）；5b = 1.00（`wanted[]` 带 `demanders`/`demand_count`）；5c = 每个 `done` 构建一行 `wiki_graph_readings`；5d 自链进 `self_links` 且**不计入** `links_out` | ✅ 判据（`the_two_graph_endpoints_cannot_disagree`：逐页 `links_out/links_in/links_out_broken` 与 `degrees` 逐位相等 + 读数行落库） |
| **G6** | `frozen_page_rate` | **not_measured**（真库无冻结页；B-06 edited=0/4） | 任何被 skip 的页都有一条 `wiki_corrections`（§13-3 自动 `note`；pin 由写入方给 reason）；行 `error` 里带原因 | ✅ 判据（`a_page_that_stays_stale_records_since_and_says_why` 的 `note` 行 + `a_pin_skips_the_page_and_the_reason_is_visible` 的 pin/release，含**空 reason 必须被拒**的负例）；**读数口径按 RV-D-5 收窄**：`frozen_by` **仅 pin 路径非空**（见 §5-D14） |
| **G7** | 三条表示/行为读数 | 7a plan-only 存储词汇 **3**（`planned` / `planned_only` / `dry_run=1`）+ 1 个响应词汇；7b 组合被静默忽略 **1**（B-21 D1c）；7c `pending` 混淆 **22/41 = 53.7%** | 7a：`DRY_RUN_STATUS` 由 `planned_only` 改为 `planned`，响应与行同一词汇（legacy 两行由 0022 回填 + 触发器钉死）；7b：同时给 `dry_run` 与 `confirm_plan` ⇒ **400 且错误信息点名两个字段**，不写任何行；7c：只读计划的页行写 `planned`（`PLAN_ONLY_ROW_STATUS`），执行行才写 `pending` | ✅ 判据（`dry_run_has_one_vocabulary_and_rejects_the_combined_request`：响应/行/页行三级词一致 + 组合 400 + 列表里再无 `planned_only`） |
| **G8** | `wiki_block_lead_marks` | **0/2**（B-18：块行是 `wiki/<slug>: <chunk 正文>`） | **生产侧已就绪**：`WikiLead`（`stale`/`stale_since`/`edited`/`has_anchors`/`anchored_sections`/`cite_coverage`/`anchors`/`hint`/`summary`）+ `lead_for()`/`lead_from()`；`recall_stubs` 已带 `anchors`/`hint`/`cite_coverage`/`stale_since`/`has_anchors` | ⚠️ **not_measured**（消费端在 `crates/memory`（I-B）+ `chat.rs`/`runs.rs`（I-INT），**不在 I-D 的 inScope**）；见 §5-D1。**且本条原标定读数受 RV-D-1 污染**（注入的 `cite_coverage` 曾是构造性常量），修好后必须重读 —— 见 §5-D13 |
| **G9** | `slug_identity_churn` | **1**（`rose-gardening` build 3 delete → `gardening-roses` build 4 create） | 一次构建内「删 X + 建 Y 且 Y 复用 X 的来源」被识别为**改名**：X 的行 `renamed` + `error="renamed to Y"`，Y 的 `aliases` 追加旧 slug，`deleted` 行数 = 0 | ✅ 判据（`a_rename_moves_the_topic_instead_of_destroying_it`：无 `deleted` 行 + 旧 slug 出现在幸存页 `aliases`） |
| **E10** | `wiki_page_hashes` 泄漏（附加读数） | 真库 **5 行 / 4 页**（B-06，`doctor-probe` 无文件无文档） | 每次构建开头对账：无文件且无索引文档的 hash 行被删；泄漏行 **1 → 0** | ✅ 判据（`a_page_hash_with_no_page_is_swept_by_the_next_build`） |

**t4 规定的硬门**（G1 ∧ G2 ∧ G3 ∧ G7）四条全部有「坏的一侧真的红」的读数（§3.2），且 G3 的第三态另有独立用例（§3.3）；读数门 G4/G5/G6/G9 已有成对读数；**G8 是唯一 not_measured**，原因与归属见 §5-D1。

---

## 2. 机制（每条目标落在哪个函数）

### 2.1 引用可验证（G1/G2，`wiki.rs` 新增 ~330 行）

- **锚注释**：`CITE_PREFIX = "<!-- cite:"` / `CITE_SUFFIX = "-->"`，正文里一行一条 `<!-- cite: <document>#<chunk_id> -->`；`citations_in(body)` 解析并**跳过代码围栏**，返回 `(section 标题, document, chunk_id)`。
- **节切分**：`page_sections(body)` 按 `## ` 切（同样跳围栏）；`is_content_section(title)` 排除 `来源`/`相关页面`/`相关记录`（`NON_CONTENT_SECTIONS`）。**「相关页面」这类导航节不算声称**，所以不被要求带锚——否则每页都会被自己的导航链判死。
- **来源节**：`sources_section(body)` → `(存在?, 名单)`。
- **活 chunk 解析**：`chunk_index(kb, names)` → `(document, chunk_id) → chunk 正文` 的 map，数据来自 `Knowledge::document_chunks`；`chunk_hash` 由 daemon 侧算 `sha256_hex(正文)`。
- **闸门**：`verify_page(kb, slug, planned_sources, body) -> CiteReport`，产出 `content_sections` / `cited_sections` / `citations` / `coverage` / `problems: Vec<CiteProblem>`；`CiteProblemKind` **七个**值，`as_str()` 就是写进行 `error` 的那串词：`uncited section`、`missing sources section`、`sources section mismatch`、`dangling citation`、`unaligned citation`、`no content section`、`unknown chunk`。
- **落地点**：`run_page` 的 stage 3b。任一 problem ⇒ `anyhow::bail!(report.error_line())` ⇒ 该页 `failed`，**文件不写**（页文件是真相源，失败的页不能进库存）。空壳（`content_sections.is_empty()`）走 `verify_page` 的 **rule 0** 报成具名族 `no content section` —— 空集上的全称命题是真空通过，不是通过（captain 裁决 D7，§5-D7）。
- **frontmatter**：`PageMeta` 增 `verified: VerifyState`（`unverified|verified|failed`，默认 `Unverified`）与 `citations: Vec<Citation>`；序列化在后（`verified:` 一行 + `citations:` 块列表）；解析容错——未知键跳过、锚条目缺字段则**丢掉该条并把 `verified` 降级为 `Unverified`**、frontmatter 未闭合则整页 `None`（这是原有契约，本代没有放松）。
- **writer prompt**：来源不再以整篇文本喂进去，而是 `<document name="X"><chunk id="N">…</chunk></document>`：锚必须指向**检索单元**，不能指向一个文件；来源文档在 KB 里没有 chunk 时直接 bail，而不是喂一个没有 id 的文本。

**证不了什么（诚实边界）**：`verify_page` 能证明**可解析性 + 对齐 + chunk 新鲜度 + 每节覆盖**，**不能**证明「正文那句话被 chunk 蕴含」（没有 NLI 模型，这是 t4 B1 里不采纳 ALCE 精度半边的代价）。所以 `verified` 的语义是「每条声称都有活证据指路」，不是「每条声称都被证实」。

### 2.2 新鲜度与失效（G3/G4）

- **唯一计算点**：`freshness_with(kb, recorded_hash, slug, meta) -> PageFreshness`（`freshness()` 是它的 `recorded_hash=None` 薄封装）。`StaleReason` 五值：`SourceHashDrift` / `SourceMissing` / `ChunkMissing` / `ChunkHashDrift` / `HandEdited`，每个有冻结词（`as_str()`，RV-D-3）；`PageFreshness` 带 `built_at`/`stale: Option<bool>`/`stale_since`/`reasons`/`stale_sources`/`drifted_citations`/`unknown_cause`。
- **第三态（措辞已按 RV-D-2 对齐，实现未变）**：**KB 整体没有可用的 documents row**（`chunk_index` 报 unknown，`kb_docs == 0`）⇒ `stale = None` + `unknown_cause`，不猜成 `false`；而**页点名的来源消失/对不上**是一种**确定的**失效 ⇒ `stale = true` + `StaleReason::SourceMissing`/`SourceHashDrift`（还有 `ChunkMissing`/`ChunkHashDrift`）。证据：V-D 的两个夹具 `VD T8-A`（空 KB ⇒ `freshness="unknown"`）与 `VD T8-B`（非空 KB + 页里点名的 `never-src` 无 row ⇒ `freshness="stale"`），见 `gen2-wiki-verify.md` §5-F2。**判据的措辞改的是规格，不是行为**：原措辞写「KB 无该来源 row 时必须 `unknown`」，与 D.5 自己的 `StaleReason` 枚举（把「来源消失」列为 stale 的原因）冲突，captain 裁决保留实现、改措辞。
- **"为什么"必须可答（RV-D-3）**：`/wiki/pages` 除 `stale_sources` 外给出 `stale_reasons`（原因词，stale 时非空）与 `unknown_cause`（仅 `unknown` 时非空）。`stale_sources` **只在页面记了 `source_hashes` 时才可能有名字**（V-D 的夹具 `source_hashes: {}` ⇒ `stale=true` 而 `stale_sources=[]`，两种形态都写进 §8.3 R5）；`stale_reasons` 不受此限，因此"为什么 stale"永远可答。
- **同步半支**：`source_drift(kb, meta)` 只做「来源名 → 记录 hash vs 当前 hash」的比对（`source_drift` 是 `freshness_with` 内部的同一段逻辑抽出来的）：给 `lead_from`（HTTP 召回 stub 是同步路径）和 `entity_related_pages` 用。代价写在 §5-D2。
- **持久化**：`freshness_all(db, kb)` 遍历磁盘一次；`record_invalidation(db, &[PageFreshness])` 把结果 upsert 进 `wiki_pages`，`stale_since = CASE WHEN excluded.stale=1 THEN COALESCE(wiki_pages.stale_since, excluded.stale_since) ELSE NULL END`（**首次观测时刻只写一次**，成功重建即清零）；同一处写 `uncertain_markers`（正文里 `⚠️` 的计数）、`content_hash`、`edited`。
- **观测窗口（必须写清）**：`stale_since` 的语义是「**首次被某次构建观测到的时刻**」，不是「漂移发生的时刻」；在构建之间，`freshness` 能报 `stale=true`+`stale_sources`，但 `stale_since` 仍是 `null`。这条在 `a_stale_page_is_repaired_even_when_the_plan_says_keep` 里被正面断言（改写来源后立刻读：`stale=true` 而 `stale_since=null`），免得下游把它读成 bug。
- **失效传播**：`scope_sources` 的 `Scope::Changed` 除「来源变了」外，**也把 stale 页的来源拉进本次构建**；`run_page` 对 stale 的 `keep` 不再放行（§1 G4）。
- **PageFreshness 读 API**：`pages()` 每页带 `freshness`（三态字符串）+ `stale`（bool，兼容旧消费者）+ `stale_since`/`stale_sources`/`built_at`/`edited`。

### 2.3 链接图（G5）

- **唯一图计算**：`link_graph(&[(slug, targets)]) -> LinkGraph`，一次遍历同时产出 `nodes` / `edges` / `wanted` / `self_links` / `degrees` / `orphans` / `unreachable`。
- `wanted: Vec<WantedPage{slug, demanders, demand_count}>`：断链目标**带需求方**（B-15 的 0/5 → 5/5）。`broken` 保留为 `wanted` 的 slug 列表，旧消费者不变。
- `PageDegree{links_in, links_out, links_out_broken}`：`links_out` = 去重后的**非自链**目标（含断链），`links_out_broken` 单独报；`links_out == 解析成的出边 + links_out_broken` 是构造性成立的，测试逐页断言。
- `self_links`：`[[x]]` 写在 `x` 自己正文里 ⇒ 进 `self_links`，**不计入 `links_out`**（B-14 里 `cooking-pasta` 报 3 出度而图出度 0，一半原因是那条自链）。
- `unreachable`：没有任何入边的节点（与 `orphans` 同源但分开命名：`orphans` 保持旧语义「没被别的页链入」）。
- **持久化**：`record_graph_reading(db, kb, build_id)` 每个完成构建写一行 `wiki_graph_readings`（`nodes/edges/broken/orphans/unreachable/self_links/read_at`）并刷新每页度数；读回用 `graph_reading(db, id)`。
- **消费面**：`pages()` 用 `link_graph` 的度数填 `links_out`/`links_in`/`links_out_broken`/`self_links`，`links()` 把同一个 `LinkGraph` 序列化——**两个端点不可能不一致**（§4）。

### 2.4 干跑单源（G7）

- 常量：`DRY_RUN_STATUS = "planned"`（原 `"planned_only"`）、`PENDING_ROW_STATUS = "pending"`、`PLAN_ONLY_ROW_STATUS = "planned"`、`RENAMED_ROW_STATUS = "renamed"`。
- `start_build_inner`：`confirm_plan.is_some() && dry_run` ⇒ 400，错误串**同时点名 `dry_run` 与 `confirm_plan`**（原文：`"dry_run and confirm_plan are mutually exclusive: ..."`），且**不写任何行**（在落行之前返回）。
- 只读计划的页行：`insert_build_pages(db, build_id, pages, PLAN_ONLY_ROW_STATUS)`；执行路径传 `PENDING_ROW_STATUS`。
- 存储词汇 3 → 2（`planned` + `dry_run` 列由 0022 的触发器钉成函数依赖；路线 A 再降到 1 留给后续）。

### 2.5 纠错回路（G6）与改名（G9）

- `CorrectionKind{Pin, Release, Note}` + `Correction{slug, kind, reason, author, at}`；`add_correction` 拒绝空 reason / 空 author（**在写之前** return Err，不是靠 DB CHECK 兜底）；`corrections(db, slug)` 列出；`active_correction(db, slug)` = 「最近一次 pin 之后没有 release」。
- `run_page` 的顺序：**pin 冻结** → 改名识别 → 计划动作（`keep` 只对不 stale 的页生效）→ `delete` 守卫（引用来源仍存在 ⇒ skip）→ **§13-3 手改冻结**（同时写一条 `note` 记录，`author=daemon`）→ writer → 引证闸门 → 落盘。
- 冻结一定**可解释**：pin 的 `error` 是 `frozen: <reason> (by <author> at <at>)`；手改是 `human-edited since last build — skipped (§13-3); recorded: human-edited since build #N`。
- 改名：`rename_pairs(plan, kb)` 找出「`delete` X + 同来源的 `create`/`update` Y」的对；X 走 `retire_page`（删文件、删索引文档、删 hash 行），返回 `PageLanded::Renamed{to}`，行写 `renamed` + `renamed to Y`；Y 落盘时把旧 slug 追加进 `aliases`（`aliases` 过滤掉「仍然存在的页」，所以同一 slug 的往返不会自引用）。
- **实际执行的动作**：`PageLanded::Written{action}` + `set_page_outcome(...)`——计划里的 `keep` 被 stale 修复覆盖后，行里记 `action='update'`，被请求的动作仍留在 `wiki_builds.plan_json`（G4 问的正是「这一页被真的动过吗」）。

### 2.6 对账（E10）

`reconcile_page_hashes(db, kb)`：`wiki_page_hashes` 里出现但**磁盘无页文件且 KB 无对应文档**的 slug 行被删除，返回删除条数；在 `execute_build_inner` 开头调用（每次构建都自愈）。现场反例是 `doctor-probe`（B-06：5 行 hash / 4 页）。

---

## 3. 两侧证据（好侧必须绿、坏侧必须红）

### 3.1 好侧（`dry_run_plan_then_confirmed_build_lands_pages`）

判据（全部在 `crates/mock-agent/tests/wiki_pipeline.rs`，临时 root，脚本化 mock agent）：

| 断言 | 读数 |
| --- | --- |
| 两个来源 PUT 后回读 chunk id | `deploy-guide → [1]`、`tea-doc → [2]`（**先测量、再引用**：chunk id 变了会在这里红，而不是三步之后变成莫名的 `dangling citation`） |
| dry-run 响应 | `status=planned`、`pages_planned=2`，且**没有页文件** |
| 确认执行 | `done`、`pages_written=2`，两页 `status=written` |
| 页 frontmatter（daemon 唯一写者） | `verified: verified`、`citations:`、`chunk_id: 1`、`chunk_hash:` |
| 读 API | 两页 `cite_coverage = 1.0`、`freshness = fresh` |
| 搜索面 | `wiki/deploy-pipeline` 命中 `deploy.sh` |

### 3.2 坏侧（`the_citation_gate_fails_a_page_whose_claims_cannot_be_traced`）

**同一 rig、同一来源**，只换 writer 正文；六个缺陷族逐条断言 `pages_written=0 && pages_failed=1 && status=failed`，`error` 必须含下表的 kind 词，且**页文件不存在**、`/wiki/pages` 里也没有它（失败的页不进库存）——第 7 族另有自己的用例（下一小节）：

| 缺陷族 | 正文特征 | 期望 kind 词 |
| --- | --- | --- |
| 有节无锚（凭空事实） | 两节只给一节锚 | `uncited section` |
| 完全没有来源节 | 只有锚、无 `## 来源` | `missing sources section` |
| 锚指向不存在的 chunk | `src-alpha#999` | `dangling citation` |
| 锚指向非本页来源 | `other-doc#1` | `unaligned citation` |
| 来源节多列了不引用的文档 | `- src-alpha` + `- other-doc` | `sources section mismatch` |
| 没有可引用的内容节 | 只有 `## 来源` | `no content section` |

另外三条负例在别的用例里：`{"dry_run":true,"confirm_plan":N}` ⇒ 400（`dry_run_has_one_vocabulary_and_rejects_the_combined_request`）、空 reason 的 pin ⇒ `Err`（`a_pin_skips_the_page_and_the_reason_is_visible`）、把仍有存活来源的页 `delete` ⇒ `skipped`（`delete_guard_protects_pages_with_live_sources`，2026-09-14 事故回归）。

**第 7 族（`no content section`）单独成案**：`a_page_with_nothing_to_cite_fails_by_its_own_name`。它不只是「这条也在那一轮里红」，而是断言**归因正确**——错误串里出现 `no content section`，同时**必须不出现** `uncited section` / `missing sources section` / `sources section mismatch` / `dangling citation`（该页确实没有内容节、且来源节存在且正确）。理由（captain 裁决 D7）：`verify_page` 的其余规则都是「**每个** content section 都带锚」这种**全称命题**，而全称命题在**空集上恒真** ⇒ 没有这一族，一个空壳（`# T` + 来源列表）会以 `written` 落地。**判据必须在空集上失败，而不是通过。** 另配单测 `cite_problem_kinds_have_distinct_words`：7 个族的词两两不同、都非空（两个族共用一个词会让 `error` 行无法审计）。

### 3.3 第三态：不可判定 ≠ 新鲜（G3 的硬门要素）

`an_unanswerable_freshness_reading_is_unknown_never_fresh`：先在临时 root 里**只写一个页文件**（`wiki/ghost-notes.md`，带一条锚 `ghost-src#1`），此时 KB 里**还没有任何文档**（索引每 60s 才跑一次，刚写下的页还没被扫到 ⇒ 这正是影子滞后的窗口）。判据与读数：

| 断言 | 读数 |
| --- | --- |
| 前置：`GET /knowledge/documents` 的文档数 = 0（**先测量再引用**，否则「unknown」可能只是碰巧） | `0` |
| `freshness` | `"unknown"`（不是 `"fresh"`） |
| 冻结不变量 `stale == (freshness=="stale")` | 成立（`stale=false`）—— D.2 把 `stale` 冻成 `bool` 以不破坏 CLI/panel，第三态由 `freshness` 承载 |
| 该页确实被列出来（不是「查不到所以当新鲜」） | `pages.len() == 1` |

这条读数是**我自己先写错、被测试抓出来的**：第一版 `WikiPageInfo.stale` 我打算给三态（`null`），测试直接红了 ⇒ 回读 t4 D.2 才确认**冻结的形状是 `stale: bool` + 不变量 `stale == (freshness=="stale")`**，于是按冻结接口改测试而不是改接口。**代码没有任何改动**，改的是我对接口的理解。

### 3.4 旧代码反证（每一条新判据都必须有「旧代码上会红」的证据）

**对象集**：`$env:TEMP\ruagent-rd\debug\ruagent.exe` —— sha256 `CB8B55A06BDE3BE29B6F136BA96B52C6D880F599245E54C9A8C58D7219043C0E`，322,650,112 B，mtime **2026-09-27T22:18:32+08:00**。它是 HEAD `0a39e5b8` 上、**我改 `wiki.rs` 之前**构建的（R-D 的 canary 用的同一个二进制），所以它就是 gen2 之前的代码。

**命令**（脚本落在 `$env:TEMP\ruagent-id\oldcode-probe6.ps1`，不在仓库内；一个 daemon、每次构建前重写 replies）：

```powershell
pwsh -NoProfile -ExecutionPolicy Bypass -File "$env:TEMP\ruagent-id\oldcode-probe6.ps1"
```

**读数（2026-09-27 23:58:41 – 23:59:13 +08:00，pid 29444，起于 127.0.0.1:8802，结束时按记录 PID 停掉，复查 `alive after stop: False`）**：

```
[23:59:04] CASE[fabricated fact, one section uncited]                 row.status=written file=True written=1 failed=0 error=
[23:59:07] CASE[no 来源 section at all]                                row.status=written file=True written=1 failed=0 error=
[23:59:09] CASE[anchor names a chunk that does not exist]              row.status=written file=True written=1 failed=0 error=
[23:59:11] CASE[anchor names a document that is not this page's source] row.status=written file=True written=1 failed=0 error=
[23:59:12] CASE[来源 lists a document the page does not cite]           row.status=written file=True written=1 failed=0 error=
[23:59:13] CASE[nothing that can be cited (no content section)]        row.status=written file=True written=1 failed=0 error=
[23:59:13] RESULT: 6 / 6 of the bodies the gen2 gate rejects LANDED as written on the pre-gen2 binary
```

**6/6 全部以 `written` 落地、`pages_failed=0`、`error=null`**，其中第一条同时复现了 B-21 的现场故障（凭空事实 + 无来源节）。旧代码的 stage-3 只有两条校验（`body.starts_with("# ")` 与长度上限），所以「坏侧红不红」在旧代码上根本无从谈起——**坏侧不是变红，而是压根没有被判**，这正是本代要补的洞。

**其它新判据的旧代码侧证据**：
- **G7b（组合请求）**：R-D 的 canary B-21 D1c 记录 `{"dry_run":true,"confirm_plan":1}` 在旧代码上 **200 + 新建一行 build**（不是 400）——同一探针、同一命令。
- **G7a/7c（词汇与 pending）**：旧代码 `DRY_RUN_STATUS="planned_only"` 而响应 `status="planned"`，且只读计划的页行写 `pending`；现场读数就是 B-04（`planned_only`×2 + legacy `planned`×2）与 B-05（22/41 pending）。新代码把两行 legacy 交给 0022 回填，并在写入侧改用新词汇。
- **G3/G4/G6/G9**：旧代码在真库上**信号从未正例**（`stale=0/4`、无冻结页、无 `stale_since` 字段、`wiki_corrections` 表不存在），所以这四条的反证只能靠**构造**：R-D 的 canary（B-11，22:29:51）已经证过「改写来源后 `index.md` 不出现 `⚠️ 源已更新`」（G3 的坏侧）；G4/G6/G9 的坏侧就是同一代旧代码的结构（`keep` 直接 skip、无纠错表、`delete` 无改名识别），我在 §5-D4 里登记了它们**没有**独立旧二进制重跑，只有源码坐标 + 现场读数。

---

## 4. 单源强制点（E.4.4 要求的「变更前后对比」）

命令（只读，不编译）：

```powershell
Select-String -Path crates\daemon\src\wiki.rs -Pattern "let stale"
Select-String -Path crates\daemon\src\wiki.rs -Pattern "freshness_with\(|freshness\(kb|freshness_all\("
Select-String -Path crates\daemon\src\wiki.rs -Pattern "links_out:"
Select-String -Path crates\daemon\src\wiki.rs -Pattern "chunk_id"
```

| 单源对象 | before | after | 强制点 |
| --- | --- | --- | --- |
| `stale` 的计算 | **5 处**独立实现（`wiki_state:525`、`regenerate_index:1227`、`pages:1446`、`recall_stubs:1501`、`entity_related_pages:1602`，每处自己比 `source_hashes`） | **1 处**（`freshness_with`，`wiki.rs:773`）+ 它的调用者：`freshness`(766)、`freshness_all`(894)、`pages`(2703)、`lead_for`(2795)、`regenerate_index`(2362)、`run_page`(1977) | 全文 `let stale` 只剩一处是**赋值**（`freshness_with` 内，`:850` 那处是 `stale` 变量本身）；另两处同步面（`lead_from:2766`、`entity_related_pages:3035`）调 `source_drift`，不再各写一份比较 |
| `links_out` | **2 处**（`pages()` 数原始目标；`link_graph()` 解析边）⇒ 现场 2/4 页两面不等（B-14） | **1 处**（`link_graph` 内 `out_all`→`degrees`，`:2611`）；`WikiPageInfo.links_out` 直接取 `d.links_out`（`:2724`） | `the_two_graph_endpoints_cannot_disagree` 对每页断言：`pages` 里该页的 `links_out` == `degrees` **数组中该 slug 的那一行**（`PageDegree`；`degrees` 是 `Vec<PageDegree>`，**不是**以 slug 为键的 map）的 `links_out`，且 `links_in`/`links_out_broken` 同样逐位相等 |
| `chunk_id` | **0 处**在 `PageMeta` 里（只有 `:1512` 召回 stub 的透传）⇒ chunk 级可引用率不可测 | `Citation` 里 3 处（结构 + 序列化 + 解析），页 frontmatter 里出现 `chunk_id:`/`chunk_hash:` | `dry_run_plan_then_confirmed_build_lands_pages` 断言页文件含 `chunk_id: 1` 与 `chunk_hash:` |
| `wiki/index.md` 的 `⚠️ 源已更新` | 由 `regenerate_index` **自己算** stale（与 API 可能不一致，B-11） | `regenerate_index` 调 `freshness(kb, …)`，与 `/pages` 同一函数 | `a_page_that_stays_stale_records_since_and_says_why` 断言 API 说 stale 的同一页在 `index.md` 里也带标记 |

---

## 5. 偏差与未落地登记（逐条给判据 / 现状 / 代价 / 归属）

> 纪律：**不静默降级**。凡是没做到的，这里写明；凡是做了但形状与冻结接口不同的，这里写明。

**D1 · G8（`<wiki>` 块标记 2/2）= not_measured，不在本单能完成的范围。**
- 现状：生产侧数据已齐（`WikiLead` + `lead_for`/`lead_from`，`recall_stubs` 已带 `anchors`/`hint`/`cite_coverage`/`stale_since`）。
- 缺谁：`crates/memory/src/inject.rs`（I-B 的 `RetrievalHit.lead` + 块渲染）与 `crates/daemon/src/{chat,runs}.rs` 的两处构造点（I-INT）。
- 判据照旧、**目标不降**：`cargo test -p ruagent-memory` + `cargo test -p ruagent-daemon` 上测 `2/2`；本单无权改这两个 crate。

**D2 · `lead_from`/`entity_related_pages` 的 stale 只看来源级漂移。**
- 同步路径拿不到 DB（手改 hash）与 chunk 表，所以它只能报 `SourceHashDrift`/`SourceMissing` 两种原因。`WikiLead.edited` 在同步路径上是 `None`（三态而非 `false`），`stale_since` 为 `None`。
- 代价：HTTP 召回 stub 在「只被手改、来源没变」的页上会说 `stale=false`。**这是刻意的**：宁可少报一种原因，也不在那个位置重写一份 staleness（E.4.4 的单源要求）。要拿到完整三态，走 `lead_for(kb, recorded_hash, slug)`（async）。

**D3 · 冻结接口签名偏差 3 条（需要 I-INT/I-B 知道）。captain 已裁决：接受，作为破坏性变更保留在本文登记。**
1. `lead_for` 冻结形是 `pub fn lead_for(kb, slug) -> Option<WikiLead>`，实现是 `pub async fn lead_for(kb: &Knowledge, recorded_hash: Option<&str>, slug: &str) -> Option<WikiLead>`（要读 DB 与 chunk 表）。同步场景请用 `lead_from(kb, slug)`。**破坏性**：调用方必须 `.await` 并给出 `recorded_hash`（t19 已按此签名接线，无第二份调用面）。
2. `WikiLead.edited` 是 `Option<bool>`（同步面 `None`，async 面 `Some`）；`WikiLead.anchors` 的类型是 **`Vec<Citation>`**（不是节标题字符串），每个锚 `{section, document, chunk_id, chunk_hash}`，最多 3 条，按 `(section, chunk_id)` 排序——注入侧要用 `document`+`chunk_id` 去 `GET /api/v1/knowledge/expand/{chunk_id}` 取证据。
3. `WikiPageInfo` 没有把 `stale` 改成 `Option<bool>`（那会破坏所有既有消费者），而是**追加** `freshness: String`（`fresh`/`stale`/`unknown`）承载第三态；`stale` 仍保留 bool。
4. 追加（不是偏差，但下游要看）：`WikiPageInfo` 增 `citations`/`cite_coverage`/`uncited_sections`/`stale_since`/`stale_sources`/`built_at`/`freshness`/`links_out_broken`/`self_links`/`frozen_by`；`WikiLinks` 增 `wanted`/`unreachable`/`self_links`/`degrees`/`readings_at`（`broken` 语义保留 = `wanted` 的 slug 列表）。

**D4 · 反证覆盖的不均匀（诚实标注）。**
- 6 个引证缺陷族有**同二进制重跑**的 6/6 反证（第 7 族 `no content section` 在旧代码上的形状相同：旧 stage-3 只校验 H1 与长度，空壳当然也以 `written` 落地——这一族的旧代码侧由同一份 6/6 探针的第 6 例覆盖）；G7b 有 canary 读数；G7a/7c 有现场表读数。
- G3/G4/G6/G9 的旧代码侧只有源码坐标 + 现场读数（其信号在真库上从未正例），**没有**为它们单独跑一遍旧二进制。若要补，代价是给每条写一个旧二进制探针（`~1 min/条`，脚本已在 `$env:TEMP\ruagent-id\` 里可复用）。**我没有把它们写成「已验证」**。

**D5 · E11（残余面 `shadow_lag`）只做了一半。**
- 做了：影子与磁盘的对账（`reconcile_page_hashes`，删 1 行泄漏即可自愈）+ 图读数带 `read_at`（`/wiki/links.readings_at`、`wiki_graph_readings.read_at`）。
- 没做：**任何残余回答都带 `read_at` 与 `shadow_lag`**（「影子与磁盘 mtime/内容的差」）。现状是 wiki 面的残余读（`/knowledge/search` 等）不在 `wiki.rs` 里，且 `shadow_lag` 需要一个「磁盘 mtime vs 影子 updated_at」的比较面——`wiki.rs` 没有这个读路径。归属：`crates/knowledge`（I-A，U-7 的开放项）+ I-INT 的端点层。

**D6 · 迁移文件名偏差。** t4 的 E.2 请求的是 `0019_wiki_gen2.sql`；I-SCHEMA 实际落在 `0022_wiki_gen2.sql`（0019–0023 一起，22:51–22:57 落盘）。DDL-4 按路线 B 用**触发器 + 视图**实现（不是 CHECK，因为 FK 挡表重建，且 `finished_at` 那条 CHECK 对旧代码「一条 INSERT + 一条 UPDATE」的 dry-run 不可能成立）；DDL-5 按「写一个**独立**的 `wiki_build_pages.status`」落地（本单写 `planned`）。这些都按 I-SCHEMA 的裁决执行，不是偏离。

**D7 · 「页必须至少有一个内容节」→ captain 已裁决：接受，收进冻结枚举（已实现）。** 原始登记：这条加严超出 t4 的 6 值 `CiteProblemKind`，所以第一版把它做在 `run_page` 里一条独立 `bail!`（错误词同样是 `no content section`）。**裁决与落地**（2026-09-28，captain）：接受作为**独立的第 7 个变体** `CiteProblemKind::NoContentSection`，并配自己的触发用例。理由与判据方向一致：其余规则都是「**每个** content section 都带锚」的全称命题，而**全称命题在空集上恒真** ⇒ 判据必须在空集上**失败**，不能真空通过（这正是本仓「判据不许真空成立」的方向）。

- 实现：`verify_page` 的 **rule 0**（在来源节与锚规则之前）——`content_sections.is_empty()` ⇒ push `CiteProblem{kind: NoContentSection, section: "", detail: "the body has no content section (## H2) — nothing that can be cited"}`；`run_page` 里原来那条**重复的** bail 已删除（同一个事实只有一个判定点）。
- 新增 `CiteProblemKind::ALL`（7 项）与单测 `cite_problem_kinds_have_distinct_words`：7 个词两两不同、都非空——两个族共用一个词会让 `error` 行无法审计。
- 新增集成用例 `a_page_with_nothing_to_cite_fails_by_its_own_name`：断言 `failed` + 词 `no content section` + **不出现**其余四个族的词（**归因**，不是「那一轮里也红了」）+ 页文件不存在。
- 读数：`test -p ruagent-mock-agent --test wiki_pipeline` **14 passed / 0 failed**；`test -p ruagent-daemon --lib` **72 passed / 0 failed**。
- 注：RV-D 只需确认它**被登记为具名变体 + 有用例**（captain 原话），不需要重新决定要不要收。

**D8 · `uncited_sections_of` 的代价。** `/wiki/pages` 对**有锚**的页仍要重读一次页文件正文（为了给出未引用节的**名字**）。对象集 = 页数（现场 4），采样面 = 全体，代价量级 = 每页一次 `read_to_string`；`cite_coverage`/`citations`/`stale` 都走 frontmatter，不需要正文。若页数上千再谈缓存；**现在是刻意不做缓存**，为了不引入第二份状态。

**D9 · 自查抓到并修掉的一个真缺陷：`stale_since` 曾经只写不读。** 第一版把 `stale_since` 写进 `wiki_pages` 就完事了，`freshness_with` 返回的 `stale_since` 恒为 `None` ⇒ **没有任何消费面能读到它**（G3 的 `stale_persistence` 于是不可观测，`a_page_that_stays_stale_records_since_and_says_why` 会红）。修法：新增 `recorded_page_state(db)`——一次两条查询读回「构建写下的内容 hash」与「本段 stale 的首次观测时刻」，`pages()` 与 `freshness_all()` 都用它（顺带把原来每页一次 `page_hash` 查询合并成每页零次）；`pages()` 只在**确实 stale** 时上报该值，`record_invalidation` 的 `ELSE` 分支由「保留旧值」改为 `NULL`（页面既然已经新鲜，观测时刻不该继续挂着）。读数：该用例断言 `stale_since` 非空且含 `T`（RFC3339），通过。

**D10 · 自查抓到的第二个真缺陷：`## 来源` 的围栏内容会被当成来源名。** 一个页在来源列表**后面**再放一个代码块（很常见：示例命令），旧解析会把围栏里的每一行都收进名单 ⇒ `sources section mismatch` 冤枉判失败。修法：`sources_section` 跳过 ``` / ~~~ 围栏与结构行（`#`/`<!--`/`|`/`>` 开头），并容忍 `- name — 说明`、`` * `name` ``、裸 `name`、`[[name]]` 四种写法（取 `—`/`:`/`(`/tab 之前的第一个词，去掉包裹的 `` ` ``/`[]`/`()`/引号，去掉尾部 `.md`）。读数：单测 `citations_are_parsed_per_section_and_content_sections_exclude_navigation` 通过，且集成测试的 6 个负例族仍逐条红（宽容没有把闸门放松——**它只对格式宽容，不对缺锚/缺节宽容**）。

**D11 · 行 `action` 的语义收窄（下游要知道）。** `wiki_build_pages.action` 现在记的是**执行器实际做的动作**，不是计划请求的动作：计划说 `keep`、页面已 stale ⇒ 行记 `update`（计划原话仍在 `wiki_builds.plan_json` 里）。这是为了能回答 G4 的问题；消费方若要「计划想做什么」，读 `plan_json`。

**D12 · 我自己的两条 warning 与一次「假绿」的取证过程（仪器教训，留档）。** 我名下原有两条 clippy warning，都已修掉并复测：`wiki.rs:403` 的 `collapsible_if`（frontmatter 的 citations 块解析——**修法保留原语义**：缩进行无论能否解析都必须被消费掉，否则一个看不懂的缩进行会提前结束块列表）与 `wiki.rs:3518` 的 `type_complexity`（抽成 `type RecordedPageState = …`）。真正要留档的是取证过程：`clippy … -- -D warnings` 的尾参**不进 cargo 指纹**，所以「先前跑过一次不带 `-D` 的 clippy」会让这条命令**命中缓存、一条都不重查、直接报成功**（我实测到 `exit=0` + 0.8s + 输出里没有 `Checking ruagent-daemon`）。**可信读数的判据必须带第二条**：输出里出现本次的 `Checking ruagent-daemon`；否则先 `cargo-team.ps1 clean -p ruagent-daemon` 再跑（§8.3 R3）。

---

## 5bis. t34 修复（RV-D 六条 finding）：改了什么、现在的读数

> 本节是**修复**记录。RV-D 的判决（`docs/design/reviews/gen2-wiki-review.md`）与 V-D 的独立取证（`gen2-wiki-verify.md`）钉在 `wiki.rs 45AA136091C5D17D @01:10:13` 那一版上；本节的读数全部取自修复后的工作树（字节号见 §8.3 R6）。
> **两类修复分开记**（captain 的报告纪律）：§5bis.1/5bis.2 是**代码**修复（RV-D-1/RV-D-3 的实现半边），§5bis.3 是**措辞与计数**（RV-D-2/RV-D-4/RV-D-5 的文档半边、RV-D-6），**没有一处改动写侧准入逻辑**。

### 5bis.1 代码：`cite_coverage` 改成「有记录的读数 + 第三态」（RV-D-1）

**缺陷（RV-D-1，high，G1 硬门的读数半边）**：`cite_coverage_of(meta)` 的分子取自 `meta.citations` 里 distinct 的 content section，分母 `content_section_count(meta)` **从同一个列表算同一个集合** ⇒ 返回值只能是 `0.0`/`1.0`；四个读面（`/wiki/pages`、`lead_from`、`lead_for`、hints JSON）全用它，而构建期真值（`report.coverage` → `wiki_pages.cite_coverage`）**零读回**。后果：同一响应里 `cite_coverage=1.0` 与 `uncited_sections=["事实"]` 并存；`wiki_pages` 里**没有行**的手写页也报 `1.0`。

| 面 | 改前 | 改后 |
| --- | --- | --- |
| `cite_coverage_of(meta) -> f32` | 由 frontmatter 重算，「只能 0.0/1.0」（注释还自称是构建期算的，**说反话**） | **删除**。拆成两个诚实函数：`has_anchors(meta) -> bool`（页里记了至少一条锚）与 `anchored_sections(meta) -> usize`（页记录了锚的 distinct content section 数 = 真值的分子） |
| `/wiki/pages` 的 `cite_coverage` | `f32`，恒等式 | **`Option<f32>` = `wiki_pages.cite_coverage` 记录值**，且只在页面**仍 fresh** 时敢报（`stale`/`unknown` ⇒ `null`） |
| `lead_from`（同步 / HTTP stub） | 同上恒等式 | `cite_coverage: None`（它看不到 DB ⇒ **unknown**），并新增 `has_anchors`/`anchored_sections` 两个页级事实 |
| `lead_for`（async） | 同上恒等式 | 第二参由 `recorded_hash: Option<&str>` 改为 **`&RecordedPage`**（`wiki::page_record(&db, slug).await` 一行取），覆盖率同样只在 fresh 时给出 |
| hints JSON / 实体软链卡 | 同上恒等式 | `"cite_coverage": null` + `"has_anchors": bool`（两个都是同步面，不许猜） |
| `wiki_pages` 无行 | 报 `1.0` | **`null`**（第三态）。同一个读法配一条判据：`cite_coverage` 列是 `NOT NULL DEFAULT 0.0`，所以只有 **`build_id IS NOT NULL`** 的行才算「有记录的读数」—— 否则那个默认值 0.0 会被读成一个**测出来的 0**（与「不可判定不许被布尔掩盖」同族） |

**单源点**：`recorded_coverage(record, freshness) -> Option<f32>` —— 四个读面都从它取数，规则一句话：**`Some(v)` 只在「有构建记录 且 该页现在仍 fresh」时出现**。它同时让 captain 要的**不变量**成立：
> 同一响应里**不许**同时出现 `cite_coverage == 1.0` 与非空 `uncited_sections`。

因为 `1.0` 要求 `fresh`，`fresh` 要求每条记录的锚都还解析得了，而页落地时每条 content section 都带锚（写侧闸门拒绝其余）⇒ 三者不可能与「有未引用节」并存。

**两条新判据（都是 captain 指定要的）**
1. **black-box**（`an_unresolvable_anchor_cannot_keep_reporting_full_coverage`）：一张页先以 1.0 落地（`freshness=fresh`、`cite_coverage=1.0`、`stale_reasons=[]`），随后**来源被重摄成只剩一节**、锚指向的 chunk 消失 ⇒ 同一个面的读数变成 `stale=true`、`cite_coverage=null`、`uncited_sections` 非空、`stale_reasons` 含 `chunk missing`/`chunk hash drift`；并逐页断言不变量与「覆盖率只在 fresh 页出现」。
2. **第三态**（`an_unanswerable_freshness_reading_is_unknown_never_fresh` 加强）：无 `wiki_pages` 行 + 锚解析不了 ⇒ `cite_coverage` 为 `null`（**不是 1.0**）且 `uncited_sections` 非空；`stale_reasons` 为**空**而 `unknown_cause` 非空 —— 两个第三态用**不同的词**回答「为什么不能信」（「KB 答不了」不是漂移原因，不许拿假原因凑满）。

**写侧闸门没有动**（captain 明令）：`the_citation_gate_fails_a_page_whose_claims_cannot_be_traced`（7 族）与 `a_page_with_nothing_to_cite_fails_by_its_own_name` 原样通过；本修复只换**读数**的取数口。

### 5bis.2 代码：`stale_reasons` / `unknown_cause`（RV-D-3）

`stale_sources` **只在页面记录了 `source_hashes` 时才可能有名字**（V-D 的 `VD T8-B` 夹具 `source_hashes: {}` ⇒ `stale=true` 而 `stale_sources=[]`）。两种形态都写进标定读数：**形态 A**（有 `source_hashes`）⇒ `stale_sources=["src-alpha"]`（`a_page_that_stays_stale_records_since_and_says_why` 断言）；**形态 B**（无记录）⇒ `stale_sources=[]`，但 `/wiki/pages` 的 `stale_reasons` 非空、行 `error` 与 `wiki_corrections` 有原因，所以「为什么」仍然可答。新增字段：`stale_reasons: Vec<String>`（原因词，stale 时非空）+ `unknown_cause: Option<String>`（仅 unknown 时非空）；单测 `stale_reasons_have_distinct_words` 保证 5 个原因词两两不同。

### 5bis.3 文档：措辞对齐与登记（RV-D-2 / RV-D-4 / RV-D-5 / RV-D-6）

- **RV-D-2（措辞，非代码）**：规格 G3 与本节 §2.2 的那句话已改为「**KB 整体无可用 documents row ⇒ `unknown`；页点名的来源消失 ⇒ `stale{SourceMissing}`**」，并把 V-D 的 `VD T8-A`/`VD T8-B` 登记为证据。实现未动（`SourceMissing` 保留）。
- **RV-D-4（空表登记）**：`wiki_citations`（0022 DDL-2）**本代未使用**，按下游 `chunk_id` 反查受影响页的索引 **deferred**；现状的失效传播真实路径 = **重新扫描页文件**（`freshness_all`/`uncited_sections_of` 读盘 + `wiki_pages.citations_json` 里的锚），**不是**反查表。登记在规格 E.2 + 本节；**没有**为了「别浪费表」在 `record_page_row` 里顺手写它（那会造出一条没有消费者的写路径）。DDL 侧由 I-SCHEMA 拥有，已同时告知。
- **RV-D-5（口径收窄）**：`frozen_by` **仅 pin 路径非空**（手改冻结那条路 `frozen_by=null`，但同一响应里 `edited=true` + `freshness="stale"` + 行 `error` 与 `wiki_corrections` 都带原因，面板据此足以解释）。选择「改标定读数」而不是「给 note 也发 frozen_by」：把一条 `note`（人工备注，可能是「我核对过」）当成「冻结」是**语义错误**，会让面板把备注显示成冻结。
- **RV-D-6（计数）**：见 §8.3 R0 —— t10 摘要里那句 `13 passed` 是旧数（应为 14），本修复又**新增 1 条**判据（black-box 不变量）⇒ 现在是 **15**。三个数都写明：**改前 4 → t10 交付 14（新增 10）→ t34 修复 15（新增 1）**。

---

## 6. 对 I-INT（t19）的接线请求（api.rs / chat.rs / runs.rs）

我**没有**改这些文件（不在 inScope）。需要 I-INT 吃下：

1. **两个 `RetrievalHit` 构造点**（`chat.rs:560`、`runs.rs:1884`）填 `lead`：用 `ruagent_daemon::wiki::lead_for(kb, &record, &slug).await`（async，**签名在 t34 变了**：先 `let record = wiki::page_record(&db, &slug).await;`，见 §5-D3/§5bis.1）；只做同步渲染的地方用 `lead_from(kb, &slug)`，那里 `edited`/`stale_since`/`cite_coverage` 都是 `None`（**unknown**，不是 `false`/`0.0`）。
2. `/wiki/pages`、`/wiki/links`：新字段**自动透传**（`wiki.rs` 已序列化），端点不需要改。
3. `crates/daemon/tests/knowledge_api.rs` 的逐字段断言按 D.2/D.3 更新（原文件不在我 inScope；现有 `links_out==2`、`broken==["k8s"]`、`orphans==["orphan-page"]` 这三条**在我的语义下依然成立**，可以原样保留）。
4. **纠错写入端点**（可选，`POST /api/v1/knowledge/wiki/pages/{slug}/corrections`）：`add_correction(&db, &Correction{slug, kind, reason, author, at})`，`kind ∈ {pin, release, note}`，空 reason/author 会 `Err`（HTTP 应映 400）。`corrections`/`active_correction` 是读侧。
5. **panel**：`WikiPageInfo`/`WikiLinks` 是**追加**字段，panel 不改也能跑；要显示三态请读 `freshness`，不要用 `stale` 推断 `unknown`。**覆盖率别再当"分数"显示**：`cite_coverage` 现在是 `number | null`，`null` 的含义是 **unknown**（没构建过 / 记录值已不再适用），要显示成 `coverage=unknown`；页级的 `has_anchors`/`anchored_sections` 是页里记了锚的**事实**，不是及格线。要看"为什么"：`stale_reasons`（stale 时）/ `unknown_cause`（unknown 时）/ `stale_sources`（仅有 `source_hashes` 时）。
6. 若 I-INT 要把 `readings_at`/图读数暴露到 `/wiki/links`：已经在了（`readings_at`），`wiki_graph_readings` 的历史行需要一个新端点或复用 builds 详情。

---

## 7. 对注入契约（`<wiki>` 块）的影响

- **wiki 仍是 lead，不是 evidence**：`lead_from`/`lead_for` 与 `recall_stubs` 只输出**页的身份 + 可靠性标记**（`slug`/`title`/`summary`/`stale`/`stale_since`/`edited`/`has_anchors`/`anchored_sections`/`cite_coverage`/`anchors`/`hint`），**不输出正文**。既有纪律「无命中不产生占位块」「wiki 是线索不是证据」不变。
- `LEAD_HINT` 的文案固定为一条：`generated wiki page — verify against its sources before trusting`（`anchors` 给最多 3 个锚 `{section, document, chunk_id, chunk_hash}`，指路用）。
- `WIKI_PAGES = 2` 不变（`inject.rs` 的常量，我没碰）。
- 新字段全是**可选/追加**：`stale` 从 bool 变「bool 或 null」（`unknown` 时显式给 `null` 而不是省略字段，G8 要求「`stale=unknown` 时不得省略该字段」）。
- `recall_stubs` 的既有键（`kind`/`slug`/`title`/`stale`/`chunk_id`）全部保留，未删任何键。
- **⚠️ 破坏性变更（RV-D-1，必读）**：`WikiLead.cite_coverage` 由 `f32` 改为 **`Option<f32>`**，`has_anchors: bool` 与 `anchored_sections: usize` 是**新键**；`lead_for` 的签名再次变化（第二参由 `recorded_hash: Option<&str>` 改为 `&RecordedPage`，用 `wiki::page_record(&db, slug).await` 取）。注入侧**不许**把 `None` 渲染成 `0.0` 或 `1.0`，也不许省略该字段——它必须与 `stale=unknown` 同词表地表现为 **unknown**。这正是 captain 的 (a)/(c)：**「不知道」不许被压成满分**。

---

## 8. 复现命令与纪律声明

### 8.1 命令（全部在仓库根，临时 root，不碰真库）

```powershell
# 资源纪律（2026-09-28 起全队生效）：不自己设 CARGO_TARGET_DIR，一次只跑一条 cargo
$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"
pwsh -NoProfile -ExecutionPolicy Bypass -Command "& './scripts/cargo-team.ps1' -CargoArgs 'test','-p','ruagent-mock-agent','--test','wiki_pipeline'"
pwsh -NoProfile -ExecutionPolicy Bypass -Command "& './scripts/cargo-team.ps1' -CargoArgs 'clippy','-p','ruagent-daemon','--all-targets','--','-D','warnings'"
pwsh -NoProfile -ExecutionPolicy Bypass -Command "& './scripts/cargo-team.ps1' -CargoArgs 'check','-p','ruagent-daemon','--all-targets'"
# 旧代码反证（gen2 之前的二进制，见 §3.4）
pwsh -NoProfile -ExecutionPolicy Bypass -File "$env:TEMP\ruagent-id\oldcode-probe6.ps1"
# 单源强制点（§4）
Select-String -Path crates\daemon\src\wiki.rs -Pattern "let stale"
git status --porcelain -- crates/store crates/knowledge crates/memory crates/graph panel
```

> **注意**：captain 文档里的 `-File scripts/cargo-team.ps1 test -p …` 形状会因参数绑定失败（`Cannot convert value "test" to type "System.Int32"`，2026-09-28 00:12 +08:00 实测）；上面用的是 `-CargoArgs` 形状，已通报 captain。

### 8.2 本轮实际跑过的验证（读数见下）

| 命令 | 结果 |
| --- | --- |
| `cargo check -p ruagent-daemon` | ✅ 绿（我的改动全部编过）；期间两次因同伴在途编辑变红（`crates/knowledge/src/store.rs` 的 `SearchEvidence: Serialize`、`crates/daemon/src/memembed.rs:513` 的 `Namespace::parse` 返回类型），我**没有**改别人的文件，各发了一条带**原文错误 + 命令**的消息给属主，之后都自然转绿 |
| `cargo check -p ruagent-daemon --all-targets` | ✅ 绿 |
| `cargo check -p ruagent-mock-agent --tests` | ✅ 绿 |
| `cargo test -p ruagent-mock-agent --test wiki_pipeline --no-run` | ✅ 绿：`Executable tests\wiki_pipeline.rs`，`Finished test profile in 1m00s`（00:0x +08:00） |
| `rustfmt --edition 2024`（我改的两个文件） | ✅ 无 diff |
| `cargo test -p ruagent-mock-agent --test wiki_pipeline` | 见 §8.3 |
| `clippy -p ruagent-daemon --all-targets -- -D warnings` | 见 §8.3 |

### 8.3 测试与 clippy 的最终读数（2026-09-28 00:2x–00:5x +08:00）

**R0 · 集成测试（t4 的契约 verify 命令）**

```powershell
$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-mock-agent --test wiki_pipeline
```
```
running 15 tests
test a_page_hash_with_no_page_is_swept_by_the_next_build ... ok
test a_page_that_stays_stale_records_since_and_says_why ... ok
test a_page_with_nothing_to_cite_fails_by_its_own_name ... ok
test a_pin_skips_the_page_and_the_reason_is_visible ... ok
test a_rename_moves_the_topic_instead_of_destroying_it ... ok
test a_stale_page_is_repaired_even_when_the_plan_says_keep ... ok
test an_unanswerable_freshness_reading_is_unknown_never_fresh ... ok
test an_unresolvable_anchor_cannot_keep_reporting_full_coverage ... ok     ← t34 新增（RV-D-1 black-box）
test delete_action_removes_page_and_empty_scope_is_rejected ... ok
test delete_guard_protects_pages_with_live_sources ... ok
test dry_run_has_one_vocabulary_and_rejects_the_combined_request ... ok
test dry_run_plan_then_confirmed_build_lands_pages ... ok
test rebuild_updates_backup_edited_skip_and_delete ... ok
test the_citation_gate_fails_a_page_whose_claims_cannot_be_traced ... ok
test the_two_graph_endpoints_cannot_disagree ... ok
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 18.27s
[cargo-team] exit=0 elapsed=39.1s
```
**计数（RV-D-6 + captain 要求）**：改前（B-17，同一份文件）**4 passed / 0 failed**（测试执行 6.80s）⇒ t10 交付 **14**（新增 10：6 个负例族 + 第 7 族的归因用例 + 干跑单源 + 图单源 + 第三态 + 泄漏对账 + 改名 + pin/hand-edit 冻结，其中 4 个旧用例被加强）⇒ **t34 修复 15**（再新增 1：RV-D-1 的 black-box 不变量；另有 2 处旧断言被加强：第三态用例加 `cite_coverage=null`/`unknown_cause`，stale 用例加 `stale_reasons`/`cite_coverage=null`）。**t10 摘要里那句 `13 passed` 是旧数**，正确值是 14（V-D 与 RV-D 独立跑到的也是 14），本报告以本节为准。

**R1 · daemon 单测（含 wiki.rs 的纯函数单测）**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-daemon --lib
```
```
test wiki::tests::citations_are_parsed_per_section_and_content_sections_exclude_navigation ... ok
test wiki::tests::cite_problem_kinds_have_distinct_words ... ok
test wiki::tests::link_graph_math ... ok
test wiki::tests::dry_run_row_is_terminal_and_distinct ... ok
test wiki::tests::frontmatter_roundtrip ... ok
test wiki::tests::frontmatter_tolerates_hand_edits_and_garbage ... ok
test wiki::tests::plan_json_parses_with_fences ... ok
test wiki::tests::a_status_write_that_does_not_land_is_logged_and_leaves_the_build_running ... ok
test result: ok. 72 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.80s
```
**t34 修复后同一命令**（`76 passed / 0 failed`，wiki 名下新增 2 条纯函数单测）：
```
test wiki::tests::stale_reasons_have_distinct_words ... ok
test wiki::tests::recorded_coverage_is_quoted_only_on_a_fresh_page ... ok
test wiki::tests::cite_problem_kinds_have_distinct_words ... ok
test result: ok. 76 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.21s
[cargo-team] exit=0 elapsed=4.0s
```

**R5 · `stale_sources` 的两种形态（RV-D-3 的标定读数，两种都写了）**

| 形态 | 夹具 | 读数 |
| --- | --- | --- |
| **A：页里记了 `source_hashes`** | `a_page_that_stays_stale_records_since_and_says_why`（改写 `src-alpha`） | `stale=true`、`stale_sources=["src-alpha"]`、`stale_reasons` 含 `source hash drift` 与 `hand edited`、`cite_coverage=null` |
| **B：页里没有 `source_hashes` 记录** | V-D 的 `VD T8-B`（`source_hashes: {}`）+ 本仓 `an_unanswerable_freshness_reading_is_unknown_never_fresh` | `stale_sources=[]`（**没有名字可填**，不是"没有原因"），而 `stale_reasons` / `unknown_cause` 非空、行 `error` 与 `wiki_corrections` 带原因 ⇒ 「为什么」仍可答 |

**R6 · 改后字节号（修复后的对象集，供 V-D/RV-D 复审引坐标）**

```powershell
Get-FileHash crates/daemon/src/wiki.rs, crates/mock-agent/tests/wiki_pipeline.rs -Algorithm MD5
# t34 修复后（2026-09-28 02:25 +08:00）：
#   crates/daemon/src/wiki.rs                MD5 874EC903C94EB263…   02:25:49
#   crates/mock-agent/tests/wiki_pipeline.rs MD5 B141DFE3A40D8827…   02:25:52
# 修复前（评审/V-D 的字节）：
#   crates/daemon/src/wiki.rs                45AA136091C5D17D    @ 2026-09-28T01:10:13
#   crates/mock-agent/tests/wiki_pipeline.rs F881F6B272945291    @ 2026-09-28T01:10:10
```
两个文件在 t34 修复后**都已变化**，所以评审/V-D 的旧字节号只适用于修复前那一版，**两版读数不要混引**。规格（`gen2-wiki-spec.md`）与本报告也在同一窗口改动（措辞/登记，见两文件的修订记录）。

**R2 · 编译**

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 check -p ruagent-daemon
# [t10] Finished `dev` profile [unoptimized + debuginfo] target(s) in 2m 42s   [cargo-team] exit=0
# [t34 修复后，含 --all-targets，04:0x]
#       check -p ruagent-daemon --all-targets → Finished in 2.36s  exit=0
#       （与下面的 clippy run D 同源：run D 把整个包连同全部 target 重新查了一遍）
```

**R3 · clippy（`-D warnings`）—— 已取得绿读数，但必须说明它是怎么取得的**

```powershell
# 任务单 verify 的第 2 条。走 captain 的共享 target 包装；-CleanFirst 是 captain 之后
# 加进包装脚本的「同一个持锁窗口内先强制重查、再跑门」开关（见下）。
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 `
    clippy -p ruagent-daemon --all-targets -DenyWarnings -CleanFirst ruagent-daemon
# 历史（同一命令，不含 -CleanFirst）：
# [00:4x] error: manual_contains   --> crates\knowledge\src\store.rs:1230:16    (recall，已修)
# [01:4x] error: type_complexity   --> crates\daemon\src\distill.rs:1098/1100 (graph，已修)
#         error: could not compile `ruagent-daemon` (lib test) due to 2 previous errors
# [02:2x，D7 修订后的读数]
#   [cargo-team] clean -p ruagent-daemon (forced re-check)   ← 与下面那条门同一个持锁窗口
#        Removed 369 files, 1.7GiB total
#   Checking ruagent-daemon v0.1.0 (C:\...\crates\daemon)
#   Finished `dev` profile [unoptimized + debuginfo] target(s) in 7.07s
#   [cargo-team] exit=0      ← 0 条 warning、0 条 error
# [04:0x，t34 修复后的最终读数 —— 中间撞了两次同伴在途编辑，逐条记下]
#   run A: error: very complex type used  --> crates\daemon\src\memembed.rs:342:28   (mem-core，已修)
#   run B: error[E0425]: cannot find function `skip_missing_mock`
#                            --> crates\daemon\tests\injection_e2e.rs:685:9          (integ，已修)
#   run C（收窄读数，证明本单的代码面）:
#          clippy -p ruagent-daemon --lib -DenyWarnings -CleanFirst ruagent-daemon
#          → clean -p … (forced re-check) / Checking ruagent-daemon / Finished in 5.61s / exit=0
#   run D（最终、完整、强制重查）:
#   [cargo-team] clean -p ruagent-daemon (forced re-check)
#   Checking ruagent-daemon v0.1.0 (C:\...\crates\daemon)
#   Finished `dev` profile [unoptimized + debuginfo] target(s) in 6.78s
#   [cargo-team] exit=0      ← 0 条 warning、0 条 error，含 `--all-targets`
```
**归因读数**（`--lib` 面那条不成立时给出的、只证明"不是我"的方式）：`clippy -p ruagent-daemon --all-targets -CleanFirst`（**不加 `-D`**）在 04:0x 那次全包只有 **1 条** warning，且位置在 `crates/daemon/src/memembed.rs:342`（mem-core 的文件，根因是他们把 `usage_of` 的形状换成嵌套元组；他们随即抽成 `type UsageRow`）——**wiki.rs 与 wiki_pipeline.rs 名下 0 条**。两个同伴的阻塞都按纪律"只报文件与错误、不代改"，各自修好；run D 是全部收敛后的绿读数。

此前出现过的五条 warning/error（`wiki.rs` 两条我自己的、`knowledge/store.rs` 一条、`distill.rs` 两条、`tests/injection_e2e.rs` 一条）也都已消失：`distill.rs` 由 graph 修（两处行类型抽成 `type EdgeRow`；graph 同时记下自己只跑过 `-p ruagent-graph`、因此没看见 daemon 门这个盲区），`injection_e2e.rs` 的 `collapsible_if` 是更早一轮的形状、该文件现在已是折叠 let-chain。

**⚠️ 仪器教训 1：``-- -D warnings`` 不在 cargo 的指纹里 ⇒ 一条「绿」可能是假的。** 我实测到这条命令 **exit=0 但只花 0.77s / 0.82s / 1.44s、输出里没有 `Checking ruagent-daemon`** —— clippy 的 `-- -D warnings` 尾参**不参与 fingerprint**：只要先前有一次**不带 `-D`** 的 clippy 成功编译过各 target（共享 target 下这很容易发生——另一位同伴跑一次普通 clippy 就行），后一次带 `-D` 的运行就会命中缓存、**一条都不重查**、直接 `Finished` 报成功（此时仓库里明明还有 warning）。**判据因此是两条**：① 命令 `exit=0`；② 输出里**有**本次的 `Checking ruagent-daemon`。**没有第 ② 条时，第 ① 条什么也不证明。**

**⚠️ 仪器教训 2（竞态，captain 已把它从纪律下沉到工具）**：队内锁是**按 cargo 调用**持有的，所以「先 `clean -p`、再单独跑门」是**两次调用、中间会放锁**——队友的一次普通 clippy 可以插进来把缓存重新刷暖，于是我又拿到一次 **1.44s 的假绿**。我当时的兜底是 touch 自己的源文件 mtime（`git diff` 仍为空，已核对），但那是「用时间伪造重查」。现在正确做法是 captain 新增的 **`-CleanFirst <crate>`**：它在**同一个持锁窗口内**先 `cargo clean -p <crate>` 再跑门，中途放不进任何人的构建；上面那条最终读数就是这么取的（`clean -p ruagent-daemon (forced re-check)` 与门在同一个 `[cargo-team]` 调用里）。

**⚠️ 限制（graph 提的、我采纳为「收窄声称面」的范例）**：cargo 的 `Checking` 进度行是**按包**打印的，所以「`Checking ruagent-daemon` 出现」只能证明**这个包**被重查，**不能逐 target 枚举**（`--verbose` 的 `clippy-driver --crate-name …` 那批行也会被包装脚本的管道滤掉）。因此本报告**只声称**：这次调用 `exit=0`，且该包被重查过；**不声称逐 target 证据**。（一个 target 只有在 lint 成功过之后才会被缓存成干净，所以「重查 + exit=0」是这个窗口内可达的最强读数。）

**⚠️ 仪器教训 3（mem-core 先在 clippy 上踩到、我复现）**：cargo 在**第一个失败的 target 就停住**，`--all-targets` 的「all」**不是保证**。01:4x 那轮红在 `ruagent-daemon (lib test)`，它**后面**的 test target 根本没被编译 ⇒ 「这一轮读数里没有 `injection_e2e`」**不等于**「它已经修好」。所以要拿到完整读数，必须**先清掉前面的红、再重跑**。

**R4 · 改了哪些文件**

```powershell
git status --porcelain -- crates/daemon/src/wiki.rs crates/mock-agent/tests/wiki_pipeline.rs docs/design/reviews/gen2-wiki-impl.md
#  M crates/daemon/src/wiki.rs
#  M crates/mock-agent/tests/wiki_pipeline.rs
# ?? docs/design/reviews/gen2-wiki-impl.md
git diff --stat -- crates/daemon/src/wiki.rs crates/mock-agent/tests/wiki_pipeline.rs
#  crates/daemon/src/wiki.rs                | 2281 +++++++++++++++++++++++++++---
#  crates/mock-agent/tests/wiki_pipeline.rs | 1045 ++++++++++++--
#  2 files changed, 2991 insertions(+), 335 deletions(-)
```
```powershell
git status --porcelain -- crates/store crates/knowledge crates/memory crates/graph panel
# 非空：38 条（`crates/graph` 9 · `crates/knowledge` 10 · `crates/memory` 9 · `crates/store` 10 …）
```
**这条读数必须按归属读**：那 38 条是**同伴的在途编辑**（t6 的 0019–0023 迁移与 `store`、t7 的 `knowledge`、t8/I-B 的 `memory`、t9 的 `graph`），**没有一条是本单产生的**——本单的全部写入落在上一条命令列出的两个文件 + 本报告，三者都在 inScope 内。我不能把「我没碰」证成「那些目录是空的」（它们本来就不空），只能证成「我的写入集合 = inScope 的两个路径 + 报告」。`panel/` 在本单的 `git status` 里没有任何条目。

### 8.4 纪律声明

- 只写了 inScope 里的两个文件 + 本报告（写入集合 = `crates/daemon/src/wiki.rs`、`crates/mock-agent/tests/wiki_pipeline.rs`、`docs/design/reviews/gen2-wiki-impl.md`）；`crates/store`/`knowledge`/`memory`/`graph`/`panel` 里的 38 条改动是同伴的在途编辑，**没有一条来自本单**（§8.3 R4）。
- 真守护进程 **pid 79984（127.0.0.1:8787）全程未启停**；**未调用 `/api/v1/recall`**；**未对真库跑任何 wiki build**；真库只读（`file:...?mode=ro`）。
- 自己启动的进程：旧代码探针 daemon pid **91084**（8801）与 **29444**（8802），两个都按记录的 PID 停掉并在脚本内复查 `alive after stop: False`；无同类残留（按命令行复查）。
- **误杀披露**：00:1x 清自己的 cargo 包装进程时误杀了 PID 69716（别人的 `cargo test -p ruagent-daemon --lib distill` 包装进程，当时在等锁、未在编译），已按纪律报给 captain，影响面与纠正措施见该消息。
- 未测/跳过的项：全部列在 §5（D1 是唯一的生产目标未达成项，其余是签名偏差或半落地）。

---

## 9. 风险与不确定

1. **chunk id 不稳定**：重新 ingest 一个来源会换掉它的 chunk id，于是**旧锚全部失效**（`ChunkMissing`）。这是「页必须被重建才能继续可引用」的真实代价，也是 T7 里必须用**当前** chunk id 重建的原因。对生产的意义：来源改动后，页的重建是必须的，不能只改 hash。建议 I-A/后续在 `documents` 上给「chunk id 是否稳定」一个明确契约（现在没有）。
2. **`verify_page` 不证蕴含**（§2.1），所以 `verified: verified` 可能被读成「事实正确」。文案上我把它定位为「每条声称有活证据指路」；若 RV-D 认为字段名会误导，请裁决改名为 `anchored`。
3. **`/wiki/pages` 的正文重读**（D8）在页数大时会成为 N 次 IO。已知、未优化。
4. **改名识别的口径**：要求「Y 的来源集合与 X 的记录来源有交集」才算改名；来源被完全换掉的「改名」会被当成 delete + create（这是刻意的，避免把「换主题复用 slug」误判成移动）。
5. **`stale_since` 的观测窗口**是「下一次构建」，不是漂移时刻（§2.2）。消费面若在此之前展示「stale 已持续 N 天」会低报。
