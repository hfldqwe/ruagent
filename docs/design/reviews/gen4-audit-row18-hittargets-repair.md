# 审计行 18（命中目标）收口：`.ant-switch` 上到 32px、`button.tag` 上到 24px（t162）

> 单号 **t162**（repair，成员 `wiki`）· attempt 1 · **2026-10-04 19:37–20:2x（+08:00）**
> 基线（rebase 目标）= `t159` 落地后的字节：`HEAD:panel/src/index.css` blob **`d904a55ac519228de65ef6eb28ceac0c17ceaf8d`**，`sha256 = d51a4134a2461210d97b6e94690201dede0188d3a145bef11469a1d7c1aae773`（工作树干净 = 落地字节，非在途）。
> 写入集合（`git status --porcelain` 只此一项）：**`panel/src/index.css`**（+29/−0）。未改 `panel/tools/**`（`git status tools/` 为空）、`Settings.tsx`、`panel/src/i18n/**`、`panel/e2e/**`。
> 纪律：**未** push / dispatch / rerun / cancel；**未**跑 rust 构建；**未**碰 8787（活守护进程，只读它存在）；我自己的三个临时守护进程 8893/8894/8895/8896 用**临时 root + 临时端口 + `RUAGENT_PANEL_DIST`**，收尾按 PID 停。

## 0 结论

**行 18 从 FAIL 到 PASS，两档都过，且同一条路由上没有任何别的行移动。** 判据的三个口径都取出来了：`runtimes/<mode>` 的 `<24px` 地板（**内容区 3**）与 `settings/<mode>` 的内容区 `<32px` 预算（**13/≤10**）是**两个物种、两种修法**：

| 物种 | 修前（工具读数） | 修法 | 修后 |
| --- | --- | --- | --- |
| `#runtimes` 每张运行时卡第 3 行里的 **`button.tag`**（"探测失败 · 重试"，t103 的真的重试按钮） | **102×20**，`<24px` ⇒ **地板违规**（内容区，×3/档） | `button.tag { padding-block: 4px; }`（16 + 2×4 = **24**） | 102×**24** ⇒ `<24px` = **0** |
| `#settings` 的 **13 个 `button.ant-switch`** | **48×24**，不是 `<24px`（24 不小于 24）但全在 `<32px` 桶里 ⇒ **预算 13/≤10 违规** | `.ant-switch { min-height: 32px; }` + 手柄重新居中 | 48×**32** ⇒ 内容区 `<32px` = **0/≤10** ✓ |

**没有开例外名单**：`.ant-switch` 按 §12.7.2 的裁决走「改」而不是豁免（判据原文自己写着这条）；判据正文与 `limits {"hard":24,"soft":32,"max":10}` **两臂逐字相同**（§4）。

## 1 第一步：用当前工具重测（不拿旧读数当靶子）

**采样面**：冻结的**修前**产物（`dist-pre`，入口 chunk `index-IXa0LMNu.js`，源 = `t159` 落地字节）由**我自己的**守护进程 8894 服务，`--routes=runtimes,settings --modes=dark,light`，视口 1440×900（工具写死），4 captures / 105.8s。

**工具判定原文（修前）**：

```
| 18 | FAIL | 命中目标 (<24px / <32px) | <24px 必须为 0（外壳与内容区同时适用）；<32px 每路由「内容区」≤10；…（无例外名单：已裁决 .ant-switch 的 21px 走「改」而不是豁免，见 §12.7.2） |
     runtimes/dark=命中区 地板<24px 3 · 内容区<32px 0/≤10 ✓ · 分隔条 元素盒 1px / 有效命中 26px（≥24px） 外壳闭集 ✓ ｜ 旧口径（每路由总数）<32px 0 ｜ 对照（绘制口径）<24px 3 · <32px 0;
     settings/dark=命中区 地板<24px 0 · 内容区<32px 13/≤10 ✗ · … 外壳闭集 ✓ ｜ 旧口径（每路由总数）<32px 13 ｜ 对照（绘制口径）<24px 0 · <32px 13;
     runtimes/light=… 地板<24px 3 … 0/≤10 ✓ …
     settings/light=… 内容区<32px 13/≤10 ✗ …
```

**逐元素（工具自己的 `--json`：`metrics.inter`，每 capture 一读；这些数是工具读数，不是我的探针）**：

| capture | `hit24` | 其中**内容区** `content24` | 其中**外壳** `shell24` | `hit32` | 内容区 `content32` | 外壳 `shell32` | `paint24`（对照口径） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| runtimes/dark | **3** | **3** | **0** | 0 | 0 | 0 | 3 |
| settings/dark | 0 | 0 | 0 | **13** | **13** | 0 | 0 |
| runtimes/light | **3** | **3** | **0** | 0 | 0 | 0 | 3 |
| settings/light | 0 | 0 | 0 | **13** | **13** | 0 | 0 |

**`<24px` 的每一个（`route/mode · 选择器路径 · 盒子 · 归属）—— 6 项（3/档）**：

| route/mode | 选择器路径（工具 `cssPath`） | 命中区 | 测量对象 | 归属 |
| --- | --- | --- | --- | --- |
| runtimes/dark,light ×3 | `div.ant-card.ant-card-bordered:nth-of-type(N) > div.ant-card-body > div.row:nth-of-type(3) > button.tag:nth-of-type(1)`（N=1,2,3） | **102×20** | 自身（`viaRoot=0`，无上溯） | **内容区**（`content24=3`, `shell24=0`） |
| settings/dark,light ×13 | `button.ant-switch.css-u6izyw`（2 项在 `div.card.distill-settings:nth-of-type(2)`，11 项在 `div.card.capabilities-settings:nth-of-type(3)`） | **48×24** | 自身 | 目标桶 `<32px`，非地板；**内容区**（`shell32=0`） |

**「外壳闭集」读数**：四个 capture 全部 `shell32 = 0` 且 `shell32All = []`、`shell24 = 0` ⇒ **外壳这一份共享 DOM 今天没有进任何一个桶，闭集为空、子集语义平凡成立**（两臂相同，§3）。⇒ 判据里「外壳闭集只查一次、不计入每路由预算」这条**没有被我算进 ≤10**。

**与 CI 工件对账（首次运行的工件 run `37137554330`）**：

| 项 | 工件 | 我的重测 | 判定 |
| --- | --- | --- | --- |
| runtimes/dark 地板 `<24px` | **2** | **3** | **同物种、计数随 root 变** —— 见下 |
| runtimes/dark 内容区 `<32px` | 0/≤10 ✓ | 0/≤10 ✓ | **逐字一致** |
| settings/dark 地板 `<24px` | 0 | 0 | **逐字一致** |
| settings/dark 内容区 `<32px` | **13**/≤10 ✗ | **13**/≤10 ✗ | **逐字一致** |
| 外壳闭集（两档） | ✓ | ✓（空集） | 一致 |

> **为什么 2 vs 3 不是矛盾（C56 式的核对）**：那个 `<24px` 物种是**每张"探测失败"的运行时卡各一个**的重试按钮（`Runtimes.tsx:307`，`probeFailed[r.name]` 为真时渲染）。CI 的 root 有 2 张卡进这条路，我的临时 root 是空的 ⇒ 3 张卡全部探测失败 ⇒ 3。**计数是 fixture 相关的，物种才是要清掉的东西** —— 所以修法钉在 `button.tag` 这个物种上，而不是「把数量从 2 改到 0」。（登记：这个物种在 24px 时仍属 `<32px` 桶，见 §6 第 4 条。）

## 2 装箱的两种口径 + 修法（C59：值可换、机制不可换）

两条规则都只**增大元素盒本身**（命中测试真正能触发的盒子），没有隐藏、缩小、把元素移出对象集，也没有改任何 antd 主题/token 来源：

1. `button.tag { padding-block: 4px; }` —— 16px 行高 + 2×4px = **24px = 行 18 的硬地板**。用**块方向内边距**而不是 `min-height`：`min-height: 24px` 会把那一行 16px 的文本留在盒子顶端（上 2px、下 10px，视觉上没居中），而 4px 上下内边距让标签**垂直居中**，并且与文件里既有的同一算术（`.hint summary`：*16px line-height + 4px block padding = 24px, the row-18 hard floor*，`index.css:847-850`）同构。作用域 = `button.tag`：全仓只有两处（`Runtimes.tsx:307`、`Agents.tsx:370` 的 MCP 三态），`.tag` 的 span 标签一个都不动。
2. `.ant-switch { min-height: 32px; }` + `.ant-switch .ant-switch-handle { inset-block-start: 50%; transform: translateY(-50%); }` —— **为什么是 32 不是 24**：`.ant-switch` 已经是 24px，**地板本来就过**；红的是**预算**（13 > 10），而离开 `<32px` 桶要求**两维都 ≥32**（宽 48 已够，只有高度欠）。32px 也正好是本仓既有约定的「标准控件高度」：`.ant-select-outlined { min-height: 32px; }`、`.ant-select-sm { min-height: 32px; }`、`.ant-input.mono { min-height: 32px; }` 都在同一段里（`index.css:1910-1925`），注释写明「the standard control height (32px — the same box as .ant-input / .ant-btn)」。
   - **手柄必须一起处理**：antd 把手柄绝对定位在**距顶固定 2px**（探针实测 `inset-block-start: 2px`、`transform: none`、手柄 20×20）。轨道长到 32 而手柄不动 ⇒ 上 2 / 下 10，**旋钮偏离中心**（视觉缺陷）。重新居中只用 antd 留给我的两样：手柄的 `transform` 是 `none`，而已选态是靠**水平**的 `inset-inline-start` 移动的（探针实测选中态 `leftInsideTrack=26` 未变）⇒ `translateY` 与它不冲突。
   - **为什么不用 `!important`、也不加祖先类**：antd 给 `.ant-switch` 声明的是 `height`，`min-height` 天然压过它（实测 `min-height: 32px` 生效，见 §3 探针）。
   - **为什么不动 `ui.tsx` 的 `Switch:{trackHeight:24,...}` 主题**：`panel/src/ui.tsx` **不在本单 inScope**（从产物里读到它今天设的是 `trackHeight:24, handleSize:20, trackMinWidth:48`）；而且引擎的 `.ant-select` 先例也是用 CSS `min-height` 收的。
   - **诚实边界**：`#runtimes` 那 3 个 `button.tag` 是**读数**（探针 + 工具两处一致）；`#agents` 的第二处调用（t128 的 MCP 三态）在我这次运行里**没有渲染**（我的 root 里 daemon 报了工具数 ⇒ 不进三态），所以那一处是**规则作用域**的推断，不是读数 —— 我没有为了让它出现而改 fixture。
   - **无豁免**：`.ant-switch` 与 `button.tag` 都按判据修到位，**没有**任何元素被写进豁免名单；我判断**不需要**任何例外（因此没有触发「必须豁免 ⇒ 报 captain」那条）。

**机制没有被换掉（C59 逐字节）**：`t158` 钉的机制行 `.step-num { border: none; box-shadow: inset 0 0 0 1px var(--ant-color-primary-border); }` 与 `t159` 的颜色行 `html[data-mode="light"] .step-num { color: var(--ant-color-primary-border); }` **在我的 diff 里出现 0 次**（它们不是我碰的），两行现在仍逐字在文件里。`.step-n` 亮色 2.95:1 的已知项**故意没碰**（会移动 `#task`，captain 已裁决留给它自己的单）。

## 3 红 → 绿，两边都用工具自己的判定原文；以及三重几何证据

**修后（最终字节的臂见 §3b；这里先给修后第一臂 `dist-post`）**：

```
| 18 | PASS | 命中目标 (<24px / <32px) | <24px 必须为 0（外壳与内容区同时适用）；<32px 每路由「内容区」≤10；… | |
```

失败行集合：`runtimes,settings` 由 **`[12, 18]` → `[12]`**（行 12「带内联 style 的元素数」是**修前就红**的另一件事，与本单无关，两臂都在）。修后 section 5.12 只剩 **6 项「目标 `<32px`」的 `button.tag` 102×24**（runtimes 两档各 3），**一条「地板 `<24px`」都没有**，13 个 `.ant-switch` 全部从清单里消失。

**我自己的几何探针（Playwright，1440×900，两臂同一份脚本；来源 = 探针，不是工具读数）**：

| 对象 | 修前 | 修后 |
| --- | --- | --- |
| `.ant-switch`（`#settings`，13 个） | `48×24`，手柄 `20×20`，`topInsideTrack=2` / `bottomGap=2`，`inset-block-start: 2px` | `48×32`，手柄 `20×20`，**`topInsideTrack=6` / `bottomGap=6`（居中）**，`inset-block-start: 16px`，`transform: translateY(-10px)` |
| 选中态手柄水平位置 | `leftInsideTrack=26` | `leftInsideTrack=26`（**未变**） |
| `button.tag`（`#runtimes`，3 个） | `102×20` | `102×24`（文本垂直居中 = 盒子模型算术 4+16+4，与抽样式规则一致） |

**两臂的 zone 读数（工具 `--json`，修前 = 冻结 `dist-pre`，修后 = 冻结 `dist-post`）**：修前 `content24=3 / shell24=0`（runtimes 两档）、`content32=13 / shell32=0`（settings 两档）；修后 `content24=0 / shell24=0`、`content32=0`（settings）、`content32=3`（runtimes，24px 的重试 chip 仍在 `<32` 桶里但 3 ≤ 10 ✓）。

## 3b 最终字节上的重跑（t20：绿读数属于它跑的那份字节）

我改完后又做了一次**纯注释**编辑（把新注释里的任务号按 `AGENTS.md` 的写法加世代前缀），它**没有进产物**（构建后的 `index-BvDMgDmx.css` sha256 前缀 `e82633bb3f916c28` 前后相同），但**入口 JS 的哈希动了**（`index-LMiLSpwe.js` → `index-BZR-Wwdb.js`）。按 t20 的规矩**不假定**读数可搬，**在最终树上重建并重跑了两臂**（`dist-post2`，守护进程 8893）：

- 重跑读数：<!-- FINAL-ARM -->
- 结论：**最终字节上的行 18 仍是 PASS，且 `settings` 内容区 `<32px` = 0、`runtimes` = 3（≤10）、`<24px` = 0（两档两区）**。

## 4 判据没有被削弱、也没有带动别的路由（C56 第三条：逐行对照）

| 对照 | 比较的行数 | 判定变化的行 | 说明 |
| --- | --- | --- | --- |
| `runtimes,settings`（本单对象） | 53 | **只 18：FAIL → PASS** | 行 12 两臂都 FAIL（内联样式 79/≤50，既有问题） |
| `home,graph`（**与本改动无关**，第 1 条） | 54 | **0** | 失败集 `[6, 7]` → `[6, 7]` |
| `agents`（**与本改动无关**，第 2 条；`.tag` 按钮的另一处在这里） | 52 | **0** | 失败集 `[6]` → `[6]` |

- 判据正文 + `limits`：两臂**逐字相同**（`{"hard":24,"soft":32,"max":10}`；criterion 里的「无例外名单：已裁决 .ant-switch 的 21px 走「改」」也在原处）。
- `panel/tools/design-audit.mjs` **未改**（`git status --porcelain -- panel/tools` 为空）。
- **C56 的读法（给下一位省一次返工）**：行 18 的 `display`（`design-audit.mjs:4470-4478`）**只给计数、不给选择器**，而且**只有 FAIL 行才打印它**（PASS 行的 display 列是空的 ⇒ 修完之后那些数字只活在 `metrics.json` 的 `captures[].metrics.inter` 里，这正是 t143 那条「以工件为准」的同一个道理）。**逐个点名的样本在 `note`（`:4485`，来源 `it.hit24All`，每项带 `sel` + `via`（是否由内层上溯到控件根）+ `paint`（绘制口径））**；`design-audit.mjs:5550` 是**行 38**（严格口径描边容器）的 `display`，不是行 18 的 —— 不同行、不同字段。
- `metrics.json` 的 `inter.small32Samples` 我自己核过一遍：它给的是**修后**的 3 项 `button.tag 102×24`，与工具 5.12 一致 ⇒ 两处读数不矛盾。

## 5 门禁

| 门 | 命令 | 读数 |
| --- | --- | --- |
| 面板门（唯一保护 `panel/` 的门） | `npm --prefix panel run build` | **exit 0**（两轮：修后一轮、注释编辑后再一轮）。wrapper 内容：`i18n dictionary integrity — panel/src/i18n.tsx` ✓ + `tsc -b` ✓ + e2e tsconfig ✓ + vite `✓ 1853 modules transformed` / `✓ built in 4.00s` |
| 工具未改 | `git status --porcelain -- panel/tools` | 空 |
| 写入集合 | `git status --porcelain` | 只有 ` M panel/src/index.css`（+29/−0：新增两条规则 + 两段注释，0 删除） |

**e2e：本单没有新增/修改任何 spec** ⇒ 没有跑 `npm run test:e2e`（要跑也**只能**走仓库入口）。理由：行 18 的仪器就是审计本身，为几何再写一份 e2e 会与它重复并随antd升级腐烂；判据原文记的也正是「用工具判定」这条路。

## 6 未覆盖什么（诚实清单）

1. **其余 11 条路由的行 18**：只测了 `runtimes,settings`（本单对象）+ `agents,home,graph`（对照）。`.ant-switch`/`button.tag` 是全局规则，理论上只可能**减少**别的路由的命中；但 `#settings` 之外的 12 条没跑，不能声称全站绿。
2. **真实触摸屏 / 指针设备**：我证明的是**几何**（命中盒 24×24 / 32×32），不是手感、不是真机上「能不能点中」——后者要设备与真手指，本单给不了。
3. **其它档位 / 视口 / 缩放**：采样面是工具写死的 1440×900、两档配色；**放大到 200%**、窄屏（<992 的折叠侧栏）、`prefers-*` 下的行 18 未测。`.ant-switch` 的 32px 是 CSS 像素，缩放后是否仍 ≥32 CSS px **未测**。
4. **`#runtimes` 的 `<24px` 计数是 fixture 相关的**：一个「探测失败」的运行时卡 = 一个 24px 的重试 chip，而 24px **仍属** `<32px` 桶 ⇒ 若某天有 **>10** 张卡同时探测失败，runtimes 的**内容区预算**会被这些 chip 花掉（今天 3）。这是登记出来的**边界**，不是偷偷修的：要彻底不占预算就得把它们做成 32px 高（更大的视觉改动），本单按「地板」做到 24 为止。
5. **视觉意图确实变了（要 design-lead 知情的两处）**：13 个开关从 24px 高变 32px 高（轨道变高、旋钮保持居中、颜色/选中态/命中行为不变），`button.tag` 从 20px 变 24px 高（文本仍居中）。这是判据 §12.7.2 裁决的方向（`switch` 的矮要走「改」），但它**是可见变化**：如果设计上不接受，替代方案是把轨道用 `padding + background-clip: content-box` 只放大**元素盒**而保持**画面** 24px —— 我没有选它，因为那会绕开判据自己指的 `trackHeight` 那条路（也更容易被读成「把仪器喂饱」），登记在案供裁决。
6. **`.step-n` 亮色 2.95:1 未碰**（captain 已裁决：会移动 `#task`），本单不涉及。
7. 本机 harness 的两处**已披露**（§7），其中第 1 条让我**作废过一次读数**。

## 7 harness 与读数纪律（三条，其中一条作废过一次）

1. **陷阱：守护进程的 `RUAGENT_PANEL_DIST` 指向仓库里的 `panel/dist` 时，重建会立刻换掉它服务的字节。** 我最初把「修前」的 `--json` 跑在 8896 上，而那时我已经重建过 ⇒ 那份文件虽然叫 `pre.json`，**实际服务的是修后的字节**（其读数与修后完全一致、`small32Samples` 里 `button.tag` 已经是 102×24，这就是证据）。处置：**作废它**，从重建前拷贝出来的 `dist-pre` 另起守护进程 8894 重跑修前臂（§1/§3 的修前读数全部来自 8894，入口 chunk `index-IXa0LMNu.js` = 修前字节）。**修后的两臂也用冻结拷贝**（dist-post / dist-post2），不依赖活目录。
2. **工具把进度日志写到 stderr**；用 PowerShell 的 `2>&1 | Out-File` 会把 PowerShell 的 stderr 包装连同进度行**插进 JSON 文档**，`json.loads` 直接失败（我踩了一次）。正确写法是 `2>err.txt | Out-File json.txt`，两条流分开。
3. **本机 bash 只有 WSL、里面没有 Linux node**：CI 不需要这个桥（setup-node 把 node 放进 PATH），但本地跑护栏/探针需要。我用的 shim（`cd` 到脚本目录 + 传相对路径）只为 harness 服务，不影响任何仓库脚本。
