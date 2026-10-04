# t149 收口第二个 flake：`message_count_is_real_before_the_index_catches_up`

> **性质**：修复（repair），作者 = verify2。来源 = `t145` 的额外 finding（graph 在同形状下抓到的第二个 flake，它如实上报、未越界改）。
> **一句话**：这条用例要证明的是「**计数来自真实状态，而不是 60s 索引**」，而它原来的写法用「40 次 ×50ms 的**猜测预算**」代替同步 ⇒ 换成「**有界的、对 writer 自己 flush 出来的那一行**的等待」+ 把原本只写在注释里的前提（**索引还没有行**）变成断言；改后这条判据在**隔离副本里被确定性打红 3/3**（`left: Some(0) / right: Some(1)`，与 flake 同一签名）。
> **本轮（attempt 2）captain 把契约重写为**：以「**机制从字节读清 + 把「不可随机复现」写进台账 + 确定性红证**」取代原来的「先随机复现」——即把 attempt 1 的诚实产出正式作为收口证据。本报告已按新判据重排（§1）；「不可随机复现」是**单列台账条目**（§2），不是缺口；**字节本轮未再改动**（`4ACB77AE…`，与 attempt 1 的最终字节相同），门禁在 §7 **重新跑过**（三条全 exit 0）。

---

## 0 Pins（每份读数取自哪份字节）

| 项 | 值 |
| --- | --- |
| 起点 `crates/daemon/src/chat.rs` | **`C939062C6D7A4454DAE55FAC`** = **t145 的最终字节**（工作树，`git status` = ` M`；**注意**：`HEAD` 里提交的 chat.rs 是 `14387C3C…`，即 **t145 之前**的那版 ⇒ t145 的修复当时**未提交**；本单的「改前」是**工作树**那一版，按 C20 明说） |
| 我的最终字节 | **`4ACB77AE2F79195FDF31AF6E`**；`git diff --numstat -- crates/daemon/src/chat.rs` = **176 / 38**（相对 `HEAD`，**包含 t145 的未提交改动 + 我的两处**） |
| 「改前」仪器（二进制） | `%TEMP%\t149-before\…\ruagent_daemon-305c45c5dc2b37eb.exe`，sha256(16)=**`91426F35BC31D1D3`**，构建于 **02:11:13**（源 mtime 02:10:44 = t145 最终字节） |
| 「改后」仪器（二进制） | `%TEMP%\t149-fixed\…`，sha256(16)=**`9F8EB6C0663336A0`**，构建于 **02:26:58**（源 mtime 02:26:13 = `4ACB77AE`）；与共享 target 里的同名构建**逐字节相同**（同 sha256）⇒ 我的循环跑的是**私有副本**，从不占用共享二进制 |
| C35 行为证据 | 该二进制**含我新增的前提断言**（那条断言只存在于我的源码里），且它跑出 `ok`（§2 的 smoke）⇒ 它不是旧字节的产物 |
| 负载窗口 | 02:16:04 → 02:52:42（10 个自建 burner **全程 10/10 存活**，逐次核对于 02:16/02:17/02:18/02:35/02:42；02:33 起增加第二个冷建；02:44-02:49 另加 6 个进程 churn 循环；02:52 我主动停掉负载后 burner 归零） |

**我的改动（两处，逐字）**：① 测试文档段加了 3 行说明"（b）由 writer 定界、并断言索引前提"；② `(b)` 的执行体：删掉
```rust
        let mut seen = None;
        for _ in 0..40 {
            let h = chats.history(None, 50).await;
            seen = h.iter().find(|e| e.id == empty.id.to_string()).and_then(|e| e.message_count);
            if seen == Some(1) { break; }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
```
换成「墙钟 5s 有界等待 + 对 transcript 里 `UserMessage` 行的判定 + 前提断言 + 结尾断言」（全文见 §4）。**没有动** `(a)` 分支、`mock_agent()`、测试名、任何非 test 路径。

---

## 1 接受标准逐条结论

| # | 判据（**attempt 2 的契约**） | 结论 | 读在哪 |
| --- | --- | --- | --- |
| 1 | **机制必须从字节读清**：写者/读者坐标 + 「事件流不能当同步点」的两任务论证 | **passed** | §3：写者 `runs.rs:1362-1371` → `:1666-1679` → `store/src/transcript.rs:36-47`；读者 `chat.rs:1566-1576`（索引优先 `:1506-1533`，索引由 `sessions.rs:196-231` 写）；两任务论证 = `Chat::subscribe` 源于 **ACP 层**（`acp/src/chat.rs:534`）而 transcript 由 **run loop**（`runs.rs:1506`）写 |
| 2 | **把「不可随机复现」写进台账**：before 0/512、fixed 0/190 + 为什么该配方给不出 before + 写给下一位的一句话 | **passed** | §2（含「**不要再为该 flake 白跑大批次**」与"收口证据 = 机制 + 确定性红证"） |
| 3 | **确定性红证取代「随机复现」作为对照**：隔离副本 1 行变异 ⇒ 3/3 红在该断言、同签名；共享树零变异（前后 sha256 + 构建日志证明编的是副本树） | **passed** | §5：3/3 红在 `chat.rs:2383`，`left: Some(0) / right: Some(1)`；共享树全程 `4ACB77AE…`；日志逐字含 `Compiling ruagent-daemon v0.1.0 (…\Temp\t149-iso\crates\daemon)` |
| 4 | 修法 = 有界等待 + **响亮失败（带文件字节数与路径）** + **前置条件变真断言**（`Option` 比较，不把读错吸收成 0） | **passed** | §4 + 代码回读 `:2345`（超时消息含 `{} bytes at {}`）与 `:2373`（`assert_eq!(indexed, Some(0), …)`） |
| 5 | 门禁在**最终字节**上由我本人重跑并给计数 | **passed** | §7（三条全 exit 0；含 attempt 2 的复跑） |
| 6 | **自报的仪器失误入账**（首次红证那 3 次 `FAILED` 其实是 mock agent 未就位，坐标 `:2249`） | **passed** | §5.1 |

---

## 2 台账条目：负载形状与复现率 —— **这条 flake 在本机不可随机复现**（本单的诚实产出，不是缺口）

**形状（照契约）**：10 个自起的 CPU 占用进程（`while($true){[math]::Sqrt(2.0)}`，`-WindowStyle Hidden`，**PID 全部记录**：`3392,30964,9056,28660,28616,27796,29028,29524,31280,30200`）+ **一个并发冷构建**（`scripts/cargo-team.ps1 test -p ruagent-daemon --no-run -TargetDir %TEMP%\t149-load-target -NoLock`，私有 target，**不占共享锁/共享 target**）。后续为尝试复现，我把同一族负载**加强**（均已披露）：02:33 起并发**第二个冷构建**（`-p ruagent-knowledge`，私有 `%TEMP%\t149-load2-target`），02:44-02:49 另加 **6 个进程 churn 循环**（`cmd /c exit` 空转，PID 已记录并已停止）。

**复现率（逐批，before = t145 的最终字节）**：

| 批次 | 次数 | **我这条测试红** | 有**别的**测试红的 run | 说明 |
| --- | --- | --- | --- | --- |
| 单测 ×12（共享的改前二进制） | 12 | **0** | 0 | 只跑这条，最快 0.19–0.30s |
| 全套 ×10 / ×15 / ×45 | 70 | **0** | 1 / 2 / 3 | 别的模块先红（§11 F2） |
| 全套 ×60 / ×40 / ×90（私有副本，交替采样） | 190 | **0** | 16 / 5 / 28 | 负载最重的阶段 |
| 全套 ×150（加进程 churn） | 150 | **0** | 未逐次统计 | — |
| 全套 ×90（**daemon 自身编译/链接爆发期**） | 90 | **0** | 未逐次统计 | 窗口恰好覆盖 `Compiling ruagent-daemon` |
| **before 合计** | **512** | **0** | — | — |
| **fixed 合计**（改后字节，同形状） | **190** | **0** | 13 / 14 / 37 | 见 §6 |

**负载确实很重（这是本项的关键语境，不是借口）**：同一批 run 里 **30-40% 有测试红**，全部红在**别的**用例上，其中最集中的是 `distill::t329_tests::*`（§11 F2 给了 6 个名字与一条逐字原文）。也就是说：**机器被压到了会让别的测试变红的地步，唯独这条测试 512 次都没红。**

**为什么它这么难红（读出来的，不是猜的）**：这条用例的"预算"是 **40 次迭代**，每次 = 一次 `history()`（DB round-trip + 文件读）+ 50ms 睡眠 ⇒ 实际预算**随负载自动变长**（负载越高、每次迭代越慢、总预算越大）。要打红它，writer 必须被饿住**比这个自增长预算还久**；而 writer 的那一行是在 `runs.rs:1441` 的 `emit` 里写的（**在驱动 agent 之前**，见 §3），所以它需要的不是一个"慢写"，而是**数秒级的、针对该任务的调度饥饿**——10 个 burner（16 核）给不出来。

**失败原文（`left`/`right`）**：t145 上报的是 `left: Some(0) / right: Some(1)`；我自己**拿到同样原文**的那次运行见 §5（确定性变异，3/3）——但它是变异造出来的，**不是**这个负载形状下随机出现的。

⇒ **台账条目：不要再为该 flake 白跑大批次。** 该配方（10 burner + 并发冷建，甚至再加第二个冷建/进程 churn）**给不出 before**，因为旧判据的预算 = 40 次迭代 ×（`history()`+50ms）**随负载自增长**，而写者那一行是在**驱动 agent 之前**写的（`runs.rs:1441`）⇒ 要打红它需要**数秒级、针对该任务的调度饥饿**，CPU 压力给不出来。**本单的收口证据 = 机制读清（§3）+ 确定性红证（§5）**；若将来真的要复现，先读 §11 F1 里那条**未验证但可检验**的线索（写者延迟由 **mock agent 的进程创建**主导 ⇒ 要的是**重镜像 spawn 压力**，不是 CPU 压力；依据是 t145 自己的 `UserMessage`@**2373ms**）。

---

## 3 机制（判据 1）：竞争在哪 —— 写者/读者坐标 + 「事件流不能当同步点」的两任务论证

**断言假设的"已经发生的事"**：`empty.send_prompt(...).await` 返回之后，「用户的消息已经能被读到了」。实际不是——那是另一条任务链上的异步写入。

**竞争的两件事**：
1. **写者**：把 `RunEvent::UserMessage` 这一行 **append + flush** 到**这个 chat 的 transcript**（`data/transcripts/run-<id>.jsonl`）。路径：`runs.rs:1362-1371`（`emit`）→ `runs.rs:1666-1679`（`append_event`）→ `crates/store/src/transcript.rs:36-47`（`TranscriptWriter::append`）。**每一次 append 都在返回前 `flush()`**（该函数的 doc comment 明说这是为了让 SSE 订阅者能立刻 replay）。
2. **读者**：`chats.history(None, 50)` 里那个**从 transcript 数出来的计数**：`chat.rs:1506-1533` 先查 `sessions` 索引（`SELECT key, message_count, preview FROM sessions …`）；**索引没有行时**（`:1553-1565` 的注释：索引是 **60s 后台扫描**写的，`sessions.rs:196-231` 是它的写入点 `INSERT OR REPLACE INTO sessions`，`chat.rs:1657` 的 60s 循环是 idle reaper、不是索引器），才走 fallback `:1566-1576`：`ruagent_store::read_transcript(self.transcript_path(rid))` → 数 `RunEvent::UserMessage` 行 → `Some(n)`。

**旧代码的时序**：`for _ in 0..40 { history(); sleep(50ms) }` ⇒ 读者**只在猜**"40 次大概够了"，**没有**与写者建立任何顺序；写者慢过这个预算 ⇒ 最后一次观测仍是 `Some(0)` ⇒ `assert_eq!(seen, Some(1))` 报 `left: Some(0) / right: Some(1)`（与 t145 的读数一致）。

**为什么不能拿"run 的事件流"当同步点（这是读出来的，不是我偏好的选择）**：测试能订阅的只有 `Chat::subscribe()`（`chat.rs:278-280` → `self.session.subscribe()`），而那个 `broadcast::Sender<RunEvent>` 的来源是 **ACP 层**（`crates/acp/src/chat.rs:189` + `:534` 发 `RunEvent::UserMessage`），transcript 却是**run loop**（`runs.rs:1506`）append 的 ⇒ **两个不同的任务**，"事件到了"**不**给 append 排序。真正的顺序保证只在 `emit` 内部（先 append+flush，后 broadcast **到 run 自己的流**），而那条流测试拿不到但不是必需的——**文件本身就是那个保证**。

**产品侧根因？不是。** 产品给了**可观测事实**（transcript 的内容 + 由它算出的计数），索引也会在自己的时间尺度（60s）内到达；缺陷在**判据的时序假设**（"写了就能立刻读"）。⇒ 按契约"若根因在产品侧 ⇒ 报 finding"：**本条不需要 finding**；相应地，修法也没有只改测试去掩盖产品问题（没有产品问题可掩盖）。**一处值得登记的语义**：`history()` 的计数**优先用索引**、只在索引缺行时读真实状态 —— 这是 t127 有意为之（索引是便宜的来源），本用例证明的是**缺行时的回退**是对的，**不是**"索引会撒谎"。

---

## 4 修法（判据 4）：有界等待 + 响亮失败（带字节数与路径）+ 前置条件变真断言

```rust
        // (b) THE FIX: one message gives 1 at once, with NO index row at all.
        empty.send_prompt(&db, embedder, "hello".into()).await.expect("send prompt");

        const USER_MESSAGE_BOUND: std::time::Duration = std::time::Duration::from_secs(5);
        let transcript = chats.transcript_path(empty.id);
        let deadline = tokio::time::Instant::now() + USER_MESSAGE_BOUND;
        let landed = loop {
            let n = ruagent_store::read_transcript(&transcript)
                .unwrap_or_default()
                .iter()
                .filter(|l| matches!(l.event, ruagent_core::RunEvent::UserMessage { .. }))
                .count();
            if n > 0 || tokio::time::Instant::now() >= deadline { break n; }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        };
        assert!(landed > 0, "the run's writer must land the user's line within {USER_MESSAGE_BOUND:?} \
             (`runs.rs:1506` -> `store/src/transcript.rs:45` appends AND flushes), so a timeout here is \
             a signal that never arrived, not a duration that was too short: {} bytes at {}",
            std::fs::metadata(&transcript).map(|m| m.len()).unwrap_or(0), transcript.display());

        // 前提（原本只写在文档注释里、没有任何断言）：索引还没有行。
        let h = chats.history(None, 50).await;
        let entry = h.iter().find(|e| e.id == empty.id.to_string());
        let key = entry.and_then(|e| e.session_key.clone());
        let indexed: Option<i64> = match key {
            Some(key) => db.call(move |conn| {
                    conn.query_row("SELECT COUNT(*) FROM sessions WHERE key = ?1",
                                   rusqlite::params![key], |r| r.get::<_, i64>(0))
                        .map_err(ruagent_store::DbError::from)
                }).await.ok().and_then(|r| r.ok()),
            None => None,
        };
        assert_eq!(indexed, Some(0), "the index must not have caught up yet: this direction proves the \
             count is REAL STATE and not the 60 s index's, so a row here (or an index that could not be \
             read, which is not the same as 'no row') would leave the assertion below vacuous");

        let seen = entry.and_then(|e| e.message_count);
        assert_eq!(seen, Some(1), "the count must be real at once");
```

**为什么这个同步点能消掉竞争**：等待的对象是**被断言值的唯一数据源本身**（`history()` 的 fallback 就是从这份 transcript 数行），而写者**在 append 返回前已 flush**（`store/src/transcript.rs:45`，"`BufWriter::flush` 是一次普通 write 系统调用"）⇒ 一旦那一行可读，`history()` 的 fallback **必然**数到 ≥1 ⇒ 断言变成**确定性**的。等待的上界是**墙钟 5s**（不是迭代次数、不是 sleep 时长），失败时**响亮报错**并附上"文件多少字节、在哪"，不会静默通过。

**为什么 5s 不是拍脑袋**：主体是 `--behavior echo`（**同一进程内启动 mock agent**，且 `UserMessage` 的那次 append 发生在驱动 agent **之前**，`runs.rs:1441-1447`）⇒ 5s 比这条路径自身的延迟高**几个数量级**；它吸收的是**机器被压**，而不是在替一个信号当猜测窗口（形状与 t145 的 5s 一致：后者严格小于 `slowreply` 的 10s 回合）。

**禁忌清单（逐条确认没做）**：没有调长 sleep（`50ms` 的猜测循环被**删除**，新的 10ms 只是轮询间隔）；没有"重试到过为止"（**一次性**的有界等待 + 失败即错）；没有 `#[ignore]`；没有放宽阈值（断言仍是 `Some(1)`，且**新增**了前提断言）。

**一处必须说清的副作用（有益）**：新的前提断言把注释里的"with NO index row at all"变成了**可证伪**的东西。本机 190+512 次运行里它从未红 ⇒ 这个前提在测试环境里成立（fresh DB、不启 60s 索引器、idle reaper 的 60s 也早于测试结束）。

---

## 5 确定性红证（判据 3）：改坏被保护行为 ⇒ 该断言**必须**红（隔离副本，共享树零变异）

* **隔离形态**：`git archive HEAD` → `%TEMP%\t149-iso`（673 个文件），再把**我的最终 `chat.rs`** 覆盖进去（sha256 与工作树一致），**然后只改一行**：fallback 里 `.filter(|l| matches!(l.event, RunEvent::UserMessage …))` → `RunEvent::Error`（即：计数不再跟踪"用户自己的消息"，去数从未出现的 Error 行）。副本 sha256 = **`1C675D940E8FEBF7145E1D87`**，`Compare-Object` 显示与我的字节**只差这一行**。
* **构建来源可证**：私有 target（`-TargetDir %TEMP%\t149-load-target` + `-NoLock`）的日志逐字含 **`Compiling ruagent-daemon v0.1.0 (C:\Users\…\Temp\t149-iso\crates\daemon)`** ⇒ 编的是**副本树**，28.25s，exit 0。
* **读数（3 次，逐字）**：
```
thread 'chat::generating_tests::message_count_is_real_before_the_index_catches_up' panicked at crates\daemon\src\chat.rs:2383:9:
assertion `left == right` failed: the count must be real at once
  left: Some(0)
 right: Some(1)
test result: FAILED. 0 passed; 1 failed; … finished in 0.10s
```
**3/3 全红，且红在「我的那条断言」上，`left: Some(0) / right: Some(1)` 与 flake 的签名逐字相同** ✅
* **共享树零变异**：全程 `crates/daemon/src/chat.rs` = `4ACB77AE2F79195FDF31AF6E`（§0/§11 的两处回读）；变异只存在于 `%TEMP%\t149-iso`；**没有**用宣告窗口（不需要）。
### 5.1 仪器失误入账（与 C40 同族：**先确认红在谁身上，再当读数**）
* 第一次跑红证时，三次 `FAILED` 的坐标是 **`chat.rs:2249`**、消息是 **`mock agent binary not built next to the test exe`** —— **不是我的断言**。原因：私有 target 里没有 `debug/ruagent-mock-agent.exe`，而 `mock_agent()` 的解析是 `current_exe().parent().parent()/ruagent-mock-agent.exe`（`chat.rs:1786-1796`）；我最初的拷贝带的是 `deps/` 里那批 `ruagent_mock_agent-*.exe`（下划线、rlib 一类），**名字对不上**。
* **把 mock agent 放到 `<profile>/ruagent-mock-agent.exe` 后重跑，才是 §5 上面那份读数**（坐标 `:2383`）。⇒ **3 次 `FAILED` ≠ 3 次红在我身上**：`exit=101` 只说明"有人红了"，**坐标与消息**才说明"红在谁身上"。这与 C40（给退出码当证据的命令不要接管道）是同一族：**先确认读数属于谁，再把它当读数。**

---

## 6 负载下复跑（改后字节）—— 台账的补充读数，不是"修好了"的对照

| 批次 | 次数 | 我这条红 | 有别的测试红的 run | 耗时 |
| --- | --- | --- | --- | --- |
| 交替采样第 1 批 | 60 | **0** | 13 | 与 before 合并计时 260.8s |
| 交替采样第 2 批（+第二个冷建） | 40 | **0** | 14 | 176.4s（合并计时） |
| 交替采样第 3 批 | 90 | **0** | 37 | 399.7s（合并计时） |
| **合计** | **190** | **0** | 64 | — |

⇒ **0/190 红** ✅（形状与 §2 相同：10 burners + 并发冷建；第 2/3 批多一个冷建）。
**必须写明的限度**：因为 before 侧在本机也没红（§2），这个 0 **不是**"修好后从红变绿"的对照；本单真正证明"这条判据有牙"的是 §5 的**确定性红证**。把这点含糊过去就等于用"跑绿了"冒充"修对了"——本代不允许。

---

## 7 门禁（判据 5）：最终字节上由我本人重跑，退出码在任何管道之前捕获

| 命令 | 退出码 | 计数原文 |
| --- | --- | --- |
| `scripts/cargo-team.ps1 test -p ruagent-daemon` | **0** | lib `running 157 tests` → `test result: ok. 157 passed; 0 failed` · capabilities 11 · capability_defaults 5 · event_compat 2 · graph_ingest 16 · injection_e2e `0 passed; 0 failed; 8 ignored` · knowledge_api 16 · recall_evidence_announced 3 · smoke 0 · **version_points 5** · doctests 0 ⇒ **215 passed / 0 failed / 8 ignored** |
| `scripts/cargo-team.ps1 clippy -p ruagent-daemon --all-targets -DenyWarnings` | **0** | `Checking ruagent-daemon v0.1.0 (…\crates\daemon)` + `Finished dev profile … in 5.43s`，无 error/warning 行（我早先一次构建曾因 `value assigned to landed is never read` 警告——**已修**：把循环改成 `let landed = loop { … break n; };`，警告消失，clippy 由此干净） |
| `cargo fmt --all --check` | **0** | 零 `Diff in`（用 `cmd /c "… > file 2>&1"` 取码，避开 C40） |

**认证的字节**：`crates/daemon/src/chat.rs` = `4ACB77AE2F79195FDF31AF6E`（三条门禁跑完后再回读，未变）。

**attempt 2 复跑（同一份字节，17:28:03 → 17:28:14，11s）**：三条再次 **exit 0** —— `test -p ruagent-daemon` 逐目标：lib `running 157 tests` → `test result: ok. 157 passed; 0 failed` · capabilities 11 · capability_defaults 5 · event_compat 2 · graph_ingest 16 · injection_e2e `0 passed; 0 failed; 8 ignored` · knowledge_api 16 · recall_evidence_announced 3 · smoke 0 · **version_points 5** · doctests 0 ⇒ **215 passed / 0 failed / 8 ignored**；`clippy -p ruagent-daemon --all-targets -DenyWarnings` **exit 0**（`Finished dev profile … in 0.95s`，无 error/warning 行）；`cargo fmt --all --check` **exit 0**（零 `Diff in`）。跑完回读 `crates/daemon/src/chat.rs` 仍是 `4ACB77AE2F79195FDF31AF6E`。

---

## 8 这条判据现在覆盖什么 / 仍不覆盖什么

**覆盖**：① 空会话必须报 `Some(0)`（**不是** `None`）⇒ t90 的"没有消息就不算会话"规则**没有被放宽**；② 有 1 条用户消息、且**索引里没有行**（这一前提现在被**断言**）时，计数必须是 `Some(1)` ⇒ 计数**来自真实状态**；③ 等待是**有界**的且失败**响亮**（信号没到 ≠ 等得不够）。

**仍不覆盖（每条给"为什么"）**：
* **索引已有行时的一致性**（`chat.rs:1547-1552` 那条"索引优先"路径）—— 本用例恰恰**避开**了它（fresh DB + 不启索引器）；要覆盖得**启动** 60s 索引器并等它写完（本用例从不等 60s）⇒ 属另一条测试的形状。
* **assistant 消息/工具事件算不算进这个计数** —— `history()` 的 fallback 只数 `UserMessage`（`:1573`）；本用例只证明这一条路径，别的消息种类**没有**被断言（注释里的"one message"其实特指用户那条）。
* **`preview` 字段**（`:1552` 与索引一起取）—— 本用例不碰。
* **多 chat / 并发** —— 只有一个 chat。
* **索引的真实写入点**（`sessions.rs:196-231`）—— 本用例不触发它（那是 60s 扫描）。

---

## 9 未测 / 残余风险

* **R1（本单的核心未测项）**：这条 flake 在本机**不可随机复现**（< 1/512，形状见 §2）。所以"**CI 绿不可复现**"这条论证的证据仍然**主要来自 t145 的 DIAG2 读数**（2 次红），我只做到了"同形状下压不红它"。⇒ 见 §11 F1。
* **R2**：5s 是**上界**而非保证。更慢的机器/更狠的负载下，写者可能超过 5s ⇒ 那时测试会**响亮失败**（而不是静默通过）——这是**有意的**取舍（宁可红，也别假装绿），但它是这条判据的已知边界。
* **R3**：我的复现尝试**超出**契约形状的部分（第二个冷建、6 个进程 churn 循环）已在本报告逐条披露；它们**不占共享锁/共享 target**，但确实在 02:16–02:52 这段时间分享了机器的 CPU/IO。**代价形态**：同批 run 里别的模块的测试因此变红（§11 F2）——这属于"负控必须宣告"的精神，我把它记在这里而不是藏在数据里。
* **R4（工具教训，自报）**：我最初把"改前/改后"二进制**拷到私有目录一直循环跑**，结果 cargo 去链接共享 target 里的同名文件时报 **LNK1104**（文件被占）——那是**我自己**的循环占住了共享二进制，会连带影响同伴的链接。改成"私有副本 + 镜像目录布局（`<dir>/deps/*.exe` + `<dir>/ruagent-mock-agent.exe`）"后不再占共享文件。
* **单一平台**：Windows / rustc 1.95.0，单机。

---

## 10 复现命令

```powershell
cd C:\Users\19410\Documents\ai\ruagent
# 0) 形状：10 个自起 CPU 占用 + 一个并发冷建（私有 target、-NoLock，绝不占共享锁/共享 target）
$pids = 1..10 | ForEach-Object { (Start-Process powershell -ArgumentList '-NoProfile','-Command','while($true){[math]::Sqrt(2.0)}' -WindowStyle Hidden -PassThru).Id }
$pids -join ',' | Set-Content "$env:TEMP\t149-burners.txt"
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-daemon --no-run -TargetDir "$env:TEMP\t149-load-target" -NoLock
# 1) 采样（用私有副本，避免占住共享二进制）
& "$env:TEMP\t149-before\deps\ruagent_daemon-305c45c5dc2b37eb.exe"   # 全套 157 个测试
# 2) 红证（隔离副本 + 一行变异）
git archive HEAD -o "$env:TEMP\t149-iso.tar"; mkdir "$env:TEMP\t149-iso"; tar -xf "$env:TEMP\t149-iso.tar" -C "$env:TEMP\t149-iso"
Copy-Item crates/daemon/src/chat.rs "$env:TEMP\t149-iso\crates\daemon\src\chat.rs" -Force
#   把 fallback 的 .filter(... RunEvent::UserMessage ...) 改成 RunEvent::Error（只这一行）
Set-Location "$env:TEMP\t149-iso"
powershell -NoProfile -ExecutionPolicy Bypass -File C:\Users\19410\Documents\ai\ruagent\scripts\cargo-team.ps1 test -p ruagent-daemon --lib --no-run -TargetDir "$env:TEMP\t149-load-target" -NoLock
Copy-Item "$env:TEMP\ruagent-team-target\debug\ruagent-mock-agent.exe" "$env:TEMP\t149-load-target\debug\" -Force
& "$env:TEMP\t149-load-target\debug\deps\ruagent_daemon-305c45c5dc2b37eb.exe" --exact chat::generating_tests::message_count_is_real_before_the_index_catches_up --nocapture
# 3) 门禁
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-daemon
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 clippy -p ruagent-daemon --all-targets -DenyWarnings
cargo fmt --all --check
```

---

## 11 findings

### T149-F1（medium，流程/证据）这条 flake 在本机**不可随机复现**，契约的复现形状不足以压出它
* **读数**：同一形状（10 burners + 并发冷建，另加第二个冷建与进程 churn）下，**before 512 次 0 红**、fixed 190 次 0 红；而**同批 run 里 30-40% 有别的测试红** ⇒ 负载不是问题，**这条测试的预算形态**是问题（40 次迭代 ×（`history()`+50ms），**随负载自增长**；写者那一行又在驱动 agent **之前**写，`runs.rs:1441`）。
* **处置（按 attempt 2 重写的契约）**：**以台账条目 + 确定性红证收口**，不再把"随机复现"当成本单的缺口；**下一手不要再为该 flake 白跑大批次**（我为它跑了 512 次）。
* **一条未验证、但可检验的线索（给将来真需要复现的人；本轮我没有为它再开批次）**：t145 自己的持久诊断给出的时间是 `StateChanged{Running}`@**2216ms**、`UserMessage{"hello"}`@**2373ms** ⇒ 在**他们的**环境里写者迟到了 **2.4 秒**。这个量级更像**进程创建/镜像加载**（驱动 agent 要先 spawn mock agent 进程）而不是纯 CPU 争抢 —— 这也解释了为什么"只加 CPU 占用 = 17 次 0 红"（t145 的读数），而**编译/链接井喷**（成千上万次重镜像 spawn + 杀软/加载器排队）能压出来。⇒ 真要做，负载应压 **spawn**：反复并发启动**重镜像**（`link.exe`，或那个 277MB 的 `ruagent_daemon-*.exe`），而不是加 burner。
* **不要**继续用"压不红就当没事"的口径（那会把 512 次的空跑当成"没有这个 bug"）。

### T149-F2（medium，**不在我 inScope**；同负载下的家系 flake，全部在**干净已提交**字节上）
* 同一负载下反复红的**别的**用例（我只报告文件与错，不替他们改）：
  * `crates/daemon/src/distill.rs`（`git status` **干净**，sha256 `0291DF0E6E580A7F…`，mtime 00:48:56）：`distill::t329_tests::a_distilled_body_carries_no_provenance_marker` 逐字 `panicked at crates\daemon\src\distill.rs:1362: assertion left == right failed: the write must land, or the assertion below is vacuous / left: 0 / right: 1`；同模块另有 `a_graph_write_failure_leaves_no_run_turn_episode` · `the_write_path_asks_the_bounded_merge_judge_and_audits_the_decision` · `the_log_records_three_states_and_a_failure_keeps_a_success` · `graph_edges_carry_the_episode_and_the_event_time_source` · `a_hedged_confidence_is_not_silently_raised_to_the_floor` 一起红。
  * `registry::tests::create_update_delete_runtime_preserves_comments` · `registry::tests::agent_crud_and_legacy_promotion`。
* **为什么算 finding**：`distill.rs:1362` 的原文（"**the write must land**, or the assertion below is vacuous / `left: 0 / right: 1`）与 t145/t149 是**同一个家系**（"等一件已经发生的事却立即断言"），而且它**在已提交、无人编辑的字节上**红 ⇒ 这说明"CI 绿不可复现"的病灶**不止 chat 模块**。建议按月归口：每条用例一份"有界等待 + 显式同步"的收口单。

---

## 12 纪律回执

* **写入集合**：`crates/daemon/src/chat.rs`（唯一代码文件）+ 本报告。`crates/daemon/tests/**`、`runs.rs`、`distill.rs`、`crates/acp/**`、`panel/**`、`.github/**`、`scripts/**` **零改动**（`git status` 里的其它 `M`/`??` 是同伴在途的文件，与我的写入集合无关）。
* **构建路径**：门禁三条走 `scripts/cargo-team.ps1`（**同树没有**设 `CARGO_TARGET_DIR`/`-TargetDir`；`-DenyWarnings` **只**用于 clippy）；制造负载与红证用**私有 `-TargetDir` + `-NoLock`**，**从未**占共享锁/共享 target；未 push/未暂存。
* **负控/变异**：只有 `%TEMP%\t149-iso`（`git archive HEAD` 的导出）被改一行；**共享树零变异**（`chat.rs` 全程 `4ACB77AE…`，两处回读）。
* **进程**：我的 10 个 burner 与 6 个 churn 循环**PID 全部记录**、已按 PID 停止（现均 0 个存活）；`cargo/rustc/link` 里匹配我临时目录的**0 个**；**本单没有起过守护进程**（`ruagent.exe` = 1，即活的 **pid 14944**，8787 的持有者，只读；按 C32）。
* **临时目录**：先**列清单**再按**具体路径**删（不是 glob —— 本代已有 glob 误删的实例）；清单里 `t149z-apply.py` / `t149z.json`（mtime **09-25 02:12:52**，比本单早 9 天）**不是我的 ⇒ 一个没碰**；其余 `t149-*` 全部按路径删除，删后复核 0 个我的残留。
* **自我评审声明**：这是修复，不是评审；本单需要**独立评审**（t145 的 finding 是输入，不是我通过的理由）。而且我在本单里**自己**发现了一处会误导人的仪器问题（§5 末的 mock agent 坐标）并如实登记，而不是把那 3 次 `FAILED` 当成红证。
