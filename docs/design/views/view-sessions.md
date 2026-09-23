# sessions — `#sessions`

> **路由**：`#sessions`
> **视图**：`panel/src/views/Sessions.tsx` → `Sessions()`；同文件导出 `SessionDetail`（原始会话查看器浮层）、`sourceHue()`、`msToIso()`、`SOURCE_LABEL`
> **消费的 MASTER 行**：§12 行 **1、3、5、6、12、17、18、22**（本页特有）+ README §2 全局
> **消费的变更编号**：`C2`（**核心：信号色误用 `V1`**）`C5 C9` · `T2 T5 T6` · `S1` · `R1 R2` · `A1 A2 A3 A4` · `Y1 Y3` · `B1` · `Z1 Z2`
> **冻结选择器**：`.view-bar h2`、`.row-btn`、`.ant-empty`、`.card`

## 1 职责与首要任务

**职责**：把**本机所有编码 CLI 的历史会话**（claude-code / dsh / ruagent / opencode / codex）汇成一张索引表，并提供「读原文」和「蒸馏成记忆」两个动作。
**首要任务**：**找到某一次会话并打开它**。蒸馏是次要动作（200 行里只会有几次）。

据此决定：
- 过滤优先于搜索：来源是**唯一**的分面（`Segmented` 一行放得下，实测 5 项），不引入搜索框——本地索引 200 行，`Segmented` + 时间倒序足够。
- 行内**不放**第二个输入；蒸馏按钮**常显**（原 `.row-action` 门控，**2026-09-23 起改为常显** —— 门控造成「用户以为没有删除/归档按钮」的投诉，且实测没换来布局收益：`opacity` 不占布局、`1440` 标题宽 **441px 两态相同**、三视口溢出全 0；见 primitives §3.3 C10 与 MASTER §12.13）。
- 行用 `role="button"` 的 `div`（`.row-btn`）而不是真 `<button>`：行内要塞一个蒸馏按钮，`<button>` 套 `<button>` 是非法 HTML 且产生两个激活目标（源码注释已写明）。
- 读数行只放两个数（会话数 / 消息总数）——这一页的"大"属于**列表本身**，不属于仪表。

## 2 版式骨架

```
1440：侧栏 228 │ content 1212（可用 1149）
┌ .view-bar ─────────────────────────────────────────────────────────────┐
│ h2 会话    .muted「N 个会话」   .grow   [全部│dsh│Claude Code│ruagent│opencode] │
├ .readout-strip  panel 1149×91 ─────────────────────────────────────────┤
│   200会话        │       4061消息        （filtered 后重算）            │
├ .card 1149×8432 ──────────────────────────────────────────────────────┤
│ .row-btn  (×200, 行高 42.16px)                                         │
│  ┌ .tag ────────┐ ┌ .title ───────────────┐ .muted(12) .tag  .muted   │
│  │ ● <来源标签> │ │ <SessionRecord.title> │  12 条     <agent>  mono    │
│  │  ↑ .dot      │ │                        │                    <项目> │
│  └──────────────┘ └───────────────────────┘        .grow    .time      │
│                                                        [蒸馏] .row-action│
└───────────────────────────────────────────────────────────────────────┘
滚动到底 = 8724px（200 行）——本页是全站最长的页

查看器（overlay，非路由）：.modal-backdrop > .modal.wide.session-viewer
┌ .modal.wide (min(760px,100%)) ────────────────────────────────────────┐
│ .modal-head  <title>  .muted(12,w400) 来源·条数·时间        [关闭]     │
│ .modal-body.session-log（max-height 64vh, 滚动）                       │
│   .chat-msg.user ×N   /   .chat-msg.agent > Markdown                   │
└───────────────────────────────────────────────────────────────────────┘
390：content 318（可用 294）｜.row-btn 内 .title 单行省略；**≤520 的让位规则已改由 §4.2 的 IA 裁决接管**（收起路径/来源/状态 tag、保留「标题 + 时间」；共享层 `≤520` 规则在本页已成死规则 —— 见 §6 的「≤520 的验收口径」）
     .readout-strip 高 150px（2 格换行）
```

**实测**：`.card` 1149×8432（@1440）、996×8432（@1280）、662×8432（@768）、294×8432（@390）。
行高 = 8432 / 200 = **42.16px**；`.readout-strip` 高 91px（@1440/1280/768）、150px（@390）。

## 3 层级

**本页最大元素 = `.readout`（32/40 mono，2 个）**，实测 `maxFs = 32`、≥18px 节点 **3** 个（h2 + 2 读数）。

为什么是两个读数而不是四个：这一页有两个**有意义的聚合**（会话数、消息数）。把"来源数""今日新增"凑成 4 格会让仪表比数据更响——P3 要的是**跳档**，不是**凑数**。两个读数已经在 1149px 里各占一半，视觉重量充足。
次级：`.row-btn` 的 `.title`（14/21，weight 400——**不是 600**：200 行全加粗会变成一片黑，行标题靠"独占一行+省略号"取胜）。
第三级：`.time` / `.muted`（11–12px，等宽，tabular-nums 对齐）。

**禁止**：给行标题加粗、给行加"选中态"背景（本页没有选中概念——打开就是进浮层）。

## 4 内容模型

| 调用 | 返回类型 | 用到字段 | 展示位 |
|---|---|---|---|
| `api.sessions()`（10s 轮询，`Sessions.tsx:60`） | `SessionRecord[]` | `key, source, title, project, ref_path, started_at, updated_at, message_count, preview, agent?` | 全部行 |
| `api.sessionMessages(key)` | `{ messages: { role: string; text: string; ts: number }[] }` | `role, text, ts` | 浮层 `.session-log` |
| `api.sessionDistill(key)` | `{ distilled: { memories_written, memories_skipped, entities_written, relations_written, agent } }` | 全部 5 个计数 | 成功 toast（**必须把 5 个数字都报出来**，现状只报部分） |

字段 → 展示位映射：

| 展示位 | 字段 | 类型 | 规则 |
|---|---|---|---|
| `.dot` 背景 | `source` | `string` | **`sourceHue(source)`**，逐来源映射见 §4.1（唯一取色处；**禁止 `--signal`**，V1） |
| `.tag` 文本 | `SOURCE_LABEL[source] ?? source` | — | `claude-code`→"Claude Code"、`dsh`→"dsh"、`ruagent`→"ruagent"；缺项回退原始 `source`（`opencode` / `codex` 现状即回退，见 §4.1 表末注） |
| `.title` | `title` | `string \| null` | 空 → `sessions.untitled` |
| `.muted`(12) | `message_count` | `number` | "N 条" |
| `.tag`（第二个） | `agent` | `string \| undefined` | 仅 ruagent 会话有 |
| `.muted.mono`(11) | `project` → `shortProject()` | `string \| null` | 截断，"—" 当空 |
| `.time` | `updated_at` / `started_at` | `number`（epoch ms） | 行用 `updated_at`，浮层用两者 |
| 浮层标题 | `title ?? key` | — | — |
| 浮层正文 | `role`（`user`→`.chat-msg.user`，其余→`.chat-msg.agent`） | — | `text` 走 `Markdown` |

### 4.1 来源点取色（`sourceHue()` 终稿，MASTER §3.1 I）

**裁决**：会话来源标记（域③）**并入**运行时品牌域（域②），**不新开色族**（MASTER §3.1 I 的合并裁决；primitives §7 R6 同结论，**零新增 hex**）。
逐来源映射（可直接对 `panel/src/views/Sessions.tsx:17` 的 `sourceHue` 逐行核）：

| `source` | `sourceHue()` 返回 | 暗色解析值 | 亮色解析值 | 依据 |
|---|---|---|---|---|
| `claude-code` | `var(--brand-claude)` | `#d97757`（H=14.8°） | `#d97757` | 域② 官方品牌色（MASTER §3.1 G1）；与信号琥珀（H≈36°）不同族 |
| `opencode` | `var(--brand-opencode)` | `#e8e8ea` | `#121212` | 域②（G3）；**必须双模式**（primitives §7 R6） |
| `deepseek` | `var(--brand-deepseek)` | `#5786fe` | `#5786fe` | 域②（G2） |
| `dsh` | `var(--ant-color-text-tertiary)` | 中性档 | 中性档 | 无品牌色 → **中性**（§3.1 I 域③明文列举） |
| `ruagent` | `var(--ant-color-text-tertiary)` | 中性档 | 中性档 | 同上 |
| `codex` | `var(--ant-color-text-tertiary)` | 中性档 | 中性档 | 同上（§3.1 I 域③列举） |
| 未知 / 空串 | `var(--ant-color-text-tertiary)` | 中性档 | 中性档 | `default` 分支 |

> 「中性档」= `--ant-color-text-tertiary`。**实测（2026-09-21，在 `.row-btn` 内的真实元素上探针取值）**：暗 `#8e939c` / 亮 `#5f646d`（亮值即 primitives §10.2 X3 落地的结果）。规格仍**不复制 hex**——值随 `theme.tsx` 的 `D/L.textTertiary` 走，写死数字会立刻过期。

**终稿签名**（`Sessions.tsx` 导出，`Home.tsx:12` **已经** `import { SOURCE_LABEL, msToIso, sourceHue }`——两页共用这一个函数，**不得各写一份**）：

```ts
/** 会话来源点取色。终稿：只用域②品牌色与中性档（MASTER §3.1 I 域③→②，零新增 hex）。 */
export function sourceHue(s: string): string {
  switch (s) {
    case "claude-code": return "var(--brand-claude)";
    case "opencode":    return "var(--brand-opencode)";
    case "deepseek":    return "var(--brand-deepseek)";
    default:            return "var(--ant-color-text-tertiary)";
  }
}
```

**约束（机器可判定）**

1. 返回值集合恒为 `{var(--brand-claude), var(--brand-opencode), var(--brand-deepseek), var(--ant-color-text-tertiary)}`——**不含** `--signal` / `--status-*` / `--graph-*` / `--ws-*`（§3.1 I 硬规则 1/2/3/4），也不含任何来源专用新色族。
2. 函数名与签名 `(s: string) => string` **不变**（`Home.tsx` / `SessionDetail` 已 import；改签名会静默落到 `default` 分支）。
3. 品牌色只承担「厂商身份」，**不承担状态**（§3.1 I 硬规则 3）；来源身份另由旁边的 `.tag` 文本承担。
4. 判定命令：`grep -nE 'var\(--(signal|status-|graph-|ws-)' panel/src/views/Sessions.tsx` 在 `sourceHue` 函数体内必须**不命中**。
5. `SOURCE_LABEL` 只覆盖 3 个来源，`opencode` / `codex` 现状回退成原始 `source` 串（`Segmented` 实测显示为 `opencode`）。**这是标签问题，不是取色问题**：取色由本表 `default` 覆盖，标签是否补齐不影响行 3 / 行 32 的判定。
6. **终稿取色对琥珀判据的复算**（用 §8「S3 计数口径」里同一条判据 `H ∈ [20°, 50°]` ∧ `max−min > 25` ∧ `max > 110`）：
   * `#d97757` → H = 14.8° ✗
   * `#e8e8ea` / `#121212` → `max−min` = 2 / 0 ✗
   * `#5786fe` → H = 216.5° ✗
   * `#8e939c`（暗）/ `#5f646d`（亮）→ `max−min` = 14 / 14 ✗
   → **终稿下 `#sessions` 的来源点琥珀数 = 0**（现状 131 → 0），S3 的 138 因此只剩导航项那 7 个。


**分组**：单层（无分组），**时间倒序**。过滤 = 单一维度（`source`），由 `Segmented` 表达。
**密度上限**（这是全站最高密度的一页，必须硬性设限）：

| 项 | 实测 | 上限 | 手段 |
|---|---|---|---|
| 行数 | **200** | **200** | 超出**强制虚拟化**（`content-visibility: auto` 或窗口切片）。200 行 = 8432px 已到可接受边界 |
| 页高 | **8724px** | **≤9000px** | 同上；虚拟化后回落到 ~900px |
| 节点数 | **2607** | **≤1200**（虚拟化后） | 现状 2607 是 200 行 × ~13 节点 |
| 字符数 | 23763 | ≤25000 | `preview` **不渲染**（现状已不渲染 ✔，别加回来） |
| 浮层消息 | — | **≤400 条**，超出只渲染最近 400 + 顶部提示 | 与 chat 同策略 |
| `.tag` 数 | 358 | ≤3×行数 | — |

### 4.2 ≤520 的 IA 取舍与「已知接受的限制」（2026-09-23 裁决，t72 落地）

**裁决**：窄屏（≤520）**保留「标题 + 时间」，收起路径 tag**（同时收起 source tag 与状态 tag）。 **⚠️ 2026-09-23 更新（与 §12 行 46 相容，t94 修）**：上面的「保留『标题 + 时间』」**必须让位于标题的可读性下限** —— **优先级 = 标题 > 时间 > agent 标签**（t89 已定）⇒ **窄屏保留标题；时间在空间不足时截断 / 让位**（**不得继续压标题**）。**依据（出厂构建实测）**：`#sessions` 的标题 `390 = **40.98px**、`768 = **59.25px**（fs 14 ⇒ 约 **3 个字符**）⇒ **那不是可读的标题**，与 `#knowledge` 的 **0px** **同根因**（`.title { flex:1; min-width:0 }` 的塌陷），**且正是用户抱怨的形状**（「根本就看不到 session 的文字」）⇒ **判为真缺陷**（不是「窄屏可接受的密集布局」）✓

**算术依据（390 实测）**：行内容盒只有 **230px**；`.row-btn` 的 `gap` 是 **10px**，每多一个 item 再吃掉 10px；
「source tag + 标题 + meta + 2 动作」= 5 个 item 时标题只剩 **64px**（需 70）⇒ 窄屏只留「标题 + meta + 动作」。

**改后实测（t72）**

| 视口 | 标题宽（需 70） | count | 时间 | tag | 行高 | 溢出 |
|---|---|---|---|---|---|---|
| **390** | `3/70 ✗` → **`75/75 ✔`** | 0 → 20 | 0 → **49** | 2 → **0** | 仍 **40** | 0 |
| **520** | `61/70 ✗` → **`205/205 ✔`** | — | — | — | — | 0 |
| 768 / 1024 / 1440 | **逐项不变** | — | — | — | — | 0 |

**官方记录（ui-list 的措辞，captain 要求原样落进契约）**

> ≤520：行内只保留标题 + 元数据（条数 + 时间）+ 行级动作；路径 / source / 状态 tag 收起。390 档为算术下限：3 动作行（ruagent 源）的标题最多约 41px，超长标题（>150px）必然省略 —— 实测 390 挂载 14 行中 **2 行被裁**（`ruagent:5bebb5681b81b04f` **41/70**；`dsh:5d5b5e26a218b26a` **75/180**）。**这是被接受的决定，不是缺陷。**

**逐行证据（390，挂载 14 行，被裁 2 行）**

| # | 类型 | key | source / title | 标题 cw/sw | 差额 | 原因 |
|---|---|---|---|---|---|---|
| 1 | **3 动作行的算术硬限** | `ruagent:5bebb5681b81b04f` | source=ruagent，title=null → 渲染「未命名会话」，2 条消息 | **41 / 70** | −29px | 该行有 **3 个动作按钮（含删除）** |
| 2 | **极长标题** | `dsh:5d5b5e26a218b26a` | source=dsh，title=「[retry context — the previous…」，2 条消息 | **75 / 180** | −105px | 标题超长 |

算式：`标题 70 + count 20 + 时间 49 + 3×24 动作 + 4×10 gap = 236 > 230`（390 行内容盒 **230px**，`.row-btn` gap **10px**）。
**520 / 768 / 1024 / 1440：0 行被裁** ⇒ 这条限制**只存在于 390 这一档**。

**配套裁决：`.icon-btn-lg` 只在 >520 启用，≤520 保持 24px**（captain，2026-09-23）。
理由：若 390 也用 32px，**3 动作行的标题会从 41 → 17px** —— 等于用行 18 的收益换来更严重的窄屏裁切。**审计的 26 captures 在 1440 跑 ⇒ 行 18 判的就是 >520 那一档**（本页在 ≤520 的目标因此不参与行 18）。

**行 18 的视口分流是刻意的（明文，2026-09-23 captain 裁决；ui-list 提出）**

> `>520` 行级动作 **32×32**（守行 18 的命中区）；`≤520` **24×24**（守标题宽度），换取标题宽度。

| 视口档 | 行级动作命中区 | 各自守什么 | 为什么 |
|---|---|---|---|
| **>520**（含审计的判定档 **1440**） | **32×32** | **行 18 的 <32px 命中区目标** | 审计的 26 captures 全在 1440 ⇒ 行 18 判的就是这一档 |
| **≤520** | **24×24** | **标题宽度**（过行 18 的 **24px 地板**，但**不追 32 目标**） | 若也用 32px，390 的 3 动作行标题会从 **41 → 17px** ⇒ 用行 18 的收益换更严重的窄屏裁切 |

**当前读数（1440）**：`#sessions` 行 18 = `地板<24px 0 · 内容区<32px 0/≤10 ✓`（常显改动期间一度是 **38/39** ⇒ 已由 **t74** 的视口分流修正）。
**若将来把行 18 扩到 ≤520 档，应预期什么**：会看到 **29–33 个 <32px 元素** —— **那是本节的刻意分流，不是回归**。验收该档时应判 **PASS**（**地板 0 才是硬线**），并**不要**把 32×32 目标套到 ≤520。

**三条出路的记录**：**(a) 接受省略（裁）** · (b) 窄屏把蒸馏收进别处（**会丢功能**）· (c) 共享层零 gap 簇类（**覆盖不了长标题**，且零 gap 也只到 67，仍差 3px）。
⇒ **验收口径**：**390 下被裁行数 ≤2 ⇒ 判 PASS**；**超过 2 行才是新缺陷**。
**⚠️ 判据不得写成「这两行必须被裁」**：上表两行的 key（`ruagent:5bebb5681b81b04f` / `dsh:5d5b5e26a218b26a`）是**用户会话数据，不是固定夹具** —— 换一个数据库就会漂移。**判据是「被裁行数 ≤2」这个计数，不是「哪两行」**（同理：上表的 cw/sw 数字是 2026-09-23 那次实测的快照，只作算术佐证，不作逐行断言）。

## 5 四种状态

| 态 | 契约 | 现状 |
|---|---|---|
| **loading** | `sessions === null` → `Spinner`「会话…」（`Sessions.tsx:64`）。**规格追加**：>200ms 未返回时先渲染 `.card` 骨架（10 条灰行），避免空屏 | `Spinner` |
| **empty** | ①源为空 → `Empty icon="chat"` + `sessions.empty` + `sessions.emptyHint`（**不显示 `.card`**）②过滤后为空 → 同一 `Empty`，但 hint 改为「该来源没有会话」（现状用的是同一句，语义错） | 部分违反（过滤空态文案错） |
| **error** | `load()` 的 `catch` 现状**兜成空数组**（`Sessions.tsx` 的 `load`）→ 页面显示"没有会话"。规格要求：①`role=alert` 常驻块「会话索引加载失败 · 重试」②**保留上一次成功的数据**并加一行"数据可能过期 · N 秒前"（本页 10s 轮询，静默失败会让用户看到假空表） | **违反**（MASTER 行 20 同源） |
| **密集** | 见 §4 密度上限。200 行时的额外要求：①行级动作**常显**（**2026-09-23 起**；原「只在 hover/focus-within 显形」已废止 —— 可发现性优先，见 primitives §3.3 C10）②行高固定 42.16px，**不得因内容不等高**（`.title` 单行省略、`.tag` `max-width:40%`）③`.time` 用 `tabular-nums` 右对齐避免抖动 | 已达标 |

## 6 响应式行为

| 视口 | 侧栏 | content | 行内让位 | 行为 |
|---|---|---|---|---|
| **390** | 72 | 318（294） | **按 §4.2 的 IA 裁决**：收起路径/来源/状态 tag，保留「标题 + 时间」；**共享层 ≤520 规则（`index.css:615-618`，原文引用 `:448` 已过时）在本页已不适用** —— 见下方「≤520 的验收口径」 | `.readout-strip` 高 **150px**（2 格换行）；行高 t72 实测 **40**（§S11 的 42.16px 是 200 行满数据基线，**两者数据基线不同，验收时须固定基线**）；pageH 8824 |
| **768** | 72 | 696（662） | 无 | `.card` 662；pageH 8752；无溢出 |
| **1280** | 228 | 1052（996） | 无 | `.card` 996；pageH 8721 |
| **1440** | 228 | 1212（1149） | 无 | `.card` 1149；pageH 8724（**全站最高**） |

横向溢出：**0**（四视口实测全 0）。`#sessions` 的 8 个"越界"元素都在 `overflow:hidden` 的截断容器内（上一轮审计结论，本轮复测一致：`scrollWidth - innerWidth = 0`）。

**≤520 的验收口径（2026-09-23，必读）**：共享层的 `index.css:615-618`（`.row-btn > .muted, .row-btn .time { display: none }`）**保留**（仍服务 knowledge/wiki 等其它列表），但**在 `#sessions` 的 ≤520 下已成死规则** —— ui-list 按 §4.2 的裁决用**嵌套的中性工具类**重绘了 count 与时间，而 `>` **只匹配直接子元素**，包一层即不再命中。因此：

- **≤520 时 `#sessions` 不渲染 `.time` 与 `.tag`**（t72 实测 `.time` = 0 个）；**任何按 `.time` 找窄屏时间的判据都会找不到对象** —— 那是**选择器过时**，不是 FAIL。
- 窄屏时间的选择器是 **`.row-btn > .row.tight > .muted.micro:nth-child(2)`**（count 是 `:nth-child(1)`）；**390 / 520 实测 cw = 20 / 49**。
- 该共享层规则**保留是正确的**：它在 `#knowledge` / `#wiki` 等其它列表**仍然生效**，只有 `#sessions` 因为改用嵌套的中性工具类而不再命中。
- systems 会同步更新那条 CSS 注释；**本节是契约侧的唯一出处**。


## 7 复用的共享原语与类名

`.view-bar` `.readout-strip` `.readout` `.readout-label` `.card` `.row-btn`（**8px → 10px**，primitives §9.2）``.row-action``（**已删**，2026-09-23：门控类随最后一个使用者移除，改用 `.icon-btn`） · `.icon-btn-lg`（**仅 >520**：≤520 保持 24px，否则 3 动作行标题 41 → 17px —— 见 §4.2） `.tag` `.dot` `.title` `.muted` `.mono` `.time` `.grow` `.truncated`
**本页不引用任何已删除原语**：primitives §10.4 的 11 条删除清单见 [`README.md`](./README.md) §3.4 X6 / §3.5 判定表；本行清单原先含一个已删除的读数类，现已移出，读数一律走 `.readout*` / `.stat-num`。
浮层：`.modal-backdrop` `.modal` `.modal.wide` `.session-viewer` `.modal-head` `.modal-body` `.session-log` `.chat-msg` `.user` `.agent` `.md`
组件：`Spinner` `Empty` `ReadoutStrip` `Modal` `Markdown` `RelTime` `useToast` `Segmented`（antd）
**本页请求的 zone 化（Z1）**：过滤行（`Segmented` + 计数）应移入 `.view-bar` 保持冻结；列表本身是 **panel**（`.card`），**不是 zone**——因为每一行是可点击实体。
新增类请求：`.sr-only`（隐藏 `h1`）、`.micro`（**11/15**；替代 `className="muted" style={{fontSize:11}}`，本页 **314 处**，行 11 的主要来源）、`.error-state` + `.spinner-block` + `.icon-btn`。**本页不新增数据文本类**（primitives §7 R3 已拒绝该请求）——`.mono` 本就是 12/16，带内联 12px 的 `.mono` 只需删内联。

## 8 本页特有的验收数字

| ID | 指标 | 基线（实测） | 目标 | 判定（MASTER 行） |
|---|---|---|---|---|
| S1 | 单亮度带占比 | **13.7%**（全站最低=最平坦） | ≤75% ✔；**下界**：不得降到 <8%（会变成一片同色死灰） | 行 1、2 |
| S2 | 信号色像素占比 | **0.94%**（全站最高）；131 个来源琥珀点 = 107 `dsh`（`--signal`）+ 24 `Claude Code`（`--status-warn`，H=23.7° 仍落在琥珀带） | **≤1.5%（行 3 硬阈值不变）**；`sourceHue()` 终稿（§4.1）下**来源点琥珀数 = 0**（复算见 §4.1 约束 6），像素占比按实现后重跑 | **行 3 + §3.2 黑名单 V1** |
| S3 | 琥珀声明节点数 | **138** = 131 来源点 + **7**（当前导航项子树）；逐项构成见下方「S3 计数口径与 7 的构成」 | **≤7**，且这 7 个必须**逐个**对上下表 | 行 3、32（行 32 的检查集见 MASTER §12.3） |
| S4 | 内联 style 节点数 | **731**（全站最高） | **≤50**；内联 `font-size` = 0 | **行 12、11** |
| S5 | 无名称可交互元素 | **0**（上一轮 204 → 已修复） | **恒 0**（回归护栏） | **行 17** |
| S6 | <32px 命中目标（**1440 档**） | **0**（当前实测：地板 <24px **0** · 内容区 <32px **0**/≤10；**历史**：裁决时 4 → 行级动作常显期间 **38/39** → t74 视口分流后 **0**） | <24px = 0；<32px ≤10（**仅 >520 档**；≤520 档按 §4.2 的刻意分流判，地板 0 才是硬线） | 行 18 ✅ |
| S7 | 可见描边元素数 | **4**（侧栏边框 + brand + sidebar-foot + view-bar 分隔不计） | ≤40 | 行 5 ✅ |
| S8 | 描边内容容器 | **0** | ≤1 且必须是控件单元 | 行 4 ✅ |
| S9 | ≥18px 文本节点 / 最大字号 | **3 / 32px** | ≥3 / ≥32px | 行 6、7 ✅ |
| S10 | 行数 / 页高 / 节点数 | **200 / 8724px / 2607** | 200 / ≤9000px / ≤1200（虚拟化后） | 本页特有 |
| S11 | 单行高度一致性 | **42.16px**（200 行标准差 0） | 恒定 | 本页特有 |
| S12 | 浮层关闭后焦点 | 现状未验证；`Escape` 可关 | 焦点回到触发行的蒸馏按钮或行本身 | **行 22** |
| S13 | 浏览器后退 | 打开浮层后按后退会**离开页面**（浮层不是路由） | 规格接受现状，但要求 `Escape` 与点遮罩都能关，且 `aria-modal="true"` | 行 21 同源 |

### S3 计数口径与「≤7」的 7 个节点（可直接计数判定）

**口径（与 MASTER §3.2 的像素上限同一判据，只是作用在计算样式而非截图像素）**

> 琥珀声明节点 = `#sessions`（1440×900，`?mode=dark` 与 `?mode=light` 各跑一次）DOM 里，**可见**元素的下列任一**计算样式**命中琥珀判据的元素数。
> 属性集：`background-color` · `color` · `border-{top,right,bottom,left}-color` · `outline-color` · `fill` · `stroke` · `box-shadow` · `background-image`。
> 琥珀判据：`max(R,G,B) > 110` ∧ `max−min > 25` ∧ `(max−min)/max > 0.30` ∧ HSV 色相 `H ∈ [20°, 50°]`。
> 可见 = `getBoundingClientRect()` 的 `w,h ≥ 1` ∧ `visibility ≠ hidden` ∧ `display ≠ none`。

**138 的构成（实测 2026-09-21，200 行，暗/亮两模式结果相同）**

- **131** = `.row-btn > .tag > .dot` 的 `background-color`（**来源点，MASTER §3.2 黑名单 V1**）
  - 107 = `dsh` 行 → `sourceHue("dsh")` 现状 `var(--ant-color-primary)`，暗 `rgb(240,169,59)` / 亮 `rgb(200,132,19)` = `--signal`
  - 24 = `Claude Code` 行 → `sourceHue("claude-code")` 现状 `var(--ant-color-warning)`，暗 `rgb(226,137,79)` / 亮 `rgb(138,74,18)` = `--status-warn`（H=23.7°，仍在琥珀带内）
  - （余下 69 行 `ruagent` ×63 + `opencode` ×6 现状走 `--ant-color-success` 绿档，**不计入琥珀**，但同样是域外借用，见 §4.1）
- **7** = `.sider-nav` 的**当前导航项**（`#sessions` 自己）子树，全部命中 `color`（暗 `rgb(245,180,87)` / 亮 `rgb(122,76,0)`），落在 MASTER §3.2 **W1 白名单**内：

| # | 节点 | DOM 路径 | 命中属性 | 尺寸 |
|---|---|---|---|---|
| 1 | `li.ant-menu-item.ant-menu-item-selected`（文本「会话」） | `.app-sider .sider-nav > ul > li` | `color`（未设 border/outline 时其 computed 值 = `currentColor`，连带命中） | 207×36 |
| 2 | `svg.ant-menu-item-icon` | `li > svg` | `color`, `stroke` | 16×16 |
| 3 | `g`（该 svg 的组） | `li > svg > g` | `color`, `stroke` | 12×12 |
| 4 | `path` | `li > svg > g > path` | `color`, `stroke` | 12×12 |
| 5 | `path` | `li > svg > g > path` | `color`, `stroke` | 3×3 |
| 6 | `path` | `li > svg > g > path` | `color`, `stroke` | 3×5 |
| 7 | `span.ant-menu-title-content` | `li > span` | `color` | 141×36 |

**合计 7**，且**无一在 `#sessions` 的内容区**（1–7 全在侧栏导航）。

**判定**

- 修完 `sourceHue()`（§4.1 终稿）后重跑同一计数，**= 7 即通过**；`> 7` 说明来源点没清干净或新引入了琥珀；`< 7` 说明导航项被改动——**必须先改本表再改实现，不许静默变数**。
- 计数时**不得手工剔除** `currentColor` 连带命中的 `border-*-color` / `outline-color`（口径已包含，剔除后不可复现）。
- 亮色模式必须用应用自己的开关测（`http://127.0.0.1:8787/?mode=light#sessions`）。**只给 `<html data-mode="light">` 而不走 `?mode=light`，antd 的 JS token 仍是暗色**（导航项字色会停在暗色值），测出来是混合态——本表的亮色数字就是用 `?mode=light` 取的。


### S7 列表卫生：默认隐藏系统生成的来源（2026-09-23 新增；与 t90 同一条纪律）

**实测（captain，1440 暗色；199 条）**：**43 条临时工作区**（`C:\...\Temp\modlens-work-XXXX` —— 视觉桥的临时会话）· **28 条记忆蒸馏会话** ⇒ **这些不应出现在用户的默认视图里** ✗
**规则**：**默认视图排除系统生成的来源**（记忆蒸馏 · 临时工作区）· **保留一个显式入口让用户能看到它们**（与「归档默认隐藏、可显示」同族）✓
**现状**：该页已有 **source 过滤页签**（全部 / ruagent / dsh / Claude Code / opencode）但**没有「隐藏系统 / 临时来源」** ⇒ 需要补**一个开关或一个「系统来源」页签**（形态由实现定，**但默认必须是隐藏** ✓）
**可判定**：**默认进入该页时，列表里不得出现 `cwd` 属于临时目录（含 `Temp` / `tmp`）或来源为「记忆蒸馏」的行**；**且必须存在一个可见入口能显示它们**（存在性可判定：入口元素可见且可聚焦 ✓）
**为什么是「规则保证」而不是「人记得清理」**：**列表卫生是需求** —— 与 t90（不列出没有消息的会话）同一条纪律：**不靠人记得清理，靠规则保证** ✓（**这次的残留里有 captain 自己的 modlens 临时会话** ⇒ 正好证明「靠人记得」不成立 ✓）

### S8 存量注入名行的补救：三个选项（**需用户裁决**，2026-09-23）

**事实（t101 查明）**：本修复**只前向生效** —— 旧 transcript **没有 sentinel** ⇒ 索引器 `user_text()` 是 **no-op** ⇒ **旧行永久保留注入名**（**26 行**）· **即使重新索引也不会好** ✓ ⇒ **§12 行 50 会一直红，那是诚实的红** ✓

**⚠️ 这是用户的真实数据 ⇒ 不替他选** ⇒ 三个选项（影响 / 代价 / 可否撤销）：

| # | 选项 | 影响 | 代价 | 可否撤销 |
|---|---|---|---|---|
| **(a)** | **用删除端点删掉这些 `chats` / `sessions` 行** | 这些会话**从列表消失**（**内容仍在 transcript 里** ⇒ **删历史条目 ≠ 撤回内容** ✓） | 需逐条 / 批量操作；依赖删除端点可用（t95） | **不可撤销**（列表条目删后无法从 UI 恢复；**transcript 内容仍在** ✓） |
| **(b)** | **对旧 transcript 做一次性重写** | 注入名消失、内容保留 | **会改动用户的数据** ✗（改的是历史文件本身） | **不可撤销**（除非事先备份） |
| **(c)** | **什么都不做** | 26 行继续显示注入名 | **行 50 长期为红**（**诚实的红** ✓） | **可撤销**（随时可改选 (a) / (b)） |

**推荐顺序（design-lead 的建议，最终由用户定）**：**(a) > (c) > (b)** —— **(a)** 只动**列表条目**、不动内容，且**与 t95 的删除能力同源**；**(c)** 零风险但让判据长期红；**(b)** 唯一会**改动用户数据**的选项 ⇒ 只在用户明确接受时才做 ✓ **结论见 S9**：用户选了 (a)，但 **(a) 被产品规则挡住**（只有历史文件的拥有者能删 ⇒ 403 ✓）⇒ **实际采用 S9.4 的 (ii)+(i)**（归入「系统会话」+ 诚实占位名 ✓）✓

**(a) 的语义核实（已读代码，不是假设）**：
- **`sessions` 侧**：`0017_session_deletions` 墓碑**阻止索引器重新加回** ✓ —— `crates/store/src/lib.rs:695-698` 的注释原话：「Unlike a missing row these survive the indexer's next INSERT OR REPLACE, **which is what makes the delete stick (measured: without a tombstone the row returned in <70s)**」✓（写入在 `:781`，读取在 `:701`）
- **`chats` 侧：没有墓碑，而且不需要** ✓ —— `crates/daemon/src/chat.rs:825-834` 的原话：「**WHY NO TOMBSTONE (checked, not assumed)**：nothing rebuilds the `chats` table …… **There is no periodic re-index of `chats`, unlike `sessions`**」✓
- **⚠️ 但 `chats` 有一条例外**：**`resume` 一个已删除的 chat id 会重建该行** ✓（`chat.rs:834-836`：「A resume of a deleted id **DOES** re-create the row; that is a **deliberate user action**, not a background rebuild」）⇒ **删掉 chat 行之后，只要用户不再 resume 它，它就不会回来** ✓

### S9 无法修复的存量注入名行：显示规则与「为什么删不掉」（2026-09-23 落定）

#### S9.1 删除被产品规则挡住 —— **这是正确行为，不是缺陷**
**实测**：符合判据（`preview` / `title` 以平台注入块头开头）**37 / 608** 行；按来源 `{dsh: 37}`（**全部是别人的历史文件**）· 按可删性 `{False: 37}`（**全部不可删**）；全库可删性分布 `{False: 521, True: 87}`；**逐条 DELETE ⇒ 成功 0 / 失败 37（全部 HTTP 403）** ✓
**规则（`api.rs:1882` 原话）**：**「A session is only deletable by the tool that owns its history file.」** ✓（单测 `api.rs:3525-3526`：`ruagent:aa` → `deletable=true`、`claude-code:bb` → `false` ✓；路由侧 `api.rs:2009`：「removing it is refused with 403 and an explanation the UI」✓）
⇒ **ruagent 不该、也不会删 dsh / Claude Code 的记录** ✓ **请下一个读者不要把它当缺陷** ✓

#### S9.2 其中 36 行连 `preview` 也是脏的 ⇒ **无法从 transcript 恢复**
**实测**：**只有 `title` 脏、`preview` 已干净 = 1**（captain 的探针会话，t101 修复后的形态）· **`preview` 也还脏（旧行）= 36** ✓
**原因**（t101 已查明）：**旧 transcript 没有 sentinel** ⇒ `user_text()` 是 **no-op** ⇒ **重新索引也恢复不出用户的话** ✓ ⇒ **这 36 行无法被「修复」，只能换一种显示规则** ✓

#### S9.3 `title` 回退规则（**可修**，已派 tools）
**事实**：t101 的修复**覆盖 `preview` 但没覆盖 `title`** ✓ —— 证据：那 1 条探针会话 `preview='SENTINEL-PROBE-找一下 skill'`（干净 ✓）而 `title='[memory context — what the'`（脏 ✗）✓
**原因**：`title` 来自 harness 自己的 **`ai-title` / `session/title` 事件**（`sessions.rs:353` / `:409`）⇒ **harness 拿「第一条消息」（含注入块）当标题** ⇒ **sentinel 不在标题里 ⇒ 切分用不上** ✗
**规则**：**`title` 以我们自己的注入块头开头时丢弃它、回退到 `preview`** ✓
**对象集 = 我们自己的全部注入块头**，**定义成「生产者常量的那一个集合」**：`crates/daemon/src/chat.rs` 的 **`pub const INJECTED_HEADERS`** ✓ —— **生产者、索引器、本判据、面板分类器全部读同一个集合 ⇒ 不可能漂移** ✓（这是「**阈值是派生的，不可漂**」在**对象集**上的应用 ✓：凡「工具里有、契约里也有」的东西，应当**从一处派生** ✓）。**当前应有成员（4 条，含本轮新并入的 2 条）**：`[memory context` · `[role — you are` · `[conversation resume — you are` · `[retry context — the previous` ✓（**⚠️ 实测：该常量当前只声明了前 2 条**（`pub const INJECTED_HEADERS: [&str; 2]`）⇒ **由 tools 另单补齐** ✓；DOM 实测默认视图里仍有 1 行 `[conversation resume — you are` 与 2 行 `[retry context — the previous` ⇒ **列举法滞后于生产者，这就是「对象集比意图窄」** ✓）。**成员是「块头的可判别前缀」（短前缀），不是整段块正文** ✓ —— 例：`[memory context` ✓（**harness 会把首条消息截断成标题**，实测存的就是 `'[memory context — what the'` ✓）；**不要把整段块正文塞进常量** ✗（**正文里含变量** ⇒ 不能当字面量）✓ **约束（三条）**：① **只含「平台自己发出的」块头** —— **不得**把用户内容里可能出现的 `[` 形态收进去 ✗（**判据的对象应是平台产物的字面量，不是对用户内容的猜测** ✓）② **新增块头时必须同步更新该常量** ✓（**改产物让正确做法成为默认** ✓）③ **只判开头、不用子串包含** ✗（用户可能在自己的消息里**引用**这些字面量 ✓）

#### S9.4 那 36 行的显示规则（**design-lead 落定：主选 (ii)，并把 (i) 作为它在「显示时」的名字形态**）
- **采用 (ii) 归入 S7 的「系统会话」分类**（**默认隐藏、可显示**）—— **理由**：这些行的名字是**平台的提示词** ⇒ 它们**不属于「用户命名的会话」** ⇒ 与 S7 的蒸馏 / 临时工作区**同一处理**；**用户的默认视图保持干净**（这正是用户报的第一类问题）✓ 且**不删数据、不猜内容** ✓
- **并在「显示时」用 (i) 的诚实占位名**（形如 **「（含平台上下文的历史会话）· <日期>」**）—— **行仍可达** ✓ · **名字不冒充用户的话** ✓
- **可判定**：① 该行的名字**不得以该集合中任一成员开头**（**对象集 = `INJECTED_HEADERS`，与 §12 行 50 同一集合** ✓；**只判开头、不用子串** ✓）② 该行的名字**必须含日期** ③ **默认视图里不得出现该行**，**且必须存在一个可见入口能显示它**（可见 ∧ 可聚焦 ✓）
- **否掉 (iii) 的理由（两条）**：① **改变语义** —— 「名字 = **第一条**消息」被换成「最后一条用户消息」✗ ② **同一行的名字会随后续消息变化** ⇒ **列表不再稳定** ✗（与 §9.5 的 **L1 排序键唯一 / L6 身份稳定**冲突）✓
- **否掉 (iv)「直接隐藏不显示」**：用户会**找不到自己确实用过的会话**（它们的**内容仍在 transcript** 里 ✓）⇒ 必须保留**可显示入口** ✓

#### S9.5 给用户的说明（**照此转述即可**）
> 你选的「删掉这些行」**执行不了** —— 不是因为出错，而是因为**产品规则不允许**：**一条会话只有「拥有它历史文件的工具」能删**（`ruagent` 自己的能删，**dsh / Claude Code 的历史文件不能删**）。这 37 行**全部来自 dsh** ⇒ 逐条删除**全部返回 403**，这是**正确行为**（若我们真删了别人的记录，那是更严重的问题）。
> **实际会怎样**：这些行**不会出现在你的默认列表里**（归入「系统会话」分类，**默认隐藏**）；如果你想看，有一个**显式入口**可以显示它们；显示时它们的名字是**诚实的占位名**（「（含平台上下文的历史会话）· 日期」），**不会冒充你自己的话**；**点开仍能看到当时的真实消息**（内容一直在 transcript 里）。
> **为什么不能「修好」它们的名字**：那 36 行的旧记录里**没有我们后来才加的边界标记** ⇒ **连重新索引也恢复不出你当时说的话** ⇒ 只能**换一种显示规则**（上面的做法）✓ 新产生的会话**不会再出现这个问题** ✓
