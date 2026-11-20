# 单一入口 · 验收剧本（独立验证用）

基线：`d30931b` + 三路改动（`supervise` / `receipts` / `watchdog`）。
设计：`docs/plans/2026-11-20-primary-supervisor-design.md`。

**这份剧本是给验证者的**：不许引用实现者的读数，每一项都自己取一次。

## A 结构面（静态，五个布尔）

| # | 断言 | 怎么查 |
| --- | --- | --- |
| A1 | `runs` 有 `origin_chat_id` 列，且迁移**可重放** | 找到迁移文件；跑既有的重放测试，**数字必须是新的总数**（原来是 25/25，加了迁移就是 26/26 —— 数字没变说明测试没跟上） |
| A2 | 终态映射**穷举**，不是 `if completed` | 读收尾函数：`Failed` / `Cancelled` / `Interrupted` 是否都回写 task；`WaitingPermission` 是否被当成非终态 |
| A3 | 聚合判据**只有一处** | 全仓搜「同簇还有非终态」这个判断，命中数应为 1（两处 = 两个真相） |
| A4 | `notify` 走 `send_prompt` 且带 `generating` 检查 | 读 `ChatManager::notify`，确认它没有绕过 `generating`；确认「正在飞」时是**排队**不是丢弃 |
| A5 | watchdog 默认关闭且挂 capability | 读 policy 默认值 + capability id；确认不是只靠 policy 一行 |

## B 行为面（端到端，这是一票否决项）

**B1 失败 → 自愈 → 仍失败 → 聊天里真的多一条平台事件**

```
私有 root + 私有端口起 daemon（RUAGENT_PANEL_DIST 指向 repo/panel/dist）
1. 造一个必然失败的 agent（mock-agent 的失败模式，或一个不存在的命令）
2. 造一个 chat（哪怕是 SHELL 级：POST /api/v1/chat 拿到 chat id）
3. POST /api/v1/tasks  → task id
4. POST /api/v1/tasks/{id}/runs  body 带 origin_chat_id = 上面那个 chat id，agent = 失败 agent
5. 轮询 GET /api/v1/runs/{run_id} 到终态
6. 读 chat 的消息列表
```
**必须看到**：① 有一条 run 事件记录了**自愈尝试**（「我试了 N 次」）；② chat 里出现一条**平台事件**（不是用户消息）；③ 那条事件**说得出失败原因**；④ task 状态**不是** `in_progress`。

**反例（必须验）**：同样流程但 **不带** `origin_chat_id` → chat 里**一条都不许多**（面板派的活不唤醒人）。

**B2 中间结果不打扰**

fanout 两个 agent（一个快一个慢）→ 快的先终态 → **chat 里必须还是空的**；慢的也终态 → **这时才出现一条**。
（这条抓的是「每 run 唤醒一次」的实现错误。如果实现里加了计时器而不是靠聚合，这条会红。）

**B3 失败时看板不再骗人**

`GET /api/v1/tasks/{id}` 在 run 失败后，状态**不是** `in_progress`，且**说得出上一次为什么没成**。

## C 读数面（P1）

| # | 断言 |
| --- | --- |
| C1 | `GET /api/v1/runs` 存在；`status` 过滤正确；**未知 status → 400** |
| C2 | 每行的 `agent` 是**名字**不是 registry id |
| C3 | 空结果返回空数组 + 正常信封，面板显示「还没有」而不是 `0` |
| C4 | 面板 `Home` 的两个新块在**真的没有数据**时说的是「还没有」；有数据时数字对得上 `GET /api/v1/runs` |

## D 门禁（每一条都要带读数，不许只说 exit 0）

```
cargo fmt --all --check
scripts/cargo-team.ps1 clippy --workspace --all-targets -DenyWarnings
scripts/cargo-team.ps1 test --workspace     # 要报：ok 行数 / FAILED 行数 / passed 求和
cd panel && npm run build
```
`test` 的三元组是硬要求（`test result: ok` 行数、`FAILED` 行数、`passed` 求和）——只说 exit 0 不算读数。

## E 必须报「没覆盖」的
凡是靠读代码推断、没在活实例上看到的，一律写进「我没能确认」。**推断不是读数。**
