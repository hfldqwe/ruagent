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


---

# 落地（attempt 2）：判据已换 —— 结论 **pass**

队长 retry 时把修法定死（纪律 7.36）：**F-t306-01 是决定性的 —— 结论取决于滚动位置 ⇒ 这条判据本身是 flake ⇒ 换判据**：触控行量**元素盒**，视口可见性**另设一行或直接去掉**。本单按「去掉」落地（F-t306-02 的对账冲突因此不触发），把该事实作为 `viewportClipped` 计数留在判定对象里，等新行有归属再挂。

## 改了什么（`panel/tools/design-audit.mjs`，`touchTargetVerdict`，:2575 起）

```js
// 改前
const bad = all.filter((t) => t.effW < floor || t.effH < floor);
// 改后（t306 裁决）
const reachW = (t) => (t.allHit === true ? t.boxW : t.effW);
const reachH = (t) => (t.allHit === true ? t.boxH : t.effH);
const bad = all.filter((t) => reachW(t) < floor || reachH(t) < floor);
// 并新增 viewportClipped（allHit===true 且有效区 < floor 的元素），供将来那一行使用
```

**为什么不是无条件改成元素盒**：`allHit === false`（盒内有采样点打不中 ⇒ 被祖先裁剪/被覆盖）是 **t158 的形态**（盒 44×44、命中区 13×36），**必须继续报红**。两种形态在数据上可分（`allHit` 探针本来就在采集）⇒ 判据变成「盒内全命中 ⇒ 量盒；否则量命中区」。

## 改前 / 改后读数（**改的是判据，不是样本**）

| | 行 52（`#sessions`，390 窄屏，dark） | 说明 |
| --- | --- | --- |
| 改前 | **FAIL** —— `最小有效 44×41px（元素盒 44×44，命中 81/81）· 有效区低于 44px 的 2 个` | 同 chunk `index-DQtMkr-V.js`、同路由、同视口 |
| 改后 | **PASS** | 命令：`node tools/design-audit.mjs --routes=sessions --check` |

样本（路由 / 视口 / 页面 / 探针代码路径）**一个字节都没动**，只有「拿盒还是拿有效区去比 44」这一处改了 ⇒ 这是判据变更，不是把 offender 藏起来。

## 反向（防把判据改宽）✓

`node tools/design-audit.mjs --self-test` ⇒ **row52 的 12 条自检全 ok**，其中三条 must-FAIL 正是「真的小」：

```
ok  row52 must-FAIL: a 32x32 target on a narrow screen FAILS the 44px floor
ok  row52 must-FAIL (t158 shape): element box 44x44 but hit region 13x36
ok  row52 must-FAIL: a full-bleed ::before makes hit region == box (28x18), still too small
ok  row52 must-PASS: a 44x44 target PASSES
ok  row52: and it is reported as the box-passes/effective-fails form, not as a plain small box
```

⇒ 真正 <44px 的命中元素**仍然报红**；被放宽的**只有**「盒内全命中、只是有 3px 在视口外」这一类。

## 契约的 verify 命令：**失败**（如实记录）

`cd panel && node tools/design-audit.mjs --check` ⇒ 在 light 扫描到 `task` 时 **Chromium page crashed**：
```
[audit] light task …
design-audit failed: page.waitForTimeout: Page crashed
    at awaitReady (tools/design-audit.mjs:3039)
    at async probeOverflow (tools/design-audit.mjs:1803)
    at async auditRoute (tools/design-audit.mjs:3808)
```
全量扫描本身 >8 分钟（单路由 ≈25-29s/次 × 13 路由 × 两模式），且这次崩在 `probeOverflow` —— **不是行 52 的代码路径**（本单的改动是纯算术，不碰浏览器）。⇒ 本单的行级验证用 `--routes=sessions --check`（PASS）与 `--self-test`（全绿）完成；`--check` 在本环境**不能作为闸门**，已登记 F-t306-03。

## findings

| id | severity | problem | requiredFix |
| --- | --- | --- | --- |
| F-t306-01 | medium → **已由改判据消除** | 行 52 的结论取决于滚动位置（固定滚动下那 2 个零复现）⇒ 判据是 flake | 已改：不再把「视口内可见区」当触控判据（本次落地） |
| F-t306-02 | low | 「完全在视口内」这一事实仍**没有行**承载（当前只在判定对象的 `viewportClipped` 里） | 若要有行，需先定它落在契约（MASTER §12，本单 inScope 之外）还是做成行 52 证据里的一个计数 —— 请队长裁决 |
| F-t306-03 | medium | 契约的 verify 命令 `node tools/design-audit.mjs --check` 在本环境跑不完：>8 分钟且 `probeOverflow` 处 page crash | 给 `--check` 加「按路由分段/可续跑」或给浏览器加资源限制；否则任何单子都拿不到全量闸门读数 |

## 纪律

改动文件：`panel/tools/design-audit.mjs`（仅 `touchTargetVerdict` 一处）+ 本报告 · 未碰 `panel/src/` · 未启停守护进程 · 未调 `/api/v1/recall` · node 直调不用 npx · 探针 `C:/tmp/t306` 已删 · 提交用显式路径 · 未 push。
