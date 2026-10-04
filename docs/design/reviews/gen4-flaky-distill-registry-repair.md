# F2 收口：`distill::t329_tests` 与 `registry::tests` 在负载下的红

**任务**：`t154`（repair）· **日期**：2026-10-04 · **inScope 唯一写入**：`crates/daemon/src/distill.rs`、`crates/daemon/src/registry.rs` · 报告：本文件

**一句话**：F2 的红**我未能随机复现**（0/4，按 C47 记「**未复现**」，不是「不成立」）；但**机制被我读出来了，而且我加的「前提断言」把它当场抓住**：这两个模块的**夹具（fixture）名字是 `pid + seq`**，而 **pid 会被 OS 重用** ⇒ 新进程打开的是**上一轮遗留的数据库/`agents.toml`** ⇒ 被写的那行**按构造就是重复行** ⇒ `written = 0` ⇒ `distill.rs:1362` 的 `left: 0 / right: 1`，并且**一个被重用的 pid 会同时打红整个模块**（这正是 verify2 报的**成片红**的签名）。修法是**让「fresh」真的等于「empty」**（两者都先清目录），并保留那条**把前提变成断言**的仪器。

---

## 1. 复现读数（第 1 条判据）：**未复现**，并按 C47 标明它的信息量为零

| 项 | 读数 |
| --- | --- |
| 负载形状 | **10 个自建 CPU 占用进程 + 一个并发冷构建**（私有 `%TEMP%\t154-load-target`，`-TargetDir` + **`-NoLock`** ⇒ 不占共享锁/共享 target）+ 全套 `test -p ruagent-daemon` |
| 次数 / 红 | **4 / 0** |
| 同形状在**别的测试**上的前置红证 | ✅ **有**：同形状（10 占用 + 私有 `-NoLock` 冷构建）在 `chat::generating_tests::delete_removes_the_row_and_stops_a_live_run_first` 的**修前版本**上 **4/10 红**（见 `gen4-flaky-chat-delete-repair.md` §1）⇒ 这个形状**能**打红这一族 |
| 同形状在 **F2 这两组** 上的前置红证 | ❌ **无**（从未观测到）⇒ **本次 0/4 是零信息**，按 C47 **只能记「未复现」**，不能记「F2 已消失」 |

**树状态（发布前回读）**：`distill.rs` **干净**且 sha256 **`0291DF0E6E580A7F5D806D5EDF8A36A2CF7D409519E8E2045BA533F1A7CA4E3F`**（**与 verify2 报的完全一致**）；`registry.rs` **干净**，sha256 `1183D4F9D1244C18FCFD57956B9DC375D7A58908BD41AE9F27E4845276FC99F3`。当时 ` M` 的文件 = `crates/daemon/src/chat.rs`（我 t145 的修复）、`docs/design/reviews/gen4-audit-ci-first-run.md`、`panel/src/views/Settings.tsx` —— **均非本单**。

## 2. 逐条把竞争从字节读清（第 2 条判据）

### 2.1 `distill::t329_tests`（verify2 报 6 条同时红，逐字一条 `distill.rs:1362: the write must land, or the assertion below is vacuous / left: 0 / right: 1`）

- **读者**：`assert_eq!(w, 1, …)`，读 `out.written`；**写者**：`Distiller::write_memories`（`distill.rs`）里 `W::Inserted(_) / W::Superseded{..}` 两个分支各 `written += 1`，跳过分支是 `W::SkippedDuplicate(_) / W::RejectedNamespace` ⇒ `skipped += 1`。
- **它不是「固定猜测窗口」型**：该模块（原 1213-2140）**通篇无 `sleep` / `spawn` / `Instant::now` / `timeout`**（grep 零命中）；`write_memories(…).await` 是**同步返回**，`Db::call` 还会等单写者 actor 的回复 ⇒ **字节上看不到时序窗口**。
- **那 `written = 0` 从哪来 —— 夹具不是 fresh 的**：`t329_tests::root(tag)` 的目录名 = `ruagent-t329-{tag}-{pid}-{n}`（`n` 每进程从 0 起），且它**从不清理**。⇒ **pid 被 OS 重用**时（本机实测确实频繁发生），新进程的 `root()` 命中**上一轮留下的目录**，`Db::open` **安静地打开旧库**，库里已经有同 `store/namespace` 的活行 ⇒ `merge_target`/`write_memory` 判定**重复** ⇒ `written = 0`。
- **证据（本轮实测）**：① 我加的**前提断言**当场触发 —— `panicked at crates\daemon\src\distill.rs:1363:9`（「the fresh root must start with no live `profile/user` row」），**模块内 5 条一起红**；② 遗留目录 **6202 个**，其中**含 `ruagent.db` 的 = 6202 个**（`ruagent-t329-*`，全量列举，2026-10-04）；③ 修掉夹具后**同一命令 exit=0（ok=11 / FAILED=0 / panicked=0）**。
- **分类**：**判据侧（夹具/仪器）**，不是产品侧缺陷。**回答 C48 的问题「`written = 0` 在生产里能不能被解释」：能** —— 重新蒸馏同一段内容时，同样的 content hash 会走 `SkippedDuplicate`，这是**合法的**，并且产品**把它报出来了**（`MemoryWriteOutcome.skipped` → `DistillOutcome.memories_skipped` → `distill_log`），所以**不存在「跳过却当写成功」的静默成功**。测试要的 `written == 1` 是**对 fresh 库的前提**，不是生产不变量 —— 正因为它是前提而没人断言，F2 才看起来像产品缺陷。

### 2.2 `registry::tests`（verify2 报 2 条同时红）

- **读者/写者**：每条用例 `editor()` 造一个目录 + `agents.toml`，`Editor::{create_runtime,create_agent,...}` 读改写它，用例再 `std::fs::read_to_string(ed.path()).unwrap()` 读回。
- **同族的第一层（潜伏，已硬化）**：目录名原来是 `pid + subsec_nanos()`（**亚秒级时钟**）⇒ 同 tick 两用例同目录，且败者的 `TempDir::drop` 会删掉胜者的文件。**但我实测否掉了它作为已观测红因**：本机 20000 次连续 `UtcNow` 读数 **20000 个互不相同**。⇒ 按同族硬化（改进程内 `SEQ`），并**在注释里写明「潜伏、非已证成因」**，不冒领。
- **同族的第二层（与 2.1 同一根因，已修）**：`pid + seq` 同样会被**pid 重用**命中，而 `TempDir` **只在成功 drop 时**才删目录 ⇒ 上一轮 panic 留下的目录会被复用（带着旧 `agents.toml`）。修法与 2.1 同：**先清再建**。
- **分类**：**判据侧（仪器）**。

## 3. 修法（第 3 条判据）：不是调长 sleep、不是重试、不是 `#[ignore]`、不是放宽阈值

| 文件 | 改动 | 为什么这样就能消掉竞争 |
| --- | --- | --- |
| `distill.rs` `t329_tests::root` | `let _ = std::fs::remove_dir_all(&p);` 后再 `create_dir_all` | 「fresh root」变成**真的空**：无论 pid 是否被重用、上一轮是否留下 `ruagent.db`，本次看到的都是空库 ⇒ 那条「按构造重复」的路径不再存在。**这是夹具的显式同步**：先清（把上一进程的状态从这个世界里删掉）再建（本测试的世界从零开始），而不是**等**一个时钟或延时。 |
| `registry.rs` `editor()` | 同上（清+建），并把目录名的时钟换成进程内 `SEQ` | 同上；SEQ 顺带消掉 2.2 第一层的潜伏碰撞。 |
| `distill.rs` 1362 那条断言 | **把注释里的前提变成断言**（t149 形状）：先断言该 `(store,namespace)` 的**活行 = 0**，再断言 `written == 1`，并把 `skipped` / `episode_recorded` 写进失败信息 | 上一次的失败信息只说 `left: 0 / right: 1`，**两种成因不可分**（旧库重复 vs 写路径拒绝）；现在**前提单独可证伪**，失败信息直接点名 —— 这条断言正是本轮抓到根因的仪器。 |

**每条用例自己的边界论证（同族纪律：形状相同 ≠ 仪器共享）**：
- `t329_tests`：**这里不需要任何时间上界** —— 修好后判据是**同步值**（`written`/`skipped`）与**前提**（活行 0），没有等待，因此**不存在**「bound 取多少」的问题（这是我把它与 t145 区别开的地方：t145 必须等一个异步终局，本模块不是）。
- `registry::tests`：全部是 **`std::fs` 同步 I/O**（`Editor::save` 写临时文件后 rename），没有异步终局可等；判据是「文件内容 == 预期」，修的是**夹具唯一性/新鲜性**，同样**不引入时间上界**。

## 4. 改完仍能红（第 4 条判据）：两个窗口，均按字节恢复

**窗口 A（`distill.rs`，非 test 路径：`Distiller::write_memories` 的 `W::Inserted(_)` 分支去掉 `written += 1;`）**
- 开窗 `17:34:20`（sha256 `42E6AA30C09CD3978A92F7B87D8B075597F049C650CB1E1F336FCC5027DF5E2D`）
- 读数：`test -p ruagent-daemon --lib t329_tests::a_distilled_body_carries_no_provenance_marker` **exit=101**
  `panicked at crates\daemon\src\distill.rs:1383:9: assertion left == right failed: the write must land, or the assertion below is vacuous: written=0 skipped=0 episode_recorded=true -- … ; left: 0 / right: 1`
- ⇒ **签名与 verify2 报的逐字一致**（`left: 0 / right: 1`），且新消息**证明**它是「写没被报告」而非「重复跳过」（`skipped=0`）。
- 关窗 `17:34:41`，恢复后 sha256 = **`D3C2A869512A61C60CD58A7BE6D5D72D69F69544C311BFC6E3463168B11AB1F0`** = 开窗前值 ✓

**窗口 B（`registry.rs`，只改测试仪器 `editor()` 的唯一目录名为固定名 —— 模拟「两用例同目录」这一族）**
- 开窗 `17:34:46`（sha256 `9102D36B23E5B59A27A0F2D815D07769A96109AD2577C1B8197B6DB2067A5345`）
- 读数：`test -p ruagent-daemon --lib registry::tests` **exit=101**，**5/6 条同时红**（`panicked at registry.rs:438/470/485/591/618`）⇒ **成片红**，与 verify2 报的**同签名**（他 2 条、我 5 条，数量随交错变化，机制相同）
- 关窗 `17:35:06`，恢复后 sha256 = **`81547380F923DC038788232C54447F346B310C30997B5C58C5F1B3FF88295301`** = 开窗前值 ✓

**共享树零变异**：两个窗口都在**共享树内**做（宣告过），都用**开窗前的 worktree 值**作恢复判据（窗口 A 前 = `D3C2A869…`、窗口 B 前 = `81547380…`），恢复后逐字相等 ⇒ 零残留（唯一的残留是窗口 B 造出的 `%TEMP%\ruagent-reg-SHARED-DIR` 目录，**已按具体路径删除**，未用 glob）。

## 5. 本轮踩到并如实登记的两个环境陷阱（都不是本单的代码问题）

- **恢复文件会把「旧 mtime」一起恢复 ⇒ cargo 继续跑变异后的二进制（新踩到，值得入账）**：我用 `Copy-Item` 从备份恢复（备份早于变异，**mtime 更早**），于是 cargo 认为目标是最新的，**继续执行窗口 B 的变异二进制** ⇒ 我接下来的一次全包门禁出现 **5 条 registry 红，报错路径赫然是 `ruagent-reg-SHARED-DIR`**（即变异名）。**修法**：恢复后**把 mtime 刷新**（或 `cargo clean -p`）再取读数；**教训**：`sha256 相同` **不等于**「跑的就是这份字节」—— 与 t20 同源但更隐蔽（t20 是「绿读数属于旧 revision」，这里是「**红**读数属于旧二进制」）。
- **工具链 1.95.0 的 manifest 缺失（环境，非本单）**：`rustc -V` ⇒ `error: Missing manifest in toolchain '1.95.0-x86_64-pc-windows-msvc'`；而 `rustc +stable -V` = **同一个 `1.95.0 (59807616e)`** 正常。**我**的做法：三条门禁用 `$env:RUSTUP_TOOLCHAIN='stable'` 跑（**同一编译器版本、同一共享 target**，per-process 环境变量，不改机器设置），并**在此披露**；`+1.94.0` 也可用。⇒ 这条需要 capt/main 处置（它挡住全队的 Rust 门禁）。

## 6. 这一族还有多少（第 5 条判据：点名，含不在 inScope 的）

| 成员 | 位置 | 读数/状态 |
| --- | --- | --- |
| **夹具名含 `pid`、靠 pid 不重用来保证唯一** | `distill.rs::t329_tests::root`（本单已修）、`registry.rs::editor`（本单已修） | **6202 个** `ruagent-t329-*` 遗留目录、**6202/6202 含 `ruagent.db`** |
| 夹具名用**亚秒级时钟** | `crates/daemon/src/config.rs:685`、`crates/daemon/src/config.rs:760`（**不在本单 inScope**）、`crates/mock-agent/tests/e2e_daemon.rs:1467`（out of scope） | 本机时钟**细**（20000/20000 互异）⇒ 潜伏；但这几处与 `registry.rs` 当初同一个写法 |
| **遗留目录不清理**（同族的「不释放资源」） | `ruagent-kbapi-*` **2407**、`ruagent-del-test-*` **1013**（`chat.rs`，out of scope）、`ruagent-vp-*` 403、`ruagent-chat*` 405；`%TEMP%` 下 `ruagent-*` 共 **45498** 个目录 | 只是**计数读数**，未逐条验证；建议各模块属主补 `Drop` 清理 |
| 「固定 `sleep` 后立即断言」 | 全仓 `sleep(std::time::Duration::from_millis` **27 处**（lead，未逐条判） | 需属主逐条判：是等异步终局（⇒ 判据侧）还是等一件同步已完成的事（⇒ 噪声） |

## 7. 门禁（第 6 条判据，最终字节）

最终字节：`distill.rs` = **`AA018D6E844E08945EF00D9C040A6D290CD7FF4B46D0F11E9AA8FB6C2C9F4ED8`**、`registry.rs` = **`9932C4939379552689B5B5C553A632796FB05FD349CDCAEE2EDDE79E601F8F33`**。

| 命令（`RUSTUP_TOOLCHAIN=stable`，见 §5） | 读数 |
| --- | --- |
| `scripts/cargo-team.ps1 test -p ruagent-daemon` | **exit=0**，**ok-lines=11 · FAILED-lines=0 · panicked-lines=0**（含此前成片红的 5 条 `distill::t329_tests`） |
| `scripts/cargo-team.ps1 clippy -p ruagent-daemon --all-targets -DenyWarnings` | **exit=0**，`error-lines=0` |
| `cargo fmt --all --check` | **exit=0**，零 `Diff in` |

（`rustfmt` 直接调用会撞上 §5 的工具链问题，故我只以 `cargo fmt --all --check` 为准；它 exit 0。）

## 8. 未覆盖 / 不冒领

- **F2 的红我仍然没有「随机复现」的 before 读数**（按 C47：**未复现**，信息量为零）。我给出的是**机制 + 前提断言现场捕获 + 6202/6202 的遗留证据 + 修后成片红消失**；**不声称**「F2 已被我复现并修好」。
- **没有解释的部分如实留着**：verify2 那次红的**触发时刻**（哪次 pid 重用、哪个测试先跑）无法回放 —— 遗留目录已被本轮修法「先清再建」中和，事后无法复原当时那一轮的 pid 映射；我不编一个。
- **未做**：不清理那 45498 个遗留目录（**禁止 glob**，且非本单 inScope）；不改 `config.rs` / `mock-agent` 的同类写法（out of scope，已在 §6 点名）；不改 `chat.rs`（out of scope，t145 已单独收口）。
- **未启停活守护进程**（8787 = pid 14944 全程未触碰）、**未写活库**、未动 `panel/**`、`.github/**`、`scripts/**`、`crates/daemon/tests/**`。

## 9. 附：C49 的解法与被点到名的残留（**本节是 t154 completed 之后的文档追加，Rust 字节未动**）

**追加声明（t20 口径）**：本节是报告成文后按 captain 要求补写的**文档**，`distill.rs` / `registry.rs` 的字节**未再改动**（最终字节仍为 `AA018D6E…` / `9932C493…`，§7 的三条门禁读数仍属于它们）⇒ 无需重跑门禁。

### 9.1 `Copy-Item` 恢复带来的 mtime 陷阱：**具体命令**

**触发它的（我实际用的）恢复写法**（把备份拷回去；备份早于变异，于是**旧 mtime 被一起带回**）：

```powershell
Copy-Item "$env:TEMP\t154-fixed-registry.rs" crates\daemon\src\registry.rs -Force
```

**修复它（我实际用的、captain 要写进 C49 的那一条）**：

```powershell
foreach ($f in @('crates\daemon\src\registry.rs','crates\daemon\src\distill.rs')) {
    (Get-Item $f).LastWriteTime = Get-Date    # 刷新 mtime => cargo 必须重编
}
```

**两条更保守的等价物**（任选一条，代价不同）：① `cargo clean -p ruagent-daemon`（绝对可靠，但在共享 target 上要重编整个包，**会把代价转嫁给全队**，本代已量到 ~1m24s）；② 恢复时**不要**用 `Copy-Item` 的默认时间戳语义，改用「写入 + 立刻刷新 mtime」的同一段代码包装恢复动作（即把 9.1 第一条和第二条合成一个 `Restore-File` 函数）。

**怎么发现它（这是 C49 更该带走的一半）**：`cargo` 在目标最新时只打印 `Finished`，**不会**打印 `Compiling ruagent-daemon` ⇒ **「改了源码后某次门禁没有重编」就是这只陷阱的现形**；本次更硬的证据是**报错信息里的路径** —— `writing …\ruagent-reg-SHARED-DIR\agents.toml.tmp`（`SHARED-DIR` 是窗口 B 的**变异名**，源码里已不存在）。⇒ 判据：**红/绿读数里出现了「源码里已不存在的字面量」= 跑的不是这份字节**。

### 9.2 `%TEMP%` 残留目录：建议的清理命令（**本单未执行，也不属于本单 inScope**）

**读数（2026-10-04 全量列举）**：`%TEMP%` 下 `ruagent-*` 目录共 **45498** 个（全部目录 47957 个），其中 `ruagent-t329-*` **6202**（**6202/6202 含 `ruagent.db`**，本单根因）、`ruagent-kbapi-*` 2407、`ruagent-del-test-*` 1013、`ruagent-vp-*` 403、`ruagent-chat*` 405。

**先列后删，一次一族**（**不要**写成 `Remove-Item $env:TEMP\ruagent-*`：一次误伤正在运行的测试目录，本代已有 glob 误删先例）：

```powershell
# 1) 只列（read-only）
Get-ChildItem $env:TEMP -Directory -Filter 'ruagent-t329-*' | Measure-Object
Get-ChildItem $env:TEMP -Directory -Filter 'ruagent-t329-*' | Select-Object -First 5 FullName, LastWriteTime

# 2) 确认没有在跑的门禁/测试（你自己的或别人的）之后再删，仍然一族一次
Get-ChildItem $env:TEMP -Directory -Filter 'ruagent-t329-*' |
    Where-Object { $_.LastWriteTime -lt (Get-Date).AddHours(-1) } |   # 只碰一小时前的
    Remove-Item -Recurse -Force -ErrorAction Continue
```

**为什么本单不执行**：① `%TEMP%` 不是本单 inScope；② 这些目录**跨模块**（`kbapi`/`del-test`/`vp`/`chat` 各有属主），在途测试正在用它们；③ 我的夹具修法（先清再建）已经让**未来的 pid 重用不再有害**，清理只是回收磁盘/目录项，不是修复的必要条件。⇒ **建议各模块属主各清自己那一族**，或由一个明确的维护单统一清。

