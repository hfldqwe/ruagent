# `panel/e2e/**` 与 `scripts/**` 的固定等待 —— 两张可查表（t161，只读）

> 单号 **t161**（work）· 成员 `recall` · attempt **1** · 2026-10-04 18:5x–19:0x
> 来源：`t157`（`docs/design/reviews/gen4-wait-shaped-tests-inventory.md`）**点名「一行没读」**的两个面（其 §7②：`grep -F 'waitForTimeout'` = **19**、`waitFor(` = 0、`scripts/**` 的 `Start-Sleep` = 3）。
> **本单零代码改动**：`panel/**`、`scripts/**`、`crates/**`、`.github/**`、`tools/**` **一律只读**（要改的只写建议，由 captain 立单）。**未在共享树上造任何变异**；**未启停活守护进程**。

## 0 结论（一句话 + 三条读数）

**19 处 `waitForTimeout` 不是 19 处 CI 风险**：其中 **10 处在 CI 会跑的 `*.spec.ts` 里**、**9 处在手工探针 `*.mjs` 里（CI 一行都不跑）**。**10 处 CI 里，9 处判为 ②（固定猜测预算）**——那就是「CI 绿不可复现」的**候选集**，另 1 处只污染**读数**不影响判定。**`scripts/**` 的 3 处里有 2 处是健康的 ①**（轮询一个真实事实、耗尽响亮失败），第 3 处是 ①**但耗尽不响亮**（③ 侧缺陷）。**并且：这 22 处没有一处有「前置红证」（C47）** ⇒ 本报告**不宣布**任何一处「已经没问题」。

| 面 | 读数（我的检索） | CI 会跑？ | ②（固定猜测预算） | ① | ③ 侧缺陷 | ④ |
| --- | --- | --- | --- | --- | --- | --- |
| `panel/e2e/**` `waitForTimeout` | **19** 处 | **10 处**（`*.spec.ts`） | **9**（E1–E7、E9、E10） | 0（E9 是「②叠在①之上」） | E7 有「空转」风险（否定断言无正锚） | **无** |
| `panel/e2e/**` 手工探针 `*.mjs` | （含在上面 19 里）**9** 处 | **0 处** | 8（P1/P2、P4–P9） | 0 | 0 | 无 |
| `scripts/**` `Start-Sleep` | **3** 处 | **0 处**（见 §1.3 的假阳性说明） | **0** | **3**（S1/S2/S3，其中 S3 耗尽不响亮） | S3 | 无 |

## 1 检索命令与命中计数（可重跑）+ C51 三关

### 1.1 我用的命令（`pwsh`，工作目录 = 仓根）
```powershell
# 正对照（必然非零）
(Get-ChildItem panel\e2e -Recurse -File | Select-String -Pattern 'waitForTimeout' | Measure-Object).Count   # -> 19
(Select-String -Path scripts\* -Pattern 'Start-Sleep' | Measure-Object).Count                                 # -> 3
# 负对照（必然为 0）
(Get-ChildItem panel\e2e -Recurse -File | Select-String -Pattern 'waitForTimeoutQZX' | Measure-Object).Count  # -> 0
(Select-String -Path scripts\* -Pattern 'Start-SleepQZX' | Measure-Object).Count                              # -> 0
# 邻近形状（t157 的 claim + 本族的健康反面）
(Get-ChildItem panel\e2e -Recurse -File | Select-String -Pattern 'waitFor\(' | Measure-Object).Count          # -> 0   （复现 t157「waitFor( = 0」）
(Get-ChildItem panel\e2e -Recurse -File | Select-String -Pattern 'expect\.poll' | Measure-Object).Count       # -> 1
(Get-ChildItem panel\e2e -Recurse -File | Select-String -Pattern '\.toPass\(' | Measure-Object).Count         # -> 0
(Get-ChildItem panel\e2e -Recurse -File | Select-String -Pattern 'setTimeout\(' | Measure-Object).Count       # -> 1
```

### 1.2 C51 三关（我自己的计数先过这三关）
| 关 | 内容 | 读数 |
| --- | --- | --- |
| **正对照** | 必然存在的串必须非零 | `waitForTimeout` = **19** ✓ · `Start-Sleep` = **3** ✓ |
| **负对照** | 不可能的串必须为 0 | `waitForTimeoutQZX` = **0** ✓ · `Start-SleepQZX` = **0** ✓ |
| **模式与转义一致** | 计数用的串与逐条列举用的串**同一个** | 两条命令都是**字面串**（`Select-String` 默认不按正则解释；唯一含元字符的是 `waitFor\(`，其中 `\(` 在两个用途里写法相同）。**同一串既用于计数也用于列表** ⇒ 19 = 列表条数，逐条核过 ✓ |

### 1.3 一处**我自己的假阳性**（按纪律点名）
`Select-String -Path .github\workflows\* -Pattern 'ruagent-daemon'` 有 **1 命中**（`ci.yml:209`），但那一行是 **`cargo test -p ruagent-daemon …`（crate 名）**，**不是** `scripts/ruagent-daemon.ps1`。三个脚本在 `.github/workflows/**` 里**真实被调用的次数 = 0**（`cargo-team` 0 · `ruagent-canary` 0 · `ruagent-daemon` 0 次脚本调用）。**结论：三个脚本的固定等待不跑 CI。**

## 2 分类判据（沿用 t157 的边界，另加两条本单要区分的）

| 类 | 判据（我实际使用的） |
| --- | --- |
| **① 等一个可观测事实到终态** | 退出条件是**产品/系统给出的事实**（HTTP 字段、行、事件、文件、锁），**预算只是上界**，耗尽**响亮失败** |
| **② 固定猜测预算** | **判定（或读数）依赖墙钟**：裸 `sleep(N)` 之后就读/就断言；或**已经在等一个事实之后仍补一段固定 sleep** |
| **③ 等待体/清理里断言空转或静默吞掉** | 等不到就**走过去**（无失败），或**吞掉错误**；**否定断言没有正锚**时也归这里（见下） |
| **④ 产品侧缺信号** | 产品**根本没有**可观测完成信号，只能靠时间猜 ⇒ 必须写成产品侧 finding |

**本单新增的两条区分（不混类的关键）**：
1. **「采样窗口」不是等待**：`api-window-probe.mjs` 的 `WINDOW_MS=11_500` 是**被测的量本身**（数 11.5 秒内的请求），不是「等某个事实」。它与 ② 的区别是：② 用时间**替代**判定，而它是**定义**判定。⇒ 记作 **非等待（仪器定义）**。
2. **「只污染读数」的 ②**：若某处 sleep 之后只有 `console.log`（读数的打印）而**没有断言**，它在慢机器上会**打印错的读数**，但**不会**让 CI 变红 ⇒ 它是「CI 证据不可复现」，不是「CI 绿不可复现」。E8 与 P1–P9 属于此类，与 E1–E7/E9/E10 **分开**。

## 3 表 A —— `panel/e2e/**` 的 19 处 `waitForTimeout`

**（列「CI」= 该文件是否被 Playwright 收集**：`panel/playwright.config.ts:52` `testDir: "./e2e"`，**未设 `testMatch`/`testIgnore`** ⇒ 用 Playwright 默认的 `**/*.spec.ts` ⇒ **只有 `*.spec.ts` 会被跑**；三个 `*.mjs` 探针在 `.github/workflows/**` 与 `scripts/**` 里**零引用**（§1.3），只能手工 `node e2e/xxx.mjs` 跑。）

| # | 坐标（符号优先） | 时长 | 在等什么 | 类 | CI | C47 前置红证 | C48 生产/CI 可解释 | 建议处置 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **E1** | `responsive.spec.ts:35` · `phone (390px) › ${route} fits the viewport` | 600ms | `goto(/#route)` 之后**布局结算**，然后读 `.ant-layout-sider` 的 `boundingBox()` 并断言 `width ≤ 80` | **②** | **是** | **无** | 能（慢 ⇒ 读到未结算的盒子） | **立单**：把 600ms 换成 `expect.poll(() => sider.width)`/`toHaveCSS`，让判定依赖事实 |
| **E2** | `responsive.spec.ts:51` · `chat history rail opens and closes as a slide-over` | 600ms | `goto("/#chat")` 后、**点击前**的结算（等 hydration/首屏就绪），随后 `.click()` → `toHaveClass(/open/)` | **②** | **是** | **无** | 能（点击落在未挂载的 DOM 上） | **立单**（与 E1 同族，可一次改完） |
| **E3** | `responsive.spec.ts:66` · `tablet (768px) › home and board fit with the rail collapsed` | 600ms | 同 E1（读 sider 宽度 + `noOverflow`） | **②** | **是** | **无** | 能 | **立单**（同族） |
| **E4** | `responsive.spec.ts:83` · `sider expands at lg and the footer toggle collapses it` | 600ms | `goto("/#home")` 后布局结算，断言 `width === 228` | **②** | **是** | **无** | 能 | **立单**（同族） |
| **E5** | `responsive.spec.ts:94` · 同上，折叠点击之后 | 300ms | **折叠动画/样式过渡**结束，随后读 rail 宽度 `≤ 80` | **②** | **是** | **无** | 能（动画时长随机器） | **立单**：等过渡结束或 `expect.poll` 宽度 |
| **E6** | `responsive.spec.ts:106` · 同上，再展开 | 300ms | 同 E5（展开方向） | **②** | **是** | **无** | 能 | **立单**（同族） |
| **E7** | `failure-visibility.spec.ts:178` · `P2: a 500 on /recall/log is an ERROR, not an empty log` | 1200ms | `openAuditLog(page)`（它只 `goto` + 点「审计日志」分段项，**点完不等任何事实**）之后，让审计日志视图**渲染出来**，再读 `innerText` 做两条**否定**断言（`expect(body).not.toMatch(/召回失败/)` 等） | **② + ③ 风险** | **是** | **无** | 能（渲染未完成时否定断言**照样通过** ⇒ 空转） | **立单（小）**：先在同一个测试里**锚一个正面事实**（例如 `await expect(logView).toContainText(<日志表头>)`），再做否定断言 —— 否则「仪器坏了」与「产品没出错」读起来一样 |
| **E8** | `failure-visibility.spec.ts:228` · `P3: a failed mount probe says so on the CHIP…`（成功重试那一段） | 2000ms | 点击重试后**卡片的读数**（`cardText` / `readoutOf(page,"模型")`）—— 其后面的 5 行**全是 `console.log`，没有断言** | **②（只污染读数）** | **是** | **无** | 能 | **只登记**（它不会让 CI 变红；若要读数也可复现，改 `expect.poll` 后再打印） |
| **E9** | `failure-visibility.spec.ts:250` · `P3 success control: a real 0 models is 'not probed'…` | 1800ms | **已经在 `:249` 等到了可观测事实**（`expect(cardOf(RT)).toBeVisible({timeout:15_000})`）之后，再等「模型读数」渲染出来，才读 `readoutOf` 并断言 `toMatch(/未探测/)` | **② 叠在 ① 之上** | **是** | **无** | 能（可见 ≠ 读数就绪） | **立单**：用 `await expect(readoutOf(page,"模型")).toHaveText(/未探测\|not probed/)` 取代这段 sleep（同一个事实、不用墙钟） |
| **E10** | `settings-capabilities.spec.ts:377` · 拒绝写入的那一条（`capabilities-plugins-design.md:2524` 指向的两个测试之一） | 500ms | 注释自己写了：**「请求事件经 CDP 投递，读一个『没有发生』之前给它一拍」** —— 等 CDP 事件到达，然后 `expect(puts).toEqual([])` | **②（判定直接依赖这 500ms）** | **是** | **无** | 能（延迟本身），**但不可证伪**：**全文件没有一处正对照**（`puts` 只出现 3 次：声明 `:352`、采集 `:354`、断言空 `:378`） | **立单（本表最该修的一条）**：同一个窗口里加**正对照**——先走一条**会发 PUT** 的路径证明 `puts` 非空（或等一个能证明 CDP 通道活着的正面事实），再断言这次为空。**否则仪器坏掉时这条测试也是绿的** |
| **P1** | `api-window-probe.mjs:53`（手工探针） | 2500ms | `goto(…, {waitUntil:"load"})` 之后的**应用结算**，然后才开测量窗口 | ②（读数） | **否** | 无 | 能 | 只登记 |
| **P2** | `api-window-probe.mjs:56`（手工探针） | 600ms | `--click=` 点击后、开窗口前的结算 | ②（读数） | **否** | 无 | 能 | 只登记 |
| **P3** | `api-window-probe.mjs:59`（手工探针） | **`WINDOW_MS = 11_500`**（`BURST_MS = 2_500`） | **不是等待**：它就是「11.5 秒内数请求」的**采样窗口**（仪器的定义） | **非等待** | **否** | — | — | 只登记（并说明它与 ② 的区别，见 §2①） |
| **P4** | `focus-owner-probe.mjs:31`（手工探针） | 2500ms | 逐个路由 `goto` 之后的应用结算（随后读 `document.activeElement`） | ②（读数） | **否** | 无 | 能 | 只登记 |
| **P5** | `focus-owner-probe.mjs:53`（手工探针） | 100ms | 路由之间按一次 `Tab` 把焦点「移开」后的结算 | ②（读数） | **否** | 无 | 能 | 只登记 |
| **P6** | `x4-parity.mjs:56`（手工探针） | 300ms | `waitForFunction` 之后的**再渲染**（注释：`waitForFunction needs the key to repeat: prime it once, then confirm`） | ②（读数） | **否** | 无 | 能 | 只登记 |
| **P7** | `x4-parity.mjs:111`（手工探针） | 250ms | `setViewportSize(WIDTHS…)` 之后布局结算，再读 rect | ②（读数） | **否** | 无 | 能 | 只登记 |
| **P8** | `x4-parity.mjs:114`（手工探针） | 120ms | 去掉 `.grid` 类之后布局结算 | ②（读数） | **否** | 无 | 能 | 只登记 |
| **P9** | `x4-parity.mjs:117`（手工探针） | 120ms | 加回 `.grid` 类之后布局结算 | ②（读数） | **否** | 无 | 能 | 只登记 |

**逐条都读了「等什么」**：19/19 都能从上下文读出（无一处「读不出」）。补充读数：`responsive.spec.ts` 4 个 test / 11 个 `expect`、`failure-visibility.spec.ts` 6 个 test / 20 个 `expect` / 20 个 `console.log`、`settings-capabilities.spec.ts` 2 个 test / 28 个 `expect` ⇒ 三个文件**都真的在断言**，不是空壳。

## 4 表 B —— `scripts/**` 的 3 处 `Start-Sleep`

**先回答契约的问题：这条路径上「有没有可观测终态可等」——有，而且三处都在等它。**

| # | 坐标（符号优先） | 时长 | 在等什么（**可观测终态**） | 类 | C47 | C48 | 建议处置 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| **S1** | `scripts/cargo-team.ps1:263`（团队构建锁的获取循环） | `-Seconds 10`，**循环上限 = 90 分钟 deadline**，超时 **`exit 75`** 并打印「gave up waiting … after 90 min」 | **锁文件可独占打开**（`[IO.File]::Open($lockPath, 'OpenOrCreate','ReadWrite','None')` 成功）—— 真正的可观测终态 | **①**（**10s 只是轮询间隔**，判定依赖事实；耗尽**响亮**） | 无 | 能 | **只登记**（形状正确；这是本表里最健康的一处） |
| **S2** | `scripts/ruagent-canary.ps1:98`（健康轮询） | `-Milliseconds 750`，上限 `$deadline = now + TimeoutSec` | **`GET http://$Addr/` 返回 200**（`$healthy = $true; break`）；**耗尽后** `Write-Host "canary: NOT healthy within Ns …"` + **`exit 1`** | **①** | 无 | 能 | **只登记**（**3 处里的范例**：事实 + 上界 + 耗尽响亮，`exit 1` 可被调用方检查） |
| **S3** | `scripts/ruagent-daemon.ps1:98`（`start` 之后） | `for ($i=0; $i -lt 20; $i++) { Start-Sleep -Milliseconds 500; if (Test-Health) { …; break } }` = **最多 10s** | **`Test-Health` 为真**（健康检查）—— 有可观测终态 | **①（但耗尽不响亮 ⇒ ③ 侧缺陷）** | 无 | 能 | **立单（小）**：循环**之后**没有检查是否健康——未健康时脚本**仍然打印 `started pid=…` 并 exit 0**。改成：未健康则打印警告并以非零退出（或把 `status` 的健康行打出来）。★ 这正是 AGENTS.md 让人用来启动守护进程的那个脚本 |

**为什么 S1/S2/S3 的 10s/750ms/500ms 不是 ②**：三处的**判定**都取决于那个**事实**（锁、HTTP 200、health），数字只决定**多久放弃**，且 S1/S2 放弃时**响亮失败**；S3 的缺陷不在「用时间判定」，而在**放弃了也不说**（③ 侧）。

## 5 专节：**②（固定猜测预算）**与「只是慢」分开 —— 以及 CI 相关性

**(a) 只有 10 处在 CI 上。** 契约说「19 处运行在 CI 上」——**实测更窄**：`testDir="./e2e"` + 未设 `testMatch` ⇒ Playwright 只收集 `*.spec.ts`；三个 `*.mjs` 探针（`api-window-probe.mjs` / `focus-owner-probe.mjs` / `x4-parity.mjs`，共 **9** 处）在 `.github/workflows/**` 与 `scripts/**` 里**零引用**（§1.3）⇒ **CI 跑的是 10 处**，另外 9 处只能手工跑。**这条不改结论的风险面，但改「谁该动手」**：9 处只需登记，10 处才与合并门相关。

**(b) ② 的 CI 候选集 = E1–E7、E9、E10（9 处）**，理由是它们**用墙钟决定判定**：读几何盒/宽度（E1–E6）、在**否定断言**前只等一段时间（E7）、在**已等到事实之后**再补 sleep 才读（E9）、把**「没有发生」**判给一个固定窗口（E10）。慢/负载高的 runner 上，这些**可以**翻面。

**(c) 与「只是慢」的区别（本节的要点）**：
| 形状 | 例子 | 会翻面吗 |
| --- | --- | --- |
| **② 用时间代替事实** | E1–E7/E9/E10 | **会**（判定依赖墙钟） |
| **慢，但判定仍是事实** | S1（10s 轮询）、S2（750ms 轮询）、`crates/**` 里 t157 表 D 的 `poll_*` | 不会（只是更久；上界到点**响亮报错**） |
| **预算被抬高** | `panel/e2e/judge.spec.ts:18 test.setTimeout(60_000)`、E2E job 的 30 分钟 job 超时（`e2e.yml:30`，出处 `gen3-ci-timeouts.md`） | 不会（它们只给上界） |
| **只污染读数** | E8、P1/P2、P4–P9 | 不会让 CI 红，**会**让 CI 的**证据**不可复现 |

**(d) 真正的「只在 CI 上」的证据面**：E2E 工作流历史（我读的 20 次运行）里 **17 绿 / 2 红 / 1 进行中**，两次红**都不是** spec 失败（§6），⇒ 就现有记录看，**这套 suite 近期没有因这 10 处固定等待翻面的实例**——但按 C47，这**不能**反过来说它们没问题（见 §6）。

## 6 C47 / C48 逐条（含两次红 run 的机制）

**C47（前置红证）**：**表 A 19 处 + 表 B 3 处，全部「无前置红证」。** 依据：
1. 仓内文档对这些名字的命中**都不是红证**：`responsive.spec` 5 处（`MASTER.md:728` 的 B2 规格引用、`primitives.md:845/1236` 的断言说明）、`settings-capabilities` 1 处（`capability-plugins-design.md:2524` 描述它断言什么）、`failure-visibility` **0 处**；`x4-parity`/`api-window-probe`/`focus-owner-probe` 的命中都在 `acceptance-report.md`（**工具被创建/使用**的记录，不是失败记录）。
2. E2E 工作流**最近 20 次运行**里只有 2 次红，而**两次红都不是 spec 失败**：
   - run **`36510848293`**（push，head `02dea44`）——**0 秒死、无 job**。机制**已由 `e2e.yml:32-47` 自己写明**：那版的路径用了 `runner.temp`，而**它在 job 级 `env:` 里不可用**，GitHub 静态校验直接**拒掉整个 workflow 文件**（「This run likely failed because of a workflow file issue」）⇒ 与固定等待无关。
   - run **`36508857994`**（push，head `5c0bc8f`）——失败的 job 是 **`Daemon + doctor + panel smoke`**，失败的 step 是 **`E2E evidence (spec counts + exit code, skips named)`**，错误原文是 **`no Playwright JSON report at …/playwright.json`**（`e2e.yml:39` 引用了同一条 `5c0bc8f`）⇒ 是 **JSON 报告载具**（`PW_TEST_REPORTER` vs `--reporter=json`）的问题，**不是** spec 断言失败。
   ⇒ **因此：任何「这 19/3 处 0/N 绿」都不能用来宣布它们已修好**（C47）；本报告一处也没这么写。
3. **存在性反证不适用**：本族**没有**「改前红过」的记录可引 ⇒ 按 C47 只能**登记为无前置红证**（我没有在共享树上造变异去伪造红证，纪律②）。

**C48（红过而机制不明 ⇒ 先回答「生产/CI 能不能解释」）**：本族**没有**「红且机制不明」的条目——两次红 run 的机制都**能解释**（工作流文件校验、JSON 载具），且都**不在**本族的因果链上。**唯一留成未解释项的是我自己的取证边界**（§8③），不是本族的成员。

**④（产品侧缺信号）**：**本单判 ④ = 空**，但把**唯一真被判过的那一处**写出来：**E10** 是最接近 ④ 的——「没有发 PUT」在产品侧**天生**没有正面字段可读（否定事实）。我仍判**不是 ④**，理由：同一个测试里**拒绝本身有一个正面产品事实**（`:373` `await expect(input).toHaveValue(REFUSED_TEXT)`），缺的不是产品信号，而是**仪器的正对照**（证明 `puts` 采集通道活着）。⇒ 处置是**测试侧**（加正对照），**不是**产品侧 finding。若将来发现「连拒绝都没有可读事实」，那一处才升级为 ④。

## 7 建议处置汇总（排序）

| 排序 | 组 | 成员 | 建议 | 谁动手 |
| --- | --- | --- | --- | --- |
| **1（现在就修，小而值）** | 否定断言的正锚/正对照 | **E10**（`settings-capabilities.spec.ts:377`，全文件无正对照）· **E7**（`failure-visibility.spec.ts:178`，审计日志视图点完不等事实就做否定断言） | 各加一处**正面事实**等待/对照，再做否定断言；E10 还要证明 `puts` 通道能看到 PUT | `panel/e2e/**` 属主 **立单** |
| **2（立单，一次改完一族）** | 几何/动画的 ② | **E1–E6**（`responsive.spec.ts` 6 处：600/600/600/600/300/300ms）· **E9**（1800ms 叠在 `toBeVisible` 之上） | 统一改用 `expect.poll(...)` / `toHave...`，让判定依赖事实（本 suite 里**已有**健康样板：`agents-health-third-state.spec.ts:98` 的 `expect.poll(..., {timeout:5_000})`） | 同上 |
| **3（立单，小）** | 静默的轮询 | **S3**（`scripts/ruagent-daemon.ps1:98`） | 循环后检查健康；未健康则报错/非零退出（别让 `started pid=…` 冒充成功） | `scripts/**` 属主 |
| **4（只登记）** | 手工探针 9 处 + 只污染读数的 1 处 + 健康脚本 2 处 | `P1–P9`（`.mjs`，**不跑 CI**）· **E8**（读数）· **S1/S2**（形状正确） | 无需动作；下次有人动这些文件时顺手改成事实等待 | — |
| **5（产品侧 finding）** | — | **无**（§6 的 ④ 判定） | — | — |
| **不做** | `#[ignore]` 16 条逐条读 · `crates/**` 重盘 · 在本单改代码 | 见 §8 | — | — |

**本单零代码改动**：以上全部是**建议**；`panel/**`、`scripts/**`、`crates/**`、`.github/**` **一个字节未改**。

## 8 我【没有】覆盖的（点名，不静默省略）

1. **`#[ignore]` 的 16 条**：t157 已登记为未读（其 §7①），本单**也没有**逐条读它们的处境。
2. **`crates/**` 的测试**：t157 已盘（一个形状、4 处要动）⇒ **本单没有重盘**，也没有把它的结论继承到这两个面（这两个面是独立的仪器）。
3. **产品代码里的等待**：`chat.rs` 的 `MODELS_WAIT` 系列、`spawn_idle_reaper` 的 `sleep(60s)` 等（t157 §7③点名）——那是**产品行为不是测试判据**，本单只**点名**，未评估。
4. **`*.mjs` 探针 9 处在非 CI 场景下的表现**：我只读代码，**没有运行**它们（运行需要活守护进程与浏览器；纪律不许启停活守护进程，本单也无需跑）。
5. **我没有跑 E2E**：因此**没有**任何「这 10 处在真跑中的读数」（例如实际耗时分布、是否真翻面）。按 C47，这也意味着**我没有任何一处的前置红证**。
6. **读得不完全的两处**（点名）：
   - `x4-parity.mjs:56` 的 300ms：注释只说「`waitForFunction` 需要键重复：先 prime 一次再确认」，**为什么要 300ms** 我读不出（可能是 React 再渲染的猜测）；
   - `api-window-probe.mjs:59` 的 `WINDOW_MS=11_500`：我判它是采样窗口，但**它是不是也隐含了「稳态」假设**（即假设 11.5 秒内应用不再结算）我读不出。
7. **`scripts/**` 其它非 `Start-Sleep` 的等待形状**：如 `Wait-Process` / `Get-Date` 循环 / `-TimeoutSec` 参数——**未逐条盘**（本单只盘 `Start-Sleep`，与 t157 的计数口径一致）。已知一例：`ruagent-canary.ps1` 的 `Invoke-WebRequest -TimeoutSec 5`（单次请求超时，不是等待）。
8. **`.github/workflows/**` 里的等待**（`sleep`/timeout 键）：不在本单 inScope（且 t157 亦未盘）——只在 §5(c) 引用了 `e2e.yml:30` 的 30 分钟 job 超时作为「预算」的对照。

## 9 纪律回执

- **只读**：`panel/**`、`scripts/**`、`crates/**`、`.github/**`、`tools/**` **零改动**；我唯一的写入 = 本报告。读取方式全是 `Select-String` / `Get-Content` / `gh run view`（只读）。
- **没有在共享树上造变异**（C47 要求的红证我**登记为无**，没有伪造）。
- **没有启停活守护进程**：本单**一次都没有**访问 8787 的写端点，也没有起停任何 `ruagent.exe`；只读的 `gh` 用 `NO_PROXY='*'`（⑤：`git`/`gh` 用 `*`，Rust 门禁才用 host 列表——本单未跑 Rust 门禁）。
- **收尾清理**：临时文件按**具体路径**删（`%TEMP%\t161-sites.txt`）；**没有**创建 junction，故 C39 不适用。
