# t13 评审 — 外壳与次级视图（App / CommandPalette / Graph / Runtimes / Settings）

**评审对象**：t7（attempt 2）交付的 7 个 inScope 文件 + 它声明额外改动的 `panel/src/i18n.tsx`
**评审基线**：`docs/design/views/view-graph.md` §8 G1–G14、`view-runtimes.md` §8 R1–R11、`view-settings.md` §8 G1–G13、
`views/README.md` §2/§3/§5、`MASTER.md` §12 全表
**评审用构建**：`zfrUqmgI`（`panel/dist/assets/index-zfrUqmgI.js`，1439006 B；`meta.dist.buildId` 直读，**不以 mtime 为证**）
**评审人**：design-lead（本代设计负责人）· 只评审，未改任何 `panel/` 文件

## 0 结论摘要

| 面 | 规格条目 | 实现 | 未实现 / 缺陷 |
|---|---|---|---|
| **App / 外壳** | MASTER 行 35/36 + 行 17/19/24/28（shell 部分） | 228/72、992 断点、单 2s 轮询器 + `#inbox` 让渡、可见性感知、离线 banner、theme-color | **无** |
| **CommandPalette** | 行 21（6 属性）/ 行 22（焦点归还） | 6/6 属性齐、Escape 回到 `.kbd-hint` | 焦点未真正陷阱（行 21 不覆盖）；`agents/sessions` 失败静默 |
| **Graph** | G1–G14 | **11/14 实现**（G1/G2/G3/G5/G6/G7/G8/G9/G10/G11/G12/G13/G14 全部到位） | **G4 未实现**（`--graph-protocol` 仍是 `#ca9d33`）；**N+1：每次加载 55 个 `/graph/entity/<id>` 请求** |
| **Runtimes** | R1–R11 | **10/11 实现** | **R8 未达成**：tile 描边对底 **1.559:1**（暗）/ **1.372:1**（亮）≪ ≥3:1；且**品牌色根本没生效**（三个 tile 的 computed `color` 完全相同） |
| **Settings** | G1–G13 | **13/13 实现** | 仅一处注释与代码不一致（G13 的 toast） |

**机器判定（我自己跑的审计，非转述）**：`--routes=graph,runtimes,settings --modes=dark,light --overflow-viewports=390x844,768x900,1280x900,1440x900`
→ 6 captures，**34 pass / 1 fail / 3 not measured**；唯一 fail 是 **行 27（首屏 JS 1405KB / gzip 438KB，门槛 350/120）**——全站性、非 t7 引入。

**裁定：needs_revision** —— 见 §9 的 4 条 findings（F1 R8、F2 Graph N+1 在 t7 自己的文件里，必须修；F3/F4 转其他所有者）。

## 1 证据与方法（可复现）

```
# 审计（本轮，构建 zfrUqmgI）
cd panel && node tools/design-audit.mjs --json --routes=graph,runtimes,settings --modes=dark,light \
  --overflow-viewports=390x844,768x900,1280x900,1440x900 --out=<temp>/t13-audit
# → 6 captures in 31.4s · 34 pass / 1 fail / 3 not measured · failing rows: 27

# 探针（我写的，直连 8787 的当前构建）
node <temp>/t13-palette.mjs    # 命令面板 6 属性 + Escape 焦点 + 8 次 Tab 轨迹
node <temp>/t13-logo.mjs       # .runtime-logo 品牌色/描边对比度（canvas 合成，含 color(srgb …) 解析）+ theme-color
node <temp>/t13-final.mjs      # G9 检查器宽度预留 / Settings 卡宽 1920 / R10 边界
node <temp>/t13-graphreq.mjs   # #graph 的 /api 请求逐条计数（12s 窗口）
```

**口径说明（两条，避免误读）**：
1. `--overflow-viewports` 我**显式指定了四断点**（390/768/1280/1440），因为验收要求「四个断点是否不塌」——用默认 7 视口会掩盖这一点。
2. 审计的 `meta.dist.buildId` 是**唯一的构建标识**。本环境时钟发生过跳变，**mtime 不可作证据**：我另有一次 02:21 的运行读到的 buildId 是 `DKuo6EzU`（t7 之前的构建），那一轮的行 17/21/22/24 结论全部作废、不得引用。

## 2 App / 外壳

| 检查项 | 证据 | 结论 |
|---|---|---|
| 侧栏 228 / 72（行 36） | `App.tsx:213-214` `width={228} collapsedWidth={72}`；实测 228`1440 / 72`768 ×3 路由 | ✅ |
| 断点 ≤4 CSS + 1 契约值 992（行 35） | 审计行 35：`CSS [520, 768, 1024, 1240] ✓ · JS [992]`；`App.tsx:98/109` `matchMedia("(max-width: 992px)")` | ✅ |
| 无重复轮询（行 28） | `App.tsx:115-147`：`#inbox` 挂载时 shell **让渡**（`if (inboxRoute) return`），其余路由 1 个 2s 轮询器 + `document.hidden` 感知；`#inbox` 实测 6 次/11.5s（基线 14） | ✅ |
| 横向溢出 0（行 19） | 四断点 × 暗/亮 × 三路由，最大 0px | ✅ |
| 每路由 1 个 h1（行 24） | `graph/dark=1 "实体图谱" · runtimes/dark=1 "运行时" · settings/dark=1 "设置"`（含 sr-only） | ✅ |
| 无名称可交互 = 0（行 17） | `graph 0/21 · runtimes 0/34 · settings 0/26`；`App.tsx:258/268` 两个图标按钮带 `aria-label`，语言按钮（:273-275）有文本 `EN`/`中` + `title` | ✅ |
| theme-color（t7 的并入项） | `index.html:10-11` 两条 media-scoped meta `#08090b`/`#eaebee`；运行时改写首条：**暗跑首条 = `#08090b`、亮跑首条 = `#eaebee`**（我实测两个模式各一次） | ✅ |
| 失败态可见（行 20 同源） | `App.tsx:290-298` 离线 banner（`daemonUp === false`） | ✅ |

**层级中心**：外壳不持有层级中心（`.brand` / 导航项都在 20px 以下），三页的中心分别是 `.view-bar h2` 20px（settings）/ 仪表读数 32px（graph、runtimes）——与行 7 的逐页分类一致（审计行 7：`graph 32 · runtimes 32 · settings 20`）。

**四种状态**：外壳侧只有「在线 / 离线」两态（banner），这是外壳的正确粒度（空态/错误态属于视图）。✅

## 3 CommandPalette

| 检查项 | 证据 | 结论 |
|---|---|---|
| 行 21：6 个属性 | `role="dialog"` + `aria-modal="true"` + `aria-label`（`CommandPalette.tsx:261-263`）；input `role="combobox"` + `aria-expanded="true"` + `aria-controls="cmdk-list"` + `aria-activedescendant="cmdk-opt-0"`（:267-281）；`role="listbox"` + `id="cmdk-list"`（:284）；34 × `role="option"` + `aria-selected`（:300-302）。**实测 0 缺口**（审计行 21，6 captures 全 0） | ✅ |
| 行 22：焦点归还触发元素 | `CommandPalette.tsx:76-98`（关闭态挂 `focusin` 记触发者，且 `el.closest(".cmdk")` 排除自身）；**实测 Escape 后 `document.activeElement` = `BUTTON.kbd-hint`**（审计行 22 + 我的探针） | ✅ |
| C11：`.cmdk-item` 焦点环 `outline-offset: -2px` | `index.css` `.cmdk-item:focus-visible { outline-offset: -2px }` 在位 | ✅ |
| 四种状态 | 空态 `CommandPalette.tsx:285-286` `.cmdk-empty`；加载态用缓存即时渲染再后台刷新（:117-135）；**错误态缺失**（见 F4） | ⚠️ |
| 键盘 | `:229-243` ↑/↓/Enter/Escape；`:215-219` 选中项 `scrollIntoView({block:"nearest"})` | ✅ |

**跨文件一致性（本组特有风险）**：`sourceHue` **单一实现**——`Sessions.tsx:41-52` `export function sourceHue(s: string)`，
映射 `claude-code → var(--brand-claude)` / `opencode → var(--brand-opencode)` / `deepseek → var(--brand-deepseek)` / 其余 → `var(--ant-color-text-tertiary)`；
**Home 直接 import**（`Home.tsx:18`），**CommandPalette 也 import 同一份**（`CommandPalette.tsx:12` 取 `SOURCE_LABEL`，它只用文本、不用色相）。
→ **语义一致，无第二份实现** ✅（Chat 的 `wsColor`（`Chat.tsx:32`）是**工作区文件夹**色族，与 `sourceHue` 无关，不构成本项风险。）

## 4 Graph（G1–G14 逐条）

| ID | 目标 | 实测 / 证据 | 结论 |
|---|---|---|---|
| G1 | 亮色 `--graph-edge` ≥3.0（alpha .34→.50） | 审计行 31：`graph/light = 3.41:1 (α0.5) on #ffffff`；暗 `3.1:1 (α0.34)` | ✅ |
| G2 | 暗色 ≥3.0 保持 | 同上 `3.1:1` | ✅ |
| G3 | 9 分类色 ≥3.0 两模式 | 审计行 33：`graph/dark = 4.69:1（最低，9 色）· graph/light = 4.91:1` | ✅ |
| G4 | `--graph-protocol` 换非暖色相 | `index.css:98 --graph-protocol: #ca9d33` **未改**；当前数据有 **1 个 `protocol` 实体**（55 实体：product 16 / tool 22 / concept 8 / person 3 / project 3 / org 2 / protocol 1）⇒ 琥珀节点**实际可见** | ❌ **F3** |
| G5 | 信号色 ≤1.5% | 审计行 3：`graph/dark = 0.37%` | ✅ |
| G6 | ≥18px ≥3 / max ≥32 | 审计行 6 `3 / ≥3 · 标题 1`20px`；行 7 `32px` | ✅ |
| G7 | `.graph-detail-head h3` 字重 600 | `index.css:960-964` `font-size:16px; line-height:24px; font-weight:600`（不是 650） | ✅ |
| G8 | `.graph-detail` `clamp(320px,32vw,460px)` | `index.css:954` `flex: 0 0 clamp(320px, 32vw, 460px); max-width: 460px` | ✅ |
| G9 | 检查器宽度**恒预留** | `Graph.tsx:247-260`：未选中时渲染空 `.graph-detail`（`.zone-head` + 提示）；**实测 `.graph-detail` = 460px、`.graph-main` = 673px，选中前后完全一致** | ✅ |
| G10 | 截断在**画布上也可见** | `Graph.tsx:176-180`：`capped` 提示渲染在 `.graph-layout` **之上**（两种模式都看得到） | ✅ |
| G11 | reduce 时静态、不跑物理 | `Graph.tsx:430` 读 `matchMedia("(prefers-reduced-motion: reduce)")`；`:632-644` `kick()` 在 `s.reduce` 时只 `draw()` 不起循环 | ✅ |
| G12 | 画布 `role="img"` + 等价列表路径 | `Graph.tsx:828-829` `role="img"` + `aria-label={graph.canvasAlt(nodes, edges)}`；列表模式 `:232-244`（`.row-btn` 逐实体） | ✅ |
| G13 | 能量阈值停帧、交互重启 | `Graph.tsx:624-630` `tick()` 在 `step()` 返回不动且脉冲过期时 `s.running=false` 不再排帧；`kick()` 重启 | ✅ |
| G14 | `__graphDebug` 需 DEV 门控 | `Graph.tsx:786-789` `if (import.meta.env.DEV)` 才挂；`:802` 卸载 `delete` | ✅ |

**层级中心 / 四态 / 断点**：max = 32px 仪表档（行 7）✅；四态 loading(`Spinner`)/empty(`Empty`)/error(`ErrorState` + `canvasFailed` 降级列表)/normal 齐 ✅（另加搜索无命中的 `noHits` 态）；四断点横向溢出 0 ✅。
**共享原语**：`ReadoutStrip`/`Empty`/`ErrorState`/`Spinner`/`useToast` + `.graph-*`（视图私有，已登记 README §3.5.1）✅；内联 style 18 个 ≤50、内联 font-size 0（审计行 11/12）✅。

**❌ F2（本页最大的问题，见 §9）**：`Graph.tsx:65-66` `Promise.all(rendered.map(([e]) => api.graphEntity(e.id)))` —— **每个渲染实体一个请求**。实测 `#graph` 12s 内 **63 个 `/api` 请求 = 1×`/graph/entities?limit=500` + 7×`/permissions`（shell 2s 轮询）+ 55×`/graph/entity/<id>`**。

## 5 Runtimes（R1–R11 逐条）

| ID | 目标 | 实测 / 证据 | 结论 |
|---|---|---|---|
| R1 | ≥18px ≥3 / max ≥32 | 审计行 6 `4 / ≥3`；行 7 `32px` | ✅ |
| R2 | 单亮度带 ≤75% / 色度 1–6% / 信号 ≤1.5% | 审计行 1/2/3：`0.19% · 2.22% · 0.51%` | ✅ |
| R3 | 描边内容容器 ≤1 | 审计行 4：`0 / ≤1` | ✅ |
| R4 | 可见描边元素 ≤40 | 审计行 5：`7 / ≤40`（暗亮同值） | ✅ |
| R5 | <24px = 0；<32px ≤10 | 审计行 18：`内容区<32px 0/≤10 ✓ · 地板<24px 0 · 外壳闭集 ✓`（基线 16 → 0） | ✅ |
| R6 | 无名称可交互 = 0 | 审计行 17：`0/34`；`Runtimes.tsx:268-269` 同步按钮带 `aria-label` + `title` | ✅ |
| R7 | 内联 style ≤50、内联 font-size = 0 | 审计行 12：`21 / ≤50`；行 11：`0`。**顺带确认**：我在 t24 报的 `Runtimes.tsx:222` 内联 `marginTop: 14`（阶梯外）**已改为 16**（现 `:332`），行 37 = 0 | ✅ |
| R8 | `.runtime-logo` 描边对底 **≥3:1** | **描边合成后 (59,62,69) 对卡片 (27,30,36) = 1.559:1（暗）/ (219,220,222) 对白 = 1.372:1（亮）**；且三个 tile 的 computed `color` **完全相同**（暗 `rgb(173,178,187)` / 亮 `rgb(91,96,104)` = `--ant-color-text-secondary`）⇒ **品牌色一个都没生效** | ❌ **F1** |
| R9 | 删除前列出受影响角色 | `Runtimes.tsx:214-216` `usedBy` 由 `a.runtime === r.name \|\| a.runtimes?.includes(r.name)` 派生；`:308-318` Popconfirm 描述逐名列出 | ✅ |
| R10 | 与 `#agents` 无交集（Σ = `agents.length`） | 实测 `/api/v1/agents` = **10** 个；`#runtimes` 卡 **3** + `#agents` 卡 **7** = **10** ✔；无 disabled agent | ✅ |
| R11 | 探测失败可见（`role=alert` + 重试） | `Runtimes.tsx:44-55` `sync()` 返回 bool（不再吞异常）；`:156-159` `probe()` 落 `probeErr`；`:273-277` `<span className="tag err" role="alert">` | ✅ |

**层级中心 / 四态 / 断点 / 原语**：max 32px ✅；loading(`Spinner`)/empty(`Empty`)/error(`ErrorState` + 陈旧数据的 `common.stale` 分支 `:173-180`)/normal 齐 ✅；四断点溢出 0 ✅；`ReadoutStrip`（`ui.tsx`，带 `grid`）+ `Empty`/`ErrorState`/`Modal`/`useToast` ✅。

## 6 Settings（G1–G13 逐条）

| ID | 目标 | 实测 / 证据 | 结论 |
|---|---|---|---|
| G1 | ≥18px ≥3（§3 的 3 格读数 + h2 = 4） | 审计行 6：`settings/dark=4 / ≥1 §12.1 豁免 · 标题 1`20px`；`Settings.tsx:128-147` 三格 `.readout.s` | ✅ |
| G2 | max ≥20（无仪表盘页） | 审计行 7：`20px / ≥20px (无仪表盘)` | ✅ |
| G3 | 无名称可交互 = 0 | 审计行 17：`0/26`；两个 `Switch` 带 `aria-label`（`:161`/`:185`），`Select`/`Input`/`TextArea` 同样（`:196`/`:211`/`:228`）+ `.chat-field` 的 `<label>` 包裹 | ✅ |
| G4 | 无焦点环 Tab 停留点 = 0 | 审计行 16：`0/20 不合格`（暗亮同值；t27 之前 settings 是 2/2） | ✅ |
| G5 | 描边内容容器 ≤1 | 审计行 4：`0 / ≤1` | ✅ |
| G6 | 可见描边元素 ≤40 | 审计行 5：`6 / ≤40` | ✅ |
| G7 | 单亮度带/色度/信号 | 审计行 1/2/3：`0.02% · 2.82% · 0.78%` | ✅ |
| G8 | 内联 style ≤50、内联 font-size 0 | 审计行 12：`27 / ≤50`；行 11：`0` | ✅ |
| G9 | 卡宽 760 固定 | `index.css:1703 .distill-settings { max-width: 760px }`；**实测 1440 → 760、1920 → 760×726（不变宽）** | ✅ |
| G10 | 副题改为「蒸馏策略」 | `i18n.tsx:25 "settings.subtitle": "蒸馏策略"`（en: `"distillation policy"`）；`Settings.tsx:20` 引用该键 | ✅ |
| G11 | `graph` 三态可见 | `Settings.tsx:90-95` `null → distill.graphFollow`；`:174-189` `null` 时显示 `.tag`「跟随默认」、非 null 时给一键回退的 link | ✅ |
| G12 | `builtin_prompt` 可见 | `Settings.tsx:238-241` `details.prompt-view > summary + pre`；`index.css:1724 .prompt-view > pre` | ✅ |
| G13 | 常驻「已保存 hh:mm:ss」+ 失败保留表单 | `:111` `setSavedAt(...)` + `:244-248` 常驻行；`:113-118` 失败只 `setSaveErr`（**不动表单值**）+ `:257` `ErrorState` | ✅（注释见 F5） |

**层级中心 / 四态 / 断点 / 原语**：中心 = `.view-bar h2` 20px，次级读数 18px 在中心之下 ✅；四态 loading(`Spinner` `:73`)/empty(`Select notFoundContent` `:205`)/error(`ErrorState` + 重试 `:60-71`)/normal ✅；`.row.wrap` 的两个 `.chat-field[flex:1 1 220px]` 按规格换行，四断点溢出 0 ✅；原语 `.readout-strip`/`.readout.s`/`.zone-head`/`.zone-title`/`.chat-field`/`.tag`/`.prompt-view` + `Spinner`/`ErrorState`/`useToast` ✅。

## 7 契约缺口（**不是实现缺陷**，需另立条目）

1. **行 21 的属性集不覆盖「焦点是否被限制在模态内」**。`CommandPalette` 声明 `aria-modal="true"`，但**没有焦点陷阱**：实测连续 8 次 Tab 都停在 `.cmdk-item`（34 项，暂时走不出去），但**第 35 次之后会落到背景的外壳按钮上**。`primitives.md:551` 写着「`Modal` … 唯一浮层模态（**焦点陷阱由 antd 提供**）」——这条假设**对这份手写 `.cmdk` 不成立**。行 21 判 0 缺口是**判据的边界**，不是实现的功劳。
2. **行 28 的判定范围与它的意图不匹配**。行的标题是「单路由 ≤7」，意图是「无重复轮询」；工具 `scope: route` ⇒ 一旦用 `--api-window=11500` 跑全量，**每条路由都会按「11.5s 内全部请求数」判**。此时 `#graph` 的首屏 N+1（55 个请求）与 `#inbox` 的轮询语义混在一个数里。**这是 t8 的雷**：见 F2。
3. **行 27（首屏 JS 1405KB / gzip 438KB）是全站唯一 fail**，非 t7 引入；登记在案，不重复派工。

## 8 未测量项（诚实登记）

| 项 | 原因 |
|---|---|
| 行 20（API 失败时的 `role=alert` + 重试） | 需要 CDP 请求屏蔽与失败态注入，工具未实现（工具自己标 `not_measured`）。**代码层面**三页都按规格写了（`Runtimes.tsx:118-133`、`Settings.tsx:59-74`、`Graph.tsx:184-196`），但我**没有**在失败注入下验证过 |
| 行 25 | 行的 scope 是 `#task`，与本组无关 |
| 行 28（11.5s 窗口） | 我本轮未跑长窗口（2.5s 窗口不判定）。`#inbox` 的 6 次是 t7 的数字，我未复测 |
| 焦点陷阱的第 35 次 Tab | 我只走到 8 次；「Tab 最终会离开」是从 DOM 结构推断（`.cmdk` 之后没有可聚焦元素、背景未 inert），未实测到出界那一刻 |

## 9 发现清单

### F1（high，t7 文件 + 共享层）— R8 未达成，且品牌色从未生效
- **现象**：三个 `.runtime-logo`（`brand-claude` / `brand-deepseek` / `brand-opencode`）的 computed `color` **完全相同**（暗 `rgb(173,178,187)` = `--ant-color-text-secondary`；亮 `rgb(91,96,104)`）；描边合成后对卡片 = **1.559:1（暗）/ 1.372:1（亮）**，规格要求 **≥3:1**。
- **根因（两处，都要修）**：
  1. **CSS 顺序**：`index.css:1848-1850` 的 `.brand-claude/.brand-deepseek/.brand-opencode { color: var(--brand-*) }` 是**单类选择器**，而 `.runtime-logo { color: var(--ant-color-text-secondary) }` 在 `:1854-1858`、**同优先级但更靠后** ⇒ 后者胜出，品牌色被覆盖。
  2. **视图侧 `mono`**：`Runtimes.tsx:225` `<BrandMark … mono />` —— `mono` 让 SVG 不带品牌类、`fill: currentColor` 直接取中性色。这行的注释（`:220-221`「a tint of its brand color」）与实测不符。
- **算式（为什么 22% 达不到 3:1，换色也救不了）**：卡片底 `#1b1e24`（L=0.012880）。要 ≥3:1 需合成后 L ≥ 3×0.06288 − 0.05 = **0.13864**。
  以 claude `#d97757` 为例按 alpha 混合：a=0.22 → L≈0.0375（**1.39:1**）、a=0.5 → L≈0.0951（**2.31:1**）、a=0.75 → L≈0.1748（**3.58:1**）。
  ⇒ **22% 的描边在任何品牌色下都到不了 3:1；要达标需 ≈75% 混合（或改实色描边）**。
- **requiredFix**：① 视图侧去掉 `mono`（或只让「无品牌」的 mock/未知走中性）；② 共享层把 `.brand-*` 规则移到 `.runtime-logo` 之后（或写成 `.runtime-logo.brand-*` 提高优先级），并把描边混合提到 ≥75% 以满足 3:1；③ **若团队判定该 tile 是纯装饰**（图标本身 6.547:1 / 5.498:1 已达标），则**必须改 `view-runtimes.md` R8 的目标**——**不许**保留「≥3:1」而让实现停在 1.56:1。

### F2（high，t7 文件）— `#graph` 首屏 N+1：55 个请求
- **现象**：12s 窗口实测 **63 个 `/api` 请求** = 1 × `/graph/entities?limit=500` + 7 × `/permissions` + **55 × `/graph/entity/<id>`**（每个渲染实体一个）。工具在 2.5s 窗口记到 `graph/dark = 59`（同组 runtimes 6 / settings 4）。
- **位置**：`Graph.tsx:65-66` `Promise.all(rendered.map(([e]) => api.graphEntity(e.id).catch(…)))`；`MAX_NODES = 150` ⇒ 最坏 150 个请求。
- **为什么不能只怪视图**：daemon **没有**批量 facts 端点（`crates/daemon/src/api.rs:49-55` 只有 `/entities`、`/entity/{id}`、`/entity/{id}/facts`、`/neighbors`）⇒ 边表只能靠逐个实体取 facts。**这是视图 + API 形状的共同问题**。
- **风险（t8 必须知道）**：行 28 的 `scope = route`。一旦 `t8` 用 `--api-window=11500` 跑全量，`#graph` 会按「11.5s 内全部请求数」判 ⇒ **必然 FAIL**（55 + 6 ≈ 61 ≫ 7），而这与「无重复轮询」无关。**建议**：t8 落槌前先把行 28 的判定范围写成「只判有轮询器的路由」或「排除首屏载入突发」，否则验收会被一个与被度量对象无关的常数打红。
- **requiredFix**：①（根因）daemon 增批量 `GET /api/v1/graph/facts?limit=…`（或让 `/entities` 直接带 facts），视图一次取回；②（缓解，视图可自做）只对**实际会画边的实体**取 facts，或按 `MAX_NODES` 之下再设一个「首屏取 facts 上限」（例如 20），其余按需在选中时取；③ 无论选哪条，**把请求数写进 `view-graph.md` 的验收行**（例如「`#graph` 首屏 `/api` 请求数 ≤ 8」），否则这条永远不会被冻结的判据看见。

### F3（medium，共享层 / 契约）— G4 未实现
- **现象**：`index.css:98 --graph-protocol: #ca9d33` **一字未改**（亮色块亦无覆盖）。`view-graph.md:61` 的「禁止」与 `:136` 的 G4 都要求换非暖色相（示例 `#8fa3e8`/紫蓝族）。
- **不是纸面问题**：当前数据 **55 个实体里有 1 个 `protocol`** ⇒ 琥珀节点**在今天的 `#graph` 上真实渲染**，与「运行中」的信号色同色系。
- **requiredFix**：改 `--graph-protocol`（暗/亮各一值）到非暖色相并复测行 33（9 分类色 ≥3:1）；若决定不换，则改 `view-graph.md` G4 与 MASTER §3.1 W6 的措辞（**不许**两边都留着）。

### F4（low，契约 + 视图）— 命令面板的焦点未陷阱 / 错误态缺失
- `aria-modal="true"` 但无焦点陷阱（见 §7.1）。**requiredFix**：加一个最小陷阱（Tab/Shift+Tab 在 `.cmdk` 内循环）或对背景加 `inert`；若认为不必，则**在行 21 的判据里写明「不检焦点限制」**，别让 `aria-modal` 变成一句空承诺。
- `CommandPalette.tsx:127/134` `catch(() => {})` 吞掉 `agents()`/`sessions()` 的失败 ⇒ 面板静默退化成只有导航项。**requiredFix**：失败时在列表顶部给一行「角色/会话加载失败」+ 重试（与行 20 同源），或至少在 `note` 里说明。

### F5（low，t7 文件）— 注释与代码不一致（G13）
- `Settings.tsx:110` 注释写「the confirmation is a line that stays, **not a 2s toast**」，但 `:112` 仍然 `toast("ok", …)`。行为上不违规（常驻行在位、表单值保留），但**注释在说假话**。**requiredFix**：要么删掉 `:112` 的 toast，要么改注释。

## 10 我确认「已修好」的既有缺陷（回归证据）

| 之前的问题 | 现在 | 证据 |
|---|---|---|
| `#settings` 无名称控件 4 个 | 0 | 审计行 17 `settings 0/26`；`aria-label` 五处 |
| `#settings` 2 个 `.ant-select-input` 无焦点环 | 0/20 | 审计行 16 |
| `#settings` 错误态渲染成永久 Spinner | 已分离 | `Settings.tsx:59-74`；`catch(e) => setErr(e)` |
| `#settings` 副题与内容不符 | 已改 | `i18n.tsx:25` |
| `#settings` `graph` 三态不可见 | 已可见 | `Settings.tsx:90-95/174-189` |
| `builtin_prompt` 完全不可见 | 只读展开 | `Settings.tsx:238-241` |
| `#runtimes` 读取失败渲染成空态 | 已分离 | `Runtimes.tsx:87-96/118-133` |
| `#runtimes` 探测失败静默 | `role=alert` | `Runtimes.tsx:273-277` |
| `#runtimes` 内联 `marginTop: 14`（行 37 越界，我在 t24 报的） | 已改 16 | `Runtimes.tsx:332`；审计行 37 `0 / 18 raw` |
| `#graph` 检查器选中即重排 | 恒预留 | 实测 460/673 前后一致 |
| `#graph` 调试钩子无门控 | DEV 门控 | `Graph.tsx:786-789` |
| 行 28 双轮询器（14 次） | 让渡后 6 次 | `App.tsx:115-147` + t7 实测 |
| `#settings` `.ant-switch` 21px | 48×24（t28/t30 已落地） | 审计行 18 `settings 内容区<32px 2`（两个 switch 的 24 高，过地板） |

## 11 附：本轮我自己复核过的「不是问题」项

1. **`.graph-detail-head h3` 的字重是 600**，不是基线里的 650（G7 ✅）——我读了 `index.css:960-964` 而不是凭观感。
2. **`#graph` 的 55 个实体请求是「载入突发」不是「轮询」**：12s 窗口内 `/graph/entity/*` 每个路径**只出现 1 次**，没有重复轮询。F2 是 N+1，不是轮询风暴——这两件事的修法不同，不能混为一谈。
3. **`#runtimes` 的 R10 边界是干净的**：3 + 7 = 10 = `agents.length`，且当前没有 disabled agent（若将来有 disabled runtime，`runtimes` 会把它藏起来 ⇒ Σ 会小于 `agents.length`，届时需要把「Σ = agents.length」改成「两页交集为空」）。
4. **审计工具已实现 §12.9 的分区判定**（行 18 输出「外壳闭集 ✓ · 内容区<32px N/≤10」）——我在 t38 报的 ⑧ 号工具偏差**已闭环**，此处更正。
