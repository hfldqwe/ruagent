# t161 候选集收口：9 处固定猜测预算 → 等事实 · E7 正锚 · S3 响亮失败（t164）

> 单号 **t164**（repair）· 成员 `recall` · attempt 1 · 2026-10-04 19:3x–20:2x
> 来源 = **我自己**的 `t161` 只读盘点（`docs/design/reviews/gen4-e2e-scripts-wait-tables.md`）：19 处 `waitForTimeout` 里 **10 处在 CI 会跑的 `*.spec.ts`**，其中 **9 处是 ②（固定猜测预算）**；`scripts/**` 的 **S3 是 ① 但耗尽不响亮**。
> **inScope 写入集合**：`panel/e2e/{responsive,failure-visibility,settings-capabilities}.spec.ts`、`scripts/ruagent-daemon.ps1`、本报告。
> **零改动**：`panel/src/**`、`panel/tools/**`、`crates/**`、`.github/**`、`panel/e2e/*.mjs`、`scripts/cargo-team.ps1`、`scripts/ruagent-canary.ps1`、`scripts/lib/**`。
> **依赖已落地后开工**：`t158`/`t159`/`t162` 三点都完成、`panel/src/views/Runtimes.tsx` 已是 graph 修好的 `9D40A563…` 之后，我才跑「重建 dist + 全量 E2E」这条正规路径（§5 把那之前的两批读数按 dist 身份分开写）。

## 0 结论

| 项 | 读数 |
| --- | --- |
| **9 处 ②** | 全部改成**等可观测事实**（`expect.poll` / `toHaveText` / `toBeVisible`），**没有**调大时长、**没有**重试到过为止、**没有**放宽断言、**没有** `skip` |
| **`await page.waitForTimeout(` 实际调用** | **10 → 2**，剩下两处都是**故意**的：`failure-visibility.spec.ts:241` 的 2000ms（契约明确划在范围外，只污染读数）· `settings-capabilities.spec.ts:409` 的 500ms（**有界窗口**，且**前面已有正对照**、读它时会打印所见请求 ⇒ 可诊断；见 §2） |
| **E7 正锚** | 先等**审计视图自己的数据请求**（`GET /api/v1/memory/diffs`，200）+ **终端态元素就位**，再做两条否定断言 |
| **E10 正对照** | **零写标定**：同 URL 任意方法**页面**请求 ≥1（**我实测自证**：2 次 `GET …/api/v1/capabilities`、`resourceType=fetch`、`hasFrame=true`、200）**然后**才是窄口径 `puts == []` |
| **S3** | 选 **(b)**：只有健康后才打印 `started pid=… log=…`；失败路径**同样逐字保留 pid 与 log 路径**并 `exit 1`。**红证 exit=1（116s、点名、原文见 §3）· 正对照 exit=0（`started…` + `health ok`）** |
| **门禁** | `npm --prefix panel run build` **exit 0**（5.46s）· 本机 `npm run test:e2e`（仓库入口）**exit 0，`62 passed (19.2s)`**，逐 spec 合计 **62**、`stats.skipped=0`、`errors: []` |
| **C47 口径** | 这 22 处**均无前置红证** ⇒ 本单**不宣称**「修好了一条会红的 flake」，写成「**移除了一个假绿来源**」；能做的确定性红证做了三条（§4），做不到的**点名说明为什么** |

## 1 逐处：改前 → 改后 + 每处的边界论证

**「边界论证」的含义**（review 5b）：对象是什么 · 它持续多久 · 为什么我给的界**小于**它（即界是保守的、不是理想值）。

| # | 位置（改后行号） | 对象 | 改前 | 改后 | 边界论证 |
| --- | --- | --- | --- | --- | --- |
| E1 | `responsive.spec.ts:46`（390px × 9 路由） | `.ant-layout-sider` 的**渲染宽度**（断点触发后的图标轨） | `waitForTimeout(600)` + 读一次 `boundingBox()` + `expect(<=80)` | `expect.poll(railWidth, {timeout:5_000, message}) <=80` | 对象是**一次布局结算**（antd Sider 的响应式断点在首帧后立即生效，若带过渡也是 CSS transition 量级，**< 300ms**）⇒ 5s 是它的**一个数量级以上的上界**；耗尽**响亮**（message 点名路由与谓词）。且谓词与被替换的完全同一个（`<=80`） |
| E3 | `responsive.spec.ts:80`（768px × home/board） | 同上 | 同 E1 | 同 E1 | 同 E1（路由名进 message） |
| E4 | `responsive.spec.ts:97`（desktop 1280px） | 展开态 sider 宽度（应用里的**常量 228**） | `waitForTimeout(600)` + `expect(width).toBe(228)` | `expect.poll(railWidth, {timeout:5_000, message}).toBe(228)` | 对象是常量 ⇒ 只要它出现，谓词立刻成立；5s 是「等到它出现」的上界，而应用的过渡 **< 300ms** |
| E2 | `responsive.spec.ts:65`（chat 滑出层） | `.chat-side-toggle` 的**存在/可见** | `waitForTimeout(600)` 然后 `click()` | `await expect(locator).toBeVisible()` 然后 `click()` | 对象是元素的可见性；`toBeVisible` 自带**有界重试**（配置 `expect.timeout = 8s`），click 自己还有 actionability 契约（visible/stable/enabled/receives events）⇒ 我把一个**对任何事实都不设界的睡眠**换成了两条本来就带界的等待 |
| E5 | `responsive.spec.ts:113`（收起） | 同上宽度（**状态变化后**） | `waitForTimeout(300)` + 读一次 + `<=80` | `expect.poll(railWidth, {timeout:5_000, message}) <=80` | 300ms 赌的就是那次折叠过渡；实测过渡是 CSS transition 量级 ⇒ 5s 保守一个数量级以上 |
| E6 | `responsive.spec.ts:129`（再展开） | `.brand-name` 可见性 | `waitForTimeout(300)` + `toBeVisible()` | **只留 `toBeVisible()`** | 被删掉的等待**完全冗余**：它后面那条断言自己就有界、可重试 —— 这是「移除一个假绿来源」最干净的一种 |
| E9 | `failure-visibility.spec.ts:263` | 运行时卡片上「**未探测 / not probed**」文本（探测返回 0 个模型后） | `toBeVisible(card)` + `waitForTimeout(1800)` + 读文本断言 | `toBeVisible(card)` + `expect(card).toHaveText(/未探测\|not probed/, {timeout:15_000})` | 1800ms 是**叠在一条已经正确的等待之上的**猜测（`toBeVisible` 证明卡在、不证明文本已终态）⇒ 现在直接等**那个文本**，用的是本 suite 自己的样板 `failure-visibility.spec.ts:196`（同类断言用同一形态）。**更强**：两条后续否定断言现在读的是一张**已终态**的卡 |
| E7 | `failure-visibility.spec.ts:171-187` | 审计视图的**渲染完成**（它 mount 时必发 `GET /api/v1/memory/diffs`，然后进入三个终端态之一：`.card` / `.ant-empty` / `.error-state`） | `openAuditLog()` + `waitForTimeout(1200)`，随后**两条纯否定断言** | 先 `waitForResponse(/api/v1/memory/diffs/, 200)`（**在点击之前**挂上，防漏）+ 断言终端态元素可见，**再**做否定断言 | 「渲染完成」这个对象**有产品侧信号**（它自己的数据请求 + 三选一的终端态）⇒ 我不需要猜时长。缺了它，**「仪器坏」与「产品好」读起来一样**（本代已经吃过一次这种假绿）。10s 是 CDP+一次本地 GET 的上界，比 sleep 更紧 |
| E10 | `settings-capabilities.spec.ts:349-411` | 「**这次没有 PUT**」这个**缺席**事实 | 窄口径采集器 + `waitForTimeout(500)` + `expect(puts).toEqual([])` | **采集器拓宽到同 URL 任意方法** + **正对照 `expect.poll(capsRequests.length) > 0`**（在编辑器已可见之后，故确定成立）+ 读缺席前**打印所见请求** + 保留 500ms 有界窗口 | **唯一一个对象没有产品侧信号的**：没有任何渲染事实能证明「一个请求没有被发出」⇒ 这正是契约允许的「保留等待但变成可诊断」那一支。**但它的可证伪性不再来自时长，而来自正对照**（仪器坏 ⇒ 第一个断言红）——见 §2 |

**共同点（不削弱的三条自证）**：① 每处谓词与被替换的**逐字相同**（`<=80` / `228` / `未探测` / `puts` 为空），没有一处放宽；② 每处**新增**的都是「等到事实」或「读到就已确定」的等待，没有任何一处增加了时长或重试次数；③ 实际 `waitForTimeout` 调用 **10 → 2**（见 §0 表）。

**一处我自己抓到的自伤（值得写下来）**：`railWidth` 的「取不到值」哨兵我第一版写的是 `-1`。而 `-1 <= 80` == **true** ⇒ 一旦 sider 根本不存在，E1/E3/E5 的 poll 会**立刻通过** —— 比被替换的 `expect(sider?.width).toBeLessThanOrEqual(80)`（`undefined <= 80` == false ⇒ 失败）**更弱**。改成 `NaN`（对这三个谓词一律 false）并把这个理由写进代码注释；随后**重跑门禁与全量 E2E**（§5 的第四批），本报告所有读数都属于修后的字节。

## 2 E10 的自证（零写标定的**承重**证据）

判据原文的旧形状「造一次会被采集到的 PUT」**错且危险**（`page.on` 看不见 `request.put`；真经 UI 发 PUT = 一次真实配置写）。改成零写标定之后，**正对照本身成了承重点**，所以它必须自带证据 —— 我用一个**独立探针**（`%TEMP%\t164\probe-e10.mjs`，跑在**我自己的**守护进程 + 我自己的端口上）实测，而不是读代码推断：

```
capsRequestsSeenByPageOn: [
  { method: "GET", url: "http://127.0.0.1:19013/api/v1/capabilities", resourceType: "fetch", isNavigationRequest: false, hasFrame: true },
  { method: "GET", url: "http://127.0.0.1:19013/api/v1/capabilities", resourceType: "fetch", isNavigationRequest: false, hasFrame: true }
]
responses: [{ status: 200, ... }, { status: 200, ... }]
impossibleUrlCount: 0        editorsRendered: 6        optionInputsRendered: 9
daemonCapabilitiesShape: { status: 200, top: ["table_present","config_file","capabilities","conflicts"] }
```

**为什么这就证明了「是页面发的、路径就是这个 URL」**：
1. **路径逐字**：URL 就是 `…/api/v1/capabilities`（与 spec 里 `.includes("/api/v1/capabilities")` 同一个串）。
2. **页面发的**：这些事件是经由 **`page.on("request")`** 收到的 —— 那是**页面自己的网络栈**的事件通道；`APIRequestContext`（spec 里 `capabilities(request)` 那条）走的是**另一个客户端**，**不会**出现在这个通道里。附加字段同向佐证：`resourceType = "fetch"`（页面 fetch API 的归类）、`hasFrame = true`（有归属 frame）、`isNavigationRequest = false`（不是导航）。
3. **时机成立**：探针在 `#settings` 上等到 `[data-options-for]` 出现时**已经**收到它们（`editorsRendered = 6`），而 spec 的正对照就断言在编辑器可见**之后** ⇒ 那条 `≥1` 是**确定性**成立的，不是一个可能为空的赌注。
4. **仪器不是恒真的**：同一个仪器指向一个不可能存在的路径时计数为 **0**（`impossibleUrlCount = 0`）⇒ 「计数 ≥1」是**真信号**，不是常量。
5. **真实套件里也印出来了**（不只是探针）：全量 E2E 跑到该用例时，我加的那行 console 输出
   `DOM READING E10 caps requests seen: ["GET http://127.0.0.1:19013/api/v1/capabilities"] | PUTs: []`
   ⇒ **正对照在真跑中成立、且缺席断言背后确实有一个工作的仪器**。这两件事在同一个 run 里，正是「缺席可证伪」的定义。

## 3 S3：形态 **(b)** 的理由 + 红证/正对照 + 一条额外发现

**选 (b) 的理由**：「started」是对**守护进程**的断言，不是对 **WMI 调用**的断言。旧字节里 `started pid=…` 打在健康轮询**之前**且轮询结果被丢弃 ⇒ 未健康也照样 `exit 0`，读日志的人先看到「started」就当成「已就绪」（这正是 S3 的现场缺陷）。把「已启动」挪到健康之后，让**这一行本身**成为「真起来了」的证据，比「保留它 + 之后报错」更少歧义（(a) 的代价是输出里先出现一个可能被误读的成功词）。

**信息不得丢失（硬约束）**：`pid` 与 `log` 路径在**两条路径上都逐字保留**——成功路径是原来那行 `Write-Output "started pid=$($res.ProcessId) log=$logPath"`（**一字未改**），失败路径是
`NOT healthy after ${waited}s (pid=… log=…) -- nothing answered http://…/api/v1/health. The process may have exited (read the log) or another listener may hold <addr>.`
两处都给出 WMI 那句唯一的返回值（pid/log）⇒ 失败时我把信息**补全**了，而不是丢掉。

**额外细节（诚实性）**：`Test-Health` 自己有 `-TimeoutSec 5` ⇒ 被「接受连接但永不应答」的监听占住端口时，这个循环会远远超过「20×500ms」的名义时长。所以我的失败消息里的**已等秒数是实测的**（`$healthStart = Get-Date` 起算），不是写死的「10s」。这条修正是红证逼出来的：第一次红证实测 **110s**，如果我写死「after 10s」，那句话就是**假的**。

**红证 + 正对照（两条读数，都是我自己端口 19014 + 自己的临时 root）**：
```
RED  (我的 dummy 监听先占住 127.0.0.1:19014，绝不用 8787):
  exit=1  in 116s
  NOT healthy after 110s (pid=27108 log=C:\…\t164\root-s3\logs\daemon.log) -- nothing answered
  http://127.0.0.1:19014/api/v1/health. The process may have exited (read the log) or another
  listener may hold 127.0.0.1:19014.
POSITIVE CONTROL (让开端口后同一命令):
  exit=0  in 24s
  started pid=40740 log=C:\…\t164\root-s3\logs\daemon.log
  health ok
```
⇒ **不存在 CI 消费者**（该脚本在 `.github/workflows/**` 引用 **0** 次）⇒ 按 C47 它**不能靠 0/N 绿收口**，这两条就是它的收口证据。

**额外发现（S3 邻近，**只报不改**，因为它超出本单授权的那一处改动）**：`started pid=` 报的是 **WMI 创建的 `cmd.exe` 的 pid**，**不是 `ruagent.exe` 的 pid**。证据：我按报出的 pid 收尾时 `Stop-Process` 之后 **19014 仍被占**，端口归属查询给出真正的守护进程 `pid 36712: ruagent.exe … serve --addr 127.0.0.1:19014 --root C:\…\t164\root-s3`（我按「命令行含我的临时 root + 不是 8787」确认是**我的**进程后才停它）。⇒ 任何想用那行 pid 去停守护进程的调用方都会**停错对象**。这是**既有行为**、不在本单授权范围内（判据只允许我在健康轮询之后加失败路径），故只登记；顺带说明为什么那行必须**逐字保留**（改它就不是 S3 了）。

## 4 确定性红证：我做了什么、做不到什么（C47 口径）

**做了三条（都能红、都不是 0/N 绿）**：
1. **S3**：真红证 + 正对照（§3，非零退出并点名 / 真健康 exit 0）—— **不碰活守护进程**（自己的端口 + dummy 监听 + 自己的 root；8787 与 pid 14944 全程未碰）。
2. **E10 仪器的可证伪性**：同一仪器指向不可能存在的路径 ⇒ 计数 0（§2 第 4 条）；而正对照在**真实套件**里 ≥1（§2 第 5 条）⇒ 仪器坏的情况**一定**会让第一个断言红。
3. **`expect.poll` 机制本身**（E1–E6 用的那个机制）：`%TEMP%\t164\poll-control.mjs` 在 `@playwright/test` 上跑两条对照 —— 不可满足的谓词**必须响亮抛错并带上我的 message**，可满足的谓词**不许抛**：
   ```
   { "controlTimedOutLoudly": true,
     "controlMessage": "Error: t164 control: this MUST time out | … | expect(received).toBeLessThanOrEqual(expected)",
     "passingControl": "ok (no throw)" }
   ```
   ⇒ 「耗尽响亮」不是声明，是读数；且这不是签名用错导致的假红（正对照不抛）。

**做不到的（点名）**：要**确定性地**让 E1–E6/E9 那几处的**应用侧**断言变红，必须把 `panel/src/**` 在那一刻弄坏（例如让 sider 不折叠、让探测文本不出现）—— `panel/src/**` 是**本单 out of scope**，所以我没有开任何共享树变异窗口（也**没有**用「临时改一下再改回来」那种形状：按 AGENTS.md，那会让别人的测试红，且**未宣告的控制与真 bug 无法区分**）。这些处的「不误红」证据是：**谓词与被替换的逐字相同** + **全量 E2E 在最终字节上 62/62 绿**（§5 第四批）+ **同一机制的红证**（上条 3）。

## 5 门禁与**读数归属**（C64/t20/C66）

`panel/dist` 在我这一轮里换过三次身份，按 C66 分开写（**不许**混成「E2E 全绿」）：

| 批次 | dist 身份 | 命令 | 读数 | 归属 |
| --- | --- | --- | --- | --- |
| dist-1 | 19:16 构建（**当时没哈希**，后被覆盖 ⇒ **不可回溯认证**，如实声明） | `npm --prefix panel run test:e2e` | exit 0 · `62 passed (18.2s)` | `t166`（另一个单，那个单的字节） |
| dist-2 | 20:14:24（**graph 修完 TS2693 后自己重建那份**）：js `index-DH3Te_St.js` `DC3B9AC3…` | 只跑了 E10 探针与 S3（**没跑全量 E2E**） | 探针：2×GET/200 | 本单的**过渡批** |
| dist-3（**权威批**） | 20:17:37（我第一次重建）：js `index-CCq7Z-KK.js` `1B26C7C1…` · tree hash `064656631fb879bc…` | `npm --prefix panel run build` exit 0 · 全量 E2E | exit 0 · `62 passed (19.7s)` | 本单（**已被下一批取代**） |
| **dist-4（最终字节，权威）** | **20:20:11**（NaN 哨兵修正后重建）：`index-BvDMgDmx.css` `E82633BB3F916C28…` · js `index-FCbDMJr7.js` `487C6BFEF6EB0D26…` · **tree hash `b540128213d33295bca92aa755046bf1143100c1520ca15964eca4a259151195`** · `Runtimes.tsx` `9D40A5638BCF6E21…` | `npm --prefix panel run build` **exit 0**（5.46s）· `npm run test:e2e`（仓库入口）**exit 0（退出码先于任何管道取），`62 passed (19.2s)`** | 逐 spec：`responsive 12 · views 12 · palette 7 · failure-visibility 6 · agents-health-third-state 5 · consumption 4 · graph-aliases 4 · wiki 3 · settings-capabilities 2 · chat/i18n/judge/knowledge/recall/registry/theme 各 1` = **62**；`stats={expected:62, skipped:0, unexpected:0, flaky:0}`、`errors: []` | **本报告所有 E2E 结论都属于这一批** |

- **A 门禁（C60 落地）**：E2E 全程跑在**我自己的**守护进程上：临时 root `%TEMP%\t164\root`（自写 `config/agents.toml`：alpha/beta scripted + judge）、**端口 19013**、`RUAGENT_PANEL_DIST=<repo>/panel/dist`、`RUAGENT_EMBEDDER=hash`；**E2E 走仓库入口**（写保护由 `run-e2e.mjs` 武装，**全程未用裸 `npx playwright test`**）；收尾**只按我记录的 PID**（`alive=False` 已核）；**8787 / pid 14944 全程未触碰**（多次核对：`8787 owner: 14944`）。
- **B 一次真实的归属事故（已按规矩处置）**：开工时 `npm --prefix panel run build` **exit 1**，错误是 `src/views/Runtimes.tsx(229,79): error TS2693: 'unknown' only refers to a type…`。它**不属于我**：`git status` 显示该文件是 ` M`（在途编辑），`git diff -U0` 里 `@@ -161,0 +225,6 @@` 是新增块，且**工作副本 L229**（`… !unknown[n]);`）与 **HEAD 的 L229**（`title={t("agents.runtimes.empty")}`）是完全不同的行 ⇒ 同伴在途改动的产物。我**没有替它改**，把它报给 captain（含文件/行/错误原文），并**先做不依赖 dist 重建的部分**；graph 修完后 `build` 转绿，我才走正规路径并**重取**读数（本节 dist-3/dist-4 就是重取的）。

## 6 我【没有】覆盖的（点名）

1. **9 处手工探针 `*.mjs`（19 − 10）本单没改**：`testMatch` 默认只收 `**/*.spec.ts` ⇒ 它们**不在 CI 的合并门里**，本单按契约只登记。⇒ 「CI 绿可信」这条链上它们不是承重点（但手工跑探针的人仍会撞到那 9 处猜测预算）。
2. **`failure-visibility.spec.ts:241` 的 2000ms 保持不变**（契约明确划在范围外：它后面**没有断言**，只污染读数）——只加了它的邻居（E9）的锚，没有动它。
3. **`scripts/**` 其它等待形状**：`cargo-team.ps1:263` 的 `Start-Sleep 10`（构建锁重试，90 分钟上限 + `exit 75`）与 `ruagent-canary.ps1:98` 的 `Start-Sleep -Milliseconds 750`（健康轮询 200 + 耗尽 `exit 1`）——**都在 out of scope**，且我核对过它们是 ①（有界 + 耗尽非零），本单不改。
4. **`#[ignore]` 16→17** 是 `t165` 的单，本单不碰。
5. **新字节还没在 CI 上跑过**：我不 push（推送是 captain 的事）⇒ 不存在「CI 上绿」的读数；我的证据是 §5 的 dist-4 全量 E2E + §2/§3/§4 的探针。**CI 的 `E2E evidence` 步与本案无关**（那是 panel 测试的跳过计数，不是这些等待）。
6. **N 及其它：本单没有「bound 的推荐值」**，只有「等到事实现已出现」；我没有给任何一处留下「以后记得调大/调小」的余地（唯一的 500ms 有界窗口在 §1 E10 行里说明了它的性质与它前面的正对照）。
7. **`panel/tools/design-audit.mjs` 与 `panel/src/views/Runtimes.tsx` 的改动不是我的**（`git status` 里同在，但我一行未动）⇒ 本单不认领它们，也不为其读数背书。
8. **我没重跑 design-audit 的 53 行**：本单**不改 `panel/src/**`** ⇒ 不可能移动任何审计行（t159 改 `.step-num` 颜色所影响的 `#home` 同路由行是它自己的事，已由 `t162`/`t159` 的验证覆盖）。这是**范围论证**，不是「我跑过了」。

## 7 纪律回执

- **写入集合**（`git diff --numstat`）：`panel/e2e/failure-visibility.spec.ts` 20/2 · `panel/e2e/responsive.spec.ts` 55/19 · `panel/e2e/settings-capabilities.spec.ts` 38/3 · `scripts/ruagent-daemon.ps1` 21/1 · 本报告。**没有新增被 git 跟踪的源文件**（探针全是 `%TEMP%` 下的临时文件）。
- **不许碰的两套安全机制未动**：`Get-DaemonIdentity` 的身份判定（注释 `:101`「never a name or port sweep」）逐字未动、`stop` 的 `-Force` 拒绝与 `daemon-stop-notice.log` 的追加/轮转（`:23-25/:41/:59`）逐字未动、`:153`「This path never calls 'stop': it only starts」逐字未动；我的改动**只在 `start` 分支的健康轮询那一小段**。
- **没启停活守护进程**：8787 / pid 14944 未碰；我起的守护进程按**自记 PID** 停（`alive=False`）；**一次例外已处置**：S3 正对照报出的 pid 是 `cmd.exe`、真守护进程仍在 ⇒ 我按**端口归属 + 命令行含我的临时 root + 非 8787** 认出并只停我自己的那个（`pid 36712`），并把它写成 §3 的发现。
- **收尾按具体路径删**（不用 glob）：`%TEMP%\t164` 下的探针/脚本/日志按名删，`node_modules` **junction 先 `rmdir` 再删树**（C39）；证据文件（`evidence.log`/`evidence3.log`/`e2e-t164.json`/`e2e-t164-stdout.txt`/`probe-e10*.json`）按名保留。
- **NO_PROXY**：浏览器/curl 走 `127.0.0.1,localhost,::1`；`git`/`gh` 用 `*`；**两者未混用**（本单没有跑 Rust 门禁）。
- 未 push / 未 dispatch / 未 `rerun`；未开共享树变异窗口；未用裸 `npx playwright test`。
