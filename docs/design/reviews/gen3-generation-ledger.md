# 第三代总账（ruagent 记忆/知识/wiki/图谱/召回 + 单一消费面）

> **本文件是 captain 维护的总结底座**：所有数字都**带来源与限定**；**未取到/未验证**一律明写。
> 读数时间窗：2026-09-27 → 2026-09-29（UTC+08）。代码基线 `5db881d`；**本代成果已推送 10 次**（`origin/main = 1516814`，见 §1/§7）。**CI 已在 `b1e0e9f` 全绿（两条 workflow 都是 success，见 §10）。**
> 修订规则：只追加/就地更正，旧文字逐字保留。

---

## 1. 一页结论（截至本账落盘时）

| 目标 | 状态 | 依据 |
|---|---|---|
| ① 提交并推送到 `main` | **✅ 已完成** | **七次推送**：`0da0cb6`（159 文件/+45,177/−914）→ `cb55073`（memory 编译修复）→ `877a909`（memory 无警告 + graph as-of）→ `cfb52b1`（fmt 漂移 + t75 备份面 + t103 面板第三态）→ `44636db`（t104 判定步假红修复 + t105 审计）→ `5d3adfd`（`e2e.yml` 非法 `${{ }}` 一行修复）→ `e3a58ea`（t106 守卫加固 + t107 e2e 不许猜目标 + t85 规格锚点）。**全程未 force-push、未推 tag**（仍只有 `v0.1.0`） |
| ② `main` 上 CI 转绿 | **✅ 已完成（`b1e0e9f`）** | **`CI 36507120586 = success` + `E2E 36507120602 = success`**：ubuntu 的 `Guard(paths)/Coverage/Format/Clippy/nested-Result 守卫/Test/Test evidence/Ignored 清单` **全绿**，windows ✅，`Panel (node)` ✅；E2E 的 `Doctor(7)/Playwright/E2E evidence/Upload` 全绿。**四层结构性红逐层消解**（编译错误 → 警告即错误 → fmt 漂移 → 判定步恒红），**三个真实缺陷全部修掉**（t108 平台幼稚夹具 · t109 `claude` 子串巧合 · t110 点名机制——最后一条仍**在办**且**与绿无关**：机制失效时它静默报 0，绿并不代表纪律落地）。**这条绿自己声明了「什么没跑」**：全 CI 仅一步 SKIPPED（`Test attempt 2`，设计使然），ignored-instrument manifest 打出 **`15 ignored = 7 run + 8 declared`**（8 条逐条点名 + 理由） |
| ③ 持续发掘 → 实现 → 验证 → 评审 → 集成 | **在运转** | **113 张单**；八轮只读审计的 finding 全部登记；**已完成并入库**：`t104`（判定步假红）`t105`（守卫审计）`t106`（守卫加固 + 非法 `${{ }}` 检测）`t107`（e2e 不许猜目标）`t85`（规格锚点）`t108`（平台幼稚夹具）`t109`（`claude` 子串巧合）`t113`（永不执行的覆盖）`t98`（**迁移 25/25 可重放 + 账本连续性**）；在办/排队：`t110`（跳过点名）`t111`（超时上界）`t112`（12 处 `uses:` 固定 SHA） |

**一句话**：这一代把「**读数可不可信**」当成一等目标来打 —— 每一处「绿」都要求能被负控打红、每一处「未测」都要求写成未测。**目标①②均已达成**；收敛过程本身产出了本代最贵的一批发现：**一条永远为红的判定步会把真红淹没**（`rc = … && 0 || 1` 在 GitHub 表达式里恒为 1）· **warning 即错误** · **fmt 漂移是累积的** · **合法 YAML ≠ GitHub 可加载**（`run:` 块内注释里的非法 `${{ }}` 拒掉整个文件，PyYAML 看不见）· **e2e 隐性依赖操作者的活配置**（默认打活守护进程并**写活库**；本地绿、CI 红）· **测试里的平台假设**（Windows 路径写死 / **子串匹配**让「绿」变成巧合）· **无条件的跳过 = 永不执行的覆盖** · **未锚定的模式会造出不存在的 finding**（「2 处裸 `#[ignore]`」实为文档散文 `<code>#[ignore]</code>d`；与本代 C24 同族）。最后四条都是「**门禁必须声明哪一侧被跑过、断言的真值依赖什么**」的一手代价。

---

## 2. 各区域改前 → 改后（详见 `gen3-backlog.md` §E）

| 区域 | 读数 | 限定 |
|---|---|---|
| 召回 | **recall@1 0.4667→0.7333** · **MRR 0.6889→0.8185** · **nDCG@10 0.7682→0.8621**；分页无关性 2/13→15/15 | 活库只读、分层 gold |
| 召回（CJK） | CJK 字面腿 + 融合/重排接入；`score_kind` 线上形状 | 负控见 `gen2-recall-impl.md` |
| 图谱 | **G7 关系精度 0.7500→0.9149（43/47）** · **G5 多跳 0/20→0.70** · **G1 实体腿 1/23→13/23** | gold 夹具；**`GRAPH_PATHS=3` 最优性未获支持** ⇒ 只说可达 |
| 图谱（注入面） | **`<graph>` 块 0/5→3/5**，负控干净（106 vs 242 字符） | 事件读数 |
| Wiki | **`cite_coverage` 1.0**，负控（无锚点页）判 **failed** | 页级判据 |
| Wiki（新鲜度） | 正文/锚点判定**逐位一致**；`stale_since` 读路径非空且连读两次逐字相同 | **代价**：`/wiki/pages` 53 页 157.8→**317.6ms**（登记待优化） |
| 记忆（衰减） | `used=5,last_used=now → 0.500000` vs `used=0,last_used=now-30d → 0.058660` | 纯函数 |
| 记忆（判据性） | 误并 **0/20**、召回 **14/20** | **自造夹具**（人工 gold 属遗留项） |
| 记忆（写侧） | 软删后同内容重写 `Inserted(1)` 且可见；非法 supersede `Err` + 零行 + 审计 `supersede_refused` | HTTP 端到端；HTTP 面仍 500（R-1 待修） |
| 消费面 | MCP **14 工具全是到 HTTP 路由的一行薄代理** ⇒「两份实现」不成立 | 只读审计 + stdio 实例 |
| 安全 | 非 loopback 必须显式解锁，否则非零退出 + WARN 点名 | 已入 `AGENTS.md` 约定 |
| 领域层 | `RunStatus` active/terminal **类型上不可同时为真**；矩阵非法行 1→0 | 负控真红 |
| 存储层 | ~~25 个迁移里 20 个不可重放；删版本行 ⇒ 起不来或**静默跳过**~~ ⇒ **t98 后**：**25/25 可重放**（单元测试逐个重跑、`sqlite_master` 快照逐字节不变）；**删最高版本行 ⇒ 重启 `health=True`**（账本回 25/1/25）；**挖洞 ⇒ `health=False` + 逐字点名**（`version(s) [3] are missing while version 25 is recorded (24 row(s) present)`，逐版本表 `v1..v24` 点名拒绝 / `v25` 启动）；**合成 v18 旧库升级 ⇒ health=True、账本 18→25、遗留行与 `selected_by='agent:judge'` 完好** | 库副本实测。**中/低限**：0024 的**数据拷贝路径**只结构性走过（合成旧库 `distill_log` 0 行）；**中间缺行不自动补跑**（裁决：点名拒绝，`db-doctor` 类修复入口登记待办）；不覆盖 LanceDB 文件层与并发迁移 |

---

## 3. 闭合与在办（findings → 单 → 状态）

**已闭合（有独立验证或负控）**：A-3 网络暴露（t76，captain 独立复核 grep 0→13）· 面板命名空间矩阵（t59/t63）· 图证据进注入面（t58/t63）· 预算账本生产者侧（t60）· core 三态保真（t88，负控真红）· 记忆写侧两处静默失真（t72：**自己验到根** —— `UNIQUE` 跨墓碑生效 ⇒ 上游修法必要而不充分）· wiki 新鲜度两处（t74，判决「改变行为的单必须拥有钉住该行为的测试」）· design-audit 退出码载重（t97，副本树负控）· 发布路径三把锁（t100，真 `gh`+API 负控）· 迁移账本取证（t94，high）· 面板/存储/MCP-ACP 三份审计（t79/t94/t91）。

**在办**：`t110`（E2E 跳过点名 —— **读数的诚实性**，不是绿的条件）· `t111`（CI 超时上界）· `t112`（12 处 `uses:` 固定 SHA）· `t96`（消费面收口第 4 轮，**冻结**）· 状态机验收由 t88 补上互斥后仍未接线（见 §5 冻结）· **存储层中间缺行的显式修复入口**（`db-doctor` 类；判据见 `gen3-store-migrations-repair.md` §7）。
**本代新闭合**：`t98`（**迁移 25/25 可重放 + 账本连续性点名**，并顺手修掉 `0012` 回填会**改掉 judge 出身**的真数据风险）· `t109`（`claude` 子串巧合）· `t113`（永不执行的覆盖）· `t108`（平台幼稚夹具）· `t107`（e2e 不许猜目标）· `t106`（守卫加固 + 非法 `${{ }}` 检测）· `t104`（判定步假红）· `t105`（守卫审计）· `t85`（规格锚点）。

**未派/遗留**：R-1..R-4（记忆路由与 schema 岔口，R-3 裁决为**独立迁移轮次**）· T-2/T-4/T-5/T-6 的余项 · S3–S6、S9–S10（存储层）· C-4..C-8（策略编译器静默、无 run 级超时、三处 fail-open、级联不可辨、pipeline 丢字段）· M3/M4/M8–M10（MCP/ACP）· A1–A17 等更早的遗留项。

---

## 4. 质量门与纪律

**门禁（本代加固后）**：`fmt --all --check` · `clippy --workspace --all-targets -- -D warnings` · `test --workspace --no-fail-fast`（**逐 target 计数 + 退出码**，0 reporting target = error）· `cd panel && npm run build`（内含 i18n-check / `tsc -b` / e2e tsconfig / vite）· F1 守卫（workflow 引用的路径必须已跟踪）· nested-Result 守卫 · ignored-instrument 清单对账。

**纪律账（`gen2-integration-contract.md` 22 条 + 本代新增）**：读数只在**最终字节**上取 · **编译面/测试面/作用域三面分开写** · 负控必须能红 · 未测写成未测 · **门禁要么真跑要么明确拒绝**（`-CleanFirst` 空 SPEC 实例）· **跨 Windows→WSL 测退出码只能用 `$LASTEXITCODE`/`if`**（`echo $?` 会静默给 0）· **共享树不可编译必须立刻广播** · 跨文件前先确认树可编译 · **同一形状多处改动必须一起落地**。
**本代末段新增（都是吃过亏换来的）**：**上界必须按「绿」的运行取，不能按红的**（红 run 早期失败、后面 `skipped` ⇒ 标定会得到荒谬小值）· **推的人必须用 `numstat` 证明「我推的正是我说的那些字节」**（否则会把同伴在途改动卷进提交）· **固定动作 ≠ 固定工具链**（`uses:` 钉 SHA 而 `toolchain: stable` 仍浮动 ⇒ 必须**行内写明**这层不对称）· **护栏把「静默绿」变成「红」时，不许为了让绿回来而削弱护栏**（`5c0bc8f` 的 E2E 红即此例：它换来的是「再也无法在拿不到名字时报绿」）· **未锚定的匹配会造出不存在的 finding**（「2 处裸 `#[ignore]`」实为文档散文 `` `#[ignore]`d ``；与 `Select-String 'FAILED'` 大小写不敏感同族）· **变异窗口的看门狗不能依赖同一个工具调用返回**（t98 窗口 1 被 600s 上限掐住 ⇒ 变异体留了 ~10 分钟；窗口 2 用独立看门狗 ⇒ 5.1 秒）。

---

## 5. 冻结与结构性限制（重要）

平台的依赖规则组合导致**永久冻结**：失败依赖封锁下游；被封锁的 pending 任务**既不能重派也不能取消**；其 inScope 对所有人变禁区；而「摘掉失败依赖」又常被「与在途从属单重叠」挡住。
**当前冻结**：`t64`（预算账本接线 + F7 + D-11 契约同步）· `t86`（裸 `#[ignore]` 守卫，inScope 已缩到仅报告文件、路径已腾出）· `t89`（权限审计可辨，C-3）· `t90`（状态机住 `core` + 13 处接线，C-1）。**内容均已在本账与总账登记**，可在新会话干净重建。
**教训**：宁可**拆 inScope 路径**，也不要把关键路径挂在可能失败的任务后面。

---

## 6. 复现命令

```bash
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 check --workspace --all-targets
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 clippy --workspace --all-targets -DenyWarnings
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test --workspace       # 内部加 --no-fail-fast
cd panel && npm run build
bash .github/workflows/scripts/check-workflow-refs.sh      # 未跟踪脚本未入库时按定义红
cd panel && node tools/design-audit.mjs --self-test         # 495/495
```
**注**：`-CleanFirst` 必须带包名（或至少有一个 `-p <spec>`），否则 wrapper **明确拒绝**（exit 2）—— 这是为修「空 SPEC ⇒ 门禁根本没跑」那个缺陷加的。

---

## 7. 未取到 / 未验证（诚实登记）

- **已推送七次**（`0da0cb6` → `cb55073` → `877a909` → `cfb52b1` → `44636db` → `5d3adfd` → `e3a58ea`）。**已确认的判定**：`Panel (node)` 真跑 **success**（多次）；ubuntu 的 `Format`/`Clippy`/两个守卫步**已真正执行**（首次真跑分别暴露了编译错误、警告即错误、fmt 漂移，均已修），且**加固后的守卫在真实 CI 上 ✅**（`checked: 15` 的真实树，无误伤）；**ubuntu `Test` 已收敛为「恰好 1 个 target 失败」**（annotations 原文 `exit code 101 with 1 failing target(s)`；`gold-seed-idempotence.rs:373`，t108）；**E2E 的 Playwright 已在 CI 上真跑**：`5d3adfd` = **3 failed**/1 skipped/46 passed，`e3a58ea` = **4 failed**/1 skipped/46 passed ⇒ **失败集合不稳定**（同 spec 内残留状态依赖），已并入 t109 的判据；**判定步的假红已消除**（`e3a58ea` 的 `Test evidence` 报的是**真实 rc=101 + 1 failing target**，而不再是恒 1）。**仍未取得**：**windows 的真实测试判定**（见 §9 —— 唯一空白）。~~E2E 的 Playwright 是否复现 `consumption.spec.ts` 的 Wiki 覆盖率失败~~ **已解决**：那条红是 e2e 打到了**两天前的旧守护进程**（t107 已修，wiki 取证；当前字节上 `1 passed`）。
- `design-audit --check` 的**四计数**未取到（t97 只给 ≈6.5 min 估算）；t99 要补。
- `release.yml` 的**六个 SHA 固定尚未被任何真实运行验证**；上一次真实发布（09-17）在 `Create release` 失败（`<!DOCTYPE html>`）⇒ 加固只保证「红的不会被发」。
- 人工 gold（实体家族、真实蒸馏批次）与 `wiki_citations`/`episode_count`/`GRAPH_PATHS=3` 的依据仍在遗留清单。
- ~~迁移可重放修复（t98）与状态机接线（t90，冻结）尚未落地~~ ⇒ **t98 已落地**（读数见 §2 存储层行）；**状态机接线（t90，冻结）仍未落地** ⇒ 状态机仍是**能力缺口**。
- **新成本（本代自己制造的，如实记）**：t98 的「25 次迁移重放」集成测试让 **ubuntu 的 `Test` 从 156s 涨到 ≥7.9 分钟**（首次观测时仍在跑）⇒ 仍 **< `t111` 给的 30 分钟上界**，但余量**从 ×11 缩到 ×3.8**。⇒ 结论：**测试套件再长，上界必须重新标定**；`t111` 的判据（「按**绿** run 取」）依旧适用，但「绿 run」从此必须是**含 t98 之后**的那些。

---

## 8. 对外解释：账目上的几处「噪音」（避免被读成「坏了没人管」）

平台的任务列表里，有几行看起来像失败簇。**逐条说清它们是什么**（依据 = 任务标题与状态，凡属推断我都标明）：

- **`t45 → t46 → t47 → t48`（四轮「INT 集成补完」）**：这是**同一条路的四轮迭代**，每轮把范围收窄一次（第 1 轮：MCP 工具/面板四页/跨区读数/cli lint → … → 第 4 轮只剩 **P1 = G8 带数字 coverage** 这一个 blocker）。它们显示为「**failed without a follow-up repair**」，是因为**后继单由 captain 直接建立、没有走平台的 `repair` 链接**（平台据此认为「失败后没有接班人」），**不代表坏了没人管**。⇒ 读账时应把这一串当成**一次收敛过程**，不是四次独立失败。
- **`t40`（captain 自己的「I-C deferred 登记 + 规格复现命令勘误补齐」）被取消**：它属于**账目类工作**，内容已并入总账的 §A/§B 系列与各报告，**不产生代码**；取消它没有留下未处理的缺陷。
- **同类形状**：`t57` 等被标失败的单也多为「**第 N 轮收窄后仍未全中**」，其未交付项都已逐条登记（见 §5 与 `gen3-backlog.md` 的 §B 各波）。
- **另一类噪音**：**变异负控窗口**（t93/t81）会让其他人在几十秒内看到「真红」，本代出现过**一次误诊**；现已立纪律 C22（见 §4），窗口必须**先广播 + 限时 + 报告起止与恢复读数**。
- **第三类噪音**：**被平台依赖规则冻结的单**（t64/t86/t89/t90，见 §5）—— 它们会一直显示 `pending`，既不是失败也不是在办。

## 9. 目标②的收敛清单（**已全部落地**，2026-09-29）

| 红 | 根因 | 单 | 结果（有读数） |
|---|---|---|---|
| ubuntu `Test` | `gold-seed-idempotence.rs:373` 写死 `C:\…`，Linux 上反斜杠是普通字符 ⇒ 退化成单组件 | `t108` | ✅ 夹具改成平台中立（相对 + join；**无 `cfg(windows)`、无 `#[ignore]`**，四 case 一条不少）；**负控做在隔离树里**（`git stash create`→`archive`），恰红在 case ③ **且是在改后的夹具上** ⇒ **判别力未被削弱** |
| E2E `Playwright` | `failure-visibility.spec.ts` 假设有 `claude`；而 `cardOf` 是**子串**过滤 ⇒ **本机绿从一开始就是命中 `claude-code` 的巧合** | `t109` | ✅ 改成从 `/agents` 取 `kind==="runtime"`（`/runtimes` 实测 405）；无 runtime ⇒ **具名 skip**；改后**连续两次 `6 passed`/exit 0** |
| E2E 跳过点名 | `1 skipped` 却 `0 named`（作者自陈「未用真跑验证」，CI 反证） | `t110` | ⏳ **仍在办，且它是「读数诚实性」而不是绿的条件**：机制失效时它静默报 0，CI 照样绿 ⇒ **不许因为绿了就把它关掉** |
| 无上界 | `ci.yml`/`e2e.yml` 无 `timeout-minutes` ⇒ 默认 360 分钟 | `t111` | ⏳ 排队。校准：windows `Test attempt 1` 真值 **1087s（18.1 分钟）**，另一次 7.9 分钟 ⇒ 上界必须显著高于 18 分钟 |
| `uses:` 浮动 | 12 处未固定 SHA（含 `dtolnay/rust-toolchain@stable` 移动分支），违反仓库自定标准 | `t112` | ⏳ 排队（待 t111） |
| windows 判定 | 从未取得（5 次被自己的连续推送取消；`0da0cb6` 的 `success` 是吞退出码的假绿） | — | ✅ `e3a58ea`：`attempt 1` **success / 1087s**、证据步 success（**rc=0**）、`attempt 2` 正确 `skipped` |

## 10. 绿的时刻（`b1e0e9f`）—— 目标②达成

```
CI 36507120586 @b1e0e9f: completed/success
  Rust (ubuntu): Guard(paths) ✅ · Coverage declaration ✅ · Format ✅ · Clippy ✅ · Guard(nested-Result) ✅
                 Test ✅ · Test evidence ✅ · Ignored instruments ✅ · Ignored-instrument manifest ✅
  Rust (windows): ✅        Panel (node): ✅
E2E 36507120602 @b1e0e9f: completed/success（Doctor 7 ✅ · Panel E2E (Playwright) ✅ · E2E evidence ✅ · Upload ✅）
```
**这条绿自己声明了「什么没跑」**（本代的纪律，也是它区别于「看起来绿」的地方）：
- 全 CI **只有一步 SKIPPED** = `Test attempt 2 (only for the known build race)` —— **设计使然**（attempt 1 成功就不重试）；对照：失败的那些 run 里 `Ignored instruments CI can satisfy` 也被跳过（**级联**），这次它 **success**。
- ignored-instrument manifest 的原文：`cargo test --workspace -- --ignored --list` 找到 **15** 个，本 job 记满 **15 = 7 run + 8 declared**，8 条**逐条点名 + 理由**（`store` 3 条要真实库的迁移副本 · `graph` 3 条要真实图库副本 · `knowledge` 2 条要含 934 文档语料 + e5 模型的活 root）⇒ `skip accounting OK`。
- **E2E 侧零 skip**（`registry.spec.ts` 真跑 ⇒ 写路径被验过）。

**仍未闭合、不许因绿而掩盖**：**E2E 侧的「跳过点名」仍然失效**（`t110`）—— Rust 侧已做完（15=7+8），E2E 侧仍是「有数字没名字」；`t111`（无上界）与 `t112`（12 处浮动 `uses:`）也仍在排队。⇒ **本账不把「CI 绿」读成「纪律全部落地」**。

**推后逐条记录（每次推送都会取消前一条 run，故必须逐条写）**：
- `05755d0`（t109）E2E ✅ · `b1e0e9f`（t108）CI+E2E ✅ · `1516814`（t113）E2E ✅ **且算术闭合**：`51 passed`、零跳过（上一代是「46 passed + 1 skipped + 4 failed = 51」⇒ **同一批 51 个测试现在全过、一个不跳**）。
- ⚠️ **`5c0bc8f`（t110 增量）E2E ❌** —— 而这次红是**护栏按设计工作**：`no Playwright JSON report at …/t65/playwright.json -- this leg cannot name a skipped spec … a skip that cannot be named must never be reported as green`（步骤链其余全绿，**含 `Panel E2E (Playwright)` success**）。根因（待证实）：`PLAYWRIGHT_JSON_OUTPUT_NAME` 是**相对 `outputDir` 的文件名**、`PLAYWRIGHT_JSON_OUTPUT_FILE` 才是绝对路径，而 t107 之后 `outputDir` **每次运行独占** ⇒ **写的人与读的人用了两个路径假设**。⇒ 处置：修复单（同一处 `env:` 定义路径 + 三条护栏一条不削弱 + 报告把「未真跑」更新成「CI 已证明不工作 → 已修 → 待验证」）。**不把这次红藏起来**：它换来的是「再也无法在拿不到名字时报绿」。
- **待推**：`t111`（四个 job 超时上界；**判据「上界按『绿』的运行取，不能按红的」**）+ `t112`（12 处 `uses:` 固定 SHA，含 `dtolnay/rust-toolchain@stable` **移动分支**；**固定动作 ≠ 固定工具链**已行内写明）。

**判据提醒**：上表的「结果」都要求**成对读数**（改前→改后）与**能红的负控**；「CI 绿」只有在**这六项**都有读数之后才允许宣称 —— 现在它们是：五项有读数、一项（t110）明确写着**未完成**。

---

## 附：本账的维护

- 每有新的**独立验证**或**新读数**，追加一行到 §2/§3，并在 §E（`gen3-backlog.md`）同步来源与限定。
- **不许**把估算写进实测列；**不许**把「不可达的能力」写成达标；**不许**在红的状态下宣称 CI 绿。
