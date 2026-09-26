# 知识库与记忆的闭环优化 — 诊断与任务图（2026-09-26）

> 修订 3（最新）：**§6 取代 §2 与 §3**，并补上 §1 缺的 t246/t250 读数。§1.1 保留被推翻的前提，不删。

## 0 一句话

**引擎算法不差，缺的是闭环 —— 而现在有了最硬的一条读数：知识库对 agent 的回答零贡献。**

召回在 99.3% 的查询里返回知识命中、100% 返回 wiki 命中，而 79 个 `context_injected` 一等事件里
含 knowledge / wiki / entity 块的 = **0**。两条注入路径都只查 memories。
也就是说：**crates/knowledge 建了一套没人用的检索**。

## 1 基线读数（2026-09-26，全部可复现）

| 项 | 读数 | 复现方式 |
| --- | --- | --- |
| knowledge 文档 / chunk | **5 / 12** | `curl /api/v1/knowledge/documents` |
| 磁盘知识目录 | **3,516 B / 5 文件**（含 `doctor-probe.md` 111 B，占 3.2%） | `ls -R ~/.ruagent/knowledge` |
| 自动扫描 | **在跑**：boot + 每 60s，SHA-256 增量，扫描面只有 `<root>/knowledge/**/*.md` | `sed -n '195,215p' crates/daemon/src/lib.rs` |
| 可喂料字节 | transcripts 27.83 MB / 146 · `~/.claude/projects` 103.89 MB / 136 · 仓库 `docs/` 70.45 MB **但 .md 只有 2.44 MB** | 磁盘字节 |
| 索引能力 vs 喂料 | sessions **631 行** vs documents **5 行** | 只读 sqlite |
| episodes | **6 行**，全 `kind='mcp_write'` = 4 条事实 + 2 条探针残留 | `SELECT kind,source_run FROM episodes` |
| episodes 的写入者 | `memory/episode.rs:60`，唯一调用者 = `POST /api/v1/memory/write`；**distill 不写 episodes** | `grep -n episode crates/daemon/src/distill.rs` |
| distill_log | **32 行 = 32 个会话**（`INSERT OR REPLACE`，每会话一行、重跑替换） | 只读 sqlite |
| 蒸馏三者全 0 | **18/32 = 56.25%**（`=0` 可能是已去重，也可能是 agent 返回空） | 只读 sqlite |
| memories | **154 行**，154/154 真实模型嵌入 | `SELECT embedder,COUNT(*) FROM memories GROUP BY embedder` |
| recall_log | **574 行 / 15 条不同查询** | 只读 sqlite |
| 探针占比 | kettle 279 + autohotkey-v2 275 = **554/574 = 96.5%**，间隔 median 4.2 分钟（成簇调用，非定时任务） | 只读 sqlite |
| `top_knowledge_score` | 574 行**只有 3 个取值**且逐位等于 f32 公式：`1/61` ×295 · `1/61+1/62` ×276 · `1/61+1/63` ×3；**上界 = 2/61 = 0.0327869**，观测最大值 = 上界的 99.19% | `crates/knowledge/src/rrf.rs` + 只读 sqlite |
| 融合分与查询长度无关 | 1 词 `kettle` 与 7 词查询**同为 0.0163934** | 只读 sqlite |
| 实体腿 | 最近 **574/574** `entities=0`；真实 15 条查询 **0/15** | 同上 |
| 实体 FTS 构造 | `MATCH "autohotkey-v2"` → **0 行**（实体 #4 存在）；`潜艇`/`麒麟` → **0**（#63/#6 存在）；`*` 前缀被静默吞掉 | t247 探针 |
| 自查询对照 | 用实体自己的名字查自己 = **63/63 命中** ⇒ 构造对「名字本身」没坏 | t247 |
| 中文分词 | unicode61 无 CJK 分词：整段汉字 = 一个 term；173 个 CJK run 里 **118 个 ≥3 字**，用 2 字查询永远达不到 | t247 |
| 阈值 | `min_score` 默认 **0.0**，574/574 行**丢弃 0 条**；真正丢命中的是 `recall_stubs`（7 个 wiki 块里 6 个永不出现在响应里） | t247 |
| `GET /api/v1/memory/list` | 不带 `store` → **400** | `curl` |
| 记忆删除 | **不存在**（只有 write / supersede） | `grep 'route("/api/v1/memory' crates/daemon/src/api.rs` |
| confidence | 只在 `<0.5` 时渲染，而 154 行**没有一行 <0.5**（写入侧固定 0.9，MCP 工具没有该参数）⇒ 该读数在真实数据上永不触发 | t249 |
| wiki_builds | 5 行中 2 行 `status='planned'` 且都 `dry_run=1`、`finished_at=NULL` ⇒ **`status='planned'` ⟺ `dry_run=1`（同一信息两份）**，且 `confirm_plan` 按 `dry_run` 取计划而非 `status` | t248 |
| **注入（闭环的另一半）** | 79 个 `context_injected` 事件 / 77 个 transcript 文件：runs-path **25**（`runs` 表正好 25 行 ⇒ 25/25 = 100%），chat-path **54**；**含 knowledge/wiki/entity 块的 = 0**；注入 108–2092 字符 vs 契约 total 4096（从未逼近上界） | t259 |
| 注入的生产调用点 | **只有 1 个**走契约（`runs.rs:747` → `render_run_injection` → `render_injection`）；**chat.rs 自写一套**：自己的 SQL（LIMIT 12）· 自己的预算 700 · 自己的块头 · 自己的哨兵 | t259 |
| 检索质量（harness） | fused **recall@1 0.6667 · recall@5 0.8667 · MRR 0.7500**（16 语料 / 18 查询 / 15 可答，hash embedder）；两条中文查询 `gold_rank=null`；三条无答案查询 top_score 全 = 1/61 | `cargo test -p ruagent-knowledge --test retrieval-quality` |

### 1.1 被读数推翻的前提（保留在此，不删）

| 我写的 | 实测 | 错在哪 |
| --- | --- | --- |
| 「`scan()` 只在手动 rebuild 时被调用，没有自动路径」 | **自动扫描一直在跑**（boot + 每 60s） | 把「没人往被扫的目录里放东西」误读成「没有扫描」——**扫描能力在，喂料路径不在** |
| 「episodes=6 因为蒸馏没写 episodes」 | episodes 由 `memory_write` 写，**distill 按设计就不写** | 把现象的归因给了一个不相干的机制 |
| 「distill_log 多数写 0 ⇒ 蒸馏基本空转」 | `=0` 可能是已去重，也可能是 agent 返回空 | **一个数被当成了它没回答的那个问题的答案** |
| 「仓库 docs/ 有 70 MB 可入库」 | 其中 **.md 只有 2.44 MB** | 对象集没有按「可入库」这个判据收窄 |
| 「召回日志在 #agents」 | 在 **#stats**（`App.tsx:48` 把 `#stats` 映到 `Agents.tsx` 导出的 `m.Stats`）；生产者是 **#memory** | 按「哪个文件里有这段代码」定位，而不是按「哪个路由渲染它」 |
| （t249 自纠）「wiki 空态缺失」 | 那是它自己注入的 payload 形状不对导致的整页空白 | 前提由被测对象之外的输入造成 —— 但那个空白本身可登记（无形状守卫、无错误边界） |

## 2 五条结构性问题（按影响排序）

**① 闭环断了：知识库对 agent 零贡献。**
召回在 99.3% 的查询里返回知识命中、100% 返回 wiki 命中；79 个注入事件里含知识块的 = **0**。
两条注入路径（`runs.rs` / `chat.rs`）都只查 memories。**建了一套没人用的检索。**

**② 主路径绕过注入契约，两个生产者 = 两份判据。**
`render_injection` 的生产调用点只有 1 个（runs）；`chat.rs` 自写 SQL / 预算 700 / 块头 / 哨兵 ⇒
chat 上没有 tag 块、没有可见截断、没有丢块计数，双预算在 chat 上不生效。两条路径**已经漂过一次**（块头措辞在 `a097c5e` 改写，旧 transcript 仍是旧措辞）。

**③ 没有喂料 —— 扫描器在跑，但没人往被扫的目录里放东西。**
`<root>/knowledge` 只有 4 篇手放的 md（3,516 B）。196 个会话的 transcripts、仓库 docs/ 的 2.44 MB md、154 条记忆、63 个实体，**一个都不进知识库**。
对照：sessions 631 行 vs documents 5 行。

**④ 没有质量读数 —— 唯一的相关度指标是错的。**
`top_knowledge_score` 是 RRF 名次分，574 行只有 3 个取值，与查询长度无关，观测最大值 = 上界的 99.19%；
而 UI 把它当相似度显示（同一行并列 `m 0.86`（余弦）与 `k 0.03`（名次分）⇒ 诱导「记忆比知识相关 26 倍」的误读）。
日志本身 96.5% 由两个自检探针填满。

**⑤ 没有纠正手段 + 自检污染被测对象。**
记忆只有 write / supersede，没有 DELETE；`memory/list` 强制要求 `store`；`confidence` 这个契约承诺的读数在真实数据上永不触发。
而 `ruagent doctor` 一次调用留下三行（memory + episode + entity）、再跑会 upsert 文档、`:364–560` 内 `DELETE/cleanup` 命中 0，并把 recall_log 灌成 96.5% 探针。

## 3 任务图（17 单；面板 8 人上限，无新建团队 —— 平台拒绝第二支团队，本代挂在 `panel-ui` 下）

```
读数阶段（全部完成）
  t245 tools        检索质量 harness                    [completed · 队长接管 attempt 3]
  t246 mem-core     记忆召回与注入读数                    [claimed]
  t247 retrieval    知识腿与实体腿查询构造读数             [completed · 队长代收]
  t248 contract-lead 语料入库与蒸馏读数                   [completed]
  t249 ui-audit     四页信息架构读数                      [completed]
  t259 contract-lead 注入链路端到端读数（闭环的另一半）     [completed]

实现阶段（文件级串行，平台强制 inScope 不重叠）
  t250 tools        检索腿修复：实体腿查询构造 + 逐腿证据   deps t245,t247  [claimed]
  t251 mem-core     记忆召回融合重写 + 生命周期 API + 日志溯源 deps t245,t246,t250
  t252 contract-lead 语料喂料闭环 + doctor 探针隔离 + wiki 状态 deps t245,t248,t250
  t260 mem-core     闭环缺口：知识/wiki 进入注入 + chat 回归契约 deps t251
  t253 ui-work      前端：逐腿证据 / 纠正入口 / 召回日志归位   deps t249,t251,t250,t252

验证
  t254 ui-audit     复核读数阶段        t255 ui-audit 验证 t251
  t256 ui-audit     验证 t250           t257 tools   验证 t252
  t258 ui-audit     验证 t253
```

**接口冻结**：t250 必须暴露逐腿检索入口（签名写进 output），t251 负责接线；`api.rs` 只有 mem-core 一个写者。

## 4 队长裁决

**t245（两次失败后由队长接管，attempt 3 完成）**
前两次形状相同（不编译 + 一张错误清单，零读数），直接机制是编辑方式：两次 python 锚点手术失败，从没建立编译循环。
换成精确匹配的单点替换后一次通过。修掉的四处里**三处是前两版都有的真缺陷**：
1. **判据对象集错位**：gold 取 document id、hits 是 chunk id，直接比 ⇒ 前两版的 `gold_rank` 全是错的（并用 `judge_is_falsifiable` 钉死）；
2. **JSON 里有 `elapsed_ms`** ⇒「连跑两次逐字节一致」这条判据每次都必然失败；
3. **在 harness 里重建 semantic/keyword 腿** ⇒ 仪器测量它自己那份逻辑的副本（裁决 ②）；
4. 输出写到相对路径 `target/` ⇒ 落进 `crates/knowledge/target`（已在盘上出现）。

**改派（执行面故障）**：t247 的负责人认领后无法自己提交（它的工作由没有 `agent_teams_*` 工具的 subagent 完成）⇒
t250 改派给 tools（它是唯一两次读进 crates/knowledge 内部的人），t256 随之改派给 ui-audit，避免作者验证自己。

## 5 纪律（写进每张单）

- 一个文件一个写者；inScope 外的文件不碰，要改先发消息给 captain。
- fmt/clippy/test 是全 workspace 的，可能因同伴在途编辑失败：报告「哪个文件、什么错」，不要替同伴改。
- 读数要带三件套：**对象集 + 采样面 + 可证伪判据**。「看起来一样」是论据不是读数。
- 判据单源：期望值只写一处，探针从函数取。
- 不许静默跳过；跳过要写明原因并出现在输出里。
- 测试用临时 root；`~/.ruagent/knowledge` 与 `~/.ruagent/data/ruagent.db` 是用户真实数据，只读用 `file:...?mode=ro`。
- 不许调用 `/api/v1/recall` 做取证（它会往 recall_log 写一行，那 574 行里 279 行已是探针残留）。
- 只杀自己记录过 PID 的进程；不许 taskkill/pkill/按端口批量杀；不许自己启停守护进程。
- 提交多行 message 用 `git commit -F`；不要 `git add` 同伴在编辑的整文件；不要 push。
- output 必须给「改前读数 → 改后读数」，不接受只写「已优化」。
## 6 修订 3（t246 / t250 之后）—— 本节取代 §2 与 §3

### 6.1 新增读数

| 项 | 读数 | 来源 |
| --- | --- | --- |
| 记忆两腿合并 | 5 条只被 semantic 命中 + 2 条只被 FTS 命中的记忆 ⇒ 返回 **7 条 > top_n=5**（上界 2·top_n） | t246 |
| 关键词条目的分数 | **不存在** —— aggressive 键 `[kind,id,store,namespace,content]`、conservative 键 `[kind,id,store,namespace,title,hint]` | t246 |
| 真实语料 15/15 | sem 恒等于 top_n、fts 0~2、**new 恒为 0** ⇒ 当前语料把上面两条掩盖了 | t246 |
| memory/list 未知 store | `store=bogus` → **200 + 13 行**（静默降级成 Observation） | t246 |
| memory/list 的 namespace | 默认 `user` ⇒ `store=lesson` → **0 行**，而库里 lesson/global 有 **38 条**（同一响应的 counts 显示 38） | t246 |
| chat 注入预算 | 700 **字节**（用的是 entry.len() 而非字符数）⇒ 符合条件 **115 行只发 4 条**，静默丢 111 行、渲染零提示 | t246 |
| 注入截断 | 契约路径 24/25 有 [+N chars truncated]、**0/25 有丢块通知**；chat 路径 0/54 有截断标记 | t246 |
| 实体腿 6 类矩阵 | autohotkey-v2 strict 0 → loose 1 · 潜艇 0 → 1 · 麒麟 0 → 1，其余 8 条无回归 | t250 |
| 逐腿原始分数 | 现在可取（semantic = LanceDB 距离、keyword = bm25），且融合排名与 search() 逐条一致 | t250 |

### 6.2 新被推翻的前提（接 §1.1）

| 我/成员写的 | 实测 | 错在哪 |
| --- | --- | --- |
| 「semantic 满额即丢弃 FTS 腿」（我写进 t251 契约） | FTS 的独占命中会被追加（5+2 ⇒ 7 条） | 把「去重后新增 0」误读成「被丢弃」；当前语料恰好把它掩盖 |
| 「前缀短语能命中 AutoHotkey」（tools 的机制判断） | 前缀化短语仍要求相邻同列 ⇒ 依然 0 命中 | 把「切词」的修复错记到「前缀」上，且是推断不是读数 |
| 「strict 必须 miss autohotkey-v2」（我的第一版测试） | 我造的实体叫 `AutoHotkey v2` ⇒ strict 命中，断言当场变红 | 合成语料没照活库的形状造 —— **判据的结论取决于采样面** |
| t245 前两次 / t250 前两次的形状 | 都是「不编译 + 一张错误清单，零读数」；直接机制是 python 锚点手术 | **问题在方法不在努力**：换精确单点替换后一次通过 |

### 6.3 任务图（18 单）

```
读数（全部完成）  t245 我接管 · t246 我代收 · t247 我代收 · t248 · t249 · t259
实现
  t250 检索腿修复（我接管 attempt 3）        completed
  t251 召回融合 + 生命周期 API + 日志溯源     claimed mem-core（契约按 t246 修正过）
  t252 喂料闭环 + doctor 隔离 + wiki 状态     in_progress contract-lead
  t260 知识/wiki 进入注入 + chat 回归契约     pending mem-core  ← 最高价值
  t261 知识库 keyword 腿（11/15 真实查询 0 命中） pending retrieval
  t253 前端：逐腿证据 / 纠正入口 / 日志归位    pending ui-work
验证  t254 ui-audit · t255 ui-audit · t256 ui-audit(in_progress) · t257 tools · t258 ui-audit
```

### 6.4 纪律补充（写进后续每张单）

- **改 Rust 一律精确单点替换 + 改完立刻 `cargo check --all-targets`，不要攒批**（三次失败都栽在这里）。
- **schema 只有 `crates/store/src/migrations` 一个真相源**；需要新列就申请，不要在别的 crate 另起一套（第二套 schema = 第二个真相源）。
- **执行会话可能没有 `agent_teams_*` 工具**：拿不到就立刻说，不要默默做完再交（t247 与 t246 各卡一次）。
- **历史行的新列是 NULL，而 NULL 不是 user**：渲染成「unknown（早于该列）」，不许按内容猜、不许静默过滤掉。
- 环境：`cargo` 需要 `protoc` 在 PATH 上（本机 `~/.protoc/bin`），`CARGO_TARGET_DIR=D:/rust_cache`。

## 7 环境危害（2026-09-26 实测，会制造假读数与假错误）

### 7.1 共享 CARGO_TARGET_DIR 的跨树产物串扰

同一个 `CARGO_TARGET_DIR` 下用**两棵不同的树**跑同一个包，cargo 会把**另一棵树**的产物交给测试：

- t256 实测：改后树的 `retrieval-legs` 报 `no method named search_legs`，`-v` 显示它链接的 rlib 里该符号计数 **0**（另一棵树的库）。给每棵树不同包版本后：改前链接 `c03582ee…`（计数 0）、改后链接 `223f9d34…`（计数 6）。
- t254 实测：`cargo test -p ruagent-knowledge` 报 `E0433 cannot find fts in ruagent_store` ×4 + `E0277 str` ×2，链接的是 `libruagent_store-d7f32265aa2a6468.rmeta`（mtime 19:20:29，正是两棵临时树的构建窗口），而更新的 rmeta（19:52:47）就在旁边；`cargo build -p ruagent-store` 单独跑无错。

**后果**：这组假错误把 contract-lead 的 t252 打成了 failed —— 一个**已完成、4/5 条有读数**的任务因为环境串扰被记成失败。

**对策**：① 任何「两棵树对比」的读数必须附**链接证明**（让读者能判断你链接的是哪棵树）；② 对比实验用**各自独立的 target dir**，不要共用；③ 已经污染时 `cargo clean -p <crate>`（不要整仓 clean）。

### 7.2 活库在自变

运行中的守护进程自己在写库（蒸馏）：复核期间实测 `recall_log 574 → 582`、`memories 154 → 164`。

**后果**：旧读数的**数值**会过期，**口径与取值集合不变**（t254 复核 t247 的取值集合仍是同 3 个值 301/278/3）。

**对策**：读数必须自带时间窗；引用旧数值时说明它是哪个时间点的。

### 7.3 全 workspace 的 verify 命令会把一个任务绑架给另一个任务（2026-09-26，t252 栽了两次）

t252 的 verify 里有 `cargo fmt --all --check`，而它是**全 workspace** 的：

- 第一次：`crates/knowledge/src/store.rs` 被 t261 在途编辑留下半成品（14 个未定义名字）⇒ t252 的第 5 条 verify 红 ⇒ 它只能如实报 failed。
- 第二次：mem-core 的临时探针 `crates/daemon/tests/t260_probe.rs` 有语法错误 ⇒ fmt exit 1 ⇒ 又一次把 t252 打红。

两次都不是 t252 的代码问题，两次都要靠**别人**去修。

**对策**：
1. **新任务的 verify 用限定范围的命令**：`cargo fmt -p <crate> -- --check`、`cargo test -p <crate>`，不要写 `--all` / `--workspace`。
2. **全 workspace 的检查是队长的集成步骤**，不是单个成员任务的验收条件。
3. **临时探针要么住在仓库外**（tools 在 t245 的先例：文件放 C:/tmp，仓库回到干净），**要么随时可编译**。住在 `tests/` 下的，先 `cargo check` 一次再往下写。

### 7.4 验证单的派发前置：被测改动必须已经进二进制（t257 的裁决）

t257（tools 独立验证 t252）的两个验收命令跑不了，而**两个阻塞是同一个**：

```
D:/rust_cache/debug/ruagent.exe  mtime 14:44（早于 t252）
$ ruagent.exe knowledge --help  ⇒ error: unrecognized subcommand 'knowledge'
```

更严重的是第二个后果：这份二进制**也早于 t252 的探针隔离** ⇒ 跑两次 doctor 就是**拿旧版本往用户真实库里写探针行** ⇒ tools 拒绝执行，并写明「拒绝本身就是这条验收的正确执行」。

**对策（已编码进平台，不是提醒）**：验证单的前置里加一条「重建二进制 + 切换」的任务（t273），验证单依赖它。

**平台缺口（记账）**：`create_task` 的 assignee 必须是活跃成员，**captain 不是活跃成员** ⇒ 队长的任务创建时只能留空；而 `reassign_task(assignee="captain")` 会被**未完成的前置**挡住 ⇒ **一个还没到执行时机的队长任务无法预先指派给自己**，只能在依赖完成的那一刻再收回。这是「对策依赖人记得」的形状，目前只能靠队长自己盯。

### 7.5 run_code 里的后台任务不会活过一次调用（2026-09-26，实测两次）

`nohup cargo build -p ruagent > /c/tmp/t-build.log 2>&1 &` 与 `nohup cargo test --workspace > … &` 两次都只留下 4–5 行日志、**既没有 `Finished` 也没有 error**，而进程表里再也找不到它们 —— 调用返回时进程树被收走了。

**后果**：
1. **长构建必须在前台跑**（单次调用上限 600s，必要时连着几次调用推进，cargo 是增量的）。
2. **不能把「后台跑测试、下一轮读结果」当计划** —— 这是「对策依赖人记得」的邻居：它依赖一个不存在的后台。
3. 判据：日志里没有 `Finished` 也没有 error，而进程表里没有它 ⇒ 它是被收走的，不是失败的。

### 7.6 `git add` 是全局状态：一次失败的 commit 会被下一个提交者继承（2026-09-26，实测）

实测序列：

1. 队长 `git add scripts/ruagent-canary.ps1 docs/plans/…` 成功，紧接着 `git commit` **失败**（`.git/index.lock` 被同伴的 git 占用）。
2. 队长当时没有 `git reset`，索引里留着那两个已暂存的文件。
3. 稍后成员提交自己的 7 个面板文件（它 `git add` 了自己那 7 个，然后 `git commit -F`）—— **`git commit` 提交的是整个索引** ⇒ 那 9 个文件一起进了 `29129d6`，提交信息却只讲面板。

**后果**：提交历史把两份不相干的工作记成一份，而成员以为自己只提交了自己的文件（它的自述是诚实的，错在索引）。

**对策（两条都要）**：
1. **提交时给出显式路径**：`git commit -F <msg> -- <你改的路径…>` —— 它只提交列出的路径，忽略索引里其余的东西。
2. **commit 失败就先 `git reset`**（清索引，不丢工作区改动），不要把一个脏索引留给下一个提交者。

**判据**：`git show --stat <commit>` 的文件数 == 你 add 的文件数；不等就是被继承的脏索引。
