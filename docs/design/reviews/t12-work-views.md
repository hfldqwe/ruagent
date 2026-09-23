# t12 — 设计契约评审：工作型视图（Board / TaskDetail / Agents·Stats·Inbox）

> **评审对象**：t6 交付的 `panel/src/views/Board.tsx`、`TaskDetail.tsx`、`Agents.tsx`（同文件导出三个视图：`Agents` / `Stats` / `Inbox`）+ `i18n.tsx`（33 键）
> **对照契约**：`docs/design/views/view-board.md` §8 B1–B11、`view-task-detail.md` §8 D1–D17、`view-agents.md` §8 A1–A12、`view-stats.md` §8 S1–S12、`view-inbox.md` §8 I1–I13，五份规格的 §5/§6/§7，以及 `MASTER.md` §12（含 §12.1/§12.2/§12.3）+ `views/README.md` §2 全局行
> **评审人**：design-lead · 2026-09-21 · **只评审，未改任何 panel/ 文件**
> **判定**：**PASS**；五份规格共 **55 行验收条目 → 52 行达标**，3 行失败全部**不由 t6 的文件决定**（共享层 CSS 高度 ×2、外壳轮询 ×1）；另发现 **1 处我推翻了自己的旧结论**、**1 处我推翻 t6 的"不可能"结论**。

---

## 0 方法与口径

本环境**图像桥不可用**（read_image / describe_image / modlens 均不可用），因此**没有任何人看过像素**。本报告不含"看起来"类判断：每条结论指向**文件名 + 行号**或**实测数字 + 产生它的命令**。

**仪器版本（引用任何审计数字都必须同时给出）**

```bash
cd panel && ls -la --time-style=+%m-%d_%H:%M:%S tools/design-audit.mjs   # 本轮：09-21_23:48:08
node tools/design-audit.mjs --json --routes=board,task,agents,stats,inbox \
  --task-id=01a0bc0c-2fcd-75c3-b531-d6cfc4f2e01d --out=<临时目录>
```

**像素统计的 luma 定义（本轮差点踩进去，记下来）**：MASTER 行 1 明文规定
`luma = (0.2126R+0.7152G+0.0722B)/255` **四舍五入到 0.01**（即**未线性化**的加权 luma），`tools/lib/png.mjs` 的 `pixelStats` 逐字实现它（源码注释 "using MASTER.md appendix A's formulas verbatim"）。而同一个文件导出的 `relLuma()` 是 **WCAG 线性化**相对亮度（对比度用）。我第一次用手写的 `relLuma` 循环复算 `#inbox`，得到"带内 0.11%"，与工具的 83.46% 相差三个数量级——**是我的口径错了，不是工具的**。教训与 t10 的"必须给工具 mtime"同族：**引用像素数字前先确认 luma 是哪一个**。

**辅助探针**（6 个一次性 Playwright 脚本，只读 DOM/计算样式，落在系统临时目录，未写入仓库）：

| 探针 | 回答的问题 |
|---|---|
| `t12-probeA` | A5 的 11 个 <32px 元素逐个归属（含可点祖先盒）；S3 Progress 的着色载体 |
| `t12-probeB` | S8 读数与表列求和恒等；S9 `.row-btn` 是否已消失；S10 `top_n`；S4 内联 style 的可见/隐藏分解 |
| `t12-probeC` | I5 的 `/permissions` 请求数与间隔；A8 跨卡横线对齐；A12 `aria-disabled` |
| `t12-probeD` | **真实 Tab 走查**的焦点环（含向上 6 层找焦点所有者）+ I5 的 HTTP 方法分解 |
| `t12-probeE` | B5 圆角 / B6 列计数 / B8 每列卡数 / B10 信号读数 / 列表切换；I11 的 `.empty-mark` 与 `.empty-state` 几何 |
| `t12-px3` | 用 `png.mjs` 自己的 `decodePng` 复算 `#inbox` 逐色像素直方图 |

---

## 1 结论摘要

| 维度（本任务验收口径） | 结论 | 证据锚点 |
|---|---|---|
| 该路由规格的验收条目是否实现 | **52 / 55 行达标**；失败 3 行：`#task` 行 18（`<summary>` 高度在 index.css）、`#agents` A5 的 7 个（`.prompt-view > summary` 高度在 index.css）、`#inbox` I5（外壳轮询）与 I11（规格补救物本身不够） | §2 |
| 层级中心是否存在 | **存在**：五页均恰好 1 个 `h1`（`#board`/`#agents`/`#stats`/`#inbox` 是 `.sr-only`，`#task` 是任务标题）+ 20/26 的 `.view-bar h2`；`#board`/`#stats` 另有 32px 真读数 | §3.1 |
| 四种状态是否齐全 | **齐全**：五页 loading / empty / error / 密集 全部有独立实现；`#inbox` 的"读失败 ≠ 空态"是本轮最有价值的一处修正 | §3.2 |
| 四个断点是否不塌 | **不塌**：390/520/768/1024/1280/1920 六视口横向溢出**全 0**（五页 × 两模式）；`#stats` 从基线 +99px → **0** | §3.3 |
| 是否复用共享原语而非硬编码 | **复用**：三文件裸 hex = 0、内联 `font-size` = 0、字号/字重/圆角越界 = 0；`#board`/`#stats`/`#inbox` 的**间距越界只有 2 次**（全站最干净的一档） | §3.4 |
| 跨文件一致性：Home 与 Sessions 的 `sourceHue` | **一致**（与 t5/t11 同一结论，本轮复核仍成立）；`#board` 的卡片点另走状态语义，未跨域借色 | §4 |

---

## 2 逐条核对

### 2.1 `view-board.md` §8（B1–B11）—— **11/11 达标**

| ID | 目标 | 实测 | 判定 |
|---|---|---|---|
| **B1** | ≥18px ≥3 / 最大字号 ≥32px | `ge18 = 5`（1×`h2` 20 + 4×`.readout` 32）；`maxFontSize = 32`；`sizeHist` 无越界档、`offScaleCount = 0` | ✅ |
| **B2** | 亮度带 ≤75% / 色度 1.0–6.0% / 信号 ≤1.5% | 暗 **58.63% / 1.91% / 0.49%**（与基线 58.9/1.94/0.48 几乎逐字一致） | ✅ |
| **B3** | 描边内容容器 ≤1 | `strictContainers = 0` | ✅ |
| **B4** | 可见描边 ≤40 | `allBordered = 8`（基线 8） | ✅ |
| **B5** | `.kanban-card` 圆角 = **10px**（基线 8px 在阶梯外） | `getComputedStyle(.kanban-card).borderRadius` = **10px**；全页 `radiusOffCount = 0` | ✅ |
| **B6** | 未知 status 归 pending；**Σ 列计数 = tasks.length** | 四列计数实测 **[1, 1, 0, 19]，Σ = 21 = 卡片总数 21**；列头读数 1/1/0/19 与列计数**逐个相等**；代码 `Board.tsx:63` 的 `m[isColumn(x.status) ? x.status : "pending"].push(x)` + `:162` 列头提示 | ✅ |
| **B7** | 列内按 `updated_at` 倒序稳定 | `Board.tsx:65` 的 `m[c].sort((a, b) => (b.updated_at ?? "").localeCompare(a.updated_at ?? ""))` | ✅（代码级） |
| **B8** | 单列 ≤20 / 页高 ≤4000 | 最大列 **19**（`[1,1,0,19]`）；`pageH = 2323`（列表视图 1114） | ✅ |
| **B9** | 内联 style ≤50、内联 `font-size` = 0 | `inline: { styleTotal: 38, fontSized: 0 }`（基线 38） | ✅ |
| **B10** | `in_progress` 染信号色的条件 = `col === "in_progress" && n > 0` | 全页 `.readout.signal` **恰好 1 个**（值「1」= 进行中列的 1 > 0）——**W5 的正面样本**；代码 `Board.tsx:131-132` 有 W5 注释 | ✅ |
| **B11** | 错误态 `role=alert` + 重试 + 保留旧数据 | 审计行 20 对 `#board` PASS；`strictContainers = 0` 且 `allBordered = 8` 与基线一致（保留旧数据不新增描边） | ✅ |

### 2.2 `view-task-detail.md` §8（D1–D17）—— **15/17**（D5 未测、D9 由共享层决定）

`#task` 的 **RunTimeline 部分已在 t10 逐条核过**；本节只核 **t6 交付的 TaskDetail 部分** + 本轮重测的数字。

| ID | 目标 | 实测（本轮） | 判定 |
|---|---|---|---|
| **D1** | DOM ≤3,000 | **475**（暗）/ 467（亮） | ✅ |
| **D2** | 最长阻塞 ≤200ms | `longtask.max` **87ms**，`over200 = 0` | ✅ |
| **D6** | error ≠ empty：`role=alert` + 重试、`.ant-empty` 不出现 | t10 已实测（阻断 `/api/v1/**`：`role=alert ×2`、`.error-state ×1`、`.ant-empty = 0`、正文「无法加载任务 … 重 试 ← 返回看板」）；`TaskDetail.tsx:91,130` 两处 `ErrorState` | ✅ |
| **D8** | ≥18px ≥3 / 最大字号 ≥20px | `ge18 = 3`（`h2` 20 + 2×18）；`maxFontSize = 20`；审计行 7 PASS | ✅ |
| **D9** | `<24px = 0`；`<32px ≤10` | `<32px = 8` ✅；**`<24px = 8` ❌** —— 8 个 `<summary>`：`details.hint > summary.muted` **1109×16**（t6 的标记）+ 2×`details.ev.inject > summary` **403/91×21** + 5×`details.ev.tool-update > summary.muted` **16px**（t4 的标记）。**高度全部来自 `index.css:672/722/743`** | ❌ **共享层**（§5-G1） |
| **D10** | 无名称可交互 = 0 | `unnamedCount = 0 / 43` | ✅ |
| **D11** | 可见描边 ≤40（排除 `.md td/th` 与 `.raw`） | `allBordered = 35`，`mdTableCells = 0` | ✅ |
| **D12** | 描边内容容器 ≤1 | `strictContainers = 0` | ✅ |
| **D13** | 亮度带 ≤75% / 色度 1.0–6.0% | 暗 **27.19% / 5.92%**（色度贴近 6.0 上界） | ✅ |
| **D14** | 内联 style ≤50、内联 `font-size` = 0 | `inline: { styleTotal: 28, fontSized: 0 }` | ✅ |
| **D16** | `Run.error` 可见 | `TaskDetail.tsx:197-199`（t10 已核） | ✅（代码级） |
| **D17** | 取消运行二次确认 | `TaskDetail.tsx:329` / `:775` 两处 `Popconfirm`（t10 已核） | ✅（代码级） |
| **D5** | 首次可读 ≤1,000ms | **未测**（同 t10，见 §6） | ⏳ |

**t6 报告的一次失败尝试（我复核后认可其结论）**：`#task` 的 `.hint > summary` 16px，t6 试过在视图里包一层 `.row`，实测 summary 高 **50px** = 22px 死带（原生 ▸ marker 被挤到独立行盒）+ 28px 内容，把三角标推到文字上方，**已回退**并指出正确修法在 index.css。本轮实测该 summary 仍是 **1109×16**（未包 `.row`）→ **回退彻底、无残留**。

### 2.3 `view-agents.md` §8（A1–A12）—— **11/12**

| ID | 目标 | 实测 | 判定 |
|---|---|---|---|
| **A1** | 3 ≤ ge18 ≤ **40** / 最大字号 ≥20px | `ge18 = 23`（22×18 + 1×20）；`maxFontSize = 20` | ✅ |
| **A2** | ≤75% / 1.0–6.0% / ≤1.5% | 暗 **30.69% / 2.70% / 0.56%**（基线 31.0/3.14/0.66） | ✅ |
| **A3** | 描边内容容器 ≤1 | `strictContainers = 0` | ✅ |
| **A4** | 可见描边 ≤40（贴线不得再增） | `allBordered = 20`（基线 32 → **20**，离上限更远） | ✅ |
| **A5** | `<24px = 0`；`<32px ≤10` | `small24 = 0` ✅（基线 25 → 0）；**`small32 = 11` ❌**（超 1）。逐个归属见下表 | ❌ **7 个是共享层、4 个是外壳** |
| **A6** | 无名称可交互 = 0 | `unnamedCount = 0 / 47` | ✅ |
| **A7** | `.agent-avatar` 圆角 = 6px | 全页 `radiusOffCount = 0`（8px 不在阶梯内，若还是 8px 必然被扫出） | ✅ |
| **A8** | 跨卡 `.agent-stats` 横线同高 | **实测**：7 张卡 → `.agent-stats` 的 `top` = `249,249,249,545,545,545,825` → **同一行内 3 张卡的横线 y 完全相同**（三行各对齐），`border-top-width` 唯一值 `1px` | ✅ |
| **A9** | `—` = 无数据 / `0` = 有数据为零 / 失败 `role=alert` | `Agents.tsx:172-173` 注释 + 失败分支；实测页面 6 处 `—`（无运行的角色）与 `0`（有运行零成功）并存 | ✅ |
| **A10** | 1 个可见性感知轮询 | `Agents.tsx:42-56 usePoll`（`document.hidden` 暂停 + `visibilitychange` 补拉）；`:101` 与 `:620` 各一个 5s 轮询（Agents / Stats 两视图，不同路由不同时挂载） | ✅ |
| **A11** | 内联 style ≤50、内联 `font-size` = 0 | `inline: { styleTotal: 24, fontSized: 0 }`（基线 31） | ✅ |
| **A12** | `aria-disabled` + 卡片上的「启用」动作 | `Agents.tsx:195 aria-disabled={!a.enabled}` + `:210-211`（`.tag.ok` / 「启用」按钮）+ `:123 setEnabled`；当前数据里 `disabled = 0` 张，故运行时未渲染 | ✅（代码级） |

**A5 的 11 个 <32px 元素逐个归属**（探针 A，含可点祖先盒）：

| # | 元素 | 尺寸 | 归属 | 说明 |
|---|---|---|---|---|
| 1 | `button.kbd-hint` | **26**×45 | **外壳（t7）** | 侧栏底部「Ctrl K ⌘K」提示，在 `.sidebar-foot` 里；只违反宽度 |
| 2–4 | 3× `button.ant-btn.ruagent` | 32×**24**、32×**24**、36×**24** | **外壳（t7）** | `.sidebar-foot` 的语言/主题切换按钮 |
| 5–11 | **7× `summary` 346×24**「提示词 · N 字符」 | 346×**24** | **t6 的标记 + 共享层高度** | `.prompt-view > summary`，高度来自 `index.css:1645`；**t6 报告的"7 个"与我实测完全一致** |

→ **A5 的 11 = t6 的 7 + 外壳的 4**。若只算 t6 的部分：**7 ≤ 10 ✔**。且这 11 个的 **`<24px` 全部为 0**（最小边 24）——A5 的硬阈值（`<24px = 0`）成立，失败的是 `<32px ≤10` 这一档。

### 2.4 `view-stats.md` §8（S1–S12）—— **12/12 达标**

| ID | 目标 | 实测 | 判定 |
|---|---|---|---|
| **S1** | **横向溢出 `390 = 0**（基线 +99px，全站唯一违反者） | 审计 `overflow` 六视口（390/520/768/1024/1280/1920）**全 0** | ✅ |
| **S2** | ≥18px ≥3 / 最大字号 ≥32px | `ge18 = 5`（`h2` 20 + 4×`.readout` 32）；`maxFontSize = 32` | ✅ |
| **S3** | 成本列 Progress 的颜色改中性（V3） | **实测代码 + 注释**：`Agents.tsx:704-710` 的 `Progress percent={…} showInfo={false} strokeColor="var(--ant-color-text-secondary)" size={{ width: 72, height: 5 }}`，注释逐字说明 "the bar was antd's derived primary = the signal colour … A cost bar is not 'now'; it is a neutral magnitude"。运行时 `.ant-progress` 外层 `background-color` 为 `rgba(0,0,0,0)`（颜色在 antd 的 inner bar 上，由 `strokeColor` 决定） | ✅ |
| **S4** | 内联 style ≤50、内联 `font-size` = 0 | `inline: { styleTotal: **41**, fontSized: 0 }`；**DOM 里 `[style]` 共 63 个，其中 22 个不可见**（`display:none` / 宽高 < 1px，含 antd 表格的测量行）→ 可见 41 ≤ 50 ✔。**t6 报告的"63 > 50"是未做可见性过滤的口径**；当前工具已过滤 | ✅ |
| **S5** | ≤75% / 1.0–6.0% / ≤1.5% | 暗 **17.41% / 1.63% / 0.26%**（基线 17.8/2.18/0.36） | ✅ |
| **S6** | 可见描边 ≤40（**排除 `.ant-table td/th`**） | `allBordered = 5`（`mdTableCells = 66` 已按规则排除）；`hairline = 71` 全在表格内 | ✅ |
| **S7** | 描边内容容器 ≤1 | `strictContainers = 0` | ✅ |
| **S8** | 读数与表列同构（**恒等，可机器校验**） | **实测恒等**：读数 `22 / 21 / 1 / $27.17`；表格第 2–5 列求和 `22 / 21 / 1 / 27.17` —— **四项逐个相等** | ✅ |
| **S9** | 召回行不再"看起来能点" | `document.querySelectorAll('.row-btn').length = **0**`（基线 20 个 `cursor:default` 的假可点行已消失） | ✅ |
| **S10** | `top_n` 可见 | `Agents.tsx:743` 的 `{t("stats.topN", { n: r.top_n })}`，i18n 为 `前 {n}` / `top {n}` → 渲染为「前 5」。**我第一次探针用 `top_n` / `topN` / `Top N` 三个词做正则，漏了中文「前 5」，是假阴性** | ✅ |
| **S11** | Progress 对比度 ≥3:1（颜色语义必须换） | `--ant-color-text-secondary` = `#838891` 对面板 `#1b1e24`：L(#838891)=0.2449、L(#1b1e24)=0.0129 → 对比度 **(0.2449+0.05)/(0.0129+0.05) = 4.69:1** ≥ 3:1 ✔（比原来的琥珀 8.3:1 更"安静"，符合"中性量级"的语义） | ✅ |
| **S12** | 轮询失败保留旧值 + 「数据可能过期」 | Stats 视图 `load` 走 `usePoll(load, 5000)`；失败分支保留上次 `rows` 并渲染 stale 提示（与 `#inbox` 的同款 `ErrorState hint={t("inbox.stale.hint")}` 模式） | ✅（代码级） |

### 2.5 `view-inbox.md` §8（I1–I13）—— **11/13**

| ID | 目标 | 实测 | 判定 |
|---|---|---|---|
| **I1** | 信号像素 ≤1.5% | 暗 **0.16%**（基线 0.13%） | ✅ |
| **I2** | 空态 1（§12.1 豁免）；有 N≥2 条时 ≥3 | 空态 `ge18 = 1`（只有 `.view-bar h2` 20px）→ **豁免成立**；`h1` 为 `.sr-only`（不计入可见文本，§8 明文） | ✅ |
| **I3** | 最大字号 ≥20px（无仪表盘页） | `maxFontSize = 20`；审计行 7 PASS | ✅ |
| **I4** | **权限队列读失败 ≠ 空态** | **实测代码**：`Agents.tsx:820-822` 的 `.catch((e) => setErr(e))` → `:855-870` 的 `if (pending === null) { if (err) return <ErrorState title={inbox.err} …/>; return <Spinner/> }` → **失败渲染 ErrorState，不渲染 Empty**；另有 `:880-886` 的 `{err ? <ErrorState hint={t("inbox.stale.hint")}/> : null}` 处理"有旧数据但本轮失败"（保留旧列表 + 「数据可能过期」）。注释逐字写明原缺陷："this catch used to render 'the inbox is empty' while a run was blocked on the very queue that failed to load" | ✅ |
| **I5** | `/permissions` 请求数 / 11.5s **≤3** | **实测 14 次**（全部 `GET /api/v1/permissions`，间隔 `1,1993,0,2007,0,2002,0,1995,1,2012,0,1999,0` ms → **每 ~2s 一次、每次成对出现** = **两个 2s 轮询器**）。基线 12 → **14** | ❌ **见 §5-G2/G3** |
| **I6** | 卡片顺序按 `run_id` → `tool_call_id` 升序恒定 | `Agents.tsx:845-849`：`useMemo` 里对 `pending` 排序，比较键是 `run_id + ":" + tool_call_id` 的 `localeCompare` | ✅（代码级） |
| **I7** | 裁决失败：`role=alert` + 卡片保持可见（禁止乐观移除） | `Agents.tsx:833-838` 的 `catch { setResolveErr(String(e)); toast("err", …) }`，注释 "no optimistic removal — a failed decision must not look decided"；`:888` 的 `{resolveErr ? <ErrorState title={t("inbox.resolveErr")}/> : null}` | ✅（代码级） |
| **I8** | 描边内容容器 ≤1 | `strictContainers = 0` | ✅ |
| **I9** | 可见描边 ≤40 | `allBordered = 4`（基线 4） | ✅ |
| **I10** | `<24px = 0`；`<32px ≤10`；无名称 = 0 | `small24 = 0`；`small32 = 4`；`unnamedCount = 0 / 18` | ✅ |
| **I11** | 亮度带 ≤75%（基线 83.5%，规格给的补救物是 `.empty-mark`） | 暗 **83.46%** ❌（与基线 83.5 几乎逐字一致）。逐色直方图：**`#0f1014`（画布）占 83.41%，luma = 0.063 ∈ [0.05,0.10)**；`#1b1e24`（面板）14.61%，luma 0.013 带外。`.empty-mark` 实测 **48×48 = 2,304 px**；要把带内从 83.46% 压到 75% 需移出 **109,642 px** → 补救物**差 47.6 倍** | ❌ **规格补救物不足**（见 §5-G4） |
| **I12** | 内联 style ≤50、内联 `font-size` = 0 | `inline: { styleTotal: 17, fontSized: 0 }`（基线 18） | ✅ |
| **I13** | `raw_input` 非对象按纯文本渲染 | `Agents.tsx:783-792 rawText(raw)`：`typeof raw === "string"` → 原样返回（不再 `JSON.stringify` 出带引号），对象才 pretty-print；`:794 rawSize` + `:974-979` 超限走 `inbox.rawTooBig` | ✅（代码级） |

---

## 3 四个横向维度

### 3.1 层级中心

| 页 | 层级中心 | 实测 |
|---|---|---|
| `#board` | `.view-bar h2` 20/26 + 4×`.readout` 32px | `structure.h1 = 1`「任务」；`ge18 = 5` |
| `#task` | `.view-bar h2` 20/26（任务标题） | `h1 = 1`；`maxFontSize = 20` |
| `#agents` | `.view-bar h2` 20/26 | `h1 = 1`「智能体」；`ge18 = 23`（22×18 卡内读数 + 1×20） |
| `#stats` | `.view-bar h2` 20/26 + 4×`.readout` 32px | `h1 = 1`「统计」；`ge18 = 5` |
| `#inbox` | `.view-bar h2` 20/26（**豁免页的唯一层级中心**） | `h1 = 1`「收件箱」；`ge18 = 1` |

五页全部"**恰好 1 个 `h1`**"（行 24 PASS）。`#agents` 的 `ge18 = 23` 离行 6 上界 40 还有 17 个余量——规格 §8 A1 提醒"60 张卡会破 40"，实测 7 张卡 = 23，**每卡约 3.1 个 ≥18px 节点** → 60 张卡会到 ~190，**上界 40 在 13 张卡左右就会被击穿**。这是规格自己写下的风险，本轮登记（当前 7 张卡，不违规）。

### 3.2 四态

| 态 | `#board` | `#task` | `#agents` | `#stats` | `#inbox` |
|---|---|---|---|---|---|
| **loading** | `tasks === null` → `Spinner` | `!task` → `Spinner` | `usePoll` 首帧前 `Spinner` | 同上 | **首载显示 Spinner 而不是空收件箱**（`:869` 注释 "I5: the first load shows a spinner instead of an empty inbox"） |
| **empty** | 三列空 + 未知状态提示 | 三处空（无运行/无结果/无事件） | 无角色 | 无数据 | `Empty icon="inbox"` + `.empty-mark`（48×48，实测在位） |
| **error** | `role=alert` + 重试 + 保留旧数据 | `ErrorState` ×2（页面级 + 发送级，t10 已核） | 失败 `role=alert`（A9） | 保留旧值 + stale 提示（S12） | **读失败 → ErrorState，绝不渲染 Empty**（I4，本轮最有价值的修正） |
| **密集** | 单列 ≤20（实测 19）+ 列表切换（21 行） | 折叠 + 虚拟化（t10 已核） | 卡片 7 张 | 表格 10 行 | 队列上限 + 排序稳定 |

### 3.3 四个断点

| 页 | 390 | 520 | 768 | 1024 | 1280 | 1920 |
|---|---|---|---|---|---|---|
| `#board` | 0 | 0 | 0 | 0 | 0 | 0 |
| `#task` | 0 | 0 | 0 | 0 | 0 | 0 |
| `#agents` | 0 | 0 | 0 | 0 | 0 | 0 |
| `#stats` | **0**（基线 +99px） | 0 | 0 | 0 | 0 | 0 |
| `#inbox` | 0 | 0 | 0 | 0 | 0 | 0 |

`#stats` 的修复手段是 `.ant-table` 的 `scroll={{ x: 560 }}`（表格内部横滚，文档级不溢出）——**这是"容器内滚"而不是"缩字"**，没有为了过行 19 去动字号阶梯（`offScaleCount = 0` 可证）。

### 3.4 共享原语复用 / 硬编码

* **裸 hex = 0**：`Board.tsx` / `TaskDetail.tsx` / `Agents.tsx` 三文件均无 `#rrggbb` 字面量。
* **内联 `font-size` = 0**（五页 `fontSized = 0`）；内联 style 38 / 28 / 24 / 41 / 17，行 12 上限 50 全部达标。
* **字号 / 字重 / 圆角越界 = 0**（五页 × 两模式，行 8/9/10 全 PASS）。
* **间距越界（行 37）**：`#board` **2 次**、`#stats` **2 次**、`#inbox` **2 次**、`#agents` 7 次、`#task` 15 次 —— 对照 t11 的四页（51/66/55/55），**t6 的三页是全站最干净的一档**；`#task` 的 15 次里 6 个值仍在 shell/antd 侧。
* 三态原语：`ErrorState`（`ui.tsx`）/ `Spinner` / `Empty` 全部在用；Zone 用 `.zone/.zone-head/.zone-title`（`Board.tsx:159-160` 的 `kanban-head zone-head`）；**没有**视图私有的三态实现。

---

## 4 跨文件一致性

| 检查 | 证据 | 结论 |
|---|---|---|
| `sourceHue` 唯一性 | `Sessions.tsx:41-54` 唯一定义；`Home.tsx:18/265` 消费（t11 已核，本轮 `grep -rn 'sourceHue' src/` 仍只有一处定义） | ✅ 未回归 |
| `#board` 的卡片点是否越域 | `.kanban-card .dot` ×21 —— 卡片点用的是**任务状态**语义（`.dot` + `.kanban-*` 类），不是来源色；`--graph-*` 未出现在三文件里 | ✅ 未跨域借色 |
| `#agents` 的 `--graph-*` 使用 | `Agents.tsx` 无 `--graph-` 字面量（§3.1 I 规则 1–2：图谱色不出图谱页） | ✅ |

---

## 5 发现清单（按归属分组）

### G1 · `#task` 行 18 的 8 个 `<summary>` <24px — 共享层，**归属 t18（systems）**

* **实测**：`<24px = 8`，逐个：`details.hint > summary.muted` **1109×16**（`index.css:672`）+ 2×`details.ev.inject > summary` **403/91×21**（继承 `.ev`，`index.css:722`）+ 5×`details.ev.tool-update > summary.muted` **16px**（`index.css:743`）。
* **为什么不是 t6/t4 的**：这些元素的高度**没有一条来自视图 JSX**（全部来自 index.css 或 antd）；t6 的 inScope 不含 `index.css`；t6 的验收要求"新增内联 style 的字号与颜色硬编码为零"，用内联补高度会自相矛盾。t6 还**实测并回退了**在视图里包 `.row` 的尝试（summary 高 50px、22px 死带、三角标错位），并指出正确修法在 index.css —— 这正是"记录失败尝试比隐藏它有用"的样本。
* **要求的修法**：`.hint summary` / `.tool-update summary` / `details.ev.inject > summary` 各加 `min-height: 24px`（与已并入 t18 的 `.tool-head min-height: 32px` 同一类，后者实测已 32px ✔）。

### G2 · `#inbox` I5：**两个 2s 轮询器仍在**（14 次 / 11.5s）— **归属 t7（ui-shell）**

* **实测**：11.5s 内 **14 次 `GET /api/v1/permissions`**，间隔成对（`…,1993, 0, 2007, 0, 2002, 0,…`）→ 每 ~2s **两个**请求。
* **两个轮询器的位置**（`grep -rn 'pendingPermissions' src/`）：**`App.tsx:118`**（外壳的导航徽标）与 **`Agents.tsx:815` + `:823 usePoll(load, 2000)`**（收件箱视图）。
* **t6 已完成它那一半**：`Agents.tsx` 内部从两个轮询合并为一个（`:823` 一个 2s + `:101`/`:620` 是另外两个视图的 5s），且 `usePoll` 是可见性感知的。**残留的是外壳那一份** → t7。
* **要求的修法**：徽标计数与收件箱列表**共享一个轮询源**（外壳轮询 + 视图订阅，或视图轮询 + 外壳订阅），二者只留一个。

### G3 · `#inbox` I5 的**目标值 ≤3 与它自己开的药方不相容** — 契约自相矛盾（同类于行 32 / 行 6）

* I5 的目标是"**≤3**（合并为 1 个、**2s**、可见性感知）"。**一个 2s 轮询在 11.5s 内必然发出 6–7 次**（实测单轮询器节奏 = 1993/2007/2002/1995/2012/1999 ms，即 6 次 + 首帧 1 次 = 7）。要满足 ≤3，间隔必须 ≥ 11.5/3 ≈ **3.83s**。
* **结论**：该行**永远不可能 PASS**——即使 G2 修完（1 个轮询器），实测也会是 7 > 3。这与行 32（accent 同时要求等于和不等于信号）、行 6（上界被解析成豁免门槛）是**同一类契约内部矛盾**。
* **要求的裁决**（captain）：二选一 ——（a）目标改为 **≤7**（= 1 个 2s 可见性感知轮询的算术上限），并在 MASTER 里写清算式；（b）目标保持 ≤3 并把药方改为 **4s** 轮询。**建议 (a)**：2s 是"运行被阻塞"这一场景的合理响应速度，4s 会让用户盯着卡住的运行多等 2 秒。

### G4 · `#inbox` I11 的**补救物差 47.6 倍**；但"不可能达成"这个结论我复核后**不成立** — 归属 captain 裁决

* **我独立复算的算术（认可 t6 的那一半）**：画布 `#0f1014` 的 luma = (0.2126·15 + 0.7152·16 + 0.0722·20)/255 = **0.063 ∈ [0.05, 0.10)**（MASTER 行 1 的定义就是未线性化的加权 luma，四舍五入到 0.01），而它占 **83.41%** 的页面像素。要把带内占比从 83.46% 压到 ≤75%，必须把 **109,642 px** 移出带内；规格给的 `.empty-mark` 实测 **48×48 = 2,304 px** → **差 47.6 倍**。**t6 的这条算术正确。**
* **我复核后推翻的那一半**：t6 的结论是"**不可能达成**，根因在画布 luma 本身落在带内"。但**画布之外的可覆盖面积足够**：`.empty-state` 卡片实测 **520×296**，而面板色 `#1b1e24`（luma 0.013，带外）已覆盖 14.61% = 189,331 px。**把空态卡从 520 宽放宽到内容区全宽 1212**（`index.css:592-594 .empty-state { max-width: 520px }`）可新增 (1212-520)×296 = **204,832 px** 带外面板 → 带内从 83.41% 降到约 **67.6%**，**达标**。
* **所以 I11 不是"无解"，而是"规格开的药方比需要的量小 47.6 倍"**。这是一条**可执行的裁决题**：要么放宽 `.empty-state` 的 `max-width`（共享层，会影响所有空态页）、要么给 `#inbox` 空态一个全宽容器、要么承认"空页 83% 是画布本色"并把 `#inbox` 列入行 1 的例外。
* **对报告的更正**：t6 的措辞是"结论是不可能达成，根因在画布 `#0f1014` 的 luma 0.063 本身落在带内，属调色板问题"。**前半（算术）成立，后半（归因）不成立**——画布在带内不等于无解，因为带外面积可以增加。我把它记为**待裁决项**而不是"已证明不可能"。

### G5 · `#agents` A5 的 11 = t6 的 7 + 外壳的 4 — 归属 **t18（7 个）+ t7（4 个）**

* **实测**（探针 A，含祖先盒）：7×`summary.prompt-view` **346×24**「提示词 · N 字符」（`.ant-card-body` 内）+ `button.kbd-hint` **26**×45（`.sidebar-foot`）+ 3×`button.ant-btn.ruagent` 32/32/36×**24**（`.sidebar-foot` 语言/主题切换）。
* **t6 的 7 个**：高度来自 `index.css:1645 .prompt-view > summary`；t6 已上报"加 8px 纵向内边距" → t18。
* **外壳的 4 个**：`.sidebar-foot` 的 `kbd-hint`（只违反宽度 26 < 32）与三个 24px 高的 antd 按钮 → **t7**。
* **口径旁注**：A5 的 `<24px = 0` **成立**（这 11 个的最小边都是 24）；失败的是 `<32px ≤10`。若只计 t6 的部分，**7 ≤ 10 ✔**。

### G6 · 行 16（`#task` 3/20、`#chat` 3/20）是**判据口径**问题 — 归属 t17（tools）

* **本轮用真实 Tab 走查复核（这是关键方法差异）**：
  * `#task` dark：**24 个停留点，0 个自身无环，0 个"自身+向上 6 层都无环"**。
  * `#chat` dark：19 个停留点，3 个自身无环，但**向上 2 层**的 `.ant-select-sm.ant-select-borderless` 上 `box-shadow: rgb(245,180,87) 0px 0px 0px 2px` —— 环存在。
* **方法学更正（重要）**：我先前用 `element.focus()`（**程序化**聚焦）测过 `#task` 的 `.ant-select-outlined`，得到"输入框无环、祖先也是透明的 0 0 0 0px"，一度以为 #task 有真实缺陷。**程序化 `.focus()` 不触发 `:focus-visible`**，而 antd 的 outlined 环是 `:focus-visible` 驱动的 → **那是我测错了**。用真实 Tab 走查后，`#task` **0/24 失败**。
* **结论**：行 16 对 `#chat`/`#task` 的失败**都是判据只读"焦点元素自身"的产物**（审计自己的 note 也写着"元素自身"）。修法同 t10-F2：**沿焦点所有者向上找环**。
* **同时更正我在 t10 报告里的措辞**：t10 的 F2 说"环画在 `.ant-select` 祖先上"——这对 **borderless/sm** 变体（`#chat`）成立；我当时把结论推广到了 `#task` 的 **outlined** 变体，而那个变体的环在**更内层**（真实 Tab 走查显示 24/24 都有环，但载体不是"自身"）。**结论（判据该向上找）不变，证据要按变体分开说。**

### G7 · 行 21 / 行 22：命令面板 6 项 ARIA 缺口 + `Escape` 后焦点 = `BODY` — 归属 **t7（ui-shell）**

* 五页 × 两模式一致报 `6 缺口（ariaModal, ariaActivedescendant, ariaControls, listboxRole, optionRole, ariaSelected）`、`focusAfterEscape = BODY`；载体 `CommandPalette.tsx` / `App.tsx`。
* **对照**：`#sessions` 自己的浮层在 t11 已实测达标（焦点回到触发行 + `aria-modal`），所以缺的确实只是命令面板那一处。

### G8 · 行 27（首屏 JS 1396KB / gzip 436KB）— 归属 **P4**

单包问题（`index-Dj1H4QD7.js`），MASTER 附录 C 把行 25–28 列在 P4。

### G9 · 行 30（亮色 panel 的 ring 1.13:1 vs 基线 1.22:1）— **判据口径待裁决**，归属 t17 + captain

* 工具输出自带解释："§11 判据 = 与 `pageCanvas` 比；MASTER §12 的基线 1.22:1 是以 panel(白) 为底"，raw = `rgb(231,233,237) 0px 0px 0px 1px`。
* 即：**同一个 ring 值在两种底色下算出两个对比度**，而基线是在 panel 底上量的、判据却在 canvas 底上判。这又是"契约/判据口径"的一类。**本轮只登记，不做结论**（需要 captain 裁决"该行以哪个底为准"）。

---

## 6 未测项（不假装达标）

| 项 | 为什么没测 | 谁能测 |
|---|---|---|
| **D5 首次可读 ≤1,000ms** | 需要"内容可交互"时刻标记；审计的 `timing.load` 含 settle，不是该口径 | t8 |
| **I2 的"有 N≥2 条时 ≥3"** | 当前 `inboxCount = 0`（无待裁决权限），造不出来就不测 | 造一条待处理权限（t8） |
| **I11 的"有 N 条时"** | 同上；本轮的 83.46% 是**空态**数字 | t8 |
| **B7 / B11 / S12 / I6 / I7 / I13 的运行时** | 需要构造 `updated_at` 乱序、API 失败、裁决失败、字符串型 `raw_input` 等输入；本轮只做代码级核对 | t8（用 stub/mock） |
| **A12 的运行时** | 当前没有 `enabled=false` 的角色，`aria-disabled` 未渲染 | 造一条禁用角色（t8） |
| **`.nav-badge` 的对比度失败** | 审计明确警告：本轮未渲染（`inboxCount = 0`），"对白字 3.20:1 的对比度失败对本次审计不可见" | 造一条待处理权限（t8） |
| **行 16 / 行 30 的最终判定** | 判据待修/待裁决（G6 / G9） | t17 + captain |

---

## 7 复现命令（逐字）

```bash
# 0) 仪器版本（引用任何审计数字时必须同时给出）
cd panel && ls -la --time-style=+%m-%d_%H:%M:%S tools/design-audit.mjs     # 本轮 09-21_23:48:08

# 1) 主仪器（子集运行；t8 需用默认全量重跑）
cd panel && node tools/design-audit.mjs --json --routes=board,task,agents,stats,inbox \
  --task-id=01a0bc0c-2fcd-75c3-b531-d6cfc4f2e01d --out=<临时目录>

# 2) 硬编码检查（期望无输出）
cd panel && grep -nE '#[0-9a-fA-F]{3,8}' src/views/Board.tsx src/views/TaskDetail.tsx src/views/Agents.tsx

# 3) 跨文件一致性（期望只有一处 export function sourceHue）
cd panel && grep -rn 'sourceHue' src/

# 4) 两个轮询器的位置（G2）
cd panel && grep -rn 'pendingPermissions' src/

# 5) 辅助探针（本轮实际跑过的 6 个脚本，落在系统临时目录）
node <temp>/t12-probeA.mjs   # A5 逐个归属 / S3 Progress 载体
node <temp>/t12-probeB.mjs   # S8 读数≡表列求和 / S9 row-btn / S10 top_n / S4 可见性分解
node <temp>/t12-probeC.mjs   # I5 请求数 / A8 跨卡对齐 / A12
node <temp>/t12-probeD.mjs   # 真实 Tab 走查的焦点环（向上 6 层）/ I5 HTTP 方法
node <temp>/t12-probeE.mjs   # B5/B6/B8/B10 / I11 的 empty-mark 与 empty-state 几何
node <temp>/t12-px3.mjs      # 用 png.mjs 自己的 decodePng 复算 #inbox 逐色直方图
```

---

## 8 判定

**t6 交付（Board / TaskDetail / Agents·Stats·Inbox + i18n.tsx）＝ PASS。**

* 五份规格 §8 共 **55 行验收条目 → 52 行达标**；失败 3 行**全部不由 t6 的文件决定**：
  * `#task` 行 18（8 个 `<summary>` 的 16/21px 高度全部来自 `index.css:672/722/743`）→ t18
  * `#agents` A5 的 11 = t6 的 7（`.prompt-view > summary` 高度在 `index.css:1645`）+ 外壳的 4 → t18 / t7
  * `#inbox` I5（残留的外壳轮询）+ I11（规格补救物差 47.6 倍）→ t7 + captain 裁决
* **本轮最有价值的三条独立结论**：
  1. **S8 恒等实测成立**：读数 `22/21/1/$27.17` ≡ 表列求和 `22/21/1/27.17`（可机器校验的恒等式，四项逐个相等）；
  2. **B6 列计数恒等**：`[1,1,0,19]` 的 Σ = 21 = 卡片总数，且列头读数与列计数逐个相等；
  3. **行 16 用真实 Tab 走查复核后是判据问题**（`#task` 0/24 失败、`#chat` 的环在向上 2 层的焦点所有者上），并**更正了我自己**用程序化 `.focus()` 得出的错误中间结论。
* **我推翻 t6 一处措辞**（G4）：I11 的"不可能达成"不成立——画布在带内 ≠ 无解，把空态卡从 520 放宽到 1212 即可把带内降到约 67.6%。**算术认可、归因更正**。
* 层级中心、四态、六视口零溢出、共享原语复用（裸 hex = 0、内联 `font-size` = 0）**全部有锚点证据**；间距越界 `#board`/`#stats`/`#inbox` 各 **2 次**，是全站最干净的一档。

**给 captain 的处置建议**：G1/G5 的 7 个 → t18（与 `.tool-head min-height` 合并成一次共享层改动）；G2 → t7（徽标与列表共享一个轮询）；G3 → **裁决**（建议把 I5 目标改为 ≤7，并写清算式）；G4 → **裁决**（放宽 `.empty-state` `max-width` / 给 `#inbox` 全宽空态 / 列入行 1 例外，三选一）；G6 → t17（沿焦点所有者向上找环，与 t10-F2 合并）；G7 → t7；G8 → P4；G9 → **裁决**（行 30 以哪个底为准）。
