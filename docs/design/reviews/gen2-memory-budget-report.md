# 预算/丢块报告：`budget` 恒 null 的收口（t60 / INT-F6，mem-core）

- 任务：t60（kind=repair round 2）；attempt `4c886656-59c5-4795-9b6f-ada721029c6e`
- inScope（stored）：`crates/memory/` · `docs/design/reviews/gen2-memory-spec.md` · 本报告
- outOfScope（未触碰）：`crates/store/` · `crates/knowledge/` · `crates/graph/` · `crates/daemon/src/wiki.rs` · `crates/daemon/src/distill.rs` · `crates/daemon/src/memembed.rs`
- 纪律：不写活库、未启停 pid 79984、未用 `/api/v1/recall`

## 1 先复现

| 读数 | 来源/时间 | 值 |
| --- | --- | --- |
| `context_injected` 里 `budget` 非 null 的条数 | **V-INT t20/t21**（`gen2-integration-verify.md:38`：`5/5 有 path`、`0/5 有 budget`；`gen2-integration-review.md:83` 同读数） | **0/5（恒 null）** |
| 恒 null 的代码原因（我今天复读） | `crates/daemon/src/runs.rs:1278` · `crates/acp/src/chat.rs:505` | 两处都写 `budget: None,`，注释逐字自述：契约**只返回字符串**，没有报告型渲染 ⇒ 填字段只能**反推** `blocks[]`（= 把「看起来对」当读数，t19 §9-3） |

**它使两条锚点不可判**（不是「没跑到」，是**数据从不产生**）：
- **A-2 的预算半边（契约 §6.1 第 449 行）**：含 `budget` 的事件数 ÷ 总数、`used_chars ≤ 4096`、`blocks[]` 的 tag 集合与渲染一致、预算绑定 ⇒ `dropped_items>0` —— 一样也读不出来。
- **N-6（§6.2 第 473 行）**：「渲染里出现丢块时，事件里 `budget.dropped_items>0`」—— 事件里根本没有这个字段。

**契约自相矛盾（原文逐字）**
- §3.2 第 198 行：``budget [新增] | 对象 | 见下；**null = 本次未采集**（不许写全 0 假装采集了）``
- §6.1 A-2：要求「含 `budget`/`path` 的注入事件数 ÷ 总注入事件数…`used_chars ≤ 4096`、`blocks[]` 的 tag 集合与渲染一致」；§6.2 N-6：要求 `dropped_items>0`

⇒ **「允许缺省」与「要求取值」被写在同一份契约里，且没有东西裁决谁赢**。这与 **t41 裁过的 R-B D4 vs D2(b)** 是同形冲突，处理方式照抄那次的裁决逻辑：**不是二选一地删一半，而是把缺的仪器补上，让要求可满足**。

## 2 裁决：(a) 已落地 —— 渲染侧给可序列化报告，字节逐字节不变

`crates/memory/src/inject.rs`：

```rust
pub struct BlockReport { tag: &'static str, items: u32, chars: usize, truncated_chars: u32, dropped_items: u32 }
pub struct BudgetReport { per_block_chars, total_chars, used_chars: usize,
                          truncated_blocks, dropped_items: u32,
                          blocks: Vec<BlockReport>, notes: Option<String> }
pub fn render_context_report(items, budget) -> (String, BudgetReport);
pub fn render_context(items, budget) -> String { render_context_report(items, budget).0 }
```

**为什么是「一个代码路径」而不是「在 render_context 旁边再写一个」**：报告若由第二次遍历/文本反推产出，就有两个来源 —— 正是 t19 拒绝反推的理由。现在 `render_context` 是薄包装，**字节与账出自同一次遍历**，二者不可能分家（并由性质测试在任意输入上断言）。

**形状 = 契约 §3.2 的字段表**（逐字段断言，因而不是「加了 `#[derive(Serialize)]` 就算」）：

```json
{"per_block_chars":24,"total_chars":4096,"used_chars":81,"truncated_blocks":1,"dropped_items":0,
 "blocks":[{"tag":"user_profile","items":1,"chars":81,"truncated_chars":190,"dropped_items":0}]}
```
`notes` 未设 ⇒ **不序列化**（§3.2 的可选字段）；空输入 ⇒ `{"used_chars":0,"dropped_items":0,"blocks":[]}` —— **测得的 0，不是缺字段**。

## 3 读数（正读 / 负控 / 不变量）

**(1) 改前 → 改后逐字节对比**（改前读数由一个临时探针在**重构前**跑出并记下；改后由 `the_report_refactor_moved_no_byte` 钉住）

| 夹具 | 改前 chars / 字节 | 改后 | 报告 |
| --- | --- | --- | --- |
| `multi_block`（4 块） | 276 | **逐字节相同** | `used=276/1000 blocks=["user_profile","knowledge","wiki","project_context"] truncated=0 dropped=0` |
| `per_block_cut` | 81（`… [+190 chars truncated]`） | 相同 | `used=81/4096 truncated_blocks=1 blocks[0].truncated_chars=190` |
| `whole_block_drop` | 14（`… [+2 dropped]`） | 相同 | `used=14/40 blocks=[] dropped_items=2` |
| `all_dropped` | **0（空渲染）** | 相同 | **`used=0/10 blocks=[] dropped_items=1`** ← 文本说「什么都没发」，账本说「1 条被丢」 |
| `empty` | 0 | 相同 | `used=0/4096 dropped_items=0`（**测得的空**） |
| `low_confidence` | 81（`[unverified] ` 前缀） | 相同 | `blocks=["user_profile"]` |

`all_dropped` 那一行是本单最有价值的读数：**同一个空字符串，两个不同的世界**（没有输入 vs 全被丢）—— 渲染文本分不开，账本分得开。这就是 N-6 需要仪器的原因。

**(2) 负控与正控**（验收原话：无丢块 ⇒ 计数 **0** 而不是 null/缺失；有丢块 ⇒ **>0**）

```
READING t60 negative control: used=68/4096 chars (per_block=1024) blocks=["user_profile"] truncated_blocks=0 dropped_items=0
READING t60 positive control: render="… [+2 dropped]" used=14/40 chars (per_block=4096) blocks=[] truncated_blocks=0 dropped_items=2
READING t60 truncation control: used=81/4096 chars (per_block=24) blocks=["user_profile"] truncated_blocks=1 dropped_items=0
```
断言：负控里 `dropped_items == 0`、`truncated_blocks == 0`、`lost_anything() == false`（**「0 条被丢」是一个读数，不是一个缺失**）；正控 `dropped_items == 2`；只截断不丢块时 `truncated_blocks == 1` 且 `lost_anything() == true`（**截断也是损失**）。

**(3) 不变量：三条全部仍绿，且新增了一条「报告 == 渲染」的性质测试**

- `the_total_bound_holds_with_knowledge_items_too`（**有界**）· `per_block_truncation_is_visible` / `the_low_confidence_mark_survives_truncation`（**截断可见**）· `the_two_presets_state_their_own_differences`（**带 tag**）· `golden_render`（字节 golden）—— 全绿。
- 原有性质测试 `never_exceeds_total`（任意记忆集/预算下 `chars ≤ total`）—— 全绿。
- 新增性质测试 `the_report_never_disagrees_with_the_render`（任意输入）断言：`used_chars == render.chars().count()`；`used_chars ≤ total_chars`；无丢块时「块字符和 == 渲染」；`truncated_blocks` == 渲染里 ` chars truncated]` 的出现次数 == `blocks[]` 里 `truncated_chars>0` 的个数；`blocks[]` 的 tag 序列 == 渲染里的 tag 块序列。
  - **写这条时它先红了一次**，红的正是「`<context_budget>` 不是 tag 块」：我的 tag 扫描把丢弃通知的包裹标签当成了一个 tag 块，而 `blocks[]` 按 A-2 的判据只装 tag 块。修法是让比较**只认输入 tag**（`rendered_tag_blocks`），并**保留** proptest 存档的那条种子（`crates/memory/proptest-regressions/inject.txt` 新增一行，注释即最小反例）—— 这条种子每次都会重跑，从此钉住「通知块 ≠ tag 块」这个区分。这是本单唯一一次红→绿，如实记录。

## 4 交回 captain 的 finding（确切构造点 + 需要的形状）

| 文件 | 现在 | 需要的形状 |
| --- | --- | --- |
| `crates/daemon/src/runs.rs:1270-1279`（run 路径事件） | `budget: None,`（第 1278 行；注释自述「契约没有报告型渲染」） | 让 `render_run_injection` 回 **`(String, BudgetReport)`**（它现在回 `String`，报告在它内部就已算出）；事件处 `budget: Some(serde_json::to_value(&report).expect("report is serializable"))` |
| `crates/acp/src/chat.rs:496-505`（chat 路径事件） | `budget: None,`（第 505 行） | 同上：`injection_context` 把 `(String, BudgetReport)` 交给调用方，`Some(serde_json::to_value(&report)…)` |

- 两处都是**第 16 条跨 crate**：`RunEvent::ContextInjected.budget` 的类型是 `Option<serde_json::Value>`（`crates/core/src/event.rs:138`）⇒ 调用方只需 `serde_json::to_value`，**不需要改 core**（尤其**不要**把 `budget` 改成必填：那会破坏 §3.2 对旧 JSONL 的向后兼容，`event_compat.rs:36` 正断言旧事件读出 `None`）。
- **A-2 的预算半边与 N-6 仍是「未达成」，不是 not_measured**：仪器已就位（本单），缺的是从事件到仪器的那一段接线；owner = 持 `runs.rs` / `acp/chat.rs` 写权的那张单。**今天谁若报「budget 已达标」是假的；谁若把它记成 not_measured 也是假的** —— 它现在可判为未达成（有确切构造点、确切形状、确切 owner）。

## 5 一处诚实的限制（路由给 INT）

`blocks[]` 只列**发出去的**块 —— 否则 A-2 的「`blocks[]` 的 tag 集合与渲染一致」在读侧会被判失败（我在实现报告时把这一点当作硬约束）。代价：**「丢的是哪个 tag」（§3.3 问题 2）在冻结字段里答不出来**，只能从渲染的 `<context_budget>` 通知看「丢了几条」。若这条问题要保留，§3.2 需要二选一：加 `dropped_tags: [tag]`，或明确 tag 集合的语义是「⊇」（把 `chars=0` 的丢块也列进 `blocks[]`）。**本单不改契约**（INT 的文件，不在 inScope），只登记。

## 6 门禁与范围

| 命令 | 读数 |
| --- | --- |
| `test -p ruagent-memory` | **exit=0** · `71 passed; 0 failed; 0 ignored`（3.42s）+ doctest 0/0（t58 后 66 → +5：4 条报告测试 + 1 条报告性质测试） |
| `clippy -p ruagent-memory --all-targets -DenyWarnings -CleanFirst ruagent-memory` | **exit=0** · 含本轮 `Checking ruagent-memory`（同锁窗口 clean+gate ⇒ 真重查），零诊断 |
| （额外，未跨 crate 改文件）`check -p ruagent-daemon --all-targets` | **exit=0** ⇒ 记忆侧新增公共面与同伴在途文件一起编译通过（本单**没有**改任何 daemon/acp 文件，故第 16 条的 daemon clippy 不触发） |

**changedPaths（本单实际改动）**：`crates/memory/src/inject.rs`（报告类型 + 报告型渲染 + 5 条测试）· `crates/memory/Cargo.toml`（`serde_json` 进 dev-dependencies，用于逐字段断言 §3.2 形状）· `crates/memory/proptest-regressions/inject.txt`（新增 1 行种子，见 §3(3)）· 规格追加段（1309 → 1363 行，纯追加）· 本报告。`store`/`knowledge`/`graph`/`wiki.rs`/`distill.rs`/`memembed.rs`/`runs.rs`/`acp` **一字未改**。

## 7 未做 / 边界（无静默跳过）

- **事件里的 `budget` 仍是 null**（本单结束时）：仪器在，接线不在（§4）。这是唯一未闭合项，且是「没接线」不是「测不了」。
- 盘上第三层读数（真实 turn 的 `budget` 非 null）需要真 daemon 或临时 root 的进程内探针；本单按纪律不起真 daemon、不写活库，留给接线后的那张单（它必须给 `budget` 非 null 的事件数 ÷ 总数）。
- `notes` 字段本单只实现「未设则不序列化」，没有生产写入者 —— 谁采集谁写（例：`"probe"`），不预设。
