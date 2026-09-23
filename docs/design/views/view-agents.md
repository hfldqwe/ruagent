# agents — `#agents`

> **路由**：`#agents`
> **视图**：`panel/src/views/Agents.tsx` → `Agents()`（同文件还导出 `Stats` 与 `Inbox`，各属自己的路由）；内部 `AgentEditor` 弹窗
> **消费的 MASTER 行**：§12 行 **6、7、18**（本页特有）+ README §2 全局
> **消费的变更编号**：`C2 C5` · `T2 T3 T5 T6` · `S1` · `R1`（`.agent-avatar` 圆角 8→6/10）`A1 A2 A3 A4` · `Y1 Y3` · `Z1 Z2`
> **冻结选择器**：`.view-bar h2`、`.agent-card`、`.ant-empty`（`registry.spec.ts` 走 `#agents`）

## 1 职责与首要任务

**职责**：管理**角色**（role：带 prompt 或运行时引用的卡）与查看 **MCP 注册表**。运行时本身的增删在 `#runtimes`。
**首要任务**：**看清每个角色的运行表现（跑了几次、成功率、花了多少钱），并改它的提示词**。

据此决定：
- 卡片必须**等高**且统计块**底对齐**（`.agent-card { height:100% }` + `.agent-stats { margin-top:auto; border-top }`）——一排 3 张卡的横线必须在同一高度，否则描述长短会让统计数字跳动。**这条"跨卡对齐"的规则是本页最重要的版式约束**。
- 统计是**每张卡 4 格**（运行 / 成功率 / 成本 / 最近一次），来自 `api.stats()`；没有运行记录时**必须显示 `—`**，不能显示 0（"没跑过" ≠ "跑了 0 次"）。
- 提示词**折叠**（`details.prompt-view`，标题写「提示词 · N 字符」）：它是角色卡里最长的字段，展开会把卡片拉伸 300px。
- MCP 注册表放在同页下方（`h3.sec`）：它是角色的**依赖**，用户在改角色时需要看到"这个角色能用哪些 MCP"。
- 两处 5s 轮询（`agents` + `mcp`）共用一个 `load()`，规格要求**合并为一次**（MASTER 行 28 的同源要求）。

## 2 版式骨架

```
1440：侧栏 228 │ content 1212（可用 1149）
┌ .view-bar ─────────────────────────────────────────────────────────────┐
│ h2 智能体  .muted 副题  .grow  [+ 新建角色](primary)                     │
├ .agent-grid  repeat(auto-fill, minmax(min(310px,100%),1fr)) gap 14 ────┤
│ .agent-card[.disabled] ×3（实测 374×~250）│ ×3 │ ×1                     │
│ ┌ .ant-card-body（flex column, height 100%）─────────────────────────┐ │
│ │ .row ───────────────────────────────────────────────────────────── │ │
│ │  [.agent-avatar 36×36 r8 mono 12/16 w600 大写前 2 字]  <name>  .grow │ │
│ │                                          .tag <runtime>  .tag<models>│ │
│ │                                          .tag.ok 启用 | .tag 已禁用  │ │
│ │ p.muted「<description>」                                           │ │
│ │ details.prompt-view  ▸ 提示词 · 417 字符                            │ │
│ │ .tag.mono <model>                                                  │ │
│ │ ─────────────────────────────────────────  ← .agent-stats 的上线    │ │
│ │ .agent-stats  .stat-cell ×4                                        │ │
│ │   [.stat-num 18/24 mono]  运行    成功率    成本    最近一次         │ │
│ │   （无数据 → .stat-num 显示 —  + .muted「没有运行」）                │ │
│ │ .row.end  [编辑] [删除(popconfirm)]                                 │ │
│ └────────────────────────────────────────────────────────────────────┘ │
├ h3.sec「MCP 注册表」────────────────────────────────────────────────────┤
│ .row ×N  .doc-icon │ <name>  .tag.ok健康|.tag.warn down  .tag<inject_for>│
│          .grow  .muted.mono.truncated  <url ?? command>                  │
│ 空 → p.muted.pad（mcp.overlay 提示）                                     │
└──────────────────────────────────────────────────────────────────────────┘
390：.agent-grid 单列 294（min(310px,100%) 生效）；pageH 2405
768：2 列 324；pageH 1502        1280：3 列 323；pageH 1232
```

**实测**：`.agent-grid` 1149×822 / `grid[374,374,374]`（@1440）；996×864 / `[323,323,323]`（@1280）；662×1141 / `[324,324]`（@768）；294×1972 / `[294]`（@390）。
`.agent-avatar` 36×36 r8 → **r6**（primitives §9.2）；`.runtime-logo`（在 runtimes）38×38 r10。

## 3 层级

**本页最大元素 = `.stat-num`（18/24 mono w500）**，实测 `maxFs = 20`（`.view-bar h2` 20 是最大）、`≥18px` 节点 **23** 个（h2 + 22 个 `.stat-num`）。

这是全站 ≥18px 节点最多的页（23），机制正确：**每张卡的 4 个统计数字才是用户要看的东西**，它们升到 18/24 mono，而角色名留在 14/21。
次级：`.view-bar h2` 20/26（页面标题，唯一比 `.stat-num` 大的元素，符合"页面标题 > 内容读数"的次序）。
第三级：`.tag`（11px）、`.muted`（**12/17 → 12/16**，primitives §3.1：L1 没有 17px 行高档）。

**禁止**：`.stat-num` 超过 18/24（它是卡片内读数，不是页面读数——页面读数才配 32）；给 `.agent-avatar` 上色（源码注释已写明"琥珀是信号，头像不是 live 状态"，**这条注释是 P2 的范例，必须在改动中保留**）。

## 4 内容模型

| 调用 | 返回类型（`api.ts`） | 用到字段 | 展示位 |
|---|---|---|---|
| `api.agents()` | `AgentInfo[]` | `id, name, harness, description, model, enabled, models?, runtime?, runtimes?, prompt?, options?, kind?, command?` | 卡片；**本页只显示 `isRoleAgent(a) === true` 的**（`kind === "role"` 或形状回退：有 `prompt`/`runtime`/`runtimes`） |
| `api.stats()`（5s 轮询） | `AgentStats[]` | `agent, runs, completed, failed, total_cost_usd, last_run_at` | `.agent-stats` 4 格：`runs` / `completed/(completed+failed)` / `fmtUsd(total_cost_usd)` / `RelTime(last_run_at)` |
| `api.mcp()`（5s 轮询） | `McpRegistry` | `servers[{name, command, url, inject_for, health{state, tools, latency_ms, error, checked_at}}]`、`profiles[{name, servers[]}]` | MCP 注册表行 |
| `api.createAgent({name, prompt, description?, model?, runtimes?, runtime?, options?})` | `AgentInfo` | — | 新建角色 |
| `api.updateAgent(name, body)` | — | `body` 可含 `prompt/description/model/runtimes/options` | 编辑 |
| `api.deleteAgent(name)` | — | — | 删除（`Popconfirm` ✅ 现状已有） |

**字段 → 展示位**

| 展示位 | 字段 | 规则 |
|---|---|---|
| `.agent-avatar` | `name.slice(0,2).toUpperCase()` | 派生，非数据 |
| 角色名 | `name`（`id` 同为 `name`） | — |
| `.tag`① | `runtime` | 单运行时；`runtimes` 数组则取第一个 + "`+N`" |
| `.tag`② | `options?.mode` / `options?.effort` | 有则显示两个 .tag（权限模式 / 推理力度） |
| `.tag.ok` / `.tag` | `enabled` | `true` → 「启用」(`.tag.ok`)；`false` → 「已禁用」(`.tag` + 整卡 `.disabled` = `opacity .5`) |
| `.muted` 描述 | `description` | — |
| `details.prompt-view` 标题 | `prompt.length` | 「提示词 · N 字符」；正文 `<pre>` |
| `.tag.mono` | `model` | `null` 不渲染 |
| MCP 行状态 | `health.state` | `"ok"` → `.tag.ok` + `tools` 数；`"down"` → `.tag.warn`（**不得用 `.tag.err`**：down 不是错误态，是状态）；`null` → 无 tag |
| MCP inject_for | `inject_for` | 数组 join(", ") |

**分组**：角色卡网格（无分组）+ MCP 注册表（单段）。`profiles` **当前未渲染** → 规格要求作为注册表的第二段显示（`profiles[].servers` 用 `.tag` 列出），否则该字段无接口。
**密度上限**：

| 项 | 实测 | 上限 | 手段 |
|---|---|---|---|
| 角色卡 | 7 | **≤60** | 超出后每卡折叠描述，或按 harness 分组 |
| 卡片高 | ~250px | **≤320px** | `description` 2 行截断；`prompt` 恒折叠 |
| `.stat-cell` | 28（7×4，实测 22 因为部分卡显示 `—`） | 4/卡 | 固定 |
| MCP 行 | 0（本机无注册） | ≤50 | 超出滚动 |
| `.stat-num`（≥18px 节点） | **23** | ≤40（行 6 的上界） | 60 张卡 × 4 = 240 会破上界 → **卡片数 >10 时 `.stat-num` 降到 15/22**（section 档） |
| 页高 | 1193px | ≤3000px | — |
| 节点数 | 331 | ≤1500 | — |

## 5 四种状态

| 态 | 契约 | 现状 |
|---|---|---|
| **loading** | `agents === null` → `Spinner` | ✅ |
| **empty** | ①无角色 → `Empty` + 「还没有角色」+ 「+ 新建角色」②无 MCP → `p.muted.pad`（`mcp.overlay`）③`stats` 里没有该角色 → 4 格显示 `—` + 「没有运行」（`agents.noRuns`） | ①③✅ ②✅ |
| **error** | ①`api.stats()` 与 `api.mcp()` 的 `catch(() => {})` **静默吞掉** → 卡片显示全部 `—`，与"真的没跑过"无法区分；②`deleteAgent` 失败只有 toast。规格要求：`role=alert` + 重试，且**统计缺失与统计为 0 必须可区分**（`—` vs `0`） | **违反** |
| **密集** | 见 §4 上限。额外：①5s 轮询后**卡片顺序稳定**（按 `name` 排序；现状按 API 返回顺序）②`.agent-grid` 必须保留 `min(310px,100%)`（294px 视口不溢出） | 部分达标 |

## 6 响应式行为

| 视口 | 侧栏 | content | 列数 | 卡宽 | pageH |
|---|---|---|---|---|---|
| **390** | 72 | 318（294） | **1** | 294 | 2405 |
| **768** | 72 | 696（662） | **2** | 324 | 1502 |
| **1280** | 228 | 1052（996） | **3** | 323 | 1232 |
| **1440** | 228 | 1212（1149） | **3** | 374 | 1193 |

横向溢出：**0**（四视口实测全 0）。列式固定为 `repeat(auto-fill, minmax(min(310px,100%),1fr))` —— **不得改成裸 `310px` 下限**（294px 视口会溢出；`index.css` 注释已记录这次事故）。

## 7 复用的共享原语与类名

`.view-bar` `.agent-grid` `.agent-card` `.disabled` `.agent-avatar` `.agent-stats` `.stat-cell` `.stat-num` `.row` `.row.end` `.tag` `.tag.ok` `.tag.warn` `.tag.mono` `.muted` `.mono` `.truncated` `.grow` `.pad` `.card`（antd `Card` 经 `.ant-card`）`.prompt-view` `.doc-icon` `.empty-state`
浮层：`.modal`（via `Modal`）+ `.field`
组件：`Spinner` `Empty` `Modal` `RelTime` `fmtUsd` `useToast` `Input` `Select` `Button` `Popconfirm` `Icon`
**本页请求的 zone 化（Z1）**：`h3.sec`「MCP 注册表」→ `.zone-head` + `.zone-title` + `.zone-note`（服务器计数）。
新增类请求：`.sr-only`（隐藏 `h1`「智能体」）、`.micro`（**11/15**，与 `.muted` 叠用）、`.error-state` + `.spinner-block`（经 `ui.tsx` 的三态契约）。**本页不新增数据文本类** —— primitives §7 R3 已拒绝该请求（它与 `.mono` 逐字相同），数据文本用既有 `.mono`。

## 8 本页特有的验收数字

| ID | 指标 | 基线（实测） | 目标 | 判定（MASTER 行） |
|---|---|---|---|---|
| A1 | ≥18px 文本节点 / 最大字号 | **23 / 20px** | 3 ≤ x ≤ **40** / ≥20px | 行 6、7（**注意上界**：60 张卡会破 40） |
| A2 | 单亮度带 / 色度 / 信号占比 | **31.0% / 3.14% / 0.66%** | ≤75% / 1.0–6.0% / ≤1.5% | 行 1、2、3 |
| A3 | 描边内容容器 | **0** | ≤1 且必须是控件单元 | 行 4 |
| A4 | 可见描边元素数 | **32** | ≤40（贴线，不得再增） | 行 5 |
| A5 | <32px 命中目标 | **25**（`.row.end` 的编辑/删除按钮） | <24px = 0；<32px ≤10 | **行 18**（超标 15） |
| A6 | 无名称可交互元素 | **0** | **0** | 行 17 ✅ |
| A7 | `.agent-avatar` 圆角 | **8px（阶梯外）** | **6px**（primitives §9.2 定死：8 不加入阶梯，相邻档差会掉到 2px） | **行 10**（R1） |
| A8 | 跨卡横线对齐 | `.agent-stats` 的 `border-top` 在各卡同高 | **保持**（`height:100%` + `margin-top:auto`） | 本页特有（版式硬约束） |
| A9 | 统计缺失 vs 0 | 无运行 → `—`（现状 ✅）；但 API 失败也是 `—` | `—` = 无数据；`0` = 有数据为零；失败 → `role=alert` | 行 20 同源 |
| A10 | 轮询合并 | `agents` + `mcp` 两次 5s 轮询 | **1 个**可见性感知轮询（`document.hidden` 暂停） | **行 28** 同源 |
| A11 | 内联 style 节点数 | **31** | ≤50；内联 `font-size` = 0 | 行 11、12 |
| A12 | `disabled` 卡的可达性 | `opacity .5` + `.tag`「已禁用」，**无重新启用入口** | 补 `aria-disabled` + 卡片上的「启用」动作 | 行 17 同源 + 审计 §八 |
