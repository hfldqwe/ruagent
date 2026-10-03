# gen2 集成契约 · 第二轮独立评审（t61 / RV-INT round 2）

**对象**：INT（t19）是否达成 `gen2-integration-contract.md` 的六节交付；**被评任务 t60**（预算/丢块报告）与依赖结果里的 t52/t58/t62/t63。
**本轮我的独立读数**（全部在**当前字节**上、本回合跑出；命令与行号逐条在下）：
* `test --workspace`（`scripts/cargo-team.ps1 test --workspace`，`NO_PROXY=127.0.0.1,localhost,::1`）⇒ **exit 101**，单点失败 `-p ruagent-mcp --lib` / `entity_existence_is_read_from_name_and_drift_is_not_absorbed` / `crates/mcp/src/lib.rs:1684`（`assertion left == right failed`）。
* **同一条测试单跑（9.6s 后）⇒ `1 passed; 0 failed`，exit 0** ⇒ 与本回合 workspace 红**不一致**；`crates/mcp/src/lib.rs` mtime `2026-10-04 00:16:08` 晚于最后一次提交（`94aed37` Oct 1）⇒ **该文件在被在途编辑**。按 §6.0 第 15/19 条与 AGENTS.md 的共享树纪律：**只报文件+错误、不归因、不选边**（见 §4）。
* `chat.rs:668`、`runs.rs:2060` 各有 `items.extend(crate::memembed::graph_evidence_items(` ⇒ **图块生产者在树上**（t63 的接线成立）。
* `runs.rs:1387`、`chat.rs:505` 仍是 `budget: None` ⇒ **事件侧没接线**。
* `crates/memory/src/inject.rs:626 render_context` / `:646 BlockReport` / `:673 BudgetReport` / `:687 lost_anything` / `:711 render_context_report` ⇒ t60 的渲染侧报告**在树上**（但 `render_context` 只回 `String`，`budget` 进不了事件）。
* `api.rs` 里 `prompt_hash` **0 命中**（t62 的 F3 结论方向一致）。
* 面板读数：`panel/src/views/Memory.tsx:37` = `observation: ["user", "global"]`；`:82` = `…api.recallLog>>["log"]`。
* t52 的单一来源：`crates/memory/src/lib.rs:99 allowed_kinds` / `:124 allows_namespace` / `:141 write_vocabulary`；`write.rs:68` 渲染拒绝串。
* t58 的契约形状：`inject.rs:349 GRAPH_PATHS: usize = 3` / `:370 graph_items`。

---

## 1 契约六节逐条判决（判据**逐条对齐契约**，不引入契约外要求）

| 节 | 判决 | 依据（读数） |
| --- | --- | --- |
| **§1 HTTP 面** | **达成** | `/recall/log` 读出逐列透出（24 列）+ 键名 `"log"` 与两个消费方一致（`api.rs` 的 `"log": rows`、`knowledge_api.rs` 的 `log["log"]`、`Memory.tsx:82`）—— 我 t80/t62 读数 + 本回合的面板行；`facts?at=` 已归一化（`api.rs` 的 `parse_ts`）；MCP 14 工具闭集（t80）。**注意**：键名 `rows` 与实现的 `log` **不一致这件事**属 §1.2 响应 schema 与实现之争，t83 已承接（按契约改 `rows` 需三处同改）；本轮**不重复计缺陷**。 |
| **§2 MCP 面** | **达成** | 14 个工具闭集、逐一对应（t80 的静态读数）；本轮无新反证。 |
| **§3 遥测面（含 §3.2 字段表）** | **未达成** | **本回合直接读数**：`runs.rs:1387` 与 `chat.rs:505` 仍是 `budget: None` ⇒ **A-2 的预算半边与 N-6 依然不可判**（t60 亦自述「未达成 NOT not_measured」）。t60 的落地（`inject.rs:711 render_context_report` + 逐字段 serde 断言）**只解决渲染侧**，事件侧按契约要求必须携带 `budget` 对象。契约 §3.2 L198「`null` = 本次未采集」与 §6.1/§6.2「要求必填」**互相矛盾**，需裁（见 §3 INT-R2-5）。 |
| **§4 面板面** | **未达成（一处）** | **本回合面板读数**：`Memory.tsx:37` 的 `NAMESPACES.observation = ["user","global"]` 与已实现的写词表不一致（t52 实测 `observation×global` ⇒ `RejectedNamespace`） ⇒ 用户在写入对话框里被允许选一个**必然被拒**的组合，而拒绝串只说「不支持该组合」。t52 的 R-3 已登记而**未修**（面板归 panel 写权）。其余：`Memory.tsx:82` 的 `["log"]` 与实现一致 ✓。 |
| **§5 版本读点** | **未达成** | `api.rs` 里 `prompt_hash` **0 命中**（本回合）⇒ `/distill` 拿不到生成侧版本；`/stats` 侧 t62 读数 = `{"agents":[]}` ⇒ `embedder`/`scoring_version` 都没有。t62 是**只读留档**，未落地。 |
| **§6 验收锚点** | **部分达成 / 一项未收口** | ① **`<graph>` 块（G10）**：**实现已落地**（本回合 `chat.rs:668`/`runs.rs:2060` 两个调用点；`inject.rs:349/370` 的契约形状）且 t63 给出盘上读数 `with_graph=3/5`（其负控：无图证据 ⇒ 不出现块、也不出现空占位符）⇒ 从「不可达」变为**有盘上读数**；② **`GRAPH_PATHS=3` 的充分性**：t63 自述其块内行匹配仪器返回 **0 匹配**、并明确「最优性未被本代任何读数支持」⇒ **只能记「值未受支持」，既不算达标也不许写成 not_measured**；③ A-2/N-6：未达成（同 §3）。 |

---

## 2 findings（round 2）

### INT-R2-1（high）§3.2 `budget` 事件侧仍是 `None` ⇒ A-2 预算半边 + N-6 未达成
* **契约位置**：§3.2 字段表（`gen2-integration-contract.md:198-204`）· §6.1 **A-2** · §6.2 **N-6**。
* **当前读数**：`crates/daemon/src/runs.rs:1387 budget: None,`、`crates/daemon/src/chat.rs:505 budget: None,`（本回合 grep）。渲染侧已具备 `render_context_report`（`crates/memory/src/inject.rs:711`）与 `BudgetReport`（`:673`）。
* **为什么重要**：没有它，§3.2 的 `used_chars ≤ total_chars` / `truncated_blocks` / `dropped_items` / `blocks[]` 四条判据在**任何事件上都不可判**；t60 报告里最有价值的一条（「文本分不开『没输入』与『全被丢』，账本分得开」）目前**只活在渲染函数的返回值里**，没有出口。
* **可证伪修复判据**：`runs.rs:1270-1279` 一带让 `render_run_injection` 回 `(String, BudgetReport)` 且事件处写 `budget: Some(serde_json::to_value(&report)?)`；`acp/chat.rs:496-505` 同形。**修完一条读数**：从一个「有丢块」的运行里取 `context_injected.budget.dropped_items > 0`，且同一事件的 `budget.used_chars ≤ budget.total_chars`。**不得改成必填**（`event_compat.rs:36` 正断言旧事件读出 `None`）。
* **owner**：持 `crates/daemon/src/chat.rs` / `runs.rs` 写权的下一步入口（t60 已交回行级修法）。

### INT-R2-2（medium）§4 面板仍声明 `observation` 接受 `global`（拒绝必然是运行期才发现）
* **契约位置**：§4（面板）与 §1.4/§1.5 的写词表声明（t52 已把**实现**收敛到单一来源）。
* **当前读数**：`panel/src/views/Memory.tsx:37` = `observation: ["user", "global"],`（本回合）；t52 的实测：`observation×global` ⇒ `{"outcome":"RejectedNamespace"}`。
* **为什么重要**：这正是 t52 命名的那种「**声称的词表 vs 实际的词表**」缺陷 —— 代码侧已经一个来源，**用户可见的一侧还错着**。
* **可证伪修复判据**：把该矩阵改为由后端/契约生成的 8 格（或手写与 `MemoryStore::allowed_kinds()` 逐格一致），并加一条面板侧断言/快照；**读数**：在面板里选 `observation` 时下拉**不再出现** `global`。
* **owner**：panel（`Memory.tsx`）+ t52 的 R-3。

### INT-R2-3（medium）§5 两个版本读点仍未落地
* **契约位置**：§5（`prompt_hash` 接 `/distill`；`/stats` 的 `embedder`/`scoring_version`）。
* **当前读数**：`prompt_hash` 在 `crates/daemon/src/api.rs` **0 命中**（本回合）；t62：`/distill` GET keys 无 `prompt_hash`，`/stats` = `{"agents":[]}`。
* **可证伪修复判据**：`GET /api/v1/distill` 的响应里出现 `prompt_hash`（与 `distill_log` 里落的那一列**同源**，不许两个来源）；`GET /api/v1/stats` 里出现 `embedder` 与 `scoring_version` 且与索引/召回侧实际使用的一致。
* **owner**：daemon API（`/distill` 与 `stats` 处理器）。

### INT-R2-4（medium）§6 `GRAPH_PATHS=3` 的充分性无读数 —— 不得升级为达标，也不得写成 not_measured
* **契约位置**：§6.3 第 2 行（`<graph>` 的路径条数判据）与 graph 规格的 G10。
* **当前读数**：`inject.rs:349 GRAPH_PATHS: usize = 3`；t63 的盘上事件 `with_graph=3/5` + 干净负控；t63 自述**块内行匹配仪器 0 匹配**（仪器缺陷，不是「0 条证据」）⇒ **充分性无读数**。
* **可证伪修复判据**：给出「块内实际路径条数 vs `edges`/`retrieval` 候选数」的读数（例如 10 条候选里发了 3 条、被丢 7 条的原因），或直接测「把 3 改成 4/6 是否让块内证据更完整」。
* **owner**：graph + mem-core（t63 已给下一步读数形状）。

### INT-R2-5（low，登记）契约 §3.2 L198 与 §6.1/§6.2 自相矛盾，需 captain 裁
* **位置**：`gen2-integration-contract.md:198`（「`null` = 本次未采集」）vs §6.1 A-2 / §6.2 N-6（要求预算读数）。
* **读数**：两者同时成立不可能（t60 亦指出与 t41 裁过的 R-B D4 vs D2(b) 同形）。
* **判据**：裁「必填」则 §3.2 的 `null` 只允许出现在**旧事件**上（向后兼容），或裁「可选」则 A-2/N-6 改为「若采集则必须自洽」。**owner**：captain + 契约属主。

### INT-R2-6（low，登记）`blocks[]` 只列**发出**的块 ⇒ §3.3 Q2「丢的是哪个 tag」在冻结字段里答不了
* **读数**：t60 的诚实限制（保留 `⊇` 语义会使 A-2 的 tag 集合判据失败；加 `dropped_tags` 则改冻结形状）。
* **判据**：二选一并写进契约：加 `dropped_tags`（新字段、向后兼容）**或**把 §3.3 Q2 明确为「只保证 ⊇」。**owner**：captain。

---

## 3 未裁定 / 归因边界（**不选边、不编根因**）

1. **t20 的 wiki `5/5` vs t63 的 `with_wiki=0` 未裁定**。两个读数的**载体不同**：t20 取自**真实 root 的 transcript**，t63 取自**新建临时 root**（自身注明「临时 root、0 残留」）。代码侧线索：`WIKI_PAGES` 在 `chat.rs:656`/`runs.rs:2052` 被选择、`memembed.rs:1055` 的合成调用**位于 ```ignore 文档块内**（是文档，不是执行路径）。⇒ 我**无法**在只读范围内判定「临时 root 没有 wiki 页（载体差异）」还是「wiki 块已不在生产路径上（回归）」；按 §6.0 第 14 条记**未裁定**，owner：wiki + integ（证伪判据：在**同一 root** 上各取一次 `with_wiki`，或给一个已知有 wiki 页的临时 root 的正例）。
2. **本回合 `test --workspace` 的红不可复现、不归因**：失败点 `crates/mcp/src/lib.rs:1684`（`entity_existence_is_read_from_name_and_drift_is_not_absorbed`，`assertion left == right failed`）在 **9.6 秒后的单跑里 1 passed / exit 0**，且该文件 mtime `2026-10-04 00:16:08` 晚于最后一次提交 ⇒ 共享树在途编辑/控制窗口的典型形状（AGENTS.md 的 t93/t81 条目正记了这一类）。⇒ **t63 的「`test --workspace` exit 0」在本回合无法确认**（不是被推翻，是**不可复现**）；我在只读范围内**没有**做隔离 worktree 基线（不在本单 inScope），故**不判定它是缺陷**，只把文件+错误+两次数交回。owner：`crates/mcp` 的属主。
3. **`test --workspace` 与 §6 的关系**：t63 的门禁读数取自**去掉 `&` 之前**的字节并已如实标注未重跑；本回合我重跑时遇到上述在途编辑 ⇒ 该读数**仍未在最终字节上取得**（第 22 条形态）。

---

## 4 不覆盖什么（§6.0 第 19 条）

* 本单**只读**：未改任何源码/契约/他人报告；未写活库；**未启停真守护进程**（pid 79984 未触碰；`GET :8787/api/v1/health` 由 t62 记过「ok 但 pid 已换」）。活库**没有**被读取（`budget` 的活库分布类读数只能由会用 `?mode=ro` 的探针或临时 root 提供）。
* **没有**取得 `--check`/面板 e2e 读数；面板只做**静态行级**读数（`Memory.tsx:37/:82`），未驱动 UI。
* 六节判决**逐条对齐契约**；契约自相矛盾处（§3.2 vs §6.1/§6.2）**登记为待裁**，不自行改判据。
* 我**不是** INT 的作者（第二轮评审员；第一轮是 `gen2-integration-review.md`）。

---

## 5 verdict

**needs_revision**（任务判 **failed**）。理由：§3（budget 事件侧）、§4（面板词表）、§5（两个版本读点）三处**在最终字节上未达成**，§6 的 `GRAPH_PATHS` 充分性未收口，且 wiki 归属未裁定；这些都不是本评审新加的要求，而是契约 §3.2/§4/§5/§6.1/§6.2/§6.3 自己写明的判据。§1/§2 达成，§6 的图块落地部分（t63）**已达「有盘上读数 + 干净负控」**，应记为部分达成。
