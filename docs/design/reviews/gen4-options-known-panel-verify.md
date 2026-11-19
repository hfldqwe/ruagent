# t190 · 独立验证 t185（`known:false` 不被渲染成 `0`）—— **attempt 2：后端读数补上了**

**唯一写入**：本报告。`panel/**`、`crates/**`、`.github/**`、`scripts/**`、`panel/tools/**` 一行未改（stub、二进制、驱动、两臂全在 `%TEMP%`）。**8787 全程未访问**，真守护进程未启停。
**结论**：后端两种情形、①「未知不被持久化」、② 判别联合拿不到数字、③ `t84` 逐字未削弱 + 等价性表、④ 其余 4 个调用方 —— **五项全部由我自己量到并同意**；边界照抄见 §7。

## 1 身份（C69）

| 项 | 值 |
| --- | --- |
| 被测字节 | `panel/src/views/Runtimes.tsx`，commit **`a173d8b`**「fix(panel): an unknown option catalogue is not a count of zero」，父提交 `0b17721` |
| 字节三证 | blob **`759cdf9d`**（`git rev-parse a173d8b:…` = HEAD = `git hash-object` 三处相同），sha256 **`9D40A5638BCF6E21…`** ✓ 与申报逐字一致 |
| **后端读数用的守护进程** | **我自己从当前源码编的私有二进制** `%TEMP%\t190\bin\ruagent.exe`（`cargo-team.ps1 rustc -p ruagent --bin ruagent -- -o <私有路径>`，**exit 0**，sha256 **`FC1C1F6B4C62B948…`**）；临时 root `%TEMP%\t190\root6`；端口 **8899 / 8902 / 8903** |
| 两个 runtime（我自己写进 `config/agents.toml`，均显式 `enabled = true`） | `t190silent` = `harness="mock"` + `command="node %TEMP%\t190\silent.cjs"`（**我自己写的最小静默 ACP server**）· `t190advert` = `harness="mock"` + `command="%TEMP%\t190\mock-agent.exe"`（**我自己编译的真 mock agent**，`rustc -p ruagent-mock-agent --bin ruagent-mock-agent` exit 0） |
| 面板侧驱动 | 用 esbuild 把**真实的** `Runtimes.tsx` 打成 `%TEMP%\t190\runtimes.mjs`（6,097,495 B）后 `import` 其导出（我自己的驱动，非作者的 `control.mjs`） |
| **作废并重取的一组读数（披露）** | 先用共享 `D:\rust_cache\debug\ruagent.exe`（sha256 `0C97B01C2439B89E…`）跑过：它的响应是 `{"agent","options","cached","updated_at"}` —— **没有 `known` 字段**，且静默 runtime 返回 `options:[], cached:true`（t182 之前的行为）⇒ 那是**共享 target 目录里来自另一修订的产物**（AGENTS.md 那条陷阱）⇒ **该组读数作废**，重编私有二进制后重取（上表那台）。 |

## 2 后端两种情形（**我自己量**，私有二进制，8899/8902/8903）

| 读数 | 我实测的响应 | 判读 |
| --- | --- | --- |
| ① **从未被告知**（`t190silent` 第一次） | `{"agent":"t190silent","options":[],"cached":false,"known":false,…}`，**HTTP 200，20,014 ms**（= `MODELS_WAIT` 上界到点） | **`known:false` 且 `options:[]`** ✓ |
| ① **同 runtime 第二次** | `{"agent":"t190silent","options":[],"cached":false,"known":false,…}`，**20,011 ms** | **仍 `cached:false` ⇒ 「不知道」没有被持久化** ✓ |
| ② **第一手报告过**（`t190advert`） | `agent=t190advert cached=True known=True options_count=3`（`model` 2 选项 · `mode` 2 · `reasoning_effort` 3），**11 ms**（命中缓存） | **`known:true` 且有选项** ✓ |

**「我的静默 stub 什么都没说」是直接证明的，不是转述**：把两条 JSON-RPC 请求喂给 `silent.cjs`，**66 ms** 内回两行 —— `{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentCapabilities":{}}}` 与 `{"jsonrpc":"2.0","id":2,"result":{"sessionId":"silent-1"}}`；**回复不含 `configOptions`**（实测 `含 configOptions = False`）。
**过程披露（上一轮 404 的成因，已查明）**：`api.rs:4786-4790` 是 `state.mgr.agent(&name).filter(|a| a.enabled)` ⇒ **`enabled=false` 的 agent 就是 404**；上一轮我追加的 `[agent.mock]` 没写 `enabled`，而种子自带的 `[agent.mock]` 正是 `enabled = false` ⇒ 404 是**我的配置缺口**，与 t185 无关。这一轮两个条目显式 `enabled = true`，`/api/v1/agents` 里可见 `t190advert enabled=True harness=Mock` 与 `t190silent enabled=True harness=Mock`。

## 3 面板侧设计声明：未知分支拿不到数字 —— **同意**

对**真实打包产物**里的导出函数逐状态调用（我自己的驱动）：

| 我构造的输入 | 返回值 | 判读 |
| --- | --- | --- |
| `modelChipState(false,false,5)` | `{"kind":"count","n":5}` | 已知且有数 ⇒ 才给数字 |
| `modelChipState(false,TRUE,7)` | **`{"kind":"unknown"}`** | **未知 ⇒ 数字被丢弃**（不是格式化） |
| `modelChipState(TRUE,true,7)` | `{"kind":"failed"}` | `failed` 优先于 `unknown` ⇒ t84 判据不变 |
| `modelChipState(false,false,0)` / `(…,undefined)` | `{"kind":"notProbed"}` | 0 不算数（与旧真值判断同形，见 §4） |
| **攻击**：`modelsReadout({a:5},{},{a:true},…)` | **`"NOTPROBED-LABEL"`（typeof string）** | 陈旧计数**没有被求和** |
| `modelsReadout({},{},{},…)` / `({a:5},{},{a:true})` | 均为 string | 全未知 ⇒ 第三态文本，**不是 0** |

⇒ **我没能构造出让未知分支打印数字的输入**（设计声明成立）。**边界**：这是**规则层**读数，**不是 DOM 断言**（`panel/` 无单元测试运行器、`panel/e2e/**` 不在 inScope）⇒ 作者的负控（「忽略 `known` 恰好返回 0」）我判为**有判别力但弱于 DOM 断言**，差别在此明说。

## 4 `t84` 未被削弱（逐字级，C61 按提交号读）—— **同意**

* **文案**：父 `0b17721` 与 `a173d8b` 对照，`runtimes.probeFailed` / `runtimes.notProbed` / `runtimes.models { n: … }` **三处键与写法逐字保留**（前两者改为作 `labels` 传入，`models` 仍在同一个 `<span className="tag">`）。
* **删除行**：`git diff 0b17721 a173d8b -- panel/src/views/Runtimes.tsx` = **14 删 / 112 增**，删除行全是旧的 `?? 0` 兜底、旧三元与旧 `counts[r.name] ?` 真值门；门改为联合类型 `kind === "count"` ⇒ **收窄，不是放宽**。
* **等价性表我复刻了父提交的旧表达式**（逐字抄自 `git show 0b17721:…`）：`all-success` old `8` = new `8` ✓ ｜ `all-failed` old `"FAILED-LABEL"` = new ✓ ｜ `failed+answered` old `3` = new `3` ✓ ⇒ **三种旧情形逐项等价，唯一行为变化 = 全未知由 `0` 变成第三态**。

## 5 其余 4 个调用方（**同意「无 t84 类缺陷」**）

`Agents.tsx:512` 与 `TaskDetail.tsx:493` ⇒ `.then(r => setCatalog(r.options)).catch(() => setCatalog([]))`，存数组、按列表用（两文件内 `catalog.|.length` grep 无数值/文本渲染）｜`Chat.tsx:562/572` ⇒ `if (r.options.length === 0) { … agentOptions(agent, true, …) }` 再 `apply(r.options)`，只做**存在性**判断、把空目录当「不是答案」再探一次｜`Runtimes.tsx:88` = 本单改动处。⇒ 四处都不会把未知渲染成 `0` 或确定数字。**low 登记（非缺陷）**：除 `Runtimes.tsx` 外三处**不消费 `known`** ⇒ 「未知」在它们那里表现为空列表而非可区分的第三态，与作者自报边界一致。

## 6 门禁

`npm --prefix panel run build` ⇒ **exit 0**（6.35s）· `scripts/cargo-team.ps1 test -p ruagent-daemon` ⇒ **exit 0**（**217 passed / 0 failed / 8 ignored**，与作者同数）· `cargo-team.ps1 rustc -p ruagent --bin ruagent -o <私有>` exit 0 · `rustc -p ruagent-mock-agent --bin ruagent-mock-agent` exit 0 · **未跑任何 playwright**。

## 7 边界（照抄作者四条 + 我这次没覆盖什么，C63）

① **触发条件仍未证实**：真实 ACP 运行时「首答为空、稍后真有选项」这个转移我没观测到 —— 我用的是**我自己写的**静默 stub（已直接证明它什么都没说）与我编的 mock agent，二者都不是真实运行时。② **DOM 级渲染断言未测**（无单元测试运行器；`panel/e2e/**` 不在 inScope）⇒ §3 是规则层读数。③ i18n 不在 inScope（第三态复用 `runtimes.notProbed`，加不了专键）。④ `api.ts` 未改（`known` 靠一处窄 cast 读，属登记项）。⑤ `run.rs:265` / `O2` 只登记。⑥ **`known:false` 面板侧的端到端（浏览器里看到「未探测」而不是 `0 models`）我没有量** ⇒ 本单链条是「HTTP `known` 字段（我实测）→ 纯函数（我实测）→ 渲染分支（字节级读过，未在 DOM 里看）」。
