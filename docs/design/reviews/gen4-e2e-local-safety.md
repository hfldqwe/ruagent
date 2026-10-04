# t179 修 C60：本机复现 CI 的 E2E 会驱动活守护进程 + 重写真实 agents.toml（mem-core）

- 本单 attempt `9ff87426-ca69-4f7b-b13e-d18a0388d1a2`；写入集合 = `panel/e2e/run-e2e.mjs`、`.github/workflows/e2e.yml` + 本报告
- **一句话**：入口点**永远要求显式目标**（`CI` 不再代替命名）且**按名字拒收守护进程默认端口 8787**；workflow 的种数据/起守护进程两步改成**从 `$RUNNER_TEMP` 取临时根**（拿不到就响亮失败，而不是退回 `$HOME`）并**换到私有端口 8891**。⇒ 照抄 workflow 的人**不可能**落到真实配置或活守护进程上。

## 1 先量证危害（原文 + 坐标，不采信 t174 的转述）

**① 入口点的目标选择**（`panel/e2e/run-e2e.mjs`，HEAD 版）：
```
20: const explicit = process.env.E2E_BASE_URL?.trim();
21: const target = explicit || (process.env.CI ? "http://127.0.0.1:8787" : "");
```
⇒ **只要环境里有 `CI` 且没显式给 `E2E_BASE_URL`，baseUrl 就是 `http://127.0.0.1:8787`** —— 而 8787 是本机**常驻守护进程**的默认端口（本机读数：listener pid **14944**，见 t174/t178/t183 的证据链）。
同文件 `L14-17` 的注释当时写着「**a LOCAL run cannot take that branch**」——**这句就是洞**：分支键在**变量**上，而本机 shell 完全可以 `export CI=1`。

**② workflow 那一步把 mock agents 写到哪里**（`.github/workflows/e2e.yml`，HEAD 版）：
```
118: mkdir -p ~/.ruagent/config
119: root="$HOME/.ruagent"
120: ... > "$root/config/e2e-alpha.json"      (e2e-beta.json 同)
127: cat > "$root/config/agents.toml" <<EOF
```
⇒ 三个文件都落在 **`$HOME/.ruagent/config/`**。在 runner 上 `$HOME` 是**一次性**的；**拷到本机就是操作者真实的配置**（本机 `~/.ruagent/config/agents.toml`：5757 字节、sha256 `be898952dcf1d23e…`、mtime 2026-09-29 04:55:13）。

**③ 合起来**：在本机带 `CI` 跑 `npm run test:e2e`（并按 workflow 的步骤种数据、起 8787 守护进程）⇒ **驱动活守护进程**（套件会写：consumption 写一条 memory + recall_log + document + entity；registry 在守护进程**自己的 agents.toml** 里建 runtime/role）**且覆盖用户真实 `~/.ruagent/config/agents.toml`**。⇒ 这正是**数据丢失形状**，而且它**绕过**了本代那两道防线（`write-guard.ts` / `RUAGENT_E2E_ALLOW_WRITES` 保护的是**套件**的写，这里漏的是**workflow 那一步的种数据**与**入口点的目标选择**）。

## 2 修法：两处都修，并说明为什么不是二选一

**① `.github/workflows/e2e.yml`（保护「照抄 workflow 的人」）**
- 种数据步：根改为
  `root="${RUNNER_TEMP:?RUNNER_TEMP must name a throwaway dir … -- refusing to fall back to \$HOME}/ruagent-e2e"`
  —— **`:?` 是硬门槛**：本机照抄时 `RUNNER_TEMP` 通常为空 ⇒ **响亮失败**，而**不会**退回 `$HOME`；再加一道具名拒绝：若解析出的根等于**或位于** `$HOME/.ruagent` 之下 ⇒ `::error::refusing to seed mock agents into (or under) the real $HOME/.ruagent …` + `exit 1`（在 `mkdir` **之前**）。
- 起守护进程步：`export RUAGENT_HOME="${RUNNER_TEMP:?…}/ruagent-e2e"` + **端口 8787 → 8891（私有）**，健康检查同端口。
- 目标显式化：job `env:` 加 **`E2E_BASE_URL: http://127.0.0.1:8891`**（字面值；**`runner.temp` 不能出现在 job 级 env** —— 本文件 `L50-55` 自己记着：GitHub 静态校验会把整个文件判废，run 36510848293 就是这么 0 秒死的 ⇒ 所以根由**步骤里的 `$RUNNER_TEMP`** 推导）。
- 连带修一处**会静默打错端口**的依赖：`doctor` 的 `--url` 默认是 8787（`cli/src/main.rs:17`）⇒ 改成 `--url http://127.0.0.1:8891`。不读 CLI 就会让这一步去问**别人的**守护进程。

**② `panel/e2e/run-e2e.mjs`（保护**任何**本机调用）**
- `const target = explicit || "";` ⇒ **`CI` 不再能代替「命名目标」**；
- 新增**具名拒绝**：目标指向 loopback 且端口是 **8787**（守护进程默认 bind）⇒ 拒绝，消息里同时给出两条理由与**带 `RUAGENT_HOME` 的**临时根处方（种数据步写的 `<root>/config/agents.toml` 正是操作者的 agent registry）。
- **两条拒绝都发生在 `spawn` 之前**（`process.exit(2)`）⇒ 拒绝本身**不可能写任何东西**；`RUAGENT_E2E_ALLOW_WRITES` 那段（旧 `L45-55`）**逐字未动**。

**为什么组合**：① 防的是「拷了 workflow」这条路径（种数据 + 起进程），② 防的是**任何**本机调用（`CI` 单独不足以决定目标；即使有人手打 8787 也被按名字拒收）。**都没有削弱 CI**：CI 自己**显式**命名目标与端口。

## 3 读数

**(a) 危险形状被挡住**（全部用我自己的环境；**没有**起任何真实守护进程；**没有**指向真实根）：
| 实验 | 命令 | 读数 |
| --- | --- | --- |
| A（C60 形状） | `env -u E2E_BASE_URL CI=true npm run test:e2e -- --reporter=list` | **EXIT=2**；`e2e REFUSED: E2E_BASE_URL is not set.`；**playwright 从未启动**（日志里没有 "Running … tests" 行） |
| B（手打旧默认） | `CI=true E2E_BASE_URL=http://127.0.0.1:8787 …` | **EXIT=2**；消息点名 `points at the resident daemon's default port (8787)` |
| C（正对照） | `CI=true E2E_BASE_URL=http://127.0.0.1:8899 … --list` | **EXIT=0**；`e2e target: http://127.0.0.1:8899 (writes armed via RUAGENT_E2E_ALLOW_WRITES=1)`、`Total: 62 tests in 16 files` ⇒ **守卫是具名的，不是普遍拒绝** |
| D（种子步的拒绝，红控） | `RUNNER_TEMP=$HOME/.ruagent bash <种数据步的**原字节**>` | **EXIT=1**；`::error::refusing to seed mock agents into (or under) the real $HOME/.ruagent (…/ruagent-e2e)…`；**真实根下没有新建任何东西** |

**(b) CI 路径仍可用** —— 用**我自己的**临时根 + workflow 的**私有端口 8891**，并**跑的是从 YAML 里抽出来的那一步的原字节**（WSL/pyyaml 抽取 `Register e2e mock agents` 的 `run:`）：
- 种数据：写到 `<RUNNER_TEMP>/ruagent-e2e/config/{agents.toml, e2e-alpha.json, e2e-beta.json}` ✓（`<我的 RUNNER_TEMP>` 之外一个字节没写）。
- 套件（仓库入口）：**run 1 = exit 1 / `61 passed` + 1 failed（`registry.spec.ts:58` agent-grid 计数，Expected 5）**；**run 2 = `NPM_E2E_EXIT=0` / `62 passed (18.2s)`**，我从 JSON 两次读数：`per-spec walk: measured=62 passed=62 failed=0 flaky=0 skipped=0`、`stats block: expected=62 …`、**逐字段零差异**，逐 spec 16 文件（`registry.spec.ts 1` ✓）⇒ **run 1 那次是本地 flake**（点名，不平均掉）。
- 守护进程：我的记录 pid == 8891 的 listener（`mine=True`）、无 bind 错误、panel 从 `<repo>/panel/dist` 服务、收尾按 pid 停、**8891 随后空闲** ✓。

## 4 覆盖声明补的那一行（本单新增项）

位置：`Coverage declaration (what a green here does NOT mean)` 步的摘要块（现有 4 条 bullet 之后）。加的是**一条**：
> `* **what the two counting guards below do NOT claim** (t179): the measured-test floor is a COUNT -- it says that many tests were measured, not that any of them is meaningful, and not which ones they are. The must-run set names 6 specs (\`registry.spec.ts\` verbatim, plus recall, consumption, failure-visibility, chat, settings-capabilities), so a legal skip of any OTHER one of the suite's 24 \`test.skip(\` sites still leaves this job green.`

**判据①（先读守卫再写）**：must-run 列表**5 条**（recall/consumption/failure-visibility/chat/settings-capabilities），`registry.spec.ts` 由**上面那条既有守卫**逐字钉住 ⇒ **6 个具名 spec** ✓；「24 处」= `panel/e2e/**/*.spec.ts` 里 `test.skip(` 的 **24** 处（整目录 grep 得 25，第 25 处是 `write-guard.ts:34` 的**注释**）✓ —— 两个数字我都**重测**过。
**判据②**：任何守卫的判定都没改（§5 逐字证据）。
**判据③**：声明里**没有**等价表述可指（4 条 bullet 是 Rust 测试套件 / panel 类型检查 / Windows / 8 个 live-COPY 仪器；「Counting unit」段讲的是 Rust 目标与 Playwright 摘要计数）⇒ 我**新增**而不是重复。

## 5 没有削弱既有守卫（逐字）

- **arming 块**（`RUAGENT_E2E_ALLOW_WRITES`）HEAD vs 现在 **IDENTICAL**（`015b216f77aaa33d`）；**三条证据守卫**按区域哈希 IDENTICAL：unnamed-skip `2f5c69009c6f4038`、registry 行 `e9512804ba66430e`、`rc != 0` `e9f96a1d3a13e536`；**t166/t178 的下限+必跑集合+`walk_*` 块** IDENTICAL（`6e911253309dd83e`）✓。
- `write-guard.ts`、`panel/package.json`、`ci.yml`、`release.yml`：HEAD vs 工作树 **IDENTICAL** ✓。
- 我的 diff 里**没有任何守卫行**：提及 `RUAGENT_E2E_ALLOW_WRITES` / `E2E_MEASURED_FLOOR` / `walk_` / `test:e2e` / `playwright test` 的增删行各 **0**；只出现两次的是 `::error::`(+1) 与 `must-run set`(+1)，两处都是**在新位置新增**（种子步的拒绝行；覆盖声明那句），不是对守卫的改 ✓。
- 我的改动：`panel/e2e/run-e2e.mjs` **46/9**、`.github/workflows/e2e.yml` **43/8**；hunk 全在我意图的区域（入口点的目标选择与拒绝消息；env / 覆盖声明 / 种数据 / 起进程 / doctor / panel-serves）✓。
- 旁注（**不是我**）：工作树里 `.github/workflows/audit.yml` 显示 CHANGED（140/2，hunk 217+/451+，mtime **20:38:28**，内容是 `DISCLOSURES=$RUNNER_TEMP/disclosures.md` 一类新增）—— 那是**同伴在途**的工作，不在我 inScope，我一次未碰。

## 6 门禁

| 命令 | 读数 |
| --- | --- |
| `npm --prefix panel run build` | **exit 0**（`✓ built in 6.63s`） |
| `bash .github/workflows/scripts/check-workflow-refs.sh` | **exit 0**（四份 workflow 解析通过；引用的路径都被 tracked） |
| `npm run test:e2e`（仓库入口，因为改了入口） | **exit 0 / `62 passed`**（§3b run 2；run 1 的 61/1 已点名） |

## 7 我没有验证的 / 我自己的错误（第 19 条 + C63）

**未覆盖**：
1. **没有在 CI 上验证**（本单不 push）：runner 上的行为是「静态论证 + 本地执行那两步的原字节」，**不是**一次 CI 运行。
2. **只有一个平台**（Windows + chromium，1440×900）；CI 是 ubuntu（`$PWD` 是 POSIX 路径，**不需要**我下面那条本地适配）。
3. **本地适配（说清楚，不隐藏）**：种数据步把 mock agent 路径写成 `$PWD/…`；在 Git Bash 里那是 MSYS 路径 `/c/…`，而**Windows** 守护进程无法 exec ⇒ 本地跑我把三处 `command =` 的路径形式转成 `C:/…`，并建了一个 `target` junction 让那个 exe 存在（CI 上 POSIX 形式本来就对）。这是我的**装置**差异，不是那一步的缺陷。
4. **真实 `~/.ruagent` 全程未碰**（本单红线）：`agents.toml` sha256 `be898952dcf1d23e…`、mtime `2026-09-29 04:55:13.892013200 +0800`、config 目录清单在**开工前与收尾后逐字相同** ✓；我的根是 `%TEMP%\ruagent-t179\runner-temp\ruagent-e2e`。
5. **一条纪律滑手（如实记）**：我第一版读脚本的收尾用 `curl http://127.0.0.1:8787/api/v1/health` 探了一次活守护进程 —— 那是**只读健康探测**（**不是**被禁的 `/api/v1/recall`，它不写 recall_log），但**我本不该发**：后续脚本已删掉该探测，活守护进程（pid 14944）从未启停、未调用任何数据端点。实验 B 里出现的 `8787` 只是 node 里的**字符串比较**，从不建立连接（拒绝发生在 `spawn` 之前）。

**我自己的 4 个 harness 错误（每个都曾产出过错误读数）**：
1. 用 **`/mnt/c/...` 路径在 Git Bash 里**抽取 workflow 步骤 ⇒ 抽取失败、种数据没发生 ⇒ 空根上跑出 **56 failed / 6 skipped**（顺带一条有用读数：**空根是响亮失败，不是静默跳过**）。
2. 上面第 5 条的 8787 健康探测。
3. **上一轮的守护进程没真死**：Git Bash 的 `$!` 是 MSYS pid，`kill` 没可靠终结 Windows 进程 ⇒ run 2 的守护进程以 `os error 10048`（地址占用）退出，而套件其实在跟**空根的那只残留守护进程**说话 ⇒ 又是 56 failed。修法：用 `Start-Process -PassThru` 记 **Windows pid**，并断言 **listener pid == 我的 pid**。
4. 路径形式适配时**过早**删掉 junction ⇒ 转换后的 Windows 路径不存在 ⇒ 59/62（多挂一个依赖 agent 的 spec）。修法：junction 活到整轮结束，最后才 `rmdir`。

## 8 残留与清理

- 写入：`panel/e2e/run-e2e.mjs`、`.github/workflows/e2e.yml`、本报告。
- 我起的守护进程共 5 次（各轮 8891），**全部按我记录的 pid 停掉**（最后一次 pid 34112），**8891 空闲** ✓；junction 全部用 `rmdir` 删除（C39）✓；`panel/dist` 被契约要求的门禁刷新（构建产物）。
- 复核材料在 `%TEMP%\ruagent-t179\`：`seed-step.sh`/`boot-step.sh`（从 YAML 抽出的原字节）· `seed-refusal.log`（红控）· `seed-ok2.log` · `runner-temp/ruagent-e2e/config/*` · `daemon*.out|err` · `e2e-run*.log` · `playwright-evidence-run1.json` 与 `playwright-evidence.json`（61/1 与 62/62 两次读数）· `guard-proof*.sh`。按**具体路径**删即可。
