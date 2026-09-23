# t43 评审（round 2）— 外壳与次级视图：App / CommandPalette / Graph / Runtimes / Settings

**评审对象**：t42（repair round 2，修复 t13 的 F1–F5）+ t45（共享层：F1 共享侧 + F3）
**评审基线**：`docs/design/views/view-graph.md` §8 G1–G15、`view-runtimes.md` §8 R1–R11、`view-settings.md` §8 G1–G13、`views/README.md` §2/§3、`MASTER.md` §12（含 §12.10 本代四项裁决）
**评审用构建**：**`DYuzBlVy`**（`meta.dist.buildId` 直读；`dist/assets/` 共 **16 个 chunk**）—— **不以 mtime 为证**（本环境时钟跳变过）
**评审人**：design-lead · 只评审，未改任何 `panel/` 文件

## 0 结论摘要

| t13 的 finding | 现在 | 独立证据 |
|---|---|---|
| **F1** 品牌色从未生效 + R8 描边 1.56:1 | ✅ **已修**（视图侧 t42 + 共享层 t45） | 三 tile 的 computed `color` 互不相同；描边 alpha **0.97**；**6 格矩阵全部 ≥3**（最紧 亮/claude **3.013**） |
| **F2** `#graph` 首屏 N+1（55 请求） | ✅ **已修** | 11.5s 窗口 **总计 7** = 6×permissions + **1×entities**；非 permissions 端点最大组 **1** |
| **F3** `--graph-protocol` 暖黄 | ✅ **已修**（实现）；⚠️ **规格文本待 t51 同步** | 运行时 token 暗 **`#8fa3e8`** / 亮 **`#5b6fc4`**（旧 `#ca9d33` 已消失）；`--signal` 仍 `#f0a93b` ⇒ 不再同色系 |
| **F4** 命令面板无焦点陷阱 + 加载失败静默 | ✅ **已修**（两个子项） | **连续 45 次 Tab 未离开 `.cmdk`**（`escapedAtTab = -1`）；失败行 `role="alert"` + 重试按钮 |
| **F5** `Settings.tsx` 注释与 toast 矛盾 | ✅ **已修** | `Settings.tsx:110-113` 注释解释「刻意无 toast」，`toast()` 调用已删 |

**机器判定（我自己跑的审计，非转述）**：`--routes=home,graph,runtimes,settings --modes=dark,light --overflow-viewports=390x844,768x900,1280x900,1440x900`
→ 8 captures，**35 pass / 1 fail / 3 not measured**；**唯一 fail = 行 27（首屏 JS 体积）**—— 全站性 bundle 项，**不属于本组五个面**（见 §7.3）。

**裁定：pass**（t42 的五个修复全部独立复核通过，五个面的规格条目无回归）。§6 有一条**低危新发现**（转交、不阻塞），§7 是转交/登记清单。

## 1 证据与方法（可复现）

```
cd panel && node tools/design-audit.mjs --json --routes=home,graph,runtimes,settings --modes=dark,light \
  --overflow-viewports=390x844,768x900,1280x900,1440x900 --out=<temp>/t43-audit
# → 8 captures in 47.1s · 35 pass / 1 fail / 3 not measured · buildId DYuzBlVy · failing rows: 27

node <temp>/t43-probe.mjs    # F1 三 tile 取色 + 描边合成 / F2 11.5s 逐端点请求 / F4 45 次 Tab + Escape
node <temp>/t43-contrast.mjs # 由实测合成色独立算 6 格对比度（canvas 合成，含 color(srgb …) 解析）
node <temp>/t43-vars.mjs     # 运行时 CSS 变量（--graph-protocol 等）暗/亮两模式
```

**口径声明**：① 行 19（横向溢出）我**显式指定四断点** 390/768/1280/1440，与验收的「四个断点不塌」对齐；② 构建标识只用 `meta.dist.buildId` 与 `dist/assets/` 的文件名，**不用 mtime**（时钟跳变过两次）；③ 对比度一律**先把描边按 alpha 合成到卡片底**再算 WCAG，与 §12.10.4 的口径一致。

## 2 F1 品牌色与 R8 描边（逐条复核）

**① 视图侧（t42）**：`Runtimes.tsx:224` 给 tile 加品牌类（`runtime-logo brand-*`），`:229` `mono={!brandClass(r.harness)}` —— `mono` 语义收窄为「无品牌」。
**② 共享层（t45）**：`index.css:1867-1869` 新增**复合选择器** `.runtime-logo.brand-claude/-deepseek/-opencode { color: var(--brand-*) }`（(0,2,0) > (0,1,0)，**未用 !important**）；`:1873` 起的 `.runtime-logo` 描边改为 `color-mix(in srgb, currentColor 97%, transparent)`。

**③ 实测：三个 tile 的 computed `color` 与描边（我自己的探针）**

| 模式 | claude | deepseek | opencode |
|---|---|---|---|
| 暗 | `rgb(217,119,87)` | `rgb(87,134,254)` | `rgb(232,232,234)` |
| 亮 | `rgb(217,119,87)` | `rgb(87,134,254)` | **`rgb(18,18,18)`**（近黑标在亮色正确翻转） |

描边 computed = `color(srgb … / **0.97**)`（三个 tile 各自的品牌色，不再是中性 `rgb(173,178,187)`）。

**④ 实测：6 格对比度矩阵**（描边合成到卡片底后与卡片比；我按实测合成色独立复算）

| 模式 | claude | deepseek | opencode |
|---|---|---|---|
| 暗（卡底 `rgb(27,30,36)`） | **5.064** | **4.744** | **12.880** |
| 亮（卡底 `#ffffff`） | **3.013** | **3.220** | **17.582** |

**六格全部 ≥3.0 ✔**，最紧一格 = **亮色/claude 3.013** ⇒ R8（`view-runtimes.md:132`，已按 §12.10.4 写成 alpha 0.97）**达标**。
（与 t45 的 5.108/4.752/12.897/3.010/3.230/17.562 逐格同阶；差异 ≤0.05 来自 canvas 合成把 8 位小数取整 —— 两套数都 ≥3，结论一致。）
**⑤ 影响面**：行 5 可见描边元素数不变（graph 5 / runtimes 7，与 t13 同）；行 4 描边内容容器仍 0 ⇒ 只改颜色、不改结构 ✔。
## 3 F2 `#graph` 的 N+1（逐条复核）

**代码**：`Graph.tsx:58-62` 的注释与 `refresh()` 只取 `graphEntitiesAll()`；`:99-105` 的 `edges` 注释写明「Nothing is fetched here」；`:876-878` 检查器把已取到的 facts 交给画布（`onFacts`）。**首屏不再有任何 per-entity 请求。**
**实测（11.5s 窗口，request 事件计数，构建 `DYuzBlVy`）**：**总计 7 个 `/api` 请求** = 6×`/api/v1/permissions`（外壳 2s 轮询）+ **1×`/api/v1/graph/entities?limit=500`**；非 permissions 端点最大组 **1**、端点种类 `K = 1`、`R = 1`。
⇒ 行 28 的逐路由子断言 `最大组 1 ≤ 7` ✔；行 39 的 `R 1 ≤ 2K+2 = 4` ✔；`view-graph.md` 的 **G15（≤8）实测 7** ✔。
（t13 的 63 = 1 + 7 + **55** 个 `/graph/entity/<id>` —— 现在这条路径在首屏出现 **0** 次。）
**顺带修掉的潜伏 bug 已复核**：`Graph.tsx:96-105` 的 `rendered` 现在真的截断节点集（注释：「the cap used to apply to the prefetch only, so the node count was never actually bounded」）⇒ G10 的「超出只画连接最多的 150」成立 ✔。

## 4 F3 `--graph-protocol`、F4 命令面板、F5 Settings

### F3 —— 实现已落地，规格文本滞后
- **运行时实测**（`getComputedStyle(document.documentElement)`）：暗 **`#8fa3e8`** / 亮 **`#5b6fc4`**；旧值 `#ca9d33` 在 CSS 与运行时**都不再存在**（`grep graph-protocol` → `index.css:103` 与 `:154` 两处新值）。`--signal` 仍 `#f0a93b`（暗）/ `#c88413`（亮）⇒ 暖黄档与信号色**不再同色系** ✔。
- **旁证**：审计行 33 的 9 分类色最低值 暗 4.69 / **亮 4.65**（t13 时亮色 4.91，因换色略降）—— 仍 ≥3.0 ✔；行 3 信号占比 graph 0.36%（≤1.5%）✔。
- **⚠️ 规格文本仍把 `#ca9d33` 当现状**（MASTER §3.1 F8、§3.1 I「唯一的暖黄档」、`view-graph.md` §3、G4）⇒ **已由 t51 排队同步**。按 captain 的指示：**判「已实现」**，不因文档滞后判未实现。

### F4 —— 焦点陷阱与加载失败可见（两个子项都已修）
- **① 焦点陷阱**：`CommandPalette.tsx:242-260` 的 `onDialogKey` 在 `.cmdk` 内取可聚焦元素并把 Tab 循环在首/尾之间。**实测：连续按 45 次 Tab，`escapedAtTab = -1`（一次都没离开对话框）** —— t13 时第 35 次之后会落到背景。
- **② 加载失败可见**：`:119-137` 两个 `catch(() => {})` 改为 `setLoadErr(String(e))`；`:329-337` 渲染 `<div className="row" role="alert">` + 「加载失败」文案 + **重试按钮**（`refreshList`）✔（与行 20 同源）。文案键 `cmd.loadFailed` 已在 `i18n.tsx` 落地。
- **行 21/22 无回归**：审计行 21 = **0 缺口**（8/8 captures），行 22 = `BUTTON.kbd-hint`（Escape 后焦点回到触发元素）✔；我另实测 6 属性齐（`role=dialog` / `aria-modal=true` / `role=combobox` / `aria-controls=cmdk-list` / `aria-activedescendant=cmdk-opt-0` / `role=listbox` + 34×`role=option` 带 `aria-selected`）。

### F5 —— 注释不再说假话
`Settings.tsx:110-113`：注释写明「the confirmation is the line that stays… **There is deliberately NO toast here**」，`toast("ok", …)` 调用**已删除**（`grep toast src/views/Settings.tsx` 只剩 `useToast` 的 import 与失败路径的 `ErrorState`）。G13 的两半（常驻行 `setSavedAt` + 失败保留表单值 `setSaveError` 不动 policy）都还在 ✔。

## 5 五个面的验收条目回归（无回归）

**审计逐行（构建 `DYuzBlVy`，4 路由 × 暗/亮）**：行 1 `0.09/0.13/0.19/0.02%` · 行 2 `2.95/2.43/2.40/2.82%` · 行 3 `0.69/0.36/0.54/0.77%` · 行 4 `0` · 行 5 `7/5/7/6` · 行 6 `9/3/4/4` · 行 7 `32/32/32/20` · 行 8/9/10/11 `0` · 行 12 `27/18/21/27` · 行 13/14 `0` · 行 15 `20/18/19/19 环，最低 9.18/7.92/9.18/8.02（暗）· 5.74/5.74/5.74/5.75（亮）` · 行 16 `0/20 ×8` · 行 17 `0/37·0/21·0/34·0/26` · 行 18 `地板<24px 0 · 内容区<32px 0/0/0/2 · 外壳闭集 ✓` · 行 19 `最大 0px ×4 视口 ×2 模式` · 行 21 `0 缺口` · 行 22 `kbd-hint` · 行 24 `各 1 个 h1` · 行 26 `88/0/0/80ms（暗）` · 行 29 `2.47x` · 行 30 `ring #dee0e4 1.23/1.32` · 行 31 `3.1/3.41` · 行 32 `0 组重复，Δhue 12.8°/9.5°` · 行 33 `4.69/4.65` · 行 35 `CSS [520,768,1024,1240] + JS [992]` · 行 36 `228/72` · 行 37 `0/41·0/21·0/18·0/33` · 行 38 `0` · **行 39 `home R6≤14(K=6) · graph R1≤4(K=1) · runtimes R4≤10(K=4) · settings R2≤6(K=2)` 全 ✓**。

| 验收项 | 结论 | 证据 |
|---|---|---|
| 逐条核对规格条目 | ✅ | Graph **G1–G15**：G1/G2 行 31 `3.1/3.41` · G3 行 33 `4.69/4.65` · **G4 实现（token `#8fa3e8`/`#5b6fc4`，文本待 t51）** · G5 行 3 `0.36%` · G6/G7 行 6/7 `3/32px` · G8 `index.css` `clamp(320px,32vw,460px)` · G9 恒预留（t13 实测 460/673 前后一致，本轮未变） · G10 `Graph.tsx:96-105` 截断生效 · G11 `Graph.tsx:430/632-644` · G12 `:828-829` `role=img`+`aria-label` · G13 `:624-630` 停帧 · G14 `:786-789` DEV 门控 · **G15 实测 7 ≤ 8**。Runtimes **R1–R11**：R1 行 6/7 `4/32px` · R2 行 1/2/3 · R3 行 4 `0` · R4 行 5 `7` · R5 行 18 内容区 `0`（基线 16） · R6 行 17 `0/34` · R7 行 11/12 `0/21` · **R8 见 §2（6 格全 ≥3）** · R9 `Runtimes.tsx:214-216/308-318` · R10 边界（t13 实测 3+7=10） · R11 `:273-277` `role=alert`。Settings **G1–G13**：G1 行 6 `4` · G2 行 7 `20px` · G3 行 17 `0/26` · G4 行 16 `0/20` · G5/G6 行 4/5 `0/6` · G7 行 1/2/3 · G8 行 11/12 `0/27` · G9 `.distill-settings max-width 760`（t13 实测 1920 仍 760） · G10 `i18n.tsx` 「蒸馏策略」 · G11/G12/G13 见 §4 F5 |
| 层级中心 | ✅ | 审计行 6/7：graph `3/32px`、runtimes `4/32px`、settings `4/20px`（18px 次级读数在中心之下）、home `9/32px` |
| 四种状态 | ✅ | Graph `Spinner/Empty/ErrorState+canvasFailed 降级/noHits`；Runtimes `Spinner/ErrorState+stale/Empty/正常`；Settings `Spinner/ErrorState+重试/Select notFoundContent/正常`；**CommandPalette 现在四态齐**（空态 `.cmdk-empty`、缓存即时渲染、**失败 `role=alert` + 重试**、正常）；外壳在线/离线 banner |
| 四个断点不塌 | ✅ | 行 19：**我显式指定** 390/768/1280/1440，8 个 capture 全部「最大 0px」；行 36 `228`1440 / 72`768`；Settings 1920 仍 `760×726` |
| 共享原语而非硬编码 | ✅ | 内联 style 行 12 `27/18/21/27 ≤50`、内联 `font-size` 行 11 `0`、行 37 `0/41·0/21·0/18·0/33`（作者值越界 0）；组件 `ReadoutStrip/Empty/ErrorState/Spinner/Modal/useToast`；类名 `.readout*/.zone-*/.chat-field/.tag/.prompt-view/.row-btn/.runtime-logo`（视图私有已登记 README §3.5.1） |
| **跨文件一致性：sourceHue** | ✅ | **单一实现未变**：`Sessions.tsx:41-52` `export function sourceHue(s: string)`（claude-code→`var(--brand-claude)` / opencode→`var(--brand-opencode)` / deepseek→`var(--brand-deepseek)` / 其余→`var(--ant-color-text-tertiary)`）；**`Home.tsx:18` import 同一函数**、`Home.tsx:310` 与 `Sessions.tsx:323` 是仅有的两个调用点；`CommandPalette.tsx:12` 只 import `SOURCE_LABEL`（用文本、不用色相）⇒ **无第二份实现** |
| 不以主观感受作为通过理由 | ✅ | 本报告每条结论都带 `file:line` 或实测数字（含我自己的探针输出与审计 JSON） |

## 6 新发现（低危，转交不阻塞）

**N1（low）`role="alert"` 被放在 `role="listbox"` 里面**：`CommandPalette.tsx:328` 是 `<div className="cmdk-list" id="cmdk-list" role="listbox">`，而 F4② 新增的失败行 `:329-337` 是它的**子元素**。ARIA 要求 `listbox` 的直接子元素只能是 `option` / `group`（否则部分读屏会忽略该行或报结构错误）。
**requiredFix**：把失败行**移到 `.cmdk-list` 之前**（或给 `.cmdk-list` 加一层 wrapper，让 `role="alert"` 与 listbox 同级）；文案与重试按钮不变。**这是 F4② 的副作用，不是 t13 的遗留**，所以只登记、不改判。

## 7 转交与登记（都不是 t42 的交付问题）

| # | 事项 | 归属 |
|---|---|---|
| 1 | **t51**：F3 的四处规格文本同步（MASTER §3.1 F8 / §3.1 I / view-graph.md §3 / G4 的 `#ca9d33` → 新值） | 已在队列 |
| 2 | **t52**：行 1 的「按位置取数字」仪器缺陷（把基线列 `13.645` 当目标；`chat/dark` 读数与 t41 那次完全相同）—— **第三次同类**（行 6、行 18、行 1），需全表排查 | 已在队列 |
| 3 | **行 27 仍是全站唯一 FAIL**：构建 `DYuzBlVy` 总量 1289KB、地板 980KB、应用层（我的 §12.10.2 推导 = 总量 − 地板）**309KB > 预算 150KB** ⇒ 按 §12.10.2 **正确判 FAIL**。**但工具的 display 同时印出「应用层 1117KB」与「应用层 309KB」两个互相矛盾的数** —— 请 tools 对齐分类口径（`manualChunks` 已落地：`dist/assets/` 现有 16 个 chunk = antd 793494 / react 223639 / markdown 153056 / index 103565 / vendor 23386 / brand 4378 + 10 个路由 chunk） | tools |
| 4 | 结构断言 ①② 已满足（markdown 独立 chunk ✔、存在路由级 chunk ✔）—— 行 27 的剩余差距只在**应用层预算**，与五个面无关 | build/optimize |
| 5 | **N1**（§6） | ui-shell |
| 6 | 行 28 的 11.5s 全量复测（我只跑了 2.5s 窗口，`not_measured`）—— 按 §12.10.1 的新口径应全 PASS，`#home` 的 permissions 独立断言若 `Home.tsx` 未修则 FAIL | t8/tools |

## 8 未测量项（诚实登记）

| 项 | 原因 |
|---|---|
| 行 20（API 失败注入下的 `role=alert` + 重试） | 工具未实现失败注入（自标 `not_measured`）。**代码层面**三页 + 命令面板都已按规格写（`Runtimes.tsx:118-133`、`Settings.tsx:59-74`、`Graph.tsx:184-196`、`CommandPalette.tsx:329-337`），但我**没有**在注入失败下验证过 |
| 行 25 | 行的 scope 是 `#task`，与本组无关 |
| 行 28 的 11.5s 判定 | 本轮窗口 2.5s ⇒ `not_measured`（转交 t8 全量复测，见 §7.6） |
| 行 1 在 `#chat` 上的 FAIL | **未复现**：我跑的是 home/graph/runtimes/settings 四条路由，行 1 全 PASS（`0.09/0.13/0.19/0.02%`）。`#chat` 的 FAIL 是 captain 告知的**仪器缺陷（t52）**，我未独立复现，**不作为实现问题** |
| F3 的画布像素 A/B | t45 做过（旧 token 琥珀 165 px → 新 token 0 px）。我改用**运行时 CSS 变量 + 行 33 对比度 + CSS 源码**三条独立证据，未重跑像素普查 |

## 9 裁定

**pass**：t42 的五个 finding 全部独立复核通过（F1 视图侧 + t45 共享侧、F2、F3、F4 两个子项、F5），五个面的 38+ 条规格条目**无回归**（审计 35 pass / 1 fail / 3 not measured，唯一 fail 是全站性 bundle 行 27），跨文件 `sourceHue` 一致性保持单一实现。
**一条低危新发现 N1**（`role=alert` 在 `listbox` 内）与**五条转交/登记**不改变本裁定；F3 的规格文本滞后属 **t51**，按 captain 指示**不据此判未实现**。
