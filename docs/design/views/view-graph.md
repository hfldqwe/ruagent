# graph — `#graph`

> **路由**：`#graph`
> **视图**：`panel/src/views/Graph.tsx` → `Graph()`；内部 `GraphCanvas`（rAF 力导向）、`EntityDetail`（检查器）、`NewEntityModal`、`AddFactModal`
> **消费的 MASTER 行**：§12 行 **3、6、7、31、33**（本页特有；**31 与 33 只有本页有意义**）+ README §2 全局
> **消费的变更编号**：`C2 C4`（**核心：`--graph-edge` 亮色 2.17:1 → .50**）`C5` · `T3 T5 T6` · `S1` · `R4`（`.graph-detail` 400px 固定 + `.graph-canvas` 520px 固定 → `clamp()`）`A1 A2 A3` · `Y1` · `Z1`
> **冻结选择器**：`.view-bar h2`、`.ant-empty`、`.ant-table`

## 1 职责与首要任务

**职责**：实体-关系图谱的**探索**（力导向画布 + 列表两种模式）与**检查**（某个实体的双向事实、时间点查询、邻域）+ 手工补实体/事实。
**首要任务**：**点开一个实体，看清它现在和谁有什么关系**。画布负责"发现"，检查器负责"确认"。

据此决定：
- **画布不是全屏**：`.graph-layout` 左画布 + 右检查器（400px）。画布负责"哪块密"，检查器负责"具体是什么"——两者必须同屏，否则每次点击都要往返。
- **列表模式与画布并列成 Segmented**（图/列表），不是可选装饰：**画布对键盘与读屏用户不可达**，列表模式是可访问性的等价物（§11 的硬要求）。
- 时间点是图谱的**一等参数**（`valid_at` / `invalid_at` 双时态）：检查器顶部常驻 `.mono` 日期输入 + 「截至」标签，查询结果里被失效的事实加 `.old`（`opacity .55`）。
- 节点数有硬上限（`MAX_NODES = 150`，`Graph.tsx:14`），超出时**必须显式告知**（`graph.tooMany`），否则用户以为图谱只有 150 个实体。

## 2 版式骨架

```
1440：侧栏 228 │ content 1212（可用 1149）
┌ .view-bar ─────────────────────────────────────────────────────────────┐
│ h2 实体图谱  .muted 副题  .grow  [图│列表]  [+ 新建实体](primary)          │
├ .readout-strip panel 1149×91 ──────────────────────────────────────────┤
│       55实体          │         100事实                                  │
├ .search-bar 1149×32 ───────────────────────────────────────────────────┤
│ [Input.grow 搜索实体（名称、摘要）…]                                     │
├ .graph-layout  flex gap 14, align-items flex-start ────────────────────┤
│ ┌ .graph-main  flex 1 1 auto (≈735) ─────────┐ ┌ .graph-detail 400 ────┐ │
│ │ .graph-canvas  100% × 520px               │ │ .graph-detail-head    │ │
│ │   <canvas> rAF 物理：库仑斥力+弹簧+向心   │ │  h3 <name>  .tag<kind> │ │
│ │   DPR 缩放 devicePixelRatio               │ │  .grow  [关闭]         │ │
│ │   touch-action: none                      │ │ p.intent <summary>     │ │
│ │   MAX_NODES=150 → 超出只画连接最多的      │ │ .card                  │ │
│ │ .graph-hint 11/15「滚轮缩放 · 拖拽平移…」  │ │  .row.tight「截至」     │ │
│ │   （超出时追加 graph.tooMany 文案）        │ │   Input.mono <valid_at>│ │
│ └───────────────────────────────────────────┘ │  Table.facts           │ │
│  list mode: .card > .row-btn ×N               │   relation|事实|生效|失效│ │
│   .doc-icon │ <name 14/21> │ .tag<kind>        │ .card（邻域）           │ │
│   .muted「N 条事实」 │ .grow │ .muted.truncated│  .row.wrap             │ │
│                                              │   .neighbor ×N（图标+名+跳数）│
│                                              └────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────┘
390：content 318（294）｜.graph-layout 仍 flex → 规格要求 ≤1024 换列（画布在上、检查器在下）
     .graph-canvas 294×520（不缩宽只缩高 → 见 R4）；pageH 967
```

**实测**：`.graph-layout` 1149×541；`.graph-canvas` **1149×520**（@1440）→ 这里有个真实缺陷：**画布没有为 400px 检查器让出宽度**（`.graph-layout` 的 flex 里 `.graph-main` 是 `flex: 1 1 auto`，但当检查器未挂载时画布独占 1149；挂载后才是 ~735）。规格要求：**检查器宽度恒预留**（未选中时也要占位，用 `.zone` 空态），否则选一个实体画布会突然从 1149 缩到 735，力导向布局整体重排。

## 3 层级

**本页最大元素 = `.readout`（32/40 mono，2 个）**，实测 `maxFs = 32`、≥18px 节点 **3** 个（h2 + 2 读数）。

为什么是 2 格：实体数与事实数是这一页仅有的两个全库总量，且它们的关系（事实/实体 ≈ 1.8）本身就是信息。
次级：`.graph-detail-head h3` 16/24 —— **实测 weight 是 650（阶梯外）→ 必须改 600**（MASTER §4.3 / T3）。
第三级：`.facts td` 13/18；`.neighbor` 13/18；`.graph-hint` 11/15。
**画布本身的"层级"由颜色承担**：节点按 `kind` 取 `--graph-*`（9 色 + default），边取 `--graph-edge`。这是 P2 的**反面许可区**：本页**允许**多色，因为颜色表达的是"实体类型"，与"现在"无关——但**颜色不得离开本视图**（MASTER §3.1 F / W6）。
**环的颜色（2026-09-23 裁决）**：选中 / 搜索命中时，节点画 2px 外环（`Graph.tsx:623-628`），颜色是 accent（`pal.ring = --ant-color-primary` = `--signal`）—— 这是 **§3.2 白名单 W7**：环表达「**现在**（你选中 / 命中的那个）」，与 W1（当前导航项）同类，**不是分类色**。**约束**：环不得是唯一线索（选中另有标签与检查器、命中另有半径脉冲）；**新增的节点色仍不得落进琥珀带**（见上「禁止」条与 G4）。**注意**：悬停（`s.hoverId`）只影响标签、**不画环**（`:587` vs `:623`）。

**禁止**：给节点用琥珀（暖黄档 H≈42° 视觉上接近信号 `--signal` H≈36°）。**本页曾违反此条**：`--graph-protocol` 原为暗 `#ca9d33` / 亮 `#8b6100`（H=42°）；**2026-09-22 已按本节要求换成非暖色相 —— 暗 `#8fa3e8`（H=227°）/ 亮 `#5b6fc4`（H=229°）**（旧值保留为历史记录，见 §8 G4）。理由：琥珀在 `#graph` 里会与「运行中」混淆 —— 图谱里没有「运行中」这个概念，暖黄档只会制造假信号。**新增的节点色不得再落进 H ∈ [20°, 50°] 的琥珀带**（与 §8 G4 同一判据）。

## 4 内容模型

| 调用 | 返回类型（`api.ts`） | 用到字段 | 展示位 |
|---|---|---|---|
| `api.graphEntitiesAll(500)` | `{ entities: [GraphEntity, number][] }` | `[GraphEntity, factCount]`；`GraphEntity { id, name, kind, summary }` | 画布节点 / 列表行；**按 `factCount` 倒序取前 150** |
| `api.graphSearch(q)` | `{ entities: GraphEntity[] }` | `id` | 命中集合 `hitIds`（画布高亮 / 列表过滤） |
| `api.graphEntity(id)` | `{ facts: GraphEdge[] }` | `GraphEdge { id, src, dst, relation, fact_text, valid_at, invalid_at, source_episode }` | 画布连边；选中实体的边 |
| `api.graphFacts(id, at?)` | `{ facts: GraphEdge[] }` | 同上 + `at` 过滤 | 检查器 `.facts` 表（`at` 来自日期输入） |
| `api.graphNeighbors(id, hops=2)` | `{ neighbors: [GraphEntity, number][] }` | `[entity, distance]` | `.neighbor` 行（`·${d}` 显示跳数） |
| `api.graphCreateEntity(name, kind?, summary?)` | `{ id: number }` | `id` | 新建实体 |
| `api.graphAddFact({src, dst, relation, fact_text, valid_at?})` | `{ id: number }` | `id` | 加事实；**若 `dst` 名不存在，先 `graphCreateEntity(dst)` 再建边**（现状行为，规格保留但要求提示"已创建目标实体"） |

**字段 → 展示位**

| 展示位 | 字段 | 规则 |
|---|---|---|
| 节点颜色 | `normalizeKind(kind)` | `organization`→`org`；`null`→`default`；映射到 `--graph-*` 的 9 个键 |
| 节点图标 | `kindIcon(kind)` | 6 种图标（person/org/project/repo/tool/其他=`tag`） |
| `.tag` 文本（列表/检查器） | `kind` | 原样显示（`null` 不渲染） |
| `.muted`「N 条事实」 | `factCount` | 来自 `graphEntitiesAll` 的二元组 |
| `.facts` 表 4 列 | `relation` / `fact_text` / `valid_at` / `invalid_at` | `invalid_at == null` → `—`；非 null → 该行加 `.row-old` |
| `.neighbor` | `GraphEntity.name` + `distance` | `distance` 用 `.muted` 后缀 |
| 时间点输入 | 用户输入 → `api.graphFacts(id, at)` | `.mono`（日期是数据） |

**分组**：无分组（单图）。检查器内分两段：事实表（zone）与邻域（zone）。
**密度上限**：

| 项 | 实测 | 上限 | 手段 |
|---|---|---|---|
| 画布节点 | 55 | **150**（`MAX_NODES`） | 已有；**超出必须显示 `graph.tooMany`**（现状只在 list 的 hint 里显示，规格要求画布上也要可见） |
| 画布边 | 100 | ≤600 | 超过只画 `valid_at` 区间内的边（现状已按"当前有效"过滤） |
| 列表行数 | 55 | **≤300** | 超出虚拟化 |
| `.facts` 表行 | — | **≤100/实体** | 超出「显示前 100 / 共 N」 |
| `.neighbor` 数 | — | **≤50** | 超出截断 + 计数 |
| 页高 | 900px | ≤1600px | 画布 520 + 检查器并排 |
| 节点数 | 137 | ≤1500（画布是单 `<canvas>`，DOM 与实体数无关） | 本页是全站 DOM 最省的页之一 |

## 5 四种状态

| 态 | 契约 | 现状 |
|---|---|---|
| **loading** | `entities === null` → `Spinner` | ✅ |
| **empty** | ①无实体 → `Empty` + 「图谱是空的」+ 「+ 新建实体」动作 ②搜索无命中 → `Empty`「没有匹配的实体」③检查器无事实 → `p.muted.pad`「没有事实」（`graph.noFacts`）④无邻域 → `p.muted.pad`（`graph.noNeighbors`） | ③④✅ ①②待补 |
| **error** | ①`graphEntitiesAll` 失败现状 **未处理**（无 catch 分支显示）→ 规格要求 `role=alert` + 重试 ②**画布渲染失败**（`getContext("2d")` 返回 null）现状会静默不画 → 规格要求降级到列表模式并显示一行说明 ③`graphAddFact` / `graphCreateEntity` 失败 → 常驻错误行 + 保留表单值 | **违反** |
| **密集** | ①超过 `MAX_NODES`：只画连接最多的 150 个 + **画布上可见的截断说明** ②物理循环必须**收敛后停帧**（现状 rAF 持续 tick，直到阻尼收敛；规格要求：能量低于阈值后停止 rAF，交互时再启动）③`prefers-reduced-motion: reduce` → **不跑物理，直接按圆形/网格布局静态摆放**（MASTER §9.4 明确要求逐视图给图谱开关） | ③**缺失（必须补）**；②部分 |

## 6 响应式行为

| 视口 | 侧栏 | content | 画布 | 行为 |
|---|---|---|---|---|
| **390** | 72 | 318（294） | `.graph-canvas` 294×**520** | 现状：画布不换列，检查器在右被压到 320 最小宽 → **规格要求 `≤1024` 改纵向堆叠**（画布全宽 + 检查器在下）；画布高 ≤1024 时改 `clamp(320px, 56vh, 620px)`（R4） |
| **768** | 72 | 696（662） | 662×520 | 同上堆叠（`≤1024`）；pageH 900 → 堆叠后 ~1350 |
| **1280** | 228 | 1052（996） | ≈596×520（检查器 400） | 并排；`.graph-detail` `flex: 0 0 400px`（R4 改 `clamp(320px,32vw,460px)`） |
| **1440** | 228 | 1212（1149） | ≈735×520（检查器 400） | 并排；pageH 900 |

横向溢出：**0**（四视口实测全 0）。`.graph-detail` 的 `min-width: 320px` 是 390 视口不溢出的前提；改成 `clamp()` 后必须复测 390。

## 7 复用的共享原语与类名

`.view-bar` `.readout-strip` `.readout` `.readout-label` `.search-bar` `.card` `.row-btn` `.doc-icon` `.tag` `.muted` `.mono` `.truncated` `.grow` `.pad` `.field` `.intent` `.row` `.row.tight` `.row.wrap` `.row-old` `.facts` `.neighbor`
图谱：`.graph-layout` `.graph-main` `.graph-canvas` `.graph-hint` `.graph-detail` `.graph-detail-head`
浮层：`.modal`（via `Modal`）+ `.raw`（事实输入预览）
组件：`Spinner` `Empty` `ReadoutStrip` `Modal` `Markdown` `RelTime` `useToast` `Segmented` `Input` `Button` `Table`（antd `.facts`）
**本页请求的 zone 化（Z1）**：检查器内的「事实」与「邻域」两段是 zone（规则线 + 标签 + 计数）→ `.zone-head` + `.zone-title`（计数进 `.zone-note`）。
新增类请求：`.sr-only`（隐藏 `h1`「实体图谱」）、`.micro`（**11/15**，与 `.muted` 叠用）、`.error-state` + `.spinner-block`（经 `ui.tsx` 的三态契约）。**本页不新增数据文本类** —— primitives §7 R3 已拒绝该请求（它与 `.mono` 逐字相同），数据文本用既有 `.mono`。

## 8 本页特有的验收数字

| ID | 指标 | 基线（实测） | 目标 | 判定（MASTER 行） |
|---|---|---|---|---|
| G1 | **`--graph-edge` 对比度（亮色）** | **2.17:1（<3:1）** | **≥3.0**：亮色 alpha `.34 → .50`（实测 3.41:1） | **行 31**（本页是该行唯一持有者） |
| G2 | `--graph-edge` 对比度（暗色） | **3.08:1** | ≥3.0（保持） | 行 31 |
| G3 | 9 个分类色对比度 | 暗 **4.69–7.16** / 亮 **4.58–5.88** | ≥3.0 两模式 | **行 33**（本页是该行唯一持有者） |
| G4 | `--graph-protocol` 的色相（**已达成**） | 原 暗 `#ca9d33` / 亮 `#8b6100`（H=42°，与信号色 `#f0a93b` H=36° 同色相家族） | **已换非暖色相**：暗 **`#8fa3e8`（H=227°）** / 亮 **`#5b6fc4`（H=229°）**；对比度 暗 对 panel **6.82** / 对暗画布 7.76、亮 对白 **4.65** / 对亮画布 4.34（两模式 ≥3 ✔）；画布 A/B：旧 token 琥珀 165 px → 新 token **0** px（2026-09-22，t45） | 行 3 白名单 W6 的边界澄清（见 §3） |
| G5 | 信号色像素占比 | **0.34%** | ≤1.5% | 行 3 |
| G6 | ≥18px 文本节点 / 最大字号 | **3 / 32px** | ≥3 / ≥32px | 行 6、7 |
| G7 | `.graph-detail-head h3` 字重 | **650（阶梯外）** | **600** | **行 9**（T3） |
| G8 | `.graph-detail` 宽 / `.graph-canvas` 高 | **400px 固定 / 520px 固定** | `clamp(320px,32vw,460px)` / `clamp(320px,56vh,620px)` | **行 35 同源**（R4/B3） |
| G9 | 检查器宽度稳定性 | 未选中时画布 1149 → 选中后 ~735（**整体重排**） | 检查器宽度**恒预留** | 本页特有（本规格新增） |
| G10 | `MAX_NODES` 截断可见性 | 只在列表 hint 里显示 | **画布上也可见**（`graph.tooMany`） | 本页特有（正确性） |
| G11 | `prefers-reduced-motion` | **未处理**（rAF 物理照跑） | reduce 时**静态布局**，不跑物理 | **MASTER §9.4**（明确点名的唯一例外） |
| G12 | 画布可访问性 | `<canvas>` 无文本、无 `role` | 列表模式为等价路径 + 画布 `role="img"` + `aria-label`（实体数/事实数） | **行 24 同源 + §11.4** |
| G13 | rAF 停帧 | 收敛后疑似继续（需复测） | 能量 < 阈值即停帧；交互时重启 | 本页特有（性能） |
| G14 | `window.__graphDebug` | 挂载时设置、**卸载时 delete**（现状已清理，非泄漏） | 保留但要求 `import.meta.env.DEV` 门控 | 审计 §九的修正记录 |
| G15 | **`#graph` 首屏 `/api` 请求数** | 修前 **56**（1×`/graph/entities?limit=500` + **55×`/graph/entity/<id>`**，每实体一个 —— N+1） | **`R ≤ 2K + 2`**（行 39 的比例式；`R` **排除** `/permissions`）：修后实测 **R=1, K=1 ⇒ 1 ≤ 4 ✔**（修前 56 > 6 ✘）。外壳 `/permissions` **不计入本行**，由行 28 的独立断言判（全站最大 **7 ≤ 7** ✔）。原写「≤8（含 `/permissions`）」是行 39 落槌前的绝对数写法，已由 MASTER §12.10.3 取代 | **行 39**（主）+ **行 28**（外壳轮询单独判）—— 两者分工见 MASTER §12.10.1/§12.10.3 |
