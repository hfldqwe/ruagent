# daemon 侧同族收口：`injection_e2e` 的 7 条「跳过也计 passed」（t37）

- 单号 / 尝试：**t37** / attempt 1 / `attempt_id = 28bd42ec-fc3d-4918-90f1-9decd3bde64c`
- 归属：**integ**（集成与消费面工程师）
- inScope：`crates/daemon/tests/injection_e2e.rs`、本报告
- 形状来源：**t30 / t33**（`docs/design/reviews/gen2-store-instrument-repair.md`）+ 集成契约 §6.0 **第 11 条**（「判据必须让漏测自己红 —— 绿可能是空跑」）
- 环境：一切 cargo 走 `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1`（共享 target `$env:TEMP\ruagent-team-target`、一次一个编译、CPU 0–11、BelowNormal）；clippy 用 `-CleanFirst ruagent-daemon` 强制重查；**未写活库、未启停 pid 79984**。

**一句话**：`injection_e2e.rs` 里 7 条需要 `ruagent-mock-agent` 的 e2e，原先在条件不满足时 `println!("SKIP …") + return` —— 而**早退在 cargo 汇总里就是 passed**，所以「没测」与「测过」印出同一行 `test result: ok`。现在它们默认是 **ignored**（harness 自己分账）、**显式 `-- --ignored` 而缺件时 FAILED（exit 101，失败信息点名缺什么）**、**有件时真跑并通过**。判据原则一句话：**不存在一个不测量也能通过的状态。**

---

## 1 逐条验收

| # | 验收项 | 读数 | 判定 |
| --- | --- | --- | --- |
| 1 | 7 条按 t30/t33 形状收口：默认 **ignored**、显式缺件 **FAILED**、有件真跑 | ① `ok. 0 passed; 0 failed; 7 ignored`（exit 0）② `FAILED. 0 passed; 7 failed`（exit **101**）③ `ok. 7 passed; 0 failed; 0 ignored`（exit 0，1.18s） | **达标** |
| 2 | 真实读数断言（不只靠「条件在不在」），保证「有件但没测」也红 | 7 条各自的读数断言见 §4：注入端断言**渲染内容**（`<relevant_memories>`/`<knowledge>`/`<wiki>` + 文档/页面原文），消费端断言 **mock 回显的 prompt 里含这些文本**，`drive_run`/`drive_chat` 另断言**回显非空**（"the agent echoed nothing -- the run never reached the model"）；两条负例断言**记忆块仍在**（否则「没有知识块」可由「什么都没注入」假达标） | **达标** |
| 3 | grep `RUAGENT_REQUIRE_MOCK`；若无人设置要写明「这个严格开关事实上不存在」 | 全仓 **5 处提及**：本文件 3 处（`:90/:93/:94`，**定义侧**）+ 2 处文档描述；`scripts/**`、`.github/**`、`*.toml`、`*.yml`、契约命令里**一处也没有设置** ⇒ **事实上不存在**。已随本次改动**整段删除**（无引用的 `fn` 会触发 `dead_code`，在 `-D warnings` 下红） | **达标**（见 §5） |
| 4 | 两次读数 + 有件时真跑读数 | 见 §3 的三态读数（均为一手） | **达标** |
| 5 | 三条 verify 命令全过 + 两件证据 | `test`（默认）exit 0 / 27.2s · `test -- --ignored` exit 0 / 2.3s · `clippy --all-targets -DenyWarnings -CleanFirst ruagent-daemon` exit 0 / 7.5s；覆盖 target = `--all-targets`（含 `--lib`/`--lib test`/`--test injection_e2e`），重查证据 = 本次输出里的 `Checking ruagent-daemon v0.1.0`（强制重查形态：`Removed 414 files, 1.7GiB total`）；三条读数均取自**最终字节**（格式修正后重跑） | **达标** |
| 6 | 不动 `crates/knowledge/tests/common/mod.rs:391`（t29 活库守卫）；只改 `injection_e2e.rs` + 本报告；不写活库、不启停 pid 79984 | 本单 porcelain = ` M crates/daemon/tests/injection_e2e.rs` + `?? 本报告`；`crates/knowledge/**` 一个字未动（t29 的守卫保持原样）；全程只读活库（**本单根本没读库**）、未触碰 pid 79984 | **达标** |

---

## 2 改动的确切形状（3 处形状，共 7 个调用点）

**(a) 删掉「早退 + 环境开关」，换成一条常量**（`fn skip_missing_mock()` → `const NEEDS_MOCK: &str`，`injection_e2e.rs:88-101`）：

```rust
const NEEDS_MOCK: &str = "needs ruagent-mock-agent next to this test binary: build it with \
    `cargo build -p ruagent-mock-agent` (or `cargo test --workspace`), then run this test with `-- --ignored`";
```

删除的旧形状原文（`git diff` 可复核）：

```rust
fn skip_missing_mock() {
    let msg = "no ruagent-mock-agent binary next to this test binary. …";
    if std::env::var("RUAGENT_REQUIRE_MOCK").is_ok() {
        panic!("RUAGENT_REQUIRE_MOCK is set and there is {msg}");
    }
    println!("SKIP t292: {msg} THIS TEST DID NOT RUN.");
}
```

**(b) 7 条测试，每条两行同一形状**（改前 `#[tokio::test]` 在 `:329/:379/:495/:535/:566/:651/:693`，其下 4 行是 `let Some(d) = boot(...).await else { skip_missing_mock(); return; };`）：

| 测试 | 改后 |
| --- | --- |
| `run_injects_memory_knowledge_and_wiki` | `#[ignore = "needs ruagent-mock-agent next to the test binary; run with -- --ignored"]` + `let d = boot("withkb", true).await.expect(NEEDS_MOCK);` |
| `run_without_knowledge_documents_has_no_knowledge_block` | 同上，`boot("nokb", false)` |
| `chat_injects_memory_knowledge_and_wiki` | 同上，`boot("chatwithkb", true)` |
| `chat_without_knowledge_documents_has_no_knowledge_block` | 同上，`boot("chatnokb", false)` |
| `chat_and_run_use_the_same_block_wording` | 同上，`boot("wording", true)` |
| `both_paths_emit_the_contract_truncation_marker` | 同上，`boot("markers", true)` |
| `a_retried_run_carries_the_shared_retry_prefix` | 同上，`boot("retry", false)` |

**为什么必须成对改**：只有 `#[ignore]` 而没有 `expect` ⇒ 显式 `-- --ignored` 缺件时会「跳过」而不是红；只有 `expect` 而没有 `#[ignore]` ⇒ 默认跑直接红（本地没装 mock 的人无法跑默认门）。两件一起才是「**默认分账、显式必红**」。

**(c) 规模**：`git diff --stat -- crates/daemon/tests/injection_e2e.rs` = **32 insertions(+), 46 deletions(-)**，文件 794 → **780** 行；`skip_missing_mock` 出现 **0** 次、`.expect(NEEDS_MOCK)` **7** 次、`#[ignore` **7** 次（另 2 处是既有注释里的字样）。

---

## 3 三态读数（改前 → 改后）

### ① 默认跑（`scripts/cargo-team.ps1 test -p ruagent-daemon --test injection_e2e`，exit=0 / 27.2s）

```
running 7 tests
test a_retried_run_carries_the_shared_retry_prefix ... ignored, needs ruagent-mock-agent next to the test binary; run with -- --ignored
test both_paths_emit_the_contract_truncation_marker ... ignored, needs ruagent-mock-agent next to the test binary; run with -- --ignored
test chat_and_run_use_the_same_block_wording ... ignored, needs ruagent-mock-agent next to the test binary; run with -- --ignored
test chat_injects_memory_knowledge_and_wiki ... ignored, needs ruagent-mock-agent next to the test binary; run with -- --ignored
test chat_without_knowledge_documents_has_no_knowledge_block ... ignored, needs ruagent-mock-agent next to the test binary; run with -- --ignored
test run_injects_memory_knowledge_and_wiki ... ignored, needs ruagent-mock-agent next to the test binary; run with -- --ignored
test run_without_knowledge_documents_has_no_knowledge_block ... ignored, needs ruagent-mock-agent next to the test binary; run with -- --ignored

test result: ok. 0 passed; 0 failed; 7 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

**`N passed / M ignored` 由 harness 自己分账**，且每条 ignore 的**理由印在结果行上** —— 读者不会把 7 条未尝试读成 7 次测量。（这条正是「空跑绿」的反面：默认跑不再假装测过。）

**改前的一手对照（标注推断成分）**：本环境**装了** mock（`$env:TEMP\ruagent-team-target\debug\ruagent-mock-agent.exe`，5,757,440 B，mtime 2026/9/28 0:58:12），所以改前默认跑是 `7 passed; 0 ignored` —— 那次是真跑，**不是**本单要修的失真。失真的可复现形态是**缺件**：改前它会打印 `SKIP t292: … THIS TEST DID NOT RUN.` 然后 `return`，cargo 汇总**仍写 `7 passed`**（这一点由被删函数自己的注释写明：「an early return is a PASS in cargo's summary」），而改后同一缺件形态是 **7 failed / exit 101**（下一节一手读数）。改前的「7 passed」我没有第二份一手读数（旧二进制已被覆盖），**此处按代码原文 + 该缺件形态的新读数标注为推断**。

### ② 显式 `-- --ignored` 而**缺件**：FAILED（exit 101），不是跳过

```
running 7 tests
test both_paths_emit_the_contract_truncation_marker ... FAILED
test run_injects_memory_knowledge_and_wiki ... FAILED
test a_retried_run_carries_the_shared_retry_prefix ... FAILED
test chat_without_knowledge_documents_has_no_knowledge_block ... FAILED
test chat_injects_memory_knowledge_and_wiki ... FAILED
test run_without_knowledge_documents_has_no_knowledge_block ... FAILED
test chat_and_run_use_the_same_block_wording ... FAILED

test result: FAILED. 0 passed; 7 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
EXITCODE=101
```

失败信息**点名缺什么**（7 条各印一次，取前两条）：

```
needs ruagent-mock-agent next to this test binary: build it with `cargo build -p ruagent-mock-agent` (or `cargo test --workspace`), then run this test with `-- --ignored`
needs ruagent-mock-agent next to this test binary: build it with `cargo build -p ruagent-mock-agent` (or `cargo test --workspace`), then run this test with `-- --ignored`
```

**这条读数是怎么造出来的（诚实交代，且不碰别人的产物）**：`mock_bin()` 从**测试二进制自身位置**推 `target/<profile>/ruagent-mock-agent[.exe]`（`injection_e2e.rs:66-80`）。所以我把**同一个**测试二进制（`…\debug\deps\injection_e2e-5ed1b3e6530a5dcf.exe`）复制到一个**祖父目录里不可能有 mock** 的临时目录（`$env:TEMP\ruagent-t37-nomock\bin\`）再跑 `--ignored` ⇒ `mock_bin()` 返回 `None` ⇒ 7 条 `expect` panic。**没有**去改名/移动共享 target 里别的包的产物（那是并发风险），跑完已删除临时目录（`Test-Path` = False），并确认**没有残留 `injection_e2e.exe` 进程**。

### ③ 有件时：那 7 条真跑通过（`… test -p ruagent-daemon --test injection_e2e -- --ignored`，exit=0 / 2.3s）

```
running 7 tests
test run_without_knowledge_documents_has_no_knowledge_block ... ok
test run_injects_memory_knowledge_and_wiki ... ok
test chat_without_knowledge_documents_has_no_knowledge_block ... ok
test chat_injects_memory_knowledge_and_wiki ... ok
test both_paths_emit_the_contract_truncation_marker ... ok
test chat_and_run_use_the_same_block_wording ... ok
test a_retried_run_carries_the_shared_retry_prefix ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.43s
```

二进制路径（覆盖 target 的实证）：`C:\Users\19410\AppData\Local\Temp\ruagent-team-target\debug\deps\injection_e2e-5ed1b3e6530a5dcf.exe`。

---

## 4 非真空断言：7 条各自防的哪一种真空

「有 mock-agent 但没测」必须红 —— 靠的是断言**读数本身**，不是「条件在不在」。逐条：

| 测试 | 真实读数断言（改后仍在，本单未削弱） | 防的真空 |
| --- | --- | --- |
| `run_injects_memory_knowledge_and_wiki` | 渲染含 `<relevant_memories>` + 种子记忆原文 + `<knowledge>` + 文档原文 + `<wiki>` + 页面原文；且 **mock 回显的 prompt 里含文档/页面/记忆**；`drive_run` 断言**回显非空** | 「构造了但没发出去」「发出去但 agent 没收到」 |
| `run_without_knowledge_documents_has_no_knowledge_block` | 负例：**没有** `<knowledge>`/`<wiki>`，同时断言**记忆块仍在**（`<relevant_memories>` + 原文） | 「什么都没注入」被读成「正确地没有知识块」 |
| `chat_injects_memory_knowledge_and_wiki` | 同 chat 路径的三层读数（渲染 / 事件 / 回显含三类文本） | chat 路径未接线却看起来在注入 |
| `chat_without_knowledge_documents_has_no_knowledge_block` | 负例：渲染与回显都不含 `<knowledge>`，而记忆块仍在 | 同上（chat 路径） |
| `chat_and_run_use_the_same_block_wording` | `assert_eq!(chat_headers, run_headers)`，**并断言两个 header 列表都真的含 `<knowledge>`/`<wiki>`** | 「两边都空」也能过的 header 比较 |
| `both_paths_emit_the_contract_truncation_marker` | 断言 marker **非空**（"no marker to compare -- the bound never bound"），且每个 marker 与契约构造器 `tail_truncated(n)` **逐字相等** | 「没有截断」被读成「截断标记一致」 |
| `a_retried_run_carries_the_shared_retry_prefix` | `render.expect("the retried run must emit context_injected")` + 断言重试前缀与 `chat.rs` 的 `retry_head()` 逐字相同 | 重试路径没发生却「前缀测试通过」 |

**边界（诚实）**：本单只把「早退」换成「ignored + 显式必红」，**没有**新增断言 —— 因为这 7 条原本就有上表的读数断言（这也是它们值钱的地方：坏的是**门**，不是断言）。因此「新加的空转断言也要在真空输入上实跑出红」（契约 §6.0 第 11 条同族条）在本单**不适用**：本单没有新增断言 ⇒ 无需给真空红读数。

---

## 5 `RUAGENT_REQUIRE_MOCK` 的存在性审计（grep 全仓）

| 命中位置 | 性质 |
| --- | --- |
| `crates/daemon/tests/injection_e2e.rs:90`（注释）、`:93`（`std::env::var`）、`:94`（`panic!`） | **定义侧**（改前；现已整段删除） |
| `docs/design/reviews/gen2-store-instrument-falsifiability.md:129` | 描述：**「默认门不设它，且没有任何契约命令会设」** |
| `docs/design/reviews/gen2-store-instrument-falsifiability.md:158` | 把它列为 t36 盘点里的**未查项**（建议 t19/t37 顺手 grep） |
| `docs/plans/2026-09-26-memory-knowledge-closure.md:484` | 设计叙述（「无人值守运行可以拒绝歧义」） |

**扫过的载体**：`crates/**/*.rs`、`scripts/**/*.ps1`、`panel/**/*.{mjs,ts,json}`、`docs/**/*.md`、`*.toml`、`.github/**/*.{yml,yaml}`。

**结论（本单要写明的那句）**：**这个严格开关事实上不存在** —— 只有 `injection_e2e.rs` 自己读它，而**没有任何脚本、CI 配置、toml 或契约命令设置它**。所以默认门下这 7 条**仍会被算作 passed**（改前），严格行为永远不会被触发；「无人值守运行可以拒绝歧义」这句话在仓库里没有执行者。**本次把它换成不需要人记得去打开的形状**（`#[ignore]` + `expect`）后，该开关已删除。

---

## 6 覆盖与重查证据（契约 §6.0 第 6–10 条要求的「两件」）

```
[cargo-team] cargo clippy -p ruagent-daemon --all-targets -- -D warnings
[cargo-team] clean -p ruagent-daemon (forced re-check)
[cargo-team] exit=0 elapsed=7.5s
[stderr]
     Removed 414 files, 1.7GiB total
    Checking ruagent-daemon v0.1.0 (C:\Users\19410\Documents\ai\ruagent\crates\daemon)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 7.37s
```

**三条读数都取自交付字节**：我先跑过一轮（默认 / `--ignored` / clippy），随后 `rustfmt --edition 2024 --check crates/daemon/tests/injection_e2e.rs` 报了**一处空白行**（我替换常量时多留了一个空行）⇒ 我改掉后**重跑了上面三条命令**，本节与 §3 的数字都是**改完格式后的最终字节**上的读数。`rustfmt --check` 最终 = **exit 0**（只影响我的文件；此处没有跑全仓 `fmt --all`，以免把同伴在途文件的格式问题混进本单）。

- **① 覆盖了哪些 target**：`--all-targets` ⇒ `--lib`、`--lib test`、全部 `tests/*` target（含 `--test injection_e2e`）。
- **② 本次确实重查了哪个包**：`Checking ruagent-daemon v0.1.0` 在本次输出里（且其一字形前缀是 `clean -p ruagent-daemon (forced re-check)`，`Removed 465 files, 1.8GiB`）⇒ **不是缓存命中的绿**、也不是 0.8s 形状。
- **收窄声明面**：`Checking` 是**按包**打印的，所以本节只声称「这次调用 exit=0 且 daemon 包被重查过」，**不声称逐 target 的 `clippy-driver` 证据**（未取 `--verbose`）。
- **独立复核（同伴，不是我自己的读数）**：**wiki** 在她 t34 的门禁里独立跑过同一条命令，读到 `clean -p ruagent-daemon (forced re-check)` → `Checking ruagent-daemon v0.1.0` → `Finished in 6.78s` → `exit=0`（0 warning / 0 error，覆盖 `--all-targets`：lib + lib 单测 + `injection_e2e` / `knowledge_api` / `smoke`）。她还报：她先前看到的 `E0425 skip_missing_mock`（`injection_e2e.rs:685`）**只出现过一次**，复跑已抓不到 —— 与「我对 7 处同形调用点做了先删后改的中间态」一致，且她**没有**动我的文件（只发坐标，并取了一条 `-p ruagent-daemon --lib` 的收窄读数撑住她的单子）。

---

## 7 边界、未测与纪律回执

| 项 | 状态 |
| --- | --- |
| 本单 porcelain | ` M crates/daemon/tests/injection_e2e.rs`（**唯一**代码改动）+ `?? docs/design/reviews/gen2-injection-instrument-repair.md`（本报告） |
| 明确**没动**的 | `crates/knowledge/tests/common/mod.rs:391`（t29 的**活库守卫** —— 路径无法证明不是活库时返回，**不是**测量门）；`crates/acp/src/adapter.rs:160`（门 = `resolve_program("dsh").is_none()`，**不在本单**，留给 acp 侧的同类收口单）；`crates/daemon/src/**`；`panel/**` |
| 活库 / 守护进程 | 全程**未读也未写**活库（本单的测量面只有临时 root 与 mock agent）；**未启停** pid 79984 |
| 临时产物与进程 | 造缺件读数用的 `$env:TEMP\ruagent-t37-nomock\` 已删除（`Test-Path` = False）；按**我自己的临时路径**查过孤儿 `injection_e2e.exe` = 无（未按名字/端口批量杀任何东西） |
| 未测（写明原因） | ① **没有**在「缺 mock 的共享 target」上跑过 `--ignored`（那需要 `clean` 掉别人也可能在用的 mock 产物 + 重建，代价与并发风险都不值）—— 缺件读数用「同一二进制放到没有兄弟 mock 的目录」取得，代码路径完全相同（`mock_bin()` → `None`）；② `crates/acp` 那条同族项**没查**（不在 inScope）；③ 未取 `clippy --verbose` 的逐 target `clippy-driver` 行（收窄声称面，见 §6） |
| 一次编辑窗口事故（诚实记录） | 我先删 `skip_missing_mock`、再逐个改 7 个调用点，**wiki 的 t34 门禁恰好在两者之间运行**，读到 `:685 cannot find function skip_missing_mock`（exit 101）。我已在选定窗口内改完并回信说明（`skip_missing_mock` 出现 0 次；同一 README 里的 `collapsible_if` 是我更早的修复）。**教训**：同形多处替换应整块一次改完再离开文件。 |

---

## 8 复现命令（全部经包装脚本）

```powershell
# ① 默认：7 条 ignored（harness 分账）
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-daemon --test injection_e2e

# ② 有件时真跑（本机共享 target 里有 ruagent-mock-agent.exe）
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-daemon --test injection_e2e -- --ignored

# ③ 缺件时必须红（把同一测试二进制放到没有兄弟 mock 的目录再跑）
$src = (Get-ChildItem "$env:TEMP\ruagent-team-target\debug\deps\injection_e2e-*.exe" | Select-Object -First 1).FullName
$dir = Join-Path $env:TEMP 'ruagent-t37-nomock\bin'; New-Item -ItemType Directory -Force -Path $dir | Out-Null
Copy-Item $src (Join-Path $dir 'injection_e2e.exe') -Force
$p = Start-Process (Join-Path $dir 'injection_e2e.exe') -ArgumentList '--ignored' -PassThru -Wait -NoNewWindow
"EXITCODE=$($p.ExitCode)"   # 期望 101

# ④ lint 门：强制重查 + 两件证据
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 clippy -p ruagent-daemon --all-targets -DenyWarnings -CleanFirst ruagent-daemon
```
