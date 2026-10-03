# t144 收口 CI 红：t137 新增的 `#[ignore]` 进 skip 账目（mem-core）

- 任务：t144（kind=repair）；attempt `93bb7826-0e4f-45c0-b3a5-30b5fc102c32`
- 一句话：`found` 的真值是 **16**，多出来的那一条是 **`chat_event_carries_the_injection_budget_report`**（我 t137 的产物）；我选 **① RUN** —— 在第 7 步的 `--include-ignored` 调用里**点名**它、`covered` 7→8，并在**相同字节上**复现了这道门（新值 exit 0、旧值 exit 1）。

## 1 真值（我自己量的，不是转述）

`cargo test --workspace -- --ignored --list`（走 `scripts/cargo-team.ps1`，目标目录 = 团队共享 target；`--list` 只列不跑，但需要树能编译 ⇒ 全量编译 2m15s）⇒ 逐 target 的 `: test` 行：

```
injection_e2e.rs                 8   ← 其中新增的那一条：
                                        chat_event_carries_the_injection_budget_report
graph/live-after.rs              3
knowledge/retrieval-gold-copy.rs 1  + retrieval-gold-live.rs 1  = 2
store（lib.rs 1 + migrations.rs 2）3
                                 ---
found                          = 16
```

**`found = 16`**，与 captain 独立按 `^\s*#\[ignore` 逐文件数出的 16（8/3/2/3）**逐项吻合**；`expected`（改前）= `covered 7 + declared 8 = 15` ⇒ 差值**恰好一处**，就是我 t137 加的那条。CI 现场原话（run `37139111136`）：`found **16** … accounts for **15** (7 run …, 8 declared …)` + `::error::ignored-test count changed: found 16, this job accounts for 15. …`。

**我差点读错的一个数（记下来）**：第一次我用 `grep -cE ': test$'` 数自己 Tee 下来的清单，得到 **0** —— 因为 PowerShell 写出的文件是 **CRLF**，`$` 锚在 `\r` 前不匹配。`tr -d '\r'` 之后才是 **16**。这是与 CI（LF）不一致的**本地形状**，不是平台差异：**同一个数字在两种行尾下是 0 与 16**。

## 2 选择：① RUN（不是 DECLARE）

**它需要的东西，第 7 步恰好都有**：mock-agent 二进制（`cargo test --workspace` 就把它建在测试二进制旁边）、一个临时 root + sqlite、knowledge 句柄 —— 与那 7 条**同文件、同 `boot()` 夹具**的兄弟完全一样，而它们已经在 CI 上被 RUN（含 `chat_injects_memory_knowledge_and_wiki`，与我最接近的那条）。它**不需要** `~/.ruagent`、不需要 e5 模型、不需要任何 live COPY ⇒ 落进「CI 能满足」这一类，而不是 `declared_not_covered` 那一类（那 8 条的对象集是**真实历史/真实语料**，合成夹具测不了 —— 我没有动它们）。

## 3 改前 → 改后（逐行；5 处，**每一处都是计数/名字/文本，控制流零改动**）

| # | 行（改前） | 改后 | 理由 |
| --- | --- | --- | --- |
| 1 | `# THE SATISFIABLE ONE. The seven daemon instruments in` | `… The eight daemon instruments …` | 那句注释是在陈述「本步覆盖几条」；不改就成了一句会漂的话 |
| 2 | 第 7 步 `test-evidence.sh` 的名字表最后一行 `a_retried_run_carries_the_shared_retry_prefix`（无续行符） | 该行加 ` \`，**新增一行** `chat_event_carries_the_injection_budget_report` | 守卫的原话要求「RUN 必须**在 test-evidence 调用里点名**」——只改数字不点名正是它要防的 |
| 3 | `# RUN by the step above (7). Their names are demanded there.` | `(8)` | 同上，注释里的计数 |
| 4 | `covered=7` | `covered=8` | 账目的真值；`expected = covered + declared` 自动变成 16 |
| 5 | `echo "\| \`injection_e2e\`: 7 names \| RUN (…)"` | `8 names` | summary 表格里的具名行必须与实际点名数一致 |
| 6 | （windows job 的覆盖声明文本）`instrumentation of the daemon 7` / `those 7 names` | `daemon 8` / `those 8 names` | **同一个计数的第三处**；这是纯文本声明行，不是守卫步骤（windows 那个 job **没有** manifest 这一步——见 §6） |

**没动的**：`cargo test --workspace -- --ignored --list` 的调用形状、`found=$(grep -cE ': test$' …)` 的判据、`::error::` 那一行的文字与判据、第 7 步里**既有的 7 个名字**（只在其后追加第 8 个）、`declared_not_covered=8` 与那三类具名理由（graph 3 / knowledge 2 / store 3）。**也没有**为了让数字对上而删掉那条 `#[ignore]`。
`git diff --numstat .github/workflows/ci.yml` = **7 插入 / 6 删除**（6 行改写 + 1 行新增），与上表逐条对应。

## 4 本地复现守卫（逐字导出该步脚本；私有 `RUNNER_TEMP`）

harness：`%TEMP%\ruagent-t144\repro.sh`（Git Bash，继承 Windows 的 PATH ⇒ 能跑 `cargo`）。脚本用 `awk` 从 workflow 里**逐字**取该步的 `run:` 块（从 `set -uo pipefail` 到它成功时打印的最后一行），只做一件事：**去掉 YAML 的 10 空格缩进**；`$RUNNER_TEMP`/`$GITHUB_STEP_SUMMARY` 各给一个私有目录（`t65/` 按 CI 的既定形状预建 —— 那是**上一步**在 CI 里建的）。

| 运行 | 取字节 | 读数 |
| --- | --- | --- |
| RUN 1 | **当前文件**（`covered=8`） | `found **16** ignored test(s); this job accounts for **16** (8 run with --include-ignored, 8 declared…)` ⇒ **`skip accounting OK: 16 ignored = 8 run + 8 declared`**，**GREEN_EXIT=0** |
| RUN 2 | **`git show HEAD:.github/workflows/ci.yml`**（`covered=7`，旧字节） | `found **16** … accounts for **15** (7 run …)` ⇒ `::error::ignored-test count changed: found 16, this job accounts for 15. A new #[ignore] must be either RUN with --include-ignored (and its name demanded in the test-evidence call) or DECLARED here with its reason -- silently skipping is not an option (t65).` ⇒ **RED_EXIT=1** |

**两次取出的脚本正文逐行 diff 只有 3 行**（`# RUN by the step above (7|8)`、`covered=7|8`、表格 `7|8 names`）⇒ **脚本的控制流形状在两份字节之间完全相同**，红/绿之差只来自计数 ⇒ 这一条证明的是「门仍能红」，也证明了「我没有动它的判据」。

**RED 的方向性**：RUN 2 用的是 HEAD 的字节、跑的是**现在的树**（`found=16`）⇒ 它红的方式与 CI 现场**同一形状**（16 vs 15）⇒ 这次复现不是构造出来的假红，而是**复现了那次真红**。

## 5 门禁

| 命令 | 读数 |
| --- | --- |
| `bash .github/workflows/scripts/check-workflow-refs.sh`（本单 verify） | **exit=0**：`refs tracked + workflows parse`；其自身输出逐份列了 `ci.yml: parses as YAML (PyYAML)`（四份 workflow 全 parse） |
| 独立 YAML 解析（我自己的 python3 + PyYAML） | `parses as YAML: jobs = ['rust-linux', 'rust-windows', 'panel']` ⇒ 结构未被我改坏 |

## 6 未测 / 未覆盖（点名）

1. **CI 上的那次绿我没有观测到**：本单不许 push/dispatch/rerun，所以 ubuntu job 的结论是**由逐字导出的脚本在本地复现**（§4）得出的，不是从一次真实 run 读到的。要真观测需要一次推送后的 run（`Ignored-instrument manifest` 那一步打印 `skip accounting OK: 16 …`）。
2. **`test-evidence.sh` 对第 8 个名字的「demand」没有在本地跑**：它检查那个名字出现在 `--include-ignored` 的日志里。我没有在本地跑那一步（需要一次 8 条 ignored 测试的真实运行 —— 那正是第 7 步）。**它能不能通过，从两个已测事实推**：①测试在本地 `-- --ignored` 下 `1 passed`（t137 的读数）；②`test-evidence.sh` 的 demand 是「名字出现在日志里」，而 `cargo test` 的 per-test 行一定带名字。⇒ 这是**推断，不是读数**，明写在这里。
3. **windows job 的计数**：按 captain 的更正（他撤回了自己的假设），windows 那份 job **没有** manifest 步骤，所以**不存在**「平台不同 ⇒ 数字不同」这回事；我只改了它声明文本里的计数（第 6 行），**没有**给它加任何守卫。若有人在别处照着「两平台一致」去改，那是错的。
4. 本单**没有**跑 Rust 门（`cargo test`/`clippy`/`fmt`）：契约说明本单不需要，且我只改了 workflow + 报告；`crates/**` 一行未动（`git status` 可核）。
5. 我这次本地 `cargo` 用的是**仓库默认 target 目录**（`D:\rust_cache`，因为逐字脚本直接调 `cargo`，我没套 wrapper）；第一次量 `found` 时走的是 wrapper（`%TEMP%\ruagent-team-target`）。两者都只做 `--list`，**都没有碰活守护进程**（8787 = pid 14944 未引用、未起停任何进程），但口径不同这一事实写出来。

## 7 残留

- 改动：`.github/workflows/ci.yml`（7/6）+ 本报告。`crates/**`、`panel/**`、`scripts/**`、其余 workflow 一行未动。
- 未 push / dispatch / rerun / cancel / 建 tag。
- 保留的复核材料：`%TEMP%\ruagent-t144\`（`ignored-list.txt`（CRLF，含我那次 0 的读数）、`repro.sh`、`repro-full.txt`（两次运行的完整输出）、`step-green.sh` / `step-red.sh`（两份逐字导出的脚本）、`ci-head.yml`、`yamlcheck.py`）。
