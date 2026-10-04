# `release.yml` 门禁覆盖声明：**未落地** —— 量证完成 + 交接包（t192, attempt 1）

**状态：failed（交接；不是判断为不可做）** · **零源码改动**：`.github/workflows/release.yml` 一字未改 ⇒ 树没被留在半成品。唯一写入 = 本文件。停手原因是**本会话预算耗尽**：剩余上下文不足以完成「写声明 → 四种情形负控（假 `gh`）→ `check-workflow-refs.sh` → 报告」。在无读数的情况下改 YAML 只会留下**未验证的改动**，所以选择干净交接；**我没有把「本地假 gh 跑通」写成「release 已验证」——因为我一条都没跑**。

## 0 身份（C61：按提交号，不用 `HEAD` 当结论）

| 项 | 值 |
| --- | --- |
| 提交（本轮 `git rev-parse HEAD`，作为**读取点**） | `4c3b99182e18768c4bf00ede9bf30ae4487db321` |
| 该提交上的 blob（`git rev-parse <commit>:.github/workflows/release.yml`） | `382509a183c4d2d1a9292be39ef80799b1cd7eb5` |
| 工作树文件的 sha256 | `05BCA92F05AFD00FD6F157F8BB943FE3FA06ECBCC91CCD84C00250AD60053B31` |
| 最后改过 `release.yml` 的提交 | `02dea44`（2026-09-29T10:04:00+08:00，= t100 硬化那单） |

## 1 量证（逐行原文已在本轮逐行打印过 200 行；坐标为**回读**结果）

1. **门查的是哪个 workflow**：**只有 `ci.yml`** —— `:70` 的 `gh run list --repo "$REPO" --workflow=ci.yml --commit "$SHA" --limit 1 --json status,conclusion --jq '.[0] // {} | "\(.status // "none") \(.conclusion // "")"'`。全文件**没有**任何 `e2e` 字样（200 行逐行确认）⇒ **`e2e.yml` 不在门视野里**（③ 的答案：不在）。
2. **四种情形**（`:76-95`，`set -euo pipefail` + `deadline=$(( $(date +%s) + 900 ))` 在 `:68`）：
   | 情形 | 判定 | 退出码 | 文案坐标 |
   | --- | --- | --- | --- |
   | `completed` + `success` | 放行 | **0** | `:79 "gate: green CI verdict for ${SHA}"` |
   | `completed` + 非 success | 拒绝 | **1** | `:82 "::error::the newest CI run for ${SHA} concluded '…'. A red commit must not be published. See https://…/runs?query=head_sha%3A${SHA}"` |
   | `none` | 拒绝 | **1** | `:86 "::error::no CI run found for ${SHA}. Push this commit to main (or open a PR) and let CI report a verdict, then re-push the tag. A tag with no verdict is not a release candidate."` |
   | 15 分钟未完成（`date +%s ≥ deadline`） | 拒绝 | **1** | `:91 "::error::CI for ${SHA} is still '${status}' after 15 minutes; not publishing on an unfinished verdict. Re-push the tag once CI is green."`（轮询 `sleep 30`，`:94`） |
3. **`audit.yml` 的触发**：`:71 on:` 下只有 **`workflow_dispatch`（`:73`）** 与 **`schedule`（`:77-78`，`cron: "0 3 * * 1"`）** ⇒ **没有 `push`** ⇒ 「每个提交本就没有判定」成立 ⇒ 它**不能**成为 per-commit 门的输入。**排除它是正当的**，只是这份正当性今天**一个字都没写**。
4. **`e2e.yml` 的触发**：`:15 on:` → `:16 push:`（每次 push 都跑）。**`concurrency:` / `cancel-in-progress` 在 `e2e.yml` 上 grep = 0 命中**（⇒ 新 push **不会**取消同 SHA 的旧 run，判定按 SHA 保留 —— 这条对第 3 项的决定是**支持**要求它的证据；但**我只做了 grep，没有读 `gh run list` 的同 SHA 多 run 形态**，那条**未取到**）。

## 2 声明该说什么（配方，逐条与门实际读的东西对应）

放在 `gate` job **旁边**（与 `ci.yml:85` 的 `Coverage declaration (what a green here does NOT mean)`、`e2e.yml` 同一种纪律），至少五条：

1. **本门只消费 `ci.yml` 的判定**（`:70`）—— 点名它就是门读的东西。
2. **`e2e.yml` 的判定不被要求** —— 并写明这是**有意**还是**待定**（我尚未替你决定，见 §3；**声明不能对这一点含糊**）。
3. **`audit.yml` 是 dispatch + 周更 schedule ⇒ 每个提交本就没有判定**，排除它是**有意的**。
4. **矩阵构建 ≠ 在各目标平台上被测过**：`build` 只做 `cargo build --release --target`（`:142-143`）+ `npm ci && npm run build`（`:146-150`）⇒ **没有测试**在那两个 os/target 上跑；`windows-latest` 与 `ubuntu-latest` 只是**构建宿主**。
5. **tag 上的产物与被验证字节的关系**：门放行的是 **`github.sha`（tag 指向的提交）的 `ci.yml` 判定**；`build` 在该提交上**现场重编**（`actions/checkout` 无 `ref:` ⇒ 取 tag 指向的提交），产出的 `ruagent-*.zip`/`.tar.gz` **没有**独立验证者 —— **`build` 自身即唯一见证**（`release` 只下载并上传，`:193-201`）。⇒ 谁验证了产物本身：**没有人**，除了该 job 自己成功。

## 3 「是否把 `e2e.yml` 的判定也要求上」—— **建议：要求，但不在本单做**（需要一次独立评估）

- **支持**：① `e2e.yml` 是**每次 push 都跑**的面板运行时证据，且**没有 `cancel-in-progress`**（grep 0 命中）⇒ 同 SHA 的判定不会被新 push 抹掉，门可以像读 `ci.yml` 一样读它；② 现门自称 `Gate (this commit must already be green)`（`:55`）却又只读一份 workflow ⇒ 文案强于事实（这正是本单要修的缺口）；③ 参考量级：`e2e.yml` 的历史耗时为分钟级（本代记录：Build 246s / Playwright 119s / Doctor 37s ≈ 7 分钟），**900s 上限足够**，不需要上调。
- **反对/待证**：把第二条 workflow 纳入 = **改门的行为**（今天只读 `ci.yml`），会影响「`ci.yml` 绿而 `e2e.yml` 红时是否拒绝发布」这一策略选择；且「`gh run list --workflow=e2e.yml` 在同 SHA 有**多个 run**（重跑）时 `--limit 1` 取到的是哪一个」**我未取到读数** ⇒ 在拿到「同 SHA 多 run 形态」之前，我不建议把策略改掉。
- **⇒ 明确结论**：**本单不把 `e2e.yml` 纳入门的判定**（保持行为不变，避免在无读数的情况下改策略）；但**声明里必须写明这是「待定」而不是「有意省略」**，并把上面两条未取读数（同 SHA 多 run 形态、是否调 900s）列为下一次评估的输入。**若 captain 要立刻纳入**，配方是：在 `:70` 之后加第二个 `gh run list --workflow=e2e.yml …`，对 `completed+success` 放行、`none`/非 success/超时**同样拒绝**（文案逐字沿用 `:82/:86/:91` 的形状），并在声明里点名它现在是**两**份判定。

## 4 本单**未取到**的必交读数（下一位的待办，全部有配方）

- **负控（第 5 条验收）**：`%TEMP%` 放假 `gh` + 抽出 `:66-95` 的 shell 逐情形跑 —— **未做**（这是我的预算缺口里最该补的一条）。配方：从 `:66` 的 `run: |` 抽出脚本体，把 `gh` 换成返回 `{"status":"completed","conclusion":"success"}` 等四种应答的假脚本；`PATH` 前置；`SHA`/`REPO` 用假值；断言四种退出码 `0/1/1/1` 与 `:79/:82/:86/:91` 文案。**不碰 GitHub**。
- **`bash .github/workflows/scripts/check-workflow-refs.sh`**：**未跑**。
- **YAML 静态校验的自判**：**未做**；已知避雷点（t179）：**job 级 `env` 里用 `runner.temp` 会让整个文件被判废** ⇒ 写声明时**不要碰 `env`**，纯注释最安全。
- **真正的 tag 触发**（`:32-34`）**无法本地验证**，只能由真实 tag 触发验证。

## 5 未覆盖什么（第 19 条）

- **不覆盖** `e2e.yml`/`audit.yml`/`ci.yml` 的任何行为（三者均**本单 outOfScope**，未改一字）。
- **不覆盖**门在真实 GitHub 上的可达性、Actions API 的限额/权限行为（`actions: read` 只做了静态读取）。
- **不覆盖** tag 产物在**用户机器**上的可用性（无验证者，见 §2.5）。
- **不覆盖**四种情形之外的形态（例如同 SHA 多个 run、`cancelled`/`skipped` 结论的具体表现）。

## 6 零改动自证 + 边界（C63）

`git status --porcelain -- .github/workflows/release.yml` ⇒ **空**（本 attempt 未改它）；**未发布任何 tag、未 dispatch、未 push**；未碰 GitHub；未启停活守护进程、未写其库。
