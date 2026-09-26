# ruagent Panel 设计系统总纲 — Bench

> **文档地位**：本文件是 panel 前端（13 条 hash 路由）的**实现级设计契约**。它把
> `docs/design-language.md` 里已经落地的 "Bench" 方向扩写成可执行规格：每个数字都有来源，
> 每条规则都可机器判定，每一处「现状 ≠ 规格」都显式登记为**建议变更**（附理由与迁移成本），
> 不静默改数。
>
> - **版本**：v1（2026-09-21）
> - **适用代码**：`panel/`（React 19 + antd 6 + Vite 6，hash 路由）
> - **规格来源**：`panel/src/theme.tsx`（`D` / `L` 两个 token 对象）、`panel/src/index.css`（角色与组件层）
> - **本文件不改代码**。落地由 `systems`（共享层唯一写者）与各 `ui-*` 视图成员按本文件执行。
> - **数字口径**：全部来自 DOM 计算样式直方图与截图像素统计（可复现，见附录 A）。
>   本环境图像桥不可用，**没有任何人看过像素**；因此本文不出现"看起来更好"这类判断，
>   只出现比值、计数、直方图。

---

## 0 导读：三件事怎么用这份文档

| 你要做的事 | 看哪一节 | 硬约束 |
|---|---|---|
| 新增/修改一个颜色 | **3 色彩** | 只能取 token；任何裸 hex 都是缺陷 |
| 新增/修改一段文案样式 | **4 字体阶梯** | 只能取阶梯内的 size/line-height/weight |
| 决定"这块内容要不要包一个盒子" | **7 容器三角色判定规则** | 判定树是强制的，答案只有 zone / panel / overlay |
| 写交互反馈 | **8 交互态** | 7 态每个都要有定义值 |
| 加动画 | **9 动效** | 只能取 4 个时长 token |
| 加断点 | **10 响应式断点行为** | 只能取 4 个断点 |
| 判断"做完没有" | **12 验收阈值表** | 每行可机器判定，`npm run build` + 审计脚本 |

**冻结契约（不可破坏）**：e2e 依赖的选择器
`.view-bar h2` / `.card.launcher` / `.kanban-col` / `.row-btn` / `.agent-card` / `.inbox-card` /
`.ant-table` / `.ant-empty` / `.composer` / `.composer-controls .ctl-select` / `.chat-side-toggle` /
`.chat-side.open` / `.chat-side-backdrop` / `.brand-mark` / `.brand-name` / `.dash-grid` / `.dash-head` /
`.guide-grid` / `.home-hero h1` / `.kbd-hint` / `.app-sider` / `.cmdk*`；
侧栏宽 228（收起 72）；390 / 768 / 1280 / 1440 视口不得横向溢出。
本文件的所有建议都必须在这组契约内落地——凡与之冲突的建议，在表中已标注「受限」。

---

## 1 设计原则

六条。每条后面跟一个**可判定的推论**——原则是否被违反，由推论对应的阈值行（见第 12 节）判定。

**P1 材质代替描边（Material, not borders）。**
表面的身份由明度台阶表达，不由 1px 线表达。当"这个东西是一块面板"需要用户找边框才能确认时，就是缺陷。
*推论*：可见描边的内容容器 ≤1/路由，且那 1 个必须是控件单元（如 `.composer`）。

**P2 一个信号，一个含义（One signal: amber）。**
全站只有一个强调色，它只表示"现在"——进行中、等你处理、主操作。信号被稀释到第四个含义时，"现在"就不存在了。
*推论*：信号色像素占比 ≤5%，且**出现位置必须命中第 3.2 节白名单**；`--status-warn` 与 `--signal` 不得同值。

**P3 读数要大（Readouts are big）。**
层级来自**跳档**的字号阶梯，不来自字重。每个视图开场有一块仪表盘：2–4 个数字，32/40 mono，用规则线分隔而不是四个一模一样的盒子。
*推论*：每路由 ≥18px 的可见文本节点 ≥3；含仪表的页最大字号 ≥32px。

**P4 规则分隔、面板包围、区带分组（Rules divide, panels bound, zones group）。**
三角色取代"什么都套一个盒子"的条件反射。判定树是强制的（第 7 节）。
*推论*：新增容器必须先在第 7 节判定树里定位，不允许出现第四种角色。

**P5 只有真实数据用 mono（Mono is for real data）。**
IBM Plex Sans 负责 UI 文案，IBM Plex Mono 负责 id / hash / 计数 / 时间戳 / 代码。**mono 绝不用于标签**，但用于**大号读数**。
*推论*：`.readout-label` 必须是 sans；`.mono` / `.time` / `.build-id` / `.conn` / `.readout` / `code` 的内容必须是机器生成的数据。

**P6 每个状态都有定义（Every state is defined）。**
hover / active / focus / selected / disabled / loading / empty / error 八态里，任何一态缺失都是缺陷；其中 error 必须与 empty 可区分——**"加载失败"不得渲染成"还没有数据"**。
*推论*：第 12 节中 error 与 empty 各占一行，且 error 行以 `role=alert` + 重试按钮为判定条件。

---

## 2 材质阶梯

### 2.1 台阶定义（暗色优先）

暗色下四级台阶的**原始相对亮度比**（`L = 0.2126R' + 0.7152G' + 0.0722B'`，sRGB 反伽马）：

| 级 | token | hex | 相对亮度 L | 相对上一级 |
|---|---|---|---|---|
| shell（app 外壳/overscroll） | `D.appShell` | `#08090b` | 0.0027 | — |
| canvas（页面底） | `D.pageCanvas` | `#0f1014` | 0.0052 | **1.93x** |
| panel（默认面板） | `D.surface` | `#1b1e24` | 0.0129 | **2.48x** |
| raised（浮起的面板/选中面） | `D.surfaceRaised` | `#262a31` | 0.0229 | **1.78x** |
| selected（选中态面） | `D.surfaceSelected` | `#333844` | 0.0395 | 1.72x |

> `docs/design-language.md` 说 "canvas → panel is ~2.5x in relative luminance"（原始亮度比）；
> 本次复测 **2.48x**，一致。
> 注意口径：若改用 WCAG 对比度公式，canvas→panel 只有 **1.14:1**——两个数都对，说的是不同东西。
> **判定一律用原始亮度比**，因为 WCAG 比在小亮度区会压缩到看不出台阶。

亮色下台阶弱得多，靠 **ring** 而不是明度差：

| 级 | token | hex | 相对亮度 L | 相对上一级 |
|---|---|---|---|---|
| shell | `L.appShell` | `#eaebee` | 0.8308 | — |
| canvas | `L.pageCanvas` | `#f6f7f9` | 0.9295 | 1.12x |
| panel | `L.surface` | `#ffffff` | 1.0000 | 1.08x |
| raised | `L.surfaceRaised` | `#ffffff` | 1.0000 | **1.00x（无台阶）** |
| selected | `L.surfaceSelected` | `#e4e7ec` | 0.7970 | 0.80x（**向内**） |

**亮色只有 3 个可用的明度台阶**（shell / canvas / panel），`surfaceRaised` 与 `surface` 同值。
所以亮色下"浮起"必须由 ring + shadow 承担：`--panel-shadow: 0 0 0 1px var(--rule-soft), 0 1px 2px rgba(15, 20, 30, 0.05)`。

### 2.2 每个台阶配什么装饰（唯一合法性表）

| 材质 | 暗色 | 亮色 | **禁止** |
|---|---|---|---|
| shell | 无 | 无 | 任何描边、阴影 |
| canvas | 无 | 无 | 面板级阴影 |
| panel | `--panel-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.055)`（顶边一道光） | ring：`0 0 0 1px #dee0e4` + `0 1px 2px rgba(15, 20, 30, 0.05)` | **drop shadow** |
| raised | 同 panel（顶边光）+ 需要时由 overlay 阴影接管 | ring + `0 1px 2px` | drop shadow（除非同时是 overlay） |
| overlay（菜单/模态/命令面板） | `D.menuShadow` = `0 12px 32px rgba(0, 0, 0, 0.44), 0 2px 8px rgba(0, 0, 0, 0.26)`；大型用 `D.floatingShadow` | `L.menuShadow` / `L.floatingShadow`（见 3.1） | **inset 顶边光**（浮层不在地上） |

**实测（本轮）**：13 路由中带 `box-shadow` 的可见元素数为
home 8、chat 1、sessions 203、board 26、task 4、memory 15、knowledge 8、graph 4、agents 8、runtimes 6、stats 25、settings 1、inbox 0。
其中 sessions 的 203 与 stats 的 25 需要逐条归因（大概率是 `.row-btn` 的 `box-shadow: 0 -1px 0 …` 行分隔线与表格 header 分隔），
**归因未完成，列入附录 B 的待办**；归因完成前不得据此调材质。

### 2.3 分隔线：用 shadow 还是 border

- **堆叠元素之间的 1px 线**：用 `box-shadow: 0 -1px 0 0 <rule>`（`.card > .row-btn + .row-btn` 的现行做法）。理由：不占布局，不引发布局抖动。
- **结构性分隔（页面头/区带头）**：用 `border-bottom: 1px solid var(--rule-soft)`（`.view-bar`、`.zone-head` 的现行做法）。
- **内容容器（panel）**：两侧都不用，靠材质。
- **判定**：一个 1px 线如果是"两个可点击行之间的边"，必须是 shadow；如果是"两个语义区块之间的边"，可以是 border。

### 2.4 规则线与控件边界的对比度（实测）

| 线 | 暗色合成后对底 | 亮色对底 | 结论 |
|---|---|---|---|
| `--rule`（`D.border` rgba(255, 255, 255, 0.13) / `L.border` `#d6d9df`） | **1.49:1**（panel 上） | **1.41:1**（白上） | 装饰级分隔线，**不受 3:1 约束** |
| `--rule-soft`（`D.borderSecondary` .07 / `L.borderSecondary` `#dee0e4`） | 1.22:1 | **对 canvas 1.2330:1 / 对白 1.3217:1**（旧记的 1.22 是**白底**值；基准已统一到画布，见 §12.7.1） | 同上 |
| 表单控件边界（antd `colorBorder` = 上面同一个 `--rule`） | **1.49:1** | **1.41:1** | **不达标**，见 3.3 建议变更 C7 |

WCAG 1.4.11 只约束"识别控件/状态所必需"的视觉信息。分隔线不算，**输入框的边界算**。

---

## 3 色彩

### 3.1 令牌全表（与 `panel/src/theme.tsx` 的 `D` / `L` 逐条对齐）

下表是 `theme.tsx` 中 `D` 与 `L` 两个对象的**逐条快照**（本文件撰写时实测一致），
外加 `index.css` 中 antd 无对应 token 的 `--*` 变量（标注来源）。

**A. 材质 / 表面**

| # | token | `D`（暗） | `L`（亮） | 角色 |
|---|---|---|---|---|
| A1 | `appShell` | `#08090b` | `#eaebee` | 外壳、overscroll |
| A2 | `pageCanvas` | `#0f1014` | `#f6f7f9` | 页面底 |
| A3 | `surface` | `#1b1e24` | `#ffffff` | 默认面板 |
| A4 | `surfaceRaised` | `#262a31` | `#ffffff` | 浮起面 |
| A5 | `surfaceHover` | `#262a31` | `#f0f1f4` | hover 面 |
| A6 | `surfaceSelected` | `#333844` | `#e4e7ec` | 选中面 |
| A7 | `panelTop`（inset 顶边光） | `rgba(255, 255, 255, 0.055)` | `rgba(255, 255, 255, 0)` | 面板顶边 |

> A4 亮色 = A3 亮色（`#ffffff`）。这是**实现现状**，不是笔误；建议变更见 C1。

**B. 分隔**

| # | token | `D` | `L` | 角色 |
|---|---|---|---|---|
| B1 | `border` | `rgba(255, 255, 255, 0.13)` | `#d6d9df` | 规则线 / 控件边 |
| B2 | `borderSecondary` | `rgba(255, 255, 255, 0.07)` | `#dee0e4` | 弱规则线 |
| B3 | `--rule`（index.css） | `rgba(255, 255, 255, 0.13)` | `#d6d9df` | = B1 |
| B4 | `--rule-soft`（index.css） | `rgba(255, 255, 255, 0.07)` | `#dee0e4` | = B2（**镜像**，见下方注） |

> **B4 = B2 是「镜像」关系，不是「概念上相等」（2026-09-21 补，`t30` 实测发现）**
> `--rule-soft` 在 `index.css` 里是**字面量**（亮 `#dee0e4` / 暗 `rgba(255, 255, 255, 0.07)`），**不引用** `L.borderSecondary`；
> 而亮色 ring 正是由 `--panel-shadow = 0 0 0 1px var(--rule-soft)` 提供的（§2.2 的 panel / raised 行）。
> 所以**改 ring 必须两处同改**：只改 `theme.tsx` 的 `L.borderSecondary` 时，ring 与 5 条分隔线**一个像素都不会变** ——
> `t30` 实测：只改 TS 侧达不到行 30 的验收。`index.css` 里 `var(--rule-soft)` 有 **6 处使用**
> （1 处 ring + 5 处 border：`.brand` / `.sidebar-foot` / `.view-bar` / `.chat-side` / `.composer`；另有 1 处出现在注释里）。
> **现行值两处一致**：`#dee0e4`（亮）；改值依据、算式与影响面见 §12.7.1。暗色两处同为 `rgba(255, 255, 255, 0.07)`，本次未动。

**C. 文本**（括号内为**本次复测**的对比度，底色 = panel `A3` / canvas `A2` / shell `A1`）

| # | token | `D` | `L` | 暗：panel/canvas/shell | 亮：panel/canvas/shell |
|---|---|---|---|---|---|
| C1 | `text` | `#f4f5f7` | `#0c0d10` | 15.31 / 17.43 / 18.26 | 19.43 / 18.13 / 16.30 |
| C2 | `textSecondary` | `#adb2bb` | `#5b6068` | 7.84 / 8.93 / 9.36 | 6.33 / 5.90 / 5.31 |
| C3 | `textTertiary` | `#8e939c` | `#6e737c` | 5.41 / 6.16 / 6.45 | 4.77 / **4.45** / **4.00** |
| C4 | `textQuaternary` | `#838891` | `#656a73` | **4.69** / 5.34 / 5.59 | 5.44 / 5.07 / 4.56 |
| C5 | `link` | `#f5b457` | `#8a5600` | 9.18 / 10.46 / 10.95 | 6.16 / 5.74 / 5.16 |
| C6 | `ink`（信号上的字） | `#1a1206` | `#241703` | 对 `brand` 9.22 | 对 `brand` 7.70 |

**加粗 = 低于 4.5:1**。两处：
- `C3`（亮）在 canvas 4.45、在 shell 4.00 → **13/14px 正文失败**（实测线上节点：`#board` 的 `.muted` 4.45）；
- `C4`（暗）在 panel 4.69 → 达标但仅余 0.19 冗余；`C4` 是 11px micro 文本的主色（`.time` / `.build-id` / `.chat-field > span`）。

**D. 信号（唯一强调色）**

| # | token | `D` | `L` | 用途 |
|---|---|---|---|---|
| D1 | `brand` | `#f0a93b` | `#e0a02a` | 唯一强调色的**填充档**（主按钮底、信号点） |
| D2 | `link` | `#f5b457` | `#8a5600` | 强调色的**文本档**（可读文本 ≥4.5:1） |
| D3 | `ink` | `#1a1206` | `#241703` | 填充档上的文字色 |
| D4 | `--signal`（index.css） | `#f0a93b` | `#8a5600` | 组件层直用（`.readout.signal` / `.inbox-card` / `.live-dot`） |
| D5 | `--signal-ink`（index.css） | `#1a1206` | `#241703` | = D3 |
| D6 | `--signal-wash`（index.css） | `rgba(240, 169, 59, 0.14)` | `rgba(224, 160, 42, 0.16)` | 洗色底 |
| D7 | `navSelectedBg` | `#2c2517` | `#fbf0da` | 当前导航项底（洗色） |
| D8 | `navSelectedColor` | `#f5b457` | `#7a4c00` | 当前导航项字 |

**实测**：D7 对 A3 只有 1.10:1（暗）/ 1.13:1（亮）——它是**洗色**，不是可辨识边界；
所以当前导航项**必须**同时有 D8 文本（8.34:1 / 6.49:1，实测）与 2px 左脊（`.sider-nav .ant-menu-item-selected::before`）。三者缺一即缺陷。
**D4 在亮色 = `#8a5600`（相对亮度 0.121），在暗色 = `#f0a93b`（相对亮度 0.472）**——见建议变更 C6。

**E. 状态**

| # | token | `D` | `L` | 对底（暗 panel / 亮 white） |
|---|---|---|---|---|
| E1 | `success` / `--status-ok` | `#4fb477` | `#17793b` | 6.46 / 5.48 |
| E2 | `warning` / `--status-warn` | `#f0a93b` | `#8a5600` | 8.30 / 6.16 |
| E3 | `error` / `--status-err` | `#f0615c` | `#c4121c` | 5.22 / 6.09 |
| E4 | `--status-idle` | `#838891` | `#656a73` | 4.69 / 5.44 |

> **E2 与 D1/D4 同值**（暗 `#f0a93b`，亮 `#8a5600`）——这是**规格冲突**，见建议变更 C2。

**F. 图谱分类色**（非文本标记，目标 ≥3:1；实测全部达标）
> **⚠️ 色值以实现为准，本文档不记录具体色值（2026-09-23 约定）** ✓ —— **`--graph-*` 的色值唯一来源 = `panel/src/index.css` 的暗色块与亮色覆盖块** ✓（**消费处唯一**：`panel/src/views/Graph.tsx` 的 `readPalette()` ✓）。**本表只记录：令牌名 · 判据（非文本标记 ≥3:1）· 目标列** ✓ —— **具体色值与实测比值一律不写在这里** ✗（**同一事实两处出现必然漂移**：**t153 重排 9 个类别色后，本表的旧 hex 已全部过期** ✓）。
> **验证方式**：**由审计的判据行验证** —— **行 31**（图谱边对比度 ≥3.0 两模式）· **行 33**（图谱分类色非文本对比度 ≥3.0）✓ —— **不是靠文档比对** ✓。

| # | token | `D` | 亮对白 / 亮对 canvas | 暗对 A3 |
|---|---|---|---|---|
| F1 | `--graph-person` | 见 `index.css`（暗色块） | 见 `index.css`（亮色覆盖块） | 6.37 |
| F2 | `--graph-org` | 见 `index.css`（暗色块） | 见 `index.css`（亮色覆盖块） | 6.47 |
| F3 | `--graph-project` | 见 `index.css`（暗色块） | 见 `index.css`（亮色覆盖块） | 7.16 |
| F4 | `--graph-repo` | 见 `index.css`（暗色块） | 见 `index.css`（亮色覆盖块） | 7.12 |
| F5 | `--graph-tool` | 见 `index.css`（暗色块） | 见 `index.css`（亮色覆盖块） | 7.08 |
| F6 | `--graph-concept` | 见 `index.css`（暗色块） | 见 `index.css`（亮色覆盖块） | 6.37 |
| F7 | `--graph-product` | 见 `index.css`（暗色块） | 见 `index.css`（亮色覆盖块） | 6.79 |
| F8 | `--graph-protocol` | 见 `index.css`（暗色块） | 见 `index.css`（亮色覆盖块） | 6.82 |
| F9 | `--graph-default` | 见 `index.css`（暗色块） | 见 `index.css`（亮色覆盖块） | 4.69 |
| F10 | `--graph-edge` | `rgba(255, 255, 255, 0.34)` | `rgba(15, 23, 42, 0.34)` | **3.08** ✔ / **2.17** ✘ |
| F11 | `--graph-edge-hi` | `rgba(255, 255, 255, 0.55)` | `rgba(15, 23, 42, 0.55)` | 4.7+ / 3.99 |
| F12 | `--graph-label` | 见 `index.css`（暗色块） | 见 `index.css`（亮色覆盖块） | 7.88 / — |

> **F8 已换非暖色相（2026-09-22 t45 落地；2026-09-23 起值不在此记录）**：暗 `#ca9d33`（H=42°，暖黄，与信号 `#f0a93b` H=36° 同色系）→ **换为非暖色相（H≈227°，值见 `index.css`）**；亮 `#8b6100`（H=42°）→ **同族加深（值见 `index.css`）**。`#ca9d33` / `#8b6100` 是「改前值」，保留为历史记录 ✓；**「改后值」不写在这里**（t153 又重排过一次 ⇒ 任何写死的「新值」都会过期 ✗）✓
> **新值的对比度：两模式 ≥3 ✔（审计行 33）** —— 具体比值以审计读数为准，不在此记录 ✓
> 新值对比度：**暗 对 panel 6.82 / 对暗画布 7.76；亮 对白 4.65 / 对亮画布 4.34**（两模式 ≥3 ✔，行 33）。
> **画布 A/B 实测**（唯一变量是该 token）：旧 token → 琥珀 165 px / 周蓝 0；新 token → 琥珀 **0** / 周蓝 **164**。
> 换色理由：`view-graph.md` §3 的「禁止」条 —— 琥珀在 `#graph` 里会与「运行中」混淆（图谱里没有「运行中」这个概念）。
> 仍然成立的部分：F8 表达的是**实体类型**（不是「现在」），**图谱内允许**，但**不得离开图谱视图**。见 3.2 白名单第 6 条与建议变更 C5。
> F10 亮色不达标 → 建议变更 C4。

**G. 域外品牌色**（允许存在、不得外溢）

| # | 选择器 | 值 | 说明 |
|---|---|---|---|
| G1 | `.brand-claude` | `#d97757` | 官方品牌色 |
| G2 | `.brand-deepseek` | `#5786fe` | 官方品牌色 |
| G3 | `.brand-opencode` | `#121212`（暗色翻为 `#e8e8ea`） | 近黑标 |
| G4 | `.runtime-logo` | `color-mix(in srgb, currentColor 10%/22%, transparent)` | 品牌色派生瓷砖 |

**H. 必须消灭的调色（实测存在）**

| 位置 | 值 | 问题 |
|---|---|---|
| `index.css` `::selection`（暗） | `rgba(93, 107, 224, 0.32)` | 靛蓝，调色板外第 3 个色相 |
| `index.css` `::selection`（亮） | `rgba(79, 90, 214, 0.18)` | 同上 |
| `ui.tsx` / 各视图里的 `style={{ background: '#…' }}` | 10 个裸 hex（历史遗留） | 不受主题切换控制 |

**I. 三个分类色域互不借用（2026-09-21 增补，守护第 12 节第 33 行）**

站内的「分类色」不是一套，而是**三个各自独立的域**。每个域只能用自己的色档：

| 域 | 色档 | 允许出现的位置 | 表达的语义 |
|---|---|---|---|
| ① 图谱实体类型 | `--graph-*`（9 个实体档 + `--graph-edge` / `--graph-edge-hi` / `--graph-label`） | **仅 `#graph` 视图内** | 实体类型（person / org / project / repo / tool / concept / product / protocol） |
| ② 运行时品牌 | `--brand-claude` / `--brand-deepseek` / `--brand-opencode`（+ 未来同级品牌色） | `#runtimes` 的 `.runtime-logo` / `.brand-*`；`#sessions` 的**来源点** | 厂商身份 |
| ③ 会话来源标记 | **不新开色族** —— 与 ②**合并**，用 `--brand-*`；无品牌色的来源（`dsh` / `ruagent` / `codex` / 未知）一律中性 `--ant-color-text-tertiary` | `#sessions` 的 `.dot` | 这条会话来自哪个运行时 |

**合并裁决（③ → ②）及理由**：`SessionRecord.source` 的取值就是**运行时名**（`claude-code` / `opencode` / `dsh` …），与 `#runtimes` 的品牌标指向**同一实体类别**，不是另一套分类维度。若为它新开 `--source-1..5`，站内会出现 **4 套**分类色族（`--graph-*` 9 + `--ws-*` 10 + `--status-*` 4 + `--brand-*` 3），第 33 行的「一色一义」将不可维护。**`--source-*` 明确拒绝**（与 primitives.md §7 R6 同结论，零新增 hex）。

**禁止跨域借用（硬规则）**

1. `--graph-*` 不得离开 `#graph`（含 `--graph-protocol` —— 它**曾是**图谱色里唯一的暖黄档（暗 `#ca9d33` / 亮 `#8b6100`，H=42°），**2026-09-22 已按 §3.1 F8 换成非暖色相**（暗 `#8fa3e8` / 亮 `#5b6fc4`，H=227°/229°）；无论取什么色相，它只允许在图谱内表达实体类型）；
2. `--ws-*`（`#chat` 的 10 个工作区色）不得离开 `#chat`；
3. `--brand-*` 不得用作**状态**（运行中 / 成功 / 失败 / 等待）——状态只用 `--signal` 与 `--status-*`；
4. `--signal` / `--status-*` 不得借给任何分类域（它们表达「现在 / 结果」，不表达「类别」）；
5. **同一个 hex 不得在两个域里各自承担一个含义**：跨域引用的直接后果是**第 33 行失效**——行 33 要求图谱分类色「≥3:1 且一色一义」，一旦 `--graph-person` 同时也被用来表示一个运行状态，它的对比度目标与语义都无从判定。

**机器判定**：建立 (hex → 语义域) 映射并要求一对一；再按 `grep` 断言每个 `--graph-*` 的引用只出现在 `.graph-*` 选择器内、`--ws-*` 只出现在 `#chat` 的选择器内。

**域②同时是第 12 节行 32 的唯一性检查集**（`--brand-claude` / `--brand-deepseek` / `--brand-opencode`），见 §12.3。accent（`--brand` / `--ant-color-primary`）**不属于任何分类域**——它是 `--signal` 的渲染路径，与 `--signal` 同值是契约要求（primitives §2.4 C6），因此被显式排除出行 32 的检查集。

### 3.2 信号色：唯一含义 + 出现位置白名单 + 像素上限

**唯一含义（三选一，同一条规则）：现在 / 运行中 / 等你处理。**

**白名单（允许出现的全部位置）**

| # | 位置 | 具体选择器 | 实测证据（本轮） |
|---|---|---|---|
| W1 | 当前导航项 | `.sider-nav .ant-menu-item-selected`（D7 底 + D8 字 + 2px 左脊 `::before`） | 13/13 路由各 1 处（`li` + `svg>g>path` 计数 1） |
| W2 | 进行中的运行状态 | `.pill`（`in_progress`/`running`/`spawning`）、`.live-dot`、`.live-flag` | `#task` 2 处 `.pill`；`#chat` 1 处 `.live-dot` |
| W3 | 等你处理 | `.inbox-card`（`box-shadow: inset 3px 0 0 var(--signal)`）、`.nav-badge`（收件箱计数） | `#inbox` 卡片脊线 |
| W4 | 主操作 | antd `<Button type="primary">`、`.composer:focus-within` 边框、手写 `:focus-visible` 环 | 各路由 1 处 primray 按钮 |
| W5 | 仪表读数（条件） | `.readout.signal`——**仅当**该数字表示 in-flight 或 waiting | `#board` 1 处 |
| W6 | 图谱分类色 F8（`--graph-protocol`） | 仅 `#graph` 视图内 | 图谱画布 |
| W7 | 画布上的**选中环 / 搜索命中环** | `.graph-canvas` 的 `pal.ring`（= `--ant-color-primary`，即 `--signal`）：`Graph.tsx:623-628`，选中或命中的节点画 `lineWidth 2` 的外环（半径 `rr + 3`） | `#graph` 1 处（选中/命中时）；30 点网格点击实测 2 次命中，琥珀 **190 / 124 px** = 1440×900 的 **0.015%** |

**信号色的使用边界（2026-09-23 重述，随 W7 增补）**

- **含义集不变**（「现在 / 运行中 / 等你处理」），但「**现在**」明确包含**当前位置 / 当前选中 / 当前命中** —— W1（当前导航项）本来就是这个语义，W7 是它在画布上的同类（不是新语义）。
- **必须同时满足的三条**（W7 增补后，对 W1–W7 一体适用）：
  1. **只表达「现在」，不得表达「类别 / 身份」** —— 这正是 F8（`--graph-protocol`）从暖黄档换走的理由（见 §3.1 F8 注），也是 V1 的违规形态；
  2. **不得是唯一线索**：以信号色标记的对象必须另有非颜色线索（文字 / 标签 / 尺寸 / 容器），使「把信号色移走」后信息不丢失。W7 实测满足 —— 选中节点另有标签（`Graph.tsx:626-630` 的 `labeled`）与检查器（`:247-260`）；命中节点另有半径脉冲（`:612` `rr = n.r × 1.25·sin`）与标签；
  3. **不得进入数据可视化**（V3：图表条用 accent 编码数值）与**常态装饰**（V5）。
- **与行 32（信号令牌唯一性）的关系**：行 32 判的是**令牌值**（`{--signal, --status-warn, --brand-*}` 两两互不相同 + Δhue ≥8°），**W7 不新增也不修改任何令牌** ⇒ **行 32 不受影响**（仍 0 组重复）。W7 影响的只是 §3.2 的**像素上限**：它只在「有选中 / 有命中」时出现，实测 190 px `1440×900 = **0.015%**，远低于 1.5%/路由 的内部目标（`#graph` 无选中时 0.34%，本行增补前就在预算内）。
- **不采纳的替代方案（改色）**：把 `pal.ring` 换成中性 ink（`--graph-edge-hi` / `--control-border`，后者实测 暗 3.09 / 亮 3.19 对 panel）也能满足 ≥3:1，但会与系统既有事实矛盾 —— **W4 已经允许 accent 用于「手写 `:focus-visible` 环」与 `.composer:focus-within` 边框**，antd 自身的焦点环（F-7）也仍是 accent 派生且**刻意保留**（§12 行 32 的「刻意设计，保留」）。只改画布环而保留其余 accent 环，会造成「同一语义（当前交互目标）两种颜色」。
**黑名单（实测存在，必须清掉）**

| # | 违规 | 实测证据 | 为什么是违规 |
|---|---|---|---|
| V1 | `Sessions.tsx:sourceHue` 把 `claude-code` 映射为 `var(--ant-color-warning)` = **信号色** | `#sessions` 131 个 `.tag > .dot` 填充 `rgb(240,169,59)` | 131 行会话点着"现在"，"现在"失去含义 |
| V2 | `ui.tsx` `STATUS_COLORS` 把 `interrupted` / `waiting_permission` / `blocked` 全部映射到 `--status-warn` = 与 `--signal` 同值 | 令牌级同值（暗 `#f0a93b`，亮 `#8a5600`） | `interrupted` 是**过去时**，不能穿"现在"的颜色。**处置（2026-09-21）**：C2 已落地——`--status-warn` 现为暗 `#e2894f` / 亮 `#8a4a12`（与 `--signal` 不再同值），`interrupted` 改 `--status-err`，`waiting_permission` / `blocked` 改 `--signal`。本行作为**历史基线**保留，不静默改数；行 32 的判据因此改挂运行时品牌族，见 §12.3 |
| V3 | `#stats` 图表条用 antd 派生的 `--ant-color-primary`（实测 `rgb(207,147,53)`） | 9 个 `div` 背景为该色 | 数据可视化不是"现在" |
| V4 | `#memory` 次级文本按钮用 antd link/primary 文本档（实测 `rgb(211,156,77)`） | 10 个 `button`（"替代"、"＋ 新建命名空间"） | 次级动作不是"现在" |
| V5 | `.guide-icon { color: var(--signal) }` 等常态装饰图标 | `#home` 指南卡图标 | 装饰不是"现在" |

**像素上限**

- 全站任意路由，信号色像素 ≤ **5.0%**（硬上限）。
- 更严的内部目标：**≤1.5%/路由**（不含 W1 导航态时 ≤1.0%）。
- 口径：截图像素 `HSV` 中 `20° ≤ H ≤ 50°` 且 `max-min > 25` 且 `S > 0.30` 且 `V > 110/255`。
- **实测（本轮，1440×900 暗色）**：home 0.73%｜chat 0.51%｜sessions **0.94%**｜board 0.48%｜task 0.34%｜memory 0.81%｜knowledge 0.72%｜graph 0.34%｜agents 0.66%｜runtimes 0.48%｜stats 0.36%｜settings 0.67%｜inbox 0.13%。
  → **上限已达标**；真正的风险不是面积，而是**语义稀释**（V1–V5）。

### 3.3 建议变更（每条：现状 → 建议 → 理由 → 迁移成本）

> 规则：**凡与现有实现不一致处，只登记建议，不静默改数。** 每条都给可复现证据。

| ID | 现状（实测） | 建议 | 理由 | 迁移成本 |
|---|---|---|---|---|
| **C1** | `L.surfaceRaised === L.surface === #ffffff`；亮色只有 3 个明度台阶 | 保留同值，但在规格中**显式声明**"亮色 raised 由 ring 承载"；并把 `--panel-shadow`（亮）升格为 raised 的定义性装饰 | 现状是一致实现，不是 bug；但文档必须承认这一差异，否则新视图会去调明度而破坏 ring 体系 | **S**（仅文档 + 1 行注释） |
| **C2** | `--status-warn` 与 `--signal` 同值（暗 `#f0a93b` / 亮 `#8a5600`）；`interrupted` 穿信号色 | `--status-warn` 改为**独立**值；`interrupted` 归 `--status-err` 或 `--status-idle`；`waiting_permission`/`blocked` 保留信号色（它们确实是"等你处理"） | 令牌同值 = 一个颜色两个含义，直接违反 P2。这是"信号色被稀释"的**根因**，且是令牌级可判定缺陷 | **S**（`index.css` 2 行 + `ui.tsx` 1 张表；视觉影响：中断状态从琥珀变红/灰） |
| **C3** | 亮色 `.tag.ok`：`color:#14691f` on `background:rgb(173,184,175)` = **3.34:1**（11px，实测像素确认：该矩形内 738/823 像素为 `(173,184,175)`） | 亮色下 pin `--ant-color-success-bg` 到淡绿（候选 `#e8f4ec` → 6.05:1）；或 tag 改为"描边+文字"不铺底 | 11px 文本需 4.5:1；当前 3.34 是**全站最差的正文对比度** | **S**（`index.css` 加 1 条 `html[data-mode="light"]` 规则；需同步 `.tag.warn` / `.tag.err` 一并实测） |
| **C4** | `--graph-edge`（亮）`rgba(15, 23, 42, 0.34)` 对白 = **2.17:1** < 3:1；暗色 `.34` = 3.08:1 ✔ | 亮色 alpha `.34 → .50`（实测 3.41:1）；或 `.55`（3.99:1） | 图谱的**主数据**（边）必须满足非文本 ≥3:1 | **S**（`index.css` 1 行；图谱检查器 400px 固定宽需同步确认，见 R4） |
| **C5** | `#stats` 图表条用 `--ant-color-primary`（派生 `rgb(207,147,53)`）；`#memory` 次级按钮用 primary 文本档（`rgb(211,156,77)`） | 图表条改用中性档（`textSecondary` 或 `--graph-*` 分类色）；次级文本按钮改 `--ant-color-text-secondary`，hover 才上色 | 违反 P2 白名单 W1–W5；实测 V3/V4 分别 9 / 10 处 | **M**（`Stats.tsx` 图表与 `Memory.tsx` 按钮；需 e2e 回归） |
| **C6** | `--signal` 亮色 = `#8a5600`（L=0.121），暗色 = `#f0a93b`（L=0.472）——**同一个 token 兼做"文本档"和"填充档"**；亮色下 `.live-dot` / `.inbox-card` 脊线都是 `#8a5600` | 拆成两个：`--signal`（填充档，亮 `#c88413`→对白 3.10:1）与 `--signal-text`（亮 `#8a5600` → 6.16:1）；暗色两者都 `#f0a93b` | 6×6 的点在亮色下是"深褐"，在暗色下是"亮琥珀"——同一个语义跨模式换了身份。拆分后亮色填充档仍满足 ≥3:1（实测 `#c88413` = 3.10:1） | **M**（`index.css` + `ui.tsx` + 3 个视图引用；需两模式截图回归） |
| **C7** | 表单控件边界 = `--rule`：暗 **1.49:1** / 亮 **1.41:1**（实测合成） | 控件边界独立成 `--control-border`：暗 `rgba(255, 255, 255, 0.34)`（对 panel **3.10:1**）、亮 `#8f96a1`（对白 2.98，取 `#8a9198` → 3.19:1） | WCAG 1.4.11：识别输入框所必需。当前靠"输入框比底略亮"（1.14:1）几乎不可见 | **L**（涉及 antd token `colorBorder`，全站控件与表格线都受影响；须两模式逐路由回归，且 `.ant-table` 线可能与 2.1 节的描边计数冲突） |
| **C8** | 暗色派生的 hover/active 主色比 base **更暗**：base `#f0a93b`（ink 对比 9.22）→ hover 派生 `rgb(207,147,53)`（ink 对比 6.95） | 暗色下 pin `colorPrimaryHover` = `#f5b457`、`colorPrimaryActive` = `#f7c073`（沿材质"向上"而不是向下） | 暗色里 hover 应该更亮；当前 hover 让主按钮**变暗**，且降了 2.27 个对比度点 | **S**（`theme.tsx` 的 `components` 或 token 里 pin 2 个 key） |
| **C9** | `C4 textQuaternary` 暗色在 panel 上仅 4.69:1；`C3 textTertiary` 亮色在 canvas 4.45 / shell 4.00 | 亮色 `textTertiary` 改为 `#656a72`（canvas **5.08** / shell **4.57** / white 5.44）——但此时与 `textQuaternary #656a73` 撞值；**因此同批把 `textQuaternary` 提到 `#5f646d`**（shell 待测） | 亮色文本梯度目前是**非单调**的：`textQuaternary`(5.07 canvas) 比 `textTertiary`(4.45 canvas) **更可读**。四级阶梯在亮色下名不副实 | **M**（两个 hex，但影响所有亮色小字号；必须重跑对比度审计并对齐 12 节第 12 行） |
| **C10** | `::selection` 是靛蓝（`rgba(93, 107, 224, 0.32)` / `rgba(79, 90, 214, 0.18)`） | 改 `--signal-wash`，或改中性 `rgba(255, 255, 255, 0.14)` / `rgba(15, 20, 30, 0.10)` | 调色板外第三个色相，且是唯一残留的旧强调色 | **S**（1 行 × 2） |
| **C11** | `.brand-mark` 用中性色（刻意不用信号色） | **保持不变**，并在规格中写成硬约束 | `docs/design-language.md` 已声明；logo 若变琥珀，就会在导航里冒充"当前项" | **S**（文档） |

---

## 4 字体阶梯

### 4.1 阶梯表（唯一合法字号集合）

字体族：UI = `IBM Plex Sans Variable` → `Segoe UI Variable Text` → `Segoe UI` → `system-ui` → `PingFang SC` → `Microsoft YaHei` → `Noto Sans SC`。
数据 = `IBM Plex Mono` → `Cascadia Code` → `ui-monospace` → `Consolas`。
CJK 必须落在 PingFang SC / Microsoft YaHei，**不得落回浏览器默认字体**（`theme.tsx` 的 `FONT` 常量已保证）。

| 级 | sizepx / line-heightpx | weight | family | 用途（唯一） | 实测存在 |
|---|---|---|---|---|---|
| **readout** | 32 / 40 | 500 | mono | 仪表大盘数字、`#task` 标题区读数 | 25 个文本节点（13 路由） |
| **display** | 24 / 30 | 500 | mono/sans | `#home` hero h1、`readout.l` | 1 个节点 |
| **title** | 20 / 26 | 600 | sans | `.view-bar h2`、`.chat-hero h3` | 13 个节点 |
| **readout.s** | 18 / 24 | 500 | mono | 次级读数（卡片内） | 26 个节点 |
| **subhead** | 16 / 24 | 600 | sans | `.empty-title`、`.modal-head h3`、`.graph-detail-head h3` | 1 个节点（**本阶梯新增档**，见 4.3） |
| **section** | 15 / 22 | 600 | sans | `.zone-title`、`h3.sec` | 23 个节点 |
| **body** | 14 / 21 | 400 | sans | 正文 | 2,345 个节点 |
| **label** | 13 / 18 | 400–500 | sans | 行标题、表单标签、说明 | 23,784 个节点 |
| **data** | 12 / 16 | 400 | mono | 内联数据：`.mono`、id、命令、slug | 684 个节点 |
| **micro** | 11 / 16 | 400 | sans/mono | 时间戳、构建号、计数、kbd 提示（**仅数据**） | 1,076 个节点 |

**实测字号直方图（13 路由全部可见文本节点，1440×900 暗色）**：
`13px 23784｜14px 2345｜11px 1076｜12px 684｜18px 26｜32px 25｜15px 23｜20px 13｜24px 1｜16px 1`
→ **没有出现 10px / 12.5px / 17px**。`docs/ui-ux-review.md` 记录的"字号越界"（10/12.5/17px）在本轮**已清零**，实测确认。

### 4.2 mono 规则（硬约束）

1. **mono 只用于真实数据**：id、hash、slug、命令、路径、计数、时间戳、代码、token 数。
2. **mono 绝不用作标签**：`.readout-label`（12/17 sans）、`.stat-strip .ant-statistic-title`（12/16 sans）、`.zone-title`（15/22 sans）、`.field > span`（13/18 sans）——实测全部为 sans ✔。
3. **mono 用于大号读数**：`readout`(32) / `readout.l`(24) / `readout.s`(18) / `.stat-strip` 值（**30，越界见 4.3**）。
4. **数字一律 tabular**：`.nums, .time, .mono, table { font-variant-numeric: tabular-nums }`（现行）。
5. **不假造粗体/斜体**：`font-synthesis: style`（现行）。

**实测**：mono 文本节点数 home 41｜chat 5｜sessions 764｜board 36｜task 330｜memory 44｜knowledge 14｜graph 6｜agents 58｜runtimes 25｜stats 118｜settings 4｜inbox 4。
`#sessions` 的 764 中绝大多数是 `.time` 时间戳（真实数据 ✔）。**未发现 mono 标签**（人工核对 13 路由的 1,449 个 mono 文本节点的类名集合，全部落在 `.mono/.time/.build-id/.conn/.readout/.live-flag/.raw/.md code` 内）。

### 4.3 字重阶梯

| weight | 用途 |
|---|---|
| 400 | 正文、标签、说明 |
| 500 | 数据、读数、强调型按钮文字（antd `Button.fontWeight = 500`） |
| 600 | 标题族（title / subhead / section / 空态标题） |
| 700 | **仅** markdown 正文里的 `<strong>`（浏览器默认，不主动声明） |

**实测越界**：`font-weight: 450`（`.chat-session .title`，1 处）、`font-weight: 650`（`.graph-detail-head h3` 1 处、`.home-hero h1` 1 处）→ 见建议变更 T3。
实测权重分布：400 占绝大多数；700 出现在 200 个 `.row-btn`（sessions）、21 个 `.kanban-card`、96 个 task 节点——需归因为 `<strong>` 或行内 `<b>`，**归因未完成**，列入附录 B。

### 4.4 建议变更

| ID | 现状 | 建议 | 理由 | 成本 |
|---|---|---|---|---|
| **T1** | 阶梯声明 micro `11/16`，但 CSS 实际 **16 处用 11/15**、1 处用 11/17 | 把 micro 档**改成 `11/15`**，并把那 1 处 11/17（`.count`）归一 | 声明与实现不一致；实测 16:1 的多数票是 15px 行高 | **S** |
| **T2** | 12px 有 **14 处 12/16** + 6 处 12/17（`.stat-cell > .muted`、`.mem-sub`、`.chat-ws-title` 等） | 新增 **data `12/16`** 档（本规格已收录），12/17 全部归一到 12/16 | 12px 是 antd `fontSizeSM` 但原来没有设计档，导致 17px 行高乱入 | **M**（6 处 CSS） |
| **T3** | 450 / 650 两个越界字重（3 处） | 450 → 400 或 500；650 → 600 | 阶梯只允许 400/500/600/700 | **S**（3 行 CSS） |
| **T4** | `subhead 16/24` 未在阶梯中，但已被 3 处使用（`.empty-title`、`.modal-head h3`、`.graph-detail-head h3`） | **把 subhead 16/24 正式收录**（本规格已收录，weight 600） | 已经事实存在的档位，补进阶梯比拆掉它便宜 | **S** |
| **T5** | 行高越界：`14/22`（3 处：`.intent`、`.memory-card .memory-content`、`.home-hero p`）、`14/20`（3 处：`.steps strong`、`.guide-card strong`）、`13/20`（`.empty-hint`）、`15/24`（`.chat-msg.agent .md`）、`15/23`（`.composer textarea`）、`16/23`（`.cmdk-input-row input`）、`24/32`（`.home-hero h1`）、`30/38`（`.stat-strip` 值） | 全部归一到阶梯：14/21、13/18、15/22、24/30、32/40（`.stat-strip` 值改 32/40 并复用 `.readout`） | 同一字号两种行高会让相邻区块的基线对不齐；`.stat-strip` 的 30px 与 readout 32px 并存是纯重复 | **M**（8 处 CSS） |
| **T6** | 25 处 tsx 内联 `fontSize`，其中 `<span className="mono" style={{fontSize:12}}>` 是**冗余**（`.mono` 已声明 12/16） | 删除冗余内联；`<span className="muted" style={{fontSize:11}}>`（sessions **314 处**、stats 60 处）改为 `.micro` 类 | 实测：sessions 314 / task 131 / stats 80 / runtimes 3 个元素带内联 font-size；其中 `.mono`+12 属于重复声明 | **M**（1 个类 + 若干替换） |

---

## 5 间距与栅格

### 5.1 基准与阶梯

基准 **4px**。合法阶梯（唯一集合）：

| 名 | 值 | 用途 |
|---|---|---|
| `space-0.5` | 2px | 行内元素之间的微缝 |
| `space-1` | 4px | 图标与文字、chip 内边距 |
| `space-1.5` | 6px | 紧凑行的行距（`.row.tight`） |
| `space-2` | 8px | 控件之间的缝、表单字段内部 |
| `space-2.5` | 10px | 行内元素间距（`.row`） |
| `space-3` | 12px | 卡片之间的缝、区块内间距 |
| `space-4` | 16px | panel 内边距（下限） |
| `space-5` | 20px | panel 内边距（舒适）/ 仪表盘左右内边距 |
| `space-6` | 24px | 区块之间的间距 |
| `space-8` | 32px | 页面顶/底留白（上限值） |
| `space-20` | 80px | `#sessions` 这类长列表页底部的滚动缓冲 |

**实测间距值分布（12 路由聚合，元素计次）**：
`7(1106)｜8(1081)｜2(997)｜12(714)｜10(623)｜6(556)｜9(466)｜4(427)｜16(206)｜24(148)｜14(92)｜22(85)｜3(82)｜18(82)｜20(58)｜11(50)｜15(22)｜31.68(22)｜5(21)｜80(12)｜25.92(12)｜…`

**越界值（不在阶梯内）**：`7 / 9 / 11 / 14 / 18 / 22 / 3 / 5 / 15 / 21 / 23 / 28 / 31.68 / 25.92 / 4.5 / 8.5 / 37.44 / 64 / 76`。
其中 `31.68 / 25.92 / 37.44` 来自 `.content` 的 `clamp()`（见 5.3），是设计意图，**保留**。
其余（`7/9/11/14/18/22/3/5/15/21/23/28`）为历史魔数，权重低但应就近归一（建议变更 S1）。

### 5.2 页面栅格

- `.content`：`padding: clamp(16px, 1.8vw, 32px) clamp(16px, 2.2vw, 40px) 80px`；`max-width: 1560px`；居中。
  **实测**：1440 视口下 content 宽 1212（x=228），左右内边距 25.92 / 31.68；1920 视口下 content 宽 1560（触顶）。
- `.view-bar`：页面头，`padding-bottom: 14px` + `border-bottom: 1px solid var(--rule-soft)`，`margin-bottom: 22px`。
  → **14 与 22 都是越界值**，建议归一到 `space-4(16)` 与 `space-6(24)`。
- 侧栏：**228px / 收起 72px**（e2e 冻结）。≤992 视口自动收起（`App.tsx` matchMedia）。

### 5.3 容器内边距

| 容器 | 现行 | 规格 |
|---|---|---|
| `.card` | `padding: 16px 18px` | 上下 `space-4`(16) ✔；左右改 `space-5`(20) |
| `.panel` | 无内边距（由子元素决定） | 保持 |
| `.readout-strip` | `padding: 16px 20px` | 保持（这是 panel 内边距的基准样张） |
| `.pad` | `padding: 14px 18px` | 改 `16px 20px` |
| `.zone` | 无 | 保持（zone 不包围，见第 7 节） |

### 5.4 建议变更

| ID | 现状 | 建议 | 理由 | 成本 |
|---|---|---|---|---|
| **S1** | 间距出现 19 个非阶梯值（`7/9/11/14/18/22/3/5/15/21/23/28` 等） | 就近归一：`3/5→4`、`7/9→8`、`11→12`、`14→16`、`18→20`、`22→24`、`15→16`、`21→20`、`23→24`、`28→24` | 视觉上 7 与 8、22 与 24 无差异，但会让"间距是不是 token"无法机器判定 | **M**（约 25 处 CSS；需两模式回归） |
| **S2** | `.view-bar` 的 14/22 | → 16 / 24 | 同上，且 `.view-bar` 是全站出现频次最高的结构 | **S** |

---

## 6 圆角与描边

### 6.1 圆角阶梯

| 名 | 值 | 用途 |
|---|---|---|
| `radius-chip` | 4px | chip、tag、`.md code` |
| `radius-control` | 6px | 按钮、输入框、菜单项、行按钮 |
| `radius-panel` | 10px | `.card` / `.panel` / `.graph-canvas` / 运行时瓷砖 |
| `radius-overlay` | 14px | 模态、命令面板、浮层 |
| （特例） | `999px` / `50%` | 圆形按钮（`.send-btn`）；状态点 `.dot`（8px 圆） |

**实测（本轮的越界清单，按出现次数）**：

| 越界值 | 出现位置（实测计数） | 处置 |
|---|---|---|
| **8px** | `.row-btn`（sessions 200、stats 20、home 9、knowledge 4、task 1、chat 1）、`.kanban-card` 21、`.agent-avatar` 7、`.prompt-view` 7 | **最高优先级**：8px 事实上已经是控件/卡片的事实标准。二选一：(a) 把 `radius-control` 改成 8、(b) 全部归到 6/10。**建议 (b)**：`row-btn`/`kanban-card` → 10，`agent-avatar`/`prompt-view` → 6 |
| **5px** | `.brand-mark` 1/路由（13） | 改 6（视觉不可辨） |
| **12px** | `.chat-hero-mark`、`.empty-mark` | 改 10 或 14（视是否浮层） |
| **100px** | `#stats` 12 个 div（图例/条）、`#settings` 4 个（头像/圆片） | 统一用 `999px`（同样是圆，但要能机器判定） |

### 6.2 描边（边框）规则

**铁律：内容容器不描边。**

判定（与 e2e 的 `.ant-table` 契约相容）：

| 允许描边 | 条件 |
|---|---|
| 表单控件 | `.ant-input` / `.ant-select-selector` / `.ant-checkbox` / `.ant-radio` / `.ant-switch` / `.ant-segmented` |
| 按钮 | `.ant-btn`（`variant="outlined"` 时） |
| 状态标记 | `.tag` / `.pill` |
| 表格与 markdown 单元格 | `.ant-table td/th`、`.md td/th`（数据网格必须有线） |
| 域内标记 | `.runtime-logo`、`.prompt-view`、`.compare-card.selected`（`outline`） |

| 禁止描边 | 说明 |
|---|---|
| `.card` / `.panel` / `.zone` | 用材质（第 2 节） |
| `.kanban-col` / `.kanban-card` | 列用规则线 `.kanban-head`；卡片用材质 + hover 变色。**现有 `.kanban-card` 无描边 ✔** |
| `.row-btn` / `.guide-card` / `.mem-preview` | 行与卡片用 hover 面，不用框 |

**实测（严格口径：`DIV/SECTION/ARTICLE/LI/ASIDE/MAIN/UL` 且 ≥2 边可见描边且非控件类）**

| 路由 | 描边内容容器数 | 全部可见描边元素数 |
|---|---|---|
| home / sessions / board / task / memory / knowledge / graph / agents / runtimes / stats / settings / inbox | **0** | 4 / 4 / 8 / 1679 / 5 / 14 / 5 / 32 / 19 / 70 / 7 / 4 |
| chat | **1**（`.composer`，控件单元，合规） | 11 |

→ `docs/design-language.md` 的 "Bordered boxes are now 0 on every page" 在**严格口径下成立**（12/13 为 0，chat 的 1 个是控件）。
但**宽松口径**（只数"元素有没有任意一条可见边"）会得到 4–1679 的数字（其中 task 的 1679 几乎全是 markdown 表格单元格）。
因此本规格**冻结严格口径**并写进第 12 节第 4 行——这是必要的，否则"0 个盒子"永远无法机器判定。

### 6.3 建议变更

| ID | 现状 | 建议 | 理由 | 成本 |
|---|---|---|---|---|
| **R1** | 5 / 8 / 12 / 100px 四个越界半径，其中 8px 出现 ~240 次 | 按 6.1 表处置；`radius-chip/control/panel/overlay` = 4/6/10/14 不变 | 阶梯只有 4 档，实测 7 档 | **M**（`index.css` 6 处 + tsx 内联若干） |
| **R2** | `.row-btn` 当前 `border-radius: 8` | → 10（它承载的是"卡片里的一个实体"） | 与 `.card` 同面，圆角应同族 | **S** |
| **R3** | `.prompt-view` `border-radius: 8` **且有 1px border** | → 圆角 6；边框保留（它是"可展开的代码块"，属控件） | 唯一还在用 8 的带框元素 | **S** |
| **R4** | `.graph-detail` `flex: 0 0 400px; min-width: 320px` | 改为 `clamp(320px, 32vw, 460px)`；`.graph-canvas` 的 `height: 520px` 改为 `clamp(360px, 56vh, 620px)` | 固定像素在窄屏/矮屏不随视口自适应；与第 10 节的断点行为冲突 | **M** |

---

## 7 容器三角色判定规则（zone / panel / overlay）

**只有一个问题需要回答：这块内容的身份是什么？** 按顺序问，第一个"是"就是答案。

```
Q1  它是浮在页面之上、可以随时关掉的东西吗？
    （菜单 / 下拉 / 模态 / 命令面板 / 抽屉 / tooltip 宿主）
    └─ 是 → overlay   （有阴影，圆角 14，无顶边光）

Q2  它是一个可以被点击、被测量、被单独引用的实体吗？
    （一个任务 / 一个会话 / 一个 agent / 一个记忆 / 一个 chunk / 一块仪表盘）
    └─ 是 → panel     （有材质，圆角 10，顶边光或 ring，**无描边**）

Q3  都不是 → zone     （无盒，只有"规则线 + 标签"）
```

### 7.1 三者的定义性样式

| 角色 | 背景 | 圆角 | 装饰 | 内边距 | 描边 |
|---|---|---|---|---|---|
| `.zone` | 透明（继承 canvas/panel） | — | `.zone-head` 的 `border-bottom: 1px solid var(--rule)` | 0 | **禁止** |
| `.panel` / `.card` | `--ant-color-bg-container` | 10px | `var(--panel-shadow)` | 16–20px | **禁止** |
| overlay | `--ant-color-bg-elevated` | 14px | `D.menuShadow` / `D.floatingShadow` | 由组件定 | 允许 1px 内 ring |

### 7.2 判定示例（把现行代码映射进三角色）

| 现行元素 | 角色 | 依据 |
|---|---|---|
| `.view-bar` | zone | 页面头不是实体，只是一条规则 + 标签 |
| `.readout-strip` | panel | 一块仪表盘，可被单独引用 |
| `.kanban-col` | zone | **列不是盒子**：标题在规则线上，卡片才是实体（现行 CSS 注释已写明） |
| `.kanban-card` | panel | 可点击的实体 |
| `.row-btn` | panel（行形态） | 可点击的实体，靠 hover 面而非框 |
| `.card.launcher` | panel | 控件组，是可测实体 |
| `.agent-card` / `.inbox-card` | panel | 实体；`.inbox-card` 额外带 3px 信号脊（W3） |
| `.memory-card` | panel | 实体 |
| `.timeline-wrap` | panel | 一块仪表（飞行记录仪） |
| `.composer` | panel（控件单元） | 唯一允许带描边的内容容器 |
| `.cmdk` / `.modal` | overlay | 可关闭的浮层 |
| `#home` 的 `.guide-grid` 卡片 | panel | 可点击实体 |

### 7.3 实测现状

| 每路由计数 | home | chat | sessions | board | task | memory | knowledge | graph | agents | runtimes | stats | settings | inbox |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `.panel` | 1 | 0 | 1 | 1 | 0 | 1 | 1 | 1 | 0 | 1 | 1 | 0 | 0 |
| `.zone` | **0** | **0** | **0** | **0** | **0** | **0** | **0** | **0** | **0** | **0** | **0** | **0** | **0** |
| `.card` | 0 | 0 | 1 | 0 | 2 | 9 | 1 | 0 | 0 | 0 | 1 | 1 | 0 |

**关键发现**：`.zone` 作为一个已定义的类，**13 个路由一次都没用到**——视图实际靠 `.view-bar`、`.kanban-head`、`.zone-head` 的**内联实现**来扮演 zone。
→ 这不是缺陷（角色仍然成立），但意味着 `.zone/.zone-head/.zone-title/.zone-note` 这套原语**要么被采用，要么被删除**。见建议变更 Z1。

### 7.4 建议变更

| ID | 现状 | 建议 | 理由 | 成本 |
|---|---|---|---|---|
| **Z1** | `.zone*` 原语定义完整但 0 引用；`.view-bar` 等各自内联实现 zone 语义 | 在**逐视图规格**中强制：凡"规则线 + 标签 + 无盒"的结构都用 `.zone` / `.zone-head` / `.zone-title` / `.zone-note`；`.view-bar` 保持冻结（e2e 契约），内部可以复用 `.zone-title` 的字号档 | 三角色若第三色无人使用，规格就退化成两色，新代码会继续发明第四种盒子 | **M**（视图层批量替换；`.view-bar h2` 选择器必须保留） |
| **Z2** | `#home` / `#chat` / `#agents` / `#settings` / `#inbox` 的 `.panel` 计数为 0 | 这 5 个路由按第 7 节判定树逐块补齐角色标注（在逐视图规格中逐条落） | 计数为 0 意味着这些页的实体**没有用 panel 材质**，可能各自内联了背景/阴影 | **M** |

---

## 8 交互态

八态。每个可交互元素必须**同时**定义 hover / active / focus-visible / selected（如适用）/ disabled（如适用）/ loading（如适用），
以及每一态在**两种模式**下的取值。

### 8.1 取值表（唯一合法来源）

| 态 | 面（panel/行/卡） | 文字 | 边界 | 计时 |
|---|---|---|---|---|
| **rest** | 透明或 `surface` | `text` / `textSecondary` | 无 | — |
| **hover** | `--surface-hover`（暗 `#262a31` / 亮 `#f0f1f4`） | 副文字由 `textTertiary` 升 `textSecondary` | 无 | 100–120ms |
| **active（按下）** | `--surface-selected`（暗 `#333844` / 亮 `#e4e7ec`） | `text` | 无 | 立即（0ms） |
| **focus-visible** | 不变 | 不变 | `outline: 2px solid color-mix(signal 65%, transparent)`，`offset: 2px`；**对比度 ≥3:1 对相邻背景** | 立即 |
| **selected** | `--surface-selected` | `text`；导航项另加 `navSelectedBg` 洗色 + `navSelectedColor` + 2px 左脊 | 无 | 120ms |
| **disabled** | `opacity: 0.5`（`.agent-card.disabled`）/ `0.6`（`.memory-card.superseded`） | 继承 | 无 | — |
| **loading** | antd `Spin` / 骨架 | `textTertiary` | — | — |
| **empty** | 无盒：`.empty-state` = 图形 + 一句邀请 + 一个动作 | `.empty-title` 16/24 600 + `.empty-hint` 13/18 | — | 视图入场 260ms |
| **error** | `Alert`（antd）或页级离线横幅 | 错误文案 + **重试按钮** | — | — |

### 8.2 实测覆盖情况（本轮）

**hover**：`.readout-btn`(120ms)、`.row-btn`(100ms)、`.kanban-card`(120ms)、`.recall-stub-head`(100ms)、`.neighbor`(100ms)、`.guide-card`(120ms)、`.mem-preview`、`.kbd-hint`、`.cmdk-item`、`.tool-head`、`.suggest-chip`、`.ws-new-btn`、`.chat-group-more`。
**缺失**：**没有任何 `:active` 规则**（除 antd 自带）——按下态目前完全由 antd 承担，手写组件（`.row-btn` / `.kanban-card` / `.guide-card` / `.suggest-chip`）**没有按下反馈**。

**focus-visible**：手写元素由 `:is(.row-btn, .kanban-card, .guide-card, .suggest-chip, .brand, .kbd-hint, .chat-group-more, .recall-stub-head, .tool-head, .mem-preview, .neighbor, .ws-new-btn):focus-visible` 统一（`index.css` 1425–1432）。

**实测焦点环对比度（Tab 走查，`#board`）**：

| 目标 | 暗色环 | 对底 | 亮色环 | 对底 |
|---|---|---|---|---|
| 手写 `:focus-visible`（`.brand` / `.kbd-hint` / `.kanban-card`） | `color(srgb .941 .663 .231/.65)` → 合成 `rgb(165,120,51)` | **4.26:1** ✔ | `color(srgb .878 .627 .165/.65)` → 合成 `rgb(235,193,117)` | **1.69:1** ✘ |
| antd 组件（`.ant-btn` / `.ant-segmented` / 菜单） | `rgb(86,65,32)` | **1.73:1** ✘ | `rgb(255,234,171)` | **1.19:1** ✘ |

→ **antd 侧的焦点环在两种模式下都不达标；手写环只在暗色达标。** 这是建议变更 A1 的核心证据。

**无焦点环的 Tab 停留点（实测）**：`#chat` **5/21**（`textarea`、`.ant-input-sm` 搜索框、3 个 `.ant-select-input`）、`#settings` **2/20**（2 个 `.ant-select-input`）、`#sessions` **0/22** ✔、`#board` **0/22** ✔。
→ 与 `docs/ui-ux-review.md` 记录的 "5/21" 完全一致（那 5 个仍然存在，裸 `outline: none` 在 `index.css:1267`（composer textarea）与 `:1377`（cmdk input））。

**empty / error 的区分**：本轮实测 13 路由中 `.ant-empty|.empty-state` 命中 `#inbox` 1 处、`.ant-alert-error` 命中 0 处。
`docs/ui-ux-review.md` 的 P0-2（错误渲染成空态、`#task` 永久 spinner）是**源码级**结论（`TaskDetail.tsx` 吞异常），本规格把它转成硬契约（第 12 节第 19–20 行）。

### 8.3 建议变更

| ID | 现状 | 建议 | 理由 | 成本 |
|---|---|---|---|---|
| **A1** | antd 焦点环 1.73:1(暗)/1.19:1(亮)；手写环亮色 1.69:1 | 统一到**一条 ring 定义**：`outline: 2px solid var(--focus-ring); outline-offset: 2px`，其中 `--focus-ring` = 暗 `#f5b457`（对 panel 7.92:1）、亮 `#8a5600`（对白 6.16:1）；并 pin antd 的 `colorPrimaryBorder` / outline 到同值 | 焦点环是可访问性硬要求（≥3:1）；当前 4 个组合里 3 个不达标 | **M**（`index.css` + antd token；须逐路由 Tab 走查回归） |
| **A2** | 手写组件无 `:active` | 补 `:active { background: var(--surface-selected) }`（120ms 之内、不位移） | 八态缺一态；且按下无反馈在触摸设备上等于"没点着" | **S** |
| **A3** | `.chat` 的 textarea 与 3 个 Select 无焦点环（5/21） | 删掉 `index.css:1267` 的裸 `outline: none`，让它们落入 antd/统一 ring | 与 `docs/ui-ux-review.md` P1-10 同源；两轮未修 | **S** |
| **A4** | disabled 只有 2 处（`opacity .5/.6`），且没有 disabled 的**可达性**语义 | 定义 `disabled` 档：`opacity: .5` + `aria-disabled` + 去 hover；只允许这两个值 | 现有两个魔数，不是阶梯 | **S** |

---

## 9 动效

### 9.1 四个时长 token（唯一集合）

| token | 值 | 用途 | 实测现存 |
|---|---|---|---|
| `dur-1` | **100ms** | 行/工具头 hover（纯 background） | `.row-btn`、`.recall-stub-head`、`.neighbor`（**2026-09-23 t224 逐条核对 `index.css` 后订正：删 `.ws-new-btn`** —— 它实测 **120ms**（`index.css:2191`）⇒ 归 `dur-2`）|
| `dur-2` | **120ms** | 卡片 hover、chip、composer 边框、~~行内动作淡入~~（2026-09-23 废止：行级动作常显，无淡入 —— 见 §12.13） | `.readout-btn`、`.kanban-card`、`.guide-card`、`.suggest-chip`、`.icon-btn`、`.composer`、`.ws-new-btn`、`details.chat-thought > summary::after`（**2026-09-23 t224 逐条核对 `index.css` 后订正**：**删 `.mem-preview`** —— 它只有 `:hover { background }`、**没有 transition 声明**（`index.css:1290`）✗；**删 `.row-action`** —— §12.13 已删该规则、全仓 0 使用者；**补 4 个实测存在的载体**：`.icon-btn` / `.composer` / `.ws-new-btn` / `details.chat-thought > summary::after`）|
| `dur-3` | **180ms** | 结构展开/收起（`grid-template-rows` 0fr→1fr） | `.recall-expand` |
| `dur-4` | **260ms** | 视图入场（一个视图一次） | `.content > *` `view-in` |

**越界（2026-09-23 t224 逐条核对 `index.css` 后订正）**：① **`.chat-side` 抽屉 `transform 200ms`**（`index.css:1435`）—— **仍在，未裁决**（→ 归 `dur-3` 或新增 200 档；建议归 `dur-3` 并把 `.recall-expand` 也改成 200，以免两个半途值）② **「`.chat-side` 收起 `150ms`」已消失** ✓（t64 已归 `dur-2` ⇒ **从「越界」移入「已修」**）③ **`.cmdk` 的 `cmdk-in 160ms`**（`index.css:1755`，**动画**）不在四档上 ⇒ **新增 1 处越界**。⇒ **当前越界 = 2 处**（1 transition + 1 animation；判据见 **§12 行 68**）✓。**另登记（不是越界）**：`typing-bounce 1.2s` / `pulse 1.6s` / `loadingCircle 1s` 都是 `animation-iteration-count: infinite` 的**不定长循环** ⇒ 行 68 按**结构**豁免 ✓。**⚠️ 本表的「实测现存」列是「具名载体」不是「穷尽清单」**（2026-09-23 t224）：同档还有未列出的载体 ⇒ **判定以行 68 的逐条声明读数为准，不以本列的完整性为准** ✓。

### 9.2 缓动

| 名 | 值 | 用途 |
|---|---|---|
| `ease-out` | `cubic-bezier(0.16, 1, 0.3, 1)` | 所有入场与展开（视图入场、抽屉、`recall-expand`） |
| `linear`（默认） | 无 | 纯颜色/背景过渡（100–120ms 的 hover） |

**禁止**：`ease-in-out` 与自定义贝塞尔（当前无越界 ✔）。

### 9.3 动效铁律

1. **hover 只改材质**（background / border-color），**不做位移**（现行注释已声明："Hover is a material change, not a jump"）。禁止 `translate` / `scale` / `margin` 变化。
2. **每个视图只入场一次**（`.content > *` 的 `view-in` + 40/80/110ms 阶梯延迟）。禁止在列表项上叠加逐项入场。
3. **不阻塞**：`dur-4` 的入场动画不得让首屏文本超过 300ms 才可读。

### 9.4 reduced-motion 契约

`index.css:137` 的 `@media (prefers-reduced-motion: reduce)` 把 `animation-duration` 与 `transition-duration` 压到 `0.01ms !important`（含 `*::before/::after`）。

**实测**：存在 ✔。**要求**：新增任何动画不得使用 `animation-delay` 之外的绕过方式（例如 JS 驱动的 rAF 动画绕过 CSS 媒体查询）——图谱的 rAF 物理循环**不在本契约覆盖内**，逐视图规格中必须单独给出"reduced-motion 时图谱静态布局"的开关。

---

## 10 响应式断点行为

### 10.1 建议的断点阶梯（4 个）

| 名 | 值 | 语义 |
|---|---|---|
| `bp-phone` | **520** | 手机；次要元数据让步（`.row` 里计数/时间戳隐藏） |
| `bp-tablet` | **768** | 平板；看板 2 列、图谱检查器堆叠 |
| `bp-narrow` | **1024** | 窄桌面；抽屉式侧栏、`.dash-grid` 单列 |
| `bp-wide` | **1240** | 宽桌面；看板 4 列、`.readout-strip` 一行放满 |

### 10.2 实测现状（与建议不一致，登记为建议变更）

| 断点值 | 出现位置 | 判定 |
|---|---|---|
| 520 | `index.css:448` | ✔ 对应 `bp-phone` |
| 700 | `index.css:245, 518, 1034, 1041`（4 处） | 魔数 → 归 `768` |
| 940 | `index.css:1038` | 魔数 → 归 `1024` |
| 992 | `App.tsx:98,109`（matchMedia 侧栏折叠） | **e2e 契约**（侧栏在 992 折叠）→ 保留，并在 `bp-narrow` 中把它写进规格 |
| 1100 | `index.css:812, 933`（2 处） | 魔数 → 归 `1024`（或保留为图谱专用，见下） |
| 1240 | `index.css:517`（看板 2 列） | ✔ 对应 `bp-wide` |

→ **当前共 6 个断点值（520/700/940/992/1100/1240）+ antd 自带的 576/768/992/1200**，其中只有 3 个能对上阶梯。

### 10.3 各断点的实测行为（1440 基线，`#board`）

| 视口 | 侧栏 | `.kanban` 列数 | `.content` 宽 | 横向溢出 |
|---|---|---|---|---|
| 390 | 72（收起） | 1 | 318 | 0 |
| 520 | 72 | 1 | 448 | 0 |
| 700 | 72 | 1 | 628 | 0 |
| 768 | 72 | 2 | 696 | 0 |
| 940 | 72 | 2 | 868 | 0 |
| 992 | 72 | 2 | 920 | 0 |
| 1100 | **228** | 2 | 872 | 0 |
| 1240 | 228 | 2 | 1012 | 0 |
| 1280 | 228 | **4** | 1052 | 0 |
| 1440 | 228 | 4 | 1212 | 0 |
| 1920 | 228 | 4 | **1560**（触顶） | 0 |

### 10.4 横向溢出（390 视口，13 路由实测）

`#stats` = **+99px**（`documentElement.scrollWidth - innerWidth`），其余 12 路由 = 0。
根因（`docs/ui-ux-review.md` 第三节已定位）：`.ant-table` 6 列 `table-layout: auto` 撑破容器（当时 498 vs 390，即 +108px；本轮 +99px，说明列数/内容略有变化，**问题仍在**）。

**规格要求**：390 / 520 / 768 / 1024 / 1280 / 1440 / 1920 七档均不得横向溢出（`scrollWidth - innerWidth == 0`）。
`#stats` 的表格必须 `scroll={{ x: 560 }}` 或窄屏换卡片列表。

### 10.5 建议变更

| ID | 现状 | 建议 | 理由 | 成本 |
|---|---|---|---|---|
| **B1** | 6 个 CSS/media 断点 + antd 4 个 | 收敛到 520 / 768 / 1024 / 1240 四个；992 作为侧栏折叠的**契约值**单独登记 | 断点越散，"在某宽度下会发生什么"越无法预测 | **M**（5 处 media query 改写，须逐档截图回归） |
| **B2** | `#stats` 390 视口溢出 +99px | `<Table scroll={{ x: 560 }} />`；1.0k 以下换卡片列表 | e2e `responsive.spec.ts` 会检查溢出；这是唯一残留的溢出 | **S** |
| **B3** | `.graph-canvas` `height: 520px`、`.graph-detail` 400px 固定 | 改 `clamp()`（见 R4） | 矮屏（如 1280×720）下图谱占满视口 | **M** |

---

## 11 无障碍下限

以下为**下限**（不是目标）。每一条都在第 12 节有对应判定行。

### 11.1 对比度

| 对象 | 下限 | 实测现状 |
|---|---|---|
| 正文文本（<18.66px） | **≥4.5:1** | 暗色多数组达标；亮色 `textTertiary` 4.45/4.00 ✘、亮色 `.tag.ok` 3.34 ✘ |
| 大号文本（≥18.66px 或 ≥14px bold） | ≥3:1 | 达标 |
| 非文本必需图形（控件边界、状态点、焦点环、图表线） | **≥3:1** | 焦点环 1.19–1.73 ✘；控件边界 1.41–1.49 ✘；图谱边（亮）2.17 ✘ |
| 装饰性分隔线 | 无要求 | 1.22–1.49（合规） |

### 11.2 焦点

- **每个 Tab 停留点必须有可见环**，且环对相邻背景 ≥3:1。
  实测：`#chat` 5/21 无环 ✘；`#settings` 2/20 ✘；`#sessions` `#board` 0 ✔。
- **焦点环不得被 `overflow: hidden` 裁掉**（`.row-btn` 在 `.card` 内，`outline-offset: 2px` 可能被裁 → 逐视图规格中需确认）。
- **浮层关闭后焦点必须还给触发元素**。实测：命令面板 Esc 后焦点在 **`BODY`** ✘。

### 11.3 命中目标

- 下限 **24×24 CSS px**；目标 **32×32**。
- 实测 <32px 的交互元素：`#sessions` 4、`#task` 198、`#agents` 25、`#runtimes` 16、`#memory` 15、`#knowledge` 12、`#chat` 11、`#settings` 7、`#home` 5、`#board` 4、`#stats` 4、`#inbox` 4、`#graph` 4。
- 实测**无名称**交互元素：`#task` 5、`#settings` 4、`#chat` 2、`#memory` 1、`#knowledge` 1、`#graph` 1。
  （`docs/ui-ux-review.md` 记录的 `#sessions` 200 个无名称 24×24 图标按钮，在本轮实测为 **0 个 unlabeled / 4 个小目标** → 该 P1 项**已修复**，`.row-action` 改为 hover 显现且有 `aria-label`。）

### 11.4 结构、语义、键盘

- **文档大纲**：每路由**恰好 1 个 `h1`**。
  实测：`#home` 1 ✔；**其余 12 个路由 0 个 `h1`** ✘，且 `#chat`/`#task`/`#agents`/`#settings` 出现 `h3` 而无 `h1`（大纲断裂）。
  **受限的解决方案**：`.view-bar h2` 是 e2e 冻结选择器，**不得改成 `h1`**；改为在每个视图内加一个视觉隐藏的 `<h1 class="sr-only">`（`.view-bar h2` 保留并 `aria-hidden` 或改为普通 div 语义不变）。这需要 `.sr-only` 工具类（当前不存在）。
- **命令面板 ARIA（实测，`Ctrl+K`）**：`role=dialog` ✔、`aria-label` ✔；**缺** `aria-modal`、`aria-activedescendant`、`aria-controls`、列表 `role=listbox`、选项 `role=option`（34 个选项全部无 role）、`aria-selected`（0/34）。
  → 读屏用户无法知道回车会执行什么。
- **`aria-expanded`**：全站 **0 个**（实测 `document.querySelectorAll('[aria-expanded]').length === 0`）——所有展开/收起控件（`.recall-stub-head`、`.tool-head`、`.cmdk` 相关）都缺。 2026-09-21 更正：该断言只覆盖**手写**展开控件。排除 antd 后实测全站 `[aria-expanded]` = `#chat` 3 / `#task` 3 / `#memory` 1 / `#settings` 1（全部来自 antd `Select`），手写控件 0 个。见第 12 节行 23 与 §12.2。
- **语言**：`html lang` 与界面语言同步 ✔（`docs/ui-ux-review.md` 已确认）。
- **键盘**：`Escape` 关闭浮层 ✔；命令面板内部 Tab 不逃逸（因列表长，非真陷阱）。
- **`reduced-motion`**：CSS 层已处理 ✔（见 9.4）。
- **`theme-color` meta**：`index.html:6` 是静态 `#0a0a0c`（与当前 `D.appShell #08090b` **不一致**），且不随模式切换——切浅色后浏览器 chrome 仍是深色 ✘。（`docs/ui-ux-review.md` 记录的是更旧的 `#0a0c0e`；现值为实测 `#0a0a0c`。）

### 11.5 建议变更

| ID | 现状 | 建议 | 理由 | 成本 |
|---|---|---|---|---|
| **Y1** | 12 路由无 `h1`；`.view-bar h2` 被 e2e 冻结 | 新增 `.sr-only`（`position:absolute; width:1px; height:1px; clip-path:inset(50%)`）+ 每视图一个视觉隐藏 `h1`；`.view-bar h2` **原样保留** | 大纲与 e2e 契约不冲突的唯一解法 | **M**（13 个视图各 1 行 + 1 条 CSS） |
| **Y2** | 命令面板 6 项 ARIA 缺口 | 补齐 `aria-modal="true"`、输入框 `role="combobox"` + `aria-expanded` + `aria-controls` + `aria-activedescendant`、列表 `role="listbox"`、选项 `role="option"` + `aria-selected`；关闭还焦 | 6 项全部可机器判定（第 12 节第 21–22 行） | **M**（`CommandPalette` 一处，但涉及键盘索引状态） |
| **Y3** | `[aria-expanded]` 全站 0 | `.recall-stub-head` / `.tool-head` / `.chat-group-more` 等展开控件补 `aria-expanded` | 展开态是状态，必须可被读屏读到 | **S** |
| **Y4** | `theme-color` 与 `D.appShell` 不一致（实测 `index.html:6` = `#0a0a0c` vs `#08090b`），且不随模式切换 | 由 `ThemeProvider` 在 `useEffect` 里写 `meta[name=theme-color]` | 一个静态 meta 两个问题：错值 + 不跟随 | **S** |

---

## 12 验收阈值表

**格式 = `指标 | 基线数字 | 目标数字 | 判定方式`。**
- 基线来源标记：`DL` = `docs/design-language.md`；`UX` = `docs/ui-ux-review.md`；`M1` = **本轮复测**（2026-09-21，同一 daemon，1440×900 暗色，口径见附录 A）。
- 所有行**不含主观判断**：给定 `panel/dist` 与活 daemon，脚本可给出通过/不通过。


**§12 表的列语义（2026-09-23 明确约定；本条修的是「对象集写在哪一列」这个缺口）**

| 列 | 语义 | 放什么 / 不放什么 |
|---|---|---|
| **#** | 行号 | 与工具 CHECKS 的行号**逐位一致** ✓ |
| **指标** | 判据的名字 / 主题 | 一句话说清「判什么」✓ |
| **基线数字** | **现状读数** | 含「**诚实的红**」记录 ✓（如行 50 / 54 / 55 / 56 / 57 / 58 ✓） |
| **目标数字（阈值列）** | **判据的数字 / 字符串阈值** | **只放阈值** ✓ —— **不放对象集、不放测法** ✗ |
| **判定方式** | **可判定的测法 + 对象集定义 + 依据标注 + 出处** | **对象集的唯一来源** ✓ —— **工具应从本列读对象集与测法** ✓ |

**⚠️ 本表没有独立的「依据列」** ✓ —— **依据标注（裁决 / 测量）与出处合并写在「判定方式」列** ✓（**避免下一个人去找一个不存在的列** ✗）
**⚠️ 因此的硬约定**：**对象集与测法的来源 = 「判定方式」列** ✓ —— **不得把对象集写进「目标数字」列** ✗（**那会让同一事实两处出现 ⇒ 必然漂移** ✓ —— 本代已吃过：`PENDING_ENTRY` 与自检期望的两份拷贝 ✓）

| # | 指标 | 基线数字 | 目标数字 | 判定方式 |
|---|---|---|---|---|
| 1 | 暗色单亮度带像素占比（`0.05 ≤ luma < 0.10`） | `DL` 92.9% → 14–75%（中位 ~50%）；**`M1` 13.7–83.5%（中位 37.4%）** | 每路由 **≤75%**（**口径＝「被设计的面」**，排除应用画布，见 §12.5） | 截图像素直方图，`luma = (0.2126R+0.7152G+0.0722B)/255` 四舍五入到 0.01，统计落入 `[0.05,0.10)` 的比例。**2026-09-21 口径收窄（§12.5）**：分子与分母**都排除应用画布像素**（画布是单一扁平 token `#0f1014`，luma 0.063 本身落在带内），工具**同时报告「设计面」与「含画布」两个数字**，原值不隐藏、不删除。**画布不是调色板能吸收的问题**：**升画布**（脱离带上界）会让行 29 的 canvas→panel 比从 **2.47 掉到 1.18**；**降画布**（脱离带下界）会让 shell→canvas 只剩 **1.24** —— 两个方向都撞墙，复算与反证见 §12.5 |
| 2 | 有真实色度像素占比 | `DL` 0.45–2.9%；**`M1` 1.16–5.02%** | **1.0% ≤ x ≤ 6.0%**（过低=灰死，过高=花） | 同上像素集；`S = (max-min)/max > 0.15` 且 `max-min > 12` |
| 3 | 信号色像素占比 | `M1` 0.13–0.94% | **≤5.0%/路由**（内部目标 ≤1.5%） | 像素 `20°≤H≤50°` 且 `max-min>25` 且 `S>0.30` 且 `max>110` |
| 4 | 严格口径"描边内容容器"数 | `M1` 12/13 路由 = 0；`#chat` = 1（`.composer`） | **≤1/路由，且那 1 个必须是控件单元** | `DIV/SECTION/ARTICLE/LI/ASIDE/MAIN/UL` ∩ 可见 ∩ ≥2 边 `border-width>0` ∩ 边框色 alpha>0.04 ∩ class 不匹配 `ant-(input\|select\|btn\|tag\|table\|checkbox\|radio\|switch\|segmented\|pagination\|progress\|slider\|picker\|upload)` |
| 5 | 全部可见描边元素数 | `M1` 4 / 11 / 4 / 8 / 1679 / 5 / 14 / 5 / 32 / 19 / 70 / 7 / 4 | **≤40/路由**（排除 `.md` 表格单元与注册控件） | 同上不加 class 过滤；`#task` 需先排除 `.md th/td` |
| 6 | 每页 ≥18px 可见文本节点数 | `DL` 1–23；`M1` 1–23（下界 1 出现在 `#task` / `#settings` / `#inbox`，空态 `#chat` 2） | **含读数的页 ≥3；无读数的页 ≥1；所有页 ≤40** | 计算样式 `fontSize ≥ 18px` ∩ 有直接文本 ∩ 可见。**条件判定**：默认门槛 **≥3**；路由 ∈ §12.1 豁免清单（无读数的页）→ 门槛 **≥1**，且仍必须恰好 1 个 20px 的页面标题（`.view-bar h2`；`#home` 的层级中心是 `.home-hero h1`，**尺寸按 §4.1 的 display 档 24/30**——2026-09-21 前的行文误写为 32px，见 §12.4） |
| 7 | 每路由最大字号 | `DL` 32px（9 页）；`M1` 32px（12 路由），`#chat`/`#settings` 20px | **≥20px；含仪表盘的页 ≥32px** | `max(fontSize)` over 可见文本节点 |
| 8 | 字号越界数（不在 11/12/13/14/15/16/18/20/24/32） | `UX` 存在 10 / 12.5 / 17px；**`M1` 0** | **0** | 计算样式 `fontSize` 取集，必须 ⊆ 阶梯 |
| 9 | 字重越界数（不在 400/500/600/700） | `M1` 450×1、650×2 | **0** | 计算样式 `fontWeight` |
| 10 | 圆角越界元素数（不在 0/4/6/10/14/50%/999px） | `M1` 5px×13、8px≈240、12px×2、100px×14 | **0** | 计算样式 `borderRadius` |
| 11 | 带内联 `font-size` 的元素数 | `M1` `#sessions` 314、`#task` 131、`#stats` 80、`#runtimes` 3 | **0** | DOM 元素 `style` 属性包含 `font-size` |
| 12 | 带内联 `style` 的元素数 | `UX` `#sessions` 732；**`M1` 731** | **≤50/路由** | DOM 元素 `style` 属性非空 |
| 13 | 暗色文本对比度失败数 | `UX` `#task` 18016/19327、`#sessions` 205/949 | **0**（**当前观测：工具 `02:26:57`（sha `fdb93db1…`）在 build `DKadudCh` 上报 `knowledge/dark = 1`（WCAG 口径 1）⇒ 该行现为 FAIL，待定位那 1 个文本节点；工具 `02:18:02` 与 round-2（工具 `00:33:38`）均报 0，**同样是工具版本差异**，需以最新工具复测为准） | 计算 `color` 与最近不透明祖先背景的 WCAG 比 `< 4.5`（`fontSize < 18.66`） |
| 14 | 亮色文本对比度失败数 | `M1` `.muted` 4.45、`.tag.ok` 3.34 | **0** | 同上，`?mode=light` 复跑 |
| 15 | 焦点环对比度 | `M1` 手写环 暗 4.26 ✔ / 亮 1.69 ✘；antd 环 暗 1.73 ✘ / 亮 1.19 ✘ | **两模式均 ≥3.0:1** | Tab 走查取 `outlineColor`/`boxShadow`，与相邻不透明背景按 alpha 合成后计算 |
| 16 | 无焦点环的 Tab 停留点 | `UX` `#chat` 5/21；**`M1` `#chat` 5/21、`#settings` 2/20** | **0** | **判据以 primitives.md §11 行 16 为准**：逐次 Tab 记录 `outlineStyle` / `outlineWidth` / `boxShadow`，**只有 `outlineStyle === 'solid' && outlineWidth ≥ 2px` 才算有环**；`none` 与 `auto`（UA 默认）一律判**无环**。例外仅 `.composer`（环由容器提供）。见 §12.2 |
| 17 | 无名称的可交互元素数 | `UX` `#sessions` 200；**`M1` `#settings` 4、`#task` 5、`#chat` 2** | **0** | 可见交互元素且 `aria-label`、文本内容、`title` 全为空 |
| 18 | <32px 命中目标数（<24px 为硬失败） | `M1` `#task` 198、`#agents` 25、`#runtimes` 16、`#memory` 15、`#knowledge` 12、`#chat` 11、`#settings` 7、`#home` 5、`#sessions` 4 | **<24px 必须为 0（外壳与内容区同时适用）；<32px 每路由「内容区」≤10**；**外壳常数不计入每路由预算，改用具名清单只查一次**（见 §12.9）（**无例外名单**：已裁决 `.ant-switch` 的 21px 走「改」而不是豁免，见 §12.7.2） | 可见交互元素的**控件根** `getBoundingClientRect()`（t27：按**交互语义**取控件根 —— antd 控件根类名 / 原生控件 / `role` / `tabindex ≥ 0` / `cursor: pointer`；antd 嵌套时**取最外层**控件根，否则会停在 21px 的子部件上）。**两套口径并列输出**：display 给「命中区口径」与「对照（绘制口径）」，note 里每个地板越界项附绘制口径原值；**判定一律用命中区口径**。**2026-09-21 外壳裁决（§12.9）**：外壳 = `aside.ant-layout-sider.app-sider` 子树（含 `.sidebar-foot`）+ `body` 下的全局 portal 宿主；内容区 = `main.ant-layout-content.content` 子树（含 `.view-bar`，它逐路由不同）。外壳 <32px 是**具名闭集（2026-09-21 t36 后为 1 项：`kbd-hint` 26×45；t36 前那 3 个 `ant-btn` 32/32/36 × 24 已被 `controlHeightSM: 32` 抬到 32 退出）**，**只查一次**且必须逐路由不变；**⊆ 语义：变少满足、变多 FAIL**。**`.ant-switch` 登记**：`#settings` 的 `button.ant-switch` 实测 **42×21**（手柄 17×17），是 antd 6.6.3 由 `fontSize × lineHeight` **派生**的值（本应用 14×1.5=21）。**2026-09-21 裁决：改** —— pin `Switch` 的 `trackHeight / handleSize / trackMinWidth` = **24 / 20 / 48**；实测布局零影响（行内最高元素是 42px 文本块，行高 54px 不变）；t27 复测已落地为 **48×24**，地板不再触发。WCAG 2.5.8 的间距例外**已满足**（24px 圆不与任何其它目标相交，最近目标 49px）—— 但本行的 24px 地板刻意严于 WCAG，故不据此豁免。见 §12.7.2 **测法（2026-09-23 裁）**：命中区按**有效命中盒（含参与命中测试的伪元素）**测 —— 见 `views/README.md` **S2.1**；**反向证据：去掉伪元素 ⇒ 本行必须 FAIL** ✓ |
| 19 | 横向溢出（390/520/768/1024/1280/1440/1920） | `UX` `#stats` @390 = +108px；**`M1` `#stats` @390 = +99px**，其余 0 | **所有路由 × 所有视口 = 0** | `documentElement.scrollWidth - window.innerWidth` |
| 20 | error 与 empty 可区分 | `UX` `#task` 屏蔽 API 后 7s 仍永久 spinner（源码 `TaskDetail.tsx:59` 吞异常） | **每路由在 API 失败时呈现 `role=alert` + 重试按钮** | **已实现（t59；工具 mtime 2026-09-23 02:18:02）**：只打**该路由自己的数据端点**（正常加载时观测到的 `/api` 端点，**排除外壳 `/permissions`** —— 打外壳测到的是白屏，且外壳自己的 alert 与本行的 alert 无法区分），三条分开判：① **失败态可见**（`role=alert` / `role=status` / `.ant-alert-error` 且可见）② **可恢复**（重试控件存在、命中区 ≥24×24、**真实点击**后错误消失）③ **不得渲染成空态或成功态**（t13 的 inbox 形态：运行正被阻塞而页面说没事）；另要求**外壳仍在**（侧栏 / main / body 有文本）。**状态随工具版本翻转，引用时必须带版本**：工具 `02:18:02`（sha `a6281fca…`，11.5s 窗口，build `DKadudCh`）→ **FAIL**：13 路由 × 2 模式全部「失败态可见 ✓ · **重试可点 ✗ · 恢复 ✗**」；工具 `02:26:57`（sha `fdb93db1…`，默认窗口，同 build）→ **PASS**（无失败明细）。⇒ 同一份源码两种结论，**差别在注入/点击的实现**（不是产品改了）；修复者与引用者都必须重新测量一次再下结论（原「CDP 屏蔽 + 看 `.ant-empty` 不出现」是工具未实现时的占位描述，已由本条取代） |
| 21 | 命令面板 ARIA 缺口数 | `UX` 3 项；**`M1` 6 项**（`aria-modal` / `aria-activedescendant` / `aria-controls` / `listbox role` / `option role` / `aria-selected`） | **0** | 按 `Ctrl+K` 后读取上述 6 个属性 |
| 22 | 浮层关闭后焦点归属 | `M1` = `BODY` | **= 触发元素** | `Escape` 后 `document.activeElement` 是否为打开前的元素 |
| 23 | 手写展开控件的 `aria-expanded` 覆盖率 | `M1` **旧测法「全站 0」不成立**（它只覆盖手写控件，未排除 antd）；实测全站 `[aria-expanded]` = `#chat` 3 / `#task` 3 / `#memory` 1 / `#settings` 1，**全部来自 antd `Select`**；手写展开控件实测 0 个 | **覆盖率 = 100%**（`.recall-stub-head` / `.tool-head` / `.chat-group-more` 每个都要有 `aria-expanded ∈ {"true","false"}`；原生 `<summary>` 自动 pass） | **不得**用全站 `document.querySelectorAll('[aria-expanded]').length` 计数（antd `Select` 会造成**虚假通过**，见 §12.2）；只数上述三个选择器（**判据定义见 primitives.md §11 行 23**） |
| 24 | 每路由 `h1` 数 | `M1` `#home` 1；其余 12 路由 **0** | **恰好 1** | `document.querySelectorAll('h1').length`（允许 `.sr-only`） |
| 25 | `#task` DOM 节点数 | `UX` 153,122；**`M1` 153,074** | **≤3,000** | `document.querySelectorAll('*').length` |
| 26 | `#task` 最长单次阻塞 | `UX` 2,999ms | **≤200ms** | `PerformanceObserver('longtask')` |
| 27 | 首屏入口 JS 体积（**地板 + 预算**两段） | `UX` 1367KB（gzip 428KB）；**`M2` 1406KB / gzip 436KB**（构建 `D6UEuuvX`；t8 在上一构建测得 1405KB / 438KB，跨构建漂移 ±2KB） | **地板（登记，不判 PASS/FAIL）= 框架层 980KB（gzip ≈305KB）**；**预算（判定）= 应用层首屏 ≤150KB（gzip ≤48KB）**；**两条结构断言**：① markdown 栈不得在入口（必须是独立 chunk）② 必须存在路由级分割 chunk。**原「≤350KB（gzip ≤120KB）」保留为历史记录，已由 §12.10.2 裁决修订** | `panel/dist/assets/index-*.js` 字节数，**并按 chunk 归类**（需 `vite.config.ts` 的 `manualChunks`：vendor / app / markdown 三块）。地板值随技术栈变更**重测，不写死**。**归类未落地前行 27 判 `not_measured`**，并打印「地板 / 应用层 / 总量」三个数。**当前构建 `C048RA32` 实测**：应用层首屏 **94KB / gzip 31KB ✔**、首屏静态 **1114KB**、地板（实测 vendor chunk）**1020KB**、dist JS chunk **16 个**、结构断言 ①/② 均 ✔（详见 §12.10.2 ③ 与 `docs/screenshots/audit-run/round2-verification.md`） |
| 28 | 11.5s 窗口内 API 请求数（**逐端点 + 外壳轮询分离**） | `UX` `#inbox` 12（`permissions`×12）；**实测：1 个 2s 轮询 = 7 次（节奏 1993/2007/2002/1995/2012/1999 ms），2 个轮询器 = 14 次**；**`M2` 11.5s 实测（构建 `D6UEuuvX`）：逐路由非 permissions 端点最大组 = home 2 / task 6 / graph 1 / runtimes 1 / settings 1 / inbox 0** | **门槛 ≤7 不变**（原 ≤3 与 I5 自开的药方不相容，见 §12.8）。**判定范围改为（§12.10.1）**：① **逐路由**：只计该路由自身请求，把路径中的数字 id 归一化为 `:id` 后**按端点分组**，取**最大组 ≤7**；② **`/api/v1/permissions` 不进逐路由计数**，改为**只查一次的独立断言 ≤7**（全站 13 路由 × 两模式取最大值） | CDP `Network.requestWillBeSent` 计数；窗口 <11.5s 不判定（informational）。**算式**：`11.5s ÷ 2s ≈ 6 次间隔 + 首次请求 = 7`；**2s 间隔不动**。**区分度保留**：1 个轮询器 = 7 ✔ / 2 个 = 14 ✘。**为什么逐端点**：载入突发里每个端点只被取 1–2 次（远低于 7），因此突发**自然不计入**，无需后移窗口起点；而「同一端点被两个轮询器各取一遍」会立刻翻倍。**残余盲区（登记）**：两个**较慢**的同端点轮询器（如 2×5s = 6 次）仍在门槛内；堵它需要「每端点 ≤ ceil(窗口 / 该端点最小观测间隔) + 1」的间隔感知判据，本轮不做 |
| 29 | 暗色材质台阶（canvas→panel 原始相对亮度比） | `DL` ~2.5x；`M1` **2.48x** | **≥2.4x** | `L = 0.2126R'+0.7152G'+0.0722B'`（sRGB 反伽马），比值 |
| 30 | 亮色 panel 的 ring | `M1` `0 0 0 1px #e7e9ed` + `0 1px 2px`（**对 pageCanvas 1.1340:1** / 对 panel 白 1.2155:1 —— 旧记的 1.22 是**白底**值，基准已统一到画布，见 §12.7.1） | **ring 存在且对 `pageCanvas` ≥1.2:1**（目标不动；**令牌已改（t30 已落地，2026-09-21）**：亮色 `borderSecondary` `#e7e9ed` → **`#dee0e4`** ⇒ 对 canvas **1.2330**） | 计算 `box-shadow` 解析为 ring 颜色后与 `pageCanvas` 比。**为什么基准是画布**：ring 是 `0 0 0 1px` 的**向外** shadow，画在 panel 盒子**之外**，外缘贴的就是画布（它的职责就是把 panel 的边从画布上分出来，primitives §2.1 C1）。**为什么降目标不行**：1.2 是系统发丝线族的下限（§11.1「装饰性分隔线 1.22–1.49」、§2.4 `--rule` 亮 1.41 / `--rule-soft` 亮 1.22），降到 1.13 会让 panel 的边成为**全系统最弱的一条发丝线**。算式与影响面见 §12.7.1 |
| 31 | 图谱边对比度（两模式） | `DL` 暗 3.09（改造后）；`M1` 暗 **3.08** ✔ / 亮 **2.17** ✘ | **≥3.0 两模式** | `--graph-edge` 按 alpha 与 `--graph-canvas` 背景合成后计算 |
| 32 | 信号令牌唯一性 | `M1` 两件事混在一条：① `--status-warn === --signal`（暗 `#f0a93b`、亮 `#8a5600`，**已由 C2 消除**）；② accent `--brand` / `--ant-color-primary` `=== --signal`（**刻意设计，保留**） | **0 组重复**：检查集 = **`{--signal, --status-warn, --brand-claude, --brand-deepseek, --brand-opencode}`（5 个）**，两模式各自两两互不相同；且 `--status-warn` 与 `--signal` 每模式 **Δhue ≥ 8°** | 解析 CSS 变量表，字符串比较 + 色相差。**检查集只读运行时品牌族**（§3.1 I 域②）；accent（`--brand` / `--ant-color-primary`）`=== --signal` 属刻意设计，**显式排除**——判据与理由见 §12.3 |
| 33 | 图谱分类色非文本对比度 | `M1` **暗 4.69–10.02 · 亮 4.51–8.07** —— **区间下端 = 最差**（**决定它离红多近**）：**暗 4.69 = `--graph-default` 对 panel A3 `#1b1e24`** ✓ · **亮 4.51 = `--graph-product` 对 canvas A2 `#f6f7f9`** ✓ ⇒ **余量 1.69 / 1.51（阈值 ≥3.0）** ✓；**区间上端 = 最好**：**暗 10.02 = `--graph-product` 对 canvas A2** ✓ · **亮 8.65 = `--graph-project` 对 panel A3 `#ffffff`** ✓（**2026-09-23 t186 复测订正**：原记录 **暗 4.69–7.16 · 亮 4.58–5.88** —— **两端都过期** ✗：下端 4.58→4.51 更差、上端 5.88→8.65 更好 ✓；**读数由 9 色 × 2 面 × 2 模式逐项算出，归属取自读数表** ✓） | **≥3.0 两模式** | 每个 `--graph-*` 与 canvas/panel 比；并按 §3.1 I 断言「一个 hex 只服务一个语义域」（跨域引用即判失败） **对象集与口径（2026-09-23 t184 裁决）**：**对象集 = 每个 `--graph-*`** —— **含 `--graph-default`** ✓（**它会被真实绘制**：`normalizeKind` 把 `null` / 未知 kind 映到 `default`，见 `views/view-graph.md:80` ✓）· **口径 = 对 canvas 与 panel 两个面各算一次、取最小** ✓ —— **t184 复测（可复现）**：`--graph-default` 暗 **对 canvas `#0f1014` = 5.34** · **对 panel A3 `#1b1e24` = 4.69** ⇒ **本行基线里的 4.69 就是后者** ✓ —— 即 **4.69 的来源 = 色↔背景对比度**（不是类别间可辨性 ✓）。**⚠️ 验收锚 `dark ≥5.34` 的数是对的，但口径必须写明** ✗：它只对 **canvas** 成立；本行的对象集是 **canvas 与 panel 两者取最小** ⇒ **最小是 4.69** ⇒ **两者不可混用**（否则同一条判据会有两个结论 ✓）。**⚠️ 并报告（本单不改）**：本行基线区间两端都已过期 —— 记录 **暗 4.69–7.16 / 亮 4.58–5.88**，**t184 复测为 暗 4.69–10.02 / 亮 4.51–8.07**（亮 canvas = `#f6f7f9`，`--graph-default` 对亮 canvas = **4.51** ✓）⇒ **由 captain 决定是否另单订正** ✓ **⚠️ 对象集精确化（2026-09-23 t186）**：**对象集 = 9 个类别色**（`--graph-person / org / project / repo / tool / concept / product / protocol / default` ✓）—— **不含** `--graph-edge` / `--graph-edge-hi`（**归行 31** ✓）与 `--graph-label`（**文本，不属非文本对比度** ✓）✓。**两个面的 token 与值**：**canvas = A2 `pageCanvas`**（暗 `#0f1014` / 亮 `#f6f7f9` ✓）· **panel = A3 `surface`**（暗 `#1b1e24` / 亮 `#ffffff` ✓）✓。**「区间」的读法**：**下端 = 最差（离红多近）· 上端 = 最好** ✓ —— 本行基线按此读 ✓ |
| 34 | 每路由无 `:active` 反馈的手写可点元素 | `M1` 4 类（`.row-btn` / `.kanban-card` / `.guide-card` / `.suggest-chip`） | **0** | 检查这些选择器是否存在 `:active` 规则 |
| 35 | 断点值个数 | `M1` 6（520/700/940/992/1100/1240）+ antd 4；**当前实测**：CSS **[520,768,1024,1240]（恰好 4 ✔）** · JS **[520,992]** | **① CSS 侧 ≤4 个断点**（不变）∧ **② JS 断点集 ⊆（CSS 断点集 ∪ 契约值 {992}）** —— **子集断言**：出现任何 **JS-only 断点**（既不在 CSS 侧、也不是 992）即 **FAIL** | 正则提取 `@media (max-width: …)` 与 `matchMedia`；**逐值判归属**（CSS / 契约 / JS-only），不是数个数。**历史（保留）**：原措辞「**≤4 CSS 断点 + 1 契约值(992)**」把「**防 JS/CSS 漂移**」误表达为「**JS 侧只允许一个值**」⇒ 把 **520**（M1 既有的合法 CSS 断点，`ui.tsx:346` / `Board.tsx:147` 有明文语义）在 JS 侧**误判越界**。见 **§12.16**。**判定式**：@css ⊆ {520,768,1024,1240} ∧ len(css) ≤ 4 ∧ ∀v ∈ js: v ∈ css ∪ {992}@。**must-FAIL 自检对应**：a JS-only 700 ⇒ FAIL · 5 个 CSS 断点 ⇒ FAIL。**底层实测**：@runtimeSheets@ = 30（30 张 antd 表里 **0 个断点值**）⇒ 红是**我们自己的 JS 常量**，判据陈旧（与 t78 结论一致）。**历史链**：原措辞把「防漂移」误表达为「只允许一个值」⇒ **报出一个没有任何改动能消除的红**（全量里唯一的 FAIL 长期挂着）—— 详见 §12.16。 |
| 36 | 侧栏宽度 | `M1` 228 / 72（≤992 折叠） | **228 / 72 不变** | `getBoundingClientRect().width` |
| 37 | 间距越界值个数 | `M1` 19 个非阶梯值 | **0**（**只判「设计者写的值」**，见 §12.6） | 采集可见元素的 `padding/margin/gap`；剔除 `clamp()` 产出；**只保留获胜声明来自作者来源**（构建后的 `index.css` 或元素 `style` 属性）的值，antd CSS-in-JS 注入值与 UA 默认值单独计数不计入分子；**指定值为 `auto` 的（flex 自动外边距解析成 px）跳过**；正值须 ⊆ 阶梯，**负值按出血单列**（见 §12.6 算式）。**⚠️ 本行的对象集 = 声明值（作者来源）** ⇒ **有效几何（计算值）不在本行**：实测 37 个不同的计算值里，off-ladder 的那些由三类构成 —— **clamp / vw 派生**（23.5 · 31.68 · 25.92）· **antd 注入值** · **已登记的负出血**（-12）—— 三类都**不是缺陷**且都在本行对象集之外**按设计** ⇒ **为什么不另立一行，见 §12.17 ②** ✓ |
| 38 | 严格口径描边内容容器：`#chat` 的例外 | `M1` 1 = `.composer` | **恒定 ≤1 且必须是 `.composer`** | 见第 4 行 |
| 39 | 单路由 `/api` 请求数（**首屏条目级展开**为主判据；**轮询的判定归行 28** —— 2026-09-23 措辞订正：原写「窗口内轮询腿为同源子类」，与 §12.10.3 改写后的口径矛盾，见 §12.18 的复核表） | `M2` 修前 `#graph` = **56**（1×`/graph/entities` + **55×`/graph/entity/<id>`**，每实体一个）；修后 = **1**。其余实测：home 12 / task 9 / runtimes 4 / settings 2 | **`R ≤ 2K + 2`**，其中 `K` = 归一化端点种类数（数字 id 折叠为 `:id` 后去重）、`R` = 首屏请求数（排除 `/permissions`）。**实测**：graph 1 ≤ 6 ✔（修前 56 > 6 ✘）· home 12 ≤ 16 ✔ · task 9 ≤ 12 ✔ · runtimes 4 ≤ 12 ✔ · settings 2 ≤ 8 ✔。**范围声明（2026-09-23 改写，数值不变）**：本行**不**判「是不是轮询」（那是行 28 的职责），它判「同一数据面在窗口内被取了多少次」——**首屏条目级展开**与**轮询腿**是同一现象的两个来源，故合并计数、不并入行 28（并入会把两种不同现象混进一个数） | **测量窗口 = `load` → `domStable` + settle，t47 起实现在 11.5s 窗口上跑**（与行 28 共用同一窗口）—— 所以本行同时抓到 **① 首屏条目级展开**（t44 的原始意图：每个条目一个请求，`#graph` 修前 55× `/graph/entity/<id>` 即此类）与 **② 窗口内的轮询腿**（同一端点在窗口内被重复取）。t57 的证据：`#board` 的 R=5 是 **3s 轮询**、`#agents` 的 R=9 是 **5s 轮询**，三条腿时间戳完全等距 ⇒ 轮询腿确实被计入，故名称与判据文本按实际度量范围改写，**`R ≤ 2K+2` 不变**。把端点路径的数字 id 折叠为 `:id` 后分组。**为什么不是绝对数**：绝对阈值会随页面数据面数量变化而失准；`2K+2` 直接编码「每个数据面允许首取 + 一次重取，另给 2 次余量」，**N+1 无论怎么变形都会撑爆比例**。**与行 28 的分工**：行 28 判「轮询重复」，本行判「首屏条目级展开」——**不并入行 28**（并入会把两种不同现象混进一个数，正是本代反复出现的口径病） |
| 40 | 压缩致折行（短文本被挤到换行） | `M1` 全站多行短文本载体 = **0**（构建 321,657,344 B 实测；`.sidebar-foot` 三项已不命中：`.conn.ok` 85×15 / `.build-id` 86×15 / `.kbd-hint` 65×32，均 1 行） | **每 capture ≤0** | 见 **§12.14.1**：三步判定（① 直接文本 ≤12 字符 ② 自身文本节点的行盒 ≥2 行 ③ **自然宽度 > 元素宽 + 2px**）+ 有意截断排除 |
| 41 | 兄弟元素压盖 | `M1` **1 对**（chat 暗/亮各一次）：`div.row.chat-session-row` 内 `button.row-btn.chat-session.grow` × `button.icon-btn.icon-btn-lg`，交 **2×32**（容差 1px 已给足） | **每 capture ≤0** | 见 **§12.14.2**：同容器**兄弟**矩形相交（容差 1px）+ 覆盖层排除（position / z-index / portal / SVG / 不可见 / clip） |
| 42 | `transition: all`（必须显式列出属性） | `M1` 项目侧 `transition` = **0**；全站命中 **199/199 全部来自 antd-cssinjs**（`link:index-*.css` 来源 = 0） | **每 capture ≤0**（项目侧）；**例外**：`owner === antd-cssinjs*` 排除 | 判定 = `transition-property === "all"` **∧** `transition-duration !== "0s"`（**两条件缺一不可** —— 该属性的**初始值就是 `all`**，只判属性会把整页命中）。**例外口径三点**：(a) 排除项是 **antd 的 199 处**，**不是「已合规」**；(b) 来源 `unknown` **不排除**（来源不可判时**保守判 FAIL**）；(c) 项目侧覆盖 antd 的 `transition-property` 属**独立决策，不在本判据内**。**must-FAIL 已实装并绿**（在项目自有 class 上注入 `transition: all` ⇒ 仍判 FAIL ⇒ 该例外不掩盖自有回归 ✓）（**历史**：早期一次快照记的是「81/81」，**以本轮 199 处为准**；**不带任何 `ant-*` class 的 = 0**、**带项目自有 class 的 146/199**）**为什么否决「项目侧覆盖」**：tools 实测对 36 个 `ant-*` class 注入 36 条规则**只消一半**（`#inbox` 6 → 3，残留 `ant-menu-item-group-title` / `ant-progress-track` / `ant-switch-handle`），代价是**维护 36 条针对 antd 内部类名的规则**（升级即失效）+ **改写 antd 自己的动效** ⇒ **① 优于它**。 |
| 43 | `img` 缺 `alt` | `M1` **13 路由 × 2 态实测 `<img>` 元素总数 = 0** | **每 capture ≤0** | 判定 = `img` 无 `alt`（或 `alt` 为空且非装饰用法 `alt=""`）。**⚠️ 当前是「空集通过」**：活体上**没有对象可判**（`<img>` = **0 个**）⇒ **必须记「0 个对象」，不得记「已验证合规」**（同 §12.15 的边界写法：**快照不是保证**）。must-FAIL 自测证明判据能 FAIL ✓ |
| 44 | 字面三点号（应为省略号 `…`） | `M1` 0 | **每 capture ≤0** | 判定 = **读 DOM 而非源码**：文本节点或 `placeholder` / `title` / `aria-label` 属性值里出现字面 `...`；**`…`（U+2026）不算**；**有意排除** `pre` / `code` / `kbd` / `samp` / `textarea` 子树（代码样例里的三个点是**内容**）。⇒ **天然不误判注释** —— 与 §12.15 里两处 `outline: none`「都在注释里」被排除是**同一条理由（注释进不了 DOM）**；这是「**判据的对象集与真实意图对齐**」的正面例子 |
| 45 | 数值读数带 `tabular-nums`（覆盖率） | `M1` **旧范围**覆盖率 83.3% / 85.7% / 50.0%（`grandPeer≥2`）；**新范围 ⇒ 转 PASS** | **覆盖率 100%** | **范围**：① 自身文本只由数字与分隔符构成 ② 长度 2–24 ③ 属于一个**数字组** = **一个「每个直接子元素各含一个数字读数」的容器**（**不是**「子树里有 ≥2 个数字」；**注意措辞**：写成「直接子元素**全部是数字**」会把**读数栅格本身**排除掉 —— `div.readout-row > div.readout-cell > span.readout` 的直接子元素是 `div.readout-cell`，它们自己不是数字 ⇒ 栅格会全部出局；**这条措辞偏离是第 7 条的实例，见 §12.10.5 第 7 条**）。**有意排除的 4 处（逐条）**：`#home` `div.dash-head.zone-head > span.zone-note`「199」· `#home` 同层「126」· `#board` `div.kanban-head.zone-head > span.count`「20」· `#task` `div.row-btn.selected > span.muted`「17%」—— 理由：**孤立单值、不构成列 / 对照**；`section.zone` 是**区块**（还装着标题 / 栅格 / 图例），**不是列**。**同范围自动排除** 5 个 `button.kanban-card > div.time` 日期（`2026/9/12`）。**must-FAIL 保留**：`a READOUT-GRID figure without tabular-nums FAILS`。**历史（保留）**：上一版范围 `grandPeer≥2` **被如实落地、读数也变了**（`#home` 83.3% → 90.9%），**但只排除了 4 处里的 2 处**（另 2 处的祖父层是 `section.zone`，`grandPeers` 8 / 6）⇒ 行 45 仍 FAIL；**本行按新定义判**（见 §12.10.5 第 7 条）**再加一条排除（captain 2026-09-23 裁）：排除日期 / 时间形态** —— `2026/9/12`、`12:34` 这类是**时间戳**，本来就不是要对齐的数字列（与「时间戳不该被要求」一致），且**可判定**。**根因**（tools 实测）：`section.zone.kanban-col` 的两个直接子元素是 `kanban-head`（含「20」）与**卡片列表**（含 5 个 `div.time` 日期）⇒ 两个都「含数字」⇒ 该列被读成「一行数字」⇒「20」留在范围内。**本条的定位是「修根」而不是「逐个加例外」**（本代已判过「能不加例外就不加」）。**被否掉的备选**：(b) 要求组内数字与元素**同层** —— 依赖层级，而层级会随布局变化；(c) 把「20」列为**第 5 处**有意排除 —— 逐个加例外。 |
| 46 | **列表行主标签可读性**（列表行标签宽度） | `M1` **本行是本代第一条看「可读性」的判据** —— 之前 45 行里**没有任何一行会因为「文字被压到看不见」而变红**（用户报的第一批问题正是这一类）。**落表时有两处红**：`#knowledge` 在 390 = **0** · `#sessions` 在 390 = **40.98** / 在 768 = **59.25**（fs 14 ⇒ 约 **3 个字符**）；**绿**：`#chat` 65 / 65 / 151 · `#wiki`；**其余 8 路由 `not_measured`（无对象）** —— **这行落表时是红的，那不是失败，是它第一次真的看见了对象** | **每 capture 全绿**（`width > 0` 且 `width ≥ 5 × fontSize − 0.5`） | **对象集** = 所有**列表行主标签**，探针选择器 `.row-btn .title`（8 路由 · 14 captures：chat / knowledge / wiki / sessions / agents / runtimes / inbox / board；探针在 capture 内自扫 **1440 / 768 / 390**）。**单一下限（不分区）**：`width > 0` **且** `width ≥ 5 × fontSize − 0.5` —— **判 `width` 不判 `scrollWidth`**（省略号截断是有意的）。**5em 的出处与论证**：`.chat-session-row .title { min-width: 5em }`（`index.css:1909`）写在**同一个元素类**上、修的是 `index.css:638 .title { flex:1; min-width:0 }` 造成的**同一个塌陷** ⇒ 按**元素自身字号**算 **5em = 至少放得下 5 个字**，**对任何列表都成立**；**并且逐样本打印各自的 computed `min-width`** ⇒ 「哪些列表自己声明了保护」以**读数**而非**阈值**的形式可见。**`not_measured`** = 0 个对象或探针抛错 ⇒ **不是 PASS**。**未覆盖及理由**：memory / graph / stats / settings / home / task 的标题不在 `.row-btn` 内 ⇒ **0 个对象 ⇒ not_measured**。 |
| 47 | **点击不得改变行序** | `M1` **GREEN**（点击测试：`domChanged=false` · `apiChanged=false`） | **0 处变化**（点击前后的 API id 序列 + DOM 行序**逐位相同**） | **对象** = 点击前后的（API id 序列 + DOM 行序）。**阈值 0 处变化 —— 依据 = 裁决**（用户原始指控「点击会话把它移到最上方」）。**阈值标注：裁决** ✓ |
| 48 | **无成员变化的轮询不得改变行序** | `M1` **GREEN** | **0** | **对象** = 相邻 tick（1s）的序列差。**依据 = 测量**：**428s / 415 ticks 内纯重排 0 次** ✓。**阈值标注：测量**（不是裁决）✓ |
| 49 | **轮询引起的成员变化不得移动用户正在看的行** | `M1` **RED —— 控制跑实测 3 行移动 −35.5px（恰好一个行高）** | **≤0.5px** | **对象** = 被跟踪行在**成员变化前后**的屏幕 `top`。**依据 = 裁决**。**现状 RED 是真实缺陷**（用户报的「点击会话把它移到最上方 / 不流畅」）✓。**阈值标注：裁决** ✓ |
| 50 | **会话行的名字不得以平台注入块头开头** | `M1` **GREEN —— 0 行**（**t196 自读** `/api/v1/sessions?archived=include&limit=400` ⇒ 398 行：**title 以我们自己的块头开头 = 0 · preview = 0** ✓）—— 历史：**t167 前** title-inj **32** / preview-inj **32** · **t101 时** **26/199** ✓。**⚠️ 这是「对象集内的诚实的绿」** ✓：**对象集 = 我们自己的块头（4 条）** ✓；**对象集之外仍有 14 行**（`[search-mode]` ×1 · `<user_profile>` / `<relevant_memories>` ×13）—— **不属本行**（见判定方式列 ✓） | **0** | **对象** = `/api/v1/sessions` 每行的 `preview` / `title`。**对象集 = 我们自己的全部注入块头**，**定义成「生产者常量的那一个集合」**：`crates/daemon/src/chat.rs` 的 **`pub const INJECTED_HEADERS`** ✓ —— **生产者、索引器、本判据、面板分类器全部读同一个集合 ⇒ 不可能漂移** ✓（这是「**阈值是派生的，不可漂**」在**对象集**上的应用 ✓：凡「工具里有、契约里也有」的东西，应当**从一处派生** ✓）。**当前成员（4 条，与常量逐字一致）**：`[memory context` · `[role — you are` · `[conversation resume` · `[retry context` ✓ —— **短前缀** ✓ · **⚠️ 契约「列出」它们只为可读；成员的真相源是 `crates/daemon/src/chat.rs` 的 `pub const INJECTED_HEADERS`** ✓（**t124 已完成**：常量 = **4 条** · **8 处发出方全部改读具名常量、内联归零** · **mock 测试遍历 4 个 builder 断言「发出的块头 ∈ 常量」⇒ 不登记就红** ✓）。**说明性例子（不是成员定义）**：DOM 里**观测到的完整标题**是 `[conversation resume — you are …` / `[retry context — the previous …` —— 它们**解释了为什么成员是短前缀**：**harness 会把首条消息截断成标题**，实测存的就是 `'[memory context — what the'` ✓。**历史（保留）**：本轮之前该常量**只声明 2 条**、且**6 处发出方内联字面量** ⇒ **列举法滞后于生产者 = 「对象集比意图窄」** ✓（修法与「递归一层」实例见 §12.10.5 总纲 ✓）。**约束（三条）**：① **只含「平台自己发出的」块头** —— **不得**把用户内容里可能出现的 `[` 形态收进去 ✗（**判据的对象应是平台产物的字面量，不是对用户内容的猜测** ✓）② **新增块头时必须同步更新该常量** ✓（**改产物让正确做法成为默认** ✓）③ **只判开头、不用子串包含** ✗（用户可能在自己的消息里**引用**这些字面量 ✓）。**依据 = 裁决**。**⚠️ 它会一直红，直到存量行被清理 —— 那是诚实的红**：本修复**只前向生效**（旧 transcript 无 sentinel ⇒ 索引器 `user_text()` 是 **no-op** ⇒ 旧行永久保留注入名；**即使重新索引也不会好**）⇒ **下一个读者不要以为判据坏了** ✓ **⚠️ 现状更新（2026-09-23 t196 复核；触发条件命中后）**：**本行 GREEN** ✓ —— **t196 自读 `/api/v1/sessions?archived=include&limit=400`（398 行）：以我们自己的块头开头的 title = `0` · preview = `0`** ✓（t167 前：title-inj 32 / preview-inj 32 ✓）。**对象集边界（必须写明，否则会数错对象）**：**本行的对象集 = 我们自己的注入块头（`INJECTED_HEADERS` 那 4 条）** ✓ —— **对象集之外另有两类，都不是本行的违规** ✗：① **harness 级前缀** `[search-mode]`（**opencode** 的历史文件 ×1 —— **是 harness 自己的提示词，不是我们发出的块** ✓）② **平台记忆块** `<user_profile>` / `<relevant_memories>`（×13 —— **属记忆蒸馏 / 记忆注入会话** ⇒ **归 S7「系统来源」** ✓）。**⚠️ 触发条件的精度订正（我自己的写法有代理量）**：**复核信号必须用「本判据自己的读数」（title/preview 以 `INJECTED_HEADERS` 成员开头），不得用「以 `[` 开头」这类代理量** ✗ —— **t196 的触发正是由代理量（`[` / `<` 前缀）命中的，而判据本身一直是 0** ✓（**第 2 类的又一实例：有直接量时不要用代理量** ✓）。**⚠️ 并如实报告**：**「名字必须来自用户自己的话」这个意图，在对象集之外仍未满足** ✗（`[search-mode]` 那行是 **另一个工具的会话**，其标题是 harness 提示词 ⇒ **我们无法安全切分**（**没有我们的 sentinel，且不得猜用户内容** ✓）⇒ **建议：按 S7 的系统来源处理或另开单** ✓） |
| 51 | **窄屏输入字号 ≥16px** | `M1` **RED**（chat 10 个输入最小 **14px** · sessions 8 个最小 **14px**） | **16px** | **对象 = 只有 `input` / `textarea` / `select`**（**明确排除标题与标签** ✓）。**依据 = 裁决** · **出处 = `docs/research/web-interface-guidelines.md:28`** ✓。**断点同源 = `media (max-width: 520px)`** ✓ |
| 52 | **窄屏触摸目标 ≥44×44** | `M1` **RED**（chat 76 个可点最小 **24×24**、74 个 <44 · sessions 97 个最小 **24×24**、95 个 <44 · agents 38 个最小 **32×32**、36 个 <44） | **44px** | **对象** = 窄屏下可点击元素。**依据 = 裁决** · **出处 = `docs/research/web-interface-guidelines.md:27`** ✓。**断点同源 = `media (max-width: 520px)`** ✓。**与行 18 的关系（写明，不合并也不重复）**：行 18 判「可交互 <32px（**全站含桌面**）」，**行 51/52 判「窄屏 ≥16px / ≥44px」⇒ 对象与阈值都不同** ✓ **形态裁决见 `view-sessions.md` S10**（窄屏收进菜单 ⇒ 44px 达标 + **降低相邻破坏性动作的误触风险** ✓；**行高由内容宽度驱动**：390 档 **114px = 内容集最小值（不是缺陷）**、520 档 ≤60px 绝对上界 ✓ —— **2026-09-23 已改判据 ⑥**）✓ |
| 53 | **reduce 下不得有 >0.05s 的动效残留** | `M1` **PASS —— 但是「未验证的绿」**（tools 未做「证明 `emulateMedia` 真的生效」的对照跑） | **0 处** | **对象** = reduce 下 `transitionDuration`（或 `animationName != none` 时 `animationDuration`）**> 0.05s** 的元素。**依据 = 裁决** · **出处 = `docs/research/web-interface-guidelines.md:55` 与 `:272`** ✓。**⚠️ 未验证的绿**：**若模拟未生效，计数也会是 0 ⇒ 那是「空集绿」** ⇒ **本行在对照跑完成前不得当作已验证的结论** ✓。**跟进方法**：**对照组 = 不模拟 reduce 时的同一计数（预期远大于 0）** ✓ **⚠️ 子断言（t211 的差集 ④，2026-09-23 t216 落表）—— 本行的对象集是「声明的时长」，而「在跑的动画」必须另取一个运行期读数**：`document.getAnimations()` 里 `playState === 'running'` 的数量 ≤ 0，**或**两样本证明不变（相隔 300ms 取两次，若两次的 `currentTime` 与像素都不变 ⇒ 视为已停 ✓）。**实测**：26 capture 一致 · `>0.05s` 的元素数全 0（`index.css:254` 的全局规则把时长压到 0.01ms）· **唯一 running 是 `#chat` 的 `loadingCircle`（0.01ms infinite）而 300ms 内无可视变化** ⇒ 判**已停** ✓。**如实标注**：`#agents` / `#settings` 的 `twoSampleChanged=true` 归因是**异步内容到达**，不是动画 ✗ （**两样本法必须排除异步内容**，否则会把数据到达误判成动画在跑）✓ |
| 54 | **窄屏 #sessions 行级形态（触摸可达 + 误触风险）** | `M1` **390 档 = 114px = 内容集所决定的最小值（见算术 ⇒ 不是缺陷 ✓）· 520 档 = 60px ≤ 60 绝对上界 ✓** · 行内控件 **3 > 2** 已由菜单收窄（t130）· **审计判据的更新由 tools 另单跟进** ✓ | **行高 = 该行内容集在该宽度下所需的最小高度**（**不得出现「因控件数导致的额外换行」**）· **520 档 ≤60px（绝对上界 / 回归哨）** · **行内控件 ≤2** · **「更多」可访问名含该行会话标题** · **菜单内每个动作 ≥44×44** · **键盘全程无鼠标完成「打开→选中→执行→Esc 关闭且焦点回到更多」** | **对象** = ≤520 下 `#sessions` 的**行级形态**。**判定法（可判定）**：在 **390 与 520 两档**分别测「行高」与「该行内容单行所需高度」（后者用 `scrollWidth` vs `clientWidth` 判 ✓）⇒ **行高必须等于「该行内容不换行所需的高度」** ✓；**并保留 520 档 ≤60px 绝对上界** ✓。**依据标注**：**「按内容集可达的最小高度」= 测量 / 派生**（由内容宽度驱动 ✓）· **520 档 60px = 测量**（t108 实测）· **其余四项 = 裁决**（S10①②③④）✓。**与行 52 的关系**：行 52 = **单个元素** ≥44px（意图 = 触摸可达）；**本行 = 行级形态**（意图 = **降低相邻破坏性动作的误触风险** + 内容驱动的行高）⇒ **同一断点（520），对象与意图都不同** ✓。**与行 19 不重复**：**溢出为 0 已由行 19 覆盖** ✓。**空集语义**：**无行 ⇒ `not_measured`，不报 PASS**，且必须点名原因 ✓。**⚠️ 前提修正**：原 ⑥ 假设「控件 3→2 就能恢复行高」**不成立**（行高由**内容宽度**驱动：390 内容盒 **254px**，2 控件最小内容 **302 > 254**、3 控件 **358 > 254** ⇒ **都必然 2 行 = 114px** ✓）⇒ **判据已改，机制保留**；**菜单的价值不是密度，是误触风险**（3 个相邻 44px 按钮里「归档」与「删除」相邻 ✗）✓ |
| 55 | **同义入口：每条栏 ≥1024 下恰好 1 个可见的收起入口** | `M1` **RED**（1440 实测**两个** 32×32 的 `.chat-rail-toggle` **同时可见**：`x=422` aria=「收起侧栏」与 `x=517` aria=「收起 / 展开会话历史」）⇒ **删掉视栏那个（t134）后转绿** | **≥1024 下每条栏可见的收起入口恰好 1 个** | **对象** = 每条侧栏（主导航栏 · chat 会话栏）的**收起 / 展开入口**。**对象集的正面定义（工具已实装，锚点必须一致）**：**控件（`button` / `[role=button]`）其可访问名以「收起」或「展开」开头，且位于该栏自身 DOM 子树内（按包含关系定界，不按类名）** ✓。**为什么不用 `aria-expanded`（实测决定）**：**两个收起入口的 `aria-expanded` 都是 `null`**，而 chat 会话栏里 `.chat-group-more` / `.chat-group-toggle` 带 `aria-expanded` + `aria-controls`（**折叠的是工作区，不是栏**）⇒ **按它取会数错对象** ✗。**「可见」的可判定定义** = `width > 0` **且** `height > 0` **且**边界盒与视口相交（`getBoundingClientRect()` 与 `(0,0,innerWidth,innerHeight)` 有交集）✓。**<1024 的形态**：**只对抽屉入口（`.chat-side-toggle`，冻结选择器）计数**；**抽屉打开时若头部控件与它同时可见，同样必须恰好 1 个** ✓。**与既有行的关系（对象不同 ⇒ 不合并也不重复）**：行 18 = 命中可达 · 行 52 = 单个元素 ≥44px · 行 54 = 窄屏行级形态 · **本行 = 入口的数量** ✓。**空集语义**：该路由没有该栏 / 没有入口 / 视口内有无法归入任何栏的入口 ⇒ **`not_measured` 并点名原因**（不得静默 PASS）✓。**依据标注：裁决**（不是测量）· **出处 = `views/README.md` §S1.1** ✓ |
| 56 | **`<title>` 反映当前上下文** | `M1` **RED**：`document.title` 在 **13/13** 路由恒为 `ruagent` ✗ | **每条路由的 `document.title` 必须包含该路由的**可识别标识**，且与其它路由不同** | **对象** = 每条路由的 `document.title`。**判定法（可判定）**：逐路由取 `document.title`，要求 **① 含该路由的标识串**（路由名或其 i18n 显示名 ✓）**② 13 条路由的 title 两两不同** ✓ —— **可判定，不是「应恰当」** ✗。**依据 = 裁决** · **出处 = verifier t148 的外部标准分析**（`docs/research/` 的 409 行：103 条规则 + 33 条散文）+ 本代路由表 ✓。**现状红如实记录**；**由 captain 另派实现** ✓ |
| 57 | **导航项必须是 `<a>` 且带 `href`** | `M1` **RED**：导航 `a[href]` = **0/13** 路由 ⇒ **Cmd+click / 中键 /「在新标签打开」全部失效** ✗ | **导航项必须是 `<a>` 且带 `href`（指向该路由的 hash）** | **对象** = 主导航的导航项。**判定法（可判定）**：每个导航项元素 `tagName === 'A'` **且** `getAttribute('href')` 非空 **且**等于该路由的 hash ✓。**依据 = 裁决** · **出处 = 两条外部标准的交集（verifier t148）** ✓。**与行 22 / 行 16 的关系（对象不同 ⇒ 不合并也不重复）**：**行 22 = 浮层关闭后焦点归属** · **行 16 = 焦点环** ⇒ 对象不同；**但本行的实现可能影响它们** ⇒ **实现本行时不得破坏行 22 / 16** ✓。**现状红如实记录**；**由 captain 另派实现** ✓ |
| 58 | **URL 反映视图状态** | `M1` **RED**：**切标签后 URL 完全不变** ✗ | **URL 必须随「视图状态」变化**。**「视图状态」的对象集（正面定义）**：① **路由**（已实现 ✓）② **改变「正在看什么」的页签 / 模式**（如 `#sessions` 的来源过滤 · `#board` 的视图 ✓）③ **改变「看的是哪一个对象」的选中项** ✓；**明确排除**：**滚动位置 / 悬停 / 输入焦点 / 未提交的输入**（**瞬时 UI 状态，不属于「视图状态」** ✓） | **判定法（可判定）**：逐条切换 ② / ③ 的取值 ⇒ **`location.hash` 必须变化且可回放**（**用变化后的 URL 重新加载 ⇒ 落到同一视图状态** ✓）。**依据 = 裁决** · **出处 = verifier t148** ✓。**⚠️ 它是一次架构改动**（本应用用 hash 路由 ✓）⇒ **design-lead 判断：采纳，分两步** —— **① 先做「页签 / 模式进 URL」**（**收益最大、风险最小** ✓）· **② 再考虑「选中项进 URL」**（**涉及历史栈语义 ⇒ 需单独裁决** ✓）；**不采纳「瞬时 UI 状态进 URL」** ✗（理由：**会污染历史栈，且用户不会分享它** ✓）。**现状红如实记录**；**由 captain 另派实现** ✓ **⚠️ capture 集必须写进行内（2026-09-23 t201）—— 否则同一行的红/绿取决于谁跑它** ✗：**对象集 = ① 路由 ② 改变「正在看什么」的页签 / 模式 ③ 改变「看的是哪一个对象」的选中项** ✓ —— **不得弱化成「至少要有一个视图接上」** ✗（**知识视图的标签切换属 ②** ⇒ **它是实现缺口，不是对象集之外** ✓）。**capture 集（可判定）**：**= 13 条路由中「存在 ② / ③ 控件」的全部路由 × 其全部取值** ✓ —— **枚举必须由路由表 / 控件扫描产生，不得手写清单** ✗（**手写 = 由回忆产生 ⇒ 必然漏** ✓）；**枚举结果必须写出清单**（哪条路由 · 哪个控件 · 几个取值 ✓）；**某路由没有 ② / ③ 控件 ⇒ 不入 capture 集，但要记录「不适用 + 原因」** ✓（**不得静默跳过** ✗）。**判定输出必须逐路由** ✓：**PASS 必须带「覆盖了 N 条路由 / M 个控件状态」的读数**（**一个笼统 PASS 不算过** ✗）。**已知实例（verifier 复跑）**：`#sessions` 源筛选 `dsh` ⇒ `#sessions?src=dsh` ✓ 且**回放有效** ✓ · `#knowledge` 切「Wiki」⇒ `location.href` **完全不变** ✗ ⇒ **后者是缺口**（**t201 已报告 captain 派单** ✓）。**⚠️ 这条口径何时被推翻（可证伪）**：① **若某个属 ② / ③ 的控件在 URL 变化后「重新加载落不到同一状态」** ⇒ 回放语义写错了 ✓；② **若枚举出的 capture 集里出现「同一控件的两个取值产生同一个 hash」** ⇒ 该控件其实是瞬时 UI ⇒ **对象集写宽了** ✗；③ **若枚举只覆盖 1–2 条路由、而路由表里明显还有别的页签** ⇒ **枚举法漏对象**（**必须由路由表遍历产生** ✓）。**⚠️ ③ 是待裁决项（原文自己写「需单独裁决」）⇒ 本行的判定当前只判 ②** ✓。**③ 的现状读数（t208 实测）**：图谱选中态**不进 URL**（切 `#graph?mode=list` 再切回，选中态 / 墨迹 / Σα 与检查器**逐位保留**；全新加载 `#graph` ⇒ 无选中）⇒ **判定 = 不进 URL 是对的** ✓（理由：实体 id 是**活数据的身份** ⇒ 会出现「有时能回放、有时不能」的 URL，那是本行证伪条款 ② 的邻居 ✗；且点节点是高频道动作，进历史栈会污染返回键 ✓）。**⚠️ 而枚举方法有一个结构性盲区（t208 发现）**：本行的 capture 集由 `[role=tab]` / radio-in-label / `aria-pressed` / `select` 扫描产生 ⇒ **画布上的命中目标（canvas 内的节点）结构上不可能出现在这种枚举里** ⇒ **若 ③ 要涵盖它，枚举方法本身必须先扩**（否则「不得手写清单」会退化成「**方法决定对象集**」✗）。 |
| 59 | **图谱类别色的 CVD 可辨性（类别之间）** | **GREEN —— 诚实的绿**：t153 落地后 **36 对 × 3 种 CVD 全部 ≥10** ⇒ **绿了，但余量薄**：**亮最差 10.27（org↔product · deuteranopia）⇒ 余量 +0.27** ✓ · **暗最差 12.22（person↔default · protanopia）⇒ 余量 +2.22** ✓；**改前最差 1.6** ✓。**⚠️ 余量只有 0.27 ⇒ 任何配色微调都可能把它推红** ✓（**2026-09-23 t184 复测订正**：原文写「最差 10.7，离阈值 0.7」—— **数被写乐观了** ✗） | **ΔE ≥ 10** | **对象集** = **9 个类别色两两配对 × 3 种 CVD（protanopia / deuteranopia / tritanopia）** ✓（9 选 2 = **36 对** ✓）。**⚠️ 度量口径（必须写明，否则同一判据在不同口径下会给出不同结论）**：**CVD 模型 = Machado 2009 · severity 1.0** ✓ · **ΔE 度量 = CIE76（Lab · D65）** ✓ —— **t153 的读数表明数值随口径变：同一对 CIE76 读到 1.6、CIEDE2000 读到 2.0** ✓。**⚠️ 这是惯例阈值**：**CIE76 的 JND ≈ 2.3 ⇒ ≥10 ≈ 4× JND** ✓；**而 WCAG 1.4.1 并不要求颜色分离，它要求「第二通道」** ✓（**这正是为什么还需要第二通道那条判据** ✓）。**与行 31 / 行 33 的关系（对象不同 ⇒ 不合并也不重复）**：**行 31 = 图谱边对比度** · **行 33 = 分类色非文本对比度**（**色 ↔ 背景** ✓）· **本行 = 类别之间的可辨性**（**色 ↔ 色** ✓）。**依据标注 = 裁决** ✓ · **出处 = t153 的实测读数（36 对 × 3 种全部 ≥10，最差 10.7）** ✓ |
| 60 | **卡片头行的同列对齐（标签宽度不得推动后续兄弟）** | `M1` **GREEN（按正确的采样面）**：t215 三档实测 —— `#runtimes` @390 3 实例 `k1w38 → k2x148` **集合大小 1** ✓ · @768 2 实例 `k2x148` + 1 实例 `k2x492`（**不同列**）✓ · @1440 每列 1 实例（`k2x325` / `k2x713` / `k2x1100`）✓；`#agents` @390 **7 实例全部 `k1w36 → k2x146`（集合大小 1）** ✓ · @768 4 实例 `k2x146` + 3 实例 `k2x490` ✓ · @1440 `k2x323`×3 / `k2x711`×2 / `k2x1098`×2 ✓ ⇒ **同列内集合大小 = 1（三档全部成立）** ✓。**历史（保留）**：t213 报「`#runtimes` 差 52px / `#agents` 漂移」**不复现** ✗ —— 那是**度量口径缺陷**（把同一容器类的**不同行**与**不同网格列**混比），逐实例读数见 §12.17 ① | **同一网格列内，「头行的第 2 个可见兄弟」起始 x 的集合大小 = 1（容差 ≤1px）** | **对象集（正面定义）** = **卡片头行** = **首个子元素是头像 / logo / 图标载体**（`.agent-avatar` / `.runtime-logo` / `.doc-icon` / `.ant-avatar`）的行，**且该行有 ≥2 个可见兄弟**（**可见 = 附录 A 口径**：rect 宽高 ≥1px ∧ `visibility ≠ hidden` ∧ `display ≠ none` ∧ `opacity > 0`）。**采样面（必须写进行内）**：① **先按网格列分组** —— 列 = 卡片（或行容器）左边界 x 按 ≤1px 容差聚类，**不是**「同一容器类」（t215 的反向证据：按容器类分组会把 logo 行与名称行、三列栅格混成一堆 ✗）；② 每组内取「**第 2 个可见兄弟**（**按 DOM 序，不是视觉序** —— flex `order` / 绝对定位会给出不同顺序，2026-09-23 t224 补）」的 `getBoundingClientRect().left`；③ **同组内集合大小必须 = 1（≤1px）**；④ **某列只有 1 个实例 ⇒ 该列无可比对象**，必须登记「该列 1 实例、不可判」而**不得**算作通过（否则本行在多列栅格上退化为恒 PASS ✗）；⑤ 逐实例打印 `首子元素宽 · 第 2 兄弟 x`（**读数而非阈值** ✓）。**路由 × 视口** = 13 路由 × {390, 768, 1440}；**无头行的路由 ⇒ `not_measured` 并点名原因**（`#inbox` / `#board` 卡片无 avatar / logo 头行 ⇒ 该口径不适用 ✓）。**可证伪条件**：① 同一列内出现两个不同的「第 2 兄弟 x」⇒ FAIL（**正是用户报的那一类** ✓）；② **把「同一容器类」当分组 ⇒ 本行必须 FAIL**（t215 的假阳性就是它的反向证据 ✓）；③ 注入一个 `width: 90px` 的标签进头行 ⇒ 同列集合大小变 2 ⇒ FAIL。**依据 = 裁决**（用户亲口报的一类）· **出处 = t215 的三档逐实例读数 + `views/README.md` §S1.1 的「按包含关系定界」手法** ✓ |
| 61 | **散文块的列宽（measure，`ch`）** | `M1` **GREEN —— 对象集内 1 个对象**：13 路由 × 1440 实测 —— `#home` `.mem-preview`（**2 行**）**`ch` = 43** ✓；`#memory` `p.memory-content` 容器 **`ch` = 132 但文本 44 字符 / 1 行 ⇒ 不入对象集**（见判定方式）· `#task/<id>` 的 2 个 `.md p` 均 **1 行 ⇒ 不入对象集** · 其余路由 **0 对象**。**⚠️ 历史读数归因（保留）**：t213 报「`#memory` 行长 **139** 字符 / 行 ✗」= **把容器量当成行长**（宽容器 + 短文本 ⇒ 1 行），与 §12.17 ① 同一形状 | **对象集内每个散文块的列宽 ≤ 90 `ch`**（**无下界** —— 理由见判定方式） | **对象集（正面定义）** = **渲染为 ≥2 行的散文块**：**扫描域 = `main.ant-layout-content.content` 子树内的全部元素**，**排除 antd 注入子树**（`[class*="ant-"]`）—— **排除项的条数、以及其中「自身文本 ≥40 字符」的容器数必须一并打印**（与行 42 的「排除项要打印出来」同一规矩 ✓；**若 markdown 渲染在 antd 容器内会被这条排除漏掉 ⇒ 以打印的条数为准，不得静默** ✗）；对象 = **该元素自己的文本节点**（不是子树文本）≥40 字符 ∧ 可见（**附录 A 口径**：rect 宽高 ≥1px ∧ `visibility ≠ hidden` ∧ `display ≠ none` ∧ `opacity > 0`）∧ 不是 `pre` / `code` / `table` / 数据行 ∧ 不是**有意截断**（`text-overflow: ellipsis` 或 `-webkit-line-clamp ≥1`）∧ **行数 ≥2**。**「1 行不入对象集」必须写进行内**：1 行 ⇒ 该块**没有折行** ⇒ 它的容器宽度**不构成「行长」** —— 宽容器 + 短文本会让容器量报出 130+ 而**文本只有 44 字符**（这正是 t213 的 139 的成因 ✗）；把 1 行实例判 FAIL 就是**把「容器宽」当缺陷**（与 F-215a 同族：结论取决于采样面 ✗）。**采样面**：13 路由 × {390, 768, 1440}，**等 DOM 静止**（`--dom-quiet`）后取一次；**量 = 容器 measure** = `(clientWidth − paddingLeft − paddingRight) ÷ advance('0')`（`advance('0')` = 用该元素的计算 `font` 在 canvas 上 `measureText('0').width`）；**不量 `text.length ÷ 行数`** —— 后者对末行不满的块**系统性低估**（250 字符 / 3 行读到 83，而真实最长行可到 90+ ✗）；**行数** = 用 `Range.getClientRects()` 数**该元素自己文本节点**的行盒并按 **4px** 聚类（§12.14.1 行 40 的同一手法 ✓）；**退路**（拿不到 Range 时）= **内容盒高 ÷ 计算 `line-height`**，其中内容盒高 = 盒高 − `paddingTop` − `paddingBottom` − `borderTop` − `borderBottom`（**不扣内边距会把「1 行 + 大内边距」的块算成 ≥2 行** —— `.mem-preview` 是 `button`，正是这种 ✗，2026-09-23 t224 修）。**逐实例打印**：选择器路径 · 行数 · `ch` · 文本字符数。**目标只有上界**：**下界会把窄卡片预览判红** —— 窄栏是**布局选择**，不是可读性缺陷（`#home` 的 `.mem-preview` 实测 43 `ch` 就是这种合法窄栏 ✗）。**可证伪条件**：① 对象集内任一实例 `ch > 90` ⇒ FAIL；② **把 1 行实例并进对象集** ⇒ `#memory` 的 132 `ch` 会立刻 FAIL ⇒ 那证明采样面写宽了（**必须靠这一条自检** ✓）；③ 空对象集 ⇒ **`not_measured` 并点名**，不得报 PASS。**依据 = 裁决 + 惯例（两者分开写，2026-09-23 t224 修）**：**惯例** = 经典可读性上限 **45–75 字符**（Bringhurst 的传统区间 —— **本契约没有逐字出处 ⇒ 标为惯例、且不作为本行的判定值** ✓）；**裁决** = **本行取 90 `ch` 作上界**（给数据密集面板留余量；**它不是派生值** ⇒ 要收紧到 75 只需改这一个数 ✓）。**出处 = 本代实测（t216 的对象集读数）+ t213 / t215 的读数** ✓ |
| 62 | **同类共享组件的形态跨视图唯一（含字体族配对）** | `M1` **GREEN**：`.tag` 在 **8 条路由**（home 8 / sessions 30 / board 3 / memory 39 / agents 18 / runtimes 15 / stats 20 / settings 1）实测**形态元组唯一** = `(h 20 · radius 4px · padding 2px 8px · font 11px · family "IBM Plex Mono")` ⇒ **集合大小 = 1** ✓（**2026-09-23 t215 修**：`#settings` 曾因 `.tag` 是**行内元素**、被父级 15px 行高挤成 **h = 18** ✗ ⇒ `.tag { display: inline-block }` 后 h = 20 ✔）。其余族：`.ant-btn` 12 路由 **1 种**（h32 / radius 4 / padding 0 8px / font 14）· `.ant-card` radius 10 / padding 0 / font 14 一致 · `.ant-empty` 1 种 · `.ant-table` 1 种。**模态本次采样 0 个**（需交互打开）⇒ **未测，不冒充已测** ✗。**字体族**：实测 2 族、职责分明（`code` 族 = `IBM Plex Mono`、其余 inherit）⇒ **并入本行的元组**（t213 的 ⑨ 不另立行，理由见 §12.17 ⑦） | **每个组件族在每个模式下，跨 13 路由的「形态元组」集合大小 = 1**；**缺该族的路由 `not_measured` 并点名** | **对象集（正面定义）** = 共享层定义的**同类组件族 × 变体**：`.tag` · **`.ant-btn-primary` / `.ant-btn-default` / `.ant-btn-text` / `.ant-btn-link`（变体各自成族）** · `.ant-card` · `.ant-empty` · `.ant-table` · `.md code`（**具名清单，随共享层新增而增列**；**不得**用「class 前缀相同」自动扩 ✗ —— 那会把内容驱动的差异算成形态差异）。**族内只比较同变体的实例** ✓（2026-09-23 t224 修：原把 `.ant-btn` 当一族 ⇒ primary 与 text 的 padding 本来就不同，会把**合法差异**判成形态不一致 ✗）。**形态元组** = `(height, border-radius, padding, font-size, font-family)` **五元**，全部读 `getComputedStyle` + `getBoundingClientRect`（**不读源码**：源码是意图、计算样式是事实 ✓）。**采样面**：13 路由 × {dark, light} × {390, 768, 1440}；**每族每 capture 逐个实例打印元组与计数**（**读数而非阈值** ✓）；**高度随内容变化的族**（`.ant-card` / `.ant-table`）**只判元组里的非高度项**，高度另列「内容驱动」而不判（否则 373 / 339 / 289 / 660 这 4 个高度会被误判成形态不一致 ✗ —— 它们由内容决定）。**可证伪条件**：① 任一族的元组集合大小 ≥2（**同模式同视口内**）⇒ FAIL；② **反向证据（本代已实测过）**：`#settings` 的 `.tag` 去掉 `display: inline-block` ⇒ h 回到 18 ⇒ 本行必须 FAIL；③ 某族在某路由**一个实例都没有** ⇒ `not_measured` 并点名，不得静默 PASS。**依据 = 裁决**（用户报的「同类组件在不同页长得不一样」）· **出处 = t213 的形态读数 + t215 的父级行高归因** ✓。**与行 8 / 9 / 10 的关系**：那三行判**单个属性是否越界**（字号 / 字重 / 圆角 ∈ 阶梯），**本行判同一组件族的跨视图一致性**（同一个 chip 在 8 条路由上必须是同一个盒子）⇒ 对象不同、不合并 ✓ |
| 63 | **图标不得用 emoji 码点表示** | `M1` **GREEN —— 0 处**（12 路由 × 1440 实测；t213 对全部文本节点做过码点扫描，亦为 **0** ✓） | **0 处** | **对象集** = 渲染文本里的 **emoji 码点**：`U+1F000–U+1FAFF` · `U+2600–U+27BF` · `U+FE0F`（变体选择符）· `U+1F1E6–U+1F1FF`（区域指示符）。**采样面** = `document.createTreeWalker(body, SHOW_TEXT)` 遍历**全部文本节点**（**不是** `innerHTML` 正则 —— 后者会命中**源码里的**码点与注释，且对 `&#x1F600;` 这类转义会漏报 / 误报 ✗），逐条打印 `码点 · 所在元素选择器路径 · 上下文 24 字符`；**排除**：不可见文本（**文本节点没有盒子 ⇒ 取最近的元素祖先的计算可见性**，按附录 A 口径，2026-09-23 t224 补）与代码块（`.md pre` / `code` —— 那是**内容**不是 UI 图标 ✓）。**可证伪条件**：① 任一处命中 ⇒ FAIL；② **注入**一个 `✅` 到按钮文本里 ⇒ 必须 FAIL（must-FAIL 自检）；③ **文本节点数为 0**（页面没渲染完）⇒ `not_measured`，不得报 PASS（**空集绿是这条判据最容易退化的方向** ✗）。**依据 = 裁决**（设计语言：图标一律走 `Icon` 原语；emoji 在不同平台字形不一致）· **出处 = t213 的实测 0 处 + `primitives.md` §3.1 的图标原语** ✓ |
| 64 | **首屏实际传输字节（HTML 里 eager 的全部东西）** | `M2` 当前构建实测（`index-VPLeVyHd`）：**≈1180KB raw / ≈377KB gzip** = 入口 `index` 108.6 + modulepreload `vendor` 1019.9 + `vendor.css` 7.5 + `index.css` 44.0（gzip 分量 35.8 + 324.2 + ~1.5 + ~11.6）。**⚠️ 子断言 ① 当前为红（登记，不参与本行的 PASS / FAIL）**：MASTER 登记的地板 gzip **≈305KB** vs 实测 **324.2KB（+6.3%）** ✗ | **≤ 实测地板 + 应用层预算（行 27 的 150KB / 48KB）**；**CSS 字节只作读数、不参与判定**（当前两张 CSS 实测 7.5 + 44.0KB raw / ≈13KB gzip）—— **本行的阈值里不得出现任何未在行内定义的量** ✓（2026-09-23 t224 修：原写「+ CSS 预算」，而它全行无值无源 ⇒ 两个实现者会给两个阈值 ✗）；**且子断言 ①：登记的地板值与实测框架层 chunk 之差 ≤ 5%** | **对象集（正面定义）** = `panel/dist/index.html` 里**首屏必须下载**的资源：入口 `<script src>` + **每一条** `<link rel="modulepreload">` + **每一条** render-blocking `<link rel="stylesheet">`；**按构造不含**懒加载 chunk（它们不在 HTML 里 ✓）。**采样面**：**读产物**（解析 `dist/index.html`，**不读 `src/`、不读 `vite.config.ts`** —— 那是意图），逐条打印 `文件名 · raw · gzip · 类别（入口 / 框架 / 路由 / 样式）`；**K = 条数**（当前 4 条）。**目标为什么是派生式而不是常数**：地板是**登记值**（行 27 的裁决：随技术栈变更**重测、不写死**）⇒ 本行的预算 = **实测地板 + 行 27 的预算**，**地板分量不参与判定**（否则框架升级会让本行无故变红 ✗）。**子断言 ①（地板漂移）**：**`框架层` = 行 27 判据里的 `vendor` 块**（`vite.config.ts` 的 `manualChunks` 三分：vendor / app / markdown；**本行不另立定义** ✓，2026-09-23 t226 补 —— 否则本行的阈值里又出现一个未定义的量 ✗）：`abs(登记值 − 实测框架层) ÷ 实测 ≤ 5%` ⇒ 超出即 **FAIL**（**305 vs 324.2 正是它的落点** ✓）；**修法是把登记值更新为实测值**，**不是**改本行的阈值；**⚠️ 更新登记值需要工具同步** —— `panel/tools` 的自检把 980 / 305 钉成 must-PASS 断言 ⇒ 单改 MASTER 会让工具自检变红 ✗ ⇒ **已报告 captain 派单**（§12.17 ⑤）。**空集语义**：`dist/index.html` 不存在、或解析不出任何 eager 资源（`<script src>` / `modulepreload` / render-blocking stylesheet 全空）⇒ **`not_measured` 并点名原因**（不得静默 PASS ✗）。**可证伪条件（含 t225 的「太松 / 太严」三问，2026-09-23 t226 写进行内）**：① 首屏实际传输 > 实测地板 + 应用层预算 ⇒ **FAIL**；② 登记地板与实测差 > 5% ⇒ 子断言 ① **FAIL**；③ **把一条懒加载 chunk 加进 `modulepreload`** ⇒ K 变、读数变（**这是本行必须抓到的形状** ✓）；④ **太松**：HTML eager 集合不变而**总量 > 地板 + 150KB 仍 PASS** ⇒ **判据坏了**（必须 FAIL）✗；⑤ **太严**：只增一张**合法的 render-blocking CSS**（读数变）而判定**翻转** ⇒ **CSS 又回到阈值里了**（必须**不翻转** ✓）；⑥ **太严**：框架升级使地板 **+30%** 而本行**无故变红** ⇒ **地板分量又参与判定了**（必须**不红** ✓ —— 地板随实测走）。**依据 = 裁决**（t212 的差集 ①）· **出处 = t7 的 1405KB / 438KB 与 t212 的 1180KB / 377KB 是同一个量的两轮读数** ✓ |
| 65 | **路由级分割必须真的惰性（逐路由的 chunk 请求集合）** | `M2` **GREEN —— 越界项 0**（**2026-09-23 t226 复测，t214 落地后的构建**：`dist` 比 `App.tsx` 新、`requestIdleCallback` 计数 0 ✓）：13 条路由冷加载**每路由只取 3–5 个 JS chunk**（`index` + `vendor` + **自己的路由 chunk** + 视需要 `lazy-markdown` / `brand` / `CreateTaskModal` / `markdown`）⇒ **每路由只取自己的路由 chunk**：home→`Home` · chat→`Chat` · sessions→`SessionsView` · board→`Board` · memory→`Memory` · knowledge→`Knowledge` · graph→`Graph` · agents / stats / inbox→**共享 `Agents`** · runtimes→`Runtimes` · settings→`Settings` · task→`TaskDetail` ⇒ **越界 0** ✓。**逐路由读数（JS 请求数 / raw 字节）**：home 4 / 1,165,558 · chat 5 / 1,188,791 · sessions 4 / 1,171,887 · board 4 / 1,163,245 · memory 4 / 1,173,657 · knowledge 4 / 1,329,366 · graph 4 / 1,178,037 · agents 3 / 1,172,980 · stats 3 / 1,172,980 · settings 3 / 1,159,944 · inbox 3 / 1,172,980 · runtimes 4 / 1,167,715 · task 4 / 1,332,259（字节里 vendor 占 1,044,405 ⇒ **请求数才是本行的量** ✓）。**历史（保留）**：t212 在 t214 之前实测**每路由 16 个 chunk / 1453KB**（12 个路由 chunk 全被取）⇒ RED ✗ —— **t214 移除 idle 预热后本行转绿** ✓ | **load 后窗口内请求的 JS chunk 集合 ∩ 其它路由的 chunk 集合 = ∅**（**判定的形状 = 「不得请求其它路由的 chunk」** ✓）。**为什么这个集合可判定（2026-09-23 t226 修）**：**「其它路由的 chunk」是有限、可枚举的**（`dist/assets/` 里与路由同名的 chunk），判定只需**一次集合求交**，**不需要任何可达性分析** ✓；而 t224 写的「⊆ 首屏 ∪ 本路由 chunk ∪ **按需可达**的懒加载 chunk」**不可判定** ✗ —— 它要的是**构建期 chunk 图**，而对象集是**运行期请求（CDP）**，**两个不同源的东西没法判**。**天然放行**（它们**都不是路由 chunk**）：`vendor-*.js` · `index-*.js` · `markdown-*.js` · `lazy-markdown-*.js` · `CreateTaskModal-*.js` · `brand-*.js` ✓ | **对象集** = 窗口内**实际发起的 JS 请求**（CDP `Network.requestWillBeSent`，`type === "Script"`）。**采样面**：13 路由 × {dark, light}，**窗口 = load 起 2.5s**（与 `--api-window` 同源口径，写进读数）；**首屏 chunk 集合由 `dist/index.html` 派生**（**不是手写清单** ✗）；**禁止集（其它路由的 chunk 集合）的枚举 = 三个派生源的合成**：① **路由表**（13 条）② `dist/assets/` 的 **chunk 文件名** ③ **路由 → 视图模块的映射**（`App.tsx` 的 `load` 表 —— **读表，不手写字面清单** ✗）。**⚠️ 共享 chunk 必须按映射处理**：`#inbox` / `#stats` 与 `#agents` **共用 `Agents-*.js`** ⇒ 对这三条路由，`Agents-*.js` **属于自己、不在禁止集里** ✓（**按「13 条路由 = 13 个 chunk」枚举会造出一个不存在的 chunk** ✗ —— t226 实测：dist 里只有 **11 个**路由名 chunk）。**非路由 chunk 永不在禁止集里** ⇒ `#knowledge` / `#task` 取 `markdown-*.js`、`#home` / `#board` / `#task` 取 `CreateTaskModal-*.js` **都判 PASS** ✓。逐路由打印 `请求到的 chunk 名 · 是否首屏 · 是否本路由 · 越界项`。**与行 27 断言 ② 的关系**：断言 ② 只要求「**存在**路由级分割 chunk」（**形式**），**本行要求「只在需要它的路由取」**（**意图**）⇒ 对象不同、不合并 ✓。**可证伪条件（含 t225 的「太松 / 太严」三问，2026-09-23 t226 写进行内）**：① 任一路由请求了**别的路由**的 chunk ⇒ **FAIL**；② **反向证据（本代已实测）**：把 idle 预热放回去（t169 的形状）⇒ 13 路由全部请求 12 个路由 chunk ⇒ 本行必须 FAIL（t212 的 16 chunk / 1453KB 就是这个状态 ✓）；③ **太松**：某路由取到**另一个路由**的 chunk 仍 PASS ⇒ **判据坏了** ✗；④ **太严**：`#knowledge` / `#task` 取 `markdown-*.js`、`#home` / `#board` / `#task` 取 `CreateTaskModal-*.js` **必须 PASS** ✓（**已实测**：knowledge 4 chunk / task 4 chunk / home 4 chunk ✓）；⑤ **太严**：把某路由 chunk 从**静态依赖**改成**动态 import** ⇒ **请求集合应不变**；**若判定翻转 ⇒ 说明集合依赖了「静态 / 动态」这个实现细节、而不是「是不是别的路由的 chunk」** ✗；⑥ 窗口内**一个 JS 请求都没有**（页面没加载）⇒ `not_measured`，不得报 PASS。**依据 = 裁决** · **出处 = t212 的逐路由读数 + t214 的移除依据（`panel/src/App.tsx` 的注释）** ✓ |
| 66 | **JS 未使用占比（逐路由）** | `M2` **未使用 28–32%（使用率 68–72%，CDP `takePreciseCoverage`）—— 当前状态判 `not_measured`**（**分量未分解**，见判定方式：本行在分量打印出来之前**不得报 PASS**）✗（2026-09-23 t224 修：原基线列写 PASS，与判定方式列的 `not_measured` 冲突 ⇒ 同一行两列两个结论 ✗）。**⚠️ 分量未分解**：16 个 JS chunk / 1453KB 里框架层（vendor）**1044KB = 72%** ⇒ **这个比值主要由框架层决定**，尚未按「框架层 / 应用层」分解 ✗ ⇒ **在分量分解打印出来之前，本行不得当作已验证的结论**（与行 53 的「未验证的绿」同一处置 ✓） | **未使用占比 ≤ 35%**（**口径 = 该路由窗口内下载的全部 JS**：`index` / `vendor` / 本路由 chunk / 非路由 chunk，**不含 CSS** —— 2026-09-23 t226 把「整页口径」这个**未定义的词**换成可判定的定义 ✓）；**且必须打印「框架层 / 应用层」两个分量**（**框架层 = 行 27 的 `vendor` 块**，与行 64 同一派生 ✓）；**⚠️ 阈值的出处 = 裁决**（**2026-09-23 t227 逐条排除另外两种可能**：**派生**不成立 —— 唯一可能的派生源 §9.1 是**时间阶梯**（100 / 120 / 180 / 260ms），与**字节占比**无关；行 27 / 64 的量是**应用层绝对预算**（150KB / 48KB），而本行的分母是**该路由下载的全部 JS**（vendor 占 72%）⇒ **不是同一个量、无派生式** ✗；**惯例**不成立 —— `docs/research/` + `MASTER.md` + `primitives.md` 全文搜 `unused` / `coverage threshold` / `dead code` / `tree-shake` / 未使用代码 ⇒ **0 命中** ✗）；**唯一待查的是「分量分解」这一件事**（**当前唯一读数 28–32% 来自 t212 的整页口径、未按分量分解**）⇒ **首次实现必须给出分量读数，否则本行报 `not_measured`** ✓（2026-09-23 t228 修：**原目标列的措辞**是「出处待查」、而判定方式列写「依据 = 裁决」⇒ **一个读者读出两个结论** ✗ —— 与 t224 修掉的「同一行两列两个结论」同形状） | **对象集** = 该路由窗口内**下载的 JS** 的执行覆盖（CDP `Profiler.startPreciseCoverage` / `takePreciseCoverage`）；**未使用占比 = Σ(未执行字节) ÷ Σ(下载字节)**。**⚠️ 本行的阈值里不得出现未定义的量（2026-09-23 t226 自检，与 F-216-01 同形状）**：原写「整页口径」—— **「整页」指哪一页、含不含 CSS、含不含外壳，行内一个字都没说** ✗ ⇒ 已换成上面那句可判定的定义；**「框架层」按行 27 / 行 64 的同一派生读**（`manualChunks` 的 vendor 块）✓。**采样面**：13 路由 × {dark, light}，窗口 = load 起 2.5s，**必须在窗口末尾 stop 一次**（提前 stop 会把「尚未执行的懒代码」算成未使用 ✗）；逐路由打印 `总字节 · 已执行 · 未使用% · 框架层分量 · 应用层分量`。**可证伪条件**：① 未使用 > 35% ⇒ FAIL；② **分量分解缺失 ⇒ `not_measured`**（**不得**用一个主要由框架层决定的比值冒充应用层读数 ✗）；③ **反向证据**：往一个路由 chunk 里塞一个 200KB 且不被调用的模块 ⇒ 未使用占比必须上升并越过阈值。**依据 = 裁决**（t212 的差集 ③；**t227 已排除派生与惯例**，逐条见目标列）· **⚠️ 阈值不是派生的**（当前余量 3–7pt）⇒ **下一次复测若余量 ≤0，必须重新裁决阈值而不是调数** ✓。**⚠️ 为什么是 35 而不是 30 / 40（2026-09-23 t228 补 —— 原文唯一的锚是「实测带 + 余量」，而任何高于实测带的数都满足它 ✗）**：实测带 **28–32%**（t212，13 路由）⇒ 取**带上沿 + 3pt**，**用途是吸收跨构建漂移**；**漂移量级在行 27 已有读数**（首屏 JS **1405 → 1406KB** · 地板声明 **305KB vs 实测 324.2KB = +6.3%**）⇒ **3pt 是「实测带宽度（4pt）」与「漂移量级」之间的折中** ✓；**30 会落进实测带内**（把当前就有 32% 的路由直接判红 ✗）、**40 会让「上升 8pt 仍 PASS」**（漂移吸收过头 ✗）⇒ **35 是裁决，而这段理由就是它的依据** ✓。**⚠️ 不要把它类比成行 59 的 ΔE ≥ 10（2026-09-23 t228 删掉该类比）**：**行 59 的判定方式列自带完整度量口径（有来源）**，而本行的 35% **没有来源** ⇒ **用有来源的阈值去类比无来源的阈值，等于把「惯例」这个标签借给一个惯例不支持的数** ✗；**若一定要说同类，正确说法是「同为裁决」** —— 两条的**共同点只有「由人裁决 + 自带再裁决条款」**，**不是「有同一个出处」** ✓ |
| 67 | **首绘与首次内容绘（FP / FCP）** | `M2` **GREEN**：13 路由 **FP = FCP 逐路由完全相同**（240/240 · 264/264 · 272/272 · 276/276 …）⇒ **首绘即有内容，没有「只有背景的白屏首绘」** ✓；**FCP 240–276ms**（DCL 95–129ms · load 99–133ms） | **FCP ≤ 1s ∧ FP = FCP** | **对象集** = `PerformancePaintTiming` 的 `first-paint` 与 `first-contentful-paint` 两条 entry。**仪器 = Chromium headless（Playwright 自带，视口 1440×900）** —— paint timing 的支持度随浏览器不同，**换仪器即换结论** ⇒ 必须点名（2026-09-23 t224 补）✓。**采样面**：13 路由 × {dark, light} × {1440}（**本行判的是「有没有内容」与一个上界，故不逐宽度判定**；宽度读数另列）；**必须在 load 后等到两条 entry 都出现再读**（缺任一条 ⇒ `not_measured`，**不得**把「没有 entry」当成 0 ✗）；逐路由打印 `FP · FCP · DCL · load`。**与行 26 的关系（对象不同 ⇒ 不合并）**：**行 26 = 最长单次阻塞（主线程）** · **本行 = 首绘时刻（渲染）** ⇒ 对象不同 ✓。**可证伪条件**：① `FCP > 1000ms` ⇒ FAIL；② `FP ≠ FCP`（先画背景再画内容）⇒ FAIL；③ **反向证据**：在 `index.html` 里插一条同步长任务 ⇒ FP / FCP 分离或超界 ⇒ FAIL。**依据 = 裁决** · **出处 = t212 的 13 路由逐条读数** ✓ |
| 68 | **微交互时长必须落在 §9.1 的四档阶梯上** | `M2` **RED —— 面板声明的 6 个 transition 时长 + 4 个 animation**（`panel/src/index.css`，作者来源）：transition **100ms**（`.row-btn` ✓ · `.recall-stub-head` ✓ · `.neighbor` ✓ —— **`.ws-new-btn` 实测 120ms ⇒ 已移出本档**，2026-09-23 t224 订正）· **120ms**（`.kanban-card` ✓ · `.suggest-chip` ✓ · `.composer` ✓ · `.readout-btn` ✓ · `.guide-card` ✓ · `.icon-btn` ✓ · `.ws-new-btn` ✓ · `details.chat-thought > summary::after` ✓ —— **`.mem-preview` 不在其中**：`index.css` 里它只有 `:hover { background }`、**没有 transition 声明** ⇒ 2026-09-23 t224 订正，§9.1 的「实测现存」列同步订正）· **180ms**（`.recall-expand` 的 `grid-template-rows`）· **`200ms`（`.chat-side` 抽屉 `transform`）✗** ；animation **260ms**（`view-in` ✔ = `dur-4`）· **`160ms`（`.cmdk` 的 `cmdk-in`）✗**（**这是动画、不是 transition ⇒ 2026-09-23 t224 订正归类**）· **`1.2s` / `1.6s`**（`typing-bounce` / `pulse`，`infinite`）⇒ **按结构豁免**（`iteration-count: infinite` 的不定长循环）不计入分子。**⚠️ 两个越界项与 §9.1 的散文记录一致**（§9.1 原文：「`.chat-side` 抽屉 `transform 200ms` … 实测共 2 处」）⇒ **本行把那段散文变成可判定的读数** ✓ | **面板声明的每个 `transition-duration` / `animation-duration` ∈ §9.1 表的四档（从表派生）∪ 具名豁免（`animation-iteration-count: infinite` 的不定长循环）** | **对象集 = 面板声明的时长**（作者来源：构建后的 `index.css` + 元素 `style` 属性），**不是计算值** —— **为什么必须这样定**：计算值里 **156 个元素**的 `color 0.3s` 来自 **antd 的规则**（`a.brand` 上实测）、`all 0.2s`（`aside.ant-layout-sider`）与 `width 0.3s`（`ul.ant-menu`）同样是 antd 的 ⇒ **按计算值判会把 antd 的内部时长算成我们的越界** ✗（**t211 报的「A.brand 的 color 0.3s」正是这一类的误认**）。**档位从 §9.1 派生、不在本行复述数值（2026-09-23 t226 修）**：**读表** = 解析 §9.1「四个时长 token」表的**值列**（`dur-1` … `dur-4` ⇒ 当前 100 / 120 / 180 / 260ms），**本行的阶梯就是表里的值** ✓；**为什么「读表」与「行内复述」的区别重要**：**行内复述必然漂** —— 2026-09-23 t224 的行内复述**当场漂了三处**（`.ws-new-btn` 记成 100ms、实际 120ms · `.mem-preview` 当成 120ms 载体、实际**无 transition 声明** · `160ms` 归成 transition、实际是**动画**）⇒ **两处各写一份 = 两处各自漂**（与 §12.10.5 第 6 条「从一处派生」同一规矩 ✓）；**从表派生后，改 §9.1 即改判据，本行一个字不动** ✓。**⚠️ 派生读表的失败形状**：解析不到四档（表被改写 / 锚点丢失）⇒ **报 `not_measured` 并点名**，**不得退回行内常量** ✗。**采样面**：全部 `transition` / `animation` 简写与长写声明（**多值简写按逗号拆成逐属性读数** —— `border-color 120ms, color 120ms, background 120ms` 是 **3 条**，不是 1 条 ✓，2026-09-23 t224 修），**逐条打印** `选择器 · 属性 · 时长 · 是否豁免 · 来源文件:行`；**豁免的判定是结构性的**（`iteration-count: infinite`），**按选择器名豁免一律不接受** ✗（那是「凭名字入场」）。**与行 42 / 行 53 的关系**：**行 42 = 属性写法（`all`）** · **行 53 = reduce 下仍在跑的动效** · **本行 = 时长取值是否在阶梯上** ⇒ 三条对象不同 ✓。**可证伪条件**：① 任一**未豁免**的声明时长 ∉ 阶梯 ⇒ FAIL（**当前 `200ms` 与 `160ms` 就是** ✓）；② 把 `typing-bounce` 的 `infinite` 去掉 ⇒ 它落入对象集且 1.2s ∉ 阶梯 ⇒ FAIL（**证明豁免是结构的、不是名单的** ✓）；③ 声明里解析不出任何时长 ⇒ `not_measured` 并点名。**依据 = 裁决** · **出处 = MASTER §9.1 的四档 token + `docs/research/web-interface-guidelines.md:33`（show-delay ~150–300ms）** ✓ |
| 69 | **过渡 / 动画不得作用于布局属性** | `M1` **GREEN（面板侧 0 处）**：面板声明的过渡属性实测 = `background` · `background-color` · `border-color` · `color` · `opacity` · `transform` · `box-shadow` · `grid-template-rows`（**最后一个具名豁免**，见判定方式）⇒ **布局属性 0 处** ✓。**对象集之外（登记，不判）**：antd 的 `all 0.2s`（`aside.ant-layout-sider`）与 `width 0.3s`（`ul.ant-menu`）—— 它们是 antd 的内部规则，按**行 42 的例外口径**（`owner === antd-cssinjs*` 排除）处理 ✓ | **0 处**（面板声明的过渡 / 动画属性 ∩ {`width`, `height`, `top`, `left`, `right`, `bottom`, `margin*`, `padding*`, `all`} = ∅） | **对象集 = 面板声明的 `transition-property` 与 `@keyframes` 内被动的属性**（作者来源），**排除**：① antd 注入的规则（**与行 42 同一例外口径，三点复述**：(a) 排除的是 antd 的规则、**不是「已合规」** ✓；(b) **来源 `unknown` 不排除**（来源不可判时保守判 FAIL）✓；(c) **项目侧覆盖 antd 的过渡属性是独立决策、不在本判据内** ✓；2026-09-23 t224 复述）② **展开 / 收起的技术手段 —— 从 §9.1 派生，不在本行写属性名**（2026-09-23 t226 修）：**豁免 = 解析 §9.1 `dur-3` 行「用途」列里反引号内的属性名**（当前 = `grid-template-rows`；`.recall-expand` 的 `0fr → 1fr`）⇒ **理由只写在 §9.1 一处**（「结构展开 / 收起」），**本行只读它、不复制它** ✓；**⚠️ 具名豁免本身就是「本判据会误判合法行为」的证据**（t225 指出）⇒ **处置三条**：(a) **能派生就派生**（已做）(b) **必须打印豁免条数与命中项**（当前 **1 处**：`.recall-expand`）(c) **条数与理由写在同一处**（§9.1 的 `dur-3`）⇒ **将来若出现第 2 个豁免，那是「契约该改」的信号，不是往名单里加一行** ✗。**采样面**：逐条声明打印 `选择器 · 属性 · 时长 · 来源`；**关键帧**（`@keyframes`）内被动的属性同样计入（`view-in` 只动 `opacity` / `transform` ✓）。**可证伪条件**：① 面板声明里出现 `width` / `height` / `top` / `left` / `margin` / `padding` / `all` ⇒ FAIL；② **反向证据**：给 `.kanban-card` 加一条 `transition: width 120ms` ⇒ 本行必须 FAIL（**行 42 不会抓它** —— `width` 不是 `all` ✓ ⇒ 两条不重复）；③ 声明集合为空 ⇒ `not_measured`。④ **派生失败**：解析不出 §9.1 `dur-3` 的用途列、而本行**仍 FAIL 某个 `grid-template-rows` 过渡** ⇒ **那是派生读表失败**（必须报 `not_measured`，**不得当成越界** ✗，2026-09-23 t226 补）。**依据 = 裁决** · **出处 = `docs/research/web-interface-guidelines.md:58`（compositor-friendly：`transform` / `opacity`，避免 `width` / `height` / `top` / `left`）与 `:273`** ✓ |
| 70 | **加载态必须有可见指示，且加载完成后消失** | `M2` **GREEN —— 13/13**：把该路由自己的数据端点延迟 **1.5s** 后，13 条路由**全部**有可见加载指示（`Spinner` / `.spinner-block` / antd `Skeleton`），且在加载完成后 **0 个残留** ✓ | **延迟窗口内 ≥1 个可见加载指示 ∧ 加载完成后 = 0** | **对象集** = ① **加载指示**：`.spinner-block` / `.ant-spin` / `.ant-skeleton` / `[role="status"]`（**具名清单，随共享层新增而增列**；**「可见」按附录 A 口径**：rect 宽高 ≥1px ∧ `visibility ≠ hidden` ∧ `display ≠ none` ∧ `opacity > 0`）② **该路由自己的数据端点**（由窗口内正常加载观测到的 `/api` 端点派生，**排除外壳的 `/permissions`** —— 打外壳测到的是白屏，不是加载态 ✗）。**采样面**：13 路由 × {dark, light}；**注入** = `page.route` 把该路由的数据端点延迟 **1500ms**（**窗口必须 ≥ 延迟时长**，否则「指示还没出现」会被误判成缺口 ✗）；**两次读数** = 延迟窗口内（指示数）与加载完成后（残留数）；**逐路由打印** `指示选择器 · 窗口内计数 · 完成后计数`。**为什么窗口内要有 ≥1**：延迟期间**没有任何指示** = 用户面对一个不动的页面 ⇒ 本行最容易退化成「反正加载完就是 0 ⇒ PASS」✗。**可证伪条件**：① 延迟窗口内指示数 = 0 ⇒ FAIL；② 加载完成后残留 > 0 ⇒ FAIL（**加载完了还在转是另一种缺陷** ✓）；③ **反向证据**：注入 CSS 隐藏 `.spinner-block` ⇒ 必须 FAIL。**依据 = 裁决** · **出处 = `docs/research/web-interface-guidelines.md:32`（Loading buttons）与 `:33`（show-delay ~150–300ms / 最短可见 ~300–500ms）** ✓ |
| 71 | **累计布局位移（CLS）** | `M2` **GREEN —— 最大 0.016**：13 路由实测 **`#chat` 0.016**，**8 条路由为 0**，其余 ≤0.016 ⇒ 距阈值 0.1 余量 ≥6× ✓ | **每路由累计布局位移 ≤ 0.1**（= Core Web Vitals 的 good 档） | **对象集** = `PerformanceObserver('layout-shift')` 的 entry，**排除 `hadRecentInput === true`**（用户点击引起的位移不是布局事故 ✓）；**累计窗口 = load 起 2.5s**（与行 28 / 39 的窗口同源口径）；**聚合口径（必须写明，否则同一页两个数）**：**判定值 = 窗口内 Σ `entry.value`**（排除 `hadRecentInput`，**不做 session window 聚合**）；**同时打印 session-windowed CLS**（1s 间隔 / 5s 上限）作为**对照读数** —— 两个数都给，**判定用前者**（与 t211 的实测口径一致 ✓，2026-09-23 t224 修）。**采样面**：13 路由 × {dark, light} × {1440}（**390 / 768 不判** —— 窄屏的折叠 / 抽屉是**用户动作**触发的位移，会被 `hadRecentInput` 排除掉大半，读数不稳定 ⇒ 不冒充已测 ✗）；**逐路由打印** `CLS · 最大单次位移 · 来源元素（前 3）`。**与行 49 的关系（对象不同 ⇒ 不合并）**：**行 49 = 被跟踪行在成员变化前后的 `top` 位移（≤0.5px）** · **本行 = 整页的累计位移分数** ⇒ 行 49 是**行级、单次**的量，本行是**页面级、累计**的量 ✓。**可证伪条件**：① CLS > 0.1 ⇒ FAIL；② **反向证据**：把一块骨架 / 图片去掉固定高度（或把首屏从 `Skeleton` 改成「先空后填」）⇒ CLS 必须越过 0.1；③ **一条 entry 都没有**时**两种可能必须分开**（真的没有位移 / 观察器没装上）⇒ **必须用两样本区分**（注入一个已知位移 ⇒ 观察器必须报数），否则 `not_measured` ✗（**「空集绿」与「已验证的 0」必须分开记** ✓）。**依据 = 惯例阈值**（Core Web Vitals：CLS ≤ 0.1 = good）· **出处 = t211 的 13 路由实测 + `docs/research/web-interface-guidelines.md:81`（Stable skeletons）与 `:140`（Preload fonts）** ✓ |

### 12.1 行 6 的豁免路由与理由（2026-09-21 增补）

行 6 的目标从无条件 `≥3 且 ≤40` 改为**条件规则**：**含读数的页 ≥3；无读数的页 ≥1；上界 ≤40 不变**。

| 豁免路由 | 实测 ≥18px 节点 | 为什么「无读数」 | 门槛 |
|---|---|---|---|
| `#chat` | 空态 2 / 活跃态 1 | 对话页的「大元素」是消息本身与输入器；本页没有可放大的真实数字（本文件记录过「`#chat` 和 `#settings` 都不想要仪表行」） | **≥1** |
| `#settings` | 1 | 一页表单，唯一内容是当前配置；放大任何数字都是装饰 | **≥1** |
| `#inbox` | 空态 1 | 待裁决项数量已在侧栏 `.nav-badge` 与 `.view-bar` 副题出现两次，顶部再加读数是重复 | **≥1**（有 N 条待裁决时自然到 1+N） |

**豁免的三条硬边界（防止豁免变成放任）**

1. **不得为凑数增设装饰性大字**。豁免页若要达到 ≥3，只能靠**真实数据的档位提升**——`view-task-detail.md` §3、`view-settings.md` §3、`view-inbox.md` §3 各给了一条这样的路径（升的是已存在的真实字段，不是装饰）。
2. **层级中心不能丢**：豁免页仍必须恰好 1 个 **20px 的页面标题**（`.view-bar h2`，title 档 20/26）作为层级中心；`#home` 用 `.home-hero h1`（**24/30**，§4.1 的 display 档；本句 2026-09-21 前误写为 32px，见 §12.4）。**这是豁免的交换条件**：不硬凑读数，但必须有一个明确的层级中心。
3. **上界与其余行全部不变**：≤40 依旧；行 7（最大字号 ≥20px）、行 24（恰好 1 个 `h1`）、行 15/16（焦点环）不因豁免放宽。

**为什么不是把 ≥3 直接降成 ≥1**：那会让 10 条有读数的路由一起失去约束。条件规则把「有可放大数据的页」与「没有可放大数据的页」分开，前者保持 ≥3，后者要求「恰好一个层级中心 + 不造假」。

### 12.2 与 primitives.md §11 的对齐（行 16 与行 23 的判据以 §11 为准）

**行 16**：原判据「`outline-style: none` 且 `box-shadow: none` 即判无环」会**虚假通过**——`.readout-btn` / `.ant-segmented-item-input` 落到 UA 的 `outline-style: auto`（非 `none`），会被记成「有环」。改为 `outlineStyle === 'solid' && outlineWidth ≥ 2px` 才算有环（`auto` 判 fail）。

**行 23**：原判据用全站 `querySelectorAll('[aria-expanded]')` 计数，而 antd `Select` 自身就产出 `aria-expanded`（实测 `#chat` 3 / `#task` 3 / `#memory` 1 / `#settings` 1）→ 原基线「全站 0」与目标「≥ 展开控件数」会**同时失真**（0 ≥ 0 是虚假通过）。改为只数三个手写展开控件的覆盖率。

**权威来源**：这两行的判据定义在 `docs/design/primitives.md` §11（行 16 / 行 23）与 §5.1 / §5.2.2，**与本节不一致时以 §11 为准**；本节只保留必要复述，避免两处漂移。行 20 的三态判据同理引用 primitives.md §3.5。

### 12.3 行 32 的判据修正：`--brand` 读作**运行时品牌族**（2026-09-21 增补）

**冲突（t2 验收暴露）**：行 32 原文把唯一性检查集写成「`--signal` / `--brand` / `--status-warn` 三者互不相同」。审计工具把 `--brand` 读成 antd 派生的 accent（`--ant-color-primary`）——而 accent 与 `--signal` **就是同一个值**：

| 令牌 | 暗 | 亮 | 出处 |
|---|---|---|---|
| `--signal`（填充档） | `#f0a93b` | `#c88413` | `index.css:56` / `:111` |
| `--ant-color-primary`（accent） | `#f0a93b` | `#c88413` | `theme.tsx:54` `D.brand` / `:80` `L.brand` → 经 §2.6 的 `colorPrimary` 落到 CSS 变量 |
| `--status-warn` | `#e2894f` | `#8a4a12` | `index.css:62` / `:117`（C2 落地后的值） |

→ 该行在**原读法下必然报 1 组重复、永不可能 PASS**；而它与 primitives §2.4 C6 / §2.6 **明文要求** `L.brand === --signal` 直接打架。这是**契约内部自相矛盾**，不是实现缺陷。

**裁决（captain 2026-09-21 定方向，本文落槌）**

1. **唯一性检查改读运行时品牌族**，即 §3.1 I 定义的**域②**：`--brand-claude` / `--brand-deepseek` / `--brand-opencode`。检查集从「3 个、含 accent」改为 **5 个纯语义令牌**：
   `{--signal, --status-warn, --brand-claude, --brand-deepseek, --brand-opencode}`。
2. **accent 与 `--signal` 同值是刻意设计，显式排除在唯一性检查之外。**

**为什么 accent 必须等于 `--signal`（单一信号色通道）**

行 32 要守护的是「**一个颜色一个含义**」。accent 与 `--signal` 的关系不是「两个含义撞车」，而是**同一个含义的两条渲染路径**：

* 全站的「现在 / 运行中 / 等你处理」只有一个色相——这是 §3.2「信号色唯一含义」的**前提**，也是 P2 的落地方式；
* 这个含义要落到三类载体上：手写 CSS（`--signal`）、antd 组件的主色（`--ant-color-primary`）、文本档（`--signal-text`）。**若让 accent 取一个与 `--signal` 不同的值，主按钮 / `Select` 选中态 / `Progress` 就会变成站内第二个「现在」色**——信号色当场裂成两个色相，那才是真正的语义稀释；
* primitives §2.4 C6 与 §2.6 因此**明文要求** `L.brand` → `#c88413`（= 亮色 `--signal` 的填充档），这不是巧合，是同一决定的两处表述；
* 所以 **accent === `--signal` 是结构性同值**（一条通道），而 **`--status-warn` === `--signal` 是语义性同值**（两个含义共用一个色）。前者保留，后者由 C2 消除。行 32 只判后者 + 品牌族，不判前者。

**排除的判定形式（机器可读）**

* 检查集**不含** `--brand`，也**不含** `--ant-color-primary` / `--ant-color-primary-hover` / `--ant-color-primary-active` / `--ant-color-link`——它们都是 accent 通道的派生档，与 `--signal` / `--signal-text` 同源；
* 若某个令牌**既不是 5 个检查集成员、也不在上述 accent 通道白名单内**，却与 `--signal` 同值 → **仍判失败**。排除是**具名白名单**，不是「凡撞车就放过」。

**改完后行 32 为什么能 PASS（复算，不必真跑）**

| 模式 | `--signal` | `--status-warn` | `--brand-claude` | `--brand-deepseek` | `--brand-opencode` |
|---|---|---|---|---|---|
| 暗 | `#f0a93b` | `#e2894f` | `#d97757` | `#5786fe` | `#e8e8ea` |
| 亮 | `#c88413` | `#8a4a12` | `#d97757` | `#5786fe` | `#121212` |

（出处：`index.css:56/62/76/77/78` 暗、`:111/117/122/123/124` 亮；`--signal` 与 `theme.tsx` 的 `D.brand`/`L.brand` 同值。）

1. 两模式下各 5 个值**两两互不相同**（字符串比较，每模式 10 组）→ 重复组数 = **0** ✔
2. `--status-warn` vs `--signal` 的 Δhue：暗 **12.8°** / 亮 **9.4°**，均 ≥ 8° ✔（primitives §2.3）
3. 被排除的 accent 两值分别等于同模式的 `--signal`（刻意），**不计入** → 不产生重复组
4. 品牌族三值在两模式共 **4 个不同取值**（`#d97757` / `#5786fe` 两模式相同，`--brand-opencode` 暗 `#e8e8ea` / 亮 `#121212`），与 `--signal` / `--status-warn` 的 4 个取值**无交集** ✔

→ 判据在「accent === signal」前提下**可 PASS**。

**复算脚本（本次实跑，输出逐字粘贴）**

```bash
python - <<'PY'
def hue(h):
    h=h.lstrip('#'); r,g,b=[int(h[i:i+2],16) for i in (0,2,4)]
    mx,mn=max(r,g,b),min(r,g,b)
    if mx==mn: return None
    H = 60*(((g-b)/(mx-mn))%6) if mx==r else 60*((b-r)/(mx-mn)+2) if mx==g else 60*((r-g)/(mx-mn)+4)
    return (H+360)%360
for mode,s in {
 'dark' :{'--signal':'#f0a93b','--status-warn':'#e2894f','--brand-claude':'#d97757','--brand-deepseek':'#5786fe','--brand-opencode':'#e8e8ea'},
 'light':{'--signal':'#c88413','--status-warn':'#8a4a12','--brand-claude':'#d97757','--brand-deepseek':'#5786fe','--brand-opencode':'#121212'},
}.items():
    v=list(s.values()); n=list(s.keys())
    print(mode,'dups=',sum(1 for i in range(5) for j in range(i+1,5) if v[i]==v[j]),
          'dHue=%.2f'%abs(hue(s['--signal'])-hue(s['--status-warn'])))
PY
```

```console
dark dups= 0 dHue=12.79
light dups= 0 dHue=9.46
```

**下游必须同步的两处（各自归属，不在本文范围）**

1. `panel/tools/design-audit.mjs` 第 32 行的 `entries` 现读 `["--brand(antd primary)", g["--ant-color-primary"]]` → 改为读 `--brand-claude` / `--brand-deepseek` / `--brand-opencode`，并把这三个变量加进采集列表。**归属 `tools`**。
2. `docs/design/primitives.md` §2.3 / §2.4 C6 / §2.6 的措辞需同步——见下。**归属 `systems`，由 captain 转达**。

**primitives 措辞修改建议（t14 提出，请 captain 转 `systems`）**

| 位置 | 现文 | 建议改为 |
|---|---|---|
| §2.3 开头「规则（可机器判定，阈值行 32）」 | 「`--signal` / `--brand` / `--status-warn` 三者**字符串互不相同**」 | 「**运行时品牌族** `--brand-claude` / `--brand-deepseek` / `--brand-opencode` 与 `--signal` / `--status-warn` **两两互不相同**（检查集 5 个）；`--status-warn` 与 `--signal` 每模式 **Δhue ≥ 8°**。**accent（`--brand` / `--ant-color-primary`）不在检查集内**——它 `=== --signal` 是刻意设计（单一信号色通道，见 §2.4 C6），显式排除。判据落槌处：MASTER §12.3」 |
| §2.4 C6「信号拆档」段末（「**副作用（登记并接受）**：亮色主按钮"字/底"从 7.70 → **5.64:1**」之后） | —（无此句） | 追加：「**同源声明**：亮色 `L.brand`（`#c88413`）与 `--signal`（`#c88413`）**刻意同值**——accent 是 `--signal` 的 antd 渲染路径，不是第二个信号色。因此阈值行 32 的唯一性检查**只读运行时品牌族**（`--brand-*`），accent 显式排除（MASTER §12.3）」 |
| §2.6 表 `colorPrimary` 行的「依据」列 | 「C6」 | 「C6；**与 `--signal` 同值是契约要求**，MASTER §12.3 已把 accent 排除出行 32 的唯一性检查」 |

（本文只提出措辞建议，**不改 primitives.md**——它不在本任务 inScope。）

### 12.4 行 6 括注的 `#home` 标题尺寸修正：32px → 24/30（2026-09-21 增补）

**冲突（`t17` 审计工具暴露）**：行 6 的判据括注原文写「`#home` 用 32px 的 `.home-hero h1`」，而 §4.1 的字号阶梯把**同一个元素**明确列在 **display 档 24/30**：

| 位置 | 原文 | 该元素的档位 |
|---|---|---|
| §4.1 字号阶梯（本文件 行 335） | 档位列写 `24 / 30`；用途列写明「`#home` hero h1、`readout.l`」 | **24/30** |
| §4.1 T5 行高越界表（本文件 行 380） | `24/32`（`.home-hero h1`）→ 归一到 `24/30` | **24/30** |

`#home` 实测的 32px 来自 **`.readout` 读数档**（4 个读数），**不是**这个 `h1`：

```console
$ cd panel && node tools/design-audit.mjs --json --routes=home --out=<临时目录>   # 工具 09-21_23:43:04
# capture home/dark 的 sizeHist 与 ge18Samples：
sizeHist = [11×33, 12×11, 13×7, 14×27, 15×5, 18×4, 24×1, 32×4]
ge18Samples: .home-hero h1 = 24px
             .readout-btn > span.readout = 32px ×4
             .stat-cell > span.stat-num = 18px ×4
```

→ **24px 只有 1 个（那个 `h1`）、32px 有 4 个（读数）**，与 §4.1 完全一致。

**后果**：行 6 的「页面标题」子断言对 `#home` **假 FAIL** —— 判据要求「恰好 1 个 ≥32px 的 `.home-hero h1`」，而该 `h1` 按 §4.1 只能是 24px；要满足旧行文就得把 `h1` 抬到 32px，那会同时击穿 §4.1（display 档的唯一值是 24/30）并让 `#home` 出现 5 个 32px 节点，层级中心从「欢迎回来」漂到读数上。

**裁决（captain 2026-09-21）**：**把契约改到与实测一致**，不是让工具改回 32px。行 6 与 §12.1 边界 2 的括注均已改为「`#home` 的层级中心是 `.home-hero h1`，尺寸按 §4.1 的 display 档 24/30」。`t17` 已按 §4.1 实现，并在行 6 的 note 里登记该冲突。

**这是本次的第三处「契约内部矛盾」**（前两处：行 32 的 accent 读法 §12.3、行 6 上界被解析成豁免门槛）——共同特征都是**判据文里写死了一个与阶梯表/豁免表不一致的数字**。后续凡在判据里写具体数值，必须同时指向它所在的阶梯表行号。

### 12.5 行 1 的口径收窄：只算「被设计的面」（2026-09-21 追认）

**裁决（captain 2026-09-21，本文追认）**：行 1 的目标适用于**被设计的面**——面板 / 卡片 / 空态 / 侧栏；
**不适用于应用画布本身**。工具**同时报告「设计面」与「含画布」两个数字**，原值不隐藏、不删除。
`panel/tools/design-audit.mjs` 已按此实现（判据文本逐字引用本裁决；`lumaBandRatioDesigned` 参与判定，`lumaBandRatio` 并列输出）。

**为什么这不是「为了让数字变绿而改口径」——三条硬证据**

**证据 1：画布本身落在带内，空态页几乎全是画布。**
画布 token `D.pageCanvas = "#0f1014"`（`theme.tsx:42`；亮色 `:68` 为 `#f6f7f9`，经 `colorBgBase`/`colorBgLayout` 落地，`theme.tsx:136/139`）。
按行 1 的公式 `luma = (0.2126R+0.7152G+0.0722B)/255`：
`(0.2126·15 + 0.7152·16 + 0.0722·20)/255 = 16.076/255 = **0.0630**` → **落在 `[0.05,0.10)` 带内**。
实测 `#inbox` 空态：画布像素占 **83.41%**，行 1 因此报 **83.46%**。
于是行 1 在空页面上度量的不是「面有多浑浊」，而是「**页面有多空**」——空页永远比满页「更糟」。

**证据 2：靠调画布本身解决不了，两个方向都撞墙。**
把带内占比从 83.46% 压到 ≤75%，必须移出 **109,642 px**；而规格给空态的唯一补救物 `.empty-mark` 实测只有
**48×48 = 2,304 px** → **差 47.6 倍**。把画布挪出带内也不行，因为它被**行 29（材质台阶 ≥2.4×）**与**外壳→画布**两头夹住：

| 方向 | 脱离的带界 | 画布必须到达 | 复算的后果（行 29 口径：`L = 0.2126R'+0.7152G'+0.0722B'`，sRGB 反伽马） |
|---|---|---|---|
| **升画布** | 上界 `luma ≥ 0.10` → sRGB ≈ **27** | `L ≈ 0.0109` | canvas→panel 比 = `L(panel)/L(canvas) = 0.012880/0.010915` = **1.18**（现值 **2.47**）→ **行 29 直接失败** |
| **降画布** | 下界 `luma < 0.05` → sRGB ≈ **11** | `L ≈ 0.00336` | 外壳→画布比 = `0.00336/L(--app-shell #08090b) = 0.00336/0.002710` = **1.24** → 外壳与画布几乎同色，侧栏边界消失 |

现值复算：`L(#0f1014) = 0.2126·0.004773 + 0.7152·0.005179 + 0.0722·0.006987 = 0.005223`；
`L(#1b1e24) = 0.012880` → 比 = `0.012880/0.005223` = **2.466 ≈ 2.47**（行 29 基线列记 2.48，同源）；
`L(#08090b) = 0.002710`（`theme.tsx:41 appShell` / `index.css:45 --app-shell`）。

→ **这不是调色板能吸收的问题，而是指标范围问题**：画布是「舞台」，不是「舞台上的东西」；
用同一个阈值同时约束舞台和台上之物，必然把空页面判成缺陷。

**证据 3：明确否决「放宽 `.empty-state` 增加带外像素」。**
把空态卡从 520px 放宽到内容区全宽 1212px 可新增 `(1212−520)×296 = 204,832 px` 带外面板，
足以把 `#inbox` 的带内占比从 83.41% 降到约 **67.6%** —— 数字会变绿。
**但那是为了让指标好看而往页面上加像素**，与行 1 要度量的东西无关，也会让空态页多出一块没有信息的表面。
**否决**（captain 裁决）。注：`t12` 报告里我提过这条「可达路径」，本节的裁决把它关掉 —— **可达 ≠ 应该做**。

**收窄后的判定式（机器可读）**

```
lumaBandRatioDesigned = #{ 设计面像素 : 0.05 ≤ luma < 0.10 } / #{ 设计面像素 }
设计面像素 = 截图全部像素 − 应用画布像素（按画布 token 精确匹配）
判定：lumaBandRatioDesigned ≤ 0.75；同时输出 lumaBandRatio（含画布）作为对照，不参与判定
```

**口径变更的下游影响**：行 1 的基线列（`M1` 13.7–83.5%）**保持原样** —— 它是「含画布」口径的历史实测值，
不因本次收窄而改写；新口径下的基线由工具重跑给出。

### 12.6 行 37 的残余裁决：口径收窄到「设计者写的值」+ 具名出血闭集（2026-09-21 裁决）

**背景（systems 实测）**：`t18` 已把 43 处手写声明 + 4 个 antd token 归一到阶梯，每模式越界从 636 降到约 150（−76%）。
残余已全部归因：负向出血、antd 派生值、`auto` 解析值、UA 默认。

**裁决：采纳 C 的口径（只判设计者写的值）+ A 的闭集（负向出血具名白名单），否决 B。**

1. **为什么是 C 而不是 B（口径 vs 例外表）**：行 37 的目的是「让**手写的**间距落在阶梯上」（§5.1 原文）。
   `getComputedStyle` 返回的是**层叠结果**，不是「谁写的」——所以现行判据把三类**设计者根本没写**的值算成了越界：
   antd CSS-in-JS 注入的（实测 `#task` 的 `.ant-select-outlined` `padding-right: 11px` ×6、`.ant-layout-sider-children` 的 `0.1px`）、
   UA 默认的（`p` 的 `margin: 1em` → 14px）。**B 是给这些值编一张例外表**：antd 一升级表就烂，且它只覆盖 antd、覆盖不到 UA 默认。
   **C 从「谁能写这个值」上分类**，不随 antd 版本漂移 → **语义正确**。
2. **为什么 C 还不够，必须加 A**：实测（13 路由 × 2 模式）——只做 C，分子从 **268 降到 162**，其中 **154** 是负向出血，
   **行 37 仍不可能为 0**。原因是**结构性的**：阶梯是**正值集合**，负值**永远不在阶梯里** ——
   出血是「光学修正」，不是「间距步长」。所以 A 不是放水，而是**承认负值属于另一个判据域**。
3. **为什么 A 必须是「具名闭集」而不是「任意负值白名单」**：若写成「`{-4,-8,-12}` 一律豁免」，
   任何元素都能用 `-12` 把 hover 面撑到行外，行 37 就失去意义。裁决采纳**凭票入场**：
   负值只允许 `{-4, -8, -12}`，**且必须落在 primitives 点名的载体上**，每个载体都有具名规则：

| 值 | 载体 | 出处（具名规则） | 理由 |
|---|---|---|---|
| `-12` | `.row-btn` | `index.css:491`；**primitives §3.3 C1 明文**「`padding 9px 12px`、`margin 0 -12px`」 | 行按钮的 hover/selected 面出血到卡片边，文字仍对齐内容列 |
| `-8` | `.mem-preview` | `index.css:1091`（**代码注释逐字写明**：`-8px` 是它自己 8px padding 的负数） | 同上，C6 载体 |
| `-4` | `.intent` | `index.css:369` `margin: -4px 0 16px` | 标题下说明段的光学上提 |

未点名元素上的任何负值、或白名单外的负值（`-1` / `-2` / `-6` …）→ **仍计越界**。

**新判据（机器可读，含算式）**

```
分子 = #{ 正值 off-ladder 的作者值 }
     + #{ 负值 : 值 ∉ {-4,-8,-12}  ∨  元素 ∉ 出血载体白名单 }
「作者值」= 获胜声明来自 index.css（构建产物 <link>）或元素的 style 属性
排除并单独计数（作为对照列，不参与判定）：
  antdDerived   ← antd CSS-in-JS（<style data-css-hash>）注入的值
  uaDefault     ← 无任何声明命中该属性（UA 默认，如 p 的 margin: 1em）
  autoResolved  ← 指定值为 auto/normal（flex 自动外边距会被解析成 px）
  clampVw       ← clamp()/vw 产出（≥2 位小数，§5.1 明文保留）
判定：分子 == 0
```

**算式与实测（我本轮跑的探针；13 路由 × 2 模式，1440×900）**

| 量 | 次数 | 说明 |
|---|---|---|
| 现行判据的分子（raw） | **268** | = **134/模式**（captain 记 ~150/模式；差异来自路由集与 settle 条件） |
| − 非作者来源（antd CSS-in-JS + UA 默认） | **106** | C 口径排除（= 53/模式） |
| − 指定值为 `auto` | **6** | `.agent-stats { margin-top: auto }` 等 6 条声明解析成 21px / 42px（= 3/模式） |
| − 具名载体上的负向出血 | **154** | A 闭集排除：`-12`×**140** / `-8`×**12** / `-4`×**2**（= 77/模式，其中 `-12` 70/模式） |
| **= 新判据的分子** | **2** | **1 个元素 × 2 模式**：`Runtimes.tsx:222` 的 `<p className="muted pad" style={{ marginTop: 14 }}>` —— 内联 14px 不在阶梯上 |

→ 新口径下 13 路由 × 2 模式的越界 = **2**，且这 2 次是**真越界**（视图内联写死 14px，应为 12 或 16）。
**这条残差本身就是新口径有效的证明**：它没有把真问题一起洗掉。

**`-12px` 在新口径下的归属（逐字回答）**：**保留、不删除、不计入分子**。
依据是 primitives §3.3 **C1 明文**（`.row-btn` 的 `margin: 0 -12px` 是契约要求的取值）；
本裁决只把它从「间距阶梯」搬到「出血闭集」这个判据域，**取值与规则一字未改**。
`systems` 在 `t18` 里「已按契约保留未删」的做法正确。

**登记为下游跟进（不在本文范围）**：`Runtimes.tsx:222` 的内联 `marginTop: 14` → 12 或 16（视图侧一行改动）。
**B 被否决的记录**：若将来 antd 派生值确需豁免，必须走「具名例外 + 版本号」的登记，不得引入无主例外表。

### 12.7 两项视觉裁决：行 30 的 ring 基准与令牌、行 18 的 `.ant-switch`（2026-09-21）

#### 12.7.1 行 30：基准统一到画布，**改令牌**而不是降目标

**冲突**：行 30 的**判据**列写「与 `pageCanvas` 比」，而**基线**列的 1.22 与**目标** 1.2 是在**白底**上量的。
`t23` 实测 13 条亮色路由的 ring 都是 `#e7e9ed` 1px：**对 canvas 1.1340 / 对 panel 白 1.2155**。
→ 在判据自己写的基准上，现值 1.1340 < 1.2 → FAIL。**这是第四次同类问题**（行 32 / 行 6 / 行 1 / 行 30）：
**阈值在一个基准上标定、却在另一个基准上度量**。

**基准该取谁：画布。** `--panel-shadow`（亮）= `0 0 0 1px var(--rule-soft), 0 1px 2px …` —— `0 0 0 1px` 是
**向外**扩 1px 的 shadow，ring 画在 panel 盒子**之外**，外缘贴的就是 `pageCanvas`；
它的职责正是「把 panel 的边缘从画布上分出来」（primitives §2.1 C1：亮色 `raised` 由 ring 承载）。

**裁决：目标 1.2 保持不动，改令牌。**

1. **1.2 不是拟合现值出来的数**，它是本系统**发丝线族的下限**：§11.1 记「装饰性分隔线 1.22–1.49（合规）」、
   §2.4 记 `--rule`（亮）对白 **1.41** / `--rule-soft`（亮）对白 **1.22**（**改值前**；改值后为 1.3217，见 §2.4）—— 系统里每条发丝线都在 1.22 以上。
   若允许 panel 的 ring 在它自己的基准上停在 1.1340，**panel 的边会成为全系统最弱的一条发丝线**，
   比它旁边任何一条分隔线都弱。这是可验证的不一致，不是审美。
2. **降目标 = 拟合观测值**：1.1340 只是「在白底标定的目标被搬到画布上量」的结果，不是一个设计决定；
   把它写成新目标，等于把「阈值跟着观测走」制度化 —— 与 §12.5 拒绝「放宽 `.empty-state` 增加带外像素」同一条原则。
3. **代价可算、方向正确**：`--rule-soft`（亮）同时承担 ring 与 7 处分隔线；压暗一档会让**整族一起变强**
   （从「ring 1.13 / 分隔线 1.22」到「ring 1.23 / 分隔线 1.32」）——**族内一致性变好**，不是局部改观。

**算式**（复现口径与 §2.4/§11.1 一致：`L = 0.2126R'+0.7152G'+0.0722B'`，sRGB 反伽马；`cr = (L_hi+0.05)/(L_lo+0.05)`）

```
L(pageCanvas #f6f7f9) = 0.929539      L(panel #ffffff) = 1.000000
现值 #e7e9ed：L = 0.813812 → 对 canvas 1.1340 ✗ / 对 panel 1.2155
要求：cr(ring, canvas) ≥ 1.2  ⇒  L(ring) ≤ (0.929539 + 0.05)/1.2 − 0.05 = 0.766283
取值 #dee0e4：L = 0.744423 → 对 canvas 1.2330 ✔（余量 +2.75%）/ 对 panel 1.3217 ✔
取法：#e7e9ed × 0.9615，保持同一色相比例 231:233:237 → 222:224:228（不引入新色相）
```

（校核：同一算式对 `L.border #d6d9df` 给出对白 **1.4141**，与 §2.4 记的 **1.41** 一致 —— 公式口径已对齐。）

**下游改动（captain 转 `systems`，`panel/theme.tsx` 一行）**：`L.borderSecondary`：`#e7e9ed` → **`#dee0e4`**。

**影响面（必须写明）**：这是**亮色模式下所有卡片/面板边缘**的观感变化，**不是局部改动** ——
`--panel-shadow`（亮）被 `.panel` / `.card` / `.kanban-card` / `.tool-detail` / `.prompt-view` 等所有材质面使用；
`--rule-soft` 另承担 `.view-bar` / `.zone-head` 等 **7 处结构性分隔线**。变暗一档后：材质面的边更清楚（这是目的），
7 处分隔线同步变强（仍在「装饰级、不受 3:1 约束」档内，且更接近 `--rule` 的 1.41）。
**暗色模式不受影响**（暗色 `--rule-soft` 是 `rgba(255,255,255,.07)`，本次不动）。

#### 12.7.2 行 18：`.ant-switch` 的 21px —— 裁决**改**（pin 三个派生 token）

**事实（`t23`/`t28` 实测）**：`#settings` 的 `button.ant-switch` = **42×21**，手柄 **17×17**。
这是 **antd 6.6.3 的派生值**，不是任何人手写的：`prepareComponentToken` 里
`trackHeight = fontSize × lineHeight`、`handleSize = trackHeight − 2×2`、`trackMinWidth = 2 × trackHeight`。
本应用 `theme.tsx:99 fontSize: 14` + `:108 lineHeight: 1.5` ⇒ **21 / 17 / 42** —— 与实测三个数字**逐个吻合**。
即：**开关的高度被文本行高意外决定了**（改全局 `lineHeight` 不可行 —— 那会动所有文本）。

**WCAG 2.5.8 的间距例外是满足的（实测，供记录）**：把 24px 直径圆居中在开关上，**不与任何其它命中目标相交**；
最近目标（另一个开关）**49px** 远。所以现状**不违反 WCAG AA** —— 本行的 24px 地板**本来就是刻意严于 WCAG 的设计规则**。

**裁决：改。** 理由：

1. **同一份契约已有先例**：primitives §3.3 **C8** 明文把 `.ws-new-btn` 从 `22×22 → ≥24×24`，理由就是 §5.4 的命中区下限。
   同一个系统里 22px 的**手写**控件已被裁决要升到 24，21px 的 **antd** 控件没有理由例外 —— 否则就是双标。
2. **用 WCAG 的例外豁免本行 = 抹掉本行的增量**：WCAG 允许隔离的小目标，本行不允许。本行的价值就在于比 WCAG 严。
3. **代价可量化，且比预想的小 —— 布局零影响**：实测开关所在 `.row` 高 **54px**（padding `6px 4px`），
   行内最高元素是**文本块 42px**，开关只有 21px；升到 24px 仍 < 42px ⇒ **行高不变、页面高度不变**。
   唯一可见代价在控件自身：轨道 21→24、手柄 17→20（线性 +18%）。手柄变大**与系统自身的命中区哲学一致**
   （`.icon-btn` min 24×24、`.ws-new-btn` ≥24×24 都为此把控件做大过）。
4. **若确实要留例外，正确的类别是**「**WCAG 2.5.8 spacing 例外已满足的隔离控件**」，且必须写成**条件式**
   （与最近目标的间距一旦 <24px 即失效；本轮实测 49px 满足）。**但本裁决不采纳**：例外名额应留给真正改不动的
   （如 UA 内建 `<select>` 的下拉箭头区域），而不是留给一个改三个 token 就能达标的控件。

**确切改动（captain 转 `systems`，`panel/theme.tsx` 的 `components.Switch`，三个 token 必须同时给）**

```ts
components: {
  Switch: {
    trackHeight: 24,     // 21 → 24（= 命中区地板）
    handleSize: 20,      // 17 → 20（= trackHeight − 2×trackPadding，trackPadding 固定为 2）
    trackMinWidth: 48,   // 42 → 48（= 2 × trackHeight）
  },
}
```

**为什么必须给三个、不能只给 `trackHeight`**：`handleSize` 与 `trackMinWidth` 是 antd 在 `prepareComponentToken` 里
**由 trackHeight 派生**的。只覆盖 `trackHeight` 会得到「24 的轨道 + 17 的手柄 + 42 的最小宽」——
手柄与轨道比例失衡，且 antd 内部的 `innerMinMargin` / `innerMaxMargin`（= `handleSize/2` 等）仍按 17 计算，**滑块行程会错位**。
三个一起给才保持 antd 自身的比例关系（padding 固定 2、handle = track − 4、minWidth = 2 × track）。

### 12.8 行 28 的门槛 ≤3 → ≤7：判据与它自开的药方不相容（2026-09-21 落槌）

**冲突**：行 28 要求「11.5s 窗口内 API 请求数 **≤3**」，而**同一行来源规格 view-inbox I5 自开的药方**是
「合并为 **1 个**、**2s**、可见性感知的轮询」。**1 个 2s 轮询在 11.5s 窗口内必然发 7 次**
（实测节奏 **1993 / 2007 / 2002 / 1995 / 2012 / 1999 ms** = 6 个间隔 + 首次请求）。
于是：**这一行即使把重复轮询彻底修完，也永不可能 PASS** —— FAIL 是**判据的性质**，不是面板的性质。
（与 §12.3 的行 32 同类：契约同时要求两件互斥的事，该行恒假。）

**算式（两种读法都算一遍）**

```
1 个 2s 轮询在 11.5s 窗口内的请求数 = ceil(11.5 / 2) 次间隔 + 首次请求
                                  = 6 + 1 = 7
   （用 ceil 而非 floor：实测节奏 1993–2012ms 略快于名义 2s，最后一次落在窗口边缘内）
要压到「≤3 次请求」：间隔须 ≥ 11.5 / (3 − 1) = 5.75s
若把门槛理解为「≤3 个间隔」（= 4 次请求）：间隔须 ≥ 11.5 / 3 = 3.83s
→ 两种读法下 2s 都不满足 ⇒ 该行在 I5 自己的药方下恒假
```

**裁决：门槛由 ≤3 改为 ≤7**，算式写进判据；**2s 间隔不动**。

1. **7 是从契约自己的药方推出来的，不是从现状量出来的** —— 这一点决定它不是「放水」：
   现状实测 **14**（= 2 个轮询器叠加），**仍 > 7 ⇒ 该行现在依然 FAIL**，且 FAIL 的原因正是它要抓的东西。
   门槛 7 = 「按 I5 的药方实现后应有的请求数」，所以 ≤7 的语义是「**没有多余的轮询器**」，不是「多少都行」。
2. **区分度仍在，而且是二值的**：1 个轮询器 = **7** ✔ / 2 个 = **14** ✘（实测）；3 个 = 21 同样会被抓住。
   中间没有灰带 —— 这正是「无重复轮询」这条检验需要的分辨率。
3. **不动 2s 的理由**：2s 是**产品行为**（运行被阻塞时用户多久看到新状态），不是为通过阈值而选的参数。
   为迁就一个门槛把轮询放慢到 3.83s，等于**拿产品行为换指标** —— 与 §12.5 拒绝「放宽 `.empty-state` 增加带外像素」同一条原则：
   **该改的是判据，不是被度量的对象**。
4. **为什么不改成「间隔 ≥2s 且无重复轮询」**：那是把可判定的**计数**换成需要读代码的**定性判断**。
   请求数在 CDP 层是**可观测的整数**；间隔要推断（还要处理可见性暂停与后台节流），判据会变脆。
   保持计数口径，只修正门槛。

**登记为第六处契约内部矛盾**（枚举口径与 §12.4 一致 —— 那里把「行 6 上界被解析成豁免门槛」另计为**工具侧根因**，不计入本表）：

| # | 矛盾 | 落槌处 |
|---|---|---|
| ① | 行 32 的 `--brand` 读法（accent vs 运行时品牌族） | §12.3 |
| ② | 行 6 括注 32px 与 §4.1 的 24/30 | §12.4 |
| ③ | 行 1 阈值在「含画布 / 全画面」标定、却在「设计面」度量 | §12.5 |
| ④ | 行 37「⊆ 阶梯」对负值结构性不可满足（阶梯是正值集合） | §12.6 |
| ⑤ | 行 30 阈值在白底标定、却在画布度量 | §12.7.1 |
| ⑥ | **行 28 的门槛 ≤3 与 I5 自开的「1 个 2s 轮询」药方不相容** | **§12.8（本处）** |

**工具侧联动**：`panel/tools/design-audit.mjs` 的门槛**从 MASTER 读**（`targetFor()` 取行 28 **目标列**的第一个数字）。
落槌前它按 `PENDING_RULINGS[28].cap = 7` 兜底并在 note 里标「待落槌」；**本文落槌后 `numAt(t, 0) = 7 ≥ cap` → 自动切换为
「门槛 7 读自 MASTER §12 行 28」**，该行由 **pending 转为正常判定**。自检已把两种状态都钉死
（`--self-test` 中 `r28row.parse({ text: "≤3", nums: [3] })` 与 `{ text: "≤7", nums: [7] }` 两条断言）。

### 12.9 行 18 的 <32px 预算与「外壳常数」：外壳不计入每路由、改用具名清单单查一次（2026-09-21 裁决）

**问题（结构性的，不是某一页的缺陷）**：预算写「<32px 每路由 ≤10」，但**外壳在每一条路由上都贡献同一个常数** ——
**裁决时（2026-09-21）实测 4 个 <32px 控件**（`button.kbd-hint` **26×45** + 3 个 `button.ant-btn` **32/32/36 × 24**，全在 `.sidebar-foot`）；
**t36 之后为 1 项**（`kbd-hint` 26×45 —— 那 3 个按钮被 `controlHeightSM: 32` 抬到 32，**退出了集合**，见 ②/⑤）。
外壳是 13 条路由共享的**同一份 DOM**，逐路由计入等于把**同一个常数重复算 13 次**：
预算被固定吃掉 4（t36 后 1），每路由只剩 6（t36 后 9）个名额，而这个常数不携带任何关于该页设计的信息。
**这与 §12.5 排除画布是同一结构**：与页面设计无关的常数贡献，不该进逐页指标。

**裁决：采纳 (a)** —— 外壳**不计入**每路由预算，改为**独立的、只查一次的具名清单检查**。
外壳因此**仍被检验**，只是不再被重复计数。

**① 「外壳常数」的判定边界（机器可判定，不留解释空间）**

```
外壳 = aside.ant-layout-sider.app-sider 子树（侧栏，含 .sidebar-foot）
     + 挂到 body 下的全局 portal 宿主（模态/浮层；默认关闭时不渲染，故通常为空）
内容区 = main.ant-layout-content.content 子树 —— 含 .view-bar
     （.view-bar 逐路由不同：它承载路由标题与页级动作，属内容区，不是外壳）
其他（other）= 两者都不在的交互元素 → 计为**内容区**（宁可多算，不可漏算）
```

**「常数」这个前提本身也要被检验**：工具必须逐路由比对**外壳 <32px 清单**（**选择器路径 + className + 几何**三者相同），
**13 条路由逐条一致才成立**。任何一条路由上不同 ⇒ 该元素**不是**外壳常数 ⇒ **按内容区计入**（防止用「外壳」当豁免口袋）。

**② 外壳的控件在何处被独立检验（具名闭集：2026-09-21 t36 后为 **1** 项）**

作为**行 18 的第三个子断言**（与 <24px 地板、<32px 预算并列），**只在一次 capture 上判定**：

```
外壳清单 = 具名闭集，**2026-09-23 起为空（0 项）**：
  （无成员）
  旧成员（**症状，不是设计常数** —— 见 §12.12 的 ③ 类）：
  button.kbd-hint                        26×45
      实测 26×45 = **3 行折行**（line-height 15 × 3）：.kbd-hint 是 .sidebar-foot 这个 flex 行的第 4 个子项，
      white-space: normal + flex-shrink: 1 ⇒ 被压缩到 26px 宽，文本「Ctrl K ⌘K」竖折 3 行（1440×950 实测 box 26×45）。
      关闭压缩源（white-space: nowrap; flex-shrink: 0）后实测 **65×15** ⇒ 单行、宽 = 内容固有宽。
      **所以 26×45 是「把症状当常数」**：审计把一个布局缺陷认证成了设计常数，行 18 因此一直通过。
  期望与实际（三种情形的几何都已实测，可直接判）：
      ① nowrap + flex-shrink: 0 + **min-height: 32px** ⇒ 65×32 ⇒ **外壳 <32px == 0 ∧ 外壳 <24px == 0** ✔
         ← **这是实际落地的情形**（t66 落地后 captain 实测 `.kbd-hint` **w65 h32 min-height:32px**，`shell32 = 0`、`shell24 = 0`）
      ② 只落 nowrap + flex-shrink: 0                ⇒ 65×15 ⇒ **外壳 <24px == 1 ⇒ 地板 FAIL**（修法不完整，必须补 min-height）
         ← 中途构建曾被读到（2026-09-23 的一轮 `--check`：暗色 capture 26×45 / 亮色 capture 65×15），**已由 t66 收尾修正**
      ③ nowrap + flex-shrink: 0 + min-height: 24px  ⇒ 65×24 ⇒ 外壳 <32px == 1（过地板，但仍在集合内）← 未采纳（24 仍落在 <32 集合内）
  已退出集合的旧成员（2026-09-21，t36 的 controlHeightSM: 32）：
  button.ant-btn（.sidebar-foot 第 1/2/3 个）32×24 / 32×24 / 36×24 → 32×32 / 32×32 / 36×32
  已退出集合的旧成员（2026-09-21，t36 的 controlHeightSM: 32）：
  button.ant-btn（.sidebar-foot 第 1/2/3 个）32×24 / 32×24 / 36×24 → 32×32 / 32×32 / 36×32
判定：外壳 <32px 集合 ⊆ 上面 1 项  ∧  外壳 <24px == 0
     —— **变少满足、变多 FAIL**：⊆ 是子集语义，成员**退出**集合（变少）⇒ 通过（t36 让 3 个按钮退出正是此例）；
        出现闭集**之外**的外壳 <32px 控件（变多）⇒ FAIL（凭票入场，与 §12.6 的出血闭集同一手法）
```

用具名清单而不是数字上限，是为了**钉住身份**而不是只钉数量：数量可以是「1」（t36 前是 4），但「哪几个」才是契约 —— 集合成员随共享层改动而变，**身份判据不变**。
**外壳 <24px 必须是 0** —— 地板**不因「是外壳」而豁免**（**2026-09-23 之前**实测外壳 1 项 ≥24：`kbd-hint` 26×45；**修后应为 0**，见 §12.12；t36 前那 3 个按钮 32/32/36 × 24 也都 ≥24）。

**③ 为什么不是 (b)/(c)**

- **(b) 把外壳的控件提到 ≥32px —— 否决，理由是它连目标都达不到，且代价错位**：
  `button.kbd-hint` 实测**宽 26**，把 3 个 antd 按钮改成 32×32 之后，**kbd-hint 仍然 <32** ⇒ 外壳仍逐路由贡献 ≥1，问题没解决。
  **这一点已被 t36 实测证明（不再是推论）**：`controlHeightSM: 32` 把那 3 个按钮抬到 32×32 之后，
  外壳 <32px 仍有 **1** 项 —— 就是本节 ② 的当前闭集 `kbd-hint` 26×45。
  要让它 ≥32 就得给一个**键盘提示**加 6px 的横向 padding —— 那是纯为凑数。
  **2026-09-23 更正（本条的前提错了）**：当时把 `kbd-hint` 的「宽 26」当成一个**设计尺寸**，于是推论「要 ≥32 就只能加横向 padding 凑数」。
  **实测推翻了前提**：26×45 是**折行症状**（3 行 × line-height 15），关掉压缩源后它是 **65×15** ——
  **≥32 不是靠 padding 凑出来的，而是靠「先修折行、再把命中区高度钉到与同一 flex 行里三个兄弟按钮一致的 32」**。
  这与本条否决的「拿被度量的对象换指标」**不是同一件事**：横向 padding 改的是**外观**去骗**宽度**读数；
  `min-height: 32` 修的是**命中区高度**，且与同排 32×32 的兄弟控件对齐 —— 论证同 **t41** 对 `.ant-input.mono` 的 27→32（「同一行差 5px 无设计理由」）。
  本条对 **(c)** 的否决与其余原则**不变**。
  更根本的是：**外壳的控件并不小**（全部 ≥24px 地板），它们不是缺陷；为了一个**页面密度**指标去改**共享外壳**的外观，
  是 §12.5（画布）、§12.6（`-12px`）、§12.8（2s）拒绝过的同一种做法 —— **拿被度量的对象换指标**。
- **(c) 上调 ≤10 —— 否决**：裁决时把上限提到 16 会让 **#runtimes 的 12 个 24px 动作按钮 PASS**，即**把一个真实发现洗掉**
  （那 12 个已被 t36 抬到 32，故这条**现已无活例**；原则不变）；
  且新数值是从观测反推出来的（拟合陷阱）。10 这个预算本身没有问题，**错的是它被施加在了错误的区域**。

**④ 三条必须写明的事项（无论选哪条都成立）**

1. **24px 硬地板是独立子断言**：「`<24px 必须为 0`」与「<32px 预算」**分开计数、分开判定**，
   本裁决**不改变**地板，且地板**对外壳与内容区同时适用**（外壳实测 0 ✔）。
2. **两套口径并列，判定用命中区口径**：t27 已实现 —— display 同时给「命中区口径」与「对照（绘制口径）」，
   note 里每个地板越界项附其绘制口径原值。**判定一律用命中区口径**（控件根的真实接收交互盒子），
   绘制口径只作对照、不参与判定。
3. **外壳清单的判据出处**：`aside.ant-layout-sider.app-sider`（外壳）vs `main.ant-layout-content.content`（内容区）——
   两个选择器都是**冻结的 e2e 断言对象**，不会随视图改名而漂移。

**⑤ 裁决后的账（t38 复跑审计工具，13 路由 × 暗色，1440×900，当前构建 + 当前数据）**

> 数字来源：`node tools/design-audit.mjs --json --routes=<13> --modes=dark --no-shots --no-pixels --no-focus`，
> 读 `metrics.inter` 的 `shell32 / content32 / shell24 / content24`（即行 18 note 里「§12.9 分区」那一句）。
> **裁决时（t34/t36 之前）的数**已在括号里保留作对照，不静默改数。

| 区域 | 路由 | <32px（当前） | 明细 |
|---|---|---|---|
| **外壳**（只查一次） | 全部 13 条，逐条相同 | **实测 0**（2026-09-23，构建 **321,657,344 B / 03:56**；见 §12.12） | 原 1 项 `button.kbd-hint` **26×45** 是**折行症状**（t28 时是 4 项）—— 关掉压缩源后实测 **65×15**，故它不是设计常数；t66 修掉折行 **并钉 `min-height: 32px`** ⇒ 实测 **65×32** ⇒ **退出集合**。**修前（2026-09-23 之前）为 1**。**历史（保留）**：「应为 0（修后）」是裁决时（t66 落地前）的措辞；本行为**实测的 0** |
| **内容区** | home / sessions / board / memory / knowledge / graph / agents / stats / inbox | **0** | —（home 在 t28 时是 1：`ant-btn-sm`「查看全部」74×24，已被 t36 抬到 32） |
| **内容区** | chat | **1** | `.chat-side-head` 的图标按钮 **24×32**（宽 24 < 32） |
| **内容区** | settings | **2** | 2× `button.ant-switch` **48×24** —— 过 24 地板、未过 32 目标（t28 裁决 24 高的直接结果） |
| **内容区** | runtimes | **0** | —（裁决时是 3：3 张卡各 1 个动作按钮 **31×32**；t36 已把小号控件抬到 32×32，**2026-09-23 实测退出集合**） |
| **内容区** | task | **8** | 1× `details.hint > summary` **1109×24** ＋ 2× `details.ev.inject > summary` **403/91×25** ＋ 5× `details.ev.tool-update > summary` **230–246×24**（全部是 `<details>` 的命中区）—— 裁决时是 9：其中 `input.ant-input.mono` **1109×27** 已由 **t41** 抬到 32 退出；3 个 Select 由 t34 pin 到 32 退出 |

→ 排除外壳后**所有路由的内容区都 ≤10**（最大 **#task 8**，**2026-09-23 复测**；裁决时是 9），行 18 由 FAIL 转 PASS（t36 的结果）。
⚠️ **两个必须知道的边界**：① **判定口径已切到分区**（t39 落地）：工具输出「内容区<32px N/≤10 · 外壳闭集 ✓」，**旧口径（每路由总数）只作对照打印**。当前 **#task：旧口径 9/≤10、内容区 8/≤10 ⇒ 2 个名额余量**（t38 报的「10/≤10 恰好压线」是 **t41 之前**的状态）。**真正零余量的是行 28 与行 39**（见 §12.10.1 / §12.10.3 与 `docs/design-language.md` 的「护栏」清单）。
② 这些数字**随数据变化**（列表/时间线的行数决定 `summary` 个数），跨轮比较时必须固定数据基线。

**⑥ 登记为第七处契约内部矛盾**（延续 §12.8 的枚举；属「口径类」第四例）

| # | 矛盾 | 落槌处 |
|---|---|---|
| ⑦ | **行 18 的「每路由 ≤10」把 13 路由共享的外壳常数重复计入 13 次** | **§12.9（本处）** |

口径类四例：③ 行 1 画布（§12.5）→ ⑤ 行 30 基准（§12.7.1）→ ④ 行 37 作者来源（§12.6）→ ⑦ 行 18 外壳常数（本处）。**2026-09-22 更新：现共六例，新增 ⑨ 行 27 框架地板与 ⑧ 行 28 首屏突发，见 §12.10.5。**
共同形态：**度量对象里混进了与判据目的无关的常数或另一种来源**。

**⑦ 观察已闭环（2026-09-21 更新）**：裁决时 #task 的 3 个 `.ant-select-outlined` 是 **31px**（距 32 目标差 1px），
机制是**内容驱动**（`.ant-select-content` 21px = 14×1.5 + padding-block + 1px 边框 = 31），不是 antd 的 off-by-one。
**t34 已按本节观察把 `.ant-select-outlined { min-height: 32px }` 落地**（t36 另补 `.ant-select-sm`），
实测 120/180/154 × 31 → **×32**、#settings 351×31 → ×32、#memory 170×31 → ×32 ⇒ **这 3 项已退出 <32px 集合**（见 ⑤ 的 task 行）。

**⑧ 工具侧的已知偏差（登记，转 tools）**：审计工具**已按本节 ① 分区并输出 `shell32 / content32 / shell24 / content24`**，
但**判定仍按「每路由总数」**（shell + content），未切到「内容区 ≤10 + 外壳具名闭集」。工具自己在运行时就会报这条：
`行 18 契约漂移（§12.9）：MASTER 已裁决「外壳常数不计入每路由 <32px 预算…」，本工具只输出分区数字、尚未改变判定`。
**在它落地前**，行 18 的 PASS/FAIL 以「每路由总数」为准（当前 #task 恰好 10/≤10，见 ⑤ 的边界注）。
**2026-09-22 更新：工具已按本节实现分区判定**（行 18 输出「内容区<32px N/≤10 · 外壳闭集 ✓」），本 ⑧ 条的工具侧偏差**已闭环**（t13 复核）。

---

### 12.10 四项裁决（2026-09-22）：行 28 的判定范围、行 27 的地板与预算、行 39 的新增、品牌色 alpha

**背景**：t13 评审（本代设计负责人）报出三处、t8 的组成分析报出四处，共四项需要落槌。其中 **A 是硬阻塞** —— 不落槌，t8 的行 28 必然误判。

#### 12.10.1 行 28：判定范围从「每路由总数」改为「逐端点 + 外壳轮询分离」

**① 病灶（五组独立实测）**

| 观察 | 数字 | 来源 |
|---|---|---|
| 11.5s 口径下逐路由判定的结果 | **9/13 路由 FAIL** | t8（全量跑，`--api-window=11500`） |
| 全站 2s 轮询器数量 | **恰好 1 个**（`/api/v1/permissions`，由外壳持有） | t8 的突发/稳态分解；本处复核见 ④ |
| 外壳的 `permissions` 在 11.5s 内的次数 | **6–9**（`#settings` 6、`#home` 8–9） | 本处实测，构建 `D6UEuuvX` |
| 修前 `#graph` 首屏 | **63 个 `/api` 请求** = 1×entities + 7×permissions + **55×`/graph/entity/<id>`** | t13 实测；t42 修复后 = **7** |
| 外壳常数占门槛的比例 | **6/7 = 86%** —— 逐路由预算被外壳吃光 | 本处实测 |

⇒ 行 28 的**门槛（7）是对的**（它区分 1 个与 2 个 2s 轮询器），**错的是判定范围**：把「首屏载入突发」与「13 路由共享的外壳轮询常数」都算进了「单路由请求数」。

**② 三个候选的评估**

| 候选 | 能否解决 | 裁决 |
|---|---|---|
| 一、稳态窗口（`domStable` + settle 后再起 11.5s） | 能排除首屏突发，但**不能**解决外壳常数占掉 6/7 的结构问题；且引入一个新的起点信号 | **不采纳**（治了一半，代价是新增信号） |
| 二、只判有轮询器的路由 | 会把 12 条路由的该项判 `not_measured` —— 而「没有轮询器」**正是本行要确认的结论**，判 `not_measured` 等于放弃信号 | **不采纳** |
| 三、只计被轮询的端点（如 `/permissions`） | 会漏掉「视图自己的轮询器」（`#task` 的 `tasks/<id>` 2s、`#runtimes` 的 `agents` 5s）；且「哪些端点是轮询端点」本身循环定义 | **不采纳** |
| **四（另拟）：逐端点分组 + 外壳轮询分离** | 突发里每个端点只被取 1–2 次 ⇒ **自然不计入**；同端点被两个轮询器各取一遍 ⇒ **立刻翻倍被抓**；外壳的 `/permissions` 单独只查一次 ⇒ **不再吃掉逐路由预算** | **采纳** |

**③ 落槌：机器可判定的定义**

1. **窗口起点**：**不变** —— 仍从页面 `load` 起 11.5s（工具现有口径）。**不引入新的起点信号**，因为逐端点分组已经把载入突发排除（突发里同一端点只出现 1–2 次，远低于 7）。
2. **计入哪些请求**：
   - **逐路由部分**：该路由在窗口内的**全部 `/api` 请求**，**但排除 `/api/v1/permissions`**；把路径里的数字 id 归一化为 `:id`（`/graph/entity/55` → `/graph/entity/:id`）后**按端点分组**，取**最大组**。
   - **独立断言部分**：`/api/v1/permissions` 单独计数，**只查一次**（13 路由 × 两模式取最大值）。
3. **判定**：`逐路由最大组 ≤ 7` **且** `permissions 最大值 ≤ 7`。两个子断言**分开计数、分开判定**（与 §12.9 的 24px 地板同手法）。
4. **没有轮询器的路由该行判什么**：**照常判 PASS**（其端点最大组为 1–2），**不判 `not_measured`、不判「不适用」**。理由：本行的目的是「确认没有重复轮询」，而「没有轮询器」就是该目的的直接结论。
5. **门槛 7 不变**，2s 间隔不变；**要修的是判定范围，不是数值**。
6. **新口径下的账（本处实测，构建 `D6UEuuvX`，11.5s 窗口）**：

| 路由 | 总请求 | `/permissions` | 非 permissions 最大组 | 端点种类 | 新口径 |
|---|---|---|---|---|---|
| `#home` | 20 | 8 | **2**（agents/sessions/tasks/stats/memoryList/knowledgeDocs 各 2） | 7 | 逐路由 PASS · **permissions 断言 FAIL（8 > 7）** |
| `#task` | 15 | 6 | **6**（`tasks/<id>`，TaskDetail 自己的 2s 轮询） | 5 | PASS（两个子断言都 ≤7） |
| `#graph` | 7 | 6 | **1**（`/graph/entities?limit=500`） | 2 | PASS |
| `#runtimes` | 10 | 6 | **1** | 5 | PASS |
| `#settings` | 8 | 6 | **1** | 3 | PASS |
| `#inbox` | 6 | 6 | 0（外壳让渡，视图持有轮询器） | 1 | PASS |
**④ 顺带抓到的真缺陷：`#home` 的第二条 `/permissions` 调用**（新口径的直接产物）

- 位置：`panel/src/views/Home.tsx:141` 在 `load()` 里调 `pendingPermissions()`，而 `Home.tsx:150` 让它 **mount + 每 10s** 各跑一次；外壳的 2s 轮询器（`App.tsx:126`）**同时在跑**（外壳只在 `#inbox` 让渡）。
- 实测时间戳（`#home`）：`198, 198, 2193, 4200, 6199, 8203, 10207, 10207, 12194`（gaps `0, 1995, 2007, 1999, 2004, 2004, 0, 1987`）—— `198` 与 `10207` 各有一对**同毫秒**请求，`10207 − 198 = 10009 ≈ 10s` 正是 Home 的 10s tick。
- ⇒ 新口径下 `#home` 的独立断言 = **8–9 > 7 → FAIL**。**这是判据修好后的正确结果**：旧口径下它被突发掩盖，看不出是谁在重复。
- **修法**：`Home.tsx` 的 `load()` 去掉 `pendingPermissions()`（徽标计数由外壳拥有；Home 的 inbox 磁贴改读外壳已取到的数，或删掉该磁贴）。
- **归属**：ui-shell（外壳所有者），已登记进 §12.10.5 的转交清单。
- **2026-09-23 已闭环（t50）**：`Home.tsx` 不再调 `pendingPermissions()`，改由**外壳单方拥有**并以 `inbox: number \| null` prop 下发（`App.tsx` 只在 `#inbox` 让渡）。实测（build `C048RA32`）：`#home` **逐路由最大组 = 2 ≤7 ✔**、`/permissions` 独立断言**全站最大 = 7 ≤7 ✔** ⇒ 本 ④ 条的缺陷已消除。

**⑤ A3 的纳入**：t8 的分解结论被本处复核确认 —— `#task` 的 `tasks/<id>` 6 次是 TaskDetail **自己的** 2s 轮询（合法，≤7 ✔）；`#home` 的 12 次非 permissions 请求是「**同一批端点被取两遍**」（7 个端点 × ~2，其中一次是 `Home.tsx:150` 的 10s tick）。**两者在新口径下都 PASS**，后者另登记为**优化项**（不是判据问题，见 §12.10.3 ④）。
**关于 t8 登记的「t13 的 63 不可复现」**：t8 构建的是 **t42 修复之后**的源码（实测 7），t13 的 63 是在**修复之前**测的，t42 自己也报 63 → 7 ⇒ **三者一致**，「不可复现」是**无效否证**，不作为削弱本裁决前提的依据。

**⑥ 残余盲区（诚实登记）**：两个**较慢**的同端点轮询器（例如两个 5s 轮询器 → 11.5s 内 6 次）仍在门槛 7 之内，本口径抓不到。堵它需要**间隔感知**判据：`每端点请求数 ≤ ceil(11.5s / 该端点在窗口内的最小观测间隔) + 1`。本轮不做（无活例），登记备查。

#### 12.10.2 行 27：从单一门槛改为「地板 + 预算」两段（E 落槌）

**① 结构不可达的算式**（t8 的组成百分比 × 本代实测总量 1406KB）

| 层 | 占比（t8） | 字节 |
|---|---|---|
| antd + rc | 51.1% | 718KB |
| react-dom | 18.6% | 261KB |
| **框架层小计 = 地板（应用侧改不动）** | **69.7%** | **980KB** |
| markdown 栈（可懒加载） | 16.3% | 229KB |
| 应用代码（可路由级分割） | 11.6% | 163KB |
| 未归类（其他依赖 + 运行时胶水） | 2.5% | 35KB |

⇒ **地板 980KB > 门槛 350KB**。即便把 markdown 栈与应用代码**全部删掉**，入口仍 ≥980KB ⇒ **保留当前技术栈时行 27 结构上不可达**，与「应用侧改不改得动」无关。
（t8 报告另写「框架地板 686KB」，与它自己的百分比不符 —— `69.7% × 1405KB = 979KB`；本处**以复算的 980KB 为准**。两个数都远大于 350KB，结论不受影响。）

**② 修订后的判据（两段 + 两条结构断言）**

| 段 | 数值 | 判定 |
|---|---|---|
| **地板**（框架层：antd+rc+react-dom） | **980KB（gzip ≈305KB）** | **登记，不判 PASS/FAIL**；每次审计打印；随技术栈变更**重测** |
| **预算**（应用层首屏） | **≤150KB（gzip ≤48KB）** | **判定** |
| 结构断言 ① | markdown 栈**不得在入口**（必须是独立 chunk） | 判定 |
| 结构断言 ② | 必须存在**路由级分割 chunk** | 判定 |

**③ 预算 150KB 的推导（口径提醒：本句的 426KB 是「优化前的**全量**应用层」，判定用的是**首屏口径**，两者差 4 倍 —— 见本小节末）**：应用层实测 = `1406 − 980 = 426KB`；其中 markdown 栈 229KB 应懒加载、应用代码里重页（`#graph`/`#task`）应路由级分割（t8 估 **−119KB**）⇒ **首屏应用层目标 = 426 − 229 − 119 = 78KB**；预算取 **150KB ≈ 目标的 2×**，给应用留一倍增长空间而不触发。

**③ 的两个口径不可混用（t54 落地，2026-09-23）**：`426KB`（本代实测 `391KB`）是**优化前的全量应用层** = 总量 − 地板 —— 它把 **15 个懒加载路由 chunk** 也算进了首屏，**不是首屏要下载的量**；**判定值 = 首屏静态 chunk − 地板**，其中首屏集合**由产物实测**：`dist/index.html` 里的入口 `<script>` + 每个静态依赖的 `modulepreload`（懒加载 chunk 按构造不在集合里）。实测（构建 `C048RA32`）：首屏静态 **1114KB** − 地板 **1020KB** = **应用层首屏 94KB / gzip 31KB ✔**（≤150KB / ≤48KB）。**地板同样按裁决「随技术栈变更重测、不写死」取实测 vendor chunk**，本表登记的 980KB 只打印、不参与判定。

**④ 实施前提**：新判据需要 ① `panel/vite.config.ts` 加 `manualChunks`（vendor / app / markdown 三块）+ ② 工具按 chunk 读 `dist`。**两者落地之前，行 27 判 `not_measured`** 并打印「地板 / 应用层 / 总量」三个数（工具现在的做法是拿总量比 350KB，必然 FAIL，属于无效判定）。

**⑤ 不做的事**：**不把行 27 改成 PASS**（那是粉饰）；**不删掉 350KB 这一行**（那是抹掉记录）。历史数字（`UX` 1367KB/428KB、`M2` 1406KB/436KB）保留在行内。
#### 12.10.3 行 39：新增「首屏 `/api` 请求数」防 N+1（B 落槌）

**① 归属裁决：不并入行 28，新增行 39（全局）**
- 行 28 判的是「**轮询重复**」（同一端点被反复取），行 39 判的是「**首屏条目级展开**」（一个条目一个请求）。两者是**不同现象**，并入会把它们混进一个数 —— 这正是本代反复出现的口径病（§12.10.5）。
- 选**全局行**而不是只写进 `view-graph.md`：N+1 是**全站风险**（任何列表页都可能犯），不该只由一份视图规格守着。`#graph` 的**绝对数回归**另由 `view-graph.md` G15 守（见 ③）。

**② 可判定的目标**：`R ≤ 2K + 2`（`R` = 首屏请求数，排除 `/permissions`；`K` = 归一化端点种类数）。
- 实测：`#graph` 修前 **56 > 6 ✘**（1×entities + 55×`/graph/entity/<id>` ⇒ `K = 2`）／修后 **1 ≤ 6 ✔**；`#home` 12 ≤ 16 ✔；`#task` 9 ≤ 12 ✔；`#runtimes` 4 ≤ 12 ✔；`#settings` 2 ≤ 8 ✔。
- 为什么是比例式而不是绝对数：绝对阈值会随页面数据面数量变化而失准；`2K+2` 直接编码「每个数据面允许首取 + 一次重取，另给 2 次余量」，**N+1 无论怎么变形都会撑爆比例**。

**③ 与 `view-graph.md` G15 的关系**：G15 是**绝对数**（`#graph` 首屏 `/api` ≤ 8，含外壳 permissions，实测 7），简单直接、专守本页回归；行 39 是**比例式**，全站自标定。**两条并存，不冲突**。

**④ 登记为优化项（不是判据问题）**：`Home.tsx` 的 `load()` 让同一批端点被取两遍（`#home` 12 次 = 7 端点 × ~2）⇒ 修完 `Home` 的第二条 `permissions`（§12.10.1 ④）后一并处理，最重页可降到 ~7。**不改行 39 的判据**。

#### 12.10.4 品牌色 alpha 的落槌（C，对应 t13 的 F1）

**① 复核算式**（本处独立复算，**两模式 × 三品牌**；合成 = `a × 品牌色 + (1−a) × 卡片底`，再算 WCAG 对比度）

| alpha | 暗/claude | 暗/deepseek | 暗/opencode | 亮/claude | 亮/deepseek | 亮/opencode | 全 6 格 ≥3.0？ |
|---|---|---|---|---|---|---|---|
| 0.22（现状） | 1.385 | 1.372 | 1.894 | 1.255 | 1.268 | 1.625 | ✘ |
| 0.50 | 2.297 | 2.218 | 4.403 | 1.713 | 1.760 | 3.521 | ✘ |
| 0.75（t13 的暗色解） | 3.576 | 3.379 | 8.193 | **2.302** | **2.415** | 8.420 | ✘（亮色不够） |
| 0.90 | 4.577 | 4.278 | 11.254 | **2.763** | **2.943** | 14.412 | ✘ |
| **0.97** | 5.108 | 4.752 | 12.897 | **3.010** | **3.230** | 17.562 | **✔** |
| 1.00 | 5.348 | 4.966 | 13.645 | 3.122 | 3.362 | 18.734 | ✔ |

⇒ **目标 alpha = 0.97**（最小可行值；最紧的一格是 **亮色/claude = 3.010**，0.90 时只有 2.763 ✘）。
t13 的算式只覆盖暗色（得 0.75）；t42 的亮色数据正确，**本处独立复算与之一致**。
**② 落点与归属（两处根因各归谁）**

| 项 | 改什么 | 落点 | 归属 |
|---|---|---|---|
| 描边 alpha | `22% → 97%` | `index.css` 的 `.runtime-logo` 描边（`border: 1px solid color-mix(in srgb, currentColor 22%, transparent)`） | **共享层（systems）** |
| 品牌色被覆盖 | `.brand-claude` / `.brand-deepseek` / `.brand-opencode`（`index.css:1848-1850`，单类选择器）被 `index.css:1854-1858` 的 `.runtime-logo` 的 `color: var(--ant-color-text-secondary)` **同优先级后置覆盖** | `index.css` | **共享层（systems）** |
| `mono` 让标记走中性色 | 去掉 `mono`（只让无品牌的 mock/未知走中性） | `Runtimes.tsx:225` | **视图侧 —— t42 已修 ✔**（实测三 tile 标记色已互不相同：暗 claude `rgb(217,119,87)` / deepseek `rgb(87,134,254)` / opencode `rgb(232,232,234)`） |

**③ 备选方案（登记但不采纳）**：把 tile 描边判为**装饰**、把 R8 的目标移到图标（图标对 tile 底实测 **6.547:1（暗）/ 5.498:1（亮）**，已达标）。**不采纳**的理由：R8 原文写的是「描边对底 ≥3:1」，且 `a=0.97` 同时解决了 F1 的另一半（tile 现在能看出品牌）；**若日后要改判为装饰，必须同时改 R8 的目标，不许留着 3:1 而停在 1.56:1**。

**④ 视觉影响面**：3 个 38×38 tile 的 1px 描边由「中性 22%」变为「品牌 97%」（近实色品牌描边）；**不改尺寸**，行 5 的可见描边元素数不变（仍是 3 个）。
**⑤ 底色（2026-09-23 更正）**：本处原写「不改底色（底色仍是 10% 混合）」—— **该结论已被 §12.11.2 推翻**：标记是**内嵌图形**，其对比度基准是**自身底色**而非卡片，10% 品牌淡染下亮色 claude 只有 **2.81:1** ⇒ **底色 alpha 必须 ≤2%**（临界 3.966%）。描边的基准仍是卡片（边界类元素对**外部**相邻色判），不受此更正影响。

#### 12.10.5 口径类问题现共六例（枚举）+ 转交清单

| # | 口径问题 | 度量对象里混进了什么 | 落槌处 |
|---|---|---|---|
| ③ | 行 1 暗色单亮度带 | 应用画布（非「被设计的面」） | §12.5 |
| ④ | 行 37 间距越界 | antd 注入值与 UA 默认值（非设计者写的值） | §12.6 |
| ⑤ | 行 30 ring 基准 | 白底（非 ring 实际贴的画布） | §12.7.1 |
| ⑦ | 行 18 <32px 预算 | 13 路由共享的外壳常数（重复计 13 次） | §12.9 |
| **⑨** | **行 27 首屏体积** | **框架地板（应用侧改不动）** | **§12.10.2（本处）** |
| **⑧** | **行 28 请求数** | **首屏载入突发 + 外壳的轮询常数** | **§12.10.1（本处）** |

**共同形态**：度量对象里混进了与判据目的**无关的常数、另一种来源，或另一种现象**。
**另有九类问题（不是口径问题，但同属「契约文本骗人」）**：

1. **把缺陷症状认证成常数** —— §12.9 的外壳具名闭集曾把 `button.kbd-hint` 的 `26×45`（= 3 行折行的症状）当作设计常数，见 **§12.12**。
2. **把一次性读数当成常数** —— 本代已判过多次（行 30 的画布基准 · 行 18 的外壳常数 · S6 的 4 · K5 的 12 · `kbd-hint` 的 26×45）。**对策：在记录里写明它不是保证**（§12.15 的「⚠️ 效力与边界」段即此 —— 在源头堵住下一个人把它当常数）。
   **具体动作（2026-09-23 增补）：同文件内的自描述计数必须「派生或断言」，不得手写。** 实例：§12.10.5 的引导句曾写「**另有三类问题**」，而其后紧跟的条目已扩到 5 条（现 6 条）—— 它描述的是**同一文件内、紧跟其后的 items 数量**，**完全可派生**，却被人手写并漂了。**本代同类**：S6 的 **4 → 0** · view-knowledge K5 的 **12 → 0** · 行 18 外壳常数的 **26×45** · §12.9 的 `kbd-hint 26×45` · 行 30 的画布基准 —— **每一次都是「发现 → 改成当前值」，下一次结构一变还会漂**。
   **落地形式（本仓库是 markdown，散文无法求值）**：**能派生的地方派生**（工具输出 / 生成物用 `{items.length}` 这类），**手写的散文计数必须被断言** —— 把「引导语里的数字」与「items 的实际条数」比对，不等即报错（与 pipe 校验、序号单调断言并列）。**一般化：凡是散文里出现、用来描述某个结构的数字，都必须能追溯到那个结构**，否则它就是「把读数当常数」的**手写版**。（captain 2026-09-23：**优选派生** —— **本代漂过的每一个计数都是手写的**：引导句「另有三类问题」· S6 的 4 · K5 的 12 · `kbd-hint` 26×45。**手写的数字必然漂，派生的不会。**）**断言版与派生版的差别（captain 2026-09-23 补）**：**派生版不可能漂**（数字由结构生成），**断言版漂了会被抓住**（数字仍手写，但机器比对结构）；**两者都远好于「靠人记得改」** —— 本仓库是 markdown，散文数字无法求值 ⇒ 实际落地是**断言版**（@check-contract.mjs@ 的计数规则），它已经在「六类 → 七类」那次生效（改完实跑 PASS 才说明一致）。
   **更锋利的判据（由一次真实失效提炼，2026-09-23 captain 采纳）**：**问「这个阈值在结构增长后还成立吗？」** —— 若答案依赖「文件现在有多少行 / 多少条 / 多少字」，它就是**代理量**，应换成**结构量**。
   **实例**：`check-contract.mjs` 的「往前找最近的表」曾写成 **「最多回溯 120 行」** —— 它**在写下时是对的**（那时表就在窗口内 ✓），**结构一长就悄悄错了**：我往 §12.10.5 插了总纲 + 第 8 条 + 写法规则 + 判断法 ⇒ **把那张表挤出窗口** ⇒ 规则**派生不到**。**修法**：换成**结构作用域**（往上找到**本节标题** `^#{2,4} ` 为止），并在**注释里写明为什么不用魔数**（「那会让插入内容悄悄把表挤出窗口，而报出『无法派生』」）—— **注释写在下一个编辑者会读到的地方** ✓
   ⇒ **任何编码了「文件今天恰好长什么样」的阈值，都是一个来自快照的常数**；**结构性查询不会随插入失效**。

   **⚠️ 顺带登记的缺口（本代第 7 个「假绿」形态）**：契约自身的结构校验（pipe 数 / 标题层级 / 序号单调 / 计数一致）目前跑在 design-lead 的**临时脚本**里，**仓库里没有归属**。更值得警惕的是：**它一直在跑，却没在看会漂的东西** —— pipe 数我从没写错过，而**真正漂过的三样（自描述计数、标题层级、陈旧措辞）它一项都没查**（§12.14 的父标题粘连是 `grep` 抓到的、计数漂移是 captain 抓到的）⇒ **这正是第 4 条的形态：判据匹配不到对象时静默通过**。已补两条断言（计数一致 + 标题层级），见下方转交。
3. **把部分验证当成整体验证** —— 「**我核过的那部分是对的**」造成的错觉。实例（2026-09-23，本文件）：落 §12.14 时只核了 `#### 12.14.x` 子标题、**没核父标题** `### 12.14`，而它当时被粘成 `---### 12.14`（不合法标题）。**与「只测修复后的构建」是同一类错误**（都发生在「我核过的那部分是对的」这个错觉上）。**对策：核结构必须核层级完整性**（父标题 / 子标题 / 围栏闭合），**核修复必须核修复后的构建**。
   **新增动作（2026-09-23）：判断一个产物属于哪一轮，必须用「冻结的时间戳证据」**（那一轮的 `metrics.json` / 构建 id），**不得读活文件的当前状态** —— 尤其当**该文件正在被某个 in_progress 的任务修改**时。本次实例：captain 读 `design-audit.mjs:2901` 的当前内容断定 42–45「早就交付」，而 t78 正在写那个文件；t78 用 `t77-run/metrics.json`（t78 之前**冻结**的那一份）证明当时只有 **41 行** ✓。**与本条原实例同族但不同**：原实例是**修复已落地**（要测**修复后**的构建），这一例是**被评判的任务正在写那个文件**。**一般化：活的产物是移动靶；要断言历史，就用被冻结的那一份。**

4. **判据匹配不到对象时静默通过（找不到 = 没问题）** —— 前三条都发生在「读数被**人**误用」；这一条发生在**读数根本没产生**的时候：判据找不到对象 ⇒ 报 **PASS**，而**没有人会去检查它到底量到了什么**。**它的失败模式是「静默通过」**，而绿是默认状态。
   **本代实例（都是真实踩过的）**：
   - ① **CJK 空格陷阱**：antd 在两个汉字之间插入空格 ⇒ 重试按钮的 `innerText` 是 `重 试` ⇒ 朴素的 `/重试/` **匹配不到任何东西** ⇒ **行 20 从 02:18 的 FAIL 变成 02:26 的 PASS（同一份源码、不同工具构建）**；若没人发现，**13 条路由会一直是「绿」的假绿**。
   - ② **`.time` 选择器过时**：t72 后 ≤520 下 `#sessions` **不渲染 `.time`**（改用嵌套中性类）⇒ 任何按 `.time` 找窄屏时间的判据**找不到对象**。**view-sessions.md §6 的「≤520 的验收口径」那句提醒本身就是对策** —— 但也正说明：**若没写，判据会静默通过**。
   - ③ **行 13 锚点失配**：目标格改成 `**0**（当前观测：…）` 后，`^\s*(\d+)\s*$` **匹配不到** ⇒ 锚点检查**静默跳过**（本代登记为「anchor misses = 1 行」）。
   - ④ **`parseInt("1.00") === 1`**：把 §12.10.4 alpha 表的第 1 行**当成行 1** ⇒ 覆盖了行 1 的目标（13.645）⇒ **行 1 从 t44 起一直在用错的判据**（也属「量错对象」，但**沉默期特别长**）。
   - ⑤ **宽选择器数到的 4 个 <32px** 实为 **antd 内部 INPUT**（外层控件 32px）；以及**行内 `span` 的 `clientWidth` 恒为 0**（第一版就这样量出 0/0）—— **「读到 0」与「量错对象」在输出里长得一模一样**。
   **对策（三条，必须一起）**：
   - **选择器命中数必须非零**，或**显式声明「本档预期 0 个对象」并给出理由**；
   - **锚点失配必须报出来，不得静默跳过** —— 本代已有 Warnings 机制（行 13 正是这么被发现的 ✓），应把 **anchor misses 从「警告」升级为必须出现在结论行**；
   - **任何「目标值」解析后必须与原文对得上**（`parseInt("1.00")` 这类**必须报错**，不是静默取整）。
   **为什么它比前三条更危险**：前三条是**人**误用读数（**可以被复核**）；这一条是**仪器不再产生读数**，而**绿是默认状态** ⇒ **没有人会去看一个 PASS**。**本代整个方法论建立在「机器判定 + 双向自检」上，第 4 条正好攻击这一点** —— 因此 **must-FAIL 自测就是它的通用解药**：注入违规后必须 FAIL，**等于证明了「它确实在看那个东西」**（后续每个新判据的行条目文本都必须写明这层关系，见 t78 的 4 个类）。
   **新增动作（2026-09-23）：「空集」必须被区分并声明是哪一种。** 行 45 的初版判据是「≥2 个同类兄弟同父」⇒ **每条路由候选 0** ⇒ **空集通过**；而行 43 的空集通过是**真实的**（全站 `<img>` = **0 个**，已声明）。**两者表面一样、本质相反**：一个是**判据错**（面板把数字放在各自单元格、列在**祖父**层），一个是**对象确实不存在**。⇒ **只看「0 命中」无法区分**；条目里必须写明是「**对象不存在**」（合法，记「0 个对象」）还是「**判据找不到对象**」（**必须改判据**）。**与本条对策 ① 是同一件事的两个方向**：① 要求「说清为什么是 0」，这里要求「说清 0 的两种含义」。
   **同族反面（2026-09-23，design-lead 自己踩到）：「提及」不是「发生」。** 我为「`---` 与标题粘连」新写的断言**第一版就报了假 FAIL** —— 它在全文里搜 `---###` 并命中，而那处唯一的出现是 **§12.10.5 第 3 条里对这个缺陷的「提及」**（写在行内的代码片段里）。**修正后的定义 = 「以 `---#` 开头的行」**（真正粘连的形态），随即 PASS。⇒ **判据的对象集不仅要排除「不存在」，还要排除「被提到」** —— 与**行 44 的「读 DOM 而非源码」**（注释进不了 DOM）、**§12.15 里两处 `outline: none` 命中「都在注释里」**是**同一条理由**。
   **三个方向，同一条理由（captain 2026-09-23）**：① **行 44「读 DOM 而非源码」** —— 读源码会误判（注释里的 `...` 不是内容）；② **§12.15 两处 `outline: none` 在注释里** —— 同上；③ **本条这个 `---###` 断言** —— 命中的是**对那个缺陷的提及**（行内代码片段），**不是那个缺陷**。⇒ **共同点：判据在「文本」上操作时，必须区分「那个东西」与「对那个东西的引用」。**
   **可判定的形态**：**行内代码片段 / 代码块 / 引用块 / 注释** —— 这四种里的内容一律**先剥离再判**。
   **注意方向之别**：行 44 是「**读源码**会误判」（⇒ 要读 **DOM**），本条是「**读全文**会误判」（⇒ 要按**行首 / 结构**定义）。
   **最直白的注脚（2026-09-23）：夹具绿 ≠ 判据在看对象。** 我为 t82 写的自检脚本第一版：@count-items@ 的正则要求「问题」后紧跟 @**@，而真实句子是「另有六类问题（不是口径问题…）」⇒ **一次都没匹配过**；@table12@ 对 15 个无 §12 表的文件报噪声；@main()@ 的 @rel@ 初始化顺序 ⇒ @ReferenceError@。**三样的共同点：判据没有在错误的地方失败，而是根本没在跑**（或跑在错误的对象集上）—— 与第 4 条、第 5 条同族。**而抓到它的方式，正是 must-FAIL 注入 + 真实契约实跑**（baseline @[]@ → 注入 ⇒ @count-items:1493@ / @glue:1715@ / @table12:788@ / @count-examples:1549@ / @hierarchy:1670,1687,1704@）。
   **补充实例（2026-09-23，captain）：对象在嵌套结构里时，找错层级会得到与「不存在」相同的输出。** 实例：在 @--json@ 的**顶层**找 @checks@ 得到 @None@（它实际嵌在 @summary@ 下）⇒ 差点据此报「声明与产物不符」。**它与本条同族**（都是「匹配不到对象」），但**下游动作相反**：本条多数实例是**误报合规**，这一例是**误报对方**（把「我找错层级」读成「你没实现」）。⇒ **对策同构**：**在断言「某对象不存在」之前，必须先证明你在正确的层级找过**（本仓库的落法：@--json@ 的形状写进脚本头注释与 @--help@，让下一个读者不必猜层级）。
   **新子形态（2026-09-23）：仪器本身报错 ⇒ 下游把「缺失的数据」读成「空集」⇒ 同样静默通过。** 实例：tools 落行 45 的 (a) 后，`isDateTime` 被插在用到它的循环**之后** ⇒ **探针抛 `ReferenceError`** ⇒ 行 42–45 把**缺失的数据读成空集** ⇒ 四行全绿（**Warnings 里写了，但行是绿的**）。**机制与本条原形态不同**（**不匹配** vs **异常**）、**输出相同**（绿），而且**本条原有对策抓不到它** —— 「命中数必须非零」抓不到「**探针整个坏了**」，因为**它连命中数都没有**。**对策（已实装）**：**探针 / 依赖失败必须显式降级为 `not_measured`，不得落到 PASS** ⇒ 即 **「没有数据」与「数据为空」必须被区分** —— 与行 43「**0 个对象 ≠ 已验证合规**」是**同一条理由的两种表现**。**对策的产物形态（tools 自检名，照录）**：**`a FAILED markup probe yields not-measured, never pass (a broken instrument must not read green)`**。
   **又一实例（2026-09-23，captain 的探针）：正确动作 + 错误判据 = 相反结论。** 在**验证 C5③ 的前提**（SSE 会重放）时，探针去找 `event:` 行统计事件类型，**而事件类型在 `data:` 的 JSON 里**（SSE 用默认 message 事件 + JSON payload）⇒ 它打印「**8s 内未收到任何事件**」，**而 2323 字节的事件就在缓冲区里** ⇒ **「读到 0」= 「量错了对象」**。**危险在于动作本身是对的**（「验前提」正是本条要提倡的）：若只看那句「未收到任何事件」就下结论，会得出**与事实相反**的结论（「C5③ 前提不成立」）**并去改一个本来正确的契约**。**对策**：**读到「空」时必须先证明「我在正确的对象上读」**，并把这一步**写进判据**（例：**SSE 探针必须同时报告 `bytes > 0` 与 `seq` 范围**，而不是只报事件类型计数）。

   **对偶实例（2026-09-23，systems 在 t89 发现）：规则的作用域比它的意图宽 —— 「滥」。** 一条「**隐藏元数据**」的规则（选择器 `.row-btn > .muted`）**与主标签共享了类**（会话标题的类名是 `title muted`）⇒ **顺手把主标签也隐藏了** ⇒ **390 下 chat 轨道只剩一个 live-dot**，而**用户症状正是「看不到 session 的文字」**。**它与本条是「对象集与意图不对齐」的两端**：**本条是「漏」**（匹配不到对象 ⇒ 静默通过），**这一例是「滥」**（匹配到多余对象 ⇒ 静默多删），而**两端都无信号**（**审计 41 项全绿、e2e 全绿**）。**对策**：**作用域必须由结构给定，而不是由「类名共享」给定** —— 已折进 t91：把该规则收窄到它**真正想隐藏的元数据类**（**结构豁免、不靠特异性**）。

   **正面实例（对策生效，而不只是「失败被记录」；2026-09-23）**：**完整连锁** —— 我的插入（总纲 + 第 8 条 + 写法规则 + 判断法）→ 表被挤出窗口 → 规则无法派生 → **自检 FAIL** → 暴露魔数 → 换成结构作用域 → 复验（含**注入仍咬人**：`六例→五例` ⇒ `count-examples:1613`）。**关键的一环是「规则没有静默通过」** —— 而这一环**不是这次才有的**：是**之前某次就把「无法派生」写成了显式输出**（而不是「找不到就跳过」）⇒ **早期的一个决定，在这次救了一条规则**。本代的条目多是「这里错过一次」，**这一条是「这里没有错过，因为当初没有偷懒」**。

   **又一小实例（2026-09-23，工具侧）：对象的「名字」比「实际对象集」窄 ⇒ 读者会以为别处没被管。** 审计第 46 行的**标题**写「**会话行**主标签可读性（**侧栏**标题宽度）」，而**正文已是「列表行主标签（所有列表）」** ⇒ **只看标题的人会以为它只覆盖 chat** ✗。**它与本条的「漏」同族，但发生在「命名」这一步**：**对象集是全的，而它的名字让人以为不全** —— 同样**无信号**（标题永远绿）。**对策**：**判据的标题必须与它的对象集同宽**（对象集改了，标题必须一起改）。

   **又一实例（2026-09-23，tools 自查）：控制跑读数「与出厂完全相同」看起来是通过，实际是没生效。** 控制跑的第一版把样式注入放在 `goto` **之前** ⇒ **被页面加载抹掉** ⇒ **读数与出厂构建完全相同** ✗（且行 20 的失败态探针会重载文档 ⇒ 注入必须在探针内**再次应用**）。**它是靠「与出厂完全相同」这个可疑之处发现的** ✓。**对策**：**控制跑的读数是「与出厂不同」，那才是生效的证据；相同即为未生效。**

   **第三个子形态（2026-09-23）：对象在到达判据之前被改动了。** `maskRefs()` 为「提及 vs 出现」而剥离**行内代码片段**；**当引用形态本身畸形（未配对的反引号）时，剥离会越过引用边界、把单元格分隔符一起吃掉** ⇒ **判据看到的不是原文，而是一个被改写过的版本** ✗（实例：§12 **第 46 行**落表时我写了一个孤立反引号 ⇒ `pipes` 报「表格列数不一致：5 vs 表头 7」）。
   **与已有两个子形态并列**：① **判据匹配不到对象**（**漏**）② **判据匹配到多余对象**（**滥**）③ **对象在到达判据前被改动**（**这一条**）—— **共同点是「判据看到的 ≠ 产物里的」，而方向不同。**
   **⚠️ 最关键的是「谁抓到了它」**：**抓到它的是 `pipes` 规则**（我因为一次真实漂移才补的规则 6）⇒ **一条规则失效，被另一条规则抓住了**。**对策（可推广的部分）**：**同一对象上要有互相独立的检查** —— 若只有 `maskRefs()` 那条链，这次会**静默通过**；而 pipe 规则**从另一个角度看同一个对象**，于是看见了。**修法（用成对引用重写）只是止血；真正防住它的是那条独立规则。**

   **又一实例（2026-09-23，captain 自报）：从 n=1 推广。** 截图看到**一个** 294×32 的按钮 ⇒ 推断它是**板级控件**且位置错了 ✗ —— 实际**每张卡一个**，而探针只看到一个，**因为其它列是空的（0 任务）** ✓ ⇒ 这是本条的「**漏**」，但发生在**取证**这一步：**观察的对象集太窄**（只看一个样本就断言「这个控件」的性质 ✓）。
   **对策（可推广）**：**当结论是关于「一类对象」时，先证明你观察了不止一个样本；样本数是 1 时，结论只能关于那一个** ✓
   **取证专项对策**：**截图给的结论必须回 DOM 验证（数量 · 边界 · 来源），而不是只验证那一个元素的存在** —— 那次**验了那个元素（294×32 读数正确）但没验数量** ✗。**⚠️ 风险**：**captain 是唯一能做视觉检查的**（成员无原生视觉）⇒ **他的误读没有第二个人能复核** ⇒ **这一类更危险** ✓






   **可推广的判断法（由「漏 / 滥」两端提炼，2026-09-23 captain 采纳）**：**判据 / 选择器的「对象集」是一个可以独立审查的东西** —— 对任何判据问两个问题：**① 它匹配到的东西，是不是全都是我想管的？**（防「**滥**」）**② 我想管的东西，是不是全都被它匹配到了？**（防「**漏**」）。**本代这两端都出现过实例**（滥 = `.row-btn > .muted` 顺手隐藏主标签；漏 = 命中数 0 静默通过）。
   **与第 6 条的关系**：第 6 条是**工具 / 契约两处真相不一致**；**这一条是同处真相内部「意图 vs 对象集」不一致** —— 都属「**不一致且无信号**」。







5. **把「键齐全」当成「文案齐全」** —— 与第 4 条是**同一族但不同部位**：第 4 条是**判据匹配不到对象**，这一条是**判据的对象集本身不全**（**键齐全 ≠ 文案齐全**）。两者都会让检查**报绿**。
   **实例**：本代反复引用的 i18n 证据是「zh 625 = en 625、单边 0、重复 0」—— 但那个检查**只数 `t()` 的键**；**绕过 `t()` 直接写死的用户可见字符串，它一个都看不见**。captain 用 `title|placeholder|aria-label` 扫 `panel/src`，查出 **17 处**非 `t()` 字面量，其中 **3 处是真问题**：
   - `App.tsx:313` `aria-label="toggle theme"` —— **图标按钮的可及名称**只有英文，而界面是双语的 ⇒ 读屏用户在任何语言下都只听到英文；
   - `App.tsx:291` `title="panel build"` —— 悬浮提示只有英文；且可见文本 `b {__BUILD_ID__}` 的**裸前缀 `b` 对读屏与非鼠标用户没有可及名称**（视觉模型在本代会话**每一张截图**里都把它读成一个无法解释的字形 —— 这是它 20 多次被读错的唯一字符串）；
   - `Board.tsx:424/433` —— **中文界面里的英文占位示例**（`Fix the login bug…`）。
   其余 **14 处是值示例**（`codex` / `codex --acp` / `default` / `glm-5.1, qwen3.7-max` / `—` / 品牌名 `ruagent`）⇒ **不应翻译**（占位符按规范应展示**真实示例模式**）。
   **对策**：i18n 证据必须**同时**给出「键齐全」与「**字面量清单**」两侧；扫 `title|placeholder|aria-label`（及可见文本）后**逐条分类**（值示例 / UI 文案 / 品牌名），**并明确声明哪一类不翻译**。
   **转交**：3 处真问题的修复 + 完整 17 处分类表 ⇒ **ui-shell**（t78 之后的任务；验收要求含「明确声明不翻译值示例」）。
   **⚠️ 一条空间约束（修 `App.tsx:291` 时必须一起满足）**：页脚有硬约束 —— systems 实测**七项自然宽合计 347.9px > 203px** 可用宽 ⇒ 现为**两行布局**（172.7 + 171.2 ≤ 203）。**若把 `b` 展开成单词，必须重测「不换行 / 不溢出」**（与 §12.9 的外壳常数同一片区域；行 40 的压缩致折行判据也覆盖这里）。
6. **工具领先于契约，且无人察觉**（原写法「检查的对象集超出契约」**归因不准**，见下）—— 与第 5 条**互为镜像**：
   - 第 5 条：**检查的对象集不全**（键齐全 ≠ 文案齐全）⇒ **假绿**；
   - 第 6 条：**检查的对象集超出契约** ⇒ **FAIL 无法被裁决** —— 契约里没有该行的**目标值 / 判定方法 / 例外规则**，因此**没人能说它该 PASS 还是该 FAIL**（是缺陷还是判据错，都无从判）。
   **共同点**：都源于「**工具与契约没有对账**」。
   **本代实例（2026-09-23 对账；归因已更正）**：工具侧 `n: N, title:` 共 **45 行**，MASTER §12 表到 **41 行** —— 缺 **42–45** 四行（`transition: all` · `img` 缺 `alt` · 字面三点号 · `tabular-nums` 覆盖率）。
**归因更正（captain，2026-09-23）**：先前被告知「42–45 是 t69 交付的」—— **错**。t78 用**时间戳证据**证明：`docs/screenshots/t77-run/metrics.json`（**t78 开始之前**）→ **41 行**；`t78-run`（本轮）→ **45 行** ⇒ **t69 只交付了 40 / 41；42–45 是 t78 按派单写的**。
**错误的机制（记档）**：captain 读 `design-audit.mjs:2901` 有 `n: 42` 就断定「早就交付了」—— **但他读的是文件的当前状态，而那个文件正在被他正在评判的那个任务修改**。这是「**用当前状态推断历史**」，与「在修复后的构建上复测」**同族**（**被评判的任务正在写那个文件，而我拿它当历史证据**）。**对策见第 3 条的新增动作（用冻结的时间戳证据，不读活文件）**。
**真正的风险（改写后的表述）**：**工具先行、契约后补是这个项目的正常流程**（t69 的契约就写了「点名 design-lead 补 MASTER 行」）—— 风险不是「工具领先」，而是**领先这件事没有被记录、也没有人被通知** ⇒ 于是那个 FAIL **无法被裁决**（没人能说它该 PASS 还是 FAIL）—— **这正是行 42 / 行 35 长期为红的处境**。
**对策**：**每次工具新增 / 删除行，必须与契约表双向对账** —— **工具→契约**：工具判的每一行都要有契约条目；**契约→工具**：契约的每一条都要有判据（含**反方向**：有没有「契约有、工具不判」的行）。**对账结果作为交付物**（一张表），归 **tools**（t78）。
**对账是「发现」机制，不是「防止」机制** —— 本次正是它**发现了** 42–45 的缺口 ✓ ⇒ 因此**必须定期跑**（**每次工具新增 / 删除行时强制**），而不是只在出问题时跑。
**反方向的好消息（t78 逐行核过）**：**「仅契约」= 0 行** —— **没有「契约承诺了但没人验」的行** ✓（行 28 的 `not_measured` 不算：它**仍在表内、有判据**，只是窗口不足时不判定）。
   **「更正的传播」也是对账的一部分（2026-09-23 captain 采纳）**：**工具里引用契约结论的措辞（自检标签 / 断言名）也是对账对象** —— 对账脚本应能核对它们与契约文本是否冲突。实例：tools 的自检标签写了「a JS-only 700 FAILS (the old wording could not catch this)」，而**旧措辞照样拒绝 700**（错源是 captain 口头的「更严」被传播）。⇒ **口头更正只在当场有效，记录更正只在记录被读到时有效** —— 这就是**为什么**要把工具措辞纳入对账，而不只是「应该纳入」。
   **对账的两个对象（横向 + 纵向，2026-09-23 captain 采纳）**：
   - **横向：工具 ↔ 契约**（本节主体）—— 工具领先于契约且无人察觉 ⇒ **FAIL 无法被裁决**；
   - **纵向：旧工具 ↔ 新工具** —— **新工具落后于旧工具且无人察觉** ⇒ **覆盖静默减少**，而**绿是默认状态**。
   **纵向实例（当场发生的）**：正在退役的**临时 validator 拥有一条新脚本没有的规则**（「表格列数一致」）—— 若新脚本一上线就删掉临时脚本，**这条规则就永久消失，且没人会知道它消失过**（新脚本会一直绿）。**它被发现纯属巧合**：我恰好在同一轮里把 @|css|@ 写进了表格单元格而漂了一次；**若那天我没漂，这条规则就无声地没了**。
   ⇒ **对策（与本条同构）**：**退役一个检查器之前，必须把它的规则集枚举出来，与替代者的规则集双向对账** —— 「**旧有、新无**」的每一条都要**显式决定**（迁移 / 判定不再需要并写明理由），**不得因为「新脚本能跑、且是绿的」就认为覆盖等价**。
   **「差集」本身也是自描述计数（2026-09-23 记档）**：**它必须由枚举产生，不能由回忆产生** —— 我上一轮**凭印象**报「差集一条（表格列数）」，本轮**逐条枚举**旧规则集后实际是**两条**（+ **编号单调**）。**若只按「新脚本能跑、且是绿的」判断（也就是不枚举），两条规则会无声消失，而且你会以为只丢了一条。**
   **本次退役记录（可追溯 —— 否则下一个读到「差集 = 0」的人不知道差集是对谁算的）**：旧检查器 **`validator.py`**（规则：表格 pipe 数一致 ⇒ 规则 6）与 **`contract-check.py`**（规则：自描述计数一致 ⇒ 规则 4 · 粘连（行首定义）⇒ 规则 1 · §12 表行连续 ⇒ 规则 3 · **编号单调 ⇒ 规则 7**）**已于本轮退役，其规则集 5 条全部迁移**；**枚举后差集 = 0**；`--self-test` **10/10**（新增夹具 ⑨：`1,2,4` 跳号 ⇒ **必须 FAIL**）。**「若只按新脚本能跑且是绿的判断，这条就无声地没了」** —— 这句比「已迁移」更能说明**为什么**要做这件事。

   
   **第三个方向：指令 ↔ 契约（2026-09-23 captain 采纳）** —— 与横向（工具 ↔ 契约）、纵向（旧工具 ↔ 新工具）**同源**：**两处真相不一致，而没有任何信号**。
   **实例（t88）**：captain 的指令（默认显示全部 34 条）**覆盖了** `view-chat.md` 既有的「单组折叠 **5 行**」；**ui-work 照做（正确 —— 本代约定指令是权威的）**，但它在 output 里**点明了 N=5 的契约出处** ⇒ 这就是最好的处置：**照做 + 标明冲突**。后果链完整：**指令 → 全量平铺 34 条 → 用户改主意 → 回到契约的 5 条**。
   **可推广的规则**：**当一条指令与既有契约条款冲突时，执行者应在 output 里标明「此指令与 §X 的 Y 冲突，我按指令执行」** ⇒ 冲突**留下痕迹**，而不是被静默覆盖。
   **对策**：**指令若覆盖契约，必须在落点（output / 契约本身）留一行说明。**

   **撤回一条裁决时（2026-09-23 实例）**：**必须写成「若你已经落了，把 X 改成 Y」—— 即撤回必须能在事后被直接执行**；更好的是**明说「这条可能与你正在做的事交叉」**（那次的撤回与我的落表**同时发生** ⇒ 我落了上一版，靠「具体改法」才在同一轮内改完 ✓）。⇒ **撤回也是指令：它必须携带可执行的落点。**

   **派生方向（2026-09-23 captain 采纳）：凡「工具里有、契约里也有」的数，工具应从契约读出，而不是两边各写一份。**
   **理由**：两边各写一份 ⇒ **将来一边改了另一边不改** ⇒ 正是本条的形态（工具 ↔ 契约不一致）；**从契约读出把它变成结构上不可能** ✓
   **与第 2 条的关系**：第 2 条是「**代理量应换成结构量**」；**这一条更具体 —— 派生的方向应当是「工具从契约读」**（**契约是权威**）✓
   **与总纲的关系**：这是「**改产物而不是要求人记得**」在工具 / 契约边界上的形态 —— 靠人记得「两边都要改」不是对策 ✓
   **本代实例**：§12 **行 46** 的下限乘数 **5** —— 它**不是工具里的硬编码**，而是从 MASTER 目标列用锚点 `/width\s*≥\s*(\d+)\s*×\s*fontSize/` 解析出来的，并用自检钉住（`row46.parse('…width ≥ 5 × fontSize − 0.5…').em === 5`）✓ ⇒ **契约改了，工具自动跟随** ✓。**反例（尚未做，作为适用范围说明）**：`--self-test` 的用例数 / 行数之类若在两处各写一份，同样会漂 ✓
   **配套形态（同源）：阈值是派生的（不可漂），边界是可见的（读数）。** 行 46 的 display **逐路由打印实测最小值 + 字号 + 该元素自己的 computed `min-width`**（`knowledge` 390×900 = 0px / fs14 / 声明 min —）⇒ **`#sessions` 的 41px 即使 PASS 也出现在读数里** ⇒ **阈值用派生保证不漂，边界用读数保证可见** ✓

   **验收文本规则（2026-09-23 captain 采纳）：任务验收中的审计项必须写成「相对该任务自己的基线不新增 FAIL」，且基线必须在任务开始时实测并记录 —— 禁止写死一个全局的 PASS / FAIL 数字。**
   **实例（同一天出现两个「都对」的基线）**：ui-work 记 `PASS=40 / FAIL=0`、ui-list 记 `41 pass / 6 not_measured / 2 FAIL` —— **两者都对**（工具在长：行 50/51/52/53 陆续加入 ✓）⇒ **写死计数的验收会随工具的成长而失效** ✗。
   ⇒ **这是「阈值是派生的，不可漂」在验收文本上的应用**：**验收里的数字必须相对基线，不能是绝对常数** ✓
   **同族补充（本机绿 ≠ 流水线绿）**：本轮出现两类「**只在干净环境暴露**」的缺陷（**model 字段为空** · **空态按钮导致严格模式违例**）⇒ **本机绿不等于流水线绿** ✓ ⇒ **修复单必须用 `panel/tools/repro-ci-e2e.sh` 验收**（已由 captain 变成机制，不是提醒 ✓）。




**为什么这一类值得单独记**：第 4 / 5 / 6 条判的都是「检查器的对象集与真实意图**不**对齐」；**这一条是它的时间维度版本** —— 对象集**曾经**对齐过，**在替换的那一刻起不再对齐**，而**没有任何信号**。

   **时间维度的第二个实例（2026-09-23）：两次读数都正确，错的是「它们是同一时刻的读数」这个隐含假设。** 同一个元素（chat 侧栏的 `.time`）：**22:30（verifier run ①）** `white-space: normal` ⇒ **2 行折行**（28×45）⇒ 行 40 **FAIL** —— **当时是对的**（`.truncated` 还没加到 `.time` 上）；**00:10（systems 核查）** `cls="time truncated", ws="nowrap", w=14, h=15` ⇒ **1 行** —— **现在也是对的**（t87 在 ~23:5x 加的）。**两次相隔约 90 分钟，中间发生了 t87** —— 而「两人读数不一致」**看起来像是有人错了** ✗
   **对策**：**当两次对同一对象的测量不一致时，先问「它们是不是在同一时刻测的」** —— **记录里必须带时间戳**（本代先例：mtimes 不可信 ⇒ 用内容断言 + 带时间戳的冻结产物）。
   **附带价值（值得单独一句）**：systems **没有停在「verifier 读错了」**，而是去查**它读的是谁** ⇒ 发现真正的另一半在别处（共享 `.time` 在 `#sessions` / `#board` / `#memory` / `#home` 上仍是 `normal`，实测 **20 / 21 / 9 / 11** 个元素）⇒ **一个看似「有人搞错了」的分歧，最后产出了一个更宽的修复**。
   **这个形态**：**分歧不是噪音，是「两处观察不一致」的信号 —— 先查它是不是时间差，再查是不是对象差；两者都排除了，才是真错。**

   **第三个实例（2026-09-23，用户报告）：「因果相邻」被读成「因果相连」。** 用户报「点击会话把它排到最上方」，**罕见且难复现**；排查后：**排序没有任何写入**（点击路径不写 `updated_at`）、**候选「折叠展开造成位移」与「排序键并列」都被证伪** ⇒ **真凶是轮询每 3–5 秒整表替换，而期间有新会话产生 ⇒ 已有行整体下移一位**，而**它常常与用户的点击在时间上相邻** ⇒ **被读成一个导致另一个**。**形态**：**两件事时间上紧邻，就被读成因果** —— 而**代码检查看不到它**（没有写入、没有排序变化）⇒ 与本节同族：**分歧先查时间差、再查对象差；两者都排除了，才是真错。**

   **与第 8 条的关系**：第 8 条的前提错是「**字段语义**没验」（`active` 表示什么）；**这一条的前提错是「时间被忽略了」** ⇒ **同类不同源**。

7. **裁决被字面实现，而它的效果 ≠ 它的意图** —— 与第 4 / 5 / 6 条不同：那些是「**判据**与意图不对齐」；**这一条是「裁决」与意图不对齐**，而且**发现它的方式是「看效果」，不是「看实现」**。
   **实例**：行 45 的范围裁决（@grandPeer≥2@ = 「祖父层子树里有 ≥2 个数字」）**被如实落地了**，读数也确实变了（@#home@ 覆盖率 83.3% → 90.9%）—— **但只排除了 4 处里的 2 处**，因为那个**字面定义**把 @section.zone@（**区块**，还装着标题 / 栅格 / 图例）也算成了「组」。**意图是「对齐的数字列」，字面定义却把「区块」算了进去。**
   **关键的一点**：tools **报告了偏差，而不是宣布完成** —— 这正是它能被发现的原因。
   **对策**：**验收一个裁决时，要核「它是否产生了裁决想要的效果」，而不是「它是否被字面执行」** —— 两者不同，且**后者很容易看起来成功**（读数变了、脚本绿了、条目落了）。
   **与纵向对账的关系**：那条是「**覆盖**在替换时静默减少」，这一条是「**意图**在落地时静默偏移」—— **都发生在没有任何信号的时候**。
   **补充实例（2026-09-23，**在落表时就发生**）：措辞本身可以造成字面实现与意图的偏离。** 行 45 的范围我写成「数字组 = 一个『**直接子元素全部是数字**』的容器」—— **意图**是「对齐的数字列」，**字面实现却会把读数栅格本身排除掉**（`div.readout-row > div.readout-cell > span.readout` 的直接子元素是 `div.readout-cell`，它们自己不是数字 ⇒ 栅格全部出局）。**tools 按意图实现并报告了偏差** ⇒ 改为「**每个直接子元素各含一个数字读数**」。⇒ **发现方式是「实现者按意图做并报告」，而不是「看实现」或「看效果」** —— 这是本条的第 3 种发现路径（前两种：看效果、看偏差报告）。
   **一条推论（tools 原话，2026-09-23）**：**「『预期达成』和『实测达成』是两件事，而『实测达成得过于顺利』本身是个信号。」** —— 两次应用：上轮报**实际 43/1/1 vs 预测 44/0/1**（**不迁就预测**）；这轮跑出 44/0/1 **但没有直接交**，而是去核「**为什么突然全绿**」⇒ 抓到假绿。**第 2 次更难，因为数字恰好等于预期，交出它是最自然的动作。**





**当前状态（2026-09-23 更新）**：**行 42 / 43 / 44 已落表**（captain 裁决后，条目含目标值 / 判定方法 / 例外与有意排除 / 与 must-FAIL 自检的对应说明）。
   **行 42 的例外口径已裁**：排除 `owner === antd-cssinjs*`（t78 数据：**81/81 命中来自 antd-cssinjs**，0 来自 `link:index-*.css`）。captain 原先的候选「`ant-` 前缀且不含项目自有 class」**被数据推翻** —— 81 命中里 **53 个同时带项目自有 class**（`.ruagent` ×53、`.kanban-runs` ×21）⇒ 那条**一个都排除不掉、还会误伤自己**。
   **行 45 暂不落表**：它**现在 FAIL**（覆盖率 83.3% / 85.7% / 50.0%）；captain 已向 tools 要「未覆盖读数逐条清单」以区分「数字列 / 对照（应补）」与「时间戳 / ID / 纯装饰数字（**不该**被该判据要求）」，**看到清单再裁「补类」还是「写进有意例外」**（那决定条目文本怎么写）。
   **行 35 的 `runtime(antd)`** 仍在 **t81** 的数据上待裁 —— 依据第 4 条：**不得作为「已知 pre-existing」无限期挂着**。
   **自检脚本的归属（captain 2026-09-23 裁决）**：契约侧自检脚本落在 **`docs/design/`**（**与被检查的契约同处** —— 理由：**检查器必须随契约一起演进，放远处必然腐化**；契约本身是 design-lead 的产物）。交付于 **t82**（依赖 t80）。
   **接口要求**：提供 **`--json`** 或等价输出格式，供 **tools 的双向对账**（本条对策）调用；**本任务不调用审计工具** —— 两边各自有归属，**由对账连接**。
   **它必须真的在看会漂的东西**：至少覆盖 **自描述计数一致 · 标题层级（父标题存在 + 无 `---#` 粘连）· §12 表行连续 · 表格 pipe 数一致**，并带 **must-FAIL 自检**（注入违规必须 FAIL ⇒ 证明判据确实在看那个对象，见第 4 条）。

**总纲（2026-09-23，captain）：对策若依赖人记得，它就不是对策，只是提醒；真正的对策要改产物，让正确的做法成为默认。**

**本代有效的对策全都是这个形态（逐条数）**：行 43 把「**0 个对象**」与「**已验证合规**」**写成两种记录**（而不是要求读者记得区分）· 行 42 **把排除数打印出来** · `checkPipes` 带 **`\|` 转义提示** · `maskRefs` **保留换行**以免行号漂移 · 对账断言改成**双向**（「工具领先于契约」自己会让自检变红）· `check-contract.mjs` 把 §12 表的 **`rows/max/gaps` 作为输出**（让对账可机器做）· 契约表把**陈旧读数保留为历史快照并标注轮次**（而不是删掉）· **`--json` 形状写进头注释** ⇒ **共同点：它们都不依赖任何人「记得」。**

**而本代被判过的失败，全都是「依赖人记得」的产物**：S6 的 4 · K5 的 12 · `kbd-hint 26×45` · captain 预测的 **44/0/1** · 我凭印象报的「差集一条」。

⇒ **判断一条对策是否成立的方法**：问它**改的是产物还是人**。

   **⚠️ 对策本身也会「依赖人记得」—— 递归一层（2026-09-23 实例）**：为了消除「靠人记得登记块头」，本代建了生产者常量 `INJECTED_HEADERS`；**而实测发现 6 处发出方（`chat.rs:141/175/441/562` · `runs.rs:762/764/1702/1723`）全部内联字面量、根本不读该常量** ✗ ⇒ **光把常量补到 4 条不够：发出方与常量是两份字面量，下次新增块头还会漏** ✓ ⇒ **这正是本条总纲本身的一个实例**（**对策若依赖人记得，它就不是对策**），而且**发生在用来消除「靠人记得」的那个常量身上** ✓
   **修法（已开 t124）**：**每个块头一个具名常量**（`HDR_ROLE` / `HDR_MEMORY` / `HDR_RESUME` / `HDR_RETRY`）· **`INJECTED_HEADERS` 由它们组成** ⇒ **集合与字面量只有一个来源** ✓ · **6 处发出方全部改用具名常量** ⇒ **新增块头 = 新增一个具名常量，而它天然就在集合里**（**不需要有人记得去登记** ✓）· **可判定项：mock 测试断言「实际发出的 prompt 的块头 ∈ `INJECTED_HEADERS`」并覆盖全部 4 个块头** ⇒ **新增块头若不登记，测试就会红** ✓ —— **这才是把「靠人记得」变成「测试会红」** ✓
本代 7 条问题里，凡是落成「改产物」的都**生效**了；凡是落成「提醒」的，**都又漂了一次**。
8. **裁决的前提本身是错的（比第 7 条更早一步）** —— 第 7 条是「裁决被字面实现、**效果** ≠ 意图」；**这一条落在第 7 条之前**：**契约里的判据可以建立在一个未经验证的前提上，而它的文本完全可判定、也完全自洽。**
   **实例（2026-09-23）**：@view-chat.md@ 的 **C5②**「@active@ 给可见标记」读起来**无懈可击**（可判定、有出处、还带了补充限定），但它**依赖「@active@ 表示正在生成」这个前提，而那个前提是错的** —— 实测：**不挂 SSE** 起一段会话，回复已到（@message_count@ **2 → 4**），而 @active@ 数分钟后仍为 **true**；代码证据 @chat.rs:1063@ + @chat.rs:218@ 的文档原话 + @chat.rs:1124@ 的空闲回收器 ⇒ **@active@ 是生命周期，不是运行状态**。**若照此落地，标记会失去区分力**（这段生命周期内用过的会话都亮）—— 即「**看起来实现了、而它没有区分力**」。
   **发现方式**：**在实现之前，由 captain 直接测了那个前提**（起一段会话、不挂 SSE、看 @active@ 何时变 false）—— **不是靠读契约、也不是靠读代码**（**读代码其实也能看出来**：@live.contains(&e.id)@ 与文档原话都在那儿，**但没人去读它**）。
   **对策**：**契约里凡是「用 X 表示 Y」的判据，必须写明 X 的语义出处（哪个字段 / 哪行代码 / 哪条注释），并且在落地前验证那个语义** —— **「可判定」不等于「前提成立」**；**一个可判定但前提错误的判据，比一个模糊的判据更危险**（因为它看起来已经想清楚了）。

   **可操作子类（2026-09-23）：跨档位搬值前必须核对「成因」** —— **纪律**：**「从 A 档的实测值推出 B 档的上界」时，必须同时核对「让 A 档取该值的那个条件，在 B 档是否也成立」** ✓
   **可勾选步骤（三步）**：
   1. **写下 A 档取该值的「成因」** ✓ —— 例：**520 档 60px 的成因是内容盒 ≈384 装得下 113 + 77 + 132 + gaps ≈ 358 ⇒ 1 行** ✓
   2. **逐条核对该成因在 B 档是否成立** ✓ —— 例：**390 档内容盒 254** ✗（**装不下 290（2 控件）、也装不下 358（3 控件）⇒ 必然 2 行**）⇒ **成因不成立** ✓
   3. **若成因不成立 ⇒ 该值不是上界，只是「A 档在那个条件下的观测值」** ✓
   **本代实例（行 54 判据 ⑥ 的修正史）**：原推导「60px 来自 520 档实测值 ⇒ 作为 390 档的上界」**漏掉了「内容盒宽度」这个条件** ⇒ 订正后 **390 档 114px 是内容集决定的最小值、不是缺陷** ✓（**写在这里是为了下一个人不会重新提这件事** ✓）
   **⚠️ 与两条既有规则的区别（不同，别混）**：
   - 与「**阈值是派生的，不可漂**」的区别：那条讲**阈值的来源**（工具里有、契约里也有 ⇒ 应从契约读出 ✓）；**本条讲跨档位 / 跨条件搬值时的核对** ✓
   - 与「**从 n=1 推广**」的区别：那条讲**样本数**（结论关于一类对象时须先证明观察了 >1 个样本 ✓）；**本条讲跨条件的搬用**（**样本可以足够，但条件变了**）✓


   **第二实例（2026-09-23，captain 自报）：前提来自「上一轮的转述」，而不是本轮的原始来源。** t87 把「**全部会话**」当成用户要求写进契约执行，而**用户本轮的原话是「默认前几条 + 显示更多」** ⇒ **需求在两轮之间变了，而上一轮的转述被当成了同一件事**。**它与本条的机制同族但不同**：本条首例是「**字段语义**没验」（`active`），这一例是「**需求来源**没回核」（拿上一轮的转述当本轮的需求）。**对策**：**凡「用户要求」类前提，必须回到用户原话 / 本轮指令去核，不得沿用上一轮的转述**（转述是**派生品**，不是来源）。

**契约写法规则（由 C2 的改写提炼，2026-09-23 captain 采纳；**预防性**，不是失败类 —— 故不编号）**：**契约里的「必须显示 X」在空间受限时容易变成必然 FAIL —— 应改写为「必须可识别 X + 明确路径」，除非你能证明 X 在任何档位都放得下。**
   **实例（C2）**：原写「每条会话行**必须显示**它自己的 agent」——**实测在 1440 就冲突**（侧栏客户端宽 **237px**、行内容盒 **125px**（减去两个行级动作按钮 74px 与 gap）；标题 `cw=0 / sw=65` · agent 标签 `cw=32 / sw=53` · 时间 `cw=28 / sw=47` ⇒ 三者需求 **165px > 125px**）⇒ **冲突是结构性的（侧栏 228px 是冻结的 e2e 契约），不是视口特有的** ⇒ **视口分档会是「假精确」**（把一个到处都发生的冲突写成只在窄屏生效的例外）。**改写后**：agent 标签**可截断 / 可省略**，但**该行必须仍可识别 agent**，且**至少两条路径**（行内 `title` 悬停可见完整名；选中后会话头部显示该 agent）⇒ **契约不再是必然 FAIL 的条款，而识别性也没有被放弃（只换了路径）**。

**注释写法规则（2026-09-23 captain 采纳；同为**预防性**、不编号）**：**注释里除了「为什么这样做」，还应写下「为什么不采用那个更显然的做法」** —— 因为**后者才是下一个人会去改的地方**。
   **三例同形（否定的对象都是一个「更省事、看起来也说得通」的替代方案）**：① **t92 的轮询注释**（**为什么不能只轮询「已知在生成」的行** —— 那永远发现不了**开始**生成；并写下代价：空闲 ≤12 req/min、生成中 ≤20）② **本脚本的「为什么不用魔数」**（那会让插入内容悄悄把表挤出窗口，而报出「无法派生」）③ **行 42** 的「已排除 antd 的 **199** 处（**不是「已合规」，是「不是我们的」**）」（明确否掉那个更容易的误读）。
   ⇒ 与总纲的关系：**这是「改产物而不是要求人记得」的另一半** —— **把理由放进产物里，并把「更显然的替代方案」一并否掉**（否则下一个人会「优化」回那个替代方案）。

   **三件套（2026-09-23 captain 裁决：第三件必须写，且它是唯一在「防守」的那一件）**：**为什么这样做**（意图）· **为什么不那样做**（排除更省事的方案）· **这样做要付什么**（**为当前选择辩护**）。
   **理由比「t92 已经这么做了」更硬**：**「为什么不那样做」只说明了「那样做不行」；「这样做要付什么」才说明了「这样做值」** —— **若缺第三件，前两件会一起失效**（下一个人看到的是「这个方案更贵，而且没写为什么值」⇒ **他会换回去** ⇒ **你写的第二条也被无视了**）。⇒ 所以它不是「多写一条」，而是**让前两条站得住的那一条**。
   **受众**：**注释是产物的一部分，但它服务于「下一个编辑者」这个读者** —— **不是解释给用户看，是防守给下一个人看**。
   **规则来源（本代的写法）**：**规则的来源应写明「来自哪个实例」** —— 本条的三例已照录 ⇒ 读者能判断它的**适用边界**，而不只是记住一句话；**这比「先立原则、再找例子」更可靠**（本代已判过多次：凭印象 / 凭推演定的规则容易漂）。
9. **产品自己报出的状态与真实状态不一致（第四步：报告）** —— 第 1–8 条的**对象是判据 / 选择器 / 前提 / 工具与契约**（**做判断的东西**）；**这一条的对象是产品自己报出的状态**（**做展示的东西**）⇒ **对象不同 ⇒ 单列**。**它的两面属于同一条**：**成功被谎报** 与 **失败被谎报成成功** —— 这与第 4 条的**漏 / 滥**是**同一个结构**：**一类东西的两端失配，两端都无信号**。
   **实例一（成功被谎报）**：会话删除按钮写「删除」、文案写「**不可撤销**」，**而删除会自我撤销** —— 索引器每 **60s** 用 `INSERT OR REPLACE` 重写 `sessions`，**把行扫回来** ⇒ **界面在说谎**。（当时的修法：墓碑侧表 `0016_session_archives` / `0017_session_deletions`。）
   **实例二（失败被谎报成成功）**：乐观移除后**失败却不回滚、不给错误、停在 pending** ⇒ **用户以为删掉了**。（修法：`view-chat.md` §9.4 **D3** 三条同时满足：行必须重新出现 · 必须出现可见错误 · 不得停在 pending。）
   **两端都无信号**：**审计全绿、e2e 全绿** —— 因为**没有任何判据看着「界面显示的状态是否等于真实状态」** ⇒ 与第 4 条同源：**不是判据判错了，是没有判据看这个对象**。
   **对策（一般化）**：**凡「界面先于结果改变」的乐观更新，必须定义「结果失败时如何回到真实状态」，且该回退本身必须是可见的** —— 因为**乐观更新的默认失败形态就是「界面停在一个从未发生过的状态」**。
   **与第 2 条同族**：**乐观更新是把「真实状态」换成了「预期状态」—— 一个代理量**；**代理量必须有回退路径**（第 2 条：代理量应换成结构量；这一条：**换不掉的代理量必须有回退**）。







**六例的修法都不动数值**（行 1 口径 / 行 37 来源 / 行 30 基准 / 行 18 区域 / 行 28 范围 / 行 27 分段）—— **要修的始终是判定范围，不是门槛**。

**转交清单（t44 产出，按归属）**

| # | 事项 | 归属 |
|---|---|---|
| ① | 行 28 判据改为「逐端点 + 外壳分离」，`/permissions` 独立断言 ≤7 | **tools** |
| ② | 行 39 新增 + 实现（需要端点归一化分组算 `K`） | **tools** |
| ③ | 行 27 的 `manualChunks`（vendor/app/markdown）+ 工具按 chunk 判定；落地前行 27 = `not_measured` | **build + tools** |
| ④ | `Home.tsx` 去掉第二条 `/permissions` 调用（否则行 28 独立断言在 `#home` 上 FAIL） | **ui-shell** |
| ⑤ | `index.css`：`.brand-*` 优先级 + `.runtime-logo` 描边 alpha `0.97`（R8） | **systems（共享层）** |
| ⑥ | `--graph-protocol` 换非暖色相（`view-graph.md` G4，t13 的 F3；裁决时 `#ca9d33`，且有 1 个 protocol 实体实际渲染） | **systems（共享层）—— 已由 t45 落地（2026-09-22）：暗 `#8fa3e8` / 亮 `#5b6fc4`；规格文本由 t51 同步** |
| ⑦ | `#graph` 首屏请求数回归 | **ui-work（t42 已达标：1 ≤ G15 的 8）** |

---
### 12.11 两项裁决（2026-09-23）：画布选中环进白名单（W7）+ 品牌标记的对比度基准是「自身底色」

**来源**：t46（round-2 定稿验收，0 FAIL）报出的两条实测新发现。两者都**不改变已通过的判定**，但都是真实的契约边界问题。

#### 12.11.1 发现 1：图谱画布的选中 / 搜索命中环用了信号色 —— 裁决「补白名单」

**事实（t46 实测 + 本处复核代码）**：`Graph.tsx:623-628` 在 `n.id === sel || hit` 时画 `lineWidth 2` 的外环（半径 `rr + 3`），颜色 `pal.ring = --ant-color-primary`（`readPalette` `:390`）—— 本系统里 `--ant-color-primary === --signal`（§12 行 32「刻意设计，保留」）⇒ 暗色下就是 `#f0a93b`。可达性已实测：30 点网格点击，2 次点中后画布出现琥珀像素 **190 / 124 px**。
**措辞更正**：这不是「选中 / **悬停**环」—— 悬停（`s.hoverId`，`:587`）只影响**标签**（`:630` `n.id === focus`），**不画环**。W7 的措辞按实际用途写为「**选中环 / 搜索命中环**」。

**裁决：补进 §3.2 白名单，新增 W7**（不改色）。理由三条：
1. **语义同类**：白名单第一条 W1 就是「**当前**导航项」—— 一个**选中态**而不是运行状态。「现在」这一类本来就有「你在哪 / 你选了什么」的含义；画布选中环是它在 `#graph` 上的同类，**不是新语义**。
2. **系统既有事实**：W4 已允许 accent 用于「手写 `:focus-visible` 环」与 `.composer:focus-within` 边框；antd 自身的焦点环（F-7）仍是 accent 派生且**刻意保留**。只改画布环而保留其余 accent 环，会造成「同一语义（当前交互目标）两种颜色」。
3. **不会造成假信号**（色相复算，9 个分类色逐个）：信号 `#f0a93b` H=**36.5°**；**9 色无一落在 §3.2 的琥珀带 [20°, 50°] 内**，最近的 `--graph-person` `#eb827b` H=**3.7°**（Δ=**32.8°**）、次近 `--graph-tool` H=122.3°（Δ=85.8°）；且 `#graph` 视图内**没有**「运行中」这个状态可被误读；环是**瞬态**的（选中 / 命中脉冲 2.4s）。
**边界重述**：见 §3.2 新增的「信号色的使用边界」段 —— 含义集不变；新增三条硬约束（只表达「现在」/ 不得是唯一线索 / 不得进入数据可视化与常态装饰）；并明确 **W7 与行 32 无关**（行 32 判令牌值，W7 不动任何令牌），只影响 §3.2 的像素上限（实测 0.015% ≪ 1.5%）。

#### 12.11.2 发现 2：亮色 claude 品牌标记对**自身 10% 底色**只有 2.81:1 —— 裁决「基准 = 自身底色」，底色 alpha ≤2%

**① 基准裁决：分两类**
| 元素 | 基准（相邻色） | 依据 |
|---|---|---|
| **描边**（`.runtime-logo` 的 1px 边框，**边界类**） | **卡片底**（组件**外部**相邻色） | WCAG 1.4.11 的通行做法：**组件边界**要求与**周围背景**可辨；R8 的原文判据也是「描边对底」（t44/§12.10.4 落槌为 ≥3:1，实测亮 3.010 / 暗 5.108 ✔）。**不变**。 |
| **标记**（品牌 SVG，**内嵌图形**） | **标记自身的底色**（紧邻像素） | WCAG 1.4.11「against **adjacent** color(s)」的**字面**读法 —— 标记嵌在 tile 的淡染底上，紧邻它的是那块底色，不是卡片。primitives 也把「非文本**必需**图形（控件边、状态点、焦点环、图表线、脊线）」统一挂在 3:1 上（§11.1 表），标记是**识别运行时**的图形，按同一档判。 |

⇒ t46 的读数（亮 claude **2.81** / deepseek 3.03）在**新基准下是 FAIL**，必须修 —— 但修的是**底色**，不是品牌色（品牌色是域②身份，不可动）。

**② 修法：降低 tile 底色 alpha（而不是改品牌色 / 改描边）**
合成模型：`底色 = a × 品牌色 + (1−a) × 卡片底`（CSS 是 `color-mix(in srgb, currentColor a%, transparent)`，即品牌色在卡片上的 alpha 合成）。**对比度随 a 单调下降**（底色越靠近品牌色，与标记的差越小）⇒ 只能**下调 a**。实测扫描（本处独立复算，三品牌 × 两模式）：

| a | 暗/claude | 暗/deepseek | 暗/opencode | 亮/claude | 亮/deepseek | 亮/opencode | 最小 | 判定 |
|---|---|---|---|---|---|---|---|---|
| **0.10（现状）** | 4.680 | 4.355 | 10.467 | **2.821** | 3.025 | 15.191 | 2.821 | ✘ |
| 0.06 | 4.954 | 4.604 | 11.732 | **2.939** | 3.157 | 16.553 | 2.939 | ✘ |
| 0.04 | 5.089 | 4.727 | 12.376 | **2.999** | 3.224 | 17.262 | 2.999 | ✘ |
| **0.02（新值）** | **5.220** | **4.848** | **13.017** | **3.060** | **3.293** | **17.988** | **3.060** | **✔** |
| 0.00（无底色） | 5.348 | 4.966 | 13.645 | 3.122 | 3.362 | 18.734 | 3.122 | ✔ |

**临界值（解方程）**：亮色 claude 是唯一约束 —— 要求 `(L_底 + 0.05) / (L_品牌 + 0.05) ≥ 3`，`L(#d97757) = 0.286362` ⇒ 需 `L_底 ≥ 0.959086` ⇒ **a ≤ 0.03966（3.966%）**；deepseek 的临界是 0.10（即 10% 时 3.025 贴线）。**取 a = 0.02**（余量：最紧一格亮 claude **3.060**，距门槛 +0.060）。
**0%（删除底色）同样合规**（亮 claude 3.122，余量最大）—— 判据只要求 **≤2%**，两种落地任选。
**代价**：2% 的品牌淡染在视觉上几乎不可见 ⇒ 落地时「直接删掉底色」是等价且更简单的选择；tile 的身份由 **97% 品牌描边（≥3:1 ✔）+ 品牌标记**承担，不依赖淡染。

**③ 影响面**：只改 `.runtime-logo` 的 `background` 一处；**不改** 尺寸、描边、标记色；行 5（可见描边元素 ≤40）与行 4 不受影响；R8 的**描边**判据不变（对卡片 ≥3:1）。
**④ 转交**：`index.css` 的 `.runtime-logo { background: color-mix(in srgb, currentColor 10%, transparent) }` → alpha **2%**（或删除该行）—— 归 **systems（共享层）**；`view-runtimes.md` 的 R8 判据文本由本任务同步（见下）。

**⑤ 登记：同类风险已清点** —— 全仓还有没有「内嵌图形 + 品牌/信号淡染底」的组合？`grep color-mix` 命中的淡染只有 `.runtime-logo` 一处（`.tag` 的变体是**实色**底，t2 已钉死 5.97/5.89/5.11:1 ✔）；`.agent-avatar` / `.stat-num` 等无淡染底 ⇒ **无同类活例**。

---

### 12.12 具名项的「症状 vs 常数」复核（2026-09-23 裁决）

**来源**：captain 的视觉巡检发现 §12.9 的外壳具名闭集把 `button.kbd-hint` 的 **26×45** 当成了设计常数 —— 而 26×45 恰恰是**折行缺陷的症状**（26 宽 = 被 flex 压缩，45 高 = 15×3 三行）。审计因此把一个布局缺陷认证成常数，行 18 一直通过。

**① 判据：具名项分四类（可判定）**

| 类 | 定义 | 能否当常数 | 可判定测试 |
|---|---|---|---|
| **① 设计常数** | 尺寸由 token / 规则**钉死**，与内容无关 | ✅ 可只查一次 | 改内容、改容器宽度，尺寸不变 |
| **② 内容驱动** | 尺寸由内容 / 行数决定，但**无布局事故** | ⚠️ 可具名，但必须标注「随内容变化」，**不得跨轮当常数** | 改内容尺寸随之变；无折行 / 无压缩 |
| **③ 症状** | 尺寸是**布局事故**的产物（折行 / flex 压缩 / 溢出裁切 / 字体派生） | ❌ **不得具名，必须修** | **自然宽度测试**：单行自然宽度 > 元素宽 + 2px ⇒ 容器给少了宽度 ⇒ FAIL（**2026-09-23 更正**：原写「关闭压缩源后尺寸显著改变」，该测试**有盲区** —— 在**子项**上置 `flex-shrink: 0` / `min-width: max-content` 不能解除压缩，父项的 `min-width: 0` 仍然赢，一个真实的 9 行缺陷会被判 PASS；见 **§12.14.1**） |
| **④ 陈旧** | 曾具名、**已修复**但契约表未更新 | ❌ 必须标注日期与修复来源 | 按现测重跑，数字与表不符 |

**② 逐项复核（§12.9 的每个具名项都给依据与现测；1440×900/950 暗色）**

| 具名项 | 现测几何 | 类 | 依据 |
|---|---|---|---|
| `button.kbd-hint`（外壳） | 现状 **26×45**（3 行）；关压缩源后 **65×15** | **③ 症状** | 高 45 = `line-height 15 × 3`；`.sidebar-foot` 是 flex 行、`white-space: normal` + `flex-shrink: 1` ⇒ 被压到 26 宽竖折。**关闭压缩源后 65×15**（尺寸显著改变）⇒ 不是设计值。**并暴露第二个问题：单行后 15 < 24 地板** ⇒ 修法必须同时钉 `min-height` |
| `.ant-switch` 48×24（settings） | 48×24 | **① 常数** | §12.7.2 裁决 pin 了 `trackHeight 24 / trackMinWidth 48`；改内容不变 |
| `.chat-side-head` 图标按钮（chat） | 24×32 | **② 内容驱动** | `padding: 0 4px` + 16px 图标 = 24 宽；`flex-shrink: 0`、无折行、无压缩 ⇒ 合法。**但宽度与图标尺寸耦合** ⇒ 标注「随图标尺寸变化」 |
| `<details> > summary` ×8（task） | 1109×24 / 403·91×25 / 230–246×24 | **② 内容驱动** | 原生 disclosure 的命中区 = 文本行高；**随数据行数变化**（跨轮比较须固定数据基线） |
| `.ant-input-sm`（chat 搜索）**187×21** | `offsetParent = SPAN.ant-input-affix-wrapper`（**237×32**） | **非集合成员**（复合控件**内部部件**） | 21 是内部 `<input>` 的字体派生高；行 18 的口径「antd 嵌套取**最外层**控件根」正确地把它排除 —— **控制根是 32 高的 wrapper** ✔（**不是漏检**） |
| `.ant-select-input` ×N（chat/task） | 24 高 | **非集合成员** | 同上：控制根是 `.ant-select-selector`（32） |
| runtimes 3× 动作按钮 31×32 | **现测 0 项** | **④ 陈旧（已更新）** | t36 抬到 32×32；§12.9 ⑤ 的表已在 2026-09-23 更新为 **0** ✔（本轮复核确认，无需再动） |

**③ 动作**
1. **§12.9 ② 的外壳闭集**：`26×45` 已标注为 **③ 症状**，期望改为**修后 0 项**（依赖 t66 的 `white-space: nowrap` + `flex-shrink: 0` + **`min-height: 32px`**；三种情形的实测几何已写在该代码块里，可直接判）。
2. **§12.9 ③ 的 (b) 条就地更正**：它的前提（26 是设计尺寸）错了 ⇒「≥32 只能靠加横向 padding 凑数」的推论随之失效；正确路径是**先修折行、再把命中区高度钉到与同排兄弟控件一致的 32**（同 t41 对 `.ant-input.mono` 27→32 的论证）。**(c) 的否决与其余原则不变。**
3. **其余具名项全部合规**：① 常数 1 项（switch）· ② 内容驱动 2 项（图标按钮、`summary`，须带「随内容变化」标注）· ④ 陈旧 1 项（runtimes，表已更新）· 内部部件 2 项（口径正确排除，不是漏检）。
4. **给后续的规则**：往具名闭集里加成员之前，必须先用 **③ 的测试**（**自然宽度**，见 §12.14.1 —— 不是「关掉压缩源」）跑一遍；只有通过测试的才准进闭集，进闭集的 ② 类必须带「随内容变化」标注。

---
### 12.13 行级动作由「悬停门控」改为「常显」（2026-09-23 裁决）—— 契约同步 + 一处新 FAIL

**① 裁决**：以**用户投诉**为准 —— **行级动作常显**。
- **原文（历史）**：primitives §3.3 **C10** = 「行内动作 @.row-action@：悬停显形的行操作；显形 120ms；@hover:none@ 常显；@:focus-within@ 显形」，@view-sessions.md@ §3/§4/§7 同源（「200 个常驻图标会把标题挤没」）。
- **推翻理由**：门控**没有换来布局收益**（@opacity@ 不占布局；图标两态都占那 24px；@1440@ 标题宽 **441px 两态完全相同**；@390/768/1440@ 溢出全 0），代价是**可发现性** —— **用户直接投诉「chat 页面，对于 session 没有删除或者归档的按钮」**，那正是门控造成的。⇒ **可发现性 > 视觉安静**。
- **落地**：@t64@ 移除门控，@t66@ 删掉 @index.css@ 的 @.row-action@ 规则（**全仓 0 使用者**）。契约文本（primitives §3.3 C10 / @view-sessions.md@ §3·§4·§7 / MASTER 的 @dur-2@ 行）由本节同步，**原文一律保留为历史**。

**② 实测的连带后果（新 FAIL，必须派工）**：门控的 @opacity: 0@ 让审计的可见性口径（@opacity > 0@）**看不见**这些行级动作；常显后它们进入行 18 的「内容区<32px」计数：

| 路由 | 行 18 内容区 <32px | 明细（@--check@ 实测，构建 @Cgpoljp3@） | 判定 |
|---|---|---|---|
| @#sessions@ | **39** | @.row-btn > button.icon-btn@ **24×24**，**每行 2 个** × `19–20 行 | **39 > ≤10 ⇒ FAIL** |
| 其余 12 路由 | 0–3 | 均为既有项 | PASS |

**两条可判定出路（选一，captain 裁决）**：
- **(a) 把命中区抬到 ≥32**（与 §5.4 的 32×32 目标、C9 的 @min 24×24@ 同向）：行级动作 24×24 → **32×32**（@padding@ 或 @min-width/min-height@）⇒ 退出 <32px 集合 ⇒ 行 18 恢复 PASS。**代价**：行内占宽 +8px × 2（实测 @1440@ 标题 441px 有余量、三视口溢出 0，可承受）。
- **(b) 用「具名闭集」手法给行级动作单开一个预算**（与 §12.9 外壳、§12.6 出血闭集同手法）：例如「@.row-btn@ 内的行级动作 ≤ 40/路由 且必须全部是 @.icon-btn@ + @aria-label@」。**代价**：多一条具名规则；**且必须写明「不是豁免 32 目标，而是承认列表行的动作是密集区的例外」**。

**③ 我的建议**：**(a)** —— 因为 (b) 会重演本代已六次判过的「口径类」问题（把密集度当成豁免），而 (a) 有实测余量支撑（标题 441px、溢出 0）。
**④ 转交**：@ui-list@（行级动作的实现者）+ captain（若选 (b) 需裁决）。

---

### 12.14 两条新判据（t69 落地）：压缩致折行（行 40）与兄弟压盖（行 41）

**来源**：captain 在视觉巡检里挖出「`kbd-hint` 一直通过」的盲区 —— **审计当时没有「意外换行 / 元素压盖」这两类判据**。tools 在 t69 实现，**本节由 design-lead 落进契约**（工具未自行改 MASTER）。两条都已在 `--self-test 264 全绿` 下带**必然 FAIL 与必然 PASS 的双向自检**。

#### 12.14.1 行 40：压缩致折行（短文本被挤到换行）—— 目标 **每 capture ≤0**

**三步判定，缺一不可**：
1. 该元素的**直接文本** ≤ **12 字符**（只算元素**自己的文本节点**，不算子树）；
2. 实际行数 ≥2 —— 用 `Range.getClientRects()` 数**该元素自己文本节点**的行盒，按 **4px** 聚类。
   **数子树会假报**：容器里「标签 + mono 值」两个 inline 子项各出一个行盒，实测 `div.status-row` 被读成 2–3 行 ⇒ 必须只数自身文本节点；
3. **可判定式 = 自然宽度**：用元素自身的字体在屏外以 `white-space:nowrap` 量出该文本的**单行自然宽度**；若 **> 元素宽度 + 2px** ⇒ 容器给少了宽度 ⇒ 折行由**容器压缩**造成 ⇒ **FAIL**。

**必须排除「有意截断」**（否则天天假 FAIL）：见下方 12.14.3 的可判定定义。

**⚠️ 对 §12.12 ③ 的判据更正（工具实测反例，必须记住）**：**不能只用「关掉压缩源看尺寸是否变化」判定**。
把 flex 子项压到 1–2px 后，在**子项**上置 `flex-shrink: 0` / `min-width: max-content` **不能解除压缩**（**父项的 `min-width: 0` 仍然赢**），盒子仍是 2px、放松后高度也不塌 ⇒ **一个真实的 9 行缺陷会被判 PASS**。
**自然宽度没有这个盲区**（85px 的文字塞进 2px 盒 ⇒ 自然宽度 84.9 > 2 + 2 ⇒ FAIL ✔）。
⇒ **判据改为「自然宽度 > 元素宽 + 2px」**；「关掉压缩源后的几何」**只作佐证记录，不参与判定**（§12.12 ③ 的测试列已同步更正）。

**活体正向对照（证明判据仍看得见这个缺陷）**：把压缩源强制改回（`.sidebar-foot` 宽 96px + `overflow: hidden` + 子项 `flex-shrink: 1` / `min-width: 0` / `white-space: normal`）后，三项分别量到 **2×135 / 2×165 / 1×105**，自然宽度 **84.9 / 85.8 / 64.9** ⇒ 三项**全部判 FAIL(compressed wrap)** ✔。

#### 12.14.2 行 41：兄弟元素压盖 —— 目标 **每 capture ≤0**

**判定**：同一容器内的**兄弟**元素矩形两两相交，容差 **≤1px**（`ox > 1 && oy > 1`）。
**只比兄弟** —— 父子几何天然相交，纳入会淹没真信号。

**排除规则（合法覆盖层，按位置判定而非类名）**：
1. 任一方 `position: absolute | fixed`；
2. 任一方在**覆盖层子树**内（antd `.ant-dropdown` / `.ant-select-dropdown` / `.ant-popover` / `.ant-tooltip` / `.ant-modal-wrap|mask` / `.ant-drawer` / `.ant-picker-dropdown` / `.ant-notification` / `.ant-message`，或 `role=menu|listbox|dialog|tooltip|alertdialog`）—— 这些在 portal 里**保留 static 外壳**，只看 position 会漏；
3. 设了 `z-index` 且非 `static`。
**为什么按位置而不是类名**：类名会随重构改名失效，而「覆盖层靠 `position` / `z-index` 脱离文档流」是**布局的定义**。
另排除：不可见元素、被 `clip` / `clip-path` 裁掉的（`.sr-only`）、**SVG 子树**（`polyline` 压 `path` 是画图不是布局事故，实测在 home 的 `<g>` 里误报 2 对）。

**实测（26 captures）**：**FAIL，1 对真压盖** —— `#chat` 的暗/亮各一次：`div.row.chat-session-row` 内 `button.row-btn.chat-session.grow` × `button.icon-btn.icon-btn-lg`，**交 2×32**。
⇒ **与 §12.13 同源**（行级动作常显 + 大号图标按钮的宽度），**已派 t74**（ui-list）。**注意**：`.icon-btn-lg` 按 captain 裁决**只在 >520 启用 32px**（§4.2 / @view-sessions.md），而审计的 26 captures 在 **1440** 跑 ⇒ 行 41 判的就是 **>520 那一档**：这 2px 压盖正是「32px 的大号图标按钮 + 行内 gap 不足」的算术后果，**t74 需要同时满足「>520 不压盖」与「≤520 不缩标题」**。
**t74 已落地**（captain 实测：`#sessions` 行 18 = `地板<24px 0 · 内容区<32px 0/≤10 ✓`）⇒ **本行这 1 对压盖需由 tools 在 t74 后的构建上复测**（`.icon-btn-lg` 在 >520 变 32×32 后是否仍与 `.row-btn` 相交 2×32）；本节保留的是 t74 之前那次实测。
另有 **8 对合法覆盖被正确排除**：antd Select 内部 4 对（`.ant-select-placeholder` / `.sel-opt` × `input.ant-select-input`，`position: absolute`）、`.ant-switch` 手柄 × 内层 4 对（`position: absolute`）✔。

#### 12.14.3 「有意截断」vs「意外裁切」—— 可判定定义（行 40 的排除规则，也供行 18/35 复用）

`scrollHeight > clientHeight` **本身不是缺陷**。可判定规则：
- `-webkit-line-clamp ≥ 1` ⇒ **有意截断**（设计要 N 行）；
- `text-overflow: ellipsis` + 任一轴 `overflow: hidden|clip` ⇒ **有意截断**（设计要 1 行）；
- **两者皆无**而文本仍溢出 / 折行 ⇒ **意外裁切 / 压缩** ⇒ 交 **行 40 的自然宽度测试**。

**`#home` 的 `.mp-text` 判定：有意截断（PASS）**。实测第二个 `.mp-text`：212 字符、`w365 h36 lh18`、**`sh 72 > ch 36`**、`-webkit-line-clamp: 2`、`display: flow-root`、`-webkit-box-orient: vertical`、`overflow: hidden`、`textOverflow: clip` ⇒ 它就是**设计要 2 行的 clamp**，不是意外裁切。（第一个 `.mp-text` 34 字符 `sh18 == ch18`、1 行，本来就不涉及。）另：212 字符 > 12，也被行 40 的长度门直接排除 —— **两条路径结论一致** ✔。

---

### 12.15 外部标准交叉验证（Vercel《Web Interface Guidelines》，2026-09-23）

**目的**：让「已验证」成为**可追溯的事实**，而不是口头承诺 —— 本代的判据全部是**自己定的**（§12 的 40 行），需要一个**外部标准**作对照，避免自证。

**标准与跑法**：**Vercel《Web Interface Guidelines》**（外部实用清单；**不是** WCAG —— WCAG 1.4.11 / 2.5.8 等仍是本契约的**主标准**，见 §2.4 / §11.1）。跑法：**DOM + CSS 双侧探测**，**13 路由 × 暗色**（captain，2026-09-23）。

**结论：0 条真实违规。** 命中项逐条排除：

| 命中 | 为什么不是违规 |
|---|---|
| 图标按钮缺 `aria-label` | 只有 antd 自带的 `.ant-input-clear-icon`（**非本项目代码**） |
| `div onClick` 代替 `button` / `a` | 只有 antd `Menu` 的 `li`（antd 内部已带 `role` + 键盘处理） |
| `outline: none` 无替代（2 处） | 两处**都在注释里**：一处是**禁令说明**（`Banned: a bare outline: none with no replacement`），一处是**引用 antd reset 以说明如何用特异性击败它而不加 !important** |
| `img` 缺 `alt` / 字面三点号 / `transition: all` | **0 命中** |

**本项目比该指南更严之处（本记录的主要价值）**：焦点系统把「键盘焦点只有一个环」拆成**两条渲染路径**并各自判 ≥3:1（**行 15**）—— ① 自绘 / 原生控件走**零特异性规则**；② antd 控件走 `--ant-color-primary-border` **重着色其 3px 轮廓**；并**明文禁止**「裸 `outline: none`」与「落到 UA `auto` 环」。

**与本契约已有行的关系（避免重复计算）**：该清单的类里有 3 个**已被本契约的行覆盖** —— 图标按钮命名 ⇒ **行 17**（无名称的可交互元素恒 0）；`:focus-visible` 12 处 + `outline: none` 无替代 ⇒ **行 15 / 行 16**（焦点环两模式 ≥3.0:1、无焦点环的 Tab 停留点 = 0）。其余 4 个类（`transition: all`、`img alt`、字面三点号、`tabular-nums`）在本契约里**只有弱覆盖或没有对应行**（`transition: all` 与 `alt=` 在 MASTER 里 **0 处提及**）⇒ **本记录是它们的唯一出处**。

**⚠️ 效力与边界（必须一起读）**：
- 这是一次**一次性探测**，**不在 26 captures 的审计链里**，**没有 `--check` 行**、**不受行 40 / 41 那类机器判定保护**，且**只跑暗色**。
- 因此它是**快照，不是保证**：**不得**当作「以后也不会违规」的依据。要让它持续，需另开任务把探测做成 tools 的**可选检查项**（tools 已提议，**本轮不做**）。
- 本节的结论**不替代** §12 的任何一行，**也不改变任何判定**。

---
### 12.16 行 35 的判据锐化：从「只允许一个值」改为「JS 断点集 ⊆ CSS 断点集」（2026-09-23 裁决）

**问题**：行 35 原文的目标是「≤4 CSS 断点 + 1 契约值(992)」⇒ 实测 **JS [520,992]** 被判「**越界 520**」。
**但 520 不是新断点** —— 它是 M1 的**既有 CSS 断点**，且有**明文语义**：`ui.tsx:346`（the ≤520 2-up grid）、`Board.tsx:147`（the 520 branch turns the four lane gauges into a 2x2）。t72 加的 `SessionsView.tsx:379 matchMedia("(max-width: 520px)")` 只是**从 JS 侧读同一个断点**（窄屏要分支渲染，纯 CSS 做不到而不复制 DOM）。
⇒ **规则禁止了一件合法且既有的事。** 但**不因此把 520 加进白名单**（那是**扩大例外集**，与本代一贯的做法相反）。

**裁决：把判据的目标锐化为子集断言**（真正的意图是**防 JS/CSS 漂移** —— JS 侧不许出现 CSS 侧没有的断点，而不是「JS 侧只能有一个值」）：
- **CSS 侧 ≤4 个断点**（不变）；
- **JS 侧读到的每个值都必须 ∈（CSS 断点集 ∪ 契约值 {992}）** ⇒ 出现任何 **JS-only 断点**即 **FAIL**；
- **992 仍是契约值**（shell 折叠的约定，可同时出现在两侧）。

**当前实测（子集断言下）**：CSS **[520,768,1024,1240]**（4 个，恰好 ≤4 ✔）· JS **[520,992]** ⇒ **520 ∈ CSS ✔、992 = 契约值 ✔ ⇒ PASS**（原措辞下 520 被误判越界 ⇒ 行 35 由**假 FAIL** 变回**可判**）。

**为什么这是锐化而不是放宽**：它**仍然拒绝任何 JS-only 断点**（注入 700 ⇒ 必须 FAIL），并且**新增一条耦合**：JS 侧的值必须**在 CSS 侧真实存在** —— 若 CSS 侧删掉某个断点而 JS 侧还在读，漂移会被抓到（原措辞抓不到这种「两侧不同步」）。

**理由（记档）**：原措辞把「**防漂移**」误表达为「**只允许一个值**」，导致一个既有的合法 CSS 断点（520）在 JS 侧被误判越界。
**与 §12.10.5 第 4 / 5 条的关系（是它们的镜像）**：第 4 条（判据匹配不到对象）与第 5 条（对象集不全）让检查**假绿**；这一条让检查**假红（假 FAIL）** —— **同源于「判据的含义 / 对象集与真实意图没对齐」**。

**假红与假绿的危险形状不同（对策也不同）**：
- **假绿的危险是「隐藏工作」** —— 绿是默认状态，**没有人会去看一个 PASS** ⇒ 缺陷可以活很久（第 4 条的 5 个实例里，`parseInt("1.00")` 让行 1 用错判据**从 t44 一直沉默**）。
- **假红的危险是「训练人忽略仪器」** —— 它不隐藏工作，它**浪费工作**；更糟的是：**一个长期无故为红的行，会让整个工具变得可忽略**（「它总是红，别管」）。本代行 35 曾经是**全量里唯一的 FAIL** ⇒ 整轮读数一直是「1 fail」，而那个红**与任何人的改动都无关**。

**对策：一个跨构建持续为红、且不随任何改动出现的 FAIL，本身应被视为可疑** —— **无解释的红与无解释的绿同样可疑**。
**具体动作**：若某行的 FAIL 在**连续两次独立构建**上都出现、且**期间无相关改动**，必须**先判它是判据问题还是缺陷**，**不得**作为「已知 pre-existing」无限期挂着。（本代行 35 就被多次标注为「pre-existing、与本次无关」—— **那个标注是对的，但没有人去问它为什么一直是红的**，直到本节把它记档。）

**实测（2026-09-23，行 45 的 (a) 落地后，tools 复跑全量 285.7s）**：**44 pass / 0 fail / 1 not measured / 0 pending** —— **本代第一次零 FAIL 的全量跑**。行 42 转 PASS（`transition: all` **0 处（自有代码）** · 已排除 antd-cssinjs **199 处**）；行 45 转 PASS 且**非空集**（探针无 error · 26 captures 共 **24 个候选全部覆盖**）；`not_measured` = 行 28（informational：默认窗口 2500 < 11.5s）；**未出现成组假 FAIL ⇒ 未触发判别法**。行 35 的「**由假红变可判**」过程保留在本节（问题 → 裁决 → 子集断言 → 实测 PASS）✔。
   **⚠️ 成立前提（必须一起读）：这个 44/0/1 是「探针正常」条件下的** —— 见下方「一次假绿」记录；**下一个读者不得把它当作无条件结论**。
   **⚠️ 记录一个偏差（值得单列）**：captain 事前**预测 44 / 0 / 1**，**实测 43 / 1 / 1**（差在行 45）。**预测也是一个手写的数，而且它基于一个未验证的假设**（「4 处全部出局」，实测只出局 3 处）。**tools 没有迁就预测去写读数、也没有去调判据** ⇒ 偏差才可见。**这是第 2 条（手写的数会漂）与第 7 条（核效果不核字面执行）在同一件事上的叠加。**
   **⚠️ 一次假绿（记档）**：tools 第一次落 (a) 后**也跑出 44/0/1，但那是假的** —— `isDateTime` 被插在用到它的循环**之后** ⇒ **探针抛 `ReferenceError`** ⇒ 行 42–45 把**缺失的数据读成空集** ⇒ 四行全绿（**Warnings 里写了，但行是绿的**）。**已加守卫：探针失败 ⇒ `not_measured`，绝不 PASS**（自检名照录：**`a FAILED markup probe yields not-measured, never pass (a broken instrument must not read green)`**）。
   **tools 的判断力两次应用（第 2 次更难）**：上轮它报**实际 43/1/1 vs 预测 44/0/1**（**不迁就预测**）；这轮它先跑出 44/0/1 **但没有直接交**，而是去核「**为什么突然全绿**」⇒ 抓到假绿。**「『预期达成』和『实测达成』是两件事，而『实测达成得过于顺利』本身是个信号。」**（tools 原话）—— **第 2 次更难，因为这次的数字恰好等于预期，交出它是最自然的动作。**
   **本轮的过程与决定另见轮次报告 `docs/round-report-2026-09-23.md`**（**报告是轮次的、契约是持久的**，两者不重复；本行只是指针）。
**转交**：**tools** 在判据里实现**子集断言** + **must-FAIL 自测**（注入一个 JS-only 断点如 700 ⇒ 必须 FAIL；按 §12.10.5 第 4 条，必须能证明「它确实在看断点」）。

### 12.17 「差集 → 行」的判定登记（2026-09-23，t216）

**来源**：三条评估轴共给出 **21 项差集**（t213 轴四 9 项 · t212 轴三 6 项 · t211 轴二 6 项）+ **t208 的 3 条方法级发现**。本节把每一项的**判定**写下来 —— **「不值得成为行」也是结论** ✓。

**准入条件（本节用的判据，先写明）**：一条差集只有同时满足三条才成为行 —— ① **有可指名的失败模式**（一个具体的、可能发生的改动会让它变红）；② **对象集可以正面定义**（不是「凡看着不对的都算」）；③ **与既有行不重复**（对象不同）。**反例（不满足 ① / ②）**：行高现在全 1.5 且无阶梯可依 ⇒ 加了就是**一条永远 PASS 的行**，那是噪声，不是判据 ✗。

| 差集 | 判定 | 理由与读数 |
|---|---|---|
| t213 ① 同排对齐 | **成为行 60**（**判据按 t215 的证伪改写**） | t213 的建议判据「同容器类内第 2 个可见兄弟起始 x 集合大小 = 1」**已被 t215 证伪** ✗：它把**同一容器类的不同行**（logo 行 vs 名称行 / 状态行）与**不同网格列**的 x 混比 ⇒ **每张多列卡片栅格都会被判成坏的**。改写为「**先按网格列分组**（x 聚类 ≤1px），**再**要求同列内集合大小 = 1」，并把「某列只有 1 实例 ⇒ 不可判」写进行内（否则多列栅格上退化为恒 PASS）✓ |
| t213 ② 间距的有效几何 | **不成为行**（登记为行 37 的已知盲区） | 行 37 的对象集是**声明值**（作者来源），有效几何里 off-ladder 的值实测由三类构成：**clamp / vw 派生**（23.5 · 31.68 · 25.92 —— 流体间距是设计选择 ✓）、**antd 注入值**、**已登记的负出血**（-12）⇒ 三类都**不是缺陷**，且都在行 37 的对象集之外**按设计**。要立一行就必须给出一份**白名单**，而那份白名单只能写成「这些派生值」—— 那等于把**不需要判的东西**列成契约 ⇒ **对象集为空的行** ✗ |
| t213 ③ z-index 刻度 | **不成为行**（附提升条件） | **读数（t216 复测）**：12 路由 × 1440 下 z-index 不为 auto 的元素**只有 1 个**（div.ant-select-placeholder，**antd 的**）⇒ **面板侧 0 个声明**；**契约里没有任何 z-index 刻度可派生**（docs/design 与 docs/research 全文只有行 41 的「覆盖层」**定义**用到这个概念，**没有刻度值**）⇒ 立一行等于**当场发明一个刻度**（t213 引的 10/20/30/50 来自外部技能建议，**不是本契约的裁决** ✗），且对象集当前为空。**提升条件（可证伪）**：面板侧出现第 2 个 z-index 声明，**或**出现一次叠层缺陷 ⇒ 那时**先裁决刻度**（刻度必须由「与 antd 1000+ 层的关系」推出，不是抄一个数字）**再**落行 |
| t213 ④ 移动端 ≥16px | **不成为行**（**已由行 51 覆盖输入**） | t213 量的是**正文**（390 与 1440 都 14px）—— 与行 51 的**对象不同**（行 51 是 input / textarea / select）。**依据**：docs/research/web-interface-guidelines.md:28 的规则是**输入**（iOS 聚焦自动缩放），**不是正文** ✗；正文 14px 在**字号阶梯**上（行 8）⇒ 「窄屏正文 ≥16px」会把行 8 的 14 档在窄屏判红（**同一契约两处冲突** ✗）。**读数**：行 51 本轮审计实测 **min = 16 ✓ PASS**（520 / 390 档）；桌面档（1440）实测输入 = **14 / 15px** —— 那**不是**本行的对象（≥16 是窄屏规则）✓ |
| t213 ⑤ 行长（measure） | **成为行 61** | 对象集 = **渲染为 ≥2 行的散文块**；**1 行实例不入对象集** —— 1 行 ⇒ 无折行 ⇒ 容器宽度不构成「行长」（#memory 的 132 ch 容器里文本只有 44 字符 / 1 行 ⇒ t213 的「139 字符 / 行」正是把**容器量当行长** ✗）。量取**容器 measure**（ch）而**不取 text.length ÷ 行数**（后者对末行不满的块**系统性低估**）✓ |
| t213 ⑥ 行高 1.5–1.75 | **不成为行**（附提升条件） | 读数：13 路由正文行高**全 1.5**。① **无阶梯可依** —— primitives.md §2.5 的阶梯覆盖间距 / 字号 / 字重 / 圆角，**不含行高**；② **对象集无法正面定义** —— 「哪些元素的行高是排版决策」会把标题（合法 1.2）、单行盒、控件内部文本全卷进来，**收窄成「正文」又只是复述一条共享声明**（body 一处 1.5 ⇒ 全站派生）⇒ 一条**永远 PASS 且不能失败**的判据 ✗。**提升条件**：若出现「同一层级的文本两处行高不同」（两个视图的正文一处 1.5 一处 1.7）⇒ 那是**行 62 的形状**，届时把行高**并入行 62 的元组**（并入已有对象集，不新立空行）✓ |
| t213 ⑦ 同类组件跨视图形态 | **成为行 62** | 读数：.tag 在 **8 条路由**形态元组**唯一**（t215 修后 h = 20；改前 #settings 因**行内元素**被父级 15px 行高挤成 18 ✗）· .ant-btn 12 路由 1 种 · .ant-card / .ant-empty / .ant-table 各 1 种。**反向证据已在盘上**（去掉 display: inline-block ⇒ 必须 FAIL）✓ |
| t213 ⑧ no-emoji-icons | **成为行 63** | 读数 0 处。**它不是「永远 PASS」**：失败模式是**一个 ✅ 写进按钮文本**，而且它是**既有散文规则的可判定形式**（图标一律走 Icon 原语）✓ |
| t213 ⑨ font-pairing | **并入行 62**（不另立行） | 读数：实测 2 族、职责分明（code 族 = IBM Plex Mono、其余 inherit）⇒ **单独立一条 font-pairing 行会永远 PASS**（族是共享层的一处声明）✗；它的**可判定部分**（code 用 mono、其余 inherit）正是行 62 元组里的 font-family 分量 ✓ |
| t212 ① 首屏实际传输 | **成为行 64**（含**地板漂移**子断言） | 行 27 只量**入口 chunk**（108.6KB），而首屏实际要下 **≈1180KB raw / ≈377KB gzip**（入口 + modulepreload vendor + 两张 CSS）⇒ 这是**另一个量**，此前**没有任何行拥有它** ✓。子断言 ① 把「登记地板 305KB vs 实测 324.2KB（+6.3%）」变成可判定项 ✓ |
| t212 ② 路由 chunk 惰性 | **成为行 65**（**当前 RED**） | 读数：13 路由冷加载**每路由都取 16 个 JS chunk / 1453KB** ⇒ 12 个路由 chunk 全部被取 ✗。行 27 的断言 ② 只判「**存在**路由级分割 chunk」（形式），本行判「**只在需要它的路由取**」（意图）⇒ 对象不同 ✓。**t214 已移除 idle 预热** ⇒ 读数须在 t214 之后重取 ✓ |
| t212 ③ JS 覆盖率 | **成为行 66**（附「分量未分解 ⇒ 不得判 PASS」） | 读数：使用率 68–72%（未使用 28–32%）。**但** 16 个 chunk / 1453KB 里 vendor 占 **1044KB = 72%** ⇒ 这个比值**主要由框架层决定** ✗ ⇒ 与行 53 的「未验证的绿」同一处置：**分量分解打印出来之前不得当结论** ✓ |
| t212 ④ FP / FCP | **成为行 67** | 读数：**FP = FCP 逐路由完全相同**（240/240 · 264/264 · …）· FCP 240–276ms ✓。行 26 只判**主线程阻塞**，不判**首绘** ⇒ 对象不同 ✓ |
| t212 ⑤ permissions 每路由 2 次 | **不成为行**（**我复测的归因推翻了这条差集**） | **读数（t216 复测，读源码 + 运行期）**：panel/src/App.tsx:336–352 的**外壳 2s 轮询**（setInterval(..., 2000)，document.hidden 时跳过）+ views/Agents.tsx:844 的**一次**调用 ⇒ 窗口 2.5s 内 **2 次是轮询的自然节奏**，**不是重复取** ✗。行 28 的子断言 ② 已把 permissions **单独计数 ≤7**（正是为了不让轮询污染逐路由数）⇒ 再立一行会**与行 28 的排除口径打架** ✗ |
| t212 ⑥ 外壳替视图取数 ≈23 次 | **不成为行**（t212 自己也标了「属余量不是红」） | 行 39 通过（R ≤ 2K+2）· **无用户可见缺陷**（时间不变、字节在预算内）⇒ 立行会把**架构偏好**写成验收阈值 ✗。**提升条件**：若某路由的 API 数逼近行 39 的余量，**或**外壳取数推迟首绘（行 67）⇒ 那时再立 |
| t211 ① 微交互时长 | **成为行 68** | 阈值**派生自 MASTER §9.1 的四档 token**（100 / 120 / 180 / 260ms），**不是发明一个区间** ✓；docs/research:33（show-delay ~150–300ms）只作旁证。读数：面板声明的时长里 **200ms（.chat-side）与 160ms（.cmdk）不在阶梯上** ✗ —— 与 §9.1 散文里那句「实测共 2 处」逐字吻合 ⇒ **本行把散文变成读数** ✓ |
| t211 ② transform / 布局属性 | **成为行 69** | 读数：面板声明的过渡属性 = background · background-color · border-color · color · opacity · transform · box-shadow · grid-template-rows（**最后一个具名豁免**）⇒ 布局属性 **0 处** ✓。**出处**：docs/research:58 / :273（compositor-friendly）。**与行 42 不重复**：行 42 只判 transition-property === all ⇒ 注入 transition: width 120ms 时**行 42 不响**，本行必须响 ✓ |
| t211 ③ loading-states | **成为行 70** | 读数：延迟该路由数据端点 1.5s ⇒ **13/13 有可见指示**且加载后 **0 残留** ✓。**出处**：docs/research:32 / :33。**它最容易退化的方向写进了行内**：只判「加载完成后 = 0」会变成永远 PASS ✗ |
| t211 ④ reduced-motion 的「在跑的动画」 | **扩展行 53 的子断言**（不新立行） | **同一个对象**（reduce 下的动效），只是把「**声明的时长**」补成「**在跑的动画**」（document.getAnimations() 的 playState / 两样本法）⇒ **扩展一条已有行，比新立一条同对象的行更诚实** ✓。读数：26 capture 一致 · >0.05s 的元素数全 0（index.css:254 的全局规则）· 唯一 running 是 #chat 的 loadingCircle（0.01ms infinite）而 300ms 内无可视变化 ⇒ 判**已停** ✓；**并登记**：#agents / #settings 的 twoSampleChanged=true 归因是**异步内容到达**，不是动画 ✗ |
| t211 ⑤ content-jumping（CLS） | **成为行 71** | 读数：**最大 0.016（#chat）· 8 条路由为 0** ⇒ 距 0.1 余量 ≥6× ✓。**依据 = 惯例阈值**（Core Web Vitals：CLS ≤0.1 = good）· 出处 = docs/research:81 / :140（稳定骨架 / 预载字体，都是为了不跳）。**与行 49 不重复**：行 49 是**行级单次**位移（≤0.5px），本行是**页面级累计**分数 ✓ |
| t211 ⑥ image-optimization | **不成为行** | 读数：全站 `<img>` **13 条全 0**（对象集为空）· 行 43 已判 `alt` ⇒ **空集上的行是空集绿** ✗（这正是行 43 把「0 个对象」与「已验证合规」写成两种记录的那条规矩）✓ |
| t211 ⑦ 过期引用（行 53 的判据写「§12 行 50」） | **工具侧，超出本单 inScope** ⇒ **已报告 captain** | 该字符串在 **panel/tools/design-audit.mjs**（本单不得改 panel/ ✗）⇒ 只能报告 ✓。**docs 侧我逐条查了**：全仓 `§12 行 N` 引用 **164 处**，逐处对照被引行的**指标列** ⇒ **0 处指错行** ✓；**发现 1 处过期措辞**（**不是指错行**）：**行 39 的指标列**仍写「窗口内轮询腿为同源子类」，而 §12.10.3 已把口径改写为「本行**不判**是不是轮询（那是行 28 的职责）」⇒ **标题已改**（见行 39）✓ |

**t208 的三条方法级发现（本节收口）**

**① F-t208a —— 「用两个口径的数互证」的形状：我扫了，现状无残留。**
扫描方法（可复现）：对 MASTER §12 **全部 59 行的「判定方式」列**做正则 `\d+ [-−] \d+ = \d+` 与 `\d+ × \d+ = \d+`；再对 **docs/design 全部 .md** 做同一正则。
结果：§12 判定方式列 **0 处命中** ✓；全仓 **9 处命中，逐条看过，全部是正当推导**（`48×48 = 2,304px` · `198 与 10207` 的同毫秒归因 · `294 − 40 = 254px` 的内容盒 · `1406 − 980 = 426KB` 等）✓。
**其中一处正是 F-t208a 的形状，且本代已经自己订正过**：§12.10.2 ③ 的 `应用层实测 = 1406 − 980 = 426KB` —— `1406` 是**首屏 JS 总量**、`980` 是**框架层地板** ⇒ 差出来的 `426KB` 被叫作「应用层」是**两个口径混用**；该句**现在带着订正**：「（口径提醒：本句的 426KB 是『优化前的**全量**应用层』，判定用的是**首屏口径**，两者差 4 倍）」+「③ 的两个口径不可混用（t54 落地）」，`before-after.md:141` 也登记了这次订正 ✓。
⇒ **结论**：这条形状在**报告**里出现过（t191 的 45 − 27 = 18），在**契约文本**里出现过一次（426KB）且已订正 ⇒ **现状 0 处未标注** ✓。**留给下一个人的检查动作**：新写行时，凡出现「A − B = C」，**必须写明 A 与 B 各自的口径**（本节 ① 就是为此。

**② F-t208b —— 行 58 的 ③ 是待裁决项，且枚举方法有结构性盲区。**
判定与读数已写进**行 58 的判定方式列**（本单改的是契约文本，不是产品行为）：③（选中项进 URL）**在原文里就是「需单独裁决」**，因此**本行的判定当前只判 ②**；t208 的读数支持「图谱选中态**不进** URL 是对的」（切 mode 往返后选中态与像素**逐位保留**、全新加载 ⇒ 无选中）；**并写明枚举盲区**：capture 集由 `[role=tab]` / radio-in-label / `aria-pressed` / select 扫描产生 ⇒ **画布命中目标结构上不可能进入这种枚举** ⇒ 若 ③ 要涵盖它，**枚举方法本身必须先扩**（否则「不得手写清单」会退化成「**方法决定对象集**」✗）。

**③ F-t208c —— 采样面不写进行内，判据就会在不同对象上给出不同结论。**
三条实例：**F-215a**（同容器类混比 ⇒ 每张多列栅格都判坏）· **t208 的墨迹**（同一判据在 hub / 度 1 / 孤立节点上给出 −32% / −72% / −75% 三个结论）· **t213 的 139 字符 / 行**（容器量当行长）。⇒ **写法约束落成 §12.18** ✓。

### 12.18 新行的写法约束：对象集 / 采样面 / 可证伪条件 / 依据（2026-09-23，t216 落表）

**一条 §12 的行必须能回答四个问题** —— 缺任一条，判据就会在不同人手里给出不同结论（本代已踩过三次，见 §12.17 ③）：

1. **对象集**：判的是**哪些东西**（正面定义 + 具名排除）—— 不得写成「凡看着不对的」；「没有对象」与「对象合格」必须分开记（行 43 / 46 的做法）。
2. **采样面**：在**哪些路由 / 视口 / 模式 / 窗口**上取、等什么信号、**每个对象打印什么读数** —— **必须写进行内**（理由见下条硬约束）。
3. **可证伪条件**：什么读数会让它 FAIL，以及**注入什么必须让它 FAIL**（must-FAIL 自检）。**只写目标不写证伪条件的行，会在第一次争议时被改成能通过** ✗。**⚠️ 可操作化（2026-09-23 t236，方法见附录 A.1）**：**验「某判据会不会红」时，先问「能不能把判定函数从工具里取出来直接 eval」** —— **能就别造场景** ✓；**⇒ 对新行的要求**：**判定逻辑必须写成一个具名函数**（工具里的 `function xxxVerdict(…)`），**探针不得自己重写一份判定** —— **探针与工具必须 eval 同一份源码**，这样两者**不可能漂移** ✓（造场景法必然有两份判据、两个作者 ⇒ 漂移是必然的 ✗）。
4. **依据**：阈值是**派生**（由契约里已有的阶梯 / 常量推出）· **裁决**（人定的）还是**惯例**（外部标准），并给**出处** —— 三者必须写明，**不得把「改判据让它通过」当成修法**（行 59 的 ΔE ≥10 就是「惯例 + 余量薄」的写法）。

**两条硬约束（本代各踩过一次，故写成硬约束）**

**① 采样面必须写进行内** —— 否则同一判据在不同采样面上给出不同结论，而**结论看起来都像读数**：
- **F-215a（t215 证伪 t213 的判据）**：把「同一容器类的不同行」与「不同网格列」混比 ⇒ **每张多列卡片栅格都会被判成坏的**。
- **F-t208c（t208 的墨迹）**：同一个「选中前后墨迹」判据，在 hub / 度 1 / 孤立节点上给出 **−32% / −72% / −75%** 三个结论。
- **t213 的 139 字符 / 行**：把**容器量**当成行长（宽容器 + 短文本）。
⇒ **动作**：新行的判定方式列里必须出现「采样面」三个字与它的具体取值（路由 × 视口 × 模式 × 窗口 + 逐对象打印什么）。

**② 源码计数不是运行期计数（反过来也成立）** —— **每条行都要写明它的读数怎么取**：
- **评审已记过**：`grep -c '<h1'` 读到 **2** 而运行期是 **1**（loading / loaded 是**互斥分支**，源码里两处都在）⇒ 该读运行期的行不得用源码计数。
- **镜像方向（本单新记）**：**行 68 的对象集必须是声明值**（作者来源），因为按**计算值**判会把 antd 的 `color 0.3s`（156 个元素）、`all 0.2s`（sider）、`width 0.3s`（menu）算成**我们的**越界 ✗ —— **有直接量（声明）时不要用代理量（计算值）**。
- **行 42 的例外口径**是同一件事的第三种写法：**排除项要打印出来**（199 处 antd），而不是静默过滤。

**三条「空集」语义（不得静默 PASS）**：某路由**没有对象** · 某性能 entry **没出现** · **探针没装上** —— 三种都必须报 `not_measured` 并点名原因。**「0 个对象」与「已验证合规」是两种记录** ✓（行 43 / 46 的先例）。

**准入条件（新行必须满足，否则不落表）**：**有可指名的失败模式** ∧ **对象集可正面定义** ∧ **与既有行不重复**（对象不同）。**一条永远 PASS 且不能失败的行是噪声，不是判据** ✗ —— §12.17 的 21 项判定全部按这条办。

**既有 59 行的复核（本单的第三个交付，逐条给读数）**

| 复核项 | 读数 | 处置 |
|---|---|---|
| 有没有**被本轮评估证伪**的行 | **没有** ✓ —— 被证伪的是 t213 的**建议判据**（同排对齐），它**还没进表**；既有 59 行的对象集里没有一条带着那个缺陷 | 无需改 ✓ |
| `§12 行 N` **引用是否指错行** | 全仓 **164 处**引用逐处对照被引行的**指标列** ⇒ **0 处指错行** ✓ | 无需改 ✓ |
| **过期措辞** | **1 处**：**行 39 的指标列**仍写「窗口内轮询腿为同源子类」，而 §12.10.3 已改写为「本行**不判**是不是轮询（那是行 28 的职责）」 | **已改**（行 39 指标列）✓ |
| **过期引用（工具侧）** | **1 处**：**行 53 的判据文本**写「MASTER §12 行 50」，而 reduce 那条是 **行 53**（行 50 是会话名注入）—— 该字符串在 `panel/tools/design-audit.mjs` | **超出本单 inScope（不改 panel/）⇒ 已报告 captain 派 tools** ✓ |
| **登记值与实测不一致** | **1 处**：行 27 的地板 gzip 声明 **≈305KB** vs 实测 **324.2KB（+6.3%）** | **不在本单改** —— `panel/tools` 的自检把 980 / 305 钉成 **must-PASS 断言** ⇒ 单改 MASTER 会让**工具自检变红** ✗ ⇒ 需 tools 同步改（**已报告**）✓；本单把这条漂移写成**行 64 的子断言 ①**（可判定）✓ |
| **读数暂不可信的行** | 行 **47 / 50** 的探针仍有缺陷（t198 在办）⇒ 其读数**暂不可信** | 如实登记 ✓ |
| **新增行后行号连续性** | 1–71 **无缺口**（check-contract 输出 `§12 表：71 行，最大 71，缺口 []`） | ✓ |

**本节与 §12.17 的关系**：§12.17 记录**这一批差集**的判定（逐项），本节记录**判定与写法的规矩**（面向下一批）⇒ **两者都要读**：前者回答「这 21 项各自怎么判的」，后者回答「下一项该按什么规矩判」✓。
---
## 附录 A：口径与复现方法

**所有不是"能算出唯一值"的指标，都必须在脚本里写成同样的公式。** 本轮的测量脚本口径如下（工具成员应把它固化成可复现的审计脚本）：

**可见性**：`getBoundingClientRect()` 宽高 ≥1px，且 `visibility !== hidden`、`display !== none`、`opacity > 0`。

**像素统计**（截图 → 直方图）：
- 截图：Chromium headless，`viewport = 1440×900`，`deviceScaleFactor = 1`，`waitUntil: 'load'` 后等 2.5s，全页截图（非 fullPage）。
- `luma = (0.2126·R + 0.7152·G + 0.0722·B) / 255`，取两位小数后归桶。
- 色度：`S = (max-min)/max`，`S > 0.15` 且 `max-min > 12`。
- 信号：HSV 色相 `20–50°`，`sat > 0.30`，`max-min > 25`，`max > 110`。

**对比度**：WCAG 2.x 相对亮度公式（`c ≤ 0.03928 ? c/12.92 : ((c+0.055)/1.055)^2.4`），
带 alpha 的前景先与最近的不透明祖先背景**线性合成**再计算。

**材质台阶**：用**原始相对亮度比**（`(L1+0.05)/(L2+0.05)` 是 WCAG；台阶用 `L1/L2`），
因为 WCAG 公式在低亮度区会把 2.48x 压成 1.14:1。

**字号/字重/圆角/间距/内联样式**：一律读 `getComputedStyle`，不读源码文本（源码是意图，计算样式是事实）。

**焦点走查**：`body.focus()` 后连按 Tab，逐次记录 `document.activeElement` 的
`outlineStyle / outlineWidth / outlineColor / boxShadow`；两者皆空即"无焦点环"。

**路由表（13 条，hash 形式，注意没有 `/` 前缀）**：
`#home` `#chat` `#sessions` `#board` `#task/<id>` `#memory` `#knowledge` `#graph` `#agents` `#runtimes` `#stats` `#settings` `#inbox`
（`parseHash()` 的 `default` 分支**就是** `home`；写 `#/home` 会静默落到 home，审计时容易全体取到同一页——本轮踩过这个坑，脚本必须断言 `location.hash` 与内容指纹。）

---

### A.1 判定函数的 eval 法：验「某判据会不会红」的标准做法（2026-09-23 t236 落表）

**问题**：一条行的可证伪条件写完之后，「**它真的会红吗**」通常靠**造一个真实场景**来验 ⇒ 而**场景没造对会给出假结论**（假绿：以为不会红；或假红：以为会红、其实根本没触发）✗，而且造场景比验判据**贵得多**。

**做法（四步，可操作）**：
1. **把该行的判定函数原文从工具里抽出来**（`panel/tools/design-audit.mjs` 里的 `function xxxVerdict(…) { … }`，**连函数体一起**）—— **不是重写一份** ✓；
2. **`eval` 它**（`new Function` 亦可），**把读数当参数喂进去** —— 判定函数只吃「读数 + 解析出的阈值」，**不吃页面** ✓；
3. **喂两组读数**：一组**必须 FAIL**（例如「真的读到 2 行以注入块头开头」）、一组**必须 PASS**（干净读数）⇒ **两半都要给**（只给一半 ⇒ 要么**静默通过**、要么**卡死在红** ✗）；
4. **把两组读数与判定结果写进该行的读数记录** ✓。

**t234 的实测（行 50，函数体 967 字节）**：`sessionNameVerdict` 原文抽出 eval ⇒ **F「真读到 2 行以注入块头开头」⇒ `FAIL :: bad=2/196`** ✓ · **G「干净读数」⇒ `PASS :: bad=0/196`** ✓ ⇒ **两半都给了** ✓。

**为什么这比造场景强**：**探针与工具不可能漂移** —— 它们**是同一份代码** ✓（工具改了、探针跟着改 ⇒ 不存在「探针还在用旧判据」这种假绿）；造场景法**必然有两份判据、两个作者** ⇒ 漂移是**必然**的 ✗。

**可证伪条件（什么读数说明这条方法被违背了）**：
1. **某行的判定逻辑内联在探针里、取不出来**（工具里找不到对应函数，**或探针里另写了一份** `pass = …`）⇒ **本方法被违背** ✗；
2. **抽出来的函数体 eval 时依赖闭包外的自由变量**（`pct` / `pick` / `ctx` / `parse` 的结果）⇒ **取不出独立单元** ⇒ 必须先具名、并把依赖**当参数传入** ✗；
3. **只喂了一组读数**（只有 FAIL 或只有 PASS）⇒ **不算验过**（静默通过 / 卡死在红两种退化都还在）✗；
4. **反向证据（must-FAIL 自检）**：喂一组**故意违规**的读数而函数仍返回 `pass: true` ⇒ **是判定函数本身坏了**（不是场景的问题）✗。

**覆盖边界（2026-09-23 t236 逐行扫描 71 行；判据 = 该行的判定逻辑是否落在一个具名函数里）**：
- **取得出来 ⇒ 15 行**（判定逻辑 = 一个具名函数，工具里共 17 个）：**18 · 35 · 46 · 47 · 48 · 49 · 50 · 51 · 52 · 53 · 54 · 55 · 56 · 57 · 58** ✓（`splitterHitVerdict` · `breakpointVerdict` · `sessionTitleVerdict` · `clickOrderVerdict` · `pollOrderVerdict` · `anchorMoveVerdict` · `sessionNameVerdict` · `inputFontVerdict` · `touchTargetVerdict` · `reducedMotionVerdict` · `menuSizeVerdict` / `moreLabelVerdict` / `railControlsVerdict` · `railToggleVerdict` · `titleVerdict` · `navLinkVerdict` · `urlStateVerdict`）；
- **取不出来 ⇒ 44 行**（判定逻辑内联在 `judge` 箭头里）：**1–17 · 19–34 · 36–45 · 59** ✗ —— 要 eval 就得把 `pct` / `pick` / `ctx` 这些自由变量一起抽 ⇒ **不是独立单元**；
- **还不存在 ⇒ 12 行**：**60–71**（工具里在 `PENDING_TOOL`、尚无判定函数）⇒ **它们落地时必须先具名** ✓。

**不追溯原则（必须写清楚，否则这条规则会让既有 44 行立刻变红）**：**新行必须具名；既有 59 行不追溯** —— 那 44 行**登记为「内联」**（清单见上），**迁移是独立的活** ✓；**但任何一行在被要求「验它会不会红」时，先做迁移**；**若来不及、只能用场景法，必须在读数记录里写明那是场景法而不是 eval 法** ✗（**两种方法的结论不得混记** —— 否则下一次读的人会以为它是同一份判据跑出来的）。

## 附录 B：与既有文档的数字差异登记（不静默改数）

| 出处 | 原文数字 | 本轮复测 | 处置 |
|---|---|---|---|
| `docs/design-language.md` §Acceptance 第 1 行 | 单亮度带 "14–75%（median ~50%）" | **13.7–83.5%（中位 37.4%）**；`#inbox` 83.5% 超出原文上界 | 口径差异（原文未写归桶方式）。**保留原文作为 DL 基线，本轮数字作为 M1 基线**，并把口径写进附录 A。不改原文 |
| 同上 第 3 行 | "Elements ≥18px, per page: 1–23（32px max on 9 pages）" | 1–23 ✔；但 32px 出现在 **12 个路由**，`#chat`/`#settings` 最大 20px | 与原文"9 pages"不符。**保留**原文作为 DL 基线；第 7 行目标写成"含仪表的页 ≥32px"，避免把 `#chat`/`#settings` 判为失败 |
| 同上 第 4 行 | "Visible hairline borders, all pages: 1–29" | 严格口径 **0–1**；宽松口径 4–1679 | 口径不同。本规格**冻结严格口径**并把两个数字都登记 |
| 同上 "Bordered boxes 0" | 0 | 严格口径 0（12/13），`#chat` 1（`.composer`） | 成立；把例外写进第 12 节第 4 行 |
| `docs/ui-ux-review.md` §五 | `textQuaternary` 11px 暗 4.14:1 / 浅 3.48:1 | 暗 **4.69:1**（palette 5 `#1b1e24`）、浅 **5.44:1**（白） | 该问题**已修复**（`textQuaternary` 现为 `#838891`/`#656a73`）。原文保留为历史基线 |
| 同上 | `.tag.ok` 浅色 **2.6:1** | **3.34:1** | 有改善但**仍未达标**（→ 建议变更 C3） |
| 同上 | `--graph-edge` 暗 1.47 → 3.09 | 暗 **3.08** ✔；**亮 2.17 ✘**（原文未记录亮色改造结果） | 新增亮色缺口（→ C4） |
| `docs/ui-ux-review.md` §四 1 | 焦点环：antd 为 3px `rgb(215,222,252)` | 现为 `rgb(86,65,32)`（暗）/ `rgb(255,234,171)`（亮） | Bench 重做后 antd 环**变差**（1.73/1.19）→ 建议变更 A1 |
| 同上 4.3 | `#sessions` 200 个无名称 24×24 图标按钮 | **0 个 unlabeled / 4 个小目标** | 已修复 ✔ |
| 同上 §九 | 字号越界 10 / 12.5 / 17px | **0** | 已修复 ✔ |
| 同上 §九 | 内联 `style` 113 处、`#sessions` 732 个 | tsx 里 25 处 `fontSize:`；`#sessions` DOM 内联 **731** | 基本未变 |
| 同上 §一表 | `#task` DOM 153,122 | **153,074** | 未变 |
| `docs/ui-ux-review.md` §二 | `#task` 事件行 26,342 | 仍为灾难级（节点 15.3 万） | 未变（不在本设计系统范围内，但第 12 节第 25/26 行保留门槛） |
| 本文件第 11.4 节 | — | `[aria-expanded]` = 0 | **该口径不完整**（未排除 antd）：实测全站 = `#chat` 3 / `#task` 3 / `#memory` 1 / `#settings` 1，手写控件 0。**就地更正并保留原文**，处置见第 12 节行 23 |
| 本节第 12 节 行 6 | 目标「≥3 且 ≤40」（无条件） | 实测下界 1（`#task` / `#settings` / `#inbox`），空态 `#chat` 2 | **判据条件化**（§12.1）：默认 ≥3；`#chat` / `#settings` / `#inbox` 三个无读数路由 ≥1，且仍须恰好 1 个 20px 页面标题。**数字未改，改的是适用范围**；原行不删 |
| 本节第 12 节 行 16 | 判定「`outline-style: none` 且 `box-shadow: none` 即判无环」 | `.readout-btn` / `.ant-segmented-item-input` 落到 UA `outline-style: auto` → 会被记成有环 | **判据改写（趋严）**：只有 outlineStyle 为 solid 且 outlineWidth ≥ 2px 才算有环（auto 判 fail）。权威定义改挂 primitives.md §11 行 16 |
| 本节第 12 节 行 23 | 基线「`[aria-expanded]` = 0」；判定用全站 `querySelectorAll` | 全站 = 3 / 3 / 1 / 1（**全为 antd `Select`**） | **基线更正 + 判据改写**：旧口径 0 ≥ 0 属**虚假通过**；改为只数 `.recall-stub-head` / `.tool-head` / `.chat-group-more` 覆盖率 = 100%（primitives.md §11 行 23） |
| 本节第 3.1 节 | 无跨域规则 | 三个分类色域并存（`--graph-*` 9 / `--brand-*` 3 / 会话来源） | **新增 §3.1 I 明文规则**：会话来源域（③）**合并进**品牌域（②），`--source-*` 明确拒绝；守护第 33 行。**零 hex 变化** |
| 本节第 12 节 行 32 | 判据「`--signal` / `--brand` / `--status-warn` 三者互不相同」 | accent `--ant-color-primary` 与 `--signal` **同值**（暗 `#f0a93b` / 亮 `#c88413`），而 primitives §2.4 C6 + §2.6 **要求** `L.brand === --signal` → 原判据**永不可能 PASS**（必然报 1 组重复） | **契约内部冲突；判据读法 + 适用范围修正**（§12.3）：唯一性检查集从「含 accent 的 3 个」改为「运行时品牌族 + signal + warn 共 5 个」，accent 显式排除（单一信号色通道）。**数字未改**——`M1` 基线与 5 个令牌的 hex 一字未动，改的是**读法与检查集**；原行不删。另一半（primitives §2.3 / §2.4 C6 / §2.6 措辞）已在 §12.3 给出修改建议，由 captain 转 `systems` |
| `docs/ui-ux-review.md` §九 | `theme-color` = `#0a0c0e` | 实测 `index.html:6` = **`#0a0a0c`** | 原文记录的是更旧的值；本规格用实测值，差异在此登记，不改原文 |

**待完成归因（不得凭猜测下结论）**：
1. sessions 的 203 个 `box-shadow` 元素、stats 的 25 个——需要逐条归因后才能据此调材质（2.2 节）。
2. 700 字重的 200/21/96 个节点——需确认是否全部来自 markdown `<strong>`（4.3 节）。
3. `#stats` 图表条的 9 个 amber div 与 `#memory` 的 10 个 amber button——建议变更 C5 的具体选择器清单尚未逐条列出。

---

## 附录 C：落地批次（给 `systems` 与 `ui-*` 的执行顺序）

> 只排"先做什么"，不排"谁做"。共享热文件（`index.css` / `theme.tsx` / `ui.tsx`）只有 `systems` 可改。

| 批次 | 内容 | 影响面 | 依赖 |
|---|---|---|---|
| **P0（token 层，1 个 PR）** | C2（信号/警告去重）、C3（亮色 `.tag.ok`）、C4（亮色图谱边）、C8（暗色 hover 变亮）、A1（统一焦点环）、A3（删 2 处裸 `outline:none`）、C10（`::selection`） | `index.css` + `theme.tsx` | 无 |
| **P1（类型与几何收敛）** | T1–T6（字号/行高/字重/内联 fontSize）、R1–R3（圆角）、S1–S2（间距）、C9（亮色文本梯度单调化） | `index.css` + 视图内联 | P0 |
| **P2（结构与语义）** | Y1（`sr-only h1`）、Y2（命令面板 ARIA）、Y3（`aria-expanded`）、Y4（theme-color）、B1（断点收敛）、B2（`#stats` 溢出）、A2（`:active`）、A4（disabled 档） | 视图 + `ui.tsx` | P1 |
| **P3（逐视图规格落地）** | 13 条路由逐视图规格（由本文件派生）→ Z1/Z2（三角色、panel 计数为 0 的 5 个路由）、C5（图表/次级按钮去信号色）、R4/B3（图谱自适应）、T5 的 `.stat-strip` 归一 | 视图 | P2 |
| **P4（性能，超出设计系统但影响验收）** | 第 12 节第 25–28 行（`#task` 虚拟化、`React.lazy`、轮询收敛） | 视图 + 构建 | 独立 |

**每批结束必须**：`cd panel && npm run build` → `npx playwright test`（41 项，当前 39 passed / 2 skipped）→ 审计脚本跑第 12 节全表 → 截图归档 `docs/screenshots/`（`final_*` 为当前态基线）。
