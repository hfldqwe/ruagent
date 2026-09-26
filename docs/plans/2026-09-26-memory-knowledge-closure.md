# 知识库与记忆的闭环优化 — 诊断与任务图（2026-09-26）

> **修订 1（2026-09-26，t248 复核后）**：初版的四条前提被读数推翻，见 §1.1。
> 本文件里的每个数字都带复现方式；被推翻的前提保留在 §1.1 里，不删。

## 0 一句话

引擎算法不差（混合检索 + RRF + E5 查询/段落前缀 + 注入契约 + 命名空间治理都在），缺的是**闭环**：
**没有喂料、没有质量读数、没有纠正手段，而自检污染被测对象。**

优化顺序：**先造读数 → 修检索腿 → 修召回融合与生命周期 → 修语料闭环与自检隔离 → 最后才是 UI**。
UI 放最后是因为它现在展示的数字本身就是错的（把 RRF 名次分当相似度），先做 UI 只是把错的数字摆得更漂亮。

## 1 基线读数（2026-09-26，全部可复现）

| 项 | 读数 | 复现方式 |
| --- | --- | --- |
| knowledge 文档 / chunk | **5 / 12** | `curl /api/v1/knowledge/documents` |
| 磁盘知识目录 | **3,516 B / 5 文件**（含 `doctor-probe.md` 111 B 残留，占 3.2%） | `ls -R ~/.ruagent/knowledge` |
| 自动扫描 | **在跑**：`daemon/lib.rs` 的 `tokio::spawn` 循环，**boot + 每 60s**，SHA-256 增量；扫描面只有 `<root>/knowledge/**/*.md` | `sed -n '195,215p' crates/daemon/src/lib.rs` |
| 写入知识目录的路径 | 4 个 `save()` 调用点（`api.rs` raw PUT · `api.rs` ingest · `wiki.rs` ×2） | `grep -n 'save(' crates/` |
| 可喂料的字节 | transcripts **27.83 MB / 146**；`~/.claude/projects` **103.89 MB / 136**；仓库 `docs/` 70.45 MB / 665 **但其中 .md 只有 2.44 MB** | 磁盘字节 |
| 索引能力 vs 喂料 | sessions **631 行** vs documents **5 行** | 只读 sqlite |
| episodes（非丢失底座） | **6 行**，全 `kind='mcp_write'`、`source_run` 全 NULL = **4 条事实 + 2 条探针残留** | `SELECT kind,source_run FROM episodes` |
| episodes 的写入者 | `memory/episode.rs:60`，**唯一调用者 = `api.rs` 的 `POST /api/v1/memory/write`**；**distill 不写 episodes** | `grep -n episode crates/daemon/src/distill.rs` → 只有 `source_episode: None` |
| distill_log | **32 行 = 32 个会话**（`INSERT OR REPLACE ... session_key`，每会话一行、重跑替换） | `SELECT COUNT(*) FROM distill_log` |
| 蒸馏空转 | 三者全 0 → **18/32 = 56.25%**；`[distill] auto = true` | 只读 sqlite + 配置 |
| memories | **154 行**，**154/154 真实模型嵌入** | `SELECT embedder,COUNT(*) FROM memories GROUP BY embedder` |
| recall_log | **574 行 / 只有 15 个不同 query** | `SELECT COUNT(*),COUNT(DISTINCT query) FROM recall_log` |
| 探针占比 | kettle 279 + autohotkey-v2 275 = **554/574 = 96.5%**；kettle 间隔 min/median/max = **0.2 / 4.2 / 1417 分钟**，152 段 <5 分钟 ⇒ **成簇的探针调用**，不是定时任务 | 只读 sqlite |
| `top_knowledge_score` | 0.0325 / 0.0164 | `curl /api/v1/recall/log` |
| 实体腿 | 最近 574 条**全部** `entities=0` | 同上 |
| 实体 FTS 实测 | `MATCH "autohotkey-v2"` → **0 行**；`MATCH autohotkey` → 1 行 | 只读 sqlite |
| `GET /api/v1/memory/list` | 不带 `store` → **400**（missing field store） | `curl` |
| 记忆删除 | **不存在**（只有 write / supersede） | `grep 'route("/api/v1/memory' crates/daemon/src/api.rs` |
| wiki_builds | 5 行中 **2 行 `status='planned'` 且都 `dry_run=1`、`finished_at=NULL`** | 只读 sqlite |
| embedder | `fastembed:multilingual-e5-small`（真语义，非 hash 兜底） | `~/.ruagent/logs/daemon.log` |

### 1.1 被读数推翻的前提（初版写错了，保留在此）

| 初版的说法 | 实测 | 错在哪 |
| --- | --- | --- |
| 「`scan()` 只在手动 rebuild 时被调用，**没有任何自动路径**」 | **自动扫描一直在跑**（boot + 每 60s） | 把「没人往被扫的目录里放东西」误读成「没有扫描」——**扫描能力在，喂料路径不在** |
| 「episodes=6 是因为**蒸馏没写 episodes**」 | episodes 由 `memory_write` 写，**distill 按设计就不写 episodes** | 把「底座只有 6 行」的**现象**归因给了一个不相干的**机制** |
| 「distill_log 多数写 0 ⇒ **蒸馏基本空转**」 | `=0` 可能是**已去重**，也可能是 **agent 返回空**；该行只是最新一次结果 | **一个数被当成了它没回答的那个问题的答案** —— 这是本工作流当天反复出现的同一类错误 |
| 「仓库 `docs/` 有 70 MB 可入库」 | 70.45 MB 里 **.md 只有 2.44 MB**，其余 68 MB 是截图 | 对象集没有按「可入库」这个判据收窄 |

## 2 四条结构性问题

**① 没有喂料 —— 知识库是空的，但扫描器不是。**
扫描器 boot + 每 60s 在跑，只扫 `<root>/knowledge/**/*.md`；而那个目录里只有 4 篇手放的 md（3,516 B）。
196 个会话的 transcripts（27.83 MB）、仓库 docs/ 的 2.44 MB md、154 条记忆、63 个实体，**一个都不进知识库**。
对照：sessions 表 631 行 vs documents 表 5 行 —— **索引能力在，喂料路径不在**。

**② 没有质量读数 —— 唯一的相关度指标是错的。**
`top_knowledge_score` 是 RRF 名次分 `Σ 1/(60+rank)`，上界 = 腿数/61 ≈ 0.033；
它回答的是「几条腿命中了」，不是「命中得多好」。而 UI 把它当相似度展示（0.0325 会被读成 3% 匹配）。
更糟的是这个日志 **96.5% 由两个自检探针填满**，所以它连「有几条腿命中」都只回答了探针的问题。

**③ 没有纠正手段 —— 错记忆删不掉。**
记忆只有 write / supersede，没有 DELETE；一条写错的记忆只能被「取代」，永远留在库里并继续参与检索。
`memory/list` 还强制要求 `store`，所以「把所有记忆列出来看一眼」在 API 层就不成立。

**④ 自检污染被测对象 —— `ruagent doctor` 往生产语料写永久垃圾。**
一次 doctor 调用留下**三行**：`memories`（id 112）+ `episodes`（id 3，经 `memory_write`）+ `entities`（`doctor-node`，id 53）；
再跑一次会 upsert 文档行（`doctor-probe`，09-23），**换文案就会再加一份**；`:364–560` 内 `DELETE/cleanup` 命中 0 ⇒ **无清理**。
它同时把 recall_log 灌成 96.5% 探针，而它自己**看不见蒸馏**（源码里写着 skip quietly）。

## 3 任务图（15 单；面板 8 人上限，无新建团队 —— 平台拒绝第二支团队，本代挂在 `panel-ui` 下）

```
读数阶段（并行）
  t245 tools        检索质量 harness：黄金查询集 + 逐腿读数        [第 2 次尝试，见 §4]
  t246 mem-core     记忆召回与注入读数：关键词腿是否被丢弃 / 两腿分数可比性 / 注入预算
  t247 retrieval    知识腿与实体腿查询构造读数：FTS 命中矩阵 / RRF 上界 / 阈值丢弃了什么
  t248 contract-lead 语料入库与蒸馏读数                        [completed]
  t249 ui-audit     四页信息架构读数：展示的数字与真实含义 / 召回日志位置 / 纠正入口缺失
  t259 contract-lead 召回是否真的进了 agent 上下文：注入链路端到端读数（闭环的另一半）

实现阶段（文件级串行，平台强制 inScope 不重叠）
  t250 retrieval    检索腿修复：实体腿查询构造 + 逐腿证据与分数语义   deps t245,t247
  t251 mem-core     记忆召回融合重写 + 生命周期 API + 召回日志溯源     deps t245,t246,t250
  t252 contract-lead 语料喂料闭环 + doctor 探针隔离 + wiki 状态诚实化  deps t245,t248,t250
  t253 ui-work      前端：逐腿证据 / 纠正入口 / 召回日志归位 / 数字含义 deps t249,t251,t250,t252

验证
  t254 ui-audit     复核读数阶段 t245–t249（对象集 + 采样面 + 判据）
  t255 ui-audit     独立验证 t251        t256 tools  独立验证 t250
  t257 tools        独立验证 t252        t258 ui-audit 独立验证 t253
```

**接口冻结**：t250 必须暴露逐腿检索入口（签名写进 output），t251 负责接线；
`crates/daemon/src/api.rs` 只有 mem-core 一个写者。

## 4 队长裁决（t245 第一次失败后）

t245 如实报 failed 而不是把「写完了」当「做完了」——230 行、语料与查询集都在，但它不编译，
且它发现修 JSON 需要改 `crates/knowledge/Cargo.toml`（在它的单文件 inScope 之外），于是没有越权。三条裁决：

1. **批准加 `[dev-dependencies] serde_json`**，理由不是「不多一个依赖」，而是**判据的载体不许有自研转义**：
   逐字节一致这条判据压在序列化上，而它要为中文内容自己处理转义。serde_json 在本 workspace 早已编译，零新增成本。
2. **禁止在 harness 里重建检索腿。** 那会让仪器测量它自己那份检索逻辑的副本 —— 今天已栽过一次同类（审计工具在测自己）。
   fused 读数必须来自产物 `Knowledge::search()`；产物暴露腿之前，逐腿列写 `ABSENT` + 原因（不静默）。
3. 修 4 类编译错误，并改掉它自报的两处方向错误：加「来源腿」字段、`FastEmbedder` 失败时把 keyword 腿缺席写成输出字段。

## 5 纪律（写进每张单）

- 一个文件一个写者；inScope 外的文件不碰，要改先发消息给 captain。
- fmt/clippy/test 是全 workspace 的，可能因同伴在途编辑失败：报告「哪个文件、什么错」，不要替同伴改。
- 读数要带三件套：**对象集 + 采样面 + 可证伪判据**。「看起来一样」是论据不是读数。
- 判据单源：期望值只写一处，探针从函数取。
- 不许静默跳过；跳过要写明原因并出现在输出里。
- 测试用临时 root；`~/.ruagent/knowledge` 与 `~/.ruagent/data/ruagent.db` 是用户真实数据，只读用 `file:...?mode=ro`。
- 只杀自己记录过 PID 的进程；不许 taskkill/pkill/按端口批量杀；不许自己启停守护进程。
- 提交多行 message 用 `git commit -F`；不要 `git add` 同伴在编辑的整文件；不要 push。
- output 必须给「改前读数 → 改后读数」，不接受只写「已优化」。