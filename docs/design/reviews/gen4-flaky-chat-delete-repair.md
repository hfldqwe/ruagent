# V-B7 收口：负载敏感 flaky 的 chat 删除用例 → 有界等待 + 显式同步

**任务**：`t145`（repair）· **日期**：2026-10-04 · **inScope 唯一写入**：`crates/daemon/src/chat.rs`（测试模块 `mod generating_tests`）· **产品路径最终零改动**

**结论一句话**：那条红**不是**产品侧竞争 —— 是**「固定 1200ms 猜测窗口」对「机器决定的事件投递延迟」**，加上**判据数了所有 `RunEvent` 而声明说的是消息**。已改成**有界等待 + 显式同步（等运行自己的终局信号）**，并只数 `AgentMessageChunk`；在**同一负载形状**下复跑 **0 红**。同时**如实登记**：修后仍有**一个未收敛的残余**（见 §6，两条产品侧读数），它指向 `delete` 的**停止路径可能排在在飞 prompt 之后**。

---

## 1. 复现率（第 1 条判据）

**负载形状（我自己起的，共享树零变异）**：10 个自建 CPU 占用进程（`[math]::Sqrt` 忙循环，pid 记录、结束按 pid 杀）+ **一个并发的冷构建**（私有 target `%TEMP%\t145-load-target[2..6]` + `-TargetDir` + `-NoLock`，**不占共享锁、不碰共享 target**）+ 全套 `test -p ruagent-daemon`。

| 形状 | 运行次数 | 红 | 备注 |
| --- | --- | --- | --- |
| 只加 10 个占用进程（无并发构建） | 17（10×单测 + 4×lib 全套 + 3×全套） | **0** | ⇒ 不是「一般负载」 |
| 占用 + **并发冷构建**（PRE2，旧字节 `14387C3C…`） | 3 | **1** | run3 `exit=101`，`chat.rs` 那条 `left: 1 / right: 0` |
| 占用 + 并发冷构建（DIAG，窗口 #1） | 4 | **2** | run3/run4 红在本用例 |
| 占用 + 并发冷构建（DIAG2，窗口 #1 持久诊断） | 3 | **3** | 其中 2 次红在**另一个**用例（见 §6.f1）、1 次红在本用例 |
| **本用例合计** | **10** | **4** | **复现率 4/10 ≈ 40%**，且**只在编译/链接井喷式负载**下 |

**第一版方法论失误（已重做，如实记）**：头一次我把并发构建写成 `--target-dir`（cargo 参数）而没给包装器的 `-NoLock` ⇒ 包装器的锁把我的测试**串行化**（单次跑到 310s / 839s），那批读数**无效**。正确形状 = `-TargetDir <私有> -NoLock`。

## 2. 竞争在哪（第 2 条判据：读出来的，不是猜的）

窗口 #1 的**持久诊断**（把每个被计入的事件的**种类**与**相对删除的毫秒数**写到临时文件）给出**有名有姓**的答案：

```
t145-diag-30732-quiet-0: 2216ms quiet StateChanged { status: Running }
t145-diag-30732-quiet-1: 2373ms quiet UserMessage { text: "hello" }
```

⇒ 被计入的两个事件是 **`StateChanged { Running }`（删除后 2216ms）** 与 **`UserMessage { "hello" }`（2373ms）** —— **在飞那次 prompt 自己的派发簿记**，**不是** agent 消息；删除后**没有任何 `AgentMessageChunk`**。同形状一次通过运行的 settle 窗口按序排空了整轮：

```
906ms StateChanged{Running} → 953ms UserMessage{"hello"} → 1000ms AgentMessageChunk{"echo: hello"}
→ 1047ms UsageUpdate → 1095ms Stopped{EndTurn} → 1144ms StateChanged{Completed}
```

**竞争的两件事**（坐标按当时那份字节，且**符号优先**）：
1. **在飞那一轮的自身事件投递**（`UserMessage`/`StateChanged`，尾随 `Stopped`）—— 由 `Chat::send_prompt` 把 `ChatCommand::Prompt` 交给会话、会话再广播 `RunEvent` 而产生（`send_prompt` 里 `generating.store(true)` 与 `self.send(ChatCommand::Prompt{..})`）；它的**投递时刻由机器决定**。
2. **测试里那段固定 1200ms 的 `settle` 猜测窗口**（旧字节：`let settle = Instant::now() + Duration::from_millis(1200)` 与其后的 `if witness.try_recv().is_ok() { after_teardown += 1 }`）。

第 1 件晚于第 2 件 ⇒ 尾巴落进「quiet」窗口 ⇒ 被算成「删除后又产出一条消息」。**第二个缺陷**：`try_recv().is_ok()` 把**任何** `RunEvent` 都当消息，而声明（`"a deleted chat must not produce another message"`）只关于消息。另外：测试的 witness 是在 `send_prompt` **之后**才 `subscribe()` 的，所以它看到的本就是「prompt 派发」这一族事件。

⇒ **不是产品侧竞争**：删除后没有新消息被生产；红的是**投递延迟**与**判据口径**。

## 3. 修法（第 3 条判据）

1. **主体换成 `--behavior slowreply`**（`crates/mock-agent`：10s 可取消回合，**取消时不发消息**，只在自然结束时回 `EndTurn`）⇒ ①「delete WHILE GENERATING」不再是竞争（`is_generating()` 断言也有了真实主体）；② 取消/未取消两个世界**可分**。
2. **显式同步 + 有界等待**：删除后**等运行自己的终局信号** —— `RunEvent::Stopped`（`Stopped{stop_reason}`）或 `RunEvent::Error`，**与 `start` 里那个把 `generating` 清掉的 watcher 用的同一对**（`Ok(RunEvent::Stopped{..}) | Ok(RunEvent::Error{..}) => generating.store(false,..)`）；**或者**会话 sender 消失（broadcast 在**所有** sender 掉落后报 `Closed`，而 `delete` → `close` → `ChatCommand::Shutdown` 正是拆掉那个 sender 的路径）。**上界 `TEARDOWN_BOUND = 5s`**：不是口味 —— 主体自己的回合是 **10s**，所以**没被 delete 停掉的运行不可能在上界内产生终局事件**。等待**吸收负载**，断言保留声明。
3. **只数 `AgentMessageChunk`**（声明的主体），尾部再给一个 500ms 有界窗口。
4. 断言**自解释**：失败信息带上「删除后看到的事件序列（种类+毫秒）与消息计数」，下一次红会自己说明原因。

**为什么这不是「把 sleep 调长」**：退出条件是**产品自己的状态变化**（终局事件/通道封闭），不是时长；5s 只是**上界**（防挂），且它被选成**严格小于主体自身回合** ⇒ 它区分的是「被停掉」与「自己跑完」两个世界。**也不是「重试到过为止」**：一次等待、一次判定。

## 4. 改完仍能红（第 4 条判据）

**窗口 #2**（宣告过；按 captain 要求给**耐久标识**而非行号）：

```
文件: crates/daemon/src/chat.rs
断言原文: assert!(torn_down, "delete must stop the live run BEFORE dropping the record: …")
预期红的测试名: chat::generating_tests::delete_removes_the_row_and_stops_a_live_run_first
动过的非 #[cfg(test)] 路径: (1) ChatManager::delete —— 去掉「若 is_generating 则 send(ChatCommand::Stop)」；
                          (2) ChatManager::close —— 去掉 `let _ = c.send(ChatCommand::Shutdown);`
```

**读数**：开窗 `02:07:51`（变异后 `chat.rs` sha256 `2A2B5431…`）⇒ `test -p ruagent-daemon --lib delete_removes_the_row_and_stops_a_live_run_first` **exit=101 RED**：

```
panicked at crates/daemon/src/chat.rs:2068:9:
delete must stop the live run BEFORE dropping the record: 5s after the delete the run had
produced neither its terminal event nor released its session …
test result: FAILED. 0 passed; 1 failed; 0 ignored; finished in 5.11s
```

⇒ **5.11s = 上界到点**，正是预期的红。关窗 `02:08:54`（**63.4s**），按字节恢复：
`sha256(worktree chat.rs)` 开窗前 = 恢复后 = **`7EB4D185CC7933CCAB6F6FFA1D9BED33F0D11912714E173E793EBECDA05E52F6`** ⇒ **共享树零残留**，产品路径零改动。

**窗口 #1 的双锚（C44 口径）**：(a) HEAD 侧 = `14387C3C…`（mem-core 核对 = t137 交付字节）；(b) 开窗前的 worktree 值 = **同一个 `14387C3C…`** —— 出处在窗口 #1 **之前**跑的那次负载作业的自打印（`[PRE2] chat.rs sha256=14387C3C…`）。⇒ 那一刻文件在盘上是**干净的**，所以两锚相同；`git checkout` 恢复到的正是 (b)。窗口内值 = `3C04E28D…`（eprintln 版）/持久诊断版同理。**教训**：(b) 应由开窗者**当场打印**；我这次是负载作业的例行打印**兼职**当了基线，属于运气，不该当习惯。

## 5. 负载下的复跑 + 门禁（第 5、6 条判据）

**最终字节** `sha256(chat.rs) = C939062C6D7A4454DAE55FACEFB2F1C40C636785C748FC8880F44598006EAA92`。

| 门禁（最终字节上，exit 码在**任何管道之前**捕获，按 C40） | 读数 |
| --- | --- |
| `scripts/cargo-team.ps1 test -p ruagent-daemon` | **exit=0**；**ok-lines=11 · FAILED-lines=0 · panicked-lines=0** |
| `... clippy -p ruagent-daemon --all-targets -DenyWarnings` | **exit=0**，`error-lines=0`（增量重查，契约命令未带 `-CleanFirst`，如实记） |
| `cargo fmt --all --check` | **exit=0**，零 `Diff in` |

**负载复跑（同一形状：10 占用进程 + 冷私有 target 构建 + 全套）**：改前 **4/10 红**；改后 **0/4 红**（timings 312.7/49.7/49.5/17.1s，且其中 run1 的 lib 全套 3.74s、`injection_e2e` 16.33s，说明负载真实）。**t20 口径**：那 4 次读数属于**上一版字节** `7EB4D185…`；最终字节 `C939062C…` 与它的差异**只在同一测试块内的诊断簿记**（`seen: Vec<String>` 事件序列进断言信息），**同步点与判据未变**；我另在最终字节上重跑了三条门禁与一次全套（上表）。

**最终字节上的负载复跑（把 t20 那条补齐，不再依赖上一版字节）**：同形状（10 个占用进程 + **冷**私有 target 构建（新目录 ⇒ 真冷）+ 全套）**N=3 ⇒ 0 red**（timings 19.2 / 25.8 / 18.5s；每次 `157 passed; 0 failed` 等 11 个目标全绿）⇒ 「改后在同一负载形状下 0 红」**在最终字节上成立**。

## 6. 未覆盖 / 残余 / 新 finding（第 19 条口径）

- **残余（必须看的诚实项）**：在**同一版修复**上，我有 **2 次「无负载」全套红**（`chat.rs:2068` = 终局信号 5s 内没到；关闭窗口 #2 后紧接着的两次全套），**随后 3 次全套 + 1 次全套门禁全绿**，**我自己的负载残留进程数 = 0**（按命令行匹配我自己路径查过，排除自身 pid）。⇒ **我把它当作产品侧读数，不当作已消失的噪声**：`delete` 的停止路径（`Stop`/`Shutdown` 经会话命令通道）**可能排在在飞 prompt 之后** ⇒ 运行**不被及时终止**，最坏到该轮自己结束（`slowreply` = 10s）。**需要什么**：`crates/daemon/src/session`/命令循环的所有者（**不在本单 inScope**）确认 `Stop`/`Shutdown` 是否会在 prompt 在飞时被处理；若是「排在后面」，修法是让停止**带外**生效，或让 `delete` 等到运行真的结束。**本单新增的自解释断言**（`events seen after the delete: …`）就是为下一次复现准备的证据。
- **f1（新发现，同形状下抓到，本单不修）**：`chat::generating_tests::message_count_is_real_before_the_index_catches_up` 在与 §1 **相同**形状下 **2/3 红**（DIAG2 run1/run2，断言 `left: Some(0) / right: Some(1)`）⇒ 同一家族（等一个**已经发生**的事，却用**立即**断言），建议单独开单；它不共享本单的等待 helper（形状可共享，机制不能）。
- **不覆盖**：`delete` 的**停止路径本身**（session/命令循环在 `crates/daemon/src/` 的其它模块，本单 out of scope）；`ChatCommand::Stop` 是否真的把 `$/cancel_request` 发给 ACP 子进程（**未验证**，只观察到「有时终止很快、有时 5s 内不终止」）；负载形状的**通用性**（我只测了「CPU 占用 + 编译/链接井喷」这一族，没测内存压力/磁盘压力）。

## 7. 零改动自证

- 产品路径（非 `#[cfg(test)]`）**最终零改动**：`ChatManager::delete` / `close` / `Chat::send_prompt` / watcher 均未变；全部改动落在测试模块内（含主体 `--behavior echo` → `slowreply`）。
- 两个窗口都按字节恢复并给出 sha256 双锚；未启停活守护进程（8787 = pid 14944 全程未触碰）、未写活库、未按名字/端口批量杀进程（负载进程按**记录过的 pid** 与**我自己的命令行标记**清理，且查过残留 = 0）。
- 未动 `crates/daemon/tests/**`（out of scope）、未动 `crates/daemon/src/runs.rs`、未动 `crates/mcp/**`、未动 `panel/**`、`.github/**`、`scripts/**`。
