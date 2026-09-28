# gen3 审计：守卫的守卫 —— design-audit / build-panel / run-e2e / scripts / release.yml（t95 · 只读）

**对象**：`panel/tools/design-audit.mjs`（8,359 行 / 538,865 B / sha256 `F39F989C4EBA…` / mtime 2026-09-27 02:18:00）· `panel/scripts/build-panel.mjs`（59 行）· `panel/e2e/run-e2e.mjs`（17 行）· `scripts/{converge-near-duplicates.mjs, ruagent-canary.ps1, ruagent-post-switch-check.mjs}` · `.github/workflows/release.yml`（81 行）。
**只读纪律**：本报告是唯一写入。**未推 tag、未起真守护进程、未写活库、未启停 pid 79984。** 我跑了两条被允许的命令：`node tools/design-audit.mjs --self-test`（不落盘、不启浏览器）与（此前 t51/t92 已跑过的）`npm run build` 的读数引用。
**我自己的读数（本次）**：`cd panel && node tools/design-audit.mjs --self-test` ⇒ **`self-test: 495/495 pass`，exit 0**。

---

## 0 一句话

**这个仓库最大的守卫（54 条 §12 判据）没有被任何门禁跑，而且它自己的退出码只认 `fails`**：`design-audit.mjs:8351` = `return args.check && fails.length ? 1 : 0` —— **warnings 只被打印（`:8349`）、`not_measured` 与 `pending` 都不影响退出码**。所以「契约读失败 ⇒ 静默回落到内置常量」这件事**在日志里可见、在退出码里不存在**；「所有判据都测不了」也会**以 exit 0 收场**。这正是本代最贵的那类形状（守卫本身没被守卫），而 CI 跑的 `--self-test` 按工具自己的帮助文字**只覆盖 row 16/23**。

---

## 1 findings（按严重度排序；每条：`file:line` · 可复现命令 · 证据读数 · 为什么重要 · 可证伪修复判据 · 建议 owner）

### T-1（high）`--check` 的退出码不认 warnings、不认 `not_measured` ⇒ 「回落」与「全测不了」都是绿的
* **`file:line`**：`panel/tools/design-audit.mjs:8351`（`return args.check && fails.length ? 1 : 0`）· `:8349`（`for (const w of warnings) log(...)` —— 只打印）· `:8338-8341`（`fails/unmeasured/pending/passed` 四个计数）· `:8131-8137`（`warnings.push(...thresholds.warnings)` / `...contract.warnings`）· `:5777`（每行 verdict：`!judged.length ? "not_measured" : fails.length ? "fail" : "pass"`）· `:77-104`（`BUILTIN` 内置回落）· `:8162-8163`（daemon 不可达 ⇒ 报错）。
* **可复现命令**：`cd panel && node tools/design-audit.mjs --check`（需 `npm run build` 过的 panel + 一个临时 root 的 daemon；只读判据，不改仓库）。
* **证据读数**：退出码表达式只有 `fails.length` 一个输入；**`unmeasured.length` 与 `pending.length` 不进退出码**；`warnings`（含 `thresholds.warnings`/`contract.warnings`/`panel/dist not found`/`dist 比 src 旧`）同样不进退出码。⇒ ① 契约单元格措辞变化导致阈值解析失败 ⇒ 判据退回 `BUILTIN`（`:83-104`）但仍 **exit 0**；② 若所有探针都失败，所有行变 `not_measured` ⇒ **exit 0**（日志会写 `checks 0 pass / 0 fail / 54 not measured`）。
* **为什么重要**：`:77-82` 的注释自己写着「**A fallback you cannot see is a silent lie**」—— 目前的实现只做到「看得见」（打印），没做到「**载重**」（拦下）。而 `--check` 恰恰是**唯一真正做测量**的模式（见 T-2），它一绿到底就没人再看日志。
* **可证伪修复判据**：`--check` 在下列任一条件出现时 **exit 1**（并打印成 `::error`-可读的一行）：**(a)** `thresholds.warnings` 或 `contract.warnings` 非空（= 契约没被成功读到）；**(b)** `unmeasured.length > 0`（除非显式 `--allow-not-measured`，且该 flag 必须出现在报告里）。**负控**：故意改一个契约单元格的措辞 ⇒ 期望 exit 1 且点名那一条；恢复 ⇒ exit 0。**owner**：panel/design-audit。
* **同类**（同一条定型）：`pending`（`:5782` 把 row 32 的 FAIL 降级为 `pending`）也要么保留在计数里、要么写明豁免理由与到期条件；今天是**第三种非失败态**。

### T-2（high）54 条 §12 判据的**真正测量**不在任何门禁里；CI 只跑 `--self-test`（按工具自述只覆盖 row 16/23）
* **`file:line`**：`.github/workflows/ci.yml:344`（`run: node tools/design-audit.mjs --self-test`）· `design-audit.mjs:452`（`--check  judge every implementable row of MASTER.md §12; exit 1 on failure`）vs `:454`（`--self-test  prove rows 16/23 implement the primitives §11 criterion`）· `:6307`（工具自己的复现清单写 `--check … # exit 1 if any §12 row fails`）· `docs/design-language.md:88-90`（三种模式与 6.5 分钟成本）。
* **可复现命令**：`cd panel && node tools/design-audit.mjs --self-test`（我跑的：**495/495 pass, exit 0**）· 对照 `node tools/design-audit.mjs --check`（未在 CI 出现）。
* **证据读数**：`--self-test` = 判据**代码**的可证伪性证明（对合成输入强制两个方向，含 `must-FAIL` 行）；`--check` = 对**活 DOM** 逐行判定（13 路由 × 暗/亮 = 26 captures，`docs/design-language.md:90` 记 ≈6.5 min）。⇒ **覆盖差 = 除 row 16/23 逻辑之外的全部实现行**（含 rows 40-45 文本/标记探针、rows 72-77 axis-1、row 20 失败态注入、row 57/58 导航与 URL 态、像素/发丝线/画布色等）。t70 F4 的结论成立，并且**今天仍然成立**（同一行号：`ci.yml:344`）。
* **为什么重要**：面板是本代交付面的一半，而它的验收判据（54 行）**只有人手动跑**；一旦某探针因页面结构变化失效，行会变 `not_measured` 而**没人看**（且见 T-1：即使看了也是 exit 0）。
* **可证伪修复判据（二选一，都要落纸）**：**(a)** CI 加一个**非每推必跑**的 job（`workflow_dispatch` 或 nightly）跑 `--check`（自带 `npm run build` + 临时 root daemon），负控 = 注入一条已知会 FAIL 的 CSS 断点 ⇒ 该 job 红；**(b)** 若不进 CI：把它登记为**每代固定仪式**（coverage declaration 里点名 `--check` + 记录一份带日期的读数：`checks N pass / M fail / K not measured` + 26 captures），并在契约/report 模板里给出触发条件。**owner**：panel/design-audit + CI 属主。

### T-3（medium）文档里的守卫读数已经过期（203/203 与 160/160 vs 今天的 495/495）
* **`file:line`**：`docs/design-language.md:88`（`node tools/design-audit.mjs --self-test    # 203/203`）· `docs/design/acceptance-report.md:29`（`--self-test → 160/160 pass`）。
* **可复现命令**：`cd panel && node tools/design-audit.mjs --self-test | tail -1`。
* **证据读数**：**495/495**（我的运行）。两处文档写的是 203 与 160 ⇒ 读者会以为守卫缩水或被替换过。
* **为什么重要**：这正是 t80 D-2 的同一类（文档里的冻结读数随代码漂移），只不过这次是**守卫自己的读数**；「引用任何数字前先跑一次」（`design-language.md:34` 自己就这么写）是唯一防线。
* **可证伪修复判据**：文档不再写死计数（改为「`--self-test` 全绿」+ 命令），或写死时带日期与哈希（本报告 §头部给了 `design-audit.mjs` 的 sha256 前 12 位与体积）。**owner**：design-language 的属主。

### T-4（medium）`build-panel.mjs`：**换出之后的失败面没有护栏**、且没有「换出后自检」
* **`file:line`**：`panel/scripts/build-panel.mjs:25-31`（`run()`：非零 ⇒ 打印 `dist untouched` 并 `process.exit`）· `:39-42`（`PANEL_BUILD_FORCE_FAIL` 故意失败钩子）· `:46-58`（唯一触碰 dist 的段）· `:51`（逐文件 `copyFileSync` 到 `dist/assets`）· `:54-55`（写 `index.html.tmp` 再 `renameSync`）· `:57`（**换出之后**删除不再被引用的旧 asset）。
* **可复现命令**：`cd panel && $env:PANEL_BUILD_FORCE_FAIL=1; npm run build; $env:PANEL_BUILD_FORCE_FAIL=$null`（读 `dist/index.html` 的 mtime 与 `build-panel: …dist untouched` 行）。
* **证据读数**：三步检查（i18n / `tsc -b` / `tsc -p e2e/tsconfig.json`，`:33-35`）与 FAIL 钩子（`:39-42`）**都发生在触碰 dist 之前** ⇒ 「失败 ⇒ dist untouched」这一条**成立**（我核过代码顺序，未复现失败路径）。**但**：① `:57` 的 `rmSync` 在**换出之后**执行，若它抛（Windows 上被浏览器/杀软占用是最常见的形状）⇒ 进程非零退出而**面板其实已经更新**（信号反向），且没有 `try/catch` 给出 `dist untouched` 那类清晰文案；② `:55` 的 `renameSync` 在 Windows 上遇目标被占用会抛 `EPERM` ⇒ **未捕获的栈**，并可能留下 `dist/index.html.tmp`（不在服务路径上，但下次会看到）；③ **没有任何换出后的自检**：没有断言「新 `index.html` 引用的每个 `/assets/...` 都确实存在于 `dist/assets`」，也没有断言「换出的 index 就是 staging 的那一份（哈希）」—— 现在的保证写在注释里（`:7-15`），不是断言。
* **为什么重要**：面板是唯一服务面；这个 wrapper 的存在理由（`:2-5`：`vite build` 会先清空 outDir，曾让 `/` 404 数秒）说明它的**失败面就是产品面**。今天有 3 条「失败 ⇒ dist untouched」的路径被设计过，**换出之后的 2 条没有**。
* **可证伪修复判据**：把 `:46-58` 包进 `try/catch`：失败时打印**已到哪一步**、清理 `index.html.tmp`、并明确区分「dist 未被触碰」与「index 已换出但housekeeping 失败」；**并**在 `:57` 之前加一条断言：解析新 `index.html` 里所有 `/assets/...` 引用，逐个 `existsSync`（缺一个就报错且**不删旧 asset**）。**负控**：在 `:51` 之后删掉一个 staged asset ⇒ 期望得到点名错误且 dist 仍是旧的可用版本；恢复 ⇒ 绿。**owner**：panel/scripts。

### T-5（medium）`run-e2e.mjs`：写权限旗标对了，但**没有信号处理**（孤儿树风险落在 AGENTS.md 的纪律上，而不是这个入口上）
* **`file:line`**：`panel/e2e/run-e2e.mjs:9`（`process.env.RUAGENT_E2E_ALLOW_WRITES = "1"`）· `:12-16`（`spawn("npx", ["playwright","test",…], { stdio: "inherit", shell: true, env: process.env })`）· `:17`（`child.on("exit", code => process.exit(code ?? 1))`）。
* **可复现命令**：`cd panel && npm run test:e2e -- --output=%TEMP%\pw95`（中途 Ctrl-C），随后按 AGENTS.md 的规矩**只按自己的临时输出路径**找孤儿：`Get-CimInstance Win32_Process | Where-Object CommandLine -Match 'pw95'`。
* **证据读数**：旗标**只在这里设置**（`AGENTS.md` 与 `e2e.yml:63-76` 的 Entry-point guard 都在钉这一点）✓；退出码正确传播 ✓。**但**：`shell: true` ⇒ 实际是 `cmd.exe /c npx playwright test …`，**杀掉父进程不会杀到 playwright/chromium 这层孙进程**；文件里**没有任何** `SIGINT`/`SIGTERM`/`exit` 钩子去收割子树（AGENTS.md 两段专讲这个坑：WMI 隐藏窗口 + 被杀的工具调用会留下后代）。
* **为什么重要**：e2e 是本代唯一会**启动浏览器**的门禁；孤儿 chromium 会占内存、占端口、并让下一轮「不可复现的红」更难定位（正是 t42 的教训族）。
* **可证伪修复判据**：入口里加信号/退出处理：收到 `SIGINT`/`SIGTERM` 时按**子进程树**结束（Windows：`taskkill /T /F /PID <child.pid>`；POSIX：进程组）；并在结束时断言「本轮的临时输出目录下不再有活跃的 chromium/node」。**负控**：跑一次并中途中断 ⇒ 上述查询返回 **0** 个匹配进程。**owner**：panel/e2e。

### T-6（medium）`release.yml`：工作流级 `contents: write` + 未按 SHA 固定的 `uses:` + 发布前**不跑任何门禁**
* **`file:line`**：`.github/workflows/release.yml:3-5`（`on: push: tags: ["v*"]`）· `:7-8`（**工作流级** `permissions: contents: write`）· `:28`（`actions/checkout@v4`）· `:29`（`dtolnay/rust-toolchain@stable`）· `:32`（`Swatinem/rust-cache@v2`）· `:40`（`cargo build --release`）· `:44-46`（`npm ci` + `npm run build`）· `:67`（`actions/upload-artifact@v4`）· `:78`（`actions/download-artifact@v4`）· `:83`（`softprops/action-gh-release@v2`）。
* **可复现命令**：`Select-String -Path .github/workflows/release.yml -Pattern 'permissions|uses:|tags'`（**只读**；本单**未**推 tag、**未**触发任何发布动作）。
* **证据读数**：`permissions: contents: write` 在**工作流**级 ⇒ **每个 job**（含跑 `npm ci`、`choco install`、`cargo build` 的构建 job）都拿到写权限；`uses:` 全部是**标签/分支**引用，其中 `dtolnay/rust-toolchain@stable` 是**移动分支**（连版本 tag 都不是）；发布 job（`:75-81`）**不跑测试**，只 download-artifact + `gh-release`。
* **为什么重要**：tag 推送是**唯一**能触发写权限的路径；一个被移动/被投毒的行动（或构建期被注入的依赖）在那种权限下能直接写仓库。而「发布前一跑就绿」的判据今天不存在 ⇒ 一个红提交打 tag 也会发出去。
* **可证伪修复判据**：① 工作流级降到 `permissions: contents: read`，只在发布 job 上给 `contents: write`；② `uses:` 固定到 commit SHA（至少把 `dtolnay/rust-toolchain@stable` 固定到版本或 SHA）；③ 发布 job 增加「同 SHA 的 CI 成功」判据（`workflow_run` 门或在本 job 内跑 `cargo test --workspace` + 面板 `npm run build`）——**负控**：在一个已知失败的 tag 上触发 ⇒ 发布 job 必须红。**owner**：release/ops（当前无具名属主，建议 captain 指定）。

### T-7（low-medium）`converge-near-duplicates.mjs` 的 `--verify` 在**空集**上通过（且没有任何自动调用点）
* **`file:line`**：`scripts/converge-near-duplicates.mjs:11-15`（`--dry-run` 只读 / `--execute` 唯一的写 / `--verify` 事后重扫）· `:115`（`readOnly = !has('--execute')`）· `:251-260`（`--verify` 分支，有重复则 `process.exit(1)`）。
* **可复现命令**：`node scripts/converge-near-duplicates.mjs --verify`（对一个 0 行 live 的临时库）。
* **证据读数**：`--verify` 的判据是「**发现**重复组就 exit 1」⇒ 对象集为空时**没有可发现的东西**，于是 exit 0；本代已在 `docs/plans/2026-09-26-memory-knowledge-closure.md:1262` 登记过这个「空集上的通过」。该脚本**没有任何自动调用点**（我排除了 `.agent-teams` 归档后，只有 `docs/plans/*` 提到它）—— 它是**操作工具**，不是门禁。
* **为什么重要**：这是第 11 条（「不存在一个不测量也能通过的状态」）在同一仓库里的**又一实例**；只要哪天有人把 `--verify` 接进 CI，它第一次就会以空集绿。
* **可证伪修复判据**：`--verify` 必须**先打印并断言对象集**（例如 `live paired rows = N`，`N == 0` 时要求显式 `--allow-empty`，否则 exit 2），并把 N 写进输出；负控 = 空库 + 无 flag ⇒ 非零。**owner**：mem-core（该脚本的属主）。

### T-8（low）三个 `scripts/` 工具**没有自动调用点** —— 但这一条与 `check-workflow-refs.sh` **不同类**，我建议**不判缺陷**
* **`file:line` + 读数**：`scripts/ruagent-canary.ps1:16`（「`data/daemon.pid` 是真相」）· `:58-61`（`Stop-Process -Id <它自己 root 的 pid>`；**拒绝**杀一个「活着但不是这个 root 的 daemon」的 pid，并要求 `name-match`）· `scripts/ruagent-post-switch-check.mjs:14`（**刻意不用** `/api/v1/recall`，因为它会写 `recall_log`）· `:35-41`（用法与退出码 0/1/2）· `:52`（`parseArgs` 支持 `--since/--marker/--require-wiki/--transcripts/--root/--json`）。
* **判定**：三者在仓库里**都没有自动调用点**（只有 `docs/**` 的引用），但它们是**操作工具**（人手动跑、判据与用法都写在文件头）；canary 的 pid 安全设计（不按名字/端口批量杀）与 post-switch-check 的「不用 `/recall`」都**符合本代纪律**。⇒ 我**不判缺陷**，只登记「它们不会自己跑，别拿它们当门禁」。
* **唯一建议**：把「谁是这些工具的触发器」写进 `scripts/README` 或各自文件头（canary 已写了）；不建议接进 CI（canary 会起子进程、post-switch-check 需要真实使用数据）。

---

## 2 已核对**成立**的守卫（同样的读数纪律，写下来免得下一轮重做）

| # | 断言 | 读数 |
| --- | --- | --- |
| P-1 | `build-panel.mjs` 的三步检查与 FAIL 钩子都在**触碰 dist 之前** ⇒ 「失败 ⇒ dist untouched」 | `build-panel.mjs:33-35`（i18n/tsc×2）+ `:39-42`（钩子）都在 `:43`（首个 dist 操作）之前 ✓ |
| P-2 | `run-e2e.mjs` 是**唯一**设置写权限旗标的地方 | `:9`；`e2e.yml:63-76` 的 Entry-point guard 独立钉住这一点（`package.json` 的 `test:e2e` 必须是 `node e2e/run-e2e.mjs`，且 `run-e2e.mjs` 里必须出现 `RUAGENT_E2E_ALLOW_WRITES`）✓ |
| P-3 | `--self-test` 是**两侧都测**的（对合成输入同时强制 PASS 与 FAIL） | 我跑的输出里逐行 `ok`，含多条 `must-FAIL:` 行（例：`row35 must-FAIL: 5 CSS breakpoints FAIL`、`row35 must-FAIL: a CSS value off the allow-list FAILS`、`the judge is single-sourced`）✓ |
| P-4 | 阈值解析的**健壮性**被断言（不是只靠注释） | `--self-test` 里有 `row35: the OLD wording still parses to 4` / `the NEW wording (§12.16) parses to 4` / **`an unrelated cell records a miss instead of silently returning the default`** ✓（这正是 t17 的落地形状；**但**见 T-1：它在 `--check` 的退出码里不载重） |
| P-5 | `BUILTIN` 常量被**明文标注**为「不是判据」 | `design-audit.mjs:77-104` 注释 + `:8133` 把 `thresholds.warnings` 汇进报告 ✓（可见性有了；载重见 T-1） |
| P-6 | `--check` 会把四个计数都打印出来（不是只打印绿） | `:8342`（`checks N pass / M fail / K not measured` + `pending`）、`:8344`（pending 逐条）✓ |
| P-7 | canary 不按名字/端口批量杀 | `ruagent-canary.ps1:58-61`（只杀自己 root 的 pid，否则**明确拒绝**并说明理由）✓ |
| P-8 | post-switch-check 不用会写库的取证入口 | `ruagent-post-switch-check.mjs:14`（明确写「deliberately NOT used — it would append a recall_log row」）✓ |

---

## 3 未验证猜想（**不是** finding，我没有做实验）

1. **Windows `renameSync` 到底会不会在这里 EPERM**：我说的是「目标被占用时会抛」这一平台事实 + 该处没有 try/catch；**我没有**在被 daemon/浏览器占用的条件下复现（需要先有服务者持有 `index.html`）。
2. **row 32 的 `pending` 降级（`:5782`）是否是漏洞**：它由 `row32CriterionLanded(ctx.thresholds)` 决定，我没有构造「契约措辞使其恒为 false ⇒ 恒 pending」的实验。
3. **`PANEL_BUILD_FORCE_FAIL=1` 会不会留下 `dist-staging`**：`rmSync(staging)`（`:43`）在钩子（`:39-42`）**之后** ⇒ 钩子触发时 staging 可能残留；它不在服务路径上，且下一次成功构建会清掉 —— 我**没有**实际触发验证。
4. **`--check` 在 CI 里跑得动吗**：它需要 build 过的 panel + 一个 daemon（我读到 `:8162` 的「daemon not reachable」分支）—— 我在 T-2 推荐 `workflow_dispatch`/nightly 时给的是**估算**，没有实测 CI 下的 6.5 分钟与浏览器依赖。
5. **`release.yml` 的 `contents: write` 是否真的被构建 job 用到**：从 YAML 看是工作流级继承，但 GitHub 的精确继承语义与「哪个 job 实际需要写」我没有逐 job 审计其 steps 的 API 调用。

---

## 4 未覆盖范围（如实写）

1. **`design-audit.mjs` 的 54 条判据本身没有逐条审**（8,359 行）：我审的是**它的模式、退出码、阈值回落路径、报告面**，不是每条判据的算法正确性；我只抽读了 row 20 失败态注入（`:111-127`）、`:61-75` 路由表、`:8338-8351` 的判定/退出面。
2. **`--check` 我一次都没跑**（需 build 过的 panel + daemon，≈6.5 分钟，且本单只允许 `--self-test` 与 `npm run build`）⇒ T-2 的覆盖差是**从工具自己的帮助文字与代码**得出的，不是我实测的行集合。
3. **`.github/workflows/{ci,e2e}.yml` 的其余步骤**只按 t92 的读数引用，本轮没有重新逐行审。
4. **`panel/tools/lib/*.mjs`**（`thresholds.mjs` / `png.mjs` / `contract.mjs`）**没有读**——阈值解析的实现细节（`resolveContract`）我只看到注释与 self-test 的断言，没有审代码。
5. **`release.yml` 未触发任何动作**：我**没有**推 tag、没有建 tag、没有 dispatch workflow；结论全部来自文件字节。
6. **三个 `scripts/` 工具未运行**（canary 会起进程、post-switch-check 需真实使用数据、converge 会读活库/需临时库）⇒ T-7/T-8 的判据来自代码与既有文档。

---

## 5 建议的处置顺序

1. **T-1**（退出码载重：warnings + `not_measured`）—— 它把「可见的谎言」变成「会红的门禁」，是 T-2 的前提。
2. **T-2**（把 `--check` 接进非每推必跑的 CI，或登记为带日期的固定仪式）—— 决定这个守卫到底算不算门禁。
3. **T-6**（发布面权限与固定 SHA + 发布前门禁）—— 唯一有**写仓库**能力的路径。
4. **T-4 / T-5**（构建换出后自检、e2e 信号收割）—— 都是「失败面/孤儿」类，改动小、负控清楚。
5. **T-3 / T-7 / T-8**（文档计数、空集绿、操作工具登记）—— 便宜，顺手做。
