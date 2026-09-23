# home — `#home`

> **路由**：`#home`（`parseHash()` `default` 分支；`#/` 也会落到这里）
> **视图**：`panel/src/views/Home.tsx` → `Home({ onOpenTask, onNav })`；外壳 `App.tsx`
> **消费的 MASTER 行**：§12 行 **1、2、3、6、7**（本页特有）+ README §2 的 20 行全局
> **消费的变更编号**：`C2`（信号去重）`C5`（.guide-icon 去信号色）`T5 T6`（行高/内联字号归一）`S1`（间距）`R1`（圆角）`A1 A2`（焦点/按下）`Y1`（sr-only h1）`Z1 Z2`（zone + panel 角色）`B1`
> **本页不涉及**：行 25–28（性能）、行 20（错误态由全局横幅承担，见 §5）

## 1 职责与首要任务

**职责**：把"平台现在怎么样"和"我该做什么"压缩成一屏，并把用户推向他真正要去的那个视图。
**首要任务**：**一眼确认平台健在 + 进入最近的那次会话/那个任务**。90% 的进入是为了这两件事之一。

据此决定：
- 打开的默认落点，不做任何"欢迎动画"；`.home-hero` 只有一句 `h1` + 一句说明 + 两个主操作。
- 「最近会话」是 `.dash-main`（左、宽、第一眼），「平台状态/记忆/待办」是 `.dash-side`（右、窄）。
- 全部数字可点：读数即入口（`ReadoutStrip` 的 `onOpen`）。

## 2 版式骨架

```
1440 视口：侧栏 228 | content 1212（可用 1149）                    768 视口：侧栏 72 | content 696（可用 662）
┌─ .home.dash ────────────────────────────────────────┐            ┌────────────── 662 ──────────────┐
│ .home-hero.dash-hero                              │            │ .home-hero (h1 24/32)           │
│   h1「欢迎回来」 24/32 w650   p.muted 14/22         │            │   h1 / p / [新对话][新任务]      │
│   [新对话] 主操作(primary)  [新任务]                 │            ├───────── .dash-grid ────────────┤
├─ .readout-strip (panel, 1149×99, 4 格) ─────────────┤            │ .dash-main 662×368             │
│  7智能体 │ 21任务 │ 22运行 │ 124记忆条目             │            │   .dash-head 最近会话 200 查看全部│
│  readout 32/40 mono · 分隔用 -1px 规则线不是盒子     │            │   .row-btn ×8                  │
├─ .dash-grid  grid[729, 405] gap 0 1 auto ───────────┤            ├─────────────────────────────────┤
│ .dash-main 729×368          │ .dash-side 405×555    │            │ .dash-side 662×555             │
│  .dash-head 最近会话 200    │  Card 平台状态          │            │   Card 平台状态                 │
│  查看全部 →                 │    ● daemon / ● embedder│            │   Card 记忆（4 格 + 3 预览）    │
│  .row-btn ×8（最近 8 条）   │  Card 记忆（4 格+3 预览）│            │   Card 待办（inbox 计数）       │
│   .tag>.dot 来源 .title     │  Card 待办（inbox 计数）│            └─────────────────────────────────┘
│   .muted mono 条数 .time    │                          │
└──────────────────────────────┴───────────────────────┘            390 视口：侧栏 72 | content 318（可用 294）
                                                                    .dash-grid 单列 294，pageH 1495
                                                                    .readout-strip 换行成 300px 高（4×75）
```

**栏宽（实测）**：`.dash-grid` `grid[729, 405]` @1440 · `[631, 351]` @1280 · `[662]` @768/@390（`@media (max-width:1100px)` 单列）。
`.readout-strip` 高 99px（@1440/@1280）、99px（@768）、**300px**（@390，4 格折成 2×2 再折 4 行）。
`.content` 外框 1212 / 1052 / 696 / 318。

**断点行为**
- `>1100`：`.dash-grid` 双列。
- `≤1100`：单列，`.dash-side` 落到 `.dash-main` 之下（**待办与平台状态不能被压到首屏之外，见 §6**）。
- `≤992`：侧栏折到 72。
- `≤520`：`.row` 里的次要元数据让步（`.tag` 已有 `max-width:40%` 规则）。

## 3 层级

**本页最大元素 = `.readout`（32/40，IBM Plex Mono，weight 500）**，实测 4 个节点、`maxFs=32`。
为什么是它：首页没有"主内容"——它是一块**仪表盘**。仪表盘的本职就是把 4 个平台级计数做大，用户扫一眼就知道平台有没有在动。层级来自字号跳档（13 → 32，跨 3 级），不来自字重。
次级：`.home-hero h1` 24/32 w650（display 档，实测 `maxFs` 第二档）。
第三级：`.dash-head` 13/18 w600（section 档的轻量变体，只是标签不是标题）。

**禁止**：在首页再引入 18/20/24px 的第三、第四档字号；也禁止把 `.stat-strip .ant-statistic-content-value`（30/38，MASTER §4.4 T5 判定为越界档）当作第二套读数——本页已改用 `ReadoutStrip`。

## 4 内容模型

一次并发加载 7 个端点，之后 **10s 轮询**（`Home.tsx:86`）；全部 `.catch()` 兜底成空值（→ §5 的 error 契约）。

| # | 调用 | 返回类型（`api.ts` 真实字段） | 展示位 |
|---|---|---|---|
| 1 | `api.agents()` | `AgentInfo[]` | 读数①「智能体」= `length` |
| 2 | `api.tasks()` | `Task[]` | 读数②「任务」= `length`；待办卡的空判断 |
| 3 | `api.stats()` | `AgentStats[]` | 读数③「运行」= `Σ runs` |
| 4 | `api.memoryList("observation","user")` | `{ memories: MemoryRow[]; counts: [string,string,number][] }` | 读数④「记忆条目」= `Σ counts[2]`；`.mem-cells` 4 格 = 各 store 计数；`.mem-preview` 取 `memories` **最新 3 条**，每张显示 `MemoryRow.content` + `updated_at` |
| 5 | `api.sessions()` | `SessionRecord[]` | `.dash-head` 的计数 = `length`；`.dash-main` 列表取按 `updated_at` 倒序 **前 8 条** |
| 6 | `api.knowledgeDocs()` | `{ documents: KnowledgeDocument[]; embedder: string }` | 平台状态第 2 行：`.mono.truncated` = `embedder` |
| 7 | ~~`api.pendingPermissions()`~~ → **外壳 prop `inbox: number \| null`**（**t50 后本页不再调该 API**） | ~~`PendingPermission[]`~~ → `number \| null` | 待办卡：**计数由外壳单方拥有**（`App.tsx` 的 2s 轮询器，仅在 `#inbox` 让渡）；Home 只**消费 prop** —— `> 0` → `.nav-badge` 计数 + 可点；`= 0` → `home.todo.empty`；`null` → 徽标显示「—」。原「Home 自己调 `pendingPermissions()`」是行 28 的重复轮询来源（§12.10.1 / t50） |

`.dash-main` 每行（`.row-btn`）的字段映射：

| 展示位 | 字段 | 类型 | 说明 |
|---|---|---|---|
| `.tag > .dot` 背景 | `SessionRecord.source` | `string`（`claude-code` \| `dsh` \| `ruagent` \| `opencode` \| `codex`） | **按 MASTER §3.1 I 域③（已合并进域②）取色**：`claude-code` / `opencode` / `deepseek` → `var(--brand-*)`；`dsh` / `ruagent` / `codex` / 未知 → `var(--ant-color-text-tertiary)`（中性）。取色表与 `sourceHue()` 终稿签名见 [`view-sessions.md`](./view-sessions.md) §4.1（两页共用同一个函数，不得各写一份）。**禁止**：`--signal` / `--status-*`（§3.2 V1、§3.1 I 硬规则 4）、`--graph-*` / `--ws-*`（跨域借用，硬规则 1/2）、以及新开任何来源专用色族（§3.1 I 明确拒绝，零新增 hex） |
| `.tag` 文本 | `SOURCE_LABEL[source] ?? source` | — | 来源标签 |
| `.title` | `SessionRecord.title` | `string \| null` | 空则回退「未命名会话」 |
| `.muted.mono` | `SessionRecord.message_count` | `number` | 条数 |
| `.time` | `SessionRecord.updated_at` | `number`（epoch ms） | 经 `RelTime` |

**分组**：无分组（扁平 8 条）。平台状态卡 2 行；记忆卡 = 4 格 + 3 预览 + 1 行小节标题；待办卡 1 行。
**密度上限**：`.dash-main` **≤8 行**（超出以「查看全部 →」跳 `#sessions`，不得内联分页）；`.mem-preview` **≤3**；
整页节点 **≤400**、高度 **≤1100px**（实测 245 节点 / 900px，余量充足）；文本 **≤2000 字符**（实测 1378）。

## 5 四种状态

| 态 | 契约 | 现状 |
|---|---|---|
| **loading** | `dash === null` → `Spinner`（`Home.tsx:90`）。**并发 7 个请求，任意一个不返回就整页 spinner**：规格要求 7 个请求各自 `catch` 成空，首屏在**第一个**返回后即可渲染（骨架：读数位显示 `—`，不是 0） | 现状是 `Promise.all` 全等，慢端点（`memoryList`）拖住首屏 |
| **empty** | 分三类：①无会话 → `.dash-main` 内 `home.memory.empty`（现文案）②无记忆 → `home.memory.empty` ③无待办 → `home.todo.empty`。**空态不得整页替换**，只替换那一块 | 已分区 |
| **error** | 全局：`App.tsx` 的离线横幅（`.offline-banner`，`role=alert`）；本页：**每个读数位显示 `—` + 卡片内一行「无法加载 · 重试」**（用 `ErrorState` → `.error-state`，primitives §3.5；不新增视图私有类）。**禁止把失败渲染成 0**（MASTER §12 行 20 的同源要求：0 与"取不到"必须可区分） | **现状违反**：7 处 `catch` 全部兜成空值 → 页面显示 `0 智能体 / 0 任务`，用户会以为真的没有 |
| **密集** | `SessionRecord` 200 条时本页**只渲染 8 条**（已达标）；`MemoryRow` 124 条时只渲染 3 条预览（已达标）。真正的密度风险是 `.mem-preview .mp-text` 的长正文：**单条预览 ≤2 行**，用 `.mp-text` 的既有 clamp，超出不撑高卡片 | 待确认（见 §8 行 F3） |

## 6 响应式行为

| 视口 | 侧栏 | content | 行为（实测） |
|---|---|---|---|
| **390** | 72 | 318（可用 294） | `.dash-grid` 单列；`.readout-strip` 高 **300px**（4 格换行）→ 规格要求 ≤520 启用 `.readout-strip.grid`（2 列 ×2 行，**高 164px** = 2×66px 单元 + 上下 padding 32px；2026-09-21 由 `~150px` 估值改为实测值），避免首屏 1/3 被仪表吃掉；**视图不自己写这段 CSS**（README §3.4 X4）；pageH 1495。**前置条件（2026-09-21 补）**：2×2 需要**内容盒 ≥280px**（= 2×`minmax(140px,1fr)`，primitives §3.2 B7）→ **视口 ≥ 约 416px**；390 视口下条宽 294 − 条 padding 16px 20px = 内容盒 **254px < 280px**，故 390 仍是 4 行换行。B7 的 `auto-fit` 公式仍是 **>520 的默认行为**（不要改它），≤520 的 2 轨特化由 `t18` 在共享层落地 |
| **768** | 72 | 696（可用 662） | `.dash-grid` 单列（≤1100 触发）；`.dash-side` 在 `.dash-main` **之后** —— 规格要求 ≤1100 时把「待办」提到 `.dash-side` 的**第一位**（它是行动召唤，不能沉到 1200px 之下）；pageH 1292 |
| **1280** | 228 | 1052（可用 996） | `.dash-grid` `[631, 351]`；无横向溢出 |
| **1440** | 228 | 1212（可用 1149） | `.dash-grid` `[729, 405]`；`.readout-strip` 一行 4 格 |

横向溢出：**0**（实测 390/768/1280/1440 全部 0，MASTER §12 行 19）。

## 7 复用的共享原语与类名

`.home` `.dash-hero` `.home-hero`（冻结：`.home-hero h1`）`.dash-grid`（冻结）`.dash-main` `.dash-side` `.dash-head` `.status-row` `.mem-cells` `.stat-cell` `.stat-num` `.mem-sub` `.mem-preview` `.mp-text` `.row-btn` `.tag` `.dot` `.title` `.muted` `.mono` `.time` `.grow` `.nav-badge` `.guide-grid`（冻结）`.guide-card` `.guide-icon` `.readout-strip` `.readout` `.readout-label` `.card` `.panel` `.empty-state`（经由 `Empty`）
组件：`ReadoutStrip`（`ui.tsx`）`Spinner` `Empty` `RelTime` `StatusDot` `Modal` `useToast`
**本页请求的 zone 化（Z1/Z2）**：`.dash-main` 与 `.dash-side` 的 Card 组是"区带"而不是"实体"，规格要求用 `.zone` + `.zone-head` + `.zone-title`（`.zone-note` 放计数）承载「最近会话 / 平台状态 / 记忆 / 待办」四个区带标题，替换现在的 `.dash-head`。
新增类请求：`.micro`（**11/15**，替代内联 11px，与 `.muted` 叠用）、`.error-state` + `.spinner-block`。`.sr-only` **本页不需要**（已有可见 `h1`）；**本页不新增数据文本类**（primitives §7 R3 已拒绝该请求，它与 `.mono` 逐字相同），12px 数据文本用既有 `.mono`。

## 8 本页特有的验收数字

| ID | 指标 | 基线（实测） | 目标 | 判定（MASTER 行） |
|---|---|---|---|---|
| F1 | 单亮度带像素占比 | **37.9%** | ≤75% | 行 1 |
| F2 | 真实色度像素占比 | **3.08%** | 1.0–6.0% | 行 2 |
| F3 | 信号色像素占比 | **0.73%**（15 个声明节点：导航 1 + 主按钮 1 + 指示 13） | ≤1.5%；**清掉 `.guide-icon` 的 amber（V5）后 ≤1.2%** | 行 3 |
| F4 | ≥18px 可见文本节点 | **9**（4 读数 + 1 h1 + 4 其他） | ≥3 且 ≤40 | 行 6 |
| F5 | 最大字号 | **32px** | ≥32px（含仪表盘） | 行 7 |
| F6 | 描边内容容器 | **0** | ≤1 且必须是控件单元 | 行 4 |
| F7 | 节点数 / 页高 | 245 / 900px | ≤400 / ≤1100px | 行 4、5 的密度推论 |
| F8 | `.dash-main` 行数 | 8（200 条会话时） | **恒 ≤8** | 本页特有（行 6 之外的密度上限） |
| F9 | `.mem-preview` 条数 | 3 | 恒 ≤3，单元 ≤2 行 | 本页特有 |
| F10 | 错误态可区分 | `catch` 兜成 0 → **违反** | 7 个读数位失败显示 `—` + 「重试」，不显示 0 | 行 20（同源） |
| F11 | 内联 style 节点数 | 35（现状） | ≤50 全页、**内联 `font-size` = 0** | 行 11、12 |
| F12 | 来源点取色合规 | `--status-warn` 曾 `=== --signal`；来源点若继续用 warning 则额外违反 | 来源点改用 `var(--brand-*)` / `var(--ant-color-text-tertiary)`（**MASTER §3.1 I** 域③→②；表见 [`view-sessions.md`](./view-sessions.md) §4.1）。判定：`sourceHue()` 的返回值不含 `--signal` / `--status-*` / `--graph-*` / `--ws-*`，也不含任何来源专用新色族 | 行 32（+ §3.2 V1、§3.1 I、§12.3） |
