# 独立验证：面板 MCP 健康行第三态（`t128` / `ec24b61`）—— t136

> 单号 **t136**（verification，独立验证者 = `wiki`）· attempt 1 · **2026-10-04 00:3x–00:5x（+08:00）**
> inScope：**本报告**。`panel/src/**`、`crates/**`、`.github/**`、`scripts/**` **一行未改**（读到的都是读数或 finding）。
> 被测对象：`t128`（commit `ec24b61`，含 `panel/src/views/Agents.tsx` +38/−7、`panel/src/i18n/agents.ts` +9、`panel/e2e/agents-health-third-state.spec.ts` +176）。验证时 HEAD = `93d4491`。作者报告：`docs/design/reviews/gen4-panel-absorption-repair.md`（156 行，已读）。
> **我是自己驱动的**：自己的临时守护进程 + 自己的**计数代理** + 自己的 Playwright 探针 + 自己构建的**改前**面板；**没有**复述作者的注入读数。

## 0 结论

**通过（pass）**，三点独立成立：① 三态在**中英各一次**的真浏览器里都对，第三态**一个数字都没有**、且是**真按钮**；② **作者明确未测的那条补上了** —— 第三态的点击在**真实 HTTP 路径**上确实重取（`/api/v1/mcp` 命中就在点击后 **+52ms**，而轮询间隔是 **10000ms**），并且重取拿到的新数据**真的进了 DOM**（**+160ms** 后 `在线 · 7`）；③ **既有渲染未改写** —— 我把**改前字节**在隔离副本里重新构建、用第二个真守护进程渲染，`ok` 支与 `down` 支与当前构建**逐字相同**。
另交付：tooltip 边界（**视觉不可读 = 覆盖缺口**；**a11y 描述可读 ⇒ 有能红的断言配方**）、两个坐标的**漂移更正**、以及我本人重跑的两个面板门读数。

## 1 独立装置（我自己的，全部在 `%TEMP%`，收尾已删净见 §9）

| 装置 | 位置 / 身份 | 读数 |
| --- | --- | --- |
| 真守护进程（改后） | `D:\rust_cache\debug\ruagent.exe` **10/02 23:05:41，312.9 MB** · `--root %TEMP%\t136\root --addr 127.0.0.1:8897` · `RUAGENT_PANEL_DIST=<repo>\panel\dist` | `GET /api/v1/health` = `{"status":"ok",...}`；`GET /` 有 `id="root"` |
| **计数代理**（把「真实 HTTP」与「可数的重取」同时拿到） | `%TEMP%\t136\proxy.mjs` 监听 `8898` → 转发 `8897`；只改写 `GET /api/v1/mcp` 的载荷，并把**每一次** MCP 请求打上时间戳写日志 | 面板因此走**真网络**（不是浏览器内 `page.route` 拦截）⇒ 点击的请求**在线上可数** |
| **改前**面板（隔离副本） | `%TEMP%\t136\head-panel`（`robocopy` 复制 + `mklink /J node_modules`），两个源文件用 `git cat-file blob` 换成 `ec24b61^` 的字节 | 副本自证：`tools ?? 0` **2 处命中**、`toolsUnknown` **0 命中**；副本 build **exit 0**（`1853 modules transformed`，12.30s）；entry chunk `index-Dax5AB_6.js`（改后是另一个 chunk 名） |
| 第二个真守护进程（渲染改前构建） | `--root %TEMP%\t136\head-root --addr 127.0.0.1:8899` + 代理2 `8890→8899` | `/api/v1/health` = 200；`GET /` 有 `id="root"` |
| 浏览器 | `@playwright/test` 1.63.0 的 Chromium（仓库 `panel/node_modules`，用 junction 给小脚本解析） | 语言由 `localStorage["ruagent.lang"]` 控制（`i18n.tsx:125`）⇒ **中英各跑一遍** |

**C35（二进制年份）由我钉**：这份 **10/02 23:05:41** 的二进制**确实**在 `/api/v1/mcp` 里发出数字字段 —— 真实载荷 `{"servers":[{"name":"ruagent","command":"ruagent",...,"health":{"state":"ok","tools":18,"latency_ms":42,...}}]}` ⇒ 「不早于 `name` 字段变更」这一判断**在能测的地方成立**。但本单的**第三态本身**不靠二进制年份：它是**载荷**决定的（键缺失 / `null`），我下面用两条路径各测了一次（代理改写 = 真 HTTP；作者 spec 的 `page.route` = 浏览器内）。

## 2 三态 DOM 读数（我自己驱动，当前构建 `panel/dist`，经真守护进程）

| 注入形态 | 语言 | `tagName` | `class` | **逐字文本** | 数字 |
| --- | --- | --- | --- | --- | --- |
| `tools: 7` | zh | `SPAN` | `tag ok` | `在线 · 7` | 7 |
| `tools: 7` | en | `SPAN` | `tag ok` | `up · 7` | 7 |
| **`tools: null`** | zh | **`BUTTON`** | **`tag`** | `在线 · 未告知工具数 · 重试` | **none** |
| **`tools: null`** | en | **`BUTTON`** | **`tag`** | `up · tools not reported · retry` | **none** |
| **键被删（`absent`）** | zh | **`BUTTON`** | **`tag`** | `在线 · 未告知工具数 · 重试` | **none** |
| **键被删（`absent`）** | en | **`BUTTON`** | **`tag`** | `up · tools not reported · retry` | **none** |
| `state: "down"` | zh | `SPAN` | `tag warn` | `已下线` | none |
| `state: "down"` | en | `SPAN` | `tag warn` | `down` | none |

- **「没有数字」是机械判定的**：探针对文本做 `/\d/` 扫描并打印 `digits=none`（不是靠眼看）。
- **中性 `.tag` 而非 `.ok`/`.warn`**：与作者注释里写的形状一致（`warn` 是「已下线」，它本身是一个测量）。
- **真按钮**：`tagName=BUTTON`（`type="button"`，`Agents.tsx:370`），不是被样式伪装成可点的 `span`。
- **附带一条真实世界的读数**（不经我的改写，`passthrough`）：真守护进程自己的行（`ruagent`）渲染 **`在线 · 18` / `up · 18`**、`class="tag ok"` ⇒ `ok` 支在**真实数据**上也对。

## 3 作者明确未测的那条：**在真实守护进程下点第三态 = 真的重取**（本报告最有价值的读数）

命令形态：`mode=null` 起手 ⇒ 面板停在第三态 ⇒ **先把上游翻成 `tools:7`** ⇒ **只点一次**第三态按钮：

```
CLICK poll-baseline: 0 MCP request(s) in 6.0s (usePoll interval is 10000ms)
CLICK text-before="在线 · 未告知工具数 · 重试"
CLICK hits before=1 after=2 delta=1
CLICK first-hit-after-click at +52ms (poll is +10000ms)
CLICK upstream null->tools7: DOM followed=yes at +160ms text-after="在线 · 7" class-after="tag ok ant-tooltip-open"
CLICK clicks=1 (the row was never clicked twice in this reading)
```

**代理线上的原始日志**（每行 = 一次真实 `/api/v1/mcp` 请求；第二列是那一刻代理的输出模式）：

```
1791045825435 null        ← 挂载时的那次
1791045832238 tools7      ← 点击后的那次（间隔 6.8s < 10s 轮询 ⇒ 不是轮询）
```

**归因论证（为什么这是「点击」而不是「轮询」）**：
1. **静态**：第三态按钮的 `onClick={load}`（`Agents.tsx:370`），`load` 就是页面那个 loader（`Agents.tsx:79`），而轮询是 `usePoll(load, 10000)`（`:107`）；
2. **动态**：静默窗口 6.0s 内 **0** 次 MCP 请求（基线）⇒ 请求不是背景噪声；点击后 **+52ms** 出现一次；
3. **有后果**：上游在点击前一刻已经改成 `tools:7`，而 DOM 在 **+160ms** 变成 `在线 · 7`（`class="tag ok"`）⇒ 这次请求**把新数据带回了界面**，不是「点了但没重新拉」。
⇒ **作者的注入路径无法证明这一点**（它把重取也一起拦截了）；本读数在**真 HTTP** 上把它证明了。

## 4 既有渲染未改写（把 `down` 支与 `ok` 支与**改前构建**逐字对照）

**源码 diff 区域**（`ec24b61^ → HEAD`，且 `ec24b61..HEAD` 之后**没有任何提交**再碰这两个文件 ⇒ 比较对象就是 HEAD 的字节）：
- `panel/src/i18n/agents.ts`：只**新增** 5 行 zh + 3 行 en（`agents.mcp.toolsUnknown` 与 `.hint`），**没有一行改写既有 key**；
- `panel/src/views/Agents.tsx`：**唯一 hunk** 是 `@@ -349,13 +349,37 @@` —— 即 `ok` 支里的两处 `?? 0` 被去掉、并插入第三态分支。**`down` 支在 diff 里一行都没有**（`:383-388` 只是成了新 `if` 的 `else` 之后的上下文）。

**渲染字符串逐字对照**（两臂都是**独立构建 + 独立真守护进程 + 同一份探针**）：

| 状态 | 改前构建（`ec24b61^` 字节，副本） | 当前构建（`panel/dist`） | 判定 |
| --- | --- | --- | --- |
| `tools: 7` zh / en | `在线 · 7` / `up · 7`，`SPAN.tag.ok` | `在线 · 7` / `up · 7`，`SPAN.tag.ok` | **逐字相同** |
| `tools: null` zh / en | **`在线 · 0`** / **`up · 0`**，`SPAN.tag.ok` | `在线 · 未告知工具数 · 重试` / `up · tools not reported · retry`，`BUTTON.tag` | **改前是「自信的 0」⇒ 缺陷在本报告里被独立复现**；改后是第三态、无数字 |
| `state: down` zh / en | `已下线` / `down`，`SPAN.tag.warn` | 同左 | **逐字相同** |

⇒ **「既有渲染改写量 = 0」这条作者结论，在「我自己建的改前构建」上成立**（不是复述：我用的是**另一次 build + 另一个真守护进程 + 真 HTTP**，作者的负控用的是浏览器内注入）。

## 5 tooltip 边界：hint 文本**可读**（经 a11y 描述），但**视觉弹层在两种 harness 里都没打开**

- **视觉**：`hover()` 与 `focus()` 都没让 `.ant-tooltip-inner` 变成 visible（我这边 4.0s + 3.0s 两次等待，`shown=no`；作者也两次尝试未打开）⇒ **「tooltip 视觉渲染」= 覆盖缺口（coverage gap），不是缺陷** —— 它是 antd 的行为 + 测试环境，不是这一行代码错。
- **可读性（关键）**：第三态按钮上挂着 `aria-describedby="_r_10_"`，而 `#_r_10_` 里的文本**就是那句 hint**，逐字：
  - zh：`守护进程回了 health，但没有给出工具数——缺失不等于「没有工具」。点这里重新拉取。`
  - en：`The daemon answered health but did not report a tool count — missing is not the same as none. Click to re-read.`
- **⇒ 作者删掉的那条断言不必放弃**，可以做成**不需要 hover** 的能红断言（配方，未落地）：
  ```ts
  // on the third state (tools == null / absent):
  const id = await tag.getAttribute("aria-describedby");
  expect(id).toBeTruthy();
  expect(await page.locator("#" + id!).innerText()).toContain(
    lang === "zh" ? "缺失不等于" : "missing is not the same as none",
  );
  ```
  **能红证明（不改工具，只改输入）**：把 `agents.mcp.toolsUnknown.hint` 的文案改成任意别的句子（或把断言指向错误的键）⇒ 该断言红；`aria-describedby` 缺失时 `toBeTruthy()` 先红。**它的边界**：它证的是「读屏用户与 DOM 里能拿到那句话」，**不**证「鼠标悬停时看得见」——后者仍是 §5 第一条的缺口。

## 6 我本人重跑的面板门 + 两个坐标

| 门 | 命令 | 读数 |
| --- | --- | --- |
| 面板构建（含 i18n + `tsc -b` + e2e tsconfig + vite） | `npm --prefix panel run build` | **exit 0**，23.7s；`i18n dictionary integrity — panel/src/i18n.tsx` · `✓ 1853 modules transformed` · `✓ built in 5.31s` |
| i18n 字典门 | `node panel/e2e/i18n-check.mjs` | **exit 0**；末行 **`VERDICT: PASS`**（报告里同时列出动态族与 dead-key 候选，例如 `mcp.empty.hint` 在候选里 —— 那是既存清单，与本单无关） |

**两个「只登记未做」的坐标（C33：坐标会漂移，要回读）**：

| 作者报告里的坐标 | **HEAD 里该坐标现在是什么** | 该修点**现在**在哪 | 是否被 t128 改过 |
| --- | --- | --- | --- |
| `Agents.tsx:233` | `{(a.runtimes?.length ?? 0) > 1 ? (` | 同左（**＝报告的坐标仍然有效**） | **否**（最后被 `5134af2` 碰过；t128 的唯一 hunk 是 `@@ -349,13 +349,37 @@`） |
| `Agents.tsx:457` | `// ---------------------------------------------------------------------------`（一行注释分隔线） | **`:481`** `runtimes: editing.runtimes ?? [],`、**`:482`** `runtime: editing.runtime ?? editing.runtimes?.[0] ?? "",` | **否**（不在 t128 的 hunk 内） |

- 工作树对 `panel/src/views/Agents.tsx` **clean** ⇒ 我读到的就是 HEAD 的字节，「未被改动」因此可核。
- 顺带一条**不该算进这一族**的：`:850` `return JSON.stringify(raw)?.length ?? 0;` 在 `rawSize()` 里，函数契约就是「返回一个长度」，且 `:847` 已把 `raw == null` 单独返回 0 ⇒ **它不是「把未告知印成 0」**。⇒ 将来做同族扫描时别把它一并算作缺陷（否则会得出「还有 3 处」的结论）。

## 7 独立复跑作者的 spec（走仓库入口，对我自己的临时守护进程）

```
cd panel; E2E_BASE_URL=http://127.0.0.1:8897 npm run test:e2e -- agents-health-third-state.spec.ts
e2e target: http://127.0.0.1:8897 (writes armed via RUAGENT_E2E_ALLOW_WRITES=1)
Running 5 tests using 5 workers
DOM READING (2) tools=null -> "在线 · 未告知工具数 · 重试"
DOM READING (2b) tools key deleted -> "在线 · 未告知工具数 · 重试"
DOM READING (1) tools=7 -> "在线 · 7"
DOM READING (3) state=down -> "已下线"
DOM READING (2c) route hits before=1 after=2
5 passed (2.0s)
```
⇒ 作者自报的 `5 passed / exit=0` **复现**；且它的四条 DOM 读数与我在 §2/§3 用**另一条路径**（计数代理 + 真 HTTP）拿到的**逐字一致** ⇒ 两条独立路径互证。

## 8 不覆盖什么（第 19 条）

1. **视觉层**：tooltip 的**视觉**渲染未测（§5 第一条）；同样未测的是像素/对比度层面（那是 design-audit 的活）。
2. **第三态的「可点性」只在 Chromium 上测**：没跑真实鼠标设备/触摸。
3. **两个登记点本身的行为未测**：我只确认 `:233` 与 `:481/:482` **未被 t128 改动**，**没有**验证它们各自的缺陷（那需要它们自己的负控）。
4. **改前臂 = 我自己构建的 `ec24b61^` 字节**，不是作者那次负控的副本（他的副本 entry chunk 是 `index-CPFTiOJ6.js`）⇒ 两臂可比的是**渲染字符串**，不是「同一个制品」。
5. **我的改写路径与作者不同**：我用代理**合成** `/api/v1/mcp` 的载荷（传输是真的：面板→代理→真守护进程；被合成的只是 MCP 那一份 JSON）。作者的 `page.route` 在浏览器内拦截。**两条路径都不是「一个真的不报工具数的守护进程」** —— 这种守护进程按今天的 API 契约并不存在（真守护进程报 `tools: 18`），所以「键缺失 / `null`」只能由注入产生。
6. **CI/ubuntu**：全部读数来自本机 Windows + Chromium 1243；CI 上的字体/光栅器差异不影响这几条文本读数，但也没有在 CI 上跑过。
7. **`--check`/design-audit 与面板门的关系**：本单只重跑了 `npm run build` 与 `i18n-check.mjs`，没有跑 e2e 全量套件（只跑了被测 spec）。

## 9 纪律回执

- **写入集合 = 本报告**：`git status --porcelain -- panel crates .github scripts` 只有队友的在途改动 `M crates/mcp/src/lib.rs`（非我的）；`panel/src/**` 我只是读。
- **未碰 8787**（pid 14944）：全程在听，我一次请求都没发；我用的是**我自己的** 8897（改后）与 8899（改前）两个临时守护进程 + 8898/8890 两个代理，收尾全部停掉（`alive now: False` ×4），端口 8890/8897/8898/8899 全部释放。
- **e2e 走仓库入口**（`npm run test:e2e`，含 `E2E_BASE_URL` 命名目标）—— 没有裸 `npx playwright test`；我自己驱动的探针只读（不改面板、不改守护进程配置），且**关掉自己的浏览器**（`browser.close()`）。
- **一个真实危险被避开（记下来）**：我的临时树里有两个 **junction**（`%TEMP%\t136\node_modules` 与 `head-panel\node_modules` → 仓库 `panel/node_modules`）。**若直接 `Remove-Item -Recurse` 整个临时树，可能沿 junction 删进仓库**。我的收尾顺序是**先 `cmd rmdir` 两个 junction（只删链接）**，再删临时树；删后核对 **仓库 `panel/node_modules` 仍有 152 个条目、`@playwright/test` 仍在**。⇒ 以后任何「隔离副本 + junction」的做法都应照这个顺序收尾。
- 临时项清理：`%TEMP%\t136`（含探针/代理/两个守护进程 root/改前副本与 dist/日志/pid）已按具体路径删除；`*t136*` 残留 **0**；命令行匹配我临时路径的 `chrome.exe`/`node.exe` 孤儿 **0**。
