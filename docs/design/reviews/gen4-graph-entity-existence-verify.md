# gen4 独立验证 — `graph_entity`：「不存在」与「存在但零事实」是可分的两件事（t119）

> 被验对象：t118（`ruagent-tunable-capabilities t118`，已随提交 **`ec24b61`** 落地）。
> 我的写入集合 = 本文件。**没有改任何被测代码**；`crates/mcp/src`、`crates/daemon/src`、`crates/graph`、`panel`、`.github`、`scripts` 零字节改动。
> 环境：**没有触碰** 127.0.0.1:8787 的常驻守护进程与活库 `~/.ruagent`；我的一切实验在自己的临时 root（`%TEMP%\t119\root`）+ 临时端口（**8821**）+ 自有 python stub（**8822**）上。
> ⚠️ 环境变动（我先报出来）：我的任务文字里点名要避开的 **pid 79984 在今天已经不存在了**；8787 现在是 **pid 14944**（`D:\rust_cache\debug\ruagent.exe`，StartTime **2026/10/2 23:05:45**），`/api/v1/health` 200。两个都不是我起停的（我的脚本每次都会打印这一行，00:33 那次打印为空——就是这次变动的观测点）。

## 0 结论

**t118 的声称成立，我在最终字节上用自己的原始读数复核通过**：不存在的实体 id 在 MCP 工具出口是**拒绝**，存在但零事实的实体是**成功 + 文本**；判存在的信号确实是 daemon 早就写在 wire 上的 **`name`**（`null` ⇔ 该 id 不在图里）。**另外两条也成立**：消费者对信号漂移是**失败关闭**（我构造的 5 种旧/漂移形状全部拒绝，无一被读成"零事实"）；**残余风险不构成 finding**（`entities.name` 是 `TEXT NOT NULL`，且**空串名字**我实测能造出、且被正确读成"存在"）。

**我给出的三条与作者不同/作者没说的读数**：

1. **refusal 在 stdio 上的形状是 JSON-RPC error（`code -32603`），不是 `CallToolResult{isError:true}`**。作者报告里写"客户端 `isError=true`"，那是他进程内 rmcp 客户端的措辞（他的 helper `crates/mcp/src/lib.rs:1736-1752` 把 `Err` 与 `is_error==Some(true)` 都算"拒绝"）；**两种都是拒绝**，规格上客户端必须处理 JSON-RPC error，所以不构成缺陷——但"客户端看到的是 isError"这句话**不是 wire 上的事实**，我按 rig 原文给出。
2. **作者报的 G2（`test -p ruagent-daemon --lib`）在他那次是 `exit 0 / 156 passed`；我这次是 `exit 101 / 156 passed + 1 failed`**，失败用例是 **`runs::tests::the_injection_budget_the_event_carries_is_measured_not_all_zero`**，`panicked at crates\daemon\src\runs.rs:2148`。**归因不是 t118**：`crates/daemon/src/runs.rs` 在我跑门禁的**窗口内**被同伴改动（`git status --porcelain` = ` M crates/daemon/src/runs.rs`，mtime **00:34:25**，我的门禁窗口 00:34:22–00:35:29），而 **t118 自己的钉住用例 `api::tests::the_entity_route_exposes_the_alias_text_and_the_list_opt_in_is_additive ... ok`** 在同一次运行里通过。按纪律报"哪个文件、什么错"，我不修。
3. **共享构建树里的 `ruagent.exe` 并不是"陈旧到没有存在信号"**：它 mtime `2026-10-04T00:18:35`，**字符串里已含 t118 的新消息** `not found: the graph has no entity with that id`（我做了二进制字符串探针）⇒ 它属于 t118 **实现之后、最终字节之前**的那一版；我全程用自己构建的二进制（见 §1），所以这条只是把 C35 的"先证明年份"落到实处：**我探到了它的年份，没有拿它读数**。

**一条 low finding（残余风险侧，见 §5）**：`entities.name TEXT NOT NULL` 是这条信号**唯一**的健全性依据，而**全仓没有任何测试钉住它**（我扫了 graph/store/daemon 的测试面，没有任何"插入 NULL name 必须失败"的断言）——如果将来有表重建把 NOT NULL 丢掉，一行 `name = NULL` 的**存在的实体**会被读成"不在图里"，而现有测试面不会红。

## 1 方法与仪器（我用的哪一种，可复现）

**我选"真守护进程 + 真 MCP 协议 + 我自己的客户端"这条路径**（不用进程内 axum stub，也不用作者的任何读数）：

| 步骤 | 具体命令/路径 |
| --- | --- |
| 构建（避开被占用的共享 exe） | `scripts/cargo-team.ps1 rustc -p ruagent --bin ruagent -- -o %TEMP%\t119\ruagent-new.exe` ⇒ exit 0，59.0 s，`328,074,240` B，mtime `2026-10-04T00:32:08` |
| 临时守护进程 | 我的二进制 `serve --addr 127.0.0.1:8821 --root %TEMP%\t119\root`（自带 `config/{agents,mcp,policy}.toml`），`RUAGENT_EMBEDDER=hash` |
| wire 读数 | `%TEMP%\t119-wire.py`（urllib，我自己的 HTTP 客户端） |
| 工具边界 | `%TEMP%\t119-mcp.py`：**真 stdio MCP 客户端**（`initialize` → `tools/call graph_entity`），跑的是 `ruagent-new.exe mcp-serve`，`RUAGENT_URL` 指向我的 8821 ⇒ 真 JSON-RPC → 真 HTTP → 真 daemon 路由 |
| 漂移/证伪 | `%TEMP%\t119-stub.py`：我的 python stub 守护进程在 **8822**，按 id 返回 5 种旧/漂移形状，仍由**真 `mcp-serve`** 经真 MCP 协议去打它 |
| 门禁 | §4 的四条，全部经 `scripts/cargo-team.ps1`（同一时刻一个 rust 构建） |

**「存在但零事实」的确切构造步骤**（我自己做的，不是复用作者的）：对**我自己的**临时 root 发 `POST /api/v1/graph/entity`，body `{"name":"t119-verify-zero-facts","kind":"concept"}` ⇒ `HTTP 200 {"id": 1}`；**不加任何 edge/fact**；再 `GET /api/v1/graph/entity/1`。空串名字那个用 body `{"name":"","kind":"concept"}` ⇒ `{"id": 2}`。

## 2 两个方向的原始读数（我的，原文）

### 2.1 方向一：**不存在**的 id

```
# wire（我自己的 daemon，端口 8821）
GET /api/v1/graph/entity/987654321 -> HTTP 200  raw={"name":null,"kind":null,"aliases":[],"facts":[]}
# 工具出口（真 MCP stdio，my binary mcp-serve，RUAGENT_URL=http://127.0.0.1:8821）
id='987654321' -> JSONRPC ERROR {"code": -32603, "message": "entity #987654321 not found: the graph has no entity with that id"}
```
读数：**拒绝**，且拒绝里**点名了调用方发的 id**，并且**不含** `has no current facts`。

### 2.2 方向二：**存在但零事实**

```
POST /api/v1/graph/entity {"name":"t119-verify-zero-facts","kind":"concept"} -> HTTP 200 {"id": 1}
GET  /api/v1/graph/entity/1 -> HTTP 200 raw={"name":"t119-verify-zero-facts","kind":"concept","aliases":[],"facts":[]}
# 工具出口
id='1' -> isError=False text='entity #1 has no current facts'
   RAW RESULT: {"content": [{"type": "text", "text": "entity #1 has no current facts"}], "isError": false}
```
读数：**成功 + 今天的文本**；与 2.1 的 `facts` 数组**完全相同**（都是 `[]`）⇒ 分出这两件事的确**只能靠 `name`**，不可能靠 `facts`。

### 2.3 信号来源与它被钉住的地方

- **信号 = 响应对象里的 `name` 键；`null` ⇔ 该 id 不在图里；任何字符串（含空串）⇔ 行在那里。**
- 发出者（符号优先；坐标读于 **2026-10-04**，`crates/daemon/src/api.rs` sha256 `C43BACC0A535A107…`，mtime `00:19:28`，最后一次提交 `ec24b61`）：
  - 路由 `:57` `"/api/v1/graph/entity/{id}"`；
  - handler `async fn graph_entity`（`:1347`），其中 `:1361` `"name": entity.as_ref().map(|e| e.name.clone()),` —— `entity` 来自 `ruagent_graph::entity_by_id`，**`None` ⇒ `null`**；四键字面量 `:1360-1365`（`name/kind/aliases/facts`）；`:1358-1359` 的注释明说"不在图里的 id 保持 200 + 空集合，不变 404"。
- **钉住它的 daemon 侧测试（我亲自在当前字节上定位，行号对上）**：
  - 测试 `api::tests::the_entity_route_exposes_the_alias_text_and_the_list_opt_in_is_additive`，`async fn` 在 **`api.rs:6711`**；
  - `:6783` `assert_eq!(st, StatusCode::OK, "{raw}");`（不在图里也是 200）；
  - **`:6784` `assert!(missing["name"].is_null(), "{raw}");`**（就是这条钉住 `null`）；
  - `:6785-6788` 断言 `facts == []`；
  - **同一测试的 `:6738` `assert_eq!(v["name"], "AMBIGUOUS", "{raw}");`** —— 这条很关键（见 §3.1）。
  - **我的运行读数**：`test api::tests::the_entity_route_exposes_the_alias_text_and_the_list_opt_in_is_additive ... ok`（在我自己的 G2 门禁运行里；§4）。
- 这条测试**确实钉住了信号**，而且**钉的是一对**：`name` 的存在+值（`:6738`）与 `name == null`（`:6784`）。任务单说"若它只断言 facts 为空就是 finding"——**它不断言 facts 作为存在判据**，所以**不是 finding**。

### 2.4 可证伪性（≥3 个可能把两件事重新合并的输入）

**（a）经真 daemon 路由 + 真 MCP 协议（我的二进制）**：

| 输入 | wire | 工具出口 | 会不会把两件事合并 |
| --- | --- | --- | --- |
| `id = -1`（负数） | `{"name":null,…}` 200 | JSONRPC ERROR `entity #-1 not found…` | 否（正确读成不存在） |
| `id = 0` | `{"name":null,…}` 200 | JSONRPC ERROR `entity #0 not found…` | 否 |
| `id = 9223372036854775807`（i64::MAX，超出现有范围） | `{"name":null,…}` 200 | JSONRPC ERROR `entity #… not found…` | 否 |
| `id = "abc"`（非数字，经工具参数） | ——（工具参数是 `i64`，到不了 HTTP） | `isError=true` `failed to deserialize parameters: invalid type: string "abc", expected i64` | 否（在**参数边界**失败关闭） |
| 路由直打 `…/entity/abc`、`…/entity/99999999999999999999`（溢出 i64） | `HTTP 400 invalid entity id` | ——（工具参数是 i64，无法产生这两种 id） | 否（要经工具就得是 i64；绕过工具是 400） |
| **空串名字的实体**（`POST {"name":""}` ⇒ id 2） | `{"name":"","kind":"concept","aliases":[],"facts":[]}` 200 | `isError=False` `entity #2 has no current facts` | **否**：空串被正确读成**存在**（这正是"对存在的实体也可能为空"的那条轴，见 §5） |

**（b）构造"旧形状"的响应（不改被测代码——`crates/mcp/src` 明确在我的 inScope 之外）**：我的 stub 只做一件事：返回**旧读法当年期待的形状**，让**真 `mcp-serve`** 去读：

```
STUB: 101 -> {"entity": null, "facts": []}         102 -> {"entity_name": "Kubernetes", "facts": []}
      103 -> {"name": 7, "facts": []}              104 -> {"facts": []}          105 -> HTTP 404
工具出口（真 MCP）:
  101 -> JSONRPC ERROR "daemon response carries no `name` key, and `name` is the entity-existence signal (`name: null` = that id is not in the graph): {"entity":null,"facts":[]}"
  102 -> JSONRPC ERROR "… carries no `name` key …: {"entity_name":"Kubernetes","facts":[]}"
  103 -> JSONRPC ERROR "daemon response has `name` of type number (expected a string, or null …): {"name":7,"facts":[]}"
  104 -> JSONRPC ERROR "… carries no `name` key …: {"facts":[]}"
  105 -> JSONRPC ERROR "graph_entity refused by the ruagent daemon (HTTP 404): not found"
```
⇒ 5/5 **失败关闭**；没有任何一种漂移被读成"存在且零事实"。

**（c）旧判读在同样输入上的行为（判定"测试有没有判别力"）**：从 **`ec24b61` 自己的 diff** 里取出的、被删掉的那几行（原文）：

```rust
        // M1 (first half): "this entity does not exist" must not read the same as
        // "this entity exists with no current facts". The daemon's
        // `GET /graph/entity/{id}` (api.rs:1207-1216) returns `{"facts": []}` for
        // BOTH today -- it carries no existence field and no 404 -- so this tool
        // cannot invent the distinction on its own. The branch below already
        // honours an existence field (`entity: null` or `exists: false`), so the
        // daemon side is a one-line change; see the finding in the report.
        let entity_missing = resp
            .get("entity")
            .map(|e| e.is_null())
            .or_else(|| resp.get("exists").and_then(|e| e.as_bool()).map(|b| !b))
            .unwrap_or(false);
        if entity_missing {
```
- 我实测的**两份真 wire 原文**都**没有** `entity` 也没有 `exists` 键 ⇒ 旧判读对**两者都**得到 `entity_missing = false` ⇒ 两个方向**都**走"成功 + `has no current facts`"⇒ **旧读法把两件事合并**，新测试（断言一个是 `Err`、一个是 `Ok(text)`）**确有判别力**。
- 旧注释自己断言"daemon 不携带存在字段"——**被我的读数直接推翻**：`name` 自 t22 起就在（§2.3 的 `:1334-1346` 注释 + `:6738` 断言），**信号一直在，缺的是读它的人**。这与作者的根因判断一致。
- **我没有做**"把 `entity_exists` 改回旧读法"的源码变异（那需要写 `crates/mcp/src`，超出我的 inScope，也是本单纪律禁止的）；我用的是"构造旧形状响应"这一等价路径（上面 (b)），并给出旧判读在被测字节里的**原文**与它对同一批输入的结果。

### 2.5 残余风险（点名字段 + 依据）

- **字段 = `name`。对存在的实体它会不会合法为空？** 两条依据：
  1. **schema**：`crates/store/src/migrations/0005_graph.sql:9` `name       TEXT NOT NULL,`（该文件 sha256 `505F0F81A4E599CD…`，mtime `2026-09-29`）⇒ 行存在 ⇒ `name` **不可能是 NULL** ⇒ `null` 只能来自"没有行"。
  2. **写入侧实测**：daemon **接受空串名字**（`POST {"name":""}` 成功，id 2），wire 回 `"name":""` ⇒ **空串是"存在"**，且消费者按**类型**判定（`:1228-1240`：`null`⇒不存在、`is_string`⇒存在、其他/缺键⇒错误）⇒ 空串**不会**被读成不存在（我实测 `entity #2 has no current facts` ✓；作者的同轴用例在 **`crates/mcp/src/lib.rs:1806`** `assert_eq!(entity_exists(&present_empty_name), Ok(true))`）。
- 另一个"对存在的实体也可能为空"的键是 **`kind`**（`:1362` 用 `and_then`，行存在时可以是 `null`）：消费者**没有**拿它判存在（只读 `name`）✓ 不构成风险，但值得写下来：**将来的消费者不许用 `kind`/`aliases`/`facts` 判存在**。
- ⇒ **本条不构成 finding**（没有字段能同时"存在且为空到被读成不存在"）。**falsifiable 的反向检查**（我做的）：空串 name 的实体经真 daemon + 真 MCP 读出来是"存在"（§2.4a 最后一行）。

## 3 与作者读数的差异（逐条，不作调和）

**3.1 关于"钉住信号"的那条，我做了额外的对手检查并否证了一个怀疑。** 我先怀疑：`:6784` 的 `assert!(missing["name"].is_null())` 在 serde_json 里对**缺键**也成立（`Value` 的 `Index` 对缺失键返回 `Null`）⇒ 光看这一条，把 `name` 键删掉它仍然绿。**但这个怀疑被同一测试的 `:6738`（`assert_eq!(v["name"], "AMBIGUOUS")`）否掉了**：缺键时 `v["name"]` 是 `Null ≠ "AMBIGUOUS"` ⇒ 键的存在与取值都被钉住。结论：**不是 finding**（这正是"看起来能过的判据要从两侧查"的一次实践）。

**3.2 `isError` 的措辞 vs wire 的事实**（见 §0-1）：作者的"客户端 `isError=true`"在 **stdio wire 上**表现为 **JSON-RPC error `-32603`**；他的 helper 把两者统一记作"拒绝"，所以他的断言（`expect_err`）与我看到的协议形状一致。**记录差异，不判缺陷**。

**3.3 G2 门禁的红**（见 §0-2）：作者 `exit 0 / 156 passed`；我 `exit 101 / 156 passed + 1 failed`，红在 `crates/daemon/src/runs.rs:2148` 的 `runs::tests::the_injection_budget_the_event_carries_is_measured_not_all_zero`，**该文件在我的门禁窗口内被同伴修改**（` M crates/daemon/src/runs.rs`，mtime `00:34:25`）。**t118 的钉住用例在同一次运行 `ok`** ⇒ 归因同伴在途编辑（按纪律：我给文件与错，不替代他修，也不把它算到 t118 头上）。我的读数取自：G2 日志 `%TEMP%\t119-g2.log`（187 行）+ 当次 `git status --porcelain`。

**3.4 共享 exe 的"年份"**（见 §0-3）：我探到的它不是 pre-t118；**"预编译 exe 陈旧"这条警告在本次读数里没有被用来取证**（我用的是自建二进制），但我把它当成纪律实例记下：任何"最近才加的字段"都能当年份探针——我用的是 t118 的新消息字符串。

## 4 门禁（在**最终字节**上由我本人重跑，给计数）

**字节归属（每个读数都合法挂在这一份上）**：`crates/mcp/src/lib.rs` sha256 **`2BF35EEC14DB72D0…`**（作者披露的最终版；且它**就是提交 `ec24b61` 的内容**，`git show --numstat --format='' ec24b61 -- crates/mcp` 逐字给出 `222\t13\tcrates/mcp/src/lib.rs`），`crates/mcp/tests/roundtrip.rs` sha256 `E902BC9D234CECC5…`（= HEAD 内容，未被本次改动污染）。我的二进制构建于 `00:32:08`，读数取自 `00:32:50–00:35:29`。作者报告里的 §6/三窗口读数属于更早的 `52E9F00A…`，**我没有复用它们**。

| # | 命令 | 退出码 | 计数/原文 |
| --- | --- | --- | --- |
| G1 | `scripts/cargo-team.ps1 test -p ruagent-mcp` | **0** | 日志 **93 行**；4 个 target：lib **46 passed / 0 failed / 0 ignored**、`roundtrip` **5 passed / 0 failed**、`tool_surface` **3 passed / 0 failed**、doc 0 |
| G2 | `scripts/cargo-team.ps1 test -p ruagent-daemon --lib` | **101** | 日志 **187 行**；`156 passed; 1 failed`，唯一红 = `runs::tests::the_injection_budget_the_event_carries_is_measured_not_all_zero`（`crates\daemon\src\runs.rs:2148`，`the drop must also be VISIBLE in the bytes: "<knowledge>…</knowledge>\n"`）⇒ **同伴在途编辑**（§3.3）；**t118 的钉住用例 `... the_list_opt_in_is_additive ... ok`** |
| G3 | `scripts/cargo-team.ps1 clippy -p ruagent-mcp --all-targets -DenyWarnings` | **0** | 日志 **12 行**；`Checking ruagent-mcp` → `Finished … 5.17s`；**`warning:` 行 0** |
| G4 | `scripts/cargo-team.ps1 fmt --all --check` | **0** | 日志 3 行（`cargo fmt --all --check` 原文） |

## 5 查过、**不是**缺陷（含一条 low finding）

1. **不是缺陷**：`facts` 为 `[]` 在两方向都相同 ⇒ 分不出它们的是 `name`，不是 `facts`；消费者的 `facts_of`（`:1175`）对**缺/错类型**的 `facts` 报错（t93 另一半），与 `entity_exists` 同一条纪律 ✓。
2. **不是缺陷**：`entity_exists` 的四个分支（`null`/`is_string`/其他类型/缺键）语义互斥且都可达 ⇒ 没有死分支（作者声称"两个死分支已删除"，我在当前字节上 grep `entity`/`exists` 的键读取**零命中**、只有注释里的历史叙述）✓。
3. **不是缺陷**：daemon 侧钉住是**一对**断言（`:6738` 值 + `:6784` null，§3.1）。
4. **不是缺陷（我实测的反例轴）**：空串名字的实体读作"存在"（wire + 工具出口 + `:1806` 用例）✓。
5. **L1（low finding）**：**`entities.name NOT NULL` 没有任何测试钉住**。依据：schema `crates/store/src/migrations/0005_graph.sql:9`；我扫了 `crates/graph/src`、`crates/graph/tests`、`crates/store/src`、`crates/store/tests`、`crates/daemon/src`、`crates/daemon/tests`，关于 NOT NULL/约束的测试命中都是 **FOREIGN KEY**（`crates/graph/src/lib.rs:186` 的文档、`crates/graph/tests/delete-merged-entity.rs:7/14/103`），**没有一条**断言"插入 NULL name 必须失败"；本仓**已有**做这件事的家什，但**极性相反**：`crates/store/src/migrations.rs:637-661` 用 `PRAGMA table_info` 逐个断言若干**新增列** `notnull == 0`（必须可空），`entities.name` 不在其列。**为什么重要**：整个存在信号建立在"行在 ⇒ `name` 非 NULL"之上；一次表重建（本仓已有 0022/0024 那种 DROP+RENAME 重建的先例）若丢掉 NOT NULL，一行 `name = NULL` 的**存在的实体**会被读成"不在图里"，而**门禁全绿**。**可证伪的修复判据**：把 `("entities","name")` 加进一个断言 `notnull == 1` 的测试（照 `:637-661` 的形状写，极性反过来），或直接断言"插入 NULL name 必须报错"。坐标/字节：`0005_graph.sql` `505F0F81A4E599CD…`、`crates/store/src/migrations.rs`（读于 2026-10-04）。
6. **不是缺陷**：`kind` 对存在的实体可以是 `null`（`:1362` `and_then`），消费者没有用它判存在 ✓。

## 6 不覆盖什么（第 19 条）

1. **我没做源码变异负控**（"把 `entity_exists` 改回旧读法"）：`crates/mcp/src` 在本单 inScope 之外，写它本身就是违规；我用"构造旧形状响应 + 引旧判读原文"的等价路径（§2.4b/c）。**作者的三窗口负控读数我没有复现、也不采信为我的读数**——它们属于 `52E9F00A…` 那版字节。
2. **pre-t118 二进制的可执行 A/B 没做**：那需要 `git worktree add <pre-t118> ` + **自带 `CARGO_TARGET_DIR`** 的冷构建（AGENTS 记录约 9 分钟、十几 GB），而 captain 已把该负控列为可选。若要做，判据是：**同一台 daemon、同一份 wire 原文**，旧二进制对方向一必须回 `has no current facts`（假成功），新二进制必须拒绝。
3. **`panel` 消费面**：面板是否也读这个信号、有没有同样的"恒存在"判定，**没有测**（不在我的 inScope）。
4. **真实语料**：我造的是空图 + 两个标量实体；**没有**在活库副本上取"真实实体 + 零事实"的读数（活库只读，且本单的两件事在 wire 层已被穷尽）。
5. **并发/锁**：`mcp-serve` 与 daemon 在同一连接上的多客户端并发没有测。
6. **`aliases`/`facts` 的语义**（t22/t18 的别名文本、facts 过滤）不在本单，只验"存在信号"。

## 7 我的仪器与残留

- 我起的进程：临时 daemon **pid 29368**（端口 8821，已停，`alive=False`）与 **pid 28508**（同一 root 复用一次，已停）；python 的 stub（**8822**）在进程内、随脚本退出；**没有**起停 8787 的常驻 daemon（今天是 pid 14944，非我）。
- 我的临时文件：`%TEMP%\t119\ruagent-new.exe`（328 MB，构建产物）、`%TEMP%\t119\root`（我的临时 root，含实体 1/2）、`%TEMP%\t119-*.py/.ps1/.log`。**收尾时删除构建产物与 root**（保留日志作为证据），残留读数写进任务 output。
- 活库 `~/.ruagent`：**未写**（本单所有 HTTP 请求都打在我的 8821 上）。
