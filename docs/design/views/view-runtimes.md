# runtimes — `#runtimes`

> **路由**：`#runtimes`
> **视图**：`panel/src/views/Runtimes.tsx` → `Runtimes()`；内部 `RuntimeEditor` 弹窗 + `SyncModelsButton`（用 `api.agentOptions(name, true)` 探测模型）
> **消费的 MASTER 行**：§12 行 **6、7、18**（本页特有）+ README §2 全局
> **消费的变更编号**：`C2 C5` · `T2 T3 T5 T6` · `S1` · `R1` · `A1 A2 A3 A4` · `Y1 Y3` · `Z1 Z2`
> **冻结选择器**：`.view-bar h2`、`.agent-card`、`.ant-empty`（`registry.spec.ts` 走 `#runtimes`）

## 1 职责与首要任务

**职责**：管理**运行时**（runtime：裸 harness 实例 / `[runtime.*]` 卡，如 claude-code、dsh、opencode）——新增、编辑、删除、探测模型、直连对话。
**首要任务**：**确认某个运行时可不可用（命令在不在、模型有几款、被哪些角色用着）**，以及**立刻和它开一场对话**。

据此决定：
- 每张卡必须显示**三件事**：`command`（可复制）、`models` 数量、**被哪些角色引用（usedBy）**——删除一个运行时前必须能看出会破坏谁。
- 「直连对话」是卡上的**主行动**：运行时是"能和它说话"的东西，配置是次要的。
- 运行时的身份用 `.runtime-logo`（38×38，品牌色派生底 + 品牌标）表达，**不是**头像字母：这里的"身份"是厂商而不是个体。
- 与 `#agents` 的分工是硬边界：**role 归 `#agents`，runtime 归 `#runtimes`**（`isRoleAgent()` 判定），两页不得互相显示对方的对象。

## 2 版式骨架

```
1440：侧栏 228 │ content 1212（可用 1149）
┌ .view-bar ─────────────────────────────────────────────────────────────┐
│ h2 运行时  .muted 副题  .grow  [+ 新建运行时](primary)                    │
├ .readout-strip panel 1149×91 ──────────────────────────────────────────┤
│      3运行时     │     7智能体      │      31模型                        │
├ .agent-grid repeat(auto-fill, minmax(min(310px,100%),1fr)) gap 14 ────┤
│ .agent-card ×3（实测 374×229）                                            │
│ ┌ .ant-card-body ────────────────────────────────────────────────────┐ │
│ │ .row  [.runtime-logo 38×38 r10 + .brand-claude]  <harness>  .grow   │ │
│ │                                              .tag.ok 启用           │ │
│ │ p.muted「<description>」                                            │ │
│ │ .row  .doc-icon  .muted.mono.truncated <command>                    │ │
│ │ .row（models）  .tag「N 模型」 [同步]    │                           │ │
│ │ .row（usedBy）  .tag ×N <角色名>  或 .muted「未被引用」              │ │
│ │ .row.end  [直连对话] [编辑] [删除(popconfirm)]                       │ │
│ └────────────────────────────────────────────────────────────────────┘ │
├ p.muted.pad「runtimes.hint」（底部说明）────────────────────────────────┤
└────────────────────────────────────────────────────────────────────────┘
390：单列 294；pageH 1273        768：2 列 324；pageH 900        1280/1440：3 列
```

**实测**：`.agent-grid` 1149×229 / `grid[374,374,374]`（@1440）；996×229 / `[323,323,323]`（@1280）；662×472 / `[324,324]`（@768）；294×715 / `[294]`（@390）。
`.runtime-logo` 38×38 r10；底色 `color-mix(in srgb, currentColor 10%, transparent)` + 1px `22%` 描边。

## 3 层级

**本页最大元素 = `.readout`（32/40 mono，3 个）**，实测 `maxFs = 32`、≥18px 节点 **4** 个（h2 + 3 读数）。

为什么是 3 格：运行时（3）/ 智能体（7）/ 模型（31）——**三者的关系是这一页的全部信息**（3 个后端支撑 7 个角色、31 个模型可选）。数字的**比例**比数字本身重要。
次级：`.runtime-logo` 是 38×38 的**视觉锚点**（比任何文字都大），这是唯一允许"图形大于文字"的场景——因为它承载品牌识别。
第三级：卡内 `.tag`（11px）、`.muted.mono.truncated`（12px）。

**禁止**：给 `.runtime-logo` 加信号色；把 `command` 放大（它是数据，用 `.mono.truncated` 12px 即可，完整值靠 `title`）。

## 4 内容模型

| 调用 | 返回类型（`api.ts`） | 用到字段 | 展示位 |
|---|---|---|---|
| `api.agents()`（5s 轮询） | `AgentInfo[]` | 全字段 | **本页只显示 `isRoleAgent(a) === false` 的**；`harness, command, models?, description, enabled` |
| `api.agentOptions(name, refresh=true)` | `AgentOptions { agent, options, cached?, updated_at? }` | `options[].id/category === "model"` 的 `choices` 数 | 「同步」按钮 → 模型数；`cached === false` 表示刚探测 |
| `api.createRuntime({name, harness?, command?, description?, mcp_profile?, models?})` | `AgentInfo` | — | 新建 |
| `api.updateRuntime(name, body)` | — | `body: Partial<AgentInfo>` | 编辑 |
| `api.deleteRuntime(name)` | — | — | 删除（`Popconfirm` ✅ 现状已有） |

**usedBy 的推导（无专门端点）**：`agents.filter(a => isRoleAgent(a) && (a.runtime === r.name || a.runtimes?.includes(r.name)))` —— 这是**派生数据**，必须在规格里写明推导规则，否则实现者会去找一个不存在的 API。

**字段 → 展示位**

| 展示位 | 字段 | 规则 |
|---|---|---|
| `.runtime-logo` 内容 | `brandClass(harness)` | `claude`/`deepseek`/`opencode` 走 `.brand-*`（官方色）；未知 harness → 中性 `--ant-color-text-secondary` |
| 标题行 | `harness` | 主标题是 **harness**，`name` 只在两者不同时以 `.tag` 补充 |
| `.tag.ok` | `enabled` | 恒为启用（运行时无禁用态）——若后端给出 `false`，规格要求显示 `.tag`「已禁用」而非谎报启用 |
| `.muted.mono.truncated` | `command` | `title` 放全量；`max-width: 340px` |
| `.tag`「N 模型」 | `models?.length` | 无 `models` → 显示「未探测」+ 「同步」按钮 |
| usedBy 的 `.tag` | 派生（见上） | 空 → `.muted`「未被任何角色引用」（**删除前的关键信息**） |
| 「直连对话」 | — | `onNav(\`chat?agent=${r.name}\`)`（`#chat?agent=` 只首次挂载生效，见 view-chat.md §1） |

**分组**：无分组（运行时数量天然少，实测 3）。
**密度上限**：

| 项 | 实测 | 上限 | 手段 |
|---|---|---|---|
| 运行时卡 | 3 | **≤30** | 超出按 `harness` 分组（claude-code / dsh / opencode / 其他） |
| 卡高 | 229px | ≤300px | `description` 2 行截断 |
| usedBy tags | ≤3 | **≤5 + "+$N"** | 防止一个被 20 个角色引用的运行时撑破卡片 |
| models | 31（总） | 每卡 ≤∞（只显示数字） | 只显示计数，不列模型名 |
| page height | 900px | ≤2500px | — |
| 节点数 | 237 | ≤1200 | — |

## 5 四种状态

| 态 | 契约 | 现状 |
|---|---|---|
| **loading** | `agents === null` → `Spinner`；「同步」按钮 `loading`（`syncing === name`） | ✅ |
| **empty** | ①无运行时 → `Empty` + 「还没有运行时」+ 「+ 新建运行时」②无 `models` → 「未探测」+ 同步按钮（**不是 0**）③usedBy 为空 → 「未被任何角色引用」 | ①待补 ②③✅ |
| **error** | ①`api.agentOptions(name, true)` 的 `.catch(() => {})`（`Runtimes.tsx:36`）**静默** → 同步失败看起来像"没反应"；规格要求按钮旁 `role=alert` 一行「探测失败 · 重试」②`deleteRuntime` 失败只有 toast；③删除被引用的运行时时**必须**在 `Popconfirm` 里列出受影响的角色名 | **违反** |
| **密集** | 见 §4 上限。额外：5s 轮询后卡片顺序稳定（按 `harness` 再按 `name` 排序） | 待实现 |

## 6 响应式行为

| 视口 | 侧栏 | content | 列数 | 卡宽 | pageH |
|---|---|---|---|---|---|
| **390** | 72 | 318（294） | **1** | 294 | 1273 |
| **768** | 72 | 696（662） | **2** | 324 | 900 |
| **1280** | 228 | 1052（996） | **3** | 323 | 900 |
| **1440** | 228 | 1212（1149） | **3** | 374 | 900 |

横向溢出：**0**（四视口实测全 0）。`.runtime-logo` 是固定 38px，不参与收缩；`.muted.mono.truncated` 的 `max-width` 在 294px 内会先截断。

## 7 复用的共享原语与类名

`.view-bar` `.readout-strip` `.readout` `.readout-label` `.agent-grid` `.agent-card` `.runtime-logo`；品牌色**类保留**（`.brand-claude` / `.brand-deepseek` / `.brand-opencode` = color: var(--brand-*)），但**标记取色必须写变量形式** var(--brand-claude)（primitives §7 R6） `.row` `.row.end` `.tag` `.tag.ok` `.muted` `.mono` `.truncated` `.grow` `.pad` `.doc-icon` `.field`
浮层：`.modal`（via `Modal`）
组件：`Spinner` `Empty` `Modal` `BrandMark`（`brand.tsx`：`brandClass()`）`useToast` `Input` `Select` `Button` `Popconfirm` `Icon`
**本页请求的 zone 化（Z1）**：底部的 `p.muted.pad`（`runtimes.hint`）与卡内「模型」「被引用」两组是 zone 语义 → `.zone-head` + `.zone-title` + `.zone-note`。
新增类请求：`.sr-only`（隐藏 `h1`「运行时」）、`.micro`（**11/15**，与 `.muted` 叠用）、`.error-state` + `.spinner-block`（经 `ui.tsx` 的三态契约）。**本页不新增数据文本类** —— primitives §7 R3 已拒绝该请求（它与 `.mono` 逐字相同），数据文本用既有 `.mono`。

## 8 本页特有的验收数字

| ID | 指标 | 基线（实测） | 目标 | 判定（MASTER 行） |
|---|---|---|---|---|
| R1 | ≥18px 文本节点 / 最大字号 | **4 / 32px** | ≥3 / ≥32px | 行 6、7 |
| R2 | 单亮度带 / 色度 / 信号占比 | **55.6% / 2.17% / 0.48%** | ≤75% / 1.0–6.0% / ≤1.5% | 行 1、2、3 |
| R3 | 描边内容容器 | **0** | ≤1 且必须是控件单元 | 行 4 |
| R4 | 可见描边元素数 | **19**（`.runtime-logo` 的 1px 品牌色描边占 3 个） | ≤40 | 行 5 |
| R5 | <32px 命中目标 | **16** | <24px = 0；<32px ≤10 | **行 18**（超标 6） |
| R6 | 无名称可交互元素 | **0** | **0** | 行 17 ✅ |
| R7 | 内联 style 节点数 | **30**（其中 3 个内联 `font-size:12`） | ≤50；内联 `font-size` = 0 | 行 11、12 |
| R8 | `.runtime-logo` 的**标记**与**描边**对比度 | 标记对**卡片** 亮 3.12 / 暗 5.35 ✔；**但对标记自身底色（10% 品牌淡染）亮 claude 只有 2.81**（deepseek 3.03 贴线）；描边（97%）亮 3.010 / 暗 5.108 ✔ | **基准分两类（MASTER §12.11.2）**：① **描边（边界类）→ 对卡片底 ≥3:1**（现状达标，不变）；② **标记（内嵌图形）→ 对其自身底色 ≥3:1** ⇒ **tile 底色 alpha ≤2%**（临界 **3.966%**；实测 2% 时 亮 claude **3.060** / deepseek 3.293 / opencode 17.988，暗 5.220 / 4.848 / 13.017）；**0%（删掉底色）同样合规** | **行 33 的同源要求**（域外品牌色不参与分类色阈值，但标记是「识别运行时所必需」的图形）。**落点与归属见 MASTER §12.11.2**：底色 alpha 在共享层（`index.css` 的 `.runtime-logo`）；`mono` 与品牌类在视图侧（t42 已修） |
| R9 | `usedBy` 推导正确性 | 派生自 `agents`（无端点） | **删除前必须列出受影响角色** | 本页特有（正确性） |
| R10 | 与 `#agents` 的对象边界 | `isRoleAgent()` 判定 | **两页显示集合无交集**（Σ = `agents.length`） | 本页特有（正确性） |
| R11 | 模型探测失败可见 | `catch(() => {})` 静默 | `role=alert` + 重试 | 行 20 同源 |
