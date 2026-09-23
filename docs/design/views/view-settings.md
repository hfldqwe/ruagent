# settings — `#settings`

> **路由**：`#settings`
> **视图**：`panel/src/views/Settings.tsx` → `Settings()`
> **消费的 MASTER 行**：§12 行 **6、7、16、17**（本页特有）+ README §2 全局
> **消费的变更编号**：`C2 C9` · `T2 T5 T6` · `S1` · `R1` · `A1 A3 A4` · `Y1 Y4` · `Z1 Z2`
> **冻结选择器**：`.view-bar h2`、`.ant-empty`（`i18n.spec.ts` 走 `#settings` 的开关文案）

## 1 职责与首要任务

**职责**：配置**蒸馏策略**（自动开关、图谱开关、蒸馏 Agent、语言、自定义提示词）。
**首要任务**：**改一个开关或换一个蒸馏 Agent，然后保存**。这一页的全部内容就是"当前配置 + 保存"。

据此决定：
- **单卡单标题**（`.card.distill-settings`，max-width 760px）：设置页不该有卡片网格，一列窄栏读起来最省心。
- **当前值必须可见**：所有控件都是"已经填好当前值"的形式（Select 的 value、Input 的 value、Switch 的 checked），不引入"读一次才能编辑"的两段式。
- 自定义提示词与内置提示词的关系要**说明白**：`.field-hint` + `.tag`（"自定义" / "内置"）——因为 `builtin_prompt` 与 `prompt` 是两个字段，用户必须知道当前生效的是哪个。
- **保存是显式的**（`.row.end` 的按钮），不自动提交：改 5 个字段触发 5 次 PUT 会让"哪一次生效"不可知。

## 2 版式骨架

```
1440：侧栏 228 │ content 1212
┌ .view-bar ─────────────────────────────────────────────────────────────┐
│ h2 设置  .muted「蒸馏策略、运行偏好」                                    │
├ .card.distill-settings  760×592（max-width:760，左对齐）───────────────┤
│  h3.sec 蒸馏                                                          │
│  .row[justify-content:space-between]「自动蒸馏」+ p.muted 说明  [Switch] │
│  .row[justify-content:space-between]「图谱提取」+ p.muted 说明  [Switch] │
│  .row.wrap                                                            │
│     .chat-field[flex:1 1 220px]  «蒸馏 Agent»   [Select]              │
│     .chat-field[flex:1 1 220px]  «语言»          [Input 例如：简体中文] │
│  .chat-field  «自定义提示词»  .tag 自定义|内置                          │
│     [Input.TextArea]                                                  │
│     .field-hint「<distill.promptHint>」                                │
│  .row.end   [保存]                                                     │
└───────────────────────────────────────────────────────────────────────┘
390：content 318（294）｜.card/.distill-settings 294×712；.row.wrap 的两个 field 换行；
     pageH 863（一屏内）
768：662×592        1280：760×592（不随内容变宽）
```

**实测**：`.distill-settings` 与 `.card` 同尺寸 760×592（@1440/1280）、662×592（@768）、294×712（@390）。

## 3 层级

**本页最大元素 = `.view-bar h2`（title 20/26 w600）**，实测 `maxFs = 20`、**≥18px 节点只有 1 个** —— 这是本页的已知缺口（设计语言文档记录过"`#chat` 和 `#settings` 都不想要仪表行"）。

**本规格的解法（真实数据，非装饰）**：在 `h3.sec` 之上加一条 **3 格 `.readout.s`（18/24 mono）** 的「当前策略」行：

| 读数 | 字段 | 值示例 |
|---|---|---|
| 蒸馏 | `DistillPolicy.auto` | 「自动」/「手动」 |
| 图谱提取 | `DistillPolicy.graph` | 「开」/「关」/「跟随」（`null`） |
| 默认 Agent | `DistillPolicy.agent` | agent 名或「未设置」 |

**为什么这不是凑数**：设置页的第一问永远是"我现在配的是什么"。三个值散在页面里需要逐行读；一条 18/24 的读数行让用户在改动前先确认现状。这也让 ≥18px 节点从 1 升到 4，满足行 6，同时不违反"不设 32px 仪表"的设计意图——18 是**次级读数档**（`.readout.s`），不是仪表。

次级：`h3.sec` 15/22 w600（section）；`.field > span` 13/18；`.field-hint` **12/17 → 12/16**（primitives §3.1）；`p.muted` 13/18。

**禁止**：把 `.distill-settings` 加宽过 760px（长行文本的可读性上限）；给开关加颜色（`auto` 打开 ≠ "现在"）。

## 4 内容模型

| 调用 | 返回类型（`api.ts`） | 用到字段 | 展示位 |
|---|---|---|---|
| `api.distillPolicy()` | `DistillPolicy`：`auto: boolean`、`graph?: boolean \| null`、`agent: string \| null`、`language: string \| null`、`prompt: string \| null`、`builtin_prompt: string` | 全字段 | 两个 Switch、两个 field、TextArea、3 格读数 |
| `api.agents()` | `AgentInfo[]` | `name` | 「蒸馏 Agent」Select 的选项 |
| `api.setDistillPolicy({auto, graph?, agent, language, prompt})` | — | — | 「保存」；成功后常驻一行「已保存 hh:mm:ss」（不是 2s toast） |

**字段 → 展示位**

| 展示位 | 字段 | 规则 |
|---|---|---|
| Switch① | `auto` | `checked={auto}` |
| Switch② | `graph` | 三态：`true`→开、`false`→关、`null/undefined`→**必须显示「跟随默认」**（不得静默当成关） |
| Select | `agent` | 选项来自 `api.agents()`；`agent === null` → placeholder「未设置」 |
| Input | `language` | placeholder「例如：简体中文」；`null` → 空 |
| TextArea | `prompt` | `null` → 空 + `.tag`「内置」（生效的是 `builtin_prompt`） |
| `.tag` | `prompt != null` | 「自定义」/「内置」 |
| `.field-hint` | `distill.promptHint` | 静态文案 |
| （折叠） | `builtin_prompt` | **当前完全不可见** → 规格要求放进 `details.prompt-view`（只读），否则用户无法知道内置提示词长什么样 |

**分组**：单卡单段（蒸馏）。`.view-bar` 副题写"蒸馏策略、运行偏好"，但**本页只有蒸馏一张卡**（审计 §八已记录 IA 不一致）→ 规格两条处置：①把副题改成「蒸馏策略」，或 ②补"运行偏好"段。**默认选 ①**（改文案比造功能便宜，且不虚构不存在的配置）。
**密度上限**：

| 项 | 实测 | 上限 | 手段 |
|---|---|---|---|
| 表单字段 | 5 | ≤12 | 超出分两卡 |
| 卡宽 | 760px | **≤760px（固定）** | `max-width` |
| 提示词 TextArea | — | `max-height: 320px` + 滚动 | 防 10k 字符撑破页面 |
| 页高 | 900px | ≤1600px | — |
| 节点数 | 155 | ≤600 | — |
| 内联 style | 26 | ≤50；内联 `font-size` = 0 | 行 11、12 |

## 5 四种状态

| 态 | 契约 | 现状 |
|---|---|---|
| **loading** | `policy === null` → `Spinner`（`Settings.tsx:46`） | 可用，但**没有把 error 与"还在加载"分开**（见下） |
| **empty** | 设置页无"空态"概念；但 `api.agents()` 返回 0 个角色时，Select 必须显示「没有可选角色」而不是空下拉 | 待实现 |
| **error** | `api.distillPolicy().catch(() => setPolicy(null))` → **永久 Spinner**（与 task-detail 同一种缺陷：把失败渲染成"还在加载"）。规格要求：①`role=alert` + 「重试」+ 保留表单 ②`setDistillPolicy` 失败：**表单值保留**、错误常驻在「保存」上方、按钮退出 busy | **违反** |
| **密集** | 不适用（表单最多 12 字段）。唯一风险：`builtin_prompt` 展开后很长 → 用 `details.prompt-view > pre`（`max-height: 300px`，现状已有该规则） | ✅ |

## 6 响应式行为

| 视口 | 侧栏 | content | 卡宽 | 行为 |
|---|---|---|---|---|
| **390** | 72 | 318（294） | 294×712 | `.row.wrap` 的两个 `.chat-field`（`flex: 1 1 220px`）各占一行；pageH 863（**一屏内**） |
| **768** | 72 | 696（662） | 662×592 | 两个 field 并排（220+220+gap < 662） |
| **1280** | 228 | 1052（996） | 760×592 | 不随可用宽变宽（max-width 760） |
| **1440** | 228 | 1212（1149） | 760×592 | 同上 |

横向溢出：**0**（四视口实测全 0）。

## 7 复用的共享原语与类名

`.view-bar` `.card` `.distill-settings` `h3.sec` `.row` `.row.wrap` `.row.end` `.chat-field` `.field` `.field-hint` `.tag` `.muted` `.prompt-view` `.readout.s`（新增使用，MASTER §4.1 次级读数档）`.readout-label`
组件：`Spinner` `Modal`（无 `Empty` —— 设置页无空态）`useToast` `Switch` `Select` `Input` `Input.TextArea` `Button`（antd）
**本页请求的 zone 化（Z1）**：`h3.sec`「蒸馏」→ `.zone-head` + `.zone-title`（若补"运行偏好"段则两段各一个 zone）。
新增类请求：`.sr-only`（隐藏 `h1`「设置」）、`.micro`（**11/15**，与 `.muted` 叠用）、`.error-state` + `.spinner-block`（经 `ui.tsx` 的三态契约）。**本页不新增数据文本类** —— primitives §7 R3 已拒绝该请求（它与 `.mono` 逐字相同），数据文本用既有 `.mono`。

## 8 本页特有的验收数字

| ID | 指标 | 基线（实测） | 目标 | 判定（MASTER 行） |
|---|---|---|---|---|
| G1 | ≥18px 可见文本节点 | **1**（只有 `h2`） | **≥3**（§3 的 3 格 18/24 读数 + h2 = 4） | **行 6**（§12.1 豁免：门槛 ≥1）。本页按 §3 的 3 格 18/24 读数做到 **4** —— 升的是已存在的真实字段（`auto` / `graph` / `agent`），不是装饰 |
| G2 | 最大字号 | **20px** | ≥20px（无仪表盘页） | 行 7 ✅ |
| G3 | **无名称可交互元素** | **4**（两个 Switch + 两个无标签控件；实测 `#settings` 是该指标全站最差页） | **0**（Switch 加 `aria-label`；`.chat-field` 的 `<label>` 必须真正绑定） | **行 17** |
| G4 | 无焦点环的 Tab 停留点 | **2/20**（`.ant-select-input`） | **0** | **行 16**（A1/A3） |
| G5 | 描边内容容器 | **0** | ≤1 | 行 4 |
| G6 | 可见描边元素数 | **7** | ≤40 | 行 5 |
| G7 | 单亮度带 / 色度 / 信号占比 | **49.2% / 2.66% / 0.67%** | ≤75% / 1.0–6.0% / ≤1.5% | 行 1、2、3 |
| G8 | 内联 style 节点数 | **26** | ≤50；内联 `font-size` = 0 | 行 11、12 |
| G9 | 卡宽 | **760px 固定** | **保持**（不随视口变宽） | 本页特有 |
| G10 | 副题与内容一致 | 副题"蒸馏策略、运行偏好"但只有蒸馏一段 | **改为「蒸馏策略」**（或补段落） | 审计 §八（IA） |
| G11 | `graph` 三态可见 | `null` 会被当"关"渲染 | **「跟随默认」必须可见** | 本页特有（正确性） |
| G12 | `builtin_prompt` 可见性 | **完全不可见** | 放 `details.prompt-view` 只读展示 | 本页特有（正确性） |
| G13 | 保存反馈 | 2–4s toast | 常驻「已保存 hh:mm:ss」+ 失败保留表单值 | 行 20 同源 |
