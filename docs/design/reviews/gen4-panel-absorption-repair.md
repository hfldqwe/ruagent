# 面板 MCP 健康行的 `?? 0` 吸收修复（t128 = t123 §3 的排序第 13 项）

- 任务：t128（kind=implementation）；attempt `5569087f-7311-4d08-9a00-2da2ac691636`
- 来源：`docs/design/reviews/gen4-protocol-drift-sweep.md` §3（面板侧残留）—— `Agents.tsx` 的 MCP 健康行把「**键缺失 / `null`**」用 `?? 0` 吸收成「**有 0 个**」
- inScope（**只改了这三个**）：`panel/src/views/Agents.tsx`、`panel/src/i18n/agents.ts`、`panel/e2e/agents-health-third-state.spec.ts`（新建）
- 未碰：`crates/**`、`.github/**`、`scripts/**`、`panel/src/views/Runtimes.tsx`、`panel/src/api.ts`（后两个是只读样板）
- **共享树零变异负控**：本单的变异只发生在 `%TEMP%` 下的**隔离副本**里，共享树**没有负控窗口**（§4）
- 早先那版 `docs/design/reviews/gen4-panel-absorption-repair.md` 不存在，本文件是这一单的交付物

## 0 一句话

`在线 · 0` 里那个 `0` 以前同时代表**两件相反的事**：「探测成功且真的 0 个工具」与「守护进程没告诉我们」。改后第二件事有了自己的字：**`在线 · 未告知工具数 · 重试`**（一个真的会重取的按钮），而第一件事与「已下线」**一个字节都没变**（§2 有跨两个构建的成对 DOM 读数）。

## 1 改动（逐字 diff）

```diff
@@ -350,11 +350,35 @@ export function Agents() {
                   {s.health ? (
                     s.health.state === "ok" ? (
-                      <Tooltip
-                        title={`${s.health.tools ?? 0} ${t("mcp.tools")} · ${s.health.latency_ms}ms`}
-                      >
-                        <span className="tag ok">
-                          {t("mcp.up")} · {s.health.tools ?? 0}
-                        </span>
-                      </Tooltip>
+                      // t128 (t123 §3 F1 family): `tools == null` is the THIRD
+                      // state, and it is not a zero.
+                      // …（理由注释：`number | null` + 键可缺失；`?? 0` 把两者塌成自信的 0）
+                      s.health.tools == null ? (
+                        <Tooltip title={t("agents.mcp.toolsUnknown.hint")}>
+                          <button type="button" className="tag" onClick={load}>
+                            {t("mcp.up")} · {t("agents.mcp.toolsUnknown")}
+                          </button>
+                        </Tooltip>
+                      ) : (
+                        <Tooltip
+                          title={`${s.health.tools} ${t("mcp.tools")} · ${s.health.latency_ms}ms`}
+                        >
+                          <span className="tag ok">
+                            {t("mcp.up")} · {s.health.tools}
+                          </span>
+                        </Tooltip>
+                      )
                     ) : (
                       // "down" is a state, not an error: .tag.warn, never .tag.err
```

**既有渲染的改写量 = 0（可核）**：`ok` 支里唯一的文本变化是两处 `?? 0` 被去掉（`${s.health.tools ?? 0}` → `${s.health.tools}`），**数字存在时渲染完全相同**（§2 成对读数）；`down` 支在 diff 里**没有一行 `+`/`-`**（它只是成了新 `if` 的 `else` 之后的上下文）。

**为什么这样选（沿用仓里已成形的第三态，不发明新机制）**：

| 依据 | 读数 |
| --- | --- |
| 失败/未知不得渲染成「没有数据」 | `Runtimes.tsx:213-218`：`probeFailed` 的汇总不把没答的探针按 0 相加，而是显示 `runtimes.probeFailed`（`"探测失败 · 重试"`） |
| 「缺」与「0」是两个判据 | `panel/src/api.ts:1341-1346`（`knowledgeSearchLegs` 注释）：漏腿是 `null`，**never 0**，明令禁止 `?? 0` |
| 标签承诺重试就必须**是**重试 | `Runtimes.tsx:305-310`：`probeFailed` 的 chip 是 `<button type="button" className="tag" onClick={…}>`（**中性 `.tag`**，不是 `.ok` 也不是 `.warn`） |

⇒ 新分支照抄这三条：**中性 `.tag` + 真 `<button onClick={load}>`**，文案与 `runtimes.probeFailed` 同词表（状态短语 + `· 重试`）：zh `未告知工具数 · 重试` / en `tools not reported · retry`（新增 key 落在**本视图自己的域文件** `panel/src/i18n/agents.ts`，zh+en 同时加，见 §7 的 diff）。
`==` 而非 `===` 是**故意的**：类型是 `number | null`（`api.ts:261`），但**键被删掉**时运行时是 `undefined`；`?? 0` 当年**两者都吸收**，所以第三态必须两者都接。

## 2 三态 DOM 读数（原文），以及跨两个构建的成对对照

**改后**（本树构建的 `panel/dist`，见 §5 字节归属）：

| 注入的 `health` | DOM 读数（`textContent`，逐字） | 状态类 / 角色 |
| --- | --- | --- |
| `{state:"ok", tools:7, latency_ms:33}` | **`在线 · 7`** | `.tag.ok`（绿，span） |
| `{state:"ok", tools:null, latency_ms:33}` | **`在线 · 未告知工具数 · 重试`** | 中性 `.tag`，**`role=button`**（可点重取） |
| `{state:"ok", latency_ms:33}`（**键被删**） | **`在线 · 未告知工具数 · 重试`** | 同上 |
| `{state:"down", error:"e2e forced down"}` | **`已下线`** | `.tag.warn`（span，**不是** button） |

第三态那一格里**没有任何数字**：断言为 `not.toMatch(/\d/)`（`0 个工具` 是这次要消灭的那句话），另有 `not.toMatch(/0 (个工具|tools)/)`。

**改前（负控构建）成对读数** —— 同一 spec、同一注入、另一份 dist：

| 注入的 `health` | 改前 DOM 读数 | 改后 DOM 读数 | 结论 |
| --- | --- | --- | --- |
| `tools:7` | **`在线 · 7`** | **`在线 · 7`** | **逐字相同**（既有渲染未变） |
| `tools:null` | **`在线 · 0`** | **`在线 · 未告知工具数 · 重试`** | 这就是那个「自信的零」 |
| 键被删 | **`在线 · 0`** | **`在线 · 未告知工具数 · 重试`** | 与 `null` 同罪同修 |
| `state:"down"` | **`已下线`** | **`已下线`** | **逐字相同**（down 路径未被改写） |

⇒ 「**只有 `tools` 缺失会被第三态接住**」不是推论，是上表第 1/4 行的**实测**（两个构建给出同一句话、同一个类）。

## 3 能红的注入式断言（`panel/e2e/agents-health-third-state.spec.ts`）

- **注入点**：`page.route("**/api/v1/mcp", …)` 直接回一份合成 `McpRegistry`（含一个服务器 `e2e-health`）。**不写守护进程配置 ⇒ 不调 `writeAccess()`**；整份 spec 只读。
- **5 条读数**：(1) `tools=7` 成功控制（数字仍是数字）· (2) `tools=null` 第三态 + 无数字 + `role=button` · (2b) **键被删**同样进第三态 · (2c) **点它真的会重取**（`page.route` 命中数 `before=1 → after=2`）· (3) `down` 逐字不变 + 仍是 `.tag.warn` + **不是** button。
- **(2c) 为什么重要**：标签写着「重试」，就必须真的是重试（`Runtimes.tsx:305-310` 的同一论证）。它**也**被同一 `page.route` 拦截 ⇒ 这条读数**不依赖守护进程自己的答案**。
- **为什么能红**（不靠同义反复）：第三态那句话在**改前的构建里不存在**（§4 的负控实测），所以「断言它出现」在改前必然失败；断言里还钉了「这一格没有数字」，因此「把行删掉」或「改成别的字」也过不了。成功控制 (1)/(3) 保证「把这条读数删掉」不算通过。

## 4 负控：隔离副本里的改前构建（**共享树零变异**）

**做法（可复现）**：`%TEMP%\ruagent-t128\prefix\panel` = `robocopy panel /E /XD node_modules dist dist-staging` 的**副本**（`node_modules` 用 junction 指向共享的，只读使用）；把本单改的两个源文件换回 `git show HEAD:` 的**改前字节**；在副本里 `npm run build`；用**第二个**临时守护进程（8791、自己的 root、`RUAGENT_PANEL_DIST=<副本>\panel\dist`）跑**同一份 spec**。⇒ **共享源文件一行未动**（`git status -- panel` 只有本单那三个文件），所以**没有需要宣告的窗口**。

**副本确实是改前的（自证）**：副本 `Agents.tsx` 含旧形状 `tools ?? 0` = **True**、含 `toolsUnknown` = **False**；副本 dist 里搜不到新文案 `未告知工具数` = **False**；副本 bundle `index-CPFTiOJ6.js`（00:27:37）≠ 改后 `index-DC4GKd0-.js`（00:18:44）⇒ 两臂确实是不同的字节。

**负控读数（改前构建）**：`CONTROL_E2E_EXIT=1`，**3 failed / 2 passed**

```
1) (2) tools=null is the THIRD state: no confident zero
   Error: expect(received).toMatch(expected)
   Expected pattern: /未告知工具数|tools not reported/
   Received string:  "在线 · 0"
2) (2b) an ABSENT tools key is the same third state as null
   Expected pattern: /未告知工具数|tools not reported/
   Received string:  "在线 · 0"
3) (2c) the third state's retry is a REAL re-read (route hit count rises)
   Error: expect(received).toBeGreaterThan(expected)
   Expected: > 1
   Received:   1
```

⇒ **红的是这次改动自己的三条读数**（第三态文案 / 无数字 / 重试），**绿的是两条「既有渲染」控制**（`在线 · 7`、`已下线`）。改后同一条命令：**5 passed, exit 0**（§2 的读数表）。

## 5 字节归属（C35：**逐条**写清每个读数属于哪份字节）

| 读数 | 属于哪份字节 | 时间/身份 |
| --- | --- | --- |
| 被审面板源 | 本树 `panel/src/views/Agents.tsx`（`git diff` 见 §1）、`panel/src/i18n/agents.ts` | 2026-10-04 00:2x |
| 改后 `panel/dist` | 本树构建产物：`index-DC4GKd0-.js` | mtime **2026-10-04 00:18:44**；新文案 `未告知工具数` 就在这个 bundle 里（全 dist 搜索唯一命中） |
| 改前 `panel/dist`（负控） | 隔离副本产物：`index-CPFTiOJ6.js` | mtime **2026-10-04 00:27:37**；搜不到新文案 |
| 起面板的守护进程（两臂各一） | `C:\Users\19410\AppData\Local\Temp\ruagent-team-target\debug\ruagent.exe` | mtime **2026-09-29 09:28:43**，size 325,523,456 |
| 端口 / 临时 root | 改后臂 `127.0.0.1:8899` + `%TEMP%\ruagent-t128\root`（listener pid 31020）；负控臂 `127.0.0.1:8791` + `%TEMP%\ruagent-t128\root-control`（listener pid 34020） | 起于 2026-10-04 00:24:44 / 00:27:42 |
| **C35 年份探针**（跑在**两个**临时守护进程上） | `GET /api/v1/graph/entity/987654321` ⇒ **`{"name":null,"kind":null,"aliases":[],"facts":[]}`** | 两臂同结果 ⇒ 这份二进制**不早于**该字段的变更（**不是**「新字段读成不存在」的那类陈旧二进制） |
| 活守护进程（**只引用、未触碰**） | 按 **C32 的写法**：8787 的**端口归属 + health** ⇒ `OwningProcess=14944`、`health: ok`，读于 **2026-10-04 00:14:06**（不引用被记住的 pid） | 未起、未停、未写它的库 |

**e2e 是怎么拿到面板的**：临时守护进程以 `RUAGENT_PANEL_DIST` 指向对应的 `panel/dist` 提供静态面板；spec 打开 `#agents` 后，**所有 `GET /api/v1/mcp` 都被 `page.route` 拦截**并回合成响应。
**因此本单读数与守护进程二进制的年份无关**（显式声明 + 理由）：三个状态、两条控制、重试计数**全部**来自注入响应；连 (2c) 的重取也被同一 route 拦截。守护进程在这三样东西里只是**静态文件服务器**（面板字节由 §5 第 2/3 行钉住）。**唯一**会依赖它的是「第三态按钮在真实守护进程下重取的答案」——那条**没有**被本单读数使用（见 §6）。

## 6 未覆盖什么（点名 + 原因 + 需要什么才能测）

1. **Tooltip 文本未读数**：新分支的 `Tooltip title`（`agents.mcp.toolsUnknown.hint`）与既有 `7 个工具 · 33ms` 的 tooltip 文本**都没有被 DOM 读到**。两次尝试（`hover()`；`dispatchEvent("mouseover")`+`mouseenter` + `toBeVisible`）都没能让 antd 的 tooltip 出现；**仓里没有任何 spec 读 tooltip**（`grep ant-tooltip/hover panel/e2e/*.ts` 只命中我自己的两行）。⇒ 断言没有留成「永不出现也通过」，而是**删除**并在本报告点名。要测它需要：真实指针停留配合该组件的 `mouseEnterDelay`，或从 React 层读 `title`（本 harness 都没有做）。
2. **另两处同族读点，本单不做，只登记**：`Agents.tsx:233` `a.runtimes?.length ?? 0`（把「没告诉我们有几个运行时」显示成 `0`）与 `Agents.tsx:457` `runtimes: editing.runtimes ?? []`（编辑表单里把缺失变成空数组，保存时可能把「未读到」写成「清空」）。⇒ 后者比本单这一处**更危险**（不只是显示：它进表单、可能回写），建议作为下一单的第一位。
3. **英文 copy 的 DOM 路径**：本 harness 的语言是 `navigator.language`（本机渲染 **zh**），所以上面所有 DOM 读数都是**中文**那一套；`en` 的两条新 key 由 **i18n 门**钉住（zh/en 键集必须完全一致，§7），但没有 DOM 读数。
4. **真实守护进程下点第三态的回答**：未读数（注入拦截了它，见 §5 末）。要测需要一份 `tools` 真的为 `null` 的真实 MCP 服务器（不是注入）。
5. **`latency_ms` 的同类吸收**：本单**明确不动**（验收要求「latency 显示不得被改写」）。同时登记：若 `latency_ms` 缺失，今天会渲染成 `undefinedms`；这是**响亮的错**（不是自信的零），与 F1 族不同形，留给后续单判定。

## 7 门与纪律读数

| 门/命令 | 读数 |
| --- | --- |
| `npm --prefix panel run build` | **exit=0**；`build-panel: dist updated (55 assets, index.html swapped by rename)`（i18n-check → `tsc -b` → `tsc -p e2e/tsconfig.json --noEmit` → vite 全部通过） |
| `node panel/e2e/i18n-check.mjs` | **VERDICT: PASS**，`exit=0`（zh=en 键集；无重复键；所有字面 `t()` 可解析） |
| 目标 spec（改后） | `5 passed`，`exit=0` |
| 目标 spec（改前负控） | `3 failed / 2 passed`，`exit=1`（§4 逐字红） |
| 我起的两个临时守护进程 | 用**我记录的 listener pid**（31020 / 34020）停掉；停后 `8899`/`8791` 端口归属为空；`Get-Process ruagent` 只剩**活守护进程** 14944（`D:\rust_cache\debug\ruagent.exe`），不是我起的 |
| 临时目录清理（按具体路径，无通配） | `%TEMP%\ruagent-t128\prefix` **27.72 MB** 删除 → `exists=False`；`pw-prefix` **1.66 MB** 删除 → `exists=False`；保留 `logs\*.log`、`probeE2E.ps1`、`startDaemon.ps1`、`startControl.ps1`、`control.ps1`、`pw-postfix`（复核材料） |
| 未 push / dispatch / rerun / cancel / 建 tag | 是；未写活库；未碰 `~/.ruagent` |

## 8 方法学：本单踩到的三个读数陷阱（工具本身在污染读数）

1. **`Start-Process … -RedirectStandardOutput` 会把调用方挂住**：第一次起守护进程时 pwsh 调用在 300 s 处被超时杀掉（日志里已看到 `health=ok` 与端口归属），**而且把刚起的守护进程一起带走**（pid 29992 事后不存在）。AGENTS.md 已经记过这个形状（「both hung and truncated the previous log」）——我复现了它的**前半段**。改用**WMI `Win32_Process.Create` + `ShowWindow=0` + `cmd.exe /c … >> log 2>&1`**（仓库自己脚本的形状）后，进程不再随调用方死亡，且「端口归属」给出的是**守护进程自己的 pid**（31020），而我记录的 WMI 返回值（32576）只是 `cmd.exe` 包装 ⇒ **用返回值当 pid 会误杀 shell**。
2. **`-g "a|b"` 经 `cmd.exe` 会被 `|` 拆开**：`npm run … -- -g "measured number|down is unchanged"` 变成 `'down' is not recognized as an internal or external command`，`exit=255`——**看起来像测试失败，其实是参数被 shell 吃掉**。⇒ 过滤表达式要么不带 `|`，要么整条跑。
3. **`hover()` 打不开 antd tooltip，而「等不到元素」与「读到空串」必须区分开**：`locator.textContent()` 会等元素出现（超时=红），`locator.textContent()` 直接读不存在的元素则可能拿到空串（绿）——我选的是前者（会红），最终因 tooltip 根本不开而**删掉**该断言并点名（§6-1）。探针「红得对」比「红」重要。
