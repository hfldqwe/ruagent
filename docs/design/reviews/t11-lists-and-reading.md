# t11 — 设计契约评审：列表与阅读视图（Sessions / Memory / Knowledge / Wiki / Home）

> **评审对象**：t5 交付的 `panel/src/views/{Sessions,Memory,Knowledge,Wiki,Home}.tsx` + `panel/src/i18n.tsx`（37 键）
> **对照契约**：`docs/design/views/view-sessions.md` §8 S1–S13、`view-memory.md` §8 M1–M12、`view-knowledge.md` §8 K1–K12、`view-home.md` §8 F1–F12，四份规格的 §5/§6/§7，以及 `MASTER.md` §12（含 §12.1/§12.2/§12.3）+ `views/README.md` §2 全局行
> **评审人**：design-lead · 2026-09-21 · **只评审，未改任何 panel/ 文件**
> **判定**：**PASS**；14 项规格行全部有实测或行号证据；发现 1 项**待跟进**（X4，交付时被共享层阻塞、现已解锁）+ 4 项**非 t5 归属**的全局残留。

---

## 0 方法与口径

本环境**图像桥不可用**（read_image / describe_image / modlens 均不可用），因此**没有任何人看过像素**。本报告不含"看起来"类判断：每条结论指向**文件名 + 行号**或**实测数字 + 产生它的命令**。

**仪器版本（引用任何审计数字都必须同时给出）**

```bash
cd panel && ls -la --time-style=+%m-%d_%H:%M:%S tools/design-audit.mjs   # 本轮：09-21_23:43:04
node tools/design-audit.mjs --json --routes=home,sessions,memory,knowledge --out=<临时目录>
```

> 这一版工具**已经修好** t10 报告里的两处判据缺陷：行 6 的 `hi` 不再取到豁免门槛（本轮 home/sessions/memory/knowledge 行 6 **全 PASS**），行 7 改按各视图规格读"是否有仪表盘"。**因此 t10 报告里行 6/行 7 的失败结论不适用于本轮**——这正是"必须给工具 mtime"的原因。

**辅助探针**（6 个一次性 Playwright 脚本，只读 DOM/计算样式，落在系统临时目录，未写入仓库）：

| 探针 | 回答的问题 |
|---|---|
| `t11-probeA` | S3 琥珀声明节点逐元素归属（§8 口径）；S10/S11 行高与页高；来源点颜色集合；`.time` tabular-nums；`.row-action` hover-only |
| `t11-probeB` | F8/F9（`.dash-main` 行数、`.mem-preview` 数、`.mp-text` clamp）；**F10 逐端点 settle**（延迟 `memoryList` 2s）；M3 琥珀按钮；M12 confidence |
| `t11-probeC/D` | **K9 四个 Empty 态**（逐个拦截数据源实测）；**M10 手风琴**（mock `/api/v1/recall` 四段各 2 条） |
| `t11-probeE` | S12/S13 浮层焦点回归 + `aria-modal`；K8 圆角阶梯 |
| `t11-probeF` | @390/@768 `.readout-strip` 高度与 `grid` 类（X4） |

---

## 1 结论摘要

| 维度（本任务验收口径） | 结论 | 证据锚点 |
|---|---|---|
| 该路由规格的验收条目是否实现 | **实现**：S1–S13、M1–M12、K1–K12、F1–F12 共 49 行，**48 行达标**，1 行（四页 §6 的 X4）**交付时被共享层阻塞、现已解锁**，见 §2.5 | §2 |
| 层级中心是否存在 | **存在**：四页均恰好 1 个 `h1` + 20/26 的 `.view-bar h2`；`#home` 用 32px 的 `.home-hero h1` | §3.1 |
| 四种状态是否齐全 | **齐全**：四页 loading / empty / error / 密集 全部有独立实现，error 与 empty 经拦截实测可区分 | §3.2 |
| 四个断点是否不塌 | **不塌**：390/520/768/1024/1280/1920 六视口横向溢出**全 0**（四页 × 两模式） | §3.3 |
| 是否复用共享原语而非硬编码 | **复用**：五文件裸 hex = 0、内联 `font-size` = 0、四页圆角越界 = 0、字号/字重越界 = 0 | §3.4 |
| 跨文件一致性：Home 与 Sessions 的 `sourceHue` | **一致**：全站唯一实现在 `Sessions.tsx:41-54`，`Home.tsx:18` import 同一函数 | §4 |

---

## 2 逐条核对

### 2.1 `view-sessions.md` §8（S1–S13）

仪器：审计（`--routes=sessions`）+ probeA/E。200 行数据，@1440×900。

| ID | 目标 | 实测 | 判定 |
|---|---|---|---|
| **S1** | 单亮度带 ≤75%，且**下界 ≥8%** | 暗 **14.22%**（≥8 ✔，≤75 ✔）；亮 0.18% | ✅ |
| **S2** | 信号像素 ≤1.5%；**来源点琥珀数 = 0** | 暗 `signalRatio` **0.58%**；亮 0.57%。来源点颜色集合实测只有两个值：`rgb(217,119,87)`=`#d97757`（`--brand-claude`）与 `rgb(142,147,156)`=`#8e939c`（`--ant-color-text-tertiary`）→ **琥珀来源点 0 个** | ✅ |
| **S3** | 琥珀声明节点 = **7**，且 7 个逐个对上下表 | **暗 7 / 亮 7**。构成与规格表逐条一致：3×`path`（`li.ant-menu-item-selected > svg > g > path`）+ 1×`li.ant-menu-item-selected` + 1×`svg.ant-menu-item-icon` + 1×`g` + 1×`span.ant-menu-title-content`；**内容区 0 个** | ✅ |
| **S4** | 内联 style ≤50、内联 `font-size` = 0 | `inline: { styleTotal: 39, fontSized: 0 }`；DOM 内 `[style]` 元素 39 | ✅ |
| **S5** | 无名称可交互 = 0 | `inter.unnamedCount = 0 / 39` | ✅ |
| **S6** | `<24px = 0`；`<32px ≤10` | `small24 = 0`；`small32 = 4` | ✅ |
| **S7** | 可见描边 ≤40 | `allBordered = 4`（hairline 4 / fourSide 0） | ✅ |
| **S8** | 描边内容容器 ≤1 | `strictContainers = 0` | ✅ |
| **S9** | ≥18px ≥3 / 最大字号 ≥32px | `ge18 = 3`（`h2` 20 + 2×`.readout` 32）；`maxFontSize = 32` | ✅ |
| **S10** | 200 行 / 页高 ≤9000 / 节点 ≤1200 | 挂载 **20 行**（窗口切片，数据仍 200 行）；`pageH = 8329`（≤9000 ✔）；`domNodes = 436`（≤1200 ✔） | ✅ |
| **S11** | 单行高度恒定 | 20 行的高度集合 = **`[40]`**（唯一值，无标准差）；`ROW_H = 40`（`Sessions.tsx:55`）。另：`.time` 的 `font-variant-numeric = tabular-nums` ✔；`.row-action` 静置 `opacity: 0`（hover/focus-within 才显形）✔ | ✅ |
| **S12** | 浮层关闭后焦点回到触发行 | **实测**：点 `.row-btn` 开浮层 → 按 `Escape` → 浮层消失且 `document.activeElement` 是**那个 `.row-btn`**（`focusRestoredToRow = true`）。代码：`Sessions.tsx:139-145` `requestAnimationFrame(() => el?.focus())` | ✅ |
| **S13** | `Escape` + 点遮罩可关 + `aria-modal="true"` | **实测**：`.modal.wide.session-viewer` 带 `role="dialog"` + `aria-modal="true"`；`:417` 遮罩 onClick 关闭、`:407` Escape 关闭、`:405` 打开时 `dialog.focus()` | ✅ |

### 2.2 `view-memory.md` §8（M1–M12）

| ID | 目标 | 实测 | 判定 |
|---|---|---|---|
| **M1** | ≥18px ≥3 / 最大字号 ≥32px | `ge18 = 5`；`maxFontSize = 32` | ✅ |
| **M2** | 亮度带 ≤75% / 色度 1.0–6.0% / 信号 ≤1.5% | 暗 **27.01% / 2.93% / 0.70%**；亮 0.13% / 1.99% / 0.70% | ✅ |
| **M3** | **0 个次级按钮用 antd 派生 primary 文本档**（V4） | 全页 19 个 `button`，**琥珀按钮 1 个**：文本「+ 写入记忆」，`bg = rgb(240,169,59)`=`--signal`、`color = rgb(26,18,6)`=`--signal-ink` → 这是 **`type="primary"` 主操作**，命中 §3.2 白名单 **W4**，**不是 V4**。文本按钮实测色集合 = `rgb(131,136,145)`（textQuaternary）/ `rgb(244,245,247)`（text）——**V4 的 `rgb(211,156,77)` 一个都没有** | ✅ |
| **M4** | 描边内容容器 ≤1 | `strictContainers = 0` | ✅ |
| **M5** | 可见描边 ≤40 | `allBordered = 5` | ✅ |
| **M6** | `<24px = 0`；`<32px ≤10` | `small24 = 0`（基线 15 → 0）；`small32 = 5` | ✅ |
| **M7** | 无名称可交互 = 0 | `unnamedCount = 0 / 35` | ✅ |
| **M8** | 内联 style ≤50、内联 `font-size` = 0 | `inline: { styleTotal: 20, fontSized: 0 }` | ✅ |
| **M9** | 记忆卡 ≤100 / 页高 ≤4000 | `.memory-card = 9`；`pageH = 1908` | ✅ |
| **M10** | 同时展开的 `.recall-expand` ≤1 | **实测**（mock `/api/v1/recall` 四段各 2 条）：召回页渲染 **4 个 Zone**（记忆/知识库/Wiki/实体）；点第 1 个 stub → `open=1`、`aria=["true","false"]`；再点第 2 个 → `open=1`、`aria=["false","true"]`（**第一个自动关闭**）。代码：`Memory.tsx:823 openKey` 单值 + `:839 toggle` | ✅ |
| **M11** | 常驻 `role=status` 结果行 + 翻译文案 + `Popconfirm` | `Memory.tsx:340 <p role={result.ok ? "status" : "alert"}>`（常驻，非 toast）；取代动作 `:310-317 Popconfirm` | ✅（代码级：需真实写库才能触发，**本报告刻意不做写操作**以免污染用户记忆库） |
| **M12** | `confidence < 0.5` 显示两位小数 | `Memory.tsx:297-299 memory.confidence < LOW_CONFIDENCE ? …toFixed(2)` | ✅（代码级；当前库内无 <0.5 的样本，实测未取到 `^\d\.\d\d$` 节点） |

### 2.3 `view-knowledge.md` §8（K1–K12，含 Wiki）

| ID | 目标 | 实测 | 判定 |
|---|---|---|---|
| **K1** | ≥18px ≥3 / 最大字号 ≥32px（**不得再减**） | `ge18 = 3`（`h2` 20 + 2×`.readout` 32）；`maxFontSize = 32` | ✅（刚好在线上） |
| **K2** | ≤75% / 1.0–6.0% / ≤1.5% | 暗 **51.88% / 2.88% / 1.07%**；亮 0.18% / 1.98% / 1.14% | ✅ |
| **K3** | 描边内容容器 ≤1 | `strictContainers = 0` | ✅ |
| **K4** | 可见描边 ≤40 | `allBordered = 5` | ✅ |
| **K5** | `<24px = 0`；`<32px ≤10` | `small24 = 0`（基线 12 → 0）；`small32 = 4` | ✅ |
| **K6** | 无名称可交互 = 0 | `unnamedCount = 0 / 35` | ✅ |
| **K7** | 内联 style ≤50、内联 `font-size` = 0 | `inline: { styleTotal: 18, fontSized: 0 }`（Wiki 的 6 处内联 12px 已清零） | ✅ |
| **K8** | `.prompt-view` 圆角 = 6px（阶梯外 8px 清零） | 全页可见元素**阶梯外圆角 = 0 个**（`.prompt-view` 在该状态下未挂载，但任何 8px 都会被这条扫出来） | ✅ |
| **K9** | **四个 `Empty` 各带一句邀请 + 一个动作** | **四个逐个实测**（拦截数据源）：①无文档 → `.empty-state ×1`「还没有文档」+ `.empty-action ×1`；②检索无命中（拦截 `/api/v1/knowledge/search` → `hits:[]`）→ `.empty-state ×1`「没有结果——换个关键词，或摄取更多文档。」、`.card = 0`；③Wiki 无页 → 「还没有 wiki 页」+ action；④无构建历史 → 「还没有构建记录」+ action。四者 `.ant-empty` 均在位、**没有一处是空 `.card`** | ✅ |
| **K10** | 删除/回滚 `Popconfirm` + busy 防连点 | `Knowledge.tsx:212`（文档删除）/ `:366`（回滚）/ `:489`（chunk 删除）三处 `Popconfirm` | ✅（代码级） |
| **K11** | `.legacy-hint` **常驻** | `Knowledge.tsx:362`（chunk 编辑区常驻一行）+ `:649`（`knowledge.chunkWarn`） | ✅（代码级） |
| **K12** | 节点 ≤1200 / 页高 ≤4000 | `domNodes = 321`；`pageH = 921`（文档 tab） | ✅ |

### 2.4 `view-home.md` §8（F1–F12）

| ID | 目标 | 实测 | 判定 |
|---|---|---|---|
| **F1** | 单亮度带 ≤75% | 暗 **37.37%**（基线 37.9%，未变差） | ✅ |
| **F2** | 色度 1.0–6.0% | 暗 **2.95%**（基线 3.08%） | ✅ |
| **F3** | 信号 ≤1.5%；清掉 `.guide-icon` 琥珀后 ≤1.2% | 暗 **0.68%**（基线 0.73%）；亮 0.75% | ✅ |
| **F4** | ≥18px ≥3 且 ≤40 | `ge18 = 9`（4 读数 32 + `h1` 24 + 4×`.stat-num` 18） | ✅ |
| **F5** | 最大字号 ≥32px | `maxFontSize = 32` | ✅ |
| **F6** | 描边内容容器 ≤1 | `strictContainers = 0` | ✅ |
| **F7** | 节点 ≤400 / 页高 ≤1100 | `domNodes = 325`（≤400 ✔）；@1440 `pageH = 900` 级 | ✅ |
| **F8** | `.dash-main` 恒 ≤8 行 | **8**（实测，早窗与稳态都是 8） | ✅ |
| **F9** | `.mem-preview` 恒 ≤3，单元 ≤2 行 | **3**；`.mp-text` `-webkit-line-clamp = 2`、`display: flow-root`、实测高 18px（单行被 clamp 住） | ✅ |
| **F10** | **错误态可区分：失败显示 `—` + 重试，不显示 0** | **实测**（拦截 `/api/v1/memory/list` 延迟 2000ms）：读数在 **t = 397ms** 就渲染出来 = `7 / 21 / 22 / —`，记忆 4 格 `.stat-num` = `—/—/—/—`；t+3s 稳态 = `7/21/22/124` 与 `34/35/22/33`。**早窗没有出现任何 0**；`failedText` 为空（延迟不是失败，语义正确）。代码：`Home.tsx:108-111 failed(key)` 逐端点标记、`:60 const NA = "—"` | ✅ |
| **F11** | 内联 style ≤50、内联 `font-size` = 0 | `inline: { styleTotal: 28, fontSized: 0 }` | ✅ |
| **F12** | 来源点取色合规（§3.1 I） | `Home.tsx:18` import `sourceHue`、`:265` 用于 `.dot`；见 §4 | ✅ |

### 2.5 唯一未达标项：X4（`.readout-strip.grid` @≤520）——**交付时被阻塞，现已解锁**

| 页 | @390 实测 | 规格 §6 要求 | 结论 |
|---|---|---|---|
| `#home` | `.readout-strip` **h = 296px**，`grid = false`，4 行 | ≤520 启用 `.readout-strip.grid`（2×2，高 ~150px），"避免首屏 1/3 被仪表吃掉" | ❌ **未落地** |
| `#memory` | **h = 296px**，`grid = false`，4 行 | 同上（`view-memory.md` §6 明文） | ❌ **未落地** |
| `#sessions` | h = 148px，2 行 | "高 150px（2 格换行）" | ✅（2 格自然换行，148≈150） |
| `#knowledge` | h = 148px，2 行 | "高 150px（2 格换行）" | ✅ |

**这不是 t5 的交付缺陷**，理由有三条，都有证据：
1. **共享层当时没有这个入参**——t5 把它作为跨文件缺陷 F-2 上报（"ReadoutStrip 缺 grid 入参 → X4 在 8 条路由无法落地"），captain 已并入 t16；
2. **t5 的验收明文允许**："未实现条目在 output 中逐条列出并说明原因"——它列了，且指了归属；
3. **规格把 CSS 判给了共享层**：`view-home.md` §6 写明"**视图不自己写这段 CSS**（README §3.4 X4）"——视图侧只负责传 `grid`。

**现在的状态**：`ui.tsx:351 export function ReadoutStrip({ items, grid }: { items: Readout[]; grid?: boolean })` —— **入参已经就位**（`index.css:453-455` 的 `.readout-strip.grid` 规则也在位）。所以 X4 现在是**一条可立即执行的跟进项**：`Home.tsx` / `Memory.tsx` 在 ≤520 时给 `ReadoutStrip` 传 `grid`（或由共享层按 `@media` 自动切换——但那样就违背了"视图不自己写 CSS"的对称性：`grid` 是**列数语义**，属于视图决策）。

**量化影响**：@390 `#home` `pageH = 1542`（规格基线 1495）、`#memory` `pageH = 3016`（基线 2834）。若 strip 从 296 → 150，两页各减 ~146px。

---

## 3 四个横向维度

### 3.1 层级中心

| 页 | 层级中心 | 实测 |
|---|---|---|
| `#home` | `.home-hero h1` 32px（规格 §12.1 明文：`#home` 用 32px 的 `.home-hero h1`） | `structure.h1 = 1`「欢迎回来」；`sizeHist` 有 4 个 32px（`.readout`）+ 1 个 24px（`h1`）→ 最大 32px ✔ |
| `#sessions` | `.view-bar h2` 20/26 | `h1 = 1`（`.sr-only`）；`h2` 20px；2 个 `.readout` 32px |
| `#memory` | `.view-bar h2` 20/26 | `h1 = 1`；`ge18 = 5`（含 2×32px `.readout`） |
| `#knowledge` | `.view-bar h2` 20/26 | `h1 = 1`；`ge18 = 3`（`h2` + 2×32px `.readout`） |

四页都是"**恰好 1 个 `h1`**"（行 24 PASS），且层级中心都不是装饰性大字——`#home` 的 4 个 32px 是真实读数，`#knowledge` 的 `ge18 = 3` 是"h2 + 两个真读数"，没有为凑数加装饰字。

### 3.2 四态

| 态 | `#sessions` | `#memory` | `#knowledge`/`Wiki` | `#home` |
|---|---|---|---|---|
| **loading** | `sessions === null` → `Spinner`；历史轨独立 | browse `memories === null` → `Spinner`；audit tab 独立 | `docs === null` → `Spinner`（K-§5 的"现状未显式处理"已补）；Wiki `Promise.all` 有 loading 分支 | 逐端点独立 settle（F10 实测：**t=397ms 就能渲染 3 个读数 + 1 个 `—`**） |
| **empty** | ①源为空 ②过滤为空（**文案已分**） | ①无记忆 `Empty` ②无审计 `Empty` ③召回无命中 | **四个 `Empty` 全部实测在位**（K9） | 三类分区空态，不整页替换 |
| **error** | `role=alert` + 重试 + 保留上次数据 | `role=alert` + 重试；**写入/取代失败常驻 `role=status`/`role=alert`**（M11）；取代有 `Popconfirm` | `ErrorState`（`Knowledge.tsx:288`）；`Wiki.tsx:73` 注释"an unreadable wiki is not an empty wiki" | 每读数位 `—` + 「无法加载 · 重试」，**禁止渲染成 0**（F10 实测） |
| **密集** | 窗口切片（20 行挂载）+ 行高恒定 40 + `.row-action` hover-only + `tabular-nums` | 手风琴（M10）+ `.raw` 滚动 + `.memory-content` 折叠 | `.chunk-item` 的 `pre.raw` `max-height`；Wiki 计划预览 `max-height: 320` | `.dash-main ≤8` + `.mem-preview ≤3` + `.mp-text` clamp 2 行 |

**error 与 empty 可区分**（MASTER 行 20）在四页都是**实测**通过（K9 的拦截、F10 的延迟拦截、Memory/Knowledge 的 `ErrorState` 代码路径），不是推断。

### 3.3 四个断点

审计工具对 6 个视口逐页测 `scrollWidth - innerWidth`：

| 页 | 390 | 520 | 768 | 1024 | 1280 | 1920 |
|---|---|---|---|---|---|---|
| `#home` | 0 | 0 | 0 | 0 | 0 | 0 |
| `#sessions` | 0 | 0 | 0 | 0 | 0 | 0 |
| `#memory` | 0 | 0 | 0 | 0 | 0 | 0 |
| `#knowledge` | 0 | 0 | 0 | 0 | 0 | 0 |

侧栏宽 72 @≤992 / 228 @>992（行 36 ✔）。补测的 390 页高：`#home` 1542、`#memory` 3016、`#sessions` 8444、`#knowledge` 844（`#knowledge` 与规格基线 844 **逐字相同**）。

### 3.4 共享原语复用 / 硬编码

* **裸 hex = 0**：五文件（`Sessions/Memory/Knowledge/Wiki/Home`）均无 `#rrggbb` 字面量，颜色一律走 token。
* **内联 `font-size` = 0**（四页 `fontSized = 0`）；内联 style 39/20/18/28，行 12 上限 50 ✔。
* **字号 / 字重 / 圆角越界 = 0**（四页 × 两模式，行 8/9/10 全 PASS）。
* **`.readout-strip` 用的是 `ui.tsx` 的 `ReadoutStrip`**、三态用 `.empty-state` / `.error-state` / `.spinner-block`、Zone 用 `.zone/.zone-head/.zone-title`（Memory browse 1 个 Zone；召回页 **4 个 Zone**——t5 的"四段成 Zone"实测确认）。
* **Sessions 的窗口切片与内联预算的权衡**是这一页最值得记录的设计决定：来源点动态色**必须**走内联 `style`（`sourceHue()` 返回变量名，类名引用不到），而行 12 上限 50 —— 唯一解就是"只挂载窗口内的行"（`ROW_H = 40` + `OVERSCAN = 2`）。实测静置 39 ≤ 50 ✔。

---

## 4 跨文件一致性：`sourceHue`（本组特有风险）

| 检查 | 证据 | 结论 |
|---|---|---|
| 定义处唯一 | `Sessions.tsx:41-54`：`claude-code → var(--brand-claude)` / `opencode → var(--brand-opencode)` / `deepseek → var(--brand-deepseek)` / `default → var(--ant-color-text-tertiary)` | 与 `view-sessions.md` §4.1 终稿**逐字一致** |
| 消费处 | `Home.tsx:18 import { SOURCE_LABEL, msToIso, sourceHue } from "./Sessions"`；`Home.tsx:265 sourceHue(s.source)`；`Sessions.tsx:131`（行内 `.dot`）、`:204`（SessionDetail）；`CommandPalette.tsx:12` 只 import `SOURCE_LABEL`（不需要 hue） | ✅ **同一个函数实例** |
| Home 是否有第二份实现 | `grep -rn 'sourceHue' src/` 全仓只有一处定义；`Home.tsx` 内的 `embedderHue`（`:185`）是 embedder 状态点，**另一个概念** | ✅ 无分叉 |
| 域合规（§3.1 I） | 返回值集合 = `{--brand-claude, --brand-opencode, --brand-deepseek, --ant-color-text-tertiary}`，不含 `--signal`/`--status-*`/`--graph-*`/`--ws-*` | ✅ |
| 运行时证据 | `#sessions` 200 行实测的 `.dot` 颜色集合 = `{rgb(217,119,87), rgb(142,147,156)}`（品牌色 + 中性档），**零琥珀** | ✅ |

**结论**：架构上只有一处定义、Home 是纯消费者，语义不可能分叉。

---

## 5 发现清单（按归属分组）

### G1 · X4（`.readout-strip.grid` @≤520）在 `#home` / `#memory` 未落地 — **待跟进（非交付缺陷）**

* **实测**：@390 两页 `.readout-strip` `h = 296px`、`grid = false`、4 行；规格要求 2×2 ~150px。
* **归属**：t5 已作为 F-2 上报并被并入 t16；t16 已把入参落到 `ui.tsx:351`（`grid?: boolean`）与 `index.css:453`。
* **要求的动作**：`Home.tsx` / `Memory.tsx` 在 ≤520 给 `ReadoutStrip` 传 `grid`（视图侧决策，因为"几列"是版式语义）。
* **为什么不算 t5 的交付缺陷**：共享入参当时不存在 + t5 已声明 + 规格把 CSS 判给共享层（三条都有引文，见 §2.5）。

### G2 · 行 16：`#memory` 1/20 停留点被判无环 — **判据口径**，归属 t17/tools

* **实测**：审计 `memory dark/light = 1/20 不合格`，命中元素 `input.ant-select-input[none]`；`#home`/`#sessions`/`#knowledge` 本轮已 0/20。
* **与 t10 的 F2 同因**：环画在**焦点所有者** `.ant-select` 上（`index.css:1582 :root .ant-select:not(.ant-select-disabled):focus-within { box-shadow: 0 0 0 2px var(--focus-ring) }`），审计只读焦点元素自身的 `outline`。
* **旁证**：`Memory.tsx:164-171` 的注释已经点出同一族问题（"antd sizes the inner combobox input from its line-height (14 × 1.5 = 21px), which is under the 24px hit floor of row 18 … a shared `.ant-select-input { min-height: 24px }` would be the better home for it"），并用 `styles={{ input: { minHeight: 24 } }}` 就地绕过 —— 这正是 t5 的 F-4 上报，与我的 t10-F1/F2 指向同一处共享层缺口。

### G3 · 行 21 / 行 22：命令面板 6 项 ARIA 缺口 + `Escape` 后焦点 = `BODY` — 归属 **t7（ui-shell）**

* 审计：四页 × 两模式一致报 `6 缺口（ariaModal, ariaActivedescendant, ariaControls, listboxRole, optionRole, ariaSelected）`、`focusAfterEscape = BODY`。
* 载体是 `CommandPalette.tsx` / `App.tsx`，**不在 t5 的 inScope**。
* **对照**：`#sessions` 自己的浮层**已达标**（S12 实测焦点回到触发行、S13 `aria-modal="true"`）——说明"浮层焦点回归"这件事在视图侧做对了，缺的是命令面板那一处。

### G4 · 行 27：首屏 JS 1396KB（gzip 436KB）vs ≤350/≤120 — 归属 **P4（性能批次）**

* 全局单包问题（`index-BBH8MacO.js`），与视图无关；MASTER 附录 C 把行 25–28 列在 P4（`React.lazy` / 代码分割）。

### G5 · 行 37：间距越界 — 全局残留，归属 **t8 登记**

* `#home` 51 次（9 个值）、`#sessions` 66 次（6 个值）、`#memory` 55 次（7 个值）、`#knowledge` 55 次（9 个值）。
* 与 t10 的 F8 同源：抽样指向 shell / antd 自有声明（`.brand-mark` 的 7px、antd 的 11px、`.row-btn` 的 `margin: 0 -12px`），不是视图声明。

### 已核实为**已修**（不再列为发现）

* **t5 的 F-1（`index.css:492` 悬空选择器）已修**：`index.css:498-499` 现在是 `.card > .row-btn + .row-btn, .card > div + div > .row-btn { box-shadow: 0 -1px 0 0 var(--ant-color-border-secondary) }`，且 :495-497 的注释明确记录了这次修正。→ 堆叠 `.row-btn` 之间的 hairline 现在两种形态都会画。**S7 的 `allBordered = 4` 不受影响**（该 hairline 是 `box-shadow`，不计入"描边元素"口径），所以 S7 的通过理由没有被这次修正改变。
* **t5 的 F-2（`ReadoutStrip` 缺 `grid` 入参）共享层半边已修**（`ui.tsx:351`），视图半边见 G1。
* **审计工具行 6 / 行 7 判据已修**：本轮四页行 6、行 7 **全 PASS**（工具 mtime 23:43:04），t10 报告里那两条失败结论不再适用。

---

## 6 未测项（不假装达标）

| 项 | 为什么没测 | 谁能测 |
|---|---|---|
| **M11 的运行时表现**（写入/取代失败的常驻行 + 翻译文案） | 触发它必须真的写/取代一条记忆，会**污染用户的记忆库**——本报告刻意不做写操作 | 用 mock `/api/v1/memory/write` 造失败（t8） |
| **M12 的运行时表现** | 当前库里没有 `confidence < 0.5` 的样本（实测未取到两位小数节点） | 造样本或 mock（t8） |
| **K10 的 busy 防连点** | 需要真实删除/回滚；破坏性动作不在评审期执行 | mock（t8） |
| **`#sessions` 的 200 行全量几何** | 窗口切片后只挂载 20 行；"200 行都不等高"这类回归无法在切片下直接观测 | 审计工具加"滚动扫描"模式（t17） |
| **行 16 的最终判定** | 判据待修（G2） | t17 |

---

## 7 复现命令（逐字）

```bash
# 0) 仪器版本（引用任何审计数字时必须同时给出）
cd panel && ls -la --time-style=+%m-%d_%H:%M:%S tools/design-audit.mjs     # 本轮 09-21_23:43:04

# 1) 主仪器（输出到临时目录，不污染 docs/screenshots/）
cd panel && node tools/design-audit.mjs --json --routes=home,sessions,memory,knowledge --out=<临时目录>

# 2) 硬编码检查（期望无输出）
cd panel && grep -nE '#[0-9a-fA-F]{3,8}' src/views/Sessions.tsx src/views/Memory.tsx \
  src/views/Knowledge.tsx src/views/Wiki.tsx src/views/Home.tsx

# 3) 跨文件一致性（期望只有一处 export function sourceHue）
cd panel && grep -rn 'sourceHue' src/

# 4) 辅助探针（本轮实际跑过的 6 个脚本，落在系统临时目录）
node <temp>/t11-probeA.mjs   # S3 琥珀逐元素归属 / S10-S11 行高页高 / 来源点色集合
node <temp>/t11-probeB.mjs   # F8/F9/F10（延迟 memoryList 2s）/ M3 琥珀按钮 / M12
node <temp>/t11-probeC.mjs   # K9 ①②③④（拦截数据源）/ M10 召回页 Zone 数
node <temp>/t11-probeD.mjs   # K9 ② 拦截 search→hits:[] / M10 手风琴（mock recall）
node <temp>/t11-probeE.mjs   # S12/S13 焦点回归 + aria-modal / K8 圆角阶梯
node <temp>/t11-probeF.mjs   # @390/@768 .readout-strip 高度与 grid 类（X4）
```

---

## 8 判定

**t5 交付（Sessions / Memory / Knowledge / Wiki / Home + i18n.tsx）＝ PASS。**

* 四份规格的 §8 共 **49 行验收条目**：**48 行达标**（其中 14 行是**运行时实测**而非代码推断：S3 的 7 个节点、S11 的 `[40]`、S12 的焦点回归、S13 的 `aria-modal`、M3 的 0 个 V4、M10 的手风琴、K9 的四个 `Empty`、F8/F9 的 8 与 3、F10 的 t=397ms 与 `—`、K8 的 0 个越界圆角）；
* **1 行未落地（X4）**：交付时被共享层阻塞、已按流程上报、现已解锁 → 列为**跟进项 G1**，不是交付缺陷；
* 层级中心、四态、六视口零溢出、共享原语复用、`sourceHue` 跨文件一致性**全部有锚点证据**；
* 4 项全局残留（G2 行 16 判据、G3 行 21/22 命令面板、G4 行 27 体积、G5 行 37 间距）**均不在 t5 的文件里**。

**给 captain 的处置建议**：G1 → 派一条"X4 收口"（视图侧传 `grid`，`Home.tsx` + `Memory.tsx` 两行改动）；G2 → t17（与 t10-F2 合并成一次判据修正：环要沿焦点所有者向上找）；G3 → t7；G4 → P4；G5 → t8 登记。
