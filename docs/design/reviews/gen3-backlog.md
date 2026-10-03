# gen3 backlog — ruagent 待完善项总账（captain 维护）

**这份文件的作用**：把「持续发掘 → 优化」的入口与在办事项集中在**一处**，使每一轮都有唯一的去处可查、可对账。逐条以**各任务报告**为准；本文件只做**索引与状态**，不替代证据。

## 0. 工作方式（intake → 任务 → 合并门）

1. **intake**：审计或任何成员交回一条 finding 时必须带 —
   `id` · 区域 · 严重度 · **`file:line`** · **可复现命令** · **证据读数** · 「为什么重要」· **可证伪的修复判据**（修好后能跑出什么读数）· 建议 owner。
   缺「可复现命令」或「可证伪判据」的条目**不进实现队列**（先补证据，或进 §4 未验证猜想）。
2. **分诊**：captain 把 finding 变成实现单，写进质量契约（objective / acceptance / verify / inScope），并与其它单**串行化**（同一成员、同一路径一次一个写者）。
3. **合并门**：**CI 绿** + 独立验证 + 评审。读数按契约 §6.0 第 22 条形态报（逐 target `test result: ok`/`FAILED`/`panicked` **行数** + 退出码，**不用求和**）。
4. **状态词**：`open` · `in-task (tNN)` · `deferred (owner+前置)` · `wontfix (理由)`。**不许**用 `not_measured` 掩盖「未达成」。

## A. gen2 遗留（已登记，带 owner 与前置）

| # | 事项 | 前置 / 原因 | owner | 状态 |
|---|---|---|---|---|
| A1 | 召回侧 C3：人工分级 gold（≥60 查询，含 ≥20 不可答） | **需要人类标注**（`judged_by='human'` 今天 0 行） | 你我 + 下一代 recall | deferred |
| A2 | G7 实体族 P≥0.85 / R≥0.70（20 段会话） | **需要人工标注的会话语料** | 你我 + 下一代 graph | deferred |
| A3 | 关系 recall ≥0.60 + 幻觉率 ≤0.10（R-4） | 需**一次真实重蒸馏跑批**产出带 `source_episode` 的边集（活库 0/67） | 下一代 graph | deferred |
| A4 | G4 `extracted ≥30%` | 挂在 A3 的 gold 上；gold 落地前不判 | 下一代 graph | deferred |
| A5 | G8 三指标成对判决 ≥60% | 需 **LLM 判决跑批** | 下一代评测 | deferred |
| A6 | `wiki_citations` 表未被使用 + 反查索引 | 无消费者写路径 | 下一代 wiki | deferred |
| A7 | `selected_by` 在活库 16/21 行为 NULL | 数据/运维侧回填 | 运维 | deferred |
| A8 | `episode::episode_count` 不分 `kind`（可删除候选） | 今天无调用点；**任何「蒸馏发生过」的代理必须按 kind 过滤** | 下一代 mem | deferred |
| A9 | `GRAPH_PATHS = 3` 的取值依据 | 未实测最优；t63 的块内行匹配仪器缺陷导致取不到「3 条够不够」 | 下一代 graph/INT | deferred |
| A10 | §3.3 Q2「丢的是哪个 tag」 | `blocks[]` 只列**发出**的块；t64 正在二选一处置 | t64 | in-task |
| A11 | `crates/knowledge/src/lib.rs` 的除零 ms/row 与采样上限 | recall 侧遗留，未立单 | recall | open |
| A12 | `crates/acp/src/adapter.rs:160` 条件跳过（`dsh` 不在 PATH 就 skip） | 无主路径（acp 侧） | 下一代 | open |
| A13 | A-4 MCP stdio 未被独立驱动 · A-12 文献注（`knowledge_api.rs` 调 `/recall` 未注明会写 `recall_log`） | INT/文献面 | 下一代 INT | open |
| A14 | 面板 e2e 的绿依赖根的数据形状（F8） | 测试基建；4 条红可归因、非面板缺陷 | 下一代 | open |
| A15 | 「断言恒真但**没有** `return`」这一类全仓未扫 | 本代只修了 store 侧已知实例 | **t70 审计** | in-task |
| A16 | `cli/` 无属主（含 `cli/src/main.rs:809` 那处作用域限定 `#[allow]`） | 已在 `AGENTS.md` 登记为 intentional | 下一代 | deferred |
| A17 | 交付账：`t45–t48` 被平台记为「failed 且无后续 repair」 | 实为同一件集成的连续部分交付（内容已被 t50/t57 等吸收） | captain | open（见 §C） |

## B. 审计 intake（新阶段「持续发掘」第一批）

| 单 | 区域 | 报告 | 状态 |
|---|---|---|---|
| t67 | `crates/graph/**` + `daemon/src/distill.rs` | `gen3-audit-graph.md` | 在跑 |
| t68 | `daemon/src/wiki.rs` + wiki 测试/面板 | `gen3-audit-wiki.md` | 在跑 |
| t69 | `crates/memory/**` + `memembed.rs`（对抗输入与边界） | `gen3-audit-memory.md` | 在跑 |
| t70 | **全仓测量面**（静默跳过/恒真断言/陈旧期望/CI 覆盖缺口） | `gen3-audit-instruments.md` | 在跑 |
| t71 | **安全/隐私/耐久**（凭据/SQL/路径/迁移/进程） | `gen3-audit-security.md` | 在跑 |

审计单的共同硬要求：**只读**（只写自己的报告）、每条带证据与可证伪判据、最多 12–15 条、猜测另列、**写清未覆盖范围**。

## B2. 第二波 intake 已立案（t67–t71 审计产出）

| 单 | 来源 | 事项（每条都带审计给的可证伪判据） | owner |
|---|---|---|---|
| t72 | t69 **F1+F2**（high） | 软删除的行堵死同内容重写（**静默数据丢失**）+ `supersedes` 静默 no-op 与**说谎审计** | mem-core |
| t73 | t69 **F3**（high） | 公开 FTS 吃原始用户文本 ⇒ 净化收进 `crates/memory::query` 共享层 + 负控 | mem-core |
| t74 | t68 **A1+A2**（high） | 正文锚点不参与 freshness/uncited（同一处断链两种读数）+ `stale_since` 无起点 | wiki |
| t75 | t71 **A-1**（high） | 遗忘的残余面**不含备份副本** ⇒ `ResidualSurface` 扩面 + C5 判据（**captain 裁决：备份属结论面**；本单只检测/报告，不做破坏性动作） | mem-core |
| t76 | t71 **A-3**（medium） | 非 loopback 绑定必须显式解锁（默认值不变）+ 日志点名暴露面 | integ |
| t77 | t70 **F6**（medium） | 两条**恒真断言**（`migrations.rs:1285/:1286` 自比较）换成真 before/after + **能红的负控** | recall |
| t66（已 amend） | t70 **F1+F2**（high） | 提交前必须把测量硬化与**未跟踪的** `.github/workflows/scripts/test-evidence.sh` **同一次**提交（否则 evidence 步直接红、fresh checkout 静默跑旧 CI）+ 新增「workflow 引用的路径必须在 `git ls-files` 里」自检；提交版不得再有 `cargo test.*||` | recall |

## B3. 已登记待派（`open`，按投产比排序；每条均在对应审计报告里有六要素）

**来自 t69（记忆）**：F4 正文可伪造块标签与截断/丢块词表（**冻结面改动 ⇒ 必须单独成单 + 更新冻结清单 + 全量 before/after**，且让「数块头/找截断标记」这类**验证方法本身**不可靠） · F5 `run_turn_failed` 不在 `EpisodeKind`（只以裸 SQL 存在） · F6 episode 去重只看 content hash ⇒ **kind 由第一个写入者决定** · F7 `confidence` 越界静默夹紧、NaN 报 NOT NULL · F8 `per_block` **不是块大小上界**（只有 `total` 是硬界） · F9 `decay_score` 对非有限 base 无守卫 · F10 用量列不在 `MemoryRow`（E0609）+ `record_usage` 会更新已取代的行 · F11 `tag_rank` 精确匹配、无未知 tag breadcrumb。

**来自 t68（wiki）**：A3 纠错端点不校验 slug（**已并入 t59 的 F6 验收**） · A4 `corrections()` 静默丢弃无法解析的行 · A5 同一事实两份记录（`wiki_pages.content_hash` **只写不读**） · A6 `/wiki/pages` 每页成本线性增长（3→28→53 页 = 30.2→73.7→157.8ms）而面板 3s 轮询 —— **需先定预算才有判据** · A7 闸门词表只有中文（`starts_with("来源")`）⇒ 写 `## Sources` 的页永远不可能通过，且该约束**只存在于代码里** · A8 面板原因字段 0 命中（`stale_reasons`/`stale_since` 都不显示） · A9 `citations` 不去重 · A10 冻结判定按 `at` **字符串**排序且 `at` 可由调用方给出 · **K-1**（猜想，最要紧）：构建后手改正文删掉锚点，`cite_coverage` 是否仍 1.0？需构建行/mock distiller 才能取证，建议补测试。

**来自 t71（安全/隐私/耐久）**：A-2 agent stderr **整行不脱敏**进日志（`acp/run.rs:80`、`chat.rs:217`；默认 filter 关着，`RUST_LOG=debug` 一开就全量落盘，且**日志不在任何清理面上**） · A-4 工作流 `uses:` 共 **19 处、SHA 固定 0 处**；`release.yml` 持 `contents: write`；`ci.yml` **无 `permissions:`** · A-5 全仓 **0 命中** `integrity_check|quick_check`、`VACUUM INTO` 只在测试里、`doctor` 是 HTTP 版（不开库）⇒ **DB/WAL 损坏 = 起不来、无检测、无恢复路径** · A-7 `orphans.rs:83-100` 活性检查与子进程枚举之间的 **TOCTOU**（全仓唯一可能杀到非记录进程的形状） · A-8 `wiki.rs:3400` 动态列名 `&str` · A-9 `knowledge/files.rs:145-152` 注释说不跟随符号链接，但**指向文件的 `.md` 符号链接会被索引** · A-10（仅测试）session key 插进带引号的 SQL 字面量。

**来自 t70（测量面）**：F3 env 门控的**测量分支在 skip 记账之外**（`retrieval-quality.rs:927` 的 `RUAGENT_T245_REAL`：默认 hash embedder，「真 e5」那一侧没有任何门禁会跑） · F4 CI 只跑设计工具的 `--self-test`（判据代码的定理证明），**§12 真正的测量 `--check` 只在本地**且 coverage declaration 没声明它 · F5 Playwright **无 PR 闸门**（`on: push [main]`）且除 `registry.spec.ts` 外任何 skip 都不失败 · F7 四个 **tracked 仪器全仓 0 提及**（`capture-readiness`/`interaction-probe`/`memory-kb-readings`/`t150-blank-control`） · F8 **83 条**把 `len()/count()` 钉成字面量的断言（31 文件） · F10 写路径闸门挂在**文件名字面量**上（改名即失去闸门） · F11 ignored 清单是「总数机器核对 + 逐名散文」（改名/挪窝能通过） · F12 PR 从不跑 Windows Rust（**已声明**的缺口）。

### 已证伪 / 已核验干净（**别重查**）

- **t68 已证伪 7 条**：大小写差异 fail-closed · `reconcile_page_hashes` 不抹手改证据 · 重复锚点不能绕闸门 · 正文 `#0`/负数不能绕 · 部分 chunk 变化有双层检测 · 两端点度数不矛盾 · RV-D-1 本批无反例。
- **t69 阳性 7 条**：对抗输入下「有界·带 tag·截断可见」全部成立（1M 正文/2000 条目/零宽+星平面+NUL/退化预算）· 冻结面在 **crate 外**重算与 crate 内 golden 逐字节一致 · **适配器唯一性成立** · 审计 op 词表与 `write_vocabulary()` 一致 · `store_counts` 确是当前行计数 · 衰减四处边界正确。
- **t70 防伪证 5 条**：`npm run check` 不是覆盖缺口 · `e2e_daemon.rs` 陈旧期望**已修好** · `recall.spec.ts:32` 是**条件**跳过 · `registry.spec.ts` 在 npm 入口会跑 · `smoke` 特性/目标存在。
- **t71 干净清单 10 行**：生产路径**没有 SQL 注入面** · 文档名/wiki slug 校验严格 · 路径遍历被代码+测试挡住 · 面板无 XSS 注入点 · `~/.ruagent` ACL 只给 SYSTEM/Administrators/属主 · 脚本只按记录的 PID 杀。



## B4. 第三波 intake（t67 图谱审计）已立案 / 待派

| 单 | 来源 | 事项 | owner |
|---|---|---|---|
| t81 | t67 **#1+#2+#6（同根因：多步写入没开事务）** | 蒸馏失败不留半成品 · `build_communities` 的层级替换真的原子 · `merge_entities` 八条语句同事务（三处都要「注入失败后读产物」+ 成功路径负控） | graph |
| t82 | t67 **#3** | `facts_as_of` 把时刻**当字符串比**（同一时刻三种写法 **38/30/43** 条边，解析正确值 30；API 形状下 6/63 个实体条数不同） | graph |
| t83 | t59 未交付项 + **captain 键名裁决** | `log`→`rows` **三处同改**（`api.rs` + `crates/daemon/tests/**` + `Memory.tsx:82`；只改 api.rs 必红已实测）· R-1..R-4 · F6（非空 `author` + 0 页根）· F3/F4 版本读点 · 报告 §20 | integ |

**已登记待派（t67 其余）**：`retrieve()` 每次读**全边表**（`EXPLAIN` = `SCAN e`；67→670→6700 边放大后 SQL 0.0002→0.0025→0.0220s；「bounded by construction」只对**输出**成立） · 合并相连实体会造**自环**且不转移社区成员（`covered != non_isolated`） · `merge_entities` 注释与实现不符 · **错误被吞成误阴**（`lib.rs:946/953` 把任何 DB 错误当 no-match ⇒ 静默造重复实体；`distill.rs:432-455` 把补偿失败吞成 `false`） · `build_evidence` N+1（每节点一次 `query_row`） · `resolve_seeds` 循环里 `prepare`（全 crate `prepare_cached` **0 处**） · 两条种子腿全表扫 · `redundant_pairs` **O(N²)**（63 实体 ⇒ 1953 次判据，**且在 HTTP 端点上**） · `base_name` **0 个调用者**。

**t67 已证伪 4 条（别重查）**：`list_entities` 的 `OR` 实际走 `MULTI-INDEX OR` · `hops` 在 `api.rs:1260` 被 `.min(4)` 封顶 · 产品路径 **0** 个 unwrap · `log_outcome` 已是普通 INSERT（0024 的警告已兑现）。

## B5. 第四波 intake（t79 面板审计 / t80 规格漂移审计）

| 单 | 来源 | 事项 | owner |
|---|---|---|---|
| t84 | t79 **P2+P3** | 面板**失败可见性**：召回日志读取失败不得显示成「没有记录」· 模型探针失败不得显示成「0 models」 | integ |

**并入既有单**：**t83** 的 R-3 现在带 **P1 的双向读数** —— `observation×global` 服务器**永远拒绝**却可选、`procedure/lesson` **漏了 `project:<x>`**（服务器允许但 UI 选不到）⇒ 判据是**双向**的：**UI 可选集合 ⊆ 服务器接受集合**，且 `project:<x>` 必须有输入路径。

**已登记待派（t79 其余）**：P4 `Settings.tsx` 是 13 个路由里**唯一零 e2e 覆盖**（却承载 `PUT /distill` 等写路径）· P5 9 个轮询点里 **5 个不因标签隐藏而停**（Wiki 3s、**TaskDetail 2s**、Sessions/Home 10s、Chat 3/5s）⇒ 一个隐藏标签即 ≥50 请求/分，多视图叠加在结构上看不到 · P6「自定义 prompt」徽章是**文本比较**（因为 `/distill` 还没有 `prompt_hash` ⇒ 随 t83 的 F3 一起解）· P7 6 条 `eslint-disable` **抑制的是空气**（无 eslint 依赖/配置/lint 脚本）⇒ 恰好 6 处 hooks 依赖无机械门禁 · P8 来源筛选硬编码 `["probe","distill","user"]` + 当前页出现的 source（**筛选器与被筛数据同源**）· P9 19 个 `test.skip` 点 / 9 个 spec ⇒「套件绿」= 有数据的那些 spec 跑了。

**t80 漂移（规格 vs 实现）**：**D-2** 五份规格共 **80 处 `crates/….rs:NNN` 引用**、机械扫描 55 处可疑、语义复核 **≥8 处真漂移**（最远 `wiki.rs:1482 → 3181`，**+1699 行**）⇒ 建议**一次性脚本化勘误**（每条引用用锚词断言该行）· **D-3** graph-spec:165「未知 tag 返回 **5**」vs today `inject.rs:334 _ => 6` · **D-4** graph-spec G9 判据落后于**迁移 0024**（契约 §3.1 落地表止于 0023）· **D-5** 契约 §6.0 第 11 条要求的 `#[ignore = "…needs ENV…"]` 只覆盖一部分：16 处合格、**7 处裸 `#[ignore]`**（`injection_e2e.rs:96`、`graph/live-after.rs:3`、`knowledge/retrieval-gold-copy.rs:34`、`retrieval-gold-live.rs:26`、`store/src/lib.rs:1281`、`store/src/migrations.rs:766/894`）⇒ 显式跑且缺 env 时可能静默通过 · **D-10** 22 条纪律里 **6–10 已工具化进 `scripts/cargo-team.ps1`**（本代最好的形态）、1–5/16 有 CI、11 部分，**12/13/14/15②/17/18/20/21/22 只活在文档**（建议至少给 17/21 加守卫）· **D-11** 在飞单作废预警（t60/t63/t62 落地后契约 §3.2/§6.3 的相应文字作废，须随单同步）。

**t79/t80 已核对成立（别重做）**：召回日志**列与端点 12 个行键一致** ⇒ 看不到新遥测列是**服务端缺口**· i18n「76 无字面量 / 40 无动态族」按契约**只列不算失败**· 类型面干净（`@ts-ignore` 0）· `LEG_WINDOW=60` · **MCP 恰好 14 个工具**（用 `#[tool` 数会误得 16）· `facts?at=` 已归一化（t57 落地，INT-F2 关闭）· G7 43/47=0.9149 · `TAG_GRAPH` rank=3。

## B6. 第五波 intake（t78 编排/策略/纯域层审计）

| 单 | 来源 | 事项 | owner |
|---|---|---|---|
| t88 | t78 **C-2** | `Interrupted` 同时 active 且 terminal ⇒ 互斥（裁决 `is_active=false`/`is_terminal=true`）+ 更正把矛盾断言成期望的测试 + **能红的负控** | verify2 |
| t89 | t78 **C-3** | 权限审计把「默认值决定」记成「规则决定」⇒ 可辨 + 停止用标题合成 `rule_id` + 可反查回 `policy.toml` | integ |
| t90 | t78 **C-1** | 状态机没有家 ⇒ **裁决：转移表与校验器住 `core`**；daemon 13 处赋值**同块接线**（第 13 条）+ 逐格矩阵 + 非法转移负控 | integ |

**已登记待派（t78 其余）**：**C-4** 策略编译器全函数且不出声（未解析 `action` 静默丢弃**且被测试钉成期望** · `default` 拼错静默变 `Ask` · 先宽后具体**静默遮蔽** · `per_harness=0` 被接受）· **C-5 没有任何 run 级超时**（`select!` 四分支无 timer，而同仓 `distill.rs:612` 就是现成的 300s 模式；**许可位永久占住有指向证据**：`_permit` 是 `start_run` 的局部量、同函数 `:808` await `supervise`，默认 `per_harness=2` ⇒ 两个挂死 run 让该 harness 容量归零且**无任何遥测**）· **C-6 三处 fail-open 兜底**（`_ => Pending` / `_ => Queued` / **`_ => EndTurn`（失败读回成成功）**；`run_status_from_str` 漏 `"queued"` 靠兜底恰好等于真值掩盖；`api.rs` 的同义映射却是严格 `Option` ⇒ **同义两实现、语义不同**）· **C-7 级联不可辨**（`route() -> Option` 无理由通道、`rule_id` 是 Debug 拼串）· **C-8 pipeline 第 2 步起丢掉 `project`/`pinned_agent`**（同一逻辑形状因步数不同而路由到不同 agent）· low 5 条（`HarnessKind::ALL` 靠注释同步 · `core` 构造器读挂钟且「时间有序」**仅进程内** · `task.rs:106` 恒真断言 · `path()` 把三种「要人」压成无理由 `Human` · `AgentCard.command: String` 空白切分 ⇒ **含空格路径（Windows 常态）无法表达** · 三处描述把不存在的东西写成存在，含 policy 的 **"cost policy" 全仓 0 命中**）。

**t78 已证伪 6 条（别重查）**：`orchestrator:208` 的 `.ok()?` 跳过 default（上游 `api.rs:4699/4707/4710` 已预解析成 UUID 串，生产路径不可达）· `runs.rs:673/680` 的 Failed→Queued 是同一 `if let Err(reserve_slot)` 的互斥分支 · 恢复循环只遍历四个非终态 · 四个终态出口都发 `StateChanged` · `ids_are_time_ordered` 不是 flaky · **`core` 确实零 I/O**（依赖表只有 serde/serde_json/uuid/chrono）。

## B7. 第六波 intake（t92 纪律守卫决策文档）—— captain 裁决

**先做（已并入/待接）**：
- **G-21**：`check-workflow-refs.sh` **已存在但 0 调用点**（死代码）⇒ **接进 CI + 跟踪入库**（并入 t66 的 F1 项；见 §C17）。
- **G-15②**：每个**会写输入的夹具**断言「我建的已删 + 同 pid 能重建」，且断言必须在**连接关闭后**跑（t42 的原始成因）；每夹具≈10 行、CI≈0、**自归因 ⇒ 无归因歧义**。（登记待派，owner = 各夹具属主）
- **G-20**：把 `-DryRun` 解析出的命令行打进 CI（极低成本；杀 t53→t54→t55 那类「只读入口名字」，其中一次还被写进 `AGENTS.md` 传播两次）。

**留待决定**：**G-12 公开面快照 + 增量打印**（≈40 行 + baseline、~1s、维护面中）—— 价值最高但**必须归一化**（排序/剥注释/剥行号）且**必须同时记 `#[derive]` 与手写 `as_str()`**，否则漏掉 `ScoreKind` 漂移那一类；建议先在**一个 crate 试点**（跨 crate 被消费最多的那个）。

**只做 warn-only**：**G-15③ 临时 root 泄漏增量**（共享 `%TEMP%` 的增量**不可归因**，做成 fail 会与「不误归因」冲突）。

**不进 CI 的工具**：**G-21b `scripts/evidence.ps1 <路径…>`**（打印 `tracked? + sha256 + 行数 + mtime`）—— 报告期工具，专治「未跟踪文件的空 `git diff` 被当证据」。

**captain 采纳「不机械化」的三条**（附抽检触发条件）：**13**（门禁分不清「队友正在改」与「我读错了」；`git status` 在共享树里**不是证据** —— 本代已误归因过一次）· **14**（判据是**行为归因**，机械豁免不可判定 ⇒ 必误报；触发 = 报告写推断性归因且它支撑判据/结论时，看「把 X 改回去、症状是否仍在」的两次读数）· **18**（静态无法判「读得够晚」；替代品 = 把「等到终态」的 helper 放进共享测试工具，让**默认写法就是对的**）。

**t80 的三条自我更正（记此以免重复）**：第 **17/22** 条**早已机械化**（`test-evidence.sh`，`ci.yml` 5 处调用）· 第 **11/19** 条比 t80 写的更全（ignored 清单 `ci.yml:153-186`；每 job 都有 Coverage declaration 步）· 第 **21** 条的守卫**写完却从未被调用**。

## B8. 第七波 intake（t91 MCP/ACP 审计 + t95 守卫审计 + t83/t72 交回）

| 单 | 来源 | 事项 | owner |
|---|---|---|---|
| t93 | t91 **M1+M5** | `graph_entity` 把不存在报成成功（`unwrap_or_default()` 静默吸收协议字段改名）+ roundtrip 只对 9/14 名字做 `any()` | integ |
| t94 | — | 审计：存储层本体（单写者 actor / 迁移运行器 / 三实现一致性 / LanceDB） | verify |
| t95 | — | 审计：守卫的守卫（design-audit / build-panel / run-e2e / scripts / release.yml）→ **completed** | review |
| t96 | t83+t80 F7 | 消费面收口第 4 轮：`log`→`rows` **四处同改（含面板 api 客户端）** + F3/F4 + §20 | integ |
| t97 | t95 **T-1/T-3** | design-audit 退出码载重 + 过期读数更正 | review |

**t95 待派（按它的排序 T-1→T-2→T-6→T-4/T-5→T-3/T-7/T-8）**：**T-2** 真测量没门禁（CI 只跑 `--self-test`＝证明 row 16/23，`--check`＝判全部可实现行 ≈6.5 min）⇒ 需非每推必跑的 job（dispatch/nightly）或登记为**每代仪式** + coverage declaration 点名；**T-6** `release.yml` 工作流级 `contents: write` 对每个 job 生效、`uses:` 全未固定 SHA、`dtolnay/rust-toolchain@stable` 是移动分支、**发布 job 不跑测试（红提交打 tag 也会发）**；**T-4** `build-panel.mjs` 换出后无护栏（`renameSync` EPERM 未捕获、`rmSync` 在换出后才跑 ⇒ 信号反向、无「新 index 引用的 asset 都在」自检）；**T-5** `run-e2e.mjs` 零信号钩子 ⇒ 中断后孤儿 chromium/node 只靠人工纪律。
**t95 已证伪/不判缺陷**：`build-panel.mjs` 的「失败 ⇒ dist untouched」**成立**（三步检查 + FORCE_FAIL 钩子都在触碰 dist 之前）；三个 scripts 无自动调用点但 canary 拒绝杀非本 root 的 pid、post-switch-check 刻意不用 `/recall` ⇒ **合纪律，只登记「别当门禁」**。

**t72 交回（R-1..R-4，owner 待派；R-1 需排在 t96 之后，因同写 `crates/daemon/src/api.rs`）**：**R-1** 非法 supersede 的 HTTP 面仍是 **500**（`sqlite error: supersede refused…` / `FOREIGN KEY constraint failed`）⇒ 应映成 **400 + 明确原因**，并去掉 `sqlite error:` 前缀；**R-2** MCP 说明补「只替换同一 store+namespace 的当前条目」；**R-3** 墓碑占键的设计岔口（复活 vs 部分唯一索引 `WHERE deleted_at IS NULL`）⇒ **裁决：本代保留复活**（已端到端验证），部分唯一索引需**重建 `memories` 表** ⇒ 登记为**独立迁移轮次**的 schema 项，**在那之前谁都不许动该唯一约束**；**R-4** `lifecycle.rs` 的 delete/restore 按**预读**而非受影响行数（low）。

## B9. 第八波 intake（t94 存储层审计）

| 单 | 来源 | 事项 | owner |
|---|---|---|---|
| t98 | t94 **S1+S2+S7** | 账本不是事实：25 个迁移**可重放**（今天 5/25）+ 连续性校验（挖洞必须报缺 vN，今天静默跳过）+ 声称面更正（`store` 无 LanceDB 实现） | verify |

**待派（t94 其余）**：**S3（medium）** 四处 `_ => Default` 把读不出来的文本变成合法状态（`lib.rs:564/582/608/630`：未知 → `Pending`/`DependsOn`/`Queued`/**`EndTurn`**）⇒ 未知停止原因**看起来正常结束**；这段在**所有消费面之下** ⇒ 判据：插 `status='donee'` 读回必须报错/报未知（今天静默 `Pending`）· **S4（medium）** 删除路径三处把 SQL 失败伪装成「不存在/已删除」（`delete_chat` 的 `unwrap_or(0)`、`delete_session` 的 `.ok()`、最重的是 **`let _ = conn.execute("INSERT OR REPLACE INTO session_deletions …")` ⇒ tombstone 写失败仍照删会话并回 `Deleted`**，正是它上面注释要防的事；`let _ = DELETE FROM session_archives` 留孤儿档案）· **S5（medium）** CI 的 nested-Result 守卫**只覆盖 1/4 种形状**（`ci.yml:101-110` 只抓 `.await.is_err()/.is_ok()`；`sqlite.rs:76-77` 自己点名的还有 `if let Err(..)` / `let _ = call(..)` / 只对 `?` 作用于外层；用法普查 `call` 199 处 vs `call_flat` 48 处）⇒ 这条守卫**在已提交版本里**，覆盖面问题此刻就在生效 · **S6（medium）** 迁移登记表是手写 `include_str!` 列表、**版本号来自数组下标**（`(i+1)`）而非文件名、且**无目录↔数组漂移检查** ⇒ 丢一个 `.sql` 进目录 = 存在但永不应用（静默）；中间插入/重排 ⇒ 已迁移的库静默跳过新脚本 · **S9/S10（low）** `let _ = tx.send(f(conn))`（接收方消失后写仍执行、结果被丢）；`Db::open` 失败留半初始化不可用 DB 且无修复指引。
**t94 的前提更正（重要）**：① 本仓**不用 `PRAGMA user_version`**（fresh root 实测 = 0，grep 0 命中），版本在 `schema_migrations` 表里；② **store 里没有 LanceDB 实现**（`crates/store/src` 0 处引用；向量层在 `crates/knowledge`，`lancedb 0.38`）⇒ 以后说「三实现」要改成「SQLite+JSONL（store）与 LanceDB（knowledge）」，**LanceDB 层单独立项**。
**t94 的残留读数（S8）**：`%TEMP%` 匹配 `ruagent*|ra-*|pw-*|vint*` = **598 个目录 / 78.4 GB**，其中共享构建树 `ruagent-team-target` = **72.12 GB**（合法但巨大；删它要一次全量重建 ⇒ 暂留）；store crate 自己留下 ~298 MB（`ra-t36-target` 272.8 MB 等）。⇒ 成员自清私有 target 的纪律继续有效；**谁都不许删别人正在用的 target**。

**t74 的代价（诚实登记，需后续优化）**：A1（正文锚点参与判定）使 `/wiki/pages` 53 页 **157.8ms → 317.6ms（~2×）**，审计 A6 变重 ⇒ 若该端点被面板轮询（见 t79 的 P 系列），下一轮给它**缓存/预计算**；**先记录，不在本代为此开单**。
**t74 的范围裁决（cap 定案）**：它必须改 `crates/mock-agent/tests/wiki_pipeline.rs`（该断言**逐字把本单判定为失真的行为钉成期望**：注释 "the OBSERVATION TIME is written by the next build … nothing else observes drift" + `stale_since.is_null()`），而该文件不在原 inScope ⇒ **captain 裁决：纳入 inScope**（规则：**改变行为的单必须拥有钉住该行为的测试**）。成员**拒绝**用「只给没有 build row 的页盖章」让旧断言继续绿（那会让「构建后漂移」这一最常见情形仍无起点）——**记功**。

## B10. 第九波 intake（t95 守卫审计的收尾 + t97 交回）

| 单 | 来源 | 事项 | owner |
|---|---|---|---|
| t97 | t95 **T-1+T-3** | design-audit 退出码载重（唯一决策点纯函数）+ 过期读数更正 → **completed**（负控①在**副本树**里做、仓库契约未动；诚实登记「四计数未取得」） | review |
| t99 | t95 **T-2** | 把真测量接进门禁：`workflow_dispatch`+nightly 跑 `--check`（临时 root daemon）+ 四计数进 job summary + 能红的负控 + 第 19 条覆盖声明 | recall |

**t99 的基线要求**：t97 **未取到四计数**（只给 ≈6.5 min 的**估算**）⇒ 本单必须真取到，或写「未取得 + 原因 + 为何不影响判据」。
**captain 已自行落地的勘误**：`docs/design-language.md:88/89`（**`203/203` → `495/495`（2026-09-29 实测，旧读数保留）**，并把 `--check` 的新语义写进注释）。review 交回时说任务单里的 `docs/design/design-language.md` **不存在**、真实路径是 `docs/design-language.md`（我的路径写错，已改注释）。

**待派（t95 其余，按 review 给的排序）**：**T-6** `release.yml` 工作流级 `contents: write` 对每个 job 生效、`uses:` **全未固定 SHA**（`dtolnay/rust-toolchain@stable` 是移动分支）、**发布 job 不跑测试**（红提交打 tag 也会发）⇒ 危险面在**只读评估 + 固定 SHA + 发布前门禁**；**T-4** `build-panel.mjs` 换出后无护栏（`renameSync` EPERM 未捕获、`rmSync` 在换出后才跑 ⇒ 信号反向、无「新 index 引用的 asset 都在」自检）；**T-5** `run-e2e.mjs` `spawn(..., shell:true)` + **零信号钩子** ⇒ 中断后孤儿 chromium/node 只靠人工纪律。
**t97 §6 两条顺带读数（待裁）**：① §12.1 的节匹配是**前缀正则**（`/^### 12\.1/` 也匹配 `### 12.1x`）⇒ 将来的 `### 12.10` 会被吞进 §12.1 的切片（判据：加一个 `### 12.10` 小节看 §12.1 解析是否变化）；② row 32 的 `pending`（判据修订态）**仍不进退出码**——有意保留为第三态，若要与 T-1 同强度，先给它一个**到期条件**。
**t97 的事务性请求（已采纳）**：**它是作者 ⇒ 不把 t97 的评审单派给它**。

## E. 已验证的头条读数（可引用，须带来源与限定）

> 规则：每条都带**来源**（哪份报告/哪次读数）与**限定**（哪些是自造夹具、哪些只在临时 root 上测过）。**未验证/未取到的**一律写成「未取到 + 原因」，不得写成达标。

| 区域 | 读数（改前 → 改后） | 来源 | 限定 |
|---|---|---|---|
| 召回 | **recall@1 0.4667 → 0.7333** · **MRR 0.6889 → 0.8185** · **nDCG@10 0.7682 → 0.8621** | `gen2-recall-impl.md`（t7）+ 独立验证 t11 | 活库只读、分层 gold；分页无关性 2/13 → 15/15 |
| 召回（CJK） | CJK 字面腿接入 + 融合/重排；`score_kind` 线上形状 | 同上 | 负控见报告 |
| 图谱 | **G7 关系精度 0.7500 → 0.9149（43/47）** · **G5 多跳 0/20 → 0.70** · **G1 实体腿 1/23 → 13/23** | `gen2-graph-impl.md`（t9）+ t13 | gold 夹具；`GRAPH_PATHS=3` 的**最优性未获支持**（t63 的在块匹配器有缺陷）⇒ 只说「可达」，不说「最优」 |
| 图谱（注入面） | **`<graph>` 块 0/5 → 3/5**（`with_graph=3`），负控干净（106 vs 242 字符） | t58/t63 | 事件读数；`context_injected` 5 条里 3 条带 graph |
| Wiki | **`cite_coverage` 1.0**，且**负控**（无锚点的页）判 **failed** ⇒ 判据不是空转 | `gen2-wiki-impl.md`（t10）+ t14 | 页级判据 |
| Wiki（新鲜度） | 正文锚点与 frontmatter 锚点**判定逐位一致**（`p-body-dangle` vs `p-front-dangle`）；`stale_since` 在**读路径**非空且连读两次逐字相同 | t74 | **代价**：`/wiki/pages` 53 页 157.8 → **317.6ms**（~2×，已登记待优化） |
| 记忆（衰减） | `used=5,last_used=now → 0.500000` vs `used=0,last_used=now-30d → 0.058660` | `gen2-memory-impl.md`（t8）+ t12 | 纯函数读数 |
| 记忆（判据性） | 标注语料 **误并 0/20、召回 14/20** | t8 | **自造夹具**（非人工 gold；人工 gold 为 C3/G7 的遗留项） |
| 记忆（写侧） | 软删后同内容重写 **`Inserted(1)` 且列表可见**；supersede 非法目标 **`Err` + 零行写入 + 审计 `supersede_refused`** | t72 | HTTP 端到端；HTTP 面仍 500（R-1 待修） |
| 消费面 | MCP **14 个工具全是到 HTTP 路由的一行薄代理**（`lib.rs:289-296`），URL/参数逐条核过 ⇒「两份实现」不成立 | t91 | 只读审计 + 运行时 stdio 实例 |
| 消费面（键名） | `log` → `rows`：**daemon 半已绿**（编译面 exit 0；测试面 `ok=56 / Running=45`），卡在**面板 api 客户端**一个文件 | t83 | **已一致回退到 `log`**，第 4 轮 t96 收口 |
| 安全 | 非 loopback 绑定**必须显式解锁**，否则非零退出 + 点名实际地址与「无认证」；grep 由 0 → 13 命中 | t76 + captain 独立复核 | 已写进 `AGENTS.md` 约定 |
| 领域层 | `RunStatus` 的 active/terminal **在类型上不再可能同时为真**；8 状态矩阵非法行 **1 → 0** | t88 | 负控真红（注入旧实现 ⇒ FAILED / exit 101） |
| 存储层 | **25 个迁移里 20 个不可重放**；删一行版本记录 ⇒ 守护进程起不来或**静默跳过** | t94 | 已在库副本上测；修复单 t98 |
| CI 加固 | 计数口径机械化（0 reporting target = error）· `--no-fail-fast` 4 处 · F1 守卫**今天真红**（6 处引用 2 个未跟踪脚本、**零误报**）· 负控 exit 1 / 健康日志 exit 0 | t65 + captain 独立复核（`$LASTEXITCODE` 通道） | 见 C20/C21 |
| CI 基线 | run `36272344732`：ubuntu **Format 红** ⇒ Clippy/守卫/Test **全部 skipped**；windows **Test 红**；Panel 绿；E2E 绿 | `gh run view` | 这是 `t66` 要翻转的红基线 |

## B11. 第十波 intake（t100 发布加固 + CI 红史取证）

| 单 | 来源 | 事项 | owner |
|---|---|---|---|
| t100 | t95 **T-6** | 发布路径加固：工作流级 `contents: write` → **`read`** + 按 job 授足（`gate{actions:read}` / `build{contents:read,actions:write}` / `release{contents:write,actions:read}`）· **六个 `uses:` 全部固定 40-hex SHA**（含 `dtolnay/rust-toolchain@stable` → `6bed0761…`）· **新增 `gate` job**（读**这个 commit** 的 CI 裁决：success 放行 / failure 与「没有 run」**fail-closed** / 还在跑最多等 15 分钟），`build.needs=gate`、`release.needs=build` → **completed** | wiki |
| t101 | 同上 | 审计（只读）：CI 红史取证 —— 失败步分类 + 那 87 个 `cancelled` 的机制 + 「第一次绿还差什么」 | wiki |

**t100 的负控与诚实标注**：负控用**真 `gh` + 真 API** 跑（真红 commit `5db881d8…` ⇒ `exit=1`；无 CI run 的 SHA ⇒ `status=none` ⇒ `exit=1`；成功桩 ⇒ `exit=0`）；**未验证环已标出** —— GitHub 的 `needs:` 调度语义本机不可执行（无 act/Docker），以 YAML 策略断言 `build.needs==gate` 佐证。**未覆盖 10 条**（GitHub 端 environment/tag 保护未验证 · 未真跑发布 ⇒ 六个 SHA 固定尚未被运行验证 · SHA 固定 ≠ Rust 版本固定（stable 仍浮动）· 无 Dependabot ⇒ 升级要人工 bump · 门禁依赖 `ci.yml` 名字（改名 fail-closed）· 15 分钟等待上限是**行为变化** · runner 镜像仍浮动 · `build` 的 `actions:write` 是「最小且够用」而非只读 · 发布产物质量不在门禁射程 · 已提交的 scripts 目录需被正确 `git add`）。

**⚠️ CI 红史（captain 独立核实，纠正一处错误前提）**：**`ci.yml` 不是从未绿过** —— **全量 300 次历史 success = 97**，**最近一次绿 = 2026-09-26T11:09:39Z**；**最近 100 次**（`14:06:59Z → 21:16:08Z`）= `failure 13` + **`cancelled 87`** + `success 0`。最新 run `36272344732`：ubuntu **`Format`** 红 ⇒ 同 job `Clippy`/`Guard`/`Test` **全 skipped**；windows **`Test`** 红；**Panel 绿**。`e2e.yml` 近 50 次 **49 绿 / 1 红**。⇒ 结论：**「那天下午一串推送把 CI 打红并一直红着」**，不是「从来绿不了」；目标②是**恢复**绿，不是**首次**绿。（t100 报告里「CI 从来没有绿过」的说法据此更正。）
**t100 另两条登记**：① `release.yml` 上一次真实运行（2026-09-17）**在 `Create release` 失败**（`##[error]<!DOCTYPE html>`）⇒ 加固只保证「红的不会被发」，**不保证下次发布成功**；② 本机 `check-workflow-refs.sh` **exit 1**（两脚本未跟踪）⇒ **推送时若忘记 `git add` 它们，CI 第一次就会红在那条守卫上**（正是该守卫的目的）。

## B12. 第十一波 intake（t73 / t84 交单）

| 单 | 来源 | 事项 | 状态 |
|---|---|---|---|
| t73 | t69 **F3** | 公开 FTS 吃原始用户文本 ⇒ 净化上移进共享 `query.rs::fts_pattern`（**幂等**，见 C23）| **completed**：`before(raw) errors=8/11 → after ok=11/11`；八条特殊语法 **HTTP 500→200**；**顺带修好真实查询 `release.sh`**（`.` 是 FTS5 语法字符）；负控 `deploy`/`部署`/`"deploy script"`/`café` 一致 |
| t84 | t79 **P2/P3** | 面板失败可见性 | **failed（只差「观察」）**：代码落地、`panel build` exit 0、成功路径 diff 级未变（`Memory.tsx 38/4`、`Runtimes.tsx 21/5`）；**四条 DOM 读数未取** ⇒ 已立 t102（verify2）补上 |

**t73 交回的路由（R-6..R-9，均未越界改）**：**R-6** `api.rs:3638`（`fts_related`）是这条净化规则的**第三份拷贝**（内联、无名、没人当它是净化器）⇒ 与 R-1 同批走 `api.rs`（现被 t96 占着）；**R-7** `memembed.rs:200` 现成重复实现待删；**R-8** graph 的 `entities_fts`（`graph/src/lib.rs:357,412`、`retrieve.rs:517,578`）与 store/knowledge 的 `chunks_fts_cjk`（`store/src/lib.rs:1187,1382`、`migrations.rs:560,634`）是**同族 FTS 面**、未取证其参数是否来自用户文本 ⇒ 各自 owner 自查一次；**R-9** 多词语义变更（隐式 AND → 短语）已登记、不建议在 repair 里改。

## B13. 第十二波（t81 图侧原子性收口）

| 单 | 事项 | 状态 |
|---|---|---|
| t81 | 图侧三处非原子写（t67 #1/#2/#6，同根因） | **completed**：**一种形状修三处** —— 每个公开入口的语句抽成吃 `&Connection` 的私有 `*_in`、公开签名一字未变（薄包装开一个事务）；新增聚合入口 `ruagent_graph::apply_extraction`（+ `ExtractEntity`/`ExtractFact`/`ExtractionWrite`）让「整份抽取」在**一个闭包、一个事务**里写完，且沿用同一套 `resolve_entity_in`/`upsert_fact_in`（G7 写侧去重、别名解析、`NeedsJudgement` 不合并**一条未绕**）。**`crates/store` 一个字节未改** ⇒ 「根因在 `call*` 语义」**经检不成立**（`store/lib.rs:838–845` 自己就用 `FnOnce(&mut Connection)`）⇒ 只留一条文档建议，**不需要交回 finding** |
| 验收 | 注入失败读产物（最终字节） | `#1 entities 0/0 · aliases 0/0 · edges 0/0`（审计改前 2 实体 + 1 别名残留）· `#2 partition after == before`（2 社区/成员 6/`parent_id NULL 0`）· `#6 entities 2/2 · edges 1->1 · aliases 1/1`；三条成功路径负控各一；**RVC-3 契约一字未动** |
| 反做负控 | 坏侧红 | 三处事务降级成 autocommit ⇒ 三条测试全红并打出**审计同族残留**（`#1 0/2 · #6 1/2 · #2 members 5/NULL 父 2`）、`3 passed; 3 failed`、exit 101 ⇒ **断言真挡得住「事务被拿掉」**；按新纪律（先广播 → 限时 ~12s → 报窗口起止 + 恢复绿 + SHA-256 逐字还原） |
| 门禁 | 三面 | 编译面 `check --workspace --all-targets` exit 0 · graph 测试面 ok=12/FAILED=0/exit 0 · daemon lib ok=80/exit 0 · 两条 clippy `-CleanFirst` exit 0（error-lines=0） |
| **未做** | 明确留原地 | 审计 **#5**（合并自环/社区成员不转移）· **#7**（`if let Ok(..) = query_row` 吞错误、`void_episode` 吞成 false）· **#4/#8/#9/#10/#11/#12**（全表读、N+1、循环内 prepare、全表扫、O(N²)、死 API）—— 报告 §8 逐条登记待 triage |

## B14. 首次推送的事故与处置（commit `0da0cb6`）

**事故**：captain 代执行的首次推送 `0da0cb6` **定格了一份不完整的多文件改动** —— `crates/memory/src/lifecycle.rs`（mtime **04:24:55**，mem-core 正在做 t75）引用了**尚未定义**的 `backup_surface`；提交发生在 ~**04:25:30**。CI 原文：

```
Build | error[E0425]: cannot find function `backup_surface` in this scope
Build | error: could not compile `ruagent-memory` (lib) due to 1 previous error
```

后果（两个工作流**同根因**）：
- **`E2E` run `36479210565` = failure**：`Build` 红 ⇒ 其后 6 步（Build panel / Register mock agents / Boot daemon / Doctor / Panel serves / Playwright）**全 skip**，`E2E evidence` **按设计失败**（这一步就是「跳过必须可见」的执行者）。
- **`CI` run `36479210529`**：**`Panel (node)` ✅ success**；**`Rust (ubuntu)` ❌ `Clippy`** —— 这是该 job **历史上第一次真正跑到 Clippy**，而它撞在同一个编译错误上；`nested-Result 守卫`/`Test`/`ignored instruments` 被 skip；**`Test evidence` 与 `Ignored-instrument manifest` 按设计失败** ✓；`Rust (windows)` 当时仍在跑。

**captain 的错（两份，都记账）**：① **明知** `lifecycle.rs` 在 38 秒前被写过，**仍然提交**；② 为省时间**跳过了「推送前对已提交字节跑一遍编译面」**——我在同一轮里明确想到了这一步又放弃了它。**两个假设被证伪**：我曾怀疑「**工具链漂移**（本地 stable `1.95.0` vs CI 解析到的最新 stable，`rust-toolchain.toml` 只写 `channel = "stable"`）」与「`orphans.rs` 的平台分叉」——**都不是**本次红因；**同一份 `E0425` 同时解释了 ubuntu 与 E2E 的失败**。⇒ 教训：**先取原文，再怀疑环境**（诊断偏差：把「环境差异」当第一嫌疑，是本地绿/CI 红时最省事的解释，但这次错了）。

**处置**：mem-core 收到 P0 —— 补齐 `backup_surface` 或撤掉调用点，**但绝不许把「残余面检测」删成空壳**（那会把 t75 的目的抹掉，会判假绿并要求重做）；完成后 captain 推第二个提交并按 **C16 + C20** 复核。

**仍然成立的旁证发现（与本次红因无关，但要记账）**：`crates/daemon/src/orphans.rs` 有 **6 处 `#[cfg(unix)]` + 6 处 `#[cfg(windows)]`** ⇒ 这是仓库里第一个真正的**平台分叉**文件，本地 Windows 的 clippy **从不编译** cfg(unix) 的那些区块 —— 一条真实的**门禁覆盖**缺口（第 19 条：门禁必须声明「哪一侧被 lint 过」）。

## B15. 推送第二、三批与 CI 红因逐层剥开

**推送账**：`0da0cb6`（首批，159 文件/+45,177/−914）→ **红因 `E0425 backup_surface`（memory 定格半成品，captain 自犯，见 §B14）** → `cb55073`（只带 memory 修复 + 总账，**刻意不含** graph 在途且编译不过的文件）→ **红因①：memory 4 条 rustc 警告被 `clippy -D warnings` 拒绝（我的过滤口径漏掉，见 C25a）** → `877a909`（memory 警告收干净 + graph `facts_as_of` 时刻解析 30/30/30 + `panel/e2e/failure-visibility.spec.ts` + 四份报告）。

**④ 类红因（假红）：`e2e.yml:148` 的判定表达式恒为 1**
```
rc=${{ job.status == 'success' && 0 || 1 }}
```
GitHub 表达式里 **`0` 是 falsy** ⇒ `true && 0` ⇒ `0`（falsy）⇒ `|| 1` ⇒ **恒等于 1**。**铁证**：run `36480566583` 中证据步之前**每一步都 success**（含 `Panel E2E (Playwright)` = **44 passed / 1 skipped**），证据步仍打出 `| exit code | 1 |` 并走 `::error::Playwright run failed (exit 1)` 分支 ⇒ **只要这步在，E2E 永不可能绿**。⇒ **一条永远为红的判定步比没有判定步更坏：它把真实红淹没**（C21/C24 同族）。已派 recall（t99）修 + 给能红的负控 + 打印被 skip 的 spec 名。

**recall 的 YAML 发现（高价值）**：t65 加的守卫步 `- name: Guard: workflow-referenced paths must be tracked` 里**未加引号的 `: `** 会让 PyYAML 报 `mapping values are not allowed here (line 62, column 20)` ⇒ **GitHub 会在跑任何一步之前拒掉整个文件**（一个「防 CI 指向不存在的东西」的守卫，差点让流水线**无法启动**）。已修并入库；`check-workflow-refs.sh` 现做「未加引号 `: ` 检测 + PyYAML 完整解析」并有负控。**建议：提交前的固定一步 = workflow 本地可解析 + 引用已跟踪**（交给 t99）。

**recall 自报的两处 CI 代码形状**：① `Test attempt 1` 步**吞掉 cargo 退出码**（`set +e; cargo …; echo $? > rc; set -e` 但步本身恒 0）⇒ `attempt 2` **永不触发**（本轮 windows 实测 skipped）；② `if: always()` 的 evidence/manifest 步在上游短路时产生**派生红**（三红步、读者需自行推断唯一根因）⇒ 根因应指向上游步。

**t87 交单（integ）**：四情形绑定探针（`127.0.0.1` 起 / `0.0.0.0` 无同意 **exit 1** + 逐字 WARN / `RUAGENT_ALLOW_REMOTE=1` 与 `--allow-remote` 各起且 WARN 逐字一致）· 私有构建产物两件证据（**325,455,872 B / mtime 04:35:58**）· **`test --workspace` = TEST_EXIT 0，`Running` 45 / `test result:` 56 / ok 56 / FAILED 0 / panicked 0，且 `ruagent-graph` error 行 = 0**（这是把 CI `Test` 推向绿的最强本地读数）· 迁移行数一手读数：**今天一次 POST 重写 0 行**（只在只读副本上跑）⇒ 审计的「156+」是历史规模、WARN 文本保持**定性**；附带事实：存记忆的表名是 **`memories`**。

## B16. t101 的 CI 红史取证（wiki，只读）与 t104 的立单

**全量口径（303 run，比 captain 的 100-run 窗口更完整）**：success **97** / cancelled **137** / failure **68**；最后绿 = `ed472e4a` @ **09-26T11:09:39Z**；其后 110 run（17 failure + 93 cancelled + **0 success**）。captain 早前引用的「success=0 / 其余 87」只是 **09-26 14:06:59Z→21:16:08Z 那 100 次窗口**（已按 sha+时间更正）。

**⚠️ 方法学发现（最有价值）：只看 `conclusion==failure` 会系统性低估** —— `ci.yml:29-31` 的工作流级 `concurrency` + `cancel-in-progress: true` 让后续推送顶掉前一次，**136 个 cancelled 里有 59 个（43.4%）已经有步骤判过 `failure`**。按「失败步」重算：**Clippy 35→77 · Format 18→29 · Test 26→33**。（好消息：**没有**只出现在 cancelled 里的新类别，类别集合与 failure run 完全一致。）⇒ **纪律**：统计 CI 失败类别必须**把 cancelled 里的失败步一起算**，否则「红过多少次」被腰斩。

**全量失败步分类（首→末）**：`Clippy` 35（09-11→09-26T19:19:43Z，**全 ubuntu**）· `Test` 26（windows 15 / ubuntu 11）· **`Format` 18（09-25 才出现）** · `Audit self-test` 10（09-24 起）· `Install protoc` 3（仅 09-11）。红串第一枪 = 11:29:23Z 的 ubuntu `Test`。

**级联：1 个缺陷被放大成 4 个红步**（读红日志必须按 `skipped` 还原）：`Clippy` 红 ⇒ 同 job `Test` **skipped**（该步没有 `if: always()`）⇒ `Test evidence`「日志读不到」红 ⇒ `Manifest`「found 0≠15」红；windows 同理。**正面读数**：证据步**确实拒绝**把「0 targets / 日志读不到」当绿（t65 的静默跳过被挡住），且 `Panel (node)` **两次真跑都 success**。

**当字节现状（收尾 `877a909`）**：脚本已入库 ⇒ `check-workflow-refs.sh` **exit 0**、`ci.yml` 375 行已提交、**clippy 0**；历史阻塞全部消解；开工字节上 `test --workspace --no-fail-fast` = **0（205.8s）**、`injection_e2e --include-ignored` 7 passed、ignored **15==15**、audit self-test 0、panel build 0、`e2e_daemon` 23 passed。**未取到 1 条 + 原因**：`Test evidence` 的 per-target 计数（需要一条编译成功的 run 的 `$RUNNER_TEMP/t65/*.log`）。

**§8 未覆盖 8 条**（不预测未来 run · 不验证 GitHub 端设置 · 不改文件 · 不验证推送本身 · **不覆盖 `ci.yml` 的 `uses:` 仍浮动**（t100 只做了 `release.yml` ⇒ 值得单独立单）· 不覆盖 macOS/runner 镜像 · 不分类 `e2e.yml` 的 15 次红 · 不覆盖「绿之外的质量」）。

**§9 方法学三条（都是真踩到的）**：① **移动靶** —— 它自己上一轮报的「CI 从未绿过」与开工时的「唯一阻塞 = Guard 的 6 条引用」都因团队自己的推送**秒过时** ⇒ **读数必须带 sha + 时间**（报告分 A/B 两段）；② 第一次 PyYAML 解析 `ci.yml` 报错是 **t66 正在改该文件的中途读取**，差一点把并发写当成文件缺陷；③ **绕过团队 wrapper 直接用 `cargo` 打共享 target dir 得到 `os error 2` 假红**，走 wrapper 正常（⇒ wrapper 不只是限流器，还是**正确性**的一部分）。

**t104 立单**（recall，deps=[]）：`t99` 的依赖边**平台没清掉**（仍记 `deps: t66`，而 `t66` 是终态 failed ⇒ claim 被拒）⇒ 按 recall 的请求单开小单：两处恒 1 的 rc 表达式、派生红指向上游、`attempt 1` 的退出码、**跳过点名**（`--reporter=list`，不动 `panel/**`）、workflow 本地可解析成固定一步，+ **三条能红的负控**。

## B17. 第四次推送（`cfb52b1`）与待解的两个红点

**第四次推送**（10 文件）：graph 的 fmt 修复（唯一站点 `crates/graph/src/lib.rs:1845`，5 增 1 删）· memory 的 4 条警告真修 + **t75 本体**（`ResidualSurface` 13→**14** 加 `BackupFile`，`forget_report` 签名不变、该面**总是出现**，真实 root 只读读数 **2 个候选都被检出** 而负控 `Some(0)`、A-1 场景复现）· **t103**（F1 第三态 + F2 闩锁 + 可点 retry，聚焦 e2e 6 passed/exit 0）· 四份报告 + 总账。**刻意排除**：`scripts/spec-anchors.ps1`（review 在途）、`.github/workflows/ci.yml`（recall 的 t104 在途）。

**t75 交回的路由**：**A-4**（`api.rs:3326-3340` 的 `GET /api/v1/forget-report` 应改调 `forget_report_at(db, Some(root), hash)`，否则 HTTP/MCP 永远看不到本机备份）⇒ **R-10，冻结中**（`api.rs` 归 `t96`，而 `t96` 是 repair 类且依赖已失败的 `t84` ⇒ 平台拒改、inScope 冻结）· **A-2**（`KnowledgeChunkFts`/`KnowledgeVectors`/`WikiBuildPage` 三面无任何报告提及）已登记 · **A-3**（`wiki.rs:2317-2323` 写前副本）只以 `ExternalResidual` 请求、**未改文件** · **A-5**（备份保留/加密策略）另立单 · **A-6**（探针在活库数据目录留下的 `…-shm` 32,768 B / `…-wal` 0 B）captain 裁决 **保留不删**（本会话活库只读；修复版已把二者排除出候选集，不影响任何读数）。

**⚠️ 预判的下一个红点（还没证实）**：integ 在 t103 收尾跑了**仓库入口整条 e2e** = `48 passed / **1 failed** / 2 skipped / exit 1`，唯一红点 = **`consumption.spec.ts` 的 Wiki 覆盖率用例**（`consumption-Wiki-page-…-shows-coverage-unknown`，产物 `test-results/run-74076-…/test-failed-1.png`）。它按第 10 条**只报坐标不改**；captain 已把「区分断言过期 vs 实现回归」的取证单转给 wiki（含它自己的 `cite_coverage` 负控是否仍能红）。**若该失败在 CI 上复现，则 E2E 即使修掉 rc 恒 1 也仍会红。**

**t104**（recall，deps=[]）：判定步假红修复（两处 rc 表达式 + 派生红指向上游 + attempt 退出码 + **跳过点名** + workflow 本地可解析 + 三条能红的负控）—— **recall 已开始改 `ci.yml`**。
**t105**（mem-core，deps=[]）：只读审计 CI 证据链里两个**从未被复核的守卫脚本**（`test-evidence.sh` / `check-workflow-refs.sh`），要求每条判据都有能红的负控、并找**假红/假绿面**。

## B18. t104 的落地形状（captain 独立预验）

**恒 1 的表达式已从代码里消失**：`Select-String "&&\s*0\s*\|\|\s*1"` 现在只命中 **2 处解释性注释**（`ci.yml:130`、`e2e.yml:148`，都在写「GitHub 表达式里 `0` 是 falsy ⇒ 恒为 1」）⇒ 读者不会再踩同一个坑。
**五种新形状都在位**：`PIPESTATUS` **4 处**（状态来自命令自身）· `reporter=list` **2 处**（跳过要**名字**）· 上游归因措辞 **15 处**（`was skipped / does not compile / no reading / upstream`）· 文件体量 `ci.yml` 20,251→**25,508 B**、`e2e.yml` 9,341→**11,438 B**。

**最重要的一条：`⑤` 变成了**可跑的提交前命令**（captain 在 WSL 里独立跑通，exit 0）**：
```
workflow path references checked: 15, not tracked/missing: 0
  .github/workflows/{ci,e2e,release}.yml: parses as YAML (PyYAML)
every executed path a workflow references is tracked
PRE-SUBMIT COMMAND (t104): bash .github/workflows/scripts/check-workflow-refs.sh
```
⇒ 两个守卫脚本现在**同时**承担「引用已跟踪」与「三份 workflow 本地可解析」，并且**自己把该跑的命令打印出来** —— 这是「**CI 的第一个真裁判不应该是 GitHub，而应该是本地能解析我们自己的 workflow**」那条发现的落地。WSL 里 PyYAML 5.4.1 可用；**yaml 不可用时必须写「跳过 + 原因」，不许写成通过**（已写进 t104 验收）。

**同时救回被 t66 封锁的依赖者**：`t77`（恒真断言修复：迁移测试里的自比较换成真 before/after + 能红的负控）`deps → []` ⇒ 它解锁 `t98`（迁移账本可重放，high）。`t99` 的依赖边**平台未清掉**（仍记 `deps: t66`），故按 recall 的请求单开 `t104`。

## B19. ubuntu 的第一条真实测试面红（`cfb52b1`）

**判定**：`cfb52b1` 的 ubuntu 步骤链 `Format ✅ Clippy ✅ nested-Result 守卫 ✅` ⇒ **`Test` 首次真正执行**（本仓近期历史上第一次）⇒ **`Test` 失败**。CI 原文（annotations）：
```
[failure] Rust (ubuntu) -- default run: exit code 1 with 1 failing target(s)
```
⇒ **只有 1 个测试 target 失败**；`Test evidence` 随后也红（派生 + rc 恒 1 的假红），`Ignored instruments` 被 skip（级联）。

**取证技巧（captain 发现，建议全员用）**：**run 未完成时 job 日志被 GitHub gate 住，但 `check-run annotations` 可读** ——
```powershell
gh api "repos/<owner>/<repo>/commits/<sha>/check-runs" | ConvertFrom-Json   # 每个 check-run 的 conclusion + output.annotations_count
gh api <output.annotations_url> | ConvertFrom-Json                           # ::error:: / ::notice 原文与计数
```
本轮就靠它先拿到了「**1 failing target**」这个关键读数，不必干等 run 结束。

**嫌疑面（按证据排序）**：`crates/daemon/src/orphans.rs` 是仓库里**唯一的**平台分叉源文件（**6 处 `#[cfg(unix)]` + 6 处 `#[cfg(windows)]`**：`unix_children` 扫 `/proc` 按 ppid 匹配、`unix_process_start` 读 `/proc/<pid>/stat` 第 22 字段 + `/proc/stat` 的 `btime`、`read_stat_field` 用 `rsplit(')')` 取 comm 之后的字段）—— 这些 Unix 分支**在这台 Windows 开发机上从未被编译或执行**，今天第一次在 ubuntu 上跑。**captain 逐行读过，未发现明显缺陷**（字段偏移与 `/proc` 语义对得上）⇒ 嫌疑**不能**停在这里，需日志定位。`crates/knowledge/src/files.rs` 的 2 处命中**只是文档注释**（提到 symlink），**已排除**；`crates/acp/src/fs_tools.rs` 1 处待查。

## B20. t85（规格坐标漂移）与 t107（e2e 打活守护进程）

**t85 完成（review）**：新增 `scripts/spec-anchors.ps1`（三层检查器）+ 四份规格勘误 + `gen3-spec-errata.md`。**读数**：改前 `refs=158 pass=5 drift=20 suspect=43 unverified=133 unresolved=9 → exit 1`；改后 `refs=153 pass=20 **drift=0** suspect=42 unverified=133 unresolved=9 → **exit 0**`；**负控**（副本里把 `wiki.rs:877` 改成 `:100`）⇒ `DRIFT … 'regenerate_index' lives at :877` + **exit 1**（`-Spec` 传副本、仓库未动）。**三层只有一层会红**：DRIFT（反引号点名了**该文件定义**的符号、而行 ±3 内没有它）/ SUSPECT（只有使用点锚词，打印）/ UNVERIFIED（无可判别符号、basename 多义、文件不存在；打印+计数）。**它不是偷懒，是被读数逼出来的**：天真版「任一 token 不在该行 ⇒ 红」给 **122** 处、「代码形状+稀有」97、「反引号+稀有」60、加「必须是定义符号」才到 **20** ⇒ **122→20 是误报变少而非发现变少**（正是 t92 的头号告诫）。代价：**42 SUSPECT + 133 UNVERIFIED 需要人工，作者明确没把它们写成「已核对」**。一次性勘误 `applied=34 missed=0`，**每条旧值就地保留**；检查器**跳过并计数**勘误段内的旧引用。**交回**：① 契约 §3.1 落地表**止于 0023**、缺 `0024_distill_attempts.sql` 的形状变更行（owner `t64`，冻结）；② **9 条无法解析的引用**（basename 匹配多个文件；`output.rs:49` **全仓无此文件**）⇒ 建议规格一律写全 `crates/…`；③ 42 SUSPECT；④ 133 UNVERIFIED（**不是已核对**）。**未接进 CI**（接线时要带「改坏⇒红」负控）；作者**请求不要把 t85 的评审派给它自己**。

**t107 立单（wiki）**：**e2e 默认打操作者的活守护进程并写活库**。根因：`panel/playwright.config.ts:36` 的 `baseURL ?? "http://127.0.0.1:8787"` + `run-e2e.mjs:9-16` 不设 `E2E_BASE_URL`；`consumption.spec.ts:48-65` 的 beforeAll **写记忆 + `recall_log`** 而 afterAll 不删；**证据**：活库（只读指纹）里有 1 条该 spec 的探针记忆 + 4 条 query 含 `consumption` 的 `recall_log` 行，而该 spec **今天 04:25 才进仓库** ⇒ 只能来自今天那几次 e2e。⇒ 那条「Wiki 覆盖率」红点**既不是实现回归也不是断言过期**，而是 e2e 被指到一个**两天前的旧守护进程**（其镜像 09/27 05:35:34 **早于**把 `cite_coverage` 加进 `WikiPageInfo` 的 `0da0cb6`；把同一旧镜像跑在临时 root 上 ⇒ `has cite_coverage key: False`，与 `Received: undefined` 逐字对上）。**成对读数**：旧守护进程 `1 failed` → 当前字节 `1 passed (3.0s) exit 0`。**⚠️ 需用户知悉**：今天的 e2e 运行**写了活库**（1 条探针记忆 + 4 条 `recall_log`）—— 本会话对活库的规则是**只读**，故 captain 不自作主张清理，已如实上报并提供「清理 or 保留」选项。

## B21. ubuntu 唯一的失败用例 = 一个平台幼稚的测试夹具（t108）

**CI 原文**（run `36483026749` / `cfb52b1` 的 ubuntu job，完整日志）：
```
test the_seed_guard_tells_a_copy_from_the_live_root ... FAILED
thread '…' panicked at crates/knowledge/tests/gold-seed-idempotence.rs:373:9:
assertion `left == right` failed: the live database itself: C:\Users\x\.ruagent\data\ruagent.db
test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.16s
error: test failed, to rerun pass `-p ruagent-knowledge --test gold-seed-idempotence`
```
**同一轮其余 target 全部 `ok`**（daemon lib **80 passed**、35 passed、多组 7/8/5/3/2 passed、若干 `0 passed; N ignored`）⇒ **ubuntu 的 Rust 测试面只差这一条**。

**根因（源码级）**：`gold-seed-idempotence.rs:350` 的 `live = PathBuf::from(r"C:\Users\x\.ruagent")` 与四个 `C:\…` 字面量。该测试想验的是 `common::is_inside` 的**逐组件包含**（含「兄弟目录 `…/.ruagent-other/…` 不得被前缀混淆」这条核心判据）。**Windows** 上反斜杠是分隔符 ⇒ 成立；**Linux** 上反斜杠只是普通字符 ⇒ 整串退化成**单个组件** ⇒ 第 1 个 case（期望 `true`）在 `:373` panic，文案正是 `the live database itself`。⇒ 与 CI 原文逐字吻合。
**为什么此前一直绿**：本机是 Windows ⇒ 走另一条分支；**CI ubuntu 是该测试第一次在 Linux 上执行** —— **第 19 条**（门禁必须声明「哪一侧被跑过」）的一手实例。captain 此前对 `orphans.rs` 的逐行排查是**正确的否定结果**（真正的分叉点在测试里）。

**t108 立单（recall）**：平台中立地构造夹具（**不许** `#[cfg(windows)]` 整段跳过）· 四个 case 一个不少 · **能红的负控**（把 `is_inside` 临时改成字符串前缀比较 ⇒ 兄弟目录那条必须红）· 读数含「为何此前本机绿」· **顺带精确正则全仓扫同类硬编码 `C:\` 字面量**（只报不改）。

**取证技巧（本轮实践，与 t101 的 supersede 机制同源）**：**被后续推送取消（superseded）的 run，其已完成 job 的日志不再被 gate** ⇒ 想读某条早已结束的 job 日志，可以「推一个新提交把它取消」，但**代价是丢掉未完成 job 的结论**（本轮就因此丢掉了 windows 的结论）。反之，run 处于 `in_progress` 时**只有 annotations 可读**（见 §B19）。

## B22. e2e 的 skip 清单与「无条件 skip」finding（t107 落地形状）

**t107 的核心已由 wiki 落地（在途、未提交）**：`panel/e2e/run-e2e.mjs` 头部写入 **「AND IT REFUSES TO GUESS THE TARGET (t107)」** —— 不指名守护进程就**什么都不跑**（playwright 根本不启动，而非跑完再 skip）；**CI 显式豁免，豁免键是 `process.env.CI`，绝不是端口或 URL**（「本地运行无法走那条分支」）。`consumption.spec.ts:42-43` 已改为 `writeAccess()` + `test.skip(!access.allowed, access.reason)`。

**skip 清单（19 处，captain 复核）**：`registry.spec.ts:22` 与 `consumption.spec.ts:43` 挂在 `writeAccess()`（CI 里 `run-e2e.mjs` 已武装 ⇒ 应当真跑）；其余多依赖环境（`list.ok()`/`mock`/`judge`/`made.ok()`/`put.ok()`/`enabled.length`/wiki 有页）。⇒ CI 里那个 `1 skipped` 最可能是 **`recall.spec.ts:32`**：
```ts
test.skip(true, "recall matched no memories in this database");
```
**这是无条件跳过** ⇒ **一个永远不会执行的覆盖**（属「静默跳过」家族；区别是 t104 之后它**至少会被点名** —— 这正是 `--reporter=list` 的价值）。**登记待判**：这条 spec 要么在 CI 的 fixture 下真的建出可匹配记忆（让它跑），要么被删/改写成有意义的条件跳过；**不许**把它当成「覆盖已存在」。

## B23. windows 面长期失明的机制与后果（目标②最后的未知）

**机制（两个原因叠加）**：
1. **run 级 concurrency**：我的连续推送会取消前一条 run ⇒ windows 的 `Test attempt 1` **5 次全部被取消**（`cb55073`/`877a909`/`cfb52b1`/`5d3adfd`/`44636db`），而它的耗时又**最长**（ubuntu 245s ⇒ windows 是同量级工作的 **~6.4×**：本轮 ubuntu `Test` = **156s**，windows 同期已 >16.75 分钟仍在跑）。
2. **历史观测值全是下界**：那 4 个未被打断的观测（**540s / 629s / 767s / 772s**）都是**取消点**，不是完成点 ⇒ **windows 的真实完成时间从未被观测过**。⇒ 不能拿它们当上界，也不能据此宣称「windows 就是 9–13 分钟」。
3. **一个必须记住的陷阱**：`0da0cb6` 那次 `Test attempt 1` = **335s 且 `success`** —— 那次工作区**不编译**，cargo 编完依赖就早退；其 `success` 还是「吞掉退出码」的假绿（t104 修）。**既不能当性能基准，也不能当超时上界。**

**后果（两条）**：
- **windows 的真实测试判定从未取得** ⇒ 目标②的「CI 绿」在 windows 面**没有依据**（本代之前最后一次 windows `success` 是 09-26 的 `663983d`）。⇒ 处置：**在它出结论前不再推送**（这是本轮采用的做法）。
- **`ci.yml`/`e2e.yml` 没有 `timeout-minutes`** ⇒ 一个**挂住**的测试会让 runner 静默占满 **360 分钟**（GitHub 默认），run 一直 `in_progress`。⇒ 已立 `t111`（超时上界；我已把上述标定数据与陷阱一并交给 owner）。**这也是「挂住」与「慢」在读数上不可区分的根因**：没有上界时，沉默与正常工作长得一样。

## B24. t108 完成（平台幼稚夹具）+ 两条新 finding（R-11/R-12 登记）

**改动**（1 file，+35/−14）：`live` 从 `PathBuf::from(r"C:\Users\x\.ruagent")` 改成 **`Path::new("home").join("x").join(".ruagent")`**（相对 + join）；**无 `#[cfg(windows)]`、无 `#[ignore]`**（0 命中，只在注释里说明「故意不用」）；**四个 case 与理由注释一条不少**；被验语义未变（`common::is_inside` = `Path::starts_with` 的**逐组件**包含，`crates/knowledge/tests/common/mod.rs:413`）。
**为什么本机一直绿**（第 19 条实例）：探针显示旧夹具 `live = 5 组件 / candidate = 7 组件` ⇒ 本机成立；**Linux 上反斜杠是普通字符** ⇒ 两边各自坍缩成 **1 个组件且不相等** ⇒ 期望 true 得 false，报错文案正是 `the live database itself`（与 CI 原文逐字吻合）⇒ **CI ubuntu 是它第一次在 Linux 上执行**。
**负控（C22 推荐路径：完全隔离）**：`git stash create` → `git archive` → `%TEMP%\ra-t108-pre`，**只在那里**把 `is_inside` 改成字符串前缀比较 + 私有 `-TargetDir` ⇒
```
panicked at …gold-seed-idempotence.rs:398:9: assertion `left == right` failed:
a SIBLING directory: component-wise containment must not confuse it
"home\\x\\.ruagent-other\\data\\ruagent.db"   →  FAILED. 2 passed; 1 failed → exit=101 (130.1s)
```
⇒ **恰红在 case ③，且是在改后的平台中立夹具上** ⇒ **修复没有削弱判别力**（这一点比「负控能红」本身更重要）。共享树零变异（`git status` 只有 inScope 文件、`git stash list` 空）。
**门禁**：`rustfmt --check` exit 0 · 该测试 **exit 0**（ok 3 / FAILED 0 / panicked 0）· `test -p ruagent-knowledge` **exit 0**（10 个 `test result:` 行、FAILED 0；`retrieval-gold-{live,copy}` = `0 passed; 1 ignored` ⇒ **碰活库的两条没跑**）· `clippy … -DenyWarnings -CleanFirst` **exit 0 + 两件证据**（`Checking ruagent-store`/`ruagent-knowledge` 都出现 = 清缓存后真重编；`^warning`/`^error` 0 行 + `Finished 7.49s`）。
**未覆盖（作者自陈）**：改后夹具的 **Linux 侧没有真跑**（本机 Windows；Linux 行为是分析必然 + 旧夹具的 CI 原文提供失败侧证据）⇒ **以 CI ubuntu 的下一次真跑为准**。

**两条新 finding（captain 登记）**：
- **R-11（`daemon/src/api.rs:4856`）**：`PathBuf::from("C:\\")` 被 `if set_current_dir(&second).is_ok()` 守卫 ⇒ **Linux 上该分支静默不执行** ⇒「答案不随 cwd 移动」这条断言在 Linux 上**从未被验过**，而测试照样绿 —— 属**静默平台跳过**形状。**未立单**：`crates/daemon/src/api.rs` 属**被冻结的 `t96`**（消费面收口第 4 轮，其依赖链上有 failed 单）⇒ 按 R-10 的先例只登记，待 t96 解冻后并入。
- **R-12（`daemon/sessions.rs` + `acp/adapter.rs` 共 7 处往返断言）**：**现在中性**（原样回显），但若将来改用 `file_name()`/`parent()` 推导，同一夹具在 Linux 上会给出**另一个**答案 ⇒ 建议加备注或换平台中立夹具。**登记，不立单**（当前不构成缺陷）。
- **方法学结论（比清单本身更有用）**：全仓 19 处同类命中里，**真问题不是「有没有 `C:\`」，而是「断言的真值是否依赖宿主的分隔符语义」**。19 处中只有这一处踩上。

## B25. 2026-10-03 恢复运转：两条新 finding（F1 已立单 / F2 待立单）

**开工前的树与 `main` 读数（captain 亲自取）**：`HEAD = 9059225`（2026-10-02 23:05 +08:00），**与 `origin/main` 同字节**；`gh run list` 显示 `9059225` 的 **CI ✅（25m29s）+ E2E ✅（3m58s）**；`git status --porcelain` 在本轮开始时**只有一份未提交的账本改动**（本账 §10 的续写行）。工作树最近一次被写是 **10-02**，此后约 25 小时无人写 ⇒ 本轮在**安静窗口**里开工。

**本代之后的代码是别的会话写的，不是本队的**：`e24263d..9059225` 共 15 个提交（capability 面、新增 `crates/extract`、`extract_plane.rs`、`knowledge_graph.rs`、graph 别名 FTS、routing 修正），设计文档在 `docs/plans/capability-plugins-design.md`（现 181 KB / 2667 行）。⇒ **本账 §10 的「待推/仍在办」是 2026-09-29 的快照**，不是现状；t110/t111/t112/t114 都已落地且 main 上绿。本账不追认他人提交的细节，只记「该快照已被后续现实取代」。

### F1（已立单 t118）`graph_entity` 把「不存在」读成「存在但无事实」—— 调用方可见的假成功

| 面 | 读数（captain 在当前字节上读出） |
| --- | --- |
| daemon（wire） | `GET /api/v1/graph/entity/{id}` 返回 `name`/`kind`/`aliases`/`facts`；`name` 由 `entity.as_ref().map(\|e\| e.name.clone())` 产生 ⇒ 不存在的 id 让 `name` 为 **`null`**，HTTP 仍 **200**（`crates/daemon/src/api.rs` 的 `async fn graph_entity`；同文件 `mod tests` 正是不存在 id ⇒ `200` + `name.is_null()` + `facts == []`）。**存在性本来就在 wire 上。** |
| MCP（消费面） | `crates/mcp/src/lib.rs:232-236` 判存在读 `resp.get("entity")`，`or_else(\|\| resp.get("exists")...)` —— **这两个键在 wire 上都不存在** ⇒ `entity_missing` **恒 false** ⇒ `:248-250` 把不存在的实体答成 `entity #N has no current facts`，`isError=false`。**agent 把自己瞎编的 id 读成「真实存在、只是没有事实」。** |

**与谁同族**：与 capability 设计文档 §20 第 4 条那条「不可传输的键被静默丢弃、调用方拿到成功」同族（t93 的 M1 前半；当时以「需 daemon 侧」交回 finding，此后 daemon 长出 `name` 而**消费面没跟上**）。**修法是纯消费面**（存在信号已在 wire 上、且由 daemon 自己的测试钉住）⇒ t118 的 inScope 只落 `crates/mcp/**`。

**附带证据（坐标漂移的又一处）**：`:225-231` 的注释声称 daemon「carries no existence field and no 404」——**这句现在是假的**；它引的 `api.rs:1207-1216` 在当前字节上是 `wiki_page_corrections`，**早已不是 graph entity**。⇒ t118 的验收要求注释里每个行号引用都对当前字节回读。

### F2（待立单）面板的真测量从未进过 CI：`--check` 判活 DOM 并 exit 1，CI 只跑 `--self-test`

| 面 | 读数 |
| --- | --- |
| 工具自己的契约 | `panel/tools/design-audit.mjs:14` = `--check` judge §12，**exit 1 on failure**；`:493` = `--check` **DOES judge the live DOM**；`:490` = `--self-test` 只证 rows 16/23 的准则 **LOGIC** 与可读性（不需要浏览器）。 |
| CI 实际跑的 | `.github/workflows/ci.yml:459` = `node tools/design-audit.mjs --self-test` —— **只有自检**。全仓 `grep design-audit .github/` 只有这一处。 |

⇒ **后果**：§12 的可实施行（含活 DOM 判据）在 main 上**没有任何门禁**；面板真实行为回归可以一路绿到 main。这正是 t95 的 T-2 与 t99 的立单理由，而 t99 依赖 `t66`（终态失败）⇒ **永不可认领**。⇒ 下一轮首要候选：把 `--check` 接进流水线（四条护栏：计数 + 跳过点名 + 能红的负控 + 不可测记 `not_measured` 且 exit 1），**且在接进去之前先用一次真跑证明它今天就能红/能绿**。

### 卫生与账目更正（本轮 captain 动作）

- **t96 的前提已被取代**：`log`→`rows` 的改名在本代早已被裁决**回退到 `log`**（§C12 与 §192「已一致回退到 `log`」）；当前字节两侧一致且写明理由（`panel/src/api.ts:480-483/485/489`：`log` = **一页**、`rows` = **总数**）。⇒ t96 的改名动作作废（不是未完成）；它另带的 F3/F4 已由 t62 承载。t96 的依赖是两张终态失败单 ⇒ **它本已不可认领**（这正是 R-11 被登记而非立单的同一条理由）。
- **t62 已从 t96 摘开依赖**（`update_task` 原子生效），并追加重测要求：`api.rs` 已被 capability 那代重写，t62 当初登记的行号极可能已漂 ⇒ **先在当前字节上量**（已有 ⇒ 以「读数 + 已满足」收口；缺 ⇒ 才是实现）。
- **未提交的账本改动随之入库**：本账 §10 的续写行在本轮开工时**已在工作树上挂了 3 天未提交**。这本身就是隐患 —— **共享树上任何未提交的文件，都会被下一位读者的 `git diff` 读成「有人正在改」**。

### 两条方法学观察（本轮的，不是重复）

1. **「核对当前字节」直接省掉了一整张单**：capability 设计文档 §20 第 4 条列着三条 follow-up（让不可传输的键**响亮**、删掉 prose 里的数字边界、加字段集守卫）。派活前我核了当前字节 —— **三条都已落地**（`crates/mcp/src/lib.rs:1210` 的「EVERY struct here carries `deny_unknown_fields`」、`:1357` 的「`deny_unknown_fields` IS THE FIX FOR A MEASURED FALSE SUCCESS」、`:1400` 的属性、`:1409-1425` 已改成「read that row's `options_schema`」、`:1865`/`:2417` 的字段集守卫）。**照文档立单就是让成员去做一件已经做完的事**，而成员会（正确地）拿现有代码回报「已完成」，白烧一轮。⇒ 通则：**登记的 follow-up 也是会过期的读数，立单前必须回到字节。**
2. **平台规则改变了正确的动作，而不是只阻挡它**：t118 与 t96 在 `crates/daemon/src/api.rs` 上重叠，而 running 团队只允许对 pending 单做 `update_task`（改不了 inScope、也删不掉）。我没有去削弱 t96 的 inScope，而是**回头重审修法本身** —— 发现存在信号已在 wire 上，于是本轮变成**纯消费面修复**，根本不需要那块路径。⇒ **通则：inScope 冲突先当设计问题重审，别当权限问题绕过。**

## B26. F1 根因族的同族盘点（t123，mem-core 只读）：桥里另有 11 处，排序第一会让 agent 说出直接的假话

**报告**：`docs/design/reviews/gen4-protocol-drift-sweep.md`（107 行）。**形状**（= F1 的根因）：读生产方响应键 + **吸收缺失**（`unwrap_or`/`unwrap_or_default`/`map(false)`/`or_else`）⇒ 键名漂移**静默**变成一句读起来正常的**成功**。

**11 处 (a) 类，按「后果严重度 × 可达」排序（前 5 名）**

| 排名 | 读点 | 漂移后 agent/用户看到的原话 |
| --- | --- | --- |
| **1** | `hits_to_text` 的 `hits`（`lib.rs:72`/`:157`，`memory_search`/`knowledge_search` 共用） | **`no results`** —— 而生产方确实发 `hits`（`api.rs:4974`/`5377`）。**有命中却告诉 agent「记忆/知识库里没有」**（并可能诱发重复写入） |
| 2 | `memory_get` 的 `memory.store`/`.namespace`/`.content`（`:132-137`） | 一条真实记忆显示成 **`[?/?] ?`** ⇒ agent 以为记忆是空的/别人的 |
| 3 | `recall_to_text` 的 `kind` + `_ => continue`（`:1521-1596`）；`legs_disabled` | 章节标题下**一行都没有**；「薄结果」的**解释行消失** |
| 4 | `hits_to_text` 的兜底键 `result`（`:1614-1624`） | `result` 在 daemon **0 次** ⇒ **死兜底**（与 t118 删掉的 `entity`/`exists` 同形）；`content` 改名 ⇒ 单条回 `?` |
| 5 | `chunks`（`:179`）· `tier` 过滤（`:760-768`）· facts 四字段（`:251-254`）· `tasks`（`:477-484`）· `truncated`/`dry_run`（`:1047-1077`）· expand 三键（`:202-204`） | `ingested 0 chunks` · `0 of N capabilities` · `#0 --?--> #0 ?` · `no tasks` · 「未截断」/回显自己的请求 · 展开内容成了 `?` |

**同一个文件里已经有一批 (c) 响亮样板**：`field_str`/`field_bool`/`drift`（点名「文件、键、取回的键集」）+ `capabilities`/`distill`/`ingest` 的承载键都走 `ok_or_else(drift(...))`，并有 5 条测试钉住 ⇒ **修法是把旧点改成新点的写法，不是发明新机制**（t118 的 `entity_exists` 已是第三个样板）。

**面板侧**：`panel/src/api.ts` 的 `getChecked`/`expectShape` **已是**运行期形状断言 (c)；残留 (a) 集中在**视图**对可选字段的 `?? 0`/`?? []` —— `Agents.tsx:353` `s.health.tools ?? 0`（MCP 服务器显示 **`0 tools`**）、`:233`、`:458`。

**下一轮的头（已定，未立单）**：修 **M1**（并顺手删 `result` 死兜底）。**但它与在飞的 t118 是同一个文件** `crates/mcp/src/lib.rs` ⇒ 单子必须**等 t118 落地后**才能立（平台会拒 inScope 重叠），且坐标要**按 t118 落地后的字节重读**。

**t123 自陈的两条读数陷阱（值得进纪律）**：① **清点生产方只 grep 带引号的键名会漏掉 serde 结构体字段** —— `grep -rn '"chunk"' crates/daemon/src/` = 0 命中，差点把 `knowledge_expand` 的 `chunk` 判成「生产方从不发出」，实际生产方是 `#[derive(Serialize)] struct Expansion { pub chunk: String }`（`knowledge/src/files.rs:52-61`），serde 按**字段名**发出、**没有引号** ⇒ 必须三种都查（`json!` 字面量 + `Serialize` 字段 + SELECT 列名）；② **文件在读取期间会动** —— `lib.rs` 同一轮里 2474 → 2560 → 2561 行（t118 在途），同一读点两次 `grep -n` 给出不同行号 ⇒ **引坐标必须带读取时刻的 sha256**。

## B27. 平台规则 vs 登记法：**冻住的单仍然占着它的 inScope**（captain 本轮撞到两处）

**事实**：running 团队里 `edit_plan` 只允许对 pending/never-started 任务做 `update_task`，而 `update_task` **改不了 inScope、也不能删单**。于是「一张被终态失败前置冻住、因而永不可认领的单」会**一直**占着它声明的路径，**阻止新单被创建**（平台的 inScope 重叠检查在创建时就拒）。

**本轮的两处**：

| 冲突 | 形状 | 处置 |
| --- | --- | --- |
| `t96` ↔ F3/F4 实现单 | `t96`（never-started，依赖两张终态失败单）占着 `crates/daemon/src/api.rs`，使 F3/F4 的实现单**立不了** | ✅ **captain 直接取消 `t96`**（协议明许：captain 可直接取消 never-started 的 pending 单）。理由不是「烦」，而是它的 `log`→`rows` 前提**已被裁决推翻**、且经 `t62` 逐行独立复核（三处一致全部读 `log`）。取消后 `t126`（F3/F4 实现）立刻立单成功 ⇒ **这是本代第一条「用读数正当取消」的记录** |
| `t64` ↔ `t89` | 两张**都冻着**，且**都占 `crates/daemon/src/runs.rs`**；我试图解冻 `t64`（deps → `[]`）时被拒：`inScope overlaps t89 at crates/daemon/src/runs.rs` | ⏳ **未处置**。注意 `t89` 的依赖里**本来就有 `t64`** ⇒ **次序其实已声明**，平台的重叠检查不看成对依赖。⇒ 下一步：**先核 `t89` 的前提今天是否仍成立**（`t78` C-3 是「权限审计把『默认值决定』记成『规则决定』」，而 daemon 的 api/config 已被 capability 那代重写过 —— C29：登记的 finding 也会过期）。**若已成立则保留、若已被取代则同样用读数正当取消**，两种情形都要把依据写进本账再动 |

| `t126` → `t127`（**「失败单的续做」在平台上没有表达方式**） | `t126`（F3/F4 实现）因**预算耗尽**以 `failed` 收场，而它的**续做单就是 `t127`**（我按它自己查清的坐标与裁决重新立单）。但平台的 `Delivery` 把它读成 **「t126 failed without a follow-up repair」** —— 因为在平台的模型里，**「repair 单的 `sourceTaskId`」只表达「修复某单的 finding」，不表达「续做同一件事」** | ⏳ **只能靠文字**：`t127` 的 description 第一句就写「本单是 t126 的续做，不是重做」，我在这里也记下这条对应关系。⇒ **给下一位的判据**：`Delivery` 里的「failed without a follow-up repair」**不等于**「没人接手」，要逐条回看该失败单的 output 里有没有交出**可续做的交接**（t126 交出了：零源码改动自证 + 坐标表 + 两个 blocker 的解法 + 裁决请求）。**同族的第二个例子是 `t99`→`t131`**（F2 单：我取消 `t99` 后以更精确的契约立了 `t131`，平台的 Delivery 不会把两者联系起来） |

| **目录式 inScope 与「精确文件路径」完成校验不兼容**（`t118`，代价 = 一次结构性搬移） | `t118` 的契约 inScope 写的是**目录** `crates/mcp/tests`，而平台的完成校验把 `crates/mcp/tests/roundtrip.rs` 判为 **`undeclared`** 并**拒绝完成**。作者为了**不改契约、不扩面**，把两条端到端用例与夹具从 `tests/roundtrip.rs` 搬进已声明的 `src/lib.rs` 的 `mod tests`。**代价不止是位置**：那两条**不再驱动真守护进程**（改成对 axum **stub** 发真 HTTP、经真 MCP 协议，stub 回它从真 daemon 实测到的 JSON 原文）⇒ **「真路由」那一层一度没有任何测试驱动**，已改由 `t119` 取（契据已更新，配方与夹具由作者备好）。**未处置** | **给下一位的判据**：立单时 inScope **写精确文件路径，别写目录**（目录会被平台的校验器读成「未声明」）；若已经写了目录、又不想扩面 ⇒ 你会在「搬代码」与「改契约」之间二选一，**代价是真实的**（本例是一次搬移 + 证据层级的降级）。**同族**：C18（`repair must not depend on failed task`）、§B27 的前两条 —— 都是「平台模型与登记法之间的缝」 |
| **平台的「自动质量环」是活的**（`t120` 判 `needs_revision` 时实测，2026-10-04） | 我原以为本队是纯 captain 计划制、没有自动派生 —— **实测相反**：`t120`（`review` 判 `needs_revision` 并带 findings）终态之后，平台**自动**建了派生单：`t138`（repair，`sourceTaskId=t118`、`sourceFindingIds=[GEN4-EX-R1]`）与 **`t139`（`review-round-2`，`depends on t138`）**，并把**原本挂在旧评审上的集成单 `t121` 的依赖重新接到 `t139`**。⇒ 链子自动变成 `t118 → t119(✓) → t120(needs_revision) → t138(修) → t139(复审) → t121(集成+推送载荷)` | ✅ **已处置（学到即用）**：① **不要手工再建 repair/复审单**（会与自动派生的重复）；② 我建 `t138` 时平台报 `inScope overlaps t138 at crates/mcp/src/lib.rs` —— **那次报错反而救了我**，它说明「我漏 `subject` 的那次调用其实已经建了单」⇒ **判据**：`create` 报「与**同号**任务重叠」时，**先去 `edit_plan` 补它的 subject/assignee**，不要再建第二张；③ 自动派生单**依赖也自动接线**，集成单的「等谁」不要手工维护 |

**通则（给下一位）**：撞到 `inScope overlaps <已冻单>` 时，**不要**去改自己任务的路径（那是把工作挪开而不是把问题解决），也不要去削弱被冻单的 inScope（`update_task` 本来就改不了）。三条合法出路，按优先级：① **用读数证明被冻单的前提已被取代 ⇒ captain 直接取消它**；② 把工作**真的**挪到不重叠的路径（只有当工作本来就可以那样切分时才成立）；③ 记下冲突**等**（例如 `t64`/`t89` 这一对）。**本代已经在这条规则上付出了两次代价**（`t96` 堵住 api.rs 一轮；`t111/t112` 曾被 `t110` 串行化）。

## B28. `crates/extract` 的第一次独立审计（t125，verify2 攻击式）：4 条声称成立、5 条不成立、2 处机械证明被打红

**报告**：`docs/design/reviews/gen4-audit-extract.md`（44,763 B，§0–§8；提交 `1bf51f2`）。**被审对象**：capability 那代新增的整块 crate（~2755 源码行 / ~1247 测试行），**此前从未被独立审过**。

**先记成立的（正读数，别只记 findings）**：① **zero-token 是结构性的** —— `Cargo.toml` **无 `[dependencies]`**、`Cargo.lock` 只有 dev-dep `serde_json`、19 项 FORBIDDEN 扫描五文件 clean、`Rules` 臂体内无 `ask_agent`（模型只在 `AcpExtractor::ask`）；② **`max_per_input` 是精确的齿** —— 记忆 3/4/5/6/7 轮（cap=5）→ 3/4/5/5/5 且 `truncated=false/false/false/true/true` ⇒ **「正好 n」与「被砍在 n」可区分**（t9 那套口径）；③ **确定性跨进程**同指纹；④ **图谱路径真有界**（1/4/16 MB → 34/23/39 ms、输出恒 3）。

| id | sev | 一句话 | 状态 |
| --- | --- | --- | --- |
| **F1** | medium | **`tests/bounded.rs` 的名字在承诺有界，而转录路径没有按字节的界** —— `max_turns` 只界条数；单轮文本整段进 `split_sentences`（先建 `char_indices()` = 16 B/字符）⇒ 实测 `137 → 513 → 1943 ms`、**峰值分配 = 输入 ×16**（16/64/256 MiB）；而那个 1.1 MB 单轮用例**只断言输出条数** | **`t132` 在办**（verify2） |
| **F2** | medium | **`max_content_bytes` 的丢弃是静默的**：46 B 句 + cap=45 ⇒ **0 候选而所有计数器全零**（`truncated`/`suppressed`/`turns_skipped` 都不动）⇒ 调用方分不清「句子太长被丢」「没有规则命中」「去重了」。**与这个 crate 自己在 `extract_plane.rs:457-489` 立的规矩同类**（「调用方必须能区分 exactly 32 与 capped at 32」），只是**在另一个位置复发** | 未立单 |
| **F3** | medium | **gold fixture 不钉证据字段**：记忆侧渲染只比 `rule\|store\|confidence\|content`；图谱侧 6/7 段无 `origin`/`source` ⇒ 实测**篡改 `origin.index-1` + 翻转 `role` 仍被接受**、`source="WRONG-SESSION"` 渲染相同；图谱 `section=Some(99)`+`source=""` **两个渲染串相同且全部 in-pass 断言通过 ⇒ 接受=true**。**（它如实列出：`index+1` 被范围检查兜住了 —— 这条反例让 F3 的边界准确）** | 未立单 |
| **F4** | medium | **证据链缺位置一维，且现有坐标无人消费**：`split_sentences` 的 offset **算得出来却被丢弃**（`memory.rs:147`）、三个候选结构体都无区间字段，且**全仓 `origin` 消费者为零** ⇒ 一行记忆/图谱边**回溯不到具体轮次/章节** | 未立单 |
| **F5** | low | `window_text` 注释承诺「回退到前一段落边界、段落绝不会被抽一半」，而 `None => cut` 是**生切**（200,000 B 单段落实测 `bytes_skipped=134,464`、窗口末尾落在段落内部） | 未立单（**可能升级**：见下 G1） |
| **F6** | low | **`min_score` 齿在出厂值上不咬**：默认 `0.0`，而图谱实体最低分 0.25 ⇒ 永不裁剪；两个抽取 capability 只声明 `max_per_input`/`max_docs_per_pass` ⇒ 生产路径上这个齿**从未被设过** | 未立单 |
| **F7** | low | **「四个齿」不在同一个 crate 里**：`max_per_input`/`min_score` 在 extract（后者惰性）、`max_docs_per_pass` 在 **daemon 的摄取扫描**、`weight` 是**召回腿融合权重**（与抽取无关）；且**默认抽取路由是 `acp`（花 token）、零 token 层默认 off** | 未立单（文档归属问题） |
| **F8** | low | **内容预算按字节、名字预算按字符 ⇒ 脚本间不对称**：同一 `max_content_bytes=400` 下 140 ASCII 字符（156 B）留着、140 汉字（416 B）被丢 ⇒ 汉语有效句长预算约为 ASCII 的 1/3 | 未立单 |
| **F9** | low | **纯度扫描的文件清单是手写 5 项常量** ⇒ 隔离副本里加 `src/io_probe.rs`（真 `std::fs`）+ `pub mod io_probe;` 后 **扫描仍判 clean**（而副本能编译）⇒ 真实的 I/O 可达路径能绕过「纯度的机械证明」 | 未立单 |

**两条猜想（§5，作者明确不混进 findings）**：**G1「半句变逐字证据」** —— F5 的生切原则上让窗口末尾出现半句，而关系/实体把所在句**逐字**写进 `fact`/`summary`；作者**没造出来**（碎片被 df≥2 闸门与反引号配对挡住），但给了**精确的下一步配方**（把 64 KiB 切点对准**成对定界符的闭括号之后**、或 inline-code 的**闭反引号之后**，同时让该 token 在前文出现 ≥2 次）⇒ **若能造出带半句 `summary`/`fact` 的候选，F5 应从 low 升为 medium/high**（因为它把「半句」写成了「逐字证据」）。**G2** `min_score = NaN` ⇒ 恒假⇒静默清空（今天 daemon 校验 finite 且 0..=1，产不出 NaN）。

**审计者自己的纪律（值得记）**：① **拒绝抬高分级**并写出理由（无 blocker/high：没有一条会把**错误的结论**写进记忆/图谱；唯一可能伪造事实的路径**没造出来** ⇒ 进猜想）；② 列出**与结论相反**的读数（`index+1` 被兜住）；③ 把读数**钉在字节上**（五个文件 SHA-256，因为 HEAD 在它侦察期连移三次 `873bb90→db46f6c→e8aaead`）；④ 环境纠正：活 daemon **pid 79984 开工时已不存在**（8787 由 14944 持有），`%TEMP%` 里另有 11 个不是它的 `t125*` 条目，**只删自己那一个具体路径**。

## B29. `design-audit.mjs` 自己那份「危险默认」——t107 的修法只落到了两处中的一处（未立单）

**读数（`t122` §0 第 2 条 + §1 ④-a，坐标在当前字节）**：`panel/tools/design-audit.mjs:401` = `baseUrl: process.env.E2E_BASE_URL ?? "http://127.0.0.1:8787"`。⇒ **在一台有活守护进程的机器上不带 `--base-url` 跑 `--check`，就是对准活库**。

**为什么这是独立的一条（不是 t107 的重复）**：`t107` 修的是 **e2e 的入口点**（`panel/e2e/run-e2e.mjs` + `playwright.config.ts`：不设 `E2E_BASE_URL` 时**拒绝猜目标**），而 `design-audit.mjs` **有它自己的一份默认值**，**没有**被那次修法覆盖 ⇒ 同一个危险默认在**第二个地方**活着。这是本代的核心失败族的又一实例：**同一件事必须多处同改**（§B26 的 M1 是消费方与生产方的漂移；这里是「修了一个调用点，另一个调用点还带着旧默认」）。

**严重度与可达性**：`--check` 会驱动浏览器、读面板、并（在本机）可能触及活守护进程的读端点；t122 的作者**全程显式传 `--base-url`**，且把「这个默认值未被行使」列进**未测项** ⇒ 今天**没有被触发过**的证据，但这正是「守卫在，而它保护的东西已经不对」的形状（与 pid 守卫 C32、纯度扫描清单 F9 同族）。

**建议的修法形状（立单时用）**：与 t107 在 e2e 入口点采用同一条规则 —— **要么显式要求（缺 `--base-url` 即拒绝并点名），要么拒绝 8787 这个默认值**；判据要给**能红的负控**（不带 `--base-url` 跑 ⇒ 必须拒绝，而不是「悄悄地打 8787」）。**未立单**：`panel/tools/**` 此刻无主，且成员都在跑；留给下一轮（与 F2 的 `audit.yml` 同族，可在同一轮收）。

## B30. 工具侧线索（未立单）：`design-audit.mjs` 的死端口控制在抖动下会走**另一条路径**

**读数（`t131` 作者自报，负控步 `:131-157`）**：把 `--base-url` 指向保证无人监听的地址时，**5 次观测里 4 次**是健康形状 —— `exit 2 / 0.44s`（3 次 0.46/0.41/0.42s）+ 原话 `daemon not reachable at http://127.0.0.1:1 (no panel to audit). Start it with: cargo run -p ruagent-cli -- serve`；**另 1 次是 `129.9s` + 栈回溯**，仍 `exit 2`，但**报文里没有那句点名**。

**为什么它是一条线索而不是「护栏太严」**：健康的形状是「**快速拒绝并说清为什么**」；那条抖动的形状是「**跑了两个数量级的时间、吐了栈回溯、且没说人话**」—— 它更像**挂住/异常**路径。⇒ 处置：**不许**为了让负控稳定变绿而放宽断言（`rc==2` **与** 报文含 `daemon not reachable` **两半都留**）；若它在 CI 上复现，那是 `design-audit` 的**独立缺陷**，单独立案。

**同族**：本代的 `C34`（窗口的代价不因宣告而消失）、以及「**控制红/绿 ≠ 控制对了**」那 7 例 —— **一个接受任意「exit 2」的负控永远是绿的，而它本该证明的东西已经不在**。

**未立单的理由**：`panel/tools/**` 此刻无主；且它只是 **1/5 的抖动**，先登记证据（时间、退出码、报文缺失、栈回溯），等复现或等 `panel/tools` 有人再立。**复现配方**：对无人监听的地址重复跑 `node tools/design-audit.mjs --check --base-url=http://127.0.0.1:1 --out=<仓库外>`，记录 wall-clock 与是否含 `daemon not reachable`。

**有界复现（t131 作者，16 次补充，全部干净）**：`10×` 顺序裸形式（exit 2 / **0.38–0.46s** / 10/10 点名）· `3×` 对**刚刚停止监听**的端口（0.39s / 全点名）· `3×` **并发三条**（0.43/0.53/0.66s / 全点名）⇒ **16/16 exit 2 + 点名、无 >5s、无栈回溯**。合计观测 **21 次 / 1 次异常**，且与「重复执行」「端口刚变死」「并发」**都不相关**。

**更强的假设（未证实，但有可构造输入 —— 交给立案者）**：那一次异常落在 **8899 正被 `t128` 的临时 daemon（PID 31020，启动 `00:24:44`）绑定的窗口内**。「**连接被拒绝**」与「**TCP 连上了但对方不服务**」是两件事：前者让点名检查**快速**返回 `daemon not reachable`；后者会让 run 进入 **`auditRoute`（`design-audit.mjs:3857`）**，在 **13 条路由**上各自等到超时（合计 **~130s**）后抛错，再由 **`:8436-8439` 的 catch** 换成 **exit 2 + 栈回溯** —— 与该次观测的**全部特征吻合**（既慢、又没那句话、且有栈回溯）。**构造输入**：让目标端口上有一个**会 `accept` 但不服务**的监听者（或一个正在绑定的进程）再跑控制 —— 若能复现，就同时钉住这条路径**与它的可判定输入**（这条线索就能从「抖动」升级为「一个有确切触发条件的缺陷」）。


## B31. `entities.name NOT NULL` **全仓没有测试钉住**（`t119` 的独立验证带出的 low finding，未立单）

**读数（t119 报告 §残余风险 + L1）**：存在信号是 `name`（`null` ⇔ 不在图里），而该信号**依赖一个 schema 约束**：`crates/store/src/migrations.rs` 的 `0005_graph.sql:9` = `name TEXT NOT NULL`（sha256 `505F0F81A4E599CD…`）。但**全仓没有任何测试钉住这个 `NOT NULL`**：约束类命中**全是 FOREIGN KEY**；仓库里唯一碰 `PRAGMA table_info` 的家什（`migrations.rs:637-661`）断言的是 **`notnull == 0` 的反极性**（它检查的是「某列**不是** NOT NULL」，恰与本条要钉的方向相反）。

**后果（为什么这不只是「缺个测试」）**：将来一次表重建（SQLite 的列变更要走重建）若把 `NOT NULL` 丢掉，**「实体存在但 `name` 为 NULL」会被读成「不在图里」** —— 而**门禁全绿**。这正好是本代反复打的族：**守卫/判据还在，它保护的不变量已经不在**（C32 的 pid 守卫、F9 的纯度扫描清单、§B29 的第二份危险默认、t119 自己证伪的「空转断言」都是同一族）。

**可证伪的判据（立单时照用）**：把 `("entities","name")` 加进一条断言 `notnull == 1` 的测试 ⇒ 今天它**必须先红**（因为该断言尚不存在），补上后绿；再把 `0005_graph.sql` 的 `NOT NULL` 去掉（**隔离副本**）⇒ 该测试**必须红**。

**未立单的理由**：`crates/store/**` 此刻无主（t94 的存储层审计已终结）；且这条需要「改 schema 的隔离副本负控」才有判别力 —— 成本明确后立单更稳。**关联**：t119 的残余风险结论是「`name` 不构成阻断性 finding」（因为 `NULL` 在今天的 schema 下造不出来），所以本条**不是** t118 的缺陷，而是**它依赖的不变量没有守卫**。

## B32. 可照抄的范式：**源码扫描型守卫的三件套**（`crates/daemon/src/extract_plane.rs` 已落地，captain 本轮读到）

**现场**（`extract_plane.rs:995-1057`，t137 改动旁边）：这是本代一直在要的那个形状，而且**已经在树里**，不必新造 ——

1. **扫描下限**：`assert!(scanned > 10, "the scan must actually walk the daemon's sources; it walked {scanned}")` ⇒ **扫到 0 个文件不是「干净」，是「守卫失明」**（对照本代的反例：F9 的纯度扫描手写清单、C32 的 pid 守卫 —— 它们都**没有**这一半）。
2. **植入控制**：`let planted = "...session.send(ChatCommand::Prompt {..."` + `assert_eq!(unmarked_prompt_sends(planted), vec![2], "the scanner must fail on a sender that skips the marker, otherwise the check is vacuous")` ⇒ **证明扫描器不是空转的**，而且**控制永不进树**（写在测试里）。
3. **写明排除 + 理由**：跳过 `chat.rs`（「the user's own path, by name and for a stated reason」）、在 `#[cfg(test)]` 处截断（理由：测试模块里的字面量会提到该符号，而它们不是发送者）⇒ **每处排除都给出对象集理由**，而不是「这样测试就过了」。

**为什么值得单独记**：本代记了太多「守卫静默失明」的缺陷（C32 pid 守卫 / F9 纯度清单 / §B29 第二份危险默认 / B31 未守卫的不变量），**却很少记「长对了的样子」**。这一处可以直接照抄到其它扫描型守卫（例如 `.github/workflows/scripts/*.sh` 的路径检查、`check-workflow-refs.sh` 的引用检查）：**下限 + 植入控制 + 具名排除**，三件缺一，扫描就会在某天变成空转。

## C. 质量门与仓库工程

| # | 事项 | 证据 | 状态 |
|---|---|---|---|
| C1 | **CI 红 · `Format`** | `cargo fmt --all --check` 本地 **170 处** diff（多在 `daemon/src/distill.rs`）；因格式步在第一位，**ubuntu 上 clippy/test 从未跑到** | t66 |
| C2 | **CI 红 · `Test`(windows)** —— **工作区已修好，待提交** | 原挂 `crates/mock-agent/tests/e2e_daemon.rs:2161` 的 `distill_graph_toggle_controls_entity_extraction`（t347 移除 `[distilled]` 正文前缀后的过期期望）。复核：t70 读 `:2161-2179` 确认**已修好**（该行今天是注释）；captain 本机复现 `test -p ruagent-mock-agent --test e2e_daemon distill_graph_toggle_controls_entity_extraction` ⇒ `1 passed; 0 failed`、wrapper **exit 0**。**剩下的只是提交**（t66） | t66 |
| C3 | CI 加固：跳过必须可见 / 计数口径 / 门禁覆盖声明 | `#[ignore]` 活库仪器共 13 条（store 3 · graph 3 · daemon `injection_e2e` 7）在默认跑里只显示 `ignored` | t65 |
| C4 | 首次推送前的卫生审查（captain 已验） | 120 条目 / untracked 79 / 1.9 MB / 无大文件 / 无被漏掉的构建产物 / **密钥扫描 0 命中** | ✅ 已验 |
| C5 | CI 其余步的潜在失败（captain 已探） | nested-Result 守卫（t324）在 93 个 `.rs` 上 **0 命中**；`clippy --workspace --all-targets -D warnings` 本地 **exit 0**（重查 memory/daemon/mock-agent/mcp/cli） | ✅ 已探 |
| C6 | `scripts/cargo-team.ps1`（8.1 KB，未跟踪）应随提交进入 | `scripts/` 下已有 5 个跟踪文件；它是本代构建纪律（共享 target / 一次一个编译 / 0–11 核）的载体 | t66 |
| C7 | 推送方式：**直接推 `main`**（沿用仓库现状——远端所有 CI 记录都是 push 事件） | 若改为 PR + 评审合并，需先立规则 | 已定（可改） |
| C8 | 成员上限 8 ⇒ 无法新增专职发布工程师 | `add_member` 被平台拒；发布/CI 职责交给空闲的 `recall` | 已定 |
| C9 | **推送前自检的正确形状**（captain 已探明；天真实现会误报） | 工作区 `ci.yml` 357 行/18,998 B vs 提交版 110 行；`test-evidence` 命中 **工作区 7 / 提交版 0**。**真实未跟踪且被 5 处引用**（`ci.yml:111/131/243/264/274`）的只有一个：`.github/workflows/scripts/test-evidence.sh`（70 行/2,580 B）。天真检查会误报 2 类：① `scripts/build-panel.mjs`、`tools/design-audit.mjs` 实际在 `working-directory: panel` 之下（`panel/...`，**已跟踪**）；② `docs/design/reviews/gen2-ci-hardening.md` 是 `ci.yml:3` 的**散文引用**。⇒ 自检必须**只取 `run:` 体里的可执行引用**、**相对路径按 `working-directory` 解析**，再做 tracked 断言 | t66 |

| C10 | **CI 的测试计数必须覆盖全部 target ⇒ 用 `--no-fail-fast`**（captain 裁决） | `cargo test --workspace` 默认在**第一个失败 target 后中止** ⇒ `test-evidence.sh` 的「targets reporting」**按构造残缺**，「0 targets = error」也挡不住「后面的 target 根本没跑」（第 6 条同族） | t66 |
| C11 | **`test-evidence.sh` 在日志缺失时静默地绿**（captain 实测） | `bash .github/workflows/scripts/test-evidence.sh missing .does-not-exist.log 0` ⇒ 三行 `integer expression expected` + **exit 0**（计数字段为空串时 `[ -gt ]`/`[ -eq ]` 静默为假）。修法：开头 `[ -r "$log" ] \|\| exit 1` + 计数默认 0；负控：删日志 ⇒ 必红、空日志 ⇒ 必红（后者今天已经是红的，别退化） | recall（t65/t66） |
| C12 | **键名现状（captain 机械核对，纠正 t80 的误读）** | `api.rs:3472` = **`"log": rows`**（GET `/recall/log` 的响应键）· `:3455` 的 `"rows"` 属**计数端点**（同响应有 `max_rows`）· 消费方 `knowledge_api.rs:737`、`Memory.tsx:82` 都读 `["log"]` ⇒ **契约写 `rows`、实现仍是 `log`，不一致未收口** | t83（三处同改） |

| C13 | **平台的 inScope 串行规则（captain 撞了 4 次）** | 新增/编辑一个任务时，若它的 inScope 与**任何 pending 任务的 inScope 重叠，就必须把那个任务列进自己的 `dependencies`** —— **跨成员也一样**（我第 9 轮为「裸 `#[ignore]` 守卫」依次被 `t65`→`t77`→`t83`→`t85` 挡回）。⇒ 规划时应**先算路径交集再写依赖**；同目录（如 `scripts/`、`crates/daemon/tests/`）会让多个单互相串起来 | captain |

| C14 | **构建锁争用会把成员的工具调用饿死**（captain 观察 + 成员实测） | 共享单飞锁上排队导致的**工具调用超时**：integ 私有构建 **479.9s 无产物**、graph 的 `check` **600s 无输出**、我两次 **120s/300s** 超时。⇒ **只读检查**可用 `-NoLock` + **私有 `-TargetDir`**（verify2 实测 `core 11 + orchestrator 4 + policy 7 = 22 passed/0 failed`），或把构建放**后台**再收；**写型/正式门禁仍必须走锁**（一次一个编译是纪律，不是障碍） | 各属主 |
| C15 | **共享树停在不可编译状态的代价**（t81 实例） | graph 分步改同一文件时停在不可编译窗口、**未广播** ⇒ integ 白烧 479.9s 私有构建 + 四探针全打在「文件不存在」上；`test --workspace` = exit 101 且 `ok=0 FAILED=0 Running=0`（**根本没编译过**）。**纪律候选（第 23 条）**：跨文件/跨步骤改动前先确认当前树可编译；若不可避免，**立刻广播 `file:line` + 错误原文**。graph 已认领该违反并改为「每跨文件前先跑 `check -p ruagent-graph --all-targets`」 | 契约（t64 落地时收编） |

| C16 | **推送后立即核 CI 的清单**（captain 自用，避免「推了就算完」） | ① `gh run list --limit 3` 确认新 run 出现且 **headSha 对齐**要推的 commit；② 对每个 run `gh run view <id> --json jobs` 取**逐 job 结论**，红的取**失败步名**；③ **特别核 evidence 步真的跑了**：摘要里计数非空、`Running ` 与 `test result:` 行数 > 0（刚加固过的脚本不能「跑了但没读数」）；④ **windows job 只在 main push 跑** ⇒ 必须确认它**有结论**（不是 skipped）；⑤ 若红：把失败步名 + 日志片段贴进总账，开修复单（owner = 该步属主），**不许**在红的状态下宣称「CI 绿」 | captain |

| C17 | **第 21 条的守卫是死代码**（t92 发现） | `.github/workflows/scripts/check-workflow-refs.sh`（t65/F1 产物）**全仓 0 调用点**（除自身 `:30/:57`）⇒ 与 `RUAGENT_REQUIRE_MOCK` 同形（「谁打开它？」→**没有人**）。它已按「**欠近似**」写好（注释里记着天真版本误报过的两类：`panel/scripts/…` 被读成 `scripts/…`、文档头注释里的路径是散文）⇒ **只需接线 + 入库**（已并入 t66 的 F1 项：跟踪入库 + 在 `ci.yml` 里真的调用 + 让它失败即红） | t66 |

| C18 | **平台依赖规则的第二形态：repair 不许依赖已失败任务** | 我试图把 `t83` 硬挂进 `t66` 的依赖（防止推送快照到「半改键名」的树），平台先拒 `repair must not depend on failed task \"t59\"`（`t66` 的旧依赖里有失败的 `t59` ⇒ 必须摘掉），摘掉后再拒 `inScope overlaps t86 at .github/workflows/`（`t86` 依赖 `t66` 且共享该路径）。⇒ **与在途从属单重叠时，该任务的依赖根本改不动**；硬保证拿不到时，改为**一行的提交前一致性检查**（我在消息里给了 recall 具体命令：服务端键与每一个消费方必须一致，`grep '\["log"\]|\["rows"\]'`） | captain |

| C19 | **失败依赖会永久冻结一个 pending 任务（连取消都不行）** —— 本代最贵的结构性限制 | 实测链：`DELETE`/失败让 `t59`、`t83` 终态失败 ⇒ ① 任何**依赖它们**的 pending 任务被永久封锁（`reassign_task` 原文：`task t64 is blocked by unfinished dependencies: t59 — complete them before captain takeover`）⇒ **既不能重派、也不能取消**；② 更糟：被冻结任务的 **inScope 对所有人变成禁区**（新单与它重叠就必须依赖它 ⇒ 一起死）；③ 而**依赖编辑又常被「与在途从属单重叠」挡住**（`t86`→`t66`、`t89/t90`→`t64`），于是连「摘掉失败依赖」都做不到。**本代被冻结**：**t64**（预算账本接线 + F7 + D-11 契约同步）· **t89**（权限审计可辨，C-3）· **t90**（状态机住 `core` + 13 处接线，C-1）—— 三者的内容都已在总账登记，可在**新会话/新团队**中干净重建（或平台提供「取消被封锁任务」后解冻）。**教训（今后立单纪律）**：**宁可拆 inScope 路径，也不要把关键路径挂在可能失败的任务后面**；`t66`（推送）就因此被 `t59`/`t64`/`t83` 连咬三口，每次都要拆路径抢救 | captain |

| C20 | **main 上 CI 红的确切基线（`gh` 直查真实 run，非推断）** | run `36272344732`（headSha `5db881d` = `origin/main`）：**Rust (ubuntu) = failure，失败步 `Format`** ⇒ **`Clippy`/`Guard against nested-Result misjudgement`/`Test` 全部 `skipped`**（也就是说 ubuntu 侧的静态检查与测试**从未在这条 run 上跑过**）；**Rust (windows) = failure，失败步 `Test`**（`crates/mock-agent/tests/e2e_daemon.rs:2161` 的过期期望，**已在工作树里修好**）；**Panel (node) = success**；独立 E2E run `36272344709` = **success**。⇒ `t66` 要翻转的正是这两条：`cargo fmt --all`（现 178 处）+ windows 重跑测试。**取证指令**：推完必须用 `gh run view <id> --json jobs` 逐 job 取「结论 + 失败步名」，并确认 ubuntu 的 Clippy/Test **这次真的跑了**（不是又一次被 Format 短路） | captain |

| C21 | **跨 Windows→WSL 测退出码：只能用 `$LASTEXITCODE` 或内部 `if`，`echo $?` 不可信** | 我在多行 `wsl bash -c '…; echo "RC=$?"'` 里读到**全 0**（含 `check-workflow-refs.sh` 打印了 `::error::` 却「退出 0」）⇒ 差点**对正确的守卫报假警**、进而「修好」一个并不存在的 bug。判定性对照：`wsl bash -c "exit 7"` 用 `echo $?` 也读 **0**，而 **PowerShell 读 `$LASTEXITCODE` 得 7** ⇒ **测量通道本身是坏的**。真相（两条独立通道一致 + `bash -x` 轨迹显示 `+ exit 1` 确实执行）：**F1 守卫在今天的树上 exit 1**（6 处引用 2 个未跟踪脚本，**零误报**；`panel/` 路径正确按 working-directory 解析并标 tracked）· 证据脚本 **缺日志 ⇒ exit 1** · **空日志 ⇒ exit 1** · **健康日志 ⇒ exit 0** ⇒ **整套 CI 加固是载重的，不是装饰**。**纪律**：测退出码要用**调用方的退出码通道**（`$LASTEXITCODE`）或被测程序内部 `if`；`echo $?` 在多行 `bash -c` 里会静默给你 0 —— 这是「工具默认值/测量口径」族的又一实例（与第 17/20 条同族） | captain |

| C22 | **变异负控不得把共享树弄红**（t93/t81，成员自报的流程 finding） | 为证明「测试能红」而**临时改共享源码**（把 `facts_of` 改回 `unwrap_or_default()`、把事务降级成 autocommit、改断言主语）会让**全员的 `test --workspace` 在那个窗口变红**；本代已因此产生**一次误诊**（`crates/mcp/src/lib.rs:495` 的红被读成「测试已写、实现未落」）。⇒ 纪律：**在隔离检出/`git worktree` 里做**，或**事先广播**（哪些文件/哪些测试/预计时长）+ **限时** + **报告窗口起止与恢复后的绿读数**。**没广播的变异窗口，对其他人来说与被它抓的那个 bug 无法区分**。已写入 `AGENTS.md` | 全员 |

| C23 | **规则上移一层必须对旧调用点幂等**（t73，captain 采纳为通则） | 把净化规则从 `memembed.rs:200` 上移到共享 `query.rs::fts_pattern` 时，若不做幂等（`fts_pattern(fts_pattern(x))==fts_pattern(x)`），**旧调用点传进来的已是短语** ⇒ 共享层再转义一次会让召回 keyword 腿**静默 0 命中**。⇒ **通则：上移一条规则时，必须保证它对旧调用点幂等，否则上移本身就是一次静默行为变更**（与「同一件事两处说」「不可判定被合法值掩盖」同族）。附带读数：`before(raw) errors=8/11 → after ok=11/11`；负控 `deploy`/`部署`/`"deploy script"`/`café` 前后一致；**唯一差异已明写**（多词 `deploy script` 由隐式 AND `[1,2]` → 短语 `[1]`，理由是 keyword 腿一直如此，替代方案会拆坏合法引号查询）；**顺带修好真实查询 `release.sh` 的 500**（`.` 是 FTS5 语法字符） | 全员 |

| C24 | **`Select-String 'FAILED'` 默认大小写不敏感 ⇒ 把 `0 failed` 数成假红**（t81 自报） | 成员第一遍统计测试行时，把 **12 行 `0 failed`** 计成 FAILED ⇒ **假红**（与假绿同族、方向相反，同属「测量口径的默认值本身就是缺陷来源」）。**判据**：统计一律 `-CaseSensitive` 并**锚定 `test result: FAILED`**；`test-evidence.sh` 的口径正是如此（按 `test result:` 行分类 + 大小写敏感）。与第 17/20 条、C21（跨 WSL 的 `$?` 永远是 0）同族 | 全员 |

| C25 | **提交/推送前必须查「最近被写过的文件」，并对已提交字节跑编译面**（t66 首次推送事故，**captain 自犯**） | 在 `lifecycle.rs` 被写后 38 秒提交、且跳过推送前的编译面 ⇒ 推上去的提交**编译不过**（`E0425 backup_surface`），两个工作流同因变红。**判据**：① 提交前查 `git status` 中是否有文件 mtime 落在最近 ~60 秒内，有则**等**；② **推送前**对已提交字节跑 `check --workspace --all-targets`（本地绿 ≠ CI 绿，但**本地红一定 CI 红**）；③「安静窗口」不是奢侈品，是提交的**必要条件**；④ 诊断顺序：**先取 CI 原文，再怀疑环境**（我这次先怀疑了工具链漂移与平台分叉，两者都不是） | captain（沿用） |
| **C25a** | **编译面必须在 CI 的口径下取**（同上，**第二次犯**） | 我的推送前 `check` 打印了 `Finished` 我就读成绿 —— 但**我自己的过滤器**（只找 `error` 与 `warning: unused`）把 **4 条真实 rustc 警告滤掉了**，而 CI 是 `clippy --workspace --all-targets -- -D warnings` ⇒ **警告即错误**。**判据**：推送前的编译面要么直接跑 **`clippy … -- -D warnings`**（与 CI 同命令），要么**必须报出 warning 行数**并在非 0 时停；**「Finished」不等于「没有警告」**。与 C21/C24 同族：**测量口径的默认值本身就是缺陷来源** | captain（沿用） |

| **C25b** | **推送前的固定面 = fmt + clippy(`-D warnings`)，每次都要重跑**（第三次推送，captain 自犯的缺口） | 我只在**首次**推送前跑过 fmt；第二、三次推送前跑了 clippy 却**没有重跑 fmt** ⇒ `877a909` 的 ubuntu **红在 `Format`**（唯一站点 = `crates/graph/src/lib.rs:1845`，graph 的 t82 改动留下的一个长元组；成员各自的「fmt 0 改动」都是**对它自己的文件、在它自己的时刻**成立，**树是累积的**）。**判据**：每次推送前在**同一份字节**上依次跑 ① `cargo fmt --all --check` ② `clippy --workspace --all-targets -- -D warnings` ③ 并报出 warning 行数；**推送者负责最终那次检查**（作者各自的 clean 不能替代它）。修复：`rustfmt --edition 2024 crates/graph/src/lib.rs`（5 增 1 删，纯规范） | captain（沿用） |

| C26 | **改了源码但没重建产物 ⇒ 假绿**（t103，integ 自曝） | 面板行为验证时，它改了 `Runtimes.tsx` 却**没有重建 `dist`** 就重跑 e2e ⇒ 服务的是**旧 bundle** ⇒ 负控「通过」了，但那是**旧行为**。它自己判定该次负控**无效**并重跑（删掉 F2 的清标记行 + `npm run build` ⇒ `expect(locator).not.toHaveText` 在 `failure-visibility.spec.ts:253` 红、1 failed/5 passed/exit 1；恢复后重建 ⇒ 6 passed/exit 0）。**判据**：**面板的任何行为读数（绿或红）都必须在 `npm run build` 之后取**；「上次构建的产物」是本代第 15 条（陈旧产物）的实例 | 全员 |
| C27 | **`immutable=1` 是正确性，不是优化**（t75，mem-core 实测） | 朴素 `SQLITE_OPEN_READ_ONLY` 打开一份 WAL 副本，会**在用户数据目录里生成** `-shm`/`-wal`（实测生成 32,768 B 的 `-shm` 与 0 B 的 `-wal`，**而一个「遗忘报告」不许写它正在报告的那个目录**）。⇒ 判据：读副本必须 `immutable=1`；候选集排除 `-shm`/`-wal`；**带非空 `-wal` 的副本判为未测**。同族的两条：**读不出来的候选 ⇒ 该面 `total: None`（未测）而不是 0**；**文件种类必须按字节嗅探而不是按扩展名**（`-150802` 结尾的副本被 `.db` 过滤器漏掉 ⇒ 候选 1 vs 2） | 全员 |

| C28 | **合法 YAML ≠ GitHub 可加载的工作流：非法 `${{ }}` 会拒掉整个文件，而 YAML 解析器看不见**（captain 本轮真实事故，一行修复 `5d3adfd`） | t104 在 `e2e.yml` 的 `run: |` **块标量内部**（即 shell 脚本正文、**不是 YAML 注释**）写了字面量 `` `${{ ... }}` ``。**GitHub 对整段 `run:` 字符串做表达式替换（shell 注释也照做）** ⇒ 去解析 `...` 这个非法表达式 ⇒ **整个 workflow 文件被拒**：run `36484582363` **0s、无 job**、`name` 退化成**文件路径**、纯文本视图直说「This run likely failed because of a workflow file issue」。**PyYAML 说该文件合法**（我复核过：229 行、0 tab、唯一 `${{` 就在那处）⇒ **守卫与我都漏了它**。**判据**：① workflow 里的 `${{ }}` 必须**逐个是合法表达式**，且在 `run:` 块内也要检查；② `concurrency.group` 里的合法表达式**不许**被误报；③ 推一个改动过 workflow 的提交前，**必须用能看见这一层的检查**（已并入 t106 的验收，带「写 `${{ ... }}` ⇒ 必须红」的负控）。**旁证**：`ci.yml:30` 的 `${{ github.workflow }}-${{ github.ref }}` 是合法的、不在 run 块内 ⇒ 那次 CI run 只是 `pending`（没被拒），两件事正好互证 | captain + t106 |

| **C29** | **登记的 follow-up 也会过期：立单前必须回到字节**（captain 本轮，差点白烧一整轮） | capability 设计文档 §20 第 4 条列着三条 follow-up（让不可传输的 MCP 键**响亮**、删掉 prose 里的数字边界、加字段集守卫）。派活前核当前字节 ⇒ **三条都已落地**（坐标见 §B25）。若照文档立单，成员会（正确地）拿现有代码回报「已完成」⇒ 白烧一轮，且会让人误以为「登记还开着」。**判据**：任何由文档/账本行号驱动的立单，派活前先在**当前字节**上核一次；**文档说「未决」不等于今天未决**。同族：C22、§B25 的两处坐标漂移（`api.rs:1207-1216` 已指向 wiki corrections）。 | captain（沿用） |

| **C30** | **「看起来是缺口」不等于缺口：候选缺口必须由读数了结，而最便宜的那次读数往往就是流水线自己的输出**（captain 本轮，三次假设三次被推翻） | 本轮我按「找缺口」的姿态提了三条候选，**全部被当前字节或 CI 自己的读数否掉**：① capability 设计文档 §20 第 4 条列的三条 follow-up（`deny_unknown_fields` / 删 prose 边界 / 字段集守卫）—— 核字节发现**都已落地**（见 §B25）；② §22.3「面板 e2e 有一条从未打开 `#settings` 的绿色套件」—— 该文自己写着 **After: 51 passed / 2 skipped** 且 `panel/e2e/settings-capabilities.spec.ts` 存在 ⇒ **已闭合**；③ 我怀疑「Wiki 标签页无覆盖」（`views.spec.ts` 走 12 个视图、`App.tsx:39-56` 的路由表里没有 `wiki` 这个 kind）—— 但 `panel/src/views/Knowledge.tsx:25` 直接 `import { WikiTab } from "./Wiki"`（它是知识页的**标签页**，不是独立路由），`panel/e2e/wiki.spec.ts:12/79` 真的点开它，而 **CI run `37024461223` 的证据步原文**是 `playwright: exit 0, JSON says **0 skipped / 0 named**` + `CONFIRMED: no spec was skipped in this run` ⇒ 那两条 `test.skip(pages.length === 0, …)`（`:53`/`:76`）**在 CI 上没有跳**，覆盖是真的。**判据**：① 提缺口前先问「谁能否掉它」，并用**最便宜的可信读数**去试（CI 日志、证据步输出、字节）；② **条件性 `test.skip` 的存在不等于它今天在跳** —— 仲裁者是**具名跳过计数**，不是守卫的写法；③ 被推翻的假设要**写下来**（本节即是），否则下一位会重开同一张空单。同族：C29（登记的 follow-up 会过期）、C24（`Select-String` 默认大小写把 `0 failed` 数成假红 —— 都是「测量口径本身就是缺陷来源」）。 | captain（沿用） |

| **C31** | **同树构建用私有 `-TargetDir` 的代价 = 冷编整棵树，而整段窗口它独占全局锁**（captain 本轮运行期实测，非推断） | 观测（2026-10-04 00:00:51 → 00:07:59 +08:00，同一台机器）：`scripts/cargo-team.ps1 **-TargetDir %TEMP%\ruagent-t62-target** build -p ruagent` 从 `00:00:51` 起跑，到本读数时**仍在编**（`%TEMP%\ruagent-t62-target` 里 00:07:58 还在写 `lance-*.rcgu.o`）⇒ **已耗 >7 分钟且覆盖整棵依赖树**（lance/datafusion/onnx 一类）。同期 **`scripts/cargo-team.ps1 test -p ruagent-mcp` 从 `00:05:40` 起在排队**、尚无 `cargo` 子进程 ⇒ **锁按设计工作**（没有第二个编译器），但**队友的整条质量门被排在冷编窗口之后**。⇒ **判据**：`-TargetDir` 是给**隔离 worktree**（HEAD 旧字节）用的；**同一棵树上不要用它** —— 它把「一份缓存」换成「N 份缓存 + N 次冷编」，并把每次冷编的整段时长转嫁成**全局串行等待**。**不估未完成数**：本单只记「>7 分钟且仍在编」，总时长由完成它的那次运行给出。**改进候选（留给下一轮，不本轮派）**：让包装脚本在「`-TargetDir` 指向的不是共享目录」时**印一行后果**（冷编 + 独占锁窗口），一次即可，不阻断。**【2026-10-04 补充坐标（当前字节已核）】这个缺口是「镜像的一半」**：脚本在同一处**已经有**「印一行说明」的写法 —— `scripts/cargo-team.ps1:280-281` 的 `if ($noLock -and -not $targetDir) { Write-Host "[cargo-team] NOTE: -NoLock with the SHARED target dir. …" }`（它警告的是**反向**的危险组合），而 `:221` 的 `$shared = if ($targetDir) { $targetDir } else { Join-Path $env:TEMP 'ruagent-team-target' }` 就是判定共享/非共享的那个点 ⇒ **修法 = 在同一处加镜像的 2 行，并沿用 `:281` 的措辞形状**；`-TargetDir` **缺值**的那一半（t116 W-2）已在 `:156-161` 修好。与 t116/t117 的 W-2（`-TargetDir` **缺值**静默回落）同族但不同点：那条是**缺值**，本条是**给了值但给了非共享值**；且**不许在构建进行中改这个脚本**（它是全体共用的量具，改它等于「改变量具的同时在读它」）。**【2026-10-04 实测补全（C31 原写「不估未完成数」，此处给出那次运行的真值）】**同一次观测的完整时间线：`00:00:51` integ 的私有冷编**开始持锁** → `00:05:40` recall 的 `test -p ruagent-mcp` **开始排队** → `00:09:54` verify2 的 `test -p ruagent-extract`（t125）**开始排队** → `00:10:31` **锁释放**，recall 的 `cargo test -p ruagent-mcp` 立刻拿到锁（下一瞬在编 `ruagent_daemon`）。⇒ **一次私有冷编独占全局锁 9m40s**；期间 recall 的门等了 **4m51s**，verify2 的门仍在等。**锁本身按设计工作**（排队而非并发编译，全程没有第二个编译器）—— 代价是**队友的整条质量门被排在冷编窗口之后**。**注**：此处只记「锁在何时释放」，**不**声称那次构建成功（退出码归它的作者报）。 | captain（沿用） |

| **C32** | **「未触碰 pid X」是**会静默失效的守卫**：pid 在重启后不复存在，而按 pid 记账的自证仍**逐字读起来完好**（integ t62 发现，captain 复核）** | **实测（2026-10-04 00:11:52）**：`GET :8787/api/v1/health` = `{"status":"ok",…}`；`Get-NetTCPConnection -LocalPort 8787 -State Listen` ⇒ `OwningProcess = **14944**`；`Get-CimInstance Win32_Process`（`ruagent.exe`）⇒ **14944，起于 `2026-10-02 23:05:45`**；`~/.ruagent/data/daemon.pid` = **14944**（三方一致）。而本代全体自证所引的 **`pid 79984`（StartTime 09-27 05:35:37）早已不存在** ⇒ 自 10-02 23:05 起，**「未碰 pid 79984」在保护一个不存在的进程**：一个真的启停过活守护进程的成员也会写出这句完好的自证。全仓这类引用 **162 处**（`gen2-*.md` 等）—— **不改写历史**（那些引用各自属于它自己的时间窗，当时 79984 真的在），**改的是往后的写法**。**判据**：① 引用活守护进程必须读**当时的**权威来源 —— 端口归属（`Get-NetTCPConnection -LocalPort 8787 -State Listen` 的 `OwningProcess`）**加** `health`，**两个一起**，或读 `daemon.pid`（此刻 14944，与端口归属一致）；② **不许把任何被记住的 pid 当作守卫**（它只证明「某个曾经存在的进程没被动过」）；③ 时序也要说清：**本代的自证窗口跨过了那次重启**（10-02 23:05），所以窗口内的自证只有一半是有效的。**附带的第二条读数（同样重要）**：活守护进程跑的是 `D:\rust_cache\debug\ruagent.exe`（**328,059,904 B，mtime `2026-10-02 23:05:41`**，即进程启动前 4 秒），而树上的共享 target 是 `%TEMP%\ruagent-team-target\debug\ruagent.exe`（mtime **2026-09-29 09:28:43**）⇒ **两者不是同一份字节** ⇒ 任何**活库/活守护进程**的读数都属于**那份 10-02 的二进制**，**不属于当前树**；引用时必须写明它属于哪份字节（天 19/22 条同族：**一个绿读数只认证它跑过的那份字节**）。 | captain（沿用） |

| **C33** | **你自己的编辑会让**你刚写下的**引用失效**：在引用上方插入代码 = 静默把行号推走（captain 本轮实测，两处坐标错在同一份新写的注释里）** | 现场（`t127` 的 F4 注释，写完几分钟内）：① 引用路径 **`crates/store/src/store.rs:625`** —— **该路径不存在**（`crates/store/src/` = `fts.rs` `lib.rs` `migrations.rs` `sqlite.rs` `transcript.rs`），`pub fn embedder_name(&self)` 真实定义在 **`crates/knowledge/src/store.rs:625`**（同一个错误的路径也被上个 attempt 的交接报告照抄了一遍）；② 引用 `api.rs:648` 说那是 `/knowledge/documents` 的 `embedder_name()` 用法 —— **实测已是 `:664`**，成因正是**作者自己在 `:479` 插入了 +16 行**，把下面引用过的行号推走了（`:648` 今天已是 memory 路由）；③ 同一批坐标里 `crates/knowledge/src/lib.rs:24` 是**对的**，而**交接报告里写的 `:21` 是旧值**。⇒ **三条规则**：① **能按符号引就按符号引**（函数名 / 常量名 / 字段名），行号是最脆的那一部分；② 必须给行号时，**在最后一次编辑之后回读** —— **插入会让引用失效，哪怕插入前刚核过**；③ **从交接/文档抄来的坐标一律当「声称」**，读一遍再写（与 C29 同族，但 C29 讲的是「别人的登记会过期」，本条讲的是「**你自己的编辑**会让你的引用过期」）。**代价的形状**：这正是 F1 的那条注释（`api.rs:1207-1216` 漂到 wiki corrections 上）的同一族 —— 下一位读者会被送到无关的一行，而注释看起来百分之百权威。 | captain（沿用） |

| **C34** | **宣告变异窗口使红点可归因，但不消除代价：窗口内落下的全量门禁读数一律作废**（本轮两处独立读数） | 现场：`recall` 为 t118 的负控宣告了 ≤5 分钟窗口（只改 `entity_exists` 体，预期只红「不存在」那条），**但窗口内实际跑的是 `cargo test --workspace`**（起于 `00:12:21`）。**【2026-10-04 更正：我把跑 `--workspace` 的人认错了 —— 原文保留，见下】** 那条 `--workspace` 是 **`review` 为 t61 跑的锚点命令**，**不是 recall 的**；recall 的窗口 #1 实测 **`00:12:37 → 00:16:08`（3m56s，恢复后 `sha256 = 52E9F00A…B4B9EB` 与改前逐位一致）⇒ 它宣告的「≤5 分钟」是被兑现的**，且窗口内它只跑 `-p ruagent-mcp --test roundtrip`（影响面 = 1 个 target 的 1 条用例，不是全量）。⇒ **真实形状比我原来记的更尖锐**：不是「宣告者跑了长命令」，而是**一个变异窗口可以在别人已经在跑的全量门禁底下开启** —— review 的 run 比窗口起点早 16 秒开始、**跨过**了窗口起点，于是它那次全量读数死在窗口里。后果被一位**第三方**读到：`review` 为 t61 跑锚点命令 `NO_PROXY=127.0.0.1,localhost,::1 scripts/cargo-team.ps1 test --workspace` ⇒ **exit 101**，单点 `crates/mcp/src/lib.rs:1684`（`entity_existence_is_read_from_name_and_drift_is_not_absorbed`），而**同一条测试 9.6 秒后单跑 = 1 passed / exit 0**（文件 mtime `00:16:08` 晚于最后一次提交）。**它做对了**：只报「文件 + 错误」、**拒绝归因**、并声明 t63 的「workspace exit 0」本轮**不可复现**（不是被推翻）—— 这正是宣告机制要买到的东西。**但代价是真的**：那条全量读数**永久作废**，review 要么重跑要么带着一个不可复现的洞收尾。⇒ **三条**：① **窗口内只跑最小命令** —— 宣告里写哪条命令就只跑那条（`-p <crate> --test <name>` 足够证明「只有一条红」，全量跑是**把代价转嫁给别人**）；② **落在窗口内的全量读数不许当证据** —— 必须标注「窗口内取得，不可复现」并重跑；③ 窗口要**广播到位**（本次是宣告 + captain 转述，才让第三方正确拒绝归因）。**推论（比本条更狠）**：**「已宣告」只把红点从「疑似真实失败」降级为「已知扰动」，它不把扰动变成零** —— 所以宣告的**上界**必须由**要跑的命令**的实测时长推出（recall 宣告 ≤5 分钟而实际跑 8–18 分钟量级的全量门禁，上界成了愿望）。 | captain（沿用） |

| **C35** | **复用「现成二进制」之前，先用一个你确知是最近才加的字段证明它的年份** —— 一份陈旧二进制会把新字段**安静地读成「不存在」**，而那看起来正是一个结论（captain 本轮自犯：我建议了它） | 现场：我为省下构建锁的名额，建议 `mem-core` 用 `%TEMP%\ruagent-team-target\debug\ruagent.exe`（mtime **2026-09-29 09:28:43**）起临时 daemon 取数。`recall` 在 t118 里量到：用那份二进制，**不存在的实体 id 回的是 `{"facts":[]}`（旧 wire 形状，缺 `name`/`kind`/`aliases`）**，而重建后（`Finished in 1m11s`）才回 `{"name":null,"kind":null,"aliases":[],"facts":[]}` ⇒ **用陈旧字节取数会「推翻」新字段的存在**。**判据**：① 复用任何预编译产物前，用一个**你确知是最近才加的字段/行为**当探针（本轮的形状：`GET /api/v1/graph/entity/<不存在的 id>` 看有没有 `name` 键），**10 秒的成本**；② 报告里的「字节归属」必须**逐条**给路径 + mtime，不允许一句带过；③ 三条候选二进制在本机**年份各不相同**（`D:\rust_cache\debug\ruagent.exe` = **10-02 23:05:41**（活守护进程的、也是 t122 用的）· 共享 target = **09-29 09:28:43**（陈旧，缺 `name`）· 各人新编的 = 各自时间）⇒ **「哪份字节」是一个必须每次都答的问题**。**同族**：C32（活守护进程的字节与树上的不是同一份）、C33（你自己的编辑会推走行号）、t123 §8②（文件在你读的时候被改）—— 本条是它们的镜像：**字节在你读之前就已经旧了**。 | captain（沿用） |

| **C37** | **`github.com` 可以单独不可达，而 `api.github.com` / `codeload.github.com` 正常** —— 此时 HTTPS 推送必失败，但 **SSH 旁路可通且零配置改动**（本轮实测） | 现场（2026-10-04 `00:3x`）：`curl https://github.com` **超时**（12/15/20s 三次都超）· `https://api.github.com` **200 / 2.8s** · `https://codeload.github.com` **301 / 1.6s**；`Resolve-DnsName` 三个主机 = `140.82.121.4 / .5 / .10` ⇒ **同一 /24 上只有 `.4` 不可达** ⇒ **单 IP 的路由/ISP 故障，不是代理**（`ProxyEnable=0`、7890 无监听；`git config http.proxy`/`https.proxy` 皆空）。**`gh` 照常工作，是因为它打 `api.github.com`** —— 这正是「`gh` 绿 ≠ `git push` 能通」的原因。**有效旁路（零配置改动、不碰用户系统设置）**：`ssh -p 443 -o StrictHostKeyChecking=accept-new -T git@ssh.github.com` ⇒ `Hi hfldqwe! You've successfully authenticated`（exit 1 是预期的）；`git push ssh://git@ssh.github.com:443/hfldqwe/ruagent.git main:main` ⇒ **`9059225..ec24b61  main -> main`，exit 0** —— `ssh.github.com` 是**另一个主机名**，**可达**，且用现成的 `id_rsa`。**记账副作用与补法**：显式 URL 推送**不会**更新本地 `origin/main` ⇒ `git update-ref refs/remotes/origin/main <sha>`，并用 **`gh api repos/<o>/<r>/commits/main --jq .sha` 独立核实远端**（走 `api.github.com`，不受本故障影响）。**判据**：`git push` 报 `Failed to connect to github.com port 443` 而 `gh` 正常 ⇒ **先分辨是哪台主机**；**不要去改代理设置**（那是用户的系统设置，而且对「单主机不可达」无效 —— 实测 `ProxyEnable=0` 本来就是直连）。 | captain（沿用） |

| **C38** | **一次提交装多个单元 ⇒「写入集合」这条判据当场失效**（`t120` 的评审给我开的单；owner 写明是 captain） | 现场：我把**五个单元**（t118/t127/t128/t131/t132）装进同一个提交 `ec24b61`（19 个文件 / +1989−42），因为当时四张单几乎同时交付、我想尽快把成果推上 main。**后果是真实的、不是形式**：`t118` 的 acceptance 第 6 条与评审自己的「写入集合」判据都要求用**工作树 diff** 证明「只落在 `crates/mcp/**`」—— **改动一旦提交，这条检查就不可用**；评审只能退而「按各单元自己申报的路径」推断那 39 行 `daemon/src/api.rs` 不是 t118 的（它**明说了这是限制**）。⇒ **规则**：**一个单元一次提交**；若确实要批量推，**在任务单里把判据写成**「该单元申报的路径 == 其提交里的路径子集」（可审计），而**不要**依赖事后无法执行的工作树 diff。**同族**：C33（引用会漂）、t20（绿读数只认证它跑过的字节）—— 都是「**判据必须能在事后被执行**」。**我的处置**：这条我立刻执行（未推送的两提交按单元拆开），而不是辩解「反正 CI 在跑」。 | captain（本轮） |

| **C39** | **指向仓库的 junction 会让 `Remove-Item -Recurse` 删进仓库** —— 隔离副本用 `mklink /J node_modules` 时，删树的动作会**沿链接穿进真实仓库**（本代至少两人用过这种副本：`mem-core` 的 t128 负控、`wiki` 的 t136 两臂） | 现场：`t136` 的临时树里两个 junction（`node_modules` 指向仓库 `panel/node_modules`）⇒ 作者在删树**之前**用 `cmd rmdir` 先删链接、再删树，**删后核对仓库 `node_modules` 仍是 152 条目、`@playwright/test` 仍在**。**为什么这条比它看起来严重**：`Remove-Item -Recurse -Force` 在 PowerShell 上**会跟随 junction/symlink 进入目标**（与「删除链接本身」是两种语义），所以一次「例行清临时目录」可以删掉仓库的依赖树 —— 那是**数据损坏级**的后果，而且**门禁不会立刻发现**（下一次 `npm run build` 才炸，且看起来像「node_modules 坏了」）。⇒ **规则**：凡用 junction/符号链接做的隔离副本，**收尾必须先 `rmdir` 链接、再删树**，并**核对仓库侧条目数**（不是「命令没报错」）。**同族**：C32/F9/§B29/B31 —— 都是「**你以为你在操作 A，其实你在操作 A 指向的 B**」。**候选**：这条够格进 `AGENTS.md`（与「进程不得把窗口放到屏幕上」并列的操作性纪律），但 **AGENTS.md 的改动应由拥有它的那一代来做**，本账先记。 | `t136` 作者（已自行避开并记录） |

| **C40** | **`<native cmd> ... \| Select-Object -First N` 会让 `$LASTEXITCODE` 读到 0，而同一份输出明明在报红** —— 我本轮实测：`cargo fmt --all --check 2>&1 \| Select-Object -First 6` 之后 `$LASTEXITCODE` = **0**，而紧接着的 `cargo fmt --all --check *> $null` 得到 **1**（同一份字节、同一秒）。**机理**：`-First N` 在取够 N 个对象后**终止上游管道**（上游拿到断管而被提前结束），于是「命令自己的退出码」没有被保留下来。 | **后果**：本代**整套纪律都建立在「命令 → 退出码 → 读数」上**（门禁绿/红、负控是否点火、护栏是否 fire），而**这个写法会静默地把红读成绿** —— 而且**看起来完全正常**（输出里还印着 `Diff in …:929`，只有退出码是假的）。**规则**：① **退出码必须在任何管道之前捕获**（`cmd *> $null; $LASTEXITCODE`，或 `cmd; $ec = $LASTEXITCODE` 之后再处理输出）；② 需要「截断输出」时，**先跑两遍**（一遍只取退出码、一遍取输出），或把输出**落盘**再读；③ **判据**：退出码与输出**语义不一致**时（输出在报错而码是 0），**先怀疑量具**，不要调和。**同族**：C24（`Select-String` 默认大小写把 `0 failed` 数成假红）、t123 §8①（只 grep 带引号的键名会漏 serde 字段）、以及本轮我先前那次 `^\s*uses:` 漏掉 `- uses:` —— 都是**量具先错、数据后到**。 | captain（本轮） |

| **C41** | **「载体不同 ⇒ 不可测集会不同」是猜测，不是读数**；而**门的证据源若是一条「可能被截断的流」，门就会在「其实读到了数据」的那天变红** | 现场（`37137554330`，F2 的首次真跑）：作者**与**验收单**都预期**护栏 ② 会在 CI 上红（理由：ubuntu 的光栅器/字体与 Windows 不同 ⇒ 未测集变）⇒ **实测未测集与 pin 逐项相同**（`25 28 31 33 46 47 49 50 54 77`）、`judged = 65`（`45 pass / 10 fail / 10 not_measured`）⇒ **护栏 ②④ 本该绿**；**唯一点火的是护栏 ①**，因为 `audit.log` **在一句话中间被截断**（末行 `… CIE76 的 JND ≈ 2.3 `）⇒ **汇总行从未印出**，而工具**早已写出结构化、完整、且在 CI 上可下载的 `metrics.json`**（24 captures / 65 checks / 全部 verdict）⇒ **红在接线，不在产品**。**真正随载体变的是个别判定**：t122 本机是 `44/11/10` 且 fail 含 **2**，CI 是 `45/10/10` 且 fail **不含 2** ⇒ **要量的是「哪些判定变」，而不是假定「集合变」**。**两条可复用判据**：① **别把「环境不同所以集合会变」当结论** —— 它必须被量出来，而量的对象是**逐条的判定与集合的成员**，不是整体计数；② **门禁的证据源要选「即使上游中途死亡也存在」的那一份**（本例：结构化的 `metrics.json` 优于 stdout；这与「跳过必须点名」「计数口径」是同一族 —— 门的可判定性不该依赖一个**可能不完整**的通道）。**代价**：这一条让 `audit.yml` 在首跑就红，而**数据其实是完整的**。 | captain（本轮） |

| **C42** | **一个 job 是绿的，可能只是因为它【没有那一步】** —— CI 的守卫套件**只在 ubuntu 上跑**；windows 是精简 job（只有 coverage 声明 + 两次 test attempt 的证据） | 现场（run `37139111136`，2026-10-04）：`ignored-test count changed: found 16, this job accounts for 15` **只在 ubuntu 上点火**，而 **windows job ✓ 10m12s**。**我先假设「计数随平台变（windows 上 15）」—— 实测否掉**：windows 的**步骤表里根本没有 `Ignored-instrument manifest` 这一步**（本地读 `ci.yml` 亦确认：`rust-windows` 只有 `Install protoc` · `Coverage declaration` · `Test attempt 1 (+evidence)` · `Test attempt 2 (+evidence)`；而 ubuntu 的 `rust` 才有 Format/Clippy/nested-Result/Test evidence/Ignored instruments/Manifest）。**三条判据**：① 说「**某个 job 是绿的**」之前，先看它**有没有那一步** —— 缺步骤的绿与通过检查的绿在日志里长得一样；② **跨载体比较前先比「步骤表」**，不要先比「结论」（我这次就是先比结论、后查步骤表，白走一步）；③ 这与「**跳过必须点名**」同族：**没跑的检查必须被声明，而不是靠读者从绿色里推断它跑过**。**注**：这是**设计选择**（windows ≈ ubuntu 的 2× CI 分钟），不是缺陷；缺的是一句**覆盖声明** —— 而 `panel` 与 `rust-windows` 两个 job **都已经有** `Coverage declaration (what a green here does NOT mean)` 这一步，说明本仓已经意识到这件事，只是 `rust-windows` 的声明没有把「守卫套件不在这里跑」逐条列出。 | captain（本轮） |

| **C43** | **内嵌双引号会把 acceptance 列表【截断】，而被截断的契约会让作者在完成校验上被反复拒** —— 本代最贵的一条工具链缺陷（它同时伤规划与交付） | 现场：`t134` 的 acceptance 在**派发时**被截断（断在 `… 与 \`EXPECTED_NOT_MEASURED="` 处），而**完成校验要求「每一条已声明验收项都 passed」** ⇒ 作者按原文 3 条提交**被拒两次**，最后按「3 条原文 + 自己推断的第 4/5 条」才通过（它请我核对第 4/5 条文字是否与它写的一致 —— 这正是**契约被截断的代价：作者只能猜**）。**我的同一缺陷三例**：建 `t134`（第一次就断在第三项）、`amend t134`（我用 amend 才补回完整五项）、建 `t142`（同样断）。⇒ **规则**：① acceptance/objective 里**不要用双引号**（用 `「」`/反引号/直接写值），把带引号的值**写进 description**（那里不参与校验）；② **建完单回读一遍存储的契约**（`create`/`amend` 的回执里会回显）；③ 成员报告「我的验收项被截断」时，**captain 用 `amend_task` 补全**（已声明的旧值进 revisions 账），**不要让作者去猜**；④ **不要**因此责怪作者提交不通过 —— 我的量具/工具链截断了它的契约。**同族**：C40（管道截断把红读成绿）· t133/C33（量具先错）—— 都是**同一条教训的不同载体：你以为你在传 A，其实传过去的是 A 的前缀**。 | recall（本轮实测）+ captain（三例） |

| **C44** | **宣告窗口要按「`+` 字节落在哪里」分类，而那个分类【必须能从 diff 证明】，不能只由宣告声称** —— 否则「这是控制窗口的红」与「这是产品回归」对读者不可分 | 现场（`t145`，2026-10-04 01:4x）：graph 的窗口 #1 只在该**测试**里加一行 `eprintln!`（读迟到事件的种类）。**recall 从 HEAD 侧独立读 diff** 得到更强的说法：worktree `3c04e28d…`（114,331 B）vs HEAD `14387c3c…`（114,225 B）= **+106 字节**，且 `-`/`+` **只有** `if witness.try_recv().is_ok()` → `if let Ok(ev) = …` + `#[cfg(test)] eprintln!(…)` ⇒ **产品字节一行未动** ⇒ 「窗口期内任何红只可能来自那一族断言」**从一个经验判断变成了可证命题**。**三条判据**：① 宣告里必须给**`+` 字节所在的路径与 `#[cfg(test)]` 归属**（测试内 ⇒ 红点可界定；产品码内 ⇒ **必须点名每一条被动的非 test 路径与预期红的坐标**，因为红会散到别的坐标）；② 「窗口开着」不能只有作者自报 —— 本次 **recall 与 wiki 两个独立见证者**在 `01:40:26` 前后读到**同一个窗口内哈希** `3C04E28D…`，而「窗口前」的值也有**两条独立来源**（mem-core 的 t137 报告 + recall 对 `git show HEAD:` 直接取哈希），并给了**第二个标识符** git blob sha1 `dba02c1f…`；④ **窗口宣告必须带【两个锚】（recall 的更正，它指出的正是本条的根子错误）**：(a) **HEAD 侧**的 blob/sha256（内容基线）；(b) **窗口前 worktree 的 sha256**（**脏态基线**）。**关闭判据是 (b) 复原**，(a) 只用来证明「被打断的只是工作树、不是提交历史」。**为什么**：本代第一版只写了 (a)，而 **`t145` 的窗口 #1 开在一个【本来就有在途改动】的文件上** ⇒ 「回到 HEAD 值」**从来不可能成立** ⇒ **「恢复失败」与「修复在途」在这条判据下不可区分**（与 C22 同族，换了一维）。**实测代价**：recall 的窗口 #1 复读因此**只能给出弱形式见证** —— 它能确定的三件是「`[t145-diag]` 标记已消失」「diff 两处都在 `mod generating_tests` 内、产品字节零改动」「**HEAD 侧的 `chat.rs` 完全没动**（`14387C3C…` / blob `dba02c1f…`，⇒ 提交历史无窗口残留）」，而「`7EB4D185…` 是否逐字节等于开窗前的脏态」**无法证实**（当时没人记脏态哈希）。⇒ **可操作结论**：**开窗前先记录 worktree 的 sha256 并写进宣告**；复读时以它为锚。

③ **恢复判据要三条并记录 HEAD**：内容哈希回到交付值 **且** `git diff --name-only` 为空 **且 复读时连同 `HEAD` 的 commit id 一起报** —— 前两条**只在 HEAD 未移动时等价**：若窗口期间有人提交，第二条会**平凡为真**而字节可能仍不是交付值（wiki 的更正）。不一致时**不修、不 checkout、不 stash**（那会抹掉证据），只报原始读数 + 时间戳；「内容没回」与「内容回了但 git 面/HEAD 语义变了」**必须分开说**。**顺带更正了 captain 本人的公告**：我按行号（`chat.rs:2044`）指认那条红，而注入行把断言族**推后了一行**（`:2044` 在窗口期是 `tokio::time::sleep`）⇒ **宣告要用断言原文 + 文件 + 测试名，行号只在报告里作为「某份字节上的坐标」出现**（AGENTS.md 的「引概念而非裸坐标」/ C33 / t20 同族）。 | recall + wiki（本轮实测；更正了 captain 的公告） |

| **C45** | **推送前那道守卫本身会挂住 = 无读数**：`bash .github/workflows/scripts/check-workflow-refs.sh` 在 **Git Bash** 下会**挂死**（两次 300 s 超时），因为它的第 479 行 `command -v python3 && python3 -c 'import yaml'` 命中的是 **`C:\...\WindowsApps\python3.exe`（Microsoft Store 应用执行别名，非交互下阻塞）** | 现场（`t147`，2026-10-04）：mem-core 前两次跑该守卫**挂住**，**第三次换到 WSL**（真有 `python3`+PyYAML、stdin 关闭、`timeout 150`）**38 s 得 `exit 0`**（`22 refs, not tracked/missing: 0` + 四份 `parses as YAML (PyYAML)`）。⇒ **同一个数字/退出码在不同解释器解析下是「无读数」还是「exit 0」**。**为什么 captain 这一侧一直没撞到**：我的 `bash` 是 **`C:\WINDOWS\system32\bash.exe` = WSL bash**（PATH 上的真 `python3`），所以我每轮跑它都是几十秒内 exit 0；**mem-core 用的是 Git Bash** ⇒ **同一条命令、两个载体、两种命运**（与 C41 同族：**别假定载体，要量载体**）。**判据**：① 该守卫**挂住时要读成「无读数」，不许读成红、也不许读成绿**（mem-core 的处理正确）；② 跑它之前先确认 `bash` 是哪个（`wsl` 还是 Git Bash）与 `python3 -c 'import yaml'` 是否**真能返回**；③ 同族教训：**「无读数 ≠ 红 ≠ 绿」** —— 本代已四例（C40 管道截断 · C43 契约截断 · `t134` 的截断机制未定案 · 本条），**工具链的默认行为会把这三者混起来**。 | mem-core（本轮实测） |

## D. 纪律账

本代把 22 条纪律写进了 `docs/design/reviews/gen2-integration-contract.md` §6.0（六族：**判据 / 绿红 / 解释 / 量词 / 声称面 / 异步面**）。此处**不复制**，只指向那份契约——**一个事实一个来源**。
