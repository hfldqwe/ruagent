# t189 独立验证 t164：轮询替换的等价性 + E10 正对照的承重 + S3 两条路径（mem-core，report-only）

- 被验证对象：`t164`（commit **`0b1772185d3a961c8d680a3396304ddb0da620a5`**）；本单 attempt `a8d6b836-fa73-4805-8141-421444026817`
- **我的唯一写入是本报告**；**判定：pass**（8 条全部我自己复核）——带 5 条边界、4 条我自己的 harness 错误、1 条我自己的假警报（CRLF），以及**一处对作者说法的精确化**（§6）

## 0 我量到的（一眼版）

| 项 | 作者的读数 | 我的读数 |
| --- | --- | --- |
| `await page.waitForTimeout(`（`panel/e2e/**`） | 10 → 2 | **10 → 2** ✓（逐提交逐文件，§1） |
| 剩下 2 处 | `:241` 2000ms · `settings-capabilities:409` 500ms | **同坐标** ✓，且两处理由我**自己判为成立**（§1） |
| E10 正对照 | 页面同 URL ≥1（2 次）、`puts==[]`、不可能路径 0 | **页面 1 次** ✓ / `APIRequestContext` **200 但采集器 delta=0** ✓ / **不可能路径被看见 1、计入真 URL 0** ✓（§2） |
| S3 失败路径 | `exit=1 in 116s`、`NOT healthy after 110s (pid=… log=…)` | **exit=1 / 116s / `NOT healthy after 110s (pid=36296 log=…\s3-root\logs\daemon.log)`** ✓（§3） |
| S3 成功路径 | `exit=0`、`started pid=… log=…`、`health ok` | **exit=0 / 24s**，两行原文 ✓（§3） |
| `started pid=` 语义 | 是 WMI `cmd.exe`，不是 `ruagent.exe` | **证实**：报出 **16232** vs 端口归属 **38996 (ruagent)** ✓（§3） |
| E7 锚顺序 | 点击**之前**挂上 | **L183-186 挂锚 → L187 click → L189-191 终端态 → L198-199 两条否定** ✓（§4） |
| `railWidth` miss 哨兵 | `NaN`（第一版 `-1` 会让 poll 立刻通过） | **`Number.NaN`** ✓；`-1` 确实会让**三处 `<=80`** 通过（`-1===228` 仍 false ⇒ 对 E4 不成立，§6） |
| 真套件 | `62 passed (19.2s)`、`skipped=0` | **exit 0 / `62 passed (19.2s)`**、JSON 两次读数 `62/62/0/0/0` ✓，并且**E10 那行在真跑里印出来了** ✓ |

## 1 重数（我自己的命令，不采信转述）

```
git grep -n "await page.waitForTimeout(" 0b17721^ -- 'panel/e2e/*.ts'
  failure-visibility.spec.ts: 178(1200) · 228(2000) · 250(1800)
  responsive.spec.ts:          35 · 51 · 66 · 83 (600) · 94 · 106 (300)      = 6
  settings-capabilities.spec.ts: 377(500)                                     = 1
  ⇒ 10
git grep -n "await page.waitForTimeout(" 0b17721  -- 'panel/e2e/*.ts'
  failure-visibility.spec.ts:241(2000) · settings-capabilities.spec.ts:409(500)  ⇒ 2
（HEAD 与工作树同为 2）
```

**剩下两处，我自己判理由是否成立**：
- **`failure-visibility.spec.ts:241`（2000ms）** —— 我读了上下文（L232-248）：它在 **`// DIAGNOSTIC (no assertion here …)` 块**里，后面只有三条 `console.log` 的 DOM READING，**没有任何断言**。⇒ 「把等待换成对断言的轮询」这条规则**在这里不适用**（没有可轮询的谓词，硬造一个就是替一个不存在的断言发明事实）。**理由成立**，而且比「契约划在范围外」更准确。
- **`settings-capabilities.spec.ts:409`（500ms）** —— 它前面（L381-387）就是**正对照** `expect.poll(() => capsRequests.length).toBeGreaterThan(0)`，后面（L410-411）**先打印所见请求**，再断言 `puts == []`（L413）。⇒ 保留一个有界的缺席窗口**且**把可证伪性移到正对照上，**理由成立** ✓。

## 2 承重点：E10 的正对照（我用**自己的**装置复现两侧）

我写了独立探针（与 spec **同一通道** `page.on("request")`），在我自己的守护进程 **127.0.0.1:19015** 上跑：

```
A_page_own_caps_requests=1                # 页面自己发的 GET /api/v1/capabilities ⇒ ≥1 侧成立
B_api_context_status=200                  # 同一个 URL 经 APIRequestContext 取，服务端确实回了 200
B_caps_count_delta_after_api_request=0    # …而页面采集器**一次都没看见**它 ⇒ 作者的论证成立
C_impossible_path_seen_by_page_on=1       # 页面 fetch 一个不存在的路径：采集器**看得见**
C_impossible_path_counted_for_the_REAL_url=0   # 但它不计入被测 URL ⇒ “0 侧”是对 URL 的读数，不是仪器死了
TOTAL_requests_seen_by_page_on=17
```
**论证我核了（不是转述）**：`page.on("request")` 只承载**页面自己网络栈**的事件；`APIRequestContext`（`context.request.*`）走另一客户端 ⇒ 同一个 URL、服务端 200、而页面采集器 delta **0** —— 这正是「页面发的」与「经 API 客户端发的」的**可分辨**读数 ✓。
**它什么时候会变成恒真**（我自己给判据）：正对照 `capsRequests.length > 0` 只有在**页面确实会对该精确 URL 发请求**时才成立 —— 它的口径与缺席断言**同一个** `includes("/api/v1/capabilities")`（我读了 L369-379 的采集器）；所以**会变恒真**的形状是：① 把正对照的 URL 放宽（例如 `/api/`）、② 把 `> 0` 改成 `>= 0`、③ 在**没有渲染**的页面上先给采集器喂一条自造的假事件。反过来，若 `#settings` 某天不再 GET 该 URL，正对照会**红** ⇒ 方向正确（页面不渲染时**不是**静默通过，而是 poll 超时红）。**没有**发现恒真风险 ✓。

## 3 S3 的两条路径（我自己跑，端口 19014 + 我自己的临时 root）

**成功路径**：`scripts/ruagent-daemon.ps1 start -Root <我的临时 root> -Exe <team 预编译 exe> -Addr 127.0.0.1:19014`
```
EXIT=0  elapsed=24s
  | started pid=16232 log=C:\Users\19410\AppData\Local\Temp\ruagent-t189\s3-root\logs\daemon.log
  | health ok
```
**失败路径**（我先把端口用一个「接受连接但永不应答」的 node 监听占住）：
```
EXIT=1  elapsed=116s
  | NOT healthy after 110s (pid=36296 log=C:\…\ruagent-t189\s3-root\logs\daemon.log) -- nothing answered
    http://127.0.0.1:19014/api/v1/health. The process may have exited (read the log) or another listener may hold 127.0.0.1:19014.
```
- `pid=` 与 `log=` **逐字保留** ✓；**没有**出现 `started`（我把它当失败路径的否证断言跑了一遍）✓；点名了原因 ✓。
- **110s 自洽**：`Test-Health` 原文是 `Invoke-RestMethod -TimeoutSec 5` × 20 轮 ⇒ 被「接受但不应答」的监听占住时 20×~5.5s ≈ 110s ✓（作者说这条修正是红证逼出来的，我实测同一个数）。
- 我自己的 root 里 `logs/daemon.log` 的尾巴**独立地**说明了原因：`os error 10048`（地址已被占用）✓ —— 也就是那条消息让读者去读的东西。
- **`started pid=` 的语义：证实。** 报出 **16232**，而 `GET /api/v1/health` 真的应答、端口归属是 **38996 (ruagent)** ⇒ 报的是 **WMI 包装的 `cmd.exe` pid**，按它收尾会**停错对象** ✓（既有行为，我只证实，未改）。
- **一条观察（只报）**：`<root>/data/daemon.pid` 里是**两个 token**（`38996 1791117876731`）。脚本自己的 `stop/status` 走 `Get-DaemonIdentity -Root … -PidFile …` 并校验身份（L75/125/159），不是裸 `-Id` cast；**我的**第一版脚本直接 `Stop-Process -Id (Get-Content …)` 因此报错 —— 那是我的问题，**不是**脚本的缺陷，但值得记：那个文件不是「一个整数」。

## 4 E7 的锚点顺序（读代码位置）

`failure-visibility.spec.ts`（HEAD 字节）：
```
183:  const auditLoaded = page.waitForResponse(
184:    (r) => r.url().includes("/api/v1/memory/diffs") && r.status() === 200,
185:    { timeout: 10_000 },
186:  );
187:  await openAuditLog(page);        <- 点击在这里，锚已经在上面被捕获（promise 未 await ⇒ 不会漏）
188:  await auditLoaded;
189-191: await expect(page.locator(".content .card, .content .ant-empty, .content .error-state").first()).toBeVisible();
193-199: const body = …; expect(body).not.toMatch(/召回失败|Recall failed/); expect(body).not.toMatch(/无法读取记忆|Cannot read memory/);
```
⇒ 锚**在点击之前**挂上 ✓；两条否定断言**在终端态断言之后** ✓（而且它们读的是一张已终态的卡 ⇒ 比原来「睡 1200ms 再读」更强）。

## 5 谓词等价性（我自己 diff 的逐处对照）

| 处 | 被替换的旧形状（`0b17721^` 原文） | 新形状（HEAD 原文） | 谓词是否逐字相同 |
| --- | --- | --- | --- |
| E1 `responsive:51`（390px×9 路由） | `waitForTimeout(600)` + `boundingBox()` + `expect(sider?.width).toBeLessThanOrEqual(80)` | `expect.poll(() => railWidth(page), {timeout:5_000, message}).toBeLessThanOrEqual(80)` | **`<= 80` 逐字相同** ✓ |
| E3 `responsive:85`（768px） | 同上（600ms） | 同上（message 点名路由） | **`<= 80`** ✓ |
| E4 `responsive:103`（desktop 展开） | `waitForTimeout(600)` + `expect(wide?.width).toBe(228)` | `expect.poll(railWidth…).toBe(228)` | **`toBe(228)`** ✓ |
| E5（折叠后再展开） | `waitForTimeout(300)` + `<= 80` | `expect.poll(railWidth…).toBeLessThanOrEqual(80)` | **`<= 80`** ✓ |
| E2 `responsive:66`（chat 滑出层） | `waitForTimeout(600)` 然后 `click()` | `await expect(locator).toBeVisible()` 然后 `click()` | 谓词升级为**元素可见性**（带界，`expect.timeout=8_000`，我读了 `playwright.config.ts:72`）✓ |
| E6（再展开） | `waitForTimeout(300)` + `toBeVisible()` | **只留 `toBeVisible()`**（等待完全冗余） | 断言原样 ✓ |
| E9 `failure-visibility` | `toBeVisible(card)` + `waitForTimeout(1800)` + 读文本 | `toBeVisible(card)` + `expect(card).toHaveText(/未探测\|not probed/, {timeout:15_000})` | 旧文本断言 → **同一形态的样板**（同文件 F2 `:277` 同正则）✓ |
| E7 | `openAuditLog()` + `waitForTimeout(1200)` + 两条否定 | 锚（`waitForResponse` + 终端态）+ 两条否定 | 否定谓词原样，**加了正锚** ✓ |
| E10 | 窄采集器 + `waitForTimeout(500)` + `puts==[]` | 宽采集器 + **正对照** + 打印所见 + 保留 500ms 有界窗口 | 缺席谓词 `puts==[]` 原样 ✓ |

**没有一处调大时长、放宽断言或加 `skip`**（我看了三类文件的完整 diff 行）✓。

## 6 【C70】哨兵方向：我自己验（含一处对作者说法的精确化）

**读字节**：`responsive.spec.ts:31-34` 的 `.catch(() => Number.NaN)` ⇒ 哨兵是 **`NaN`**，不是 `-1` ✓。
**我自己的行为读数**（把那段表达式逐字复刻成探针，chromium）：
```
railWidth_on_a_page_without_a_sider = NaN     (isNaN=true)
railWidth_with_a_80px_sider        = 80      (⇒ 它不是恒为 NaN 的常数读取器)
predicate                 NaN(miss)    -1(被拒哨兵)    真值 80px
  <= 80                    false        true            true
  === 228                  false        false           false
  toBeLessThanOrEqual(80)  false        true            true
```
再加上被替换的行为（实测）：`undefined <= 80` = **false**（`boundingBox()` 缺席 ⇒ `sider?.width` 是 `undefined`）⇒ **方向正确**：缺席时旧代码**失败**、`NaN` **也失败**（且会轮询到超时后响亮点名）、而 `-1` 会**通过** ⇒ 作者的自我修正是必要的 ✓。
**精确化（不矛盾）**：`-1` 会让三处 **`<=80`** 的 poll 立刻通过；但对 E4 的 **`toBe(228)`**，`-1 === 228` 仍为 **false** ⇒ E4 不受那条自伤影响。作者原话是「E1/E3/E5 的 poll 会立刻通过」——**与我的读数一致** ✓（我把它写清楚，免得下一位以为四处都受影响）。

## 7 我的覆盖层 / 没覆盖什么（C63 口径）

**覆盖到的**：
- **静态字节**（都在 HEAD 字节上读）：重数 10→2、两处保留的判断、E7 顺序、九处谓词对照、`railWidth` 哨兵、`started pid=`/`log=`/`exit 1`/`health ok` 的原文、`Test-Health` 的 5s 上限、`playwright.config.ts` 的 `expect.timeout=8_000`。
- **动态读数（我跑的）**：S3 两路（失败 116s/exit 1、成功 24s/exit 0）、E10 三侧（真 URL ≥1、`APIRequestContext` 看不见、不可能路径看见但不计入）、C70 的 NaN/`-1`/`undefined` 谓词表、**真套件 62/62 exit 0 且 E10 那行在真跑里印出来**、以及 `~/.ruagent` 未被写（sha+mtime 前后同为 `be898952dcf1d23e…` / 2026-09-29 04:55:13）。

**没覆盖（点名）**：
1. **没在 CI 上跑**（我不 push）⇒ runner 上的行为只有静态论证。
2. **E1-E6/E9 的「应用侧」红我没有制造**：要确定性变红必须**破坏 `panel/src/**`**（让 sider 不折叠、让探测文本不出现），那是**本单 out of scope**，我也不开共享树变异窗口（作者同样如此）。⇒ 这几处的「不误红」证据是：**谓词与被替换的逐字相同**（§5，我 diff 过）+ **全量 E2E 在最终字节上 62/62 绿**（§3/§7 的我的那份读数）。**这属于「另一类证据」，不是「我跑红了」**（C63）。
3. **9 处 `.mjs` 探针**：`0b17721` 一个 `.mjs` 都没碰（我数了）✓，且它们**不在套件里**：`playwright.config.ts:52` 只设 `testDir: "./e2e"`、**没有 `testMatch`** ⇒ 默认匹配器不认 `*-probe.mjs` / `x4-parity.mjs` / `i18n-check.mjs` / `bundle-stats.config.mjs` / `run-e2e.mjs` 这些**不带 `.spec.`/`.test.`** 的名字 ⇒ 那 9 处固定等待在 CI 门之外（我**没有**跑它们）。
4. **`scripts/ruagent-canary.ps1` / `scripts/cargo-team.ps1`** 未跑（作者声明 out of scope；我只核到它们的形状是有界 + 耗尽非零的是作者的说法，我**没有**读它们——那不在本单判据里）。
5. **没有重跑 design-audit**（本单不改 `panel/src`，与作者的范围论证一致）。

**我这次跑的工具身份（C69）**：
```
panel/e2e/run-e2e.mjs      19c57bf626dfc7d75f0957ca0eca2200e4e2af4e709af10a6253fca7ace21831
panel/e2e/write-guard.ts   bda940844e678bb65830118a18c3773e2c057f5f8c17813bbc62a1224ed1ab32
panel/dist/index.html      64b49e044a1e3373fd2422bc4d3ce12d0ecfb3546e15c4b37e0953cb7d2edf73
panel/dist/assets/index-C3qaG-gm.js   bd729035e7e5baafdea3a254b8bae91336c178f0cde6c7271c1009212b12964f
panel/dist/assets/index-BBnBpmKB.css  6818528ea170d2ee8f9bd8955a08c06199ffd96c3dbdbba8709dccc8dbb979e2
port 19015 · RUAGENT_HOME=%TEMP%\ruagent-t189\root · RUAGENT_PANEL_DIST=<repo>\panel\dist
writes: ARMED via the entry point (RUAGENT_E2E_ALLOW_WRITES=1) · CI 未设 · 8787 一次未碰
```
**必须说清的仪器差异**：① 我的入口点是**我 t179 改过之后**的 `run-e2e.mjs`（上面这个哈希），**不是** t164 当时那个（t164 的读数属于它自己的字节）；② 三个 spec 文件**与 HEAD 逐字节相同**（`12a066be…`/`f538adee…`/`341aeda3…`）⇒ **被测的那批字节就是 t164 的** ✓；③ 我跑套件时 `panel/dist` 是由**含同伴在途编辑**的 `panel/src`（`App.tsx` mtime 20:36:45、`Settings.tsx` 20:34:35、`index.css` 20:47:20，均**早于**我 20:49:31 的运行）构建的 ⇒ 「62/62」是关于**当前树**的读数，不是「只关于 t164 的提交」；④ 我用完后跑了门禁构建，dist 现在是 `index-DUP7Q-s-.js`（20:51:23，**我的**那次构建），**我那次跑的 `index-C3qaG-gm.js` 已不在盘上** ⇒ 只能按哈希引用。

## 8 门禁与残留

| 命令 | 读数 |
| --- | --- |
| `npm --prefix panel run build` | **exit 0**（`✓ built in 5.24s`） |

- 我的临时物：`%TEMP%\ruagent-t189\`（`facts*.sh`/`facts*.txt` · `e10probe.js` · `c70.js` · `s3b/s3c.ps1`+日志 · `e10-suite*.ps1`+日志 · `root/`（我的 RUAGENT_HOME）· `s3-root/` · `playwright-evidence.json` · `pw-out/`）——按**具体路径**删。
- 我的进程：S3 成功路径的守护进程、E10/套件的守护进程、以及一个我自己的残留（**38996**，被我按「端口 19014 + 命令行含 19014」认出是**我的**之后停掉，C71 口径：**没有**按名字/端口批量杀）——全部已停；19014/19015 现在空闲。**8787 全程未碰**（连健康探测都没有）。
- **真实 `~/.ruagent` 全程未碰**：`config/agents.toml` 的 sha256 `be898952dcf1d23e…` 与 mtime `2026-09-29 04:55:13` 在我所有运行**前后相同** ✓（我的两个 root 都在 `%TEMP%` 下）。

## 9 我自己的错误与假警报（每条都曾产出过错读数）

1. **TOML 反斜杠**（我此前已记过一次，又犯了）：把 Windows 路径写进 TOML 双引号串 ⇒ `\U` 是转义 ⇒ `parsing … agents.toml / TOML parse error at line 3 column 16`，守护进程起不来、**套件没跑**。改成正斜杠后正常。⇒ 这是**第二次**，我把它写进报告以免第三个人踩。
2. **PowerShell 的 `$PORT:`** 被当成驱动器限定变量 ⇒ 整个脚本 ParserError、S3 一次都没跑成。改用 `${PORT}`。
3. **我按 pid 文件裸 cast 停进程**：`<root>/data/daemon.pid` 是 `38996 1791117876731` 两个 token ⇒ `Stop-Process -Id` 报错，**我的**守护进程活着，导致我的第一次「失败路径」实验变成 `already running (health ok)`（脚本行为正确，是我的装置脏了）。⇒ 教训：那个文件不是单个整数；脚本自己用 `Get-DaemonIdentity` 校验身份。
4. **一条假警报（CRLF）**：我用 `git cat-file blob HEAD:file` 与**工作树**比哈希，判 `scripts/ruagent-daemon.ps1` 为 `CHANGED`（`259ac74f…` vs `3ac6dd31…`）；而 `git diff HEAD --` 对它是**空的**，工作树有 **504 个 CR**、HEAD blob 有 **0 个** ⇒ **差异只是换行**。按换行归一后四处区域（`NOT healthy after` / `started pid=` / `health ok` / `exit 1`）**全部 IDENTICAL** ⇒ 我跑的 S3 就是 t164 的字节。⇒ 与 C68 同族：**「CHANGED」先问是不是编码/换行**，`git diff` 才是内容变化的仲裁。
5. 我 E10 探针里 `B_api_request_visible_to_page_on` 那个表达式写坏了（`seen.some(...) && caps.length === before` ⇒ 恒真时也打印 true）；**该行标签无意义**，但同一格里的 `status=200` 与 `delta=0` 是真读数 ⇒ 我在 §2 只用这两个。
6. `<RUNNER_TEMP>`/root 之外：我还把 dist 当成「稳定产物」了一阵 —— 它在我两次运行之间被同伴/我自己重建过（`index-C3sdE6IH.js` → `index-C3qaG-gm.js` → `index-DUP7Q-s-.js`）⇒ 与 t162 报告里的「活目录陷阱」同一个形状：**引用 dist 必须带哈希**。

## 10 作者边界声明：我判到哪一层

- 「**22 处均无前置红证 ⇒ 只写移除一个假绿来源**」：这句话的**口径**我认，但它把哪些站算进「22」**报告里没有列全**（报告开头给的是「19 处 `waitForTimeout`：10 在 spec、9 是 `.mjs`」，而 S3 是第 20 项）。我能独立核的是 **10 处 spec 站**（§1）、**9 处 `.mjs` 未改且不在套件里**（§7）、以及**两处 `scripts/**` 被具名**（§7）。⇒ 这是**低危的文档性缺口**：建议下一位把 22 的组成列出来（否则「22」不可复算）。**这不是实现缺陷**。
- 「**未修 `:241` 的 2000ms**」：成立，且我的判断比作者的理由更强（§1：那里根本没有断言）。
- 「**未跑 `scripts/{cargo-team,ruagent-canary}.ps1`**」：与我的范围一致（我也没跑）。
- 「**7 处 `.mjs` 探针只登记不改**」：我数到的是 **6 个 `.mjs` 文件 / 9 处固定等待**（`0b17721` 未碰任何 `.mjs`）；**「7 处」与我的读数不一致 —— 但这可能是作者在数别的口径（例如包括 `run-e2e.mjs` 或 t161 盘点表里的另一项），我不能断言它是错的，只能给出我数到的**：这是本报告唯一与作者数字不完全对齐之处，**建议 captain 让作者补一句它的口径**。
- 「**新字节未过 CI**」：与我的范围一致（我也没有 push）。
