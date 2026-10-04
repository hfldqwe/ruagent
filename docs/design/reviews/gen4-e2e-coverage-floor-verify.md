# t174 独立验证 t166：E2E 被测量数下限（≥62）+ 5 个必跑 spec（mem-core，report-only）

- 被验证对象：`t166`（commit `3363bbf`，`feat(ci): the e2e evidence step counts what was measured…`）
- 本单 attempt `9a982872-d8e1-4f20-81a0-2b0878276e94`；我的**唯一写入**是本报告
- **判定：pass（两条守卫如作者所述成立），但带 1 条低危缺陷（休眠）+ 2 条边界表态 + 我自己的 2 处方法学翻车**

## 0 结论速览

| 维度 | 我的独立读数 | 作者自报 | 一致？ |
| --- | --- | --- | --- |
| 被测字节 = 提交字节 | 整文件 `097a1aad…`；`L297-399` 块 `87767506c09b4095…`；HEAD blob = `3363bbf` blob `f6c7c2bf…` | 同 | ✅ |
| 7 格矩阵 | **7/7 与声明一致**（外加我自加的 5 行「每个必跑条目单独红」+ 2 个形状） | 7/7 | ✅ |
| 本机 E2E | **exit 0 · `62 passed (15.1s)`**；JSON 两次读数 `measured=62 passed=62 failed=0 flaky=0 skipped=0`；逐 spec 16 个文件 = 12,12,7,6,5,4,4,3,2,1×7 | 62 同明细 | ✅ |
| 阈值来源 N=62 | **独立核对 3/6 个 run 的日志**：全 `success`、全 `62 passed` | 6 个 run | ✅（3 个） |
| 是否削弱既有守卫 | commit 纯插入 **104/0**，唯一 hunk `@@ -295,0 +296,104 @@`；L285/289/293 三条守卫**文本与行号未动**；`write-guard.ts`/其他 workflow 未碰；**24 处 spec 级合法跳过未改** | 同 | ✅ |
| **新发现** | **F-1（低，休眠）**：块的自检用的是 `stats_flaky`，不是逐 spec 的 flaky（greedy `sed` 命中 `stats_*` 孪生字段）⇒ 见 §3 | 未报 | ⚠️ |

## 1 C52：被测字节 = 提交字节（先做这一步，否则后面全是空气）

```
git cat-file blob HEAD:.github/workflows/e2e.yml        ⇒ sha256 097a1aad0e03359e… （418 行）
sed -n '297,399p' 同一份                                  ⇒ 103 行，sha256 87767506c09b409562652d8a704abf34bf836536128be43691de5680822621f6
git rev-parse HEAD:.github/workflows/e2e.yml vs 3363bbf:.github/workflows/e2e.yml ⇒ 同为 f6c7c2bf8f3d246792e8759ddd06042a55b0c847
```
⇒ 两个哈希**逐字等于作者申报**，且 HEAD 上的 blob 就是 t166 那次提交留下的 blob（之后没人动过它；`git status` 里只有同伴在途的 `chat.rs`/`index.css`）。

**注**：`git show HEAD --numstat` **不是** t166 —— HEAD 已经是 `ef68a48e`（另一条 daemon 加固）。t166 是 `3363bbf`；本报告中所有「这次提交改了什么」的结论都取 `3363bbf`。**这是我这轮第一个差点写错的地方**（用 HEAD 会得到一份毫不相关的 numstat）。

## 2 我自己复跑的矩阵（**不采信作者的读数**）

从已入库字节抽出块 → 去 YAML 的 10 空格缩进（**必需**：块内的 `<<'NODE'` heredoc 终止符必须在行首，带缩进时整个块会被当成 heredoc 体、一行逻辑都不执行 —— 我的第一版就踩了这个，见 §7）→ 得到可执行副本 `sha256 77747ef717fb00ab…`（缩进版 `87767506…` 才是 C52 要核的那份；两份哈希都在这里列出，避免「我跑的不是我核的」）。

每一格我都自己造 JSON 与 `skip_list`，读的是块自己的退出码与自己的 `::error::` 原文：

| 格 | 形状 | 退出码 | 块自己的原文（截断） |
| --- | --- | --- | --- |
| **G1** | 今日形状（62 measured，0 skipped） | **0** | `measured: 62 test(s) >= floor 62; must-run set (recall, consumption, failure-visibility, chat, settings-capabilities) all executed` |
| **R1** | 丢 8 个测量（54） | **1** | `only 54 test(s) were MEASURED, below the floor of 62 … does not cover 8 test(s) that the reference runs covered` |
| **R2** | 总量仍 62，但 `failure-visibility.spec.ts` 被跳过 | **1** | `failure-visibility.spec.ts was SKIPPED, but it is in the must-run set…` ⇒ **下限放行、集合点名**（互补性成立） |
| **G2** | 合法数据依赖跳过（`wiki.spec.ts`，总量仍 62） | **0** | 同上「all executed」⇒ **不误红** |
| **R3** | `stats` 与逐 spec 自相矛盾（逐 spec 62 / stats 40） | **1** | `the JSON report disagrees with itself: per-spec statuses say 62 measured … while stats says 40 …` |
| **R4** | 把下限调到 63（改的是**我那份副本**） | **1** | `only 62 test(s) were MEASURED, below the floor of 63 … does not cover 1 test(s)` ⇒ 下限是**活的** |
| **R5** | `chat.spec.ts` 被跳过（总量仍 62） | **1** | `chat.spec.ts was SKIPPED, but it is in the must-run set…` |
| **R6 ×5**（我加的） | 5 个必跑条目**各单独**进跳过名单 | 全 **1** | 5 条各自点名自己 ⇒ **没有死条目**（若某条名字写错/永不可能命中，这一行会绿） |
| **X1**（我加的） | 逐 spec 62 / stats 61（方向相反的自相矛盾） | **1** | 自检按设计报「disagrees with itself」 |
| **X2**（我加的） | 空报告（0 measured） | **1** | `only 0 test(s) were MEASURED, below the floor of 62 … does not cover 62 test(s)` ⇒ **「0 failed」不再是「0 measured」**（这正是本单价值所在） |
| **X3b**（我加的） | 逐 spec skipped=1 / stats.skipped=5，**测量总量相等** | **0** | 自检只看测量数，跳过量不影响判定（不误红） |
| **X4**（我加的） | 逐 spec 61 expected+1 flaky / stats 62 expected+0 flaky，**两边测量总量都=62** | **1** ⚠️ | 自检报 `per-spec statuses say 61 measured (passed=61 failed=0 flaky=0) while stats says 62` ⇒ **暴露 F-1** |

**必跑条目的名字形态也核了**（否则「点名」可能永远命中不了）：5 个文件名都存在；块的 `skip_list` 由上游 `${sp.file} :: ${sp.title} :: …` 生成（`sp.file` 是带路径的 spec 文件），而块用 `grep -q "$spec"` 以**裸文件名**匹配 ⇒ 子串命中成立，且 5 个名字互不构成彼此的别名。

## 3 新发现 F-1（低危，**今天休眠**）：自检读的是 `stats_flaky`，不是逐 spec 的 flaky

- **机制（读数，不是推断）**：块里
  `m_flaky=$(printf '%s\n' "$counts" | sed -n 's/.*flaky=\([0-9]*\).*/\1/p')` —— 而 `$counts` 一行里同时有 `flaky=` 与 `stats_flaky=`，`.*` 贪心 ⇒ 捕获到**后者**。（`m_skipped` 同理对 `stats_skipped`，只是它在今天的分支里不参与判定。）
- **证明（X4）**：造一份「逐 spec 61+1 flaky、stats 62+0 flaky，两边测量总量都 62」的报告 ⇒ 块自检打印 `… say 61 measured (passed=61 failed=0 flaky=0) while stats says 62` ⇒ **它用了 stats 的 flaky**（同一份报告里块自己上一行 `JSON_COUNTS` 明明写着 `flaky=1 measured=62`）。
- **影响**：今天**休眠**，因为它要求逐 spec 走查与 `stats` 在**字段级**不一致 —— 而我本机真报告实测 **FIELD-BY-FIELD differences: NONE**（`expected/unexpected/flaky/skipped` 四项逐字段相同）⇒ Playwright 今天不会产生这种报告。若哪天产生（例如某版本把 retry 记在别处），后果是**假红**（一条真的量了 62 的报告被判「自相矛盾」）与**诊断数字错**（消息里 `flaky=`/`skipped=` 来自 stats）。
- **建议判据/修法**（不属本单 inScope，登记）：把取值锚定到**行内独立 token**，例如
  `m_flaky=$(printf '%s\n' "$counts" | awk '{for(i=1;i<=NF;i++) if ($i ~ /^flaky=/) {sub(/^flaky=/,"",$i); print $i}}')`
  或给 per-spec 字段加不会被 `stats_` 前缀命中的名字（如 `walk_flaky=`）。**判据**：造一份「逐 spec flaky=1 / stats flaky=0、测量总量相等」的报告 ⇒ 今日**红**，修后必须**绿**（这条判据就是我 X4 那格）。

## 4 我自己跑的本机 E2E（仓库入口）

- 形状：**我自己的 root**（`%TEMP%\ruagent-t174\e2e-root`）+ **我自己的端口 8899** + **我自己记录的 pid**；`~/.ruagent` **一次未碰**；8787 只用 `Get-NetTCPConnection` 读了一眼（listener pid **14944**，未启停）。
  **为什么不能用 CI 的写法**：`panel/e2e/run-e2e.mjs:21` 的 `CI ? "http://127.0.0.1:8787" : ""` 意味着 **CI 环境变量会让本机跑指到活守护进程 8787**；而 CI 的 `Register e2e mock agents` 那步写的是 **`$HOME/.ruagent/config/agents.toml`**（在 runner 上是临时的，在本机就是操作者的真配置）。⇒ 我用 `RUAGENT_HOME=<temp root>` + 该 root 下的 `config/agents.toml`（mock agent 三条）+ `E2E_BASE_URL=http://127.0.0.1:8899`，并显式 `Remove-Item Env:\CI`。
- 命令与读数：`npm run test:e2e -- --reporter=list --output=<temp>`（**不是**裸 `npx playwright test`；入口打印 `e2e target: http://127.0.0.1:8899 (writes armed via RUAGENT_E2E_ALLOW_WRITES=1)`）⇒ **`NPM_E2E_EXIT=0`**、结尾 `62 passed (15.1s)`（退出码在任何管道之前取）。
- 报告 JSON（78,003 B）两次独立读数：`per-spec walk: measured=62 passed=62 failed=0 flaky=0 skipped=0 collected=62` / `stats block: expected=62 unexpected=0 flaky=0 skipped=0`；**逐 spec**：16 个文件 = 12,12,7,6,5,4,4,3,2,1×7 —— **与作者给的明细同多重集**。
- ⇒ **本机 62 独立复现**（作者报 62）。

## 5 有没有削弱既有守卫（纯字节）

- `git show --numstat 3363bbf`：`.github/workflows/e2e.yml` **104 / 0**（报告文件 131/0）⇒ **纯插入**，无删除、无改写；`ci.yml`/`audit.yml`/`release.yml`/`panel/**`/`scripts/**` **一次未碰**。
- 唯一 hunk：`@@ -295,0 +296,104 @@` ⇒ 插入点在**旧第 295 行之后**，即三条既有守卫之后 ⇒ 它们的**文本与行号**不可能被动到；逐字核对：`L285` unnamed-skip 的 `::error::`、`L289` registry 的 `::error::`、`L293`（`if [ "$rc" -ne 0 ]` 的 echo）**与作者描述一致**。
- `write-guard.ts` / `writeAccess()` / `RUAGENT_E2E_ALLOW_WRITES`：不在该 commit 的 numstat 里 ⇒ 未碰。
- 「24 处合法跳过」：`panel/e2e/**/*.spec.ts` 里 `test.skip(` = **24**（我按文件数过：3+2+1+5+2+1+3+1+1+1+4 = 24）✓；整目录 grep 得 25，多出的 1 处是 **`write-guard.ts:34` 的一句注释**（说明性文字，不是跳过点）⇒ 与作者的 24 **对得上**，且该文件未被本 commit 触碰。

## 6 阈值 N=62 的来源（C47 口径：至少独立核对 2–3 个 run）

`gh run view <id> --log`（`NO_PROXY='*'`）实测：

| run | conclusion | headSha（互不相同 ⇒ 确为 6 次不同提交） | 日志里的 E2E 读数 |
| --- | --- | --- | --- |
| `37138197234` | **success** | `84e62942…` | `62 passed (45.5s)` + 证据步 `(none: no spec was skipped)` |
| `37194596042` | **success** | `262ba8e7…` | `62 passed (36.1s)` |
| `37196424418` | **success** | `3d5468d6…` | `62 passed (45.6s)` |

⇒ 「六次连续成功 run 都 62 passed」这一**类**读数成立（我核了 3 个；另外 3 个没核，见 §8）。**没有**把作者的列表当读数。

## 7 两条作者自报边界，我的表态

1. **新守卫还没在 CI 上跑过 ⇒ 这是「未覆盖」**，不是「已覆盖」。我能证的是：块在**抽出来之后**对 12 种形状的行为、以及本机 62。**未覆盖的是**：它在真实 runner 上读到的 `$json`/`$skip_list` 与真实 `PLAYWRIGHT_JSON_OUTPUT_FILE` 的耦合（那些上游变量的形状我只能读源码推断）。覆盖它需要一次 main push（本单禁止）。
2. **本机 E2E 不会执行证据步 —— 确认**：证据步是 **workflow 的一步**（`E2E evidence (spec counts + exit code, skips named)`），Playwright 套件里没有它；本机 `npm run test:e2e` 只跑 spec。⇒ 本机 62 是**套件**读数，CI 62 是**套件 + 证据步**读数；两者是「同一人群的两次独立测量」，**不是同一个读数被用了两遍**。我这轮把这两件事分开取证：本机跑套件（§4），块则在 §2 用抽出的字节单独复跑（含 CI 才会走的自检/下限分支）。

## 8 我没有验证什么（第 19 条）

- **run 只核了 3/6**（`37138197234`/`37194596042`/`37196424418`）；`37139111120`/`37141299806`/`37192592208` **未核**。
- **没有在另一个平台核对 62**：只有本机 Windows + 3 个 ubuntu runner 的日志；windows 上的 62 未测（该 workflow 只 ubuntu）。
- 矩阵的 12 格**全部由我自跑**（不是抽样），但它们是我**合成的 JSON**；我**没有**在真跑里制造一次真实跳过（那需要在 runner 上改 spec，禁止）⇒ 「真跳过会被点名」这一步是**按代码语义**证的，不是按真事件证的。
- 我跑的是**去缩进副本**（`77747ef7…`）；缩进原样（C52 的 `87767506…`）**不能直接执行**（heredoc 终止符），这一点在报告里写明，以免下一位以为两者可互换。
- **没有** push / dispatch / rerun / cancel / tag；没有动 `.github/**`、`panel/**`、`crates/**`、`scripts/**`、`tools/**` 的任何字节（本报告是唯一写入）。
- 我**没有**复跑 t166 报告里的其它条目（例：它关于 `list` reporter 不给名字的论证、`PW_TEST_REPORTER` 与 `--reporter=json` 的对照）—— 那些不在本单契约里。

## 9 我自己的两处翻车（方法学，留给下一位）

1. **heredoc + 缩进**：第一次把块按原样（10 空格缩进）当脚本跑 ⇒ `<<'NODE'` 的终止符不在行首 ⇒ **整块变成 heredoc 体，12 格全红**（含本该绿的 G1）。⇒ **「全红」先怀疑自己**：这里正是「一个读数先看它能不能自洽」的一次实例。
2. **TOML 反斜杠**：我照搬 CI 的 `agents.toml` 写法，但 CI 的路径是 POSIX；本机 Windows 路径写进 TOML 双引号串触发 `too few unicode value digits`（`\U` 被当转义）⇒ 守护进程起不来（健康检查 60s 失败）。改成**正斜杠路径**后 7 s 即健康。⇒ 这是「把 CI 的形状搬到本机」的经典坑，`run-e2e.mjs` 的注释其实已经把这类坑记了一处。

## 10 残留与清理

- 我的写入：**只有本报告**。`git status` 里 `chat.rs`/`index.css` 是**同伴**在途编辑。
- 我自己起的守护进程：pid **27828**，已用 `Stop-Process -Id`（**只按我记录的 pid**）停掉并确认 stopped；上一轮失败的 pid 37488 自己已退出（也确认过）。8787 的活守护进程（pid 14944）**未启停**。
- 临时物：`%TEMP%\ruagent-t174\`（`e2e-head.yml` · `block.sh`（C52 哈希对象）· `block-runnable.sh`（去缩进）· `matrix.sh`+`x3.sh`+`x4.sh`（矩阵与两个额外形状）· `cases/**`（12 格逐格产物）· `playwright-evidence.json`（78,003 B）· `e2e-run.log` · `daemon.*.log` · `daemon.pid`）；`e2e-root`（我的临时 root）。需要时按**具体路径**删；重跑前请先确认 8899 空闲（我停掉后为空闲）。
