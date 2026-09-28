# RV-C round 3 评审（t39）：t38 的四条 deferred 登记 / 三类勘误 / 重取读数逐条判决

> **结论：`pass`**（attempt `09e06dee`）。这是**对「本单 finding 是否全部收口 + 登记是否如实」的判决，不是把四条 deferred 判成达标** —— 那四条按 captain 裁决**仍是未达成**，只是本代不具备测量条件，已按四要素显式登记（§2）。
> **本单关键的三句核对（captain 点名的）**：
> 1) **四要素 + 「仍是未达成」的字面**：规格 **§C·G7 L136** 与**附录 H.3 L733** 都逐字写着「**标 deferred 的四项仍是「未达成」，不是达标**」「**不得**被下游读成达标或通过」⇒ **没有被写成达标、也没有含糊化**（§2.1）。
> 2) **G5 的根因是否成立**：**不成立** —— 我在导出副本里**把那两处 tie-break 删掉**（把 RVC-4 的修复反做），**3/3 次**跑出的仍是 `0.7500/0.8000`、中位 `9`、`{None 1, Hops 6, MaxPaths 13}`，与交付树逐位相同；删掉 tie-break 唯一复现出来的是**原来的不可复现缺陷**（`6 runs ... byte-identical: false`，长度 13756/13701/13680/13738/13719/13667）。⇒ 新数字是**对的且可复现**，但「排序 tie 吃掉了预算」这个**根因分类是错的**：真因是「读数没在交付那一版上重取」（与 G2 同类），旧数字在反做后的代码上**不可复现**（RV-C3-1）。
> 3) **G2 的根因是否成立**：**成立，且我自己重取到了 46** —— 我在同一棵树上按帧逐腿汇总：`RV-C3 league totals over the frame: {"ExactName": 1, "NameToken": 8, "SummaryFts": 46} (sum 55)` ⇒ 报告里的 `39` 是旧读数，`46` 才是当前字节 ⊕（`non-empty: old(strict) 1 -> new 13` 同一次跑里也复现）。
> **契约命令（我自己的运行）**：`test -p ruagent-graph` = **34 passed / 0 failed / 3 ignored**（11 个 target 逐个绿，exit 0）。
> **评审员**：review（独立评审员，**不是** I-C 的作者；仓库里我只写本文件，`crates/` 与 `panel/` 一行未改；反做实验只在 `%TEMP%` 的导出副本 + 独立 target 里做）。
> **输入位置**（按 captain 更正）：`gen2-graph-spec.md` 附录 H.2/H.3 + §C 就地标注 · `gen2-graph-impl.md` §13/§14（t38 登记；repair 报告已回退到 t27 交付态，**那不是静默改写**）· 重取读数（G5/G2）。
> **时间窗**：2026-09-28T05:0x → 05:4x +08:00。**真守护进程 pid 79984 未启停**；`~/.ruagent` 只读；**未用** `GET /api/v1/recall`；共享工作树与共享 target 未被我的探针触碰。

---

## 1 我自己的读数（含一个受控反做实验）

### 1.1 门禁（共享 target，交付字节）

```
scripts/cargo-team.ps1 test -p ruagent-graph   → exit 0
  11 个 target：3 / 1 / 3 / 7 / 0 / 0(3 ignored) / 7 / 3 / 5 / 5 / 0 ⇒ 34 passed / 0 failed / 3 ignored
```

### 1.2 G5：交付树 vs「反做 tie-break」的受控对照（**我自己的实验**）

| 树 | `path-recall@12` | facts 中位 | `truncated_by` | `6 runs` 逐字节 |
| --- | --- | --- | --- | --- |
| **交付树**（含 tie-break） | `0.7500 / 0.8000` | `9`（min 3 / max 12） | `{None 1, Some(Hops) 6, Some(MaxPaths) 13}` | **true**（6 × 13696 B） |
| **我的副本：删掉两处 `.then_with(edge_seq)`**（= 反做 RVC-4） | `0.7500 / 0.8000`（**3/3 次**） | `9`（3/3 次） | `{None 1, Some(Hops) 6, Some(MaxPaths) 13}`（3/3 次） | **false**（13756/13701/13680/13738/13719/13667） |
| 报告里的旧值（**现标不可引用**） | `0.7000 / 0.7500` | `7` | `{Hops 4, MaxPaths 15, None 1}` | —— |

* 实验做法：`robocopy` 导出当前树到 `%TEMP%\rvc3-tree` → 在**副本**里删除 `crates/graph/src/retrieve.rs:931` 与 `:957` 两行（t27 为 RVC-4 追加的 tie-break 键）→ `-TargetDir %TEMP%\ruagent-cmp-rvc3` 独立编译/运行（共享 target 未被污染）。
* **判**：删掉 tie-break **不复现**旧数字（3/3 次都与交付树逐位相同），只复现**不可复现性**本身 ⇒ t38 的根因分类「排序 tie 改变了并列路径进入 `truncate(beam/max_paths)` 的顺序 ⇒ 预算不再被任意一组并列路径吃掉」**不成立**。真因更简单也更常见：**旧数字没在交付那一版上重取**（同 G2）。⇒ **RV-C3-1**。

### 1.3 G2：我按帧逐腿重取（同一棵树）

```
frame: 23 queries
RV-C3 league totals over the frame: {"ExactName": 1, "NameToken": 8, "SummaryFts": 46} (sum 55)
non-empty: old(strict) 1 -> new 13
```
⇒ **`summary_fts = 46` 我独立复现**（报告旧值 39），合计 55 与 t9 §1 的「55 种子」同口径（差异只在 FTS 这一条腿）⇒ **G2 的根因分类成立**（其余两腿 1/8 逐位相同，对象集与解析逻辑两次一致）。

### 1.4 G7 / G8 / G4 / G3（我自己的运行）

```
G7 AFTER: replay of 60 frozen current edges -> written 47, duplicate 11, refused 2 | labels {"duplicate":2,"mislabeled":2,"supported":43}
G7 AFTER: relation-level precision (strict) = 43/47 = 0.9149  (BEFORE: 45/60 = 0.7500)
G7 ontology direction: distinct relation names 35 -> 29, object-specific (ad_hoc) 6 -> 3
G8 falsifiable structure: largest community 14/43 = 0.3256 (target <= 0.50) | isolated 20/63 = 0.3175 (target <= 0.40)
partition: 9 communities, sizes [14,5,3,3,2,6,2,4,4], split_by_modularity=1 | summaries written: 1 of 9
as_of ruagent at … -> MATCH  6/6（8/8、17/17、9/9、17/17、10/10、17/17 条自有边；orphan edges []）
TrueAsOf: 1 of the 1 true-then edges came back; 1 of 9 rendered lines say true_as_of
direction check: 40 paths / 26 reverse hops, all in the stored direction
```

---

## 2 §C 逐条重判（**四条 deferred 仍是未达成**）

### 2.1 登记形状核对（captain 点名的字面）

| 核对项 | 我在文本里看到的 | 判 |
| --- | --- | --- |
| **「仍是未达成，不是达标」的字面** | 规格 **§C·G7 L136**：「**t38 逐行裁决：本代能判的已判，不能判的显式 `deferred` —— 标 deferred 的四项仍是「未达成」，不是达标**」；**附录 H.3 L733**：「**它们仍然是「未达成」**：`deferred` 表示「本代不具备测量它的数据/跑批条件」，**不得**被下游读成达标或通过」 | **在，且两处都逐字写明**（没有被写成达标、没有含糊化） |
| **四要素（目标原文位置 / 为什么测不了 + 证据 / 下一代前置 / owner 角色）** | H.3 的表 L735–L740：四条各一行，四列齐全（L737 实体族；L738 关系 R + 幻觉率；L739 `extracted ≥30%`；L740 G8 三指标） | **齐** |
| **勘误指针（G3/G4/G8 三行 + G7 的 gold 路径）** | H.2 表 L723–L727（原命令保留删除线）+ §C 各行就地勘误指针（L109/L116/L142/L143/L150） | **齐**，且**不新增空壳 target**（L729 写明理由） |
| **登记落点** | impl §14.1–14.5（L400–L430），repair 报告已回退到 t27 交付态（captain 已确认这是**恢复**不是静默改写） | **齐** |

### 2.2 逐条目标

| G | 判决 | 我的独立读数依据 |
| --- | --- | --- |
| **G1** 实体腿不再恒空 | 腿级**达成**；生产端 **not_measured**（owner=INT/t19，excused） | `non-empty: old(strict) 1 -> new 13`（我按帧重跑）+ `>=13/23` 断言在跑；生产端仍是 `api.rs` 的 strict 入口（DEP-1） |
| **G2** 逐腿归因 | **达成** | 我重取：`{"ExactName": 1, "NameToken": 8, "SummaryFts": 46}`（含 0 的腿也在证据里）；报告旧值 39 已标不可引用（RV-C2-5 关闭，根因成立） |
| **G3** 每条新边带来源 | **达成** | H.2 的勘误指向真实 target；`test -p ruagent-graph` 全绿含 `the_old_writer_leaves_the_source_unknown_and_the_new_one_records_it`（temporal 5 passed）；边写入侧读数见 RV-C r2 我自己的 `edges: […Some("extracted")…]` |
| **G4** 事件时间 ≠ 记录时间 | 时点查询**达成**；`extracted ≥30%` **未达成 · deferred** | `as_of … -> MATCH` **6/6**、`orphan edges []`（我本轮的运行）；`event_time_source` 两态在读数里；30% 一行按 H.3 第 3 条登记 |
| **G5** 多跳检索 | **达成** | `0.7500 / 0.8000 ≥ 0.60`、中位 `9 ≥ 3`、截断必报、`6 runs` 逐字节相同、方向 40/26 全对（我本轮运行）；**根因分类不成立 ⇒ RV-C3-1** |
| **G6** 消解与别名 | **达成** | 全绿 3 passed（resolution）；具体数字用我 RV-C r2 的独立运行（`TP=7 FP=0 FN=0 TN=16`、`P/R=1.0000`、真库冗余 7→0、`queued:` 可见） |
| **G7** 抽取质量 gold | 关系 P **达成**；实体族 P/R · 关系 R · 幻觉率 **未达成 · deferred** | `43/47 = 0.9149 ≥ 0.80`（我本轮运行）；本体 35→29、ad_hoc 6→3；四条按 H.3 登记 |
| **G8** 社区层 | 结构侧**前置条件** + 两条可证伪指标 **达成**；三指标 **未达成 · deferred**；全局注入 **not_measured**（owner=INT/t19） | `largest community 14/43 = 0.3256 (target <= 0.50) | isolated 20/63 = 0.3175 (target <= 0.40)`（阈值与读数同一行打印 ⇒ **可失败**）；`communities=9 covered=43 non_isolated=43`、`summaries 1 of 9` |
| **G9** distill 三态 | **达成** | 我用 RVC r2/r3 同一批读数：三态 `[empty, failed, ok]`、`attempts=4 rows=4 rows-attempts=0`、失败带 reason、失败不覆盖成功；t38 未改 `distill.rs` |
| **G10** 图证据进注入 | **not_measured**（owner=INT/t19 + mem-core，excused；终判 t20/t21） | 生产注入点在 `api.rs`/`runs.rs`；任务禁止启停 79984、禁止用 `/api/v1/recall` |

**四条 deferred 登记（它们**不是**达标）**：① G7 实体族 P≥0.85/R≥0.70（前置=人工标注的工程实体族 gold，owner 角色=数据标注）② G7 关系 R≥0.60 + 幻觉率≤0.10（活库 0/67 历史边有 `source_episode`；前置=一次真实重蒸馏跑批）③ G4 `extracted ≥30%`（并入 G7 数据待办）④ G8 三指标 ≥60%（前置=LLM 判决跑批 + 对照组）。**选项 (i)（给 20 段手工标注会话具名一个成员/任务）仍开着** —— 按 captain 裁决，**它的前置是人类标注，我不把「captain 未具名」记成 I-C 的失败**。

---

## 3 findings

### RV-C3-1（low · 诊断准确性）impl §14.3 给 G5 的根因分类不成立
* **doc 原文**（`gen2-graph-impl.md:406`）：「**排序 tie**：RVC-4 的 tie-break（排序键追加边 id）改变了并列路径进入 `truncate(beam/max_paths)` 的顺序 ⇒ 预算不再被任意一组并列路径吃掉。方向自洽：`MaxPaths 15→13`、`Hops 4→6`、recall +0.05 两级、中位 `7→9`」。
* **我的反做实验（§1.2）**：在导出副本里删掉那两处 tie-break 键（= 反做 RVC-4）后，**3/3 次**仍是 `0.7500/0.8000`、中位 `9`、`{None 1, Hops 6, MaxPaths 13}`；唯一被复现的是**不可复现性**（`byte-identical: false`，6 个不同长度）。⇒ tie-break **不是** G5 数字变化的原因；旧数字在反做后的代码上**也不可复现**。
* **正确的分类**（与 G2 同类）：**读数没在交付那一版上重取**（G2 我已独立复现 46，成立）。**这不影响 G5 的判决**（新数字是我反复复现的真读数，target ≥0.60 达标）。
* **requiredFix**：把 §14.3 表里 G5 三行的「根因」列改成「**读数未在交付版上重取**（反做 tie-break 的对照实验：3/3 次与交付树逐位相同，只复现不可复现性）」，并保留「残留弱点：13/20 报 `Some(MaxPaths)` ⇒ 该 recall 是**预算下的读数**、不是质量上限」这一句（这句我给肯定：它正是 RV-C2 的 U-5 形状）。**不要**为此重开 t38 的判决。

### 观察（不进 findings）
* **O-1 四条 deferred 的登记形状合格**：两处逐字写明「仍是未达成、不得读成达标」（§C·G7 L136 / H.3 L733）+ 四要素齐；`owner` 是**角色**而非具名成员，但 captain 已把该选项的前置（人类标注）交给用户，故不计失败面。
* **O-2 G8 的两条新指标是可证伪的**（阈值与读数同一行打印，`0.3256 ≤ 0.50` 与 `0.3175 ≤ 0.40`）；其中「孤立比例」余量较薄（0.3175 vs 0.40），若下一代数据一变就可能触线 —— 这**正是可证伪**的样子（不是构造性满分），无需改。
* **O-3 t38 的两条仪器缺陷（`live-after.rs` 共用副本 + `VACUUM INTO` 目标已存在）我按 captain 的范围不判**（已立 t42，owner=graph，属可复现性）；我自己的实验已避开这两条（新导出目录 + 一次跑一个 target）。
* **O-4 载体纪律**：t38 的登记落 impl §14 而不是 repair 报告（后者已回退到 t27 交付态）—— 这是 inScope 白名单下的**正确恢复**，不是静默改写；我判 RV-C r3 时按 captain 给的三处输入读。
* **O-5 RV-C2-9 的责任划分已登记**（impl §14.5：mem-core 确认「同意」并给措辞加强；条款文字归 mem-core 的 t41，graph 保留 `void_episode` 与「episodes 计数不变」的断言）⇒ 我 RV-C2-9 的 requiredFix（写成一致性裁决 + mem-core 确认）**满足**。

---

## 4 关闭表：RV-C2 的九条（本轮的收口核对）

| # | 我的 requiredFix | t38 的落地 | 判 |
| --- | --- | --- | --- |
| **RV-C2-1** 实体族 P/R 无读数无具名 owner | 具名 owner **或**规格显式 deferred | H.3 第 1 条（四要素 + 「仍是未达成」） | **关闭**（仍**未达成**，已登记） |
| **RV-C2-2** 关系 recall/幻觉率 | 同上 | H.3 第 2 条 | **关闭**（仍**未达成**） |
| **RV-C2-3** `extracted ≥30%` | 同上 | H.3 第 3 条（并入 G7 数据待办） | **关闭**（仍**未达成**） |
| **RV-C2-4** G8 三指标 | 同上 | H.3 第 4 条 | **关闭**（仍**未达成**） |
| **RV-C2-5** 报告数字不可复现 | 换可复现读数 | G5 三行 + G2 一行两侧并排；我逐位复现（§1.2/§1.3） | **关闭**（根因分类见 RV-C3-1） |
| **RV-C2-6** 24/35 vs 26/35 对象集 | 两个对象集分开写 | impl §14.3 末段（全 67 = 24/35、current 60 = 26/35，已复算） | **关闭** |
| **RV-C2-7** G8 结构判据按构造必真 | 降为前置条件 + 可证伪指标 | §C·G8 L149 明写「按构造必真」+ 两条新指标（我读到读数） | **关闭** |
| **RV-C2-8** 三条命令指向不存在的 target | 扩勘误 | H.2 表 + §C 就地指针，**不新增空壳 target** | **关闭** |
| **RV-C2-9** 一致性登记 | 写成裁决 + mem-core 确认 | impl §14.5（mem-core message `cd4f804d`「同意」+ 措辞加强；条款归 t41） | **关闭** |

---

## 5 未测 / owner 清单（**不据此判 I-C 失败**，终判 t20/t21）

| # | 项 | owner | 为什么只能在它之后测 |
| --- | --- | --- | --- |
| U-1 | G1 生产端到端（`recall_log.entities>0` / 召回响应实体腿） | **INT/t19**（DEP-1） | 生产点在 `api.rs`；任务禁止启停 pid 79984、禁止 `/api/v1/recall` |
| U-2 | G8 注入块 2/2（`<graph>` 第三层） | **INT/t19** | 同上 |
| U-3 | 四条 deferred（实体族 / 关系 recall + 幻觉率 / `extracted ≥30%` / G8 三指标） | 数据标注 · 跑批 · 评测（**选项 (i) 开着**） | 缺人工标注语料 / 一次真实重蒸馏 / LLM 判决 harness —— 属**可测量性**，不是判据放宽 |
| U-4 | G5 的「预算上限」对照（放大 `max_paths` 后的 recall） | graph（下一代） | 13/20 报 `Some(MaxPaths)`；已在 §14.3 残留弱点里写明 |
| U-5 | 旧代码两棵树的反证 | **我本轮做了半件事** | 我给的对照是「反做 tie-break」（§1.2），不是整棵旧树；结论已足以否掉根因分类 |

---

## 6 复现命令（我实际跑的）

```powershell
$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"    # 全队包装脚本；共享 target 未自设
scripts/cargo-team.ps1 test -p ruagent-graph                                      # 34 passed / 3 ignored，exit 0
scripts/cargo-team.ps1 test -p ruagent-graph --test multihop-gold -Nocapture        # G5 三行 + 6 runs 逐字节 true
scripts/cargo-team.ps1 test -p ruagent-graph --test extraction-gold -Nocapture      # G7 43/47 · G8 两条可证伪指标 · summaries 1 of 9
scripts/cargo-team.ps1 test -p ruagent-graph --test temporal -Nocapture             # as_of 6/6 MATCH · orphan [] · TrueAsOf 1/1
scripts/cargo-team.ps1 test -p ruagent-graph --test seed-resolution -Nocapture      # frame 23 · non-empty 1 -> 13
# 受控反做实验（只在导出的副本里，共享树一行未动）：
robocopy <repo> %TEMP%\rvc3-tree /E /XD target .git node_modules .agent-teams
#   在副本删掉 retrieve.rs 的两处 .then_with(|| edge_seq(a).cmp(&edge_seq(b)))（反做 RVC-4）
#   副本内为 seed-resolution.rs 的帧测试加一行逐腿汇总（复用文件自身的 LIVE_QUERY_FRAME 常量，不复制常量）
cd %TEMP%\rvc3-tree
scripts\cargo-team.ps1 test -p ruagent-graph --test multihop-gold    -TargetDir %TEMP%\ruagent-cmp-rvc3 -Nocapture   # 3 次：0.7500/0.8000 · 中位 9 · {None 1, Hops 6, MaxPaths 13} · byte-identical false
scripts\cargo-team.ps1 test -p ruagent-graph --test seed-resolution  -TargetDir %TEMP%\ruagent-cmp-rvc3 -Nocapture   # {"ExactName":1,"NameToken":8,"SummaryFts":46} (sum 55)
git status --porcelain -- crates panel    # 我唯一新增 docs/design/reviews/gen2-graph-review-r3.md
```

---

## 7 纪律回执

* **只写本单 inScope**：本文件 `docs/design/reviews/gen2-graph-review-r3.md`。`crates/`、`panel/`、以及被评审的 spec/impl 报告**一行未改**；反做实验只在 `%TEMP%\rvc3-tree` 副本 + 独立 target `%TEMP%\ruagent-cmp-rvc3`。
* **读数三件套**：对象集（交付字节 + 冻结快照 63 实体/67 边；副本的同一批目标文件）· 采样面（11 个 graph target + G5 的 3 次重复 + 帧逐腿汇总 + 反做对照）· 可证伪判据（反做后**不复现**旧数字 ⇒ 根因分类可被否；G8 两条阈值同行打印；`6 runs` 逐字节相同）。
* **期望值只写一处**（规格 §C/H.3 与本报告 §2），判据从代码与测试取；副本里的汇总探针**复用文件自身的 `LIVE_QUERY_FRAME` 常量**，没有复制常量。
* **未测一律写明**（§5，5 项），A 类具名 owner=INT/t19 的**不据此判 I-C 失败**；B 类（四条 deferred）按 captain 裁决记「**未达成但显式 deferred**」，**没有**被判成达标或被含糊化。
* **真守护进程 pid 79984 未启停**；`~/.ruagent` 只读；**未用** `GET /api/v1/recall`；未在真库上跑任何写操作。
* **并发声明**：`git status --porcelain -- crates panel` 仍是并发波次；我只在读数受影响时报「哪个文件、什么错」，不替同伴改。
