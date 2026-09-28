# gen2 召回独立验证报告（V-A · t11）

> 被验对象：I-A（t7）对 `docs/design/reviews/gen2-recall-spec.md` C 节目标的达成情况。
> 本文件只写**我自己跑出来的数**；作者读数一律引用并逐条对照。
> 验证者：verify，**未改任何被验代码**（inScope 只有本文件）；**从未 `git stash` / checkout 共享工作树**。
> 编译纪律（captain 2026-09-28）：每一次 cargo 都走 `scripts/cargo-team.ps1`（单一编译 / 共享 target / CPU 0-11 / BelowNormal）。**这是对任务单里 `CARGO_TARGET_DIR=$env:TEMP\ruagent-verify-va` 的有意替换**；两棵树对比用独立 `-TargetDir "$env:TEMP\ruagent-cmp-va"`（closure §7.1）。
> 真守护进程 pid 79984 未启停、`~/.ruagent` 未写入、**未调用 `GET /api/v1/recall`**。

## 0 一句话

C 节 9 条我逐条独立复现：**C1 0.7333 / C2 0.8185 / C9 0.8621 / C4a 20/20 / C4c stage=Bigram / C5 15/15 / C6 库侧 4 种量纲 / C8a 活库切片有量程 / C8b 真模型 PRESENT** —— 与作者报告**逐位相同**（我自己造的活库副本 + 我自己的 HF 模型缓存，不是作者的 target/副本）。
**C3 / C7 按 captain 裁决记为 deferred**（数据受限 / 指标循环），不作为 I-A 失败，原因与前置数据写在 §7。
**C4b（产品路径 p50）**作者如实记「未测」，我复核其理由成立（§7）。
三条 **medium** finding：**F-4** 我另造的采样面上「新实现更好」**不成立**（rank-1 命中 3→2，MRR/nDCG 略降；只有 recall@5 变好）、**F-1** `query_eval_gold` 的"幂等"注释不成立（跑 6 次得 132 行/90 可答，而集合只有 22 条 ⇒ 下一轮 C3 的「≥60 条」前置若按表读会**假达标**）、**F-5** `ScoreKind` 的 serde 形状是 `"RrfRank"` 而冻结字面量是 `"rrf_rank"`（H-1 一落地就会静默换名，且无测试钉住）；两条 low（§1.1 的距离摘要引的是同名的另一个量；门禁 `64 passed` 里有 2 条在环境变量缺失时什么都没测）。
可证伪性：旧树（`git archive HEAD`）上同一批判据 **实测变红**（§5）；另有**产品路径**（跑着 pre-t7 二进制的守护进程）复现出 0.4667/0.6889/0.7682 的改前基线，与规格 A3 逐位相同 —— 两条独立路径交叉验证了"改前"这一侧。

## 1 对象集 / 采样面 / 时间窗（读数三件套 + 归因）

| # | 对象集 | 采样面 | 我的读数 | 时间点 |
| --- | --- | --- | --- | --- |
| S1 | 活库（**只读**） | `~/.ruagent/data/ruagent.db` | documents **934** / chunks **10765** / `chunks_fts_cjk` 10765；文件 mtime **2026-09-27T21:15:21.078+08:00**（整个验证期未变 ⇒ 我从未写入） | 2026-09-28T01:06+08:00 |
| S2 | 活库向量索引 | `~/.ruagent/data/lancedb` | 最新文件 mtime **2026-09-27T03:10:12**（未变） | 同上 |
| S3 | **我自己的副本 root** | `%TEMP%\va-live`（`VACUUM INTO` 的 db + 复制的 `lancedb` + `knowledge/`）= **120.3 MB** | 仪器写入这个副本（gold 集、run 记录），**永不指向 `~/.ruagent`** | 建于 2026-09-28T01:06:1x |
| S4 | **我自己的模型缓存** | `%TEMP%\va-hf` = **1,449.8 MB**（`models--intfloat--multilingual-e5-small/snapshots/…/onnx/model.onnx` 448.5 MB） | `HF_HOME=%TEMP%\va-hf` ⇒ `embedder=fastembed:multilingual-e5-small (dim 384)` | 建于 2026-09-28T01:12 |
| S5 | 被验代码树 | 工作树 HEAD `0a39e5b801`（含 t7 未提交改动） | 见下表哈希 | 快照 2026-09-28T01:05:43 |
| S6 | 改动前的代码树 | `git archive HEAD` → `%TEMP%\va-old`（**独立 target** `%TEMP%\ruagent-cmp-va`） | 旧树 `crates/knowledge/src` 无 `LEG_WINDOW`/`ScoreKind`/`search_page`（0 命中）；tests 只有 2 个旧文件 | 建于 2026-09-28T01:13 |

**归因（captain F5 纪律，并发波次）**：全树 `git status --porcelain -- crates panel` 收尾 **47 项**（store/knowledge/memory/graph/daemon 都有同伴在途）⇒ **只作背景读数，不据此写 finding**。被验对象的归因只用「mtime 落在被验窗口内 + 作者 `changedPaths`」：

| 文件 | mtime（+08:00） | SHA256/16（我核对时） |
| --- | --- | --- |
| `crates/knowledge/src/lib.rs` | 2026-09-28T00:27:01 | `9884EDF3D30F7345` |
| `crates/knowledge/src/rrf.rs` | 2026-09-28T00:28:49 | `7EA083F8589EA51C` |
| `crates/knowledge/src/store.rs` | 2026-09-28T00:34:56 | `EDF4FC81240C0C4A` |
| `crates/knowledge/Cargo.toml` | 2026-09-27T23:52:22 | `984A2C6E2B286F39` |
| `crates/knowledge/tests/common/mod.rs` | 2026-09-28T00:01:52 | `BC110110757F3A69` |
| `crates/knowledge/tests/retrieval-gold-copy.rs` | 2026-09-28T00:02:00 | `99C891634B6AAE5C` |
| `crates/knowledge/tests/retrieval-gold-live.rs` | 2026-09-28T00:51:07 | `C570379CA868C666` |
| `crates/knowledge/tests/retrieval-legs.rs` | 2026-09-28T00:02:41 | `B4EE8CA75D33D2A1` |
| `crates/knowledge/tests/retrieval-quality.rs` | 2026-09-27T23:58:44 | `56CAC06E9BD2CAA6` |
| `crates/knowledge/tests/retrieval-cjk.rs` | 2026-09-28T00:44:13 | `216B397FF387E6F7` |
| `crates/knowledge/tests/residual-scan.rs` | 2026-09-27T23:52:10 | `646D20A3CF82B75C` |

全部 mtime 早于我的读数窗口（01:05 起），与作者 `changedPaths` 一致 ⇒ 本报告读数归因于 I-A。收尾复核见 §8。

## 2 待验目标表（C1–C9：规格 target → 作者读数 → 我的独立读数 → 差异）

| # | metric（规格） | target | 作者读数 | **我的独立读数** | 差异 |
| --- | --- | --- | --- | --- | --- |
| C1 | 活库 recall@1（15 条冻结 gold，limit=20） | ≥0.70 | 0.4667 → **0.7333** | **0.7333**（我自己的副本 + 真模型） | 一致 |
| C2 | 同集合 MRR | ≥0.80 | 0.6889 → **0.8185** | **0.8185** | 一致 |
| C3 | 无答案落在可答 90 分位外的比例（≥60 条集） | ≥0.85 | 未达/未测（22 条，间隔 0.0011） | **deferred**（captain 裁决；我复核数据前置：`query_eval_gold` 的 `judged_by` 只有 `title-derived`，`human` 0 行） | 记录为 deferred（§7） |
| C4a | 只能靠子串的 2 字汉字里索引腿命中率 | ≥0.95 | 20/20 = **1.0000** | **20/20 = 1.0000**（原文：51 条派生 2 字汉字；term-prefix 已覆盖 31/31；FTS 两阶段够不到的 20 条里 bigram 答 20；fell through=[]） | 一致 |
| C4b | 这些查询 p50 延迟 | ≤1 ms | store 级 0.08 ms；**产品路径未测**（每次含真模型 query 嵌入 ≈380 ms） | 我**复核理由成立**：产品路径的 380 ms 量级来自真模型嵌入而非 LIKE 扫描；我没有独立复测产品路径（§7） | 一致（not_measured，理由成立） |
| C4c | 热路径是否还有 2 字汉字全表 LIKE | 不再走 | stage = **Bigram** | **一致**：`retrieval-cjk::a_two_char_han_substring_is_served_by_the_bigram_index` 断言 `legs.keyword_stage == KeywordStage::Bigram`，我跑该文件 9 passed；该断言在旧代码上是 `Substring`（§5） | 一致 |
| C5 | limit ∈ {5,10,20,30} 的 top-1 文档名与分数逐条相同 | 13/13 | 2/13 不同 → **15/15 相同** | **15/15 相同**（gold-live 打印 `differing=[]`）+ 我自己的 11 条查询同样是 **0 条不同**（§3.5） | 一致（我用**新采样面**再验一次） |
| C6 | (a) 每个数值分数字段有 kind；(b) 无并排混量纲路径 | 5/5、0 | 库侧 **4/4**；JSON 5/5 = H-1（integ） | 库侧：`["bm25","calibrated","rrf_rank","semantic_l2sq"]` 实读到；**JSON 半我读代码确认仍未接线**（`api.rs:3003/3006/3008` 仍是行级 `"rrf_rank"` + 无 kind 的 `semantic_score`/`keyword_score`）⇒ 作者**没有多报** | 一致（JSON 半 not_measured，owner=integ） |
| C7 | gold 名含查询原文子集的 rank-1 命中率（**循环，只作诊断**） | 14/14（规格 §E7 自述本轮不设线） | 8/14 → **11/14 = 0.7857** | **11/14 = 0.7857**（我从自己的逐查询 rank 表独立数出：miss 的是 `docker 常用命令`(2)、`Springboot 热部署`(2)、`ChatGPT提示词`(1)） | 一致，且按 captain 裁决记为 **deferred**（指标循环） |
| C8a | 至少一个切片 baseline < 1.0 | 有量程 | 合成 harness 仍 1.0000；**活库切片** 0.4667/0.6889/0.7682 全 <1.0 | **一致**：合成 harness 我跑出 `recall@1=1.0000 recall@5=1.0000 mrr=1.0000`（仍饱和，且 `byte-identical across two runs = yes`）；活库切片 0.7333/0.9333/0.8185 全 <1.0，测试内断言「不许饱和」 | 一致 |
| C8b | 真模型读数存在（或显式 not_measured） | 有读数 | **PRESENT** `fastembed:multilingual-e5-small (dim 384)`；未设时大声 `ABSENT` | **一致**：我自己造缓存后 `PRESENT`；**两种失败态我都实测**：错误的缓存层级 ⇒ `ABSENT … Failed to retrieve model file 'onnx/model.onnx'` + recall 0.0000；完全未设 `HF_HOME` ⇒ 同样 ABSENT 且断言失败（**不是静默回退**） | 一致 |
| C9 | nDCG@10（二值、每查询 1 gold 的退化形式） | ≥0.83 | 0.7682 → **0.8621** | **0.8621**（我用自己的公式在逐查询 rank 上重算） | 一致 |

## 3 独立读数表（命令 → 我看到的原始行）

### 3.1 门禁（contract 的 Verify）+ 一条「passed ≠ 测过」

```
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-knowledge
→ exit=0；35(lib) + 6(residual-scan) + 9(retrieval-cjk) + 1(gold-copy) + 1(gold-live)
        + 9(retrieval-legs) + 3(retrieval-quality) + 0(doctest) = 64 passed / 0 failed
```
**观察**：这一跑没有设 `RUAGENT_IA_LIVE_COPY`，两个活库仪器各自**立刻 return**（`retrieval-gold-copy.rs:30-37` / `retrieval-gold-live.rs:25-32` 的 `NOT MEASURED` 分支），但计数上它们是 **2 个 passed**。⇒ 「64 passed」不等于「64 个读数」；不设环境变量时，这两条**什么都没测也报绿**（NOT MEASURED 的打印只有 `--nocapture` 才看得到）。这不是 I-A 的缺陷（作者在报告里写明了要设环境变量），但**门禁数字会骗人**，评委与 CI 需要知道。

### 3.2 C1/C2/C9（`--test retrieval-gold-copy -Nocapture`，真模型，**我的副本**）

```
[t7-gold] gold set id=1 newly written=22 embedder=fastembed:multilingual-e5-small (dim 384)
[t7-gold] n=15 misses=0 recall@1=0.7333 recall@5=0.9333 recall@20=1.0000 MRR=0.8185 nDCG@10=0.8621
[t7-gold]   class=cjk-run-long     n=1 recall@1=1.0000 MRR=1.0000
[t7-gold]   class=cjk-run-nosep    n=1 recall@1=1.0000 MRR=1.0000
[t7-gold]   class=content-wiki     n=1 recall@1=0.0000 recall@5=0.0000 recall@20=1.0000 MRR=0.1111
[t7-gold]   class=exact-ascii      n=2 recall@1=1.0000 MRR=1.0000
[t7-gold]   class=mixed-cjk-ascii  n=8 recall@1=0.6250 recall@5=1.0000 MRR=0.7708
[t7-gold]   class=mixed-long       n=1 recall@1=1.0000 MRR=1.0000
[t7-gold]   class=punctuated       n=1 recall@1=1.0000 MRR=1.0000
```
逐查询 rank（我的副本；用于 §2 的 C7 独立核算与 §2.2 的代价定位）：
`Docker compose 0 · docker 常用命令 2 · Springboot 热部署 2 · 反射的访问权限问题 0 · AQS并发锁 0 · 缓存caffeine 0 · Maven项目管理 0 · JdbcTemplate 0 · OJ在线判题系统 0 · docker exec进入容器并执行命令 0 · @Param装饰器 0 · RPC框架 0 · ChatGPT提示词 1 · github相关 0 · kubernetes 滚动更新回滚 **8**`（content-derived 的那条 rank 8、其语义腿 rank 0 ⇒ 与作者 §2.2 的"名字匹配得利、内容匹配受损"逐位一致）。

### 3.3 C4/C5/C6（`--test retrieval-gold-live -Nocapture`）

```
[t7-evidence] fusion=rrf(k=60,w_sem=2,w_kw=1) leg_window=60 scoring_version=2 relevance_version=1
[t7-evidence] C5 page-size independence: 15/15 queries keep the same top-1 document AND score across limit ∈ {5,10,20,30}; differing=[]
[t7-evidence] C4(a) over 51 derived 2-char Han substrings in 20031ms: a term-prefix stage already reaches 31/31 (ladder working);
              of the 20 the two FTS stages CANNOT reach, the bigram index answered 20 = 1.0000; fell through: []
[t7-evidence] score kinds seen=["bm25","calibrated","rrf_rank","semantic_l2sq"]
              hits with a missing leg reported as None=231 reported as 0.0=0 relevance min=0.0000 max=0.5556
[t7-evidence] recorded a query_eval_runs row: {"recall_at_1":0.7333,…,"n":15,"relevance_version":1}
```

### 3.4 C8（`--test retrieval-quality -Nocapture`，合成 harness）

```
[t245/a] embedder=hash-embedder (deterministic, offline) docs=16 queries=19 answerable=16
         fused recall@1=1.0000 recall@5=1.0000 mrr=1.0000 keyword_nonempty=18/19 semantic_nonempty=19
         beyond_precision_prefix=1 cjk_substring_not_scan=1/1 elapsed=889ms
[t245/b] …（同上）…
[t245] byte-identical across two runs = yes
```
⇒ 合成 harness **仍然饱和**（三个指标 1.0000），作者没有假装它有了量程，而是把量程放在活库切片（C8a）——我确认这个选择是必要的：**没有活库切片，任何回归都测不出来**。

### 3.5 我的独立采样面（规格里没有的 11 条）与 C5 再验（我自己写的探针，真模型）

采样规则（机械、可复现、避免手挑）：取 `documents.name` **最后一个 `-` 段**当查询（与 R-A 同一类弱代理），排除冻结的 22 条，按固定步长取 8 条；再加 1 条 **content-derived**（取 chunk 正文里的 4-8 字汉字短语，且**不在**任何文档名里）+ 1 条中文非词 + 1 条代码标识符非词。全部 11 条与冻结集**不相交**。

```
== MY-OWN-11 (n=11) ==
  q="notes"                 class=va-title-derived   rank=None top1=wiki/autohotkey-v2                       stage=Precision cands=119
  q="Slurm基本使用"           class=va-title-derived   rank=1    top1=…Slurm使用笔记-Slurm使用笔记             dist1=0.1630
  q="17da3f"                class=va-title-derived   rank=None top1=…Openwhisk基本使用-数据库内容-数据库内容   stage=Empty cands=60
  q="文件检查点"               class=va-title-derived   rank=0    top1=…CC-文件检查点                        dist1=0.1737
  q="声音克隆"                class=va-title-derived   rank=0    top1=…C-015-声音克隆                       dist1=0.1402
  q="Ultrareview深度代码审查"   class=va-title-derived   rank=1    top1=…CC-Web版使用指南                     dist1=0.1151
  q="tmp临时笔记"             class=va-title-derived   rank=1    top1=obsidian/wiki/log
  q="Obsidian"              class=va-title-derived   rank=3    top1=…S-055-Obsidian可视化Skills套装        dist1=0.2674
  q="使用笔记"                class=va-content-derived rank=2    top1=…S-030-HPC相关笔记
  q="量子香蕉协议 zqx"          class=va-unanswerable    rank=None top1=…E-058-Apache-Zookeeper           stage=Substring
  q="build_hybrid_index_zzq" class=va-code-unanswerable rank=None top1=…jackson远程调用漏洞修复…           stage=Prefix cands=117
METRICS MY-OWN-11: n=9 recall@1=0.2222 recall@5=0.7778 recall@20=0.7778 MRR=0.4537 nDCG@10=0.5359
MY sample: 0/11 differ across limit {5,10,20,30} (target 0)
score kinds of the top hits: rrf_rank|sem=semantic_l2sq|kw=bm25 （腿没命中时 kw=- ，不是 0.0）
```
同一批 11 条在**旧代码**上的读数（产品路径，见 §5）：`n=9 recall@1=0.3333 recall@5=0.6667 recall@20=0.7778 MRR=0.4974 nDCG@10=0.5661`。
⇒ 见 **F-4**：在**另一张采样面**上，「新实现更好」这条结论**不成立**（rank-1 的命中数 3 → 2，MRR/nDCG 都略降，只有 recall@5 变好）。

## 4 差异表

| 项 | 作者读数 | 我的独立读数 | 差异 / 归因 |
| --- | --- | --- | --- |
| C1/C2/C9、逐类 recall、`kubernetes` rank 8、recall@5 0.9333 | 0.7333/0.8185/0.8621、rank 8、0.9333 | **逐位相同** | 无 |
| C4a / C4c / C5 / C6 库侧 / C8a / C8b | 20/20、Bigram、15/15、4 种量纲、活库有量程、PRESENT | **逐位相同** | 无 |
| §1.1 的语义距离摘要 | `answerable mean=0.1583 max=0.2263 \| no-answer mean=0.3269 min=0.2274` | 同一批查询、两件仪器、两个**不同的量**：<br>**(a) 语义腿最近距离**（live-copy 测试自己打印的）：`mean=0.1510 **max=0.1985** \| mean=0.3196 **min=0.2222**`（gap **0.0237**）<br>**(b) 融合 top-1 命中的 semantic_score**（我在产品路径上复算）：`max=**0.2263** \| min=**0.2274**`（gap **0.001100**） | **不是漂移，是同名不同量（F-2）**：max/min 与作者逐位相同，说明 §1.1 引用的是 (b)，而那一跑的仪器打印的是 (a)；gap 差 20×。两条我都能复现 |
| C3 / C7 | 未达 / 未测 | **deferred**（captain 裁决） | 不是差异，是裁决（§7） |
| 门禁 64 passed | 64 passed | 64 passed（其中 2 条未测也报绿） | 计数一致，**含义不同**（F-3） |
| gold 集 registry | 「Idempotent，重跑不重复」 | 跑 6 次 ⇒ **132 行 / 90 answerable**（集合只有 22 条） | **F-1** |
| **独立采样面**（我造，11 条，与冻结集不相交） | 未做；报告用 0.7333 表述召回质量 | 新 **0.2222 / 0.7778 / 0.7778**、MRR 0.4537、nDCG 0.5359；旧 **0.3333 / 0.6667 / 0.7778**、MRR 0.4974、nDCG 0.5661 | **F-4**：方向不一致（rank-1 变差、recall@5 变好） |
| `ScoreKind` 的序列化 | 报告按 `as_str()` 的 `"rrf_rank"` 叙述（且把它列为冻结字面量） | serde 实读 `"RrfRank"` | **F-5**：两个 wire 形状，无测试钉住 |

## 5 可证伪性复核（两棵树 + 构建树证明）

**我是怎么做的（没有碰共享工作树）**：`git archive HEAD` 导出 → `%TEMP%\va-old`（HEAD `0a39e5b801ea30cb8189cd6d10c4480ddfeedbfe`）；把**判据**（`tests/common/mod.rs`、`retrieval-gold-copy.rs`、`retrieval-legs.rs`、`retrieval-quality.rs`，这四个文件本身是**只用 pre-t7 API** 写的，注释里就写明这一点）与 `Cargo.toml` 复制进导出树；在导出树里用**独立 target** `%TEMP%\ruagent-cmp-va` 跑。**全程没有 `git stash`、没有 checkout 旧文件**（本代已因此误伤 mem-core 的 E0425 与 wiki 的 t10），我对仓库的写入只有本报告一个文件。

**两棵树的差异证明（构建树）**
| | 新树（工作树，未提交的 t7 改动） | 旧树（`git archive HEAD`） |
| --- | --- | --- |
| graph 之外的证据 | `crates/knowledge/src/{lib,rrf,store}.rs` 含 `LEG_WINDOW`/`ScoreKind`/`search_page`/`relevance_from_distance` | `crates/knowledge/src/*.rs` 对上述符号 **0 命中** |
| 测试文件 | 5 个新文件 + `tests/common/` | 只有 `retrieval-legs.rs`、`retrieval-quality.rs` 两个旧文件（判据由我复制进去） |
| target | `%TEMP%\ruagent-team-target`（全队共享，captain 纪律） | `%TEMP%\ruagent-cmp-va`（**独立**，closure §7.1） |

**旧树上的读数（我跑出来的原文，四条判据全部变红）**
```
[OLD TREE] cargo test -p ruagent-knowledge --test retrieval-gold-copy --no-fail-fast -Nocapture -TargetDir %TEMP%\ruagent-cmp-va
  thread 'live_copy_gold_set' panicked at crates\knowledge\tests\retrieval-gold-copy.rs:83:5:
  C1 not met: recall@1 = 0.4666666666666667 (baseline 0.4667, target 0.70)          → FAILED

[OLD TREE] --test retrieval-legs --test retrieval-quality --no-fail-fast
  thread 'han_substring_is_not_served_by_the_full_table_scan' panicked at retrieval-legs.rs:193:5:
  assertion `left != right` failed: a 2-character Han substring must be indexed, not scanned
    left: Substring   right: Substring                                             → FAILED
  thread 'the_leg_window_does_not_follow_the_page_size' panicked at retrieval-legs.rs:266:9:
  assertion `left == right` failed: the keyword leg's size depends on the page size for "kettle"
    left: 10   right: 12                                                           → FAILED
  thread 'retrieval_quality' panicked at retrieval-quality.rs:958:5:
  assertion `left == right` failed: every cjk-substring query must be indexed rather than scanned
    left: 0   right: 1                                                             → FAILED
```
**同一批判据在新树上（同一份副本、同一个真模型）**：C1 = **0.7333**、C4a = **20/20**、C5 = **15/15**、quality 的 `cjk_substring_not_scan` = **1/1** ⇒ **坏侧红、好侧绿，两侧都测到了**（不是单边断言）。

**第二条独立路径（产品路径，跑着 pre-t7 二进制的守护进程）**：`GET /api/v1/knowledge/search` 对冻结 15 条给出
```
METRICS frozen: n=15 recall@1=0.4667 recall@5=1.0000 recall@20=1.0000 MRR=0.6889 nDCG@10=0.7682
recall_log rows: before=651 after=651 (delta=0)
```
与规格 A3 的 HTTP 基线**逐位相同**，也与旧树上的库内仪器**逐位相同** ⇒ 作者报告里"三个数同时是一次独立交叉验证"的说法，我**从改前这一侧复现了**（两条仪器、同一批数）。

**我自己的采样面也做了两侧**（这是判据自证伪的地方）：新实现 `recall@1 0.2222 / recall@5 0.7778 / MRR 0.4537 / nDCG@10 0.5359`，旧代码 `0.3333 / 0.6667 / 0.4974 / 0.5661` ⇒ **「新的比旧的好」这条判据在我自己的采样面上失败**（见 F-4）。这正是可证伪性的用途：判据只在作者选的点上通过，就该被登记。

## 6 findings

**没有 blocker / high。** C 节 9 条目标我全部独立复现；三条 medium 都不推翻任何 KPI 的判决，但都会让**下一个人**读到错的结论、错的集合规模或被冻结点之外的 JSON 形状绊倒。复现命令写在每条里；我**没有替作者修任何代码**。

### F-4（medium · 采样面）在**独立采样的面**上，「新实现把 recall@1 从 0.4667 推到 0.7333」这条结论**不成立**

- 采样面（我造的，与冻结 22 条不相交，规则见 §3.5）：8 条 title-derived（机械取 `documents.name` 的末段）+ 1 条 content-derived + 2 条非词。
- 读数（9 条可答，**同一份真模型、同一份副本**）：

| | 旧代码（产品路径，pre-t7 daemon） | 新实现（我的探针，post-t7） | 差 |
| --- | --- | --- | --- |
| recall@1 | **0.3333** | **0.2222** | −1 条 |
| recall@5 | 0.6667 | **0.7778** | +1 条 |
| recall@20 | 0.7778 | 0.7778 | 0 |
| MRR | **0.4974** | **0.4537** | −0.044 |
| nDCG@10 | **0.5661** | **0.5359** | −0.030 |

- 逐条：`Slurm基本使用` 从 rank 0 掉到 rank 1（top-1 被同族文档 `…Slurm使用笔记-Slurm使用笔记` 抢走）；`Obsidian` 从 rank 6 升到 rank 3；`notes`/`17da3f` 在两侧都进不了 top-20。
- **结论怎么写才准确**：C1 的 metric 在规格里**就是定义在冻结 15 条上的**，所以「C1 达标（0.7333 ≥ 0.70）」依然成立；**不成立的是它的推广读法**（"召回质量整体被推到 0.7333"）。作者报告里没有把 0.7333 说成全集读数（§2.3 甚至主动写了"整体 0.7333 是被 mixed-cjk-ascii 这一档稀释出来的"），但**也没有第二张采样面**去检验方向。
- 影响：medium。下一轮（C3/C9 的分级 gold、λ 标定）如果继续只用这 15 条，会把一个**面向 title-derived 查询族**的改进当成通用改进；我的面上 `Obsidian` 这类"短名 + 多同族文档"的查询在新打分下也没变好多少。
- 复现：`va-probe`（见 §8）+ `python %TEMP%\va-http.py`（旧侧）。

### F-1（medium · 判据前置）`query_eval_gold` 的「幂等」注释不成立：集合每次运行都再写一遍

- **期望**（`tests/common/mod.rs:159-160` 的注释）：*"Idempotent, so re-running the instrument neither duplicates rows nor needs a fresh copy."*
- **实际**：schema 里 `query_eval_gold` 只有 `id INTEGER PRIMARY KEY`（`crates/store/src/migrations/0019_recall_quality.sql:59-70`），**没有 `UNIQUE(set_id, query)`**；seed() 用的是 `INSERT OR IGNORE`（`mod.rs:187`）——没有唯一约束时 `OR IGNORE` 什么都不忽略。
- **我的读数**（在我的副本上跑了 6 次仪器后，只读 SQL）：
  ```
  SELECT COUNT(*), SUM(answerable) FROM query_eval_gold   →  (132, 90)
  SELECT COUNT(DISTINCT query)                            →  22
  SELECT query, COUNT(*) … GROUP BY query                 →  每条 6 行
  SELECT set_id, COUNT(*) FROM query_eval_gold GROUP BY 1 →  (1, 132)     -- 集合本身只有 1 行
  ```
  即一个 22 条的冻结集合（15 可答 + 7 无答案）在表里变成 132 行 / 90 条可答。
- **影响**：本轮的 recall/MRR/nDCG **不受影响**（判据读 `GOLD` 常量，n 恒为 15）——这正是它没被抓住的原因。但 `query_eval_gold` 是「两次读数可比」的登记表，**任何按表读集合规模的消费者**（t19 遥测/面板、RV-INT 的核对、下一轮做 C3 的人）会把 15 读成 90；而 C3 的前置判据恰好是「集合 ≥60 条查询、≥20 条无答案」⇒ **按表读会让 C3 的前置条件假达标**。
- **复现**：`cargo-team.ps1 test -p ruagent-knowledge --test retrieval-gold-copy -Nocapture` 跑 6 次，然后 `python %TEMP%\va-gold.py`（只读 SQL）。
- **建议（不替作者修）**：给 `(set_id, query)` 加唯一约束，或 seed() 先按 set 清空再插；C3 的集合规模判据写成**去重后的行数**。

### F-5（medium · 冻结点之外的接口形状）`ScoreKind` 的 serde 形状是 `"RrfRank"`，而冻结的 wire 字面量是 `"rrf_rank"`

- 证据：`ScoreKind` 只有 `#[derive(…, serde::Serialize)]`（`crates/knowledge/src/store.rs:70`），**没有 `rename_all`**；我的探针实读：`score_kind serializes: "RrfRank"`，而 `ScoreKind::as_str()` 返回 `"rrf_rank"`（store.rs:85-96），守护进程手写的 JSON 也用 `"rrf_rank"`（`api.rs:3003`）。同一概念**两个 wire 形状**。
- 为什么重要：C6 的 JSON 半（5/5，H-1，owner=integ）最自然的实现就是直接 `serde` 序列化 `RankedHit`/`SearchPage` ⇒ 会把 `"RrfRank"` 发到面板/MCP，而 R-A C6 与 R-B D.7 都把 `"rrf_rank"` 列为**冻结字面量**（"new scales get new keys, never a rename"）。crate 里只有 `ResidualOrigin` 有「序列化字面量必须逐字符相等」的漂移钉（`tests/residual-scan.rs:40`），**ScoreKind 没有**（我搜了全部测试与源码，没有对 `ScoreKind` 的 serde 形式做任何断言）。
- 影响：medium（今天没有消费者走 serde 路径 ⇒ 不炸；但 H-1 一落地就会静默换名）。建议：给 `ScoreKind` 加 `#[serde(rename_all = "snake_case")]`（或逐变体 `rename`），并照 `ResidualOrigin` 的样子补一条序列化漂移钉。

### F-2（low · 报告出处）§1.1 的语义距离摘要引的是**同名的另一个量**（融合 top-1 的距离），不是那一跑打印的量（语义腿最近距离）

- 作者 §1.1 写：`semantic top-1 distance: answerable mean=0.1583 max=0.2263 | no-answer mean=0.3269 min=0.2274`。
- 我在**同一件仪器**（live-copy 测试，真模型，我自己的副本）上读到的是：`answerable mean=0.1510 max=0.1985 | no-answer mean=0.3196 min=0.2222`（gap 0.0237）。
- 我用**产品路径**（跑着 pre-t7 二进制的守护进程，`GET /api/v1/knowledge/search`）分别量了两个量：
  - (a) **语义腿最近距离**（对返回集取最小 `semantic_score`）= 上面那组（0.1985 / 0.2222）；
  - (b) **融合 top-1 命中的 `semantic_score`** = `max=0.2263`、`min=0.2274`、**gap = 0.001100** ⇒ **与作者 §1.1 逐位相同**。
- ⇒ 作者的数字没错，是**量的定义不同**：§1.1 引用 (b)，而那个代码块的仪器打印 (a)。影响是读者照报告复现时会得到"不一样"的数（gap 差 20×），进而怀疑仪器漂移；对 KPI 无影响。**C3 重设计时应当明确用 (a) 还是 (b)**——(a) 的可分性比 (b) 好 20 倍，这是有用信息。
- 复现：`python %TEMP%\va-c3.py`（(a)）与 `python %TEMP%\va-c3b.py`（(b)）。

### F-3（low · 门禁含义）`64 passed` 里有 2 条在环境变量缺失时**什么都没测**

- 证据：不设 `RUAGENT_IA_LIVE_COPY` 时 `retrieval-gold-copy`（`retrieval-gold-copy.rs:30-37`）与 `retrieval-gold-live`（`retrieval-gold-live.rs:25-32`）走 `NOT MEASURED` 分支直接 `return`，各自记 **1 passed**（用时 0.00s）；设了环境变量后同一个二进制要跑 **50s / 86s**，并会真失败（我用错误的缓存层级实测：`FAILED … recall@20 = 0`）。
- 影响：低。作者在报告 §7.1 写明了要设环境变量，`NOT MEASURED` 也会打印（但只有 `--nocapture` 可见）。要点是**计数不等于测量数**。
- 建议：gate 跑法里显式带上环境变量（或让 `NOT MEASURED` 走 `eprintln!`）。

### 观察（不进 findings）

- **O-1** 登记表语义不对称：`query_eval_sets` 幂等（1 行）、`query_eval_gold` 不幂等（每次 +22），`query_eval_runs` 是「一次 gold-live 记一行」。我的副本里唯一那行 run 的 `embedder` 写着 `ABSENT -- …`、metrics 全 0 —— 那是**我自己**第一次用错缓存层级的产物，不是作者的；`embedder` 字段正好让它可识别（这是该 schema 设计对了的地方）。
- **O-2 C6 的 JSON 半我读代码确认仍未接线**（作者记 not_measured 是**准确**的，不是少报）：`api.rs:3003/3006/3008` 的行级 `score_kind` 恒为 `"rrf_rank"`，`semantic_score`/`keyword_score` 没有 kind；memory 侧 `api.rs:2581-2582` 同样。
- **O-3 C6(b) 的具体形状**（给 integ 当 H-1 的输入）：旧产品路径的 JSON 里 `score`（rrf_rank，越大越好）与 `semantic_score`（距离，越小越好）并排在同一对象里，而 `score_kind` 只描述行级 ⇒ 「两种量纲并排且没有 kind 可读」在 JSON 层仍成立。
- **O-4** 我的采样面上 `notes`、`17da3f` 两条在**新旧两侧**都进不了 top-20（`17da3f` 的 keyword stage = `Empty`、`notes` 是极泛词）⇒ 「候选召回没问题、坏的是排序」这句子在被验的 15 条上成立（misses=0），在机械采样的面上至少对这两条是"候选也缺"；它们更像**查询构造太弱**（把目录段/哈希段当查询）而不是检索缺陷。登记为采样面的边界。

## 7 未测项（含 captain 裁决的 deferred）

| 项 | 状态 | 原因 / 前置数据（我复核过） |
| --- | --- | --- |
| **C3** 无答案判据 ≥0.85 | **deferred（captain 裁决）** | 判据前置是「**≥60 条查询、其中 ≥20 条无答案**」，且需要 `judged_by='human'` 的人工分级 gold。我复核数据前置：`SELECT judged_by, COUNT(*) FROM query_eval_gold` ⇒ **只有 `title-derived`，`human` 0 行**；冻结集 **22 条**（15 可答 + 7 无答案），距 60 条不到一半。两个量的可分性我都量了：(a) gap **0.0237**、(b) gap **0.001100** —— 坐标轴上有信号，但**任何切点都不可标定**；规格明文禁止把 0.227 写成常量。⇒ **本代无法判定**，不是 I-A 的失败。 |
| **C7** 14/14 | **deferred（captain 裁决）** | 指标**循环**：gold 是 title-derived 的，用一个只匹配 `documents.name` 的重排**按构造**就是 14/14（规格 C7 自己写明"不能当 headline"，§E7 又写"本轮不设达标线"）。我的独立复算：**11/14 = 0.7857**（miss：`docker 常用命令`、`Springboot 热部署`、`ChatGPT提示词`）。⇒ 指标待重设计（需要内容派生的 gold）。 |
| **C4b** 产品路径 p50 ≤1 ms | **not_measured（作者登记，我复核理由成立）** | 产品路径每次查询含真模型 query 嵌入（实测 ≈380 ms 量级），不是 LIKE 扫描的成本；store 级 0.08 ms/40-40 是 V-SCHEMA 已复现的读数。我没有独立复测产品路径（需要构造成本仪器），**理由成立**，不是掩盖。 |
| **C6 的 JSON 半（5/5）** | **not_measured（owner = integ / H-1）** | 我只读代码确认现状（O-2/O-3），没有起守护进程、没有改 `api.rs`（不在本单 inScope）。 |
| 真模型 harness 的 **entity 腿** | **ABSENT（设计如此）** | `retrieval-quality.rs` 的 JSON 里 `leg_status_note` 明写 entity 腿在 `crates/graph`，本 crate 读不到；我确认它被**具名**登记而不是静默跳过。 |
| 真守护进程的 **`/api/v1/recall`** | **未用（任务禁止）** | 我用的是 `GET /api/v1/knowledge/search`（只读）；且我逐次核对 `recall_log` 行数 `before=651 after=651 (delta=0)`。 |
| 合成 harness 的"量程" | **仍然饱和** | `[t245/a]`、`[t245/b]` 三个指标都是 1.0000（我复跑确认，且 byte-identical）⇒ 有量程的是**活库切片**，不是这个合成 harness；作者没有假装它变了。 |

## 8 附录：收尾哈希 + 复现命令

**收尾复核**：§1 的 11 个被验文件在收尾时重新哈希，与快照（01:05:43）**逐位相同**（见收尾段）。活库 mtime 仍是 `2026-09-27T21:15:21.078+08:00`，`recall_log` 我每次核对都是 **651 行**（HTTP 探针前后 `before=651 after=651`）⇒ 整个验证没有写活库、没有用 `/api/v1/recall`。

**复现命令**（`-Nocapture` / `-TargetDir` 是 `scripts/cargo-team.ps1` 的开关；`--nocapture` 需要传给测试时写在 `--` 列表里，否则会被脚本追加的第二个 `--` 吃掉）：

```powershell
$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"
# 门禁（注意：不设 RUAGENT_IA_LIVE_COPY 时两个活库仪器会"未测也 pass"，见 F-3）
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-knowledge
# 我的副本 + 我的模型缓存（源一律只读）
$va="$env:TEMP\va-live"; New-Item -ItemType Directory -Force "$va\data","$va\knowledge" | Out-Null
python -c "import sqlite3,os;con=sqlite3.connect('file:'+os.path.expanduser('~/.ruagent/data/ruagent.db').replace('\\','/')+'?mode=ro',uri=True);con.execute('VACUUM INTO ?',(os.path.join(os.environ['TEMP'],'va-live','data','ruagent.db'),));con.close()"
Copy-Item $env:USERPROFILE\.ruagent\data\lancedb "$va\data\lancedb" -Recurse; Copy-Item $env:USERPROFILE\.ruagent\knowledge\* "$va\knowledge\" -Recurse
New-Item -ItemType Directory -Force "$env:TEMP\va-hf" | Out-Null; Copy-Item $env:USERPROFILE\.ruagent\models\hub\* "$env:TEMP\va-hf\" -Recurse   # HF_HOME=...\va-hf
# 逐目标
$env:RUAGENT_IA_LIVE_COPY="$env:TEMP\va-live"; $env:HF_HOME="$env:TEMP\va-hf"
... test -p ruagent-knowledge --test retrieval-gold-copy -Nocapture     # C1/C2/C9/C8a（真模型；见 F-1 的登记表副作用）
... test -p ruagent-knowledge --test retrieval-gold-live -Nocapture     # C4/C5/C6（真模型）
... test -p ruagent-knowledge --test retrieval-quality   -Nocapture     # C8a 的合成 harness（仍饱和）
New-Item -ItemType Directory -Force "$env:TEMP\va-probe\src" | Out-Null   # 我自写的探针（仓库外）：见 §3.5/§4
... run --manifest-path "$env:TEMP\va-probe\Cargo.toml" --bin va-probe  # 我的采样面 + 我自己的指标实现 + C5 + C6
# 改前：产品路径（跑着 pre-t7 二进制的守护进程，只读 GET）
python "$env:TEMP\va-http.py"    # 冻结 15 条 + 我的 11 条 在旧代码上的 rank/指标
python "$env:TEMP\va-c3.py"      # C3 的 (a) 量；python "$env:TEMP\va-c3b.py"  # C3 的 (b) 量
# 改前：旧代码树（git archive，独立 target；绝不 git stash / checkout 共享树）
git archive --format=tar -o "$env:TEMP\va-old.tar" HEAD; New-Item -ItemType Directory -Force "$env:TEMP\va-old" | Out-Null; tar -xf "$env:TEMP\va-old.tar" -C "$env:TEMP\va-old"
Copy-Item crates\knowledge\Cargo.toml "$env:TEMP\va-old\crates\knowledge\Cargo.toml"
New-Item -ItemType Directory -Force "$env:TEMP\va-old\crates\knowledge\tests\common" | Out-Null
foreach($f in @('common\mod.rs','retrieval-gold-copy.rs','retrieval-legs.rs','retrieval-quality.rs')){ Copy-Item "crates\knowledge\tests\$f" "$env:TEMP\va-old\crates\knowledge\tests\$f" -Force }
cd "$env:TEMP\va-old"
... -File <repo>\scripts\cargo-team.ps1 test -p ruagent-knowledge `
      --test retrieval-gold-copy --test retrieval-legs --test retrieval-quality `
      -Nocapture -TargetDir "$env:TEMP\ruagent-cmp-va"
# 登记表取证（只读）
python "$env:TEMP\va-gold.py"    # COUNT(*), SUM(answerable), judged_by, runs…
# 背景
git status --porcelain -- crates panel
```

### 8.1 收尾复核（2026-09-28T01:41:35+08:00）

**§1 的 11 个被验文件在收尾时重新哈希，与快照（01:05:43）逐位相同** ⇒ §2–§6 的读数绑定在同一份字节上：

```
crates/knowledge/Cargo.toml                   2026-09-27T23:52:22  984A2C6E2B286F39
crates/knowledge/src/chunk.rs                 2026-09-15T01:23:12  5B1082E9B5ECEE31
crates/knowledge/src/embed.rs                 2026-09-16T21:19:28  4DC61A592046A427
crates/knowledge/src/fast.rs                  2026-09-16T21:25:54  E3E0AF9F66D25DE9
crates/knowledge/src/files.rs                 2026-09-26T19:17:08  93661C0FBDCB08E1
crates/knowledge/src/lib.rs                   2026-09-28T00:27:01  9884EDF3D30F7345
crates/knowledge/src/rrf.rs                   2026-09-28T00:28:49  7EA083F8589EA51C
crates/knowledge/src/store.rs                 2026-09-28T00:34:56  EDF4FC81240C0C4A
crates/knowledge/tests/common/mod.rs          2026-09-28T00:01:52  BC110110757F3A69
crates/knowledge/tests/residual-scan.rs       2026-09-27T23:52:10  646D20A3CF82B75C
crates/knowledge/tests/retrieval-cjk.rs       2026-09-28T00:44:13  216B397FF387E6F7
crates/knowledge/tests/retrieval-gold-copy.rs 2026-09-28T00:02:00  99C891634B6AAE5C
crates/knowledge/tests/retrieval-gold-live.rs 2026-09-28T00:51:07  C570379CA868C666
crates/knowledge/tests/retrieval-legs.rs      2026-09-28T00:02:41  B4EE8CA75D33D2A1
crates/knowledge/tests/retrieval-quality.rs   2026-09-27T23:58:44  56CAC06E9BD2CAA6
```
**活库未被我写入**：`~/.ruagent/data/ruagent.db` mtime 仍是 `2026-09-27T21:15:21.078+08:00`、14,946,304 B，`recall_log` = **651 行**（HTTP 探针前后 `before=651 after=651`）。

**背景读数（只作背景，不写 finding）**：收尾 `git status --porcelain -- crates panel` = **47 项**，全部是并发波次里同伴的在途改动（store/knowledge/memory/graph/daemon/mock-agent）。被验对象的归因只用 mtime + 作者 `changedPaths`（§1）；**我自己的写入只有 `docs/design/reviews/gen2-recall-verify.md` 一个文件**（`git status --porcelain -- docs` 里只有它，且 `crates/knowledge` 的全部哈希与读数时逐位相同）。
