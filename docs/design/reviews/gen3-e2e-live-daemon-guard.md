# gen3 e2e 默认打活守护进程：`E2E_BASE_URL` 缺省必须安全 + 写真实数据必须过闸门（t107）

> 单号 t107（work）· 成员 `wiki` · attempt 1 · 2026-09-29 05:0x–05:2x（+08:00）
> 来源：我在 t104 对 `consumption.spec.ts` 红点的取证（**全程只读、未改任何文件**）。
> inScope：`panel/playwright.config.ts` · `panel/e2e/consumption.spec.ts` · `panel/e2e/run-e2e.mjs` · 本报告。
> **未碰** `panel/src/**`（实现面归 integ）· `crates/**` · `.github/workflows/**`。
> 纪律：**未启停 pid 79984、未写活库**（活库只有 `mode=ro` 读取）；e2e 一律走仓库入口；临时 root/端口/PID **按具体路径逐个删除**（没有用 `%TEMP%\t10*` 这类通配）。

## 0 一句话

把「不设 `E2E_BASE_URL` 就等于驱动（并写）操作者的活库」这个默认**关掉**：入口点在未命名守护进程时**什么都不跑**（playwright 根本不启动），配置层再抛一次错；同时把这条**会写真实数据**的 spec 挂到仓库既有的 `writeAccess()` 闸门下（与 `registry.spec.ts` 同形），并让它的注释与行为一致。判据没松（三态读数与 RV-D-1 控制见 §3/§4）。

## 1 根因链（三条证据，逐条可复核）

1. **失败原文**：`panel/e2e/consumption.spec.ts:92` `expect(row?.cite_coverage).toBeNull()` ⇒ **`Received: undefined`**（响应里**没有这个键**，不是 `null`）。产物：`panel/test-results/run-74076-1790628896033/consumption-Wiki-page-a-pa-3e43c-ding-shows-coverage-unknown/error-context.md`。
2. **活守护进程是旧的**：pid 79984 = `D:\rust_cache\debug\ruagent.exe`，镜像 **09/27 05:35:34**，早于 `0da0cb6`（09-29 04:25 +08:00）—— `cite_coverage` 正是该提交加进 `WikiPageInfo` 的。把**同一个镜像**跑在我自己的 temp root/端口 8900 上：`GET /wiki/pages` 的行 **`has cite_coverage key: False`**。
3. **活库指纹（只读 `mode=ro`）**：`~/.ruagent/data/ruagent.db` 里有该 spec 写入的 **1 条探针记忆** + **4 条 query 含 `consumption` 的 `recall_log` 行**，而该 spec 是 **09-29 04:25 才进仓库**的 ⇒ 这些指纹**只能来自今天那几次 e2e 运行** ⇒ **e2e 真的打了活守护进程并写了它**。

**机制**：`panel/playwright.config.ts:36`（改前）`baseURL: process.env.E2E_BASE_URL ?? "http://127.0.0.1:8787"` + `panel/e2e/run-e2e.mjs` 不设 `E2E_BASE_URL` ⇒ 「只是跑个 e2e」= 驱动并写操作者的真实数据。

## 2 改动（三个文件；逐条给读数）

| # | 文件 | 改前 | 改后 |
| --- | --- | --- | --- |
| 1 | `playwright.config.ts` | `baseURL: process.env.E2E_BASE_URL ?? "http://127.0.0.1:8787"`（**静默默认 = 活守护进程**） | 新增 `e2eBaseUrl()`：`E2E_BASE_URL` 有值 ⇒ 用它；**未设且 `CI`** ⇒ 用 8787（**按 CI 判，不按端口/URL 判**：CI 的守护进程由 `e2e.yml` 在几步之前用 runner 临时 root 起好，不存在操作者数据；这条分支的存在正是**CI 无需改 workflow** 的原因）；**未设且非 CI** ⇒ **抛错**，错误信息里给出「起一个临时守护进程 + 命名它」的完整命令与报告路径 |
| 2 | `run-e2e.mjs` | `RUAGENT_E2E_ALLOW_WRITES=1; spawn(playwright)` | **开跑前先判目标**：未命名 ⇒ 打印可执行的说明并 `exit(2)`，**playwright 从未启动**（比 spec 级 skip 更早，因为退出发生在任何 spec/hook 之前）；打印生效目标；写闸门默认仍由**入口点**武装，唯一保留的调用方取值是显式 `RUAGENT_E2E_ALLOW_WRITES=0`（**只能让这次运行更只读**，存在的理由是让闸门本身**能通过仓库入口被检验**——需要一个裸 `npx playwright test` 的控制等于没人能跑的控制） |
| 3 | `consumption.spec.ts` | 头部写着「a THROWAWAY root in this task's run」，但实际会打到活守护进程；`beforeAll` 无条件写 | 头部**改成事实**：写什么（记忆 + `recall_log` + 文档 + 实体）、`afterAll` 只删 wiki 页与文档、记忆与 `recall_log` **故意留着**（在临时 root 上它们是「这次跑过」的证据）；`import { writeAccess } from "./write-guard"` + `const access = writeAccess(); test.skip(!access.allowed, access.reason)`（**与 `registry.spec.ts:21-22` 同形**）；`beforeAll`/`afterAll` 各自**先判闸门再动手**（删除也是写） |

`git diff --stat`：`consumption.spec.ts +33/-…` · `run-e2e.mjs +58/-…` · `playwright.config.ts +46/-…`（合计 3 files changed, **125 insertions(+), 12 deletions(-)**）；`node --check panel/e2e/run-e2e.mjs` ⇒ **exit=0**。

## 3 改前 → 改后读数（成对 + 三态）

**① spec 成对**（`-g "shows coverage unknown"`，走 `npm run test:e2e`）：

| 侧 | 目标 | 读数 |
| --- | --- | --- |
| 改前（integ，隐式默认） | 活守护进程 79984（镜像 09/27 05:35） | **1 failed**；`consumption.spec.ts:92`；`Error: no build wrote this page, so its reading is null` / **`Received: undefined`** |
| 改后（当字节） | 我自己的临时守护进程（当前源码构建，镜像 **09-29 05:05:46**，`--root %TEMP%\ruagent-t107-root --addr 127.0.0.1:8899`） | **1 passed (3.5s)**，exit=0；DOM：`wiki page row: Consumption probe 孤儿页 覆盖 unknown …`、`wiki coverage badge: 覆盖 unknown` |

**② 三态 API 读数**（当前字节，我的临时守护进程）：

| 情形 | 读数 |
| --- | --- |
| 无锚点（spec 原样探针页，**无构建**） | `has_key=True` · **`cite_coverage: null`** · `freshness=fresh` · `stale_reasons=[]` · `uncited_sections=[]` · `stale_since=null` |
| 正文锚点指向**不存在**的 chunk（`<!-- cite: probe-doc#424242 -->`） | `has_key=True` · **`cite_coverage: null`** · `freshness=stale` · `stale_reasons=["chunk missing"]` · `uncited_sections=["事实"]` · **`stale_since` 非空** |
| **有构建记录且仍新鲜**（DB 行 `build_id=1, cite_coverage=1.0`） | **`cite_coverage: 1`** · `uncited_sections=[]` |
| 同上但页面已 stale（正文锚点断链） | **`cite_coverage: null`** · `uncited_sections=["事实"]` ⇒ **「1.0 + 非空 uncited」这个形状被拒绝**（RV-D-1 的判决点 `wiki.rs:3910-3915`） |

## 4 负控（两条，都能红；活库未被写）

| 控制 | 命令（都走仓库入口） | 读数 |
| --- | --- | --- |
| **A. 未命名目标** | `cd panel; npm run test:e2e -- consumption.spec.ts`（`E2E_BASE_URL` 未设、非 CI） | **exit=2**，输出 `e2e REFUSED: E2E_BASE_URL is not set.` + 「它写什么 + 起临时守护进程并命名」的完整命令；**没有 `Running N tests` 行 ⇒ playwright 从未启动 ⇒ 不可能有任何写入** |
| **B. 写闸门未武装** | `E2E_BASE_URL=http://127.0.0.1:8899 RUAGENT_E2E_ALLOW_WRITES=0 npm run test:e2e -- consumption.spec.ts` | **`4 skipped`**，exit=0；目标守护进程里的探针页**原样存留**（既没被写也没被 afterAll 删）；武装时同一命令 ⇒ **1 passed** |

**「活库未被写」的证据（只读 `mode=ro`，控制前后各取一次，逐字相同）**：

```
before controls: live: probe_memories=1 recall_consumption=4 memories_total=164
after  controls: live: probe_memories=1 recall_consumption=4 memories_total=164   → 未变 ⇒ 未写
```

（那 1/4 条是**今天早些时候那次越界运行**留下的指纹，不是本次控制产生的；本单全程只读，未删未改操作者数据。）

## 5 产物共享隐患：**已修** + 剩余登记

- **已修**（不是我改的，读数在此备案）：`playwright.config.ts:11-25` 已把 `outputDir` 做成**每次运行独占**（`"test-results/run-" + process.pid + "-" + Date.now()`），注释记录了改前的真实事故（并发两次全量跑 ⇒ `browserContext.close: ENOENT …traces/*` 假红）；commit `703c01c test(e2e): give every run its own artifact directory, and upload them in CI`。我这次的每次运行也都落在独立目录（`run-77852-…`、`run-70512-…`）✓
- **剩余登记（触发条件 + 代价）**：
  1. `panel/test-results/.last-run.json` 仍是共享路径（实测时间戳 **09-25 11:11**，是上面那次修复**之前**的遗留）；今天同一时刻并发的多次运行**各自**的 last-run 现在写进自己的 `outputDir`，但那份旧文件还躺在共享位置，**任何按它判断「上次跑绿没绿」的人会读到 09-25 的结果**。代价：一次误导性判断；处置：由 `panel/test-results/` 的日常清理/ignored 策略处理，本单只登记（**没有**顺手删别人的产物）。
  2. **同一目标守护进程上的并发运行**仍会互相踩：`consumption.spec.ts` 的 fixture slug（`e2e-consumption-probe` / `e2e-consumption-doc` / `e2e-consumption-entity`）是**固定的**，两次并发运行会互相 `afterAll` 删除对方正在断言的对象（实测 09-29 04:52–04:55 有 **9 个 run 目录**）。本单的 baseURL 闸门**不能**解决并发（两次运行都可以合法地命名同一个临时 root）。代价：偶发假红；判据：同一 slug 被两个进程写/删；建议（不在本单）：anti-parallel 锁或每次运行给 slug 加后缀。

## 6 未取到 / 需要别人改一行 / 不做

- **未取到（1 条 + 原因）**：**CI 分支**（`未设 E2E_BASE_URL` 且 `CI=true` ⇒ 允许默认 8787）在一台**活守护进程占着 8787** 的机器上无法安全地直接跑（跑它就等于把活库当目标）。这里给的是**静态读数**：`playwright.config.ts` 的 `e2eBaseUrl()` 按 `process.env.CI` 分支，`run-e2e.mjs` 同条件放行；它之所以安全，是因为 `.github/workflows/e2e.yml:114` 在跑套件前用 runner 临时 root 起了自己的守护进程（`:143` 才调 `npm run test:e2e`）。**改后第一次 CI 运行就是这条分支的读数**（由 captain 推送后自然产生）。
- **需要别人改一行（不在我 inScope，只登记）**：`panel/e2e/write-guard.ts:17-19` 的名单仍写「`registry.spec.ts` 写；**every other spec** 只读」—— `consumption.spec.ts`（以及它写入的记忆/`recall_log`/文档/实体）现在**也是写者**。文件头那句「ADDING A SPEC THAT WRITES? Import writeAccess() … then add the spec to the list above」正是要求这一步；改动是**一行**，但该文件不在本单 inScope。
- **不做（明确）**：不改 `panel/src/**`；不改 `crates/**`（`cite_coverage` 的行为留给其属主）；不改 `.github/workflows/**`（CI 的 e2e job 因为按 CI 分支而不需要改；若将来有人把 `e2e.yml` 的守护进程换成共享/长期实例，必须同时给 `E2E_BASE_URL`）。
- **一个登记而不是修的观察**：我为了取「有构建记录且新鲜 ⇒ 1」这条读数，在**自己的临时 root** 里直接写了一行 `build_id=1, cite_coverage=1.0`；随后同一条 spec **立刻变红**，报的正是 `Error: no build wrote this page, so its reading is null`。这既证明该断言**对「库里有没有构建记录」敏感**（不是我改坏的），也顺带暴露：**重新 PUT 一个手写页不会清掉已有的 `wiki_pages` 行** ⇒ 只要页面仍被判 fresh，旧的构建读数会继续被引用；而「手改」检测依赖 `wiki_page_hashes`（真实构建才会写它）。⇒ 这是 wiki 侧的一条**待验假设**，本单不改，记在这里。

## 7 不覆盖什么（第 19 条）

1. **不覆盖 CI 的第一次运行**：改后 CI 里 e2e job 是否绿、耗时多少 —— 未跑（我不会有意义地在本机伪造 CI 分支）。
2. **不覆盖活守护进程本身**：我只读它的库、只读它的镜像时间；它的镜像要不要更新、库里那 1 条记忆 + 4 条 `recall_log` 要不要清理，是 captain 呈报用户后的决定（**我没有动它们**）。
3. **不覆盖 `panel/src/**` 与 `crates/**`**：本单只动 e2e 入口/配置/spec 与报告。
4. **不覆盖其它会写的 spec**：我只把 `consumption.spec.ts` 挂上闸门并核对 `registry.spec.ts`；**其它 spec 是否写**未逐一复核（`write-guard.ts` 的名单更正见 §6）。
5. **不覆盖并发运行**：§5 的两条只登记/建议，未实现锁或 slug 隔离。
6. **不覆盖 GUI/浏览器侧**：未做截图比对、未覆盖其余 40+ spec 的行为。
7. **不覆盖「黄」**：本单不判断面板渲染是否好看，只判断「跑之前会不会写错库」与「读数量是否仍成立」。

## 8 纪律回执 + 方法学

- **我起的临时进程/路径全部按具体路径收尾**：守护进程 PID **66120 / 91308 / 30596 / 58640** 逐个 `Stop-Process`（终态 `alive after stop: False`）；删除 `%TEMP%\ruagent-t107-root`、`%TEMP%\t107-daemon.pid`、`%TEMP%\t107-liveprobe.py`、`%TEMP%\t107-clear.py`（**逐条具体路径，无通配前缀**）；端口 8899 已释放（`listening: False`）。
- **活守护进程 79984 未启停、活库未写**（只读 `mode=ro` 三次；前后指纹相同）。**未**调活守护进程的 `/wiki/pages`（当字节上该端点会写 `stale_since`）。
- **e2e 全部走仓库入口** `npm run test:e2e`（含两条负控），**没有**裸 `npx playwright test`。
- **方法学**：控制用的 DB 注入只打在**我自己的临时 root**（脚本里有 `assert "ruagent-t107-root" in db`），并在取正读前清空，避免把自己的夹具污染当成被测行为 —— 这次污染**确实发生了**（spec 立刻红），我把它当成 §6 的一条观察记录下来，而不是藏起来。
