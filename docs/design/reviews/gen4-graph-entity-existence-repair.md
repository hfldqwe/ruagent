# graph_entity：把「不存在」读成「存在但无事实」（t118 / t93 M1 前半）

> 状态：**完成，等 captain 推送**。写入集合 = `crates/mcp/src/lib.rs` · `crates/mcp/tests/roundtrip.rs` · 本报告。
> `git diff --stat`：**2 files changed, 215 insertions(+), 13 deletions(-)**（`lib.rs` +113 · `roundtrip.rs` +115）。
> 作者 recall，时间窗 2026-10-04T00:0x → 00:2x（含三次宣告过的负控窗口）。

---

## 0 一句话

daemon 一直在 wire 上写着存在信号（`name: null` = 该 id 不在图里），**是消费面没去读它**：MCP 工具读的是两个从未存在过的键（`entity` / `exists`），于是判存在恒为「存在」，一个瞎编的 id 被答成**成功**：`entity #N has no current facts`（`is_error: Some(false)`）。本单把判定换成读真实信号，并把「不存在 ⇒ 拒绝」与「存在但零事实 ⇒ 那句文本」钉成两件可分的事实。

## 1 实测的 wire 原文（当前字节，**重建二进制之后**）

| 情形 | 真实响应（`GET /api/v1/graph/entity/{id}`） | HTTP |
| --- | --- | --- |
| **id 不在图里**（`987654321`） | `{"name":null,"kind":null,"aliases":[],"facts":[]}` | **200** |
| **实体存在、零事实**（POST 建 `t118-probe-without-facts`，id=1） | `{"name":"t118-probe-without-facts","kind":"concept","aliases":[],"facts":[]}` | **200** |

⇒ **存在信号逐字是：`name` 这个键的 `null` 值**（存在时它是字符串，哪怕空串）。`name` 之外没有任何键能区分这两种情形：`facts` 两边都是 `[]`，状态码两边都是 200。

**⚠️ 顺带发现（会影响任何用预编译二进制取数的人）**：`%TEMP%\ruagent-team-target\debug\ruagent.exe` **是陈旧的** —— 用它取数时那个不存在的 id 回的是 **`{"facts":[]}`**（旧 wire 形状，缺 `name`/`kind`/`aliases`）。`build -p ruagent`（`Compiling ruagent-daemon` → `Finished in 1m11s`）之后才回上表的值。**用陈旧二进制取数会「推翻」这个信号**；我报告里的 wire 读数一律取自重建后的字节。

## 2 daemon 侧坐标（本单**逐条对当前字节回读**，未沿用任何转述）

| 坐标 | 内容 |
| --- | --- |
| `crates/daemon/src/api.rs:1328` | `async fn graph_entity`（路由 `/api/v1/graph/entity/{id}`，注册见 `:57-59`） |
| `api.rs:1336` | `let entity = ruagent_graph::entity_by_id(db, id).await?;` ⇒ 行不存在时是 `None` |
| **`api.rs:1342`** | **`"name": entity.as_ref().map(\|e\| e.name.clone()),`** ⇒ **`null` ⇔ 没有这一行** |
| `api.rs:1341-1346` | 响应字面量**只有四个键**：`name` / `kind` / `aliases` / `facts`（**没有** `entity`、**没有** `exists`） |
| `api.rs:1339-1340` | 注释原文：不在图里的 id **保留 200 + 空集合**，**不变 404** ⇒ 状态码不携带存在信息 |
| **钉住它的 daemon 测试** | `api.rs:6673` `the_entity_route_exposes_the_alias_text_and_the_list_opt_in_is_additive`：`:6745` 断言 200、**`:6746` 断言 `missing["name"].is_null()`**、`:6747-6750` 断言 `facts == []`。本单实跑：`cargo-team.ps1 test -p ruagent-daemon --lib` ⇒ **156 passed, 0 failed**，该用例 `... ok` |

## 3 改前 → 改后（同一条路径的两个方向）

### 3.1 改前（实测）
纯读法在真实响应上给出 **`Ok(true)`**（=「存在」），而同一个响应是 `{"name":null,…}` —— 负控窗口 #1 的机器读数原文：
```
assertion `left == right` failed: the pure reading must agree with the wire: {"name":null,"kind":null,"aliases":[],"facts":[]}
  left: Ok(true)
 right: Ok(false)
```
客户端可见的答案（负控窗口 #3 实测原文）：
```
expected a refusal, got a successful result: entity #987654321 has no current facts /
CallToolResult { … content: [Text(TextContent { text: "entity #987654321 has no current facts", … })], …, is_error: Some(false), … }
```
⇒ **假成功**：`is_error = Some(false)`，文本是「这个实体没有当前事实」——调用方无从知道这个 id 根本不存在。

### 3.2 改后
- 对不存在的 id：返回 **JSON-RPC 错误**（`refusal()` 形状 = `Err(ErrorData)` ⇒ 客户端看到 `isError=true`），文本
  `entity #987654321 not found: the graph has no entity with that id`（**点名调用方发来的 id**）。测试断言 `message.contains("not found")` 与 `message.contains("987654321")`，并显式断言它**不含** `has no current facts`。
- 对存在但零事实的实体：**仍是成功**（`is_error != Some(true)`），文本仍 `entity #1 has no current facts`。
- 两个方向并排住在 `crates/mcp/tests/roundtrip.rs`（`a_fabricated_entity_id_is_refused_not_answered_with_no_facts` / `an_entity_with_no_current_facts_is_still_a_success`），并且**两个都会先读出真实 wire JSON** 再断言。

## 4 实现（判存在 = 一个可单测的纯函数）

`crates/mcp/src/lib.rs` 新增 `pub fn entity_exists(resp) -> Result<bool, String>`（与既有 `facts_of` 同一位置、同一风格），**四个分支都有语义**：

| 输入 | 返回 | 理由 |
| --- | --- | --- |
| `name` 是字符串（含 `""`） | `Ok(true)` | 行在那里；`null` 才是「不在」 |
| `name` 是 `null` | **`Ok(false)`** | daemon `api.rs:1342` 的 `map(...)` ⇒ 没有这一行 |
| 没有 `name` 键 | `Err` | 信号键被改名 ⇒ **报错**，不是「不存在」 |
| `name` 是别的类型 | `Err`（带 `json_kind`，报出实际类型） | 信号被改类型 ⇒ **报错** |

**与 `facts_of` 同一条纪律**：漂移必须炸出来，不能被吸收成默认值 —— 这正是旧读法（`unwrap_or(false)`）的反面。
**旧分支已删除，不是保留**：`resp["entity"]` / `resp["exists"]` 两个键 daemon 从未发出（`api.rs:1341-1346` 的响应字面量只有四个键），所以那个「已经支持存在字段」的注释是**假的**，两个分支也从不可能命中；代码里现在**零命中**（`Select-String 'NEGATIVE CONTROL|entity_missing|resp.get("entity")|resp.get("exists")'` = 空）。原注释 `lib.rs:225-231` 的「carries no existence field and no 404」已按实测改写：信号是 `name`，坐标与钉住它的 daemon 测试写在新函数上，且**所有行号都在本单对当前字节回读过**。

## 5 负控：三个窗口（全部**先宣告**、限时、按字节恢复）

| 窗口 | 起止 | 变异 | 红面读数 | 恢复 |
| --- | --- | --- | --- | --- |
| **#1** | `00:12:37` → 恢复 `00:16:08` → 结束 `00:16:33`（**3m56s**，宣告上界 ≤5min） | `entity_exists` → 旧读法 | **`6 passed; 1 failed`**：红的是**「不存在」方向**（`:388` 的纯读法/wire 不一致，原文见 §3.1）；**`an_entity_with_no_current_facts_is_still_a_success` 保持 `ok`** ✓ | `sha256(lib.rs)` 前/后 **`52E9F00A…B4B9EB` 完全一致**；重跑 **`7 passed; 0 failed`** |
| **#2** | `00:19:52` → 恢复 `00:20:41` → 结束 `00:21:05`（**1m13s**） | 同上 + 想把断言挪到工具调用之后 | 我这次的插入点错了（断言仍在工具调用**之前**），红面与 #1 同形 ⇒ **没取到新读数**（诚实记账，也是为什么有第三个窗口） | 两文件哈希**均一致**；重跑 `7 passed` |
| **#3** | `00:21:23` → 恢复 `00:21:46` → 结束 `00:22:09`（**46s**，宣告上界 ≤2min） | 同上 + 一个**临时探针块**（恢复时删除） | **取到 §3.1 的客户端可见原文**（`is_error: Some(false)`） | `lib.rs` 与 `roundtrip.rs` 哈希**均一致**；重跑 **`7 passed; 0 failed`** |

**影响面（capped）**：三次窗口都只用 `-p ruagent-mcp --test roundtrip`（**没有**用 `--workspace`），受影响的 target 最多一个、用例最多一条。窗口外若有人看到 `crates/mcp` 之外的任何红，那不是我的窗口。

## 6 门禁读数（第 22 条形态）

| 命令 | 读数 |
| --- | --- |
| `cargo-team.ps1 test -p ruagent-mcp` | **exit 0**（50.6s）：lib `ok. 44 passed` · roundtrip `ok. 7 passed`（含本单两条新用例）· tool_surface `ok. 3 passed` · doc-tests `ok. 0` |
| `cargo-team.ps1 test -p ruagent-daemon --lib` | **exit 0**（34.0s）：`ok. 156 passed; 0 failed`，含 `api::tests::the_entity_route_exposes_the_alias_text_and_the_list_opt_in_is_additive ... ok` |
| `cargo-team.ps1 clippy -p ruagent-mcp --all-targets -DenyWarnings` | **exit 0**（13.2s）：`Checking ruagent-mcp` → `Finished`，`^warning`/`^error` **0 行** |
| `cargo fmt --all --check` | **exit 0**（先对本单两个文件 `rustfmt --edition 2024 --check` 报过 2 处 `Diff in`，只格式化我这两个文件后复检 0） |
| 测试环境 | `NO_PROXY=127.0.0.1,localhost,::1`（本机系统代理指向无人监听的 `127.0.0.1:7890`，AGENTS.md 记过）；两个新用例都做真实 loopback HTTP |

## 7 不覆盖什么 / 登记（第 19 条）

1. **`name` 是「展示字段兼信号」**：今天 `name: null` 与「没有这一行」严格等价，因为它由 `entity.as_ref().map(...)` 产生；但这是**耦合**——若 daemon 哪天把它改成 `unwrap_or_default()`（空串），信号会静默失效（那时 `entity_exists` 会说「存在」，而 daemon 侧测试 `:6746` 会立刻红，所以**不是无保护**，但保护在 daemon 侧的测试上，不在协议上）。
   **登记（不做）**：在 `crates/daemon/src/api.rs` 里加一个**为存在性而设的显式字段**（例如 `exists: bool`）在工程上更自明 —— 但那要动 `api.rs`，该路径当前被另一张在办单占着（inScope 重叠）⇒ **本单只登记、不做**，留给后续轮次。
2. **未测**：客户端可见的**拒绝**文本是「被断言包含」而不是「被打印出来」（测试用 `refusal()` 拿 `Err` 的 Display 并断言 `contains("not found")` 与 id）；`rmcp` 对 `Err(ErrorData)` 的 JSON-RPC 包装文本本身没有逐字打印。改前的方向是**实测原文**（§3.1）。
3. **未测**：`cargo test --workspace` 未在本单跑（契约的 verify 只要求 `-p ruagent-mcp` 与 `-p ruagent-daemon --lib`）；工作区其它 crate 的红与此无关。
4. **历史坐标**：任务单里给的 `lib.rs:232-236` / `:225-231` / `api.rs:1207-1216` 在本单的当前字节上**已漂移**；本报告所有坐标都是**本单回读**的（见 §2、§4），代码里也不再依赖任何未复核的行号。

## 8 纪律回执

- 写入集合：`crates/mcp/src/lib.rs` · `crates/mcp/tests/roundtrip.rs` · 本报告（**全部在 inScope**）。**`crates/daemon/**` 零改动**（`git status` 可证）。
- 三次负控窗口全部**先宣告**、限时（3m56s / 1m13s / 46s）、按字节恢复（三个窗口的恢复哈希都已记录并在 §5 给出；窗口 #2 是我自己的插入点失误，已如实记账）。
- 临时守护进程：本单共起 3 个，**自记 pid 30780**（负控窗口 #1）· **15748**（第一次原始 JSON 取数，那次 curl 抢在健康检查之前、读数被我作废）· **7860**（重建二进制后的原始 JSON 取数）—— 三个全部停止；临时 root `ra-t118-root` / `ra-t118-root2` / `ra-t118-root3` 全部删除；**活守护进程 pid 79984 与活库 `~/.ruagent` 全程未被触碰**（只读健康检查）。
- 未 push / dispatch / rerun / cancel / tag。

## 9 交付后改动：披露 + 在最终字节上复跑（AGENTS.md「green 门只认证它跑过的字节」t20）

### 9.1 披露（改动发生在 `00:25:56` 前后）
| 文件 | 报告落笔时 | 现在（`00:28:17`） |
| --- | --- | --- |
| `crates/mcp/src/lib.rs` | `sha256 52E9F00A…B4B9EB`，numstat **100/13** | `sha256 2BF35EEC…141421`，numstat **222/13** |
| `crates/mcp/tests/roundtrip.rs` | `sha256 6C24593E…083287`，numstat **115/0** | **与 HEAD 无差异**（已整文件还原） |

**移动了什么**：两条端到端用例 + 其夹具（`stub_daemon` / `ToolClient` / `graph_entity_as_a_caller_sees_it`）从 `crates/mcp/tests/roundtrip.rs` 移入 `crates/mcp/src/lib.rs` 的 `mod tests`。最终坐标：`stub_daemon` **L1679** · `a_fabricated_entity_id_is_refused_not_answered_with_no_facts` **L1763** · `an_entity_with_no_current_facts_is_still_a_success` **L1781** · 纯函数用例 `entity_existence_is_read_from_name_and_drift_is_not_absorbed` **L1795**。

**为什么**：**平台把 `crates/mcp/tests/roundtrip.rs` 判为 `undeclared` 并拒绝 t118 完成** —— 契约 inScope 写的是目录 `crates/mcp/tests`，而完成校验要求**精确文件路径**。为了不改契约、不扩面，把用例搬进已声明的 `src/lib.rs`。（这是本单遇到的一个真问题：目录式 inScope 与精确路径校验器不兼容，值得记一笔。）

### 9.2 哪些读数属于哪一份字节
- **属于改动前字节（`52E9F00A…`）**：§6 的四门读数（mcp exit 0 / 50.6s；daemon-lib 156；clippy 13.2s；fmt 0）· **三个负控窗口的全部读数**（#1 `6 passed; 1 failed`、#2、#3 的客户端可见原文）· §1 的 wire 原文（那是**二进制**读数）。
- **属于改动后最终字节（`2BF35EEC…`，`00:28:17` 取）**——这才是我现在提交的字节：
  - `cargo-team.ps1 test -p ruagent-mcp` → **exit 0**（26.5s）：lib `test result: ok. 46 passed; 0 failed`（含新用例 `tests::a_fabricated_entity_id_is_refused_not_answered_with_no_facts ... ok`、`tests::an_entity_with_no_current_facts_is_still_a_success ... ok`、`tests::entity_existence_is_read_from_name_and_drift_is_not_absorbed ... ok`）· roundtrip `ok. 5 passed` · tool_surface `ok. 3 passed` · doc-tests `ok. 0`
  - `cargo-team.ps1 clippy -p ruagent-mcp --all-targets -DenyWarnings` → **exit 0**（4.4s，`^warning`/`^error` 0 行）
  - `cargo fmt --all --check` → **exit 0**；`rustfmt --edition 2024 --check crates\mcp\src\lib.rs` → exit 0
  - `cargo-team.ps1 test -p ruagent-daemon --lib` → **exit 0**（20.6s）：`ok. 156 passed`，含 `api::tests::the_entity_route_exposes_the_alias_text_and_the_list_opt_in_is_additive ... ok`

### 9.3 协议层证据的状态（实质问题，不是形式）
移动后这两条用例**不再驱动真守护进程**：它们对**一个 axum stub** 发真实 HTTP、经由**真实 MCP 协议**（rmcp duplex）调用工具，而 stub 回的正是**本单从真守护进程实测到的两份 JSON 原文**（§1）。
⇒ 交付物覆盖的是**工具出口**这一层（两方向各一条，断言的正是客户端可见形状）；**「真实 daemon 路由」这一层不再由本单的测试驱动**。按 captain 的口径：**协议层（真路由）证据改由 t119 独立验证取**，可直接复用的材料已备好：
1. **真路由夹具**：`crates/mcp/tests/roundtrip.rs` 的 `start_test_daemon_with_policy(..)`（真守护进程 + 临时 root）与 `mcp_pair(..)`（rmcp duplex）—— 该文件当前**与 HEAD 无差异**，夹具现成；
2. **断言配方**：先读 wire（`raw["name"].is_null()` ⇒ 该 id 不在图里；`raw["name"].is_string()` ⇒ 在）**再**断言工具出口；「存在但零事实」用 `POST /api/v1/graph/entity`（body `{"name":…,"kind":…}`）造，**不加任何 fact**；
3. **本单实测的两份原始 JSON** 与三窗口读数（§1/§5）；
4. 负控配方：把 `entity_exists` 换回旧读法 ⇒ 只有「不存在」方向红，约 **46 秒**（窗口 #3 的形状）；**本单不再开窗口**（captain 已裁定成本到此为止），若 t119 要跑，建议照窗口 #3 的宣告形状做。

### 9.4 与负控的关系（一处必须说清）
三个窗口的变异对象始终是 **`entity_exists`**（两版字节里都是它），窗口读数是在**移动前**的用例上取的；移动换掉的是**夹具**（真守护进程 ⇒ stub），**没有改** `entity_exists`、也没有改被断言的出口语义。所以 §5 的负控结论对最终字节仍然成立（同一个函数、同一条拒绝分支），但**严格按「门只认证它跑过的字节」**：§5 的窗口读数属于 `52E9F00A…`，最终字节上的窗口未跑（由 9.3 的配方交给 t119）。

