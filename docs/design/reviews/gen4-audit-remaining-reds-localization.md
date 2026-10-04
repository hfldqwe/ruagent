# 审计剩余红行定位：6 / 7 / 12（t184，report-only）

> 单号 **t184**（work，成员 `wiki`）· attempt 1 · **2026-10-04 20:09–20:2x（+08:00）**
> **本单零代码改动**：唯一写入 = 本报告。`panel/**`、`panel/tools/**`、`panel/e2e/**`、`crates/**`、`.github/**`、`scripts/**` 一个字节都没动（收尾核对见 §7）。
> 纪律：**未** push / dispatch / rerun；**未**碰 8787（活守护进程）；我的守护进程一律**临时端口 + 临时 root + `RUAGENT_PANEL_DIST` 冻结产物**，收尾按 PID 停、按具体路径删（junction 先 `rmdir`）。

## 0 结论（给「立 repair 还是立评估」的直接输入）

**今天的失败行全集 = `{6, 7, 12}`**（12 条路由 × 两档，见 §1）。三行的**物种完全不同**，而其中**两行根本不是界面缺陷**：

| 行 | 物种 | 定位 | 我的可行性判断 |
| --- | --- | --- | --- |
| **12** 带内联 style 的元素数 | **两个物种**：① 项目自己写的 `style={{…}}`（**主导**，可修）② **antd 自己写上去的**行内 style（**不可修的地板**） | `#settings` **79 / ≤50**；`div.row`×**30** + `p.muted`×**14** + 匿名 `div`×13 + `label.chat-field`×2（项目侧 ≈60）· `li.ant-menu-item`×12 等（antd 侧 ≈16–18） | **可修**，但是**跨文件的机械重构**（`Settings.tsx` 20 处 `style={{` 是大头）⇒ 建议**只立 `#settings` 的 repair**；不是产品决策 |
| **7** 每路由最大字号 | **fixture 依赖**（审计的 root 没有数据） | `stats`/`home`/`board`/`graph`：空 root 下 `20px/≥32px`、`24px/≥32px`；**换成有数据的 root 后同一份字节两档全部 PASS**（§3 实测，五条路由全覆盖） | **不是界面缺陷** ⇒ 立**评估/工具**单（seed 审计 root 或对无数据 root 判 `not_measured`），**不要**立 UI repair |
| **6** ≥18px 可见文本元素数 | 与行 7 **同源同物种**（同一批路由、同一原因；有数据 root 下两行一起转绿） | `board/graph/agents/stats`：`1 / ≥3 · 标题 1@20px`；有数据 root 上**两档全部 PASS** | 同上（**评估**） |

**一句话**：**行 12 是真缺陷（可修，需跨文件重构）**；**行 6/7 是取数管道的问题（审计 root 无数据）**，用「改界面」去追它们会改错东西 —— 而 `t162` 的教训（同一行两个物种、两种修法）在这里以「**同一批路由的两行、一个原因**」的形式再出现了一次。

## 1 我的读数：失败行全集（当前字节，不拿旧读数当靶子）

**配置**：我自己的守护进程 **8892**，`RUAGENT_PANEL_DIST = %TEMP%\t184\dist-cur`（**我自己重建**的当前产物，不是复用 `t162` 的冻结臂），临时 root；视口 1440×900（工具写死）、settle 2500ms；两档 `dark,light`。工具按路由分两半**并行**跑（各自 12 captures）：A = `home,board,memory,knowledge,graph,agents`（314.8s）、B = `chat,sessions,runtimes,stats,settings,inbox`（312.7s）；另跑一个 `--json` 臂 = `settings,home,stats,memory,knowledge`（267.7s，用于 §4/§5 的逐 capture 计数）。

**失败行全集 = `{6, 7, 12}`**，逐行逐路由的**工具原文**（`display` 列，两档）：

```
| 6 | FAIL | ≥18px 可见文本元素数 | 含读数的页 ≥3；无读数的页 ≥1；所有页 ≤40
     | board/dark=1 / ≥3 · 标题 1@20px; graph/dark=1 / ≥3 · 标题 1@20px; agents/dark=1 / ≥3 · 标题 1@20px;
       board/light=1 / ≥3 · 标题 1@20px; graph/light=1 / ≥3 · 标题 1@20px; agents/light=1 / ≥3 · 标题 1@20px;
       stats/dark=1 / ≥3 · 标题 1@20px; stats/light=1 / ≥3 · 标题 1@20px
| 7 | FAIL | 每路由最大字号 | ≥20px；含仪表盘的页 ≥32px
     | home/dark=24px / ≥32px (含仪表盘); board/dark=20px / ≥32px (含仪表盘); graph/dark=20px / ≥32px (含仪表盘);
       home/light=24px / ≥32px (含仪表盘); board/light=20px / ≥32px (含仪表盘); graph/light=20px / ≥32px (含仪表盘);
       stats/dark=20px / ≥32px (含仪表盘); stats/light=20px / ≥32px (含仪表盘)
| 12 | FAIL | 带内联 style 的元素数 | ≤50/路由 | settings/dark=79 / ≤50; settings/light=79 / ≤50
```

**其余全部 PASS**，其中包括**行 18（t162 刚修的）在两档都 PASS**（旁证那次修复在当前字节上仍然成立）、行 4/5/38、行 13/14、行 5/30。

**与手上旧读数的差异（明说，不硬拼）**：

| 来源 | 失败行 | 说明 |
| --- | --- | --- |
| `t159` 报（修后） | `7, 12, 18` | 它的臂**只跑 home/settings 之类的小路由集** ⇒ 行 6 不在它的覆盖面里 |
| `t162`（我，修 18 后） | `12`（在 `runtimes,settings` 上） | 路由集不同（不含 board/graph/agents/stats） |
| **本单（12 条路由 × 两档）** | **`6, 7, 12`** | **多出来的行 6 = 路由覆盖面的差别**（board/graph/agents/stats），不是回归；本单**没有**新增任何红 |
| CI 工件 run `37141491947`（2026-10-04，修 4/13/14/18/38 之前） | `4, 6, 7, 12, 13, 14, 18, 20, 38, 75` | 6/7/12 当时也红 ⇒ 与我的读数一致（该工件的 root 与本机一样没有数据，见 §3） |

**没跑的路由**：`#task`（`needsTaskId`，没有 id 时工具不测）⇒ 「全集」是在**13 条路由里的 12 条**上取的。

## 2 行 12：带内联 style 的元素数（**两个物种**）

**① 判据的谓词原文**（`panel/tools/design-audit.mjs`）：

```
:4378  parse: (t) => ({ max: pick(t.text, /≤\s*(\d+)\s*\/\s*路由/, 50) }),
:4379  judge: (c, l) => ({ display: `${c.metrics.inline.styleTotal} / ≤${l.max}`, pass: c.metrics.inline.styleTotal <= l.max }),
```
metric（`:1035`）：`if (visText && styleAttr && styleAttr.trim()) inline.styleTotal++;`，其中
`vis = rect.width>=1 && rect.height>=1 && visibility!="hidden" && display!="none" && opacity>0`（`:935`）、
`visText = vis && !clippedAway(cs, r)`（`:936`），`clippedAway`（`:692`）= clip-path `inset(…50%)` / `clip:rect(0…)` / **1×1 + absolute + overflow:hidden 的 `.sr-only` 盒子**。

**② 失败样本的 `sel`/`note`：工具【没有】给** —— 这个 judge 只返回 `display`，没有 `note`、没有 `List`（判据正文只解释了它已经排除了 antd 的隐藏测量行：*`#stats` 的 63 里有 22 个来自这些不可见行*）。⇒ 我**复刻谓词**自己取（**来源 = 我的探针，不是工具读数**），并**先校准**：

> **校准**：我的探针在 `settings/dark` 数出 **`inSty=79`**，与工具同 capture 的 **79 完全相同**（`settings/light` 同为 79）；`inFS=0`、`ambiguous/alphaSkipped` 见 §4。数字对齐后我才使用它的分解。

**③ 我量的逐元素读数（探针；`#settings`，两档相同）**——按「标签.首类名」聚合：

| 贡献者 | 个数 | 归属 | 备注/样式签名 |
| --- | --- | --- | --- |
| `div.row` | **30** | **项目** | `style={{…}}` 在 `.row` 上反复出现（含 `justifyContent:"space-between"` 等） |
| `p.muted` | **14** | **项目** | 主要是 `style={{ margin: 0 }}`（`Settings.tsx` 里 5 处字面量 + 循环展开） |
| `li.ant-menu-item` | **12** | **antd** | `padding-left: 24px`（侧栏缩进，antd 菜单自己写的） |
| 匿名 `div` | **13** | **项目** | `style={{ minWidth: 0 }}` 一类包装层（`Settings.tsx:190/204` 同款） |
| `label.chat-field` | 2 | 项目 | 表单字段包装 |
| `html` | 1 | 项目（`ui.tsx` 主题 effect） | `color-scheme: dark; background: rgb(8,9,11)` |
| `ul.ant-menu` | 1 | 项目（JSX 传 style） | `border-inline-end: none; padding-block: 4px` |
| `div.ant-layout` / `aside.ant-layout-sider` / `div.ant-select` / `input.ant-select-input` / `span.tag` / `span` | 各 1 | **antd 为主** | `min-height: 100vh`、`flex/max-width/min-width/width`、antd 控件内部 |

按属性聚合（同一份读数）：`padding-left=12`（antd 菜单）、`min-width=14`、`flex=3`、`width=2`、`max-width/min-height/background/color-scheme/border-inline-end/padding-block/display/align-items` 各 1。
⇒ **项目侧 ≈ 60 项，antd 侧 ≈ 16–18 项（不可修的地板）**：要从 79 降到 ≤50 只需去掉 **≥30**，全在项目侧。

**④ 成因（哪条规则/谁喂的）**：`panel/src/**` 里的 **78 处 `style={{…}}`**，分布（`grep style={{`，read-only）：

| 文件 | `style={{` 数 |
| --- | --- |
| `panel/src/views/Settings.tsx` | **20** |
| `panel/src/views/Agents.tsx` | 10 |
| `panel/src/views/Chat.tsx` | 9 |
| `panel/src/views/Runtimes.tsx` | 8 |
| `panel/src/views/TaskDetail.tsx` | 8 |
| `panel/src/views/SessionsView.tsx` | 6 |
| `App.tsx` / `views/Home.tsx` / `views/Memory.tsx` / `ui.tsx` / `views/Graph.tsx` / `views/RunTimeline.tsx` | 各 2–3 |

`Settings.tsx` 内部的高频字面量：`margin: 0`×5 · `justifyContent:"space-between"`×3 · `minWidth: 0`×3 · `flex:"1 1 220px"`×2 · `padding: 0`×2 · `margin:"0 4px 6px"`×1。（同一处 JSX 在 `.map()` 里会把计数放大 —— 20 处字面量喂出了 60 个元素。）

**⑤ 修法方向**：把项目侧的行内 style 换成 **类名 + `index.css` 规则**（`Settings.tsx` 优先：`.row` 的 `justify-content` 与 `.muted` 的 `margin: 0` 是最集中的两批，`min-width: 0` 已有 `.grow` 这类现成工具类）。**不要**试图去掉 antd 的行内 style（它们由 antd 在运行时写到 DOM 上，源码侧无法声明式移除；这也是判据「≤50/路由」必须留的**地板余量**：`#settings` 的地板 ≈16–18，其它路由在我这次读数里是 **17（home）/17（stats）/19（memory）/17（knowledge）**，都没超）。

**⑥ 可行性 = 可修（跨文件）**：不是产品决策、不需要 spec 裁决；但**必须在多个 view 文件里动**（`Settings.tsx` 20 处 + 少量共享包装），所以建议**单独立一个 repair 单、先把 `#settings` 修到 ≤50**（去掉 30 项即可，余量充足），并把它与「不许把元素移出对象集/不许隐藏」（行 12 的口径是**可见元素**）一起在验收里钉住。

## 3 行 6 + 行 7：**同一个物种 —— 审计的 root 没有数据**（不是界面缺陷）

**① 判据的谓词原文**：

```
行 7 :4316  const m = c.metrics.text.maxFontSize;
      :4317  const spec = ctx?.contract?.viewSpecs?.get(c.route) ?? null;
      :4325  const need = spec.maxFontPx;
      :4328  const drift = need !== l.base && need !== l.dash;   // l = {base:20, dash:32}
      :4331  pass: m >= need,                                    // 规格文件说了算；读不到规格就退回 ≥20
行 6 :4282  const n = c.metrics.text.ge18;
      :4284  const exempt = ctx?.contract?.exemptRoutes?.get("#" + c.route) ?? null;
      :4285  const need = exempt ? exempt.threshold : l.lo;      // l = {lo:3, exemptLo:1, hi:40}
      :4291  if (n < need) bad.push(`${n} < 门槛 ${need}…`);
      :4289  const T = judgePageTitle(ctx?.contract, c.route, title);   // 恰好 1 个标题载体且尺寸达标
```
⇒ **行 7 的「need」不是工具里的常数，而是每个视图自己的规格文件**（`viewSpecs`）；这也是它为什么可能「**规格说 32、页面只有 20**」——**要么页面没做到，要么规格记的是"有数据时"的状态**。见 ③。

**② 失败样本的 `sel`/`note`**：这两行**都不给 `sel`**（它们数的是文本节点），工具在 `display` 里给 `标题 N@XXpx`（页面标题载体的个数与尺寸）。**逐元素读数我用探针取（来源 = 探针）**，`ge18`/`maxFs` 的分解：

| route | maxFs（我的探针） | 最大的那个元素 | ge18 |
| --- | --- | --- | --- |
| settings | 20px | `.view-bar > h2` | 4（`.readout` 18px ×4） |
| home | **24px** | `.home-hero > h1` | 6（`.stat-strip .stat-num` 18px ×5 + h1） |
| stats | 20px | `.view-bar > h2` | 1 |
| knowledge | **32px** | `.panel.readout-strip .readout` | 3 |
| memory | **32px** | `.panel.readout-strip .readout`（`.readout-btn` 内） | 5 |

**③ 规格侧（read-only 读，决定「该不该改界面」）**：

| 视图规格 | 原文（含"要求"列） |
| --- | --- |
| `docs/design/views/view-home.md:55` | 「首页没有"主内容"——它是一块**仪表盘**。仪表盘的本职就是把 4 个平台级计数做大…层级来自字号跳档（**13 → 32**，跨 3 级）」；`:59` **禁止**再引入 18/20/24px 的第三、第四档字号，并说已改用 `ReadoutStrip` |
| `docs/design/views/view-board.md:117` | `\| B1 \| ≥18px 文本节点 / 最大字号 \| **5 / 32px** \| ≥3 / ≥32px \| 行 6、7 \|` |
| `docs/design/views/view-graph.md:139` | `\| G6 \| ≥18px 文本节点 / 最大字号 \| **3 / 32px** \| ≥3 / ≥32px \| 行 6、7 \|` |
| `docs/design/views/view-stats.md:122` | `\| S2 \| ≥18px 文本节点 / 最大字号 \| **5 / 32px** \| ≥3 / ≥32px \| 行 6、7 \|` |
| `docs/design/views/view-agents.md:136` | `\| A1 \| ≥18px 文本节点 / 最大字号 \| **23 / 20px** \| 3 ≤ x ≤ 40 / **≥20px** \|`（⇒ agents 行 7 本来就**只要求 ≥20**，它红的是行 6） |

**④ fixture 实测（本单最硬的一条）**：

- **第一次尝试【作废】**：我把守护进程起在 **8891**，但那个端口**已被别人的进程占着**（`%TEMP%\t169\...` 一类），我的 daemon **bind 失败**（`os error 10048`）而退出，**审计量到的是别人的东西**（行 6/7 报 `0px / 标题 0@—px` = 白屏）。⇒ **这次读数作废、不进任何结论**（这正是 captain 转来的 `t181`/丙 的同一个坑；我**没有**去停别人的进程）。
- **第二次尝试【有效】**：换到**先验证过空闲**的 **8881**，并逐条证明「服务的是我这份产物」：① daemon 自己的日志 `INFO ruagent_daemon::api: serving the web panel dist=…\t184\dist-cur`；② 首页引用的 chunk = `index-CQRkmRdw.js` = `dist-cur` 的 chunk；③ root 是**活账本的拷贝**（`~/.ruagent` 的 `config/` + `data/ruagent.db{,-wal,-shm}`，**只读使用**，未复制 lancedb/models/transcripts），用 `sqlite3 mode=ro` 读我的**副本**确认 `runs=29 · tasks=25 · agents=24`。
  **结果**：`stats,home` 两档 ⇒ **行 6 PASS、行 7 PASS**（工具不再打印任何 FAIL 项）；**同一份 dist、同一个工具、同一批路由，只把 root 从"空"换成"有数据"**，空 root 下是 `stats 1/≥3`、`stats 20px/≥32px`、`home 24px/≥32px` 全红。
  ⇒ **行 6/7 在 `#stats`、`#home` 上是取数管道造成的（fixture artifact），不是界面做不到**；用「把首页/统计页的字号做大」去追它们会**改错东西**（而且 `view-home.md:59` 明确禁止在首页加 18/20/24px 档 —— 追行 7 的直觉动作恰好被规格禁止）。
- **第三臂（`board,graph,agents`，同一个有数据 root，2026-10-04 20:22–20:26，165.4s）——已完成：行 6 PASS、行 7 PASS**（工具不再打印任何 FAIL 项；旁证：同一臂里**行 18 也 PASS**）。对照空 root 的同一批路由：`board/graph/agents` 两档都是 `1 / ≥3 · 标题 1@20px`（行 6 红）与 `20px / ≥32px (含仪表盘)`（行 7 红）。
  ⇒ **结论（实测，五条路由全部覆盖）**：`stats · home · board · graph · agents` 上，**行 6 与行 7 的红全部是"审计的 root 没有数据"造成的**；换成有数据的 root、**同一份 dist 与同一个工具**下，两行**全部转绿**。⇒ 若 root 有数据，我这次测到的失败行全集就只剩 **`{12}`**（`#settings` 那 79）。

**⑤ 修法方向与可行性 = 评估/工具，不是 repair**：
- 审计的 daemon 若跑在**空 root** 上，行 6/7 对 `home/stats/board/graph` 这类**"含读数"的页**必然红 —— 这与页面质量无关（CI 的 run `37141491947` 里 6/7 也红，很可能同因）。
- 可选处置（都需要 captain 在**评估/工具**层面裁决，不属于任何 view 的 repair）：① 给审计的 root **种一份可复现的种子数据**（我这次用的"活账本拷贝"就是可用的形状：只拷 `config/` + SQLite 三件套）；② 或让这行在**无数据 root** 下判 `not_measured` 并写明原因（与行 18 的「空集语义 ⇒ not_measured 不报 PASS」同构）；③ 或把视图规格里那两个数（`5 / 32px` 一类）**标注为"有数据时的读数"**，并让工具读「要求列」而不是「读数列」。
  ⚠️ 我**不**建议去改 `view-*.md` 的要求值来让它变绿：那是**削弱判据**。

## 4 甲：`t181` 声明「未测」的两项 —— 我这里转录（同工具的另一次运行）

**可比性声明**：我的臂与作者的臂**路由/档位集相同**（`home` + `settings` × `dark` + `light`），但**产物与仪器字节不同**（我的 JS chunk `index-CQRkmRdw`，见 §5）⇒ 下面每个数都注明**来源 = 我的 `--json` 臂**（`meta.generatedAt = 2026-10-04T12:15:19Z`，baseUrl `127.0.0.1:8892`），**不是**复述作者的数字；**相同**才是交叉核对成功，**不同**就该按「两份读数用了不同字节」处理。

| 项 | 作者（`t159`）报 | **我的读数（工具 `--json` 的 `metrics`）** | 判定 |
| --- | --- | --- | --- |
| 行 4 观测计数（strict containers） | `0` | **`home 0 / settings 0`**（两档皆 0） | **一致** ✓ |
| 行 5 观测计数 | `3` | **`home 3`**（两档）、`settings 7`（两档） | **home 一致** ✓；`settings 7` 是我补上的第二个读数 |
| 行 38 观测计数 | `0` | **`home 0 / settings 0`**（两档） | **一致** ✓ |
| `checked`（对比度检查对） | `61 / 140 / 61 / 140` | **`home/dark 61 · settings/dark 140 · home/light 61 · settings/light 140`** | **逐项一致** ✓ |
| `ambiguous` / `alphaSkipped` | `0 / 0` | **全部 capture `0 / 0`** | 一致 ✓ |

**C63 的口径**：上表里 `strict/allBordered/checked` 都是**指标转载**（我把工具的 `metrics.borders.*`、`metrics.contrast.*` 读出来），**不是**「行 4/5/38 通过」的断言 —— 这三行的 `display` 列在 **PASS 行上是空的**（工具只在 FAIL 行打印计数），所以「没提取到计数」绝不能写成「通过」。行 4/5/38 的 PASS 结论来自 §1 的判定表，不是来自上表。

## 5 C69：两个身份（这一节是为了让下一位能判断"两份读数是不是同一份字节"）

| 身份 | 值 | 备注 |
| --- | --- | --- |
| **仪器**（被测工具）worktree | `sha256 = 99f9ed14b36e2d2f3d70bd679cef59eac0f7c0856104686f8a473df1fa7185ed` | **脏**（`M panel/tools/design-audit.mjs`），**mtime = 2026-10-04 20:10:13** —— **正好是我这次 recon 的启动时刻** ⇒ `t169` 正在同一分钟写它 |
| 仪器 blob | `HEAD: = ed25988ac13d9c33298b186c8fef212ed0146be6`（最后提交 `0da0cb6`，2026-09-29） | 工作树字节 ≠ HEAD 字节（脏） |
| **被测产物**（我的臂） | JS `index-CQRkmRdw.js`（sha256 前缀 `b381a2f7e0c2f2cd`）· CSS `index-BvDMgDmx.css`（sha256 前缀 **`e82633bb3f916c28`**） | CSS 与 `t162` 落地的 CSS **同一份**（该哈希在 t162 报告里也是它）⇒ §2/§3 里**由 CSS 驱动的读数**可以归到已提交的那份 CSS 上 |
| 工具自己记的产物 | 报告头：`target: http://127.0.0.1:8892 — panel build CQRkmRdw **(STALE: panel/src is newer than panel/dist)**` | 这是工具对「产物比源码旧」的告警（源码在 `t169` 手里动着）⇒ **我的读数属于"20:10 那一刻的源码快照 + 已提交的 CSS"**，不是某个可复现的 commit 快照 |

**诚实登记（C69 的直接后果）**：我**无法**把这次的仪器钉到字节（`t169` 与我的读取同分钟，见上表）；所以我给的是**时间点 + 当时的 sha256 + 当时的产物哈希 + 工具自己记的 build id**，并**明确**：本次所有读数属于**20:10–20:22 那几分钟的工作树字节**。若 `t169` 的编辑落在我的两次运行之间，**两次读数就可能不是同一份仪器** —— 我这次没有看到工具行为差异（两半 + json + fixture 臂的行判定自洽），但这**不能**当作"仪器没变"的证明。

## 6 未覆盖什么（诚实清单）

1. **13 条路由里的 12 条**：`#task`（`needsTaskId`）没有 id ⇒ 没测。
2. **只跑了两档、一个视口**（工具写死的 1440×900）：窄屏（<992 折叠侧栏）、放大/缩放、`prefers-*` 下的行 6/7/12 **未测**。
3. **只在**"我自己的守护进程 + 临时 root"上测：**没有**在 CI 上验证（本单不许 push/dispatch）⇒ CI 的行 6/7 是否同因，只能**引用**它的工件（`37141491947` 的 6/7 也是红）作为旁证。
4. **fixture 臂覆盖 `stats,home`（臂 2）与 `board,graph,agents`（臂 3）**：即**行 6/7 全部五条失败路由都做了对照** ✓；`memory/knowledge` 在空 root 下就已经有 32px 的 `.readout`（本来就 PASS）⇒ 它们的数据依赖**没测**（也不需要）。**没有测**：把同一份"活账本拷贝"扩到**全部 13 条路由**的整轮审计（那需要把 lancedb/models 也拷进去，本单没做）。
5. **行 12 的逐元素分解来自我的探针**（工具没有样本），但我**验证过它复刻同一个谓词**（`settings` 上 79 = 工具的 79）；其它路由的探针读数（home 17 / stats 17 / memory 19 / knowledge 17）**只用于"地板余量"的说明**，不是判定依据。
6. **没有复用 `t162` 的冻结产物**：本单是**重新构建**的 `dist-cur`（`t162` 的 `dist-post2` 是另一份字节）；`t162` 的读数与本单**没有被混用**。
7. **没有测**：行 12 在**暗/亮两档是否真的同值**（我读到 79/79，只说明这两档在这一版上相同，不代表所有路由）。
8. 本轮另有**别人的变异窗口**（`t182` 改 `crates/{acp,daemon}/src/chat.rs`）与本轮无关：本单**未编译 Rust**、**未跑 rust 门**、路由集不含需要编译的东西；我 20:05 看到那两个文件是 `M`，此处按要求记时间点。

## 7 收尾与纪律

- **零代码改动**：`git status --porcelain` 里我的写入只有本报告（`docs/design/reviews/gen4-audit-remaining-reds-localization.md`）；`panel/**`、`panel/tools/**`、`crates/**`、`.github/**`、`scripts/**` 无我的改动。
- 我起的守护进程（8892 / 8891 那次是**别人的端口**，作废 / 8881）按记录的 PID 停；8891 上**不是我的**进程**没有**被碰；临时树按具体路径删（junction 先 `rmdir`）。
- **C68 两条我踩到/避开的**：① 取数管道编码 —— 我全程用 `Out-File -Encoding utf8`（PS 5.1 会带 BOM）并用 `utf-8-sig` 读回；**没有**用 `>` 重定向（那会写出 UTF-16LE）；② 「规则在不在」**不用子串** —— 本报告里所有「某条规则在/不在」的判断都按**该规则自己的选择器/表达式**给出（例如行 12 我引的是它的 `judge` 表达式，而不是搜 `inline.styleTotal` 这个字符串；`min-height:32px` 那种子串会误命中 `t162` 之前就有的 `.ant-select-sm`）。
