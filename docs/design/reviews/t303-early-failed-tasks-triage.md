# t303 清点九个早期面板单的失败终态（2026-09-26）

任务：把 docs/plans/2026-09-26-memory-knowledge-closure.md §8 最后一行（t13 · t20 · t57 · t70 · t100 · t112 · t120 · t125 · t210 「未核对」）变成「知道」。只读 panel/src/ + 运行 e2e；未改 panel/src/ 任何文件；未启停守护进程。

**一句话结论：这九单里 5 条已解决 / 1 条仍成立 / 3 条不可判定。**

| 单 | 判定 | 依据（提交/代码位置/实测读数） |
| --- | --- | --- |
| t13 设计契约评审（外壳与次级视图，needs_revision） | **不可判定** | 它的失败是**评审结论**（needs_revision），不是单一可测缺陷：要判「是否仍成立」= 把 `docs/design/reviews/t13-round1-shell-and-secondary.md` 的逐条 findings 在今天的树上重跑一遍，那是一件评审工作，不是一次读数。**不猜。** |
| t20 X4 收口（Home/Memory ≤520 传 `grid`） | **已解决** | 视图半边在盘上：`panel/src/views/Home.tsx:233 grid`（注释 :229-231 直接引 README §3.4 X4）；t20 自己的 output 记 Memory 侧为 `Memory.tsx:129`（我未逐行复核 Memory）。失败的那条是 captain 写的契约第 2 条（@390 要求 2×2 且 pageH 下降），t20 已论证它在 inScope 内**不可满足** ⇒ 契约缺陷，不是交付缺陷。 |
| t57 行 39 两处重复取数 + 行 27 超预算 | **已解决** | ①②：t57 自己的 output = 已修并实测转 PASS（定位是**轮询节奏**：`Board.tsx:121 setInterval`、`Board.tsx:370 setInterval(load, 3000)`）。③：**我的实测** `node tools/design-audit.mjs --routes=sessions --check` ⇒ **行 27 = PASS**（判据已由 §12.10.2 修订为「应用层首屏 ≤150KB + 两条结构断言」，原 ≤350KB 降为历史记录）⇒ 失败的是旧判据。**未复核**：行 39 的 board 专项读数（切换窗口期间 daemon 不可用，未取到）—— 如实标注。 |
| t70 行 18 回归（#sessions 内容区 39 个 <32px） | **已解决** | **我的实测**（切换前取的）：`node tools/design-audit.mjs --routes=sessions --check` ⇒ **行 18 = PASS**（`<24px 必须为 0；<32px 每路由内容区 ≤10`），该路由**唯一的 FAIL 是行 52**。与 t70 自己的 output 一致（「那条回归在我开工前就已经修好了…唯一失败行是 52」）✓ |
| t100 chat 侧栏轮询不扰动列表 | **已解决（核心）** | 代码位置：`panel/src/views/Chat.tsx:1105 const mergeHistory = (next) =>`、`:1155 mergeHistory(rows)`（t100 的修法：按 id 合并、未变化行返回原对象 ⇒ 零 DOM churn）；t100 自己的 output = 核心修复已实测通过（行序/滚动/焦点稳定）。**未测**：它的验收 ④⑤ 两条（我当时也没测）⇒ 这两条本身**不可判定**，不影响核心判定的依据。 |
| t112 干净 daemon 上 composer 的 model 字段为空 | **不可判定** | 该缺陷**只在「全新 DB + 注册 e2e mock agents」的 daemon 上出现**，而本单不许启停守护进程、活体 8787 不是干净环境 ⇒ 我无法复现它的触发面。可给的定位：路由 `crates/daemon/src/api.rs:38 /api/v1/agents/{name}/options`、handler `:3468`；t112 自己的结论是「daemon 侧 option 目录在干净环境里就是空的」，并在面板侧修掉了「空缓存永久空白」。**要判定必须自起一个干净 root 的 daemon** —— 那是另一单的事。 |
| t120 CI 缺陷⑥ 的真根因（干净环境 ACP option 探测返回空目录） | **不可判定** | 同 t112：触发面是干净环境，本单不可复现。可给的定位：`crates/acp/src/run.rs:263 if !opts.options.is_empty()`（消费侧的守卫）+ 上面的 options handler；t120 自己的 output 明确写「**未定到根因、未交付修复**」⇒ 记录上它就是未修，但我**不能**据「没有后续提交」推断今天仍成立（那会是把「未核对」换成「猜」）。 |
| t125 chat.spec.ts:55 红（`.chat-injection summary` 未找到） | **已解决（断言）/ 不可判定（深层）** | 断言：`panel/e2e/chat.spec.ts:48-58` 现在**先种一条记忆**再跑（注释写明：全新数据根没有记忆 ⇒ chip 不可能出现 ⇒ 原断言测的是**环境**不是面板；种一条后 chip 成为必需，断言保持严格）✓；**我的真跑**（不武装写权限、临时 output 目录）：`node node_modules/@playwright/test/cli.js test --reporter=line` ⇒ **38 passed / 3 skipped / 0 failed**，exit 0 ⇒ 含该断言的用例绿 ✓。深层：t125 暴露的第二个问题（干净环境 `message_count` 恒为 null ⇒ t90 规则把每行隐藏）——规则仍在 `panel/src/views/Chat.tsx:1416 if (!h.message_count) return false;`，但它的触发面同样是干净环境 ⇒ **不可判定**。 |
| t210 可访问性与触控（13 路由 × 390/768/1440） | **仍成立（1 条）** | **我的实测**（切换前）：`--routes=sessions --check` ⇒ 行 13/18/23/27/35 = PASS，**行 52 = FAIL**：`sessions/dark 窄屏可点 75 个 · 最小有效 44×41px（button.icon-btn，元素盒 44×44，命中 81/81）· 有效区低于 44px 的 2 个`。**口径待裁决**：我在 t222 已登记这是「有效区=视口内采样区 44×41、元素盒 44×44」的形态（角部采样点落在圆角外），队长当时收下了这条待裁决 —— 所以「仍成立」指的是**审计行 52 今天仍然 FAIL**，不是「产品一定坏了」。t210 的其余轴（对比度 0/39 违例、焦点环齐备、aria 名齐备）在我 t222 的 39 格扫描里是通过的。 |

## 读法说明（避免把这张表读成「红变绿」）

* 【已解决】= 有**今天的**读数或代码位置支持「失败的那个形状不再存在」；【仍成立】= 有**今天的**读数支持它仍然存在；【不可判定】= 触发面本单够不到（干净环境/需要起 daemon）或它根本不是一次读数能回答的东西（评审结论）。
* **不可判定 ≠ 已解决**：t112/t120 的记录就是「未修」，我只是**不能**在没有干净环境的情况下说它今天怎样。要收口它们，需要一单「临时 root 自起 daemon + 照抄 e2e.yml 的 mock 注册」——那正是 t112/t120 当时用的仪器。
* 本单**没有**把任何记录从 failed 改成 pass（平台记录终态不可变）；它做的是把 §8 的「未核对」换成一格一格有出处的判定。

## 纪律

只读 panel/src/（未改任何文件）· 运行 e2e 未武装写权限（`RUAGENT_E2E_ALLOW_WRITES` 未设 ⇒ registry.spec 自跳过）· 浏览器自建自 close（探针脚本在仓外 C:/tmp/t303）· 未启停守护进程 · 未调 /api/v1/recall · 只 add 本报告 · 未 push。切换窗口期间 daemon 不可用 ⇒ 行 39 的 board 专项读数未取到（如实标注，不猜）。


---

## 补记（切换窗口 2 之后，新 chunk `index-DQtMkr-V.js`）

窗口 2 重建了面板 dist（captain 广播：`assets/index-DQtMkr-V.js` 服务中）⇒ 我上面那些审计行读数是在**旧 chunk** 上取的，按「重做产物证明」的要求在新 chunk 上复测：

| 路由 | 行 | 复测（新 chunk） | 与 t303 正文的关系 |
| --- | --- | --- | --- |
| sessions | 18 | **PASS** | t70【已解决】不变 ✓ |
| sessions | 27 | **PASS** | t57 ③ 不变 ✓ |
| sessions | 52 | **FAIL** | t210【仍成立】不变 ✓ |
| board | **39** | **PASS** | **补齐了正文里「未取到」的那条**：t57 ① 现在有直接读数（不再是只引用它自己的 output）✓ |
| board | 27 | **PASS** | 同上 |

⇒ **t303 的九条判定在新 chunk 上逐条不变**，且正文唯一标「未取到」的行 39 已补上（PASS）。命令：`node tools/design-audit.mjs --routes={sessions,board} --check`。
