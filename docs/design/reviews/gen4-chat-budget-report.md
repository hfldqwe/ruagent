# t137 收口 F-A（t129）：注入预算账本接进 chat 路径（mem-core）

- 任务：t137（kind=implementation）；attempt `8ac51ad4-3508-4f32-8c6e-a5d99a501dc2`
- 结论文一句话：**chat 半边接上了，并且第一次有了「盘上事件」的读数**（真实 chat 的 transcript JSONL 里 `context_injected.budget` 是实测账本）；一条能红的测试 + 隔离/宣告窗口负控都做了；**一条判据没复现**（chat 路径上的**总额丢弃** `dropped_items>0`），点名在 §5。

## 1 改了什么（5 处，全在 inScope）

| 文件 | 位置 | 改动 |
| --- | --- | --- |
| `crates/acp/src/chat.rs` | `ChatCommand::Prompt`（147-150） | 新增 `context_budget: Option<serde_json::Value>`：**用事件自己的类型**，所以 ACP 客户端不需要依赖 memory crate —— 它**转发**生产者测到的账本，不解释它 |
| `crates/acp/src/chat.rs` | 事件处（~503-530） | `budget: context_budget`（原来是硬写的 `None`）；注释改写：`None` = 生产者**没渲染**（未采集），全零作为 `Some` 走（那是**实测**） |
| `crates/daemon/src/chat.rs` | `ChatManager::injection_context`（577-） | 返回 `Result<Option<(String, serde_json::Value)>>`：用 `render_context_report`，**在同一处**把报告 `serde_json::to_value(&report).with_context(|| "serialising the chat injection budget report")?` ⇒ **序列化失败是真错误**，绝不 `None`（那会把「渲染过但序列化失败」读成「未采集」） |
| `crates/daemon/src/chat.rs` | `send_prompt`（173-…） | 块返回 `(ctx, ctx_budget)`（两个值**同一个 `render_context_report` 调用**出来，不可能漂）；`ChatCommand::Prompt { text, context, context_budget: ctx_budget }`。**哨兵只改 agent 看到的字符串，不改渲染** ⇒ 账本仍描述那次渲染 |
| `crates/daemon/src/distill.rs` | 732 | `context_budget: None` + 注释：**这里没有渲染**，所以是「未采集」(`null`)，**不是**全零报告 |

与 run 路径**同一口径**：门禁开 = `Some(report)`（全零也算实测）、关 = `None`（未采集）；序列化失败 = 真错误（`runs.rs` 里那次是 `?`，这里是 `with_context`）。**向后兼容未动**：`RunEvent::ContextInjected` 的 `#[serde(default, skip_serializing_if)]` 没碰，`event_compat` 两条都绿（§4）。

## 2 读数：真实 chat 的**盘上事件**（这是本单最重要的一条）

探针 = 仓库既有的 mock-agent 全流程（`crates/daemon/tests/injection_e2e.rs` 的 `boot` + `drive_chat` 形状），我加了自己的测试 `chat_event_carries_the_injection_budget_report`，**读的是 `data/transcripts/run-<chat_id>.jsonl` 里那行事件**，不是代码路径：

```
T137 CHAT BUDGET>>>{"per_block_chars":1024,"total_chars":4096,"used_chars":1473,
  "truncated_blocks":1,"dropped_items":0,
  "blocks":[{"tag":"user_profile","items":5,"chars":1082,"truncated_chars":8107,"dropped_items":0},
            {"tag":"relevant_memories","items":1,"chars":99,"truncated_chars":0,"dropped_items":0},
            {"tag":"knowledge","items":1,"chars":151,"truncated_chars":0,"dropped_items":0},
            {"tag":"wiki","items":1,"chars":141,"truncated_chars":0,"dropped_items":0}]}<<<
render_chars=1496      →  test result: ok. 1 passed; 0 failed
```

测试里**断言**（不是观察）：
1. `budget.is_object()` —— 改前这里是 `null`（负控 §3 实测）；
2. `used_chars == Σ blocks[].chars`（1473 = 1082+99+151+141）—— 账本**自己两种表示必须一致**；
3. `truncated_blocks == ` 报告 `truncated_chars>0` 的块数（1）；
4. **账本描述盘上的字节**：每个 `blocks[].tag` 都必须在同一事件的 `render` 里以 `<tag>` 出现（「⊇」规则）；某块声称 `truncated_chars=8107` ⇒ `render` 里必须能找到 `[+8107 chars truncated]`；若 `dropped_items>0` ⇒ 必须能找到 `items dropped`（本夹具为 0，见 §5）；
5. 夹具必须真溢出：6000 字符的 profile 行 ⇒ `truncated_blocks > 0`（否则这条测试什么也不证明）；
6. 第 3 层：agent 的 echo 里带着注入内容。

**我量错并改正了一处（值得记）**：第一版断言写的是 `used_chars == render.chars().count()`，**真实 chat 上红了**：`used_chars=1473` vs `render_chars=1496`，差 **23** 字符 = 块头/包裹的字节。`used_chars` 是**块内容之和**，不是渲染长度 ⇒ 断言改成上面第 2/4 条（同一事实的两个来源：账本自洽 + 账本对字节）。这条差数是我在真实链路上量出来的，不是从文档抄的。

## 3 负控：宣告窗口（t20 形状），共享树零残留

| 项 | 读数 |
| --- | --- |
| 窗口起 | **2026-10-04T00:54:20.686+08:00** |
| 变异 | `crates/acp/src/chat.rs` 单文件单词：`budget: context_budget,` → `budget: None,` |
| 控制红 | `T137 CHAT BUDGET>>>null<<<`，`panicked at ...injection_e2e.rs:898: the chat event carries NO budget report (null) — the ledger is not wired to this path`，`1 failed` |
| 窗口止 | **2026-10-04T00:54:45.055+08:00**（≈24 s） |
| 恢复判据 | 恢复后 `sha256` 与变异前**逐字节相同**：`2db996df15872a81…160dd8`（前后同值），`grep budget: context_budget,` 命中唯一站点 |
| 恢复后绿 | 同一条测试 `1 passed; 0 failed` |

**为什么窗口是安全的**：这条测试带 `#[ignore = "needs ruagent-mock-agent …"]`，默认 `cargo test` 不跑它 ⇒ 窗口期内**任何同伴的门都不会看到红**（它不是靠「大家没跑」赌的，是结构上跑不到）。控制与恢复都在同一次调用里串行完成。

## 4 门禁（我在**最终字节**上本人重跑；最终字节 = 跑完 `rustfmt` 之后）

| 命令 | 读数 |
| --- | --- |
| `scripts/cargo-team.ps1 test -p ruagent-daemon` | **exit=0**；11 个 `test result` 行：`157/0`、`11/0`、`5/0`、`2/0`、`16/0`、`0/0(8 ignored)`、`16/0`、`3/0`、`0/0`、`4/0`、`0/0` ⇒ **214 passed / 0 failed** |
| `scripts/cargo-team.ps1 test -p ruagent-acp` | **exit=0**；`15 passed / 0 failed` + `0/0` |
| `scripts/cargo-team.ps1 clippy -p ruagent-daemon --all-targets -DenyWarnings` | **exit=0** |
| `scripts/cargo-team.ps1 clippy -p ruagent-acp --all-targets -DenyWarnings` | **exit=0**（我动了 acp，所以这条必须跑） |
| `cargo fmt --all --check` | **exit=0**，**0 行 diff** |
| 我那条 ignored 测试（额外） | **exit=0**，`1 passed`（§2 的 JSON 就是它的输出） |

**最终字节归属**（sha256；读到报告里的每个读数都来自这四份）：
`crates/acp/src/chat.rs` `2db996df15872a815899530843c0097cb1b4684bdb5e0ce796521c8a1f160dd8` ·
`crates/daemon/src/chat.rs` `14387c3cc3120a4f7f20d975b0c1322703f57cc61cd126e8142b8b032ff9bdac` ·
`crates/daemon/src/distill.rs` `0291df0e6e580a7f5d806d5edf8a36a2cf7d409519e8e2045ba533f1a7ca4e3f` ·
`crates/daemon/tests/injection_e2e.rs` `5dbc7ac13c4a892a5e16eb5a03163660b71b8ef15eae5e1f9388463ce4b43d58`。
mock-agent 二进制（我只复用、未重建，它只负责 echo）：`%TEMP%\ruagent-team-target\debug\ruagent-mock-agent.exe`，mtime **2026-10-04 00:14:28**，5,756,928 B。

## 5 未测 / 未覆盖（点名）

1. **chat 路径上的「总额丢弃」没有复现**：`dropped_items=0`。原因是这条路径的四个块**各自被 `per_block_chars=1024` 截断**（profile 1082 + memory 99 + knowledge 151 + wiki 141 = 1473 < 4096），要挤掉一个块需要**四个块同时逼近 1024**（约 4096+），而 memory/knowledge/wiki 三条腿是**向量检索**选出来的：我塞的 20 条 `project:overflow` 观测行**一条都没被选中**（读数：`relevant_memories items=1`）—— 所以「塞数据就能溢出」是错的。**需要什么才能测**：给 knowledge 文档建索引（多条长文档），或让 chat 的记忆腿拿到多行；owner = fixtures / `memembed` 的选择旋钮，**不是本单 inScope**。**我没有用「代码路径已钉」代替它** —— §2 的读数是真实 chat 的盘上事件，只是它**不含**总额丢弃这一档。
2. **只有「渲染过」那一档有盘上读数**：`None`（门禁关 / 无渲染）这一档**没有真实 chat 读数**。未做的原因：把 `MemoryInjectChat` 关掉要在测试里改 capability plane（`boot` 不做），而 `None` 的语义由 distill 站点（`context_budget: None`）与适配器注释承载 ⇒ **未测**，不是「已验证」。
3. **没有测「序列化失败」这条真错误路径**：要有一次 `to_value` 失败才看得到 `with_context`（`BudgetReport` 全是数字/字符串，构造不出来）⇒ 该分支**未测**；它的形状与 `runs.rs` 的同款（t129 已落地）一致，但「同款」不是读数。
4. **`ChatCommand::Prompt` 是进程内的枚举**，测试与生产用同一份枚举 ⇒ 「事件里的三字段与**报告**逐字段相等」我是用**同一事件里的账本与渲染字节**两条来源做的（§2 的 2/4 条），**没有**再调一次生产者拿第二份报告做逐字段 diff：`TestDaemon` 不暴露 knowledge/embedder 句柄（要改 `boot` 的结构，超出本单的最小改动）。这一点**明写为未测**。
5. 未做：活守护进程（C32 未引用；本单没起任何进程 —— 除了测试自己起的临时 daemon+临时 root，测试结束时由 harness 清理）；未跑 `test --workspace`（本单只跑指定门）。

## 6 我造成的一次仓库级红（点名，已修）

**`cargo fmt --all --check` 被我弄红过一次，并且让 `graph` 的 t138 如实报 failed**（它的四条门里前三条全绿、自己文件 rustfmt 漂移为 0，第 4 条红）。原因：我新加的 `assert_eq!` 被写成多行（`:929`）以及文件尾多一空行 —— 是**我写进 `crates/daemon/tests/injection_e2e.rs` 的字节**。

**更该记的是我为什么没当场看见**：我第一次跑这条门时用了 `... | Select-Object -First 10`，**管道提前终止把 `$LASTEXITCODE` 污染成 0** ⇒ 我读到了一个**假绿**（输出里明明有 diff 行）。这正是本队已记账的 t117 陷阱（`Select-Object -First` 污染原生命令退出码），我**在本代重犯了一次**，代价是别人的一张单。修法：`rustfmt` 只格式化我碰过的四个文件（不跑 `--all`，避免动到同伴在途文件），然后 `cargo fmt --all --check 2>&1 | Tee-Object -Variable f | Out-Null; $LASTEXITCODE` ⇒ **exit 0、0 行 diff**；`git status --porcelain` 前后**同一份文件清单**（我只动了上述四个 + 本报告）。

## 7 残留

- 未改：`crates/daemon/src/api.rs`、`runs.rs`（按纪律不动）、`crates/memory`、`panel`、`scripts`、`.github`。
- 未 push / dispatch / rerun / cancel / 建 tag；未写活库；**未起停任何常驻进程**。
- 测试自建的临时 root 由 harness 清理；我另外留下的只有本报告。
