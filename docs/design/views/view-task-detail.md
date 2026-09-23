# task-detail — `#task/<id>`

> **路由**：`#task/<Run.id 之外的 Task.id>`（`parseHash()` 的 `/^task\/([\w-]+)/`）
> **视图**：`panel/src/views/TaskDetail.tsx` → `TaskDetail({ id, onBack })`（`key={view.id}` 强制重挂载）；执行日志在 `panel/src/views/RunTimeline.tsx` → `RunTimeline({ run, live })`
> **消费的 MASTER 行**：§12 行 **5、6、7、13、17、18、20、22、25、26**（本页特有，数量最多）+ README §2 全局
> **消费的变更编号**：`C2 C5` · `T2 T5 T6` · `S1` · `R1` · `A1 A2 A3 A4` · `Y1 Y2 Y3` · `Z1` · `P4`（MASTER 附录 C：**性能批次是本页的前置**）
> **冻结选择器**：`.view-bar h2`（e2e `toContainText` 任务标题）、`.card.launcher`（e2e `toBeVisible`）、`.row-btn`

> ⚠ **本页是全站唯一的 P0 页**：实测 **152,329 节点 / 26,342 行 `.ev` / 490,088 字符 / 最长单次阻塞 2,999ms**。
> MASTER §12 行 25（DOM ≤3,000）与行 26（longtask ≤200ms）**只对本页有意义**。规格给出的手段按顺序是：
> **①合并流式 chunk → ②虚拟化 → ③图标 sprite → ④scrollTop 直写**（与 `docs/ui-ux-review.md` §二的建议一致，本文把它们写成验收条件）。

## 1 职责与首要任务

**职责**：一个任务的执行台——启动运行（单发/扇出/流水线）、看每个运行、对比多个运行、裁决赢家、落盘、以及**读执行日志**。
**首要任务**：**搞清楚"这个任务跑到哪了、成没成"**。90% 的停留发生在那条 580px 高的执行日志里。

据此决定：
- 页面头（`.view-bar`）承载**身份与破坏性动作**：`← 返回看板` + `h2`=任务标题 + `.tag`=`project` + `[删除]` + `[运行]`。
- `task.intent` 紧跟其后（`.muted.intent`，14/22，max-width 720px）——**意图必须可见**，否则日志里的行为无法解释。
- 启动器（`.card.launcher`）是 **panel**，它的模式切换（单发/扇出/流水线）用 antd `Segmented`；**三种模式共用同一个卡片**，不各占一块。
- 执行日志是**一等公民**（580px 固定高 + 自动跟随 + 「跳到最新」），不是可折叠的附属。
- 「扇出对比」（`.compare`）只在 `runs.length > 1` 时才有意义。

## 2 版式骨架

```
1440：侧栏 228 │ content 1212（可用 1149）
┌ .view-bar（可换行，flex-wrap）────────────────────────────────────────────┐
│ [← 返回看板](link)  h2 <task.title>  .tag <project>  .grow  [删除] [运行](primary) │
├ .intent  14/22 max-width 720 ────────────────────────────────────────────┤
│ <task.intent>                                                            │
├ .card.launcher 1149（冻结：e2e 断言可见）─────────────────────────────────┤
│  [运行│扇出对比│流水线]   agents 多选 / steps 编辑 / prompt / repo / options │
│  .row.end  [启动]                                                        │
├ h3.sec「扇出对比」───────────────────────────────────────────────────────┤
│ .compare  grid auto-fit minmax(310px,1fr) gap 14                          │
│  .card.compare-card[.selected] ×N                                         │
│    ● StatusDot  <agent 名>  <成本>          [落盘](当 landable)           │
│    .compare-result  <run.result 或 空态>                                  │
│    .row  [选它](link)  [重试] [取消]                                      │
├ h3.sec「运行 （N） · 共 $X」─────────────────────────────────────────────┤
│ .card > .row-btn[.selected] ×N     ← 点击切换下面显示哪个 run 的日志        │
│   ● StatusPill  <agent>  .muted.mono <run.id 前 8>  .grow  .time          │
├ h3.sec「执行日志」──────────────────────────────────────────────────────┤
│ .timeline-wrap 1149×622（1 个 run 时）                                     │
│  .timeline-head  .muted.mono <id 13 位…> <acp_session_id> <workspace>      │
│                  <成本>  .live-flag「● 实时」  .time                        │
│  .timeline  max-height 580  overflow-y auto                               │
│   .ev ×26342（实测！）其中 agent_thought_chunk 24446 / agent_message_chunk 1572
│     .ev.thought  .ev-icon + .thought-text                                 │
│     .ev.msg      .md（Markdown）                                          │
│     .ev.tool     .tool-head（可展开）→ .tool-detail > pre.raw             │
│     .ev.plan     .plan-body > .plan-head + .plan-item[.done]              │
│     .ev.sys / .ev.perm[.resolved] / .ev.inject / .ev.err                  │
│   .jump-latest（回到底部按钮，离底 >60px 时出现）                          │
└──────────────────────────────────────────────────────────────────────────┘
390：content 318（294）｜.timeline 294×580（不缩，横向由 .raw 内部滚动）；.compare 单列；
     .timeline-wrap 高 726（head 换行）；pageH 1599
```

**实测**：`.timeline` 1149×580（@1440/1280/768）、294×580（@390）；`.timeline-wrap` 622 / 654 / 726；`.card`（对比卡）321 高；`.compare` `auto-fit minmax(310px,1fr)`。

## 3 层级

**本页最大元素 = `.view-bar h2`（title 20/26 w600，任务标题）**，实测 `maxFs = 20`、**≥18px 节点只有 1 个**。

为什么本页**不该**有仪表盘：任务详情是一个**状态机视图**，它的"读数"（运行数、成本、状态）已经嵌在 `h3.sec`「运行 （1） · 共 $5.88」与 `.compare-card` 的每卡头部里，处于**它们所描述的内容旁边**。把它们抽成顶部仪表会让"哪个数字属于哪个运行"变模糊。
**但行 6（≥3 个 ≥18px 节点）因此不达标**——诚实做法不是硬凑，而是把**已经存在但过小**的三个数字升到该有的档位（全部是真实数据）：

| 升级项 | 现状 | 规格 | 字段 |
|---|---|---|---|
| 成本合计 | `.muted`（13px，拼在 h3 里） | `h3.sec` 拆成「运行 （N）」+ **`.readout.s`（18/24 mono）`共 $X`** | `Σ runs[].cost_usd` |
| 选中 run 的成本 | `.muted`（13px） | `.compare-card` 头部 → **`.readout.s`** | `Run.cost_usd` |
| context 用量 | `UsageMeter`（12px mono） | 保留 12px（它是一根进度条，不是读数） | `Run.context_usage.used/size` |

升级后 ≥18px 节点 = `h2`20 + 运行总数读数 18 + 每张 `.compare-card` 成本 18（≤6 张）= **≥3**，且**没有一个是装饰**。

**禁止**：为凑行 6 而给日志行加放大字号；给 `.ev` 行加卡片化背景（26342 行会变成 26342 个盒子）。

## 4 内容模型

### 4.1 数据源

| 调用 | 返回类型 | 用到字段 | 展示位 |
|---|---|---|---|
| `api.task(id)`（**2s 轮询**，`TaskDetail.tsx:64`） | `TaskDetail` | `task: Task`、`runs: Run[]`、`selected_run_id`、`selected_by`、`judgement: Judgement \| null`、`landable: boolean` | 全页 |
| `api.agents()` | `AgentInfo[]` | `id, name` | launcher 的 agent 多选；`agentName(run)` 把 `run.params.agent` 映射成可读名 |
| `api.startRun(taskId, agent, prompt, repo?, options?)` | `Run` | — | launcher「运行」 |
| `api.fanout(taskId, agents[], prompt, repo?, options?)` | `{ runs: Run[] }` | — | launcher「扇出对比」（`agents.length ≥ 2`，否则 `launcher.needs2`） |
| `api.pipeline(taskId, steps[{agent, prompt?}])` | `{ tasks: string[] }` | `tasks` | launcher「流水线」（`steps.length ≥ 2`） |
| `api.judgeTask(taskId, agent)` | `{ judge_run: Run }` | — | 裁决（`judgeAgent` 选择器 + `judging` busy） |
| `api.selectRun(runId)` | — | — | `.compare-card` 的「选它」→ 回写 `selected_run_id` |
| `api.landTask(taskId)` | `{ landed: string }` | `landed`（短 hash） | `landable` 时的「落盘」→ toast 报 hash |
| `api.cancelRun(runId)` / `api.retryRun(runId)` | — / `Run` | — | 行内动作（**取消是破坏性动作，现状无二次确认 → 规格要求 `Popconfirm`**） |
| `api.updateTaskStatus(id, status)` | — | — | 状态变更 |
| `api.deleteTask(id)` | — | — | `.view-bar` 的「删除」（现状已用 `Popconfirm` ✔） |
| `GET /api/v1/runs/{runId}/events` | SSE `EventLine { ts, seq, event }` | live → `EventSource`；非 live → `fetch` 全文并逐行 `data: ` 解析 | `.timeline` |

### 4.2 字段 → 展示位（关键映射）

| 展示位 | 字段 | 规则 |
|---|---|---|
| `.compare-card` 是否存在 | `runs.length > 1` | 单运行时隐藏整个「扇出对比」段 |
| `.compare-card.selected` | `selected_run_id === r.id` | 赢家卡 |
| 赢家标记 `.tag.ok` | `r.id === winner` | — |
| 「落盘」按钮 | `landable && selected_run_id === r.id` | `landTask` |
| 裁决块 | `judgement { judge_run_id, judge_run_status, winner_run_id, rationale }` | `rationale` 走 `Markdown`；`judge_run_status` 用 `StatusPill` |
| `.row-btn` 状态点 | `Run.status` | `StatusDot`/`StatusPill`（`ui.tsx` 的 `STATUS_COLORS`） |
| `live` | `Run.status ∈ {queued, spawning, running, waiting_permission}` | `activeStatuses`（`TaskDetail.tsx:70`）→ `RunTimeline` 用 SSE 还是 fetch |
| `.timeline-head` | `run.id`、`run.acp_session_id`、`run.workspace`、`run.cost_usd`、`run.updated_at` | 全部 `.muted.mono`，13 位截断 |
| `.waiting_permission` | `Run.waiting_permission?.title` | **必须可见**：run 卡在权限上时，行内要显示标题 + 跳 `#inbox` |
| `.ev` 行类型 | `event.type` ∈ {agent_message_chunk, agent_thought_chunk, tool_call, tool_call_update, plan, usage_update, routed, user_message, context_injected, permission_requested, permission_resolved, state_changed, stopped, error} | 14 种，每一种有固定外壳（`.ev.thought/.msg/.tool/.plan/.sys/.perm/.inject/.err`） |

### 4.3 密度上限（本页的核心约束）

| 项 | 实测 | 上限 | 手段（按顺序） |
|---|---|---|---|
| `.ev` 行数 | **26,342** | **≤600** | **①在 state 层合并连续同类 chunk**：连续 `agent_thought_chunk` → 1 行、连续 `agent_message_chunk` → 1 行。实测 24,446 条 thought chunk 只承载 82,272 字符（**平均 3.4 字符/行**），合并后行数降到百级 |
| DOM 节点 | **152,329** | **≤3,000** | ②`.timeline` 虚拟化（`content-visibility: auto` 或窗口切片）+ ③`Icon` 改 sprite/`mask-image`（现状每行一个内联 `<svg>`，实测 24,563 个） |
| 最长阻塞 | **2,999ms** | **≤200ms** | ①②③ + ④自动滚动改 `box.scrollTop = box.scrollHeight`（现状每来一行 `scrollIntoView`，在 615,467px 高容器上反复触发布局） |
| 字符数 | **490,088** | ≤60,000 | ①合并 + 单行 `.raw` `max-height` |
| `.compare-card` 数 | 1（本机） | **≤6**，超出横向滚动 | `.compare` 已是 `auto-fit`，超出后设 `grid-auto-flow: column` |
| 单条 `.ev` 文本 | — | `.md` 段落 1 行内联 + 展开；`<pre class="raw">` max-height | 已有 `.raw` |

## 5 四种状态

| 态 | 契约 | 现状 |
|---|---|---|
| **loading** | `!task` → `Spinner`（`TaskDetail.tsx:68`） | **违反（MASTER 行 20，两轮未修）**：①`refresh()` 是 `.catch(() => {})`——**错误被吞掉**，`task` 永远为 `null` → **永久 spinner** ②spinner 标签写的是 `board.title`（"任务"）而不是本页标题 ③无重试、无错误文案。规格要求：`catch` 里 `setError(e)` → 渲染 `role=alert` + 「重试」+ 返回看板；标签改用任务标题（或"载入任务…"） |
| **empty** | ①`runs.length === 0` → `p.muted.pad`「还没有运行」（`task.noRuns`）②`run.result == null` → `.muted`「无结果」③``.timeline` 无事件 → `p.muted.pad`「没有事件」 | 三处已实现；但**空态与错误态不可区分**（见上） |
| **error** | ①**API 失败** → 见 loading 行 ②**Run 失败**：`run.status === "failed"` → `.ev.err` 行 + `.compare-card` 上的 `.tag.err`；`run.error` 必须**渲染出来**（现状只在 timeline 的 `error` 事件里出现，`Run.error` 字段本身未展示）→ 规格要求 `.compare-result` 顶部显示 `run.error` ③**SSE 中断**：`live` 但 `events` 流断开 → 显示「连接中断 · 重新连接」（现状 `es.onerror` 是空函数，静默） | **违反** |
| **密集** | 见 §4.3。**这就是本页存在的理由**——所有密度手段都为了这一态 | **违反**（15.3 万节点） |

## 6 响应式行为

| 视口 | 侧栏 | content | `.timeline` | 行为 |
|---|---|---|---|---|
| **390** | 72 | 318（294） | 294×580 | `.compare` 单列 310→294（`minmax(310px,1fr)` 会溢出 → **规格要求把 minmax 下限改 `min(310px,100%)`**）；`.timeline-wrap` 726（head 换行）；pageH 1599 |
| **768** | 72 | 696（662） | 662×580 | `.compare` 单列；pageH 1440 |
| **1280** | 228 | 1052（996） | 996×580 | `.compare` 3 列 |
| **1440** | 228 | 1212（1149） | 1149×580 | `.compare` 3 列；pageH 1374 |

横向溢出：**0**（四视口实测全 0）。`.timeline` 的 580px 是**固定高**——规格保留（它是"飞行记录仪"的高度，不随视口变），但要求 ≤700 视口时改 `max-height: 60vh`。

## 7 复用的共享原语与类名

`.view-bar` `.intent` `.card` `.card.launcher`（冻结）`.launcher` `.pipeline-builder` `.step-n` `.step-num` `.compare` `.compare-card` `.selected` `.compare-result` `.row-btn` `.selected` `.tag` `.tag.ok` `.tag.warn` `.tag.err` `.muted` `.mono` `.time` `.grow` `.pad` `.hint`
日志：`.timeline-wrap` `.timeline-head` `.timeline` `.ev` `.ev-icon` `.ev.msg` `.ev.sys` `.ev.thought` `.thought-text` `.ev.err` `.ev.inject` `.ev.perm` `.resolved` `.tool-head` `.tool-detail` `.tool-update` `.plan-body` `.plan-head` `.plan-item` `.done` `.plan-check` `.raw` `.jump-latest` `.live-flag` `.chev` `.md`
组件：`Spinner` `Empty` `Modal` `Markdown` `RelTime` `StatusDot` `StatusPill` `UsageMeter` `fmtUsd` `fmtTokens` `useToast` `Segmented` `Input` `Button` `Popconfirm`
**本页请求的 zone 化（Z1）**：三个 `h3.sec`（扇出对比 / 运行 / 执行日志）是"规则线上的一句标签 + 数字"→ 规格要求改用 `.zone-head` + `.zone-title` + `.zone-note`（数字进 `.zone-note`），这正是 `.zone` 原语要解决的第一个真实场景。
新增类请求：`.sr-only`（隐藏 `h1`）、`.micro`（**11/15**，与 `.muted` 叠用）、`.error-state` + `.spinner-block`。**本页不新增数据文本类**（primitives §7 R3 已拒绝该请求，数据文本用既有 `.mono`）。

## 8 本页特有的验收数字

| ID | 指标 | 基线（实测） | 目标 | 判定（MASTER 行） |
|---|---|---|---|---|
| D1 | DOM 节点数 | **152,329** | **≤3,000** | **行 25** |
| D2 | 最长单次阻塞 | **2,999ms**（累计 6,316ms / 9 次） | **≤200ms** | **行 26** |
| D3 | `.ev` 行数 | **26,342**（thought chunk 24,446） | **≤600**（合并后） | 行 25 的手段 |
| D4 | 内联 `<svg>` 数 | **24,563** | ≤600（sprite / `mask-image`） | 行 25 的手段 |
| D5 | 内容可交互时刻 | 7,431ms 时 26,342 行 | **首次可读 ≤1,000ms** | 行 26 的推论 |
| D6 | **错误态 ≠ 空态** | `catch(() => {})` → **永久 spinner**；spinner 标签还写错成"任务" | `role=alert` + 「重试」+ 返回看板 | **行 20**（本页是该行最严重的违反） |
| D7 | 暗色文本对比度失败数 | **18,016 / 19,327**（thought 正文 13px） | **0** | **行 13** |
| D8 | ≥18px 文本节点 / 最大字号 | **1 / 20px** | ≥3（§3 的三处 18/24 升级）/ ≥20px | **行 6、7** |
| D9 | <32px 命中目标 | **198**（timeline 行内图标按钮） | <24px = 0；<32px ≤10 | **行 18** |
| D10 | 无名称可交互元素 | **5** | **0** | **行 17** |
| D11 | 可见描边元素数 | **1,679**（几乎全是 `.md` 表格单元格与 `.raw`） | ≤40（**排除 `.md td/th` 与 `.raw`**） | **行 5**（判定必须带这条排除规则，否则本页永远不通过） |
| D12 | 描边内容容器 | **0** | ≤1 | 行 4 |
| D13 | 单亮度带 / 色度占比 | **15.4% / 5.02%** | ≤75% / **1.0–6.0%** | 行 1、2（色度已贴上界） |
| D14 | 内联 style 节点数 | **162**（其中 131 个内联 `font-size`） | ≤50 / 内联 `font-size` = 0 | 行 11、12 |
| D15 | SSE 中断可见 | `es.onerror = () => {}`（静默） | 「连接中断 · 重新连接」+ 自动重连 | 行 20 同源 |
| D16 | `Run.error` 可见性 | **未渲染** | `.compare-result` 顶部显示 `run.error` | 本页特有（正确性） |
| D17 | 取消运行的二次确认 | **无** | `Popconfirm` | MASTER §12 行 20 同源 + 审计 P2 |

**本页不引用任何已删除原语**：primitives §10.4 的 11 条删除清单见 [`README.md`](./README.md) §3.4 X6 / §3.5 判定表。原先本行记录过一个已删除的 IO 标签类（primitives §10.1 D6），它对应的容器类在 `index.css` 里**从来没有规则**（`grep -c 'tool-io' panel/src/index.css` = 0）——本页的 §7 清单与线框已一并改成真实存在的 `.tool-detail > pre.raw`；行内 IO 标签一律用 `.muted.micro`。
