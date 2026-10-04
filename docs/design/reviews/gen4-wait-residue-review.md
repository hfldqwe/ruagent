# 独立评审 `t173`：两处等待残留（A 判类/臂论证/对账 + B 可失败设计）

**任务**：`t175`（kind = review，round 1）· **评审对象**：`t173`（commit **`9f4eb9f`**）· **日期**：2026-10-04
**本单写入**：仅本文件（`crates/**`、`panel/**`、`.github/**`、`scripts/**` **只读**）
**评审方式**：**纯字节**（不采信转述）+ **自己重跑四门口禁** + **零变异**（未开任何窗口）

## 结论：**verdict = pass**（两处交付的声明逐条在字节上成立；附 2 条**不阻塞**的建议项）

**被评审字节（我自己算的 hash，与 captain/作者报的逐字相同）**：
- `crates/daemon/src/chat.rs` sha256 = **`DB6A3AE9052F1348AB09A65BDC06508A6F43FF783B24C5347563C6A8624483DD`**，blob = **`9240ea0262016afdbc7820ad0f4e7b5f98034b71`** ✓
- `crates/store/tests/ledger_reopen.rs` sha256 = **`8195E9257E8C8917EF0CD2553F65C4154390241A496F375F61FF25A0C57E5F1F`**，blob = **`5a815dd60675babc1d8339590e21202c8e0381f9`** ✓
- 工作树 `git status --porcelain` 只有一条**别单元**的未跟踪文件（`docs/design/reviews/gen4-e2e-coverage-floor-verify.md`）⇒ **被评审字节没有任何未记录的后续改动**（入库 blob 与工作树一致）。

---

## 1. A —— 判类、臂论证、对账（全部自己核）

### 1.1 A① 那处 sleep 是否真的消失：**成立**

- `Select-String -Path crates\daemon\src\chat.rs -Pattern 'from_millis\(300\)' -SimpleMatch` ⇒ **0** 命中 ✓。
- **更强的一层（我加了这一步）**：`git show 9f4eb9f --unified=0 -- crates/daemon/src/chat.rs` 显示该 commit 在 chat.rs 里**只有一处删除**：
  `-        tokio::time::sleep(std::time::Duration::from_millis(300)).await;`（hunk 头 `@@ -1967 +1967,12 @@ mod generating_tests`），替换为 12 行注释。⇒ **没有任何断言被删、没有阈值被放宽、没有 `#[ignore]`、没有把断言换成打印**。

### 1.2 A② 臂论证：**成立**（且在字节上是「只有一条路能写」）

- `ChatManager::record_probe`（`chat.rs:1339-1383`）现在是显式全匹配；**`NotReportedInTime | ReportedNone` 臂**（`:1348-1369`）只做：`tracing::debug!`、读 `model_cache`、`previous.unwrap_or(...)` ⇒ **该臂内没有任何 `persist_options` 调用** ✓。
- chat.rs 里 `persist_options` 的**调用点只有两处**：`:957`（**会话选项跟踪器**，见 §4 建议项 O1）与 `:1379`（`record_probe` 的 `Reported` 臂）；定义在 `:1820`。⇒ **`record_probe` 的非报告臂到不了写者** ✓。
- 注释里引用的两个坐标我逐个核了：`persist_options` **定义在 `:1820`** ✓、其 **`tokio::spawn` 在 `:1831`** ✓ —— **注释不是约数，是准确坐标**。

### 1.3 A③ 第 1 步是否真的把「读者看得见 spawn 出来的写」变成读数：**成立**

`:1940-1951`：`record_probe(Reported([mock-pro]), 1)` → 断言 cache 长度 1 → **轮询 `persisted(&db,"mock")` 最多 100×20ms 直到 `Some`** → `assert_eq!(stored, Some(1), "a real probe must be persisted")`。⇒ 第 2 步的「表里还是第 1 步那一行」是**两条臂之间的差异**（读者并非看不见写），不是空断言 ✓。

### 1.4 「`t157` 漏了一条」这个对账：**成立**（同族计数 ≥ 2）

**我自己判这处是不是 ②**（按 C55 的三条判据，逐条对字节）：
1. **否定性断言** ✓ —— 删掉的那处 sleep 后面是 `assert_eq!(persisted(&db,"mock").await, Some(1), "an empty probe must not persist over the stored catalog")`（`:1979-1983`），命题是「**没有**发生一次新写」。
2. **退出条件是时间** ✓ —— 在「被测调用（`record_probe(ReportedNone,2)`）」与「该断言」之间，**唯一的**东西就是那 300ms（cache 断言是同步的，表断言只靠这个窗口）。
3. **睡不够就假绿** ✓ —— 若将来有人让空臂也 spawn 一次写，窗口太短则**写还在飞**而读者看到旧行 ⇒ 断言通过。**它的失败模式是假绿**，与本族 ② 的定义（用时间**代替**判定）一致。
⇒ **判 ② 成立**，作者的判类**正确**。

**对账**：`t157` 自己的表格把这一条**归错类**了 —— 我在 `gen4-wait-shaped-tests-inventory.md` 的 B6 行写的是：
> `| B6 | chat::option_catalog_tests::persisted（chat.rs，for _ in 0..100 { sleep(20ms) }，退出=行出现，耗尽 assert_eq!(stored, Some(1), …)） | … | ① | 无 |`

即：**t157 只读到了第 1 步那个「退出=事实」的轮询循环，从未把第 2 步那处裸 `sleep(300ms)` 单列为一件仪器** ⇒ t157 的「全仓 ② 只有 B1 一条」**确实是漏了一条**，同族计数应为 **≥2** ✓。**并且我要如实补一句**：t157 该行还把模块名写成 `option_catalog_tests`，而该 commit 的 hunk 头显示它是 `mod generating_tests` ⇒ **这是我（本评审者）在 t157 里犯的第二处坐标/模块名错误**（第一处是 `the_unattended_plan…` 的 `t8_tests`/`generating_tests`），作者的更正因此**不只是对账，也是对我的一次纠错**。

---

## 2. B —— 让泄漏「可失败」且不误伤邻居（逐条核）

| 判据 | 读数（字节） | 结论 |
| --- | --- | --- |
| ① **按 tag 而非全局计数** | `:22 static LEAKED_TAGS: Mutex<Vec<String>>`；`:41-48 assert_own_root_did_not_leak(tag, what)` 断言 `!leaked.contains(&tag)`；`:123-125` 在失败分支 `tags.push(self.1.clone())`（`Root` 的第二个字段就是 tag，`:51`/`:60`） | **成立** ✓。一条**故意泄漏**的用例推入的 tag 是 `"leak-marker"`，而 max/hole/future 各断言**自己的** tag ⇒ **并行邻居不会被误伤** ✓（作者的理由「会误报的门禁比没有门禁更坏」在实现上被落实了，不是口号） |
| ② **三条拥有 root 的用例末尾真的调了守卫（`drop(root)` 之后）** | `:207-208`（max）、`:247-248`（hole）、`:271-272`（future），每处都是 `drop(root);` 紧跟 `assert_own_root_did_not_leak("<tag>", "<test name>");` | **成立** ✓。**顺序很关键而且它做对了**：守卫在**显式 `drop` 之后**，所以 `Drop` 的记账已经跑完，守卫读到的不是过期状态 |
| ③ **稳定位置那份副本只在失败分支才建** | `:74-83` 是 20 次重试，`Ok(()) => return`；`:100-116`（t160 的就地标记 + `eprintln!`）与 **`:117-144`（t173 的 tag push + `%TEMP%\ruagent-leaked-roots\<root名>.txt`）都在 `return` 之后** ⇒ 只在「20 次全失败」的路径上 ✓。**运行读数**：我在**四门口禁跑完之后**再查 `Test-Path $env:TEMP\ruagent-leaked-roots` ⇒ **False** ✓（未走失败分支 ⇒ 没建稳定副本，也没留下残渣） | **成立** ✓ |
| ④ **「只用例末尾断言不够：已 panic 的用例走不到末尾」** | 论证成立且**与代码结构一致**：守卫在用例**末尾**，一个已经 panic（断言失败）的用例根本执行不到那里；那种情形下唯一留得住的证据就是**磁盘上的标记**（就地 + 稳定位置） | **成立** ✓ ⇒ **「两半都要」有充分理由**：末尾守卫把「通过用例的静默泄漏」变成红，磁盘标记保住「已红的用例」的证据 |

**B 的红证：我按它给的方式独立复核，结论是「无需窗口」——它是树内取得的** ✓
- `:338-341`：`let reddened = std::panic::catch_unwind(|| assert_own_root_did_not_leak("leak-marker", "the t173 probe")).is_err(); assert!(reddened, "the leak guard must redden when a root leaked, or the leak is still only cosmetic");`
- 这条**不是**「先变异再观察」：它在**未变异的共享树**里断言 **「守卫确实会 panic」**；而 `ledger_reopen` 目标在我的门禁里 **4 passed** ⇒ **运行时读数就是 `reddened == true`**（否则该断言会红）⇒ **红证在真实字节上成立，共享树零变异，确实没有需要宣告的窗口** ✓。
- 同时 `:318-337` 还断言了「tag 已被记录」与「稳定标记可读且**点名 root**」⇒ 「可见」不是装饰 ✓。

---

## 3. 红线（纯字节，逐条）

| 声明 | 我的读数 |
| --- | --- |
| `ledger_reopen.rs` **删除行只有** `struct Root(std::path::PathBuf);` 与 `Root(dir)` | **成立** ✓：`git show 9f4eb9f --unified=0` 在 ledger 里只有两处**修改**（`-struct Root(std::path::PathBuf);` → `+struct Root(std::path::PathBuf, String);`；`-Root(dir)` → `+Root(dir, tag.to_string())`），其余全部是**新增** |
| 「不 panic」注释、`for _ in 0..20`、marker + `eprintln!` 逐字在位 | **成立** ✓：`:90-96` 的 `WHY NOT panic!` 注释、`:75 for _ in 0..20`、`:100-111` 就地 marker、`:112-116 eprintln!` 全部在；**Drop 里没有新增 panic** ✓ |
| `chat.rs` **删除行只有**那行 sleep | **成立** ✓（§1.1 的 commit 级证据） |
| `wait_for_rows` 的 `panic!` 与 `for _ in 0..60` 未动 | **成立** ✓（本文件 `wait_for_rows` 的 panic 消息仍在；重试上界仍在。t173 的 patch 只碰 `:1967`） |
| `t172` 的 `OptionsRead`/`record_probe`/`MODELS_WAIT` 未动 | **成立** ✓：`OptionsRead` 17 处命中、`record_probe` 的显式匹配 `:1347-1382` 完好、`MODELS_WAIT: Duration = Duration::from_secs(20)` 恰好 1 处（**20s 未被改动**）；commit 的文件清单也不含任何形状改动 |
| `crates/daemon/tests/**` 未触碰；没有新增固定 sleep / 重试到过 / `#[ignore]` / 断言换打印 | **成立** ✓：commit `9f4eb9f` 只动 **3 个文件** —— `crates/daemon/src/chat.rs`、`crates/store/tests/ledger_reopen.rs`、`docs/design/reviews/gen4-wait-residue-repair.md`；新增内容全是注释、tag 记账、守卫与标记，**无一处 sleep 增量**，**无 `#[ignore]`**，**无「打印代替断言」**（唯一新增的 `eprintln!` 是 t160 已有的那条，t173 只加了 `fs::write` 标记与断言） |

---

## 4. 我自己重跑的门禁（**最终字节**，退出码**先于任何管道**取值）

| 门禁 | 读数（逐 target） |
| --- | --- |
| `scripts/cargo-team.ps1 test -p ruagent-daemon` | **exit=0** —— `unittests src/lib.rs` **157 passed / 0 failed / 0 ignored** · `capabilities.rs` 11 · `capability_defaults.rs` 5 · `event_compat.rs` 2 · `graph_ingest.rs` 16 · `injection_e2e.rs` 0 passed/**8 ignored** · `knowledge_api.rs` 16 · `recall_evidence_announced.rs` 3 · `smoke.rs` 0 · `version_points.rs` 5 · `Doc-tests` 0 ⇒ **合计 215 passed · 0 failed · 8 ignored** ⇒ **与作者自报逐字一致** ✓ |
| `scripts/cargo-team.ps1 test -p ruagent-store` | **exit=0** —— lib **44 passed / 0 failed / 3 ignored**（目标内 47）· **`ledger_reopen.rs` 4 passed / 0 failed**（含那条故意泄漏的 Windows 用例）· `migration_write_lock.rs` 1 passed · `Doc-tests` 0 |
| `scripts/cargo-team.ps1 clippy -p ruagent-daemon -p ruagent-store --all-targets -DenyWarnings` | **exit=0**，`^error` = **0** |
| `cargo fmt --all --check` | **exit=0**，零 `Diff in` |

⇒ **`t172` 的底座 + `t173` 的两处改动，四门全绿**；且 fmt 全树零 diff（见 §5①）。

---

## 5. 两条自报披露的表态

① **fmt 处置：符合 AGENTS.md，判为正确**。作者首跑 `fmt --all --check` 在**它自己插入的** `ledger_reopen.rs:328` 报 1 处 `Diff in`，随后只对**自己改的两个文件**跑 `rustfmt`（**没有** `--all`），并在**格式化后的字节**上重跑全部门禁。AGENTS.md 的纪律正是「format before you submit」+「**不要**重排你没碰的文件」（后者一旦违反会把局部 diff 变成全仓 diff、无法评审）⇒ **它选的是纪律允许的那条路**；而我的 `fmt --all --check` = **exit 0** 证明最终全树是干净的 ✓。**唯一可挑剔的是时序**（先跑了 `--check` 才发现自己新写的行未格式化），但**处置无瑕疵**。

② **坐标更正：核对成立**。`t167` 报的 `:1894` → 当前 `:1967`：我核了 **pre-image**（commit hunk 头 `@@ -1967 +1967,12 @@`）⇒ 删除前那行**确实在 1967** ✓。`t160`（我）与 `t172`（我）各对该文件动过行，**方向与量级都支持这个位移**（t160 删了两处 sleep 并加了注释、t172 加了 `OptionsRead` 与匹配体）。**`t167` 当时那一版里的 `:1894` 我没有独立复核**（见 §7）。

---

## 6. 建议项（**不阻塞本单**，交 captain 派单）

**O1（advisory，**不在被评审 diff 内**，需 chat/acp 属主判意图）—— 会话选项跟踪器对「报告了空」采用了与 `record_probe` **不同**的策略**：
- 字节：`chat.rs:943-963` 的跟踪器循环里，`if let Some(state) = watch.borrow_and_update().clone() && last != state` ⇒ `model_cache.insert(...)` + **`persist_options(&db, &runtime_key, &state, updated_at)`（`:957`）—— 对空 `Vec` 没有任何判据**。而 `record_probe`（t192/t172）对同一观测（「报告了没有」）的规则是**保留上一次、不落库**。
- **可达性（我核了发布端）**：`acp/src/chat.rs:444-445` `let advertised = extract_options(session.config_options.as_deref().unwrap_or(&[])); options_tx.send_replace(Some(advertised));`（另有 `:470`、`:612` 两处 `Some(...)`）⇒ **发布端从形态上允许发布 `Some(vec![])`**（没有 config options 时 `unwrap_or(&[])` 就是空列表）。⇒ 若某个运行时报告空列表，跟踪器会**把空目录写进 cache 并持久化到 `agent_options`**，而 `load_option_cache` 会在重启后把它读回来 —— **正是 t192 注释里描述的那次故障形状**（空 picker 且跨重启固化、不再重试）。
- **它为什么不是 t173 的缺陷**：跟踪器路径与 t173 的测试路径无关（该测试直接调 `record_probe`，没有 live session ⇒ 没有跟踪器）⇒ 作者的「此处 nothing can land」结论**依然成立**；跟踪器也没有被 t173 改动或削弱（`git show` 的文件级证据：`:957` 那行不在 diff 内）。
- **真正要问的是意图**：「live 会话第一手的『我没有选项』」是否**应当**权威（与 probe 的低置信不同）？若**是**，则应在 `:945-957` 写明这条**有意的差异**；若**不是**，这里与 t192 的规则冲突。两种答案都要求属主表态 ⇒ **我没有把它算作 t173 的问题，也没有要求 revision**。

**O2（advisory，low）—— 删掉那处 sleep 的残余覆盖**：删掉之后，若将来有人**只在空臂**加一次 `persist_options`，**当前没有任何测试会红**（cache 断言只看得见 cache 的写；表断言在写落地前就读完了）。这不是回归（旧窗口是**假绿易发**的仪器，去掉它没有丢失可信读数），但它说明了「**「没有 spawn」这件事本身不可观测**」：更合适的机械判据是让该臂的「不写」成为**结构/类型事实**（t172 的显式匹配已经做了一半），或让 `record_probe` 的返回**携带「我是否调用过写者」**这类可断言的信息 —— 留作**登记**，不建议为此单开工。

---

## 7. 我没有验证的（第 19 条）

- **Unix 上 B 的失败分支压不出来**：`#[cfg(windows)]` 是那条红证的**唯一入口**（作者已承认）。⇒ 在 Unix 上「泄漏 ⇒ 用例红」这条路径**既不触发也不执行**，我在此**不声称它在 Unix 上变好**；我只验证了它在 Windows 上成立。
- **我没有重跑真实负载**：本单只跑了四门口禁（§4），**没有**造占用进程/并发冷构建那套形状 ⇒ 我**没有**关于「负载下泄漏门禁稳定性」的读数。
- **`panel/src/index.css`**：它**不在**当前工作树 diff 里（`git status --porcelain` 只有一条别单元的未跟踪报告）⇒ 我无法复核作者当时看到的「diff 里有 index.css 但不是本单的」这一条；我核到的等价事实是**commit `9f4eb9f` 只含 3 个文件、不含任何 `panel/**`**。
- **`t167` 报的 `:1894`**：我只核到「t173 前该行在 `:1967`」，**没有**去读 `t167` 的报告原文确认它当时写的就是 1894（因此「位移的量级一致」是**推断**，不是读数）。
- **跟踪器 O1 的运行时可达性**：我核的是**类型与发布端形态**允许 `Some(vec![])`；我**没有**构造一个真会报告空选项的运行时去观测它（那需要真实 harness）⇒ O1 是**代码级**可达，不是运行级已观测。
- **本评审未开任何变异窗口、未改任何字节**；`crates/**` 全仓只读，活守护进程（8787）未启停，收尾无临时物。
