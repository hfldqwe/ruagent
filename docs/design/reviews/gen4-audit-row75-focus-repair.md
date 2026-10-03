# 审计行 75 修复 — `#memory` 的 Tab 序把焦点送进不可见元素（t146）

> 来源 = 审计自己给出的判定（首次 CI 运行的工件 `metrics.json`，run `37137554330`）：
> `row 75 · Tab 顺序无陷阱 · verdict=fail · modes=dark · scope=route` —— 前半（40 Tab presses / 29 distinct / max repeat 1）是好的，**红的是后半：焦点落进了一个 rect <1px 的元素**。
> 我的写入集合：`panel/src/views/Memory.tsx`（唯一源码改动）+ 本报告。`panel/src/index.css`、`panel/tools/design-audit.mjs`、`tools/**`、`crates/**`、`.github/**`、`scripts/**` **零字节改动**。
> ⚠️ **我（verify）是本单修复的作者**，所以下面那对红→绿读数是**我自己量的**，不是独立评审：本单仍需一位独立验证/评审者对这批字节给结论（§9）。

## 0 结论

**定位到了那个元素，修掉了，红→绿都是审计自己给的判定原文（不在同一份字节上）。**

| 读数 | 臂 | 行 75 判定 | 可见性证据 |
| --- | --- | --- | --- |
| **红** | 修前字节（worktree @ HEAD，dist 资产 `Memory-hrgVsVE-.js`，sha256 `681821F4FA021008`） | `verdict=fail`，exit 1 | `invisible: ["2:main:invisible(input)"]`，`why: focus lands on an element the user cannot see: 2:main:invisible(input)` |
| **绿** | 修后最终字节（`Memory.tsx` sha256 `993CDF82A79D1B7D` → dist `Memory-TSxncrT3.js`，sha256 `9D5787768615C67D`） | `verdict=pass`，exit 0 | `invisible: []`，`why: null`；`tabs=40, distinct=29, maxRepeat=1`（与红臂逐字相同） |

**那个元素 = antd 6.6.3 `Segmented` 自己的单选 `input.ant-segmented-item-input`，rect `[0, 0]`**（宽 0 × 高 0 ⇒ <1px），`#memory`/dark 上有**两个**：tab 切换器（浏览/画像/观察/流程/经验）与 store 切换器（画像/观察/流程/经验）各一个。

**修法 = 让它可见（不是删掉它）**：把该 input **铺满它自己的 item**（`.memory-view .ant-segmented-item-input { inset:0; width:100%; height:100% }`）。它必须留下：**它就是键盘操作的那个 radio**（焦点落点 + 方向键换选），而 antd 的 `Segmented` 只暴露 `root/icon/label/item` 四个语义槽、**没有 `input` 槽**，所以 `tabIndex={-1}` 从 API 上不可达；面板的可见焦点环本来画在 label 上（`index.css:1862`）。修后实测：**rect `[52,28]`（== item 的 rect）**、焦点仍落在同一个 input、方向键仍换选、label 的环仍在、点击仍选中、`pointer-events` 仍是 `none`。

**判据逐字未动**：两份 `metrics.json` 里行 75 的 `criterion` 文本与 `limits: {"maxRepeat": 5}` **完全相同**，`panel/tools/design-audit.mjs`（sha256 `E3B7E45B59FA1C49`，mtime 2026-09-29）**一个字节没改**；额外用 `#home`/`#wiki` 做了对照：两臂行 75 都 PASS，且**其它失败行集合逐行相同**（4/7/13/38）⇒ 修复没有把别的路由动到。

## 1 仪器与字节（每个读数挂在哪份字节上）

| 项 | 值 |
| --- | --- |
| 审计工具 | `panel/tools/design-audit.mjs`（544,462 B，sha256 `E3B7E45B59FA1C49…`，mtime 2026-09-29T03:51:49）；**只读使用，未改** |
| 行 75 的判据实现 | `probeAxis1`（`:2816-2905`）走 40 次 Tab，`visible = rect.w >= 1 && rect.h >= 1 && display != none && visibility != hidden`（`:2879-2881`）；`row75FocusTrap`（`:5691-5721`）`ok = !stuck && invisible.length === 0`，`stuck = distinct < 2 || maxRepeat >= limit`，`limit` 读契约（`{maxRepeat: 5}`） |
| 调用方式（我用的） | `node panel/tools/design-audit.mjs --check --routes=memory --modes=dark --tabs=40 --no-shots --no-pixels --allow-not-measured --base-url=http://127.0.0.1:<port> --out=<绝对临时目录>`；`--out` 必须在仓外（工具自己拒绝仓内目录，我全程仓外） |
| 我自己的守护进程 | 私建 `ruagent.exe`（`cargo-team.ps1 rustc -p ruagent --bin ruagent -- -o %TEMP%\t146\bin\ruagent.exe`，exit 0 / 49.2 s / sha256 `C7718CCE9685554F…`）+ **临时 root**（`%TEMP%\t146\root`、`root-pre`）+ **临时端口 8841（修后臂）/ 8843（修前对照臂）**，`RUAGENT_PANEL_DIST` 分别指向仓内 `panel/dist` 与 worktree 的 `panel/dist`；活守护进程 8787 = pid 14944 **未触碰**（C32） |
| 哪个 dist 在线上 | 8843：`/assets/Memory-hrgVsVE-.js` → 200（21,888 B），`Memory-egbYOLTq.js` → 1139 B（index 回退，即不存在）；8841：`Memory-egbYOLTq.js` → 200（22,034 B），`hrgVsVE-` → 1139 B ⇒ 两臂各自供的是自己的字节 |
| 修前对照臂怎么来的 | `git worktree add --detach %TEMP%\t146-prefix HEAD`（HEAD 未含我的改动）+ `mklink /J` 复用仓内 `panel/node_modules`（**没有 `npm ci`**）+ `node scripts/build-panel.mjs`（exit 0）⇒ 真正的**修前 dist** |
| 我自己的复核工具（不是证据来源，只用于定位） | `%TEMP%\t146-probe.mjs`（复刻 `probeAxis1` 的 40 次 Tab 走查，打印每个落点的 DOM 路径/rect/computed style）、`%TEMP%\t146-experiment.mjs`（键盘/焦点环/点击的行为对照） |

## 2 定位：那个元素是谁，rect 是多少（**实测，不是猜**）

修前臂（8841 早在修前就量过一次，同一份字节）的 `probeAxis1` 复刻读数：

```
WALK: 40 Tab presses, 37 distinct element(s), max repeat 1 on ""
     left main / invisible: 23 outside, 2 invisible
     sample (audit format): 1:outside(a), 2:outside(ul), 3:outside(a), …
ALL INVISIBLE FOCUS STOPS (2):
  <input> rect=[0,0] class="ant-segmented-item-input"
    path = main.ant-layout-content.content > div > div.view-bar > div.ant-segmented
           > div.ant-segmented-group > label.ant-segmented-item.ant-segmented-item-selected
           > input.ant-segmented-item-input
  <input> rect=[0,0] class="ant-segmented-item-input"
    path = main.ant-layout-content.content > div > section.zone > div.filter-bar > div.ant-segmented
           > div.ant-segmented-group > label.ant-segmented-item… > input.ant-segmented-item-input
```

计算样式（同一次读数）：`position: absolute`、`display: inline-block`、`visibility: visible`、`opacity: 0`、`pointer-events: none`，**`width: 0; height: 0`** ⇒ 正是审计判据 `rect >=1px` 排除的那一类。它的 `label.ant-segmented-item` 是 `position: relative`（52×28 / 80×28），input 却没有尺寸。

审计自己的取样把同一个事实写在 `sample` 里：**`2:main:invisible(input)`**（第 2 次 Tab，落点是 `main` 里的 `input`）—— 与任务里引的那条更早读数**逐字一致**。

**这就是 antd 自己写的**（`panel/node_modules/antd/es/segmented/style/index.js:143-151`，`&-input`）：
`{ position: 'absolute', insetBlockStart: 0, insetInlineStart: 0, width: 0, height: 0, opacity: 0, pointerEvents: 'none' }` —— antd 故意让 input 不参与布局。

## 3 修法：保住它存在的理由

**它为什么存在（行为上必需）**：它就是 `Segmented` 的 radio，是**键盘真正的落点**；实测（修后字节）焦点停在 `input.ant-segmented-item-input` 上按 `ArrowRight`，**选择发生变化**（`selected` 从 `["浏览","观察"]` 变为 `["激进"]`）⇒ 删掉它/让它不可聚焦 = 拆掉这个控件的键盘能力。而面板的**可见**焦点指示是画在 label 上的（`panel/src/index.css:1862-1865`：`.ant-segmented-item:has(.ant-segmented-item-input:focus-visible) { outline: 2px solid var(--focus-ring); outline-offset: 2px }`）—— 实测焦点在该 input 上时 label 的 `outline` = `2px solid rgb(245, 180, 87)`、`labelMatchesHas: true`。

**为什么不能 `tabIndex={-1}`**：antd 6.6.3 的 `Segmented` 语义槽只有 `root/icon/label/item`（`node_modules/antd/es/segmented/index.d.ts:8-21`），**没有 `input`** —— 从组件 API 上根本够不到那个 input。所以「移出 Tab 序」在本组件上不可实现，只能走「让它可见」这一半（契约给的另一半）。

**改法（`panel/src/views/Memory.tsx`，唯一源码改动）**：
```tsx
const SEGMENTED_INPUT_BOX = `
.memory-view .ant-segmented-item-input {
  inset: 0;
  width: 100%;
  height: 100%;
}
`;
// …view 根节点：
<div className="memory-view">
  <style>{SEGMENTED_INPUT_BOX}</style>
```
- **为什么是 CSS 而不是内联 style**：内联 `style` 只能落在 React 自己渲染的元素上，而那个 input 是 antd 内部渲染的；`Segmented` 也没有 `input` 语义槽（同上）。`panel/src/index.css` 不在我的 inScope（AGENTS 也要求「不要 reformat/改动你没动的文件」），所以规则随视图走、**选择器限定 `.memory-view`**。
- **为什么 `inset:0` 而不是只写 `width/height`**：item 已经是 `position: relative`（实测），`inset:0` 让 input 精确铺满它的 item，语义上「可聚焦的控件占据它的控件盒」，而不是铺到某个更远的定位祖先。
- **行为不变的实测（修后最终字节）**：焦点仍落在同一个 `input.ant-segmented-item-input`（rect `[52,28]` == `labelRect`）；方向键仍换选；label 的焦点环仍在；点击 label 仍选中（`{"clicked":"浏览"}` 后 selected 含 `浏览`）；`pointer-events` 仍是 `none`（点击穿透不变）。
- **对审计其它口径的影响**：`<style>` 元素没有 `style` 属性（行 12 数的是**属性**，`design-audit.mjs:4344`），也不是可聚焦元素（不进 Tab 序）；审计把作者 stylesheet 只当作**来源分类/证据**（`:800-807`、`:1518`），没有任何行对 `cssom.authorSheets` 判阈值（我 grep 过，只有 `:1518` 一处记录）。

## 4 红 → 绿（两边的判定原文都是审计自己给的）

**红（修前字节，对照臂 8843，`--routes=memory --modes=dark --tabs=40`）**，输出表格行（逐字）：
```
| 75 | FAIL | Tab 顺序无陷阱 | 焦点不得卡住：走过 >=2 个不同元素、同一元素连续 <5 次，且落点元素可见（rect >=1px） | memory/dark=40 Tab presses, 29 distinct element(s), max repeat 1 on "div#.ant-segmented.css-u6izyw:激进保守@1" (30 left the main subtree -- information, not a failure) |
```
`metrics.json` 的 `checks[75]`（逐字，节选）：`"verdict": "fail"`, `"failures": ["memory/dark=40 Tab presses, 29 distinct element(s), max repeat 1 on \"div#.ant-segmented.css-u6izyw:激进保守@1\" (30 left the main subtree -- information, not a failure)"]`, evidence:
```json
{"route":"memory","mode":"dark","pass":false,
 "display":"40 Tab presses, 29 distinct element(s), max repeat 1 on \"div#.ant-segmented.css-u6izyw:激进保守@1\" (30 left the main subtree -- information, not a failure)",
 "note":"focus lands on an element the user cannot see: 2:main:invisible(input)",
 "detail":{"measured":true,"pass":false,"tabs":40,"escapes":30,"bodyStreak":1,"bodyHits":2,"distinct":29,"maxRepeat":1,
           "topKey":"div#.ant-segmented.css-u6izyw:激进保守@1","invisible":["2:main:invisible(input)"],
           "sample":["2:main:invisible(input)","4:body(body)","5:outside(a)","6:outside(ul)","7:outside(a)"],
           "why":"focus lands on an element the user cannot see: 2:main:invisible(input)"}}
```
⇒ **它把任务里引用的那条 CI 失败原文复现得逐字一致**（40/29/1、`激进保守@1`、`30 left the main subtree`），而且 `tabs/distinct/maxRepeat` 三个计数与 CI 相同。exit=1。

**绿（修后最终字节，8841，gate 重建后的 dist）**，输出表格行（逐字）：
```
| 75 | PASS | Tab 顺序无陷阱 | 焦点不得卡住：走过 >=2 个不同元素、同一元素连续 <5 次，且落点元素可见（rect >=1px） |  |
```
`metrics.json`：`"verdict": "pass"`、`"failures": []`，evidence：
```json
{"route":"memory","mode":"dark","pass":true,
 "display":"40 Tab presses, 29 distinct element(s), max repeat 1 (27 left the main subtree -- information, not a failure)",
 "detail":{"measured":true,"pass":true,"tabs":40,"escapes":27,"bodyStreak":1,"bodyHits":2,"distinct":29,"maxRepeat":1,
           "topKey":"div#.ant-segmented.css-u6izyw:激进保守@1","invisible":[],"sample":["4:body(body)","5:outside(a)","6:outside(ul)","7:outside(a)","8:outside(a)"],
           "why":null}}
```
⇒ **`invisible` 从 `["2:main:invisible(input)"]` 变成 `[]`，`why` 从那条原因变成 `null`，而 `tabs=40 / distinct=29 / maxRepeat=1` 一字未变**（修的是**可见性**那一半，不是「不卡住」那一半）。exit=0。

（同一次修复还有一份更早的绿读数：gate 之前构建的 `Memory-egbYOLTq.js`（22,034 B，sha256 `835B040A2CB0C72F`）上 `verdict=pass`、`invisible=[]`、计数同上。按 T20/C20，**最终字节的那份以 `Memory-TSxncrT3.js`（`9D5787768615C67D`）为准**，两份只有 bundle 哈希不同、源码 sha256 都是 `993CDF82A79D1B7D`。）

## 5 判据没有被削弱 + 回归对照

- **判据/阈值逐字不动**：两份 `metrics.json` 里行 75 的 `criterion` 字符串**完全相同**（含三条口径与「rect >=1px · display != none · visibility != hidden」与空集语义的实现），`limits` 都是 `{"maxRepeat": 5}`。
- **工具未改**：`panel/tools/design-audit.mjs` sha256 `E3B7E45B59FA1C49…`、mtime 2026-09-29T03:51:49，`git status --porcelain` 对该文件为空 ⇒ **不需要改工具就能收口**（若我当时判断必须改工具，会按契约明写配方交 captain 另立单；不需要）。
- **其它路由的对照（同一台仪器，两臂各跑一次）**，`--routes=home,wiki --modes=dark --tabs=40`：
  - 行 75：**两臂都 PASS**（`| 75 | PASS | … | |`）；
  - 两臂的**失败行集合逐行相同**：`4`（`home/dark=3 / ≤1`）、`7`（`home/dark=24px / ≥32px`）、`13`（`home/dark=1 (WCAG口径 1)`）、`38`（`home/dark=3 … div.step-num`）—— 连证据文本都一样 ⇒ 我的改动**没有把任何别的路由的读数动到**。
  - 原理上也一致：选择器前缀是 `.memory-view`，而该类只加在 Memory 视图根节点上。

## 6 面板门禁（我本人跑）

- `npm --prefix panel run build`（仓根）⇒ **exit 0**（修前一次、最终一次；内含 `e2e/i18n-check.mjs` + `tsc -b` + `tsc -p e2e/tsconfig.json --noEmit` + vite：`✓ built in 6.57s` / `build-panel: dist updated (55 assets, index.html swapped by rename)`）。
- **没有新增/修改任何 e2e spec** ⇒ 按契约不必（也不应）跑 `npm run test:e2e`；**没有**用裸 `npx playwright test`。行 75 的哨兵在 CI（`metrics.json` 来自 run `37137554330`，`panel/tools/design-audit.mjs --check` 由 `.github/workflows/audit.yml` 驱动，该文件现在是已提交状态 `01b9021`），所以我没有再写一份重复的 e2e 断言。

## 7 未覆盖什么（第 19 条）

1. **其余 11 条路由**的 Tab 序没有全跑：我只跑了 `memory`（修复目标）与 `home,wiki`（对照）。选择器作用域论证 + 这次对照说明它们不受影响，但**不等于**它们各自没有别的不可见落点。
2. **light 模式**：行 75 的 `modes` 就是 `["dark"]`（工具行定义 `:5593`），**light 下这一行本来就不跑** ⇒ light 的 Tab 序在本单**未测**。
3. **其它视图的同类控件**：`Chat.tsx`/`Agents.tsx`/`Graph.tsx` 等视图里的 `Segmented`（若有）仍是 antd 的 0×0 input；**我的修复是视图作用域的，不覆盖它们**。它们是否红，取决于各自路由的行 75 读数（本次未跑全量）。
4. **浮层（portal）里的 Segmented**：`Modal` 渲染到 `body`，不在 `.memory-view` 子树内 ⇒ **portal 内的同类 input 不会被这条规则覆盖**。本次 Memory 视图的三个 `Segmented`（tab 切换 / store 切换 / recall 激进保守）都在视图子树内，所以目标达成；换到别的视图要单独确认。
5. **真实屏幕阅读器**（NVDA/JAWS/VoiceOver）未跑：我改的是几何盒，**无障碍树没有变化**（input 仍是 `type=radio`，仍在 label 内），但我没有用读屏实测「用户听到什么」。
6. **像素/截图行**：两次审计都带 `--no-shots --no-pixels`，并用 `--allow-not-measured` 让若干 `not_measured` 行（工具会**逐行点名**，如 76/77「no element of the named toggle set is on this route」）不阻断；这些行的 PASS/FAIL 不在本单范围内。
7. **全量 `--check`（13 路由 × 两模式）** 未跑（耗时长，且本单目标是单行收口）。
8. **CI 工件本身**：我读的是任务引用的 CI 失败原文 + **我自己两臂的读数**；我没有重跑 CI job（那是 `.github/**`，不在我的 inScope）。

## 8 过程、收尾与残留

- **负控/对照没有污染共享树**：修前臂来自 `git worktree`（HEAD，未含我的改动），`node_modules` 用 **junction 复用**（没有 `npm ci`）；收尾先 `cmd /c rmdir <wt>\panel\node_modules`（对 junction 只删链接）→ `git worktree remove --force`；**仓内 `panel/node_modules/playwright` 事后仍在**（已验证），`git worktree list` 只剩主树。
- **我的进程**：两个临时 daemon（pid **21536** 端口 8841 / pid **33880** 端口 8843）按记录 PID 停掉，事后 `alive=False`；**浏览器**由脚本自行 `browser.close()`，并按**我自己的临时路径**（`*t146*`）扫过 `chrome.exe/msedge.exe/node.exe` ⇒ 没有我的孤儿（唯一命中是我自己的 DSH node 进程，命令行里含该路径）。**没有**按进程名/端口批量杀。**8787 的常驻 pid 14944 未触碰**（health 200，StartTime 2026/10/2 23:05:45）。
- **仓内改动面**：`git status --porcelain -- panel/` = **仅** ` M panel/src/views/Memory.tsx`（+ 本报告）。没有 `docs/screenshots` 残留（`--out` 全程指向仓外，工具自己也拒绝仓内目录）。
- **临时文件**：清掉 2.26 GB（328 MB 私建 exe + 2 GB `.pdb` + 两个 root 的库/日志）；保留 5 份 `metrics.json`（`%TEMP%\t146\audit-{pre,post,final,other-pre,other-post}\metrics.json`，共 1,945,372 B）与本轮日志（21 个文件 / 529,336 B）作为证据。C: 可用 **71.14 GB**。
- **字节归属**：`panel/src/views/Memory.tsx`（修后）sha256 `993CDF82A79D1B7D…`，mtime 2026-10-04T01:40:22；修前同文件 sha256 `B2BA6901D9E76554…`（2026-10-04T00:33:04）与 worktree 检出（LF）共享同一文本（`git diff HEAD` 只有我这 29+/1- 一处 diff）。注意：**工作副本是 CRLF、worktree 检出是 LF**，所以同一文本在两个树里的文件级 sha256 不同（`54385BA5…` vs `B2BA6901…`），我以「`git diff HEAD` 只有我的改动」为准。

## 9 声明：作者不是评审

我是本单修复的作者，上面的红→绿是我用审计工具自己量的**读数**，不是一次独立验证。按队内纪律，**这批字节还需要一位独立验证者**：重跑行 75（修前/修后）并对"修法是否保住该元素存在的理由"给结论。可复现配方（我用的原命令，供下一位直接抄）：

```powershell
# 1) 两个臂：修后臂 = 仓内 panel/dist（我的字节）；修前臂 = git worktree @ HEAD + junction node_modules + node scripts/build-panel.mjs
# 2) 各自被一个自己的临时 daemon 服务（临时 root/端口，RUAGENT_PANEL_DIST 指向对应 dist）
node panel/tools/design-audit.mjs --check --routes=memory --modes=dark --tabs=40 `
  --no-shots --no-pixels --allow-not-measured `
  --base-url=http://127.0.0.1:<port> --out=<仓外绝对目录>
# 3) 读 <out>/metrics.json 的 checks[75]：verdict / failures / evidence[0].detail.{tabs,distinct,maxRepeat,invisible,why}
```
