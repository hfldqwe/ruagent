# 只读对账：`Delivery` 里 8 条「failed without a follow-up repair」今天是什么（t124）

> **性质**：只读对账。**写入集合 = 本报告**。`crates/**`、`panel/**`、`.github/**`、`scripts/**` 一行未改（`git status --porcelain` 只有 2 个他人（t118）在途文件与本报告）。
> **不跑 rust 构建**（同伴 t118/t62 占用唯一构建名额）；证据取自**读文件、`git log/show`、`gh run list`（只读）与团队任务表**。
> **未触碰** pid 79984 与活库 `~/.ruagent`。
> **日期**：2026-10-04。判定为 `CLOSED-BY-LATER-WORK` 的每条给**两条以上互证**；只给一条的标「单证」。

## 0 结论表

| 单号 | 它今天要的生产事实 | 判定 | 证据条数 |
| --- | --- | --- | --- |
| t45 | INT 补完：MCP 5 工具 / 面板四页读数 / wiki DEP-W + knowledge_api 断言 / 五条跨区读数 / `cli` lint / §1.6 BREAK-* | **STILL-OPEN** | 双证（6 项闭合 + 2 项仍缺） |
| t46 | INT 第 2 轮剩余七项交付 | **CLOSED-BY-LATER-WORK** | 双证 ×7（t51/t50/t20/t47/t91 + 今天字节） |
| t47 | P1 全绿（forget-report / 旧 transcript / G8）+ P2 + P3 | **CLOSED-BY-LATER-WORK** | 双证（t50 §17 + 今天字节 + t51/t20） |
| t48 | 第 4 轮唯一 blocker = G8 带数字 coverage | **CLOSED-BY-LATER-WORK** | 双证（t50 §17 两侧对照 + 今天字节 + t20 独立复核） |
| t57 | V-INT 消费面 6 条 finding + 第 22 条 | **STILL-OPEN** | 双证（F2/F1 有字节坐标；F3/F4/F6/F7 今天字节上仍无） |
| t66 | CI 转绿 + 首次推送 | **CLOSED-BY-LATER-WORK** | 双证（`gh run list` main success + t66 推送读数 + 今天字节） |
| t84 | 面板两处「失败即消失」变可见失败态（P2/P3） | **CLOSED-BY-LATER-WORK** | 三证（t102 四读数 + t103 进 main + 今天字节） |
| t93 | `graph_entity` 区分不存在/零事实 + 协议漂移不静默 + M5 集合断言 | **STILL-OPEN** | 双证（M1 后半 + M5 在 committed 字节；M1 前半在**工作树**、非终态） |

**一句话**：8 条里 **4 条实体已闭合（t46/t47/t48/t66）**、**1 条由后续完成单闭合（t84）**、**2 条仍开（t45/t57）**、**1 条在途（t93，代码在工作树里但非终态）**。⇒「失败单」是**账目状态**（平台按 `repair` 链接算账，见 `gen3-generation-ledger.md:98`：「它们显示为 failed without a follow-up repair，是因为后继单由 captain 直接建立、没有走平台的 repair 链接」），不等于「今天还有缺陷」。

## 1 口径（先说清，否则下面的判定会被读错）

1. **账目状态 ≠ 产品事实**。`Delivery` 的 8 条失败是 4 轮 INT 收敛（t45→t46→t47→t48）+ t57/t66/t84/t93 的账目状态，`gen3-generation-ledger.md:98` 已把它写成「一次收敛过程，不是四次独立失败」。本单把每条的**每一条交付物**分别对到今天的字节/提交上。
2. **in-flight 字节 ≠ committed 字节**（本单实测到一处，见 §9 t93）：`git status --porcelain` 今天有 2 个 t118 在途文件（` M crates/mcp/src/lib.rs`、` M crates/mcp/tests/roundtrip.rs`），而 `git show HEAD:` 的同一函数还是旧注释。凡「已修」结论都写明取自哪份字节（AGENTS.md「绿门禁只证明它跑过的那份字节」）。
3. **只读证据的代价**：`gh` 是本机经系统代理的 curl 系客户端，直接调用会打到 `127.0.0.1:7890`（无监听）；`NO_PROXY='*'` 后成功（本单 §8 t66 的两条读数即由此取得）。本单**未跑任何 rust 构建**。

## 2 t45 —— INT 集成补完（第 1 轮）：**STILL-OPEN**

**① 验收要点**：补完 INT 未交付部分 —— MCP 5 个新工具各给调用读数、面板四页（Memory/Knowledge/Graph/Wiki）每页 ≥1 条 DOM 读数、wiki DEP-W + `crates/daemon/tests/knowledge_api.rs` 逐字段断言、五条跨区读数一次跑完（含 `as_of` 三种写法逐字节一致与 G8 带数字 coverage）、修掉 `cli/src/main.rs:809` 的 lint 使 workspace clippy 全绿、§1.6 BREAK-* 逐条处置。

**② 今天的证据**（逐项，坐标齐）：

| 交付物 | 完成单读数 | 今天字节/提交坐标 |
| --- | --- | --- |
| MCP 5 工具 | **t46**（completed，14 工具 + 4/5 调用读数）· **t91**（`tools/list = 14`） | `crates/mcp/src/lib.rs` 工具数 **18**（committed HEAD 亦 18；t6 wave C 另加 4 个） |
| 面板四页读数 | **t51**（completed，「消费面装配：wiki DEP-W + knowledge_api 断言 + 面板四页读数」） | `panel/src/views/Memory.tsx:159-180`（面板消费 `/recall/log`） |
| wiki DEP-W + knowledge_api 断言 | **t51**（completed） | `crates/daemon/tests/knowledge_api.rs:502/505/512/523`（`unknown` 三态 + 空列表是读数） |
| `as_of` 三种写法一致 | **t27**（completed，检索侧 RVC-1）· **t82**（completed，API 侧 `facts_as_of` 解析时刻） | `crates/graph/src/lib.rs:269`（`facts_as_of`）+ 单测 `facts_as_of_answers_the_same_for_one_instant_written_three_ways` |
| G8 带数字 coverage | **t50**（completed，「G8 已产出」）· **t20**（completed，独立复核） | `docs/design/reviews/gen2-integration-impl.md:427`（§17）、`:473`（构建侧 `cite_coverage=1.0` / 注入侧 `coverage=1.00`）· `gen2-integration-verify.md:199-201` |
| DEP-1 生产帧三方对照 | **t20**（completed，「G8 与 DEP-1 两条历史缺口我独立复现为已闭合」） | `gen2-integration-verify.md:11/66-70`（含负控） |
| 旧 transcript 反序列化 | **t47**（P1② ✅） | `crates/daemon/tests/event_compat.rs:3/23`（旧 `ContextInjected`（无 path/budget）仍可反序列化）+ `crates/core/src/event.rs:133` |
| `cli/src/main.rs:809` lint | **t45 自身**（「lint 门转绿（两项）」） | `cli/src/main.rs:809`（AGENTS.md 记为**有意保留**的 scoped allow） |
| 五条跨区读数 ⑤ `recall_log` 逐列填充 | **t57**（F1 已交付：24 键 / 22 非 null） | `crates/daemon/src/api.rs:3947-4024`（SELECT）+ `:4049-4064`（响应含 `scoring_version`/`fusion`） |
| **五条跨区读数 ④ budget 对象** | **t60**（completed，但自陈「A-2 预算半边与 N-6 = 未达成」） | **t64 仍 pending**（依赖 t59 failed） |
| **§1.6 其余 BREAK-*** | — | `gen2-integration-impl.md:485`：已按 captain 裁决**转 t51 与后续** |

**③ 判定**：**STILL-OPEN**。理由：8 项交付物有 6 项由后续完成单闭合，**仍缺 2 项** —— ① **budget 对象**（t60 只给了渲染侧报告，A-2/N-6 未达成；t64 是那张仍 pending 的单）；② **§1.6 其余 BREAK-***。

**④ 最小下一步（新立单候选已存在，不需要另立）**：**解冻/认领 t64**（inScope：`crates/daemon/src/runs.rs`、`crates/acp/src/chat.rs`；依赖 t59 终态失败 ⇒ 本报告 §11 记录它是**账目冻结**）。动作：把 `BudgetReport` 接进注入事件（保持 `Option`）；**能红的负控**：注入「无输入」与「全被丢」两个世界，断言 `budget` 非 null 且 `dropped_items` 在两个世界可区分 ⇒ 今天必红（t60 的读数正是 A-2/N-6「不可判」）。§1.6 BREAK-* 的残余若无人认领，按「一条单一个动作」另立单（inScope：`crates/daemon/src/api.rs` + `docs/design/reviews/gen2-integration-impl.md`）。

## 3 t46 —— INT 集成补完（第 2 轮）：**CLOSED-BY-LATER-WORK**

**① 验收要点**：完成 INT 剩余七项 —— MCP 5 工具 / 面板四页 DOM 读数 / wiki DEP-W + `knowledge_api.rs` 断言 / G8 带数字 coverage（脚本化 mock agent）/ DEP-1 生产帧三方对照 / 旧 transcript 反序列化读数 / §1.6 其余 BREAK-*；每条给读数或显式 not_measured + owner。

**② 今天的证据**（七项逐条，全部有 ≥2 条互证）：

| 交付物 | 完成单读数 | 今天字节/提交坐标 |
| --- | --- | --- |
| MCP 5 工具 | **t46 自身**（14 工具）· **t91**（`tools/list = 14`） | 今天 **18** 个工具：`crates/mcp/src/lib.rs:58…464`（`memory_search` … `list_tasks`） |
| 面板四页 DOM 读数 | **t51**（completed） | `panel/src/views/Memory.tsx`（页存在、消费 `/recall/log`，见 §5） |
| wiki DEP-W + knowledge_api 断言 | **t51**（completed） | `crates/daemon/tests/knowledge_api.rs:502-523` |
| G8 带数字 coverage | **t50**（completed） | `gen2-integration-impl.md:427/473` |
| DEP-1 生产帧三方对照 | **t20**（completed） | `gen2-integration-verify.md:11/66-70` |
| 旧 transcript 反序列化 | **t47**（P1② ✅） | `crates/daemon/tests/event_compat.rs:3/23/25` |
| §1.6 其余 BREAK-* | **t51 与后续**（captain 裁决转移） | `gen2-integration-impl.md:485`（逐字：「已按 captain 裁决转 t51 与后续」） |

**③ 判定**：**CLOSED-BY-LATER-WORK**。七项各有 ≥1 个完成单读数 + 今天字节坐标；其中 MCP 五项、G8、DEP-1、旧 transcript 为**双证**，面板四页、wiki DEP-W / knowledge_api 断言为 t51 + 今天字节**双证**。**判据未降到 t46 之外**（它的 acceptance 正是这七项）。

**④ 不适用**（无 STILL-OPEN 项）。

## 4 t47 —— INT 第三轮收口：**CLOSED-BY-LATER-WORK**

**① 验收要点**：P1 必须全绿（补 `/api/v1/forget-report` 路由使 MCP 工具真可用 · 旧 transcript 反序列化用例 · G8 带数字 coverage 的脚本化 mock）；P2 尽量完成（面板四页 DOM + wiki DEP-W + `knowledge_api` 断言）；P3 如实记录（DEP-1 23 帧对照 + §1.6 其余 BREAK-*）。

**② 今天的证据**：
- **P1①**（forget-report 路由）：今天字节 `crates/daemon/src/api.rs:201`（`// INT46-1 (t47): the route the MCP tool memory_forget_report proxies`）+ `:206`（`.route("/api/v1/forget-report", get(forget_report))`）+ `:3892-3921`（handler）。
- **P1②**（旧 transcript）：今天字节 `crates/daemon/tests/event_compat.rs:23`（`an old ContextInjected event (no path, no budget) must still deserialize`）+ `crates/core/src/event.rs:133`。
- **P1③**（G8）：**t50**（completed，§17「G8 已产出 —— 带数字 coverage 的两侧对照拿到」）+ 今天字节 `gen2-integration-impl.md:427/473`。
- **P2**：**t51**（completed，面板四页 + wiki DEP-W + knowledge_api 断言）；今天字节 `knowledge_api.rs:502-523`、`panel/src/views/Memory.tsx:159-180`。
- **P3**：**t20**（completed，DEP-1 独立复核）+ `gen2-integration-verify.md:11`。

**③ 判定**：**CLOSED-BY-LATER-WORK**。理由：P1③ 由 t50 闭合（**双证**：t50 的两侧对照读数 + 今天字节 `gen2-integration-impl.md:427/473`；另有 t20 的独立复核 `gen2-integration-verify.md:11`）；P1①/P1② 由 t47 自身交付并有今天字节坐标；P2/P3 由 t51/t20 闭合。t47 被判 failed 的原因是「P1 必须全绿」而 P1③ 当时未产出 —— 这个前提已被 t50 推翻。

**④ 不适用**。

## 5 t48 —— INT 第四轮（P1 = G8 唯一 blocker）：**CLOSED-BY-LATER-WORK**

**① 验收要点**：P1（**本轮唯一 blocker**）= 用 `--behavior scripted --replies replies.json`（`ScriptedReply` 按 marker 命中）产出一张过引用门且有记录值的 wiki 页，使注入块里出现**带数字的 coverage**；P2 = 面板四页 DOM + wiki DEP-W + `knowledge_api` 断言；P3 = DEP-1 23 帧对照 + §1.6 其余 BREAK-*。

**② 今天的证据**：
- **P1（G8）**：**t50**（completed，「**G8 已产出 —— 带数字 coverage 的两侧对照拿到，本代最后一个构造性达标风险点关闭**」）+ 今天字节 `gen2-integration-impl.md:427`（§17 标题）、`:473`（**构建侧 `cite_coverage=1.0` 与注入侧 `coverage=1.00` 两侧对上**）；**t20**（completed）的独立复核 `gen2-integration-verify.md:199-201`（构建侧记录值 / 注入侧 `coverage=1.00` / 负控 `#0 ⇒ failed + citation check failed`）**真假两侧都测到了**。
- **P2**：**t51**（completed）；今天字节 `knowledge_api.rs:502-523`、`panel/src/views/Memory.tsx`。
- **P3**：**t20**（completed）；`gen2-integration-verify.md:11/66-70`。

**③ 判定**：**CLOSED-BY-LATER-WORK**。理由：t48 的**唯一 blocker** 就是 P1=G8，而 G8 由 t50 产出并经 t20 独立复核为「已闭合」；t48 自己的 output 也写着「入口从『只知道大概』推进到『只差锚点识别』」，t50 正是那一步。

**④ 不适用**。

## 6 t57 —— V-INT 消费面 findings 收口：**STILL-OPEN**

**① 验收要点**：F1 `/recall/log` 透出新列 · F2 `/graph/entity/{id}/facts?at=` 归一化 + 非时刻 400 · F3 `prompt_hash` 接线 · F4 `/stats` 补 `embedder`/`scoring_version` · F6 纠错端点对未知 slug 明确拒绝且不落盘 · F7 契约键名更正；并新增第 22 条（跨 target 的 `passed` 求和不是稳定读数）。

**② 今天的证据**：
- **F2 已交付**：今天字节 `crates/daemon/src/api.rs:1288-1295`（`parse_ts` → 非时刻 400 → `to_rfc3339()`）；并由 **t82**（completed）补上底层 `crates/graph/src/lib.rs:269` `facts_as_of` 本身的时刻解析（同族：`retrieve.rs:737-751` RVC-1 只在检索侧修过）。
- **F1 已交付**：t57 自身读数（`/recall/log` 行内 24 键、22 非 null、`null_keys=source`）；今天字节 `crates/daemon/src/api.rs:3947-4024`（SELECT）+ `:4049-4064`（响应含 `scoring_version`/`fusion`/`source_filter`）。
- **第 22 条已交付**：t57 自身读数（t50 读 386 / V-INT 387 / t55 390 的口径漂移）。
- **F3 未交付**：今天全树 `prompt_hash` 只命中 `crates/daemon/src/distill.rs:283/293/316/599/604/609/616/627/1723/1767` 与 `crates/store/src/migrations.rs:585/633`；政策端点（`api.rs:2947` GET / `:2979` PUT）**一处都没有**。
- **F4 未交付**：今天字节 `crates/daemon/src/api.rs:480-483` 的 `/api/v1/stats` 只回 `{"agents": …}`，无 `embedder`/`scoring_version`；`embedder` 只在 `/api/v1/knowledge/documents`（`:648`）—— 与 t57 的原始读数（F-8）逐字一致。
- **F6 未确认**：今天字节 `crates/daemon/src/api.rs:134-138`（corrections 读/写端点）、`:1207-1234`（空列表是读数、`None`/空被拒），但 `crates/daemon/tests/knowledge_api.rs` 里**没有**未知 slug 的拒绝断言（grep `correction|unknown|404` 只命中 `unknown` 三态与 `links_out`）⇒ 拒绝行为未被测试压住。
- **F7 未交付**：今天字节 `crates/daemon/src/api.rs:4050-4060` 逐字写着「**t83 HANDED BACK (kept as `log`)**: captain adjudicated that this key moves to the contract's `rows`」，`:4061` 仍是 `"log": rows`；面板消费方 `panel/src/views/Memory.tsx:163` 仍读 `page.log`。

**③ 判定**：**STILL-OPEN**（F2/F1/第 22 条有今天字节坐标，属本单已交付；**F3/F4/F6/F7 今天字节上仍缺**，且 t62 [claimed]、t96 [pending]、t83 [failed] 均非终态）。

**④ 最小下一步**：**解冻并认领 t96**（inScope：`crates/daemon/src/api.rs`、`crates/daemon/tests/knowledge_api.rs`、`panel/src/`；依赖 t66/t84 —— 本单已判二者 CLOSED-BY-LATER-WORK）。动作：把 `log`→`rows` 四处同改一次闭合（含面板 api 客户端的响应类型）+ F3/F4 版本读点；**能红的负控**：**只改服务端不改面板** ⇒ `cd panel && npm run build` 必须**红**（t83 的实测：`Awaited<ReturnType<typeof api.recallLog>>["rows"]` 无类型 ⇒ TS7006 at `Memory.tsx:344`；把状态写成 `MemoryRow[]` ⇒ 3 处 TS2339 `top_knowledge_score`/`ts`），改回一致 ⇒ 绿。**F6 的未知 slug 拒绝断言**是本单另立的候选（inScope：`crates/daemon/tests/knowledge_api.rs`，能红的负控 = 未知 slug 必须 4xx 且 `wiki_pages` 行数不变）。

## 7 t66 —— CI 转绿 + 首次推送：**CLOSED-BY-LATER-WORK**（双证）

**① 验收要点**：使 `cargo fmt --all --check` 干净、修掉 `e2e_daemon.rs` 过期期望、确保提交树里 CI 引用的一切都在 `git ls-files` 内（含未跟踪的 `test-evidence.sh`）、按语义分组提交并推送 origin/main，然后跟 CI 直到 main 逐 job 绿。

**② 今天的证据**：
- **推送已落地**：提交 **`0da0cb6`**（t66 自身 output：`5db881d..0da0cb6 main -> main`、exit 0；`HEAD == origin/main == 0da0cb6`、`unpushed = 0`；**未推 tag**）。证一。
- **main 上 CI 今天绿**：`gh run list --branch main --limit 6`（只读；`NO_PROXY='*'`）→ 最新 run **`37024461223` / headSha `90592251b439eb8169aacc116feed0c03ba1605d` / `conclusion: success`**（同日同 sha 另有 `37024461018` success；`684b5d8`、`e4bd0a4` 亦有 success）。证二。
- 旁证：今天字节 `.github/workflows/ci.yml` `timeout-minutes` **3 处**、固定 SHA 的 `uses:` **12 处**、浮动 `@v` **0 处**（titer t111/t112/t114 的交付面；`e2e.yml` 同形 0 处浮动）。
- 后续完成单：t104（判定步假红修复）、t105/t106（守卫）、t107/t109/t110（e2e 默认安全 + 跳过必须点名）、t111/t114（超时上界）、t112（动作固定 SHA）、t115（表达式上下文），全部 completed。

**③ 判定**：**CLOSED-BY-LATER-WORK**（它自己的交付「CI 转绿 + 首次推送」今天实测成立：推送读数 + main 的 success run；它被判 failed 是平台规则（契约 verify 有失败项即 failed），不是今天缺行为）。**双证**已给（§7②）。

**④ 不适用**。备注：本单只取到 **main 最近 6 次 run**，其中最新为 2026-10-02；两个 2026-10-04 的 docs 提交（`873bb90`、`674c29f`）在该列表里**没有 run**（如实记，不推断）。

## 8 t84 —— 面板失败可见性（P2/P3）：**CLOSED-BY-LATER-WORK**（三证）

**① 验收要点**：P2 = `panel/src/views/Memory.tsx:163-167` 的三段 catch 不得把失败清空成「没有记录」（配合 `:309` 的 `length > 0`），判据两条（500/网络失败 ⇒ 可见错误态且不出现「无记录」文案；200 空数组 ⇒ 无错误文案）；P3 = `panel/src/views/Runtimes.tsx:36` 的 `.catch(() => {})` + `:34` 的 `?? 0` 不得把探针失败显示成「0 models」；**正常路径显示逐字不变**。

**② 今天的证据**：
- **t102**（completed，独立验证）output：「**t84 三条主张观察到成立** + 交回 2 条 finding + 1 条我自己的越界自陈」；其四条 DOM 读数（`page.route` 注入 500，`4 passed/exit 0`）第一条即「`DOM READING P2 error zone: 最近召回召回失败召回失败`」。证一。
- **t103**（completed）output：F1 汇总第三态 + F2 探针闩锁 + F2c 可点 chip 三处修好，**并有真 e2e 读数与能红的负控**，已进 main。证二。
- **今天字节（对当前字节回读）**：`panel/src/views/Memory.tsx:94`（`const [logError, setLogError] = useState(false)`）、`:171-175`（`.catch(() => {` + 注释「**t84 (P2): NOT `setRecallLog([])`**」+ `setRecallLog(null)`）、`:321`（`{logError && (` 可见错误态）、`:329`（`recallLog && recallLog.length > 0`）；`panel/src/views/Runtimes.tsx:29`（`probeFailed` 状态）、`:216-218`（`runtimes.probeFailed` 文案）、`:300/312`。证三。

**③ 判定**：**CLOSED-BY-LATER-WORK**（三证：t102 的四个 DOM 读数 + t103 的修复已进 main + 今天字节上的三处代码坐标；且正常路径渲染代码 `:329` 的 `length > 0` 与 `:220` 的过滤逐字未变）。

**④ 不适用**。

## 9 t93 —— MCP 存在性区分 + 协议漂移 + roundtrip 集合断言：**STILL-OPEN**

**① 验收要点**：M1 —— `graph_entity` 对「实体不存在」必须与「存在但零事实」在协议层可区分（`isError:true` 或结构化字段，择一并给理由）；M1 后半 —— `crates/mcp/src/lib.rs` 的 `resp["facts"].as_array().cloned().unwrap_or_default()` 让协议字段漂移被静默吸收（改名 ⇒ 每个实体说同一句话）；M5 —— `roundtrip` 工具名断言从 `any()` 改成集合相等 + 数量断言（删任一工具必红）。

**② 今天的证据**：
- **M1 后半已交付（committed 字节）**：`crates/mcp/src/lib.rs:1173-1175`（注释「`unwrap_or_default()` here is what made a renamed field report 'has no current facts'」+ `pub fn facts_of(resp) -> Result<Vec<…>, String>`）+ `:1642`（`fn t93_facts_of_rejects_a_renamed_or_mistyped_field()`、`:1655` 断言改名必须 Err、`:1660` 类型错必须 Err）；t93 自身 output 亦记「M1 后半 + M5 已交付并各带有效负控」。证一。
- **M5 已交付（committed 字节）**：`crates/mcp/tests/roundtrip.rs:136-175`（`expected` 向量 + `assert_eq!(got, expected)`、`:168` 断言数量）；**数量已被后续工作改写 14 → 18**（`:138` 注释「used to be `any()` over 9 of the 14 names」、`:142`「t6 (wave C) added the capability plane's four tools」）⇒ 这是**前提被后续裁决更新**，不是缺陷。证二。
- **M1 前半仍未终态（今天在工作树里，不在 HEAD）**：今天 `git status --porcelain` = ` M crates/mcp/src/lib.rs`、` M crates/mcp/tests/roundtrip.rs`（t118 **[claimed] attempt 2**）；其新增测试自述「**t118 (t93's M1, first half), direction 1 of 2**」，把旧的 `resp["entity"]` / `resp["exists"]`（daemon 从未发出的两个键）换成 `entity_exists(&resp)`；**daemon 侧的存在信号今天已存在**：`crates/daemon/src/api.rs:1328`（`graph_entity`）、`:1342`（`"name": entity.as_ref().map(|e| e.name.clone())`，缺失 id ⇒ `name: null`）、`:1351`（「404 for an id that is not there」）。t119/t120/t121 仍 pending。证三（负证：只证「在工作树里」，不证「已终态」）。

**③ 判定**：**STILL-OPEN**。理由：M1 后半与 M5 已在 committed 字节上交付；**M1 前半的修复只在工作树里、非终态**（t118 direction 1 of 2，t119 验证未开始），按 AGENTS.md 的「绿读数只属于它跑过的那份字节」，**不许把 in-flight 字节当交付**。

**④ 最小下一步**：**推动 t118 到终态**（inScope：`crates/mcp/src/lib.rs`、`crates/mcp/tests/roundtrip.rs`）。动作：把「不存在 ⇒ `isError:true` 或结构化字段」在两侧注释与读出同源下落定，做完 direction 2 of 2（daemon 的 `name:null` 已可作为存在信号，`api.rs:1339-1346/6673/6746` 是它的钉）。**能红的负控**：把 `entity_exists` 的判据反向注入（`name` 存在而非 null）⇒ 新增测试必须**红**；同时给两条不同读数 —— 「不存在的 id ⇒ 拒绝」「零事实的 id ⇒ 文本」。**不要拿工作树字节当终态读数**。

## 10 仍 pending 的九条（按**依赖列表**标一行，不猜）

| 单号 | 依据（团队任务表） | 一行状态 |
| --- | --- | --- |
| t61 | deps t60✓ t62(claimed) t63✓ | **可认领**（无终态失败前置；需等 t62 终态） |
| t62 | claimed（版本读点 F3/F4 独立复核） | **在办**（他人在途，非本单） |
| t64 | deps t59(failed) | **被终态失败前置冻住（t59）** |
| t77 | deps t66(failed) | **被账目冻结（t66）**——本单 §7 判 t66 实体已闭合，建议解冻 |
| t86 | deps t65(failed) t66(failed) t77 t85✓ | **被终态失败前置冻住（t65、t66）** |
| t89 | deps t64 t83(failed) | **被终态失败前置冻住（t83）**（t64 传递自 t59） |
| t90 | deps t88✓ t89 t64 t83(failed) t87✓ | **被终态失败前置冻住（t83）**（t64/t89 传递自 t59） |
| t96 | deps t66(failed) t84(failed) | **被终态失败前置冻住（t66、t84）**——本单判二者实体闭合，**建议解冻** |
| t99 | deps t66(failed) | **被终态失败前置冻住（t66）** |

## 11 本单的口径发现（只读，未改任何源码）

1. **账目状态 ≠ 产品事实**（§0）：`gen3-generation-ledger.md:98` 已写「它们显示为 failed without a follow-up repair，是因为后继单由 captain 直接建立、没有走平台的 repair 链接」，本单把 8 条逐条对到产品事实上：**4 条 CLOSED-BY-LATER-WORK、1 条由后续完成单闭合、2 条 STILL-OPEN、1 条在途（in-flight）**。
2. **in-flight 字节 ≠ committed 字节**（§9）：t93 的 M1 前半修复今天在**工作树**里，`git show HEAD:` 的同一函数仍是旧注释 ⇒ 任何「已修」结论都必须写明取自哪份字节（AGENTS.md 同纪律已载）。
3. **只读取证的代价**：`gh` 直接调用会打到系统代理 `127.0.0.1:7890`（无监听）；`NO_PROXY='*'` 后成功。本单**未跑任何 rust 构建**（把唯一构建名额让给 t118/t62）。
4. **单证项如实标注**：t45 的「面板四页读数」以 t51 的完成单 + 今天字节（`panel/src/views/Memory.tsx` 存在且消费 `/recall/log`）为证 —— 但 t51 的 output 文本没有落在本单可核的读数字符串上 ⇒ 记 **单证**（t51 单号 + 今天字节路径）。其余全部为 ≥2 条互证。
