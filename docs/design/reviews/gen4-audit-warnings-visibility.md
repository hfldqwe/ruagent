# 审计 disclosure 进 CI 载体 + 「既无 check 也无理由」的行号集变化即红（t188）

> 单号 **t188**（repair）· 成员 `recall` · attempt 1 · 2026-10-04 20:3x–21:0x
> **inScope 写入**：`.github/workflows/audit.yml`（**146 增 / 2 删**）+ 本报告。
> **零改动**：`panel/tools/**`（工具一行未动，C69 见 §6）· `panel/src/**` · `panel/e2e/**` · `crates/**` · `scripts/**` · `.github/workflows/{ci,e2e}.yml`。
> 判据原文的「来源」= run **`37200876216`**（head **`bb2d99ad`**，`Design audit (§12, live DOM)`，conclusion success）的 artifact `design-audit`，我**自己下载后量的**（不转述）。

## 0 结论

| 项 | 读数 |
| --- | --- |
| **缺口量证** | artifact 的 `counts.env` 只有 10 个键、**没有 `warn=`/`blind=`**；而 `metrics.json` 里 **7 条 `warnings[]`** 与 **6 条 `blindSpots[]`**（含 12 行「既无 check 也无理由」）**没有任何载体** |
| **改了什么** | `counts.env` 新增 5 键：`warn` / `blind` / `no_reason_raw` / `warn_sig` / `blind_items`；新增人读载体 `disclosures.md`（逐字列出每条 warning 与每条 blindSpot，**同时进 step summary 并与 artifact 一起上传**）；新增 **③④ 两条红线** |
| **红线机制** | ③：`§12 rows with neither a check nor a stated reason` 的**行号集合**（`60…71`）**移动即红**，**空集也红**（与既有 `observed_raw` 钉法同款：集合是期望值，不是备注）；④：**其余 warning 集合**（逐字、排序、`|` 连接）变化即红 |
| **本地跑通** | 把该步 shell **从 workflow 抽出**（PyYAML 去缩进，`sha256 106bf0205c900a23…`，347 行）对**同一份 CI artifact** 跑：**rc=0**，`counts.env` 948 B（逐行原文见 §2）|
| **负控（6 格全对）** | 原始 artifact ⇒ **0**；no-reason 集合**少一行** ⇒ **1**（点名）；**多一条 warning** ⇒ **1**（点名）；no-reason warning **整条消失** ⇒ **1**（`got '#'`）；**删一条 blindSpot ⇒ 仍 0**（有意为之的边界格）；恢复 ⇒ **0** |
| **未削弱** | 计数仍取自 `metrics.json` · 两载体交叉核对仍在 · `observed_raw` 钉法逐字保留且仍被 ② 使用 · 仍拒绝 `--allow-not-measured` · **既有 `::error::` 文案一条未改** · `git diff --numstat` = **146/2**，两条删除逐字列出并解释（§5） |
| **门禁** | `bash .github/workflows/scripts/check-workflow-refs.sh` **exit 0**（22 refs / 四份 workflow 均 `parses as YAML (PyYAML)`） |

## 1 缺口：我自己从 artifact 量的（判据要求「不许转述」）

**① `warnings[]`：7 条**（逐字，顺序即数组顺序）：

```
1. #task skipped: could not resolve a task id (/api/v1/tasks empty or unreachable)
2. chat/dark: hash mismatch: asked #chat, got #chat?agent=claude
3. graph/dark: no content marker found (.graph-canvas)
4. chat/light: hash mismatch: asked #chat, got #chat?agent=claude
5. graph/light: no content marker found (.graph-canvas)
6. §12 rows with neither a check nor a stated reason: 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71
7. .nav-badge 本轮未渲染（inboxCount = 0）：它对白字 3.20:1 的对比度失败对本次审计不可见 —— 复测该行前必须先造出一条待处理权限
```

**② `blindSpots[]`：6 条**（每条形如 `{item, reason}`）：

| # | item | reason |
| --- | --- | --- |
| 1 | `.nav-badge` | 仅在 inboxCount > 0 时渲染；本轮 13 路由实测 0 个 |
| 2 | `::selection` 的字/底对比度 | 不在 §5.3 下限的判据范围（primitives 附录 A.5 登记为未测） |
| 3 | `:hover` 面上的文本对比度 | 同上，静态截图与计算样式都取不到 hover 面 |
| 4 | 动效中间帧 | 同上 |
| 5 | `.chat-thought / .md blockquote` 的装饰竖线 | 装饰线无下限要求（§5.3） |
| 6 | 暗色 tag 底色（primitives A.15 估计 3.99-4.23，未核定） | 本脚本按计算样式的实际合成值测，不做估计；若 tag 底色是 antd 派生值，需逐变体核对 |

**③ 行级缺席那一类**（「既无 check 也无理由」）：**行号 = `60 61 62 63 64 65 66 67 68 69 70 71`，共 12 行** —— 与 captain 转述的原文**逐字一致**（我按「warning[6] 截取最后一个 `:` 之后的数字并按数值排序」得到，机械提取而非照抄）。与 workflow 自己 L336-347 的注释也一致：§12 有 77 行连续编号、工具硬编码 65 条 check、`77 − 12 = 65`。

**④ artifact 的 `counts.env` 原文**（`.../artifact/counts.env`，**145 B**，前 4 字节 `99,97,112,116` ⇒ **ASCII/UTF-8 文本，不是 UTF-16LE**，C68 已核）：

```
captures='24'
checks='65'
pass='52'
fail='3'
unmeas='10'
pending='0'
judged='65'
failing='6, 7, 12'
observed_raw='25 28 31 33 46 47 49 50 54 77'
```

⇒ **没有任何 `warn=` / `blind=`**，`warnings[]`/`blindSpots[]` 在 CI 载体里**不存在**。一个只看 CI 的读者因此**无法**知道：12 行根本不在清单里、`#graph` 两档被跳过、`.nav-badge` 的对比度失败这次不可见。**这就是本单要补的第二个层次**（② 钉的是「试过但没测到」，本单钉的是「**整行缺席**」）。

## 2 改了什么（只改 workflow）

**(a) 机器载体**：`counts.cjs` 在**原有 9 个键一字不改**的前提下追加 5 个键。绿灯实测（`.../run/G-pristine/counts.env`，948 B）：

```
captures='24'
checks='65'
pass='52'
fail='3'
unmeas='10'
pending='0'
judged='65'
failing='6, 7, 12'
observed_raw='25 28 31 33 46 47 49 50 54 77'
warn='7'
blind='6'
no_reason_raw='60 61 62 63 64 65 66 67 68 69 70 71'
warn_sig='#task skipped: could not resolve a task id (/api/v1/tasks empty or unreachable) | .nav-badge 本轮未渲染（inboxCount = 0）：它对白字 3.20:1 的对比度失败对本次审计不可见 —— 复测该行前必须先造出一条待处理权限 | chat/dark: hash mismatch: asked #chat, got #chat?agent=claude | chat/light: hash mismatch: asked #chat, got #chat?agent=claude | graph/dark: no content marker found (.graph-canvas) | graph/light: no content marker found (.graph-canvas)'
blind_items='.chat-thought / .md blockquote 的装饰竖线 | .nav-badge | ::selection 的字/底对比度 | :hover 面上的文本对比度 | 动效中间帧 | 暗色 tag 底色（primitives A.15 估计 3.99-4.23，未核定）'
```

**(b) 人读载体**：`disclosures.md`（由同一个 `counts.cjs` 从 `metrics.json` 逐字生成，**不转述**），被 append 进 `$GITHUB_STEP_SUMMARY` **并且**列入 artifact 上传路径。绿灯实测的 summary 片段（2964 B）：

```
### Coverage declarations the instrument itself made (t188)

- warnings: **7** (`warn=7`)
- blind spots: **6** (`blind=6`)

Every warning, verbatim:
- `#task skipped: could not resolve a task id (/api/v1/tasks empty or unreachable)`
- `chat/dark: hash mismatch: asked #chat, got #chat?agent=claude`
- `graph/dark: no content marker found (.graph-canvas)`
- `chat/light: hash mismatch: asked #chat, got #chat?agent=claude`
- `graph/light: no content marker found (.graph-canvas)`
- `§12 rows with neither a check nor a stated reason: 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71`
- `.nav-badge 本轮未渲染（inboxCount = 0）：…`
…
- rows with **neither a check nor a stated reason** (silent skips, pinned by guardrail ③): `#60 61 62 63 64 65 66 67 68 69 70 71`
```

**(c) ③ 行级缺席集合是期望值**（pin 值来自 §1③，来源写进注释：run `37200876216` head `bb2d99ad`）：

```bash
EXPECTED_NO_REASON="60 61 62 63 64 65 66 67 68 69 70 71"
got_nr=$(norm "$no_reason_raw"); want_nr=$(norm "$EXPECTED_NO_REASON")
if [ "$got_nr" != "$want_nr" ]; then … ::error::… exit 1; fi
```
**空集也红**（`norm ""` ⇒ `""` ≠ pin ⇒ 红）：warning 整条消失 = 仪器不再报告这一类，不是「没有缺席行」。

**(d) ④ 其余 warning 集合是期望值**（`EXPECTED_WARN_SIG`，6 条逐字 `|` 连接）。**「一个原因一个红」的设计**：签名**排除** no-reason 那条 warning（它由 ③ 从 `no_reason_raw` 管**内容与存在性**）⇒ 行号列表变化**只**触发 ③，不会同时触发 ③④。签名是**逐字**的（不做数字归一化）：一条 warning 被改写、增加或消失，都是「仪器对自己覆盖面的声明变了」，都该吵醒人。

## 3 本地跑通 + 负控（判据要求「不许只靠 CI 试」）

**方法**：`harness.py` 用 PyYAML 从 `.github/workflows/audit.yml` 里取出那个 guard 步的 `run`（YAML 块标量在这里自动完成去缩进 ⇒ **bash 执行的字节 = workflow 携带的字节**，`sha256 106bf0205c900a23…`，347 行），然后在 `%TEMP%\t188\run\<case>\` 下造一个**独立的 `$RUNNER_TEMP`**（`audit.log` / `audit.exit` / `audit/metrics.json` 全部来自 artifact），用 `RUNNER_TEMP`+`GITHUB_STEP_SUMMARY` 指向它跑。**共享树零变异**：所有变异都在 `%TEMP%` 里那份 `metrics.json` 副本上。

```
G pristine artefact                                    rc=0 want=0 OK
N1 no-reason set loses one row                         rc=1 want=1 OK
N2 one extra warning                                   rc=1 want=1 OK
N3 the no-reason warning disappears                    rc=1 want=1 OK
N4 one blind spot removed (advisory: must stay GREEN)  rc=0 want=0 OK
G2 pristine again (restored)                           rc=0 want=0 OK
=== every case behaved as expected: True ===
```

**两条红的原文**（`::error::`，逐字）：

```
N1（把 §12 行号列表里的 71 去掉）:
  ::error::the set of §12 rows with neither a check nor a stated reason moved: expected
  '#60 61 62 63 64 65 66 67 68 69 70 71', got '#60 61 62 63 64 65 66 67 68 69 70'
N2（追加一条不存在的 warning: "t188 negative control: an extra warning …"）:
  ::error::the instrument's warning signature moved: expected 6 pinned entries, got a
  signature with 7 entries. Read disclosures.md (uploaded) and diff it against the pin
  in .github/workflows/audit.yml.
N3（整条 no-reason warning 消失）:
  ::error::the set of §12 rows with neither a check nor a stated reason moved:
  expected '#60 61 62 63 64 65 66 67 68 69 70 71', got '#'
```

**N4 是边界格而不是遗漏**：删掉一条 `blindSpot` 仍 **rc=0**。**为什么故意不 pin**：`blindSpots[]` 是工具**自己声明的「测不到什么」的完整清单**，本单把它**逐字印出来**（summary + `disclosures.md` + `blind=N` + `blind_items`），但**不把它做成红线** —— 这条 pin 我**无法在 CI 上复核**（我不 push），而一条我复核不了的 pin 会把**下一次**审计 run 变红，代价由下一个读它的人承担。workflow 里的 summary 行把这条边界写明了（「NOT pinned -- see the report's stated boundary」）。t141 对「§12 行数」也做过同样的取舍（stated boundary 而不是第二个手工常数）。

**恢复即绿**：G2 与 G 用**同一份**原始 artifact（`%TEMP%` 里的变异副本被覆盖重建）⇒ `rc=0`、`counts.env` 与上表逐字一致。

## 4 本地跑通抓到的**真 bug**（说明这一步不是形式主义）

第一次跑 G（原始 artifact）**rc=2**，报错：

```
<...>/counts.env: line 19: unexpected EOF while looking for matching `''
```

根因**在我新加的代码里**：我把 `counts.cjs` 的调用写成 `node counts.cjs "$METRICS" "$COUNTS" "$DISCLOSURES"`，而脚本里的 disclosure 路径读的是 **`process.argv[3]`** —— 那是 `$COUNTS`，不是 `$DISCLOSURES`（`argv[2]` 才是 `$METRICS`）⇒ 于是它把 **markdown 写进了 `counts.env`**，接着 `. "$COUNTS"` 去 source 一段 markdown，撞上未闭合的引号 ⇒ rc=2。

**修法**：调用改成 `node counts.cjs "$METRICS" "$DISCLOSURES" > "$COUNTS"`（counts 本来就来自 stdout，那个多余的 `$COUNTS` 参数从来不需要）⇒ `argv[2]`=metrics、`argv[3]`=disclosures ✓。修完 6 格全绿。**若只在 CI 上试**，这一条会表现为「护栏红在一个没人预料的地方」；顺带说明为什么脚本把 disclosure **写在 stdout 之前**（写不出 ⇒ `counts.env` 为空 ⇒ ① 红），以及 summary 那段 `[ -s "$DISCLOSURES" ]` 的兜底（缺失即红，不是安静放过）。

## 5 没有削弱既有护栏（逐条可核）

| 既有护栏 | 现状 |
| --- | --- |
| 计数取自 `metrics.json` | 未变（`counts.cjs` 仍以 argv[2] 的 metrics.json 为唯一来源；stdout 只作为 **交叉核对**的第二个载体） |
| 两载体交叉核对、不一致即红 | 未变（`if [ "$s_captures" != "$captures" ] \|\| … ⇒ ::error::the two carriers disagree … exit 1` **原文未改**） |
| `observed_raw` 钉法逐字保留 | 未变（`EXPECTED_NOT_MEASURED="25 28 31 33 46 47 49 50 54 77"` 与 `got=$(norm "$observed_raw")` / ② 的 `::error::not_measured set moved: …` **逐字保留**；新的 `observed_raw=` 键仍由 `counts.cjs` 发出） |
| 仍拒绝 `--allow-not-measured` | 未变（两处提到它的注释保留了「不许一次豁免全部未测量行」的理由） |
| 既有 `::error::` 文案 | **一条未改**（新增的 ③④ 文案是新加的；`git diff` 的两条删除见下） |
| `captures` 不 pin / `judged==65` 的 pin | 未变（`if [ "$judged" != "65" ]` 与其解释性长文案原文保留） |

**diff = `146 增 / 2 删`，两条删除逐字列出并解释**：

```
-            node "$RUNNER_TEMP/counts.cjs" "$METRICS" > "$COUNTS" 2>"$RUNNER_TEMP/counts.err" || true
-            echo '```'
```
① 第一条 = §4 那个 argv 修正（同一行被替换成新的调用 + 5 行说明）；② 第二条 = advisory 汇总块的**第一行代码围栏**：我插入 ③④ 时它被移动了位置，**内容逐字重新加回**（绿灯 summary 里能看到 `### design-audit --check (advisory)` + 围栏 + `[audit] 24 captures in 593.2s · checks 52 pass / 3 fail / 10 not measured` 全都在），所以这是**位置变化**而非内容删除。

## 6 边界与未覆盖（点名）

1. **只让披露可见，不修任何产品侧/判据侧问题**：那 **12 行**（60-71）**没修**；`#graph` 两档的 `.graph-canvas` **没修**；`.nav-badge` 的对比度失败**没修**（产品侧，另立单）；**65 条 check 的判据一条未动**（本单不碰 `panel/tools/**`）。
2. **不改工具（C69）**：`panel/tools/**` **零改动** ⇒ 不需要「改前改后哈希 + 语义未变」那套声明；本单的判定语义**完全在 workflow 里**（新增的红线只读 `metrics.json` 已有的字段）。
3. **未在 CI 上验证**（我不 push）⇒ 不存在「CI 上绿」的读数。我的证据是：**同一份真实 artifact 上本地跑通** + 6 格负控 + 门禁 exit 0。
4. **本地 shell 与 CI shell 的实现差异**（如实记）：本地是 git-bash（GNU coreutils/sed/tr/wc），CI 是 ubuntu 的 bash（同为 GNU）⇒ 我用到的都是 POSIX/GNU 共同行为（`sed`/`tr`/`wc`/`printf`/`[`），但**这不是同一台 shell 的同一份字节**，所以「CI 上会怎样」仍属未验证。
5. **pin 的已知后果（必须让下一个读它的人知道）**：`EXPECTED_WARN_SIG` 与 `EXPECTED_NO_REASON` 是**按 run `37200876216`（head `bb2d99ad`）的 fixture 校准**的。若下一次审计 run 因为**fixture 变化**合法地增删了一条 warning（例如 `#task skipped` 因为 root 里终于有 task 而消失、或 `inboxCount > 0` 让 `.nav-badge` 出现），这一步**会红** —— 那正是设计意图（有人必须看一眼），更新方式写在红的那条 `::error::` 里：读本次 run 的 `disclosures.md`（已上传）并就地更新 pin。**我没有把这个后果藏起来**。
6. **`blindSpots` 故意不 pin**（理由见 §3 的 N4 格）；它只进载体与 summary。
7. **unit/集成层的 `#[ignore]`**（B33/B34/B35 那条同族）**不在本单**：那是 crate 侧忽略清单，本单只做审计侧；本单不碰 `crates/**`。
8. **`negctl`（负控目录）**：既有上传路径里有 `${{ runner.temp }}/negctl`，本单**没有**在 CI 里新增负控步骤（我的负控是**本地**在 `%TEMP%` 副本上做的），所以那一条上传路径的用途未被本单改变，也未使用。

## 7 纪律回执

- **写入集合**：`.github/workflows/audit.yml`（inScope，`git diff --numstat` = **146/2**）+ 本报告（inScope，未跟踪新文件）—— **没有第三个文件**。
  **不是我的、我一行未动**（提交时 `git status` 的实况）：`panel/src/App.tsx` · `panel/src/index.css` · `panel/src/views/Settings.tsx` · `panel/tools/design-audit.mjs`（sha256 `99F9ED14B36E2D2F…`，**同伴在途**）· `docs/design/reviews/gen4-release-gate-coverage.md`（同伴的报告）。
  **本会话我早先两个单的产物（t164 的三个 spec + `scripts/ruagent-daemon.ps1`、t166 的 `.github/workflows/e2e.yml`）此刻已不在 `git status` 里** ⇒ 已被落地/提交，**不计入本单的写入集合**，也不作为本单的读数依据。
  **工具侧与我无关的旁证（C69）**：`panel/tools/design-audit.mjs` 是**别人正在改**的文件；本单**没碰它**，而且我的所有读数都取自 **run `37200876216` 的 artifact**（下载来的 `metrics.json`/`counts.env`），**不是**工作树里那份正在变的工具产出的 ⇒ 同伴的在途编辑不会影响本单的任何结论。
- **负控不在共享树**：全部在 `%TEMP%\t188\run\*\` 的 `metrics.json` 副本上（`git diff` 里只有我的两个 inScope 路径）✓。
- **没起真守护进程、没碰 8787（B29）**：本单**不需要**浏览器/守护进程；artifact 是**下载**来的 ✓。收尾核对 `8787` 归属未变。
- **编码（C68）**：所有读取都先验编码 —— artifact 的 `counts.env` 首 4 字节 `99,97,112,116`（文本，非 UTF-16LE）；我本地产生的 `counts.env`（948 B）/`disclosures.md`（1445 B）/`summary.md`（2964 B）同样验了首字节；没有再犯「PS 5.1 `>` 写 UTF-16LE」那类错（Python 全程显式 `encoding="utf-8"`，harness 读文件用 `errors="replace"` 以防半截写）。
- **`NO_PROXY`**：`gh` 用 `'*'`（下载 artifact）；本单**没有**跑 Rust/curl ⇒ 两个值没有混用。
- **收尾按具体路径删**（见下），证据文件按名保留。
- 未 push / 未 dispatch / 未 rerun；未改工具；未改判据。
