# t173 · 收口 t167 的两个残留（A 同族判类 + B 让泄漏可失败）

**来源**：`t167`（独立评审 t160）交回的两条非阻断观察。**我写本单的 finding，也实现了它 ⇒ 本单必须由别人评审**（我不能评自己的实现）。
**最终字节**：`crates/daemon/src/chat.rs` 131,272 B / sha256 `DB6A3AE9…` / blob `9240ea02…`（numstat **12/1**）· `crates/store/tests/ledger_reopen.rs` 14,317 B / sha256 `8195E925…` / blob `5a815dd6…`（numstat **100/2**）。
**依赖**：已 rebase 到 `t172` 之后的字节（`OptionsRead`、`record_probe` 的三态匹配、`MODELS_WAIT` 均未动）。

---

## A 残留：`chat.rs` 里那处裸 `sleep(300ms)` —— **是 ② 类；`t157` 的盘点漏了一条**

**坐标更正（先说我自己那条）**：`t167` 写的是 `chat.rs:1894`，那是**当时字节**上的行号；`t160` 与 `t172` 各动过这个文件 ⇒ **当前字节上是 `:1967`**（同一处 sleep，同一用例）。我按当前字节重新定位，不沿用旧号。

**它是哪条用例、等什么**：`an_empty_probe_neither_overwrites_nor_persists_the_catalog`（`mod` 内，`fn` 在 `:1883`）的第 2 步 ——
```
1953: // 2. An empty probe must not touch either one. Its table write would be
1954: //    spawned too, so give that window a chance to (not) happen before
1955: //    asserting the row is still the one from step 1.
1967: tokio::time::sleep(Duration::from_millis(300)).await;          ← 已删
1968: assert_eq!(persisted(&db, "mock").await, Some(1),
                "an empty probe must not persist over the stored catalog");
```
**判类（三条逐条对）**：① **否定性断言** —— 断言的是「表里还是旧行」（nothing persisted）✓；② **退出条件是可观测事实吗** —— **不是**，退出条件是**时间**（固定 300ms），注释自己就写着「give that window a chance to (not) happen」；③ **睡不够会不会假绿** —— **会**：spawn 出来的写若仍在飞，读者读到的就是旧行 ⇒ 断言通过而实现是错的 ✓。
⇒ **判定：这是 ② 类（用时间代替判定）**。
**对账结论（不许含糊）：`t157` 的「全仓 ② 类只有 B1 一条」漏了这一条。** 依据不是口味而是机制：那个窗口存在的原因是**这条臂之外的写是 `tokio::spawn` 的**（见下）；把 spawn 当异步理由、用固定时长对冲，正是 ② 的定义。这条应补进同族计数，`t157` 的计数应为 **≥2 条**。

**修法（把猜测换成事实，而不是把窗口调长）**：删掉 sleep，改为**按臂论证 + 复用本用例已有的灵敏度读数**：
* 机制（字节可见，符号优先）：`record_probe`（`:1339`）的 `OptionsRead::NotReportedInTime | OptionsRead::ReportedNone` 臂**只读 cache、不写库**，**到不了 `persist_options`**（`:1820`，`agent_options` 的唯一写者）；spawn 只出现在它的 `Reported` 臂（`:1831`）⇒ **这条路径上没有任何异步生产者**，`record_probe` 返回后不可能有写落地。
* 非空转：同用例**第 1 步**已经把「读者能看见 spawn 出来的写」变成了读数（`for _ in 0..100` 轮询 `persisted(…)` 直到出现并断言 `Some(1)`）⇒ 第 2 步的「没变」是**两条臂之间的差异**，不是「读者永远看不见」的空断言。
* 我没有加新的等待，也没有把 sleep 调长；`from_millis(300)` 在本文件里现在 **0 处**。

## B 残留：让 `LEAKED-ROOT.txt` 的泄漏**可失败**

**修法：① 稳定位置的标记 + 让拥有 root 的用例在泄漏时失败（②的机制用于①的标记）**，并说明为什么必须**两半都有**：
* **只有就地标记不够**：它写在**那个删不掉的目录里**（`t160` 的既有行为，逐字保留），且通过的用例的 `eprintln!` 被 harness 捕获 ⇒ 退出码仍是 0。
* **只用例末尾断言也不够**：`Drop` 在 unwinds 中 panic 会 abort 整个测试二进制（`t167` 已独立复核这条判断，本单**没有**改成 panic），而**一个已经 panic 的用例根本走不到它末尾的断言** ⇒ 那种泄漏只有磁盘上的标记能留下证据。
* **为什么用 tag 而不是全局计数**：`ledger_reopen.rs` 里有一条**故意泄漏**的用例（Windows 的 marker 用例）⇒ 全局计数会让**并行跑的别的用例**因它而红。守卫按 root 的 **tag** 记（`static LEAKED_TAGS: Mutex<Vec<String>>`），每个用例只断言**自己那个 tag** 没出现 ⇒ 故意泄漏不会误伤邻居（一条会误报的门禁比没有门禁更坏）。

**落地**（`crates/store/tests/ledger_reopen.rs`）：
1. `struct Root(PathBuf)` → `struct Root(PathBuf, String)`（第二位 = tag），ctor `Root(dir, tag.to_string())` —— 这**是新增的两行删除**，既有守卫一行未改。
2. `Drop` 失败分支在**原有 marker + eprintln 之后**追加：把 tag 推进 `LEAKED_TAGS`；把同一份说明再写一份到**稳定位置** `%TEMP%\ruagent-leaked-roots\<root 名>.txt`（含 root 路径、最后错误、就地 marker 路径；同名重复泄漏覆盖同一文件）。
3. 新守卫 `assert_own_root_did_not_leak(tag, what)`：泄漏 ⇒ **该用例红**，报文点名 tag 与已泄漏的 tag 列表。
4. 三条拥有 root 的用例（`max`/`hole`/`future`）末尾 `drop(root)` 后各调一次守卫。
5. Windows 的 marker 用例（故意泄漏）：保留其既有三条断言**逐字**，另加**阳性对照 + 确定性红证**——断言 tag 已被记录、稳定标记存在且点名失败与 root，并用 `std::panic::catch_unwind(|| assert_own_root_did_not_leak("leak-marker", …))` **证明那条守卫会对这个条件红**（捕获住 ⇒ 这条故意泄漏的用例自己仍然通过）。

**读数（C47 口径：不拿「0/N 绿」收口）**
| 读数 | 结果 |
| --- | --- |
| **无泄漏 ⇒ exit 0** | `test -p ruagent-store` **exit 0**，三条守卫在各自用例末尾执行通过（`ledger_reopen` **4 passed**） |
| **有泄漏 ⇒ 守卫会红** | Windows 用例里 `catch_unwind` 捕获到守卫的 panic ⇒ `reddened = true`（**确定性红证，在树内、非变异**；共享树从未被改动，故无需宣告窗口） |
| **泄漏被记录** | 同用例断言 `leaked_tags()` 含 `leak-marker` ✓ |
| **稳定标记真的可读到** | 同用例读 `%TEMP%\ruagent-leaked-roots\<root名>.txt`，断言含 `could not be removed` 与 root 路径 ✓ |
| **不留残留** | 用例末尾按**具体路径**删该标记与空目录；跑完后 `Test-Path %TEMP%\ruagent-leaked-roots` = **False** ✓（创建只发生在失败分支，干净运行不产生任何东西） |

**没有削弱既有守卫（纯字节核对删除行）**：`ledger_reopen.rs` 的全部删除行只有 `struct Root(std::path::PathBuf);` 与 `Root(dir)` 两行 ⇒ 「不 panic 的注释」「`for _ in 0..20`」「marker + eprintln」**逐字在位**（现 `:90`/`:75`/`:113`）；`chat.rs` 的全部删除行只有那一行 sleep ⇒ `wait_for_rows` 的 `panic!`（`:2686`）与 `for _ in 0..60`（`:2678`）**逐字未动**。`crates/daemon/tests/**` 未触碰。

## 门禁（最终字节，退出码先于任何管道）

```
FMT=0 · DAEMON=0 · STORE=0 · CLIPPY_FINAL=0（^error 行 0）
test -p ruagent-daemon : 157 / 11 / 5 / 2 / 16 / 0(+8 ignored) / 16 / 3 / 0 / 5 / 0
                         = 215 passed · 0 failed · 8 ignored
test -p ruagent-store  : lib 44(+3 ignored) · ledger_reopen 4 passed · migration_write_lock 1 · doc 0
```
**过程披露（按「绿门只对它跑过的字节负责」）**：第一次 `cargo fmt --all --check` 在我的插入代码上报了 1 处 `Diff in …ledger_reopen.rs:328` ⇒ 我只对**我改的两个文件**跑 `rustfmt`（没有 `--all`，避免动别人的文件），随后在**格式化后的字节**上重跑 `fmt`/`-p ruagent-daemon`/`-p ruagent-store` 全绿；`clippy` 我也在最终字节上**重跑**了一次（exit 0）。

## 未覆盖什么（第 19 条）

1. **A 若是 ① 就不改** —— 不适用：我按三条判据判它是 **②**，故改了；判据与坐标都写在上文。
2. **B 的跨平台**：**泄漏条件**在 Unix 上压不出来（`remove_dir_all` 不会被打开的文件挡住），所以 `#[cfg(windows)]` 那条用例仍是**唯一能真正进入失败分支**的地方 ⇒ **Unix 上：标记/计数/守卫的代码会被编译并运行，但既不触发也不会执行我那条红证**（`catch_unwind` 那段在 Windows 用例里）。⇒ 与 `t160` 同一边界，**没有变好也没有变坏**：我**没有**声称它在 Unix 上可覆盖。
3. **没有重跑真实负载**（也不该用它收口）：本单两处都**没有前置红证**，所以结论按纪律写成「**移除了一个假绿来源（A）/ 把一个静默变成了可失败（B）**」，而不是「跑绿了所以好了」。
4. 我没有为 B 做**隔离副本 + 单行变异**：红证已经**在树内**用 `catch_unwind` 取得（守卫对该条件确实 panic），且**共享树零变异** ⇒ 没有窗口需要宣告；如果你要求的是「把实现改坏 ⇒ 目标测试红」，那是另一种红证，可以在后续单里做（需隔离副本 + 冷构建）。
5. **`panel/src/index.css` 现在出现在 `git diff --name-only HEAD` 里，但那不是本单的改动**（本单 changedPaths 只有上面两个 Rust 文件 + 本报告）—— 是别的在途单元在编辑它，我不碰。

## 写入集合

`crates/daemon/src/chat.rs` · `crates/store/tests/ledger_reopen.rs` · 本报告（`docs/design/reviews/gen4-wait-residue-repair.md`）。未启停活守护进程，未写活库，未触碰 8787。
