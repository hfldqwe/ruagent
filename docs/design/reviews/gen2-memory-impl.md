# gen2 记忆实现（I-B / t8，mem-core）

- 任务：t8 I-B —— 按 `docs/design/reviews/gen2-memory-spec.md` 实现记忆生命周期
- attempt：`65959f93-c402-4e4a-b2c8-da1fe89e0676`（round 1）
- 改动面（inScope）：`crates/memory/**` · `crates/daemon/src/memembed.rs` · `docs/design/reviews/gen2-memory-spec.md`（本报告另计）
- 未改动：`crates/store` · `crates/knowledge` · `crates/graph` · `crates/daemon/src/api.rs` · `crates/daemon/src/wiki.rs` · `crates/daemon/src/distill.rs` · `panel/`（`git status --porcelain` 见 §7）
- 环境：`CARGO_TARGET_DIR=%TEMP%\ruagent-ib`（验证用 `%TEMP%\ruagent-verify-ib`）；真 daemon pid 79984 未启停、`~/.ruagent` 未写、未用 `/api/v1/recall` 取证

---

## 1 一句话

C-local 的七条目标全部落地（其中三条的「活体半边」按 R-3 标 jointly-delivered 并由 I-C/I-A/I-D 交付），注入契约多了两个跨区字段的**适配形状**（`EnrichedHit`），并且每一条新判据都在**改动前的代码上被跑红过**（§5）。**一处按 §7.138 上报的偏差**：`RetrievalHit` 没有加字段，见 §4.1 —— 加字段会让 `chat.rs`/`runs.rs`（integ 的文件）编译不过，而验收要求 `cargo check -p ruagent-daemon --all-targets` 通过。

---

## 2 逐条目标：改前 → 改后（含产出该数的命令）

全部读数来自 `cargo test -p ruagent-memory` 的 stdout（测试里 `println!("READING …")`），命令：

```powershell
$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"
$env:CARGO_TARGET_DIR="$env:TEMP\ruagent-ib"
cargo test -p ruagent-memory -- --nocapture
```

**测试计数：改前 28 passed / 0 failed（R-B 规格 A.1，2026-09-27T21:28:58+08:00）→ 改后 53 passed / 0 failed。**

### C1 置信度可校准

| 项 | 改前 | 改后 |
| --- | --- | --- |
| 判据落点 | 无 confidence 模块；三条写路径：HTTP 常量 `0.9`（api.rs:3730）、MCP 无参数、distill `clamp(0.5,1.0)`（distill.rs:467） | `crates/memory/src/confidence.rs`：`ConfidenceSignals` + `confidence()`，具名常量 `CONF_CONFIRMED=1.0 / CONF_CORRECTED=1.0 / CONF_HEDGED=0.4 / CONF_UNCONFIRMED=0.8 / LOW_CONFIDENCE=0.5` |
| `<0.5` 是否可达 | live **0/157**；三条生产路径都不可达 | `hedged -> 0.4`（`READING C1 hedged -> 0.4`）；12 条事实场景 `distinct=[0.4, 0.6, 0.8, 1.0]`，其中 `is_low` 命中 1 条 |
| 优先级 | 无（不存在） | `explicit > confirmed > corrected > hedged > unconfirmed`，并由 `confirmed_beats_hedged` 钉住 |
| 阈值单源 | — | `rule_values()` 从模块导出，测试从不复写数字（`READING C1 rule_values=[0.4, 0.8, 1.0] LOW_CONFIDENCE=0.5`） |
| 注入可见 | 注入面**完全不渲染** confidence（`ContextItem` 无字段） | §C6/D.4：`[unverified]` 前缀 |

**jointly-delivered（R-3）**：mem-core 半边 = 已交付（上表）；**graph / I-C 半边 = `distill.rs:467` 去 `clamp(0.5,1.0)` 并把信号交给 `confidence()` —— 未落地 ⇒ 记 not_measured（原因：依赖未落地）**。活库上 `<0.5` 仍为 0 行，这是预期的（规格 C1 已写明：活库无正例不等于功能未落地）。

### C2 合并判据：有界候选集 + 具名判据 + 可审计决策

| 项 | 改前 | 改后 |
| --- | --- | --- |
| 判据 | `judge()` 是全部判据；候选面 = 同 (store,namespace) **全部 live 行**（distill.rs:436-438，无 LIMIT） | `dedupe::merge_decision(new, &[MergeCandidate], &MergeConfig)`：先 `bounded_candidates(top_k)`，再词法判据，最后「升级判定」 |
| 真实改写 | 0.9884 的改写对 `Refused("a content word is in the difference")` ⇒ **召回 0/2297** | 同一对 → `NeedsJudgement { candidate: 5, cosine: 0.9884 }`（`READING C2 decision ...`） |
| 极性 | 终止（本就如此） | **仍然终止**：`a_polarity_flip_is_never_escalated`（cos 0.99 也不升级） |
| 阈值 | 无（或文献常数 0.90：实测放行 45.19% 的配对） | `tau_scope` **由该 scope 自己的分布导出**：`scope_tau(cosines, TAU_PERCENTILE=0.99)`；`MergeConfig::default()` 的 0.95 只是「分布不可得时」的兜底，并在 doc 里写明来自 live p99 |
| 候选界 | 无界全表扫 | `DEFAULT_TOP_K = 3`（标定：cos≥0.95 的 24 对，K=1→21、K=2→23、**K=3→24**，R-B A.9）；缺失向量排最后（未知 ≠ 0.0），tie 用 id 升序（确定性） |
| 决策审计 | `memory_diffs` 只有**结果**（insert/supersede/skip_dedupe…），没有决策 | `write::audit_merge_decision` 落 `op='merge_judged'`，`reason` 由**唯一写者** `merge_audit_reason` 产出：`candidates=3 rule=lexical_ignorable verdict=merge`（`READING C2 audit reason`） |
| 候选集生成（memembed） | 无 | `memembed::merge_candidates()`：scope 行 + 行内向量 → 余弦；`tau_scope` 取 scope 内采样对（≤`TAU_SAMPLE=64` 行）的 p99；`embedder.embed()` 失败时给 `cosine: None`（未知，仍过词法） |
| 决策接线 | distill 自己判 | `memembed::judge_merge()` 判定**并审计**，返回 `(MergeVerdict, reason)`；**I-C(t9) 调用它** ⇒ jointly-delivered |
| schema | — | **无需迁移**：`memory_diffs.op` 无 CHECK（0020 已核），新值 `merge_judged` 直接可写 |

### C3 衰减与巩固（使用状态 → 排序）

| 项 | 改前 | 改后 |
| --- | --- | --- |
| 使用状态列 | `memories` 0/7 个使用类列 | 0020 的 `access_count` / `last_used_at` 被消费；`COALESCE(NULL,0)+1` 的**首次写入**是记账（该行确实被用了），NULL 仍是「计数器存在前」 |
| 排序 | `ORDER BY updated_at DESC`（写入时间）；注入选择自述「N most recent, not N most relevant」 | `usage::decay_score(base, updated_at, now, access_count) = base + 0.5·e^{-Δt/(S·14d)}`，`S = 1+uses`；`READING C3 fresh+used=0.4998 stale+unused=0.0587 stale+used=0.3498`（同一 base=0，只有使用与新鲜度不同） |
| 未知 ≠ 零 | — | `unknown_usage_is_not_treated_as_heavily_used`：`None` 与 `Some(0)` **同分**，且都低于 `Some(9)` |
| 谁写回 | 无（`recall_log` 651 行，0 行写回） | `usage::record_usage(db, ids)`；`memembed::select_injection_memories` 对**真正进入注入**的 id 调它（`READING C3 access_count=Some(2)`） |
| 窗口 | 每组只读 `limit` 行（排序无法改变**哪些**行进窗口） | 每组读 `limit × DECAY_OVERFETCH(=3)` 行 → 按 decay 排序 → 截断到 limit；tie 用 id 升序 |
| 不删 | — | 衰减**从不**导致删除：`decay_score` 恒 `> 0`，删除仍只走 `delete/purge` + 审计 |

**行为影响（登记）**：C3 的 overfetch 会改变**哪几行**进入注入（不再是「最近的 N 行」）。这是规格 C3 要求的变化，且只影响排序/窗口；`chat.rs`/`runs.rs` 未改。**活读数**：改后 live 库 163 行的 `access_count` 仍全为 NULL（历史行，符合 0020 的三态纪律）；第一次真实注入后才会出现非 NULL 值。

### C4 会话关闭的巩固/反思

| 项 | 改前 | 改后 |
| --- | --- | --- |
| 来源存储 | 无表、无字段（`memory_sources` 不存在） | `consolidate::record_consolidation(&ConsolidationWrite{ memory_id, sources })` → `memory_sources`；`READING C4 first=2 second=0`（**幂等**） |
| 双向可查 | 只有 `memories.source_episode`（正向） | `sources_of(memory_id)`（正）与 `derived_from(kind, id)`（反，**遗忘面要用**） |
| 幂等键 | 无 | `consolidation_key(content, sources)`：内容 + **排序去重后的来源集合** 的 sha256（64 hex）；来源顺序无关、内容不同则不同 |
| 会话读路径 | 无（面板/接口只能读 episode 计数） | `query::session_memories(db, session_key)`：经 `episodes.source_run` 反查该会话产出的记忆（`READING C4 session_memories = 2`） |
| 产出侧 | distill 写 run_turn episode | **I-C(t9)**：`run_turn` episode + 反思产物的内容与判定 ⇒ jointly-delivered；当前 `run_turn` episode **仍为 0** ⇒ 「一次真实蒸馏产出正例」判 **not_measured（原因：依赖未落地）** |

### C5 遗忘与可审计

| 项 | 改前 | 改后 |
| --- | --- | --- |
| 记忆侧消失 | 已成立（purge 真删 + FTS 触发器跟随） | 由 `forget_report` 读出来：purge **前** `memories hits=1 / fts hits=1`，**后** `0 / 0` |
| 「还有哪里」 | **无法回答** | `lifecycle::forget_report(db, content_hash)`：4 个本地面（`Memories` 任意状态 / `MemoriesFts` / `Derived` / `Episodes`）+ 6 个外部**请求**面 |
| 三态 | 二态（有/无） | `ResidualStatus::{Readout(ResidualCount), NotAvailable(&'static str)}`；`Episodes` = `NotAvailable("episodes has no column linking a memory's hash to a transcript …")`；知识/wiki 六面 = `NotAvailable` 并具名 owner（I-A/recall、I-D/wiki）。**没有一处写 `Readout(0)` 冒充「查过了」** |
| 截断/总计 | — | `ResidualCount{hits, truncated, total: Option<usize>}`：`None ≠ 0`（照 C5-D 语义第四条） |
| 报告不含内容 | — | 断言：`format!("{report:?}")` 里**没有**被遗忘的文本，只有 `memory/<id> (<ns>)` 这类标识 |
| 派生结论 | 无法查 | `Derived` 面经 `memory_sources` 反查（`READING C5 derived surface: … ["memory/2"]`） |
| 求值边界 | — | `RESIDUAL_SAMPLE_LIMIT=20`（**单源常量**），超出时 `truncated=true` |

### C6 注入面：可见账目 + graph 块 + 低置信标记

| 项 | 改前 | 改后 |
| --- | --- | --- |
| 预算单位 | 字符（1024/4096），两路径都调 `render_context` | **未动**（冻结） |
| 可见截断 | `… [+N chars truncated]`（契约唯一词表） | **未动**；新增测试确认低置信行被截断后标记仍可见 |
| 丢块计数 | 整块丢 + 计数（条目数） | **未动**；`never_exceeds_total` / `total_budget_drops_blocks_visibly` 仍绿 |
| tag 序 | `user_profile 0 / relevant_memories 1 / knowledge 2 / wiki 3 / project_context 4 / 未知 5` | `… / knowledge 2 / **graph 3** / wiki 4 / project_context 5 / 未知 6`（`READING C6 tag ranks: … graph=3 wiki=4 project=5 unknown=6`） |
| 低置信 | 无字段、无标记 | `ContextItem.confidence: Option<f64>` + `dated_with_confidence()`；`LOW_CONFIDENCE_MARK="[unverified]"` 作为**行首前缀**渲染（见 §4.2：后缀会被 per_block 截掉，是本单自己的测试抓到的） |
| wiki 线索 | `wiki/<slug>: <正文>`（页的自述不过桥） | 带 lead 时渲染 `slug: <title> — stale=… coverage=… anchors=N edited=…` + hint 行 + 正文 + `anchors: doc#id` 取回行；**不带 lead 时字节与改前逐字相同**（断言 `<wiki>\nwiki/kubernetes-troubleshooting: 当 Pod 出现 crash-loop 时 …\n</wiki>\n`） |

**两条消费路径一致性（读数）**：`chat.rs:581` 与 `runs.rs:1906` 都调 `render_context(&items, &InjectionBudget::default())`，选择规则同为 `memembed::select_injection_memories`，排序键同为 `tag_rank`（都已核）；本单未给 daemon 增加接线点（memembed 是我的文件）。crate 内读数：截断标记与丢块计数的测试全绿；**第三层（盘上 `context_injected` 里出现 `<graph>`）判 not_measured**（图块内容由 I-C 产出）。

### C7 诚实失败（mem-core 半边）

| 项 | 改前 | 改后 |
| --- | --- | --- |
| 未知 store 的读 | `row_to_memory` 的 `_ => Observation` 静默降级：实测 `all_memories` 返回 `Ok([MemoryRow { store: Observation, … }])` | `MemoryStore::try_parse` + 显式错误：`Err("unknown memory store \`bogus\`")`（`READING C7 …`） |
| 单源解析 | 三处各写一份（api.rs / distill.rs / query.rs） | 读路径用 `MemoryStore::try_parse`（单源）；api.rs 的 400 半边归 INT/t19（C-INT-1，本单未改 api.rs） |
| namespace | list 缺省 = 不过滤（已由 t246 修） | 未动 |

---

## 3 跨区接口

### 3.1 R-D D.7（G8 的记忆半边）：`WikiLeadMeta` + `<wiki>` 块自述

- 已交付（`crates/memory/src/inject.rs`）：`WikiLeadMeta { slug, stale: Option<bool>, stale_since, edited, cite_coverage, anchors: Vec<(String,i64)>, hint }`、`WIKI_LEAD_HINT`、`render_wiki_item()`，并由 `a_wiki_lead_renders_its_marks_and_a_plain_wiki_hit_does_not_change` 钉住字节。
- **未落地半边（not_measured，原因：依赖未落地）**：I-D(t10) 的 `WikiLead`/`lead_for` 生产端；构造点在 chat.rs/runs.rs（I-INT/t19）。本单只出契约与渲染。
- **与 D.7 例子的偏差（登记）**：D.7 的示例把 `(cite: ops-handbook#18)` 挂在**正文行尾**。本单把它落成块尾的 `anchors: doc#id (GET /api/v1/knowledge/expand/{chunk_id})` 行，因为「哪条命中对应哪个 chunk」需要命中自带 `chunk_id`，而 `RetrievalHit` 加字段会破坏 chat.rs/runs.rs（§4.1）。锚仍在同一块内、仍可取回，但**不是逐行 cite** —— 若 captain/I-D 要求逐行，需 integ 在构造点把 `chunk_id` 传进来。

### 3.2 R-A H-4：注入重排消费 `relevance`

- 已交付：`RelevanceMeta { value, kind, version, query_background }`；`EnrichedHit::order_value()`（有 relevance 用它，否则用 `hit.score`，**替换点只有一处**）；`knowledge_items_enriched()`。
- 读数（crate 内，合成命中集）：改前按名次分 = `["a: body of a", "b: body of b", "c: body of c"]`；改后按 relevance = `["c: body of c", "b: body of b", "a: body of a"]`（`READING H-4 …`）。
- **不得当相似度送进注入**：断言 `the_relevance_number_is_not_rendered_into_the_block`（渲染里既无 `0.8615` 也无 `0.016393`）。
- **未落地半边（not_measured，原因：依赖未落地）**：`crates/knowledge` 的 `SearchHit.relevance`/`search_page`（I-A t7）与 daemon 构造点（I-INT t19，见其 H-1）。

---

## 4 上报的偏差与不可达项（§7.138：不擅自降低判据）

### 4.1 `RetrievalHit` 未加字段，改用 `EnrichedHit`（请 captain 裁决）

- 任务单字面要求：`RetrievalHit.lead: Option<WikiLeadMeta>`（R-D D.7 与 R-A H-4 都这么写）。
- 事实：`RetrievalHit` 是**公开字段结构体**，`chat.rs`（560-568）与 `runs.rs`（1884-1891）用**结构体字面量**构造它。加字段 ⇒ 这两个 out-of-scope 文件（integ 的）编译不过 ⇒ 验收要求的 `cargo check -p ruagent-daemon --all-targets` **必定失败**（除非我改别人的文件，而任务单明令不许）。
- 本单选择：**适配形状** —— `EnrichedHit { hit, lead: Option<WikiLeadMeta>, relevance: Option<RelevanceMeta> }` + `knowledge_items_enriched(&[EnrichedHit], sources, wiki)`；`knowledge_items(&[RetrievalHit], …)` 保留为薄包装（`EnrichedHit::plain`），**既有调用点零改动、字节零变化**。
- 影响面：integ 只需在构造命中时**多传一个类型**（一个构造点），契约字段与语义与 D.7/H-4 一致。
- 若 captain 要求字面形状：需要 integ(t19) 与我在同一窗口改 `chat.rs`/`runs.rs` 两处，且本单验收命令在那之前会红。**请裁决。**

### 4.2 `LOW_CONFIDENCE_MARK` 是**前缀**而不是规格 D.4 冻结的「行尾后缀」

- 规格 D.4 原文写「行尾追加 ` [unverified]`」。本单的测试 `the_low_confidence_mark_survives_truncation` 当场把它跑红：一条 4000 字符的低置信行按 `per_block=100` 截断后，**后缀被截掉** —— 标记在最需要它的那一类行长上消失。
- 改法：标记作为**行首前缀**渲染（`[unverified] [2026-09-27] …`），任何截断都截不掉它。规格 D.4 已按此**追加**修订（原文保留，加修订块）。

### 4.3 其余 not_measured（逐条原因）

| 项 | 原因 |
| --- | --- |
| C1「活体 `<0.5` 可达」 | 依赖 I-C(t9) 去掉 distill 的 `clamp(0.5,1.0)` |
| C2「在 distill 写入路径上生效」 | 依赖 I-C(t9) 调 `memembed::judge_merge` |
| C4「一次真实蒸馏产出正例」 | 依赖 I-C(t9) 产出 `run_turn` episode（今天 0） |
| C5「残余面的 knowledge/wiki 半边」 | 依赖 I-A(t7) `residual_scan` 与 I-D(t10) 只读查询（我按 `NotAvailable` 记录，不写 0） |
| C6「盘上 render 出现 `<graph>`」 | 依赖 I-C 的图块内容与 integ 的转 `ContextItem` |
| R-D D.7 生产端 / R-A H-4 生产端 | 见 §3.1/§3.2 |
| C-INT-1/C-INT-2（api.rs） | 归 INT/t19（C-INT 节，验收 t20/t21）；本单未动 api.rs 一行 |

---

## 5 反例证据：新判据在**改动前**的代码上被跑红

装置（任务单允许的「临时 worktree」，不碰同伴文件）：

```powershell
git worktree add "$env:TEMP\ruagent-t8-old" HEAD      # HEAD = 0a39e5b8（t8 之前）
# 把四个只使用「旧 API」的断言写进该工作树：crates/memory/tests/old_code_counterexample.rs
$env:CARGO_TARGET_DIR="$env:TEMP\ruagent-ib-cf"; cd "$env:TEMP\ruagent-t8-old"
cargo test -p ruagent-memory --test old_code_counterexample
```

读数（**4 failed; 0 passed**，逐条原文）：

| 断言 | 旧代码上的结果 |
| --- | --- |
| C2 真实改写不得被静默拒绝 | `OLD CODE C2 verdict on the 0.9884 paraphrase: Refused("a content word is in the difference")` → panic |
| C6 graph 块必须在 wiki 之前 | `OLD CODE C6 tag_rank(graph)=5 tag_rank(wiki)=3` → panic |
| C1 低置信行必须有标记 | `OLD CODE C1 render: "<user_profile>\n[2026-09-27] the user might use xlwt\n</user_profile>\n"`（无标记）→ panic |
| C7 未知 store 必须报错 | `OLD CODE C7 all_memories … Ok([MemoryRow { id: 1, store: Observation, …, content: "a row from nowhere" }])` → panic |

**C3 与 C5 的反例是编译级缺席**（旧代码里符号不存在，非运行期断言失败），原文：

```
error[E0433]: cannot find `usage` in `m`
error[E0425]: cannot find function `forget_report` in crate `m`
```

这两条的行为学「改前读数」因此取自规格的现场基线：使用类列 **0/7**（无衰减输入）、`forget_report` **不存在**（不存在残余面清点）。**这是弱证据，已按弱证据写明。**

---

## 6 对 A / C / D / E 的消费影响与尚未解决项

### 6.1 对 A（基线表）的影响

- A.3「`<0.5` 不可达」已由 C1 的**模块**修掉，但**活读数仍为 0 行**（依赖 distill 去 clamp）⇒ A.3 的读数在 I-C 落地前不变。
- A.9「mergeable 0/2297」仍是判据在**活库**上的触发率；改后新增的是**升级判定**（`NeedsJudgement`），它不会自己增加活库的合并数 —— 真实合并要靠 I-C 调用 `judge_merge`。
- A.10 注入面读数**未变**（预算、截断、丢块词表均冻结）；新增 `<graph>` 与 `[unverified]` 两种可能字节，但都需要生产者。

### 6.2 对 C（目标表）的影响

C1/C2/C3/C5/C6/C7 的 mem-core 半边全部落地（§2）；C4 的存储/读路径落地。**C-INT 未动**。新增一个原规格没有的常量：`DECAY_OVERFETCH`（理由：不然 decay 只能重排窗口、不能改变窗口内容）。

### 6.3 对 D（冻结接口）的影响

- `ContextItem` **新增公开字段** `confidence: Option<f64>`（D.4 的破坏性变更 B-3）：因为所有构造都走构造器，`chat.rs`/`runs.rs` **未受影响**，daemon check 绿。
- 新增公开 API：`LOW_CONFIDENCE_MARK` · `TAG_GRAPH` · `EnrichedHit` · `WikiLeadMeta` · `RelevanceMeta` · `WIKI_LEAD_HINT` · `knowledge_items_enriched()`。
- `tag_rank` 值域 0..6（D.2 的 B-1）；`LOW_CONFIDENCE_MARK` 由后缀改前缀（§4.2，D.4 规格已追加修订）。
- `RetrievalHit` / `knowledge_items` / `render_context` / `InjectionBudget` / 截断词表 **签名与字节未变**（golden 仍绿）。

### 6.4 对 E（实现清单）的影响

E.1（confidence.rs，新）· E.2（dedupe 决策 + write 审计 + query 候选面）· E.3（usage.rs，新；消费 0020 列）· E.4（consolidate.rs，新；消费 `memory_sources`）· E.5（lifecycle `forget_report`）· E.6（inject）· E.8 的 mem-core 半边（`query.rs`）**均已落地**。E.7 归 I-C/I-SCHEMA。

### 6.5 尚未解决项

1. **§4.1 的适配形状**（`EnrichedHit` vs `RetrievalHit` 字段）等 captain 裁决。
2. **integ 的构造点**：`knowledge_items_enriched` 需要一个调用者把 lead/relevance 传进来（R-D D.7 / R-A H-4 的生产端）。
3. **I-C(t9) 的三处行为**：去 clamp、调 `judge_merge`、产出 `run_turn` episode 与反思产物。
4. **I-A(t7) / I-D(t10)** 的残余面只读查询（我按 `NotAvailable` 记录）。
5. `memory_diffs` 的 op 注释（0004 是冻结的历史迁移）仍只列 4 个 op，实际有 9 个 —— **不改历史迁移**，在 0020 的注释里已自述。

---

## 7 验收命令与本单的读数

全部在 `CARGO_TARGET_DIR=%TEMP%\ruagent-ib`（验证目录 `%TEMP%\ruagent-verify-ib` 同义），时间 2026-09-27T23:3x–23:5x+08:00。

| 命令 | 结果 | 读数/原文 |
| --- | --- | --- |
| `cargo test -p ruagent-memory` | **exit 0** | `test result: ok. 53 passed; 0 failed`（改前基线 28 passed，R-B A.1） |
| `cargo clippy -p ruagent-memory --all-targets -- -D warnings` | **exit 0** | `Finished dev profile`。过程中修掉一条**既有**告警：`lifecycle.rs` 的 `type_complexity`（`purge_memory` 的三元组 → 命名别名 `Doomed`，行为不变）。这条命令在本单之前是**红的** |
| `cargo check -p ruagent-daemon --all-targets` | **exit 0** | `Finished dev profile`。中途曾红，两段原因都记在下面 |
| `git status --porcelain`（我的 inScope） | — | `M crates/daemon/src/memembed.rs` · `M crates/memory/src/{dedupe,inject,lib,lifecycle,query,write}.rs` · `?? crates/memory/src/{confidence,consolidate,usage}.rs` · `?? docs/design/reviews/gen2-memory-impl.md` · `?? docs/design/reviews/gen2-memory-spec.md` |
| `git status --porcelain`（out-of-scope 路径） | — | `crates/daemon/src/distill.rs`(graph) · `crates/daemon/src/wiki.rs`(wiki) · `crates/knowledge/src/{lib,rrf,store}.rs`(recall) · `crates/store/src/{fts,lib,migrations}.rs` + `0019–0023`(t6) · `crates/graph/**`(graph) · `panel/` **未出现在任何一条**。这些改动**都不是我做的**，我一个字节都没写 |

**「没碰别人的文件」的第二重证据**（第一条是文件清单，第二条是符号）：本单新增的符号在这些文件里**一个都搜不到**，而它们自己的在途改动量很大 —— 说明两条工作线没有交叠：

```
Select-String -Path crates/store/src/*.rs,crates/knowledge/src/*.rs,crates/graph/src/*.rs,
  crates/daemon/src/{api,wiki,distill}.rs -Pattern 'audit_merge_decision|decay_score|EnrichedHit|
  WikiLeadMeta|forget_report|record_usage|consolidation_key'
  ⇒ （无匹配）

git diff --stat -- crates/daemon/src/distill.rs crates/daemon/src/wiki.rs crates/knowledge crates/store
  distills.rs 492+ · wiki.rs 2060+ · knowledge/lib.rs 9 · rrf.rs 75+ · store.rs 713+ ·
  store/fts.rs 209+ · store/lib.rs 306+ · store/migrations.rs 544+   ⇒ 8 files, 4118 insertions(+)
```

### 7.1 期间遇到的两类红，以及本单的处理（纪律：不替同伴改）

1. **我自己的手误**（wiki 报的坐标）：`crates/daemon/src/memembed.rs:513` 写成 `if let Ok(ns) = Namespace::parse(...)`，而 `Namespace::parse` 返回 `Option`：

```
error[E0308]: mismatched types
   --> crates\daemon\src\memembed.rs:513:12
    |    ^^^^^^  this expression has type `std::option::Option<Namespace>`
```
→ 已改 `Some`，并立刻复跑 `cargo check -p ruagent-daemon --all-targets`。（wiki 的记录方式我认同：同伴的红不算自己的读数，我的绿也不该被同伴的红污染。）
2. **同伴在途编辑**：同一命令的其余错误在 `crates/daemon/src/distill.rs:879` 与 `:926`（`let (w, _s) = d.write_memories(...)`：`write_memories` 已返回 `MemoryWriteOutcome`，测试仍按元组解构）。**这是 graph 的文件、graph 在途的改动**，我按纪律只把坐标与原文发给他，**没有替他改**，并在当次读数里写明「红来自哪个文件」。同期 `cargo check -p ruagent-daemon --lib` 是**绿**的，用来证明我自己那一半没有残留错误（`Finished dev profile`, 11.40s）。

**结论**：本单三条验收命令现在全绿；`distill.rs`/`wiki.rs`/`knowledge` 的 clippy/编译问题（如有）归各自所有者。
