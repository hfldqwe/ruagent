# CI 守卫加固：rc 必须是退出码 · 扫到 0 条必须红 · 半检查缺席不许静默 · 非法 `${{ }}` 必须被拒（t106）（mem-core）

- 任务：t106（kind=work）；attempt `adbd4e42-4b8e-4d24-97d8-6052fb63494b`
- inScope：`.github/workflows/scripts/test-evidence.sh` · `.github/workflows/scripts/check-workflow-refs.sh` · 本报告
- **未动** `.github/workflows/ci.yml`、`.github/workflows/e2e.yml`（收尾 `git status --porcelain -- .github/workflows/*.yml` = **0**）· 未 push / dispatch / rerun / cancel / tag
- 来源：t105 的只读审计（`gen3-ci-guard-scripts-audit.md`）+ captain 的 t106 追加判据（非法 `${{ }}`，本轮真实事故）

## 0 改动前后（哪一份字节，变了多少）

| 文件 | 改前 sha256 / 行数 | 改后 sha256 / 行数 | 变更 |
| --- | --- | --- | --- |
| `check-workflow-refs.sh` | `493175c7…` / 223 | `6cd24c4c…` / **366** | +149/−2：**F-A2**（空扫描下限）· **F-A3**（workdir 剥引号）· **F-A9**（`$CI` 下缺 PyYAML ⇒ 红；本地措辞改为「UNMEASURED」）· **t106 新判据**（`${{ }}` 表达式形状检查，不依赖 PyYAML） |
| `test-evidence.sh` | `c6f1af78…` / 84 | `dbd862b6…` / **135** | +69/−3：**F-A1**（rc 形状校验 + 空 rc 注解化）· **F-A5/F-A6**（计数读 `tr -d '\r'` + 剥 ANSI 的副本）· 头注释记录这两族 |
| 合计 | — | — | `git diff --stat -- .github/workflows/scripts` = **2 files, +206, −12** |

`bash -n` 两份都 **syntax OK**（GitHub 的 `bash -e {0}` 里脚本是子进程、自带 `set -uo pipefail`/`set -u`，与 t105 的等价性结论一致）。

## 1 每项改动的判据与负控读数

### 1.1 F-A1（假绿，必须修）：rc 必须是退出码

**改前**（t105 实测）：`test-evidence.sh strrc healthy.log boom` ⇒ stderr `[: boom: integer expression expected`，**exit=0**，整表绿 ⇒ 退出码那半条门禁当场失效。

**改后**（同样命令）：

```
::error::strrc: 'boom' is not an exit code (expected a non-negative integer) -- an unvalidated exit code is not a green (t105 F-A1)
exit=1
```

| 控制 | 读数 |
| --- | --- |
| `rc=boom`（非数字，改前 exit=0） | **exit=1** + `::error::… is not an exit code …` |
| `rc=""`（空；改前是 bash 自己在 `${3:?}` 上抛 `3: exit code`） | **exit=1** + `::error::emptyrc: no exit code was given (empty rc argument) …`（注解化，语义不变：还是红） |
| `rc=0`（合法） | **exit=0** + `okrc: 2 target(s) ok, 0 failed, 0 panic site(s), exit 0` |
| `rc=2`（合法但非 0） | **exit=1** + `::error::rc2: exit code 2 with 0 failing target(s)` |
| `rc` 形状校验先于使用 | 代码里 `rc_bad` 在计数与判定之前算好；非数字时**不再**进入 `[ "$rc" -ne 0 ]` ⇒ 不会再有「整数表达式」告警 |

**边界（不许含糊）**：这只管「rc 是不是退出码」，**不管** rc 与日志是否自洽（rc=0 而日志有 FAILED 会被 `failed>0` 抓到；rc=1 而日志全 ok 会被 `rc≠0` 抓到——两条都在改前就有）。

### 1.2 F-A2（判据缺下界，必须修）：`checked: 0` 必须红

**改前**：`checked: 0` 与「全部已跟踪」打印同一行 `every executed path a workflow references is tracked` ⇒「什么都没扫到」不可见（t105 的 6 个实例全部 exit=0）。

**改后**：`refs == 0` ⇒ 视**是否见过 `jobs:`** 分两种原因，打 `::error::` 并 **exit 1**：

```
N5  capital Jobs:      exit=1  ::error::… no 'jobs:' key was found in 1 workflow file(s), so nothing was scanned -- an empty scan is not a green (t105 F-A2). Is the key indented, capitalised ('Jobs:') or written 'jobs :'?
N6  'jobs :'           exit=1  同上
N10 无 jobs: 的文件     exit=1  同上
N7  纯引号引用          exit=1  ::error::… 'jobs:' was found but NO executable path reference was recognised in 1 workflow file(s) -- an empty scan is not a green (t105 F-A2). Quoted, variable-carrying and extension-less references are deliberately not scanned; …
N8  python3 tools/gen.py exit=1 同上
N9  根级 bash rootscript.sh exit=1 同上
N11 真实树（无参数）    exit=0  workflow path references checked: 15, not tracked/missing: 0 + every executed path … is tracked
```

⇒ 六条「本该红」的实例全部转红，**真实树仍是 `checked: 15` exit=0**（不误报）。

### 1.3 F-A9（半检查静默缺席）：`$CI` 下缺 PyYAML ⇒ 红；本地 ⇒ 明说 UNMEASURED

| 场景 | 读数 |
| --- | --- |
| 有 PyYAML（本机 WSL 5.4.1） | `  .github/workflows/resolution.yml: parses as YAML (PyYAML)`；exit=0 |
| `CI=true` + 桩 `python3`（`command -v` 命中、`import yaml` 失败） | **exit=1**：先印 `PARSE CHECK SKIPPED (python3 is present but has no PyYAML module): the full-YAML half of this guard did NOT run.`，再 `::error::check-workflow-refs: PARSE CHECK SKIPPED (…) while CI is set -- the full-YAML half of this guard did NOT run, and in CI that is an environment defect, not a pass (t105 F-A9).` |
| 无 `CI` + 同一个桩 | **exit=0**（本地可用性），但多印一行 `NOT a green for 'the workflows parse': this half is UNMEASURED here, and the files were NOT parsed.` ⇒ 不会被读成「解析过了」 |
| 无 python3 那一支 | **仍未取到**（原因见 §5） |

### 1.4 F-A3（假红，本轮修）：`working-directory: "ok"` 的引号不对称

剥掉一层外层引号（单/双都剥），并把**该值**按剥后路径判定；token 扫描本来就是引号感知的，现在两者一致。

```
P2  working-directory: "ok"  ⇒ exit=0  … ok/  tracked (working-directory)
P2b working-directory: 'ok'  ⇒ exit=0  … ok/  tracked (working-directory)
（改前：`… "ok"/   MISSING/UNTRACKED` ⇒ exit=1 假红）
```

### 1.5 F-A5 / F-A6（假红，本轮修）：CRLF 与 ANSI

计数与「被点名测试名」改读一份**归一化副本**（`tr -d '\r'` + 剥 `\x1b[…m`），原文日志仍用于不可读判定；若 `mktemp`/归一化失败则退回原日志并印一行 NOTE —— **门禁不能因为自己的管道而消失**。

| 控制 | 改前 | 改后 |
| --- | --- | --- |
| CRLF 日志 + `instrument_ran`（日志里确实有 `test instrument_ran ... ok`） | **exit=1** 假红（`did not run instrument_ran`） | **exit=0** + `crlf: 1 target(s) ok, …` |
| 整行 ANSI 着色的健康日志 | **exit=1** 假红（`reported 0 targets`） | **exit=0** + 表里 `test result:` 计数 = 1 |
| CRLF + ANSI + 真 FAILED + rc=1（组合） | — | **exit=1** 且理由正确：`::error::combo: exit code 1 with 1 failing target(s)` + 回显 `test result: FAILED. 0 passed; 1 failed` |

（`\r?$` 也加进了被点名名字的正则 —— 双保险：即使归一化失败，CRLF 也不会再假红。）

### 1.6 **t106 新判据（captain 追加）：非法/不可解析的 `${{ }}` 必须被拒**

**为什么必须加**：GitHub 对 `run:` 的**整段字符串**做表达式替换（cell 注释也算），非法表达式 ⇒ **整个 workflow 文件被拒**（`gh run view` 原文「This run likely failed because of a workflow file issue」、`run: 0s`、无 job、run 名退化成文件路径），而 **PyYAML 反而说文件合法** ⇒ t105 的解析半检查看不到这一层。

**实现**（不依赖 PyYAML，总是运行）：逐行找出每个 `${{`，对齐到 `}}`，对表达式体做四项形状判定 —— ① 非空；② 至少有一个**操作数**（标识符/字符串字面量/数字），因此**裸 `...` 必红**；③ 字母表（引号外的字符必须在 GitHub 表达式可用字符内）；④ 引号/圆括号/方括号配平。字符串字面量里的字符不参与字母表判定（见 §4 的自测缺陷）。

| 控制 | 读数 |
| --- | --- |
| **事故形状**：`run: \|` 里一行 shell 注释含字面 `${{ ... }}`（自造副本，**没有改 e2e.yml**） | **exit=1**：`.github/workflows/incident2.yml:10 BAD EXPRESSION: no operand (only operators/dots -- e.g. a bare '...'):\|...\| in \|          # mirrors e2e.yml before 5d3adfd: ${{ ... }}\|` + `::error::1 workflow expression(s) do not parse. GitHub substitutes ${{ ... }} over the whole run: string (shell comments included) BEFORE the runner starts, so an illegal expression rejects the ENTIRE file: "This run likely failed because of a workflow file issue", run 0s, no job, and the run name degrades to the file path (t106). PyYAML cannot see this layer.` |
| 同一文件把字面量改成文字描述 | **exit=0**（对照） |
| `${{ .... }}`（只有点） | red，理由 `no operand` |
| `${{ }}`（空） | red，理由 `empty expression` |
| `${{ github.ref`（未闭合） | red，理由 `unterminated ${{ ... }}` |
| `${{ (1 }}`（括号不配平） | red，理由 `unbalanced quotes, parentheses or brackets` |
| `${{ github@ref }}` / `${{ github.ref; rm -rf / }}` | red，理由 `characters outside the expression alphabet` |
| **不许误报**：12 种合法形状（`github.workflow`/`github.ref` 在 `concurrency.group`、`matrix.os`、`steps.x.outcome == 'success'`、`fromJSON(needs.a.outputs.b).c`、`hashFiles('**/Cargo.lock')`、`!cancelled() && github.event_name == 'push'`、`format('{0}-{1}', …)`、`secrets.*`、`inputs.*`、`github.event.inputs.*`、`runner.temp`/`job.status`/`needs.build.result`、`strategy.job-index`/`vars.FLAG`/`env.X`、`matrix.include[0].name`，**以及 shell 注释里一个合法表达式**） | **exit=0** |
| 真实三份 workflow（`ci.yml` 2 个表达式、`e2e.yml` 0 个、`release.yml` 15 个） | **exit=0**，`checked: 15, not tracked/missing: 0` |

**判据边界（captain 要求写明）**：它能拦住「**非法/不可解析**的形状」；**不等于**能拦住「表达式合法但语义写错」—— 例如恒 1 的 `success && 0 || 1` 是**合法表达式**，本检查**不会**报（那靠 t104 的「状态取自命令自身」的结构来防，不靠解析）。另：函数集合不校验（未知但形状合法的函数不报），双引号不报（GitHub 只在单引号里有字符串字面量，此处取**宽松**方向以避免误报）—— 都是**故意的下近似**，写在代码注释与本表里。

## 2 t105 负控清单重跑：哪几条仍能红、哪几条变了

| # | 控制 | 改前（t105） | 改后（t106） | 变化 |
| --- | --- | --- | --- | --- |
| M1 | 索引副本拿掉 `test-evidence.sh` ⇒ 引用必须红 | exit=1，5 处 UNTRACKED | **exit=1，5 处 UNTRACKED**，`::error::5 workflow path reference(s)…`；真实索引前后都命中 | **不变**（同一条 `::error::` 原文） |
| M2 | 未加引号 `: ` | exit=1 + `UNQUOTED ': ' …` + PyYAML 也拒 | 同 | 不变 |
| M3 | 0 个 reporting target | exit=1 + `reported 0 targets …` | 同 | 不变 |
| M4 | 健康日志 | exit=0 + 表格 | 同 | 不变 |
| S1 | 无此 workflow 文件 | exit=1 + `no such workflow` | 同 | 不变 |
| S2 | MISSING 引用 | exit=1 | 同 | 不变 |
| S3 | UNTRACKED 引用 | exit=1 | 同 | 不变 |
| S4 | `working-directory` 不存在 | exit=1（2 红） | 同 | 不变 |
| S5 | **解析规则**：相对引用按本步 workdir | exit=0 | 同 | 不变 |
| S6 | **步骤边界重置** | exit=1（第二处 MISSING） | 同 | 不变 |
| S7 | 日志不可读 | exit=1 | 同 | 不变 |
| S8 | FAILED 行、rc=0 | exit=1 | 同 | 不变 |
| S9 | rc≠0、日志健康 | exit=1 | 同 | 不变 |
| S10 | 点名测试名在 | exit=0 | exit=0（并印 `and all 1 required test name(s) reported ok`） | 不变 |
| S11 | 点名测试名不在 | exit=1 | 同 | 不变 |
| S12 | `grep -c` 0 命中不炸 | 打印 0、exit 1（被 `\|\| true` 吸收） | 同 | 不变 |
| — | `working-directory: "ok"`（F-A3） | **exit=1 假红** | **exit=0** | **变绿（修复）** |
| — | CRLF + 点名测试名（F-A5） | **exit=1 假红** | **exit=0** | **变绿（修复）** |
| — | ANSI 健康日志（F-A6） | **exit=1 假红** | **exit=0** | **变绿（修复）** |
| — | 6 个空扫描实例（F-A2） | **全部 exit=0 假绿** | **全部 exit=1** | **变红（修复）** |
| — | `rc=boom`（F-A1） | **exit=0 假绿** | **exit=1** | **变红（修复）** |
| — | 空 rc | exit=1（bash 裸错） | exit=1（注解化 `::error::`） | 变清楚，语义不变 |

**打不红的判据（点名，不修也要说清）**：
1. `panicked at`、`launched`、`ok+FAILED≠total` —— **本来就不是判据**（表格里是读数列）。改后仍未把它们做成判据：`should_panic` 会产生合法 panic 行，`launched>reporting` 在 `rc=0` 时也可能出现（日志被 `head` 截断）⇒ 做成判据就会误报。
2. 文本计数可被行首打印伪造（t105 F-A7）—— **本质上无法在纯文本层堵住**；锚是调用方传入的真实 `rc`（现在还会校验形状）。要真堵需解析测试协议，超出守卫职责。
3. 表达式检查对「合法但语义错」无能为力（见 §1.6 边界）—— 这一条**按设计不可红**。
4. `--` 之外的漏检族（t105 F-A4）**仍未修**，理由见 §3。

## 3 仍未修的假红/假绿与理由（第 19 条）

| # | 面 | 触发条件 | 为什么不修 |
| --- | --- | --- | --- |
| F-A4 | 漏检族：`.py`/`.rb`/`.md` 等扩展名、无斜杠的根级引用（`bash build.sh`）、`uses: ./local-action`、纯引号引用、含 `$VAR` 的引用 | 见 t105 §3 | 头部已自陈是**故意的下近似**；扩白名单会把「像路径的普通词」拉进来（误报换漏报）。**本轮已把它从「静默」变成「可选可见」**：当整个扫描为 0 时 `::error::` 会点名「no reference was recognised」，读者至少知道这次没看东西。要做真须逐个基准取证（各 owner 的单） |
| F-A7 | 计数是文本 | 日志里出现行首 `test result: ok …` | 见 §2 第 2 条；`rc` 是锚 |
| F-A10 | 三处「不带读数的 exit 0」（`ci.yml` 的编译失败/attempt 2 未跑分支、`e2e.yml` 的 NO READING 分支） | 上游步骤失败时 | **不在本单 inScope**（`ci.yml`/`e2e.yml` 归 t104）；三处都已明确打印「这不是测试结论」，且红由上游步骤负责。建议在 t104/后续单里写明「本步骤的绿依赖 X 步骤红」 |

## 4 方法学：**控制红了 ≠ 控制对了**（t105 那条，本轮又添三例）

1. **t105 的实例（保留）**：第一版「缩进 `jobs:`」控制 **exit=1**，但红的原因是 **YAML 非法**（PyYAML 分支），证明的是解析器不是扫描器 ⇒ 改用 YAML 合法的 `Jobs:`/`jobs :` 才拿到「扫到 0 条仍绿」的真读数。
2. **t106 新增**：`ok_concurrency`（12 种合法表达式）第一版 **exit=1** —— 不是表达式检查误报，而是**它自己触发了新的 F-A2 空扫描下限**（该文件全是 `echo ${{…}}`，一个路径引用都没有）⇒ 给它加一条真实引用后才得到 `exit=0` 的干净读数。
3. **t106 新增**：一次「缩进 `jobs:` 无关」的**控制被 cwd 污染**：我在临时 mini repo 里跑 guard 却传了**真实树**的 `ci.yml`，于是 `panel/` 之类被按 mini repo 解析 ⇒ 报出 8 处 not-tracked（5 处 `test-evidence.sh` + 3 处 `panel/`）。干净的读数必须在**仓库根**取：`checked: 11 … not tracked/missing: 5`。
4. **t106 新增**：索引副本放到 `/mnt/c`（DrvFs）时 `git rm --cached` 因 **mmap 失败**（`unable to map index file: No such device`）而没生效 ⇒ 那次读数全是假的；改把副本放 `/tmp`（Linux 原生 fs）后一次成功。

⇒ 结论没变：**一条控制的红/绿只有在「它证明的是那条性质」时才成立**；控制本身也要有对照。

## 5 未取到 + 原因（第 19 条）

1. **真实 CI 上的运行**：不许 dispatch/rerun（纪律），本机也没有那些 run 的日志 ⇒ 所有读数都来自**本地同一个 bash** 对真实脚本字节的运行；与 CI 的等价性由 t105 §5 的口径（同一命令行、仓库根、LF 字节、PyYAML 存在）承担，本单复核了「真实树 exit=0」这一条。
2. **「没有任何 python3」那一支提示/`::error::`**：python3 与脚本必需的 `git/grep/sed/head` 同在 `/usr/bin`，无法在不破坏 PATH 的前提下让 `command -v python3` 失败；用「最小 PATH」的尝试会让脚本在找到 python3 之前以 127 退出（t105 已记为无效控制）。**另一支（有 python3、无 PyYAML）本轮取到了**（§1.3）。
3. **真实 e2e.yml 事故文件本身**：它在 HEAD 里已被 captain 修好（`5d3adfd`），且 `e2e.yml` **不在本单 inScope** ⇒ 事故形状用**自造副本**（`incident2.yml`）复现，绝不改真文件。
4. **`${{` 跨行表达式**：三份真实 workflow 里 0 例（`grep` 计数：ci 2、e2e 0、release 15，全部单行）⇒ 未构造跨行样本；实现把「跨行未闭合」判为 red（宁可漏一行也不放过非法形状），**未在真实文件上验证过跨行合法样本**。

## 6 不覆盖什么（第 19 条）

- **不检查工作流语义**：action 版本/pin、`if:`/矩阵/`env` 插值、permissions、triggers、并发语义。
- **表达式检查只做形状**：函数集合、上下文可用性（`steps.x` 是否存在）、类型、语义全不管；`success && 0 || 1` 这类**合法而恒一**的写法**不会**被本检查抓到（那靠结构，不靠解析）。
- **不覆盖 Windows job / 本机开发树**：guard 仍只挂在 `ci.yml` 的 `rust-linux` job；`e2e.yml` 有自己的 inline 守卫（未审）。
- **不覆盖 runner 镜像**：PyYAML「CI 里有」是自陈，本单未在镜像上验证。
- **不覆盖 panel/rust**（非本单 inScope）。

## 7 纪律与残留

- 只改两个脚本 + 本报告：`git diff --stat -- .github/workflows/scripts` = **2 files, +206, −12**；`.github/workflows/*.yml` 的 `git status` = **0**（未碰 t104 的两个文件）；`crates/`、`panel/`、`cli/` 零改动。
- 未 push / 未 dispatch / 未 rerun / 未 cancel / 未建 tag；本单不涉 `.rs` ⇒ 无 fmt/clippy 门禁。
- 临时物：`/tmp/t106*`、`/tmp/t106-index-*`、`%TEMP%\ruagent-t106{,b,c,d,e}` 内的 mini repo/索引副本/日志**全部删除**（每次运行末尾 `WORK exists: no`）；保留的只是 `%TEMP%\ruagent-rb\t106_probe*.sh`（5 个探针脚本，供复核）。
- 真实 `.git/index` 零改动：索引控制用的是 `GIT_INDEX_FILE` 指向的副本，控制前后 `git ls-files --error-unmatch .github/workflows/scripts/test-evidence.sh` 都 exit 0。
