# ruagent Panel —— UI/UX 与前端质量审查

- **审查对象**：`http://127.0.0.1:8787/`（本地 daemon 实际渲染的 panel，13 个 hash 路由）
- **日期**：2026-09-20
- **方法**：Chrome 153 headless + CDP 直连真实渲染 → 逐路由采集 DOM 规模、布局盒模型、点击目标尺寸、计算样式、WCAG 对比度、控制台报错、网络请求；再与 `panel/src` 源码交叉验证。
- **重要限制（必读）**：**本次没有"看"截图**。这个会话的模型收不到图像，视觉桥接（modlens）两次都只返回了 CDN 上传回执、没有把像素送进上下文，所以我**放弃"看图"，改为对真实渲染做量化审计**。因此：颜色/尺寸/结构/性能/对比度都是实测数字，可直接复现；但"好不好看"这类主观感受不在本报告结论范围内。截图仍已生成（见文末），需要视觉判断请你自己过一眼。

---

## TL;DR —— 按性价比排序的 12 条

| # | 问题 | 证据（实测） | 建议动作 | 成本 |
|---|---|---|---|---|
| **1** | 任务详情页渲染爆炸：**153,122 个 DOM 节点 / 26,342 条事件行 / 24,563 个内联 SVG** | 单页 JS 堆 110MB，最长单次阻塞 **2,999ms**，累计阻塞 6,316ms，内容 7.4s 才出现 | 合并流式 chunk + 事件列表虚拟化 + 图标改 SVG sprite/mask | 中 |
| **2** | 错误 = 空态；任务详情页失败后**永久 spinner** | 屏蔽 `/api/v1/*` 后 `#task/<id>` 只显示 `任务…` 转圈，无报错无重试；其余页显示"还没有任务/收件箱是空的" | 统一 `loading/error/empty` hook + 重试按钮 | **低** |
| **3** | 路由无代码分割，首屏一个 **1367KB（gzip 428KB）** 的 JS | `panel/dist/assets/index-*.js` | `React.lazy` 按 view 切分 | **低** |
| **4** | 文本对比度系统性不达标（浅色下更差） | `textQuaternary` 11px：暗 4.14:1 / 浅 **3.48:1**；`.tag.ok` 浅色 **2.6:1** | 11px 文本改用已有 `textTertiary`（5.5:1），或调色号 | **低** |
| **5** | 命令面板与模态缺 ARIA 语义 | 实测 `aria-modal` 缺失、`aria-selected` 0 个、无 `aria-activedescendant`；关闭后焦点丢到 `BODY` | 补 `aria-modal` / `aria-activedescendant` / 关闭后还焦 | 低 |
| **6** | 会话列表 200 行各带一个 **24×24 无名称图标按钮** | 实测 200 个 unlabeled、204 个小目标；页面高 8,201px，无虚拟化 | 加 `aria-label`，行动作移出行按钮 | 低 |
| **7** | 移动端统计页**横向溢出** | 390px 视口下 `documentElement.scrollWidth=498`，根因 `.ant-table` 6 列 `table-layout:auto` | 表格改 `scroll={{x:...}}` 或小屏换卡片 | 低 |
| **8** | 10 个独立轮询、无取消、可乱序覆盖 | 实测 11.5s 内：`#home` 20 次 API、`#inbox` **12 次 `/permissions`**（两个轮询器叠加） | 合并为 1 个可见性感知轮询 + AbortController | 中 |
| **9** | 破坏性/写操作无二次确认、无 busy | 源码：知识库删除、回滚、取消运行、清空对话无确认；Ingest/写记忆/建实体可重复提交 | `Popconfirm` + `loading` | 低 |
| **10** | 焦点指示不一致 | 实测 antd 组件为 3px `rgb(215,222,252)`，但 **5/21 个 tab 停留点完全无环**（对话输入框、搜索框、3 个 Select）；`.kanban-card` 等为 1px `rgb(16,16,16)` | 统一 `:focus-visible` token | 低 |
| **11** | 图谱边几乎不可见 | `--graph-edge` 合成后 **1.47:1（暗）/ 1.37:1（浅）** | alpha .14 → .34（3.09:1） | 低 |
| **12** | 设计系统漂移 | 113 处内联 style（`#sessions` 一页 732 个 `<div style>`）、4 套状态色、出现 10px/12.5px/17px 等未登记字号 | 收敛 token，禁内联 fontSize | 中 |

---

## 一、实测数据总表（1440×900，暗色，逐路由）

| 路由 | DOM 节点 | 页面高 | 交互元素 | 小目标<32px | 无名称 | 对比度失败 | 溢出裁切 | 内联 style |
|---|---|---|---|---|---|---|---|---|
| `#`（首页） | 318 | 900 | 33 | 5 | 0 | 17/82 | 3 | 35 |
| `#board` | 335 | 2114 | 41 | 4 | 0 | 26/97 | 0 | 38 |
| `#memory` | 328 | 1610 | 32 | 15 | 1 | 14/87 | 0 | 28 |
| `#knowledge` | 286 | 900 | 35 | 12 | 1 | 9/52 | 0 | 17 |
| `#graph` | 204 | 900 | 21 | 4 | 1 | 6/26 | 0 | 18 |
| `#agents` | 410 | 1200 | 33 | 18 | 0 | 12/138 | 2 | 33 |
| `#runtimes` | 307 | 900 | 31 | 16 | 0 | 5/61 | 6 | 30 |
| `#stats` | 487 | 1388 | 18 | 4 | 0 | 25/180 | 0 | **205** |
| `#settings` | 236 | 900 | 24 | 7 | **4** | 8/35 | 2 | 26 |
| `#inbox` | 190 | 900 | 18 | 4 | 0 | 5/25 | 0 | 22 |
| `#chat` | 316 | 900 | 31 | 11 | 2 | 7/35 | 0 | 21 |
| `#sessions` | **2873** | **8201** | **419** | **204** | **200** | **205/949** | 132 | **732** |
| `#task/<id>` | **153130** | 1342 | 121 | 102 | 5 | **18016/19327** | 0 | 162 |

正常使用下 **13 个页面控制台零报错**（这点做得好）。全站 `html lang=zh-CN` 正确同步，`prefers-reduced-motion` 在 CSS 层已处理（`panel/src/index.css:121`）。

---

## 二、P0：任务详情页的渲染爆炸（最大单点问题）

`#task/01a0bc0c-…`（一次已完成、带完整执行日志的运行）实测：

| 指标 | 实测值 |
|---|---|
| DOM 节点总数 | 153,122 |
| 事件行（`.timeline .ev`） | 26,342 |
| 其中 `agent_thought_chunk` 行 | **24,446** |
| `agent_message_chunk` 行 | 1,572 |
| 内联 `<svg>` / `<path>` | 24,563 / 24,589 |
| 时间线 `scrollHeight` | **615,467px**（可视高 580px） |
| JS 堆 | 110MB |
| 最长单次 Long Task | **2,999ms** |
| Long Task 累计 | 6,316ms（9 次） |
| 内容可交互（TTI 观测） | 693ms 时 0 行 → **7,431ms 时 26,342 行** |

根因（源码已核对）：

1. **流式 chunk 未合并**：`panel/src/views/RunTimeline.tsx:114` 对每一条事件都 `lines.map(... => <EventRow/>)`，`:138-141` 把每个 `agent_message_chunk` 单独渲染成一个 `<MessageRow>`+`Markdown`，`:143-153` 把每个 thought chunk 单独渲染成一行。24,446 行承载的 thought 正文合计只有 **82,272 字符**（平均 3.4 字符/行）——即"每个 token 一个 DOM 行"。
2. **图标逐行内联 SVG**：`RunTimeline.tsx:149` 每行渲染 `<Icon name="thought"/>`，2.4 万行 → 2.4 万个 SVG。
3. **零虚拟化**：`:114-116` 全量渲染；`index.css:417` 只做了 `max-height: 580px` 裁切，节点一个没少。
4. **自动滚动基于 `scrollIntoView`**：`:84-86` 每来一行触发一次 `scrollIntoView`，在 61 万像素高、2.6 万子节点的容器上反复触发布局。

建议（按顺序）：
- **合并**：把连续同类型 chunk 在 state 层累积成一条（thought→一条、message→一条），行数从 2.6 万降到百级。
- **虚拟化**：时间线/对话/会话查看器接 `react-window` 或自己写 `content-visibility: auto` + 窗口切片。
- **图标**：`Icon` 改用共享 `<symbol>` sprite 或 CSS `mask-image`，避免每行一份 path 数据。
- **滚动**：容器 `scrollTop = scrollHeight` 直写，替代 `scrollIntoView`。

---

## 三、P0：错误态 = 空态 / 永久 spinner

**实测**：用 CDP 屏蔽 `*/api/v1/*` 后逐页复访。

- ✅ 好的部分：`App.tsx` 的全局健康检查会出**红色横幅**「daemon 不可达 — 无法加载数据，正在自动重试」，13 个页面都在。这一点比常见本地工具强。
- ❌ 但页面正文同时渲染"正常空态"：`#board` → 「还没有任务 / 创建一个任务…」、`#inbox` → 「收件箱是空的」、`#memory` → 「暂无记忆」。横幅和"空"互相打架，用户会以为是"真的没有数据"。
- ❌ **`#task/<id>` 是死胡同**：屏蔽 API 后 7 秒仍是 `任务…` + 转圈，永不报错、永不重试。源码对应 `TaskDetail.tsx:59` 的 `.catch(() => {})`（吞掉异常）与 `:68` 的 `if (!task) return <Spinner label={t("board.title")}…>`（标签还写成了"看板"）——`TaskDetail.tsx:59,64,68` 已逐行核对。

建议：加一个共享 `useAsync`（`idle/loading/error/empty/data` + `retry()`），把 ~30 处 `catch(() => setX([]))` / `catch(() => {})` 换掉；错误态用 `Alert` + 「重试」按钮，而不是 `Empty`。**这是全场性价比最高的一条。**

---

## 四、P1：无障碍（全部实测，非推断）

**4.1 焦点指示不一致**（Tab 走查，`#chat` 21 个停留点）

- antd 组件：`outline: 3px solid rgb(215,222,252)` —— 清晰。
- **完全无焦点环的 5 个**（`ring=false`）：对话输入 `textarea`、会话搜索框、以及「智能体/模型/推理力度」三个 `ant-select-input`。原因即 `index.css:1034,1144` 的裸 `outline: none`。
- 裸元素（`.kanban-card`、`.suggest-chip`、`.brand`、`.kbd-hint`、`.row-btn`）落到 UA 默认 `1px auto rgb(16,16,16)` —— 在 `#101013` 画布上几乎是隐形的。**看板卡片是本页主要交互对象，却只有 1px 近黑环。**

> 注：子代理的静态审计断言"全站没有任何 `:focus-visible` 样式"，**实测不成立**（antd 侧是有的）；真问题是"不一致 + 裸元素环不可见"。

**4.2 命令面板（实测 Ctrl+K）**
- `role="dialog"` + `aria-label` 有；`aria-modal` **缺**；`aria-activedescendant` **缺**；34 个选项中 `aria-selected` 命中 **0** —— 当前选中项只靠 `.cmdk-item.selected` 视觉表达，读屏用户不知道回车会执行什么。
- Tab 实测 10 次仍停在对话框内（34 个可聚焦项够长），但这是"列表够长"而非真正的焦点陷阱。
- `Escape` 可关闭 ✅；**关闭后焦点落到 `BODY`**，没有还给触发按钮 ❌。

**4.3 会话列表**
- 200 行 × 1 个 `24×24` 图标按钮，**既无 `aria-label` 也无可见文字**（`Tooltip` 的 `title` 不算无障碍名）→ 实测 200 个 unlabeled。源码：`Sessions.tsx:129-140`，按钮嵌在行 `<button>` 内（`:96` 的 `row-btn` 里放了 antd `<Button>`，即 `<button>` 套 `<button>`，非法 HTML 且产生两个激活目标）。
- 展开/收起类控件普遍缺 `aria-expanded`（`RunTimeline.tsx:300` 等）。

**4.4 标题层级**：只有首页是 `h1`，其余 12 个路由的主标题都是 `h2`（`.view-bar`）。建议每页一个 `h1`。

---

## 五、P1：对比度（实测 + 计算，含可直接用的色值）

被测文本全部是**真内容**（时间戳、构建号、快捷键提示、分组标题），不是装饰。

| 组合 | 实测/计算比值 | 结论 |
|---|---|---|
| `textQuaternary #787884` on 侧栏 `#161619`（暗） | **4.14:1** | 11px 正文失败（需 4.5） |
| `textQuaternary` on 画布 `#101013`（暗） | 4.36:1 | 失败 |
| `textQuaternary #81818b` on 白（浅） | 3.86:1 | 失败 |
| `textQuaternary` on 画布 `#fbfbfb`（浅） | 3.73:1 | 失败 |
| `textQuaternary` on `#f3f3f4`（浅，app shell） | **3.48:1** | 失败（浅色下最差） |
| `.tag.ok`（浅） | **2.6:1** | 失败 |
| `.pill`（运行中，暗） | 3.99:1 | 失败 |
| `.agent-avatar`（暗，12px） | 2.89:1 | 失败 |
| thought 正文（浅，13px） | 3.86:1 | 失败（也是 `#task` 18,016 次失败的主因） |
| `--graph-edge` 合成后（暗 / 浅） | **1.47:1 / 1.37:1** | 图谱主数据几乎不可见 |

**建议（已验证可行）**：
- 11px 文本不要用 `textQuaternary`，改用现成的 `textTertiary #8d8d98` → 暗色 **5.5:1**，零新增颜色。
- 若必须保留四级灰：暗色 `#8f8f9b`（5.65:1）、浅色 `#6b6b76`（4.75:1 on shell）。
- `--graph-edge` alpha `.14` → `.34`（暗色 3.09:1）。
- 浅色下 `.tag.ok`/`WS_HUES` 需要单独调（后者 10 个色全部 2.03–3.11:1）。

---

## 六、P1：响应式

**实测 390×844（iPhone 尺寸）**：
- `#stats`：`documentElement.scrollWidth = 498` vs 视口 390 → **横向溢出**。定位到根因：`.ant-table` 宽 401px、起点 x=97（表格 6 列，`table-layout: auto`），整页 46 个元素越界。修法：`<Table scroll={{ x: 560 }} />` 或窄屏切卡片列表。
- 其余 12 个路由无横向溢出 ✅（`#sessions` 有 166 个"越界"元素，但都在 `overflow:hidden` 的截断容器内，属预期省略）。
- 侧栏在 992px 断点自动折叠（`App.tsx:98`），但 CSS 侧另有 520/700/940/1100/1240 六个断点（`index.css:203,294,323,622,720,815`），与 antd 的 576/768/992/1200 都不重合 —— 建议统一到一套断点 token。
- 固定像素：侧栏 228/72（`App.tsx:193`）、对话轨 248px（`index.css:773`）、图谱检查器 400px（`:621`）、图谱画布高 520px（`:610`），后者在窄屏不会随视口高度自适应。

---

## 七、P1：轮询与请求放大

CDP 抓 11.5 秒窗口内 `/api` 请求数：

| 路由 | 总请求 | 明细 |
|---|---|---|
| `#`（首页） | **20** | `permissions`×8、`agents/sessions/tasks/stats/memory/knowledge` 各 ×2 |
| `#inbox` | 12 | **全部是 `permissions`×12** —— 两个轮询器叠加（`App.tsx:125` 2s + `Agents.tsx:538` 2s） |
| `#board` | 10 | `permissions`×6 + `tasks`×4 |
| `#stats` | 9 | `permissions`×5 + `stats`×3 + `recall/log`×1 |

问题不在带宽（本地），而在：**每次 tick 都触发整棵子树重渲染**，且**没有任何 AbortController**，慢响应可以覆盖新响应（源码中只有 `Runtimes.tsx:28-41` 与 `Chat.tsx:288-305` 做了序号/存活保护）。

建议：一个可见性感知（`document.hidden` 暂停）的共享轮询器 + 请求排序号 + 卸载即 abort；删除重复的 `/permissions` 轮询。

---

## 八、P2：交互、i18n、信息架构

- **破坏性操作无确认**（与项目自身已有做法不一致 —— `Board.tsx:172`、`Runtimes.tsx:250`、`Agents.tsx:328` 都做了）：知识库文档删除 `Knowledge.tsx:271-286`、版本回滚 `:321`、取消运行 `TaskDetail.tsx:258-275`、清空当前对话 `Chat.tsx:981-991`。
- **写操作无 busy/禁用** → 可重复提交：知识摄取、写记忆、supersede、建实体、收件箱裁决（连点两次 = 两次裁决）。
- **副作用反馈只有 2–4 秒的 toast**，且会把原始错误串/枚举直接抛给用户（`Memory.tsx:254` toast 出 `"superseded"`）。轮询失败会重复弹同一条。
- **i18n 泄漏**：英文界面里出现中文（`Agents.tsx:495`），中文界面里出现英文占位（`Board.tsx:151,160,168` 等 11 处）；`i18n.tsx:1063` 的 `t()` 缺 key 时**静默返回原始 key**，`RunTimeline.tsx:241` 会把 `status.allow_once` 直接渲染出来（daemon 侧 `PermissionKind` 是 snake_case）。
- **导航/状态**：
  - 切路由即销毁视图状态（对话内容、记忆命名空间、看板视图切换、图谱选中），因为 `App.tsx:279-298` 一次只挂载一个视图；
  - `#chat?agent=x` 只在首次挂载时生效（`Chat.tsx:176` 用 `useState` 初值），已在 `#chat` 时再触发命令面板切智能体**不会切换**（源码级结论，未做实测）；
  - 模态不是路由：会话查看器打开后按浏览器返回会**离开页面**（`Sessions.tsx:99`）；
  - 命令面板有全部 12 个路由，但**没有任务条目**；"会话: {title}" 实际只跳到 `#sessions` 列表。
- **信息架构**：`inbox`（需要人决策的阻塞队列）被归到「系统」组里和设置并列；MCP 注册表藏在「智能体」页内，无导航入口、无 hash、无面板命令；设置页文案写"蒸馏策略、运行偏好"但只有蒸馏一张卡；被禁用的角色在 UI 上无法重新启用。

---

## 九、P2：设计系统漂移与打包

- **token 两处手抄**：`theme.tsx:33-81`（JS 对象）与 `index.css:42-99`（`--*`）各自维护同一批颜色，靠一句注释约束同步。
- **四套状态色系统**：`theme.tsx` seeds、`index.css` 的 `--status-*`/`--graph-*`、`ui.tsx:128-141` 的映射、`Sessions.tsx:17-27` 的 `sourceHue`，外加 `Chat.tsx:26-33` 的 10 个裸 hex（浅色下全部低于 3:1）。
- **内联样式泛滥**：113 处 `style={{...}}`，其中 26 处直接写 `fontSize`；`#sessions` 一页就有 732 个带 `style` 的元素；`marginBottom:10` 重复 16 次。
- **字号越界**：注释宣称"不存在其他字号"，实际出现 10px / 12.5px / 17px，且 14px 被配上 22px 行高（5 处）。
- **打包**：单 chunk `index-*.js` **1367KB（gzip 428KB）**，14 个 view 全是静态 import；字体同时产出 woff 与 woff2（0.48MB）。
- **无 ESLint/Prettier/Stylelint 配置**（`panel/package.json` devDeps 只有 playwright/typescript/vite），但源码里有 5 处 `// eslint-disable-next-line react-hooks/exhaustive-deps` 被当作"已检查"—— 实际没人检查 hook 依赖。测试只有 11 个 Playwright E2E（需要活的 daemon），**无单测、无 axe/对比度检测**。
- `index.html:6` 的 `<meta name="theme-color" content="#0a0c0e">` 是静态的，切换浅色后浏览器 chrome 仍是深色。
- `Graph.tsx:674` 把 `window.__graphDebug` 暴露给页面（调试全局泄漏）。

---

## 十、建议的落地批次

| 批次 | 内容 | 影响面 |
|---|---|---|
| **PR-1（半天，收益最大）** | ① 共享 `useAsync` + 错误态/重试，替换 ~30 处吞异常的 catch（含 `TaskDetail.tsx:59,68`）；② 11px 文本改用 `textTertiary`；③ `--graph-edge` alpha 提到 .34；④ 会话行按钮加 `aria-label` | 立竿见影，几乎无风险 |
| **PR-2（1–2 天）** | ① `React.lazy` 拆路由；② 流式 chunk 合并 + 事件列表虚拟化 + 图标 sprite；③ `scrollIntoView` 改直写 `scrollTop` | 任务详情页从 7.4s 降到可用 |
| **PR-3（1 天）** | ① 统一 `:focus-visible`（含删掉两处裸 `outline:none`）；② 面板/模态补 `aria-modal`、`aria-activedescendant`、关闭还焦；③ 表格 `scroll={{x}}` 修移动端溢出；④ 每页一个 `h1` | 无障碍 + 响应式 |
| **PR-4（1–2 天）** | ① 轮询收敛为 1 个 + abort + 序号；② 破坏性操作 `Popconfirm` + busy；③ toast 去原始枚举 | 稳定性 |
| **PR-5（按需）** | 视图状态进 hash（可深链/可后退）、`#chat?agent=` 响应式、命令面板加任务与"会话直达" | 体验提升，改动面最大 |

---

## 十一、复现方式与产物

审计脚本（一次性工具，位于临时目录，可随时删除）：

- `%TEMP%\ruagent-ui-review\cdp-audit.mjs` —— 13 路由全量审计（DOM/对比度/点击目标/溢出/控制台）
- `%TEMP%\ruagent-ui-review\cdp-probe.mjs`、`cdp-extra.mjs`、`cdp-misc.mjs` —— 节点构成 / TTI / 焦点走查 / 离线降级 / 请求计数 / 面板 ARIA
- 原始结果：`audit-desktop.json`、`audit-mobile.json`（390×844）、`audit-light.json`（`?mode=light`）
- 截图 13 张：`%TEMP%\ruagent-ui-review\*.png`（1440×900，Chrome headless）

复现前提：`cargo run -p ruagent -- serve` 已在 8787 运行，然后 `chrome --headless=new --remote-debugging-port=9333`。

---

## 附：本次能确认 / 不能确认

**能确认（实测）**：所有 DOM/节点/尺寸/对比度/请求数/TTI/Long Task/焦点走查/离线降级/面板 ARIA 结论。

**不能确认**：
- 视觉美观度、配色情绪、动画手感 —— 我没有看到像素。
- 长会话（1600+ 条消息）在对话页的流式重渲染开销 —— 需要真实流式复现；源码层面 `Markdown` 未 memo、且每个 chunk 都重建 messages 数组，**推断**为二次方开销。
- 浅色模式下 antd 运行时算法生成的部分背景色（`.tag.ok` 之外的 tag 变体）未逐一校准。
- 图谱 rAF 物理循环的实际帧率。
