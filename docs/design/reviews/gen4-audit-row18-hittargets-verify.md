# t183 独立验证 t162：审计行 18「两个物种两种修法」（mem-core，report-only）

- 被验证对象：`t162`（commit **`bb2d99ad1951b856d7031cb1fa11ddda7535aa33`**，`fix(panel): row 18 is two species with two fixes, and the exception list stays closed`）
- 本单 attempt `67b06fb9-579a-49a0-8a54-a2d6c4ba1879`；**我的唯一写入是本报告**
- **判定：pass** —— 两臂我自己重跑、两组几何我自己量、同路由**逐行**对照、无例外名单、C59 全部成立；带 5 条明确边界与 2 条**我自己**的仪器错误（§7）

## 0 我量到的（一眼版）

| 项 | 修前臂（`bb2d99ad^` 的 `index.css` = `d51a4134…`） | 修后臂（`bb2d99ad`/HEAD 的 `index.css` = `ed07224a…`） |
| --- | --- | --- |
| **row 18 verdict** | **fail**（4 个 capture 全部 `pass=false`） | **pass**（`failures` 空） |
| `#runtimes` 第 3 行 `button.tag`（3 个/档） | **101.81×20**（`padding-block: 2px/2px`） | **101.81×24**（`padding-block: 4px/4px`） |
| `#settings` `button.ant-switch`（13 个） | **48×24**（`min-height: auto`） | **48×32**（`min-height: 32px`，`padding-block` 仍 `0px/0px`） |
| 内容区 `<24px`（runtimes / settings） | **3** / 0 | **0** / 0 |
| 内容区 `<32px`（runtimes / settings） | 0 / **13**（>10 预算） | 3（≤10）/ **0** |
| 65 个 check 里**动了的** | — | **只有 row 18**（fail→pass） |
| 判据正文 / `limits` | — | **两臂逐字相同**（`{"hard":24,"soft":32,"max":10}`） |

三行一句：**同一行由两个不同成因驱动、两种修法**（一个是「作为按钮的标签」欠 24px 地板，一个是 antd 控件根进 `<32px` 预算）——**这个判类我独立复现了**，且两处都是**改**而不是**豁免**。

## 1 先确认「被测字节 = 提交字节」（C52 + C61：按提交号，不用 HEAD）

```
bb2d99ad1951b856d7031cb1fa11ddda7535aa33  fix(panel): row 18 is two species with two fixes…
  141  0  docs/design/reviews/gen4-audit-row18-hittargets-repair.md
   30  0  panel/src/index.css            <- 只改了这两个文件（与作者申报一致）
bb2d99ad:panel/src/index.css   blob b9fc5d0d59fcbc7bd535d5d1f0d6516e89a12718  sha256 ed07224ae24ff68c…
bb2d99ad^:panel/src/index.css  blob d904a55ac519228de65ef6eb28ceac0c17ceaf8d  sha256 d51a4134a2461210…   <- 作者申报的修前哈希
HEAD(现为 aa733481…):panel/src/index.css  blob 与 bb2d99ad 的相同 ⇒ 最终字节 == t162 的修后字节
```

**C61 现场**：我**开工时** HEAD = `bb2d99ad`，核对到一半 HEAD 前移成 `aa733481`（另一条 docs 提交）⇒ 若我用 `HEAD^` 当「修前」就会**取到修后的字节**。因此本报告所有锚都**按提交号**写，并且我的修前臂源码哈希实测 = **`d51a4134a2461210…`** ✓（= 契约里那个 `D51A4134…`）。

## 2 我自己的两臂（冻结产物 + 我自己的端口/root + 显式 base URL，B29）

- **修后臂**：仓库门禁 `npm --prefix panel run build`（**exit 0**）产出的 `panel/dist` → **冻结拷贝** `dist-post`（入口 `index-DTndI_TN.js`，CSS `index-BvDMgDmx.css`）。
- **修前臂**：把 HEAD 的 `panel/**` 复制到 `%TEMP%`（排除 `node_modules`/`dist`），**只把 `src/index.css` 换成 `bb2d99ad^` 的字节**，用 junction 借仓库的 `node_modules` 跑 `npx vite build` → `dist-pre`（入口 `index-BZQlUSI0.js`，CSS `index-CQhHFKM-.css`）。构造的等价性**是量出来的**：`diff -rq` 报**唯一**差异就是 `src/index.css` ✓（t162 那一提交除 CSS 外没动 `panel/**`）。收尾用 `rmdir` 删 junction（C39）。
- **两臂实际服务的规则**（精确 minified 选择器，因为**我第一版 grep 出过假阳性**，见 §7）：
  `dist-pre`: `button.tag{padding-block:4px}`=**0** · `.ant-switch{min-height:32px}`=**0** · handle 重居中=**0**
  `dist-post`: **1 / 1 / 1** ✓
- 两个守护进程：`RUAGENT_PANEL_DIST=` 各自的冻结目录 + 各自临时 root + **127.0.0.1:8899 / :8900**，审计一律 `--base-url` 显式；**8787 一次未碰**。audit 命令 = `node tools/design-audit.mjs --base-url=… --routes=runtimes,settings --modes=dark,light --json --no-shots --no-pixels --out=<%TEMP% 绝对路径>`；两臂 `PRE_AUDIT_EXIT=0` / `POST_AUDIT_EXIT=0`（各自 ~110 s，4 captures/臂），收尾按**我记录的 pid** 停（全部 stopped=True）。

## 3 row 18 FAIL→PASS（工具自己的原文，两臂）

**修前**（`verdict=fail`，4 条 failures）：
```
runtimes/dark = 命中区 地板<24px 3 · 内容区<32px 0/≤10 ✓ … ｜ 对照（绘制口径）<24px 3 · <32px 0
settings/dark = 命中区 地板<24px 0 · 内容区<32px 13/≤10 ✗ … ｜ 对照（绘制口径）<24px 0 · <32px 13
（light 两档同形）
```
**修后**（`verdict=pass`，`failures` 空）：
```
runtimes/dark = 命中区 地板<24px 0 · 内容区<32px 3/≤10 ✓ …
settings/dark = 命中区 地板<24px 0 · 内容区<32px 0/≤10 ✓ …
（light 两档同形）
```
⇒ **两档都 FAIL→PASS** ✓，且**两侧（floor 与 budget）各自独立**：修前 runtimes 是**地板**违规而 settings 是**预算**违规 —— 这正是「同一行两个成因」的机器读数。

## 4 两组几何（**我自己用 `getBoundingClientRect` + `getComputedStyle` 量的**，不经工具）

我另写了一个 Playwright 探针（`%TEMP%` 内，chromium，1440×900，`#runtimes` / `#settings` dark）：

| 探针 | 修前臂 | 修后臂 |
| --- | --- | --- |
| `button.tag`（`#runtimes`，3 个） | `distinct_h=[20]`，`distinct_w=[101.81]`，`padding-block 2px/2px`，`min-height auto` | `distinct_h=[24]`，`distinct_w=[101.81]`，`padding-block 4px/4px` |
| `button.ant-switch`（`#settings`，**13 个**） | `distinct_h=[24]`，`distinct_w=[48]`，`min-height auto`，`padding-block 0px/0px` | `distinct_h=[32]`，`distinct_w=[48]`，**`min-height: 32px`**，`padding-block 0px/0px` |

⇒ 作者的 **102×20→102×24**（工具四舍五入；原始 101.81）与 **48×24→48×32** 与**13 个**全部复现 ✓；而且**两种修法在计算样式上可见地不同**：tag 靠 **padding-block 2→4**（16+2×4=24），switch 靠 **min-height auto→32px**（它的 padding 一点没动）⇒ 「两个物种」不是叙述，是读数。工具侧的样本也对上：`small32Samples` 里 tag 的选择器是 `div.ant-card… > div.ant-card-body > div.row:nth-of-type(3) > button.tag`（**每张卡第 3 行**，3 个/档）且 `via=自身`；switch 项同为 `via=自身`（取的是控件自己的盒子，不是上溯）。

## 5 无例外名单（两臂逐字相同 + 提交面上不可能加）

- row 18 的 `criterion` / `target` / `targetSource` / `limits` / `scope` / `modes` 在两臂的哈希**逐一相同**（例：`criterion 9f3cd204eff475c3`、`target 05f4b0a752d3e1b7`、`limits 3887c2fb462389e6`）；`limits = {"hard":24,"soft":32,"max":10}` ✓。
- **判据的来源不是工具**：`targetSource: "MASTER.md"`，工具是**解析** `docs/design/MASTER.md §12` 的表格（`row.parse(t) ⇒ limits`）。我按来源核了一遍：第 823 行（§12 行 18）**正文本身**就写着「（**无例外名单**：已裁决 `.ant-switch` 的 21px 走「改」而不是豁免，见 §12.7.2）」，并把 `.ant-switch` 的状态**登记为「改」**（pin `Switch` 的 trackHeight/handleSize/trackMinWidth = 24/20/48；t27 复测已落地 48×24）✓。`docs/design/MASTER.md` 当时**未被他人在途修改**（`git status docs/` 只有我这份新报告）⇒ **两臂读的是同一份判据文本**。
- 结构上**不可能**加例外：例外只能活在判据源或工具里 —— 那次提交只碰了 `panel/src/index.css`（30/0）与报告（141/0），判据源 `MASTER.md` 不在其中；row 18 对象上也没有任何 exemption/allow/waive 形状的字段 ✓。
- 实际落地的两条规则是 `.ant-switch{min-height:32px}`（+ 手柄 `inset-block-start:50%; transform:translateY(-50%)` 重居中）与 `button.tag{padding-block:4px}` ⇒ 前者把 `.ant-switch` 从 `<32px` 桶里**抬出去**、后者把它抬到地板之上，**都是改** ✓。

## 6 没有带动别的行 / 别的路由（C56 第③条）与 C59

- **逐行对照**：同一份 `checks`（65 条）在两臂逐条比 verdict ⇒ **动了的只有 row 18（fail→pass）**，其余 64 条**完全相同**（含 row 12 的 fail、row 1/2/3/25/76/77 的 `not_measured`）。
- **一条必须说清的边界**：`global.bundle.entry.file` 在**两臂**都是 `index-DTndI_TN.js` —— 即**读磁盘上 `panel/dist` 的 bundle 类行**（我这次两臂服务的是冻结拷贝，但磁盘 `panel/dist` 是门禁刚构建的修后产物）⇒ 这类行的「相同」是**我装置的性质**，不是对 CSS 改动的测量。row 18 是**浏览器里量的**，与所服务的字节绑定 ✓，所以第③条的结论对 row 18 及所有浏览器量测行成立；我从工具自身拿到这条线索并把它写出来，而不是把它当成「别的行都没动」的证据。
- **C59**：`.step-num` 的机制 `border: none` + `box-shadow: inset 0 0 0 1px …`（t158 钉的那条）：`.step-num` 规则块**50 行**在两臂哈希相同（`60100e972a470b23`）；含 `border: none` / `inset 0 0 0 1px` 的行集合各自同哈希；t158 相关行（369、2189 上下文）两臂逐字相同 ✓。机制上更直接：那次提交的改动 hunk 只有两处（`@@ -661,2 +661,13 @@`、`@@ -1925,2 +1936,21 @@`），`.step-num` 在 819–856 ⇒ **在 hunk 之外，按构造逐字未动** ✓（全文件 2420 行两臂同、diff **0 处删除**）。

## 7 我自己没有验证的 / 我的仪器错误（第 19 条 + C63）

**没有验证的**：
1. **像素/感知层**：我用的是 `--no-shots --no-pixels`（与作者同）⇒ 本报告结论只对**计算样式与边界框**成立；row 1/2/3 在三臂都是 `not_measured`（工具自己标的）。
2. **只有 2/13 条路由**（`runtimes, settings`）⇒ 其它路由**未测**；工具自己也 warn：「子集运行：`--routes` 只覆盖 2/13 条路由…本次写出的 metrics.json / report.md 不是全站结论」。
3. **只有一个平台/浏览器**（Windows + chromium，1440×900）；没有别的平台、别的浏览器、别的视口。
4. **两臂的 JS 不是逐字节同一份**：修后臂走仓库 wrapper（i18n-check + tsc + vite），修前臂在我复制出来的树上走 `npx vite build`（且 `__BUILD_ID__` 是构建时间）⇒ 入口 chunk 名不同（`index-DTndI_TN.js` vs `index-BZQlUSI0.js`）。**我的结论只关于 CSS 规则与盒子几何**，不是「两臂 dist 逐字节相同」。
5. 没有 push / dispatch / rerun / 没有跑 rust 门禁（本单不需要）；未碰 8787。

**我自己的两处仪器错误（都记下来，因为它们都能伪造出「绿」）**：
1. **PowerShell 5.1 的 `>` 把审计 stdout 写成了 UTF-16LE**，`JSON.parse` 直接炸（`Unexpected token '�'`）。处置：用 node 以 `utf16le` 解码后转 UTF-8，**原始字节另存为 `*.as-captured`**，转换后 `captures=4` 解析通过；本报告读数均来自转换后的文件。⇒ 与「读数不是它看起来的那个」同族：**编码也是取数的一部分**。
2. **我第一次判「哪臂含哪条规则」的 grep 是假阳性**：`min-height:32px` 在修前 CSS 里命中的是既有的 `.ant-select-sm{min-height:32px}` ⇒ 我一度以为「修前臂带着修后的 switch 规则」。改成精确 minified 选择器后：修前 **0/0/0**、修后 **1/1/1** ✓。
3. **我的仪器在读数之后被换过（t20 形状，必须点名）**：两臂审计跑在 **20:04:15–20:08:35**，用的 `panel/tools/design-audit.mjs` 当时工作树 = 已提交 = sha256 `e3b7e45b59fa1c49`（当时 `git status panel/tools` 为空）；**20:10:13** 有同伴的在途编辑落到该文件（现在工作树 `99f9ed14b36e…`）。我读了他的 7 个 hunk：**没有一处**触及 row 18 的判定、`limits`、或任何豁免机制（改动在 `const PROBE` / `const CHECKS`（新增一条 check）/ `runSelfTest`）⇒ 我的读数**不因它失效**，但**今天重跑会用到不同的仪器字节**。
4. **HEAD 在本单期间前移了至少两次**（`bb2d99ad` → `aa733481` → 现在 `b61fa099…`；工具 blob 也从 `e3b7e45b…` 变成 `ed25988a…`）⇒ 这正是 **C61** 的现场：所有锚我一律按**提交号**写；`HEAD^` 在中途已经不再是「t162 的父提交」。

## 8 残留与清理

- 仓库写入：**只有本报告**（`git status` 里的 `M panel/e2e/*`、`M panel/src/views/Runtimes.tsx`、`M panel/tools/design-audit.mjs`、`M scripts/ruagent-daemon.ps1`、`D/?? skills|.claude/skills` 全部是**同伴在途**的改动，不是我）；`panel/dist` 被契约要求的门禁命令刷新（构建产物）；`panel/src/**` 的字节我一次未碰（工作树 `index.css` sha 仍是 `ed07224a…` = HEAD 的 blob）。
- 我起的守护进程共 4 个（各轮 8899/8900 的配对），**全部按我记录的 pid 停掉**并确认；junction 用 `rmdir` 删除（确认 gone）；`%TEMP%\ruagent-t183\` 内留有复核材料（`dist-pre`/`dist-post` 冻结产物、两臂 `audit-*.json` + `*.as-captured`、`geom.js` 探针与 `geom-*.out`、`pre-src`、`pins.sh`/`bytes.sh`/`c59.sh`/`rows.js`/`row18.js`）；按**具体路径**删即可。按命令行匹配我自己临时路径找过的"孤儿"都是本次会话自己的 shell，**未**按名/端口批量杀。
