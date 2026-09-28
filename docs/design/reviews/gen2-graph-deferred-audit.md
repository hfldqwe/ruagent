# I-C 账目补全审计：四条 deferred 与三条勘误命令（t49）

> **性质**：**账目补全 + 行号复核**，不是重做工作。t40 是 captain 为「RV-C r2 卡住的两件事」立的单，发现平台已自动补出内容重复的 t38 后取消（避免同一份规格被追加两次）——做法正确，代价是这两条用户约束在账上挂在已取消的 t40 上。本单用**行号证据**把它们落到 t38（落地）/ t39（复核）上。
> **只读**：只写了本文件；`gen2-graph-spec.md`、`gen2-graph-impl.md`、`crates/**`、`panel/` 一字未改。未写活库、未启停 pid 79984。
> **日期**：2026-09-28。被核对象 = 当前工作树（`gen2-graph-spec.md` 742 行）。

## 0 结论（一句话）

**两条用户约束都已由 t38（落地）+ t39（独立复核，verdict=`pass`）的证据支撑**，本单复核到行号，未发现任何地方把四条 deferred 写成达标/通过，也未发现任何地方把「角色 owner」写成具名成员。
支撑证据：**§C·G7 L136** 与**附录 H.3 L733** 逐字写明「仍是「未达成」，不是达标」· **H.3 L735–L740** 四条各 5 格（五个字段无一为空，机械核过）· **H.2 L719–L729** 三条勘误给出真实 target 且**本轮实跑复核通过**（G4 `as_of … -> MATCH` **6/6**、G8 `communities=9 covered=43`）· 反向 grep 全树 `deferred`（31 处）与 `达标/通过`（分类见 §4）**无一处把四条写成达标**。

## 1 约束一：不可测量的目标必须显式 deferred 并点名前置

### 1.1 字面证据（逐字引用）

| 位置 | 逐字原文 | 判定 |
| --- | --- | --- |
| `gen2-graph-spec.md` **L136**（§C·G7 target 行首） | 「**t38 逐行裁决：本代能判的已判，不能判的显式 `deferred` —— 标 deferred 的四项仍是「未达成」，不是达标**，依据与前置见附录 H.3」 | ✅ 逐字含「仍是「未达成」，不是达标」 |
| `gen2-graph-spec.md` **L733**（附录 H.3 开头） | 「…这四条按**可测量性**显式 `deferred`…**不是判据放宽**。**它们仍然是「未达成」**：`deferred` 表示「本代不具备测量它的数据/跑批条件」，**不得**被下游读成达标或通过。」 | ✅ 第二处、语义更强（「不得被下游读成达标或通过」） |
| `gen2-graph-impl.md` **L377**（§14.1 标题） | 「### 14.1 §C 四条目标的显式 `deferred` 裁决（**仍是未达成，不是达标**）」 | ✅ 第三处在位 |
| `gen2-graph-impl.md` **L388** | 「**并逐字写明「它们仍是未达成」**——不许被读成通过」 | ✅ 声明了这条纪律 |

**验证命令（契约指定，逐字跑过）**：
```powershell
powershell -NoProfile -Command "Select-String -Path docs/design/reviews/gen2-graph-spec.md -Pattern '未达成' -CaseSensitive | Select-Object LineNumber, Line"
```
读数：**恰好 2 行，L136 与 L733**（无第三处、也没有缺行）。

### 1.2 四条 deferred 的行号 + 四要素（机械核对，不是目测）

**表格头**：`gen2-graph-spec.md` **L735** = `| # | 目标原文位置 | 为什么本代测不了（证据） | 下一代的前置 | owner 角色 |` —— **四要素即四个列名**。
**机械核对**（把表格逐行切格数非空格）：L735 头 5 格 / L736 分隔 5 格 / **L737、L738、L739、L740 各 5 格且无空格** ⇒ 四条**每条都有**「目标原文位置 · 证据 · 下一代前置 · owner 角色」，**没有留空项**。

| # | 行号 | 目标原文位置 | 为什么测不了（证据） | 下一代前置 | owner 角色 |
| --- | --- | --- | --- | --- | --- |
| 1 | **L737** | §C·G7 target「20 个 gold 会话上 实体 P ≥0.85 / R ≥0.70」 | gold 三族是边/关系名/实体对，**无实体族、无会话语料**（`crates/graph/tests/gold/` 六个 JSON 无实体族字段）；活库快照不能替代手工标注 | **人工标注**的工程实体族 gold | 人工标注（**需具名成员/任务**；角色=数据标注） |
| 2 | **L738** | §C·G7 target「关系 R ≥0.60；幻觉率 ≤0.10」 | 活库 **0/67** 历史边有 `source_episode` ⇒ 无可回溯 transcript；幻觉率**机制半边已在 fixture 上被测**（含「分不清改写与发明」的已知假阳性） | **一次真实重蒸馏跑批** | 跑批（角色=重蒸馏 + 标注） |
| 3 | **L739** | §C·G4 target「`extracted` 比例 ≥30%（活库）」 | 需真实重蒸馏；且 30% 下限**挂在第 1、2 条的 gold 会话构成上**（规格原文自述） | 同第 1、2 条；**在这之前不判 30%** | 同第 2 条（并入 G7 的数据待办） |
| 4 | **L740** | §C·G8 target「三指标成对判决 ≥60%」 | 需 LLM 成对判决**跑批**（协议照 B2）；本代无 harness、无对照组 | 评测 harness + 对照组 | 评测（角色=跑批） |

**§C 就地标注也在位**（读者打开 §C 就能看见，不必翻附录）：L115（G4 `extracted ≥30%` → `deferred`）· L136–L141（G7 逐条：L137 实体族 `deferred`、L138 **关系 P 已达标并关闭**、L139 关系 R `deferred`、L141 幻觉率 `deferred`）· L147/L149（G8：结构侧**降为前置条件**、质量侧三指标 deferred）· L150（复现命令勘误指针）。

## 2 约束二：规格里的复现命令必须真的存在

**附录 H.2 = `gen2-graph-spec.md` L719–L729**：

| 目标 | 规格原命令（→ 为什么不能用） | 真实 target | 行号 |
| --- | --- | --- | --- |
| G3 | `--test provenance-fresh-db`（L109）→ 全树 **0 命中** | `--test temporal` + `--test extraction-gold` | H.2 **L725** |
| G4 | `--test as-of-gold`（L116）→ 全树 **0 命中** | `--test temporal`（`as_of_gold_returns_what_was_true_then`） | H.2 **L726** |
| G8 | `--test communities`（L144）→ 文件不存在 | `--test extraction-gold` | H.2 **L727** |
| — | 「**不新增三个空壳 target**」的理由 | — | H.2 **L729** |

**本轮实跑复核（不是复述 t38）**，跑的是 H.2 给出的真实 target：

```
… test -p ruagent-graph --test temporal -Nocapture         → exit 0，5 passed / 0 failed
   as_of ruagent at 2026-09-13T18:34:20.791485300+00:00: own 8/8 edges, 12 seeds, orphan edges [] -> MATCH
   as_of ruagent at 2026-09-13T18:38:15.350633200+00:00: own 17/17 edges, 12 seeds, orphan edges [] -> MATCH
   as_of ruagent at 2026-09-13T18:34:20.792034600+00:00: own 9/9 edges, 12 seeds, orphan edges [] -> MATCH
   as_of ruagent at 2026-09-13T18:38:15.351211800+00:00: own 17/17 edges, 12 seeds, orphan edges [] -> MATCH
   as_of ruagent at 2026-09-13T18:34:20.792301700+00:00: own 10/10 edges, 12 seeds, orphan edges [] -> MATCH
   as_of ruagent at 2026-09-13T18:38:15.350897100+00:00: own 17/17 edges, 12 seeds, orphan edges [] -> MATCH
   test the_old_writer_leaves_the_source_unknown_and_the_new_one_records_it ... ok      ← G3 的那条也在同一 target 里
… test -p ruagent-graph --test extraction-gold -Nocapture   → exit 0，7 passed / 0 failed
   level 0: communities=9 covered=43 non_isolated=43 split_by_modularity=1
   partition: 9 communities, sizes [14, 5, 3, 3, 2, 6, 2, 4, 4], split_by_modularity=1
   G8 falsifiable structure: largest community 14/43 = 0.3256 (target <= 0.50) | isolated 20/63 = 0.3175 (target <= 0.40)
```
⇒ **G4 = `MATCH` 6/6**（与 H.2 L726 记录读数一致）、**G8 = `communities=9 covered=43`**（与 L727 一致）、**G3 的那条测试在 `--test temporal` 里 `ok`**（与 L725 一致）。

**没有新增空壳 target（机械核对）**：`crates/graph/tests/` 现有 10 个 `.rs` —— `empty-recall-pattern`、`entity-query-shapes`、`extraction-gold`、`fixture-ownership`、`fixture`、`live-after`、`multihop-gold`、`resolution`、`seed-resolution`、`temporal`；**`provenance-fresh-db.rs` / `as-of-gold.rs` / `communities.rs` 均不存在**，且全部测试源码里对这三个名字 **0 命中**（grep 无 HIT）。名字与规格不一致的能力都落在已有 target 里，符合 H.2 L729「不改测试」的 requiredFix。

## 3 owner 命名：角色 ≠ 成员（反向检查）

机械检查「四条 deferred 行 + §C 五处标注行里是否出现成员名」：**L115 / L136 / L137 / L139 / L141 / L147 / L149 / L150 全部 NONE**（无 `recall` / `mem-core` / `wiki` / `captain` / `integ` / `INT/t19` 等）。
唯一命中的 `graph` 字样是**文件名/路径**，不是 owner：L138 出现在 `gen2-graph-repair.md` 里、L737 出现在 `crates/graph/tests/gold/` 里、L738 出现在 `gen2-graph-repair.md` 里。
并且 H.3 第 1 条（L737）**自己写明**「人工标注（**需具名成员/任务**；角色=数据标注）」—— 即「还没具名」是**写在字面上的**；`gen2-graph-impl.md` L388 同样写「我没有替 captain 具名：**角色 ≠ 成员**」。
⇒ **不存在把角色 owner 读成具名成员的空间**；t39（RV-C r3）L98 也独立记了同一件事：「`owner` 是**角色**而非具名成员，但 captain 已把该选项的前置（人类标注）交给用户，故不计失败面」。

## 4 反向检查：全文档 grep（`deferred` / `未达成` / `达标` / `通过`）

**`deferred` 全量（31 处）**：`gen2-graph-spec.md` L115/136/137/139/141/147/149/150/731/733/742 · `gen2-graph-impl.md` L377/388 · `gen2-graph-review-r2.md` L6/145/156/168（= 提出这条 requiredFix 的评审）· `gen2-graph-review-r3.md` L1/3/5/59/65/77/80/81/85/98/110/128/160（= 复核方）。
**无一处**把四条写成达标/通过。

**`达标` / `通过` 逐处分类**（凡与 G4/G7/G8 相关的命中，全部核过）：

| 命中 | 原文要点 | 判定 |
| --- | --- | --- |
| `spec.md:138` | 「关系 P ≥ **0.80** —— **已达标并关闭**（t27：0.7500 → 0.9149）」 | **不是四条之一**：四条里 deferred 的是**关系 R**（L139）；P 是**另一个子项**且**确实达标**（t39 L80 复核过 `43/47 = 0.9149`） |
| `impl.md:100 / 159` | 「G8 …（结构侧达标）」/「G8 三指标成对判决 ≥60% \| **not_measured** \| …；结构侧（覆盖 100%）已达标」 | **不是四条之一**：结构侧在 t38 被**降为前置条件**（§C·L147/L149、H.3 第 4 条），三指标那一条如实写着 not_measured/deferred |
| `impl.md:438`、`review-r3.md:94`、`verify.md:263/269` | G5 的 `≥0.60` **达标**（另一条 finding 的更正记录） | 与四条无关（G5 是另一条目标） |
| `review-r3.md:85` | 「**四条 deferred 登记（它们**不是**达标）**…」 | **第三方独立确认**（复核方同样判定四条不是达标） |
| `review.md:18` / `verify.md:48` | G7 关系 P 当时**未达标**（RV-C r1 / V-C 时期） | 历史读数，方向正确 |
| `impl.md:10`、`verify.md:10` | t9 期的**汇总行**：「**8 条达标**（G1 G2 G3 G4* G5 G6 G8* G9*）…带 * 的是「结构侧达标、需要在别处接线的读数仍为 not_measured」」 | **兼容，非违规**：它把 G4*/G8* 的被 deferred 的那半边叫 **not_measured**（从未叫达标）；但它是**粗粒度**汇总、成文早于 t38 的 deferred 裁决，**没有点名那四条** ⇒ 见 §5 观察 O-1 |

**t39（RV-C r3）的独立判决**（最强的一条外部证据）：文首 L3「**结论：`pass`**…这是**对「本单 finding 是否全部收口 + 登记是否如实」的判决，不是把四条 deferred 判成达标** —— 那四条按 captain 裁决**仍是未达成**」；L65 专列一行「**「仍是未达成，不是达标」的字面**：L136 / L733 … **在，且两处都逐字写明**（没有被写成达标、没有含糊化）」；L110 RV-C2-1 判「**关闭**（仍**未达成**，已登记）」；L160「**没有**被判成达标或被含糊化」。

## 5 观察（不阻塞，供下一次触碰这些文件时处置）

* **O-1 · t9 期汇总行仍是粗粒度**：`impl.md:10`（及 `verify.md:10`）把「G4*、G8*」列进「8 条达标」，脚注说明「结构侧达标、别处接线仍 not_measured」。**它没有把四条写成达标**（§4 已核），但读者若不看脚注，可能把「G4 达标」读成整条达标。**权威表述是 §C 逐行 + H.3 + impl §14.1**（L377/L388）。建议下一次触碰 `gen2-graph-impl.md` 时，按 §15 的修订记录形式，在该行补一句「G4/G8 被 deferred 的半边见 H.3」，或直接引用 §14.1。**本单 out of scope，只登记。**
* **O-2 · 「具名前置」的口径**：用户约束原文是「显式 deferred **并具名前置**」。现状是**前置被点名到具体数据/跑批**（人工标注的工程实体族 gold / 一次真实重蒸馏跑批 / LLM 判决 harness + 对照组）而 **owner 只到角色**；「谁来做」由 captain 的选项 (i) 留给用户（L737「需具名成员/任务」+ impl L388「角色 ≠ 成员」）。t39 L98 认可这个形状不计失败面。⇒ 若下游把「具名」理解为**必须点名成员**，需要 captain 做一次具名；**当前登记不满足「成员具名」，满足「前置具名 + 角色 owner」**，本审计按后者判过，并把差别写在这里，不替任何人具名。

## 6 账目（本单要补的那一笔）

| 账目项 | 原挂 | 实质交付 | 本单证据 |
| --- | --- | --- | --- |
| 「不可测目标必须显式 deferred + 前置/owner」 | t40（**已取消**，与 t38 内容重复） | **t38 落地**（规格 §C 逐行 + 附录 H.3 四要素）· **t39 复核 pass**（L98 O-1「登记形状合格」） | §1（L136/L733 逐字 + L735–L740 机械核对）· §3（角色非成员）· §4（反向 grep 无达标） |
| 「规格里的复现命令必须真的存在」 | t40（同上） | **t38 落地**（附录 H.2 三条勘误 + 真实 target）· **t39 复核 pass** | §2（H.2 L719–L729 行号 + **本轮实跑** G4 6/6、G8 `communities=9 covered=43`、G3 `ok` + 无空壳 target） |

**⇒ 这两条用户约束由 t38 + t39 + 本审计的行号证据支撑**；t40 的取消不再留下悬空的覆盖项。

## 7 复现命令

```powershell
# 契约指定（本单必跑）：字面证据恰好两行
powershell -NoProfile -Command "Select-String -Path docs/design/reviews/gen2-graph-spec.md -Pattern '未达成' -CaseSensitive | Select-Object LineNumber, Line"

# 四要素机械核对（切格数空）
#   L735 表头 5 格；L737–L740 各 5 格、无空格

# 反向 grep
powershell -NoProfile -Command "Select-String -Path docs/design/reviews/gen2-graph-*.md -Pattern 'deferred' | Select-Object Filename, LineNumber"
powershell -NoProfile -Command "Select-String -Path docs/design/reviews/gen2-graph-*.md -Pattern '达标|通过' | Select-Object Filename, LineNumber, Line"

# H.2 真实 target 实跑（本轮）
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-graph --test temporal -Nocapture
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-graph --test extraction-gold -Nocapture
```
