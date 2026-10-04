# 首次 CI 读数收口：护栏的计数改从 `metrics.json` 来（t143）

> 单号 **t143**（repair，成员 `wiki`）· attempt 1 · **2026-10-04 01:2x–02:0x（+08:00）**
> **追加（2026-10-04，t143 结单之后）**：§1b 与 §7 已用**第二次运行** `37141491947` 的工件读数补成**实测**（读数由 captain 从工件提供）。**只改本报告**：`audit.yml` 与 `gen4-audit-gate-impl.md` 在结单后**未再动**；门禁已在**最终字节**上重跑（§9 末行）。
> inScope 与实改：`.github/workflows/audit.yml`（**+134/−31**，443 行）· 本报告 · `docs/design/reviews/gen4-audit-gate-impl.md`（带日期的修订记录）。**未改** `crates/**`、`panel/**`（含 `panel/tools/**`）、`scripts/**`、`ci.yml`、`e2e.yml`、`release.yml`。
> 纪律：**未** push / dispatch / rerun / cancel；**未**跑 rust 构建；未碰 8787。

## 0 结论

**首次 CI 运行红在接线，不在测量。** 护栏 ① 当时读的是 stdout，而 CI 的 stdout **被截断在报告中间**（`audit.log` 67,938 B，末行断在 `**⚠️ 这是惯例阈值**：**CIE76 的 JND ≈ 2.3 `），汇总行因此从未出现；而工具**早在打印长报告之前**就把 `metrics.json` 写好了（`design-audit.mjs:8385` 写文件，`:8390-8391` 才打印报告）—— 那份文件**完整**：65 条判定 / 24 captures / 45 pass / 10 fail / 10 not measured，未测集与 pin **逐项相同**。
**t143 的处置（合同的选项 ①）**：护栏的计数、两份名单、captures 全部改从 `metrics.json` 读；stdout 的汇总行**没有删掉**，改为**交叉校验**（两者不一致 ⇒ 红 —— 这是这次新加的护栏）。⇒ 首次 CI 的那个形状（artefact 完整 + stdout 被截断）在本地复现后，**新接线 exit 0**，旧接线 exit 1（复现出 CI 的原话）。**四道护栏都仍能红**（§5）。

## 1 首次 CI 读数（逐字；来源 = captain 从 run 工件取的读数，我照此作为证据）

| 项 | 读数 |
| --- | --- |
| run | **`37137554330`**，`workflow_dispatch`，**20m47s**，结论 **completed failure** |
| 审计步 | **exit 1**（advisory，`continue-on-error` 未拦；这一步的红本身就是「产品读数」） |
| 护栏步 | **exit 1**，`##[error] no '[audit] N captures ... checks P pass / F fail / U not measured' line in the log` ⇒ **唯一点火的是护栏 ①** |
| `metrics.json` | **完整**：`checks total: 65` · `captures: 24` · `verdict: fail=10 / not_measured=10 / pass=45` ⇒ `judged = 65` |
| FAIL 集 | **`4 6 7 12 13 14 18 20 38 75`** |
| NOT_MEASURED 集 | **`25 28 31 33 46 47 49 50 54 77`** ⇒ **与 `EXPECTED_NOT_MEASURED` 逐项相同** |
| `audit.log` | **67,938 B**，**在一句话中间被截断**（末行 `**⚠️ 这是惯例阈值**：**CIE76 的 JND ≈ 2.3 `） |
| 计时 | 审计步 `16:48:30 → 16:58:33` = **10m03s**；单次捕获 **23.7s**（`[audit] dark home: 23745ms`） |

**⇒ 由此可判定的两件事（不是推测，是这次读数直接蕴含的）**：**②（未测集与 pin 相同）本该绿**、**④（`judged == 65`）本该绿**；它们没跑起来是因为 ① 先 `exit 1` —— **红的是接线**。这一点我在本地用同一份「artefact 完整 + stdout 被截断」的输入**跑成了 0**（§5 格子 6），不是口头承诺。

## 1b 第二次运行 = t143 修复的 CI 验证（**实测**；run `37141491947`）

`workflow_dispatch`，ref `main` = **`b1c59e7`**（含 t143 的字节）：**`completed` · `success` · 16m51s**；12 步全 `✓`，其中 **`✓ Design audit --check (ADVISORY)`**、**`✓ Guardrails`**、`✓ Upload`、`✓ Record the verdict`。工件 `design-audit` 的读数（captain 下载后提供）：

| 工件 | 读数 | 含义 |
| --- | --- | --- |
| `audit.exit` | **1** | advisory 判定 = 1 —— 与首次 CI 相同（产品侧仍是 10 fail / 10 not measured） |
| `counts.env` | `captures=24` · `checks=65` · `pass=45` · `fail=10` · `unmeas=10` · `pending=0` · **`judged=65`** · `failing='4, 6, 7, 12, 13, 14, 18, 20, 38, 75'` · `observed_raw='25 28 31 33 46 47 49 50 54 77'` | **护栏第一次在 CI 上拿到结构化计数**；未测集**逐项命中 pin**（⇒ ② 绿）、`judged == 65`（⇒ ④ 绿） |
| `counts.err` | **0 B** | 读取器没有报错 ⇒ 「路径不同」那条嫌疑**不必再查**（它在 CI 上就是同一个 `$RUNNER_TEMP/audit/metrics.json`） |
| `audit.log` | **67,938 B** | **与首跑被截断的字节数【完全相同】** ⇒ 截断**不是随机的** |
| 护栏步自报 | 走了**第二支**：`counts source=metrics.json (structured) · stdout summary line ABSENT (log <N> bytes, truncated mid-report …)` | 正是我在本地格子 6 跑出来的那一行 ⇒ **本地复现的接线行为与 CI 一致** |

⇒ **t143 的修法在 CI 上成立**：同一个「artefact 完整 + stdout 被截断」的形状，**首跑红（护栏 ①）、本次绿**，且 ②④ 的判据都直接可读。

## 2 载体差异的精确边界（这条要写清）

| 载体 | captures | pass / fail / not measured | FAIL 集 | NOT_MEASURED 集 |
| --- | --- | --- | --- | --- |
| **本机**（t122，2026-10-02，Windows + chromium-1243） | 24 | **44 / 11 / 10** | `2 4 6 7 12 13 14 18 20 38 75` | `25 28 31 33 46 47 49 50 54 77` |
| **CI**（2026-10-04，ubuntu-latest） | 24 | **45 / 10 / 10** | `4 6 7 12 13 14 18 20 38 75` | `25 28 31 33 46 47 49 50 54 77` |

- **差别只有一处**：**第 2 行在 CI 上通过**（本机它 fail）；两份名单的其余部分一致。
- **未测集完全相同** —— 这正是 pin 的形状：**pin 钉的是「哪些行测不了」，而载体会改变「个别行判成什么」**。
  ⇒ 精确边界：**pin 对载体稳健（本次实测）；若把 pass/fail 名单也钉成常量，它不会稳健**（第 2 行就是反例）。这条结论直接影响 §4 的「何时可以改阻塞」：那条路要求把 fail 名单也钉住，而**必须先解决载体差异**（同一行在两种载体上判定不同）。
- 证据边界（诚实）：上面的 CI 数字是 **captain 从 run 工件读到并转述**的；我**没有**自己下载工件（不许 dispatch，也没有用 `gh` 去拉那个 run 的 artifact）。本报告因此把它标成「转述读数」，而**我亲手复现的是同一份数字的形状**（§5 格子 1：`45/10/10` + pin 集 ⇒ 0）。

## 3 接线缺陷的证据链（为什么这是接线，不是产品）

1. **源码顺序**（我回读的，不是印象）：`:8384` 组装 `doc` → **`:8385` 写 `metrics.json`** → `:8386` 写 `report.md` → **`:8390-8391` 把长报告打到 stdout** → **`:8394-8398` 才计算并打印汇总行**。⇒ **汇总行在长报告之后**，而 artefact 在它之前 ⇒ 一旦 stdout 这个载体在报告中途丢失，**只有 stdout 受影响**。
2. **CI 现象**与 1 完全吻合：`audit.log` 断在报告正文中间，汇总行从未出现，`metrics.json` 完整。
3. 护栏 ① 当时**只**看 stdout ⇒ 它把「载体丢了」读成了「什么都没测」。这正是 t101/t104/t110 那条规则的**误伤面**：规则本身是对的（不许把「没测」读成「测了」），但它**选错了证据来源**。
4. **未解决的一处（登记，不掩盖）**：`audit.log` 被截断的原因我不能从本地判定 —— 是「进程中途死亡」还是「载体/写端截断」。两种解释不改变本单的处置（`metrics.json` 在两种情况下都完整），但**下一步的探针已经埋好**：新护栏把 `log_bytes` 与 `counts.err` 写进 step summary 并上传（`counts.env` / `counts.err` / `counts.cjs` / `audit.exit` 也进了 artifact 列表）⇒ 下一次 CI 运行能直接从工件里回答「日志多少字节、是否有汇总行、artefact 是否完整」。

## 4 决定：选项 ①（读 `metrics.json`），以及为什么不是 ②

- **选 ①**：把**计数、两份名单、captures** 全部改从 `--out` 目录里的 `metrics.json` 读。它今天就在 artifact 里、今天就是完整的，且**写在长打印之前**（§3 第 1 条）。
- **不选 ②**（要求工具「在长报告之前先印/落盘汇总」）：那要改 `panel/tools/**`（**本单 out of scope**），而且**只能救一行**；① 一次把**全部读数**搬到不会随 stdout 丢的东西上。⇒ 若将来仍想做 ②，配方是：在 `:8385` 之后、`:8390` 之前 `log()` 一行同样的汇总（或把汇总写进 `metrics.json.meta`），**由 captain 决定是否另立单**；本单不改工具。
- **没有削弱任何护栏**：① 的条件由「stdout 汇总行缺失」换成「**结构化计数缺失/不可解析**」（仍然红，见格子 3/8）；② 的 pin 比对改从 artefact 取（仍红，格子 2）；④ 的 `captures ≥ 1` 与 `judged == 65` 逐字未动（仍红，格子 4/5）；③ 负控一步未动（仍红，§6）。**另外新增**一道：**两个载体必须一致**（格子 7，旧接线对它**看不见**）。

## 5 本地复现与十个格子（新接线）＋ 四个旧接线对照

方法：`python -c yaml.safe_load` 把 `jobs.audit.steps[*].run` **逐字**导出（9 个块；`bash -n` **9/9** 通过），用**私有** `RUNNER_TEMP` / `GITHUB_STEP_SUMMARY` 跑护栏；每个格子准备一套 `audit.log` + `audit/metrics.json` + `audit.exit`。旧接线用的是 **`git show HEAD:.github/workflows/audit.yml`** 里的同一脚本（逐字）。

| # | 格子 | 输入 | 新接线退出 | 旧接线退出 | 谁点火 |
| --- | --- | --- | --- | --- | --- |
| 1 | **CI 读数**（45/10/10，日志完整） | metrics + 全文日志 | **0** | 0 | —（`counts source=metrics.json · stdout summary line present and in agreement`） |
| 1b | **本机读数**（44/11/10，第 2 行 fail） | metrics + 全文日志 | **0** | — | —（⇒ pin 接受**两种载体**） |
| 2 | 未测集漂移（`#77` 变 measurable） | metrics（9 条未测） | **1** | — | ② `not_measured set moved: expected '#25 … 77', got '#25 … 54'` |
| 3 | **没有 `metrics.json`** | 无 artefact | **1** | — | ① 新形状：`no usable counts: … is missing or unparseable` |
| 4 | `judged == 64` | metrics 64 条判定 | **1** | — | ④ `judged=64 (pinned 65), checks=64, pending=0` |
| 5 | `captures == 0` | metrics 0 captures | **1** | — | ④ `the audit captured 0 pages` |
| 6 | **CI 的形状：artefact 完整 + stdout 被截断** | metrics(CI) + **无汇总行**的日志 | **0** | **1** | 旧：① `no '[audit] N captures …' line in the log`（= **CI 的原话，本地复现**）／新：`counts source=metrics.json (structured) · stdout summary line ABSENT (log 8844 bytes, truncated mid-report …)` |
| 7 | **两载体不一致**（日志 44/11，artefact 45/10） | 两者矛盾 | **1** | **0** | 新：`the two carriers disagree -- metrics.json says … 45 … while the stdout summary line says … 44 …` ⇒ **旧接线对此完全看不见** |
| 8 | `metrics.json` 自身被截断 | 断掉的 JSON | **1** | **0** | ① `no usable counts: … unparseable` ⇒ 旧接线同样看不见 |
| 9 | 一行进入 `pending`（t141 的 F1 场景） | metrics 1 pending | **1** | — | ④ `judged=64 (pinned 65), checks=65, pending=1`（t141 的报文仍在） |

**五格矩阵（1/2/3/4/5）= `0 / 1 / 1 / 1 / 1`** —— 与改前完全一致（期望正中）。
**格子 6 是新旧之间唯一的行为变化，也正是 CI 该有的行为**：那次红的**唯一**原因是载体被截断。

**格子 6 的 step summary（新接线，逐字节选）**：
```
### design-audit --check (advisory)
[stdout carried no summary line: the log was truncated mid-report (8844 bytes). The counts
 below come from metrics.json, which the tool writes BEFORE it prints the report
 (design-audit.mjs:8385 vs :8390).]
- audit exit code: **1** (advisory: recorded, not enforced)
- counts source: **metrics.json** (structured artefact, complete even when the log is truncated); stdout summary line: ABSENT · log size: `8844` bytes
- judged rows: **65** (pass 45 / fail 10 / not measured 10); the pinned constant is **65** …
- failing rows: `4, 6, 7, 12, 13, 14, 18, 20, 38, 75`
- not measured (matches the pinned set `#25 28 31 33 46 47 49 50 54 77`): `#25 28 31 33 46 47 49 50 54 77`
```
（旧接线对同一输入给出的是 `### Guardrail ① FAILED: the run produced no summary line` —— **就是 CI 上那一步**。）

## 6 `timeout-minutes` 的新依据（基于 CI 实测，替换旧推导）

| 量 | 旧推导（t131） | **CI 实测** |
| --- | --- | --- |
| 审计耗时 | 本机 624.6s（10m24.6s）× 3 ≈ 31m | **10m03s**（16:48:30 → 16:58:33，24 captures ⇒ 平均 **25.1s/capture**，单次实测 23.7s） |
| 前置（cargo/panel/playwright/起 daemon） | ≈ 6m | **≈ 10m44s**（整 job 20m47s − 审计 10m03s） |
| 推导 | ~37m ⇒ 取 45 | **整 job 实测 20m47s** ⇒ `45` = **2.16 ×** 实测整 job、≈ **4.5 ×** 实测审计步 |
| 结论 | — | **保持 `timeout-minutes: 45`**：它仍是「挂住」与「慢」可分辨的上界（GitHub 默认 360m 则不可分辨），而 CI 的审计比我本机那次**更快**（603s vs 624.6s）⇒ 旧推导落在同一处，但**依据换成 CI 实测** |

## 7 未覆盖什么

1. **截断的原因 —— 已把读数补成实测，但仍未定案（诚实状态）**。实测：`audit.exit = 1`、`audit.log = 67,938 B`（**两次独立 `workflow_dispatch` 逐字节相同**）、汇总行缺席、`metrics.json`/`counts.env` 完整。按我**事先登记**的决策表（在第二次运行之前发给 captain 的那张），这落在**「不可分」**那一格：
   | `audit.exit` | 日志 | 汇总行 | 含义 |
   | --- | --- | --- | --- |
   | **1** | **67,938 B（两次同值）** | **无** | **① 进程跑完返回判定 1，载体丢了尾巴** ↔ **② 进程在打印中途撞 EPIPE（写端消失），Node 未捕获的流错误默认也是 exit 1** —— **两者同码，工件分不开** |
   | 137/139/143 | — | 无 | 进程被杀 ⇒ 进程死亡（**本次不是**） |
   | 2 | — | 无 | 工具自己的 catch ⇒ 进程死亡（**本次不是**） |
   ⇒ **确定性**（两次同值）**不利于**「OOM/被杀」这类随机死亡；但它**本身不能**把 ① 与 ② 分开。
2. **我试过一次机制实验，结果是【负】的（如实登记）**：假设是「Node 对**管道**是异步写，`process.exit()` 会丢掉未刷完的尾部；对**文件**是同步写，所以不丢」。用真 Node（**v24.13.0**）与真实的字节管道测了 **7 次**（`cmd.exe` 管道两端都是显式 `node.exe`；载荷 56.5 kB 与 **192.9 kB**；快速消费者 `tee` 与**慢消费者** `sleep 2; cat`；`process.exit(1)` 与 `process.exitCode=1` 两种收尾）⇒ **每一次都完整送达（含汇总行），file-redirect 参考值也相同** ⇒ **该机制在这台机器（Windows）上【不复现】**。Linux 侧的管道语义**我无法在本机测**（本机没有 Linux node）⇒ 这条假设**既未被推翻、也未被证实**；它的判据仍是 captain 定的「**C 项落地后再点一次 dispatch**」。
3. **两个决定性探针（我登记，未动手；都需要改 `.github/workflows/audit.yml` 的审计步 ⇒ 由 captain 决定是否立单）**：
   - **C**：把 `node … 2>&1 | tee "$RUNNER_TEMP/audit.log"` 换成**不经管道**的 `node … > "$RUNNER_TEMP/audit.log" 2>&1`（`PIPESTATUS[0]` 换成 `$?`）。⇒ 若日志变完整：机制是**管道/刷写**；若**仍然截断**：机制在**文件侧/工具侧的写入**，与管道无关。
   - **D（更便宜，且能与 C 同时做）**：在管道后加一行 `echo "tee-exit=${PIPESTATUS[1]}"`。⇒ **tee 非 0**（如 141）说明**下游先断**（node 的 stdout 被关 ⇒ EPIPE 一侧）；**tee = 0 而文件仍短**说明是 **node 侧的写端提前结束**。这一行把 ① 与 ② 直接分开。
4. **我没有下载那个 run 的 artifacts**（不许 dispatch；也没用 `gh` 拉工件）⇒ §1 的首次读数与 §3 的第 1 条都是**转述**；§1b 的第二次读数同样是 captain 下载后转述的。我亲手做的是：**照同一份数字在本地复现形状**（§5 全部格子）与**两次运行间的一致性核对**（未测集/判定集一字不差、日志字节数相同）。
5. **本机跑护栏需要一个 node 桥接**（诚实披露）：这台机器唯一的 bash 是 WSL，里面**没有 Linux node**；WSL interop 能按绝对路径执行 Windows 的 `node.exe`，但**不翻译 argv 里的 `/mnt/c/...` 路径**（cwd 会翻译）。所以我给本地 harness 加了一个 `node` shim（`cd` 到脚本目录并传**相对路径**）。**CI 不需要它**（setup-node 把 node 放进 PATH）⇒ shim 只影响「本地怎么跑」，不影响脚本逻辑；但「在真正的 Linux node 上跑」这一条我**没有**在本地复现。§2 第 2 条那条机制实验后半段我把管道两端都换成**显式的 `C:\Dev\NodeJs\node.exe`**（不再经过 shim），所以那 7 次读数的 node 身份是确定的。
6. **`panel/tools/**` 未动**：选项 ② 的配方见 §4，仍待 captain 决定是否另立单。
7. **护栏 ③（负控）的改造未做**：它今天只断言「死端口 ⇒ exit 2 + 点名」（`audit.exit` 与 stdout 都够用）；我没有把它也改成读 artefact —— 因为它的失败输出只有两行、CI 上从未被截断（两次运行里它都是绿的）。登记为**观察项**，不是缺口。
8. **`metrics.json` 的 schema 未加版本断言**：新的读取器要求 `checks[].n` 是数字、`checks[].verdict` ∈ {pass, fail, not_measured, pending}（缺一个就红，格子 8 的邻居）；但**没有**校验 `meta`/`contract` 等字段。数值口径若变（例如 verdict 改名），这道护栏会红——那是**设计**（宁可红也不静默读错）。

## 8 逐行分类（我动过的每一行）

`git diff -U0` 的机械分类：**新增 134 行 = 50 注释 + 12 报文 + 71 代码 + 1 空行**；**删除 31 行 = 14 注释 + 4 报文 + 13 代码**。

**控制流的逐条理由（增删都列）**：
| 变更 | 位置 | 理由 |
| --- | --- | --- |
| `if [ -f "$METRICS" ]; then … else … fi` | 读取器调用处 | 不存在的文件不该交给 node 去报错；缺失走下面的 ① 红（并且 `counts.err` 里留下「文件不在」这一行） |
| `if [ ! -s "$COUNTS" ]; then … exit 1; fi` | **新 ①** | 计数必须存在 ⇒ 取代旧的「stdout 汇总行必须存在」。红的能力保留（格子 3/8），**证据来源**换掉（这正是本单的目的） |
| `line=$(grep … )`（保留，位置下移） | 交叉校验 | 汇总行不再当数据源，改当**第二载体** |
| `if [ -n "$line" ]; then … 5×sed … fi` | 交叉校验 | 只有 stdout 真的带了汇总行时才比对（截断时不该红） |
| `if [ "$s_captures" != "$captures" ] \|\| … ; then echo ::error::; exit 1; fi` | **新护栏** | 两载体不一致 = 仪器缺陷；旧接线对此**完全看不见**（格子 7 旧=0） |
| `if [ -n "$line" ]; then echo "$line"; else echo "[…truncated…]"; fi` | summary 围栏 | 报文：截断时不留空围栏，改写明「计数来自 artefact」 |
| `\|\| true`（`node … > …` 与两处 `tail`） | 读取器与红块 | 让**护栏自己**的报文成为失败原因，而不是读取器的次生错误 |
| 删除 `if [ -z "$line" ]; then … exit 1; fi` | 旧 ① | 被新 ① 取代（见上） |
| 删除 `captures/pass/fail/unmeas/pending/judged/checks/failing/observed_raw` 的 **stdout 解析**（9 行） | 旧数据源 | 全部改从 `counts.env`（由 `metrics.json` 生成）—— 这是本单的核心替换 |
| 删除 `tail -40 "$LOG"` | 旧 ① 红块 | 换成 `counts.err` 的 5 行 + 日志尾 20 行（更能说明「读取器为什么失败」） |

**未动的（逐字沿用）**：`if [ "$captures" -lt 1 ]`（④）、`if [ "$judged" != "65" ]`（④，含 t141 的三量报文）、② 的 `EXPECTED_NOT_MEASURED` 与 `norm()` 比对、③ 的负控步、审计步、`timeout-minutes`、权限、`uses:` 固定 SHA。
**非控制流新增（配置）**：artifact 路径加了 `audit.exit` / `counts.env` / `counts.err` / `counts.cjs`（4 行，理由：让下一次 CI 能直接从工件回答 §7 第 1 条）。

## 9 门禁与纪律

| 门 | 命令 | 读数 |
| --- | --- | --- |
| refs + 表达式 + YAML | `bash .github/workflows/scripts/check-workflow-refs.sh` | **exit 0**；`workflow path references checked: 22, not tracked/missing: 0`；`.github/workflows/audit.yml: parses as YAML (PyYAML)`（四份都过） |
| 独立 YAML 回读 | `python -c yaml.safe_load` | `parses; steps = 14` |
| 抽出的 9 个 `run` 块 | `bash -n` | **9/9 exit 0** |
| 四道护栏的能红性 | 格子 2（②）、3/8（①）、4/5（④）、③ 变异（真在听端口 ⇒ 步 exit 1） | 全部 **能红**；③ 健康路径仍 exit 0（`negative control exit=2 (want 2)` + `daemon not reachable …` 点名） |

- **写入集合**：`.github/workflows/audit.yml`、本报告、`docs/design/reviews/gen4-audit-gate-impl.md`（修订记录）。其余 `git status` 条目都是队友的在途交付。
- **未** push / dispatch / rerun / cancel；**未**跑 rust 构建；**未**碰 8787（本单不需要 daemon；唯一一次起进程是我自己的 python stub，已停并核对 `8898 listening: False`）。
- 临时项：`%TEMP%\t143`（提取脚本、9 个 run 块、10 个格子、wrapper、node shim、stub）与 `%TEMP%\t143b`（机制实验的 writer/harness）—— 收尾按**具体路径**删除。

**结单后的追加编辑（按 t20 的义务披露）**：本次只改了**本报告**（§1b 新增、§7 第 1–3 条补成实测并登记两个探针、§0 未动），`audit.yml` 与 `gen4-audit-gate-impl.md` **未再动**；门禁在**最终字节**上**重跑**如下（读数属于追加后的这一版）：
| 门 | 命令 | 读数（追加后重跑） |
| --- | --- | --- |
| refs + 表达式 + YAML | `bash .github/workflows/scripts/check-workflow-refs.sh` | **exit 0**；`workflow path references checked: 22, not tracked/missing: 0`；四份 workflow 均 `parses as YAML (PyYAML)` |
| 独立 YAML 回读 | `python -c yaml.safe_load` | `parses; steps = 14` / `timeout-minutes = 45`（未动） |
| `audit.yml` 的字节是否动过 | `sha256(worktree)` vs `sha256(git show HEAD:.github/workflows/audit.yml)` + `git diff --numstat` | **两者相同 = `b88a33a29d449ffc72647c0baadf72c54f8ebbe4d4df9a118bceccaea67a1408`**（git blob `cc5c28ac927dd14cec6daf36f8a7f789b87dc3f0`，HEAD `d6ace1b`）；`git diff` **为空** ⇒ **我已交付的字节就是已提交的字节**，追加编辑**没有**碰工作流（结单时的 diff 是 **+134/−31**，那是**推送前**的读数） |
