# t306 裁 t210 的行 52：触控判据量元素盒还是视口内可见区 —— **needs_revision**（判据未改，附两条会改变结论的读数）

任务：裁 t210 的行 52（`最小有效 44×41px（元素盒 44×44，命中 81/81）· 有效区低于 44px 的 2 个`），若判为口径问题就改判据 + 另设一行「完全在视口内」。
inScope：panel/tools/ · docs/design/reviews/。**我没有改 panel/tools/ 的任何文件** —— 原因见 ③，不是没做完就停手，是**先测出改的前提不成立**。

## ① 读数（先给）：那 2 个元素

来源 = 审计自己的证据文本（`node tools/design-audit.mjs --routes=sessions --check`，新 chunk `index-DQtMkr-V.js`）：

| 元素（选择器） | 元素盒 | 视口内可见区 | 被裁的维 | 命中采样 | visPct |
| --- | --- | --- | --- | --- | --- |
| `button.icon-btn@390` | 44×44 | **44×41** | **高（3px）** | 81/81 | 93% |
| `button.ant-dropdown-trigger.icon-btn@390` | 44×44 | **44×41** | **高（3px）** | 81/81 | 93% |

代码里的口径也写着同一件事（`tools/design-audit.mjs` 约 2492-2500 行，t232 登记）：
> the effective box IS the clamped sampling region (x1-x0 by y1-y0), NOT the element's box. t232: a 44x44 target 93% on screen reported '44x41' beside 'box 44x44' … **The 3px is below the fold -- a fact about the SCROLL POSITION, not about whether a finger can reach the target.**

⇒ 这两个元素**不是「小目标」**：盒 44×44、盒内 81 个采样点全部命中、缺的 3px 在视口下沿之外。

## ② 判定（理由）

**判为口径问题 —— 但这条判据不能只改一半。** WCAG 2.5.8 量的是**目标自身的尺寸**，不是「它在视口里露出来多少」；把「露出来多少」算作触控失败，等于把**滚动位置**报成**触控缺陷**。所以：

1. **行 52 的判定对象改成元素盒**（`boxW/boxH ≥ 44`），并把「部分在视口外」这一事实交给**新的一行**去报（这正是任务要求的「另设一行」）。
2. **但不能无条件放宽**：同一份代码里 `tools/design-audit.mjs` 的既有自检把另一种形态钉成 **must-FAIL**（`:7377` `row52 must-FAIL (t158 shape): element box 44x44 but hit region 13x36`，`:7379` `reported as the box-passes/effective-fails form, not as a plain small box`）—— t158 的形态是**盒达标而命中区真的小**（锚点只裹文字、被 `overflow:hidden` 裁到约 13.5×36），那**必须继续报红**。
   ⇒ 两种形态在数据上**已经可分**：探针同时带 `allHit`（盒内采样点是否全命中）与 `visPct`。
   **正确的最小改动**：`allHit === true`（缺的只是视口裁剪）⇒ 按**元素盒**判 + 把 `visPct < 100` 交给新行；`allHit === false`（盒内有采样点打不中 ⇒ 被祖先裁剪/被覆盖）⇒ **仍按有效命中区判，保持 t158 必须报红**。这一条同时就是任务里要的「反向（防把判据改宽）」。

## ③ 为什么我没有动 panel/tools/（两条会改变结论的实测/结构发现）

**F-t306-01（medium）行 52 的「那 2 个」不可独立复现 —— 证据里没有采样位置。**
我按审计自己的谓词（可见比例 ≥0.9 ∧ 盒 ≥44 ∧ 某一维可见 <44）在 390×844、`?mode=dark#sessions`、等待 2.6s 后**独立重测**：
```
{"viewport":[390,844],"offenders":[]}     ← 零个
```
同一路由、同一视口宽度，**一个都测不到** ⇒ 那 2 个只出现在**审计自己的滚动/采样位置**下。而证据文本里**没有记录采样时的滚动位置**（只有 `@390` 与 93%）。⇒ 在这个前提下改判据，等于把一个**无法独立复现的读数**固化进判据：改前/改后的对照也就不可信（同一条命令、两次运行可能给出不同的 offender 集合）。
**requiredFix**：行 52 的证据必须带上采样位置（`scrollY` 与目标中心相对视口的位置），或改为在**固定滚动锚点**下采样（例如「把目标滚到视口中线再测」）——先让读数可复现，再改判据。

**F-t306-02（medium）「另设一行」与 `--check` 的契约对账相撞 —— 新增行需要契约里也有一行，而契约不在本单 inScope。**
审计用 `--check` 把**工具的行表**与**契约的行**对账（`resolveContract` 读契约，见 `tools/design-audit.mjs:315-392`；`--self-test` 里就有 `reconcile: tool=59 contract=71` 这类读数）。新行的**判据文本**按惯例落在契约（MASTER §12）里 —— 那是 `docs/design/MASTER.md`，**本单 inScope 之外**（`panel/tools/` · `docs/design/reviews/`）。
**requiredFix**：要么本单 inScope 加上契约文件（或由 contract-lead 先补一行），要么把「完全在视口内」做成**行 52 证据里的一个计数**（不新增行）——请队长裁决走哪条。

## ④ 结论：**needs_revision**

* ① 读数已给（盒 / 可见区 / 被裁的维 = **高 3px** / 选择器）。
* ② 判定已给（量元素盒；**但只在 `allHit === true` 时**放宽，t158 的 `allHit === false` 形态必须继续报红 —— 这就是反向闸门）。
* ③ 判据**未改**：两个前置条件（读数可复现、新增行的归属）不满足，先改会把不可复现的读数固化进判据、并让 `--check` 对账失去意义。
* 反向读数（任务第 4 条）**当前无法给**：本单没改判据，就没有「改后仍报红」的对照；而**既有自检已经钉住**该方向（`:7370` 32×32 must-FAIL · `:7377` t158 形态 must-FAIL · `:7386` 全出血 ::before 28×18 must-FAIL）—— 改动落地时必须让这三条继续绿。

**下一步（机械可执行）**：① 先给行 52 加采样位置并复现那 2 个（F-t306-01）→ ② 把 `row52` 判定改为 `allHit ? box : eff`（F-t306-02 定下新行归属）→ ③ 跑 `--routes=sessions --check` 给改前/改后行 52 读数 → ④ 跑 `--self-test` 证明 32×32 / t158 / ::before 三条 must-FAIL 仍绿。

## ⑤ 纪律

**未改 panel/tools/ 或 panel/src/ 任何文件**（只读 + 仓外探针）· 探针在 `C:/tmp/t306/`（已删）· 未启停守护进程（本单不需要 8787）· 未调 `/api/v1/recall` · node 直调不用 npx · 只 add 本报告 · 未 push。
