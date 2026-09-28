# t97 · design-audit 退出码载重（T-1）+ 过期读数更正（T-3）——修复报告

**改动对象**：`panel/tools/design-audit.mjs`（工作树 **544,462 B / 8,440 行 / sha256 `E3B7E45B59FA…`**；HEAD 版 538,865 B / 8,359 行 ⇒ 我的净增 **+81 行**，`git diff -U0` 只有 **12 个 hunk**，全部落在 `parseArgs` / `HELP` / `runSelfTest` / `main` 四处）· `docs/design/acceptance-report.md`（46,382 B / sha256 `426D2EA6C966…`）。
**纪律**：只读活库；**未**启停 pid 79984；**未**推 tag、**未**触发 release；临时文件（`%TEMP%\ruagent-t97-copy`、`%TEMP%\t97_exitdecision_probe.mjs`）已自清（见 §5）。**仓库的契约文档 `docs/design/MASTER.md` 一字未改**（负控只在副本树里做，见 §2.3 的恢复读数）。
**我是本单的实现者** ⇒ **任何针对 t97 的评审单不要派给我**（我不能评审自己的实现）。

---

## 1 改了什么（每条：位置 · 之前 · 之后）

| # | 位置（改后行号） | 之前 | 之后 |
| --- | --- | --- | --- |
| 1 | `design-audit.mjs:399` | `parseArgs` 默认值里没有 `allowNotMeasured` | 新增 `allowNotMeasured: false,` |
| 2 | `:417` | 无该 flag（未知 flag 会 `throw`） | `case "--allow-not-measured": o.allowNotMeasured = true; break;` |
| 3 | `:454-468`（HELP） | `--check … exit 1 on failure`；`--self-test … exit 1 on any failure` | 两段都写明**退出码载重**的完整条件；新增 `--allow-not-measured` 的说明（「每条被放行的行都会被点名；它永远不放行 contract/threshold 回落」） |
| 4 | `:488-497`（HELP） | 无「不覆盖什么」段 | 新增 `WHAT THIS TOOL DOES NOT COVER (t97, §6.0 item 19)` 四条（见 §4） |
| 5 | `:8075-8092`（`runSelfTest`） | 没有任何契约读取的断言 | 新增**守卫**（不是编号用例）：读 `loadThresholds(REPO)` + `resolveContract(...)`，把 warnings 数打印成 `self-test: contract/threshold read warnings = N`，逐条打印 `CONTRACT-WARNING <w>`；返回值改为 `failed.length \|\| guardWarnings.length ? 1 : 0` |
| 6 | `:8184-8192`（`main`） | 只用 `warnings` 一个数组，退出码只看 `fails` | 拆出 `contractWarnings = [...thresholds.warnings, ...contract.warnings]`（与 `warnings` 并存，前者不可豁免） |
| 7 | `:8095-8112`（新增 `exitDecision`，导出） | 无 | **纯函数**，退出码的**唯一**决策点：`fatal = fails>0 \|\| contractWarnings.length>0 \|\| (unmeasured.length>0 && !allowNotMeasured)`；返回 `{code, waived, fatal}`，`waived` 就是调用方必须点名的行 |
| 8 | `:8407-8430`（`main` 尾部） | `for (w of warnings) log(...); return args.check && fails.length ? 1 : 0;` | 打印 `contract/threshold read fell back N time(s)` + 逐条列出；`--allow-not-measured` 时打印 `--allow-not-measured waives N row(s): #a, #b` 与逐行 `waived #n <title>`；未给 flag 时打印 `not measured: N row(s) … — exit 1 (pass --allow-not-measured to waive; the waiver is named)`；`return exitDecision({...}).code` |
| 9 | `:8431-8440`（入口） | `main().then(...)` 无条件执行 | 加 `if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url))` 守卫 —— 让 `exitDecision` **可以被探针 import 而不启动一次 26-capture 的审计**（见 §2.4 的负控②） |

**没有削弱任何既有断言**：`runSelfTest` 的 495 个编号用例一字未动，新增的是一条**额外的**守卫（不计数），所以「495/495」这个团队门禁读数**保持不变**，而契约读失败现在也会让它红。

---

## 2 读数（每条：可复现命令 · 读数）

### 2.1 门禁 1 —— `--self-test`（改前 / 改后都绿，且新增守卫行为 0）
```
cd panel; node tools/design-audit.mjs --self-test
```
* 改前（t95 读数）：`self-test: 495/495 pass`，exit 0。
* **改后**：`self-test: contract/threshold read warnings = 0` · **`self-test: 495/495 pass`** · **`SELFTEST_EXIT=0`**（`node --check` 也 exit 0）。
* 副本树（未改契约）基线同样为 `contract/threshold read warnings = 0` + `495/495` + exit 0 ⇒ 副本与仓库等价。

### 2.2 门禁 2 —— `npm run build`
```
cd panel; npm run build
```
⇒ **BUILD_EXIT=0**，末行 `build-panel: dist updated (55 assets, index.html swapped by rename)`。

### 2.3 负控 ① —— 改一个契约单元格的措辞 ⇒ **红**（在 `%TEMP%\ruagent-t97-copy` 副本树里做，仓库契约不动）
副本布局：`%TEMP%\ruagent-t97-copy\{panel\tools\**, docs\design\**}` + `panel\node_modules` 用 **junction** 指向仓库（工具顶层 `import { chromium } from "@playwright/test"`，副本必须能解析到它）。
* **改动**：把 `docs/design/MASTER.md` §12.1 里 `#chat` 那一行的**门槛单元格**从 `**≥1**` 改成 `**至少一个**`（`[IO.File]::WriteAllText` + UTF8 无 BOM，仅副本）。
* **读数（副本，改后）**：
  ```
  self-test: contract/threshold read warnings = 1
  self-test: 493/495 pass
  self-test FAILED: contract: row 6 exempt routes read from MASTER §12.1; row6: ge18=2 on an exempt route (#chat) -> PASS
  POSCTRL1a_EXIT=1
  ```
  ⇒ 两件事同时发生：**守卫报出 1 条契约 warning**，且 2 条既有用例变红 ⇒ **exit 1**。
* **命名读数**（把「读失败」那一类单独打出来）：副本里另一次运行打印了
  `CONTRACT-WARNING MASTER §12.1 row #chat carries no numeric 门槛; skipped`
  ⇒ 回落是**点名的**，不再是「只在日志里的一句 warning」。
* **恢复读数（副本）**：`Copy-Item docs\design\MASTER.md 副本` 之后 ⇒ `contract/threshold read warnings = 0` · `495/495 pass` · **exit 0**。
* **仓库未被触碰的读数**：`docs/design/MASTER.md` 的 `| \`#chat\` | … | **≥1** |` 行仍在（`Get-Content | Select-String '^\| \`#chat\`'`），且该文件不在 `git status` 的改动列表里。

### 2.4 负控 ② —— `--allow-not-measured` ⇒ **绿且点名**（直接读 §1 的 `exitDecision`，不需要浏览器/daemon）
```
node %TEMP%\t97_exitdecision_probe.mjs      # import(file:///…/design-audit.mjs) 后调用 m.exitDecision(...)
```
四向读数（同一函数，也就是 `main` 唯一的退出码决策点）：
```
A allow=true  + unmeasured=[#12,#33] -> {"code":0,"waived":["#12","#33"]}   ← 绿 + 点名
B allow=false + unmeasured=[#12,#33] -> {"code":1,"waived":0}               ← 不放行 ⇒ 红
C allow=true  + contractWarnings=["MASTER §12.1 not found"] -> {"code":1,"fatal":true}   ← 豁免永远不覆盖回落
D clean (无 fail/无 warning/无 not_measured) -> {"code":0,"fatal":false}     ← 只有这种才绿
```
* **`main` 里同一路径的输出形状**（`:8416-8419`，flag 给到时）：
  `[audit] --allow-not-measured waives N row(s): #12, #33` + 每行 `waived #n <title>`；未给 flag 时 `:8420` 打印 `not measured: N row(s) #12, #33 — exit 1 (pass --allow-not-measured to waive; the waiver is named)`。
* **诚实标注**：A/B/C/D 是**代码路径读数**（读的正是产出退出码的那个纯函数），**不是**一次完整的 `--check` 运行 —— 本单没有跑过 `--check`（见 §3）。

### 2.5 特别读：`not_measured` 到底还会不会绿？
* `exitDecision({check:true, fails:0, contractWarnings:[], unmeasured:[…], allowNotMeasured:false}).code === 1` ⇒ **「54 行全 not_measured」不再是 exit 0**（改前 `:8351` 的 `fails.length` 只看 FAIL）。
* `fails>0` ⇒ 仍是 1（未回归）；`pending`（row 32 的判据修订态）**仍不进退出码** —— 见 §6(b)，这是有意保留的第三种状态。

---

## 3 T-2：`--check` 的成本读数（**只给读数与形状，不接 CI**）

* **本单没有跑 `--check`**（它需要 build 过的 panel + 一个可达 daemon + Playwright 浏览器，≈26 captures；`.github/workflows/**` 不在本单 inScope，且真 8787 由 pid 79984 占着不许动）。所以：
  * **耗时 = 明确标注为估算**：`docs/design-language.md:90` 记的 **全量 26 captures ≈ 6.5 min**（上一代实测）；`panel/acceptance-report` 里另有两轮 153.9s / 153.3s 的读数（那一代的环境）。
  * **四计数（pass/fail/not_measured/pending）= 未取得**（要有一次真跑）。★ 这是本单**唯一**没拿到的读数，留给接线任务：第一次接线的任务**应当**把这次读数当作接线前基线记下来。
* **我建议的接线形状**（二选一，由 captain 在推送后用独立任务定）：
  1. **`workflow_dispatch` + nightly 的独立 job**（推荐）：不进每推门禁（浏览器 + 6.5 min 太重），失败时按 `--check` 的退出码红；job 里先 `npm run build`，再用**临时 root** 起 daemon（`--addr 127.0.0.1:<free>`），跑 `--check --api-window=11500`，把 `checks N pass / M fail / K not measured` 与四计数**写进 job summary**（这样「红了」永远带着「为什么」）。
  2. **每代固定仪式 + 记录读数**：不入 CI，但 coverage declaration 里点名 `--check`，且每代必须留一份带日期的读数（`N pass / M fail / K not measured` + captures 数），由评审抽检。
* **接线后必须带负控**：故意把一条已知会 FAIL 的判据（例如某页 CSS 断点 >4）注入 ⇒ 该 job **红**；恢复 ⇒ 绿。否则接的还是一个会绿的仪式。

---

## 4 这套工具**不覆盖什么**（§6.0 第 19 条；同时已写进 `--help`，`:488-497`）

1. `--self-test` 只证明 **row 16/23 的判据逻辑**，加上（t97 新增）**契约确实被读到**；它**不判活 DOM** —— 给不出任何一条判据在**出厂面板**上的判决。
2. `--check` 判活 DOM，但**只对「已被 build、且由可达 daemon 服务」的 panel**成立（26 captures、分钟级墙钟）。**今天没有任何 CI job 跑它**（`ci.yml:344` 只有 `--self-test`）。
3. 两种模式都**不评价 daemon 的数据质量**：需要数据的行会报 `not_measured` —— `--check` 现在会因此 exit 1，这正是本次改动的意义（但「数据够不够」仍然只能由数据侧的门禁回答）。
4. 像素/截图类判据依赖浏览器的光栅化（字体、GPU、缩放）；没有改任何面板源码也可能动。
5. 不在本工具职责内：面板的**类型/i18n** 门禁（`build-panel.mjs` 的三步）、**e2e**（`run-e2e.mjs`）、**Rust** 侧的任何东西。

---

## 5 T-3：过期读数更正（一份在 inScope 内已改；另一份**交回**，不越界）

### 5.1 已改（在 inScope 内）：`docs/design/acceptance-report.md`
* **原文（逐字保留在修订块里）**：
  `| 工具自检 | \`node tools/design-audit.mjs --self-test\` → **160/160 pass**（含行 18 的 §12.9 内容区口径 / 外壳闭集 / 常数前提三组可证伪断言） |`
* **改动**：① 该行尾加 `**⟪t97（2026-09-29）更正：160/160 是当轮读数，今天同一命令为 495/495，见本节末修订记录⟫**`；② 表格下方新增 `⟪修订记录（t97，2026-09-29）⟫` 块：**原文逐字引用**为 blockquote，说明 160/160 是 **2026-09-22 当轮**读数、**2026-09-29 实测同一命令为 `495/495 pass`**（附工具体积/哈希/mtime），差异来自**本代断言数量增加**（不是守卫缩水），并写明「不要再把它当作『今天跑一次会得到什么』的判据」。
* **为什么保留旧值而不覆盖**：这是**当轮的验收证据**，覆盖它等于篡改历史；带日期的修订块才是可追溯的形状（与本代 `gen2-integration-contract.md` 的附录勘误同一做法）。

### 5.2 **交回给 captain（不在 inScope，我没有改）**：`docs/design-language.md`
* **任务单写的路径 `docs/design/design-language.md` 不存在**；真实路径是 **`docs/design-language.md`**（不在本单 inScope 的 `docs/design/` 下）⇒ 按你的指示**只交回路径与原文**：
  * **文件**：`docs/design-language.md`
  * **行 88（原文逐字）**：<code>node tools/design-audit.mjs --self-test                    # 203/203</code>
  * **行 89（原文逐字）**：<code>node tools/design-audit.mjs --check                        # §12 判定表（有 FAIL 则 exit 1）</code>
  * **今天的实测读数**：同一命令 **495/495 pass**（exit 0）。
  * **建议修法**（给该文件属主/接线任务）：把 `# 203/203` 改为 `# 495/495（2026-09-29 实测；旧读数 203/203）`，或干脆不写死数字、改成「全绿」+ 指向命令；行 89 可顺手补一句「退出码另受 contract/threshold 回落与 not_measured 影响（t97）」。

---

## 6 顺带读数（**不属于** T-1/T-3 的要求，留作下一轮的小项）

* **(a) §12.1 的节匹配是「前缀正则」而不是「整词」**：`sliceSection(masterMd, /^### 12\.1/m, /^### 12\.2/m)` —— 我在副本里把标题改成 `### 12.1x …` 时**没有**产生「找不到 §12.1」的 warning（切片照旧命中）。⇒ 一个**将来**新增的 `### 12.10 …` 会被吞进 §12.1 的切片。**可证伪判据**：在副本里加一个 `### 12.10` 小节并在其中放一行表格式文本 ⇒ 若 §12.1 的解析结果发生变化（行数/门槛集合变了）即为缺陷；修法 = 锚定 `^### 12\.1\s`（或用 `$`/`\s` 收尾）。**owner**：panel/tools（design-audit）。
* **(b) `pending` 仍不进退出码**（`row 32` 的判据修订态，`:5782`）：本次**有意不合并进 T-1 的严格化** —— 它是「判据待修订」的第三种状态，且已逐条打印（`:8344`）。若 captain 希望它同样载重，需要先给它一个「到期条件」（否则会变成第五条永久豁免）。
* **(c) 工具现在可被 import**（入口守卫，§1 #9）：这是「让退出码可被独立读数」的前提，未改变 CLI 行为（`node panel/tools/design-audit.mjs …` 一切照旧，所有门禁读数在改动后重取）。

---

## 7 未覆盖 / 限制（如实写）

1. **`--check` 一次都没跑** ⇒ 四计数与墙钟时间只有估算（§3）；`--check` 的新退出码路径是按**同一纯函数**（探针 A/B/C/D）读的，不是一次真实 DOM 运行。
2. **负控在副本树里做**：副本用 junction 复用仓库的 `node_modules`；`docs/design/**` 与 `panel/tools/**` 是副本，其余依赖（`panel/src` 等）不在副本里 —— 对 `--self-test` 足够（它只读契约与工具自身），但**不足以**跑 `--check`。
3. **未审 54 条判据本身**（本轮只动退出码与文档读数）；`panel/tools/lib/*.mjs` 仍未逐行读。
4. **`git diff --check` 干净**（exit 0），但两个文件仍带 git 的 autocrlf 提示（工作树 CRLF、入库 LF）；我的插入行若为 LF 会在下一次 `add` 时被规整 —— 不影响 node 解析与任何门禁，登记为已知噪音。
5. **临时产物**：`%TEMP%\ruagent-t97-copy`（含 junction 与副本）与 `%TEMP%\t97_exitdecision_probe.mjs` 我在收尾时删除；`panel/dist` 由门禁 `npm run build` 正常更新（构建产物，`.gitignore` 覆盖）。
6. **我是本单作者** ⇒ 不要派我做 t97 的评审（我会在拿到该任务时拒绝并说明）。

---

## 8 给评审者的复现清单（按顺序，全部只读）

```powershell
cd C:\Users\19410\Documents\ai\ruagent\panel
node --check tools/design-audit.mjs                                            # exit 0
node tools/design-audit.mjs --self-test                                        # 495/495 + "contract/threshold read warnings = 0" + exit 0
node tools/design-audit.mjs --help | Select-String 'allow-not-measured|LOAD-BEARING|DOES NOT COVER'
# 退出码四向（不需要浏览器/daemon）
node %TEMP%\t97_exitdecision_probe.mjs        # A 绿+点名 / B 红 / C 回落不可豁免 / D 才绿
npm run build                                                                   # exit 0
git diff -U0 -- ..\panel\tools\design-audit.mjs | Select-String '^@@'           # 12 个 hunk，全部在 parseArgs/HELP/runSelfTest/main
```
负控①需要一份副本树（不要改仓库契约）：复制 `panel\tools` + `docs\design` 到 `%TEMP%`，junction `node_modules`，把 `docs/design/MASTER.md` §12.1 的 `#chat` 门槛单元格 `**≥1**` 改成任意无数字的措辞 ⇒ `--self-test` 应报 `warnings = 1` + `493/495` + exit 1。
