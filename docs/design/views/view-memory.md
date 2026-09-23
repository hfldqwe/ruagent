# memory — `#memory`

> **路由**：`#memory`
> **视图**：`panel/src/views/Memory.tsx` → `Memory()`；内部组件 `MemoryCard` `SupersedeModal` `WriteModal` `AuditView` `RecallPlayground` `useStubFetch` + 5 个 `RecallStub*` 折叠块
> **消费的 MASTER 行**：§12 行 **6、7**（本页特有）+ README §2 全局
> **消费的变更编号**：`C5`（**核心：次级按钮去信号色 `V4`**）`C2 T5 T6` · `S1` · `R1 R2` · `A1 A2 A3 A4` · `Y1 Y2 Y3` · `Z1 Z2`
> **冻结选择器**：`.view-bar h2`、`.ant-empty`、`.row-btn`（治理：搜索框 e2e 见 `recall.spec.ts`）

## 1 职责与首要任务

**职责**：六大记忆存储的**浏览器 + 写入端 + 召回实验室 + 审计日志**。
**首要任务**：**在某个 store/namespace 里找到一条记忆、看清它是否已被取代、必要时取代它**。

据此决定：
- 三块用 `Segmented` 分屏（浏览 / 召回 / 审计日志），**不并列**：三者是三种任务，同屏会互相打断。
- store 切换由 `ReadoutStrip` 的 4 格承担（`onOpen` → 切 store + namespace）——**读数即导航**，这是本页最好的一处设计，必须保持。
- `记忆是否被取代` 是这一页的核心信息：`.tag.ok`「当前」/ `.tag.warn`「已被取代 + 日期」+ 被取代条目 `opacity .6` + 正文 `line-through`。三重信号保留。
- 召回实验室用**渐进披露**：命中先出 stub（`.recall-stub-head`），展开才拉详情（`useStubFetch`）——因为召回结果里 80% 是不需要展开的。

## 2 版式骨架

```
1440：侧栏 228 │ content 1212（可用 1149）
┌ .view-bar ────────────────────────────────────────────────────────────┐
│ h2 记忆  .muted 副题  .grow  [浏览│召回│审计日志]  [+ 写入记忆](primary,仅浏览) │
├ .readout-strip panel 1149×99（4 格，每格可点）────────────────────────────┤
│  34画像 │ 35观察 │ 22流程 │ 33经验    ← 点一格 = 切 store+namespace      │
├ tab=browse ───────────────────────────────────────────────────────────┤
│ .filter-bar  [画像│观察│流程│经验]  <namespace Select 170px>  [+ 新建命名空间] │
│ .memory-list  flex column gap 10                                        │
│  .card.memory-card ×9（[.superseded]）                                   │
│   ┌ .row.tight ──────────────────────────────────────────────────────┐  │
│   │ .tag<store> .tag<namespace> [.tag.ok 当前 | .tag.warn 已被取代 yyyy-mm-dd]  │
│   │ .muted.mono「取代 #id」   .grow   .time <title=updated_at>        │  │
│   ├──────────────────────────────────────────────────────────────────┤  │
│   │ p.memory-content 14/22（[.old → 删除线 + 0.6 透明]）                │  │
│   ├──────────────────────────────────────────────────────────────────┤  │
│   │ .row.tight   [替代] [删除]  ← 必须是次级(neutral)，不是琥珀         │  │
│   └──────────────────────────────────────────────────────────────────┘  │
├ tab=recall ───────────────────────────────────────────────────────────┤
│ .search-bar  [Input.grow 搜索记忆…]  [Conservative│Aggressive]           │
│ .card  策略 .tag + 计数行                                                │
│   ▸ memories  → .search-hit ×N        .recall-hit > .recall-stub-head    │
│   ▸ knowledge → .search-hit ×N        （tag document + mono score）      │
│   ▸ wiki      → .recall-stub-head（tag wiki + .tag.warn 生成物 + stale） │
│   ▸ entities  → .recall-stub-head（&id + kind + facts/related）          │
│   .recall-expand[.open] > .recall-inner > .recall-body.md                │
├ tab=audit ────────────────────────────────────────────────────────────┤
│ .card > .diff-row ×N   时间.mono │ op │ store.ns │ before→after │ reason │
└───────────────────────────────────────────────────────────────────────┘
390：content 318（294）｜.readout-strip 高 300px（4 格换行）；.memory-list 卡高 136；
     .filter-bar 换行 3 行；pageH 2834
```

**实测**：`.memory-list` 1149×1432（9 卡，卡高 126）；`.readout-strip` 99px（@1440/1280）、99px（@768）、**300px**（@390）；`.card` 1149/996/662/294。

## 3 层级

**本页最大元素 = `.readout`（32/40 mono，4 个）**，实测 `maxFs = 32`、≥18px 节点 **5** 个（h2 + 4 读数）。

为什么是 4 格：**4 个 store 是这一页最重要的分类轴**，读数格 + 可点击 = store 切换器。这不是"统计"，是**顶层的分组导航**。
次级：`.memory-content` 14/22（正文，weight 400）；`.tag` 11/15（**sans**，primitives §3.1：mono 只用于真实数据，tag 不是）；`.time` 11/15 mono。
**被取代条目的层级**：整卡 `opacity .6` + 正文 `line-through`——**降低层级用透明度和删除线，不用字号**（字号已经最小）。

**禁止**：给 `.memory-content` 加粗；给 4 格读数加图标；用琥珀色标记"当前"（P2 白名单里没有"当前"这个语义——当前用 `.tag.ok` 绿色）。

## 4 内容模型

| 调用 | 返回类型（`api.ts`） | 用到字段 | 展示位 |
|---|---|---|---|
| `api.memoryList(store, namespace)` | `{ memories: MemoryRow[]; counts: [string, string, number][] }` | `counts` = `[store, namespace, n][]` → 4 格读数与 namespace 选项；`memories` | 主列表 |
| `api.memoryWrite(store, namespace, content)` | `{ outcome: string }` | `outcome` | 写入 toast（**`outcome` 是后端枚举，规格要求翻译成人类文案，不得直接吐 "`superseded`"**——审计 §八已记录此缺陷） |
| `api.memorySupersede(id, newContent)` | `{ outcome: string }` | 同上 | 取代 toast |
| `api.memoryDiffs(limit=200)` | `MemoryDiff[]` | `id, ts, op, mem_store, namespace, before, after, reason` | 审计 tab 的 `.diff-row` |
| `api.recall(q, conservative, topN=5)` | `RecallResult` | `strategy`、`memories[]`、`knowledge[]`、`wiki[]`、`entities[]` | 召回 tab |
| `api.memoryGet(id)` | `{ memory: MemoryRow }` | — | stub 展开（`useStubFetch`） |
| `api.knowledgeExpand(chunkId)` | `KnowledgeExpansion` | `chunk_id, document, section, chunk, file` | knowledge/wiki stub 展开 |
| `api.graphFacts(id)` | `GraphEdge[]` | `relation, fact_text, valid_at, invalid_at` | entity stub 展开 |
| `api.graphNeighbors(id, 1)` | `[GraphEntity, number][]` | — | entity stub 展开 |

**`MemoryRow` 字段 → 展示位**

| 展示位 | 字段 | 规则 |
|---|---|---|
| `.tag`① | `store` | `profile\|observation\|procedure\|lesson` |
| `.tag`② | `namespace` | 受 `NAMESPACES` 约束：`profile:[user]`、`observation:[user,global]`、`procedure:[global]`、`lesson:[global]` |
| `.tag.ok` / `.tag.warn` | `superseded_at` | `null` → 「当前」(`.tag.ok`)；非 null → 「已被取代 {date}」(`.tag.warn`) |
| `.muted.mono` | `supersedes` | 非 null → 「取代 #{id}」 |
| `.time` | `updated_at`；`title` = 全量时间 | 经 `RelTime` |
| `.memory-content` | `content` | superseded 时加 `.old` |
| `confidence` | — | **当前未展示**。规格要求：`confidence < 0.5` 时在行尾加 `.muted.mono` 显示两位小数（真实数据，且它是唯一没露出的质量信号） |

**分组**：tab（3）→ store（4）→ namespace（枚举）。召回结果分 4 段（memories / knowledge / wiki / entities），**wiki 必须单独成段且带「生成物」标签**（`memory.recallWikiGenerated`）——它永远不与 knowledge 混排。
**密度上限**：

| 项 | 实测 | 上限 | 手段 |
|---|---|---|---|
| 记忆卡数 | 9 | **100/页** | `memoryList` 无分页参数 → 前端切片 100 + 底部「已显示 100 / 共 N」 |
| 卡高 | 126px | ≤200px | `.memory-content` `max-height` + 展开 |
| `.diff-row` 数 | 0（本机无审计） | **≤200**（= `memoryDiffs(200)`） | 已限 |
| 召回 stub 数 | — | **≤5/段**（`recall(q, …, topN=5)`） | 已限 |
| 展开后的 `.recall-body` | — | `.raw` `max-height` + `.recall-expand` 单开（同时只允许 1 个展开） | `.recall-expand` 已有 `grid-template-rows` 动画；**规格要求改为手风琴（单开）**以防 4 段同时展开 |
| 页高 | 1766px | ≤4000px | — |
| 节点数 | 250 | ≤800 | — |

## 5 四种状态

| 态 | 契约 | 现状 |
|---|---|---|
| **loading** | `memories === null` → `Spinner`（`Memory.tsx` browse 分支）；`diffs === null` → 审计 tab 内 `Spinner`；召回 busy → 按钮 `loading` | 已实现 |
| **empty** | ①`memories.length === 0` → `Empty icon="brain"` + `memory.empty.title`（带 store 与 namespace 名）+ `memory.empty.hint`；②无审计记录 → 需补（现状 `diffs.length === 0` 渲染空 `.card`，规格要求 `Empty`）；③召回无命中 → 需补「没有命中 · 试试激进策略」 | ①✅ ②③**违反** |
| **error** | `memoryList` / `memoryDiffs` / `recall` 的 `catch` 现状兜成空数组 → 显示"暂无记忆"。规格要求：①`role=alert` + 重试 ②**写入/取代失败必须常驻显示**（现状只有 2–4s toast，且抛后端枚举）③取代是破坏性动作 → `Popconfirm` + busy 禁用防连点（审计 P2） | **违反** |
| **密集** | 见 §4.2 上限。额外：①`.recall-body` 的长 chunk 用 `.raw` 滚动 ②`.diff-row` 的 `before/after` 各截断到 1 行 + `title` 全量 ③`memory-content` 超过 6 行折叠 | ②③待实现 |

## 6 响应式行为

| 视口 | 侧栏 | content | 行为 |
|---|---|---|---|
| **390** | 72 | 318（294） | `.readout-strip` 高 **300px**（4 格逐行换行）→ 规格要求 ≤520 启用 `.readout-strip.grid`（2×2，**高 164px** = 2×66 + 上下 padding 32；2026-09-21 实测校正，原为 `~150px` 估值）。**前置条件（2026-09-21 补）**：需**内容盒 ≥280px**（2×`minmax(140px,1fr)`，primitives §3.2 B7）→ **视口 ≥ 约 416px**；390 视口的内容盒只有 **254px**，仍是逐行换行。B7 的 `auto-fit` 是 **>520 的默认行为**（不要改），≤520 特化由 `t18` 落地（README §3.4 X4）；`.filter-bar` 换行成 3 行；`.memory-list` 卡高 136；pageH 2834 |
| **768** | 72 | 696（662） | `.readout-strip` 99px（4 格单行挤在 662 内，每格 min 130px）；pageH 1954 |
| **1280** | 228 | 1052（996） | 无变化；`.card` 996 |
| **1440** | 228 | 1212（1149） | `.readout-strip` 99px；pageH 1766 |

横向溢出：**0**（四视口实测全 0）。`.tag` 在行内已受 `max-width:40%` 约束，长 namespace 名不会撑破行。

## 7 复用的共享原语与类名

`.view-bar` `.readout-strip` `.readout` `.readout-label` `.readout-btn` `.filter-bar` `.memory-list` `.card` `.memory-card` `.superseded` `.memory-content` `.old` `.row` `.row.tight` `.tag` `.tag.ok` `.tag.warn` `.muted` `.mono` `.time` `.grow` `.pad` `.field` `.hint`
召回：`.search-bar` `.search-hit` `.hit-content` `.recall-hit` `.recall-stub-head` `.stub-text` `.stub-afford` `.recall-expand` `.open` `.recall-inner` `.recall-body` `.recall-chunk-hl` `.recall-fact` `.old` `.recall-loading` `.chev` `.md` `.raw`
审计：`.diff-row` `.diff-detail`
组件：`ReadoutStrip` `Spinner` `Empty` `Modal` `Markdown` `RelTime` `useToast` `Segmented` `Select` `Input` `Button` `Popconfirm`
**本页请求的 zone 化（Z1）**：`.filter-bar`（store + namespace + 新建）是 zone 语义（一行规则 + 一个分组标签）→ 用 `.zone-head` + `.zone-title` 包裹，`.zone-note` 放计数。
新增类请求：`.sr-only`（隐藏 `h1`「记忆」）、`.micro`（**11/15**，与 `.muted` 叠用）、`.error-state` + `.spinner-block`（经 `ui.tsx` 的三态契约）。**本页不新增数据文本类** —— primitives §7 R3 已拒绝该请求（它与 `.mono` 逐字相同），数据文本用既有 `.mono`。

## 8 本页特有的验收数字

| ID | 指标 | 基线（实测） | 目标 | 判定（MASTER 行） |
|---|---|---|---|---|
| M1 | ≥18px 文本节点 / 最大字号 | **5 / 32px** | ≥3 / ≥32px | 行 6、7 |
| M2 | 单亮度带 / 色度 / 信号占比 | **23.7% / 3.15% / 0.81%** | ≤75% / 1.0–6.0% / **≤1.5%** | 行 1、2、3 |
| M3 | **次级按钮的颜色** | **10 个 `button` 用 antd 派生 primary 文本档 `rgb(211,156,77)`**（V4） | **0 个**：改 `--ant-color-text-secondary`，hover 才上色 | **行 3 + §3.2 黑名单 V4** |
| M4 | 描边内容容器 | **0** | ≤1 且必须是控件单元 | 行 4 |
| M5 | 可见描边元素数 | **5** | ≤40 | 行 5 |
| M6 | <32px 命中目标 | **15** | <24px = 0；<32px ≤10 | **行 18**（超标 5 个：`.tag` 内的图标按钮与 `.chunk-actions` 同类） |
| M7 | 无名称可交互元素 | **1** | **0** | **行 17** |
| M8 | 内联 style 节点数 | **28**（其中 `.hit-content.mono` 带内联 12px ×4） | ≤50；内联 `font-size` = 0 | 行 11、12 |
| M9 | 记忆卡数 / 页高 | 9 / 1766px | ≤100 / ≤4000px | 本页特有 |
| M10 | 同时展开的 `.recall-expand` | 无限制 | **≤1**（手风琴） | 本页特有 |
| M11 | 写/取代反馈 | 2–4s toast，且直接吐后端枚举（如 `superseded`） | 常驻 `role=status` 结果行 + 翻译文案 + `Popconfirm` | 行 20 同源 + 审计 §八 |
| M12 | `confidence` 可见性 | **未展示** | `< 0.5` 时显示两位小数 | 本页特有（正确性） |
