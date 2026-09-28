# gen3 CI 红史审计（t101）：不是「从来没绿过」；以及 2026-09-28 那次「第一次真跑」红在哪

> 单号 t101（work，**只读取证**）· 成员 `wiki` · attempt 1 · 快照时间 **2026-09-29 04:0x–05:0x（+08:00）= 09-28 20:0x–21:0x UTC**
> 唯一写入 = 本报告。`gh` 只用 `list`/`view`/`api`（只读）：**没有** dispatch / rerun / cancel / tag / push；**没有**改任何 `.github/workflows/**`、`crates/**`、`panel/**`、`scripts/**`。
> ⚠️ **审计期间靶子在动**：我开工时 HEAD = `0a39e5b8`（远端 main 停在最后那次红 `5db881d8`）；审计中途团队连续推了 **三个提交**（`0da0cb68` → `cb550730` → `877a909`，最后一个我收尾时**还在跑**）。**本报告每条读数都带 sha + 时间**；结构性的结论（§2/§3/§4）与瞬时读数（§6）分开写。

## 0 先更正前提（决定后面所有问题的形状）

**「`ci.yml` 从来没有绿过」是错的。** 全量历史读数：

```
gh run list --workflow=ci.yml --limit 1000 --json conclusion,event,createdAt
→ 开工时：{"total":300,"by_conclusion":{"success":97,"failure":67,"cancelled":136},"oldest":"2026-09-10T17:26:32Z","newest":"2026-09-26T21:16:08Z"}
→ 收尾时：{"total":303,"by_conclusion":{"success":97,"failure":68,"cancelled":137,"":1}}   # ""= 正在跑的那个
（`event` 全是 `push`；没有 workflow_dispatch / workflow_run / schedule）
```

| 口径 | success | cancelled | failure | 说明 |
| --- | --- | --- | --- | --- |
| **全量**（09-10 17:26Z → 09-28 20:49Z） | **97** | 137 | 68 | —— |
| 最近 100 次（09-26 14:06:59Z → 21:16:08Z） | **0** | **87** | 13 | 「success=0 / 其余 87」的真实窗口 —— 那**一个下午** |
| 最后一次绿之后（09-26 11:09:39Z → 21:16:08Z） | 0 | 93 | **17** | §3 |
| 对照 `e2e.yml`（最近 200 次） | 185 | 0 | 15 | 我的读数；captain 的「近 50 次 49 绿/1 红」不矛盾（15 次红在更早窗口） |

- **最后一次绿**：run `36237951722` · sha **`ed472e4a…`** · **2026-09-26T11:09:39Z** · `docs(memory): 修订 3 —— t246/t250 的读数与被推翻的前提`
- **它之前那次红**：`5db881d8…`（2026-09-26T21:16:08Z）—— 远端 main 就停在这里；本机 HEAD 是它的子提交。
⇒ 正确判据是两条：**(a) 绿→红之间坏了什么**（§3/§4）；**(b) 现在第一次推上去还差什么**（§5/§7）。

## 1 审计期间发生了什么（这轮最重要的「活证据」）

| 时间（UTC） | 本地 sha | GitHub run | 结论 | 红在哪（失败步） |
| --- | --- | --- | --- | --- |
| 09-26T21:16:08Z | `5db881d8` | 36272344732 | failure | ubuntu `Format` · windows `Test`（§2） |
| **09-28T20:26:31Z** | **`0da0cb68`** | 36479210529 | **failure** | **ubuntu `Clippy` + `Test evidence` + `Ignored-instrument manifest`；windows `Test attempt 1 evidence`；Panel 绿** |
| 09-28T20:37:54Z | `cb550730` | 36480566511 | **cancelled**（被后一次推送顶掉，但 ubuntu 已红同一组三步） | ubuntu `Clippy`（`lifecycle.rs:605/617`）等 |
| 09-28T20:49:00Z | **`877a909`** | —— **in_progress** | —— | 收尾时仍在跑；提交信息 `fix(memory,graph,panel): unblock CI -- warn-free memory, as-of fix, e2e pin` |

`0da0cb68` 的提交信息是 `feat(gen2): 记忆/知识/wiki/图谱/召回四区收口 + 单一消费面 + CI 证据纪律` ⇒ **这就是那条 375 行新流水线的第一次真跑**（§4 的盲区在这里被填上了），也就是说：**「第一次绿」已经不再是假设，而是正在发生的事**。

## 2 绿→红之间坏了什么（判据 a，历史）

最后一次绿 `ed472e4a`（09-26T11:09:39Z）之后共 **110 个 run**（17 failure + 93 cancelled + 0 success）。17 次失败按失败步：

| 失败步 | 次数 | job | 首次 | 最近 |
| --- | --- | --- | --- | --- |
| `Clippy` | **14** | Rust (ubuntu) | 09-26T13:19:33Z | 09-26T19:19:43Z |
| `Test` | 4 | windows 3 · ubuntu 1 | **09-26T11:29:23Z（绿之后第一枪）** | 09-26T21:16:08Z |
| `Format` | 2 | Rust (ubuntu) | 09-26T20:53:02Z | 09-26T21:16:08Z |

**可查原因的三次**：`36238973733`（11:29:23Z）红在 **ubuntu `Test`**（exit 101）；`36265665687`（19:19:43Z）红在 `Clippy`（`--> crates/memory/src/lifecycle.rs:186:32`，exit 101）；`36272344732`（21:16:08Z）红在 **ubuntu `Format`**（`Diff in …/crates/daemon/src/api.rs:2266`、`…/distill.rs:735`、`…/crates/memory/src/lifecycle.rs:260/317/400/576/599`）**与 windows `Test`**（`test result: FAILED. 22 passed; 1 failed` + `error: test failed, to rerun pass -p ruagent-mock-agent --test e2e_daemon`），Panel 绿。
**同 job 内的连锁**：`Format` 一红，同 job 的 `Clippy`/`Test` 全部 `skipped` ⇒ 一次红会掩盖后面所有步的读数。

## 3 全量失败步分类（67+1 次失败 run，含首次/最近）

| 失败步 | failure run 里 | 藏在 cancelled run 里 | 合计 | 哪个 job | 首次 | 最近 |
| --- | --- | --- | --- | --- | --- | --- |
| `Clippy` | 35 | **42** | **77** | Rust (ubuntu) | 09-11T14:49:18Z | 09-26T19:19:43Z |
| `Test` | 26 | 7 | 33 | windows 15 · ubuntu 11 | 09-10T18:43:40Z | 09-26T21:16:08Z |
| `Format` | 18 | 11 | 29 | Rust (ubuntu) | 09-25T17:29:23Z | 09-26T21:16:08Z |
| `Audit self-test` | 10 | 2 | 12 | Panel (node) | 09-24T16:39:57Z | 09-26T07:29:29Z |
| `Install protoc` | 3 | 1 | 4 | Rust (windows) | 09-11T14:49:18Z | 09-11T15:13:35Z |

读法：`Clippy`/`Test` 是慢性病（09-10 起）；`Format` 是 **09-25 才出现**的新病（与「没有门禁的文件没人格式化」同源）；`Audit self-test` 是 09-24 起的新病；`Install protoc` **只在 09-11 一天**出现 4 次后再没复现 ⇒ 环境 flake，不是当字节性质。

## 4 那 87（实为 136/137）个 cancelled：机制，以及它们**确实掩盖失败**

**机制**：`ci.yml:29-31` 的工作流级并发组

```yaml
concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true
```

⇒ **同一 ref 的后续推送取消前一次 run**。指纹：`17:35:21 → 17:39:25 → 17:41:53 → 17:42:31 → 17:44:47 → 17:45:17` 全 cancelled，紧接着 `17:45:29` 才是跑完的 failure。**不是手动取消、不是分支保护**（`event` 全 `push`）。

**但「cancelled 只是被顶掉」只对了一半**：把**全部 136 个 cancelled run** 逐个取 job/step 结论 ——

```
cancelled runs analysed: 136; with >=1 step concluded "failure": 59 (43.4%)
```

**43.4% 的 cancelled run 里已经有步骤判过 `failure`**。所以只看 `conclusion == "failure"` 会**系统性低估**失败次数（上表「藏在 cancelled」列）。
**是否掩盖了别的类别？没有** —— cancelled 里出现的类别集合与 failure run 完全一致（Clippy/Format/Test/Audit self-test/Install protoc），**没有第六种类别只出现在 cancelled 里**。准确说法：取消**放大已知类别的计数**，不隐藏新类别。

## 5 结构性盲区 + 它如何被填上：375 行的 ci.yml 从未跑过 → 09-28 第一次真跑

```
审计开工时：HEAD ci.yml 110 行 · 工作树 375 行（git diff --stat HEAD → 281 insertions(+), 16 deletions(-)）
           check-workflow-refs / test-evidence / include-ignored / Coverage declaration 在 HEAD 中出现 0 次
最后一次绿那个 run 的步骤表（gh run view 36237951722 --json jobs）：
  ubuntu = Format/Clippy/Test 三步；panel = Install/Audit self-test/Typecheck+build；windows = Test
⇒ 要推的那条流水线（Guard/evidence/ignored instruments/manifest/coverage）**从未在 GitHub 上执行过**
```

**09-28 的第一次真跑（`0da0cb68`）结果**，逐 job：

| job | 结论 | 失败步 | 日志原文 |
| --- | --- | --- | --- |
| **Panel (node)** | **success** | — | Install + Audit self-test + Typecheck&build 全绿 ⇒ 我的本机读数（panel build exit 0 · audit self-test exit 0）与 CI 一致 |
| Rust (ubuntu) | failure | `Clippy` · `Test evidence` · `Ignored-instrument manifest` | Clippy：`error[E0425]: cannot find function backup_surface in this scope` → `--> crates\memory\src\lifecycle.rs:535:9`，`exit code 101`；`Test evidence`：`##[error]… evidence log '…/t65/linux-default.log' is unreadable -- a run whose log we cannot read is not a green (t65)`；`Manifest`：`::error::ignored-test count changed: found $found, this accounts for $expected` |
| Rust (windows) | failure | `Test attempt 1 evidence` | `##[error]Rust (windows) -- attempt 1 reported **0 targets** -- an empty run is not a green (t65)` · `attempt 1 exit code: 101` |

`cb550730`（"define backup_surface so crates/memory compiles again"）之后，ubuntu 的 `Clippy` **仍然红**，这次是 **warnings-as-errors**：`--> crates/memory/src/lifecycle.rs:605:5` 与 `:617:5`（该步是 `cargo clippy … -- -D warnings`）⇒ 于是有了 `877a909`（"unblock CI -- warn-free memory, as-of fix, e2e pin"），**收尾时 in_progress**。

**三个必须写出来的结论**：
1. **根因是单一的、定位到行的**：`crates/memory/src/lifecycle.rs`（先 `E0425 backup_surface@535`，再 `605/617` 的 warning-as-error）。第一次真跑的红**不是**流水线设计问题。
2. **red 被级联放大了 3–4 倍**：`Clippy` 一红 ⇒ 同 job 的 `Test` **skipped**（该步没有 `if: always()`）⇒ `Test evidence`（`always()`）读不到日志 ⇒ 红；`Manifest`（`always()`）也因 `--ignored --list` 编不出来 ⇒ `found=0 ≠ 15` ⇒ 红；windows 同理。**一个缺陷 → 四个红步**，读日志时必须按 `skipped` 还原。
3. **静默跳过被挡住了（正面读数）**：`Test evidence` / `Test attempt 1 evidence` 都**拒绝**把「0 targets / 日志读不到」当成绿，正是 t65 想要的行为。同一 run 里 `Format`、`Guard: workflow-referenced paths`、panel 三步**都没红**（不在失败列表里）⇒ 新流水线的非测试步在 CI 上是通的。

## 6 每一类「今天的现状」——在当字节上复现（每条带 sha/时间）

**A. 审计开工时的字节（HEAD `0a39e5b8` + 未提交的 375 行 ci.yml）**

| 要推的步骤（行号=375 行版） | 本机命令 | 读数 |
| --- | --- | --- |
| `Guard: workflow-referenced paths`（62） | `bash .github/workflows/scripts/check-workflow-refs.sh` | **exit=1** · `not tracked/missing: 6`（`?? .github/workflows/scripts/`；`git ls-files -- .github/workflows/scripts/` 为空） |
| `Format`（88） | `cargo fmt --all --check` | **exit=0（CLEAN）** |
| `Clippy`（90） | `cargo clippy --workspace --all-targets -- -D warnings` | **exit=0**（16.53s） |
| `Guard against nested-Result misjudgement`（101） | 该步 `run:` 块原样抽出后 bash 执行 | **exit=0** · `no nested-Result misjudgement found` |
| `Test (default run -- skips stay countable)`（119） | `cargo test --workspace --no-fail-fast` | **exit=0**（205.8s） |
| `Test evidence`（125） | 未跑 | **未取到 + 原因**：读 `$RUNNER_TEMP/t65/*.log`（runner 独有）+ 调用未跟踪的 `test-evidence.sh` |
| `Ignored instruments CI can satisfy`（143） | `cargo test -p ruagent-daemon --test injection_e2e -- --include-ignored` | **exit=0 · 7 passed**（1.44s；首次 exit=101 是编译窗口，见 §9 注记 2） |
| `Ignored-instrument manifest`（165） | `cargo test --workspace -- --ignored --list`（走团队 wrapper）→ `': test$'` 计数 | **15 == 15（7 run + 8 declared）**；静态交叉核对：全树 `#[ignore]` 恰 15，落在同样文件（store 3 · daemon/injection_e2e 7 · graph/live-after 3 · knowledge 2） |
| Panel `Audit self-test`（342） | `node panel/tools/design-audit.mjs --self-test` | **exit=0** · `contract/threshold read warnings = 0` |
| Panel `Typecheck + build`（350） | `cd panel && npm run build` | **exit=0** · 1852 modules · 5.85s |
| Windows `Test attempt 1/2`（243/266） | `cargo test -p ruagent-mock-agent --test e2e_daemon` | **exit=0 · 23 passed** ⇒ CI 最后两次红的那个 target 在当字节上绿 |

**B. 收尾时的字节（HEAD `877a909`，ci.yml 已提交 375 行）**

| 读数 | 结果 |
| --- | --- |
| `git ls-files -- .github/workflows/scripts/` | **2 个文件已入库**（`check-workflow-refs.sh`、`test-evidence.sh`）⇒ §6A 的 6 条未跟踪引用**已被这次推送修掉** |
| `bash …/check-workflow-refs.sh` | **exit=0** · `every executed path a workflow references is tracked`（它还顺带打印三份 workflow `parses as YAML (PyYAML)`） |
| ci.yml | 工作树 375 行 == HEAD 375 行（**已提交**） |
| `clippy --workspace --all-targets -D warnings` | **exit=0**（7.70s）⇒ `877a909` 的 warn-free 修复在当字节上成立 |
| `cargo fmt --all --check` | **exit=1** —— `Diff in crates/graph/src/lib.rs:1845`（**同伴在途编辑**） |
| `test --workspace --no-fail-fast` | **exit=101** —— `-p ruagent-memory --lib` `77 passed; 2 failed`；**同一 target 立刻重跑 = exit=0 / 79 passed** |
| 说明 | 这两条是**在途编辑窗口内的读数**（`git diff --stat` 在我测量期间从「graph/lib.rs 有 diff」变成空）；我**没有**再取第三次数 ⇒ 不把它们当「当字节结论」，只登记为窗口。**推送中的 `877a909` 才是 CI 的真读数，它收尾时仍在跑。** |

## 7 「第一次绿还差什么」判据清单（每条 = 事项 · 证据 · 取证方式 · 已归谁）

| # | 事项 | 证据 | 取证方式 | 状态/归属 |
| --- | --- | --- | --- | --- |
| 1 | 把 `.github/workflows/scripts/` 两个脚本与 ci.yml/e2e.yml **同一次提交** | 开工时 guard `exit=1`（6 条未跟踪）；收尾时 `git ls-files` = 2 文件、guard **exit=0**、CI 上 `Guard` 步未红 | `bash …/check-workflow-refs.sh` + `git ls-files` | **已解决**（09-28 推送）；原始要求属 **t66** 的 F1 |
| 2 | 让 375 行 ci.yml 第一次真跑 | §5：`0da0cb68` 的 job/step 表 | `gh run view 36479210529 --json jobs` | **已发生**（t66 inScope） |
| 3 | `Format` 类清零 | 历史 29 次；开工本机 exit=0；CI 上 `Format` **不在** `0da0cb68` 的失败列表 | 本机 + `gh run view --json` | 已解决（t66 acceptance 第 1 条） |
| 4 | **`Clippy` 类清零（唯一的实质阻塞）** | `0da0cb68`：`E0425 backup_surface @ crates/memory/src/lifecycle.rs:535:9`；`cb550730`：同文件 `605/617` warnings-as-errors；本机 `-D warnings` **exit=0** | `gh run view --job <ubuntu job> --log` + 本机 clippy | **推送中的 `877a909`（warn-free memory）就是它的修复**；收尾时 in_progress |
| 5 | `Test` 类清零（尤其 `e2e_daemon`） | 历史 33 次；本机该 target **23 passed**；**CI 上 `Test` 在 `0da0cb68` 里被 skipped（因 Clippy 先红）⇒ CI 侧尚未有过它的读数** | 本机；CI 待 `877a909` 结果 | 部分（t66 acceptance 点名 e2e_daemon.rs） |
| 6 | Panel `Audit self-test` 类清零 | 历史 12 次；本机 exit=0；CI **Panel job 两次真跑都 success** | 本机 + `gh run view --json jobs` | 已解决；「真测量」接线属 **t99**（`design-audit.yml` **不存在**） |
| 7 | 级联红可读性 | §5：1 个缺陷 → ubuntu 3 红步 + windows 1 红步；`Test` 因 Clippy 被 skip | `gh run view --json jobs` 的 `skipped` 列表 | 这是**形状**不是缺陷；读日志必须按 skipped 还原（t66 的 evidence 设计本来就要求如此） |
| 8 | windows job 第一次启用 | `ci.yml:208` `if: push && refs/heads/main`；`0da0cb68` 的 windows 侧为首次真跑 | 读 ci.yml + run jobs | 同 #4 的根因（内存 crate），非 windows 特有 |
| 9 | **未取到**：`Test evidence` 的 per-target 计数 | 需要一条**编译成功**的 run 才会产生可读日志；两次真跑都因同一编译错而「日志读不到」 | —— | 待 `877a909`/下一次绿 run |
| 10 | `Install protoc` 环境 flake | 仅 09-11 四次，之后 15 天未复现 | 历史读数 | 不建议开单 |

**结论一句话**：**「第一次绿」的历史阻塞（Format、Guard/F1、panel、audit self-test、e2e_daemon）在当字节上都已消解；第一次真跑只剩一个根因 —— `crates/memory/src/lifecycle.rs` 的编译/警告状态，而它正是 09-28 那三次推送在修的东西（`877a909` 收尾时仍在跑）。** 级联让「一个缺陷」看起来像「四步都坏」。

## 8 不覆盖什么（第 19 条）

1. **不预测未来 run**：不说 `877a909` 会不会绿、要几次；本报告所有数字都是**过去 run 的读数**或**当字节的本地复现**。
2. **不验证 GitHub 端设置**：分支保护、environment、仓库默认 workflow 权限、Actions 审批、组织级 token 限制 —— 都没查（也无权限）。
3. **不改任何文件**（唯一写入 = 本报告）。
4. **不验证推送本身**：谁推、推哪些字节、是否 `git add` 全 —— 人的动作。
5. **不覆盖 ci.yml 的 action 版本固定**：ci.yml 的 `uses:` 仍浮动（`@v4`/`@stable`/`@v2`）；t100 只对 `release.yml` 做过，ci.yml 属 t66/t99 面。
6. **不覆盖 macOS**（CI 无该 job）；**不覆盖 runner 镜像/工具链漂移**。
7. **不分类 `e2e.yml` 的 15 次红**（只给计数）。
8. **不覆盖「绿之外的质量」**：门禁绿 ≠ 产品对。

## 9 方法学注记与纪律回执（都是这轮真踩到的）

1. **移动靶**：开工时远端 main = `5db881d8`（最后那次红），审计中途团队推了 `0da0cb68`/`cb550730`/`877a909` ⇒ 我先前"唯一阻塞 = Guard 的 6 条引用"的结论在收尾时已被团队自己的推送修掉。**教训**：审计进行中的仓库，读数必须写 sha + 时间，否则结论会秒过时（本报告 §6 分 A/B 两段就是这个原因）。
2. **中途读到半成品**：第一次用 PyYAML 解析 ci.yml 报 `ScannerError … line 62, column 20` —— 那是 **t66 正在改这个文件时的中途读取**；稍后同一文件解析 OK 且第 62 行已是带引号的 `- name: "Guard: …"`。别把并发写当成文件缺陷。（收尾时 ci.yml 与 `e2e.yml`/`release.yml` 都解析 OK；`design-audit.yml` 确实**不存在**。）
3. **同一命令两次不同结果要都报**：`injection_e2e --include-ignored` 首次 exit=101（编译窗口）→ 重试 exit=0/7 passed；`-p ruagent-memory --lib` 在 workspace 跑里 2 failed → 单跑 exit=0/79 passed。两边的读数都写在 §6，不挑好看的。
4. **绕过团队 wrapper 会得到假红**：直接 `cargo test … --ignored --list`（自设共享 `CARGO_TARGET_DIR`）报 `os error 2`；走 `scripts/cargo-team.ps1` exit=0 且给出 15 条 ⇒ 门禁读数必须走 wrapper。
5. 纪律：`gh` 只用 `list`/`view`/`api`（只读）；**未** dispatch/rerun/cancel、**未**建 tag、**未** push；未改非报告文件；临时文件已清（`%TEMP%\t101-*.sh|*.txt|*.log`、WSL `/tmp/t101-*`）；未写活库；未启停 pid 79984。
