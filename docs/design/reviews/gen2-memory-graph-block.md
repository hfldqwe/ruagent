# 图证据进注入面（t58 / F5 / G10，mem-core）

- 任务：t58（kind=repair round 1）；attempt `3fc9d6d4-7961-4e54-ad1e-1362c51fe373`
- inScope（stored）：`crates/memory/` · `crates/daemon/src/memembed.rs` · `docs/design/reviews/gen2-memory-spec.md` · 本报告
- outOfScope（未触碰）：`crates/knowledge/` · `crates/graph/` · `crates/store/` · `crates/daemon/src/api.rs` · **`crates/daemon/src/chat.rs`** · **`crates/daemon/src/runs.rs`** · `panel/` · `cli/`
- 纪律：不写活库、未启停 pid 79984、未用 `/api/v1/recall`

## 1 先复现 V-INT 的读数

**它报的**（F5，含图证据的同一批 turn）：5 个 `context_injected` 的块分布 = `knowledge 5/5 + wiki 5/5 + **graph 0/5**`；同根 `/recall` 读到 `graph.entities=1..5`、`graph.paths>0` ⇒ **不是「没样本」**，所以 §6.3 的豁免不适用。

**我的复读（代码侧，`Select-String`，2026-09-28）**

| 读数 | 结果 |
| --- | --- |
| `TAG_GRAPH\|graph_items\|graph_evidence_items\|GraphEvidence` 的命中文件 | `crates/memory/src/inject.rs`（15 处）· `crates/daemon/src/memembed.rs`（10 处，**本单新增**）· 其他 crate **0 处** |
| `crates/daemon/src/chat.rs` / `runs.rs` 里的构造点 | **0 处** |
| `graph_evidence_items(` 的调用者 | 只有**定义**与**本单的测试**（`memembed.rs:764/922/957`）⇒ **没有生产调用者** |
| 因此生产的 `<graph>` 计数 | **仍是 0/5** |

⇒ **G10 判「未达标」，不是 not_measured**：生产者已存在（`crates/graph::retrieve` → `GraphEvidence::lines()`），缺的是**一次调用**。把它记成 not_measured 等于把「没人接线」写成「没有依赖」。

## 2 裁决：选 (a)，并在 inScope 内做到边界

**(a) 的两半都在我的 inScope 里，已完成**；**(a) 的第三半（生产构造点）在 outOfScope，按验收第 4 条交回 captain**。

| 层 | 归属 | 本单状态 |
| --- | --- | --- |
| 契约：tag/块形/预算/空输入规则 | `crates/memory/src/inject.rs`（我） | **已落地** |
| 适配器：`GraphEvidence` → `ContextItem` | `crates/daemon/src/memembed.rs`（我） | **已落地** |
| 生产构造点：在两条注入路径里取图证据并 push | `crates/daemon/src/chat.rs:602` 之后 · `runs.rs:1926` 之后（**integ 的文件**） | **未落地 ⇒ G10 未达标，owner = 下一步入口** |

不改 chat.rs/runs.rs 的理由不是「不方便」：t58 的 outOfScope 明确列了这两个文件（一个文件一个写者），而缺的那 5 行**不需要新依赖、不需要新参数**，交回比越界便宜。

## 3 本单落地的形状（契约 + 适配点）

```rust
// crates/memory/src/inject.rs（契约侧）
pub const GRAPH_PATHS: usize = 3;
pub fn graph_items(lines: &[String], limit: usize) -> Vec<ContextItem>;

// crates/daemon/src/memembed.rs（唯一转换处）
pub fn graph_evidence_items(e: &ruagent_graph::GraphEvidence, limit: usize)
    -> Vec<ruagent_memory::inject::ContextItem>;
```

- **依赖方向**：`ruagent-memory` 只依赖 `core` + `store`（`crates/memory/Cargo.toml` 实测），所以契约收**已渲染的文本行**；渲染（一行内含 `hops / valid_at / state / edges / source`，RVC-5）归 `crates/graph::GraphEvidence::lines()`。这与 `RetrievalHit`/`WikiLeadMeta`/`RelevanceMeta` 是同一套「本地纯数据类型 + 唯一适配器」形状。
- **不发明内容**：空输入 ⇒ 空 Vec ⇒ 调用方**不发块**（与 `knowledge_items` 同一条规则）；**空白行被丢弃**（否则块里出现「有证据但内容缺失」）；不给行加日期（行自带时间/来源）。
- **预算**：`GRAPH_PATHS = 3` 是**契约里的一个数**，不是调用点的魔法字面量；`tag_rank(graph)=3` 与丢弃顺序**未改**（graph 比 wiki/project_context 更晚被丢）。

## 4 读数（正读 / 负控 / 不变量）

**(1) 契约侧**（`cargo test -p ruagent-memory`，`--nocapture`）

```
READING t58 graph items: 2 (limit=3)
READING t58 graph block:
<graph>
用户19410 -uses-> 微信 (hops=1 valid_at=2026-09-20T00:00:00Z state=current edges=e12 source=session:abc)
微信 -runs_on-> macOS (hops=2 state=current edges=e12,e13 source=session:abc)
</graph>
READING t58 no-evidence render: ""
READING t58 blank-lines items: 0
READING t58 cap: lines=10 limit=3 items=3
```

**(2) 适配器侧**（`cargo test -p ruagent-daemon --lib -Nocapture memembed`）

```
READING t58 adapter: lines()=["用户19410 -uses-> 微信 [hops=1 valid_at=2026-09-20T00:00:00Z state=current edges=12 source=session:abc]"]
READING t58 adapter: items=1
READING t58 adapter: rendered block:
<graph>
用户19410 -uses-> 微信 [hops=1 valid_at=2026-09-20T00:00:00Z state=current edges=12 source=session:abc]
</graph>
READING t58 adapter: empty evidence -> items=0 render=""
```

- **正读**：有一条第 1 跳路径的真实 `GraphEvidence`（手工夹具，因为 `retrieve` 需要数据库；这正是「测适配器而不是测走图」）⇒ 块出现，行的**出处字段一个没丢**。
- **负控**：结构合法但 `paths=[]` 的 evidence ⇒ `items=0`、`render=""`（**不发块、不发占位符**）；`["", "   ", "\n"]` ⇒ 0 项；**只有 knowledge 的渲染里不含 `<graph>`**。

**(3) 注入契约不变量（加块之后仍然成立）**

```
READING t58 long-line render (per_block=120): … [+N chars truncated] 在块内可见
READING t58 tight-total render (total=120): … [+N items dropped: context budget reached]（块被整块丢并计数）
```
- `per_block=120` + 400 字符的行 ⇒ 截断标记可见，`chars ≤ total`。
- `total=120` + 一个 user_profile 行 ⇒ 整块 `<graph>` 被丢且**计数进 `<context_budget>`**，`chars ≤ total`。
- 对 `total = 20/60/200/1000` 四档断言 `chars ≤ total`（把这代已有的有界性质在**新块**上再钉一遍）。

**这一条至关重要**：`GRAPH_PATHS` 只约束**行数**，不约束**字符**；真正的界仍由 `render_context` 的 `per_block`/`total` 给出，上面的读数就是「加块没有把界打开」的证据。

## 5 交回的 finding（确切构造点 + 需要的形状）

| 文件 | 插入点（当前行号） | 形状（逐字可用） |
| --- | --- | --- |
| `crates/daemon/src/chat.rs` | `knowledge_items_enriched(...)` 的 `if let` 块**之后**（第 602 行 `}` 与第 604 行注释之间） | `if let Ok(evidence) = ruagent_graph::retrieve(db, &ruagent_graph::GraphQuery::for_text(query)).await { items.extend(crate::memembed::graph_evidence_items(&evidence, ruagent_memory::inject::GRAPH_PATHS)); }` |
| `crates/daemon/src/runs.rs` | 知识块 `if let` **之后**（第 1926 行 `}` 与第 1927 行注释之间） | 同上，query 换成 `run_query(task)` |

理由与依据：
- `chat.rs::injection_context(db, knowledge, embedder, query: &str)` 与 `runs.rs::render_run_injection(db, task, knowledge)` **都已有 `db`**，`query`/`run_query(task)` 也在作用域内 ⇒ **不需要新参数、不需要新依赖**（daemon 已依赖 `ruagent_graph`）。
- 插在这两处的位置保证 `items.sort_by_key(tag_rank)` 仍然在最后执行（两文件分别在 608/1937 行排序），所以**块顺序与丢弃顺序不受影响**。
- 接线后**必须**给 K-5 要求的盘上读数：至少一条带 `<graph>` 的 `context_injected` 事件（这是 G10 达标与「第三层证据缺失」的分界）。

## 6 门禁

| 命令 | 读数 |
| --- | --- |
| `test -p ruagent-memory` | **exit=0** · `66 passed; 0 failed; 0 ignored`（3.43s）+ doctest 0/0（t52 后 62 → +4 条 graph 块测试） |
| `clippy -p ruagent-memory --all-targets -DenyWarnings -CleanFirst ruagent-memory` | **exit=0** · 含本轮 `Checking ruagent-memory`（真重查），零诊断 |
| （第 16 条：本单跨 crate 改了 `memembed.rs`）`clippy -p ruagent-daemon --all-targets -DenyWarnings -CleanFirst ruagent-daemon` | **exit=0** · `Checking ruagent-daemon v0.1.0`，零诊断 |
| （额外）`check -p ruagent-daemon --all-targets` | **exit=0** |
| （额外）`test -p ruagent-daemon --lib -Nocapture memembed` | **exit=0** · 3 passed / 75 filtered（适配器的三条读数见 §4） |

**越界证明**：本单改动 = `crates/memory/src/inject.rs` · `crates/daemon/src/memembed.rs` · 规格追加段 · 本报告；`chat.rs`/`runs.rs`/`api.rs`/`crates/graph`/`crates/store`/`panel`/`cli` **未触碰**。

## 7 未做 / 边界（无静默跳过）

- **生产的 `<graph>` 仍是 0/5**（本单结束时）：契约与适配器就位，但两个生产构造点在 outOfScope ⇒ **G10 未达标，owner = 下一步入口**（§5 的确切坐标与形状已给）。这是本单**唯一**未闭合项，且不是「测不了」，是「还没接线」。
- **第三层（盘上）读数仍缺**：需要真实 turn 跑一次注入才有 `<graph>` 事件；本单不启动真 daemon（也不写活库），所以这一条留给接线后的那张单。
- **`GRAPH_PATHS=3` 的取值依据是「最小证据块」**，不是实测最优：接线后应从 `context_injected` 事件里读「3 条是否够/是否太多」再定；今天没有盘上读数可依据（诚实标注，不假设）。
- 未改 `crates/graph` 的 `lines()`（内容形状归它，RVC-5 的箭头方向已有它的测试）；未改 `api.rs` 的 `/graph/retrieve`（同源证据面）。
