# CI 加固：跳过必须可见 + 计数口径 + 门禁覆盖声明（t65）

> 状态：**交付完成**；两条契约门禁的**当下读数都带阻塞归因**（§9：编译面 vs 测试面、文件:行、错误原文），阻塞全部来自 out-of-scope 在途编辑，按第 10 条**只报不改**。
> 产物：`.github/workflows/ci.yml`、`.github/workflows/e2e.yml`、`.github/workflows/scripts/test-evidence.sh`（新）、`.github/workflows/scripts/check-workflow-refs.sh`（新）、本报告。作者 recall，时间窗 2026-09-28T03:0x → 22:1x。

**一句话**：把本代买来的四种「没读数的绿」写进流水线 —— ①空跑绿 ②缓存假绿 ③旧二进制 ④静默跳过 —— 手段是**计数口径机械化**（逐 target 行数 + 退出码，0 target = 错误）、**`--no-fail-fast`**（否则计数按构造残缺）、**跳过必须可见**（能跑的真跑并点名校验；不能跑的显式登记 + 数量对账）、**每 job 声明不覆盖什么**、**Windows 重试只对已知 race 放开**、以及一条**接线后才生效的 F1 自检**（工作流引用的路径必须在同一次提交里被跟踪）。

---

## 1 计数口径 + `--no-fail-fast`（第 22 条 + 船长裁决）

**口径机械化在 `test-evidence.sh`**（它自己头部就写着 "never a sum of (N) passed"）：每个测试步 tee 日志，然后由它输出**逐 target 行数与退出码**（`Running`/`Doc-tests` 行、`test result:`、`test result: ok`、`test result: FAILED`、`panicked at`、exit code），并且 **0 个 reporting target = `::error::` + exit 1**。第 17/22 条**早已由此机械化**，本单是把它的读数接进流水线并补两处漏洞（§2），不是新造口径。

实测（本地同形命令，走 `scripts/cargo-team.ps1`）：

| 输入 | 读数 | 判 |
| --- | --- | --- |
| 健康日志（1 target ok） | `1 target(s) ok, 0 failed, 0 panic site(s), exit 0` | exit **0** ✓ |
| 空日志（0 个 `test result:`） | `::error::empty reported 0 targets -- an empty run is not a green` | exit **1** ✓ |
| 日志不存在 | `::error::missing: evidence log '…' is unreadable -- a run whose log we cannot read is not a green` | exit **1** ✓（**修前是 exit 0**，见 §2） |
| 真实红跑 | `targets reporting 1 / FAILED 1 / panicked 1 / exit 1` + 失败 target 与 panic 站点逐行列出 | exit **1** ✓ |

**`--no-fail-fast` 落点：`rust-linux` 的默认 Test 步 + `rust-windows` 的 attempt 1 与 attempt 2**（三处 `cargo test --workspace` 全部加）。**理由是可测的，不是风格**：

```
改前（plain `cargo test --workspace`，2026-09-28 22:0x）：
  targets launched 18 / reporting 9  -> cargo 在 `-p ruagent-graph --lib` 失败处停止发射后续 target
  该 target：test result: FAILED. 5 passed; 1 failed
  失败测试：community::tests::a_failed_rebuild_leaves_the_previous_partition_intact
           panicked at crates\graph\src\community.rs:516  (assertion `left == right` failed: still six member rows: left 12, right 6)
改后（`--no-fail-fast`，2026-09-28 21:5x 同树同形状全量跑）：
  targets launched 56 / reporting 56 / ok 56 / FAILED 0 / panicked 0 / exit 0
⇒ 差值：plain 形态下 **47 个 target 从未报告**（56-9）；「0 targets = error」也挡不住「后面的 target 根本没跑」。
```
（56 vs 45 的旧读数差是 `Doc-tests` 行：`Running` 只数 unittests/集成二进制，我已把模式补成 `Running|Doc-tests`，两行现在一致 —— 这也是本单自查出的一个读数不自洽。）

## 2 `test-evidence.sh` 缺日志时静默绿（船长 review 逮到）+ 两条负控

**病**：日志不存在时每个计数都是空串，`[ "" -eq 0 ]` 打印 `integer expression expected` 并**静默为假** ⇒ 脚本报 `0 target(s) ok … exit 0` —— **一条为阻止静默绿而写的门禁，自己在静默地绿**。

**修法（两件，判据都给）**：
```bash
[ ! -r "$log" ] && { echo "::error::$label: evidence log '$log' is unreadable -- a run whose log
                                we cannot read is not a green (t65)"; exit 1; }
total=$(grep -c '^test result:' "$log" || true); total=${total:-0}   # 同 fail/ok/panics
```
**负控读数**：删日志 ⇒ **exit 1**（修前 exit 0）；日志存在但空 ⇒ **exit 1**（今天已是红的，未退化）；健康日志 ⇒ exit 0。

## 3 F1：把那个**死脚本**接上线（跟踪入库 + 真的调用 + 失败即红）

`.github/workflows/scripts/check-workflow-refs.sh` 之前**全仓 0 调用点**（与 `RUAGENT_REQUIRE_MOCK` 同形：**谁打开它？没人**）。本单：① 在 `ci.yml` 里加步 **`Guard: workflow-referenced paths must be tracked`**（`bash .github/workflows/scripts/check-workflow-refs.sh`，失败即红）；② 两脚本与 workflow **必须同一次提交**（§4）。

**三条口径**（写在脚本头部，且是实测代价换来的）：只取 **`run:` 体里可执行位置的引用**（executor 后一个 token、`working-directory:`）；相对路径按**该 step 自己的 `working-directory`** 解析（`workdir` 在每个 `- name:`/`- uses:` 处**重置**）；**排除注释与散文**，且只认**未被引号包裹**的 token（`"node e2e/run-e2e.mjs"` 是字符串比较，不是执行）。**两个实测误报**（天真版会当场误报三条，其中两条是真实路径）：`scripts/build-panel.mjs` → 真身 `panel/scripts/…`、`tools/design-audit.mjs` → 真身 `panel/tools/…`、`docs/design/reviews/gen2-ci-hardening.md` → 头部注释**散文**。

**读数**：
```
今天的真实状态（本就该红）：references checked 15, not tracked/missing 6
  ci.yml 5×`.github/workflows/scripts/test-evidence.sh` UNTRACKED
  ci.yml 1×`.github/workflows/scripts/check-workflow-refs.sh` UNTRACKED
  （误报已消除：`panel/e2e/run-e2e.mjs` 由 7 bad → 判为 tracked）
负控（故意引用未跟踪的 `scripts/_t65_pretend.sh`）：该路径被点名为 UNTRACKED ⇒ exit 1
绿色可达性（把真实 ci.yml/e2e.yml/release.yml + 两脚本 + 被引用文件放进临时 git 仓库并 `git add`）：
  references checked 15, not tracked/missing 0  ->  "every executed path a workflow references is tracked"  exit=0
```
⇒ 它**不是**一条永远红的门禁；今天红**正是 F1 本身**（两脚本未跟踪）。

## 4 两条未跟踪脚本必须与 workflow 同一次提交

`test-evidence.sh` 与 `check-workflow-refs.sh` 均为 `??`（未跟踪），被 `ci.yml` 引用：`test-evidence.sh` **5 处**（`:111 :131 :243 :264 :274` 附近）+ 自检 **1 处**。若不同批提交，fresh checkout 上 **evidence 步与自检步直接红**（不是「CI 变弱」）。这条约束现在由 §3 的自检**持续**保证。

## 5 跳过必须可见：清单对账 + 缺件/有件两组读数

`cargo test --workspace -- --ignored --list` 是**唯一真相源**（今天 15 条：store 3 · graph 3 · knowledge 2 · daemon 7）。`rust-linux` 的两步：
- **能跑的 7 条（daemon / mock-agent）**：`cargo test -p ruagent-daemon --test injection_e2e -- --include-ignored`，且 helper **点名索要**这 7 个测试名（`--include-ignored` 单独还挡不住「新增一条 `#[ignore]` 悄悄跳过」）。
- **不能跑的 8 条**：manifest 步**显式登记**（store 3：需要真实历史的迁移副本 / graph 3：需要真实图库副本且拒绝任何含 `.ruagent` 的路径 / knowledge 2：需要真实语料 + e5 模型），并把 `found == covered(7) + declared(8)` 当判据 ⇒ **新增一条 `#[ignore]` 会让 CI 红并列出清单**。

**两组读数**：
```
缺件 ⇒ FAILED（不是 skip 计 passed）
  store 3 条（`test -p ruagent-store -- --include-ignored`，无 env）：
    migrations.rs:775 / migrations.rs:902 / lib.rs:1290 三处 body-head expect panic
    test result: FAILED. 32 passed; 3 failed   exit 101
  daemon 7 条（把该 target 的 exe 单独拷到没有 mock 二进制的目录再 `--ignored`）：
    7 × `test … FAILED` + 7 × `needs ruagent-mock-agent next to this test binary: … ` exit 101
有件 ⇒ 通过
  daemon 7 条（`--include-ignored`，mock 二进制由 `cargo test --workspace` 建在同目录）：
    7 × `test … ok`，`test result: ok. 7 passed`，helper「all 7 required test name(s) reported ok」，exit 0
```

## 6 Windows 重试：从「无条件 `||`」改成「只对已知 race」

**证据（run 36272344732 / job 108488556558，2026-09-26）**：attempt 1 与 attempt 2 **逐字相同**地失败在 `distill_graph_toggle_controls_entity_extraction` + `panicked at crates\mock-agent\tests\e2e_daemon.rs:2161`（各 `test result: FAILED. 22 passed; 1 failed`），最终 `##[error]Process completed with exit code 1`；而这条重试本来要吸收的 **`E0786` 在整份日志里 0 次出现** ⇒ 这次重试**只把墙钟翻倍、什么都没吸收**，且能吸收的是**构建期** race。

**处置（可证伪）**：attempt 1 的日志里出现 `panicked at` 或 `test result: FAILED` ⇒ **拒绝重试并红**（测试级失败永远不是 rmeta race），只有**无测试级失败**（构建/工具链级）才重试一次；每一次都跑 §1 的计数并把 attempt 1 的失败**原样印出**。对上面那份真实日志的 dry-run：`panicked at` 2 行、`FAILED` 2 行、`E0786` **0** 行 ⇒ 规则判定「拒绝重试」（即那次会更快红、且不带侥幸）。

## 7 `e2e.yml`：入口守卫 + 跳过可见

- **确认走 repo 入口**：`panel/package.json` 的 `test:e2e` = `node e2e/run-e2e.mjs` ✓，且 `run-e2e.mjs` 设 `RUAGENT_E2E_ALLOW_WRITES=1` ✓（写型 spec 只有经此入口才会跑）。新增 **`Entry-point guard` 步**把这两点变成**持续检查**：`package.json` 改道、或 `run-e2e.mjs` 不再武装写入 ⇒ **红**。
- **跳过可见**：Playwright 输出 tee 到日志，evidence 步打印 `N passed/failed/flaky/skipped` 行数 + 退出码 + **实际 skipped 行**；且 **`registry.spec.ts` 被 skip = 错误**（它只在写守卫武装时运行，skip 意味着写路径没被测）。
- 覆盖声明：本 job 不覆盖 Rust 测试套件、不覆盖 Windows、不覆盖那 8 条活库仪器。

## 8 每 job 的不覆盖声明（第 19 条，均已落到 `$GITHUB_STEP_SUMMARY`）

`rust-linux`：不覆盖 `panel/**`（TS 类型/构建）· 不覆盖 Windows 特有路径 · 不覆盖 8 条需真实历史的活库仪器 · 不覆盖 Playwright。`rust-windows`（仅 main push）：不覆盖 panel/Playwright · 不覆盖那 8 条 · 不覆盖「ubuntu 上以 `--include-ignored` 点名的 daemon 7」· 不覆盖只在 PR 出现的问题。`panel`：完全不覆盖 Rust · 不覆盖 Playwright e2e · 不覆盖打包后的运行期行为；并说明计数单位是退出码 + `error TS` 行数（本 job 没有 Rust test target）。`e2e smoke`：不覆盖 `cargo test`（只 build）· 不覆盖 Windows · 不覆盖 8 条活库仪器。

## 9 门禁读数（编译面 vs 测试面**分开报**）+ 阻塞归因

| 时点 | 命令 | 读数 | 归因 |
| --- | --- | --- | --- |
| 21:5x | `test --workspace --no-fail-fast` | **56 launched / 56 reporting / ok 56 / FAILED 0 / exit 0**（247s） | 绿 |
| 22:0x | `test --workspace`（**契约字面命令**） | **18 launched / 9 reporting**，`-p ruagent-graph --lib` `FAILED 5 passed; 1 failed`（`community::tests::a_failed_rebuild_leaves_the_previous_partition_intact`，`crates/graph/src/community.rs:516`：`still six member rows: left 12, right 6`），exit 101 | **测试面红**，out-of-scope（graph 在途；同一次日志里可见 `READING t81 #2` 等注入失败读数） |
| 22:0x | `cd panel; npm run build` | **exit 1**，`error TS` 5 行：`Memory.tsx(82,47) TS2339 Property 'rows' does not exist on type 'RecallLogPage'`、`(156,27)` 同、`(334,60)/(344,29)/(344,32) TS7006` | **编译/类型面红**，out-of-scope（`t83` 三处同改在途：`api.rs` 已 `"rows"`、`knowledge_api.rs` 已 `["rows"]`、`Memory.tsx` 正在改） |
| 22:0x | 同上 workspace 命令的一次中途跑 | `crates/mcp/src/lib.rs:484` `E0433: cannot find module or crate 'anyhow'` | **编译面红**（mcp 在途），随后已自愈 |

**没做的事**：没有为了让任何一条变绿而缩小 gate 范围；没有替同伴改任何文件（第 10 条）。

## 10 未测（不静默跳过）

1. **CI 本身没跑过**：本单不推送（推送在 t66）⇒ 上面全部是**本地同形命令**读数，不是 GitHub 上的执行读数。
2. **自检在真实树上的绿**未取得（两脚本未跟踪 ⇒ 按定义就是红的）；绿可达性用**临时 git 仓库**证明（§3）。t66 提交后应自然转绿 —— 这也是它的第一个真读数。
3. `--ignored --list` 的 workspace 形态在**某 crate 编译不过**时会红（当次 mcp E0433 就是这样）—— 这是**有意的**（编译不过就测不了），但它意味着 manifest 步的成败也依赖全仓可编译。
4. 我自己踩到的两个探针坑（记下来免得下一个人再踩）：`Select-String` 过滤把 `error[E0308]` 藏掉了一整轮（我看到的只是「2.5s exit 101」）；`Select-Object -First N` 会**提前关闭上游管道**，让 `Tee-Object` 的日志被截断（那次 daemon 缺件日志因此少了 `test result:` 行）—— **探针不能只看自己想看的行**。
