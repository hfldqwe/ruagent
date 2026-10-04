# t157 B 组收口：裸 sleep+否定断言 / 耗尽静默 / Drop 无声

**任务**：`t160`（repair）· **日期**：2026-10-04 · **inScope 写入**：`crates/daemon/src/chat.rs`、`crates/store/tests/ledger_reopen.rs`、本文件
**来源**：`t157`（已 completed 的只读盘点）B 组 3 处 —— **本单不重新盘点**，B4/B5/B6 与全部 `poll_*` helper **只登记不改**。

**最终字节**：`chat.rs` = **`3CB995BE581E02433E14F5BDE106BE0B71551C003EF840EE33673B7CB305862A`**、`ledger_reopen.rs` = **`844BBD76AA2B49050DC71699E702A997C6D715E923B104460FB07AB56599FB71`**（与窗口前后锚**逐字相同**）。

---

## 1. B1 —— 两处裸 `sleep(300ms)` + 否定性断言（唯一「判定依赖预算」的一条）

### 1.1 从字节读清「它在等什么」（改了行号，故以符号为准）

- 坐标：**`chat::t8_tests::the_unattended_plan_decides_and_the_default_spends_nothing`**（`crates/daemon/src/chat.rs`）。
  ⚠️ **更正我 t157 报告里的一处坐标**：那里我写的是 `chat::generating_tests::…`；本单的 panic 路径显示该模块是 **`t8_tests`**（符号优先的规矩就是用在这里）。
- **它在等的「事实」根本不存在**（字节依据）：`ChatManager::maybe_auto_distill` 的**头四行**是
  `let Some(distiller) = self.auto_distiller() else { return };` → `let plan = self.unattended_plan();` → **`if plan.is_empty() { return; }`** → 之后才是 `tokio::spawn(async move { auto_distill_now(...) })`。
  ⇒ **空计划下函数在 spawn 之前就返回了**：没有异步生产者，没有「稍后会出现的行」。原来那两处 `sleep(300ms)` 是在**等一个不可能发生的事**，而它后面的断言是**否定性**的 ⇒ 这是本族里唯一「**睡不够就会通过**」的形状（假绿）。
- 产品**已经报出**这个事实：`unattended_plan()` → `ExtractPlan::is_empty()`（同一个测试在 (2)/(3) 里本来就在读它）。

### 1.2 改法：断言前提，删掉 sleep（**没有引入任何 bound**）

- (1) DEFAULT：新增前提断言 `assert!(manager.unattended_plan().is_empty(), "the default plane must resolve to an EMPTY plan …")`，随后**立即**断言 `rows(&db).await.is_empty()`；**删掉 `sleep(300ms)`**。
- (3) OFF AGAIN：原有的 `assert!(manager.unattended_plan().is_empty())` **保留**（它就是前提），**删掉 `sleep(300ms)`**，随后立即断言 `rows(&db).await.len() == 1`。
- **为什么这里「有界 + 耗尽响亮报错」的诚实写法是「没有等待」**：本族的病根是「**判定依赖等了多久**」。在一处**没有异步生产者**的代码路径上，任何 `sleep(N)`/`for _ in 0..N` 都只能**把真绿变成假绿的来源**，不能提高判别力 —— 所以替换品是**可证伪的前提断言**（若闸门被放行，**是这条断言先红**，且**在 0.07s 内红**，见 §5 窗口 A），而不是把 300ms 调大、重试到过为止、或放宽阈值（这三件本单一件都没做）。
- **不是空转**（反空转论证）：同一个用例的 (2)/(4) 用**同一套 `rows()` 读数**在计划非空时**确实看到了行**（(2) 见 1 行、`ok`；(4) 见 2 行、第二行 `failed`）⇒ 「计划空 ⇒ 无行」不是「仪器坏了」，而是**这条仪器两向都有读数**。

### 1.3 C47（前置红证）

**无前置红证 ⇒ 0/N 绿是零信息**：我**没有**任何证据表明旧的「sleep + 否定断言」形状在任何负载形状下红过（t157 §4 已把 B1 记为 C47=无）。⇒ 本单**不**声称「修好了一条会红的 flake」；我声称的是：**假绿通道被一条可证伪的前提断言替换掉了**，并用 §5 窗口 A 给出**新仪器的确定性红证**。

---

## 2. B2 —— `wait_for_rows` 耗尽后静默返回

- 坐标：`chat::t8_tests::wait_for_rows`（`chat.rs`；同文件内 3 个调用点，**全部**是「拿到值就立刻断言长度/内容」）。
- 改法：循环内每次读 `rows(db)`；**耗尽即 `panic!`**，消息**点名在等什么**（「one per distillation pass that RAN」）、**要几行**、**实际几行**、以及两种非零成因的区别（0 = 没有任何 pass 跑完；少于 want = 跑的 pass 数不够）——**同 t149 的形状**（把「读到的值」与「超时」分开）。
- **它和 B1 是否共用同一份 helper？——不共用，且本单让它们更不可能混用**：
  - B1 的两处否定性断言**不再调用任何等待 helper**（走前提断言）；
  - `wait_for_rows` 只服务**肯定性**等待（等行出现），其 3 个调用点都在等一个**必然会发生**的事。
  ⇒ 同族纪律（形状相同 ≠ 仪器共享）在这里的结论是：**不共享**。若将来有人想让 B1 复用它，必须先回答「空计划下要等的那个行由谁写」——**答案是没有**，所以复用不成立。
- **边界论证（3s 上界 vs 被测对象的持续时间）**：该行由 `auto_distill_now` 在 pass **结束时**写入；调用点驱动的 pass 要么是**确定性 rules 层**（微秒级），要么是**连命令都 spawn 不起来的 ACP 尝试**（立即以 `failed` 落行）——两者都比 60×50ms = 3s 低**两三个数量级**；而「pass 挂住了」（这条仪器必须揭发的失败）**没有上界** ⇒ 3s 是上界而非判定，退出条件是**行数这个事实**。
- **C48**：它红过吗？**没有**（t157 记为 C47=无）。所以这里不是「修一条红」，而是**把两条不同的故障从同一条消息里分开**：旧消息「len 0」在「没跑完」与「跑了但没写」之间不可判 —— 后者是**另一个 owner** 的缺陷。

---

## 3. B3 —— `store/tests/ledger_reopen.rs` 的 `TempDir::drop` 20 次重试后无声

- 改法：把最终失败**留成耐久痕迹** —— 在**那个删不掉的目录里**写 `LEAKED-ROOT.txt`（内容含 root 路径、最后一次错误、以及「这个 marker 本身就是修法」的说明），**并** `eprintln!` 一行（`--nocapture` 时可见）。
  ⇒ 本代反复抱怨的「**root 泄漏**」从此**可诊断**：泄漏目录里会有一份写着原因的文件，而不是只剩一个空目录。
- **为什么不能直接 `panic!`（理由写进代码注释）**：
  1. 它在 **`Drop`** 里：若线程**已经在因断言失败而 unwind**，`Drop` 中的 panic 会 **abort 整个测试二进制** —— 一个泄漏的 root 会**连带干掉同进程里的其它所有测试**，把「可诊断的残留」升级成「不可诊断的崩溃」；
  2. 清理失败**不是这条测试要证明的事**，让它把一条本来通过的测试判红，是把注意力从被测对象上引开。
  ⇒ 因此选择**耐久 marker（跨进程存在）+ stderr 一行**，而不是断言/panic。
- **确定性机制证据（不是「它变绿了」）**：新增 `#[cfg(windows)] #[test] fn a_root_that_cannot_be_removed_leaves_a_marker_instead_of_silence` —— 用 `OpenOptionsExt::share_mode(0)` **独占**打开 root 里的一个文件（真的共享冲突，**不是 sleep**）：Windows 下 `remove_dir_all` 必然失败 ⇒ 断言 marker **存在**且**内容点名失败**。
  - **实测**（focused）：`t98 LEAKED ROOT after 20 attempts: …ruagent-t98-leak-marker-17912 (last error: 另一个程序正在使用此文件，进程无法访问。(os error 32)); marker: …\LEAKED-ROOT.txt` ⇒ `4 passed`。
  - **红证（§5 窗口 B）**：把 marker 路径换成一个写不进去的占位（模拟「静默」的修前行为）⇒ 该测试 **exit 101**，panic 在 `assert!(marker.exists(), …)`，消息里带它找的**具体路径**。
  - **为什么只在 Windows 上断言这条**（诚实标注）：Unix 下打开的文件**不阻止** `remove_dir_all`，这条失败分支在那边压不出来；**成功分支两边都跑**（本文件其余测试每个都在走它）。⇒ 这是「**平台能力**限制」，不是「为了绿而挑平台」。

---

## 4. 边界论证汇总（review 的 5b：不许拿别人的 bound/读数当依据）

| 处 | 上界 | 该被测对象的持续时间 | 为什么上界成立 | 依据来源 |
| --- | --- | --- | --- | --- |
| B1 | **无等待**（删掉两处 300ms） | 生产者**不存在**（空计划 ⇒ `maybe_auto_distill` 在 spawn 前返回） | 无等待可论证，故改为前提断言；判别力由「前提红 ⇒ 用例红」保证 | 自读字节：`maybe_auto_distill` 的 `if plan.is_empty() { return; }` 在 `tokio::spawn` **之前** |
| B2 | 60×50ms = **3s** | rules 层 pass = 微秒级；ACP 层 pass = spawn 失败、立即落 `failed` 行 | 3s 比两者高 2~3 个数量级；挂住的 pass 无上界 ⇒ 3s 是上界不是判定 | 自读字节：调用点只驱动这两层（(2) rules、(4) 命令不存在的 ACP） |
| B3 | 20×50ms = **~1s** | Windows 上「写者线程在最后一个 `Db` 句柄 drop 后关闭文件」的延迟 | 1s 是「句柄关闭」这个已知短暂事件的**安全余量**；耗尽也只**留痕**、不判红 | 自读字节 + 本单实测（owner-drop ⇒ `os error 32`） |

**没有引用任何别人的 bound 或读数**：上表三行的依据都指向本单读到的字节或本单实测输出；t157 的「形状分类」只用来**定位**要改哪几行，没有被当作「这段代码正确/错误」的证据。

---

## 5. 红证（两个窗口，均已按字节恢复 + 按 C49 刷新 mtime）

| | 窗口 A | 窗口 B |
| --- | --- | --- |
| 文件 / 性质 | `crates/daemon/src/chat.rs`（**非 test 路径**：`ChatManager::unattended_plan`） | `crates/store/tests/ledger_reopen.rs`（**测试文件**：marker 路径） |
| 单行变异 | `ExtractPlan::unattended(&plane, policy_auto)` → `(…, true)` | `self.0.join("LEAKED-ROOT.txt")` → `PathBuf::from("<silent>")` |
| 开窗前 sha256 | `3CB995BE…` | `844BBD76…` |
| 读数 | **exit=101**，`panicked at chat.rs:2633`：`the default plane must resolve to an EMPTY plan -- that is what makes 'nothing ran' decidable without a clock`，**0.07s** | **exit=101**，`panicked at ledger_reopen.rs:238`：`a root that could not be removed must leave a durable marker, or the leak is silent again (looked for …\LEAKED-ROOT.txt)` |
| 关窗 | `18:47:43 → 18:48:00`（**17 秒**） | 同窗口 |
| 恢复后 sha256 | `3CB995BE…` ✓ 逐字相同 | `844BBD76…` ✓ 逐字相同 |

**共享树零变异**：两处都在同一宣告窗口内变异，窗口**先宣告**（含两个锚与非 test 路径），**17 秒内关闭**，按字节恢复并**刷新 mtime**（C49：`Copy-Item` 会把旧 mtime 带回 ⇒ cargo 会继续跑变异二进制），随后**才**跑门禁 ⇒ 门禁读的是**恢复后的字节**（sha256 与锚一致）。

---

## 6. 门禁（最终字节，退出码在任何管道之前捕获）

| 命令 | 读数 |
| --- | --- |
| `scripts/cargo-team.ps1 test -p ruagent-daemon` | **exit=0**，`test result: ok.` × **11**，`FAILED` × 0，`panicked at` × 0 |
| `scripts/cargo-team.ps1 test -p ruagent-store` | **exit=0**，`FAILED` × 0（`ledger_reopen` 目标 **4 passed**，含新增的 marker 测试；其余目标亦绿） |
| `scripts/cargo-team.ps1 clippy -p ruagent-daemon -p ruagent-store --all-targets -DenyWarnings` | **exit=0**，`^error` × 0 |
| `cargo fmt --all --check` | **exit=0**，零 `Diff in` |

---

## 7. 未覆盖（明写，不静默）

- **B4/B5/B6 与全部 `poll_*` helper：不改**。理由（t157 §2/§4 的分类，本轮复核未变）：它们的退出条件是**可观测事实**、耗尽**响亮报错**，属 **①**；本族的病根是「判定依赖预算」，它们不具此性质 ⇒ 动它们只会把「形状相同」误当成「仪器共享」。**本轮没有发现其中某条其实是 ② 类**（这是任务单要求的复核）。
- **`#[ignore]`**：不由本单覆盖；且 t157 的计数（`-F` 后 **24**）与 t144 的「**16 条**」**不一致**，差 8 条未核对（t157 表 C2 已登记，本单不重复也不假装解决）。
- **`panel/e2e/**`（`waitForTimeout` 19）与 `scripts/**`（`Start-Sleep` 3）**：out of scope，**一行未读**。
- **产品代码里的等待**（`chat.rs` 的 `MODELS_WAIT` 系列、`spawn_idle_reaper` 的 `sleep(60s)`）：**本单不动**；若将来要为「切换是否生效」立判据，那里应先补可观测终态（t157 §7③ 已点名）。
- **`crates/daemon/tests/**`、`panel/**`、`.github/**`、`scripts/**`**：零改动。**活守护进程（8787 = pid 14944）**：未启停、未写其库。

## 8. 本单涉及的两条既有纪律的现场印证

- **C49（恢复后刷 mtime）**：本单在关窗后**先** `(Get-Item …).LastWriteTime = Get-Date`（两个文件），**再**跑门禁 ⇒ 门禁确实重编了恢复后的字节（sha256 与锚一致，读数一致）。**这正是 t154 那次「幻影红」的对照组**。
- **符号优先、改了行号就重取坐标**：本单据此**更正了 t157 里 B1 的模块名**（`generating_tests` → **`t8_tests`**）—— 行号会漂、模块名不会，但**模块名也会被我记错**，所以最终以 panic 路径为准并在此更正。
