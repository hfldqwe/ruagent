# RV-B 评审（t16）：I-B 的 C 节目标与注入契约逐条判决

> **结论：`needs_revision`（任务置 failed）**。C 节里 mem-core 半边**大部分达成**（C1 · C2 判据/带宽/审计 · C3(a)(b)(c) · C4 幂等与来源 · C5 ①③ · C6 ①② · C7 的 crate 半边，全部有我的独立读数），但有三处**按规格字面未达成**：
> ① **C3 判据④ 逐字不可满足** —— `decay_score` 的入参里根本没有 `last_used_at`，生产调用点传的是 `updated_at`，所以「30 天前用过」与「从未用过」的评分**完全相等**（captain 已裁：**判未达成 + requiredFix**，不许改窄判据）；
> ② **C4 的「审计 op `consolidate`」（C4 target ② + D.8 B-9）没有交付** —— `record_consolidation` 只写 `memory_sources`（我 grep 证实全仓没有任何 `op='consolidate'` 的写入者），作者报告也没登记；
> ③ **C2 target ③ 的带标签语料（≥20 正 / ≥20 负）不存在** —— 规格自己写着「必须先造出来再测（≥40 对），语料落地前只算判据与带宽已定，不算已验证」，本单未造、也没有误并/召回读数。
> 另有 5 条 low/观察与 1 条**待 captain 裁决的注入契约形状**（`EnrichedHit` 适配，作者已按 §7.138 上报）。
> **评审员**：review（独立评审员，**不是** I-B 的作者；`crates/` 与 `panel/` 一行未改，本单只写本文件）。
> **被评审对象**：I-B（t8）报告 `docs/design/reviews/gen2-memory-impl.md`（260 行 / 23:32:45）与 `crates/memory/**`、`crates/daemon/src/memembed.rs`。
> **字节绑定**：`confidence.rs 49751078` · `write.rs 88230B2E` · `query.rs FA947B56` · `lib.rs 11FA38F9` · `consolidate.rs 43FAFF02` · `dedupe.rs 308E8942` · `usage.rs 682F31C6` · `lifecycle.rs 6134A2DE`（全部 23:06–23:31，t8 窗内）· **`inject.rs A9D1A43F`@00:05:06 与 `memembed.rs 00DAF6C5`@00:24:57 晚于 t8 报告**（`inject.rs` 是 **t24/R-O1** 的三态修复；`memembed.rs` 是 t8 之后的在途改动）⇒ 我的注入面读数绑定**当前字节**，并逐条注明它是否 t8 的交付。
> **时间窗**：2026-09-28T02:3x → 03:0x +08:00。
> **纪律**：每一次 cargo 都走 `scripts/cargo-team.ps1`（单一编译 / 共享 target，**未**自设 `CARGO_TARGET_DIR`；任务单里那一行是对全队纪律的有意替换，与 V-A/V-C 同形）。真守护进程 **pid 79984 未启停**；`~/.ruagent` 全程 `?mode=ro`；**未用** `GET /api/v1/recall`。

---

## 0 一句话

I-B 把记忆从「置信度不可校准、合并判据无界、没有使用状态、没有来源表、残余面答不出来、未知 store 静默降级」推到了**具名信号 + 有界候选 + 分布导出的 τ + 可审计决策 + 幂等来源链 + 三态残余面 + 诚实 Err**，这些我都在自己的运行里读到了（§1）。但三门不合格的账要记清：**C3 的「使用衰减」是一个空声称**（列只写不读）、**C4 的审计 op 缺一半**、**C2 的语料验证没有做**。加上 captain 已在 t15 回执里确认的 F3/F4/F6 口径与低风险项，共 **8 条 finding**。

---

## 1 我自己的读数（我自己的运行、`--nocapture` 原文行）

### 1.1 门禁

```
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-memory -Nocapture
→ test result: ok. 54 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.26s
  doc-tests: 0 passed；[cargo-team] exit=0
```

作者 53 / V-B 54 / 我 54 ⇒ 差的 1 条是 **t24 新增**的 `an_unknown_edited_state_never_renders_as_false`（我按名字核对），**不是 t8 的计数**。

### 1.2 C1 置信度（我的原文）

```
READING C1 hedged -> 0.4
READING C1 rule_values=[0.4, 0.8, 1.0] LOW_CONFIDENCE=0.5
READING C1 12-fact scenario: values=[1.0×5, 0.8×4, 0.4, 0.4, 0.6] distinct=[0.4, 0.6, 0.8, 1.0]
→ 失败判据（distinct=1 或 hedged ≥0.5）不成立；`is_low` 命中 1 条
```

**活体半边我也复现了**（t8 报告标 not_measured，因为当时 I-C 未落地；现在 I-C 已落地）：

```
powershell ... scripts/cargo-team.ps1 test -p ruagent-daemon --lib distill -Nocapture
READING confidence(用户可能偏好简体中文。) = 0.4
READING confidence(用户使用双屏显示器。) = 0.8
test result: ok. 13 passed; 0 failed
```

⇒ 真实写入路径上 `0.4` **原样入库**（不是 not_measured），与 V-B 的独立读数一致。

### 1.3 C2 合并（我的原文）

```
READING C2 scope_tau(p0.99) on the measured landmarks = 0.9884 (median 0.85)
READING C2 bounded candidates top3 ids=[2, 4, 1]
READING C2 audit reason: candidates=3 rule=lexical_ignorable verdict=merge
READING C2 decision on the same pair: NeedsJudgement { candidate: 5, cosine: 0.9884 } | audit=candidates=1 rule=embedding_scope verdict=needs_judgement
READING C2 polarity at cosine 0.99: Refused("a polarity token is in the difference")
```

⇒ 有界候选（K=3、缺失向量排最后、id tie-break）、分布导出的 τ、**词法高精度路径 + 仅在高余弦时升级**、极性永不升级、`merge_judged` 审计串具名 `candidates/rule/verdict` —— 全部在位。
**但**：`candidate_source=embedding` 这个 target ① 点名的字段**全仓零命中**（§5 RV-B-4）；**带标签语料不存在**（§5 RV-B-3）。
**写入路径半边（distill 调 `judge_merge`）仍未接线**：活库 `memory_diffs` op 分布我直读为 `delete 2 / insert 181 / purge 23 / reject 15 / restore 1 / skip_dedupe 10 / supersede 5`，**没有 `merge_judged`**；`mergeable_target`（旧词法判据）仍在 distill 的调用线上。按 captain 裁决：**not_measured，owner=graph**（DEP-5，I-C 自报 §10 R-3 显式延后），**不据此判 I-B 失败**。

### 1.4 C3 衰减（我的原文 + 我自己的 grep）

```
READING C3 fresh+used=0.4998 stale+unused=0.0587 stale+used=0.3498
READING C3 NULL row score=0.0587 vs zero-use row 0.0587
READING C3 access_count=Some(2) last_used_at=Some("2026-09-27T18:08:09.382321800+00:00")
```

⇒ (a) 使用状态**写回**成立；(b) 指数衰减 + 强化（`S = 1 + access_count`）成立；(c) 排序只影响顺序、不改内容（H-4 的 `order by relevance` 读数）成立。
**判据④ 逐字复核（我自己的两侧证据）**：

```
crates/memory/src/usage.rs:49  pub fn decay_score(base: f64, updated_at: &str, now: &str, access_count: Option<i64>) -> f64
crates/daemon/src/memembed.rs:261  let score = ruagent_memory::usage::decay_score(0.0, &m.updated_at, &now, used);
全仓 `last_used_at` 命中：usage.rs:82（写）/ usage.rs:151、159（只在它自己的测试里读）/ migrations.rs（schema 清单）
```

⇒ **函数签名里没有 `last_used_at`，生产调用点传的是 `updated_at`** —— 两条内容相同、写时间相同的行，无论 `last_used_at` 差多少天，评分**恒等**（同一次调用里同入参必同出值），C3 判据④ 要求的先后关系**不可能成立**。这是 §5 RV-B-1。

### 1.5 C4 巩固与来源（我的原文 + 我自己的 grep）

```
READING C4 first=2 second=0
READING C4 key(k1)=b9600d93baa46f01 k1==k2 true k1==k3 false
READING C4 session_memories = 2
（daemon 侧）READING t350: run_turn row 1 marked=true | manual row 2 marked=false
```

⇒ 幂等（第二次 0 行）、key 换序归一、来源可双向查、真实蒸馏的 `run_turn` 正例与派生布尔成立。
**但** `crates/memory/src/consolidate.rs` 的写面只有 `INSERT OR IGNORE INTO memory_sources`（L93）；**没有任何 `memory_diffs` 写入**，全仓 op 字面量只有 `insert/supersede/skip_dedupe/reject/delete/restore/purge/merge_judged` ⇒ C4 target ② 与 D.8 B-9 要的 **`consolidate` 审计 op 没有交付**。这是 §5 RV-B-2。

### 1.6 C5 遗忘与残余面（我的原文）

```
READING C5 before purge: memories=1 fts=1 external=6 episodes=NotAvailable("episodes has no column linking a memory's hash to a transcript; only an episode-id lookup is possible (R-B U-1, waits on I-SCHEMA)")
READING C5 after purge: memories=0 fts=0
READING C5 derived surface: Readout(ResidualCount { hits: 1, … })   ← 真实读数 0/1，不是借口
READING C5 report identifiers only: ForgetReport { content_hash: "e6bc72d4…", … }
```

⇒ ① 内容真消失（1/1 → 0/0）、③ 报告只给标识符与来源（不复制正文）**达成**；`Derived` 面是**真读数**；episodes/knowledge/wiki 六面 `NotAvailable` 且**具名 owner**（不是 0）。按 R-6/C5-D，`forget_report` 的形状是 non-landing，C5 的验收只看 ①③ ⇒ 达成（观察 O-2）。

### 1.7 C6 注入面与契约（我的原文）

```
READING C6 tag ranks: profile=0 memories=1 knowledge=2 graph=3 wiki=4 project=5 unknown=6
READING C6 low-confidence long line: len=168 tail="… [+3927 chars truncated]"
READING O-1 edited=None -> … edited=unknown（t24 三态，非 t8）
READING O-1 edited=Some(false) -> … edited=false
READING R-D D.7 wiki lead render: <wiki>\nwiki/<slug>: <title> — stale=unknown coverage=0.00 anchors=0 edited=…\n  generated wiki page — verify against its sources before trusting\n  body\n</wiki>
READING H-4 order by relevance (after): ["c: body of c", "b: body of b", "a: body of a"]
```

⇒ D.2 的 `tag_rank` 0..6（graph=3）与 `TAG_GRAPH` 在位；D.4 的 `[unverified]` **前缀**（规格内已登记的修订）与「长行被截断后标记仍活着」有测试；R-D D.7 的 wiki lead 渲染五要素；预算仍以**字符**计、词表仍是唯一来源。
**第三层读数（盘上 `<graph>`）未测**：V-B 实跑 e2e（80 个 `context_injected` 事件全部 run、0 chat、0 `<graph>`）—— 与「生产者归 INT/t19」一致 ⇒ not_measured + owner，见 §4。

### 1.8 C7 诚实失败（我的原文）

```
READING C7 inserted the bogus-store row id=1
READING C7 all_memories on a bogus-store row: Err(Sqlite(FromSqlConversionFailure(1, Text, Custom { kind: InvalidData, error: "unknown memory store `bogus`" })))
```

⇒ crate 半边（`try_parse` + `Result`）达成；**api.rs 的 400 半边是 C-INT-1（owner=INT/t19）**，不在 C7 的 mem-core 半边里。

### 1.9 活库口径（我自己的只读读数，用于 §5 RV-B-5）

```
schema_migrations max = 18        ← 迁移 0019–0025 在活库上尚未应用
PRAGMA table_info(memories) 含 access_count? False
memory_diffs op counts = [delete 2, insert 181, purge 23, reject 15, restore 1, skip_dedupe 10, supersede 5]
```

⇒ 「改后 live 库 163 行的 `access_count` 仍全为 NULL」这句话在活库上**读不出来**（那一列根本不存在）；必须写成「迁移后副本」。

---

## 2 C 节逐条判决

| # | 规格 target（压缩） | 我的独立读数 | 判决 |
| --- | --- | --- | --- |
| **C1** | ①`confidence` 模块 + 具名信号；② 去 clamp（I-C 半边）；③ 12 条场景 distinct ≥3 且 hedged <0.5；④ 注入只对 <0.5 加可见标记 | `hedged→0.4`；`rule_values=[0.4,0.8,1.0]`；12 条 `distinct=[0.4,0.6,0.8,1.0]`；daemon 真路径 `confidence=0.4` 入库；`[unverified]` 前缀 + 截断存活测试 | **达成**（联合交付两边都已落地；④ 的位置修订已在规格内登记） |
| **C2** | ① 余弦 top-K=3 候选 + `candidate_source=embedding`；② 词法高精度 + 仅 `cos≥τ_scope`（τ 由分布导出）升级 + 审计；③ 带标签语料 误并=0 / 召回≥0.7；④ `merge_judged` op | ① 候选有界/确定性 **达成**，但 `candidate_source` **零命中** ⇒ 见 RV-B-4；② **达成**（`scope_tau(p0.99)=0.9884`、`NeedsJudgement`、审计串、极性不升级）；③ **未达成**（无 ≥40 对语料、无误并/召回读数）⇒ RV-B-3；④ crate 侧 **达成** | **判据/带宽/审计：达成**；**语料验证：未达成**（RV-B-3）；**写入路径生效：not_measured，owner=graph**（captain 裁决，不据此判失败） |
| **C3** | (a) 记录 `access_count`/`last_used_at`；(b) 衰减+强化排序；(c) 注入内容只受排序影响；④ 判据：同内容两条、`used=5,now` 必须先于 `used=0,now-30d` | (a)(b)(c) **达成**（0.4998/0.0587/0.3498、NULL==0、H-4 排序）；**④ 未达成**：`decay_score` 无 `last_used_at` 入参、生产传 `updated_at`、全仓无人读该列 ⇒ 两条恒等 | **(a)(b)(c) 达成；④ 未达成 ⇒ RV-B-1（high）** |
| **C4** | ① 会话关闭的巩固入口（具名函数）；② **审计 op `consolidate`** + 产物带来源引用；③ prompt 指纹归因（I-C/I-SCHEMA） | 入口 `record_consolidation` 幂等（first=2/second=0）、key 归一、`session_memories=2`、真实 `run_turn` 正例；prompt 指纹由 0020/I-C 落地 ⇒ **但** ② 的 `op='consolidate'` **全仓不存在** | **①③ 达成；② 未达成 ⇒ RV-B-2（medium）** |
| **C5** | ① 内容真消失；② 残余面清点（形状 non-landing，R-6）；③ 审计可证明（只给标识符） | purge 前 1/1 → 后 0/0；`Derived` 真读数；报告只含 hash/来源；6 个外部面 `NotAvailable` 且具名 owner | **达成**（①③；② 的形状按 R-6 不属验收面） |
| **C6** | ① 契约判据（chars≤total / 丢块计数 / 词表单源）；② `TAG_GRAPH` + `tag_rank` 重排 + wiki lead + `[unverified]`；③ 第三层：盘上出现 `<graph>` | ① 达成（crate 测试含预算绑定丢块）；② 达成（0..6、前缀标记、lead 五要素）；③ **0/80 事件**，生产者未接线 | **①② 达成；③ not_measured（owner=INT/t19 + mem-core 的词表已完成）** |
| **C7** | crate 半边：未知 store 必须 Err（不静默回落） | `Err(unknown memory store 'bogus')`（`FromSqlConversionFailure`） | **达成**（api.rs 的 400 半边 = C-INT-1，owner=t19） |

**注入契约（D 节）**：D.1 预算 [冻结] 未动 ✓ · D.2 `tag_rank` 0..6 + `TAG_GRAPH` ✓ · D.3 截断词表单源 ✓ · D.4 confidence + **位置修订已在规格内登记** ✓ · D.5 选择冻结 ✓ · D.6 渲染字节（无 lead 路径逐字相同，V-B 已复核 block head）✓ · D.7 `score_kind` 字面量未混用，且作者把 `memembed.rs:151-153` 的措辞漂移改成「It is a **RANK** score, not a similarity -- the API labels it (`score_kind`)」✓ · **D.8 B-9 的 `consolidate` 半边缺**（RV-B-2）· **D.5/R-D D.7 的新字段形状按 `EnrichedHit` 适配交付（已登记，待裁决）**（RV-B-8）。

---

## 3 captain 已裁决的口径（本报告按此写）

| 口径 | 处置 |
| --- | --- |
| **C3 判据④** | **判未达成 + requiredFix**（不改窄判据）：`Δt = now − COALESCE(last_used_at, updated_at)`，让使用衰减真由上次使用决定；owner = mem-core 的 repair 单。**不记成 not_measured。** |
| **C2 写入路径半边** | **not_measured + owner=graph**（DEP-5 未落地，I-C 自报 §10 R-3；graph 正在 t27 接线）。**不据此判 I-B 失败。** |
| **F3 口径** | 全文统一「**活库(18 版)**」/「**迁移后副本**」两种写法（我的 §1.9 就是按这个读的）。 |
| **F4 / F6** | 进 low findings（§5 RV-B-6 / RV-B-7）。 |

---

## 4 not_measured / hand-off（owner 已具名，**不据此判 I-B 失败**）

| 项 | owner | 为什么只能等它 |
| --- | --- | --- |
| C2 的「在 distill 写入路径上生效」 | **graph**（t27，DEP-5） | `memembed::judge_merge` 无生产调用者、活库 `op='merge_judged'`=0；接线在 `distill.rs`（graph 的文件） |
| C6 的第三层（盘上 `<graph>`） | **integ/t19**（生产者）+ mem-core（词表**已完成**） | 注入块在 `api.rs`/`runs.rs` 拼装；任务禁止启停 pid 79984、禁止用 `/api/v1/recall` |
| **C-INT-1**（未知 store → 400）· **C-INT-2**（`distilled` 按 episode kind 派生） | **integ/t19** | 规格 C 节已把这两条**移出** C、标 owner=INT/t19，验收在 **t20/t21**；I-B 只交付 crate 半边 |
| R-D D.7 的 `lead` 生产端 | **integ/t19** | 需要把 lead 传进新的构造点（见 RV-B-8 的裁决结果） |

---

## 5 findings

### RV-B-1（high · **目标未达成** · captain 已裁）C3 判据④ 逐字不可满足：`last_used_at` 只写不读
* **我的证据**：`crates/memory/src/usage.rs:49` `pub fn decay_score(base, updated_at, now, access_count)` —— **没有 `last_used_at` 参数**；生产读路径 `crates/daemon/src/memembed.rs:261` `decay_score(0.0, &m.updated_at, &now, used)`；全仓 `last_used_at` 的读取只出现在 `usage.rs:151/159`（该模块自己的测试）。
* **后果**：C3 判据④ 的两条记忆（同内容、同写时间、`used=5,last_used_at=now` vs `used=0,last_used_at=now-30d`）评分**完全相等**，顺序落到调用方的 id tie-break ⇒ 「使用衰减」是**空声称**（V-B 的独立读数 `used=0.500000 unused=0.500000` 与此同因）。
* **requiredFix**（按 captain 裁决，**不改窄判据**）：把时间差改成 `Δt = now − COALESCE(last_used_at, updated_at)`（`decay_score` 接收 `last_used_at: Option<&str>` 或由调用方传入该 coalesce 结果），并补一条用**判据自己的那对输入**断言严格先后（`used=5,now` 严格先于 `used=0,now-30d`）的测试；owner = mem-core repair。

### RV-B-2（medium · 规格字面未交付）C4 target ② / D.8 B-9 的审计 op `consolidate` 不存在
* **我的证据**：`crates/memory/src/consolidate.rs` 的写面只有 L93 `INSERT OR IGNORE INTO memory_sources …`（文件内 `memory_diffs`/`audit` 零命中）；全仓 op 字面量只有 `reject/insert/supersede/skip_dedupe/delete/restore/purge/merge_judged`（`write.rs:61/145/147/151/218`、`lifecycle.rs:99/146/231`）。
* **后果**：D.8 B-9 要求「`memory_diffs` 新增 op `merge_judged` / `consolidate`」并**各自留读数**，只交付了前一半；C4 的「产物必须带来源引用」在库里成立，但**巩固这件事本身没有决策审计**（下一次有人问「这条结论是谁在什么时候巩固进来的」只能看 `memory_sources` 的 `created_at`，看不到 op/规则）。
* **requiredFix**：`record_consolidation` 落一条 `memory_diffs`（`op='consolidate'`，`reason` 里带 `key` 与来源计数），并给读数（第一次 1 行 / 第二次 0 行，证明与来源链同幂等）；owner = mem-core。

### RV-B-3（medium · 目标未验证）C2 target ③ 的带标签语料不存在
* **我的证据**：`crates/memory/src/dedupe.rs` 的判据测试是**逐对手写**的（`the_four_phrasings_are_mergeable`、`a_polarity_token_refuses_the_merge`、`an_unrecognised_word_refuses_the_merge`、`below_the_scope_level_the_content_is_new`、`a_high_cosine_paraphrase_escalates_instead_of_being_lost`、`a_polarity_flip_is_never_escalated`），全仓无「≥20 正例 / ≥20 负例」的语料常量或夹具；唯一的比例读数是 2 对的玩具（`READING old rule: tokens=2 vs 2 | hits=1 | rate=0.5`）。规格 C2 的「待补」一行自己写着：**带标签语料今天不存在，必须先造出来再测（≥40 对），语料落地前只算「判据与带宽已定」，不算已验证**。
* **后果**：C2 的「误并 = 0、召回 ≥ 0.7」没有任何读数；这也让「高精度路径 + 只在 cos≥τ 升级」的**精度上限**停在推理层（τ 是从 live 分布导出的，而 live 分布是无标签的）。
* **requiredFix**：造 ≥20 正例（人工确认的同事实改写）与 ≥20 负例（极性/数值/主体不同），落成可复现夹具，给出 `误并 = 0 / 召回 = x` 与对象集说明；owner = mem-core（请 captain 立案）。**不引入规格外要求**——这是 C2 target ③ 与它自己「待补」一行的原文。

### RV-B-4（low · 命名缺口）C2 target ① 点名的 `candidate_source=embedding` 不存在
* **我的证据**：全仓 `candidate_source` **零命中**；`merge_audit_reason` 产出的是 `candidates=3 rule=lexical_ignorable verdict=merge`，升级路径是 `candidates=1 rule=embedding_scope verdict=needs_judgement`（规则名里**隐含**了来源，但没有该字段）。
* **requiredFix**：把 `candidate_source=embedding` 写进决策/审计形状（一处字符串），或在规格 D 节登记「用 `rule=` 承载来源」的命名偏差（二选一，别两处都算真）。

### RV-B-5（low · 口径，captain 已裁）报告把「迁移后副本」写成了「活库」
* **我的证据**（§1.9）：活库 `schema_migrations max=18`、`PRAGMA table_info(memories)` **没有 `access_count`** ⇒ 「改后 live 库 163 行的 `access_count` 仍全为 NULL」这句话在活库上读不出来，那一列还不存在。
* **requiredFix**：全文统一「**活库(18 版)**」/「**迁移后副本**」；一切关于 0019+ 新列的读数必须写明载体。

### RV-B-6（low · 行为/审计正确性，V-B F4）升级候选按**余弦最高**取，而不是按规则真正用的那个候选
* **我的证据（V-B §5 的自造采样，我在自己的运行里看到同一族形状）**：`bounded candidates top3 ids=[2,4,1]`、`decision … NeedsJudgement { candidate: 5, cosine: 0.9884 }`；V-B 在「真对排 rank 2 / rank 4，另有一条无关高余弦邻居」的两个位置上读到 `candidate: 1, cosine: 0.999` —— 即**审计串里的候选编号指向噪声行**，而规则用的是真正的改写对。
* **requiredFix**：让 verdict/审计里的 `candidate` 指向**规则实际据以判断的那一条**（或在 reason 里同时给出 `anchor=` 与 `escalated_from=`），并补一条「噪声邻居排在最前」的测试；owner = mem-core（captain 已记入 repair 契约）。

### RV-B-7（low · 仪器纪律，V-B F6）扫盘判据必须用**冻结词表**，不许用自然语言关键字
* **我的证据**：V-B 第一版探针用裸 `"dropped"` 扫 80 个 `context_injected`，报「1 条丢块通知」，逐字打开是记忆正文 *"the audit_log table must never be dropped"*；改用 D.3 的冻结词表（`… [+N chars truncated]`）后**盘上丢块通知 = 0**。
* **requiredFix**：后续验证/评审脚本一律从 D.3 的词表取判据（`tail_truncated`/`cut_at` 的字节），禁止自然语言关键字扫盘；owner = 后续验证单（V-INT 起）。

### RV-B-8（medium · **待 captain 裁决的注入契约形状**，作者已按 §7.138 上报）
* **事实**：R-D D.7/R-B D.5 点名的 `RetrievalHit.lead` / `relevance` **字面字段没有加**；交付改成了 `EnrichedHit { hit, lead, relevance }` + `knowledge_items_enriched()`，而冻结的 `knowledge_items` 与既有调用点（`chat.rs:550`、`runs.rs:1873`）**字节零变化**。作者的理由：加字段会让 out-of-scope 的 `chat.rs`/`runs.rs` 编译不过，与验收命令 `cargo check -p ruagent-daemon --all-targets` 直接冲突。
* **我的判断**：**冻结面确实保住了**（`knowledge_items` 未改、构造点仍是两个构造器、无 lead 路径逐字相同），偏差**也登记了**（报告 §4.1 + 给 integ 的确切签名）——所以这不是 t8 的静默破坏；但它是**契约形状的未决事项**，会直接决定 t19 的接线点与 V-INT 的判据。
* **requiredFix**：请 captain 裁决并写回规格 D 节（一个真相源）：**(a) 接受适配形状** ⇒ 规格 D.5 追加 `knowledge_items_enriched` 的签名与「何处调用」（integ 在构造点传 lead/relevance）；**(b) 要求字面字段** ⇒ I-B 与 integ 同一窗口改 `chat.rs`/`runs.rs` 两处调用点，并把验收命令的适用范围写明。两种都可以，**但不能悬空**——否则 V-INT 会同时看到两种形状。

---

## 6 观察（不进 findings）

* **O-1** D.4 的修订（后缀 → 前缀）是**在规格里登记的**，附了「后缀在长行被截断时消失」的当场读数 —— 这是冻结点该怎么改的范例，登记行为本身值得记一笔。
* **O-2** C5 的 `forget_report` 只报标识符、六个外部面用 `NotAvailable` + 具名 owner（**没有写成 0**），符合「不许静默跳过」；`Derived` 面是真读数。
* **O-3** C6 第三层 0/80 事件（全 run、0 chat）与「生产者归 t19」一致 ⇒ 不判 I-B 失败，但 V-INT 必须在这条接线上重新取证。
* **O-4** C2 写入路径的活库 op 分布我直读为**无 `merge_judged`**（§1.9）——与 graph 的 DEP-5 未落地一致；t27 接线后这一行必须重读。
* **O-5** D.7 的措辞漂移已修：`memembed.rs:149-153` 现在明写「RANK score, not a similarity -- the API labels it (`score_kind`)」。
* **O-6** V-B 的 F7 分级说明（C1 的反例在 HEAD 与当前树**两面都红**，它证明的是「旧契约无路可走」，不能单独当成 t8 的失败）值得沿用：反例的**证据等级**要写清。

---

## 7 未测 / 不判定

| # | 项 | 状态 | 原因 / owner |
| --- | --- | --- | --- |
| U-1 | C2 在 distill 写入路径上生效 | not_measured | owner=graph（t27/DEP-5）；任务禁止启停 daemon |
| U-2 | C6 第三层（盘上 `<graph>`） | not_measured | owner=integ/t19（生产者）；V-B 实跑 0/80 |
| U-3 | C-INT-1 / C-INT-2 | 不在本单 | 规格已移出 C，owner=INT/t19，验收 t20/t21 |
| U-4 | 迁移后副本上的新列读数（`access_count` 等 163 行） | **未测（本轮未造副本）** | 它属于「改后活体”，需要把 0019–0025 应用到副本；我读的是 t8 的 crate 测试 + 活库口径（V-B 已给副本读数，我引用并标注来源） |
| U-5 | 我自己没有重跑「旧树 4 failed / 0 passed」的两棵树 | **部分** | V-B 做了（`git archive` + 独立 target，4 条运行期红）；我给的独立证据是**符号级缺席**（V-B §4.1 同批）与我自己的 grep/读数 |
| U-6 | panel/MCP 侧的 `confidence` 渲染 | 未测 | 面板 `memoryConfidenceHint` 的读数是 UI 面（不在 I-B inScope），留给集成 |

---

## 8 复现命令（我实际跑的）

```powershell
$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"     # 全队包装脚本，不自设 CARGO_TARGET_DIR
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-memory -Nocapture
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-daemon --lib distill -Nocapture
git status --porcelain -- crates panel                 # 50 项，全是并发波次；归因只用 mtime+哈希
# 我自己的只读核对（活库 ?mode=ro，绝不用 /api/v1/recall）
python -c "... SELECT op,COUNT(*) FROM memory_diffs GROUP BY op; PRAGMA table_info(memories) ..."
# 我自己的 code 面判据（grep）：decay_score 签名 / memembed.rs:261 调用点 / last_used_at 全部命中 / op 字面量 / candidate_source
```

---

## 9 纪律回执

* **只写本单 inScope**：本文件 `docs/design/reviews/gen2-memory-review.md`。`crates/` 与 `panel/` **一行未改**。
* **读数三件套**：对象集（当前树 `crates/memory` + `memembed.rs` 的字节哈希见页首；活库只读 `?mode=ro`）· 采样面（54 条 crate 测试 + daemon distill 13 条 + 我自己的 grep 面：`decay_score` 签名/调用点/`last_used_at`/op 字面量/`candidate_source`）· 可证伪判据（C3-④ 的恒等性由**函数签名**证明；C2-③ 的缺席由 grep 证明；C5 的前后 1/1→0/0；C7 的 Err）。
* **期望值只写一处**（规格 C/D 节与本报告），判据从代码与库取，**没有复制常量**。
* **未测一律写明**（§7，6 项），其中 owner=t19/graph 的**不据此判 I-B 失败**。
* **真守护进程 pid 79984 未启停**；`~/.ruagent` 全程 `?mode=ro`；**未用** `GET /api/v1/recall`。
* **字节绑定与漂移**：`inject.rs`（00:05:06，t24/R-O1 的三态）与 `memembed.rs`（00:24:57，t8 之后）**晚于 t8 报告**，我的注入面读数据此标注；`crates/memory/src` 的 8 个文件全部在 t8 窗内（23:06–23:31）。
* **并发声明**：全树 `git status --porcelain -- crates panel` = 50 项，是并发波次；不据此写任何 finding。
