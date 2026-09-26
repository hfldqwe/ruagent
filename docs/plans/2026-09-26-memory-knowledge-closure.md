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

### 7.7 平台机制三条（2026-09-26，都是实测踩出来的）

1. **队长的「收回」不跨轮存活**：`reassign_task(assignee="captain")` 的说明写着「未完成的收回会在队长空闲时回到成员池」——实测如此。队长**不能把一个任务停在自己名下跨轮**。后果：t273（重建）被收回后，下一个空闲成员就认领它、读到前置未满足、如实 failed，两次都是这样。
   ⇒ **前置必须写成【依赖】，不能写成验收里的一句话。** 写进验收的那版（「【前置】t276 必须已落地」）挡不住任何人，只是让每次认领都多一条 failed 记录。

2. **取消的依赖会把依赖方变成僵尸**：t273 被取消后，t274/t275/t277 报 `blocked by unfinished dependencies: t273` ⇒ **既不可认领、也不可收回**（收回被同一个依赖挡住）⇒ 它们永久占用 inScope（t275 占着 `cli/`，直接挡住了新单的创建）。

3. **队长可以直接取消【pending】的任务，但取消不了【已认领】的**：`update_task(status=cancelled)` 对 pending 单直接成功（t274/t275/t277）；对已认领的（t271/t272，in_progress）报 `owned by member X; call reassign_task with assignee=captain before takeover`。⇒ 清僵尸要在它们被认领之前做。

**共同教训**：平台的依赖图是唯一可靠的排序机制；凡是「应该等某件事」的地方，要么写成依赖，要么就会以「有人白跑一趟」的形式付账。

### 7.8 两条共享仓库事故（2026-09-26，都由当事人主动披露并修复）

**① `git commit --amend` 落在别人的提交上**：contract-lead 想把自己的 `crates/graph/src/lib.rs` fmt 修正补进**自己**的 commit `a33663d`，跑了 `git commit --amend --no-edit`；而这段时间里有人提交了，HEAD 已经是 t263 的提交 ⇒ 它的修正被并进 t263 的提交，**t263 的 hash 被改写成 `7daccd5`**（内容没丢，8 files / +219 / −34，message 保留）。它没有 push，并如实披露。

**为什么这次没造成损失**：那两个提交都还没 push（`origin/main..HEAD` 只有它们两条）⇒ 被改写的是一段还没公开的历史。**如果已经 push，改写就会伤到所有拉过它的人。**

**规则**：多写者仓库里【不要 amend】，也不要 rebase 已 push 的提交。要动历史之前先 `git log -1` 确认 HEAD 是谁的；`--amend` 只对「仍是 HEAD 且是你自己的、且未 push 的」提交安全 —— 而这个条件在并发提交下**随时会失效**。

**② `git worktree remove --force` 跟随 junction，删空了仓库自己的 `panel/node_modules`**：ui-audit 建临时工作树取「改前」面板读数，把 `panel/node_modules` 以 junction 指向仓库的 node_modules；收尾 `git worktree remove --force` 递归跟随了那个链接，把仓库的 `panel/node_modules` 删空（顶层 0 项）。它用 `npm ci --offline` 修复并**做了功能验证**（`npm run build` exit 0 · playwright/esbuild 在位 · `--list` = 41 tests in 11 files，与损坏前一致），影响窗口约 3 分钟。

**规则**：临时工作树里不要用 junction/symlink 指向共享目录；删除含 junction 的目录只能用 `rmdir`（只删链接），**不能**用 `git worktree remove` / `rm -rf`（会递归跟随）。

**共同点**：两条都是「一个看起来局部、实际全局」的动作（改 HEAD 的历史 / 删一个共享目录）。两次都由当事人**主动披露并给出修复后的功能验证**，而不是等别人发现 —— 这是它们没有变成事故的原因。

### 7.9 未提交的源码改动会污染别人的 before/after 读数（2026-09-26，第三次同一族）

实测：retrieval 的 t270 守卫在 **21:46 前已写进工作区（未提交）**；ui-audit 在 21:49:54 提交的 t286 结论说「t261 自报 18/18 被否证为 17/18，违例 1 条 = `how do I change a bicycle tyre`」。而 retrieval 用 **HEAD 的字节**复跑 BEFORE 得到 **18/18**（21:49:35 起）⇒ **那个 17/18 是别人的在途改动**，t261 的自报数字在它自己的代码上成立。ui-audit 找到的那条「违例」正是 t270 要清的 `do`/`i`/`a` 类噪声 —— **它独立测到了别人的修复，却记成了 t261 的缺陷。**

**这是同一族的第三次**：
1. 共享 `CARGO_TARGET_DIR` 的跨树产物串扰（t256/t254）—— 产物层面。
2. 工作区里别人在途的源文件（t276 的 fmt 红挡住 t252 的 verify）—— 编译层面。
3. 未提交的源码改动污染 before/after（t286 对 t261）—— **语义层面**。

**规则**：before/after 读数必须【钉住树】（给出提交 hash）并证明产物来自那棵树（链接证明 / rlib 符号计数 / 仪器 md5 逐字节相同）。**「当前工作区」不是合法的测量面** —— 多写者仓库里它随时包含别人的未提交改动。

**推论**：这也是「本单由队长执行、成员不要认领」这类写法为什么必要 —— 一个任务的**测量面**和它的**产物**一样，是要被声明和保护的东西。

### 7.10 链接证明不是提交证明（2026-09-26，ui-audit 自己纠出来的）

t286 的第一版结论说「t261 自报 18/18 被否证为 17/18」。它的证明当时是：链接到的 rlib 里 `KeywordStage` 出现 5 次 ⇒ 「链接的是 post-t261 的库」。

**那个证明不足以支撑它的结论**：它证明了【链接的是哪个库】，没有证明【那个库等于 23add4e 的字节】—— 而工作区里还有 t270 未提交的守卫。**链接证明与提交证明是两个不同的性质。**

它这次补强的证明是一箭双雕：钉住树的 rlib 里 `KeywordStage` = 5 **且** `MIN_RECALL_ASCII` = 0（没有 t270 守卫）⇒ 既证明了「是 post-t261 的」，也证明了「不含别人在途的改动」。

**规则**：证明被测产物时，要同时钉住【哪棵树】与【那棵树的哪一次提交】。前者靠链接证明，后者靠内容断言（某个只应出现在该提交里的符号/常量在场，某个不该出现的不在场）。

**它自己的结论**：「被测改动必须先进提交 —— 这次踩的是验证者，而且是我。」这条与 t257/t252 登记的是同一条，只是从产品的另一侧被踩到。

### 7.11 自述必须跟随能力，否则它会在能力具备之后继续说做不到（2026-09-26，t280 的 F1）

doctor 的清理报告行写着 `isolated (no delete route yet): entity __probe__doctor-node, memory agent:__probe__`。
**t276 交付了 `delete_entity` 与 `purge_memory` 及其路由之后，那句话仍然照旧打印。**

它是一句**写死在报告里的断言**，而不是一次**尝试的结果** ⇒ 它的真假与产品能力脱钩。

**对策**（tools 提的，比修法更重要）：报告行不要写「某条能力不存在」，而应当**尝试那个动作并报出结果**（`removed …` / `could not remove …: <原因>`）—— 那样它自己就会随能力变。

**同族**：本节的 7.12（我自己的 canary 脚本）是同一形状 —— **自述与现实脱钩**。

### 7.12 我自己的 canary 脚本：stop 报「already gone」而 daemon 还在跑（2026-09-26）

脚本用 `Win32_Process.Create` 返回的 pid 记录进程，而那个 pid 是 **`cmd.exe` 包装进程**：它 spawn 出 daemon 之后立刻退出。

实测后果：`-Action stop` 打印 `canary: pid 52736 already gone`，而 **8791 端口仍然回 200** ⇒ **每次 canary 都漏一个 daemon**，最后要靠命令行匹配 `ruagent-canary` 才把它找出来。

**修法**：daemon 自己会把 pid 写进 `<root>/data/daemon.pid` ⇒ 那是真相；并且在 kill 之前**要求目标的命令行里含这个 root**（绝不允许按进程名或按端口杀）。

**修完复测**：`start` ⇒ `healthy (200) on 127.0.0.1:8791, daemon pid 23420` · `status` ⇒ 同一个 pid + health 200 · `stop` ⇒ `stopped pid 23420` · 8791 ⇒ 000（不可达）· 8787 ⇒ 200（真 daemon 未受影响）。

**教训**：**在需要它的那一刻之前测试自己的工具**。这个脚本写出来时看起来对，而它错在「谁才是那个进程」——一个只有在真正用到时才会暴露的错误。

### 7.13 验证单如果需要现场造仪器，会超掉验证者的预算（2026-09-26，t281）

t281（独立验证 t260）报了 `needs_revision`：第 4 条 grep 完成、第 1 条部分完成，而 **第 1/2/3 条三条运行时读数一条都没取到** —— 因为仪器（真 chat + 真 run 的 context_injected 探针）要从零写，预算在写探针时耗尽。

它没有静默跳过：报告里写下了**可直接照做的配方**，并明确写了一句「**请勿把 needs_revision 读成 t260 有缺陷** —— 我已验的部分与它的声明一致，缺的是我的测量」。

**对策**：把**仪器**与**判定**分开。作者已经把探针留在仓外（`C:/tmp/t260/t260_probe.rs` + 输出），复验单就以它为起点，但要求验证者先做一次**仪器有效性检查**：改一处输入（换语料/换查询）并确认输出随之改变 —— 否则测的是它的仪器，不是产品。这样验证者的预算是花在判定上，不是花在造仪器上。

**7.11 的后续（同日，t282）：这条对策当场证明了自己。** contract-lead 把报告行改成「尝试删除并报结果」之后，第一版实现里有一个**双重删除缺陷**，那条报告行立刻打印成 `[FAIL] probe hygiene … FAILED to purge memory #164 … HTTP 404`；它用 `dedupe_by_id` 修掉后才 `all checks passed`。

⇒ 这比「改对了」更强：**那条报告行证明了自己会失败**。一条永远不会失败的检查，与一条写死的断言没有区别。

### 7.14 在后端失败时取的 UI 读数，测的是失败路径（2026-09-26，t258 vs t284 的矛盾）

t258 在 14:44 那个二进制上测「删除确认框按 Escape 不关闭」⇒ `dialogStillOpen=true`。而那一轮 `DELETE` 返回 **405**（路由还没上线）⇒ 确认动作走的是**失败分支**，弹层很可能停在错误/loading 形态。

t284 在切换后的二进制上三种焦点位置各测一次：Escape 都关闭、焦点回到触发按钮、REQ 全程为空（未触发删除）⇒ 三条性质都成立。

**规则**：一次 UI 读数如果是在【后端失败】的状态下取的，它测的是**失败路径的表现**，不是组件本身。要判断组件，必须先让被测路径成功一次。

**推论（与今天另一条同族）**：t263 的「逐腿证据上屏」也是在**拦截并注入一个它以为的形状**下取的 —— 那不是活体读数。**读数必须写明它的状态前提**：后端成功了吗？产物是哪一次构建？树是哪一次提交？

### 7.15 wiki dry-run 的 G1 是【我任务描述里的请求形状错了】（2026-09-26）

t294 报 G1（high）：一次 dry-run 存下来的行与普通构建无法区分（`dry_run=0` + `status='done'`），尽管请求带了 `?dry_run=true`。

**我在 canary（临时 root + 预置一篇知识文档）上做了决定性对照**：

```
A  查询串 ?dry_run=true + body {}      ⇒ 202 {"build_id":1,"status":"running","pages_planned":0}
B  body {"dry_run":true}             ⇒ 200 {"build_id":2,"status":"planned","pages_planned":1,"plan":[…] }
wiki_builds: [[1, 0, "running", null], [2, 1, "planned_only", "2026-09-26T14:11:19.412676100+00:00"]]
```

**B 产出的正是 t252 声称的终态：`dry_run=1` + `status=planned_only` + `finished_at` 有值** ✓。handler 读的是 `Json(req).dry_run`（`crates/daemon/src/api.rs` 的 `wiki_build`），**查询串不参与**。

⇒ **G1 不成立**；它是**我在 t294 的任务描述里写了错误的请求形状**（`?dry_run=true`），而验证者照着它做，读到的自然是普通构建。**A 那一行的 `dry_run=0` 与 t294 在活库上看到的完全一致** ✓。

**规则（今天的第 N 次同族）**：**任务描述也是一个测量面。** 描述里写错一个形状，验证者就会忠实地测出那个形状的结果，并把「我的错误」记成「产品的缺陷」。写验收/派单时，形状必须来自源码或一次实测，不能来自记忆。

### 7.16 「已提交」是一个需要 hash 的声称（2026-09-26，t284 的提交不存在）

t284（ui-work）报告「已用显式路径提交」，而 `git log -- panel/src/views/Memory.tsx` 里**没有它的提交**：

```
30a0790 t297: the recall-log total comes from retention.rows   ← ui-chat 的提交，含 t284 的两处改动
7daccd5 t263: …
```

t297（ui-chat）如实报了这件事：它动手前那两个文件**已是 modified**（t284 的未提交改动），它按显式路径把三个文件一起提交 ⇒ **t284 的改动被带走了**。

**两条规则**：
1. **提交后要读 `git log`/`git show --stat` 并把 hash 写进报告** —— 没有 hash 的「已提交」无法被检查（t297 给了 hash，t284 没给）。
2. **一个带着别人未提交改动的文件是「被占用的文件」**：第二个写者要么等第一个提交完，要么就会把两者混进一个提交。混进去时，像 t297 那样**主动报出来**是对的（它没有擅自回退别人的改动）。

**为什么这次不修历史**：内容完整（工作没丢），而 7.8 的规则是不要在并发仓库里改写历史。错的是**归属**，记在这里比动历史安全。

### 7.17 一个「已提交」是如何被证伪的，以及三条可机械判定的提交检查（2026-09-26，t284）

ui-work 的 t284 报了「已用显式路径提交」，而它**没有落盘**。它自己查清了，证据链：

- `git log --oneline -- <两个路径>` ⇒ 只有 `30a0790`(t297) 与 `7daccd5`(t263)，没有 t284
- `git log --all --grep=t284` ⇒ 只有一条 **docs** 提交，**没有任何 t284 代码提交**
- **`git reflog` 是决定性的**：22:04→22:13 全是别人的条目（`133e172` t282 · 四条 docs · **`30a0790` t297 22:11:18** · `62e9ae5` t290）⇒ **它的 commit 连 reflog 都没进** ⇒ 既没有成功，也不是被 amend 掉的（**amend 会留 reflog**）
- 内容确认：`30a0790` 的 stat 含那两个文件，且 `git show 30a0790 -- panel/src/views/Memory.tsx | grep -c t284` = **2** ⇒ 改动在别人的提交里（**工作没丢，丢的是归属**）

**它自己的归因**：命令是 `… ; git commit … | tail -1; git show --stat --oneline HEAD | head -4; …`，而它**只打印了输出的前 900 字，没有对内容做任何断言，也没有读退出码**。那条链里的 `git show … HEAD` 如果跑到了，打出来的会是**当时的 HEAD**（22:11:18 起就是 t297 的提交，3 个文件）⇒ **线索本来就在输出里，而它没读**。最可能的直接原因：`.git/index.lock`（那 9 分钟里有 5 次提交）⇒ `| tail -1` 把 fatal 吞进了被丢弃的输出；它明确标注这是**推断**而不是读数（它没有留 stderr）。

**三条可机械判定的提交检查（它给的，照此执行）**：
1. `git log -1 --format=%H -- <你 add 的路径>` 必须是一个**新** hash；
2. `git show --name-only --format= <该 hash>` 必须**恰好等于**你 add 的路径集；
3. `git status --porcelain <这些路径>` 必须为空。

⇒ 三条同时成立才算「已提交」；`git commit` 的**退出码与 stderr 必须读出来并进报告**。

**另外两条**：并发仓库里 `| tail -1` 会吃掉失败信息 ⇒ **不改写、不过滤 git 的输出**；**报告里的「已提交」必须带 hash**（t297 给了 `30a0790`，t284 没给）。

**归属而非历史**：内容在 `30a0790` 且已推送 ⇒ 按 7.8 不改写历史；归属只在这里记一笔 —— **`30a0790`（提交信息写的是 t297）同时含 t284 的 `panel/src/views/Memory.tsx` 与 `panel/src/i18n/memory.ts` 两处改动。**

### 7.18 一个报错的 finding 必须被显式撤回（2026-09-26，t294 的 G1）

任务记录是**终态且不可变**的：t294 永远停在 failed + needs_revision + G1。而 G1 被证伪了（见 7.15）。

⇒ tools 做了正确的事：把更正写进**记录在案的产物** `docs/design/reviews/t294-t252-wiki-dryrun.md`（G1/G2 标 **WITHDRAWN** + 我的 A/B 原文 + 更正后的最小形状 + 保留的 H1），并说明为什么写在那里。

**规则**：**一个验证者报错的 finding 如果不显式撤回，比不报更坏** —— 它会以「已验证的缺陷」的身份进入历史，而下一个人会照着它去修一个不存在的问题。

### 7.19 判据必须允许第三态：不可判定（2026-09-26，systems 对 t299 的提醒）

t299 的判据要问「某个字段有没有被断言得比后端更严」。systems 指出：**语料证不了「永不为 null」**，只能证明「当前语料里是 X」。

例：`title:null` 只在存在无标题页面时出现，而现在库里可能一个都没有 ⇒ 那一格在语料里**不可观测** ⇒ 若把「未观测到 null」当成「不可能 null」，它就犯了与 (a) 无前置时同一个错，只是换成验证器犯。

⇒ 判据写成三值：**(i) 语料观测到的类型/是否见过 null · (ii) 语料是否覆盖到「可能为 null」的情形（不可观测就明说）· (iii) 结论 = 更严 / 不更严 / 不可判定**。

**第三态必须允许存在，否则判据会逼出一个证不了的绿。** 而最强的 ground truth 在 Rust 侧的响应类型（字段是不是 `Option<T>`）—— 那比语料强，所以我把「只读 api.rs 的 Option 性」显式写进了 t299。

### 7.20 一个被静默忽略的参数，比一个被拒绝的参数更坏（2026-09-26，t294 的 H1）

`POST /api/v1/knowledge/wiki/build?dry_run=true` + body `{}` ⇒ **202 成功**，响应体甚至说 `pages_planned: 0`（dry-run 行为确实发生了），而落库是 `dry_run=0` / `status='done'` ⇒ **按文档写法的调用者拿到一个成功码和一行看起来像生产构建的记录**。

handler 读的是 `Json(req).dry_run`，**查询串被静默忽略**。

⇒ **拒绝会说话，忽略不会。** 而这是 tools 那一族里最隐蔽的一次：**一切看起来都成功了。**（已立单 t300。）

**7.18 的落点说明（tools 的收尾）**：t294 的任务记录会**永久**停在 `failed` + `needs_revision` + G1，而更正写在 `docs/design/reviews/t294-t252-wiki-dryrun.md` 顶部的 CORRECTION 块里。**顺序反了，就会看到一个已经被证伪的 high finding 挂在 t252 名下。**

⇒ 这条风险是有界的，因为 **t294 自己的 output 就写着那份产物的路径** —— 顺着指针走会先落到 CORRECTION 块。**这是「终态不可变的记录」与「可更正的产物」之间的唯一桥**：记录指向产物，产物承载更正。

### 7.21 第二次 amend 事故：这次改写了一条【已推送】的提交（2026-09-26，t298）

时序（reflog 是决定性的）：

```
8a1f54a HEAD@{0}: commit: docs(memory): 7.18 落点说明 …
41cbbe8 HEAD@{1}: commit (amend): t298: assert the element fields the views actually read   ← amend
c619139 HEAD@{2}: commit: docs(memory): 7.18 报错的 finding …                              ← 我的提交，已推送
7bb844b HEAD@{3}: commit: t298: assert …
```

ui-chat 为了修一个被「真实响应逐字回放」抓到的假红（`edges.src/dst` 写成 string，实测三个都是整数）跑了 `git commit --amend` —— **而当时 HEAD 是我刚推送的 `c619139`** ⇒ amend 把**我的 docs 提交折进了它的 t298 提交**（`41cbbe8` 的信息是 t298 的，内容含我的 26 行 docs），本地与远端分叉，`git push` 被拒（non-fast-forward）。

**恢复做法（不改写已推送的提交）**：
1. `git reset -q origin/main`（mixed：HEAD 回到已推送的 `c619139`，工作区不动）；
2. 把**内容**按显式路径重新提交在它之上 —— `b77b524`（t298 的迟到修复）+ `3ec7b15`（我的 docs）；
3. push（快进）⇒ `origin/main` 是 HEAD 的祖先 ✓。

**规则（在 7.8/7.17 之上收紧）**：amend 只在**两个条件同时成立**时安全 —— **(a) HEAD 是你自己的提交；(b) 它没有被推送**。两条都要在 amend 之前用 `git log -1` 与 `git log origin/main..HEAD` 查，而不是凭印象。

### 7.22 一个未被提交的产物不是记录（同日，同一轮恢复里发现）

恢复过程中 `git status` 露出：**八份验证报告在盘上但从未提交**（t257 · t280 · t283 · t283-addendum · t285 · t288 · t294 · t296）。

⇒ **一次 `git clean` 就会抹掉那些结论的全部证据。** 已由队长以显式路径提交（`c50812e`），信息里写明是成员自己的产物、只是没入库。

**规则**：验证单的产物如果只活在**工作区**里，那份验证就等于没有留档 —— 而工作区是全体写者共享的、随时被清理的地方。

### 7.23 契约的修订会改变完成载荷的形状（2026-09-26，t298 的闸门连拒三次）

闸门原话：`repair completion requires passed acceptanceResults for every acceptance item` —— 而它交的每一条都是 `passed`。

**原因**：我在它**已经读到契约之后**改了 t298 的验收（systems 提了「不可判定」，我加了 ground truth 与三值判据）⇒ 契约从 5 条变成 8 条，而它按旧条数提交 ⇒ 条数对不上。**闸门只说「每一条都要 passed」，没有说「你少交了」。**

**三条规则**：
1. 能改就**在成员开工前**改；开工后改，必须**明确告诉它重读**（平台文档说实施者会在下一次质量门之前重读被修订的契约 —— 但它不会自己知道条数变了）。
2. **契约条目内部不要出现列表分隔符**（这里怀疑是「；」）：一条含「；」的验收会被数成两条。
3. 闸门的报错说的是**形状**，不是**内容** —— 它拒绝时先数条数，再去怀疑值。

### 7.24 事件只证明「发出了」，不证明「收到了」（2026-09-26，t292）

mem-core 的 e2e 用例不只断言 `context_injected` 事件里有三块（memory/knowledge/wiki），**还断言 mock agent 回显的 prompt 里也含这三段文本**：

「**事件只证明守护进程发出了，回显才证明 agent 收到了。**」

⇒ 与 t260 的「渲染字节相同」是三层：**渲染证明构造，事件证明发送，回显证明送达**。一条只测到构造的判据，会在发送被截断时仍然绿。

### 7.25 「跳过」会被读成绿，所以歧义必须可拒绝（2026-09-26，t292）

mock-agent 二进制是那条测试的前置，而 `CARGO_BIN_EXE_*` 只对本包测试定义 ⇒ 它从 `current_exe` 推导路径。**它两面都实测了**：二进制在 ⇒ 2 passed 且无 SKIP 行；把测试二进制复制到没有 mock 的目录 ⇒ 打印 `SKIP … THIS TEST DID NOT RUN.`，**而 cargo 汇总仍是 2 passed**（Rust 里 early return 就是 pass）。

⇒ 它把这个歧义**显式化**：`RUAGENT_REQUIRE_MOCK=1` 时同一场景**失败**（实测 panic）⇒ **无人值守运行可以拒绝歧义，而不是把「跳过」读成绿。**

**规则**：一条可能静默跳过的测试，必须在无人值守时能把自己变成红 —— 否则它的绿什么也不保证。

### 7.26 清理器撞到列表上界时会静默留下对象（2026-09-26，t295）

contract-lead 在 sweep 里加了**页界/截断守卫**：`entity listing hit its page bound (500)` · `memory listing truncated: N of M rows`（用响应的 `matched` 对比）⇒ 都 `left += 1` ⇒ `[FAIL]`。

⇒ 与 doctor 的 `no delete route yet` 同族但更隐蔽：**一个「清完了」的自述，在它没清完时不会说话。** 现在它会。

**而 t295 的负例是这一单的骨头**：它造了 `t282p-plain` / `agent:t282p-plain` / `t282p-plain-doc` 三条【非前缀】对象，两轮都验证它们**留下**（1|1|1）⇒ 前缀没写宽；取证后把它自己造的三条也删干净（0|0|0）。
