# RV-A 评审（t15）：I-A 的 C 节目标逐条判决

> **结论：`pass`**。`gen2-recall-spec.md` C 节里 **I-A 自有的目标全部有我的独立可复现读数支撑**（C1/C2/C4a/C4c/C5/C6-库侧/C8a/C8b/C9 达标；C4b 库侧达标、产品级 owner=integ/t19 判 not_measured；C6 的 JSON 半 owner=integ/t19 判 not_measured；C3/C7 按 captain 裁决 **deferred**）。**findings 5 条（3 medium + 2 low），没有一条推翻任何 KPI 判决**，每条都给了 requiredFix 与 owner；另有 4 条观察与 1 条在途 repair 的现场登记。
> **评审员**：review（独立评审员，**不是** I-A 的作者；`crates/` 与 `panel/` 一行未改，本单只写本文件）。
> **被评审对象**：I-A（t7）报告 `docs/design/reviews/gen2-recall-impl.md`（222 行 / 00:53:52）与 `crates/knowledge/**`。
> **字节绑定**：`src/lib.rs 9884EDF3`（00:27:01）· `src/rrf.rs 7EA083F8`（00:28:49）· `src/store.rs EDF4FC81`（00:34:56）· t7 的 6 个测试文件全部 ≤ 00:51:07（`retrieval-quality 56CAC06E` / `retrieval-gold-copy 99C89163` / `retrieval-legs B4EE8CA7` / `retrieval-cjk 216B397F` / `retrieval-gold-live C570379C` / `tests/common` 于本轮被 repair 改过，见 §1.2）。⇒ **我的全部读数归 I-A 的字节**。
> **时间窗**：2026-09-28T01:44 → 02:2x +08:00。
> **构建与纪律**：每一次 cargo 都走 `scripts/cargo-team.ps1`（单一编译 / 共享 target，**未**自设 `CARGO_TARGET_DIR`；任务单里那一行 `$env:TEMP\ruagent-review-rva` 是对全队纪律的有意替换，与 V-C/V-A 同形）。真守护进程 **pid 79984 未启停**；`~/.ruagent` 全程只读；**未用** `GET /api/v1/recall`。我自己的副本 `%TEMP%\rva-live2`（`VACUUM INTO` 的 db + `robocopy` 的 `data/lancedb`、`knowledge/`）与我自己的模型缓存 `%TEMP%\rva-hf`（只复制 `models--intfloat--multilingual-e5-small`）。

---

## 0 一句话

I-A 把召回从「唯一的分数是名次分、CJK 只靠全表扫、排序随分页参数变」推到了**加权融合 + 恒定腿窗口 + 真实量纲 + 汉字 bigram 索引 + 冻结 gold 集分层评测**。我在**我自己的副本、我自己的真模型缓存**上逐位复现了它的三个 KPI（**recall@1 0.7333 / MRR 0.8185 / nDCG@10 0.8621**）、C4a 20/20、C5 15/15、C6 的 4 种量纲，并**自己测了 store 级 C4b**（bigram 索引 p50 **0.047 ms**，对照 LIKE 全表扫 23.489 ms）。代价披露也是真的：recall@5 1.0 → **0.9333**，牺牲的是唯一一条 content-derived gold（`kubernetes 滚动更新回滚` rank 2 → **8**，我逐位复现）。

**三条 medium 都不能写成「通过」，也不能写成「I-A 失败」**：F-4（具名局限，owner=下一代）· F-1（`query_eval_gold` 不幂等，captain 已另立 repair 单，且是本轮**在途落地**的）· F-5（`ScoreKind` 的 serde 形状，captain 已裁归 integ/t19）。§5 按这个形状逐条写。

---

## 1 我的独立读数

### 1.1 我先造自己的仪器（不借作者/V-A 的副本）

```
%TEMP%\rva-live2\data\ruagent.db   ← VACUUM INTO（从 ~/.ruagent 的 ?mode=ro 连接）
%TEMP%\rva-live2\data\lancedb      ← robocopy（103 MB / 6812 文件）
%TEMP%\rva-live2\knowledge         ← robocopy（3.8 MB / 934 文件）
%TEMP%\rva-hf\models--intfloat--multilingual-e5-small  ← 469.6 MB（只这一个模型）
```

### 1.2 门禁的三次状态（时间点 + 文件 + 错误，按 F5 纪律不归因给 t7）

| 时间 | 命令（无 env / 有 env） | 我的读数 |
| --- | --- | --- |
| **01:44–01:46** | `... test -p ruagent-knowledge -Nocapture`（env：我的副本 + 我的 HF） | **64 passed / 0 failed / 0 ignored**（35+6+9+1+1+9+3）——这就是 **t7 字节**的绿读数 |
| 01:47–01:49 | 仓库里出现 repair(t29) 的三件：`tests/gold-seed-idempotence.rs`（01:47:21）· `crates/store/src/migrations/0025_query_eval_gold_unique.sql`（01:48:57）· `tests/common/mod.rs`（01:49:09） | —— |
| **01:48** | 同一命令（不带 env） | **exit 101**：新测试 `the_gold_seed_is_idempotent_only_because_0025_makes_it_so` 断言失败：`left: [(22,15),(44,30),(66,45),(88,60),(110,75),(132,90)]` vs `right: [(22,15)×6]` |
| **01:5x** | 同一命令（不带 env） | **exit 101**：`error[E0382]: borrow of moved value: expected`（`gold-seed-idempotence.rs` 编译不过） |

⇒ **这不是 t7 的缺陷**，是 F-1 的 repair **正在落地**造成的中间态（测试先行、迁移与 seed 还未对齐）。我按纪律只报「哪个文件、什么错」，不替同伴改。**我的判决绑定在 01:44–01:46 那次绿读数 + t7 的字节哈希上**；带 env 的三条 live 测试我在 §1.3–§1.7 又单独各跑了一遍（都绿）。

### 1.3 C1/C2/C9（`--test retrieval-gold-copy -Nocapture`，我的副本、真模型）

```
[t7-gold] gold set id=1 newly written=22 embedder=fastembed:multilingual-e5-small (dim 384)
[t7-gold] n=15 misses=0 recall@1=0.7333 recall@5=0.9333 recall@20=1.0000 MRR=0.8185 nDCG@10=0.8621
[t7-gold] 逐类：exact-ascii(2) 1.0000 · cjk-run-long(1) 1.0000 · cjk-run-nosep(1) 1.0000
          mixed-long(1) 1.0000 · punctuated(1) 1.0000 · mixed-cjk-ascii(8) recall@1=0.6250 MRR=0.7708
          content-wiki(1) recall@1=0.0000 recall@20=1.0000 MRR=0.1111
[t7-gold] "kubernetes 滚动更新回滚" class=content-wiki rank=Some(8)   ← 代价定位到这一条
```

与作者/V-A **逐位相同**（0.7333 / 0.9333 / 1.0000 / 0.8185 / 0.8621、rank 8、mixed-cjk-ascii 0.6250）。

### 1.4 C4a / C4b / C4c

```
[t7-evidence] C4(a) over 51 derived 2-char Han substrings in 21300ms: a term-prefix stage already
              reaches 31/31 (ladder working); of the 20 the two FTS stages CANNOT reach,
              the bigram index answered 20 = 1.0000; fell through: []          ← 我自己的读数
--test retrieval-cjk: a_two_char_han_substring_is_served_by_the_bigram_index ... ok（9 passed）
```

**C4b 我自己独立测的 store 级读数**（`%TEMP%\rva_latency.py`，只读我的副本）：`chunks=10765`、`grams NULL=0`（写路径 + 回填在真实副本上确实填满）· **bigram 索引 `MATCH` p50 = 0.047 ms**（p90 0.068 / max 0.210）· **对照 LIKE 全表扫 p50 = 23.489 ms**（p90 26.070）· 索引与 LIKE 命中数 **86/86 一致** ⇒ `p50 ≤ 1 ms` **达标**（比 A8 的 0.08 ms 标定还好，因为那是旧采样）。**产品路径 p50 未测**（owner=H-2/t19，见 §4）。

### 1.5 C5（`--test retrieval-gold-live -Nocapture`）

```
[t7-evidence] C5 page-size independence: 15/15 queries keep the same top-1 document AND score
              across limit ∈ {5,10,20,30}; differing=[]
```

目标 13/13 ⇒ 达标（15 条比规格的 13 条多两条）。

### 1.6 C6（同一跑）

```
[t7-evidence] fusion=rrf(k=60,w_sem=2,w_kw=1) leg_window=60 scoring_version=2 relevance_version=1
[t7-evidence] score kinds seen=["bm25","calibrated","rrf_rank","semantic_l2sq"]
              hits with a missing leg reported as None=231 reported as 0.0=0
              relevance min=0.0000 max=0.5556
```

库侧 4/4 分数位都有量纲（第三个 `keyword` 腿的 `bm25` 也在），**缺腿用 `None` 而不是 `0.0`**（231 vs 0）。**JSON 5/5 未接线**：我读 `api.rs` 只有三处 `"score_kind": "rrf_rank"` 手写字面量（L2579 / L2591 / L3003），没有任何一处从 `ScoreKind` 取 ⇒ 作者**没有多报**（owner=integ H-1）。

### 1.7 C8a / C8b

* **C8b 达标（我自己的 HF 副本）**：`embedder=fastembed:multilingual-e5-small (dim 384)` —— 真模型读数 **PRESENT**；合成 harness 仍打印 `"entity": "ABSENT -- the entity leg lives in crates/graph…"`（**不是**静默回退，且是**另一条腿**的说明，不是模型）。
* **C8a 达标**：活库切片不饱和（我读到 `recall@1=0.7333 < 1.0`），且断言就在仪器里：`retrieval-gold-copy.rs:95-98` 「the live slice is saturated: every metric is 1.0, so nothing here can fail」。**字面归属有个小坎**：规格 C8a 写的是「在 `retrieval-quality` 仪器上」，而非饱和切片在新的 `retrieval-gold-copy.rs`；规格 target 自己允许「例如把活库 gold 的前 N 条做成固定切片」⇒ 判**达成**，但建议规格 owner 把仪器名写准（观察 O-1）。

### 1.8 冻结接口没有被破坏（我自己的 diff 分析 + 编译证明）

| 冻结项 | 我的读数 | 判 |
| --- | --- | --- |
| `rrf(rankings, k)` | `rrf.rs:6` 签名逐字未动；`rrf_weighted` 是**新增**（`rrf.rs:35`） | 保持 |
| `SearchLegs` | `store.rs:371` `pub type SearchLegs = SearchEvidence`（**旧名字与新字段都保留**，lib.rs:24 仍导出） | 保持（别名） |
| `SearchHit` / `LegHit` / `KeywordStage` / `search(&str, u32)` | 均在位（`store.rs:45/299/325`；`search` 签名未改，body 改用 `LEG_WINDOW`） | 保持 |
| `score_kind` 字面量 | `retrieval-cjk.rs:188-192` 逐条 pin 了 `as_str()` 的 5 个字面量 | 保持 |
| 三文件的删行 | `lib.rs` 7/2 · `rrf.rs` 84/**0** · `store.rs` 658/55；删的都是文档注释、旧 `struct SearchLegs`（→ 别名）、旧 `search` body（含 `leg_k = limit.max(10)`）、一行 re-export | 无未登记的破坏 |
| 消费者编译 | 我跑 `... check -p ruagent-daemon --all-targets` = **exit 0**（Finished） | 冻结面不破编译 |

### 1.9 判据可证伪（我自己的、不制造失败的一侧）

* **符号面**：`git grep` 在 **HEAD** 上 0 命中、工作树 N 命中：`LEG_WINDOW` 0/14 · `ScoreKind` 0/16 · `search_page` 0/3 · `relevance_from_distance` 0/5 · `rrf_weighted` 0/10 · `grams` 0/7 ⇒ t7 的判据断言的对象在**改前根本不存在**。
* **行为面（引 HEAD 源码）**：HEAD 的 `search` 里是 `leg_k = limit.max(10)`（正是 C5 的 10↔20 翻转机制），HEAD 的 keyword 腿注释写着 `LIKE substring scan. Runs only when both FTS stages were empty`（正是 C4c 的「唯一路径就是全表扫」）。⇒ 四条判据在旧码上必然红（V-A 用两棵树实测了红侧，我在这里给的是**同一结论的静态证据**，不重复制造失败）。
* **F-1 我自己两侧都读到**：代码侧 `tests/common/mod.rs:187` 是 `INSERT OR IGNORE INTO query_eval_gold`，而表上**没有 `UNIQUE(set_id, query)`**（0019 只给了 `id PRIMARY KEY` + `(set_id, class)` 索引）⇒ `OR IGNORE` 无可忽略；数据侧在我自己的副本上连跑两次：**22 行 → 44 行（可答 15 → 30）**。

---

## 2 C 节逐条判决

| # | 规格 target | 我的独立读数 | 判决 |
| --- | --- | --- | --- |
| **C1** | 活库 recall@1 ≥ 0.70 | **0.7333**（n=15, misses=0；my copy, real model） | **达标** |
| **C2** | MRR ≥ 0.80 | **0.8185** | **达标** |
| **C3** | 无答案查询 ≥85% 落在可答 90 分位外（≥60 条集、≥20 条无答案） | 前置我复核：集合 **22 条**（15 可答 + 7 无答案）、`judged_by` 全 `title-derived`（human **0** 行）；两个量：语义腿最近距离 gap **0.0237**（我的运行 `max=0.1985 / min=0.2222`）、融合 top-1 的 semantic_score gap **0.001100** | **deferred（captain 裁决）**，§3 |
| **C4a** | 索引腿能命中 ≥ 0.95 | **20/20 = 1.0000**（两个 FTS 段够不到的那 20 条；另 31 条由 prefix 段命中——降级链正常） | **达标** |
| **C4b** | 这些查询 p50 ≤ 1 ms | store 级 **p50 0.047 ms**（我自己测；对照 LIKE 23.489 ms）；产品级 not_measured | **库侧达标**；产品级 **not_measured (excused)**，§4 |
| **C4c** | 2 字汉字不再走全表 LIKE | `stage=Bigram` 断言绿；LIKE 段**保留**且只服务 1 字汉字（规格/V-SCHEMA F6 的设计） | **达标** |
| **C5** | 13/13 相同 | **15/15** 相同（文档名 + 分数） | **达标** |
| **C6** | 5/5 分数键带量纲 | 库侧 **4/4**（`bm25`/`calibrated`/`rrf_rank`/`semantic_l2sq`）+ 缺腿 `None=231` / `0.0=0`；JSON 5/5 未接线（api.rs 三处手写） | 库侧**达标**；JSON **not_measured (excused)**，§4 |
| **C7** | 名含查询原文的子集 rank-1 = 14/14（循环诊断） | **11/14 = 0.7857**；规格 §E7 自述本轮不设达标线 | **deferred（captain 裁决）**，§3 |
| **C8a** | 至少一个切片 baseline < 1.0 | 活库切片 `0.7333/0.9333/0.8185` 全 < 1.0 + 仪器内反饱和断言 | **达标**（观察 O-1） |
| **C8b** | 真模型读数或显式 not_measured | **PRESENT**：`fastembed:multilingual-e5-small (dim 384)`；未设缓存时**大声** ABSENT | **达标** |
| **C9** | nDCG@10 ≥ 0.83 | **0.8621**（二值、每查询 1 gold 的退化形式，规格已声明） | **达标** |

---

## 3 captain 已裁决 deferred 的两条（如实登记：原因 + 下一代前置）

| 目标 | 为什么 deferred | **下一代前置（必须完成才能重标）** |
| --- | --- | --- |
| **C3** | 数据受限：判据定义在 **≥60 条查询、其中 ≥20 条无答案**的冻结集上，今天只有 **22 条（15+7）**；规格明文**禁止**把 `0.227` 写成阈值常量（它的作用只是证明坐标轴上有信号），且两个候选量的间隔只有 0.0011 / 0.0237 | ① 人工分级 gold（`judged_by='human'`，今天 0 行）≥60 条含 ≥20 无答案；② **先修 F-1**，否则「按表行数读」会把这个前置判成**假达标**（我实测 22 条的集合可以涨到 132 行）；③ 在新集上重标切点与 λ |
| **C7** | 指标循环：在 **title 派生**的 gold 上做 **title 匹配**，规格 §E7 已声明本轮不设达标线（它只作诊断） | 分级 gold（与 C9 的分级版同源）到位后，把 C7 升级成非循环指标（正文/内容级相关性）再设线 |

---

## 4 not_measured（owner 已具名，**不据此判 I-A 失败**）

| 目标/读数 | owner | 为什么只能在 t19 之后测 |
| --- | --- | --- |
| **C6 的 JSON 5/5**（响应里每个分数键带同级 `*_kind`） | **integ/t19**（规格 §E.9 **H-1**） | 响应拼装在 `daemon/src/api.rs`（今天 L2579/L2591/L3003 手写 `"score_kind": "rrf_rank"`）；I-A 的 inScope 是 `crates/knowledge`，`ScoreKind` 已经在库里备好，只等 H-1 序列化 |
| **C4b 的产品路径 p50** | **integ/t19**（规格 §E.9 **H-2**） | 产品路径每次调用含真模型 query 嵌入（≈380 ms/查询），不是 C4b 的口径；H-2 把 `search` + `search_legs` 合并成 `search_page` 并把腿窗口/候选/丢弃写进 `recall_log`（0023 的列），那时才有一条**产品路径**的 p50 可测 |

---

## 5 findings（每条给确切文件/命令；medium 逐条给处置，**不用「不构成 blocker」放过**）

### RV-A-1（medium · **具名局限**，owner = 下一代）F-4：0.7333 只在作者那张 gold 面上成立，不能当通用改进
* **采样面（V-A 造，与冻结集不相交）**：11 条（8 条 title-derived 机械取 `documents.name` 末段 + 1 条 content-derived + 2 条非词），**n=9 可答**，同一份真模型、同一份副本。
* **读数**：旧（pre-t7 产品路径）recall@1 **0.3333** / recall@5 **0.6667** / recall@20 0.7778 / MRR **0.4974** / nDCG@10 **0.5661**；新（post-t7）**0.2222** / **0.7778** / 0.7778 / **0.4537** / **0.5359**。逐条：`Slurm基本使用` rank 0 → 1、`Obsidian` rank 6 → 3。
* **怎么写它才准确**：C1 的 metric 在规格里**就是定义在冻结 15 条上的** ⇒ 「C1 达标（0.7333 ≥ 0.70）」成立；**不成立的是推广读法**（「召回质量整体被推到 0.7333」）。作者报告没有把 0.7333 说成全集读数（还主动说它是被 mixed-cjk-ascii 档稀释出来的），但**没有第二张采样面**检验方向。
* **requiredFix（下一代第一优先）**：在 C3/C9 的分级 gold 与 λ 标定**之前**先补一张**机械采样面**（规则化取样、与冻结集不相交、记录 n），把「面向 title-derived 查询族的改进」与「通用改进」分开报告；在拿到那张面之前，任何文档/面板/MCP 文案**不得**把 0.7333 表述为整体召回质量。

### RV-A-2（medium · **下一轮假达标的入口**，owner = recall 的 repair 单 t29，captain 已另立）
* **问题**：`crates/knowledge/tests/common/mod.rs:159-160` 的注释声称 seed「Idempotent, so re-running the instrument neither duplicates rows nor needs a fresh copy」，但 `mod.rs:187` 是 `INSERT OR IGNORE INTO query_eval_gold`，而 `query_eval_gold` **没有 `UNIQUE(set_id, query)`**（0019 只给 `id PRIMARY KEY` + `(set_id, class)` 索引）⇒ `OR IGNORE` 无可忽略。
* **我的读数**：我自己的副本连跑两次 → **22 行 → 44 行（可答 15 → 30）**；repair 新测试的同批读数 `[22,44,66,88,110,132]`。
* **为什么它必须在报告里点明**：下一轮 **C3 的前置是「≥60 条」**；若有人按 `SELECT COUNT(*) FROM query_eval_gold` 读，跑三次就有 66 行 —— **假达标**。
* **requiredFix**（与 repair 单一致，且我确认它**正在落地**）：迁移加 `UNIQUE(set_id, query)`（0025）· seed 改成真幂等（`ON CONFLICT DO NOTHING` 或先查后插）· 提供**只读**评估活库重复行的语句 + 文档化清理语句 · 注释改成事实。

### RV-A-3（medium · 冻结点之外的接口形状，owner = integ/t19 按 captain 裁决）
* **问题**：`crates/knowledge/src/store.rs` 的 `pub enum ScoreKind` 只有 `#[derive(…, serde::Serialize)]`，**没有 `rename_all`** ⇒ serde 序列化是 `"RrfRank"`，而冻结的 wire 字面量（R-B D.7 / 本规格 D.1）是 `"rrf_rank"`（`as_str()` 给的也是它，`retrieval-cjk.rs:188` 只 pin 了 `as_str`）。
* **影响**：H-1 一旦把 `ScoreKind` 直接 serde 进 JSON，字面量会**静默换名**；两代消费方（面板/MCP/注入）读的是同一个键。
* **requiredFix**：给该 enum 加 `#[serde(rename_all = "snake_case")]`（或让 H-1 一律用 `as_str()`），并补一条 **serde 形状 pin**：`assert_eq!(serde_json::to_string(&ScoreKind::RrfRank).unwrap(), "\"rrf_rank\"")`。captain 已把这条归 integ/t19 并要求补漂移钉；类型在 I-A 的 crate 里，两边任一都能修。

### RV-A-4（low · 报告出处）F-2：§1.1 的语义距离摘要引的是**同名的另一个量**
* 作者 §1.1 写 `answerable mean=0.1583 max=0.2263 | no-answer mean=0.3269 min=0.2274`（gap **0.001100**）—— 这是**融合 top-1 命中的 `semantic_score`**；而那一跑的仪器（`retrieval-gold-copy`）打印的是**语义腿最近距离**：我的运行原文 `semantic top-1 distance: answerable mean=0.1510 max=0.1985 | no-answer mean=0.3196 min=0.2222`（gap **0.0237**）。
* **requiredFix**：报告把这两行拆成「同一批查询 / 两件仪器 / 两个量」，各自给 gap；**两条数都对**，错的是标签与仪器归属（差 20× 的 gap 会误导下一轮的切点讨论）。

### RV-A-5（low · 门禁含义）F-3：`64 passed` 里 2 条「没测也报绿」
* 不带 env 时 live 类测试打印 `NOT MEASURED` 后**直接返回**，计数仍是 `passed` ⇒ **计数 ≠ 测量数**。
* **requiredFix**：交接里把判据写成「**有没有真打出 t7 读数行**」（我本单就是按这个判的），并考虑把这两条改成显式 skip/ignored 或在报告里分开计数。

### 观察（不进 findings）
* **O-1** C8a 的仪器归属（见 §1.7）：规格字面写 `retrieval-quality`，非饱和切片在新仪器；规格 target 自己允许该替代 ⇒ 判达标，建议规格 owner 把仪器名写准。
* **O-2** C9 的两个数不矛盾：规格标定 **0.8421** 是「在活库 top-20 内重排」的结果，交付读数 **0.8621** 是端到端（新腿能把新文档带进 top-20，重排不能）——同一冻结 15 条、不同仪器，交付数更好有可解释机制。
* **O-3** 代价披露完整且我复现：`recall@5 1.0000 → 0.9333`，牺牲的是 15 条里**唯一**一条 content-derived gold（`kubernetes 滚动更新回滚` rank 2 → 8），与 C7 的循环性指向同一件事。
* **O-4** 流程事故（作者已自报 §8.1，我登记不另判）：为取改前读数对**共享树** `git stash`，使 t7 API 在 **23:28–00:27** 缺失，连带 mem-core 的 E0425 与 wiki 的 t10 被挡。正确做法（V-A 与我都用的）：`git archive HEAD` 导出 + **独立** `-TargetDir`。建议写进团队纪律：**取改前读数不许污染共享树**。

---

## 6 未测 / 不判定

| # | 项 | 状态 | 原因 / owner |
| --- | --- | --- | --- |
| U-1 | C4b 产品路径 p50 | not_measured | 需 H-2 接线（api.rs，含真模型 query 嵌入）⇒ integ/t19 |
| U-2 | C6 JSON 5/5 | not_measured | H-1 ⇒ integ/t19（我读 api.rs 三处手写确认仍未接线） |
| U-3 | C3 / C7 | deferred | captain 裁决，见 §3 |
| U-4 | 生产 `recall_log` 上新遥测列（0023 的 6 列）是否真被写入 | **未测** | 写入侧在 daemon（E6/H-2）；我不启停 pid 79984、不用 `/api/v1/recall` ⇒ 留给 V-INT/t20 |
| U-5 | 我自己没有重跑「旧树四条判据变红」的两棵树全过程 | **部分** | V-A 做了（`git archive` + 独立 target，四条实测红）；我给了**静态可证伪证据**（HEAD 上 6 个符号 0 命中 + HEAD 的 `limit.max(10)` 与「唯一路径是 LIKE」原文）与**新树绿侧**（§1.3–§1.5）。诚实声明：红侧的动态复现是 V-A 的读数 |

---

## 7 复现命令（我实际跑的）

```powershell
$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"     # 每一次 cargo 都走全队包装脚本
$env:HF_HOME="$env:TEMP\rva-hf"                        # 我自己的模型缓存（只 e5-small）
$env:RUAGENT_IA_LIVE_COPY="$env:TEMP\rva-live2"        # 我自己的副本 root

powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test  -p ruagent-knowledge -Nocapture
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test  -p ruagent-knowledge --test retrieval-gold-copy -Nocapture
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test  -p ruagent-knowledge --test retrieval-gold-live -Nocapture
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test  -p ruagent-knowledge --test retrieval-cjk      -Nocapture
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 check -p ruagent-daemon    --all-targets   # 冻结面不破编译
# 不带 env 的同一条门禁 → live 类测试打印 NOT MEASURED 后仍绿（假通过形状，F-3）
# 我自己的仪器与探针（都在 %TEMP%，不在仓库里）
python %TEMP%\rva_latency.py            # C4b store 级 p50（我自己的读数 0.047 ms vs LIKE 23.489 ms）
# 造副本：db = VACUUM INTO(ro)；lancedb/knowledge = robocopy；HF = 只复制 e5-small
```

---

## 8 纪律回执

* **只写本单 inScope**：本文件 `docs/design/reviews/gen2-recall-review.md`。`crates/` 与 `panel/` **一行未改**；探针与副本全在 `%TEMP%`。
* **读数三件套**：对象集（**我自己的**副本 `rva-live2` + **我自己的**模型缓存 `rva-hf` + 只读活库 `?mode=ro`）· 采样面（冻结 15 条 gold / 51 条 2 字汉字子串 / 4 个 limit / 86 条子串的索引-vs-LIKE / 我自己的 API 级探针）· 可证伪判据（§1.9：HEAD 符号 0 命中 + HEAD 旧码原文；F-1 的两侧读数；C4b 的对照读数）。
* **期望值只写一处**（规格 C 节与本报告 §2），探针从函数/库取，**没有复制常量**。
* **未测一律写明**（§6，5 项），其中 2 项 owner=integ/t19 的**不判 I-A 失败**，C3/C7 按 captain 裁决 deferred。
* **真守护进程 pid 79984 未启停**；`~/.ruagent` 只读；**未用** `GET /api/v1/recall`；所有写操作发生在 `%TEMP%` 的副本上。
* **并发声明**：全树 `git status --porcelain` = 70 项，是并发波次（t6/t7/t8/t9/t10/t22/t25/t29…）；归因只用「被评文件 mtime + 哈希 + 作者 changedPaths」，**不据此写任何 finding**；本单期间 `crates/knowledge/src` 三个文件哈希未变（§0 顶部）。
