# t10 — 设计契约评审：对话与运行时间线（Chat / RunTimeline）

> **评审对象**：t4 交付的 `panel/src/views/Chat.tsx`（1413 行）+ `panel/src/views/RunTimeline.tsx`（661 行）
> **对照契约**：`docs/design/views/view-chat.md` §8 C1–C12、`docs/design/views/view-task-detail.md` §8 D1–D17、`docs/design/MASTER.md` §12（+ §12.1/§12.2/§12.3）、`docs/design/views/README.md` §2 全局行
> **评审人**：design-lead · 2026-09-21 · **只评审，未改任何 panel/ 文件**
> **判定**：**PASS（t4 交付物无阻断项）**；发现 2 项**非 t4 归属**的阻断行 + 3 项低危文档项，逐条给出归属与证据。

---

## 0 方法与口径（先声明，再给结论）

本环境**图像桥不可用**（read_image / describe_image / modlens 均不可用），因此**没有任何人看过像素**。本报告不含"看起来"类判断，每条结论指向：**文件名 + 行号**，或**实测数字 + 产生它的命令**。

**主仪器**（团队可复现口径，t1 交付）：

```bash
cd panel
node tools/design-audit.mjs --json --routes=chat,task \
  --task-id=01a0bc0c-2fcd-75c3-b531-d6cfc4f2e01d \
  --out=<临时目录>            # 不写 docs/screenshots/（那是 tools/verifier 的产物目录）
```

> **仪器版本纪律（本次踩到）**：`tools/design-audit.mjs` 在本轮评审期间被 `tools` 持续修改（**mtime 23:34:08**）。我第一次跑（23:20）得到的是**旧判据**的结论（行 6 报 `chat=2`、行 7 报 `task=20px 仪表盘页 ≥32`）；**重跑后行 7 已 PASS、行 6 变成另一处失败**。**凡引用审计结论，必须同时给工具的 mtime**，否则结论会指向已不存在的判据。

**辅助仪器**（4 个一次性 Playwright 探针，只读 DOM/计算样式，落在系统临时目录，未写入仓库）：

| 探针 | 回答的问题 |
|---|---|
| `t10-probe1.mjs` | #chat 交互元素 <24px 及其**可点祖先**；antd Select 的焦点环落在谁身上；#task 密度（.ev / svg / summary） |
| `t10-probe2.mjs` | 三个 composer picker 的**自身盒高**；**活跃态** ge18（mock `/api/v1/chats` + `/api/v1/chat`，数据 mock、DOM/CSS 真实） |
| `t10-probe3.mjs` | `/api/v1/**` 全阻断时 chat / task 的 `role=alert`；#chat @390 `.composer-controls` |
| `t10-probe4/5/6/7.mjs` | 用**审计工具自己的** `INTERACTIVE_SEL` 复算 <24px 集合 + 祖先盒；`<summary>` 逐元素归属；task 阻断态复测；两种模式的焦点环 |

---

## 1 结论摘要

| 维度（本任务验收口径） | 结论 | 证据锚点 |
|---|---|---|
| 该路由规格的验收条目是否实现 | **实现**：#chat C1–C12 全部有落点（C6/C12 的残余不在 t4 文件）；#task D1–D17 中 15 行达标、D5 未测、D9 由**共享层**决定 | §2 / §3 |
| 层级中心是否存在 | **存在**：两页都是 `.view-bar h2` 20/26 + 恰好 1 个 `h1` | §4.1 |
| 四种状态是否齐全 | **齐全**：loading / empty / error / 密集 四态两页均有独立实现，error 与 empty 可区分 | §4.2 |
| 四个断点是否不塌 | **不塌**：390/520/768/1024/1280/1920 六视口横向溢出**全 0**；@390 `.composer-controls` 单行横滚不换行 | §4.3 |
| 是否复用共享原语而非硬编码 | **复用**：两文件**裸 hex = 0**、内联 `font-size` = 0、无自造样式；新增 2 个纯钩子类（无 CSS）已登记 | §4.4 |
| 跨文件一致性：Home 与 Sessions 的 `sourceHue` 语义必须一致 | **一致**：全站**唯一**定义在 `Sessions.tsx`，`Home.tsx:18` import 同一个函数 | §5 |

**归属结论（关键）**：两个仍失败的行**都不是 t4 的文件能修的**——
* **行 18**（`<24px 必须为 0`）：#task 的 8 个 `<summary>` 命中区高度由 **index.css** 的 `.hint summary` / `.tool-update summary` 决定（16px），#chat 的两个 23px picker 是 **antd SM 档 + index.css 布局**的结果 → **归属共享层（t16/systems）**。t4 的 inScope 明确排除 `index.css`，且若用内联 style 补高度会违反 t4 自己的验收（"新增内联 style 的字号与颜色硬编码为零"）。
* **行 6**：`hi` 被解析成 1 → **归属审计工具（t17/tools）**，见 §6-F3。

---

## 2 逐条核对：`view-chat.md` §8（C1–C12）

仪器：审计工具（mtime 23:34:08，`--routes=chat`）+ probe1/2/3/6/7。空会话态 @1440×900，两模式。

| ID | 目标 | 实测 | 判定 |
|---|---|---|---|
| **C1** | `.view-bar h2` 保留「对话」 | 行 24 evidence：`1 "对话"`；`Chat.tsx:1201 <h2>{t("chat.title")}</h2>` | ✅ |
| **C2** | 活跃态 ≥3；空态 2（§12.1 豁免，门槛 ≥1） | **空态 ge18 = 2**（`h2` 20 + `.chat-hero h3` 20）；**活跃态 ge18 = 3**（probe2：`h2` 20 + `.readout.s` 18「7 条」+ `.chat-session-head h3` 20） | ✅（附条件，见 §6-F5） |
| **C3** | 最大字号 ≥20px | `maxFontSize = 20`（sizeHist 无越界档，`offScaleCount = 0`） | ✅ |
| **C4** | 描边内容容器恒 = 1 且必须是 `.composer`（行 38） | `strictContainers = 1`，唯一命中 `div.chat-wrap.is-empty > div.chat-bottom > div.composer`（4 边）；`Chat.tsx:924 <div className="composer">` | ✅ |
| **C5** | 可见描边元素 ≤40 | `allBordered = 11`（hairline 12 / fourSide 5） | ✅ |
| **C6** | 无焦点环的 Tab 停留点 = 0 | 审计：dark **0/20**（PASS）、light **3/20**（FAIL，全部 `input.ant-select-input`）。**实测反证**（probe6，两模式）：聚焦该 input 时，**input 自身** `outline: none`，但其**焦点所有者** `.ant-select-sm` 上 `box-shadow: 0 0 0 2px` = `#f5b457`(暗)/`#8a5600`(亮)，即 `--focus-ring`；规则是 `index.css:1582 :root .ant-select:not(.ant-select-disabled):focus-within { box-shadow: 0 0 0 2px var(--focus-ring) }` | ⚠️ **判据口径问题**（§6-F2），非实现缺陷 |
| **C7** | 信号色像素 ≤1.5% | `signalRatio` 暗 **0.52%** / 亮 **0.56%** | ✅ |
| **C8** | 单亮度带 ≤75%；色度 1.0–6.0% | 暗 `lumaBand = 73.94%`（≤75，余量 1.06pp）、`chroma = 1.78%`；亮 `chroma = 1.16%` | ✅（**贴上限**，规格 §8 已自带警告：引入会话标题读数后不得继续升高；实测未升高，基线 73.8% → 73.94%） |
| **C9** | 历史轨 ≤50 行、单组折叠 5 行 | `Chat.tsx:1112 items.slice(0, 5)`；`.chat-group-more` 带 `aria-expanded`（`:1165`）；条数上限来自 `api.chatsHistory(agent)` 默认 `limit=50`（`api.ts:689`） | ✅（代码级） |
| **C10** | `.chat-log` 消息节点 ≤400 | `Chat.tsx:42 const MSG_CAP = 400`；`:904-905` 窗口切片 + `:1265-1270` `.chat-earlier`「显示全部」；`:43 LONG_MSG = 2000` + `:1328` 单条折叠 | ✅（代码级；本机无 400 条会话可实测） |
| **C11** | 内联 style ≤50、内联 `font-size` = 0 | `inline: { styleTotal: 22, fontSized: 0 }`；`Chat.tsx` 8 处 `style={{...}}`（均为 dot 取色/尺寸），`RunTimeline.tsx` 2 处 | ✅ |
| **C12** | `WS_HUES` → `--ws-1..10` 双模式 token **并删除 filter** | `Chat.tsx:31-36`：`wsColor()` 返回 `var(--ws-${i+1})`，**无硬编码 hex**；`index.css:80-89`（暗）/ `:126-135`（亮）已定义 `--ws-1..--ws-10`。**但 `index.css:1642 html[data-mode="light"] .ws-folder { filter: brightness(.72) saturate(1.12) }` 仍在** | ⚠️ **半交付**：Chat.tsx 侧 ✅；filter 在 **index.css（t4 的 outOfScope）** → 归属 t16，见 §6-F4 |

**结论**：#chat 的 12 行中 **10 行完全达标**，2 行（C6 / C12）的残余**都不在 t4 的文件里**。

---

## 3 逐条核对：`view-task-detail.md` §8（D1–D17）

仪器：审计工具（`--routes=task`）+ probe1/3/5。参考任务 `01a0bc0c-2fcd-75c3-b531-d6cfc4f2e01d`，@1440×900。

| ID | 目标 | 实测 | 判定 |
|---|---|---|---|
| **D1** | DOM ≤3,000 | **476**（暗）/ 468（亮）；基线 152,329 → **降 99.7%** | ✅ |
| **D2** | 最长单次阻塞 ≤200ms | `longtask.max` 暗 **85ms** / 亮 0ms，`over200 = 0` | ✅ |
| **D3** | `.ev` 行数 ≤600 | **挂载 28**（虚拟化 `MAX_MOUNTED=30`，`RunTimeline.tsx:144`）；折叠后全量 405（t4 报告） | ✅ |
| **D4** | 内联 `<svg>` ≤600 | **32**；基线 24,563 | ✅ |
| **D5** | 首次可读 ≤1,000ms | **未测**——见 §7 | ⏳ 未测 |
| **D6** | error ≠ empty：`role=alert` + 重试；`.ant-empty` 不出现 | 阻断 `/api/v1/**`（probe5）：`[role=alert] ×2`、`.error-state ×1`、`.ant-empty = 0`、spinner = 0，正文「无法加载任务 / daemon 不可达，或这个任务已经不存在。/ 重 试 / ← 返回看板」；`TaskDetail.tsx:91,130` 两处 `ErrorState` | ✅ |
| **D7** | 暗色对比度失败 = 0 | `wcagViolations = 0`（暗 100 个文本节点受检）；亮同样 0 | ✅ |
| **D8** | ≥18px ≥3 / 最大字号 ≥20px | `ge18 = 3`（`h2` 20 + `.zone-note .readout.s` 18 + `.zone-head .readout.s` 18）；`maxFontSize = 20`；审计行 7 在 23:34 版工具下 **PASS**（按 `view-task-detail.md` 的"无仪表盘页 ≥20px"读） | ✅ |
| **D9** | `<24px = 0`；`<32px ≤10` | `<32px = 5` ✅；**`<24px = 11` ❌** → 8 个 `<summary>`（5×`.ev.tool-update` **16px**、2×`.ev.inject` **21px**、1×`details.hint` **16px**，全部 `padding: 0`）+ 3 个 antd 内层 `input`（其**控件盒**为 `.ant-select-outlined` **31px** ≥24）。**高度来源是 index.css**：`:743 .tool-update summary { line-height:16px }`、`:672 .hint summary { line-height:16px }`、`.ev.inject summary` 无独立高度（继承 `.ev`，21px） | ❌ **归属共享层**（§6-F1） |
| **D10** | 无名称可交互元素 = 0 | `0/43`（暗/亮同） | ✅ |
| **D11** | 可见描边 ≤40（排除 `.md td/th` 与 `.raw`） | `allBordered = 35`，`mdTableCells = 0`——**已不需要那条排除规则** | ✅ |
| **D12** | 描边内容容器 ≤1 | `strictContainers = 0` | ✅ |
| **D13** | 单亮度带 ≤75% / 色度 1.0–6.0% | 暗 `lumaBand = 27.18%` / `chroma = 5.21%`（贴近 6.0 上界）；亮 `chroma = 1.76%` | ✅ |
| **D14** | 内联 style ≤50 / 内联 `font-size` = 0 | `inline: { styleTotal: 29, fontSized: 0 }` | ✅ |
| **D15** | SSE 中断可见 | `RunTimeline.tsx:204 es.onerror` → `setDropped(true)` + 指数退避重连（`:208-214`）；`:369-373` 渲染 `.tag.err`「daemon 不可达」+ 重试按钮 | ✅（代码级；需真实断流才能实测） |
| **D16** | `Run.error` 可见 | `TaskDetail.tsx:197-199 {r.error ? <span className="tag err">{t("task.runError")}</span> … }` | ✅（代码级；本机 run 无 error 字段，故 DOM 不渲染） |
| **D17** | 取消运行二次确认 | `TaskDetail.tsx:329` / `:775` 两处 `Popconfirm` | ✅ |

**结论**：#task 的 17 行中 **15 行达标、1 行未测（D5）、1 行失败（D9）**；D9 的失败点在 **index.css 的高度声明**，不在 t4 的两个文件里（t4 的 JSX 只写 `<details className="ev tool-update"><summary className="muted">`，与规格 §7 的共享原语清单逐字一致）。

---

## 4 四个横向维度

### 4.1 层级中心（每页"最大元素"）

| 页 | 层级中心 | 实测 |
|---|---|---|
| `#chat` | `.view-bar h2`（title 20/26 w600），另有 `.sr-only h1`「对话」 | 行 24：`h1 = 1`；`Chat.tsx:1200-1201` `<h1 className="sr-only micro">` + `<h2>`。**`.micro` 是必须的**——`.sr-only` 不重置字号，隐藏 `h1` 会继承 UA 2em=28px 落进行 8 的字号越界（`Chat.tsx:1197-1199` 有注释）；实测 `offScaleCount = 0`，这条防住了 t16 的那个坑 |
| `#task` | `.view-bar h2` = 任务标题（20/26） | 行 24：`h1 = 1`「chat 项目目录全链路…」；`maxFontSize = 20` |

两页都**没有**装饰性大字：chat 的第三种 ≥18px 被规格 §3 明文禁止，t4 用"会话标题 + 轮次读数"两个**真实数据**凑到 3（§2-C2 已实测）。

### 4.2 四态

| 态 | `#chat` | `#task` |
|---|---|---|
| **loading** | 页面级 `Chat.tsx:890 if (!agents) return <Spinner label="对话…"/>`；**历史轨独立**：`:1105-1106 {history === null ? <Spinner/> : …}`（在 `.chat-side-list` 内，不阻塞输入器 ✔ 规格 §5 明文要求） | `!task` → `Spinner`（`TaskDetail.tsx`），阻断态实测 spinner=0 且已换 `ErrorState` |
| **empty** | 三种空齐：①`.chat-empty-zone`（hero + 居中 composer + 3 个 `.suggest-chip`，`Chat.tsx:1222-1250`）②历史空 ③`.chat-group-empty` | 三处齐（无运行 / 无结果 / 无事件），实测 `.empty-state` 与 `.ant-empty` 均为 0（当前任务有数据） |
| **error** | **从"最差"修到"可区分"**：页面级 `agentsError → ErrorState`（`:881-888`）；发送失败 `sendError → ErrorState + 重试`（`:912-921`）；**失败文本回填输入框**（`:628 setInput(cur => cur ? cur : text)`），不再只出 2s toast。阻断实测：`[role=alert] ×2`、`.error-state ×1`、重试按钮「重 试」、`.ant-empty = 0` | `[role=alert] + 重试 + 返回看板` 实测通过；`Run.error` 渲染（D16）；SSE 断流可见 + 重连（D15） |
| **密集** | 400 条上限 + `LONG_MSG` 折叠 + 单组 5 行折叠（C9/C10） | 折叠 + 虚拟化：`.ev` 26,343 → 挂载 28，DOM 15.3 万 → 476 |

**error 与 empty 可区分**这一条（MASTER 行 20）在两页都是**实测**通过，不是推断。

### 4.3 四个断点

审计工具对 6 个视口（390/520/768/1024/1280/1920）逐页测 `scrollWidth - innerWidth`：

| 页 | 390 | 520 | 768 | 1024 | 1280 | 1920 |
|---|---|---|---|---|---|---|
| `#chat` | 0 | 0 | 0 | 0 | 0 | 0 |
| `#task` | 0 | 0 | 0 | 0 | 0 | 0 |

侧栏宽 `siderWidth`：72 @≤992 / 228 @>992（行 36 ✔）。@390（probe3）`.composer-controls`：`flex-wrap: nowrap`、`overflow-x: auto`、`clientWidth 261 → scrollWidth 357`（**单行横滚**，规格 §6 明文要求"横向可滚不换行溢出"）、`docOverflow = 0`。该规则在 `index.css:1399-1402 @media (max-width:520px)`。

### 4.4 共享原语复用 / 硬编码

* **裸 hex：0**（`grep -nE '#[0-9a-fA-F]{3,8}' src/views/Chat.tsx src/views/RunTimeline.tsx` → 无输出）。颜色一律走 token（`var(--ws-*)` / `var(--brand-*)` / `--signal-text` / `--status-*`）。
* **内联 `font-size`：0**；内联 style 22（chat）/ 29（task），行 12 上限 50 ✔。
* **zone 化（规格 §7 的 Z1 请求）已落地**：`Chat.tsx:1070 <div className="chat-side-head zone-head">`、`:1104 <div className="chat-ws-title zone-title">`、`RunTimeline.tsx:357 <div className="timeline-head zone-head">`——**双类名**而非替换冻结选择器，符合 README §3.4 X3。
* **三态原语**：`.error-state`（`ui.tsx` 的 `ErrorState`）、`.spinner-block` 均在用；**没有**视图私有三态实现。
* **新增钩子类 2 个**：`.chat-session-head`（`Chat.tsx:1257`）、`.chat-earlier`（`:1267`）——`index.css` 中规则数 = **0**，即纯选择器钩子（视觉由并用的 `.chat-hero h3` / `.row-btn` 提供）。与已登记的 `.is-empty` 同类，见 §6-F6。

---

## 5 跨文件一致性：Home 与 Sessions 的 `sourceHue`（本组特有风险）

| 检查 | 证据 | 结论 |
|---|---|---|
| 是否存在**两份**取色实现 | `grep -n 'Hue\|hue' src/views/Home.tsx` → 只有 `:185 embedderHue`（**另一个概念**：embedder 状态点，`:297`），**没有**第二个 source 取色函数 | ✅ 单一实现 |
| 定义处 | `Sessions.tsx:41-54`：`claude-code → var(--brand-claude)` / `opencode → var(--brand-opencode)` / `deepseek → var(--brand-deepseek)` / `default → var(--ant-color-text-tertiary)` | 与 `view-sessions.md` §4.1 终稿签名**逐字一致** |
| 消费处 | `Home.tsx:18 import { SOURCE_LABEL, msToIso, sourceHue } from "./Sessions"`；`:265 <span className="dot" style={{ background: sourceHue(s.source) }} />`；`Sessions.tsx:131` 同一函数 | ✅ **同一个函数实例**，语义不可能分叉 |
| 函数名/签名 | `export function sourceHue(s: string): string`——与 §4.1 约束 2（"签名不变，改窄会静默落到 default"）一致；注释里也写明了这条 | ✅ |
| 域合规（§3.1 I） | 返回值集合 = `{--brand-claude, --brand-opencode, --brand-deepseek, --ant-color-text-tertiary}`，**不含** `--signal`/`--status-*`/`--graph-*`/`--ws-*` | ✅ |

**结论**：跨文件一致性**无风险**——架构上只有一处定义，Home 是纯消费者。

---

## 6 发现清单（按归属分组）

### F1 · 行 18：`#task` 8 个 `<summary>` 命中区 <24px — 阻断行，**归属共享层（t16 / systems）**

* **实测**：`<24px = 11`（审计 23:34 版）。逐元素归属（probe5）：
  | 元素 | 尺寸 | 容器 | 高度来源 |
  |---|---|---|---|
  | `summary.muted` ×5 | 230–246 × **16** | `details.ev.tool-update`（RunTimeline.tsx:644-652 的 JSX） | `index.css:743 .tool-update summary { line-height:16px }` |
  | `summary` ×2 | 403/91 × **21** | `details.ev.inject` | 无独立规则，继承 `.ev`（`index.css:722`） |
  | `summary.muted` ×1 | 1109 × **16** | `details.hint` in `.card.launcher`（TaskDetail 的 JSX） | `index.css:672 .hint summary { line-height:16px }` |
  | `input.ant-select-input` ×3 | 21 | `.ant-select-outlined` **31px** | **口径假阳性**（控件盒 31 ≥ 24） |
* **为什么不是 t4 的问题**：这 8 个元素的高度**没有一条来自 t4 的 JSX**（全部来自 index.css 或 antd）；t4 的 inScope 是 `['panel/src/views/Chat.tsx','panel/src/views/RunTimeline.tsx']`，`index.css` 明确在 outOfScope；且 t4 的验收要求"新增内联 style 的字号与颜色硬编码为零"，用内联补高度会自相矛盾。
* **必需的修法**（共享层）：给 `summary` 加命中区下限，例如 `index.css` 的 `.hint summary, .tool-update summary { min-height: 24px; display: flex; align-items: center }`，并给 `details.ev.inject > summary` 同款 `min-height: 24px`。**这与 captain 已并入 t16 的 `.tool-head min-height` 是同一类**（`.tool-head` 现已 `min-height:32px`，实测 32px ✔，说明该修法在本仓库可行）。
* **判据旁注**：`<summary>` 是原生展开控件，`primitives §5.2.2` 已把它当作交互控件（"原生 `<summary>` 自动 pass"）；行 18 的判据"可见交互元素 `getBoundingClientRect()`"没有给 `<summary>` 任何例外。

### F2 · 行 16：审计判据读错了焦点环的**载体** — 判据缺陷，**归属 tools（t17）+ primitives §5.1 措辞（systems）**

* **实测反证**（probe6，两种模式）：`document.activeElement` = `input.ant-select-input`，其**自身** `outline: none` / `box-shadow: none`；但**焦点所有者** `.ant-select-sm` 上：暗 `box-shadow: 0 0 0 2px rgb(245,180,87)`（=`#f5b457`），亮 `box-shadow: 0 0 0 2px rgb(138,86,0)`（=`#8a5600`）——正是 `--focus-ring`。
* **规则出处**：`index.css:1582 :root .ant-select:not(.ant-select-disabled):focus-within { box-shadow: 0 0 0 2px var(--focus-ring) }`（注释说明：antd 的 outline 被 `!important` 占用，所以环改用 box-shadow 画在同一个 2px token 上）。
* **审计的表现**：dark 判 **0/20 PASS**、light 判 **3/20 FAIL**，两模式同一 DOM 却不同结论 → 工具内部对"环画在祖先上"的处理不一致。
* **必需的修法**：`primitives §5.1` 的措辞由"每停留点记录 `outlineStyle`/`outlineWidth`/`boxShadow`"改为"**记录焦点元素自身及其焦点所有者（最近的、因 `:focus-within`/`:focus-visible` 而变色的祖先）**的 outline/box-shadow，取两者中可见的那个"；工具相应改为向上找环。**在修好之前，行 16 对含 antd Select 的页不可判**。

### F3 · 行 6：上界被解析成 1，该行**永不可能 PASS** — 判据缺陷，**归属 tools（t17）**

* **实测**：审计 23:34 版对 `#chat` 与 `#task` 全部 4 个 capture 报同一句 `note: 'N > 上界 1'`（`chat=2`、`task=3`）。
* **根因**：`design-audit.mjs` 的 `parse: (t) => ({ lo: numAt(t, 0, 3), hi: numAt(t, 1, 40) })`，而 MASTER 行 6 的目标文本是「**含读数的页 ≥3；无读数的页 ≥1；所有页 ≤40**」——第 2 个数字是**豁免门槛 1**，不是上界 40。
* **必需的修法**：`hi: numAt(t, 2, 40)`（或直接匹配 `≤40` 子串）。修完后按 §12.1：`#chat` 门槛 ≥1（实测 2 ✔）、`#task` 门槛 ≥3（实测 3 ✔），行 6 对这两页应为 **PASS**。
* **旁证**：同一行在 23:20 的旧版本报的是 `chat/dark=2`（扁平判据，无豁免）——两次都 FAIL，但**原因不同**，说明该行的判据本身在漂移，**必须钉死**。

### F4 · C12 后半（删 `.ws-folder` 亮色 filter）— 已解锁未执行，**归属 t16**

* `index.css:1642 html[data-mode="light"] .ws-folder { filter: brightness(0.72) saturate(1.12) }` 仍在。
* 解封条件（`Chat.tsx:27-35` 已改用 `var(--ws-*)`）**已满足**；filter 的作用是让 `getComputedStyle` 读不到最终色 → 审计无法计算。captain 已并入 t16，本报告只确认它**仍未删**。

### F5 · C2 的活跃态 ≥3 有一个**窗口期空洞** — 低危，**归属 t4（可选修）+ 规格措辞**

* 读数 `.readout.s` 的门是 `Chat.tsx:1202 active && currentSession?.message_count != null`，而 `currentSession = history?.find(h => h.id === chatId)`（`:900`）。
* **实测**：当 `history` **包含**该 live 会话时，ge18 = **3** ✔（probe2）；**新建会话后、`refreshHistory()` 回来之前**，`chatId` 不在 `history` 里 → `currentSession` 为 null → ge18 = **2**。
* **是否违规**：不违规——绑定门槛是 MASTER 行 6 + §12.1（`#chat` 豁免页 ≥1），2 ≥ 1 ✔。这是规格自设的"升级路径"存在一个窗口期空洞。
* **可选修法**：`message_count` 缺失时回退到本地计数 `messages.filter(m => m.role === "user" && !m.kind).length`，让活跃态恒 ≥3。
* **旁注**：真实 daemon 的 `GET /api/v1/chats?limit=50` 返回 **24/24** 条都带 `message_count` 与 `active`（实测），所以这个门在稳态下是开着的。

### F6 · 2 个未登记的钩子类 — 低危，**归属 design-lead（docs/design/views/README.md §3.5）**

* `.chat-session-head`（`Chat.tsx:1257`）与 `.chat-earlier`（`:1267`）在 `index.css` 里规则数 = 0。
* README §3.5 的"view-private"清单目前只登记了 `.ctl-agent` / `.dash-main` / `.is-empty` 三个。**要么登记这两个，要么让它们消失**。登记是我（design-lead）的 inScope，但需要先与 t4 确认它们不会被后续改动删掉——本报告只提出，不在本轮改。

### F7 · 规格偏离（已文档化）：会话标题的位置 — 低危，**归属规格**

* `view-chat.md` §3 要求把当前会话标题"放进 `.view-bar`，与 `h2` 并列"；t4 把它放在 `.view-bar` **下方**的 `.chat-hero.chat-session-head`（`Chat.tsx:1251-1260`），并在代码里写了理由（`view-bar` 在该档位没有副标题槽）。
* **验收不受影响**：层级中心仍是 `.view-bar h2` 20px，活跃态 ge18 = 3 实测达标，无溢出。
* **建议**：改规格（把"放进 `.view-bar`"改为"放进页首标题区，20/26，与 `.view-bar h2` 构成层级中心"），而不是改代码——这是"文档跟不上实现"的一类。

### F8 · 全局行 37（间距越界 = 0）仍失败 — **归属共享层/外壳**，非 t4

* `#chat` 48 次越界（10 个值：`7px×20`、`14px×9`、`22px×6`、`-1px×4`、`0.5px×2`、`-12px×2`、`11px×2`、`0.1px×1`）；`#task` 57 次（13 个值）。
* 抽样指向 **shell / antd 自有元素**：`.brand-mark` 的 `padding: 7px`、`.sidebar-foot button`、`.ant-select`/`.ant-segmented-item-label` 的 `11px`、`.row-btn` 的 `margin: 0 -12px`。这些都不是 t4 的两个文件里的声明。
* **登记**：这是**全局行**，属 t8 全站验收的残留清单；本报告只确认它在 `#chat`/`#task` 上仍然失败，且**不是 t4 引入**。

---

## 7 未测项（不假装达标）

| 项 | 为什么没测 | 谁能测 |
|---|---|---|
| **D5 首次可读 ≤1,000ms** | 需要"内容可交互"的时刻标记；审计的 `timing.load` 含 2.5s settle，不是该口径 | 需要 `PerformanceObserver` + 首帧可读探针（t8） |
| **C10 的 400 条上限** | 本机没有 400+ 条的 live 会话，无法触发 `windowOffset > 0` 分支 | 造数据或用 mock（本报告只做代码级核对） |
| **D15 SSE 真实断流** | 需要在流式中途掐断连接；本次只做了代码级核对 + 阻断 `/api/v1/**`（那是初始加载失败，不是中途断流） | t8 |
| **D16 `Run.error` 的渲染** | 参考任务的 run 没有 `error` 字段 → 元素不渲染；只做了代码级核对 | t8（造一个失败 run） |
| **行 6 / 行 16 的最终判定** | 判据本身待修（F2 / F3），修好前任何结论都会误导 | t17 |

---

## 8 复现命令（逐字）

```bash
# 0) 仪器版本（引用任何审计数字时必须同时给出）
cd panel && ls -la --time-style=+%m-%d_%H:%M:%S tools/design-audit.mjs      # 本轮：09-21_23:34:08

# 1) 主仪器（输出到临时目录，不污染 docs/screenshots/）
cd panel && node tools/design-audit.mjs --json --routes=chat,task \
  --task-id=01a0bc0c-2fcd-75c3-b531-d6cfc4f2e01d --out=<临时目录>

# 2) 硬编码检查（期望无输出）
cd panel && grep -nE '#[0-9a-fA-F]{3,8}' src/views/Chat.tsx src/views/RunTimeline.tsx

# 3) 跨文件一致性（期望只有一处 export function sourceHue）
cd panel && grep -rn 'sourceHue' src/

# 4) 辅助探针（本轮实际跑过的四个脚本，落在系统临时目录）
node <temp>/t10-probe1.mjs   # <24px + 可点祖先 / antd Select 焦点环 / #task 密度
node <temp>/t10-probe2.mjs   # composer picker 盒高 / 活跃态 ge18（mock 数据）
node <temp>/t10-probe3.mjs   # /api/v1/** 阻断态 role=alert / @390 composer-controls
node <temp>/t10-probe5.mjs   # <summary> 逐元素归属 / task 阻断态复测
node <temp>/t10-probe6.mjs   # 两模式焦点环载体
```

---

## 9 判定

**t4 交付（Chat.tsx + RunTimeline.tsx）＝ PASS。**

* 两页的层级中心、四态、四断点、共享原语复用、跨文件一致性**全部有实测或行号证据**，无主观项。
* 仍失败的两个阈值行（18 / 6）**均不由 t4 的文件决定**：行 18 的高度声明在 `index.css`（t4 的 outOfScope），行 6 的失败是工具把上界解析成 1（t17）。
* 剩余 3 项低危（F5 窗口期空洞、F6 钩子类登记、F7 规格措辞）+ 1 项登记（F8 全局行 37）都不阻断本页交付。

**给 captain 的处置建议**：F1 → t16（与已并入的 `.tool-head min-height` 合并成一次共享层改动）；F2/F3 → t17（两个判据都要修，否则 t8 的全站验收在这两行上永远是噪声）；F4 → t16（已解锁，可直接删）；F5/F6/F7 → 由我（design-lead）在规格侧收口；F8 → t8 登记为全局残留。
