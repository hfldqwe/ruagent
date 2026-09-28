# gen2 知识图谱 repair 报告（t27 · I-C · round 2）

> 修复对象：`docs/design/reviews/gen2-graph-review.md`（RV-C / t17）的六条 medium+ findings + G7 的未达标目标 + captain 追加的 R-3（`judge_merge` 接线）。
> 改动面（全在 inScope）：`crates/graph/{src/retrieve.rs,src/lib.rs,tests/**}`、`crates/daemon/src/distill.rs`、`docs/design/reviews/gen2-graph-spec.md`（**只追加勘误段**）、`docs/design/reviews/gen2-graph-impl.md`（**只追加修订记录**）、本文件。
> 没碰：`crates/store/`、`crates/memory/`、`crates/knowledge/`、`crates/daemon/src/api.rs`、`crates/daemon/src/wiki.rs`、`crates/mock-agent/tests/e2e_daemon.rs`、`panel/`。真守护进程 pid 79984 只读，未启停、未写 `~/.ruagent`。
> 环境：一切 cargo 走 `scripts/cargo-team.ps1`（共享 target、一次一个编译、CPU 0-11、BelowNormal），按 captain 的纪律**不自设 `CARGO_TARGET_DIR`**。

## 0 一句话

六条 findings **六条全部关闭**（RVC-1 / RVC-3 / RVC-4 / RVC-5 / RVC-6 / RVC-10），G7 的关系级 precision 从 **0.7500（45/60）提到 0.9149（43/47）**（判据、标注、gold 文件**一字未改**），captain 追加的 **R-3** 也落地（`op='merge_judged'` **0 → 2**，且一次真实写入路径真的完成了一次合并）。另有 1 条目标在不同处**登记 not_measured**（G7 的实体族，无 gold、无会话语料），见 §5。

## 1 逐条 finding：改前 → 改后

### RVC-1（medium · 门禁 t19）`as_of` 是字符串比较 → 归一化为时刻

**改前**：`as_of` 与边的 `valid_at`/`invalid_at` 按**文本**比较（`retrieve.rs:602`、`:719-726`），同一瞬间的三种写法给出不同证据。

**改后**：入口处 `parse_ts` 把 `as_of` 解析成 `DateTime<Utc>`（解析不了 ⇒ **报错**，不再静默按字符串比）；边的两个时刻在索引构建时各自解析一次（`RawEdge.valid_at_t / invalid_at_t`，逐行回落到文本比较**只**用于形状不认识的孤行）。比较全部走 `true_then_at()`（`e.valid_at_t <= t` 且 `e.invalid_at_t > t`）。

**读数**（`... test -p ruagent-graph --test temporal -Nocapture`）：

```
instant 1: canonical "2026-09-13T18:34:20.791485300+00:00" | Z "2026-09-13T18:34:20.791485300Z" | +08:00 "2026-09-14T02:34:20.791485300+08:00" -> facts 9 / 9 / 9 | bytes 10349 / 10349 / 10349 | identical true
   old TEXT rule vs the parsed instant: canonical 0/67, Z-form 0/67, +08:00-form 27/67 edges judged backwards
instant 2: ... +08:00 "2026-09-14T02:38:15.350633200+08:00" -> facts 21 / 21 / 21 | bytes 21268 / 21268 / 21268 | identical true
   old TEXT rule vs the parsed instant: canonical 0/67, Z-form 0/67, +08:00-form 17/67 edges judged backwards
instant 3: ... -> facts 43 / 43 / 43 | bytes 39215 / 39215 / 39215 | identical true
   old TEXT rule vs the parsed instant: canonical 0/67, Z-form 0/67, +08:00-form 0/67 edges judged backwards
as_of="not-a-time" -> Some("io error: as_of is not an instant: \"not-a-time\" (accepted: RFC3339 with Z or an offset, with or without nanoseconds; YYYY-MM-DD; %Y-%m-%d %H:%M:%S)")
RVC-1 summary: old text rule judged backwards per instant (canonical, Z, +08:00) = [(1, [0, 0, 27]), (2, [0, 0, 17]), (3, [0, 0, 0])]
```

**三条负例读数**（同一瞬间的三种写法结果一致）：instant 1 / 2 / 3 各三种写法 → **逐字节相同**（10349 / 21268 / 39215 B），且每次非空（9/21/43 条不同事实）⇒ 不是空集上的「一致」。

**改前读数**：**旧文本比较在同一次运行里内联重跑**作对照 —— 规范形 **0/67** 判反（**这正是 as-of gold 全是规范形、对该缺陷完全看不见的原因**）、`+08:00` 形 **27/67** 与 **17/67** 判反。
**与 RV-C 数字的关系（不把两个读数混为一个）**：RV-C 量到的 `Z` 形 16/19 是**另一种 Z 拼写**（小数位不同）；我这边保纳秒的 `Z` 形与规范形同解，因为差异只出现在字符串尾部（`+00:00` vs `Z`）时两侧判定恰好相同 —— 我用 `+08:00` 形复现了同一类缺陷。**证据集合本身不同**这一点在改后也在测：三种写法的**全长 JSON** 相同，不只是边数相同。

### RVC-3（medium · 契约）失败尝试不再留下 `run_turn` episode

**改前**：`write_memories`（episode 在这里创建，D2(b) 要求如此）→ `write_graph`；`write_graph` 失败时 episode 已存在而日志是 `failed` ⇒ 面板的 `distilled` 徽章（`episodes.kind='run_turn'`，t350）会把一次**失败的**蒸馏显示成发生过。原测试只直调 `log_outcome`，**断言层弱于声称层**。

**改后**：新增 `Distiller::write_extraction()`（把「episode 之后的所有步骤」收进一个函数，`distill_once` 调它），失败时调 `void_episode()` **显式标记**（`kind = 'run_turn_failed'`，并把 `voided_by/reason` 写进 `meta`，`COALESCE(meta, ?)` 保证不覆盖既有 meta）。**episode 的创建位置没有动**（D2(b) 保持）。
为什么不删：`memories.source_episode` 指向该 episode（t347 把来源从正文挪到字段），删除要么撞外键、要么丢掉好记忆的来源；标记同时保住「记忆 + 来源」和「徽章不撒谎」。

**读数**（真实失败路径：测试在 `entity_edges` 上建 `BEFORE INSERT … RAISE(ABORT)` 触发器，让 `write_extraction` 真的报 SQL 错）：

```
the real failure: adding relation runs_on: sqlite error: t27 injected failure: t27 injected failure: Error code 1811: A RAISE function within a trigger fired
READING RVC-3: episodes 1 -> 1 | kind=run_turn 1 -> 0 | episode kind="run_turn_failed" | meta=Some("{\"voided_by\":\"write_graph\",\"reason\":\"adding relation runs_on\"}") | memories keeping their provenance 1
```
⇒ **episode 计数不变（1 → 1）**、`kind='run_turn'` 的计数 **1 → 0**（徽章为假）、episode 被**标记**而非删除、记忆的来源仍在。

### RVC-4（medium · 可复现性）并列边的顺序确定

**改前**：`next.into_values()`（HashMap 迭代序，每实例随机）+ 稳定排序 ⇒ 同分同端点序列的并列边在 `truncate(beam)` / `truncate(max_paths)` 之前**无法全序**；RV-C 实测 6 次 6 份不同 JSON（13689–13723 B），同一槽位在 49/52/53 之间轮换。
**改后**：两处排序键末尾追加**边 id 序列**（`edge_seq()`，同时是路径形状去重的身份）。

**读数**（`... --test multihop-gold -Nocapture`）：

```
the largest parallel-edge group in the fixture: [49, 52, 53, 55]
6 runs of the same query: lengths [13696, 13696, 13696, 13696, 13696, 13696] | byte-identical: true
ranking key: 12 paths -> (score,hops,nodes) distinct 8, + edge ids distinct 12
```
⇒ 既是「6 次逐字节相同」，也是「键真的能区分」：旧键在 12 条路径上只区分 **8** 条，追加边 id 后 **12/12**。并列组正是 RV-C 点名的 `[49,52,53,55]`。

### RVC-5（medium · D.2 契约）证据行按真实边方向渲染

**改前**：`lines()` 恒渲染 `nodes[i] -rel-> nodes[i+1]`，反向遍历时把 `用户19410 -uses-> 微信` 写成 `微信 -uses-> 用户19410` —— 一行**要注入给 agent** 的文本在断言反向事实（RV-C：`微信` 的 4 条路径全部反向）。
**改后**：箭头由每条边自己的 `src`/`dst` 决定：正向 `-rel->`、反向 `<-rel-`；两端名字恒按边存储的方向给出；形状不可能出现的分支也按边端点渲染（宁可啰嗦也不静默接错）。

**读数**：

```
Q "微信": 4 paths, 4 reverse hops, all in the stored direction
Q "DeepSeek Harness": 12 paths, 8 reverse hops, all in the stored direction
Q "银河麒麟 V10": 12 paths, 6 reverse hops, all in the stored direction
Q "ruagent": 12 paths, 8 reverse hops, all in the stored direction
direction check: 40 paths across 4 questions, 26 of their hops traverse an edge backwards, all rendered in the stored direction
```
断言形态：每个反向跳必须出现 `here <-rel- next`，且**旧的错渲染串 `here -rel-> next` 必须不存在**；4 个问题里 26/40 个跳是反向的 ⇒ 非空集。

### RVC-6（medium · 交付面不完整）实体族登记 + 规格勘误

**(a) 实体族目标登记为 `not_measured`**（不许留空、不许算达成）：见 §5 表第 1 行 —— 原因：现有 gold 只有**边 / 关系名 / 实体对**三族，**没有实体族**、也**没有会话语料**（规格 §C·G7 要的是「20 个 gold 会话」上的实体 P≥0.85 / R≥0.70）。owner = 下一代数据待办（需要 20 段**手工标注的会话**，而不是活库快照）。
**(b) 规格勘误**：`docs/design/reviews/gen2-graph-spec.md` **追加**「附录 H 勘误（t27 · 2026-09-28 追加，只追加不删原文）」，把 §C·G7 的复现命令从不可执行的 `python docs/design/reviews/gold/score.py …` 指向**真实位置** `crates/graph/tests/gold/` 与真实复现方式（`cargo test -p ruagent-graph --test extraction-gold`），并写明**为什么不补 `score.py`**（那会成为 gold 判据的第二个实现，与 `extraction-gold.rs` 必然分叉：单一真相源）。旧命令原文仍在 §C·G7 里，勘误段引用它，不删。

### RVC-10（low · 断言层）`TrueAsOf` 第三态有断言了

**改前**：`grep TrueAsOf tests/` = **0 命中**，四态里第三态只活在实现里。
**改后**：新增 `a_fact_true_then_and_superseded_now_is_reported_as_true_as_of`：先由快照独立算出「当时为真、后来失效」的边集，再要求检索结果里这些边必须是 `TrueAsOf{..}`，且 `lines()` 渲染含 `true_as_of`。
**读数**：
```
TrueAsOf: 1 of the 1 true-then edges came back in the evidence; 1 of 9 rendered lines say true_as_of
```
（该瞬间这样的边只有 1 条：区间内的失效边；断言非空由 `assert!(!expected_true_as_of.is_empty())` 守着。）

## 2 G7：关系级 precision 0.7500 → 0.9149（两侧并排，避免误读）

**测法**：把冻结的 **60 条 current 边按 id 顺序重放进真实写入器** `upsert_fact`（在「实体已在、边为空」的副本上），再用**同一份人工标注**评分 —— 测的是今天写侧的行为，不是对旧数据的模拟；判据、标注、gold 文件**一字未改**。

| | 改前 | 改后 |
| --- | --- | --- |
| **写出的边** | **60** | **47**（11 条判为同一主张 + 2 条拒收） |
| 关系级 precision（严格） | **45/60 = 0.7500** | **43/47 = 0.9149** |
| 端点级（容忍重复） | 56/60 = 0.9333 | — |
| 标注构成 | supported 45 / duplicate 11 / mislabeled 4 / unsupported 0 | supported 43 / duplicate 2 / mislabeled 2 / refused 2 |
| 关系名个数 | 35 | **29** |
| 其中对象专名（ad_hoc） | 6 | **3** |

**必须一起读的事实**：precision 升高的同时，**被写出的边从 60 降到 47**（`duplicate 11` + `refused 2` 不再落盘），而且 supported 的**绝对条数从 45 降到 43**（两条 supported 边被判定为与同族代表同一主张 —— 见下面「为什么 first-wins」）。所以这不是「全都变好了」，而是「重复与非法关系名不再成为事实」。

**机制（三条，都在写侧）**：
1. **关系族（`relation_family`）**：`model_inference`{drives_model,runs_model,powers,powered_by}、`runtime_integration`{integrates_runtime,orchestrates,supports}、`permission_gatekeeper`{uses_model_for_permission_gatekeeping,uses_as_permission_gatekeeper,uses_for_permission_gatekeeping,hosts}、`protocol_use`{uses_protocol,uses}。
2. **`upsert_fact` 的规则**：非关系名拒收 → 同一**无序端点对**上已存在**同族**当前边（名字不同）⇒ 判重复、不写（**first-wins**，不销毁已有行）→ 同名同对：事实哈希相同判幂等、不同则按既有双时态路径 supersede（更正仍然生效）。
   **为什么不对事实文本做比较**：这 11 条重复是**改写**（「DeepSeek Harness 驱动 deepseek-v4-flash 模型运行。」vs「…在该会话中运行…」），实测**11 条的 fact_hash 全不相同** —— 文本/哈希看不见它们，族才看得见。
   **为什么 first-wins 而不是 last-wins**：写侧没法知道后面还会来一条更「正」的拼写；last-wins 会把 49/14/15/50 这些代表替换成后写的同族名（预测读数更差且会丢弃已有行）。代价：同一对、同族、**不同关系名**的更正会被当重复跳过 —— 更正应走同名 supersede（已支持）或 `merge_entities`/人工判。
3. **拒收非关系名（`relation_verdict`）**：`must_never_drop`、`planned_queryable_store` 这类**陈述约束/计划**的名字不落盘为边（理由与计数写进日志，不静默跳过）。其余对象专名（`uses_as_production_database` 等）**照收** —— 词表不是封闭白名单，新的一般关系名按原样写入（否则会静默丢真事实）。

**读数**：
```
G7 AFTER: replay of 60 frozen current edges -> written 47, duplicate 11, refused 2 | labels {"duplicate": 2, "mislabeled": 2, "supported": 43}
G7 AFTER: relation-level precision (strict) = 43/47 = 0.9149  (BEFORE: 45/60 = 0.7500)
G7 ontology direction: distinct relation names 35 -> 29, object-specific (ad_hoc) 6 -> 3
```
逐条 drop/refuse 的名单也在同一测试输出里（例如 `dropped #52 runs_model(42->55) as the same claim as #49 drives_model`、`refused #47 planned_queryable_store(52->51) : a constraint/plan about ONE object, not a relation between two`）。

## 3 R-3（captain 追加）：`judge_merge` 接进写入路径

**改前**：`memembed::judge_merge` **全仓无生产调用者**（V-B/t12：活库 `memory_diffs.op='merge_judged'` = **0 行**）；写入路径到 `distill.rs → mergeable_target → dedupe::judge` 就停了。
**改后**：`write_memories` 的合并决策改走 `Distiller::merge_target`：
* **有真 embedder** ⇒ `memembed::judge_merge(&db, embedder, store, namespace, content)`（它自带**有界候选集** top_k=3、按该 scope 自己的 p99 导出的 τ、具名判据，并把决策写进 `memory_diffs.op='merge_judged'`）；只有 `MergeVerdict::Merge{candidate}` 变成 supersede。
* `Same` ⇒ 交回 store 自己的 content-hash（记 `skip_dedupe`），不重复决算。
* **`NeedsJudgement` ⇒ 不合并**（**LLM 收口那一半未落地**，见 §5）。
* **无 embedder 或 `is_fallback()`** ⇒ 回落 `mergeable_target` 的词法判据。理由（这条 mem-core 也认同）：候选规则是**余弦**排序，而 hash fallback 的余弦不是证据 —— 本文件早先实测极性对 `用户偏好简体中文` / `用户不使用简体中文` 的 cos = **0.5000**；在无意义尺度上排序会把无关行合并，那比留下两行更糟。

**读数**（真实写入路径 + 非 fallback 的测试替身 embedder；`op='merge_judged'` **0 → 2**）：
```
READING R-3: merge_judged rows 0 -> 2 | written=2 skipped=0 | live=1 superseded=1
   audit: before=None reason=Some("candidates=0 source=none anchor=none escalated_from=none over_tau=none polarity_refused=none rule=no_candidate verdict=new")
   audit: before=Some("id=1") reason=Some("candidates=1 source=embedding anchor=1 escalated_from=none over_tau=none polarity_refused=none rule=lexical_ignorable verdict=merge")
```
第二条审计行就是**一次真实合并**：有界候选集（1 条）、来源标 `embedding`、决策锚在那条上、`verdict=merge`，结果是 `live=1 superseded=1`。断言按 mem-core 的键集写（`candidates=`/`source=`/`anchor=`/`rule=`/`verdict=`），**不只**断言 `rule=`/`verdict=` —— 否则「跟错锚点」的决策也能通过。

## 4 被否决 / 有异议的 finding

**无被否决项。** 六条 findings 与 G7、R-3 全部按 requiredFix 落地。三点口径说明（不是异议，是防止两个读数被混为一谈）：
1. RV-C 的 `Z` 形 16/19 与我的 `+08:00` 形 27/17 是**不同拼写**量到的同一类缺陷（§1 RVC-1）。
2. G7 的 precision 升高**伴随**写出边数 60 → 47（§2 表上方那段）。
3. RVC-6(b) 我选择「规格勘误」而不是「补 `score.py`」（单一真相源，理由写在规格附录 H）。

## 5 仍然 `not_measured` 的项与 owner（不静默跳过）

| 项 | 状态 | 原因 | owner |
| --- | --- | --- | --- |
| G7 **实体族** 实体 P≥0.85 / R≥0.70（「20 个 gold 会话」） | **not_measured** | 现有 gold 无实体族、无会话语料；活库快照不能替代手工标注会话 | 下一代数据待办（需人工标注 20 段会话） |
| G7 关系 **recall** ≥0.60 | **not_measured** | 无可追溯的 gold 语料（历史边无 `source_episode`） | 需重跑真实蒸馏 + 标注 |
| G7 **幻觉率** ≤0.10 | **not_measured** | 同上：无 transcript 可比对；机制已在 fixture 上被测（含「分不清改写与发明」的已知假阳性） | 同上一行 |
| **R-3 的 LLM 收口**（`NeedsJudgement` 升级为一次 LLM 判定） | **未落地** | 需要一次 per-memory 的 LLM 往返；本单只落地「有界候选集 + 具名判据 + 审计」这一半，且**不把未决当合并**（审计行照写，`verdict=needs_judgement` 可见） | 本代后半 / 下一代（mem-core 的 C2 记为「一半落地」） |
| G1 生产端到端实体腿（recall 响应内） | **not_measured** | 需 t19 的 `api.rs:2524` 换线（DEP-1）；**as_of 已修复，`?as_of=` 现在可以接线了** | INT/t19 |
| G10 `<graph>` 进注入 | **not_measured** | tag 词表在 `crates/memory/src/inject.rs`（mem-core）+ 生产点在 api.rs（t19） | INT/t19 + mem-core |
| G4 `extracted ≥30%`（活库） | **not_measured** | 需要一次真实重蒸馏 | 跑批 |
| E10 向量种子腿 | **未实现** | 本 crate 未接嵌入器；`SeedLeg::VectorNearest` 恒为 0 且**出现在读数里** | 下一代 |
| G8 三指标成对判决 ≥60% | **not_measured** | 需 LLM 判定跑批 | 评测 harness |

**阶段说明**：RVC-1 修复前后，**t19 不得接 `?as_of=`** 这条闸门现在解除（读数为证：三种写法逐字节相同、非时刻输入报错）。

## 6 门禁与两件证据（覆盖哪些 target + 本次确实重查了哪个包）

规则要点：**先 `clean -p <crate>` 再跑**（cargo 的「绿」可能是缓存命中，本代已被骗过多次），并用 `--verbose` 的逐 unit `--crate-name …` 行作为第②件证据。

### 6.1 `test -p ruagent-graph` → **exit 0**

① **覆盖的 target（11 个）**：`ruagent_graph`（lib）、`ruagent_graph`（lib test）、`resolution`、`fixture`、`extraction_gold`、`empty_recall_pattern`、`entity_query_shapes`、`live_after`、`seed_resolution`、`temporal`、`multihop_gold`。
② **本次确实重查了哪个包**：先 `clean -p ruagent-graph` → `Removed 4985 files, 987.5MiB total`（另一次为 115 files / 39.2MiB），随后 test 输出里的 `test result:` 行是本轮的。
读数：**34 passed / 0 failed / 3 ignored**（ignored = `live-after` 的三条 opt-in 活库副本读数；带 `RUAGENT_GRAPH_LIVE_COPY` 时会真跑）；各 target：3 / 1 / 3 / 7 / 7 / 3 / 5 / 5。

### 6.2 `clippy -p ruagent-graph --all-targets -DenyWarnings`（先 `clean -p ruagent-graph`）→ **exit 0**

① **覆盖的 target（同一批 11 个 unit）**：逐 unit 的 `Running clippy-driver … --crate-name …` 行给出 `ruagent_graph`(lib) / `ruagent_graph`(lib test) / `resolution` / `fixture` / `extraction_gold` / `empty_recall_pattern` / `entity_query_shapes` / `live_after` / `seed_resolution` / `temporal` / `multihop_gold`。
② **本次确实重查了哪个包**：`clean -p ruagent-graph` → `Removed 115 files, 39.2MiB`，接着 `Finished dev profile in 1.98s`、**exit 0**、无 error/warning。本轮 prior 修掉的两条（都出在我这次新写的代码里，`-D warnings` 直接红）：`clone` 用在 `Copy` 的 `Option<DateTime<Utc>>` 上、以及 `let as_of = as_of;` 的冗余重绑。

### 6.3 `test -p ruagent-daemon --lib distill` → **exit 0** · `check -p ruagent-daemon --all-targets` → **exit 0**

**时序（两个状态都记录，因为「什么时候绿的」是读数的一部分）**：
1. mem-core 修好 `memembed.rs:529` 的 `MergeAudit` 类型不一致后，我读到第一次绿：`test -p ruagent-daemon --lib distill` → **15 passed / 0 failed**（含 RVC-3 与 R-3 的读数）。
2. 随后 wiki 的破坏性接口变更（`WikiLead.cite_coverage: f32 → Option<f32>`、新增 `has_anchors`/`anchored_sections`）在 mem-core 的 `memembed.rs` 里尚未适配 ⇒ 同一条命令变红（3 个错误，**全在别人的文件里**）。我把坐标与原文报给 wiki 与 mem-core，**未改他们一个字节**。
3. 两人收敛后重跑，**四条 verify 全绿**（本轮最终读数）：
   - `test -p ruagent-graph` → exit 0，**34 passed / 0 failed / 3 ignored**（11 个 target 逐个枚举；先 `clean -p ruagent-graph` 强制重查）。
   - `clippy -p ruagent-graph --all-targets -DenyWarnings -CleanFirst ruagent-graph` → exit 0，`-CleanFirst` 卸掉 284 files / 71.4MiB 后 `Finished in 1.72s`，**无 error/warning**，逐 unit 列出 11 个 target。
   - `test -p ruagent-daemon --lib distill` → exit 0，**15 passed / 0 failed**。
   - `check -p ruagent-daemon --all-targets` → exit 0，逐 unit 覆盖 **`ruagent_daemon`(lib) + `ruagent_daemon`(lib test) + `injection_e2e` + `knowledge_api` + `smoke`**（外加依赖 `ruagent_graph`）⇒ daemon 的**全部 target** 本次确实重新检查过，`Finished in 2.74s`。
