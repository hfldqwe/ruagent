# 审计行 13+14 修复 — 正文文本对比度 < 4.5:1（t159）

> 来源 = 审计自己给出的判定（首次 CI 运行 `37137554330` 的工件）：`row 13 [fail] home/dark=1, settings/dark=6`、`row 14 [fail] home/light=3`，`target: 0`。
> **我的写入集合**：`panel/src/index.css`（唯一源码改动，20 行全为新增）+ 本报告。
> `panel/tools/design-audit.mjs` sha256 `E3B7E45B59FA1C49…`（mtime 2026-09-29T03:51:49）**一字节未改**；`Home.tsx` / `Settings.tsx` / `panel/src/i18n` / `panel/e2e` 未改（本次不需要，理由见 §3）。
> ⚠️ **我（verify）是本单修复的作者**：下面的红→绿是我自己量的**读数**，不是独立评审 —— §9 给出可复现配方，这批字节仍需一位独立验证者。

## 0 结论

**用当前工具重测（不是拿旧读数当靶子）→ 定位到 10 个节点、两类成因 → 只改颜色 → 两行两档都 0，且逐行未带动别的行。**

| 读数 | 修前臂 | 修后臂 |
| --- | --- | --- |
| `row 13`（暗） | **FAIL** `home/dark=1 (WCAG口径 1)`、`settings/dark=6 (WCAG口径 6)` | **PASS** `home/dark=0 (WCAG口径 0)`、`settings/dark=0 (WCAG口径 0)` |
| `row 14`（亮） | **FAIL** `home/light=3 (WCAG口径 3)`（`settings/light=0` 本就 PASS） | **PASS** `home/light=0 (WCAG口径 0)`、`settings/light=0 (WCAG口径 0)` |
| 整轮**失败行集合** | `7, 12, 13, 14, 18` | `7, 12, 18`（**只掉了 13/14**；7/12/18 是先前就红、与本单无关） |
| `#home` 逐行（C56） | 4 `0/≤1` · 5 `3/≤40` · 13 `1(WCAG 1)` · 14 `3(WCAG 3)` · 30 `ring #dee0e4 1px → canvas 1.23:1 / panel 1.32:1` · 38 `0` | **4 同 · 5 同 · 13→`0` · 14→`0` · 30 逐字同 · 38 同** |

**两类成因（十个节点全在）**：
1. **暗色 ×7**：`disabled` 的 antd 按钮标签。文字 = `var(--ant-color-text-quaternary)` `#838891`，而按钮自己用 `rgba(255,255,255,0.08)` 抬了一层底，复合到卡片 `#1b1e24` 之上 = **`#2d3036`** ⇒ **3.71:1**（14px/500）。`#home` ×1（「看 板」）、`#settings` ×6（「应 用」）。
2. **亮色 ×3**：`#home` 的 `.step-num` 徽章数字。文字 = `var(--ant-color-primary)` `#c88413` 压在 `var(--ant-color-primary-bg)` `#fff9e6` 上 ⇒ **2.95:1**（11px/400）。

**修法只碰颜色，不碰机制**（`verify2`/captain 的处方）：`t158` 钉在同一个 `.step-num` 上的**画环机制**（`border: none; box-shadow: inset 0 0 0 1px …`，`index.css:824`）**我一个字符都没动**；我的 diff 是 **20 行纯新增、0 行删除**，且探针实测该元素 `borderWidth` 与 `boxShadow` 前后**逐字相同**（§5）。这正是 `t158` 的 V158-3 教训所在——它把 Card 改 `borderless` 时 `#home` 的 row 30 掉过 PASS；我这一轮 row 30 的读数**逐字未变**。

## 1 仪器与字节（C47：把「哪份工具 + 哪份 build」写死）

| 项 | 值 |
| --- | --- |
| 工具 | `panel/tools/design-audit.mjs` sha256 **`E3B7E45B59FA1C49…`**，mtime **2026-09-29T03:51:49**，`git status` 对该文件为空。**注意**：判据正文里引用的工具是 `fdb93db1…`（02:26:57）——**不是**这一份 ⇒ 我的 0 属于「当前工具 + 下面的 build」，不是那个版本。 |
| 调用 | `node panel/tools/design-audit.mjs --check --routes=home,settings --modes=dark,light --tabs=40 --no-shots --no-pixels --allow-not-measured --base-url=http://127.0.0.1:<port> --out=<仓外绝对目录>`；`E2E_BASE_URL` **显式**设成同一个 port（B29：默认值指向活守护进程）。 |
| 我自己的 daemon | `cargo-team.ps1 rustc -p ruagent --bin ruagent -- -o %TEMP%\t159\bin\ruagent.exe` ⇒ **exit 0 / 87.3 s**，328,077,312 B，sha256 **`F39D6A51A6531ED4`**，构建于 **19:17:43**（早于 t172 窗口）。临时 root + 临时端口 8861/8862，`NO_PROXY=127.0.0.1,localhost,::1`；**8787 的 pid 14944 未触碰**。 |
| 两臂的字节（**内容级识别**，C56） | **修前臂** = 19:18 从当时工作副本（含 `t158` 落地的 `index.css`）构建并**冻结**的 `dist-pre`：`index-DH8EJgem.css` sha256 `C94A9566236A42F0`（45,785 B），端口 **8861**（pid 27452，线上探到该 css 200 / 45,785 B）。**修后臂** = 我的修复后构建的 `dist-post`：`index-CQhHFKM-.css` sha256 `46DDF76A81E3C152`（45,932 B，= 45,785 + 147 B，即两条新规则 + 注释），端口 **8862**（pid 11932；该 port 上旧名 `index-DH8EJgem.css` 只回 1139 B 的 index 回退 ⇒ 两臂各供自己的字节）。 |
| 源码字节 | 修前 `panel/src/index.css` sha256 `FB575D60EDFE7F82`（工作副本 CRLF 字节；其 `git` blob 即 captain 引的 `d3384f3f…`，两者是同一文本的不同编码，同 t146/t151 记录的 CRLF/LF 差异）；修后 sha256 **`D51A4134A2461210`**（mtime 2026-10-04T19:25:57）。`git diff --numstat` = **20 增 / 0 删**。 |

## 2 第一步：用【当前工具】重测，逐个节点定位

四个 capture 的 `contrast` 计数（工具读数）：

| capture | checked | docViolations | wcagViolations | ambiguous | alphaSkipped |
| --- | --- | --- | --- | --- | --- |
| `home/dark` | 61 | **1** | **1** | 0 | 0 |
| `settings/dark` | 140 | **6** | **6** | 0 | 0 |
| `home/light` | 61 | **3** | **3** | 0 | 0 |
| `settings/light` | 140 | 0 | 0 | 0 | 0 |

⇒ `home/dark=1`、`settings/dark=6`、`home/light=3` **与 CI 工件逐字一致**（判据正文警告的 0↔1 漂移在**这份**工具上表现为**没有漂移**：三个数都复现）。`ambiguous=0`/`alphaSkipped=0` ⇒ 没有因渐变/图片底或透明前缀被跳过的对，十个节点全部落进对象集。

**逐节点读数（工具 `metrics.json` 的 `contrast.worst[]`，来源 = **工具读数**）**：

| # | capture | 选择器（工具给的 `sel`，节选） | 文本 | fg | bg | 实测比值 | px/w |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | `home/dark` | `div.ant-card.ant-card-bordered:nth-of-type(2) > div.ant-card-body > button.ant-btn… > span` | 「看 板」 | `#838891` | `#2d3036` | **3.71:1** | 14 / 500 |
| 2–7 | `settings/dark` ×6 | `div.row.tight > span:nth-of-type(1) > button.ant-btn… > span` | 「应 用」 | `#838891` | `#2d3036` | **3.71:1** | 14 / 500 |
| 8–10 | `home/light` ×3 | `div.steps:nth-of-type(3) > div.ant-card…:nth-of-type(1…3) > div.ant-card-body > div.step-num` | 「1」「2」「3」 | `#c88413` | `#fff9e6` | **2.95:1** | 11 / 400 |

**逐条对齐 WCAG 口径**：十个节点都是 11–14px、权重 400/500 ⇒ 全部属于「正文」（都不是 large text：<24px，且未达 ≥18.66px+700）⇒ 阈值为 **4.5:1**（WCAG 2.x 1.4.3）。因此「文档口径」与「WCAG 口径」在这十处**同数**（1/6/3 vs 1/6/3），工具 display 里 `(WCAG口径 n)` 与前面数字相等也与之一致。

**它们由哪个 token 喂的（来源 = 我自己复刻谓词的探针，不是工具读数）**：我按 `design-audit.mjs:571-584` 的**逐层 alpha 复合**规则在浏览器里复刻了 `bgFor`（工具 JSON 只给最终的 `bg`，不给链路），并读 `getComputedStyle`：
- 暗色那 7 个都是 **`disabled` 的 antd 按钮**（`button.ant-btn…` 且 `disabled: true`；`#home` 是 `ant-btn-default`，`#settings` 是 `ant-btn-primary`）：文字 `#838891` = `--ant-color-text-quaternary`，按钮自身底色 `rgba(255,255,255,0.08)` 复合在卡片 `#1b1e24` 上 ⇒ `#2d3036`（**与工具给的 `bg` 逐字相同** ⇒ 我的复刻成立）。
- 亮色那 3 个是 `.step-num`：规则在 `index.css:808-817` —— `background: var(--ant-color-primary-bg)`、`color: var(--ant-color-primary)`、`font-size: 11px`。

## 3 修法（两行颜色；不改机制、不改对象集）

```css
html[data-mode="light"] .step-num { color: var(--ant-color-primary-border); }
html[data-mode="dark"]  .ant-btn:disabled { color: var(--ant-color-text-secondary); }
```
（连同说明注释，共 20 行新增，紧跟 `t158` 的 `.step-num { border: none; box-shadow: inset … }` **之后**，不覆盖它。）

- **亮色徽章**：`--ant-color-primary` `#c88413` → **`--ant-color-primary-border` `#8a5600`**（同一个琥珀色系里更深的那支，而且**本来就是这枚徽章环的颜色** ⇒ 数字与环现在同色，视觉上更自洽）；在同一个 `#fff9e6` 底上实测 **5.85:1**。**作用域限定 `light` + `.step-num`**：暗色同规则实测 4.81:1 本来就过，限定亮色可以让暗色读数**逐字不变**；`.step-n` 是另一个类（`TaskDetail.tsx:621` 用于 `#task`），本单不动（见 §8 的发现）。
- **暗色按钮**：标签从 quaternary `#838891` → **`--ant-color-text-secondary` `#adb2bb`**（2d3036 上 **6.21:1**；在 `--ant-color-bg-elevated` 那类更亮的抬升面上也 ≥5.1:1）。**`disabled` 状态本身没被削弱**：`[disabled]` 属性、`cursor: not-allowed`、以及那层抬升底都还在。**限定 `dark`**：亮色同位置实测 4.81:1 本来就过。
- **没有做的事**（按验收禁止项）：没有把文本隐藏/缩小/移出对象集（十个节点的 `checked` 计数前后都是 61/140/61/140，**对象集规模未变**）；没有改 `panel/src/ui.tsx`、没有改 `Home.tsx`/`Settings.tsx`（颜色都在 token 层，视图代码不需要动）；**没有**把 `t158` 的 `box-shadow` 环改回 `border`。

## 4 红 → 绿（两边都是工具自己的判定原文）

**修前臂（8861 / `index-DH8EJgem.css`）**，`--check` 整轮 `exit=1`，表格行（逐字）：
```
| 13 | FAIL | 暗色文本对比度失败数 (正文 4.5:1) | 0（当前观测：工具 02:26:57（sha fdb93db1…）在 build DKadudCh 上报 knowledge/dark = 1（WCAG 口径 1）⇒ 该行现为 FAIL，待定位那 1 个文本节点；工具 02:18:02 与 round-2（工具 00:33:38）均报 0，同样是工具版本差异，需以最新工具复测为准） | home/dark=1 (WCAG口径 1); settings/dark=6 (WCAG口径 6) |
| 14 | FAIL | 亮色文本对比度失败数 (正文 4.5:1) | 0 | home/light=3 (WCAG口径 3) |
```
`metrics.json`：`row 13` `verdict=fail` / `row 14` `verdict=fail`，两者 `limits={"max":0}`；`[home/dark] pass=False display=1 (WCAG口径 1)`、`[settings/dark] pass=False display=6 (WCAG口径 6)`、`[home/light] pass=False display=3 (WCAG口径 3)`、`[settings/light] pass=True display=0 (WCAG口径 0)`。

**修后臂（8862 / `index-CQhHFKM-.css`，最终字节）**：
```
| 13 | PASS | 暗色文本对比度失败数 (正文 4.5:1) | 0（…同上判据正文，逐字未动…） |  |
| 14 | PASS | 亮色文本对比度失败数 (正文 4.5:1) | 0 |  |
```
`metrics.json`：`row 13` `verdict=pass` / `row 14` `verdict=pass`，`limits={"max":0}`（与红臂逐字同），`[home/dark] pass=True display=0 (WCAG口径 0)`、`[settings/dark] pass=True display=0 (WCAG口径 0)`、`[home/light] pass=True display=0 (WCAG口径 0)`、`[settings/light] pass=True display=0 (WCAG口径 0)`。

**判据未削弱**：两臂的行 13/14 `criterion` 长度都是 **21**、逐字相同（`0（当前观测：工具 02:26:57…）` / `0`），`limits` 都是 `{"max":0}`；工具文件 sha 未变 ⇒ **不需要改工具**。

## 5 C56：同一条 `#home` 路由的逐行对照（两档），以及机制未被换掉

| 行 | 修前（home/dark · home/light） | 修后 | 判定 |
| --- | --- | --- | --- |
| 4 | `0 / ≤1` · `0 / ≤1` | `0 / ≤1` · `0 / ≤1` | **逐字同** |
| 5 | `3 / ≤40` · `3 / ≤40` | `3 / ≤40` · `3 / ≤40` | **逐字同** |
| **13** | `1 (WCAG口径 1)` · —（本行只判暗色） | **`0 (WCAG口径 0)`** | 我的目标 ✓ |
| **14** | —（只判亮色）· `3 (WCAG口径 3)` | **`0 (WCAG口径 0)`** | 我的目标 ✓ |
| 30 | 只判亮色：`ring #dee0e4 1px → canvas 1.23:1 / panel 1.32:1` | **`ring #dee0e4 1px → canvas 1.23:1 / panel 1.32:1`（逐字同）** | **未被削弱** ✓ |
| 38 | `0` · `0` | `0` · `0` | `t158` 的机制仍活着 ✓ |

**内容级复核（来源 = 我自己复刻谓词的探针；同一台浏览器、同样 2.5 s 等待）**：

| 元素 | 修前 | 修后 |
| --- | --- | --- |
| `.step-num`（`#home/light`） | `color=#c88413`、`bg=#fff9e6`、`2.95:1`、`borderWidth=0px/none`、`boxShadow=rgb(138, 86, 0) 0px 0px 0px 1px inset` | `color=#8a5600`、`bg=#fff9e6`、**`5.85:1`**、`borderWidth=0px/none`、**`boxShadow` 逐字相同** |
| `disabled` 按钮（`#home/dark`） | `color=#838891`、`bg=#2d3036`、`3.71:1`（「看 板」） | `color=#adb2bb`、`bg=#2d3036`、**`6.21:1`** |
| `disabled` 按钮（`#settings/dark`） | `color=#838891`、`bg=#2d3036`、`3.71:1`（「应 用」） | `color=#adb2bb`、`bg=#2d3036`、**`6.21:1`** |

⇒ 「字体颜色可以换、画边的机制不能换」这条被**逐字验证**：环还是那层 `inset 0 0 0 1px rgb(138,86,0)`、`border-width` 仍是 `0px`，而行 4/38/30 的读数一条没动。

## 6 两条与改动无关的路由（两臂对照）

| 臂 | 行 13/14 在 `sessions`/`graph` 上 | 失败行集合与原文 |
| --- | --- | --- |
| 修前（8861） | 四个 capture 全 `pass`，`0 (WCAG口径 0)` | `row 6 → graph/dark=1 / ≥3 · 标题 1@20px ; graph/light=1 / ≥3 · 标题 1@20px` ｜ `row 7 → graph/dark=20px / ≥32px (含仪表盘) ; graph/light=20px / ≥32px (含仪表盘)` |
| 修后（8862） | 四个 capture 全 `pass`，`0 (WCAG口径 0)` | **逐字相同**（同上） |

⇒ 两条无关路由的**失败行集合与证据逐行相同**；我的改动没有带动它们（`sessions` 上 13/14 两臂都是 0，本来就过）。

## 7 面板门禁

`npm --prefix panel run build`（仓根）⇒ **exit 0**，跑了三次（冻结修前臂、修后、最终）：`✓ built in 4.03s / 4.20s / …`、`build-panel: dist updated (55 assets, index.html swapped by rename)` ⇒ wrapper 内的 `e2e/i18n-check.mjs` + `tsc -b` + `tsc -p e2e/tsconfig.json --noEmit` + vite 全过。最终产物 `panel/dist/assets/index-CQhHFKM-.css` sha256 `46DDF76A81E3C152`。**没有新增/修改任何 e2e spec** ⇒ 按契约不触发 `npm run test:e2e`，也**没有**用裸 `npx playwright test`。

## 8 未覆盖什么（第 19 条）

1. **其余路由的行 13/14**：只测了 `home`/`settings`（本单目标）+ `sessions`/`graph`（对照）；其余 9 条路由没有读数。
2. **真实屏幕阅读器**：没有任何 AT 读数——我做的是**计算样式 + WCAG 数学**，不能替屏幕阅读器/放大镜对颜色改动的体验发言。
3. **其它对比度/色觉偏好档位**：只覆盖该行定义的 `dark`/`light` 两档；`prefers-contrast`、Windows 高对比度/forced-colors、系统缩放、色盲模拟**未测**。
4. **模板里的颜色是否还会在别处复用**：我改的是**两处 token 用法**，不是 token 本身值 ⇒ ① 亮色 `.step-num` 只改了 home 徽章；**同一个规则块 `:808-817` 里的 `.step-n`（`TaskDetail.tsx:621`，用于 `#task`）吃的是同样的 `--ant-color-primary` / `--ant-color-primary-bg`，因此它在亮色下仍是 2.95:1 —— 我【没有】顺手修它**（修它会移动 `#task`，本单明确保护别的路由）。这是一条**给下一单的发现**：修法照抄本单即可（`html[data-mode="light"] .step-n { color: var(--ant-color-primary-border) }`），但需要有人量 `#task` 两臂（我这台临时 root 没有任务行，`#task` 详情渲染不出来，所以**没有** `#task` 读数）。② 暗色那条规则是**应用级**的（所有 `disabled` 按钮）；在我实测的各处它都落在卡片抬升底上，但**没测**「暗色里落在主色底（`--ant-color-primary-bg`）之上的 disabled 按钮」这种组合，那种底更亮，`#adb2bb` 可能仍不足 4.5:1 ⇒ 未测残留。
5. **视觉意图是否被改变**：两处我都记录为「同色系、只调明度」，但**感知层**没测——① 徽章数字从 `#c88413` 变 `#8a5600`（更深，且与环同色）；② 暗色 disabled 标签从 quaternary 变 secondary = **更清楚**，代价是「靠低对比度读disabled」这个（本来就不合规的）提示变弱；`[disabled]`、`cursor: not-allowed`、抬升底仍在。是否可接受属于设计判断，我给出读数不替设计签字。
6. **`--no-pixels`/`--no-shots`**：两臂都带这两个开关 ⇒ 我的比值全是**计算样式**口径，没有像素级交叉复核（该行本来就是计算样式判据）。另：工具的 `worst[]` 有 `sampleCap` 上限，我列的十个节点都在上限内、未见截断（counts 1/6/3 与列出的条数相等）。
7. **CI job 未重跑**（`.github/**` 出 inScope），也**没有**全量 `--check`（13 路由 × 两档）。

## 9 过程、收尾、残留 + 复现配方

- **两臂怎么造**（无 worktree、无需 `npm ci`）：修前臂 = `npm --prefix panel run build` 后立刻把 `panel/dist` **冻结**到仓外、由**我自己的**临时 daemon（临时 root + 临时端口 + `RUAGENT_PANEL_DIST`）服务；再改源码、重建、冻结第二份。两臂的**内容级身份**用 css 资产名 + sha256 + 大小 + 线上 `GET /assets/<name>` 的字节数认定（§1），不用「谁先谁后」猜。
- **进程**：两个临时 daemon（pid **27452**/8861、pid **11932**/8862）按记录 PID 停掉（`alive=False`）；浏览器由探针自行 `close()`；收尾**按我自己的临时路径**（`*t159*`）扫过 `chrome/msedge/headless_shell/node`，命中过一个 `node.exe`（pid 12972），复查时已自行退出 ⇒ **无我的孤儿**；**没有**按进程名/端口批量杀；**8787 的 pid 14944 未触碰**（StartTime 2026-10-02 23:05:45）。
- **临时文件**：`%TEMP%\t159` 从 **2,426,221,959 B** 清到 **3,237,052 B**（删私建 exe 328 MB + pdb/两个 root/两份冻结 dist），保留四份 `metrics.json`（`audit-pre` 772,625 / `audit-post` 772,307 / `audit-ctl-pre` 676,431 / `audit-ctl-post` 676,430 B）与日志（`%TEMP%\t159*` 共 20 个文件 / 486,026 B）作为证据；`--out` 全程仓外 ⇒ **无 `docs/screenshots` 残留**。C: 可用 **39.58 GB**。
- **仓内改动面**：`git status --porcelain -- panel/` = 仅 **` M panel/src/index.css`**（另一份 `?? docs/design/reviews/gen4-wait-residue-repair.md` 不是我的）。

**复现配方（独立验证者用）**：
```powershell
# 修前臂：git worktree @ HEAD（含 t158 的 index.css blob d3384f3f…）或按 §9 冻结 dist；修后臂：当前 index.css sha256 D51A4134A2461210
$env:E2E_BASE_URL="http://127.0.0.1:<your-port>"   # B29：别让默认值指向活守护进程
node panel/tools/design-audit.mjs --check --routes=home,settings --modes=dark,light --tabs=40 `
  --no-shots --no-pixels --allow-not-measured --base-url=http://127.0.0.1:<your-port> --out=<仓外绝对目录>
# 判据：metrics.json 的 checks[13]/checks[14] 在 home/settings × dark/light 上全为 0 且 verdict=pass；
#       再看同一份 JSON 在 #home 上的 rows 4/5/30/38（C56：别让别的行被削）与 checks 的失败行集合。
# 逐元素证据：`contrast.worst[]`（工具读数）给出 sel/px/weight/fg/bg/ratio/text；
#       要问「哪个 token、是不是 disabled、底色怎么复合的」必须自己复刻 design-audit.mjs:571-584（工具 JSON 不带）。
```
