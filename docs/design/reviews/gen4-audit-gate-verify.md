# 独立验证：`audit.yml`（t131）能不能加载 / 四道护栏真的能红吗 / pin 自洽 + 首次 CI 读数

> 单号 **t134**（verification）· 成员 `recall` · attempt **1** · **2026-10-04**（时间以本次命令的输出为准；CI 时间戳是 UTC）
> inScope：**本报告**。`.github/**` · `panel/tools/**` · `crates/**` · `scripts/**` **只读** —— 发现要改的地方一律进 §6 的 finding，本单**未改任何一行代码**。
> 被验对象：`.github/workflows/audit.yml`（297 行，`ec24b61` 已推 main）+ 作者报告 `docs/design/reviews/gen4-audit-gate-impl.md` + 其输入 `gen4-audit-gate-recon.md`（t122）。
> 方法：**不抄作者的 harness**。我把 workflow 的 `run` 体用 YAML 解析器**原样抽出**（`yaml.safe_load` → 写文件，零手抄），再用**我自己写的**合成日志驱动它；`${{ }}` 的上下文可用性是我按 GitHub 的 per-context 规则逐条判的（不是复述）。
> 纪律：**未** push / dispatch / rerun / cancel / 建 tag；**未**触碰活守护进程 8787（pid 14944）；本机只跑了**死端口**形式（`--base-url=http://127.0.0.1:1`，不驱动浏览器）；`--out` 全部落在 `%TEMP%\t134`（仓库外）。**未**跑 rust 构建。

## 0 结论（一句话 + 计数）

**能加载 ✓ · 四道护栏都能红 ✓（我另加 3 格）· pin 的来源可追且自洽 ✓（CI 上逐项再现）· 首次 CI 读数取到 ✓ —— 红在护栏 ①，不是作者预期的 ②。** 5 格矩阵我在**自己的日志/自己的数字**上重跑为 **8 格全中**；CI run 是 **`failure`**，红的**只有**护栏步，而它的 `judged==65` 与未测名单**与 pin 完全一致**（⇒ **pin 不需要改**）；另发现 **5 条 finding**（F5 是「门」级、其余为注释/口径级），**没有**一条是「不该绿而绿」。

| 验的东西 | 结论 | 证据在哪 |
| --- | --- | --- |
| 五格能红矩阵（一致 / 名单漂移 / 无汇总行 / `judged≠65` / `captures=0`） | **0/1/1/1/1 ✓**（我另加 3 格：无日志文件 / `captures=1` / 名单消失） | §1 |
| 可加载性（三类「YAML 合法但 GitHub 拒」+ refs 守卫） | **全过**：refs **22 checked / 0 missing · exit 0**；9 处 `${{ }}` **0 处上下文不可用**；未加引号的 `: ` **0 处**；GitHub 自己收下了这份文件并**跑完了 21m47s** | §2 |
| 护栏 ③ 的两半（`rc==2` **且** 点名句） | **两半都在，且两半都能各自点火**（两次隔离变异）；**CI 上也成立**（`negative control exit=2 (want 2)`，步 11 success） | §3、§5.1 |
| pin 来源与自洽（`judged==65` · `EXPECTED_NOT_MEASURED`） | **可追到 t122 原始读数**；算术自洽；**未被放宽**；**CI 上独立再现**（`checks=65`、未测 ids 与 pin 逐项相同）⇒ **无需更新 pin** | §4、§5.2 |
| 首次 CI 读数（run `37137554330`） | **`failure`，红在护栏 ①**：工具输出在 **≈64 KiB 管道边界**被截断，汇总行没进 `audit.log`（工具其实跑完了：`audit exit=1` + 完整 65 行判定表）。**advisory 形状在 CI 上被亲证**（审计步 `success` 而日志里是 `exit 1`） | §5 |
| findings | **5 条**：**F5（high，对门不对产品）** 载体被截断 + F1 / F2 / F3 / F6（low） | §6 |


## 1 五格能红矩阵 —— 我自己写的日志，我自己的数字

方法：`python -c yaml.safe_load` 把 `jobs.audit.steps[*].run` **逐字**导出为 `guard.sh`（4913 bytes）与 `negctl.sh`（681 bytes），再用 `RUNNER_TEMP`/`GITHUB_STEP_SUMMARY` 指向私有目录运行。**日志由我构造**，且第 1 格**故意用与作者不同的拆分**（`47 pass / 8 fail / 10 not measured`，作者是 44/11/10）—— 如果 pin 钉的是**拆分**而不是**总和**，这一格会红；它的 green 就是「pin 钉的是总和」的证明。余数用**乱序 + 多余空格**的 id 列表（`#77 #28 #25   #33 …`）压归一化。

| 格 | 我构造的日志（要点） | 期望 | 实测 exit | `::error::` 原文 |
| --- | --- | --- | --- | --- |
| ① 一致 | `[audit] 26 captures in 611.2s · checks 47 pass / 8 fail / 10 not measured` + 乱序 `not measured:` 十项 + `failing rows: 3, 9, 21` | 绿 | **0** | （无）—— 且 `captures=26 pass=47 fail=8 not_measured=10 judged=65` |
| ② 名单漂移 | 同上，但 `#77` → **`#78`** | 红 | **1** | `::error::not_measured set moved: expected '#25 28 31 33 46 47 49 50 54 77', got '#25 28 31 33 46 47 49 50 54 78'` |
| ③ 无汇总行 | 只有 `[audit] starting` / `[audit] route home ok` / `noise` | 红 | **1** | `::error::no '[audit] N captures ... checks P pass / F fail / U not measured' line in the log` |
| ④ `judged≠65` | `checks 46 pass / 8 fail / 12 not measured`（=66），pin 名单**完好** | 红 | **1** | `::error::the audit judged 66 rows, not 65: the CHECK LIST changed size (tool or threshold drift), so this reading is not comparable with the pinned one` |
| ⑤ `captures=0` | `[audit] 0 captures in 0.9s · checks 47 pass / 8 fail / 10 not measured` + pin 名单 | 红 | **1** | `::error::the audit captured 0 pages -- nothing was measured, so nothing may pass` |
| ⑥（我加的）**日志文件不存在** | 不写日志（= 审计步没跑） | 红 | **1** | `::error::no audit log at <RUNNER_TEMP>/audit.log -- the audit step did not run` |
| ⑦（我加的）**`captures=1` 边界** | `[audit] 1 captures in 3.0s …` + pin 名单 | 绿 | **0** | （无）⇒ **`captures` 确实不是常数**，门槛就是 `≥1`；把 24/26 钉死会误红，而这一格证明它没被钉死 |
| ⑧（我加的）**名单消失**（有行变可测了） | 汇总行在，`not measured:` 行**整行消失** | 红 | **1** | `::error::not_measured set moved: expected '#25 28 31 33 46 47 49 50 54 77', got '#'`（`#` 是报文里的字面前缀，`got` 为空） |

**8 格全部与期望一致（8/8）**，且每种失败都打出具名 `::error::`；**没有出现「不该绿而绿」**。
补充：第 ①/⑦ 格还写出了完整的 step summary（我自己日志的数字），说明 summary 分支（`$GITHUB_STEP_SUMMARY`）也能正常渲染。

## 2 可加载性：三类「YAML 合法但 GitHub 拒」+ refs 守卫

| 检查 | 命令 / 方法 | 读数 |
| --- | --- | --- |
| **refs 守卫**（契约的 Verify 命令） | `bash .github/workflows/scripts/check-workflow-refs.sh`（git-bash 5.2.37） | **exit 0**，**266s**（本机 git-bash；CI 上更快）：`workflow path references checked: 22, not tracked/missing: 0` · **四份** workflow 全部 `parses as YAML (PyYAML)`（含 `audit.yml`）· `every executed path a workflow references is tracked` |
| **类 1：非法 `${{ }}`** | 抽出全部表达式 + 括号配平/charset 检查 | 9 处表达式，**括号全配平**，charset 合法；**0 处语法问题** |
| **类 2：未加引号的 `: `** | 逐行扫描 plain scalar（跳过已引号/块标量） | **0 处**（作者报告 §6 说守卫当时抓到过 `:155/:265` 两处，现字节上已无；PyYAML 能解析也印证） |
| **类 3：表达式用了该上下文不可用的变量** | 按 GitHub 的 per-context 规则逐条判（结果见下表） | **0 处不可用**。特别地：**job 级根本没有 `env:`**，所以「`runner.*` 出现在 job 级 env」这一形状不存在；`runner.temp` 只出现在**步骤级 `with.path`**（那里 `runner` 可用） |
| **GitHub 自己的判词**（最强） | 该文件已被成功派发并正在跑（run `37137554330`，job `111245060897`） | **平台收下了它**：非法 workflow 无法被派发/dispatch，也无法进入 steps。前 11 步已 `success` |
| **分支保护**（文件头 `:19-22` 的声明） | `gh api repos/{owner}/{repo}/branches/main/protection` | **HTTP 404 `Branch not protected`** ⇒ 文件头「不挡任何 push」**成立** |

**9 处 `${{ }}` 逐条**（`步骤序号` 取自 `jobs.audit.steps`）：

| 位置 | 表达式 | 上下文是否可用 |
| --- | --- | --- |
| `concurrency.group` | `${{ github.ref }}` | ✓ 可用（concurrency 允许 `github/inputs/vars`） |
| step 12 `path`（×4，upload-artifact 的 `with.path`） | `${{ runner.temp }}` | ✓ 可用（步骤级 `with` 允许 `runner`） |
| step 13 `run` | `${{ steps.audit.outcome }}` | ✓ 可用（引用的是**前面的** `id: audit` 那一步，存在） |
| step 13 `run` | `${{ github.job }}` / `${{ github.ref_name }}` / `${{ github.event_name }}` | ✓ 可用 |

## 3 护栏 ③ 的两半：都在，且各自都能点火

先看**静态**（抽出的 `negctl.sh` 逐字）：`if [ "$rc" != "2" ]; then … exit 1; fi`（第一半）**且** `echo "$out" | grep -q "daemon not reachable" || { … exit 1; }`（第二半）—— 两半都在，`set -euo pipefail` 下都会真的 `exit 1`。

再**实跑**（三次，都是 workflow 自己的脚本）：

| 场景 | 怎么造 | 脚本 exit | `::error::` 原文 |
| --- | --- | --- | --- |
| **真情形**（保留端口 1） | 直接跑 workflow 的 node 命令：`node tools/design-audit.mjs --check --routes=home --modes=dark --no-shots --base-url=http://127.0.0.1:1 --out=$TEMP/t134/rt/negctl` | **0**（两半都满足） | （无）。工具读数：**exit 2 / 2s**，报文含 `daemon not reachable at http://127.0.0.1:1 (no panel to audit). Start it with: cargo run -p ruagent-cli -- serve`；并打印 `--out resolves to …(outside the repo)` **且真的写在仓库外** |
| **变异 1**（工具 exit 1 且点名句） | 用 PATH 前置的替身 `node` 让工具 rc=1、输出含点名句 | **1** | `::error::the audit did NOT fail on a dead port (exit 1): this gate cannot redden` ⇒ **第一半点火** |
| **变异 2**（工具 exit 2 但**不**点名句） | 替身 `node` 输出 `audit: something went wrong` + `at auditRoute (design-audit.mjs:3857)`，rc=2 | **1** | `::error::the dead-port failure did not NAME the missing precondition` ⇒ **第二半点火** |

> 变异只用「把工具换成一个按指令返回 rc/文本的替身」这一招 —— 被测对象是**护栏的两条断言**，不是工具本身；这样无需浏览器、无需真 daemon，也不碰 8787。**披露**：替身是我写的 shell 脚本，不是 `design-audit.mjs`。
> 顺带印证了文件头与作者报告的一条机制：`exitDecision` 只会返回 **0 或 1**（`design-audit.mjs:8110`），所以死端口的 **exit 2 来自 catch**（`:8436-8439` 的 `process.exit(2)`）—— 与作者登记的「TCP 连上但不服务」抖动（走 `auditRoute` 超时后由同一个 catch 换成 exit 2、报文里没有点名句）**机制吻合**：那种情形会让**第二半**点火 ⇒ 这道护栏把那个抖动当红处理，方向是安全的。

## 4 pin 的来源与自洽（并说清它**不**覆盖什么）

### 4.1 来源可追（不是手抄的）
- `audit.yml:238` 的 `EXPECTED_NOT_MEASURED="25 28 31 33 46 47 49 50 54 77"` 与 `:221` 的 `judged != 65`，**逐条对上了 t122 侦察报告里的原始读数**：`gen4-audit-gate-recon.md:38` = `[audit] 24 captures in 620.7s · checks 44 pass / 11 fail / 10 not measured`，`:40` = `[audit] not measured: 10 row(s) #25, #28, #31, #33, #46, #47, #49, #50, #54, #77`。
- **算术自洽**：`44 + 11 + 10 = 65` ✓；pin 的 id **正好 10 个**，与该读数的 `10 not measured` ✓；`audit.yml:10` 文件头引的也是同一次读数 ✓（三处同源）。
- **未被放宽**：全文件扫描，**被执行的命令行里 `--allow-not-measured` 命中 0**（只有 2 行注释提到它）。护栏 ② 是**逐项归一化后严格相等**（`sort -n` 使顺序无关 —— 我用乱序日志独立证明过，§1 格①）。
- **id 空间自洽**：`not measured` 打的是 **§12 行号**（`design-audit.mjs:8420` 用 `unmeasured.map(r => '#'+r.n)`），而 `docs/design/MASTER.md` §12（`:784` 起、到 `:2052` 前）**正好 77 行编号、1..77 无缺号**。⇒ 于是 **`#77 > judged=65` 不是矛盾**：65 是**判定行数**，id 是**§12 行号**，两个不同的基数。这一点文件里**没有写**（见 F3/F2）。

### 4.2 `judged` 这个量的精确定义（我回读了工具的算式）
`design-audit.mjs:8397`：`const passed = checks.length - fails.length - unmeasured.length - pending.length;`
⇒ `judged = passed + fail + unmeasured = **checks.length − pending.length**`。
所以 pin 钉住的是「**工具自己的判定行数**（检查表大小减去 pending）」。在**本机两次真跑**（全量 `44+11+10`、子集 `29+6+30`）都是 65 —— 两次拆分不同、总和相同，这正是 pin 想钉的东西 ✓。

### 4.3 它**不**覆盖什么（F2，长话短说）
工具**不在运行时读 MASTER.md**（全文件只有注释提到它，没有 `readFileSync(...MASTER...)`；检查表是硬编码的 `const CHECKS = [`（`:4185`，17 个 spec 组经展开产出 65 条判定行）。
⇒ **§12（77 行）与 65 条判定行之间只有散文注释相连**。后果：**新增一条 §12 行而它不产生任何判定行时，`judged==65` 仍然绿** —— pin 发现不了这一类「文档涨了、仪器没涨」。`77 − 65 = 12` 这个差值说明今天至少有 12 行没有判定行；**我没能逐行指认是哪 12 行**（那需要执行工具；`CHECKS` 是 spec 组、不是逐行表），所以我把「差 12 行」记为**算术推断**而不是实测。

## 5 首次 CI 读数（run `37137554330`）—— 取到了，而且**推翻**了作者的预期

> run 创建 `2026-10-03T16:37:50Z` · 完成 `16:58:37Z` · **墙钟 21m47s**（`timeout-minutes: 45` ⇒ 余量 2.07×，作者「624.6×3≈31min + 前置 6min ≈ 37min，取 45」的推导被这次实测**验证为保守且安全**）。
> 结论：**run = `failure`，但红的不是审计，是护栏 ①；而 `judged==65` 与未测名单在 CI 上与 pin 完全一致**。

### 5.1 逐步结论（advisory 形状在 CI 上真的成立）
| 步 | 结论 |
| --- | --- |
| 1–11 | 全部 `success`（含 **11. Negative control ⇒ 护栏 ③ 在 CI 上两半都成立**：日志有 `negative control exit=2 (want 2)`） |
| **12. Design audit（advisory）** | **`success`** —— 但它的日志里是 **`audit exit=1 (advisory: this step's failure does not fail the job)`** + `##[error]Process completed with exit code 1.` ⇒ **`continue-on-error` 的作用被 CI 亲证**：审计「结果难看」记成读数，**不**把 job 弄红 |
| **13. Guardrails** | **`failure`** ⇐ **job 唯一红的一步**。红的是 **护栏 ①**：`##[error]no '[audit] N captures ... checks P pass / F fail / U not measured' line in the log` ⇒ **「测量结果难看」与「测量本身坏了」在 CI 上真的被分开了**（这一步红 = 接线问题） |
| 14 / 15 | `success` ⇒ 工件照常上传（`if: always()` 生效，失败那次的工件留下来了 —— 我就是用它取到下面这些读数的） |

### 5.2 用**上传的工件**（不是用 GitHub 的日志）取到的真实读数
`gh run download 37137554330 -n design-audit` ⇒ `audit.log`（**67,938 B / 351 行**）· `audit/metrics.json`（**2,513,392 B**）· `audit/report.md`（99,573 B）· 24 张 PNG · `daemon.log`。

| 量 | CI 读数（`metrics.json` + `audit.log` 的表交叉核对） | 与 pin 的关系 |
| --- | --- | --- |
| `checks\|length` | **65** | = pin 的 `judged == 65` ⇒ **CI 独立再现** ✓ |
| `verdict` 分布 | **pass 45 / fail 10 / not_measured 10** | 和 = **65** ✓（本机 2026-10-02 是 44/11/10：**一条行从 fail 翻成 pass，总和不变** ⇒ pin 钉总和正是为了吸收这类翻转） |
| `pending` | **0** ⇒ `judged = checks.length − pending = 65` | 与 §4.2 的算式一致 ✓ |
| **`not measured` 的 id 集合** | **`25 28 31 33 46 47 49 50 54 77`** | **与 `EXPECTED_NOT_MEASURED` 逐项相同**（我在 `metrics.json` 与 `audit.log` 的表里各算了一遍） ⇒ **护栏 ② 在 CI 上会是绿的** |
| `thresholdRows\|length` | **77** | = `MASTER.md` §12 的编号行数（§4.1）⇒ **77 就在机器可读产物里**（见 F2 的修法） |
| `captures\|length` | **24** | ≥1 ✓（护栏 ④ 只要求非零，CI 恰好也是 24） |
| CI 的 `failing row ids` | `4, 6, 7, 12, 13, 14, 18, 20, 38, 75`（10 条） | 这是**今天还没被钉住**的那份清单（文件头 `:24-33` 自己说了要加第二常量再谈阻塞） |
| `meta` | `baseUrl=http://127.0.0.1:8898` · `dist.buildId=h46VyK4g` · `dist.staleSrc=false` · `generatedAt=2026-10-03T16:58:33.891Z` | 夹具形状与 t122 侦察（§7 第 3 条「fixture 同形」）一致 ⇒ 可比 ✓ |

**⇒ 对 captain 那条问题的直接回答：「pin 是否要按证据更新？」——`judged==65` 与未测名单都不需要改**（CI 与 pin 逐项一致）。作者预期的「② 很可能在 CI 上红」**被证据推翻**；红的是 **①**，原因见 §5.3。

### 5.3 为什么 ① 会红：**读数的载体被截断了**（证据链，根因未定案）
1. **护栏 ① 的判词是真的**：CI 的 `audit.log` 里，护栏的正则（`^\[audit\] N captures in N.Ns · checks …`）**匹配 0 行** —— 我用 python 以 UTF-8 重跑同一条正则复核（PowerShell 的默认解码不足以作数）⇒ 文件里**确实没有**那行，不是 grep 口径问题。
2. **但工具确实跑完了**：`audit exit=1` ⇒ 它的 `exitDecision`（`:8105-8111`）返回 1 ⇒ `main()` 走到了 `:8422` 的调用点，而**汇总行是在那之前**用 `log(...)` 打的（`:8398-8420`，`log` = `console.error`，`:503-505`）。而且工具**跑满了全程**：日志里有 50 行 `[audit]` 进度（`dark home: 23745ms …` … `palette contract …`）和**完整的 65 行判定表**（表里 65 行我数过）。
3. **文件是在半行处断的**：67,938 B，**不以换行结尾**，最后一行断在报告文字中间（`**⚠️ 这是惯例阈值**：**CIE76 的 JND ≈ 2.3 `）。67,938 B ≈ **64 KiB（管道容量）+ 2,402 B** ⇒ 截断点就是管道缓冲的边界形状；被丢掉的正是**队列尾部的汇总块**（`:8398-8420`，含 `captures=` 与 `not measured:` 两行）。
4. **机制候选与我的证伪尝试**：最像的是「`process.exit(code)` 丢掉仍在队列里的 stdout/stderr 写」（工具的收尾正是 `.then((code) => process.exit(code))`，`:8435`）。我做了**本机探针**：向管道写 300 KB 后再 `process.exit(1)`，**尾部没有丢**（300 KB 全到，摘要行在）——**所以这个候选在我这台机器（Windows + Node v24.13.0）上不成立**。但 CI 是 **ubuntu + Node 22**，而 `process.exit()` 与异步管道写的组合行为是**平台/版本相关**的 ⇒ **本机探针不能否证 CI 上的机制**。
5. **因此我把它记成「截断已证明、根因未定案」**：定案需要一次 **Linux 上的受控复现**（例：`cd panel && node tools/design-audit.mjs --check --routes=home --modes=dark | tee /tmp/x.log`，看摘要行是否也停在缓冲边界）。本单**不许改代码**、也不该为复现跑一次 10.4 分钟的真跑，所以我把它交给 §6 的 F5。

### 5.4 这次 CI 红该被读成什么
- **不是**「audit.yml 是坏的」：它的四道护栏、advisory 形状、工件上传、超时预算、负控——**都按设计工作了**（负控在 CI 上 exit 2 + 点名；护栏 ① 精确点名了缺失的行；工件留下了全量证据）。
- **是**「护栏的**输入载体**不可靠」：唯一给护栏供数的通道是工具打到标准输出/错误流上的那几行，而这条通道在 CI 上被截断 ⇒ **护栏 ① 的判词完全正确，但它今天红在「读数在路上丢了」而不是「审计判了 0 行」** —— 这两种原因该被分开（见 F5 的修法）。
- **对产品本身没有新增坏消息**：11 条 fail（本机）→ 10 条 fail（CI），且都在审计步（advisory）里如实记录；`dist.staleSrc=false`。


## 6 Findings（本单未改任何代码；都属「要改就得改 `.github/**`」⇒ 一律登记）

| id | 严重度 | 问题 | 需要怎么修 |
| --- | --- | --- | --- |
| **F5** | **high**（对门，不对产品） | **护栏的唯一输入载体是工具的标准输出/错误流，而它在 CI 上被截断**：CI run `37137554330` 的 `audit.log` 在 **67,938 B（≈64 KiB 管道边界）** 处**断在半行**，工具的汇总块（`design-audit.mjs:8398-8420`，即护栏 ① 要求的 `[audit] N captures …` 与 ② 要求的 `not measured:` 行）**没有进入文件** ⇒ 护栏 ① 红。**工具本身跑完了**（`audit exit=1`，走到 `:8422`；日志里有 50 行进度 + 完整 65 行判定表）。本机探针（Windows/Node v24.13.0，300 KB + `process.exit(1)`）**未能复现**截断 ⇒ **截断已证明、根因未定案**（CI 是 ubuntu + Node 22；`.then((code) => process.exit(code))`（`:8435`）是最像的候选）。 | **首选（去掉对流的依赖，同一课在 t110 上过一次）**：护栏改成读**机器可读载体** `$RUNNER_TEMP/audit/metrics.json`（工具**已经**产出、且已被上传；我验过它含护栏需要的全部量 —— 见下）。例：`checks=$(jq '.checks\|length' …)`、`nm=$(jq -c '[.checks[]\|select(.verdict=="not_measured")\|.n]\|sort' …)`、`rows=$(jq '.thresholdRows\|length' …)`；再把 `audit.log` 只当**给人看的**附件。**次选/并行**：按 §5.3 第 5 点在 ubuntu 上做一次受控复现（`--routes=home` + `tee`），定案后修工具的收尾（`process.exit` → `process.exitCode` 或退出前显式 flush）。 |
| **F1** | low | **护栏 ④ 的报文会把 pending 误报成「检查表变了」**：`judged = checks.length − pending.length`（`design-audit.mjs:8397` + CI 实测 `pending=0`），所以只要有一次 `pending > 0`，护栏就以 `the audit judged N rows, not 65: the CHECK LIST changed size (tool or threshold drift)` 红 —— 而检查表**没有**变。红是对的（那次读数确实不可比），但报文会把读者引到工具/阈值上去。 | `audit.yml:221-224`：报文同时点名两个量（`checks=… pending=… judged=…`），或直接改钉 `checks.length`。 |
| **F2** | low-medium | **pin 保护的是「工具的判定行数」，不是「§12 的覆盖」**：工具**不**在运行时读 MASTER.md（只有注释提到它；检查表是硬编码 `const CHECKS = [`（`:4185`），17 个 spec 组展开成 65 条判定行）。§12 有 **77** 行（`MASTER.md:784`–`:2052`，1..77 无缺号；**`metrics.json` 里就有 `thresholdRows\|length = 77`**）⇒ **新增一条不产生判定的 §12 行，`judged==65` 依旧绿**。 | 后续小单：把 **§12 行数（77）作为第二个常量**钉住（CI 的 `metrics.json` 已经就有这个量，见 F5 的读法），并在文件头写明「65 = 工具判定行数，不是 §12 行数」这条边界。 |
| **F3** | low | **引用的坐标是错的**：`audit.yml:10-14` 文件头与 t122 报告都写「`exitDecision`（`design-audit.mjs:8431-8442`）」。实测：**函数定义在 `:8105-8111`**，`:8422` 是调用点，`:8431-8442` 是 `if (process.argv[1] …)` 的入口块。**声明本身是真的**（`:8107-8109` 三个致命源：`fails > 0` ∨ `contractWarnings.length > 0` ∨ `unmeasured > 0 && !allowNotMeasured`），只有坐标错。 | 注释级：把两处坐标改成 `:8105-8111`（或改用符号引用，避免再漂）。 |
| **F4** | —（已答） | **CI 上护栏 ② 是否红**：**否**。CI 的未测名单与 pin **逐项相同**（§5.2）⇒ 作者预期的「② 很可能红」**被证据推翻**，pin **不需要**按 CI 更新（也没有被放宽）。 | 无需改。这条记下来是为了**纠正预期**：以后把「CI 与 pin 一定不同」当默认假设是不成立的（至少这次 oxide 差异没影响这 10 行）。 |
| **F6** | low | **`fail` 名单仍未被钉住，而它在 CI 上已经与本机不同**（本机 2026-10-02：11 条 fail；CI：10 条，ids `4,6,7,12,13,14,18,20,38,75`）。文件头 `:24-33` 自己把「钉 fail 名单」列为转阻塞的前置条件 ✓ ⇒ 这不是隐藏缺陷，但**今天它意味着：一行 fail↔pass 的翻转对任何护栏都不可见**（只有总和 65 受影响，而它不变）。 | 转阻塞前照 ② 的形状加第二个常量（fail 名单），并把 CI 这次读数作为初始值。 |


## 7 未覆盖什么（第 19 条）

1. **本机没有跑真正的 `--check`**（只跑了死端口形式）：一次全量审计要 10.4 分钟 + 浏览器 + 真 daemon。⇒ 「`judged==65` 在本机再现」这一读数**没有**由我取得；我取到的是它的**定义**（`:8397`）与**两次历史读数**，以及 **CI 那次独立再现**（§5）。作者报告 §6 的本机读数我**没有**复跑，只做了溯源。
2. **§12「12 行没有判定行」是算术推断**，不是逐行实测（见 §4.3）。
3. **CI 的 chromium/字体差异**对未测名单的影响：只能由 §5 的 CI 读数回答。
4. **`NO_PROXY` 在 CI 上是否需要**（作者报告 §7 第 4 条登记的风险）：§5 的 CI 读数里「Boot a throwaway daemon」= `success` ⇒ CI 上连自己的 daemon **没有**被代理挡住（至少那一步没有）；但这不等于审计步里的 Chromium 也不受影响，仍然只有 §5 的完整读数能回答。
5. **`schedule` 触发未验**（下周一 03:00 UTC 才第一次），只能验 `on:` 的 YAML 形状。
6. **branch protection 的其它分支/规则**未查（只查了 `main` 的 protection = 404）。
7. 本机 `check-workflow-refs.sh` 用的 **git-bash 5.2.37**，与 CI 的 ubuntu bash 不同；脚本本身只用 `git ls-files` + PyYAML，跨平台风险低，但不等于 CI 上跑过。
8. **CI 截断的根因未定案**（§5.3 第 4/5 点）：我证明了「文件在半行处断于 ≈64 KiB」与「工具跑到了收尾」，但**没有**在 CI 的平台上复现机制 —— 本机探针（Windows + Node v24.13.0）**没有**复现 `process.exit()` 截断，而 CI 是 ubuntu + Node 22 ⇒ 「平台相关」是**待证**而不是已证。定案需要一次 Linux 上的受控复现（命令写在 F5 里）。
9. **我没有在本机跑全量 `--check`**：因此「本机 `judged==65`」仍未由我独立再现（我拿到的是 CI 的 65 + `metrics.json` 的 65 + 算式 `checks.length − pending`）。本机的 44/11/10 只有 t122 的原始日志为证。
10. **`schedule` 触发的第一次运行**（下周一 03:00 UTC）未验；`cancel-in-progress: false` 的实际行为（并发两次 dispatch）也未验。
11. **工件保留期**（`retention-days: 14`）与 `actions: write` 权限的最小性未逐条核（只核了 job 权限里没有 `contents: write`）。


## 8 纪律回执

- **写入集合 = 本报告一个文件**；`.github/**`、`panel/tools/**`、`crates/**`、`scripts/**` **零改动**（`git status --porcelain -- .github panel crates scripts` 无我的条目）。
- **未** push / dispatch / rerun / cancel / 建 tag；只用了 `gh run view` / `gh api`（只读）。
- **未触碰活守护进程 8787（pid 14944）与活库**：本机只跑了死端口形式（`127.0.0.1:1`）与**替身 node**，没有起任何真 daemon；`--out` 全部落在 `%TEMP%\t134\…`（仓库外）。
- 我的临时资源：`%TEMP%\t134\`（抽出的 `guard.sh`/`negctl.sh`/`boot.sh`/`audit.sh`/`verdict.sh`、合成日志、summary、替身 `node`、matrix/guard3 脚本、`rt/negctl` 输出）· `%TEMP%\t134*.py` · `%TEMP%\t134-ci.log` · `%TEMP%\t134-refs.log`。**保留为证据，未删**（其中没有仓库内文件、没有常驻进程）。
- 我**未**按名字/端口批量杀任何进程；本次没有起过需要收尾的常驻进程（替身 `node` 是脚本，死端口无人监听）。
