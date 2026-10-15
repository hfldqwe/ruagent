# #45–#55 收口报告（team `ruagent-issues-45-55`，t18）

**结论：11/11 条 issue 认定为「已修」**，0 条「部分」、0 条「未修」；每条都带读数与**归属**，每条都把残余写在原处，而不是塞进脚注。

## 0 怎么读这份报告

* **验收条目**取自团队任务契约里的原文（派单时引用了 issue 的验收清单），本报告按 issue 逐条对照。
* **读数归属**标注在每行：`[t18]` = 本次收口在**冻结字节**上由我实测；`[t13/t14/t15/t16/t17/t19]` = 该独立验证任务当场实测；`[tN impl]` = 实现任务自报的读数。**没有读数的行不许写成「已修」。**
* **仪器**：当前字节编出的私有二进制 + 嵌套丢弃式 root + 私有端口（8903/8904/8901）；**常驻 8787 只做对照，绝不用来验证新功能**（它是 10-02 旧构建，新端点会回落到面板 HTML）。
* 本代**没有任何一条读数**来自常驻 8787；它每次只出现在「旧构建 vs 当前树」的对照里。

## 1 全量门禁（本次收口实测，同一安静窗口）

| 命令 | 退出码 | 读数 `[t18]` |
| --- | --- | --- |
| `NO_PROXY=127.0.0.1,localhost,::1 cargo fmt --all --check` | **0** | 无输出（全树已格式化；此前 177 个站点的欠债已清零） |
| `NO_PROXY=127.0.0.1,localhost,::1 cargo clippy --workspace --all-targets -- -D warnings` | **0** | 无 warning（`-D warnings` 下全绿） |
| `NO_PROXY=127.0.0.1,localhost,::1 cargo test --workspace` | **0** | **75 个 target 全 ok、739 passed / 0 failed / 17 ignored**（63 个非空 target）。含 `e2e_daemon` 24、`roundtrip` 11、`knowledge_api` 16、`graph_ingest` 16、`capabilities` 11、`extraction-gold` 7、`facts-provenance` 4、`resolution` 3、`seed-resolution` 5、`ledger_reopen` 4、`migration_write_lock` 1 |
| `cd panel && npm run build` | **0** | i18n 词典完整性 VERDICT PASS → `tsc -b` + `tsc -p e2e/tsconfig.json --noEmit` → vite `✓ built in 6.20s`，`dist updated (55 assets)` |

**安静窗口证据**：窗口 `23:11:29 → 23:17:58`（6m29s）。跑前跑后各取一次工作树指纹：`git diff` 的 blob 哈希 **两次相同**（`de29f9d93022d3daa322847cd7ada7f167a06acf`），`git status --porcelain` **逐字节相同** ⇒ 窗口内无人编辑、无人重链接；本轮没有出现「doctest 工件缺失」或 spawn 类超时。

**冻结字节哈希（我的读数，不是任何成员引用的快照）**：`api.rs 9e64c227` · `distill.rs af3f816a` · `sessions.rs 5f278faa` · `graph/lib.rs 9fd8df74` · `community.rs 9633b27c` · `mcp/lib.rs d562c6a0` · `mcp/tests/roundtrip.rs 1da558c0` · `store/migrations.rs 63022749` · `cli/src/main.rs e09670a3` · `scripts/ruagent-daemon.ps1 ca6e849a` · `docs/plans/capability-plugins-design.md 6830bcc1` · `skills/ruagent-operator/SKILL.md 076c23fa` · `AGENTS.md ff0c1c4d`。

## 2 逐条对照

| issue | 主题 | 承接任务 | 结论 |
| --- | --- | --- | --- |
| #45 | 蒸馏竞态与重试 | t4（+t13/t14 复核、t23 收口） | 已修 |
| #46 | 蒸馏账本可读 | t6 端点 + t9 CLI/doctor + t8 面板（+t17 复核） | 已修 |
| #47 | 批量补蒸馏与覆盖度 | t7 端点 + t9 CLI + t8 面板（+t15 复核） | 已修 |
| #48 | 长会话分窗/截断 | t5（+t14 复核、t23 收口） | 已修 |
| #49 | 图谱事实的事件时间契约 | t1 + t2 前端（+t13 复核、t21 收口） | 已修 |
| #50 | 待合并队列消费 | t10 + t11 面板（+t15 复核、t24 收敛） | 已修 |
| #51 | MCP 写入/状态闭环 | t12（+t17 复核、t27 补 e2e） | 已修 |
| #52 | 事实来源可读 | t1 后端 + t2 面板（+t13 复核） | 已修 |
| #53 | 视图失败态 | t11 + t16 独立验证 | 已修 |
| #54 | 列表截断契约 | t6 信封 + t22/t25/t26 三处读端 + t12/t27 MCP（+t15/t17 复核） | 已修 |
| #55 | 用量成本落库 | t3 列 + t5 写端 + t20 e2e + t8 面板（+t14/t15 复核） | 已修 |

### #45 蒸馏竞态与重试 —— 已修

* 验收①「不预先建 sessions 行直接蒸馏：先索引或明确返回 empty，不再出现 Query returned no rows」：`[t14]` 索引优先路径 = 删掉 sessions 行但文件仍在 → **200 / 0.01s / retries=0**，行被重建；文件不存在 → **400 该会话尚未进入索引，正在同步 / 1.25s**，账本 `status=failed, retries=2`。
* 验收②「10 秒内开闭的会话：成功或明确记为无内容」：`[t14]` 文件被覆盖成不可解析 → **400 没有可蒸馏的内容 / 0.21s**，账本 `status=empty, retries=0`（`NoContent` 是终态，不重试）。
* 验收③「重试次数如实入账」：`[t14]` 单测读数 `READING t45 retry budget: 2 retries, waits 400ms then 800ms`；失败行 `retries=2`。
* 验收④「用户可见文案不含 SQL 原文」：`[t4]` 两条常量 + 两处账本 reason 断言不含 `SELECT/INSERT/sqlite/Query returned no rows`。
* **残余（已登记）**：真实存储故障（如 `database is locked`）保留其 cause —— 隐藏它会让真实故障不可诊断；入口不再从「缺行」伪造这类文案。assistant-only 会话**不被拒**是**待决产品问题**（t23 已在 `distill.rs:726` 登记 + 用例钉住行为）。

### #46 蒸馏账本可读 —— 已修

* 端点：`GET /api/v1/distill/log?status=&session=&limit=&offset=` 带 `by_status`（四桶恒等于 total）与 `totals`。`[t15]` 四桶相加 = total（empty 4/failed 1/not_recorded 1/ok 1）；`?status=not_recorded` 取回 NULL 状态行；`?status=bogus` → 400；`totals.tokens_used=267712`（只累加已记录）与 `tokens_used_not_recorded=6`/`cost_usd_not_recorded=7` 并存 —— **绝不 COALESCE(...,0)**。
* CLI：`[t17]` `distill --session <key> --extractor rules` exit 0 并打印三态与「not recorded」；失败 attempt → **exit 1**；clap 误用 → **exit 2**；`doctor` 第 6 项可达时打印「last attempt … [failed] — 该会话尚未进入索引 (of 2 recorded); cumulative tokens not recorded (2 of 2 attempt(s))」且 **exit 0**，不可达时 `[FAIL] distillation pipeline 未检查（daemon 不可达）` 且 **exit 1**（对照见 t17 报告）。
* 面板：`[t16]` 账本卡 `[data-last-failure='2']` + 原因原文、四桶=daemon 计数、pre-0028 行显示「未记录×3」而非 0。

### #47 批量补蒸馏与覆盖度 —— 已修

* dry_run 不花 token、不写库：`[t15]` `POST /sessions/distill/batch?limit=1&dry_run=true` → `attempted 0 / actual null / results []`，账本 6→6、memories 5→5、episodes 8→8；预估 `{sessions 1, bytes 8161904, windows 8 = ceil(bytes/1048576), tokens_used 100.0, basis{ok_attempts 1,…}}`；同一会话实跑 `actual.windows = 8` —— **预估与实测对得上**。
* 三态与逐条结果：`[t15]` 实跑 `attempted 1, results[{session_key, status empty, reason null}]`，账本 +1 行；失败不中断（独立用例 `a_batch_counts_the_three_states_and_survives_a_failing_session`）。
* 覆盖度筛选：`[t15]` 会话行带 `distilled/distill_status/distilled_at`（只认 `status='ok'`）；`only=undistilled` → 3 行、`only=distilled` → 0 行、`only=bogus` → 400。
* **残余（已登记）**：规划窗口 `DISTILL_BATCH_SCAN=2000`（活库 688 条时仍是窗口内计数）；**无法被蒸馏的会话永远留在 backlog**（设计如此）；`only=undistilled` 从 t25 起会「加宽补齐本页」而不再是「收窄本页」（见 #54）。

### #48 长会话分窗/截断 —— 已修

* `[t14]` 单窗路径逐字节不变：`coverage.bytes 1090` == 我独立用 Python 重渲染 `render_transcript` 的 **1090** 字节；4.99 MB 会话 → 5 窗、`truncated=false`；单个 1.5 MB 轮次 → 独占一窗、`truncated=true`、`bytes=1050856` vs 预算 1048576。
* 账本如实标注：`windows/truncated` 是测量值，未测量为 NULL；失败行两者皆 NULL。`[t17]` MCP/CLI 读端把「单窗截断丢尾 / 多窗无损 / 均未记录」三种组合分开表述。
* t14-F1（注释声称丢弃 assistant 轮次而实际保留）→ **t23 收口**：注释改成如实描述并登记待决问题；`[t15/t17]` 实测 assistant-only → `status=empty, windows=1, memories_written=0`，**行为未变**。
* **残余（已登记）**：覆盖的消息范围（from/to/total）**没有账本列**（需 0030 迁移），只随 HTTP 响应返回。

### #49 图谱事实的事件时间契约 —— 已修

* 唯一写入路径：`[t13]` 生产代码 `INSERT INTO entity_edges` 仅 `lib.rs:1036`（`insert_fact_in`）；其余 5 处全在 `#[cfg(test)]` 或测试文件内。
* 契约可解释：`[t13]` HTTP 探针按名/按 id 写入 → 202/200 `event_time_source: "recorded"` 且 SQL 列一致（`fact_hash=3293b7ea8ad1451a`）；`extracted` 只由抽取路径在 transcript 给出时间时写入；旧 NULL 行读作 `kind='unknown'`（**不静默升级**）。
* 拒绝面：`[t13]` 每端两种形式 → 400 `give exactly one of `src` and `src_name``；名字无命中 → 400 且点名该字符串。`[t17]` MCP 侧同样是 daemon 原文。
* 别名来源收敛：`[t21 收口/t15 复核]` `merge_entities` 内被引号包住的 `'merge'` 字面量 **0 处**，改走 `AliasSource::Merge`；新测试 `merge_writes_its_alias_source_through_the_alias_source_vocabulary` 在本次门禁中 ok。
* **残余（已登记）**：活库 67 条 pre-0021 边 `event_time_source` 仍为 NULL（历史未知，如实 NULL）。

### #50 待合并队列消费 —— 已修

* 消费面：`[t15]` `GET /graph/resolution/pending` 返回 a/b（id+name+kind）、`a_facts/b_facts`、`reason`、`pending_count/dismissed_count/redundant_count` 与 `counts_note`（口径原文）。
* 裁决持久化：`[t15]` dismiss 首决 `{dismissed true, already_dismissed false}` → 重复/反序 `already_dismissed true` → 从未入队 **404** → `a==b` **400**；之后 `pending_count 2→1`、`dismissed_count 0→1`、该对消失；再按 `queue_pending` 的 upsert 重放，**`dismissed_at` 保住**（0029 设计主张成立）。
* 口径收敛：`[t24/t15]` `one_entity_current_facts` 只有一处定义（`graph/lib.rs:535`），被 `current_facts(320)`、`current_fact_count(552)`、`community.rs:364` 共用；`lib.rs:395` 仅剩的写法是 `facts_as_of`（无 `invalid_at IS NULL`，在 Rust 里按时间判定），不是这份谓词。
* 面板：`[t16]` 待合并入口=实时 `pending_count`、两对两端 kind/summary/双向事实数、counts_note 原文；合并后 2→1 且实体真的从表里消失，保持分离后 pending 0/dismissed 1。
* **别误判**：merge 响应 `alias_written=false` 是**常态**（alias 由 merge 在同一事务内写入，第二次写没有新行），不是缺陷。

### #51 MCP 写入/状态闭环 —— 已修

* `[t17]` 真 stdio 会话（rmcp 3.3.0）：`tools/list` = **21 个工具**（18→21）；`graph_fact_add` 按名写 fact #1 并回带 EVENT TIME CONTRACT 文本，**同一会话** `graph_entity{id:1}` 读回 `1 owns 2: t17 atlas owns the rollback procedure`；两条拒绝（同端两种形式 / 名字无命中）为 daemon 原文。
* 状态闭环：`[t17]` `distill_status` 把两个问题分开答（整段历史的 `by_status` vs `entries[0]` 的最新一次，含失败原因），未记录项打印 `NOT RECORDED`，绝不 0；空账本说明「无行=没有尝试」并指向免费档 `distill_session`。
* 参数面：`[t17]` 5 个被核工具的 `tools/list` 描述同时含 `WHEN:`/`COST:`；每个 inputSchema `additionalProperties=false`；带未知字段调用 → 运行时拒绝（`unknown field bogus_key, expected session_key or limit`）；源码 19 个参数结构体**无一缺** `#[serde(deny_unknown_fields)]`。
* 非目标：`docs/plans/capability-plugins-design.md §14.4` 写明面为 21 + 中文「非目标」一句 + 三条理由；`skills/ruagent-operator/SKILL.md` 与 `crates/mcp/tests/tool_surface.rs`（计数锁 21）同步。

### #52 事实来源可读 —— 已修

* 后端：`[t13]` `Edge` 追加 `source_session{key,title}`（由 `episodes.source_run` 关联）与 `source_kind`（extracted→extraction / recorded→manual / NULL→unknown）；三条读路径（facts / edges / as_of）**逐行一致**；无 `source_run` 的 episode → `extracted/extraction/session=null`；旧 NULL 行 → `ets=None kind='unknown' session=null`。
* 前端：`[t16]` 来源列三条真实数据（`extraction` → 可点跳转的会话链接（带 `data-session-key`，点击真的切页并按来源收窄）；`manual` → 「手工记录 无来源会话」；unknown → 「来源未知」）；**无空白单元格**；手工建边 body 无 `valid_at`、回执 `data-fact-receipt='recorded'`。

### #53 视图失败态 —— 已修

* 五处「失败渲染成空」全部三态化：`[t16]` PHASE A（全部端点 500）五视图**全部出错误态、空态计数 0**，且剔除 `.error-state` 后的页面文本里「空数据」断言一次都没出现；PHASE B **逐视图重试恢复**（邻居 0→3、任务启动器出现、设置错误清零、聊天重试加载真实消息、运行时空闲清零）；B6 重读失败**保留 3 张卡片并标注「数据可能过期」**。
* 基线：`[t16]` 活体探针 **67/67 checks passed**，0 次模型调用；`cd panel && npm run build` exit 0（与本次收口的读数一致）。
* **残余（已登记）**：`[t16]` 走仓库入口的 e2e 为 **57 passed / 3 failed / 2 skipped**（重跑 58/2/2），失败全是 `page.goto` 30s `ERR_ABORTED`，落在本次 diff **未触碰**的 `agents-health-third-state/consumption`；**e2e 抖动未在本轮解决**（#53 的验证用的是桩注入 + 面板构建，未把 e2e 抖动当作已知绿灯）。

### #54 列表截断契约 —— 已修（四处合写）

* **信封**：`[t6/t15]` 仓库内只有一个信封构造器 `fn page()`（`api.rs:477`），**四个调用点**（graph/entities、graph/edges、tasks、distill/log）；`truncated = (offset+本页行数) < total`。`[t15]` 空表 total=0/truncated=false；4 条时 limit=2 → true、limit=4 → false、limit=10 → false。窗口型读端（sessions、memory/list、recall、recall/log）**没有 offset 键** —— 一套信封 + 一套另行声明的窗口形状，**不存在第二套分页**（grep 与端点实测各一）。
* **窗口读端**：`[t15]` memory/list 5 条 limit=2 → `truncated=true, sections=['memories']`、limit=5/10 → false/[]、空库 → 0/0/false；recall 知识腿 top_n=2（恰好）→ false、top_n=1 → **true/['knowledge']**、top_n=20 → false；既有键一个不改。
* **会话列表（t25）**：`truncated + truncated_sections` 追加；`offset` **明确 400 并点名**（含 `offset=0`），响应体无 `sessions` 键；`distill_scope_keeps` 是加宽循环与响应循环共用的**唯一**判据（`api.rs:3280/3397/3414`，`[t18]` 代码核对）。**残余**：加宽上限之外可能读作 false；未发布 `total`。
* **search 两路由（t26）**：`[t18]` 冻结字节含 `SEARCH_PAGE_MAX=200`(1025)、`SEARCH_MATCH_CEILING=5000`(1035)、`match_window`(1064)、`search_cut_keys`(1111)、`matched_exact`（daemon 7 处 / mcp 12 处）；`ROUTE_WINDOW` **已无赋值**（只剩说明 t17-O1 的注释）。`[t26]` 实测边界：触达 limit=2 → `matched 3, matched_exact=true, truncated=true`；恰好相等 limit=3 → `truncated=false`；未触达 → `limit=10, matched=3, truncated=false`；空匹配集 `matched=0`。撞上限时 `matched_exact=false` ⇒ MCP 打印 **AT LEAST** 下界，不冒充精确值。
* **MCP 侧（t12 + t27）**：`[t17]` `list_tasks` → `shown 1 of 3 (limit=1, offset=0); 还有 2 条未显示`；`knowledge_search` → `matched 2, this reply carries 1; 还有 1 条未显示`；`memory_search` 在 t26 前后**都**如实（t17 当时是「余量未知」，t26 之后是真实数字）。`[t27]` 真 stdio 会话 e2e：触达 → `matched 5, this reply carries 2; 还有 3 条未显示`（全文不含 UNKNOWN），恰好相等/未触达 → **一个余量字都不说**；5005 行撞上限 → `matched AT LEAST 5000 … 至少还有 4998 条未显示 … the exact match count is unknown`，且断言下界**不以精确措辞**出现。本次门禁中 `roundtrip` **11 passed**（原 9 + t27 的 2）。

### #55 用量成本落库 —— 已修

* 列：`[t3]` 0028 追加 8 个可空无默认列（含 `tokens_used/context_size/cost_usd/retries/windows/truncated`），0029 追加 `resolution_pending.dismissed_at`；旧库副本 33 行 × 8 列 = **264/264 全 NULL**，行不动；丢 ledger 行后重放幂等（`the_ledger_and_dismissal_columns_upgrade_an_old_database_and_reapply_cleanly`）。`[t14]` 老库 27→29 升级 + 缺列重放不报 `duplicate column name`。
* 写端：`[t5/t20]` ACP 用量折叠（最后一次 `UsageUpdate`，cost 缺失不清空）→ 三列入库；未上报 → **NULL 与 0 区分**。`[t15]` 真实 ACP 行 `tokens_used=267612, context_size=1000000, cost_usd=NULL`（该次是我探针造成的真实调用，已披露）；`totals` 只累加已记录并单列未记录计数。`[t20]` `mock-agent/tests/e2e_daemon.rs:2328` 的 `distill_records_the_agents_usage_and_leaves_it_null_when_unreported` 断言两半（有值 / NULL），本次门禁该 target 24 passed。
* 面板：`[t16]` 账本卡显示 `12.3k / $0.42` 与「未记录×3」、失败行保留「1 次调用 + 2×未记录」。

## 3 残余与裁决台账（按登记顺序，如实落盘）

| # | 条目 | 状态 |
| --- | --- | --- |
| 1 | 覆盖范围（from/to/total）不入账本，需 0030 迁移 | 登记（未做，需独立一轮） |
| 2 | memory/list 与 recall 保持窗口形状（非第二套分页） | 已定（§1 有 grep + 端点证据） |
| 3 | `crates/graph/tests/aliases.rs` 属 t1/t6 越界（3 个调用点编译必需） | 已定 |
| 4 | 修订归属纪律：事后编辑要披露并重跑 | 已定（本轮 AGENTS.md 已收录） |
| 5 | t19 的脚本注释与 AGENTS.md 收尾 | 已完成（`AGENTS.md ff0c1c4d`、`ruagent-daemon.ps1 ca6e849a`） |
| 6 | 真实任务 `ruagent-daemon-watchdog` 仍 `State=Disabled` | 未变（不由团队启用；启用是用户动作） |
| 7 | watchdog.log 因验证用一次性任务自身触发 1292→1299 行，其后 409s 无新增 | 证据非残留 |
| 8 | assistant-only 会话是否应被拒 | 待**用户/产品**裁决（代码登记于 `distill.rs:726`，行为被用例钉住） |
| 9 | 并发构建/重链接的瞬时 RED 与安静窗口纪律 | 已定（本次门禁带前后指纹） |
| 10 | t20 的 e2e 脚手架保真度、t23 的 TRANSCRIPT 夹具保真度 | 各处已如实声明 |
| 11 | t10 的形状变更（数组→对象）与 `dismissed_count` 会随 merge 下降 | 已定（panel/mcp/cli 无旧消费者） |
| 12 | 常驻 8787 是旧构建 | 已定（见 §5） |
| 13 | t22 的 `truncated_sections` 与 recall memories 欠报 | 已声明；**未修**（需 memembed 计数） |
| 14 | t15-O1（会话列表无截断声明） | **已由 t25 修复** |
| 15 | t15 探针产生 1 次真实 dsh 调用（267,612 tokens） | 已披露（#55 行内亦记） |
| 16 | #53 前提更正（视图失败态范围） | 已定 |
| 17 | t11 未跑 e2e → 归 t16 | 已完成 |
| 18 | t7 残余 (b) 被 t25 取代（`only=` 现会加宽补齐） | 已定 |
| 19 | t25 残留：加宽上限之外可能读 false；`offset` 一律 400 | 登记 |
| 20 | t12 越界文件（roundtrip.rs / tool_surface.rs / SKILL.md） | 本轮提交**包含**这三个文件（属本计划） |
| 21 | #54 第三条验收按 t22/t25/t26/t27 四处合写 | 已定（§#54） |
| 22 | merge 响应 `alias_written=false` 是常态 | 已定（非缺陷） |
| 23 | t17 = PASS 且模型调用 0 次；三处覆盖边界不得夸大 | 已定（`graph_search ≥10` 未触发、`memory_recall INCOMPLETE` 未制造） |
| 24 | t17-O1 已由 t26 落地（`ROUTE_WINDOW=10` 已删除） | 已完成 |
| 25 | t26 越界最小改动（docs §14.4 如实化） | 已定 |
| 26 | **新问题（未修，建议另开 issue）**：`POST /api/v1/memory/write` 对不允许的 store×namespace 组合返回 **200 + `outcome=RejectedNamespace`** 而非 4xx | 登记（t26 发现；会波及写路径既有调用方与 MCP 输出形状，需独立一轮） |
| 27 | 修订归属警号（t26/t27 自报哈希与 captain 复测不一致） | 见 §4（**当前字节与成员自报一致，captain 的两组读数是中途快照**） |
| 28 | 常驻 daemon 不由团队重启 | 已定（见 §5） |

## 4 哈希与归属纪律

* 本报告引用的**唯一**哈希来自我在冻结字节上的读数（§1）。成员自报值与它们**一致**：`api.rs 9e64c227`（t26 自报）、`mcp/src/lib.rs d562c6a0`（t26 自报）、`mcp/tests/roundtrip.rs 1da558c0`（t27 自报）。
* captain 复测得到的三组不同值（`e42c7707` @22:42:26、`0c65fe44` @22:47:00、`da89f22a` @22:53:41）**不是冻结树**：它们落在成员最后一笔写入**之前/期间**，属中途快照。因此「成员引用的哈希 ≠ 交付树」这一判断本轮**不成立**；成立的说法是「captain 的两组读数是中途快照，冻结树与成员自报一致」——**任何引用成员哈希的句子都必须连带说明这一点**。
* 功能面已按冻结字节逐点复核：`match_window:1064`、`search_cut_keys:1111`、`search_remainder_note:1984`、`SEARCH_MATCH_CEILING:1035`、`matched_exact` 19 处、`ROUTE_WINDOW` 无赋值、`distill_scope_keeps:3280`、`roundtrip.rs:819/901`、`AT LEAST` 断言。
* **共享树事实（本轮最重要的一条诚实声明）**：工作树里另有**另一支在途工作**的未提交改动 —— `.github/workflows/e2e.yml` 与四个 `docs/design/reviews/gen{2,3,4}-*.md`，其内容引用 `t46/t169/t179/t185/t190/t191` 与 2026-10-04 的时间戳，与本代（`ruagent-issues-45-55`）无关。**本次提交不含这 5 个文件**（`git add` 逐个点名，见提交正文）；把它们扫进来会把别支的工作记到我们账上。
* 本计划 inScope 对照：`git status` 中 37 条路径被任务 `changedPaths` 声明覆盖，另有 3 条属本计划但未在任务里声明（`AGENTS.md` = t19 收尾；`crates/mcp/tests/tool_surface.rs` 与 `skills/ruagent-operator/SKILL.md` = t12 的越界登记）；其余 5 条即上段的外支文件。

## 5 常驻 daemon（维护命令，本轮不由团队执行）

常驻 8787 是 **10-02 旧构建**：`/api/v1/distill/log`、`/sessions/distill/batch` 等新路由在它上面**不可服务**（返回面板 HTML 的 SPA 回退），而 `/api/v1/health` 正常 —— 于是「HTTP 200」在这里**不代表端点存在**。**新端点与新面板功能要换成新构建才在该 daemon 上可见。** 精确步骤与后果：

```powershell
# 1) 新构建无法覆盖运行中的镜像：常驻进程占着 D:\rust_cache\debug\ruagent.exe -> os error 5
cargo build -p ruagent                      # 会在 remove file ... os error 5 处失败，这是预期
# 2) 要重建就得先停；stop 不带 -Force 会拒绝执行并打印该广播文本（脚本不会假装已通知）
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/ruagent-daemon.ps1 stop -Force -Reason "new build"
#    stop 会先往 <root>\logs\daemon-stop-notice.log 追加一条 notice（含时间/pid/原因/广播文本），再停掉记录在案的 pid
cargo build -p ruagent
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/ruagent-daemon.ps1 start
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/ruagent-daemon.ps1 status   # 健康检查 + pid + 日志尾
```

* 团队本轮**没有**执行上述任何一步：8787 是多处对照读数的参照，且它是用户机器上的服务。**停它会写 notice 并中断该 daemon 上的在途会话**，需要用户知情。
* 本轮所有活体验证都跑在私有端口 + 丢弃式 root 上，**未写 `~/.ruagent`，未碰常驻 DB**（验证结束时 `/api/v1/health` 仍 ok）。

## 6 本轮**没有**做的事（覆盖边界，不许夸大）

1. **e2e 未由我重跑**：#53 的面板结论建立在 t16 的桩注入 + DOM 证据与本次 `npm run build` 上；t16 的 e2e 读数（57/3/2 → 58/2/2，失败为未触碰文件的 `page.goto` 超时）**抖动未解决**。
2. **#54 的 sessions/search 两条腿**：t25/t26/t27 的自报读数 + 代码逐点核对 + 它们的用例在我这次门禁中通过；**我没有再起私有 daemon 重跑这三条端点**。
3. `graph_search` 的「≥10 命中触发余量警告」与 `memory_recall` 的 INCOMPLETE 行**未实盘制造**（前者在 t26 后已改为读 daemon 发布值；后者需被截断的 recall 腿）。
4. t19 的 **Windows 10 回退分支**（builder < 22000 或 `conhost` 缺失 → `-WindowStyle Hidden`）在本机不可达（build 10.0.26200）**未验证**：任何「无窗口」结论只对 **Windows 11 22000+ 的 `--headless` 形态**成立。
5. 本报告**不宣称**任何未在上面出现的读数；**没有读数就不写「已修」**。
