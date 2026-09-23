# 全站设计验收报告 — 13 路由 × 暗/亮（t8 / attempt 1）

> ⚠️ **本文件是 round 1 快照（t8，构建 `D6UEuuvX`），已被 round 2 取代。** 定稿结论见
> **`docs/screenshots/audit-run/round2-verification.md`（t46，构建 `C048RA32`：38 PASS / 0 FAIL / 1 not_measured）**
> 与 `docs/design/before-after.md` 的对照表。
>
> **读本文件时必须知道的三件事**（否则会把历史当现状）：
> 1. 它的**行 27 = FAIL** 是**旧判据**（≤350KB / gzip ≤120KB）下的结论；该判据已由 **§12.10.2** 修订为
>    「地板（登记）+ 应用层首屏 ≤150KB（判定）+ 两条结构断言」，round 2 实测 **94KB / gzip 31KB ⇒ PASS**。
> 2. 它的**行 28 = not_measured** 是默认 2.5s 窗口下的状态；t44 落槌后（§12.10.1）行 28 改为
>    「逐路由最大组 + `/permissions` 只查一次」双断言，round 2 在 **11.5s 窗口**下判 **PASS**。
> 3. 它的**行 18 数字（#task 内容区 8）与外壳闭集**、**行 37 残差**、**F-7** 等条目仍然有效，但**行 1 的 target**
>    （当时正确）后来被 t47 修过的表读取器短暂破坏、又已复原；现状以 round 2 报告为准。
>
> 本报告保留的价值：它是**修复前**的量化基线（构建 `D6UEuuvX`），before-after.md 的「改前」列引用它。
> 执行期间我实测到 2 次 `npm run build` 变红（`Settings.tsx:32 'toast' unused`、`Graph.tsx:84/95 useMemo 未导入`），
> 都发生在 t42 的编辑过程中，最终构建已绿。

## 0. 本轮的事实基线（每条结论都必须与这三行绑定）

| 项 | 值 |
| --- | --- |
| 审计工具 | `panel/tools/design-audit.mjs` — mtime **2026-09-22 02:29:54 (+0800)**，212,303 B，sha256 `2770de29955dc29c10b3f1da39bbc35457d3576c239efe07788370f4d2b609a6` |
| 工具依赖 | `panel/tools/lib/contract.mjs` mtime 02:25:00（sha256 `54eba3ffba05fa3e…`）；`lib/thresholds.mjs` 2026-09-20 22:44:13 |
| 构建标识 | **`index-D6UEuuvX.js`** 1,439,939 B / gzip 448,588 B ＋ `index-Bs2J3_3z.css` 50,155 B / gzip 9,582 B（`npm run build` 于 22:18:13 完成） |
| `staleSrc` | **false**（`meta.dist.newestSrc` 1790086525798 < `newestDist` 1790086693372）⇒ 本轮数字有效 |
| 源码指纹 | `find src index.html -type f \| sort \| xargs cat \| sha256sum` = `a951e027cb9d72604eccd9a6a10d6e8e79f1496ea279b6c9734a1967f574ac34`；构建前 / 全量①后 / 全量②后 / 报告前**四次取样一致** ⇒ 26 个 capture 同源 |
| 环境 | daemon `http://127.0.0.1:8787` 提供 `panel/dist`；viewport 1440×900，deviceScaleFactor 1，locale zh-CN，settle 2500ms；溢出档 390/520/768/1024/1280/1920 |
| 工具自检 | `node tools/design-audit.mjs --self-test` → **160/160 pass**（含行 18 的 §12.9 内容区口径 / 外壳闭集 / 常数前提三组可证伪断言） |

**工具 mtime 校验**：上表的 mtime 在**本轮 4 次审计运行前后各取样一次，均未变化** —— t10 评审期间
「工具被并发修改导致更早结论作废」的情形本轮不存在；本报告引用的每一条判据都指向同一份
`design-audit.mjs`。

## 1. 结论摘要

| 项目 | 结果 |
| --- | --- |
| §12 审计（13 路由 × 暗/亮，全量） | 26 captures · **35 pass / 1 fail / 2 not_measured / 0 pending** |
| 唯一 FAIL | **行 27 首屏 JS 体积**：1406KB（gzip 438KB）vs ≤350KB（≤120KB），13 路由 × 2 模式全中（单包，非视图引入） |
| not_measured | **行 20**（需 CDP 请求屏蔽 + 失败态注入，本轮未实现）、**行 28**（默认 2.5s 窗口 < 11.5s 不判） |
| e2e | `npx playwright test` → **39 passed / 2 skipped**（41 项，与基线一致） |
| X4 端到端 | 8/8 路由传 `grid` 且 @390 = 2 轨；>520 六档 A/B **全部 EQUAL**（@520 有 1 处按设计的差异，见 §8） |
| i18n | zh 585 = en 585、0 重复、0 差集、780 条字面 `t()` 全部可解析 → **PASS** |
| 抖动 | 两次全量 **判定 0 行不同**；10 处数值差异全是 longtask 时长与 2.5s 信息窗口内的 ±1 请求数 |
| 行 18 | 内容区口径：最大 `content32` = **#task 8/≤10（余量 2）**；外壳 `shell32`=1 ⊆ §12.9 具名闭集 ✓；`<24px` 全站 0 |
| 行 37 | 0 作者值越界（Runtimes.tsx:222 的内联 `marginTop 14` 已清零，现存唯一内联间距是 `marginTop: 16`，在阶梯上） |
| 行 28（补充 11.5s 窗口） | 9/13 路由 FAIL；**#graph 7/≤7 PASS** —— t13/t44 前提里的「63 个请求 / 55× `/graph/entity/<id>`」在 D6UEuuvX 上**不可复现**（见 §6、§13） |

## 2. 运行记录（次数、参数、差异）

| # | 命令 | 时间 | captures | 用时 | 结果 | 产物 |
| --- | --- | --- | --- | --- | --- | --- |
| ① | `node tools/design-audit.mjs --json` | 22:18→22:21 | 26（13×2） | 153.9s | 35 pass / 1 fail / 2 not_measured，exit 0 | `docs/screenshots/audit-run/{metrics.json,report.md,26 png}` |
| ② | `node tools/design-audit.mjs --json --out=docs/screenshots/audit-run2` | 22:21→22:24 | 26 | 153.3s | 同上（**判定逐行相同**） | `docs/screenshots/audit-run2/…` |
| ③ | `node tools/design-audit.mjs --check` | 22:27 | 26 | — | 同判定表，**exit 1**（因行 27） | 无（stdout 表格） |
| ④ | `node tools/design-audit.mjs --check --modes=dark --api-window=11500 --no-shots --no-pixels`（**补充诊断**，非验收依据） | 22:33→22:37 | 13（暗色） | 192.8s | 30 pass / 2 fail（27、28）/ 6 not_measured（1、2、3、14、20、30 —— 只跑暗色，需双模式的行走 not_measured） | 临时目录 |

- ①②③ 都是**全量**（13 路由 × 暗/亮），未使用 `--routes=` 子集；④ 是刻意的子集/单模式补充诊断，
  只用于把行 28 从 not_measured 变成有数字，**不作为任何 PASS/FAIL 的依据**。
- 运行① 的 stderr 只有一条 warning：`.nav-badge 本轮未渲染（inboxCount = 0）`（见 §12 登记项）。
- ①② 的差异见 §10「抖动检查」。
- 运行② 的 `--out=docs/screenshots/audit-run2` 被工具按 CWD 解析成 `panel/docs/screenshots/audit-run2`；
  我已把该目录**整体移出**到仓库根 `docs/screenshots/audit-run2/`（`panel/docs/screenshots/` 下原有的
  `audit-t7 / audit-t7b / audit-t42` 未动）。

## 3. §12 逐行判定（38 行）

判据出处列：**MASTER** = §12 原文；**§12.3** = 行 32 的判据修正；**primitives §11** = 行 16/23 的权威判据
（MASTER 原文在这两行有缺陷，按契约让位）。

| 行 | 判据出处 | 目标（摘自契约） | 判定 | 实测（暗色，除注明） |
| --- | --- | --- | --- | --- |
| 1 | MASTER §12.5 | 暗色单亮度带 ≤75%（口径＝「被设计的面」，排除应用画布） | **PASS** | 设计面 0.02–24.22%（最差 #chat 24.22%）；含画布对照 14.21–83.46% |
| 2 | MASTER | 有效彩度像素 1.0%–6.0% | **PASS** | 1.16%（#inbox）– 5.57%（#task） |
| 3 | MASTER | 信号色像素 ≤5.0%/路由（内部 ≤1.5%） | **PASS** | 0.13%（#inbox）– 1.08%（#knowledge） |
| 4 | MASTER | 严格口径描边容器 ≤1，且必须是控件单元 | **PASS** | 仅 #chat 1/≤1（= `.composer`，行 38 同一对象），其余 12 路由 0 |
| 5 | MASTER + view-task-detail D11 | 全部可见描边元素 ≤40（排除 `.md th/td` 与 `.raw`） | **PASS** | 4–35（最差 #task 35/≤40） |
| 6 | MASTER §12.1 | ≥18px 文本：含读数页 ≥3；豁免页（#chat/#settings/#inbox）≥1；所有页 ≤40 | **PASS** | 豁免页 chat 2 / settings 4 / inbox 1；其余 3–23（#agents 23） |
| 7 | MASTER | 每路由最大字号 ≥20px；含仪表盘页 ≥32px | **PASS** | 含仪表盘 8 路由 = 32px；无仪表盘 5 路由 = 20px |
| 8 | MASTER | 字号越界 0 | **PASS** | 全 13 路由 0 |
| 9 | MASTER | 字重越界 0 | **PASS** | 全 13 路由 0 |
| 10 | MASTER | 圆角越界 0 | **PASS** | 全 13 路由 0 |
| 11 | MASTER | 内联 `font-size` 0 | **PASS** | 全 13 路由 0 |
| 12 | MASTER | 内联 `style` ≤50/路由 | **PASS** | 17–41（最差 #stats 41/≤50） |
| 13 | MASTER | 暗色正文对比度失败 0 | **PASS** | 全 13 路由 0（WCAG 口径亦 0） |
| 14 | MASTER | 亮色正文对比度失败 0 | **PASS** | 亮色 13 路由 0 |
| 15 | MASTER | 焦点环对比度两模式 ≥3.0:1 | **PASS** | 暗色 17–20 个环/路由，最低 7.92:1；亮色 17–20 个环，最低 **5.74:1**（#stats/#inbox 6.16:1） |
| 16 | **primitives §11** | `outlineStyle==='solid' && width>=2px && 环对比度>=3:1`；`auto`/`none` 一律 fail | **PASS** | 全 26 capture **0/20 不合格**；`none` 只出现在 `body`（Tab 绕回伪停留点） |
| 17 | MASTER | 无名称的可交互元素 0 | **PASS** | 0/18 – 0/47 |
| 18 | MASTER §12.9 | `<24px`=0（外壳与内容区同时适用）；`<32px` 内容区 ≤10；外壳常数只查一次 | **PASS** | 见 §4：`content32` 最大 #task **8**；`shell32`=1（`kbd-hint 26×45`）；`<24px` 全站 0 |
| 19 | MASTER | 横向溢出：所有路由 × 所有视口 = 0 | **PASS** | 13 路由 × {390,520,768,1024,1280,1920} 全部 `overflow=0`，`docWidth==innerWidth` |
| 20 | MASTER | API 失败时 role=alert + 重试按钮 | **not_measured** | 需要 CDP 请求屏蔽与失败态注入，本轮未实现（登记项，见 §7） |
| 21 | MASTER | 命令面板 ARIA 缺口 0 | **PASS** | 6/6 属性齐全（dialog/aria-modal/listbox/option/aria-selected/aria-controls），13 路由 × 2 模式 |
| 22 | MASTER（`targetSource=builtin`） | 浮层关闭后焦点回到触发元素 | **PASS** | 13×2 全部 `BUTTON.kbd-hint` |
| 23 | **primitives §11 / §5.2.2** | 只手写展开控件 `.recall-stub-head/.tool-head/.chat-group-more`，覆盖率 100% | **PASS** | #task `.tool-head` **7/7** 带 `aria-expanded`；其余 12 路由无该类控件（空过，非虚假通过：全站计数被明确禁止） |
| 24 | MASTER | 每路由恰好 1 个 `h1` | **PASS** | 13 路由各 1（文案随语言切换，zh 见工具报告） |
| 25 | MASTER | #task DOM ≤3,000 | **PASS** | 477/≤3000（最重路由 #stats 597） |
| 26 | MASTER | 最长单次阻塞 ≤200ms | **PASS** | 0–74ms，**0 次 >200ms**（13×2） |
| 27 | MASTER | 首屏 JS ≤350KB（gzip ≤120KB） | **FAIL** | **1406KB / gzip 438KB**，13 路由 × 2 模式全中；单包（见 §5） |
| 28 | MASTER §12.8 | 11.5s 窗口内单路由 API 请求 ≤7 | **not_measured**（默认窗口 2.5s） | 补充 11.5s 运行：**9/13 FAIL**（home 21、chat 10、sessions 8、board 11、task 17、agents 15、runtimes 11、stats 12、settings 8），PASS 4 条 = memory/knowledge/graph/inbox **各 7**（见 §6） |
| 29 | MASTER | 暗色材质台阶 canvas→panel ≥2.4x | **PASS** | 13 路由一律 **2.47x**（`#0f1014 → #1b1e24`） |
| 30 | MASTER | 亮色 panel ring 存在且对 pageCanvas ≥1.2:1 | **PASS** | 亮色 13 路由一律 `ring #dee0e4 1px → canvas 1.23:1 / panel 1.32:1`（t30 已把 `borderSecondary` 改到 `#dee0e4`） |
| 31 | MASTER | 图谱边对比度 ≥3.0 两模式 | **PASS** | 仅 #graph 有值：暗色 3.1:1（α0.34，底 `#1b1e24`）、亮色 **3.41:1**（α0.5，底 `#ffffff`） |
| 32 | **§12.3** | 检查集 5 个令牌两两不同 + `--status-warn` 与 `--signal` Δhue ≥8° | **PASS** | 两模式各 0 组重复 / 5 个令牌；Δhue 暗色 **12.8°** / 亮色 **9.5°** |
| 33 | MASTER | 图谱分类色非文本对比度 ≥3.0 | **PASS** | #graph 最低 **4.69:1**（暗，9 色）/ **4.91:1**（亮） |
| 34 | MASTER | 无 `:active` 反馈的手写可点元素 0 | **PASS** | 0/3（home/chat）、0/2（sessions/board）、0/1（task/memory/knowledge）；其余路由无候选 |
| 35 | MASTER / primitives M91 | ≤4 CSS 断点 + 1 契约值(992) | **PASS** | CSS 断点 `[520, 768, 1024, 1240]`（4 个）· JS 契约值 `[992]` |
| 36 | MASTER | 侧栏宽 228 / 72 不变 | **PASS** | 13 路由均 **228 @1440 / 72 @768** |
| 37 | MASTER §12.6 | 只判「设计者写的值」：分子 = 0 | **PASS** | 全 13 路由 **0**（对照列 antdDerived/uaDefault/autoResolved/clampVw 单独计数，见 §3.3） |
| 38 | MASTER | 严格口径描边容器 #chat 例外恒定 ≤1 且必须是 `.composer` | **PASS** | #chat 1（`div.composer`），其余 12 路由 0 |

### 3.1 逐路由 × 逐指标实测表（暗色，1440×900；亮色见 `docs/screenshots/audit-run/report.md`）

`hit24/hit32` = 行 18 的**命中区**口径（控件根）；`sh32/ct32` = §12.9 分区；`spaceRaw` = 行 37 的对照 raw
（不参与判定）；`ltaskMax` = 行 26；`sider` 取 @390 档。

| route | text | ≤14 | ≥18 | maxFs | offFS | offW | radOff | inFS | inSty | dom | h1 | aExp | unnamed | hit24 | hit32 | sh32 | ct32 | sh24 | ct24 | spaceRaw | contDoc | ltaskMax | sider |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| home | 92 | 78 | 9 | 32 | 0 | 0 | 0 | 0 | 27 | 326 | 1 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 41 | 0 | 74 | 72 |
| chat | 36 | 33 | 2 | 20 | 0 | 0 | 0 | 0 | 21 | 326 | 1 | 3 | 0 | 0 | 2 | 1 | 1 | 0 | 0 | 26 | 0 | 0 | 72 |
| sessions | 130 | 126 | 3 | 32 | 0 | 0 | 0 | 0 | 38 | 445 | 1 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 57 | 0 | 0 | 72 |
| board | 108 | 98 | 5 | 32 | 0 | 0 | 0 | 0 | 38 | 378 | 1 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 17 | 0 | 0 | 72 |
| task | 99 | 93 | 3 | 20 | 0 | 0 | 0 | 0 | 28 | 477 | 1 | 10 | 0 | 0 | 9 | 1 | 8 | 0 | 0 | 37 | 0 | 63 | 72 |
| memory | 97 | 90 | 5 | 32 | 0 | 0 | 0 | 0 | 19 | 364 | 1 | 1 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 19 | 0 | 0 | 72 |
| knowledge | 58 | 53 | 3 | 32 | 0 | 0 | 0 | 0 | 17 | 322 | 1 | 4 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 27 | 0 | 0 | 72 |
| graph | 33 | 28 | 3 | 32 | 0 | 0 | 0 | 0 | 18 | 234 | 1 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 21 | 0 | 0 | 72 |
| agents | 150 | 124 | 23 | 20 | 0 | 0 | 0 | 0 | 24 | 453 | 1 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 22 | 0 | 0 | 72 |
| runtimes | 67 | 62 | 4 | 32 | 0 | 0 | 0 | 0 | 21 | 341 | 1 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 18 | 0 | 0 | 72 |
| stats | 243 | 236 | 5 | 32 | 0 | 0 | 0 | 0 | 41 | 597 | 1 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 17 | 0 | 56 | 72 |
| settings | 45 | 39 | 4 | 20 | 0 | 0 | 0 | 0 | 27 | 269 | 1 | 1 | 0 | 0 | 3 | 1 | 2 | 0 | 0 | 33 | 0 | 0 | 72 |
| inbox | 25 | 22 | 1 | 20 | 0 | 0 | 0 | 0 | 17 | 219 | 1 | 0 | 0 | 0 | 1 | 1 | 0 | 0 | 0 | 19 | 0 | 0 | 72 |

### 3.2 其他关键证据（暗色；亮色逐路由判定与暗色一致，除行 27 在两模式都 FAIL）

- **行 3 信号色占比**：home 0.69 / chat 0.65 / sessions 0.65 / board 0.49 / task 0.49 / memory 0.71 /
  knowledge 1.08 / graph 0.41 / agents 0.59 / runtimes 0.51 / stats 0.29 / settings 0.78 / inbox 0.13（%）。
- **行 15 焦点环**：20/19/20/20/20/20/19/18/20/19/17/19/18 个环，最低 9.18 / 7.92 / 7.92 / 7.92 / 7.92 /
  7.92 / 9.18 / 7.92 / 8.02 / 9.18 / 9.18 / 8.02 / 9.18 :1；**亮色** 13 路由最低 5.74:1（#stats/#inbox 6.16:1）。
- **行 16 形式分布**：全部 `solid` 除 1–3 个 `none`（`body` 的伪停留点）；**`viaOwner`（环画在焦点所有者上）= 0**
  —— 契约点名的「同一 DOM 暗判 PASS、亮判 FAIL」本轮**两种模式都是 0/20 不合格**，不再矛盾。
- **行 19 溢出**：13 路由 × 6 档全部 `overflow=0 / overflowBody=0`；`siderWidth` 在 ≤768 档 = 72、≥1024 档 = 228。
- **行 25 DOM**：home 326 / chat 326 / sessions 445 / board 378 / **task 477** / memory 364 / knowledge 322 /
  graph 234 / agents 453 / runtimes 341 / stats 597 / settings 269 / inbox 219（上限 3000）。
- **行 29 材质台阶**：13 路由一律 `#0f1014 → #1b1e24` = **2.47x**（≥2.4x）。
- **行 31/33 图谱**：边对比度暗 3.1:1（α0.34）/ 亮 3.41:1（α0.5）；9 个分类色最低 4.69:1（暗）/ 4.91:1（亮）。
- **行 36**：228 @1440 / 72 @768（13 路由）。
- **行 38**：#chat 严格描边容器恰好 1 个，且是 `div.chat-wrap.is-empty > div.chat-empty-zone:nth-of-type(2) > div.chat-bottom:nth-of-type(2) > div.composer`。

### 3.3 行 37 的对照列（不参与判定，逐路由）

分子（作者值越界）= **0**；raw 总数与构成（antdDerived / uaDefault / autoResolved / clampVw / 具名出血）：

| route | raw | antdDerived | uaDefault | autoResolved | clampVw | 具名出血 |
| --- | --- | --- | --- | --- | --- | --- |
| home | 41 | 14 | 0 | 0 | 3 | 24 |
| chat | 26 | 22 | 0 | 0 | 2 | 2 |
| sessions | 57 | 14 | 0 | 0 | 3 | 40 |
| board | 17 | 14 | 0 | 0 | 3 | 0 |
| task | 37 | 24 | 0 | 7 | 3 | 3 |
| memory | 19 | 16 | 0 | 0 | 3 | 0 |
| knowledge | 27 | 16 | 0 | 0 | 3 | 8 |
| graph | 21 | 16 | 2 | 0 | 3 | 0 |
| agents | 22 | 14 | 2 | 3 | 3 | 0 |
| runtimes | 18 | 14 | 1 | 0 | 3 | 0 |
| stats | 17 | 14 | 0 | 0 | 3 | 0 |
| settings | 33 | 21 | 0 | 0 | 3 | 9 |
| inbox | 19 | 16 | 0 | 0 | 3 | 0 |

> 说明：先前报告里「#chat 48 次 / #task 57 次」是 **raw** 口径。§12.6 裁决后只判「设计者写的值」，
> 分子全站 0；raw 的构成已逐项归因（antd css-in-js、UA 默认、`auto` 解析值、契约保留的 `clamp()`、
> 具名出血白名单）。

## 4. 行 18 专章：用的是哪种读法、分区数字、边界事实

### 4.1 读法 = **内容区口径**（§12.9 / t39 已落地，不是「每路由总数」）

工具的 `criterion` 与 `target` 都从契约读入（`targetSource=MASTER.md`），判定式：
**地板 `<24px` == 0（外壳与内容区同时适用）· 内容区 `<32px` ≤ 10 · 外壳具名闭集只查一次**。
display 同时并列打印**旧口径（每路由总数）**与**绘制口径对照**，判定只用内容区口径。

### 4.2 分区数字（暗色 1440×900；亮色逐条相同）

| route | `<24px`（命中区） | `<32px` 内容区 | 外壳 `<32px` | 内容区 `<24px` | 外壳 `<24px` | 旧口径总数 | 绘制口径 `<32px` |
| --- | --- | --- | --- | --- | --- | --- | --- |
| home | 0 | **0**/≤10 | 1 | 0 | 0 | 1 | 1 |
| chat | 0 | **1**/≤10 | 1 | 0 | 0 | 2 | 5 |
| sessions | 0 | **0**/≤10 | 1 | 0 | 0 | 1 | 1 |
| board | 0 | **0**/≤10 | 1 | 0 | 0 | 1 | 1 |
| **task** | 0 | **8**/≤10 | 1 | 0 | 0 | 9 | 10 |
| memory | 0 | **0**/≤10 | 1 | 0 | 0 | 1 | 1 |
| knowledge | 0 | **0**/≤10 | 1 | 0 | 0 | 1 | 1 |
| graph | 0 | **0**/≤10 | 1 | 0 | 0 | 1 | 1 |
| agents | 0 | **0**/≤10 | 1 | 0 | 0 | 1 | 1 |
| runtimes | 0 | **0**/≤10 | 1 | 0 | 0 | 1 | 1 |
| stats | 0 | **0**/≤10 | 1 | 0 | 0 | 1 | 1 |
| settings | 0 | **2**/≤10 | 1 | 0 | 0 | 3 | 3 |
| inbox | 0 | **0**/≤10 | 1 | 0 | 0 | 1 | 1 |

- 全站 `<24px`（命中区）= **0**；外壳 `<24px` = 0；内容区 `<24px` = 0 —— 地板在三种分区下都干净。
- 「旧口径总数 − 内容区」恒为 **1**（= 外壳那一个常数），正是 §12.9 要消除的「同一个常数算 13 次」。
- 两种读法今天**结论相同（都 PASS）**，无分叉需要登记。

### 4.3 #task 的边界事实（design-lead t38 提出的那条）

| 读法 | 数字 | 余量 |
| --- | --- | --- |
| 旧口径（每路由总数） | **9** / ≤10 | 1 个名额 |
| **内容区口径（本报告判定用）** | **8** / ≤10 | **2 个名额** |

**与 t38/t39 报的数字相比已经变了**：t38 报「旧口径 10/≤10 恰好压线、内容区 9」，
t39 复测「task 9/10，内容区 9」。本轮实测是 **9 / 8** —— 差的那 1 项是 **t41 把
`input.ant-input.mono` 从 27px 抬到 32px**（`panel/src/index.css` 的 `.ant-input.mono { min-height: 32px }`），
它从 #task 的 `<32px` 集合里退出了。所以：**t38 的「恰好压线、任何新增小控件都会翻成 FAIL」在内容区
口径下现在是 2 个名额余量**（旧口径也只有 9/10）。这条要同步给 t44/round 2 的读者。

### 4.4 #task / #settings / #chat 的 `<32px` 逐项清单（完整不截断，最小复现用）

#task（内容区 8 项，全部是 `<details>` 的 `summary` 命中区）：

| # | 元素 | 命中区 |
| --- | --- | --- |
| 1 | `div.card.launcher:nth-of-type(2) > details.hint > summary.muted` | 1109×**24** |
| 2 | `div.timeline:nth-of-type(2) > div.ev-row:nth-of-type(2) > details.ev.inject > summary` | 403×**25** |
| 3 | `… div.ev-row:nth-of-type(4) > details.ev.inject > summary` | 91×**25** |
| 4–8 | `… div.ev-row:nth-of-type({11,12,19,20,25}) > details.ev.tool-update > summary.muted` | 242/230/239/246/240×**24** |

#settings（内容区 2 项）：`div.card.distill-settings:nth-of-type(2) > div.row:nth-of-type(3) > button.ant-switch`
48×**24**、`… > div.row:nth-of-type(4) > span.row.tight > button.ant-switch:nth-of-type(2)` 48×**24**
（真实 antd 默认值，t28 裁决 48×24 满足 24 地板；内容区共 2 项，远在 ≤10 内）。

#chat（内容区 1 项）：`div.chat-layout > aside.chat-side > div.chat-side-head.zone-head:nth-of-type(1) > button.ant-btn:nth-of-type(2)`
**24**×32（任一维 <32 即计入；该页另有 4 个内层元素被「命中区上溯」提升到控件根后计入）。

### 4.5 外壳子断言与「常数前提检验」

- **闭集来源 = MASTER §12.9 具名闭集**（工具 `loadShellSet()` 解析契约，不硬编码），读到 **1 项：
  `button.kbd-hint 26×45`**。
- **成员判定用身份而非尺寸**：`shellSetAdmits()` 要求 tag + 全部 class + 几何三者相同；已退役的
  `button.ant-btn 32×32` 与「同尺寸但类名不同」都不被接纳。
- **⊆ 语义**：成员**变少满足**（t36 让 3 个按钮退出正是此例）、**变多 FAIL**。本轮 13 条路由
  `shell32` 逐条 = 1，命中元素逐条都是 `div.ant-layout-sider-children > div.sider-inner > div.sidebar-foot:nth-of-type(3) > button.kbd-hint:nth-of-type(1)`（26×45）⇒ 子集成立。
- **常数前提真的在逐路由比对**：工具 note 写明「26 个 capture 逐条比对（选择器路径 + className + 几何），
  恒定 1 项 `[kbd-hint 26x45]`；**无非常数项**」；非常数项若出现会**回落进内容区预算**（自检里有
  「只在部分路由出现 → 非常数 → 回落 → 把内容区压爆成 11 → FAIL」的可证伪断言）。

## 5. 行 27 专章：数字、组成分析、处置建议

### 5.1 数字（build D6UEuuvX）

`index-D6UEuuvX.js` = **1,439,939 B（1406KB）/ gzip 448,588 B（438KB）**；CSS 另计 50,155 B / gzip 9,582 B。
目标 ≤350KB / gzip ≤120KB ⇒ **超 4.0×（gzip 3.65×）**，13 路由 × 2 模式全部同一数字（单包，非某视图引入）。

### 5.2 组成分析（方法可复现：`panel/e2e/bundle-stats.config.mjs` + `npx vite build --config …`）

Rollup 的 `renderedLength`（**压缩前**渲染尺寸）按模块归属，总计 3,580,243 B；最小化后的实际产物
1,429,470 B（≈0.40 比例）。占比：

| 分组 | renderedLength | 占比 |
| --- | --- | --- |
| **antd + rc-\*** | 1,828,924 | **51.1%** |
| react-dom（含 scheduler） | 665,159 | 18.6% |
| markdown 栈（react-markdown / remark-gfm / micromark / mdast / unified…） | 582,861 | 16.3% |
| 应用代码（`panel/src`） | 416,716 | 11.6% |
| 其他 node_modules | 65,408 | 1.8% |
| react | 21,175 | 0.6% |

最大的单模块：`react-dom-client.production.js` 644,593；`src/i18n.tsx` 54,442；`src/views/Chat.tsx` 40,850；
`src/views/Agents.tsx` 36,118；`@rc-component/tree/es/Tree.js` 34,364；`src/views/Graph.tsx` 34,206。
13 个视图模块合计 298,477（8.3%）。

### 5.3 处置建议：**先修阈值，同时登记可执行的优化项**（不要把 ≤350KB 当可达成目标）

按最小化比例 0.40 折算，理论收益上界：

| 动作 | 估算收益（最小化后） | 备注 |
| --- | --- | --- |
| 路由级代码分割（13 视图 + 视图私有组件） | ≈ **−119KB** | 视图模块 298,477 × 0.40；首屏仍要 shell + 当前路由 |
| `react-markdown`/`remark-gfm` 栈改 `import()` 懒加载（只有 Chat/Wiki/TaskDetail 用） | ≈ **−233KB** | 582,861 × 0.40，是单笔最大可动项 |
| antd 已经是 ESM 按组件引用（无 `antd/dist/antd.css` 全量） | ≈ 0 | 51.1% 里绝大部分是 `@rc-component/*` 与各组件的 style 模块，tree-shaking 已生效 |
| react + react-dom | ≈ 0（不可动） | 686KB rendered / ≈274KB min，框架地板 |

⇒ 两项优化全做完约 **1406KB → ~1050KB**，距离 350KB 仍有 3 倍。**结论：行 27 的 ≤350KB/≤120KB 在当前
技术栈（React 19 + antd 6 + markdown 栈）下不可达**。建议：

1. **修订阈值**（属 design-lead 的裁决范围）：给出算式化的新目标，例如「入口 chunk ≤1.6MB（gzip ≤500KB）」
   并在 §12 里写明「按依赖地板 686KB(react+dom) + antd 51% 推导」，把「不可达」从判据里拿掉；
2. 同时把优化登记为**独立、可验收**的子目标：`lazy(react-markdown 栈)` + 13 视图路由分割（预期 −350KB 左右），
   验收方式 = 本报告的 `bundle-stats.config.mjs` 复跑对比；
3. 若坚持「首屏 ≤350KB」，那等价于**放弃 antd 或改为多入口/多页**，那是产品决策，不是视图实现能补的缺口。

## 6. 行 28 专章：口径待定 + 原始数字（**不写成「重复轮询」**）

### 6.1 默认全量的状态：`not_measured`

默认 `--api-window=2500`，工具只在窗口 ≥11.5s 时才判定（note 原文：`informational: window < 11.5s
(--api-window=11500 to judge)`）。2.5s 信息窗口内的原始计数（暗/亮）：home 9/9、chat 6/5、sessions 2/2、
board 3/3、task 6/7、memory 2/3、knowledge 2/2、graph 2/3、agents 5/5、runtimes 6/6、stats 4/4、settings 4/4、inbox 2/2。

### 6.2 补充运行（暗色，11.5s 窗口）的判定与数字

| route | 11.5s 内 `/api` 请求 | ≤7 | 判定 |
| --- | --- | --- | --- |
| home | **21** | ≤7 | FAIL |
| chat | 10 | ≤7 | FAIL |
| sessions | 8 | ≤7 | FAIL |
| board | 11 | ≤7 | FAIL |
| task | **17** | ≤7 | FAIL |
| memory | 7 | ≤7 | PASS |
| knowledge | 7 | ≤7 | PASS |
| **graph** | **7** | ≤7 | **PASS** |
| agents | 15 | ≤7 | FAIL |
| runtimes | 11 | ≤7 | FAIL |
| stats | 12 | ≤7 | FAIL |
| settings | 8 | ≤7 | FAIL |
| inbox | 7 | ≤7 | PASS |

### 6.3 「载入突发 vs 稳态轮询」的分解（`panel/e2e/api-window-probe.mjs`，独立于审计工具）

同一 11.5s 窗口按「前 2.5s / 之后 9s」与路径拆分（暗色 1440×900，一次加载）：

| route | 合计 | 前 2.5s | 之后 | 构成 |
| --- | --- | --- | --- | --- |
| graph（画布） | 7 | 3 | 4 | `6×/api/v1/permissions`（2 前 / 4 后）+ `1×/api/v1/graph/entities` |
| graph（列表模式，点「列表」后测） | 9 | 4 | 5 | `8×/api/v1/permissions` + `1×/api/v1/graph/entities` |
| inbox | 6 | 2 | 4 | `6×/api/v1/permissions` |
| home | 20 | 9 | 11 | `8×permissions` + `agents/sessions/tasks/stats/memory/list/knowledge/documents` **各 2 次**（1 次在突发、1 次在 2.5s 之后） |
| task | 20 | 9 | 11 | 同 home（6 个摘要端点各 2 次 + 8×permissions） |
| chat | 10 | 6 | 4 | `6×permissions` + `agents/approver/options` 2 + `agents` 1 + `chats` 1 |

**读法（重要，写给 t44 的裁决）**：

- 全站**只有一个稳定的 2s 级轮询器**：`/api/v1/permissions`（每条路由 6–8 次/11.5s，前 2.5s 里只有 1–3 次，
  其余均匀落在之后 —— 这是轮询的形态）。
- home/task 的额外 ~13 次是**同一批摘要端点被取了两遍**（一遍在首屏突发、一遍在 2.5s 之后），
  不是「多出 2 个轮询器」；#graph 只有 1×entities，**没有** 55 次 `/graph/entity/<id>`。
- 因此「≤7 = 1 个轮询器」这条算式**只在「窗口内只计轮询请求」的口径下成立**。若按「所有 `/api` 请求」，
  9/13 路由会因为**载入突发**而 FAIL，与「有没有重复轮询」无关。
- **本报告把 #graph 记为「口径待定 + 原始数字 7/≤7（PASS）」**，不写成「重复轮询」。

## 7. 行 20 专章：为什么 not_measured

`error 与 empty 可区分`（每路由在 API 失败时呈现 `role=alert` + 重试按钮）需要 **CDP 请求屏蔽 +
失败态注入**才能构造失败态；工具当前的 `scope=none`、`note=API 失败时的 role=alert + 重试按钮需要 CDP
请求屏蔽与失败态注入，未在本轮实现`。**本轮 13 路由 × 暗/亮都没有失败态数据**，所以这一行既不是 PASS
也不是 FAIL，是**未覆盖**。可复现的替代证据（部分）：`.error-state` 组件与 `ErrorState` 的用法在
`panel/src/ui.tsx`，但「每路由都接了它」需要失败注入才能验；建议 round 2 用 `page.route('**/api/**',
route => route.abort())` 做注入（不需要 CDP）。


## 8. X4 端到端验收（8 条路由）

工具：`panel/e2e/x4-parity.mjs`（本轮新增，在 t8 的 inScope `panel/e2e/` 内），跑法
`cd panel && node e2e/x4-parity.mjs`。判据：README §3.4 X4 + primitives §3.2 B7。

### 8.1 `grid` 与 @390 的 2 轨

| route | 传 `grid` | @390 display | @390 轨数 | @390 行数 | 单元数 |
| --- | --- | --- | --- | --- | --- |
| home | ✔ | grid | **2** | 2 | 4 |
| board | ✔ | grid | **2** | 2 | 4 |
| memory | ✔ | grid | **2** | 2 | 4 |
| sessions | ✔ | grid | **2** | 1 | 2 |
| knowledge | ✔ | grid | **2** | 1 | 2 |
| graph | ✔ | grid | **2** | 1 | 2 |
| runtimes | ✔ | grid | **2** | 2 | 3 |
| stats | ✔ | grid | **2** | 2 | 4 |

**8/8 通过**（`grid` 类存在 ∧ `display:grid` ∧ 轨数 = 2）。

### 8.2 >520 parity（同一次加载内加/去 `grid`，逐单元 rect 比较）

| route | @520 | @768 | @1024 | @1280 | @1440 | **@1920** |
| --- | --- | --- | --- | --- | --- | --- |
| home（4 项条） | EQUAL | EQUAL | EQUAL | EQUAL | EQUAL | EQUAL |
| board（4 项） | EQUAL | EQUAL | EQUAL | EQUAL | EQUAL | EQUAL |
| memory（4 项） | EQUAL | EQUAL | EQUAL | EQUAL | EQUAL | EQUAL |
| stats（4 项） | EQUAL | EQUAL | EQUAL | EQUAL | EQUAL | EQUAL |
| sessions（2 项） | EQUAL | EQUAL | EQUAL | EQUAL | EQUAL | EQUAL |
| knowledge（2 项） | EQUAL | EQUAL | EQUAL | EQUAL | EQUAL | EQUAL |
| graph（2 项） | EQUAL | EQUAL | EQUAL | EQUAL | EQUAL | EQUAL |
| runtimes（3 项） | **DIFFER**（见下） | EQUAL | EQUAL | EQUAL | EQUAL | EQUAL |

- 实测单元宽（A/B 相同）：4 项条 @768 = 158、@1024 = 177.73/177.75、@1280 = 238.92、@1440 = 277.16/277.17、
  **@1920 = 320**（flex 的 320px 上限生效，t21 报的「1920 时 360」已消除）；2 项条 @768 = 316、@1024 起 = 320。
  ⇒ 与 primitives 记的「偏移判据：轨道 > 320px 才发生」一致：**现在任何宽度都撞不到 320 以上**。
- **t20/t21 各自漏测一档的历史已被覆盖**：本脚本固定跑 520/768/1024/1280/1440/**1920**，1920 不是可选项。
- **@520 的 #runtimes 差异（如实登记，不是缺陷）**：3 项条在 @520 时 `grid` → `[192,192,192]`（2 轨，第三项落到第 2 行第 1 列），
  去掉 `grid` → `[192,192,320]`（flex 换行后孤项吃满 320 上限）。原因是 `@media (max-width: 520px)`
  **包含 520 本身**，所以 520 落在「≤520 特化」之内 —— 「>520 无规则」这条严格成立于 **≥521**。
  登记两点：(a) 判据文字若想覆盖 520，应写成「≥521 parity」或「≤520 按 2 轨特化」；
  (b) 3 项条（runtimes）不在契约的 2 项/4 项分组里，2 轨下第三项独占一行是 2 轨特化的自然结果。

## 9. i18n 字典完整性

工具：`panel/e2e/i18n-check.mjs`（本轮新增）。判据：zh 与 en 键集完全相等 ∧ 无重复键 ∧ 全仓字面 `t()` 可解析。

| 检查 | 结果 |
| --- | --- |
| 键数 | **zh = 585，en = 585** |
| 重复键 | zh **0** / en **0** |
| 键集差 | only-in-zh **0** / only-in-en **0** |
| 字面 `t("…")` 引用 | **780 条，missing-in-zh 0 / missing-in-en 0** → 没有裸 key 会渲染出来 |
| 动态引用（静态不可判定，已列出） | 23 条：`t(\`status.${x}\`)`、`t(\`board.col.${c}\`)`、`t(\`memory.store.${s}\`)`、`t(\`wiki.action.${a}\`)`、`t(\`chat.opt.${id}\`)`、`t(n.labelKey)`、`t(s.key)`、`t(mode==="dark"?"theme.light":"theme.dark")` |
| 动态族覆盖 | `status.*` 12 键、`board.col.*` 4、`memory.store.*` 4、`wiki.action.*` 4 —— 族内键在 zh/en 都存在 |
| 死键候选（登记，不改） | **28 个**：`task.runsCount`、`task.comparison`、`task.cancelling`、`distill.subtitle`、`memory.ns.user`、`graph.back`、`knowledge.score`、`graph.search`、`graph.addFact.targetPh`、`graph.addFact.relationPh`、`graph.newEntity.{namePh,kindPh,btn}`、`mcp.empty.hint`、`task.waitingPerm`、`chat.project`、`chat.projectPh`、`chat.opt.{reasoning_effort,effort,fast}`、`chat.syncedAt`、`toast.memoryWritten`、`toast.written.{inserted,superseded}`、`memory.outcome.{inserted,superseded,skippedDuplicate,rejectedNamespace}`（在 `panel/src` 里**不出现为任何字符串字面量**，也不在任何动态族前缀下） |

**判定：PASS**（失败条件只有重复键/键集差/不可解析引用，三项全 0）。死键候选不影响判定，登记给 round 2 决定是否清理。

## 10. 抖动检查（同一份源码两次全量）

同一 buildId（`D6UEuuvX`）、同一源码指纹（`a951e027cb9d7260…`）下，运行①与运行②逐 capture 对比：

- **判定差异：0 行**（38 行 × 26 capture 的 `verdict` 与每条 `evidence.pass` 全部相同）。
- 数值差异共 **10 处**，全部不影响判定：

| capture | 指标 | 运行① | 运行② |
| --- | --- | --- | --- |
| home/dark | longtask max | 74ms | 66ms |
| task/dark | longtask count/max | 1 / 63ms | 0 / 0ms |
| stats/dark | longtask max | 56ms | 60ms |
| home/light | longtask max | 78ms | 70ms |
| chat/dark | 2.5s 窗口请求数 | 6 | 5 |
| task/dark | 同上 | 6 | 7 |
| memory/dark | 同上 | 2 | 3 |
| knowledge/dark | 同上 | 2 | 3 |
| graph/dark | 同上 | 2 | 3 |
| board/light | 同上 | 3 | 4 |

- **契约点名的两个历史抖动源，本轮都不再抖**：
  - `#task` 的 `<24px` 计数（历史 4/10/11，RunTimeline 渐进挂载）：两次全量 **都是 0**，且
    `hit24/hit32/shell32/content32` 在 26 个 capture 上逐条相同；
  - 行 16 的「同一 DOM 暗 PASS / 亮 FAIL」：本轮暗/亮**都 0/20 不合格**，`viaOwner=0`（环都画在元素自身）。
- 结论：本轮判定可复现；10 处差异是**计时噪声**（longtask 时长）与**信息窗口内 ±1 请求**（行 28 在默认口径下
  不判定的那列），都不是实现漂移。

## 11. e2e

`cd panel && npx playwright test` → **41 项：39 passed / 2 skipped（22.2s）**，与基线一致。

2 项 skip 的**名字**（不是「既有 2 项」了事）：

| 测试 | skip 条件 |
| --- | --- |
| `chat.spec.ts:11:1 › chat round-trip lands in history and reattaches live` | `needs an e2e mock agent (chat with real runtimes costs money)` / agents API 不可用时 |
| `judge.spec.ts:10:1 › fan-out judge picks a winner with provenance` | `needs the e2e mock agents (2 members + judge)` |

本轮新增的 3 个可执行验收工具都在 `panel/e2e/` 且**不匹配 Playwright 的 `*.spec.ts`**，所以 41 项的计数未变
（`x4-parity.mjs` / `i18n-check.mjs` / `api-window-probe.mjs` / `focus-owner-probe.mjs` / `bundle-stats.config.mjs`）。

## 12. 登记项（仍不达标 / 未覆盖 / 遗留）—— 不用模糊措辞

### 12.1 未达标

1. **行 27 FAIL**：1406KB / gzip 438KB vs ≤350KB / ≤120KB。处置建议见 §5.3（修订阈值 + 登记懒加载/路由分割）。
   最小复现：`cd panel && npm run build` → `dist/assets/index-D6UEuuvX.js` 1,439,939 B；或
   `node tools/design-audit.mjs --check` → 行 27 FAIL（13×2 全中）。

### 12.2 未覆盖（not_measured）

2. **行 20**（error vs empty 可区分）：需失败态注入，本轮未实现，工具 `scope=none`。建议 round 2 用
   `page.route('**/api/**', r => r.abort())`。
3. **行 28**（11.5s 窗口 API 请求数）：默认口径 not_measured；补充口径（暗色 11.5s）9/13 FAIL，
   但**判定范围待 t44 裁决**（载入突发 vs 稳态轮询），见 §6。
4. 工具自报的**盲区**（`blindSpots`，原文照录）：`.nav-badge`（仅在 `inboxCount > 0` 时渲染，本轮 13 路由实测 0 个
   ⇒ 它「对白字 3.20:1 的对比度失败」本轮不可见）；`::selection` 字/底对比度；`:hover` 面上的文本对比度；
   动效中间帧；`.chat-thought` / `.md blockquote` 的装饰竖线（无下限要求）；暗色 tag 底色（antd 派生值，未逐变体核对）。
5. 工具 warning（唯一一条）：`.nav-badge 本轮未渲染（inboxCount = 0）`。要复测该行必须先造一条待处理权限。

### 12.3 F-7（#knowledge 自动聚焦 antd Input 的 focus 边框/boxShadow）—— **已实测确认**

工具：`panel/e2e/focus-owner-probe.mjs`（新增）。这是行 16 的**盲区**：Tab 走查从页面初始状态开始，
而视图 `autoFocus` 的元素正是第一个 Tab 会**离开**的那个，所以它的环从未被测到。

| 路由 | `document.activeElement` | outline | border-color | box-shadow |
| --- | --- | --- | --- | --- |
| **#knowledge** | `INPUT.ant-input.ant-input-outlined` 1075×32（`Knowledge.tsx:266` autoFocus） | `solid 2px rgb(245,180,87)`（信号环，合格） | **`1px rgb(207,147,53)`**（= `--ant-color-primary` 派生档） | **`rgba(254,165,21,0.26) 0px 0px 0px 2px`**（primary 派生辉光） |
| #memory / #board / #graph / #chat / #settings / #task | `BODY` | none | — | none（这些页的 autoFocus 只在弹层里，未打开） |
| #home | `BUTTON.ant-btn.ant-btn-primary` 99×32 | `solid 3px rgb(245,180,87)` | 透明 | none |

**结论**：F-7 **成立且已量化** —— #knowledge 的自动聚焦输入框同时戴两套信号：面板的信号环（合格）＋
antd 自带的 primary 边框与 2px 辉光。MASTER §3.2 的白名单 W1–W6 里 W4 只写了「手写 `:focus-visible` 环」与
`.composer:focus-within` 边框，**未列 antd Input 自带的 focus 边框/boxShadow** ⇒ 属**文档缺口**，不被任何
阈值行计数（行 16 因信号环是 solid ≥2px 而 PASS）。登记给 design-lead 决定：补进 W4 的白名单，或让共享层
把 `.ant-input:focus-visible` 的 border/shadow 也归一到信号族。

### 12.4 行 37 的间距残差 —— **已清零**

`Runtimes.tsx:222` 的内联 `marginTop: 14` **不存在了**（t7 已修）。当前 `panel/src/views/*.tsx` 里唯一的
内联间距是 `Runtimes.tsx:338` 的 `style={{ marginTop: 16 }}` —— 16 在 §5.1 阶梯上 ⇒ 不构成越界。
工具判定：全 13 路由分子 = **0**（对照列见 §3.3）。

### 12.5 `#settings` 的 2 个 `button.ant-switch` 48×24

真实 antd 默认值，按 t28 裁决已改到 48×24（过 24 地板）；`<32px` 预算下计 **2 项**，
settings 内容区共 **2** 项，远在 ≤10 内 ⇒ **不需要改**（t40 已就同一问题给过结论，本轮数字一致）。

### 12.6 `panel/` 下遗留的临时文件清单（**只登记，不清理**）

| 路径 | mtime | 大小 | 说明 |
| --- | --- | --- | --- |
| `panel/verify-final.tmp.mjs` | 2026-09-20 01:45 | 4,300 B | 未跟踪散落脚本 |
| `panel/verify-tab.tmp.mjs` | 2026-09-19 23:55 | 1,492 B | 未跟踪散落脚本 |
| `panel/docs/screenshots/audit-t7/`、`audit-t7b/`、`audit-t42/` | 09-22 09:36 / 09:39 / 22:17 | — | 其他任务的截图目录（`audit-t42` 在我执行期间仍在增长） |
| `panel/tsconfig.tsbuildinfo` | 22:18 | 565 B | 增量构建缓存（`npm run build` 的正常副产物） |

**未发现**：`.probe*.mjs`、`.audit/`、`.audit-before.json`（契约点名的这三类在本轮不存在）。
**本轮新增的常驻文件**（都在 inScope 内，属验收工具而非临时垃圾）：
`panel/e2e/{x4-parity.mjs, i18n-check.mjs, api-window-probe.mjs, focus-owner-probe.mjs, bundle-stats.config.mjs}`、
`docs/screenshots/audit-run2/`（运行②的产物，从 `panel/docs/` 移出）。

### 12.7 其他需要读者知道的事实

- 行 4/31/33 只有 #chat（`.composer`）与 #graph 有值，其余路由「—」（无对象）；这不是「没测」，
  是判据的作用域为空。
- 行 32 的检查集是 5 个令牌（`--signal/--status-warn/--brand-claude/--brand-deepseek/--brand-opencode`），
  不是全部颜色令牌。
- 行 23 在 12 条路由「无手写展开控件（空过）」是**判据要求的**（禁止全站 `querySelectorAll`，否则会命中
  antd Select 而虚假通过）；有对象的只有 #task（`.tool-head` 7/7）。
- 本轮我实测到 t42 编辑期间**构建两次变红**（`Settings.tsx:32`、`Graph.tsx:84/95`）—— 记录在案，
  因为它说明「本轮的数字属于哪个源码状态」这件事在并发修复期间是**易变的**。

## 13. 与上游结论的分歧（必须让 t44/round 2 知道）

1. **#graph 的 N+1（55× `/graph/entity/<id>`）在 build D6UEuuvX 上不可复现**。
   - 我用两种独立方法测：审计工具 `--api-window=11500`（#graph = **7/≤7 PASS**）与自写的
     `api-window-probe.mjs`（画布模式 7 = 1×entities + 6×permissions；列表模式 9 = 1×entities + 8×permissions），
     **都没有逐实体请求**。
   - 代码侧一致：`Graph.tsx` 只在**检查器**里按选中实体取 `graphFacts/graphNeighbors`（890/904 行），
     首屏渲染用的是 `/graph/entities` 返回里自带的 facts 计数，没有对全部实体循环取详情。
   - 可能原因：t13 的 63 次是在**修复前的构建**（DKuo6EzU，02:21）上量的，而 `Graph.tsx` 在 09:34 被改过。
   - ⇒ 请 t44 在裁决行 28 前**先确认口径与基线构建**；本报告把 #graph 记为「口径待定 + 原始数字 7」。
2. **行 28 的 FAIL 面比「#graph 一条」大得多**：11.5s 口径下 9/13 路由 FAIL，且其中 8 条的主因是
   **载入突发**（同一批端点被取两遍）而不是多轮询器；全站只有 1 个 2s 级轮询器（`/api/v1/permissions`）。
   若按「≤7 只约束轮询」，则除 #task/#home 这类「同一批端点取两遍」的形态外都能过 —— 这正是 t44 要落的裁决。
3. **#task 的行 18 数字变了**：t38/t39 的「旧口径 10 / 内容区 9」在本轮是「旧口径 **9** / 内容区 **8**」，
   差的那 1 项是 t41 的 `.ant-input.mono` 27→32px。余量从 1 变 2（旧口径）/ 2（内容区）。
4. **@520 的 X4 parity**：`#runtimes`（3 项条）在 520 档 A/B 不同（192/192/192 vs 192/192/320）。
   若契约想主张「520 也 parity」，需要改写判据（见 §8.2）。

## 14. round 2（t46）待办清单

1. 等 t42（repair-round-2）/ t44（行 28 裁决）/ t45（共享层）落地后**重跑本报告的全部 3 条 verify 命令**，
   并重新记录 buildId + 工具 mtime + 源码指纹（本报告的 §0 三行必须整体替换，不能沿用）。
2. 行 27：按 §5.3 修订阈值，或先落地懒加载/路由分割再复测（`bundle-stats.config.mjs` 可直接对比）。
3. 行 28：按 t44 的裁决改判据后重跑（默认 `--api-window=11500`），并把 §6.3 的突发/稳态分解作为解释。
4. 行 20：实现失败态注入（`page.route(...).abort()`），把它从 not_measured 变成 PASS/FAIL。
5. F-7（§12.3）：补 W4 白名单，或让共享层归一 `.ant-input` 的 focus border/shadow。
6. i18n 的 28 个死键候选（§9）：确认后清理或标注保留理由。
7. 若 t45 改了共享层 token，行 29/30/32/33 与行 18 的分区数字都需要复测（本报告的这些数字只对 D6UEuuvX 成立）。

## 附录 A：本报告的复现命令

```bash
cd panel
npm run build                                   # 记 buildId：grep -o 'index-[A-Za-z0-9_-]*\.js' dist/index.html
find src index.html -type f | sort | xargs cat | sha256sum   # 源码指纹
node tools/design-audit.mjs --self-test          # 160/160
node tools/design-audit.mjs --json   > /tmp/a1.json   # 全量 ①（26 captures）
node tools/design-audit.mjs --json --out=docs/screenshots/audit-run2 > /tmp/a2.json   # 全量 ②（抖动）
node tools/design-audit.mjs --check              # 判定表，exit 1（行 27）
node tools/design-audit.mjs --check --modes=dark --api-window=11500 --no-shots --no-pixels   # 行 28 补充
node e2e/x4-parity.mjs                           # X4：8 路由 × @390 2 轨 + 六档 A/B
node e2e/i18n-check.mjs                          # i18n：585/585、0 重复、0 未解析
node e2e/api-window-probe.mjs                    # 行 28 的突发/稳态分解（--click=列表 测列表模式）
node e2e/focus-owner-probe.mjs                   # F-7：autoFocus 元素的环/边框/辉光
npx vite build --config e2e/bundle-stats.config.mjs --outDir <临时目录> --emptyOutDir   # 行 27 组成分析
npx playwright test                              # 41 项：39 passed / 2 skipped
```

