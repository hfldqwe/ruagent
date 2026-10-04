# 独立验证：审计行 75（Tab 焦点可见性）—— t148 验 t146

> 单号 **t148**（verification）· 成员 `recall` · attempt **1** · 2026-10-04 02:0x–02:4x
> inScope：**本报告**。`panel/**`（含 `panel/tools/**`）、`crates/**`、`.github/**`、`scripts/**` **只读** —— 我为了实验**没有改一行**；`npm --prefix panel run build` 只写 `panel/dist`（gitignore）。
> 被验对象：`t146` 对审计行 75 的修复（唯一源码改动 = `panel/src/views/Memory.tsx` 的 `.memory-view .ant-segmented-item-input { inset:0; width:100%; height:100% }`）+ 报告 `docs/design/reviews/gen4-audit-row75-focus-repair.md`。
> 方法：**两臂都是我自己建的、我自己供的、我自己读的**；作者的读数只作为对照，不作为证据。

## 0 判定：**pass**（修法成立 · 判据未削弱 · 元素理由保住 · 无附带 · 无遗漏）

| 验收 | 我的独立读数 | 结论 |
| --- | --- | --- |
| 两臂各自复跑，读 `checks[75]` | 修前臂 `verdict=fail` / exit 1 / `invisible=["2:main:invisible(input)"]`；修后臂 `verdict=pass` / exit 0 / `invisible=[]`；两臂 `tabs=40 distinct=29 maxRepeat=1` **一字未变** | ✓ 红→绿**我复现了**（§2） |
| 修法保住元素存在的理由 | 我自己的浏览器探针：`ArrowRight` **换选**（`['浏览','观察']→['激进']`，两臂皆然）· focus rect `[0,0]→[52,28]`（= 它自己 label 的盒）· label 焦点环 `2px solid rgb(245,180,87)` 两臂都在 · 点击仍选中 · `pointer-events: none` 未变 | ✓ 四条声称全部成立（§4） |
| 判据没被偷偷放宽 | `checks[75].criterion` **逐字相同**（668 字符）· `limits` 都 `{"maxRepeat":5}` · **全部 65 条**的 criterion+limits 两臂完全相同 · 工具 sha256 未变（两臂用**同一份文件**）· `thresholdsPath` 两次都是仓内 `docs/design/MASTER.md` | ✓ 未削弱（§3） |
| 没动别的路由 | 我另选的一对（`sessions,graph`，dark）：两臂失败行**都是 `[6,7]`，集合与证据逐行相同**；行 75 两臂 PASS。**更强**：我自己 t134 的 CI 工件（pre-fix、**12 路由**）显示行 75 的不可见落点**只出现在 `memory`** | ✓ 无附带，且无遗漏（§5） |
| 门禁 | `npm --prefix panel run build` ⇒ **exit 0 / 15s** | ✓（§6） |

**findings：2 条，均 low，均不否定修复**（F1 = 作者报告 §9 的复现配方已过期；F2 = bundle 哈希不可跨检出比较）。**没有** blocker/high，**没有**「判据被放宽」「行为被削」「别的路由被带动」这一类。

## 1 两臂是我自己建的（并顺手发现作者配方的过期点）

**版本观察（重要）**：修复**已经被合入 main**：`d6ace1b fix(panel): the keyboard can see where focus is on #memory`（只动 `Memory.tsx` +30/−1 与那份报告）。我开工时 HEAD 还是 `8706de1`，后来是 `d6ace1b`，收尾已是 `1476a4b`。
⇒ **作者报告 §9 的配方「修前臂 = `git worktree` @ HEAD」现在会给出两个「修后」臂**（我第一轮就这么中招：worktree 臂的 Memory bundle 也是 22,034 B）。⇒ 真前臂必须钉在 **`d6ace1b^`**（= `8706de1`）。内容级判据（比 commit 名硬）：`SEGMENTED_INPUT_BOX` 在 `d6ace1b^` 出现 **0** 次、在 `d6ace1b` 出现 **3** 次。

| 臂 | 怎么来的 | 产物（我构建） | 它供的字节（我实测） |
| --- | --- | --- | --- |
| **修前 armA** | `git worktree add --detach %TEMP%\t148\armA 8706de1` + `mklink /J panel\node_modules`（复用，**无 `npm ci`**）+ `node scripts/build-panel.mjs` ⇒ exit 0 | `dist/assets/Memory-DAlaFIJS.js` **21,888 B** sha256 `F2A62B0F5E3DC8B0…` | `GET /assets/Memory-DAlaFIJS.js` → **200 / 21,888 B**；取**修后臂**的 bundle 名 → 200 / **1,139 B**（index.html 回退） |
| **修后 armB** | 仓内 `npm --prefix panel run build`（= 契约门禁那一次，exit 0）⇒ 快照到仓外 | `dist/assets/Memory-6hgR27yg.js` **22,034 B** sha256 `283F4A0B02EF4223…` | `GET /assets/Memory-6hgR27yg.js` → **200 / 22,034 B**；取**修前臂**的 bundle 名 → 200 / **1,139 B** |

- 每臂一个**我自己的**临时 daemon：**端口 19003 / 19004** + 各自 **临时 root**（`%TEMP%\t148\rootA`、`rootB`）+ `RUAGENT_PANEL_DIST` 指向各自的 dist；二进制 = `%TEMP%\ruagent-team-target\debug\ruagent.exe`（**臂无关**：修复是纯 panel 侧，两臂共用同一个 daemon 二进制）。**活 8787 / pid 14944 全程未触碰**。
- **工具同一份**：两次审计都由**仓内** `panel/tools/design-audit.mjs` 执行（sha256 `E3B7E45B59FA1C49…`，`git diff` 对该文件为空）。worktree 副本的**原始**哈希不同（`DAC55DEC8D65D45D…`）—— 那只因工作副本是 **CRLF**、worktree 检出是 **LF**；**去掉 `\r` 归一化后两者逐字节相等** ✓，且 `git` 侧两处都 == HEAD ✓。
- 审计命令（两臂同形）：`node panel/tools/design-audit.mjs --check --routes=<r> --modes=dark --tabs=40 --no-shots --no-pixels --allow-not-measured --base-url=http://127.0.0.1:<port> --out=%TEMP%\t148\<name>`；`--out` 全在**仓外**。每次 `checks=65`，`captures` = 1（memory）/ 2（sessions,graph）。

## 2 行 75 的判定原文（我的读数；`#memory` / dark）

```
修前 armA（19003）  audit exit = 1   （36s）
  checks[75].verdict = "fail"
  failures = ["memory/dark=40 Tab presses, 29 distinct element(s), max repeat 1 on \"div#.ant-segmented.css-u6izyw:激进保守@1\" (30 left the main subtree -- information, not a failure)"]
  evidence[0].pass = false
  detail = tabs 40 · distinct 29 · maxRepeat 1 · escapes 30 · bodyHits 2 · topKey "div#.ant-segmented.css-u6izyw:激进保守@1"
           invisible ["2:main:invisible(input)"] · why "focus lands on an element the user cannot see: 2:main:invisible(input)"
           sample ["2:main:invisible(input)","4:body(body)","5:outside(a)","6:outside(ul)","7:outside(a)"]

修后 armB（19004）  audit exit = 0   （35s）
  checks[75].verdict = "pass"
  failures = []
  evidence[0].pass = true
  detail = tabs 40 · distinct 29 · maxRepeat 1 · escapes 27 · bodyHits 2 · topKey 同上
           invisible [] · why null
           sample ["4:body(body)","5:outside(a)","6:outside(ul)","7:outside(a)","8:outside(a)"]
```
- **红→绿由我独立复现**，且三个承重计数 `tabs=40 / distinct=29 / maxRepeat=1`（= CI 与作者两侧的同一组数）**一字未变** ⇒ 修的是**可见性**那一半，不是「不卡住」那一半 ✓。
- 修前臂的失败文本与 **CI run `37137554330`** 的失败文本**逐字相同**（同一句 `…激进保守@1" (30 left the main subtree…)`）✓。
- **一处我不完全解释的差异**（如实记）：`escapes` 30→27、`sample` 首项从 `2:main:invisible(input)` 变成 `4:body(body)`。可见性判据相关的量（`tabs/distinct/maxRepeat/invisible/why`）是自洽的；`escapes` 是工具自己标注为「information, not a failure」的量，我**不**声称理解它为何少 3。

## 3 判据没有被削弱（逐条）

| 检查 | 读数 |
| --- | --- |
| `checks[75].criterion` 逐字比对 | **完全相同**（668 字符；含三条口径与 `rect >=1px · display != none · visibility != hidden` 与空集语义） |
| `checks[75].limits` | 两臂都 `{"maxRepeat": 5}` ✓ |
| **全部 65 条** check 的 criterion + limits | 两臂**完全相同** ✓（不只是行 75） |
| 判据来源 | 四次运行都 `thresholdsFound=true`、`thresholdsPath=…\docs\design\MASTER.md` ✓ |
| 工具是否被动过 | 未动：两臂用**同一份仓内文件** `E3B7E45B59FA1C49…`；`git status` 对该文件为空；worktree 副本经行尾归一化后与之相等 ✓ |
| 作者报告的对照（`home,wiki`） | 我**没有**照抄；我换了我自己的一对（§5），结论同为「无附带」 |

## 4 「元素存在的理由」逐条自验（我自己的 playwright 探针）

探针：`%TEMP%\t148\probe\probe.mjs`（我自己写的，非作者脚本；Playwright 经 `mklink /J` 从仓内 `node_modules` 解析，收尾 `rmdir` 链接）。两臂各跑一次，断言对象是**真实键盘 Tab 落点**：

| 量 | 修前 armA | 修后 armB | 说明 |
| --- | --- | --- | --- |
| `main` 子树里的 `input.ant-segmented-item-input` 数量 | **7** | **7** | 三个 `Segmented`（`Memory.tsx:262/310/1235`） |
| 它们的 rect | 全部 **`[0,0]`** | **`[52,28]`×6 + `[80,28]`×1**（= 各自 label 的盒；labelRect 与之逐项相等） | 修复正是「让它占满自己的 item」 |
| 第一次落在该 input 上的 Tab 序号 | **21** | **21** | **Tab 序未变**、落点仍是同一个 input |
| 该元素 `:focus-visible` | `true` | `true` | 真实键盘焦点 |
| label 的 outline | **`2px solid rgb(245, 180, 87)`** | **同上** | 与 `index.css` 的 `--focus-ring` 一致 |
| `.ant-segmented-item:has(.ant-segmented-item-input:focus-visible)` 命中 | `true` | `true` | 可见焦点环画在 label 上，且仍在 |
| **`ArrowRight` 是否换选** | **`['浏览','观察'] → ['激进']`（changed=true）** | **同一读数（changed=true）** | ⇒ **它确实是键盘操作的 radio**，且修复后仍能操作 |
| 点击一个未选中项是否选中 | `clicked='浏览'`，`['激进'] → ['浏览','观察']`，changed=true | 同上 | 指针行为未变 |
| `pointer-events` / `opacity` / `type` / `tabIndex` | `none` / `0` / `radio` / `0` | **完全相同** | 点击穿透与焦点语义未动 |
| 焦点路径 | `div > div.view-bar > … > label.ant-segmented-item > input…` | `div.memory-view > div.view-bar > …`（多了 `.memory-view`） | 两臂确实是不同字节 |

静态声称（只读核对）：
- antd 版本 **6.6.3**；`node_modules/antd/es/segmented/index.d.ts` 的语义槽**只有** `root / icon / label / item`，全文件**`input` 出现 0 次** ⇒ **`tabIndex={-1}` 从组件 API 不可达** ✓（作者的这条声称成立）。
- `panel/src/index.css` 确有 `.ant-segmented-item:has(.ant-segmented-item-input:focus-visible) { outline: 2px solid var(--focus-ring); outline-offset: 2px }` ✓。
- `memory-view` 在 `panel/src` 里只出现在 `Memory.tsx`（`:51` 的 CSS 串、`:252` 的视图根）⇒ 作用域确实是**这一个视图** ✓。

## 5 没动别的路由 + **没有留下别的路由的同类缺陷**

① **我自己选的一对**（`--routes=sessions,graph --modes=dark`，两臂各一次，~60s/次）：
```
pre  failing rows = [6, 7]   post failing rows = [6, 7]   SAME ROW SET = True
证据逐行相同（含 failures[0] 全文）= True    行 75：pre=pass，post=pass
```
② **全路由的 pre-fix 读数（我自己的 t134 CI 工件，run `37137554330`，12 路由）** —— 这一条比 ①强，因为它覆盖**全部**路由：

| 路由 | 行 75 evidence | 有不可见落点？ |
| --- | --- | --- |
| **memory** | `pass=False`，`invisible=['2:main:invisible(input)']` | **是（唯一）** |
| home / chat / sessions / board / knowledge / graph / agents / runtimes / settings | 全部 `pass=True`，`invisible=[]`，`tabs=40`，`maxRepeat=1` | 否 |
| stats / inbox | `pass=None`（该行在这两条路由上 `not_measured`） | 未测 |

⇒ **行 75 的不可见落点今天只出现在 `memory`**，而修复正好作用在 `Memory` 视图 ⇒ **`.memory-view` 的作用域覆盖了唯一的实例**。别的视图**也**用 `<Segmented>`（`Board.tsx:176`、`Graph.tsx:453`、`Knowledge.tsx:294`、`SessionsView.tsx:455`、`TaskDetail.tsx:548`、`Wiki.tsx:422`）却都 PASS ⇒ 作者声明的「作用域限制」在今天是一个**登记**（未来给别的视图加 Segmented 时，行 75 会在那条路由上红 —— 门禁自己会喊），**不是留下的缺陷** ✓。

## 6 门禁（我自己跑）

```
npm --prefix panel run build   ⇒  exit 0（15s）
  i18n dictionary integrity — panel/src/i18n.tsx
  panel/src/i18n.tsx:50 t(...)
  keys with no string literal anywhere outside i18n.tsx: 87 (of which 40 are also outside every dynamic family prefix)
  vite v6.4.3 building for production...
  ✓ built in 4.35s
  build-panel: dist updated (55 assets, index.html swapped by rename)
```
未新增/修改任何 e2e spec ⇒ 按契约不跑 `npm run test:e2e`（也**未**用裸 `npx playwright test`）。

## 7 Findings

| id | 严重度 | 问题 | requiredFix | 坐标 |
| --- | --- | --- | --- | --- |
| **RV75-1** | low | **复现配方已过期**：报告 §9 让下一位用 `git worktree` @ **HEAD** 当「修前臂」。修复合入 `d6ace1b` 之后照抄会得到**两个修后臂**（我第一轮实测：worktree 臂 bundle 也是 22,034 B），于是「红→绿」无法复现，且很容易被误读成「本来就绿」。 | 配方改成钉 commit：`git worktree add --detach <path> d6ace1b^`（或写明「HEAD 必须早于本修复的提交」），并保留内容级判据「`SEGMENTED_INPUT_BOX` 出现 0 次」。 | `docs/design/reviews/gen4-audit-row75-focus-repair.md:156`（该报告已合入） |
| **RV75-2** | low | **bundle 哈希不可跨检出比较**：同一份源码文本下，LF 检出（worktree/armA）与 CRLF 工作副本（仓内/armB）的 vite 产物**尺寸相同、哈希不同**（作者的 `Memory-hrgVsVE-.js` / `Memory-TSxncrT3.js` vs 我的 `Memory-DAlaFIJS.js` / `Memory-6hgR27yg.js`）。用 bundle 名/哈希当「哪一臂」的标识是脆的。 | 在配方里加一句：**臂的可靠标识 = 源码文本（`SEGMENTED_INPUT_BOX` 有/无）+ 审计自己的 `checks[75]`**；bundle 哈希随行尾变化，只用来核「这一臂供的是自己的字节」。 | 同上报告 §0 表、§9 |

**没有**发现：判据被放宽、去掉了任何三道口径、`limits` 被调大、工具被改、别的路由被带动、元素存在理由被削（键盘/指针/焦点环/点击四项实测都在）。

## 8 我没有覆盖什么（第 19 条）

1. **其余 11 条路由的 Tab 序**我**没有**两臂各跑一遍（只跑了 `memory` 与 `sessions,graph`）；但 §5 ② 用的是**全 12 路由**的 pre-fix CI 工件，所以「还有没有别的路由有同类缺陷」这一问有全量读数（结论：只有 memory；`stats`/`inbox` 该行 `not_measured`）。
2. **light 模式未测**：行 75 的 `modes` 就是 `["dark"]`（工具行定义），light 下这行不跑。
3. **真实屏幕阅读器**（NVDA/JAWS/VoiceOver）**未测**：我量的是几何盒 + 焦点/键盘/指针行为；**无障碍树是否变化我没有读**。
4. **portal/Modal 里的 Segmented 未测**：它们渲染到 `body`，不在 `.memory-view` 子树内，本规则不覆盖（今天没有实例落进行 75 的采样）。
5. **`stats` / `inbox` 两路由的该行**未测（工具判 `not_measured`）。
6. **未跑 e2e**（未新增 spec）；**未重跑 CI**（`.github/**` 不在 inScope）。
7. **不是同一份字节上的 A/B**：两臂是两份构建产物（`armA=8706de1`、`armB=d6ace1b` 的 panel 源），这与契约要求一致，但严格说「红→绿」是**两次构建之间**的对比，不是同一进程内的重复测量。
8. 我**没有**验证作者报告的其它章节（例如它对 antd 源码行号 `style/index.js:143-151` 的引用）；我只核了与本次判定有关的静止事实（§4 的那些）。

## 9 纪律回执 + 证据哈希

- **写入集合 = 本报告**。`panel/**`、`crates/**`、`.github/**`、`scripts/**` **零改动**（只读；`panel/dist` 由门禁构建写出，gitignore）。
- **证据（保留在 `%TEMP%\t148`，145 文件 / 37 MB）**：`audit-pre/metrics.json` sha256 `1981BF99BCD0D0E7…` · `audit-post/metrics.json` `EC4F46D1D8B10B2D…` · `other-pre/metrics.json` `CE0E8AF53CF3A1EE…` · `other-post/metrics.json` `1C5C6427AC78C4B9…` · `probe2-pre.json` `35ED5EC2CB301DDB…` · `probe2-post.json` `9E1D6FD53B0DA47B…` · 两臂 dist 快照 · 构建/审计日志。
- **我自己的进程**：临时 daemon 全部按**自记 PID** 停掉并核对 `alive=False`（33144 / 8904 / 24244 / 10872 / …），**临时 root 全删**；worktree 收尾**先 `cmd /c rmdir` 掉 junction 再 `git worktree remove --force`**（C39），事后 `git worktree list` 只剩主树 + **队友的** `t151-prefix`（未动），仓内 `panel/node_modules/playwright` 仍在 ✓。
- **没有按进程名或端口批量杀**：期间出现过两个 build 步 node 进程（pid 10856 `scripts/build-panel.mjs`、12812 `tsc -p e2e/…`），我**无法确证归属**（可能是我一个被进程终止的 `Start-Job` 的遗留，也可能是队友同时在建面板）⇒ **未杀**，收尾复查两者**已自行退出**。★ 如实记：我第一次跑门禁用了 `Start-Job`，该 job 随调用进程一起死掉、读数丢失（因此 §6 的读数是**第二次**、在同一进程内跑完的那次）。
- `NO_PROXY=127.0.0.1,localhost,::1`；`--base-url` 全程显式指向**我自己**的临时端口；**活 8787 / pid 14944 未触碰**（多次核对 `alive=True`）。
- **观察（不是我的改动，但会进别人读数的上下文）**：`panel/src/views/Settings.tsx` 在我收尾时是脏的（队友在途）—— 我的 `armB` 快照取自**它干净时**的 `d6ace1b` 树，故不受影响；`HEAD` 现为 `1476a4b`。
