# gen3 发布路径加固（t100 / t95 T-6）：最小权限 + 固定 SHA + 发布前门禁

> 单号 t100（repair round 1）· 成员 `wiki` · attempt 1 · 时间窗 2026-09-29 03:5x–04:2x UTC
> 上游：`docs/design/reviews/gen3-audit-panel-tooling.md`（t95 的 T-6）
> inScope：`.github/workflows/release.yml` + 本报告。**未改** `ci.yml` / `e2e.yml` / `design-audit.yml`（分别由 t66 / t99 拥有）。
> **全程未触发发布**：没有建 tag（`git tag --list` 仍是 1 个既存的 `v0.1.0`）、没有 `gh workflow run`/`gh run` 的写命令、没有 push。所有 GitHub 侧读数来自只读命令（`gh workflow view` / `gh run list` / `gh run view` / `gh api …/commits/<ref>`）。

## 0 一句话

给发布路径上了三把锁：**默认最小权限**（工作流默认 `contents: read`，只有真正创建 release 的那个 job 升到 `contents: write`）· **六个 action 全部固定到 commit SHA**（其中 `dtolnay/rust-toolchain@stable` 是移动分支）· **发布前必须有绿裁决**（新 `gate` job 要求这个 commit 已经有成功的 CI run，`build` 与 `release` 都 `needs:` 它 ⇒ 红提交打 tag 不会发布）。

## 1 现场（改前读数，全部只读取得）

| 面 | 改前 | 证据 |
| --- | --- | --- |
| 权限 | `release.yml:7-8` 是**工作流级** `permissions: contents: write` ⇒ 每个 job 都拿写权限（含只跑 `npm ci` / `choco install` / `cargo build` 的 `build`） | `git show HEAD:.github/workflows/release.yml` |
| 版本固定 | 6 个 `uses:` **全部浮动**：`actions/checkout@v4`、`dtolnay/rust-toolchain@stable`（**移动分支**）、`Swatinem/rust-cache@v2`、`actions/upload-artifact@v4`、`actions/download-artifact@v4`、`softprops/action-gh-release@v2` | 同上 |
| 发布前门禁 | `release` job（`:72-86`）`needs: build`，**不跑任何测试**；`build` 也不跑测试 ⇒ 红提交打 tag 照样发 | 同上 |
| 历史 run（远端，只读） | `gh run list --workflow=release.yml --limit 3` → **2 个 run**：`completed success 34849294052 2026-09-14T13:26:49Z`（tag v0.1.0）、`completed failure 35240185033 2026-09-17T15:25:53Z`（tag v0.1.0）；失败点在 `Create release` 步骤，日志里的错误行是 `##[error]<!DOCTYPE html>`（2026-09-17T16:35:06Z）。**该失败的具体成因超出本单证据**（需要完整响应体/仓库设置），只登记 | `gh run view 35240185033 --json jobs` + `--log-failed` |
| **本单最重的背景读数** | **CI 从来没有绿过**：`gh run list --workflow=ci.yml --limit 100` → `{total:100, success:0, failure:13, newest_success:"none"}`；最新一个 run（36272344732，2026-09-26）在 `Rust (ubuntu)` 失败（`##[error]Process completed with exit code 1`，日志里出现 `.map_err(DbError::from)?;` 附近），而同一次 `Rust (windows)` 的各 target 全绿 | 只读命令，见 §4 |

**这条背景读数的含义（必须写清）**：新门禁的语义是"**这个 commit 必须已经绿**"。以当前远端状态，**任何 tag 都会被门禁拦住**（100 个 CI run 里 0 个成功）。这不是门禁写错了，而是**门禁第一次把"发布不看 CI"这件事变成可见的**：发布路径过去在没有门禁的情况下发过版（其中一次还失败了）。要恢复可发布，前提是 CI 变绿 —— 那是 `ci.yml` 的属主（t66）的活，本单只登记，不越界。

## 2 三条加固（逐条给 diff 读数）

### ① 工作流级 `contents: write` → job 级最小权限

```
改前：permissions:            （工作流级，第 7-8 行）
        contents: write
改后：permissions:            （工作流级，第 37-38 行）
        contents: read
      jobs.gate.permissions:    { actions: read }                      ← 只读裁决，连 contents 都不给
      jobs.build.permissions:   { contents: read, actions: write }     ← upload-artifact 需要
      jobs.release.permissions: { contents: write, actions: read }     ← 唯一写者
```
**为什么不是"每个 job 只有 `contents: read`"**（这是本条最容易写错的地方）：一旦写了工作流级 `permissions` 块，**没列出的 scope 对没覆盖它的 job 变成 `none`**。所以
- `build` 必须显式给 `actions: write`（`actions/upload-artifact@…` 要写 artifact 存储；不给就会在第一个矩阵腿上传 artifact 时失败）；
- `release` 必须显式给 `actions: read`（`actions/download-artifact@…` 要读 artifact）；
- `gate` 只需要 `actions: read`（它读这个 commit 的 run 裁决）。
`actions: write` **不是** `contents: write`：这个 job 仍然无法改仓库内容、无法建 release。这三行都写进了文件里的注释。

### ② 六个 `uses:` 全部固定 SHA（固定前 → 固定后）

SHA 来源：`gh api repos/<owner>/<repo>/commits/<ref> --jq '{sha,date,subject}'`（只读；注：对 `dtolnay/rust-toolchain` 我解析的是**分支 `stable` 的 HEAD**，因为它本来就不是 tag）。

| 行动 | 固定前 | 固定后（SHA # ref/日期） | SHA 来源读数（日期 + subject） |
| --- | --- | --- | --- |
| `actions/checkout` | `@v4` | `@11d5960a326750d5838078e36cf38b85af677262 # v4 (2026-07-16)` | 2026-07-16T19:43:47Z · "backport fixes to releases-v4 (#2524)" |
| `dtolnay/rust-toolchain` | `@stable`（**移动分支**） | `@6bed0761d98439e5a578e2877258200ad565ba87 # stable (branch head 2026-09-03)` | 2026-09-03T15:22:44Z · "toolchain: stable" |
| `Swatinem/rust-cache` | `@v2` | `@6323deb102c322ba6fcbdcafc7e3dddab59af2b6 # v2.9.2 (2026-08-06)` | 2026-08-06T06:23:42Z · "2.9.2" |
| `actions/upload-artifact` | `@v4` | `@ea165f8d65b6e75b540449e92b4886f43607fa02 # v4 (2025-03-19)` | 2025-03-19T17:34:59Z · merge PR #685 |
| `actions/download-artifact` | `@v4` | `@d3f86a106a0bac45b974a628896c90dbdf5c8093 # v4 (2025-04-24)` | 2025-04-24T16:25:03Z · merge PR #404 (v4.3.0) |
| `softprops/action-gh-release` | `@v2` | `@3bb12739c298aeb8a4eeaf626c5b8d85266b0e65 # v2.6.2 (2026-04-12)` | 2026-04-12T03:31:51Z · "release 2.6.2" |

文件头部写了升级流程（`gh api …/commits/<ref> --jq .sha` → 同时更新 `uses:` 与注释），所以"下次升级要人工 bump"是被记录的行为而不是欠账。

### ③ 发布前门禁：新 `gate` job + `needs:` 链

```
jobs:
  gate:      ← 新增；permissions: {actions: read}；timeout 20min
    steps: Require a green CI verdict for this commit  ← 唯一一步
  build:     needs: gate        ← 新增 needs（红提交连构建都不做）
  release:   needs: build       ← 原本就有
```

`gate` 的判据（内联在 `release.yml` 里，**没有新增脚本文件** —— 见 §3）：用 `gh run list --workflow=ci.yml --commit "$SHA"` 读**这个 commit** 的最新 CI 裁决；`completed + success` ⇒ 放行；`completed + 其它` ⇒ 失败并点名裁决与链接；没有 run ⇒ 失败并告诉人怎么补（先让 main 的 CI 出裁决，再重推 tag）；还在跑 ⇒ 最多等 15 分钟再判。**它消费 `ci.yml` 的裁决，不重复跑那套 30 分钟的测试** —— 这正是"只发布流水线已经通过的东西"。

**负控读数（把 YAML 里那一步的字节原样抽出来，在本地 bash 执行；`gh` 是真 CLI + 真 API，只读）**：

```
=== case 1: RED commit (real gh; newest CI verdict for that SHA is failure) ===
CI for 5db881d8b8937cec8386bba5dfff95ddb8dd4b03: status=completed conclusion=failure
::error::the newest CI run for 5db881d8… concluded 'failure'. A red commit must not be published. See …
case1 exit=1                       ← 红提交 ⇒ 门禁退出 1 ⇒ `needs: gate` 的 build/release 被跳过
=== case 2: commit with NO CI run at all (real gh) ===
CI for 1111…: status=none conclusion=<none>
::error::no CI run found for 1111…. Push this commit to main … then re-push the tag.
case2 exit=1                       ← 没有裁决也 fail-closed（不许"没证据"当通过）
=== case 3: SUCCESS branch (gh stubbed to answer 'completed success') ===
CI for 2222…: status=completed conclusion=success
gate: green CI verdict for 2222…
case3 exit=0                       ← 成功分支可达（这一条是**桩**：本仓 100 个 CI run 里 0 个成功，没有真绿 SHA 可用）
```

**我实际验证到哪一步（不夸大）**：
- **验证了**：这脚本的**真实字节**在三个分支上的行为（红 / 无裁决 / 绿），其中红与无裁决用的是**真 API 真数据**；
- **没有验证**：GitHub 的 `needs:` 调度语义本身（依赖失败 ⇒ 依赖它的 job 被 skip）在本机不可执行 —— 没有 `act`/Docker（`bash` 只有 WSL 的那个，GitHub 端的 job 调度无法本地复现）。这一条依据的是 GitHub 记录在案的行为 + 我在 §4 用 YAML 策略检查断言 `job.build.needs == gate` 且 `job.release.needs == build`。**这是本单证据链上唯一的推理环节，我把它标出来。**
- 另外：case 3 用桩，是因为真环境里没有绿 run 可测；桩只证明"我的逻辑在 success 时会放行"，不证明 GitHub 会给 success。

## 3 没有碰别人的 workflow；耦合面说明

- `git status --porcelain -- .github/workflows/` 里 `ci.yml` 与 `e2e.yml` 是 `M`，**那不是我改的**（t66 / t99 的在途改动）；本单只写了 `release.yml`。
- `release.yml` 与 `ci.yml`/`e2e.yml` **没有共用 composite action 或共用脚本** ⇒ 无需跨文件改动。**唯一的新耦合是"读裁决"**：`gate` 用 `--workflow=ci.yml` 依赖 CI 的工作流**名字**。若 CI 改名/停用，门禁会以"没有 run"的形态**失败**（fail-closed），不会静默放行 —— 这点写进了文件的注释。
- 顺带跑了一遍 CI 自己的守卫（t65 F1，`.github/workflows/scripts/check-workflow-refs.sh`），读数：**`release.yml` 段只有一行**（`release.yml:139 Build panel … panel/ tracked (working-directory)`）⇒ **我的加固没有引入任何未跟踪路径**（内联脚本，不引用新文件）。同一次运行 exit=1，原因是 **`ci.yml` 引用的两个脚本在本机工作树里是 `?? .github/workflows/scripts/`（未进索引）** —— 这是**先于本单**的状态（t65 的产物），按纪律只报告不动手（也不在我的 inScope）。**若下次推送时忘记 `git add` 它们，CI 的守卫步骤会在第一次推送就红**（正是那条守卫存在的理由）。

## 4 门禁（YAML 改动 ⇒ 可解析性 + diff + 只读远端读数）

| 命令 | 读数 |
| --- | --- |
| `python … yaml.safe_load(release.yml)` + 7 条策略断言（`%TEMP%\t100-policy-check.py`） | **7/7 PASS，exit=0**：工作流级 `permissions == {contents: read}` · 全部 `uses:` 匹配 `@[0-9a-f]{40}` · 三个 job 的 permissions 逐一等于期望 · `release` 是唯一 `contents: write` · `build.needs == gate`、`release.needs == build` · gate 步骤含 `gh run list` + `--workflow=ci.yml` + 对 `"$conclusion" = "success"` 的判断 · 触发仍是 `push: tags: ["v*"]` |
| `git diff --stat -- .github/workflows/release.yml` | `1 file changed, **105 insertions(+), 8 deletions(-)**`（hunk：`@@ -8 +38 @@ permissions:`、`@@ -13,0 +44,53 @@ jobs:`（新增 gate）、`@@ -15,0 +99 @@`、`@@ -16,0 +101,6 @@`（build 的 permissions）、`@@ -28,2 +118,4 @@`、`@@ -32 +124 @@`、`@@ -67 +159 @@`、`@@ -75,0 +168,5 @@`（release 的 permissions）、`@@ -77,2 +174,2 @@`、`@@ -83 +180 @@`） |
| `gh workflow view release.yml`（契约 verify；**只读**） | 显示的是**远端已推版本**（ID 357851382，2 个 run）—— 本单改动**未推送**，所以这条读数**不含**加固；本地文件才是待推版本。**明确登记这一点，避免把远端读数当成本单的验证** |
| `gh run list --workflow=release.yml --limit 3`（契约 verify） | `completed failure … 35240185033 2026-09-17T15:25:53Z` / `completed success … 34849294052 2026-09-14T13:26:49Z`（只有 2 个 run，未新增） |
| `bash .github/workflows/scripts/check-workflow-refs.sh`（CI 自己的 F1 守卫） | 见 §3：release.yml 段 1 行且 tracked ✓；整体 exit=1 由 ci.yml 的两个未跟踪脚本引起（先于本单） |
| 手工负面读数的证据 | `git tag --list` → 仍只有 1 个 `v0.1.0`（**我没有建 tag**）；`gh run list --limit 3` → 未见新 run（**没有 dispatch**） |

## 5 第 19 条：这次加固**不覆盖**什么

1. **GitHub 端的仓库/环境保护设置**：`environment: production` 保护规则、tag 保护规则、"Allow GitHub Actions to create and approve pull requests"、仓库级默认 workflow 权限、`GITHUB_TOKEN` 的组织级限制 —— 我**没有验证也没有改**（需要仓库设置权限，且改它们不是本单 inScope）。注：本文件现在自带工作流级 `permissions`，所以**本工作流不再受仓库默认权限影响**，但其它工作流仍然受。
2. **没有真正跑过一次发布**（本单明令禁止）：所以六个 SHA 固定**尚未被运行验证**；`softprops/action-gh-release` 上一次真实运行还是**失败**的（§1），其成因未查明 —— 加固**不保证**下一次发布成功，只保证"红的不会被发"。
3. **SHA 固定 ≠ 版本固定**：`dtolnay/rust-toolchain@<sha>` 运行时仍然装当时的 `stable` 工具链（Rust 版本照旧浮动）；npm 侧靠 `npm ci` + lockfile，没有额外固定。要钉 Rust 版本是另一个决定（会让 fmt/clippy 随 stable 更新的风险变成"必须人工升级"）。
4. **升级要人工 bump**：没有加 Dependabot/Renovate（新增配置文件不在 inScope）。action 的安全更新不会自动到达。
5. **门禁依赖 `ci.yml` 的名字**：改名/停用会让门禁 fail-closed（不会静默放行），但需要同步改 `gate`。
6. **门禁等待上限 15 分钟**：tag 推得太早（CI 还没出裁决）会失败并要求重推 tag —— 这是有意的 fail-closed，不是缺陷；但它是行为变化（以前推 tag 不会失败）。
7. **运行器镜像仍然浮动**（`windows-latest` / `ubuntu-latest`）；固定到 `-22.04`/具体镜像版本是更侵入的决定，未做。
8. **`build` job 的 `actions: write`** 是"最小**且**够用"的结果，不是"只读"：上传 artifact 必须写 artifact 存储。若将来改成"先在 build 里打包、再由一个只读 job 上传"，可以进一步收紧 —— 未做。
9. **发布产物本身的质量**（二进制里到底有没有那个 bug、panel 是否可用）不在任何门禁的射程内：门禁只说"这个 commit 的 CI 绿了"。
10. **本地未提交的一堆工作**（含 `.github/workflows/scripts/` 两个脚本）没有被本单处理；它们能否被正确 `git add` 进下一次推送，决定 CI 会不会在第一次推送就红（§3）。

## 6 纪律回执

- **未触发发布**：没有 `git tag`、没有 `gh workflow run`/`gh run rerun`、没有 push；`git tag --list` = 1 个既存 `v0.1.0`；`gh run list` 未见新 run。
- 写入集合：`.github/workflows/release.yml`（`M`）+ 本报告（`??`）。**未改** `ci.yml`/`e2e.yml`/`design-audit.yml`/`AGENTS.md`。
- 临时文件自清（收尾读数）：`%TEMP%\t100-policy-check.py`、`t100-gate.sh`、`t100-driver.sh`、`t100-bin/`、`t100-bin-stub/`（以及 WSL `/tmp/t100-bin`、`/tmp/t100-stub`）—— 删除后实测 `temp removed: True / True / True`。报告里保留了这些脚本的**命令与期望读数**，所以结论不依赖临时文件存在。
- 活库只读（本单不涉及数据库）；未启停真守护进程 pid 79984（本单不涉及守护进程，仍未触碰）。
