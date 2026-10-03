# gen4 只读侦察（F2 前置）：把 `design-audit.mjs --check` 接进流水线

> ## 修订记录（2026-10-04，t141）—— 一处坐标错了，旧值保留在下方
>
> 本报告的**读数全部不变**，只有 `exitDecision` 的**坐标**错了（t134 的独立验证带出，t141 收口）：
>
> | 位置 | 旧值（保留，不删） | 更正后（回读自 `panel/tools/design-audit.mjs`） |
> | --- | --- | --- |
> | 本文 §0 第 1 条 · §4 第 8 条 · §7 第 8 条 —— 原始行号 `:9` / `:65` / `:185`，**插入本说明后**为 **`:20` / `:76` / `:196`**（行号本身也会漂移，这正是本记录存在的原因） | `exitDecision`，**`:8431-8442`** | **定义在 `:8105-8111`**（三个致命源在 `:8107-8109`）；**调用点在 `:8422`**；`:8431-8442` 是文件末尾的 `if (process.argv[1] …)` **入口块**，而且这个范围还**越过了文件末尾**（入口块是 `:8433-8440`，其注释在 `:8431-8432`；文件共 **8440** 行） |
>
> 另一份报告（`gen3-panel-tooling-repair.md:21`）把入口块写作 `:8431-8440`（并标明是「入口」）—— 那与我的实测一致（`:8431-8432` 注释 + `:8433-8440` 块），**不属于**同一类错误。
>
> **声明本身是真的**（三种致命源 = `fails > 0` ∨ `contractWarnings.length > 0` ∨ `unmeasured > 0 && !allowNotMeasured`，见 `:8107-8109`）—— **只有坐标错**。为什么值得记：本报告引用了它三次，而三次都是同一个错值；`audit.yml` 的文件头也照抄了这个错值，已在 t141 一并改正（`docs/design/reviews/gen4-audit-gate-comment-repair.md`）。
> 下文三处旧值**原样保留**以便对照，读到时请以本表为准。

> 单号 **t122**（work，**只读侦察**）· 成员 `wiki` · attempt **2** · 2026-10-02 23:0x–23:5x（+08:00）
> inScope：**本报告**。`panel/**`、`.github/**`、`crates/**`、`scripts/**` **一行未改**（所有改动都是「建议」）。
> 纪律：**未** push / dispatch / rerun / cancel / 建 tag；**未**跑任何 rust 构建（t118 占着唯一构建名额）；**未**触碰 127.0.0.1:8787 的活守护进程与 `~/.ruagent`（只用临时 root `%TEMP%\ruagent-t122-root` + 临时端口 **8897**，收尾已停已删，见 §7）。

## 0 结论（一句话 + 两条最重要的读数）

1. **它今天就能跑，而且今天就是红的**：当前树上跑真 `--check` ⇒ **exit 1**，`[audit] 24 captures in 620.7s · checks 44 pass / 11 fail / 10 not measured` ⇒ **不需要构造就有能红的证据**；**真正的 blocker 是相反的**：它今天**绿不了**（11 条真 fail + 10 条 not_measured），而工具**没有 baseline / 已知失败清单机制**（`exitDecision`，`:8431-8442`）⇒ **直接做成阻塞门 ⇒ main 永久红**。
2. **修正 captain 的两个前提**：① **`t86` 的 inScope 并不覆盖 `.github/workflows/`** —— 平台状态里是 `inScope:['docs/design/reviews/gen3-ignore-guard.md']`，它「不可认领」的原因是**依赖里有终态失败单**（`dependencies:["t65","t66","t77","t85"]`，其中 t66 失败、t77 未完成），**不是**因为它占着 workflow 目录 ⇒ F2 用**新文件** `.github/workflows/audit.yml` **今天就能落**（§5）。② `--check` **不需要** `RUAGENT_E2E_ALLOW_WRITES`（失败态注入是浏览器内 `page.route` 拦截，不是真写 API，`:132/:204/:225`），但它**继承了 t107 修过的那个危险默认**：`E2E_BASE_URL ?? "http://127.0.0.1:8787"`（`:401`）。

## 1 Q1 运行需求（逐条给出处行号）

| # | 需求 | 出处 | 本次实测 |
| --- | --- | --- | --- |
| ① | **一份已构建的 panel**，由 **daemon 静态服务**（工具自己读 `panel/dist`，并把 dist/src 的 mtime 写进产物 `meta.dist`） | `:3705-3708`（`distDir`/`htmlPath`）· `:3748`（`newestDist`）· `:8334-8340`（`meta.dist{buildId,entry,staleSrc,newestSrc,newestDist}`）· `:474`（HELP: `--base-url=URL  daemon serving panel/dist`） | `panel/dist/index.html` = **10/01 10:42:58**，`panel/src` 最新 = **10/01 09:54:38** ⇒ **dist 更新 ⇒ 复用，未重建**（本单没有跑 `npm run build`）。`dist` 由我的临时 daemon 服务：`GET http://127.0.0.1:8897/` = 200，入口 `index-CUe1BZdC.js` |
| ② | **一个 daemon**（工具打它的 HTTP API + 它服务的 SPA 路由；默认目标写死为活守护进程端口） | `:401` `baseUrl: process.env.E2E_BASE_URL ?? "http://127.0.0.1:8787"` · `:425` `--base-url` 覆盖它 · `:474` HELP | 我用 **`D:\rust_cache\debug\ruagent.exe`（10/02 23:05，未新编）× `--root %TEMP%\ruagent-t122-root` × `--addr 127.0.0.1:8897` × `RUAGENT_PANEL_DIST=<repo>\panel\dist`**；`/api/v1/health` = 200。**root 的「空/有数据」直接改变能测什么**（见 §3 的数据依赖） |
| ③ | **浏览器：Playwright 自带 Chromium**（不是 puppeteer、不是 CDP 手写、不是截图工具） | `:43` `import { chromium } from "@playwright/test"` · `:8253` `chromium.launch()` · `:8254` `newContext({viewport, deviceScaleFactor:1, locale:"zh-CN"})` · `:8309` `browser.close()` | 本机 `@playwright/test` **1.63.0**，浏览器缓存已有 `chromium-1243`（09/15）⇒ **本机零安装**；**CI 必须安装**（现成先例：`e2e.yml:169` `npx playwright install chromium --with-deps`） |
| ④-a | **`E2E_BASE_URL`**（与 e2e 同一变量；**默认值是活守护进程**） | `:401` | 本次**没用到**（我显式传 `--base-url`）；**建议：永远显式传 `--base-url`**，见 §4 的护栏与 §6 未测项 |
| ④-b | **不需要写标志**：失败态注入是**浏览器内请求拦截**，不是真写 daemon | `:132` `page.route(routePattern, …)` · `:204/:225` `page.unroute(...)` | 我对**自己的**临时 root 做了一次只读指纹：`memories=0 · chunks=0 · recall_log=0`（db 4096 B）⇒ **本次没有产生记忆/召回写入**（≠「永不写」，见 §6） |
| ④-c | **`--out` 必须是仓库外的目录**；**省略 `--out` 会把产物写进工作树** | `:8152-8172`（显式 `--out` 在仓库内 ⇒ **exit 2**）· `:8175` `docs/screenshots/audit-run`（省略时）· `:8385-8386` 写 `metrics.json`/`report.md` | 我传 `--out=%TEMP%\ruagent-t122-audit`，工具打印 `--out resolves to … (outside the repo)`；**CI 应传 `--out=$RUNNER_TEMP/audit` 并 upload-artifact**（否则工件落在工作树里、且默认路径是仓库内的） |
| ④-d | **代理**：本机系统代理指向死端口 `127.0.0.1:7890`（AGENTS.md 记过） | — | **本次未设任何 `NO_PROXY`，node 与 Chromium 都通**（`8897`/`8898` 均 200/命中）⇒ 这次不是问题；**CI 上按 AGENTS.md 的 host 列表设 `NO_PROXY` 属廉价保险，但未测**（§6 第 2 条） |
| ④-e | **前置缺失会自己红且点名**（不需要另写探针） | `:8436-8439`（catch ⇒ `design-audit failed: …`，`process.exit(2)`）；工具自报信息 | 死端口控制：**exit 2，0.4s**，原话 `daemon not reachable at http://127.0.0.1:8899 (no panel to audit). Start it with: cargo run -p ruagent-cli -- serve` |

## 2 Q2 今天的真判词（真跑，原始读数）

**命令**（在这个 repo 上，只读；临时 root/端口/产物目录都在 `%TEMP%`）：

```
cd panel
node tools/design-audit.mjs --check --base-url=http://127.0.0.1:8897 --out=%TEMP%/ruagent-t122-audit
```

**读数**（原文行，逐字）：

```
[audit] --out resolves to C:\Users\19410\AppData\Local\Temp\ruagent-t122-audit (outside the repo)
[audit] 24 captures in 620.7s · checks 44 pass / 11 fail / 10 not measured
[audit] failing rows: 2, 4, 6, 7, 12, 13, 14, 18, 20, 38, 75
[audit] not measured: 10 row(s) #25, #28, #31, #33, #46, #47, #49, #50, #54, #77 — exit 1 (pass --allow-not-measured to waive; the waiver is named)
[audit] warning: #task skipped: could not resolve a task id (/api/v1/tasks empty or unreachable)
[audit] warning: chat/dark: hash mismatch: asked #chat, got #chat?agent=claude
[audit] warning: graph/dark: no content marker found (.graph-canvas)
[audit] warning: §12 rows with neither a check nor a stated reason: 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71
[audit] warning: .nav-badge 本轮未渲染（inboxCount = 0）：它对白字 3.20:1 的对比度失败对本次审计不可见
```

- **退出码：1**（不是 0，也不是 2）· **耗时：624.6s（10.4 分钟）** · **captures：24**（= 13 路由 × 2 态 **减去** 2 个 `#task` capture）。
- **11 条 fail 的具名原因**（工具自己打的，摘）：`#2 有效彩度像素占比 — stats/dark=0.98%` · `#4 描边容器 home/dark=3 / ≤1` · `#6 ≥18px 文本 board/dark=1 / ≥3` · `#7 最大字号 home/dark=24px / ≥32px` · `#12 内联 style settings/dark=79 / ≤50` · `#13 暗色对比度 settings/dark=6` · `#14 亮色对比度 home/light=3` · `#18 命中目标 settings/dark 内容区 13/≤10` · `#20 settings/dark 失败态「恢复 ✗」` · `#38 home 的 `#chat` 例外 3 处` · `#75 memory/dark Tab 40 次 29 个元素`。
- **10 条 not_measured 逐条点名**：`#25 #28 #31 #33 #46 #47 #49 #50 #54 #77`。
- **数据依赖（这条会改变计数，做护栏必须知道）**：空 root 下 `#task` 路由**整条 skip**（`/api/v1/tasks` 为空）⇒ captures = **24** 而不是 26；`#graph` 找不到 `.graph-canvas`；`.nav-badge` 因 `inboxCount=0` 未渲染（登记为盲区）。**⇒ 「captures 数」不是常数**。

**对照：现有 CI 那一条腿（`--self-test`）**：`ci.yml:457-459`（`working-directory: panel`，`run: node tools/design-audit.mjs --self-test`，注释写明「433ms, no browser, no daemon, no network: the self-test branch never reaches chromium.launch」）；我实跑 **exit 0 / 0.54s / `self-test: 495/495 pass`**。⇒ **自检不碰 DOM，`--check` 才是真测量**，两者不能互相替代（captain 的判断成立）。

## 3 Q3 能红性（今天能不能红：能，三种证据；重点在「能不能绿」）

| 证据 | 构造 | 读数 |
| --- | --- | --- |
| **A. 真跑就红**（最强） | 当前树 + 空 root 的临时 daemon | **exit 1**；`44 pass / 11 fail / 10 not measured`；failing rows 具名 |
| **B. 判词真的判 DOM**（负控：换掉被审对象 ⇒ 判词改变） | 用 `python -m http.server` 供一个**空壳页**（只有 `<div id="root">`），`--check --routes=home --modes=dark --no-shots` | **exit 1，53.9s**；`1 captures in 51.0s · checks 29 pass / 6 fail / 30 not measured`；failing rows **6, 7, 24, 35, 36, 56**（与 A 的集合不同 ⇒ 它真的在读被审页面，不是「永远同一张表」） |
| **C. 前置缺失就红且点名** | `--base-url=http://127.0.0.1:8899`（无人监听） | **exit 2，0.4s**，原话 `daemon not reachable at … (no panel to audit)` |

**⇒ 它是可红的门，能红性不是问题。真正的 blocker 是「今天绿不了 + 没有基线机制」**：

- `exitDecision`（`:8431-8442`）只有三种致命源：`fails > 0` ∨ `contractWarnings.length > 0` ∨ `unmeasured > 0 && !allowNotMeasured`。**没有** baseline / known-failures / `--max-fail` 之类的东西 ⇒ 今天把它设成**阻塞**门，等于让 main 停在 `11 fail + 10 not measured` 上。
- `--allow-not-measured`（`:417` / `:466` / `:8416-8418`）会**一次性豁免全部** not_measured（它确实逐条打印被豁免的 `#n`，但「豁免 10 条」和「豁免 3 条」在退出码上一样）⇒ **CI 里不许裸用它**；若要用，必须把被豁免的名单**钉死成期望值**（§4 护栏 2）。
- **⇒ 落地形状必须是「先非阻塞（advisory） + 计分与工件齐全 + 负控每次都跑」，等 11 fail 与 10 not_measured 收敛到一份**具名**清单后再改阻塞**。这是本单最重要的一条建议。

## 4 Q4 接入形状：**新增 `.github/workflows/audit.yml`**（不改 `ci.yml`）

**为什么用新文件**：① `ci.yml` 在每次 push 上已经 ~25 分钟（本代 t111 的读数：ubuntu 绿 run 542s + windows 552s + panel 35s）；② t86 的实际状态见 §5 —— 它与 workflow 目录**并不冲突**，但「让 push 关键路径保持不动」这个理由独立成立；③ 新文件与 `ci.yml`/`e2e.yml` 的**并发写入面为零**。

### 4.1 触发器（建议）

- `workflow_dispatch`（人可随时点，**必须有**：这是接进流水线的第一步读数来源）；
- `schedule: cron: "0 3 * * 1"`（每周一 03:00 UTC，慢但便宜，用来测「产品漂移」而不是「本次 push」）；
- `push`：**只对会让判词失效的路径**开 —— `paths: ['panel/src/**','panel/tools/**','panel/src/index.css','docs/design/MASTER.md','docs/design/contracts/**']`，**且只对 main**；
- `concurrency` 与 ci.yml 同族（`cancel-in-progress: true`），避免同一个 tag/推送堆叠。

### 4.2 `timeout-minutes`（依据，不拍脑袋）

- **实测**：全量 `--check` = **620.7s capture / 624.6s wall（10.4 min）**（本机，Windows，24 captures）。
- **CI 上还要加**（都能从本代已有读数取）：`cargo build`（e2e 那条约 **246s** 的 Build 腿）· panel build（**22s**）· `npx playwright install chromium --with-deps`（**未测**）· daemon 启动 + `curl` 健康等待（~5s）。
- **取上界按 t111 确立的判据**：**用「完整跑完」的读数取，不用被截断的短读数** —— 这里恰好有反面样本：**stub 负控 53.9s、死端口控制 0.4s**，按它们取上界会把真跑杀在中途。
- **⇒ `timeout-minutes: 45`**：`624.6×3 ≈ 31 min` + 前置 ~6 min ≈ **37 min**，向上取整到 45（约 1.2× 余量；GitHub 默认 360 min 会让「挂住」与「慢」不可区分，同 t111）。**若要更省**：把 `--routes` 拆成两三个 lane（每条路由 ×2 态 ≈ 50s），**但子集不是全站结论**（工具自己会打 warning），不能拿它当门。

### 4.3 四道护栏（都可机械判定）

1. **计数（不许空跑绿）**：解析工具自己的汇总行 `[audit] (\d+) captures in ([\d.]+)s · checks (\d+) pass / (\d+) fail / (\d+) not measured`，要求
   - 汇总行**必须存在**（解析不到 ⇒ exit 1，这条封死「没跑却报绿」）；
   - `pass+fail+not measured == 65`（本次两次真跑都是 65：44+11+10 与 29+6+30 ⇒ **检查集大小是常数 65**）；
   - `captures >= 24`，并把实测值打印出来（**24 vs 26 取决于 `#task` 是否有数据** ⇒ 期望值必须由 fixture 固定，见 4.5）。
2. **跳过必须点名**：**不传 `--allow-not-measured`**；改把工具自己打的 `not measured: (\d+) row\(s\) (#\d+(, #\d+)*)` **逐条写进 step summary**；若将来必须豁免，走工具自己的豁免（它会打印豁免名单）**并在 workflow 里把被豁免集合钉成字面量**，集合不等 ⇒ exit 1。
3. **能红的负控（每次都跑）**：单独一步跑死端口控制并断言**非 0**：
   `node tools/design-audit.mjs --check --routes=home --modes=dark --no-shots --base-url=http://127.0.0.1:1`
   —— 本次实测 **exit 2 / 0.4s**（`daemon not reachable`）。这条让「门能红」在每次运行里被证明，而不是被假设（与 t110「跳过必须点名」同族）。
4. **`not_measured` ⇒ exit 1（默认语义）不许被吞**：`set -o pipefail`；**不许** `|| true`；**不许**把 `--check` 的退出码只写进日志而不传播（除 4.6 的 advisory 阶段，那时也要**在同一步里打出 `::warning::` 并把计数写进 summary**，不能静默）。

### 4.4 上报

- `actions/upload-artifact`：`--out=$RUNNER_TEMP/audit` 的 `metrics.json` + `report.md` + 24 张 PNG（**`if: always()`**，失败时的产物才是证据）；
- `$GITHUB_STEP_SUMMARY`：汇总行 + `failing rows: …` 的**逐条**失败理由（工具 stdout 已经逐条打印，直接搬运）；
- 建议把 `metrics.json` 里的 `meta.dist.staleSrc/newestSrc/newestDist` 也写进 summary（**判词是否针对当前 src** 是这条门的前提）。

### 4.5 fixture 配方（可照抄 e2e.yml 的现成形状）

```
工作目录 panel
1) cargo build -p ruagent                       # 前置（t118 的构建名额；本单未跑）
2) cd panel && npm ci && npm run build          # panel/dist（本次复用已存在的 dist，未重建）
3) 起 daemon：--root $RUNNER_TEMP/audit-root --addr 127.0.0.1:8898 + RUAGENT_PANEL_DIST=<repo>/panel/dist
   （**显式命名目标**：不设 E2E_BASE_URL、也不依赖工具那个 8787 默认值 —— 见 §1 ④-a）
4) curl -sf .../api/v1/health 轮询就绪
5) npx playwright install chromium --with-deps
6) node tools/design-audit.mjs --check --base-url=http://127.0.0.1:8898 --out="$RUNNER_TEMP/audit"
```
**空 root 是「便宜但会 skip」的 fixture**（本次实测：24 captures、`#task` skip、`#graph` 无画布、`.nav-badge` 盲区）。**若想让 26 个 capture 都跑到**，需要在起 daemon 后造一条 task 与一条待处理权限（`e2e.yml:114-140` 的 mock agents 配方是现成先例）——**这会把期望的 captures 从 24 推到 26**，护栏里的期望值必须跟着 fixture 走。

### 4.6 落地顺序（建议）

**第 1 步（可立刻做）**：`audit.yml` 全量上，但**审计步 `continue-on-error: true`**（advisory），四道护栏照跑（护栏 3/1/2 只依赖真实输出，不依赖审计是否红）⇒ 得到「今天的 11+10」进入 CI 历史的**稳定读数**；
**第 2 步**：`panel/**` 侧把 11 条 fail + 10 条 not_measured 收敛（那是产品/仪器单，不是本单）；
**第 3 步**：把 `continue-on-error` 去掉，并把「已收敛的具名清单」钉进护栏 2/4 —— 这时它才是真门。

## 5 Q5 t86 冲突：**核实结果与前提不符（是「依赖终态失败」，不是「占着 workflows 目录」）**

平台状态（读 `.agent-teams/ruagent-mem-gen2/team.json`，只读）里 t86 的原文：

```
t86 | status=pending | assignee=recall | kind=implementation
inScope: ['docs/design/reviews/gen3-ignore-guard.md']
dependencies: ['t65','t66','t77','t85']
objective: 把契约第 11 条机械化：新增「#[ignore] 必须带点名 env 的理由」守卫（含负控证明它能红），并在同一次提交里修掉今天那些裸 #[ignore]
```

- **inScope 是单个 docs 路径**，**不包含** `.github/workflows/`（更不是「整个目录」）⇒ **按一文件一写者的规则，F2 落地 `.github/workflows/audit.yml` 与 t86 不冲突**。
- t86 **不可认领的原因**在 `dependencies` 里：`t65/t66/t77/t85`，而平台状态显示 **t66 失败且无后续修复、t77（repair）未完成** ⇒ 依赖链上有终态失败单 ⇒ 它不会被解锁。**这一点 captain 说对了**（只是原因不是 inScope）。
- **仍然存在的、真实的重叠面**：t86 的守卫**要落地时**必然想碰 `.github/workflows/scripts/`（新脚本）与 `ci.yml`（接线）——这两处**都不在它声明的 inScope 里**，所以它将需要一次**范围扩展**。
- **建议处置（优先级从高到低）**：
  1. **F2 现在就落 `audit.yml`**（新文件；不碰 `ci.yml`、不碰 `.github/workflows/scripts/*`）——这与 t86 的当前声明范围零重叠；
  2. **不要**顺手改 `check-workflow-refs.sh`：新 workflow 只要**能被它扫过**（路径引用可跟踪 + 表达式合法）就不需要改它（本次 t111/t112 的经验：它已经在扫三份 workflow 并 exit 0）；
  3. **若 captain 要扩展 t86 的 inScope 到 `.github/workflows/`**：那才需要串行 —— 先 t86（它会改 `scripts/` + `ci.yml`），再 F2；或者把 t86 的范围**只**扩到 `scripts/`，让 `ci.yml` 的接线单独成单（这样 F2 仍不受影响）。

## 6 Q6 成本（wall-clock / CI 分钟 / 与 e2e 的关系）

| 项 | 实测 | 与现有 job 的关系 |
| --- | --- | --- |
| 全量 `--check`（24 captures） | **624.6s wall（10.4 min）**，其中 capture 段 620.7s | **≈ 1.25× 整个 e2e job（~500s）**；CI 分钟 ≈ **11 min** |
| 前置（CI 侧） | `cargo build` ~246s · panel build 22s · playwright install（**未测**）· 起 daemon ~5s | e2e job 里同样有这些（那 500s 已含 Build 246s + panel 22s + doctor 37s + Playwright 119s） |
| **合计（估）** | **≈ 15–17 CI 分钟** | 约为 e2e job 的 **1.5×** |
| 便宜腿：`--routes=home --modes=dark --no-shots` | **53.9s**（1 capture） | 适合做**负控/冒烟**，**不能**当全站门 |
| 便宜腿：死端口前置检查 | **0.4s** | 适合当护栏 3 |
| 现有 CI 腿 `--self-test` | **0.54s**（495 断言，无浏览器） | 它已经在 `ci.yml` 里；**不替代** `--check` |

**⇒ 成本结论**：把 `--check` 接进来 ≈ **每周 +11~17 CI 分钟**（若只在 schedule + paths 上跑，对 push 关键路径是 **0**）。用 `--routes` 子集压成本会让判词**不是全站结论**（工具自己会 warning），不建议作为门。

## 7 纪律回执与未测项

**我做了什么（全部只读）**：读 `panel/tools/design-audit.mjs` 与 `panel/tools/lib/*` 的调用面 · 读 `ci.yml`/`e2e.yml` 的相关步骤 · 读 `team.json` 的 t86 记录 · 起**自己的**临时 daemon（PID **32496**，`%TEMP%\ruagent-t122-root`，端口 **8897**，`RUAGENT_PANEL_DIST` 指向已经存在的 `panel/dist`）· 起**自己的**临时静态服务（PID **26032**，端口 8898）· 跑 1 次全量 `--check` + 2 次控制跑 + 1 次 `--self-test`。

**收尾核对**（原文输出）：
```
t122-daemon.pid -> PID 32496 stopped, alive now: False
t122-stub.pid   -> PID 26032 stopped, alive now: False
removed: %TEMP%\ruagent-t122-root / ruagent-t122-audit / ruagent-t122-stub-out / ruagent-t122-dead-out / ruagent-t122-stub
removed: %TEMP%\t122-daemon.pid / t122-stub.pid / ruagent-t122-daemon.out / ruagent-t122-daemon.err / t122-audit-run1.log
ports 8897/8898 listening: False / False
node/chromium orphans matching my temp path: 0
live daemon on 8787 untouched and still listening: True
```
- **`%TEMP%` 里另有 4 个 `t122*` 文件（`t122-run1.log`/`t122-run2.log`/`t122z-apply.py`/`t122z.json`，mtime 全是 09/24/2026）——那是上一代的同名任务留下的，不是我的，按纪律未删**（内容里引的是 t107/t124 等上一代单号）。
- 仓库写入集合 = **本报告**（`git status --porcelain -- panel .github crates scripts docs/design/reviews` 里只出现同伴的在途改动：`crates/mcp/src/lib.rs`、`crates/mcp/tests/roundtrip.rs`、`docs/design/reviews/gen4-version-points-impl.md`）。
- **未**跑 rust 构建（t118 占着构建名额）；daemon 二进制用的是**已存在**的 `D:\rust_cache\debug\ruagent.exe`（10/02 23:05）。
- **未**触碰活守护进程（8787 自始至终在听，且我只对自己的 8897 发过请求）与 `~/.ruagent`（唯一的库指纹是**我自己的**临时 root）。

**未测项（含原因，不许当成已测）**：

1. **CI 侧（ubuntu + chromium）的判词未测**：我这份读数出自 **Windows + chromium-1243**。像素类行（`#1/#2/#4/#29/#30` 等）依赖浏览器光栅器与字体 ⇒ **同一份代码在 CI 上可能给出不同的 fail 集合**（工具自己也在 `:498` 注明 pixel/screenshot 行依赖光栅器）。⇒ 第 1 步的 advisory 阶段正好是取这份 CI 读数的地方。
2. **代理未测**：本次没设 `NO_PROXY` 也全通；CI 是否需要它**未测**（AGENTS.md 的 host-list 保险未在本次验证）。
3. **`npx playwright install chromium --with-deps` 在 CI 的耗时未测**（我的本机已有浏览器缓存 ⇒ 零安装）。
4. **有数据的 root 未测**：我只跑了「空 root」fixture。26 captures / 有 task / 有 inboxCount 的 fixture **会改变计数**与部分行的 not_measured 集合；护栏里的期望值必须重新标定。
5. **「审计是否会写 daemon 的数据」只做了一次窄读数**：我的临时 root 在跑完后 `memories=0 / chunks=0 / recall_log=0`，且失败态注入是 `page.route` 拦截（`:132`）——但这**不构成**「永不写」的证明（面板自发的写请求没有被逐一枚举）。**⇒ 建议：审计只对一次性 root 跑**（与 t107 给 e2e 的那条规则同族），并在 `panel/tools/design-audit.mjs` 侧（**不在本单 inScope**）考虑拒绝「活 root」。
6. **工具的默认 base URL 未被行使**：我从头到尾显式传 `--base-url`；`:401` 那个 `E2E_BASE_URL ?? 8787` 的默认值在**本机有活守护进程**时是危险的（t107 修的正是这个形状）⇒ 建议在工具侧（另立单）把它改成**必需或拒绝**。
7. **子集运行不是全站结论**：`--routes` 的 53.9s 读数只覆盖 home/dark ⇒ 不能外推到全站。
8. **没有 baseline 机制的实测**：我只读了 `exitDecision`（`:8431-8442`）与 HELP（`:466`），没有构造「已知失败清单」来验证（工具没有这个开关 ⇒ 也就无从构造）。
