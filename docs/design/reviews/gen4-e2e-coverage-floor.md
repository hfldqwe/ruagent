# E2E 证据步补「被测量数」守卫（t166）

> 单号 **t166**（repair）· 成员 `recall` · attempt **1** · 2026-10-04 19:0x–19:2x
> inScope 内改动：`.github/workflows/e2e.yml`（**+104 / −0，纯插入**）+ 本报告。
> `crates/**`、`panel/src/**`、`panel/e2e/**`、`scripts/**`、`tools/**`、`ci.yml`、`audit.yml`、`write-guard`、断路器（`RUAGENT_E2E_ALLOW_WRITES`）**零改动**。
> 来源：`t161`（`gen4-e2e-scripts-wait-tables.md`）里我**自己**做邻域核查时留下的一条**残留**（当时我的假设被流水线记账推翻，我如实交回）—— captain 据此立了本单。

## 0 结论

| 项 | 读数 |
| --- | --- |
| **加了什么** | 两条守卫，插在 `e2e.yml` 证据步**最后三条既有守卫之后**（新块 = **L297–399**）：**(b) 被测量数下限** `passed+failed+flaky >= 62`；**(a) 必跑集合**（5 个 spec，每个**自带理由**；`registry.spec.ts` 由既有行继续钉住，**未重复**） |
| **N 从哪来** | **不是拍脑袋**：**6 次连续成功的 E2E run**（`37138197234`/`37139111120`/`37141299806`/`37192592208`/`37194596042`/`37196424418`，2026-10-03/04）**每次都打印 `62 passed (...)`，0 failed / 0 flaky / `JSON says 0 skipped / 0 named`**；**我本机复跑也是 62**（§1） |
| **能红吗** | **7 格隔离矩阵全对**（§3）：今日形状绿；丢 8 个测量 ⇒ 红并点名；**总量仍达标但必跑 spec 被跳过 ⇒ 仍红**；合法数据依赖跳过 ⇒ **仍绿**；解析器自相矛盾 ⇒ 红；**把 N 调到 63 ⇒ 红**（证明下限是活的） |
| **没削弱什么** | diff **+104 / −0**；三条既有守卫的**文本与行号一字未动**（`L285`/`L289`/`L293`，与改前相同，见 §5） |
| **门禁** | `npm --prefix panel run build` **exit 0** · 本机 `npm run test:e2e`（仓库入口）**exit 0，`62 passed (18.2s)`** · `bash .github/workflows/scripts/check-workflow-refs.sh` **exit 0** |

## 1 先取读数，再定阈值（N = 62）

**CI 侧（来自 run 自己的日志，不是我推的）**——`gh run view <id> --log` 抽出的两行事实：

| run | Playwright 汇总行 | 证据步自己的行 |
| --- | --- | --- |
| `37196424418`（最近一次成功） | **62 passed (45.6s)** | `JSON says 0 skipped / 0 named` |
| `37194596042` | **62 passed (36.1s)** | `JSON says 0 skipped / 0 named` |
| `37192592208` | **62 passed (48.0s)** | `JSON says 0 skipped / 0 named` |
| `37141299806` | **62 passed (36.5s)** | `JSON says 0 skipped / 0 named` |
| `37139111120` | **62 passed (46.2s)** | `JSON says 0 skipped / 0 named` |
| `37138197234` | **62 passed (45.5s)** | `JSON says 0 skipped / 0 named` |

⇒ **6/6 都是 62，且一处跳过都没有** ⇒ **N := 62**，来源写进代码注释（`e2e.yml:297-311`）与本节。**升可以；降才是这条守卫要抓的事**；要改 N 必须**附一次同形读数**（注释里写死了这条规则）。

**本机独立复跑（§4）逐 spec 计数**（同一份 JSON 载体，与 CI 用的同一个 `PW_TEST_REPORTER=json`）：

| spec | tests | | spec | tests |
| --- | --- | --- | --- | --- |
| `responsive.spec.ts` | 12 | | `wiki.spec.ts` | 3 |
| `views.spec.ts` | 12 | | `settings-capabilities.spec.ts` | 2 |
| `palette.spec.ts` | 7 | | `chat.spec.ts` | 1 |
| `failure-visibility.spec.ts` | 6 | | `i18n.spec.ts` | 1 |
| `agents-health-third-state.spec.ts` | 5 | | `judge.spec.ts` | 1 |
| `consumption.spec.ts` | 4 | | `knowledge.spec.ts` | 1 |
| `graph-aliases.spec.ts` | 4 | | `recall.spec.ts` | 1 |
| | | | `registry.spec.ts` / `theme.spec.ts` | 1 / 1 |
| **合计** | | | | **62** ✓ |

`stats = {expected: 62, skipped: 0, unexpected: 0, flaky: 0}`、`errors: []` ⇒ **本机 62 == CI 62** ⇒ 这条下限不是机器特有的。

## 2 两条守卫，以及**为什么它们互补**（不是重复）

- **(b) 下限**：`m_ran >= 62`，其中 `m_ran = passed + failed + flaky`。
  - `flaky` **计入**测量（它先失败后重试通过，套件确实行使了它）；不算它会让「重试很多」的运行显得比实际小，而算上它**不可能掩盖**任何未测测试。这个取舍写在注释里。
  - **同时**做一次**两次独立读数必须一致**的自检：按 spec 走出来的 `passed+failed+flaky` 必须等于报告 `stats` 的 `expected+unexpected+flaky`；不等就红（**解析器坏了比读数错更危险**）。
- **(a) 必跑集合**（每个都是**它自己的理由**，不是「所有 spec 一律不许跳过」）：
  | spec | 为什么它必跑（跳过它会静默哪一类缺陷） |
  | --- | --- |
  | `recall.spec.ts` | **召回链（query → stubs → inline expand）只由它测**；它旧日的无条件跳过正是「recall matched no memories」被藏起来的方式，t113 的结论是 **a skip is not coverage** |
  | `consumption.spec.ts` | **唯一同时行使四条消费腿**（memory、`recall_log`、document、entity）并逐条断言的 spec；跳过它 = 整个消费面没被测而 job 仍绿 |
  | `failure-visibility.spec.ts` | 它断言「**失败是可见的**」（错误文本、而非空状态），以及「缺席要在一段有界窗口里读」——本代丢过一次测量就发生在这一类 |
  | `chat.spec.ts` | 它需要**已注册的 mock agent** ⇒ 它被跳过是**夹具回归的指纹**（同一原因也会静默 `judge.spec.ts`）；「mock agent 没注册」不该是一个更安静的绿 |
  | `settings-capabilities.spec.ts` | **第二个写保护 spec**：跳过它 = 守护进程 `policy.toml` 的 `[capabilities]` 路径没被行使（与 `registry.spec.ts` 同类理由，**不同的配置文件**） |
  | （`registry.spec.ts`） | **既有守卫已钉（`L288-291` 原文未动）**，本块**故意不重复** |
- **互补性（本单的要害）**：下限抓**总量**丢失；必跑集合抓**定点**丢失——**即使总量仍达标**。矩阵里的 **R2** 就是这个情形（62 个被测量**达标**、同时 `recall.spec.ts` 被跳过 ⇒ 仍然红）。反过来，合法的数据依赖跳过（`palette.spec.ts` 的 G2 格）**不会**被必跑集合误伤。
- **顺带一条实测观察**：本机是**全新 root、且 doctor 那步没跑成功**（§6 我自己的失误），**仍然 62/62、0 跳过** ⇒ 说明这些数据依赖跳过在正常路径上**不会**被触发，套件自己的写入者（`consumption.spec.ts` 写 wiki 页/memory/document/entity）就把后续 spec 需要的夹具造出来了 ⇒ 那些 `test.skip` 是**失效保护**，这也正是为什么该用「下限」而不是「一概不许跳过」。

## 3 守卫能红的证明（隔离执行 + 共享树零变异）

**方法**：把新块**从 `e2e.yml` 原样抽出**（`%TEMP%\t166_block.txt`，`sha256 87767506c09b4095…`，8058 字符）——**被测字节 = 提交字节**；按 YAML 块标量的语义**去缩进**（`sha256 77747ef717fb00ab…`，7048 字符）后放进**隔离临时目录**，用**合成的 Playwright JSON**驱动。**没有在共享树上造任何变异**（R4 那一格是改**抽出来的副本**里的 N）。

| 格 | 形状 | 期望 | 实得 | 判定 |
| --- | --- | --- | --- | --- |
| **G1** | 62 被测量 / 0 跳过（今日形状） | 绿 | **exit 0** | ✓ |
| **R1** | 54 被测量 + 8 个**带名字**的跳过（`wiki.spec.ts` 等） | 红并点名 | **exit 1** | ✓ |
| **R2** | 62 被测量（**达标**）+ `recall.spec.ts` 被跳过 | 红并点名 | **exit 1** | ✓ |
| **G2** | 62 被测量 + `palette.spec.ts` 被跳过（**不在**必跑集合） | 绿 | **exit 0** | ✓ |
| **R3** | 按 spec 走 = 62，`stats` = 40（解析器自相矛盾） | 红 | **exit 1** | ✓ |
| **R4** | G1 的同一份数据，但把副本里的 N 改成 **63** | 红 | **exit 1** | ✓ |
| **R5** | 62 被测量 + `chat.spec.ts` 被跳过（夹具回归指纹） | 红并点名 | **exit 1** | ✓ |

**守卫自己的原文（节选，逐字）**：
```
::error::only 54 test(s) were MEASURED, below the floor of 62 (passed=54 failed=0 flaky=0 skipped=1 of 55 collected).
          '0 failed' is not '0 measured': this run's green does not cover 8 test(s) that the reference runs covered …
::error::recall.spec.ts was SKIPPED, but it is in the must-run set: the recall chain (query -> stubs -> inline expand)
          is that spec's subject alone; its old unconditional skip hid 'recall matched no memories' …
::error::the JSON report disagrees with itself: per-spec statuses say 62 measured (passed=62 failed=0 flaky=0)
          while stats says 40 (expected=40 unexpected=0 flaky=0). Fix the parser before believing either.
measured: 62 test(s) >= floor 62; must-run set (recall, consumption, failure-visibility, chat, settings-capabilities) all executed
```
绿格写进 step summary 的三行（G1 实得）：
```
| measured tests (passed + failed + flaky) | 62 |
| collected (measured + skipped) | 62 |
| reference floor for this suite | 62 (six CI runs 2026-10-03/04, all `62 passed`) |
```
**共享树零变异（前后 sha256）**：`e2e.yml` 在我改完的瞬时 = **`097a1aad0e03359e09f1acfb3e198ff0…`**（YAML 合法性检查时读的），跑完 7 格证明 + 本机 E2E 之后**再读仍是 `097A1AAD0E03359E09F1ACFB3E198FF0…`** ⇒ **证明期间一个字节都没变**；证明的写入全部落在 `%TEMP%\t166\cells\**`；`git status` 里 `panel/e2e/**`、`panel/tools/**` **为空**（`write-guard.ts` sha256 `BDA940844E678BB6…`）。

## 4 门禁

| 门禁 | 命令 | 读数 |
| --- | --- | --- |
| panel 构建 | `npm --prefix panel run build` | **exit 0**（`✓ built in 4.97s`、`build-panel: dist updated (55 assets, index.html swapped by rename)`） |
| **E2E（仓库入口）** | `E2E_BASE_URL=http://127.0.0.1:19011 npm --prefix panel run test:e2e`（`PW_TEST_REPORTER=json`） | **exit 0（exit code 先于任何管道取）**，**`62 passed (18.2s)`**，JSON `stats.expected=62 / skipped=0 / unexpected=0 / flaky=0`、`errors: []` |
| workflow 守卫 | `bash .github/workflows/scripts/check-workflow-refs.sh` | **exit 0**：`workflow path references checked: 22, not tracked/missing: 0`；四份 workflow 全 `parses as YAML (PyYAML)` |

**E2E 的运行环境是我自己的**：临时 root `%TEMP%\t166\root`（含 `config/agents.toml`：`alpha`/`beta` scripted + `judge`，指向 `ruagent-mock-agent.exe`）、**临时端口 19011**、`RUAGENT_PANEL_DIST=<repo>/panel/dist`、`RUAGENT_EMBEDDER=hash`；守护进程是我 `Start-Process` 起的 **pid 20192**，收尾**只按我记录的 PID** 停掉（`alive=False` 已核），**活守护进程 8787 / pid 14944 全程未触碰**（多次核对 `alive=True`；按 C32 只读端口归属）。E2E 走的是**仓库入口**，写保护由 `run-e2e.mjs` 武装（未用裸 `npx playwright test`）。

## 5 没有削弱什么（可核）

- **`git diff --numstat` = `104 0 .github/workflows/e2e.yml`** ⇒ **纯插入**，没有删除或改写任何既有行。
- 三条既有守卫在改动后的**行号与文本都没动**：`L285`（`the JSON report says … but … could be NAMED` + `An unnamed skip is not a reading (t110/t104)`）、`L289`（`registry.spec.ts was SKIPPED …`）、`L293`（`Playwright run failed (exit $rc)`）—— 与改前一致（改前的行号就是 285/289/293）。
- `write-guard.ts` / `writeAccess()` / `RUAGENT_E2E_ALLOW_WRITES` 所在文件**零改动**（`panel/e2e/**` 干净）；`ci.yml`、`audit.yml` 未碰。
- **没有为了让新守卫好看而删掉任何合法跳过**（那 24 处 `test.skip(...)` 一处未动）。

## 6 我【没有】覆盖的（点名）

1. **新守卫还没在 CI 上跑过** —— 我不 push（推送是 captain 的事），所以**不存在**「CI 上绿」的读数；我的证据是 §3 的 7 格隔离矩阵 + §4 的本机 E2E。**本机 E2E 不会执行证据步**（那是 workflow 的一步，不是套件的一部分）⇒ 这两件事要分开读。
2. **只覆盖 `panel/e2e` 的「跳过/测量」计数**：不改那 24 处带名字的跳过本身；数据依赖的合法跳过（`wiki.spec.ts:53/76`、`palette.spec.ts:90`、`knowledge.spec.ts:35`、`views.spec.ts:114`、`chat.spec.ts:13/19`、`judge.spec.ts:20/27/44/51`、`recall.spec.ts:33`）**仍然合法**——本单只是让「它们真的发生时」那次运行**不再与绿混同**（而且**只在总量跌破 62 或落在必跑集合里**时才红）。
3. **生产代码、其它 workflow**（`ci.yml`/`audit.yml`/`release.yml`）不在范围内；`crates/**` 的忽略清单门禁（t144/t65）与面板侧的 t113 先例**未被本单改动**。
4. **N=62 的平台性**：它由 ubuntu CI 的六次读数定，并在 Windows 本机复现为 62 ⇒ 目前不平台敏感；**但如果将来某个平台的合法测试集真的变小，这条守卫会红**，那时按注释里的规则**附新读数**再改 N（这正是设计意图，不是缺陷）。
5. **我自己的失误（如实记）**：我第一次跑本机 E2E 时把 `doctor` 写成 `ruagent doctor --root <root>`，**exit 2（用法错误）** —— `--root` 是**全局**参数，必须在子命令**之前**。所以那次运行**没有 doctor 这一步**（CI 有）。这不影响本单的门禁（契约只要求 `npm run test:e2e`），但要说清：**「doctor 前置」这一格我没覆盖**；而且它顺带成了一个有用的读数（§2 末：无 doctor、全新 root，仍 62/62、0 跳过）。
6. **`#[ignore]`/`crates/**` 的测试面**不在本单（t157/t163 已盘）。
7. 我**没有**重跑 CI 的 E2E job（不 push、不 dispatch），也**没有**用 `gh run rerun`。

## 7 纪律回执

- **写入集合**：`.github/workflows/e2e.yml`（inScope）+ 本报告（inScope）；其余 inScope 目录（`.github/workflows/scripts`）**本轮不需要**（守卫内联在证据步里，**没有新增文件** ⇒ 也不触碰 `check-workflow-refs.sh` 的「被引用的路径必须被 git 跟踪」那条）。
- **共享树零变异**（§3 的前后 sha256）；红/绿证明全在 `%TEMP%` 内。
- **没启停活守护进程**：8787 / pid 14944 未触碰；我自己的临时守护进程按**自记 PID** 停（`alive=False`），临时 root 留作证据（`%TEMP%\t166\root`），临时脚本按**具体路径**删（提交前清理）。
- `NO_PROXY`：Rust/curl 与浏览器走 `127.0.0.1,localhost,::1`；`gh` 用 `*`（两者未混用）。
- 未跑 Rust 门禁（本单不改 Rust）；未用裸 `npx playwright test`。
