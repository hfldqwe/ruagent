# ruagent 设计文档

日期：2026-09-11 ｜ 状态：已与所有者逐段确认 ｜ 修订：v1（④ 待 KG/Karpathy 专项调研补强）

调研依据见 `docs/research/`（OpenViking / Multica+LoopX / ACP 生态 / Agno / Agent 工程实践 五份报告）。

---

## 1. 概述与定位

**ruagent 是本地优先、单用户的 agent engineering 平台**：一个常驻 Rust daemon 通过 ACP（Agent Client Protocol）编排多个 coding-agent CLI（Claude Code / OpenCode / DeepSeek Harness…），并提供统一知识库、分层记忆、集中 MCP 与 skill 管理、完整 run 可观测性。

- **定位**：自用为主，但架构不堵死产品化路径（单用户、无多租户/鉴权，但模块边界、配置与数据模型按可拆服务标准设计）
- **生态坐标**：MVP 三条目标线（Claude Code、OpenCode、dsh）今天即可全部经 ACP 接通；「ACP 原生多 agent 协作 × 中央知识库/记忆 × MCP/skill 统一管理」组合无竞品占据
- **协议策略**：全踩现成标准——ACP（接入）+ MCP（工具）+ SKILL.md（技能）+ AGENTS.md（指令），零协议发明
- **许可纪律**：OpenViking（AGPL-3.0）只借鉴设计不抄代码；Multica（Apache-2.0+附加条件）抄代码前须核对 LICENSE；本仓库自身 MIT OR Apache-2.0 双许可

## 2. 决策记录

| # | 决策 | 内容 |
|---|---|---|
| D1 | 定位 | 自用为主 + 产品化留位 |
| D2 | 交互形态 | CLI + 本地 Web 面板；daemon 为核心，CLI/面板均为同一本地 API 的瘦客户端 |
| D3 | 技术栈 | Rust 核心（daemon/ACP/MCP/知识/记忆）+ TypeScript 面板 |
| D4 | 协作 MVP | 直连 + 扇出对比 + 流水线；leader 路由与共享任务板通过 Task/Run+类型化边预留 |
| D5 | 路由 | 级联策略接口 `route(task, agent_registry) -> RoutingDecision`：显式 > 规则 > LLM 兜底（可插拔、默认关） |
| D6 | MVP 范围 | M1（接通）+ M2（协作）+ M3（记忆/知识）全量 |
| D7 | 存储 | 事件流→per-run append-only JSONL；可查询状态→SQLite(WAL)+单写者 actor+组提交；向量→嵌入式 LanceDB；Postgres 仅作为产品化触发后的后端替换（repository trait 屏蔽） |
| D8 | 权限 | 级联：确定性规则 > 审批 agent（不能批自己的 run、授权范围显式、高风险永远人审）> 人收件箱；fail-closed |
| D9 | 项目名 | ruagent |

## 3. 总体架构

```
┌────────────┐      ┌─────────────────┐
│ CLI (Rust) │      │ Web 面板 (TS)    │   ← 瘦客户端，无业务状态
└─────┬──────┘      └───────┬─────────┘
      │    本地 HTTP + WebSocket/SSE   │
      └──────────┬──────────┘
         ┌───────┴────────────┐
         │   ruagent daemon    │   Rust / tokio / axum
         │  ┌──────────────┐  │
         │  │ ACP 连接管理器 │──┼─→ spawn 子进程：
         │  │ (多 harness)  │  │    claude-agent-acp / opencode acp / dsh --profile acp
         │  ├──────────────┤  │
         │  │ 编排器        │  │   Task/Run 状态机 + 路由级联 + 三拓扑
         │  ├──────────────┤  │
         │  │ 知识/记忆引擎  │  │   摄取管道 + 分层记忆 + 注入契约
         │  ├──────────────┤  │
         │  │ MCP/Skill 注册│  │   集中配置 → 按需注入/分发
         │  ├──────────────┤  │
         │  │ 策略中心      │  │   权限级联 + 路由规则 + 成本策略
         │  ├──────────────┤  │
         │  │ 观测层        │  │   trace / token 成本 / run 事件流
         │  └──────────────┘  │
         └───┬─────────┬──────┘
       SQLite │         │ LanceDB          ┌────────────────┐
      (状态库)│         │(向量库，嵌入式)    │ daemon 同时是    │
             │         │                   │ MCP server 宿主 │→ 所有 agent 都能访问
        ~/.ruagent/ (配置 + 数据根目录)      └────────────────┘  记忆/知识/平台操作
```

要点：

- **daemon 持有一切状态**（ACP 连接、任务、记忆、注册表），崩溃重启从 SQLite 恢复；CLI 断线重连即可
- **daemon 自身是 MCP server 宿主**：记忆/知识查询、平台操作（查任务、领工作）经 MCP 暴露——任何 harness 无需改造即可消费（Agno 调研结论：服务外部 CLI 的正确接缝是 MCP/REST/上下文文件，不是 SDK 适配器）
- **存储零运维**：SQLite + LanceDB + JSONL，无外部服务依赖
- **Windows 一等公民**：进程管理（Job Objects、stdio 编码）、路径处理自第一天在 Windows 验证，CI 双平台

## 4. Agent 接入层

### 4.1 Agent 注册表（`~/.ruagent/config/agents.toml`）

```toml
[agent.claude]
harness         = "claude-code"
command         = "npx @agentclientprotocol/claude-agent-acp"
description     = "主力编码，擅长复杂重构与长上下文分析"  # 自由文本：LLM router 语义提示
model           = "opus"
reasoning_effort = "high"      # 思考等级：per-agent 默认值
context_window  = 200000
mcp_profile     = "default"
tags            = ["coding"]   # 可选：规则路由用轻量标签

[agent.dsh]
harness  = "dsh"
command  = "dsh --profile acp"
description = "快速便宜的第二实现"
model    = "deepseek-chat"
```

**能力卡三层分工**：

| 层 | 内容 | 消费方 |
|---|---|---|
| 机器参数 | model、思考等级、上下文窗口 | 适配器 → 映射到各 harness 原生配置（dsh `set_config_option` 等） |
| 语义描述 | `description` 自由文本 | LLM router 冷启动提示 |
| 统计层 | 历史成功率/成本/时长（自动累积） | 路由 + 面板；与声明并排展示（"实测 vs 声明"） |

### 4.2 Harness 适配器

`HarnessAdapter` trait，每 CLI 一个薄实现，唯一职责是编码该 CLI 的怪癖（Multica 模式）：

- `claude-code`：`npx @agentclientprotocol/claude-agent-acp` 启动；注意 adapter 的 session resume 语义
- `opencode`：原生 `opencode acp`；config 选项透传
- `dsh`：`--profile acp` 为 automation-only——权限应答完全依赖平台侧策略；单连接可多路复用并发 session；`set_config_option` 动态切模型/思考等级
- `mock`（内置）：脚本化假 agent，CI 测试基座（见 §12）

### 4.3 事件归一化与生命周期

- 所有 `session/update` 通知（message chunk、tool_call、plan、usage_update）→ 统一内部事件模型 → 一路追加 run JSONL、一路推 WebSocket（CLI/面板实时）
- 空闲 session 超时回收；子进程崩溃检测 → run 标记 failed、JSONL 保留、可重试；按 harness 有界并发
- **权限流**（详见 §9.2）：`request_permission` → 策略中心级联裁决 → 自动应答 / 审批 agent / 人收件箱

## 5. 编排层

### 5.1 数据模型

```
Task（持久意图）                Run（一次执行尝试）
┌────────────────┐            ┌────────────────┐
│ id, intent     │    1 ── N  │ id, task_id    │
│ status         │ ─────────→ │ agent, params  │
│ edges: []      │            │ status         │
│ created_by     │            │ workspace(隔离) │
│ (human/agent/  │            │ trace, cost    │
│  rule/schedule)│            │ result         │
└────────────────┘            └────────────────┘
```

类型化关系边：`depends_on`、`spawned_by`、`reviews`、`fanout_of`。

**状态机**：
- Run：`queued → spawning → running → (waiting_permission)* → completed | failed | cancelled`
- Task：`pending → in_progress → (blocked)* → done | cancelled`

### 5.2 三拓扑（同一套原语）

- **直连**：`T → R(agent)`；CLI 随口一句也隐式建 ad-hoc Task（一切进 trace，记忆有数据可喂）
- **扇出对比**：`T → R(a) ∥ R(b) ∥ R(c)`，面板并排 diff + 成本；选优：人挑（默认）/ 自动评分（测试、diff 统计）/ judge run（judge 本身是普通 Run，复用全部机制）
- **流水线**：`T1 → R1 ──handoff──→ T2 → R2`；handoff 契约 = 上游 result + 有界摘要 + 相关记忆注入（注入规范统一于 §7.3）

### 5.3 「一切皆 Run」与预留

judge、审批 agent、未来 LLM router 都是 Run（router 是输出 schema 为 `RoutingDecision` 的特殊 Run，产出直接派生子 Task）——一套 trace/成本/权限模型通吃，leader 路由免重构接入。任务板预留：Task 表 v2 加 `claimed_by + lease_expires_at` 即成认领协议。

### 5.4 路由级联

```
route(task, registry):
  1. 显式：CLI --agent / task.agent
  2. 规则：路由表（项目/路径模式/标签 → agent 组合）
  3. LLM router（若启用）：能力卡（声明+统计）→ 决策 + 理由
  兜底：default agent
```

每个决策**记录来源**（explicit/rule/llm/default），永远可审计。

## 6. 知识/记忆子系统（差异化核心，v1 设计；KG/Karpathy 补强见 §6.6）

**双子系统、一条存储脊柱**：知识库（投放的：文档/代码库/网页）与记忆（学到的：交互产物）分开建模，共享存储/检索/治理管道。

### 6.1 六类记忆存储（Agno 分类法改造为 agent/project 中心）

| 存储 | 存什么 | 键控 namespace |
|---|---|---|
| Profile | 关于**用户**的结构化事实 | `user` |
| Observations | 非结构化观察 | `user/project/agent` |
| Session Context | 目标/计划/进度**快照**（覆盖不追加） | `session` |
| Entities | 实体-事实-事件-关系（属性图） | `project/global` |
| Procedures | 蒸馏流程知识 → 进知识库同一向量库 | `project/global` |
| Decision Log | 决策 + 实际结果（喂能力卡统计 + 未来评测） | `agent/run` |

### 6.2 虚拟文件系统 `ruagent://`（OpenViking 灵感）

SQLite（结构）+ LanceDB（向量）之上盖 URI 寻址层，经 MCP 工具暴露 `ls / find / grep / search`：确定性导航 + 语义搜索双层。分层目录摘要（L0/L1/L2 式）控制浏览成本。**治理在存储层强制**：namespace 隔离、保留策略、写入校验为代码而非 prompt。

### 6.3 写路径（治理管道）

捕获模式（per-store 可配）：**Always**（run 后后台抽取，不阻塞；抽取 provider 可插拔，默认关闭——MVP 无 API key 也能全功能运行）/ **Agentic**（agent 主动调 `memory.write`）/ **Propose**（排队人审）。

管道：`候选 → 去重/supersession → namespace 校验 → 提交 + memory_diff 审计日志`（两阶段提交思想）。**避开 Agno token 陷阱**：抽取基于 run 摘要、行级定向写、永不全量加载重写。

### 6.4 读路径（注入契约）

- **Push（有限）**：run 启动注入有界带标签块（`<user_profile>` `<project_context>` `<relevant_memories>`，每块预算、截断可见、事实带日期）；per-run 工作目录写受管上下文文件（AGENTS.md 的 daemon 拥有段落，边界标记清晰）
- **Pull（主力）**：agentic RAG——agent 经 MCP 按需查 `memory.search / browse / knowledge.search`

### 6.5 暴露方式 = 一个 MCP server

daemon 内嵌 MCP server（rmcp）：记忆/知识查写 + 平台操作（查任务、领工作），经统一 MCP 配置注入每个 agent。这是外部 CLI 吃到中央记忆的**唯一接缝**。

### 6.6 KG/Karpathy 补强方向（专项调研进行中，落地后修订本节）

1. **记忆层级以 LLM OS 隐象重构**：上下文=RAM（注入即换页）/ MCP pull=按需读盘 / 知识库=冷存储
2. **Entities 升级为真属性图**：类型化节点/边 + 双时间线有效性（valid_from/valid_to，事件时间 vs 知悉时间）+ 多跳查询进 MCP
3. **混合检索融合**：向量 + 关键词 + 图遍历三路召回；社区摘要类 LLM 重活以分层目录摘要替代（避开成本陷阱）

## 7. MCP / Skill 管理

### 7.1 MCP 注册表（`~/.ruagent/config/mcp.toml`）

```toml
[mcp.filesystem]
command = "npx @modelcontextprotocol/server-filesystem"
args    = ["--root", "~/projects"]
inject_for = ["claude", "opencode", "dsh"]

[mcp.ruagent-memory]        # 内置：daemon 自身的记忆/知识 server
builtin = "memory"

[profile.minimal]
servers = ["ruagent-memory", "filesystem"]
```

- **注入是叠加层不是接管**：ACP `session/new` 携带 `mcpServers`，daemon 展开 profile 注入；外部 CLI 自身 MCP 配置一字不动
- **同步导出**：注册表导出为各 CLI 原生格式（`.mcp.json` / `opencode.json` / `config.toml`）——IDE 直连场景共享同一份注册表
- **健康检查**：定期 `list_tools` ping，面板显示；注入前剔除失败项并告警

### 7.2 Skill 管理

- 格式 = SKILL.md 标准；平台库（`~/.ruagent/skills/`）+ 项目级（`.ruagent/skills/`，并直接读 `.claude/skills/`）
- **分发器**：`ruagent skill sync` 按 agent 配置分发到各 harness 技能目录；兼容 `npx skills`（skills.sh）导入；来源记录、可刷新（identity-preserving）
- skill `allowed-tools` 与权限级联**合并计算**，skill 不能偷渡权限
- 平台自身提供「ruagent 操作手册」skill，教 agent 用平台 MCP 工具（对称设计）

## 8. 可观测性 / 安全 / 错误处理

### 8.1 可观测性

- **Trace 即回放**：JSONL 事件流 + SQLite 状态 = 完整历史；时间线含消息、tool_call、权限裁决（谁批/依据）、路由决策（为什么是它）、注入了哪些记忆块
- **上下文可观测**（Karpathy「读 transcript」产品化）：trace 按 agent 实际收到的样子渲染注入块——记忆注入是否过多/少/离题一眼可查
- **成本记账**：`usage_update` 按 run 落账 → agent/Task/天聚合；价格表估算；能力卡统计层复用

### 8.2 安全

- 权限级联见 §9.2；审批 agent 不能批自己参与的 run；高风险（删除/外发/凭据）永远人审；**fail-closed**（策略中心不可达一律拒绝）
- Run 级隔离：per-run 工作目录（git worktree，扇出互不踩脚）；凭证永不明文入库（env 文件 0600）
- **诚实边界**（文档第一页）：本平台**不做沙箱承诺**，边界是 daemon 的 OS 用户；要更强隔离自行上容器/专用账户
- skill `allowed-tools` 与权限级联合并计算

### 8.3 错误处理

| 故障 | 处理 |
|---|---|
| agent 子进程崩溃 | 检测退出/stdio 断开 → failed + JSONL 保留 → 一键重试（新 run + Session Context 快照恢复） |
| 卡死/超时 | 分阶段超时 → 优雅取消 → 进程组强杀（SIGTERM→SIGKILL；Windows Job Object） |
| daemon 崩溃重启 | SQLite 恢复状态；孤儿进程清理；中断 run 标记 interrupted + 可恢复 |
| 并发过载 | 每 harness 有界并发 + 排队；队满拒绝并明说 |
| 协议错乱 | 畸形 JSON-RPC → 记日志、run 失败、错误直白 |

## 9. 策略中心

### 9.1 路由规则（§5.4 的第 2 级）

配置式路由表：项目 / 路径模式 / 标签 → agent 组合；可版本化、可 debug（LoopX：路由是代码不是 prompt）。

### 9.2 权限级联（fail-closed）

```
request_permission 到达
 ① 确定性规则命中 → 自动应答（按 scope 记 allow_once / allow_always）
 ② 审批 agent（若配置且在授权范围内）→ 读 run 上下文 + 任务目标代为裁决
 ③ 人收件箱（默认兜底）
```

硬约束：审批 agent 不能审批自己参与的 run；高风险操作永远直达人；授权范围显式（可批工具类别 × 单 run 预算 × 每日上限）；每次裁决落 trace。**无人值守** = 规则覆盖常见 + 审批 agent 处理长尾 + 高风险留人。

## 10. 存储与数据模型

```
~/.ruagent/
├── config/            agents.toml / mcp.toml / policy.toml / 路由规则
├── data/
│   ├── ruagent.db     SQLite(WAL)：tasks、runs 登记、记忆元数据、实体图、
│   │                  决策日志、权限审计、能力统计、（v2：claims/leases）
│   ├── lancedb/       向量库（知识 + procedures）
│   └── transcripts/   run-*.jsonl（append-only）
├── skills/            平台 skill 库
└── workspaces/        per-run 工作目录（git worktree）
```

**并发设计**（D7）：唯一写者 = daemon；WAL 读写不互斥；单写者 actor（tokio mpsc 汇聚）+ 组提交；高频事件走 JSONL 纯顺序追加。**Postgres 触发器**（满足任一才换）：多 daemon 副本 / 远程访问 / 持续 >1k 写每秒 / 多用户。repository trait + sqlx 双方言保证届时为配置级改动。

**SQLite 核心表**：`tasks`、`task_edges`、`runs`、`agents_stats`、`memories`（含 namespace/validity/audit 字段）、`entities`、`entity_edges`、`decisions`、`permission_log`、`mcp_registry`、`schema_migrations`。

## 11. 项目结构

```
ruagent/
├── crates/
│   ├── core           领域模型（纯类型，零 IO）
│   ├── acp            ACP client + HarnessAdapter + 3 适配器
│   ├── mock-agent     脚本化 ACP 假 agent（测试基座）
│   ├── orchestrator   状态机 + 拓扑执行器 + 路由级联
│   ├── memory         六类存储 + 注入契约 + 摄取管道
│   ├── graph          实体属性图（邻接表 + 时序有效性）
│   ├── mcp            MCP 注册表 + rmcp server
│   ├── policy         权限级联 + 路由规则 + 成本策略
│   ├── store          repository trait + SQLite/LanceDB/JSONL
│   └── daemon         axum 组装 + WebSocket（薄壳）
├── cli/               ruagent CLI（clap）
├── panel/             Web 面板（TS/React/Vite）
└── docs/              设计 + 调研
```

## 12. 测试策略

1. **Mock ACP agent**（最重要资产）：Rust 实现的 ACP server，按脚本回放工具调用/权限请求/崩溃；CI 全绿无需真实 harness 与密钥
2. **属性测试注入契约**：任意记忆库状态下注入块永不超预算（proptest）
3. **金测试**：记忆库快照 → 注入内容字节级断言
4. 单元：状态机、路由级联、图查询
5. 真实三 harness 冒烟：feature-gated（`--features smoke`），本机手动
6. 评测留位：Decision Log 自第一天记 outcome

## 13. 里程碑与交付标准

| 里程碑 | 内容 | 完成标准 |
|---|---|---|
| M1 能跑 | ACP 三 harness、直连、会话/resume、MCP 注入、trace+成本、面板骨架、CLI 基本命令 | mock 全测 + 三 harness 真实冒烟通过 |
| M2 能协作 | Task/Run 模型、扇出对比、流水线、路由级联（显式+规则）、权限级联（规则+审批 agent+收件箱）、per-run 工作目录 | 全拓扑 mock 集成测试 + 权限级联单元/属性测试 |
| M3 有记忆 | 六类存储、注入契约、摄取管道、知识库+LanceDB、实体图、MCP server 暴露、skill 分发 | 注入契约属性+金测试；mock agent 经 MCP 读写记忆端到端 |

每个里程碑：`fmt`+`clippy -D warnings`+`test` 全绿、面板 `tsc` 零错、提交并推送。

## 14. 风险与开放问题

- **ACP v2 演进**：v2 draft 改动 file-change 结构——适配器层隔离，锁 v1 SDK
- **记忆抽取的 LLM 成本**：默认关闭 Always 模式；Agentic 写入 + 手动为主
- **审批 agent 的安全边界**：授权范围建模需在 M2 实现时细化；高风险白名单从保守开始
- **Windows 进程组语义**：Job Object 封装需早期验证（M1）
- **panel 与 daemon 的版本耦合**：panel 构建产物由 daemon 静态服务（rust-embed），API 加版本前缀 `/api/v1`

## 15. 参考

- `docs/research/2026-09-10-openviking.md` — 上下文数据库设计（viking://、分层摘要、两阶段提交）
- `docs/research/2026-09-10-multica-loopx.md` — 多 CLI 编排与确定性控制面
- `docs/research/2026-09-10-acp-ecosystem.md` — ACP 规范、支持矩阵、dsh 集成
- `docs/research/2026-09-10-agno.md` — 六类记忆存储、注入契约、resume 契约
- `docs/research/2026-09-10-agent-engineering-practices.md` — 标准收敛现状与 hub 层空白
- `docs/research/2026-09-11-knowledge-graph-karpathy.md` —（进行中）KG 记忆与 Karpathy 方法论
