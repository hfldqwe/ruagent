# knowledge — `#knowledge`

> **路由**：`#knowledge`（两个 tab：**文档**、**Wiki**）
> **视图**：`panel/src/views/Knowledge.tsx` → `Knowledge()`；Wiki tab 在 `panel/src/views/Wiki.tsx` → `WikiTab({ openEditor })`；弹窗 `DocEditor` `ChunkEditor` `IngestModal`（同文件）
> **消费的 MASTER 行**：§12 行 **6、7、18**（本页特有）+ README §2 全局
> **消费的变更编号**：`C5`（次级按钮去信号色）`C2 T2 T5 T6` · `S1` · `R1 R3`（`.prompt-view` 圆角 8→6）`A1 A2 A3 A4` · `Y1 Y3` · `Z1`
> **冻结选择器**：`.view-bar h2`、`.ant-empty`、`.ant-table`（Wiki 构建详情若用表）、`.row-btn`（`knowledge.spec.ts`）

## 1 职责与首要任务

**职责**：知识库的**摄取 / 检索 / 编辑真相源 / 查看索引**，以及 **Wiki（生成页）的编目与构建**。
**首要任务**：**找到一份文档并读/改它的原文**（markdown 是真相源，chunk 只是索引产物）。

据此决定：
- **检索框常驻在内容之上**（`.search-bar`），不藏在 tab 里：知识库的第一动作永远是"我上次存的那个东西在哪"。
- 文档行点击 = **展开 chunk 列表**（就地），编辑原文是行内动作（`编辑`/`删除`），不是先跳详情页——因为"改真相源"和"看索引结果"要在同一个视线里完成。
- 编辑 chunk 时**必须显示它所属的 section 原文**（`details.hint` → `.raw`），并明确警告"下次重建索引会覆盖此修改"——这是"markdown 是真相源"这一设计的必要提示，缺了它用户会以为改 chunk 是永久的。
- 回滚（revision）是**破坏性动作**：`Popconfirm` + busy（审计 P2 记录当前缺失）。
- Wiki 是**生成物**：页面上必须一直带「生成」标签与 stale/edited 标记，不得与手写知识混排。

## 2 版式骨架

```
1440：侧栏 228 │ content 1212（可用 1149）
┌ .view-bar ───────────────────────────────────────────────────────────────┐
│ h2 知识库  .muted「副题 (embedder)」  [文档│Wiki]  .grow  [重建索引](popconfirm) [+ 摄取文档] │
├ .readout-strip panel 1149×91 ────────────────────────────────────────────┤
│      4文档            │            11分块                                │
├ .search-bar 1149×32 ─────────────────────────────────────────────────────┤
│ [Input.grow 搜索文档（语义 + 关键词）…]                                   │
├ tab=文档 ───────────────────────────────────────────────────────────────┤
│ .card > .row-btn ×4                ┌ 展开后 ────────────────────────────┐ │
│  .doc-icon │ strong.title <name>   │ .chunks（缩进 30px）                │ │
│  .tag <source>  .muted「N 分块」    │  .chunk-item ×N                     │ │
│  .grow  .time  [编辑][删除]         │   <pre class="raw">…</pre>          │ │
│                                    │   .chunk-actions [编辑][历史]       │ │
│                                    │   .chunk-revisions                  │ │
│                                    │    .revision > .rev-label「旧」+ raw │ │
│                                    │               > .rev-label「新」+ raw │ │
│                                    └────────────────────────────────────┘ │
├ tab=Wiki ───────────────────────────────────────────────────────────────┤
│ 行：生成 N 页 · 构建 N 次        .grow        [编译（干跑→确认）]           │
│ .row.tight「待补页面（wanted）」  .tag.err.mono <slug>?                    │
│ .card > .row-btn ×N  页面：.doc-icon + <title> + [stale][edited][orphan]   │
│                        + .muted.mono <slug> + .muted「出链 N / 入链 N」    │
│                        + .muted <sources 逗号连接>                        │
│ .dash-head「构建历史」 > .card > .row-btn ×N                              │
│   strong.mono #<id>  .tag<status>  .tag<scope>  .muted.mono <agent>        │
│   .muted「写入 N / 计划 N」  .time                                        │
│   展开 → .chunks > .chunk-item（.tag<action> + .mono slug + .tag<status>） │
└──────────────────────────────────────────────────────────────────────────┘
浮层：DocEditor / ChunkEditor / IngestModal → .modal.wide + Input.TextArea.md-editor
      Wiki 页阅读器 → .md.wiki-view（[[wikilink]]；断链用 .wiki-broken）
      编译弹窗：Scope Segmented(全部/有变更) + agent Select + 干跑计划预览（max-height 320 滚动）
390：content 318（294）｜.readout-strip 150px；.search-bar 294×32 单行；.card 294；
     .chunks 缩进收窄；pageH 844（tab=文档 4 行）
```

**实测**：`.search-bar` 1149×32（@1440/1280）、662×32（@768）、294×32（@390）；`.card` 200 高（文档列表）；`.chunk-item` 由内容决定高度（`.raw` 提供滚动上限）。

## 3 层级

**本页最大元素 = `.readout`（32/40 mono，2 个）**，实测 `maxFs = 32`、≥18px 节点 **3** 个（h2 + 2 读数）。

为什么只 2 格：知识库只有两个有意义的总量（文档数、分块数）。**分块数是索引健康的唯一指标**（文档没变但分块数下降 = 索引坏了），所以它与文档数平级。
次级：`.title`（文档名，14/21 w400——`<strong>` 只给语义不给视觉重量）；`.chunk-item` 的 `<pre class="raw">`（**mono 12/16**，真实数据 ✔）；`.muted.mono` slug 与 score。
第三级：`.tag`（source / status / stale / edited / orphan）。

**禁止**：给 `.chunk-item` 的 `pre` 用比例字体（它是索引产物 = 真实数据，必须 mono）；给「待补页面」用琥珀（wanted 不是"现在"，用 `.tag.err`——它表示"缺"）。

## 4 内容模型

### 4.1 数据源

| 调用 | 返回类型（`api.ts`） | 用到字段 | 展示位 |
|---|---|---|---|
| `api.knowledgeDocs()` | `{ documents: KnowledgeDocument[]; embedder: string }` | `documents[].id, name, source, chunk_count, created_at`；`embedder` | 文档列表；`embedder` 拼在 `.view-bar` 副题里 |
| `api.knowledgeSearch(q)` | `{ hits: SearchHit[] }` | `chunk_id, document, content, score` | 检索结果（`.search-hit`） |
| `api.knowledgeChunks(id)` | `{ chunks: [number, string][] }` | `[chunk_id, text]` | 展开后的 `.chunk-item` |
| `api.knowledgeRaw(name)` | `string`（纯文本端点） | — | `DocEditor` 的初始内容 |
| `api.knowledgeSave(name, content)` | `{ chunks: number; file: string }` | 两者 | 保存 toast（"已写入 N 分块 → file"） |
| `api.knowledgeIngest(name, content)` | `{ chunks: number }` | `chunks` | 摄取 toast |
| `api.knowledgeDelete(id)` | — | — | 删除（**必须 `Popconfirm`**） |
| `api.knowledgeEditChunk(id, content)` | `KnowledgeEditOutcome { revision, document, chunks }` | 三者 | 编辑 chunk；**提示 revision 号** |
| `api.knowledgeChunkRevisions(id)` | `KnowledgeRevision[]` | `id, chunk_id, document_name, old_content, new_content, edited_at` | `.chunk-revisions` |
| `api.knowledgeRollback(revisionId)` | `KnowledgeEditOutcome` | — | 回滚（**`Popconfirm` + busy**） |
| `api.knowledgeExpand(chunkId)` | `KnowledgeExpansion { chunk_id, document, section, chunk, file }` | `section` | `ChunkEditor` 的 `details.hint` 预览 |
| `api.knowledgeRebuild()` | `{ rebuild: KnowledgeRebuildReport }` | `indexed, unchanged, removed, errors` | 重建结果 toast（**四个数都要报，`errors > 0` 必须用 `tag.err`**） |
| Wiki：`api.wikiPages()` | `{ pages: WikiPageInfo[] }` | `slug, title, summary, aliases, entities, sources, stale, edited, links_out, links_in` | Wiki 页面列表 |
| Wiki：`api.wikiLinks()` | `WikiLinks { nodes, edges, broken, orphans }` | `broken`（wanted 页）、`orphans` | 「待补页面」行 + orphan 标记 |
| Wiki：`api.wikiBuilds(20)` / `api.wikiBuild(id)` | `WikiBuild[]` / `WikiBuildDetail` | `id, scope, status, dry_run, agent, pages_planned/written/failed, error, started_at, finished_at` / `plan, pages[{slug, action, status, error}]` | 构建历史 + 展开的计划明细 |
| Wiki：`api.wikiBuildStart({scope, dry_run, agent})` / `api.wikiBuildConfirm(buildId, agent)` | `WikiBuildStarted { build_id, status, agent, pages_planned, plan?, notes? }` | `plan[]`（`WikiPagePlan: slug, title, action, summary?, aliases?, entities?, sources?`） | 干跑预览 → 确认 |

**分组**：tab（2）→ 文档列表（扁平）/ Wiki 三段（待补页面 / 页面 / 构建历史）。
**密度上限**：

| 项 | 实测 | 上限 | 手段 |
|---|---|---|---|
| 文档数 | 4 | **200** | 超出分页 50/页（`search-bar` 优先） |
| 单文档 chunk 数 | 11（全库） | **展开后 ≤200 行** | 超出「显示前 200 / 共 N」 |
| `.revision` 数 | 0 | **≤50**（最近 50 条） | 服务端 limit |
| 检索命中 | — | **≤50** | 超出「只显示前 50」 |
| Wiki 页数 | — | ≤300 | 分页 |
| 构建历史 | — | ≤20（`wikiBuilds(20)`） | 已限 |
| 展开的文档 | 0 | **≤3 同时展开** | 控制 `.chunks` 的内存量 |
| 节点数 | 197 | ≤1200 | — |

## 5 四种状态

| 态 | 契约 | 现状 |
|---|---|---|
| **loading** | `docs === null` → 需补 `Spinner`（**现状未显式处理**）；Wiki tab 的 `Promise.all` 无 loading 分支 | **违反** |
| **empty** | ①无文档 → `Empty` + 「还没有文档」+ 摄取动作（`.empty-action`）②检索无命中 → `Empty` + 「没有命中」③无 Wiki 页 → `Empty` + 「还没有生成 Wiki」④无构建历史 → `Empty` | **现状：四处都没有 `Empty`**（渲染空 `.card`）→ 违反 P6 |
| **error** | `knowledgeDelete` / `ingest` / `rollback` / `rebuild` / `wikiBuild` 失败现状只有 toast。规格要求：①`role=alert` 常驻 + 重试 ②**破坏性动作（删除文档、回滚）必须 `Popconfirm` + busy 防连点**（审计 P2）③摄取/保存失败时**内容留在编辑器里**，不清空 | **违反** |
| **密集** | 见 §4.2。额外：①`.chunk-item` 的 `pre.raw` `max-height` 固定，避免 11 个长 chunk 把页面撑到 8000px ②`.revision` 的 `old/new` 两个 `pre` 各 `max-height: 140px`（现状 ✔）③Wiki 计划预览 `max-height: 320`（现状 ✔） | 部分达标 |

## 6 响应式行为

| 视口 | 侧栏 | content | 行为 |
|---|---|---|---|
| **390** | 72 | 318（294） | `.readout-strip` 高 150px（2 格换行，每格 ~150 宽）；`.search-bar` 单行 294（Input `.grow` 收缩，按钮不换行）；`.chunks` 左缩进 30→12px；pageH 844 |
| **768** | 72 | 696（662） | 文档行内 `.time` 与 `.tag` 让位（≤520 规则外的第一档） |
| **1280** | 228 | 1052（996） | `.card` 996 |
| **1440** | 228 | 1212（1149） | `.card` 1149；pageH 900 |

横向溢出：**0**（四视口实测全 0）。`.chunk-item` 的 `pre` 用 `white-space: pre-wrap` + `word-break: break-word`（`index.css` `.prompt-view > pre` 已有同类规则），长 token 不会撑破 294px。

## 7 复用的共享原语与类名

`.view-bar` `.readout-strip` `.readout` `.readout-label` `.search-bar` `.card` `.row-btn` `.doc-icon` `.title` `.tag` `.tag.ok` `.tag.warn` `.tag.err` `.muted` `.mono` `.time` `.grow` `.pad` `.field` `.hint`
知识：`.chunks` `.chunk-item` `.chunk-actions` `.chunk-revisions` `.revision` `.rev-label` `.rev-old` `.md-editor` `.legacy-hint` `.raw`
Wiki：`.wiki-view` `.wiki-broken` `.wiki-q` `.prompt-view`
浮层：`.modal-backdrop` `.modal` `.modal.wide` `.modal-head` `.modal-body`
组件：`Spinner` `Empty` `Modal` `Markdown` `RelTime` `useToast` `Segmented` `Select` `Input` `Input.TextArea` `Button` `Popconfirm` `Icon`
**本页请求的 zone 化（Z1）**：`.search-bar` 与 Wiki 的「待补页面」行都是 zone 语义 → 用 `.zone-head` + `.zone-title`（计数进 `.zone-note`）。
新增类请求：`.sr-only`（隐藏 `h1`「知识库」）、`.micro`（**11/15**，与 `.muted` 叠用）、`.error-state` + `.spinner-block`（经 `ui.tsx` 的三态契约）。**本页不新增数据文本类** —— primitives §7 R3 已拒绝该请求（它与 `.mono` 逐字相同），数据文本用既有 `.mono`。

## 8 本页特有的验收数字

| ID | 指标 | 基线（实测） | 目标 | 判定（MASTER 行） |
|---|---|---|---|---|
| K1 | ≥18px 文本节点 / 最大字号 | **3 / 32px** | ≥3 / ≥32px | 行 6、7（刚过线，**不得再减**） |
| K2 | 单亮度带 / 色度 / 信号占比 | **54.4% / 2.51% / 0.72%** | ≤75% / 1.0–6.0% / ≤1.5% | 行 1、2、3 |
| K3 | 描边内容容器 | **0** | ≤1 且必须是控件单元 | 行 4 |
| K4 | 可见描边元素数 | **14**（`.md-editor`、Input、Button、`.prompt-view`） | ≤40 | 行 5 |
| K5 | <32px 命中目标 | **0**（当前实测 @1440；**历史**：裁决时 **12** ⇒ 超标 2，其后由共享层修复降到 0） | <24px = 0；<32px ≤10 | 行 18 ✅。**登记（2026-09-23 captain 裁决）**：本页 <32px 实测 **0** ⇒ **不加 `.icon-btn-lg`**（YAGNI，先测后报）；**若将来数字变了，按同一判据处理**（行 18 + `view-sessions.md` §4.2 的视口分流） |
| K6 | 无名称可交互元素 | **1** | **0** | **行 17** |
| K7 | 内联 style 节点数 | **17** | ≤50；内联 `font-size` = 0（Wiki 里有 6 处内联 12px） | 行 11、12 |
| K8 | `.prompt-view` 圆角 | **8px（阶梯外）** | **6px** | **行 10**（R3） |
| K9 | 四个 empty 态 | **0 个 `Empty`**（文档/检索/Wiki/构建历史都渲染空盒子） | 4 个 `Empty`，各带一句邀请 + 一个动作 | **行 20 同源 + P6** |
| K10 | 破坏性动作确认 | 删除文档/回滚**无确认、无 busy** | `Popconfirm` + busy 防连点 | 审计 P2 + 行 20 同源 |
| K11 | chunk 编辑的「真相源」提示 | 有 `details.hint` 预览 section | 保留，并**常驻**一行 `.legacy-hint`「重建索引会覆盖此修改」 | 本页特有（正确性） |
| K12 | 节点数 / 页高（文档 tab） | 197 / 900px | ≤1200 / ≤4000px | 本页特有 |
