# t147 补 C42 的覆盖声明：`rust-windows` 的绿不意味着四类守卫通过（mem-core）

- 任务：t147（kind=repair）；attempt `0b61c28e-de4c-4907-bf6d-c6aff6722b09`
- 一句话：把「**这个 job 里没有跑什么**」逐条写进 `rust-windows` 的 `Coverage declaration`，让「某个 job 是绿的」不再被读成「它通过了某项检查」。**只加宣言、行为零改动**（步骤数 13/9/6 前后不变）。

## 1 先读事实，再写字（读数来自回读 + PyYAML 解析，不来自印象）

`ci.yml`：改前 sha256 `08a26c5c…`（27736 B）→ 改后 `d3eedcd1…`（29855 B）。三个 job 的**步骤表**（PyYAML `safe_load`，逐 step `name`）：

| job | runs-on | `if:` | 步骤数 | 步骤（按序，只列有名步） |
| --- | --- | --- | --- | --- |
| `rust-linux` | ubuntu-latest | （无） | **13** | Install protoc · **Guard: workflow-referenced paths must be tracked** · Coverage declaration · **Format** · **Clippy** · **Guard against nested-Result misjudgement** · **Test (default run -- skips stay countable)** · **Test evidence (per-target counts + exit code)** · **Ignored instruments CI can satisfy (daemon / mock-agent present)** · **Ignored-instrument manifest** |
| `rust-windows` | windows-latest | `github.event_name == 'push' && github.ref == 'refs/heads/main'` | **9** | Install protoc · Coverage declaration · **Test attempt 1** · **Test attempt 1 evidence** · **Test attempt 2 (only for the known build race)** · **Test attempt 2 evidence** |
| `panel` | ubuntu-latest | （无） | **6** | Coverage declaration · Install · **Audit self-test** · Typecheck + build (with counts) |

**`rust-windows` 里没有的（对照 ubuntu 的 `rust` job，逐条）**：`Guard: workflow-referenced paths must be tracked`（linux 第 5 步）· `Format`（7）· `Clippy`（8）· `Guard against nested-Result misjudgement`（9）· `Test evidence (per-target counts + exit code)`（11，windows 用它自己的两次 attempt evidence 替代）· `Ignored instruments CI can satisfy`（12）· `Ignored-instrument manifest`（13）。⇒ **守卫套件（Format / Clippy / nested-Result / ignored-instrument 记账）整个不在这个 job 里**，另外 workflow-refs 守卫也不在。

**`rust-windows` 实际有的（并且它确实做了的事）**：`Test attempt 1` = `cargo test --workspace --no-fail-fast` + `Test attempt 1 evidence`（逐 target 计数 × 该次退出码）；`Test attempt 2 (only for the known build race)` **只在 attempt 1 没有测试级失败时**才重跑 + `Test attempt 2 evidence`。

**顺带读到的两条「只在一个 job 里跑」（合同要求如实写清）**：`Test attempt 2` 只在 `rust-windows`；`panel` 的 `Audit self-test` 只在 `panel`（该 job 第 5 步，我已在宣言里点名）。

## 2 改前 → 改后（逐行）

diff（`git diff -U0 .github/workflows/ci.yml`）= **14 插入 / 1 删除**，全部在 `rust-windows` 的 `Coverage declaration` 的 `run:` 块**内部**：

- **新增 14 行**（全是 `echo`）：① 一句总纲「**A green here is NOT a green on the guard suite**」+ 实测步骤数（13 vs 9）；② **Format**（linux 第 7 步，本 job 不跑）；③ **Clippy**（第 8 步）；④ **nested-Result 守卫**（第 9 步）；⑤ **ignored-instrument 记账**（`Ignored instruments CI can satisfy` 第 12 步 + `Ignored-instrument manifest` 第 13 步）；⑥ **workflow-referenced paths 守卫**（第 5 步）；⑦ 现场那一条：run `37139111136` 在 `Ignored-instrument manifest` 上红（`ignored-test count changed: found 16, this job accounts for 15`）**而本 job 绿（10m12s）** ⇒ 「看起来像『skip 记账通过了』，实际是『skip 记账没跑』」；⑧ **本 job 确实覆盖什么**（两次 attempt + 各自 evidence 的语义）；⑨ `panel` 的 `Audit self-test` 也不在这里。
- **删除 1 行**：原来那句 `anything only a non-main push would catch: this job does not run on pull requests.` —— 被**重写**为「runs on main pushes only」（与上面读到的 `if:` 逐字一致：main push 且 ref=main），并与新增的 `Audit self-test` 行并列。

**逐行分类**：注释 **0** 处 · 报文（`echo`）**15** 处（14 新增 + 1 改写）· **控制流 0 处**。

**控制流 0 处的证据**（不是声明，是读数）：改后 PyYAML 复核 ⇒ jobs 仍是 `['rust-linux','rust-windows','panel']`，步骤数仍是 **13 / 9 / 6**，`rust-windows` 的 `runs-on='windows-latest'` 与 `if="github.event_name == 'push' && github.ref == 'refs/heads/main'"` **逐字未变**；没有新增/删除/移动任何 step，没有碰 `timeout-minutes`/matrix。

## 3 门禁与块级复现

| 命令 / 探针 | 读数 |
| --- | --- |
| `bash .github/workflows/scripts/check-workflow-refs.sh`（本单 verify） | **exit=0**，38 s：`workflow path references checked: 22, not tracked/missing: 0` + 四份 workflow 全 `parses as YAML (PyYAML)` + `every executed path a workflow references is tracked` |
| 我的 PyYAML 复核 | 改后 `jobs`/步骤数/`if:`/`runs-on` 见 §1、§2（形状不变） |
| **抽出该 step 的 `run:` 块并在本地真跑**（私有 `GITHUB_STEP_SUMMARY`） | 抽出 21 行（`sha256=e1000fa1bed7cd84…`），`bash -n` **exit 0**，`RUN_EXIT=0`，summary **2376 B**；逐条存在性检查：`**Format**`/`**Clippy**`/`nested-Result`/`ignored-instrument accounting`/`workflow-referenced paths`/`NOT a green on the guard suite`/`What this job DOES cover`/`Audit self-test` **全部 present** |

**两次「无读数」我没有当成红（点名）**：头两次跑这条门都**挂住**（300 s 超时），原因**不在树**：Git Bash 下 `python3` 解析到 `C:\Users\19410\AppData\Local\Microsoft\WindowsApps\python3.exe`（商店别名，非交互下会阻塞），而该脚本第 479 行正是 `command -v python3 && python3 -c 'import yaml'`。改用**真有 `python3`+PyYAML 的 WSL**（并把 stdin 关掉、加 `timeout 150`）后 38 s 拿到 exit 0。⇒ **同一条命令在不同解释器解析下是「无读数」还是「exit 0」**，这本身是一条要写下来的方法学（与「一个绿只认证它跑过的字节与命令」同族）。此前 CI 上它一直是绿的（ubuntu runner 的 python3 是真的）。

## 4 未测 / 未覆盖（点名）

1. **windows job 自身的绿我没有观测到**（不许 push/dispatch）：我验证的是**宣言文本**（抽出脚本实跑 + 内容存在性）与**workflow 形状**，**不是**一次 windows runner 上的运行。
2. **「本 job 也构建 mock-agent」这句话是继承来的**：它原本就在这一行里（我只改了 7→8 的计数），我没有在 windows 上重新证明它；能说的是同一份 `cargo test --workspace --no-fail-fast` 与 ubuntu 的 `cargo test --workspace` 会建同一批二进制，但**这是推理不是读数**。
3. 我**没有**把「ubuntu 的 6 类守卫」逐条写成 CI 文档（只在宣言里点名四类 + workflow-refs）；`Test evidence` 那一类在 windows 上**以另一种形状存在**（两次 attempt 各自的 evidence），所以我没有把它写成「缺失」，只在 §1 如实分开列。
4. 本单不跑 Rust 门（契约：不需要，且 t145 正在 `crates/daemon/src/chat.rs` 开**已宣告窗口**，整包会撞到它 —— 我没有跑整包）。

## 5 残留

- 改动：`.github/workflows/ci.yml`（14/1，全部是 `echo` 报文）+ 本报告。`crates/**`、`panel/**`、`scripts/**`、其余三份 workflow 一行未动。
- 未 push / dispatch / rerun / cancel / 建 tag。
- 复核材料：`%TEMP%\ruagent-t147\`（`shape.py`（PyYAML 步骤表）、`verify4.sh`（干净抽取+实跑）、`win-decl.sh`（抽出的 21 行）、`summary4.md`（2376 B 的宣言成品）、`verify.sh`/`verify2.sh`/`verify3.sh`（三次失败/半成品尝试，留着当 §3 那条方法学的证据）。
