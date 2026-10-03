# F2 落地：`audit.yml` —— 面板真测量的 advisory 门（t131）

> 单号 **t131**（implementation）· 成员 `wiki` · attempt **2** · **2026-10-04 00:1x–00:29（+08:00）**
> 时间以文件 mtime 为准：`audit.yml` **00:21:35** · 本报告 **00:27:02** · 侦察报告 `gen4-audit-gate-recon.md` 00:17:25。**初版表头写的「08:3x–09:5x」是我的估计值，已按实测 mtime 更正** —— 与 §6 同一条纪律：时间/坐标要回读，不凭印象。
> inScope：`.github/workflows/audit.yml`（**新增，297 行**）· 本报告。**未改** `ci.yml` · `e2e.yml` · `release.yml` · `crates/**` · `panel/**`。
> 输入 = 我自己的侦察报告 `docs/design/reviews/gen4-audit-gate-recon.md`（t122，185 行）。
> 纪律：**未** push / dispatch / rerun / cancel / 建 tag；**未**触碰活守护进程（8787 在听）· 也**未**触碰队友 t128 在 8899 上的临时守护进程（见 §8）；未跑 rust 构建。

## 0 状态一句话

`.github/workflows/audit.yml` 已落地：`workflow_dispatch` + 每周 `schedule`，以 **advisory** 方式跑真 `--check`（判活 DOM、能红、exit 1），**四道护栏每道都本地跑过并通过/能红**，便宜负控每次运行都执行，产物指向仓库外并上传。**首次 CI 读数：待推送**（§4 写明补齐方式）。

## 1 形状（行号是对**写完之后**的字节回读的，C33）

| 项 | 位置 | 值 / 读数 |
| --- | --- | --- |
| 文件 | `.github/workflows/audit.yml` | **297 行**，**新增**（`?? .github/workflows/audit.yml`） |
| 名称 | `:35` | `Design audit (§12, live DOM)` |
| 触发器 | `:37-44` | **`workflow_dispatch`**（`:39`，人可点 ⇒ 第一步读数的来源）+ **`schedule`**（`:43-44`，`cron: "0 3 * * 1"` 周一 03:00 UTC ⇒ 测产品漂移，不占 push 关键路径）。**故意没有 push 触发**（§7 第 2 条） |
| 权限（workflow） | `:47-48` | `permissions: contents: read` |
| concurrency | `:50-53` | `group: design-audit-${{ github.ref }}` · **`cancel-in-progress: false`**（被取消的 run 会把为它而跑的那份读数一起抹掉 —— `gen3-ci-red-history.md` 的教训） |
| job | `:57-58` | `audit:` / `design-audit --check (advisory)`，`runs-on: ubuntu-latest` |
| **`timeout-minutes`** | **`:69`** | **45** —— 推导：**实测基线 `t122` = 624.6s（10.4 min，24 captures）** × 3 ≈ 31 min，加 CI 独有前置（`cargo build` ~246s · panel build ~22s · `npx playwright install` · 起 daemon）≈ 6 min ⇒ ~37 min ⇒ 取 **45**（约 1.2× 余量；GitHub 默认 360 min 会让「挂住」与「慢」不可区分，同 `gen3-ci-timeouts.md`）。**关键：倍数取自「完整跑完」的那次**——本 workflow 里的便宜腿是 53.9s（单路由）与 0.44s（死端口），拿它们当基线会把真审计杀在中途 |
| 权限（job） | `:70-75` | `contents: read` + `actions: write`，与 `release.yml:101-106` 的 build job 同一对，并注明**不是** `contents: write` |
| 步骤 | `:76-297` | checkout(`:77`) → toolchain(`:78`) → rust-cache(`:79`) → protoc(`:80`) → setup-node(`:82`) → **Build the daemon**(`:88`) → **Build the panel**(`:91`) → **Install the browser**(`:95`，`npx playwright install chromium --with-deps`) → **Boot a throwaway daemon**(`:101`) → **Negative control**(`:131`) → **audit --check（advisory）**(`:159`) → **Guardrails**(`:177`) → **upload-artifact**(`:277`) → **Record the verdict**(`:290`) |
| **永远显式命名目标** | `:169`（审计）/ `:140`（负控） | `--base-url=http://127.0.0.1:8898`（throwaway daemon）与 `--base-url=http://127.0.0.1:1`（死端口）——**绝不**落到 `design-audit.mjs:401` 那个 `E2E_BASE_URL ?? http://127.0.0.1:8787` 的活守护进程默认值；daemon 用**显式 temp root** 起（`:101`：`--root "$RUNNER_TEMP/audit-root"` + `RUAGENT_PANEL_DIST="$PWD/panel/dist"`） |
| **产物在仓库外** | `:169` + `:277-289` | `--out="$RUNNER_TEMP/audit"`（`:8152-8172` 指向仓库内会 exit 2；省略会写进 `docs/screenshots/audit-run`）配 `upload-artifact@ea165f8d…`（`:279`，固定 SHA）+ **`if: always()`**（`:278`，失败的那次才最值得留） |
| `uses:` 固定 | `:77,78,79,82,279` | 五条**全部** 40 位 SHA，且与 `ci.yml`/`e2e.yml`/`release.yml` **同 action 同 SHA**（机械核对：跨四份 workflow **没有**任何 action 出现两个不同 SHA） |

## 2 四道护栏（逐条：机制 + 本地读数 + 能红的证明）

| # | 机制（位置） | 本地读数 |
| --- | --- | --- |
| **① 计数进 step summary** | `:180-199`（解析汇总行 + 失败名单）与 `:255-275`（写 summary）。**汇总行必须存在**，否则 `::error::`+`exit 1`（`:181-191`） | 真日志：`captures=24 pass=44 fail=11 not_measured=10 judged=65 audit_exit=1`；summary 里逐条列出 `failing rows: 2, 4, 6, 7, 12, 13, 14, 18, 20, 38, 75` 与 not_measured 具名清单（§6 附逐字内容）。**缺汇总行的情形实测 ⇒ exit 1** |
| **② 未测名单钉成期望值** | `:238` `EXPECTED_NOT_MEASURED="25 28 31 33 46 47 49 50 54 77"`；`:239-253` 归一化后**逐项比对，不一致即 `::error::` + exit 1**。**故意不传 `--allow-not-measured`**（它一次性豁免全部未测项而退出码不变，`design-audit.mjs:417/466/8416-8418`）——本 workflow 把**名单本身**当期望值，比「豁免后比名单」更强 | 一致 ⇒ **exit 0**；把 `#77` 从日志里去掉 ⇒ **exit 1**，原话 `::error::not_measured set moved: expected '#25 28 31 33 46 47 49 50 54 77', got '#25 28 31 33 46 47 49 50 54'` |
| **③ 每次跑的便宜负控** | `:131-157`：把 `--base-url` 指向 **`http://127.0.0.1:1`**（保证无人监听，见 §7 第 6 条），断言 **`rc == 2`**（`:143-147`）**且**原话含 `daemon not reachable`（`:149-152`），结果也写进 summary（`:153-157`） | 健康情形：**exit 2 / 0.44s（3 次：0.46/0.41/0.42s）**，原话 `daemon not reachable at http://127.0.0.1:1 (no panel to audit). Start it with: cargo run -p ruagent-cli -- serve` ⇒ 脚本断言全过。**变异（把同一脚本指向一个真在听的端口 ⇒ exit 1）⇒ 脚本的 `rc != 2` 分支点火 ⇒ exit 1** ⇒ **这道护栏真的能红** |
| **④ captures 不当常数** | `:206-237`：`captures` **只打印、只要求 ≥1**，**从不**断言 24 或 26；被钉死的是**检查集大小 65**（`judged == 65`），因为两次真跑（全量 44+11+10、子集 29+6+30）都是 65 ⇒ 工具/契约漂移能被发现，而「空 root 让 `#task` 整条 skip」不会误红 | `judged 64 != 65` ⇒ **exit 1**（原话 `the audit judged 64 rows, not 65: the CHECK LIST changed size…`）；`captures=0` ⇒ **exit 1**（`the audit captured 0 pages -- nothing was measured, so nothing may pass`） |

**护栏能红性的五格矩阵**（同一份脚本、五份合成日志，逐格给退出码）：**0 / 1 / 1 / 1 / 1**（一致 / 名单漂移 / 无汇总行 / judged≠65 / captures=0）。

## 3 advisory → 阻塞：条件与「今天谁在阻塞谁」

- **今天是 advisory**：`:164` 的 `continue-on-error: true` 只挂在**审计那一步**上 ⇒ 审计自身的 exit 1 是**读数**（那一步显示红叉，job 不因此失败）；而护栏 ①②③④ **不带** `continue-on-error` ⇒ 一旦护栏点火，**job 红**（那是接线缺陷，不是产品缺陷）。这个区分是有意的：**「测量结果难看」与「测量本身坏了」必须可分辨**。
- **今天谁在阻塞谁：谁都没阻塞**。本 workflow **不在 branch protection 里，也不应现在加**——它红了**不挡**任何 push（`:266-268` 的 summary 原话也这么写）。它红的事实通过 **step summary + artifact（`if: always()`）**被记录。
- **何时可以改阻塞（可复核的四条）**（文件头 `:20-33` 与本节一致）：
  1. 11 条 fail 收敛成一份**具名**清单，并像 ② 那样**钉成第二个常量**（今天只有未测名单有 pin）；
  2. 未测名单在连续 `EXPECTED_STABLE_RUNS` 次运行里**保持等于** `EXPECTED_NOT_MEASURED`（今天就已经是 pin ⇒ 这条是「连续 N 次」的计量）；
  3. 这 N 次的 fixture **同形**（同一 root 形态、同一端口、同一 dist 新鲜度规则）⇒ 比较的是可比的读数；
  4. 满足 1–3 之后，**删掉 `:164` 这一行**才有意义，并把 fail 名单的 pin 也加进护栏。
  在 1–3 完成前，任何「先删 `continue-on-error` 再说」都会把 main 钉红并教会所有人忽略它。

## 4 首次 CI 读数 —— **待推送**（推送后由我补齐）

- **当前状态（推送前）**：`audit.yml` 已就绪且本地门禁全绿（§6），但**从未在 GitHub 上跑过** ⇒ **本项未测**，原因是本单**不许** push/dispatch。
- **补什么、怎么补**（推送后我按此填）：① `gh run list --workflow=audit.yml` 拿到 run 编号与结论；② `gh run view <id> --log` 取**逐字**的 `[audit] N captures in …s · checks P pass / F fail / U not measured` 行；③ `failing rows: …` 与 `not measured: …` 两份具名清单；④ 若 CI 的未测名单与 pin 不同（**很可能**，见 §7 第 4 条），把那次读数作为证据写进本节并**在后续提交里更新 `:238` 的 pin**（改动本身就是证据，不许静默放宽）。
- **第一次 CI 运行若不绿，最可能的两个原因都已预备好解释**：护栏 ②（ubuntu+chromium 的未测集合与 Windows 不同）与审计自身的 11 条 fail（那是**产品侧**的读数）。

## 5 两个被纠正的前提（带日期的修订记录）

| # | 旧说法 | 实测（以平台状态/真跑为准） | 日期与证据 |
| --- | --- | --- | --- |
| ① | 「`t86` 占着 `.github/workflows/` 整个目录 ⇒ F2 要排在它后面」 | **错**。`t86`（pending，assignee=recall）的 `inScope: ['docs/design/reviews/gen3-ignore-guard.md']`，**不覆盖** workflows 目录；它不可认领是**依赖里有终态失败单**（`dependencies: ['t65','t66','t77','t85']`，其中 t66 失败、t77 未完成） | 2026-10-02（t122）读 `.agent-teams/ruagent-mem-gen2/team.json`；captain 2026-10-04 在 t131 合同里采纳 ⇒ **F2 不需要等 t86**（本单即证明：新增文件与它零重叠） |
| ② | 「`--check` 今天跑不起来」 | **错**。它**跑得起来但绿不了**：真跑 `24 captures in 620.7s · 44 pass / 11 fail / 10 not measured`，exit 1 | 2026-10-02（t122 §2 原始读数）⇒ 这正是 **advisory-first** 的理由（§3） |

## 6 本地验证读数（逐条可复核）

| 校验 | 命令 | 读数 |
| --- | --- | --- |
| **守卫**（refs + 表达式形状与**上下文可用性** + YAML 可解析） | `bash .github/workflows/scripts/check-workflow-refs.sh` | **exit 0**：`workflow path references checked: 22, not tracked/missing: 0` · `every executed path a workflow references is tracked` · **四份** workflow（含新文件）`parses as YAML (PyYAML)` |
| **守卫在本单里真的抓到了两处缺陷**（过程证据，说明它不是走过场） | 同上，第一次运行 | ① `:155/:265` 的 `name:` 里含**未加引号的 `: `** ⇒ GitHub 在**解析期**就会拒整个文件（`UNQUOTED ': ' in a plain scalar -> invalid YAML`）⇒ 已改成 `--`；② `:111` 引用了 `target/debug/ruagent`（**未被 git 跟踪的构建产物**）⇒ `MISSING` ⇒ 已换成 `cargo run -p ruagent --`（与 `e2e.yml:143` 同形）。**两次都是守卫先红、我再改** |
| **每个 `run:` 块的 bash 语法** | 抽出 9 个 `run:` 块 → `bash -n` | **9/9 exit 0** |
| **护栏五格矩阵**（合成日志，见 §2） | 抽出护栏脚本，配 5 份日志 | **0 / 1 / 1 / 1 / 1** 且四种失败都打出具名 `::error::` |
| **负控健康情形** | 抽出负控脚本（等价 harness） | exit **2** / **0.44s**（3 次 0.46/0.41/0.42）· 含 `daemon not reachable` |
| **负控变异（能红）** | 同一脚本指向一个**真在听**的端口 | 审计 exit **1** ⇒ 负控的 `rc != 2` 分支点火 ⇒ **exit 1** |
| **合同里的 Verify 命令**（裸形式，死端口） | `cd panel; node tools/design-audit.mjs --check --base-url=http://127.0.0.1:8899 --out=$env:TEMP/ruagent-t129-deadport` | **exit 2**，~**0.45s**，原话含 `daemon not reachable at http://127.0.0.1:8899 (no panel to audit)`（另见 §7 第 3 条的抖动登记） |
| **成对读数（subset 形式）** | `… --check --routes=home --modes=dark --no-shots --base-url=http://127.0.0.1:8899 …` | **exit 2 / 0.48s**，同一句原话 ⇒ 两种形式都点名，选 subset 只是更省 |
| **step summary 实际内容** | 上述五格矩阵第 1 格的 `$GITHUB_STEP_SUMMARY` | 逐字见 §2①；其中 `- audit exit code: **1** (advisory: recorded, not enforced)`、`- captures: **24** -- NOT a constant…`、`- judged rows: **65** (pass 44 / fail 11 / not measured 10); the pinned constant is 65`、`- failing rows: \`2, 4, 6, 7, 12, 13, 14, 18, 20, 38, 75\``、`- not measured (matches the pinned set \`#25 28 31 33 46 47 49 50 54 77\`): \`#25 28 31 33 46 47 49 50 54 77\`` |
| **SHA 固定与跨文件一致性** | 逐行 `\s*-?\s*uses:` + `@[0-9a-f]{40}` | audit.yml 五条全部命中；四份 workflow 的 action→SHA 映射**无冲突** |
| **YAML 形状**（PyYAML 回读） | `yaml.safe_load` | 触发器 = `{workflow_dispatch: None, schedule: [{cron: '0 3 * * 1'}]}` · job 14 步 · `timeout-minutes=45` · `permissions={contents: read, actions: write}` |

## 7 未覆盖什么（第 19 条）

1. **`--self-test` 与 `--check` 的关系**：`ci.yml:457-459` 已经跑 `--self-test`（我实测 **exit 0 / 0.54s / 495/495 断言**），它证明的是**准则的解析与逻辑**，**不碰 DOM、不要浏览器、不要 daemon**；本 workflow 跑的是 `--check`（判活 DOM）。**两者互不替代**，新 workflow **没有**动 `ci.yml` 的那一条。
2. **触发面不覆盖 push**：这是有意的（一次 10.4 分钟，`ci.yml` 每次 push 已 ~25 分钟）。**代价**：一次只在 push 上引入、又被下一周的 schedule 才发现的回归，会在 main 上停留最多 7 天；**缓解**：任何人都可以 `workflow_dispatch` 点一次。
3. **CI 侧的判词未测，而且预期会不同**：本机读数出自 **Windows + chromium-1243**；CI 是 **ubuntu + 其自带 chromium**，像素类行（`#1/#2/#4/#29/#30` 等）依赖光栅器与字体 ⇒ **未测名单很可能与 pin 不同**，第一次 CI 运行会因此让护栏 ② 红（那是**预期**的第一动作，见 §4）。
4. **代理（`NO_PROXY` host 列表）在 CI 上未测**：本机这次不需要它（8897/8898/8899 都通），但 AGENTS.md 记着本机系统代理指向死端口 `127.0.0.1:7890`；**CI 上是否需要 host 列表我没测**。风险面：node 与 Chromium 都可能读系统/环境代理；若第一次 CI 运行在**连 own daemon** 这一步失败，第一嫌疑是它（届时加 `env: NO_PROXY: 127.0.0.1,localhost,::1` 并给出成对读数）。
5. **有数据的 fixture 未测**：本 workflow 用**空 temp root**（`24` captures）；若将来为了跑到 `#task` 与 `.nav-badge` 而造数据，**captures 与未测名单都会变**，pin 必须重新标定（④ 已按「captures 非常数」设计，但 `judged==65` 这个 65 也要复核）。
6. **「死端口」不是一个固定端口**：合同里的 Verify 用 **8899**，而我写 workflow 时发现 **8899 上已经有队友 t128 的临时守护进程在听**（§8）⇒ 若照抄 8899 当「死端口」，**负控会被一个活的 daemon 骗过**（它会进入真审计、exit 1 而不是 exit 2，于是负控点火——方向安全，但原因错）。**本 workflow 因此用 `127.0.0.1:1`**（保留端口，实际不会有人起服务）。这条也是「坐标会变」的实例：**任何『已知死』的端口都会随时间失效**。
7. **一次抖动已登记，并且我做了有界复现（不掩盖，也不弱化断言）**：死端口控制在 5 次观测里有 **1 次**是 **129.9s + 栈回溯**（`at auditRoute … :3857 … at async main … :8283`）而不是 0.44s 的点名失败；5 次都是 **exit 2**，但**那一次的报文里没有 `daemon not reachable`**。
   **复现尝试（本报告成稿后追加，16 次全清）**：`10×` 顺序裸形式（exit 2 / 0.38–0.46s / 全部点名）· `3×` 对一个**刚刚停止监听**的端口（0.39s / 全部点名）· `3×` **并发**三条控制（0.43/0.53/0.66s / 全部点名）⇒ **16/16 exit 2 + 点名**，无 >5s、无栈回溯。⇒ 观测合计 **21 次里 1 次**，复现率很低，且**与「重复执行 / 端口刚变死 / 并发」三个假设都无关**。
   **更强的假设（给立案者，未证实）**：那一次发生在 **8899 正被队友 t128 的临时 daemon 绑定的窗口内** —— `PID 31020` 的启动时间是 **00:24:44**，而我的 8899 观测横跨它启动前后。**「连接被拒绝」与「TCP 连上了但对方不服务」是两件事**：前者的点名检查会很快返回 `daemon not reachable`；后者会让它**进入 `auditRoute`（`:3857`）**，在 13 条路由上各自等到超时（合计 ~130s）后抛错，再由 `:8436-8439` 的 catch 换成 **exit 2 + 栈回溯**。这与「既慢、又没那句话」这一次的全部特征吻合。
   **处置（按 captain 裁决 2026-10-04 记录在此）**：护栏 ③ **两半都保留**（`rc == 2` **且** 报文含 `daemon not reachable`），**不因这次抖动放宽** —— 只留退出码等于「任何 exit 2 都算负控成立」；若它在 CI 上复现，那是 `design-audit` 的**独立缺陷（挂住路径）**，**单独立案，不并入本单**。
8. **WSL 不能忠实地跑这些 bash 步**：本机 WSL 里没有 node（`bash` 跑负控步得到 **exit 127**）⇒ 我用 **`bash -n`（语法）+ 等价 harness（PowerShell 执行同一条 node 命令并施加同样的两条断言）**替代「在 bash 里端到端跑」；**这两个替代都不能证明 GitHub runner 上的行为**，它只能由第一次真实运行证明。
9. **不覆盖面板实现本身**：11 条 fail 是产品读数，修它们是 `panel/**` 的单子，不在本单。

## 8 纪律回执

- **写入集合** = `.github/workflows/audit.yml`（新增，297 行）+ 本报告；`git status --porcelain` 里其余条目（`crates/**`、`panel/src/i18n/agents.ts`、`panel/src/views/Agents.tsx`、`panel/e2e/agents-health-third-state.spec.ts`、`docs/design/reviews/gen4-version-points-impl.md` 等）**都是队友的在途改动**。
- **未** push / dispatch / rerun / cancel / 建 tag；**未**跑 rust 构建（`audit.yml` 里的 `cargo build` 只在 CI 上执行）。
- **未触碰别人的进程**：**8787** 的活守护进程自始至终在听；**8899** 上队友 t128 的临时 daemon（`PID 31020`，`--root %TEMP%\ruagent-t128\root`，**启动 00:24:44**，收尾核对时**已退出**）**被识别但未动**（§7 第 6/7 条就是它给的教训：它是「死端口会变活」与那次抖动的共同线索）。
- **我自己的临时资源**（按具体路径，无通配）：删除 `%TEMP%\t131-check`（含 9 个抽出的 `run` 块、5 份合成日志、2 个 summary、stub 站点、两份 wrapper）、`%TEMP%\ruagent-t129-deadport`、`%TEMP%\ruagent-t129-deadport-sub`、`%TEMP%\t131-flake`（复现尝试的 16 次运行与 stub）；我起过的 stub server（python http.server 8898）已停并核对 `8898 listening: False`。**00:29 的端口读数**：`8899 listening: False`（t128 的 daemon 已退出）· `8787 listening: True`（活守护进程，未触碰）。复现尝试的 16 次运行都发生在 8899 重新变空闲之后（00:29），这也是它们全清的原因之一。
- **不是我但名字相近、按纪律未删**：`%TEMP%\t131run`（10/01 20:15）· `%TEMP%\t129z-apply.py` / `t129z.json`（09/24，上一代同名任务）。
