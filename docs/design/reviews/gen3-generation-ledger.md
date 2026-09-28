# 第三代总账（ruagent 记忆/知识/wiki/图谱/召回 + 单一消费面）

> **本文件是 captain 维护的总结底座**：所有数字都**带来源与限定**；**未取到/未验证**一律明写。
> 读数时间窗：2026-09-27 → 2026-09-29（UTC+08）。代码基线 `5db881d`（origin/main），本代成果**尚未提交推送**（见 §7）。
> 修订规则：只追加/就地更正，旧文字逐字保留。

---

## 1. 一页结论（截至本账落盘时）

| 目标 | 状态 | 依据 |
|---|---|---|
| ① 提交并推送到 `main` | **未完成** | `HEAD 0a39e5b` / `origin/main 5db881d`；本代 150 条变更仍在工作树（50 modified / 100 untracked）；`t66` 正在跑最后一条门禁（`cargo test --workspace --no-fail-fast`，pid 75120/72416） |
| ② `main` 上 CI 转绿 | **未完成**（但有明确红基线） | 最近一次 run `36272344732`：ubuntu **`Format` 红** ⇒ 同 job `Clippy`/`Guard`/`Test` **全 skipped**；windows **`Test` 红**；Panel 绿。两处红因在工作树里**已修**（fmt 已 CLEAN；过期期望已改），待推送后由 CI 判定 |
| ③ 持续发掘 → 实现 → 验证 → 评审 → 集成 | **在运转** | 101 张单；八轮只读审计产出的 finding 已全部登记（`gen3-backlog.md`），本代闭合见 §3 |

**一句话**：这一代把「**读数可不可信**」当成一等目标来打 —— 每一处「绿」都要求能被负控打红、每一处「未测」都要求写成未测。代价是本代**尚未推送**、CI **尚未绿**，且有三张单被平台的依赖规则永久冻结（§5）。

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
| 存储层 | **25 个迁移里 20 个不可重放**；删版本行 ⇒ 起不来或**静默跳过** | 库副本实测；修复单 t98 |

---

## 3. 闭合与在办（findings → 单 → 状态）

**已闭合（有独立验证或负控）**：A-3 网络暴露（t76，captain 独立复核 grep 0→13）· 面板命名空间矩阵（t59/t63）· 图证据进注入面（t58/t63）· 预算账本生产者侧（t60）· core 三态保真（t88，负控真红）· 记忆写侧两处静默失真（t72：**自己验到根** —— `UNIQUE` 跨墓碑生效 ⇒ 上游修法必要而不充分）· wiki 新鲜度两处（t74，判决「改变行为的单必须拥有钉住该行为的测试」）· design-audit 退出码载重（t97，副本树负控）· 发布路径三把锁（t100，真 `gh`+API 负控）· 迁移账本取证（t94，high）· 面板/存储/MCP-ACP 三份审计（t79/t94/t91）。

**在办**：消费面收口第 4 轮（t96：`log`→`rows` **四处同改含面板 api 客户端**）· 面板失败可见性（t84）· 真测量接入门禁（t99）· 迁移可重放（t98）· 状态机验收由 t88 补上互斥后仍未接线（见 §5 冻结）· CI 第一次绿（t66）。

**未派/遗留**：R-1..R-4（记忆路由与 schema 岔口，R-3 裁决为**独立迁移轮次**）· T-2/T-4/T-5/T-6 的余项 · S3–S6、S9–S10（存储层）· C-4..C-8（策略编译器静默、无 run 级超时、三处 fail-open、级联不可辨、pipeline 丢字段）· M3/M4/M8–M10（MCP/ACP）· A1–A17 等更早的遗留项。

---

## 4. 质量门与纪律

**门禁（本代加固后）**：`fmt --all --check` · `clippy --workspace --all-targets -- -D warnings` · `test --workspace --no-fail-fast`（**逐 target 计数 + 退出码**，0 reporting target = error）· `cd panel && npm run build`（内含 i18n-check / `tsc -b` / e2e tsconfig / vite）· F1 守卫（workflow 引用的路径必须已跟踪）· nested-Result 守卫 · ignored-instrument 清单对账。

**纪律账（`gen2-integration-contract.md` 22 条 + 本代新增）**：读数只在**最终字节**上取 · **编译面/测试面/作用域三面分开写** · 负控必须能红 · 未测写成未测 · **门禁要么真跑要么明确拒绝**（`-CleanFirst` 空 SPEC 实例）· **跨 Windows→WSL 测退出码只能用 `$LASTEXITCODE`/`if`**（`echo $?` 会静默给 0）· **共享树不可编译必须立刻广播** · 跨文件前先确认树可编译 · **同一形状多处改动必须一起落地**。

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

- **本代尚未推送**，因此：CI 是否第一次绿、windows `Test` 是否真的转绿、`evidence` 步计数是否非空、windows job 是否 skipped —— **全部未知**。
- `design-audit --check` 的**四计数**未取到（t97 只给 ≈6.5 min 估算）；t99 要补。
- `release.yml` 的**六个 SHA 固定尚未被任何真实运行验证**；上一次真实发布（09-17）在 `Create release` 失败（`<!DOCTYPE html>`）⇒ 加固只保证「红的不会被发」。
- 人工 gold（实体家族、真实蒸馏批次）与 `wiki_citations`/`episode_count`/`GRAPH_PATHS=3` 的依据仍在遗留清单。
- 迁移可重放修复（t98）与状态机接线（t90，冻结）尚未落地 ⇒ 存储层与状态机仍是**能力缺口**，不是已修。

---

## 8. 对外解释：账目上的几处「噪音」（避免被读成「坏了没人管」）

平台的任务列表里，有几行看起来像失败簇。**逐条说清它们是什么**（依据 = 任务标题与状态，凡属推断我都标明）：

- **`t45 → t46 → t47 → t48`（四轮「INT 集成补完」）**：这是**同一条路的四轮迭代**，每轮把范围收窄一次（第 1 轮：MCP 工具/面板四页/跨区读数/cli lint → … → 第 4 轮只剩 **P1 = G8 带数字 coverage** 这一个 blocker）。它们显示为「**failed without a follow-up repair**」，是因为**后继单由 captain 直接建立、没有走平台的 `repair` 链接**（平台据此认为「失败后没有接班人」），**不代表坏了没人管**。⇒ 读账时应把这一串当成**一次收敛过程**，不是四次独立失败。
- **`t40`（captain 自己的「I-C deferred 登记 + 规格复现命令勘误补齐」）被取消**：它属于**账目类工作**，内容已并入总账的 §A/§B 系列与各报告，**不产生代码**；取消它没有留下未处理的缺陷。
- **同类形状**：`t57` 等被标失败的单也多为「**第 N 轮收窄后仍未全中**」，其未交付项都已逐条登记（见 §5 与 `gen3-backlog.md` 的 §B 各波）。
- **另一类噪音**：**变异负控窗口**（t93/t81）会让其他人在几十秒内看到「真红」，本代出现过**一次误诊**；现已立纪律 C22（见 §4），窗口必须**先广播 + 限时 + 报告起止与恢复读数**。
- **第三类噪音**：**被平台依赖规则冻结的单**（t64/t86/t89/t90，见 §5）—— 它们会一直显示 `pending`，既不是失败也不是在办。

## 附：本账的维护

- 每有新的**独立验证**或**新读数**，追加一行到 §2/§3，并在 §E（`gen3-backlog.md`）同步来源与限定。
- **不许**把估算写进实测列；**不许**把「不可达的能力」写成达标；**不许**在红的状态下宣称 CI 绿。
