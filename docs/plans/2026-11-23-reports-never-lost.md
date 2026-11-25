# 汇报永不丢：平台事件 outbox + 自愈世代 + origin 自动注入

日期：2026-11-23 · 基线：`d563b03`（v0.1.10）· 前作：`2026-11-20-primary-supervisor-design.md`

## 0 为什么是这三件

v0.1.10 让「主 agent 派活 → 干完汇报」成立，但独立验证挖出三个洞，按严重性排序：

1. **汇报会丢**（最严重）。`ChatManager::notify` 在 chat 不在内存时直接丢弃（`chat.rs` 的 `platform event dropped: the chat exists in history but has no live session`）。这包括两种情况：
   - daemon 重启后：boot 清扫触发的汇报必然丢（上一轮为此关掉了 boot 自愈，代价是重启后不会自动恢复被打断的活）；
   - 用户关掉/闲置回收了会话：那条汇报永远消失，而任务确实做完了。
   用户的原话是「如果是最终结果，还是需要汇报的」——丢掉就不是汇报。
2. **同一 task 一辈子只自愈一次**（`runs.rs` 的 `healed_already` 按簇数「有没有 `self_heal_of`」）。你第二次、第三次重派同一个 task 再失败时，不会再有自动重试——而「重派」恰恰说明你想要它成。
3. **MCP 侧 origin 要 agent 自己填**（`task_dispatch` 的 `origin_chat_id` 是调用方给的）。agent 不知道自己在哪个会话，所以实际上没人填 → 派出去的活没有归属 → 不汇报。这是「主 agent 会汇报」在真实使用里的**最常见失效点**。

## 1 P4：平台事件 outbox（汇报落盘，恢复时补发）

### P4.1 表（一条迁移 0033）

```sql
CREATE TABLE IF NOT EXISTS chat_reports (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  chat_id      TEXT NOT NULL,
  run_id       TEXT,              -- 哪次 run 促成的汇报（可空：不是所有汇报都来自 run）
  text         TEXT NOT NULL,     -- 正文， WITHOUT the platform marker
  created_at   TEXT NOT NULL,     -- RFC3339, 仓库既有格式
  delivered_at TEXT,              -- NULL = 还没送出去
  attempts     INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_chat_reports_pending ON chat_reports(chat_id, delivered_at);
```

- `text` **不带** `USER_TEXT_SENTINEL`：标记在投递那一刻才加（`platform_event()` 是既有函数，别把标记固化进库里，否则库里的东西就不是"可读正文"）。
- `delivered_at` 是**唯一**的已投递判据。`attempts` 只用于诊断（投递失败几次），**不**用作重试上限的判据（上限由 P4.4 定）。

### P4.2 `notify` 改为「先落盘、再投递」

顺序不能反：**先写库，再尝试发送**。写库失败 → 返回 `Err`（这一次确实没记下来，调用方 WARN）；写库成功但投递失败 → 留 pending、返回 `Ok`，日志说「已落盘，等会话恢复时补发」。

三条分支（判据仍是 `generating`，只在 `deliver_or_queue` 一处）：
- chat 在内存 + 空闲 → 落盘 → `send_prompt` → 标 `delivered_at`
- chat 在内存 + generating → 落盘 → 进内存队列（既有机制）→ 轮次结束后 drain → 标 `delivered_at`
- chat 不在内存 → **落盘后直接返回**（不丢、不重试、不再 WARN 成"dropped"；日志改成「已落盘，等该会话恢复时补发」）

**内存队列与 outbox 的关系**（必须写清，否则就是两个真相）：内存队列是「本轮结束就发」的**加速通道**，outbox 是**真相**。drain 成功 = 标 `delivered_at`。若 drain 时进程死了，事件仍在 outbox 里，下次恢复时补发——这正是要修的那件事。

### P4.3 补发时机（只在一处发起）

**`ChatManager::resume_chat` 成功建立 session 之后**：读该 chat 所有 `delivered_at IS NULL` 的行，按 `id` 升序逐条投递，成功一条标一条。

- 补发时**仍然要走 `deliver_or_queue`**（一个判据），所以「一轮只能有一条在飞」的约束不被绕过。
- 若补发的第一条就失败了 → 停止本 chat 的补发（后面的留到下次），并 WARN 带 chat_id 与剩余条数。**不要**在一个 turn 里连发 N 条（那会把主 agent 的话抢成一串独白）。
- **删除 chat 时**：`DELETE /api/v1/chats/{id}` 要一并删掉该 chat 的 outbox 行（否则永久堆积）。这是「删除」的既有语义（`DELETE /tasks/{id}` 也一并删 runs，同一形状）。

### P4.4 上限与可见性

- **每个 chat 最多保留 N=50 条未投递**（可配常量）。超出时**丢最旧的一条**并 WARN 明确说「上限触发，丢了第 X 条」。丢弃必须说出来——静默丢正是本设计要消灭的模式。
- **可见性**：`GET /api/v1/chats` 的每一行带 `pending_reports: <n>`；面板会话列表该行显示未读数（三态：读不到 → 不说、0 → 不显示、>0 → 显示数字）。**新增一个端点** `GET /api/v1/chat/{id}/reports?pending=1` 供面板展开看（这是 P4 的读面，不做渲染也可以，但要能查到）。

### P4.5 连带收益（做完后必须一起改）

outbox 落地后，**重启不再"花钱买不到话"** —— 汇报留在盘上，你下次打开会话就能看到。所以：
- `crates/daemon/src/lib.rs` 的 boot 清扫**可以恢复 `allow_self_heal = true`**；
- 但**只有在 P4.2/P4.3 都落地并验证过之后**才改这一行，且要补一条测试：重启 → 自愈一轮 → **会话恢复后那条汇报真的出现**。
- 顺序错了就是**又花钱又没话说**。谁改这一行，谁必须交那条测试。

## 2 P5：自愈世代（同一 task 可再次自愈）

**问题**：`healed_already = runs.iter().any(|r| r.params.self_heal_of.is_some())` 按簇数，所以一个 task 被人类重派第二、三次后失败，不再自愈。

**修法（不加列，走 `params` JSON）**：
- `RunParams` 加 `dispatch_epoch: Option<u32>`（`#[serde(default)]`）—— 第几次「人类派发」。`start_run` 时算：同一 task 已有多少个**非自愈** run，就取那个数（0 起）。
- 自愈判定改为：**只看本世代** —— 存在 `params.self_heal_of.is_some() && params.dispatch_epoch == 本 run 的 dispatch_epoch` 才算预算已用。
- 自愈 run 继承被治愈者的 `dispatch_epoch`（它属于同一世代）。
- **向后兼容**：旧行没有这个字段 ⇒ `None` ⇒ 视为同一世代（保守：不额外给自愈预算）。这条要写进注释，否则老数据的行为变化没人看得懂。

## 3 P6：`origin_chat_id` 自动注入（让 agent 不用自己填）

**问题**：MCP `task_dispatch` 的 `origin_chat_id` 只能由 agent 自己填，而 agent 不知道自己在一个 chat 里。

**修法**：daemon 在**启动 agent 进程**时注入环境变量，MCP server 继承后自动填。
- run 的 `origin_chat_id` 已知（`runs.origin_chat_id`，v0.1.10 已有）→ 在 spawn 该 run 的 agent 进程时加 `RUAGENT_CHAT_ID=<origin>`（没有 origin 就不加，**不要**注入空串）。
- `crates/mcp/src/lib.rs` 的 `task_dispatch`：`origin_chat_id` 参数**仍然接受**（显式优先），缺省时读 `RUAGENT_CHAT_ID`。
- 工具描述要改：从「你自己填」改成「默认自动带上你所在会话；显式传值时以你传的为准」。
- **不注入的情况要说清**：面板/CLI 派发的 run 没有 origin ⇒ 不注入 ⇒ agent 派活也没有归属 ⇒ 不汇报。这是对的（没人在等），但要在描述里写明。

**风险**：env 会被 agent 传给它的所有子进程。这是**期望行为**（它的子 agent 也在同一个会话里干活）。但如果同一个 agent 进程服务多个会话（今天不是这样，一个 run 一个进程），这个假设要重新检查——请在报告里写明你验证过这一点。

## 4 分工与文件所有权

| 成员 | 范围 | 独占 | 禁碰 |
| --- | --- | --- | --- |
| `outbox` | P4.1–P4.5 的 P4 部分 | `crates/daemon/src/chat.rs`、`crates/store/src/lib.rs`、`crates/store/src/migrations.rs`、`crates/store/src/migrations/0033_*.sql`、`crates/daemon/src/api.rs`、`panel/**` | `crates/daemon/src/runs.rs`、`crates/core/**`、`crates/mcp/**`、`crates/policy/**` |
| `heal-origin` | P5 + P6 | `crates/daemon/src/runs.rs`、`crates/core/src/run.rs`、`crates/mcp/src/lib.rs`、ACP adapter 里的 spawn env 点 | `chat.rs`、`store/**`、`api.rs`、`panel/**` |

`lib.rs` 的 boot 那一行（P4.5）**由 `outbox` 改**，因为它是 P4 的连带收益；`heal-origin` 不动它。

## 5 验收（一票否决项）

1. **P4 主判据**：造一个**不在内存**的 chat（32 位 UUID 形状的 id，`chats` 表里有行但无 live session）→ `notify` → 事件**在 outbox 里**（没有丢）→ 然后 `POST /chat/{id}/resume` → **那条汇报真的以平台事件形式出现在会话里**，且 outbox 该行 `delivered_at` 非空、`attempts >= 1`。
2. **P4 上限**：塞满 N+3 条 → 只剩 N 条，且日志点名丢了哪几条。
3. **P4 删除**：删 chat → 它的 outbox 行一起没了。
4. **P4.5**：重启 → boot 自愈一轮 → **会话恢复后那条汇报真的出现**（这条证明「重启不再花钱买不到话」）。
5. **P5**：同一 task 派发两次、两次都失败 → **两次都各有一次自愈**（第二次不是"预算已用"）；老行（`dispatch_epoch` 缺失）仍是「不额外给预算」。
6. **P6**：带 origin 的 run → 它的 agent 进程环境里有 `RUAGENT_CHAT_ID`；`task_dispatch` 不带显式参数时，新 run 的 `origin_chat_id` 等于它；显式参数仍然优先；无 origin 的 run 里该 env **不存在**。

## 6 纪律

- 门禁四条，`test --workspace` 必须交三元组。`PROTOC` + `NO_PROXY='127.0.0.1,localhost,::1'`。
- 迁移必须可重放；`SCHEMA_VERSION` 现在是 32，加一条就是 33，**数字要跟着改**（两处 `Some(...)` pin 已改成 `CapabilityId::ALL.len()` 的关系式，但**迁移 head 是硬数字**）。
- 不提交、不 push、不打 tag。
- 每个「丢掉/放弃」路径都要能说出来（WARN 带 id），这是本仓库的既有判据。
