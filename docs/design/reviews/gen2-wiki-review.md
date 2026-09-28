# RV-D 评审（t18）：I-D 的 C 节目标与冻结面逐条判决

> **结论：`needs_revision`（任务置 failed）**。规格 §C 自己定了总门：**G1 ∧ G2 ∧ G3 ∧ G7 是硬门，任一不达标即 `needs_revision`**。G2/G3/G7 我判**达成**（负例真被拦、单源真收敛、组合请求真 400，且都有我自己的绿读数）；**G1 判未达成 —— 但不是准入半边，是读数半边**：`cite_coverage` 在构造上只能是 `0.0`/`1.0`（分子与分母由同一个 `meta.citations` 集合算出，`wiki.rs:2931-2960`），四个读面全用它，而**构建期真算出来的覆盖率写进 `wiki_pages.cite_coverage` 之后没有任何读路径读回**（我 grep 证实：全仓只有 `SELECT slug, stale_since FROM wiki_pages`）。**写侧闸门是真的**（我自己的契约运行里，6 个负例族 + 「无内容节」归因用例全部通过；V-D 自造 7 族 7/7 被拒；旧代码 8/8 落地）。所以这条 finding 只推翻**读数**，不推翻准入。另 5 条 low finding（一条是 captain 已裁的**一致性裁决**、一条是空表登记、一条是 `frozen_by` 与标定措辞不符、一条过期计数、一条 `stale_sources` 空数组）与 4 条观察。
> **评审员**：review（独立评审员，**不是** I-D 的作者；`crates/` 与 `panel/` 一行未改，本单只写本文件）。
> **被评审对象**：I-D（t10）报告 `docs/design/reviews/gen2-wiki-impl.md`（411 行 / 01:42:03）与 `crates/daemon/src/wiki.rs`、`crates/mock-agent/tests/wiki_pipeline.rs`。
> **字节绑定**：`crates/daemon/src/wiki.rs` **45AA136091C5D17D** @ 2026-09-28T01:10:13 · `crates/mock-agent/tests/wiki_pipeline.rs` **F881F6B272945291** @ 01:10:10（两者自 t10 收敛后未再变；V-D 的 02:14:26 报告与我的读数都在这个字节上）。
> **时间窗**：2026-09-28T02:0x → 02:18 +08:00。
> **纪律**：每一次 cargo 都走 `scripts/cargo-team.ps1`（单一编译 / 共享 target，**未**自设 `CARGO_TARGET_DIR`；任务单里那一行是对全队纪律的有意替换，与 V-A/V-B/V-C 同形）。真守护进程 **pid 79984 未启停**；`~/.ruagent` 全程 `~?mode=ro`；**未在真库上跑任何 wiki build**；**未用** `GET /api/v1/recall`。

---

## 0 一句话

I-D 把 wiki 从「引用不可验证、`## 来源` 可以没有、新鲜度只有两态、图读数两个口径、干跑有三种词汇」推到了「锚必须解析到活 chunk 否则页不落地、五值失效原因 + `stale_since` 只写一次、图读数单源 + 持久化、干跑一个词汇 + 组合请求 400」。**负例真的被拦住**（这是本代最容易假通过的地方，我按契约命令亲眼看到 `pages_written=0 / pages_failed=1 / status=failed` 的断言在跑）。**但 G1 的读数是一枚构造性满分**：`cite_coverage_of` 的注释写着「the reading was computed once, at build time, by `verify_page`」，而函数体只读 frontmatter 的 `citations`，**注释正好在说反话**；于是同一响应能同时报 `cite_coverage=1.0` 与 `uncited_sections=["事实"]`，一张在 `wiki_pages` 里根本没有行的页也报 `1.0`。

---

## 1 我自己的读数

### 1.1 契约命令：两次尝试，第一次是同伴在途，第二次绿（按 F5 纪律绑定）

```
① 02:0x   scripts/cargo-team.ps1 test -p ruagent-mock-agent --test wiki_pipeline
   → error[E0308] crates\daemon\src\memembed.rs:529:53
     expected `&MergeAudit`, found `&MergeVerdict`
     note: function defined here → crates\memory\src\dedupe.rs:342
     → could not compile `ruagent-daemon` (lib)  ⇒ exit 101
② 02:1x   同一条命令（同一字节，未改任何文件）
   → test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 16.50s
     [cargo-team] exit=0 elapsed=48.3s
```

**归因**：① 的红来自 **t31（mem-core 的 repair 单）在途**（`merge_audit_reason` 的入参从 `&MergeVerdict` 改成 `&MergeAudit`，`memembed.rs` 同一窗口还没跟上）——**不是 I-D 的文件、不是 I-D 的回归**。我按纪律只报文件与错误、没有替同伴改；几分钟后它自己收敛，② 就是我的绿读数。V-D 的 O-1 记录的是同一类事（它撞到的是 `distill.rs`/t27）。

### 1.2 负例真的被拦住（我自己的运行里逐条通过）

```
test the_citation_gate_fails_a_page_whose_claims_cannot_be_traced ... ok
test a_page_with_nothing_to_cite_fails_by_its_own_name ... ok
test an_unanswerable_freshness_reading_is_unknown_never_fresh ... ok
test dry_run_has_one_vocabulary_and_rejects_the_combined_request ... ok
test the_two_graph_endpoints_cannot_disagree ... ok
test a_stale_page_is_repaired_even_when_the_plan_says_keep ... ok
test a_pin_skips_the_page_and_the_reason_is_visible ... ok
test a_rename_moves_the_topic_instead_of_destroying_it ... ok
test a_page_hash_with_no_page_is_swept_by_the_next_build ... ok
```

我另外**读了断言本身**（不是只看计数）：`wiki_pipeline.rs:629-632` 对每个负例族逐条断言 `pages_written == 0`、`pages_failed == 1`、`row["status"] == "failed"`；`:665-673` 的「无内容节」用例还断言 `!err.contains("uncited section")`（**归因**不许串族）。⇒ 这些不是真空断言。

### 1.3 我自己的代码面复核（F1，三处独立证据）

```
crates/daemon/src/wiki.rs:2931  pub fn cite_coverage_of(meta: &frontmatter::PageMeta) -> f32
  :2932-2939  分子 = meta.citations 的 section → filter(is_content_section) → sort → dedup
  :2943       分母 = content_section_count(meta).max(分子)
  :2950-2960  content_section_count() 用**完全相同的 filter/sort/dedup** 算**同一个列表**
  :2928-2930  注释：“the reading was computed once, at build time, by `verify_page`”
              ← 函数体**没有读任何记录值**（注释说反话）
读面四处：:2789（pages）· :2861（lead_from）· :2890（lead_for）· :3128（hints JSON）
构建期真值：:3334 `let cite_coverage = report.coverage as f64;` → 写 wiki_pages.cite_coverage（:3344/:3351/:3364）
读回处：**零**（全仓对 wiki_pages 的 SELECT 只有 :3609 `SELECT slug, stale_since FROM wiki_pages`）
```

**对照**（说明修法很小）：同一个读路径对 `stale_since` **是**读记录值的（`:2791` `if f.is_stale() { recorded_since } else { None }`）——只有 `cite_coverage` 走了重算的恒等式。

### 1.4 硬门 G2/G3/G7 的单源点（我自己的 code 读数）

```
G2 六族 + 归因：CiteProblemKind 六值，as_str() 就是写进行 error 的那串词（:95-139）；
  失败落点 :1968 set_page_status(..., "failed", Some(&format!("{e:#}"))) ⇒ 页文件不写、库存不留
G3 stale_since 只写一次：:1014-1015
  `stale_since = CASE WHEN excluded.stale = 1 THEN COALESCE(wiki_pages.stale_since, excluded.stale_since) ELSE NULL END`
  `index.md` 标记 :2461 `let mark = if *stale { " ⚠️ 源已更新" } else { "" };`
G7 组合请求：wiki.rs:1727 `if req.dry_run && req.confirm_plan.is_some() {` → 错误串点名两个字段（:1729-1732）
  → api.rs:296-298 `ApiError::bad_request` ⇒ StatusCode::BAD_REQUEST（400）
```

---

## 2 C 节逐条判决（按规格 §C 的总门汇总）

| G | 门 | 我的独立读数 | 判决 |
| --- | --- | --- | --- |
| **G1** | **硬门** | **准入半边达成**：负例族逐条 `pages_written=0 / pages_failed=1 / status=failed`（我的运行 §1.2）+ 旧代码 6/6–8/8 落地（作者 §3.4、V-D）· **读数半边未达成**：`cite_coverage` 只能是 `0.0/1.0`（§1.3），四个读面全用它，构建期真值写库后无人读回；同一响应自相矛盾；无 `wiki_pages` 行的页也报 `1.0` | **未达成（读数半边）⇒ RV-D-1** |
| **G2** | **硬门** | 六族 + 「无内容节」归因各自成用例，`error` 逐条点名 kind；我的运行 `the_citation_gate_...` / `a_page_with_nothing_to_cite_...` **ok**；词表互不相同有单测 | **达成** |
| **G3** | **硬门** | `stale_since` 由下一次构建写入且 `COALESCE` 只写一次（§1.4）；`index.md` 出 `⚠️ 源已更新`；三面同源；第三态有独立用例且我这次运行 **ok** | **达成**（+ 一致性裁决 RV-D-2） |
| **G4** | 读数门 | 计划说 `keep` 也被改写并重建，行里记实际动作；`a_stale_page_is_repaired_...` **ok**（我这次运行）；before 是 `not_measured`（真库 0 个 stale 页）——按规格不写 0 | **达成** |
| **G5** | 读数门 | `the_two_graph_endpoints_cannot_disagree` **ok**（我有自己这次运行的绿）；`links_out` 只来自 `LinkGraph.degrees`；`wanted[].demanders`；每 `done` 一行读数；自链进 `self_links` 不计出度 | **达成** |
| **G6** | 读数门 | 被 skip 的页必有 `wiki_corrections`（自动 `note` / pin 的 reason，空 reason 被拒）；`/pages` 给 `frozen_by` —— **但手改冻结那条路上 `frozen_by=null`**（RV-D-5） | **达成**（+ RV-D-5） |
| **G7** | **硬门** | 一个 plan-only 词汇（`planned`）+ 组合请求 400 且点名两字段（§1.4）；我的运行 `dry_run_has_one_vocabulary_and_rejects_the_combined_request` **ok**；`pending` 只给执行行 | **达成** |
| **G8** | 联合门 | **not_measured**：I-D 侧数据齐（`WikiLead`/`lead_for`/`lead_from`/`recall_stubs` 带 `anchors`/`hint`/`cite_coverage`/`stale_since`），消费端（I-B 的 `<wiki>` 渲染 + I-INT 的构造点）不在 I-D 的 inScope ⇒ 终判留给 **t20/t21**。**并注明：本条的达标证据被 RV-D-1 污染**（注入的 `cite_coverage` 会是常量），修好后必须重读 | **not_measured（owner 分工）** |
| **G9** | 读数门 | `a_rename_moves_the_topic_instead_of_destroying_it` **ok**（我这次运行）；旧 slug 进 `aliases`、无 `deleted` 行；旧代码侧 V-D 补测取得（旧页留下 + 新页另建） | **达成** |
| E10 | 附加读数 | `a_page_hash_with_no_page_is_swept_by_the_next_build` **ok**；每次构建对账自愈 | **达成** |

**总门结论**：G2 ∧ G3 ∧ G7 达成，**G1 的读数半边不达标** ⇒ 按规格自己的汇总规则 **`needs_revision`**。

---

## 3 逐条复核任务单要求的三问

1. **判据是否可证伪（旧代码上会失败）**：作者 §3.4 给出「旧二进制上 6/6 全部以 `written` 落地」；V-D 用 `git archive HEAD` + 独立 target 取得 8/8 落地、G7a 两种词汇、G7b 组合请求 200、G9 旧页留下。我自己没有重跑两棵树（见 §5 U-2/U-3），但**我自己在代码面确认了这些判据的锚点**（`CiteProblemKind` 六值、`set_page_status("failed")`、`COALESCE(...)` 只写一次、`confirm_plan` 的 400 分支），且**新树上这些断言是我亲眼看到跑的**。
2. **是否只测了 happy path**：不是。契约文件里每个新判据都有「坏的一侧」（`:587` 引用闸门负例、`:661` 无内容节、`:1048` 第三态、`:1205` 哈希泄漏、`:1305` 改名），我这次的运行里这些用例逐条通过（§1.2）；作者另有「空 reason 必须被拒」的 pin 负例。
3. **双真相源是否真的消除**：`stale` 有单一计算点（`freshness_with`）、图度数单源（`LinkGraph.degrees`，`the_two_graph_endpoints_cannot_disagree` 钉住两面逐位相等）、`stale_since` 单源且读面读记录值 ⇒ **形状是做对的**。但 `cite_coverage` 被**统一到了一个错误的单一实现上**（恒等式 + 说反话的注释）⇒ 单源做对了形式、选错了对象。这是 RV-D-1 的实质。

---

## 4 findings

### RV-D-1（high · G1 硬门的读数半边未达成 · captain 已复核并裁定）`cite_coverage` 是构造性满分
* **我自己的证据**（三处，§1.3）：① `cite_coverage_of` 的分子与 `content_section_count` 的分母由**同一个** `meta.citations` 集合、**同一套** filter/sort/dedup 算出 ⇒ 分子恒等于分母（或 0）⇒ 返回只能是 `1.0`/`0.0`；② 函数上方注释自称「the reading was computed once, at build time, by `verify_page`」，**函数体没有读任何记录值**（注释说反话，本身就该记一条）；③ 构建期真值写进 `wiki_pages.cite_coverage` 后**没有任何读路径读回**（全仓对该表的 SELECT 只有 `:3609` 的 `slug, stale_since`）。V-D 另有两条独立读数：同一响应里 `cite_coverage=1.0` 与 `uncited_sections=["事实"]` 并存；**在 `wiki_pages` 里没有行**的手写页仍报 `1.0`。
* **影响**：G1 的标定读数「written 页 4 页 `cite_coverage=1.0`」是构造性成立的伪读数；面板/lead 无法区分「每节都有活锚」与「锚根本不解析」；**G8 要注入的正是 `WikiLead.cite_coverage`** ⇒ 会把常量带进注入面。
* **requiredFix**（按 captain 裁定，四件一起做）：**(a)** 四个读面（`/wiki/pages`、`lead_from`、`lead_for`、hints JSON）改取**记录值**（构建期 `report.coverage` → `wiki_pages.cite_coverage`），不许再从 frontmatter 重算恒等式；**(b)** `cite_coverage_of` 改成它诚实的功能名（如 `has_anchors`/`anchored_sections`）；**(c)** **无 `wiki_pages` 行的情况必须是第三态（`None`/`unknown`），不许报 `1.0`** —— 这是 O-1（`edited: bool` 把「不知道」压成 `false`）的同类缺陷，只是这次把「不知道」报成了满分；**(d)** 规格 G1 的标定读数与 G8 的 `WikiLead.cite_coverage` 都要标注**受本缺陷污染**，不得作为达标证据引用。
* **必须同时写明**：**写侧闸门是真的** —— 我自己这次的契约运行里 6 个负例族 + 归因用例全部通过，V-D 自造 7 族 7/7 被拒、旧代码 8/8 落地、双缺陷页一次报两族。这条 finding 是**读数**缺陷，不是准入缺陷。

### RV-D-2（low · **一致性裁决**，captain 已裁：保留实现、改规格措辞）
* **事实**：规格 G3 目标写「`stale=unknown` 第三态在**「KB 无该来源 row」**时必须出现」，D.5 的结构体注释也写 `None = 不可判定（… 来源 row 不存在）`；而实现的第三态只在「KB 整体没有 documents」出现，页里点名的来源查不到 row 走 **`stale{SourceMissing}`**（`wiki.rs:853+` 的 `freshness_with`）。V-D 的两个夹具：`VD T8-A`（空 KB）⇒ `freshness="unknown"` ✓；`VD T8-B`（非空 KB + 点名的 `never-src` 无 row）⇒ `freshness="stale"`。
* **裁决**：**保留实现语义**（一个**消失了**的引用来源是**信息**，不是「无法判定」；第三态留给「KB 整体为空」），**改规格那句**。方向是 fail-safe（不是把不可判定算成 `fresh`）⇒ 这不是静默收窄。**注意**：作者自己的报告 §2.2 第 59 行也写着「来源文档在 KB 里没有 row ⇒ `stale = None` + `unknown_cause`」，与实现不符 —— 一并订正。
* **requiredFix**：把规格 §2.2/G3 的措辞改成「KB 整体没有可用 documents row 时 `unknown`；页点名的来源消失 ⇒ `stale{SourceMissing}`」，并把 `VD T8-A`/`VD T8-B` 作为证据登记；impl 报告 §2.2 同步订正。

### RV-D-3（low · 为什么 stale 读不出来）`stale_sources` 在 `stale=true` 时是空数组
* V-D 的同族读数：`VD T8-B … freshness="stale" stale=true stale_sources=[]`，而夹具 `source_hashes` 是空表、没有来源名 ⇒ 读者无法从 `/pages` 知道**为什么** stale。
* **requiredFix**：`stale_sources` 必须由失效判据填上来源名（`SourceMissing`/`SourceHashDrift`/`ChunkMissing` 各有名字可用），或在 D.5 里明确「该字段只在有 `source_hashes` 记录时可填」并把两种形态的读数写进标定。

### RV-D-4（low · 空表登记，captain 已裁）
* `wiki_citations`（规格 E.2 DDL-2，目的是「按下游 `chunk_id` 反查受影响页」）**全树没有任何读写者**，三个 root 都是 0 行；实际把锚放在 `wiki_pages.citations_json` + 页 frontmatter，失效传播靠重新扫描页文件。
* **裁决**：登记为**本代未使用** + 反查索引 **deferred** 到下一代；**不要**为了「别浪费表」在 `record_page_row` 里顺手写它 —— 那会造出一条**没有消费者**的写路径。
* **requiredFix**：在规格/DDL 登记该表本代未使用与其目的（deferred），并写清现状的失效传播实际路径（重扫页文件）。

### RV-D-5（low · 标定措辞与行为不符）手改冻结的 `/wiki/pages` `frozen_by=null`
* 代码：`wiki.rs:2818 frozen_by: frozen.map(|c| c.kind.as_str().to_string())`，`frozen` 取 `active_correction`（pin 才有值）。V-D 读数：`VD T4 build#2 … frozen_by=null stale=true freshness="stale"` —— 页被 §13-3 冻结、`error` 与 `wiki_corrections` 都带 reason，但 `/pages` 的 `frozen_by` 为空。
* 规格 G6 的标定读数写「`/wiki/pages` 的 `frozen_by` 非空」，在**手改**这条路上不成立 ⇒ 面板若用 `frozen_by` 解释「为什么冻结」会看到空值。
* **requiredFix**：二选一 —— 让手改冻结也带 `frozen_by`（用自动 `note` 的 kind/来源），或把 G6 的标定读数改成「`wiki_corrections` 有 1 行且 `error` 带 reason；`frozen_by` 仅 pin 路径非空」。

### RV-D-6（low · 过期计数）作者摘要里的 `13 passed` 是旧数
* 我实测 **14 passed**（§1.1 ②），作者报告 §8.3 正文与 V-D 的独立读数也都是 **14**（改前 4、新增 10）。任务摘要那句 13 需要订正。
* **requiredFix**：把摘要与报告里的计数统一为 14（并注明改前 4）。

---

## 5 观察（不进 findings）

* **O-1（同伴在途，按 F5 只报文件与错误）**：我第一次跑契约命令时 `crates/daemon/src/memembed.rs:529` 与 `crates/memory/src/dedupe.rs:342` 的 `merge_audit_reason` 签名不匹配（`&MergeVerdict` vs `&MergeAudit`）⇒ daemon lib 编译失败、exit 101。那是 **t31 在途**，不是 I-D 的文件；同一命令几分钟后（同一字节）绿。**我未替同伴改任何文件。**
* **O-2（修法很小的证据）**：同一读路径对 `stale_since` 读的是记录值（`wiki.rs:2791`），只有 `cite_coverage` 走重算 ⇒ RV-D-1 的 (a) 是**同一函数体内的一处取数口**，不是重构。
* **O-3（证据污染标注）**：impl 报告 §1 的 G1 行把「落地页 `cite_coverage = 1.00`」当作标定读数 —— 按 captain 的 (d) 必须标注为受 RV-D-1 污染；G8 同理。
* **O-4（口径）**：本单所有 wiki 读数都来自临时 root（V-D 自建、我跑契约测试时由测试自建 `%TEMP%\ruagent-wiki-*`）；**真库上的 wiki 读数未测**，因为纪律禁止在真库上跑 build、禁止写 `~/.ruagent`。

---

## 6 未测 / 不判定

| # | 项 | 状态 | 原因 |
| --- | --- | --- | --- |
| U-1 | 真库（活库）上的 wiki 读数 | **未测** | 纪律：不许在真库跑 build、不许写 `~/.ruagent` |
| U-2 | 我自己重跑「旧树 8/8 落地」的两棵树 | **未测** | 作者 §3.4 与 V-D 都已给（`git archive HEAD` + 独立 target）；我这一轮给的是**代码面锚点确认 + 新树负例的我自己的绿**（§1.2/§1.3） |
| U-3 | `clippy -D warnings` 独立复跑 | **未测** | 不在本单验收命令里；作者自述绿并有 `-CleanFirst` 的取得方式 |
| U-4 | G3/G4/G6/G9 的旧代码侧（部分） | 引用 V-D | 旧树上没有 `wiki_corrections`/`stale_since`（0022 才建），判据在旧树**无法表达**；V-D 已用混合态补 U-3 |
| U-5 | G8 的注入面 2/2 | **not_measured** | owner 分工（I-B 渲染 + I-INT 构造点），终判 t20/t21；**且被 RV-D-1 污染，修好后必须重读** |
| U-6 | 「四面一致」的第 4 面（注入线索卡） | **未测** | 它就是 G8 的消费端（U-5）；`/wiki/pages`、`/wiki/links`、`index.md` 三面有读数 |

---

## 7 复现命令（我实际跑的）

```powershell
$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"     # 每次 cargo 都走全队包装脚本，未自设 CARGO_TARGET_DIR
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-mock-agent --test wiki_pipeline
# → 第二次 14 passed / 0 failed（exit 0）；第一次 exit 101 = t31 在途的 memembed.rs:529（见 O-1）
git status --porcelain -- crates panel                 # 我唯一新增 docs/design/reviews/gen2-wiki-review.md
# 我自己的 code 面判据（grep，不经作者转述）：
#   cite_coverage_of :2931-2960 · 读面 :2789/:2861/:2890/:3128 · 构建期真值 :3334-3364
#   对该表的 SELECT 只有 :3609（slug, stale_since）⇒ 记录值无人读回
#   stale_since 只写一次 :1014-1015 · index.md 标记 :2461 · confirm_plan 400 分支 wiki.rs:1727 + api.rs:296
```

---

## 8 纪律回执

* **只写本单 inScope**：本文件 `docs/design/reviews/gen2-wiki-review.md`。`crates/` 与 `panel/` **一行未改**。
* **读数三件套**：对象集（`wiki.rs 45AA136091C5D17D` @01:10:13 · `wiki_pipeline.rs F881F6B272945291` @01:10:10，自 t10 收敛后未变）· 采样面（契约命令 14 条用例逐条点名 + 我自己的 code 面 grep + V-D 的独立夹具引用）· 可证伪判据（负例族的断言内容我逐条读过；旧代码侧引用时**标明是作者/V-D 的读数**）。
* **期望值只写一处**（规格 §C 与本报告 §2），判据从代码与测试取，**没有复制常量**。
* **未测一律写明**（§6，6 项），其中 G8 按 owner 分工判 **not_measured**，**不据此判 I-D 失败**。
* **真守护进程 pid 79984 未启停**；`~/.ruagent` 只读；**未在真库上跑 wiki build**；**未用** `GET /api/v1/recall`。
* **并发声明**：本单两次契约尝试恰好演示了并发波次（第一次撞 t31 的 `memembed.rs:529`，第二次同字节绿）；我按 F5 只报「哪个文件、什么错」，不替同伴改、不把它算成 I-D 的回归。
