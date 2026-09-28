# RV-C 评审（t17）：I-C 的 C 节目标逐条判决 + 两处 medium 的处置

> **结论：`needs_revision`（任务置 failed）**。C 节 10 条目标里，**G2/G3/G4(结构侧)/G5/G6/G8(结构侧)/G9(三态)** 有我的独立可复现读数；**G7 未达成**（关系 P 0.75 < 0.80，且实体族**根本没有 gold、没有读数、也没有 not_measured 登记**）；此外 I-C 交付物里有两处**正确性缺陷**（`as_of` 字符串比较、证据行方向反转）与一处**不可复现**（`retrieve` 的并列边 tie），共 **6 条 medium 及以上 finding，全部给出 requiredFix**。
> **评审员**：review（独立评审员，**不是** I-C 的作者；`crates/` 与 `panel/` 一行未改，本单只写本文件）。
> **被评审对象**：I-C（t9）报告 `docs/design/reviews/gen2-graph-impl.md`（219 行 / 00:47:15）与其交付物 `crates/graph/**`、`crates/daemon/src/distill.rs`。
> **字节绑定（读数归属）**：`retrieve.rs` `7306185BB6929561`（23:28:06）· `community.rs` `6F3EA5E9B9A06B3C`（23:33:53）· `lib.rs` `DD2EBC23B34EC131`（23:33:58）· `Cargo.toml` `C624BED10106E31D` · tests/gold 六个 JSON 与 V-C（t13）§9 的收尾哈希**逐位相同** ⇒ 我的图侧读数与 V-C 的绑定在同一份字节上。**`distill.rs` 已被他人改动**：V-C 读数时 `A912C17FC05D17E6`@00:08:09 → 现在 **`9A81BF5C06F6FD23`**@01:09:08（同批新增 `crates/store/src/migrations/0024_distill_attempts.sql`）。我下面凡引 distill 读数都写明是**当前字节**，不把它们记在 I-C 账上。
> **时间窗**：2026-09-28T01:10 → 02:0x +08:00。
> **环境纪律**：真守护进程 **pid 79984 未启停**；`~/.ruagent/data/ruagent.db` 全程 `?mode=ro`，**sha256 前后逐位相同**（`51539f88c4f901b148cd0ab05c232de84baff2f8f6d9db54b3366494b3c50aa1`、size 14,946,304 B、mtime `2026-09-27T21:15:21`）；**未用** `GET /api/v1/recall`；所有写操作发生在 `%TEMP%` 的副本上；编译全部经 `scripts/cargo-team.ps1`（单一编译 / 共享 target / CPU 0-11），**未自设 `CARGO_TARGET_DIR`**。

---

## 0 一句话

I-C 把「多跳检索」从一个不存在的入口做成了一个**带路径证据、可逐因子复算、有截断理由**的冻结接口，这一点我用自己的问题、自己的副本、自己的因子复算确认了（§1.3）。但质量门有三处不能放过：

1. **`as_of` 用字符串比较** —— 同一个瞬间的三种 RFC3339 写法给出**不同的证据**（我在两个瞬间上量化：规范形 0 错，`Z` 形 16/19 条判反，`+08:00` 形 21/32 条判反；证据集合本身也不同）。captain 已裁决它**门禁 t19 的 `?as_of=` 接线** ⇒ **RVC-1（medium）**。
2. **「失败行绝不创建 episode」不成立**：episode 在 `write_memories` 内创建、`write_graph` 在其后，`write_graph` 失败会留下 episode + `failed` 行，面板的 `distilled` 徽章会把一次**失败的蒸馏**显示成发生过 ⇒ **RVC-3（medium）**，按 captain 的裁决走 Err 路径补偿。
3. **G7 的关系级 precision 0.75 < 0.80 未达标**（作者如实登记、未放宽），而且 **G7 的实体族（实体 P ≥0.85 / R ≥0.70）既没有 gold、也没有读数、也没有出现在 not_measured 清单里** ⇒ **RVC-2（high）+ RVC-6（medium）**。

另外我在复核中**发现了两条 V-C 没报的问题**：`retrieve` 对同一查询**不可复现**（并列边的 tie 由 `HashMap` 迭代顺序决定）⇒ RVC-4（medium）；D.2 契约要求的证据行**在反向遍历时把箭头方向写反**（`微信 -uses-> 用户19410`，而事实是反向的）⇒ RVC-5（medium）。

---

## 1 我的独立读数

### 1.1 门禁（我自己的构建）

```
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-graph
→ exit=0：lib 3 + empty-recall-pattern 1 + entity-query-shapes 3 + extraction-gold 6
        + multihop-gold 5 + resolution 3 + seed-resolution 5 + temporal 3 = 29 passed / 0 failed
        live-after 3 ignored（需 RUAGENT_GRAPH_LIVE_COPY；我下面用**我自己的副本**跑了这三条）
```

### 1.2 逐目标带读数的实跑（我看到的原始行）

```
seed-resolution:  non-empty: old(strict) 1 -> new 13
                  self-name resolution: old(strict) 63/63 -> new 63/63 legs={ExactName}   ← 无回归
multihop-gold:    gold reachable n=20 | old(1-hop current_facts) reached 0 | new hops=2 reached 15 | new hops=3 reached 16
                  truncated_by {"None":1,"Some(Hops)":6,"Some(MaxPaths)":13} | facts/query median 9 min 3 max 12
                  path-recall@12: hops=2 0.7500 hops=3 0.8000
                  tight budgets -> paths=2 facts=2 truncated_by=Some(Beam) frontiers=[2,0]
                  beyond-cap negatives checked: 5 | isolated seed: reason=NoEdgeFromSeeds | nonce: reason=NoSeed
                  as-of 2000: edges_reachable=60 reason=AsOfBeforeAnyFact
temporal:         as_of gold 6 条 → 6/6 MATCH（**六条全是「纳秒 + `+00:00`」规范形**，逐条读了 as_of.json）
                  temporal states: extracted=Current recorded=RecordedAtOnly old(null)=RecordedAtOnly
                  superseded edge 18 = Superseded{invalid_at: …18:38:15.350633200+00:00}
resolution:       gold n=24 | TP=7 FP=0 FN=0 TN=16 | abstain(queue)=5 | declared-alias-only=1 | P=1.0000 R=1.0000
                  BEFORE (norm_name equality): 0/8 true pairs merged | queued: #65 vs #37
extraction-gold:  edges n=60 supported=45 duplicate=11 mislabeled=4 unsupported=0
                  relation-level precision (strict) = 0.7500 | endpoint-level = 0.9333
                  ontology: 35 relations, controlled=29 ad_hoc=6, singleton-based names=24
                  level 0: communities=9 covered=43 non_isolated=43 split_by_modularity=1
                  partition sizes [14,5,3,3,2,6,2,4,4] | summaries written: 1 of 9
                  support(verbatim)=true support(paraphrase)=false support(invented)=false
```

**我自己的副本上的三条 live 读数**（每条测试**各用一份新副本**，见 §1.6；这点与 V-C 的做法一致，也是 F-0b 要求的）：

```
#1 (frame):      REAL FRAME n=23 | strict non-empty 1 -> new 13 | queries with >1 seed: 7
                 seeds by leg (all queries): {"exact_name": 1, "name_token": 8, "summary_fts": 46}
#2 (redundancy): REAL: the judge finds 7 redundant pairs … | merged 5 pairs -> redundant 0
                 | entities 58 | edges 67 (unchanged) | aliases 5
#3 (community):  REAL communities: 9 communities, covered 43/43 non-isolated, split_by_modularity=1
                 REAL retrieve(ruagent): seeds=12 paths=12 facts=11 truncated_by=Some(MaxPaths) frontiers=[8, 7]
```

⇒ **`summary_fts` 我读到 46（不是作者报告的 39）**，而且我是在**从未被 merge 用过的原始副本**上读的（V-C 曾用「跑过 merge 的副本」验证过 46，我另外证否了「是对象集被改过」这一假说：原始副本与合并后副本都给 46）⇒ 作者 §1 G2 的 `39` 与我 §1.2 的 **`0.7500/0.8000 / median 9 / cuts {None 1, Hops 6, MaxPaths 13}`** 一样**不可引用**（RVC-7）。

### 1.3 我自己的多跳问题 + 证据形状（这台机器上我亲手构造的）

探针：`%TEMP%\rv-c-probe\`（我用 `ruagent-graph`/`ruagent-store` 的**公开 API** 写的 bin，不在仓库里）；对象集 = 我自己的 `VACUUM INTO` 副本 `%TEMP%\rvc-copy-D.db`。四个问题由我自选：`"银河麒麟 V10"`（CJK、当前边上 2 度）、`"openpyxl"`（孤立实体，0 度）、`"DeepSeek Harness"`（8 度中间枢纽）、`"微信"`（CJK、1 度）。

**形状（原文摘录，路径/跳数/时序/来源在同一行）**：

```
微信 -uses-> 用户19410 [hops=1 valid_at=2026-09-13T17:06:33.251418700+00:00 state=recorded_at_only edges=5 source=-]
微信 -uses-> 用户19410 -uses-> AutoHotkey [hops=2 valid_at=…state=recorded_at_only+recorded_at_only edges=5,4 source=-]
Q "openpyxl": seeds=1 paths=0 empty=Some(NoEdgeFromSeeds)      ← 不是「图里没有这件事」，是「种子上没有边」
```

**我对每条返回边的独立复核（不是只读一个分数）**：

| 复核项 | 我的做法 | 结果 |
| --- | --- | --- |
| 路径几何 | `hops == edges.len() == nodes.len()-1`、无重复节点 | `ok`（我把「方向」单列一行，见下） |
| **边的方向** | 把证据里的 `src/dst` 与 `nodes[i]/nodes[i+1]` 比 | **不一致（反向遍历）——见 RVC-5**：`微信` 的 4 条路径全部反向，`DeepSeek Harness` 的 `-supports-> Multica` 也是（存储方向是 `Multica -supports-> DeepSeek Harness`） |
| 分数可复算（D.1 冻结公式） | 我自己从副本里数 `n_r`、`deg(src)`、`deg(dst)`，按 `0.5 × 1/(1+ln(1+n_r)) × 1/(1+ln(1+deg_src+deg_dst))` 重算我抽的 8 条边 | **逐位相同**：e2 `0.04760989`、e3 `0.11316922`（实现 0.113169216）、e4/e5 `0.05023873`、e41 `0.05498344`（0.054983433）、e53 `0.07827730`、e25 `0.10024340`、e26 `0.08941724`（0.08941725） |
| 分数 == factors 乘积 | `1.0×0.05023873 = 0.050239`（1 跳）、`×0.05023873 = 0.0025239`（2 跳，实现 0.0025239298） | 一致 |
| 时序 | 自己按同行的 `invalid_at`/`event_time_source` 判 `current/recorded_at_only` | 默认视图全部 `recorded_at_only`（与 66/67 的现场一致） |
| **无效边是否被排除** | 默认（`as_of=None`）返回的证据边集里是否出现 `invalid_at` 非空的 18/19/20/28/33/48/59 | **一个都没有**；`include_superseded=true` 时它们出现（下一节） |
| 预算 | 三组预算（12/24/8、3/2/2、1/1/1） | `paths ≤ max_paths`、`facts ≤ max_facts`、每次截断都报 `truncated_by`（`MaxPaths`/`Beam`/`MaxFacts`） |
| 确定性 | 同一查询连跑 6 次逐字节比较 | **不成立 —— 见 RVC-4** |

**我自己的悬浮读数**：`deterministic_default=false`（Q `"DeepSeek Harness"`），6 次连续调用得到 **6 份不同的 JSON**（长度 13689–13723），路径的**节点序列完全相同**（`42-55 | 42-55 | 42-55 | 27-21 | …`），但同一个槽位里的**边在 49 / 52 / 53 之间轮换**（第一次 `edge_id:53`，第二次 `49`，第三次 `52`…）。三条边是同一对端点（42→55）的三条**并列边**（`drives_model` / `runs_model` / `powers`），分数、跳数、节点序列**全部相等** ⇒ 冻结的 tie-break `(score 降序, hops 升序, 节点 id 序列字典序)` **无法区分它们**，实际由 `HashMap` 迭代顺序决定（`retrieve.rs:806 next.into_values()` → `:807` 稳定排序保留输入序 → `:813-816` 在 tie 组里 `truncate(beam)`；`:832-837` + `:847-848` 同理）。**同一条查询返回哪条事实会变**，注入的就是不同的句子。

### 1.4 我自己的 `as_of` 复核（规则层 + 证据层，两层都做）

**规则层**：我把 `retrieve.rs:602` 的判据（`valid_at <= t && invalid_at > t`，**纯字符串比较**）在副本的 67 条边上复刻，并与**瞬间算术**（`fromisoformat` → 同一具体时刻）逐边对比。两个瞬间、每个瞬间三种**同一瞬间**的写法：

```
instant A 2026-09-13T18:38:15Z（correct true-set = 30/67）
  canonical 2026-09-13T18:38:15.000000000+00:00 → code 30  错 0
  Z-form    2026-09-13T18:38:15Z                → code 38  错 16（漏 18,19,20,28；多 31..36…）
  +08:00    2026-09-14T02:38:15+08:00           → code 43  错 21
instant B 2026-09-13T18:34:20Z（correct = 11/67）
  canonical                                     → code 11  错 0
  Z-form                                        → code 30  错 19
  +08:00                                       → code 43  错 32
```

**证据层**（同一瞬间、同一个问题、同一份副本，只有字符串写法不同）：

```
Q "银河麒麟 V10" canonical → edge_ids [2,3,4,5,10,11,14,26]
                Z-form    → edge_ids [2,3,4,5,10,11,14,26,39]        ← 多一条
                +08:00    → 同 Z-form
Q "DeepSeek Harness" canonical → [12,13,16,21,24,25,26,28,29]
                     Z-form    → [12,13,16,21,24,25,26,29,36,38,41,42]
                     +08:00    → 同 Z-form
```

⇒ 不是「我觉得字符串比较不好」，而是**同一瞬间的不同写法给出不同的证据**（SET 级差异），并且 `Z` 形还会把**当时还不成立的边**（36/38/41/42）当成事实端上来、把**当时成立的边**（18/19/20/28）漏掉。作者的 as-of gold 六条**全部**是规范形（我逐条读了 `as_of.json`），所以这个 gold 对这条缺陷**完全不可见**（RVC-1）。

### 1.5 我自己的 gold 复核（不看作者结论）

**(a) 多跳 gold 的对象集与样本量**：我用自己的 Python BFS 在 `live_graph_snapshot.json` 上只走 current 边重建邻接，对 `multihop.json` 的 **25 条**逐条复算：

```
reachable 20 条：我的 BFS 距离与 gold 的 hops 逐条相同（20/20）
beyond_cap 5 条：我的 BFS 距离 4/5/6，全部 > 3
不一致条目 = 0
```

**(b) 抽取 gold 的对象集**：`edges.json` 标注了 **60/60 条 current 边**（无遗漏）；`ontology.json` 标注了 **35/35 个关系名**；标签分布 `supported 45 / mislabeled 4 / duplicate 11`；`resolution.json` 24 对（`same` 8）。**我自己重判了那 4 条 mislabeled 与 4 组重复**：`#3 shares_kernel_with`（文本说的是「共享 arm64 模拟**测试思路**」）、`#12 built_on`（文本说「ruagent **是** ACP client」）、`#45 must_never_drop`（是约束不是关系）、`#47 planned_queryable_store`（是计划不是现状）——**我同意这 4 条**；重复组 `52/53/55`（同一事实三种写法）、`54/56`、`39/62`、`40/63` 逐条读下来**确实是同一事实**。⇒ **gold 的标注是诚实的，0.7500 是真读数、不是标注失误**。

**(c) 我自己的单例计数**（口径核对）：全部 67 条边上 **24/35** 个关系名只出现一次；**只在 current 60 条边上**是 **26/35**（多出 `owns`、`runs_on`）。作者报告把「24/35 单例」与「60 条 current 边的 gold」并排写（RVC-8）。

**(d) gold 的交付面**：`docs/design/reviews/gold/` **不存在**，全仓**没有 `score.py`**；gold 实际落在 `crates/graph/tests/gold/`。规格 G7 的复现命令 `python docs/design/reviews/gold/score.py --gold docs/design/reviews/gold/*.json --pred <dump>` **无法执行**。规格 G7 的 target 还写着「**20 个 gold 会话**上 **实体 P ≥0.85 / R ≥0.70**」——现 gold 只有边/关系/消解三族，**没有实体族**，报告里也没有把实体族登记为 not_measured（RVC-6）。

### 1.6 仪器、隔离与归因

* **副本**：`rvc-copy-{A,B,C,D}.db` 全部由 `VACUUM INTO` 从 `?mode=ro` 连接生成（**逐位一致**，含 WAL）；三条 live-after 测试**各用一份**新副本（`live-after` 的三条测试共用一个环境变量，其中两条会**真写**：合并 5 次、写社区表 ⇒ 在同一个副本上二次运行读到的就不是原始图）。
* **源库未被碰过**：读数全部跑完后 `sha256 = 51539f88…`、`mtime = 2026-09-27T21:15:21`、`schema_migrations max = 18`、`entity_edges 67 / current 60`、`source_episode 非空 0/67`（历史边**没有**被回填）。
* **归因**：图侧被评文件 mtime 全部 ≤ 23:39:43，与 V-C §9 的哈希逐位相同 ⇒ 我的图读数归 I-C。`distill.rs` 现在是 `9A81BF5C`@01:09:08（I-C 报告写 00:47、V-C 读 00:08）；`0024_distill_attempts.sql` 与 `distill.rs` 的这次改动**不是 I-C 的产物**（0024 的注释自己写着「I-SCHEMA-2 / t25」，且它点名 distill.rs 的 `log_outcome` 是必须改的写入者）。全树 `git status --porcelain` = 64 项（并发波次），**只作背景，不据此写我自己的越界结论**。

---

## 2 C 节 10 条逐条判决

判据只对齐 `gen2-graph-spec.md` C 节，不引入规格外的新要求。`not_measured (excused)` = 规格/任务单已具名 owner（DEP-1 → INT/t19）的接线后才有读数，按本单契约**不据此判 I-C 失败**。

| 目标 | 规格 target | 我的独立读数 | 判决 |
| --- | --- | --- | --- |
| **G1** 实体腿不再恒空 | 真实帧 ≥13/23；生产端到端 | **13/23**（冻结帧 `1→13`；真帧 `1→13`）；自查询 63/63→63/63 无回归；生产 **not_measured**（我读 `api.rs:2524` 仍是 `search_entities`，`resolve_seeds/retrieve` 在 api.rs 里**零调用点**） | **腿级：达成**；生产端到端：**not_measured (excused)**，owner=INT/t19（DEP-1） |
| **G2** 逐腿归因 | 5 条腿（含 0）都出现在证据里；23 条覆盖 | 5 条腿恒列出、含 0；真帧 `{exact_name 1, name_token 8, summary_fts 46, alias_table 0, vector_nearest 0}` | **达成**（形状）；作者的具体计数不可引用 ⇒ RVC-7 |
| **G3** 每条新边带来源 | 本次抽取 100%；失败保持 NULL；历史不追溯 | 当前字节的 distill 测试：`edges: [("runs_on", Some(1), Some("extracted"), Some("3b8…")), ("uses", Some(1), Some("recorded"), Some("330…"))]` ⇒ **2/2 带 episode + 16 位 fact_hash**；`add_fact_with_source(…, source_episode: Option<i64>)` **无哨兵/回填分支**；活库仍 **0/67**（不追溯） | **达成** |
| **G4** 事件时间 ≠ 记录时间 | 新边 100% 带 `event_time_source`；`extracted ≥30%`；as-of gold 6/6 | 写入侧：`extracted=Current / recorded=RecordedAtOnly / 旧写入器(NULL)=RecordedAtOnly`；as-of gold **6/6 MATCH**；`extracted ≥30%`（活库）**not_measured** | **结构/写入侧：达成**；6/6 这个读数**被 RVC-1 削弱**（gold 全是规范形，对缺陷盲）；**`extracted ≥30%` 未达成**（不可测，规格未具名 owner，按本单契约算未达成） |
| **G5** 多跳检索 | path-recall@12 ≥0.60；facts 中位 ≥3；截断必报 | `0.7500`(hops=2) / `0.8000`(hops=3)；中位 **9**；cuts `{None 1, Hops 6, MaxPaths 13}`；`tight(3/2/2)` → `truncated_by=Beam`；≥4 跳负例 5/5 正确缺席；**我自己的 BFS 20/20 复现 gold hops**；**我自己复算 8 条边的因子逐位吻合** | **达成**（目标 ≥0.60）；但**同一查询的边身份不可复现** ⇒ RVC-4；作者那行数不可引用 ⇒ RVC-7 |
| **G6** 消解与别名 | 24 对 gold P/R；真库冗余 0；未决队列不静默丢弃 | `TP=7 FP=0 FN=0 TN=16 | abstain=5 | declared=1 | P=1.0000 R=1.0000`；`norm_name` 旧判据 **0/8**；真库 judge 找到 **7 对** → 合并 5 次 → **0**，`edges 67` 不变、`entities 58`、`aliases 5`；`queued: #65 vs #37` 有读数 | **达成**（含「判据变强使基线 4→7」的如实登记） |
| **G7** 抽取质量 gold | 实体 P≥0.85/R≥0.70；关系 P≥0.80/R≥0.60；非法 kind/relation=0；幻觉≤0.10；gold 落 `docs/design/reviews/gold/` | 关系 P（严格）**0.7500（45/60）**；端点级 0.9333；本体 29/35 controlled、ad_hoc 6；`unsupported=0`；**我独立复算：gold 覆盖 60/60 边、35/35 关系名；4 条 mislabeled 我逐条同意**；**实体 P/R：无 gold、无读数、也未登记为 not_measured**；关系 recall / 幻觉率 not_measured（无可追溯 gold 语料）；`gold/`+`score.py` 不存在 | **未达成**（关系 P 0.75<0.80）；**实体族未达成且未登记 ⇒ RVC-6**；关系 recall/幻觉率 not_measured（规格未具名 owner ⇒ 按契约算未达成） |
| **G8** 社区层 | 覆盖 ≥90% 非孤立实体；全局注入 ≤2 条摘要；三指标成对判决 ≥60% | 9 社区、`covered 43 = non_isolated 43`、`split_by_modularity=1`、sizes `[14,5,3,3,2,6,2,4,4]`、摘要 1/9、重建幂等；**判据按构造必真**（测试先断言 `covered == non_isolated` 再断言 ratio ≥0.90） | **结构侧达成但判据不可证伪** ⇒ RVC-9；「全局注入 ≤2 条」**not_measured (excused)**，owner=INT/t19（注入面）；三指标**未达成**（需要 LLM 判决跑批，规格未具名 owner） |
| **G9** distill 三态 | 同窗口差值 0；失败带 reason；empty/failed 可分；failed 不建 episode | 当前字节：`by status: [(empty,1),(failed,2),(ok,1)]`、`succeeded attempt after a later failure: ("ok", None, 3)` + `newest: failed`、`first-attempt failure: ("failed", Some("Query returned no rows"), Some("hash-b"))`、**`R-2: attempts=4 rows=4 recorded_outcomes=4 | rows - attempts = 0`**；「failed 绝不建 episode」：顺序是 `write_memories`(内建 episode) → `write_graph`，Err 路径直接写 failed ⇒ **不变量不成立** | 三态/理由/「失败不覆盖成功」= **t9 达成**；**差值 0 = 不是 t9 的交付**（t9 受 `session_key` PK 限制到不了，已如实登记 DEP-3），是 `0024`+写入侧改动后在当前字节上达到的；**「failed 绝不建 episode」不成立 ⇒ RVC-3** |
| **G10** 图证据进注入 | ≥1 个运行路径出现 `<graph>` 且可回溯 edge_id | **not_measured**：生产注入路径在 `api.rs`/`runs.rs`（I-C 的 inScope 外）；`TAG_GRAPH` 与 rank 3 已由 mem-core 落地（我读 `inject.rs:170/310-315`，rank 序列断言在跑）；消费形状已被我用自己的问题验证（`lines()` 一行含 hops/valid_at/state/edges/source） | **not_measured (excused)**，owner=INT/t19（生产者）+ mem-core（词表已完成）；终判在 t20/t21 |

---

## 3 owner 清单：「只能在 t19 之后测」与「未达成」的分界

**(A) 不判 I-C 失败（规格/任务单已具名 owner = INT/t19，接线后才能测）**

| # | 目标/读数 | owner | 为什么只能等 t19 |
| --- | --- | --- | --- |
| A-1 | G1 生产端到端：召回响应里的实体腿命中率、`recall_log.entities>0` 比例 | INT/t19（DEP-1） | 生产者是 `api.rs:2524`（现仍 `search_entities`）；`recall_log.graph_entities/graph_paths` 也无人写（列已由 t6 加好） |
| A-2 | G10 `<graph>` 进入注入（及 G8 的「全局注入 ≤2 条社区摘要」） | INT/t19（生产者）+ mem-core（词表**已完成**） | 注入块在 `api.rs`/`runs.rs` 拼装；且任务禁止启停 pid 79984、禁止用 `/api/v1/recall` 取证 |
| A-3 | DEP-5（mem-core 要的 `judge_merge` 接线） | mem-core + graph | 未落地且**没有半接线**（作者如实登记；mem-core 已同意在未落地时把其 C2 判 not_measured） |

**(B) 算「未达成」（不可测且规格未具名 owner）**

| # | 目标/读数 | 为什么仍算未达成 | 谁才能解 |
| --- | --- | --- | --- |
| B-1 | G7 **实体 P ≥0.85 / R ≥0.70** | 规格 G7 的第一族指标；现 gold 无实体族，报告里**连 not_measured 都没有** | I-C（补实体 gold + 读数）或 captain 裁决降级 |
| B-2 | G7 **关系 recall ≥0.60**、**幻觉率 ≤0.10** | 需要带 transcript 的 gold 语料（规格写「20 个 gold 会话」）；现 0/67 边有 episode ⇒ 无可比对来源 | 一次真实重蒸馏 + 人工标注（跑批 owner 未具名） |
| B-3 | G4 **`extracted` 比例 ≥30%**（活库） | 需要真实重蒸馏；取数面（`event_time_source`/`source_episode`/`prompt_hash`）已写好 | 跑批 owner |
| B-4 | G8 **三指标成对判决 ≥60%** | 需要 LLM 判决跑批；作者只写「下一轮 I-C 或评测 harness owner」，不是具名任务 | 评测 harness owner |

---

## 4 findings（medium 及以上逐条给出处置；全部未替作者改码）

### RVC-1（medium · 正确性 · **captain 已裁决门禁 t19**）`as_of` 是字符串比较，同一瞬间的不同写法给出不同证据
* 证据：`retrieve.rs:602`（`e.valid_at.as_str() <= t && invalid_at.map(|i| i > t)`）与 `:719-726`（同一条判据用于可走边过滤）。我的量化：instant A 规范形 **0 错** / `Z` 形 **16 条判反**（漏真边 18,19,20,28；多收 31..36）/ `+08:00` 形 **21 条判反**；instant B：0 / **19** / **32**。证据层：同一问题同一瞬间，`canonical [2,3,4,5,10,11,14,26]` vs `Z-form [+39]`；`DeepSeek Harness` 的 `[12,13,16,21,24,25,26,28,29]` vs `[…,29,36,38,41,42]`。作者的 as-of gold 六条**全是规范形** ⇒ 对该缺陷完全不可见。
* 影响：`GraphQuery::as_of` 是 D 节冻结签名，DEP-4 准备把 HTTP `?as_of=` 接到 `retrieve`（owner=INT/t19）；查询串里最自然的 `…Z` 写法会**静默**改变证据集合。
* **requiredFix**：入口处把 `as_of` 归一化成同一瞬间（`chrono::DateTime<Utc>` 比较，或统一 `to_rfc3339_opts(SecondsFormat::Nanos)` 规范化后再比），gold 补 **3 条「同一瞬间不同写法」的负例**（`Z` / 带偏移 / 无小数）断言三者给出**逐字节相同**的证据。**在修复前，t19 不得接 `?as_of=`。**

### RVC-2（high · 目标未达成）G7 关系级 precision 0.75 < 0.80
* 证据：`extraction-gold` = `relation-level precision (strict) 0.7500 (45/60)`，`duplicate 11 / mislabeled 4 / unsupported 0`；我独立确认 gold 覆盖 60/60 边、35/35 关系名，并**逐条重判**了 4 条 mislabeled（同意）与 4 组重复（同意），⇒ 0.7500 是真读数，不是标注问题。作者**没有放宽判据**，已如实登记。
* **requiredFix**：按报告 §10 的 R-1a/b/c 修（同事实重复边按 `fact_hash` + `(src,dst)` 归一化身份去重；关系名必须陈述事实文本所说的关系；关系词表收窄/归一），**gold 文件不改、target 不改**，修后在同一 60 边上关系级 precision ≥0.80。captain 已把 repair 派回同一批文件。

### RVC-3（medium · 正确性·契约）「失败行绝不创建 episode」只对 `write_memories` 之前的失败成立
* 我读到的顺序（当前字节 `9A81BF5C`）：`distill_once` → `write_memories`（**episode 在这里创建**，L509-527）→ `write_graph`（L273）；任何一步 `Err` 都由 `distill()` 的 `Err` 分支写 `status='failed'`（L216-243）⇒ `write_graph` 失败时 **episode 已存在**，而日志是 `failed`；面板的 `distilled` 徽章按 `episodes.kind='run_turn'` 判 ⇒ **一次失败的蒸馏会被显示成发生过**。测试只直调 `log_outcome` 断言「episode 数不变」，**没有**让 `distill()` 在 `write_memories` 之后失败（断言层弱于声称层）。
* **requiredFix**（按 captain 裁决）：在 Err 路径**补偿**（删除或标记本次 episode），**不**把 episode 创建挪到 `write_graph` 之后（保住 mem-core 的 D2(b)）；补一条走**真实 `write_graph` 失败路径**的测试，断言「失败后 `episodes` 里没有本次 episode，或它被显式标记为失败」，而不是只测日志写入器。

### RVC-4（medium · 可复现性）`retrieve` 对同一查询不可复现：并列边的 tie 由 `HashMap` 顺序决定
* 证据（我的探针，同一进程内 6 次连续调用）：`deterministic_default=false`，6 份 JSON 长度 13689–13723，**节点序列相同**而**同一槽位的边在 49/52/53 之间轮换**（`drives_model`/`runs_model`/`powers`，同一对端点、分数与节点序列全等）。机制：`retrieve.rs:806` `next.into_values()`（HashMap 迭代序，每个实例随机）→ `:807-812` 稳定排序保留输入序 → `:813-816` 在 tie 组内 `truncate(beam)`；`:832-837` + `:847-848` 同理。规格 D.1 的 tie-break 键 `(score, hops, 节点 id 序列)` **无法区分并列边**，因此「确定性 tie-break」在事实上不成立。
* 影响：同一条查询注入的**句子会变**；任何「改前 → 改后」读数都不能逐字节复现（V-C 的四条问题恰好没有并列边，所以它读到 `deterministic: true`）。
* **requiredFix**：在两处排序键末尾追加**边 id 序列**（`retrieve.rs:807`、`:832`），使 tie 真正全序；补一条测试：构造 ≥3 条同端点等权重的并列边，断言**连续两次调用序列化结果逐字节相同**。（不改任何 target。）

### RVC-5（medium · 正确性·D.2 契约）证据行在**反向遍历**时把箭头方向写反
* 证据：我的探针原文 `微信 -uses-> 用户19410`，而边 5 存的是 `src=用户19410(3), dst=微信(17), relation=uses`（JSON 里 `src/dst/src_name/dst_name` 是**对的**）；`微信` 的 4 条路径**全部**反向；`DeepSeek Harness -supports-> Multica` 同理（存的是 `Multica -supports-> DeepSeek Harness`）。`lines()`（`retrieve.rs:275`）按 `nodes[i] -relation-> nodes[i+1]` 渲染，没有按边的真实方向渲染。走查是无向的（`neighbors` 语义一致），所以反向遍历占相当比例。
* 影响：D.2 硬要求「路径/跳数/时序/来源四件事必须在同一行里可读」，而这一行是**要注入**给 agent 的文本；现在它会把 **A 用 B** 写成 **B 用 A**，读到的是一条**方向相反的断言**。
* **requiredFix**：渲染时按边的真实方向出箭头（反向遍历时写 `dst_name <-relation- src_name` 或把 `(src,dst)` 放在行内），并补一条断言「`lines()` 里每一段的箭头方向与 `edges[i].src/dst` 一致」的测试。

### RVC-6（medium · 交付面不完整）G7 的 gold 面缺实体族，且与规格点名的交付面不一致
* 证据：规格 G7 复现命令 `python docs/design/reviews/gold/score.py --gold docs/design/reviews/gold/*.json …` —— `docs/design/reviews/gold/` **不存在**、全仓**无 `score.py`**，gold 实际在 `crates/graph/tests/gold/`（6 个 JSON，**没有实体族 gold**）；规格 target 的「20 个 gold 会话」也没有对应的会话语料（现对象集是活库快照的 60 条 current 边 / 35 个关系名 / 24 对实体）。报告 §1 G7 的表里**没有**实体 P/R 这一行，§4 的 not_measured 清单里也**没有**它。
* **requiredFix**：二选一并登记——(a) 补实体族 gold（哪些实体是从 transcript 正确抽出的 + 依据）并给出 实体 P/R 读数；(b) 或把实体族按 not_measured 登记，写明原因（缺会话语料）与 owner；同时把 gold 落到规格点名的路径并落一个可执行的 `score.py`，**或**由 captain 裁决修订规格 G7 的复现命令与对象集（两者只能有一个真相源）。

### RVC-7（low · 报告准确性）作者的两处数字不可引用
* G5：作者写 `0.7000/0.7500`、`median 7`、`cuts {Hops 4, MaxPaths 15, None 1}`；**我与 V-C 各自独立跑出** `0.7500/0.8000`、`median 9`、`cuts {None 1, Hops 6, MaxPaths 13}`（我另在三次以上重跑里稳定）。G2：作者 `summary_fts 39`，我在**原始副本**上读到 **46**（V-C 在合并后的副本上也是 46 ⇒ 不是对象集差异）。
* **requiredFix**：报告里把这两行标注为**不可引用**并换成可复现读数（或由作者重跑一次）；**target 不动**（0.70 与 0.75 都 ≥0.60，判决不变）。方向上作者**低估**了自己的实现。

### RVC-8（low · 口径）「24/35 单例」与相邻 gold 不是同一个对象集
* 我自己的计数：**全部 67 条边**上 24/35 个关系名只出现一次；**current 60 条边**上是 26/35（多出 `owns`、`runs_on`）。测试用的是 `snapshot.edges`（67），紧邻的 `edges.json` 是 60 条 current 边。
* **requiredFix**：报告里把两个对象集分开写（测试本身没错）。

### RVC-9（low · 判据可证伪性）G8 的「覆盖 ≥90% 非孤立实体」按构造必真
* 证据：`community.rs` 把每个非孤立实体都放进某个社区，测试先 `assert_eq!(build.entities_covered, build.non_isolated)` 再断言 `ratio >= 0.90` ⇒ 这条 target 不可能失败（我实测 43/43）。
* **requiredFix**：把结构侧降为**前置条件**（断言 `covered == non_isolated` 与分区数/`split_by_modularity`/幂等/摘要惰性），把 G8 的**可失败 target** 落回质量侧三指标（或另立一条能被证伪的结构指标，如「最大社区 ≤ N」）。

### RVC-10（low · 断言层）`TemporalStatus::TrueAsOf`（四态里的第三态）**没有任何断言**
* 证据：全 tests 目录 grep `TrueAsOf` = 0 命中，只在 `retrieve.rs:155/298/609` 出现；as-of gold 的 6 条只比对**边 id 集合**，不检查状态。规格 G4 的 metric 写的是「`TemporalStatus` 四态」。
* **requiredFix**：在 as-of gold 里补一条断言：当时为真、现在已失效的边（18/19/20/28/33）必须以 `TrueAsOf{valid_at, invalid_at}` 出现在证据里，且 `lines()` 渲染成 `true_as_of`。

### 观察（不进 findings，登记给下游）
* **O-1** `resolution` 的计数口径：`abstain=5` 与 `TN=16` **重叠**（24 = TP7 + TN16 + declared1），报告把两者并列写容易读成 28。建议加一句口径。
* **O-2** `RetrievalStats.graph_edges` 的文档说「the whole graph at this time view」，实现给的是**当前边总数**（与 `as_of` 无关；我的 as_of 各次都是 60）。今天只是文档与语义不严，但读数使用者不能拿它当「那个时点有多少边」。
* **O-3** `SeedBucket::offer` 的同腿比较方向是**升序**（`retrieve.rs:341`），与「同分保留更高分」的注释相反；今天每条腿传的都是常量分，**无影响**；E10 的向量腿一旦传真分，同腿内会保留**较低**分。建议改成 `>` 或补一条同腿不同分的测试。

---

## 5 未测 / 不判定（不许静默跳过）

| # | 项 | 状态 | 原因 / 谁才能测 |
| --- | --- | --- | --- |
| U-1 | G1 生产端到端、`recall_log.entities>0` 比例 | **not_measured (excused)** | DEP-1 未接线（`api.rs:2524` 仍 `search_entities`）；不许启停 pid 79984、不许用 `/api/v1/recall` ⇒ INT/t19，终判 t20/t21 |
| U-2 | G10 `<graph>` 进注入；G8 的注入条数 | **not_measured (excused)** | 同上（生产者 INT/t19；mem-core 的 `TAG_GRAPH` 已完成） |
| U-3 | G4 `extracted ≥30%`（活库） | **未达成（不可测，无具名 owner）** | 需要一次真实重蒸馏（本单无 agent 调用）；取数面已写好 |
| U-4 | G7 关系 recall / 幻觉率 | **未达成（不可测，无具名 owner）** | 0/67 历史边有 `source_episode` ⇒ 没有 transcript 可比对；机制在 fixture 上被测，且它**分不清改写与发明**这一点被测试自己写下 |
| U-5 | G8 三指标成对判决 | **未达成（需 LLM 跑批）** | 协议在 t3 的 B2；结构侧我已复现 |
| U-6 | 「不破坏产品」端到端重放（面板图页 4 个旧端点） | **未测** | 端点在 `api.rs`（不在 I-C inScope）；V-C 用「冻结函数体零删行」替代，我的复核是 `git diff` 只有 1 行可见性变更 + 冻结签名被 api.rs 的既有测试钉住 ⇒ **不是端到端重放** |
| U-7 | 真库上 `as_of` 的端到端（HTTP `?as_of=`） | **未测（且被 RVC-1 阻断）** | 需要 t19 接线；**修复前不得接线** |
| U-8 | 我**没有**重新标注 60 条边的全部标签 | **部分测** | 我独立完成了：gold 覆盖 60/60 边与 35/35 关系名、4 条 mislabeled + 4 组重复的重判、以及全部 25 条多跳 gold 的 BFS 复算；**其余 supported 45 条**我只做了「抽 2 条读事实文本」的抽样，未逐条重判 |

---

## 6 复现命令（我实际跑的）

```powershell
$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"
$w = "scripts/cargo-team.ps1"          # 全队纪律：每一次 cargo 都走它，不自设 CARGO_TARGET_DIR

# 门禁 + 逐目标读数
powershell -NoProfile -ExecutionPolicy Bypass -File $w test -p ruagent-graph
powershell -NoProfile -ExecutionPolicy Bypass -File $w test -p ruagent-graph --test seed-resolution -Nocapture
powershell -NoProfile -ExecutionPolicy Bypass -File $w test -p ruagent-graph --test multihop-gold   -Nocapture
powershell -NoProfile -ExecutionPolicy Bypass -File $w test -p ruagent-graph --test temporal        -Nocapture
powershell -NoProfile -ExecutionPolicy Bypass -File $w test -p ruagent-graph --test resolution      -Nocapture
powershell -NoProfile -ExecutionPolicy Bypass -File $w test -p ruagent-graph --test extraction-gold -Nocapture
powershell -NoProfile -ExecutionPolicy Bypass -File $w test -p ruagent-daemon --lib distill         -Nocapture

# 我自己的副本（只读源库 → VACUUM INTO；绝不指向 ~/.ruagent）
python %TEMP%\rvc_probe.py          # 副本 A/B/C + as_of 规则层量化 + 我自己的 BFS + gold 覆盖
python %TEMP%\rvc_labels.py         # 4 条 mislabeled / 重复组 / ad_hoc 的逐条重判
python %TEMP%\rvc_factors.py        # D.1 因子复算（我用到的 8 条边）
python %TEMP%\rv_c_pick.py          # 选题用的实体/度分布/失效边

# 三条 live-after（每条测试一份新副本；--nocapture 写进 `--` 之后）
$env:RUAGENT_GRAPH_LIVE_COPY="$env:TEMP\rvc-copy-A.db"
powershell -NoProfile -ExecutionPolicy Bypass -File $w test -p ruagent-graph --test live-after the_real_query_frame -- --ignored --nocapture --test-threads=1
$env:RUAGENT_GRAPH_LIVE_COPY="$env:TEMP\rvc-copy-B.db"
powershell -NoProfile -ExecutionPolicy Bypass -File $w test -p ruagent-graph --test live-after redundant_pairs -- --ignored --nocapture --test-threads=1
$env:RUAGENT_GRAPH_LIVE_COPY="$env:TEMP\rvc-copy-C.db"
powershell -NoProfile -ExecutionPolicy Bypass -File $w test -p ruagent-graph --test live-after community_coverage -- --ignored --nocapture --test-threads=1

# 我自己的探针（不在仓库里；用公开 API，跑在我自己的副本上）
$env:RV_C_LIVE="$env:TEMP\rvc-copy-D.db"; $env:RV_C_QUESTIONS="银河麒麟 V10|openpyxl|DeepSeek Harness|微信"
$env:RV_C_ASOF_CANON="2026-09-13T18:38:15.000000000+00:00"; $env:RV_C_ASOF_Z="2026-09-13T18:38:15Z"; $env:RV_C_ASOF_OFF="2026-09-14T02:38:15+08:00"
powershell -NoProfile -ExecutionPolicy Bypass -File $w run --manifest-path "$env:TEMP\rv-c-probe\Cargo.toml"     # RVC-1 证据层 + 形状
$env:RV_C_DETERMINISM="1"; $env:RV_C_QUESTIONS="DeepSeek Harness"
powershell -NoProfile -ExecutionPolicy Bypass -File $w run --manifest-path "$env:TEMP\rv-c-probe\Cargo.toml"     # RVC-4 不可复现
```

---

## 7 纪律回执

* **只写本单 inScope**：本文件 `docs/design/reviews/gen2-graph-review.md`。`crates/` 与 `panel/` **一行未改**（探针在 `%TEMP%`，不在仓库里）。
* **读数三件套**：对象集（我自己的 4 份 `VACUUM INTO` 副本 · 活库只读快照 · gold 的 6 个 JSON）· 采样面（我自选 4 个问题 · 2 个瞬间 × 3 种写法 · 67/60 条边 · 25 条多跳 gold · 8 条边的因子）· 可证伪判据（并列边连跑 6 次比 JSON；同一瞬间三种写法比集合；坏侧：`openpyxl` 必须 `NoEdgeFromSeeds`、≥4 跳负例必须缺席、非词查询必须 `NoSeed`）。期望值只写在规格/本报告一处，探针从库与函数取。
* **建议的 verdict**：`needs_revision`（⇒ 任务 failed）：6 条 medium 及以上 finding，全部给了 requiredFix；其中 **RVC-1 按 captain 裁决门禁 t19 的 `?as_of=` 接线**，**RVC-2 是未达成的目标**，**RVC-3/RVC-5 是交付物里的正确性缺陷**，**RVC-4 破坏「读数可复现」**，**RVC-6 是交付面不完整**。
* **未测一律写明**（§5，8 项），其中 3 项 owner=INT/t19 的**不判 I-C 失败**，4 项（不可测且规格未具名 owner）按本单契约**算未达成**。
* **真守护进程 pid 79984 未启停**；`~/.ruagent/data/ruagent.db` 全程 `mode=ro` 且 **sha256 前后逐位相同**；未使用 `GET /api/v1/recall`。
* **字节绑定**：图侧读数绑定 `retrieve.rs 7306185B` / `community.rs 6F3EA5E9` / `lib.rs DD2EBC23`（与 V-C 收尾哈希逐位相同）；distill 侧读数绑定**当前** `distill.rs 9A81BF5C`，并已声明它**不是 I-C 的字节**（I-C 报告 00:47 / V-C 读 00:08；现为 01:09，同批新增 `0024_distill_attempts.sql`）。
* **并发声明**：全树 `git status --porcelain` = 64 项，是并发波次；我**不**据此写任何 finding，归因只用「被评文件 mtime + 哈希 + 作者 changedPaths」。
