# gen3 CI job 超时上界：让「挂住」与「慢」可区分（t111）

> 单号 t111（work）· 成员 `wiki` · attempt 1 · 2026-09-29 09:3x–10:0x（+08:00）
> **§8 由 t114 追加**（同一作者，2026-09-29 10:4x–11:1x）：本报告因此覆盖 **6 个 job**（ci 3 + e2e 1 + release 2）。
> inScope（t111）：`.github/workflows/ci.yml` · `.github/workflows/e2e.yml` · 本报告。**未碰** `crates/` · `panel/` · `.github/workflows/release.yml`。
> inScope（t114，§8）：`.github/workflows/release.yml` · 本报告。**未碰** `ci.yml` · `e2e.yml` · `crates/` · `panel/`。
> 纪律：**未** push / dispatch / rerun / cancel / 建 tag；读数只来自 `gh run list/view/--json`（只读）。

## 0 一句话

给四个 job 各加一条 `timeout-minutes`（形状照 `release.yml:57` 的 gate job）：`Rust (ubuntu)` **30** · `Rust (windows)` **60** · `Panel (node)` **15** · e2e `Daemon + doctor + panel smoke` **30**。这些值不是为了跑得快 —— 是为了把「**挂住**」从 6 小时的静默 `in_progress` 变成**有界的失败**，而取值全部由实测耗时（`gh run view --json jobs` 的 `startedAt`→`completedAt`）+ 结构性余量推出，不是拍脑袋。

## 1 现场（改前读数）

```
HEAD:.github/workflows/ci.yml  {'rust-linux': None, 'rust-windows': None, 'panel': None}
HEAD:.github/workflows/e2e.yml {'smoke': None}
HEAD:.github/workflows/release.yml  gate=20, build=None, release=None
```

⇒ 三份 workflow 里**只有** `release.yml` 的 gate job 有上界（`timeout-minutes: 20`，我在 t100 加的）；`ci.yml` 三个 job 与 `e2e.yml` 唯一 job **一个都没有** ⇒ 全部吃 **GitHub 默认 360 分钟**。

**这条不是洁癖，有一个本轮的真实案例**（也是本单的来源）：run **36508184462**（2026-09-29T01:29:58Z，sha `1516814e`）——

| job | startedAt | completedAt | 结果 |
| --- | --- | --- | --- |
| `Panel (node)` | 01:30:02 | 01:30:33 | success（31s） |
| `Rust (ubuntu)` | 01:30:02 | 01:34:23 | success（261s） |
| **`Rust (windows)`** | 01:30:02 | **01:37:00** | **cancelled（418s）** |

它在等 windows 判定时**没有任何上界可比对**：「windows 慢」与「windows 挂了」在读数上不可区分；最终结束它的**不是上界**，而是**后一次推送的 run 级 concurrency 把它取消**。若当时真挂住且没有后续推送，这个 job 会占据 runner 最长 **6 小时**，run 一直显示 `in_progress` —— 与「正在正常工作」逐字相同。

## 2 实测耗时表（原始数字，`gh run view --json jobs`，`startedAt`→`completedAt`）

**`ci.yml`（最近 10 个 run + 09-26 的 12 个 run，秒）**

| job | 逐次观测（秒） | 最小 | **最大** |
| --- | --- | --- | --- |
| `Rust (ubuntu)` | 25 · 57 · 61 · 62 · 63 · 66 · 84 · 89 · 230 · 238 · 245 · 261 · 263 · 263 · **542** | 25（早退的红） | **542（9.0 分钟，第一个绿 run 36507120586）** |
| `Rust (windows)` | 25 · 34 · 133 · 143 · 238 · 242 · 415 · 418 · 446 · 447 · 467 · 473 · 480 · 526 · **552** · 616 · 710 · 846 · 846 · **1310** | 25（被取消） | **1310（21.8 分钟，run 36486678888）** |
| `Panel (node)` | 23 · 26 · 28 · 30 · 30 · 31 · 32 · 32 · 33 · 33 · 34 · 34 · 34 · 35 · 36 · 37 | 23 | **37** |

**`e2e.yml`（smoke job，最近 8 个 run）**：222 · 224 · 460 · 480 · 494 · 499 ⇒ 最小 222，**最大 499（8.3 分钟）**（222/224 是绿；460–499 是带失败细节的那些）。

**一个必须写出来的旁注**：run `36507120586`（09-29T01:16:30Z）是**第一个绿 run**：ubuntu **542s** · windows **552s** · panel **35s**（本代第一次全绿；红的那条链里 ubuntu 只有 25–265s，因为 `Format`/`Clippy` 一红后面就 `skipped` 了）。⇒ **上界必须按「绿」的读数取，不能按红的**。

## 3 取值与理由（每条 = 实测最坏 × 余量 + 不误杀的论证）

| job | 值 | 依据 |
| --- | --- | --- |
| `Rust (ubuntu)` | **30 min** | 最坏 **542s（9.0 min）**×3.3。红 run 25–265s ⇒ 正常路径离 30 分钟有 3 倍以上余量；余量留给**冷 rust-cache**（缓存恢复 40–44s，未命中则要整棵 workspace 重编译，叠在 9 分钟的绿工作量上）。 |
| `Rust (windows)` | **60 min** | 最坏 **1310s（21.8 min）**，而**这一个数已经包含了一整趟测试**；`Test attempt 2 (only for the known build race)` 在上游非测试级失败时**再跑一趟近似的测试**，冷缓存还要再加整棵重编译 ⇒ 21.8 × 2.7 ≈ 60 min（算术：冷编译 ~10 min + attempt 1 ~13 min + attempt 2 ~13 min ≈ 36 min < 60）。**波动极大（25s…1310s，52×）⇒ 取上界的方法只能是「历史最大值 + 结构性补足 + 向上取整」，绝不能用中位数。** 宁可宽：误杀一个真慢的 windows run，代价比多等 20 分钟大得多。 |
| `Panel (node)` | **15 min** | 最坏 **37s**×24。这里**故意不按 ×3 取**（37×3=111s≈2 min）：该 job 唯一没有本地读数可测的部分是**冷 runner 上的 `npm ci`（网络）**，而上界存在的理由是「把挂住变成有界的失败」，不是「把安装逼快」。取 15 min 作为**下限地板**：既不会误杀真慢的安装，又把挂住从 6 小时压到 15 分钟。 |
| e2e `smoke` | **30 min** | 最坏 **499s（8.3 min）**×3.6。这个 job 里慢的腿是守护进程启动 + `doctor` + 面板构建 + 整套 Playwright（t110 之后又多了一条 JSON 点名判定）：任何一条腿挂住（含浏览器挂住）都会在 30 分钟变成有界失败。 |

**「不会误杀正常路径」的统一论证**：四个值都 ≥ 观测最大值的 3 倍（panel 例外是 **24 倍**，且理由是该 job 的不可测部分是网络安装）；四个值合计 135 分钟，仍低于默认 360 分钟的单 job；**红路径不受影响**（红 run 的耗时**更短**：ubuntu 25–265s、windows 238–1310s 里最长的那个本来也是失败 run，但它的失败点在测试内部而非超时）。

## 4 判据：改后「挂住」与「慢」怎么区分

- **改前**：挂住的 job 会一直 `in_progress`，直到 360 分钟默认值或**下一次推送的 concurrency 取消**（run 36508184462 就是这样被结束的）⇒ 读者无法判断"它在跑"还是"它死了"。
- **改后**：到上界 ⇒ 该 job 置 **failure**（`The job running on runner … has exceeded the maximum execution time of N minutes`），run 结束为 failure，**点名的对象是「哪个 job 超时了」**，不再有无界等待。
- **能力边界（第 19 条，写明）**：`timeout-minutes` **只**把挂住变成有界失败，**不会**指出挂在**哪个测试**（例如 windows 的 `cargo test` 里某一条卡死，只会看到 `Test attempt 1` 这个 step 超时）。要更强的可判定性必须**另立单**（`cargo test` 侧 per-test harness 超时、或把长测试标记/分片），**本单没有顺手实现** —— 那会把一个小单扩成不可评审的改动。

## 5 门禁（改动后的读数）

| 门禁 | 读数 |
| --- | --- |
| 策略检查（PyYAML 读回四个 job 的值 + 其余 job 未变） | `PASS ci.yml :: rust-linux=30` · `PASS rust-windows=60` · `PASS panel=15` · `PASS e2e.yml :: smoke=30`；三份汇总 `ci.yml {rust-linux:30, rust-windows:60, panel:15}` · `e2e.yml {smoke:30}` · `release.yml {gate:20, build:None, release:None}`（**release.yml 未被我改动**）⇒ `POLICY OK` |
| YAML 可解析性 | `.github/workflows/{ci,e2e,release}.yml` 三份均 `parses as YAML (PyYAML)`（由 guard 与我自己的 PyYAML 各打一次） |
| `bash .github/workflows/scripts/check-workflow-refs.sh`（t106 加固版：refs 必须被跟踪 + 表达式合法性 + 三份 workflow 可解析） | **exit=0**：`workflow path references checked: 15, not tracked/missing: 0` + `every executed path a workflow references is tracked` + 三行 `parses as YAML` |
| `git diff --stat` | `.github/workflows/ci.yml \| 20 ++++++` · `.github/workflows/e2e.yml \| 6 +++` ⇒ **2 files changed, 26 insertions(+), 0 deletions(-)**（只有四条 key + 说明性注释，未动任何 step） |

## 6 不覆盖什么（第 19 条）

1. **不指出挂在哪个测试**：见 §4 的能力边界；per-test harness 超时 / 长测试标记 / 分片都必须另立单，本单不做。
2. **不改 `release.yml`**（inScope 外）：它的 `build`/`release` 两个 job 至今**仍无 `timeout-minutes`**（`{gate:20, build:None, release:None}`）—— 本单只登记，交其属主（发布路径的 owner）决定。
3. **不覆盖 `design-audit.yml`**（t99 的 inScope，该文件目前**不存在**）。
4. **不能在本机复现「超时真的触发」**：要看到那条 failure 需要一次真实运行（本单**不许** push/dispatch/rerun），所以「值是否合适」只能由**历史实测耗时**论证；**改后第一次 CI 运行**会给出第一批真实读数（尤其是 windows 的 60 分钟是否真的宽松到不误杀）。
5. **不覆盖「步内挂住」的可观测性**：一个 step 超时后，日志里最后一行仍是它挂之前的输出 —— 定位到测试要靠日志尾部人工读或另立单的 per-test 超时。
6. **不改变速度/成本**：正常路径的耗时与 runner 分钟数**一字不变**；只有挂住的 run 会提前结束（对成本是减少）。
7. **不覆盖缓存的冷/热分层**：我只用「历史最大值 + 冷缓存余量」论证，没有真的把 rust-cache 清掉跑一次冷启动（那要 push 或 dispatch）。
8. **不覆盖 runner 侧的平台差异**（windows-latest/ubuntu-latest 镜像更新可能改变耗时；上界按当前观测取，未来漂移需要重新标定）。

## 7 纪律回执

- **未** push / dispatch / rerun / cancel / 建 tag（`gh` 只用了 `list`/`view`）。
- 写入集合 = `.github/workflows/ci.yml`（+20）· `.github/workflows/e2e.yml`（+6）· 本报告；`crates/`、`panel/`、`release.yml` **零改动**（`release.yml` 的 `gate:20 / build:None / release:None` 是我**读**出来的现状，不是我改的）。
- **e2e.yml 上带着 t110 的在途改动**（`git status` 显示 `M`，`+7/−1`）：我只在 `smoke` job 的 `runs-on` 之后插了 4 行注释 + 1 行 key，**没有触碰 t110 的取证段**；两次改动叠加后该文件仍 `parses as YAML` 且 guard exit=0。
- 本单不涉及数据库/守护进程：未写活库、未启停 pid 79984；无临时文件产生（读数全在 `gh` 与一次性 `python -c` 里）。

---

## 8 t114 追加：`release.yml` 的 `build` / `release` 上界（发布路径）

> §1–§7 是 t111（`ci.yml`/`e2e.yml`）。本节只谈 `.github/workflows/release.yml`，单号 **t114**，2026-09-29 10:4x–11:1x。

### 8.1 改前读数（HEAD 原文，PyYAML 取值）

```
HEAD:.github/workflows/release.yml  {'gate': 20, 'build': None, 'release': None}
```

⇒ 三份 workflow 里 **`release.yml` 的 `gate`（我 t100 加的 20 分钟）是当时唯一的上界**；`build`（矩阵：windows + linux 各一entry，`fail-fast: false`，含 `Build release binary` / `Build panel` / Package / Upload artifact 十步）与 `release`（checkout + download-artifact + `Create GitHub release`）**都没有** ⇒ 吃 GitHub 默认 **360 分钟**。发布路径是本仓**唯一对外产生后果**的路径（tag → release），而它的 `gate` 又要求 CI 已绿 ⇒ 卡住时「没有 release」与「release 正在做」在读数上不可区分。**注意 `release.yml` 历史上只有 2 次运行**（下表），这是本报告里**证据面最薄**的一节，§8.5 明写。

### 8.2 实测表（`gh run view --json jobs`，`startedAt`→`completedAt`，秒）

| job（矩阵 entry / 单 job） | 绿 run **34849294052**（09-14） | 红 run 35240185033（09-17） | 样本数 |
| --- | --- | --- | --- |
| `Build x86_64-pc-windows-msvc` | **7677s（128 min）** ← **上界锚点** | 3537s（59 min） | 2 |
| `Build x86_64-unknown-linux-gnu` | 2820s（47 min） | 1314s（22 min） | 2 |
| `Create release`（job） | **21s** | 612s（10.2 min，且该步 **failure**） | 2 |
| `gate`（同一文件，未改） | — | — | （t100 的 20 min 保留） |

**步级拆解（绿 run 的 windows entry，用来解释 128 min 是什么构成的）**：`Build release binary` **7243s（120.7 min）** · `Build panel` 134s · `Package (Windows)` 17s · `Upload artifact` 11s · `Post Run rust-cache` 214s（= 保存缓存） · `Swatinem/rust-cache@v2` 恢复 **4s**（⇒ **这一跑是冷缓存**，不是热缓存） · 其余 setup 合计 ~50s。
**冷/热跨度（这就是「重试余量」的来源）**：同一个 windows entry，冷的那次 7677s、热的那次 3537s ⇒ **2.2×** 差距；linux 同向 2820 vs 1314（2.1×）。
`release` 侧：`Create GitHub release` 绿时 **8s**，红时该步**烧掉 612s 才失败** ⇒ 连「失败的 API 腿」都要 ~10 分钟。

### 8.3 取值与理由

| job | 值 | 依据 |
| --- | --- | --- |
| `build`（每个矩阵 entry 各有一个上界） | **180 min** | 锚点 = **最坏的一次「绿」**：windows entry **7677s = 128 min**（冷缓存、其中 120.7 min 是 `--release` 整棵编译）⇒ **180 = 1.41×**。**这是本仓里最小的倍数，而且是有意为之**：真实发布构建本来就要 2 小时，取 3×（≈6.4h）或取 240 min 就等于没有上界（默认 360 min）。补的余量用**已实测的冷/热跨度**来定：热的那次同 entry 只要 59 min（2.2× 快）⇒ **即使这里误杀一次，重跑一次（热缓存）约 1 小时就能拿回**，代价远小于一个**静默 6 小时的挂住**。 |
| `release` | **30 min** | 绿时整 job **21s**、`Create GitHub release` **8s**；最长一次该腿 **612s（10.2 min，且随后失败）**⇒ **30 = 3× 最坏观测**（也是绿的 ~85×）。它是纯 `gh`/API + 小 artifact 下载，上界只为困住「API 调用/上传挂住」。 |
| `gate` | **20**（未改） | t100 已定，且它的注释里已写明内部最多等 15 分钟读 run 状态 ⇒ 20 留了 5 分钟余量。本单**不动**它。 |

**矩阵语义（必须写清，否则会被误读）**：`timeout-minutes` 作用在**每个 matrix entry**上（两个 entry 并行），不是「两个 entry 的合计」；`fail-fast: false` ⇒ 一个 entry 超时不会掐掉另一个。**发布路径的整条上界**因此可算：`gate 20 + max(build entry 180) + release 30 = 230 min`，仍显著低于默认的 360 min —— **这就是「挂住与慢可区分」在发布路径上的具体含义**。

### 8.4 验证读数

| 项 | 读数 |
| --- | --- |
| 改后三个值 | `PASS release.yml :: gate=20` · `PASS build=180` · `PASS release=30` ⇒ `release.yml all jobs -> {'gate': 20, 'build': 180, 'release': 30}` |
| 结构未被破坏（PyYAML 读回） | `gate steps=1` · `build steps=10` · `release steps=3` · `matrix entries=2` · `fail-fast=False` · `needs: build<-gate, release<-build` · 权限逐条未变（build `{contents:read, actions:write}` · release `{contents:write, actions:read}`） |
| 三份 workflow | `release.yml` / `ci.yml` / `e2e.yml` 均 `parses as YAML (PyYAML)` |
| 守卫 | `bash .github/workflows/scripts/check-workflow-refs.sh` ⇒ **exit=0**（含表达式合法性检查 ⇒ `${{ matrix.os }}` / `${{ matrix.target }}` 未被弄坏） |
| **一处不许被顺手重排** | `git diff --numstat -- .github/workflows/release.yml` ⇒ **`18 0`**（**18 行新增、0 行删除**；删除扫描 `^-[^-]` 命中 **0** 行）⇒ 全部是我新加的注释 + 两行 key |
| 范围 | `crates/`、`panel/` status **为空**；本单**没有**对 `ci.yml`/`e2e.yml` 做任何新改动（它们在 status 里显示 `M` 是 t111/t112 的**未推**字节：numstat `ci 28/8` + `e2e 10/4` = 38/12，与 t112 报告一致） |

### 8.5 不覆盖什么（第 19 条）

1. **不指出挂在哪个步骤/命令**：超时只让 **job** 变红（点名 `Build x86_64-pc-windows-msvc` / `Create release`）。要给 `gh release create` 或 `cargo build --release` 单独限时必须**另立单**（本单**没有**顺手加 `timeout-minutes` 到 step 上）。
2. **样本只有 2 次运行（n=2）**：本节是全部报告里证据面最薄的一节；180 min 的锚点来自**唯一一次绿**（09-14），一次更慢（新依赖、runner 争用）的**合法**构建仍可能被它杀掉。这里的取舍是**有意**的：发布构建已 2 小时，任何更宽的值都等价于默认值。补强的做法（未做）：在 release 前跑一次 `--release` 的预热构建，或把 windows entry 的缓存预热纳入发布流程。
3. **不覆盖 runner 镜像漂移**：`windows-latest`/`ubuntu-latest` 仍浮动（与 §6 第 2 条同）。
4. **不覆盖 `gh` CLI 版本 / runner 自带工具**：`Create GitHub release` 用的 `gh` 来自 runner 镜像。
5. **不覆盖 GitHub API 侧的慢**：API 调用**已经开始并被服务端接受**之后，客户端挂住仍只会被这个 job 上界杀掉 —— 超时**不会**回滚已经创建的 release（这属于「有界失败」，不是「事务」）。
6. **不覆盖缓存与 artifact 存储的可用性**：`Swatinem/rust-cache` 恢复/保存（绿 run 里 4s + 214s）或 artifact 上传卡住，也只会以 job 超时的形式表现。
7. **没有真跑过**：本单不许建 tag / dispatch ⇒ 这两个新值**尚未被一次真实发布验证**；它们的第一批读数要等下一次真实 tag。

### 8.6 纪律回执（t114）

- 写入集合 = `.github/workflows/release.yml`（**+18/−0**）· 本报告（追加 §8 与表头一行）；`ci.yml`/`e2e.yml`/`crates/`/`panel/` **零新增改动**。
- **未** push / dispatch / rerun / cancel / 建 tag；`gh` 只用了 `run list` / `run view --json`（只读）。
- 无临时文件产生；未写活库、未启停 pid 79984。
