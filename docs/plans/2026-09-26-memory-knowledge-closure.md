# 知识库与记忆的闭环优化 — 诊断与任务图（2026-09-26）

## 0 一句话

引擎算法不差（混合检索 + RRF + E5 查询/段落前缀 + 注入契约 + 命名空间治理都在），缺的是**闭环**：
**没有入库源、没有质量读数、没有纠正手段，而自检污染被测对象。**

所以顺序是：**先造读数 → 修检索腿 → 修召回融合与生命周期 → 修语料闭环与自检隔离 → 最后才是 UI**。
UI 放最后是因为它现在展示的数字本身就是错的（把 RRF 名次分当相似度），先做 UI 只是把错的数字摆得更漂亮。

## 1 基线读数（2026-09-26，全部可复现）

| 项 | 读数 | 复现方式 |
| --- | --- | --- |
| knowledge 文档 / chunk | **5 / 12** | `curl /api/v1/knowledge/documents` |
| 其中探针文档 | 1 篇（`doctor-probe`） | 同上 |
| 磁盘知识目录 | 4 篇 md + `wiki/` 2 篇 | `ls -R ~/.ruagent/knowledge` |
| `Knowledge::scan` 调用点 | 仅手动 `POST /api/v1/knowledge/rebuild` | `grep -rn 'files::scan\|ScanReport' crates/` |
| episodes（非丢失底座） | **6 行** | `SELECT COUNT(*) FROM episodes` |
| distill_log | **32 条**，多数 `memories_written=0` | `SELECT * FROM distill_log` |
| memories | **154 行**，**154/154 真实模型嵌入** | `SELECT embedder,COUNT(*) FROM memories GROUP BY embedder` |
| recall_log | **574 行 / 只有 15 个不同 query** | `SELECT COUNT(*),COUNT(DISTINCT query) FROM recall_log` |
| 其中探针占比 | kettle 279 + autohotkey-v2 275 = **96.5%** | `SELECT query,COUNT(*) ... GROUP BY query` |
| `top_knowledge_score` | 0.0325 / 0.0164 | `curl /api/v1/recall/log` |
| 实体腿 | 最近 574 条**全部** `entities=0` | 同上 |
| 实体 FTS 实测 | `MATCH "autohotkey-v2"` → **0 行**；`MATCH autohotkey` → 1 行 | 只读 sqlite |
| `GET /api/v1/memory/list` | 不带 `store` → **400**（missing field store） | `curl` |
| 记忆删除 | **不存在**（只有 write / supersede） | `grep 'route("/api/v1/memory' crates/daemon/src/api.rs` |
| wiki_builds | 5 条中 **2 条停在 planned**（`dry_run=1`、`finished_at=NULL`） | 只读 sqlite |
| embedder | `fastembed:multilingual-e5-small`（真语义，非 hash 兜底） | `~/.ruagent/logs/daemon.log` |

## 2 四条结构性问题

**① 没有入库源 —— 知识库是空的。**
`~/.ruagent/knowledge/` 里只有 4 篇手放的 md。`scan()` 只在手动 rebuild 时被调用，没有任何自动路径；
196 个 session 的 transcripts、仓库 docs/、154 条记忆、63 个实体，**一个都不进知识库**。

**② 没有质量读数 —— 唯一的相关度指标是错的。**
`top_knowledge_score` 是 RRF 名次分 `Σ 1/(60+rank)`，上界 = 腿数/61 ≈ 0.033；
它回答的是「几条腿命中了」，不是「命中得多好」。而 UI 把它当相似度展示（0.0325 会被读成 3% 匹配）。
更糟的是这个日志 **96.5% 由两个自检探针填满**，所以它连「有几条腿命中」都只回答了探针的问题。

**③ 没有纠正手段 —— 错记忆删不掉。**
记忆只有 write / supersede，没有 DELETE；一条写错的记忆只能被「取代」，永远留在库里并继续参与检索。
`memory/list` 还强制要求 `store`，所以「把所有记忆列出来看一眼」在 API 层就不成立。

**④ 自检污染被测对象 —— `ruagent doctor` 往生产语料写永久垃圾。**
每次 doctor 会永久写入：`doctor-probe` 文档、`doctor-node` 实体（id 53）、`kettle` 记忆（id 112），**且无清理**；
同时把 recall_log 灌成 96.5% 探针。而 doctor 自己**看不见蒸馏**（源码里写着 skip quietly）。

## 3 任务图（19 单，面板 8 人上限，无新建团队 —— 平台拒绝第二支团队，本代挂在 `panel-ui` 下）

```
读数阶段（并行）
  t245 tools        检索质量 harness：黄金查询集 + 逐腿读数        [重试中，见 §4]
  t246 mem-core     记忆召回与注入读数：关键词腿是否被丢弃 / 两腿分数可比性 / 注入预算
  t247 retrieval    知识腿与实体腿查询构造读数：FTS 命中矩阵 / RRF 上界 / 阈值丢弃了什么
  t248 contract-lead 语料入库与蒸馏读数：scan 调用点 / 可入库字节 / 蒸馏空转 / doctor 副作用
  t249 ui-audit     四页信息架构读数：展示的数字与真实含义 / 召回日志位置 / 纠正入口缺失

实现阶段（文件级串行，平台强制 inScope 不重叠）
  t250 retrieval    检索腿修复：实体腿查询构造 + 逐腿证据与分数语义   deps t245,t247
  t251 mem-core     记忆召回融合重写 + 生命周期 API + 召回日志溯源     deps t245,t246,t250
  t252 contract-lead 语料入库闭环 + doctor 探针隔离 + wiki 状态诚实化  deps t245,t248,t250
  t253 ui-work      前端：逐腿证据 / 纠正入口 / 召回日志归位 / 数字含义 deps t249,t251,t250,t252

验证与评审
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