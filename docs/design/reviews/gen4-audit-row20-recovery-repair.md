# 审计行 20 修复 — `#settings` 的失败态「可点但不可恢复」（t151）

> 来源 = 审计自己给出的判定（首次 CI 运行的工件 `metrics.json`，run `37137554330`）：
> `row 20 · error 与 empty 可区分（失败态注入）· verdict=fail · modes=dark,light · scope=route`，两条失败原文都是
> `settings/{dark,light}=失败态 可见 ✓ · 重试 可点 ✓(49×32) · **恢复 ✗** ｜ 注入 3/3 端点 · 外壳 ✓ · 空态 无`。
> 我的写入集合：`panel/src/views/Settings.tsx`（唯一源码改动）+ 本报告。`panel/tools/design-audit.mjs`、`tools/**`、`crates/**`、`.github/**`、`scripts/**`、`panel/src/views/{Memory,Chat,Agents}.tsx`、`panel/src/ui.tsx`、`panel/src/index.css` **零字节改动**。
> ⚠️ **我（verify）是本单修复的作者**：下面的红→绿是我自己量的**读数**，不是独立评审 —— §9 给出可复现配方，仍需一位独立验证者。

## 0 结论

**定位到了「点了没用」的真实机理（不是猜），修掉后两档都 PASS，①③ 与「外壳仍在」一条没丢。**

**根因（实测）**：`#settings` 这条路由上有**两个互相独立的 loader**，各自有自己的错误态和自己的「重试」按钮：

| 组件（`panel/src/views/Settings.tsx`） | 它拉的端点 | 失败态 | 重试按钮的 handler |
| --- | --- | --- | --- |
| `DistillSettings`（`:52`，`load()` `:63-75`） | `GET /api/v1/distill`（`api.distillPolicy()`，`:65`）+ `GET /api/v1/agents`（`:72`，失败被吞成 `[]`） | `err`（`:55`）→ `ErrorState`（`:85-90`） | `onRetry={load}`（`:88`）**只重拉它自己** |
| `CapabilitiesSettings`（`:366`，`load()` `:388-396`） | `GET /api/v1/capabilities`（`:390`） | `err`（`:369`）→ `ErrorState`（`:404-411`） | `onRetry={load}`（`:409`）**只重拉它自己** |

两者都由 `Settings()`（`:33-47`）同时挂载（`:43`、`:44`）。审计注入的是**这条路由自己的 3 个端点**（`/api/v1/distill`、`/api/v1/agents`、`/api/v1/capabilities`；外壳的 `/permissions` 按工具规则排除，`design-audit.mjs:120`）⇒ **两块错误卡同时可见**；审计只**真实点击第一个**重试（`design-audit.mjs:203-215`：`unroute` → `click([data-audit-retry])` → 判据 =「可见的 `[role=alert] / .ant-alert-error / .state.error` 计数 === 0」）⇒ 第一块恢复、**第二块留在屏幕上** ⇒ `恢复 ✗`。**这不是「按钮没接线」，是「一次重试只恢复半页」。**

**修法**：把重试做成**页面作用域**（`epoch` + `onReload`），一次点击重跑这条路由上的**每一个** loader ⇒ 真重取数据、两块错误态都清掉。①（失败态仍可见）与③（不得渲染成空态/成功态）都保留，且**恢复后页面内容与健康态逐项相同**（`bodyText` 293 → 2566 == 健康态 2566；13 个开关 / 12 个输入 == 健康态）⇒ 是**真恢复**，不是把提示藏起来。

| 读数 | 臂 | 行 20 判定（原文） |
| --- | --- | --- |
| **红** | 修前字节（worktree @ HEAD，dist `Settings-Bn0pWRrx.js` sha256 `14E084E3F896A19F`） | `verdict=fail`：`settings/dark=失败态 可见 ✓ · 重试 可点 ✓(49×32) · 恢复 ✗ ｜ 注入 3/3 端点 · 外壳 ✓ · 空态 无; settings/light=…恢复 ✗…` ⇒ **与 CI 失败原文逐字一致** |
| **绿** | 修后最终字节（`Settings.tsx` sha256 `8A0E7381715FE0EA` → dist `Settings-DaP8kKWd.js` sha256 `3206A98267A32D24`） | `verdict=pass`：`settings/dark=失败态 可见 ✓ · 重试 可点 ✓(49×32) · 恢复 ✓ ｜ 注入 3/3 端点 · 外壳 ✓ · 空态 无`（light 同） |

**判据逐字未动**：两臂 `checks[20].criterion` 长度都是 **430**、内容逐字相同，`limits` 都是 `{"wantAlert":true,"wantRetry":true}`；`panel/tools/design-audit.mjs` sha256 `E3B7E45B59FA1C49…`（mtime 2026-09-29）**一字节没改** ⇒ 不需要改工具。

## 1 仪器与字节

| 项 | 值 |
| --- | --- |
| 审计工具（只读） | `panel/tools/design-audit.mjs`（sha256 `E3B7E45B59FA1C49…`）；行 20 探针 `probeFailureState`（`:121-228`）、`FAILURE_SKIP = /\/permissions$/`（`:120`）、失败态/重试识别（`:161-195`）、**恢复判定**（`:200-221`）、行定义与 `judge`（`:4511-4545`） |
| 调用 | `node panel/tools/design-audit.mjs --check --routes=settings --modes=dark,light --tabs=40 --no-shots --no-pixels --allow-not-measured --base-url=http://127.0.0.1:<port> --out=<仓外绝对目录>`（**没有**加 `--no-failure-probe`，即行 20 的注入开启；`--out` 在仓外） |
| 我自己的 daemon | 私建 `cargo-team.ps1 rustc -p ruagent --bin ruagent -- -o %TEMP%\t151\bin\ruagent.exe`（exit 0 / 47.9 s / sha256 `ECAD09C6DA6EDB5B…`）；**临时 root** + **临时端口 8851（修后臂）/ 8852（修前对照臂）**；`NO_PROXY=127.0.0.1,localhost,::1`；8787 的常驻 pid 14944 **未触碰** |
| 两臂的 dist | 8852 供 worktree @ HEAD 的 `panel/dist`：`/assets/Settings-Bn0pWRrx.js` → 200（**12,918 B**），修后那份 → 1139 B（index 回退）；8851 供仓内 `panel/dist`：`Settings-IzBtgtZg.js` → 200（**13,054 B**）⇒ 两臂各自供自己的字节（线上探过） |
| **先定位**用的探针（不是证据来源） | `%TEMP%\t151-probe.mjs`（复刻审计的注入集合 + 真点击 + 前后清点错误子树/重试控件/外壳/空态）、`%TEMP%\t151-probe2.mjs`（健康态地板 + 点**第二个**重试 + 恢复后的内容比对） |
| 字节归属 | 修前 `panel/src/views/Settings.tsx` = 已提交文本（worktree 检出 sha256 `0753FE78020474A8…`，与工作副本同文本、行尾不同）；修后 sha256 `8A0E7381715FE0EA…`（mtime 2026-10-04T02:09:26）；`git status --porcelain -- panel/` 只有 ` M panel/src/views/Settings.tsx` |

## 2 先定位：点击后**应该**发生什么 vs **今天实际**发生什么

**应该发生**（代码路径，符号优先）：用户看到的错误卡来自 `ErrorState`（`panel/src/ui.tsx:83-111` = antd `<Alert type="error">` + `action={<Button onClick={onRetry}>}`，49×32 的按钮就是它）；点它 ⇒ `onRetry` 被调用 ⇒ 对应组件重新发请求，成功时 `setData`/`setPolicy` + `setErr(null)`（`DistillSettings` `:66-69`、`CapabilitiesSettings` `:391-394`）⇒ 该卡的错误态消失。

**今天实际发生（我实测，两个独立读数）**：
1. `%TEMP%\t151-probe.mjs`（修前字节）：注入 3 个端点后 —— `errors visible: 2`（`无法读取蒸馏策略…` 与 `无法读取能力配置…`，两者都是 `[role=alert]`）、`retry controls visible: 2`（都 49×32，都在 `.error-state` 内）；审计会 tag/点击**第一个**（蒸馏那张）；真点击后 ⇒ **`errors visible: 1`** —— 留下的是**能力配置**那张 ⇒ 审计判据（可见错误 === 0）为 `false` ⇒ **`恢复 ✗`**。
2. 审计自己的 `note`（修前臂，逐字节选）把同一个事实写在盘上：`失败态呈现：div.ant-alert…「无法读取蒸馏策略…」 · div.ant-alert…「无法读取能力配置…」 · 重试控件 button.ant-btn…「重 试」真实点击成功 · 外壳完好（侧栏 + main 在位，body 293 字符）` —— **两条错误呈现 + 点击成功**，与判据 `恢复 ✗` 一起读，就是「一次点击只救回一张卡」。

⇒ 所以这**不是**「重试按钮没接线」：点击**确实**触发了请求（修前臂 bodyText 也从 293 涨起来），它只是**只重取了页面的一部分**。修法必须落在「一次重试恢复整页」。

## 3 修法（`panel/src/views/Settings.tsx`，唯一源码改动）

页面作用域的重试：
```tsx
export function Settings() {
  // t151 …（注释见源码：两 loader / 实测两块 role=alert / 一次点击只清一张）
  const [epoch, setEpoch] = useState(0);
  const reload = useCallback(() => setEpoch((e) => e + 1), []);
  …
      <DistillSettings epoch={epoch} onReload={reload} />
      <CapabilitiesSettings epoch={epoch} onReload={reload} />
}
// 两个卡各自：
function DistillSettings({ epoch, onReload }: { epoch: number; onReload: () => void }) {
  …
  useEffect(() => { load(); /* mount + 每次 epoch 变更都重读本卡 */ }, [epoch]);
  … <ErrorState … onRetry={onReload} retryLabel={t("common.retry")} />
}
```
- **这不是「把提示藏起来」**：`onReload` 只做 `setEpoch(e => e+1)`，两个 loader 的 effect 因此**重新发请求**（`api.distillPolicy()` / `api.agents()` / `api.capabilities()`），成功时各自 `setErr(null)`；错误态的清除仍然来自**数据到位**（`if (!policy)` / `if (!data)` 分支不再成立），而不是任何「隐藏」逻辑。
- **真恢复的实测**（`%TEMP%\t151-probe2.mjs`，修后最终字节）：健康态地板 = `errors=[] / bodyText=2566 / 13 switches / 12 inputs`；注入后 = 2 条错误；**点击第一个**重试后 = `errors=[]`、`bodyText=2566`、`13 switches`、`12 inputs` **与健康态逐项相同** ⇒ 内容真的回来了。**点第二个重试同样恢复**（`count=2` 个重试，点第二个 ⇒ `errors=[] recovered=true`，内容同健康态）⇒ 两个卡对称（它们共用同一个 `reload` 回调）。
- **没有牺牲 ①③**：注入期间仍是**每张卡自己的可见错误**（2 条 `role=alert`，含端点名与原因；不是空态），`空态 无`；外壳（侧栏 + main + body 文本）在注入期间与恢复后都在。
- **没有删功能**：`ErrorState`、`load()`、`err` 状态、每张卡的失败文案、`重试` 按钮、保存路径（`:419` 起）全未改动；`panel/src/ui.tsx` 未改 ⇒ `ErrorState` 对**所有**其它视图的行为一字不变。

## 4 红 → 绿（两档，两边都是工具自己的判定原文）

**修前臂**（worktree @ HEAD，daemon 8852，`Settings-Bn0pWRrx.js`），`exit=1`，表格行（逐字）：
```
| 20 | FAIL | error 与 empty 可区分（失败态注入） | 每路由在 API 失败时呈现 role=alert + 重试按钮 | settings/dark=失败态 可见 ✓ · 重试 可点 ✓(49×32) · 恢复 ✗ ｜ 注入 3/3 端点 · 外壳 ✓ · 空态 无; settings/light=失败态 可见 ✓ · 重试 可点 ✓(49×32) · 恢复 ✗ ｜ 注入 3/3 端点 · 外壳 ✓ · 空态 无 |
```
`metrics.json`：`verdict=fail`，`limits={"wantAlert":true,"wantRetry":true}`，两条 evidence 的 `pass=false`：
`[settings/dark] display=失败态 可见 ✓ · 重试 可点 ✓(49×32) · 恢复 ✗ ｜ 注入 3/3 端点 · 外壳 ✓ · 空态 无`（light 同）。
`note`（逐字节选）：`注入端点（该路由自己的，已排除外壳 /permissions）：/api/v1/distill, /api/v1/agents, /api/v1/capabilities · 失败态呈现：…「无法读取蒸馏策略…」 · …「无法读取能力配置…」 · 重试控件 button.ant-btn…「重 试」真实点击成功 · 外壳完好（侧栏 + main 在位，body 293 字符）—— 不是白屏`
⇒ **与任务引用的 CI 失败原文一致**（dark 与 light 都 `恢复 ✗`、`注入 3/3 端点`、`外壳 ✓`、`空态 无`；重试命中区 `49×32` 也对上）。

**修后臂**（最终字节，daemon 8851，`Settings-IzBtgtZg.js`；gate 重建后为 `Settings-DaP8kKWd.js`，两者同源 `Settings.tsx` sha256 `8A0E7381715FE0EA…`、同为 13,054 B），表格行（逐字）：
```
| 20 | PASS | error 与 empty 可区分（失败态注入） | 每路由在 API 失败时呈现 role=alert + 重试按钮 |  |
```
`metrics.json`：`verdict=pass`，`limits={"wantAlert":true,"wantRetry":true}`（与红臂逐字同），evidence：
`[settings/dark] pass=True display=失败态 可见 ✓ · 重试 可点 ✓(49×32) · 恢复 ✓ ｜ 注入 3/3 端点 · 外壳 ✓ · 空态 无`
`[settings/light] pass=True display=失败态 可见 ✓ · 重试 可点 ✓(49×32) · 恢复 ✓ ｜ 注入 3/3 端点 · 外壳 ✓ · 空态 无`
⇒ **三处（失败态 / 重试 / 恢复）在两档都全 ✓**，且 `注入 3/3 端点`（说明注入面没被缩小）、`外壳 ✓`、`空态 无`。
（该 `--check` 整轮 exit=1，**只因行 13** `暗色文本对比度失败数`（knowledge 上 1 个文本节点，工具版本差异，**与 `#settings` 无关**，红臂绿臂同一读数）；`#settings` 上的 4/7/38/75 两臂都 PASS。）

## 5 顺带确认（判据本就要求的两条）

- **外壳仍在**：两臂 `外壳 ✓`；修前臂 note 明写 `外壳完好（侧栏 + main 在位，body 293 字符）—— 不是白屏`；我的探针在注入期间读到 `sider=true, main=true`。
- **不得渲染成空态/成功态**：两臂 `空态 无`；注入期间是**显式错误呈现**（`div.ant-alert.ant-alert-error…` 两条，各带原因与端点名），不是 `.ant-empty`；恢复后是**真数据**（bodyText 2566 = 健康态）。
- **健康态没有多余的 alert**（这条决定判据可满足）：`%TEMP%\t151-probe2.mjs` 在**无注入**的健康 `#settings` 上读到 `errors=[] retries=0` ⇒ 恢复判据（可见错误 === 0）不是被一条合法 alert 永久钉死的。

## 6 面板门禁

`npm --prefix panel run build`（仓根）⇒ **exit 0**（修后与最终各一次：`✓ built in 4.13s / 7.93s`、`build-panel: dist updated (55 assets, index.html swapped by rename)`；内含 `e2e/i18n-check.mjs` + `tsc -b` + `tsc -p e2e/tsconfig.json --noEmit` + vite）。**没有新增/修改任何 e2e spec** ⇒ 按契约不触发 `npm run test:e2e`，也**没有**用裸 `npx playwright test`（行 20 的哨兵在 CI，由 `.github/workflows/audit.yml` 驱动；`panel/e2e/settings-capabilities.spec.ts` 会写真实 `policy.toml`，按 AGENTS.md 必须经 `npm run test:e2e` + `RUAGENT_E2E_ALLOW_WRITES=1`，本单不跑）。

## 7 未覆盖什么（第 19 条）

1. **其余路由**的行 20：只跑了 `#settings`（修复目标）；其它路由的失败态/恢复未测（本单不动它们，但也没有读数）。
2. **真实网络故障**（非注入）：审计注入的是 `page.route` 的 500；真实的连接失败/超时/半包（`fetch` 抛 `TypeError`）走的可能是**同一** `catch`，但我**没有**用真断网读数证明二者路径一致。
3. **恢复后的数据正确性**：我只证明「错误消失 + 内容与健康态逐项相同」（bodyText/开关/输入计数）；**没有**逐字段比对恢复后的 policy/capabilities 值与 daemon 的真实返回（那需要另一套量具）。
4. **light 之外的档位**：本单覆盖 dark + light（该行 `modes=["dark","light"]`）；`prefers-contrast`/放大/高对比主题不在该行口径内，未测。
5. **只有第一个重试被审计点击**：第二个重试的恢复是**我用探针**测的（`probe2`），不是审计的读数；两个卡共用同一个 `reload` 回调，对称性是代码结构保证的。
6. **像素/截图行**：两轮都带 `--no-shots --no-pixels`，`--allow-not-measured` 逐行点名的 `not_measured` 行不在本单范围；**全量 `--check`（13 路由 × 两模式）**与 CI job 重跑未做。
7. **`--no-failure-probe` 的 not_measured 分支**（该路由没有自己的端点时）未测——`#settings` 有自己的端点。

## 8 过程、收尾与残留

- **对照臂的来源与其安全**：`git worktree add --detach %TEMP%\t151-prefix HEAD`（**不含**我的改动；worktree 里 `Settings.tsx` sha256 `0753FE78020474A8` 与已提交文本一致）+ `mklink /J` 复用仓内 `panel/node_modules`（**没有 `npm ci`**）+ `node scripts/build-panel.mjs`（exit 0）。收尾先 `cmd /c rmdir <wt>\panel\node_modules`（junction 只删链接）再 `git worktree remove --force`；**仓内 `node_modules/playwright` 事后仍在**；`git worktree list` 只剩主树。
- **进程**：两个临时 daemon（pid **32764** 端口 8851 / pid **22664** 端口 8852）按记录 PID 停掉（`alive=False`）；浏览器由脚本自行 `browser.close()`，并**按我自己的临时路径**（`*t151*`）扫过 `chrome.exe/msedge.exe/headless_shell.exe` ⇒ **无我的孤儿**；**没有**按进程名/端口批量杀；**8787 的 pid 14944 未触碰**（StartTime 2026/10/2 23:05:45）。
- **临时文件**：清掉 **2.25 GB**（328 MB 私建 exe + 2 GB `.pdb` + 两个 root），保留两臂的 `metrics.json`（`%TEMP%\t151\audit-{pre,post}\metrics.json`，共 1,146,150 B）与日志（15 个文件 / 531,992 B）作为证据；`--out` 全程仓外（工具自己也拒绝仓内目录）⇒ **无 `docs/screenshots` 残留**。C: 可用 **61.72 GB**。
- **仓内改动面**：`git status --porcelain -- panel/` = **仅** ` M panel/src/views/Settings.tsx`（+ 本报告 `??`）。

## 9 声明：作者不是评审 + 复现配方

我是本单修复的**作者**，§4 的红→绿是我用审计工具自己量的**读数**，不是独立验证。按队内纪律，这批字节**仍需一位独立验证者**：重跑行 20（修前/修后、dark+light）并判「修法是否落在真实重取 + 清错误态上，且没有牺牲 ①③」。配方（我用的原命令）：

```powershell
# 修后臂 = 仓内 panel/dist（我的字节）；修前臂 = git worktree @ HEAD + junction node_modules + node scripts/build-panel.mjs
# 各由自己的临时 daemon 服务（临时 root/端口，RUAGENT_PANEL_DIST 指向对应 dist；NO_PROXY=127.0.0.1,localhost,::1）
node panel/tools/design-audit.mjs --check --routes=settings --modes=dark,light --tabs=40 `
  --no-shots --no-pixels --allow-not-measured --base-url=http://127.0.0.1:<port> --out=<仓外绝对目录>
# 读 <out>/metrics.json 的 checks[20]：verdict / limits / evidence[].{mode,pass,display,note}
# 行 20 的关键判据在工具里：design-audit.mjs:200-221（unroute → 真点击 [data-audit-retry] → 可见错误计数 === 0）
```
