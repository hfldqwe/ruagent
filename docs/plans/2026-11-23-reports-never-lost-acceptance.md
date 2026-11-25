# 汇报永不丢 · 验收剧本（独立验证用）

基线：`d563b03` + 两路改动（`outbox` / `heal-origin`）。
设计：`docs/plans/2026-11-23-reports-never-lost.md`。

**给验证者的**：不许引用实现者的读数。每一项自己取。

## A 结构面

| # | 断言 | 怎么查 |
| --- | --- | --- |
| A1 | outbox 表存在、迁移可重放 | 迁移 0033；`SCHEMA_VERSION` 与 `MIGRATIONS.len()` 都变成新数（32 → 33）；重放测试绿 |
| A2 | `text` **不带**平台标记 | 读库里一行：正文里不应出现 `USER_TEXT_SENTINEL`；标记只在投递时加 |
| A3 | `delivered_at` 是唯一已投递判据 | 全仓搜「已投递」的判断，命中应只有一处（`attempts` 不得参与该判定） |
| A4 | 补发只有一个发起处 | 搜「读 pending 并投递」，命中数应为 1（两处 = 两个真相） |
| A5 | 自愈世代按 `params.dispatch_epoch`，且旧行走保守分支 | 读 `runs.rs` 的判定；确认 `None` 时不给额外预算 |

## B 行为面（一票否决）

**B1 落盘后补发（P4 主判据）**
```
私有 root + 私有端口起 daemon（绝不碰 8787 上的常驻实例）
1. 造一个 chat 行（chats 表里有行），但**不建立 live session**
2. 触发一次 notify（例如派一个必然失败的活、带 origin=该 chat）
3. 读 outbox：那一条必须在，delivered_at 为 NULL
4. POST /api/v1/chat/{id}/resume
5. 读会话消息 + 再读 outbox
```
**必须看到**：① resume 后会话里出现**一条**平台事件（`user_text` 为空，不是用户消息）；② 该行 `delivered_at` 非空、`attempts >= 1`；③ **只补了一条**（如果堆了多条，其余仍 pending）。

**B2 上限**：塞 N+3 条 → 只剩 N 条，日志点名丢了哪几条。

**B3 删除**：删 chat → 它的 outbox 行一起没了（不留孤儿）。

**B4 重启不再"花钱买不到话"（P4.5）** —— 这条最重要，要真的重启一次 daemon：
```
1. 建 chat（有 live session）→ 派一个长活、带 origin → 杀 daemon（按 PID）
2. 重启 daemon → 观察 boot 清扫是否自愈（应当有：多一个 run，self_heal_of 指向被打断的）
3. POST /chat/{id}/resume → 那条汇报**必须真的出现**
```
判据：**既花了那一轮，也收到了那句话**。若自愈发生但汇报没出现 → 这条不达标（那就退回「boot 不自愈」）。

**B5 自愈世代（P5）**：同一 task 派发两次、两次都失败 → **两次各一次自愈**；同一世代内**不出现第二次**自愈。
**B6 origin 自动注入（P6）**：带 origin 的 run 的 agent 进程环境里有 `RUAGENT_CHAT_ID`；无 origin 的 run 里**该变量不存在**（不是空串）；`task_dispatch` 不带显式参数时新 run 的 origin 等于它，显式参数优先。

## C 读数面

| # | 断言 |
| --- | --- |
| C1 | `GET /api/v1/chats` 每行带 `pending_reports`，且与 outbox 实际未投递数一致 |
| C2 | `GET /api/v1/chat/{id}/reports?pending=1` 能查到未投递原文 |
| C3 | 面板会话列表的未读数三态：读不到 → 不说、0 → 不显示、>0 → 数字（照 `modelChipState` 那条原则） |

## D 门禁（带读数，不许只说 exit 0）
```
cargo fmt --all --check
scripts/cargo-team.ps1 clippy --workspace --all-targets -DenyWarnings
scripts/cargo-team.ps1 test --workspace     # 三元组：ok 行数 / FAILED 行数 / passed 求和
cd panel && npm run build
```

## E 必须报「没覆盖」
读代码推断 ≠ 读数。凡是没在活实例上看到的，写进「我没能确认」。

## F 一条额外的判定（问你自己）
补发机制会不会**在用户不在场的时候**反复花钱？例如：某个 chat 长期没有 live session，pending 一直堆着，而某条路径（定时器、boot、某个 drain）不断尝试投递。**如果有任何一条路径会自己触发投递而不需要人打开会话，请单独报出来**，说明它多久触发一次、每次花多少。
