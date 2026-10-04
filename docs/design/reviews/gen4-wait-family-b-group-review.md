# t167 · 独立评审 t160（三处等待收口：B1 断言前提 / B2 耗尽响亮 / B3 泄漏留痕）

**被评审对象**：t160（作者 graph）。**我读的字节 = 入库的字节**：
```
git rev-parse b6f1427:crates/daemon/src/chat.rs            = 8fafc91aae2782271ad0685508ff120fd118d2d3
git hash-object -- crates/daemon/src/chat.rs               = 8fafc91aae2782271ad0685508ff120fd118d2d3   一致 ✓
git rev-parse b6f1427:crates/store/tests/ledger_reopen.rs   = a8baa5c865cb36b98e331cc670ab40a36701e749
git hash-object -- crates/store/tests/ledger_reopen.rs      = a8baa5c865cb36b98e331cc670ab40a36701e749   一致 ✓
sha256 前缀：chat.rs 3CB995BE（124,780 B）· ledger_reopen.rs 844BBD76（9,814 B）  ⇒ 与作者申报一致
```
**写入集合（`git show --stat b6f1427`）**：只有 `crates/daemon/src/chat.rs`(+53) · `crates/store/tests/ledger_reopen.rs`(+86) · `…/gen4-wait-family-b-group-repair.md`(+111) ⇒ **`crates/daemon/tests/**`、`panel/**`、`.github/**`、`scripts/**` 在本提交里零改动** ✓（工作树里另有 `.github/workflows/{ci,e2e}.yml`、`panel/src/index.css`、`crates/memory/src/lifecycle.rs` 是**别人**的在途编辑，不是 t160 的路径）。**共享树零变异**成立（我的判据全部对着入库 blob）。

---

## 1 判据 3：三处是否真的把「猜测」换成了「事实」（纯字节）

### B1 达成 —— 两处裸 `sleep(300ms)` 消失，且新前提断言**与被测路径同源**（不是空转）
* `git show b6f1427 -- crates/daemon/src/chat.rs` 的两个 hunk（`@@ -2544 … mod t8_tests`、`@@ -2593 … mod t8_tests`）各删掉一行 `tokio::time::sleep(Duration::from_millis(300)).await;`，**没有任何 `+` 行是 sleep**。
* 当前字节里 `the_unattended_plan_decides_and_the_default_spends_nothing`（`:2584`）内只剩两处**前提断言**：`:2633-2638 assert!(manager.unattended_plan().is_empty(), "the default plane must resolve to an EMPTY plan …")` 与 `:2670 assert!(manager.unattended_plan().is_empty());`（case 3「关掉后不新增」）。
* **同源性我按字节核了，不是采信注释**：`maybe_auto_distill`（`:1151-1173`）= `let plan = self.unattended_plan();`（`:1155`）→ **`if plan.is_empty() { return; }`（`:1156-1158`）→ 之后才 `tokio::spawn`（`:1170`）**。`unattended_plan()`（`:1180-1184`）现读实时 plane。⇒ **空计划下确实没有异步生产者**，所以那两处 sleep 在等一件不可能发生的事，而后面是**否定性**断言 ⇒ 旧形状「睡不够就假绿」；现在 **「没有东西可等」由闸门保证、闸门由断言守住** ⇒ 猜测换成事实 ✓。
* **非空转**：case (2)（`:2659 wait_for_rows(&db, 1)` + `:2660 assert_eq!(got.len(), 1)` + `:2661 got[0].1 == "ok"`）用**同族的仪器**在同一用例里看到了行；case (3) 又断言行数**仍是 1** ⇒ 仪器在两个方向都灵敏，断言不是同义反复 ✓。**变异可红**：窗口 A 改的 `:1183`（`unattended(…, policy_auto)`→`(…, true)`）正好喂给 `:1155` 的闸门 ⇒ 计划变非空 ⇒ `:2633` 前提断言红 ⇒ 与作者报的 0.07s 红一致（我核对的是**红敏断言存在且正是该条件的判据**；未自己重跑，见 §3）。

### B2 达成 —— 耗尽**响亮**，且等待**没有被悄悄变长**
* `wait_for_rows`（`:2565-2581`）：`for _ in 0..60 { if got.len() >= want { return got; } sleep(50ms); got = rows(db).await; }` → **`panic!("wait_for_rows: waited 3s for {want} `distill_log` row(s) (one per distillation pass that RAN) and found {}: {got:?}. Zero means no pass ever finished; fewer than {want} means fewer passes ran than the plan called for -- neither is \"the pass wrote nothing\".")`** ⇒ 点名**在等什么**（每次**真的跑过**的 pass 一行）、**要几行/实际几行**（`{want}` / `{}` + `{got:?}`）、并区分「没跑过」与「跑了没写」✓。
* **上界未被调长**：`git show b6f1427^:…` 里父版本的 `wait_for_rows` **已经是 `for _ in 0..60`**（父版本另有 `0..100`/`0..200` 属别的用例）⇒ 3s 上限与 50ms 步长**都不是本提交加长的**；提交改的只有 `let got`→`got`、返回值→`panic!` ✓。
* 形状判断：这仍是**固定迭代次数**，但**耗尽会红**，所以它是「预算只是天花板」的正确形态（B34/C55 要的正是这个：**绿不能靠等**，等不够必须响亮）✓。

### B3 达成（带一条数值上的残余，见 §2①）—— 失败**真的留下耐久痕迹**，且「不 panic」的选择成立
* `impl Drop for Root`（`:34-49`）在 20 次（`0..20`，每次 50ms，约 1s）失败后：`let marker = self.0.join("LEAKED-ROOT.txt");`（`:66`）写入**内容含 root 路径、最后错误、以及「这个 marker 就是修复」的说明**（`:67-76`），并 `eprintln!` 带路径与错误。⇒ 痕迹是**磁盘上的文件**、随进程存活 ✓。
* **「不 panic」我独立判：成立**。理由不只是作者的转述 —— 在**线程已因断言失败而 unwind** 时，`Drop` 里再 panic 会 **abort 整个测试二进制**，一个泄漏的 root 会把同进程其它用例的结果一起带走；且它会**为一个该用例并不考察的原因**判它失败。⇒ 用「耐久 marker + stderr」替代 panic 是正确取舍。
* Windows-only：`#[cfg(windows)]`（`:220`）+ `fn a_root_that_cannot_be_removed_leaves_a_marker_instead_of_silence()`（`:222`），断言 `marker.exists()`（`:238-239`）与「marker 必须点名失败」（`:245`）。⇒ 与窗口 B 的红敏点一致（我核对断言存在与内容要求；未自己重跑）。

---

## 2 判据 5a/5b：有没有借别人的东西

* **5a（B1 不再调用等待 helper）**：断言前**零**等待调用；用例里唯一仍在的 `wait_for_rows` 是 **case (2)** —— 那个 case **确实有生产者**，等它有据 ✓。所以「共用判定 = 不共用」成立（准确表述：**两条否定性断言不再共用任何等待**，正向 case 仍用共用仪器是设计使然，不是借）。
* **5b（B2 的 3s 是它自己的论证）**：`:2554-2559` 的论证是「行写在它命名的那次抽取**之后**；本模块的调用点驱动的要么是确定性 rules 层（微秒级）要么是**根本 spawn 不起来**的 ACP pass —— 都比 60×50ms 低若干数量级；而**挂住的 pass 无上界**（那正是不能被掩盖的东西）。退出条件是行数，预算只是天花板。」⇒ **与 t145 的「5s < slowreply 10s 回合」、t149 的「写者同进程先落地」是第三段不同的论证**，**没有借用** ✓。

## 3 判据 4：我自己重跑的锚点（最终字节＝入库字节，退出码在管道之前取）

```
DAEMON=0  STORE=0  CLIPPY=0  FMT=0        （NO_PROXY=127.0.0.1,localhost,::1）
test -p ruagent-daemon : 157 / 11 / 5 / 2 / 16 / 0(+8 ignored) / 16 / 3 / 0 / 5 / 0
                         = 215 passed · 0 failed · 8 ignored（11 个 target，与作者申报一致）
test -p ruagent-store  : lib 44 passed(+3 ignored) · ledger_reopen 4 passed · migration_write_lock 1 passed · doc 0
clippy -p ruagent-daemon -p ruagent-store --all-targets -DenyWarnings : exit 0，`^error` 行数 0
cargo fmt --all --check : exit 0，`Diff in` 行数 0
```
**红线检查（纯字节）**：无**新增**固定长度 sleep（本提交无 `+` sleep）· 无「重试到过为止」（B3 的 20 次循环**耗尽即留痕**，不返回成功）· 无 `#[ignore]` · 断言未被换成打印（B3 的 `eprintln!` 是**附加**在 marker 之后，不替代断言）· `crates/daemon/tests/**`/`panel/**`/`.github/**`/`scripts/**` 零改动 ✓。

## 4 两条作者自报缺口的表态（不默认接受）

1. **三处均无前置红证** —— 接受其**替代形式**，但把话说准：两条红证都是**变异诱导的确定性红**（窗口 A：`:1183`；窗口 B：marker 路径；我核对了红敏断言的位置与内容，**未自己重跑**）。它们只证明「仪器**能**红」（必要条件），**不等于**「修前的原始 flake 会红」。真正让这次收口站得住的是**机制**：B1 的「空计划 ⇒ `return` 先于 `spawn`」是**字节上的结构事实**，此后「什么都没跑」**由逻辑可判**而非由等待时长决定；B2 是「想不到就响亮」；B3 是「放弃时留痕」。⇒ 与「0/N 绿是零信息」那条纪律**不冲突**：本单的承重读数不是任何绿样本。
2. **作者更正自己 t157 的模块坐标**（`chat::t8_tests` 而非 `generating_tests`）：**与 panic 路径一致** —— `mod t8_tests` 起于 `:2484`，该用例在 `:2584`，`wait_for_rows` 的 `panic!` 在 `:2574`，三者同 mod ⇒ 失败会以 `chat::t8_tests::…` 报出 ✓（我 t150 里把 **t149 的**用例记为 `generating_tests`、把 `wait_for_rows` 记为 `t8_tests`，与本单不矛盾：那是两条不同的用例）。

## 5 我自己【没有】验证的（第 19 条）

* **未重跑任何一次 1 行变异**（窗口 A/B）：本单 inScope 明令 `crates/**` 只读 ⇒ 只能在隔离副本 + 冷构建里做（≈10–15 min），我这一轮没花这笔时间；我做到的是核对**红敏断言存在/位置/内容**、核对**入库 blob 与工作副本一致**（零变异的更强形式）。
* **未自带负载样本**（避免与同伴抢机器），也没有复现过任何 flake；本单不依赖此类读数。
* **B3 的 marker 在真实 CI 上是否会出现**：我能说的有限 —— 该分支的**测试**是 **Windows-only**（`:220`），它在 Unix 上编译掉（注释如实写了「Unix 上打开的文件不阻止 `remove_dir_all`，所以那里压不出来」）⇒ **非 Windows 上这条失败分支既不可压、也无覆盖**；而**真实运行里**它依赖 Windows 的「占用目录删不掉」这一条件（与 `os error 32` 同族）。⇒ **在 Unix CI 上这段代码等于未被执行过**；这是诚实的范围声明，但请按「仅 Windows 有覆盖」记账。
* 未核 `runs.rs`/`sessions.rs` 一侧「谁在什么时候调 `maybe_auto_distill`」的全链（我只核到闸门与 spawn 的先后以及它在 `t8_tests` 内的可判性）。

## 6 非阻断观察（不构成 needs_revision，留给下一个入口）

① **B3 的痕迹在「绿」的 CI 运行里仍可能看不见**：marker 写在**那个删不掉的 root 里**（`:66`），而 `eprintln!` 来自**通过的**用例 —— 作者自己的注释就说「harness 会捕获它」（`:62`）⇒ 一次泄漏若发生在其它用例都通过的运行里，**退出码仍是 0**，只有人去看临时目录才发现。若要让泄漏**可失败**：把 marker 写到**稳定位置**（例如 `%TEMP%\ruagent-leaked-roots\<name>.txt`），或在用例末尾显式 `assert!(!root.path().exists())`。我判 **low**：作者没有过度宣称（注释自己写明了捕获问题），且它**明显优于修前的静默**。
② **同族残留在本文件里还在**：`chat.rs:1894` 仍有一处裸 `tokio::time::sleep(300ms)`（**另一条用例**，本提交未触及，`b6f1427` 的 hunk 只在 `:2544/:2593/:2618`）⇒ 建议按同族清单点名处置，**不是** t160 的缺陷。

## 7 verdict

**pass**。三处都把「猜测」换成了「事实」：B1 用**闸门**（空计划 `return` 先于 `spawn`，字节可核）替掉两处裸 300ms，且前提断言与被测路径**同源、非空转**（同用例正反两向都灵）；B2 把「耗尽」从**返回值**改成**响亮失败**且点名要什么/得什么，上界与步长**一字未加长**；B3 在失败时不 panic（避免 unwind 中 abort）而留**耐久 marker**（含 root/错误/说明）。**没有借别人的 bound 或读数**（B2 是第三段独立论证；B1 的否定性断言不再共用等待）。四道门我在**入库字节**上重跑全绿且逐 target 与作者一致。残余只有 §5/§6 的**覆盖与可见性**事项（Windows-only 覆盖、绿运行里泄漏不可失败、另一条用例的裸 sleep），均非实现缺陷。
