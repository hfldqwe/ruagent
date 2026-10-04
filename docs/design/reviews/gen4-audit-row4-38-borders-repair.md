# 审计行 4+38（`#home` 描边内容容器 = 3）：**未修复** —— 定位完成 + 交接包（t156, attempt 1）

**状态：failed（交接，不是判断为不可修）** · **零源码改动**（`panel/src/views/Home.tsx` 一字未改）⇒ 树没有被留在半成品。停手原因是**本会话预算耗尽**：剩余上下文不足以完成「改 → 面板构建 → 仪器两臂（2 路由 × 2 档）→ 报告」这一串有读数的动作；**在没有读数的情况下改源码会制造一个未验证的绿**，比干净交接更坏。

## 1 三处已**定位**（用仪器自己的证据链，不是目测/猜类名）

仪器行 38 的失败原文已给出 DOM 路径（这支路径就是对象集的**实例**）：

```
home/dark=3 (div.steps:nth-of-type(3) > div.ant-card.ant-card-bordered:nth-of-type(1)
             > div.ant-card-body > div.step-num)
```

⇒ 命中对象集的三处是 **`.steps` 里的三个 antd `Card`**（`div.ant-card.ant-card-bordered`）：

| # | 选择器路径（逐层） | tag | 为何命中对象集 |
| --- | --- | --- | --- |
| 1 | `div.steps:nth-of-type(3) > div.ant-card.ant-card-bordered:nth-of-type(1)` | `div` | antd `Card` 的 `ant-card-bordered` 给**四边** 1px 实线（≥2 边 ✓）、边框色不透明（alpha>0.04 ✓）、class 不匹配豁免表 ✓、可见 ✓ |
| 2 | `… .steps > div.ant-card.ant-card-bordered:nth-of-type(2)` | `div` | 同上 |
| 3 | `… .steps > div.ant-card.ant-card-bordered:nth-of-type(3)` | `div` | 同上 |

**源码坐标（今天字节，逐行回读）**：`panel/src/views/Home.tsx` `:445 <div className="steps">`，其下**三个** `Card`：`:446`、`:454`、`:462`（各自含 `<div className="step-num">1|2|3</div>`，`:447/:455/:463`）。样式侧：`panel/src/index.css:1221 .steps{…}`、`:808/:818 .step-num{…}`（`.step-num` 只是内容，**不计入**：它是 `div` 但只有单边/无边框；仪器命中的是它的**祖先 Card**）。**alpha 读数未取到**（需要仪器 `--json` 的逐元素读数；见 §3）。**注意**：`index.css` **不在本单 inScope** ⇒ 修法应当只用 `Home.tsx` 侧的手段（见 §2）。

## 2 修法配方（写给下一位；都在 inScope 内）

1. **把三个 step `Card` 变成无边框**：antd v5 用 `variant="borderless"`（旧写法 `bordered={false}`，本仓 antd 版本决定用哪个 —— 构建会立刻报错告诉我）。这样它们的 `border-width` 全 0 ⇒ 不再命中对象集，而**内容一字不减**（判据要的是「≤1 且那个是控件单元」，不是「把卡片删掉」）。
2. **行 4 的附带条款**：`.inbox-card` 必须 `borderWidth === 0px`。**本单范围内未在 `Home.tsx` 里找到 `.inbox-card`**（`Select-String -Path panel/src/views/Home.tsx -Pattern 'inbox-card'` = **0 命中**）；样式定义在 `panel/src/index.css:1197`（`.inbox-card { box-shadow: inset 3px 0 0 var(--signal), … }` ⇒ 脊线本来就是用 **inset shadow** 画的，**若它同时是 `ant-card-bordered` 则仍会 1px 描边**）。⇒ **下一位必须先定位 `.inbox-card` 的宿主视图**；若它不在 `Home.tsx`（可能在 `Memory.tsx`/`Settings.tsx`，两者**本单 outOfScope**）⇒ **报 captain**，不要越界改。
3. **判据/仪器零改动**：行 4/38 的 `criterion` 与 `limits` 逐字不动；`panel/tools/design-audit.mjs` 不在 inScope，**本单也没有改它**。

## 3 仪器怎么跑（下一步的读数配方）

- 入口：`panel/tools/design-audit.mjs`，**base URL 取自 `E2E_BASE_URL`**（`design-audit.mjs:401 baseUrl: process.env.E2E_BASE_URL ?? "http://127.0.0.1:8787"`）⇒ **必须显式给**，否则会打到活守护进程（`8787` 今天由**端口归属**决定，按 C32 读 `Get-NetTCPConnection` + health，**不要引用任何被记住的 pid**）。
- 需要一个**被服务的 panel**：用**自己的临时端口 + 临时 root + `RUAGENT_PANEL_DIST`**（本单纪律②）。`--json` 可拿逐元素读数（含 **alpha/两侧边**，本单 §1 缺的那一项）。
- 收尾：关掉自己开的浏览器；按**自己的临时路径**找孤儿（**不要按进程名/端口批量杀**）；删掉自己的临时目录（具体路径，不用通配）。

## 4 未覆盖 / 未测（第 19 条）

- **未取到**：修前臂（HEAD）与修后臂的**判定原文**——两臂都没跑 ⇒ **没有 `home/dark=3 / ≤1` 的复现读数**（本单引用的是**审计工件里的原话**，不是我自己的运行）；也**没有**「另选 2 条路由（如 `sessions,graph`）两臂失败行集合逐行相同」的对照。
- **未做**：`npm --prefix panel run build`（无改动可门）；三条能红/绿读数；`.inbox-card` 除 `borderWidth===0` 之外的两处未测（它的宿主视图、它当前是否已无边框）。
- **不覆盖**：其余路由的行 4/38；真实屏幕阅读器；**视觉意图是否被改变**（只能证明计数 ≤1，**不能证明「好看」**）；其它档位（本单只关 dark/light）。
- **明确不改**：`panel/src/index.css`（不在 inScope）——若最终修法必须改 CSS，请 captain 扩面。

## 5 零改动自证

`panel/src/views/Home.tsx`、`panel/src/i18n/**`、`panel/e2e/**` 本 attempt **均未改动**；唯一写入是本文件。活守护进程**未启停、未写库**。
