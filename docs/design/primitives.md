# ruagent Panel 共享层原语契约 — primitives

> **文档地位**：本文件是**设计与实现之间的接口**，也是共享热文件
> `panel/src/index.css` / `panel/src/theme.tsx` / `panel/src/ui.tsx` 的**唯一改造规格**。
> 上游是 `docs/design/MASTER.md`（Bench 设计系统总纲）。本文件把 MASTER 的"设计意图"
> 翻译成"共享层可实现的类名、组件签名、token 取值与验收判据"，并给出这三个文件的
> 逐条改动清单（新增 / 修改 / 删除）。
>
> - **版本**：v1（2026-09-21）
> - **作者**：`systems`（共享层唯一写者）
> - **本文件不含代码改动**，也**不允许**在本文件里改 `panel/`；落地是 t5。
> - **上游约束**：MASTER §0 的冻结契约（e2e 选择器、侧栏 228/72、四视口无溢出）
>   在本文中原样继承，不重新解释。
> - **数字口径**：全部来自 DOM 计算样式与 WCAG 合成计算；复现命令见附录 B。
>   本环境图像桥不可用，**没有任何人看过像素**；因此本文只出现比值、计数、
>   选择器与文件行号，不出现"看起来更好"。
> - **与 MASTER 的关系**：MASTER 说什么，本文要么**逐条采纳并落到类名/token**，
>   要么**显式不采纳并给出实测理由**（§8 表态表）。凡本文数字与 MASTER 不一致处，
>   一律登记在附录 A，不静默改数。

---

## 0 导读与冻结契约

### 0.1 三个角色怎么读这份文档

| 你是谁 | 读哪几节 | 你要拿走什么 |
|---|---|---|
| t5（落地共享层） | §2 令牌、§3 原语、§10 改动清单、§12 批次 | 逐条照抄的 CSS/token/组件改动 |
| t2 / t16（视图规格与对齐） | §3 原语类名、§4 容器判定树、§5 无障碍、§7 原语请求裁决 | 只能用哪些类名、哪些是新原语、哪些被拒 |
| t7（量化验收） | §5 判据、§6 改动边界、§11 验收映射、附录 B | 每个阈值行的机器判据与例外清单 |

### 0.2 硬冻结（继承 MASTER §0，任何改动都不得触碰）

**e2e 依赖的选择器（22 条）**：
`.view-bar h2` / `.card.launcher` / `.kanban-col` / `.row-btn` / `.agent-card` /
`.inbox-card` / `.ant-table` / `.ant-empty` / `.composer` /
`.composer-controls .ctl-select` / `.chat-side-toggle` / `.chat-side.open` /
`.chat-side-backdrop` / `.brand-mark` / `.brand-name` / `.dash-grid` / `.dash-head` /
`.guide-grid` / `.home-hero h1` / `.kbd-hint` / `.app-sider` / `.cmdk*`。

**其它冻结值**：侧栏宽 `228`（收起 `72`，≤992 自动收起）；390 / 768 / 1280 / 1440
（e2e 实际用到）与 520 / 1024 / 1920（审计用到）视口不得横向溢出。

**允许的改动**：这 22 条选择器**承担的元素身份、元素类型、祖先-后代关系、可见性语义**
一律不变；**颜色、圆角、字号、行高、间距、阴影、过渡**可以改——只要 §6 的边界表允许，
且改完后 e2e（41 项）全绿。

### 0.3 本轮共享层要解决的 6 件事（其余都是附带）

| # | 问题（来源） | 本文裁决 | 落点 |
|---|---|---|---|
| 1 | `--status-warn` 与 `--signal` **完全同值**（暗 `#f0a93b` / 亮 `#8a5600`） | 拆开：warn 换 hue，`interrupted` 归 err，`waiting_permission`/`blocked` 归信号 | §2.3、§8 C2 |
| 2 | 焦点环 4 组里有 3 组 < 3:1（antd 暗 1.73 / 亮 1.19；手写亮 1.69） | 一个 `--focus-ring` 令牌 + 两条渲染路径 + 一条禁令 | §5.1、§8 A1 |
| 3 | 亮色 `.tag.ok` 3.34:1、亮色 `--graph-edge` 2.17:1、亮色文本梯度非单调 | 三个都按实测值钉死 | §2.4、§8 C3/C4/C9 |
| 4 | `.zone*` 原语定义完整但 13 路由 **0 引用** | **保留 + 双类名采用路径 + 5 个指定采用点 + 删除条件** | §9.1 |
| 5 | 8px 圆角已是事实标准（≈240 处）却不在 4/6/10/14 阶梯 | **不加入阶梯**，全部归一到 6/10 | §9.2 |
| 6 | `.nav-badge` 用 `--ant-color-error`（第二强调色）且白字对 `#f0615c` = **3.20:1** | 改用信号**填充档** + `--signal-ink`（9.22 / 5.64） | §2.3、附录 A.4 |

---

## 1 分层模型：L0–L6

共享层的类名不是一堆平铺的名字，而是**分层**的。判断"一个新样式该写在哪"用这一层序：

| 层 | 内容 | 唯一写者 | 允许谁引用 |
|---|---|---|---|
| **L0 令牌** | `theme.tsx` 的 `D`/`L` + antd `token`/`components`；`index.css` 的 `--*` 变量 | systems | 所有层 |
| **L1 原子** | 文字与内联：`.mono .micro .time .muted .grow .truncated .title .pad .sr-only` | systems | 所有层 |
| **L2 容器** | `.panel .card .zone* .readout-strip .readout* .timeline-wrap .kanban-col .chat-side` | systems | 视图 |
| **L3 可交互** | `.row-btn .readout-btn .kanban-card .guide-card .suggest-chip .mem-preview .neighbor .ws-new-btn .icon-btn .row-action .cmdk-item` | systems | 视图 |
| **L4 状态标记** | `.dot .pill .tag(.ok/.warn/.err) .live-dot .live-flag .nav-badge .conn .build-id` | systems | 视图 |
| **L5 状态区块** | `.empty-state* .spinner-block .error-state .md*` | systems | 视图（经 `ui.tsx`） |
| **L6 视图私有** | `.graph-*`（图谱画布）+ 各视图自己的网格类 | 视图成员 | **不得互相引用** |

**判据**：任何新类都必须能用一句话写成"它是 L? 的第 N 个成员"。
写不出这句话的类就是漂移——§10.4 的删除清单就是这么找出来的。

**为什么 L6 要单列**：MASTER §3.2 W6 已冻结"图谱分类色不得离开图谱视图"。
L6 是这条规则的结构化表达：`.graph-*` 只允许出现在 `#graph` 的 DOM 里。

---

## 2 令牌契约

### 2.1 模式变量表（`index.css` 现状 → 契约值）

| 变量 | 暗（现状 → 契约） | 亮（现状 → 契约） | 动作 |
|---|---|---|---|
| `--app-shell` | `#08090b` | `#eaebee` | 不变 |
| `--surface-hover` | `#262a31` | `#f0f1f4` | 不变 |
| `--surface-selected` | `#333844` | `#e4e7ec` | 不变 |
| `--panel-shadow` | `inset 0 1px 0 rgba(255,255,255,.055)` | `0 0 0 1px var(--rule-soft), 0 1px 2px rgba(15,20,30,.05)` | **形状不变，环的取值随 `--rule-soft` 改**：亮色对 pageCanvas **1.2330:1** / 对白 **1.3217:1**（t30 实测；旧值 1.1340 / 1.2155，MASTER 行 30） |
| `--surface-shadow` | = panel | = panel | **删除别名**，只留 `--panel-shadow`（§10.4 D7） |
| `--rule` | `rgba(255,255,255,.13)` | `#d6d9df` | 不变（装饰线，不受 3:1 约束） |
| `--rule-soft` | `rgba(255,255,255,.07)`（**未动**） | **`#dee0e4`**（原 `#e7e9ed`） | **改值（t30）**：L = 0.744423 → 对 pageCanvas **1.2330:1** / 对白 **1.3217:1**（旧 1.1340 / 1.2155）。改值依据 MASTER §12.7.1 |
| `--control-border` | **新增** `rgba(255,255,255,.34)` | **新增** `#8a9198` | C7（§2.2） |
| `--signal`（填充档） | `#f0a93b` | `#8a5600` → **`#c88413`** | C6（§2.4） |
| `--signal-text` | **新增** `#f5b457` | **新增** `#8a5600` | C6（§2.4） |
| `--signal-ink` | `#1a1206` | `#241703` | 不变 |
| `--signal-wash` | `rgba(240,169,59,.14)` | `rgba(224,160,42,.16)` | 不变 |
| `--focus-ring` | **新增** `#f5b457` | **新增** `#8a5600` | A1（§5.1） |
| `--status-ok` | `#4fb477` | `#17793b` | 不变 |
| `--status-err` | `#f0615c` | `#c4121c` | 不变 |
| `--status-warn` | `#f0a93b` → **`#e2894f`** | `#8a5600` → **`#8a4a12`** | C2（§2.3） |
| `--status-idle` | `#838891` | `#656a73` | 不变 |
| `--graph-edge` | `rgba(255,255,255,.34)` | `rgba(15,23,42,.34)` → **`.50`** | C4（§2.4） |
| `--graph-*`（其余 11 个） | 见 MASTER §3.1 F | 见 MASTER §3.1 F | 不变 |
| `--ant-color-primary-border` | **新增 pin** `var(--focus-ring)` | **新增 pin** `var(--focus-ring)` | A1（§5.1） |

**变量书写规则（可 grep 判定）**：`index.css` 中任何 `--x: var(--y)` 的别名不得超过一层；
`--surface-shadow` 是当前唯一的双层别名，删除。

**镜像对与隐藏依赖（t30 实测登记）**：亮色 `--rule-soft`（`index.css` 的**字面量**）与
`L.borderSecondary`（`theme.tsx`）是**同一档的两个写法**（MASTER §2.1 B4「= B2」；
`index.css:22` 亦写着「Keep the hexes in sync with theme.tsx」）。而 panel 的**环**由
`--panel-shadow: 0 0 0 1px var(--rule-soft)` 提供 —— `--rule-soft` **不引用**
`L.borderSecondary`。**因此「改 ring」必须两处同改**：只改 `theme.tsx` 的
`borderSecondary`，环与 5 条分隔线一个像素都不会变（t30 实测：只改 TS 侧时 ring 仍是
1.1340:1）。两处现值均为 `#dee0e4`；`--rule`（`#d6d9df`）与暗色两个值均未动。

### 2.2 `--control-border`（C7）：表单控件边界

**问题**：控件边界现在用 `--rule`（antd `colorBorder`）——暗 **1.49:1** / 亮 **1.41:1**，
低于 WCAG 1.4.11 对"识别输入框所必需"的图形要求的 3:1（MASTER §2.4）。

**契约值（按 alpha 与底线性合成后实测）**：

| 模式 | 值 | 对 panel | 对 canvas |
|---|---|---|---|
| 暗 | `rgba(255,255,255,0.34)` → 合成 `#696a6e` | **3.09:1** ✔ | 3.08:1 ✔ |
| 亮 | `#8a9198` | 对白 **3.19:1** ✔ | 对 canvas 2.98:1（差 0.02） |

**亮色取值说明**：控件（`.ant-input`/`.ant-select-selector`/`.neighbor`）都坐在 panel
（白）上，3.19:1 达标；对 canvas 的 2.98 是**未落地的理论组合**，t7 须实测确认控件
不在 canvas 上裸放（附录 A.6）。

**实现边界（避免波及装饰线）**：
1. `theme.tsx` 的 `colorBorder` 改为 `--control-border` 的对应值（控件边界用），
   `colorBorderSecondary` 保持 `--rule-soft`。
2. **实测更正（本文复测；载体计数 t32 重新核定）**：`index.css` 的结构性分隔线**已经**用
   `--rule`（5 处）/ `--rule-soft`（**6 处**，原写 7 —— 口径见下）/
   `--ant-color-border-secondary`（26 处），**不需要"批量显式化"**。

   **`--rule-soft` 的计数口径（t32，2026-09-21 复测）—— 「声明次数」与「命中元素数」不是一回事**：

   - **声明次数 = 6**（源码剥注释 = 构建产物 = 6）：**1 个环**（`--panel-shadow`）+ **5 条 border**
     （`.brand` :217 / `.sidebar-foot` :278 / `.view-bar` :331 / `.chat-side` :1164 /
     `.composer` :1407）。命令与原始输出：

     ```bash
     # A) 源码，剥掉 /* */ 后只数 var() 引用
     python -c "import re;s=open('panel/src/index.css',encoding='utf-8').read();print(len(re.findall(r'var\(--rule-soft\)',re.sub(r'/\*.*?\*/','',s,flags=re.S))))"
     # -> 6
     # B) 构建产物（压缩器已剥注释，与 A 一致）
     grep -o 'var(--rule-soft)' panel/dist/assets/index-*.css | wc -l
     # -> 6
     # C) 对照口径：不剥注释 —— 会偏大，勿用
     grep -o 'var(--rule-soft)' panel/src/index.css | wc -l
     # -> 7（多出的 1 处是 index.css:111 的注释文本，不是声明）
     # D) 对照口径：连定义行一起数 —— 更偏大，勿用
     grep -o -- '--rule-soft' panel/src/index.css | wc -l
     # -> 9（2 处定义 + 6 处声明 + 1 处注释）
     ```

     **原写的「7 处」现在只能由 C 口径复现**（含注释的字符串计数）。历史值无法从当前文件回溯
     （文件此后被 t18/t26/t29/t30 改过），但结论明确：**以声明为准的正确值是 6**。
     本项目已多次出现「同一指标两种口径」，故此处把口径写在数字旁边。

   - **命中元素数（运行时，亮色 13 路由实测）**：5 条 border 的载体合计命中 **40 次元素**
     （`.brand` 13 / `.sidebar-foot` 13 / `.view-bar` 12 / `.chat-side` 1 /
     `.composer` 1），环（`box-shadow`）另命中 **63 个元素**（home 5 / chat 0 / sessions 2 /
     board 22 / task 3 / memory 10 / knowledge 2 / graph 2 / agents 9 / runtimes 4 / stats 3 /
     settings 1 / inbox 0）。**另有 15 处**元素的同色边框来自 `--ant-color-border-secondary`
     （`.ant-layout-sider` 右 13 + `.timeline-head` 1 + `th.ant-table-cell` 1）——
     同色但**不同令牌**，不计入本行。
   全文只剩 **2 处**用到 `colorBorder`：`.md blockquote`（`:678`，装饰引用线）与
   `details.chat-thought[open] .thought-body`（`:1142`，装饰竖线）——这两处改 `var(--rule)`，
   把"控件边"与"装饰线"彻底分开。**因此 C7 的 index.css 面只有 3 行**：
   `.neighbor` 的 `border` 改 `--control-border` + 上述 2 处。
3. 表格线走 `colorBorderSecondary`（现行 ✔），不受影响。
4. **更正**：`.neighbor`（`:786`）的边界用的是 `var(--rule)`（1.49:1），**不是** antd 的
   `colorBorder`。它是"可跳转的邻居块"（控件，不是装饰），因此归入控件边：
   `1px solid var(--control-border)`（1.49 → 3.09），hover 仍另走 primary（M45）。

### 2.3 信号唯一性与状态映射（C2）

**规则（可机器判定，阈值行 32）**：**运行时品牌族** `--brand-claude` / `--brand-deepseek` /
`--brand-opencode` 与 `--signal` / `--status-warn` **两两互不相同**（检查集 5 个）；
`--status-warn` 与 `--signal` 每模式 **Δhue ≥ 8°**。**accent（`--brand` / `--ant-color-primary`）
不在检查集内** —— 它 `=== --signal` 是刻意设计（单一信号色通道，见 §2.4 C6），显式排除。
判据落槌处：MASTER §12.3

**契约值（本文复算）**：

| 令牌 | 暗 | 亮 | 与 `--signal` 的关系 |
|---|---|---|---|
| `--signal`（填充） | `#f0a93b`（hue 36.5°） | `#c88413`（hue 37.5°） | — |
| `--status-warn` | `#e2894f`（hue **23.7°**） | `#8a4a12`（hue **28.0°**） | Δhue **12.8° / 9.4°** ✔ |
| warn 对底 | panel **6.30** / canvas 7.17 / shell 7.52 | 白 **6.84** / canvas 6.38 / shell 5.74 | 全部 ≥4.5 ✔（它是 11px 标签文本档） |

**状态 → 令牌的最终映射（`ui.tsx` `STATUS_COLORS` 是唯一定义处）**

| 状态 | 现行 | 契约 | 理由 |
|---|---|---|---|
| `done` / `completed` | `--status-ok` | 不变 | 终态·成功 |
| `failed` / `cancelled` | `--status-err` | 不变 | 终态·失败 |
| **`interrupted`** | `--status-warn` | **`--status-err`** | 它是**过去时**（已中断），语义与 `cancelled` 同类；两类终态·失败共用 err，由**文本标签**区分（`t('status.interrupted')` = 已中断） |
| `in_progress` / `running` / `spawning` | `--ant-color-primary` | **`--signal`** | 进行中 = "现在"；改用自有变量，不依赖 antd 派生（浅色派生会漂） |
| **`waiting_permission` / `blocked`** | `--status-warn` | **`--signal`** | 字面就是"等你处理"，命中白名单 **W3** |
| `queued` / `pending` | `--status-idle` | 不变 | 未开始 |

**由此收敛出的颜色语义（写进契约，供 t16 对齐）**：

> **颜色编码"活性类别"，文本编码"精确状态"。** 全站只有四类语义色：
> 现在（信号）/ 终态·成功（ok）/ 终态·失败（err）/ 未开始（idle）。
> `--status-warn` **不再表示任何运行状态**，它只用于 L4 的**标签**（`.tag.warn`）
> 与"子系统降级"标记（`#home` 的 embedder 回退点，且必须与同行的文字标签同现）。

**`.tag.warn` 的现存使用面（8 处，全部是"降级/陈旧"语义，不是运行状态）**：
`mcp.down`（Agents:188）、`memory.superseded`（Memory:215,359）、`wiki.staleTag`
（Memory:591,604 / Wiki:130）、`task.judgeNoVerdict`（TaskDetail:631）、
`chat.readonlyHistory`（Chat:1246）、Wiki:160 的 `"warn"` 分支
→ 语义与"降级"一致 ✔，**视图无需改动**。

### 2.4 逐条实测的取值裁决（C3 / C4 / C6 / C9 / C10）

**C3 — 亮色 `.tag.ok`。** 现状 11px `#14691f` on `rgb(173,184,175)` = **3.34:1**（全站最差正文对比度）。
契约：亮色下三个 tag 变体**全部钉死**（不再依赖 antd 派生底色）：

| 变体 | 文本 | 洗色底（= 变体色 10% 于白） | 对比度 |
|---|---|---|---|
| `.tag.ok` | `#14691f` | `#e8f2eb` | **5.97:1** ✔ |
| `.tag.warn` | `#8a4a12` | `#f3ede7` | **5.89:1** ✔ |
| `.tag.err` | `#c4121c` | `#f9e7e8` | **5.11:1** ✔ |

洗色一律 **10%**：实测 16% 时 `.tag.err` 掉到 **4.60:1**，只剩 0.10 冗余（附录 A.3）。
同时**删除** `.tag.ok/.warn/.err` 中的三条 `border-color` 声明（`.tag` 已是 `border: none`，
这三条是惰性声明 → §10.4 D3）。

**C4 — 亮色 `--graph-edge`。** `.34` = **2.17:1**（白）/ 2.51（canvas）。
契约：`.50` → **3.41:1**（白）/ **3.19**（canvas）✔。`.46` 只有 3.03，冗余太小，不取。

**C6 — 信号拆档。** 现状：同一个 token 在暗色是亮琥珀（L 0.472）、亮色是深褐（L 0.121），
同时充当"填充"和"文本"。契约：**填充档 `--signal`、文本档 `--signal-text`**，两模式对称：

| 用途 | 暗 | 亮 | 判据 |
|---|---|---|---|
| 填充档 `--signal`（`.live-dot` 6px、`.inbox-card` 脊线、`.nav-badge`、`.readout.signal` 的点） | `#f0a93b` | `#c88413` | 非文本标记 ≥3:1：暗 8.30 / 亮 **3.10** ✔ |
| 文本档 `--signal-text`（= `--ant-color-link`；`.readout.signal` 的数字、`.live-flag`） | `#f5b457` | `#8a5600` | 文本 ≥4.5:1：暗 9.18（panel）/ 亮 **6.16**（白）✔ |
| 填充上的字 `--signal-ink` | `#1a1206` | `#241703` | 暗 9.22 / 亮 **5.64** ✔ |

**为什么亮色填充档必须从 `#e0a02a` 改为 `#c88413`**：`#e0a02a` 对白只有 **2.27:1**
（对 canvas 2.12）——6px 的 `.live-dot`、3px 的 `.inbox-card` 脊线、11px 的 `.nav-badge`
都达不到各自下限。改后 3.10:1 ✔。
**副作用（登记并接受）**：亮色主按钮"字/底"从 7.70 → **5.64:1**，仍高于 4.5 下限 1.14 点。
**同源声明**：亮色 `L.brand`（`#c88413`）与 `--signal`（`#c88413`）**刻意同值** —— accent 是
`--signal` 的 antd 渲染路径，不是第二个信号色。因此阈值行 32 的唯一性检查**只读运行时品牌族**
（`--brand-*`），accent 显式排除（MASTER §12.3）。

**C9 — 亮色文本梯度。** 现状 `textTertiary` 在 canvas **4.45** / shell **4.00** ✘，
且 `textQuaternary` 在 canvas **5.07** 反而更可读（梯度非单调）。
契约（**把 MASTER 建议的两个 hex 对调**）：

| 级 | 亮值 | 对白 / canvas / shell | 判定 |
|---|---|---|---|
| `textSecondary` | `#5b6068` | 6.33 / 5.90 / 5.31 | 不变 |
| `textTertiary` | `#5f646d`（MASTER 给 quaternary 的那个） | **5.95 / 5.55 / 4.99** | ✔ |
| `textQuaternary` | `#656a72`（MASTER 给 tertiary 的那个） | **5.44 / 5.08 / 4.57** | ✔ |

→ 亮色四阶严格递减（19.43 > 6.33 > 5.95 > 5.44，全体 ≥4.5），且不再与任何档“撞值”。
**不采纳 MASTER C9 原始映射**的理由：按原文映射后 canvas 上 tertiary 5.08 < quaternary 5.55，
梯度仍然非单调（附录 A.1）。

**C10 — `::selection`。** 契约：一条规则、不分模式、不引入第三色相：
`::selection { background: color-mix(in srgb, var(--signal) 26%, transparent) }`，
删除现有两条靛蓝规则（§10.4 D2）。
**登记**：选区颜色不在 §12 任何阈值行的判据内，其字/底对比度是**未测项**（附录 A.5），
不假装达标。

### 2.5 阶梯（引用 MASTER，落为可 grep 的名字）

| 维度 | 唯一合法集合 | 共享层落点 |
|---|---|---|
| 字号 | 11 / 12 / 13 / 14 / 15 / 16 / 18 / 20 / 24 / 32 | §3.1、§10.1（T1/T2/T5 全采纳） |
| 行高 | 15 / 16 / 18 / 21 / 22 / 24 / 26 / 30 / 40 | 同上 |
| 字重 | 400 / 500 / 600 / 700（仅 `<strong>`） | 450 → **500**；650 → **600** |
| 圆角 | 4 / 6 / 10 / 14 / 50% / 999px | §9.2（**不含 8**） |
| 间距 | 2 / 4 / 6 / 8 / 10 / 12 / 16 / 20 / 24 / 32 / 80 | §10.1（`clamp()` 产出豁免） |
| 时长 | 100 / 120 / **200** / 260 | `.chat-side` 的 200/150 → 200 / 120 |
| 断点 | 520 / 768 / 1024 / 1240（+ 契约值 992） | §10.1 M-media |
| 缓动 | `cubic-bezier(0.16,1,0.3,1)` 与 linear | 不变 |

### 2.6 `theme.tsx` 的 antd 令牌契约

| antd token | 现 | 契约 | 依据 |
|---|---|---|---|
| `colorPrimary` | `D.brand` / `L.brand` | `L.brand` → `#c88413`；`D.brand` 不变 | C6；**与 `--signal` 同值是契约要求**，MASTER §12.3 已把 accent 排除出行 32 的唯一性检查 |
| `colorLink` | `D.link` / `L.link` | 不变（= `--signal-text` 的同源值） | C6 |
| `colorWarning` | `#f0a93b` / `#8a5600` | **`#e2894f` / `#8a4a12`** | C2 |
| `colorPrimaryHover` | antd 派生（暗色**变暗**） | **pin** `D:#f5b457` / `L:#b8760f` | C8 |
| `colorPrimaryActive` | antd 派生 | **pin** `D:#f7c073` / `L:#a96a0c` | C8 |
| `colorBorder` | `D.border` / `L.border` | **`--control-border` 的值** | C7 |
| `colorBorderSecondary` | `D.borderSecondary` | 不变 | — |
| `colorText*` 四阶 | 不变 | 亮色后两阶换成 C9 的值 | C9 |
| `Statistic.contentFontSize` | `30` | **`32`** | MASTER T5（读数归一，复用 `.readout`） |
| `Card.paddingLG` | `18` | **`20`** | MASTER §5.3 |
| `Menu.itemBorderRadius` | `6` | 不变 | 阶梯 |
| `controlOutline*` | antd 默认 | **不动**（焦点环走 `colorPrimaryBorder` pin，§5.1） | A1 |

---

## 3 原语清单（职责 / 类名 / 变体 / 组件签名 / 归属）

> 记法：**类** = 唯一写者 systems；**签** = `ui.tsx` 导出；**变体** = `.a.b`。
> 全部原语**不得**接受内联 `font-size` / `color` / `border-radius`
> （阈值行 11 要求内联 `font-size` = 0；行 12 要求内联 style ≤50/路由）。

### 3.1 L1 原子（文字与内联）

| # | 原语 | 类名 | 职责（唯一） | 变体 | 取值 |
|---|---|---|---|---|---|
| A1 | mono 数据 | `.mono` | 唯一的机器数据档：id / hash / slug / 计数 / 时间戳 / 代码 | — | 12/16 mono，`tabular-nums` |
| A2 | 次级文本 | `.muted` | 次级说明文本 | 可叠 `.micro` | 色 = `textTertiary` |
| A3 | 微文本 | `.micro` **（新增）** | **11/15 尺寸档**（时间戳/计数/kbd 提示） | 与 `.muted`/`.time` 叠用 | 11/15，**不设 color** |
| A4 | mono 微文本 | `.time` | 时间戳/构建号 | — | 11/15 mono `quaternary` |
| A5 | 弹性填满 | `.grow` | `flex:1; min-width:0`（网格/行里防溢出） | — | — |
| A6 | 单行截断 | `.truncated` | 长 id/path 单行省略 | — | `max-width:340px` |
| A7 | 行标题 | `.title` | 行内主标题：占满剩余宽度并省略 | — | 13/18 |
| A8 | 统一内边距 | `.pad` | 面板内边距的**唯一**工具类 | — | **16/20**（原 14/18） |
| A9 | 视觉隐藏 | `.sr-only` **（新增）** | 给读屏的 `<h1>` 等，不占布局 | — | §5.5 |
| — | ~~`.nums`~~ | **删除** | 0 引用；`tabular-nums` 已由 `.time/.mono/table` 承担 | — | §10.4 D1 |

**为什么 `.micro` 不设颜色**：颜色是**语义**（这一行文本有多重要），尺寸是**角色**
（这是微文本）。把两者正交拆开后，`.muted.micro`（13px 级的灰）与
`.micro`（11px，继承父色）都能表达，而 sessions 的 314 处
`<span className="muted" style={{fontSize:11}}>` 只需改成
`className="muted micro"`（**双类名，不改语义，删内联**）。

### 3.2 L2 容器

| # | 原语 | 类名 | 职责 | 变体 | 关键取值 |
|---|---|---|---|---|---|
| B1 | 面板 | `.panel` | 默认有界面：材质 + 圆角 10 + 顶边光（亮色 ring） | — | 内边距由子元素定 |
| B2 | 卡片 | `.card` | = 面板 + 底部间距 | — | `padding 16px 20px`；`margin-bottom 12px` |
| B3 | 区带 | `.zone` | **不包围**的区带 | — | 仅 `min-width:0`；无背景/描边/圆角 |
| B4 | 区带头 | `.zone-head` | 规则线 + 标签 + 右侧动作 | — | `border-bottom: 1px solid var(--rule)`；内距 10 |
| B5 | 区带标题 | `.zone-title` | 15/22 600 sans | — | 与 `h3.sec` **同档**（MASTER 已确认） |
| B6 | 区带注 | `.zone-note` | 13/18 `tertiary` 的一句话 | — | — |
| B7 | 仪表条 | `.readout-strip` | 页首仪表盘：2–4 个读数，**规则线分隔**而非四个盒子 | **`.grid`**（新增） | `padding 16px 20px`；分隔线用 `box-shadow: -1px 0 0 var(--rule)` |
| B8 | 仪表单元 | `.readout-cell` | 一个读数（不可点的） | — | — |
| B9 | 读数 | `.readout` | **唯一**的大字号档：32/40 mono 500 | **`.l`** 24/30、**`.s`** 18/24、**`.signal`**（数字用 `--signal-text`） | `tabular-nums` |
| B10 | 读标签 | `.readout-label` | 读数的说明（**sans**） | — | 12/**16**（原 12/17） |
| B11 | 看板列 | `.kanban-col` | **列不是盒子**：标题在规则线上，卡片才是实体 | — | `min-height:180px` |
| B12 | 时间线容器 | `.timeline-wrap` | 飞行记录仪：一块面板 | — | — |
| B13 | 浮层 | `.modal` / `.modal.wide` / `.cmdk` | 阴影 + 圆角 14 + **无顶边光** | — | 见 §4 |

**`.readout-strip.grid`（新增修饰符）**：为"没有横向空间、但 MASTER P3 仍要求开场有仪表"
的视图提供网格排布。

**当前机制（t26 + t29 后的真实实现）**：该修饰符**只在 `≤520` 产生规则**（整块位于
`@media (max-width: 520px)` 内），且列定义**只有一条** ——
`display: grid; grid-template-columns: repeat(2, minmax(0, 1fr))`；分隔线由 `box-shadow`
改为 `1px 0 0 var(--rule)` 的网格线（保留"规则而非盒子"）。**`>520` 时该修饰符不产生任何规则**，
条带回落为 `.readout-strip` 的 flex 默认（**含 `max-width: 320px` 单元上限**）——
因此**在宽视口传 `grid` 是空操作**，8 条路由可以用同一种调用范式（`<ReadoutStrip grid …>` 无条件传）。

> **原设计意图（已被取代，2026-09-21）**：B7 最初把该修饰符定义为
> `display:grid; grid-template-columns: repeat(auto-fit, minmax(140px,1fr))`，且**不带媒体查询**
> （在任意宽度都生效）。t26 把整块移进 `≤520` 媒体查询后，这条 auto-fit 声明**在任何宽度都不生效**
> （`≤520` 被同块内的 2 轨规则覆盖；`>520` 整块不匹配），t29 按 §10.4 的方式**删除**它。

**B7 的 `≤520` 特化与前置条件（2026-09-21 增补；t18 落地，t25 补注，t26/t29 修订）**

该修饰符**只在 `≤520` 生效**（t26 把整块移进媒体查询），且**只有一条列定义**（t29 删掉了不可达的
auto-fit 声明）。`panel/src/index.css` 的实际形态：

```css
@media (max-width: 520px) {
  .readout-strip.grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); }
  .readout-strip.grid > * { box-shadow: -1px 0 0 var(--rule); padding-inline: 20px; }
  .readout-strip.grid > :first-child { box-shadow: none; padding-inline-start: 0; }
}
```
**`>520` 时该修饰符不产生任何规则**：条带回落为 `.readout-strip` 的 flex 默认（含
`max-width: 320px` 单元上限），所以在宽视口传 `grid` 是空操作。

- **为什么是 2 轨而不是 auto-fit 的 140px 地板**：`@390` 视口实测 `.content` 内容盒 **294px**，
  减去条带自身 `padding: 16px 20px` → 网格内容盒 **294 − 40 = 254px < 280px**，
  `repeat(auto-fit, minmax(140px,1fr))` 在这种内容盒里只能放下 1 轨 → **1 列 × 4 行**（条高 264px）；
  写死 2 轨后实测 **2 轨 × 2 行**。同一份单元内容下条带高 **264px → 148px**（省 116px，两组同内容可比）；
  条带的绝对高度由读数内容决定 —— `view-home.md` 与 `view-memory.md` 用**真实读数**实测
  **164px**（= 2 × 66px 单元 + 32px 上下 padding），与 148px（合成单元）不矛盾。
- **前置条件（写死，供下一个实现者直接判断）**：2×2 需要**网格内容盒 ≥ 280px**
  （= 2 × `minmax(140px,1fr)` 的 140px 地板）→ **视口 ≥ 约 416px**。
  `@416` 实测：`.content` 内容盒 320px → 网格内容盒 **280px** → 两轨各 **140px**，恰好成立；
  `@520` 实测两轨各 192px；`@390` 靠特化拿到两轨各 **127px**（低于 140px 地板，这是特化的代价）。
  **不要为此把地板降到 110px** —— 那会把 520–768 变成 3 列。
- **>520 与 flex 版逐项一致（零行为差异，t29 复测）**：`520 / 768 / 1024 / 1280 / 1440 / 1920`
  六档 × 2 项条（sessions / knowledge）与 4 项条（board / stats / home / memory）的 A/B（同一加载内
  强制加/去 `grid` 类）**全部 EQUAL**；实测单元宽 `@768` = 158（4 项）/ 316（2 项）、
  `@1440` = 277.16 / 277.17、`@1920` = 320（被 flex 的 320px 上限封顶）。
- **没有引入新断点**：`520` 是本文已批准的断点阶梯（§10.1 M91 / 阈值行 35：520 / 768 / 1024 / 1240）
  之一，本特化只往该断点内加规则，未新增断点值。
- **修正登记（日期与缘由）**：**2026-09-21**。缘由：原 B7 公式与 README §3.4 X4
  （「≤520 的 2×2 排布统一用 `.readout-strip.grid`」）**在 254px 内容盒下数学上不可同时成立** ——
  `auto-fit` + 140px 地板在 254px 里只能产生 1 轨，X4 的 8 条路由（home / board / memory /
  sessions / knowledge / graph / runtimes / stats）因此没有可落地的判定条件。本次修正补上
  「≤520 由 2 轨特化接管」这一层，并写明 2×2 的内容盒前置条件。
  随后两轮：**t26** 把该修饰符整块移进 `@media (max-width: 520px)`（消除 `>520` 的几何偏移：
  2 项条从 1024 起 track > 320、4 项条从 ~1668 起）；**t29** 删除因此永不生效的 auto-fit 声明
  （§10.4 登记，扫描方法见附录 B.7）。实测与落地位于 `panel/src/index.css` 的
  `.readout-strip.grid` 注释。

**新增组件签名（`ui.tsx`）**

```ts
/** 区带语义糖：DOM 与手写 .zone* 完全一致（不新增层级语义）。 */
export function Zone(props: {
  title?: ReactNode;      // → .zone-title
  note?: ReactNode;       // → .zone-note
  actions?: ReactNode;    // 区带头右侧
  className?: string;     // 透传：视图可并写 .kanban-col 等冻结类（§9.1）
  children: ReactNode;
}): JSX.Element;
```

### 3.3 L3 可交互原语

| # | 原语 | 类名 | 职责 | 状态覆盖 | 取值 / 变更 |
|---|---|---|---|---|---|
| C1 | 行按钮 | `.row-btn` | 列表行 = 可点击实体 | rest / hover 100ms / **active 新增** / focus / selected / disabled | `padding 9px 12px`、`margin 0 -12px`、圆角 **10**（原 8）、hover 面、selected 用 `--surface-selected` |
| C2 | 仪表按钮 | `.readout-btn` | 可跳转的读数 | hover / focus / **active** | 圆角 6 |
| C3 | 看板卡片 | `.kanban-card` | 可点击实体 | hover / focus / **active** | 圆角 **10**（原 8） |
| C4 | 指南卡 | `.guide-card` | 可点击实体 | hover / focus / **active** | — |
| C5 | 建议 chip | `.suggest-chip` | 一次性输入建议 | hover / focus / **active** | — |
| C6 | 记忆预览 | `.mem-preview` | 可展开的预览行 | hover / focus / active | — |
| C7 | 邻居块 | `.neighbor` | 可跳转的相邻实体 | hover / focus / active | `border-color` =  **`--control-border`**（唯一允许带框的跳转块） |
| C8 | 工作区新建 | `.ws-new-btn` | 悬停显形的小按钮 | hover / focus；`hover:none` 常显 | 22×22 → **≥24×24**（§5.4） |
| C9 | 图标按钮 | `.icon-btn` **（新增基类）** | 唯一允许的图标按钮形态 | hover / focus / active / disabled | `min 24×24`、圆角 6、`padding 0 4px`；**必须 `aria-label`** |
| C10 | 行内动作 | **常显**（原 `.row-action` 门控类已删；现为 `.row-btn` 内的 `button.icon-btn`） | **常显**的行操作 | **常显（2026-09-23 起，t64 落地）**；原「悬停显形 120ms；`hover:none` 常显；`:focus-within` 显形」保留为历史（见下） | 必须位于 `.row-btn` 内；命中区按 C9 的 **min 24×24**（§5.4）。**登记**：#sessions 常显后 39 个 24×24 进入行 18 的「内容区<32px」计数 ⇒ 39 > ≤10，见 MASTER §12.13 |
| C11 | 命令项 | `.cmdk-item` | 浮层里的选项 | hover / focus / **selected**（`aria-selected` 必须同步） | 圆角 6；**focus 环 `outline-offset: -2px`**（列表可滚动，外扩环会被裁） |

**C10 为什么推翻「悬停门控」（2026-09-23 裁决，captain）**：门控的初衷是「200 个常驻图标会把标题挤没」，
但**实测证明它没换来布局收益** —— `opacity` 不占布局，图标两态都占那 24px；`1440` 标题宽 **441px 两态完全相同**；`390/768/1440` 横向溢出全 0。
代价却是**可发现性**：**用户直接投诉「chat 页面，对于 session 没有删除或者归档的按钮」** —— 那正是门控造成的。
⇒ **可发现性 > 视觉安静**，行级动作**常显**。原规则（`index.css` 的 `.row-action { opacity: 0 }` 及 hover/`:focus-within` 显形）已随最后一个使用者（t64）删除，`t66` 清掉 CSS 规则。

**`:active` 契约（MASTER A2，全站当前 0 条）**：手写可点元素必须有一条按下反馈，
取值一律 `background: var(--surface-selected)`，**不位移、不缩放**
（MASTER §9.3 铁律 1）。要求覆盖的选择器（= 阈值行 34 的判据）：
`.row-btn` / `.kanban-card` / `.guide-card` / `.suggest-chip` / `.readout-btn` /
`.mem-preview` / `.neighbor` / `.icon-btn` / `.cmdk-item`。

**`.icon-btn` 基类为什么必须新增**：`.icon-btn` 在 `Chat.tsx:1100` 已被使用，
但 `index.css` 里只有一条 `.chat-side-head .icon-btn { color: ... }`——
**没有基类规则**。命中区下限（§5.4）与"图标按钮必须有可访问名称"（阈值行 17）
都需要一个唯一的落点。

**签名（`ui.tsx` 新增）**

```ts
/** 图标按钮：把"命中区 ≥24 + 必须有可访问名称"变成类型约束。 */
export function IconButton(props: {
  label: string;          // 必填 → aria-label（阈值行 17 要求无名称交互元素 = 0）
  icon: IconName;
  onClick?: () => void;
  size?: number;          // 图标尺寸，默认 16；外框恒 ≥24×24
  disabled?: boolean;
  className?: string;
  title?: string;
}): JSX.Element;
```

### 3.4 L4 状态标记

| # | 原语 | 类名 | 职责 | 变体 | 取值 |
|---|---|---|---|---|---|
| D1 | 状态点 | `.dot` | 8px 圆，非文本标记（≥3:1） | 色来自 `StatusDot` | 8px |
| D2 | 状态胶囊 | `.pill` | **运行状态**：文本 + 自身色 15% 洗底 | 色来自 `StatusPill` | 999px、11/16 |
| D3 | 标签 | `.tag` | **元数据**标签（不是状态） | **`.ok` / `.warn` / `.err`** | 4px、11/16 mono、省略 |
| D4 | 实时点 | `.live-dot` | 6px：正在流式输出 | — | `--signal` |
| D5 | 实时旗 | `.live-flag` | 文字：正在运行 | — | `--signal-text` + pulse |
| D6 | 计数徽标 | `.nav-badge` | 收件箱待处理数 | — | **`--signal` + `--signal-ink`**（原 error 红，附录 A.4） |
| D7 | 连接态 | `.conn` / `.ok` / `.err` | 守护进程连接 | — | 11/15 mono |
| D8 | 版本号 | `.build-id` | 构建号 | — | 11/15 mono `quaternary` |

**`.pill` 与 `.tag` 的区别是语义，不是外观**（写进契约，供 t16 对齐）：
`.pill` 表示"运行/状态"，`.tag` 表示"元数据"（项目、模型、来源、陈旧标记）。
不得因为"看起来都是小圆角片"就把状态塞进 `.tag`。

**签名（已有，契约化）**

```ts
export function StatusDot(props: { status: string }): JSX.Element;
export function StatusPill(props: { status: string }): JSX.Element;
// 颜色来源唯一：STATUS_COLORS（§2.3）；表外状态一律落 --status-idle
// （现行 `?? "var(--status-idle)"` 保留，禁止改成 inherit）
```

### 3.5 L5 状态区块

| # | 原语 | 类名 | 职责 | 变体 | 取值 / 变更 |
|---|---|---|---|---|---|
| E1 | 空态 | `.empty-state` + `.empty-mark` + `.empty-copy` + `.empty-title` + `.empty-hint` + `.empty-action` | **邀请**：图形 + 一句话 + 一个动作 | — | `.empty-mark` 圆角 **10**（原 12）；`.empty-title` 16/24 600；`.empty-hint` 13/**18**（原 13/20） |
| E2 | 空态图 | `.empty-state .ant-empty-image` | 内边距由 **CSS** 拥有 | — | `margin-bottom:16px`；**删除 `ui.tsx` 的内联 `imageStyle`**（附录 A.2） |
| E3 | 载入块 | `.spinner-block` **（新增）** | 把 `Spinner` 的内联样式搬进 CSS | — | `display:flex;column;align:center;gap:12;padding:72px 0` |
| E4 | 错误块 | `.error-state` **（新增）** | 与空态**必须可区分**：`role=alert` + 重试 | — | 见下 |
| — | ~~`.state.empty`~~ | **删除**（1 处使用迁移） | 第二套空态实现 | — | §10.4 D5 |
| — | ~~`.empty-icon`~~ | **删除** | 0 引用 | — | §10.4 D4 |

**三态契约（MASTER P6 推论：error 必须与 empty 可区分）**：

| 态 | 谁渲染 | 判定钩子（t7 用） |
|---|---|---|
| loading | `Spinner` → `.spinner-block` | 存在 `.spinner-block` 或 `.ant-spin` |
| empty | `Empty` → `.empty-state` | 存在 `.empty-state`，**且不存在** `[role=alert]` |
| error | `ErrorState` → `.error-state` | 存在 `[role=alert]`，**且不存在** `.ant-empty` |

**签名（`ui.tsx` 新增 / 修改）**

```ts
/** 载入：内联样式搬进 .spinner-block（阈值行 12）。 */
export function Spinner(props: { label?: string }): JSX.Element;

/** 错误：与空态可区分（阈值行 20）。antd Alert（自带 role=alert）+ 重试按钮。 */
export function ErrorState(props: {
  title: string;            // 一句"发生了什么"
  hint?: string;            // 可选：怎么恢复
  onRetry?: () => void;     // 有则渲染重试按钮
  retryLabel?: string;
}): JSX.Element;

/** 管线类型：任何请求必须落到三态之一，禁止"空态兜底错误"。 */
export type LoadState<T> =
  | { kind: "loading" }
  | { kind: "error"; err: unknown }
  | { kind: "ready"; data: T };
```

### 3.6 markdown（`.md` 族）

| 原语 | 类名 | 契约 |
|---|---|---|
| md 容器 | `.md` | 只由 `Markdown` 组件渲染；视图不得自建 |
| 代码 | `.md code` | 圆角 4（chip 档）、12px mono |
| 表格 | `.md th` / `.md td` | 数据网格**允许**描边（MASTER §6.2 例外），线用 `--rule-soft` |
| 引用 | `.md blockquote` | 左侧 3px **规则线**（`--rule`），不随 C7 变粗 |
| 可展开代码块 | `.prompt-view` | 圆角 **6**（原 8）+ 1px `--control-border`（唯一允许带框的可展开块） |

### 3.7 `ui.tsx` 导出面（收敛后）

| 导出 | 形式 | 层 | 说明 |
|---|---|---|---|
| `ToastBridge` / `useToast` / `ToastHost` | 组件 / hook | L5 | antd message；`ToastHost` 保留兼容 |
| `Modal` | 组件 | L2 overlay | 唯一浮层模态（焦点陷阱由 antd 提供） |
| `Spinner` / `Empty` / `ErrorState` | 组件 | L5 | 三态齐全 |
| `Readout`(type) / `ReadoutStrip` | 类型 / 组件 | L2 / L3 | 仪表条 |
| `Zone` | 组件 | L2 | 区带语义糖（**新增**） |
| `IconButton` | 组件 | L3 | 图标按钮（**新增**） |
| `StatusDot` / `StatusPill` | 组件 | L4 | 状态标记 |
| `RelTime` / `fmtTokens` / `fmtUsd` / `UsageMeter` | 组件 / 函数 | L1 | 数据格式化（`UsageMeter` 去掉内联 `fontSize:12`） |
| `Markdown` | 组件 | L6 边界 | md 渲染 |

**契约**：视图**不得**直接 `import { Spin, Alert, App } from "antd"` 来做
"载入 / 空 / 错 / 模态 / 提示"这五件事——必须走 `ui.tsx`，否则三态一致性无法量化。
（例外：`Button` / `Input` / `Select` / `Table` / `Segmented` / `Popconfirm` 等控件
可直接用 antd。）

---

## 4 容器三角色判定树（zone / panel / overlay）

**判定树（强制：按顺序问，第一个"是"就是答案）**

```
Q1 它浮在页面之上、可以随时关掉吗？（菜单/下拉/模态/命令面板/抽屉/tooltip 宿主）
   └─ 是 → overlay
Q2 它是一个可以被点击、被测量、被单独引用的实体吗？
   （任务/会话/agent/记忆/chunk/一块仪表盘/一个控件单元）
   └─ 是 → panel
Q3 都不是 → zone（无盒：规则线 + 标签）
```

**三者与类名的绑定（唯一合法写法）**

| 角色 | 类名 | 背景 | 圆角 | 装饰 | 描边 | 内边距 |
|---|---|---|---|---|---|---|
| overlay | antd `.ant-modal` / `.modal` / `.cmdk` / `.ant-dropdown` | `--ant-color-bg-elevated` | **14** | `menuShadow` / `floatingShadow` | 允许 1px 内 ring | 组件定 |
| panel | `.panel`（`.card` = panel + 间距） | `--ant-color-bg-container` | **10** | `--panel-shadow`（亮色 = ring） | **禁止** | 16–20 |
| zone | `.zone` + `.zone-head` + `.zone-title` + `.zone-note` | 透明 | — | `--rule` 的 1px 底边 | **禁止** | 0 |

**第四种角色不存在**（MASTER P4 推论）：某块内容三者都套不上时，说明判定问题问错了，
**不要发明"半透明小卡片"**。

**判定示例（与 MASTER §7.2 对齐，补类名）**

| 现状元素 | 角色 | 契约写法 |
|---|---|---|
| `.view-bar` | zone | **冻结选择器**，保持原样；内部标题字号取 `.zone-title` 的档 |
| `.readout-strip` | panel | `.panel.readout-strip` ✔ 现行 |
| `.kanban-col` | zone | **并写** `className="kanban-col zone"`（§9.1 采用点 1） |
| `.kanban-card` / `.row-btn` / `.agent-card` / `.inbox-card` / `.memory-card` / `.guide-card` | panel | hover 面 + 材质，**无描边** |
| `.timeline-wrap` | panel | ✔ |
| `.composer` | panel（控件单元，**唯一**允许描边的内容容器） | ✔（阈值行 4/38 的例外） |
| `.cmdk` / `.modal` | overlay | ✔ |
| `.graph-canvas` | panel（L6，仅 `#graph`） | ✔ |

**与阈值行的对应**：本节的三角色是**行 4（严格口径描边内容容器 ≤1/路由）**与
**行 5（全部可见描边元素 ≤40/路由）**的语义来源；判定树保证"新增容器不会发明第四种角色"。

---

## 5 无障碍契约

### 5.1 焦点环（C11 / MASTER A1）——一条令牌，两条渲染路径，一条禁令

**现状（本文独立复测，Tab 走查，与 MASTER 一致）**

| 目标 | 暗色环 | 对底 | 亮色环 | 对底 |
|---|---|---|---|---|
| 手写 `:focus-visible`（`.brand` / `.kbd-hint` / `.row-btn` / `.kanban-card`） | `color(srgb .941 .663 .231/.65)` → 合成 `rgb(165,120,51)` | **4.26:1** ✔ | 合成 `rgb(235,193,117)` | **1.69:1** ✘ |
| antd 组件（`.ant-btn` / `.ant-menu-root` / `.ant-segmented`） | `3px solid rgb(86,65,32)` | **1.73:1** ✘ | `3px solid rgb(255,234,171)` | **1.19:1** ✘ |
| UA 默认（`.readout-btn` / `.ant-segmented-item-input`） | `auto 1px rgb(238,238,238)` | 未测 | `auto 1px rgb(16,16,16)` | 未测 |

**契约令牌**

| 令牌 | 暗 | 亮 | 最差配对（本文复算） |
|---|---|---|---|
| `--focus-ring` | `#f5b457` | `#8a5600` | 暗 **6.45:1**（落 `--surface-selected` 上）/ 亮 **4.97:1**（同）✔ |

复算矩阵（全部 ≥3:1）：暗 = panel 9.18 / canvas 10.46 / shell 10.95 / hover 7.92 /
selected 6.45 / nav 洗色 8.34；亮 = 白 6.16 / canvas 5.74 / shell 5.16 / hover 5.45 /
selected 4.97 / nav 洗色 5.45。

**两条渲染路径（必须同时实现）**

**路径 A —— 手写元素：一条零特异性规则**

```css
:where(a[href], button, summary, input, textarea, select,
       [tabindex], [role="button"], [role="option"], [role="menuitem"], [role="tab"]):focus-visible {
  outline: 2px solid var(--focus-ring);
  outline-offset: 2px;
}
```
- 用 `:where()` 把特异性压到 **0**，让任何手写组件都能覆盖它（契约要求"可覆盖"）。
- **例外 1**：位于可滚动容器内的元素（`.cmdk-item`）必须用 `outline-offset: -2px`，
  否则环被 `overflow` 裁掉。
- **例外 2**：`.composer` 的 `:focus-within` 已有 `border-color: var(--signal)`（1px，对面板
  8.30:1 ✔ / 亮 3.10:1 ✔）——它就是 `.composer` 内 `textarea` 的焦点指示，视为**已发声**。
  这是全站**唯一**被允许的"容器代为发声"。

**路径 B —— antd 拥有的元素：pin 令牌，不写 `!important`**

```css
html[data-mode="dark"]  .ruagent { --ant-color-primary-border: #f5b457; }
html[data-mode="light"] .ruagent { --ant-color-primary-border: #8a5600; }
```
antd 6 的焦点视觉就是 `outline: 3px solid colorPrimaryBorder`（实测）。把该令牌 pin 到
`--focus-ring` 的值后，antd 的 3px 环从 **1.73 → 9.18**（暗）/ **1.19 → 6.16**（亮），
无需任何组件级补丁，也无 `!important`。

**禁令（可 grep 判定）**

1. **禁止裸 `outline: none`**：任何 `outline: none | 0` 必须出现在**同一规则集内**
   紧跟一条替代指示（`outline: 2px solid var(--focus-ring)` 或 `box-shadow` 环），
   或**同一条规则集注释里写明是"容器代为发声"**。
   现存两处违规：`index.css:1267`（`.composer textarea`）、`index.css:1377`（`.cmdk-input-row input`）。
2. **禁止落到 UA 默认环**：`outline-style: auto` 视为**不合格**（它不受 token 控制，
   且在深色底上可能是 1px 的近黑线）。判定：`outlineStyle === 'solid' && outlineWidth >= 2px`。
3. **禁止 `!important`** 用于任何焦点样式。

**判定口径（给 t7，修正 MASTER 行 16 的判据漏洞）**

> 现状行 16 的判据是"`outline-style: none` 且 `box-shadow: none` 即判无环"。
> 这会让 `outline-style: auto`（UA 默认环）**误判为合格**——本轮实测
> `.readout-btn` 与 `.ant-segmented-item-input` 都是 `auto 1px`。
> **修正判据**：每个 Tab 停留点必须满足
> `outlineStyle === 'solid' && outlineWidth >= 2px && contrast(composite(outlineColor), 相邻背景) >= 3.0`，
> 或命中 §5.1 例外 2（`.composer`，改测其 `border-color` ≥3:1）。
> 满足者记 pass，其余记 fail（含 `auto`、含 `none`）。

### 5.2 键盘与 ARIA

**本节产出的是"AI 可实现的契约"**：每条都给 DOM 属性与判定钩子，不给形容词。

#### 5.2.1 命令面板（`panel/src/CommandPalette.tsx`，MASTER Y2）

> **状态（2026-09-23）：本条已全部落地（t53 + t56）—— 可关闭。** 逐条实测（build `C048RA32`）：面板 `role=dialog` + `aria-modal=true` + `aria-label`；输入框 `role=combobox` + `aria-expanded=true` + `aria-controls=cmdk-list`（目标存在）+ `aria-activedescendant`（指向真实 option，ArrowDown 跟随 `.selected`）+ `aria-autocomplete=list`；列表 `id=cmdk-list` + `role=listbox` + `aria-label=「命令」`；分组 `role=group` + `aria-label`（标签 `.cmdk-group` 用 `aria-hidden=true` —— 契约允许的两种写法之一）；选项 `role=option` + `id=cmdk-opt-<key>` + `aria-selected` + **`tabIndex={-1}`（34/34）**；`Esc` → `close()` + 还焦 `BUTTON.kbd-hint`；**连续 45 次 Tab：`escapedAtTab = -1`，从未落在 option 上**。
> **下表「现行」列是 2026-09-21 的落地前快照**，保留为历史记录，**勿据此判现状**。

实测缺口 6 项：`aria-modal` / `aria-activedescendant` / `aria-controls` /
列表 `role=listbox` / 选项 `role=option` / `aria-selected`（0/34）。

**契约 DOM 形状**

| 元素 | 必须有的属性 | 现行 | 说明 |
|---|---|---|---|
| 面板根 `<div class="cmdk">` | `role="dialog"`、`aria-modal="true"`、`aria-label` | 有 role/label，**缺 `aria-modal`** | `aria-modal="true"` 一旦声明，键盘焦点**不得**离开面板 |
| 输入框 | `role="combobox"`、`aria-expanded="true"`、`aria-controls="cmdk-list"`、`aria-activedescendant="cmdk-opt-<key>"`、`aria-autocomplete="list"` | 裸 `<input>` | `aria-activedescendant` 必须**实时**指向 `.cmdk-item.selected` 那一条 |
| 列表 `<div class="cmdk-list">` | `id="cmdk-list"`、`role="listbox"`、`aria-label` | 只有 class | — |
| 分组标签 `<div class="cmdk-group">` | `role="presentation"`（或 `aria-hidden`），条目组用 `role="group"` + `aria-label` | 无 | label 文本本身已是可读文本，勿再重复 |
| 选项 `<button class="cmdk-item">` | `role="option"`、`id="cmdk-opt-<key>"`、`aria-selected={sel}`、**`tabIndex={-1}`** | 无 | `tabIndex={-1}` 是**必需**的：它保证"输入框是面板里唯一的 Tab 停留点"，`aria-modal` 才自洽 |
| 关闭键 | `Esc` → `close()` ⚠️ **外加还焦** | 有 Esc，焦点丢到 `BODY` | 见下 |

**关闭后还焦（MASTER Y2 / 阈值行 22）**：打开面板时记录
`document.activeElement`（`useRef`），在 `close()` 里 `ref.current?.focus()`。
判定：`Esc` 之后 `document.activeElement` === 打开前的元素（实测基线 = `BODY` ✘）。

**`.cmdk-item.selected` 必须保留**（e2e 冻结，4 处断言）——
`aria-selected` 与 `.selected` 由**同一个 `sel` 状态**驱动，不允许两套索引。

#### 5.2.2 展开/收起（MASTER Y3，`aria-expanded`）

**本文复测与 MASTER 不一致，必须先纠正口径**：MASTER §11.4 记"全站 0 个"，
本轮实测（13 路由，1440×900）：

| 路由 | `[aria-expanded]` 计数 | 来源 |
|---|---|---|
| `#chat` | **3** | antd Select 的 `role=combobox` 自带 |
| `#task` | **3** | 同上 |
| `#memory` | **1** | 同上 |
| `#settings` | **1** | 同上 |
| 其余 9 路由 | 0 | — |

→ **"全站 0" 的结论不成立**（附录 A.7）。因此阈值行 23 **不能**用
`document.querySelectorAll('[aria-expanded]').length` 判定——antd 自带的计数会让它**虚假通过**。

**契约判据（修正行 23）**：只统计**手写展开控件**，逐个断言其 `aria-expanded` 存在且随
开合变化：

| 选择器 | 位置 | 现行 |
|---|---|---|
| `.recall-stub-head` | Memory / Knowledge | 无 |
| `.tool-head` | RunTimeline | 无 |
| `.chat-group-more` | Chat | 无 |
| `.prompt-view > summary` | Settings | 无（`<details>` 原生，可接受，但需 `aria-expanded` 由浏览器提供——**原生 `<details>` 已满足**，计为 pass） |

判定：上表前 3 个选择器所在元素 `getAttribute('aria-expanded')` ∈ {`"true"`,`"false"`}，
且与视觉展开态一致。

#### 5.2.3 其它键盘契约

| 项 | 契约 | 现状 |
|---|---|---|
| `Escape` 关闭浮层 | 所有 overlay（模态/面板/抽屉）必须 | ✔ |
| 侧栏折叠按钮 | 有 `aria-label`（`common.toggleSidebar`） | ✔ |
| 主题/语言按钮 | 有 `aria-label` / `title` | ✔（`toggle theme`） |
| ~~悬停显形的动作~~（**已废止**：2026-09-23 起行级动作常显，无门控可门） | ~~必须在 `:focus-within` 下同样显形~~ | —（原 `.row-btn:focus-within .row-action`、`.ws-new-btn:focus-visible`） |
| 破坏性操作 | `Popconfirm` 二次确认 + `loading`（视图侧，登记） | ✘（MASTER 未列，本文登记为视图侧待办） |

### 5.3 对比度下限（下限，不是目标）

| 对象 | 下限 | 本文落点 |
|---|---|---|
| 正文文本（< 18.66px） | **≥4.5:1** | `--control-border` 无关；靠 C3/C9 与 text 四阶。**已知唯一例外**：`.nav-badge` 白字（改前 3.20:1 → 改后用 `--signal-ink`：9.22 / 5.64 ✔） |
| 大号文本（≥18.66px 或 ≥14px bold） | ≥3:1 | `.readout*` 全部 ≥8 ✔ |
| 非文本必需图形（控件边、状态点、焦点环、图表线、脊线） | **≥3:1** | 控件边 3.09 / 3.19（C7）；焦点环 6.45 / 4.97（§5.1）；图谱边 3.08 / 3.41（C4）；`.live-dot` 8.30 / 3.10（C6）；`.inbox-card` 脊线同 |
| 装饰分隔线 | 无要求 | `--rule` 1.49 / 1.41；`--rule-soft` **1.2330（对画布）/ 1.3217（对白）** ✔（t30 改值后；改前 1.1340 / 1.2155） |
| 标签洗色底上的标签文本 | ≥4.5:1 | C3 三对：5.97 / 5.89 / 5.11 ✔ |

**判定口径**：带 alpha 的前景先与**最近的不透明祖先背景**线性合成，再算 WCAG 比。
**范围声明**：`::selection`、`:hover` 面之上的文本、动画中间帧**不在**本下限的判据范围，
一律登记为未测项（附录 A.5），不假装达标。

### 5.4 命中区最小尺寸

| 档 | 值 | 适用 |
|---|---|---|
| **硬下限** | **24×24 CSS px** | 所有可交互元素；< 24 即失败 |
| **目标** | **32×32** | 图标按钮、行内动作；每路由 < 32px 的目标 ≤10 个 |
| 内联文本链接 | 例外 | 行内 `<a>` 随文本，不计入 |

**共享层落点**：`.icon-btn` 基类（`min-width/height: 24px`）、`.ws-new-btn`
（22×22 → 24×24）、`.row-action` 内的 `ant-btn` 用 `size="small"`（24×24 ✔）。

**不得用 `padding` 撑大命中区却不改 `line-height`**：命中区按
`getBoundingClientRect()` 计，包含 padding，因此"视觉小、命中大"是合法手段，
但必须有可见的 hover 面（否则用户不知道命中区在哪）。

### 5.5 结构语义与 `.sr-only`

**契约（MASTER Y1，阈值行 24）**：每路由**恰好 1 个 `<h1>`**。
`.view-bar h2` 是冻结选择器，**不得**改成 `h1`；解法是每视图加一个视觉隐藏的 `h1`：

```css
.sr-only {
  position: absolute; width: 1px; height: 1px;
  padding: 0; margin: -1px; overflow: hidden;
  clip-path: inset(50%); white-space: nowrap; border: 0;
}
```

```tsx
<h1 className="sr-only">{t("nav.board")}</h1>
```

**判定**：`document.querySelectorAll('h1').length === 1`（含 `.sr-only` 计入），
且 `#home` 的 `.home-hero h1` 保持**可见**（e2e 冻结选择器，勿加 `.sr-only`）。

---

## 6 e2e 冻结选择器与允许的样式改动边界

**总则**：选择器**承担的"结构事实"**冻结；**外观值**放开。具体三层：

| 层级 | 冻结 | 放开 |
|---|---|---|
| 身份 | 类名存在、不重命名、不合并 | — |
| 结构 | 元素类型、祖先-后代关系、在父节点中的角色（如 `.kanban-col` 必须是 `.kanban` 的直接子元素） | 可**并写**新类名（`className="kanban-col zone"`） |
| 语义 | 可见性规则（何时 `display:none`）、交互角色（`button` 保持 `button`） | 颜色 / 圆角 / 字号 / 行高 / 间距 / 阴影 / 过渡 |

**逐条边界表**（"改样式"= 允许；"改结构"= 禁止）

| # | 选择器 | 谁在断言 | 冻结的结构事实 | 允许的样式改动 | 禁止 |
|---|---|---|---|---|---|
| 1 | `.view-bar h2` | views/judge/board（12 处） | `.view-bar` 内有 `h2`；`h2` 是标题 | 字号（20/26 保持）、字重、颜色、`.view-bar` 的 padding/底线 | 把 `h2` 换成 `h1`/`div`；删除 `.view-bar` |
| 2 | `.card.launcher` | views | `.card` + `.launcher` 双类；是"新任务"控件组 | 内边距（18 → 20）、圆角 | 去掉任一类；改成非 panel |
| 3 | `.kanban-col` | views/responsive | 4 列网格的子元素 | 圆角（无）、`min-width`、可并写 `.zone` | 改成 flex 子项；删除 |
| 4 | `.row-btn` | views/chat（4 处） | `button` 元素；行内可含 `.row-action`、`.dot`、`.tag` | 圆角 8→10、padding 微调、hover/active 面 | 改成 `div`；把 `.row-action` 移出行 |
| 5 | `.agent-card` | registry（8 处） | 卡片身份 + `.disabled` 变体 | 圆角、材质 | 删除 `.disabled` 语义 |
| 6 | `.inbox-card` | views | 左侧信号脊线（`box-shadow: inset 3px 0 0`） | 脊线宽度/颜色随 C6 | 把脊线换成 border（会进阈值行 4 口径） |
| 7 | `.ant-table` | views | antd 表格存在 | **只有 token 级**：`colorBorderSecondary` 等 | 覆盖 `.ant-table` 的布局相关 CSS |
| 8 | `.ant-empty` | recall/chat（`.ant-empty` 与 `.empty-state` 同现） | `Empty` 仍由 antd 渲染 | `.empty-state` 的外观（含 `.empty-mark` 圆角 12→10） | 换成自绘空态（会破坏该断言） |
| 9 | `.composer` | chat（2 处） | 输入区容器；`:focus-within` 发声 | 边框色、`--signal-wash`、圆角 6 | 删除 `:focus-within` 指示 |
| 10 | `.composer-controls .ctl-select` | chat | `select` 包装类名 `ctl-select` | 字号 12/18（阶梯内） | 改类名 |
| 11 | `.chat-side-toggle` | responsive@390 | **≤700 显示、>700 隐藏**（由 media query 决定） | 图标、命中区 24 | 改断点使其在 390 隐藏 |
| 12 | `.chat-side.open` | responsive@390 | ≤700 时 `.chat-side` 为 `position:fixed` 抽屉，`.open` 时 `transform:none` | 宽度 `min(300px,84vw)` 内调整、阴影、背景 | 删除 `transform` 机制；改 `.open` 类名 |
| 13 | `.chat-side-backdrop` | responsive@390 | ≤700 时 `display:block` 且可点关闭 | 遮罩色（`color-mix` 比例） | 改 `display` 逻辑 |
| 14 | `.brand-mark` | responsive | 收起侧栏时仍可见；`.brand-name` 反而隐藏 | 圆角 5→6、背景（**不得用信号色**，MASTER §3.2） | 用 `--signal` 上色；改类名 |
| 15 | `.brand-name` | responsive | 收起时 `display:none` | 字号 15/22 | 删除 |
| 16 | `.dash-grid` | views#home | 网格容器 | 列数断点（1100 → 1024） | 改类名 |
| 17 | `.dash-head` | views#home | 面板头 | 字号档 | — |
| 18 | `.guide-grid` | views#home | 网格容器 | `minmax` 值、gap | 改类名 |
| 19 | `.home-hero h1` | views#home | 可见的 `h1`（**不可**加 `.sr-only`） | 字重 650→600、行高 32→30 | 加 `.sr-only`；改元素 |
| 20 | `.kbd-hint` | views | `button`；在侧栏底部 | 11/15、命中区 24、焦点环 | 删除 |
| 21 | `.app-sider` | 12 处 | 侧栏宽 **228 / 72**、≤992 自动收起 | 底色、右分隔线 | 改宽度；改 992 |
| 22 | `.cmdk*` | palette（8 处） | `.cmdk` / `.cmdk-item` / `.cmdk-item.selected` / `.cmdk-list` / `.cmdk-group` / `.cmdk-label` / `.cmdk-hint` / `.cmdk-empty` / `.cmdk-input-row input` 全部保留 | 圆角、行高（16/23 → 16/24）、ARIA 属性、`tabIndex` | 改类名；把 `.selected` 拆成 aria-only |

**两条特殊说明**：

1. **`.chat-side-toggle` 的断点（行 11）**：现行是 `@media (max-width: 700px)`。
   §10.1 的 M6 要把 chat 的 700 合并到 **768**（`bp-tablet`）——**这是允许的**，
   因为 e2e 只断言 390 视口（`responsive.spec.ts` 的 390 用例），768 视口只断言
   home/board 的侧栏与溢出。改完后 390 行为不变，768 从"内联栏"变为"抽屉"，
   与 `bp-tablet` 的语义一致。
2. **`.ant-empty` + `.empty-state` 同现**：`Empty` 组件必须继续渲染 antd `Empty`
   （`.ant-empty` 是断言），因此三态契约里 `.empty-state` 是**包在 antd 结构外面**的。
   不要为了"纯 CSS 空态"去替换 antd。

---

## 7 对 `docs/design/views/README.md` §3.2「请求 systems 新增的原语」的裁决

**输入**：t2 的请求表（`.sr-only` / `.micro` / `.data` / `--focus-ring` / `--control-border`，
外加 §3.3 的"复用 `--brand-*`，0 个新 token"与"`WS_HUES` 未登记"两条登记）。
**本文逐条定稿**——t5 按此实现，t16 按此对齐 13 份规格。**决策不可再改**（除本文 §9.1 的删除条件）。

| # | 请求名 | 裁决 | 最终类名/令牌 | 理由与替代方案 |
|---|---|---|---|---|
| R1 | `.sr-only` | **采纳** | `.sr-only` | 定义见 §5.5。它是阈值行 24（每路由 1 个 `h1`）与冻结选择器 `.view-bar h2` 唯一不冲突的解法 |
| R2 | `.micro` | **采纳，但取值改为 11/15**（请求写的是 11/16） | `.micro` | MASTER §4.4 **T1** 的实测：`micro` 档在 CSS 里 **16 处是 11/15**、仅 1 处 11/17；且 `.time` 已是 11/15。**若按 11/16 采纳，`.micro` 与 `.time` 会变成两个只差行高的类**。契约：`.micro` 只管尺寸（11/15），**不设颜色**，与 `.muted`/`.time` 叠用（§3.1 A3） |
| R3 | `.data` | **拒绝**（同义原语） | 用既有 **`.mono`** | 请求描述是"12/16 mono 数据文本"——`.mono` 现行定义就是 `font-size:12px; line-height:16px` + mono + tabular-nums，**逐字相同**。新增 `.data` 只会让"设计系统漂移"无法用类名计数。**机械替换**：`className="mono" style={{fontSize:12}}` → `className="mono"`（删内联）；`className={...mono...}` 保持不变。这正是 MASTER T6 的"删除冗余内联 fontSize" |
| R4 | `--focus-ring` | **采纳**（值与请求一致） | 暗 `#f5b457` / 亮 `#8a5600` | §5.1 给实现方式（两条渲染路径 + 判据修正） |
| R5 | `--control-border` | **采纳**（值与请求一致） | 暗 `rgba(255,255,255,.34)` / 亮 `#8a9198` | §2.2 |
| R6 | 复用 `--brand-*`（sessions 来源点） | **采纳，但共享层必须先做一件事** | 新增 3 个变量：`--brand-claude` / `--brand-deepseek` / `--brand-opencode` | 现在品牌色只有**类**（`.brand-claude{color:#d97757}`），而 `.dot` 的底色是**内联 `style`**，内联样式引用不到类。因此把 3 个 hex **提升为 CSS 变量**（零新增 hex），类保留为 `color: var(--brand-claude)`。`--brand-opencode` 必须**双模式**（暗 `#e8e8ea` / 亮 `#121212`，与现行倒置规则一致） |
| R7 | `WS_HUES`（10 个 workspace hue） | **部分受理**：登记 + 模式化，**不新增 hex** | `--ws-1 … --ws-10` | 见 §7.1 |
| R8 | `.kpi`（captain 转述中出现，README §3.2 **未列**） | **不受理（无请求）**；若后续出现，按**同义原语拒绝** | 替代：`ReadoutStrip` / `.readout.l\|.s` / `.readout-strip.grid` / `.stat-strip`+`.stat-cell` | 三类"数字块"已有原语：页首仪表条（`.readout-strip`）、读数（`.readout*`）、网格 KPI（`.stat-strip`，antd Statistic）。再开 `.kpi` 会让"哪个数字放哪个类"变成审美题 |
| R9 | 规格 §3.1 列表里引用的 `.nums` | **删除**（0 引用） | 替代：`.mono` / `.time` / `table` 自带的 `tabular-nums` | §10.4 D1。**13 份规格不得再引用 `.nums`** |
| R10 | 规格 §3.1 列表里引用的 `.state.empty` | **删除**（1 处使用） | 替代：`Empty` 组件（→ `.empty-state`） | §10.4 D5。它是第二套空态实现，与 MASTER P6 的"三态齐全"冲突 |

### 7.1 `WS_HUES` 的裁决（`#chat` 的 workspace hue）

**本文实测（复算 10 个 hex 对底，非文本标记下限 3:1）**：

| 模式 | 10 个 hue 的对比度范围 | 达标 |
|---|---|---|
| 暗 → panel `#1b1e24` | **5.19 – 7.93** | 10/10 ✔ |
| 亮 → 白（**未补偿**） | **2.11 – 3.22** | **1/10 ✔**（只有 `#7c86f0` 3.22 过） |
| 亮 → 白（`brightness(.72)` 补偿后） | **3.91 – 5.67** | 10/10 ✔ |
| 亮 → canvas（补偿后） | **3.64 – 5.29** | 10/10 ✔ |

→ **t2 的"亮色下 10 个色全部低于 3:1"部分不成立**：`index.css:1513` 的
`html[data-mode="light"] .ws-folder { filter: brightness(.72) saturate(1.12) }`
**已经把它们拉回 3:1 以上**。问题不在颜色，在于 **`filter` 后置补偿让审计无法计算**
（`getComputedStyle` 读不到 filter 后的颜色）——这正是 t2 看到"全部低于 3:1"的原因。

**裁决**：
1. 把 10 个 hex 从 `Chat.tsx:26` 的 TS 常量**上移到 `index.css`**，命名 `--ws-1 … --ws-10`
   （暗模式值 = 现行值，**零新增 hex**）；`Chat.tsx` 改为读变量。
2. 亮模式**删除 filter**，改为 token 层的**等价值**（`brightness(0.72)` 精确等价于
   `color-mix(in srgb, <色> 72%, #000)`，即每通道 ×0.72，可计算、可判定）：

| 变量 | 暗（= 现行值，不变） | 亮（×0.72 派生） | 亮→白 | 亮→canvas |
|---|---|---|---|---|
| `--ws-1` | `#7c86f0` | `#5960ad` | 5.67 | 5.29 |
| `--ws-2` | `#e0916a` | `#a1684c` | 4.57 | 4.26 |
| `--ws-3` | `#56b4a4` | `#3e8276` | 4.51 | 4.20 |
| `--ws-4` | `#d97f8e` | `#9c5b66` | 5.12 | 4.78 |
| `--ws-5` | `#78b874` | `#568454` | 4.35 | 4.06 |
| `--ws-6` | `#a887e8` | `#7961a7` | 5.15 | 4.80 |
| `--ws-7` | `#5fa8dc` | `#44799e` | 4.69 | 4.38 |
| `--ws-8` | `#d8ac5c` | `#9c7c42` | 3.91 | 3.64 |
| `--ws-9` | `#9aa4b8` | `#6f7684` | 4.56 | 4.26 |
| `--ws-10` | `#c8877a` | `#906158` | 5.20 | 4.85 |

3. **登记**：删除 `saturate(1.12)` 会带来 −12% 饱和度，**该差异未测**（附录 A.8）；
   换来的是"色度可计算"。取舍：可判定 > 不可判定。
4. `--ws-*` 属 **L6（`#chat` 私有）**，与 `--graph-*` 同级：**不得离开 `#chat`**。

### 7.2 品牌色当标记用时的额外契约（R6 的补充）

**本文复算**：`--brand-claude #d97757` → 暗 panel **5.35** / 亮 **白 3.12、canvas 2.91**；
`--brand-deepseek #5786fe` → 暗 4.97 / 亮 白 3.36、canvas 3.14。

→ `#d97757` 作为 8px 来源点在**亮色 canvas** 上只有 **2.91:1**，差 0.09 不达标。契约：

1. **来源点必须画在 panel 上**（`.card` / `.row-btn` 内）——判据以**最近的 panel 底**为准，
   此时 3.12:1 ✔（冗余仅 0.12，t7 必须实测确认，不许用 canvas 当底）。
2. 品牌色**不得**为了对比度被改色（MASTER §3.1 G 的"域外品牌色"原则）——
   `#d97757` 是官方值，改色即失去品牌意义。
3. 若某来源点在亮色下必须落在 canvas 上，**唯一合法解法是把它放进 panel**，
   不是给它加深色。

---

## 8 对 MASTER 建议变更的逐条表态

### 8.1 C1–C11（MASTER §3.3）——共享层逐条答复

| ID | MASTER 建议 | 本文裁决 | 落到哪 |
|---|---|---|---|
| **C1** | 亮色 `raised === surface`，规格中显式声明"亮色由 ring 承载" | **采纳** | 本文 §2.1 已把 `--panel-shadow`（亮）写成 raised 的定义性装饰；`theme.tsx` 中 `colorBgElevated` 保持 = `surface`，**不加注释式解释以外的代码**。判定方式：阈值行 30 |
| **C2** | `--status-warn` 改独立值；`interrupted` 归 err 或 idle；`waiting_permission`/`blocked` 保留信号色 | **采纳并定稿**：warn = `#e2894f`（暗）/ `#8a4a12`（亮）；`interrupted` → **`--status-err`**（不是 idle，理由见 §2.3：它与 `cancelled` 同属"终态·失败"）；`waiting_permission`/`blocked` → **`--signal`** | `index.css` 2 行 + `theme.tsx` 2 hex + `ui.tsx` 3 行。**阈值行 32 由本文保证** |
| **C3** | 亮色 `.tag.ok` 钉底或改描边 | **采纳（钉底，且一并钉三个变体）**：三对文本/底色见 §2.4，全部 ≥5.11:1；洗色统一 10% | `index.css` 6 行（亮色三变体）+ 删除 3 条惰性 `border-color`。**阈值行 14 的共享层贡献** |
| **C4** | 亮色 `--graph-edge` `.34 → .50` | **采纳**：`.50` = 白 3.41 / canvas 3.19 | `index.css` 1 行 |
| **C5** | `#stats` 图表条与 `#memory` 次级按钮去信号色 | **采纳，但不在共享层**：两者都是**视图内联/antd 组件**（`Stats.tsx`/`Memory.tsx`）。共享层只提供替代色（`--ant-color-text-secondary`、`--graph-*`）。**因此 C5 是 t5 的视图批 + t16 对齐项**，本文只登记 `3 个选择器清单仍未逐条列出`（MASTER 附录 B 待办 3 未完成） | 视图侧 |
| **C6** | 拆 `--signal` / `--signal-text`；亮色填充档 `#c88413` | **采纳并加强**：不只是拆分——**亮色 `L.brand` 也从 `#e0a02a` 改为 `#c88413`**，否则 6px `.live-dot` 与 3px `.inbox-card` 脊线在亮色下只有 2.27:1（附录 A.4 的同源问题）。副作用 7.70 → 5.64 已登记 | `index.css` 4 行 + `theme.tsx` 1 hex |
| **C7** | 控件边界独立成 `--control-border`（成本 L） | **采纳，但降级为 M**：`theme.tsx` 1 个 token（`colorBorder`）+ `index.css` **3 行**（`.neighbor` 的 border + 2 处装饰线改 `--rule`）。结构性分隔线**实测已全部是 `--rule`/`-secondary`**，不需要批量改写 | `index.css` 3 行 + `theme.tsx` 1 token。**阈值行 4/5 需回归**（见 §11） |
| **C8** | 暗色 hover/active 主色应更亮 | **采纳**：pin `colorPrimaryHover` / `colorPrimaryActive` 两模式 | `theme.tsx` 2 行 |
| **C9** | 亮色 `textTertiary` → `#656a72`，`textQuaternary` → `#5f646d` | **部分采纳（两个 hex 对调）**：`textTertiary = #5f646d`、`textQuaternary = #656a72`。理由：按原文映射后梯度仍非单调（canvas 上 5.08 < 5.55）。对调后四阶严格递减且全部 ≥4.5（§2.4） | `theme.tsx` 2 hex |
| **C10** | `::selection` 去靛蓝 | **采纳，取更简方案**：`color-mix(in srgb, var(--signal) 26%, transparent)`，一条规则、两模式共用 | `index.css`：删 2 行 + 加 1 行。**登记**：选区对比度不在 §12 判据内（附录 A.5） |
| **C11** | `.brand-mark` 保持中性 | **采纳（保持）**，并在 §6 行 14 写成硬约束：`.brand-mark` **不得**用 `--signal` | `index.css` 0 行（仅约束） |

### 8.2 其余建议变更在共享层的落点

| 组 | ID | 裁决 | 共享层落点 |
|---|---|---|---|
| 字体 | T1（micro 11/15） | **采纳** | `.micro` / `.time` / `.readout-label` 之外全部 11/15（§10.1） |
| | T2（data 12/16，12/17 归一） | **采纳**（档位用 `.mono` 实现，不新增 `.data` 类，见 R3） | 6 处 12/17 → 12/16 |
| | T3（450 / 650 越界字重） | **采纳**：450 → **500**、650 → **600** | 3 行 |
| | T4（收录 subhead 16/24） | **采纳**（文档层；`.empty-title` / `.modal-head h3` / `.graph-detail-head h3` 已是 16/24） | 0 行 |
| | T5（8 处行高归一） | **采纳**：`14/22→14/21`、`14/20→14/21`、`13/20→13/18`、`15/24→15/22`、`15/23→15/22`、`16/23→16/24`、`24/32→24/30`、`30/38→32/40`（`Statistic.contentFontSize: 32`） | 8 行 + 1 token |
| | T6（删冗余内联 fontSize） | **采纳**：`.mono` 上的内联 12 删除；`muted + fontSize:11` → `muted micro` | `ui.tsx` 1 处 + 视图侧（sessions 314 / task 131 / stats 80 → **t2/t5 负责**，共享层只提供 `.micro`） |
| 间距 | S1（19 个越界值归一） | **部分采纳**：共享层内**全部**归一（§10.1）；视图内联值归视图（登记） | ~20 行 |
| | S2（`.view-bar` 14/22 → 16/24） | **采纳** | 2 行（`.view-bar` 的 padding-bottom / margin-bottom） |
| 圆角 | R1 / R2 / R3 | **采纳**：`.row-btn` 8→10、`.kanban-card` 8→10、`.agent-avatar` 8→6、`.prompt-view` 8→6、`.empty-mark` / `.chat-hero-mark` 12→10、`.brand-mark` 5→6、`.runtime-logo` 10 ✔、`100px`→`999px`（视图侧 14 处登记） | §9.2 + §10.1 |
| | R4（图谱固定像素 → `clamp()`） | **采纳** | `.graph-detail` / `.graph-canvas` 2 行 |
| 交互 | A1（统一焦点环） | **采纳**（本文 §5.1 是最终实现方案） | `index.css` + 2 个 pin |
| | A2（补 `:active`） | **采纳**（8 个选择器，见 §3.3） | 1 条合写规则 |
| | A3（删 2 处裸 `outline:none`） | **采纳** | `index.css:1267`、`:1377` |
| | A4（disabled 两值归一） | **采纳**：`.5`（`agent-card` / `icon-btn` / `btn`）/ `.6`（`memory-card.superseded` / `recall-fact.old` / `row-old`）——**保留两个值**但写进阶梯：`disabled = .5`、`superseded = .6` | 0 行（文档化） |
| 断点 | B1（收敛到 4 个） | **采纳**：520 / 768 / 1024 / 1240 + 契约值 992（§10.1 M-media） | **两个口径都写（2026-09-23 修正）**：**15 处 `@media` 规则块** —— 其中**断点 query 12 处**（`max-width: 768px` ×5 / `1024px` ×3 / `520px` ×3 / `1240px` ×1），**非断点 query 3 处**（`prefers-reduced-motion: reduce` ×1 / `hover: none` ×2）；**去重后的断点值 4 个** = {520, 768, 1024, 1240}。**「处」数的是规则块、「个」数的是去重后的值**，两个数不可互换：**阈值行 35 判的是后者**（`≤4 CSS 断点 + 1 契约值 992` —— 正则提取 `@media (max-width: …)` 后**去重**，见本文 §11 行 35 与附录 B.7）。原写「7 处 media query」是 B1 收敛**前**的计数，已过时。 |
| | B2（`#stats` 390 溢出） | **采纳，但不在共享层**：`<Table scroll={{x:560}}>` 是视图改动。**本文独立复测确认**：390 视口 `#stats` = **+99px**，其余 12 路由 = 0（附录 A.9） | 视图侧 |
| | B3（图谱 `clamp()`） | 同 R4 | — |
| 无障碍 | Y1（`sr-only h1`） | **采纳**（R1） | `.sr-only` |
| | Y2（命令面板 6 项 ARIA + 还焦） | **采纳**：§5.2.1 给出最终 DOM 契约（含 `tabIndex={-1}` 与 `aria-activedescendant` 同步） | `CommandPalette.tsx`（**不在共享热文件内**，本文给契约） |
| | Y3（`aria-expanded`） | **采纳，但先修正口径**：MASTER 的"全站 0"不成立（实测 `#chat` 3 / `#task` 3 / `#memory` 1 / `#settings` 1，全部来自 antd Select）。契约改为**只判手写展开控件**（§5.2.2） | 视图侧（3 处） |
| | Y4（`theme-color`） | **采纳**：`ThemeProvider` 的 `useEffect` 里写 `meta[name=theme-color]` = 当前模式的 `appShell`（并删除 `index.html:6` 的硬编码 `#0a0a0c`） | `theme.tsx` + `index.html` |
| 结构 | Z1（强制 `.zone*`） | **采纳**（§9.1 给出采用路径与删除条件） | `.zone*` 保留 |
| | Z2（5 个路由 panel 计数为 0） | **采纳，视图侧**：`#home/#chat/#agents/#settings/#inbox` 的容器角色由 t2 规格逐块指定 | 视图侧 |

---

## 9 两个结构裁决

### 9.1 `.zone*`：保留，并给出「双类名」采用路径（不作死原语删除）

**事实（本文复核，可 grep）**：`.zone` / `.zone-head` / `.zone-title` / `.zone-note`
在 `index.css:304–321` 有完整定义，而 `panel/src` 全部 `.tsx` 里
`className` 静态引用 **0 次**（13 路由 DOM 实测 `.zone`/`.zone-head`/`.zone-title` 均为 0）。

**裁决：保留**，理由有三，其中第三条是决定性的：

1. 三角色判定树（§4）若删掉第三支，规格退化为"panel / overlay"两色，
   而**13 个路由的实际 DOM 里 zone 语义大量存在**（`.view-bar`、`.kanban-head`、
   `.timeline-head`、`.dash-head`、`.graph-detail-head`、`.chat-side-head` 都是
   "规则线 + 标签、无盒"）。删掉类名只是让这 6 处继续各自内联，**不减少复杂度，只减少一致性**。
2. 删除会让"新代码该用什么"重新变成审美题（MASTER P4 的推论正是针对这一点）。
3. **采用成本极低**：`.zone` 只需 `min-width:0`，因此可以**与冻结类并写**
   （`className="kanban-col zone"`）——**e2e 选择器一个都不会破**。

**采用路径（t5/t16 执行；每处都可 grep 验证）**

| # | 位置 | 现在 | 改成 | 验收（DOM 计数） |
|---|---|---|---|---|
| 1 | `Board.tsx` 看板列 | `className="kanban-col"` + `.kanban-head` | `className="kanban-col zone"`；列头 `className="kanban-head zone-head"`；列名元素加 `.zone-title` | `#board` 的 `.zone` = 列数（4） |
| 2 | `RunTimeline.tsx` 时间线头 | `.timeline-head` | `className="timeline-head zone-head"` + `.zone-title` | `#task` / `#chat` 的 `.zone-head` ≥1 |
| 3 | `Home.tsx` / `Memory.tsx` 仪表区头 | `.dash-head` | `className="dash-head zone-head"` + `.zone-title` | `#home` / `#memory` ≥1 |
| 4 | `Graph.tsx` 检查器头 | `.graph-detail-head` | `className="graph-detail-head zone-head"` + `.zone-title`（保留 `h3`） | `#graph` ≥1 |
| 5 | `Chat.tsx` 会话栏头 | `.chat-side-head` | `className="chat-side-head zone-head"` | `#chat` ≥1 |

**删除条件（可判定，写死在这里）**：P3 批次结束时，
若 13 路由 DOM 里 `.zone-head` 命中的路由数 **< 3**，则判定该原语为死原语，
连同 `.zone-note` 一起删除，并把 §4 判定树的第三支改写为
"zone = 结构性分隔（`border-bottom: + --rule-soft`），无专用类"。
**在此之前不得删除。**（判定命令见附录 B）

**`.view-bar` 例外**：它是 e2e 冻结选择器，**不并写 `.zone`**；但它的标题字号
档必须与 `.zone-title` 相同（15/22 600）——这是"同档不同类"的唯一允许情形。

### 9.2 8px 圆角：不加入阶梯，全部归一到 6 / 10

**事实**：8px 出现 ≈240 次（`.row-btn` 228 + `.kanban-card` 21 + `.agent-avatar` 7 +
`.prompt-view` 7，按路由计次），是"事实标准"；阈值行 10 要求圆角越界 = 0。

**裁决：加入阶梯的方案被否决，采用归一。**

| 方案 | 阶梯 | 相邻档差 | 判定 |
|---|---|---|---|
| (a) 把 `radius-control` 改成 8 | 4 / **8** / 10 / 14 | 4, 2, 4 | ✘ 6 与 8 之间、8 与 10 之间只差 2px——**阶梯退化为连续值**，且 antd 的 `borderRadius:6`（按钮/输入框）会与 8 并存 |
| (b) 全部归一到 6 / 10（**采纳**） | 4 / 6 / 10 / 14 | 2, 4, 4 | ✔ 保持四档；8px 的 240 处里**只有 2 个语义族**，各归一族即可 |
| (c) 保留 8 并把 6 删掉 | 4 / 8 / 10 / 14 | 4, 2, 4 | ✘ 与 (a) 同病，且 antd `borderRadius` 无法只改一半 |

**(b) 的逐族映射**

| 族 | 元素 | 8 → | 依据 |
|---|---|---|---|
| 行/卡实体 | `.row-btn`、`.kanban-card` | **10** | 它们与 `.card` **同面**（都在 panel 上），MASTER R2；同面同圆角 |
| 小圆片/控件块 | `.agent-avatar`、`.prompt-view` | **6** | 它们是控件级（头像是 chip 放大、`prompt-view` 是可展开控件），与按钮同族 |

**其它越界值一并处死**：`5px`（`.brand-mark`，13 处）→ 6；`12px`（`.empty-mark`、`.chat-hero-mark`）
→ 10；`100px`（`#stats` 12 个 div + `#settings` 4 个）→ `999px`（视图内联，登记）。

**为什么"事实标准"不足以改变阶梯**：设计系统的阶梯是**判定用**的（阈值行 10）。
一个值不管出现多少次，只要它让相邻档差降到 2px，阶梯就不再有判定力。
240 处归一是**机械替换**（同族同值），成本 M 而非 L。

---

## 10 改动清单

> 标注：**新增** / **修改** / **删除**。行号以本文撰写时的文件为准
> （`index.css` 1587 行 / `theme.tsx` 274 行 / `ui.tsx` 250 行）。
> **本文不改代码**；t5 按本清单执行。

### 10.1 `panel/src/index.css`

#### 新增

| # | 位置 | 内容 | 依据 |
|---|---|---|---|
| N1 | `:root, html[data-mode="dark"]` | `--signal-text: #f5b457;` | C6 |
| N2 | 同上 | `--focus-ring: #f5b457;` | A1 |
| N3 | 同上 | `--control-border: rgba(255,255,255,0.34);` | C7 |
| N4 | 同上 | `--brand-claude: #d97757; --brand-deepseek: #5786fe; --brand-opencode: #e8e8ea;` | R6 |
| N5 | 同上 | `--ws-1 … --ws-10`（现行值） | R7 |
| N6 | `html[data-mode="light"]` | `--signal-text: #8a5600;` `--focus-ring: #8a5600;` | C6 / A1 |
| N7 | 同上 | `--control-border: #8a9198;` | C7 |
| N8 | 同上 | `--brand-opencode: #121212;` | R6 |
| N9 | 同上 | `--ws-1 … --ws-10` 的 ×0.72 派生值（§7.1 表） | R7 |
| N10 | `html[data-mode="dark"] .ruagent` | `--ant-color-primary-border: #f5b457;` | A1 路径 B |
| N11 | `html[data-mode="light"] .ruagent` **（新增块）** | `--ant-color-primary-border: #8a5600;` + 亮色三个 `.tag` 变体变量（如需） | A1 / C3 |
| N12 | 工具区 | `.sr-only { position:absolute; width:1px; height:1px; padding:0; margin:-1px; overflow:hidden; clip-path:inset(50%); white-space:nowrap; border:0; }` | R1 / Y1 |
| N13 | 工具区 | `.micro { font-size:11px; line-height:15px; }` | R2 |
| N14 | 靠近 `.readout-strip` | `.readout-strip.grid { display:grid; grid-template-columns:repeat(auto-fit,minmax(140px,1fr)); }`（并覆盖 `> *` 的 `box-shadow` 为网格线） | R8 替代 |
| N15 | 靠近 `.empty-state` | `.spinner-block { display:flex; flex-direction:column; align-items:center; gap:12px; padding:72px 0; }` | E3 |
| N16 | 同上 | `.error-state { … }`（antd Alert 容器：`margin: 24px 0` + 最大宽 520） | E4 |
| N17 | 靠近 `.ws-new-btn` | `.icon-btn { display:inline-flex; align-items:center; justify-content:center; min-width:24px; min-height:24px; padding:0 4px; border:none; border-radius:6px; background:none; color:inherit; cursor:pointer; }` + `:hover{background:var(--surface-hover)}` + `:active{background:var(--surface-selected)}` + `:disabled{opacity:.5}` | C9 / §5.4 |
| N18 | 焦点区 | `:where(a[href],button,summary,input,textarea,select,[tabindex],[role="button"],[role="option"],[role="menuitem"],[role="tab"]):focus-visible { outline:2px solid var(--focus-ring); outline-offset:2px; }` | §5.1 路径 A |
| N19 | 同一区 | `.cmdk-item:focus-visible { outline-offset:-2px; }`（滚动容器内，环内收） | §5.1 |
| N20 | `::selection` 处 | `::selection { background: color-mix(in srgb, var(--signal) 26%, transparent); }` | C10 |
| N21 | 交互态区 | 一条按下态合写规则：`.row-btn:active, .kanban-card:active, .guide-card:active, .suggest-chip:active, .readout-btn:active, .mem-preview:active, .neighbor:active, .icon-btn:active, .cmdk-item:active { background: var(--surface-selected); }` | A2 |
| N22 | 亮色 `.tag` 区（**扩展现有 `:455`** 的那条 `html[data-mode="light"] .tag.ok { color:#14691f }`） | 三条钉死：`.tag.ok { color:#14691f; background:#e8f2eb; }` / `.tag.warn { color:#8a4a12; background:#f3ede7; }` / `.tag.err { color:#c4121c; background:#f9e7e8; }` | C3 |
| N23 | `.brand-*` 区 | `.brand-claude{color:var(--brand-claude)} .brand-deepseek{color:var(--brand-deepseek)} .brand-opencode{color:var(--brand-opencode)}`，并**删除** `html[data-mode="dark"] .brand-opencode` 那条（改由变量承载） | R6 |

#### 修改

| # | 现位置 | 现 → 改 | 依据 |
|---|---|---|---|
| M1 | `:44–74` 暗块 `--signal` | `#f0a93b` 保持；**新增** `--signal-text`（N1） | C6 |
| M2 | `:76–107` 亮块 `--signal` | `#8a5600` → **`#c88413`**（填充档） | C6 |
| M3 | 暗块 `--status-warn` | `#f0a93b` → **`#e2894f`** | C2 |
| M4 | 亮块 `--status-warn` | `#8a5600` → **`#8a4a12`** | C2 |
| M5 | 亮块 `--graph-edge` | `rgba(15,23,42,.34)` → **`.50`** | C4 |
| M6 | `:129` `.pad` | `padding:14px 18px` → **`16px 20px`** | §5.3 |
| M7 | `:131` `.time` | 11/15 ✔ 保持（成为 `.micro` 的 mono 兄弟） | T1 |
| M8 | `:134–135` `::selection` | 删除两行（由 N20 取代） | C10 |
| M9 | `:160–176` `.brand-mark` | `border-radius:5px` → **`6px`**；其余不动（**不得**改色） | R1 / C11 |
| M10 | `:229–237` `.nav-badge` | `background: var(--ant-color-error)` → **`var(--signal)`**；`color:#fff` → **`var(--signal-ink)`** | 附录 A.4 |
| M11 | `:255–265` `.view-bar` | `margin-bottom:22px` → **24px**；`padding-bottom:14px` → **16px**（`border-bottom: var(--rule-soft)` 已是显式 ✔ **不动**） | S2 |
| M12 | `:267` `.view-bar h2` | 保持 20/26 600（**冻结选择器，不改身份**） | §6 |
| M13 | `:276` `h3.sec` | `margin:28px 0 12px` → **`24px 0 12px`**；字号 15/22 保持 | S1 |
| M14 | `:284` `.intent` | `14/22` → **`14/21`** | T5 |
| M15 | `:288–296` `.card` | `padding:16px 18px` → **`16px 20px`** | §5.3 |
| M16 | `:304` `.zone` | 保持 `min-width:0`（成为"可并写的锚类"） | §9.1 |
| M17 | `:305–313` `.zone-head` | `border-bottom: 1px solid var(--rule)` ✔ 保持；内距 10 ✔ | §4 |
| M18 | `:340–346` `.readout-label` | `line-height:17px` → **`16px`**；`margin-top:2px` ✔ | T2 |
| M19 | `:353–366` `.readout-strip` | `margin-bottom:14px` → **16px**；其余保持 | S1 |
| M20 | `:387–402` `.row-btn` | `border-radius:8px` → **`10px`** | R1/R2 |
| M21 | `:408` 行间分隔 | 已是 `var(--ant-color-border-secondary)` ✔ **不动**（C7 的正确形态，不得改成 `colorBorder`） | C7 |
| M22 | `:415–425` `.pill` | 保持；`border-radius:999px` ✔ | — |
| M23 | `:426–444` `.tag` | 保持 4px；**删除** `.tag.ok/.warn/.err` 的三条 `border-color` | C3 |
| M24 | `:452–457` `.tag.*` | 亮色由 N22 接管 | C3 |
| M25 | `:461` `.field > span` | 13/18 ✔；`margin-bottom:6px` ✔ | — |
| M26 | `:464–475` `.chat-field .field-hint` | `line-height:17px` → **`16px`** | T2 |
| M27 | `:484–493` `.empty-mark` | `border-radius:12px` → **`10px`** | R1 |
| M28 | `:494` `.empty-state .ant-empty-image` | 保留 `height:auto; margin-bottom:16px`（**现在被 `ui.tsx:98` 的内联样式覆盖成 0**，见 §10.3 Y5） | E2 |
| M29 | `:499` `.empty-hint` | `line-height:20px` → **`18px`**；`margin:6px 0 0` ✔ | T5 |
| M30 | `:516–518` `.kanban` | `gap:22px` → **`24px`**；1240 ✔ 保持；700 → **768** | S1 / B1 |
| M31 | `:523–530` `.kanban-head` | 显式 `var(--rule)` ✔；`margin-bottom:12px` ✔ | — |
| M32 | `:531–538` `.count` | `line-height` 归一到 **15px**（micro 同档） | T1 |
| M33 | `:539–556` `.kanban-card` | `border-radius:8px` → **`10px`** | R1 |
| M34 | `:575` `.hint summary` | 12/16 ✔ | — |
| M35 | `:580` `.compare` | `gap:14px` → **`16px`** | S1 |
| M36 | `:601–605` `.timeline-head` | 描边已是 `-secondary` ✔ **不动**；P3 可并写 `.zone-head` | §9.1 |
| M37 | `:606` `.timeline` | `padding:12px 14px 14px 18px` → **`12px 16px 16px 20px`** | S1 |
| M38 | `:619–625` `.ev*` | `padding:2.5px 0` → **`2px 0`**（`space-0.5`） | S1 |
| M40 | `:687` `.memory-card .memory-content` | `14/22` → **`14/21`** | T5 |
| M41 | `:690` `.diff-row` | `border-bottom` 已是 `-secondary` ✔ **不动** | — |
| M42 | `:692` `.diff-detail` | 12/16 ✔ | — |
| M43 | `:725` `.search-hit` | 描边已是 `-secondary` ✔ **不动** | — |
| M44 | `:782–783` `.row-old` | `opacity:.55` → **`.6`**（统一"被取代"档） | A4 |
| M45 | `:784–795` `.neighbor` | `border: 1px solid var(--rule)` → **`1px solid var(--control-border)`**（1.49:1 → 3.09:1；它是控件，不是装饰） | C7 |
| M46 | `:796–815` `.graph-detail` | `flex:0 0 400px` → **`flex:0 0 clamp(320px,32vw,460px)`** | R4 |
| M47 | `:798–805` `.graph-canvas` | `height:520px` → **`clamp(360px,56vh,620px)`** | B3 |
| M48 | `:811–815` `@media (max-width:1100px)` | → **`1024px`** | B1 |
| M49 | `:826–833` `.agent-grid` | gap/padding 就近归一（`14→16`） | S1 |
| M50 | `:835–849` `.agent-avatar` | `border-radius:8px` → **`6px`** | R1 |
| M51 | `:864–865` `.stat-cell > .muted` | `12/17` → **`12/16`** | T2 |
| M53 | `:873–876` `.inbox-card` | 脊线用 `var(--signal)` ✔（改后亮色为 `#c88413`） | C6 |
| M54 | `:877–896` `.home-hero` | `h1` 字重 650 → **600**、行高 32 → **30**；`p` 的 `14/22` → **`14/21`** | T3/T5 |
| M55 | `:891–896` `.stat-strip` 值 | 由 antd token 接管（32/40）；`title` 12/16 ✔ | T5 |
| M56 | `:898` `.steps strong` | `14/20` → **`14/21`** | T5 |
| M57 | `:915` `.guide-card strong` | `14/20` → **`14/21`** | T5 |
| M58 | `:917` `.guide-icon` | `color: var(--signal)` → **`var(--ant-color-text-tertiary)`**（装饰不是"现在"，MASTER 黑名单 V5） | MASTER V5 |
| M59 | `:921–933` `.dash-*` | `gap` 归一；`@media (max-width:1100px)` → **`1024px`** | B1/S1 |
| M61 | `:952–956` `.mem-sub` | `line-height:17px` → **`16px`** | T2 |
| M63 | `:1034` chat 抽屉 700 块 | 6 条规则 + 2 个 media 合并到 **768**（`.chat-rail-toggle` / `.chat-side-toggle` / `.chat-side` 固定定位 / `.chat-side.open` / `.chat-side-backdrop` / `.chat-layout` 单列） | B1 / §6 说明 1 |
| M64 | `:1038` `@media (max-width:940px)` | → **`1024px`**（chat 网格 200px 栏） | B1 |
| M65 | `:1041` 第二个 700 块 | 与 M63 合并（同一断点只写一次） | B1 |
| M67 | `:1163–1171` `.chat-hero-mark` | `border-radius:12px` → **`10px`** | R1 |
| M69 | `:1225` `.chat-msg.agent .md` | `15/24` → **`15/22`** | T5 |
| M71 | `:1266–1272` `.composer textarea` | **删除裸 `outline:none`**；`font-size:15px; line-height:23px` → **`15/22`** | A3 / T5 |
| M75 | `:1331–1338` `.modal-head` | `padding:14px 18px` → **`16px 20px`**（描边已是 `-secondary` ✔ 不动）；`h3` 16/24 ✔ | S1 |
| M76 | `:1338` `.modal-body` | `padding:12px 18px 18px` → **`12px 20px 20px`** | S1 |
| M77 | `:1376–1380` `.cmdk-input-row input` | **删除裸 `outline:none`**；`16/23` → **`16/24`** | A3/T5 |
| M79 | `:1425–1432` 手写焦点环 | **整体替换**为 N18 的 `:where()` 规则（原列表漏了 `.readout-btn` / `.icon-btn` / `.cmdk-item`） | A1 |
| M80 | `:1454–1470` `.prompt-view` | `border-radius:8px` → **`6px`**；`border` 改 `var(--control-border)` | R3/C7 |
| M81 | `:1513` `.ws-folder` filter | **删除**（亮色值改由变量承载） | R7 |
| M82 | `:1514` `.chat-session .title` | `font-weight:450` → **`500`** | T3 |
| M83 | `:1538–1554` `.ws-new-btn` | `width/height:22px` → **`min-width/height:24px`** | §5.4 |
| M84 | `:1530–1536` `.chat-ws-title` | `12/17` → **`12/16`**（`font-weight:600` **合法档，保持**） | T2 |
| M85 | `:1573–1576` `.brand-*` | 改为引用变量（N23） | R6 |
| M86 | `:817–825` `.graph-detail-head h3` | `font-weight:650` → **`600`** | T3 |
| M88 | `:678` / `:1142` | 唯一两处 `var(--ant-color-border)` → **`var(--rule)`**（装饰线）；其余 26 处 `-secondary` 保持 | C7 |
| M90 | `:957–963` `.mem-preview` | 保持；`:active` 由 N21 覆盖 | A2 |

> **编号说明**：上表编号连续，但**未列出的元素即"明确不动"**——它们已在 §10.5 或
> 各节"保持 ✔"中说明（例如 `.ant-card`、`.chat-side`、`.modal`、`.send-btn`、
> `.cmdk-*` 的结构与 `.stat-num` 的字号）。
| M91 | 全部 `@media` | 收敛后只允许 **520 / 768 / 1024 / 1240**；`992` 出现在 `App.tsx` 的 `matchMedia`（契约值，不在 CSS 里） | B1 |

#### 删除（收敛漂移：定义在 CSS、0 引用）

| # | 位置 | 内容 | 证据 |
|---|---|---|---|
| D1 | `:132` | `.nums` 选择器（保留 `.time/.mono/table` 的 `tabular-nums`） | `grep -c '\bnums\b' src/**/*.tsx` = **0** |
| D2 | `:134–135` | 两条 `::selection` 靛蓝规则 | 由 N20 取代 |
| D3 | `:452–457` | `.tag.ok/.warn/.err` 各一条 `border-color`（3 行惰性声明） | `.tag { border: none }` |
| D4 | `:512` | `.empty-icon` | tsx 0 引用 |
| D5 | `:502–511` | `.state.empty` | **1 处**使用（`Chat.tsx:1259`）→ 迁移到 `Empty` 组件 |
| D6 | `:637` | `.tool-io .io-label`（及已无 CSS 的 `.tool-io`） | tsx 0 引用 |
| D7 | `:52` / `:81` | `--surface-shadow`（两模式下各定义一次） | **全仓库 `var(--surface-shadow)` 引用数 = 0**（`grep -rn 'surface-shadow' panel/src` 只命中这两条定义） |
| D8 | `:1425–1432` | 原手写焦点环规则块 | 由 N18 取代 |
| D9 | `:1513` | `.ws-folder` 的亮色 filter | 由 N9/N23 取代 |
| D10 | `:267–274` 内 | **不删**：`.view-bar h2`（冻结）。但 `.view-bar .sub`（`:275`）**0 引用，可删** | tsx 0 引用 |
| D11 | `:347` | `.readout-row` | tsx 0 引用，被 `.readout-strip` 取代 |

### 10.2 `panel/src/theme.tsx`

| # | 类型 | 位置 | 内容 | 依据 |
|---|---|---|---|---|
| X1 | 修改 | `D`（`:46`） | `warning: "#f0a93b"` → **`"#e2894f"`** | C2 |
| X2 | 修改 | `L`（`:90`） | `warning: "#8a5600"` → **`"#8a4a12"`**；`brand: "#e0a02a"` → **`"#c88413"`** | C2 / C6 |
| X3 | 修改 | `L`（`:96–97`） | `textTertiary: "#6e737c"` → **`"#5f646d"`**；`textQuaternary: "#656a73"` → **`"#656a72"`** | C9 |
| X4 | 修改 | `base`（`:126`） | `Statistic.contentFontSize` 不在 base——在 `components()`：`Statistic: { contentFontSize: 30 }` → **`32`** | T5 |
| X5 | 修改 | `components()` `Card` | `paddingLG: 18` → **`20`** | §5.3 |
| X6 | 新增 | `darkTokens` | `colorPrimaryHover: "#f5b457"`、`colorPrimaryActive: "#f7c073"` | C8 |
| X7 | 新增 | `lightTokens` | `colorPrimaryHover: "#b8760f"`、`colorPrimaryActive: "#a96a0c"` | C8 |
| X8 | 修改 | 两个 tokens 对象 | `colorBorder` → 新的 `--control-border` 值（暗 `rgba(255,255,255,0.34)` / 亮 `#8a9198`） | C7 |
| X9 | 新增 | `ThemeProvider` 的 `useEffect` | 写 `<meta name="theme-color">` = 当前模式 `appShell`；同时删除 `panel/index.html:6` 的硬编码 `#0a0a0c`（改由 JS 在首帧前写入，或保留一个与 `D.appShell` 一致的值） | Y4 |
| X10 | 不变 | `FONT` / `MONO` / `base` 的字号/行高/圆角阶梯 | — | — |
| X11 | 不变 | `Menu` 的导航选中三件套（洗色 + 字色 + 2px 脊线在 index.css） | — | — |

### 10.3 `panel/src/ui.tsx`

| # | 类型 | 位置 | 内容 | 依据 |
|---|---|---|---|---|
| Y1 | 修改 | `:133` | `interrupted: "var(--status-warn)"` → **`"var(--status-err)"`** | C2 |
| Y2 | 修改 | `:134–136` | `in_progress` / `running` / `spawning`：`"var(--ant-color-primary)"` → **`"var(--signal)"`** | C2 |
| Y3 | 修改 | `:139–140` | `waiting_permission` / `blocked`：`"var(--status-warn)"` → **`"var(--signal)"`** | C2 |
| Y4 | 修改 | `:72–76` `Spinner` | 内联 style 对象 → `className="spinner-block"` | E3 / 阈值行 12 |
| Y5 | 修改 | `:79–107` `Empty`（内联在 `:98`） | **删除** `imageStyle={{ height:"auto", marginBottom:0 }}`（它静默覆盖 CSS 的 16px，实测 `#inbox` 内边距 = **0px**，附录 A.2） | E2 |
| Y6 | 修改 | `:194` `UsageMeter` | 删除 `style={{ fontSize: 12 }}`（`.mono` 已是 12/16） | T6 / 阈值行 11 |
| Y7 | 新增 | 文件内新增（`Empty` 之后） | `ErrorState`（antd `Alert` + 重试按钮，`role="alert"` 由 antd 提供） | E4 / 阈值行 20 |
| Y8 | 新增 | 文件内新增（`Empty` 之后） | `LoadState<T>` 类型 | E4 |
| Y9 | 新增 | 文件内新增（`Readout` 类型之前） | `Zone` 组件（§3.2 签名） | §9.1 |
| Y10 | 新增 | 文件内新增（`StatusPill` 之后） | `IconButton` 组件（§3.3 签名，`label` 必填） | §5.4 / 阈值行 17 |
| Y11 | 修改 | `:219–238` `ReadoutStrip` | `.readout.signal` 的**数字**改走 `--signal-text`（在 index.css 里定义 `.readout.signal { color: var(--signal-text) }`），此处不改 JSX | C6 |
| Y12 | 不变 | `ToastBridge` / `useToast` / `Modal` / `Markdown` / `fmtTokens` / `fmtUsd` / `RelTime` | — | — |

### 10.4 一句话总结：本次"收敛漂移"删掉的规则

**11 条**：`.nums` / `::selection`×2 / `.tag.*` 的 3 条惰性 `border-color` / `.empty-icon` /
`.state.empty`（5 行）/ `.tool-io .io-label` / `--surface-shadow` / 旧手写焦点环块 /
`.ws-folder` filter / `.view-bar .sub` / `.readout-row`。

**t29 追加 2 条（共享层清理，2026-09-21）**：
`.readout-strip.grid` 的 `grid-template-columns: repeat(auto-fit, minmax(140px,1fr))`
（原 B7 公式）—— 任何宽度都不生效：`≤520` 被同块内的 2 轨规则覆盖，`>520` 该修饰符不产生规则；
`.chat-side-toggle { display: inline-flex }` —— 在 `@media (max-width: 768px)` 里出现了两次，
后一条生效。两条都**零行为差异**（parity 与显示态实测不变）。

**判据**：每条都能用一条命令证明"0 引用"，或"被同目录的替代品取代"
（附录 B 给出逐条命令）。**删除是收敛设计系统的必要条件**：留着的每个死类
都会让下一个人以为它有语义。t29 的"被覆盖后永不可达"扫描见附录 B.7：
清理前 2 条、清理后 **0 条**。

### 10.5 明确不改的东西（防止 t5 顺手改）

| 对象 | 为什么不能改 |
|---|---|
| `.view-bar h2` 的元素类型与字号 | e2e 12 处断言 |
| 侧栏 `228` / `72`、`matchMedia("(max-width: 992px)")` | e2e `responsive.spec.ts` |
| `.chat-side.open` 的 `transform` 机制与 `.chat-side-backdrop` 的 `display:block` | e2e 390 用例 |
| `.brand-mark` 的中性色 | MASTER C11 + §6 行 14 |
| `.composer` 的 `:focus-within` 信号指示 | §5.1 例外 2 |
| `.cmdk-item.selected` 类名 | palette e2e 4 处 |
| antd 的 `Empty` 结构（`.ant-empty`） | recall/chat e2e |
| `.ant-table` 的布局 CSS | views e2e |
| 图谱的 rAF 物理循环 | MASTER §9.4 明确"不在 reduced-motion 覆盖内"，属 `#graph` L6 |

---

## 11 共享层验收映射（MASTER §12 行 → 本文判据）

| 行 | 指标 | 目标 | 共享层的贡献 | 判据（t7 照抄） |
|---|---|---|---|---|
| 4 | 严格口径描边内容容器 | ≤1/路由，且那 1 个必须是控件单元 | `.neighbor` 改 `--control-border`（它不在严格口径内）；`.prompt-view` 圆角改 6 **并保留 1px 边框**（属控件 ✔）；`.inbox-card` 必须保持**脊线（box-shadow）而非 border**——改 border 会直接把计数从 0 抬到 2 | 按 MASTER §12 行 4 的 class 过滤口径；`.inbox-card` 加断言"`borderWidth === '0px'`" |
| 5 | 全部可见描边元素数 | ≤40/路由 | 共享层不变（大头是 `#task` 的 `.md` 表格单元，已排除） | 同 MASTER 行 5 口径；`#task` 先排除 `.md th/td` |
| 8 | 字号越界数 | 0 | M8/M18/M26/M29/M32/M37(无)/M40/M51/M56/M57/M61/M69/M71/M77/M84 全部归一到 11/12/13/14/15/16/18/20/24/32 | `getComputedStyle().fontSize` ⊆ 阶梯；**重点复查** `#sessions` 的 314 处内联（视图侧） |
| 9 | 字重越界数 | 0 | M82/M86/M54（450/650 → 500/600） | `fontWeight ∈ {400,500,600,700}` |
| 10 | 圆角越界元素数 | 0 | §9.2 全族映射；`.runtime-logo` 10 ✔；`#stats` 的 12 个 `100px` 与 `#settings` 的 4 个（**视图内联**，登记） | `borderRadius ∈ {0,4,6,10,14,50%,999px,3px(滚动条 thumb)}`；**新登记**：`::-webkit-scrollbar-thumb` 的 `3px` 必须计入白名单，否则永远不为 0 |
| 11 | 内联 `font-size` 元素数 | 0 | Y6（`UsageMeter` 1 处）；其余 314/131/80/3 全在视图（t2 §3.2 R3 的替换规则已给出） | `el.getAttribute('style')?.includes('font-size')` |
| 12 | 内联 `style` 元素数 | ≤50/路由 | Y1/Y4/Y5/Y6 把 `ui.tsx` 的 3 处内联搬到 `.spinner-block` / `.empty-state` / `.mono`；**`#sessions` 732 / `#stats` 214 / `#task` 163 是视图侧**（本文实测） | `[style]` 计数；`#sessions` 达标依赖 t5 的视图批 |
| 15 | 焦点环对比度 | 两模式 ≥3.0 | `--focus-ring` + 两条渲染路径：antd **1.73→9.18 / 1.19→6.16**；手写 **1.69→4.97（最差）** | 每次 Tab 记录 `outlineColor`，与相邻不透明背景合成后算比；**antd 环 3px 也要计** |
| 16 | 无焦点环的 Tab 停留点 | 0 | N18 覆盖 `.composer textarea` / `.ant-input-sm` / `.ant-select-input`（#chat 的 5 个）；**判据改为"`outlineStyle==='solid' && width>=2px`"**（`auto` 判 fail），见 §5.1 修正 | 见 §5.1 判定口径；例外仅 `.composer` |
| 17 | 无名称可交互元素数 | 0 | `IconButton` 的 `label` 必填（新代码）；`.icon-btn` 基类；**存量 4/5/2 处属视图** | 可见交互元素且 `aria-label`/文本/`title` 全空 |
| 18 | <32px 命中目标 | <24px = 0；<32px ≤10/路由 | `.icon-btn` `min 24×24`；`.ws-new-btn` 22→24；`.send-btn` 32 ✔ | `getBoundingClientRect()` |
| 20 | error 与 empty 可区分 | 每路由 `role=alert` + 重试 | `ErrorState` 组件（共享）；视图接线 | CDP 屏蔽 `*/api/v1/*`，检查 `[role=alert]` 存在且 `.ant-empty` 不出现 |
| 21 | 命令面板 ARIA 缺口 | 0 | §5.2.1 的最终 DOM 契约（含 `aria-activedescendant` 同步规则） | 6 个属性逐个断言 |
| 22 | 浮层关闭后焦点归属 | = 触发元素 | §5.2.1 的 `useRef` 还焦规则 | `Esc` 后 `document.activeElement` |
| 23 | `[aria-expanded]` **覆盖率**（原写「出现数」，2026-09-23 统一量纲） | **覆盖率 = 100%**（原写「≥ 手写展开控件数」—— 同义改写，以便与 MASTER 行 23 的解析锚点 `= N%` 一致） | **判据修正**：只数 `.recall-stub-head` / `.tool-head` / `.chat-group-more`（+ 原生 `summary` 自动 pass）。**不得**用全站 `querySelectorAll` 计数（antd Select 会给出 3/3/1/1，见 §5.2.2） | 三个选择器所在元素 `aria-expanded ∈ {"true","false"}` |
| 24 | 每路由 `h1` 数 | 恰好 1 | `.sr-only`（N12）；`.view-bar h2` 不动；`#home` 的可见 `h1` 保持可见 | `querySelectorAll('h1').length === 1` |
| 30 | 亮色 panel 的 ring | 存在且 ≥1.2:1 | 环取值随 `--rule-soft` 改为 `#dee0e4`（**1.2330:1** 对 `pageCanvas` / 1.3217:1 对白 ✔，t30 实测；MASTER §12.7.1） | 解析 `box-shadow` 与 `pageCanvas` 比 |
| 32 | 信号令牌唯一性 | 0 组重复 | **检查集 5 个纯语义令牌**：`--signal` / `--status-warn` / `--brand-claude` / `--brand-deepseek` / `--brand-opencode` **两两互不相同**（每模式 10 组）+ `--status-warn` vs `--signal` Δhue ≥8°（§2.3）。**accent（`--brand` / `--ant-color-primary`）及 accent 通道派生档（`--ant-color-primary-hover` / `--ant-color-primary-active` / `--ant-color-link`）显式排除** —— 它们是同一含义的渲染路径，与 `--signal` 同值属刻意设计（§2.4 C6、MASTER §12.3）。排除是**具名白名单**：白名单外任何令牌与 `--signal` 同值仍判失败 | 解析 CSS 变量表做字符串比较 + HSV 色相；**accent 通道不采集** |
| 34 | 无 `:active` 反馈的手写可点元素 | 0 | N21 的 9 选择器合写规则 | 检查这些选择器**是否存在** `:active` 规则（源码级） |
| 35 | 断点值个数 | ≤4 CSS 断点 + 1 契约值 | M30/M48/M59/M63/M64/M65 | 正则提取 `@media (max-width: …)`，去重后应 ⊆ {520,768,1024,1240} |
| 36 | 侧栏宽度 | 228 / 72 | **共享层不碰** | `boundingBox().width` |
| 37 | 间距越界值个数 | 0 | S1/S2 的共享层部分 | 采集 `padding/margin/gap`，剔除 `clamp()` 产出 |

**共享层不涉及的阈值行**（不在这份契约的手上，登记以免误解）：
行 1/2/3（像素直方图）、行 6/7（大字号节点数——`.readout` 的档位已定，数量由视图决定）、
行 13/14（文本对比度——**共享层只消掉了 C3/C9/nav-badge 三处已知失败**，
`#task` 的 18016 处属视图）、行 19（`#stats` 表格）、行 25/26（`#task` 虚拟化）、
行 27（构建体积）、行 28（轮询）、行 29/31（材质台阶与图谱边——C4 已给值）、行 33（图谱分类色）。

---

## 12 落地批次（给 t5）

> 顺序不可打乱：P0 是 token 层，P1 是几何收敛，P2 是语义，P3 是视图采用。
> **每批结束都必须**：`cd panel && npm run build` → `npx playwright test` → 审计脚本 → 截图归档。

| 批次 | 内容 | 涉及本文条目 | 影响面 | 验证重点 |
|---|---|---|---|---|
| **P0 — token 层（1 个 PR）** | C2（warn 拆值 + 状态重映射）、C6（信号拆档 + `L.brand`）、C3（亮色三 tag）、C4（图谱边）、C9（文本梯度对调）、C10（`::selection`）、C7（`--control-border` 3 行：`.neighbor` + 2 处装饰线）、C8（hover/active）、A1（`--focus-ring` + 两条路径）、A3（删 2 处裸 `outline:none`）、`.nav-badge`（附录 A.4） | §2.1–2.4、§5.1、N1–N11、N18–N23、M1–M5、M10、M79、M85、M71、M77、X1–X8 | `index.css` + `theme.tsx` + `ui.tsx` | 行 15/16/32 + 行 13/14 复测 + 两模式逐路由截图 |
| **P1 — 几何与类型收敛** | T1–T6、S1–S2、R1–R3、B1（断点）、R4/B3（图谱 `clamp()`） | §2.5、§9.2、M6–M91（除 M58/M10 之外的全部"修改"） | `index.css` + `theme.tsx` | 行 8/9/10/35/37 + e2e 41 项 |
| **P2 — 语义与无障碍** | Y1（`.sr-only` + 13 个 `h1`）、Y2（面板 ARIA + 还焦）、Y3（`aria-expanded`）、Y4（`theme-color`）、A2（`:active`）、A4（disabled 档）、N12–N21 | §5.2、§5.4、§5.5、§3.3 | 视图 + `ui.tsx` + `theme.tsx` | 行 17/18/21/22/23/24/34 |
| **P3 — 原语采用与视图落地** | `.zone*` 五个采用点（§9.1）、`Zone` / `IconButton` / `ErrorState` 接线、C5（图表与次级按钮去信号色）、R7（`--ws-*` 上移）、Z2（5 个路由的 panel 角色） | §7、§9.1、§3.7 | 视图 | `.zone-head` 命中路由数 ≥3（否则触发 §9.1 的删除条件） |
| **P4 — 性能（不在本契约）** | 行 25–28 | — | 视图 + 构建 | 独立 |

**P0 的提交自检清单（6 条，全部可机器验证）**

1. 检查集 5 个纯语义令牌两两互不相同（字符串）：`--signal` / `--status-warn` / `--brand-claude` / `--brand-deepseek` / `--brand-opencode`；且 `--status-warn` vs `--signal` Δhue ≥8°。**accent（`--brand` / `--ant-color-primary`）刻意 === `--signal`，显式排除在唯一性检查外**（§2.4 C6、MASTER §12.3）
2. 两模式 Tab 走查：每个停留点 `outlineStyle === 'solid' && width >= 2px`，环对比度 ≥3.0
3. 亮色 `.tag.ok/.warn/.err` 三对对比度 ≥4.5（5.97 / 5.89 / 5.11）
4. 亮色 `.live-dot` / `.inbox-card` 脊线对白 ≥3.0（3.10）
5. 亮色 `textTertiary` 在 canvas ≥4.5（5.55）、shell ≥4.5（4.99）
6. `#inbox` 的 `.empty-state .ant-empty-image` `margin-bottom` = **16px**（不再是 0）

---

## 附录 A：本轮新增实测与差异登记（不静默改数）

**方法**：Chromium headless（Playwright 1.x）直连活 daemon `http://127.0.0.1:8787`，
viewport 1440×900 / deviceScaleFactor 1，`waitUntil:'load'` 后等 1.2–2.2s；
颜色计算用 WCAG 2.x 相对亮度公式（带 alpha 先与最近不透明背景线性合成）。
**本文没看任何像素**。复现命令见附录 B。

| # | 内容 | 结果 | 与 MASTER 的关系 |
|---|---|---|---|
| **A.1** | **C9 的两个 hex 正确、映射颠倒** | 按 MASTER C9 原始映射：亮 canvas 上 `textTertiary` **5.08** < `textQuaternary` **5.55**（梯度仍非单调） | **修正 MASTER C9**：两值对调（tertiary `#5f646d` / quaternary `#656a72`），四阶 19.43 > 6.33 > 5.95 > 5.44 严格递减 |
| **A.2** | `.empty-state .ant-empty-image` 的 `margin-bottom` | 实测 `#inbox` = **`0px`**（`ui.tsx:99` 的 `imageStyle` 覆盖了 `index.css:494` 的 `16px`）；`.empty-mark` 底到 `.empty-title` 顶的实测间距 = **6px** | MASTER 未记录。**CSS 里的 16px 是死声明**；契约 = 删内联、CSS 说了算 |
| **A.3** | 亮色 `.tag` 洗色深度 | 10% 洗色：ok **5.97** / warn **5.89** / err **5.11**；16% 洗色：ok 5.48 / warn 5.36 / **err 4.60**（逼近下限） | MASTER C3 只说"钉底"，未定 alpha。**契约取 10%** |
| **A.4** | `.nav-badge` 是**第二强调色 + 对比度失败** | 源码 `index.css:229`：`background: var(--ant-color-error)` + `color:#fff`；暗 `--ant-color-error`（已 pin）= `#f0615c` → 白字 **3.20:1**（11px，需 4.5）✘。**未能实测**：`.nav-badge` 仅在 `inboxCount > 0` 时渲染（本轮 13 路由实测 0 个） | **MASTER 未记录**；MASTER §3.2 W3 反而把 `.nav-badge` 列为信号白名单项 → **实现与白名单冲突**。契约：改 `--signal` fill + `--signal-ink`（9.22 / 5.64 ✔）。⚠ t7 必须**先造出一个待处理项**再测这一行，否则该缺陷对审计不可见 |
| **A.5** | **未测项登记**（不假装达标） | ① `::selection` 的字/底对比度；② `:hover` 面上的文本对比度；③ 动效中间帧；④ `.chat-thought`/`.md blockquote` 的 3px / 2px 装饰竖线（装饰，无下限） | 新增；提醒 t7 不要把这些算进"通过" |
| **A.6** | 亮色 `--control-border` 对 canvas | `#8a9198` 对白 **3.19** ✔ / 对 canvas **2.98**（差 0.02） | 新增。裁定：控件都坐在 panel（白）上；t7 须实测确认，若发现裸 canvas 上的控件则改 `#878e96` |
| **A.7** | **`[aria-expanded]` 不是 0** | `#chat` **3**、`#task` **3**、`#memory` **1**、`#settings` **1**，其余 9 路由 0（全部来自 antd Select 的 combobox） | **修正 MASTER §11.4 与阈值行 23**：按现状判据（全站计数）该行会**虚假通过**；契约改为只判手写展开控件（§5.2.2） |
| **A.8** | `.ws-folder` 的亮色 filter | 删除 `saturate(1.12)` 带来 **−12% 饱和度**，差异**未测** | 新增；换得"色度可计算"（§7.1） |
| **A.9** | 横向溢出复测 | 390：`#stats` **+99px**，其余 12 路由 0；768 / 1280 / 1440：**13 路由全 0** | 与 MASTER 行 19 基线**完全一致** ✔ |
| **A.10** | 焦点走查复测 | `#chat` **5 个无环**（`.ant-input-sm` / `textarea` / 3× `.ant-select-input`，与 MASTER 列的 5 个元素**逐一对应**）；`#board` **0**；`#sessions` **0**；`#settings` **1**（`.ant-select-input`，MASTER 记 2） | 5 个一致 ✔；`#settings` 的 2 → 1 **登记为差异**（可能 MASTER 在另一种面板状态下测）。手写环指定色 `color(srgb .941 .663 .231/.65)` 与 antd `3px rgb(86,65,32)` / `rgb(255,234,171)` **逐一复现** ✔ |
| **A.11** | **UA 默认环会让行 16 虚假通过** | `.readout-btn` 与 `.ant-segmented-item-input` 在 Tab 时是 `outline: auto 1px`（未落入任何 token） | **MASTER 未记录**；契约把 `outline-style: auto` 判为不合格（§5.1） |
| **A.12** | 手写环暗色方差 | 我复算的合成值 = `#a57833` → **4.24:1**（MASTER 记 4.26） | 差 0.02，两者都 ✔；登记原因为"口径差（合成取整）" |
| **A.13** | 内联 style 实测（13 路由） | sessions **732** / stats 214 / task 163 / board 39 / home 36 / agents 32 / runtimes 31 / memory 29 / settings 27 / chat 22 / inbox 19 / knowledge 18 / graph 19；内联 `font-size`：sessions **314** / task **131** / stats **80** / runtimes **3**，其余 0 | 内联 `font-size` 与 MASTER 行 11 基线**逐条一致** ✔；`#sessions` 的内联 style 为 **732**（MASTER 记 731）→ 登记 |
| **A.14** | 死类/死规则 0 引用证据（源码级） | `.nums` 0、`.empty-icon` 0、`.readout-row` 0、`.view-bar .sub` 0、`.tool-io .io-label` 0、`--surface-shadow` 0 次 `var()` 引用；`.state.empty` **1**（`Chat.tsx:1259`） | 新增；这是 §10.4 的删除依据 |
| **A.15** | 暗色 `.tag.err` 的低对比风险（**估计，非实测**） | 若暗色 tag 底是 16% 洗色，`.tag.err` 文本 `#f0615c` 只有 **3.99–4.23**（10–16% 区间） | **MASTER 行 13 说暗色 0 失败**，与此估计矛盾。**结论：暗色 tag 底色是 antd 派生值，未逐条核定**；t7 必须逐 tag 变体 × 两模式**实测 6 组**，实测前不得据此改色 |
| **A.16** | `.chat-ws-title` 的 600 | `font-weight:600` 是**合法档**（阶梯含 600） | 本文 §10.1 M84 只改其 12/17 → 12/16，不改字重（避免无谓改动） |
| **A.17** | **B7 的 `≤520` 特化缺失（数值冲突）** | `@390` 视口：`.content` 内容盒 **294px**，条带 `padding 16px 20px` → 网格内容盒 **254px < 280px**，B7 的 `auto-fit minmax(140px,1fr)` 只能产生 **1 轨**（1 列 × 4 行，条高 264px）；加 2 轨特化后实测 **2 轨 × 2 行**，同一份内容下条带高 264px → **148px**（真实读数为 164px，见 view-home / view-memory）。`@416` 内容盒 280px → 两轨各 140px（前置条件边界）；`@768` / `@1440` 仍是 4 轨 × 1 行、条高 90px，与 flex 版一致 | **修正 primitives §3.2 B7 + README §3.4 X4**：原 B7 公式与 X4 的「≤520 2×2」在 254px 内容盒下数学上不可同时成立。契约补上「≤520 由 2 轨特化接管」与「2×2 需内容盒 ≥280px / 视口 ≥ 约 416px」。520 属已批准断点阶梯，未引入新断点。**t26**：该修饰符整块移进 `≤520` 媒体查询，`>520` 回落 flex（520/768/1024/1280/1440/1920 六档 parity 实测全 EQUAL）。**t29**：删除因此永不生效的 auto-fit 声明，并登记 §10.4（扫描方法见附录 B.7）（2026-09-21） |

---

## 附录 B：复现命令

> 全部脚本在 `panel/node_modules` 里用 Playwright；**不写入仓库**（写到系统临时目录）。
> daemon 必须已在 `127.0.0.1:8787` 提供 `panel/dist`。

### B.1 13 路由的结构/内联/无障碍计数

```bash
# 每路由：h1 / aria-expanded / .zone* / 内联 style / 内联 font-size / 空态实例
node /path/to/measure.mjs          # 脚本见 B.4；一次输出 13 行 JSON
```

### B.2 Tab 走查（焦点环）

```bash
node /path/to/tabwalk.mjs          # 两模式 × {chat,settings,board,sessions,home}
# 输出每个停留点的 outlineStyle/Width/Color、boxShadow、命中区尺寸
```

判定（本文 §5.1 修正口径）：

```
pass  ⟺ activeElement 满足
        ( outlineStyle==='solid' && outlineWidth>=2 && contrast(composite(outlineColor), 相邻背景) >= 3.0 )
        ∨ ( 命中 .composer 的 :focus-within 例外 )
```

### B.3 阈值行快速核对（本文用到的几条）

```bash
# 行 19 横向溢出（390/768/1280/1440 × 13 路由）
#  — 390 期望：仅 #stats 非 0（+99）
# 行 24 每路由 h1（期望 home=1，其余 0 → 落地后应全为 1）
# 行 32 令牌唯一性（源码级；检查集 = 5 个纯语义令牌，accent 不在内 —— MASTER §12.3）
grep -n -- '--signal:\|--status-warn:\|--brand-claude:\|--brand-deepseek:\|--brand-opencode:' panel/src/index.css
# 行 34 :active 规则（源码级；落地后应 ≥1 条合写规则）
grep -n ':active' panel/src/index.css
# 行 35 断点值（期望去重后 ⊆ {520,768,1024,1240}）
grep -o 'max-width: *[0-9]*px' panel/src/index.css | sort -u
# 行 10 圆角越界（源码级粗筛）
grep -o 'border-radius: *[0-9]*px' panel/src/index.css | sort | uniq -c
```

### B.4 本文用到的两个临时脚本（原文照抄即可）

`/tmp/measure.mjs`（节选，13 路由计数）：

```js
const routes = ['home','chat','sessions','board',`task/${TASK}`,'memory','knowledge',
                'graph','agents','runtimes','stats','settings','inbox'];
for (const r of routes) {
  await p.goto(`${BASE}/?mode=dark#${r}`, { waitUntil: 'load' });
  await p.waitForTimeout(2200);
  const m = await p.evaluate(() => ({
    h1: document.querySelectorAll('h1').length,
    ariaExpanded: document.querySelectorAll('[aria-expanded]').length,
    zone: document.querySelectorAll('.zone,.zone-head,.zone-title').length,
    inlineStyle: [...document.querySelectorAll('*')].filter(e => e.getAttribute('style')).length,
    inlineFontSize: [...document.querySelectorAll('*')]
      .filter(e => (e.getAttribute('style') || '').includes('font-size')).length,
    emptyImgMarginBottom: (() => {
      const el = document.querySelector('.empty-state .ant-empty-image');
      return el ? getComputedStyle(el).marginBottom : null;
    })(),
    overflow: document.documentElement.scrollWidth - window.innerWidth,
  }));
  console.log(r, JSON.stringify(m));
}
```

`/tmp/tabwalk.mjs`（节选，两模式 Tab 走查）：

```js
for (const mode of ['dark','light']) {
  for (const r of ['chat','settings','board','sessions','home']) {
    await p.goto(`${BASE}/?mode=${mode}#${r}`, { waitUntil: 'load' });
    await p.waitForTimeout(1800);
    await p.evaluate(() => document.body.focus());
    for (let i = 0; i < 26; i++) {
      await p.keyboard.press('Tab');
      /* 记录 activeElement 的 outlineStyle/Width/Color 与 boxShadow */
    }
  }
}
```

### B.5 本文所有颜色数都是这样算出来的（单文件，无依赖）

```python
def lin(c):
    c = c / 255
    return c / 12.92 if c <= 0.03928 else ((c + 0.055) / 1.055) ** 2.4
def L(hexs):
    h = hexs.lstrip('#'); r, g, b = (int(h[i:i+2], 16) for i in (0, 2, 4))
    return 0.2126*lin(r) + 0.7152*lin(g) + 0.0722*lin(b)
def ratio(a, b):
    la, lb = L(a), L(b); hi, lo = max(la, lb), min(la, lb)
    return round((hi + 0.05) / (lo + 0.05), 2)
```

带 alpha 的前景**先线性合成**再代入 `ratio()`：

```python
def comp(fg, a, bg):
    f, b = fg.lstrip('#'), bg.lstrip('#')
    return '#%02x%02x%02x' % tuple(
        round(int(f[i:i+2], 16)*a + int(b[i:i+2], 16)*(1-a)) for i in (0, 2, 4))
```

### B.6 `.zone*` 删除条件的判定命令（§9.1）

```bash
# 落地后统计 .zone-head 命中的路由数；< 3 则触发删除条件
# 用 B.4 的脚本把 zone 字段替换为：
document.querySelectorAll('.zone-head').length;
### B.7 "被覆盖后永不可达"声明的扫描命令（t29）

**方法**：把 CSS 解析成 (媒体条件, 选择器, 声明) 三元组；若**同一个选择器**在**后出现**的另一条规则里
重复声明了某个属性，且后者的媒体范围**包含**前者的范围（`max-width` 越大范围越大，无媒体 = 全宽），
则前者那条属性永不可达。只覆盖这一类；**跨选择器覆盖**（`.a` 被 `.a.b` 覆盖）与
「选择器永不匹配」不在其内（后者见 §3.5 / §3.6 的类名引用扫描）。

```python
import re, sys
src = open(sys.argv[1], encoding='utf-8').read()
rules, stack, i, line = [], [], 0, 1
def skip(i, line):
    while i < len(src):
        if src.startswith('/*', i):
            j = src.find('*/', i); line += src.count(chr(10), i, j); i = j + 2
        elif src[i].isspace():
            line += src[i] == chr(10); i += 1
        else:
            return i, line
    return i, line
def block_end(i, line):
    d = 1
    while i < len(src) and d:
        if src.startswith('/*', i):
            j = src.find('*/', i); line += src.count(chr(10), i, j); i = j + 2; continue
        if src[i] == '{': d += 1
        elif src[i] == '}': d -= 1
        elif src[i] == chr(10): line += 1
        i += 1
    return i, line
while True:
    i, line = skip(i, line)
    if i >= len(src): break
    if src[i] == '}':
        if stack: stack.pop()
        i += 1; continue
    j = i
    while j < len(src) and src[j] not in '{}': j += 1
    if j >= len(src): break
    head = ' '.join(src[i:j].split()); start = line
    line += src.count(chr(10), i, j)
    if src[j] == '}': i = j + 1; continue
    i = j + 1
    if head.startswith('@media') or head.startswith('@supports'):
        stack.append(head); continue
    if head.startswith('@'):
        i, line = block_end(i, line); continue
    body_start = i
    i, line = block_end(i, line)
    props = [(d.split(':', 1)[0].strip().lower(), d.split(':', 1)[1].strip()) for d in src[body_start:i-1].split(';') if ':' in d]
    rules.append({'media': ' && '.join(stack), 'sel': head, 'props': props, 'line': start, 'i': len(rules)})
def maxw(cond):
    if cond == '': return float('inf')
    hi = float('inf')
    for part in [x.strip() for x in cond.split('&&')]:
        m = re.fullmatch(r'@media[ ]*\(max-width:[ ]*(\d+(?:\.\d+)?)px\)', part)
        if not m: return None
        hi = min(hi, float(m.group(1)))
    return hi
by_sel, dead, skipped = {}, [], set()
for r in rules: by_sel.setdefault(r['sel'], []).append(r)
for sel, g in by_sel.items():
    for a in g:
        ra = maxw(a['media'])
        if ra is None: skipped.add(a['media']); continue
        for b in g:
            if b['i'] <= a['i']: continue
            rb = maxw(b['media'])
            if rb is None: skipped.add(b['media']); continue
            if ra <= rb:
                bp = set(p for p, v in b['props'])
                dead += [(a['line'], a['media'] or '(top level)', sel, p, v, b['line']) for p, v in a['props'] if p in bp]
print('rules:', len(rules), '| unanalyzable media:', sorted(skipped) or 'none')
print('DEAD:', len(dead))
for d in dead: print('  line %d [%s] %s { %s: %s } overridden by line %d' % d)
```

**t29 复核结果**：清理前 **2 条**（`.readout-strip.grid` 的 auto-fit 列定义；
`.chat-side-toggle { display: inline-flex }` 在 `768` 媒体块里重复一次）；清理后 **0 条**
（源码 407 条规则、构建产物 414 条规则各扫一遍）。不可分析的媒体条件 2 个
（`@media (hover: none)`、`@media (prefers-reduced-motion: reduce)`），按保守策略跳过并登记。

---

## 附录 C：本文与 MASTER / t2 请求的三方对齐表（t16 用）## 附录 C：本文与 MASTER / t2 请求的三方对齐表（t16 用）

| 名字 | MASTER 说什么 | t2 请求什么 | 本文最终裁决 | 最终名字 | 谁实现 |
|---|---|---|---|---|---|
| 视觉隐藏 | Y1 需要 `.sr-only` | 请求 `.sr-only` | 采纳 | `.sr-only` | t5 |
| 微文本 | T6 要求删内联 11px | 请求 `.micro`（11/**16**） | 采纳，取值改 **11/15** | `.micro` | t5 |
| 数据文本 | T2 收录 12/16 档 | 请求 `.data` | **拒绝**（= `.mono`，同义） | `.mono` | — |
| 焦点环 | A1 要统一环 | 请求 `--focus-ring` | 采纳（值一致） | `--focus-ring` + `--ant-color-primary-border` | t5 |
| 控件边 | C7 要独立 token | 请求 `--control-border` | 采纳（值一致） | `--control-border` | t5 |
| 来源点 | V1 要清琥珀 | 复用 `--brand-*` | 采纳 + 品牌色提升为变量 | `--brand-claude/-deepseek/-opencode` | t5 |
| workspace hue | 未登记 | 登记 `WS_HUES` | 部分受理（变量化 + 模式化，不新增 hex） | `--ws-1…--ws-10` | t5 |
| KPI | — | （README 未列） | 不受理 | `.readout-strip.grid` / `.stat-strip` | — |
| 空态 | P6 三态齐全 | 规格 §3.1 列了 `.state.empty` | 删除 | `Empty` | t5 |
| 数字工具类 | — | 规格 §3.1 列了 `.nums` | 删除（0 引用） | `.mono` / `.time` | t5 |
| 区带 | Z1 强制使用 | 规格已开始引用 `.zone*` | 保留 + 双类名采用路径 + 删除条件 | `.zone*` | t5/t16 |

---

## 附录 D：最终类名 / 令牌清单（一页，可直接转发给 t16）

**新增（4 个类 + 16 个令牌）**

```
类：      .sr-only   .micro   .icon-btn   .spinner-block   .error-state
          （+ .readout-strip.grid 修饰符）
令牌：    --focus-ring          （暗 #f5b457 / 亮 #8a5600）
          --control-border      （暗 rgba(255,255,255,.34) / 亮 #8a9198）
          --signal-text         （暗 #f5b457 / 亮 #8a5600）
          --brand-claude / --brand-deepseek / --brand-opencode
          --ws-1 … --ws-10
          --ant-color-primary-border（= --focus-ring，两模式 pin）
```

**修改（值变了，名字不变）**

```
--signal             亮 #8a5600 → #c88413        （暗不变）
--status-warn        暗 #f0a93b → #e2894f / 亮 #8a5600 → #8a4a12
--graph-edge         亮 rgba(15,23,42,.34) → .50
textTertiary(亮)     #6e737c → #5f646d
textQuaternary(亮)   #656a73 → #656a72
brand(亮)            #e0a02a → #c88413
tag.ok/.warn/.err    亮色三对钉死 5.97 / 5.89 / 5.11
--surface-shadow     删除（0 引用）
```

**拒绝（不要新增）**

```
.data  → 用 .mono（12/16 mono，逐字相同）
.kpi   → 用 .readout* / .readout-strip(.grid) / .stat-strip
```

**删除（13 份规格不得再引用）**

```
.nums   .state.empty   .empty-icon   .readout-row   .view-bar .sub   .tool-io .io-label
```

**状态色映射（`STATUS_COLORS` 最终版）**

```
done/completed                     → --status-ok
failed/cancelled/interrupted       → --status-err          ← interrupted 从 warn 迁移
in_progress/running/spawning       → --signal              ← 不再用 --ant-color-primary
waiting_permission/blocked         → --signal              ← 从 warn 迁移
queued/pending                     → --status-idle
（--status-warn 只服务 .tag.warn 与"子系统降级"标记，不再表示任何运行状态）
```

**`.zone*`**：保留；用**双类名**采用（`className="kanban-col zone"` /
`className="kanban-head zone-head"`），5 个采用点见 §9.1；
命中路由数 < 3 时触发删除条件。

**8px 圆角**：不加入阶梯；`.row-btn` / `.kanban-card` → 10，`.agent-avatar` / `.prompt-view` → 6，
`.brand-mark` 5 → 6，`.empty-mark` / `.chat-hero-mark` 12 → 10，`100px` → `999px`（视图侧）。
