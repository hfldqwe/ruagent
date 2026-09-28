# RV-B round 2 评审（t32）：t31 的八条修缮逐条判决 + C/D 节重判

> **结论：`pass`**。t27/t31 这一轮把 RV-B 的**八条 finding 全部关闭**，而且每一条我都在**当前字节**上自己读到了读数（§1、§5）。C 节里 mem-core 自有的目标现在**全部有独立读数**：连 C2 的「在 distill 写入路径上生效」这一半也闭合了（graph 的 R-3 接线落地，我实测 `merge_judged rows 0 -> 2 | live=1 superseded=1`）。只剩 **C5 的知识/wiki 残余面**与 **C6 的「盘上 `<graph>`」**两处按 owner=INT/t19 + 依赖未落地判 `not_measured`（**excused**，终判在 t20/t21）。
> **两条 captain 口径的落地核对**：**(1)** 语料按写下来的判据算**达标**（`误并 0/20`、`召回 14/20 = 0.70`），不做「触线不算」的临时加码；弱证据随行 —— 我全 `docs/design/reviews/*.md` 搜 `14/20`，只出现 5 处（§2.3 的两行 + §6 修订 1 的强制连写行 + 两处引用性提及），**全部带限定**，没有「只引 0.70」的地方 ⇒ **不构成 finding**。**(2)** 极性门保持 fail-closed，**我未把「极性行未交给具名判定」写成 requiredFix**（它是 captain 修订 2 的裁决，也是下一代 LLM 收口的落点）。
> **评审员**：review（独立评审员，**不是** I-B/mem-core 的作者；`crates/` 与 `panel/` 一行未改，本单只写本文件；探针在 `%TEMP%`）。
> **被评审对象**：t31 报告 `docs/design/reviews/gen2-memory-repair-r2.md`（161 行 / 02:34，含 captain 的 §6 修订记录）与 `crates/memory/**`、`crates/daemon/src/memembed.rs`、`docs/design/reviews/gen2-memory-spec.md` 的 D.5/E.2 追加块。
> **字节绑定（t31 窗口）**：`usage.rs AC1D667F75E4`@02:14:13 · `consolidate.rs C180EDCE17F1`@02:21:28 · `inject.rs 6E353066BA54`@02:23:21 · `dedupe.rs 7A06996E45F3`@02:26:35 · `memembed.rs 5DA744245199`@02:31:03；`confidence.rs/write.rs/query.rs/lib.rs/lifecycle.rs` 仍是 t8 窗内字节（23:06–23:31）。
> **时间窗**：2026-09-28T02:4x → 03:1x +08:00。
> **纪律**：每一次 cargo 都走 `scripts/cargo-team.ps1`（单一飞行锁 / 共享 target，**未**自设 `CARGO_TARGET_DIR`）。真守护进程 **pid 79984 未启停**；`~/.ruagent` 全程 `?mode=ro`；**未用** `GET /api/v1/recall`。

---

## 0 一句话

这一轮把五处「声称」变成了**可读出的事实**：使用衰减的**时钟**（`last_used_at` 以前只写不读）、巩固**动作**落审计（`op='consolidate'` 以前全仓不存在）、合并判据的**带标签语料**（20+20）、审计里的**实际决定者**（并在同一窗口查出两个真缺陷：极性邻居不再替别的行做决定、极性行不再静默）、以及**载体口径**与**探针词表**的收敛。我按 captain 的七条逐条验过，**没有一条是只看报告就签字的**。

---

## 1 我自己的读数

### 1.1 门禁（我自己的运行，共享 target）

```
scripts/cargo-team.ps1 test -p ruagent-memory -Nocapture  → exit 0
  test result: ok. 59 passed; 0 failed; 0 ignored; 0 measured  (3.21s) + doctest 0/0
scripts/cargo-team.ps1 test -p ruagent-daemon --lib distill -Nocapture → exit 0
  test result: ok. 15 passed; 0 failed; 0 ignored; [61 filtered out]  (0.33s)
```

**计数口径**：我在 t16 实测 **54**（含 t24 新增的那条），本轮实测 **59** ⇒ t31 净增 **5** 条；作者报告写「53 → +6」是按 **t8 基线 53** 算的。两者指向同一个终值 59，只是基线不同（不构成 finding）。

### 1.2 Δt = now − COALESCE(last_used_at, updated_at)（口径项 1/7）

```
READING C3(Δt) used=5,last_used=now -> 0.500000 | used=0,last_used=now-30d -> 0.058660
READING C3 NULL row score=0.0587 vs zero-use row 0.0587
```
（两条的 `updated_at` **相同**，只有使用时钟不同 ⇒ 严格先后成立；与 RV-B-1 里我实测的 `used=0.500000 unused=0.500000` 构成改前/改后对照。）

**代码面（我自己看的）**：`usage.rs:65 pub fn decay_score(…, last_used_at: Option<&str>, …)` · `usage.rs:93 pub fn decay_clock(updated_at, last_used_at)`；生产调用点 `memembed.rs:263-267` 取 `usage_of` 的 `(access_count, last_used_at)` 并传 `last_used.as_deref()`，`usage_of`（`:340`）**两列都读**。⇒ 那条「只写不读」的列现在真的进入了排序。

### 1.3 巩固动作落审计（口径项 2/7）

```
READING C4 first=2 second=0
READING C4 audit after first call: [("consolidate", Some("consolidate key=854f473955ac6ee3214a2f145ee75eaf725075bc143c91ce797d8688fdb0172a sources=2 inserted=2"))]
READING C4 audit rows: first call=1 after a repeat=1
```
⇒ **第一次 1 行 / 重复 +0 行**，`reason` 形状是 `key=<64hex> sources=<给定条数> inserted=<n>`（与 `memory_sources` 同幂等）。

### 1.4 带标签语料（口径项 3/7，captain 修订 1）

```
READING C2 fixture object set: positives=20 negatives=20 tau=0.95 (MergeConfig::default, documented as the live p99 level)
READING C2 lexical-only: recall=0/20 false_merge=0/20
READING C2 decision@0.95: recall=14/20 (=0.70) false_merge=0/20 negative_escalations=0/20 below_tau=2 ([…]) polarity_gated=4 ([…])
READING C2 decision@0.99 (scope_tau of THIS fixture's 40 assigned cosines, p0.99): recall=3/20
```
⇒ **误并 0/20**（判据核心）、**召回 14/20 = 0.70**（按 captain 裁决 = 达标，不做临时加码）。**弱证据随行**：缺口逐条具名（2 条被刻意置于 τ 之下 0.94/0.93、4 条极性门），且 `decision@0.99` 给的是**循环性证据**（从待合并的那批对里取 τ ⇒ 召回掉到 3/20）—— 这条正好证明「τ 必须取 scope 背景分布」不是修辞。
**我自己的核对**：全 `docs/design/reviews/*.md` 搜 `14/20` = 5 处，**全部带限定**（§2.3 读数行与叙述行、§6 修订 1 的强制连写行、两处引用性提及）⇒ 没有「弱证据被当强证据用」的漏点。

### 1.5 `candidate_source` 入审计形状（口径项 4/7）

```
READING C2 audit reason (merge):      candidates=3 candidate_source=embedding anchor=11 escalated_from=none over_tau=none polarity_refused=none rule=lexical_ignorable verdict=merge
READING C2 audit reason (no vectors): candidates=3 candidate_source=none      anchor=11 …
```
**规格侧同步（我自己读的）**：`gen2-memory-spec.md` **E.2 追加块** L897 给出完整键值序列（旧键一个没丢），L901 写明值集 `{embedding, none}` 且唯一来源是 `dedupe::candidate_source` ⇒ **一处真相源，没有第二套说法**。

### 1.6 审计记**实际决定者**（口径项 5/7）

```
READING C2 noise-first: overlap(noise)=5 overlap(paraphrase)=16
READING C2 noise-first decision: NeedsJudgement { candidate: 9, cosine: 0.97 }
   | candidates=2 candidate_source=embedding anchor=9 escalated_from=7 over_tau=7,9 polarity_refused=none rule=embedding_scope verdict=needs_judgement
READING C2 polarity neighbour + mergeable neighbour: Merge { candidate: 6, rule: "lexical_ignorable" }
   | candidates=2 candidate_source=embedding anchor=6 escalated_from=none over_tau=none polarity_refused=4 rule=lexical_ignorable verdict=merge
READING C2 polarity at cosine 0.99: Refused("a polarity token is in the difference")
   | candidates=1 candidate_source=embedding anchor=7 escalated_from=none over_tau=none polarity_refused=7 rule=… verdict=refused
```
⇒ 噪声邻居排在最前时，决策**跟同事实那条（9）**并点名它没跟的高余弦邻居（7）、且把**所有**达 τ 的被拒行列进 `over_tau=7,9`。**顺带修掉的两个真缺陷我也读到**：一条带极性词的邻居**不再**让整条决策变 `Refused`（跨行合并成立，`polarity_refused=4` 仍被记录）；极性行**永不升级**但出现在 `polarity_refused` 里。
**边界随行**：作者自述该修法**不提高**夹具召回（14/20 前后一致）—— 与我的读数一致，它改的是**跨行**行为。

### 1.7 D.3 词表四函数（口径项 6/7）

```
inject.rs:41 pub fn tail_truncated(dropped_chars: usize) -> String
inject.rs:47 pub fn cut_at(what: &str, bound_chars: usize) -> String
inject.rs:56 pub fn items_dropped_notice(dropped_items: usize) -> String
inject.rs:63 pub fn items_dropped_minimal(dropped_items: usize) -> String
调用点：inject.rs:646 / :650 —— 两者都在 pub fn render_context(items, budget)（:588）的函数体内
READING D.3 vocabulary: "… [+7 chars truncated]" | "… [upstream result truncated at 500 chars]" | "<context_budget>\n… [+3 items dropped: context budget reached]\n</context_budget>\n" | "… [+3 dropped]"
READING D.3 drop notice as emitted: "… [+2 dropped]"
```
⇒ **后两个由渲染器本身调用** ⇒ 探针问到的字节与渲染器发出的字节**不可能漂移**（这正是 RV-B-7 的 requiredFix）。

### 1.8 载体口径（口径项 7/7，我自己的只读探针 `%TEMP%\rvb2_carrier.py`）

```
[carrier] schema_migrations max = 18
[carrier] memories columns = 14 · access_count=False · last_used_at=False · valid_from=False
[carrier] memory_sources table = 0
[carrier] memories rows = 163
[carrier] memory_diffs ops = [delete 2, insert 181, purge 23, reject 15, restore 1, skip_dedupe 10, supersede 5]
[carrier] consolidate present = False · merge_judged present = False
```
⇒ 与 repair §0 的载体表**逐位相同**：**活库(18 版)** 上那些列/表/op 根本不存在 ⇒ 「NULL」这个词只能写在**迁移后副本**上。约定 + 判据 + 读数表 + t8 那句的更正块都在 r2 §0。

### 1.9 RV-B-8 裁决 (a) 的落地（我读规格原文）

`gen2-memory-spec.md` **D.5 追加块**（L797–826，只追加、前文一字未动）包含 captain 要的四件：**(i) 签名** `EnrichedHit{hit, lead: Option<WikiLeadMeta>, relevance: Option<RelevanceMeta>}` + `plain()` + `order_value()` + `knowledge_items_enriched(…)`，旧 `knowledge_items()` 是薄包装；**(ii) 逐字节冻结面**（`RetrievalHit` 字段语义 · `knowledge_items()` 输出 · `render_context()` 块头/标记/丢块文案与**字符**预算 · `InjectionBudget{1024,4096}` · D.3 四函数）；**(iii) 唯一适配器** `memembed::{lead_meta, relevance_meta, enriched_hit}`（`lead_for` 三态保真、`RankedHit.relevance` 来源、`ScoreKind::as_str()` 不复写拼写）；**(iv) 调用点** `chat.rs:569` / `runs.rs:1893` 归 t19，判据 **「未富集调用点 = 0」**。⇒ **不悬空。**

### 1.10 C2 的写路径半边（我在本轮重跑，不再引用 t28）

```
READING R-3: merge_judged rows 0 -> 2 | written=2 skipped=0 | live=1 superseded=1
   audit: before=None reason=Some("candidates=0 candidate_source=none anchor=none escalated_from=none over_tau=none polarity_refused=none rule=no_candidate verdict=new")
   audit: before=Some("id=1") reason=Some("candidates=1 candidate_source=embedding anchor=1 escalated_from=none over_tau=none rule=lexical_ignorable verdict=merge")
```
⇒ RV-B round 1 里判 `not_measured`（owner=graph）的那半边**已经闭合**：真实写入路径上 `op='merge_judged'` 从 0 变 2，且**一次真合并**（`live=1 superseded=1`），审计键序列与 1.5/1.6 的新形状一致。

---

## 2 C 节逐条重判

| # | 规格 target（压缩） | 我的独立读数 | 判决 |
| --- | --- | --- | --- |
| **C1** | 具名信号 + `hedged ≤0.4` + 12 条场景 distinct ≥3 + 注入只对 <0.5 加标记 | `hedged -> 0.4` · `rule_values=[0.4,0.8,1.0]` · 12 条 `distinct=[0.4,0.6,0.8,1.0]` · 真路径 `confidence=0.4`（t28 我实测）· `[unverified]` 前缀 + 截断存活 | **达成** |
| **C2** | 有界候选 + 具名判据 + `merge_judged` + 语料 | ① `candidates=3 candidate_source=embedding` · ② `scope_tau(p0.99)=0.9884` + 审计键序列 · ③ `误并 0/20`、`召回 14/20=0.70`（触线、限定随行，captain 裁达标）· ④ 库侧 `first=1/重复 +0` + **写路径 `merge_judged 0→2`（我本轮实测）** | **达成**（+ RV-B2-1 具名局限） |
| **C3** | 使用状态 → 衰减排序（不删） | (a)`access_count=Some(2) last_used_at=Some(…)` (b)`0.4998/0.0587/0.3498`、`NULL==0` (c) H-4 排序 (d) **`0.500000` vs `0.058660`** | **达成** |
| **C4** | 巩固入口 + 审计 op + 来源引用（I-C/I-SCHEMA 半边） | `first=2 second=0` · **audit `consolidate` 首 1 行/重复 +0** · key 归一 · `session_memories=2` · 真 `run_turn` 正例（V-B 与 t28 我的读数） | **达成** |
| **C5** | 内容真消失 + 残余面 + 审计只给标识符 | purge 前 `1/1` → 后 `0/0`；`Derived` 真读数；`ForgetReport` 只含 hash/来源；6 个外部面 `NotAvailable` 且**具名 owner**（不写 0） | **达成**（①③；② 的形状按 R-6/C5-D 是 non-landing） |
| **C6** | 契约判据 + `TAG_GRAPH`/`tag_rank` + `[unverified]` + lead 渲染；第三层 `<graph>` | `tag ranks profile=0 … graph=3 … unknown=6` · `[unverified]` 长行截断存活 · wiki lead 五要素 · D.3 四函数 | ①②**达成**；③ **not_measured**（生产注入点在 `api.rs`/`runs.rs`，owner=INT/t19） |
| **C7** | 未知 store 必须 Err（不静默回落） | `Err(Sqlite(FromSqlConversionFailure(… "unknown memory store \`bogus\`" )))` | crate 半边**达成**；api 半边 = **C-INT-1**（owner=INT/t19） |
| **C-INT-1/2** | 未知 store 400 / `distilled` 按 kind 派生 | 不在 I-B 的 C 节（规格已移出） | **not_measured**（owner=INT/t19，验收 t20/t21） |

---

## 3 findings

### RV-B2-1（low · **具名局限**，owner = 下一代）带标签语料是**实现者自造**的，且召回恰好触线
* **事实**：`POSITIVE_PAIRS`/`NEGATIVE_PAIRS`（20+20）由 mem-core 自己写在同一文件里，每对带一个 **assign 的余弦**（正例 0.93–0.99 / 负例 0.55–0.94，按活库地标取）⇒ 本夹具测的是**给定相似度证据下的决策规则**，不是 embedder，也不能替代 I-A 的活库 gold；`召回 14/20 = 0.70` **恰好等于**规格门槛，没有余量。
* **处置（按 captain 修订 1/2）**：**按写下来的判据算达标**，不加码、不判 needs_revision；弱证据**必须随行**（§2.3 的两行与 §6 修订 1 的强制连写行已带上「恰好触线 / 夹具自造 / 6 条缺口具名 / 从夹具自身取 τ ⇒ 3/20」）；**极性行不交给具名判定**（保持 fail-closed，那是下一代 LLM 收口的落点）。
* **requiredFix（下一代）**：立**独立**语料（先冻结、再给别人用），并把 τ 继续取自 scope 背景分布而非待判定集合；在此之前，任何引用这一行的地方都必须连写限定词（我本轮已核对：`docs/` 里 5 处 `14/20` **全部带限定**）。

### 观察（不进 findings）
* **O-1** 载体口径的**残留面**：t8 的终态报告里那句写错载体的话仍在（按 captain 修订 3，「以 stored 契约与终态记录为准」，更正块放在 r2 §0）。⇒ 建议把「更正指针」放在**规格/任务单**一侧，而不是改终态报告正文。
* **O-2** 作者自报的**假绿教训**（`clippy … -- -D warnings` 的尾参不进 cargo 指纹 ⇒ 先前的无 `-D` 编译会让它命中缓存）：daemon 面 clippy 从今一律带 `-CleanFirst`。这是可复用的团队规则。
* **O-3** 跨行极性修法的边界（作者已自述、我读数一致）：它**不提高**夹具召回，改的是「一条极性邻居不再替其它行决定」；记录以免被读成召回改进。
* **O-4** 计数口径：t8 53 → 我的 t16 实测 54（含 t24）→ 本轮 **59**（t31 净增 5）；作者的「+6」按 t8 基线算，终值一致。

---

## 4 关闭表：RV-B 的八条 finding（改前 → 我的复现 → 判）

| # | round 1 的读数 | 我本轮的复现 | 判 |
| --- | --- | --- | --- |
| **RV-B-1**(high) C3 判据④：`last_used_at` 只写不读 | `decay_score` 无该入参、生产传 `updated_at`；两条恒等（`used=0.500000 unused=0.500000`） | `READING C3(Δt) used=5,last_used=now -> 0.500000 \| used=0,last_used=now-30d -> 0.058660`；签名与调用点见 §1.2 | **关闭** |
| **RV-B-2**(medium) `op='consolidate'` 全仓不存在 | 全仓无写入者；只写 `memory_sources` | `audit after first call: [("consolidate", key…sources=2 inserted=2)]`，`first=1 / 重复 +0` | **关闭** |
| **RV-B-3**(medium) C2 target ③ 无语料 | 无 ≥40 对夹具、无误并/召回读数 | 20+20 夹具 + `误并 0/20`、`召回 14/20=0.70`、`decision@0.99 ⇒ 3/20`（循环性证据） | **关闭**（+ RV-B2-1） |
| **RV-B-4**(low) `candidate_source` 零命中 | 审计串没有该键 | `candidate_source=embedding \| none` 入审计串；规格 E.2 追加块 L897/901 同步 | **关闭** |
| **RV-B-5**(low) 载体口径混写 | 「活库 … 全为 NULL」而活库无此列 | r2 §0 约定 + 判据 + 读数表；我的只读探针与它逐位相同（18 版/14 列/无表无 op） | **关闭** |
| **RV-B-6**(low) 审计指向余弦最高而非实际决定者 | `NeedsJudgement{candidate:1, cosine:0.999}` 指向噪声行 | `anchor=9 escalated_from=7 over_tau=7,9`；并修掉两个真缺陷（跨行极性、极性行记录） | **关闭** |
| **RV-B-7**(low) 扫盘判据被正文骗到 | 裸 `"dropped"` 命中记忆正文 | D.3 四函数公开、后两个由 `render_context` 自身调用（`inject.rs:588/646/650`）；测试打印四形态 + `as emitted` | **关闭** |
| **RV-B-8**(medium) 注入契约形状未决 | 字面字段未加、适配形状待裁决 | captain 裁 (a)；规格 **D.5 追加块**四件齐（签名/冻结面/唯一适配器/调用点 + 「未富集调用点=0」） | **关闭** |

---

## 5 未测 / 不判定（owner 具名，**不据此判 I-B 失败**）

| # | 项 | owner | 为什么只能在它之后测 |
| --- | --- | --- | --- |
| U-1 | C6 第三层：盘上 `<graph>` 块 | **INT/t19** | 生产注入点在 `api.rs`/`runs.rs`；任务禁止启停 pid 79984、禁止用 `/api/v1/recall` |
| U-2 | C5 的知识三面与 wiki 三面（残余面活库读数） | **I-A/recall + I-D/wiki** | 依赖它们的残余面接口；**活库(18 版)** 也没有那些表 |
| U-3 | C-INT-1（未知 store 400）· C-INT-2（`distilled` 派生） | **INT/t19** | 规格 C 节已把这两条移出 C；验收 t20/t21 |
| U-4 | `knowledge_items_enriched` 的两个真实调用点 | **INT/t19** | 构造点在 `chat.rs`/`runs.rs`；判据已写进 D.5 追加块（「未富集调用点 = 0」） |
| U-5 | 活库上的新 op / 新列读数 | **运维/跑批** | 活库是 18 版；本单所有新 op 读数都来自内存库或迁移后副本（载体已写明） |

---

## 6 复现命令（我实际跑的）

```powershell
$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"     # 全队包装脚本；未自设 CARGO_TARGET_DIR
scripts/cargo-team.ps1 test -p ruagent-memory -Nocapture            # 59 passed / 0 failed / 0 ignored，exit 0
scripts/cargo-team.ps1 test -p ruagent-daemon --lib distill -Nocapture   # 15 passed；含 R-3 的 merge_judged 0→2
python %TEMP%\rvb2_carrier.py        # 我自己的载体探针（活库 ?mode=ro）：18 版 / 14 列 / 无 memory_sources / 无新 op
git status --porcelain -- crates panel   # 50 项全部是并发波次；我唯一新增 docs/design/reviews/gen2-memory-review-r2.md
# code 面判据（grep，不经作者转述）：usage.rs:65/93 · memembed.rs:263-267/340 · inject.rs:41/47/56/63/588/646/650
#                                   · dedupe.rs 的 MergeAudit/candidate_source · 规格 D.5 追加块 L797-826 · E.2 追加块 L897/901
```

---

## 7 纪律回执

* **只写本单 inScope**：本文件 `docs/design/reviews/gen2-memory-review-r2.md`。`crates/` 与 `panel/` **一行未改**；探针只在 `%TEMP%`。
* **读数三件套**：对象集（t31 窗口的五个文件哈希 + t8 窗内其余文件；活库只读）· 采样面（59 条 crate 测试 + 15 条 daemon distill + 我自己的载体探针 + code 面 grep）· 可证伪判据（`0.500000` vs `0.058660` 严格先后；`误并 0/20`；`decision@0.99 ⇒ 3/20` 的循环性；`first=1/重复 +0`；`candidate_source` 的 embedding/none 两值）。
* **期望值只写一处**（规格 C/D 节与本报告 §2），判据从代码与测试取，**没有复制常量**。
* **未测一律写明**（§5，5 项），owner=INT/t19 与 I-A/I-D 的**不据此判 I-B 失败**。
* **真守护进程 pid 79984 未启停**；`~/.ruagent` 全程 `?mode=ro`（我自己的探针只读）；**未用** `GET /api/v1/recall`；未写任何用户数据。
* **并发声明**：`git status --porcelain -- crates panel` = 50 项，全部是并发波次（wiki 的三态接口变更、graph 的 R-3 接线在途收敛）。我只在读数受影响时报「哪个文件、什么错」，不替同伴改；本单期间 `crates/memory/src` 的五个文件哈希未变。
