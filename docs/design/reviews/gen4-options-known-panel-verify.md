# t190 · 独立验证 t185（`known:false` 不再被渲染成 `0`）

**唯一写入**：本报告。`panel/**`、`crates/**`、`.github/**`、`scripts/**`、`panel/tools/**` 一行未改（驱动/CSS 打包全在 `%TEMP%`）。**结论先说**：**面板侧（设计声明 + t84 逐字 + 等价性表 + 4 个调用方）我独立复核并同意**；**后端两种情形的 HTTP 读数这一轮我没拿到（未测）** ⇒ 按 C63 本单记为**未完成**，理由与卡点写在第 4 节。

## 1 身份（C69）

| 项 | 值 |
| --- | --- |
| 被测字节 | `panel/src/views/Runtimes.tsx`，commit **`a173d8b`**「fix(panel): an unknown option catalogue is not a count of zero」，父提交 `0b17721` |
| 提交字节 vs 工作副本 | blob **`759cdf9d`**（`git rev-parse a173d8b:…` = HEAD = `git hash-object` 三处相同），sha256 **`9D40A5638BCF6E21…`** ⇒ 与 task 申报逐字一致 |
| 我那次的后端守护进程 | **`D:\rust_cache\debug\ruagent.exe`**（sha256 `0C97B01C2439B89E…`）作为**我自己的进程**：临时 root `%TEMP%\t190\root`（自动生成 `config/agents.toml` 等），端口 **8891 / 8892 / 8893**；**8787 全程未访问**，真守护进程未启停 |
| 驱动的真实函数 | 用 esbuild 把**真实的** `Runtimes.tsx` 打成 `%TEMP%\t190\runtimes.mjs`（6,097,495 B）后 `import` 其导出（我自己的驱动，非作者的 `control.mjs`） |

## 2 面板侧设计声明：**未知分支拿不到数字** —— 我独立驱动，**同意**

对**真实打包产物**里的导出函数逐状态调用：

| 我构造的输入 | 返回值 | 判读 |
| --- | --- | --- |
| `modelChipState(false,false,5)` | `{"kind":"count","n":5}` | 已知且有数 ⇒ 才给数字 |
| `modelChipState(false,TRUE,7)` | **`{"kind":"unknown"}`** | **未知 ⇒ 数字被丢弃**（不是格式化） |
| `modelChipState(TRUE,true,7)` | `{"kind":"failed"}` | `failed` 优先级高于 `unknown` ⇒ t84 的判据不变 |
| `modelChipState(false,false,0)` | `{"kind":"notProbed"}` | 0 不算数（与旧代码的真值判断**同形**，见 §3 等价性） |
| `modelsReadout({a:5},{},{a:true},…)`（**攻击：陈旧计数 + notKnown**） | **`"NOTPROBED-LABEL"`（typeof string）** | **陈旧数字没有被求和** |
| `modelsReadout({},{},{},…)`（全未知） | `"NOTPROBED-LABEL"`（string） | 全未知 ⇒ 第三态文本，**不是 0** |
| `modelsReadout({a:5},{},{a:true},…)` | `"NOTPROBED-LABEL"`（string） | 同上 |

⇒ **我尝试构造「让未知分支打印数字」的输入，没有构造出来**（设计声明成立）；判别联合把 `n` 只挂在 `count` 上，而渲染分支（`:413`）与函数出口都受它约束。**注意这条证据的边界**：它证明的是**规则层**（真实导出函数）拿不到数字，**不是**「真实 DOM 会渲染第三态」——**DOM 级断言我也没有量**（`panel/` 无单元测试运行器，`panel/e2e/**` 不在本单 inScope）；作者那条负控（「忽略 `known` 会恰好返回 0」）我据此判为**有判别力但弱于 DOM 断言**，差别在此明说。

## 3 `t84` 逐字级未削弱（C61：按提交号读）—— **同意**

* **文案**：父提交 `0b17721` 与 `a173d8b` 的 `runtimes.*` 用法逐条对照：`runtimes.probeFailed`、`runtimes.notProbed`、`runtimes.models`（`{ n: … }`）**三处键与写法逐字保留**（新代码把前两者作为 `labels` 传入，`models` 仍在同一个 `<span className="tag">` 里）。
* **删除行**：`git diff 0b17721 a173d8b -- panel/src/views/Runtimes.tsx` = **14 删 / 112 增**；删除行全是旧的 `?? 0` 兜底、旧三元表达式与旧 `counts[r.name] ?` 真值门。关键变化是门从「数字真值」换成「联合类型的 `kind === "count"`」⇒ **收窄，不是放宽**。
* **等价性表：我自己复刻了父提交的旧表达式**（逐字抄自 `git show 0b17721:…`），与新函数在三种旧情形上对照：
  `all-success` old `8` = new `8` ✓ ｜ `all-failed` old `"FAILED-LABEL"` = new ✓ ｜ `failed+answered` old `3` = new `3` ✓ ⇒ **三种旧情形逐项等价，唯一行为变化 = 全未知由 `0` 变成第三态** ⇒ 与作者的等价性表一致。

## 4 后端两种情形：**未测（这一轮没拿到）**，卡点如实写

* 我按纪律用**自己的进程 + 临时 root + 临时端口**起守护进程（health=ok，7s），`/api/v1/agents` 列出 4 个 runtime：`claude/ClaudeCode`、`dsh/Dsh`、**`mock/Mock`**（我写进 `config/agents.toml` 的 `[agent.mock] harness="mock"` **被接受**）、`opencode/OpenCode`。
* **卡点 A**：我为自己写的**最小静默 ACP server**（`%TEMP%\t190\silent.cjs`：`initialize` 回显 `protocolVersion`、`session/new` **只回 `sessionId`、不带 `configOptions`**）配的 `harness="acp"` **没有被注册进去**（列表里没有 `silent`）⇒ 未知的 harness 串不成立，我**没有**找到正确的通用 ACP harness 名。
* **卡点 B**：`GET /api/v1/agents/<name>/options` 在我这台守护进程上返回 **404**（`mock` 与 `silent` 都是），即作者读数用的那条路由**在我这次的环境里没有命中** ⇒ **`known` 字段我一个字节也没从 HTTP 层读到**。
* ⇒ **①（从未被告知 ⇒ `known:false`, `options:[]`）与 ②（第一手报告过 ⇒ `known:true`）我都没有读数**；**「未知不被持久化」（连问两次都 `cached:false`）同样没有读数**。这两条**不是** t185 的反证，是**我的环境没搭起来**（作者那两个现场 `%TEMP%\t185-bin`、`%TEMP%\t185-root` 已不存在，我无法沿用）。
* 另：我的静默 stub 自身的行为我**没有**用一次 JSON-RPC 交换证明（写出来了但没独立跑过它）⇒ 按「不许把作者说它静默当读数」的同一标准，**我也不会把我自己写的东西当读数**。

## 5 「其余 4 个调用方没有 t84 类缺陷」—— **同意（附一条 low 观察）**

我逐处读了作者点名的四个调用点：
* `Agents.tsx:512` ⇒ `.then((r) => alive && setCatalog(r.options)).catch(() => setCatalog([]))`：存的是**数组**，下游按列表用（`Agents.tsx` 内没有 `catalog.length` 的数值/文本渲染，我按 `catalog.|.length` grep 无命中）⇒ 不打印数字 ✓
* `TaskDetail.tsx:493` ⇒ 与上同形 ✓
* `Chat.tsx:562/572` ⇒ `if (r.options.length === 0) { … agentOptions(agent, true, …) }` 再 `apply(r.options)`：**只做存在性判断**，且把空目录当「不是答案」再探一次 ⇒ 不打印数字 ✓（这里消费的是 `options`，**没有**读 `known`）
* `Runtimes.tsx:88` ⇒ 本单改过的那处（读 `known`）✓
⇒ **我同意「无 t84 类缺陷」**：四处都不会把「未知」渲染成 `0` 或某个确定数字。**low 观察（不是缺陷）**：除 `Runtimes.tsx` 外三处**不消费 `known`**，所以「未知」在它们那里表现为**空列表**而不是可区分的第三态 —— 这与作者自报的边界（C4 后半只落到 `Runtimes.tsx`）一致，登记给后续单，不构成本单的反证。

## 6 门禁与收尾

* `npm --prefix panel run build` ⇒ **exit 0**（`✓ built in 6.35s`，wrapper 的 i18n-check/tsc 步均过）。
* `scripts/cargo-team.ps1 test -p ruagent-daemon` ⇒ **exit 0**（lib 159 · capabilities 11 · 5 · 2 · graph_ingest 16 · injection_e2e 0(+8 ignored) · knowledge_api 16 · recall_evidence_announced 3 · smoke 0 · version_points 5 ⇒ **217 passed / 0 failed / 8 ignored**）——与作者申报同数。
* 我自己的守护进程 PID `35696`/`2852`/`37752` **均已按 PID 停止**（未按名/端口/命令行批量杀）；未访问 8787（只读探测也没做）；`%TEMP%\t190` 下的东西按具体路径删除。

## 7 我这次验证**没有**覆盖什么（照抄作者边界 + C63）

① **触发条件仍未证实**：「首答为空、稍后真有选项」需要真实 ACP 运行时 —— 我连静默 stub 那条路都没打通（§4），所以这条**比作者更未覆盖**。② DOM 级渲染断言未测（无单元测试运行器；`panel/e2e/**` 不在 inScope）。③ i18n 不在 inScope（第三态复用 `runtimes.notProbed`）。④ `api.ts` 未改（`known` 靠一处窄 cast 读，属登记项）。⑤ `run.rs:265` / `O2` 只登记。⑥ **后端 `known` 字段的端到端读数这一轮缺失**（§4）⇒ 本单**不能**算作对 `t185` 的完整独立验证。
