# V-D 独立验证（t14）：I-D / t10 Wiki 实现的引用可验证性与新鲜度负例复核

> **状态**：独立复核**完成**。结论 **pass（带 3 条 finding：1 条 medium 是交付面内的读数缺陷，1 条 low 是判据口径，1 条 low 是空表；另有 2 条 observation）**。
> **验证者**：verify2（独立验证员，**不是** I-D 的作者）。本单只写本文件；**未改任何被验代码**（`crates/` 与 `panel/` 一行未动，见 §8）。
> **时间窗**：2026-09-28T01:50 → 03:0x +08:00（现场读数各条带自己的时间窗）。
> **被验产物**：`crates/daemon/src/wiki.rs`（+2133/−194）与 `crates/mock-agent/tests/wiki_pipeline.rs`（+1003/−139），作者报告 `docs/design/reviews/gen2-wiki-impl.md`。
> **纪律**：
> * 真守护进程 **pid 79984 未启停**；`~/.ruagent` 未写；**未在真库上跑 wiki build**（我所有构建都在自己创建、自己删除的临时 root 上）。
> * **绝未为取「改前」读数改动共享工作树**：改前一律 `git archive HEAD` 导出树 + 另一个 `-TargetDir`（§4）。
> * 编译全部走 `scripts/cargo-team.ps1`（单飞锁、共享 target、0–11 核），`-DryRun` 先核形状；**没有两条 cargo 并发**。
> * 我的探针文件只写进 `%TEMP%\v14-tree`（当前树快照）与 `%TEMP%\v14-head`（HEAD 导出树），**仓内一行未加**。

---

## 0 一句话

I-D 的**写侧闸门是真的**：我自己造的 **7 个不可验证页面全部被拒**（`pages_failed=1`、页文件不落盘、不进库存，且错误行逐条点名族名），**同一个闸门在改动前的代码上 8/8 全部以 `written` 落地**；新鲜度的时间窗（改写来源 → 立刻 `stale=true` 而 `stale_since=null` → 下一次构建才写、且**只写一次**）与"计划说 `keep` 也必须修"在我的时间轴上逐条复现。**但我在读侧找到一条真缺陷**：`cite_coverage` 这个读数**在构造上只能是 0.0 或 1.0**（分子与分母取自同一个 `meta.citations` 列表），三个消费面（`/wiki/pages`、`lead_from`、`lead_for`）全都用它 —— 我自己造的两页里它报 `1.0` 的同时，**同一个响应**里的 `uncited_sections` 正列着那一节是未引用的；一张**根本没有 `wiki_pages` 行**的手写页也报 `1.0`。G1 的**准入**成立，G1 的**读数**没有实现。

---

## 1 待验目标表（来自 `gen2-wiki-spec.md` §C，G1–G9 + §C 总门）

| G | 判据（压缩） | 我的采样面 | 结论 |
| --- | --- | --- | --- |
| **G1** | 任一 content section 无锚 ⇒ 该页 `failed`；`written` 页 coverage = 1.00 | 我自己 3 chunk 的来源 + 2 节全锚的正例；7 个负例 | **准入 ✓ / 读数 ✗ → F1** |
| **G2** | 2a 缺 `## 来源` / 2b 名单不符 / 2c 未引用 三条比率 = 0 | 我自己的三族负例 + 逐条归因 | **✓** |
| **G3** | `stale_persistence=1.00`；四面一致；`unknown` 第三态必须出现 | 我的漂移时间轴（改来源、手改、两次构建） | **✓（口径见 F2）** |
| **G4** | 被判 stale 的页必须在下一次构建里 `update`，一次构建内完成 | 计划写 `keep`、我实测 `action=update` | **✓** |
| **G5** | 5a 两面 `links_out` 一致 / 5b `wanted` 带需求方 / 5c 每个 done 构建 1 行图读数 / 5d 自链可见 | 我自己的两页 + 断链 + 自链 | **✓（含库外读数）** |
| **G6** | 被 skip 的页必须有 `wiki_corrections` 记录、`reason` 非空 | 我的手改冻结 + 两次构建 | **✓（`frozen_by` 观察见 O-2）** |
| **G7** | 7a 单一 plan-only 词汇 / 7b 组合请求 400 点名两字段 / 7c 不写 `pending` 页行 | 我自己的一次干跑 + 组合请求 | **✓（7c 走规格允许的路线 B，见 O-3）** |
| **G8** | `<wiki>` 块每行带 `stale=`/`coverage=`/`anchors=`/`hint`（2/2） | 生产侧存在、消费侧无调用者 | **not_measured（owner I-B/I-INT）** |
| **G9** | 改名走 `update`+`aliases`，一次 rename 里 `deleted` 行 = 0 | 我自己的改名计划（旧 slug 保留为 aliases） | **✓** |

**§C 总门**：G1 ∧ G2 ∧ G3 ∧ G7 为硬门。G1 的**准入**成立、**读数**不成立（F1）；G3 的第三态只在「KB 整体无 documents」形态下出现（F2）。两条都写在 §5，请 RV-D 按总门口径裁决（我**不**替评审下判决）。

---

## 2 独立读数表

### 2.1 命令与编译面

| 命令（全部经包装脚本） | 编译面 | 结果 |
| --- | --- | --- |
| `cargo-team.ps1 test -p ruagent-mock-agent --test wiki_pipeline`（**共享 target**，当前树） | `ruagent-mock-agent` integration target `wiki_pipeline`（依赖面含 `ruagent-daemon` lib、`ruagent-knowledge`、lancedb/lance 链） | **exit 0**，`14 passed; 0 failed; 0 ignored; 0 measured`（18.13s，`[cargo-team] exit=0 elapsed=56.3s`） |
| `... test -p ruagent-mock-agent --test vd_probe -TargetDir %TEMP%\ruagent-cmp-t14-now`（快照 `%TEMP%\v14-tree`） | 我自己的 integration target | **exit 0**，`5 passed; 0 failed` |
| `... --test vd_probe2`（同上） | 同上 | **exit 0**，`3 passed; 0 failed` |
| `... --test vd_probe3`（同上） | 同上 | **exit 0**，`2 passed; 0 failed` |
| `... test -p ruagent-mock-agent --test vd_head_probe -TargetDir %TEMP%\ruagent-cmp-t14-now`（**改动前树** = `git archive HEAD` → `%TEMP%\v14-head`） | HEAD 的整条 workspace 依赖面 + 我的探针 | **exit 0**，`4 passed; 0 failed`：引用闸门探针 **8/8 以 `written` 落地**；干跑探针 **两种词汇 + 组合请求 200**；改名探针 **旧页 `skipped`/被留在原地 + 新页 `written`**（§4） |
| `... check -p ruagent-daemon`（当前树） | daemon lib | **exit 0**，`Finished dev profile … in 4.50s`（用于判定同伴在途编辑已收敛，见 §7-O-1） |
| `git status --porcelain -- crates panel` | — | **exit 0，50 行，无一行属于我**（§8） |

### 2.2 G1：正例（我自己造的页）

```
VD T1 chunk ids of MY source = [1, 2, 3]
VD T1 build status="done" written=1 failed=0 row.status="written" row.error=null
VD T1 ON DISK: content_sections=2 ["写入路径","读取路径"] cited_sections=["写入路径","读取路径"] coverage=1.0000 verified_line=true
VD T1 /wiki/pages: stale=false freshness="fresh" built_at="2026-09-27T18:07:49.726419800+00:00" edited=false frozen_by=null
VD T1 API fields: cite_coverage=1.0 citations=2 uncited_sections=[]
```
**两个独立读者一致**：我**自己在 crate 外解析落地文件**（前 8 行字符级解析 frontmatter 的 `citations:` 与正文 `## ` 节）得 2/2；接口字段 `citations=2`、`uncited_sections=[]`、`cite_coverage=1.0`。D.1 的盘上不变量（citations 的节集合 ⊇ 正文 content section 集合）成立。

### 2.3 G1/G2：负例（我自己的 7 个族，每个都是独一份的新夹具）

```
VD T2 [one of two content sections has no anchor]                 written=0 failed=1 row.status="failed" error="citation check failed (1 problems); uncited section [读取路径]: section `读取路径` of page `engine-notes` has no citation anchor"
VD T2 [no sources section at all]                                  written=0 failed=1 error="…missing sources section: the body has no `## 来源` section"
VD T2 [anchor points at a chunk id that does not exist]            written=0 failed=1 error="…(2 problems); dangling citation [写入路径]: engine-src#9999 does not exist; uncited section […]"
VD T2 [anchor points at a document that is not this page's source] written=0 failed=1 error="…(2 problems); unaligned citation [写入路径]: foreign-notes#1 is not one of this page's sources; …"
VD T2 [the sources section names a document the page never cites]  written=0 failed=1 error="…sources section mismatch: listed [\"engine-src\", \"extra-notes\"] but the page's sources are [\"engine-src\"]"
VD T2 [nothing that can be cited: no content section at all]       written=0 failed=1 error="…no content section: the body has no content section (## H2) — nothing that can be cited"
VD T2 [MY OWN EXTRA: the only anchor is inside a fenced code block] written=0 failed=1 error="…uncited section [写入路径]: …has no citation anchor"
```
每一族都额外断言：**页文件不存在**（`knowledge/wiki/engine-notes.md`）且**不在 `/wiki/pages` 库存里**。三族 2a/2b/2c 与"空集族"（`no content section`，且**只报这一个** `1 problems`）逐条归因正确。

**我自己的补充族（作者没测过的形状）**：`<!-- cite: … -->` 写在 ``` 围栏里 → 判为 `uncited section` ⇒ 报告里"`citations_in` 跳过代码围栏"的说法**成立**。
**双缺陷归因**：一节无锚 + `来源` 多列一个文档 → `error="citation check failed (2 problems); sources section mismatch: …; uncited section […]"` ⇒ **一次报全部族**，不是只报第一个。

### 2.4 G3/G4：我自己的新鲜度时间轴

```
VD T3 step1 after create:                stale=false freshness="fresh" stale_since=null  stale_sources=[]
VD T3 chunk ids after the re-ingest = [10, 11, 12]
VD T3 step2 drift, BEFORE any build:     stale=true  freshness="stale" stale_since=null  stale_sources=["engine-src"]
VD T3 step3 after changed-scope build:   row.status="written" row.action="update" stale=false freshness="fresh" stale_since=null
VD T3 the repair re-cited: new ids=10 old ids=1 frontmatter cites=["chunk_id: 10","chunk_id: 11"]
```
* **观测窗口**：漂移在**没有任何构建**时就已经可读（`stale=true`+`stale_sources`），但 `stale_since` 仍是 `null` —— 与作者登记的语义（"首次被某次构建观测到的时刻"）逐字一致。
* **G4**：计划写的是 `keep`，执行器记的是 `action="update"` + `status="written"`，**一次构建内**完成，重建后 `stale=false`、`stale_since` 被清空，且**重新引用**了重新入库后的新 chunk id（旧的 1 已不在页面里）。

```
VD T4 build#2: row.status="skipped" row.error="human-edited since last build — skipped (§13-3); recorded: human-edited since build #1"
               frozen_by=null stale=true freshness="stale" stale_since="2026-09-27T18:07:57.258717600+00:00"
VD T4 index.md carries `⚠️ 源已更新`: true
VD T4 build#3: row.status="skipped" stale_since="2026-09-27T18:07:57.258717600+00:00" unchanged=true
VD T4 corrections rows = 2: [("daemon","human-edited since build #1","18:07:58.609…"),("daemon","human-edited since build #1","18:07:57.228…")]
```
* **`stale_since` 只写一次**：第三个构建后逐字节不变（我在两次构建之间 sleep 了 1.1 s，确保"没变"不是时间分辨率造成的）。
* **`index.md` 与 API 一致**（`⚠️ 源已更新`），**冻结有记录**（每次 skip 一条 `wiki_corrections`，author/reason/at 齐全；两次构建两条 —— 记录是"每次 skip 一条"，不是幂等一条）。
* **G6 观察 O-2**：手改冻结时 `frozen_by=null`（只有 pin 才会填）——规格 G6 的标定读数写"`/wiki/pages` 的 `frozen_by` 非空"，在**手改**这条路上不成立。

### 2.5 G5：我自己的图形状（+ 库外读数）

```
VD T5 wanted=[{"slug":"wal-internals","demanders":["storage-notes"],"demand_count":1}]
VD T5 self_links=[{"src":"storage-notes","dst":"storage-notes"}]
VD T5 broken=["wal-internals"]
VD T5 compaction-notes: pages.links_out=1 links.links_out=1 | links_in 1/1 | links_out_broken 0/0
VD T5 storage-notes:    pages.links_out=2 links.links_out=2 | links_in 1/1 | links_out_broken 1/1
VD T5 storage-notes links_out=2 (two distinct non-self targets: compaction-notes + wal-internals)
```
**库外（python sqlite3 `mode=ro`，不经任何 Rust API）**：
```
T5 root: wiki_builds=[(1,'done',0,'all',2)]  wiki_graph_readings=[(1,2,2,1,0,0,1,'…18:05:00.073…')]
T7 root: wiki_builds 2 行 done → wiki_graph_readings **2 行**（每个 done 构建 1 行 ✓）
```
⇒ 5a（两面逐位一致）、5b（需求方）、5c（每次完成构建 1 行）、5d（自链可见且**不计入出度**）全部成立。

### 2.6 G7：干跑单源

```
VD T6 dry-run response status="planned" build row status="planned" dry_run=true
VD T6 combined request -> HTTP 400 body="dry_run and confirm_plan are mutually exclusive: dry_run asks for a new plan to review, confirm_plan executes a plan that already exists (got dry_run=true with confirm_plan=1)"
```
**库外**：
```
T6 root: wiki_builds=[(1,'planned',1,'all',1)]  plan-only 分组 = [('planned',1,1)]   ← 只剩一种表示
         wiki_build_pages=[(1,'engine-notes','create','planned')]                     ← 一行，status='planned'
         wiki_pages=[]                                                                ← 只读计划不产生页库存
```
⇒ 7a（响应与行同词）、7b（400 且点名两个字段）成立；7c 见 O-3（`pending` 已 0，但**写了**一行 `status='planned'`，规格 DDL-5 明确允许的**路线 B**）。

### 2.7 G9：改名

```
VD T7 old.status="renamed" old.action="delete" old.error="renamed to engine-storage" | new.status="written" new.action="create"
VD T7 inventory: slugs=["engine-storage"] aliases=["engine-notes"]
VD T7 库外: wiki_build_pages=[(1,'engine-notes','create','written'),(2,'engine-notes','delete','renamed'),(2,'engine-storage','create','written')]
```
⇒ 一次改名构建里 **`deleted` 状态行 = 0**，旧 slug 作为 `aliases` 活在新页上 ✓（`action` 列仍忠实记 `delete`、`status` 记 `renamed`）。

### 2.8 G8：生产侧在、消费侧无调用者（not_measured 的依据）

```
crates/daemon/src/wiki.rs:2845  pub fn lead_from(kb,slug) -> Option<WikiLead>
crates/daemon/src/wiki.rs:2874  pub async fn lead_for(kb,recorded_hash,slug) -> Option<WikiLead>
crates/daemon/src/wiki.rs:3009  let lead = lead_from(kb, slug);          ← 唯一活调用，仍在 wiki.rs 内
crates/daemon/src/memembed.rs:662  pub fn lead_meta(lead) -> WikiLeadMeta  ← 适配器存在
crates/daemon/src/memembed.rs:633  // crate::wiki::lead_for(kb, None, slug).await.as_ref().map(lead_meta)  ← **注释里的组合样例**
crates/daemon/src/memembed.rs:751  let meta = lead_meta(&lead);            ← 在测试里
```
⇒ `<wiki>` 块的标记面**没有任何生产调用者**（`lead_meta` 只在测试与 doc 样例里出现）⇒ G8 只能记 `not_measured`，与作者 D1 一致；owner = I-B（`crates/memory` 的块渲染）/ I-INT（`chat.rs`/`runs.rs` 的构造点）。

---

## 3 差异表：「作者读数 → 我的独立读数 → 差异」

| 项 | 作者读数（`gen2-wiki-impl.md`） | 我的独立读数 | 差异 |
| --- | --- | --- | --- |
| 契约测试 | 摘要写「**13 passed**」；§8.3 R0 正文写「running **14 tests** … 14 passed」 | `--test wiki_pipeline` = **14 passed / 0 failed**，同一份 14 个用例名单 | 作者**摘要里那个 13 是过期的**（正文自己是 14）⇒ O-4（low，文档） |
| 改前两侧 | 改前（B-17）= **4 passed** | HEAD 文件里 4 个 `#[tokio::test]`（名单与作者一致） | 无 |
| 引用闸门负例 | 「6 个负例族 + 第 7 族归因用例」 | 我用**自己的 7 个族**独立复现，另加"围栏内锚"族；双缺陷页一次报两族 | 无（我多测了两个形状） |
| 旧代码反证 | 预编译二进制（sha256 `CB8B55A0…`，mtime 22:18:32）+ 自写 PowerShell HTTP 探针：**6/6 written** | **`git archive HEAD` 导出树 + 我自己编译的 HEAD 探针：8/8 以 `written` 落地**（7 个不可验证 + 1 个正例），`pages_failed=0`、`error=null`、文件都在盘上 | **无差异（更强的独立仪器）**：作者用旧二进制，我用旧**源码树**重编译 |
| G3 `stale_since` | 「改写来源后立刻读：`stale=true` 而 `stale_since=null`」（正面断言） | 同（我的时间轴 step2） | 无 |
| G3 第三态 | 测试用例 `an_unanswerable_freshness_reading_is_unknown_never_fresh` 用**空 KB + 空 `sources`**；报告 §2.2 写「来源文档在 KB 里没有 row（`chunk_index` 报 unknown）⇒ `stale=None`」 | **两种形态我都测了**：空 KB ⇒ `unknown` ✓；**非空 KB + 页里点名的来源没有 row ⇒ `stale`（`stale_sources=[]`）** | **差异 ⇒ F2** |
| G1 读数 `cite_coverage` | 「`written` 页 4 页 `cite_coverage=1.0`」；§1 表格把这条当达成证据 | 正例 1.0 我复现了；但**它按构造恒为 0.0 或 1.0**，且**同一响应**里可以同时 `cite_coverage=1.0` 与 `uncited_sections=["事实"]`；一张**无 `wiki_pages` 行**的手写页也报 1.0 | **差异 ⇒ F1（medium）** |
| G6 `frozen_by` | pin 用例断言 `frozen_by=="pin"` | 手改冻结（§13-3）时 `frozen_by=null`，而 `error` 与 `wiki_corrections` 都有 reason | **差异 ⇒ O-2（observation）** |
| G7c 页行 | 「只读计划的页行**不再写 `pending`**」 | `pending` 行确实 0；但**写了 1 行 `status='planned'`**（规格 DDL-5 的**路线 B** 明确允许） | **差异 ⇒ O-3（observation，非 finding）** |
| G5c 图读数 | 测试断言"每页边/度一致" | 库外读：`wiki_graph_readings` **每个 done 构建恰好 1 行**（1 构建→1 行；2 构建→2 行） | 无（我补了库外读数） |
| `wiki_citations` | 规格 E.2 DDL-2 请求该表（"可按 chunk_id 反查受影响页"）；store 报告记"落地" | 三个独立 root 里 `wiki_citations` **都是 0 行**；全树 grep：**没有任何 Rust 代码读写它**；锚实际存在 `wiki_pages.citations_json` + 页 frontmatter | **差异 ⇒ F3（low）** |
| test 计数与 schema 版本 | — | 我快照的 root `schema_migrations max=25`（t12 时是 24） | 记录（跨区，非 I-D） |

---

## 4 可证伪性复核（新判据在改动前的代码上必须红）

**对象集**：`git archive HEAD`（HEAD = `0a39e5b801ea30cb8189cd6d10c4480ddfeedbfe`，"fix(memory): the distilled badge comes from the episode KIND (t350)"，2026-09-27 05:37:57 +0800）导出到 `%TEMP%\v14-head`；**独立 `-TargetDir %TEMP%\ruagent-cmp-t14-now`**；探针 `crates/mock-agent/tests/vd_head_probe.rs`（只存在于该导出树）。

**结果（我自己跑出来的原文）**：
```
VD-HEAD [CANONICAL: every content section anchored, sources correct]        build="done" written=1 failed=0 row.status="written" file=true error=Null
VD-HEAD [one of two content sections has no anchor]                          build="done" written=1 failed=0 row.status="written" file=true error=Null
VD-HEAD [no sources section at all]                                          build="done" written=1 failed=0 row.status="written" file=true error=Null
VD-HEAD [anchor points at a chunk id that does not exist]                    build="done" written=1 failed=0 row.status="written" file=true error=Null
VD-HEAD [anchor points at a document that is not this page's source]         build="done" written=1 failed=0 row.status="written" file=true error=Null
VD-HEAD [the sources section names a document the page never cites]          build="done" written=1 failed=0 row.status="written" file=true error=Null
VD-HEAD [nothing that can be cited: no content section at all]               build="done" written=1 failed=0 row.status="written" file=true error=Null
VD-HEAD [the only anchor is inside a fenced code block]                      build="done" written=1 failed=0 row.status="written" file=true error=Null
VD-HEAD total bodies that landed as written on 0a39e5b8 = 8/8
```
⇒ **改前：8/8 落地、`pages_failed=0`、`error=null`、文件都在盘上**；改后：**同样的 7 个不可验证体全部 `failed`、文件不落盘**。旧代码的 stage-3 只有"以 `# ` 开头 + 长度上限"两条校验，所以坏侧在旧代码上**不是判错，而是压根没有被判**。

**G7 的旧代码侧（同一导出树、同一探针）**：
```
VD-HEAD G7a dry-run: response status="planned" | build ROW status="planned_only" | dry_run=true   ← 两种词汇
VD-HEAD G7b combined request -> HTTP 200 body="{\"build_id\":2,\"status\":\"planned\",…}"          ← 200 且真的新建了一行 build
```
⇒ 改后 7a 是**一个词**（`planned`/`planned`）、7b 是 **400 点名两字段**；两侧都是我自己跑出来的对立读数。

**G9 的旧代码侧（同一导出树、同一探针，加了 `RUST_TEST_THREADS=1`）**：
```
VD-HEAD G9 rename-as-delete+create: rows=["engine-notes=skipped", "engine-storage=written"] written=1
```
⇒ 旧代码上"改名"（计划 `delete`+`create` 同一来源）**没有移动主题**：旧页被**留在原地**（`skipped`，不是 `deleted` —— 旧代码自己的删除守卫拒绝删掉仍有存活来源的页），新页另建 ⇒ **主题被复制、旧 slug 不记 `aliases`**。改后：`old.status="renamed"`、`error="renamed to engine-storage"`、库存只剩 1 页且 `aliases=["engine-notes"]`（§2.7）。两条读数方向相反、都是我自己跑的。

**没有取得旧代码侧读数的项（写明，不冒充）**：
* **G3/G4/G6/G9 中的 G3/G4/G6**：我这一轮**没有**在 HEAD 树上重跑这三条 —— HEAD 没有 `wiki_corrections` 表（0022 才建），也没有 `stale_since` 列，所以它们在新代码里的判据在旧树上**无法表达**（不是"跑绿了"）。作者在 §3.4 自述同样只有"源码坐标 + 现场读数"，没有旧二进制重跑。⇒ 见 §6 U-2。
* **G9 的旧侧**：第一次尝试没有取得读数（两个改名用例在并发下撞上 HEAD 的进程级单飞标志，响应体为空导致 `reqwest Decode` 失败）；**加 `RUST_TEST_THREADS=1` 重跑后取得**，读数见上。这条同时也说明：**探针必须自己钉住线程数**，否则 HEAD 的单飞标志会把并发构建变成空响应。

---

## 5 findings

**F1（medium，I-D 交付面内 —— 读侧读数缺陷）`cite_coverage` 在构造上只能是 `0.0` 或 `1.0`，三个消费面全用它。**
* 复现（三条，任一条都足以定位）：
  1. **代码**：`cite_coverage_of(meta)`（`wiki.rs:2931-2945`）分子 = `meta.citations` 里 distinct 的 content section，分母 = `content_section_count(meta).max(分子)`（`wiki.rs:2950-2960`），而 `content_section_count` **从同一个 `meta.citations` 算同一个集合** ⇒ 分母恒等于分子（或 0）⇒ 返回值只能是 `0.0` / `1.0`。
  2. **读数**：我自己造的页上 `VD T8-B slug="orphan-vd" … cite_coverage=1.0 citations=1 uncited=**["事实"]**` —— **同一个响应**里 `cite_coverage` 说 100%，`uncited_sections` 说那一节没有活锚。空 KB 的第三态页（`VD T8-A`）同样是 `cite_coverage=1.0` 而 `uncited=["事实"]`。
  3. **库外**：`orphan-vd` 在 `wiki_pages` 里**根本没有行**（我在 crate 外用 sqlite3 只读查过），接口仍报 `1.0`。
* 期望/实际：规格 G1 的 metric 是 `cited_content_sections / content_sections`，其中 cited = "该节至少一条锚**解析到活 chunk 且属于本页 sources**"。实际读侧三个面（`/wiki/pages` 的 `WikiPageInfo.cite_coverage`（`wiki.rs:2789`）、`lead_from`（`:2861`）、`lead_for`（`:2890`），另 `:3128` 的 JSON）**都用这个恒等式**；而**构建期真正算出来的覆盖率**（`report.coverage`，`wiki.rs:3334`，写进 `wiki_pages.cite_coverage`）**没有任何读路径读回**。
* 影响：① 规格 G1 的标定读数「written 页 4 页 `cite_coverage=1.0`」是**构造性成立**的伪读数，任何页只要有一条 citation 就是 1.0；② 面板/lead 无法区分"每节都有活锚"和"锚根本不解析"；③ G8 要注入 `<wiki>` 的正是 `WikiLead.cite_coverage`（`:2890`），会把常量带进注入面。
* **写侧闸门不受影响**（我 7/7 负例被拒、8/8 在旧代码上落地），所以这是**读数**缺陷、不是准入缺陷。
* 建议修法（不替作者改）：`WikiPageInfo`/`WikiLead` 的 `cite_coverage` 改取 `wiki_pages.cite_coverage` 记录值（并由 `uncited_sections_of` 兜底），或直接由"活锚解析"那一遍算出来；`cite_coverage_of` 这个名字要么删掉要么改名成"是否有锚"。

**F2（low，判据口径 vs 实现）G3 的第三态只在「KB 整体没有 documents」时出现，规格点名的「KB 无该来源 row」形态读 `stale`。**
* 复现（两个夹具，都在我的探针里）：
  * `VD T8-A kb_documents=0 … freshness="unknown" stale=false stale_since=null stale_sources=[]`（**空 KB** ⇒ 第三态 ✓）
  * `VD T8-B slug="orphan-vd" freshness="stale" stale=true stale_sources=[]`（**非空 KB + 页里点名的 `never-src` 没有任何 row** ⇒ 读 `stale`，不是 `unknown`）
* 判据出处：规格 G3 目标写「`stale=unknown` 第三态在**「KB 无该来源 row」**时必须出现」；D.5 的结构体注释写 `None = 不可判定（KB 不可用、**来源 row 不存在**）`；但 D.5 的 `StaleReason` 又把 `SourceMissing`（来源文件消失）与 `ChunkMissing` 定义为**stale 的原因**。实现走的是后者：`freshness_with`（`wiki.rs:853+`）里"整个 KB 没文档（`kb_docs == 0`）"才置 `unknown_cause`，否则缺失的锚记 `ChunkMissing` ⇒ `stale`。
* 期望/实际：按规格**字面**（"KB 无该来源 row 必须出现 unknown"）不成立；按 D.5 的枚举语义（缺失来源是一种**确定**的失效）成立。方向是 **fail-safe**（不是把不可判定算成 `fresh`），所以我不判它坏，只判**判据与实现的形状不一致**，需要 RV-D/作者二选一：把来源级 `row 查不到` 走 `unknown`，或把 G3 的措辞改成「KB 整体无 documents row」。
* 另一条同族读数：这两种形态下 `stale_sources` 都是 **空数组**（我的夹具 `source_hashes: {}` 没给名字、`stale=true` 却没有来源名），读者无法从 `/pages` 知道"为什么 stale"。

**F3（low，空对象）`wiki_citations` 表被建出来，但全树没有任何读写者，三个 root 里都是 0 行。**
* 复现：`grep wiki_citations`（全树）只有 `0022_wiki_gen2.sql` 的 DDL/索引、`migrations.rs` 的表清单、`gen2-store-impl.md` 的"落地"登记 —— **没有任何 `.rs` 读写它**；我在 crate 外用 sqlite3 读三个 root：`wiki_citations (doc, chunk, page): []`（三个 root 全空），而同期 `wiki_pages.citations_json` 里锚是齐的（例如 `[{"section":"WAL","document":"storage-src","chunk_id":1,"chunk_hash":"125c9e2c…"}]`）。
* 期望/实际：规格 E.2 DDL-2 的目的写着「可按下游 `chunk_id` 反查受影响页 —— 失效传播的索引面」，即"哪些页引用了这个 chunk"。实际实现把锚放在 `wiki_pages.citations_json` + 页 frontmatter，失效传播靠**重新扫描页文件**（`uncited_sections_of` 就是读文件），所以那张反查索引**在实践上不存在**。
* 影响：低（G1–G9 没有一条依赖它；但它是一条**没有写入者的验收对象**，谁读完 DDL-2 都会以为存在反查能力）。建议：要么 I-D 在 `record_page_row` 时顺带写 `wiki_citations`，要么 I-SCHEMA 记录该表为"未使用"。

**O-1（observation，同伴在途编辑，不是 I-D 的缺陷）我的前两次取树撞上 `crates/daemon/src/distill.rs` 的中间态。**
* 第一次快照：`error[E0282]/[E0283] type annotations needed for `Result<usize, _>``，位置 `distill.rs:388`（`void_episode` 的 `.call(move |conn| {…})` 缺返回类型）；第二次快照：`error[E0599] no method named `merge_target` found for reference `&Distiller``。两次都是同伴正在改 C2 接线（captain 已把它并进 t27/graph）。**按纪律只报"哪个文件、什么错"、不替他修**；他们在几分钟内收敛后，`check -p ruagent-daemon` = exit 0（4.50s），我的三次探针与 HEAD 探针均正常编译。
* 影响：**只影响我的取树时机**（我重取快照后全部读数都来自收敛后的树，且三次探针的运行时间戳在同一天 18:04–18:08 UTC，与收敛点一致）。**G9 的旧代码侧读数因这件事没取到**（§6 U-3）。

**O-2（observation）手改冻结的 `frozen_by` 是 `null`。**
`VD T4 build#2: … frozen_by=null stale=true freshness="stale"` —— 页被 §13-3 冻结、`error` 与 `wiki_corrections` 都带 reason，但 `/wiki/pages` 的 `frozen_by` 只有 **pin** 才有值（`wiki.rs:2818` 取 `active_correction` 的 kind）。规格 G6 的标定读数写"`/wiki/pages` 的 `frozen_by` 非空"，在**手改**这条路上不成立；面板若用 `frozen_by` 展示"为什么冻结"会看到空。低，登记即可。

**O-3（observation，不是 finding）只读计划仍写一行 `wiki_build_pages`，但状态是 `planned`。**
库外：`wiki_build_pages=[(1,'engine-notes','create','planned')]`，`wiki_pages=[]`。规格 DDL-5 **明确给了两条路线**（A 不写；B 写但 `status='planned'`），实现走的是**允许的路线 B**，且 `pending` 词汇确实 0 行 ⇒ 判据（"不写 `pending` 页行"）成立。记录它只是为了让下游知道"路线 A 的进一步收敛（词汇 2→1）还没做"。

**O-4（low，文档）作者摘要里的 `13 passed` 是过期数字。**
同一份报告 §8.3 R0 的正文写的是 `running 14 tests … 14 passed`，我独立跑到的也是 **14 passed**（名单逐条一致）。改前 4、改后 14、新增 10 —— 与 §8.3 的结论一致；只是任务摘要那句 13 需要订正。

---

## 6 未测项（写明原因，不写成 0）

| # | 项 | 状态 | 原因 |
| --- | --- | --- | --- |
| U-1 | **G8**`<wiki>` 块的 `stale=`/`coverage=`/`anchors=`/`hint` 2/2 | **not_measured** | 生产侧 `WikiLead` 齐了，但**消费侧没有任何活调用者**（`lead_meta` 只出现在 `memembed.rs:662` 的定义、`:751` 的测试、以及 `:633` 的**注释**样例）。判据不降，owner = I-B（`crates/memory` 块渲染）/ I-INT（`chat.rs`/`runs.rs` 构造点）—— 与作者 D1 一致 |
| U-2 | **G3/G4/G6/G9 的旧代码侧** | **未测** | HEAD 树上没有 `wiki_corrections` 表、没有 `stale_since` 列（0022 才建），这四条的判据在旧树上**无法表达**；作者自述同样只有源码坐标。要取这一读数必须"旧二进制 + 新 schema"的混合态，超出本单可及范围 |
| U-3 | ~~**G9 的旧代码侧**~~ **已取得（补测）** | **已取得** | 第一次没取到（并发撞 HEAD 单飞标志 ⇒ 空响应）；加 `RUST_TEST_THREADS=1` 后取得：`rows=["engine-notes=skipped","engine-storage=written"]` ⇒ 旧代码上改名**不移主题**（旧页留下 + 新页另建）。保留此行是为了记下那个坑 |
| U-4 | 真库（活库）上的 wiki 读数 | **未测** | 纪律禁止在真库上跑 wiki build、禁止写 `~/.ruagent`。本单所有读数都来自我自己创建/删除的临时 root（每个 root 的路径都写在探针输出里） |
| U-5 | 「四面一致」的第 4 面（注入线索卡） | **未测** | 第 4 面就是 G8 的消费端（U-1）；`/wiki/pages`、`/wiki/links`、`index.md` 三面我都有读数 |
| U-6 | E10（`wiki_page_hashes` 对账）/ E11（残余回答带 `read_at`） | **未测** | 本单判据清单按 §C 的 G1–G9 抽取；E10/E11 是作者 §1 另列的条目，我没有独立采样面（它们不属 G 门） |
| U-7 | `clippy -D warnings` 独立复跑 | **未测** | 不在本单验收与 verify 命令里；作者 §8.3 R3 自述为绿（且说明了 `-CleanFirst` 的取得方式），我没有独立重跑它 |

---

## 7 复现命令（我一个不漏地用了这些）

```powershell
# 0) 形状核对 + 契约命令（共享 target）
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-mock-agent --test wiki_pipeline -Nocapture -DryRun
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-mock-agent --test wiki_pipeline
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 check -p ruagent-daemon

# 1) 当前树快照（绝不改共享工作树）：robocopy 排除 .git/target/panel/node_modules/docs
$dst="$env:TEMP\v14-tree"; Remove-Item -Recurse -Force $dst -EA 0
robocopy "C:\Users\19410\Documents\ai\ruagent" $dst /E /XD .git target panel node_modules docs /NFL /NDL /NJH /NJS /NP
#   我的三个探针只写进快照：
#     crates/mock-agent/tests/vd_probe.rs   （G1 正例+7 负例族 / 双缺陷归因 / 漂移→修复 / stale_since 只写一次）
#     crates/mock-agent/tests/vd_probe2.rs  （G5 图 / G7 干跑 / G9 改名；三个 root 故意保留）
#     crates/mock-agent/tests/vd_probe3.rs  （第三态两种形态）
$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"; $env:RUST_TEST_THREADS="1"
foreach ($n in @('vd_probe','vd_probe2','vd_probe3')) {
  powershell -NoProfile -ExecutionPolicy Bypass -File <repo>\scripts\cargo-team.ps1 test -p ruagent-mock-agent --test $n `
    -Nocapture -TargetDir "$env:TEMP\ruagent-cmp-t14-now"
}
#   ⚠ 不要把 --test-threads=1 放在 `--` 之后：包装脚本会把它当成**用例名过滤器**（我第一次得到 0 passed / N filtered out）

# 2) 改动前树（可证伪性）：git archive 导出 + 同一个 -TargetDir（第三方依赖命中缓存，只重编 workspace）
$h="$env:TEMP\v14-head"; Remove-Item -Recurse -Force $h -EA 0; New-Item -ItemType Directory $h | Out-Null
cd "C:\Users\19410\Documents\ai\ruagent"; git archive HEAD | tar -x -C $h
#   在 $h 里放 crates/mock-agent/tests/vd_head_probe.rs（8 个 body + 干跑 + 改名），然后：
$env:RUST_TEST_THREADS="1"   # ← 必须：HEAD 的构建单飞标志是进程级的，并发构建会得到空响应
cd $h; powershell ... cargo-team.ps1 test -p ruagent-mock-agent --test vd_head_probe -Nocapture -TargetDir "$env:TEMP\ruagent-cmp-t14-now"
#   ⇒ exit 0，4 passed：8/8 written；G7a planned vs planned_only；G7b HTTP 200；G9 engine-notes=skipped + engine-storage=written

# 3) 库外读数（不经任何 Rust API）：我保留的 root + python sqlite3 mode=ro
python %TEMP%\v14_db_read.py     # wiki_graph_readings / wiki_build_pages / wiki_pages / wiki_citations

# 4) 边界
git status --porcelain -- crates panel
git diff --numstat HEAD -- crates/daemon/src/wiki.rs crates/mock-agent/tests/wiki_pipeline.rs
```

---

## 8 纪律回执

* **只写本单 inScope**：`docs/design/reviews/gen2-wiki-verify.md`。`git status --porcelain -- crates panel` = **50 行，全部是同伴的在途编辑，无一行属于我**；被验面改动量（作者的）：`wiki.rs` **+2133/−194**、`mock-agent/tests/wiki_pipeline.rs` **+1003/−139**。我的探针只写进 `%TEMP%\v14-tree` 与 `%TEMP%\v14-head`。
* **没有为「改前」读数动共享工作树**：无 `git stash`、无 checkout 旧文件；改前一律 `git archive HEAD` 导出树 + 另一个 `-TargetDir`。
* **编译**全部经 `scripts/cargo-team.ps1`（`-DryRun` 先核；单飞锁；共享 target；无并发两条）；「通过」的编译面已在 §2.1 逐 target 写明。
* **真库与真进程**：pid 79984 未启停；`~/.ruagent` 未写；**未在真库上跑 wiki build**；所有构建的 root 都是探针自建（路径见输出），部分故意保留供库外复读。
* **读数三件套**：每条读数带对象集（当前树快照 / HEAD 导出树 / 我保留的临时 root / 库外 sqlite3）、采样面（**全部是我自己的夹具**：我的来源名 `engine-src`/`storage-src`/`comp-src`/`live-src`、我的 slug、我的 7 个负例族、我的时间轴）、可证伪判据（§4 的红/绿两侧）。
* **未测一律写明**（§6，7 项，含原因与 owner），**不把"取不到"写成 0**（G3/G4/G6 旧侧点名；G9 旧侧第一次没取到、补测后取得，两个状态都写在表里）。
* **同伴在途编辑**：`crates/daemon/src/distill.rs`（两次中间态：`E0282` `distill.rs:388`、`E0599` `merge_target`）按纪律只报告、不代改；它只影响我的取树时机（O-1）。
* **结论**：写侧闸门（G1 准入/G2）、G4、G5、G7、G9 与 G3 的时间窗在**我自己的夹具**上独立复现成立；**G1 的读侧 `cite_coverage` 是恒等式（F1，medium）**，G3 的第三态只在"KB 无 documents"形态出现（F2，low），`wiki_citations` 是空表（F3，low）；G8 记 `not_measured`（owner I-B/I-INT）。**不替作者修任何一处**；三条 finding 已发消息给 captain。
