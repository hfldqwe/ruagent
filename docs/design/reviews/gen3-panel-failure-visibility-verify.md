# t102 独立验证：t84 面板失败可见性（P2/P3）—— 四个 DOM 读数 + 成功态逐字对比

> **性质**：**独立验证**（本单我是验证者，不是 t84 的实现者）。**只读**：没有改 `panel/src/**`（t84 的产物），唯一新增是本次的断言 spec + 本报告。
> **验证对象**：t84 的两条「失败必须看得见」—— `memory.recall.err`（P2）与 `runtimes.probeFailed`（P3）。
> **结论**：**三条主张全部观察到成立**（P2 失败出现「召回失败」且不出现「没有记录」类文案；P3 失败在**新 chip** 上出现「探测失败 · 重试」；成功态与**修改前逐字一致、差异 = 0**），**负控成立**（同一份 spec 打到修改前的构建 ⇒ **2 failed / 2 passed / exit 1**）。同时**交出两条 finding**：④「汇总不再把失败当 0」在**全失败**情形下**仍读到 0**；以及 t84 新加的 `probeFailed` 是**闩锁**——一次**成功**的重试不会清掉它（卡片仍写「探测失败 · 重试」、汇总仍排除刚测到的 3 个模型）。
> **时间**：2026-09-29 04:2x–04:4x +08:00。

---

## 1 环境：两个一次性守护进程 + 两份 dist（都是我自己起的，用完即停）

| | **改后（t84 在树里）** | **改前（pre-t84）** |
| --- | --- | --- |
| 守护进程 pid | **84000**（我已按记录到的 PID 停掉） | **91224**（同上） |
| 监听 | `127.0.0.1:8891` | `127.0.0.1:8892` |
| 数据根 | `%TEMP%\t102-root-after`（一次性，已删） | `%TEMP%\t102-root-before`（一次性，已删） |
| 面板 dist | 仓库 `panel/dist`（t84 构建，2026-09-29 04:20:51） | `%TEMP%\t102-before\panel\dist`（我从 **`0a39e5b`** 重新构建） |
| 服务出去的 bundle | `/assets/index-BBMxa5To.js` | `/assets/index-1EnVxpf5.js` |
| 启动方式 | `ruagent.exe serve --addr 127.0.0.1:8891 --root <temp>` + `RUAGENT_PANEL_DIST=<dist>` | 同形，端口/根/dist 换成上面那套 |

* **"修改前"= 哪个提交（这条必须先说清，否则对比无意义）**：t84 的改动**在本次会话期间才被提交**：`git log -S logError -- panel/src/views/Memory.tsx` ⇒ **`0da0cb6`**（`feat(gen2): …四区收口…`）是引入 `logError`/`probeFailed` 的提交 ⇒ **修改前 = 它的父提交 `0a39e5b`**。我先前的读取（工作树里 `M panel/src/views/Memory.tsx`）在会话中途变成了"已提交"，`git status --porcelain -- panel` 现在为空 —— 所以"修改前"必须用 `0a39e5b`，**不能用 HEAD**（HEAD 已是改后）。
  证据：`git archive 0a39e5b panel` 出来的树里 `logError` 命中 **0**、`probeFailed` 状态变量命中 **0**，而 R11 那条 `t("runtimes.probeFailed")` **命中 1**（见 §4 的判据讨论）。
* **dist 归属是靠 bundle 名对上的**（不是靠"我以为"）：`panel/dist/index.html` 里写的是 `assets/index-BBMxa5To.js`，而 8891 服务的正是它；`t102-before/panel/dist/index.html` 写的是 `assets/index-1EnVxpf5.js`，8892 服务的正是它 ⇒ 两个 daemon 各自服务自己的 dist。
* **一条被我自己否掉的"证据"（方法论留档）**：我本来想用"bundle 里有没有『召回失败』"来区分两份构建——**无效**：i18n 字典是整体打包的，而 `memory.recall.err` **在 `0a39e5b` 就已存在**（`git show 0a39e5b:panel/src/i18n/memory.ts` 有该键，"修改前"的 bundle 里也有这个字符串）。**真正区分两份构建的是行为（§4 的负控）与 dist 归属（上面这条）**，不是字符串是否存在。
* **活库/真 daemon**：`pid 79984` 全程未动（收尾读数：**alive = True**），`8787` 全程未动（收尾：**listening = True**）。我起的两个都是**一次性 root + 非 8787 端口**。

---

## 2 让失败真的发生：四个 DOM 读数（改后构建）

**注入方式**：`page.route()`（浏览器侧拦截，**不碰 daemon**、不改配置、不写任何数据）：
`**/api/v1/recall/log*` → 500；`**/api/v1/agents/*/options*` → 500。spec 见 `panel/e2e/failure-visibility.spec.ts`。

| # | 判据 | 读数（原样） | 结论 |
| --- | --- | --- | --- |
| ① | 「召回失败」文案**出现** | `DOM READING P2 error zone: 最近召回召回失败召回失败` | ✅ 出现（zone 标题「最近召回」+ 注记与正文两处「召回失败」） |
| ② | 「没有记录 / 0 行」类文案**不出现** | `DOM READING P2 empty-vocabulary hits: null`（正则在整块 `.content` 上找 `没有命中\|暂无有效事实\|暂无记忆\|还没有运行记录\|0 条\|0 rows\|No hits\|No current facts\|No runs recorded yet`） | ✅ 一个都没出现 |
| ③ | 「探测失败」文案**出现** | `DOM READING P3 claude card: …npx @agentclientprotocol/claude-agent-acp探测失败 · 重试尚无角色使用此运行时…`；`DOM READING P3 cards: ["claude=[…探测失败 · 重试…]","dsh=[…探测失败 · 重试…]","opencode=[…探测失败 · 重试…]"]`；`DOM READING P3 in-card R11-alert nodes: 0` | ✅ 三张卡都出现；**且卡内 `[role=alert]` = 0** |
| ④ | 汇总**不再把失败当 0**（对账汇总数字） | `DOM READING P3 models readout (all probes failed): 0`（三张卡全是「探测失败」）；对照成功态 `DOM READING P3-success models readout: 0`（三张卡全是「未探测」） | ❌ **不成立**：两种情形都读到 `0` ⇒ 见 **F1** |

**③ 为什么不是"在旧代码上也通过"的伪证（这条是本单最重要的方法学读数）**：
`runtimes.probeFailed` 这个 i18n 文案**在 t84 之前就被 R11 的同步告警用了**（`0a39e5b` 的 `Runtimes.tsx` 里命中 1 次，渲染在 `probeErr` 上）。所以"页面上出现「探测失败」"这句话**在旧构建上也可能是真的**。本单用两条把它按死：
1. **`probeErr`（R11）只由同步按钮设置，挂载探测不设置它** ⇒ 挂载失败这一场景下**卡内 `[role=alert]` 计数必须是 0**，实测 **0** ⇒ ③ 读到的文字只能来自 t84 新加的 chip。
2. **负控（§4）**：同一份 spec 打到修改前的构建，③ 直接红，收到的卡片文本是 **「未探测」**（旧行为：失败被渲染成"还没探测过"）⇒ 判据在旧代码上**不通过**。
* 同理，① 也不是"`memory.recall.err` 出现在页面上"这种宽判据：该键在 `0a39e5b` 就被**召回演练场**（`Memory.tsx:1165` 的 `ErrorState`）用着，而本单 ① 断言的是 **`section.zone`（审计日志页上的失败卡）且与「最近召回」同处一个 zone**——演练场的 `ErrorState` 不是 `section.zone`。负控里 ① 同样红（旧构建根本没有那张 zone）。

---

## 3 成功态对照（重点）：错误文案不出现，且与修改前**逐字一致**

**成功态怎么造的**：`/api/v1/recall/log` → `200 {"log":[],"retention":{…}}`（**空日志**，正是接受标准点名的 `200 + []`）；`agentOptions` → `200 {"options":[]}`（**真 0 models**——不是失败、不是"未探测"）。

| 读数 | 改后 | 改前 |
| --- | --- | --- |
| memory 成功态错误文案计数 | `0` | `0` |
| memory 成功态区域快照 | `记忆 记忆 平台记住的东西——每个智能体都通过 MCP 读写 浏览 召回 审计日志 0 画像 0 观察 0 流程 0 经验 还没有写入记录——每次写入决策都会记在这里。 每一次写入与取代决策都会落在这里。` | **同一串** |
| runtimes 成功态 chip（真 0 models） | `…claude-agent-acp未探测尚无角色使用此运行时直连对话编 辑删 除` | **同一串** |
| runtimes 成功态错误文案计数 | `0` | `0` |
| runtimes 成功态汇总条 | `3运行时0智能体0模型` | `3运行时0智能体0模型` |

**「差异 = 0」的证据（快照来源与取法）**：
* **取法**：spec 里在成功态把区域文本 `innerText()` 取出、`replace(/\s+/g," ")` 归一化后，用 `console.log("SNAP-BEGIN <区域>|<文本>|SNAP-END")` 打印（三个区域：`.content`（视图区，排除侧栏）= memory 页；`.readout-strip.grid` = runtimes 汇总条；`.content` 内 `claude` 卡的文本）。两次运行各落一份日志，再用 PowerShell 逐字节比较：
  ```powershell
  # after/before 两次聚焦运行的输出各存一份，然后：
  $a = (Get-Content t102-after-run.txt  | Select-String 'SNAP-BEGIN memory-success' | Select-Object -First 1).Line
  $b = (Get-Content t102-before-run.txt | Select-String 'SNAP-BEGIN memory-success' | Select-Object -First 1).Line
  $a -ceq $b        # ⇒ True
  ```
* **结果**：三个区域**全部 `-ceq True`（大小写敏感相等）** —— memory 视图、runtimes 汇总条、runtimes 卡片文本，**改前 = 改后逐字一致，差异 0**。也就是说：**t84 新增的失败分支没有扰动成功路径的渲染**。
* **这条对比的可比性前提（必须写明）**：快照只在**同一空 root 状态**下可比。我用的两次**聚焦运行**（只跑本 spec）里 root 是干净的（审计页的写入账本为空）⇒ 两串一致。**全套运行不能用**：`consumption.spec.ts` 会并发往同一个 root 写一条记忆，所以全套运行里 memory 页的快照变成长串（含 `1 观察` 与 `2026-09-28 skip_dedupe …` 行）——**那不是 diff，而是写入了数据**。本单的"差异 = 0"因此**只以两次聚焦运行为准**，并把这个前提写在这里，避免下游拿全套运行的数字来质疑。
* **范围说明（不是缺陷）**：**非空日志**的成功态不在本对比里——同一次提交（`0da0cb6`）还改了召回行上的**量纲文案**（`m 0.86` → `记忆腿余弦 0.86`），所以有日志行时前后**本来就应该不同**。本单按接受标准用 `200 + []` 做逐字对比。

---

## 4 负控：同一份 spec 打到**修改前**的构建 ⇒ 必须红（已红）

```
E2E_BASE_URL=http://127.0.0.1:8892  npm run test:e2e -- failure-visibility.spec.ts
→ 2 failed / 2 passed (18.8s)   EXIT=1
```
两条失败**正是**两条修补要抓的东西，报错文本原样：
```
1) P2: a 500 on /recall/log is an ERROR, not an empty log
   expect(locator).toBeVisible() failed
   Locator: locator('section.zone').filter({ hasText: /召回失败|Recall failed/ }).first()
   - waiting for locator('section.zone')…   (10s 超时)
   ⇒ 旧构建**什么都没渲染**（Playwright 的 DOM 快照里，审计页只有写入账本的「还没有写入记录」，没有召回日志卡）

2) P3: a failed mount probe says so on the CHIP…
   expect(locator).toHaveText(expected) failed
   Expected pattern: /探测失败|probe failed/
   Received string:  "ClaudeCodeclaude已启用Claude Code via the official ACP adapternpx @agentclientprotocol/claude-agent-acp未探测尚无角色使用此运行时直连对话编 辑删 除"
   ⇒ 旧构建把失败渲染成 **「未探测」**
```
* **两条成功态对照在改前构建上也通过**（`✓ 2 passed`，快照与改后逐字相同）——这是**有意**的：成功路径本来就不该变（§3 的差异 0 正是这条的强形式）。
* ⇒ **①/③ 两条判据是"能红的"**（不是只在旧代码上也通过的恒真断言）；**② 是守卫条件而非判别条件**（旧构建也不出现"0 条/没有记录"，因为旧构建什么都不显示）——这一点我如实写在这里，不把它当成"修补生效"的证据。

---

## 5 findings（交回 owner，**我没有改一行 `panel/src/**`**）

### F1（medium · 不可辨）④「汇总不再把失败当 0」在全失败情形下不成立：汇总仍读 `0`

* **`file:line`**：`panel/src/views/Runtimes.tsx:200-205`（`value: Object.entries(counts).filter(([n]) => !probeFailed[n]).reduce((s, [, n]) => s + n, 0)`）。
* **读数**：失败态（3 张卡全是「探测失败 · 重试」）⇒ `models readout: 0`；真 0 models 成功态（3 张卡全是「未探测」）⇒ `models readout: 0`。**两种根本不同的处境给出同一个数字。**
* **为什么重要**：这就是本代反复抓的「不可辨/把失败当 0」。修法的**意图**是对的（把失败从求和里剔掉），但"剔掉一个空集合再求和"仍然是 `0` —— 而 `0` 在读者眼里是**一个测量值**（"这个部署一个模型都没配"），不是"一个模型都没能测到"。真正的判别信息只在**卡片**上（「探测失败」vs「未探测」），汇总层没有。
* **可证伪的修复判据**：当**没有任何一次探测成功**时，汇总**不得**呈现一个数字：要么渲染第三态（`—` / `unknown` / 「未能测量」），要么只在"至少一次成功"时才显示求和。测试：全部 `agentOptions` 失败 ⇒ 汇总读数 **不是 `0`**（第三个状态出现）；任一成功 ⇒ 求和只统计成功的那些（今天的 filter 行为）。
* **建议 owner**：integ（t84 的作者）/ `panel/src/views/Runtimes.tsx`。

### F2（medium · 回归）新加的 `probeFailed` 是**闩锁**：一次**成功**的重试不会清掉它 ⇒ 卡片与汇总都停在失败态

* **`file:line`**：`panel/src/views/Runtimes.tsx:38-48`（只有**挂载探测**的 `.then` 会 `setProbeFailed(…false)`）vs `:56-67`（`sync` 只 `setCounts` + 返回 bool，**从不**清 `probeFailed`）；chip 在 `:283-284`，汇总在 `:200-205`。
* **读数（改后构建，诊断段，无断言）**：挂载探测失败 ⇒ 卡内 `[role=alert]`=0、卡片「探测失败 · 重试」；随后把 `agentOptions` 换成**成功**（`{"options":[{"category":"model","choices":[3 个]}]}`）并点该卡的同步按钮 ⇒
  ```
  DOM READING P3-after-successful-retry card: …claude-agent-acp探测失败 · 重试尚无角色使用此运行时…
  DOM READING P3-after-successful-retry models readout: 0
  DOM READING P3-after-successful-retry in-card R11-alert nodes: 0
  ```
  ⇒ 面板**刚刚被告知有 3 个模型**，卡片却仍说「探测失败 · 重试」、汇总仍**排除**这 3 个。改前构建同序列本应显示「3 个模型」/汇总 3（**这一侧的读数我未取到**：负控在更早的断言处就红了，见 §6 的未取到清单；F2 的成立**不依赖**它——"成功之后仍显示失败"本身即是缺陷）。
* **顺带（同一处、更小的一条）**：chip 的文案是「探测失败 · **重试**」，但 chip 是**不可点的 `<span className="tag">`**（`:284`），真正可点的重试是 R11 的 `<button class="tag err">`（`:303-311`）。该文件自己在 `:300-302` 论证过"文案承诺了动作就必须真的提供动作"——chip 这条把同一个错误又犯了一次。
* **为什么重要**：这是**同一次提交引入的行为回归**：在"挂载时探测失败、用户点重试且重试成功"这条**最常见**的恢复路径上，改前能显示 3 个模型，改后显示"探测失败/0"并且**不会再自愈**（挂载探测只在 `key` 变化时重跑，重试成功不清标志）。用户看到的是一个**永远修不好的失败**。
* **可证伪的修复判据**：`sync` 成功时必须 `setProbeFailed(name=false)`（并让汇总重新包含它）；同时 chip 要么是可点的重试按钮、要么文案不承诺「重试」。测试：挂载失败 → 同步成功（返回 3 个模型）⇒ 卡片读到「3 个模型」且汇总包含 3。
* **建议 owner**：integ（t84 的作者）/ `panel/src/views/Runtimes.tsx`。

---

## 6 未覆盖什么（第 19 条；以及**未取到**的两项）

* **未取到（写清原因，不含糊）**：
  1. **改前构建"挂载失败 → 重试成功"的读数**：负控运行在更早的 chip 断言处就红了（P3 测试整体 15s 超时后计为 failed），诊断段没执行。F2 不依赖它（见 §5 F2）。
  2. **改前构建 P2 失败态的"文案全量"**：旧构建不渲染那张 zone，我只从 Playwright 的失败 DOM 快照读到"没有召回日志卡、只有写入账本的空文案"——**没有**逐字断言旧构建整页文本（因为判据①已在更早处红）。② 在旧构建上的读数同样是"未取到"，理由相同（它排在①之后）。
* **只覆盖 Chromium / 单一视口**：Playwright 项目默认 Chromium；本次**没有**改 viewport（默认桌面尺寸）。`responsive.spec.ts` 覆盖 390px/768px 的**页面能装下**，**不覆盖**本单的失败可见性路径。
* **只覆盖 zh-CN 一种语言**：`playwright.config.ts` 的 `use.locale = "zh-CN"`；我的正则是中英并列（`/召回失败|Recall failed/` 等），但**我观察到的是中文**，**没有**切到 English 复验一遍（`i18n.spec.ts` 只覆盖导航词汇）。
* **只覆盖两个面**：Memory 的**审计日志**标签页与 Runtimes 页。没有覆盖：召回演练场（`RecallPlayground`，`memory.recall.err` 的**另一个**使用点）、Wiki 的第三态（同一次提交里的另一处修补）、会话/看板/统计/收件箱的失败态。
* **只覆盖两种故障注入**：`recall/log` 500 与 `agents/*/options` 500。**没有**覆盖：网络断开/请求中止（`route.abort`）、超时（never-resolving）、非 500 的 4xx、以及 **WebSocket/SSE** 通道的失败（面板有实时通道）。
* **没有覆盖"失败之后面板是否会自己恢复"**：除了 F2 那一次人为重试，我没有验证轮询/重挂载能否自愈（Runtimes 的 `useModelCounts` 只在 `key` 变化时重跑）。
* **没有覆盖其它 runtime**（dsh/opencode）的成功态 chip：成功态对照只断言了 `claude`（失败态三张卡都读了）。
* **没有覆盖 t84 提交里的其它改动**：量纲文案（`记忆腿余弦`）、`api.ts` 的 wiki 字段、`Wiki.tsx` 的 freshness 三态、`tools/design-audit.mjs`。
* **单一 root 状态**：一次性 root 里 `mock` 是 disabled 的（默认种子），所以我**没有**跑通需要 mock agent 的 spec（见 §7 的 3 skipped）。这不是覆盖缺口而是环境差异，但要说明：本单的"全套通过"是在**一个没有 mock agent、没有记忆**的空 root 上取得的。

---

## 7 复现命令（全部照抄可用）

```powershell
# 0) 一次性 daemon（改后）：注意 --root 是临时目录，端口不是 8787，绝不动真 daemon
$exe = "$env:TEMP\ruagent-team-target\debug\ruagent.exe"      # 团队共享 target 里的 CLI
$env:RUAGENT_PANEL_DIST = "C:\Users\19410\Documents\ai\ruagent\panel\dist"
Start-Process -FilePath $exe -ArgumentList 'serve','--addr','127.0.0.1:8891','--root',"$env:TEMP\t102-root-after" -PassThru -WindowStyle Hidden
# 健康：Invoke-RestMethod http://127.0.0.1:8891/api/v1/health   ⇒ {"status":"ok",...}
# 归属：为确认它服务的是这份 dist，比对 index.html 里的 assets/index-*.js 与 dist 目录里的那份

# 1) 改后的四个 DOM 读数（4 passed / exit 0）
cd C:\Users\19410\Documents\ai\ruagent\panel
$env:E2E_BASE_URL="http://127.0.0.1:8891"; npm run test:e2e -- failure-visibility.spec.ts

# 2) 契约的 Verify（全套，仓库入口）⇒ 49 tests / 46 passed / 3 skipped / 0 failed / exit 0
$env:E2E_BASE_URL="http://127.0.0.1:8891"; npm run test:e2e

# 3) 修改前的构建（"修改前" = 0da0cb6 的父提交 0a39e5b；不要用 HEAD）
$b="$env:TEMP\t102-before"; Remove-Item -Recurse -Force $b -EA SilentlyContinue
New-Item -ItemType Directory -Force $b | Out-Null
git archive 0a39e5b panel | tar -x -C $b
New-Item -ItemType Junction -Path "$b\panel\node_modules" -Target "C:\Users\19410\Documents\ai\ruagent\panel\node_modules" | Out-Null
cd "$b\panel"; npm run build            # ⇒ 55 assets，dist updated
# 再用同样的 Start-Process 起 8892，并把 RUAGENT_PANEL_DIST 指向 "$b\panel\dist"

# 4) 负控：同一份 spec 打到改前 ⇒ 必须红（2 failed / 2 passed / EXIT=1）
cd C:\Users\19410\Documents\ai\ruagent\panel
$env:E2E_BASE_URL="http://127.0.0.1:8892"; npm run test:e2e -- failure-visibility.spec.ts

# 5) 成功态逐字对比（见 §3 的取法与比较命令：取 SNAP-BEGIN 行，$a -ceq $b ⇒ True）

# 6) 自清（顺序重要：先摘 junction 链接，再删树 —— 否则可能删到仓库的 node_modules）
Stop-Process -Id <我记录的两个 pid> -Force          # 只用记录过的 PID，绝不按名字/端口
cmd /c rmdir "$env:TEMP\t102-before\panel\node_modules"
Remove-Item -Recurse -Force "$env:TEMP\t102-before","$env:TEMP\t102-root-after","$env:TEMP\t102-root-before"
Get-ChildItem panel\test-results -Directory | Where-Object { $_.LastWriteTime -ge <本次开始时刻> } | Remove-Item -Recurse -Force
```

**全套的 3 skipped 各是谁、为什么**（读的是 spec 自己的守卫，不是猜测）：
* `chat.spec.ts:19 test.skip(!mock, "needs an e2e mock agent (chat with real runtimes costs money)")` —— 我的空 root 里 `mock` 是 **disabled**（默认种子）。
* `judge.spec.ts:27 test.skip(!judge || members.length < 2, "needs the e2e mock agents (2 members + judge)")` —— 同上。
* `recall.spec.ts:32 test.skip(true, "recall matched no memories in this database")` —— 空 root 里没有记忆。
⇒ 三条 skipped 都是**一次性空 root 的环境差异**，与 t84 无关。

---

## 8 纪律回执（含一条**我必须自陈的越界**）

* **只改**：`panel/e2e/failure-visibility.spec.ts`（新增，`panel/e2e/**` 在 inScope 内）+ 本报告。**没有**改 `panel/src/**`（`git status --porcelain -- panel` = 仅 `?? panel/e2e/failure-visibility.spec.ts`）。发现的两条缺陷**交回 finding**（§5），**没有**自己动手修。
* **不写活库、不动真 daemon**：全程只用一次性 root；`pid 79984` 未启停（收尾 `alive = True`）、`8787` 未动（收尾 `listening = True`）；我起的两个 daemon **只按记录到的 PID**（84000、91224）停掉，收尾两个端口都**不再 listen**、两个 PID 都**不存在**。
* **`my_temp_leftovers` 读数（清理后）**：`%TEMP%\t102*` 条目 **0**、我创建的 `panel/test-results/run-*` 目录 **0**、我拥有的 `ruagent` 进程 **0**。仓库 `panel/node_modules/.bin` 完好（删树前先摘 junction，已验证）。
* ⚠️ **一条越界，必须自陈**：我的清理命令用了过宽的 `Get-ChildItem $env:TEMP -Filter 't102*'`，因此**连带删除了三个不是我的、先前就存在的临时条目**：`%TEMP%\t102a\`（目录，2026-09-26 02:11）、`%TEMP%\t102z-apply.py`、`%TEMP%\t102z.json`（2026-09-24 00:18）。它们都在 `%TEMP%`（不在仓库、不是活库数据），但**是同伴的临时产物，我没有先看就删了**——这是我的操作错误（"临时物自清"应只清**自己**的），已按纪律记录在此并同步给 captain。若某位成员需要它们的内容，我无法恢复（未读取过）。
* **负控不是"改共享源码"式的**：本单的负控是**构建两份 dist 各起一个一次性 daemon**，**没有**在共享工作树里临时改任何源码 ⇒ 不会让别人的 `test --workspace` 变红（对照 AGENTS.md 的 t93/t81 条款）。
* **我是验证者，本单不涉自评**：t84 的实现我没有评审也没有修改；本报告只给读数、判据与两条 finding。
