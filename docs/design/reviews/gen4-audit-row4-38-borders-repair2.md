# t158 收口审计行 4 + 行 38：`#home` 的描边容器（接手 `t156` 的定位）

> **性质**：repair（实现 + 自验），作者 = verify2。来源 = `t156` attempt 1 的干净交接（它零源码改动、把定位做完，理由正当：**没有读数就改源码 = 制造一个未验证的绿**）。
> **一句话**：行 4 与行 38 共用同一个对象集，`#home` 上命中它的**不是那三张 antd `Card`**（它们只有祖先身份，且**计算 border-width 本来就是 0px**），而是 `.steps` 里那三个 **`.step-num` 数字徽章**（`index.css:808` 的 `border: 1px solid …`）；我把**徽章的环改画成 inset shadow**（`border-width: 0`，环的颜色/1px/圆角/尺寸逐项不变）⇒ 两行在 **dark+light 都从 `3 / ≤1` 变 `0 / ≤1`**，**内容逐字不变**。
> **一条比修复更重要的读数**：按交接里写的「把三张 Card 改成无边框」真的去做，**会打红行 30（light）**（`ring #dee0e4 1px → canvas 1.23:1 / panel 1.32:1` 变成 `无 ring`）⇒ 我**量到之后撤回了那三处改动**（`Home.tsx` 现在与 HEAD **逐字节相同**）。**最终写集只有 `panel/src/index.css` 一个文件。**

---

## 0 判定表（按契约判据顺序）

| # | 判据 | 结论 | 读在哪 |
| --- | --- | --- | --- |
| 1 | 先取逐元素证据（alpha / 哪两条边 / tag / 选择器路径） | **passed（带一处披露）** | §2：三条都是 `DIV · sides=4(四边全活) · 1px solid · alpha=1`；**工具 `--json` 只给 `{sel, sides}`，alpha 在 `:971` 算完即丢** ⇒ alpha/逐边明细来自我的探针（同一谓词），并把工具缺口立为发现 V158-2 |
| 2 | 三个 step `Card` 改无边框 + 计数 ≤1（dark+light）+ 内容一字不减 | **passed（做法与交接不同，已量证）** | §3：计数 **3→0**（两档）；**Card 改 borderless 对计数无效（计算 border 前后都是 0px）且会打红行 30 ⇒ 已撤回**；`.step-num` 的环改 inset shadow；内容文本逐字相同（173 字符 / 3 卡 / 3 按钮 / 3 strong / 3 p） |
| 3 | `.inbox-card` 必须是脊线（`borderWidth === 0px`）—— 实测 + 给读数 | **passed（本就是 0，附「为什么」与一条新发现）** | §4：`borderWidth = 0px/0px/0px/0px`、`borderStyle = none`（**两档、两臂都如此**）；**工具的该条款今天不可达**（它只数有直接文本的 `.inbox-card`，而真卡片没有直接文本）⇒ `inboxCards = 0`；附带发现 V158-4：**脊线本身没画出来**（计算 box-shadow 里没有 `inset 3px 0`） |
| 4 | 红→绿两边都用工具判定原文（修前臂钉法 + 内容级判据） | **passed** | §5：修前 `3 / ≤1` + `3 (…step-num)` → 修后 `0 / ≤1` + `0`，两档、**各 3/3 次** |
| 5 | 判据未削弱 / 不带动别的路由 | **passed** | §6：`criterion` 与 `limits` 逐字相同（75 / 21 字符，`{"max":1}`）；工具未改；`graph` [6,7]=[6,7] 相同；`sessions` 两臂 3/3 次失败集均为空（早先单次 row 20 红已证为 flaky）；`task`（与 `.step-n` 共享规则的路由）[ ]=[ ] |
| 6 | 面板门禁 `npm --prefix panel run build` exit 0 | **passed** | §7 |
| 7 | 写清未覆盖什么 | **passed** | §8 |

---

## 1 写集与两臂身份

| 项 | 值 |
| --- | --- |
| **最终写集** | `panel/src/index.css`（**唯一代码改动**，blob `d3384f3f15085eb5dd7eda7ff5d17bd084eeb60b`）+ 本报告 |
| `panel/src/views/Home.tsx` | **与 HEAD 逐字节相同**（blob `fdfcb51e44afb4405fbd23edc00e9ca256bf8f44`）—— 我改过、又**按读数撤回**（§3.3） |
| **修前臂** | `git worktree add --detach %TEMP%\t158-before HEAD`（HEAD = `a1c2825`，**Home.tsx blob `fdfcb51e…`** = 内容级判据，不随提交名漂）+ junction 借 `node_modules` + `npm --prefix … run build` exit 0 ⇒ dist `index.html` = `82606EA02AE4E5FE` |
| **修后臂** | 主工作树最终字节 ⇒ dist `index.html` = `9EBB7789E121D872`（门禁那次构建） |
| 仪器 | daemon 是 `%TEMP%\ruagent-team-target\debug\ruagent.exe` 的**拷贝**（sha256(24) `C4C59173BB6AD05C2CDAE6D1`）；每臂自己的**临时端口**（8871/8872/8873/8874/8875/8876）+ **临时 root** + `RUAGENT_PANEL_DIST`；`E2E_BASE_URL` **每次显式给**（B29 的危险默认不改工具、只显式覆盖）；`NO_PROXY=127.0.0.1,localhost,::1` |
| 审计调用 | `node panel/tools/design-audit.mjs --check --routes=… --modes=dark,light --tabs=40 --no-shots --no-pixels --allow-not-measured --json --base-url=… --out=<仓外>` |
| 活环境 | **8787 = pid 14944 全程未触碰**；t160 的两个变异窗口动的是 `crates/daemon/src/chat.rs` 与 `crates/store/tests/ledger_reopen.rs`，本单**不编译 Rust、不改 Rust、且用的是冻结的二进制拷贝** ⇒ 窗口在物理上不可能影响本单任何读数（同 t155 的论证；因此也没有「读数作废」） |

---

## 2 判据 1：逐元素证据（补上交接缺的那一项）

**工具自己给的**（`metrics.json` / `--json` 的 `captures[].metrics.borders.strictSamples`，`#home` 两档各 3 条，`sides` 字段）：
```
sel=div.steps:nth-of-type(3) > div.ant-card.ant-card-bordered:nth-of-type(1) > div.ant-card-body > div.step-num   sides=4
sel=div.steps:nth-of-type(3) > div.ant-card.ant-card-bordered:nth-of-type(2) > div.ant-card-body > div.step-num   sides=4
sel=div.steps:nth-of-type(3) > div.ant-card.ant-card-bordered:nth-of-type(3) > div.ant-card-body > div.step-num   sides=4
strictContainers=3   allBordered=6   hairline=8   fourSide=5   extras.inboxCards=0      （dark 与 light 同）
```
**工具没给的 alpha / 逐边明细**（我的探针 `%TEMP%\t158\probe.mjs`，谓词按工具字节逐条复刻：`CONTAINER_TAGS` = `design-audit.mjs:517`；「有直接文本」= `:614-624` 的 TreeWalker 规则；`nSides>=2` 与 `每边 alpha>0.04` = `:964-985`；控制类豁免用验收给的正则）：

| 模式 | tag | nSides | 活边 | 每边 width / style / **alpha** | 路径（末尾即命中元素） |
| --- | --- | --- | --- | --- | --- |
| dark | `DIV` | **4** | top,right,bottom,left | **1px / solid / 1**（四边同） | `div.steps:nth-of-type(3) > div.ant-card.ant-card-bordered:nth-of-type(1) > div.ant-card-body > **div.step-num**` |
| dark | `DIV` | **4** | top,right,bottom,left | 1px / solid / 1 | …`:nth-of-type(2)`… `div.step-num` |
| dark | `DIV` | **4** | top,right,bottom,left | 1px / solid / 1 | …`:nth-of-type(3)`… `div.step-num` |
| light | `DIV` | **4** | top,right,bottom,left | 1px / solid / 1（颜色 `rgb(138, 86, 0)`） | 同上三条 |

* dark 的边框色 = `rgb(245, 180, 87)`、light = `rgb(138, 86, 0)`（都是 `--ant-color-primary-border` 的解析值）⇒ **alpha = 1（不透明）**，所以四条边全部「live」⇒ `nSides = 4 ≥ 2` ✓。
* **命中对象是 `div.step-num`（三条），不是 Card**：路径的最后一段才是命中元素，`div.ant-card.ant-card-bordered` 只是它的**祖先**。这也解释了为什么卡片的 `border` 与计数无关（§3）。
* **诚实披露**：`alpha` 这一项**不是**工具 `--json` 给的（`strictSamples` 的字段只有 `sel, sides`；`"a":` 在该 JSON 里出现 **0** 次），工具在 `:971` 算出 `c.a > 0.04` 后**丢掉**了它 ⇒ 逐边 alpha 出自我的探针（同一谓词、同一页面、同一档位），工具的 JSON 只交叉确认了「两档 `sides = 4`」。**这条缺口已立为发现 V158-2**（`tools/**` 不在本单 inScope，我没有改工具）。
* 我的探针在**每档都显式设** `localStorage['ruagent.mode']`（`theme.tsx:286/294/298-299` 是面板自己的档位机制），并回报 `dataset/colorScheme = dark|light` ✓ ⇒ 「两档」不是假设。

## 3 判据 2：计数 3 → 0（以及为什么**不是**按交接写的做法）

### 3.1 量证：那三张 Card 从来不是命中对象
修前臂（worktree @ HEAD，未改任何源码）我的探针读数：
```
.steps: cards=3   cardBorderWidths = [0px/0px/0px/0px ; 0px/0px/0px/0px ; 0px/0px/0px/0px]
        cardClasses      = [ant-card ant-card-bordered ant-card-small css-u6izyw ruagent ; ×3]
        badge '1'..'3'   : border=1px/1px/1px/1px style=solid boxShadow=none
```
⇒ **三张 Card 的计算 `border-width` 本来就是 `0px`**（`ant-card-bordered` 这个类在这个树里并不产生可见边框；面板已把卡片的边改由 `--panel-shadow` 承担）⇒ 把它们改成 `variant="borderless"` 对 `strictContainers` **不可能**有任何影响。真正被计数的是徽章：`border = 1px/1px/1px/1px`（`index.css:808`）。

### 3.2 实际修法（`panel/src/index.css`，一处、作用域收敛）
```css
.step-num { border: none; box-shadow: inset 0 0 0 1px var(--ant-color-primary-border); }
```
* **为什么是这个而不是删掉徽章**：判据要的是「计数 ≤1」而**不是**「把内容去掉」。徽章是**装饰性数字**，它的环用 **inset shadow** 重画 = 契约对 `.inbox-card` 脊线已经**明文认可**的机制（`borderWidth === 0px`）⇒ **看起来一样**（同色、同 1px、同 24×24、同 `border-radius: 999px`），但它不再是「描边容器」。
* **为什么只改 `.step-num` 而不改共享规则 `.step-n, .step-num`**：`.step-n` 由 **`TaskDetail.tsx:621`** 使用（`#task` 路由）⇒ 改共享规则会**带另一个路由**。我的改动只加 `.step-num` 覆盖行（`index.css:819-826`）⇒ `#task` 不动；并且我把 `task` 也当**控制路由**实测了（§6：[ ]=[ ] 相同）。
* **`isComposerOnly` 的真相**（`design-audit.mjs:4123`）：`strictContainers === 0 || (=== 1 && 样本含 "composer")` ⇒ 在 `#home` 上「≤1」实际意味着 **0**（home 没有 `.composer`）⇒ 只去掉卡片边框绝不可能过，必须让徽章不再是描边容器。

### 3.3 按交接写的做法，我做了、量了、**撤回了**
我把三张 Card 改成 `<Card size="small" variant="borderless">`（antd **6.6.3**，`Card.d.ts:67` 有 `variant?: 'borderless' | 'outlined'`；`bordered` 已 deprecated），构建 exit 0，然后对比**同一条路由上每一行**的读数：
```
row 30/light : BEFORE True|ring #dee0e4 1px → canvas 1.23:1 / panel 1.32:1   ->   AFTER **False|无 ring**
```
⇒ 把卡片改成无边框会**抹掉 light 档下卡片的那道 ring**（`--panel-shadow` 的 `0 0 0 1px var(--rule-soft)` 随 `ant-card-bordered` 一起消失），而行 30 正是在判这道 ring ⇒ **这是一处由「交接里写的做法」引入的回归**（也会削弱判据）。于是我**逐字撤回**三处改动（撤回后 `Home.tsx` blob 回到 `fdfcb51e…` = HEAD ✅），只留 `.step-num` 那一处，并**重跑**确认行 30 恢复 PASS（§5/§6 的最终读数里 `row 30` 已不在移动列表里）。**这条已立为发现 V158-3。**

### 3.4 内容一字不减（读数，不是承诺）
两臂 × 两档，`.steps` 子树：
```
text identical across arms = True (both modes)   stepsTextLen  173 -> 173
cards 3 -> 3     buttons 3 -> 3     strongs 3 -> 3     paragraphs 3 -> 3
badge '1'..'3' : border 1px/1px/1px/1px (solid, shadow=none)
              ->  0px/0px/0px/0px (none, box-shadow = inset 0 0 0 1px rgb(245,180,87) [dark] / rgb(138,86,0) [light])
size 24x24 与 border-radius 999px 前后相同；徽章背景前后相同（dark rgb(86,65,32) / light rgb(255,249,230)）
```
⇒ 「≤1」由 **3 → 0** 达成（两档），而卡片数、按钮、`strong`/`p` 文本与整段 `innerText` **逐字未变**。

## 4 判据 3：`.inbox-card` 的脊线（实测 + 为什么）

* **实测（我的探针，两档、两臂）**：宿主 `Agents.tsx:956` 渲染出的真元素
```
.inbox-card : borderWidth = [0px,0px,0px,0px]   borderStyle = none   alphas = [1,1,1,1]
              boxShadow(dark)  = rgba(255,255,255,0.055) 0px 1px 0px 0px inset
              boxShadow(light) = rgb(222,224,228) 0px 0px 0px 1px, rgba(15,20,30,0.05) 0px 1px 2px 0px
```
⇒ **判据的判定量（`borderWidth === 0px`）今天就满足** —— 但**理由不是**「有人把它修成脊线」，而是 **`ant-card` 在这个树里本来就没有计算边框**（§3.1 同一现象）⇒ 这一条**无需改动，我也没有改动**（写集里没有 `Agents.tsx`）。
* **为什么工具自己看不到它（新发现）**：条款的代码在 `design-audit.mjs:1159`：`if (visText && cls.includes("inbox-card"))`，`visText` = **该元素自己有直接文本**；而真卡片的文本全在子元素里（`.row > .span`/`Button`）⇒ `extras.inboxCards` 永远是 **0**。我**用注入的数据把真卡片渲染出来**（`page.route('**/api/v1/permissions')` 给一条 pending，`api.ts:948-949` 的 `{pending:[…]}`）并复刻同一谓词：
```
.inbox-card 元素数 = 1     countToolWouldSee(directText && visible) = **False**
工具 JSON：extras.inboxCards = 0, inboxCardBordered = 0   （两档、两臂）
```
⇒ **行 4 的 `.inbox-card` 附带条款在当前工具下不可达**（`ridgeOk = ex.inboxCards === 0 || …` 恒真）⇒ 它今天**不会**因为卡片有边框而变红，**也不会**因为脊线丢失而变红。立为发现 **V158-2**（与 §2 的 alpha 缺口同属仪器）。
* **顺带量到的一处真缺陷（发现 V158-4，我没有修）**：`.inbox-card` 的脊线规则确实存在于源码与产物（`index.css:1197`、构建后的 `inbox-card{box-shadow:inset 3px 0 0 var(--signal),var(--panel-shadow)}`），但**计算值里没有 `inset 3px 0`**（上面那两条 box-shadow 只等于 `var(--panel-shadow)`）⇒ 卡片今天**既没有边框、也没有脊线**。这是「以脊线代替边框」的语义**没有落地**（原因未定：`var(--signal)` 解析/优先级/antd 6 注入顺序都只是候选，我没有把其中任何一个证成事实）⇒ 按「读数归读数、设计归设计」**只报不改**。

## 5 判据 4：红 → 绿（工具判定原文，各 3/3 次）

**修前臂**（worktree @ HEAD，内容级判据 `Home.tsx` blob `fdfcb51e…`，dist `82606EA0…`，3 次逐次相同）：
```
row 4  verdict=fail  home/dark = 3 / ≤1     home/light = 3 / ≤1            failingRows=[4,7,13,14,38]
row 38 verdict=fail  home/dark = 3 (div.steps:nth-of-type(3) > div.ant-card.ant-card-bordered:nth-of-type(1)
                                     > div.ant-card-body > div.step-num)
                     home/light = 3 (同一条路径)
```
**修后臂**（最终字节，dist `9EBB7789…`，3 次逐次相同）：
```
row 4  verdict=pass  home/dark = 0 / ≤1   home/light = 0 / ≤1   （sessions/graph/inbox 也 0 / ≤1）
row 38 verdict=pass  home/dark = 0        home/light = 0
failingRows=[7,13,14]      （4 与 38 已离开失败集）
```
* **两臂在冻结 dist 上各跑 3 次**，失败集**只差 4 与 38** ⇒ 这是干净的「一处缺陷的两个判据面、一次收口」。
* 另有一条**数字层面的对照**：`strictContainers` **3 → 0**、`allBordered` **6 → 3**、`hairline` **8 → 5**、`fourSide` **5 → 2**（后三项减掉的正好是那三个徽章）—— 计数方向一致，不是把行「躲过去」。
* 行 5（`allBordered ≤ 40`）因此从 `6 / ≤40` 变 `3 / ≤40`，**仍 PASS**（不是把一行换成另一行红）。

## 6 判据 5：判据未削弱 / 不带动别的路由

| 项 | 修前臂 | 修后臂 | 判定 |
| --- | --- | --- | --- |
| 行 4 `criterion` / `limits` | len **75** / `{"max":1}` | len **75** / `{"max":1}` | **逐字相同**（字符串相等 = True） |
| 行 38 `criterion` / `limits` | len **21** / `{"max":1}` | len **21** / `{"max":1}` | **逐字相同**（相等 = True） |
| `panel/tools/design-audit.mjs` | 未改（`git status -- panel/` 只有 `M panel/src/index.css`） | 同 | 量具未动 |
* **无关路由**（同一次运行里两臂各 3 次 `--routes=home`，以及 5 路由那次全跑）：
  * `graph`：失败集 **[6,7] = [6,7]**（两臂相同）✓
  * `sessions`：两臂 **3/3 次失败集都是 `[]`** ✓（**早先单次运行里 `sessions` 的 row 20 红过一次** —— 我在冻结 dist 上重跑 6 次都没再现（两臂各 3 次），且那一次红在**未修复的** dist 上 ⇒ 判为 **row 20 的失败态注入探针 flaky**，与 t155 在同一行的结论一致；**不是**本改动带动的，也已作为读数登记）
  * `task`（我额外加的控制路由，因为 `.step-n` 与 `.step-num` 共享规则）：**[] = []** ✓
  * `inbox`：修后全部行 PASS（`[]`）✓
* **一处必须说清的行间差异**：`#home` 上 **row 7 / 13 / 14 在两臂都红**（3/3 次各自如此），且它们的读数**随 root 里的数据/构建而变**（早先另一次在**不同填充的 root** 上的单跑里这三行是绿的，源字节相同）⇒ 它们**先于本改动存在**、与本改动无关；我把它作为读数登记（发现 V158-5），不去替它们的 owner 改。

## 7 判据 6：面板门禁（我自己跑）

```
> npm --prefix panel run build
   (!) Some chunks are larger than 500 kB after minification. …
   ✓ built in 4.14s
   build-panel: dist updated (55 assets, index.html swapped by rename)
GATE EXIT=0
```
内含 `e2e/i18n-check.mjs` + `tsc -b` + `tsc -p e2e/tsconfig.json --noEmit` + vite（`panel/scripts/build-panel.mjs`）。**没有新增/修改任何 e2e spec** ⇒ 按契约不触发 `npm run test:e2e`，也**没有**用裸 `npx playwright test`。同一命令也用于**修前臂**（worktree 内，exit 0，`✓ built in 3.62s`）。

## 8 我没有覆盖什么（判据 7）

1. **其余路由的行 4/38**：只跑了 `home,sessions,graph,inbox,task`（被验对象是 `#home`）；全站 13 路由未跑 ⇒ 别的路由是否也有同类徽章未测。
2. **真实屏幕阅读器**：本单只读 DOM/计算样式与工具判定，**没有**用 NVDA/JAWS 之类真实 AT。
3. **视觉意图是否被改变**：我**只**证明了计数、几何（24×24、radius、1px 环）与文本不变 —— **不能**证明「好看」；把环从 `border` 改画成 `inset shadow` 在亚像素/抗锯齿上与边框**可能**有肉眼难辨的差异，我**没有**做像素对比（两轮都带 `--no-pixels`）。这一点是刻意的：判据量的是 border-width，不是观感。
4. **其它档位**：只有 `dark` 与 `light`（面板自己的 `localStorage['ruagent.mode']`）；窄屏档（row 51/52/54 那些 `modes: ["dark"]` 的窄视口行）不在本单读数里。
5. **工具缺口本身**（`strictSamples` 丢 alpha、`.inbox-card` 条款不可达）我**只报不改** —— `tools/**` 不在 inScope。
6. **脊线语义缺失（V158-4）的成因**未证成（只量到「计算值里没有 `inset 3px 0`」）；因此我**没有**动 `Agents.tsx` / `.inbox-card` 的样式。

## 9 纪律回执

* **写入集合**：`panel/src/index.css` + 本报告。`panel/src/views/Home.tsx` 曾改过三处（`variant="borderless"`）并**按读数逐字撤回**（现与 HEAD blob 相同）；`panel/src/views/Agents.tsx`、`panel/src/i18n/**`、`panel/e2e/**` **零改动**；`tools/**`、`crates/**`、`.github/**`、`scripts/**`、`docs/design/MASTER.md`、`Memory.tsx`、`Settings.tsx` **零改动**（`tools/**` 只执行、只读）。
* **先读数、再改**：所有命中对象的身份、alpha、逐边、Card 的 0px、`.inbox-card` 的 0px、`inboxCards=0`、行 30 的回归，**都是先量到再动手**；撤回也是先量再撤。
* **仪器**：私有 daemon **拷贝**（`C4C59173…`）+ 每臂自己的临时端口/root/`RUAGENT_PANEL_DIST`；`E2E_BASE_URL` **每次显式给**（不去改工具的 8787 默认，B29 的修法就是这条）；`NO_PROXY=127.0.0.1,localhost,::1`；8787 的活守护 **pid 14944 未触碰**。
* **浏览器收尾**：我的探针每轮 `browser.close()` 正常退出；收尾按**我自己临时路径**（`t158*`）找孤儿，**不按进程名/端口批量杀**。
* **junction 按 C39**：`cmd /c rmdir %TEMP%\t158-before\panel\node_modules`（**先删链接**）→ `git worktree remove --force` → 再按**具体路径**删 `%TEMP%\t158` / `t158-before`（**列清单在前，不用 glob**）；不是我的条目（`t151*`、`t149z*` 等）**一个没碰**。
* **t160 窗口**：见 §1 末 —— 本单不编译 Rust、不改 Rust，仪器是冻结二进制拷贝，窗口在物理上不可能污染本单读数。
* **不采信摘要**：t156 的定性（「命中对象集的是三张 Card」）我**没有采信**，而是用路径末段 + tag + 逐边读数**推翻**了它（§2/§3.1），并因此**没有**照抄「改卡片」的做法（照抄会带出行 30 的回归，§3.3）。

## 10 findings

* **V158-1（medium，交接定位的结论要改）**：行 4/38 在 `#home` 的命中对象是 **`.steps` 里的三个 `.step-num` 徽章**（`DIV`，四边 `1px solid`，alpha=1），**不是**那三张 antd `Card`（它们只是路径里的祖先；且计算 `border-width` 本来就是 `0px`）。t156 的交接把路径末段读成了 Card ⇒ 下一个照它做的人会改错对象、白跑一轮。
* **V158-2（medium，仪器缺口，`tools/**` 不在 inScope）**：① `borders.strictSamples` 只带 `{sel, sides}`，**把 `:971` 算出的逐边 alpha 丢掉**（本单要的 alpha 只能靠复刻谓词的探针）；② 行 4 的 `.inbox-card` 条款**不可达** —— `:1159` 要求该元素自己有直接文本，而真卡片的文本都在子元素里 ⇒ `extras.inboxCards` 恒为 0 ⇒ 条款既不保护脊线、也不会因边框而红（我用注入数据渲染真卡片证明了 `countToolWouldSee = False`）。
* **V158-3（medium，别照抄「把 Card 改 borderless」）**：把 `#home` 三张 step Card 改成 `variant="borderless"`（antd 6.6.3 支持）会**打红行 30（light）**：`ring #dee0e4 1px → canvas 1.23:1 / panel 1.32:1` 变成 **`无 ring`**（`--panel-shadow` 的 1px ring 随 `ant-card-bordered` 一起消失）；而它对 `strictContainers` **没有任何作用**（卡片计算边框本来就是 0px）。⇒ 该做法是「无效 + 有害」，我按读数撤回了它；下一位不要再试。
* **V158-4（low，真缺陷、只报不改）**：`.inbox-card` 的脊线**没有落地** —— 规则在源码（`index.css:1197`）与产物里都有，但计算 `box-shadow` 只剩 `var(--panel-shadow)`，**没有 `inset 3px 0 0 var(--signal)`** ⇒ 卡片今天既无边框也无脊线；成因未证成（不猜）。
* **V158-5（low，先存的行）**：`#home` 的 **row 7 / 13 / 14 在两臂都红**（各 3/3 次），且随 root 数据/构建变化（另一次不同 root 的单跑里三行是绿的，源字节相同）⇒ 与本改动无关，登记给这几行的 owner；同时**顺带佐证 t155 的 row 20 flaky 结论**（`sessions` 上同字节 6 次重跑全绿，早先那一次红落在未修复的 dist 上）。
