# chat — `#chat`

> **路由**：`#chat`，可深链 `#chat?agent=<AgentInfo.id>`（**仅首次挂载生效**，`Chat.tsx:176` 用 `useState` 初值——已在 `#chat` 时再用命令面板切智能体不会切换，属已知缺陷，见 §5）
> **视图**：`panel/src/views/Chat.tsx` → `Chat({ initialAgent })`
> **消费的 MASTER 行**：§12 行 **6、7、16、38**（本页特有）+ README §2 全局
> **消费的变更编号**：`C2 C5 C9` · `T2 T3 T5 T6` · `R1 R3` · `S1` · `A1 A2 A3` · `Y1 Y2 Y3` · `B1` · `Z1`（见各节）
> **冻结选择器**：`.composer`、`.composer-controls .ctl-select`、`.chat-side-toggle`、`.chat-side.open`、`.chat-side-backdrop`、`.view-bar h2`、`.cmdk*`

## 1 职责与首要任务

**职责**：与一个（角色或运行时）智能体做**单会话**对话，并把这条会话挂到某个项目目录上。
**首要任务**：**打字、发出、看到流式回复**。其他一切（历史、工作区、模型、权限模式、推理力度）都是为这三个动作服务的外围。

据此决定：
- 页面**全出血**：`.content:has(> .chat-layout) { padding-inline: 0; max-width: none }`——对话列拿到最大宽度，历史轨贴侧栏边。
- 输入器（`.composer`）是该页**唯一带描边的容器**（MASTER §6.2 的例外，也是阈值行 38 的对象）：它是"控件单元"，不是内容容器。
- 空会话时输入器**上提到剩余空间的中央**（`.chat-empty-zone`），因为"开始输入"就是首要任务，不该要求用户先滚到底部找输入框。
- 历史与工作区在**同一轨**里（`.chat-side`），不各占一栏：会话与项目目录是同一个层级的两级分组，不是两种对象。

## 2 版式骨架

```
1440：侧栏 228 │ .content 全出血 padding-inline 0 ─────────────────────────────────────────────
┌─ .chat-layout  grid[248, 889]  gap 0 24  padding-inline-start 14  end clamp(16,2.6vw,48) ─┐
│ .chat-side 248 (sticky top 12, max-h calc(100vh-96px), border-inline-end 1px rule-soft)  │
│ ┌──────────────┐ ┌─ .chat-wrap 889×760 ──────────────── .view-bar ────────────────────┐  │
│ │.chat-side-head│ │ [◧ 收起轨]  对话            [＋ 新对话] [☰ 历史(≤700)]            │  │
│ │ [+新会话][＋目录]│ ├─────────────────────────────────────────────────────────────┤  │
│ ├──────────────┤ │  messages.length === 0 ?                                     │  │
│ │.chat-ws-title │ │   .chat-empty-zone                                            │  │
│ │ 工作区         │ │     .chat-hero  (BrandMark 24) h3 20/26「<agent>」          │  │
│ ├──────────────┤ │                 p 13/18  description                         │  │
│ │.chat-group    │ │     .composer（居中，唯一描边容器）                          │  │
│ │.chat-group-head│ │     .chat-suggest  .suggest-chip ×3                         │  │
│ │ ▸ <cwd> 12      │ │   : .chat-log.grow                                          │  │
│ │     .ws-count 3 │ │     .chat-injection (details, 已注入上下文 · N 字符)          │  │
│ │     .ws-new-btn │ │     .chat-thought   (details, 思考 · N 字符)                 │  │
│ │ .row-btn        │ │     .chat-msg.user  /  .chat-msg.agent > .md 15/24           │  │
│ │  .chat-session  │ │     .chat-msg.notice (居中, 12/16)                          │  │
│ │  .live-dot      │ │     .chat-typing (三点)                                     │  │
│ │  .title .time   │ ├─────────────────────────────────────────────────────────────┤  │
│ │.chat-group-more │ │ .chat-bottom → .composer（sticky 底部）                      │  │
│ └──────────────┘ │   .composer-row   textarea 15/23  + .send-btn 32×32 圆            │  │
│                   │   .composer-controls  [.ctl-select.ctl-agent│模型│运行时]         │  │
│                   │      │ .ctl-sep │ [权限模式│推理力度] … .grow  .ws-chip(目录)      │  │
│                   └──────────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────────────────────────┘
768：.chat-layout grid[200, 448] gap 0 14        （@media max-width:940）
390：单列 .chat-wrap 288；.chat-side = position:fixed width min(300px,84vw) z1001
     transform translateX(-102%) → .chat-side.open { transform:none } transition 200ms
     .chat-side-backdrop z1000 覆盖全屏（e2e 断言：点开→有 .open，点旁边→无 .open）
     .chat-rail-toggle 隐藏、.chat-side-toggle 显示（@media max-width:700）
```

**实测栏宽**：`.chat-layout` `grid[248, 889]` @1440 · `grid[248, 733]` @1280 · `grid[200, 448]` @768 · `grid[288]` @390。
`.chat-side` 高：`.chat-side` 是 sticky，内容少时自身高度=内容高（实测 181px），**不撑满**——这是刻意的（轨是一栏而不是一块板）。

## 3 层级

**本页最大元素 = `.view-bar h2`（title 20/26 w600）与 `.chat-hero h3`（20/26 w600）**，实测 `maxFs = 20`，**≥18px 节点只有 2 个**。

为什么 chat 不设仪表盘：对话页没有"平台级计数"，硬塞读数会把唯一该大的东西（对话内容）挤小。但现状有两个问题：

1. **头像/标题缺失**：现在 `.view-bar h2` 恒为「对话」，用户看不出**正在哪条会话里**。规格要求把**当前会话标题**（`ChatHistoryEntry.title`，真实数据）以 title 20/26 放进**页首标题区**——即 `.view-bar` **下方的** `.chat-hero.chat-session-head`（`Chat.tsx:1257`），与 20px 的 `h2` 构成层级中心。**2026-09-21 措辞对齐（不是实现偏离）**：原文写「放进 `.view-bar`，与 `h2` 并列」；实现把它放在 `h2` 之下并在代码里写了理由注释。captain 裁决**改规格措辞、不改代码**——因为层级中心仍是 20px 的 `.view-bar h2`、活跃态 `ge18 = 3` 达标、六视口零溢出（实测），而把它塞进 `.view-bar` 要动共享层槽位，且冻结的 `.view-bar h2` 是 e2e 断言对象，风险更大。
2. **活跃态连 2 个 ≥18px 都保不住**：一旦有消息，`.chat-hero` 消失 → 只剩 `h2` 1 个（阈值行 6 原要求 ≥3；**2026-09-21 已条件化：`#chat` 属 MASTER §12.1 豁免，门槛 ≥1**，本页仍按 C2 把活跃态做到 ≥3）。

**本页的分级**（严格按 MASTER §4.1）：
| 级 | 用在哪 |
|---|---|
| title 20/26 w600 | `.view-bar h2`「对话」；**新增**：当前会话标题；空态 `.chat-hero h3`（智能体名） |
| readout.s 18/24 mono | **新增**：当前会话的 `message_count`（"本条 N 轮"，真实数据，属 P3 的"仪表读数"而非装饰） |
| section 15/22 w600 | 无 |
| body 14/21 | 无（对话正文用 15/24，见下） |
| label 13/18 | `.chat-session .title`、`.chat-group-head`、`.chat-ws-title`（**12/17 → 12/16**，primitives §3.1）、`.chat-group-more` |
| micro **11/15**（primitives §7 R2） | `.time`、`.ws-count`、`.chat-ws-title`（12/17 → 12/16）、`.chat-msg.notice`（12/16） |
| **对话正文** | `.chat-msg.agent .md` **15/24**（T5 已把它列为越界档"15/24"，**规格保留 15/24 并请求把它登记为 read-body 档**——因为对话正文需要比 UI 正文(14/21)更松，这是本页唯一合理的额外档位；登记比改窄更诚实） |

**禁止**：在 chat 里引入第三种 ≥18px 的装饰性大字号（例如给"新对话"按钮加大字）。

## 4 内容模型

### 4.1 数据源（字段名 = `api.ts` 真实字段）

| 调用 | 返回类型 | 用到字段 | 展示位 |
|---|---|---|---|
| `api.agents()` | `AgentInfo[]` | `id, name, harness, description, model, runtime, kind, models?` | `.ctl-select.ctl-agent` 选项（`isRoleAgent()` 分组"角色/运行时"）、`.chat-hero` 的 `BrandMark harness` 与 `description`、`.ws-chip` 之外的会话元信息 |
| `api.agentOptions(name, refresh?, runtime?)` | `AgentOptions` | `options: SessionOptionInfo[]` → `id, name, category, choices: OptionChoice[], current` | `.composer-controls` 里 model 之后的所有 picker（顺序由 `categoryRank` 固定：model → mode → thought_level → 其他；`OptionChoice.group` 决定分组） |
| `api.chatsHistory(agent?, 50)`（**agent 可选；侧栏默认不传** —— §9 C1） | `ChatHistoryEntry[]` | `id, agent, runtime, model, title, created_at, updated_at, active, cwd?, message_count, preview?, session_key?` | `.chat-side-list` 的树：按 `cwd` 分组（`cwd == null` → `chat-group-empty`）；组头 `▸ <cwd>` + `.ws-count = items.length`；行 = `.row-btn.chat-session`：`title` → `.title`、`active` → `.live-dot`、`harness` → `BrandMark`、`updated_at` → `.time` |
| `api.chatStart(agent, model, cwd?)` | `{ id, agent, runtime, model }` | — | 开新会话（主操作） |
| `api.chatMessage(id, text)` | — | — | 发送 |
| `api.chatStop(id)` | — | — | 流式中 `.send-btn.stop-btn` |
| `api.chatResume(id)` | `{ id, agent, runtime, model }` | — | 从历史行重连 live 会话 |
| `api.chatHandoff(id, agent)` | 同上 | — | 命令面板/切换智能体 |
| `api.chatModel(id, model, runtime?)` | `{ …; switched: "live" \| "restarted" }` | `switched` | **必须可见**：`restarted` 时插一条 `.chat-msg.notice`（"已重启会话"），不能静默 |
| `api.chatSetOption(id, optionId, value)` | `{ options }` | — | 选项变更即时回写 picker |
| `api.chatClose(id)` | — | — | 关闭 live 会话 |
| `api.pickDirectory()` | `{ path: string \| null }` | `path` | `.ws-new-btn`（加工作区） |

**本地类型**（不在 `api.ts`，属视图内部）：
`Message { role: "user" \| "assistant"; text: string; done: boolean; notice?: boolean; icon?: string; kind?: "injection" \| "thought" }`；
`ChatEvent { type: string; [k: string]: unknown }`（SSE 流事件）。

### 4.2 分组与密度

- **两级分组**：工作区（`cwd`，**分组键 = normCwd(cwd)，见 §9.1**）→ 会话；**默认在全部会话上分组（不按 agent 隔离，§9 C1）**。单组默认折叠到 **5 行**，其余由 `.chat-group-more` 展开（`expandedGroups`）。
- **picker 顺序固定**：`categoryRank` = model(0) → mode(1) → thought_level(2) → 其他(3)。新增选项 id 必须落进这四级之一。
- **密度上限**（实测 chat 空态 214 节点 / 900px / 212 字符，余量极大；真正的风险在满会话）：
  - `.chat-log` 消息节点 **≤400**（超出则只渲染最近 400 条 + 顶部「加载更早」；滚动容器加 `content-visibility: auto`）。
    **理由**：`#task` 实测 26342 个 `.ev` 行造成 15.3 万节点与 2999ms 阻塞（MASTER §12 行 25/26），同一类错误不能靠 chat 侥幸——
    `Chat.tsx` 的 `Markdown` 未 memo、每个 chunk 都重建 messages 数组（上一轮审计的源码级推断），满会话是同一风险面。
  - 单条 `.chat-msg` 正文 **≤2000 字符**后折叠为 `.raw` 滚动块（`max-height` 生效）。
  - `.chat-side-list` 行数 **≤50**（`chatsHistory` 的 limit），超出不可自动加页——用户要去 `#sessions`。

## 5 四种状态

| 态 | 契约 | 现状 |
|---|---|---|
| **loading** | `agents === null` → `Spinner`（`Chat.tsx:826`）。历史轨独立 loading（`.chat-side-list` 内 `Spinner`），**不得阻塞输入器** | 已分离 |
| **empty** | 三种空：①无消息 → `.chat-empty-zone`（hero + 居中 composer + 3 个 `.suggest-chip`：`chat.suggest.1/2/3`）②历史空 → `.chat-side-list` 内 `p.muted`（`chat.historyEmpty` / 搜索无果时 `chat.noResults`）③组内无会话 → `.chat-group-empty` | 已实现 |
| **error** | **本页现状最差的一处，必须修**：`chatStart` / `chatMessage` 失败只出 2–4s toast，且会把原始错误串直接抛给用户。规格要求：①输入器上方出现**常驻** `ErrorState`（→ `.error-state`，role=alert + 重试，primitives §3.5）②失败文本保留在输入框里**不清空** ③`agents` 加载失败 → 页面级 `ErrorState` + 重试，而不是永久 `Spinner` | **违反**（`Chat.tsx` 无 `useAsync`，catch 后仅 toast） |
| **密集** | ①400 条消息上限 + 虚拟化（见 §4.2）②`.chat-thought` / `.chat-injection` 默认折叠（现状：thought 在流式中展开、结束后折叠 ✔）③`.chat-side` 50 行、单组 5 行折叠 | 部分违反（无虚拟化、无上限） |

## 6 响应式行为

| 视口 | 侧栏 | `.chat-layout` | `.chat-wrap` | 行为 |
|---|---|---|---|---|
| **390** | 72 | 单列 288 | 288×704 | 历史轨 = fixed slide-over `min(300px, 84vw)`；`.chat-side-toggle` 可见、`.chat-rail-toggle` 隐藏；`.chat-side-backdrop` 点击关闭（e2e 断言）。**`.composer-controls` 必须横向可滚不换行溢出**（当前 8 个 picker 在 294px 内会挤成两行——规格要求 `overflow-x:auto` 单行） |
| **768** | 72 | `grid[200, 448]` gap 0 14 | 448×760 | 轨仍内联（`@media max-width:940` 只改列宽，不改形态）；picker 行可换行 |
| **1280** | 228 | `grid[248, 733]` | 733×760 | 轨 248；无溢出 |
| **1440** | 228 | `grid[248, 889]` | 889×760 | 全出血（`.content` `padding-inline: 0`、`max-width: none`）；右内边距 `clamp(16px,2.6vw,48px)` = 37.4px |

横向溢出：**0**（实测四视口全 0）。`.chat-layout` 的 `min-height: calc(100vh - 140px)` 在 390 会让页面高 844（正好一屏）。

## 7 复用的共享原语与类名

**结构**：`.chat-layout` `.rail-collapsed` `.chat-side` `.chat-side-head` `.chat-side-list` `.chat-side-toggle` `.chat-side-backdrop` `.chat-rail-toggle` `.chat-ws-title` `.chat-group` `.chat-group-head` `.chat-group-name` `.chat-group-empty` `.chat-group-more` `.ws-count` `.ws-new-btn` `.ws-folder` `.ws-chip` `.ws-chip-name` `.row-btn` `.chat-session` `.live-dot` `.title` `.time`
**对话**：`.chat-wrap` `.is-empty` `.chat-empty-zone` `.chat-hero` `.chat-hero-mark` `.chat-suggest` `.suggest-chip` `.chat-log` `.chat-msg` `.user` `.agent` `.notice` `.chat-injection` `.chat-thought` `.thought-body` `.chat-typing` `.md`
**输入器**：`.chat-bottom` `.composer` `.composer-row` `.send-btn` `.stop-btn` `.composer-controls` `.ctl-select` `.ctl-agent` `.ctl-sep` `.sel-opt` `.chat-field`
**共享**：`.view-bar` `.grow` `.muted` `.mono` `.tag` `.tag.warn` 无 `.card`（本页 **0 个** `.card`/`.panel`，全出血是刻意的）
组件：`Markdown` `RelTime` `Spinner` `Empty` `Modal`（历史查看器 `.session-viewer`）`BrandMark` `Icon` `useToast`
**本页请求的 zone 化**：`.chat-side-head` + `.chat-ws-title` 应改用 `.zone-head` + `.zone-title`（Z1），让"历史轨"读成 zone 而不是自造标题。
新增类请求：`.sr-only`（隐藏 `h1`「对话」）、`.micro`（**11/15**，与 `.muted` 叠用）、`.error-state` + `.spinner-block`（经 `ui.tsx` 的三态契约）。**本页不新增数据文本类** —— primitives §7 R3 已拒绝该请求（它与 `.mono` 逐字相同），数据文本用既有 `.mono`。

## 8 本页特有的验收数字

| ID | 指标 | 基线（实测） | 目标 | 判定（MASTER 行） |
|---|---|---|---|---|
| C1 | `.view-bar h2` 可见 | 是「对话」 | **保留**（e2e 依赖） | 行 24 的例外：`h1` 用 `.sr-only` 另加，`h2` 不动 |
| C2 | ≥18px 可见文本节点 | **2**（空态：h2 + hero h3）；**活跃态 1** | **活跃态 ≥3**（h2 + 当前会话标题 + `message_count` 读数）；空态 2 | **行 6（2026-09-21 已落槌）**：`#chat` 属 MASTER §12.1 的豁免清单（无读数的页），门槛 **≥1**；交换条件仍成立——恰好 1 个 20px 页面标题（本页 `.view-bar h2`） |
| C3 | 最大字号 | **20px** | ≥20px（无仪表盘页） | 行 7 ✔ |
| C4 | 描边内容容器 | **1** = `.composer` | 恒 = 1 且必须是 `.composer` | **行 38**（本页是该例外的唯一持有点） |
| C5 | 可见描边元素总数 | **11** | ≤40（排除控件） | 行 5 ✔ |
| C6 | 无焦点环的 Tab 停留点 | **5/21**（`textarea`、`.ant-input-sm` 搜索框、3 个 `.ant-select-input`） | **0** | **行 16**（`index.css:1267` 裸 `outline:none` → A3 删除） |
| C7 | 信号色像素占比 | **0.51%** | ≤1.5% | 行 3 ✔ |
| C8 | 单亮度带占比 / 色度占比 | **73.8% / 1.78%** | ≤75% / 1.0–6.0% | 行 1、2（**73.8% 已贴上限**，引入会话标题读数后不得继续升高） |
| C9 | 历史轨行数 | 1（本机只有 1 条会话） | ≤50，单组折叠 5 行 | 本页特有 |
| C10 | `.chat-log` 消息节点 | 0（空会话） | **≤400** | MASTER 行 25/26 的同源风险面 |
| C11 | 内联 style 节点数 | 21 | ≤50、内联 `font-size` = 0 | 行 11、12 ✔ |
| C12 | `WS_HUES` 对比度 | 10 个硬编码 hex。**本规格初稿写的「亮色下 2.03–3.11:1（全部 <3:1）」经 primitives §7.1 复算后不成立**：暗 5.19–7.93（10/10 达标）；亮未补偿 2.11–3.22（1/10）；`index.css:1513` 的 filter brightness(.72) saturate(1.12) **已把它拉回 3.91–5.67（10/10）** | 登记为 `--ws-1..10` 双模式 token（暗 = 现行值，亮 = ×0.72 派生），**并删除 filter** | 真问题是 **filter 让 getComputedStyle 读不到最终色 → 审计无法计算**，不是颜色不达标。按 primitives §7.1 修正，**不保留错误结论** |

### ✅ C2 已落槌（原「需要回到 MASTER 决策」）

**MASTER 2026-09-21 增补 §12 行 6 + §12.1 已裁决**：行 6 从无条件「≥3 且 ≤40」改为条件规则「**含读数的页 ≥3；无读数的页 ≥1；上界 ≤40 不变**」，`#chat` / `#settings` / `#inbox` 三个无读数路由列入**豁免清单**；豁免的交换条件是「仍必须恰好 1 个 20px 的页面标题作为层级中心」——本页即 `.view-bar h2`「对话」。

本页据此的最终实现：

1. **活跃态 ≥3**（本规格 C2 的升级路径）：`h2` 20 + 当前会话标题 20/26 + `message_count` 18/24 mono —— **升的是已存在的真实字段，不是装饰**；
2. **空态 2 个**（`h2` + `.chat-hero h3`）：**属豁免范围，不再为难**；
3. **明确不做**：为凑 ≥3 新增第三种装饰性大字（本文件 §3 已把这条写成「禁止」）——这正是 MASTER §12.1 第 1 条硬边界要防的事。

（原请求「把行 6 改为 ≥3 / ≥2」已被 MASTER 采纳为更严的版本：豁免只降下界、不降上界，且附加了「层级中心」约束。）

## 9 侧栏 IA 与并行会话（2026-09-23 裁决；用户报告驱动）

> **用户原话**：「左侧的 session 是不区分任何一个 agent 的（至少不需要针对不同的 agent 进行隔离，**完全可以点击 session，然后切到对应会话以及 agent**）。而且**会话可能是并行的**……**我可能同时切换不同的会话和不同的 agent 对话和工作**。」

**事实基线（captain 实测）**：`GET /api/v1/chats` **不带 agent** → **24 条、跨 7 个 agent**；带 `agent=approver` → **1 条** ⇒ **侧栏当前只显示 1/24**（这就是用户看到的现象）。条目字段已含 `agent / runtime / model / cwd / session_key / preview / active` ⇒ **无需新增后端接口**（`api.chatsHistory(agent?)` 的 agent **本就是可选参数**）。`active` = 「daemon 仍持有该会话（流还在跑）」；SSE **按 chat 附着**且注释写明 **reattaching replays**；`api.chatHandoff(chatId, name)` 已存在。

| # | 契约（可判定） |
|---|---|
| **C1 解耦** | 侧栏**默认显示全部会话**（**不按 agent 隔离**）：`chatsHistory()` **不传 agent**。归属是**工作区**，不是 agent —— 用户从「我要继续那次对话」出发，而不是从「我要找某个 agent」出发。 |
| **C2 行内身份（2026-09-23 改写：从「显示要求」改为「识别要求」）** | **每条会话行携带它自己的 agent 作为元数据**（`entry.agent` / `entry.runtime`）；**空间不足时 agent 标签可截断、可省略**（**含 ≤520 的「手机元数据让位」规则 —— 允许隐藏 agent 标签**；只要下面两条识别路径仍在），但**该行必须仍可识别 agent**，**路径有且至少有两条**：(a) 该行有 `title` 属性（悬停可见完整 agent 名）(b) **选中该行后，会话头部（composer 的 agent 选择器）显示该 agent**。 **依赖关系（写明，不留给读者推断）**：**没有 `title` 属性时 (a) 路径不成立 ⇒ C2 会退化** —— 故 t92 的验收要求 `.chat-session-agent` 带 `title={h.agent}` 且 **DOM 实测该属性非空**。**配套优先级**（t89 已派 systems）：**标题 > 时间 > agent 标签**；**标题必须有下限宽度、任何情况下不得为 0**。**原写法与 C3 的关系**：**点击一行 = 切到该会话 + 它的 agent**（C3，已实现并验证）—— **那才是用户的实际需要**。 |
| **C3 点击语义** | 点击一行 = 切到「**那条会话 + 它的 agent / runtime / model**」，三个值**一律从该条目字段取**（`entry.agent` / `entry.runtime` / `entry.model`），**不得从当前选择器状态取**。 |
| **C4 可选过滤（默认关）** | 保留一个 **agent 过滤**控件，**默认关闭** ⇒ **过滤 ≠ 隔离**：用户可主动缩小范围，但**默认不做任何 agent 隔离**（限定见 §9.2）。 |
| **C5 并行是一等公民** | ① 切换会话**不得**打断 / 取消其它会话（**不得**对其它 chat 发 stop）；② 侧栏对 **`entry.generating === true`**（**正在生成**）的行给**可见标记** —— **不用 `active`**（语义见 §9.3）；③ 切回时**重新附着并续流**（SSE 已支持，**契约要求保留**这条行为）。 |
| **C6 agent 选择器语义** | `.ctl-select.ctl-agent` 的含义 = 「**当前会话的 agent**」：**有会话时**改动它 = **handoff 当前会话**（`api.chatHandoff(id, name)`，已实现）；**无会话时**它是**下一条新会话的默认值**。 |
| **C7 工作区分组规范化** | 见 §9.1（**可判定规则**）。 |
| **C8 删除语义（2026-09-23 新增；细则见 §9.4）** | **删除一条 chat = 删除它的记录（会话行与其消息）并终止其 live 会话（若有）**；**允许删除正在生成（`generating`）的会话，但必须先终止再删**；**失败必须回滚并给出可见错误，不得静默成功**；**重复删除 / 不存在 id 必须幂等**；**删除需二次确认**（判断与理由见 §9.4 D5）；**删除一条不得影响其它会话**（C5① 的延伸）。 |
| **C9 侧栏收起与伸缩** | chat 会话栏的**收起入口在所有宽度都必须可用**（修「1440 下 `.chat-side-toggle` = 0×0」）；**收起控件与主导航栏统一形态**（同图标 / 32×32 / `aria-label` 走 i18n）· **位置各自自然**（chat 栏放**头部**）；**收起态持久化**（`ruagent.*`，用户主动设置）；**两条侧边栏都可拖拽伸缩**，默认宽 **228 / 248 不变**、边界与门控见 `views/README.md` 的 **S1 / S2**。 |

### 9.1 工作区路径规范化的可判定规则（Windows 是一等平台）

**目标**：同一目录的不同书写必须落进**同一组**（captain 实测：`C:\...` 与 `C:/...` 被分成两组）。

**分组键 = `normCwd(cwd)`，按序执行 6 条规则**：
1. **去 `\\?\` 前缀**：`\\?\C:\x` ⇒ `C:\x`；`\\?\UNC\srv\share` ⇒ `\\srv\share`。
2. **分隔符统一为 `/`**：所有 `\` ⇒ `/`。
3. **折叠重复分隔符**（保留开头的 `//`，用于 UNC）：`C://x///y` ⇒ `C:/x/y`；`//srv//share` ⇒ `//srv/share`。
4. **去尾斜杠（根除外）**：`C:/x/` ⇒ `C:/x`；但 `C:/` 与 `/` 保持原样（否则根会变成空串）。
5. **盘符大写 + 其余小写（仅 Windows 形态）**：`c:` ⇒ `C:`；`C:/Users/AI` ⇒ `C:/Users/ai` —— Windows 路径**大小写不敏感**，分组键必须与之一致；**`/` 开头的 POSIX 路径保持原样大小写**（POSIX 大小写敏感）。
6. **UNC 保留 `//` 前缀**：`//srv/share` 不与 `/srv/share` 合并。

**验收判据（逐对判定，`normCwd(a) === normCwd(b)` 必须为真）**

| a | b | 依据 |
|---|---|---|
| `C:\Users\ai\proj` | `C:/Users/ai/proj` | 规则 2 |
| `c:\users\AI\proj` | `C:\Users\ai\proj` | 规则 5（盘符大写 + 其余小写） |
| `\\?\C:\Users\ai\proj` | `C:\Users\ai\proj` | 规则 1 |
| `C:/Users/ai/proj/` | `C:/Users/ai/proj` | 规则 4 |
| `C:/Users//ai///proj` | `C:/Users/ai/proj` | 规则 3 |
| `\\?\UNC\srv\share` | `//srv/share` | 规则 1 + 2 |

**反例（必须**不**相等）**：`C:/Users/ai/proj` ≠ `D:/Users/ai/proj`（不同盘）· `//srv/share` ≠ `/srv/share`（UNC vs POSIX 根路径）· `/home/ai` ≠ `/home/AI`（POSIX 大小写敏感）。

**显示**：分组标题显示**原始 `cwd` 的首次出现形式**（不显示规范化后的键）—— 规范化只用于**分组键**，不改用户看到的路径。

### 9.2 对第 1 / 4 / 5 条的判断（design-lead；**不推翻第 1 条**）

- **第 1 条（解耦）**：**同意**，并建议把默认口径**写死**为「`chatsHistory()` 不传 agent」**且**「工作区分组在**全部会话**上做」—— 否则同一个工作区会被**同一个 agent 拆成多组**。
- **第 4 条（过滤）建议加两条限定**：① 过滤是**视图级、非持久**（不写进 URL、不跨刷新保留），否则「默认关闭」会被一次深链悄悄破掉；② 过滤命中为空时**必须显式空态**（「没有该 agent 的会话」+ 清除按钮），**不得**退化成显示全部会话 —— 那会让用户以为过滤没生效（与 §5 的 empty/error 口径一致）。 **这是「空集必须被区分」在 UI 上的形态**：**「筛出来是空的」与「没有筛选」是两件事，而它们的输出看起来一样（一列会话）** —— 与 MASTER §12.10.5 **第 4 条**（判据匹配不到对象时静默通过）**同族**。
- **第 5 条（并行）建议补一条**：`active` 的**可见标记必须与「当前正在看的那条」可区分**（当前会话用 `.row-btn.selected`，`active` 用独立标记）—— 否则「在跑」与「在看」会读成同一件事；且**切换会话时不得重置其它会话的 `generating`**（前端不得用本地状态覆盖服务端值；`active` 是生命周期，见 §9.3）。
- **一处需你裁的取舍**：`#chat?agent=<id>` 深链（§1 注）在解耦后**语义变了** —— 它**不再**代表「侧栏显示哪个 agent 的会话」。建议**保留但重定义为「新会话的默认 agent」**（即 C6 的「无会话时的默认值」），并在契约里写明它**不再影响侧栏列表**。


### 9.3 `active` 与 `generating` 的语义（2026-09-23 修正；**C5② 的前提曾被证伪**）

- **@active@ = 生命周期**：daemon **仍持有该会话**（@chat.rs:1063@ @e.active = live.contains(&e.id)@；文档原话 @chat.rs:218@「A live chat the daemon is still holding」；**@chat.rs:1124@ 有空闲回收器** ⇒ 会随时间衰减，实测当前 **2/25**）。**它不是运行状态** —— 实测：**不挂 SSE** 起一段会话，@POST /chat/{id}/messages@ → 202、@message_count@ **2 → 4（回复已到）**，而 @active@ 在 **t=6s / 48s / 108s 及数分钟后一直是 true** ✗
- **@generating@ = 运行状态**（**t85** 由 daemon 在 @/api/v1/chats@ 暴露）：**正在生成回复**；**@generating === true ⇒ active === true@，反之不成立**。
- **C5② 因此依赖 @generating@**：用 @active@ 做标记会让**这段 daemon 生命周期内用过的每条会话都亮** ⇒ **标记失去区分力**（「看起来实现了、而它没有区分力」）。
- **教训（详见 MASTER §12.10.5 第 8 条）**：**「可判定」不等于「前提成立」** —— C5② 的文本完全可判定、也完全自洽，但它依赖一个**错的前提**；**前提是在实现之前由 captain 直接测出来的**（不是靠读契约、也不是靠读代码）。

**C5③ 的前提已验（2026-09-23，captain 实测）：成立 ⇒ 本条无需改动。**
- `GET /api/v1/chat/{id}/events` → **HTTP 200 / `content-type: text/event-stream`**；附着后**立即收到 10 行 / 2323 字节**，且是**从 `seq: 0` 开始的重放**（`seq 0 state_changed` · `seq 1 context_injected` · `seq 2 user_message` · `seq 3 agent_message_chunk` · `seq 4 usage_update` · `seq 5 stopped`）⇒ **切走再切回会重放并重新附着** ✔
- **⚠️ 同一次验证里，探针自己出了一个目标 bug**（已记为 MASTER §12.10.5 第 4 条的又一实例）：探针去找 `event:` 行统计事件类型，**而事件类型在 `data:` 的 JSON 里**（SSE 用默认 message 事件 + JSON payload）⇒ 它打印「**8s 内未收到任何事件**」，**而 2323 字节的事件就在缓冲区里** ⇒ **「读到 0」= 「量错了对象」**。**若只看那句话就下结论，会得出与事实相反的结论（「C5③ 前提不成立」）并去改一个本来正确的契约。**
- **对策**：**读到「空」时必须先证明「我在正确的对象上读」**，并把这一步**写进判据**（例如 **SSE 探针必须同时报告 `bytes > 0` 与 `seq` 范围**，而不是只报告事件类型计数）。

**两条前提的对照（本节的收尾）**

| 前提 | 结果 | 处置 |
|---|---|---|
| **C5②**：`active` = 正在生成 | **失败** | 契约已改：标记对象换成 `generating` |
| **C5③**：SSE 会重放 | **成立** | 无需改动（证据见上） |

⇒ **「验前提」不是走过场：两次里抓到一次真问题，另一次确认了正确性** —— 而**确认正确同样有价值**（它把「我猜的」变成「我验过的」）。

### 9.4 删除的可判定细则（2026-09-23）

**D1 作用域**：**删除 = 移除该 chat 的记录（会话行与其消息）+ 终止其 live 会话（若有）**。删除**不区分来源**，也**不影响工作区**（工作区是分组键，不随其下会话被删而消失 —— 空工作区是否显示由 §4.2 的分组规则决定）；**若 daemon 侧引入「来源」限制，须由 t95 说明并在本行更新**。

**D2 与 `active` / `generating` 的关系**：
- **允许删除 `generating` 的会话**；**但必须先终止、再删** —— 否则会留下「**仍在写、而记录已删**」的竞态。
- **收场**：删除后，附着在该会话事件流上的客户端**必须观察到明确定义的收场**（连接关闭或终态事件；**具体形态由 t95 定义并实测**）。**面板的处置（可判定）**：把该会话从视图移除 · **不得因此进入 error 态** · **不得留下 spinner 或「停止」按钮** —— 该收场是**预期行为**，不是错误。

**D3 失败回滚（可判定）**：采用乐观移除时，**失败必须同时满足三条**：① **该行必须重新出现**（回到删除前的位置）② **必须出现一条可见的错误提示**（用户可读到失败，或至少「删除失败」）③ **该行不得停留在 pending 态**。**不得静默成功** —— 「行消失了但没删掉」是最坏的一种（用户以为删了，刷新后又回来）。 **同源形态（2026-09-23 captain 提示）**：本代已判过「**界面在说谎**」的另一面 —— 会话删除按钮写「删除」、文案写「不可撤销」，**而删除会自我撤销**（索引器 60s 后把行扫回来）⇒ 那是「**成功被谎报**」；**D3 是它的反面：「失败被谎报成成功」**。**两者都是「界面显示的状态 ≠ 真实状态」**，也都**没有任何信号**（用户只能靠事后发现）。⇒ **D3 的三条（行回来 · 错误可见 · 不停在 pending）与「删除必须真的删除」是同一条纪律的两端**：**界面对状态的陈述必须与真实状态一致**。

**D4 幂等（可判定）**：**重复删除同一条、或删除一个已不存在的 id**，面板的**最终状态必须是「该行不在列表里」且不报错**；daemon 侧的 **404 / 204 差异不得泄漏为用户可见的错误**。**双击 / 重复触发**同样按幂等处理（行处于 pending 时**不得接受第二次删除**）。

**D5 二次确认（design-lead 判断：**需要**，但形态要「轻」）**
- **判断：需要**。理由三条：① **不可逆**（无回收站 / 无撤销）② **内容是用户数据**（消息数可能是几十条）③ **与既有口径一致**（session 删除已被本代定为「危险动作 ⇒ 确认」）。**列表卫生的收益，不值得一次误删的代价。**
- **形态（可判定）**：确认里**必须显示将被删除的对象身份**（标题 + agent + 消息数），使「删的是不是我要删的那条」**可判定**；**破坏性按钮不得是默认焦点**（防回车误触）；文案必须**写明不可撤销**。
- **不采纳的更省事方案**：**行内一点即删（无确认）** —— 省一次点击，但把「**不可逆的数据丢失**」放在一个**与选中行相邻**的位置上（误触代价最高、而恢复手段为零）。**按本代总纲**：危险动作的默认值应由**产物**保证（确认），而不是要求用户「小心点」。

**D6 与并行的关系**：**删除一条会话不得终止 / 取消其它会话**（C5① 的延伸；可判定：删除后其它 `generating` 会话的流仍在跑）。

### 9.5 列表稳定性（2026-09-23 新增；C5 的同族）

> **用户报告**：「我看到**行的位置变了**，但是**复现确实试了好多次才能复现一次**」+「**界面有些不流畅，各种组件交互不丝滑，需要把细节全面优化**」。
> **排查（captain）**：**候选「折叠展开造成位移」证伪**（单组折叠到 5 行时**第 6 行之后不在 DOM 里** ⇒ 点击不可能触发展开；**被点击的行本来就在可见区** ⇒ 它所在的组已展开）；**候选「排序键并列」证伪**（`updated_at` **0 组并列**（37 条）· 两次拉取**顺序逐位相同**）。**判断**：与「不流畅」**同根因** —— **轮询每 3–5 秒重新拉取并整表替换 history**，**而此刻有成员在不停创建测试会话** ⇒ **新会话插到最顶 ⇒ 所有已有行下移一位** ⇒ 用户看到「我在看的那行不在原位」，**且它与点击在时间上常相邻** ⇒ 被读成「点击把它移走了」。

**L1 排序键唯一**：会话列表顺序**只由 `updated_at DESC` 决定**；**分组顺序**由工作区在列表中的**首次出现位置**决定。
**L2 除发消息外，任何交互都不得改变任何行的顺序**：**点击 / 切换 / 悬停 / 选中 / 收起展开**均**不得**写 `updated_at`、**不得**做本地重排。
**L3 折叠 / 展开不得被误读为排序**：展开一个折叠组**只允许在该组内部显示更多行**，**不得**把该组移到列表顶部。
**L4 唯一允许的重排**：**发消息** ⇒ 该行移到其**分组内第一位**（**分组顺序不变**）。
**L5 刷新不得扰动列表**：**数据刷新（轮询 / 重取）不得**改变行的**身份与顺序**（除数据本身变化外）· **不得**丢失**滚动位置**与**焦点** · **不得**造成可见跳动。**判据**：**无数据变化时，一次刷新前后「行序序列 + `scrollTop` + `document.activeElement`」三者不变** ✓
**L6 新行插入的位置必须可预测**：新会话**按其排序键进入应在位置**，**且不得导致其它行的身份变化**（**React key 稳定**）✓

**判定方法（可判定）**
1. **点击一行** ⇒ 捕获「行序序列」（`route` + 标题的序列）**前后对比，必须完全相同** ✓
2. **控制跑：发一条消息** ⇒ 该行**必须移动到第一位** —— **①+② 合起来才说明判据不是恒真的** ✓
3. **展开折叠组** ⇒ 序列**允许**在组内变长，但**组间顺序不得变化** ✓
4. **无数据变化时的一次刷新** ⇒ 「行序序列 + `scrollTop` + `activeElement`」三者**必须不变** ✓

**与 C5 的关系**：**列表是「多会话并存」的视图，它的稳定性本身就是需求** —— 并行会话是**一等公民**（C5），而**并行的前提是列表不会因为一次刷新而失稳** ✓

**tools 的实测读数（t99，2026-09-23；428s 观测）**：**6 次顺序变化，原因全部定位** —— **纯重排 0 · `updated_at` bumped 0 · `active` / `generating` 翻转 0** ✓ **全部是 `NEW ROW(S) APPEARED at index 0` 或 `ROW(S) REMOVED`** ✓ ⇒ **新会话插到最顶 ⇒ 已有行整体下移一位**，**而它与用户的点击在时间上相邻 ⇒ 被读成「点击移走了它」** ✓ **点击测试直接证伪**：`domChanged=false · apiChanged=false`（点击第 3 行后 DOM 行序与 API id 序列**逐位相同**）✓ ⇒ **「因果相邻被读成因果相连」现在有了直接证据** ✓（对应 §12 **行 47 / 48 / 49**）

**判据设计条款（t99 的两个坑，2026-09-23）**
- **空集绿**：tools 的初版交互探针**全为 0** —— **CLS 过滤掉点击后 500ms 内的位移** · **且 1.2s 窗口短于 3–5s 的轮询间隔** ⇒ **判据在「什么都没发生」时也会绿** ✗（与「夹具绿 ≠ 判据在看对象」同族，但**发生在时间维度** ✓）⇒ **对策：凡带时间窗口的判据，窗口必须长于被测事件的最小周期，且必须证明窗口内确实发生了事件** ✓
- **控制跑必须真的制造出被测事件**：**只建空 chat ⇒ 列表完全不变**（0 消息被 t90 过滤）⇒ **控制跑会是一次假 PASS** ✗ ⇒ 改为**建 chat + 发一条消息** ✓

**D7 SSE 收场的四种形态（客户端必须认全）**：收场有**两条路径、共四种形态** —— ① 会话自己的终态事件 **`stopped`** ② **`state_changed` + `status=completed`** ③ 注册表-gone 路径的 **事件名 `end`**（`sse_end` 发的是 `Event::default().event("end")`，`api.rs:3401`）④ **EOF**（连接关闭）⇒ **客户端必须认全四种，否则会卡在 streaming 态** ✗（**这正是用户报的「不丝滑」的一种** ✓）。**暂不统一两条路径**（客户端无论如何都要认全 ⇒ 统一是**加法**不是必需 ✓ **YAGNI**）；若将来要统一，另开单 ✓
