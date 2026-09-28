# G10 收口：图证据块进注入面（t63）

- 单号 / 尝试：**t63** / attempt 1 / `attempt_id = 57729ee4-899a-47e1-a2d6-9805dfb79b64`
- 归属：**integ**（INT）
- 目标：把 mem-core 在 t58 交付的**生产者**（`memembed::graph_evidence_items` + `inject::graph_items` 契约）在两个注入路径上**调用一次**，使 `context_injected` 真的产出 `<graph>` 块。
- 背景读数（V-INT t20）：5 个 `context_injected` 的块分布 = **knowledge 5/5 + wiki 5/5 + graph 0/5**；RV-INT 判 **G10「不可达」**。

## 1 接线（两处，形状按 mem-core 交回）

| 文件 | 位置 | 内容 |
| --- | --- | --- |
| `crates/daemon/src/chat.rs` | 原 L602 `}` 与 L604 注释之间（**在 `items.sort_by_key` 之前**） | `if let Ok(evidence) = ruagent_graph::retrieve(db, &ruagent_graph::GraphQuery::for_text(query)).await { items.extend(crate::memembed::graph_evidence_items(&evidence, ruagent_memory::inject::GRAPH_PATHS)); }` |
| `crates/daemon/src/runs.rs` | 原 L1926 `}` 与 L1927 注释之间（**在 `items.sort_by_key` 之前**） | 同上，query = `run_query(task)` |

- **无新参数、无新依赖**（daemon 已依赖 `ruagent_graph`）；插在排序**之前** ⇒ **块序与丢弃序仍由 `tag_rank` 独占**。
- **一处与交回文字的差异（必须披露）**：runs.rs 里交回的写法是 `GraphQuery::for_text(&run_query(task))`，而 `&` 触发 `clippy::needless_borrows_for_generic_args`（clippy 门 **exit 101**，实测）⇒ 去掉 `&` 后 clippy **exit 0**。**语义相同**（`for_text` 接受借用/所有权均可）；这不是「改形状」，是**编译器要求的最小改写**，已记在此处。

## 2 盘上读数（K-5）

探针：**私有 target dir** 构建的新二进制（`-TargetDir %TEMP%\ruagent-t63-target`，**没碰真守护进程占用的共享 `ruagent.exe`**）；临时 root `%TEMP%\ruagent-t63-graph` + 临时端口 **8815**，daemon pid **70116**；`RUAGENT_EMBEDDER=hash` + mock agent `--behavior echo`；种入图语料（`ruagent --runs_on--> Kylin`）与一份知识文档；跑 **5 次 chat**（3 次带图证据的 query + 2 次负控 query）。

| 读数 | 值 |
| --- | --- |
| `context_injected` 事件数 | **5** |
| **带 `<graph>` 的事件** | **3 / 5**（V-INT 的基线是 **0/5**） |
| 带 `<knowledge>` / `<wiki>` | 5 / 5 · 0 / 5（本 root 没建 wiki 页） |
| 逐事件 | `ev1 graph=True len=242` · `ev2 graph=True len=242` · `ev3 graph=True len=242` · **`ev4 graph=False len=106`** · **`ev5 graph=False len=106`** |
| 块样本文本 | `… ruagent runs on Kylin T63GRAPH marker.` ‖ **`<graph>`** ‖ **`</graph>`** |

**⇒ G10 从「不可达」变成「有盘上读数」：注入面真的产出 `<graph>` 块（3/5），且只在有图证据的 turn 上出现。**

## 3 负控（无图证据的 turn）

`ev4` / `ev5` 的 query 在图里没有任何实体（`zzz-no-such-entity-t63` / `totally-unrelated-nonsense-t63`）⇒ **`graph=False`，且渲染长度显著更短（106 vs 242）** ⇒ **不出现 `<graph>` 块、也不出现空占位符**（与 mem-core 的适配器负控一致）。

## 4 `GRAPH_PATHS = 3` 的取值纪律（**没有读数就不假设最优**）

- **本轮没有取得「3 条够不够」的读数**：我原本想数块内证据行，但探针里的行匹配模式（`(?m)^.*<path|<path`）返回 **0 匹配**，而块确实存在（`<graph>…</graph>`）⇒ 那是**我的仪器缺陷**（模式没匹配到块内格式），**不是**「块里 0 条证据」的证据。
- **处置**：**保持 `GRAPH_PATHS = 3` 不变**（mem-core 的「最小证据块」是暂定值），并在本文件写明：**其最优性未被本代任何读数支持**；下一步的读数形状 = 把块内证据行按「每事件条数」数出来（用与块格式匹配的模式或直接读 `memembed::graph_evidence_items` 的单测输出），再回答 3 是否需要调整。

## 5 门禁（第 22 条形态）

| 门 | 读数 |
| --- | --- |
| `test --workspace` | **`TEST_EXIT=0`**；`Select-String -CaseSensitive` ⇒ `test result: ok` **55 行**、`FAILED` **0 行**、`panicked at` **0 行**、`Running ` **44 行**。**注**：这次读数取自**去掉 `&` 之前**的字节；之后的改动只有该一处 borrow 省略（无行为变化），**未重跑 workspace test**（预算）—— 如实记录，不冒充 |
| `clippy -p ruagent-daemon --all-targets -DenyWarnings` | **exit 0**（第 16 条：本单改的是 daemon crate，这条是**本 crate 的门**） |
| `clippy --workspace --all-targets -DenyWarnings -CleanFirst ruagent-daemon` | **exit 0 / 11.7s**；两件证据 = `clean -p ruagent-daemon (forced re-check)`（本轮 `Removed 71 files, 239.1MiB`）+ 本轮 `Checking ruagent-daemon / ruagent-memory / ruagent-mcp / ruagent / ruagent-mock-agent` |
| `cd panel && npm run build` | **exit 0 / 4.68s**（55 assets） |

## 6 收尾

- 临时 root `%TEMP%\ruagent-t63-graph` **已删**；探针 daemon pid **70116 已停**（`daemon_alive=False`）。
- **私有 target dir `%TEMP%\ruagent-t63-target` 也已删**（实测 **15.54 GiB**）—— 磁盘压力纪律优先于「留着热缓存」；要重跑本探针需重新构建一次（冷构建实测 **10m05s**）。
- **真守护进程 pid 79984 未触碰**（`StartTime = 09/27/2026 05:35:37`）；**未写活库**。
- 更改路径：`crates/daemon/src/chat.rs`、`crates/daemon/src/runs.rs`、本文件。
