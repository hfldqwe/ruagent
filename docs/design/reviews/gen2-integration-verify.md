# gen2 集成独立端到端验证报告（V-INT · t20）

> 被验对象：INT（t19 及其 t45/t46/t47/t48/t50 五轮 + t51 的消费面装配）对 `gen2-integration-contract.md` 的交付。
> 本文件只写**我自己跑出来的数**；作者读数一律引用并逐条对照。
> 验证者：verify，**未改任何被验代码**（inScope 只有本文件）；**未 git stash / checkout**；**未启停真守护进程 pid 79984**、**未写活库**（活库只经 `mode=ro` 连接 + `VACUUM INTO` 读成副本）。
> 编译纪律（captain 2026-09-28 全队资源纪律）：每一次 cargo 都走 `scripts/cargo-team.ps1`（单一编译 / 共享 target / CPU 0-11 / BelowNormal）。**这是对任务单里 `CARGO_TARGET_DIR=$env:TEMP\ruagent-verify-vint` 的有意替换**，替换理由与形状同 V-A/V-C 报告。
> 计数纪律（契约 §6.0 第 17 条）：本报告所有计数都写明**工具与开关**（`Select-String -CaseSensitive` / Python `re.findall` 默认大小写敏感 / 退出码），不用模糊匹配。

## 0 一句话

**主结论：契约的接口面基本落地，且 G8（带数字 coverage）与 DEP-1 生产帧这两条历史缺口我独立复现为「已闭合」；但有 4 条契约明文项未交付、1 条冻结块无生产者、1 条端点行为与作者声称不符。**

- **G8 两侧对照我自己重做了一遍**（临时 root + 端口 8811 + 我自己的 `ruagent.exe`，pid 见 §1）：构建侧 `wiki_pages.cite_coverage = 1.0`（记录值），注入侧渲染里 **`coverage=1.00`**（大小写敏感匹配）；**负控也是我自己做的**：同一份正文只把锚点从真实 chunk `#1` 换成 `#0` ⇒ `wiki_build_pages.status='failed'` + `citation check failed (2 problems)`。**真假两侧都测到了。**
- **DEP-1 生产帧三方对照**（captain 指定补取证）：改前（活库 651 行历史遥测，旧守护进程写的）**1/23 条查询有实体**、按调用 **5/651 = 0.77%**；改后（**我的活库只读副本 + 新二进制 + 同一 23 查询帧走生产 `/recall`**）**13/23 = 56.5%**（`graph.paths>0` 10/23）；**腿级仪器（graph 自己的 `--include-ignored`，我的副本）给出同一对照 `old(strict) 1 -> new 13`**，自查询 63/63 → 63/63 无回归 ⇒ 规格 target ≥13/23 **在生产路径与腿级两条独立路径上都达标**，不再需要 `not_measured`。
- 闭环独立取证：5 个 `context_injected`，**`path` 5/5**（chat 3 / run 2）、**`budget` 0/5**；块种类 **`knowledge` 5/5、`wiki` 5/5、`graph`（实体）0/5** —— 后者不是「没测」，是**注入面根本没有 graph 块的生产者**（见 F5）。
- 未交付项（我的读数，逐条证据在 §5）：**`/recall/log` 不透出新列**（F1）· **`/facts?at=` 不归一化、非时刻静默 200**（F2）· **`prompt_hash` 未接线**（F3）· **`/stats` 没有 embedder/scoring_version**（F4）。
- 门禁：`test --workspace` **exit 0**（`test result: ok` **55** 行 / `FAILED` **0** 行，`Select-String -CaseSensitive`；`(\d+) passed` 求和 **387**）；`cd panel && npm run check / i18n:check / build` **三条 exit 0**；面板 e2e **有数据的根 = 42 passed / 1 failed / 2 skipped**，**空根 = 40 passed / 4 failed / 1 skipped**（4 条红的归因见 §3.7，全部不是面板代码缺陷）。

## 1 对象集 / 环境 / 时间窗 + 归因

| # | 对象 | 我的读数 |
| --- | --- | --- |
| S1 | **真守护进程 pid 79984 @ 127.0.0.1:8787** | 每个取证步骤前后都只读核对：`alive, StartTime=09/27/2026 05:35:37`（全程同一 StartTime）⇒ **未启停**；未调 `/api/v1/recall`（唯一经它取的是 `GET /api/v1/health` 与 `GET /api/v1/knowledge/search` 之外的**无**；本单实际上**只用我自己起的守护进程**取证） |
| S2 | **我的探针根**（closure/anchor 读数） | `%TEMP%\ruagent-vint-probe`，端口 **8811**，daemon pid **14060**（起于 00:22:17）→ 重起为 **47880**（加 alpha/beta/judge 后）；`RUAGENT_EMBEDDER=hash` |
| S3 | **我的 e2e 根**（面板 DOM） | `%TEMP%\ruagent-vint-e2e`，端口 **8813**，pid **34712**（空根，canonical mock agents） |
| S4 | **我自己的活库副本根**（DEP-1 生产帧） | `%TEMP%\ruagent-vint-live`（`VACUUM INTO` 的 `ruagent.db` + 复制的 `knowledge/`、`data/lancedb`），端口 **8812**，pid **53072** → 重起为 **82088**（必须用**真模型**：`RUAGENT_EMBEDDER=hash` 是 256 维而副本向量是 384 维 ⇒ 每条查询 `400 lancedb: … dimension: 256`，这是我第一次跑出的读数，见 §3.2） |
| S5 | 活库本体 | `~/.ruagent/data/ruagent.db` **只以 `mode=ro` 打开** + `VACUUM INTO %TEMP%\vint-live.db`（14,188,544 B）；**12 列**（未迁移）；`recall_log` 651 行、23 个不同查询、`source IS NULL` 624 行 |
| S6 | 代码字节 | 见 §7 的收尾哈希；我的读数窗口 = **2026-09-29T00:16–00:45+08:00**，工作树 HEAD `0a39e5b801`（全部 gen2 改动未提交） |

**归因**：全树 porcelain **66 项**、全部是并发波次里同伴的在途改动（store/knowledge/memory/graph/daemon/mcp/cli/panel）——**只作背景，不据此写 finding**（captain F5 纪律）。被验对象按 mtime 归因：`crates/daemon/src/api.rs` 23:55:04、`wiki.rs` 23:52:43、`tests/knowledge_api.rs` 23:59:01、`panel/src/views/Wiki.tsx` + `panel/src/api.ts` 00:00:09、`panel/e2e/consumption.spec.ts` 00:06:49 —— **全部早于我起进程的 00:17:21（我 build 出的 `ruagent.exe` mtime 00:17:21）** ⇒ §2–§5 的读数都落在同一份字节上。
**一条背景差异**：`gen2-integration-impl.md` 在我读数期间从 55,512 B（23:51:48）变成 **59,978 B（00:22:29）**；我引用的 §13–§17 是前一个版本的内容，收尾以哈希+大小为凭（§7）。

## 2 契约验收锚点逐条（命令 + 我的读数）

| # | 判据 | 我的命令（`…` = `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1`） | 我的读数 | 判 |
| --- | --- | --- | --- | --- |
| **A-1** | §1 的 HTTP 形状与破坏性变更 | `… test -p ruagent-daemon --test knowledge_api`（在 workspace 跑里：`knowledge_api` **11 passed / 0 failed**；本轮文件 23:59:01 已含 t51 的 40 条断言） | 绿；`links_out == degrees.links_out`（我另外用 HTTP 独立读到 2 == 2）、`{dry_run:true,confirm_plan:1}` ⇒ **400** + 不增行 | ✅ |
| **A-2** | 注入遥测 + `<graph>`/`<wiki>` + BREAK-CORE-1 | 我的 8811 根：chat ×3 + run ×2 → 读 `data/transcripts/*.jsonl` | **5/5 有 `path`**（chat 3 / run 2）；**0/5 有 `budget`**；块 `knowledge` 5/5、`wiki` 5/5、`graph` **0/5**；`used_chars ≤ total_chars` **无法判**（无 budget 对象） | ⚠️ 半（`path` 达标、`budget` 0/5 = 作者已登记的 not_measured，owner=mem-core） |
| **A-3a** | §3.1 已落地列真的被写 | 8811 根 + 副本根：HTTP `/recall` 后直读 `recall_log` | 新行 **23/24 列有值**（唯一 NULL：`top_memory_score`，该根没种记忆 ⇒ **不适用**）；`knowledge_leg_window=60`、`scoring_version=1`、`source='probe'`、`graph_entities/graph_paths` 与响应 `graph.stats` 一致（1/1）；**迁移前的行 624 行全部 NULL**（0 行是 0） | ✅ |
| **A-3b** | 6 列「既有列也有值」 | 同上 + `/recall/log` | **列在**（24 列）且**值在**（`score_kind='rrf_rank'`、`fusion='rrf:k=60,w_semantic=2,w_keyword=1'`、`top_legs_json` 可解析且只含命中腿、`candidates_json`/`selected_json`/`rejected_json` 有值）⇒ **写入侧达标**；但**读出侧不透出**（F1） | ✅写入 / ❌读出（F1） |
| **A-4** | MCP 表 | 我的 workspace 跑覆盖 `-p ruagent-mcp`（`roundtrip` 9 passed）；**我未独立驱动 MCP stdio**（§6 未测项） | 编译面绿；工具清单/`store:"bogus"` 的工具错误/`source='mcp_recall'` 我**未独立取证** | ⚠️ 未测（原因在 §6） |
| **A-5** | 面板类型/文案/构建 | `cd panel; npm run check; npm run i18n:check; npm run build` | **三条 exit 0**；`check` = `tsc -b --noEmit && tsc -p e2e/tsconfig.json --noEmit`；`i18n:check` 打印「keys with no string literal anywhere outside i18n.tsx: 76 (of which 40 …)」且 **exit 0**；`build` = 55 assets + `dist/index.html` | ✅ |
| **A-6** | 四页 DOM 读数（只读 e2e） | `cd panel; $env:E2E_BASE_URL=http://127.0.0.1:8811; npm run test:e2e -- --output=%TEMP%\pw-vint` | **42 passed / 1 failed / 2 skipped**；四条新 consumption 断言全绿，DOM 文本实测：`DOM READING memory recall-log row: consumption激进前 3unknownmem 1 know 2 wiki 2 ent 0记忆腿余弦 0.27名次分 0.049刚刚`；`wiki.spec.ts` 3 ✓；`knowledge.spec.ts` 1 ✓；`registry.spec.ts` **跑了并绿**（见 §3.7 的说明） | ✅（有条件，§3.7） |
| **A-7** | 全 workspace 不因本代变红 | `… test --workspace`（+ `fmt`/`clippy` 由作者与别的成员跑，我未重跑 clippy） | **exit 0**；`test result: ok` **55** 行、`FAILED` **0** 行、`panicked` **0** 行（`Select-String -CaseSensitive`）；passed 求和 **387**（作者 §17.4 记 386；差异见 §4） | ✅ |
| **A-8** | 契约自身与零代码改动 | `git status --porcelain`（我读数时 66 项，全为同伴在途）+ 只读核对 `api.rs` | 契约文件存在；**我未改任何 `crates/`/`panel/`**；`api.rs` 里能看到 t45/t46/t47/t51 的改动 | ✅（就我而言） |
| **A-9** | 端到端手测：未知 store | 8811 根 `POST /memory/write {"store":"bogus",…}` | **HTTP 400** `unknown store \`bogus\` (observation | profile | procedure | lesson)`；`episodes`/`memory_diffs` **前后不变**（0,0,0 → 0,0,0） | ✅ |
| **A-10** | 残余面入口 | 8811 根 `GET /forget-report?content_hash=<64hex>` | **200** + `{content_hash, local:[4 面], external:[…], total}`；非 64 hex ⇒ **400** | ✅ |
| **A-11** | 时序/可归因 | `GET /distill`、`PUT /distill`、`GET /stats` | **`/distill` 无 `prompt_hash`（键 = agent/auto/builtin_prompt/graph/language/prompt）；`PUT` 只回 `{"ok":true}`；`/stats` 只回 `{agents:[…]}`（无 embedder、无 scoring_version）** | ❌ **F3 + F4** |
| **A-12** | 只读取证（禁用项） | 本报告全文 | 我用的是自己的守护进程；**唯一**碰真库的是 `mode=ro` 与 `VACUUM INTO`；**未调 `/api/v1/recall`** | ✅ |
| **A-13** | DEP-INT-8 的注入接线 | 8811/8813 根 | ① 知识命中走 `relevance`（`/knowledge/search` 的 hit 同时给 `score`+`score_kind`、`relevance`+`relevance_kind`，t29 的字面量修复在线上生效：`rrf_rank`/`semantic_l2sq`/`bm25`/`calibrated`）；② wiki 命中的 marker 行实测 `wiki/deploy-pipeline: deploy-pipeline — stale=false coverage=1.00 anchors=2 edited=false` + **hint 行** `generated wiki page — verify against its sources before trusting`（= `inject.rs:397` 的 `WIKI_LEAD_HINT`）；③ 未触发低置信前缀读数；④ 无 lead/relevance 的 golden 由 crate 测试覆盖；⑤ `TAG_GRAPH` rank=3（`inject.rs:331`，crate 内有断言）；⑥ `edited=unknown` 三态**只在 crate 测试里**（e2e 层我只读到 `edited=false`，见 §6） | ⚠️ ① ② ⑤ 达标；③④⑥ 未独立取证 |
| **A-14** | §1.5 六条 graph 路由 | 8811 根（逐条 HTTP） | 六条都在：`/graph/retrieve`、`/graph/communities`（空根 `{communities:null,built:false}` ⇒ **None 与空数组可区分**）、`POST /communities/build`（`{level,communities:1,entities_covered:2,non_isolated:2,split_by_modularity:0}`，`covered ≤ non_isolated`）、`PUT /community/999/summary` ⇒ **404**、`/resolution/pending`（`pending` 与 `redundant` 分开）、`POST /resolution/merge`（`keeper==absorbed` ⇒ **400**、缺 id ⇒ **404**）；`stats.empty_reason/truncated_by` 透出（`null` 与值都可读）；**`as_of` 三写法逐字节一致**（3 条同瞬间写法：len 474、sha256/16 全为 `759cf2372f5bc019`）、非时刻 ⇒ **400 且消息点名**（3/3 变体）；**但 `/graph/entity/{id}/facts?at=` 不归一化**（F2） | ⚠️ 六条路由 ✅、`as_of` ✅、`at=` ❌（F2） |

### 负例锚点（N-1..N-6）

| # | 负例 | 我的读数 | 判 |
| --- | --- | --- | --- |
| **N-1** | 未知 store 被拒 + 无副作用 | **400**；`episodes/memory_diffs/memories` = (0,0,0)→(0,0,0) | ✅ |
| **N-2** | `dry_run+confirm_plan` 被拒 | **400** `dry_run and confirm_plan are mutually exclusive: … (got dry_run=true with confirm_plan=1)`；`wiki_builds` **2 → 2（不增行）** | ✅ |
| **N-3** | 空 `reason` 的纠错被拒 | **400** `a correction must record why (reason is empty)`；`wiki_corrections` **0 → 0** | ✅ |
| **N-4** | 分页不改排序（C5） | 见 §3.3 的 `limit ∈ {5,10,20,30}` 读数（我复现 V-A 的 15/15 与自己的采样面 0/11 不同） | ✅ |
| **N-5** | 每个分数都有 kind | `/knowledge/search` 的 hit 实测 21 个键，含 `score_kind`/`semantic_score_kind`/`keyword_score_kind`/`relevance_kind`（值见 A-13①） | ✅ |
| **N-6** | 注入丢块必有计数 | **0/5 事件有 `budget`** ⇒ 这条**无法判**（不是通过也不是失败）；渲染里我也没看到丢块通知（无预算绑定样本） | ⚠️ 无法判（原因：`budget` 未采集，作者已登记） |

## 3 重点独立复核（我自己的进程 / 副本 / 命令）

### 3.1 G8：带数字 coverage 的两侧对照（**我自己重做**，含负控）

方法：`%TEMP%\ruagent-vint-probe`（空根）→ `ruagent.exe serve --addr 127.0.0.1:8811 --root <root>` → ingest 一份 `deploy-guide` → 读它**真实存在**的 chunk id（`GET /knowledge/documents/{id}` ⇒ `[1, 2]`）→ 写 `replies.json`（marker `WIKI PLANNER` / `WIKI PAGE WRITER`）→ `POST /knowledge/wiki/build {scope:"all", agent:"wiki"}` → **轮询到终态**（契约 §6.0 第 18 条）。

```
[NEG #0] POST wiki/build -> HTTP 202 {"build_id":1,"status":"running","agent":"wiki","pages_planned":0}
[NEG #0] EARLY DB read right after POST (before polling) = [(1,'running',0,0,0)]
[NEG #0] polled status sequence = ['running','done'] (n=2)
[NEG #0] wiki_builds      = [(1,'all','done',0,'wiki',1,0,1,…)]          ← planned=1 written=0 failed=1
[NEG #0] wiki_build_pages = [(1,'neg-page','create','failed',
          'citation check failed (2 problems); uncited section [运行时间]: … has no citation anchor; …[步骤]…')]
[POS real-chunk] wiki_builds      = [(2,'all','done',0,'wiki',1,1,0,…)]   ← planned=1 written=1 failed=0
[POS real-chunk] wiki_build_pages = [(2,'deploy-pipeline','create','written', None)]
wiki_pages = [('deploy-pipeline', sections=2, cited_sections=2, **cite_coverage=1.0**, links_out=2, links_out_broken=2, self_links=0)]
GET /knowledge/wiki/pages -> {"slug":"deploy-pipeline",…,"freshness":"fresh","citations":2,
                              "cite_coverage":1.0,"uncited_sections":[],"links_out":2,"links_out_broken":2,"self_links":0}
GET /knowledge/wiki/links -> keys = [broken, degrees, edges, nodes, orphans, readings_at, self_links, unreachable, wanted]
                              degrees = [{"slug":"deploy-pipeline","links_in":0,"links_out":2,"links_out_broken":2}]
                              wanted  = [{"slug":"runbook","demanders":["deploy-pipeline"],"demand_count":1},
                                         {"slug":"tea-notes","demanders":["deploy-pipeline"],"demand_count":1}]
                              broken  = ["runbook","tea-notes"]        ← 与 wanted.slug 逐位相等
**注入侧**（同一根，一次 chat + 一次 run；大小写敏感正则 `coverage=[0-9][^\s\)]*`）
   ev[0] path='chat' tags=['knowledge','wiki'] chars=1329 coverage_digit=['coverage=1.00','coverage=1.00']
   ev[2] path='run'  tags=['knowledge','wiki'] chars=1305 coverage_digit=['coverage=1.00','coverage=1.00']
   wiki marker line: `wiki/deploy-pipeline: deploy-pipeline — stale=false coverage=1.00 anchors=2 edited=false`
   hint line:        `generated wiki page — verify against its sources before trusting`
```
⇒ **两侧对上**（构建 `cite_coverage=1.0` 记录值；注入 `coverage=1.00` 数字），且**负控同根同工具**变红（`#0` ⇒ `failed` + `citation check failed (2 problems)`）。**t50 的结论我独立复现成立**；t48 的失败形状也复现（`#0` 指不到 chunk）。
**顺带复现契约 §6.0 第 18 条的现场**：`POST …/wiki/build` 立刻回 `running`、`pages_planned=0`，紧接的 DB 早读是 `(1,'running',0,0,0)` —— **早读会给出假象**，我这轮也踩到了（第一次读到的就是这个）。

### 3.2 DEP-1 生产帧三方对照（captain 指定）

对象集 = 活库 `recall_log` 的**全部 23 个不同查询**（1 查询 = 1 样本，不按调用次数加权）；语料 = **我的 `VACUUM INTO` 只读副本**（14,188,544 B，12 列→我的进程迁移到 24 列）。

```
活库历史（改前，旧守护进程写的 651 行，只读 SQL）
  recall_log rows=651  distinct queries=23  source IS NULL=624
  entities>0 per call : **5/651 = 0.77%**
  entities>0 per query: **1/23**（唯一 `ruagent`，5 次调用全部命中）
改后（我的副本 + 新二进制 + 同一 23 查询帧走生产 `GET /recall?source=probe`）
  [1/23] 'kettle'            http=200 entities=0 graph.entities=0 graph.paths=0
  [2/23] 'autohotkey-v2'     http=200 entities=1 graph.entities=1 graph.paths=4
  [4/23] 'ruagent'           http=200 entities=5 graph.entities=5 graph.paths=12
  [9/23] 'postgres 连接池'      http=200 entities=2 graph.entities=2 graph.paths=5
  [14/23]'Windows 11 运行环境 操作系统' http=200 entities=5 … paths=12
  [19/23]'中文交流 简洁 直接 表达习惯'  http=200 entities=0
  …
  **AFTER production: entities>0 = 13/23；graph.entities>0 = 13/23；graph.paths>0 = 10/23**
冻结帧（t3/t7 报告 + 规格 A12/A13）：strict 1/23 = 4.3%，target ≥13/23
```
⇒ **改前 1/23（两条独立路径：活库历史遥测 + 规格冻结帧）、改后 13/23 = 56.5%**，**生产路径达标**（过去这条一直是 `not_measured`，owner=INT；现在有读数）。
**第一次跑的 400 也是一个读数**：`RUAGENT_EMBEDDER=hash` 让副本的 384 维向量对不上 256 维 hash embedder ⇒ 23/23 全是 `400 lancedb: … query vector dimension: 256`；换成真模型（`HF_HOME=%TEMP%\va-hf`）后 23/23 → 200。**「读数前先确认这份向量表属于哪个 embedder」**是本轮的一条仪器教训。

### 3.3 闭环计数（knowledge / wiki / entity 块）与两条路径一致性

对象 = 8811 根 `data/transcripts/*.jsonl` 的全部 `context_injected`（= 5 个）。

| 读数 | 值 |
| --- | --- |
| 总注入事件数 | **5** |
| 含 `path` | **5/5**（`path='chat'` 3、`path='run'` 2） |
| 含 `budget` | **0/5**（`budget=null` = 本次未采集，不是全 0 ⇒ 符合契约 §3.2，但 A-2 的比值这条**只有 `path` 一侧**） |
| 块 tags | `knowledge` **5/5**、`wiki` **5/5**、`graph`（实体）**0/5** ⇒ **G10 不成立**（F5） |
| 块头行 | 5/5 都是 `<knowledge>…</knowledge><wiki>…</wiki>` |
| 两条路径一致性 | 同一段上下文在 chat 与 run 上：块集合相同、`coverage=1.00` 相同、`chars` 1329/1328（chat）vs 1305/1210（run）；**两路径差异只在字符数**（run 的 prompt 段不同），无块丢失、无截断差异 |
| `limit` 分页不改排序（N-4 的 API 面） | `limit ∈ {5,10,20,30}` 的 top-1 文档与分数一致（V-A 已给 15/15 + 我的采样面 0/11）；本单另读 `/knowledge/search?limit=500` ⇒ 200 且 `hits=10`（我的根只有 10 个 chunk ⇒ **1..50 的上夹在本根上不可证伪**，§6 登记） |

### 3.4 `as_of` 三种写法 + 非时刻（A-14 ⑧）

```
/graph/retrieve?q=VintAlpha&hops=2&as_of=<同一瞬间三种写法>
   '2026-09-01T00:00:00+00:00'  http=200 len=474 sha256/16=759cf2372f5bc019
   '2026-09-01T00:00:00Z'       http=200 len=474 sha256/16=759cf2372f5bc019
   '2026-09-01T08:00:00+08:00'  http=200 len=474 sha256/16=759cf2372f5bc019   ⇒ 逐字节一致
   非时刻 'not-a-time' / '2026-13-45T99:99:99Z' / ''  ⇒ 400 `as_of \`…\` is not an RFC3339 instant`
/graph/entity/{id}/facts?at=<同一瞬间三种写法> ⇒ 三种写法 len 都 172（一致）
   **但** 'not-a-time' / '2026-13-45T99:99:99Z' / 'now' ⇒ **200** 且结果集变大（len 353，等价于「不过滤」）；`at=''` ⇒ 200 {"facts":[]}
```
⇒ A-14 ⑧ 在 `/graph/retrieve` 上**达标**，在 `/facts?at=` 上**不达标**（F2）。`stats.graph_edges_total` 我读到 1（我的根）——**它不是「那个时点的边数」**这一点我在自己的读数里也确认：`/graph/retrieve` 不带 `as_of` 与带三种 `as_of` 的 `graph_edges_total` 都是同一个值。

### 3.5 `recall_log` 逐列（A-3a/A-3b 的**值**侧）

```
副本（我的进程迁移后）recall_log = 24 列
新行（source='probe'，24 行）：**23/24 列有值**；唯一 NULL = top_memory_score（该根无记忆 ⇒ 不适用）
   score_kind='rrf_rank'  fusion='rrf:k=60,w_semantic=2,w_keyword=1'  knowledge_leg_window=60
   scoring_version=1  top_knowledge_relevance=0.2166 + kind='calibrated'
   top_legs_json=[{"chunk_id":4,"score":0.04918,"score_kind":"rrf_rank","semantic":{"rank":0,"raw":1.4655,"kind":"semantic_l2sq"},"keyword":{"rank":0,…}}]
   candidates_json / selected_json / rejected_json 都有值；graph_entities/graph_paths 与响应 graph.stats 一致
迁移前的行：624 行（source IS NULL）**任一新列非 NULL 的行数 = 0**；**任一新列 = 0 的行数 = 0**（NULL 不是 0）
副本里另有 11 行 source 非 NULL 但新列全 NULL：那是迁移前旧守护进程写的探针行（按 schema 年龄而非 source 分）
```
**⇒ 写入侧达标；读出侧不达标**：`GET /recall/log` 的响应体实测是
`{"log":[{ts,query,strategy,top_n,memories,knowledge,wiki,entities,top_memory_score,top_knowledge_score,source,source_label}],"retention":{…},"source_filter":null}`
—— **只有 11 个旧列 + `source_label`**，契约 §1.2 要求的 14 个新列**一个都没有**（F1）。

### 3.6 旧 transcript 反序列化（契约 §1.6 BREAK-CORE-1 / t47 §15.2）

- **我自己的新事件**（8811 根）：`{"type":"context_injected","render":…,"path":"chat"|"run"}`，`budget=null` ⇒ `path` 在用、`budget` 未采集。
- **旧形状的兼容性**：`crates/daemon/tests/event_compat.rs`（t47 新增）在我这次 `test --workspace` 里 **2 passed**（`event_compat` 目标）——它喂旧形状 `{"type":"context_injected","render":"…"}` 断言 `render` 字节保真 + `path==None` + `budget==None`。这是**编译面/行为面**的证据；我**没有**另外用 Rust 探针去反序列化一份真实历史 JSONL（§6 登记）。
- 活库历史 transcripts 我没有拷贝（只读也没读）⇒ 「旧 JSONL 真的读得进」这条我**只**引用 crate 测试的绿，不冒充端到端读数。

### 3.7 面板 DOM 与 t53/t55

| 读数 | 值 |
| --- | --- |
| `npm run check` | **exit 0**（`tsc -b --noEmit && tsc -p e2e/tsconfig.json --noEmit`） |
| `npm run i18n:check` | **exit 0**；打印 `keys with no string literal anywhere outside i18n.tsx: 76 (of which 40 are also outside every dynamic family prefix)` |
| `npm run build` | **exit 0**，55 assets，`dist/index.html` 存在（`build-panel.mjs` 内部先 i18n/tsc 再 vite —— 与 t55 的更正一致；我另跑了同两步的 `check`/`i18n:check`，同样 exit 0） |
| e2e（**有数据的根** 8811） | **42 passed / 1 failed / 2 skipped**；四条新 consumption 断言：`Wiki page: a page with no build reading shows coverage unknown` ✓、`Memory page: the recall log labels each score with its dimension` ✓（DOM：`…记忆腿余弦 0.27名次分 0.049…`）、`Knowledge page: a hit's score carries its dimension` ✓、`Graph page: an entity reaches the DOM` ✓；`wiki.spec.ts` 3 ✓；`knowledge.spec.ts` 1 ✓；**`registry.spec.ts` 跑了并且绿**（`npm run test:e2e` 会置 `RUAGENT_E2E_ALLOW_WRITES=1`，所以契约 A-6 里「registry 跳过」这条期望与入口行为**不一致**——它跳过只发生在裸 `npx playwright test`；我这次它写的是**我的临时根**，不是活库 config） |
| e2e（**空根** 8813） | **40 passed / 4 failed / 1 skipped**；4 条红 = `Wiki page … coverage unknown`（空根没有 wiki 页行）、`Knowledge page … dimension`（无知识文档）、`Graph page … entity`（无实体）、`chat round-trip`（冷根的模型目录探测 15 s 内没填上 `mock-pro`；有数据/已预热时它过到第 68 行才因缺脚本化 alpha 而红） |
| 归因 | **4 条红都不是面板代码缺陷**：3 条要求根里**有** wiki 页/实体/知识文档（CI 在 e2e 前跑 `ruagent doctor` 播种 ⇒ 与我的空根不同），1 条要求冷启动的模型目录在 15 s 内探到。⇒ A-6 的 DOM 读数**可复现但依赖根的数据形状**（登记为读数的条件，不写 finding） |

### 3.8 §1.6 BREAK-* 逐条状态（只读读数；captain 指定补取证）

| ID | 我的读数 | 状态 |
| --- | --- | --- |
| BREAK-REC-1（腿窗口恒 60） | 新行 `knowledge_leg_window=60`、`scoring_version=1`、`fusion='rrf:k=60,w_semantic=2,w_keyword=1'`；迁移前行这 4 列全 NULL（0 行是 0） | **已交付** |
| BREAK-REC-2（新键加法） | `/recall` 响应顶层多了 `scoring{}`（score_kind/fusion/leg_window/scoring_version/candidates/top_knowledge_score/top_knowledge_relevance{,_kind}）与 `graph{}`；旧键未改 | **已交付** |
| BREAK-KB-1（`limit` 夹 1..50） | `limit=500` ⇒ **200**、`hits=10`、`total=10`（我的根只有 10 chunk ⇒ 上夹**不可证伪**，见 §6） | **部分可判** |
| BREAK-KB-2（`bigram` 值） | `/knowledge/search` 的 `query_keyword_stage` 是字符串；V-A 已给 `stage=Bigram`/`bigram` 的读数；本轮我读到 `precision`/`substring`/`prefix`（我的查询面没触发 bigram） | **已交付（引用 V-A）** |
| BREAK-MEM-1（未知 store ⇒ 400） | **400** + 无副作用（A-9/N-1） | **已交付** |
| BREAK-MEM-2（op 词表加值） | `/memory/diffs` 我读到 `insert`/`skip_dedupe`/`reject`（含 `reject` 的 reason：`store \`profile\` cannot write namespace \`global\``）；表内计数 `insert 3 / reject 2 / skip_dedupe 3`；面板按未知 op 原样显示（e2e 未覆盖） | **已交付（部分）** |
| BREAK-WIKI-1（`links_out` 收窄） | `pages[i].links_out == degrees[i].links_out`（2 == 2）；`self_links=[]`；`links_out_broken=2`；`broken == wanted.slug`（逐位相等） | **已交付** |
| BREAK-WIKI-2（`dry_run+confirm_plan` ⇒ 400） | **400** + 不增行（N-2） | **已交付** |
| BREAK-WIKI-3（plan-only 词表） | 4 个 build：`(1,'all','done',0)`、`(2,'all','done',0)`、`(3,'all','done',0)`、`(4,'all','planned',1)` ⇒ **配对不变量（`status ∈ {planned,planned_only}` ⇔ `dry_run=1`）成立**；dry_run 响应带 `plan[]` | **已交付** |
| BREAK-GRAPH-1（实体来源换成 resolve_seeds） | 生产 `/recall`：`entities` 与 `graph.entities` 同值（13/23 帧上逐条相同）+ 新增 `graph.paths`；`recall_log.entities` 语义未改 | **已交付** |
| BREAK-CORE-1（`ContextInjected` 加可选 `path`/`budget`） | 新事件 5/5 有 `path`；`budget=null`；`event_compat` 2 passed（旧形状读得进） | **已交付（`budget` 采集未交付，已登记）** |

## 4 差异表（作者读数 → 我的独立读数）

| 项 | 作者读数 | 我的独立读数 | 差异 / 归因 |
| --- | --- | --- | --- |
| G8 构建侧 | `wiki_pages.cite_coverage=1.0`、`pages_written=1/failed=0`、API `{"freshness":"fresh","citations":2,"cite_coverage":1.0,"uncited_sections":[]}` | **逐位相同**（我的根、我的 exe） | 无 |
| G8 注入侧 | `coverage=1.00`、`coverage_with_digits_count=1`（大小写敏感） | **相同**（`coverage=1.00` 每事件 2 次） | 无 |
| G8 负控（t48） | `#0` ⇒ `failed` + `citation failed …` | 相同（`failed` + `citation check failed (2 problems)`；我还给出 `#1` 的正例） | 无 |
| DEP-1 生产帧 | `not_measured`（t19–t48 一直未交） | **改前 1/23（历史遥测 5/651）+ 改后 13/23（我的副本走生产路径）** | **从 not_measured 升级为达标读数** |
| 注入事件计数 | t45 §13.2④：`transcripts=1, context_injected=1, with_path=1, with_budget=0` | **5/5 有 path（chat 3 / run 2）、0/5 有 budget** | 一致（我另外给出两路径与块种类） |
| `as_of` 三写法 | 长度都 1053、sha `4D-54-17-A7-20-6`、`as_of_all_equal=True`（t45 的受控语料） | 我的语料不同（我的根）：len 474、sha `759cf2372f5bc019`、三写法逐字节相同 | 一致（不同语料，故绝对长度不同） |
| `recall_log` 逐列 | t45：**有值 22 列 / 恒空 2 列**（`top_memory_score`、`source`） | **23/24 有值，唯一 NULL = `top_memory_score`**（我给了 `source='probe'` ⇒ `source` 有值） | 一致（差异来自调用方是否声明 source） |
| 门禁 `test --workspace` | `TEST_EXIT=0`；`ok` 55 行 / `FAILED` 0 行；**passed 求和 386** | **exit 0**；`ok` 55 行 / `FAILED` **0** / `panicked` **0**（`Select-String -CaseSensitive`）；**passed 求和 387** | **+1**：`score-kind-wire.rs`（02:05）与 `gold-seed-idempotence.rs`（02:07）是 §17 之后落地的；且求和口径由我用正则 `(\d+) passed` 在整份日志上取（含 doc-tests 的 `0 passed` 行）⇒ 计数基准可能不同。**不是缺陷，是口径差**。 |
| 面板 e2e | `4 passed` + 负控（回退两处显示 ⇒ `2 failed`） | 有数据的根：**42 passed / 1 failed / 2 skipped**（四条新 consumption 全绿，DOM 文本逐字复现）；空根：40/4/1 | 数字不能直接比（作者只跑四条 consumption；我跑整套）。**作者的负控我没有复现**（需要改 `panel/src/**`，不在我的 inScope，§6） |
| `links_out` 收窄 | `links_out` 仍 2、自链 `self_links=1`、`links_out_broken=1`（t51 自链夹具） | 我的夹具不同：`links_out=2`、`self_links=0`、`links_out_broken=2`、`broken==wanted.slug`、`pages.links_out==degrees.links_out` | 一致（夹具不同，判据相同） |
| MCP 5 工具 | `tools/list` 14 个 | **未独立取证**（§6） | 未测 |

## 5 findings（未改任何被验代码；每条给期望/实际 + 复现命令）

### F1（medium · 读出侧）`GET /api/v1/recall/log` **不透出任何新列**，契约 §1.2 要求「新列一律透出」

- **期望**：响应行含 `score_kind`/`knowledge_leg_window`/`scoring_version`/`fusion`/`top_knowledge_relevance(_kind)`/`top_legs_json`/`candidates_json`/`selected_json`/`rejected_json`/`graph_entities`/`graph_paths`（契约 §1.2 的 `rows:[{…}]` 全列）。
- **实际**（8811 根与副本根都读到）：`{"log":[{ts,query,strategy,top_n,memories,knowledge,wiki,entities,top_memory_score,top_knowledge_score,source,source_label}],"retention":{…},"source_filter":null}` —— **12 个键，14 个新列一个也没有**；handler 的 `SELECT` 只取 11 列（`crates/daemon/src/api.rs` 的 `recall_log`，`SELECT ts, query, …, source FROM recall_log`）。
- **影响**：`recall_log` 的**值**在盘上（A-3b 写入侧达标），但**消费面读不到** —— 契约 §3.3 列的 7 个问题（哪条腿找到 top-1 / 丢了多少为什么丢 / 图这一腿有没有参与）在面板与 MCP 上**都答不了**。这是「列在、值在、读出不在」的第三态。
- **复现**：`python %TEMP%\vint-probe5.py`（打印原始响应体与行键）。
- **建议（不替作者修）**：把 handler 的 `SELECT` 扩到 24 列（并把契约写的 `rows` 键名与实际的 `log` 对齐，或反之——见 F7）。

### F2（medium · 静默语义）`GET /api/v1/graph/entity/{id}/facts?at=` 不归一化输入，非时刻字符串**静默 200**

- **期望**（契约 §1.5）：`/facts?at=` 与 `/graph/retrieve?as_of=` **同一套归一化**；非时刻 ⇒ **400**；不许静默按字符串比较。
- **实际**：三种同瞬间写法逐字节一致 ✓，但 `at='not-a-time'` / `'2026-13-45T99:99:99Z'` / `'now'` ⇒ **HTTP 200 且返回不同的结果集**（len 353，含 `valid_at=2026-09-28` 的那条；`at=''` ⇒ 200 空数组）。对照组 `/graph/retrieve?as_of=not-a-time` ⇒ **400**（消息点名）。
- **影响**：A-14 ⑧ 只在一半入口成立；这是 V-C F-1（`as_of` 文本比较）同一类问题的残留面 —— 一个打错的时间串会得到「不过滤」的结果，且**没有人会看到错误**。
- **复现**：`python %TEMP%\vint-probe6.py`（(C) 段）。

### F3（medium · 未交付）`prompt_hash` 未接线：`GET/PUT /api/v1/distill` 都不带它（F-6 / K-11）

- **期望**（契约 §1.2 F-6、A-11）：`/distill` 的 GET/PUT 返回 `prompt_hash = sha256(生效 prompt)`；一次 `PUT` 后它**必须变**。
- **实际**：`GET /distill` 键 = `[agent, auto, builtin_prompt, graph, language, prompt]`（无 `prompt_hash`）；`PUT /distill` ⇒ `{"ok":true}`（无 `prompt_hash`）。`prompt_hash` 在 `distill.rs` 内部**算了**并写 `distill_log`（`content_hash(&self.compose_prompt())`），**但没有任何 HTTP 出口** ⇒ A-11 前半不可判、后半（PUT 后变化）**无法测**。
- **影响**：K-11 的「单一计算点/单一出口」不存在；「产出率变化可归因」缺少机械判据。
- **复现**：`python %TEMP%\vint-probe6.py`（(D) 段）。

### F4（medium · 未交付）`/api/v1/stats` 没有 `embedder`，也没有 `scoring_version`（F-8）

- **期望**：F-8 要求 `/api/v1/stats` **同时**暴露 `embedder` 与当前 `scoring_version`（跨版本可比性的唯一读点）。
- **实际**：`GET /stats` ⇒ `{"agents":[{"agent":"probe","runs":2,…}]}`（只有 agents 统计）。`embedder` 实际在 **`GET /api/v1/knowledge/documents`** 上（我读到 `"embedder":"hash-embedder"`）；`scoring_version` 只在 `/recall` 响应与 `recall_log` 里。
- **影响**：任一消费方要判断「这条曲线换了轴没有」得同时读三个端点，且契约点名的那个端点是空的。
- **复现**：`python %TEMP%\vint-probe6.py`（(D) 段）。

### F5（medium · 冻结块无生产者）注入面**永远不会**出现 `<graph>` 块 —— G10 不是「未测」，是「没有构造点」

- **期望**（契约 §3.2 K-5 + §6.3 G10）：`TAG_GRAPH` rank=3 的块必须能出现在生产 transcript 里；至少一条带 `<graph>` 的盘上事件。
- **实际**：我在**有图证据**的同一批 turn 上（`/recall` 同根读到 `graph.entities=1..5`、`graph.paths>0`）读了 5 个 `context_injected`：块 tags = `{knowledge, wiki}` 5/5，**`graph` 0/5**。代码侧只读核对：`TAG_GRAPH` 仅出现在 `crates/memory/src/inject.rs`（`:186` 定义、`:331` rank 表、`:1059-1081` 断言/测试），`crates/daemon/**`、`crates/acp/**`（含 `chat.rs`）里**没有** `TAG_GRAPH`/`graph_items`/`GraphEvidence` 的构造点（grep 4 处命中全在 `inject.rs`）。
- **影响**：契约 §6.3 给 G10 的豁免是「若 A-2 未跑到含 graph 的路径 ⇒ `not_measured`」——**该豁免不适用**：我跑到了含图证据的路径，注入面仍然不产 graph 块。⇒ 这是「冻结了形状但没有生产者」，RV-INT 应按 **G10 未达标**判，而不是 `not_measured`。
- **复现**：`python %TEMP%\vint-probe2.py`（tags 与计数）、`grep TAG_GRAPH crates/`。

### F6（low-medium · 报告声称与实测不符）wiki 纠错端点对**不存在的 slug** 不报错、并落盘

- **作者声称**（t51）：`POST …/wiki/pages/{slug}/corrections` 的 400「点名未知名与空 reason/author」。
- **实际**：`{"kind":"pin","reason":"because","author":"human"}` 对 `no-such-page` ⇒ **200 `{"id":1,…}`**；再对 `no-such-page`/`zzz-does-not-exist` × `pin`/`release`/`note` **6/6 全部 200 并落盘**（`wiki_corrections` 累计 8 行，其中 7 行的 slug 不是任何 wiki 页）。`kind` 校验 **400** ✓（`unknown correction kind \`nope\`; accepted: pin, release, note`）、空 `reason` **400** ✓ 且不落盘、不存在的 `kind`/空 reason 都不增行。
- **影响**：审计面会为不存在的页积累纠正行；「未知名 ⇒ 400」这条判据目前**不成立**。契约 §1.4 并未明文要求 404（所以严格说这是**报告多报了**而不是实现违约）——两条处置：改报告，或补 existence 检查。
- **复现**：`python %TEMP%\vint-probe4.py`（(C) 段）。

### F7（low · 文档级）契约与实际响应键不一致两处

- `/recall/log`：契约 §1.2 写 `{rows:[…]}`，实际是 `{"log":[…],"retention":{…},"source_filter":null}`。
- `POST /graph/communities/build`：契约 §1.5 写 `{level, communities, entities_covered, non_isolated, split_by_modularity}` 且 `Community = {id,level,parent,summary,entity_ids}`；实际**这个端点**回的是 `communities: 1`（**计数**，不是数组）——与「回五个计数」的意图一致，但字面读会以为 `communities` 是数组（数组只在 `GET /graph/communities` 上）。A-14 ③ 的「不许 200 空体」达标。

### F8（low · 读数的条件，不是实现缺陷）A-6 的面板 DOM 读数依赖根的**数据形状**（CI 靠 `ruagent doctor` 播种）

- 有数据的根（8811）：4 条新 consumption 断言全绿，DOM 文本逐字复现；空根（8813）：其中 3 条红（缺 wiki 页/实体/知识文档），另 1 条 `chat` 红在冷根的模型目录探测上（15 s 超时；预热后它过到第 68 行再因缺脚本化 mock 而红）。
- **影响**：把「面板四页 DOM 读数」写成 root-independent 的绿会误导；正确形状是「在播种过的根上绿」。CI 的顺序（`ruagent doctor` → panel e2e）解释了它为什么绿。

### 观察（不进 findings）

- **O1** `budget` 的读数 **0/5**（作者已按 not_measured 登记，owner=mem-core）——我给出真读数：`path` 5/5、`budget` 0/5、`used_chars/total_chars/truncated_blocks/dropped_items/blocks[]` **全部无法读**（N-6 因此无法判）。
- **O2** t52 的 `RejectedNamespace` 我复现：`store='observation', namespace='global'` ⇒ **200 `{"outcome":"RejectedNamespace"}`**；`namespace='user'` ⇒ **200 `{"outcome":"Inserted(1)"}`**；未知 namespace ⇒ **400 带词表**（`user | global | project:<x> | agent:<x>`）；被拒的写入在 `memory_diffs` 里留 `op='reject'` + reason（按设计）。
- **O3** 冻结页构建：pin 之后 `POST /wiki/build` ⇒ `wiki_build_pages (3,'deploy-pipeline','create','skipped','frozen: vint probe pin (by human at 2026-09-28T16:23:40…)')` 且 `wiki_builds (3,'done',1,0,0)` ⇒ **F-3/G6 的「秒级落地 + error 含 frozen: <reason>」成立**。
- **O4** 一个**失败**的 `/recall` 也可能留下行（我的第一次跑全部 400；副本里 `source='probe'` 的行有 11 行新列全 NULL，来自迁移前的旧探针行）——读数时要用 `source` + `score_kind IS NULL` 一起判，不能只看 `source`。
- **O5** `/knowledge/search` 的 hit 有 21 个键，四个 `*_kind` 都是**冻结字面量**（`rrf_rank` / `semantic_l2sq` / `bm25` / `calibrated`）⇒ V-A 的 F-5（`ScoreKind` serde 会输出 `"RrfRank"`）在**这条手写字面量的路径上**没有发作；t29 的 `score-kind-wire.rs`（6 passed）在新树里绿。
- **O6** `registry.spec.ts` 在 `npm run test:e2e` 下**会跑**（入口置 `RUAGENT_E2E_ALLOW_WRITES=1`），契约 A-6 写的「registry 跳过」只对裸 `npx playwright test` 成立。

## 6 未测项（写明原因，不冒充通过）

| 项 | 状态 | 原因 / 前置 |
| --- | --- | --- |
| **A-4 MCP 5 工具**（`tools/list`=14、`store:"bogus"` 的工具错误、`recall_log.source='mcp_recall'`） | **未独立取证** | 需要驱动 MCP stdio 子进程（t46 用了临时 root + `ruagent mcp-serve`）；本轮预算用在了 G8/DEP-1/闭环/面板四处。**编译面证据**：我的 `test --workspace` 覆盖 `-p ruagent-mcp`（`roundtrip` 目标绿）。 |
| **t51 的负控**（回退两处显示 ⇒ e2e `2 failed`） | **未复现** | 需要改 `panel/src/**`（`panel/` 不在我的 inScope：一个文件一个写者）。我改为**输入驱动**的负控（G8 的 `#0`、N-1/N-2/N-3、`/facts?at=` 的垃圾串），并在 §4 登记该差异。 |
| **BREAK-KB-1 的上夹（1..50）** | **不可证伪（我的根）** | 我的根只有 10 个 chunk，`limit=500` 与 `limit=50` 都返回 10 ⇒ 观察不到 50 的上夹。要证伪需要 ≥51 chunk 的根。 |
| **`budget.blocks[]` / 预算绑定 / N-6 的丢块计数** | **无法判** | `budget` 全为 `null`（本次未采集，作者已登记 owner=mem-core）⇒ 没有可用于绑定的样本。 |
| **`<graph>` 在生产 transcript（G10）** | **我的读数是 0/5 = 未达标**（不是未测）；F5 给出代码侧无构造点的证据 | — |
| **`edited=unknown` 三态（A-13 ⑥）在 HTTP/注入面** | 未独立取证 | e2e 层我只读到 `edited=false`；`None` 场景由 crate 测试（`inject.rs:1124`）覆盖，我引用其绿但不冒充端到端读数。 |
| **旧 transcript 反序列化的端到端**（真实历史 JSONL） | 未做 | 我引用 `event_compat`（2 passed）作为行为面证据；未另写 Rust 探针反序列化一份真实历史行。 |
| **`clippy --workspace --all-targets -- -D warnings`** | 未重跑 | 契约 §6.0 要求「exit 0 **且**输出里有本次 `Checking <crate>`」两件证据；本轮我未跑 clippy（作者跑了 `-CleanFirst ruagent-daemon` 的形状）。⇒ 我的门禁声称面**只到 `test --workspace` 与面板三步**，不声称 lint。 |
| `--include-ignored` 的活库级仪器 | **已补跑一条**（graph `live-after`，见 §8.1：`REAL FRAME n=23 | strict non-empty 1 -> new 13`）；`retrieval-gold-copy/live` 两条**未跑**（属 recall 单） | — |

## 7 收尾（只读证据 + 我的足迹）

- **真守护进程未触碰**：`Get-Process -Id 79984` 在每个启停步骤后都读到 `alive, StartTime=09/27/2026 05:35:37`（同一值）；**未调 `/api/v1/recall`**；活库只以 `mode=ro` + `VACUUM INTO` 读取。
- **我自己起的守护进程（全部记录在案，收尾已停）**：8811 → pid **14060**（停）/ **47880**；8813 → pid **34712**；8812 → pid **53072**（停）/ **82088**。停止方式：只按我写在 `<root>\canary.pid` 里的 pid `Stop-Process -Id`（不按名字/端口批量杀）。
- **临时 root 收尾**：`%TEMP%\ruagent-vint-probe`、`%TEMP%\ruagent-vint-e2e`、`%TEMP%\ruagent-vint-live`、`%TEMP%\vint-live.db`、`%TEMP%\pw-vint*`（Playwright artefacts）—— 见 §8 的清理读数。
- **我的仓库足迹**：只有本报告一个文件（`git status --porcelain -- docs` 里 `?? docs/design/reviews/gen2-integration-verify.md`；`crates/`、`panel/` 我一行未改）；全树 porcelain 66 项是同伴在途（背景）。
- **被验文件的收尾哈希**（读数窗口结束时）：见 §8 表。

## 8 补充读数（收尾补跑）

### 8.1 腿级帧仪器（graph crate 的 `#[ignore]` 活库仪器，我自己的副本）

```
$env:RUAGENT_GRAPH_LIVE_COPY="$env:TEMP\vint-live.db"
… test -p ruagent-graph -- --include-ignored --nocapture        → ##### exit=0
REAL FRAME n=23 | strict non-empty 1 -> new 13 | queries with >1 seed: 7
frame: 23 queries
non-empty: old(strict) 1 -> new 13
self-name resolution: old(strict) 63/63 -> new 63/63 legs={ExactName}
[t42] frame instrument object set: recall_log rows=651 distinct_queries=23 entities=63
```
⇒ **腿级 before/after 也由我独立复现**（1/23 → 13/23），且与 §3.2 的**生产路径**读数（1/23 → 13/23）**两条独立路径同值**；自查询对照 63/63 → 63/63 无回归。（同一轮里 graph 自己的 `as_of` 读数也打印了 V-C F-1 的旧文本规则证据：`old TEXT rule … +08:00-form 27/67 edges judged backwards`。）

### 8.2 收尾（只读证据）

```
pid 47880 (mine, from %TEMP%\ruagent-vint-probe) alive after stop: False
pid 34712 (mine, from %TEMP%\ruagent-vint-e2e)   alive after stop: False
pid 82088 (mine, from %TEMP%\ruagent-vint-live)  alive after stop: False
pid 79984 alive: True   StartTime: 2026/9/27 5:35:37          ← 真守护进程未触碰
cleaned %TEMP%\ruagent-vint-probe: True      cleaned %TEMP%\vint-live.db: True
cleaned %TEMP%\ruagent-vint-e2e:   True      cleaned %TEMP%\pw-vint:      True
cleaned %TEMP%\ruagent-vint-live:  True      cleaned %TEMP%\pw-vint3:     True
```
（另有两个我起过并已停的 pid 记录在案：**14060**（8811 的第一代）、**53072**（8812 的第一代）。）

**背景**：收尾 `git status --porcelain` = **112 项**（并发波次持续增长：66 → 112），全部是同伴在途 —— 只作背景读数，不据此写 finding。`docs/design/reviews/` 整目录在 porcelain 里是未跟踪（`??`），所以我不能靠它证明「只有我写了我的报告」；**我的足迹证据**是：本单只创建 `docs/design/reviews/gen2-integration-verify.md`（42,931 B），`crates/**`、`panel/**`、`cli/**` 我**一行未改**（§1 的整目录 porcelain 与我的 inScope 无关）。
**一条必须披露的口径**：`gen2-integration-impl.md` 在我的验证窗口内一直在变（55,512 B @23:51:48 → 59,978 B @00:22:29 → **64,878 B @00:35:11**）；我引用的 §13–§17 是 **55,512 B 那一版**的内容（读数窗口 00:16–00:37 早于它的后两次写入）。**作者的更新不改变我的任何读数**（我的读数全部来自我自己的进程/命令），但如果 RV-INT 读到新版 §17 与我引用不一致，以「我引用的那一版」为准。

### 8.3 被验文件的收尾哈希（与读数窗口开始时逐位相同）

| 文件 | mtime（+08:00） | SHA256/16 |
| --- | --- | --- |
| `crates/daemon/src/api.rs` | 2026-09-28T23:55:04 | `D6E4EC42DB2510F9` |
| `crates/daemon/src/wiki.rs` | 2026-09-28T23:52:43 | `B9410E3D130A501F` |
| `crates/daemon/tests/knowledge_api.rs` | 2026-09-28T23:59:01 | `781B5C8913E2E36F` |
| `crates/daemon/src/{chat.rs,runs.rs}` | 2026-09-28T02:53:29 / 02:54:10 | `8A2AD0F3471D0FDC` / `D79152D7C29C86BE` |
| `crates/daemon/src/memembed.rs` | 2026-09-28T02:31:03 | `5DA744245199752B` |
| `crates/daemon/src/distill.rs` | 2026-09-28T02:19:31 | `C5EB52E7EF2F2394` |
| `crates/core/src/event.rs` / `crates/acp/src/chat.rs` | 2026-09-28T02:54:00 / 02:54:21 | `D19CA3277C6FC1B6` / `CFCBBBB27F24C494` |
| `crates/mcp/src/lib.rs` | 2026-09-28T03:32:50 | `BC14D9EF839C338F` |
| `cli/src/main.rs` | 2026-09-28T03:06:30 | `97965EFD0BA9CA7C` |
| `crates/daemon/tests/{injection_e2e.rs,event_compat.rs}` | 2026-09-28T02:35:54 / 03:40:47 | `9890CF0CDC27CE20` / `C895C21FC7049162` |
| `panel/src/views/Wiki.tsx` / `panel/src/views/Memory.tsx` / `panel/src/api.ts` | 2026-09-29T00:00:09 / 2026-09-28T23:59:53 / 2026-09-29T00:00:09 | `FC8BAF2CB0402318` / `B7A019114CBC3A66` / `32878718A3A14D99` |
| `panel/e2e/consumption.spec.ts` | 2026-09-29T00:06:49 | `D0C9C859E4B194CF` |

**⇒ 全部与我 build 出 `ruagent.exe`（00:17:21）之前一致，且收尾重哈希逐位相同** ⇒ §2–§5 的每条读数都绑定在同一份字节上。

## 9 复现命令（我的探针脚本都在 `%TEMP%`，不进仓库）

```powershell
$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"
# ① 门禁（契约 verify）
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 build -p ruagent
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test --workspace      # → exit 0
cd panel; npm run check; npm run i18n:check; npm run build                                        # → 三条 exit 0
# ② 我自己的根 + 守护进程（8811/8813；脚本：%TEMP%\vint-start.ps1 / vint-e2eroot.ps1）
# ③ G8 两侧 + 负控：python %TEMP%\vint-probe1.py     （含 §6.0#18 的早读现场）
# ④ 闭环计数/两路径：python %TEMP%\vint-probe2.py
# ⑤ 跨区读数（graph 六路由 / as_of / recall_log / forget / distill / T52 / N-1..N-3）：python %TEMP%\vint-probe3.py
# ⑥ 复核三条可疑读数 + BREAK-*：python %TEMP%\vint-probe4.py / vint-probe5.py / vint-probe6.py
# ⑦ 面板 DOM：$env:E2E_BASE_URL="http://127.0.0.1:8811"; cd panel; npm run test:e2e -- --output=%TEMP%\pw-vint
# ⑧ DEP-1 三方：python %TEMP%\vint-dep1a.py（副本+帧+改前）→ python %TEMP%\vint-dep1c.py（改后生产）→ vint-dep1d.py（逐列）
#    腿级：$env:RUAGENT_GRAPH_LIVE_COPY="$env:TEMP\vint-live.db"; … test -p ruagent-graph -- --include-ignored --nocapture
# ⑨ 收尾：%TEMP%\vint-cleanup.ps1（只按我记录的 pid 停、删我的 root）
```

## 10 裁决回执（captain 2026-09-29，追加式；不改 §1–§9 的任何读数）

| 本报告的 finding | captain 裁决 | 落点 |
| --- | --- | --- |
| **F5**（图块永不进注入面 ⇒ **G10 未达标，不是 not_measured**） | **采纳**（理由：我跑到了含图路径，5 个注入事件 graph 0/5，§6.3 的豁免不适用） | **t58（mem-core）**：让图证据真的进注入块，或显式修订契约并保留「未达标 + 下一代 owner」——**不许**把未达标写成 not_measured |
| **F1 / F2 / F3 / F4 / F6 / F7** | 全部采纳 | **t57（integ）**：F1 扩 `SELECT` 透出新列 + 键名对齐 · F2 `facts?at=` 归一化 + 非时刻 **400**（A-14⑧ 只在一半入口成立 = RVC-1 的残留面，t27 只修了 `retrieve`）· F3 `prompt_hash` 接线 · F4 `/stats` 补 `embedder`/`scoring_version` · F6 未知 slug 明确拒绝且不落盘 · F7 契约键名更正 |
| **F8**（e2e 的绿依赖根的数据形状） | 接受我的归因（4 条红全部可归因、非面板缺陷），**低优先登记为下一代测试基建项** | 下一代 |
| §4 的 **`passed` 求和 386/387** 漂移 | 判定为**非真实信号**（我/t50/t55 三份读数在同一命令、同一树附近给出三个数）⇒ 已立 **契约第 22 条**：跨 target 的 `passed` 求和不是稳定读数 | 契约（integ） |

**我这一侧对第 22 条的口径（供集成引用；全部来自我自己的日志与脚本）**：
```
工具：Select-String -CaseSensitive（PowerShell 7）；退出码取包装脚本的 ##### exit
  'test result: ok' 行数 = 55 ; 'FAILED' 出现次数 = 0 ; 'panicked' 出现次数 = 0
  逐 target：Running <target> 行与随后的 test result 行按顺序配对（两棵树对比也要这么配）
  求和口径（我用的，也是这次漂移的来源）：对整份日志正则 (\d+) passed 求和 = 387
    —— 含 doc-tests 的 0 passed 行，且含 §17 之后新落地的 target
       （score-kind-wire.rs 02:05、gold-seed-idempotence.rs 02:07）
```
**⇒ 我的门禁声称面不因裁决扩大**：`test --workspace` exit 0 + 逐 target/计数如上 + 面板 `check`/`i18n:check`/`build` 三条 exit 0；**不声称 clippy**（§6）。RV-INT（t21）现依赖 **[t57, t58]**，判整代时应连同本报告 §5 的 F1–F8 一起读。

## 11 F6 的定因实验（captain 2026-09-29：「t57 复现不出 6/6 200」）

**问题**：t57 对 `no-such-page-a` / `no-such-page-b` 打 `{"kind":"note","reason":"t57 probe","text":"x"}` ⇒ **400/400**；我 t20 报 6/6 **200 + 落盘**。两条读数都是真的 —— **差别在 body 形状，不在 slug**。

**受控实验**（我自己的新根 `%TEMP%\ruagent-vint-f6`，端口 8814，`wiki_pages` **0 行** ⇒ 该根里**每个** slug 都是「不存在的页」；同一件仪器、同一条路径 `POST /api/v1/knowledge/wiki/pages/<slug>/corrections`，无尾斜杠、无 query）：

| # | body | slug | HTTP | `wiki_corrections` 计数 |
| --- | --- | --- | --- | --- |
| A | `{"kind":"pin","reason":"vint probe","author":"human"}` | `no-such-page` | **200** `{"id":1,…}` | 0 → 1 |
| A2 | `{"kind":"release","reason":"vint probe","author":"human"}` | `zzz-does-not-exist` | **200** `{"id":2,…}` | 1 → 2 |
| A3 | `{"kind":"note","reason":"vint probe","author":"human"}` | `no-such-page` | **200** `{"id":3,…}` | 2 → 3 |
| **B** | `{"kind":"note","reason":"t57 probe","text":"x"}`（**无 `author`**） | `no-such-page-a` | **400** `a correction must record who (author is empty)` | 3 → 3 |
| **B2** | 同上 | `no-such-page-b` | **400** 同上 | 3 → 3 |
| C | `{"kind":"note","reason":"t57 probe","author":"human","text":"x"}`（有 `author` + 多余 `text`） | `no-such-page-c` | **200** `{"id":4,…}` | 3 → 4 |
| D | `{"kind":"note","reason":"t57 probe","author":"human"}` | `no-such-page-d` | **200** `{"id":5,…}` | 4 → 5 |
| E | `{…,"author":""}` | `no-such-page-e` | **400** 同上 | 5 → 5 |
| F | `{"kind":"note","reason":"t57 probe"}`（无 `author`） | `no-such-page-f` | **400** 同上 | 5 → 5 |

**结论（两条读数因此并列成立，F6 不改判）**：
1. **t57 的 400/400 = `author` 校验**（错误消息逐字是 `a correction must record who (author is empty)`），**与 slug 是否存在无关** —— 那个请求在到达任何 slug 逻辑之前就被拒了。`text` 字段**不是**原因（C 组带 `text` 且带 `author` ⇒ 200 ⇒ 未知字段被忽略，无 `deny_unknown_fields`）。
2. **在有合法 body（`author` 非空）的前提下，slug 不存在也一样 200 并落盘**（A/A2/A3/C/D 五连）；该根 `wiki_pages = 0` 行 ⇒ **端点确实没有存在性检查**。t20 我的 6/6 是 `{kind, reason:"vint probe", author:"human"}` × {`no-such-page`,`zzz-does-not-exist`} × {`pin`,`release`,`note`}（脚本 `%TEMP%\vint-probe4.py` (C) 段，逐行 `say`）。
3. 读法面：`GET /api/v1/knowledge/wiki/pages/{slug}/corrections` 对未知 slug 也 **200**（`{"slug":"no-such-page","corrections":[…]}`、`{"slug":"no-such-page-a","corrections":[]}`）⇒ 与「没有存在性概念」一致。

**原始数据（回答 captain 的 5 问）**：(1) slug 字面值 = `no-such-page` / `zzz-does-not-exist`（t20）、`no-such-page-{a..f}`（定因实验）；(2) 每条 body 见上表（t20 的是 `{"kind":<pin|release|note>,"reason":"vint probe","author":"human"}`，**`reason` 非空、无 `text`**；另有 `{"kind":"pin","reason":"because","author":"human"}` 一条）；(3) `POST /api/v1/knowledge/wiki/pages/<slug>/corrections`，JSON body，无尾斜杠、无 query，+ 同路径 `GET`；(4) **t20 的构建**：`%TEMP%\ruagent-team-target\debug\ruagent.exe`，mtime **2026-09-29T00:17:21**，root `%TEMP%\ruagent-vint-probe`（端口 8811）+ 副本根（8812）；**本次定因**用的是**更新的构建**（exe mtime **00:53:20** / sha256/16 `44BB0F8939EC671C`，`api.rs` mtime **00:49:05** / `DD78F267E6839AFB`，即 t57 在途编辑之后），root `%TEMP%\ruagent-vint-f6`（端口 8814）——**两版上 F6 行为逐字相同** ⇒ 结论不依赖旧二进制；(5) 「落盘」判据 = **整表** `wiki_corrections`：请求前后各一次 `SELECT COUNT(*)`（delta 见上表）+ 逐行 `SELECT id,slug,kind,reason,author FROM wiki_corrections ORDER BY id`（t20 的 probe3 打印 2 行、probe4 打印 8 行；本次实验 5 行）。**我的 t20 探针 root 已按收尾纪律删除**（§8.2）—— 上表是同一脚本形状的重跑，含原始输入。
**复现**：`powershell -NoProfile -ExecutionPolicy Bypass -File %TEMP%\vint-f6.ps1`（起 8814 → `python %TEMP%\vint-f6.py` → 只停我记录的 pid → 删我的 root）。

