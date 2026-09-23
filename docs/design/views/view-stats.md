# stats — `#stats`

> **路由**：`#stats`
> **视图**：`panel/src/views/Agents.tsx` → `Stats()`（与 `Agents`/`Inbox` 同文件）
> **消费的 MASTER 行**：§12 行 **6、7、12、19**（本页特有；**19 是本页唯一持有者**）+ README §2 全局
> **消费的变更编号**：`C2 C5`（**核心：成本列的 Progress 用琥珀 = 黑名单 V3**）`C6` · `T5 T6` · `S1` · `R1` · `A2` · `Y1` · `Z1`
> **冻结选择器**：`.view-bar h2`、`.ant-table`（e2e 断言表格存在）、`.row-btn`

## 1 职责与首要任务

**职责**：两张表——**每个角色的运行统计**（运行/成功/失败/成本/最近）与**最近召回日志**（M6 调参数据集）。
**首要任务**：**看谁在烧钱、谁在失败**。这是全站唯一的"事后账本"视图。

据此决定：
- 顶部 `ReadoutStrip` 只放 4 个全局合计（运行 / 成功 / 失败 / 成本）——它们与下方表格的**列一一对应**（前 4 列求和 = 顶部 4 格），读者可以立刻校验。
- 表格用 antd `Table`（**唯一使用 `.ant-table` 的路由**，e2e 依赖），`size="small"`、`pagination={false}`、`rowKey="agent"`：角色数量天然少（实测 7），分页无意义。
- 成本列用 `Progress` 做**相对条形**（`maxCost = max(total_cost_usd)`）：金额本身不可比，条形让"谁最贵"一眼可见。
- 召回日志是**只读审计数据**：`.row-btn` 只借用行样式，`cursor: default`，**不允许点击**——规格要求改用非交互行（`.row` 或 `role="row"`），否则会产生 20 个"看起来能点但点不动"的目标。

## 2 版式骨架

```
1440：侧栏 228 │ content 1212（可用 1149）
┌ .view-bar ─────────────────────────────────────────────────────────────┐
│ h2 统计  .muted 副题                                                  │
├ .readout-strip panel 1149×91 ──────────────────────────────────────────┤
│   22运行   │   21成功   │   1失败   │   $27.17成本                      │
├ .card（antd Card）─────────────────────────────────────────────────────┤
│ .ant-table（size small, pagination false, rowKey agent）                │
│  智能体 │ 运行(右) │ 成功(右) │ 失败(右) │      成本(右)      │ 最近运行  │
│  ───────┼─────────┼─────────┼─────────┼────────────────────┼──────────│
│  <name> │   12    │    11   │     1   │ ▓▓▓▓▓░░░░░░ $12.34 │ 2 天前   │
│                                            ↑ Progress 72×5            │
│   Σ = 顶部 4 格（可校验）                                               │
├ .dash-head「最近召回」（仅当 recallLog 非空）───────────────────────────┤
│ .card > .row-btn[cursor:default] ×20                                    │
│  .mono(12) <query>  .tag 保守|激进  .muted「mem N · know N · wiki N · ent N」│
│  .grow  .muted.mono(11)「m X.XX」 .muted.mono(11)「k X.XX」  .time      │
└────────────────────────────────────────────────────────────────────────┘
390：⚠ .ant-table 6 列 table-layout:auto 撑破视口 → scrollWidth 498 vs 390 = +99px（唯一溢出路由）
     修复 = <Table scroll={{ x: 560 }} /> 或 1.0k 以下换卡片
768：表格 662 内勉强放下（列被压窄）；1280/1440：表格 996/1149
```

**实测**：`.card` 1149×792（@1440）、996×792（@1280）、662×792（@768）、294×792（@390）；`.ant-table` 1 个；`.row-btn` 20 个（召回日志）；pageH 1570/1567/1560/**1801**。

## 3 层级

**本页最大元素 = `.readout`（32/40 mono，4 个）**，实测 `maxFs = 32`、≥18px 节点 **5** 个（h2 + 4 读数）。

为什么是 4 格：与表格前 4 列同构（运行/成功/失败/成本），**顶部是"总计"、表内是"分解"**。这是全站唯一一个"读数与表头一一对应"的页面，保持这个同构关系。
次级：`.ant-table` 表体 14/21；``.ant-table-thead th` 用 `colorTextSecondary`（`theme.tsx` 已配 `headerBg: transparent`、`headerSplitColor: borderSecondary`）——**表头不铺底色**（材质阶梯里表格不是面板）。
第三级：召回日志的 `.mono` 查询串（12/16）+ 分数（11/15）。

**禁止**：给表格加斑马纹（`Table` 未开 `stripe` ✔）；给失败数染红（它是**计数**不是错误——只有 `> 0` 时允许 `.tag.err` 出现在数字旁，见 §8）。

## 4 内容模型

| 调用 | 返回类型（`api.ts`） | 用到字段 | 展示位 |
|---|---|---|---|
| `api.stats()`（**5s 轮询**） | `AgentStats[]` | `agent, runs, completed, failed, total_cost_usd, last_run_at` | 4 格读数（各列求和）+ 表格 6 列 |
| `api.recallLog(20)` | `RecallLogRow[]` | `ts, query, strategy, top_n, memories, knowledge, wiki, entities, top_memory_score, top_knowledge_score` | 「最近召回」段 |

**字段 → 展示位**

| 展示位 | 字段 | 规则 |
|---|---|---|
| 读数① | `Σ runs` | 整数 |
| 读数② | `Σ completed` | 整数 |
| 读数③ | `Σ failed` | 整数；**`> 0` 时允许 `.readout.signal`？→ 不允许**。失败不是"现在"（P2 白名单 W1–W5 无此语义）。若要强调，用 `.tag.err` 在表内行 |
| 读数④ | `fmtUsd(Σ total_cost_usd)` | `fmtUsd` 保两位 |
| 表列 1 | `agent` | `rowKey` |
| 表列 2–4 | `runs` / `completed` / `failed` | `align: "right"` + `tabular-nums`（`.ant-table` 已 inherit） |
| 表列 5 | `total_cost_usd` | `Tooltip(fmtUsd)` + `Progress`（`percent = v / maxCost * 100`，`showInfo={false}`，72×5）+ `.mono` 金额 |
| 表列 6 | `last_run_at` | `RelTime`（ISO 字符串） |
| 召回行 | `query` / `strategy` / 4 个计数 / 2 个 top score / `ts` | `strategy`：`"conservative"`→「保守」，否则「激进」 |

**分组**：两段（角色统计表 / 最近召回）。`top_n` **未渲染** → 规格要求加入召回行（"只取前 N"是调参的关键参数，缺失后 `top_memory_score` 无法解释）。
**密度上限**：

| 项 | 实测 | 上限 | 手段 |
|---|---|---|---|
| 表格行 | 7 | **≤60** | 超出 `pagination`（现为 false，需在 >60 时启用） |
| 召回行 | 20 | **≤50** | `recallLog(limit)` |
| 页高 | 1570px | ≤3000px | — |
| 节点数 | 400 | ≤1500 | — |
| 内联 style | **213**（全站第二高，其中 80 个内联 `font-size`） | **≤50**；内联 `font-size` = 0 | 行 11、12（`.mono`/`.micro` 类） |

## 5 四种状态

| 态 | 契约 | 现状 |
|---|---|---|
| **loading** | `stats === null` → `Spinner`；`recallLog === null` → 不渲染该段（不是空盒子） | ✅ |
| **empty** | ①无统计行 → `Empty` + 「还没有运行记录」②`recallLog.length === 0` → 整段隐藏（现状 ✔，因为它只是数据集） | ①待补 ②✅ |
| **error** | `api.stats()` 的 `catch(() => setStats([]))` **把失败变成空表**；5s 轮询里还有一处 `catch(() => {})`（`Agents.tsx:437`）。规格要求：`role=alert` + 重试 + 保留上一次数据（**轮询失败绝不能让 27.17 美元变成 0**） | **违反** |
| **密集** | 见 §4。额外：①`Progress` 的 `maxCost` 在轮询后变化会导致所有条**同时伸缩**（视觉抖动）→ 规格要求 `maxCost = max(当前值, 上一次值)` 单调不减，或只在数据变化 >5% 时更新 ②召回行的 `query` 长串必须 `truncated`（`.mono` 12px + `max-width`） | 待实现 |

## 6 响应式行为

| 视口 | 侧栏 | content | 表格 | 行为 |
|---|---|---|---|---|
| **390** | 72 | 318（294） | 6 列 `table-layout: auto` → **scrollWidth 498，溢出 +99px** | **必须修**：`<Table scroll={{ x: 560 }} />`（表格内部横向滚动，页面不溢出）或 ≤768 换卡片列表。pageH 1801 |
| **768** | 72 | 696（662） | 6 列压到 662 | 溢出 0（实测），但列宽已临界；规格要求同样加 `scroll={{x:560}}` 保底 |
| **1280** | 228 | 1052（996） | 6 列 996 | 无溢出 |
| **1440** | 228 | 1212（1149） | 6 列 1149 | 无溢出 |

横向溢出：**390 视口 = +99px（全站唯一违反行 19 的路由）**；1280/1440/768 = 0。

## 7 复用的共享原语与类名

`.view-bar` `.readout-strip` `.readout` `.readout-label` `.card` `.dash-head` `.row-btn` `.row` `.tag` `.muted` `.mono` `.time` `.grow` `.pad` `.truncated`
表格：`.ant-table`（**e2e 冻结**）`.ant-table-thead th`（`headerBg: transparent`）
组件：`ReadoutStrip` `Spinner` `Empty` `RelTime` `useToast` `Table` `Progress` `Tooltip`（antd）`fmtUsd`（`ui.tsx`）
**本页请求的 zone 化（Z1）**：`.dash-head`「最近召回」→ `.zone-head` + `.zone-title` + `.zone-note`（20 条）。
新增类请求：`.sr-only`（隐藏 `h1`「统计」）、`.micro`（**11/15**，与 `.muted` 叠用）、`.error-state` + `.spinner-block`（经 `ui.tsx` 的三态契约）。**本页不新增数据文本类** —— primitives §7 R3 已拒绝该请求（它与 `.mono` 逐字相同），数据文本用既有 `.mono`。

## 8 本页特有的验收数字

| ID | 指标 | 基线（实测） | 目标 | 判定（MASTER 行） |
|---|---|---|---|---|
| S1 | **横向溢出 @390** | **+99px**（`.ant-table` 6 列） | **0** | **行 19**（本页是全站唯一违反者） |
| S2 | ≥18px 文本节点 / 最大字号 | **5 / 32px** | ≥3 / ≥32px | 行 6、7 |
| S3 | **成本列 Progress 的颜色** | 6×5 的 `div` 背景 `rgb(207,147,53)` = antd 派生 primary = **信号色** | **改中性**（`--ant-color-text-secondary` 或 `--graph-product` 一族的蓝），或改用一条 `.muted` 规则线 | **行 3 + §3.2 黑名单 V3** |
| S4 | 内联 style 节点数 | **213**（全站第二）；内联 `font-size` **80** | ≤50 / 0 | **行 11、12** |
| S5 | 单亮度带 / 色度 / 信号占比 | **17.8% / 2.18% / 0.36%** | ≤75% / 1.0–6.0% / ≤1.5% | 行 1、2、3 |
| S6 | 可见描边元素数 | **70**（表格单元格线） | **≤40（排除 `.ant-table td/th`）** | 行 5（判定需带表格排除规则） |
| S7 | 描边内容容器 | **0** | ≤1 | 行 4 |
| S8 | 读数与表列同构 | 读数①②③④ = 表列 2/3/4/5 之和 | **恒等**（可机器校验） | 本页特有（正确性） |
| S9 | 召回行可交互性 | 20 个 `.row-btn` + `cursor:default`（**看起来能点但点不动**） | 改用非交互行；若保留 `.row-btn` 则必须 `tabIndex={-1}` 且无 hover 变化 | 行 17、18 同源 |
| S10 | `top_n` 可见性 | **未渲染** | 显示（调参必需） | 本页特有（正确性） |
| S11 | `Progress` 条的对比度 | 琥珀对底 8.3:1（远超 3:1） | ≥3:1（保持），但**颜色语义必须换** | 行 33 同源 |
| S12 | 轮询失败的数据保留 | `catch` 兜成 `[]` → 成本显示 $0.00 | 保留旧值 + 「数据可能过期」 | 行 20 同源 |
