# 公开 FTS 面不再吃原始用户文本：净化收进共享 query 层（t73 / F3）（mem-core）

- 任务：t73（kind=repair round 1）；attempt `8a5cdc7b-c7b0-4955-b3ef-289c4ac66eeb`
- inScope：`crates/memory/src/query.rs` · 本报告；**未改** `crates/daemon`（`api.rs` / `memembed.rs`）、`crates/mcp`、`crates/store`、`crates/graph`、`crates/knowledge`
- 现场（端点读数）：临时 root `%TEMP%\ruagent-t73e2e` / `…-before` + 临时端口 **18793**（after）/ **18794**（before）；自起 PID **36180** / **10356**，两个都已停（`still_running=False`，0 残留）；私有 target 目录用后已删
- 上游：t69 审计 F3（high）；本单是它的 triage ⇒ repair

## 0 修法一句话

「怎么把用户文本变成合法 FTS5 查询」这条规则原本只住在 `crates/daemon/src/memembed.rs:200`（`keyword_pattern`，**只有召回那条路径知道**）。本单把它**搬进 `crates/memory/src/query.rs`**，成为公开函数 `fts_pattern`，并由 `search_fts` / `search_fts_scored` **在自己内部**调用 ⇒ 调用点不需要知道规则，也不可能忘（`api.rs` 一行未改）。

## 1 F3 修复：逐条读数（t69 的 C9 十一条输入）

**载体**：`Db::open_in_memory()` + 一行含 `deploy` 的记忆。`before` 列**不是引用审计文字**——测试里逐字复现了旧代码路径（把原始串直接交给 `MATCH`，helper `raw_match`），所以 8 个错误是本轮现场复现的：

| # | 输入 | before（raw → MATCH，旧路径复现） | after（共享 `fts_pattern`） |
| --- | --- | --- | --- |
| 1 | `deploy` | ok rows=1 | ok rows=1 |
| 2 | `"` | **ERR** `sqlite error: unterminated string` | ok rows=0 |
| 3 | `AND` | **ERR** `fts5: syntax error near "AND"` | ok rows=0 |
| 4 | `NEAR(` | **ERR** `fts5: syntax error near ""` | ok rows=0 |
| 5 | `*` | **ERR** `unknown special query: ` | ok rows=0 |
| 6 | `-` | **ERR** `fts5: syntax error near ""` | ok rows=0 |
| 7 | `deploy OR` | **ERR** `fts5: syntax error near ""` | ok rows=0 |
| 8 | `NOT` | **ERR** `fts5: syntax error near "NOT"` | ok rows=0 |
| 9 | `a"b` | **ERR** `sqlite error: unterminated string` | ok rows=0 |
| 10 | `café` | ok rows=0 | ok rows=0 |
| 11 | `🦀` | ok rows=0 | ok rows=0 |

```
READING t73 F3 totals: before(raw) errors=8/11 | after ok=11/11
```

测试里是**断言**：`assert_eq!(raw_errors, 8)` + `assert_eq!(after_ok, 11)`（`no_user_text_reaches_match_unquoted`）⇒ 与 t69 C9 的 `8/11` / `11/11` 逐条对齐，连错误文案都一致。

## 2 负控：正常检索的结果集没有被改掉

同一载体、`before`（raw）与 `after` 都测，比的是**行 id 集合**（测试 `legitimate_queries_return_exactly_the_same_rows`）：

| 查询 | before（raw） | after | 结论 |
| --- | --- | --- | --- |
| 普通关键词 `deploy` | `[1, 2]` | `[1, 2]` | **一致** |
| CJK `部署` | `[3]` | `[3]` | **一致** |
| 用户合法引号短语 `"deploy script"` | `[1]` | `[1]` | **一致** |
| 重音 `café` | `[4]` | `[4]` | **一致** |

### 唯一的语义差异（写出来，不藏）：多词查询从「隐式 AND」变成「短语」

```
READING t73 difference "deploy script" -> before(raw AND)=[1, 2] after(phrase)=[1] (adjacent=1 separate=2)
```

`1` = 「deploy script lives in scripts/release.sh」（两词相邻）· `2` = 「deploy notes, then later a shell script」（两词不相邻）。

- **为什么接受**：① 契约要求「复用/上移 `memembed.rs:200` 那条已被读数证明充分的规则」，那条规则就是「整串当短语」；② **召回的 keyword 腿一直就是这个语义**（`memembed.rs:573` 一直传 `keyword_pattern(query)`）⇒ 这是**把两条路径统一到同一条规则**，不是把召回改成检索、也不是反过来；③ 短语语义更可预测（不会因为文档里碰巧同时出现两个词就命中）；④ 唯一能同时保住「隐式 AND」和「合法引号短语」的替代方案是**逐词加引号**，实测它会把用户输入的 `"deploy script"` 按空白拆成 `"deploy` / `script"` 再各自加引号，从而**破坏**「带引号但合法的查询」——正是本单点名必须保住的负控 ⇒ 不取。
- **没有任何本仓测试依赖多词 FTS 查询**（Rust 侧调用全是单词：`lifecycle.rs:960/1007` 的 `"kettle"`、`query.rs:352` 的 `"deploy"`、`e2e_daemon.rs:1036` 的 `q=concise`）⇒ 短语化不会踩到在途断言；panel 用户可输入多词，语义变化在此登记。

### 幂等性（必需，不是顺手加的）

```
READING t73 idempotence "deploy" -> "\"deploy\"" -> "\"deploy\""   （11 条同形，逐条打印）
assert_eq!(fts_pattern("\"deploy\""), "\"deploy\"");            // 召回路径已建好的短语原样通过
assert_eq!(fts_pattern("\"deploy script\""), "\"deploy script\"");
assert_eq!(fts_pattern("a\"b"), "\"a\"\"b\"");                  // 只是「看着像引号」的原始文本仍被转义
```

**为什么必需**：`memembed.rs:573` 传进来的**已经是短语**，而 `memembed.rs` 不在本单 inScope。若共享层再转义一次，召回的 keyword 腿会变成「搜索字面引号字符」⇒ **静默 0 命中**（t52/t41 同族的静默失真）。幂等让这件事在结构上不可能，并有过测试——而不是靠「记得别改」。

## 3 调用点：`api.rs:4366` 无需改动

- 旧：`search_fts(state.mgr.db(), &q.q, …)` 把 `q.q` **原始**交给 `MATCH`（F3 的缺陷面）。
- 新：`search_fts` **内部**先 `fts_pattern` ⇒ 调用点**一行未改、也不知道规则**，这正是「谁懂规则只剩一处」的形状。
- 代码级依据：`api.rs:4366` 是 `search_fts(…).await?`，行为唯一来源在 memory 层；端点侧没有第二份净化（它**有第三份**：见 §6 R-6，但那是另一条路径 `fts_related`）。
- 端到端读数见 §4。

## 4 审计给的缺口：端点 HTTP 状态码 —— 本单取到了，**而且是 before/after 两半都测**

t69 明确写过「HTTP 状态码此前没测」。本轮用两台**真 daemon**、两个临时 root/端口取到完整对照：

- **after**：本单的私有构建（`%TEMP%\ruagent-t73-target\debug\ruagent.exe`，mtime 2026-09-29 04:12）⇒ `%TEMP%\ruagent-t73e2e` + 端口 18793，自起 **PID 36180** 已停。
- **before**：共享 target 里**改动之前**的构建（`%TEMP%\ruagent-team-target\debug\ruagent.exe`，mtime 2026-09-29 03:15）⇒ `%TEMP%\ruagent-t73e2e-before` + 端口 18794，自起 **PID 10356** 已停。它跑的是**旧代码**，所以这是真正的「改前」现场，不是推理。

两台都用同一个探针、同一条播种（`POST /memory/write` ⇒ `200 {"outcome":"Inserted(1)"}`，内容含 `deploy script … release.sh`），再逐条 `GET /api/v1/memory/search?q=<原始文本>`（URL 编码）：

| 输入 | **before HTTP**（旧二进制） | **after HTTP**（本单） |
| --- | --- | --- |
| `deploy` | 200（命中 id=1） | 200（命中 id=1） |
| `"` | **500** `sqlite error: unterminated string` | **200** `{"hits":[]}` |
| `AND` | **500** `sqlite error: fts5: syntax error near "AND"` | **200** `{"hits":[]}` |
| `NEAR(` | **500** `sqlite error: fts5: syntax error near ""` | **200** `{"hits":[]}` |
| `*` | **500** `sqlite error: unknown special query: ` | **200** `{"hits":[]}` |
| `-` | **500** `sqlite error: fts5: syntax error near ""` | **200** `{"hits":[]}` |
| `deploy OR` | **500** `sqlite error: fts5: syntax error near ""` | **200** `{"hits":[]}` |
| `NOT` | **500** `sqlite error: fts5: syntax error near "NOT"` | **200** `{"hits":[]}` |
| `a"b` | **500** `sqlite error: unterminated string` | **200** `{"hits":[]}` |
| `café` | 200 `{"hits":[]}` | 200 `{"hits":[]}` |
| `🦀` | 200 `{"hits":[]}` | 200 `{"hits":[]}` |
| **`release.sh`**（负控/真实查询） | **500** `sqlite error: fts5: syntax error near "."` | **200**（命中 id=1） |
| `"deploy script"`（负控） | 200（命中 id=1） | 200（命中 id=1） |

⇒ ① 8/11 特殊语法输入从 **500 变 200**；② 「同一个坏输入经该端点不再报错」有 HTTP 读数；③ 额外发现：**带点的文件名 `release.sh` 在旧端点上是 500**（`.` 是 FTS5 语法字符）——这是最常见的一类真实查询之一，修复顺带解决；④ 负控里 `deploy` / `release.sh` / `"deploy script"` 三个真实查询都仍然命中同一行。

## 5 门禁（第 22 条形态）

| 面 | 命令（契约原文） | 读数 |
| --- | --- | --- |
| 测试面 | `scripts/cargo-team.ps1 test -p ruagent-memory` | **exit=0**；`test result:` 行 **3** 行全 `ok`（`74 passed; 0 failed` 库内 — 本单 +3 条新测试 · `9 passed; 0 failed` 集成 · `0 passed` doctest）；大小写敏感 `FAILED.` = **0**；`panicked` = **0**。**只覆盖 memory**（不含 daemon/acp/panel）。 |
| 门禁 | `scripts/cargo-team.ps1 clippy -p ruagent-memory --all-targets -DenyWarnings -CleanFirst`（契约里的**裸** `-CleanFirst`） | **exit=0**，**两件证据** = `[cargo-team] clean -p ruagent-memory (forced re-check)` + `Checking ruagent-memory v0.1.0`。⇒ 顺带证明 captain 对 R-5 的源头修复（裸 `-CleanFirst` 从 `-p <spec>` 兜底）**有效**。 |
| 编译面（附加，非本单 verify） | `scripts/cargo-team.ps1 build -p ruagent -TargetDir %TEMP%\ruagent-t73-target` | exit=0，`Finished dev profile in 10m47s`（私有 target，含 `Compiling ruagent-memory` `ruagent-daemon` `ruagent`）⇒ §4 的二进制确实包含本单改动 |

**格式**（t66）：只对**本单改动的文件**跑 `rustfmt --edition 2024 crates/memory/src/query.rs`，没有 `cargo fmt --all`（不改别人文件）。

## 6 路由（本单不改）

| # | 位置 | 发现（读数/代码） | 建议 |
| --- | --- | --- | --- |
| R-6 | `crates/daemon/src/api.rs:3638` | **这条规则的第三份拷贝**：`fts_related` 里内联 `format!("\"{}\"", term.replace('"', "\"\""))` —— 与 `fts_pattern` 逐字同规则，但没有名字、没人知道它也是「净化器」 | 改成 `ruagent_memory::query::fts_pattern`（一行）；与 R-1 同批，`api.rs` 现被 t96 占着 |
| R-7 | `crates/daemon/src/memembed.rs:200` | `keyword_pattern` 现在是**重复实现**（本单搬走后它还留着；行为等价，因为 `fts_pattern` 幂等） | 删掉本地实现、直接调共享函数（同样在 api.rs/daemon 窗口） |
| R-8 | `crates/graph/src/lib.rs:357,412` · `crates/graph/src/retrieve.rs:517,578`（`entities_fts MATCH ?1`）· `crates/store/src/lib.rs:1187,1382`、`crates/store/src/migrations.rs:560,634`（`chunks_fts_cjk MATCH ?1`） | **同族的其它 FTS 面**：它们也把参数直接交给 MATCH。本单**没有**取证这些参数是否来自用户文本（不在 inScope，也没有它们的载体） | 各 owner 自查一次：是用户文本就同样收口（graph / knowledge / store） |
| R-9 | 语义变更登记 | 多词查询 `AND → 短语`（§2 的差异读数） | 若要恢复「隐式 AND」，需要的是**逐词引用**且必须单独解决「用户合法引号短语」被拆散的问题（实测会破坏它）⇒ 属独立设计轮次，不建议在 repair 里改 |

## 7 纪律与收尾

- 端点读数用临时 root + 临时端口（18793 / 18794，均 **loopback**，未用 `--allow-remote`）；自起 daemon **PID 36180 / 10356 均已 `Stop-Process`**（`still_running=False`），按命令行匹配自己的临时路径复查 **0 残留**。
- **未写活库**：两台 daemon 都指向 `%TEMP%` 下的临时 root；`~/.ruagent` 未读写；未用 `/api/v1/recall`；**pid 79984 全程未启停**（收尾复查仍在）。before 那台用的是共享 target 里的**旧二进制**（只执行，未改写、未删除）。
- 磁盘：私有 target `%TEMP%\ruagent-t73-target` 用后已删（残留读数见提交记录）；两个临时 root 也已删。
- 未做/未覆盖（不静默跳过）：`search_fts` 的**其它调用方**（只读核对过：全是单词查询）；graph/store/knowledge 的 FTS 面（R-8，未取证）；panel 侧多词输入的行为变化（R-9，已登记）。
