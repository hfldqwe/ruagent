# 审计工具的两处证据缺口 — V158-2 收口（t169）

> 来源 = `t158`（completed）的 **V158-2**：① `strictSamples` 丢掉逐边 alpha（判定却依赖 `alpha>0.04`）；② 行 4 的 `.inbox-card` 附带条款**恒真（空转）**。
> **我的写入集合**：`panel/tools/design-audit.mjs`（唯一源码改动，**75 增 / 5 删**）+ 本报告。`panel/src/**`、`panel/e2e/**`、`crates/**`、`.github/**`、`scripts/**` **零字节改动**。
> ⚠️ **我（verify）是本单修改的作者**：下面的读数是我自己量的，不是独立评审 —— §9 给出配方，仍需一位独立验证者。

## 0 结论

**两处缺口都收口了，而且用的是"只加读数、不动判定"的方式**：65 行的 `verdict` / `criterion` / `limits` / 每路由 pass 集合**逐行零变化**，自测 **504/504 exit 0**，两道门禁 exit 0。

| 缺口 | 改前（我量到的） | 改后 |
| --- | --- | --- |
| **① 逐边 alpha 不在读数里** | `borders.*Samples[]` 只有 `{sel, sides\|px}`：`strictSamples[0] = {"sel":"…div.composer","sides":4}`；整个 `metrics.json` 里 `"alpha"` 键 **0** 个 | 每个 border 样本带 **`live`（哪几条边活着）+ `alpha`（逐边颜色 alpha，含被 0.04 gate 拒掉的那些）**：`"alpha"` 键 **208** 个、`live=` **2** 处（chat 的 `.composer` ×2 档）；行 4 的 note 现在写 `[live=TRBL α=1/1/1/1]` |
| **② `.inbox-card` 脊线条款空转** | 26/26 captures 的 `extras.inboxCards=0` ⇒ `ridgeOk` 由 **`inboxCards===0` 短路**为真，而 display **把该条款整个省略**（`0 / ≤1`），读者分不清「没卡片可测」与「卡片都不带边框」 | 三种状态都被**点名**：无卡片 ⇒ `0 / ≤1 · .inbox-card 未测（该路由无 .inbox-card，空态）` + note 写明「ridgeOk 由 inboxCards===0 短路为真」；有卡片 ⇒ `… · .inbox-card 带边框 0/2`（或红）。**条款可达且载重**：见 §3 的两条工具读数（真卡片 2 张 → PASS；同一批卡片加 1px 边框 → **FAIL**） |

**⚠️ 一处对交接的更正（比修复本身更重要）**：缺口 ② 的**成因不是**「工具只数『自己带直接文本』的 `.inbox-card`」。逐行读源码：`visText` 的**唯一定义**在 `design-audit.mjs:936` = `const visText = vis && !clippedAway(cs, r);`（`vis` = `:935` 的 rect ≥1px + 可见 + opacity>0）—— **它跟文本无关**；计数点 `:1159` 是 `if (visText && cls.includes("inbox-card"))`，**文本在子元素里的真实卡片照样会被数**。真实的空转原因是**被审计的树上根本没有 `.inbox-card`**：我在自有临时 root 上的页面读数 = `#inbox {"inboxCards":0,"empty":1,"viewBar":1}`（渲染的是空态）。⇒ 修法因此不是"去掉文本前提"（它不存在），而是**让"未测"可见** + 把这条计数谓词的**可见性前提**钉进自测（防止将来有人真的加回一个文本前提）。

## 1 仪器与字节

| 项 | 值 |
| --- | --- |
| 工具（改前） | `panel/tools/design-audit.mjs` sha256 **`E3B7E45B59FA1C49…`**，544,462 B，mtime 2026-09-29T03:51:49 |
| 工具（改后） | sha256 **`99F9ED14B36E2D2F3D70BD67…`**，549,287 B，mtime 2026-10-04T20:10:13；`git diff --numstat` = **75 / 5**，5 条删除**逐条**是"原位替换"（3 条 `pushSample` + 行 4 的 display/note），**`ridgeOk` 那行连删带改都没有**（见 §4） |
| 我自己的 daemon | `cargo-team.ps1 rustc -p ruagent --bin ruagent -- -o %TEMP%\t169\bin\ruagent.exe` ⇒ exit 0 / 53.8 s，328,082,432 B，sha256 `65718E72A4358A7B`；**WMI + `cmd /c` + ShowWindow=0** 启动（仓库 `scripts/ruagent-daemon.ps1` 记录的形状：`Start-Process -RedirectStandardOutput` 会**挂住调用方**，我实测复现了这一点）；端口 **8871**、临时 root |
| 冻结 dist（**两臂共用同一份**） | `npm --prefix panel run build`（exit 0）后冻结：`index-CQhHFKM-.css` sha256 `46DDF76A81E3C152`，总 2,024,125 B；两臂都从这份 dist 加载页面（`--base-url=http://127.0.0.1:8871`，`E2E_BASE_URL` **显式**给同一个值，B29） |
| 两臂 | **before** = 改前工具（`E3B7E45B59FA1C49…`），`--check --tabs=40 --no-shots --no-pixels --allow-not-measured`，13 路由 × dark/light = **26 captures**，617 s，exit 1；**after** = 改后工具（`99F9ED14B36E2D2F…`），同一 dist/daemon/参数，629 s，exit 1 |
| 缺口 ② 的注入仪器 | 自有 stub（`%TEMP%\t169-stub.mjs`）：静态服务**同一份冻结 dist** + 合成 `GET /api/v1/permissions`（2 条 pending，字段按 `panel/src/api.ts:154-160` 的 `PendingPermission`）⇒ 让**工具自己的浏览器**渲染真实 `.inbox-card`；`bordered` 模式只在 CSS 资产后追加一条 `.inbox-card{border:1px solid red}` |

## 2 缺口 ①：逐边 alpha（判定依赖它，读数却没有）

**先量证（改前臂，我自己的读数）**：
- 源码坐标（符号优先）：`:964` `bws` 四条边宽 → `:968-972` `sideLive[i]`，其中 `:971` = `return !!c && c.a > 0.04; // MASTER row 4: border colour alpha > 0.04`（**判定用的阈值就是这里**）→ `:973` `nSides` → `:974` `isHairline` → `:975/:976/:983` 三个 `pushSample`，载荷分别只有 `{px}`、`{}`、`{sides: nSides}` ⇒ **`c.a` 算完即丢**。
- 产物：`metrics.json`（2,738,831 B）里 `strictSamples[0] = {"sel":"div.chat-wrap.is-empty… > div.composer","sides":4}`；`"alpha"` 键 **0** 个。⇒ 行 4/5/38 判定走的 `nSides/isHairline` **全靠这个 alpha gate**，而任何复核者都**无法从工件重建这一半**（t158 只能自己复刻谓词，我这次也复刻过——这正是缺口的代价）。

**修法（只加读数）**：新增 `borderEvidence = { live: "TRBL"（活边字母）, alpha: [a_T,a_R,a_B,a_L]（该边无宽度/无活样式 ⇒ null；有宽度就记原始 alpha，**包括被 0.04 拒掉的 0.02 这种值**）}`，挂到**三类** border 样本（`hairlineSamples`/`fourSideSamples`/`strictSamples`）上；行 4 的 note 同步把 strict 样本写成 `sel [live=… α=…]`。**判定表达式、阈值、计数一行未动。**

**改后读数（同一 dist）**：`metrics.json`（2,776,836 B）里 `"alpha"` 键 **208**、`live=` **2**（chat/dark + chat/light 的 `.composer`，即唯一两个 `strictContainers=1` 的 capture）。行 4 note 实例：`main.ant-layout-content.content > div > div:nth-of-type(2) > div.ant-card.ant-card-bordered:nth-of-type(1) [live=TRBL α=1/1/1/1] | …:nth-of-type(2) [live=TRBL α=1/1/1/1] · …`。
⇒ 复核者现在能直接从工件回答"这三条命中的是 DIV、四条边全活、每条 alpha=1、1px solid"——**t158 当时只能靠自写探针得到的那一项，现在在工具输出里**（§8 说明该值仍是"工具读数"，不是像素读数）。

## 3 缺口 ②：`.inbox-card` 条款的可达性与载重（含对交接成因的更正）

**改前读数（工具，26/26 captures）**：`extras.inboxCards = 0`、`bordered = 0`；行 4 display 恒为 `0 / ≤1` 或 `1 / ≤1`，**从不出现** `· .inbox-card 带边框 x/y` ⇒ 条款"绿"不可见地空转。
**页面侧读数（我的探针）**：`#inbox` 在我自有 root 上 `{"inboxCards":0,"empty":1,"viewBar":1,"bodyText":167}` ⇒ **空态**（`Agents.tsx:946-947` `visible.length===0 ⇒ <Empty/>`，卡片在 `:956`）。**加上源码 `:936` 的 `visText` 定义** ⇒ 空转成因 = **没有卡片**，**不是**文本前提（更正见 §0）。

**改法（选 ① 的实质 + ② 的理由都写进输出，但只改证据不改 pass）**：
```js
const ridgeOk = ex.inboxCards === 0 || ex.inboxCardBordered === 0;   // ← 一字未动
const ridgeShown = ex.inboxCards
  ? ` · .inbox-card 带边框 ${ex.inboxCardBordered}/${ex.inboxCards}`
  : " · .inbox-card 未测（该路由无 .inbox-card，空态）";
```
note 里在 `inboxCards===0` 时补一句「本次未测…⇒ ridgeOk 由 inboxCards===0 短路为真」。⇒ **"没东西可测"从此不可能被读成"测过且干净"**；同时 `pass` 表达式与阈值未动。

**可达性 + 载重（工具自己的判定原文，同一份冻结 dist，只换数据/一条 CSS）**：

| stub 臂 | `extras` | 行 4 判定 | display（工具原文） |
| --- | --- | --- | --- |
| `plain`（2 条真卡片，无边框） | `inboxCards=2, bordered=0` | **pass** | `0 / ≤1 · .inbox-card 带边框 0/2` |
| `bordered`（同 2 条卡片 + `.inbox-card{border:1px solid red}`） | `inboxCards=2, bordered=2` | **fail** | `2 / ≤1 · .inbox-card 带边框 2/2` |

`bordered` 臂的 note：`… [live=TRBL α=1/1/1/1] | … [live=TRBL α=1/1/1/1] · .inbox-card 出现 border（应为 box-shadow 脊线）`。
⇒ **条款可达**（不是恒真：它真的会红）**且**现在会**自己说明它是"未测"还是"测过"**。附：`plain` 臂的卡片计算样式 `borderTopWidth=0px / borderTopStyle=none`（我的探针），`bordered` 臂 `1px / solid` ⇒ 红绿由真实样式驱动。

## 4 两臂逐行对照：既有判定必须不变（同一 dist，工具改动前后）

**结论：65 行（全部行）的 `verdict`／`criterion` 文本／`limits` 文本／每路由 pass 集合 —— 变化数都是 0。**

```
rows=65  verdict changes=0  criterion changes=0  limits changes=0  per-route pass-set changes=0
before FAILING ROWS: 6, 7, 12, 18      after FAILING ROWS: 6, 7, 12, 18
```
**证据文本（display/note）只差 5 行，逐行归因**：
- **row 4**：display+note 变了 —— **本单有意的那一条**（`未测` 文案 + `[live=… α=…]`）。
- **row 26**（`pass→pass`，limits `{"max":200}`）：display `76ms` → `114ms` —— **墙钟时长**读数 ⇒ 两次独立浏览器跑的**噪声**。
- **row 28**（`not_measured→not_measured`）：`home/dark=:9 / ≤7` → `:5 / ≤7`、`chat/dark=:6` → `:5` —— **2.5s 窗口内的请求计数** ⇒ 请求时序噪声（两臂都 `not_measured`，无判定参与）。
- **row 39**（`pass→pass`）：仅 note 变（端点计数/时刻），display 逐字同 ⇒ 运行期观测噪声。
- **row 27**（`pass→pass`）：display 只差**入口 chunk 的文件名**：`index-BZR-Wwdb.js` → `index-CQRkmRdw.js`；**尺寸逐字同**（`应用层首屏 127KB (gzip 43KB) / ≤150KB`、首屏静态 1147KB、全量 JS 1488KB）⇒ **成因是别人的面板重建，不是我的改动**：工具行 27 读的是**仓库磁盘上的 `panel/dist`**（`design-audit.mjs:3722` `const distDir = join(PANEL, "dist")`），而我两臂之间 `panel/dist/index.html` 被重建了两次（mtime 20:20:11 时已是第三个名字 `index-FCbDMJr7.js`）；我自己服务的那份**冻结 dist**（entry `index-CkJ7AA4O.js`）两臂字节未变。⇒ 这条我**点名但不改**（见 §8）。

⇒ 除 row 4 外，没有任何一行因我的改动而改变判定或路由级 pass/fail 集合；row 26/27/28/39 的差异都在"**测量类**读数"上，且其中 row 27 有可指认的外部成因。

## 5 自测：改后必须绿 + 必须证明它真的会红

**（a）改后在最终字节上跑（契约命令，按 CI 的 cwd）：**
`cd panel && node tools/design-audit.mjs --self-test` ⇒ **`self-test: 504/504 pass`，exit 0**（新增 9 条 `t169 row4:` 断言全绿，其中两条是"能红"的：`a bordered .inbox-card -> FAIL`、`zero cards NAMES the un-exercised clause`；另有两条源码断言把"计数谓词是可见性、不是文本"和"`ridgeOk` 表达式未变"钉住）。

**（b）负控（共享树零变异，做法比"临时改坏仓库里的工具"更严）**：
- 把 `panel/tools/**` 复制到仓外**同深度**的镜像 `%TEMP%\t169root\panel\tools\`，并用 **junction** 补上它需要的三样：`panel/node_modules`（`@playwright/test` 解析）、`docs`（自测要读 MASTER/视图规格）、`panel/src`。
- **未改坏前**：该副本与仓库工具 **sha256 逐字相同**（`99F9ED14B36E2D2F3D70BD67…`）⇒ `504/504 pass, exit 0`。
- **改坏一条期望**（`row16: outline-style:auto + shadow ring -> FAIL` 的 `want` 由 `1` 改 `0`；替换点全文件仅 1 处）⇒ 副本 sha `54EA58EF7C5250CB35DC354F…` ⇒ **`self-test: 503/504 pass` + `self-test FAILED: row16: outline-style:auto + shadow ring -> FAIL`，exit 1** ⇒ **自测真的会红**。
- **按字节恢复**（从仓库工具复制回）⇒ sha 回到 `99F9ED14B36E2D2F3D70BD67…`（== 仓库工具）、`504/504 pass, exit 0`。
- **共享树从未被这次负控写过**（全程只动 `%TEMP%` 副本）⇒ 不存在"别人的 `--check` 落到被改坏的字节上"的窗口。
- **方法注记（也是发现）**：工具的自测**与仓库耦合**（要读 `docs/**` 的契约文本），所以它不能整树搬走就跑：我第一次把副本放在 `%TEMP%\t169\selfcopy\`（深度差一级）⇒ 自测 **478/504**，26 条"从 MASTER/视图规格读契约"的断言全红；**按同样的 `HERE/PANEL/REPO` 深度**（`design-audit.mjs:53-55`）放置后才 504/504。

## 6 门禁（全部在最终字节上）

| 门 | 结果 |
| --- | --- |
| `node panel/tools/design-audit.mjs --self-test`（在 `panel/` 下，CI 形状） | **exit 0**，`504/504 pass` |
| `npm --prefix panel run build` | **exit 0**（`✓ built in 4.71s` + `build-panel: dist updated (55 assets, index.html swapped by rename)`；wrapper 含 i18n-check + `tsc -b` + e2e tsconfig + vite） |
| `bash .github/workflows/scripts/check-workflow-refs.sh` | **exit 0**（`every executed path a workflow references is tracked`；`ci.yml:487` 引用的 `panel/tools/design-audit.mjs` 仍被跟踪） |

**未新增/修改 e2e** ⇒ 按契约不触发 `npm run test:e2e`，也**没有**用裸 `npx playwright test`。

## 7 判据未削弱（逐字）

- 65 行的 `criterion` 文本与 `limits` 文本：**变化数 0**（§4 的字节级比较，不是"看起来一样"）。
- 我在工具里**没有**改任何阈值/口径常量：`c.a > 0.04`（`:971`）一字未动；行 4 的 `pass` 表达式 `b.strictContainers <= l.max && isComposerOnly(b) && ridgeOk` 与 `ridgeOk` 定义**连"删+改"都没有出现在 diff 的删除行里**（§1 的 5 条删除逐条可查）。
- 新增的 9 条断言是**自测**（`runSelfTest`），不是判据行；`--json` 新增的 `live`/`alpha` 是**载荷字段**，不影响任何 `parse`/`judge` 的输入语义（它们只被行 4 的 note 与工件消费）。

## 8 未覆盖什么 / 我点名但不改的其余缺口

1. **只动 `panel/tools/**`**（+ 本报告）：`panel/src/**`、`panel/e2e/**` 一行未改 ⇒ **视觉缺陷（如 V158-4 脊线未落地、行 4/38 的像素结果）本单不碰**，另单负责；我只改工具的**证据面**。
2. **其余工具缺口（点名，不顺手改）**：
   - **行 27（及任何读 dist 的行）读的是仓库磁盘 `panel/dist`（`:3722`），不是 `--base-url` 指向的那份 dist**：这就是 §4 里 row 27 在我两臂之间漂移的成因；当别人正在 `npm run build` 时，这些行的读数会在审计者脚下移动（工具只在 dist 缺失/过期时 WARNING，`':8265-8266'`，我这次带 `--allow-not-measured` 所以它没拦）。**建议**：把"被审计的 dist"与"磁盘 dist"在 `meta` 里显式并列（或让 `--base-url` 成为唯一事实源）。
   - `alpha` 只记到**三位小数**（`Math.round(p.a*1000)/1000`）：足够看出 0.04 的 gate 走向（0.02 vs 0.04），但**不**是浮点原值；要精确值就得改判定口径，本单不碰。
   - 工具 JSON 里 `"a":` 的**计数口径要小心**：我 arm-before 的 `metrics.json` 里 `"a":` 出现 **56** 次（颜色的 rgba 对象里就有 `a` 字段），所以 t158 那句「`--json` 里 `"a":` 出现 0 次」**只有在"border 样本块"这个口径下**才成立 —— 我的量证与修法都按后者（`borders.*Samples[]` 原本没有 alpha；改后 `"alpha"` 键 208 个）。
3. **像素/时序行不在两臂对照的覆盖里**：两臂都用 `--no-shots --no-pixels`，所以行 1/2/3/5 一类**像素行**、以及行 26/28 这类**时序行**的逐字不变**没有**被证明（它们本来就被列为噪声；行 26/28/39 的差异已逐条归因）。
4. **缺口 ② 的"真数据"读数来自我自己的 stub**（合成 `pending` 队列）：**不是**真守护进程里真跑出来的 pending permission（要在真 root 上制造"等人类裁决"的 run，代价与本单无关）。因此"条款可达"的证明是 **工具判定 + 合成数据**；真库上没有重复验证（CI 的 root 今天也是空态 ⇒ 与我的临时 root 同类）。
5. **单 dist、单 root、单机器**：两臂同 dist、同 root、同机器、同 Playwright 版本；跨机器/跨浏览器版本的可重复性未测。
6. **V158-4（`.inbox-card` 脊线没画出来）与本单无关**（已另单），我**没有**改 `panel/src/index.css`，也没有把脊线加回去（`verify2` 的处方：动色值可以、动"画边机制"不行）。
7. **`--self-test` 之外的 CI 步骤**（真实 `--check` 全矩阵的 CI 运行）未重跑：`.github/**` 出 inScope；我本地的全矩阵是**空 root + 我自己的 dist**，与 CI 的数据面不同。

## 9 过程、收尾、残留 + 复现配方

- **两臂怎么造**：构建私建 daemon + `npm --prefix panel run build` 后**冻结** `panel/dist`（两臂共用）→ WMI 启动临时 daemon（端口 8871、临时 root）→ **改前工具**跑全矩阵（13 路由 × 2 档 = 26 captures）→ 改工具 → **改后工具**跑同一矩阵 → 逐行 diff（先按行表，再从两份 `metrics.json` 逐字段核对 `verdict/criterion/limits/evidence.pass`）。
- **进程与端口**：我的 daemon 是 WMI 起的 `cmd /c <bat>` ⇒ 记到的 pid（30756）是**包装进程**，真正的 `ruagent.exe` 是**另一个 pid（30836）**并在我杀父之后**继续服务 8871**（这正是仓库 `ruagent-daemon.ps1` 注释里"别再按名字/端口批量杀"的现场版）⇒ 我按**它自己的命令行**（我的临时路径/端口/root）认定并只杀它，之后 8871 变 closed。两个 stub（pid 35984/40552…，其中 38220 是 8893 的 bordered stub）同样按自己命令行认定后停掉；`8892` 上是**别人的 ruagent（pid 41480）**，我**没碰**，并改为"先查端口空闲再选"。
- **浏览器/孤儿**：所有 Playwright 实例由脚本自行 `close()`；收尾按**我自己的临时路径** `*t169*` 扫过 `node/chrome/headless_shell/cmd`，命中 3 个 `node.exe`（2 个是我 stub 的孤儿、1 个已自行退出）并**只按 pid** 停掉。
- **临时文件**：`%TEMP%\t169*` 从 **2,427,972,351 B** 清到 **≈6.6 MB**（删私建 exe 328 MB + root + 冻结 dist + 副本；junction 一律用 `cmd rmdir` 摘除，仓内 `panel/node_modules/playwright` 事后仍在），保留 4 份 `metrics.json`（`audit-before` 2,836,819 B / `audit-after` 2,874,957 B / `audit-stub-plain` 456,388 B / `audit-stub-bordered` 459,198 B）+ 日志与两臂逐行表（`t169-rows-{before,after}.txt`）。C: 可用 ≈36 GB。仓内：`git status --porcelain -- panel/tools/` = **仅 ` M panel/tools/design-audit.mjs`**（另两份 `?? docs/design/reviews/gen4-*.md` 不是我的）。
- **8787 的常驻守护进程未触碰**（pid 14944，StartTime 2026-10-02 23:05:45）。

**复现配方（独立验证者用）**：
```powershell
cd panel
node tools/design-audit.mjs --self-test                       # 必须 504/504 exit 0
# 两臂（同一份冻结 dist、同一个临时 daemon；E2E_BASE_URL 显式给，B29）
node tools/design-audit.mjs --check --tabs=40 --no-shots --no-pixels --allow-not-measured `
  --base-url=http://127.0.0.1:<your-port> --out=<仓外绝对目录>
# 读 checks[].{verdict,criterion,limits} 与各 evidence.pass：与改前工具（git stash/旧副本）逐行比
# 缺口①：grep metrics.json 的 "alpha"（改前 0 个 / 改后 208 个）与 borders.strictSamples[].{live,alpha}
# 缺口②：看行 4 display 是否点名未测；再用合成 pending 队列（或任何能渲染 .inbox-card 的 root）证明条款可达/可红
```
