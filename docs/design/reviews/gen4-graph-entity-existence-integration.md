# t121 集成收敛：`graph_entity` 存在性单元的冻结快照 + 最终字节四道门禁 + 推送载荷

> **性质**：**集成**（integration），作者 = verify2。收敛对象 = `t118`（实现）→ `t119`（独立验证）→ `t120`/`t139`（评审 round 1/2）之后的**最终字节**。**我不重做任何人的工作，也不 push**（push 由 captain 执行）。
> **一句话结论**：**评审判过的那份字节就是当前字节、就是 HEAD 的字节、而且已经在 `origin/main` 上**；四道门禁在最终字节上**全部 exit 0**（`test -p ruagent-mcp` lib 46 / roundtrip 5 / tool_surface 3；`test -p ruagent-daemon --lib` 157；`clippy -DenyWarnings` 0；`fmt --all --check` 0）⇒ **本单元可推送载荷 = 空集（早已推送）**，仓库里剩下的 4 个未推提交**不属于本单元**（清单见 §3）。
> **写入集合**：本报告（**唯一**我写的文件）。`crates/mcp/src`、`crates/daemon/src`、`crates/graph`、`panel`、`.github`、`scripts` **一行未改**。

---

## 0 关键结论表（先看这张）

| 问题 | 读数 | 证据位置 |
| --- | --- | --- |
| 评审判过的字节 == 当前字节？ | **是**。`crates/mcp/src/lib.rs` = **125,843 B / sha256 `EB4129AB9715414DC0262DAA7DD8AF26BE0BC2790C2B6A0D13BC10245A8FE420`**，与 t139 round-2 评审报告引用的 `EB4129AB9715414D…`（125,843 B，mtime 2026-10-04 00:49:24）**逐字相同** | §1 |
| 当前字节 == HEAD 的字节？ | **是**。`git diff --exit-code HEAD -- crates/mcp/src/lib.rs` = **0**；`git status --porcelain -- crates/mcp` = **空**；HEAD blob 与工作树做**原始重定向**哈希后**两者同为 `EB4129AB…`、同为 125,843 B** | §1 |
| 评审后又有人动过吗？ | **没有**（无任何文件改动本单元字节；`git diff --name-only HEAD` 里唯一一项**不是本单元的**，见 §1.3） | §1.3 |
| 四道门禁在最终字节上 | **全绿**：`mcp` exit 0 · `daemon --lib` exit 0 · `clippy -DenyWarnings` exit 0 · `fmt --all --check` exit 0 | §2 |
| 本单元要推什么？ | **没有**：承载 `crates/mcp/src/lib.rs` 的四个提交（`a95d474`/`ec24b61`/`94aed37`/`4677912`）与三份报告所在提交**都是 `origin/main` 的祖先** | §3 |
| 本轮报告四段齐吗？ | **齐**：改前 `:37` · 改后 `:51` · 负控 `:71` · 未测 `:91`（另有 verify `:159` / review `:90` 的"不覆盖"段） | §4 |
| 活环境碰了吗？ | **没有**：8787 的**实际**持有者是 **pid 14944**（契约与 t118 报告里写的 `79984` **不存在**，见 §5.1）；活库 `~/.ruagent/data/ruagent.db` mtime **2026-10-03 23:07:47**（早于我窗口）；本单元我**没起过任何守护进程** | §5 |

---

## 1 冻结快照（接受标准 1）

### 1.1 `git diff --name-only HEAD` 原文（冻结时刻 01:12，逐字）

```
docs/design/reviews/gen4-version-points-test-repair.md
```
**一行，且不是本单元的文件**：那是我（verify2）在 t142 里写的报告，`+6/-3` 共 9 行改动，**全部是散文**（§0 的 pins 三行、§6 的 flake 定性那句、§9 的临时目录交接那条）。成因：captain 在 `e6673d8` 里把该报告**连同** t142 的测试一起提交（`e6673d8` 含 `crates/daemon/tests/version_points.rs` +200 与报告 +259），而我**在该提交之后又做了这三处编辑** ⇒ 它们留在工作树里未提交。⇒ 谁在什么时候动的：**我自己，t142 回合内，散文级**；本单元（t118–t121）的任何字节都不在其中。

**`git ls-files --others --exclude-standard` = 空** ⇒ 没有未跟踪文件。

### 1.2 本单元字节的哈希（每个改动文件）

| 文件 | 字节 | sha256 | 状态 |
| --- | --- | --- | --- |
| `crates/mcp/src/lib.rs` | **125,843** | **`EB4129AB9715414DC0262DAA7DD8AF26BE0BC2790C2B6A0D13BC10245A8FE420`** | 工作树 == HEAD blob（原始重定向哈希实测两者相同）；== t139 评审引用值 ⇒ **评审的字节就是发运的字节** |
| 三份报告 | `gen4-graph-entity-existence-repair.md` 16,084 B sha256 `19A9A35A873120EF6AE4FE81…` · `…-verify.md` 22,144 B `8AE9CF997D170AE6B2AF3FCE…` · `…-review.md` 16,377 B `B473C8BC2E36F574D1EDEAB3…` | — | 三份都**已提交**（分别落在 `ec24b61` / `6a5961e` / `d5be811`），且都在 `origin/main` 上 |

**HEAD blob 的正确做法（含一次我自己的仪器失误，按纪律自报）**：我第一次用 `git cat-file blob HEAD:… | Set-Content …` 做对比，得到 `D042720D…` ≠ 工作树 `EB4129AB…` —— **那是仪器错**：PowerShell 的文本管道/`Set-Content` 会把 LF 折成 CRLF，**经过文本管道就不能再比较字节**。改用**原始重定向**（`cmd /c "git cat-file blob … > file"`）后：**两者同为 125,843 B、同为 `EB4129AB…`** ✅。这条与 C40（"给退出码当证据的命令不要接管道"）是同一族教训：**给字节当证据的对比不要经过文本处理**。

### 1.3 评审之后有没有人动过（逐条点名）

* **本单元的字节**：没有。最后一次改动它的提交是 **`a95d474`（2026-10-04 01:02:20，"fix(mcp): the entity comment cites coordinates that hold on the bytes it ships with"，t138 的注释修订）**，而工作树的 mtime 是 **00:49:24**（评审 t139 在 01:00:36 引用的就是这份）⇒ 提交只是把这些字节**记下来**，没有再改。
* **评审的证据链（daemon 侧坐标）也在当前字节上复核过**：`crates/daemon/src/api.rs` = 291,473 B / sha256 `C43BACC0A535A107E39F967FD81EF222A112F48FA760968422647F194B789B37`（与我之前几轮读到的一致），t139 §② 引用的坐标**逐条命中**：
  `:1355 let entity = ruagent_graph::entity_by_id(db, id).await?;` · `:1360 Ok(Json(serde_json::json!({` · `:1361 "name": entity.as_ref().map(|e| e.name.clone()),`（**存在信号的真实发出行**）· `:1365 })))` · `:6711` 钉住用例函数 · `:6783` 200 断言 · `:6784 assert!(missing["name"].is_null(), "{raw}");` · `:6785` facts 空断言 ⇒ **评审建立的"存在信号在哪儿发出、在哪儿被钉住"这条链在发运字节上仍然成立**。
* **树在我窗口里被同伴动过两个文件**（**与本单元无关**，按 t20 披露）：冻结快照（01:12）只有 1 项；四道门禁跑完后（01:16）`git status --porcelain` 多出 `.github/workflows/ci.yml` 与 `docs/design/reviews/gen4-audit-gate-verify.md` 两项。两者**都不是本单元路径、也都不是这四道门禁的编译输入**（前者是 CI yaml，后者是 markdown）⇒ 门禁读数仍然认证本单元字节。

---

## 2 最终字节上的四道门禁（接受标准 2）

**运行方式**：每个命令的**退出码在任何管道之前**捕获（C40）；输出各自重定向到文件后再检索（不用 `Select-Object` 截断管道）。运行窗口 **01:13:37 → 01:13:51**。

| # | 命令 | 退出码 | 计数原文 |
| --- | --- | --- | --- |
| 1 | `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-mcp` | **0**（`[cargo-team] exit=0 elapsed=5.7s`） | lib：`running 46 tests` / `test result: ok. 46 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out` · `tests\roundtrip.rs`：`5 passed; 0 failed` · `tests\tool_surface.rs`：`3 passed; 0 failed` · doc-tests：`0 passed; 0 failed` |
| 2 | `… test -p ruagent-daemon --lib` | **0**（`exit=0 elapsed=4.2s`） | `test result: ok. 157 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.35s` |
| 3 | `… clippy -p ruagent-mcp --all-targets -DenyWarnings` | **0**（`exit=0 elapsed=1.6s`） | `cargo clippy -p ruagent-mcp --all-targets -- -D warnings` ⇒ `Finished dev profile [unoptimized + debuginfo] target(s) in 1.41s`（无 error/warning 行） |
| 4 | `cargo fmt --all --check` | **0** | 无输出（零 `Diff in`）——用 `cmd /c "cargo fmt --all --check > file 2>&1"` 取码 |

**这批读数认证的是哪份字节**：本单元 `crates/mcp/src/lib.rs` 在整个 t121 窗口内**哈希未变**（§1.2），且我**没有写过任何 `crates/**`**；四条命令都是 Rust 门禁，不读 `.github/**` 或 `docs/**` ⇒ 上面四绿**属于 §1 冻结的那份字节**。

**第 2 条门的旁注（与 t142 的 flake 有关，值得留档）**：同一条 `test -p ruagent-daemon --lib` 在 t142 回合里**曾在负载下红过一次**（`chat::generating_tests::delete_removes_the_row_and_stops_a_live_run_first`，`chat.rs:2046`），而 **captain 在空闲机器上复核为绿**、我这里（机器空闲、无隔离构建在跑）**也是 157 passed / 0 failed** ⇒ **"负载敏感 flake"的定性得到两次独立复核**（captain 已立案 `t145`）。

---

## 3 推送载荷（接受标准 3）

### 3.1 本单元：**没有可推的东西（早已在 `origin/main` 上）**

`git merge-base --is-ancestor <c> origin/main` 逐条实测：

| 提交 | 说明 | 在 `origin/main` 上 |
| --- | --- | --- |
| `ec24b61` | "feat(memory,mcp,daemon,panel,ci): five units from the audit lines…"（**t118 的实现就装在这里**，与另外四个单元同提交） | **YES** |
| `a95d474` | "fix(mcp): the entity comment cites coordinates that hold on the bytes it ships with"（t138 的注释坐标修订） | **YES** |
| `94aed37` / `4677912` | capability 单元（同文件历史） | **YES** |
| `6a5961e` | `gen4-graph-entity-existence-verify.md`（t119 报告） | **YES** |
| `d5be811` | `gen4-graph-entity-existence-review.md`（t120/t139 报告） | **YES** |

### 3.2 仓库里**仍然未推**的东西（**不属于本单元**，供 captain 决定）

`origin/main..HEAD`（`git rev-list --left-right --count origin/main...HEAD` = `0  4` ⇒ 远程不领先，本地领先 4）：

```
6762b62 docs(reviews): t134 verification of the audit gate -- ...            (t134 报告)
e6673d8 test(daemon): the independent recomputation stops being blind on language (t142 测试 + 报告)
01b9021 fix(ci): the audit gate's counts come from metrics.json, ...          (audit-gate CI 修正)
498170a docs(reviews): a source-scanning guard that already has all three parts ... (audit-gate 报告)
```
外加**一处未提交**：`docs/design/reviews/gen4-version-points-test-repair.md` 的 `+6/-3` 散文（§1.1）。
⇒ **本单元的推送载荷 = ∅**；上面这些是**别的单元**的，只有它们的作者/captain 能定提交信息。

### 3.3 交付给 captain 的 `推送载荷`（本单元，供 changelog/推送说明用）

* **改动路径清单**：`crates/mcp/src/lib.rs`（唯一代码文件；工作树 == HEAD == 已推送）。
* **报告路径**：`docs/design/reviews/gen4-graph-entity-existence-repair.md`（t118 实现）· `…-verify.md`（t119 独立验证）· `…-review.md`（t120 round 1 + t139 round 2）· 本报告 `docs/design/reviews/gen4-graph-entity-existence-integration.md`。
* **一句话提交信息**（说清修的是什么事实；本单元的修复已在 `ec24b61`/`a95d474` 里，若要为它补一条独立说明可用这句）：
  `fix(mcp): graph_entity decides existence from the wire's real signal — an invented id is refused instead of answered with "no current facts"`（中文版：`修 mcp：graph_entity 改为读 wire 的真实存在信号（name:null），瞎编 id 现在被拒，而不是被答以"没有当前事实"`）。
* **⚠ 流程提醒（`GEN4-EX-R2` 再次成立）**：`ec24b61` 把**五个单元**装进同一个提交 ⇒ 在共享树里 `git diff --name-only HEAD` **无法把写入集合归属到某个单元**（本任务的接受标准 1 因此只能靠**哈希同一性**回答，不能靠 diff 归属）。建议后续**一单元一提交**，否则"申报路径 == 工作树里该单元的路径子集"这条审计永远是估计。
* **本报告是本次 diff 里会多出来的一项**：我写完后，`git diff --name-only HEAD` 会多出 `docs/design/reviews/gen4-graph-entity-existence-integration.md`（**本单元的集成产物**，属本单元申报范围内）。

---

## 4 本轮报告四段自检（接受标准 4）

`docs/design/reviews/gen4-graph-entity-existence-repair.md`（t118 的实现报告；**存在**，16,084 B）：

| 要求的段 | 命中（行号按当前字节） |
| --- | --- |
| **改前** | `:35 ## 3 改前 → 改后（同一条路径的两个方向）` + `:37 ### 3.1 改前（实测）` ✓（含"负控窗口 #3 实测原文"：`expected a refusal, got a successful result: … is_error: Some(false)`） |
| **改后** | `:51 ### 3.2 改后` ✓（不存在 ⇒ `Err` + `… not found: the graph has no entity with that id`；存在零事实 ⇒ 仍成功） |
| **负控** | `:71 ## 5 负控：三个窗口（全部**先宣告**、限时、按字节恢复）` ✓（窗口 #1 3m56s ⇒ `6 passed; 1 failed` 只红"不存在"方向；#2 1m13s 已记账；#3 46s 取到客户端可见原文） |
| **未测项** | `:91 ## 7 不覆盖什么 / 登记（第 19 条）` ✓（含"拒绝文本是被断言包含而非逐字打印"等） |

**另两份报告也各有"未测/不覆盖"段**：`…-verify.md:159 ## 6 不覆盖什么（第 19 条）`、`…-review.md:90 ## 4 不覆盖什么（第 19 条）` ⇒ **四段要求满足，无缺段**。

---

## 5 卫生核对（接受标准 5）

### 5.1 活守护进程与活库（**一处事实纠正**）

```
8787 owner pid = 14944  name=ruagent  started=10/02/2026 23:05:45
pid 79984 alive: False            ← 本任务契约与 t118 报告里引用的 pid
live health: ok/ruagent           ← 只读 GET /api/v1/health
```
* **纠正**：本任务契约写"活守护进程（127.0.0.1:8787 / pid 79984）"，但 **79984 不存在**；`127.0.0.1:8787` 的实际持有者是 **pid 14944**（启动于 `2026-10-02 23:05:45`，`/api/v1/health` = `ok/ruagent`）。按 **C32**（按端口归属与健康引用活进程，而不是记忆中的 pid），本报告以 **14944** 为准；建议把契约与 t118 报告里的 `79984` 一并更正（t118 报告 §8 的卫生行也引用了它）。
* **我只对它做过一次只读健康检查**（`GET /api/v1/health`，无副作用；本单元的读法**不碰** `GET /api/v1/recall`，那条会写 `recall_log`）。
* **活库**：`~/.ruagent/data/ruagent.db` mtime **2026-10-03 23:07:47**、24,272,896 B —— **早于我的窗口（01:12–01:16）**；本单元我**没有起过任何守护进程**，也没有向 8787 发过写请求。

### 5.2 我的临时进程 / 临时目录（先列清单再按具体路径删）

```
（删除前清单）%TEMP% 里匹配 t121* 的条目：
   t121-gates.txt        mtime 2026-10-04 01:13:43   ← 我本单元创建的 4 个门禁日志
   t121-gates.txt.2      mtime 2026-10-04 01:13:48
   t121-gates.txt.3      mtime 2026-10-04 01:13:50
   t121-gates.txt.4      mtime 2026-10-04 01:13:50
   t121z-apply.py        mtime 2026-09-24 02:20:25   ← 不是我的（早 10 天），不动
   t121z.json            mtime 2026-09-24 02:20:25   ← 不是我的（早 10 天），不动
```
```
（删除后）my four gate logs deleted: 0 remaining
   t121z-apply.py / t121z.json  仍在（未触碰）
   my t121 leftovers (gates only): 0  |  t121z* (not mine, kept): 2
   my t121-named processes: 0
   ruagent.exe processes: 1（= 活守护进程 pid 14944，不是我起的）
```
⇒ **我自己的临时文件全清**；两个**不是我的**、10 天前的 `t121z*` **一个没碰**（t135 那次 glob 误删的教训按"先列清单、只删自己创建的具体路径"执行）。

* 本单元我起的临时进程：**0**（没起守护进程；`ruagent.exe` 只有活的那一个）。
* 未 push、未暂存、未改他人文件：`git status --porcelain` 里 `crates/**` 无我的任何改动；`git add` 未执行过。

---

## 6 未测 / 不覆盖

* **没有重做任何人的验证**：单元**行为**（瞎编 id 被拒 / 零事实成功）的读数属于 t118（§3.1/§3.2）与 t119（§2 两方向原文），本报告只做**字节同一性**与**门禁**，不复现行为。
* **没有重开负控窗口**：t118 的三个窗口读数属于**移动前**字节（`52E9F00A…`，见其报告 §9.2 的自披露）；本单元最终字节（`EB4129AB…`）的差异是**注释级 10/5**，其报告 §9 已就此说明。若要"最终字节上的行为负控"，那是**新一轮**工作（我按契约成本约束没有开）。
* **没有起真守护进程、没有真 MCP 客户端**：真实 daemon 路由层的证据在 t119（它自己的 daemon + 端口 8821）；本单元最终字节上的两条用例对 **axum stub** 发真实 HTTP/真实 MCP 协议调用（t118 §9.3 披露），与本集成报告的分工不同。
* **不是我最先交付的东西**：本集成**没有**审查其他单元（`ec24b61` 里的另外四个），也没有回答"这五个单元能否一起推"——§3.2 只给出**清单**与归属，取舍由 captain。
* **`git diff` 无法归属写入者**（`GEN4-EX-R2`）：本报告用哈希同一性回答单元边界，但这只是"该单元的字节没动"的充分证据，不能证明"没有别人碰过该单元的字节"（那由 §1.3 的提交历史 + 哈希不变共同支持）。
* **我没有 push**（按要求），也没有验证远端历史是否与本地一致之外的任何东西（`git ls-remote origin main` 只读确认 remote main = `d5e21869b7…`，且 `origin/main` 本地引用与它一致 ⇒ 我的比较不是基于过期引用）。

---

## 7 复现命令（照抄可用）

```powershell
cd C:\Users\19410\Documents\ai\ruagent
# 冻结快照
git diff --name-only HEAD
git ls-files --others --exclude-standard
git status --porcelain -- crates/mcp
git diff --exit-code HEAD -- crates/mcp/src/lib.rs           # 0 = 工作树 == HEAD
Get-FileHash crates/mcp/src/lib.rs -Algorithm SHA256          # 须 = EB4129AB…
# HEAD blob 的哈希：必须用原始重定向（文本管道会改字节）
cmd /c "git cat-file blob HEAD:crates/mcp/src/lib.rs > %TEMP%\blob.rs"
Get-FileHash "$env:TEMP\blob.rs" -Algorithm SHA256            # 与工作树相同
Remove-Item "$env:TEMP\blob.rs"
# 推送状态
git rev-list --left-right --count origin/main...HEAD
git log --oneline origin/main..HEAD
foreach ($c in (git log --format=%h -4 -- crates/mcp/src/lib.rs)) { git merge-base --is-ancestor $c origin/main; "$c on origin/main: $(if ($LASTEXITCODE -eq 0) {'YES'} else {'no'})" }
# 四道门禁（退出码在任何管道之前捕获）
$env:PATH="$env:USERPROFILE\.protoc\bin;$env:PATH"; $env:NO_PROXY='127.0.0.1,localhost,::1'
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-mcp > $env:TEMP\g1 2>&1; "G1=$LASTEXITCODE"
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-daemon --lib > $env:TEMP\g2 2>&1; "G2=$LASTEXITCODE"
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 clippy -p ruagent-mcp --all-targets -DenyWarnings > $env:TEMP\g3 2>&1; "G3=$LASTEXITCODE"
cmd /c "cargo fmt --all --check > `"%TEMP%\g4`" 2>&1"; "G4=$LASTEXITCODE"
# 卫生
(Get-NetTCPConnection -State Listen -LocalPort 8787 | Select-Object -First 1).OwningProcess   # 14944
Get-FileHash crates/daemon/src/api.rs -Algorithm SHA256                                       # C43BACC0…
```

---

## 8 纪律回执

* **写入集合** = 本报告（`docs/design/reviews/gen4-graph-entity-existence-integration.md`）。`crates/mcp/src`、`crates/daemon/src`、`crates/graph`、`panel`、`.github`、`scripts` **零改动**；**没有用"只修正报告笔误"的授权去改任何其他报告**（§1.1 里那份 t142 报告的未提交散文**我没有替它提交/修改**——它属于 t142 的 inScope，不在本单）。
* **构建路径**：四道门禁**全部**走 `scripts/cargo-team.ps1`（同树**没有**设 `CARGO_TARGET_DIR`/没有 `-TargetDir`；`-DenyWarnings` **只**用于 clippy，未用在 test 上）；退出码在任何管道之前捕获（C40）。
* **活环境**：8787 只读健康检查一次（`ok/ruagent`），活库未触碰（mtime 早于我窗口）；**本单没起过守护进程**。
* **临时物**：`%TEMP%\t121-gates.txt{,.2,.3,.4}` 已按**具体路径**删除；两个**不是我的** `t121z*`（2026-09-24）**未触碰**；我名下进程 = 0。
* **仪器失误自报**：第一次比对 HEAD blob 用了 PowerShell 文本管道 ⇒ 得到错误的 `D042720D…`；改用原始重定向后两者同为 `EB4129AB…`（§1.2）。这条与 C40 同族（给字节/退出码当证据时不要经过文本处理），已写进本报告而不是让它过去。
* **事实纠正**：契约里的活进程 `pid 79984` 不存在，实际持有者是 **pid 14944**（§5.1）。
* **自我评审声明**：本条是集成，不是评审；t118 的实现由 t119 独立验证、t120/t139 独立评审 ⇒ 我不给自己或他们的结论背书，只给字节与门禁读数。
