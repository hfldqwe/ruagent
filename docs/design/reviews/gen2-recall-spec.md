# gen2 召回 / 检索规格（R-A / t1）

> 状态：**可执行规格**（规格单，未改任何代码）。产物：本文件。
> 作者：recall（检索与存储工程师，inScope 只写路径 §E）。
> 本单对象集：`crates/knowledge`（chunk/embed/fast/rrf/store）· `crates/store/src/fts.rs`（**只读**）· 只读活库 `~/.ruagent/data/ruagent.db?mode=ro` · 只读 `GET http://127.0.0.1:8787/api/v1/knowledge/search`。
> 本会话时间窗：**2026-09-27T21:28:58 → 22:22:07+08:00**。凡引用 closure 计划的旧值一律标 `[旧 2026-09-26]` 并写明来源行。
> 纪律回执：全程 0 次代码改动（`git status --porcelain -- crates panel` 为空）；未调用 `/api/v1/recall`（会写 `recall_log`）；`recall_log` 651→651 逐次核对未增长（A3/A4 边界各测一次）；真守护进程 pid 79984 未启停、未写它的库。

**本单的四条结论（先给结论，理由与读数在 A–E）**

1. **候选召回没问题，坏的是排序。** 15 条活库 gold 查询的答案文档 **15/15 都在 top-20 里**（A3），可 recall@1 只有 **0.4667** —— 而 **8/8 的 rank-1 错误都是"父/枢纽页压过具体页"**（A3 表）。这是一个可证伪、可定位的失败模式。
2. **RRF 不是终点，权重才是这一代最大的单点收益。** 只把语义腿权重调到 2:1（一个不依赖 gold 的结构性参数），在活库上 recall@1 **0.4667 → 0.7333**、MRR **0.6889 → 0.8167**（C1/C2 标定读数）。**同时必须披露代价**：recall@5 **1.0000 → 0.9333**。
3. **"分数"今天不是分数。** 一个响应里同时存在 4 种量纲（名次分 / 距离 / 余弦 / bm25），却共用 2 个字段名；UI 把余弦 `0.86` 与名次分 `0.016` 并排成 `m 0.86  k 0.016`（A7）。而且**同一个查询的 top-1 会随 `limit` 变化**（A4，13 条里 2 条）——`leg_k = limit.max(10)` 是一个隐含且会改排序的窗口。
4. **CJK 的缺口被量化了**：2 字汉字查询里 **63.20% 只能靠 LIKE 全表扫**（A8，实测 10.47 ms/查询）。**否决 SQLite `trigram` 方案**（实测 0/40，与 FTS5 文档一致：<3 unicode 字符永不匹配），采纳**汉字 bigram 影子索引**（实测 40/40，0.08 ms，**131×**）。

---

## A 基线表

每行 = 对象集 + 采样面 + 复现命令 + 读数 + 时间窗。**本会话实测**与 `[旧 2026-09-26]` closure 值分开标注；凡旧值先写"是否仍成立"。

### A1 `retrieval-quality` harness（本会话实测，hash embedder）

| 项 | 内容 |
| --- | --- |
| 对象集 | 测试内 `CORPUS`：**16 篇**合成文档（`crates/knowledge/tests/retrieval-quality.rs:85-150`）；hits = **chunk id**；gold = **document id**（两个不同对象集，唯一合法桥是 `doc_of`） |
| 采样面 | `QUERIES` 固定 **19 条**（版本 `t293-v2`，16 条可答 / 3 条无答案），`limit=5`；两轮（a/b）跑同一份仪器 |
| 复现命令 | `$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"; $env:CARGO_TARGET_DIR="$env:TEMP\ruagent-ra"; cargo test -p ruagent-knowledge --test retrieval-quality -- --nocapture` |
| 读数 | `3 passed; 0 failed`。`[t245/a] embedder=hash-embedder (deterministic, offline) docs=16 queries=19 answerable=16 fused recall@1=1.0000 recall@5=1.0000 mrr=1.0000 keyword_nonempty=18/19 semantic_nonempty=19 substring_rows=1 elapsed=838ms`；`[t293/a] token_or_prefix reachable=18 nonempty=17 floor_only=1 unexplained=0 · substring reachable=19 nonempty=18 floor_only=1 unexplained=0`；两轮 **byte-identical = yes**。JSON 落在 `$env:TEMP\ruagent-ra\retrieval-quality.json` |
| 时间窗 | 2026-09-27T21:53–21:56+08:00（编译 24m27s，测试 1.74s） |
| `[旧 2026-09-26]` | closure §1 行 42：fused **recall@1 0.6667 · recall@5 0.8667 · MRR 0.7500**（16 语料 / 18 查询 / 15 可答，hash embedder）；两条中文查询 `gold_rank=null`；三条无答案 top_score = 1/61。**是否仍成立：不成立，且方向反了** —— 三项现在都是 **1.0000**，查询集也从 18 条变成 19 条（t293 加了一条 cjk-substring、一条反例）。**这是一个已经饱和的仪器**（C8）。 |

### A2 harness 的真实模型分支（本会话实测，**ABSENT**）

| 项 | 内容 |
| --- | --- |
| 对象集 / 采样面 | 同 A1 |
| 复现命令 | `$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"; $env:CARGO_TARGET_DIR="$env:TEMP\ruagent-ra-b"; $env:RUAGENT_T245_REAL="1"; cargo test -p ruagent-knowledge --test retrieval-quality -- --nocapture` |
| 读数 | `embedder=ABSENT -- FastEmbedder::try_new failed (embedding failed: fastembed init: Failed to retrieve model file 'onnx/model.onnx'); this run used hash-embedder instead`；指标与 A1 逐位相同（1.0000/1.0000/1.0000）。测试仍 exit 0 |
| 时间窗 | 2026-09-27T21:56–21:57+08:00 |
| 判据 | **本工作区不存在真实模型 harness 读数。** 因此 A1 的 1.0/1.0/1.0 必须写成「hash-embedder 读数」。**对照**：活库 `knowledge_meta` = `fastembed:multilingual-e5-small`（A5）⇒ **活库是真模型表，harness 不是**，两者的数字不可互引。hash embedder 对 CJK 近乎无信息（`embed.rs:61-87` 按空白切词 + 只留 alnum ⇒ 一整句无空格中文 = 1 个词 = 1 个哈希桶），这一点必须在任何 CJK 结论旁标注。 |

### A3 活库冻结 gold 集（本会话实测，**真模型**，本代新增读数）

| 项 | 内容 |
| --- | --- |
| 对象集 | 活库 corpus：`documents` **934** / `chunks` **10765** / `chunk_sections` **8568**（A5）。hits = chunk id；**gold = document 名**（14 条 title-derived + 1 条 content-derived） |
| 采样面 | **15 条可答查询**，`limit=20`、`legs=true`；另测 **7 条无答案查询**（A9）。gold 名逐条 `SELECT COUNT(*) FROM documents WHERE name=?` 校验 = 1 |
| 复现命令 | `python %TEMP%\ra_gold_probe.py`（脚本 §F.2，经 `GET http://127.0.0.1:8787/api/v1/knowledge/search?q=<urlencoded>&limit=20&legs=true`） |
| 读数 | **recall@1 = 0.4667（7/15）· recall@5 = 1.0000 · recall@10 = 1.0000 · recall@20 = 1.0000 · MRR = 0.6889**；CJK 子集 n=13 recall@1 = **0.3846**；**misses = 0**（15/15 的 gold 都在 top-20 内）；`recall_log` 651 → 651（首尾各读一次） |
| 时间窗 | 2026-09-27T21:46:03.920+08:00（探针 `recall_log` 首读 21:46:03 / 尾读 21:47:xx） |

**8/8 的 rank-1 错误逐条（这是本规格最重要的一张表：失败模式具名）**

| 查询 | 实际 top-1（父/枢纽） | gold（具体页） | gold 名次 |
| --- | --- | --- | --- |
| `docker 常用命令` | `obsidian/notes/Docker-OQEurB-Docker` | `.../Docker-OQEurB-docker 常用命令-docker 常用命令` | 2 |
| `Springboot 热部署` | `obsidian/wiki/sources/S-019-Java学习笔记` | `.../Springboot-Springboot热部署-Springboot热部署` | 2 |
| `反射的访问权限问题` | `obsidian/notes/45v6HJ-常见问题-常见问题` | `.../常见问题-反射的访问权限问题-反射的访问权限问题` | 1 |
| `OJ在线判题系统` | `obsidian/notes/45v6HJ-项目学习-项目学习` | `.../OJ在线判题系统-OJ在线判题系统` | 1 |
| `docker exec进入容器并执行命令` | `obsidian/notes/Docker-OQEurB-Docker` | `.../docker exec进入容器并执行命令-...` | 1 |
| `@Param装饰器` | `obsidian/notes/45v6HJ-Java框架-模块-Mybatis-Mybatis` | `.../Mybatis-不使用@Param装饰器...` | 1 |
| `ChatGPT提示词` | `obsidian/notes/CNGceW/ChatGPT` | `.../CNGceW-ChatGPT提示词-ChatGPT提示词` | 2 |
| `kubernetes 滚动更新回滚` | `obsidian/wiki/sources/S-039-云计算学习笔记` | `wiki/kubernetes-troubleshooting` | 2 |

**判据（可证伪）**：若失败模式是"候选召回不足"，则 gold 应出现在 top-20 之外 —— 实测 **0/15**；若失败模式是"排序忽略了页面具体性"，则 8 条 rank-1 错误的 top-1 都应是"名字是 gold 名字前缀 / 是被 gold 文件链引用的索引页" —— 实测 **8/8** 符合。**本代不去重排序就达不到业界水平，而重排序的收益不需要新语料就能标定（C1/C2）。**

### A4 `limit` 会改排序（本会话实测，**新缺陷**）

| 项 | 内容 |
| --- | --- |
| 对象集 | 活库 corpus（同 A3）；top-1 文档名与 top-1 融合分 |
| 采样面 | **13 条查询 × limit ∈ {3,5,10,20,30}**（同一份活库、同一时刻、只读 GET） |
| 复现命令 | `python %TEMP%\ra_limit_probe.py`（§F.3） |
| 读数 | `distinct_top1 = 2`（即 top-1 变了）的有 **2/13**：`kubernetes 滚动更新回滚` 与 `ruagent memory`。**翻转点都在 limit=10 → 20 之间**：`kubernetes...` 0.016393 → 0.028814；`ruagent memory` 0.016393 → 0.027652，且 top-1 文档从 `OpenWhisk...数据库内容` 变成 `obsidian/wiki/claude-code-02-记忆与配置-CC-Memory记忆与CLAUDE-md`。**同时**：`limit=20` 与 `limit=30` 的 top-1 **与分数**对 13/13 条完全相同 |
| 时间窗 | 2026-09-27T21:47:54.472+08:00 |
| 机制（代码判据，非推断） | `crates/knowledge/src/store.rs:610` 与 `:659`：`let leg_k = limit.max(10) as usize;` —— 调用方的**分页参数变成了检索窗口**。窗口变宽 ⇒ 并集变大 ⇒ RRF 的 1/(60+rank) 相加项变多 ⇒ 排序与分数都变 |
| 判据 | 若排序是 `(query, hit)` 的性质，则任一 limit 的 top-1 必须相同 —— 实测 2/13 不同。**故"分数"今天不是 `(query, hit)` 的函数，而是 `(query, hit, limit)` 的函数。** |

### A5 活库 census（本会话实测，只读 `mode=ro`）

| 项 | 内容 |
| --- | --- |
| 对象集 | `~/.ruagent/data/ruagent.db`（真守护进程正在写的那一份，**只读打开**）+ `~/.ruagent/knowledge/` 磁盘 |
| 采样面 | 全表 `COUNT(*)`；`pragma table_info`；磁盘递归列文件数 |
| 复现命令 | `python %TEMP%\ra_probe1.py` / `ra_probe2.py`（§F.1） |
| 读数 | `documents` **934** · `chunks` **10765** · `chunk_sections` **8568** · `entities` **63** · `entity_edges` **67** · `memories` **163** · `recall_log` **651** · `chunk_revisions` **7** · `wiki_build_pages` **41** · `documents` 名以 `wiki/` 开头 **5**（其中 4 个内容页 + `wiki/index`，共 26 chunks）· `knowledge_meta` **1** 行 = `embedder → fastembed:multilingual-e5-small` · 磁盘 `<root>/knowledge` **934 个 `.md`**，`documents.source` 非空 **934/934** |
| `chunks` 列 | `[id, document_id, idx, content, span_start, span_end, section_id]` —— **没有 content_hash**（只有 `documents.content_hash` 存整篇文档的 sha256）。此事实已答 I-B（见 §D.4） |
| 时间窗 | 2026-09-27T21:30:13.799+08:00（`recall_log` 自身 ts 窗：`2026-09-16T13:05:24Z` → `2026-09-27T13:23:02Z`） |
| 与任务书现场值的对账 | 任务书写「documents=934 / wiki_pages=4 / entities=50 / recall_log=650 是 2026-09-27 的现场读数」。**同一天内已变**：`entities` 50 → **63**、`recall_log` 650 → **651**；`wiki_pages=4` 若指"4 个内容 wiki 页"则与今天的 5 个 `wiki/%` 文档（含 `wiki/index`）一致，**本规格按 4 内容页 + 1 索引页记**。`documents=934` 未变。 |

### A6 `recall_log.top_knowledge_score` 分布（本会话实测）

| 项 | 内容 |
| --- | --- |
| 对象集 | `recall_log` 全 **651** 行（`top_knowledge_score` 非空 651/651） |
| 采样面 | 全表分组；同时读 `COUNT(DISTINCT query)` 与 `source` 分布 |
| 复现命令 | `python %TEMP%\ra_probe2.py`（§F.1） |
| 读数 | **5 个取值**，逐位等于 RRF 名次和式：`0.01639344`（=1/61）×**338** · `0.03252247`（=1/61+1/62）×**287** · `0.03278688`（=2/61）×**20** · `0.03226646`（=1/61+1/63）×**3** · `0.03177806`（=1/61+1/65）×**3**；min 0.01639344 / max **0.03278688**。`COUNT(DISTINCT query)` = **23**；`source`：NULL **624** · probe 11 · t290-live 7 · t290-diag 4 · t311 2 · verify 1 · ui-chat 1 · switch-verify 1 |
| 时间窗 | 2026-09-27T21:30:13+08:00 |
| `[旧 2026-09-26]` | closure §1 行 29：**574 行只有 3 个取值**（`1/61`×295 · `1/61+1/62`×276 · `1/61+1/63`×3），**上界 = 2/61 = 0.0327869**，观测最大值 = 上界的 99.19% |
| 逐条复核 | **上界仍成立且更强**：观测最大值 **= 2/61 = 0.03278688**，即**恰好触到上界（100%）**，不是 99.19%。**「只有 3 个取值」不成立**：现在是 **5** 个 —— 新增的两个（`1/61+1/63`、`1/61+1/65`）**仍然是名次和**，来自 A4 那个更宽的腿窗口。**结论不变且更强**：名次分上界 = 2/61 ≈ 0.033，任何查询（含**无答案**查询）都能到 1/61 = 0.0164，见 A9。 |

### A7 「假相似度」：活读数 + 代码坐标（本会话实测）

| 面 | 值 | 量纲 | 代码坐标 |
| --- | --- | --- | --- |
| knowledge `score` / `score_kind` | `0.016393 … 0.032787` | **RRF 名次分**（越大越好，上界 2/61） | `crates/daemon/src/api.rs:3003`（`"score_kind": "rrf_rank"`） |
| knowledge `semantic_score` | `0.1244 … 0.4091` | **LanceDB 距离**（**越小越好**） | `api.rs:3006`；`store.rs:58-62` |
| knowledge `keyword_score` | `-4.75 … -22.16` | **bm25**（**越负越好**） | `api.rs:3008`；`store.rs:545-566` |
| memory `semantic_score` | **0.7555 … 0.9159**（均值 0.8615，n=651） | **余弦**（**越大越好**） | `api.rs:2581-2582`；`memembed.rs:41-104` |
| memory `score` / `score_kind` | 同 RRF | RRF 名次分 | `api.rs:2591`；`memembed.rs:310` |

| 项 | 内容 |
| --- | --- |
| 对象集 / 采样面 | `recall_log` 651 行的 `top_memory_score`（余弦）与 `top_knowledge_score`（名次分）；面板渲染点 |
| 复现命令 | `python %TEMP%\ra_memscore.py`（§F.1）+ 只读 `panel/src/views/Memory.tsx:367-384` |
| 读数 | 余弦桶 **0.8621 × 297 · 0.8666 × 287**（两项 = **584/651 = 89.7%**），min 0.7555 max 0.9159。面板把两者**并排**渲染成 `m {toFixed(2)}` 与 `{score: toFixed(3)}` ⇒ 实际显示 `m 0.86` 紧邻 `k 0.016`（或 `k 0.033`）。**用户能读出的结论是「记忆比知识相关 54 倍」** |
| 时间窗 | 2026-09-27T21:30:13+08:00（DB）+ 本会话读 `Memory.tsx` |
| `[旧 2026-09-26]` | closure §1 行 71：同一行并列 `m 0.86`（余弦）与 `k 0.03`（名次分）⇒ 诱导「记忆比知识相关 **26 倍**」 |
| 逐条复核 | **成立，且比旧描述更强**：不是"两种量纲并排"，而是**一个响应里 4 种量纲（名次分 / 距离 / 余弦 / bm25）共用 2 个字段名**（`score`、`semantic_score`），量纲只由行里的 `kind` 决定。旧值 26 倍/0.03 与今天 54 倍/0.016 的差别只是采样面（旧的是 574 行里的一个样本）；**性质未变**。 |

### A8 CJK 可达性普查 + 成本标定（本会话实测）

| 项 | 内容 |
| --- | --- |
| 对象集 | 活库 **10765** chunk 的**词表 V**（`ruagent_store::fts::terms` 规则逐字复刻）= **74353** 个不同 token；其中长度 ≥5 的汉字 token **30744** 个 |
| 采样面 | 从 1500 个随机长汉字 token（seed `20260927`）机械抽出的**2 字汉字子串 6922 个**（去重）；另抽 3 字子串 9397 个。成本面：从"只能靠子串"的集合里随机取 40 条 |
| 复现命令 | `python %TEMP%\ra_cjk_census.py`；`python %TEMP%\ra_bigram_calib.py`（§F.4/F.5） |
| 读数 | 2 字子串：**precision 可达 1113/6922 = 16.08%** · **+prefix 1434/6922 = 20.72%** · **只能靠子串（LIKE）4375/6922 = 63.20%**。3 字子串：9397 抽样里 **6607 = 70.31%** 只能靠子串。<br>成本（40 条只能靠子串的查询，全部约束为"某个真实 token 的子串"）：**LIKE 腿 40/40 命中，均值 10.47 ms/查询**（全表扫）；**FTS5 `trigram` 0/40 命中，0.10 ms**；**汉字 bigram 影子索引 40/40 命中，均值 0.08 ms**（建索引 2669 ms / 10765 chunk）。**131×** |
| 时间窗 | 2026-09-27T22:01:50+08:00（普查）/ 22:08:40+08:00（bigram） |
| **独立复算（跨人，同一读数）** | mem-core 于 **2026-09-27T22:23:50.723492+08:00** 在**同一台机器**上用**原生 SQLite（磁盘活库，只读）**复算同一件事：`COUNT(*) FROM chunks WHERE content LIKE '%# AutoHo%' ESCAPE '\'` 5 次 ⇒ **[6.78, 6.83, 6.61, 6.94, 6.80] ms，min 6.61 / median 6.80**；`... ORDER BY id LIMIT 51`（正是 §D.2 的 `limit+1` 探针）median **6.69**；**无命中 needle 的 median 6.63（不更便宜 ⇒ 每次 LIKE 都是全表扫，无索引可走）**；`chunks_fts` 行数 10765 = `chunks` 行数（触发器同步的旁证）。**与我的 10.47 ms 差约 1.5×**，两个读数都写出来：我的 10.47 ms 是 **Python 层**对 40 条查询取均值、含每次调用的解释器开销；他的 6.80 ms 是**原生 SQLite** 的 5 次中位数。**按 §7.118 记为"同一量级两个读数，不是矛盾"**，且**承重结论一致**：全表扫、无索引、成本随语料线性增长。**C4 的 target（p50 ≤ 1 ms）远低于两者**，故目标不受这个差异影响 |
| 活库腿阶段实测（旁证） | `茶` → `prefix` · `capital of Peru` → `substring` · `OA` / `xylophone zebra` → `empty` · `search_legs` → `prefix`（A4 探针同一次） |
| `[旧 2026-09-26]` | closure §1 行 34：「unicode61 无 CJK 分词：整段汉字 = 一个 term；**173 个 CJK run 里 118 个 ≥3 字**，用 2 字查询永远达不到」 |
| 逐条复核 | **成立并已量化**。旧值的对象集是"173 个 CJK run"（名字/实体那一档），**与本次的词表普查不是同一对象集，两个数不可互引**；本规格用 74353 token 词表 + 6922 个 2 字子串这个更宽、可复现的采样面。**关键新增**：`trigram` 方案被实测否决（0/40），与 SQLite 文档一致。**另一条重要副产物**：harness 里唯一那条 `cjk-substring`（`研磨度`）在**活库语料里不存在**（活库腿上 `empty`）——即 A1 的 `substring_rows=1` 是**夹具事实，不是活库事实**。 |

### A9 可答 / 无答案的可分性（本会话实测）

| 项 | 内容 |
| --- | --- |
| 对象集 | 活库 corpus（A3）；两条腿的原始分与融合分 |
| 采样面 | **15 条可答 + 7 条无答案**，`limit=20`、`legs=true`。无答案集：`capital of Peru` · `xylophone zebra` · `search_legs` · `如何用 Rust 写一个向量索引` · `best time to see migrating whales` · `photosynthesis light-dependent reactions` · `ruagent memory` |
| 复现命令 | `python %TEMP%\ra_gold_probe.py` + `python %TEMP%\ra_spread.py`（§F.2/F.6） |
| 读数（语义距离，越小越近） | 可答 top-1 ∈ **[0.1244, 0.2263]**；无答案 top-1 ∈ **[0.2274, 0.4091]** ⇒ 在 **0.227** 处完全分开，**但间隔只有 0.0011** |
| 读数（今天展示的融合分） | 可答 ∈ [0.028814, 0.032787]；无答案 ∈ [0.016393, **0.030214**] ⇒ **重叠，不可分** |
| 读数（查询内背景展开 `mean(top-k) − top1`） | 可答 ∈ [−0.0124, 0.1602]；无答案 ∈ [−0.0162, 0.0404] ⇒ **重叠，不可分（本方法被自己的读数否决）** |
| 时间窗 | 2026-09-27T21:46:03 → 22:00:17+08:00 |
| 判据 | 「今天展示的分数能否当相关性门槛」的判据是**无答案查询不能与可答查询混在同一个阈值带里** —— 实测**混在一起**（无答案最高 0.030214 > 可答最低 0.028814）。**这条判据今天必然失败**，与 A6 的"上界 2/61 且恰好触界"是同一件事的两面。**注意**：距离轴上那 0.0011 的间隔**不足以成为阈值**（见 C3 的明确禁止）。 |

### A10 关键字腿单独对 gold 的覆盖率（本会话实测，离线复刻）

| 项 | 内容 |
| --- | --- |
| 对象集 | 活库 10765 chunk；hits = chunk id；gold = document 名（同 A3 的 15 条） |
| 采样面 | 离线内存 FTS5 复刻产品构造（`unicode61` + `terms/match_all/match_any_prefix/like_patterns` 逐字复刻），`leg_k=20`；对照 `trigram` 与 `precision→trigram` 混合 |
| 复现命令 | `python %TEMP%\ra_cjk_calib.py`（§F.7） |
| 读数 | 产品关键字腿：**gold 落在 top-20 = 15/15**，doc-rank0 **3/15**，MRR **0.5300**；`trigram` 腿 15/15 · 3/15 · 0.5444；混合 15/15 · 3/15 · **0.5500**。16 条探针的腿阶段分布：**precision 13 / prefix 2 / empty 1** |
| 时间窗 | 2026-09-27T21:53:58+08:00 |
| 判据 | **两条腿各自都能找到 gold（15/15、15/15），而融合后的 recall@1 只有 7/15** ⇒ 损失发生在**融合这一步**，不在任何一条腿里。这条判据同时把"先补 CJK 分词就能提升 recall@1"这个假设**证伪**（混合方案 leg 级只有 +0.02 MRR，见 C4 的目标定位）。 |

### A11 四条已知缺陷的逐条复核

| # | 缺陷（任务书原文） | 复核 | 证据 |
| --- | --- | --- | --- |
| 1 | `score_kind=rrf_rank` 的名次分被当相似度显示（0.88 与 0.02 并列 ⇒「记忆比知识相关 26 倍」） | **成立且更强**：不是 2 种量纲并排，而是 **4 种量纲（名次分 / 距离 / 余弦 / bm25）共用 2 个字段名**；活读数 `m 0.86` vs `k 0.016` | **A7** · `api.rs:3003,3006,3008,2581` · `memembed.rs:41-104` · `Memory.tsx:367-384` |
| 2 | 查询 `ruagent memory` 的 top-1 是一条 OpenWhisk 笔记（semantic 0.34）⇒ 没有真实 gold 集就无法判「检索好不好」 | **前半成立 / 后半已修**：`ruagent memory` 在 **`limit=5..10` 时 top-1 确实是 OpenWhisk 笔记**（`semantic_score 0.3437`，2026-09-27T21:30:33 复现）；但 **`limit=20` 时 top-1 变成 `claude-code-02-记忆与配置`**（0.3720）⇒ **这条反例本身依赖分页参数**（A4）。「没有真实 gold」**成立**，已由 A3 的冻结 gold 集补上（**并标注它是 title-derived 的弱代理**，见 C7/D3） | **A3 · A4 · A9** |
| 3 | unicode61 无 CJK 分词（整段汉字一个 term），2 字中文查询只能靠 LIKE 段兜底 | **成立且已量化**：**63.20%** 的 2 字汉字查询只能靠子串腿；LIKE 全表扫 **10.47 ms/查询**（10765 chunk）。**并新增两条**：`trigram` 对 <3 字符**永不匹配**（0/40，与 FTS5 文档一致）；harness 的 `研磨度` 在活库语料里**不存在** | **A8 · A10** |
| 4 | 融合只用 RRF：无语义/关键词权重、无重排、无多样性、无新近性先验、无查询理解 | **成立**：`rrf(&[ann_ids, fts_ids], 60)`，两处调用（`store.rs:614`、`:663`），无权重、无重排、无多样性、无时间先验、无查询理解；`FusionKind` 类型**不存在**。**并新增一条任务书没写的缺陷**：`leg_k = limit.max(10)` 让分页参数改排序（**A4**） | **A4 · A7** · `store.rs:609-680` · `rrf.rs:1-16` |

---

## B 业界最领先做法对照

每条 = 一手来源（URL）+ 采纳 / 不采纳 + 理由 + 代价。**"采纳"只表示写进 §C 的目标表或 §E 的实现清单**；未采纳的也留在表里，因为它们解释了为什么不做。

### B1 RRF 的出处与它的边界

- 来源：Cormack, Clarke, Büttcher, *Reciprocal Rank Fusion outperforms Condorcet and individual Rank Learning Methods*, SIGIR 2009 — <https://plg.uwaterloo.ca/~gvcormac/cormacksigir09-rrf.pdf> · DOI <https://dl.acm.org/doi/10.1145/1571941.1572114>
- 一手内容：RRF 用**名次**而非分数融合多个 IR 系统的排名，稳定优于任一单系统，也优于 Condorcet Fuse。它是**无监督、无需标定**的方法。
- **采纳**：把 RRF 继续作为**基线与兜底**（它今天已经是），并保留"零 LLM、查询期无训练"的性质（`design §6.6 #3`）。
- **不采纳"RRF 就是终点"**：理由是我们有**两条质量差异很大的腿**（A10：语义腿单独 top-1 命中 9/15、关键字腿 3/15；A9 可答/无答案在语义轴上可分、在名次轴上不可分），而 RRF 的卖点正是"对分数尺度不敏感"⇒ 它**同时**丢掉了尺度里的信息。实测 w_sem:w_kw=2:1 把活库 recall@1 从 0.4667 抬到 **0.7333**（C1）。
- **代价**：多一个需要标定的超参数（必须配冻结 gold 集，否则就是拍脑袋）；且**有权重就有 trade-off** —— 实测 recall@5 **1.0000 → 0.9333**（C1 明确披露）；权重与窗口必须进 `recall_log` 的 scoring_version，否则新旧分数不可比。

### B2 把"腿窗口"当作一等参数

- 来源：Elasticsearch Reference, *Reciprocal rank fusion* — <https://www.elastic.co/docs/reference/elasticsearch/rest-apis/reciprocal-rank-fusion>
- 一手内容：RRF retriever 暴露 `rank_constant`（默认 60）与 **`rank_window_size`** —— 即"每条腿取多少候选"是**调用方可见、可与 `size` 分离**的参数。
- **采纳**：把今天隐含的 `leg_k = limit.max(10)` 改成**具名常量** `LEG_WINDOW`，并让它在响应里可读（`SearchEvidence.leg_window`）。
- 理由：A4 实测分页参数会改 top-1（2/13），而 A4 同时证明 `limit=20 ≡ limit=30`（13/13 top-1 与分数全同）⇒ **把窗口钉成不随 limit 变的常量，就能让排序重新成为 `(query, hit)` 的函数**，且代价可测。
- **代价**：窗口 ≥ 消费者会问的最大 limit ⇒ 每条腿固定多取候选（更多 RRF 项 + 更多 SQLite hydrate）。今天 `limit` 最大 30（`recall` 的 `search_n = min(3·top_n, 30)`），窗口取 60 是本规格的选择（§C5 标定），**必须把它写进 `recall_log`，否则跨越这次改动的前后读数会被误当同一读数**。

### B3 SQLite FTS5 `trigram`：能力与**硬边界**

- 来源：SQLite, *FTS5 Extension* §4.3.4 The Trigram Tokenizer — <https://sqlite.org/fts5.html>
- 一手内容（原文）：*"Substrings consisting of fewer than 3 unicode characters do not match any rows when used with a full-text query. If a LIKE or GLOB pattern does not contain at least one sequence of non-wildcard unicode characters, FTS5 falls back to a linear scan of the entire table."*
- **采纳**：把这条**当判据用**（"2 字汉字查询不能指望 trigram"），并把它写进规格以免后人重试。
- **不采纳 `tokenize='trigram'` 作为 CJK 修复**：本会话实测在活库 10765 chunk 上对 40 条"只能靠子串"的 2 字查询命中 **0/40**（A8），与文档一致。3 字以上它有效，但**我们最疼的正是 2 字**（63.20% 只能靠子串）。
- **代价**：不能靠一行 `tokenize=` 改完 ⇒ 必须自己做影子索引（§E4 的 schema + 触发器 + 回填），多一张表和一次全量回填。

### B4 汉字 bigram 分词（Lucene / Elasticsearch 的一手实现）

- 来源：Lucene `CJKBigramFilter` — <https://lucene.apache.org/core/10_0_0/analysis/common/org/apache/lucene/analysis/cjk/CJKBigramFilter.html> · Elasticsearch, *CJK bigram token filter* — <https://www.elastic.co/docs/reference/text-analysis/analysis-cjk-bigram-tokenfilter>
- 一手内容：把 CJK term 拆成 bigram；`output_unigrams=true` 时"两侧都没有相邻字符的 CJK 字符"按 unigram 输出；非 CJK 输入原样透传。
- **采纳**：`chunks.grams` = 每个汉字 run 展成**重叠 2 字 bigram**、长度 1 的 run 保留为 unigram、非汉字原样（§E4 的 `fts::han_bigrams`）。
- 理由：A8 的 63.20% 是**可复现的量化缺口**；bigram 影子索引实测 **40/40 命中、0.08 ms**，而对今日的 LIKE 是 40/40、**10.47 ms**（131×）。它同时把"2 字查询"从"全表扫"变成"索引查"，这是**规模**问题（10765 chunk 已 10 ms，百万级即 ~1 s）。
- **代价**：CJK 文本的索引体积大致翻倍（每个汉字进 2 个 bigram）；一次 10765 行回填（实测建索引 2669 ms，**产品路径会含写盘，必须分批且可续跑**）；**且 bigram 腿也要一个融合权重**，否则会把 A10 里"precision 13/16"的现状搅乱。**硬约束**：`fts::terms/match_all/match_any_prefix/like_patterns` 这四个函数**一个字节都不能改**，因为 `crates/graph` 把它们的 loose 复刻当唯一分词判据复用（R-C 规格 DEP-5）—— 新能力只能**新增**函数。

### B5 混合融合：RRF vs 归一化凸组合（含一个**与本文不一致**的一手结论）

- 来源：Bruch, Gai, Ingber, *An Analysis of Fusion Functions for Hybrid Retrieval*, ACM TOIS 2023 — <https://dl.acm.org/doi/10.1145/3596512>（结论原文："Results consistently show that convex combination with theoretical minimum-maximum normalization outperforms RRF across both settings."）· 工程侧一手说明：OpenSearch, *Building effective hybrid search* — <https://opensearch.org/blog/building-effective-hybrid-search-in-opensearch-techniques-and-best-practices/>（明确写 min-max 归一化后加权平均；并写 RRF "requires no pretraining, weight tuning, or knowledge of score ranges … robust default when labeled data for calibration is unavailable"）
- **采纳**：① 必须**同时**实现"加权 RRF"与"归一化凸组合"两条，并在冻结 gold 集上报同一组指标（否则无法证伪"我们选对了"）；② 必须把**标定所需的标注数据缺失**当成一个待解决项（这正是 C8 与 §E3 的 `query_eval_*` 表）。
- **不采纳（本轮）用凸组合替换 RRF**：**我自己在活库上的读数与 B5 的结论相反** —— 在候选窗口内 min-max 归一化凸组合 recall@1 **0.4667** / MRR **0.6800**，不优于 RRF（0.4667 / 0.6889），而加权 RRF 是 **0.7333 / 0.8167**（C1/C2）。
- **理由（为什么这次不听论文）**：该文的归一化窗口是**检索到的候选集**；在我们的形态里那是**每条查询 20 条、且大量近似并列**（A3 的融合分 0.0308/0.0302/0.0298 … 密集），min-max 在这么窄且布满并列的窗口上归一化会**放大噪声**而不是恢复信号。这是一个**可证伪的采样面差异**，不是"论文错了"。
- **代价**：保留 RRF ⇒ 展示/阈值需要一个**独立**的标定分字段（§D 的 `relevance`），于是同一个命中会有**两个数**（名次分 + 标定分），必须靠 `score_kind` 强制区分，否则就是把 A7 的缺陷换个名字再来一遍。

### B6 Contextual Retrieval 的**协议形状**（不采纳它的摄取成本）

- 来源：Anthropic, *Introducing Contextual Retrieval* — <https://www.anthropic.com/engineering/contextual-retrieval>
- 一手读数：只加 contextual embeddings，top-20 检索失败率 5.7% → 3.7%（**−35%**）；再加 contextual BM25 → 2.9%（**−49%**）；再加 reranking → 1.9%（**−67%**）。
- **采纳**：① "**top-k 失败率**"作为一种可对外解释的指标形状；② "**字面 + 语义 + 重排**"的投入排序 —— **先做融合与重排，再考虑更贵的嵌入**；③ 在摄取侧给 chunk 补上下文是一种**已被一手读数支持**的后备手段。
- **不采纳（本轮）**：逐 chunk 调 LLM 生成上下文。理由：① 成本是每 chunk 一次 LLM 调用 × **10765** 个 chunk，而本设计的原则是**查询期零 LLM**（摄取期可以贵，但需要单独一轮的预算与复现）；② 它报告的指标是"top-20 失败率"，**需要一个人工判定相关性的 gold 集**才能算 —— 那个集合今天**不存在**（A3 用的是 title-derived 弱代理），先建集合再花大钱。
- **代价**：不做上下文增强，A3 那种"枢纽页压过具体页"的 8/8 失败就**只能靠融合/重排来治**，不能指望摄取侧顺手解决。

### B7 零样本异质基准：分层切片 + 不许平均掉异质性

- 来源：Thakur et al., *BEIR* — <https://arxiv.org/abs/2104.08663>；中文侧：Xiao et al., *C-Pack / C-MTEB*（6 任务 35 数据集）— <https://arxiv.org/abs/2309.07597>
- **采纳**：① 指标必须**按层报告**（本规格的层 = `exact-ascii` / `cjk-2char` / `cjk-run` / `mixed` / `punctuated` / `no-answer`），**并且无答案查询留在分母里**（`retrieval-quality.rs:280-295` 已经这么做了，保留）；② CJK 层要单独报一个数 —— A3 实测 CJK 子集 recall@1 **0.3846** vs 全体 **0.4667**，**差 2 倍**，平均掉就看不见了。
- **代价**：需要一个**冻结**的 gold 集并标注每一层的查询来源；分层后每层样本变小（CJK 层 n=13），**必须同时报 n**，否则会拿小样本的波动当趋势。**另**：`retrieval-quality.rs` 的 16 篇合成语料**不足以**当 BEIR 式的异质基准（它只有一个作者、一种文体、19 条查询），它的值在于**确定性回归**，不在于质量（C8）。

### B8 官方评测协议：nDCG@10 为主、MRR/recall@k 为辅、分级相关

- 来源：TREC Deep Learning Track 综述 — <https://arxiv.org/html/2507.08191v1>；资源论文 — <https://www.microsoft.com/en-us/research/wp-content/uploads/2021/04/sigir2021-resource-trecdl-craswell.pdf>
- 一手内容：passage / document 任务官方指标是 **nDCG@10**（document 另有 nDCG@100）；qrels 是**深判 + 分级相关**（pooled deep judging）。
- **采纳**：§C 的 headline 指标定为 **nDCG@10**，同时保留 **recall@1/@5/@20 与 MRR** 作为可解释的辅助（今天的 gold 是二值，nDCG 会退化成 recall@1 的单调变换，所以**必须先把 gold 升到分级相关**才算真的采纳）。
- **不采纳（本轮）"用 title-derived gold 冒充 qrels"**：那样 headline 会系统性偏向字面/标题匹配，且分级 nDCG 会退化成 MRR 的单调变体。
- **代价**：产出分级 gold 是**人工工作**。本轮的折中已写进 C9 并被**明确标注**：先给一个**二值、每查询 1 个 gold** 的退化 nDCG@10（baseline 0.7682 → 标定 0.8421，可直接算、可证伪），**它不许被当成分级 nDCG**；分级版的 target 留在 §G2，等 §E3 的人工集到位。C1/C2 的目标也先落在**二值 gold 上已标定过**的 recall@1 / MRR，不越级。

### B9 多样性 / 冗余抑制（MMR）：**这一条本轮标定不了**

- 来源：Carbonell & Goldstein, *The Use of MMR, Diversity-Based Reranking* — <https://kilthub.cmu.edu/articles/journal_contribution/The_Use_of_MMR_and_Diversity-Based_Reranking_in_Document_Reranking_and_Summarization/6610814>
- **采纳（方向）**：A3 的 8/8 失败是"枢纽页与它的具体页同时进 top-k 且枢纽压前" ⇒ **文档级多样性/具体性先验**是对症的。
- **不采纳（本轮实现）**：MMR 的 λ 需要"**每条查询有多个相关文档**"才能标定，而我们的冻结 gold 集**每条查询恰好一个 gold** ⇒ 任何 λ 的读数都是**同一份数据上的自我循环**，违反"不许定一个没量过的高度"。
- **代价**：本轮只能定一个**结构性诊断指标**（C7：名字含查询原文的 gold 必须 rank-1；实测基线 8/14），并且**明确标注它是循环的**；真正的多样性目标推迟到下一轮的多 gold 集。

### B10 E5 的 query/passage 前缀 —— 为什么"距离"没有绝对零点

- 来源：Wang et al., *Text Embeddings by Weakly-Supervised Contrastive Pre-training* (E5) — <https://arxiv.org/abs/2212.03533>；实现：本仓 `crates/knowledge/src/fast.rs:44,60`（`passage: ` / `query: ` 前缀）+ fastembed 6.0.3 的 L2 归一化（`fastembed-6.0.3/src/text_embedding/output.rs:49` 的 `.map(normalize)`、`src/common.rs:230 pub fn normalize`）
- 一手读数（本会话实测）：把**某个 chunk 自己的原文**当查询，它的 `semantic_score` 是 **0.0618 / 0.0886 / 0.0725 / 0.1204**（4 条样本，2026-09-27T21:59:36+08:00），**不是 0**。
- **采纳**：① 保留前缀不对称（它是模型正确用法）；② 把"距离"这个名字写进响应（`semantic_score_kind`），**不假装它是相似度**；③ 任何阈值必须**按 embedder 标定**，并在 `migrate_embedder` 换模型时**作废**。
- **不采纳**：把 `1 − d/2` 当绝对余弦用。理由：E5 的 `query:`/`passage:` 前缀让"同文本"的 query 向量与 passage 向量**本就不同**，实测自查询距离 ≥0.0618 ⇒ 尺度上没有可靠零点；`d = 2−2cos` 与 `d = ‖a−b‖₂` 两种读法在**排序上等价**（单调），在**显示上不等价**，所以显示值必须标定（§C6 只要求"标注量纲"，不要求先确定映射）。
- **代价**：每次换模型都要重跑标定；`recall_log` 里历史行的 `top_knowledge_score` 与换模型后的值**不可比**（今天已经存在这个坑：`knowledge_meta` 换过一次模型，`fast.rs:9-11` 记着 bge-small 时代中文近邻近乎随机）。

### B11 注入侧的顺序敏感性（把"检索→注入"闭环的代价计入检索）

- 来源：Liu et al., *Lost in the Middle: How Language Models Use Long Contexts* — <https://arxiv.org/abs/2307.03172>
- **采纳**：召回的输出**顺序本身**是下游质量的一部分 ⇒ §C 的指标必须包含**位置**指标（MRR / recall@1），不能只看 recall@k；§D 的 `relevance` 必须能被注入侧用来**重排**（而不是只用来展示）。
- **不采纳**：本轮不规定注入块的排列策略（那是 I-B 的 inScope，`crates/memory/src/inject.rs` 的 `tag_rank`）；本规格只保证"给出可重排的证据"。
- **代价**：如果 I-A 只给排名不给可比较的相关性，I-B 只能维持"按 tag 的固定顺序"，中心位置的收益拿不到。

---

## C 目标表

每行 = **metric / baseline / target / 复现命令 / 一条标定读数**。标定读数一律来自本会话实测；**凡我自己没量到的高度不写**。

### C1 活库 recall@1（文档级，冻结 15 条 gold）

| 项 | 内容 |
| --- | --- |
| metric | `recall@1`（文档级；gold = 名字逐条校验存在的 document） |
| baseline | **0.4667**（7/15）—— A3，2026-09-27T21:46:03+08:00，`limit=20` |
| target | **≥ 0.70**（下一轮在人工分级 gold 上重标） |
| 复现命令 | `python %TEMP%\ra_gold_probe.py`（§F.2）→ 新实现后同一脚本 |
| **标定读数** | 在活库融合 top-20 **内部**重排：`w_sem:w_kw = 2:1` 的加权 RRF ⇒ recall@1 = **0.7333**；且 **{2:1, 3:1, 4:1} 都是 0.7333**（平台区，不是一点运气）。对照：`1:1`（今天）0.4667 · `1:2` 0.3333 · 只语义 0.6000 · 只关键字 0.2000 · min-max 凸组合 0.4667 · leg-agreement 加成 0.4667 · min-rank 0.2667 |
| **代价（必须同时披露）** | 同一标定下 recall@5 **1.0000 → 0.9333**、nDCG@10 **0.7682 → 0.8421**。**代价定位到一条查询**：`kubernetes 滚动更新回滚` 的 gold 在 w2_1 下从 rank 2 **掉到 rank 11**（top-5 与 top-10 同时丢）。而它正是 15 条里**唯一一条 content-derived gold**（`wiki/kubernetes-troubleshooting`，文档名里不含查询原文）—— 即"名字匹配得利、内容匹配受损"，这与 C7（title-derived gold 的循环性）指向同一件事。`w=1:2` 与 `w=0:1` 显著更差 ⇒ 权重方向是"语义更重要"，但**不能**把关键字腿丢掉（丢掉就 0.6000） |
| **权重选择的边界** | 平台区 `{2,3,4}:1` 都 0.7333，故这一版取 **2:1**（平台区下界，离"只语义"的 0.6000 最远）。**这不是最优值**：真最优需要多 gold、分级相关的冻结集（C9/§G2）；2:1 的地位是"**已量到能达标**"，不是"已证明最优" |
| 标定工具 | `python %TEMP%\ra_rerank_calib.py`（§F.8），作用域：**在活库 top-20 内重排**；本次 gold 15/15 都在 top-20 内 ⇒ recall@20 不受该重排影响，**recall@1 的变化只能归因于排序** |

### C2 活库 MRR（同一 gold 集）

| 项 | 内容 |
| --- | --- |
| metric | `MRR`（document 级） |
| baseline | **0.6889**（A3） |
| target | **≥ 0.80** |
| 复现命令 | 同 C1 |
| **标定读数** | `w=2:1` ⇒ MRR **0.8167**（`w=3:1`/`4:1` = 0.8111，同为平台区）；对照 0.6889（今天）· 0.7151（只语义）· 0.4811（只关键字）· 0.6800（min-max）· 0.5911（min-rank） |

### C3 展示分必须与"无答案"分离（**阈值点明确禁止写死**）

| 项 | 内容 |
| --- | --- |
| metric | 在一个**≥60 条查询、其中 ≥20 条无答案**的冻结集上：无答案查询的 `relevance`（或暴露的 `semantic_distance`）落在可答查询 90 分位之外的**比例** |
| baseline | **不可分**：今天展示的 RRF 分，无答案最高 **0.030214** > 可答最低 **0.028814**（A9）。判据必然失败 |
| target | **≥ 0.85**（无答案查询里至少 85% 被抬到可答带之外） |
| 复现命令 | `python %TEMP%\ra_gold_probe.py` + `ra_spread.py`（§F.2/F.6） |
| **标定读数** | 在 **n=22**（15 可答 / 7 无答案）上，语义距离把两组**完全分开**：可答 top-1 ≤ **0.2263**，无答案 top-1 ≥ **0.2274**。**同时报告间隔只有 0.0011、n 只有 22、单一语料** |
| **明确禁止** | **不得把 0.227 写成阈值常量。** 这条标定读数的作用是**证明坐标轴上有信号**，不是**确定切点**。切点必须在 ≥60 条冻结集上重标；在重标完成前，`relevance` 只能当**展示/排序**用，禁止当**过/不过**的闸门（任何 `min_score` 默认值改动都必须先有 C3 的新读数） |
| 反面读数（一并登记） | 「查询内背景展开 `mean(top-k) − top1`」这条候选方法**被自己的读数否决**（可答/无答案区间重叠，A9）—— 写在这里免得下一轮重试 |

### C4 CJK 2 字查询：从全表扫变成索引查

| 项 | 内容 |
| --- | --- |
| metric | (a) 只能靠子串的 2 字汉字查询里，`索引腿` 能命中的比例；(b) 这些查询的 p50 延迟；(c) 热路径上是否还有全表 LIKE |
| baseline | **63.20%** 的 2 字汉字查询只能靠子串（A8）；全表 LIKE 成本有**两个同量级读数**：**10.47 ms/查询**（我的采样面：Python 层、40 条均值）与 **6.80 ms**（mem-core 独立复算：原生 SQLite、5 次中位数；2026-09-27T22:23:50+08:00）。**承重事实**：无索引全表扫，且**无命中也不更便宜**（median 6.63） |
| target | (a) **≥ 0.95**；(b) **p50 ≤ 1 ms**；(c) 2 字汉字查询**不再走**全表 LIKE |
| 复现命令 | `python %TEMP%\ra_cjk_census.py`；`python %TEMP%\ra_bigram_calib.py`（§F.4/F.5） |
| **标定读数** | 汉字 bigram 影子索引：**40/40 命中、均值 0.08 ms**（**131×** 快于 LIKE），建索引 2669 ms / 10765 chunk。**对照（被否决的方案）**：SQLite `trigram` **0/40**（与 FTS5 文档一致） |
| 代价 | 一次 10765 行回填（产品路径含写盘，需分批可续跑）+ CJK 索引体积约翻倍 + bigram 腿要一个融合权重（不许改变四条 `fts::*` 原名函数的语义，`crates/graph` 复用它们） |

### C5 排序不得依赖分页参数

| 项 | 内容 |
| --- | --- |
| metric | 冻结查询集上：`limit ∈ {5,10,20,30}` 四条读数的 **top-1 文档名**与 **top-1 分数**是否逐条相同 |
| baseline | **2/13 条不同**（A4）；翻转点位于 `limit=10 → 20` |
| target | **13/13（100%）相同** |
| 复现命令 | `python %TEMP%\ra_limit_probe.py`（§F.3） |
| **标定读数** | **`limit=20` 与 `limit=30` 对 13/13 条返回相同的 top-1 与相同的分数**（2026-09-27T21:47:54）⇒ 把腿窗口钉成**不随 limit 变的常量**（`LEG_WINDOW ≥ 30`，本规格取 **60**）即可达到"100% 相同"；`limit.max(10)` 正是 10↔20 翻转的机制 |
| 代价（行为变更，非编译破坏） | 窗口从 `max(limit,10)` 变 60 ⇒ **rank 可能整体变化**（实测同一次探针里 2/13 条的 top-1 已经因此改变）；跨越这次改动的 `top_knowledge_score` **新旧不可比** ⇒ §E 要求写 `scoring_version` 与 `knowledge_leg_window` |

### C6 每个分数都必须自带量纲

| 项 | 内容 |
| --- | --- |
| metric | (a) 响应里每一个数值型分数键是否存在同级的 `*_kind`（可机器判定）；(b) 是否存在"渲染器把两种量纲并排而没有 kind 可读"的路径 |
| baseline | (a) **不成立**：knowledge hit 的 `semantic_score` / `keyword_score` **没有** kind；只有行级的 `score_kind: "rrf_rank"`（`api.rs:3003`）。(b) **成立**：`Memory.tsx:367-384` 并排 `m 0.86`（余弦）与 `k 0.016`（名次分） |
| target | (a) **5/5 分数键都有 kind**（字段集是有限的，见下）；(b) **0 条渲染路径能并排两种量纲而不带 kind** |
| 复现命令 | `python %TEMP%\ra_scorekind_probe.py`（§F.9）+ `cargo test -p ruagent-daemon api` |
| **标定读数** | 字段集是**有限且今天就可枚举**的：knowledge hit 5 个分数键（`score` / `semantic_score` / `keyword_score` + 新增 2）、memory hit 3 个（`score` / `semantic_score` / `keyword_score`）。四个现存量纲：`rrf_rank`（两种 kind 共用）· **距离**（knowledge semantic，越小越好）· **余弦**（memory semantic，越大越好）· **bm25**（越负越好）—— `api.rs:3003,3006,3008,2581-2582` + `memembed.rs:41-104`。**注意**（I-B 已冻结）：`"rrf_rank"` 是**字面量**，不许改写成别的名字；新量纲只能用**新键**表达 |

### C7 结构性诊断（**循环，只作诊断，不作 headline**）

| 项 | 内容 |
| --- | --- |
| metric | 冻结 gold 里"**文档名含查询原文**"的子集，其 rank-1 命中率 |
| baseline | **8/14 = 0.5714**（A3：15 条里 14 条 gold 名含查询原文，其中 8 条在 rank-0） |
| target | **14/14** |
| 复现命令 | `python %TEMP%\ra_gold_probe.py`（§F.2） |
| **标定读数** | 一个只匹配 `documents.name` 的加权重排**按构造**就是 14/14 —— 这正是它**不能当 headline** 的原因：gold 是 title-derived 的，用 title 匹配它等于用答案作答 |
| 用法 | 它唯一的价值是**证明排序忽略了一个已存在的信号**（活库 934 篇文档的撞击名全部在 `documents.name` 里，且 `Memory.tsx`/`Knowledge.tsx` 已经在展示它）。**必须**与 C1/C2 并列书写，且注明循环性 |

### C8 让质量仪器重新有量程，并且有真模型读数

| 项 | 内容 |
| --- | --- |
| metric | (a) 在 `retrieval-quality` 仪器上，**至少一个切片**的 baseline 严格小于 1.0；(b) 该仪器存在一条**真实 embedder**读数 |
| baseline | (a) **三个指标全部恰好 1.0000**（A1）⇒ 仪器饱和，**任何回归都测不出来**；(b) **不存在**：`RUAGENT_T245_REAL=1` 时 `FastEmbedder::try_new` 失败并静默回退到 hash（A2） |
| target | (a) 至少一个切片 baseline < 1.0（例如把活库 gold 的前 N 条做成固定切片，或把语料换成非合成语料）；(b) 真实模型分支**要么给出读数，要么把它记为 `not_measured` 并写进输出**（不许静默回退 —— A2 今天打印了 ABSENT，这是**正确**的行为，必须保留） |
| 复现命令 | A1 / A2 的两条命令 |
| **标定读数** | 两个前提**都是实测事实**：① 三个指标**逐位 1.0000**（`$env:TEMP\ruagent-ra\retrieval-quality.json` 的 `metrics.fused`）；② 真模型分支失败文本逐字为 `fastembed init: Failed to retrieve model file 'onnx/model.onnx'` 后回退 hash。③ 活库是**真模型表**（`knowledge_meta = fastembed:multilingual-e5-small`，A5）而 harness 不是 ⇒ 两者数字**不可互引** |

### C9 nDCG@10（**二值、每查询 1 个 gold 的退化形式**；分级版挂在 §E3 之后）

| 项 | 内容 |
| --- | --- |
| metric | `nDCG@10`。**本行的形式必须先说清**：冻结 gold 是**二值相关 + 每查询恰好 1 个 gold** ⇒ `IDCG@10 = 1`、`DCG@10 = 1/log₂(rank+2)`（rank 0-based，rank ≥10 记 0），即它就是 MRR 的一个单调变体，**不是** TREC DL 那种分级 nDCG（B8）。写成这样是为了**立刻可算、可证伪**；分级版是 §G2 的待办，不许拿这一行冒充它 |
| baseline | **0.7682**（today 的 RRF，rank 序列 `[0,2,2,1,0,0,0,0,1,1,1,0,2,0,2]`） |
| target | **≥ 0.83** |
| 复现命令 | `python %TEMP%\ra_ndcg.py`（读 F.2 落盘的 `ra_gold_probe.json`，在活库 top-20 内重排） |
| **标定读数** | `w_sem:w_kw = 2:1` ⇒ **nDCG@10 = 0.8421**（同一次标定里 recall@1 0.4667→0.7333、MRR 0.6889→0.8167）。对照：`1:2` **0.7052** · 只语义 `1:0` **0.7638** · 只关键字 `0:1` **0.6078** · 今天 `1:1` **0.7682**。**同一标定下 nDCG@10 的代价**：那一条 content-derived gold（`wiki/kubernetes-troubleshooting`）从 rank 2 掉到 **rank 11**，被 nDCG@10 记为 0（C1 已定位） |
| 何时可以改这一行 | 只有 §E3 的人工分级 gold 到位后，这一行的 metric 才升级为 TREC DL 式分级 nDCG@10，target 才重标。**在那之前禁止**引用这一行去宣称"我们的 nDCG 达到业界水平" |

---

## D 冻结接口

**D 的读法**：`[冻结]` = I-A **不可以**改名字/类型/语义；`[新增]` = 只加不改，调用方零改动；`[破坏]` = 会让别人**编译不过或行为变化**，必须显式标注并有迁移路径。

### D.1 `[冻结]` I-A 必须**保持**的签名与类型

```rust
// crates/knowledge/src/store.rs:44-50
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SearchHit { pub chunk_id: i64, pub document: String, pub content: String, pub score: f32 }

// crates/knowledge/src/store.rs:53-63
#[derive(Debug, Clone, PartialEq)]
pub struct LegHit { pub chunk_id: i64, pub rank: usize, pub raw_score: f32 }

// crates/knowledge/src/store.rs:70-81
// 前 4 个变体的**名字与含义**冻结；集合**只许加**（§E4 要加 Bigram，见 D.3 的 B-8）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeywordStage { Empty, Precision, Prefix, Substring }

// crates/knowledge/src/rrf.rs:6
pub fn rrf(rankings: &[Vec<i64>], k: u32) -> Vec<(i64, f32)>

// crates/store/src/fts.rs:25,44,80,95  ——  逐字节冻结
pub fn terms(query: &str) -> Vec<String>
pub fn match_all(terms: &[String]) -> String
pub fn match_any_prefix(terms: &[String]) -> String
pub fn like_patterns(terms: &[String]) -> Vec<String>

// crates/knowledge/src/embed.rs:7-26
pub trait Embedder: Send + Sync { fn embed(..); fn embed_query(..); fn is_fallback(); fn name(); fn dim(); }
```

**为什么这四个是硬的（每条都有活调用点，不是洁癖）**

| 冻结项 | 依赖它的活调用点 | 违约后果 |
| --- | --- | --- |
| `SearchHit` 的字段集 | `crates/daemon/src/wiki.rs:1482`（`recall_stubs(hits: &[ruagent_knowledge::SearchHit], ..)`）、`api.rs:2984` | I-D（wiki）编译不过 |
| `rrf` 的签名 | `crates/daemon/src/memembed.rs:310`（`ruagent_knowledge::rrf(&[sem_ids, kw_ids], RRF_K)`） | I-B（记忆两腿融合）编译不过；**改签名 = 跨 crate 破坏** |
| `fts::{terms,match_all,match_any_prefix,like_patterns}` 的**语义** | `crates/graph` 按 R-C 规格 DEP-5 **照抄**这 4 个函数做 loose 复刻；`knowledge/src/store.rs:488` 的注释本身就是"so this crate and the graph crate cannot drift again" | I-C 的分词与 I-A 漂移 ⇒ 两套判据（这正是 t247 踩过的坑） |
| `score_kind` 的**字面量** | R-B 规格 D.7 已把 `"rrf_rank"` 冻成字面量，且 `memembed.rs:151-152` 的注释已被点名有措辞漂移 | 两代 API 的消费方（面板/MCP/注入）读的是同一个字面量 |
| **字段名的量纲** | knowledge `semantic_score` = 距离、memory `semantic_score` = 余弦（`api.rs:2581-2582` vs `:2600-2601`） | **这是 A7 的缺陷本身**：I-A **不许**"顺手把 `semantic_score` 改成相似度好让它看起来对" —— 那会让 memory 侧同一个键反向，缺陷变成静默错误 |

### D.2 `[新增]` I-A 必须提供的类型与签名（供 B/C/D/E 消费）

```rust
// ---- 1. 分数的量纲，一等公民。A7 的根治手段。 ----
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ScoreKind { RrfRank, SemanticDistance, KeywordBm25, Cosine, Calibrated }
impl ScoreKind {
    /// 稳定的线上字面量。RrfRank 必须是 "rrf_rank"（R-B D.7 已冻结该字面量）。
    pub fn as_str(self) -> &'static str;   // "rrf_rank" | "semantic_l2sq" | "bm25" | "cosine" | "calibrated"
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct LegEvidence { pub rank: usize, pub raw_score: f32, pub kind: ScoreKind }

// ---- 2. 融合方式：可读、可审计、可入 recall_log ----
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub enum FusionKind { Rrf { k: u32, w_semantic: f32, w_keyword: f32 } }

/// 腿窗口：与调用方的分页参数**无关**的常量。C5 的根治手段。
pub const LEG_WINDOW: usize = 60;

// ---- 3. 逐腿证据（`SearchLegs` 的扩写，不是另起一套） ----
#[derive(Debug, Clone, PartialEq)]
pub struct SearchEvidence {
    pub semantic: Vec<LegHit>,        // [冻结] 字段名与类型不变
    pub keyword: Vec<LegHit>,         // [冻结] 字段名与类型不变
    pub keyword_stage: KeywordStage,  // [冻结]
    pub fused: Vec<(i64, f32)>,       // [冻结] 仍是 RRF 名次分
    pub fusion: FusionKind,           // [新增]
    pub leg_window: usize,            // [新增] 恒 == LEG_WINDOW
    pub candidates: usize,            // [新增] 进入融合的并集大小
}
/// 兼容别名：让 api.rs / retrieval-legs.rs **不需要为改名而改**（见 D.3 B-1）
pub type SearchLegs = SearchEvidence;

// ---- 4. 真实相关性分数：**与 `score` 并存**，绝不取代它 ----
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct RelevanceScore {
    pub value: f32,               // ∈ [0,1]，越大越相关
    pub kind: ScoreKind,          // == ScoreKind::Calibrated
    pub version: u32,             // 标定版本；换模型/换权重必须 +1
    pub query_background: f32,    // 该查询的背景水平（标定用），必须一起给出才能复查
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct RankedHit {
    pub hit: SearchHit,                     // 复用；`hit.score` 的语义**不变**
    pub score_kind: ScoreKind,              // == ScoreKind::RrfRank
    pub semantic: Option<LegEvidence>,      // None = 该腿未命中（**不许写 0**，R-B D.7 已冻结这个约定）
    pub keyword: Option<LegEvidence>,
    pub relevance: Option<RelevanceScore>,  // [新增] 展示/排序用；**禁止**当 min_score 闸门（C3）
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SearchPage { pub hits: Vec<RankedHit>, pub evidence: SearchEvidence }

// ---- 5. I-A 的入口（additive；search / search_legs 的签名一律不动） ----
impl Knowledge {
    pub async fn search_page(&self, query: &str, limit: u32) -> Result<SearchPage, KnowledgeError>;

    /// 只读残余面清点。**欠 I-B（R-B C5 / E.5）**，形状已按 mem-core 2026-09-27 的
    /// 询问回执冻在这里（我在同一条消息里已确认"t7 之后给"）。
    /// needle 按**原文子串**匹配（不是 token），只报不删。
    ///
    /// **截断必须可分辨**（mem-core 2026-09-27T22:16+08:00 提出，我采纳：这是
    /// `fts_like` 的 `LIMIT leg_k` 那堂课更深一层 —— 一个"带上界的结果"不能当清点器，
    /// 否则"清点完了、没有残余"与"只看了一部分"无法区分，就是假通过）。
    pub async fn residual_scan(&self, needle: &str, limit: u32, exact_total: bool)
        -> Result<ResidualPage, KnowledgeError>;

    /// 文档的磁盘副本路径 `<root>/knowledge/<name>.md`（A5：934/934 有 source）。
    pub fn document_path(&self, name: &str) -> std::path::PathBuf;
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ResidualHit {
    pub document_id: i64,
    pub name: String,
    pub chunk_id: Option<i64>,   // None = 只命中 documents.name（整篇粒度）
    pub origin: ResidualOrigin,  // 哪一份副本命中
    pub path: Option<std::path::PathBuf>,   // origin 含 File 时必填
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ResidualOrigin { Db, File, Both }

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ResidualPage {
    pub hits: Vec<ResidualHit>,   // 至多 limit 条
    /// `hits.len() == limit` 时**不能**推断"清完了" —— 由 `limit + 1` 的多取一行判定，
    /// 零额外成本（不需要 COUNT）。
    pub truncated: bool,
    /// 只有 `exact_total = true` 才有：多一次 `COUNT(*)` 全表扫
    /// （实测口径：10765 chunk 上单次 LIKE 全表扫 **10.47 ms**（我的 Python 层均值）
    ///  / **6.80 ms**（mem-core 原生 SQLite 中位数）—— 两个同量级读数见 A8）。
    pub total: Option<usize>,
}

// ---- 6. crates/store/src/fts.rs 的新增（**只加不改**） ----
/// 汉字 bigram 展开：每个汉字 run → 重叠 2 字 bigram；长度 1 的 run → 该字符本身；
/// 非汉字原样透传。B4 的一手依据（Lucene CJKBigramFilter / ES cjk_bigram）。
pub fn han_bigrams(text: &str) -> String;
/// bigram 腿的查询构造（**新函数**，不动 match_all / match_any_prefix）。
pub fn match_bigrams(terms: &[String]) -> String;
/// 现在私有（`fts.rs:68`），I-A 与 I-C 都需要读它来判"子串腿为什么空" ⇒ 改为 `pub`（additive）。
pub const MIN_RECALL_ASCII: usize = 3;

// ---- 7. 加权融合：新函数，不动 rrf() ----
pub fn rrf_weighted(rankings: &[(&[i64], f32)], k: u32) -> Vec<(i64, f32)>;
```

### D.3 `[破坏]` 逐条标注（含迁移路径与"是不是真的破坏"）

| ID | 变更 | 类型 | 谁会被打断 | 迁移路径 |
| --- | --- | --- | --- | --- |
| **B-1** | `SearchLegs` 改名/扩写为 `SearchEvidence` | **编译破坏（可消除）** | `crates/daemon/src/api.rs:2607-2620,4020-4034`（读 `.semantic/.keyword/.keyword_stage/.fused.len()`）、`crates/knowledge/tests/retrieval-legs.rs` | **不要改名**：保留 `pub struct SearchLegs` 原名 + 只加 3 个字段，或用一个 `pub type SearchLegs = SearchEvidence;` 别名。**本规格要求后者** ⇒ 破坏 = 0，`.semantic/.keyword/.keyword_stage/.fused` 全部照旧可读 |
| **B-2** | `leg_k = limit.max(10)` → `LEG_WINDOW = 60` | **行为破坏（不是编译破坏）** | 所有消费排序的人；**历史 `top_knowledge_score` 全部失去可比性** | ① 一次改到位，不许留两个窗口；② `recall_log` 加 `knowledge_leg_window` + `scoring_version`（§E schema），**历史行为 NULL**（R-B/t347 的纪律：新库上验过的字段不为历史行背书）；③ 跨这次改动**禁止**把新旧 `top_knowledge_score` 画在同一条曲线上 |
| **B-3** | `rrf(&[Vec<i64>], k)` 加权重 | **会变成编译破坏，所以不做** | `memembed.rs:310` | 权重走**新函数** `rrf_weighted`；`rrf` 签名不动。**若有人想给 `rrf` 加默认权重参数，这就是违约** |
| **B-4** | `Knowledge::search` 的 `SearchHit.score` 语义从"RRF 名次分"变成别的 | **静默语义破坏（最危险）** | `api.rs:2984-3018`（把 `hit.score` 直接放进 JSON）、面板、MCP | **禁止**。真实相关性走**新字段** `relevance`。这条是 A7 的教训：缺陷不是"分数量纲混了"，而是"换名字时没人标注" |
| **B-5** | `score_kind` 新增取值（`"semantic_l2sq"` / `"calibrated"` 等） | **加值，不是破坏** | 消费方若 `match` 穷举会不穷举 | 消费方必须按"未知 kind ⇒ 不渲染"处理。**已存在**的 `"rrf_rank"` 字面量不许动 |
| **B-8** | `KeywordStage` **新增 `Bigram` 变体**（§E4）—— 注意它出现在 D.1 的 `[冻结]` 表里，所以**必须在这里显式登记为一次对冻结类型的扩写** | **加变体：源码兼容，语义新增** | 任何对 `KeywordStage` 做**穷尽 `match`** 的代码会编译不过 | **本会话已全仓核过：不存在任何穷尽 match。** 全部 13 个使用点都是**等值比较**（`retrieval-quality.rs:452`、`retrieval-legs.rs:147,163,187,215`）或**构造**（`store.rs:517,521,525,540,542`）。`api.rs:2967` 用 `format!("{stage:?}").to_lowercase()` ⇒ 自动输出新值 `"bigram"`，**零改动**。D.1 里"变体集合"的冻结因此被**收紧**为：前 4 个变体的**名字与含义**冻结，**集合可以扩（只能加，不能改名、不能改含义）** |
| **B-6** | `fts::MIN_RECALL_ASCII` 由私有改公开 | **additive** | 无 | 无 |
| **B-7** | 迁移 `0019` 的 `ALTER TABLE chunks ADD COLUMN grams TEXT` | **schema 破坏（additive 列）** | 任何 `SELECT *` 后按列序取值的地方 | 全仓 `chunks` 的读取都按列名（`store.rs:274,716,732` 都是显式列名）⇒ 实测无列序依赖。`INSERT` 不指定 `grams` ⇒ 允许 NULL（回填前） |

### D.4 别的单欠我 / 我欠别的单（显式登记，避免 §7.62「同一文件两个写者」）

| 方向 | 内容 | 状态 |
| --- | --- | --- |
| 我 → I-B（mem-core） | 残余面清点：`residual_scan` + `document_path` 的形状（D.2 第 5 块，**含 `truncated`/`total` 的截断可分辨性**）；三条 read-only 事实（`chunks` **没有** content_hash ⇒ 只能 document 粒度按 sha256 查；裸 SQL 今天就能按子串查；934/934 有磁盘副本）。**类型各自定义、不跨 crate 依赖**（`crates/memory` 刻意不依赖 `ruagent_knowledge`，见 `inject.rs:276-289`）⇒ 两边字段集必须有一个**漂移自检**（比对字段名与 `origin` 取值集合），这是本表登记的第 5 条约定 | **已发消息两次**（2026-09-27 询问回执 + 22:16 截断请求的回执），答"t7 之后给" |
| I-B → 我 | `crates/memory/src/inject.rs` 的注入面**不使用 RRF**（R-B D.7）；我的 `relevance` **不许**作为相似度泄露到注入面 | 已由 R-B 冻结，我照办 |
| 我 → I-C（graph） | `fts::terms/match_all/match_any_prefix/like_patterns` 四函数**逐字节不变**（R-C DEP-5）；CJK 能力以**新函数**（`han_bigrams`/`match_bigrams`）提供，I-C 想同源就调它们 | 已由 R-C 登记，我确认 |
| I-D（wiki）→ 我 | `wiki.rs:1482` 依赖 `SearchHit` ⇒ 它进 D.1 冻结集（本规格新增该依据） | 本规格登记 |
| 我 → integ（t19） | `crates/daemon/src/api.rs` 的 `knowledge_hit_json`（`:2983`）与面板 `Memory.tsx:367-384` / `Knowledge.tsx:418-470` 的改动**不在我 inScope**：本规格只给要求（C6），接线归 integ。**api.rs 是高冲突文件**（closure §7.62、mem-core 也登记过），同一时间窗只许一个写者 | **本规格即登记**，见 §E9 |

---

## E 实现清单（按优先级；每项 = inScope 路径 + 依赖的 store/schema 变更）

**写者纪律**：本表的 inScope 只包含 `crates/knowledge/**` 与 `crates/store/**`（我 t6/t7 的路径）。`crates/daemon/src/api.rs`、`panel/**` **不在**我的 inScope，一律以 **HAND-OFF（integ / t19）** 标出。与 I-SCHEMA(crates/store)、I-B(crates/memory)、I-C(crates/graph + daemon/distill.rs)、I-D(daemon/wiki.rs) 的交界逐项写明"不冲突的依据"。

### E1 `[P0]` 融合：权重 + 恒定腿窗口（C1/C2/C5）

| 项 | 内容 |
| --- | --- |
| inScope | `crates/knowledge/src/store.rs`（`compute_legs` / `search` / `search_legs` / `search_page`）· `crates/knowledge/src/rrf.rs`（**只加** `rrf_weighted`） |
| 做什么 | ① `leg_k` 由 `limit.max(10)` 改为 `LEG_WINDOW`；② 融合改 `rrf_weighted(&[(ann_ids, w_sem), (fts_ids, w_kw)], 60)`，初值 `w_sem=2.0, w_kw=1.0`（C1 平台区 {2,3,4}:1）；③ `search_legs` 增 3 字段 `fusion/leg_window/candidates`，并保留 `pub type SearchLegs` 别名（B-1 ⇒ 零编译破坏） |
| schema 依赖 | **无**（这一项不动 store） |
| 不冲突依据 | `rrf` 签名不动 ⇒ `memembed.rs:310` 不受影响；`SearchHit`/`LegHit`/`KeywordStage` 不动 ⇒ `wiki.rs` 不受影响；不碰 `entities`/`entity_edges`（`Select-String` 实测 `crates/knowledge/src` 只在注释与 `store.rs:488` 提到 graph） |
| 验收锚点 | C1 ≥0.70 / C2 ≥0.80 / **C9 ≥0.83** / C5 13/13；并且**代价必须在报告里出现**：recall@5 1.0000→0.9333，以及被牺牲的那一条是 `kubernetes 滚动更新回滚`（gold 掉到 rank 11） |

### E2 `[P0]` 真实相关性分数 + 量纲（C6/C3）

| 项 | 内容 |
| --- | --- |
| inScope | `crates/knowledge/src/store.rs`（`ScoreKind` / `LegEvidence` / `RelevanceScore` / `RankedHit` / `SearchPage` / `search_page`）· `crates/knowledge/src/lib.rs`（re-export） |
| 做什么 | ① 每个命中带 `semantic`/`keyword` 的 `LegEvidence{rank, raw_score, kind}`（未命中 = `None`，**不是 0**）；② `relevance` 由语义距离经**按查询背景的标定**得到（`query_background` 一起给出，否则不可复查）；③ 标定**不做闸门**，只展示/排序（C3 的明确禁止） |
| schema 依赖 | 无（字段名与版本号进 §E3 的 `recall_log`） |
| 不冲突依据 | 全部是**新名字**；`score` / `semantic_score` / `keyword_score` 的字面量语义（R-B D.7）**一个不动**（B-4 是明文禁止项） |
| 验收锚点 | C6 的 5/5 分数键都有 kind；C3 的 ≥0.85 在重标前的读数**只能报"未达/未测"**，不许报绿 |

### E3 `[P0]` 冻结评测 gold 集 + 分层指标（C8/C9 的前置；B7/B8）

| 项 | 内容 |
| --- | --- |
| inScope | `crates/store/src/migrations/0019_recall_quality.sql`（**新文件**）· `crates/store/src/migrations.rs`（MIGRATIONS 追加）· `crates/knowledge/tests/retrieval-quality.rs`（扩写：增加一个**读 DB gold 集**的切片）· `crates/knowledge/tests/retrieval-gold-live.md`（可选夹具说明） |
| 做什么 | ① 建 `query_eval_sets` / `query_eval_gold` / `query_eval_runs`（provenance：`judged_by ∈ {'title-derived','human'}` **不许混写**）；② 把 A3 的 15 条 + A9 的 7 条**登记为 `judged_by='title-derived'` 的第一版**（诚实标注它是弱代理）；③ harness 增一条切片：从 DB 读该 gold 集，报分层（`class` 分组）的 recall@1/@5/@20 + MRR + n，并**把 `judged_by` 写进输出** |
| schema 依赖 | **这一项就是 schema 变更本身**（登记给 I-SCHEMA = 我 t6） |
| 不冲突依据 | 新表，只读既有 `documents(id)` 外键；不碰 `documents/chunks/wiki_builds/entities/memories` 的任何列；I-B/I-C/I-D 不会读这三张表（本规格即登记点） |
| 验收锚点 | C8(a)：至少一个切片 baseline < 1.0；`query_eval_runs` 里**必须**有一条 `judged_by='title-derived'` 且 embedder 字段非空的行 |

### E4 `[P1]` CJK 汉字 bigram 影子索引（C4；A8；B3/B4）

| 项 | 内容 |
| --- | --- |
| inScope | `crates/store/src/migrations/0019_recall_quality.sql`（`ALTER TABLE chunks ADD COLUMN grams TEXT` + `chunks_fts_cjk` + 3 个触发器）· `crates/store/src/fts.rs`（**只加** `han_bigrams` / `match_bigrams`；`MIN_RECALL_ASCII` 改 `pub`）· `crates/knowledge/src/store.rs`（写入侧填 `grams`、`keyword_leg` 增第 4 段 bigram 腿、`KeywordStage` 增 `Bigram` 变体） |
| 做什么 | ① 写入路径（`index_doc` / `migrate_embedder` 的 re-embed 路径）在 INSERT 时带 `grams = fts::han_bigrams(content)`；② `keyword_leg` 的降级链由 `Precision → Prefix → Substring` 改为 `Precision → Prefix → Bigram → Substring`，并把 `Substring` 降到**最后**（它是全表扫，10.47 ms）；③ 回填：在 `with_embedder` 里（`migrate_embedder` 同一个位置、同一个单写者、分批、可续跑）对 `grams IS NULL` 的行补齐，**并打印命中行数**（不许静默）；④ `KeywordStage::Bigram` 的 `raw_score` 语义：与 Substring 同为 0.0（FTS bm25 对 bigram 表是**可算的** ⇒ 实际上应给 bm25，规格要求给 bm25 并把 `kind` 标为 `KeywordBm25`） |
| schema 依赖 | `chunks.grams` + `chunks_fts_cjk` + 触发器（E3 的同一个迁移文件） |
| 不冲突依据 | `fts::{terms,match_all,match_any_prefix,like_patterns}` **逐字节不变** ⇒ R-C DEP-5 的"graph 照抄这 4 个函数"不受影响；新能力全在新函数里。`chunks` 只加可空列，既有读取全按列名（实测 `store.rs:274,716,732`）。**`KeywordStage` 加变体**：消费方 `api.rs:2967` 用 `format!("{stage:?}").to_lowercase()` ⇒ **自动跟得上**（新增变体输出 `"bigram"`），面板按字符串显示。**这是加值不是破坏** |
| 验收锚点 | C4 (a) ≥0.95 / (b) p50 ≤1 ms；回填行数 == `SELECT COUNT(*) FROM chunks WHERE content GLOB '*[一-龥]*'` 的那部分（I-A 自报口径） |

### E5 `[P1]` 分页无关性的**回归判据**（C5）

| 项 | 内容 |
| --- | --- |
| inScope | `crates/knowledge/tests/retrieval-legs.rs`（新增用例）· 可复用 `crates/knowledge/tests/retrieval-quality.rs` 的确定性夹具 |
| 做什么 | 一条**必须能失败**的用例：对一组固定查询，断言 `search(q,5).first() == search(q,60).first()`（文档名与分数逐位相等）。**不许**用"两次调用都非空"这种真空通过的判据（`§7.84` 的教训：空集上的通过是真空通过） |
| schema 依赖 | 无 |
| 不冲突依据 | 只加测试用例；`retrieval-legs.rs` 是 knowledge 自己的测试文件（无他人写者） |
| 验收锚点 | 用例在**改前**（`limit.max(10)`）必须**红**（这是它有效的证明），改后绿 |

### E6 `[P1]` 遥测：把"新分数"写进 `recall_log`（C1/C2/C5 的可观测前提）

| 项 | 内容 |
| --- | --- |
| inScope | `crates/store/src/migrations/0019_recall_quality.sql`（`ALTER TABLE recall_log ADD COLUMN ...`） |
| 做什么 | 加 4 列：`top_knowledge_relevance REAL` · `top_knowledge_relevance_kind TEXT` · `knowledge_leg_window INTEGER` · `scoring_version INTEGER`。**历史行一律 NULL**（R-B/t347 的纪律：新库验过的字段不为历史行背书）；**不重解释 `top_knowledge_score`** |
| schema 依赖 | 这一项就是 schema |
| 不冲突依据 | 只加列，既有 `SELECT`（`api.rs:2858,2899`、`gen2-graph-spec.md:619` 的复用脚本、面板）全按列名取；写入点在 `api.rs:2766`（**integ 的 inScope**）⇒ 列由我建、**写入由 integ 接**（HAND-OFF） |
| 验收锚点 | `pragma table_info(recall_log)` 有 4 个新列；一条新查询产生的行的 `knowledge_leg_window = 60` 且 `scoring_version` 非空 |

### E7 `[P2]` 多样性 / 具体性先验（C7 的诊断 → 下一轮的真目标）

| 项 | 内容 |
| --- | --- |
| inScope | `crates/knowledge/src/store.rs`（一个**可关闭**的重排阶段；默认关，只有拿到多 gold 集才开） |
| 做什么 | 提供 `rerank_documents(&[RankedHit], policy)` 的**骨架**：① 同文档去重（同一 document 在 top-k 里占多槽）；② hub 惩罚（文档名是另一命中文档名的前缀 ⇒ 降级）。**默认 policy = `None`**，因为 A3 的 gold 集每查询只有一个 gold，任何 λ 都是循环标定（B9） |
| schema 依赖 | 无 |
| 不冲突依据 | 只加一个默认关闭的阶段 |
| 验收锚点 | **本轮不设达标线**（明说：B9 的 λ 标定不了）。只要求：C7 的诊断读数（8/14 → ?）被记录，并标"循环" |

### E8 `[P2]` 查询理解（改写 / 扩展）—— **本轮不采纳**

| 项 | 内容 |
| --- | --- |
| 不采纳理由 | ① 违反本设计"查询期零 LLM"（`design §6.6 #3`，`rrf.rs:1-2` 的模块注释就写着）；② A3/A10 的证据说得很清楚：**两条腿各自都能找到 gold（15/15、15/15），损失在融合排序**，不是查询与语料不匹配 ⇒ 先修融合，收益大且零成本；③ 一手读数（B6 的 −67% 与 `arxiv 2604.01733` 的 HyDE 表）里查询扩展的收益**高度依赖任务形状**，而我们的语料 92% 是个人笔记（A5：767/934 文档名含 CJK） |
| inScope | 无（登记为**下一轮候选**，带一个前置：先完成 E3 的 gold 集，才能测出改写到底有没有用） |
| 不冲突依据 | 无代码 |

### E9 `[P1]` HAND-OFF 清单（**不在我 inScope**，逐个点名）

| # | 交给谁 | 文件与坐标 | 需要什么 | 依据 |
| --- | --- | --- | --- | --- |
| H-1 | **integ（t19）** | `crates/daemon/src/api.rs:2983-3019`（`knowledge_hit_json`） | 每个分数键旁挂 `*_kind`：`semantic_score_kind` / `keyword_score_kind`；hit 增 `relevance` / `relevance_kind` / `fusion` / `leg_window`。**保留 `score_kind: "rrf_rank"` 字面量** | C6；A7；R-B D.7 |
| H-2 | **integ（t19）** | `crates/daemon/src/api.rs:2466-2700`（`recall`）· `:4008-4058`（`knowledge_search`） | ② 把 `search_legs` 换成 `search_page`（一次调用拿到腿 + 量纲，消掉"第二条腿重算一遍"的 60% 代价 —— closure §7.47 已量过）；③ `recall_log` 的 4 个新列写入（`api.rs:2766`） | C5/C6；§E6 |
| H-3 | **integ（t19）** | `panel/src/views/Memory.tsx:367-384` · `Knowledge.tsx:418-470` · `panel/src/api.ts:206-211,1120-1125` | 并排显示时必须带量纲（`m 0.86 cosine` / `k 0.016 rrf_rank`），或干脆不并排 | C6；A7 |
| H-4 | mem-core（I-B） | `crates/memory/src/inject.rs` | 用 `relevance`（而不是 RRF 名次分）做重排；**不得**把它当相似度送进注入 | B11；R-B D.7 |
| H-5 | graph（I-C） | 无文件改动 | 想同源 CJK 能力就调 `fts::han_bigrams` / `match_bigrams`；**不许**复制新实现 | B4；R-C DEP-5 |
| H-6 | wiki（I-D） | 无文件改动 | `SearchHit` 进冻结集（D.1） | `wiki.rs:1482` |

### E10 schema 变更汇总（I-SCHEMA 照此落地，**一个文件**：`crates/store/src/migrations/0019_recall_quality.sql`）

```sql
-- 0019_recall_quality.sql  (R-A / t1 §E)
-- 三条原则：① 只 ADD，不重解释既有列（t347：新库验过的字段不为历史行背书）；
--           ② 每个新表述都带 provenance 或 version；③ 历史行留 NULL，不留默认值。

-- (1) 冻结评测 gold 集：这是今天**不存在**的东西，也是 A3/A11-#2 的直接产物。
CREATE TABLE query_eval_sets (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL UNIQUE,
    version    TEXT NOT NULL,
    created_at TEXT NOT NULL,
    frozen_at  TEXT                       -- NULL = 还可改；非 NULL = 冻结，harness 只读冻结集
);
CREATE TABLE query_eval_gold (
    id               INTEGER PRIMARY KEY,
    set_id           INTEGER NOT NULL REFERENCES query_eval_sets(id),
    query            TEXT NOT NULL,
    class            TEXT NOT NULL,       -- exact-ascii|cjk-2char|cjk-run|mixed|punctuated|no-answer
    gold_document_id INTEGER REFERENCES documents(id),   -- NULL ⟺ answerable = 0
    answerable       INTEGER NOT NULL,
    judged_by        TEXT NOT NULL,       -- 'title-derived' | 'human'  不许混写
    note             TEXT
);
CREATE INDEX idx_query_eval_gold_set ON query_eval_gold(set_id, class);
CREATE TABLE query_eval_runs (
    id           INTEGER PRIMARY KEY,
    ts           TEXT NOT NULL,
    set_id       INTEGER NOT NULL REFERENCES query_eval_sets(id),
    embedder     TEXT NOT NULL,           -- 换模型 ⇒ 读数不可比，必须落库
    leg_window   INTEGER NOT NULL,
    fusion       TEXT NOT NULL,           -- 例 "rrf(k=60,w_sem=2,w_kw=1)"
    metrics_json TEXT NOT NULL,
    artifact     TEXT
);

-- (2) recall_log：加 4 列，**不改** top_knowledge_score 的解释。
ALTER TABLE recall_log ADD COLUMN top_knowledge_relevance      REAL;
ALTER TABLE recall_log ADD COLUMN top_knowledge_relevance_kind TEXT;   -- 'calibrated'；历史 NULL
ALTER TABLE recall_log ADD COLUMN knowledge_leg_window         INTEGER;-- 历史 NULL ⇒ 与今天不可比
ALTER TABLE recall_log ADD COLUMN scoring_version              INTEGER;-- 历史 NULL

-- (3) CJK bigram 影子索引（external content over chunks.grams）。
ALTER TABLE chunks ADD COLUMN grams TEXT;   -- NULL 直到回填（§E4 ③）
CREATE VIRTUAL TABLE chunks_fts_cjk USING fts5(
    grams, content='chunks', content_rowid='id', tokenize='unicode61');
CREATE TRIGGER chunks_ai_cjk AFTER INSERT ON chunks BEGIN
    INSERT INTO chunks_fts_cjk(rowid, grams) VALUES (new.id, new.grams);
END;
CREATE TRIGGER chunks_ad_cjk AFTER DELETE ON chunks BEGIN
    INSERT INTO chunks_fts_cjk(chunks_fts_cjk, rowid, grams) VALUES ('delete', old.id, old.grams);
END;
CREATE TRIGGER chunks_au_cjk AFTER UPDATE OF grams ON chunks BEGIN
    INSERT INTO chunks_fts_cjk(chunks_fts_cjk, rowid, grams) VALUES ('delete', old.id, old.grams);
    INSERT INTO chunks_fts_cjk(rowid, grams) VALUES (new.id, new.grams);
END;
```

**迁移的可证伪面**（I-SCHEMA 与 I-A 都要交）：
1. `SELECT COUNT(*) FROM query_eval_gold WHERE judged_by='title-derived'` = 22（A3 的 15 + A9 的 7）且 `judged_by='human'` = 0（诚实：人工集还没建）；
2. 新查询后 `recall_log` 新行的 `knowledge_leg_window = 60`、`scoring_version` 非空，而**历史 651 行这 4 列全 NULL**；
3. 回填后 `SELECT COUNT(*) FROM chunks WHERE grams IS NULL` = 0，且 `chunks_fts_cjk` 行数 = `chunks` 行数；
4. `wrapper`：`PRAGMA integrity_check` 通过，且 `chunks_fts`（老表）行数不变（新表不许影响旧表）。

---

## F 复现（脚本 + 命令）

所有脚本写在 `$env:TEMP`（**不落进仓库**，避免污染 `git status`）。时间窗与对象集在每个脚本的输出第一行。

### F.1 活库 census + `recall_log` 分布（只读）

```powershell
# ra_probe1.py / ra_probe2.py / ra_memscore.py 的核心（同一个只读连接）
python - <<'PY'
import sqlite3, os, datetime
p = os.path.join(os.environ["USERPROFILE"], ".ruagent", "data", "ruagent.db")
con = sqlite3.connect("file:" + p.replace("\\","/") + "?mode=ro", uri=True)   # 只读
c = con.cursor()
print("NOW =", datetime.datetime.now().astimezone().isoformat())
for t in ("documents","chunks","chunk_sections","entities","entity_edges",
          "memories","recall_log","wiki_build_pages","chunk_revisions"):
    c.execute(f"select count(*) from {t}"); print(t, c.fetchone()[0])
c.execute("select key,value from knowledge_meta"); print(c.fetchall())
c.execute("select top_knowledge_score,count(*) from recall_log group by 1 order by 2 desc")
print("top_knowledge_score:", c.fetchall())
c.execute("select min(top_knowledge_score),max(top_knowledge_score),count(*) from recall_log")
print("min/max/n:", c.fetchone())
c.execute("select count(distinct query) from recall_log"); print("distinct queries:", c.fetchone()[0])
c.execute("select source,count(*) from recall_log group by 1 order by 2 desc"); print(c.fetchall())
c.execute("select min(top_memory_score),max(top_memory_score),avg(top_memory_score),count(*) from recall_log")
print("memory cosine min/max/avg/n:", c.fetchone())
c.execute("select round(top_memory_score,3),count(*) from recall_log group by 1 order by 2 desc limit 5")
print("memory cosine buckets:", c.fetchall())
PY
```

### F.2 活库 gold 集探针（只读 HTTP；`%TEMP%\ra_gold_probe.py`）

关键骨架（完整脚本见 `$env:TEMP\ra_gold_probe.py`；`limit=20`、`legs=true`；首尾各读一次 `recall_log` 证只读）：

```python
GOLD = [("Docker compose","obsidian/notes/Docker-OQEurB-Docker compose-Docker compose","exact-ascii"),
        ("docker 常用命令","obsidian/notes/Docker-OQEurB-docker 常用命令-docker 常用命令","mixed-cjk-ascii"),
        ("Springboot 热部署","obsidian/notes/45v6HJ-Java框架-模块-Springboot-Springboot热部署-Springboot热部署","mixed-cjk-ascii"),
        ("反射的访问权限问题","obsidian/notes/45v6HJ-常见问题-反射的访问权限问题-反射的访问权限问题","cjk-run-long"),
        ("AQS并发锁","obsidian/notes/45v6HJ-JavaSE-AQS并发锁-AQS并发锁","cjk-run-nosep"),
        ("缓存caffeine","obsidian/notes/45v6HJ-Java框架-模块-缓存caffeine-缓存caffeine","mixed-cjk-ascii"),
        ("Maven项目管理","obsidian/notes/45v6HJ-Maven项目管理-Maven项目管理","mixed-cjk-ascii"),
        ("JdbcTemplate","obsidian/notes/45v6HJ-Java框架-模块-Spring5-JdbcTemplate-JdbcTemplate","exact-ascii"),
        ("OJ在线判题系统","obsidian/notes/45v6HJ-项目学习-OJ在线判题系统-OJ在线判题系统","mixed-cjk-ascii"),
        ("docker exec进入容器并执行命令","obsidian/notes/Docker-OQEurB-docker exec进入容器并执行命令-docker exec进入容器并执行命令","mixed-long"),
        ("@Param装饰器","obsidian/notes/45v6HJ-Java框架-模块-Mybatis-不使用@Param装饰器，自动绑定参数-不使用@Param装饰器，自动绑定参数","punctuated"),
        ("RPC框架","obsidian/notes/45v6HJ-项目学习-RPC框架-RPC框架","mixed-cjk-ascii"),
        ("ChatGPT提示词","obsidian/notes/CNGceW-ChatGPT提示词-ChatGPT提示词","mixed-cjk-ascii"),
        ("github相关","obsidian/notes/14mhGE/github相关","mixed-cjk-ascii"),
        ("kubernetes 滚动更新回滚","wiki/kubernetes-troubleshooting","content-wiki")]
NOANSWER = ["capital of Peru","xylophone zebra","search_legs","如何用 Rust 写一个向量索引",
            "best time to see migrating whales","photosynthesis light-dependent reactions","ruagent memory"]
# 每条查询：GET /api/v1/knowledge/search?q=..&limit=20&legs=true
# 记 rank = hits 里 gold 文档名第一次出现的下标（0-based）；不在 ⇒ None
```

### F.3 `limit` 敏感性（`%TEMP%\ra_limit_probe.py`）

```python
for q in QUERIES:                       # 13 条
    for L in (3,5,10,20,30):
        j = get(f"/api/v1/knowledge/search?q={q}&limit={L}&legs=true")
        top1 = j["hits"][0]["document"] if j["hits"] else None
    # distinct(top1) > 1 ⇒ 排序依赖分页参数
```

### F.4 CJK 可达性普查（`%TEMP%\ra_cjk_census.py`）

```python
# V = fts::terms 逐字复刻（按 !is_alnum 切分、lowercase、去重、保序）扫过全部 chunk
# 长汉字 token（len>=5）随机 1500 个（seed 20260927）→ 机械抽全部 2 字子串（去重）
in_V   = [s for s in cand if s in V]                             # precision 可达
prefix = [s for s in cand if s not in V and any(v.startswith(s) for v in V)]
only   = [s for s in cand if s not in V and s not in set(prefix)]  # 只能靠 LIKE
# 成本：only 里随机 40 条，分别用 LIKE 与内存 FTS5(trigram) 取 top-20，计时
```

### F.5 bigram 影子索引标定（`%TEMP%\ra_bigram_calib.py`）

```python
# 内存 FTS5(grams, tokenize='unicode61')，grams = han_bigrams(chunk.content)
# han_bigrams: 每个汉字 run → 重叠 2 字 bigram；len==1 的 run → 该字符；非汉字原样
# 同样 40 条 only-LIKE 查询：`t_bg MATCH '"<2字>"' ORDER BY rank LIMIT 20`
```

### F.6 可答/无答案可分性（`%TEMP%\ra_spread.py`）

```python
# d_top1 = hits[0].semantic_score；mean_d = mean(非 None 的 semantic_score)
# spread = mean_d - d_top1     ⇒ 实测两组重叠（被否决）
# 直接用 d_top1 ⇒ 两组在 0.227 分开，间隔 0.0011（不许当阈值，C3）
```

### F.7 关键字腿对 gold 的覆盖（`%TEMP%\ra_cjk_calib.py`）

```python
# 内存 FTS5 t_u(unicode61) + t_t(trigram)，rowid = chunks.id
# 产品腿：match_all → 空则 match_any_prefix → 空则 like_patterns（严格按 store.rs:510-543 的顺序）
# best_doc_rank(rowids, gold) = gold 文档名在该腿结果里的首个下标
```

### F.8 融合/重排标定（`%TEMP%\ra_rerank_calib.py`）

```python
# 读 F.2 落盘的 ra_gold_probe.json（每条查询的 sem_ranks / kw_ranks / fused_scores / hit_ids）
# 在活库 top-20 内重排：sc = w_s/(60+s+1) + w_k/(60+k+1)（缺腿记 0）
# 扫 (1,1)(2,1)(3,1)(4,1)(1,2)(1,0)(0,1) + minrank + min-max 归一化 + leg-agreement 加成
```

### F.8b nDCG@10（`%TEMP%\ra_ndcg.py`，C9）

```python
# 二值、每查询 1 个 gold 的退化形式：IDCG@10 = 1，DCG@10 = 1/log2(rank+2)（rank>=10 记 0）
live = [r["rank"] for r in rows]                 # 今天的 RRF 顺序  ⇒ 0.7682
w21  = [rerank(r, 2, 1) for r in rows]           # 加权重排        ⇒ 0.8421
```

### F.9 量纲探针（`%TEMP%\ra_scorekind_probe.py`，I-A 落地后首次可跑）

```python
# 对同一查询同时打 /api/v1/knowledge/search?legs=true 与（如需）其他消费端
# 断言：每个数值型分数键都有同级 *_kind；并打印 kind → 量纲 的映射表
# 期望映射（C6）：score→rrf_rank · semantic_score→(knowledge) l2sq / (memory) cosine
#                 · keyword_score→bm25 · relevance→calibrated
```

### F.10 本单的 verify 命令（契约给定）

```powershell
$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"
$env:CARGO_TARGET_DIR="$env:TEMP\ruagent-verify-ra"
cargo test -p ruagent-knowledge --test retrieval-quality
git status --porcelain -- crates panel
```

**本会话实测**：两条测试命令（A1/A2）在 `$env:TEMP\ruagent-ra` 与 `ruagent-ra-b` 上跑过，`3 passed`；`git status --porcelain -- crates panel` 在本单全程为**空**（本单只新增 `docs/design/reviews/gen2-recall-spec.md`）。**注意**：verify 说的 `$env:TEMP\ruagent-verify-ra` 是**第三个** target dir，与 `ruagent-ra` 不共享缓存 ⇒ 首次会重编译（本会话在 `ruagent-ra` 上实测 **24m27s**）。这**不是**环境缺陷，是纪律 7 想要的隔离；验证者请预留这个预算。

---

## G 未测 / 不可判定清单（**不许静默跳过**）

| # | 项 | 状态 | 原因 | 谁来解 |
| --- | --- | --- | --- | --- |
| G1 | 真实 embedder 的 harness 读数 | **未测（分支存在但构造失败）** | `FastEmbedder::try_new` 在本工作区报 `Failed to retrieve model file 'onnx/model.onnx'` 并回退 hash（A2，逐字记录） | I-A（t7）：让 harness 的 fastembed 缓存在工作区可用；在可用之前，**所有 harness 数字必须标 hash** |
| G2 | **分级** nDCG@10 的目标值（TREC DL 式，B8） | **不可判定** | 没有分级相关 gold；C9 那一行是**二值、每查询 1 个 gold** 的退化形式，**不许**用它宣称分级 nDCG 达标 | 下一轮（E3 的人工分级集之后） |
| G3 | MMR / 多样性的 λ | **不可判定** | 冻结 gold 每查询只有 1 个 gold ⇒ 循环标定（B9） | 下一轮（需要多 gold 集） |
| G4 | 语义距离的**切点**（0.227 是否可用） | **不可判定，且已明令禁用** | 间隔只有 **0.0011**、n=22、单一语料（A9） | 重标（≥60 条查询 / ≥20 无答案，C3） |
| G5 | `d = 2 − 2cos`（squared-L2）与 `d = ‖a−b‖₂` 之间选哪个 | **未测（但排序上等价）** | 两种读法单调等价 ⇒ 不影响任何 ranking 目标；只影响 `semantic_score` 的**显示值**。fastembed 的 L2 归一化有源码依据（`output.rs:49`），但 LanceDB 的度量/是否平方**本会话未读源码确认** | I-A（t7）：实测一条"两个已知向量"的读数来钉死；在那之前 `semantic_score_kind` 只能写 **`l2sq`（待核）**，不许写 `cosine` |
| G6 | 多样性/具体性重排的收益 | **未测** | 同上（G3） | 下一轮 |
| G7 | `chunks.grams` 的磁盘增量 | **未测** | 本会话只量了**内存**建索引时间（2669 ms）；产品路径含写盘与 WAL | I-A（t7）在临时 root 上量一次，报 `knowledge.db` 增量字节 |
| G8 | 回填 10765 行的耗时/可续跑性 | **未测** | 同上（只在临时库跑过合成语料） | I-A（t7）：临时 root + 真实 chunk 行数，报耗时与中断续跑读数 |
| G9 | `SearchHit` 之外的 crate 内消费者 | **已核，无遗漏** | `Select-String ruagent_knowledge` 全仓命中 43 处，逐个看过：`wiki.rs`（SearchHit/sha256）· `memembed.rs`（rrf）· `lib.rs/chat.rs/runs.rs/distill.rs`（Knowledge 句柄 / Embedder）· `api.rs`（LegHit/KeywordStage/SearchHit） | — |
| G10 | 面板与 MCP 的**行为**读数 | **不在本单** | 本单是规格单；面板读数归 V-INT（t20） | t20 |

---

## 附：本单的关键数字一览（供 RV-A 逐条核对）

| 数 | 值 | 出处 |
| --- | --- | --- |
| 活库 documents / chunks / sections | 934 / 10765 / 8568 | A5 |
| 活库 embedder | `fastembed:multilingual-e5-small` | A5 |
| harness recall@1/@5/MRR（**hash**） | 1.0000 / 1.0000 / 1.0000 | A1 |
| 活库 recall@1 / recall@5 / MRR（15 条 gold） | **0.4667 / 1.0000 / 0.6889** | A3 |
| 活库 CJK 子集 recall@1 | 0.3846（n=13） | A3 |
| gold 落在 top-20 | **15/15** | A3/A10 |
| rank-1 错误里"枢纽页压具体页" | **8/8** | A3 |
| 加权 2:1 后 recall@1 / MRR / nDCG@10 | **0.7333 / 0.8167 / 0.8421** | C1/C2/C9 |
| 加权 2:1 的代价（recall@5 · nDCG@10 · 哪一条） | 1.0000→**0.9333** · 0.7682→**0.8421**（涨）· 但 `kubernetes 滚动更新回滚` 的 gold 从 rank 2 **掉到 rank 11** | C1/C9 |
| `top_knowledge_score` 上界 | 2/61 = **0.0327869**（观测**恰好触界**） | A6 |
| `recall_log` 取值个数 | 旧 3 → **今 5**（全是名次和） | A6 |
| 无答案查询最高融合分 | **0.030214** > 可答最低 0.028814 | A9 |
| 2 字汉字查询只能靠子串 | **63.20%**（6922 抽样） | A8 |
| LIKE vs bigram 索引 | 10.47 ms（我的采样面）/ **6.80 ms**（mem-core 原生复算）→ **0.08 ms**（131×/85×），40/40 命中 | A8 |
| `trigram` 对 2 字查询 | **0/40**（与 FTS5 文档一致） | A8/B3 |
| `limit` 改变 top-1 | **2/13**（10→20 之间） | A4 |
| 记忆余弦 vs 知识名次分 | 0.86 vs 0.016（4 种量纲 / 2 个字段名） | A7 |
