# 逐视图设计规格 — 索引与共享契约

> 本目录是 panel 13 条路由的**实现级设计规格**。总纲见 [`../MASTER.md`](../MASTER.md)；
> 本目录**不得与 MASTER 冲突**——凡发现冲突，改本目录，不许改 MASTER。
> 本轮不含代码改动；所有数字来自 DOM 计算样式直方图与截图像素统计（口径见 MASTER 附录 A）。

## 1 索引

| # | 路由（hash） | 规格 | 视图组件 | 标题（`.view-bar h2` / `h1`） | 数据规模（实测 1440） |
|---|---|---|---|---|---|
| 1 | `#home` | [view-home.md](./view-home.md) | `views/Home.tsx` | `h1`=欢迎回来（无 `.view-bar h2`） | 245 节点 / 900px 高 / 1378 字符 |
| 2 | `#chat` | [view-chat.md](./view-chat.md) | `views/Chat.tsx` | `h2`=对话 | 214 节点 / 900px / 212 字符（空会话） |
| 3 | `#sessions` | [view-sessions.md](./view-sessions.md) | `views/Sessions.tsx` | `h2`=会话 | 2607 节点 / **8724px** / 23763 字符 / 200 行 |
| 4 | `#board` | [view-board.md](./view-board.md) | `views/Board.tsx` | `h2`=任务 | 278 节点 / 2269px / 21 卡 × 4 列 |
| 5 | `#task/<id>` | [view-task-detail.md](./view-task-detail.md) | `views/TaskDetail.tsx` + `views/RunTimeline.tsx` | `h2`=任务标题 | **152329 节点 / 26342 事件行 / 490088 字符** |
| 6 | `#memory` | [view-memory.md](./view-memory.md) | `views/Memory.tsx` | `h2`=记忆 | 250 节点 / 1766px / 9 记忆卡 |
| 7 | `#knowledge` | [view-knowledge.md](./view-knowledge.md) | `views/Knowledge.tsx` + `views/Wiki.tsx` | `h2`=知识库 | 197 节点 / 900px / 4 文档 |
| 8 | `#graph` | [view-graph.md](./view-graph.md) | `views/Graph.tsx` | `h2`=实体图谱 | 137 节点 / 900px / 55 实体 + 100 事实 |
| 9 | `#agents` | [view-agents.md](./view-agents.md) | `views/Agents.tsx`（`Agents`） | `h2`=智能体 | 331 节点 / 1193px / 7 卡 + MCP 表 |
| 10 | `#runtimes` | [view-runtimes.md](./view-runtimes.md) | `views/Runtimes.tsx` | `h2`=运行时 | 237 节点 / 900px / 3 卡 |
| 11 | `#stats` | [view-stats.md](./view-stats.md) | `views/Agents.tsx`（`Stats`） | `h2`=统计 | 400 节点 / 1570px / 1 表 + 20 召回行 |
| 12 | `#settings` | [view-settings.md](./view-settings.md) | `views/Settings.tsx` | `h2`=设置 | 155 节点 / 900px / 1 卡（760px 宽） |
| 13 | `#inbox` | [view-inbox.md](./view-inbox.md) | `views/Agents.tsx`（`Inbox`） | `h2`=收件箱 | 126 节点 / 900px（空态） |

**路由解析提醒**：hash 无 `/` 前缀（`#board` 不是 `#/board`）；`parseHash()` 的 `default` 分支即 home。

## 2 全局验收行（每条路由都必须满足，各规格不再重复）

MASTER §12 中以下 20 行对**每一条路由**生效：

```
4  严格口径描边内容容器 ≤1 且必须是控件单元      13 暗色文本对比度失败数 = 0
5  全部可见描边元素数 ≤40/路由                   14 亮色文本对比度失败数 = 0
8  字号越界数 = 0                                15 焦点环对比度 ≥3.0 两模式
9  字重越界数 = 0                                16 无焦点环的 Tab 停留点 = 0
10 圆角越界元素数 = 0                            17 无名称可交互元素数 = 0
11 内联 font-size 元素数 = 0                     18 <24px 命中目标 = 0；<32px ≤10/路由
12 内联 style 元素数 ≤50/路由                    19 全部路由 × 全部视口横向溢出 = 0
                                                 24 每路由 h1 恰好 1 个
32 信号令牌唯一性 = 0 组重复（检查集 5 个：         35 CSS 断点 ≤4 + 契约值 992
   --signal / --status-warn / --brand-claude /
   --brand-deepseek / --brand-opencode；
   accent 显式排除，MASTER §12.3）
36 侧栏宽 228 / 72 不变                         37 间距越界值 = 0
38 `#chat` 的描边内容容器恒 ≤1 且必须是 `.composer`
```

各规格 §8 只列**本页特有**的行号，并引用 `MASTER.md` 的行号，不另起数字体系。

## 3 共享原语契约（规格只允许引用这里的名字）

### 3.1 已存在的类（`index.css` / `ui.tsx` 实测存在，13 路由必须在用）

| 角色 | 类 / 组件 | 用途 |
|---|---|---|
| 页面骨架 | `.content` `.view-bar`（内含 `h2` + `.muted` 副题 + `.grow` + 动作） | 每个视图统一开场 |
| panel（MASTER §7） | `.card` `.panel` | 默认包围面 |
| zone（MASTER §7） | `.zone` `.zone-head` `.zone-title` `.zone-note` | **当前 13 路由 0 引用**，本目录开始强制使用（Z1） |
| 行 / 卡片（实体） | `.icon-btn` `.row-btn` `.kanban-card` `.guide-card` `.agent-card` `.inbox-card` `.memory-card` `.mem-preview` `.neighbor` `.search-hit` `.recall-hit` `.recall-stub-head` `.diff-row` `.chunk-item` `.revision` `.cmdk-item` | 可点击实体 |
| 读数 | `ReadoutStrip` `.readout-strip` `.readout` `.readout.s` `.readout.l` `.readout.signal` `.readout-label` `.readout-cell` `.readout-btn` `.stat-num` `.stat-cell` `.mem-cells` | 大读数（P3） |
| 标记 | `.tag` `.tag.ok` `.tag.warn` `.tag.err` `.pill` `.dot` `.count` `.nav-badge` `.live-dot` `.live-flag` | 状态与计数 |
| 文本工具 | `.muted` `.mono` `.time` `.micro` `.title` `.truncated` `.grow` `.pad` `.sr-only` `.raw` `.intent` `.hint` `.md` | 文本（**`.nums` 已删除**：primitives §10.4 D1，0 引用） |
| 状态区块 | `Empty` → `.empty-state` `.empty-mark` `.empty-copy` `.empty-title` `.empty-hint` `.empty-action`；`Spinner` → `.spinner-block`；`ErrorState` → `.error-state`。**三态互斥**：error 存在时不得存在 `.ant-empty`（primitives §3.5；**`.state.empty` 已删除**） | 状态（P6 / 行 20） |
| 浮层 | `Modal` → `.modal-backdrop` `.modal` `.modal.wide` `.modal-head` `.modal-body`；`.cmdk\*` | overlay |
| chat | `.chat-layout` `.chat-side` `.chat-side-head` `.chat-side-list` `.chat-group` `.chat-group-head` `.chat-ws-title` `.ws-count` `.ws-new-btn` `.chat-wrap` `.chat-log` `.chat-msg` `.chat-empty-zone` `.chat-hero` `.chat-suggest` `.suggest-chip` `.composer` `.composer-row` `.composer-controls` `.ctl-select` `.ctl-sep` `.ws-chip` `.send-btn` `.chat-typing` `.chat-side-toggle` `.chat-side-backdrop` `.chat-rail-toggle` `.live-dot` | 见 view-chat.md |
| 时间线 | `.timeline-wrap` `.timeline-head` `.timeline` `.ev` `.ev-icon` `.ev.msg` `.ev.sys` `.ev.thought` `.ev.err` `.ev.inject` `.ev.perm` `.tool-head` `.tool-detail` `.tool-update` `.plan-body` `.plan-head` `.plan-item` `.plan-check` `.jump-latest` | 见 view-task-detail.md。**IO 内容容器是 `.tool-detail > pre.raw`**：原先写在清单里的那个 IO 容器类在 `index.css` 里从来没有规则（primitives §10.1 D6），已随 `.io-label` 一并从 13 份规格清除 |
| 图谱 | `.graph-layout` `.graph-main` `.graph-canvas` `.graph-hint` `.graph-detail` `.graph-detail-head` `.facts` | 见 view-graph.md |
| 记忆 | `.memory-list` `.memory-content` `.recall-expand` `.recall-inner` `.recall-body` `.recall-hit` `.recall-fact` `.recall-chunk-hl` `.stub-text` `.stub-afford` `.chev` `.recall-loading` | 见 view-memory.md |
| 知识 | `.search-bar` `.doc-icon` `.chunks` `.chunk-actions` `.chunk-revisions` `.rev-label` `.rev-old` `.md-editor` `.legacy-hint` `.wiki-view` `.wiki-broken` `.prompt-view` | 见 view-knowledge.md |
| 看板 | `.kanban` `.kanban-col` `.kanban-head` `.kanban-open` `.launcher` `.pipeline-builder` `.step-n` `.step-num` `.steps` | 见 view-board.md |
| 任务对比 | `.compare` `.compare-card` `.compare-card.selected` `.compare-result` | 见 view-task-detail.md |
| 首页 | `.home` `.home-hero` `.dash-hero` `.dash-grid` `.dash-main` `.dash-side` `.dash-head` `.status-row` `.mem-sub` `.mp-text` `.guide-grid` `.guide-icon` `.stat-strip` | 见 view-home.md |
| 运行时 | `.agent-grid` `.agent-avatar` `.agent-stats` `.runtime-logo` `.brand-claude` `.brand-deepseek` `.brand-opencode` | 见 view-runtimes.md。**来源点必须用变量形式** `var(--brand-claude)`（primitives §7 R6 把 3 个 hex 提升为变量，类保留为 `color: var(--brand-claude)`） |
| 设置 | `.distill-settings` `.chat-field` `.field-hint` `.field` | 见 view-settings.md |
| 骨架 | `.app-sider` `.sider-inner` `.sider-nav` `.sidebar-foot` `.brand` `.brand-mark` `.brand-name` `.build-id` `.conn` `.kbd-hint` `.offline-banner` | App 外壳（非视图职责） |

### 3.2 原语请求 → 终稿裁决（t16 对齐记录）

本目录 13 份规格**只引用下表中的终稿名，不另造名**。裁决出处一律是 `docs/design/primitives.md`（t3 定稿，1413 行）；
凡与我方请求不一致处，**以 primitives.md 为准**（本任务不得反向要求共享层改名）。

| # | 我方请求名（t2 提出） | 终稿名 / 形态 | 裁决 | 理由与出处 |
|---|---|---|---|---|
| R1 | `.sr-only` | `.sr-only` | **采纳（同名）** | primitives §7 R1 + §5.5。它是阈值行 24（每路由 1 个 `h1`）与冻结选择器 `.view-bar h2` **唯一不冲突的解法**；13 份规格全部采用（home 已有可见 `h1`，不重复加） |
| R2 | `.micro`（请求写 11/16） | `.micro`（**11/15**） | **采纳，取值被改正** | primitives §7 R2：CSS 里该档 **16 处是 11/15、仅 1 处 11/17**，且 `.time` 已是 11/15 —— 按 11/16 采纳会让 `.micro` 与 `.time` 变成两个只差行高的类。契约：**只管尺寸，不设 color**，与 `.muted` / `.time` 叠用 |
| R3 | `.data` | **不新增** → 用既有 `.mono` | **拒绝（同义原语）** | primitives §7 R3：`.mono` 现行定义就是 12/16 mono + tabular-nums，与 `.data` 的描述**逐字相同**。机械替换：`className="mono" style={{fontSize:12}}` → `className="mono"`（正是 MASTER T6 的「删冗余内联」） |
| R4 | `--focus-ring` | `--focus-ring` | **采纳（同名同值）** | primitives §7 R4 + §5.1：暗 `#f5b457` / 亮 `#8a5600`；实现走两条渲染路径（手写 `:where()` 零特异性 outline + antd pin `--ant-color-primary-border`），禁用裸 `outline:none` |
| R5 | `--control-border` | `--control-border` | **采纳（同名同值）** | primitives §7 R5 + §2.2：暗 `rgba(255, 255, 255, 0.34)`（对 panel 3.10:1）/ 亮 `#8a9198`（对白 3.19:1） |
| R6 | 复用 `--brand-*`（请求 0 个新 token） | 类保留 + **新增 3 个变量** `--brand-claude` / `--brand-deepseek` / `--brand-opencode` | **采纳，形态升级（0 个新 hex）** | primitives §7 R6：品牌色当时只有**类**，而 `.dot` 的底色走**内联 style**，引用不到类 → 把 3 个 hex 提升为变量，类改为 `color: var(--brand-claude)`。**规格里凡给标记取色写 `var(--brand-*)`**；`--brand-opencode` 必须双模式（暗 `#e8e8ea` / 亮 `#121212`） |
| R7 | 登记 `WS_HUES`（10 个硬编码 hex） | `--ws-1 … --ws-10`（暗 = 现行值，亮 = ×0.72 派生） | **部分受理：登记 + 模式化；并修正我方结论** | primitives §7.1 复算：我 t2 写的「亮色下 10 个色全部 < 3:1」**不成立**——`index.css:1513` 的 `filter: brightness(.72) saturate(1.12)` 已把它们拉到 3.91–5.67（10/10）。真问题是 **filter 让 `getComputedStyle` 读不到最终色、审计无法计算**。裁决：上移为 token、**删 filter**、属 L6（不得离开 `#chat`）。已按复算数字改写 [`view-chat.md`](./view-chat.md) §8 C12 |
| R8 | `.kpi`（captain 转述中出现，我方 **未列**） | **不受理** → `ReadoutStrip` / `.readout.l` / `.readout.s` / `.readout-strip.grid` / `.stat-strip` + `.stat-cell` | **不受理（无请求 + 同义原语拒绝）** | primitives §7 R8。三类数字块已有原语；再开 `.kpi` 会让「哪个数字放哪个类」变成审美题 |
| — | （非我方请求） | `.spinner-block` / `.error-state` / `.icon-btn` / `--signal-text` / `.readout-strip.grid` / `Zone` 组件 | **primitives 新增** | primitives §3.2 B7 / §3.5 E3·E4 / 附录 D。13 份规格已全部改用：loading → `.spinner-block`，error → `.error-state`，窄屏读数条 → `.readout-strip.grid`（README §3.4 X4），zone → `Zone` 组件 / 双类名 |
| — | （我方 §3.1 清单里曾列入、但**未作为请求**提出） | `.nums` / `.state.empty` / `.empty-icon` / `.readout-row` / `.view-bar .sub` / `.tool-io .io-label` | **primitives 删除**（13 份规格不得再引用） | primitives §10.4：每条都能用一条命令证明 0 引用或被替代品取代。**13 份规格的引用数已实测 = 0**（命令与原始输出见 §3.6）；这 6 个名字此后只出现在**本文件**的裁决/删除记录里（§3.2 本表、§3.4 X6、§3.5、§3.6），规格正文一个都不留 |

**对齐动作（t16 落在 13 份规格上的改动）**

1. `.micro` 一律写 **11/15**（原写 11/16 的 4 处已改：README §3.2、`view-chat.md` §3、`view-memory.md` §3、13 份规格的 §7 请求行）。
2. `.data` 全部撤下，改为「数据文本用既有 `.mono`」（13 份规格的 §7 请求行 + `view-stats.md` §8 S4）。**这是一条裁决记录**（primitives §7 R3 拒绝），不是使用位置。
3. §7 的「新增类请求」行统一改为：`.sr-only`（隐藏 `h1`）+ `.micro`（11/15）+ `.error-state` + `.spinner-block`。
4. 圆角归一（README §3.4 X2）：`.agent-avatar` **8 → 6**（`view-agents.md` A7）、`.row-btn` / `.kanban-card` → **10**、`.prompt-view` → **6**。
5. 状态语义（README §3.4 X1）：`interrupted → --status-err`；`waiting_permission` / `blocked → --signal`。
6. `WS_HUES` 结论按 primitives §7.1 复算**改写**（`view-chat.md` C12），不保留错误结论。
7. 顺手修掉 1 处改名残留：`view-runtimes.md` 的「见 chat.md §1」→「见 view-chat.md §1」。
### 3.3 会话来源点（`sourceHue`）方案 —— 不新增任何调色板

原则（按 captain 2026-09-21 提示 + MASTER §3.1 F「域内分类色」定义）：

1. **有官方品牌色的来源复用 `brand.tsx` 既有 token，零新增 hex**；primitives §7 R6 已把这 3 个 hex **提升为 CSS 变量**（`--brand-claude` / `--brand-deepseek` / `--brand-opencode`，双模式）——来源点的底色是**内联 style**，引用不到类，所以**规格里凡给标记取色，写 var(--brand-*) 而不是类名**；
2. **没有品牌色的来源一律中性**（`--ant-color-text-tertiary`），不新开分类族；
3. 来源点**禁止**使用 `--signal` / `--status-*` / `--graph-*` / `--ws-*`——它们是别的域的分类色（跨域串味 = MASTER §3.2 的黑名单逻辑）；
4. 「一色一义」的口径仍是**同一路由内**（MASTER §3.1 F 的域内约束）。

| `SessionRecord.source` | 取色 | 暗 | 亮 | 说理 |
|---|---|---|---|---|
| `claude-code` | `--brand-claude`（既有） | `#d97757` | `#d97757` | 官方品牌色（MASTER §3.1 G1），与 signal 琥珀不同族（H=14° vs 36°） |
| `opencode` | `--brand-opencode`（既有） | `#e8e8ea` | `#121212` | 官方品牌色（G3） |
| `deepseek` | `--brand-deepseek`（既有） | `#5786fe` | `#5786fe` | 官方品牌色（G2） |
| `dsh` / `ruagent` / `codex` / 未知 | `--ant-color-text-tertiary`（既有） | 中性档 | 中性档 | 无品牌色 → 中性；**来源身份由旁边的 `.tag` 文本标签承担，颜色不承担语义**。值随 `theme.tsx` 的 `D/L.textTertiary` 走，规格**不复制 hex**（primitives §10.2 X3 会把亮色档改成 `#5f646d`） |

**实现级终稿（`sourceHue()` 的逐行签名 + 约束）在 [`view-sessions.md`](./view-sessions.md) §4.1**——本表是裁决，那张表是实现指令；两处冲突以 §4.1 为准。

**被替代的现状（2026-09-21 复测，200 行）**：`Sessions.tsx:17 sourceHue()` 的三个分支**没有一个是中性档**，且两个落进琥珀带：

| 现状分支 | 返回 | 暗解析值 | 亮解析值 | 后果 |
|---|---|---|---|---|
| `claude-code` | `var(--ant-color-warning)` | `rgb(226,137,79)` = `--status-warn` | `rgb(138,74,18)` | H=23.7°，**落在琥珀带内**（24 行） |
| `dsh` | `var(--ant-color-primary)` | `rgb(240,169,59)` = **`--signal`** | `rgb(200,132,19)` = `--signal` | 直接违反 V1（107 行） |
| `default`（`ruagent` / `opencode` / …） | `var(--ant-color-success)` | `rgb(79,180,119)` | 绿档 | 域外借用（69 行），不落在琥珀带但不合规 |

实测 `#sessions` 因此有 **131 个琥珀 `.dot`**（= 107 `dsh` + 24 `claude-code`，MASTER §3.2 黑名单 V1）——把「现在」误读成「会话来源」。
本次改动把 `#sessions` 的琥珀声明节点数从 **138 降到 7**，且这 7 个**逐个列名**在 [`view-sessions.md`](./view-sessions.md) §8「S3 计数口径与 7 的构成」里（全部是侧栏当前导航项子树，落在 W1 白名单内，**无一在内容区**）。

**被否决的两个替代方案**（记录理由，避免下游重新发明）：
- ✗ 借 `--graph-*` 的 hue 槽（如 dsh=`--graph-project`）：能达标，但会让同一个 hex 在两个视图里指两种东西（graph=项目实体 / sessions=CLI 来源），与 V1 同类。
- ✗ **已拒绝**：新建 `--source-1..5` —— 全站已有 4 套分类色（`--graph-*` 9、`WS_HUES` 10、`--status-*` 4、品牌色 3），再开第 5 套会让 MASTER 阈值行 33 无法维护。

✅ **两处登记已于 2026-09-21 在 MASTER 落槌**（t17，本文目录的规格同步更新）：

1. **行 6 条件化 → MASTER §12 行 6 + §12.1**：目标从无条件「≥3 且 ≤40」改为「**含读数的页 ≥3；无读数的页 ≥1；上界 ≤40 不变**」，`#chat` / `#settings` / `#inbox` 三个无读数路由列入豁免清单（逐条给理由），**交换条件**是仍须恰好 1 个 20px 页面标题作为层级中心。落点已回填到 `view-chat.md` §8 C2、`view-settings.md` §8 G1、`view-inbox.md` §8 I2。
2. **跨域分类色不得互相借用 → MASTER §3.1 I（新增小节）**：明文规定三个域（① 图谱实体类型 `--graph-*` ② 运行时品牌 `--brand-*` ③ 会话来源标记）各自只能用自己的色档；③ **合并进** ②，`--source-*` **明确拒绝**（零新增 hex）；并写明`跨域引用的直接后果是阈值行 33 失效`，行 33 的判定已挂上这条断言。本条正是本文 §3.3 方案成立所依赖的规则。
3. **`WS_HUES` 已登记** → primitives.md §7.1：`--ws-1..--ws-10` 双模式 token（暗 = 现行值，亮 = ×0.72 派生），**删除 `filter`**；并修正了本文原先`亮色下 10 个色全部低于 3:1` 的结论（见 `view-chat.md` §8 C12）。

### 3.4 横切规则（primitives 定稿后新增，13 份规格统一遵守）

| # | 规则 | 出处 | 受影响的规格 |
|---|---|---|---|
| X1 | **状态语义**：颜色编码「活性类别」，文本编码「精确状态」。`interrupted` → `--status-err`；`waiting_permission` / `blocked` → `--signal`。**`--status-warn` 不再等于 `--signal`** | primitives §2.3 + MASTER C2 | 全部（经 `StatusDot` / `StatusPill`）；view-board.md B10、view-task-detail.md §4.2、view-memory.md 的 `.tag.warn` |
| X2 | **圆角只走 4/6/10/14**：8px 不加入阶梯。归一：`.row-btn` / `.kanban-card` → **10**；`.agent-avatar` / `.prompt-view` → **6** | primitives §9.2 | sessions / board / stats / knowledge / agents / chat |
| X3 | **`.zone*` 双类名采用**：`className="kanban-col zone"`，**不替换**冻结选择器；采用点 5 个，最终命中 <3 条路由则触发删除条件 | primitives §9.1 | home / board / task-detail / agents / memory / knowledge / graph / settings |
| X4 | **窄屏读数条**：≤520 的 2×2 排布统一用 `.readout-strip.grid`，视图**不得**自己写这段 CSS | primitives §3.2 B7 | home / board / memory / sessions / knowledge / graph / runtimes / stats |
| X5 | **三态互斥**：loading → `.spinner-block`；empty → `.empty-state`；error → `.error-state`（role=alert + 重试）。判定：error 存在时不得存在 `.ant-empty` | primitives §3.5 + MASTER P6/行 20 | 全部 |
| X6 | **已删除原语不得再引用**：`.nums` / `.state.empty` / `.empty-icon` / `.readout-row` / `.view-bar .sub` / `.tool-io .io-label` | primitives §10.4 | 全部（**13 份规格实测引用数 = 0**，命令与原始输出见 §3.6） |

### 3.5 类名来源判定（可复现证据）

**问题**：13 份规格里出现的每一个共享类名，必须能指向「`panel/src/index.css` 里已定义」/「`panel/src/ui.tsx` 导出」/「`primitives.md` 终稿清单」，**不允许出现规格自造名**。

**证据命令**（在仓库根目录执行；输出 = 每个类名一行的「名字 | 判定」）：

```bash
cd panel
# 提取：13 份规格里所有「以 . 开头的类名」。
# 前置字符限定为 行首/空白/反引号/竖线/括号/尖括号/逗号/星号 —— 这样 `options?.mode`、
# `choices[].kind`、`run.id`、`a.runtimes?.includes(`、`/.msg/.tool/.plan/` 这类
# TS 成员访问与斜杠散文都不会被误当成类名（这是本节 2026-09-21 的修正：
# 旧命令用 [^[:alnum:]_-] 前置，会把上面这些全部抓进来）。
grep -ohE '(^|[[:space:]`|(>,*])\.[a-z][a-z0-9-]+' ../docs/design/views/view-*.md \
  | grep -oE '\.[a-z][a-z0-9-]+$' | sed 's/^\.//' | sort -u \
  | grep -vE '^(catch|waiting|brand-)$' > /tmp/names.txt   # 三个已知假阳性：.catch() / waiting_permission / var(--brand-*)

# 判定。顺序即优先级：antd 内部类 → 删除清单 → index.css → ui.tsx → primitives.md → 视图私有
DELETED='empty-icon io-label nums readout-row tool-io state sub'   # primitives §10.1 D1/D4/D5/D6/D10/D11 在提取器下会产出的名字
                                                    # （后两个是复合形式的提取结果；这 7 个名字实测在 13 份规格里全部缺席 = 删除记录）
while read -r c; do
  if [ "${c#ant-}" != "$c" ]; then echo "$c | antd-internal"
  elif printf '%s\n' $DELETED | grep -qx -- "$c"; then echo "$c | primitives-DELETED"
  elif grep -qE "\.$c([^a-zA-Z0-9-]|\$)" src/index.css; then echo "$c | index.css"
  elif grep -qE "\.$c([^a-zA-Z0-9-]|\$)" src/ui.tsx; then echo "$c | ui.tsx"
  elif grep -qE "\.$c([^a-zA-Z0-9-]|\$)" ../docs/design/primitives.md; then echo "$c | primitives.md"
  elif grep -rqE "(^|[^a-zA-Z0-9_-])$c([^a-zA-Z0-9_-]|\$)" src/views/*.tsx src/*.tsx; then echo "$c | view-private"
  else echo "$c | *** UNKNOWN ***"; fi
done < /tmp/names.txt | tee /tmp/names-out.txt

# 汇总
cut -d'|' -f2 /tmp/names-out.txt | sed 's/^ *//;s/ *$//' | sort | uniq -c | sort -rn
```

> **2026-09-21 修正记录（不静默改数）**：本节 t16 版命令有两处缺陷——① `view-private` 分支的正则写在双引号里且含未转义反引号，bash 报 `syntax error near unexpected token`，导致 20 个名字的判定整段丢失；② `primitives-DELETED` 分支用 `sed -n '/^\*\*删除/,/^```/p'` 划范围，实际圈到了整节，把 `.card` / `.composer` / `.muted` / `.time` / `.zone-*` 等 20+ 个**在用**原语误判成「已删除」。旧表（`index.css` 203 / `primitives-NEW` 6 / `primitives-DELETED` 3 / `primitives-REJECTED` 1）**不可复现**，已作废；下表是用上面这条命令重跑的结果。

**判定结果（2026-09-21 实跑，214 个类名，UNKNOWN = 0）**：

| 判定 | 数量 | 含义 |
|---|---|---|
| `index.css` | **203** | 共享层已定义的既有原语。**含 primitives 新增的 6 个**（`.error-state` / `.icon-btn` / `.micro` / `.readout-strip` / `.spinner-block` / `.sr-only`）——它们已由 `systems` 落到 `index.css`，所以不再单列 `primitives-NEW` 一类 |
| `primitives-NEW` | **0（类别已退役）** | 无待落地项：6 个终稿新增名逐个在 `index.css` 命中（`for c in error-state icon-btn micro readout-strip spinner-block sr-only; do grep -cE "\.$c([^a-zA-Z0-9-]\|$)" src/index.css; done` → `1 6 1 6 1 1`，全部 > 0），因此它们已被上面 `index.css` 那一行吸收，不再单列 |
| `primitives-DELETED` | **0** | **13 份规格对已删除原语的引用数 = 0**（见 §3.6） |
| `primitives-REJECTED` | **0** | 同上：已拒绝令牌的引用数 = 0 |
| `antd-internal` | **8** | antd 内部类（引用为证据/冻结选择器，不是我们的契约） |
| `view-private` | **3**（规格提及者）+ **11**（源码侧 className 字面量，见 §3.5.1） | 视图自有 hook 类：源码在应用但 CSS 里无规则。**两个数字来源不同、不可互相替代**：本行的 3 是上面那条命令（名字取自 13 份规格）的产物；11 是源码侧扫描（§3.5.1）的产物 |
| `NOT-A-CLASS` | **3** | 非类名，提取时显式排除（`.catch` / `.waiting` / `brand-`） |
| **`*** UNKNOWN ***`** | **0** | **无规格自造名** |

**非 `index.css` 的全部判定（逐名）**

* `antd-internal`： `.ant-card` `.ant-card-body` `.ant-empty` `.ant-input-sm` `.ant-select-input` `.ant-statistic-content-value` `.ant-table` `.ant-table-thead`
* `view-private`（**真有 CSS 规则之外的例外，逐条给理由**）——**本清单只是「规格提及的 3 个」；源码侧的完整清单（12 个，含本轮补登的 `.chat-session-head` / `.chat-earlier`）见 §3.5.1**：
  * `.ctl-agent` — `Chat.tsx` 的 `<Select className="ctl-select ctl-agent">`；**无语义样式**，只作选择器钩子（样式走 `.composer-controls .ctl-select`）。
  * `.dash-main` — `Home.tsx` 的 `<Card className="dash-main">`；**无 CSS 规则**（`.dash-grid` 才是网格）。保留为版式钩子，若要删须一并清 `view-home.md` 的线框。
  * `.is-empty` — `Chat.tsx:1091` 的模板拼接 `` className=`chat-wrap${..." is-empty"}` ``；**无 CSS 规则**。规格用它描述空态，样式仍由 `.chat-empty-zone` 承担。
* `NOT-A-CLASS`：`.catch`（`Home.tsx:86` 的 `.catch()` 兜底，散文里的成员访问）、`.waiting`（`waiting_permission` 状态串的行内写法）、`brand-`（`var(--brand-*)` 的星号截断）——**提取器的已知假阳性，已显式排除**。
* **`.nums` / `.state.empty` / `.empty-icon` / `.readout-row` / `.view-bar .sub` / `.tool-io .io-label` / `.data` / `.kpi` / `--source-*` 一个都不在本名单里**——它们不是「判定为某类」，而是**在 13 份规格里不存在**（§3.6 的扫描就是这条断言的证据）。

### 3.5.1 源码侧的纯钩子类（2026-09-21 补扫；§3.5 的 `view-private` 只覆盖「规格提及者」）

**为什么单列**：§3.5 的分类器**名字取自 13 份规格**，所以只可能报出规格里写过的类名（实测 3 个）。而**源码里在应用、CSS 里却没有规则**的类名要多得多——它们是纯结构钩子（e2e 选择器 / 版式锚点 / 语义标记）。不登记，下一次审计会把它们报成 UNKNOWN。

**补扫命令（实跑）**

```bash
cd panel
# 源码侧：所有 className="..." 字面量里的小写类名 token，再逐个问
# 「index.css / ui.tsx / primitives.md 三处有没有规则」
grep -ohE 'className="[^"]*"' src/views/*.tsx src/*.tsx \
  | sed 's/className="//;s/"$//' | tr ' ' '\n' \
  | grep -E '^[a-z][a-z0-9-]+$' | sort -u > /tmp/srccls.txt
# → 220 个 token；其中「三处都无规则」的 11 个见下表
```

**登记表（12 个 = 11 个源码侧命中 + `.is-empty`，后者用模板拼接所以扫不到）**

| 类名 | 出处 | `index.css` 规则数 | 登记状态 |
|---|---|---|---|
| `.ctl-agent` | `Chat.tsx` 的 `<Select className="ctl-select ctl-agent">` | **0** | 已登记（§3.5） |
| `.dash-main` | `Home.tsx` 的 `<Card className="dash-main">` | **0** | 已登记（§3.5） |
| `.is-empty` | `Chat.tsx:1091` 的模板拼接（`chat-wrap` + 条件追加 ` is-empty`） | **0** | 已登记（§3.5）；**本表扫描扫不到它**（非字面量） |
| **`.chat-session-head`** | `Chat.tsx:1257` `<div className="chat-hero chat-session-head">` | **0** | **2026-09-21 本轮补登（t4 新增）** |
| **`.chat-earlier`** | `Chat.tsx:1267` `<button className="row-btn chat-earlier">` | **0** | **2026-09-21 本轮补登（t4 新增）** |
| `.chat-long` | `Chat.tsx:1329` `<details className="chat-long">` | **0** | 本轮补扫发现（未在规格出现，登记备用） |
| `.dash` | `Home.tsx:195` `<div className="home dash">` | **0** | 同上 |
| `.ev-row` | `RunTimeline.tsx:404` `<div className="ev-row" data-row={index}>` | **0** | 同上 |
| `.graph-canvas-wrap` | `Graph.tsx:716` `<div ref={wrapRef} className="graph-canvas-wrap" style={{position:"relative"}}>` | **0** | 同上（另带内联 `position`） |
| `.judge-bar` | `TaskDetail.tsx:713` `<div className="card judge-bar">` | **0** | 同上 |
| `.plan` | `RunTimeline.tsx:480` `<div className="ev plan">` | **0** | 同上（`.plan-body` / `.plan-head` 有规则，`.plan` 本身是 `.ev` 的裸修饰） |
| `.tool` | `RunTimeline.tsx:625` `<div className="ev tool">` | **0** | 同上（`.tool-head` / `.tool-detail` 有规则，`.tool` 本身是 `.ev` 的裸修饰） |

**规则数实测命令与输出（逐字）**

```console
$ cd panel && for c in chat-session-head chat-earlier is-empty ctl-agent dash-main; do
    n=$(grep -cE "\.$c([^a-zA-Z0-9-]|$)" src/index.css || true); echo "$c | index.css rules = $n"; done
chat-session-head | index.css rules = 0
chat-earlier | index.css rules = 0
is-empty | index.css rules = 0
ctl-agent | index.css rules = 0
dash-main | index.css rules = 0
```

**判定口径**：这 12 个**都不是缺陷**——它们是「样式由并用类承担」的钩子（`.chat-session-head` 的视觉由 `.chat-hero h3` 给、`.chat-earlier` 由 `.row-btn` 给、`.ev-row` 由子级 `.ev` 给、`.judge-bar` 由 `.card` 给）。登记的目的是**让下一次审计不把它们报成 UNKNOWN**，不是要求补 CSS。要删任何一个，必须同时清掉引用它的线框/规格（`.dash-main` 的先例）。

**与 §3.5 表的关系**：§3.5 的 `view-private = 3` **不变**（它的名字来自 13 份规格，往表里加源码侧名字会让那条命令不可复现）；本节是**另一个口径的测量**，两者不互相替代。

### 3.6 已删除 / 已拒绝令牌扫描（13 份规格引用数 = 0 的证据）

**扫描对象**：`docs/design/views/view-*.md`（13 份规格）。`README.md` 是**裁决记录**，故意保留这些名字（见下）。

**令牌清单**

| 类别 | 令牌 | 出处 |
|---|---|---|
| 已删除 | `.nums` · `.state.empty` · `.empty-icon` · `.readout-row` · `.view-bar .sub` · `.tool-io .io-label`（含已无 CSS 的 `.tool-io` 本身） | primitives §10.1 D1/D4/D5/D6/D10/D11 + §10.4 |
| 已拒绝 | `.data`（同义原语，R3） · `.kpi`（无请求 + 同义原语，R8） · `--source-*`（MASTER §3.1 I 明确拒绝） | primitives §7 R3/R8；MASTER §3.1 I |

**命令**

```bash
cd /c/Users/19410/Documents/ai/ruagent     # 仓库根

DEL='\.nums([^a-zA-Z0-9_-]|$)|\.state\.empty([^a-zA-Z0-9_-]|$)|\.empty-icon([^a-zA-Z0-9_-]|$)'
DEL="$DEL|\.readout-row([^a-zA-Z0-9_-]|$)|\.view-bar \.sub([^a-zA-Z0-9_-]|$)|\.io-label([^a-zA-Z0-9_-]|$)"
DEL="$DEL|\.tool-io([^a-zA-Z0-9_-]|$)|\.data([^a-zA-Z0-9_-]|$)|\.kpi([^a-zA-Z0-9_-]|$)|--source-"

grep -nE "$DEL" docs/design/views/view-*.md          # 原始命中行（期望：无输出，退出码 1）
grep -cE "$DEL" docs/design/views/view-*.md          # 逐文件计数（期望：全 0）
grep -cE "$DEL" docs/design/views/view-*.md | awk -F: '{s+=$2} END{print "TOTAL="s}'
```

**原始输出（2026-09-21 实跑，逐字粘贴）**

```console
$ grep -nE "$DEL" docs/design/views/view-*.md ; echo "exit=$?"
exit=1
```

```console
$ grep -cE "$DEL" docs/design/views/view-*.md
docs/design/views/view-agents.md:0
docs/design/views/view-board.md:0
docs/design/views/view-chat.md:0
docs/design/views/view-graph.md:0
docs/design/views/view-home.md:0
docs/design/views/view-inbox.md:0
docs/design/views/view-knowledge.md:0
docs/design/views/view-memory.md:0
docs/design/views/view-runtimes.md:0
docs/design/views/view-sessions.md:0
docs/design/views/view-settings.md:0
docs/design/views/view-stats.md:0
docs/design/views/view-task-detail.md:0

$ grep -cE "$DEL" docs/design/views/view-*.md | awk -F: '{s+=$2} END{print "TOTAL="s}'
TOTAL=0
```

**README.md 侧的命中（记录，不是引用）**

同一命令跑 `README.md` **会命中**（本文件就是那个说明「这些名字为什么不在任何规格里」的地方）。判定式不是「README 也必须为 0」，而是「README 的每条命中都必须落在**记录行**上」：

```bash
grep -cE "$DEL" docs/design/views/README.md          # 命中行数（随本文件自身增删而变化，不设阈值）
grep -nE "$DEL" docs/design/views/README.md \
  | grep -vE '已删除|已拒绝|拒绝|不受理|不得再引用|清除|裁决|清单|DEL=|记录|已作废|扫描|不存在'
# ↑ 期望：无输出（= 每一条命中都带记录标记）
```

**原始输出（2026-09-21 实跑）**

```console
$ grep -cE "$DEL" docs/design/views/README.md
18

$ grep -nE "$DEL" docs/design/views/README.md | grep -vE '已删除|已拒绝|拒绝|不受理|不得再引用|清除|裁决|清单|DEL=|记录|已作废|扫描|不存在' ; echo "exit=$?"
exit=1
```

命中的**类别**（不逐行钉死行号——本文件每加一行说明，行号就变，钉死只会让下一个人重新对账）：

| 位置 | 性质 |
|---|---|
| §3.1 文本工具行 / 状态区块行 / 时间线行 | 「**已删除**」标注 + IO 容器改为 `.tool-detail > pre.raw` 的说明 |
| §3.2 R3 / R8 / 末行 / 对齐动作 2 | `.data` 与 `.kpi` 的**拒绝裁决**、6 个删除名的**总记录**、撤下的**历史记录** |
| §3.3 被否决的替代方案 | `--source-*` **已拒绝**的两个方案 |
| §3.4 X6 | 横切规则本身（规则必须能点出名字） |
| §3.5 判定表 / 逐名清单 | `primitives-DELETED` / `primitives-REJECTED` 的类别定义（数量已归 0） |
| §3.6 本小节 | 令牌清单、命令、记录表——自指 |

**为什么记录留在 README 而不留在 13 份规格**：规格是**实现指令**，里面出现一个类名就意味着「用它」；README 是**裁决与索引**，它的职责正是说清「为什么这个名字不在任何规格里」。判定式：`grep -c` 在 `view-*.md` 上必须为 **0**，在 `README.md` 上可以非 0，但每一条命中都必须带记录标记（上面那条 `grep -v` 断言）。

## 4 每份规格的固定结构

```
头部：路由 / 视图组件 / 消费的 MASTER 行号 + 变更编号
1 职责与首要任务
2 版式骨架（ASCII 线框，标栏宽与断点）
3 层级（本页最大元素是什么、为什么）
4 内容模型（api.ts 真实类型 + 字段 → 展示位 + 分组 + 密度上限）
5 四种状态：loading / empty / error / 密集数据
6 响应式行为（390 / 768 / 1280 / 1440）
7 复用的共享原语与类名
8 本页特有的验收数字（引用 MASTER §12 行号）
```

## 4.5 命名规则：为什么文件叫 `view-*.md`

本目录 13 份规格的文件名一律为 **`view-<route>.md`**。

原因：Windows / macOS 是大小写不敏感文件系统，`agents.md` 与各 agent 工具链约定的目录级工作说明
`AGENTS.md` 是**同一个文件**。实测后果：本机任何进入本目录的 agent 都会把 `#agents` 的规格
当作"工作说明"注入上下文（harness 报告的路径就是 `docs/design/views/AGENTS.md`，而磁盘上是 `agents.md`）。

**已解决（captain 决定，2026-09-21）**：13 份规格统一加 `view-` 前缀，`view-agents.md` 不再命中
`AGENTS.md` 的**精确 basename** 判定。路由与文件名仍是一一对应（`view-<route>.md`），前缀只是命名空间，
不构成 t2 原先担心的"破坏对应关系"。

## 5 四视口基准（实测，所有线框以此为准）

| 视口 | 侧栏 | `.content` 外框 | 内容可用宽 | `.card` 实测宽 |
|---|---|---|---|---|
| 390 | 72（收起） | 318 | 294 | 294 |
| 768 | 72 | 696 | 662 | 662 |
| 1280 | 228 | 1052 | 996 | 996 |
| 1440 | 228 | 1212 | 1149 | 1149 |

侧栏折叠阈值 = **992**（`App.tsx` `matchMedia`，e2e 契约，MASTER §10.2 保留为契约值）。
`.content` = `padding: clamp(16px,1.8vw,32px) clamp(16px,2.2vw,40px) 80px`，`max-width: 1560px`（1920 视口触顶）。

## 6 术语

- **首要任务**：用户打开这一页 90% 是为了做的那一件事。一个页面只有一个。
- **密度上限**：本页在 1440 视口下的行数/节点数/字符数上限，超出必须分页或虚拟化。
- **信号**：`--signal`，只表示"现在/运行中/等你处理/主操作"（MASTER §3.2）。
- **冻结选择器**：e2e 依赖，不得改名（MASTER §0）。

## 侧边栏：统一的收起控件与拖拽伸缩（2026-09-23 裁决；用户报告驱动）

> **用户原话**：「**侧边栏无法伸缩，比如拖动来伸缩宽度**。而且**没有收起侧边栏的按钮（或者不统一）**」

**现状（captain 实测，1440×950 暗色 `#chat`）**：主导航栏 `.app-sider` **228×950 ` x=0**（页脚按钮 `aria="收起 / 展开侧栏"` 32×32 ` x=79 —— **一直可见** ✓）· chat 会话栏 `.chat-side` **248×536 ` x=244**（`.chat-side-toggle` **0×0 ` (0,0) `aria="历史"`** —— **1440 下不可见** ✗）· **全仓无任何拖拽伸缩**（只有 `Graph.tsx` 的节点拖拽）。
⇒ **三个问题**：**①** 宽屏下 chat 会话栏**没有收起初口**（该 toggle 只在**侧栏关闭时**出现 ⇒ **只有「打开」没有「关闭」**；关闭路径只有窄屏的 `.chat-side-backdrop`）**②** 两个收起控件**不统一**（形状 / 位置 / aria 命名都不同）**③** 都**不能拖拽伸缩**。

### S1 统一的收起控件（可判定）
- **同一形态**：**同一图标 · 同一尺寸 32×32 · 同一 aria 命名模式** —— **收起时 `aria-label="展开侧栏"` / 展开时 `aria-label="收起侧栏"`**（走 i18n，与 `ruagent.lang` 同族）✓
- **位置**：各自侧边栏内**自然的位置** —— 主导航栏**保持页脚** ✓；**chat 会话栏放在它的头部**（与「新对话」/ 搜索同区）✓。**统一的是「控件」，不强行统一「位置」** —— 理由：两条栏的**层级与用途不同**（主导航栏是全局导航、chat 栏是列表工具条），位置也统一会**牺牲各自的自然分组**；而**控件形态统一**已足以让用户「在哪儿都能找到同一个东西」✓
- **chat 会话栏的收起入口在**所有宽度**都必须可用**（修 ①）：**可判定** = 视口 ∈ {390, 768, 1440} 下该控件**必须可见且可点击**（`width > 0 ∧ height > 0` ∧ 可聚焦）；**当前 1440 下 = 0×0 ⇒ FAIL** ✓
- **收起态持久化**：写入 `ruagent.*` 键（与 `ruagent.mode` / `ruagent.lang` 同族）。**理由**：这是**用户主动设置**，不是「一次访问改写默认」—— 与「默认值不该被一次深链破掉」**方向相反但同一条理由**：**默认值不被访问改写；用户显式选择则必须保留** ✓


### S1.1 每条栏在 ≥1024 下只能有「一个」可见的收起入口（2026-09-23 裁决）

**对象**：**每条侧栏（主导航栏 · chat 会话栏）的收起 / 展开入口** ✓
**阈值**：**在 ≥1024 下，每条栏「可见的」收起入口恰好 1 个** ✓ —— **依据 = 裁决**（**不是测量** ⇒ 按本代惯例标注 ✓）
**意图（为什么是「一个」而不是「每个入口各自统一」）**：**同义入口会让人不确定点哪个**，也让 S1 的「统一」失效 ✗ —— **S1 要求的是「一个控件」，不是「每个入口都长得一样」** ✓
**实测（captain，1440px）**：`.chat-rail-toggle` **32×32 `x=422**（`aria=收起侧栏`）**与** `.chat-rail-toggle` **32×32 `x=517**（`aria=收起 / 展开会话历史`）**同时可见** ✗（**两个 32×32 都切换同一条栏** ✓）；`.chat-side-toggle` **0×0 `x=0**（窄屏冻结控件，1440 隐藏 ✓）⇒ **裁决：保留头部控件、删掉视栏那个** ✓（**实现由 t134 落** ✓）
**「可见」的可判定定义** ✓：**`width > 0 ∧ height > 0` 且其边界盒与视口相交**（`getBoundingClientRect()` 与 `(0, 0, innerWidth, innerHeight)` 有交集）✓ —— **必须含「与视口相交」这一半** ✗：否则**屏外但可聚焦**的元素会被算进来（**这正是本判据最容易出错的地方** ✓）
**<1024 的形态（必须写明）**：**`.chat-side-toggle` 是冻结选择器、是抽屉的入口 ⇒ 保留** ✓；**头部控件在抽屉关闭时位于屏外但可聚焦 ⇒ 不重复计数**（**判据在 <1024 只对「抽屉入口」计数** ✓）；**并且**：**抽屉打开时若头部控件与 `.chat-side-toggle` 同时可见，该档同样必须满足「恰好 1 个」** ✓（**否则洞会从窄屏漏回来** ✓）
**与既有行的关系（对象不同 ⇒ 不合并也不重复）**：**行 18 = 命中可达**（有效命中盒 ≥24px ✓）· **行 52 = 单个元素 ≥44px**（触摸可达 ✓）· **行 54 = 窄屏行级形态**（行内控件数 / 行高 ✓）· **本判据 = 入口的数量** ✓
**是否进审计（design-lead 判断：进）** ✓ —— **理由**：**它是结构属性、且随「新增控件」而回归**（本次就是**新增头部控件撞上既有视栏控件** ✓）；**测法便宜**（在 ≥1024 数「可见的入口元素」✓）；**建议行号 55**（**由 captain 决定是否派单** ✓）

### S2 拖拽伸缩（可判定）
- **两条侧边栏都可拖** —— 理由：用户原话是「**侧边栏**无法伸缩」（不是「chat 栏」），且两条栏都是常驻列、都占用主内容区宽度 ⇒ **只做一条会留下同一个抱怨的另一半** ✓
- **⚠️ 硬约束：默认宽度不变** —— **主导航栏 228 · chat 栏 248**（`sidebar 228` 与 collapsed `72` 是**冻结的 e2e 契约**；`palette.spec` / `theme.spec` 断言 `.app-sider` 可见）⇒ **可拖拽不得改变默认值**；**未拖过时渲染的宽度必须逐位等于 228 / 248** ✓
- **边界（派生，非拍脑袋）**：
  - **min（主导航栏）= 228**：依据 = **默认值本身已是临界** —— 页脚 7 项的**自然宽 347.9px > 203px 可用**（实测）⇒ 228 时**已折成两行**（172.7 + 171.2）⇒ **再窄会让品牌名 / 页脚不可读**；**收起态 72 是另一档，不参与拖拽区间** ✓
  - **min（chat 栏）= 248**：依据 = **实测 248 时行内容盒 125px < 行需求 165px**（标题已被压到 65 / 0）⇒ **再窄必然把主标签压到 0**（MASTER §12 **行 46** 的判据会红）⇒ **不许更窄** ✓
  - **max = min(2 × 默认, 视口 − 另一条侧栏的当前宽 − 390)**：两个因子各有出处 —— **`390`** = **实测可用的最窄主内容宽**（390 档是完整可用的布局 ✓）；**`2 ×`** = **设计裁决**：**侧栏不是主内容**，超过翻倍就该用「收起」而不是拖拽（**这一条是裁决，不是测量 —— 已标注** ✓）
  - ⇒ **1440 下的实际区间**：主导航栏 **[228, min(456, 802)] = [228, 456]** · chat 栏 **[248, min(496, 802)] = [248, 496]** ✓ **待 systems 用探针复核两个上界**（复核只收紧数字，不改规则形状 ✓）
- **视口门控**：**可拖 ⇔ 视口 ≥ 1024** —— 依据：**≥1024 时两条侧栏都是常驻列**；**≤768 时 chat 栏是抽屉 / 覆盖层**（拖拽与「溢出 0」的契约冲突）⇒ **窄屏不得可拖** ✓ **768–1024 之间同样不可拖**（该区间 chat 栏仍非常驻列）✓
- **形态（Bench）**：**一条 1px 的规则线，hover 时加重**（**rules-not-boxes**）· **不得引入卡片 / 圆角块 / 粗抓手** ✓ 可判定：把手元素 `width ≤ 2px`（含 hover 态）且**无 `border-radius` > 0 的可见块** ✓
- **可及性（硬要求，APG window splitter）**：把手**可聚焦**（`tabindex="0"`）· `role="separator"` · `aria-orientation="vertical"` · `aria-valuenow` / `aria-valuemin` / `aria-valuemax` **必须与当前 / 边界值一致** · **方向键可调**（`←` / `→` 每次 8px，`Home` / `End` 到 min / max）✓
  - **键盘判据（可判定）**：聚焦把手后按 `→` ⇒ **宽度增加且 `aria-valuenow` 同步增加**；按 `End` ⇒ **宽度 = max 且 `aria-valuenow = aria-valuemax`**；按 `Home` ⇒ **= min**；**全程无鼠标事件** ✓
- **不得破坏**：冻结选择器 **`.app-sider` / `.chat-side-toggle` / `.chat-side.open` / `.chat-side-backdrop` / `.brand-mark` / `.brand-name`** 必须仍在且语义不变 ✓；**390 / 768 / 1440 的溢出仍为 0** ✓；**未拖过时不得引入任何内联宽度**（默认路径与今天逐位相同 ✓）

### N1 会话 / 对话的「名字」必须来自用户自己的话（2026-09-23 新增；两个列表共用）

**规则**：**列表里显示的名字必须来自用户自己的话**；**平台注入的块**（角色提示词 · 记忆上下文 · handoff 尾）**不得出现在名字里**。
**实测（captain，1440 暗色 `#sessions`；`/api/v1/sessions` 199 条）**：**26 条的名字以 `[` 开头** —— 例：`title='[memory context — what the'` · `title='[role — you are] 你是'`（`preview` 同形）⇒ **用户看到的会话名是平台的提示词，不是他自己的话** ✗。
**机制**：`chat.rs` 把 ctx（角色提示词 + 记忆上下文 + handoff 尾）**拼进发给 agent 的 prompt** ⇒ harness 的 transcript 记录整段 ⇒ **索引器把整段当成「用户消息」**（**注意：`chats.title` 本身是对的**，text 是用户原文）✓
**判据**：**该行名字不得以平台的注入前缀开头**（具体形态等 **t101** 的判据草案 —— 并**明令不用「剥掉开头方括号块」这种规则**：用户完全可能真的以 `[` 开头，那会把真内容也剥掉 ✓）。**修法方向**：**优先结构分离**（独立 content block / 显式边界标记）✓
**适用范围**：**`#sessions` 与 `#chat` 的列表共用本条**（两处都显示会话名）✓

### S2.1 splitter 的命中区按「有效命中盒（含伪元素）」测（2026-09-23 裁决）

**冲突的形态**（本代第 ⑧ 类：前提本身不成立）：**S2 的形态判据**要求 splitter 元素盒 **≤2px**（rules-not-boxes：1px 规则线 ✓），而 **§12 行 18** 按**元素盒**判命中区 **≥24px** ⇒ **一个 1px 元素在构造上无法两全** ✗（t132 的透明 `::after` 抓取区实测只有 **11px** ⇒ 本身也 <24px ✗）✓

**裁决：按「有效命中盒（含伪元素）」测** ✓
- **为什么这样做**：**行 18 的原始意图是「触摸 / 指针可达性」** ✓ —— **而可达性由「实际可点的区域」决定，不由元素盒决定** ✓；**伪元素参与命中测试是浏览器的真实行为** ✓ ⇒ **不测它，判据就会漏掉真实的可达性** ✗
- **为什么不选「记具名例外」**：例外需要**正面定义对象集**（哪些元素算 splitter ✓）—— 那会把一条**通用判据**换成一张**名单**，而**名单会随新增 splitter 而漏** ✗（本代已判过：**对象集比意图窄**）；**而口径本身已经能表达意图**（可达性 ✓）⇒ **改口径比记名单更接近意图** ✓
- **代价（如实写）**：**判据的测法变复杂**（要读伪元素或做真实命中测试 ✓）；**缓解**：**判定法固定为「真实指针命中测试」**（见下）⇒ **复杂度落在一次实现里，而不是每个读者身上** ✓
- **与 S2 的关系（不矛盾，对象不同）**：**S2 约束「视觉形态」**（元素盒 ≤2px · 1px 规则线 ✓）；**行 18 约束「命中可达」**（**有效命中盒** ≥24px ✓）⇒ **对象不同（视觉 vs 命中）⇒ 不合并也不重复** ✓
- **与行 16 的关系（也不矛盾）**：**行 16 管「焦点环」**（outline 1px → 2px ✓）；**S2 管元素盒** ✓ ⇒ **对象不同** ✓ —— **下一个人不要把它也当成矛盾** ✓

**「有效命中盒」的可判定定义** ✓：**元素盒 ∪ 参与命中测试的伪元素（`::after` / `::before`）** ✓
- **判定法（首选）**：**用真实指针在元素中心与边缘 ±N px 处各命中一次，取实际能触发的横向范围** ✓（**比读样式更可靠** —— 不依赖对 `pointer-events` / 层叠的静态推理 ✓）
- **若改用读样式**：**必须写明怎么算伪元素参与的部分**（`inset-inline` / `width` 的**实际计算值** ✓）—— **不得只读元素盒** ✗
- **本代数据**：t132 的 `::after` 抓取区 **11px**（<24 ✗）⇒ **t140 改为 `inset-inline: -12px` ⇒ 有效命中宽 25px** ✓ ⇒ **按本口径，t140 的改动就足以让行 18 转绿** ✓

**⚠️ 反向证据要求（必须写进实现与验收）**：**把伪元素去掉 ⇒ 行 18 必须 FAIL** ✓ —— **否则这条口径会退化成「凡 1px 元素都自动通过」** ✗（本代反复在防的**空集绿** ✓）
