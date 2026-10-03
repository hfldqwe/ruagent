# 收口 t134 的三条 finding：护栏 ④ 报文 / pin 的覆盖边界 / `exitDecision` 坐标（t141）

> 单号 **t141**（repair，成员 `wiki`）· attempt 1 · **2026-10-04 00:5x–01:1x（+08:00）**
> inScope 与实改：`.github/workflows/audit.yml`（**+50/−7**，340 行）· `docs/design/reviews/gen4-audit-gate-recon.md`（**+13/−0**，带日期的修订记录，旧值保留）· 本报告。
> **一行逻辑都没动**：改动全部落在**注释（44 加 / 4 删）**、**echo 报文（3 加 / 3 删）**与 **3 行诊断变量**上；**没有任何 `if`/`fi`/`exit`/`return` 被增删**（§5 的分类证明），五格矩阵与七个格子重跑后行为一致（§4）。
> 纪律：**未** push / dispatch / rerun / cancel；**未**跑 rust 构建；未碰 `crates/**`、`panel/**`、`scripts/**`、`ci.yml`、`e2e.yml`、`release.yml`；未碰 8787。

## 0 三条 finding 的收口方式（一句话）

| finding | 处置 | 一句话依据 |
| --- | --- | --- |
| **F1** 护栏 ④ 报文把 `pending > 0` 误报成「检查表变了」 | **报文同时点名三个量**（`judged=… checks=… pending=…`）并说明两条成因；**断言字节未动** | 工具的算式是 `judged = checks − pending`（`design-audit.mjs:8397`）⇒ 只要 `pending > 0`，旧报文就会说「CHECK LIST changed size」，而检查表没变。新报文让读者**读数字**而不是**猜原因** |
| **F2** pin 的覆盖边界没写明 | **写明边界**（不钉第二个常量）：`65` = **工具判定行数**，**不是 §12 行数**；并给出三个实测数与「未被判定的 12 行 = 60–71」 | 工具**不在运行时读 MASTER.md**：检查表硬编码（`const CHECKS = [`，`:4185`）⇒ 钉一个手工维护的 `77` 会给出一份**没有任何读者**的常量（本仓一再拒绝的「靠人记住」形态）。实测 **77 − 12 = 65** 已经把边界量化 |
| **F3** `exitDecision` 坐标错 | audit.yml 文件头改正为 **`:8105-8111`**（+ 调用点 `:8422`）；t122 报告加**带日期的修订记录**（旧值原样保留） | 定义 `:8105-8111`（三个致命源 `:8107-8109`）、调用 `:8422`、末尾入口块 `:8433-8440`（注释 `:8431-8432`）、文件共 **8440** 行 ⇒ 旧的 `:8431-8442` **既不是函数、还越过了文件末尾** |

## 1 F1：报文不再把读者引到「工具/阈值漂移」

**改前 → 改后（逐字）**

```diff
-          judged=$((pass + fail + unmeas))
+          pending=$(printf '%s\n' "$line" | sed -nE 's/.* ([0-9]+) pending criterion revision.*/\1/p')
+          pending=${pending:-0}
+          judged=$((pass + fail + unmeas))
+          checks=$((judged + pending))
...
-          echo "captures=$captures pass=$pass fail=$fail not_measured=$unmeas judged=$judged audit_exit=$exit_code"
+          echo "captures=$captures pass=$pass fail=$fail not_measured=$unmeas judged=$judged checks=$checks pending=$pending audit_exit=$exit_code"
...
           if [ "$judged" != "65" ]; then                                ← 断言：逐字未动（diff 里是上下文，不是 +/-）
-            echo "::error::the audit judged $judged rows, not 65: the CHECK LIST changed size (tool or threshold drift), so this reading is not comparable with the pinned one"
+            echo "::error::this reading is not comparable with the pinned one: judged=$judged (pinned 65), checks=$checks, pending=$pending. judged = checks - pending (design-audit.mjs:8397), so the cause is EITHER a check-list size change (checks != 65) OR a row that moved into 'pending criterion revision' (pending != 0) -- read the two numbers, do not assume the tool drifted."
```

**为什么选「点名两个量」而不是「直接钉 `checks.length`」**：
1. **钉 `checks.length` 会把这道护栏变弱**：行进入 `pending`（判据修订）时 `checks` 仍是 65 ⇒ 不再红；而那一刻的读数**确实不可比**（t134 的判读，我同意）⇒ 应当红。我要的是「红得对 + 报文说得对」，不是「少红」。
2. **`pending` 这条尾巴必须被读出来才能点名**：工具的输出格式是 `… / K pending criterion revision`（`:8398`），只有 `pending > 0` 才出现 ⇒ 正则做成**可选**，缺省 0。
3. **代价最小**：新增 3 行诊断变量，断言与所有控制流逐字不变（§5 有证明）。

**实测（合成日志，逐个给退出码）**：

| 格子 | 日志里的汇总行 | 期望 | 实得 | `::error::` 首行 |
| --- | --- | --- | --- | --- |
| 4（改前就存在） | `checks 43 pass / 11 fail / 10 not measured` | 1 | **1** | `judged=64 (pinned 65), checks=64, pending=0. …` |
| **6（新增，F1 的场景）** | `checks 43 pass / 11 fail / 10 not measured / 1 pending criterion revision` | 1 | **1** | `judged=64 (pinned 65), checks=65, pending=1. …` |
| **7（新增）** | `checks 45 pass / 11 fail / 10 not measured` | 1 | **1** | `judged=66 (pinned 65), checks=66, pending=0. …` |

⇒ 旧报文在**格子 6** 会说「CHECK LIST changed size (tool or threshold drift)」（检查表**没变**，`checks=65`）；新报文把 `checks=65 / pending=1` 摆在读者眼前。格子 7 才是「检查表真变了」，两者现在**可区分**。

## 2 F2：pin 保护的是什么、不保护什么（三个数都是我这次自己数的）

| 读数 | 值 | 我怎么数的 |
| --- | --- | --- |
| MASTER.md §12 的行数 | **77**（1..77，**连续**） | `docs/design/MASTER.md` `:784` `## 12 验收阈值表` 起，统计首列是数字的表行 = 77，min 1 / max 77，且 `1..77` 连续 |
| 工具的检查表长度 | **65** | `panel/tools/design-audit.mjs` `:4185` `const CHECKS = [` 到 `:5629` `];` 之间，`n: <§12 行号>` 出现 **65** 次（65 个**互不重复**的 §12 行号，min 1 / max 77） |
| **§12 里没有任何检查的行** | **60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71**（12 行） | `1..77` 减去工具判定的 65 个行号 ⇒ **77 − 12 = 65**（与 pin 的常量**精确吻合**） |

⇒ **F2 的边界不是假设，而是已经在发生**：今天就有 **12 条 §12 行**不产生任何判定，而 `judged == 65` 照样绿。**新增一条没有实现的 §12 行，这道 pin 不会红**——它回答的是「**工具要跑的检查集变了没有**」。

**改前 → 改后（逐字，两处）**
- 文件头「何时可以改阻塞」列表的**最后一条**（新增）：
```
#   * the PIN'S COVERAGE BOUNDARY is understood (t141 F2): the pinned `65` is the TOOL'S
#     JUDGED ROW COUNT (`judged = checks - pending`, design-audit.mjs:8397), NOT §12's
#     row count -- measured 2026-10-04: MASTER.md §12 has **77** numbered rows (1..77,
#     contiguous) and the tool's hardcoded list (`const CHECKS = [`, :4185) has **65**
#     entries, one per judged §12 row: 77 - 12 = 65, and the 12 §12 rows with NO check are
#     rows **60-71**. The tool never reads MASTER.md at run time, so a §12 row added
#     WITHOUT an implementation leaves this pin green -- as those 12 already do. …
```
- 护栏 ④ 的注释（新增一段）与 step summary 那一行：
```
-            echo "- judged rows: **$judged** (pass $pass / fail $fail / not measured $unmeas); the pinned constant is 65"
+            echo "- judged rows: **$judged** (pass $pass / fail $fail / not measured $unmeas); the pinned constant is **65** -- that is the TOOL's judged row count (`checks=$checks`, of which `pending=$pending`), NOT §12's row count (77 today; the tool's check list is hardcoded, design-audit.mjs:4185)"
```
（step summary 的实际渲染见下，取自格子 1 的 `$GITHUB_STEP_SUMMARY`）
```
- judged rows: **65** (pass 44 / fail 11 / not measured 10); the pinned constant is **65** -- that is the TOOL's judged row count (`checks=65`, of which `pending=0`), NOT §12's row count (77 today; the tool's check list is hardcoded, design-audit.mjs:4185)
```

**为什么选「写明边界」而不是「第二个常量 77」**：`77` 没有任何运行时来源 —— 工具不读 MASTER.md，护栏也不读；钉它就是加一个**只有人记得维护、且没有任何断言会因它而红**的数字。本仓反复拒绝的正是这种形态（「靠人记住」）。写明边界 + 给出 `77/65/未被判定的 60-71` 三个数，读者能自己判断这条 pin 的射程。**真要覆盖 §12 的行数**，正确的下一步是让工具把「§12 行数 vs 检查数」**自己输出**（今天没有这个开关），那属于 `panel/tools/**`，不在本单。

## 3 F3：`exitDecision` 的坐标（含一处连 finding 自己都写错的范围）

| 事实 | 坐标（回读自 `panel/tools/design-audit.mjs`，共 **8440** 行） |
| --- | --- |
| `exitDecision` **定义** | **`:8105-8111`**（`export function exitDecision({ check, fails, contractWarnings = [], unmeasured = [], allowNotMeasured = false }) {` 起） |
| 三个致命源 | `:8107-8109`（`fails > 0` ∨ `contractWarnings.length > 0` ∨ `unmeasured.length > 0 && !allowNotMeasured`） |
| **调用点** | **`:8422`**（`return exitDecision({`） |
| 文件末尾的 `if (process.argv[1] …)` **入口块** | **`:8433-8440`**（其两行注释在 `:8431-8432`） |

**改前 → 改后（audit.yml 文件头，逐字）**
```diff
-# baseline/known-failure mechanism: `exitDecision` (design-audit.mjs:8431-8442) has
-# exactly three fatal sources -- fails > 0, contractWarnings.length > 0, or
-# unmeasured > 0 without `--allow-not-measured`. Wiring this in as a REQUIRED gate today
+# baseline/known-failure mechanism: `exitDecision` has exactly three fatal sources --
+# fails > 0, contractWarnings.length > 0, or unmeasured > 0 without
+# `--allow-not-measured` (design-audit.mjs:8107-8109). COORDINATE, corrected
+# 2026-10-04 (t141): the function is DEFINED at :8105-8111 and CALLED at :8422.
+# Earlier revisions of this header cited ":8431-8442", which is not the function at
+# all -- it is the `if (process.argv[1] ...)` entry block at the END of the file, and
+# the range even overshot it (:8433-8440, with its two comment lines at :8431-8432;
+# the file is 8440 lines). The declaration itself was and is true; only the
+# coordinate was wrong. Wiring this in as a REQUIRED gate today
```
（`8440` 行这个数我回读过：`:8440` 是 `}`，文件到此结束 ⇒ 旧的 `:8442` **不存在**。）

**t122 报告的修订记录（旧值保留）**：`docs/design/reviews/gen4-audit-gate-recon.md` 顶部新增一节 `## 修订记录（2026-10-04，t141）`，含两列表格（旧值 / 更正后）、三处出现位置、以及一句「声明本身是真的，只有坐标错」。三处旧值**原样留在正文**（现为 `:20` / `:76` / `:196`；插入说明前的行号是 `:9` / `:65` / `:185` —— 连这个我也会写清，因为**行号本身会漂移**）。

**顺带核到的一处**：`gen3-panel-tooling-repair.md:21` 把入口块写作 `:8431-8440`（并标明「入口」）⇒ 与我的实测（`:8431-8432` 注释 + `:8433-8440` 块）**一致**，不属于同一类错误，未动。

## 4 护栏行为未变：五格矩阵 + 两格新证据

方法（t134 验证报告 §1）：`python -c "yaml.safe_load(...)"` 把 `jobs.audit.steps[*].run` **逐字**导出成脚本（本单导出 **9** 个块，`bash -n` **9/9 通过**），用**私有** `RUNNER_TEMP` 与 `GITHUB_STEP_SUMMARY` 跑护栏那一步。

| # | 日志 | 期望 | **实得** |
| --- | --- | --- | --- |
| 1 | 现场读数 `24 captures … 44 pass / 11 fail / 10 not measured` + 两份名单 + `audit.exit=1` | 0 | **0** |
| 2 | not_measured 名单少一个（去掉 `#77`） | 1 | **1** |
| 3 | 没有汇总行（`garbage`） | 1 | **1** |
| 4 | `checks 43 pass …`（judged 64） | 1 | **1** |
| 5 | `0 captures …` | 1 | **1** |
| 6 | **新增**：`… / 1 pending criterion revision`（F1 场景） | 1 | **1**（报文点名 `checks=65 pending=1`） |
| 7 | **新增**：`checks 45 pass …`（检查表真变了） | 1 | **1**（报文点名 `checks=66 pending=0`） |

⇒ **五格 = `0/1/1/1/1`，与改前完全一致**；另两格证明 F1 的两种成因现在**可区分**。**断言那一行在 diff 里是上下文**（`if [ "$judged" != "65" ]; then` 无 `+`/`-` 前缀，§5）。

## 5 「只动注释/文案」的机械证明（`git diff -U0` 逐行分类）

| 类别 | 新增 | 删除 |
| --- | --- | --- |
| 注释（`# …`） | **44** | **4** |
| `echo` 报文 | **3** | **3** |
| **新增诊断变量**（`pending=$(…)`、`pending=${pending:-0}`、`checks=$((judged + pending))`） | **3** | 0 |
| 控制流（`if`/`fi`/`exit`/`return`/`for`/`while`/`case`） | **0** | **0** |

```
=== no control-flow line added or removed anywhere ===
  (empty above = no if/fi/exit/return line added or removed)
=== the assertion line and its neighbours in the DIFF (must appear as CONTEXT, never as +/-) ===
             if [ "$captures" -lt 1 ]; then
             if [ "$judged" != "65" ]; then
```
**披露**：那 3 行诊断变量是本单**唯一**的非注释/非报文新增（F1 的验收明确允许「报文同时点名两个量」，而点名要求把它读出来）。它们不参与任何判断：`judged` 的算式与断言都没变。

## 6 门禁读数

| 门 | 命令 | 读数 |
| --- | --- | --- |
| refs + 表达式形状与上下文可用性 + YAML 可解析 | `bash .github/workflows/scripts/check-workflow-refs.sh` | **exit 0**；`workflow path references checked: 22, not tracked/missing: 0`；`.github/workflows/audit.yml: parses as YAML (PyYAML)`（含 ci/e2e/release 共四份） |
| 我的文件仍可解析 | `python -c "import yaml; yaml.safe_load(open('…audit.yml',encoding='utf-8'))"` | 通过；`steps = 14`，`timeout-minutes = 45`（未被本单动过） |
| 抽出的 9 个 `run` 块的 bash 语法 | `bash -n`（WSL 路径） | **9/9 exit 0** |

## 7 未覆盖什么

1. **没有在 CI 上跑过**（不许 push/dispatch）⇒ 本单的读数全部来自本机合成日志与导出脚本；真实运行的行为仍待 `audit.yml` 的第一次 CI 运行。
2. **`pending` 的尾巴是我按 `design-audit.mjs:8398` 的格式串读出来的**（`… / K pending criterion revision`），我**没有**构造一次真的 `pending > 0` 运行（要造它需要改工具或改 MASTER 的判据状态）⇒ 空格子 6 用的是一份**按该格式写的合成汇总行**。风险面：若工具将来改这个后缀的措辞，护栏会退回旧的误导报文（但**不会**失去「红」的能力 —— 断言只看 `judged`）。
3. **`77` 没有变成断言**（这是 F2 的选择，不是遗漏）：护栏仍**不**会因为「§12 多了一行」而红；本单只是把这个射程写清。要真正覆盖 §12 的**行数**，需要工具侧支持（`panel/tools/**`，不在本单）。
4. **未动工具的源码或 HELP 文案**：`design-audit.mjs` 的注释里可能还有别的陈旧坐标，我没有全量核对（只核了本单三处 + 顺手核到 `gen3-panel-tooling-repair.md:21`）。
5. **`gen4-audit-gate-verify.md`（t134 的报告）未动**：它是本单三条 finding 的来源，且属别人的在途交付（`??` 未跟踪）。

## 8 纪律回执

- **写入集合 = 三个文件**：`.github/workflows/audit.yml`（+50/−7）· `docs/design/reviews/gen4-audit-gate-recon.md`（+13/−0）· 本报告。`git status --porcelain -- .github/workflows docs/design/reviews` 里其余条目（`gen4-audit-gate-verify.md`、`gen4-version-points-verify.md`）**都是队友的在途交付**，未动。
- **未** push / dispatch / rerun / cancel / 建 tag；**未**跑 rust 构建；未碰 `crates/**`、`panel/**`、`scripts/**`、`ci.yml`、`e2e.yml`、`release.yml`；**未**碰 8787 的活守护进程（本单不需要任何 daemon，全程 `8787 still alive: True`）。
- 临时项：`%TEMP%\t141`（导出脚本 `extract.py`、9 个 `run` 块、两个 wrapper、合成日志、两份 step summary）—— 已按**具体路径**删除（`exists now: False`）；我起的进程 **0** 个残留（按命令行匹配我自己的临时路径）。
- **不是我但名字相近、按纪律未删**：`%TEMP%\t141z-apply.py` / `t141z.json`（**2026-09-25**，上一代同名任务的产物，与更早见到的 `t129z-*` 同一族）。
