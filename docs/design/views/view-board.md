# board — `#board`

> **路由**：`#board`
> **视图**：`panel/src/views/Board.tsx` → `Board({ onOpen })`；同文件 `CreateTaskModal({ onClose, onCreated })`
> **消费的 MASTER 行**：§12 行 **6、7**（本页特有）+ README §2 全局
> **消费的变更编号**：`C2`（`in_progress` 用信号色是白名单 W5，须保持）· `T5 T6` · `R2`（`.kanban-card` 圆角 8→10）· `S1` · `A2` · `Y1` · `Z1`（`.kanban-col` = zone）
> **冻结选择器**：`.view-bar h2`、`.kanban-col`、`.card.launcher`（在 task-detail）、`.row-btn`

## 1 职责与首要任务

**职责**：一张任务看板——按状态四列展示全部任务，允许切换成列表，允许新建任务，允许进入某一任务。
**首要任务**：**看"哪些任务卡住了"，并进入它**。「待处理/进行中」两列是行动区，「已完成」是归档区。

据此决定：
- 默认视图是**看板**（`view = "board"`），不是列表：状态分列让"卡住"可见。
- `ReadoutStrip` 的 4 格 = 4 列计数，且**只有 `in_progress` 且数量 > 0 时才染信号色**（`Board.tsx:53`）——这是 MASTER §3.2 W5 的正面样本，必须保持：数字表示"有东西正在跑"才配穿琥珀。
- 每张卡片**必须**有「查看日志 →」`.kanban-open`：这一页不负责执行，只负责导航。
- 新建任务用 antd `Modal`，不在看板里插一行内联表单——内联表单会把列挤变形。

## 2 版式骨架

```
1440：侧栏 228 │ content 1212（可用 1149）
┌ .view-bar ────────────────────────────────────────────────────────────┐
│ h2 任务   .muted「共 N 个」   .grow   [看板│列表]   [+ 新建任务](primary) │
├ .readout-strip  panel 1149×91 ────────────────────────────────────────┤
│  1待处理   │  1进行中(信号)  │  0阻塞  │  19已完成                      │
├ .kanban  grid[271,271,271,271] gap 22 ────────────────────────────────┤
│ .kanban-col        │ .kanban-col        │ .kanban-col   │ .kanban-col  │
│ ┌ .kanban-head ───┐│ ┌ .kanban-head ───┐│               │              │
│ │ 待处理      [1] ││ │ 进行中      [1] ││  阻塞    [0]  │  已完成 [19] │
│ ├─────────────────┤│ ├─────────────────┤│               │              │
│ │ .kanban-card ×N ││ │ …               ││  （空列不画盒）│              │
│ │  .row.tight     ││ │                 ││               │              │
│ │   ● <StatusDot> ││ │                 ││               │              │
│ │    <title 14/21>││ │                 ││               │              │
│ │  .tag <project> ││ │                 ││               │              │
│ │  .time 2 天前   ││ │                 ││               │              │
│ │  .kanban-open   ││ │                 ││               │              │
│ └─────────────────┘│ └─────────────────┘│               │              │
└───────────────────────────────────────────────────────────────────────┘
列高由最长列决定（实测 .kanban-col 1989px，min-height 180px）→ pageH 2269

列表视图：.card 1149 > .row-btn ×N
  ┌ .title ─────────────┐ .tag <project>  .muted <中文状态>  .grow  .time ┐
768：.kanban grid[320,320]（2 列，@media max-width:1240）   pageH 2419
390：.kanban grid[294]（1 列，@media max-width:700）        pageH 3024；.readout-strip 268px
```

**实测**：`.kanban` 1149 宽 / 4×271 列 / gap 22（@1440）；`[232]×4`（@1280）；`[320,320]`（@768）；`[294]`（@390）。
`.kanban-col` `min-height: 180px`；`.kanban-head` 是**标题 + 计数的规则线**，列本身**不是盒子**（zone，MASTER §7.2）。

## 3 层级

**本页最大元素 = `.readout`（32/40 mono，4 个）**，实测 `maxFs = 32`、≥18px 节点 **5** 个（h2 + 4 读数）。

为什么 4 格而不是 2 格（对比 sessions）：看板有**四个互斥状态**，四格读数与四列一一对应，读数即列头——它们不是装饰性的统计，是**导航**（`ReadoutStrip` 的 `onOpen` 可跳列）。
次级：`.kanban-head` 的 13/18 w600 + `.count`（11/15 mono，tabular）；卡片标题 14/21 w400。
卡片**不加粗标题**：一列 20 张全加粗会变成一片黑；靠"卡片 = 材质 + hover 变色"取胜（`.kanban-card:hover` 只改 background，无位移，MASTER §9.3）。

**禁止**：`.kanban-card` 加描边（现状无描边）；给列加背景/描边（列是 zone）；给"已完成"列染绿色。

## 4 内容模型

| 调用 | 返回类型 | 用到字段 | 展示位 |
|---|---|---|---|
| `api.tasks()`（3s 轮询，`Board.tsx:20`） | `Task[]` | `id, title, intent, status, project, created_at, updated_at` | 卡片与列表行；`status` 决定列；`project` → `.tag`（`null` 不渲染）；`updated_at` → `.time`（`RelTime`） |
| `api.createTask(title, intent, project?)` | `Task` | 返回值 `id` | 成功后 `onCreated(id)` → 直接跳 `#task/<id>`（**不留在看板**：新建后的下一步一定是启动运行） |

**状态枚举（硬编码，`Board.tsx:9`）**：`COLUMNS = ["pending", "in_progress", "blocked", "done"]`。
`Task.status` 是 `string`，出现枚举外的值时**归入 `pending`** 并在列头显示一行 `.muted`「N 个未知状态」——不得静默丢任务（现状：`tasks.filter(x => x.status === col)` 会让未知状态的任务**在四个视图里全部消失**，这是本页最严重的正确性缺陷）。
i18n：`board.col.{status}`（列头）/ `status.{status}`（列表行）。

**分组**：按 `status` 分 4 组（看板）或不分组（列表）。
**排序**：列内 = `updated_at` 倒序（现状未排序 → 规格要求显式排序，否则 3s 轮询会让卡片跳动）。
**密度上限**：

| 项 | 实测 | 上限 | 手段 |
|---|---|---|---|
| 卡片总数 | 21 | **200** | 超出后每列折叠到 **20 张 + 「还有 N 个 →」**（跳列表视图） |
| 单列高度 | 1989px | **≤2400px** | 同上 |
| 页高 | 2269px | **≤4000px** | 同上 |
| 节点数 | 278 | ≤1200 | — |
| 卡片文本 | — | 标题 1 行省略；`intent` **不渲染**（卡片只给标题，细节进详情页） | 现状已不渲染 |

## 5 四种状态

| 态 | 契约 | 现状 |
|---|---|---|
| **loading** | `tasks === null` → `Spinner`「看板…」（`Board.tsx:25`） | 可用，但标签取自 `board.title`（"任务"）；**规格要求 spinner 标签与页面标题一致** |
| **empty** | `tasks.length === 0` → `Empty icon="layers"` + `board.empty.title/hint` + **主操作按钮**「+ 新建任务」（`.empty-action`） | 完全符合 P6 |
| **error** | `refresh()` 的 `catch(() => setTasks([]))` 把失败变成"没有任务" → **违反行 20**。规格要求：①`role=alert` + 「重试」②保留上一次数据 + "数据可能过期"③3s 轮询失败**不得**把已有列表清空 | **违反** |
| **密集** | 见 §4 上限。额外要求：①3s 轮询后**不重排**（按 `updated_at` 稳定排序 + `key={id}`）②`.kanban-card` hover 只改背景 | 部分（无排序 → 会跳动） |

## 6 响应式行为

| 视口 | 侧栏 | 列数 | 列宽 | pageH | 行为 |
|---|---|---|---|---|---|
| **390** | 72 | **1** | 294 | 3024 | `@media max-width:700`；`.readout-strip` 高 **268px**（4 格换行）→ 规格要求 ≤520 启用 `.readout-strip.grid`（2×2；README §3.4 X4），视图不自己写 CSS |
| **768** | 72 | **2** | 320 | 2419 | `@media max-width:1240`；`grid[320,320]` |
| **1280** | 228 | **4** | 232 | 2266 | `>1240` 即 4 列 |
| **1440** | 228 | **4** | 271 | 2269 | gap 22 |

横向溢出：**0**（四视口实测全 0）。`.kanban` 用 `repeat(N, minmax(0,1fr))`，**必须保留 `minmax(0,…)`**——裸 `1fr` 会因卡片 min-content 撑破 768 视口（`index.css` 的 `.dash-grid` 注释记录了同类事故）。

## 7 复用的共享原语与类名

`.view-bar` `.readout-strip` `.readout` `.readout-label` `.readout-btn` `.kanban`（冻结结构）`.kanban-col`（冻结）`.kanban-head` `.count` `.kanban-card` `.kanban-open` `.row-btn` `.row.tight` `.tag` `.title` `.muted` `.time` `.grow` `.card` `.empty-state`（via `Empty`）`.modal`（via `Modal`）`.field` `.row.end`
组件：`ReadoutStrip` `StatusDot` `Spinner` `Empty` `RelTime` `Modal` `useToast` `Segmented`（antd）`Input`（antd）
**本页请求的 zone 化（Z1）**：`.kanban-head` 已手写实现 zone 语义（标题 + 规则线 + 计数）→ 规格要求把它的内部结构改为 `.zone-head` + `.zone-title` + `.count`（保留 `.kanban-head` 这个冻结类名本身）。
新增类请求：`.sr-only`（隐藏 `h1`「任务」）、`.micro`（**11/15**，与 `.muted` 叠用）、`.error-state` + `.spinner-block`（经 `ui.tsx` 的三态契约）。**本页不新增数据文本类** —— primitives §7 R3 已拒绝该请求（它与 `.mono` 逐字相同），数据文本用既有 `.mono`。

## 8 本页特有的验收数字

| ID | 指标 | 基线（实测） | 目标 | 判定（MASTER 行） |
|---|---|---|---|---|
| B1 | ≥18px 文本节点 / 最大字号 | **5 / 32px** | ≥3 / ≥32px | 行 6、7 |
| B2 | 单亮度带 / 色度 / 信号占比 | **58.9% / 1.94% / 0.48%** | ≤75% / 1.0–6.0% / ≤1.5% | 行 1、2、3 |
| B3 | 描边内容容器 | **0** | ≤1 且必须是控件单元 | 行 4 |
| B4 | 可见描边元素数 | **8** | ≤40 | 行 5 |
| B5 | `.kanban-card` 圆角 | **8px（不在 4/6/10/14 阶梯内）** | **10px** | **行 10**（R2） |
| B6 | 状态直方图完整性 | 未知 `status` 的任务**在四列全部消失** | 未知状态归 `pending` + 列头提示；**Σ 列计数 = `tasks.length`** | 本页特有（正确性） |
| B7 | 轮询重排 | 无排序 → 卡片位置会跳动 | 列内按 `updated_at` 倒序稳定 | 本页特有 |
| B8 | 每列卡数 / 页高 | 21 / 2269px | 单列 ≤20 / 页高 ≤4000px | 本页特有 |
| B9 | 内联 style 节点数 | 38 | ≤50、内联 `font-size` = 0 | 行 11、12 |
| B10 | `in_progress` 染信号色的条件 | `col === "in_progress" && n > 0` | **保持**（W5 的正面样本） | 行 3 + §3.2 W5 |
| B11 | 错误态可区分 | `catch` 兜成 `[]` → 显示"没有任务" | `role=alert` + 重试 + 保留旧数据 | **行 20** |

### 看板「运行」开关：窄屏判据（2026-09-23 裁决，替代 t67 的意图）

**对象**：**每张卡一个**「运行」开关（`RunPeek` 渲染在每张卡片下面，`Board.tsx:203`）—— **不是板级控件** ✓
**判据（≤768，列已堆叠）**：**不得读成走失的标签页** ⇒ **按钮宽度 = 内容宽** · **有图标** · **不整列宽** ✓（实测：390 **294×32 → 83×32 + 图标** ✓；1440 **269×32 不变** ✓）
**必须常驻**（每张卡都渲染 ✓）。**理由（三条，均为反证而非推测）**：① **N=0 在渲染时无从得知**（`Task` 无 run 字段 · daemon 无跨任务 runs 端点 · 每卡拉一次 = 该页注释明令禁止的 N+1；**实测反证**：点开才知道是「运行（0）」）② **用 `task.status` 做代理会正好藏掉重试路径**（**那张有失败 run 的卡在「已完成」列** ✗）③ ⇒ **取消 t67 的意图**（「N=0 不渲染」作废）✓
**断点必须同源**：实现用 `matchMedia("(max-width: 768px)")`，与 `index.css:721` 的列堆叠断点一致 ✓ ⇒ **凡「在某宽度下改变形态」的判据，其断点必须与产生该布局的断点同源**（否则会出现一个宽度区间两处不一致 ✓）
**不做（YAGNI，已记录）**：**(b)** 把 `.kanban-card` 从 `<button>` 改成容器（结构重构 + 动 `index.css`）—— **目标已达成 ⇒ 不做** ✓；**(c)** daemon 跨任务 runs 端点 —— **同上不做** ✓ ⇒ **不要为了一条已经解决的视觉问题去加端点** ✓
