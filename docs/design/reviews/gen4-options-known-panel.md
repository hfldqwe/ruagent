# C4 后半：面板把 `known === false` 当「未知/重试」（t185）

**任务**：`t185`（kind = repair，接 `t182` 的 **C4 后半**）· **日期**：2026-10-04
**写入（inScope）**：`panel/src/views/Runtimes.tsx` + 本文件。**其余一律未碰**（`panel/tools/**`、`panel/e2e/**`、`panel/src/index.css`、`crates/**`、`.github/**`、`scripts/**`）。
**最终字节（修订身份，C66）**：`panel/src/views/Runtimes.tsx` sha256 `9D40A5638BCF6E21161B88F021CAA378DBC7979BB9A64FAA76945FCDF47EC42F`，git blob `759cdf9d72ba6812b5f2198a1443d5198984d1ad`。

**一句话**：`t182` 让后端会说「**我不知道**」（`known:false`）；本单让面板**不再把「不知道」渲染成「0 models」**，而是走它**已有的第三态**，并且**逐字保留 `t84` 的判据与文案**（只加强）。

---

## 1. 先量：后端信号真的存在（我自己的临时守护进程 · 临时 root · 临时端口）

**环境**：`%TEMP%\t185-bin\ruagent.exe`（我自己 `cargo rustc -p ruagent --bin ruagent -- -o <私有路径>` 编出来的，**因为共享 target 里的 `ruagent.exe` 被活守护进程占着图像**：`cargo build` 报 `failed to remove file ...\ruagent.exe`，即 AGENTS.md 里那条 `os error 5`），root = `%TEMP%\t185-root`，addr = **`127.0.0.1:8799`**。**`8787` 全程只读探测**（收尾时 `Get-NetTCPConnection -LocalPort 8787` ⇒ 监听中 pid **14944**，`GET /api/v1/stats` ⇒ **HTTP 200**）：**没有启停它、没有写它的库、没有用 `GET /api/v1/recall`**。

**读数 ②：第一手报告过（mock 运行时，命令 = 仓库里的 `ruagent-mock-agent.exe --behavior echo`）**
```
GET /api/v1/agents/mock/options   ⇒   HTTP 200
{"agent":"mock","options":[{"id":"model",...2 choices...},{"id":"mode",...},
 {"id":"reasoning_effort",...}],"cached":true,"known":true,"updated_at":1791116417796}
```
（日志同时给出 `INFO ruagent_daemon::chat: option catalog refreshed runtime=mock options=3`。）
⇒ **`known: true`**，目录有 3 个选项、`cached: true`（服务的是持久化目录）。

**读数 ①：从未被告知（`known: false`）** —— 这一格**没有现成运行时可用**：mock **无条件**在 `session/new` 里 `advertise()`（`crates/mock-agent/src/lib.rs`），所以它永远不会「不报告」。⇒ 我在**自己的临时目录**写了一个**最小的静默 ACP agent**（`silent-acp.mjs`，line-delimited JSON-RPC：`initialize` 回显协议版本、`session/new` 只回 `sessionId`、**不带 `configOptions`**，之后保持安静），把它挂成一个运行时：
```
GET /api/v1/agents/silent/options ⇒ HTTP 200   （耗时 20s = MODELS_WAIT 的上界到点）
{"agent":"silent","options":[],"cached":false,"known":false,"updated_at":1791116469666}
```
日志：`INFO ruagent_daemon::chat: option catalog refreshed runtime=silent options=0`。
**再一次**请求同一条路由，仍然是 `"cached":false,"known":false`（再等 20 s）⇒ **「不知道」没有被持久化**（否则第二次会命中缓存）—— 这是 `t192` 那条策略在 HTTP 层的可见读数。

⇒ **两种情形在同一台守护进程上可分**：`known:false`（`cached:false`、目录空、**不权威**）vs `known:true`（`cached:true`、目录就是事实）。

## 2. 面板改动（`panel/src/views/Runtimes.tsx`）

**新增两个纯函数 + 一个判别联合**（都 `export`，因为这是**可被检查驱动的缝**）：
```ts
export type ModelChipState =
  | { kind: "failed" } | { kind: "unknown" } | { kind: "notProbed" } | { kind: "count"; n: number };
export function modelChipState(failed, notKnown, count): ModelChipState   // failed > notKnown > count
export function modelsReadout(counts, probeFailed, notKnown, label): number | string
```
判别联合**故意**这么写：**数字只能靠把 `kind` 收窄到 `"count"` 才能拿到** ⇒ 任何未来的编辑都**没法把未知格式化成数字**。

**hook（`useModelCounts`）**：新增 `notKnown` 这一路读数（与 `t84` 的 `probeFailed` **分开存**，这样 `t84` 的判断与文案原样保留）：
- 挂载探针里 `known === false` ⇒ 记 `notKnown[name] = true`，**并且删除该运行时的计数**（不留陈旧的数字可被求和）；
- `known === true` ⇒ 照旧记计数、清 `notKnown`（与 `t103` 「标志不许闩死」同形）；
- **`sync`（手动同步）**：`known === false` ⇒ 记 `notKnown`、删计数、**返回 `false`**（R11：同步没拿到答案就是一次可见的失败，不能当成功走计数分支）；
- 读取 `known` 处用了**一处窄转换** `(o as { known?: boolean }).known` 并加注释：**`panel/src/api.ts` 的 `AgentOptions` 还没声明 `known`，而 `api.ts` 不在本单 inScope** ⇒ 没有借机去改共享接口（**建议另立单给 api.ts 的所有者加 `known?: boolean`**）。

**两处渲染**：
- **计数器 chip**：改走 `modelChipState(...)` ⇒ `failed` 仍是**那个可点的 `probeFailed` 按钮**（`t84`/`t103 F2b` 的形状与文案**逐字未动**）；`unknown` 落在**第三个分支**（`runtimes.notProbed` 的文案，旁边就是同步按钮可重试）；**只有 `kind === "count"` 才打印 `runtimes.models` 的数字**。
- **读数条（`ReadoutStrip` 的 `models` 格）**：改走 `modelsReadout(...)`。**这是「0 models」真正被打印的地方**（`:215-221` 的旧表达式把 `known:false` 的 0 加进总和；全未知时没有 `probeFailed` ⇒ 打印一个**纯 `0`**）。

**`t84` 为什么没有被削弱（等价性论证）**：
| 情形 | 旧表达式 | 新表达式 | 关系 |
| --- | --- | --- | --- |
| 全部成功 | 求和（过滤掉 0 项） | 求和（同一批） | **逐字等价** |
| 全失败 | `probeFailed` 文案 | 无应答 ⇒ 有失败 ⇒ `probeFailed` 文案 | **等价**（文案与判据都是原来的） |
| 失败 + 有人应答 | 只加应答者 | 只加应答者 | **等价** |
| 全未知（新） | **打印 `0`** | `notProbed` 第三态 | **本单修的就是这一格** |
| 未知 + 有人应答（新） | 求和（未知当 0 加进去） | 只加应答者 | 数字相同，但**未知不再被当成读数** |
`t84` 自己那条判断（`Object.keys(probeFailed).some(...)` ⇒ `t("runtimes.probeFailed")`）与它的注释**原样保留**，新逻辑是**加在它旁边**的一条并列情形。

## 3. 能红的负控（**红→绿**）

**先说清楚限制（C63）**：`panel/` **没有单元测试运行器** —— `panel/package.json` 只有 `dev` / `i18n:check` / `build` / `check` / `test:e2e`（Playwright），`panel/src` 下**零个 `*.test.ts(x)`**；而 **`panel/e2e/**` 不在本单 inScope**，仓库纪律又禁止裸 `npx playwright test`。⇒ **DOM 级断言在本单是「未测」**，不是「通过」。

**我实际做的负控（能红，且驱动的是真品而不是副本）**：把**真实的** `Runtimes.tsx` 用 vite 自带的 esbuild 打成 ESM（`--loader:.css=empty`，产物放在**我自己的临时路径**），然后由 `%TEMP%` 下的脚本 import 它导出的两个函数：
- **绿**：`modelsReadout({silent:0}, {}, {silent:true}, L) === L.notProbed`（未知⇒第三态，**没有数字**）；`modelChipState(false, true, 0).kind === "unknown"`。
- **红（注入条件）**：同一个真函数喂**忽略 `known` 的输入**（`{}`，即本单之前面板所看到的世界）⇒ **返回恰好 `0`** ⇒ 断言「不得是 0」若用旧输入**必然红** ⇒ 这条控制**有判别力**，不是恒真。
- **不削弱 t84**：全失败 ⇒ `PROBE-FAILED` 文案；失败+应答 ⇒ 只加应答者（3，不是 3+0）；失败压过未知 ⇒ 仍报失败。
**读数**：`node control.mjs` ⇒ **10/10 ok**，末行 `CONTROL GREEN: the unknown path cannot print a number, and the ignored-known path prints 0`，**exit=0**。

## 4. 门禁（退出码先于任何管道）

| 门禁 | 读数 |
| --- | --- |
| `npm --prefix panel run build`（含 `i18n-check` → `tsc -b` → e2e tsconfig → vite） | **exit=0**（`✓ built in 4.69s`），跑在最终字节 sha256 `9D40A563…` 上 |
| `scripts/cargo-team.ps1 test -p ruagent-daemon` | **exit=0** —— lib **159 passed** · capabilities 11 · capability_defaults 5 · event_compat 2 · graph_ingest 16 · injection_e2e **8 ignored** · knowledge_api 16 · recall_evidence_announced 3 · smoke 0 · version_points 5 ⇒ **217 passed / 0 failed / 8 ignored**（`crates/**` 本单**零改动** ⇒ 这是与 `t182` 同一批 crate 字节的复跑） |

## 5. 未覆盖 / 边界

- **触发条件仍未证实**（`t180`/`t182` 都承认，我照抄）：**「某个运行时会话首答为空、稍后真有选项」需要真实 ACP 运行时**才能观测；本单修的是「**后端现在会说不知道 + 面板据此不再把不知道渲染成没有**」这条**在字节上无条件存在过**的路径。我的读数 ① 用的是**我自己写的静默 stub**（一个真实存在但**不是真实 harness** 的 ACP 对端）⇒ 它证明的是**协议上「什么都没说」会被如实报成 `known:false`**，**不是**某个真实运行时会出现这种情况。
- **其余面板视图（先量后说）**：`api.agentOptions` 的调用方是 **4 个** —— `Runtimes.tsx`（本单）、**`Agents.tsx:512`、`Chat.tsx:562/572`、以及没人提过的 `TaskDetail.tsx:493`**。我读了它们的用法：`Agents.tsx:630-634` 是 `modelChoices.length ? 选择项 : fallbackModels`、`Chat.tsx:188` 是 `if (choices.length > 0)` ⇒ **都是「有没有」的存在性判断，不打印数字** ⇒ **它们没有 `t84` 那一类「0 models」缺陷**；但**同样的混淆在那里以更弱的形式存在**（`known:false` 与「真的没有」都掉进「自由文本/回退列表」）⇒ **我点名它们、本单不改**（不在 inScope；且见 §6 的承诺），建议另立一条面板单。
- **i18n 文件不在 inScope** ⇒ 我**没能**为「未知」加一个专门的文案键，第三态复用了既有的 `runtimes.notProbed`（「未探测」）。**这是本单的一个已知不足**：语义上「问过但没被告知」与「还没问」用同一句话，比 `t84` 那条「探针失败」要弱。建议另立单加 `runtimes.unknown`（`panel/src/i18n/**` 归别人）。
- `api.ts` 未改（`known` 靠一处带注释的窄转换读取）⇒ 建议另立单把 `known?: boolean` 加进 `AgentOptions`。
- **`acp/src/run.rs:265`** 的同一折叠写法：**只登记**（本单与 `t182` 都不在它的 inScope）。
- **`O2`**：只登记。
- **独立验证**：本单作者同时是后端那单（`t182`）的作者 ⇒ **C4 的两半需要一个独立验证者**（建议验证者至少复核：① 两条后端读数**自己重跑一遍**；② §2 的等价性表；③ 负控**自己重跑**并确认它能红）。
- **共享资源（C67）**：`panel/` 只有一棵树，没有 per-member 隔离 ⇒ **`t185` 之后我不改 `panel/src/**`**；若验证/评审要求再动，我会**先发一句「接下来 ~N 分钟 panel 树可能不可编译」**。

## 6. 两次事故登记（如实，含教训）

**(1) 我的在途编辑挡住了全队 panel 门禁（TS2693）。** 顺序是：先把「第三情形」加进 hook 的返回与 strip 的求和，**稍后**才把 `unknown` 加进组件解构 ⇒ 中间那几十秒里 `!unknown[n]` 落在**没有值绑定**的 `unknown` 上（TS 于是把它解析成**类型关键字**）⇒ `error TS2693`，`build-panel` 任一步非零 ⇒ `dist` 不动 ⇒ **同时挡住所有人的 `npm run build` 与「重建 dist 再跑 E2E」这条路**。`recall` 的归属判断正确（在途文件 ` M`、`git diff -U0` 的新增块、HEAD 的 L229 是别的行）。
**处置**：① 立刻把树恢复成能编译，并**复跑门禁 exit=0**（sha256 `5A1396ED…`）；② **不止修症状**：`unknown` 作为值名会遮蔽类型关键字 ⇒ 全部改成 `notKnown`（判别联合的字符串标签 `"unknown"` **保留**，那是值不是标识符），复跑门禁 **exit=0**（sha256 `9D40A563…`），残留 `setUnknown|unknown[` = **0**；③ 改动带**自证退路**（先备份 `%TEMP%\t185-Runtimes-before-rename.tsx`，build 非零就回滚复跑 —— **没触发**）。
**教训（建议入账，C65 同族）**：**在只有一棵树的共享资源上，中间态也是公共状态** —— 一次「先加引用后加声明」的编辑序列足以让**别人**的门禁红，并且**他读到的是你旧修订的字节**（所以他的「L229」与我当时的 L229 不同：**在途文件的读数必须带修订身份**，即哈希而不是「当前」）。

**(2) 我按命令行文本匹配「自己的孤儿」，把自己的工具调用 shell 杀了。** 收尾时我用 `Get-CimInstance Win32_Process | Where CommandLine -like '*t185-bin*'` 找孤儿并 `Stop-Process` —— 而**我这条命令自己的命令行里就含 `t185-bin` / `t185-root`** ⇒ 过滤器把**我自己的 pwsh** 匹配上并杀掉，命令以 job-runner 的 `exit code 4294967295`（「managed range 未证明为空」）中断，删除步骤只跑了一部分。
**处置**：改用**显式路径**删除（全部 `exists=False`）、**不再杀任何进程**、并**只读**列出可疑进程确认**没有真孤儿**（列出的两条是 harness 的 node 与我自己的 pwsh，文本自匹配）；确认活守护进程 **pid 14944 / 8787 健康 200 未受影响**；其他成员的临时守护进程（`t169\bin\ruagent.exe` 等）我**一个都没碰**。
**教训**：**「按名字/端口批量杀」的禁令也覆盖「按命令行文本匹配」** —— 过滤器里的模式**绝不能是本条命令自己会包含的字符串**；收尾只认**自己记录过的 PID**，杀之前先把候选打印出来看一眼。

## 7. 建议的提交信息

```
fix(panel): "never told" is not "0 models" on the runtimes page

The daemon learned to say "I was never told which options this runtime has"
(known:false, t182/C4). The panel still counted that as a runtime with 0 models:
the readout strip summed it in, and an all-unknown grid printed a plain 0 --
the reading t84 already forbids for a failed probe, because it is
indistinguishable from a runtime that really has none.

- one state helper per card (`modelChipState`) and one for the strip
  (`modelsReadout`), both exported and discriminated so a count is only
  reachable by narrowing `kind` -- an unknown cannot be formatted as a number.
- the unknown runtime contributes no count (a stale one is dropped), the chip
  falls to its existing third state, and a sync that came back "never told"
  reports a failure instead of a count.
- t84 is untouched and only extended: its branch, its label and its wording are
  byte-identical, and the all-success / all-failed / mixed sums are provably the
  same expressions as before.

Not proven: the trigger (a runtime whose first answer is empty but which has
options later) still needs a real ACP runtime; this removes the rendering path,
it does not demonstrate the outage. Panel i18n has no dedicated "unknown" string
yet, so the third state reuses `runtimes.notProbed`.
```
