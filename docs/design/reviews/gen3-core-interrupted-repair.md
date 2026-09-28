# t88 修复报告：`RunStatus::Interrupted` 的 active/terminal 语义互斥

> **单号**：t88（triage of t78 C-2，high）· **作者**：verify2（本单我是**实现者**，不是评审者）
> **结论**：`is_active()` 改为 **`!self.is_terminal()`**（派生化），`Interrupted` ⇒ **`is_active = false` / `is_terminal = true`**；`status_predicates` 从"断言矛盾"改成**逐状态矩阵 + 互补断言**；负控已跑：**故意把旧语义装回去 ⇒ 测试红（`FAILED`、exit 101、消息点名 `interrupted: is_active`）**。
> **改动**：`crates/core/src/run.rs` **1 个文件，+79 / −11**（`git diff --stat`）。报告：本文件。
> **裁决遵循**：captain 裁决 `Interrupted` 由**重启批量产生**、不会自己继续 ⇒ `is_active=false` / `is_terminal=true`。我**没有反证**：`crates/daemon/src/lib.rs:110-123` 只把 `Queued/Spawning/Running/WaitingPermission` 四个非终态标成 `Interrupted`，标完再无任何路径把它写回非终态（`grep` `run.status = RunStatus::` 的 13 处赋值里，`Interrupted` 只有恢复循环这一处生产、且只消费非终态行）⇒ 裁决与实现一致。

---

## 1 逐状态矩阵读数（每个 `RunStatus` 一行）

**读数来源**：`crates/core/src/run.rs` 的 `status_predicates`（改后）逐行 `assert_eq!` 每个变体的两个谓词，**该测试在门禁里 `ok`**；`is_active` 的取值在本改动后**由定义派生**（`!is_terminal()`），因此下表不是抄注释，而是"测试断言 + 派生化定义"两处一致的结果。

| `RunStatus` | `is_active()` **改后** | `is_terminal()` **改后** | 组合合法？（互斥且穷尽 = 恰好一个 true） | 改前 |
| --- | --- | --- | --- | --- |
| `Queued` | true | false | ✅ 合法 | (true, false) 同 |
| `Spawning` | true | false | ✅ 合法 | (true, false) 同 |
| `Running` | true | false | ✅ 合法 | (true, false) 同 |
| `WaitingPermission` | true | false | ✅ 合法 | (true, false) 同 |
| `Completed` | false | true | ✅ 合法 | (false, true) 同 |
| `Failed` | false | true | ✅ 合法 | (false, true) 同 |
| `Cancelled` | false | true | ✅ 合法 | (false, true) 同 |
| **`Interrupted`** | **false** | **true** | ✅ 合法（**本单修的就是这一行**） | ❌ **(true, true)** —— "还在动"且"已终结" |

* **非法组合的定义**：`is_active == is_terminal`（同时为真 = 矛盾；同时为假 = 穷尽性破了）。**改前有 1 行非法（`Interrupted`），改后 0 行非法。**
* **穷尽性怎么保证的（不是手抄）**：测试里 `for (status, active, terminal) in matrix` 的循环体内有一个**没有 `_` 分支**的 `match status`，用来取 `name` 供失败消息使用 ⇒ 一旦给 `RunStatus` 加第 9 个变体，**这个测试模块编译不过**，动手的人被迫回到矩阵补一行。这正是 t78 C-9/C-6 那类"手抄清单漂移"的解药，而不是又抄一份。
* **每个变体一行断言**：`assert_eq!(status.is_active(), active, "{name}: is_active")`、`assert_eq!(status.is_terminal(), terminal, "{name}: is_terminal")`、以及 `assert_eq!(status.is_active(), !status.is_terminal(), "{name}: is_active must be the complement of is_terminal")`。
* **第三个谓词不混淆**：`is_retryable()`（`run.rs:83-88`）回答的是另一个问题（dead-but-not-delivered），不是这个划分；测试末尾把 `Failed/Interrupted/Cancelled` 可重试、`Completed` 不可重试（"run again"是另一个手势）、`Running/Queued` 不可重试一起钉住。`is_retryable ⊆ is_terminal` 成立，两者不矛盾。

---

## 2 改了什么（`crates/core/src/run.rs`）

```diff
-    /// Whether the run is still moving (not in a terminal state).
+    /// Whether the run is still moving — **the exact complement of
+    /// [`Self::is_terminal`]**, never a second hand-kept list.
+    ///
+    /// REVISION 2026-09-29 (t88, adjudicated from the t78 C-2 audit): ...
     pub fn is_active(self) -> bool {
-        !matches!(
-            self,
-            RunStatus::Completed | RunStatus::Failed | RunStatus::Cancelled
-        )
+        !self.is_terminal()
     }
```

1. **`is_active()` 派生化**：从"第二份手写 `matches!` 清单"变成 `is_terminal()` 的取反。这不只是改对一行，而是让 **"两者同时为真"在类型上不可表达**——加第 9 个变体也漂移不回来。文档注释里按纪律**保留了旧定义原文 + 带日期的修订说明**（`REVISION 2026-09-29 (t88, adjudicated from the t78 C-2 audit)`）。
2. **`is_terminal()` 标注为单一真源**（doc 增加两行）：说明 `is_active` 是它的否定、两者不可能不一致，并指向 `status_predicates`。
3. **测试更正**（`run.rs:198-262`）：旧的四条断言换成 8 行矩阵；**旧的矛盾断言以原样文本保留在测试的 doc 注释里**（带 `// old: contradictory` 标记与日期），使这次改动可审计。

**行为影响面（读数，非推断）**：新旧定义只在 `Interrupted` 上不同；其余 7 个变体逐行相同（见 §1 表）。`is_terminal()` **未改语义**，所以它那 3 个生产消费者（`daemon/src/api.rs:429`、`api.rs:2358`、`daemon/src/runs.rs:1688`）行为不变。

---

## 3 负控：把旧语义装回去，测试**必须红**（已跑）

**注入**（临时把 `is_active()` 换回改前的那份清单，并在行内注明是负控）：
```rust
    pub fn is_active(self) -> bool {
        // NEGATIVE CONTROL (t88): the pre-repair body, temporarily restored to
        // prove `status_predicates` fails when `Interrupted` is active again.
        !matches!(
            self,
            RunStatus::Completed | RunStatus::Failed | RunStatus::Cancelled
        )
    }
```
**读数（原样）**：
```
test run::tests::status_predicates ... FAILED
---- run::tests::status_predicates stdout ----
thread 'run::tests::status_predicates' (35220) panicked at crates\core\src\run.rs:249:13:
assertion `left == right` failed: interrupted: is_active
  left: true
 right: false
    run::tests::status_predicates
test result: FAILED. 10 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
[cargo-team] exit=101 elapsed=2.3s
```
* **红得对**：只有 `status_predicates` 失败（10 passed / **1 failed**），失败消息**点名 `interrupted: is_active`**（`left: true` 来自旧语义、`right: false` 来自矩阵）⇒ 更正后的断言确实**能抓到这类回归**，不是"改完就再也不会红"的装饰。
* **注入已回滚**，回滚后复跑绿灯（见 §5 的门禁读数：`ok=2 FAILED=0 panicked=0`、exit 0）。

---

## 4 `is_active()` 的去留：**选择"留下并让它正确"**（并说明为什么）

**读数（grep 全仓 `*.rs`）**：`is_active()` 命中 **4 处，全部在 `crates/core/src/run.rs` 内**（3 处在我改后的测试里、1 处在 doc 注释里）⇒ **生产消费 0 处**（与 t78 的读数一致，本次没有新增消费者）。对照 `is_terminal()`：`daemon/src/api.rs:429`、`api.rs:2358`、`daemon/src/runs.rs:1688` 三处生产消费者。

**决定**：**保留** `is_active()`，并让它成为 `is_terminal()` 的精确否定。理由：
1. **缺陷是"矛盾"而不是"未被调用"**：契约禁止的是"既无消费者**又**自相矛盾"。派生化之后它不可能再矛盾，未来那个"回收器"作者拿到的是**正确**谓词而不是一个坑（captain 的理由原文是"未来的调用者必然踩"⇒留下一个正确的反而降低风险）。
2. **删它是公开 API 的移除**，而契约给删除分支附带了**额外的**工作区级编译+测试门禁（"若删，给「删除后 workspace 编译 + 测试通过」的读数"）；本单的 Verify 只列了 core 级门禁，我没有为一次 API 移除去跑工作区级门禁，也没有必要把它变成一次跨 crate 改动。
3. **不是死代码告警源**：它是 lib crate 的 `pub` 谓词，`clippy --all-targets -D warnings` 不报 dead_code（§5 门禁 exit 0 即证）。

**留给 owner 的选项**：若 reviewer 认为"未被调用就不该存在"，删除它的代价是**工作区级** `cargo check/clippy --workspace --all-targets` + 全量测试读数，且要确认 `panel/`、`cli/`、`mcp/` 侧没有以字符串/反射方式依赖它（今天的 grep 显示没有）。我**没有**在本次修复里删除。

---

## 5 门禁读数

### 5.1 `test -p ruagent-core`（契约 Verify 第 1 条，原样命令）

```
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-core
```
读数：
```
running 11 tests
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
[cargo-team] exit=0 elapsed=3.7s
COUNTS: ok=2 FAILED=0 panicked=0
EXIT=0
```
* `test result: ok` **2 行**（unit 11 + doc-test 0）· `test result: FAILED` **0 行** · `panicked` **0 行** · **退出码 0**。
* 说明：这条命令**第一次尝试在 600 s 内没拿到单飞锁**（无输出，wrapper 在等锁：当时 `cargo`/`rustc` 三个同伴进程在跑），我把它改成**后台排队**重发，锁释放后 3.7 s 跑完 —— 上面是**原样命令**的读数。

### 5.2 `clippy -p ruagent-core --all-targets -DenyWarnings -CleanFirst`（契约 Verify 第 2 条）

**⚠️ 原样命令**（去掉包参数）**跑不动，而且不是我的代码的问题**——它是 wrapper 的参数形状问题：
```
[cargo-team] cargo clippy -p ruagent-core --all-targets -- -D warnings
[cargo-team] clean -p  (forced re-check)
error: "--package <SPEC>" requires a SPEC format value, which can be any package ID specifier ...
[cargo-team] cargo clean -p  exited 101; not running the gate
EXIT=101
```
⇒ `-CleanFirst` **本身要带一个包名参数**（`scripts/cargo-team.ps1:79-84`：`if ($a -eq '-CleanFirst') { $i++; $cleanFirst.Add([string]$raw[$i]) }`；`:117` 与 `:155-161` 用它拼 `cargo clean -p $pkg`），wrapper 自己的用法说明里就是带参数的形状（`:47`：`... --all-targets -DenyWarnings -CleanFirst ruagent-daemon`）。契约里那条字符串**漏了这个参数**，于是 `cargo clean -p` 拿到空 SPEC、**门禁根本没跑**（"+79/−11 的代码不可能弄坏它：这条命令在改代码前也会这样失败"）。
**按 wrapper 文档的形状跑（同一含义的门禁）**：
```
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 clippy -p ruagent-core --all-targets -DenyWarnings -CleanFirst ruagent-core
```
读数：
```
[cargo-team] cargo clippy -p ruagent-core --all-targets -- -D warnings
[cargo-team] clean -p ruagent-core (forced re-check)
     Removed 877 files, 308.4MiB total
    Checking ruagent-core v0.1.0 (C:\Users\19410\Documents\ai\ruagent\crates\core)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.55s
[cargo-team] exit=0 elapsed=2.9s
EXIT=0
```
**两件证据**（接受标准要的两件）：
1. **缓存真被清了**：`clean -p ruagent-core (forced re-check)` + `Removed 877 files, 308.4MiB total` —— 清理与门禁在**同一个锁窗口内**，因此下面那句 `Checking` 不可能是缓存命中造的假绿（wrapper 文档点名的假绿形态：缓存命中时 0.8 s "成功"）。
2. **cargo 真的重查了这个包**：`Checking ruagent-core v0.1.0 (…crates\core)` + `Finished … in 1.55s` + `[cargo-team] exit=0` —— `-D warnings` 下 **0 条警告、退出码 0**，且这正是 wrapper 文档指定的证据行（"cargo prints that per PACKAGE, so it proves the package was re-checked"）。

### 5.3 发现（跨单，**不在本单 inScope**，只报告不代修）

**契约 Verify 第 2 条的命令字符串当前不可能通过**：`-CleanFirst` 需要一个包名参数，漏掉它会得到 `clean -p`（空 SPEC）⇒ `cargo clean` exit 101 ⇒ **门禁不跑**。危害不是"报错不明显"（wrapper 会打印 `not running the gate`），而是两件更实的：
* **它把"门禁没跑"伪装成"这一步失败"**：调用方若把 non-zero 当红，会误以为**代码**有问题（我这次就先按"是不是我的改动"排查了一轮）；若把该行当 warning（`-CleanFirst` 的可选性暗示了这个读法），就会得到一次**静默跳过**——正是本代反复抓的第 19/22 条族。
* **契约单本身**（t88 的 Verify 字段）也会把这条命令复制给下一个单。

**可证伪的修复判据**（owner = wrapper/CI 面，不是 `crates/core`）：
(a) 契约/用法里的 `-CleanFirst` 一律带包名（文档与示例同步）；或 (b) wrapper 在 `-CleanFirst` 后面**取不到值时不接受该开关**（报错打印"`-CleanFirst` needs a package spec, e.g. `-CleanFirst ruagent-core`"），而不是用空 SPEC 去 `cargo clean`；或 (c) 取不到值时**回退到调用参数里的 `-p <spec>`**。判据：`... -DenyWarnings -CleanFirst`（不带参数）**要么以清晰的参数错误在编译前失败**、要么**按 `-p` 的 core 正常清+检**，**不得**出现 `clean -p  (forced re-check)` 这种空 SPEC 形态；且无论哪种，**门禁要么真跑要么明确拒绝**，不能"退出 101 但其实什么都没查"。
**我改了这条命令吗**：没有（`scripts/` 不在本单 inScope）。我用了 wrapper 文档记载的正确形状并给出上面的读数。

---

## 6 纪律回执

* **changedPaths（只列我改的）**：`crates/core/src/run.rs`（+79/−11）、`docs/design/reviews/gen3-core-interrupted-repair.md`（本文件，新建）。**没有**改 `scripts/`、`crates/daemon/`、`cli/`、`panel/`、`crates/store/`。
* **工作区里有同伴的在途改动，我如实标注**：`crates/core/src/event.rs` 在我开工前后就是 modified（他人把 `ContextInjected` 加了 `path`/`budget`，mtime 2026-09-28 02:54，`git diff` 可见）——**不是我改的**，但**我的两次门禁读数是在包含它的工作区上跑出来的**（它编译通过：core 11 tests 全绿）。若下游要复现我的读数，需要把这一条算进去。
* **活库/守护进程**：本单**没有**碰 `~/.ruagent`，**没有**启停 pid 79984，**没有**开任何端口。临时 root/端口无需自清（一个都没建）；自建的构建临时目录 `%TEMP%\ruagent-verify2-t78` 已删除（见下）。
* **负控的注入已回滚**：`git diff` 里现在只有最终形态（`!self.is_terminal()`），没有留下负控代码。
* **我自己的实现我不评审**：本报告只给读数与判据；t88 的独立评审由另一位成员执行。
