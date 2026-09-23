# 前后对比（改前 / 改后 / 阈值）— 39 行全覆盖

> **改后** = build `C048RA32`（round-2 定稿验收，13 路由 × 暗/亮，**26 captures**，`--api-window=11500`）：
> **38 PASS / 0 FAIL / 1 not_measured**（行 20）。原始数字在 `docs/screenshots/audit-run/metrics.json`，
>
> ⚠️ **2026-09-23 02:37 更新（工具与构建都变了，务必读）**：审计工具在收口期间被改了两次、面板也重新构建
> （**`DKadudCh`**），**同一份源码出现三种结论**。三次我都实跑过：
>
> | 运行 | build | 工具 mtime / sha256 | 窗口 | 结论 | FAIL 行 |
> |---|---|---|---|---|---|
> | A（round 2） | `C048RA32` | `00:33:38` / `4b11176a…` | 11.5s | 38 PASS / 0 FAIL / 1 not_measured | — |
> | B | `DKadudCh` | `02:18:02` / `a6281fca…` | 11.5s | 38 PASS / 1 FAIL / 0 not_measured | 行 20 |
> | C（最新） | `DKadudCh` | `02:26:57` / `fdb93db1…` | 默认 2.5s | 37 PASS / 1 FAIL / 1 not_measured | **行 13** |
>
> ⇒ **本表「改后」列 = A（`C048RA32` + 工具 `00:33:38`）**；**行 13/20 的「改后」按 B 与 C 两个绑定分写**（见对应单元格）。
> 行 28 需要 11.5s 窗口（C 用默认窗口 ⇒ not_measured，A/B ⇒ PASS）。三次的锚点未命中都是 **0 行**。
> **引用任何数字前先重跑一次并把「buildId + 工具 mtime + sha256 + 窗口」写在结论旁边。**
> 完整表在 `docs/screenshots/audit-run/report.md`，逐条证据在
> `docs/screenshots/audit-run/round2-verification.md`。
> **改前** = `MASTER.md` §12 各行的**基线列**原文（来源快照 `UX` / `DL` / `M1` / `M2`，即本代改造开始前
> 的实测），逐字引用、不改写；**不把基线当目标**（这是本代反复踩过的坑，见 §12 的「口径类」六例）。
> **阈值** = §12 现行判据（行 16/23 以 `primitives` §11 为准，行 32 以 §12.3 为准）。

## 0. 怎么读这张表

- 三列的关系是 **改前（基线）→ 改后（实测）→ 阈值（判据）**，不是「基线 → 目标」。
- 「改后」列的数字**只对 build `C048RA32`** 成立；换构建必须重跑（跨轮漂移真实存在：longtask 与
  窗口内请求数有 ±1 级抖动，见 round-2 报告 §8 的抖动清单）。
- 行 20 是**唯一未覆盖项**（`not_measured`）：它既不是 PASS 也不是 FAIL，表里照写原因。
- 截图列给的是**文件名**（相对 `docs/screenshots/`）；13 路由 × 暗/亮的完整索引见 §2。

## 1. 逐行对比

| 行 | 指标 | 改前（基线） | 改后（`C048RA32` 实测） | 阈值 | 截图 |
|---|---|---|---|---|---|
| 1 | 暗色单亮度带像素占比（设计面口径） | `DL` 92.9% → 14–75%（中位 ~50%）；`M1` 13.7–83.5%（中位 37.4%） | 设计面 **0.02–24.22%**（最差 `#chat`；含画布读数 24.22/73.01 两列都打印） | ≤75%（设计面；§12.5） | `audit-run/chat_dark.png` |
| 2 | 有真实色度像素占比 | `DL` 0.45–2.9%；`M1` 1.16–5.02% | **1.16–5.57%** | 1.0% ≤ x ≤ 6.0% | `audit-run/*_dark.png` |
| 3 | 信号色像素占比 | `M1` 0.13–0.94% | **0.13–1.08%**（#knowledge 最高） | ≤5.0%（内部 ≤1.5%） | `audit-run/knowledge_dark.png` |
| 4 | 严格口径「描边内容容器」数 | `M1` 12/13 路由 = 0；`#chat` = 1 | **`#chat` 1 / 其余 0** | ≤1 且必须是控件单元 | `audit-run/chat_dark.png` |
| 5 | 全部可见描边元素数 | `M1` 4/11/4/8/**1679**/5/14/5/32/19/70/7/4 | **4–35**（#task 35） | ≤40/路由 | `audit-run/task_dark.png` |
| 6 | 每页 ≥18px 可见文本节点 | `DL`/`M1` 1–23（下界 1 出现在 task/settings/inbox 空态） | **1–23**（豁免页 chat 2 / settings 4 / inbox 1） | 含读数 ≥3；无读数 ≥1；≤40 | `audit-run/agents_dark.png` |
| 7 | 每路由最大字号 | `DL` 32px（9 页）；`M1` 32px（12 路由），chat/settings 20px | **32px**（8 个含仪表盘路由）/ **20px**（5 个无仪表盘） | ≥20px；含仪表盘 ≥32px | `audit-run/home_dark.png` |
| 8 | 字号越界数 | `UX` 10/12.5/17px；`M1` **0** | **0** | 0 | `audit-run/*_dark.png` |
| 9 | 字重越界数 | `M1` 450×1、650×2 | **0** | 0 | — |
| 10 | 圆角越界元素数 | `M1` 5px×13、8px≈240、12px×2、100px×14 | **0** | 0 | — |
| 11 | 带内联 `font-size` 的元素数 | `M1` sessions 314 / task 131 / stats 80 / runtimes 3 | **0** | 0 | — |
| 12 | 带内联 `style` 的元素数 | `UX` sessions 732；`M1` 731 | **17–41**（#stats 41） | ≤50/路由 | `audit-run/stats_dark.png` |
| 13 | 暗色文本对比度失败数 | `UX` #task 18016/19327、#sessions 205/949 | **0**（A/B 两次：WCAG 口径 0）；**C（工具 `02:26:57`）报 `knowledge/dark = 1`** ⇒ 该行现为 **FAIL**，那 1 个节点未定位（需看最新 `report.md` 明细） | 0 | `audit-run/knowledge_dark.png` |
| 14 | 亮色文本对比度失败数 | `M1` `.muted` 4.45、`.tag.ok` 3.34 | **0**（13 路由） | 0 | `audit-run/*_light.png` |
| 15 | 焦点环对比度（最差） | `M1` 手写 暗 4.26✔/亮 **1.69✘**；antd 暗 **1.73✘**/亮 **1.19✘** | 暗 **7.92–9.18**、亮 **5.74–6.16** | 两模式 ≥3.0:1 | `audit-run/*_dark.png` |
| 16 | 无焦点环的 Tab 停留点 | `UX` #chat 5/21；`M1` #chat 5/21、#settings 2/20 | **0/20**（13 路由 × 2 模式；`viaOwner`=0） | 0 | —（Tab 走查，无截图） |
| 17 | 无名称的可交互元素数 | `UX` #sessions 200；`M1` #settings 4 / #task 5 / #chat 2 | **0/18–0/47** | 0 | — |
| 18 | 命中目标（<24px / <32px） | `M1` #task 198、#agents 25、#runtimes 16、#memory 15、#knowledge 12、#chat 11、#settings 7、#home 5 | **<24px 全站 0**；内容区 <32px 最大 **#task 8**（外壳闭集 1 = `kbd-hint 26×45`） | <24px = 0；内容区 <32px ≤10 | `audit-run/task_dark.png` |
| 19 | 横向溢出 | `UX` #stats @390 **+108px**；`M1` #stats @390 +99px | **0**（13 路由 × 390/520/768/1024/1280/1920） | 0 | `audit-run/stats_dark.png` |
| 20 | error 与 empty 可区分 | `UX` #task 屏蔽 API 后 **7s 永久 spinner**（`TaskDetail.tsx:59` 吞异常） | **A：`not_measured`**（工具无注入能力）→ **B：FAIL**（`DKadudCh` + 工具 `02:18:02`，13×2 全部「失败态可见 ✓ · **重试可点 ✗ · 恢复 ✗**」；注入覆盖 home 6/6 · task 3/4 · agents 3/3 · chat 1/3 · runtimes 1/4 · 其余 1/1，外壳仍在 ✓、未渲染成空态 ✓）→ **C：PASS**（工具 `02:26:57`，同源码）。**同一源码两种结论 ⇒ 差别在工具的注入/点击实现，需以最新工具复测定论** | 每路由 `role=alert` + 可用的重试 | —（失败态无截图） |
| 21 | 命令面板 ARIA 缺口数 | `UX` 3 项；`M1` **6 项**（aria-modal/activedescendant/controls/listbox/option/selected 全缺） | **0**（6/6 属性齐全，13 路由 × 2 模式） | 0 | — |
| 22 | 浮层关闭后焦点归属 | `M1` = `BODY` | **`BUTTON.kbd-hint`**（13 × 2） | = 触发元素 | — |
| 23 | 手写展开控件的 `aria-expanded` 覆盖率 | `M1` 旧测法「全站 0」不成立；手写控件实测 **0 个** | **100%**（`#task .tool-head` **7/7**；其余 12 路由无该类控件） | 覆盖率 = 100% | `audit-run/task_dark.png` |
| 24 | 每路由 `h1` 数 | `M1` #home 1；其余 12 路由 **0** | **1**（13 路由） | 恰好 1 | — |
| 25 | `#task` DOM 节点数 | `UX` 153,122；`M1` 153,074 | **487**（全站最重 #stats 612） | ≤3,000 | `audit-run/task_dark.png` |
| 26 | 最长单次阻塞（longtask） | `UX` #task 2,999ms | **106ms**（全站 0 次 >200ms） | ≤200ms | — |
| 27 | 首屏 JS 体积 | `UX` 1367KB（gzip 428KB）；`M2` 1406KB / gzip 436KB（单包） | **应用层首屏 94KB / gzip 31KB**（首屏静态 1114KB − 实测地板 1020KB；结构断言 ①② ✔） | ≤150KB / gzip ≤48KB（地板 980KB 只登记） | —（产物数字） |
| 28 | 11.5s 窗口内 API 请求数 | `UX` #inbox 12（`permissions`×12）；`M2` 逐路由非 permissions 最大组 home 2 / task 6 / graph 1 / runtimes 1 / settings 1 / inbox 0 | 逐路由最大组 **home 2**（`/agents`）· chat 2 · sessions 2 · board 3 · **task 7** · memory/knowledge/graph/runtimes/settings 1 · agents 2 · stats 2 · inbox 0；**`/permissions` 全站最大 7**（只查一次） | 逐路由最大组 ≤7 ∧ `/permissions` ≤7 | — |
| 29 | 暗色材质台阶（canvas→panel） | `DL` ~2.5x；`M1` 2.48x | **2.47x**（`#0f1014 → #1b1e24`，13 路由一致） | ≥2.4x | `audit-run/home_dark.png` |
| 30 | 亮色 panel 的 ring | `M1` `#e7e9ed` → 对 pageCanvas **1.1340:1** | **`#dee0e4` → 1.23:1**（对 panel 1.32:1）—— **令牌已改（t30）** | ≥1.2:1 | `audit-run/*_light.png` |
| 31 | 图谱边对比度 | `DL` 暗 3.09；`M1` 暗 3.08✔ / 亮 **2.17✘** | 暗 **3.1:1**（α0.34）/ 亮 **3.41:1**（α0.5） | ≥3.0 两模式 | `audit-run/graph_dark.png` |
| 32 | 信号令牌唯一性 | `M1` ① `--status-warn === --signal` ② primary 混入 accent | **0 组重复 / 5 个令牌**；`warn`↔`signal` Δhue **12.8°**（暗）/ **9.5°**（亮） | 0 组重复 ∧ Δhue ≥8° | — |
| 33 | 图谱分类色非文本对比度 | `M1` 暗 4.69–7.16；亮 4.58–5.88 | 暗 **4.69:1** / 亮 **4.91:1**（最低，9 色） | ≥3.0 两模式 | `audit-run/graph_light.png` |
| 34 | 无 `:active` 反馈的手写可点元素 | `M1` 4 类（`.row-btn`/`.kanban-card`/`.guide-card`/`.suggest-chip`） | **0** | 0 | — |
| 35 | 断点值个数 | `M1` 6（520/700/940/992/1100/1240）+ antd 4 | **CSS `[520, 768, 1024, 1240]`（4 个去重值）+ JS 契约值 992**；源文件 **15 处 `@media`** = **断点 query 12 处** + **非断点 3 处**（`prefers-reduced-motion` ×1 / `hover: none` ×2） | ≤4 CSS 断点 + 1 契约值（**行 35 数去重后的值**） | — |
| 36 | 侧栏宽度 | `M1` 228 / 72（≤992 折叠） | **228 @1440 / 72 @768**（13 路由） | 不变 | `audit-run/home_dark.png` |
| 37 | 间距越界值个数 | `M1` 19 个非阶梯值 | **作者值分子 0**（raw 17–57 全部归因：antd css-in-js / UA 默认 / `auto` 解析 / 契约 `clamp()` / 具名出血） | 0（§12.6 只判设计者写的值） | — |
| 38 | 严格口径描边容器：`#chat` 的例外 | `M1` 1 = `.composer` | **1 = `.composer`**（恒定；其余 12 路由 0） | ≤1 且必须是 `.composer` | `audit-run/chat_dark.png` |
| 39 | 单路由 `/api` 请求数（首屏条目级展开 + 窗口内轮询腿） | `M2` 修前 `#graph` = **56**（1×`/graph/entities` + **55×`/graph/entity/<id>`**） | **`#graph` R=1, K=1 ⇒ 1 ≤ 4 ✔**；13/13 PASS（**board 3≤4**、**agents 6≤8** 已由 t57 转 PASS；#task 11≤12） | `R ≤ 2K+2`（R 排除 `/permissions`） | `audit-run/graph_dark.png` |

## 2. 截图索引（改后，26 张）

审计工具每次全量运行都会重写这一组（viewports 1440×900、deviceScaleFactor 1、locale zh-CN）：

| 路由 | 暗色 | 亮色 |
|---|---|---|
| `#home` | `audit-run/home_dark.png` | `audit-run/home_light.png` |
| `#chat` | `audit-run/chat_dark.png` | `audit-run/chat_light.png` |
| `#sessions` | `audit-run/sessions_dark.png` | `audit-run/sessions_light.png` |
| `#board` | `audit-run/board_dark.png` | `audit-run/board_light.png` |
| `#task` | `audit-run/task_dark.png` | `audit-run/task_light.png` |
| `#memory` | `audit-run/memory_dark.png` | `audit-run/memory_light.png` |
| `#knowledge` | `audit-run/knowledge_dark.png` | `audit-run/knowledge_light.png` |
| `#graph` | `audit-run/graph_dark.png` | `audit-run/graph_light.png` |
| `#agents` | `audit-run/agents_dark.png` | `audit-run/agents_light.png` |
| `#runtimes` | `audit-run/runtimes_dark.png` | `audit-run/runtimes_light.png` |
| `#stats` | `audit-run/stats_dark.png` | `audit-run/stats_light.png` |
| `#settings` | `audit-run/settings_dark.png` | `audit-run/settings_light.png` |
| `#inbox` | `audit-run/inbox_dark.png` | `audit-run/inbox_light.png` |

**改前（基线的可视记录）**：`docs/screenshots/before/home.png`、`before/board.png`、`before/chat.png`
（M1 快照，仅 3 条路由）。更早的中间轮次在 `docs/screenshots/after/`（32 个文件，含 `*_dark.png` /
`*_light.png` 与状态变体如 `agents-roles-tab.png`）—— **它不是「改前」**，是改造过程中的轮次快照，
引用时请标明轮次。

**审计自产的两份完整证据**：`docs/screenshots/audit-run/report.md`（工具的全部表：逐路由 × 逐指标 +
§5.13 目标解析逐行核对）与 `audit-run/metrics.json`（1.4MB 原始数字）。

## 3. 数字是怎么来的（可复跑）

```bash
cd panel
npm run build                                              # buildId → dist/index.html
find src index.html vite.config.ts | sort | xargs cat | sha256sum   # 源码指纹
node tools/design-audit.mjs --self-test                     # 203/203
node tools/design-audit.mjs --check                         # §12 判定表（exit 1 = 有 FAIL）
node tools/design-audit.mjs --json --api-window=11500 > /tmp/a1.json   # 全量 26 captures ≈ 6.5 min
npx playwright test                                         # 41 项：39 passed / 2 skipped
```

**绑定要求**：引用本表任何数字时必须同时给出 **buildId + `tools/design-audit.mjs` 的 mtime + 源码指纹**；
`staleSrc = true` 时本轮数字作废（先 `npm run build`）。本轮三项：`C048RA32` /
`2026-09-23 00:33:38 (+0800)`（sha256 `4b11176a…`）/ `871d4659f0f2a259…`。

## 4. 三处「改后仍要注意」的边界（不是 FAIL，但零余量）

| 行 | 路由 | 当前值 | 上限 | 什么会翻 |
|---|---|---|---|---|
| 28 | `#task` | 逐路由最大组 **7** | 7 | 任何额外取数 |
| 39 | `#task` | R = **10–11**（K = 4–5，数据依赖） | 2K+2 = **10–12** | 再多一类端点 |
| 1 | `#chat` | 设计面 24.22% / **含画布读数 73.01%** | 75% | 现行口径安全；「含画布」读法距上限 1.99pp |
| **13** | `#knowledge` 暗色 | 工具 `02:26:57` 报 **1 处 WCAG 文本对比度失败**（A/B 报 0） | 0 | **待定位那 1 个文本节点**（看最新 `report.md` 明细）；先确认是判据收紧还是真失败 |
| **20** | **全站 13 路由** | B：**重试按钮渲染了但不可点 / 点了不恢复**；C：**PASS** | — | **以最新工具复测定论**（同一源码两种结论 ⇒ 先查工具的注入/点击实现，再谈视图修复；命令面板 `.cmdk` 的同类重试实测可点且能恢复） |

（另见 `docs/design-language.md` 的「护栏」表与 round-2 报告 §9 的登记项：F-7、图谱选中环用信号色、
亮色 claude 对自身底色 2.81:1、`@520` 的 X4 parity 差异。）
## 5. 本次收口改动清单（改前 → 改后）

只动文档，**`panel/` 零写入**。逐条列出（改前 → 改后）：

| # | 文件 | 改前 | 改后 |
|---|---|---|---|
| 1 | `docs/design-language.md` | 71 行，只有 4 个决策 + 一张「Before/Now」表，数字停留在 M1/DL 快照，指向已不存在的 `docs/screenshots/final_*` | **161 行**：状态与权威来源表（6 份文件各自是什么）、四个决策带实测数字、测量方法（5 条命令）、快照表、**护栏表（零余量）**、开放项（含工具版本翻转）、变更纪律三条 |
| 2 | `MASTER` 行 30 判定列 | 「**令牌须改**：亮色 `borderSecondary` `#e7e9ed` → `#dee0e4`」 | 「**令牌已改（t30 已落地，2026-09-21）**：… ⇒ 对 canvas 1.2330」（**数字一字未动**） |
| 3 | `MASTER` 行 39 名称/判据 | 「单路由**首屏** `/api` 请求数（防「按条目展开」的 N+1）」；依据列「首屏 = load → domStable + settle」 | 「单路由 `/api` 请求数（**首屏条目级展开**为主判据 + **窗口内轮询腿**为同源子类）」；依据列写明**实现在 11.5s 窗口上跑**、t57 的 #board(3s)/#agents(5s) 等距证据；**`R ≤ 2K+2` 不变**，t44 的原始意图保留为主判据 |
| 4 | `MASTER` §12.10.2 ③ | 「**③ 预算 150KB 的推导**：应用层实测 = 1406 − 980 = 426KB…」 | 首句补明「**本句的 426KB 是「优化前的全量应用层」，判定用的是首屏口径**」，并新增「**③ 的两个口径不可混用**」段（426/391KB 全量 vs **首屏静态 − 地板 = 94KB**）；**推导算式 426 − 229 − 119 = 78KB 完整保留** |
| 5 | `primitives` §11 行 23 | 指标「`[aria-expanded]` **出现数**」，目标「≥ 手写展开控件数」 | 指标「`[aria-expanded]` **覆盖率**」，目标「**覆盖率 = 100%**」（与 MASTER 行 23 及解析锚点 `= N%` 统一；MASTER 行 23 本身已是覆盖率） |
| 6 | `primitives` B1 断点行 | 落到哪 = 「**7 处 media query**」 | 「**两个口径都写**：**15 处 `@media` 规则块** = 断点 query **12 处**（768×5 / 1024×3 / 520×3 / 1240×1）+ 非断点 **3 处**（`prefers-reduced-motion` ×1 / `hover: none` ×2）；**去重后的断点值 4 个** = {520,768,1024,1240}」+ **与行 35 的关系**（行 35 数的是去重后的值）+ 原「7 处」是收敛前计数 |
| 7 | `primitives` §5.2.1 | 只有「实测缺口 6 项」与落地前的 DOM 快照 | 顶部加**状态块**：「**已全部落地（t53 + t56）—— 可关闭**」+ 逐条实测证据（tabIndex={-1} 34/34、aria-label=「命令」、group 去重播报、45 次 Tab escapedAtTab=-1…），并声明下表是**落地前快照，勿据此判现状** |
| 8 | `view-home.md` §4 行 7 | 「`api.pendingPermissions()` → `PendingPermission[]`：待办卡…」 | 划掉旧写法，改为「**外壳 prop `inbox: number \| null`**（**t50 后本页不再调该 API**）」+ 计数由外壳单方拥有（2s 轮询、仅 `#inbox` 让渡）+ 原写法是行 28 重复轮询的来源 |
| 9 | `view-inbox.md` I5 | 目标「**≤3**（合并为 1 个、2s、可见性感知）」 | 目标「**≤7**」+ **§12.8 的修正理由**（1 个 2s 轮询器在 11.5s 内必然发 7 次，≤3 永不可能 PASS）+ 现状「全站 `/permissions` 最大 7 ≤7 ✔」 |
| 10 | `view-inbox.md` §4 行 1 | 「`api.pendingPermissions()`（2s 轮询）」归给本页 | 注明「**t50 后由应用外壳持有该轮询器**，仅在 `#inbox` 让渡；本页只消费」 |
| 11 | `view-graph.md` G15 | 目标「**≤8**（含外壳 `/permissions`；修后实测 7）」 | 目标「**`R ≤ 2K+2`**（行 39；R **排除** `/permissions`）：修后实测 **R=1, K=1 ⇒ 1 ≤ 4 ✔**」+ 外壳轮询由行 28 单独判（7 ≤7）+ 原绝对数写法已由 §12.10.3 取代 |
| 12 | `MASTER` §12.9 ⑤ 表 | runtimes 内容区 **3**、task 内容区 **9**、「最大 **#task 9**」、边界注①「按工具当前的「每路由总数」判定，#task 10/≤10 恰好压线」 | runtimes **0**、task **8**（列出 8 个 `<details>` 命中区）、「最大 **#task 8**」、边界注①改为「**判定口径已切到分区（t39 落地）**；旧口径 9/≤10 只作对照；**真正零余量的是行 28 与行 39**」 |
| 13 | `MASTER` §12.10.1 ④ | 只有缺陷描述与「修法」 | 追加「**2026-09-23 已闭环（t50）**」+ 实测（`#home` 逐路由最大组 **2 ≤7**、`/permissions` 全站最大 **7 ≤7**） |
| 14 | `MASTER` 行 20 依据列 | 「CDP 屏蔽 `*/api/v1/*`，检查 `[role=alert]` 存在且 `.ant-empty` 不出现」（工具未实现时的占位描述） | t59 已实现的注入口径（只打该路由自己的数据端点、排除外壳 `/permissions`、三条分开判、外壳仍在）+ **状态随工具版本翻转**的两次绑定（`02:18:02` FAIL / `02:26:57` PASS） |
| 15 | `MASTER` 行 13 判定列 | 「**0**」 | 「**0**（**当前观测**：工具 `02:26:57` 在 `DKadudCh` 上报 `knowledge/dark = 1` ⇒ 现为 FAIL，待定位那 1 个节点；工具 `02:18:02` 与 round-2 均报 0 —— 同为工具版本差异）」 |
| 16 | `MASTER` 行 27 依据列 | 只到「归类未落地前行 27 判 `not_measured`」 | 追加**当前构建实测**：应用层首屏 94KB/gzip 31KB ✔、首屏静态 1114KB、地板 1020KB、16 chunk、结构断言 ①② ✔ |
| 17 | `acceptance-report.md` 抬头 | 「本报告是 round 1 验收，不是定稿」 | 加**取代声明**：指向 round-2 报告与 `before-after.md`；列出**读它的三条注意**（行 27 FAIL 是旧判据 / 行 28 默认窗口 not_measured / 行 1 target 曾被短暂破坏又复原）；说明它作为**修复前基线**的价值 |
| 18 | `docs/design/before-after.md` | 不存在 | **新建（131 行）**：39 行「改前 / 改后 / 阈值」三列 + 截图文件名索引（26 张 + 改前 3 张）+ 复跑命令与绑定要求 + 零余量边界 + 本清单 |

**验证**（改动后复跑）：`--check` 的**锚点未命中 0 行**（Warnings 只剩 `.nav-badge 本轮未渲染` 一条）；
`npx playwright test` = **39 passed / 2 skipped**；一致性扫描（旧口径关键词：`≤350KB` / `出现数` /
`7 处 media query` / 行 28 的 `≤3` / 行 39 的旧名 / 行 20 的 `not_measured`）**只剩历史引用与 round-1 快照**，
**没有一处把旧口径当现行判据**。

