# gen3 只读审计：wiki 引用闸门 / 新鲜度 / 纠错回路（t68）

> 单号 t68（audit，只读）· 成员 `wiki` · attempt 1 · 时间窗 2026-09-28 18:40–18:55 UTC（+08:00 02:40–02:55）
> 审计对象：`crates/daemon/src/wiki.rs`（4333 行）· `crates/mock-agent/tests/wiki_pipeline.rs`（1539 行 / 15 个 `#[tokio::test]`）· 面板 wiki 视图（`panel/src/views/Wiki.tsx`、`panel/src/api.ts` 的类型面）
> **本单只读**：写入集合只有本报告。不改代码、不写活库、未启停真守护进程（读数 `79984 alive: True`）。
> **仪器**：自建临时守护进程 —— root `%TEMP%\ruagent-t68-root`（已删）、端口 **127.0.0.1:8898**、PID **37172**（`stop` 后实测 `alive after stop: False`）；二进制由 `scripts/cargo-team.ps1 build -p ruagent` 在**当前字节**上重建（`ruagent.exe` 时间戳 2026-09-29 02:43:11）。探针脚本在 `%TEMP%\t68-probe.ps1`、`%TEMP%\t68-probe2.ps1`（不在仓库内），端点用 `PUT /api/v1/knowledge/raw/...` 播种、用 `GET /api/v1/knowledge/wiki/pages|links` 读数。

**方法**：先静态读 `wiki.rs` 的四个面（锚点解析 `:506-567`、闸门 `:668-789`、新鲜度 `:879-987`、纠错 `:1126-1201`、读端点 `:2788-2897`），再用**活体 HTTP 探针**对每条候选结论取证；**不能取证的一律进 §4「未验证猜想」，不进 finding 清单**。已证伪的假设单列 §3（本代规矩：把正确的当错误也是失真）。

## 1 读数总表（本报告全部 finding 的证据来源）

一次 `GET /api/v1/knowledge/wiki/pages`，三页正文逐字相同、只有 frontmatter 之差（探针 `t68-probe.ps1`）：

| slug | 正文锚点 | frontmatter `citations` | `freshness` | `stale` | `stale_reasons` | `stale_since` | `cite_coverage` | `citations` | `uncited_sections` |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `p-body-dangle` | `<!-- cite: probe-src#99999 -->`（**断链**） | `[]` | **fresh** | false | `[]` | null | null | 0 | `[]` |
| `p-front-dangle` | 同上（逐字相同） | 记了 `probe-src#99999` | **stale** | true | `["chunk missing"]` | **null** | null | 1 | `["Body"]` |
| `p-drift` | 无锚点 | 记了 `probe-src#1`（hash 过期） | **stale** | true | `["source hash drift","chunk hash drift"]` | **null** | null | 1 | `["Body"]` |
| `p-zero` | 无锚点 | 记了 `chunk_id: 0`（解析器丢弃） | **fresh** | false | `[]` | null | null | 0 | `[]` |
| `p-case` | `<!-- cite: Probe-Src#1 -->`（大小写不符） | 记了 `Probe-Src#1` | **stale** | true | `["chunk missing"]` | **null** | null | 1 | `["Body"]` |
| `p-dup` | 同一锚点写两次 | 同一项记两次 | stale | true | `["chunk hash drift"]` | **null** | null | **2** | `[]` |

纠错面（同一进程）：

```
POST /api/v1/knowledge/wiki/pages/typo-slug-xyz/corrections
  {"kind":"pin","reason":"audit probe: does an unknown slug get stored?","author":"t68"}
  → HTTP 200 {"id":1,"correction":{"slug":"typo-slug-xyz","kind":"pin",...}}
GET  /api/v1/knowledge/wiki/pages/typo-slug-xyz/corrections
  → HTTP 200 {"slug":"typo-slug-xyz","corrections":[{"kind":"pin","at":"2026-09-28T18:44:29.854579700+00:00"}]}
GET  /api/v1/knowledge/wiki/pages  → 没有 typo-slug-xyz
GET  /api/v1/knowledge/wiki/links  → READING links.nodes = p-body-dangle,p-drift,p-front-dangle（同样没有）
```

规模读数（best-of-5，同一进程同一台机，页面全部由 `PUT raw/wiki/<slug>` 播种）：

```
READING latency baseline        /api/v1/knowledge/wiki/pages   best-of-5 =  30.2 ms   （3 页）
READING latency baseline        /api/v1/knowledge/wiki/links   best-of-5 =  27.7 ms
READING latency 25 no-cite      /api/v1/knowledge/wiki/pages   best-of-5 =  73.7 ms   （28 页，+25）
READING latency 25 no-cite      /api/v1/knowledge/wiki/links   best-of-5 =  33.0 ms
READING latency 50 (25 cite)    /api/v1/knowledge/wiki/pages   best-of-5 = 157.8 ms   （53 页，再 +25 带 citations）
READING latency 50 (25 cite)    /api/v1/knowledge/wiki/links   best-of-5 =  47.7 ms
```

## 2 改进清单（按 严重度 × 影响面；每条：file:line · 复现 · 读数 · 为什么重要 · 可证伪判据 · owner）

### A1（高）**主体里的锚点从不参与新鲜度与未引用判定** ⇒ 同一处断链可读成 `fresh`，也可以读成 `stale`，差别只在 frontmatter 有没有记录

- **file:line**：`crates/daemon/src/wiki.rs:906`（`if !meta.citations.is_empty()` —— 新鲜度只看**记录下来的** citations）· `:3049`（`uncited_sections_of` 在 `meta.citations.is_empty()` 时直接 `return Vec::new()`）· 判据本体 `:534-567`（`citations_in` 会解析正文锚点，但只有构建路径 `:2302` 调它）
- **复现**：`powershell -File %TEMP%\t68-probe.ps1`（播种 `p-body-dangle` 与 `p-front-dangle`，正文逐字相同，只有 frontmatter `citations` 有无之差），然后 `GET /api/v1/knowledge/wiki/pages`
- **读数**：见 §1 表前两行 —— 正文同一处 `probe-src#99999` 断链：`freshness=fresh, citations=0, uncited_sections=[], stale_reasons=[]` vs `freshness=stale, stale_reasons=["chunk missing"], uncited_sections=["Body"]`
- **为什么重要**：面板徽标、`/wiki/pages` 消费者与（G8 的）注入线索卡都要靠 `freshness`/`cite_coverage` 判断"这页能不能信"。一个手改过、正文锚点已全部断掉、而 frontmatter 未同步的页**读成 fresh**；此时 `cite_coverage=null` 只表示"没有可用的构建期记录"（诚实），却不表示"有问题" ⇒ 两个字段合起来给出"健康且未知"的错觉。
- **可证伪的修复判据**：把同一句锚点在正文/ frontmatter 之间搬来搬去，**不得改变** `freshness` 与 `uncited_sections`（对 `p-body-dangle`/`p-front-dangle` 断言同一 `freshness`）；等价实现：`meta.citations` 为空但正文含可解析锚点时，`freshness` 返回 `unknown` 而非 `fresh`（"没有记录"≠"没有锚点"）。
- **owner**：wiki（生产者侧；读端点的三态归 t34 的同一属主）

### A2（高）`stale_since` 只在构建路径落地 ⇒ 两次构建之间"从什么时候开始失效"恒为空，规格 G3 的 `stale_persistence = 1.00` 不成立

- **file:line**：`wiki.rs:1020-1065`（`record_invalidation` 写 `stale_since`）· **唯一调用点 `:2032`，在 `execute_build_inner` 内** · `:2859-2863`（`pages()` 只在 stale 时回填 `record.stale_since`）
- **复现**：`t68-probe.ps1` 播种后 `GET /api/v1/knowledge/wiki/pages`（**不跑任何构建**）
- **读数**：三个 `stale=true` 的页（`p-drift`/`p-front-dangle`/`p-case`）**`stale_since` 全是 `null`**，而 `stale_reasons` 非空
- **为什么重要**：G3 的判据就是"stale 页中 `stale_since` 非空的比例 = 1.00"，而它现在只在"恰好跑过一次构建"之后才可能为真；人类/面板无法区分"刚失效"与"失效三个月"，而失效页正是要被修的对象。
- **可证伪的修复判据**：任一 `stale=true` 的响应里 `stale_since` 非空；用例：**不跑构建**，连读两次 `/wiki/pages`，第一次即非空且两次值相同（"首次观测"语义不变）。
- **owner**：wiki

### A3（中高）纠错写端点**不校验 slug 是否存在** ⇒ 打错一个字母的 `pin` 被 200 接受，成为任何页面都看不到的孤儿行

- **file:line**：`crates/daemon/src/api.rs:1132-1151`（解析 kind → 构造 `Correction{slug: 路径里的 slug}` → 直接写，**无存在性检查**）· `wiki.rs:1126-1154`（存储层同样只校验 reason/author）
- **复现**：`POST /api/v1/knowledge/wiki/pages/typo-slug-xyz/corrections -d '{"kind":"pin","reason":"...","author":"t68"}'`
- **读数**：见 §1 —— HTTP **200** `{"id":1,...}`；`GET` 该路径返回这一行；`/wiki/pages` 与 `/wiki/links` 都没有 `typo-slug-xyz`
- **为什么重要**：纠错回路是 G6 的**唯一人工闸门**。操作者以为"这页已冻结"，而下一次构建会照常重写它（`active_correction` 只对真实存在的 slug 生效）；失败是**静默的**（响应是 200 而不是 4xx）。
- **可证伪的修复判据**：对磁盘上不存在的 slug POST 纠错必须 4xx 并点名该 slug 不是页（若有意支持"预注册"，则 `/wiki/pages` 必须暴露"孤儿纠错"计数，让这条记录可达）；用例：POST 后 `select count(*) from wiki_corrections where slug='typo-slug-xyz'` 必须为 0。
- **owner**：wiki（api.rs 的接线由我落；若判定为集成面的契约变更，转 INT）

### A4（中）`corrections()` **静默丢弃** kind 无法解析的行 ⇒ 一条 `pin` 可以在读路径上无声蒸发

- **file:line**：`wiki.rs:1175-1186`（`.filter_map(... CorrectionKind::parse(&kind)?)` —— 解析失败的行被丢掉，无日志、无返回计数）· 后果面 `:1195-1200`（`active_correction` 建立在 `corrections()` 上，决定构建是否跳过该页）
- **复现（静态读数）**：`Select-String -Path crates/daemon/src/wiki.rs -Pattern 'CorrectionKind::parse'` → `:1180`，上下文是 `filter_map`
- **读数**：`corrections()` 的返回长度可以小于表里的行数，且调用方无法区分"没有纠错"与"有纠错但读不出来"
- **为什么重要**：与 A3 配成一对（一个在写入面静默，一个在读取面静默）。触达路径不是"恶意输入"，而是**版本回滚**：新 kind 写入后回滚代码 ⇒ 老代码把该行丢掉 ⇒ 冻结状态与 DB 不一致，而没有任何一方留痕。本代纪律明令"不许静默跳过"。
- **可证伪的修复判据**：`corrections()` 返回未解析计数（或至少 `tracing::warn!` 一行含 slug/kind）；用例：插一行 `kind='freeze'` 后，系统行为可观察（被报出或有日志），不得"既不报错也看不见"。
- **owner**：wiki

### A5（中）**同一事实存在两份记录**：`wiki_pages.content_hash` 写了没人读，真正生效的是 `wiki_page_hashes.hash`

- **file:line**：写 `wiki.rs:3430`（`sha256(file)`）→ `:3437/:3447/:3461`（INSERT + ON CONFLICT 写 `wiki_pages.content_hash`）· 读 `:3709`（`recorded_page_state` 只 `SELECT slug, hash FROM wiki_page_hashes`）· 另一条写路径 `:2375`（`set_page_hash` 写 `wiki_page_hashes`）
- **复现**：`Select-String -Path crates/daemon/src/wiki.rs -Pattern 'content_hash'`（`:3430/:3437/:3447/:3461` 只有 INSERT/UPDATE 列清单；`:3714/:3775` 是另一张表）
- **读数**：`wiki_pages.content_hash` 在 `wiki.rs` 内**只有写、没有 SELECT**；手改检测（`StaleReason::HandEdited`，`:941-947`）与 `recorded_coverage`（`:3795`）都只吃 `wiki_page_hashes`
- **为什么重要**：这是本代反复付学费的形状（"一条事实两处记录，只有一处被读"）。下一个人若从 `wiki_pages.content_hash` 取"页面字节的 hash"，会拿到一个不经 `set_page_hash`/`reconcile_page_hashes` 维护的值 —— 它由构建写、**不由手改与清理路径维护**，两条路径之间必然漂移。
- **可证伪的修复判据**：二者之一被删除（列或表），或新增一条断言"两处写路径产出的 hash 相等"（当前没有）。
- **owner**：wiki（若删除 `wiki_pages.content_hash` 列，迁移部分归 store owner）

### A6（中）`/wiki/pages` 的每页成本随页数线性增长，而面板对这个端点做 **3 秒轮询**

- **file:line**：`wiki.rs:2865`（`pages()` 每页调 `uncited_sections_of`）→ `:3055`（`verify_page`）→ `:640-662`（`chunk_index`：1 次 `list_documents` + 每来源 1 次 `document_chunks`）· `:2853`（每页 1 次 `active_correction` = 1 条 SQL）· 面板 `panel/src/views/Wiki.tsx:80`（`setInterval(refresh, 3000)`）
- **复现**：`t68-probe.ps1` 的 `Timed` 段（best-of-5，`GET /api/v1/knowledge/wiki/pages` 与 `/wiki/links` 对照）
- **读数**：见 §1 —— 3 页 30.2/27.7ms → 28 页 73.7/33.0ms → 53 页 157.8/47.7ms。即 `pages` 约 **+2.7ms/页**（带 citations 的批次 +3.4ms/页），`links` 约 +0.6ms/页；同规模下 `pages` 比 `links` 多花 **+110ms**（53 页），而两者消费的是同一份 `recorded_page_state` 与同一个 `link_graph`
- **为什么重要**：团队目标把消费面收敛为**单一 HTTP 面**，面板每 3 秒读它一次 ⇒ 这个端点的成本就是面板的常驻负载。1.7ms/页 的"每页固定税"里含每页一次纠错查询；带 citations 的页翻倍（`freshness` 与 `uncited_sections_of` 各跑一次 `chunk_index`，**同一页的同一次读取算两遍**）。500 页时按此趋势约 1.4 s/次轮询。
- **可证伪的修复判据**：给定预算（例如"N=500 页时 `/wiki/pages` ≤ 200ms"或"`pages` 与 `links` 的耗时差 ≤ 2×"），在同一基准脚本上可复现；实现侧把 `uncited_sections` 变成构建期记录值（DB 列），或让 `/wiki/pages` 复用一次 `chunk_index`（现在是每页×每来源各一次）。面板侧可先降频（归 panel owner）。
- **owner**：wiki（读端点）· panel owner（轮询频率）

### A7（中）闸门的词汇表**只有中文小标题**，而这条契约只存在于代码里

- **file:line**：`wiki.rs:573-576`（`sources_section` 只认 `title.trim().starts_with("来源")`）· `:34`（`NON_CONTENT_SECTIONS` 亦只有中文 + `相关` 前缀）· `:2306`（`problems` 非空即 `bail!` ⇒ 不落地）
- **复现（静态读数）**：`Select-String -Path crates/daemon/src/wiki.rs -Pattern 'starts_with\("来源"\)'` → `:575`；对照 `is_content_section` `:36-45`
- **读数**：一个写 `## Sources` 的页既不是"来源节"（⇒ `SourceSectionMissing`），也不是导航节（⇒ 它被当成内容节，还要为自己的列表带锚点），于是**永远无法通过闸门**
- **为什么重要**：闸门实际要求"要么按我们的中文小标题写，要么别落地"。对本地单用户平台这是可接受的设计，但它必须是**写进契约的**约束（写作者提示/规格），否则下一位写作者（或英文写作的模型）只会看到"页怎么都落不了地"，而找不到规则出处。
- **可证伪的修复判据**：`writer_prompt`/规格里逐字写明 `## 来源` 是契约词汇；或 `sources_section` 同时接受 `## Sources`（并加一条用例）。
- **owner**：wiki（writer_prompt 同属 wiki）

### A8（中低）面板显示与后端三态只对齐了一半：**原因字段一个都没显示**

- **file:line**：`panel/src/views/Wiki.tsx:185-206`（行内只有 `stale`/`edited`/`orphan`/`coverage`/`freshness==unknown` 徽标）· 对照读端点 `/wiki/pages` 已发布 `stale_reasons`/`stale_sources`/`stale_since`/`unknown_cause`（§1 读数）
- **复现**：`Select-String -Path panel/src/views/Wiki.tsx -Pattern 'stale_reasons|stale_since|unknown_cause|stale_sources'` → **0 命中**
- **读数**：后端 `stale_reasons=["source hash drift","chunk hash drift"]`、`stale_sources=["probe-src"]`、`unknown_cause` 可为 `"unknown_kb: …"`，面板全不显示
- **为什么重要**：三态一致性的另一半是**原因可达**。"源已更新"不说是哪个来源；`freshness=unknown` 不说是因为 KB 读不出来（`unknown_cause` 写着）还是从没构建过 —— 而这两件事要求读者做完全不同的动作。
- **可证伪的修复判据**：Wiki 行（悬停/展开皆可）至少显示一个 `stale_reasons` token 与 `stale_since`（stale 时），`freshness=unknown` 时显示 `unknown_cause`；e2e 断言 `stale_reasons[0]` 的字符串出现在 DOM 中。
- **owner**：wiki（面板 wiki 视图）

### A9（低）`citations` 不去重：同一锚点写两次 ⇒ `citations: 2`

- **file:line**：`wiki.rs:757-762`（每个解析出的锚点 push 一次，无 `(section,document,chunk_id)` 去重）· `:2882`（`citations: cited.len()` 直接发布）
- **复现**：`t68-probe2.ps1` 播种 `p-dup`（正文两次同一锚点 + frontmatter 两条相同项）
- **读数**：`"citations":2`，而 `uncited_sections: []`（内容没问题，只有计数被虚增）
- **为什么重要**：这个数被读成"有多少条证据"。一个 claim 有两份重复的锚点时，它虚增一倍，而任何读者都无法分辨。
- **可证伪的修复判据**：按 `(section,document,chunk_id)` 去重后计数；用例断言同一锚点写两次后 `citations == 1`。
- **owner**：wiki

### A10（低）冻结判定按 **`at` 字符串**排序，而 `at` 可由调用方给出

- **file:line**：`wiki.rs:1138-1142`（`at` 为空才用 `Utc::now()`，否则**原样入库**）· `:1161-1162`（`ORDER BY at DESC, id DESC`，字符串序）· `:1195-1200`（取最新一条 pin/release 决定冻结）
- **复现（静态 + 字符串示例）**：`"2026-09-28T08:00:00+08:00"`（= 00:00Z）按字符串 **大于** `"2026-09-28T01:00:00+00:00"`（01:00Z）⇒ 跨偏移量时"谁更新"的判定不是时间先后
- **读数**：HTTP 端点当前**不**暴露 `at`（`AddCorrectionRequest` 只有 kind/reason/author，`api.rs:1114-1122`），所以这条**不是外部可达**，而是 `pub fn add_correction` + 未来调用方的风险面
- **为什么重要**：冻结/解冻是行为闸门（构建会不会重写这页），排序错一次就是"该冻结的没冻结"，而且**不会报错**。
- **可证伪的修复判据**：`at` 入库前归一化为 UTC（或拒绝带非 UTC 偏移的输入），并加用例"两个不同偏移量代表同一时刻时按真实时刻排序"。
- **owner**：wiki

## 3 已证伪 / 已排除的假设（不进清单，避免下一位重查）

| # | 假设 | 结论与读数 |
| --- | --- | --- |
| R1 | 大小写差异能绕过闸门 | **否，fail-closed**。`p-case` 实测 `stale_reasons:["chunk missing"]`（`chunk_index` 按精确名匹配 ⇒ 解析不到 ⇒ 记 `DanglingCitation`/`ChunkMissing` 一侧） |
| R2 | 重新构建会抹掉手改证据（`reconcile_page_hashes` 重算在盘页的 hash） | **否**。`reconcile_page_hashes`（`:3537-3580`）只 `DELETE` **已消失**页的行，不重算；且 `set_page_hash` 只在 `Written` 路径调用（`:2375`），`Skipped` 路径不更新 ⇒ `HandEdited` 持续成立 |
| R3 | 重复锚点能绕过引用闸门 | **否**。闸门是"每内容节 ≥1 个可解析锚点"（`:767-775`），重复只影响计数（见 A9） |
| R4 | 正文里 `#0`/负数/超大 chunk_id 能成为绕过 | **否**。`citations_in`（`:554-560`）要求 `parse::<i64>` 成功且 `chunk_id > 0`，否则该锚点被丢弃 ⇒ 该节变成 `UncitedSection` ⇒ 闸门拦下（丢弃本身的副作用记入 A1 家族） |
| R5 | 部分 chunk 变化会被漏检 | **否**。两层：文档级 `source_hashes`（`:888-900`）+ 每锚点 `chunk_hash`（`:929-934`）；`p-drift` 实测同时报 `["source hash drift","chunk hash drift"]`。**唯一空洞是 frontmatter 没记录 citations 的页**（= A1） |
| R6 | `/wiki/pages` 与 `/wiki/links` 的度数会各说一套 | **否**（t51 已逐位断言 `degrees[slug]` 与同名页相等；本次读数未发现反例） |
| R7 | `uncited_sections` 会在 `cite_coverage==1.0` 时非空（RV-D-1 不变量） | 本次**未造出反例**：`cite_coverage` 只在 fresh 时引用（`recorded_coverage` `:3795`），而本批 stale 页的 coverage 都是 `null`。**但没有覆盖"构建后手改正文"这条路径**（见 §4 K-1） |

## 4 未验证猜想（明确非 finding；需要构建路径或 mock distiller 才能取证）

- **K-1**：「构建成功后，手改正文删掉一个锚点，`cite_coverage` 是否仍为 1.0 且 `uncited_sections` 非空」——这条会直接违反 RV-D-1 的不变量，但我**没有**造出实例：它需要一个真实构建行（`wiki_pages.build_id` 非空），而要跑构建得让 distiller 工作（真 agent 或 mock）。现有套件里 `rebuild_updates_backup_edited_skip_and_delete`（`wiki_pipeline.rs:429-484`）只断言构建侧"skipped + 手改存活"，**没有**读端点的三态断言 ⇒ 建议在 `wiki_pipeline.rs` 补一条（owner wiki）。
- **K-2**：`wiki_pipeline.rs` 的 15 个用例里，哪些覆盖 A1/A2 家族（正文锚点 vs frontmatter、`stale_since`）——我**没有**逐条对照需求审计覆盖质量（见 §5），只确认了手改用例不看读端点。
- **K-3**：并发纠错（两个 pin 同时写 / pin 与 release 竞态）只做了静态阅读（单条 INSERT + `at DESC, id DESC`），**没有**并发实跑；`id DESC` 的存在说明作者考虑过同 `at` 并列。
- **K-4**：A8 的"展开后也看不到原因"是静态 grep 结论，**没有**跑 e2e 确认（可能别处有悬停提示)。

## 5 未覆盖范围（第 19 条：说清这次没看什么）

1. **构建/蒸馏路径的全部 LLM 阶段**：`plan`（`:1923`）、`run_page`（`:2055`）与 `PageLanded` 各分支的实际行为、重试与超时；本单只在静态阅读里用到 `run_page:2306` 的 bail 语义。
2. **`crates/mock-agent/tests/wiki_pipeline.rs` 的覆盖质量**：只统计了规模（1539 行 / 15 用例）并读了手改与不变量两处，未逐用例对照需求。
3. **面板**：只审了 wiki 视图的三态显示与 `api.ts` 的类型面（且为静态阅读）；未跑 e2e，未审其它视图/样式/无障碍。
4. **注入面与 MCP 面**：`crates/memory` 的 `<wiki>` 块渲染（G8）仍未测 —— 历史登记，本单未触碰。
5. **store 侧**：`0022_wiki_gen2.sql` 的列语义只从读写点推断，未审 schema 约束/索引与该表的迁移历史。
6. **规模读数**：只在**单机空库 + 最多 53 页**上取，没有 500/5000 页、没有冷热缓存分离、没有并发轮询下的读数（A6 的预算判据需要这些才能定阈值）。
7. **真库**：全部读数来自临时 root；真守护进程（pid 79984）未启停、未写入、未调用会写 `recall_log` 的端点。

## 6 纪律与可复现性

- **只读**：仓库写入集合 = 本报告；`crates/`、`panel/`、`cli/` 一个字节未改。
- **临时资源**：root `%TEMP%\ruagent-t68-root`（已删）· 端口 8898 · PID **37172**（`Stop-Process -Id 37172` 后 `alive after stop: False`）· 探针脚本留在 `%TEMP%`（不入库）。
- **取证顺序**：每条 finding 的读数都先于结论；不能取证的进 §4。**已证伪的假设单列 §3**，因为它们与 finding 同样值钱（本代已确认过一次"把正确的当错误也是失真"）。
- **二进制**：读数取自 `scripts/cargo-team.ps1 build -p ruagent` 在 2026-09-29 02:43:11 重建的 `ruagent.exe`（当前字节），不是旧构建。
