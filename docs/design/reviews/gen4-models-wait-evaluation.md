# `MODELS_WAIT` 评估：产品是否在用墙钟代替可观测终态（只读）

**任务**：`t168`（kind = work，**report-only**）· **日期**：2026-10-04 · **写入**：仅本文件（`crates/**`、`panel/**`、`.github/**`、`scripts/**`、`tools/**` **零改动**）
**来源**：`t157` §7③ 只点名未评估的那一处。**本单只评估 `MODELS_WAIT`**；`spawn_idle_reaper` 的 60s 仅登记（§7）；`crates/**` 其它等待形状由 t157 已盘，**不重盘**。

**结论（先说）**：**不是**「产品用墙钟代替可观测终态」那种缺陷 —— 终态**存在且可读**（`watch<Option<Vec<SessionOptionState>>>`，`None` = 尚未报告、`Some(vec![])` = 报告了「没有」），而且它唯一的「超时折成合法值」出口（`wait_options`）**被下游策略明确剥夺了权威性**（t192：空读数**既不覆盖也不落库**）。⇒ 按 **C55** 多数站点是 **①（有界等待 + 明确后果）**，`wait_options` 那一处属**第三类（把超时折成一个合法值 ⇒ 只让证据不可复现）**，**不是 ②**（它不翻面：最坏后果是「不更新」而不是「给出错的判定」）。⇒ **建议：不改代码**；只立一条**低优先、非缺陷**的加固单（把已经存在于类型里的三态透出来），判据见 §5。

---

## 1. 从字节读清 `MODELS_WAIT` 是什么（坐标 + 原文）

**定义**（`crates/daemon/src/chat.rs:511-512`，紧邻 `IDLE_TIMEOUT`/`OPTIONS_REFRESH`）：
```rust
/// How long to wait for a spawned agent to report its model catalog.
const MODELS_WAIT: Duration = Duration::from_secs(20);
```
它等的现象**不是**「模型切换完成」，而是：**spawn 出来的 agent 会话报告它的「会话选项目录」（options / 含 model）**。五处使用，逐处读出的**超时后果**如下（这就是判类的全部依据）：

| # | 坐标（符号优先） | 在等什么 | 超时后**实际发生什么**（原文） |
| --- | --- | --- | --- |
| S1 | `ChatManager::switch_model`（`chat.rs:1209`） | `ChatCommand::SetConfig{id:"model"}` 的 oneshot 答复（= 运行时报出**已生效**的选项/模型） | `// Reply dropped or timed out: fall through to restart.` → `_ => {}`；随后 `self.close(id)` + `start_inner(card, model, …)`（**重启会话并带上请求的模型**）⇒ 超时的后果是**明确的动作**，且用户可见结果**正确**（模型由重启生效），只是慢 |
| S2 | `ChatManager::set_option`（`chat.rs:1262-1266`） | 同上，非 model 选项 | `Err(_) => Err("agent did not answer in time".into())` ⇒ **响亮错误向上传播**（调用方拿到 `Err`，不是旧值） |
| S3 | `wait_options`（`chat.rs:1685-1691`） | 「会话报告其 advertised options」这个事实（`watch` 从 `None` 变为 `Some`） | `let _ = timeout(…).await; watch.borrow_and_update().clone().unwrap_or_default()` ⇒ **超时折成空 `Vec`**（= 与「报告了『没有』」**同一个值**）—— **本单唯一「时间变成值」的地方** |
| S4 | `apply_role_defaults`（`chat.rs:1699-1705`） | 同上 | `is_err()` ⇒ `tracing::warn!("role defaults: runtime reported no options")` + `return` ⇒ **显式 WARN**；但**标签不准**（真因是「20s 内没报告」，不是「报『没有』」） |
| S5 | `apply_role_defaults`（每项，`chat.rs:1729-1742`） | 每项 `SetConfig` 的答复 | `_ => tracing::warn!("role default: runtime did not answer")` ⇒ **显式 WARN** |

**命名小瑕（登记，不构成缺陷）**：常量叫 `MODELS_WAIT`、注释说 "model catalog"，但它实际是**整个会话选项目录**的等待（S2/S4/S5 等的是非 model 选项）—— 名字比语义窄，未来读者可能误判它的用途范围。

## 2. 这个现象**有没有**可观测终态？—— **有，而且类型里已经区分了三态**

`crates/acp/src/chat.rs:211`（`ruagent-daemon/src/chat.rs:286-287` 转发）：
```rust
pub fn options_watch(&self) -> tokio::sync::watch::Receiver<Option<Vec<SessionOptionState>>>
```
⇒ **三态全在类型里**：`None` = **运行时尚未报告**（就是那个「还没到」的事实）；`Some(vec![])` = **报告了，且它没有选项**；`Some([...])` = 报告了选项。发送方是会话自身的选项跟踪器（`chat.rs:938-948` 起，`watch.borrow_and_update()` 驱动的 per-runtime 目录刷新）；持久化侧是 `agent_options` 表 + 开机 `load_option_cache`（`chat.rs:1371`）。

⇒ **所以本单的判类不可能是 ④（产品没有可观测信号）**：信号存在，且**恰好能表达「尚未报告」**。被丢弃的是这个区分，且只丢在 S3 一处。

## 3. 唯一的落点 S3 为什么**不是 ②**，而是第三类

**②的定义**是「用时间代替判定 ⇒ **会翻面**」。S3 会翻面吗？**今天不会，因为下游把它变得不权威**：`wait_options` 的**唯一**调用者是 `probe_options`（`chat.rs:1304-1322`，两处调用：live chat 与「一次性 probe 会话」），而它必经 `record_probe`（`chat.rs:1339-1367`）：

```rust
if state.is_empty() {                       // 空读数 = 「超时」或「真的没有」，二者同形
    let previous = self.model_cache.lock()…get(runtime).cloned();
    return previous.unwrap_or(CachedOptions { options: Vec::new(), updated_at });
}                                           // ← 不 insert 缓存、不 persist_options
```
代码把这条规则的**理由**写在自己头上（`chat.rs:1325-1334` 与 `1798-1808`，t192）：
> "`wait_options` turns its own timeout into an empty Vec, so a slow (cold start) or mis-registered first probe used to be cached AND persisted as 'this runtime has no options' -- after which every picker stayed empty, including across a restart via `load_option_cache`, and nothing retried it. **Keep the last good catalog instead**: only a probe that actually reported options may replace what we know, and an empty one is not even remembered (so the next open probes again)."

并且有**钉住这条策略**的测试：`an_empty_probe_neither_overwrites_nor_persists_the_catalog`（`chat.rs:1810`，三条断言：空读数不动既存目录 / 冷运行时连「空」都不记 / 真报告仍替换）。

⇒ 于是超时的后果是**「不更新」（保留上一次好值）**，而不是「给出错的判定」⇒ **不满足 ② 的翻面条件**，我判它 **第三类：把超时折成一个合法值 ⇒ 只让证据不可复现**（外加 S4 的标签错，见 §4）。

**若判 ② 就要写的「怎么翻面」场景，我仍然写出来（因为它给出了加固的判据）**：**翻转需要一个新的 `wait_options` 调用者不知道 t192 规则**，直接把那个空 `Vec` 当成「该运行时没有选项」用（例如新的 UI 路径或新的 probe 入口）：此时一次 **冷启动 > 20s** 就会让用户看到**空 picker**，并且若它顺手 persist，还会**跨重启固化**——这正是 t192 之前发生过的真实故障（上面注释所述）。⇒ **今天的防线是一条「约定」而不是类型**：它靠「谁都记得空 vec 不可信」维持，这就是 §5 建议把三态透出来的理由。

## 4. C48：这个墙钟等待在生产里**能不能被解释**？—— **能**

逐站点说明「为什么它今天不构成缺陷」，且都落在**用户可见行为**上：

| 站点 | 超时后用户看到什么 | 为什么这不是假信号 |
| --- | --- | --- |
| S1 `switch_model` | 新模型**确实生效**（会话重启并带上请求的模型） | 超时的后果是**定义好的动作**，不是把「未确认」说成「已确认」；面板得到的是 `Ok((chat, true))`（重启过） |
| S2 `set_option` | **显式错误**（`Err("agent did not answer in time")`） | 未确认就报错 ⇒ 不会静默沿用旧值 |
| S3 `wait_options` → `record_probe` | **保留上一次的好目录**（含它**原来的 `updated_at`**） | 空读数**不覆盖、不落库、连冷运行时也不记**（t192）⇒ 最坏是「没刷新」，不是「假的空」；且 `updated_at` 保持旧值 ⇒ 面板显示的「上次更新」是**诚实**的 |
| S4/S5 `apply_role_defaults` | 角色默认值**不被应用** + 一条 WARN | 超时**不假装成功**；缺的是「没应用」这件事在日志里的**用词准确性**（S4 把「20s 内没报告」写成 "reported no options"） |

**残留的两点（这才是本单真正要交出去的东西，都很小）**：
1. **冷运行时的即时响应仍会说「没有选项，时间是现在」**：`record_probe` 在**没有既存目录**时返回 `CachedOptions { options: Vec::new(), updated_at }`（`chat.rs:1352-1355`），**不落库但不妨碍它作为一次 HTTP 响应**（`api.rs:4811` 的手动同步按钮走 `refresh_agent_options`）。⇒ 对**冷启动超时**与**真的没有选项**这两种情况，**那一次响应**是同形的「空 + 现在的时间」。它不固化（下次会重新 probe），但**这一次**的语义是含糊的。
2. **S4 的 WARN 标签错**：`"role defaults: runtime reported no options"` 说的不是它测到的事（真因是超时），会给排障者一个错方向。

⇒ 按 C48 的措辞：**能解释 ⇒ 今天不构成产品侧缺陷**（用户可见行为最坏是「没刷新」）；上面两点我按**加固建议**而不是 finding 交，因为它们**都不改变用户看到的数据正确性**（点 1 的那次响应确实是「此刻没有可用目录」，点 2 只是日志用词）。

## 5. 建议与可机械化判据（**建议：不改代码；立一条低优先加固单**）

**为什么判断「不该改（现在）」**：用户可见行为已经正确（§4 四行），而改动会碰到**模型/选项目录**这条面板主路径 —— 本代纪律不允许把「用词更准」的收益换成一个新引入的风险。**但**下面的加固是有明确收益的（让防线从**约定**变成**类型**），故建议**立单，低优先**：

**改什么（一处）**：把 `wait_options` 的返回值从 `Vec<SessionOptionState>` 换成它**已经从类型里拿到的三态**（`Option<Vec<SessionOptionState>>`，或 `Result<Vec<_>, TimedOut>`）；`record_probe` 接受三态，把今天「靠 `is_empty()` 推断」的规则写成**显式分支**。

**为什么那个事实在同路径上确实可读（引用坐标）**：`chat.options_watch()` 的负载类型本身就是 `Option<Vec<SessionOptionState>>`（`acp/src/chat.rs:211`、`daemon/src/chat.rs:286`）；`None` 与 `Some(vec![])` **在类型上已经分得开**，`wait_options` 现在用 `unwrap_or_default()`（`chat.rs:1690`）把它们**压成一个值**。⇒ 这不是「补一个新信号」，而是**不要把已有信号丢掉**。

**四条可机械化判据（给修复单的验收用）**：
1. **同形性**：存在一种输入（冷运行时、20s 内不报告）使得 `wait_options` 的返回值与「报告了空」**不是同一个值**（例如 `None` vs `Some(vec![])`）—— 今天这条**不成立**（两者都是 `Vec::new()`）。
2. **策略显式化**：`record_probe` 的「不覆盖/不落库」分支由**三态匹配**驱动，而不是由 `is_empty()` 推断；对应的测试从「喂空 vec」改成「喂 timed-out」后**仍然绿**（现有测试 `an_empty_probe_neither_overwrites_nor_persists_the_catalog`，`chat.rs:1810`，三条断言不变）。
3. **响应可区分**：`api.rs:4811`（手动同步按钮）那条路径在冷启动超时时能返回一个**可区分**的读数（如 `timed_out: true` / 相应的 4xx-5xx），而不是「空列表 + 当前时间」。
4. **标签正确**：`apply_role_defaults` 的超时 WARN 说「no report within MODELS_WAIT」，而不是 "reported no options"（`chat.rs:1703` 与 `1740-1741` 两处用词）。

**20s 这个数本身要不要动？——不要**（边界论证）：它是「**到达类事实**」的上限，而到达时刻由运行时决定（spawn + ACP 握手 + 目录请求，量级为秒；`OPTIONS_REFRESH = 6h` 是刷新节奏，不是这个等待）；挂住的运行时的等待**无界** ⇒ 20s 是**上界**；更重要的是 **它从不成为用户的判定**（§4 四个后果里没有一个把「未确认」说成「已确认」）。⇒ 调大/调小都不改变语义，只会改变「多久放弃」；**没有证据要求改**。

## 6. 分类小结（C55 口径，一句话）

- **S1**：① —— 有界等待 + **定义好的后果**（重启并应用模型）。
- **S2**：① —— 有界等待 + **响亮错误**。
- **S3**：**第三类**（静默替换：把「未报告」折成「报告了空」）—— **不是 ②**（不翻面，§3），**不是 ④**（终态存在，§2）。
- **S4/S5**：① —— 显式 WARN + 不应用（S4 的**用词**不准）。

## 7. 边界：本单**没有**做什么

- **只评估 `MODELS_WAIT`**（上述 5 处）。`spawn_idle_reaper` 的 `sleep(60s)`（`chat.rs:1662`）**只登记**：它是**周期节奏**（reaper 循环），不是等待某个终态，按 t157 已排除；本单**未评估**。
- **不重盘**：`crates/**` 其它等待形状见 `gen4-wait-shaped-tests-inventory.md`（t157）。
- **没有跑真实模型/GUI 开关来观测它**：本单是**纯字节阅读**，我**没有**启动守护进程、**没有**驱动面板、**没有**真实 ACP 运行时；因此「冷启动 > 20s 时面板看到什么」是**从代码推出的**，不是观测到的（我的 §4/§5 已按此措辞）。
- **没有跑任何测试**：因此**不给出任何「绿」读数**。按 **C47**，本处**无前置红证** ⇒ 任何「0/N 绿」都是**零信息**，本单也不据此宣布任何东西已修好/已存在。**没有在共享树上造变异**（不需要：本单不主张任何行为已改变）。
- **零代码改动**：`crates/**`、`panel/**`、`.github/**`、`scripts/**`、`tools/**` 未碰；**活守护进程（8787）未启停**（按 C32 只读归属）；未写 `~/.ruagent`；收尾无临时物需删。
