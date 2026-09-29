# `scripts/cargo-team.ps1` 自己的契约：声称 vs 实际（t116，只读审计）（mem-core）

- 任务：t116（kind=work，只读审计）；attempt `5256a224-6392-435f-89a7-12a3977cef5b`
- **被审字节**：`scripts/cargo-team.ps1`，**205 行**，sha256 `427b1c8157a2dbadfc355fbb1d9e91002dc12d0695c0becf271fde58d60655c6`——**与 `HEAD` 同字节**（`git show HEAD:scripts/cargo-team.ps1 | sha256sum` 相同），审计前后**都没有被改动**（收尾再算一次：同一 sha256；`git status --porcelain -- scripts/` 为空）
- **唯一写入 = 本报告**；未改任何脚本/CI；未 push / dispatch / rerun / cancel / 建 tag；没碰 pid 79984、没写活库
- **所有构建读数都走包装脚本本身**（`powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 …`，团队文档里的调用形状）；包装外只准备了 3 个临时 crate（`%TEMP%\ruagent-t116\{warncrate,failcrate,slowcrate}`，源码 ~1 KB）

## 0 结论摘要

1. **锁、`-NoLock`、`-Nocapture`、退出码、`-CleanFirst` 的拒绝路径**：声称与实测**一致**（逐条见 §2/§3），包括「第二个调用者等待而不失败」「`-NoLock` 真的不互斥」「包装的 exit == 被包装命令的 exit」。
2. **8 条「声称 → 实测」差距**（§4）：其中 **W-1（`-CleanFirst` 的名字可以不是被门禁的包 ⇒ 门禁看起来是强制重读、实际来自缓存）** 与 **W-2（`-TargetDir` 缺值 ⇒ 静默回落到团队共享 target）** 是「会让读数看起来像证成」的那一类；W-4（`-DenyWarnings` 配 `check`/`test` 的失败看起来像编译/测试失败，实际**没有编译/没有跑测试**）是同类。
3. **没有发现任何一处**「包装脚本把失败报成成功」：所有失败路径都非零退出且不自称成功；未发现「吞掉退出码」的形状。
4. 锁是**句柄式**独占打开（文件 0 字节、从不写入、从不删除 ⇒ 无陈旧锁；进程死了由 OS 释放，实测续跑立即成功）。
5. 因为有差距，报告逐条给出**「谁会被它误导」的具体场景**（§4 每行的影响列）。

## 1 逐开关/行为清单（声称 → 实测）

凡「实测」列给出的是**真实调用的原始输出**（`[…]` 内为包装脚本原话）；DryRun 读数来自 `-DryRun`（它自己声明「nothing compiled, no lock taken」）。

| 开关/行为 | 声称（脚本头/注释） | 实测读数 | 差距 |
| --- | --- | --- | --- |
| `-Jobs N` | 传给 cargo 的有界并行度 | DryRun：`jobs=2`；**子进程自己打印**（build.rs 的 `cargo:warning`）：`CARGO_BUILD_JOBS=4 CARGO_TARGET_DIR=…\tt-slow`（默认 `-Jobs` 为 4） | 无（但缺值见 W-3） |
| `-Cpus N` | CPU 亲和掩码，留出最高编号的核 | 默认：`cpus=0-11/16`；子进程实测 `cargo` 的 `ProcessorAffinity = 4095`（=0xFFF=核 0-11） | 无（`-Cpus` 缺值/超大见 W-3/a13） |
| `-TargetDir PATH` | 覆盖 `CARGO_TARGET_DIR`（比较两棵树时用） | DryRun：`target=…\tt-clippy-deny`；子进程 env 实测 `CARGO_TARGET_DIR=…\tt-slow` | 无（**缺值见 W-2**） |
| `-NoLock` | 只用于「故意用另一个 target dir 并行」的罕见场景 | 在 A 持有团队锁时：`-NoLock` 调用**没有**等锁、0.2s 返回；`cargo-team] …` 与普通调用同形 | 无（**没有强制「必须是另一个 dir」** —— 见 W-9） |
| `-Nocapture` | 追加 `-- --nocapture` | DryRun：`cargo test -p ruagent-core -- --nocapture`；真实运行：失败测试的 stdout `WARNCRATE TEST STDOUT VISIBLE` **内联可见**且 exit=101 | 无 |
| `-DenyWarnings` | 追加 `-- -D warnings` | DryRun：`cargo clippy -p X --all-targets -- -D warnings`；clippy + 警告 crate：`error: unused variable` **exit=101**；去掉开关：`warning: unused variable` **exit=0** | **指令集不敏感 ⇒ W-4** |
| `-DryRun` | 打印命令行、不编译、不加锁 | 在 A 持有团队锁时 `-DryRun` **0.5s** 返回、exit=0；且打印 `target/jobs/cpus` 与最终 `cargo …` | 无 |
| `-CleanFirst [name]` | 在**同一个锁窗口**里先 `clean -p` 再跑门禁；名字可从 `-p <spec>` 解析；解析不到就拒绝 | 无 `-p` 且无名：**exit=2** + `[cargo-team] Refusing: a nameless -CleanFirst would build an empty 'cargo clean -p' SPEC and the gate would NOT run.`；匹配名：`clean -p ruagent-core (forced re-check)` + 本 run 的 `Checking ruagent-core v0.1.0` + `exit=0 elapsed=1.1s` | **名字不匹配时静默弱化 ⇒ W-1** |
| （无参数） | 用法即拒绝 | 无 cargo 参数：**exit=2** + usage 一行 | 无 |
| 团队锁 `%TEMP%\ruagent-team-build.lock` | 同一时刻只有一个团队构建；第二个调用者等待、不失败、不并行编译 | 见 §3①（两个真实进程，B 打印两次 `another team build is running (one compile at a time); waiting 10s ...`，20.9s 后才跑） | 无 |
| 优先级/亲和 | `BelowNormal` + 掩码，子进程继承 | 实测 A 运行中：`cargo` 两个进程 `PriorityClass=BelowNormal`、`AffinityMask=4095` | 无 |
| 「按原样转发 cargo 参数」（`-p -j -v -q -F -r`、字面 `--`） | 原样转发 | `-q -j 2 -F smoke` ⇒ `cargo check -p ruagent-core -q -j 2 -F smoke`；**字面 `--` 也真的原样到达**：`cargo test -p ruagent-core -- --list` | **与头部「PowerShell 会吃掉裸 `--`」的说法不符 ⇒ W-6** |
| 大小写 | （未声明） | `-targetdir X` 生效（PowerShell `-eq` 大小写不敏感） | 见 W-7（低） |

## 2 五条重点契约（原始读数）

### ① 锁：覆盖整次构建，且真的互斥

**声称**：独占锁文件，同一时刻最多一次团队构建；第二个调用者**等待**；锁「覆盖整次构建」。

**读数（两个真实包装进程）**：

```
--- A: check --manifest-path slowcrate -TargetDir …\tt-slow   （build.rs 睡 25s，持有团队锁）
      A running = True
--- C1: 运行中的子进程
      Id ProcessName PriorityClass AffinityMask
   14460 cargo        BelowNormal         4095
   40456 cargo        BelowNormal         4095
--- C2: -DryRun 在 A 持锁时： exit=0 elapsed=0.5s (A still running: True)
--- C3: B1 = 第二个真实调用：  B1 已打印等待行 = True
[c-A-slow]  exit=0 elapsed=25.9s   waited-for-team-lock=False
[c-B1-waiter] exit=0 elapsed=20.9s   waited-for-team-lock=True
      | [cargo-team] another team build is running (one compile at a time); waiting 10s ...
      | [cargo-team] another team build is running (one compile at a time); waiting 10s ...
      | [cargo-team] target=…\tt-B1 jobs=4 cpus=0-11/16 priority=BelowNormal
      | [cargo-team] cargo check --manifest-path …\warncrate\Cargo.toml
      | [cargo-team] exit=0 elapsed=0.5s
[c-C6-after] exit=0 elapsed=0.9s   （A 结束后立即拿到锁）
```

- **互斥**：真（B1 等了 20.9s = 2×10s + 0.5s 构建）。
- **覆盖范围**：`-CleanFirst` 的 clean 与随后的门禁都在 `try{}` 内、锁句柄在 `finally` 里 Dispose ⇒ **结构上覆盖整次构建**（含 clean）；实测只证明「A 在构建期间任何时刻，第二个调用者都在等」，没有单独观察到「clean 阶段被等待」的瞬间（见 §5 未测）。
- **`-NoLock`**：在 A 持锁时**没有**任何等待、0.2s 完成（`waited-for-team-lock=False`）⇒ **真的放弃互斥**，不是「还是等但不说」。
- **锁文件**：`Length=0`、`LastWriteTime` 整个审计期间不变（从不写入、从不删除）；A 结束后 C6 立即成功 ⇒ 句柄式锁**没有陈旧锁问题**（进程死亡由 OS 释放）。
- **90 分钟超时 ⇒ `exit 75`**：**未构造**——构造它需要把团队共享锁占住 90 分钟，会堵住所有成员（见 §5）。

### ② `-CleanFirst`：解析规则、清的是谁、谁被迫重编

**解析规则（实测）**：① `-CleanFirst <name>`（下一个 token 不以 `-` 开头）⇒ 用该名字；② 裸 `-CleanFirst` ⇒ 从 cargo 参数里的 **每一个** `-p`/`--package` 解析；③ 两者都没有 ⇒ **exit 2**（原文见 §1）。多 `-p` 时三个都清：`(under the same lock) cargo clean -p ruagent-core, -p ruagent-memory`（DryRun 读数）。

**清的是哪个 target**：**当前生效的 `CARGO_TARGET_DIR`**（实测：A/B 用私有 `-TargetDir`，b07/b08 用团队共享 `…\ruagent-team-target`——日志里 `target=` 就是它）。

**谁被迫重编（匹配 vs 不匹配的对照，同一条门禁）**：

```
[b07-clean-matching]  exit=0 elapsed=1.9s   args: check -p ruagent-core --all-targets -CleanFirst ruagent-core
      | [cargo-team] cargo check -p ruagent-core --all-targets
      | [cargo-team] clean -p ruagent-core (forced re-check)
      |     Checking ruagent-core v0.1.0 (…\crates\core)          <-- 本 run 的重读证据
      | [cargo-team] exit=0 elapsed=1.1s

[b08-clean-mismatch]  exit=0 elapsed=1.3s   args: check -p ruagent-core --all-targets -CleanFirst ruagent-policy
      | [cargo-team] cargo check -p ruagent-core --all-targets
      | [cargo-team] clean -p ruagent-policy (forced re-check)
      | (没有任何 Checking ruagent-core 行)
      | [cargo-team] exit=0 elapsed=0.3s
```

⇒ **W-1**：`-CleanFirst` 的名字与门禁的 `-p` 无关时不报错、不警告，门禁**从缓存绿**；而两次运行的日志措辞完全相同（都有 `clean -p X (forced re-check)` 与 `exit=0`）。

**类型错误路径（红得清楚）**：`-CleanFirst no-such-pkg-xyz` ⇒
```
      | [cargo-team] clean -p no-such-pkg-xyz (forced re-check)
      | error: package ID specification `no-such-pkg-xyz` did not match any packages
      | [cargo-team] cargo clean -p no-such-pkg-xyz exited 101; not running the gate
      exit=101
```
⇒ 明确说「没跑门禁」并返回 cargo 的 101，**不是假绿**。

### ③ `-DenyWarnings`：真的传下去（并且真的会红），但不是「拒绝警告」的通用开关

**传下去**（DryRun 原话）：`[cargo-team] cargo clippy -p ruagent-core --all-targets -- -D warnings`；**真的会红**（`warncrate` 有 `let unused_variable = 41;`）：

```
clippy + -DenyWarnings : cargo clippy --manifest-path …\warncrate\Cargo.toml -- -D warnings
                         error: unused variable: `unused_variable`   ⇒ exit=101
clippy 无开关          : warning: unused variable: `unused_variable`  ⇒ exit=0
```

**但指令集不敏感**（**W-4**）：

| 命令 | 实测 |
| --- | --- |
| `check … -DenyWarnings` | cargo 自己拒绝参数：`error: unexpected argument '-D' found` ⇒ **exit=1**（`cargo check` 不接受 `--` 之后的 rustc 参数） |
| `test … -DenyWarnings` | libtest 拒绝：`error: Unrecognized option: 'D'`，随后 cargo `error: test failed, to rerun pass --lib` ⇒ **exit=101，且一个测试都没跑**（日志里没有 `test result:` 行） |

### ④ `-Nocapture`：真的是 libtest 的 `--nocapture`，且**不丢退出码**

- DryRun：`cargo test -p ruagent-core -- --nocapture`。
- 真实（失败测试）：`WARNCRATE TEST STDOUT VISIBLE` 内联出现，`test result: FAILED. 1 passed; 1 failed`，`[cargo-team] exit=101` ⇒ **失败退出码存活**。
- 对照（不加开关，同一 crate）：`test result: FAILED. …`、**没有** 那行 stdout、同样 `exit=101` ⇒ 开关的作用正是「让测试 stdout 可见」。
- 包装脚本本身**从不重定向/捕获输出**（`& cargo @cargoArgs`，没有管道）⇒ 它在任何情况下都不会因为捕获而丢输出或丢码。

### ⑤ 退出码：包装的 exit == 被包装命令的 exit

| 场景 | cargo 的码 | 包装脚本的 `exit=` | 父进程观察到的码 |
| --- | --- | --- | --- |
| 编译错误（`failcrate`） | 101 | `[cargo-team] exit=101 elapsed=0.8s` | **101** |
| 测试失败（`-Nocapture`） | 101 | `[cargo-team] exit=101` | **101** |
| `-Jobs 0`（cargo 拒绝） | 101 | `[cargo-team] exit=101` | **101** |
| `cargo clean -p` 失败（CleanFirst 拼错） | 101 | `[cargo-team] exit=101; not running the gate` | **101** |
| 无 cargo 参数 / 裸 `-CleanFirst` 无 `-p` | —— | 自己的 **2** + 明确 refuse 文案 | **2** |
| `-Jobs abc` | —— | PowerShell 转换错误（`Cannot convert value "abc" to type "System.Int32"`） | **1**（W-8：不是文档化代码，且不是包装自己的文案） |

**没有发现任何一处「失败被报成 0」或「打印成功」**（含 `exit 101` 的四条路径）。

## 3 差距清单：声称 → 实测 → 差距 → **谁会被它误导**

| # | 严重度 | 声称 | 实测 | 差距 | **谁会被它误导（具体场景）** |
| --- | --- | --- | --- | --- | --- |
| **W-1** | **高** | `-CleanFirst` 保证「clean 与门禁在同一个锁窗口里，因此门禁真的重读」 | b07 vs b08：同一条门禁，`-CleanFirst` 名字匹配 ⇒ 有 `Checking ruagent-core`；名字不匹配 ⇒ **无该行、0.3s、exit=0** | 名字与 `-p` 无关时不报错不警告；两次日志措辞相同 | ① 改了被测 crate（把 `-p ruagent-memory` 改成 `-p ruagent-graph`）却沿用旧的 `-CleanFirst ruagent-memory` 的成员；② 复制上一条命令只改一部分的人；③ 任何人按「日志里有 `clean -p … (forced re-check)` + `exit=0`」判定这道门禁跑过了。**该绿是缓存绿，不是重读绿**（脚本自己的注释说绿只有在出现本 run 的 `Checking <crate>` 行时才有效——这条纪律**有效但没被强制**） |
| **W-2** | 中 | `-TargetDir PATH` 覆盖 target 目录；比较两棵树时必须给另一棵树单独的 dir（否则 cargo 会把第一棵树的 rlib 交给第二棵，closure 7.1） | `-TargetDir` 缺值时**静默**回落：`target=C:\…\Temp\ruagent-team-target`（共享目录），exit=0，无警告 | 缺值 = 静默共享目录 | 想按文档做 before/after 比较、但路径变量展开成空（或 `-TargetDir` 后跟了另一个 `-`开关）的人：他用**团队共享缓存**编第二棵树，读到的可能是**第一棵树的产物** ⇒「two-tree 比较」的结论本身错，而日志里的 `target=` 是唯一线索 |
| **W-3** | 中 | `-Jobs`/`-Cpus` 调并行度 | `-Jobs`（缺值）⇒ `jobs=0` ⇒ cargo `error: jobs may not be 0` ⇒ exit=101（响）；`-Cpus`（缺值）⇒ `cpus=0-0/16` = **1 个核**，exit=0（**静默**）；`-Cpus 999` ⇒ `cpus=0-14/16`（clamp 到 logical-1） | 缺值处理不一致：一个响、一个静默 1 核 | 写 `… -Cpus $env:N` 而变量为空的人：构建变成单核、**不报错**，他会以为「这次怎么这么慢」而不会想到参数丢了 |
| **W-4** | 中 | `-DenyWarnings` = 把 `-D warnings` 传下去 | `check`：cargo `error: unexpected argument '-D' found` exit=1；`test`：libtest `error: Unrecognized option: 'D'` + cargo `error: test failed`，**exit=101 且没有 `test result:` 行** | 开关不区分指令集；文档把它写成通用追加尾 | 给 `test` 加 `-DenyWarnings` 的人（想「顺手也拒绝警告」）：他看到 `error: test failed` + 101，**会读成「有测试失败」**；实际上 libtest 死在参数解析上、**一个测试都没跑**（唯一的分辨方法是按「有 `test result:` 行才算跑了 target」这条纪律读数）。`check` 的情况则是把「想看警告」变成 cargo 参数错 |
| **W-5** | 低 | （未声明） | 日志顺序：先 `[cargo-team] cargo <门禁>`，再 `[cargo-team] clean -p … (forced re-check)`，然后才是门禁的输出 | 声明的命令在被清的动作之前 | 只扫日志前几行的人会把「宣布的门禁」当成已经执行；不影响退出码与证据（`exit=` 行仍在最后） |
| **W-6** | 低 | 「`cargo test … -- --nocapture` 与 `clippy … -- -D warnings` 不能经 `powershell -File` 原样转发（PowerShell 吃掉裸 `--`），所以有这两个开关」 | 从本审计的调用方（pwsh 起 `powershell -File`）：**字面 `--` 原样到达**：`cargo test -p ruagent-core -- --list` | 声称的限制在本调用路径下**不成立** | ① 以为「不写 `-Nocapture` 就无法给 libtest 传参」而绕路的人；② 反过来，依赖该说法以为「传了 `--` 也没用」的人。**实战影响小**（两个开关仍然好用），但它是「注释里的因果与实际不符」 |
| **W-7** | 低 | （未声明） | `-targetdir X` 与 `-TargetDir X` 等效（`-eq` 大小写不敏感） | 开关大小写不敏感 | 少见：若某个 cargo 子命令的单横线参数与开关同形（大小写不同），会被**包装吃掉**而不是转发给 cargo（例如 `-Jobs`/`-Cpus`/`-NoLock` 这几个人造名字与 cargo 真实参数不冲突，所以现状安全） |
| **W-8** | 低 | （未声明） | `-Jobs abc` ⇒ PowerShell `Cannot convert value "abc" to type "System.Int32"`，exit=1 | 参数校验错误不是包装自己的文案/码 | 读日志的人只看到一段 PowerShell 异常（而不是「cargo-team: -Jobs 需要一个数字」）；不影响正确性（非零） |
| **W-9** | 低 | 「`-NoLock` 只用于**故意用另一个 target dir**的场景（rare）」 | `-NoLock` **不校验** dir 是否与别人相同；实测两个 `-NoLock` 构建落在**同一个** `-TargetDir` 上：都 exit=0，其中后来者打出 cargo 自己的 `Blocking waiting for file lock on build directory`（12.4s 后完成） | 无强制，只有 cargo 的目录锁兜底 | 想「并行快点」而随手 `-NoLock` 且忘了换 dir 的人：**cargo 的 build-dir 锁会串行化**（不会损坏），他以为并行其实没并行；真正的风险是 headroom（多份编译器争 CPU/内存）而不是数据损坏 |

## 4 负控原文（能红的控制）

1. **被包装命令必然失败** ⇒ 包装必须非零且**不得**打印成功：`failcrate` ⇒ `error[E0425] … error: could not compile 'failcrate'` + `[cargo-team] exit=101`，父进程 **101**；`test`（含 `-Nocapture`）⇒ `test result: FAILED. 1 passed; 1 failed` + `exit=101`。**没有出现任何「成功」字样**。
2. **裸 `-CleanFirst` 且无 `-p`** ⇒ 按文档拒绝：**exit=2** + `[cargo-team] Refusing: a nameless -CleanFirst would build an empty 'cargo clean -p' SPEC and the gate would NOT run.`（`-DryRun` 版与真实版都测了）。
3. **无 cargo 参数** ⇒ **exit=2** + usage。
4. **并发**：两个真实包装进程 ⇒ 只有一个在跑，另一个打印等待并在释放后继续（§2①）。
5. **`-NoLock` 的反向控制**：同一时刻**没有**等待 ⇒ 证明上面那条互斥不是「碰巧」。
6. **`-DryRun` 不加锁的反向控制**：A 持锁期间 DryRun 0.5s 返回。
7. **`-CleanFirst` 名字匹配 vs 不匹配**：同一门禁下 `Checking ruagent-core` 行有/无（§2②）——这是 W-1 的**能红负控**。

**无法构造的（明写原因，不跳过）**：
- **90 分钟超时 ⇒ `exit 75`**：构造它要让团队共享锁被占满 90 分钟，会堵住全体成员（且 `-NoLock` 不能用于这条代码路径）。只做了代码阅读与「等待消息 + 10s 轮询」的实测。
- **锁文件无法打开（非 IOException 的其他异常，如权限/占位为目录）**：把 `%TEMP%\ruagent-team-build.lock` 变成目录或只读文件**会破坏所有成员的构建**，属于污染共享环境 ⇒ 不做。
- **`exit $LASTEXITCODE` 在 `$LASTEXITCODE` 为空时的行为**（cargo 不存在/未执行）：让 cargo 不在 PATH 上需要改环境，而 `$ErrorActionPreference='Stop'` 下 `& cargo` 是终止性错误（应 exit 1）——只做了代码阅读，未构造。

## 5 方法学备注（本次审计自己的读数陷阱）

1. **`Start-Process -PassThru` + `WaitForExit(timeout)` 不回填 `ExitCode`**：探针 C 打印出 `exit=`（空）。**修法**：改用包装脚本自己的 `[cargo-team] exit=N` 行 + 探针 A/B 里 `& powershell …; $LASTEXITCODE` 的父进程读数（两者一致地给出 101/2）。**这正是本单要审的形状：一个「看起来像读数」的空值**。
2. **`Select-String` 只读 stdout 会漏掉 cargo 的全部进度与警告**（cargo 写 stderr）：第一遍我因此没看到 `Blocking waiting for file lock` 与 build.rs 的 `cargo:warning` 行；读 `.err` 后才拿到（§2①的 C5、子进程 env）。
3. **两个自造控件的设计错误（延续 t105/t115 的「控制红/绿 ≠ 控制对了」）**：① 探针 C 里我先给 A 写了同步等待的调用，发现后改成异步；② 「`-Nocapture` 会丢退出码吗」这一问，我先用 `check` 造控制（结果红在 cargo 参数错，不是退出码问题），改成 `test` 才测到真正的形状。

## 6 不覆盖什么（第 19 条）

- **不覆盖 cargo 自身的正确性**：`clean -p` 到底删了哪些文件、增量指纹怎么算、`-- -D warnings` 在 clippy 里的完整语义，都按 cargo 的行为接受；本报告只审**包装脚本的契约**与其**可观察后果**。
- **不覆盖磁盘/文件系统故障**：target 目录满盘、权限、网络盘、被杀毒软件锁文件等；也不覆盖 `%TEMP%` 被清理导致缓存消失的性能后果。
- **不覆盖 GitHub runner（Linux）上的行为差异**：`ProcessorAffinity`/`PriorityClass`/`Process` 这些是 Windows API（Linux 上 `catch` 会打印 `could not set affinity/priority …; continuing`）；本审计全在 Windows 本机。
- **不覆盖 PowerShell 版本矩阵**：走的是文档里的 `powershell -NoProfile -ExecutionPolicy Bypass -File`（Windows PowerShell 5.1），未测 pwsh 7 直接执行 `.ps1`、`-Command`、或从 cmd.exe 调用时 `--` 的转发差异（W-6 的结论**只对本审计的调用路径**成立）。
- **不覆盖并发下的正确性**：只验证了「同一时刻只有一个团队构建」，没有验证「两个 `-NoLock` 构建写同一个 target 目录」的**产物正确性**（cargo 的 build-dir 锁会串行化，但我没有做产物比对）。
- **不覆盖其它脚本**：`scripts/ruagent-daemon.ps1`、`panel/scripts/build-panel.mjs`、CI 守卫等另属他单。
- **这次审计没有改任何脚本**：`scripts/cargo-team.ps1` 审计前后 sha256 相同（`427b1c81…`），`git status --porcelain -- scripts/` 为空；唯一写入是本报告。

## 7 纪律与残留

- **只读**：唯一写入 = 本报告。未改 `scripts/cargo-team.ps1` 或任何构建/CI 脚本；未 push / dispatch / rerun / cancel / 建 tag；未写活库；未碰 pid 79984。
- **临时 target 按具体路径自清**（不用通配）：删除的 13 个目录（清前体积）——`tt-warn-deny 0MB`、`tt-warn-nodeny 0.02`、`tt-fail 0.38`、`tt-test-nocap 4.38`、`tt-test-cap 4.38`、`tt-test-deny 4.38`、`tt-jobs0 0`、`tt-slow 3.34`、`tt-B1 0.02`、`tt-B2-nolock 0.02`、`tt-C6 0.02`、`tt-clippy-deny 0`、`tt-clippy-ok 0.03`，**共 16.97 MB**；逐条 `exists=False` 已复核。
- **保留的复核材料**（都在 `%TEMP%\ruagent-t116`，共 ~70 KB）：3 个临时 crate 源码（`warncrate`/`slowcrate`/`failcrate`）+ 3 个探针脚本（`probeA/B/C.ps1`）+ 38 个日志（含 `.err`，原始输出）。
- **对共享环境的已知副作用（如实记账）**：`b07` 在团队共享 target 里 `clean -p ruagent-core` 后立刻重编了它（1.1s）；`b08` 清了 `ruagent-policy` 的产物且**没有再编它**（下一位成员的构建会重编 policy）。两者都是 `-CleanFirst` 的**正常语义**，但我选 policy 作为「被清但不被门禁」的那个包就是为了让代价最小。
- 团队锁文件 `%TEMP%\ruagent-team-build.lock` 全程 0 字节、mtime 未变（未被写入/删除）；收尾无 `cargo`/`rustc` 残留进程。

---

# t117 修复面：三处可误导读数（W-1 / W-2 / W-4）+ W-3/W-5/W-6/W-7/W-8/W-9（mem-core）

- 任务：t117（kind=work）；attempt `3edd8607-3e4a-4199-aeaa-34287c529506`
- inScope：`scripts/cargo-team.ps1` + 本文件（**未另开文件**）；`git status --porcelain -- scripts/ .github/ crates/ panel/` = 仅 ` M scripts/cargo-team.ps1`
- **改前**（t116 审计的字节）：205 行，sha256 `427b1c8157a2dbadfc355fbb1d9e91002dc12d0695c0becf271fde58d60655c6`
  **改后**：sha256 `af311f43111db4ef556d63ad3fcc7b8ceeb7c7328f4a5cd90a00a4399a8b31d5`，`git diff --stat` = **1 file, +140, −14**；PowerShell `Parser::ParseFile` = **OK（无语法错）**

## 8 改了什么（一屏总览）

| # | 改动 | 判据 |
| --- | --- | --- |
| **W-1** | `clean -p X` 的措辞按「X 是不是本门禁构建的包」分叉；任一被清的包不在门禁集合里时打印 **`NOT VERIFIED BY THE CLEAN`** 横幅（点名门禁构建什么 / 本 run 清了什么 / 未覆盖什么），并在门禁跑完后补一行「去哪里确认」 | 成对/三方日志必须可区分，且不匹配的那次不得被读成「门禁跑过」 |
| **W-2** | `-TargetDir` 缺值/空值/后跟 `-`开关 ⇒ **exit 2 + 点名原因** | 缺值 ⇒ 非零 + 点名；真路径 ⇒ 正常 |
| **W-3/W-8** | `-Jobs`/`-Cpus` 缺值、非整数、`< 1` ⇒ **exit 2 + 点名**（用 `[int]::TryParse`，不再抛 PowerShell 异常） | 同上；`-Cpus 999` 仍按文档 clamp（见 §15） |
| **W-4** | `-DenyWarnings` 只允许 `clippy`/`rustc`；其它子命令 ⇒ **exit 2 + 点名**（原文写明它为什么只对 clippy 有意义） | `check`/`test`/`build` ⇒ 非零 + 点名；`clippy`/`rustc` ⇒ 仍工作 |
| **W-5** | 声明的门禁命令行从「clean 之前」移到 **clean 之后、门禁之前** | 日志顺序可读 |
| **W-6** | 删掉头部「裸 `--` 会被 PowerShell 吃掉」的断言，改成实测事实（字面 `--` 原样到达；两个开关是便利而非补救） | 文档与事实一致 |
| **W-7** | 头部写明开关大小写不敏感（`-dryrun` 可用；今天无 cargo 单横线开关与它们冲突） | 登记 |
| **W-9** | `-NoLock` 且**没有** `-TargetDir` ⇒ 打印一行 `NOTE:`（说明 cargo 目录锁只保证串行、真正代价是 CPU/内存） | 登记 + 不再静默 |

## 9 W-1（高）：`forced re-check` 现在只在它成立时说

**选「横幅」而不是「非零退出」的理由**：清**依赖**来迫使被门禁的包重编是**正当用法**（§9 的 f03 实测证明它真的会让 cargo 重查被门禁的包）；拒绝会破坏这个用法。而 W-1 的错是**声称**错了，不是「红绿」错了 ⇒ 让日志说真话，并把「怎么确认」交给读者（门禁自己的读数规则就是看 `Checking <crate>`）。

**三方成对读数（同一条门禁，只有 `-CleanFirst` 名字不同；全部走包装脚本、共享 target）**：

| 运行 | 命令 | 横幅 | `Checking ruagent-core` | `Checking 被门禁包` | exit / elapsed |
| --- | --- | --- | --- | --- | --- |
| **f01 匹配** | `clippy -p ruagent-core --all-targets -CleanFirst ruagent-core` | **False** | **1** | core=1 | 0 / **1.7s** |
| **f02 不匹配且无关** | `check -p ruagent-policy --all-targets -CleanFirst ruagent-core`（policy **不**依赖 core） | **True** | 0 | policy=**0** | 0 / **0.5s** |
| **f03 不匹配但是真依赖** | `check -p ruagent-memory --all-targets -CleanFirst ruagent-core`（memory 依赖 core） | **True** | 1 | memory=**1** | 0 / **2.6s** |

原文（f01 / f02 的关键行）：

```
f01: [cargo-team] clean -p ruagent-core (forced re-check: 'ruagent-core' IS a package this gate builds)
     [cargo-team] cargo clippy -p ruagent-core --all-targets          <-- W-5: 现在印在 clean 之后
         Checking ruagent-core v0.1.0 (…\crates\core)
     [cargo-team] exit=0 elapsed=1.7s

f02: [cargo-team] clean -p ruagent-core (NOT a package this gate builds: gate builds ruagent-policy)
     [cargo-team] ============ NOT VERIFIED BY THE CLEAN ============
     [cargo-team] This run cleaned package(s) other than the ones it gates, so whether the gate was re-read is NOT established by the clean alone:
     [cargo-team]   gate builds        : -p ruagent-policy
     [cargo-team]   this run cleaned   : ruagent-core
     [cargo-team]   cleaned, not gated: ruagent-core
     [cargo-team] A green below is a genuine re-read ONLY if the clean invalidates a gated package: cleaning a DEPENDENCY of it does, cleaning an unrelated package does NOT. Confirm by looking for 'Checking <gated crate>' in THIS run's output; if no such line appears, the green came from cargo's cache and is NOT a re-read (t116 W-1).
     [cargo-team] ===================================================
     [cargo-team] cargo check -p ruagent-policy --all-targets
     [cargo-team] re-read check: this run cleaned ruagent-core; a re-read of -p ruagent-policy shows up above as 'Checking <crate>'. No such line => cache green, not a gate (t116 W-1).
     [cargo-team] exit=0 elapsed=0.5s
```

**改前 vs 改后（t116 基线是同一对命令）**：改前匹配与不匹配都印 `clean -p X (forced re-check)`、都要读者自己去数 `Checking` 行（不匹配那次 0 行、0.3s、exit 0）；改后两次的 **措辞可区分**，不匹配那次多了横幅 + 事后提醒，且横幅**明确说出「cleaning a DEPENDENCY does, cleaning an unrelated package does NOT」**——f03 与 f02 正是这两种情形的实测对照。

**DryRun 也说实话**（同一条规则）：`-DryRun … -CleanFirst ruagent-core`（门禁 `-p ruagent-policy`）⇒ `(under the same lock) clean -p ruagent-core (NOT a package this gate builds -- the run will print NOT VERIFIED; t116 W-1)`；裸 `-CleanFirst` 且无 `-p` ⇒ **仍 exit 2**（t88/R-5 的拒绝未被削弱）。无 `-p` 的门禁（`clippy --all-targets -CleanFirst ruagent-core`）⇒ 也进横幅路径，理由是「门禁构建 workspace 默认集合，单个被清的包未被确认为门禁构建的包」。

## 10 W-2 / W-3 / W-8：缺值一律拒绝（并附一个新的子发现）

| 情形 | 改前实测（t116） | 改后实测（t117） |
| --- | --- | --- |
| `-TargetDir`（无值） | **静默**回落共享 target：`target=…\ruagent-team-target`，exit 0 | **exit 2**：`-TargetDir needs a path (got ''). Refusing: a missing value used to fall back to the SHARED team target silently, … (t116 W-2).` |
| `-TargetDir -NoLock` | **把后一个开关当成路径吞掉**（`noLock` 从未置位）——这是本单新发现的**子缺口 W-2b** | **exit 2** 并点名 `got '-NoLock'`（既修缺值，也修吞开关） |
| `-TargetDir ''`（变量展开为空） | 同第一条（静默共享） | **exit 2**（同第一条原文） |
| `-TargetDir <真路径>` | 正常 | **正常**（DryRun：`target=<真路径>`） |
| `-Jobs`（无值） | `jobs=0` ⇒ 交给 cargo ⇒ `error: jobs may not be 0`（exit 101，响但晚） | **exit 2** + `-Jobs needs a positive integer (got '')` |
| `-Jobs 0` | 同上 | **exit 2** + `(got '0')` |
| `-Jobs abc` | **PowerShell 异常** `Cannot convert value "abc" to type "System.Int32"`，exit **1**（非包装文案） | **exit 2** + 包装自己的文案（`(got 'abc')`） |
| `-Cpus`（无值） | `cpus=0-0/16` = **1 个核**，exit 0（**静默**） | **exit 2** + `-Cpus needs a positive integer (got '')` |
| `-Cpus 0` | 1 核（静默） | **exit 2** |
| `-Jobs 2 -Cpus 4` | 正常 | **正常**（`jobs=2 cpus=0-3/16`） |

## 11 W-4：`-DenyWarnings` 现在指令集敏感（拒绝而不是产出可误读的红）

**为什么不选「真支持」**：让 `check`/`test` 真正拒绝警告只能用 `RUSTFLAGS=-D warnings`，而改 RUSTFLAGS 会**改变每个 crate 的指纹** ⇒ 在团队共享 target 里重编/重缓存整棵树（一次静默的大副作用）。所以按验收允许的另一条路：**明确拒绝**。

| 命令 | 改前实测（t116） | 改后实测（t117） |
| --- | --- | --- |
| `check … -DenyWarnings` | cargo `error: unexpected argument '-D' found` ⇒ exit **1** | **exit 2** + `-DenyWarnings does not apply to 'cargo check' -- it appends '-- -D warnings', which only clippy/rustc accept. …`（原文里带着 t116 的两条实测） |
| `test … -DenyWarnings` | libtest `error: Unrecognized option: 'D'` + cargo `error: test failed` ⇒ exit **101** 且 **0 行 `test result:`**（一个测试都没跑，读起来像「有测试失败」） | **exit 2** + 同一段文案（点出这个误读形状） |
| `build … -DenyWarnings` | （未在 t116 单测，同族） | **exit 2** + 点名 |
| `clippy … -DenyWarnings` | 真传下去：警告 ⇒ `error: unused variable` exit **101**；无开关 ⇒ `warning:` exit **0** | **完全不变**（DryRun：`cargo clippy -p ruagent-core --all-targets -- -D warnings`） |
| `rustc … -DenyWarnings -- --emit=metadata` | —— | **允许**（`cargo rustc … -- --emit=metadata -- -D warnings`） |
| `-Nocapture` + 失败测试 | exit 101 + 测试 stdout 内联 | **完全不变**（r07：`WARNCRATE TEST STDOUT VISIBLE` + `test result: FAILED. 1 passed; 1 failed` + `exit=101`） |

## 12 W-5 / W-6 / W-7 / W-9：逐条「修 or 登记」

- **W-5（修）**：门禁命令行移到 clean 之后（f01 日志顺序即读数）。核实过这不改变任何退出码或门禁行为，只改打印顺序。
- **W-6（修文档）**：删掉「PowerShell 会吃掉裸 `--`」的断言，替换为实测事实（`... test -p ruagent-core -- --list` ⇒ `cargo test -p ruagent-core -- --list`，且仓库里真的有人这样用：`-- --ignored --nocapture --test-threads=1` 的形状在成员记录里出现）。两个开关保留，但定位改成「便利与自我说明」。
- **W-7（登记）**：头部写明开关大小写不敏感（`-dryrun` 实测可用 ⇒ 行为未被本次改动改变），并说明今天没有 cargo 单横线开关与之冲突。
- **W-9（修一半 + 登记剩余）**：`-NoLock` 且未给 `-TargetDir` ⇒ 打印 `NOTE:`（原文见 §8）；**不做拒绝**的理由：它是既有的、明确的「我知道我在做什么」通道，拒绝会破坏「故意并行」的合法用法；cargo 自己的 build-dir 锁（`Blocking waiting for file lock on build directory`，t116 C5 实测）已经保证同目录不会损坏，NOTE 指出真实代价是 headroom。

## 13 向后兼容：逐条核过的现有调用形状

**仓库里没有任何 CI 调用点**：`grep -rn cargo-team .github/` = **无命中**（三份 workflow 都不走包装脚本）⇒ 本单最大的风险面其实很小。`AGENTS.md` 写的是裸 `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test --workspace`，也不经包装。

**逐条 DryRun 核对（argv 必须与改前逐字相同，全部 exit 0）**——形状取自成员文档与历史记录里的真实用法：

| # | 调用形状（报来源） | 改后 argv | exit |
| --- | --- | --- | --- |
| e01 | `test -p ruagent-store`（distill-log-impl §命令） | `cargo test -p ruagent-store` | 0 |
| e02 | `clippy -p ruagent-store --all-targets -DenyWarnings`（同上） | `… --all-targets -- -D warnings` | 0 |
| e03 | `test -p ruagent-store -Nocapture`（同上） | `… -- --nocapture` | 0 |
| e04 | `clean -p ruagent-daemon`（distill-writeside §142） | `cargo clean -p ruagent-daemon` | 0 |
| e05 | `test -p ruagent-daemon --lib distill -Nocapture`（同上） | `… --lib distill -- --nocapture` | 0 |
| e06 | `check -p ruagent-daemon --all-targets`（同上） | 逐字 | 0 |
| e07 | `clippy -p ruagent-daemon --all-targets -DenyWarnings --verbose`（同上 §148，**开关在中间**） | `… --all-targets --verbose -- -D warnings` | 0 |
| e08 | `test -p ruagent-graph --test temporal -Nocapture`（graph-deferred-audit §127） | 逐字 + 尾 | 0 |
| e09 | `test --release -p ruagent-store`（脚本头用法行） | 逐字 | 0 |
| e10 | `test -p ruagent-core -- --list`（字面 `--`，W-6 的读数） | 逐字（`--` 未被吃） | 0 |
| e11 | `test -p ruagent-knowledge --test retrieval-gold-copy -Nocapture -TargetDir <路径>`（V-A 记录） | 逐字（`target=<路径>`） | 0 |
| e12 | `test -p ruagent-graph --test live-after -- --ignored --nocapture --test-threads=1`（V-C 记录） | 逐字 | 0 |
| e13 | `run --manifest-path <tmp>\Cargo.toml --bin va-probe`（V-A 记录） | 逐字 | 0 |
| e14 | （无参数） | usage 一行 | **2**（未变） |
| e15 | `-dryrun check -p ruagent-core`（大小写） | 仍识别为 `-DryRun` | 0 |

**破坏性改动**：只有 W-3/W-4/W-2 那三类**新的拒绝**——它们只作用于此前会**静默走错**或**产出可误读红**的输入（缺值/0/非数字/`-DenyWarnings` 配非 clippy 子命令）。已核过：三份 workflow 不调用本脚本；文档/history 里的 14 条形状全部照旧工作（上表）。**没有一条合法调用被拒绝。**

## 14 回归：t116 的「好消息」逐条仍有读数

| t116 结论 | t117 改后读数 |
| --- | --- |
| 锁真互斥、第二个调用者等待 | A=slowcrate（build.rs 睡 13s，持锁）⇒ `[r10] elapsed=13.8s`；B1（真实第二调用）⇒ `waiter-printed=True`、`[r11] elapsed=10.9s` ⇒ **仍有互斥** |
| `-NoLock` 真放弃互斥 | A 持锁时 B2（`-NoLock`）⇒ `waiter-printed=False`、构建 `0.2s` ⇒ **仍绕过** |
| `-DryRun` 不加锁 | A 持锁时 `-DryRun` ⇒ **exit=0 elapsed=0.3s** ⇒ 仍不加锁 |
| 句柄式锁无陈旧锁 | A 结束后普通调用立即拿到锁（t116 C6 0.9s；本次 A/B 系列亦无等待） |
| `-Jobs`/`-TargetDir` 真进子进程 env | 子进程 build.rs 自印：`SLOWCRATE build.rs: CARGO_BUILD_JOBS=4 CARGO_TARGET_DIR=C:\…\ruagent-t117\tt-slow` ⇒ **仍进 env** |
| cargo 以 BelowNormal + 掩码跑 | 采样：`cargo … PriorityClass=BelowNormal Mask=4095`（两进程）⇒ 不变 |
| `-DenyWarnings` 真传下去（有警告红/无警告绿） | clippy 对有警告 crate ⇒ 101；无开关 ⇒ 0（t116 读数不变）+ DryRun argv 不变 |
| `-Nocapture` 且 exit 存活 | r07 ⇒ `WARNCRATE TEST STDOUT VISIBLE`、`test result: FAILED …`、`exit=101` ⇒ 不变 |
| `exit == cargo 的 exit`（四条路径） | 编译错误 r05 ⇒ **101**；clean 失败 r06 ⇒ **101** + `not running the gate`；无参数 ⇒ **2**；裸 `-CleanFirst` 无 `-p` ⇒ **2** ⇒ **全部不变** |
| 「没有失败被报成成功」 | 本次所有失败运行（r05/r06/r07 + 各 exit 2 的拒绝）都非零且不自称成功 |

## 15 仍未修 / 明确登记（第 19 条）

1. **W-10（新，低）`-Nocapture` 也不做指令集检查**：`check -Nocapture` ⇒ `cargo check -- --nocapture` ⇒ cargo `error: unexpected argument '--nocapture' found` ⇒ **exit 1**。**不修的理由**：它是一条**响亮的参数错误**（点名了那个参数），不属于「让人把没验读成验过」那一族；对称加守卫会是超出本单要求的**新拒绝**。登记在此，若未来要统一，建议与 W-4 用同一张「子命令→可用开关」表。
2. **`-Cpus > logical` 仍静默 clamp**（`-Cpus 999` ⇒ `cpus=0-14/16`）：这是脚本头的**既定语义**（「留出最高编号的核」），且日志里印的是**生效掩码**而不是用户给的数字 ⇒ 不算误导读数。登记不改。
3. **W-9 只加 NOTE、不拒绝**（理由见 §12）。
4. **90 分钟超时 ⇒ `exit 75`**：仍未构造（会占住团队锁 90 分钟，堵住全体成员）⇒ 只有代码阅读。
5. **锁文件无法打开的非 IOException 分支**（权限/占位为目录）：仍未构造（会破坏所有成员的构建）。
6. **`-TargetDir` 的值若**真的**以 `-` 开头**（例如 `-TargetDir -mycache`）：现在会被拒绝。这是**有意的**取舍（`-` 开头更可能是一个写错的开关，正如 W-2b 实测的 `-TargetDir -NoLock`）；Windows/Linux 上以 `-` 开头的相对路径可用 `.\-mycache`/`.-/mycache` 表达。**登记为已知取舍**。

## 16 方法学：本轮的 2 个新陷阱（「控制红/绿 ≠ 控制对了」累计 7 例）

6. **我把「清依赖迫使重编」的第一个控制选错了包**：我选了 `-CleanFirst ruagent-core` 配门禁 `-p ruagent-policy`，并**先验地**认定「policy 依赖 core」——实测 `Finished in 0.19s`、**没有任何 `Checking`** ⇒ cargo 什么都没做（policy 的 Cargo.toml 里根本没有 `ruagent-core`）。**横幅是对的，我的期望是错的**；换成 `-p ruagent-memory`（确实依赖 core）才拿到 f03 的 `Checking ruagent-core` + `Checking ruagent-memory`。⇒ 教训：**「依赖关系」也要有读数，不能靠印象**（`grep ruagent-core crates/*/Cargo.toml` 才是判据）。
7. **`Select-Object -First N` 在原生命令管道上会提前终止上游**：我的第一次冒烟把 `& powershell … | Select-Object -First 3` 接在一起，于是**绿 DryRun 的 exit 被读成 2**（`$LASTEXITCODE` 被管道终止污染）。改用 `Tee-Object -FilePath … | Out-Null` 后读数正确。这与 t116 的 `Start-Process`/`ExitCode` 陷阱同族：**读数的采集方式本身就是被审对象的一部分**。

## 17 纪律与残留（t117）

- **只读纪律的例外只有一处且已声明**：本单 inScope 允许改 `scripts/cargo-team.ps1`（+140/−14，sha256 `af311f43…`）；**未**改 `.github/`（`git status` = 0）、`crates/`、`panel/`；唯一另一处写入是本报告。未 push / dispatch / rerun / cancel / 建 tag；未写活库；未碰 pid 79984。
- **每一跑构建都走包装脚本本身**；临时 crate 复用 t116 的 `%TEMP%\ruagent-t116\{warncrate,failcrate,slowcrate}`（slowcrate 的 build.rs 睡时从 25s 改成 13s，仅为本单的锁探针限时）。
- **临时 target 按具体路径自清**（无通配符）：`%TEMP%\ruagent-t117\` 下删除 `tt-fail 0.38MB`、`tt-nocap 4.38`、`tt-slow 3.1`、`tt-b1 0.02`、`tt-b2 0.02`、`tt-nc-check 0`（共 ~7.9 MB）；DryRun 用到的 `tt-ok`/`tt-cmp` **从未被创建**（DryRun 在 `New-Item` 之前就 exit 了 ⇒ 这本身也是一条读数）。逐条 `exists=False` 已复核。保留 `%TEMP%\ruagent-t117\{probeD.ps1,probeE.ps1,logs\*.out,logs\*.err}` = **57 个文件 / 57.7 KB**（复核材料）；`%TEMP%\ruagent-t116` 的 50 个文件（captain 要求保留）**原样未动**。
- **共享 target 的副作用（如实记账）**：`ruagent-core` 被清后重编（f01，1.7s）；`ruagent-policy` 被 r02 清掉、随后 r03 重编（现为热）；`ruagent-memory`+`ruagent-store` 被 f03 重查（2.6s）；`warncrate`（临时 crate）在共享 target 里留下 **1 个指纹文件 / 0 MB**（外来包名，不影响任何团队 crate，且无法用 `cargo clean -p` 单独移除——它不是 workspace 成员）。锁文件 0 字节、mtime 未变；收尾无 `cargo`/`rustc` 残留。
