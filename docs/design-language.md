# Panel design language — "Bench"

The control desk for a resident agent. Dark-first, written down so the next
change extends it instead of drifting from it.

**Status: 2026-09-23 · build `C048RA32`** — round-2 acceptance over 13 routes × dark/light:
**38 PASS / 0 FAIL / 1 not_measured**. Everything below has a machine-judgeable form in
`docs/design/MASTER.md` §12 (39 rows); this file is the human summary. Companions:

| 文件 | 是什么 |
|---|---|
| `docs/design/MASTER.md` | §12 = 39 行可机器判定的阈值表（唯一权威判据） |
| `docs/design/primitives.md` | 共享层契约（token / 原语 / 键盘 / 对比度下限） |
| `docs/design/views/view-*.md` | 13 份逐视图规格（含每页验收数字） |
| **`docs/design/before-after.md`** | **改前 / 改后 / 阈值 三列 × 39 行 + 截图文件名** |
| `docs/screenshots/audit-run/round2-verification.md` | round-2 证据（F1–F5/N1、行 1/18/27/28/39、抖动） |
| `docs/screenshots/audit-run/report.md` + `metrics.json` | 审计工具自产的完整表与原始数字 |

## ⚠️ 工具正在演进：请按「buildId + 工具 mtime + 窗口」读结论

收口期间审计工具被改了两次，**同一份源码出现了三种结论**。三次我都实跑过，绑定如下（**不要只抄一个数**）：

| 运行 | build | 工具 mtime / sha256 | 窗口 | 结论 | FAIL 行 |
|---|---|---|---|---|---|
| A（round 2 定稿验收） | `C048RA32` | `00:33:38` / `4b11176a…` | 11.5s | **38 PASS / 0 FAIL / 1 not_measured** | —（行 20 未覆盖） |
| B | `DKadudCh` | `02:18:02` / `a6281fca…` | 11.5s | **38 PASS / 1 FAIL / 0 not_measured** | **行 20**（13×2「重试可点 ✗ · 恢复 ✗」） |
| C（最新，02:37） | `DKadudCh` | `02:26:57` / `fdb93db1…` | 默认 2.5s | **37 PASS / 1 FAIL / 1 not_measured** | **行 13**（`knowledge/dark = 1`） |

- **行 20 在 B 里 FAIL、在 C 里 PASS**（同一份源码）⇒ 差别在工具的**注入/点击实现**，不是产品改动。
- **行 13 在 A/B 里是 0、在 C 里是 1**（`#knowledge` 暗色 1 处 WCAG 文本对比度失败）⇒ 同样是工具版本差异，
  **那 1 个节点尚未定位**。
- **行 28 需要 11.5s 窗口**：C 用默认 2.5s 窗口 ⇒ `not_measured`；A/B 用 11.5s ⇒ PASS（13/13，含 `#task 7≤7` 压线）。
- 三次的 **锚点未命中都是 0 行**（Warnings 只有 `.nav-badge 本轮未渲染` 一条）。
- ⇒ **给下一个人**：引用任何数字前先跑一次 `node tools/design-audit.mjs --check --api-window=11500`，
  并把「buildId + 工具 mtime + sha256 + 窗口」四件套写在结论旁边。

## The four decisions

1. **Material, not borders.** Four surfaces per mode — shell < canvas < panel <
   raised — with steps wide enough to read on a large dark field: canvas → panel
   is **2.47x** in relative luminance (row 29, floor 2.4x). A panel carries an inset
   top highlight in dark and a hairline ring in light; only overlays (menu, modal,
   palette) cast a drop shadow. Before this pass the panel sat ~3% above the canvas
   and ~500 elements carried a `1px rgba(255,255,255,.05)` border no one could see
   (`#task` alone had **1679** visible stroked elements, row 5). **Bordered boxes are
   now 0 on 12 routes; `#chat`'s `.composer` is the one sanctioned control-unit
   exception** (rows 4 / 38 — it is a control, not a container).

2. **One signal: amber `#f0a93b`.** It means "now" — running, streaming, waiting on
   you, and the primary action. It is the *only* accent, and it is one token: the old
   `warn` / `primary` pair was two colours for one meaning. Measured: **0 duplicate
   groups across the 5-token check set**, `--status-warn` ↔ `--signal` Δhue **12.8°**
   dark / **9.5°** light (row 32); signal pixels **0.13–1.08%** per route (row 3,
   budget 5%, internal target 1.5%). The brand mark stays neutral so the logo can
   never be mistaken for a live state.

3. **Readouts are big.** Hierarchy comes from a scale that jumps, not from weight.
   Every view opens with an instrument strip — 2–4 numbers at 32/40 IBM Plex Mono on
   one plate, divided by rules rather than boxed into identical cards. A gauge wears
   the signal only when its number means "live" or "waiting on you". Before: 95% of
   text was ≤14px and each page had exactly **one** element ≥18px (row 6: now 1–23,
   every page at or above its floor; row 7: max type 20px or 32px, never below).

4. **Rules divide, panels bound, zones group.** Three container roles replace the
   box-everything reflex: `.zone` (a rule plus a label, no box), `.panel` (the
   default bounded surface), overlays (antd's own).

## Type

IBM Plex Sans Variable for UI, IBM Plex Mono for real data — ids, hashes, counts,
timestamps, code. **Mono is never used for a label**; it is used *large*, for
readouts. CJK falls through PingFang SC → Microsoft YaHei.

```
readout     32/40 mono   display 24/30   title 20/26   section 15/22
body        14/21        label   13/18   micro 11/16 (data only)
```

Radius ladder: 4 chips · 6 controls · 10 panels · 14 overlays.
Text ladder violations: **0** (row 8), weight violations **0** (row 9), radius
violations **0** (row 10), inline `font-size` **0** (row 11).

## How it is measured (re-run, do not eyeball)

```bash
cd panel
npm run build                     # 记 buildId；staleSrc 必须为 false，否则数字无效
node tools/design-audit.mjs --self-test                    # 203/203
node tools/design-audit.mjs --check                        # §12 判定表（有 FAIL 则 exit 1）
node tools/design-audit.mjs --json --api-window=11500       # 全量 26 captures ≈ 6.5 min
npx playwright test                                        # 41 项：39 passed / 2 skipped
```

审计产物落在 `docs/screenshots/audit-run/`：`metrics.json`（原始数字）、`report.md`（全部表）、
`<route>_<mode>.png`（13 × 2 张截图）。**每条结论都要绑定 buildId + 工具 mtime + 源码指纹**
（跨轮漂移真实存在：同一份源码的 longtask 与窗口内请求数会有 ±1 级抖动）。

## 快照（改前 → 改后 → 阈值）

完整 39 行见 **`docs/design/before-after.md`**；这里是决策相关的 10 行：

| 指标 | 改前 | 改后（`C048RA32`） | 阈值 |
|---|---|---|---|
| 可见描边元素（最多的一页） | 1679（`#task`） | 35 | ≤40/路由 |
| 严格口径描边容器 | 12/13 路由 = 0，`#chat` = 1 | 同（`#chat` 的 `.composer` 是唯一例外） | ≤1 且必须是控件单元 |
| ≥18px 文本节点 / 页 | 1 | 1–23（每页 ≥ 其门槛） | 含读数 ≥3；无读数 ≥1 |
| 材质台阶 canvas→panel | ~1.03x（"panel 比 canvas 高 3%"） | **2.47x** | ≥2.4x |
| 焦点环对比度（最差） | 手写亮 1.69 / antd 暗 1.73 | 暗 7.92 / 亮 5.74 | ≥3:1 |
| 无环 Tab 停留点 | `#chat` 5/21、`#settings` 2/20 | **0/20（13 路由 × 2 模式）** | 0 |
| `#task` DOM 节点 | 153,074 | **487** | ≤3000 |
| `#task` 最长阻塞 | 2999ms | **106ms**（全站 0 次 >200ms） | ≤200ms |
| 首屏 JS | 1406KB / gzip 436KB（单包） | **应用层首屏 94KB / gzip 31KB**（首屏静态 1114KB，地板 1020KB） | ≤150KB / ≤48KB + 2 结构断言 |
| 单路由 `/api` 请求 | `#graph` 56（55× `/graph/entity/<id>`） | `#graph` **R=1, K=1 ⇒ 1 ≤ 4** | `R ≤ 2K+2`（行 39） |

## 护栏：零余量（动这里会翻 FAIL）

给下一个人：**这几处没有余量，新增取数/控件前先读 `MASTER` §12.10.1 / §12.10.3 / §12.9。**

| 行 | 路由 | 当前值 | 上限 | 什么会翻 FAIL |
|---|---|---|---|---|
| **28** | **`#task`** | 逐路由最大组 **7**（`/api/v1/tasks/:id`，每任务 1 次） | **7** | 任何一次**额外取数**（多一条轮询腿 / 多一次重取） |
| **39** | **`#task`** | **R = 10–11**（K = 4–5；跨轮数据依赖：`/runs/:id/events` 是否在首屏出现） | **2K+2 = 10–12** | **再多一类端点**即 FAIL（第二次实测 R 恰好 = 上限） |
| 1 | `#chat` | 设计面 **24.22%**（含画布读数 **73.01%**） | 75% | 现行口径安全；但「含画布」读法距上限只有 **1.99pp** |
| 18 | `#task` | 内容区 **8** | 10 | 新增 3 个小控件（余量 2） |
| 27 | 全站 | 应用层首屏 **94KB** | 150KB | 入口增长 >60%（余量 37%） |
| X4 | `#runtimes` | `@520` 的 A/B **不同**（3 项条：grid 2 轨 vs flex 320 上限） | — | `@media (max-width:520px)` **包含 520 本身** ⇒「>520 无规则」严格成立于 **≥521** |

## 已知开放项（不掩饰）

- **行 20 与行 13：两个「随工具版本翻转」的行**（详见上面的三次运行表）：
  - **行 20**：t59 把失败态注入落地后，工具 `02:18:02` 报 **FAIL**（13 路由 × 2 模式「失败态可见 ✓ ·
    **重试可点 ✗ · 恢复 ✗**」；注入覆盖 home 6/6 · task 3/4 · agents 3/3 · chat 1/3 · runtimes 1/4 · 其余 1/1，
    外壳仍在 ✓、未渲染成空态 ✓）；工具 `02:26:57` 同一源码报 **PASS**。**结论未定，需要一次以最新工具为准的复测**
    —— 修复者与引用者都要先跑再下判断。
  - **行 13**：工具 `02:26:57` 在 `#knowledge` **暗色**报 **1 处 WCAG 文本对比度失败**（`knowledge/dark = 1`），
    而工具 `02:18:02` 与 round-2（`00:33:38`）都报 0。**那 1 个节点尚未定位**（需看最新报告的 `report.md` 明细）。
  - 对比参考：命令面板 `.cmdk` 的同类重试**实测可点且能恢复**（49×32，真实点击后 alert 消失、列表回到 34 项），
    所以行 20 若确为缺陷，问题在**各视图错误态的接线**，不是注入手法。
- **F-7**：`#knowledge` 自动聚焦的 `input.ant-input` 同时戴**信号环**（合格）与 **antd primary 派生的
  `border-color: rgb(207,147,53)` + `box-shadow: rgba(254,165,21,.26) 0 0 0 2px`**；§3.2 白名单 W1–W6
  未列此位置。行 16 的 Tab 走查看不到它（第一个 Tab 会离开 autoFocus 元素）。
- **图谱画布的选中/悬停环**用 `pal.ring = --ant-color-primary`（暗 = `#f0a93b`，即信号色）；
  30 点网格点击实测 **2 次命中并出现琥珀像素 190 / 124 px** ⇒ 可达。**t58 正在裁决**
  （补白名单还是改色）。
- **亮色 claude 品牌标记**对**自身 10% 底色**的对比度 **2.81:1**（对卡片 3.12:1 ✔）——
  F1 的原文判据（对卡片 ≥3:1）PASS；**t58 正在裁决**是否按下调底色透明度处理。
- **`.nav-badge` 盲区**：仅在 `inboxCount > 0` 时渲染，本轮 13 路由实测 0 个 ⇒ 它「白字 3.20:1」的
  对比度失败对审计不可见（复测前先造一条待处理权限）。
- **`#chat` / `#settings` 仍以 20px 封顶**：两页都不要仪表盘行，只靠页标题撑层级（这是 row 7 的
  合法形态，不是缺陷）。
- **视觉判断的边界**：本环境的图像桥不可用 ⇒ **没有人「看过」这些像素，只测量过**。
  截图在 `docs/screenshots/audit-run/`（26 张，13 路由 × 暗/亮）。

## 变更纪律（三条）

1. **共享层只有 `index.css` / `theme.tsx` / `ui.tsx` 是热文件**，视图不得自己写跨页 CSS；
   新的小号控件高度必须落在**系统标准 32px**（`controlHeightSM` 一处收口）。
2. **口径写进判据旁边**：本项目反复出现「同一指标两种口径」（行 1 画布、行 18 外壳常数、行 27 首屏 vs 全量、
   行 28 逐端点 vs 总数、断点「处」vs「个」）。任何数字旁边必须能读到它属于哪个口径。
3. **判定用产物，不用源码意图**：所有阈值都从 `dist` + 运行时计算样式读，`staleSrc` 为 true 时
   本轮数字作废（先 `npm run build` 再跑）。
