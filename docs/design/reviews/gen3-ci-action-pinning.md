# gen3 CI 行动固定：`ci.yml`/`e2e.yml` 的 12 处 `uses:` 全部固定到提交 SHA（t112）

> 单号 t112（work）· 成员 `wiki` · attempt 1 · 2026-09-29 10:0x–10:3x（+08:00）
> inScope：`.github/workflows/ci.yml` · `.github/workflows/e2e.yml` · 本报告。**未碰** `release.yml` · `crates/` · `panel/`。
> 纪律：**未** push / dispatch / rerun / cancel / 建 tag；SHA 全部**现取**自 `gh api`（没有一个是凭记忆写的）。

## 0 一句话

把「**每次推送都执行**」的那条路径（`ci.yml` 8 处 + `e2e.yml` 4 处）固定到提交 SHA，形状与 `release.yml`（t100 的加固）**逐字一致**：`owner/action@<40 位 SHA> # vN (YYYY-MM-DD)`；同一个 action 在三份 workflow 里**必须是同一个 SHA**（本报告给了机械核对）。**并明确声明：固定 action ≠ 固定工具链** —— Rust 工具链仍然浮动（§3，独立决策）。

## 1 改前现场

| 文件 | 浮动 `uses:` | 行号（改前） |
| --- | --- | --- |
| `ci.yml` | **8** | `:44/:280/:398 actions/checkout@v4` · `:45/:281 dtolnay/rust-toolchain@stable` · `:48/:284 Swatinem/rust-cache@v2` · `:399 actions/setup-node@v4` |
| `e2e.yml` | **4** | `:31 checkout@v4` · `:32 rust-toolchain@stable` · `:33 rust-cache@v2` · `:222 upload-artifact@v4` |
| `release.yml` | **0**（8 处已固定） | t100 的加固，含注释与日期 |

**为什么这是缺陷**：仓库自己在 `release.yml:17-23` 写下了标准与理由 —— 「**Every `uses:` floated on a tag or branch** … `dtolnay/rust-toolchain@stable` is a **MOVING BRANCH** … so the same tag push could execute different code from one day to the next **with no change in this repository to review**」，并给出升级流程。同一条移动分支却在 **`ci.yml` 每次推送**都执行 ⇒ 供应链风险在「每次推送」的路径上比「发布」路径上更高（发布那条有 gate，推送这条没有）。

## 2 12 条固定表（action → SHA → 取证 → 日期）

取证命令（逐条现跑，输出即本表）：`gh api repos/<owner>/<repo>/commits/<ref> --jq .sha` / `.commit.committer.date`。

| # | 位置（改后） | 固定前 | 固定后 SHA | 取证（`gh api … --jq` 输出） | 日期 |
| --- | --- | --- | --- | --- | --- |
| 1 | `ci.yml:51` | `actions/checkout@v4` | `11d5960a326750d5838078e36cf38b85af677262` | `repos/actions/checkout/commits/v4` → `11d5960a…` (subject: `backport fixes to releases-v4 (#2524)`) | **2026-07-16** |
| 2 | `ci.yml:52` | `dtolnay/rust-toolchain@stable` | `6bed0761d98439e5a578e2877258200ad565ba87` | `repos/dtolnay/rust-toolchain/commits/stable` → `6bed0761…` (subject: `toolchain: stable`) | **2026-09-03**（**分支 HEAD**，见 §3） |
| 3 | `ci.yml:55` | `Swatinem/rust-cache@v2` | `6323deb102c322ba6fcbdcafc7e3dddab59af2b6` | `repos/Swatinem/rust-cache/commits/v2` → `6323deb1…` (subject: `2.9.2`) | **2026-08-06** |
| 4 | `ci.yml:295` | `actions/checkout@v4` | `11d5960a326750d5838078e36cf38b85af677262` | 同 #1（同一 action 同一 SHA） | 2026-07-16 |
| 5 | `ci.yml:296` | `dtolnay/rust-toolchain@stable` | `6bed0761d98439e5a578e2877258200ad565ba87` | 同 #2 | 2026-09-03 |
| 6 | `ci.yml:299` | `Swatinem/rust-cache@v2` | `6323deb102c322ba6fcbdcafc7e3dddab59af2b6` | 同 #3 | 2026-08-06 |
| 7 | `ci.yml:418` | `actions/checkout@v4` | `11d5960a326750d5838078e36cf38b85af677262` | 同 #1 | 2026-07-16 |
| 8 | `ci.yml:419` | `actions/setup-node@v4` | **`49933ea5288caeca8642d1e84afbd3f7d6820020`** | `repos/actions/setup-node/commits/v4` → `49933ea5…` (subject: `Bump @action/cache from 4.0.2 to 4.0.3 (#1262)`) | **2025-04-02**（**release.yml 里没有这个 action ⇒ 本条是新引入的固定**） |
| 9 | `e2e.yml:37` | `actions/checkout@v4` | `11d5960a326750d5838078e36cf38b85af677262` | 同 #1 | 2026-07-16 |
| 10 | `e2e.yml:38` | `dtolnay/rust-toolchain@stable` | `6bed0761d98439e5a578e2877258200ad565ba87` | 同 #2 | 2026-09-03 |
| 11 | `e2e.yml:39` | `Swatinem/rust-cache@v2` | `6323deb102c322ba6fcbdcafc7e3dddab59af2b6` | 同 #3 | 2026-08-06 |
| 12 | `e2e.yml:281` | `actions/upload-artifact@v4` | `ea165f8d65b6e75b540449e92b4886f43607fa02` | `repos/actions/upload-artifact/commits/v4` → `ea165f8d…` (subject: `Merge pull request #685 …`) | **2025-03-19** |

**没有把任何 ref 降级到更旧的版本**：所有 SHA 都是**今天**（2026-09-29）各 ref 指向的提交；其中 4 个（checkout / rust-toolchain / rust-cache / upload-artifact）与 `release.yml` **逐字相同**（§5 的机械核对）。

## 3 必须写明的声明：固定 action **≠** 固定工具链

`dtolnay/rust-toolchain` 固定到 `6bed0761…`（`stable` 分支 2026-09-03 的 HEAD）**只固定了「哪一版 action 代码在跑」**；这个 action 在运行时会去装**当时的 `stable` 工具链** ⇒ **Rust 版本仍然浮动**。**选了「保留 `stable`」**，理由：

1. `release.yml`（仓库自己写下的标准）**也是这么做的** —— 它固定 action 却保留 `stable`，本单若改成固定版本号，会让同一仓库两条路径对同一个 action 采取两种语义；
2. 本仓 `AGENTS.md` 写明 **MSRV = current stable**；固定工具链版本会把「随 stable 前进」变成一个**必须人工 bump** 的节奏，而 `fmt`/`clippy` 的观感会随版本变化 —— 那是**独立决策**，不是供应链固定的一部分；
3. 本单的目标是「**让执行的字节可复核**」，不是「冻结编译器」。

**为了让读者不可能误以为工具链也被固定**，我在两处 `uses:` 行尾的注释里加了一句：`# stable (branch head 2026-09-03; the TOOLCHAIN still floats -- see the report)`（`ci.yml:52/296`、`e2e.yml:38`），并在 §6 把它列为**未覆盖项**。**要固定工具链 ⇒ 另立单**（做法：`with: toolchain: 1.xx.y`，同时给 bump 节奏与 fmt/clippy 的复核口径）。

（对照：`actions/setup-node` 那一处**已经**带了 `node-version: 22` ⇒ 同一个 job 里 Node 版本是固定的、Rust 版本是浮动的 —— 这个不对称现在被显式写出来了，不再是隐含假设。）

## 4 判据（可机械核对）

| 判据 | 读数 |
| --- | --- |
| ① `ci.yml`/`e2e.yml` 浮动 `uses:` 计数 **= 0** | 扫描命令：逐行取 `^\s*-?\s*uses:` 后用 `@[0-9a-f]{40}` 命中判定 ⇒ **`ci.yml` uses=8, FLOATING=0** · **`e2e.yml` uses=4, FLOATING=0**（12 条逐行打印在验证输出里，全部带 SHA+注释） |
| ② `release.yml` **一处未改** | `git diff --stat -- .github/workflows/release.yml` ⇒ **空**；`git status --porcelain -- .github/workflows/release.yml` ⇒ **空** |
| ③ 守卫 + 可解析性 | `bash .github/workflows/scripts/check-workflow-refs.sh` ⇒ **exit=0**（`every executed path a workflow references is tracked` + 三份 `parses as YAML (PyYAML)`，其中也包含 t106 加固版的**表达式合法性**检查 ⇒ `${{ }}` 未被弄坏） |
| ④ `crates/**`、`panel/**` 零改动 | `git status --porcelain -- crates panel` ⇒ **空** |
| ⑤ 跨文件一致性（同一 action 同一 SHA） | 机械核对 7 个 action：`PASS actions/checkout → {11d5960a…}` · `PASS dtolnay/rust-toolchain → {6bed0761…}` · `PASS Swatinem/rust-cache → {6323deb1…}` · `PASS actions/upload-artifact → {ea165f8d…}` · `PASS actions/setup-node → {49933ea5…}`（仅 ci）· `PASS actions/download-artifact → {d3f86a10…}`（仅 release，未动）· `PASS softprops/action-gh-release → {3bb12739…}`（仅 release，未动） ⇒ **CONSISTENCY OK**（没有任何 action 出现两个不同 SHA） |
| ⑥ 改动量 | `ci.yml` + `e2e.yml` 合计 `38 insertions(+), 12 deletions(-)`；**算术可核**：本单只替换 **12 行**（+12/−12），其余 `+26/−0` 是**上一单 t111 的四条 `timeout-minutes` + 注释**（HEAD `5c0bc8f` 里还没有它们，所以 diff 把两单一起显示）⇒ `26+12=38`、`0+12=12` |

## 5 与 `release.yml` 既有标准的一致性（逐条）

- **形状**：`- uses: owner/action@<40hex> # <人类可读 ref> (<YYYY-MM-DD>)` —— 与 `release.yml:118/121/124/159/174/175/180` 逐字同形（我在两处 rust-toolchain 注释里多加了一句工具链警告，见 §3）。
- **同一个 action 同一个 SHA**：checkout `11d5960a…` · rust-toolchain `6bed0761…` · rust-cache `6323deb1…` · upload-artifact `ea165f8d…` —— 四处全部与 `release.yml` 相同（§4 ⑤ 的 PASS 行）。
- **唯一新增固定**：`actions/setup-node@49933ea5…`（`release.yml` 不用它）⇒ 不是"两处不一致"，而是三份 workflow 的并集里又多了一个已固定的 action。
- **升级流程同一套**（`release.yml` 头部已有的那条）：`gh api repos/<owner>/<repo>/commits/<ref> --jq .sha` → 同时改 `uses:` 与注释日期。

## 6 不覆盖什么（第 19 条）

1. **不固定 Rust 工具链**：见 §3（保留 `stable` 是独立决策；要固定须另立单并给 bump 节奏）。
2. **不固定 runner 镜像**：`runs-on: ubuntu-latest` / `windows-latest` 仍然浮动 ⇒ 镜像更新仍会改变执行环境（本代已经吃过一次：每次运行的 annotations 里有 Node 20 弃用告警）。固定到 `ubuntu-24.04` 之类是**更侵入**的决定，未做。
3. **不固定包管理器装出来的东西**：`sudo apt-get install -y protobuf-compiler`、`choco install -y protoc`、`npm ci`（后者靠 `panel/package-lock.json`）⇒ protoc 的**版本**仍由包源决定。
4. **不覆盖 `gh` CLI / runner 自带工具**（例如 t110/t111 判定步用的 `gh`、`bash`、`python3`）：它们是 runner 镜像的一部分。
5. **不覆盖 t111 的能力边界**：超时只点名 **job**，不指出挂在哪个命令/test（另立单）。
6. **没有真跑过**：本单不许 push/dispatch ⇒ 12 处固定**尚未被一次真实运行验证**；改动后**第一次 CI 运行**是它的第一批读数（若某个 SHA 写错，会在 `Set up job`/`Run …` 之前就报 "unable to resolve action" ⇒ 立刻可见）。
7. **不覆盖 GitHub 侧的 action 允许列表/策略**（例如 org 是否限制第三方 action）：未查、无权限查。
8. **未加 Dependabot/Renovate**（新增配置文件不在 inScope）⇒ 升级仍需人工 bump（流程见 §5）。

## 7 纪律回执

- **未** push / dispatch / rerun / cancel / 建 tag（`gh` 只用了 `api` 的**只读** GET 与 `--jq`）。
- 写入集合 = `.github/workflows/ci.yml` · `.github/workflows/e2e.yml` · 本报告；`release.yml`、`crates/`、`panel/` **零改动**（§4 ②④ 的读数）。
- 本单**没有**产生临时文件（SHA 取证用 `gh api` 直接读，不需要落盘）；未写活库、未启停 pid 79984。
- **e2e.yml 上同时带着 t110（已推）与 t111（未推）的痕迹**：本单只替换该文件的 4 行 `uses:`（不改它们的内容）⇒ 叠加后仍 `parses as YAML` 且 guard exit=0。
