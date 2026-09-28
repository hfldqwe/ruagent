# gen2-wiki-spec.md — Wiki 流水线这一代的可执行规格（R-D）

| 项 | 值 |
| --- | --- |
| 任务 | t4（requirements round 1），成员 `wiki`，attempt 1 |
| 产物 | 本文件（**只读**单：本单不改任何代码） |
| 基线时间窗 | 2026-09-27 21:28:55 – 22:47 +08:00（每条读数各自带时点） |
| 仓库点 | `0a39e5b801ea30cb8189cd6d10c4480ddfeedbfe`（工作树干净，`git status --porcelain` 空） |
| 现场对象 | 真守护进程 pid 79984（`D:\rust_cache\debug\ruagent.exe serve --addr 127.0.0.1:8787 --root C:\Users\19410\.ruagent`，启动于 2026-09-27 05:35:37 +08:00） |
| 只读纪律 | 活库一律 `file:...?mode=ro`；`/api/v1/recall` **未调用**（任务禁止：它会写 `recall_log`）；所有写操作只发生在我自建的临时 root |
| 本单验证 | 契约 verify 两条命令各跑一次：① `CARGO_TARGET_DIR=$env:TEMP\ruagent-verify-rd; cargo test -p ruagent-mock-agent --test wiki_pipeline` ⇒ **4 passed / 0 failed**（exit 0；编译 11m13s，测试 5.03s；读数取得 2026-09-27 22:46 +08:00）② `git status --porcelain -- crates panel` ⇒ **空**（22:47 复核）。另在 `$env:TEMP\ruagent-rd` 上跑过一次同源同命令（21:53：4 passed / 编译 24m21s / 测试 6.80s）。**全量 `git status --porcelain` 另显示三名同伴各自 inScope 内的 3 份规格文件（`gen2-{recall,memory,graph}-spec.md`），本单只新增 1 个文件** |
> **修订记录（t56，2026-09-28，owner 仍是 `wiki`）：三处 `degrees[…]` 索引简写改为明确措辞（只改措辞，不动任何判据/读数）**
> **为什么改（已误导过一次的实证）**：本规格把 `degrees` 的类型写对了（`:323` `pub degrees: Vec<PageDegree>`、`:331` `struct PageDegree{…}`，均为 `[i]` 可索引的数组），但 G5 的标定读数与 D.5 的判据里用了 `degrees[slug]` / `degrees[s]` 这种**像 map 访问**的简写。t51 里我写 `crates/daemon/tests/knowledge_api.rs` 的第一版断言时，正是按 `degrees[slug]` 的 map 形状写的（`links["degrees"]["deploy-pipeline"]["links_out"]`），**被测试当场打红**才改成按 `slug` 在数组里找那一行；而**集成契约 `gen2-integration-contract.md` L78 从头就是数组**（`degrees:[{slug,links_in,links_out,links_out_broken}]`）。即：契约对、本规格的简写误导了实现者 —— 持久文档里的措辞会驱动下一次实现。
> **旧文字（逐字引用）**：
> 1. `:198` —— `` `links_out` 与 `degrees[slug].links_out` 对 4 页逐位相等（含 `links_out_broken` 分开报）``
> 2. `:342` —— `` - `degrees[s].links_out == edges` 中以 s 为 src 的条数 + `links_out_broken[s]`；``
> **新文字**：`degrees` **数组中该 slug（或 s）的那一行**（`PageDegree`）。`:323`/`:331` 的类型声明**保留不动**（它们本来是对的）。
> **不改的东西**：判据、阈值、标定读数、任何 `Vec<PageDegree>` 的字段名与语义。逐条分类见 `gen2-wiki-shorthand.md`。

| 供谁读 | I-D（t10）实现、V-D（t14）独立验证、RV-D（t18）判决 |

> **修订记录（t34 修复单，2026-09-28 04:0x +08:00，owner 仍是 `wiki`）**：本规格按 RV-D 的六条 finding 做了**措辞/口径对齐**，并对**两处被裁决为必须改的接口**做了登记。**性质说明**：这不是新要求，是让规格与实现/裁决一致；行为侧只有 RV-D-1 那一处是**真修复**。
> 1. **G1（RV-D-1，代码修复）**：`cite_coverage` 的判据补上「**只能读构建期记录值**，且只在页面仍 fresh 时给出，否则 `null`；禁止从 frontmatter 重算」，并写明不变量（`cite_coverage==1.0` 不许与非空 `uncited_sections` 同响应）。D.2 的 `cite_coverage: f32` 改为 `Option<f32>`，D.2 新增 `stale_reasons`/`unknown_cause`。
> 2. **G3/D.5（RV-D-2，措辞对齐，实现未变）**：第三态的触发条件写成「**KB 整体无可用 documents row ⇒ `unknown`**；**页点名的来源消失 ⇒ `stale{SourceMissing}`**」，并把 V-D 的 `VD T8-A`/`VD T8-B` 登记为证据。
> 3. **G6（RV-D-5）**：标定读数里的 `/wiki/pages` 的 `frozen_by` 收窄为「**仅 pin 路径非空**」。
> 4. **E.2 DDL-2（RV-D-4）**：`wiki_citations` 登记为**本代未使用**、反查索引 **deferred**，并写清现状失效传播的真实路径（重扫页文件）。
> 5. **D.7（RV-D-1 的注入面）**：`WikiLead.cite_coverage` 改 `Option<f32>`（None=unknown，不许压成 0/1）、新增 `has_anchors`/`anchored_sections`，`lead_for` 签名破坏性变更并登记；G8 明确标注**原标定读数受污染、修好后必须重读**。

## 0 本代 wiki 的三句话定位（后面所有判据都从这三句推出来）

1. **页是「线索」（lead），不是「证据」（evidence）。** 注入契约已经把这条写死：`user_profile > relevant_memories > knowledge > wiki > project_context`，wiki 排在 knowledge 之后，注释原文是 “wiki is a LEAD: an agent-generated page that has to be verified against a source anyway”（`crates/memory/src/inject.rs:100-126`）。所以本代的判据不是「页写得好不好」，而是**「一个 agent 拿到这一页时，能不能在不知道它是生成物的情况下被误导」**。
2. **页的真相源是磁盘上的 md，DB 是影子。** `<root>/knowledge/wiki/<slug>.md`（frontmatter 由 daemon 序列化，D11）；`documents`/`chunks`/`chunks_fts`/LanceDB 是派生面，`Knowledge::scan` 会删掉「文件已消失」的 row（`crates/knowledge/src/files.rs:290-309`）。
3. **本代的缺口不是「不能写」，而是「写下来的东西没有任何一条能被机械校验」。** 见 A.2/D2：现场取到一个含凭空事实、且完全没有 `## 来源` 节的页面，以 `written` 落地。

---

## A 基线表

### A.0 对象集更正（任务单前提与实际不符，必须先说）

任务单写「`wiki_builds`/`wiki_pages`/`wiki_links` 计数与 status 分布」。**现场只有三张 wiki 表，没有 `wiki_pages`，没有 `wiki_links`**：

```
sqlite> select name from sqlite_master where type='table' and name like 'wiki%';
wiki_build_pages
wiki_builds
wiki_page_hashes
```

- 页清单 = 磁盘目录 + `documents` 里 `name LIKE 'wiki/%'` 的 row；**没有页表**。
- 链接图 = 每次请求从磁盘重算（`crates/daemon/src/wiki.rs:1322` `page_links` / `1353` `link_graph`），**不落库**。
- 所以「wiki_pages=4」这个现场读数（2026-09-27）指的是**页文件数**，不是表行数。

这三张表的 DDL 在 `crates/store/src/migrations/0010_wiki.sql`（37 行，`status` 的列注释写的是 `planned|running|done|failed`）。

**两个必须分开的计数口径（下文一律按此）**：`wiki 文档 = documents.name LIKE 'wiki/%'`（今天 **5** 个 = 4 页 + `wiki/index`）vs `wiki 页 = 页文件（排除 index.md）`（今天 **4** 个）。同理 chunk：**含 index 26 个**、**页 25 个**、**页所依赖的来源 4 个**（B-09/B-09b）。把「含 index」与「不含 index」两个读数并列而不说明，会看起来像同一次读数的矛盾。

### A.1 现场基线（每行五要素：对象集 / 采样面 / 复现命令 / 读数 / 时间窗）

复现命令统一定义（下文用 `%PROBE%`、`%GRAPH%`、`%CANARY%` 指代，全文脚本见附录 F）：

```powershell
$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"; $env:CARGO_TARGET_DIR=$env:TEMP\ruagent-rd
# R0 基线测试（契约 verify 命令，target 用 ruagent-verify-rd）
cargo test -p ruagent-mock-agent --test wiki_pipeline
# R1 活库只读
python $env:TEMP\ruagent-rd\wiki_probe.py            # %PROBE%
python $env:TEMP\ruagent-rd\wiki_cite_probe.py       # %CITE%
python $env:TEMP\ruagent-rd\wiki_graph_probe.py      # %GRAPH%
# R2 HTTP 只读（原样字节，不用 Invoke-RestMethod）
Invoke-WebRequest "http://127.0.0.1:8787/api/v1/knowledge/wiki/pages" | % Content
```

| id | 对象集 | 采样面 | 复现命令 | 读数 | 时间窗（+08:00） |
| --- | --- | --- | --- | --- | --- |
| B-01 | 磁盘 `~/.ruagent/knowledge/wiki/*.md`（排除 `index.md`） | 全体（4 个文件，无抽样） | `cmd /c dir /a ~/.ruagent/knowledge/wiki`；`GET /wiki/pages` | **wiki 页 = 4**：autohotkey-v2 / cooking-pasta / gardening-roses / kubernetes-troubleshooting，外加 `index.md`（670 B，build #6 生成） | 21:33 / 21:49 |
| B-02 | 顶层来源文档 `<root>/knowledge/*.md` | 全体（非递归，2 个文件） | `cmd /c dir /a /b ~/.ruagent/knowledge/*.md` | **2 篇**：`ahk-notes.md`（236 B）、`ops-handbook.md`（664 B）；`__probe__/`、`obsidian/` 是目录不计 | 21:33 |
| B-03 | `sqlite_master`（type=table，name like `wiki%`） | 全体 | `%PROBE%` | **3 张**（见 A.0）；无页表、无图表 | 21:33:17 |
| B-04 | `wiki_builds` 8 行 | 全体（无 LIMIT） | `%PROBE%`；`GET /wiki/builds?limit=100` | status：`done`×4 · `planned_only`×2（#7,#8） · `planned`×2（#1,#4，2026-09-14 的 legacy 行）；`dry_run=1`×4；**`finished_at IS NULL`×2 = 恰好是那两行 legacy** | 21:33 / 21:49 |
| B-05 | `wiki_build_pages` 41 行 | 全体 | `%PROBE%` | status：`pending` 22 · `written` 10 · `deleted` 4 · `skipped` 3 · `failed` 2。**22 个 pending 全部属于只读计划构建（#1,#4,#7,#8）**，即「永远不会执行的 pending」 | 21:33 |
| B-06 | `wiki_page_hashes` 5 行 | 全体 | `%PROBE%` | 5 行；`doctor-probe` **既无页文件也无 `documents` row**，hash 行仍在 ⇒ 泄漏 1 行；其余 4 行 sha256 前缀与 `documents.content_hash` 逐位相等 ⇒ **edited = 0/4** | 21:33 |
| B-07 | 4 页的 **4 条** source 引用（文档粒度；3 条指向同一文档 ⇒ 只有 2 个不同文档） | 全体（4 页 × sources） | `%PROBE%`（citation_probe） | **4/4 可解析**：每条 `sources` 名都在磁盘上，且 `source_hashes` 记录的 sha256 与磁盘逐位相等（`autohotkey-v2`→ahk-notes；`cooking-pasta`/`gardening-roses`/`kubernetes-troubleshooting`→**同一个** ops-handbook） | 21:33:17 |
| B-08 | 页 frontmatter / `PageMeta` / `0010_wiki.sql` 里的 chunk 级锚 | `crates/daemon/src/wiki.rs` 全文 `chunk_id` 出现次数；`0010_wiki.sql` 全文 | `Select-String -Path crates\daemon\src\wiki.rs -Pattern chunk_id` | wiki.rs 里 `chunk_id` **只出现 1 次**（`:1512`，召回 stub 把检索命中的 `h.chunk_id` **原样透传**，非页自身的记录）；`PageMeta`（`:49-60`）与 `0010_wiki.sql` 里 **0 次**；页只记「文档名 + 文档 sha256」⇒ **chunk 级可引用率 = 不可测（不是 0）** | 21:33 / 22:37 |
| B-09 | 语料单元的两种计数口径（**这一行是为了防止把定义差读成时间差**） | `documents WHERE name LIKE 'wiki/%'` × `chunks` | `%PROBE%`；同一对象在 22:34:26 复核，逐位相同 | **页文档 4 个 = 25 个 chunk**（ahk 6 · pasta 7 · roses 6 · k8s 6）；**含自动生成的 `wiki/index` 是 5 个文档 / 26 个 chunk**。`wiki_builds` 8 行、status 分布与 21:33 逐位相同 | 21:33:17 与 **22:34:26** |
| B-09b | 4 页所引**来源**语料单元（证据底座） | `documents`+`chunks`（顶层来源） | `%PROBE%`（source_chunks） | 四页的全部内容由 **4 个来源 chunk** 支撑（`ahk-notes` 1 + `ops-handbook` 3）。**与 B-09 是两个不同的对象集**：B-09 是「页作为文档」被切成几块，B-09b 是「页所依赖的来源」有几块 | 21:33 |
| B-10 | 4 页的新鲜度 | 全体 | `GET /wiki/pages` | **stale = 0/4**（原因是来源自 2026-09-15 03:02 起未变）⇒ **正例从未在真库上触发**；`stale_since` 字段不存在 | 21:49 |
| B-11 | 1 页 1 来源（临时 root 冷启动 daemon，pid 87144，127.0.0.1:8799） | 全链（PUT → dry-run → confirm → 改写来源 → 再读） | `%CANARY%` | 改写来源后 `stale` **false→true**（活体正例存在）；但 `stale_since` 不可得；`wiki/index.md` 里**不出现** `⚠️ 源已更新` 标记 | 22:29:51 |
| B-12 | 覆盖率（来源文档 → 页） | 全体（2 篇来源 / 4 页） | `%PROBE%`（coverage） | 有页的知识文档 **2/2 = 100%**；无页文档 0；每页 sources 非空 4/4 | 21:33 |
| B-13 | 链接图（4 节点） | 全体 | `GET /wiki/links` | `nodes`=4 · `edges`=6 · `broken`=5 · `orphans`=0 | 21:49:29 |
| B-14 | 4 页在「页清单」与「链接图」两面的一致性 | 全体 | `%GRAPH%` | **2/4 页两面给出不同边数**：`autohotkey-v2` 报 `links_out=3` 而图出度 0（3 条全是断链）；`cooking-pasta` 报 3 而图出度 0（1 条自链 + 2 条断链） | 22:31:34 |
| B-15 | 5 个断链（wanted）目标的归属 | 全体 | `GET /wiki/links` | `broken` 是扁平名单，**0/5 带需求方**；而需求方就在正文里（`autohotkey-v2` 要 keyboard-remapping/voice-input/windows-startup，`cooking-pasta` 要 parmesan/spaghetti） | 21:49 |
| B-16 | 无入链页 | 全体（4 页） | `%GRAPH%` + `%CANARY%` | 真库 `inbound=0` 的页 **0/4**；**活体新页 1/1 被判 orphan**——它只被自动生成的 `index.md` 链入，而 `index` 被 `page_links` 显式排除 ⇒ 「孤儿」在这套口径下等价于「没被别的页链」，不等于「不可达」 | 22:31 / 22:29:51 |
| B-17 | `crates/mock-agent/tests/wiki_pipeline.rs` | 4 个 test，全体 | 见 R0 | **4 passed / 0 failed**（测试执行 6.80s；冷 target 编译 24m21s） | 作业启动 21:28:55；读数取得 21:53 +08:00 |
| B-18 | 注入面 `<wiki>` 块的生产点 | 2 处构造点（`chat.rs:560-569`、`runs.rs:1884-1893`） | `grep -n "RetrievalHit" crates/daemon/src/{chat,runs}.rs` + `cargo test -p ruagent-memory` | `RetrievalHit{document, content, score, wiki}` ⇒ 块行是 `wiki/<slug>: <chunk 正文>`；**没有 stale / edited / hint / 覆盖率**；契约常量 `WIKI_PAGES = 2`（`inject.rs:274`） | 21:50 |
| B-19 | HTTP 三个只读端点 | 全体 | `Invoke-WebRequest … .Content`（原始字节） | links 536 B / pages 1357 B / builds 1914 B；**与只读 DB 逐字节一致**（见 A.3 的仪器教训） | 21:49:29 |
| B-20 | `wiki_builds` #8（`planned_only`） | 1 行 + 其 8 个页行 | `GET /wiki/builds/8` | 8 页计划（4 个 wanted 页 `create` + 2 个 `update` + 2 个 `keep`）自 `2026-09-27T13:17:49Z` 起**未执行**；8 个页行停在 `pending`；除这一行外没有任何状态表示「有一份已评审的计划待执行」 | 21:49 |
| B-21 | 活体全链（canary，临时 root） | 1 来源 / 1 页 / 3 次构建 | `%CANARY%` | PUT `raw/src-alpha` → 2 chunk（id 1,2）；dry-run ⇒ `{"build_id":1,"status":"planned","pages_planned":1}`（HTTP 200）；confirm ⇒ `{"build_id":2,"status":"running"}`（202）→ `done`,`pages_written=1`；页 frontmatter `sources: [src-alpha]` + 64-hex `source_hashes`，**无 chunk id**；写者回的是一个**含凭空事实且没有任何 `## 来源` 节**的正文，仍以 `written` 落地 | 22:29:50–22:29:51 |

### A.2 已知缺陷逐条复核（任务单要求「成立才写进规格」）

| id | 任务单原话（缩写） | 复核结论 | 证据（读数优先，源码坐标为辅） |
| --- | --- | --- | --- |
| **D1a** | `status='planned'` ⟺ `dry_run=1`，同一信息两份 | **成立，且比原话更严重** | B-04：同一列里同时存在 **3 个存储词汇**（`planned` legacy ×2、`planned_only` ×2、`done` ×4）；`0010_wiki.sql:9` 的列注释却是第 4 套（`planned\|running\|done\|failed`）；干跑**响应**又是第 5 个值 `"planned"`（`wiki.rs:776`）。即「这是只读计划」这一事实在场上有 4 套词表 |
| **D1b** | `confirm_plan` 按 `dry_run` 取计划而不是按 `status` | **成立（弱形态：今天行为等价，但无任何判据保证以后等价）** | `wiki.rs:789` 的 `WHERE id = ?1 AND dry_run = 1`；B-04 显示两值同真同假，所以今天等价。等价性**没有任何约束强制**（`0010` 无 CHECK、migration 无回填），t252 的注释只是声明 |
| **D1c**（新增） | — | **成立** | B-21：`POST /wiki/build {"dry_run":true,"scope":"all","confirm_plan":1}` ⇒ **200**，返回 `build_id=3`（新行！）、`status:"planned"`，`confirm_plan` 被**静默忽略**。这正是 t294 §H1/closure §7.20 的同一形状，t300 只修了查询串（`?dry_run=true` → 400），**body 里的组合还在**。对照组（好的一侧）：`{"confirm_plan":<已完成的 build 2>}` ⇒ **400** `build 2 is not a reviewable dry-run plan` ✓ |
| **D1d**（新增） | — | **成立** | B-05/B-21：只读计划构建仍写 `wiki_build_pages.status='pending'`（live 22/41 = 53.7%，canary 2/3）。`pending` 因此同时意味着「在途」和「终态、永不会执行」。canary 是新库，所以这不是历史数据问题 |
| **D2** | 无「引用可验证」门 | **成立，且比原话强（原话说「不能逐条指回 chunk」，现场是「一条都没有、也没人检查」）** | B-21 的活体：正文含「Alpha 有两个分部，**年收入 123 亿美元**」——来源 `src-alpha.md` 全文只有 “Alpha has two divisions.” 与 “Beta ships in autumn.”；且正文**完全没有** `## 来源` 节（写者 prompt 的硬规则），构建仍 `done`、`pages_written=1`、页行 `written`。stage-3 的全部校验是两行：`body.starts_with("# ")` 与 `chars ≤ 10_000`（`wiki.rs:1121-1128`） |
| **D3** | 无失效检测 / 增量刷新 | **部分成立：哈希失效信号存在且活体正例可复现；缺的是「持久化 + 动作 + 一致性」** | 存在：`stale` 由 `source_hashes` 与磁盘比对，在 **5 处**各自实现一遍（`wiki.rs:525` `wiki_state` · `1227` `regenerate_index` · `1446` `pages` · `1501` `recall_stubs` · `1602` `entity_related_pages`），B-11 已复现 false→true。缺 ①：`stale` 每次请求现算、从不落库，无 `stale_since`/`stale_sources`（`0010` 无对应列）。缺 ②：没有任何路径因为「页 stale」而重写它——`Scope::Changed` 选的是**来源**、planner 只被**告知** `[stale]`（`wiki.rs:556`）、`keep` 一律跳过。缺 ③：B-11 里 `GET /wiki/pages` 说 stale=true，而**人读的 `index.md` 没有任何标记**（`regenerate_index` 只在构建末尾算一次）。缺 ④：真库正例 0/4（B-10），即这条信号在生产上从未被观测到触发 |
| **D4** | 无纠错回路（人改不了、也记不下为什么改） | **部分成立：「冻结」有，「回路」没有** | 有：§13-3 的手改保护工作正常（`wiki.rs:1030-1039`；mock 测试断言 `手工批注` 在后续构建中存活）。缺 ①：改动**没有任何记录**——无 reason / author / 时点，`wiki_build_pages.error` 只有一句 "human-edited since last build — skipped (§13-3)"。缺 ②：被改的页从该构建起**永久 skip**（planner 被告知默认 keep），因此来源以后的所有变化都进不了它，而它仍以 `stale=false` 的身份继续出现在 wiki 召回命中里 ⇒ **一次人工修正 = 一次静默写死**。缺 ③：同一个消费面的另一半（`crates/knowledge`）**有**完整的修订回路 `edit_chunk`（`files.rs:354`）、`chunk_revisions`（`467`）、`rollback_revision`（`505`），wiki 侧没有对应物 |
| **D5** | 链接图没有质量读数（孤儿页、断链） | **不成立（读数已经有了）——但真实缺陷更窄、也更硬** | 已有：`WikiLinks{broken, orphans}`（`wiki.rs:1283`）、`links_in/links_out`（`WikiPageInfo`），B-13/B-16 现场取到 5 断链 / 0 孤儿。真缺陷四条：① **两面口径不一致**（B-14：2/4 页；成因是断链被计入 `links_out`、自链被一面算一面丢：`wiki.rs:1464` 的 `links_out = targets.len()`（含自链、含断链）vs `wiki.rs:1368-1370` 的 `link_graph`（丢自链、断链不计边））；② **断链无归属**（B-15：0/5 带需求方）⇒ 增长回路无法按需求强度排序；③ **图读数不持久**（无表、`wiki_builds` 无任何图列）⇒ 「这次构建让图变好了吗」不可答；④ `orphans` 的定义是「无出链且无入链」，B-16 活体证明它与「不可达」不是一回事（唯一入口 `index.md` 被排除） |
| **D6**（新增） | — | **成立** | 页面身份不稳定：同一主题在 `wiki_build_pages` 里走过 `rose-gardening`（build 3 `delete`，行 12）→ `gardening-roses`（build 4 `create`，行 19）。重命名 = 删除 + 重建，入链/历史/`wiki_page_hashes` 全部断掉 |
| **D7**（新增） | — | **成立** | B-06：`wiki_page_hashes` 有 5 行，`doctor-probe` 无文件、无 `documents` row。`delete` 路径会 `clear_page_hash`，但「文件在别处被删（扫描器删 row、或人手删文件）」这条路径没人清理它 |

### A.3 仪器：两个会制造假读数的坑（写下来，免得下游重踩）

1. **PowerShell 会把 ISO 时间串转成 `[datetime]` 再按本地偏移渲染。** 我第一次用 `Invoke-RestMethod | ConvertTo-Json` 取 `/wiki/builds`，legacy 行显示 `"started_at":"2026-09-15T03:26:09.629219+08:00"`，而同一时刻只读 DB 给出 `"2026-09-14T19:26:09.629219+00:00"`——**同一行两个不同字符串**，看起来像「活库在自变」（closure §7.2）。用 `Invoke-WebRequest … .Content`（原始字节）复核后二者**逐位一致**（21:49）。结论：**时间戳读数一律取原始字节**；这也解释了 B-04 里为什么 DB 全是 `+00:00` 而 API 显示过 `+08:00`。
2. **冷 target 的编译时间会被误读成测试时间。** R0 首次跑：编译 24m21s、测试 6.80s。若要计时作判据，必须分开报。

### A.4 未取得 / 不可测的读数（不许写成 0）

| 读数 | 状态 | 原因 |
| --- | --- | --- |
| 真库上的 `stale` **正例** | **不存在**（不是 0） | B-10：真库来源自 2026-09-15 03:02 起未变；正例只能在临时 root 复现（B-11） |
| 真库上的手改页（`edited`） | **从未发生**（0/4 是「没发生」，不是「没有能力」） | B-06；因此 D4 相关的比例类读数在本代只能标 `not_measured` |
| chunk 级可引用率 | **不可测**（字段不存在） | B-08。本代实现后基线才变成 0.00 |
| `/api/v1/recall` 侧读数 | **未取** | 任务禁止（写 `recall_log`） |
| 图读数的历史序列 | **不存在** | 无表（D5③） |

---

## B SOTA 对照（9 条一手来源；每条给 采纳/不采纳 + 理由 + 代价）

> 判据：本条能不能把「生成物是线索不是证据」变成**机械可判**的东西。凡不能的，一律不采纳，即使它更流行。

### B1. claim→source 对齐与引用校验/幻觉门

**来源**：[ALCE: Enabling Large Language Models to Generate Text with Citations](https://arxiv.org/abs/2305.14627)（EMNLP 2023，[ACL Anthology](https://aclanthology.org/2023.emnlp-main.398/)）——第一个「自动引用评测」基准，把引用质量拆成 **citation recall**（该陈述是否至少有一条引用）与 **citation precision**（每条引用是否真的支持该陈述），并证明这两个自动指标与人工判断强相关（recall 的判定准确率 85.1%、precision 77.x%）。

- **采纳**：citation recall 的**可判定那一半**——「每个内容节是否至少有一条锚，且锚指向活 chunk 且属于本页来源」。这直接把 D2 的活体反例变成 `failed`。
- **不采纳**：citation precision 需要**蕴含判定**（ALCE 用 NLI 模型）。理由：本仓没有 NLI 依赖，引入等于给 wiki 加一条模型腿，而 wiki 的定位是线索，加模型腿会把「生成物」变成「被另一个模型背书的生成物」——那比现在更危险。**代价**：闸门只能保证「每个陈述都指得出处」，不能保证「出处真的支持它」。因此判据里必须写清「可解析 + 对齐，不等于蕴含」，并在 D.4 留一个非门禁的 precision 读数位。

### B2. 事后归因与修订（纠错回路）

**来源**：[RARR: Researching and Revising What Language Models Say, Using Language Models](https://arxiv.org/abs/2210.08726)（ACL 2023，[ACL Anthology](https://aclanthology.org/2023.acl-long.910/)）——自动为既有输出找归因，并**事后修订**不支持的内容，同时尽量保留原文。

- **采纳**：把「修订」做成**一等状态**而不是「悄悄改写」：被修订/被冻结的页必须留下理由（对应 D4①）。我们只取它的形状，不取它的实现（它需要检索+编辑模型）。
- **不采纳**：自动修订（用它自己的编辑模型改页）。理由：本代要的是**可审计**，自动修订会让「谁改的」这条信息消失，正好与 D4 的诉求相反。**代价**：纠错仍需人来发起；我们只保证「发起之后有记录、且不静默把页写死」。

### B3. 增量与失效更新

**来源**：[Zep: A Temporal Knowledge Graph Architecture for Agent Memory](https://arxiv.org/abs/2501.13956)——对时间上矛盾的边设置 `t_invalid`（失效时刻）而不是删除，因此「这条还成立吗」是一个可查询的读数。

- **采纳**：**失效要有时刻**。`stale` 必须是「布尔 + `stale_since` + 失效原因」，且 `stale_since` 一经观测就不再改写（对应 D3①②）。
- **不采纳**：完整的 bitemporal 模型（valid_at/invalid_at 双时间轴）。理由：wiki 页的真相源是磁盘文件、没有事务历史，引入双时间轴会造出一个没有写入者的时间轴（closure §7.72：契约里没有的东西做了要单独标注）。**代价**：我们只能报「首次观测到漂移的观测时刻」，不能报「来源实际改动时刻」——来源 mtime 与扫描时刻的差就是这条读数的误差上界，必须写在字段文档里。

**来源**：[Anthropic — Introducing Contextual Retrieval](https://www.anthropic.com/engineering/contextual-retrieval)——chunk 级语境前置（每个 chunk 前置 50–100 token 的文档语境），并给出可复现的量化收益：仅 Contextual Embeddings 让 top-20 检索失败率 5.7%→3.7%（−35%），再加 Contextual BM25 →2.9%（−49%）。

- **采纳**：**锚必须落在 chunk 上**（引用单元的粒度决定可验证性）；这条与 ALCE 一起构成本代最核心的判据。
- **不采纳**：本代**不做** chunk 语境前置（它是检索侧收益，属于 I-A/I-B 的召回腿，且会改 `chunks.content` 的语义 ⇒ 会污染「逐字证据」这个性质）。**代价**：citation 的精度提升要等召回侧；本代只保证锚的存在与可解析。

### B4. 社区/主题摘要（GraphRAG 式）

**来源**：[From Local to Global: A GraphRAG Approach to Query-Focused Summarization](https://arxiv.org/abs/2404.16130)（[Microsoft Research](https://www.microsoft.com/en-us/research/publication/from-local-to-global-a-graph-rag-approach-to-query-focused-summarization/)、[graphrag 文档](https://microsoft.github.io/graphrag/)）——先建实体图、再分层社区、为每个社区生成摘要（community summary），查询时在某一层的社区摘要上做部分回答再归并。

- **不采纳（本代）**：分层社区摘要。理由有三条，都指向「现在做等于凭空造一层没有证据的生成物」：① 现场语料只有 **2 篇来源 / 4 个 chunk**（B-02/B-09），分层社区没有统计意义；② 社区摘要天然会被当作「全局结论」消费，而 wiki 的契约位置是**线索**（inject.rs 的 drop order），把无出处的全局结论塞进线索位正好放大 D2；③ 它需要图侧先有质量保证，而 `crates/graph` 本代由 I-C 负责，跨区依赖会把本代的验收绑到别人的读数上。
- **采纳（形状层面）**：**「页是覆盖一片语料的主题单元」**这一页粒度判据（一页一主题、不是一实体），本仓 planner prompt 已经是这个形状（`wiki.rs:321-343`），本代只需补上「每页的来源覆盖率」读数（B-12 已有 100%）。**代价**：不做社区摘要，就放弃了「跨文档综合」这个能力；在语料长大之后（>数百 chunk）本条应当重新评估，评估的前置读数是「页的来源覆盖率 < 100% 的文档数」。

### B5. 链接图与孤儿页治理

**来源**：[MediaWiki Manual:Pywikibot/lonelypages.py](https://www.mediawiki.org/wiki/Manual:Pywikibot/lonelypages.py)（`lonelypages.py` 标记「没有任何其他页链入」的页，即 `Special:LonelyPages` 的生产者）；[Wikipedia:Orphan](https://en.wikipedia.org/wiki/Wikipedia:Orphan)（把 orphan 定义为「没有或几乎没有其他条目链接到它」，并给出 de-orphan 的工作流与分类 backlog）。

- **采纳**：把**「不可达」与「孤立」分成两个读数**（`unreachable` = 入度为 0；`orphans` = 入出度皆 0，保持向后兼容），并要求**断链带需求方**（MediaWiki 的 `Special:WantedPages` 天然按需求计数排序，而我们今天是扁平名单，B-15）。
- **不采纳**：wiki 站的「de-orphan 工作流」（在相关条目里加链）。理由：本仓的入链由**写者**产生，而写者的链接规则目前只是 prompt 里的一句「至少 2 条」（`wiki.rs:354`）——现场却出现了「3 条链接全是断链」的页（B-14 的 autohotkey-v2）。所以先把**读数**做出来并让构建为它负责，再谈工作流。**代价**：不做工作流，孤儿页不会自动减少；本代只保证「它一出现就被点名且能被追到需求方」。

### B6. 人审与纠错回路

**来源**：[Wikipedia:Verifiability](https://en.wikipedia.org/wiki/Wikipedia:Verifiability)：「任何被质疑或可能被质疑的内容必须带引文」，且引文必须**直接支持**该内容；[Wikipedia:Citing sources](https://en.wikipedia.org/wiki/Wikipedia:Citing_sources)：引文的目的是让读者**能自己去核对**。

- **采纳**：① 「来源节必须存在且与计划来源一致」（D2 的活体页连 `## 来源` 都没有）；② **人可核对**是硬要求 ⇒ 页里必须给出**可取回的锚**（B1 的 citation recall + `GET /api/v1/knowledge/expand/{chunk_id}` 这条现成取回路径）。
- **不采纳**：「先到先得」的维基式开放编辑。理由：单用户本地系统，编辑者就是 owner；真正的风险是**静默**（改了但不知道为什么、以及改了之后被永久冻结），所以我们要的是**记录**，不是权限体系。**代价**：没有回退/仲裁机制；本代用「记录 + 可解冻」代替。

### B7. 把生成物当 lead 而非 evidence 的注入策略

**来源**：[NIST AI 600-1 (Generative AI Profile)](https://nvlpubs.nist.gov/nistpubs/ai/NIST.AI.600-1.pdf)——把 **provenance（数据来源）** 与 **synthetic content provenance / transparency** 列为风险缓解动作，要求对生成内容标注来源与合成性质。

- **采纳**：**生成物必须在被消费的地方自称生成物**。今天就断在这里：HTTP 召回面的 stub 有 `hint: "generated wiki page — verify against its sources before trusting"`（`wiki.rs:1522`），而**注入面没有**——`RetrievalHit{document,content,score,wiki}` 只有 `wiki: bool`，渲染出来的行是 `wiki/<slug>: <正文>`（B-18）。两条消费面对同一页的「我是生成物」说法不一致。
- **不采纳**：C2PA 式的密码学内容凭证（[C2PA 规范](https://c2pa.org/specifications/specifications/2.1/index.html)）。理由：本仓的信任边界是本机、单用户、同一个 DB，签名链解决的是跨组织分发问题；在这里它只会增加一层没人验的字节。**代价**：跨机器/跨 agent 分发时本代的标注可被抹掉（无签名）；若将来要把 wiki 页喂给外部 agent，这是必须先补的一条。

**来源**：[FreshLLMs: Refreshing Large Language Models with Search Engine Augmentation](https://arxiv.org/abs/2310.03214)（[Findings of ACL 2024](https://aclanthology.org/2024.findings-acl.813/)）——把「知识会过期」当成一等评测对象，构造时效性问答集并证明检索增强的时效收益有限。

- **采纳**：**新鲜度必须是可测的读数，而不是承诺**。对应 D3：`stale_since` + `freshness_lag`，并要求「读了但不知道新不新」时第三态必须存在（closure §7.78：两个值不够用时是三个 ⇒ `stale = true|false|unknown`）。
- **不采纳**：量化成「新鲜度分数」并参与排序。理由：本仓连 `stale` 都还没持久化（D3①），在没有读数之前加权重就是给一个未测量的量赋权（closure §7.51：质量读数要带各自的局限）。**代价**：本代的 `stale` 只影响「是否被重写」与「是否被标注」，不影响检索排序。

---

## C 目标表（每条 5 要素：metric / baseline / target / 复现命令 / 标定读数）

> **标定读数**的定义：I-D 落地后，用同一条复现命令必须观测到的具体数字/字符串。它不是目标，是**目标被达成的证据**。凡本代拿不到读数的（A.4），baseline 一律写 `not_measured`，不写 0。

| G | metric（度量式） | baseline（现场读数，+08:00） | target（可证伪判据） | 复现命令 | 标定读数（实现后必须出现） |
| --- | --- | --- | --- | --- | --- |
| **G1** | `cite_coverage = cited_content_sections / content_sections`（content_sections = H2 去掉 `来源`/`相关页面`/`相关记录`；cited = 该节至少一条锚解析到活 chunk 且属于本页 sources） | **0/10 = 0.00**（4 页）：B-08 字段不存在 ⇒ 锚数 0；内容节数 10（CITE probe：ahk 3 / pasta 3 / roses 2 / k8s 2）。活体另证：canary 页 0/2 | 任一 content section 无锚 ⇒ 该页 **`status='failed'`**（不是 `written`）；**读数只能是构建期记录值**：`wiki_pages.cite_coverage`（由 `report.coverage` 写入），**只在页面仍 fresh 时**报出，否则 **`null`（unknown）**；**禁止**从页 frontmatter 重算（分子分母同源 ⇒ 只能 0/1） | `%CITE%`；`cargo test -p ruagent-mock-agent --test wiki_pipeline` | `written` 页（fresh）`cite_coverage=1.0`；**锚解析不了**的页 `cite_coverage=null` 且 `uncited_sections` 非空；无 `wiki_pages` 行的页 `cite_coverage=null`（**不是 1.0**）；一次「去掉锚」的负例构建得到 `pages_failed=1` 且 `error` 含 `uncited section` |
| **G2** | 三条独立比率（拆开，因为它们成因不同） | 2a `sources_section_missing_rate`：活体 **1/1 = 1.00**（canary 页无 `## 来源`）；2b `sources_section_mismatch_rate`：现场 **0/4**（4 页的来源节与 frontmatter 一致）；2c `uncited_section_rate`：现场 **10/10**、活体 2/2 | 2a = 0.00（缺 `## 来源` ⇒ 页 failed）；2b = 0.00（来源节名单 ≠ 计划 sources ⇒ failed）；2c = 0.00（同 G1） | `%CITE%` + 负例测试 | 负例构建 3 次分别触发 3 条 `CiteProblemKind`，`wiki_build_pages.error` 逐条点名 kind |
| **G3** | `stale_persistence = (stale 页中 stale_since 非空的比例)`；`freshness_consistency = 四处消费面（/pages、/links、index.md、注入线索卡）对同一页 stale 逐位一致` | `stale_persistence`：**不可测**（字段不存在）；`freshness_consistency`：活体 **不一致 1/1**（B-11：API 说 true，`index.md` 无标记）；真库 stale = 0/4 | `stale_persistence = 1.00`；`freshness_consistency = 4/4 面一致`；**第三态的判据（t34/RV-D-2 措辞对齐，实现未变）**：**KB 整体没有可用的 documents row** ⇒ `freshness="unknown"`（`stale=null`）；而**页点名的来源消失/对不上**是一种**确定的**失效 ⇒ `stale=true` 且 `StaleReason::SourceMissing`（不是 unknown） | `%CANARY%`（改写来源后连读 4 面） | canary 上：`stale=true` 且 `stale_since` = 首次观测时刻（RFC3339 UTC，第二次构建不改写它）；`index.md` 出现 `⚠️ 源已更新`；`/pages` 与 `/links` 的 `degrees` 一致。**第三态的两个夹具（V-D 独立取得，登记为证据）**：`VD T8-A`（空 KB ⇒ `freshness="unknown"`、`stale=false`、`stale_since=null`）、`VD T8-B`（非空 KB + 页里点名的 `never-src` 无 row ⇒ `freshness="stale"`、`stale=true`） |
| **G4** | `invalidation_action_rate = 被判定 stale 的页在「下一次构建」中被 update 的比例` | `not_measured`（真库 0 个 stale 页 ⇒ 分母 0，**不能写 0/4**） | ≥ 0.90，且 **一次构建内**完成；stale 且未冻结的页不允许以 `keep` 结束 | `%CANARY%`：改写来源 → 跑一次 `scope=changed` 构建 | 该次构建里 `alpha-notes` 的 `action='update'`、`status='written'`，且重建后 `stale=false` |
| **G5** | 四条图读数 | 5a `graph_inconsistency`（pages.links_out ≠ 图出度的页数）：**2/4**；5b `wanted_attribution`（带需求方的断链比例）：**0/5**；5c `graph_readings_persisted`：**0**（无表）；5d `self_link_visibility`：现场 1 条自链被静默丢弃 | 5a = 0；5b = 1.00；5c = 每个 `done` 构建 1 行；5d 自链必须出现在 `self_links` | `%GRAPH%`；`GET /wiki/links` | **逐页比较**：`pages` 里该页的 `links_out`，与 `degrees` **数组中该 slug 的那一行**（`PageDegree`，类型见 D 节的 `Vec<PageDegree>`）的 `links_out` 对 4 页逐位相等（含 `links_out_broken` 分开报）；`wanted[0] = {slug:"keyboard-remapping", demanders:["autohotkey-v2"], demand_count:1}` |
| **G6** | `frozen_page_rate = 没有任何纠错记录的冻结页 / 冻结页` | `not_measured`（真库 edited=0/4、无冻结页） | = 0.00：任何被 skip 的页（§13-3 或 pin）都必须有一条 `wiki_corrections` 记录，`reason` 非空 | `%CANARY%`：手改页 → 再构建 → `GET /wiki/pages` | 该页 `status='skipped'` 且 `error` 含 reason；`wiki_corrections` 有 1 行（author/reason/at 齐全）。**标定口径按 RV-D-5 收窄**：`/wiki/pages` 的 `frozen_by` **仅 pin 路径非空**；手改冻结那条路上 `frozen_by=null`，但同一响应里 `edited=true` + `freshness="stale"` + 行 `error` 与 `wiki_corrections` 都带原因（面板据此解释，不靠 `frozen_by`） |
| **G7** | 三条表示/行为读数 | 7a `plan_only_vocabularies`（表达「这是只读计划」的独立存储字段数）：**3**（`status='planned'`、`status='planned_only'`、`dry_run=1`）+ 1 个响应词汇；7b `silently_ignored_body_combination`：活体 **1**（B-21 D1c）；7c `conflated_pending_rows`：**22/41 = 53.7%**（canary 2/3） | 7a = 1；7b = 0（必须 400 且错误信息点名两个字段）；7c = 0 | `%PROBE%` + `%CANARY%` | `select status,dry_run,count(*) from wiki_builds group by 1,2` 只剩一种 plan-only 表示；`{"dry_run":true,"confirm_plan":N}` ⇒ 400；只读计划不写 `pending` 页行 |
| **G8** | `wiki_block_lead_marks`（注入的 wiki 块里带 stale/edited/cite_coverage/hint 的比例，分母 = 实际注入的 wiki 块数上限 `WIKI_PAGES=2`） | **0/2**（B-18：块行是 `wiki/<slug>: <chunk 正文>`，无任何标记） | 2/2；且 `stale=unknown` 时不得省略该字段。**`cite_coverage` 的三态（t34/RV-D-1）**：`number` 只在页面仍 fresh 时给出，否则必须是 `unknown`（**不许**被渲染成 `0.0`/`1.0`，也不许省略） | `cargo test -p ruagent-memory`；`cargo test -p ruagent-daemon` | `<wiki>` 块每行含 `stale=`/`coverage=`/`anchors=`/`hint`，且 `crates/memory` 里有一条「空的 lead 不产生块」的负例（对应 inject.rs 现有「无命中不产生占位块」的纪律）。**⚠️ 本条原标定读数受 RV-D-1 污染**（`WikiLead.cite_coverage` 曾是构造性常量 1.0），修好后必须重读 |
| **G9** | `slug_identity_churn`（同一主题被「删 + 重建」的次数） | **1**（`rose-gardening` build 3 delete → `gardening-roses` build 4 create，`wiki_build_pages` 行 12/19） | 0：重命名必须走 `update` + `aliases` 追加，`delete` 仍只对「来源全消失」生效 | `%PROBE%`（wiki_build_pages_rows） | plan 里旧 slug 出现在目标页的 `aliases`；`deleted` 行数在一次 rename 构建中 = 0 |

**C 节的总门（RV-D 判决时按这条汇总）**：G1 ∧ G2 ∧ G3 ∧ G7 为**硬门**（任一不达标即 `needs_revision`）；G4/G5/G6/G8/G9 为**读数门**（必须给出改前→改后读数，未取到的写 `not_measured` 并说明原因）。

---

## D 冻结接口（页面 / 链接 / 可引用性的确切类型与签名；以及哪些进注入契约）

原则三条：
1. **frontmatter 只由 daemon 写**（D11 已有纪律，本代加强：新增字段一律 daemon 序列化，agent 只能产出正文里的锚注释）。
2. **stale / 覆盖率 / 图度数各只有一个计算点**（一条函数），四个消费面调它。今天 `stale` 有 **5 处**独立实现（`wiki_state:525` / `regenerate_index:1227` / `pages:1446` / `recall_stubs:1501` / `entity_related_pages:1602`），`links_out` 有 2 处（`pages:1464` 与 `link_graph:1371`）。
3. **不变量必须能被机器强制**（CHECK / 校验函数 / 测试），不能只写在注释里（`0010_wiki.sql:9` 的注释就是反例）。

### D.1 页面 frontmatter（`wiki/frontmatter::PageMeta`，新增 2 个块）

```rust
// crates/daemon/src/wiki.rs :: mod frontmatter
pub struct PageMeta {
    // —— 现有字段全部保留（序列化顺序不变，向后兼容已有 4 个页文件）——
    pub title: String, pub summary: String,
    pub aliases: Vec<String>, pub entities: Vec<String>,
    pub sources: Vec<String>, pub source_hashes: Vec<(String, String)>,
    pub status: String, pub generated_at: String, pub generator: String, pub build: i64,
    // —— 新增 ——
    /// 可引用性裁定（daemon 序列化；`verified` 是**门禁结果**，不是自述）
    pub verified: VerifyState,
    /// 逐条锚：section → (document, chunk_id, chunk_hash)
    pub citations: Vec<Citation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VerifyState { #[default] Unverified, Verified, Failed }

impl std::fmt::Display for VerifyState { /* "unverified" | "verified" | "failed" */ }
```

序列化（canonical，daemon 唯一写入者）：

```yaml
verified: verified
citations:
  - section: 改键:XButton2 → F6
    document: ahk-notes
    chunk_id: 15
    chunk_hash: 1f84ac51…（该 chunk 正文的 sha256）
```

解析容错沿用现有 `frontmatter::parse` 风格：未知键跳过；`citations` 块里格式不合法的条目**跳过并让 `verified` 失效**（不 panic、不静默算作合法锚）。

**不变量（新增）**：`status='generated'` 且 `verified='verified'` 的页里，`citations` 覆盖的 section 集合必须 ⊇ 正文的全部 content section 集合。I-D 在写完页后自己断言一次（写入路径），V-D 在磁盘上再断一次（独立读路径）。

### D.2 页面清单（`WikiPageInfo`）— 现有字段语义不变，新增 12 项（含三态的 `freshness`）

```rust
#[derive(Debug, serde::Serialize)]
pub struct WikiPageInfo {
    pub slug: String, pub title: String, pub summary: String,
    pub aliases: Vec<String>, pub entities: Vec<String>, pub sources: Vec<String>,
    pub stale: bool, pub edited: bool, pub links_out: usize, pub links_in: usize,
    // —— 新增 ——
    /// 锚数（chunk 级）；0 意味着这一页目前只能整页读，不能逐条核对
    pub citations: usize,
    /// 构建期记录的内容节覆盖率（`cited_sections / content_sections`，
    /// 由 `verify_page` 的 `report.coverage` 写进 `wiki_pages.cite_coverage`）。
    ///
    /// **三态（t34/RV-D-1 修订）**：`Some(v)` 只在「有构建记录 且 该页现在仍
    /// fresh」时出现；`None` = unknown（从没构建过，或记录值已不再适用）。
    /// **禁止**从页 frontmatter 重算 —— 分子与分母同源时它只能是 0.0/1.0，
    /// 那会让「锚根本不解析」的页报满分。不变量：**`cite_coverage == 1.0`
    /// 不许与非空 `uncited_sections` 出现在同一响应里**。
    pub cite_coverage: Option<f32>,
    pub uncited_sections: Vec<String>,
    /// 首次观测到漂移的观测时刻（RFC3339 UTC）；未 stale 时 None
    pub stale_since: Option<String>,
    /// **三态的字符串面**：`"fresh" | "stale" | "unknown"`。
    /// 保留 `stale: bool` 是为了不破坏已有消费者（CLI/panel），但不变量是
    /// `stale == (freshness == "stale")`，且 `freshness` **不许省略** ——
    /// 这样「不可判定」不会被一个 `false` 掩盖（closure §7.100）。
    pub freshness: String,
    /// 具体哪些来源漂移/消失（名字，排序去重）。**只有页面记录了
    /// `source_hashes` 时才可能有名字**；没有记录时是空数组（"没有名字可填"），
    /// 此时「为什么」由下面的 `stale_reasons` 回答（RV-D-3）
    pub stale_sources: Vec<String>,
    /// 失效原因词（`source hash drift` / `source missing` / `chunk missing` /
    /// `chunk hash drift` / `hand edited`）；stale 时非空，非 stale 时为空
    pub stale_reasons: Vec<String>,
    /// 仅 `freshness == "unknown"` 时非空：KB 为什么答不了
    pub unknown_cause: Option<String>,
    /// 该页最近一次成功构建的时刻（来自 wiki_pages.built_at；不可得时 None）
    pub built_at: Option<String>,
    /// 出链里落在「wanted」上的条数（B-14 的不一致正是因为它被并进了 links_out）
    pub links_out_broken: usize,
    /// 自链条数（今天被静默丢弃）
    pub self_links: usize,
    /// 生效中的纠错记录 kind（"pin" 表示被冻结）；None = 无。
    /// **`frozen_by` 仅 pin 路径非空**（RV-D-5）：手改冻结那条路看
    /// `edited=true` + `freshness="stale"` + 行 `error` / `wiki_corrections`
    pub frozen_by: Option<String>,
}
```

**`links_out` 的语义收窄（唯一一处破坏性变更）**：`links_out` 从「去重后的目标数（含自链、含断链）」改为「**去重后的目标数（含断链，不含自链）**」，与 `link_graph` 一致。现场影响：`cooking-pasta` 3→2（自链转记 `self_links=1`），其余 3 页不变；`crates/daemon/tests/knowledge_api.rs:478`（`links_out == 2`）不受影响（该页无自链）。

### D.3 链接图（`WikiLinks`）— 现有键全部保留，新增 5 项

```rust
#[derive(Debug, serde::Serialize)]
pub struct WikiLinks {
    // —— 现有键保留：CLI(`cli/src/main.rs:318-337`) 与 panel 读它们 ——
    pub nodes: Vec<String>,
    pub edges: Vec<WikiEdge>,
    pub broken: Vec<String>,      // = wanted 的 slug 序列，向后兼容
    pub orphans: Vec<String>,     // 定义不变：无出链且无入链
    // —— 新增 ——
    pub wanted: Vec<WantedPage>,
    /// 入度 = 0（Wikipedia 的 orphan 语义），与 `orphans` 分开报
    pub unreachable: Vec<String>,
    /// 被 link_graph 丢掉的边，仍然可见
    pub self_links: Vec<WikiEdge>,
    /// 每个节点的度数：**`WikiPageInfo.links_out/links_in` 必须取自这里**，
    /// 不许 pages() 自己再算一遍（这就是 B-14 的根因）
    pub degrees: Vec<PageDegree>,
    pub readings_at: String,      // RFC3339 UTC，图的测量时刻（可持久化）
}

#[derive(Debug, serde::Serialize)]
pub struct WantedPage { pub slug: String, pub demanders: Vec<String>, pub demand_count: usize }

#[derive(Debug, serde::Serialize)]
pub struct PageDegree {
    pub slug: String, pub links_in: usize, pub links_out: usize, pub links_out_broken: usize,
}

/// 唯一入口（纯函数，无 I/O）：nodes/edges/wanted/self_links/degrees/orphans/unreachable 全在这里算。
pub fn link_graph(pages: &[(String, Vec<String>)]) -> LinkGraph;
pub fn links(kb: &Knowledge) -> WikiLinks;              // 签名不变，内部走 link_graph
pub async fn pages(db: &Db, kb: &Knowledge) -> Vec<WikiPageInfo>; // 签名不变，度数取自 link_graph
```

`link_graph` 的判据（可单元测试，全部是纯函数）：
- `degrees` **数组中 slug 为 s 的那一行**（`PageDegree`）满足：它自己的 `links_out == edges` 中以 s 为 src 的条数 + 它自己的 `links_out_broken`；
- `broken` 与 `wanted` 的 slug 序列**逐位相等**（一个向后兼容，一个带归属）；
- `orphans ⊆ unreachable`；
- 自链计入 `self_links`，不计入 `degrees`。

### D.4 可引用性（新增；本代最核心的接口）

```rust
/// 一条锚：陈述 → 语料单元。**粒度必须是 chunk，不是文档。**
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Citation {
    /// 被支持的 content H2 节标题（导语 = ""）
    pub section: String,
    /// `documents.name`（不含 "wiki/" 前缀；顶层来源文档用它自己的名字）
    pub document: String,
    /// `chunks.id`
    pub chunk_id: i64,
    /// 该 chunk 正文在构建时刻的 sha256（漂移检测用；见 D.5 ChunkHashDrift）
    pub chunk_hash: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CiteReport {
    pub content_sections: Vec<String>,
    pub cited_sections: Vec<String>,
    pub citations: Vec<Citation>,
    /// cited/content；content 为空 ⇒ 0.0
    pub coverage: f32,
    pub problems: Vec<CiteProblem>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CiteProblem { pub section: String, pub kind: CiteProblemKind, pub detail: String }

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum CiteProblemKind {
    UncitedSection,        // 该 content 节没有任何锚 → 页 failed（G1）
    SourceSectionMissing,  // 正文没有 `## 来源` → 页 failed（G2a）
    SourceSectionMismatch, // `## 来源` 的名单 ≠ 计划 sources → 页 failed（G2b）
    DanglingCitation,      // chunk_id 在 chunks 里不存在
    UnalignedCitation,     // chunk 属于的文档不在本页 sources 里
    UnknownChunk(String),  // 取 chunk 失败（KB 不可用）：**不是通过**，是第三态
}

/// writer 的正文契约（daemon 解析；锚写在正文里，frontmatter 由 daemon 写）：
///   每个 content H2 节末尾一行  `<!-- cite: <document>#<chunk_id> -->`
///   可多行；`## 来源` 节仍按现有格式列出文档名（向后兼容人手读）
pub fn citations_in(body: &str) -> Vec<(String /*section*/, String /*document*/, i64 /*chunk_id*/)>;

/// 唯一的校验入口（零 LLM）：把 body 的锚与「本页计划来源 + KB 现状」比对。
/// 返回 Err 只在基建失败时；**任何 CiteProblem 都由调用方转成 `status='failed'`**，
/// 绝不允许「有 problem 但仍然 written」。
pub async fn verify_page(
    kb: &Knowledge,
    slug: &str,
    planned_sources: &[String],
    body: &str,
) -> anyhow::Result<CiteReport>;

/// 从页 frontmatter 的 citations 还原（面板/CLI/注入面用；不重算）
pub fn citations_of(meta: &frontmatter::PageMeta) -> Vec<Citation>;
```

**能机械验证什么、不能验证什么（必须写进文档，否则下游会把它当蕴含）**：
- **能**：锚存在（`chunk_id` ∈ `chunks`）、锚对齐（chunk 属于本页 sources）、锚新鲜（`chunk_hash` 与当前 chunk 正文相等）、覆盖（每个 content 节至少一条）。
- **不能**：该 chunk 是否**蕴含**该节的陈述。本代不引入 NLI/LLM 判定（B1 的代价）。因此 `cite_coverage = 1.00` 的正确读法是「每条陈述都指得出处」，**不是**「每条陈述都被支持」。
- **而且（t34/RV-D-1）**：这个数字**只能来自构建期的那一次判定**（`report.coverage` → `wiki_pages.cite_coverage`），并且**只在页面仍 fresh 时**才敢报；从页 frontmatter 重算会得到一个恒等式（分子分母同源 ⇒ 只能 0.0/1.0），那正是修掉的那个缺陷。无记录 ⇒ **`null`（unknown）**，不是 1.0，也不是 0.0。

### D.5 新鲜度与失效（新增）

```rust
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PageFreshness {
    pub slug: String,
    pub built_at: Option<String>,
    /// **三态**：Some(true)=stale · Some(false)=fresh · **None=不可判定**
    ///
    /// **判据口径（t34/RV-D-2 对齐后的措辞）**：`None` 的触发条件是
    /// **KB 整体没有可用的 documents row**（`unknown_kb`，即索引里什么都没有 ⇒
    /// 任何断言都无从谈起）。**页点名的来源消失/对不上不是 unknown**，它是
    /// 一种**确定的**失效 ⇒ `Some(true)` + `SourceMissing` / `SourceHashDrift`
    /// （还有 `ChunkMissing` / `ChunkHashDrift`）。证据：V-D 的 `VD T8-A`
    /// （空 KB ⇒ unknown）与 `VD T8-B`（非空 KB + 页里点名的 `never-src` 无 row
    /// ⇒ stale）两个夹具，见 `gen2-wiki-verify.md` §5-F2。
    /// 原措辞写「KB 无该来源 row 时必须 unknown」，与下面 `StaleReason` 自己的
    /// 枚举语义冲突 ⇒ 保留实现、改措辞（captain 裁决 RV-D-2）。
    pub stale: Option<bool>,
    /// 只有 Some(true) 且已落库时才非空；刚观测到、还没落库的那一次为 None
    pub stale_since: Option<String>,
    pub reasons: Vec<StaleReason>,
    /// 漂移或消失的来源名。**只有页面记录了 `source_hashes` 时才可能有名字**；
    /// 没有记录时是空数组（"没有名字可填"）⇒ 「为什么 stale」由 `reasons` 回答
    /// （RV-D-3）
    pub stale_sources: Vec<String>,
    pub drifted_citations: Vec<i64>,   // chunk_hash 变了的 chunk_id
    /// 不可判定时的原因（"unknown_kb" / "unknown_no_meta"）；可判定时 None
    pub unknown_cause: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum StaleReason {
    SourceHashDrift,  // 来源 sha256 与 source_hashes 记录不符（今天唯一被实现的一条）
    SourceMissing,    // 来源文件消失（今天被算作 drift；本代分开报）
    ChunkMissing,     // citations 里的 chunk_id 不存在
    ChunkHashDrift,   // chunk 正文变了（≠ 整篇文档 hash 变，锚级失效）
    HandEdited,       // 页被手改（§13-3），与上面几条可并存
}

/// **唯一**新鲜度计算点。pages()/links()/regenerate_index()/lead_for() 全部调它。
/// 今天同一件事有 **5 处**独立实现（`wiki_state:525` / `regenerate_index:1227` / `pages:1446` /
/// `recall_stubs:1501` / `entity_related_pages:1602`），本代收敛成 1 处 —— 这是 G3 一致性判据的前提。
pub async fn freshness(kb: &Knowledge, slug: &str, meta: &frontmatter::PageMeta) -> PageFreshness;

/// 观测 → 落库。`stale_since` 只在**首次**观测到 stale 时写入，之后不改写。
/// 返回被写入/更新的行数（0 表示没有变化，也是合法读数）。
pub async fn record_invalidation(db: &Db, rows: &[PageFreshness]) -> u32;

/// 下一次构建必须重做的页（stale 且未冻结）。planner 的输入也必须来自它。
pub async fn invalidation_plan(kb: &Knowledge, db: &Db) -> Vec<String>;
```

**第三态（两套面各自的准确形状）**：
- **内部** `PageFreshness.stale: Option<bool>`，`None` = 不可判定，并给出 `unknown_cause`（`"unknown_kb"`：KB/`documents` row 查不到；`"unknown_no_meta"`：页没有可解析的 frontmatter）。
- **JSON 线**（`WikiPageInfo`）：`stale: bool` 保持不变（不破坏 CLI/panel），另加 `freshness: "fresh"|"stale"|"unknown"`，不变量 `stale == (freshness=="stale")`，且 `freshness` **绝不省略**。
- **CLI** 渲染 `?`（第三态），不渲染空。
- **不允许**把不可判定静默算成 `false`，也不允许省略 `freshness` 让读者只剩一个 `false`（closure §7.100：永远不可判定是第三类缺陷）。

**U-7（残余/新鲜度回答的有效期）**：wiki 面的「没有残余 / 没有变化」**只在一个扫描周期内成立**——`Knowledge::scan` 会删掉 file-backed 的 `documents` row（`files.rs:290-309`），所以影子可以比磁盘旧；而 `wiki_page_hashes` 可以比磁盘新（D7 的 `doctor-probe`）。因此本代冻结两条：① 任何 wiki 面残余/新鲜度回答必须带自己的 `read_at`；② 必须存在「重读文件」这条腿（`Knowledge::read_raw`），影子与磁盘不一致时**报不一致**，不许用其中一个覆盖另一个（closure §7.77：来源缺失是一个事实，不该被一个看起来更好的值覆盖）。

### D.6 纠错回路（新增）

```rust
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Correction {
    pub slug: String,
    pub kind: CorrectionKind,
    /// **必填**：空串一律拒绝写入（这就是「记下为什么改」的强制点）
    pub reason: String,
    /// **必填**：谁改的（人类 id / agent 名）
    pub author: String,
    /// RFC3339 UTC，由 daemon 填
    pub at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CorrectionKind {
    /// 冻结：planner 必须 keep，构建必须 skip，`frozen_by="pin"`
    Pin,
    /// 解冻：允许重建（与 Pin 配对，历史全部保留）
    Release,
    /// 只记录，不改行为
    Note,
}

/// 写入一条纠错记录；`reason`/`author` 为空 ⇒ Err（不静默接受）
pub async fn add_correction(db: &Db, c: &Correction) -> anyhow::Result<i64>;
/// 当前生效的冻结记录（最新的 Pin 之后没有 Release）
pub async fn active_correction(db: &Db, slug: &str) -> Option<Correction>;
/// 该页的全部记录，新到旧（审计面）
pub async fn corrections(db: &Db, slug: &str) -> Vec<Correction>;
```

行为判据（可测）：
- 冻结页的构建行必须是 `skipped`，且 `error` 里带 `frozen: <reason>`（今天只有一句 §13-3 的自动原因）；
- §13-3 的自动 skip（手改）**也必须**落一条 `CorrectionKind::Note`，`author="daemon"`，`reason="hand-edited since build #<n>"` ⇒ G6 得以成立；
- `Correction` 的写入端点在 `api.rs`（I-INT），**wiki.rs 只提供签名与语义**。

### D.7 注入契约：哪些字段进 `<wiki>` 块

现有事实（B-18）：`RequestHit{document, content, score, wiki}` → `knowledge_items()` 渲染成 `wiki/<slug>: <chunk 正文>`，块头 `<wiki>`（`crates/memory/src/inject.rs:300-329`；契约测试 `inject.rs:651`）。**页的自述（stale/hint）没有过桥。**

```rust
// crates/daemon/src/wiki.rs（I-D 生产）
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct WikiLead {
    pub slug: String,
    pub title: String,
    pub summary: String,
    /// true / false / None(=unknown)
    pub stale: Option<bool>,
    pub stale_since: Option<String>,
    /// true / false / None(=unknown)：同步面看不到 DB ⇒ None
    pub edited: Option<bool>,
    /// 页里记了至少一条锚。**与"锚能不能解析"无关**（t34/RV-D-1 新增，
    /// 取代原来那个由 frontmatter 重算、只能 0/1 的 `cite_coverage`）
    pub has_anchors: bool,
    /// 页记录了锚的 distinct content section 数（真值的分子，不是覆盖率）
    pub anchored_sections: usize,
    /// 构建期记录值，**只在页面仍 fresh 时**给出；`None` = unknown。
    /// **不许**渲染成 0.0/1.0，也不许省略（G8 的三态要求）
    pub cite_coverage: Option<f32>,
    /// ≤3 条锚，让 agent 能自己取证据（`GET /api/v1/knowledge/expand/{chunk_id}`）
    pub anchors: Vec<Citation>,
    pub hint: &'static str, // "generated wiki page — verify against its sources before trusting"
}
/// daemon 侧唯一生产点；I-INT 在 chat.rs/runs.rs 调用。
/// **签名在 t34 修订**（破坏性，已登记）：async + 第二参由
/// `recorded_hash: Option<&str>` 改为 `&RecordedPage`（`wiki::page_record(&db,slug)`
/// 一行取；覆盖率是 DB 读数，必须从同一条记录里来）。
pub async fn lead_for(kb: &Knowledge, record: &RecordedPage, slug: &str) -> Option<WikiLead>;
/// 同步面（HTTP 召回 stub / 实体软链）：看不到 DB ⇒ `edited`/`stale_since`/
/// `cite_coverage` 都是 None（unknown），并给出 `has_anchors`/`anchored_sections`
pub fn lead_from(kb: &Knowledge, slug: &str) -> Option<WikiLead>;

// crates/memory/src/inject.rs（I-B 拥有；此处只是**请求**，不是 I-D 的改动）
pub struct RetrievalHit {
    pub document: String, pub content: String, pub score: f32, pub wiki: bool,
    /// NEW：wiki 命中才 Some。纯数据，不依赖 daemon 类型（避免反向依赖）
    pub lead: Option<WikiLeadMeta>,
}
pub struct WikiLeadMeta {
    pub slug: String, pub stale: Option<bool>, pub stale_since: Option<String>,
    pub edited: Option<bool>,
    /// t34/RV-D-1：由 daemon 的 `Option<f32>` 透传，**None=unknown**，
    /// 不许被压成 0.0/1.0
    pub cite_coverage: Option<f32>,
    pub has_anchors: bool, pub anchors: Vec<(String, i64)>,
    pub hint: String,
}
```

| 字段 | 进注入契约？ | 理由 |
| --- | --- | --- |
| `slug` / `title` | **进**（已有） | 身份；agent 要能引用它 |
| `summary` | **进** | 一行主题，成本极低，是「线索」的最小形态 |
| `stale` / `stale_since` | **进（今天没有）** | 把生成物当线索的**唯一可执行形式**：不告诉它「这条可能旧了」，就是把它当证据（B7/NIST） |
| `edited` | **进** | 手改页与自动页的可信度不同（人手改过的页往往正是纠正过的地方） |
| `cite_coverage` | **进** | 告诉 agent「这一页有多少节指得出处」，决定它该不该靠这一页。**三态**：只在页面仍 fresh 时是数字，否则 `unknown`（RV-D-1：原形是构造性常量，会让「锚不解析」的页报 100%） |
| `has_anchors` | **进** | 页级事实：页里记了锚（**不等于**锚解析得了）；`unknown` 的覆盖率旁边需要一个可用的正向信号 |
| `anchors`（≤3） | **进** | 让 agent 自己去 `expand(chunk_id)` 取证据 —— 这是「不产生幻觉证据」的落地机制 |
| `hint` | **进** | 已有的 `hint` 字符串必须在**两条消费面**上说同一句话（今天只在 HTTP 面有） |
| 页正文（原文 chunk） | **进，但必须带标记** | 不给正文则 clue 无用；给正文而不带标记 = 现在这种状态。**正文必须与上面的标记同块** |
| `sources`（仅文档名） | **不进** | 不可解析到 chunk 的出处对 agent 无用，反而制造「有据可查」的错觉；要用就用 `anchors` |
| `⚠️ 待证实` 标记 | **不进**（不进契约，**进读数**） | 它是写者的自述，不是事实（closure §7.11 的同族：自述必须跟随能力）。本代把它计数成 `wiki_pages.uncertain_markers`（现场：4 页共 6 处），作为一个**可被读到的**读数，而不是被当成证据 |
| entities / aliases | **不进** | 属图侧（I-C）与检索侧（I-A）；wiki 块保持窄 |

**渲染形状（冻结，I-B+I-INT 实现）**：

```
<wiki>
wiki/kubernetes-troubleshooting: Kubernetes 故障排查 — stale=false coverage=1.00 anchors=2 edited=false
  generated wiki page — verify against its sources before trusting
  [Pod 反复崩溃] 当 Pod 出现 crash-loop 时 … (cite: ops-handbook#18)
</wiki>
```

判据：**每一行都必须能在不看正文的情况下自证**（`stale`/`coverage`/`anchors`/`hint` 都在行内）；空 lead 不产生块（沿用「无命中不产生占位块」的既有纪律）。

### D.8 冻结的不变量（可机器强制，逐条给出强制点）

| 不变量 | 强制点 | 今天的状态 |
| --- | --- | --- |
| `plan-only ⟺ 单一存储表示` | SQLite CHECK + 迁移回填 | 无（3 个词汇，B-04） |
| `dry_run` 与 `confirm_plan` 互斥 | `wiki::start_build` 参数校验（400，点名两字段） | 无（D1c 静默忽略） |
| `status='written' ⇒ verified='verified' 且 coverage=1.00` | `run_page` 的 stage-3（写前）；V-D 在磁盘上再断一次 | 无（D2） |
| 计划来源、页 `sources`、正文 `## 来源` 名单 三者逐位相等 | `verify_page` | 部分（今天只比 frontmatter） |
| `stale=true ⇒ stale_since.is_some()`（落库之后） | `record_invalidation` | 无（无字段） |
| 正文 `## 来源` 里的每个文档名都必须出现在 frontmatter `sources` | `verify_page`（`SourceSectionMismatch`） | 无 |
| `frozen 页必有纠错记录` | `add_correction` 的必填字段 + G6 读数 | 无（D4①） |
| `links_out == degrees.links_out`（同一张图一个算法） | 单元测试 + `pages()` 只读 `link_graph` | 无（2/4 页不一致，B-14） |

---

## E 实现清单（优先级 + inScope + 所需 schema 变更）

### E.1 请求清单

优先级：**P0** = G1/G2/G3/G7（硬门的必要条件）；**P1** = G4/G5/G6；**P2** = G8/G9。

| # | 目标 | inScope（I-D 可写路径） | 依赖 | 需要的 schema 变更（**请求 I-SCHEMA，不由 I-D 写**） | 验收读数（改前 → 改后） |
| --- | --- | --- | --- | --- | --- |
| **E1** | G1+G2：引用锚 + `## 来源` 闸门（写者 prompt、`citations_in`、`verify_page`、frontmatter `citations`/`verified`） | `crates/daemon/src/wiki.rs`、`crates/mock-agent/tests/wiki_pipeline.rs` | 无（只用 `Knowledge::document_chunks`，见 E.3） | `0019_wiki_citations.sql`（见 E.2 DDL-1/2） | `cite_coverage` 0.00（0/10）→ 1.00（10/10，**fresh 页**）；锚解析不了或无 `wiki_pages` 行 ⇒ `null`(unknown)（t34/RV-D-1）；负例构建 `pages_failed=1` 且 `error` 含 `uncited section` |
| **E2** | G2a/2b：`## 来源` 必填 + 与计划来源逐位一致（活体反例正是缺这一节） | 同上 | E1 | 无新增 | `pages_failed` 从 0 → 1（缺来源节）；`sources_section_mismatch_rate` 0/4 → 0/4 保持并加负例 |
| **E3** | G7a/7b/7c：干跑单源 + 组合拒绝 + 不再写 `pending` 页行 | `crates/daemon/src/wiki.rs`（`DRY_RUN_STATUS`、`start_build_inner`、`insert_build_pages`、`BuildStarted.status`） | 无 | `0019` 的 DDL-3/4（回填 legacy + CHECK；**路线 B 不删列**） | plan-only 存储词汇 3→**2**（`planned_only` 回填为 `planned`，`dry_run` 保留但被 CHECK 钉死；路线 A 再降到 1）；`{"dry_run":true,"confirm_plan":N}` 200/新行 → **400**；`conflated_pending_rows` 22/41 → 0 |
| **E4** | G3：新鲜度单点 + 持久化 + 四面一致（含 `index.md` 标记） | `crates/daemon/src/wiki.rs` | E1（`ChunkHashDrift` 需要锚） | `0019` 的 DDL-5（`wiki_pages` 派生表，含 `stale_since`/`stale_sources`） | 真库 stale 0/4（信号从未正例）→ canary `stale=true` 且 `stale_since` 非空、`index.md` 出 `⚠️ 源已更新` |
| **E5** | G5：图单源（`link_graph` 唯一入口）+ `wanted` 归属 + `self_links` + `unreachable` + `degrees` | `crates/daemon/src/wiki.rs` | E4（`degrees` 与 freshness 共用一次磁盘遍历） | `0019` 的 DDL-6（`wiki_graph_readings`） | `graph_inconsistency` 2/4 → 0/4；`wanted_attribution` 0/5 → 5/5；每个 `done` 构建 1 行图读数 |
| **E6** | G6：纠错回路（`Correction`、`add_correction`、§13-3 自动 Note、冻结页 `frozen_by`） | `crates/daemon/src/wiki.rs` | E1/E4 | `0019` 的 DDL-7（`wiki_corrections`） | `frozen_page_rate` `not_measured` → 0.00（canary：手改页 skip 且落 1 条 `Note` 记录） |
| **E7** | G4：失效传播（`invalidation_plan` → planner 必须 `update`） | `crates/daemon/src/wiki.rs` | E4 | 无新增 | canary：改写来源后一次 `changed` 构建把该页 `action=update`、`written`、重建后 `stale=false` |
| **E8** | G9：页面身份（rename 走 `update`+`aliases`，delete 只对来源全消失） | `crates/daemon/src/wiki.rs` | E1 | 无新增 | `slug_identity_churn` 1 → 0（`deleted` 行在一次 rename 构建中 = 0） |
| **E9** | G8：注入线索卡（`WikiLead`/`lead_for`） | `crates/daemon/src/wiki.rs`（生产）；**消费端不在 I-D** | E1/E4 | 无 | `wiki_block_lead_marks` 0/2 → 2/2（由 I-B/I-INT 完成后测得） |
| **E10** | D7：`wiki_page_hashes` 对账（无文件的 hash 行清理；`doctor-probe` 是现场反例） | `crates/daemon/src/wiki.rs` | 无 | 无（对账逻辑在产品路径里，不在迁移里 —— closure §7.75） | `wiki_page_hashes` 5 行 → 4 行（活库下一次构建后）；读数为「泄漏行 1 → 0」 |
| **E11** | **U-7（mem-core 留给我方的开放项）**：残余/新鲜度的回答不许只信 DB 影子，必须带自己的有效期并给「重读文件」腿 | `crates/daemon/src/wiki.rs` | E4 | 无 | `residual_shadow_mismatch`（影子与磁盘不一致的对象数）：现场 **1**（`doctor-probe` 的 hash 行 = 影子比磁盘多）→ 0；且任何 wiki 面残余回答都带 `read_at` 与 `shadow_lag`（影子与磁盘 mtime/内容的差），不一致时**报不一致**而不是报一个看起来更好的值（closure §7.77） |

**I-D 的 inScope 合计（只有两条路径）**：
```
crates/daemon/src/wiki.rs
crates/mock-agent/tests/wiki_pipeline.rs
```
`crates/daemon/tests/knowledge_api.rs` **不在** I-D 的 inScope：它用**逐字段断言**钉住了今天的形状（`knowledge_api.rs:478-506`：`links_out==2`、`broken==["k8s"]`、`orphans==["orphan-page"]`）。D.2/D.3 落地后它必须被更新，**由 I-INT（或 captain 改派）修改**；I-D 不得碰它，否则一个文件两个写者。I-D 自己的端点回归放在 `wiki_pipeline.rs`（那条路径完全在 inScope 内）。

### E.2 请求 I-SCHEMA 的 schema 变更（`crates/store/src/migrations/0019_wiki_gen2.sql`）

> 现最高版本 = **0018**（`schema_migrations` 只读读数：`max(version)=18`）。以下 DDL 是**给 I-SCHEMA 的请求**，I-D 不写这些文件。

```sql
-- DDL-1 页的派生状态（可重建；磁盘仍是真相源）
CREATE TABLE wiki_pages (
  slug            TEXT PRIMARY KEY,
  title           TEXT NOT NULL DEFAULT '',
  summary         TEXT NOT NULL DEFAULT '',
  sources_json    TEXT NOT NULL DEFAULT '[]',   -- 引用到的来源文档名
  citations_json  TEXT NOT NULL DEFAULT '[]',   -- D.4 的 Citation 序列
  sections        INTEGER NOT NULL DEFAULT 0,
  cited_sections  INTEGER NOT NULL DEFAULT 0,
  cite_coverage   REAL    NOT NULL DEFAULT 0.0,
  -- ⚠️ 读回时（t34/RV-D-1）：**只有 `build_id IS NOT NULL` 的行才算「有记录的
  --    读数」**。只用 staleness 那一遍创建的行走 INSERT 时根本不带覆盖率，
  --    这个 DEFAULT 0.0 是**从没测过**而不是「测出来是 0」—— 读成 0 就是把
  --    "不知道" 压成一个值（本代同一族缺陷）。无记录 ⇒ 报 `null`(unknown)。
  verified        TEXT    NOT NULL DEFAULT 'unverified', -- unverified|verified|failed
  built_at        TEXT,                          -- 最近一次成功构建时刻
  build_id        INTEGER,
  stale           INTEGER NOT NULL DEFAULT 0,
  stale_since     TEXT,                          -- 首次观测到漂移的观测时刻，只写一次
  stale_sources_json TEXT NOT NULL DEFAULT '[]',
  edited          INTEGER NOT NULL DEFAULT 0,
  links_in        INTEGER NOT NULL DEFAULT 0,
  links_out       INTEGER NOT NULL DEFAULT 0,
  links_out_broken INTEGER NOT NULL DEFAULT 0,
  self_links      INTEGER NOT NULL DEFAULT 0,
  uncertain_markers INTEGER NOT NULL DEFAULT 0,   -- 正文里 `⚠️ 待证实` 的计数（写者自述 → 只作读数）
  content_hash    TEXT,                          -- 与 wiki_page_hashes 同源
  updated_at      TEXT NOT NULL
);

-- DDL-2 锚（可按下游 chunk_id 反查受影响页 —— 失效传播的索引面）
--
-- ⚠️ 本代**未使用**（I-D 侧登记，RV-D-4 / V-D F3）：全树没有任何 .rs 读写它，
--    三个 root 里都是 0 行（V-D 用 crate 外 sqlite3 只读查过）。现状的锚存放在
--    `wiki_pages.citations_json` + 页 frontmatter 的 `citations:` 块列表里，
--    **失效传播的真实路径是「重新扫描页文件」**（`freshness_all` 读页目录 +
--    `uncited_sections_of` 重读正文跑 `verify_page`），**不是**这张反查表。
--    因此按下游 chunk_id 反查受影响页的能力 **deferred**：等真有一个消费者
--    （例如「来源变更 → 一次只重建受影响页」的增量路径）再在其中写入并加索引。
--    **有意不写**：为"别浪费表"在 `record_page_row` 里顺手写它，会造出一条
--    **没有消费者**的写路径 —— 那是本代一直拒绝的形状。
CREATE TABLE wiki_citations (
  page_slug  TEXT NOT NULL,
  section    TEXT NOT NULL,
  document   TEXT NOT NULL,
  chunk_id   INTEGER NOT NULL,
  chunk_hash TEXT NOT NULL,
  build_id   INTEGER,
  PRIMARY KEY (page_slug, section, chunk_id)
);
-- 两条反查索引也随 DDL-2 一起 **deferred**（本代没有写入者，索引就没有意义；
-- 现场 rebuild 计数：`wiki_citations` 三个 root 全 0 行）
CREATE INDEX idx_wiki_citations_chunk ON wiki_citations(chunk_id);
CREATE INDEX idx_wiki_citations_doc   ON wiki_citations(document);

-- DDL-3 回填 legacy 词汇（现场 2 行：id=1,4）
UPDATE wiki_builds SET status='planned'
 WHERE dry_run=1 AND status='planned_only';
UPDATE wiki_builds SET finished_at=COALESCE(finished_at, started_at)
 WHERE dry_run=1 AND finished_at IS NULL;

-- DDL-4 把不变量写进表（SQLite 需重建表；与 DDL-3 同一次迁移内完成）
--   status 取值域收窄为 planned|running|done|failed
--   CHECK ((status='planned') = (dry_run=1))
--   CHECK (status='planned' ⇒ finished_at IS NOT NULL)
--   然后 DROP COLUMN dry_run（SQLite ≥3.35）—— 或者保留列、只加两个 CHECK，
--   两条路线的代价见下。

-- DDL-5 只读计划的页行（今天写 pending，永远不会执行）
--   路线 A：dry-run 不写 wiki_build_pages（计划已在 plan_json）
--   路线 B：写，但 status='planned'（与执行态分开）
--   现场 22 行 pending 的清理由产品路径完成，不在迁移里

-- DDL-6 每次构建的图读数（G5c：让「这次构建让图变好了吗」可答）
CREATE TABLE wiki_graph_readings (
  build_id    INTEGER PRIMARY KEY REFERENCES wiki_builds(id),
  nodes       INTEGER NOT NULL, edges INTEGER NOT NULL,
  broken      INTEGER NOT NULL, orphans INTEGER NOT NULL,
  unreachable INTEGER NOT NULL, self_links INTEGER NOT NULL,
  read_at     TEXT NOT NULL
);

-- DDL-7 纠错回路（D.6）
CREATE TABLE wiki_corrections (
  id     INTEGER PRIMARY KEY,
  slug   TEXT NOT NULL,
  kind   TEXT NOT NULL CHECK (kind IN ('pin','release','note')),
  reason TEXT NOT NULL CHECK (length(trim(reason))>0),   -- 「记下为什么改」的强制点
  author TEXT NOT NULL CHECK (length(trim(author))>0),
  at     TEXT NOT NULL
);
CREATE INDEX idx_wiki_corrections_slug ON wiki_corrections(slug, at);
```

**DDL-4 的两条路线与代价（请 I-SCHEMA/RV-D 判）**：
- **路线 A（完整单源）**：删 `dry_run` 列，只留 `status='planned'`，响应里给派生布尔 `plan_only`。代价：panel（`WikiBuild.dry_run`，`panel/src/api.ts:437`、`Wiki.tsx:244`）与 `api.rs` 的两处 json 必须同步改 → 落到 I-INT；`wiki_builds` 表重建（SQLite 需 `ALTER TABLE … RENAME` + 建新表 + 拷数据），在一个已有 8 行的活动表上需要一次**可复现的自检**。
- **路线 B（最小可行）**：保留 `dry_run` 列，只加两个 CHECK + 回填 legacy。代价：`dry_run` 与 `status='planned'` 仍互为函数依赖（同一事实两份），但**不再可能不一致**（CHECK 强制）。G7a 的读数从 3 降到 2（不是 1）。
- **本规格的裁决**：**先做路线 B**（它一次迁移闭合 D1a 的可观测缺陷，零消费者改动），把路线 A 记为 P1 的后续项，并要求路线 B 的迁移里带一条自检 SQL 断言 `count(*) where dry_run=1 and status<>'planned'` = 0。

### E.3 与 I-SCHEMA / I-A / I-B / I-C 的边界（逐条证明不冲突）

| 变更 | 文件 | 所有者 | I-D 的关系 |
| --- | --- | --- | --- |
| 迁移 0019（DDL-1…7）、`Db` 仓储方法 | `crates/store/src/**` | **I-SCHEMA (t6, recall)** | I-D **只请求**，不写。I-D 的每一次写都用现成的 `Db::call/call_flat`（`wiki.rs` 已有 `run_write` 封装） |
| chunk 读取面 `Knowledge::document_chunks(id) -> Vec<(i64,String)>`（`store.rs:708`） | `crates/knowledge/src/**` | **I-A (t7, recall)** | **I-D 不需要 I-A 改任何东西**：锚的校验只需 `document_chunks`（拿 `chunk_id` + 正文）+ `documents`（用 `Knowledge::list_documents`）+ 页里的 `source_hashes`。`chunk_hash` 由 I-D 在 daemon 侧对正文算 sha256（`ruagent_knowledge::sha256_hex` 已导出，`lib.rs:36`）。**反向约束**：I-A 不要把 `chunks.content` 改成带语境前缀的文本（B3 的不采纳项），否则 `chunk_hash` 的语义与「逐字证据」一起漂移 |
| `RetrievalHit.lead: Option<WikiLeadMeta>` + `<wiki>` 块渲染（D.7） | `crates/memory/src/inject.rs` | **I-B (t8, mem-core)** | I-D 提供**数据**（`WikiLead`/`lead_for`），I-B 提供**契约字段**。I-D 不改 memory；I-B 不改 wiki.rs。类型形状已在 D.7 冻结（纯数据，不反向依赖 daemon） |
| `chat.rs`/`runs.rs` 的 `RetrievalHit` 构造点、`api.rs` 的端点装配、`crates/daemon/tests/knowledge_api.rs` | `crates/daemon/src/{chat,runs,api}.rs`、`crates/daemon/tests/knowledge_api.rs` | **I-INT (t19, integ)** | I-D 不碰。需要 I-INT 吃下的三件事：① 两个构造点填 `lead`；② `/wiki/pages`、`/wiki/links` 的新字段直接透传（`wiki.rs` 已序列化好）；③ `knowledge_api.rs` 的逐字段断言按 D.2/D.3 更新 |
| 纠错的**写**端点（`POST /api/v1/knowledge/wiki/pages/{slug}/corrections`） | `crates/daemon/src/api.rs` | **I-INT** | I-D 只给 `add_correction` 签名与语义 |
| 实体↔wiki 软链（`entity_related_pages`） | `crates/daemon/src/wiki.rs` | **I-D**（函数在 wiki.rs）；调用点在 `api.rs` | I-D 若给它加 `cite_coverage`，**不需要 crates/graph 任何改动**（graph 名字由调用者传入）。**I-C (t9) 与本单无接口** |
| panel 四页（Wiki 视图读 `dry_run`/`status`/`orphans`/`broken`） | `panel/**` | **I-INT** | 新增字段是**追加**，panel 不改也能跑；路线 A 才需要改 `panel/src/api.ts:437` + `Wiki.tsx:244` |

**一句话**：I-D 的全部改动落在 `crates/daemon/src/wiki.rs` + `crates/mock-agent/tests/wiki_pipeline.rs` 两个文件里；它**输出**三个接口（`WikiPageInfo`/`WikiLinks` 的 JSON、`WikiLead`、`add_correction`/`freshness` 的签名），**输入**两个请求（0019 迁移、`RetrievalHit` 的字段）。四个兄弟区没有一个文件与本单重叠。

### E.4 给 V-D / RV-D 的判据骨架

1. **改前→改后**必须成对：A.1 的 B-04/B-05/B-06/B-10/B-14/B-15/B-18/B-21 就是 `before`，E.1 每行的「验收读数」是 `after`。
2. **负例必测三项**（每项都要有「坏的一侧真的红」的读数）：① 去掉一个节的锚 → 页 `failed`；② 删掉 `## 来源` → 页 `failed`；③ `{"dry_run":true,"confirm_plan":N}` → 400。只有坏侧变红而好侧仍绿，才算两侧都测到（t294/t300 的教训）。
3. **不许把不可测写成 0**：A.4 的五条在本代结束前若仍不可测，报告里必须出现 `not_measured` + 原因。
4. **单源检查**：`stale` 只允许有 1 个计算点；`links_out` 只允许来自 `link_graph`。用构造做**变更前后对比**：开工前 `Select-String -Path crates\daemon\src\wiki.rs -Pattern "let stale"` 有 **5 处**（`:525` `wiki_state`、`:1227` `regenerate_index`、`:1446` `pages`、`:1501` `recall_stubs`、`:1602` `entity_related_pages`），落地后必须 **≤1 处**（只在 `freshness()`）；`PageMeta`/页 frontmatter 里 `chunk_id` 落地前 0 处（`wiki.rs` 里那 1 处 `:1512` 是召回 stub 的透传），落地后必须出现在 `Citation` 里。
5. **活体验证只能在临时 root**：真库只读；任务禁 `/api/v1/recall`；真守护进程 pid 79984 不许启停。

---

## F 附录：本单的复现脚本

三个只读探针与一个临时 root 活体脚本，全部写在 `$env:TEMP\ruagent-rd\`（**不在仓库内**，所以 `git status --porcelain` 只有本报告文件）：

| 脚本 | 用途 | 关键读数 |
| --- | --- | --- |
| `%PROBE%` = `wiki_probe.py` | 活库只读：表存在性、builds/页行/hash 行、documents/chunks、每页引用可解析性、覆盖率、链接图 | B-03…B-09、B-12、B-16 |
| `%CITE%` = `wiki_cite_probe.py` | 活库只读：每页的 content section 数、claim 行数、`⚠️` 计数、`## 来源` 存在性 | G1/G2 的 baseline 与分母 |
| `%GRAPH%` = `wiki_graph_probe.py` | 活库只读：per-page `pages.links_out` vs 图出度、断链归属、自链、unreachable | B-14/B-15/B-16 |
| `%CANARY%` = `canary.ps1` | 临时 root 冷启动自己构建的 `ruagent.exe`（`127.0.0.1:8799`，pid 自记），全链 PUT→dry-run→confirm→改写来源→再读；**结束时只按自己记录的 pid 停进程**，并复查无同命令行残留 | B-11、B-21、D1c、D2、G3/G4/G6 的标定读数 |

`%CANARY%` 的纪律要点（照抄可用）：
```powershell
#requires -Version 7   # PS 5.1 会在 PUT/空响应上抛 NullReferenceException（本次实测）
$proc = Start-Process -FilePath $bin -ArgumentList @("serve","--addr","127.0.0.1:8799","--root",$tmpRoot) `
        -WindowStyle Hidden -PassThru -RedirectStandardOutput $out -RedirectStandardError $err
Set-Content -Path (Join-Path $tmpRoot "canary.pid") -Value $proc.Id   # 只记自己的 pid
...
finally { Stop-Process -Id $proc.Id -Force }   # 绝不按名字/端口杀
```

本单**未做**的事（照规矩列出来）：未改任何代码；未调用 `/api/v1/recall`；未对真库跑任何 wiki build（只读探测）；未启停真守护进程（pid 79984 在 21:46 仍是活的，启动时间 2026-09-27 05:35:37 +08:00）；未把写操作放到临时 root 之外。
