# t120 · 评审 t118「graph_entity 存在性可分」— 独立评审（按字节与读数，不采信转述）

**评审对象**：t118 的 6 条 acceptance（从团队状态逐字取出，见 §1）+ t119 的验证报告 + **最终字节**。
**我读的字节**：`crates/mcp/src/lib.rs` = **125,493 B / sha256 `2BF35EEC14DB72D0…` / mtime 2026-10-04 00:28:08**（与 t118 披露的最终哈希同前缀）；`crates/mcp/tests/roundtrip.rs` sha256 `E902BC9D234CECC5…`。
**我的写入集合**：本报告（`docs/design/reviews/gen4-graph-entity-existence-review.md`）—— `crates/`、`panel/`、`scripts/` 一行未改。

---

## 0 我自己的读数（原文引用）

**① 真实 wire 响应（我亲自跑，活守护进程 `GET :8787`，只读、自编 id）**：
```
GET /api/v1/graph/entity/987654321  ->  {"name":null,"kind":null,"aliases":[],"facts":[]}
GET /api/v1/health                  ->  {"status":"ok","service":"ruagent","db":{"writer":"running","writer_panics":0,"last_panic":null}}
```
⇒ **存在信号 = `name`，`null` ⇔ 该 id 不在图里**，且**不是 404**（200）—— 与 t118/t119 的读数一致，且这是我自己的读数。

**② 工具出口层（我跑的门禁，逐行读断言）**：`scripts/cargo-team.ps1 test -p ruagent-mcp` ⇒ **exit 0**，四个 target：lib **46 passed** / roundtrip **5 passed** / tool_surface **3 passed** / doc 0 ⇒ 与 t118 §9.2 的最终字节读数**逐数字一致**（独立复现）。相关断言（当前字节）：
* `lib.rs:233-236`：`if !entity_exists(&resp).map_err(|e| rmcp::ErrorData::internal_error(e, None))? { return Err(format!("entity #{} not found: the graph has no entity with that id", …)) }`
* `lib.rs:245`：`return Ok(format!("entity #{} has no current facts", params.id));`
* `lib.rs:1747`：`if r.is_error == Some(true) { … }`；`:1773` 断言错误文本**不含** `"has no current facts"`；`:1786` 断言零事实方向**含**该文本且成功；`:1800-1815` 纯函数四例（present⇒Ok(true)、空串名⇒Ok(true)、absent⇒Ok(false)、改名/缺键⇒Err）。

**③ 判别力（我自己的读数，不是复述）**：把两方向用到的真 wire JSON 喂给**旧谓词**（`resp["entity"] ?? resp["exists"]`）与新谓词：
```
absent   oldPredicate= false  newPredicate= false
present  oldPredicate= false  newPredicate= true
```
⇒ 旧代码对**两个方向**都判 false ⇒ 两者都走「成功 + has no current facts」；**方向 1 的用例（期望 Err 且文本不含该短语）必红，方向 2 的用例（期望成功且含该短语）保持绿** ⇒ **只红一条**。这一点**读代码 + 这 3 行模拟即可判定**，不必实跑；t118 的实跑（`6 passed; 1 failed`，窗口 #1）是**旁证**，不是我的证据。

**④ 注释坐标逐条回读（当前字节）**：
| 注释里的引用 | 回读结果 |
| --- | --- |
| `api.rs:1358-1359`（不在图里保 200 不变 404） | **通过**（该两行正是这句话） |
| `api.rs:6711`（钉住用例） | **通过**（`async fn the_entity_route_exposes_the_alias_text_and_the_list_opt_in_is_additive`） |
| `api.rs:6783`（200） | **通过**（`assert_eq!(st, StatusCode::OK, "{raw}")`） |
| `api.rs:6784`（`name.is_null()`） | **通过**（`assert!(missing["name"].is_null(), "{raw}")`） |
| `api.rs:6785-6788`（facts 空） | **通过** |
| **`api.rs:1341-1346`（所谓四键响应字面量）** | **不通过**：那 6 行是 `/graph/search` 假阳性的散文；四键字面量在 **`api.rs:1360-1365`**（`json!({ "name": entity.as_ref().map(…) , "kind": …, "aliases": aliases, "facts": facts })`） |

**⑤ 写入集合（`git diff --name-only HEAD` 的两张载体）**：
```
git diff --name-only HEAD            -> crates/daemon/src/runs.rs
                                        docs/design/reviews/gen2-integration-contract.md
                                        panel/src/views/Memory.tsx          (都不是 t118 的路径)
git status --porcelain -- crates/mcp -> (空)  ⇒ mcp 侧无未提交改动
git log -1 -- crates/mcp/src/lib.rs  -> ec24b61 (2026-10-04 00:31) 「feat(memory,mcp,daemon,panel,ci): five units …」
   numstat: crates/mcp/src/lib.rs 222/13 · + crates/daemon/{api.rs,extract_plane.rs,tests/version_points.rs}
            · crates/extract/** · panel/** · .github/workflows/audit.yml · 6 份报告
```
⇒ **t118 的改动已在 `ec24b61` 里提交**，而那个提交**一次装进五个单元**；工作树里那三个 dirty 文件属于**别的在途任务**（`runs.rs` 的 budget 接线、契约、面板 NAMESPACE）。

---

## 1 逐条判决（t118 的 6 条 acceptance 原文）

| # | acceptance（逐字摘要） | 判决 | 依据（字节 / 读数） |
| --- | --- | --- | --- |
| 1 | 不存在的 id ⇒ 点名该 id 的错误（`isError=true`）；报告给出改前/改后原文读数对 | **达成** | 字节 `lib.rs:233-236`（点名 id 的 `Err`）+ `:1747/:1773`（红侧断言）；我自己的 wire 读数 + 我跑的门禁（lib 46 passed）。改前原文由 t118 给出，且被我的探针 ③ 从**机制上**证实（旧谓词对两份 JSON 都是 false ⇒ 旧行为就是那条假成功） |
| 2 | 存在但零事实 ⇒ 仍「has no current facts」且不是错误；给出构造方式与原文读数 | **达成** | `lib.rs:245` + `:1778-1786`（成功 + 文本包含）；构造方式（POST entity、零 fact）与我自己的 wire 无矛盾；两侧 `facts` 都是 `[]` ⇒ 唯一可分的是 `name` |
| 3 | 报告给出 daemon 真实发出的原始 JSON、逐字指出存在信号是哪个键的什么值；给出**回读核对过**的 daemon 侧测试坐标 | **达成** | 我亲自取得 `{"name":null,…}`（§0①）；信号 = `name`，`null` ⇔ 不在图里；测试坐标 `api.rs:6711/6783/6784/6785-6788` **逐条通过**（§0④）。**注**：该条只覆盖“测试坐标”；另一处**响应字面量**坐标的不通过计入第 5 条，不重复计 |
| 4 | 判存在是可单测纯函数；两方向各有能红用例；负控**只让“不存在”用例变红**且零事实用例保持绿；报告含控制窗口起止与恢复后绿读数 | **达成** | 纯函数 `lib.rs:1227`；红侧 `:1760-1773`、绿侧 `:1778-1786`；**判别力由我的探针 ③ 判定为「只红一条」**（旧谓词两方向皆 false ⇒ 方向 2 的期望恰好被旧代码满足）；控制窗口 `00:12:37→恢复 00:16:08→结束 00:16:33` + `6 passed; 1 failed` + 恢复后 `7 passed`、哈希一致 —— 符合本仓「控制窗口必须先宣告/限时/按字节恢复」的纪律 |
| 5 | `crates/mcp/src/lib.rs:225-231` 的注释不再声称「daemon carries no existence field」；指向从未存在过的键的死分支已删或在注释里说明为何保留；**不再依赖任何未经本单复核的行号** | **未达成** | 「不再声称」**成立**：现文本只说 `resp["entity"]`/`resp["exists"]` **两个键从未被发出**（这是**真**的），并明确 `name` 才是信号（`lib.rs:1205`、`:1222-1223`）；死分支**已删**（代码里 `entity"\] / exists"\]` 仅存在于注释行 `:230/:1221`）。**但不通过的是末句**：保留的行号引用 `api.rs:1341-1346` 在当前字节上**不是**四键字面量（真实位置 `:1360-1365`）⇒ 一条已漂坐标仍在注释里。**缓解事实**（影响严重度，不影响判决）：同一注释在 `:1210-1213` 已写明「THESE NUMBERS DRIFT … Resolve by SYMBOL … each number is a hint with the date it was read」，且 `:1223` 同时给了符号（`name/kind/aliases/facts`） |
| 6 | 本单未改 `crates/daemon/**`（若必须改须先回报）；daemon 的既有 wire 形状与 api.rs 现有测试语义不变 | **达成（附载体限制）** | `git status --porcelain -- crates/mcp` 为空 + `roundtrip.rs` 与 HEAD 无差异（“整文件还原”成立，其 mtime 00:23:53 早于 lib.rs 00:28:08，与「先移动后还原」的叙述一致）；daemon 侧 wire 形状今天仍如声明（我自己的 `:8787` 读数）。**限制**：那个提交把五个单元装在一起，**单凭提交无法把 `crates/daemon/src/api.rs` 的 39/1 行归给谁**；我按各单元自己申报的路径（t62 申报 api.rs/version_points.rs、extract/panel/ci 各有其单元）判定这 39 行**不是 t118 的** ⇒ 不构成 blocker |

---

## 2 findings

### GEN4-EX-R1（low）注释里一条行号引用已漂：`api.rs:1341-1346` 不是四键响应字面量
* **位置**：`crates/mcp/src/lib.rs:1223`（注释）。
* **当前读数**：`api.rs:1341-1346` 是 `/graph/search` 假阳性统计的散文；四键字面量在 **`api.rs:1360-1365`**。
* **为什么重要**：本仓已多次因坐标漂移把下一位读者送到无关行（t80/t85 的整条工作线）；这条注释正是**教后来人如何解析存在信号**的地方，坐标错会让读者以为存在信号在别处。
* **可证伪修复判据**：把该数字改为 `api.rs:1360-1365`（或直接删数字、只留符号 `name/kind/aliases/facts` + 函数名 `entity_by_id`），然后**逐条回读**：`api.rs` 的 `:1358-1359`、`:1360-1365`、`:6711`、`:6783`、`:6784`、`:6785-6788` 各自都能在注释描述的对象上命中。
* **owner**：`crates/mcp` 的属主（t118 的同一入口）。

### GEN4-EX-R2（low，流程；不是 t118 的缺陷）提交把五个单元装在一起 ⇒ 写入集合无法用 `git diff --name-only HEAD` 审计
* **读数**：`ec24b61` 的 numstat 列了 19 个文件、跨 5 个单元；t118 的 acceptance 第 6 条与本单的“写入集合”判据都要求用工作树 diff 证明「只落在 crates/mcp/**」——**在改动已提交后这条检查不可用**（今天 `git diff --name-only HEAD` 的三个文件都属于别的在途任务）。
* **建议**：要么每单元一提交，要么在任务单里把判据写成「该单元申报的路径 == 其提交里的路径子集」。**owner**：captain（流程）。

---

## 3 判决结论（对主问题）

**今天，一个 agent 拿自己瞎编的实体 id 调 `graph_entity`，不会再拿到「实体存在、只是没有事实」的成功答复**：`entity_exists` 读的是 daemon **真实发出**的 `name` 键（`null` ⇒ 不存在），不存在走 `Err`（点名该 id），存在但零事实保持成功文本。**不存在有独立读数**（我自己的 wire JSON + 我跑的 46 个 lib 用例 + 我自己的判别力探针）。

**verdict = needs_revision**（任务判 **failed**）：6 条 acceptance 中 **5 条达成**，第 5 条因**一条已漂行号**未达成 —— 修复是**一处坐标更新或删除数字**（建议同时保留符号），约两分钟；除此外本轮没有发现阻断性问题。（规则：评审判据必须按字节，未达成的条目不能放行；captain 若裁定“带符号的日期化提示已足够”可自行关闭，但那是裁决，不是我的判决。）

---

## 4 不覆盖什么（第 19 条）

* **我没有**自己启动真守护进程跑一次 MCP 客户端：工具出口层的证据是**我读的断言 + 我跑的 `test -p ruagent-mcp`**，wire 层的证据是**我亲自打的活守护进程 `:8787`**；两层各自的载体在 §0 写明（不把测试层的绿当成真 daemon 路由层的绿；后者的独立证据在 t119）。
* **我没有**做隔离 worktree 基线，也没有重跑 t118 的三次控制窗口（控制窗口的读数我采信为**旁证**并标明；判别力由我自己的探针判定）。
* **我没有**审计 t118 之外四个单元（extract/panel/ci/version-points）的代码；只读了它们的**提交归属**。
* 未跑 `--check`/面板 e2e/活库查询；`entities.name NOT NULL` 的钉住缺口（t119 的 L1）我**复核了其存在**（本报告不重复记账，归 t119 的 finding）。
