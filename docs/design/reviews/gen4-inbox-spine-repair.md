# t170 修 V158-4：`.inbox-card` 的脊线「在源码里在、在计算值里不在」

> **性质**：repair（实现 + 自验），作者 = verify2。来源 = `t158` 的 **V158-4**（它只报不改，并明确「成因未证成、不猜」）。
> **一句话**：脊线不是画不出来，是**被同一张表里后写的一条同优先级规则盖掉了** —— `index.css` 里 `.inbox-card`（0,1,0）在前、`.ant-card { border: none; box-shadow: var(--panel-shadow); }`（0,1,0）在后 ⇒ **同权重按源码顺序，后者胜** ⇒ 计算 `box-shadow` 只剩 `var(--panel-shadow)`。修法是**把选择器收成 `.ant-card.inbox-card`（0,2,0）**：声明逐字不变、**没有加回 border**、脊线在两档都回来了。
> **起因与 `#inbox` 的关系**：`.inbox-card` 的宿主只有一个（`Agents.tsx:956`，`Inbox` 组件内），`App.tsx:47` 把它挂在 **`#inbox`** 上 ⇒ 这是用户看得见的一处缺陷。

---

## 0 判定表（按契约判据顺序）

| # | 判据 | 结论 | 读在哪 |
| --- | --- | --- | --- |
| 1 | 先量证「脊线没落地」（独立复核 t158，不采信）：当前字节上渲染真组件取计算 `box-shadow`，两档，逐字对照源码声明 | **passed** | §2：两档 `hasInset3px0 = **False**`，计算值逐字 = `var(--panel-shadow)` 的值；源码声明逐字在 `index.css` |
| 2 | 找出**为什么**那一半不见了（层叠证据，不许猜） | **passed（已指名 ①）** | §3：同一张表两条规则、**同权重 100**、`.ant-card` 在后 ⇒ 按顺序胜；②③④**都被实测排除**（`--signal` 有值、无内联、选择器命中） |
| 3 | 修到设计意图（脊线可见），**不许**加回 `border` | **passed** | §4：选择器 `→ .ant-card.inbox-card`（100→**200**），声明逐字不变；两档计算值首项即 `… 3px 0px 0px 0px inset`；`borderWidths` 全程 `0px/0px/0px/0px`、`borderStyle = none` |
| 4 | 不带动别的路由、不削弱判据（2 条无关路由两臂；行 4/38 `criterion`/`limits` 逐字不动；工具不在 inScope） | **passed** | §5：`home [7]=[7]`、`sessions []=[]`、`graph [6,7]=[6,7]`、`inbox []=[]`；行 4/38 的 `criterion`（75 / 21 字符）与 `limits`（`{"max":1}`）**逐字相同**；`tools/**` 未改 |
| 5 | 两臂对照 + **同路由每一行**（不只目标行） | **passed** | §6：四份字节（A/B/C/C2）+ 三组逐行对照，**我的改动单独那一对只动了 row 26/27（计时与产物哈希）** |
| 6 | 门禁 `npm --prefix panel run build` exit 0 | **passed** | §7（最终字节上跑，exit 0；未动 e2e ⇒ 不触发 `test:e2e`，也没用裸 `npx playwright test`） |
| 7 | 写清未覆盖什么 | **passed** | §8 |

---

## 1 四份字节（读数各属于谁）+ 一处**在飞改动**

| 臂 | 来源 | `panel/src/index.css` blob | dist 身份 |
| --- | --- | --- | --- |
| **A**（契约点名的修前臂） | `git worktree add --detach %TEMP%\t170\work-a 5a8f9f1`（= t158 落地的那个提交，`fix(panel): the counted elements were the step badges, not the cards`） | **`d3384f3f15085eb5dd7eda7ff5d17bd084eeb60b`** ✅（与契约给的一致） | `index.html DAE34BF40AE5BA09` · `index-DH8EJgem.css C94A9566236A42F0` |
| **B**（紧邻的修前态 = HEAD，含 t159+t162、不含我） | 主工作树（未改任何源码时构建） | `b9fc5d0d59fcbc7bd535d5d1f0d6516e89a12718` | `index.html 3CB89E31B3F6B9B3` · `index-BvDMgDmx.css E82633BB3F916C28` |
| **C**（我的改动落在 HEAD 上，中途读数） | 只改 `.inbox-card` 一行后构建 | `0fe57d187d9e3beabae4e02067ef9e79e124f692`（numstat 10/1） | `index.html B0802784A6EE2CB4` · `index-2tcemk4l.css 7989FEDCC6E3B471` |
| **C2 = 契约说的「最终字节」** | 同一棵树，**期间另一位成员（t186）把 27 行无关改动写进了同一个文件** ⇒ 最终态 = t186 + 我 | **`fbd9ed6b600d65ab61a20d99c650d11cedf7c10b`**（numstat 37/1） | `index.html 47A8D7251B9372E9` |

* **在飞改动披露（t20/C53 形）**：我第一次构建 C 时 `index.css` 是 `0fe57d18…`（我的 +10/−1）；收尾前回读发现它变成 **`fbd9ed6b…`（+37/−1）** —— diff 显示多出的 27 行是 **t186** 的（把 `#settings`/`App.tsx` 的内联样式搬成 `.flush/.mb0/.ml8/.pad0/.minw0/.w-full/.chat-field.half/.conflict-list/.cost-note/.app-min-h/.app-menu/.nav-item-label/.row.between` 这些工具类，目标 = 行 12）。**它不碰 `.ant-card`/`.inbox-card`/`--signal`**（回读：`:1272` 是我的复合选择器，`:1382` 是 `.ant-card`，`:56`/`:139` 是 `--signal`），所以我在**当前字节上把全部读数重取了一遍**（新增 **C2** 臂）⇒ 表内所有「最终字节」读数都属于 **C2**，C 只作为「我的改动单独落在 HEAD 上」的中间态出现（§6 的归因对照用它）。**我没有覆盖/回退 t186 的任何一行**。
* 源码里那条声明的行号随同伴改动漂过：t158 时 `:1197` → t159/t162 后 `:1236` → t186 后 **`:1272`**（这正是不拿行号当身份的理由）。
* 仪器：daemon 是 `%TEMP%\ruagent-team-target\debug\ruagent.exe` 的**拷贝**；每臂**自己的临时端口**（8881=B / 8882=A / 8883=C / 8884=C2）+ 临时 root + `RUAGENT_PANEL_DIST`（**指向冻结的 dist 拷贝**，避开 t159 记过的「活目录陷阱」）；`E2E_BASE_URL` 每次显式给；`NO_PROXY=127.0.0.1,localhost,::1`；**8787 = pid 14944 全程未触碰**。

## 2 判据 1：脊线确实没落地（独立复核，两档，逐字）

探针：`%TEMP%\t170\probe.mjs` —— 用 `page.route('**/api/v1/permissions')` 注入一条 pending（`api.ts:948-949` 的 `{pending:[…]}`）把**真组件**渲染出来（`cardCount = 1`），再读计算样式。**B（当前字节）与 A（t158 最终态）读数相同**：

| 档 | 计算 `box-shadow`（逐字） | `hasInset3px0` | `borderWidths` / `borderStyle` |
| --- | --- | --- | --- |
| dark | `rgba(255, 255, 255, 0.055) 0px 1px 0px 0px inset` | **False** | `0px/0px/0px/0px` / `none` |
| light | `rgb(222, 224, 228) 0px 0px 0px 1px, rgba(15, 20, 30, 0.05) 0px 1px 2px 0px` | **False** | `0px/0px/0px/0px` / `none` |

* 这两条计算值**恰好就是 `var(--panel-shadow)` 在对应档位的值**（回读 `--panel-shadow`：dark `inset 0 1px 0 rgba(255,255,255,0.055)`；light `0 0 0 1px var(--rule-soft), 0 1px 2px rgba(15,20,30,0.05)`）⇒ **卡片既没有边框、也没有脊线** ✅（`t158` 的 V158-4 被独立复现）。
* 与源码声明逐字对照：`panel/src/index.css`（当前 `:1272`）`.ant-card.inbox-card { box-shadow: inset 3px 0 0 var(--signal), var(--panel-shadow); }`；修前那一版（HEAD）是同一句、选择器只有 `.inbox-card` ⇒ **声明在、效果不在**。

## 3 判据 2：为什么那一半不见了（层叠证据，指名 ①）

探针按**文档顺序**枚举 `document.styleSheets` 里**所有命中该元素且声明了 `box-shadow`** 的规则（含 `!important` 标记与自算优先级）。**B 的两档读数逐字相同**：

```
rule#245  sel='.inbox-card'   specificity=100  important=False  value='inset 3px 0 0 var(--signal), var(--panel-shadow)'
rule#278  sel='.ant-card'     specificity=100  important=False  value='var(--panel-shadow)'          <- 同一张表、同权重、在后面
计算值 = 'var(--panel-shadow)' 的值        内联 = ''（空）
--signal 计算值 = '#f0a93b'（dark）/ '#c88413'（light）      选择器命中 = 是（两条都命中）
```

⇒ **指名：候选 ① —— 被后写的、同优先级的规则覆盖。** 四条候选的判定都是**实测**得出，不是排除法口头说：
* **① 成立**：`.ant-card { border: none; box-shadow: var(--panel-shadow); }`（当前 `:1382`）在 `.inbox-card` 之后，**权重同为 (0,1,0)**，`!important` 都没有 ⇒ 按源码顺序后者取。编译产物里的顺序也一致（rule#245 < #278）。
* **② 排除（实测）**：`--signal` **有值**（`#f0a93b` / `#c88413`）⇒ 不是「变量未定义导致整条声明失效」。（若真是这个，`--panel-shadow` 也不该出现在计算值里，因为整条声明会一起失效 —— 计算值的形态本身就指向「被另一条规则取代」。）
* **③ 排除（实测）**：`el.style.boxShadow` 为空，且枚举到的竞争规则**只有面板自己的两条**（没有 antd 运行时的 `<style>` 命中它、没有内联）。
* **④ 排除（实测）**：`el.matches('.inbox-card')` 为真（两条规则都命中该元素）⇒ 选择器没有失联。
* **副产品**：同一条 `.ant-card` 规则里的 `border: none` 也解释了为什么 t158 量到卡片 `border-width = 0px`（不是 antd 不画，而是被这条规则显式去掉）。

## 4 判据 3：修法（只提权重，不动声明，不加 border）

```css
/* 我的注释记录了这个层叠陷阱（见文件内） */
.ant-card.inbox-card { box-shadow: inset 3px 0 0 var(--signal), var(--panel-shadow); }
```
* 选择器 `.inbox-card`（0,1,0）⇒ **`.ant-card.inbox-card`（0,2,0）**：**靠权重取胜、不再依赖源码顺序**（这正是坑的根因，改顺序只是把陷阱留给下一次插入）。**声明逐字不变**（`inset 3px 0 0 var(--signal), var(--panel-shadow)`）。
* **没有加 `border`**：修后 `borderWidths = 0px/0px/0px/0px`、`borderStyle = none`（两档、C 与 C2 都一样）⇒ 不会把卡片重新送进行 4/38 的计数，也不会动行 30 判的那道 ring。
* **修后读数（C 与 C2 逐字相同，两档）**：
  * dark：`rgb(240, 169, 59) 3px 0px 0px 0px inset, rgba(255, 255, 255, 0.055) 0px 1px 0px 0px inset` ⇒ 脊线（`--signal` = `#f0a93b` = rgb(240,169,59)）**在**，面板影也**还在**；
  * light：`rgb(200, 132, 19) 3px 0px 0px 0px inset, rgb(222, 224, 228) 0px 0px 0px 1px, rgba(15, 20, 30, 0.05) 0px 1px 2px 0px` ⇒ 脊线（`#c88413` = rgb(200,132,19)）**在**，行 30 判的那道 1px ring **也还在**；
  * 胜出规则 = `.ant-card.inbox-card`（spec **200**），**它在表里更靠前却赢** ⇒ 这一条本身就是「权重而非顺序」的直接证据。
* 编译产物核对：`index-2tcemk4l.css` 里 `inbox-card` 只出现 **1** 次，就是 `.ant-card.inbox-card{box-shadow:inset 3px 0 0 var(--signal),var(--panel-shadow)}`（`/*` 注释 0 处，已被构建剥离）⇒ 没有留下一条裸 `.inbox-card` 规则。

## 5 判据 4：不带动别的路由 / 不削弱判据

| 项 | 读数 |
| --- | --- |
| 行 4 `criterion` / `limits` | B 与 C2：**逐字相同**（len **75**，`{"max":1}`），verdict 都 pass |
| 行 38 `criterion` / `limits` | 同上（len **21**，`{"max":1}`），verdict 都 pass |
| 失败行集合 | `home [7]=[7]` · `sessions []=[]` · `graph [6,7]=[6,7]` · `inbox []=[]` ⇒ **逐条相同** |
| `panel/tools/design-audit.mjs` | 未改（`tools/**` 不在 inScope；我连运行都不需要它来测脊线） |

## 6 判据 5：两臂对照 + 同路由每一行

**同一条 `#home` 路由上，三组逐行对照**（B = HEAD 修前，C = 只在 HEAD 上加我的改动，C2 = 最终字节 = t186 + 我）：

| 对照 | 差异承担者 | 移动的行 |
| --- | --- | --- |
| **B → C（我的改动单独那一对）** | **只有我** | row 26（`76ms → 75ms`，计时读数）· row 27 两档（产物 chunk 哈希 `index-uQbQNDJw.js → index-CMorSDAz.js`）⇒ **没有任何**判据行移动，行 4/5/13/14/30/38 全部逐字不变 |
| **B → C2** | t186 + 我 | row 12（`17 → 14 / ≤50`，**t186 的目标行**）· 26 · 27 ⇒ 我的那部分仍只有 26/27 |
| **A → C2（契约点名的修前臂 → 最终字节）** | t159 + t162 + t186 + 我 + 运行范围 | row 12（t186）· 13/14（`1 → 0`、`3 → 0`，**t159 的对比度修复**）· 26/27（计时/哈希）· 56（`2 条路由 → 4 条路由`：两臂跑的 `--routes` 集合不同，**比较口径产物**） |

⇒ **归因结论**：与我的改动有关的移动**只有 row 26/27 的计时与产物哈希**（前者是读数抖动、后者是内容变了必然换哈希）；**没有一行是因为脊线而变红/变绿的**（脊线不是任何一行的被测量 —— 这正是本单的一个 finding，见 §10 V170-2）。`#home` 两臂各 97 个行-档位读数里，A→C2 的 6 处移动都能指名归属。

## 7 判据 6：门禁

```
> npm --prefix panel run build          （在最终字节上跑）
   (!) Some chunks are larger than 500 kB after minification. …
   ✓ built in 5.20s
   build-panel: dist updated (55 assets, index.html swapped by rename)
GATE EXIT=0
```
内含 `e2e/i18n-check.mjs` + `tsc -b` + `tsc -p e2e/tsconfig.json --noEmit` + vite。**未新增/修改任何 e2e spec** ⇒ 不触发 `npm run test:e2e`，也**没有**用裸 `npx playwright test`。

## 8 我没有覆盖什么（判据 7）

1. **其余路由的脊线（先量再说）**：`.inbox-card` 在整个 `panel/src` 里只有**一个宿主**（`Agents.tsx:956`，`Inbox` 组件内；`index.css` 里是样例行）。`App.tsx:46-48` 显示同一个文件还导出 `Agents`（`#agents`）与 `Stats`（`#stats`）：**它们不用 `.inbox-card`**，所以脊线只在 **`#inbox`** 上出现。⇒ 我只在 `#inbox` 上取了脊线读数；**没有**逐路由扫「有没有别的脊线类」。
2. **其它「声明在源码里、计算值里没有」的样式**：我只**证明**了这一个实例，并点名它们属于**同一类**：任何写在前、被后面那条 `.ant-card { border: none; box-shadow: var(--panel-shadow); }`（单类、0,1,0、位置靠后）盖住的**单类卡片规则**（`box-shadow`/`border`/`background` 都会被同一条规则的对应属性盖）。我**没有**为此做全表扫描，也**没有**顺手改任何其它样式。
3. **视觉意图**：我只能证明计算值里出现了 `3px 0px 0px 0px inset`（两档、颜色 = 对应档的 `--signal`），**不能**证明「好看」，也没做像素对比（全程 `--no-pixels`）。
4. **其它档位**：只有 `dark`/`light`（面板自己的 `localStorage['ruagent.mode']`）。
5. **`#inbox` 空态**：工具在空态下对行 4 的 `.inbox-card` 子句直接印 **`.inbox-card 未测（该路由无 .inbox-card，空态）`** ⇒ 审计今天**看不到**脊线缺失（§10 V170-2）；我用注入数据把真卡片渲染出来才量到它。

## 9 自报的仪器失误（两条，都是我的）

* **`hasInset3px0` 第一次判成 `False`（假阴性）**：Chrome 计算值把颜色放前面 —— 串是 `rgb(240, 169, 59) 3px 0px 0px 0px inset`，而我的正则写的是 `/inset 3px 0px 0px/`（找「inset 在前」的写法）⇒ 修后第一次读数里我自己的探针说「没有脊线」，而**同一行打印出来的计算值明明有 `3px 0px 0px 0px inset`**。判据改成 `/\b3px\s+0px\s+0px\s+0px\s+inset\b/` 后，四份字节的读数才自洽（A/B=False、C/C2=True）。⇒ 与 C40 同族：**先确认读数属于谁/写法对不对，再把它当读数**；本报告里凡涉及脊线都是**修正后**的读数，并同时给出原始计算值供复核。
* **「胜出规则」我第一版打印错了**：我按 `ruleIdx` 取「最后一条非 `!important` 的命中规则」，于是 C/C2 里打印出 `winner: rule#278 '.ant-card' spec=100` —— 那只是**表里最后一条命中的规则**，不是**胜出的规则**（胜出的是 `rule#245 '.ant-card.inbox-card' spec=200`）。判定胜者的证据是**计算值本身**（脊线出现 = 权重 200 赢了顺序）与两条规则的 `specificity`，不是那个打印行。

## 10 findings

* **V170-1（medium，层叠陷阱的结构面）**：`.ant-card { border: none; box-shadow: var(--panel-shadow); }` 是一条**位置很靠后的单类规则**，它把**任何更早的单类卡片规则**在 `border`/`box-shadow` 上盖掉（本例：`.inbox-card` 的脊线；同一原因也解释了卡片 `border-width = 0px`）。⇒ 建议在面板 CSS 的约定里写一句（或加一条 lint/评审清单）：**给 antd 卡片加材质时就写具体到 `.ant-card.xxx` 的复合选择器**，别依赖「我的规则写在前面」。我**只改本单涉及的这一条**，没有动其它候选。
* **V170-2（low，审计看不见这类缺陷）**：行 4 的 `.inbox-card` 子句今天**永远不会**发现「脊线丢了」：空态时工具自己印 `.inbox-card 未测（该路由无 .inbox-card，空态）`；有卡片时也只数「元素自己带直接文本」的 `.inbox-card`（t158 的 V158-2 已证不可达）。⇒ 这个缺陷在全量审计（77 行）里**零信号**，本单是靠「源码里有、计算值里没有」的人工对照发现的；若希望审计真的守住它，需要先修 V158-2（`tools/**`，不在本单 inScope）。
* **（顺带，非缺陷，供台账引用）** `--signal` 是**按档变化**的 fill-grade 色（dark `#f0a93b` / light `#c88413`）⇒ 脊线的颜色随档位不同是**设计使然**，不是漂移。

## 11 纪律回执

* **写入集合**：`panel/src/index.css`（**唯一代码改动**：改选择器 + 一段解释性注释；声明逐字不变）+ 本报告。`panel/tools/**`、`panel/e2e/**`、`panel/src/views/**`、`crates/**`、`.github/**`、`scripts/**` **零改动**。
* **不覆盖别人的在飞改动**：本单期间 `index.css` 又被 **t186** 加了 27 行（行 12 的内联样式迁移）⇒ 我**只做字面替换**（`old_string` 唯一匹配），**没有**回退/覆盖它任何一行；最终字节因此是 `fbd9ed6b…`（t186 + 我）。
* **先读数、再改**：脊线缺失、层叠两条规则、`--signal` 有值、无内联、选择器命中、行 4/38 的 `criterion`/`limits`、逐行对照 —— **全部先量后动**；修后又在**当前字节**上把读数重取了一遍（C2）。
* **仪器/进程/收尾**：daemon 用**拷贝**；每臂自己的临时端口（8881-8884）与冻结 dist 拷贝（A/B/C/C2 各一份，避免活目录陷阱）；`E2E_BASE_URL` 显式给；8787 = **pid 14944 未触碰**；我起的 4 个 daemon 全按**记录 PID** 停止（端口已无监听）；收尾按**具体路径**删（`%TEMP%\t170*`，先列清单）、**不是我的 `t151*`/`t149z*` 未碰**；A 臂的 worktree 按 **C39 先 `cmd /c rmdir` 删 junction** 再 `git worktree remove --force`。
* **不采信摘要**：`t158` 的 V158-4 我**独立复现**（不是采信），成因是**量出来**的层叠顺序 + 权重，不是读源码猜的；`t159`/`t162`/`t186` 的贡献在逐行对照里**各自点名**，不混进我的账。
