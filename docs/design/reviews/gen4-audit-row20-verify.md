# t155 独立验证：审计行 20 的「恢复」修复（被验对象 = `t151`，作者 verify）

> **性质**：verification，验证者 = verify2（**不是**被验对象的作者）。证据 = **我自己起的两个 daemon + 我自己构建的两个 dist + 我自己写的探针**；作者的结论只作对照，不作依据。
> **结论**：**`恢复 ✗` → `恢复 ✓` 成立**（两档 dark/light），而且这不是「把提示藏起来」——**点一次重试后内容真的回来了**（开关 0→13、输入 0→11、正文哈希与健康态**逐字相同**）。根因「`#settings` 有两个各带重试的独立 loader」**从字节与我自己的探针两头都成立**。**未削弱**：`criterion` 两臂逐字相同（430 字符）、`limits` 相同、工具 blob 四个 revision 全同。**但见 §4：作者报告 §9 的配方（`git worktree @ HEAD`）确实会给出两个修后臂** —— 我按 captain 的钉法把修前臂钉在 **`11123a4^`**，并加了**内容级判据**（该字节里 `epoch`/`reload`/`onReload` **各 0 次**）。
> **一条自报的仪器失误**（§4.3）：我第一次用 `cmd /c "git show 11123a4^:…"` 取修前字节时，`cmd` 把 `^` 当转义符吃掉 ⇒ **实际取到的是修复后的 blob**，于是「修前文件 == 当前文件、`epoch` 出现 13 次」这条**假读数**差点让我写出「修复不在 HEAD 里」这种错误结论。改道的判据（blob sha）当场否掉了它。

---

## 0 判定表（按契约判据顺序）

| # | 判据 | 结论 | 读在哪 |
| --- | --- | --- | --- |
| 1 | 两臂各自独立复跑，读 `checks[20]`，给两档 `verdict` + 判定原文 | **passed** | §3：修前 **fail**（两档 `恢复 ✗`）/ 修后 **pass**（两档 `恢复 ✓`） |
| 2 | 不照抄作者配方 ⇒ 修前臂钉在 `11123a4^` + 内容级判据 | **passed** | §4：blob `04fda41…` + `epoch`/`reload`/`onReload` = 0；**并标出 §9 的坑** |
| 3 | 独立复核「两个独立 loader」根因 + 自己探针确认 2→1 | **passed** | §5：`onRetry={load}`×2（`:88`/`:409`）+ 我的探针 2 → **1**（幸存者 = 能力卡） |
| 4 | 判「真重取 + 真清错误态」（不是藏提示），点第一个/第二个都恢复，内容逐项相同；①③ 未牺牲 | **passed** | §6：两次点击都 0 错误，`bodyHash` == 健康基线；注入期 2 块 alert、空态 0、外壳在 |
| 5 | 判据未削弱：`criterion` 逐字、`limits`、工具哈希 | **passed** | §7：len 430 且 sha256(16) 两臂**相同**；工具 blob 四 revision 全同 |
| 6 | 门禁 `npm --prefix panel run build` exit 0（我自己跑，给结尾原文） | **passed** | §8：exit 0 + 结尾原文；**并在两份独立构建的 dist 上都复跑行 20** |
| 7 | 写出没有覆盖什么 | **passed** | §10 |

---

## 1 两臂的字节（先定身份，再看读数）

| 臂 | 来源 | `panel/src/views/Settings.tsx` | 该文件里的页面作用域重试 | 构建出的 dist |
| --- | --- | --- | --- | --- |
| **修前** | `git worktree add --detach %TEMP%\t155-before '11123a4^'` ⇒ HEAD = **`d20df4c`** | blob **`04fda41604255259ed6dea5726016ffa29997f11`**（33,290 B） | **`epoch`=0 · `reload`=0 · `onReload`=0 · `useCallback`=0** ✅ | `%TEMP%\t155-before\panel\dist`（`npm --prefix … run build` **exit 0**，4.54s；**55 assets**；构建产物里 `epoch` 出现 **0** 次） |
| **修后** | 主工作树 = 最终字节（`git hash-object` = 与 `HEAD` 相同） | blob **`4eae5def1389bfa69587a392b93da721b556fd63`**（34,573 B） | `epoch`=13 · `reload`=3 · `onReload`=8 | `panel\dist`（**exit 0**，4.53s；55 assets；产物里 `reload` 出现 6/1/1/13 次） |

* `11123a4`（"fix(panel): the settings retry recovers the whole page, not half of it"）**是 HEAD 的祖先**（`git merge-base --is-ancestor` = true），`git diff 11123a4..HEAD -- panel/src/views/Settings.tsx` **为空** ⇒ **修复确实在最终字节里**，`11123a4^` 就是它之前的那一版。
* **工具（我自己核的，不是采信）**：`panel/tools/design-audit.mjs` 的 blob 在 `11123a4^` / `11123a4` / `HEAD` / 工作树 **四个位置逐字相同** = `ed25988ac13d9c33298b186c8fef212ed0146be6`，且 `git log 11123a4^..HEAD -- panel/tools/design-audit.mjs` 为空 ⇒ **两臂用的是同一把尺子** ✅。
* 两臂的 dist 都不是「共享物」：修前在我自己的 worktree 里构建（`node_modules` 用 junction 借主树，**没有 `npm ci`**），修后是主树的 `panel/dist`（本单允许写）。

## 2 仪器（每臂一个我自己的 daemon）

* **daemon 二进制**：`%TEMP%\ruagent-team-target\debug\ruagent.exe`（mtime **10-04 00:41:51**）**拷贝**到 `%TEMP%\t155\bin\ruagent.exe`，sha256(24) **`C4C59173BB6AD05C2CDAE6D1`**。**被验对象是面板 dist，不是这个二进制**；用拷贝而不是原地跑，是为了**不占共享 target 里的文件**，也让它对同伴的在途源码编辑**免疫**（见 §11 的 t154 窗口说明）。
* **端口/根/面板**：修前 `127.0.0.1:8861` + `%TEMP%\t155\root-before` + `RUAGENT_PANEL_DIST=%TEMP%\t155-before\panel\dist`；修后 `8862` + `root-after` + 主树 `panel/dist`；§8 的复跑用 `8863` + `root-after2`。三个 PID（`1768`/`6320`/`32576`/`11416`/`29924`/`32044`）全部由我记录并**按 PID 停掉**，没有按名字/端口批量杀。
* `NO_PROXY=127.0.0.1,localhost,::1`；`RUAGENT_EMBEDDER=hash`；`RUAGENT_HOME`=临时根（`~/.ruagent` 全程未碰，活守护 **8787 = pid 14944** 未触碰）。
* 审计调用（**与作者同一把工具、同一个形状**）：`node panel/tools/design-audit.mjs --check --routes=settings --modes=dark,light --tabs=40 --no-shots --no-pixels --allow-not-measured --base-url=http://127.0.0.1:<port> --out=%TEMP%\t155\out-<arm>`（`--out` 在仓外）。

## 3 判据 1：两臂的 `checks[20]`（逐字）

**修前臂**（`metrics.json`）：`verdict = fail`、`limits = {"wantAlert":true,"wantRetry":true}`、`criterion` 长度 430。两档 evidence **逐字相同**：
```
display = 失败态 可见 ✓ · 重试 可点 ✓(49×32) · 恢复 ✗ ｜ 注入 3/3 端点 · 外壳 ✓ · 空态 无
note    = 注入端点（该路由自己的，已排除外壳 /permissions）：/api/v1/distill, /api/v1/agents, /api/v1/capabilities ·
          失败态呈现：…「无法读取蒸馏策略 读不到蒸馏配置：daemon 不可达…」· …「无法读取能力配置 读不到 GET /api/v1/capabilities…」·
          重试控件 …「重 试」真实点击成功 · 外壳完好（侧栏 + main 在位，body 293 字符）—— 不是白屏
```
**修后臂**：`verdict = pass`、`limits` 同上、`criterion` 长度 430；两档：
```
display = 失败态 可见 ✓ · 重试 可点 ✓(49×32) · 恢复 ✓ ｜ 注入 3/3 端点 · 外壳 ✓ · 空态 无
note    = （与修前臂同一段文字；差异只在 恢复）
```
`report.md` 里行 20 的两行（工具自己的判定行 + MASTER §12 的判据段）两臂逐字比对：**判定行** = `| 20 | FAIL | … 恢复 ✗ …`（修前）/ `| 20 | PASS | error 与 empty 可区分（失败态注入） | …`（修后）；**判据段**（"每路由在 API 失败时呈现 role=alert + 重试按钮。**注入只打该路由自己的数据端点**…① 失败态可见 ② 可恢复 ③ 不得渲染成空态或成功态…外壳必须仍在…"）**两臂完全相同** ✅。
* 两臂 `--check` 都是 **exit 1**，但**与行 20 无关**（修前行 20 红、修后不是）：行 **12**（内联 style 79 > ≤50）、**13**（`settings/dark` 暗对比度 6）、**18**（内容区 <32px 命中 13 > ≤10）在**两臂都 FAIL 且读数逐字相同** ⇒ 修复的作用被**干净地隔离在行 20**（§9）。

## 4 判据 2：修前臂钉在哪（不照抄 §9）

### 4.1 为什么不照抄
作者报告 §9 写的是 `git worktree add --detach %TEMP%\t151-prefix HEAD`，而**修复已经在 `11123a4` 里入库**（§1 的 blob 证明）⇒ 照抄 `HEAD` 会得到一个**修后臂**，「红 → 绿」退化成「绿 → 绿」。我钉在 **`11123a4^`**（= `d20df4c`）。

### 4.2 两条判据一起用（git 名字会漂，内容不会）
* **基于 git 名字**：worktree 的 `HEAD` = `d20df4c`，且 `git rev-parse '11123a4^'` 与它一致。
* **基于内容（不随提交漂）**：该 worktree 里 `Settings.tsx` 的 **blob = `04fda416…`**，并且该文件里 **`epoch` = 0 次、`reload` = 0 次、`onReload` = 0 次、`useCallback` = 0 次**（页面作用域重试的特征串**一个都不在**）—— 这正是 captain 建议的那条内容级判据，**成立** ✅；构建出的**修前 dist** 里 `epoch` 也是 **0** 次（连产物都对得上）。

### 4.3 我自己的仪器失误（先确认读数属于谁）
第一次取「修前」字节我用的是 `cmd /c "git show 11123a4^:panel/src/views/Settings.tsx > …"`：**`cmd` 把 `^` 当转义符吃掉**，于是实际执行的是 `git show 11123a4:…` = **修复后**的 blob ⇒ 我一度读到「修前文件 = 当前文件、`epoch`=13、字节数相同」，并据此差点写下「**修复不在 HEAD 里 / 或已被回滚**」。**否掉它的不是直觉，是另一条判据**：`git diff 11123a4^ HEAD -- panel/src/views/Settings.tsx` 明明有 `+23/-9`，而 `git rev-parse '11123a4^:…'`（PowerShell 里 `^` 是字面量）给出的是**另一个 blob**。改道后一切自洽。⇒ 教训与 C40 同族：**先证明读数属于哪个对象，再把它当读数**（本报告里所有「修前」字节都改用 blob sha + `git cat-file blob <sha>` 取，`cmd` 行里不再出现 `^`）。

## 5 判据 3：根因（两个独立 loader）——字节 + 我自己的探针

**从字节读**（修前 = `11123a4^` 的 `Settings.tsx`）：
* `function DistillSettings()` `:52` —— 自己的 `err` `:55`、自己的 `load` `:63`（`:68` 清错、`:70` `catch` 落错）、**自己的重试 `onRetry={load}` `:88`**；
* `function CapabilitiesSettings()` `:366` —— 自己的 `err` `:369`、自己的 `load` `:388`（`:393` 清错、`:395` 落错）、**自己的重试 `onRetry={load}` `:409`**。
⇒ **两个互不知情的 loader，各自渲染自己的错误卡**。审计注入的 3 个端点里 `/api/v1/distill`+`/api/v1/agents` 落在第一个、`/api/v1/capabilities` 落在第二个 ⇒ **两块 `role=alert` 同屏**（工具与我的探针都读到 **2**）。
**工具只点第一个重试**：`panel/tools/design-audit.mjs:206` 是 `page.locator("[data-audit-retry]").first().click(...)`，而 `[data-audit-retry]` 是它在 `:183` 给**第一个**匹配到的重试按钮打的标记（`:177` 的 `.find(...)`）⇒ **审计的真实点击只落在第一块卡上**。这是从**工具字节**读出来的，不是采信作者。

**我自己的探针**（`%TEMP%\t155\probe.mjs`，我写的；`page.route("**/api/**")` 只对那 3 个端点 `fulfill(500)`，然后**真点击**）——修前臂：
```
injectedOnLoad = 3        # 注入命中 3/3 端点
注入后        ：alertCount = 2  retryCount = 2  switches = 0  inputs = 0  bodyLen = 293  empty = 0  外壳(sideNav/main) = 1/1
点【第一个】重试（点击前已 unroute）= alertCount = 1  ⇒ 幸存者 = 「无法读取能力配置 读不到 GET /api/v1/capa…」
                 此时 switches = 2  inputs = 2  bodyLen = 408   ⇒ 页面只恢复了一半
再点【幸存的那个】  = alertCount = 0  switches = 13  inputs = 11  bodyLen = 2566  bodyHash == 健康基线
```
⇒ **「第一块恢复、第二块留在屏幕上」在我自己的读数里成立**（`2 → 1`），且验明**幸存的是能力卡**。修前臂的 `恢复 ✗` 因此不是工具的怪癖，而是**页面上真有一个修不掉的错误卡**（要点第二次）。

## 6 判据 4：修后是「真重取 + 真清错误态」，不是「藏提示」

修后臂（同一探针）。**健康基线**（无注入）：`alertCount=0 · switches=13 · inputs=11 · selects=1 · sideNav=1 · main=1 · bodyLen=2566 · bodyHash=7d0bf42f02541352 · empty=0`。

| 步骤 | alertCount | 内容（switches / inputs / bodyLen / bodyHash） | 与健康基线 |
| --- | --- | --- | --- |
| 注入后 | **2**（逐字同修前：`无法读取蒸馏策略…` + `无法读取能力配置…`） | 0 / 0 / 293 / `bd4738416188a4eb` | 差异 = 错误态（③：**空态 0**） |
| 点**第一个**重试 | **0**（627 ms） | **13 / 11 / 2566 / `7d0bf42f02541352`** | **逐项相同，bodyHash 相同** ✅ |
| 再注入一次 | 2 | 0 / 0 / 293 | — |
| 点**第二个**重试（`nth(1)`，共 2 个按钮） | **0**（620 ms） | **13 / 11 / 2566 / `7d0bf42f02541352`** | **逐项相同** ✅ |

* **「不是藏提示」的直接证据**：恢复不是「错误卡消失、内容仍空」——`switches 0→13`、`inputs 0→11`、`bodyLen 293→2566`、**`bodyHash` 与健康基线逐字相同** ⇒ 数据**真的重新取回来了**；且注入期间 `emptyShown = 0`（**③ 没有被牺牲**：不是空态/成功态顶替，是显式错误卡）。
* **① 没有被牺牲**：注入期间 **2 块可见 `role=alert`**（与修前逐字相同的两条文案），重试按钮 2 个、命中区 `49×32`（≥24×24）；外壳 `sideNav=1 / main=1`、`bodyLen=293`（不是白屏）。
* **修法为什么能让「点一个」恢复整页**（字节）：`Settings()` 加 `const [epoch, setEpoch] = useState(0)`（`:44`）+ `const reload = useCallback(() => setEpoch(e => e+1), [])`（`:45`），两张卡收 `{epoch, onReload}`（`:54/:55`），各自的**重试改指页面作用域**的 `onRetry={onReload}`（`:101`/`:423`），而两个 loader 的 `useEffect(..., [epoch])`（`:91`/`:413`）在 `epoch` 变化时**各自重跑** ⇒ 一次点击 = 全部三个请求重发 = 两块错误都由**数据到位**清掉。错误态本身没有被删掉（`setErr`/`Alert` 仍在，注入时仍渲染 2 块）。
* **一处如实记账**：修前臂「点第一个重试」的那次，我的探针 `settle(0)` 有界等待**等满了 15,040 ms 的界**（因为它在等一个永远不会到的 0）—— 那是**我的探针的等待上界**，不是产品的 15s 延迟；修后同一动作 **627 ms** 收敛。探针随后正确读到 `1` 并转入「点幸存者」分支。

## 7 判据 5：判据没有被削弱

| 项 | 修前臂 | 修后臂 | 判定 |
| --- | --- | --- | --- |
| `checks[20].criterion` | 430 字符 | 430 字符 | **逐字相同**（我对该字符串取 SHA256 前 16 位：两臂均 **`1F9ABFDE7AE4CA8D`**） |
| `checks[20].limits` | `{"wantAlert":true,"wantRetry":true}` | 同左 | 相同（作者的读数也一致） |
| `panel/tools/design-audit.mjs` | blob `ed25988a…` | blob `ed25988a…` | **两臂同一把尺子**（作者说「未改工具」→ 我核的是 blob，四个 revision 全同） |
* 这三条一起才说明「红 → 绿」是**被验对象**的变化，而不是「判据被放松」或「换了量具」。

## 8 判据 6：门禁（我自己跑）

```
> npm --prefix panel run build
   dist-staging/assets/vendor-DnTfy3f4.js   1,044.41 kB │ gzip: 333.39 kB
   (!) Some chunks are larger than 500 kB after minification. …
   ✓ built in 5.11s
   build-panel: dist updated (55 assets, index.html swapped by rename)
GATE EXIT=0
```
* 用的是契约的命令（内含 `e2e/i18n-check.mjs` + `tsc -b` + `tsc -p e2e/tsconfig.json --noEmit` + vite，见 `panel/scripts/build-panel.mjs`；**没有**用裸 `npx playwright test`）。
* **一处必须披露的观察**：门禁这次重建出的 `panel/dist/index.html` 与我在 §3 审计时用的那份**不同**（`0F5FA5055C57C2C4` → `80B5ACB61A81C509`），而**源码没有变**（`panel/` 在 `a8d056e..9e6c7af` 零变化、`Settings.tsx` blob 仍是 `4eae5de…`；HEAD 只多了 3 个 `docs/design/reviews/**` 提交）⇒ 这是**构建产物非确定**（Vite/rollup 层面），不是源码移动。**因此我做了第二次独立审计**（新端口 8863、新 root、`RUAGENT_PANEL_DIST` 指向**门禁刚构建的那份 dist**）：`verdict = pass`、两档 `恢复 ✓`、`criterion` 长度 430 ⇒ **修后读数在两份独立构建的 dist 上稳定**（第一份构建于 17:34:57，第二份即门禁那次；各一次审计）✅。
* 这也顺带说明：**报告「报告哈希/产物哈希」时必须带时刻与来源**，否则同一份源码会给出两个「身份」。

## 9 两臂的其它行：修复的效果被隔离在行 20

两臂 `--check` 都 **exit 1**，但失败行是**同一组**且读数**逐字相同**：行 **12**（内联 style 79 > ≤50）、行 **13**（`settings/dark` 暗对比度 6）、行 **18**（内容区 <32px 命中 13 > ≤10）。⇒ 行 20 是**唯一**在两臂间移动的行（FAIL → PASS）✅（这也是本单结论不受那 3 行影响的依据：它们与本修复无关，且两臂一致）。

## 10 我没有覆盖什么（判据 7，逐条给理由）

1. **其余路由的行 20**：我只跑 `--routes=settings`（本单被验对象就是 `#settings`）。全站 13 路由的行 20 未跑 ⇒ 别的路由是否也有「多个 loader、点一个恢复不了」的形状**未知**。
2. **真实网络故障（非注入）**：注入是 `page.route` 层面的 500（`{"error":"audit-injected"}`），**没有**真的拔掉/超时/半开连接、也没有**延迟**响应；daemon 侧真实 5xx/超时/断连下的行为未测。
3. **恢复后的数据正确性**：我只测到「错误消失 + 内容与健康基线**逐字相同**」。**没有**比对 API 返回的**语义值**（例如 `[capabilities]` 表里的 weight、策略字段的数值）与预期；「内容与基线相同」是**同一过程两次渲染一致**，不等于「值正确」。
4. **light 之外的档位**：只有 `--modes=dark,light`；其它模式/主题/窄屏档位未测。
5. **第二个重试是「我独立测的」还是作者的**：**是我独立测的** —— `point of proof` 是 §6 表格里「再注入一次 → 点 `nth(1)` → 0 + 内容相同」这一步，来自我写的 `probe.mjs`（作者的 `t151-probe*.mjs` 我**没有运行**，只在 §4 读过 `t151-audit*.log` 之类**不是**他探针的东西）。**但**：作者自己在报告里写的是「点第一个重试」，我没看到他测过第二个 ⇒ 这一条**只由我覆盖**。
6. **工具自身的改动**未做：我没有改 `panel/tools/design-audit.mjs`（只读 + 哈希比对）；也没有把行 20 的判据「加强」到能区分两块卡（例如要求**两块**都能点）—— 那属于判据变更，不在本单。
7. **未跑 pixel/shot 行**：两轮都带 `--no-shots --no-pixels --allow-not-measured`，`not_measured` 行不在本单。
8. **没有在共享树上跑任何东西**：本单所有读数的 `--out` 都在仓外；`git status` 未因我而多一行（§11）。

## 11 纪律回执

* **写入集合**：只有本报告 `docs/design/reviews/gen4-audit-row20-verify.md`。`panel/src/**`、`panel/tools/**`、`panel/package.json`、`crates/**`、`.github/**`、`scripts/**` **零改动**（我只**执行** `scripts/build-panel.mjs`（经 `npm run build`）与 `panel/tools/design-audit.mjs`，没有写它们）；`panel/dist` 被 `npm run build` 重写（**本单允许写**）。**一处写入集合之外**：按 captain 在 t150 评审后的**直接要求**，我给 **t149 的报告**补了一段「交接后编辑披露」（§0 末），已在 t149 报告里标明**哪个读数属于哪个 revision**，可随时回退。
* **junction 清理按 C39**：`cmd /c rmdir %TEMP%\t155-before\panel\node_modules`（**只删链接，不跟进去删主树**）→ 再 `git worktree remove --force %TEMP%\t155-before` → 最后按**具体路径**删 `%TEMP%\t155` 与 `%TEMP%\t155-*`（**列清单在前，不用 glob**；`%TEMP%\t151*`、`%TEMP%\t151z*` 等**不是我的，一个没碰**）。
* **进程**：我起的 6 个 daemon（`1768`/`6320`/`32576`/`11416`/`29924`/`32044`）**按记录的 PID**停止，端口 8861/8862/8863 现在都无监听；我的探针 `browser.close()` 正常退出（§11 收尾时按**我自己临时路径**复检，命中的只有**当前这条工具调用自己的** `node`/`pwsh` 进程）。**没有按名字/端口批量杀**。活守护 **8787 = pid 14944** 全程只读未触碰。
* **与 t154 宣告窗口的关系**：graph 的窗口动的是 `crates/daemon/src/distill.rs` 与 `registry.rs`，而本单**不编译 Rust、不跑 daemon 测试**，且我的 daemon 是**冻结的二进制拷贝**（`C4C59173…`，取自 00:41:51 的构建产物）⇒ **无论窗口何时开闭，源码变异在物理上都不可能改变本单任何读数**（这也是「形状相同 ≠ 仪器共享」的正例：我的仪器是「面板 dist + 冻结二进制」，与窗口改的测试仪器不共享任何字节）。窗口的**准确起止**我没有记录，因为我并不需要依赖它 —— 这也正是上面那句「物理上不可能」的意思。
* **不采信摘要**：根因、工具行为、`criterion` 长度/文本、`limits`、工具哈希**全部由我自己的字节/JSON 读出**；作者报告只用来对照（并已指出其 §9 配方会误导，见 §4.1）。
