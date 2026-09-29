# 判定步假红修复（一处根因四处症状）（t104）

> 状态：**完成，等 captain 推送**（按契约：不许 push / dispatch / rerun / cancel / 建 tag）。
> 产物：`.github/workflows/ci.yml`、`.github/workflows/e2e.yml`、`.github/workflows/scripts/check-workflow-refs.sh` + 本报告。
> `git diff --stat`：**3 files changed, 167 insertions(+), 25 deletions(-)**（ci.yml +123/-… · e2e.yml +57/-… · check-workflow-refs.sh +12）。作者 recall，时间窗 2026-09-28T05:0x → 05:4x。
> 唯一结论：**一条永远为红的判定步比没有判定步更坏 —— 它把真红淹没。**

---

## 1 逐条验收 + 负控

| # | 验收项 | 读数 | 判 |
| --- | --- | --- | --- |
| ① | 两处 `rc` 兜底表达式改成「run 步就地取状态 + 该步真实退出码」，evidence 步只读 rc 文件 | `grep 'job\.status' .github/workflows/*.yml` = **0 命中**；三份 workflow 的 `run:` 体里 `rc=${PIPESTATUS[0]}` + `echo "$rc" > "$rcfile"` + `exit "$rc"`（含两处 `Test`/`Playwright` 步） | **达标** |
| ② | 派生红指向上游、不再叠加第二个红 | 4 个分支实跑：ci 缺日志 ⇒ `exit 0` + 「因为 `Test (default run -- skips stay countable)` 没跑」；ci 日志是编译错误 ⇒ `exit 0` + 「the tree does not compile」+ 原样贴 `error[E0305]`；windows 缺日志/编译错误同形；manifest 无法 `--list` ⇒ `exit 0` + 「no skip accounting possible」 | **达标** |
| ③ | `Test attempt 1` 的退出码 = cargo 的（⇒ attempt 2 的前置恢复语义） | 步内 `rc=$?; echo "$rc" > …; exit "$rc"`；此前该步以 `echo` 结束 ⇒ 恒 0 ⇒ `steps.attempt1.outcome` 永不为 `failure` ⇒ attempt 2 永不触发（0da0cb6 实测 skipped） | **达标** |
| ④ | 跳过要**名字** | `npm run test:e2e -- --reporter=list --reporter=html`（**未动 `panel/**`**）；证据步打印 `distinct skipped SPECS (named)` + 原始 skip 行；守卫**只在** `registry.spec.ts` 时红。实跑：「另一条 spec 被 skip」⇒ `exit 0` 且**打印名字** `failure-visibility.spec.ts`；「registry 被 skip」⇒ `exit 1` + `::error::registry.spec.ts was SKIPPED` | **达标** |
| ⑤ | workflow 本地可解析做成固定一步 | 一条命令：`bash .github/workflows/scripts/check-workflow-refs.sh`（脚本末尾会打印这句 PRE-SUBMIT COMMAND）。三份 workflow 实读 `ci.yml: OK / e2e.yml: OK / release.yml: OK` + 逐份 `parses as YAML (PyYAML)`；**PyYAML 缺失时不再静默**：改为打印 `PARSE CHECK SKIPPED (<原因>)` 并说明「CI 上会跑」/「未加引号 `: ` 扫描不受影响」 | **达标** |
| 负控 | 三条必须能红 | 三种情形全部实跑（下表 10/10）：rc=1 ⇒ **红**；健康 ⇒ `playwright: exit 0, no skipped registry.spec.ts`；skip 一条 ⇒ **打印名字**，仅 registry ⇒ 红 | **达标** |
| 纪律 | 只改 `.github/workflows/**` | `git status --porcelain -- .github` 只有我改的 3 个文件；`panel/`、`crates/`、`cli/` 零改动；未 push / dispatch / rerun / cancel / tag | **达标** |

**方法说明（为什么负控是「被测物本身」而不是副本）**：脚本用 PyYAML 读出**工作流文件里那一步的 `run:` 体**，原样写成 `%TEMP%\ra-t104\body-*.sh` 再执行 ⇒ 负控跑的就是**即将被推送的那段字节**（不是手抄的复制品）。提取数：**3 个 run-body**（e2e 证据步 · ci linux 证据步 · ci 清单步）。

---

## 2 根因与四处症状（改前 → 改后）

**根因（一行）**：`rc=${{ job.status == 'success' && 0 || 1 }}` —— GitHub 表达式里 **`0` 是 falsy** ⇒ `true && 0` 求值为 `0` ⇒ `|| 1` 兜底 ⇒ **恒等于 1**。出现在 **两处**：`ci.yml` 的 linux `Test evidence`、`e2e.yml` 的 `E2E evidence`。

**铁证（captain 提供，我逐行核过）**：run `36480566583`（`cb55073`）中，证据步之前的每一步都 success（含 `Panel E2E (Playwright)` = **44 passed / 1 skipped**），证据步仍打出 `| exit code | 1 |` 并走 `::error::Playwright run failed (exit 1)` ⇒ **只要这步在，E2E 永不可能绿**，而它测的东西根本没坏。

| 症状 | 改前读数 | 改后读数 |
| --- | --- | --- |
| ① ubuntu `Test evidence` 双因红 | rc 恒 1 **且** 上游 `Test` 被 `Clippy` 短路 ⇒ 日志不存在 ⇒ 两个错因叠在一个红步上（run `0da0cb6`） | 缺日志/编译错误 ⇒ 输出**因为 `<上游步名>` 没跑 / 树不能编译，所以没有读数** + `exit 0`（一次编译错误只留一个真红） |
| ② windows `Test attempt 1` 吞退出码 | 步以 `echo` 收尾 ⇒ 恒 0 ⇒ `steps.attempt1.outcome` 永不为 `failure` ⇒ `Test attempt 2` **skipped**（实测） | `exit "$rc"` ⇒ outcome 说真话 ⇒ attempt 2 按「只对非测试级失败」的规则触发 |
| ③ `if: always()` 步的派生红 | run `0da0cb6` 上**一次编译错误被放大成 4 个红步**（`Clippy` + `Test evidence` + manifest + windows evidence） | 上述 4 个分支全部改为**指向根因的通知**；真红只剩根因那一步 |
| ④ 「1 skipped」只有数字没有名字 | CI 用 `panel/playwright.config.ts:41` 的 **github reporter**，日志只有进度点 + `1 skipped`；我的 marker 正则匹配 **0** ⇒ 第 19 条违规 | CLI 加 `--reporter=list --reporter=html`（不动 `panel/**`）⇒ 逐条 spec 名进日志；证据打印**名字**并在只有 `registry.spec.ts` 时红 |

**正面读数（要保住的，改后仍在）**：证据步**确实拒绝**把「0 targets」当绿 —— 实跑「真·空跑（0 个 `test result:` 且日志里无编译错误）」⇒ `exit 1` + `::error::… reported 0 targets -- an empty run is not a green` ✓；「真实测试失败 rc=1」⇒ `exit 1` + 逐行列出失败 target 与 panic 站点 ✓。

---

## 3 三条负控（+ 全部 10 例）读数

```
=== workflow 解析 / 表达式残留 ===
  ci.yml: OK, jobs=['rust-linux', 'rust-windows', 'panel']
  e2e.yml: OK, jobs=['smoke']
  release.yml: OK, jobs=['gate', 'build', 'release']
  job.status expressions left in workflows: 0 (must be 0)

=== 负控（每条跑的是从 workflow 抽出的 run-body）===
  [OK ] e2e ① rc=1 / Playwright failed -> the step MUST be red          exit=1
  [OK ] e2e ② healthy -> exit 0 + the exact success line                exit=0
  [OK ] e2e ③ another spec skipped -> exit 0 AND the NAME printed       exit=0   (name: failure-visibility.spec.ts)
  [OK ] e2e ③b registry.spec.ts skipped -> MUST be red, with the name   exit=1   (::error::registry.spec.ts was SKIPPED)
  [OK ] e2e ④ missing log (upstream step failed) -> notice, NO second red  exit=0  ("because `Panel E2E (Playwright)` did not run")
  [OK ] ci  ④ missing log -> notice, NO second red                      exit=0   ("because `Test (default run …)` did not run")
  [OK ] ci  ⑤ compile error in the log -> notice, NO second red         exit=0   ("the tree does not compile" + error[E0305] 原样)
  [OK ] ci  ⑥ GENUINE empty run (no compile error) -> the guard still fails  exit=1  ("reported 0 targets")
  [OK ] ci  ⑦ real test failure rc=1 -> red with the failing target named    exit=1  (FAILED + panicked 逐行)
  [OK ] ci  ⑧ manifest: tree does not compile -> notice, NO second red  exit=0
[t104] negative controls: 10/10 behaved as required
```
（完整输出另存 `%TEMP%\ra-t104-nc.txt`。）

**修复片段（改后形状，两个文件同形）**
```yaml
# run 步：状态来自命令自身
set +e
cargo test --workspace --no-fail-fast 2>&1 | tee "$RUNNER_TEMP/t65/linux-default.log"
rc=${PIPESTATUS[0]}
set -e
echo "$rc" > "$RUNNER_TEMP/t65/linux-default.rc"
exit "$rc"                       # <= 该步的结论就是命令的结论
# evidence 步：只读 rc 文件；缺读数时点名上游并 exit 0（不再叠加第二个红）
if [ ! -r "$log" ] || [ ! -r "$rcfile" ]; then … "because \`$upstream\` did not run" …; exit 0; fi
bash .github/workflows/scripts/test-evidence.sh "…" "$log" "$(cat "$rcfile")"
```

---

## 4 ⑤ 的落地：提交前固定一步 + 三份 workflow 解析读数

```
$ bash .github/workflows/scripts/check-workflow-refs.sh
…
workflow path references checked: 15, not tracked/missing: 0
  .github/workflows/ci.yml: parses as YAML (PyYAML)
  .github/workflows/e2e.yml: parses as YAML (PyYAML)
  .github/workflows/release.yml: parses as YAML (PyYAML)
every executed path a workflow references is tracked
PRE-SUBMIT COMMAND (t104): bash .github/workflows/scripts/check-workflow-refs.sh   # refs tracked + workflows parse
```
- **F1 现在是绿的**（两脚本随 `0da0cb6` 入库 ⇒ `not tracked/missing: 0`；在 t65/t66 时代它是红的，那正是 F1 本身）。
- **PyYAML 缺失的行为改了**：过去是**静默跳过**（等于把没跑的一半写成通过），现在打印 `PARSE CHECK SKIPPED (<原因>)` 并说明「CI 上会跑」+「未加引号 `: ` 扫描不受影响」。
- 本机实测：`command -v python3` = `/c/Users/19410/.local/bin/python3`、`pyyaml ok` ⇒ 本机走的是**完整解析**分支；上面三行 `parses as YAML` 就是它。

---

## 5 这条纪律的实例（报告要求写入结论）

> **一条永远为红的判定步比没有判定步更坏 —— 它把真红淹没。**

两个实例（都在 `0da0cb6` 那一代）：
1. **ubuntu `Test evidence` 双因红**：rc 恒 1（根因）+ 上游 `Test` 被 `Clippy` 短路（日志不存在）⇒ 读者看到「Test evidence FAILED」，很容易读成「测试失败」，而真实情况是**编译错误**且 `Test` 根本没跑。
2. **windows `Test attempt 1 evidence` 报 `0 targets` 且 `FAILED` 行数 = 0**：同样是编译错误 + rc 恒 1 的组合；那一次**没有任何测试失败**，但红步数被放大到 4 个。

改后的形状把这两件事分开：**根因红在它自己的步上**，其余步只说「因为 X 没跑/树不能编译，所以我没有读数」。

---

## 6 不覆盖什么（第 19 条）

1. **真实的 Playwright `list` reporter 跳过行格式，我**没有**用真跑验证**：本机跑 `npm run test:e2e` 会**武装写守卫**（`run-e2e.mjs` 设 `RUAGENT_E2E_ALLOW_WRITES=1`）并且需要真守护进程 —— 而 **pid 79984 是真活库的守护进程，我不许碰**。⇒ 负控③/③b 用的是**按 list reporter 文档格式构造的合成日志**；确认它的是**推送后那次 CI 真跑**（届时证据步会打印真实 spec 名）。这是本单最大的未测项，明确登记。
2. **整条 E2E job**（`Build` / `Boot daemon` / `Doctor` / `Panel serves` / `Playwright`）本机未跑；本单只改其**判定与计数**形状。
3. **windows `attempt 2` 的真跑路径**未重放：本单只改 rc 的取值方式；「只对非测试级失败重试」的判定逻辑未动（t66 曾用 `0da0cb6` 的真实日志 dry-run 过：`panicked at` 2 行 / `E0786` 0 行 ⇒ 拒绝重试）。
4. **html reporter 的输出路径**未在 CI 侧验证：CLI 只加 `--reporter=…`，`upload-artifact` 的 `panel/playwright-report` 路径沿用 `playwright.config.ts` 的默认（`panel/**` 未动，按契约）。
5. `--reporter=list --reporter=html` 的**多 reporter 并存**在本机未验证（Playwright 支持重复 `--reporter`，但我没有实跑证据）—— 与第 1 条同一个未测项。

## 7 顺手发现的缺陷（只报不改，供审计单）

1. **`check-workflow-refs.sh` 每个引用调一次 `git ls-files`**（本跑 15 次）⇒ 本机 git-bash 实测 **111 s**（`sys 1m5s`，进程启动开销；CI/ubuntu 上是秒级）。**结论：不是缺陷，但是可优化点**（一次 `git ls-files` 建集合再查表）。我**没有**改它（不在本单的必修项里，且改动会扩大 diff）。
2. **两个守卫脚本至今没有被独立审计/评审**（状态面：`t65 has unaudited path …test-evidence.sh / check-workflow-refs.sh`）—— 它们现在**真的在守护流水线**（F1 守卫已在 `0da0cb6` 拦截过一次）。建议的审计面：`test-evidence.sh` 的计数口径与退出码语义 · `check-workflow-refs.sh` 的「欠近似」边界（哪些引用**不会**被检查）· 两个脚本在不同 shell（bash on Linux / git-bash on Windows）上的行为一致性。

## 8 纪律回执

- 写入集合：`.github/workflows/ci.yml` · `.github/workflows/e2e.yml` · `.github/workflows/scripts/check-workflow-refs.sh` · 本报告（**全部在 inScope**）。
- **未** push / dispatch / rerun / cancel / 建 tag；**未**动 `panel/**`、`crates/**`、`cli/**`。
- 临时残留：`%TEMP%\ra-t104\`（NC 夹具与抽取出的 run-body）、`%TEMP%\ra-t104-nc.txt`（完整 NC 输出）、`%TEMP%\ra_t104_nc.py`（驱动的探针）—— 都在 `%TEMP%`，仓库内**零临时文件**（`git status --porcelain -- .github` 只有 3 个被我修改的跟踪文件）。
- 不涉及 `.rs` ⇒ 无需 fmt/clippy（本单未碰任何 Rust 文件）。

---

# §9 t110 追加：真实日志推翻了 t104 的取数 —— 「跳过点名」的收口

> 这一节替换 §6.1 里那条「未用真跑验证」的未测项。t104 的诚实标注是对的，**现在有了反证**：CI run `36485138690`（`5d3adfd`）打出 `| summary N skipped lines | 1 |` + `| skip markers with a spec name | 0 |` ⇒ **有 1 个 skip，0 个名字**。

## 9.1 真实运行怎么做的（临时 root + 临时端口，活库只读）

按 t107 的新规则显式命名目标：`ruagent serve --addr 127.0.0.1:18787 --root %TEMP%\ra-t110-root`（**不是** 8787、**不是** `~/.ruagent`），config/agents.toml 按 CI 的 `Register e2e mock agents` 步镜像。两次真跑：

| 运行 | 命令 | 真实结果 |
| --- | --- | --- |
| **A** | `E2E_BASE_URL=http://127.0.0.1:18787 npm run test:e2e -- --reporter=list --reporter=json`（入口点 ⇒ **写入闸门武装**） | `Running 51 tests` ⇒ **`51 passed (18.3s)`，0 skipped**，exit 0 |
| **B** | `npx playwright test --reporter=list --reporter=json`（**无** `RUAGENT_E2E_ALLOW_WRITES`） | `Running 51 tests` ⇒ **`6 skipped / 45 passed (14.6s)`**，exit 0 |

⇒ A/B 的差别正是闸门：那 6 条**受写入闸门保护的 spec** 在 B 里自己 skip。**这也说明 CI 那个 `1 skipped` 不是这 6 条里的**（CI 走入口点 ⇒ 闸门武装 ⇒ 它们会跑）；它到底是哪一条，**下一次 CI 就会点名说出**。

## 9.2 ① `--reporter=list` 的真实形状：**它不给 skip 起名字**

run B 真日志里与 skip 有关的**全部**内容（原文，`%TEMP%\ra-t110-runB.log`）：
```
Running 51 tests using 8 workers
[6/51] e2e\failure-visibility.spec.ts:190:1 › P3: a failed mount probe says so on the CHIP, ...
[9/51] e2e\failure-visibility.spec.ts:292:1 › F1 (t103): with EVERY probe failed the models readout carries no number
...
  6 skipped
  45 passed (14.6s)
```
`grep -c skipped` = **1 行**（就是 `6 skipped`）；`grep -E '^ *- .*\.spec\.ts'` = **0 命中**。
⇒ **`list` 只给「它跑过的」测试打名字；被 skip 的只进汇总计数。** 我 t104 写的 `skip markers with a spec name` / `distinct skipped SPECS (named)` 两行在这份真日志上必然是 `0` —— 与 CI 现象**逐字一致**。**这是取数口径选错，不是 CI 环境问题。**

## 9.3 ② 换成可判定的取数：JSON reporter（以真日志为准）

run 步加 `PLAYWRIGHT_JSON_OUTPUT_NAME=$RUNNER_TEMP/t65/playwright.json` 与 `--reporter=json`（与 `list` + `html` 并存）。**同一份真日志**下 JSON 报告给出：

```
stats.skipped = 6 | expected = 45 | unexpected = 0 | flaky = 0
NAMED skipped specs: 6
   consumption.spec.ts :: Wiki page: a page with no build reading shows coverage unknown :: [skipped]
   consumption.spec.ts :: Memory page: the recall log labels each score with its dimension :: [skipped]
   consumption.spec.ts :: Knowledge page: a hit's score carries its dimension :: [skipped]
   consumption.spec.ts :: Graph page: an entity reaches the DOM :: [skipped]
   recall.spec.ts :: conservative recall stubs expand to the full memory :: [skipped]
   registry.spec.ts :: runtimes and roles can be created and deleted from the panel :: [skipped]
```
⇒ **每一条都能点名**（`file :: title`），而且 `annotations[].description` 还带**为什么**（这里是 write-guard 的理由原文）。**captain 猜的探针也对上了**：`recall.spec.ts :: conservative recall stubs expand to the full memory` 正是 6 条之一（它是**闸门条件 skip**，不是无条件 skip —— 走入口点时会真跑）。

**成对读数（同一套件、同一临时守护进程，只换取数方式）**：

| | 改前（list 取数；CI `5d3adfd` / 本地 run B 同形） | 改后（JSON 取数；本地 run B 真日志） |
| --- | --- | --- |
| 有 skip？ | 有（`1 skipped` / `6 skipped`） | 有（`stats.skipped = 6`） |
| **点出几个名字** | **0** | **6** |
| 名字内容 | — | `consumption ×4` · `recall.spec.ts` · `registry.spec.ts`（逐字来自报告） |
| registry 被 skip 时 | index 取不到名字 ⇒ **静默放过**（t104 的真实后果） | `::error::registry.spec.ts was SKIPPED` + exit 1 |

## 9.4 ③ 判定步的新形状（**不允许任何「永远绿」**）

- 取数：`node - "$json"`（本 job 必然有 node）递归读 `suites[].specs[]`，`tests[].status === "skipped"` ⇒ `file :: title :: reason`；同时回读 `stats.skipped`。
- **三条拒绝绿的护栏**：
  1. **JSON 报告缺失** ⇒ `::error::… this leg cannot name a skipped spec` + **exit 1**（不是警告）；
  2. **JSON 解析失败** ⇒ `::error::… could not be parsed` + **exit 1**；
  3. **`stats.skipped` 与实际点出的名字数不一致** ⇒ `::error::the JSON report says N skipped spec(s) but M could be NAMED` + **exit 1** —— **这条正是 t104 那个形状的守卫**：从此「有 skip 却点不出名字」**不可能报绿**。
- 原语义保留：**只有 `registry.spec.ts` 被 skip 才致命**（`::error::` + exit 1）；其它 skip **点名打印 + 计入表**，不红；`rc≠0` 仍红；上游被短路时仍是「指向上游的通知 + exit 0」（t104 形状不变）。
- 汇总表改为语义可判定的两行：`| summary \`N skipped\` lines (text log, a NUMBER only) |` 与 `| JSON report \`stats.skipped\` |` + `| **named skipped specs** |`。

## 9.5 负控（**全部用真实日志 / 真实 JSON 报告**，8/8）

方法同 t104：把 `e2e.yml` 那一步的 `run:` 体（**即将被推送的那段字节**，4071 字符）抽出来执行；夹具 = 本机真跑的 `ra-t110-runA/B.log` 与 `ra-t110-pwA/B.json`；需要「只含 registry skip」等形状时，对**真实报告**做一处声明过的字段改动（`stats.skipped` 同步改，保持自洽）。

| # | 用例（真实夹具） | 期望 | 实测 |
| --- | --- | --- | --- |
| ①a | run B 真报告**原样**（6 skip，含 registry） | 点名 6 + registry 致命 | **exit 1** + 6 行 `file :: title :: reason` + `::error::registry.spec.ts was SKIPPED` ✓ |
| ①b | 真报告，registry 的 status 改 passed（`stats.skipped` 6→5） | 点名 5、**不红** | **exit 0** + 5 行名字 + `playwright: exit 0, 5 skipped spec(s) named, none of them registry.spec.ts` ✓ |
| ② | 真报告，**只有** registry 是 skip | 仍致命 | **exit 1** + 唯一名字 `registry.spec.ts :: runtimes and roles…` ✓ |
| ③a | 有真日志、**无** JSON 报告 | 拒绝绿 | **exit 1** + `cannot name a skipped spec` ✓ |
| ③b | 真报告 `stats.skipped=6` 但无任何 spec 带 skipped（**t104 的失败形状**） | 拒绝绿 | **exit 1** + `says 6 … but 0 could be NAMED` ✓ |
| ④ | run A 真报告（0 skip） | 绿并说明 | **exit 0** + `(none: no spec was skipped)` ✓ |
| ⑤ | 真日志 + 强制 `rc=1` | 真红 | **exit 1** + `::error::Playwright run failed (exit 1)` ✓ |
| ⑥ | 无日志（上游被短路） | 上游通知、不叠加红 | **exit 0** + `NO READING … Derivative notice` ✓ |

⇒ `[t110] negative controls: 8/8 behaved as required`（全量输出：`%TEMP%\ra-t110-nc.txt`）。

## 9.6 未测项更新

- **[已结清]** t104 §6.1「真实 `list` reporter 的 skip 行格式未用真跑验证」⇒ 现在用真跑验证了，**结论是「它不点名」**，改法按真日志换成 JSON 取数。
- **[仍开放 · 1]** CI 上那个 `1 skipped` **到底是谁**：本机 A 组（闸门武装）是 0 skip，所以它不是那 6 条闸门 spec；**答案要等下一次 CI** —— 新判定步会**直接点名**它（这条未测项是自证的：机制生效后它自己会报出来）。
- **[仍开放 · 2]** `PLAYWRIGHT_JSON_OUTPUT_NAME` + 多 `--reporter` 并存是在 **Windows/git-bash + node** 上验的；reporter 格式属于 reporter 层（与平台无关），但 ubuntu + chromium 上的组合我**没有真跑** —— 判定步会给它的读数（若 JSON 缺失 ⇒ 立即红，不会被静默吞掉）。
- **[仍开放 · 3]** 判定步依赖这个 job 里有 `node`（Playwright 自身需要它 ⇒ 实际必然满足），但这是一条**未显式断言**的前提；若将来 job 换掉 node 安装方式，表现是「JSON 缺失 ⇒ 红」，仍然是**失败而不是假绿**。

## 9.7 纪律回执（t110）

- 写入集合：`.github/workflows/e2e.yml`（取证段）+ 本报告（**都在 inScope**）。`panel/**`（含 `panel/src/**`、`panel/e2e/run-e2e.mjs`）、`crates/**` **零改动**。
- `git diff --stat`（本轮）：`.github/workflows/e2e.yml` = **68 insertions(+), 21 deletions(-)**。
- **未** push / dispatch / rerun / cancel / tag。临时守护进程用**自记 pid**（76352）在 18787 上启停；**活守护进程 pid 79984 与活库未被触碰**（只读健康检查 `{"status":"ok","service":"ruagent"}`；db mtime 收尾仍是 `2026-09-29T04:54:56`，与本轮开始前一致 —— 本轮没有任何写活库动作，Run A/B 全打临时 root）。
- 临时自清：**已删** `%TEMP%\ra-t110-root`（临时数据根）与 `%TEMP%\ra-t110-nc`（负控夹具）；**保留证据**：`ra-t110-runA.log` · `ra-t110-runB.log` · `ra-t110-pwA.json` · `ra-t110-pwB.json` · `ra-t110-nc.txt` · `ra-t110-phase1.log` · 两个 daemon 输出；仓内零临时文件。
- 复核：三份 workflow 仍可解析（`ci.yml` / `e2e.yml` / `release.yml` = OK）；`job.status` 命中 **3**，**全部是注释里的历史说明**，`${{ … job.status … }}` **表达式 0 处**。
