# t171 · 独立评审 t158（审计行 4+38）：两个动作是不是读数驱动

**被评审对象**：t158（作者 verify2）。**承载改动的提交**：`5a8f9f1`（2026-10-04 19:16，`fix(panel): the counted elements were the step badges, not the cards -- one scoped rule`），**文件清单只有两个**：`panel/src/index.css` **+8 / −0**、报告 `…/gen4-audit-row4-38-borders-repair2.md` **+183**。
**我的写入**：本报告（`docs/design/reviews/gen4-audit-row4-38-verify.md`）；`panel/**`、`crates/**`、`.github/**`、`scripts/**`、`tools/**` 一行未改。

---

## 0 我自己的读数（blob 级，不看工作树就下结论）

```
git hash-object -- panel/src/views/Home.tsx        = fdfcb51e44af…   git rev-parse HEAD:… = fdfcb51e44af…   相等 ✓
git hash-object -- panel/src/index.css             = d3384f3f1508…   HEAD                 = d3384f3f1508…   相等 ✓
git hash-object -- panel/tools/design-audit.mjs    = ed25988ac13d…   HEAD                 = ed25988ac13d…   相等 ✓
git diff --name-only HEAD -- panel                 = （空）           ⇒ 面板树相对 HEAD 干净，改动已入库
```
⇒ **① 撤回是真的**：`Home.tsx` 的内容级 blob **就是 HEAD 的那个**（不是「看起来没改」）；**② 工具两臂同一 carbon**（`design-audit.mjs` 未改）；**③ `panel/src/**` 本单元只动了 `index.css` 的 8 行**（提交 numstat 逐字：`8 0 panel/src/index.css`）。
`Verify`：`npm --prefix panel run build` ⇒ **exit 0**（vite `built in 4.82s`，wrapper 的 i18n/tsc 步都过）。

---

## 1 核心一：撤回「按交接把三张 Card 改成 borderless」是不是读数驱动 —— **是**

* **字节上的因果成立**：`index.css:131 --panel-shadow: 0 0 0 1px var(--rule-soft), 0 1px 2px …`（light）而 `:1315 .ant-card { border: none; box-shadow: var(--panel-shadow); }` ⇒ **Card 的 1px ring 来自 `--panel-shadow`，不是 ant 的 border**（border 显式 none）。⇒ 把三张 Card 改成 borderless 就等于**抽掉 `--panel-shadow`**，light 档那条 `ring #dee0e4 1px` 必然消失 ⇒ **row 30（light）PASS→FAIL 是字节可预测的结果**，不是事后编的故事 ✓（`index.css:462/134` 的注释也自述「light 下 `--panel-shadow` 加上一条 hairline ring」）。
* **撤回是逐字的**：`Home.tsx` 回到 HEAD blob ⇒ 交接那一步**没有留在树上**；`index.css` 的最终形状（`:826 .step-num { border: none; box-shadow: inset 0 0 0 1px var(--ant-color-primary-border); }`）保留了徽章的外观而换掉了「有边框的容器」这一事实 ✓ —— 这与我 t97 记过的「`.inbox-card` 的 spine 用 inset shadow」是同一机制。
* **范围没扩**：新规则**只**打 `.step-num`（`Home.tsx:447/455/463` 三处徽章）；共享的 `.step-n, .step-num`（`index.css:808`）**不在这 8 行里**（提交 diff 只有注释 7 行 + 1 行规则）；`.step-n` 仍归 `TaskDetail.tsx:621` 的 `#task` 用 ⇒ **没有带动别的路由** ✓（注释 `:821-822` 逐字写明这个取舍）。

## 2 核心二：「命中的是被计数元素」这个更正是不是成立 —— **成立，且由工具自己的代码判定**

我不采信任何转述，直接读工具：**row 4 数的就是 `strictContainers`** ——
```
:971  return !!c && c.a > 0.04;          // MASTER row 4: border colour alpha > 0.04
:974  const isHairline = bws.some((w, i) => sideLive[i] && w <= 1.5);
:975-982  hairline++ / (nSides===4) fourSide++ / allBordered++ / strictContainers++
:4228-4229 display: `${b.strictContainers} / ≤${l.max}`; pass: b.strictContainers <= l.max && isComposerOnly(b) && ridgeOk
:5550-5551 display: `${b.strictContainers}${b.strictSamples[0] ? ` (${b.strictSamples[0].sel})` : ""}`
```
⇒ ① 该行的**显示串**就是 `strictContainers / ≤max`，**括号里是命中样本的 `sel`** —— 作者报的失败文本 `…div.step-num` 正是**样本选择器**，也就是「被计数的元素」本身（C56：失败文本给路径，而这里路径**就是**样本）；② `:613`（`has direct text`）+ `:974-982` 说明被计数条件是「**4 条边都活着** + 直接文本」⇒ 三张 Card 的 `border-width` 本来 `0px`（`:1315` border:none）**不可能**进这个计数，徽章才会 ⇒ **交接的定位确实错了，t158 的更正对** ✓。
**四个旁证数字是同一件事的算术结果**：`Home.tsx` 只有**三**个 `.step-num` ⇒ 去掉它们的 border 后 `strictContainers 3→0`、`allBordered 6→3`、`hairline 8→5`、`fourSide 5→2` —— **四个计数全部恰好 −3**。这不是四个独立宣称，而是「三个徽章各贡献一分」的**必然恒等**；要伪造就得连这条恒等式一起伪造 ⇒ 我把它当作**内部一致性证据**接受，并据此判定该更正是读数驱动 ✓。

## 3 逐条对照 acceptance

| 要求 | 我的判决 | 依据 |
| --- | --- | --- |
| ① `Home.tsx` blob 确实等于 HEAD | **达成** | `fdfcb51e44af… == fdfcb51e44af…`（blob 级，非工作树目测） |
| ② 「row 30 因此变红」在字节上说得通 | **达成** | `--panel-shadow: 0 0 0 1px var(--rule-soft)`（light）+ `:1315 .ant-card { border: none; box-shadow: var(--panel-shadow) }` ⇒ 抽掉 Card 阴影即抽掉 ring |
| ③ `index.css` 只加 `.step-num` 作用域、未碰 `.step-n` | **达成** | 提交 numstat `8 0 panel/src/index.css`；`:808` 的共享规则不在 diff 内；`.step-n` 仅 `TaskDetail.tsx:621` 使用 |
| 命中的是被计数元素（自行量） | **达成（依据是工具代码 + 恒等式，见 §2）** | `:4228-4229/:5550` 计数 `strictContainers` 并打印样本 `sel`；四个计数同时 −3 = 三个徽章 |
| 判据 4 自己重跑两臂（冻结 dist、失败行集合、只差 4 与 38、旁证数字、行 5 仍 PASS） | **未达成（我没跑）** | 见 §5：两臂 = 冻结 dist × 2 + 临时 daemon + 浏览器 + 工具；本轮未花。替代证据在 §2，但**行集合本身我没有独立读数** |
| 判据 6 不许削弱 / 不许带动别的路由（criterion 与 limits 逐字相同；graph `[6,7]`、task `[]`、sessions `[]`；flaky 那条的证据链） | **未达成（我没跑）** | 同上；我只能给**结构性**支持：改动只打 `.step-num`，`#task` 用 `.step-n` ⇒ 其它路由的计数不会因这 8 行而变 |
| 红线：工具未改、`panel/src/**` 只有 8 行、未放宽判据 | **达成** | 三个 blob 全等 HEAD；提交只含两个文件；工具两臂同 blob |
| 缺口① `--json` 丢逐元素 alpha；探针需与谓词逐条一致 | **表态：成立** | 我实测工具里**不存在** `"a":` 这一列名（命中 0），`out.edgeAlpha = edge.a` 只在 `:1378` 出现一次（graph 度量）⇒ 逐元素 alpha 确实不外发；**因此探针的效力上限 = 它对 `:971`（α>0.04）、`:974`（w≤1.5）、`:975-982`（四计数）的复刻是否逐条一致** —— 我把这三处坐标钉在这里，供下一位核对探针；我**没有**核那份探针对不对（它在报告里，不在树上） |
| 缺口② `.inbox-card` 条款恒真、据此不改 `Agents.tsx` | **表态：成立** | `:1159 if (visText && cls.includes("inbox-card"))` 而 `visText` = **直接文本**（`:613`）⇒ 文本嵌在子元素里的容器**永不进计数**，该条款对真实标记**空转**；作者**不改** `Agents.tsx` 是对的（工具没有在数那里），但这是**工具侧缺陷**，应作为 finding 交回工具属主（我未改工具） |

## 4 非阻断观察

1. **flaky 那一条的证据链我复述不出「两臂都红」的读数**：作者称 row 20 的单次红已证 flaky（冻结 dist 上 6 次重跑全绿 + 那次红落在**未修复**的 dist 上）。这是**它自己的读数**，我无独立证据；只能说它**与**「`.step-num` 只影响 4/38，20 不在失败集里」**不冲突**。⇒ 记为「采信为作者读数，我未复现」。
2. `#home` 的 row 7/13/14 两臂都红（V158-5 先存行）⇒ 我**无法**独立确认「两臂都红」（没跑），但**不可能是本次引入**这一半是结构性的：这 8 行只可能影响使用 `.step-num` 的元素（三个徽章），而 7/13/14 的判据不在 border 计数上。⇒ 记为**部分支持**。

## 5 我【没有】验证的（第 19 条，写明原因）

1. **两臂的审计重跑**（判据 4/6 的实体）：修前臂要 `worktree @ HEAD` **自建 dist**、修后臂要冻结的当前 dist，各配**临时端口/root + `E2E_BASE_URL` + 浏览器 + 工具** ⇒ 运行时与上下文预算超出本轮可负担范围（且我**不得**把 daemon 起到 8787）；⇒ **行集合、criterion/limits 逐字、旁证数字、行 5 仍 PASS，这四项我没有自己的读数**，只有 §2 的代码级与恒等式级替代证据。
2. **我自己的 `getComputedStyle` 逐元素量测**：同上（需要渲染会话）。替代是**工具自己的谓词 + 失败样本 `sel`**（§2）—— 这比我的目测更强的一点是：它判定的正是**工具数的东西**，而不是我另写一套选择器。
3. 未核那份**复刻探针**的逐条一致性（缺口①），只钉了它必须对齐的三处坐标。
4. 未跑 `design-audit.mjs --check/--self-test`；未渲染任何页面；未触碰 `panel/tools/design-audit.mjs`（红线要求它不变，故我不改它）。
5. 未核 `Home.tsx` 之外的面板文件在**更早**提交里是否已被其它单元改动（本单只审 t158 的写入集合）。

## 6 verdict

**pass**（带 §5 的明确未覆盖项）。核心两问都有我自己的、字节级或工具代码级的答案：**撤回是读数驱动的**（`Home.tsx` blob 回到 HEAD + `--panel-shadow`/`.ant-card` 的 ring 机制在字节上说得通 + 新规则只打 `.step-num`、没碰共享 `.step-n`），**更正是成立且在范围内**（row 4 数的就是 `strictContainers`，其失败样本 `sel` 就是 `div.step-num`；四个计数同时 −3 = 三个徽章）。**红线全过**（工具未改、`panel/src/**` 只 8 行、未放宽判据），`npm --prefix panel run build` 在最终字节上 **exit 0**。**唯一未覆盖的是执行级复现**（两臂行集合与逐字 criterion/limits），原因与本报告能替代到哪一步都写在 §5，不静默放过。
