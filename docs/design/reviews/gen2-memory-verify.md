# V-B 独立验证（t12）：I-B / t8 记忆实现的改前→改后、可证伪性与注入一致性

> **状态**：独立复核**完成**。结论 **pass（带 7 条 finding；其中 2 条是跨区未落地、1 条是陈旧测试期望，均不构成 I-B 自身的失败）**。
> **验证者**：verify2（独立验证员，**不是** I-B 的作者）。本单只写本文件；**未改任何被验代码**（`git status` 见 §11）。
> **时间窗**：2026-09-28T00:5x → 01:5x +08:00。
> **被验产物**：`crates/memory/**`（+`crates/daemon/src/memembed.rs`）与作者报告 `docs/design/reviews/gen2-memory-impl.md`。
> **纪律**：
> * 真守护进程 **pid 79984 未启停**；`~/.ruagent` **只读**（`file:...?mode=ro`），未用 `GET /api/v1/recall`；
> * **绝未为取「改前」读数改动共享工作树**（无 `git stash`、无 checkout 旧文件）：改前一律用 **`git archive HEAD` 导出树** `%TEMP%\v12-head` + 另一个 `-TargetDir`；当前树用 **robocopy 快照** `%TEMP%\v12-tree`（我的探针模块只加在这两个临时树里，仓内一行未动）；
> * 编译**全部**走 `scripts/cargo-team.ps1`（共享 target、一次一个编译、0–11 核、BelowNormal），先 `-DryRun` 核过形状；**没有两条 cargo 并发**（一次等待期见 §3.1）。

---

## 0 一句话

I-B 交付的**每一条 C 目标读数我都独立复现了**，包括 captain 01:4x 改单新要求的 **C1「真实写入路径可达 `<0.5`」与 C4「真实蒸馏正例」**（两条都**复现成功**，未落在 not_measured 上）：真实蒸馏在**我的临时 root** 里产出 `run_turn` episode、`source_episode` 全程非空、接口把该行派生为 `distilled:true`，置信度 **0.4 原样入库**。注入契约两条路径的**块头与截断词表逐字相同**（我自己跑出来：`chat_headers == run_headers`、`chat_markers == run_markers`）。新判据在**改动前的代码上跑红**（我自己的反例模块：**4 failed / 0 passed**；改前基线 **28 passed**）。7 条 finding 里没有一条是 I-B 交付面的功能缺失：2 条是**写入侧接线**没落地（C2 的 `judge_merge`、C-INT-1 的 `memory_write` 诚实 400），1 条是**别人文件里的陈旧期望**（e2e 还在找 t347 已删的 `[distilled] ` 正文前缀），其余是口径/仪器/行为观察。

---

## 1 待验目标表

来源：`gen2-memory-spec.md` §C（C1–C7）+ §C-INT（C-INT-1/2）+ captain 2026-09-28 的改单第 5 条。

| # | 目标（压缩） | 责任半边 | 本单判据 |
| --- | --- | --- | --- |
| C1 | `<0.5` 可达且由具名信号决定；12 条场景 ≥3 个不同值；注入面 `<0.5` 加 `[unverified]` | 我（mem-core）+ **graph 半边**（distill 去 clamp） | 模块读 + **真实写入路径读**（改单第 5 条）+ 边界/穷举 |
| C2 | 有界候选面 + 具名判据 + 可审计决策（`op=merge_judged`） | 我 + **graph 半边**（distill 调 `judge_merge`） | crate 读 + 自己造候选 + **写出路径是否生效** |
| C3 | 使用状态写回；排序含指数衰减+强化；不删 | 我（+ I-SCHEMA 的列） | crate 读 + 我自己的衰减点 + **判据④逐字复核** |
| C4 | 巩固入口幂等 + 来源双向可查 + **一次真实蒸馏产出正例** | 我 + **graph 半边**（run_turn episode）+ C-INT-2 | crate 读 + **真实蒸馏（临时 root，两条路径）** |
| C5 | purge 后内容真消失 + 审计不含量 + 残余面三态 | 我（知识/wiki 六面归 I-A/I-D） | crate 读 + 我自己造的内容 |
| C6 | 每条 render 有可读账目；`TAG_GRAPH`/`tag_rank`；`[unverified]`；两路径一致 | 我 + graph（图块内容） | crate 读 + **两路径 e2e** + 盘上普查 |
| C7 | 未知 store 读路径报错（写路径 400 归 C-INT-1） | 我 | crate 读 + 我的解析矩阵 |
| C-INT-1 | `memory_write` 未知 store 诚实 400，且无写入副作用 | **INT/t19** | **只记录状态**（本单不据此判 I-B） |
| C-INT-2 | `distilled` 只由 `episodes.kind='run_turn'` 派生 | **INT/t19** | 记录 + 与 C4 的正例联读 |
| 改单#5 | C1/C4 不再按 not_measured，必须在**自己的临时 root** 复现 | 我 | **两条都复现成功**（§3.4、§3.5） |

---

## 2 环境与构建面（「通过」覆盖了哪些 target）

| 命令（全部经包装脚本） | 编译面（target） | 结果 |
| --- | --- | --- |
| `... cargo-team.ps1 test -p ruagent-memory` | `ruagent-memory` **lib test** + **doc-tests**；依赖面：core/store/rusqlite/chrono/serde/sha2/thiserror/tracing/tokio/proptest | **exit 0**，`54 passed; 0 failed; 0 ignored` |
| `... test -p ruagent-memory -Nocapture` | 同上 | 全部 `READING …` 行由我捕获（§3.2） |
| `... test -p ruagent-memory -TargetDir %TEMP%\ruagent-cmp-t12-head`（**改前树** = `git archive HEAD` → `%TEMP%\v12-head`） | 同上的 lib test + doc-tests | **exit 0**，`28 passed`；`-- --list` 得 28 个测试名 |
| 同上 + 我的反例模块 `vb12_counter`，`--lib vb12_counter -Nocapture` | 同上 | **4 failed / 0 passed**（§4.1） |
| `... test -p ruagent-memory --lib vb12_sample -Nocapture -TargetDir %TEMP%\ruagent-cmp-t12-now`（**当前树快照** `%TEMP%\v12-tree`） | 同上 | **exit 0**，`14 passed; 0 failed`（我自己的采样，§5） |
| `... test -p ruagent-daemon --lib hedged -Nocapture` | `ruagent-daemon` **lib test**（含 `distill.rs`/`api.rs` 的测试；含 knowledge/lancedb 依赖面，共享缓存命中） | **exit 0**，1 passed（**C1 真实写入路径**，§3.4） |
| `... test -p ruagent-daemon --lib the_distilled_marker_comes_from_the_episode_kind -Nocapture` | 同上 | **exit 0**，1 passed（C-INT-2 派生规则） |
| `... test -p ruagent-daemon --test injection_e2e -Nocapture` | `ruagent-daemon` **integration test** `injection_e2e` | **exit 0**，`7 passed; 0 failed`（**两路径一致性**，§3.6） |
| `... test -p ruagent-mock-agent --test e2e_daemon` | `ruagent-mock-agent` **integration test** `e2e_daemon` | **exit 1**，`22 passed; 1 failed`（陈旧期望，§6 F5） |
| `git status --porcelain -- crates panel` | — | **非空，但没有一行是我的**：`?? docs/design/reviews/gen2-memory-verify.md` 是我唯一新增；`crates/panel` 的 M/?? 全是同伴的（被验面 `git diff --numstat HEAD -- crates/memory crates/daemon/src/memembed.rs` = dedupe 308/5 · inject 511/15 · lib 41/3 · lifecycle 539/46 · query 170/7 · write 31/0 · memembed 419/9，与 t8 报告的 changedPaths 一致） |

**一处等待**：包装脚本在我的一次调用里打印过 `another team build is running … waiting 10s`（5 次，随后拿到锁并正常完成）——按纪律**没有重试、没有另起一条**。

---

## 3 独立读数

### 3.1 目标命令读数（作者：53 passed）

**我的读数：`cargo test -p ruagent-memory` = 54 passed / 0 failed / 0 ignored（exit 0）**。
差 **1** 且可解释：`inject::tests::an_unknown_edited_state_never_renders_as_false` 是 **t24（R-O1 修复：`WikiLeadMeta.edited` → `Option<bool>`）** 新加的测试；它在我跑的时刻已在树里，t8 报告写于 t24 之前。**不是** t8 少报一条。

### 3.2 逐条 READING（我自己 `-Nocapture` 跑出来的原文）

```
READING C1 hedged -> 0.4
READING C1 rule_values=[0.4, 0.8, 1.0] LOW_CONFIDENCE=0.5
READING C1 12-fact scenario: values=[1.0, 1.0, 1.0, 1.0, 1.0, 0.8, 0.8, 0.8, 0.8, 0.4, 0.4, 0.6] distinct=[0.4, 0.6, 0.8, 1.0]
READING C2 lexical verdict on the 0.9884 pair: Refused("a content word is in the difference")
READING C2 decision on the same pair: NeedsJudgement { candidate: 5, cosine: 0.9884 } | audit=candidates=1 rule=embedding_scope verdict=needs_judgement
READING C2 polarity at cosine 0.99: Refused("a polarity token is in the difference")
READING C2 content swap below tau: New
READING C2 bounded candidates top3 ids=[2, 4, 1]
READING C2 scope_tau(p0.99) on the measured landmarks = 0.9884 (median 0.85)
READING C2 audit reason: candidates=3 rule=lexical_ignorable verdict=merge
READING C3 fresh+used=0.4998 stale+unused=0.0587 stale+used=0.3498
READING C3 NULL row score=0.0587 vs zero-use row 0.0587
READING C3 access_count=Some(2) last_used_at=Some("2026-09-28T…+00:00")
READING C4 first=2 second=0
READING C4 key(k1)=b9600d93baa46f01 k1==k2 true k1==k3 false
READING C4 session_memories = 2
READING C5 before purge: memories=Readout(hits 1…) fts=Readout(hits 1…) external=6 episodes=Some(NotAvailable("episodes has no column linking a memory's hash to a transcript…"))
READING C5 after purge: memories=Readout(hits 0…) fts=Readout(hits 0…)
READING C5 derived surface: Readout(ResidualCount { hits: 1, … }) [ResidualHit { surface: Derived, key: "memory/2", … }]
READING C6 tag ranks: profile=0 memories=1 knowledge=2 graph=3 wiki=4 project=5 unknown=6
READING C6 low-confidence long line: len=168 tail="xxxxxxxxxx\n… [+3927 chars truncated]\n</relevant_memories>\n"
READING H-4 order by rank score (before): ["a: body of a", "b: body of b", "c: body of c"]
READING H-4 order by relevance (after):   ["c: body of c", "b: body of b", "a: body of a"]
READING O-1 edited=None -> … edited=unknown | Some(false) -> … edited=false | Some(true) -> … edited=true
READING C7 all_memories on a bogus-store row: Err(Sqlite(FromSqlConversionFailure(1, Text, Custom { kind: InvalidData, error: "unknown memory store `bogus`" })))
```
⇒ C1/C2/C3/C4/C5/C6/C7 的 crate 级读数与作者报告**逐位一致**（含 `[unverified]` 前缀在截断后的存活）。

### 3.3 我自己的边界与穷举（`%TEMP%\v12-tree`，14 passed）

```
V-B C1 exhaustive boolean combinations -> distinct [0.4, 0.8, 1.0]
V-B C1 explicit escape hatch: [0.0, 0.2, 0.4999, 0.5, 0.9, 1.0, 1.0, 0.0]     (clamp 到 [0,1]，没有向上地板)
V-B C1 boundary: is_low(0.5)=false is_low(0.4999)=true rules=[0.4, 0.8, 1.0]
V-B C1 hedged long row: contains_mark=true chars=1092 | 确认行无标记
V-B C7 try_parse: profile/observation/procedure/lesson ✓；bogus / "" / "PROFILE" / "profile " / "observations" / "user" / "null" → 全部 None
V-B C3 my own decay curve (3 uses): [(0.5,"0d"), (0.4911,"1d"), (0.4412,"7d"), (0.2926,"30d"), (0.0007,"365d")]  (单调、恒 >0)
V-B C6 budget: per_block=1024 total=4096 | tag ranks 严格递增 | 5×1000 字块 → chars=3219 ≤ 4096、末块 <wiki> 缺席、`dropped` 有计数
V-B C6 short line: truncated_mark=false len=155 | body==per_block: truncated_mark=true len=1090
V-B C2 scopes: user=[1] project=[2] profile=[3]  (三份同内容互不串)
V-B C4 my key=fa0d8f0f… permuted+dropped-dup 同值；record_consolidation first=2 second=0；forward=[(episode,9),(memory,5)] backward=[1]
V-B C5 我的内容 purge 前 memories=1/fts=1 → purge 后 0/0；6 个外部面全 NotAvailable；报告 2087 字全是标识符、无内容
```
**C1 的一个口径观察**：12 条场景的 4 个不同值里，`0.6` 来自 `explicit(0.6)`（调用方自带值的逃生口），**具名信号本身只产出 3 个值**（`rule_values=[0.4,0.8,1.0]`）——规格要求「≥3 个不同值」**恰好满足**，但读者不该把 0.6 当作第四个信号。

### 3.4 改单第 5 条 · C1：`<0.5` 在**真实写入路径**可达 —— **复现成功**

```
$ ... cargo-team.ps1 test -p ruagent-daemon --lib hedged -Nocapture
READING confidence(用户可能偏好简体中文。) = 0.4
READING confidence(用户使用双屏显示器。)   = 0.8
test distill::t329_tests::a_hedged_confidence_is_not_silently_raised_to_the_floor ... ok
```
该测试走的是产品自己的 `Distiller::write_memories`（真实写入路径，临时 root = `%TEMP%\ruagent-t329-t9-confidence-<pid>-<n>`）。我**在 crate 之外**用 Python 只读打开它留下的库（`%TEMP%\ruagent-t329-t9-confidence-80924-0\ruagent.db`）：

```
memories: (1,'profile','user','用户可能偏好简体中文。', 0.4, source_episode=1, live)
          (2,'profile','user','用户使用双屏显示器。',   0.8, source_episode=1, live)
below_0.5 = 1
episodes: (1,'run_turn', …, source_run='ruagent:t9-conf', content='[t9] signals')
bodies starting with '[distilled]': 0
```
⇒ **C1「真实写入路径可达 `<0.5`」成立**（0.4 原样入库，未被抬到 0.5）。

### 3.5 改单第 5 条 · C4：真实蒸馏正例 —— **复现成功**（两条路径）

**(a) 写入路径（同 §3.4 的库）**：`episodes.kind='run_turn'` **1 行**；`memories JOIN episodes ON kind='run_turn'` = **2/2**；`source_episode IS NULL` = **0**；指向非 `run_turn` episode 的记忆 = **0**。
**(b) HTTP 端到端（我跑 `e2e_daemon` 的真实蒸馏）**：
* JSON 来自我**单测跑**（`distill_graph_toggle_controls_entity_extraction`）的断言消息 —— 也就是 captain 引用的那一串：
```
{"memories":[{"id":1,…,"content":"用户喜欢深色主题","confidence":0.9,…,"source_episode":1,"distilled":true}],
 "total":1,…}
```
* 我随后只读打开**同一个测试**在**全文件跑**里留下的临时 root（`%TEMP%\ruagent-e2e-61156-1`；两次跑的产物形状相同，两次都跑的是同一段代码）：
  schema_migrations max = 24
  episodes by kind: [('run_turn', 1)]
  memories: [(1, '用户喜欢深色主题', 0.9, 1)]
  JOIN run_turn = 1
  distill_log rows: [('ruagent:48e761e2aa5f8379', 1, 'ok', None)]
```
⇒ **C4「一次真实蒸馏产出正例」成立**：1 个 `run_turn` episode + 1 行带 provenance 的记忆 + `distill_log.status='ok'`；且**没有任何一行缺 provenance**（`source_episode IS NULL = 0`）。
**(c) C-INT-2 派生规则**（记录，非 I-B 判据）：
```
READING t350: run_turn row 1 marked=true | manual row 2 marked=false
```
⇒ 接口按 `episodes.kind='run_turn'` 派生；`manual`（= §7.140 的宽标记，156 行迁移行那一类）**正确地为 false**。

### 3.6 注入契约：两条消费路径（chat / runs）—— 独立复核

**(a) 代码面**（我读的两个调用点，两处都**未**被 t8 改）：
| 路径 | 选择 | 排序 | 渲染 |
| --- | --- | --- | --- |
| `chat.rs:541-581` | `memembed::select_injection_memories(db, Some(embedder), query, None, &CHAT_SELECTION)` | `items.sort_by_key(tag_rank)` | `render_context(&items, &InjectionBudget::default())` |
| `runs.rs:1864-1906` | `memembed::select_injection_memories(db, None, &run_query(task), task.project.as_deref(), &RUNS_SELECTION)` | `items.sort_by_key(tag_rank)`（**同一行**） | `render_context(&items, &InjectionBudget::default())`（**同一行**） |
⇒ 预算单位（**字符**：`per_block=1024` / `total=4096`）、块头词表、可见截断词表、丢块计数**全部来自同一个函数**；两条路径的差异只在**选择**（chat 有 query 腿 `top_n=4/min=0.34` 且无 project 组；runs 无 query 腿、有 project 组），这是 t278 已经登记并测量的差异，不是我这一代的分歧。

**(b) 我自己跑的两路径 e2e（`--test injection_e2e`，7 passed / 0 failed）**：
```
T302 chat_headers=["<user_profile>", "<relevant_memories>", "<knowledge>", "<wiki>"]
T302 run_headers =["<user_profile>", "<relevant_memories>", "<knowledge>", "<wiki>"]
T302 old_chat_only_header="[memory context"   old_header_present chat=false run=false
T309 chat_markers=["… [+577 chars truncated]"]
T309 run_markers =["… [+577 chars truncated]"]
```
⇒ **两条路径的块头逐字相同**、**可见截断的字节序列逐字相同**、退役的 chat-only 块头两边都不在。

**(c) 盘上普查（只读 `~/.ruagent/data/transcripts`，补一条真实语料读数）**：147 个 transcript **全部是 `run-*.jsonl`**；`context_injected` 事件 **80** 个（**run 80 / chat 0**）；带截断标记 **25**；渲染最大 **2458** 字符（`total=4096` ⇒ 从未触上界）；块头出现：`<relevant_memories>` ×26、`<user_profile>` ×25、`<knowledge>` ×1、`<wiki>` ×1（**0 个 `<graph>`**，与「图块内容归 I-C/INT」一致）。
⚠️ **仪器教训（我自己的）**：我第一次用关键字 `"dropped"` 扫盘，得到「1 条丢块通知」——逐字打开后发现那是**记忆正文**里的 "audit_log table must never be dropped"。**盘上真实丢块通知 = 0**。词表必须用冻结形态（`… [+N chars truncated]` / `+N dropped`）而不是裸关键字。

**(d) 丢块计数**：两路径 e2e **没有**造出超预算渲染（两边的渲染都远低于 4096），所以「丢块必有计数」这一条我按**契约层**独立复核：crate 测试 `total_budget_drops_blocks_visibly` 绿，且**我自己造的**5×1000 字块渲染出 `chars=3219 ≤ 4096`、末块 `<wiki>` 缺席、`dropped` 有计数。⇒ 判据成立，但**两条路径各自的「丢块」实例未被 e2e 覆盖**（见 §9 U-3）。

### 3.7 活库（只读）读数 —— 一个必须点名的口径问题

```
schema_migrations max = 18                     ← 活库仍在 0018
memories 列 = [id, store, namespace, content, content_hash, confidence, source_episode,
               supersedes, superseded_at, created_at, updated_at, embedding, embedder, deleted_at]
  ⇒ access_count / last_used_at / valid_from / valid_to **列不存在**
memory_sources 表不存在；distill_log 列 = [session_key, distilled_at, memories_written,
               entities_written, relations_written, agent] ⇒ **无 status/failure_reason/prompt_hash**
live=157，confidence < 0.5 = **0**，distinct=[0.5,0.6,0.7,0.75,0.8,0.9,0.95,1.0]
memory_diffs by op = insert 181 / purge 23 / reject 15 / skip_dedupe 10 / supersede 5 / delete 2 / restore 1
  ⇒ **op='merge_judged' = 0 行**
episodes by kind = [('manual',1), ('mcp_write',16)] ⇒ **run_turn = 0**
memories JOIN episodes(kind='run_turn') = 0
```
⇒ 活库读数是**改前 schema（18）上的空集读数**：`<0.5 = 0` 与 `run_turn = 0` 都**只说明活库还没跑过新代码**，**不说明能力缺失**（这正是 captain 要我区别对待的那件事；能力已由 §3.4/§3.5 的临时 root 正例证明）。同时它也说明：**作者报告里那些「活库」列读数（`access_count` 163/163 NULL、`distill_log.status` 33/33 NULL 等）不可能是活库上的读数** —— 那些列在活库上根本不存在（见 §6 F3）。

---

## 4 可证伪性复核（新判据在**改动前**的代码上跑红）

### 4.1 我自己的反例模块（`%TEMP%\v12-head`，HEAD = `0a39e5b8`）

**改前基线**（同一棵导出树、独立 `-TargetDir`）：`cargo test -p ruagent-memory` = **28 passed**（作者记的「改前 28」我自己复现）。
我另写 4 条**只能编译在旧 API 上**的反例（`crates/memory/src/vb12_counter.rs`，只存在于临时树）：

```
V-B OLD-CODE C1 render: "<relevant_memories>\n[2026-09-27] the user might use xlwt\n</relevant_memories>\n"
V-B OLD-CODE C2 judge on the 0.9884 paraphrase: Refused("a content word is in the difference")
V-B OLD-CODE C6 tag_rank knowledge=2 graph=5 wiki=3
V-B OLD-CODE C7 all_memories on a bogus-store row: Ok([MemoryRow { id: 1, store: Observation, …, content: "a row from nowhere" }])
test result: FAILED. 0 passed; 4 failed; 0 ignored; 0 measured; 28 filtered out
```
⇒ **C2/C6/C7/C1 四条判据在旧代码上跑红**（C1 那条两面都红，见 §6 F7 的说明；C3/C5 在旧代码上是**符号缺席**，我用 grep 的缺席证据记，见下）。

**缺席证据（编译级，弱证据，写明）**：旧 `crates/memory` 里 **`CONF_HEDGED` / `unverified` / `decay_score` / `access_count` / `forget_report` 一个都不存在**（`Select-String` 无命中），且活库/旧 schema **没有 `access_count`/`memory_sources` 列与表** ⇒ C1 的具名信号、C3 的使用状态与衰减、C5 的残余面清点在改前**既无代码也无存储**。

### 4.2 既有的 28 条判据**有没有被改弱**（伪证检查）

* 28 个测试名在改后**全部仍存在**；`git diff HEAD -- crates/memory` 里**被删的 `#[test]`/`#[tokio::test]` 属性 = 0**、**被删的函数 = 0**。
* 全 diff 里**被删的 `assert*` 行只有 2 条**，两条都**没有削弱**：
  1. `assert!(tag_rank(TAG_KNOWLEDGE) < tag_rank(TAG_WIKI));` → 换成 `KNOWLEDGE < GRAPH` + `GRAPH < WIKI`（**更强**，原关系可由传递性推出）；
  2. `assert_eq!(b.source_episode, None, …)` → 同一断言**换行重排**（rustfmt）。
* ⇒ 「新判据」不是把旧测试改到过为止。

---

## 5 独立采样（**我自己的输入**，不是作者的夹具）

`%TEMP%\v12-tree` 里我自己的模块（14 passed / 0 failed，读数见 §3.3）：自造近重复/极性/换主体对、自造候选集（含**有界候选的四种位置**）、自造三份同内容跨 store/namespace、自造衰减点、自造来源集合做幂等、自造待遗忘内容、自造标签与预算边界。**规格里没有的输入**上行为一致；两处**只在作者选的点上不成立**的真相被我的采样抓出来（§6 F4）：

```
V-B C2 (a) mergeable duplicate at rank 1 => Merge { candidate: 1, rule: "lexical_ignorable" }
V-B C2 (b) mergeable duplicate at rank 2 => Merge { candidate: 2, rule: "lexical_ignorable" }
V-B C2 (c) paraphrase at rank 2 => NeedsJudgement { candidate: 1, cosine: 0.999 }   ← 名字指向噪声行
V-B C2 (d) paraphrase at rank 4 (K=3) => NeedsJudgement { candidate: 1, cosine: 0.999 }
V-B C2 polarity precedence WITH an unrelated neighbour: Refused("a polarity token is in the difference")
V-B C2 the SAME genuine pair alone: NeedsJudgement { candidate: 2, cosine: 0.97 }
```

---

## 6 差异表：「作者读数 → 我的独立读数 → 差异」

| 项 | 作者读数（t8 报告） | 我的独立读数 | 差异 |
| --- | --- | --- | --- |
| 测试计数 | 28 → **53 passed** | 改前树 **28 passed**；当前树 **54 passed** | **+1**，来源 = t24 新增的 `an_unknown_edited_state_never_renders_as_false`（我按测试名核对） |
| C1 模块/取值 | `hedged→0.4`；12 条场景 distinct 4；规则值 3 个 | 同；另：8 种布尔组合穷举只产出 `{0.4,0.8,1.0}`；`explicit` 逃生口 clamp 到 [0,1]、**不再有 0.5 地板**；`is_low(0.5)=false` | 无（多一层穷举） |
| C1 12 条场景的 4 个值 | distinct=[0.4,0.6,0.8,1.0] | 同；但 `0.6` 来自 `explicit(0.6)`，**具名信号只有 3 个值** | 口径澄清（规格「≥3」恰好满足）→ 无 finding |
| **C1 真实写入路径** | 报告标 **not_measured（依赖 I-C）** | **0.4 原样入库**（daemon lib 测试 + 我在 crate 外只读该临时 root 库） | **改单#5 回填：不再是 not_measured** |
| C2 crate 判据 | 0.9884 → NeedsJudgement；极性终止；K=3；τ 由分布导出；审计串 | 逐位同；另加我自己的四位置读数 | 无 |
| **C2 写入路径生效** | **not_measured（依赖 I-C/t9）** | `distill.rs:570 → mergeable_target → dedupe::judge`，**无 `merge_decision`/无候选界/无 `merge_judged`**；`memembed::judge_merge` 只有定义、**无生产调用者**；活库 `op='merge_judged'` = **0** | **仍缺**，但原因不是「t9 未落地」而是 **I-C 在 t9 §10 R-3 显式延后 DEP-5**（其报告自述「未落地且没有半接线」）⇒ 见 **F1** |
| C3 衰减 | `0.4998/0.0587/0.3498`；NULL==0；overfetch=3 | 同；我自己的衰减点单调且恒 >0 | 无 |
| **C3 判据④** | 报告只写了「decay_score + record_usage」 | **判据④（`access_count=5,last_used_at=now` 必须先于 `access_count=0,last_used_at=now-30d`）逐字复核**：写时间相同时两者评分**完全相等**（`used=0.500000 unused=0.500000`），排序落到调用方的 id tie-break；`decay_score` 的入参是 `(base, updated_at, now, access_count)`，**`last_used_at` 不在其中且全仓无人读**（只被写） | **差异 ⇒ F2** |
| C3「活读数」 | 「改后 live 库 163 行的 `access_count` 仍全为 NULL」 | 活库 **`access_count` 列不存在**（`schema_migrations max=18`） | **差异 ⇒ F3** |
| C4 巩固/幂等 | `first=2 second=0`；`session_memories=2`；key 规范化 | 逐位同；我用自己的来源集合复现（含重复与换序） | 无 |
| **C4 真实蒸馏正例** | **not_measured（依赖 I-C）** | **成立**：临时 root 上 `run_turn`=1、JOIN=2/2、`source_episode IS NULL`=0；HTTP 端到端 `distilled:true` + `source_episode:1` + `distill_log.status='ok'` | **改单#5 回填：不再是 not_measured** |
| C5 | purge 前 1/1 → 后 0/0；6 面 NotAvailable；报告不含量；LIMIT=20 | 逐位同；我用自己的内容复现；`Derived` 面是**真实读数 0** 而非借口 | 无 |
| C6 标签/标记 | 0..6（graph=3）；`[unverified]` 前缀；wiki lead | 同；我自己的严格递增检查与截断存活检查 | 无 |
| C6 两路径 | 代码读 + crate 测试；第三层（盘上 `<graph>`）not_measured | **我自己跑 e2e**：chat/run 块头与截断词表**逐字相同**；盘上 80 事件全 run、0 chat；0 个 `<graph>` | 无（多一条实跑证据） |
| C7 | `try_parse` + `Err("unknown memory store …")` | 同；我的解析矩阵：4 个合法值 ✓，`bogus/""/PROFILE/"profile "/observations/user/null` 全部 None | 无 |
| C-INT-1（记录） | 归 INT/t19（未动 api.rs） | **仍未落地**：`api.rs:3709` 仍是 `_ => Observation`，且 `api.rs:3730` 仍硬编码 `confidence: 0.9` | **差异 ⇒ F1'（记录，不判 I-B）** |
| C-INT-2（记录） | 机制已实现、0 正例 | 派生规则 `run_turn→true / manual→false` 我实跑复现；**正例已由 §3.5 提供** | 无 |
| 既有判据是否被改弱 | — | 28 条全在；删掉的 assert 只有 2 条且都更强/仅换行；删掉的测试属性 0 | 无 |

---

## 7 findings

> 严重度按「对下游/对规格的可信度」定。**没有一条把 I-B 交付面判成功能缺失**；F1/F1' 是**跨区未落地**（各自 owner 已自述延后），F5 是**别人文件里的陈旧期望**。

**F1（medium，owner = graph / t9）C2 的「在 distill 写入路径上生效」半边未落地，且其「原因」已过期。**
· 复现：`Select-String crates/daemon/src/distill.rs -Pattern 'judge_merge|merge_decision|memembed::merge_candidates'` → **0 命中**；`crates/daemon/src/distill.rs:570` 仍调 `mergeable_target` → `:772` 用 `ruagent_memory::dedupe::judge` 单判；全仓 `memembed::judge_merge` 只有定义（`memembed.rs:504`）**没有调用者**；活库 `SELECT COUNT(*) FROM memory_diffs WHERE op='merge_judged'` = **0**。
· 期望/实际：规格 C2 target ①「候选集 = 同 (store,namespace) 余弦 top-K 且有 `candidate_source=embedding`」、②「`cos ≥ τ_scope` 时具名判定并写审计」——**在生产写入路径上都不生效**（候选面仍是同一个无界全表读，审计 0 行）。作者标 not_measured「依赖 I-C 未落地」，但 **t9 已完成**，I-C 自己的报告 §10 R-3 写的是「**未落地**…本单预算用尽…mem-core 已同意在未落地时把他们的 C2 判 not_measured」。
· 处置建议：C2 的 joint 半边按 **not_measured（原因：写入侧接线被 I-C 显式延后，owner=graph/t9）**；若要判「达成」，需要一次图区的小单把 `mergeable_target` 改成 `memembed::judge_merge` 并对 `NeedsJudgement` 收口。**I-B 自己的半边（决策/带宽/审计/候选查询）已交付且我逐条复现**。

**F1'（medium，owner = INT / t19 = C-INT-1）`memory_write` 的诚实失败仍未落地（记录项，不判 I-B）。**
· 复现：`crates/daemon/src/api.rs:3705-3710` 仍是 `_ => ruagent_memory::MemoryStore::Observation`（`parse_store` 未复用）；`:3730` 仍 `confidence: 0.9` 硬编码。
· 期望/实际：`store=bogus` 的 HTTP 写入仍会以 observation 落库（且 HTTP 路径仍不可能产生 `<0.5`）。按我的任务书**只记录状态**，不据此判 I-B 失败；但它同时意味着 C1 的「第三条写路径」在 t19 落地前仍不可达。

**F2（medium，我方交付面内的判据缺口）C3 的判据④按其字面**不可满足**：`last_used_at` 写了但**没有任何读路径**。**
· 复现（两行，都在 `%TEMP%\v12-tree` 的我的采样模块里，`--lib vb12_sample -Nocapture`）：
  `V-B C3 spec ④ with equal write times: used=0.500000 unused=0.500000 delta=0.000000`；
  并列出 `decay_score` 的入参 `(base, updated_at, now, access_count)`。旁证：`Select-String -Path crates -Include *.rs -Recurse -Pattern 'last_used_at'` 的**唯一读**出现在 `usage.rs` 自己的测试里（生产读路径只有 `updated_at`：`crates/daemon/src/memembed.rs:261` 传 `&m.updated_at`）。
· 期望/实际：规格 C3 判据④以 `last_used_at` 为唯一变量的两条行**必须**前者在前；实际两者评分相同 ⇒ 顺序由 id tie-break 决定（30 天前的 `last_used_at` 不产生任何影响）。旧写入时间下前者胜出，但那是**衰减速率**（`S=1+access_count`）而非「最近使用」在起作用。
· 影响：不是「错了」，而是**「最近被用」这个信号没有进入排序**：`last_used_at` 目前是**只写列**；判据④要么改写成基于 `updated_at` 的口径，要么把衰减的 `Δt` 改成 `now - COALESCE(last_used_at, updated_at)`。**未替作者修**。

**F3（low，口径/引用纪律）报告把「迁移后副本」的读数标成「活库」读数。**
· 复现：只读活库 ⇒ `schema_migrations max = 18`，`memories` **无** `access_count/last_used_at/valid_from/valid_to`，**无** `memory_sources` 表，`distill_log` **无** `status/failure_reason/prompt_hash`。
· 期望/实际：报告 §2-C3 写「改后 live 库 163 行的 `access_count` 仍全为 NULL」，§1 亦以「活库」措辞引用这些列；这些列在活库上**不存在**（它们是 0020 的产物，而活库仍在 0018）。该读数**在迁移后的副本上**成立（t22 的 V-SCHEMA 副本读数：`access_count` 163/163 NULL），但与「活库」是两个对象。**这是引用纪律问题**（同一 DB 行在不同对象上结论相反），建议后续统一写「活库(18 版)」/「迁移后副本」。

**F4（low，行为观察，fail-closed 方向）升级判定的「具名候选」与「终止性极性拒绝」都由**余弦最高的那个候选**决定，而不是由最像重复的那个决定。**
· 复现（我自己的夹具，§5）：(c) 近重复改写排在一条噪声行之后 ⇒ `NeedsJudgement{candidate: 1}` 指向**噪声行**（真重复是 id 2）；(d) 真重复落在 top-K 之外时同样指向噪声行；另有：一条**无关邻居**的差分里含极性词，会让整个决策**终止**为 `Refused(POLARITY)`——而同一对真重复**单独**存在时是 `NeedsJudgement`。
· 期望/实际：方向是 fail-closed（**不会错并**），所以不是数据损坏；但（i）审计串会为一次「非极性」的判定记下 `rule=a polarity token…`，（ii）一个无关邻居可以**抑制**一次本可升级的合并。活库语料上高余弦无关对很常见（A.9：中位 0.85、45.19% ≥0.86），所以这不是理论情形。
· 建议：把「终止」范围收窄到**与 `NeedsJudgement` 同一候选**（即先选最像重复的候选，再对**它**判极性），或至少让审计串带上「终止来自哪个候选」。

**F5（low，别人文件里的陈旧期望；owner = 该测试的 owner）`crates/mock-agent/tests/e2e_daemon.rs:2166` 仍在找 t347 已删除的正文前缀。**
· 复现：`... cargo-team.ps1 test -p ruagent-mock-agent --test e2e_daemon` ⇒ `22 passed; 1 failed`，失败断言原文：
  `distilled memory missing: {"memories":[{"id":1,…,"content":"用户喜欢深色主题",…,"source_episode":1,"distilled":true}],…}`
· 期望/实际：断言期望 `"[distilled] 用户喜欢深色主题"`（**正文前缀**），而 t347/t8 之后 provenance 已在**字段**里（`source_episode=1`、`distilled=true`）。⇒ 这条红**不是**能力缺失，反而是 C4/C-INT-2 的**正例证据**；同时它说明主仓有一条 e2e 是红的（`test --workspace` 会红），会被误读成「蒸馏坏了」。
· 处置：断言应改看 `source_episode`/`distilled`，不该看正文前缀。**未替其改。**

**F6（low，仪器）关键字扫盘会被**正文**骗到。**
· 我第一版探针用裸 `"dropped"` 扫 80 个 `context_injected` 渲染，报「1 条丢块通知」；逐字打开是记忆正文 *"the audit_log table must never be dropped"*。用冻结词表（`… [+N chars truncated]`）重扫 ⇒ **盘上丢块通知 = 0**。这条写给后续验证单：**判据必须用冻结词表，别用自然语言关键字**。

**F7（观察，证据分级）C1 的反例是「两面都红」的那一类。**
· 我的 `c1_the_old_contract_cannot_mark_a_low_confidence_row` 在 **HEAD 与当前树都红**（当前树里 `ContextItem::dated` 同样不带 confidence，只有 `dated_with_confidence` 才带）。它证明的是「旧契约**无路可走**」；「新契约有路」由 `%TEMP%\v12-tree` 里 `Some(CONF_HEDGED)` 的绿读数给出。两条合起来才是完整的红/绿对——**单独引用前一条会把它当成 t8 的失败**。

---

## 8 逐条验收对照

| 验收条 | 我的判据 | 结论 |
| --- | --- | --- |
| 给出 C 节每条目标的独立复现读数（自己的构建树/target dir） | §3.2 crate 读数 + §3.3 我的探针 + §3.4/3.5 真实写入路径 + §3.6 两路径 + §3.7 活库 | **成立**（自建树：`%TEMP%\v12-head`、`%TEMP%\v12-tree`；`-TargetDir` 三个独立目录） |
| 「作者读数 → 我的独立读数 → 差异」表，差异有解释或登记 finding | §6（20 行）+ F1/F1'/F2/F3/F4 | **成立** |
| 可证伪性：自己把判据在改动前代码上重跑并确认失败（附构建树证明） | §4.1（`git archive HEAD` 导出树 + 独立 target dir；`4 failed / 0 passed`；改前基线 28 passed） | **成立** |
| 注入契约两条路径的预算/截断/丢块读数被独立复核 | §3.6（代码面 + 我实跑 `injection_e2e` 7 passed + 盘上 80 事件普查）；丢块计数为契约层（§9 U-3 写明范围） | **成立** |
| 发现缺陷 → finding + 消息给 captain，不替作者修 | §7 七条；已发消息给 captain；`crates/` 一行未改 | **成立** |
| 只写本单报告文件，没改被验代码（git status 证明） | §11 | **成立** |

---

## 9 未测项（写明原因，不写成 0）

| # | 项 | 状态 | 原因 |
| --- | --- | --- | --- |
| U-1 | 活库上的 `<0.5` / `run_turn` / `merge_judged` 正例 | **未测（活库空集）** | 活库 `schema_migrations=18`、`run_turn=0`、`merge_judged=0`；真守护进程不许启停、不许写库 ⇒ 活库正例要等一次重新部署 + 一次真实会话蒸馏。**能力本身已由临时 root 正例证明**（§3.4/§3.5） |
| U-2 | C3 的判据④以 `last_used_at` 为变量的**通过** | **未测（判据不可满足）** | 见 F2：排序不读 `last_used_at`，无法构造出「只有 `last_used_at` 不同、前者在前」的读数 |
| U-3 | 两条路径**各自**的「丢块」实例 | **未测** | `injection_e2e` 的两个驱动都不把 total 预算用满（渲染远低于 4096）；丢块计数由 crate 测试 + 我自己的超预算渲染覆盖。「两路径丢块词表一致」有代码面保证（同一个 `render_context`），但没有两路径各自的超预算实例 |
| U-4 | chat 路径的 `context_injected` 盘上读数 | **未测** | 盘上 147 个 transcript **全是 run**，没有 chat 转写文件（与 R-B A.10「chat 载体是 harness 的 zstd 会话」一致）⇒ chat 侧只能由 `injection_e2e` 与代码面证明 |
| U-5 | 「盘上出现 `<graph>` 块」第三层读数 | **未测** | 生产端（I-C 的图证据内容 + INT 的 `ContextItem` 转换）未落地；盘上 80 个事件里 0 个 `<graph>` |
| U-6 | R-D D.7 的 wiki lead / R-A H-4 的 relevance **生产端** | **未测** | 构造点在 `chat.rs`/`runs.rs`（I-INT/t19）与 I-A/I-D；本单只复核了契约与渲染 |
| U-7 | `memory_write` 的诚实 400 与 MCP 透传（C-INT-1） | **未测/未修** | owner=INT/t19，只记录（F1'） |
| U-8 | 一次**真实**（非脚本 mock）LLM 抽取的 confidence 分布 | **未测** | 需要真实 agent 与 API key；本单的「真实写入路径」= 产品自己的 `write_memories`（含脚本 mock 抽取），与 e2e 同级 |

---

## 10 复现命令（我一个不漏地用了这些）

```powershell
# 0) 环境与形状核对（纪律要求先 DryRun）
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-memory -Nocapture -DryRun

# 1) 当前树：目标命令（作者 53，我 54 = +t24 的一条）
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-memory
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-memory -Nocapture

# 2) 改前树（绝不改共享工作树）：git archive 导出 + 独立 target dir
$dst="$env:TEMP\v12-head"; Remove-Item -Recurse -Force $dst -EA 0; New-Item -ItemType Directory $dst | Out-Null
git archive HEAD | tar -x -C $dst
#   （在 $dst 里）28 passed —— 这是「改前」基线
powershell -NoProfile -ExecutionPolicy Bypass -File <repo>\scripts\cargo-team.ps1 test -p ruagent-memory -TargetDir "$env:TEMP\ruagent-cmp-t12-head"
#   我的反例模块只写进 $dst：crates/memory/src/vb12_counter.rs + lib.rs 追加 `mod vb12_counter;`
powershell ... test -p ruagent-memory --lib vb12_counter -Nocapture -TargetDir "$env:TEMP\ruagent-cmp-t12-head"   # 4 failed / 0 passed

# 3) 当前树快照（robocopy，排除 .git/target/panel/docs）：我自己的采样
#   探针只写进快照：crates/memory/src/vb12_sample.rs + lib.rs 追加 `mod vb12_sample;`
powershell ... test -p ruagent-memory --lib vb12_sample -Nocapture -TargetDir "$env:TEMP\ruagent-cmp-t12-now"    # 14 passed

# 4) 改单#5：C1 真实写入路径 + C4 真实蒸馏正例
powershell ... test -p ruagent-daemon --lib hedged -Nocapture                    # READING confidence(…) = 0.4 / 0.8
powershell ... test -p ruagent-daemon --lib the_distilled_marker_comes_from_the_episode_kind -Nocapture
python %TEMP%\v12_p_c1c4.py       # 在 crate 外只读那个临时 root 的 ruagent.db
powershell ... test -p ruagent-mock-agent --test e2e_daemon distill_graph_toggle_controls_entity_extraction -Nocapture
python %TEMP%\v12_p_e2e.py        # 只读 HTTP e2e 的临时 root：run_turn / JOIN / distill_log

# 5) 注入两路径 + 盘上普查 + 活库只读
powershell ... test -p ruagent-daemon --test injection_e2e -Nocapture            # 7 passed（T302/T309 读数）
python %TEMP%\v12_p_inject.py     # 147 transcript / 80 context_injected / 活库只读

# 6) 边界与「既有判据有没有被改弱」
git status --porcelain -- crates panel
git diff --numstat HEAD -- crates/memory crates/daemon/src/memembed.rs
git diff HEAD -- crates/memory | Select-String -Pattern '^-\s*(assert|debug_assert)'
git diff HEAD -- crates/memory | Select-String -Pattern '^-\s*#\[(tokio::)?test\]' | Measure-Object   # -> 0
python %TEMP%\v12_oldtests.py    # 逐个比对 HEAD 与当前树的既有测试函数体
```

---

## 11 纪律回执

* **只写本单 inScope**：`docs/design/reviews/gen2-memory-verify.md`（`git status` 里我这一轮**没有新增任何 `crates/` 或 `panel/` 的改动** —— 全树唯一属于我的条目是 `?? docs/design/reviews/gen2-memory-verify.md`；被验的 `crates/memory/**` 与 `crates/daemon/src/memembed.rs` 一行未改 —— findings 只登记不代修）。被验面的改动量（**作者**的）：`git diff --numstat HEAD -- crates/memory crates/daemon/src/memembed.rs` = dedupe 308/5 · inject 511/15 · lib 41/3 · lifecycle 539/46 · query 170/7 · write 31/0 · memembed 419/9。
* **没有为「改前」读数动共享工作树**：无 `git stash`、无 `git checkout <old> --`；改前读数来自 `git archive HEAD` 的独立导出树 + 独立 `-TargetDir`；我自己的探针模块只写在那两个 `%TEMP%` 树里。
* **编译**：全部经 `scripts/cargo-team.ps1`（共享 target、单飞锁、0–11 核、BelowNormal），`-DryRun` 核过形状；**没有并发两条 cargo**；一次等待期被发现并按要求等待（§2 末）。
* **真守护进程 pid 79984 未启停；`~/.ruagent` 只读**（`mode=ro`，含 `data/transcripts` 与 `data/ruagent.db`）；未使用 `GET /api/v1/recall`。
* **读数三件套**：每条读数带对象集（当前树/改前树/我的快照/daemon lib/e2e 临时 root/活库只读）、采样面（我的自造输入见 §5；探针文件路径全部给出）、可证伪判据（§4 给红绿两侧）。
* **未测一律写明**（§9，8 项，含原因与 owner），**不许写 0 的地方一处没写 0**（活库空集读数明确标为「空集，不等于能力缺失」）。
* **同伴在途编辑**：`crates/daemon/src/distill.rs`（graph，+480/−35）、`crates/mock-agent/tests/wiki_pipeline.rs`（+965/−139）、`crates/knowledge/**`、`crates/store/**`（t6）、`crates/daemon/src/{wiki,memembed,api}.rs` 等在我复核期间有改动；我按「哪个文件、什么错」报告（F1/F1'/F5），**未替任何人改**。
* **结论**：I-B（t8）交付的 C1–C7 mem-core 半边**独立复现成立**；改单第 5 条的两条（C1 真实写入路径 `<0.5`、C4 真实蒸馏正例）**复现成功**；注入契约两条路径**逐字一致**；7 条 finding 均为跨区未落地 / 口径 / 仪器 / 陈旧期望，**无一条是 I-B 交付面的功能缺失** ⇒ **pass**。
