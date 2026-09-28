# CI 证据链里无人复核的两个守卫脚本：只读审计（t105）（mem-core）

- 任务：t105（kind=work，只读审计）；attempt `d576a1dc-152a-47b5-8948-9d03c63fcc09`
- inScope：本报告；**未写入** `.github/workflows/**`、`crates/**`、`panel/**`（唯一写入 = 本文件）
- **被审计的字节（说清是哪一份）**：
  | 文件 | 工作树 sha256 | 行数 | 与 `HEAD`(0da0cb6) 的关系 |
  | --- | --- | --- | --- |
  | `.github/workflows/scripts/check-workflow-refs.sh` | `493175c7416ed3398433d3a3b5a3a97e297eeab606a8a2131475ac9ccad9989e` | 223 | **工作树比 HEAD 多 12 行**（同伴 t104 在途：PyYAML 缺失时的「跳过要说出来」提示 + pre-submit 提示行） |
  | `.github/workflows/scripts/test-evidence.sh` | `c6f1af78219076b3d756c764c511c511e4c343ee4814dc13b948513fa6fb2dd4` | 84 | 与 HEAD 相同（`git status` 未标记 M） |
  工作树 `git status --porcelain -- .github/workflows` = 3 项（`ci.yml`、`e2e.yml`、`check-workflow-refs.sh`，共 +167/−25），都是**同伴在途编辑**，不是本单产物；本单对 `.github/workflows/**` 只有读。（本报告引用行号时给「工作树」值，另注 HEAD 值。）
- **载体**：WSL bash 5.1.16 + git 2.34.1 + Python3/PyYAML 5.4.1（Windows 侧 `%TEMP%\ruagent-rb\t105_probe{,2,3,4}.sh`）；所有退出码都在 **bash 内部** `echo EXIT=$?` 取（C21：跨 WSL 的 `$?` 不可信）。

## 0 结论摘要（一句话一条）

1. 两个脚本的**每条判据都有能红的负控读数**（§2 全表，含任务单点名的 ①②③④）；**没有发现「打不红的装饰性判据」**。
2. 有 **7 条可复现的假红/假绿面**（§3），其中 2 条建议真修（`rc` 未校验整数 ⇒ 非数字 rc 绿；`checked: 0` 不算失败 ⇒ 「什么都没扫到」与「全都已跟踪」不可区分），另 5 条建议在脚本头/规格里**写明为已知缺口**（引号工作目录 ⇒ 假红；`.py`/根级引用/本地 action ⇒ 漏检；CRLF/ANSI 日志 ⇒ 假红；计数是文本 ⇒ 可被 print 伪造）。
3. 三处**「显示了但不断言」**（`panicked at`、`launched`、`ok+failed≠total`）与两处**「有不带读数的 exit 0」**（编译失败转 notice、attempt 2 未跑）已点名（§4）。
4. 本地跑与 CI 跑**等价**（同一命令行、同一工作目录、无 CRLF、PyYAML 都存在），差异逐条列出（§5）。

## 1 功能地图

### 1.1 `check-workflow-refs.sh`（223 行，工作树）

- **输入**：`argv` = 工作流文件列表；**无参数时默认 `.github/workflows/*.yml`**（脚本 34-37 行）。**输出**：逐引用一行报告（`文件:行 步骤 解析后路径 判定`）+ 汇总 `workflow path references checked: N, not tracked/missing: M` + `::error::` 注解（GitHub 会在 PR 上高亮）。
- **退出码**：`1` = 任一引用不是 tracked、或任一工作流不可解析、或某个被点名的文件不存在；`0` = 其余（**包括「一个引用都没扫到」**，见 §3 F-A2）。
- **它到底断言了什么**（全部在 `jobs:` 之后、注释被剥掉、`echo/printf/}`/`{`/`|`/`>` 开头的行按 prose 跳过）：
  1. 被点名/默认的工作流文件**必须存在**（56-59 行）。
  2. **未加引号**的、**像路径**的 token（`./…` 或含 `/` 且扩展名 ∈ `.sh .mjs .js .ts .mts .yml .yaml .json .toml .sql .ps1`）必须 `git ls-files --error-unmatch` 命中；否则报 `UNTRACKED (commit it with this workflow)`（文件在）或 `MISSING`（文件不在）（107-134 行）。解析规则：`./x` 去前缀；相对路径**按本步骤的 `working-directory` 解析**；含 `$ * { } =` 的 token 一律跳过。
  3. `working-directory:` 的值必须是**存在的目录且至少有一个 tracked 文件**（90-99 行）。
  4. 步骤边界：`- name:` / `- uses:` 会**重置** `working-directory`（76-88 行）。
  5. `- name:` / `- uses:` 的值里**未加引号的 `: `** 是非法 YAML ⇒ `::error::`（169-194 行）。
  6. 若可用：**PyYAML 全量解析**每个文件；失败 ⇒ `::error::… does not parse as YAML (PyYAML)`（196-205 行）。
- **它没有断言什么**：工作流的**语义**正确性（引用的脚本是否真能跑、action 版本是否存在/被 pin、`if:` 表达式、矩阵插值、`env:` 里的变量值、secrets）；`uses:` 的**本地 action 路径**（`uses: ./x` 被当作步骤边界跳过）；不含 `/` 的根级引用（`bash build.sh`）；扩展名不在白名单里的路径（`.py/.rb/.md/…`）；被引用的**目录**里的文件完整性；`$ {{ }}` 插值后的真实路径；并发/权限/触发器语义。
- **头部自陈的定位**：「**故意的下近似**（under-approximation），因为爱叫的哨兵会被关掉」——§3 的假绿面与该定位一致，但**「下近似」不等于「可以静默地什么都不扫」**（F-A2）。

### 1.2 `test-evidence.sh`（84 行，与 HEAD 同字节）

- **输入**：`<label> <log> <exit-code> [要求出现的测试名 …]`（`${1:?}`/`${2:?}`/`${3:?}` 三重必填）。**输出**：Markdown 表格（6 行：`Running ` 启动数 / `test result:` 上报数 / `ok` / `FAILED` / `panicked at` / exit code），`tee -a` 到 `$GITHUB_STEP_SUMMARY`（未设则 `/dev/null`）；失败行的原文回显；绿时一行 `… N target(s) ok, 0 failed, P panic site(s), exit 0`。
- **退出码**：`1` 当且仅当 ① 日志**不可读**；② `test result:` 行数 = 0（**空跑不是绿**）；③ 传入的 rc ≠ 0 **或** `test result: FAILED` 行数 > 0；④ 任一被要求的测试名没有以 `test <name> ... ok` 出现在日志里。否则 `0`。
- **它到底断言了什么**：**「有几个 target 报了结果」+「调用方给的退出码」+「被点名的测试真的报 ok」**。计数口径：`^test result:`（行首锚定、大小写敏感）、`^ *Running|^ *Doc-tests `（两种自报形态）、`^test result: ok`、`^test result: FAILED`、`panicked at`（不锚定）；`grep -c` 的 0 命中退出码 1 由 `|| true` + `${var:-0}` 吸收（35-43 行，实测 E12）。
- **它没有断言什么**：`panicked at` **只显示不判**（E14：`test result: ok` + panic 行 ⇒ 绿）；`launched` 与上报数**不比对**（E17：启动 2、上报 1 ⇒ 绿）；`ok + FAILED ≠ total` 不检查；**计数是文本匹配**：任何在**行首**打印 `test result: ok …` 的东西都会被数成一个上报的 target（E11/E18）；不校验 `rc` 是整数（F-A1）。

## 2 负控读数（每条判据一条能红的读数）

**任务单点名的四条**：

| # | 控制 | 命令 | 读数 |
| --- | --- | --- | --- |
| ① | **把被引用的路径从索引里拿掉**（用 `.git/index` 的**副本** + `GIT_INDEX_FILE`，真实索引零改动） | `GIT_INDEX_FILE=$TMP/index git rm --cached .github/workflows/scripts/test-evidence.sh` 后跑 guard | temp index `ls-files --error-unmatch` exit=**1**、真实索引 exit=**0**；guard **exit=1**，5 处 `UNTRACKED (commit it with this workflow)`（`ci.yml:174/199/357/376/390`），原文：`::error::1 workflow path reference(s) are not tracked, or a workflow does not parse. Commit the missing files in the SAME commit as the workflow that executes them (t65/F1: an untracked step script turns the pipeline red, not merely weaker).` |
| ② | **未加引号的 `: `** | 合成 `bad.yml` 里 `- name: Guard: workflow-referenced paths must be tracked` | guard **exit=1**：`  bad.yml:6 UNQUOTED ': ' in a plain scalar -> invalid YAML:\|      - name: Guard: workflow-referenced paths must be tracked\|` + `::error::1 workflow line(s) put an unquoted ': ' inside a name/uses value. GitHub rejects the whole file at parse time and the job never starts. Quote the value (t66).` + `::error::… does not parse as YAML (PyYAML)`；**独立复核**：PyYAML 对该文件 `exit=1`（真不许解析），对 `good.yml` `exit=0`（能解析） |
| ③ | **0 个 reporting target 的日志** | 空日志 / 只有 `error[E0425]` 的编译日志 | 两者都 **exit=1**，原文：`::error::empty reported 0 targets -- an empty run is not a green (t65)`；表里 `test result:` = 0 |
| ④ | **健康日志** | 2 个 target（`Running unittests` + `Doc-tests`）都 `ok` | **exit=0**，表 `Running 2 / test result: 2 / ok 2 / FAILED 0 / panicked 0 / exit code 0`，末行 `healthy: 2 target(s) ok, 0 failed, 0 panic site(s), exit 0` |

**其余判据的负控**（逐条）：

| 判据 | 控制 | 读数（红/绿 + 原文要点） |
| --- | --- | --- |
| 工作流文件必须存在 | `check-workflow-refs.sh .github/workflows/nope.yml`（真实树） | **exit=1**：`::error::check-workflow-refs: no such workflow '.github/workflows/nope.yml'` |
| 引用必须 tracked（**MISSING** 分支） | 合成 `missing.yml` 引用 `gone/missing.sh` | **exit=1**：`… gone/missing.sh  MISSING` + 同一条 `::error::1 workflow path reference(s)…` |
| 引用必须 tracked（**UNTRACKED** 分支） | 合成 `untracked.yml` 引用 `here/untracked.sh`（存在、未 `git add`） | **exit=1**：`… here/untracked.sh  UNTRACKED (commit it with this workflow)` |
| `working-directory` 必须存在且非空 | 合成 `workdir.yml`：`working-directory: nowhere` | **exit=1**：`… nowhere/   MISSING/UNTRACKED` |
| **解析按本步骤 workdir 解析**（解析规则本身） | `ok/sub/x.sh` tracked，步骤 `working-directory: ok` + `run: bash sub/x.sh` | **exit=0**：`… ok/sub/x.sh  tracked` ⇒ 只有按 workdir 解析才会绿（按根解析会红） |
| **workdir 在步骤边界重置** | 同一 token 在第二个**无 workdir** 的步骤里再出现 | **exit=1**：第一处 `ok/sub/x.sh tracked`、第二处 `sub/x.sh MISSING` ⇒ 重置是真的 |
| PyYAML 半检查（工作树 t104 提示） | PATH 首部放一个 `python3` 桩（`command -v python3` 命中、`import yaml` 失败） | guard **exit=0** 且打印 `PARSE CHECK SKIPPED (python3 is present but has no PyYAML module): the full-YAML half of this guard did NOT run.`（**HEAD 版本此处无提示 ⇒ 静默跳过**，见 §3 F-A9） |
| 日志不可读不是绿 | 不存在的日志路径 | **exit=1**：`::error::missing: evidence log '…' is unreadable -- a run whose log we cannot read is not a green (t65)` |
| `FAILED` 行 | `test result: FAILED. 1 passed; 1 failed` + `panicked at`，rc=0 | **exit=1**（表 `FAILED 1`） |
| rc≠0 | 健康日志 + rc=1 | **exit=1**：`::error::rc1: exit code 1 with 0 failing target(s)` |
| 被点名的测试必须报 ok | 日志含 `test instrument_ran ... ok` | 点名它 ⇒ **exit=0**；点名 `instrument_never_ran` ⇒ **exit=1**：`::error::namedmissing did not run instrument_never_ran -- a name that must be measured never reported ok` |
| `grep -c` 0 命中不炸 | `grep -c '^test result: FAILED' healthy.log` | 打印 `0`、**exit=1**；脚本用 `\|\| true` + `${var:-0}` 吸收 ⇒ 健康日志那行显示 `FAILED 0` 而不是空（E12/E13） |
| CI 专属：`GITHUB_STEP_SUMMARY` | 设成临时文件后跑一次 | **exit=0**，summary 文件写了 **227 B**（同一张表），stdout 仍有表（`tee -a`） |

**一次「控制被混淆」的自陈**：第一版「缩进的 `jobs:` ⇒ 扫描器进入不了 jobs」的控制 **exit=1**，但红的原因不是引用漏检而是 **YAML 非法**（PyYAML 分支报错）⇒ 该控制证明了另一件事。改用**能通过 YAML 解析**的 `Jobs:`（大写）与 `jobs :`（冒号前空格）重测，才拿到真正的「扫到 0 个引用仍绿」读数（§3 F-A1）。**这一条留在账上，是因为「控制红了」不等于「控制对了」。**

## 3 假红 / 假绿面（逐条给实例）

**假绿**（本该红却绿）：

| # | 面 | 实例与读数 |
| --- | --- | --- |
| **F-A1** | `test-evidence.sh` 把非数字 rc 当「不为非零」 | `bash test-evidence.sh strrc healthy.log boom` ⇒ stderr `ev.sh: line 68: [: boom: integer expression expected`，**exit=0**（末行 `strrc: 1 target(s) ok, … exit 0`）。**这是本审计里最值得修的一条**：与 t65 修过的「日志不可读 ⇒ 不是绿」同形，只是换成了 rc。建议：`[[ "$rc" =~ ^[0-9]+$ ]] \|\| { echo "::error::$label: '$rc' is not an exit code"; status=1; }`。（注：**空** rc 会被 `${3:?}` 拦住：`bash … ""` ⇒ bash 报 `3: exit code` + **exit=1**，是红的但不是注解。） |
| **F-A2** | `check-workflow-refs.sh` 把 `checked: 0` 当成功 | 「什么都没扫到」与「全部已跟踪」不可区分。4 个实测实例（全部 **exit=0** + `every executed path a workflow references is tracked`）：`Jobs:`（大写，YAML 合法）；`jobs :`（冒号前有空格，YAML 合法）；只写 `run: bash "here/untracked.sh"`（加了引号）；只写 `python3 tools/gen.py`（`.py` 不在白名单）；根级 `run: bash rootscript.sh`（无 `/`）；没有 `jobs:` 的文件。真实树 P1 读数是 `checked: 15, not tracked/missing: 0` ⇒ 今天不空转，但**判据本身缺一条下界**。建议：`refs == 0` 时至少 `::warning::`（CI 里可 `::error::`）。 |
| **F-A7** | 「证据」是文本计数，不是解析 | 一份**内容**恰好是 `test result: ok. 99 passed; 0 failed; …`（行首）的文件 ⇒ **exit=0**（被当成 1 个上报 target）。列首锚定确实挡住了缩进/引用里的假行（E18：缩进的 `the script printed: test result: ok …` 未被计数）⇒ 伪造需要「行首原样」。CI 里这一面由**调用方传入的真实 rc** 兜底（rc 是 cargo 的退出码，打印盖不住它）⇒ 严重度 info，建议在头部写明「本脚本的计数可被行首文本伪造，rc 才是锚」。 |
| **F-A9** | HEAD 字节里 PyYAML 半检查**静默不做** | HEAD(`0da0cb6`) 的脚本没有 `else` 分支 ⇒ 无 python3/PyYAML 时**没有任何提示**就少了整个解析半检查（工作树 t104 已加提示，实测能打印）。⇒ 今天（HEAD）在缺 PyYAML 的机器上，「工作流可解析」是**未被检验的真**。建议：`$CI` 环境下缺 PyYAML 直接 `::error::`（CI 镜像有 PyYAML，缺了就是环境坏了）。 |
| **F-A10** | 三处「不带读数的 exit 0」 | `ci.yml` 的工作树 160-173 / 345-356 行：日志里 0 个 `test result:` **且**有编译错误 ⇒ 写一行 notice、**`exit 0`**；386-389 行：attempt 2 没跑 ⇒ **`exit 0`**。三处都**明确打印**了「这不是测试结论」，且红由**别的步骤**（build/clippy）负责 ⇒ 本身不是假绿，但它们的绿**不是读数**：若上游那一步改成 `continue-on-error`、或被重排，这三处会立刻变成假绿。建议在脚本/规格里写明「本步骤的绿依赖 X 步骤红」。 |

**假红**（本该绿却红）：

| # | 面 | 实例与读数 |
| --- | --- | --- |
| **F-A3** | `working-directory` 的值**不剥引号** | `working-directory: "ok"`（`ok/` 存在且 tracked，YAML 合法）⇒ `… "ok"/   MISSING/UNTRACKED` ⇒ **exit=1 假红**。注意引号不对称：**token 扫描是引号感知的**（引号内的路径被跳过），而 workdir 值是整段捕获。建议：剥掉外层引号再判。 |
| **F-A5** | CRLF 日志 + 被点名的测试名 | 日志行以 `\r\n` 结尾时，`^test … \.\.\. ok$` 不匹配 ⇒ `::error::crlf did not run instrument_ran` + **exit=1**，而同一份日志的 `test result:` 计数（2 行/1 target）与 rc 都是绿的 ⇒ **假红**。建议：`tr -d '\r'` 或在模式里允许 `\r?$`。 |
| **F-A6** | ANSI 着色日志 | 行首带 `\e[32m` 的 `test result: ok …` 不被 `^test result:` 命中 ⇒ 表里 `test result: 0` ⇒ `::error::ansi reported 0 targets -- an empty run is not a green (t65)` + **exit=1 假红**（`CARGO_TERM_COLOR=always` 或 `--color always` 即可触发）。建议：剥 ANSI 或锚定前允许 ESC 序列。 |
| **F-A4** | 白名单/形态漏检（同时是假绿） | `.py`（`python3 tools/gen.py`）、根级无斜杠（`bash rootscript.sh`）、本地 action（`uses: ./nonexistent-local-action`，实测 `checked: 1` 只数了 `run:` 那行、**exit=0**）、引号包裹的引用 ⇒ 全都**不检查**。头部自陈「下近似」涵盖这一族，但**本地 action 路径**是 GitHub 自己会报错的硬引用，建议补 `uses: ./…`。 |

## 4 「显示了但不断言」与「不覆盖什么」

**显示了但不断言**（点名，避免读者把它们当判据）：`panicked at` 计数（E14：`ok` 的 target + panic 行 ⇒ **exit 0**，只有 `should_panic` 需要这种形状，但脚本无法区分）；`Running/Doc-tests` 启动数与上报数**不比对**（E17：2 启动 / 1 上报 ⇒ **exit 0**）；`ok + FAILED ≠ total` 不检查；表格里的 `exit code` 只是回显。

**不覆盖什么（第 19 条）**：
1. **不检查工作流语义**：引用的脚本内部是否成立、action 版本是否需要 pin、`if:`/矩阵/`env` 插值、permissions、triggers —— 只检查「引用的仓库路径是否 tracked」与「文件是否可解析」。
2. **不覆盖 Windows job 与本机开发树**：本单读的 guard 只挂在 `ci.yml` 的 `rust-linux` job（工作树 `ci.yml:63`）；`e2e.yml` **没有**调用这两个脚本（它有自己的 inline 守卫，如 `Entry-point guard`，本单未审）。
3. **不覆盖 `panel/**` 与 Rust 侧**（非本单 inScope）。
4. **不覆盖 runner 镜像本身**：PyYAML「CI 里有」是脚本头与 t104 注释的自陈，本单**未在 CI 镜像上验证**（只在本机 WSL 验证 PyYAML 5.4.1 存在 ⇒ 解析半检查确实运行）。
5. **不覆盖 GitHub 端的真实行为**：本单没有触发任何 workflow run（不许 `dispatch/rerun/cancel`），也没有在 Actions 上验证 `::error::` 的呈现。

## 5 与真实 CI 的一致性

| 项 | CI | 本地（本审计） | 等价？ |
| --- | --- | --- | --- |
| guard 命令行 | `ci.yml:63`（工作树；HEAD 同号）`run: bash .github/workflows/scripts/check-workflow-refs.sh`（**无参数** ⇒ 默认扫 3 个 yml） | `bash .github/workflows/scripts/check-workflow-refs.sh`（默认与单文件两种都跑过） | **等价**（P1：`checked: 15`） |
| evidence 命令行 | `ci.yml` 工作树 174 / 199 / 357 / 376 / 390（HEAD：129 / 149 / 261 / 282 / 292；captain 记的 175/200/358/377/391 是另一时刻的编号）——`bash … "label" "$log" "$(cat "$rcfile")" [名字…]` | 同形（临时日志 + 显式 rc + 名字） | **等价**（命令行逐字相同） |
| 工作目录 | 仓库根（checkout 后默认） | 仓库根 | **等价** |
| shell | Actions 默认 `bash -e {0}`（本步 `run:` 里的 shell）；被调脚本是**子进程**，自带 `set -uo pipefail`（guard）/ `set -u`（evidence） | WSL bash 5.1.16 直接跑脚本本体 | **等价**（脚本自带的选项一致；`-e` 不经 SHELLOPTS 传给子进程这一点两边相同） |
| 行尾 | Linux checkout = LF | 实测 `ci.yml`/guard **0 行含 CR**、`core.autocrlf` 未设、`.gitattributes` 存在 ⇒ LF | **等价**（所以 F-A5 是**CI 之外**输入的风险，不是 CI 本身的问题） |
| PyYAML | ubuntu-latest 有（自陈） | 本机 WSL PyYAML 5.4.1 | **等价**（证据：P2 打印 `.github/workflows/ci.yml: parses as YAML (PyYAML)`） |
| `$GITHUB_STEP_SUMMARY` | 有（写 job summary） | 未设 ⇒ `/dev/null`（另做一次设成文件的对照，227 B） | **等价**（两次都取过读数） |
| 证据日志的来源 | `cargo test … 2>&1 \| tee log` + `PIPESTATUS[0]`（ci.yml:190-197） | 合成日志 | 形状等价；**真实日志未取**（见 §6） |

## 6 未取到的读数（原因写清，不静默）

1. **真实 CI 日志上的 evidence 读数**：本机没有 commit `0da0cb6` 的任何 Actions run 日志（不许 `dispatch/rerun`，也没有线上日志落盘）。§2 的 evidence 读数全部来自**合成日志**；其形状取自 `cargo test` 的真实格式（`     Running unittests …` / `test result: …`）以及 ci.yml 里的调用形状。
2. **「没有任何 python3」那一半 t104 提示**（`no python3 on PATH`）：本机 python3 与脚本必需的 `git/grep/sed/head` 同在 `/usr/bin`，无法在不破坏 PATH 的前提下让 `command -v python3` 失败（第一次尝试用一个「只有符号链接的最小 PATH」⇒ 脚本在找到 python3 之前就以 **127** 退出，属**无效控制**，已弃用）。**另一支**（有 python3、无 PyYAML）**取到了**（§2 最后一行）。
3. **`.github/workflows/**` 的 HEAD 与工作树差异对 CI 的影响**：工作树多出的 12 行（t104）**尚未提交**，所以「CI 上跑的是哪一版」取决于下一次推送；本报告对**两版**都给了定位（§1.1/§3 F-A9）。

## 7 建议（不建议在本单做；脚本不在 inScope）

| 优先级 | 建议 | 位置 |
| --- | --- | --- |
| **高** | 校验 rc 是整数，否则 `::error::` + 红（F-A1） | `test-evidence.sh:68` |
| **中** | `refs == 0` 时报警/报错：区分「全 tracked」与「没扫到」（F-A2） | `check-workflow-refs.sh:207` 之前 |
| 中 | PyYAML 缺失时若 `$CI` 已设 ⇒ 直接红（F-A9，CI 镜像有 PyYAML） | `check-workflow-refs.sh:196` 的 `else` |
| 中 | CRLF 与 ANSI 归一化（F-A5/F-A6） | `test-evidence.sh:74` 与 35 行的模式 |
| 低 | 剥 workdir 值的引号（F-A3）；补 `uses: ./…` 与 `.py`、根级引用（F-A4） | `check-workflow-refs.sh:90`、`39-45` |
| 低 | 头部写明「计数是文本、可被行首打印伪造，rc 才是锚」与「本步骤绿依赖 X 步骤红」（F-A7/F-A10） | 两个脚本头 + `ci.yml` 注释 |

## 8 纪律与残留

- **只读**：本单唯一写入 = 本报告。对 `.github/workflows/**` 只有 `read`/`git show`/`grep`；`git status --porcelain -- .github/workflows` 的 3 项 `M` 是**同伴在途编辑**（+167/−25），不是本单产物；真实 `.git/index` 未被改动（控制①用的是 `GIT_INDEX_FILE` 指向的**副本**，实测真实索引在控制前后都仍能 `ls-files --error-unmatch` 命中）。
- **没有** `dispatch/rerun/cancel` 任何 run、**没有**建 tag、**没有** push。
- **残留读数**：4 个探针脚本 + 1 个 `python3` 桩留在 `%TEMP%\ruagent-rb\`（供复核，非仓库内容）；4 个临时工作目录（`ruagent-t105{,b,c,d}`）与临时 repo/索引副本**全部删除**，命令输出均为 `WORK exists: no`；`shim` 目录见 §6-2。
