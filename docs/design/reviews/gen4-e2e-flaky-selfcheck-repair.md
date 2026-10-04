# t178 修 F-1：E2E 新块的自检读的是 `stats_flaky` 而不是逐 spec 的 flaky（mem-core）

- 任务：t178（kind=repair）；attempt `806d87aa-ccd7-4e7b-bb3e-53c7e5680bba`
- 改动：`.github/workflows/e2e.yml`（**+60 / −17**，全部落在证据步的新块内）+ 本报告
- **一句话**：那条自检的左半边以前**不是**逐 spec 算出来的值（贪婪 `sed` 命中了同一行后面的 `stats_flaky=`），现在**左右两侧真的是两个独立读数**；并且**提取不到字段会按名字失败**。
- **但必须先说清楚它今天的地位**（见 §6）：`flaky=0` ⇒ 这条自检**今天是休眠的**；本单**不是**「修好了一个会红的门禁」，而是「把一条读错字段的自检改成它自称的那样」，而且**它还没在 CI 上跑过**（本单不 push）。

## 1 先量证 F-1（坐标 + 原文，都是从字节里读的，不采信 t174 的转述）

**三个坐标**（行号 = 改动前的 `HEAD` 版本，即 `097a1aad…`；工作树当时与 HEAD 同值）：

1. **计数行的打印**（旧 `L333-339`）：把**逐 spec 走查**的结果与**报告的 `stats` 块**并排打印在**同一行**上：
   ```
   JSON_COUNTS measured=… passed=… failed=… flaky=… skipped=… collected=… stats_expected=… stats_unexpected=… stats_flaky=… stats_skipped=…
   ```
2. **提取**（旧 `L351`，原文）：
   ```
   m_flaky=$(printf '%s\n' "$counts" | sed -n 's/.*flaky=\([0-9]*\).*/\1/p')
   ```
   `.*flaky=` 是**贪婪**的 ⇒ 它匹配到这一行里**最后一个** `flaky=` —— 而那一行有两个。
3. **自检**（旧 `L360`，原文）：
   ```
   if [ "$s_expected" != "?" ] && [ "$((m_passed + m_failed + m_flaky))" != "$((s_expected + s_unexpected + s_flaky))" ]; then
   ```
   右边 `s_*` 全部来自 `stats_*`；左边本应是「逐 spec 走查」，但它的 `flaky` 项**已经是 `stats` 的值** ⇒ 这一比较**比它读起来弱**。

**两处孪生字段与它们在 JSON 里的层级**：

| 打印字段 | 来源 | JSON 层级 |
| --- | --- | --- |
| `flaky=` | 走查 `c.flaky`（node 里 `walk()` 累计） | **`suites[].specs[].tests[].status === "flaky"`**（每个 test 的状态，逐层递归套件） |
| `stats_flaky=` | `g("flaky")` ⇒ `r.stats.flaky` | **`stats.flaky`**（报告顶层的聚合块） |

同类但今天不参与判定的一处（一并修掉）：旧 `L352` 的 `m_skipped` 同样是贪婪命中 **`stats_skipped`**（`m_skipped` 只在错误消息里出现，故它今天不改变判定，但同属「读的不是它自称的那个字段」）。

**我自己的量证（BEFORE/AFTER 同一个 harness，同一份 fixture）**：fixture = 逐 spec `61 expected + 1 flaky`（measured 62）而 `stats = expected 62 / flaky 0`（measured 62，两侧总量相等）。
- **旧块**（从 HEAD 抽出的 `L297-399`，去缩进，`sha256 77747ef717fb00ab…`）⇒ **exit 1**：
  `::error::the JSON report disagrees with itself: per-spec statuses say 61 measured (passed=61 failed=0 flaky=0) while stats says 62 …`
  —— 左侧算出 **61**、且 `flaky=0`，尽管同一份报告上一行 `JSON_COUNTS … flaky=1 measured=62`。（**这是假红**：两条独立读数在总量上其实一致。）
- **新块**（同 fixture）⇒ **exit 0**（`measured: 62 test(s) >= floor 62 …`）。

## 2 修法：让两侧真的独立（并让「读不到」按名字失败）

1. **per-spec 字段加 `walk_` 前缀**（node 侧改为 `walk_measured=` / `walk_passed=` / `walk_failed=` / `walk_flaky=` / `walk_skipped=` / `walk_collected=`；`stats_*` 一字不改）。
   任何 `stats_*` token 都**不可能**含 `walk_` ⇒ 贪婪 `sed` 的整类问题消失；`walk_*` = 这个脚本自己数的，`stats_*` = 报告自己说的，**两侧可见地是两个读数** ⇒ 不是「同一个字段与自己比」（自检因此**不是恒真**：§3 的 C3 证明它仍能红）。
2. **扫描下限（B32 第 1 件）**：提取之后、参与算术之前，逐个检查 6 个 `walk_*` 是否**非空且全数字**；不满足 ⇒ `::error::could not read <名字> out of the counts line (…)` + `exit 1`。理由：空串会让 `$(( … ))` 报错或**静默少一项**，而「我读到 0」与「我什么都没读到」不是一件事。
3. **`stats_*` 一侧的容忍是「具名容忍 + 理由」，不是扫描排除**：报告没有 `stats` 块时 node 代入字面量 `?`，此时自检**故意跳过**（它无法与报告没写的东西比较）——这是**改动前就有的语义**，我**保留**；但**空串**（=提取失败）现在是失败。⇒ 本单**不需要任何扫描排除**，因此没有排除表可给。

## 3 隔离矩阵（`%TEMP%` 内执行，逐格读退出码与块自己的原文）

矩阵跑的是**从工作树抽出的新块**（去 10 空格缩进后 `sha256 69d6177f4493982c…`；缩进原样 `6e911253309dd83e…`，块现在跨文件 `L297-441`）。

| 格 | 形状（我自造 JSON） | 退出码 | 块自己的原文（截断） |
| --- | --- | --- | --- |
| **C1** | 今日形状：逐 spec 62 / stats 62·0·0·0 | **0** | `measured: 62 test(s) >= floor 62; must-run set (…) all executed` |
| **C2** | **旧块的假红形状**：逐 spec 61+1 flaky / stats 62+0 flaky（两侧总量 62） | **0**（旧块同 fixture = **1**） | 同上 ⇒ **假红消失，且左侧确实是逐 spec 的值** |
| **C3** | **真不一致**：逐 spec 62 / stats 40 | **1** | `the JSON report disagrees with itself: per-spec statuses say 62 measured (…) while stats says 40 (…)` ⇒ **两侧都点名，自检非恒真** |
| **C4** | 下限仍活：逐 spec 54 | **1** | `only 54 test(s) were MEASURED, below the floor of 62 (…) does not cover 8 test(s) that the reference runs covered` |
| **C5** | 5 必跑集合仍活：`chat.spec.ts` 进跳过名单 | **1** | `chat.spec.ts was SKIPPED, but it is in the must-run set: …` |
| **C6** | 报告**没有 `stats` 块**（node 写 `?`） | **0** | `measured: 62 …` ⇒ 「无 stats ⇒ 自检跳过」这条**原有语义被保留**（我的空串下限没有误伤它） |
| **C7** | flaky **两侧一致**（逐 spec 61+1 / stats 61·0·1） | **0** | `measured: 62 …` |
| **C8** | **扫描下限**：把**我那份副本**里 node 的 `walk_flaky=` 打印删掉 | **1** | `::error::could not read walk_flaky out of the counts line (\`JSON_COUNTS walk_measured=62 walk_passed=62 walk_failed=0 walk_skipped=0 walk_collected=62 stats_…\`)… fix the extractor, do not let this pass.` |

**共享树零变异**：所有负控只动 `%TEMP%` 里的副本（C8 的变异副本 `sha256 28cbc9ef6cda143f…` ≠ 工作树抽出的块 `69d6177f4493982c…`）；仓库里唯一的写入是本单的**修复本身**（`e2e.yml`），不是负控。

## 4 真实报告上的读数（不只是合成 JSON）

形状与 t174 相同（**我自己的 root（`RUAGENT_HOME`）+ 我自己的端口 8899 + 我自己记录的 pid**；`run-e2e.mjs:21` 在 `CI` 下会指向**活守护进程 8787**，而 CI 那步把 mock agents 写进 `~/.ruagent/config/agents.toml` ⇒ 本机就是操作者真配置，所以我不照抄）：

- 套件（**仓库入口**）：`npm run test:e2e -- --reporter=list` ⇒ **`NPM_E2E_EXIT=0`**、`62 passed (19.0s)`。
- **把新块喂给这份真实 JSON**（`skip_list` 为空：真跑一跳没有）⇒ **exit 0**，块自己打印：
  `JSON_COUNTS walk_measured=62 walk_passed=62 walk_failed=0 walk_flaky=0 walk_skipped=0 walk_collected=62 stats_expected=62 stats_unexpected=0 stats_flaky=0 stats_skipped=0`
  摘要表：`measured tests (passed + failed + flaky) | 62` · `collected (measured + skipped) | 62` · `reference floor … | 62 (six CI runs …)`。
- 我起的守护进程 pid **30704**，按记录的 pid 停掉并确认 stopped；8787 一次未碰。

## 5 我**没有**改动的语义（逐条）

- `passed+failed+flaky >= 62` 的**下限**语义：值未变（`E2E_MEASURED_FLOOR=62` 那行**逐字节相同**），判定行**只有变量名变化**（`m_ran` → `walk_ran`）—— **这一行文本确实变了，我不藏它**；C4 证明它仍然能红、消息与旧版同形。
- **5 个必跑 spec**（含各自的理由文本）、`registry.spec.ts` 的既有钉法、`rc != 0` 守卫、unnamed-skip 守卫：**逐字节相同**（L285/L289/L293 行哈希 HEAD vs 现在 **IDENTICAL**，多行区域同样比对过）。
- 所有 hunk 都落在 `@@ -332,0 …` 之后 ⇒ 改动**局限在证据步的计数段落**；`git diff --numstat` = **60 / 17**。
- `ci.yml` / `audit.yml` / `release.yml` / `.github/workflows/scripts/**` / `panel/**` / `crates/**` / `scripts/**` / `tools/**`：`git status --porcelain` **无输出** ⇒ 未动。
- 前后字节：`e2e.yml` 从 HEAD `097a1aad0e03359e…`（418 行）→ 现在 `9026a94f3ba7b096…`（461 行）。

## 6 未覆盖 / 不许被读成什么（点名）

1. **它今天是休眠的**：真实报告 `flaky=0`，且逐字段一致 ⇒ 本单**没有**证明「一个会红的门禁被修好了」。它证明的是：**左侧现在真的是逐 spec 的读数**（C2 的 before/after 就是这条的唯一直接证据），且这类字段误读**不会再发生**。
2. **没有在 CI 上跑过它**（本单不 push）⇒ 它在 runner 上的行为仍是**推测**；要覆盖需要一次 main push（或 dispatch），本单禁止。
3. 我的 8 格里 7 格是**合成 JSON**；唯一用**真实报告**的一格（§4）里 `flaky=0` ⇒ 「真实 flaky 事件」这条路**仍未被真实数据覆盖**（它只能等某个测试真的 flaky）。
4. windows 未测（该 workflow 只 ubuntu）。
5. 我**没有**跑 `panel/**` 的 e2e 之外的东西来「验证」这个 workflow 改动 —— workflow 的行为只能在 runner 上被真正执行；我能做的是把块抽出**在隔离环境里执行**。

## 7 门禁与残留

| 门禁 | 读数 |
| --- | --- |
| `bash .github/workflows/scripts/check-workflow-refs.sh` | **exit=0**（22 refs / not tracked 0 / 四份 workflow `parses as YAML (PyYAML)`） |
| `npm --prefix panel run build` | **exit=0**（`✓ built in 4.13s`，`dist updated (55 assets)`） |
| `npm run test:e2e`（**仓库入口**） | **exit=0**，`62 passed (19.0s)` |

**我自己的一次假读数（记下来）**：写比较脚本时我把两边的行哈希都取成了**空串**（`sed` 的引号被外层 PowerShell 吃掉），于是打印出一排 `IDENTICAL` —— **「两个空串相等」不是读数**。改成脚本文件（引号不经外层）后重做，得到真实哈希：L285 `a4ed9235986d` idx/L289 `e9512804ba66`/L293 `00c1c8bea1df`，两侧相同。⇒ 与 t174 的「全红先怀疑自己」成对：**全绿（尤其是空值上的绿）也要先怀疑取证方式**。

**残留**：`.github/workflows/e2e.yml`（唯一代码改动）+ 本报告。未 push / dispatch / rerun / cancel / tag；未启停活守护进程（8787 只读一眼）；未碰 `~/.ruagent`（用 `%TEMP%\ruagent-t178\e2e-root`）。复核材料：`%TEMP%\ruagent-t178\`（`block.sh`（新块，去缩进）· `block-indented.sh` · `block-old.sh`（HEAD 抽出的旧块 `77747ef7…`）· `block-mutated-nomarker.sh`（C8 的负控）· `matrix.sh`+`cases/**`（8 格逐格产物）· `old-c2.sh`（before/after 对）· `playwright-evidence.json`（真实报告）· `compare.sh`（§5 的逐字节比对）· `panel-build.log`）。
