# Super-agent 机制调研：模块、驱动力、分派、异步，以及「哪些是必要的」

> **性质**：设计输入文档，只读调研。不碰产品代码与配置，本文件是唯一写入。
> **问题**：用户要「只和一个主 agent 对话，所有事情由它管理、分发、监督，不用在多个窗口之间切」。
> 本文件回答两个问题：(a) DeepSeek Harness 团队公开的 Super-agent 到底由哪些机制构成、各自边界在哪；
> (b) 哪些是**缺了就不成立**的必要机制，哪些只是他们的工程选择 —— 以便判断我们这个仓库缺的是前者还是后者。
> **证据分级**（下文每条结论后带来源）：
> - **一手**：DeepSeek 官方原文（公众号，2026-10-31）、官方原型仓库的代码与 9 篇 numbered 文档（`000`~`008`）。
> - **二手**：转载、英译、解读。凡二手推断处一律标「二手」。
> - **本仓库**：`docs/design/async-entry-audit.md`（同一目标下的仓库侧只读审计）与源码符号。
> **本文不重复 `async-entry-audit.md` 已做完的仓库读数**，只在 §6.3 与 §7 引用它的结论。

---

## 1 一句话总结

这套架构解决的不是「模型不够聪明」，而是 **多任务的上限被人的注意力卡住**：Agent 仍以「单 Session」为工作单位，
一个人同时只能跟进 3~5 个活跃 Session，于是单任务的上限由模型决定、多任务的上限由人决定
（[原文 §1](https://mp.weixin.qq.com/s/zoS7GlReWPhfCjENGxXCdg)）。它的做法是把 Session 从「产品单位」降级为
「实现细节」，用 **双层职责分离 + 事件驱动 + 异步多信道交互 + 计算/状态/环境分离 + 外部 Watchdog** 五个模块
（原文 §1 末尾与 §9 的同一句话），让系统接管会话管理与上下文同步，人只保留两件事：**决策与验收**
（[原文 §9](https://mp.weixin.qq.com/s/zoS7GlReWPhfCjENGxXCdg)）。

一句话的可执行版本：**把「谁在什么时候被叫醒、拿什么上下文、干完怎么汇报」从人的动作变成系统的数据结构。**

---

## 2 模块清单与职责边界

### 2.0 先纠正一个提问方式

原文**没有**把系统切成「任务队列 / 状态存储 / 事件总线 / 恢复重试」这样的模块。它给的是**五个模块**
（双层架构、事件驱动、面向人的交互、计算状态环境分离、Watchdog），每个模块内部再由若干机制实现
（[原文 §1 末尾](https://mp.weixin.qq.com/s/zoS7GlReWPhfCjENGxXCdg)）。你问的那几块，在**原型**里是有的，
但它们的名字和边界是：`Store`（单 SQLite + 目录锁）、`Outbox`（回执与发送）、`Effects / stored work`（先存后做 + 退避）、
`Passes`（每 2 秒一次 + 命令触发）、`DomainEvents`（**进程内**枚举，不是外部队列）。见
[007 Architecture](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/007-architecture.md)。
所以「事件总线」这个说法要小心：原型没有跨进程总线，只有「Engine 是唯一写者 + 领域事件仅供 Runtime 转通知」。

### 2.1 清单（原文口径 + 原型落点）

| # | 模块 | 职责边界（做什么 / 明确不做什么） | 来源 |
|---|---|---|---|
| 1 | **Router 层**（原型：Main Session） | 承接**所有入口**的人类交互；是全局的序点（Sequence Point）、协调点、记忆；**不亲自执行具体任务**，甚至可以不配 Bash；只处理**任务之间**的高层关联与分派，**不介入单任务内部拆解** | [原文 §2.1](https://mp.weixin.qq.com/s/zoS7GlReWPhfCjENGxXCdg)；原型侧：主 Session + 预设 `dsh-superagent-router`、工具 `orchestrator_route`，[007](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/007-architecture.md)、[004](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/004-prompts.md) |
| 2 | **Worker 层**（原型：Executor Session） | 上下文隔离、挂**完整执行工具**、专注单一任务；**不与人类直接交互**，推理与工具调用不出现在外部信道，只向 Router 汇报；同层 Worker 之间**不需要直接通信**；Worker 内部可以再开 Subagent / 自身是 Agent Team，对上层透明 | [原文 §2.2](https://mp.weixin.qq.com/s/zoS7GlReWPhfCjENGxXCdg)；原型：每个 Ticket 一个 `worker-<ticket id>` Session + 一个目录，[003](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/003-execution.md) |
| 3 | **Source** | 事件流。kind 可复用、instance 是一份配置（是否运行、监听什么）；**持有自己的读取位置**（重启不能重放）；定义「什么算一个事件」，包括只在时间上成立的条件；把 Source 自己的词（message/chat/notification）翻译成 Trigger | [001 Domain](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/001-domain.md) |
| 4 | **Trigger / Thread / Correlation / Channel** | Trigger：不可变事件 + Source 元数据，是**审计与幂等**的单位；Thread：时间序的 Trigger 流，**同一时刻最多一个 Eval 在跑**；Correlation：只加上下文、**永不用于路由**；Channel：Thread 的写侧（回执、回复、PR、review 回复） | [001](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/001-domain.md) |
| 5 | **Eval（= Gate + Assessment）** | 对**一批** Trigger 做判断，输入是「Trigger 批次 + Thread 快照 + 该 Thread 上所有 Ticket（含已结束）+ 当前 Policy」，输出一个 **Decision**；自身**不持有状态** | [001](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/001-domain.md)、[002](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/002-authorization.md) |
| 6 | **Ticket / Mandate / Assignment / Subscription** | Ticket：**授权**的工作单元，**自有状态机**（open → ended，可被人的消息 resume），不跟随执行器状态；Mandate：accept 授予的东西（Policy 版本、范围、强度、命中的规则、发起人身份），**不可变**，只有「人 stop」和「复核失败」能动它；Assignment：Ticket 与**一次** Executor 实例的绑定，一个 Ticket 生涯里有很多个；Subscription：Ticket 与 Thread 的多对多关系，**决定后续事件直通给谁** | [001](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/001-domain.md)、[002](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/002-authorization.md) |
| 7 | **Policy** | 版本化的准入规则 + 每个 Source 自己的那一份（白名单、同步、bot 名单、准入表）；**只决定「谁能发起工作」**，不决定「执行者能拿结果做什么」（后者是提示词） | [001](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/001-domain.md)、[002](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/002-authorization.md) |
| 8 | **Engine** | **唯一写者**：Triggers / Threads / Decisions / Tickets / Assignments 只由它写；**每个命令一个事务**；对外只暴露按角色裁剪的接口（`EngineForSources` / `EngineForMainSession` / `EngineForPeople` / `EngineForSettings` / `EngineForExecutor`）；内部五个 part：Intake / Evals / Tickets / Assignments / Replies，依赖单向 | [007](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/007-architecture.md) |
| 9 | **Store** | 一个 SQLite（`orchestrator.sqlite`）+ **目录锁**（防第二个进程迁移/写同一状态）；分组为域仓储（`atomic`/`triggers`/`threads`/`tickets`/`decisions`）、只读查询（`read.*`，永不写）、Source 元数据、Settings 与**编号版本** | [007](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/007-architecture.md) |
| 10 | **Outbox** | Engine 的 `Outbox` 端口，也是**回执的唯一写者**：`send` 在一个事务里「记录消息回执 + 排入回复 effect」；effect 由 `SourceRegistry.send` 发出；Source 关着就等，会话没了/没人听就丢弃 | [007](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/007-architecture.md)、[003](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/003-execution.md) |
| 11 | **Passes（内部心跳）** | **同一时刻一个 pass，每 2 秒一次**，命令也能触发：先跑到期的 effects，再 `Engine.evaluateDue`；Source 的 Trigger 经由它进入 Engine | [007](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/007-architecture.md) |
| 12 | **Effects / stored work** | 「决定之后要做的事」（转发候选、启动 Assignment、投递 Input、发消息）**先落库、后执行**；失败按退避重试；同类保持顺序 | [003](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/003-execution.md)「Stored work」、[007](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/007-architecture.md) |
| 13 | **Notices / Relays** | 把领域事件与 Worker 活动转成**主 Session 的消息**：`eventNotice`（领域通知）、`reportRelay`（问题立刻转、普通进展留在 Ticket、其他报告转发）、`followupRelay`（列出哪些 Ticket 拥有人类 follow-up）；一轮进行中到达的 relay 在**下一个 step** 合并投递 | [007](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/007-architecture.md)、[004](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/004-prompts.md) |
| 14 | **Runtime** | 组装根：锁状态目录、开 Store、装配各组件；启动顺序 = 管理路由 → Setup → 装 Session/Executor 钩子 → 确保主 Session → Policy 同步 → **恢复被打断的 Assignment** → 起 passes 与 effects | [007](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/007-architecture.md) |
| 15 | **Panel** | 只回答三个问题：有没有事需要我 / 系统是否健康且在做对的事 / 一件工作的历史；**健康状态不占地方**，只列「等人」的东西 | [005 Panel](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/005-panel.md) |
| 16 | **Health + status 脚本** | `GET state` 里按责任分组的健康条目（runtime / models / evals / assessment / launch / delivery / relay / notice / reply / worker / github / feishu / history / `sync:<id>`）；`scripts/dsh-superagent status` 用 **退出码** 表达四类状态 | [006](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/006-deployment.md) |
| 17 | **Versions（提示词/设置版本）** | 每次保存产生**不可变版本**（含 revision、前驱、时间、来源）；**Session 在每个模型请求时读最新版本**，保存不重启、不取消、不唤醒 | [004](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/004-prompts.md)、[007](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/007-architecture.md) |
| 18 | **外部 Control Loop（Watchdog）** | 在**主系统之外**独立配置的一个 Session，**定时消息驱动**；既处理基础设施故障，也**由模型判断** Worker 有没有实质进展、方向是否偏；本身有完整工具，所以人可以用自然语言问原因、也可以让它**在线改主系统行为** | [原文 §6](https://mp.weixin.qq.com/s/zoS7GlReWPhfCjENGxXCdg)；原型落地方式见 §3.4 |

### 2.2 边界怎么划的（三条判据，比模块名有用）

1. **谁写状态**：只有 `Engine`。Executor、Source、Outbox、Panel 全部通过端口回调，不直接写库
   （[007](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/007-architecture.md)）。
2. **谁拥有「授权」**：只有 `Mandate`，且只有三种操作能改它 —— accept 创建、人 stop 撤回、复核失败撤销
   （[002](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/002-authorization.md)）。
   「执行者能不能合并 PR」这种问题被明确排除在 Policy 之外，属于提示词（同上）。
3. **谁拥有「任务状态」**：只有 `Ticket`，**不跟随执行会话**
   （[001](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/001-domain.md) 的「Rejected concepts」
   里写着 `Ticket state that follows the DSH Session` 被拒绝，理由是「一个 Ticket 可以被交接、恢复，或被多个 Executor 执行」）。

---

## 3 「驱动力」是怎么换的

### 3.1 原文的说法

> 「要让 Super-agent 具备**主动**性，首先需要改变系统的驱动方式。驱动 Agent 的主要输入不再是人类在 Session 中
> 逐条键入的提示词，而是环境中的半结构化事件。」
> —— [原文 §3 开头](https://mp.weixin.qq.com/s/zoS7GlReWPhfCjENGxXCdg)

落地成两条互补的驱动：

- **事件驱动**（环境 → 系统）：Trigger 进入 → 过滤 → 路由 → 进 Session。
- **定时驱动**（时间 → 系统）：在主系统之外，用**定时消息**驱动一个 Watchdog Session（原文 §6）。

### 3.2 事件驱动：两级过滤（Fast Path / Slow Path）

- **程序层（Fast Path）**：来源、群组范围、用户/机器人白名单等确定性条件，用代码做准入检查，
  对每个来源分别配置「仅响应明确请求 / 作为候选 / 直接忽略」，**在调用模型之前滤掉大部分噪音**
  （[原文 §3.1](https://mp.weixin.qq.com/s/zoS7GlReWPhfCjENGxXCdg)）。
- **语义层（Slow Path）**：通过准入的候选交给 LLM，按**提示词写的策略**判断要不要行动；语义层同时是
  「面向人类的自然语言扩展点」—— 加一条策略就是改提示词（同上）。

原型的对应物更硬：`gate()` 是确定性函数，`assessmentOutcome()` 把模型判断转成 Decision；准入表按**接收范围**分格，
每格三选一（explicit / inferred / ignore）；`accept mode` 三档 `off / observe / auto`，**默认 observe**，
此时「模型认为该接」只记一条「would accept」，不真的接
（[002](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/002-authorization.md)）。
这条默认值很值得抄：**主动性先以「只记录」上线，再切 auto**。原文只说语义层是"面向人类的自然语言扩展点"
（[§3.1](https://mp.weixin.qq.com/s/zoS7GlReWPhfCjENGxXCdg)），"默认 observe、先记录不执行"是**原型的默认值**
（[002](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/002-authorization.md)），
000 的运营建议也是"先把主动性只记下来"（[000](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/000-philosophy.md)）。

### 3.3 事件路由：订阅与直通（这是「不用切窗口」的技术核心）

- Router 按某个 Thread 的**初始事件**建 Worker 后，Worker **直接订阅**该 Thread；
- 之后同一 Thread 的事件**由程序直投 Worker**，不再经过 Router（Fast Path）；
- 未被订阅的事件仍由 Router 观察与分派（Slow Path）；
- Thread ↔ Worker 是**多对多**：一个 Worker 订阅多个 Thread，一个 Thread 被多个 Worker 订阅，于是多个 Worker
  在同一 PR/话题里**不需要点对点通信**也能看见彼此的操作；输出侧仍由 Worker 汇报、Router 统一回复。

（[原文 §3.2](https://mp.weixin.qq.com/s/zoS7GlReWPhfCjENGxXCdg)；原型的 `Subscription` 语义见
[001](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/001-domain.md)：订阅扇出**不检查发送者**、
**不授予任何新权限**，权限始终由 Mandate 限住。）

**注意一个反直觉的实验结论**：去掉直通后，任务完成时间**基本不变**，但 Router 需要做的决定 **+30%**、
读缓存 token 从 2323 万涨到 3177 万（**+37%**）。即直通是**省 Router 的钱**，不是**提速**
（[原文 §7.1](https://mp.weixin.qq.com/s/zoS7GlReWPhfCjENGxXCdg)）。这条直接决定了它在 §6 里的判定。

### 3.4 Watchdog：原文的接口 vs 原型的接口

**原文（设计要求）**：

> 「我们在主系统外部配置了一个独立的 **Watchdog Session**，用**定时消息**驱动它，构成外部的 Control Loop。
> Watchdog 除了处理接口报错、状态恢复失败等基础设施故障，还会**由模型判断各个 Worker 是否有实质进展、
> 方向是否偏离整体目标**。发现异常时及时介入修复，遇到外部偶发故障则持续复查直至恢复正常。」
> 「由于 Watchdog 本身也是一个具备完整工具能力的 Agent Session，维护者可以**用自然语言向它询问故障原因**，
> 也可以让它**在线修正主系统的行为，例如增删提示词、调整触发规则**。」
> —— [原文 §6](https://mp.weixin.qq.com/s/zoS7GlReWPhfCjENGxXCdg)

原文**没有**给出：定时消息的具体文本、巡检周期、健康判据、它凭什么「修复」。这些只能从原型拿。

**原型（实现）**：一个「Hourly check」，一个**每小时**的定时任务，提示词是一段自然语言 + 分支规则，
**建在你自己常用的那个 dsh profile 里，不在 `superagent` profile 里**，理由是「它必须在 Agent 服务挂掉时还能跑」。
文本按原文照抄（节选，[006](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/006-deployment.md)）：

```text
Check dsh-superagent: run `scripts/dsh-superagent --profile superagent status` in <checkout>.
Exit 0: do nothing and say nothing.
Exit 2: do not restart; tell me each part under `errors` with its detail, unless you already told me about the same ones.
Exit 3: do not restart; tell me what `setup` says setup needs, unless you already told me.
If it reports manually_stopped: true, leave it stopped.
Otherwise (exit 1) run `scripts/dsh-superagent --profile superagent --respect-stop restart`, which exits the same way:
0 means it recovered, so say nothing; 2 and 3 are reported as above; if it exits 1 or fails otherwise, read the newest
.artifacts/dsh-superagent-superagent-*.log and the Troubleshooting section, fix what you can, and tell me what was wrong
and what you did.
```

**所以「Watchdog ↔ 主系统」的接口，只有三样东西**（这是本文最实用的一条）：

1. **一个命令 + 退出码**：`status` 返回 `0` 正常 / `1` 没跑·跑了两份·不答·运行时错（可重启）/ `2` 部分组件错
   （重启没用）/ `3` 在等 setup（重启没用）。Source 还在启动算正常（[006](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/006-deployment.md)）。
2. **一份结构化细节**：`errors`（每个出错组件 + detail）、`setup`（缺什么）、`health`（按责任分组的条目）、
   `load`（`running` / `queued` / `maxWorkers`）、`paused`、`manually_stopped`（同上）。
3. **一个只读日志目录**：`.artifacts/dsh-superagent-<profile>-<date>-<time>-<pid>.log`，保留最新 20 份（同上）。

注意 `--respect-stop`：只有人的 `stop` 写 stop marker，`start` 清 marker，于是**人的主动停止不会被自动恢复推翻**，
而一次超时的 restart 仍能被自动救回来（同上）。这是「自愈」与「不要自作主张」之间那条线，值得单独记一笔。

### 3.5 它怎么判断主系统「健康」

分两层，只有第一层在原型的 `status` 里：

- **基础设施层**（有）：进程在不在、跑没跑两份、`runtime`/`models`/`evals`/`launch`/`delivery`/`relay`/`notice`/`reply`/
  `worker`/各 Source 的健康条目（[006](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/006-deployment.md)）。
- **任务进展层**（原文要求、原型**没有**专门信号）：原文要判「Worker 有没有实质进展、方向是否偏」，
  特别指出「很多异常不是进程崩溃，而是模型陷入无效循环或偏离目标」
  （[原文 §6](https://mp.weixin.qq.com/s/zoS7GlReWPhfCjENGxXCdg)）。原型能提供的**间接**材料只有 Ticket stage
  （`person/blocked|failed`、`idle`、`working`）、最新 Report（六值）、`worker` 健康条目、以及 `evals`
  （近一小时未解决的 `unavailable`）。**没有任何字段回答「这个 Ticket 多久没有新 Report 了」**
  （[003](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/003-execution.md)、[006](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/006-deployment.md)）。
  二手解读也指出：原型的 status/restart「主要处理进程、配置与运行时错误」，而「重启服务」和「判断任务是否在推进」
  是两种能力（[offloop 分析](https://offloop.org/learn/deepseek-super-agent-design)，二手）。
  **这是本文认为最重要的一处「原文有、原型没有」的缺口，见 §6 与 §7。**

### 3.6 「在线修正主系统行为」改的到底是什么（原型里是有具体 API 的）

原文只说「增删提示词、调整触发规则」（[§6](https://mp.weixin.qq.com/s/zoS7GlReWPhfCjENGxXCdg)）。原型的写面是：

| 手段 | 接口 | 有效范围 | 来源 |
|---|---|---|---|
| 改提示词 | `PUT prompts`（团队三段落 + 覆盖内置 part + 团队 skills） | **下一次模型请求就生效**，不重启；每个 Session 用过哪些 revision 都有记录 | [006](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/006-deployment.md)、[004](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/004-prompts.md) |
| 改触发/准入规则 | `PUT settings/policy`（rules、白名单、准入表、accept mode） | 版本一变就**作废在跑的 Eval**、把批量 Trigger 重排，并给**受影响**的 live Ticket 投一条 policy-changed Input | 同上、[002](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/002-authorization.md) |
| 改执行侧 | `PUT settings/executor`（model、maxWorkers、目录、prepare、禁止命令） | 下一次 Assignment 生效；已有 Session 保留原 model | [006](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/006-deployment.md)、[003](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/003-execution.md) |
| 刹车 / 放行 | `POST executor/pause` | 停派发、新 Eval、超时、投递、回复；**不打断**正在跑的 turn | 同上 |
| 救人 | `POST tickets/<id>/retry`、`POST tickets/<id>/stop` | 与面板同权 | 同上 |

两条要点：

- 写权限**不等**：管理 API 的保存「拥有面板的权限」，能改执行环境与账号字段，而**主 Session 的工具改不了这些**
  （[006](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/006-deployment.md)）。
  也就是说，一个 Watchdog 若用管理 API 修东西，它的权限**比 Router 大** —— 这是设计上的一个真实风险点（本文分析）。
- 能「在线修正」的前提是**版本化**：不可变版本 + `base` 记录 + 升级时对「被覆盖的默认值」标 `Default updated`
  并让人选「保留我的 / 用新默认」（[004](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/004-prompts.md)）。
  没有版本与回滚，"让模型改提示词"就是把主系统交给一个没有 undo 的人。

### 3.7 Watchdog 自己失败/被误判时怎么恢复

- **它跑在主系统外**（另一个 profile / 另一个进程），所以主系统挂了它还在（[006](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/006-deployment.md)）。
- **它的动作是幂等的**：`--respect-stop restart` 只在不该停的时候重启；`stop` 才写 marker，`start` 清 marker（同上）。
- **它被要求闭嘴**：exit 0 时「do nothing and say nothing」；同一类错误「已经说过的不要说第二遍」（同上）。
- **它自己也可能错**：二手解读明确把「Watchdog 有权改主系统提示词」列为**新的故障点**
  （[腾讯云解读](https://developer.cloud.tencent.com/article/2774420)，二手）。原型没有为它设独立的审计/回滚路径，
  只有版本历史（本文据 [004](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/004-prompts.md) 推断）。

---

## 4 分派：Router → Worker 的契约

### 4.1 输入：Router 收到什么

- 一批候选 Trigger（同一 Thread 的排队 Trigger 会合并成**一批**，见 §4.5 的批处理）；
- 每个候选带着**Thread 快照**、该 Thread 上**所有 Ticket（含已结束）**、以及**当前 Policy 版本**
  （[001](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/001-domain.md) 的 Eval 行）；
- 一条硬约束：**Router 只转发「原始 Trigger + 路由事实」，不做技术判断**；路由备注**最多 30 词**；
  技术细节由 Executor 自己读原始来源（[004](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/004-prompts.md)
  的「Separate roles」）。

**这回答了你问的「输入是完整背景 + 任务要求 + 上游输出吗」：不是。** 上游输出不进 Router 的上下文，
只有「事实」进（例如另一个 Ticket 的结论可以用一条 ≤100 词的 note 传过去，且**该 note 不授予任何权限**，
[004](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/004-prompts.md)）。

### 4.2 输出：Decision，一次调用多条

- 工具：`orchestrator_route`，**一次调用可以带多条** accept / attach / stop / ignore，**逐条独立生效**，
  一条被拒或字段缺失不影响其他（[002](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/002-authorization.md)）。
- Decision 四值：`accept / stop / ignore / unavailable`
  （[001 词表](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/001-domain.md)）。
- accept 是**原子**的：Ticket + Mandate + Assignment + Subscriptions + 回执消息在**一个事务**里创建
  （[007](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/007-architecture.md) 的 `decide`）。
- `attach` 有硬条件：消息必须**明确指向**某个 Ticket 的 PR / Issue / 源消息 / 回执，或指向「恰好一个」匹配的最近变更；
  **话题相似不够**（[004](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/004-prompts.md)）。
  这条是防止 Router 靠语义猜测乱派的唯一防线。

### 4.3 「怎么选 worker」——这里要纠偏

原文说 Worker 层「通常配置能力最强的模型」（[§2.2](https://mp.weixin.qq.com/s/zoS7GlReWPhfCjENGxXCdg)），
**但原型里没有「从池子里挑一个 worker」这回事**：

- accept 一个 Ticket → 为它**开一个** Executor Session（命名 `worker-<ticket id>`）+ **一个目录**
  （[003](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/003-execution.md)）；
- 用哪个模型由 `settings/executor.model` 决定，**全局一个**，不是按任务挑；已有 Session 保留创建时的模型（同上）；
- 并发不是"挑选"，而是**槽位**：`maxWorkers` 个槽，Ticket 从 launch（含 prepare）起占一个，人的消息/状态查询可以
  占**额外 1 个**槽，新活排队等槽（同上）。

也就是说：**原型分派的是「活」，不是「人」。**「选 worker」在我们这种「多 agent 注册表」的系统里才是真问题，
而他们的答案是「按 Ticket 开新会话」，把选择问题消掉了（本文分析）。

### 4.4 失败 / 超时 / 输出不合格怎么处理

Worker 侧用 **Report 六值**表达：`progress / waiting / ready / blocked / failed / dropped`
（[001 词表](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/001-domain.md)）。处理规则：

| 情形 | 处理 | 来源 |
|---|---|---|
| `blocked` / `failed` / 起不来 / 报错 | 记 `failed`；**停止自动投喂**（CI 与 bot 消息只记录不投递、不唤醒）；工具仍可用，人可以继续；人的消息、下一个非 blocked/failed 的 Report、或 retry 才解除 | [003](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/003-execution.md) |
| 模型/网络**瞬时**故障（408/429/5xx、连接被拒/重置/超时） | **先重试三次，退避递增**；launch 重跑，turn 失败的 worker 被告知「继续」；**prepare 命令失败不重试** | 同上 |
| launch 失败（非瞬时） | **不自动重试**，直接 fail 这个 Ticket；只有人的 retry 会重开 | 同上 |
| 输出不合格（Router 侧：Eval 超时） | 超时从**候选送达主 Session** 起算；过期就重排队，连续两次；**第三次记 `unavailable`**；`evals` 健康条目上报近一小时未解决的 | [002](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/002-authorization.md) |
| 重启打断 | 每个运行中的 Assignment 以 "host restarted" 结束，**换一个新 Assignment 带 recovery note 重开**（要求先查目录、git 历史、GitHub，再重复任何外部动作），**最老的 Ticket 优先** | [003](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/003-execution.md) |
| 后台上限 | 一轮结束后后台工作最多 **30 分钟**，到点杀 shell job 并报 `waiting`（subagent 允许继续） | 同上 |

### 4.5 结果怎么回到 Router 与用户

- **Worker → Router**：Report 走 `reportRelay`：**问题（human_action）立刻转发**，普通 `progress` **留在 Ticket 上**、
  不打扰主 Session，其他报告转发（[007](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/007-architecture.md)）。
- **Router → 人**：所有对外写都走 Outbox 的四种 purpose —— `acknowledgement / answer / result / attention`
  （[007](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/007-architecture.md) 的 `reply(request)`）。
  于是「谁说了什么」是**可审计的一组记录**，不是一个自由文本通道。
- **「何时该说话」是硬规则**：只有在「人必须行动」或「人能用到结果」时才说话；聊天默认一句话，**硬上限 100 词**
  （[004](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/004-prompts.md)）。
  原文把这条列为对面信道定制的一部分（[§4.2](https://mp.weixin.qq.com/s/zoS7GlReWPhfCjENGxXCdg)）。
- **路由层的批处理**：上一轮推理期间积压的 Trigger 合并进下一次 LLM 调用。实测 Trigger 到达率从 1/分钟升到 40/分钟，
  batch 1.0 → 2.8，单次请求 2.5s → 3.9s，**单个决定分摊 6.4s → 1.6s**
  （[原文 §7.2](https://mp.weixin.qq.com/s/zoS7GlReWPhfCjENGxXCdg)）。这条说明「Router 会不会成为瓶颈」的答案是
  「在团队规模下不会」，**但它依赖"能合并"这个前提** —— 要合并，候选就必须先进一个队列而不是直接进对话。

---

## 5 异步

### 5.1 原文：异步 ReAct Loop（`start()` / `stop()`）

经典 ReAct Loop 里 `run()` 是同步的，循环阻塞到工具返回，期间无法响应用户。改法是：

- 把耗时命令与子任务调用换成 **`start()` / `stop()`**；
- `start()` **立即返回 "Started"**，循环不再等它；
- 后台工具完成后，结果**作为一条新的事件消息投递进会话**；
- 模型可以按上下文调 `stop()` 终止跑太久的后台命令；
- 新输入在**当前推理步结束后**就被处理，不必等后台工具。

原文明确：**「这一异步任务机制已经在 DeepSeek Harness 中实现，但尚未默认开启。」**
（[原文 §4.1](https://mp.weixin.qq.com/s/zoS7GlReWPhfCjENGxXCdg)）

### 5.2 原型：两种更土但更可控的落地

1. **把前台命令赶去后台**：worker/router 预设让 shell 命令 **5 秒后转后台 job**，于是 Worker 能在下一个 step
   收到新消息，而不是卡在长命令上（[003](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/003-execution.md)、
   [006](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/006-deployment.md)）；
   worker 的子 agent 用 DSH 的 `standard` 预设，阈值 60 秒（[003](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/003-execution.md)）。
2. **长任务 = 长 Assignment**：Ticket 不因为一轮结束就结束；一轮结束时若还有后台 job/subagent，**Ticket 继续占槽、
   算「工作中」**，job 完成后在**同一个槽**里唤醒 Executor（同上）。

### 5.3 用户界面怎么知道进展

- **不是"进度条"，是"阶段 + 该谁动"**：每个 open Ticket 有**唯一一个 stage**，自顶向下第一条匹配者胜
  （下表标签是面板文案的中文意译；面板本身中英各一套，见 [001 词表](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/001-domain.md)）：

  | stage | 条件 | 面板标签 |
  |---|---|---|
  | person / delivery | 输入投递失败，2s→倍增到 1min 重试 | 输入未送达 |
  | person / answer | 有 Attention（等人回答的问题） | 等回答 |
  | person / review | 最新 Report = `ready` | 等验收 |
  | person / blocked·failed | 最新 Report 是 blocked/failed 且此后没人说话 | 受阻 · 失败 |
  | working / queued | 没有 Assignment，或有一个 pending | 排队中 |
  | idle | 最新 Report = `waiting`，或 Assignment 已结束 | 等待 Trigger |
  | working | 其他 | 工作中 |

  （[003](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/003-execution.md)）
- **健康不占地方**：Panel 只先回答「有没有事需要我」，健康状态「takes no space」（[005](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/005-panel.md)）。
- **负载是可见的**：`GET state` 的 `load` 给出 `running` / `queued` / `maxWorkers`（[006](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/006-deployment.md)）。

### 5.4 中断 / 取消 / 恢复

| 动作 | 语义 | 来源 |
|---|---|---|
| 人 stop | **命令**，不是 Trigger：直接作用在 Ticket 上，**撤回 Mandate**，Ticket 永久结束，之后的消息只进 Eval、不唤醒它 | [002](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/002-authorization.md)、[007](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/007-architecture.md) |
| 会话里说「stop」 | **是普通 Trigger**，由 Eval 决定 stop —— Executor 不能自己宣布「人让我停」 | [002](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/002-authorization.md) |
| pause processing | 停派发/新 Eval/超时/投递/回复；Source 继续记录；**正在跑的 turn 不打断**；暂停期间 Eval 不超时，恢复后截止时间顺延 | [003](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/003-execution.md) |
| retry | 结束当前 Assignment，在**同一 Session、同一目录、同一分支、同一 PR** 上重开 | 同上 |
| 重启 | Session、目录、订阅、待发消息、已读记录都保留；被打断的 Assignment 带 recovery note 重开 | 同上 |
| 未读输入 | Ticket 结束时，未被 ack/answer/handback 也未读的消息**回到 Eval**；已读但没动作的（比如一句谢谢）不回 | [003](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/003-execution.md)、[002](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/002-authorization.md) |

---

## 6 必要机制 vs 实现选择

### 6.1 两栏表

判据只有一条：**把这一栏换成别的做法，架构还成立吗？** 成立 → 右边的实现选择；不成立 → 左边的必要机制。

| 必要机制（缺了就不成立） | 同一问题的实现选择（可替换） | 判据 / 来源 |
|---|---|---|
| **1. 职责分离：Router 只分派不执行，Worker 不直接对外** —— 且这个分离要由**结构**（工具集、汇报通道、每 Ticket 一个会话）保证，不能只写在提示词里 | ① Router 完全不配 Bash（原文设想）② 保留 Bash 但缩工具集 + 5s 后台 + 禁止 `sleep`/长等待/长 job wait（原型做法，[004](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/004-prompts.md)）③ 单 Session + subagent + 提示词约束（被实验否掉） | **最强的必要性证据是反例**：单 Session + subagent，**中性提示下 5 次运行里一次都没委派**，p95 29.5 分钟；改成「优先委派」的提示后，4 次委派、仍慢于双层，剩下 1 次它判定"很小"自己干了，形成长尾（[原文 §7.1](https://mp.weixin.qq.com/s/zoS7GlReWPhfCjENGxXCdg)）。**「靠提示词求模型自觉」在结构上不可靠。** 二手侧也确认原型保留了 Bash（[offloop](https://offloop.org/learn/deepseek-super-agent-design)），说明「不配 Bash」是实现选择 |
| **2. 事件是一等输入，且过滤分两级（确定性 Fast Path + 模型 Slow Path）** | 具体准入表/单元格/`off·observe·auto` 三档；哪一级用模型；每 2 秒 pass 还是事件唤醒 | 原文把「程序策略 + 语义策略」写成**两级过滤的必要结构**，理由是成本与延迟（[§3.1](https://mp.weixin.qq.com/s/zoS7GlReWPhfCjENGxXCdg)）；原型把它落成 gate + assessment + 准入表（[002](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/002-authorization.md)）。**「先记录不执行」（observe）是可替换的默认值，不是必要机制** |
| **3. 领域状态与执行载体解耦：Ticket 自有状态、Mandate 管授权、Assignment 管一次执行、Subscription 管后续事件去哪** | SQLite / 关系表 vs 事件溯源；stage 七行怎么切；字段命名 | 001 明确拒绝 `Ticket state that follows the DSH Session`，理由是 Ticket 会被交接、恢复、或被多个 Executor 执行（[001](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/001-domain.md)）；`Assignment` 的定义就是「让"谁在执行"进入领域，但执行器本身留在领域之外」（同上） |
| **4. 先存后做 + 幂等 + 退避重试（stored work / outbox）** | effects 与 outbox 是一张表还是两张；退避曲线 2s→1min、5s→300s；重试几次算"系统问题" | 003 的原则句：「What dsh-superagent does after a decision … is **stored first and done afterwards**」；Trigger 受理两次只建一个 Ticket；回执让 ack 不会被当成没发生（[003](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/003-execution.md)、[001](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/001-domain.md)） |
| **5. 单一写者 + 每命令一个事务** | 单写者 actor / DB 约束 / 串行化隔离级别；读模型怎么来 | Engine 是 Triggers/Threads/Decisions/Tickets/Assignments 的**唯一写者**，命令同步、事务边界在命令里（[007](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/007-architecture.md)）。没有它，第 4 条的"先存"就没有可靠的事务边界 |
| **6. 外部 Control Loop，且它在主系统之外** | 周期（原型每小时）；只读还是可写；用退出码还是读 JSON；用另一个 profile / 另一台机器 | 原文的理由是分布式的常识：**「持久在线的分布式系统很难仅靠主系统内部逻辑保证长期可靠运行」**（[§6](https://mp.weixin.qq.com/s/zoS7GlReWPhfCjENGxXCdg)）；原型的「必须在服务挂掉时还能跑」是同一个约束的实现（[006](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/006-deployment.md)） |
| **7. 行为（提示词/规则）可在线改，且有版本与回滚** | 版本表 / `base` 记录 / 升级时如何提示 `Default updated`；Sessions 每请求读最新版 vs 只在创建时读 | 「在线修正主系统行为」是原文 §6 的能力之一；原型能做到它，靠的是**版本化 + 每请求读最新**（[004](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/004-prompts.md)）。没有版本，这条能力等于给系统装了一个没有 undo 的写入口 |
| **8. 面向人的「少说」与结构化提问** | 100 词/30 词/一句话；`human_action` 单问题字段；各信道表达规范 | 000 把「take as little human attention as possible」写成**第一条原则**，并由此推出「只在人必须行动时说话」「一次一个具体问题」（[000](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/000-philosophy.md)）；004 把它落成硬限制（[004](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/004-prompts.md)）。**这是"换了驱动力"之后唯一能防止系统变成噪音源的东西** |
| **9.（有条件必要）订阅/直通** | 多对多订阅表；直通 vs 全走 Router；取消订阅的两种粒度 | 只有「规模」这一条件满足时才必要：实验显示去掉直通**完成时间基本不变**，只是 Router 决定 +30%、token +37%（[§7.1](https://mp.weixin.qq.com/s/zoS7GlReWPhfCjENGxXCdg)）。所以它是**成本机制**，不是**正确性机制**；单用户、低并发时可以最后再上 |
| 存算分离（计算上云 + 端侧轻 daemon） | 云沙箱 / BYOM / 本地一体 | 原文 §5 是**方向**；README 明确「文章里讨论的有些设计是未来方向，本原型未实现」（[README](https://github.com/deepseek-ai/dsh-superagent-prototype)）。**属于实现选择 + 部署形态** |
| 投机执行（Speculative Execution） | 原型的「observe 只记录 would accept」是它的退化版 | 原文 §3.4；原型用 accept mode 表达（[002](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/002-authorization.md)）。**可选增强** |
| 链式召回 / Inline & Spill | 卡片内联 + 超长留指针；`read.search` 查询 | 原文 §3.3 是完整机制；原型可确认的只有卡片级 Inline & Spill（「Trigger cards, with resource pointers only for truncated content」，[004](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/004-prompts.md)）与只读查询（[007](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/007-architecture.md)）。完整"沿引用逐跳展开"未见实现说明 —— **列为未确认**（§9） |
| 具体数值 | 2 秒 pass、`maxWorkers`、eval 超时秒数、30 分钟后台上限、200 行自动合并、100/30 词、重试 2 次 | 全部出现在 [003](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/003-execution.md)/[006](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/006-deployment.md)/[007](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/007-architecture.md)。**都是标定值，不是结构** |
| GitHub / 飞书 Source、git worktree、tmux、DR 具体实现 | 换 Source 就换一套 | 001 的「What is reusable」一节把「概念与规则」列为可复用、「Source 实现 / Policy 内容 / Executor / Channel 实现」列为**属于某个部署**（[001](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/001-domain.md)）。这一句基本就是本节的方法论 |

### 6.2 判断口径（四条，比表更好用）

1. **凡是「让模型自己决定要不要分工」的地方，都要用结构替代提示词。** 依据是 §7.1 的消融：中性提示 0/5 委派。
2. **凡「重启后会丢」的状态，都必须先落库再动手。** 原型把这条写成 stored work 的一般原则，而不是逐个场景补丁。
3. **凡「主系统判断自己是否健康」的地方，都不成立。** Control Loop 必须在外面（原文 §6 的论证）。
4. **凡是能"省 Router"和"让系统正确"混在一起的地方，要分开评。** 直通属于前者（省 37% token，不提速）；
   授权/Mandate 属于后者（Accepting a Trigger twice creates one Ticket 是幂等，不是优化）。

### 6.3 对照本仓库（一句话版）

`docs/design/async-entry-audit.md` 已把仓库侧读数做完了，结论是三句话：**异步执行已有、交付是拉的、会话无法被唤醒**
（`Chat::send_prompt` 全仓唯一调用者是 `chat_message`；四套定时器没有一套通向 agent）
（[async-entry-audit.md](async-entry-audit.md) §1、§2.6、§3）。

对照 §6.1 逐行点名（这是本文的结论句）：

| 必要机制 | 本仓库现状 | 依据 |
|---|---|---|
| 1 职责分离（结构保证） | **部分**：有 `primary` 前门与路由级联，但前门没有被禁止执行，也没有"只分派"的工具集 | [primary-agent.md](primary-agent.md)、[orchestrator/lib.rs](crates/orchestrator/src/lib.rs) |
| 2 事件一等输入 + 两级过滤 | **没有**：无外部事件源（`grep webhook/github SourcePlugin` 无命中），输入只有人打字 | [async-entry-audit.md](async-entry-audit.md) §2.6 |
| 3 领域状态与执行载体解耦 | **部分**：`Task`（意图）/`Run`（尝试）两层已有，但缺"授权"与"工作单位自有 stage" | [core/task.rs](crates/core/src/task.rs)、[async-entry-audit.md](async-entry-audit.md) §3 |
| 4 先存后做 + 幂等 + 退避 | **没有**：无 outbox / 待执行动作表（`grep outbox/effect_queue` 无命中）；终态只广播不投递 | 同上 |
| 5 单一写者 + 每命令一个事务 | **有**：`store` 的单写者 actor 与串行化提交 | [sqlite.rs](crates/store/src/sqlite.rs) |
| 6 外部 Control Loop | **部分**：`watch` + 5 分钟计划任务 + `watchdog.log` 已有，但判据只有 `/health` 一个布尔、没有模型参与 | [async-entry-audit.md](async-entry-audit.md) §2.6、`scripts/ruagent-daemon.ps1` |
| 7 行为版本化、可回滚 | **没有**：提示词存在 `agents.toml`/capability 里，`grep prompt_version/prompt_history` 无命中 | 本仓库 grep |
| 8 少说 + 结构化提问 | **部分**：面板与 chat 有 stop/resume/handoff，但没有"只在必须行动时说话"的硬规则 | [api.rs](crates/daemon/src/api.rs) 路由表 |

所以结论是：**缺的主要是必要机制（2/4/7 完全没有，1/3/6/8 只有一半），而不是他们的实现细节**；
而**唯一可以直接复用的是第 5 条**（单写者事务），它恰好是第 4 条"先存后做"能低成本落地的前提。
必要机制也可以分三次补，见 §7。

---

## 7 最小可移植集：只搬三件

前提（用户给的三条约束）：已有「单守护进程 + 面板 + 多 agent 注册表 + 同步 chat」。目标不是复刻 dsh-superagent，
而是让**一个主 agent 能管、能派、能收回执**。

### 7.1 三件，按"缺了就转不起来"的顺序

#### 第 1 件：**Ticket 三元组 —— 工作单元 + 授权 + 一次执行，且 Session 只是执行载体**

搬的是 §2.1 第 6 项：`Ticket`（状态机、自有生命周期）+ `Mandate`（谁允许、凭什么、多大范围、哪版规则）
+ `Assignment`（一次执行尝试，可多次），外加把「后续事件该进哪个工作」的 `Subscription`。

**为什么是它，而不是"任务队列"**：因为另外两件都**挂在它身上**。没有 Ticket，Watchdog 没有"可判的对象"
（它只能看进程）；没有 Mandate，"主 agent 派活"就没有授权记录，做的事和谁允许的脱钩；没有 Assignment，
"重试/重启恢复/取消"就没有可以重新绑定的单位。仓库里已有 `Task`（意图，五种状态）与 `Run`（尝试，八态）
（[core/task.rs](crates/core/src/task.rs)、[async-entry-audit.md](async-entry-audit.md) §2.1），
**缺的正是中间那层"授权"和"工作单位自己的状态"**：今天 task 是意图、run 是尝试，**没有"允许到哪一步"的记录**，
也没有「这个 run 属于哪个工作、工作还开着吗」的聚合。

**落地难点**（按难度排）：
1. **语义合并**：`Task/Run` 已经是两层，再引入 Ticket 会变成三层。必须先定「Ticket ≡ Task 还是新表」。
   我的判断：**把 Ticket 的语义并进 Task**（加 `mandate`、`stage`、`ended`），把 Assignment 的语义并进 Run
   （加 `attempt`、`recovery_note`、`parent_run`），而不是新开两张表 —— 否则面板与 API 要维护两套同义状态。
2. **失败可见性**：审计已确认「run 失败时 task 永远停在 `in_progress`」
   （[async-entry-audit.md](async-entry-audit.md) §3 附加行）。Ticket 要能「说得出自己在等谁」，
   这条必须先修，否则 stage 表无从计算。
3. **认领与并发**：审计确认没有 claim/lease、闸门是内存信号量不是持久队列（同上）。Ticket 化会把这个洞暴露成
   「重启后谁都不知道自己该不该继续」。
4. **迁移**：`runs` 表要加列（origin/归属），这是本仓库的**第一次写入形状变化**，代价在回归测试不在代码量。

#### 第 2 件：**一个驱动心跳 + 「先存后做」的待执行动作表**

搬的是 §2.1 第 11/12 项（Passes + Effects/stored work），最小形态是：

- 一条**周期 tick**（不是"定时消息发给 agent"，而是"系统自己检查有没有该做的事"）；
- 一张**待执行动作**表：`(kind, payload, attempts, next_at, state)`；
- 执行规则：**事务内入队、提交后执行、失败退避、同类保序、幂等键去重**。

**为什么是它**：这是「从用户每轮驱动 → 系统自己驱动」的**唯一最小实现**。仓库现状是「异步执行 + 拉取式交付」，
而且闸门是内存信号量、重启即丢（[async-entry-audit.md](async-entry-audit.md) §2.5、§3）；
没有这张表，"回执"只能靠用户回去看，"重试"只能人工点（`/runs/{id}/retry` 存在但没人自动调用）。
有了它，第 1 件里的 Ticket 才有"下一步动作"可挂，第 3 件的 Watchdog 才有"可以安全重放的动作"可救。

**落地难点**：
1. **别用 tick 去 POST 自己的 HTTP**：审计已经指出那等于把 daemon 当外部客户端、还会绕开闸门
   （[async-entry-audit.md](async-entry-audit.md) §4 第 2 步）。正确做法是 tick 直接调内部 `start_run`。
   代价：**`start_run` 目前是 async handler 的形态**，要抽出一个不依赖 HTTP 的入口。
2. **已经有的事务边界要复用**：仓库有「单写者 actor」与 SQLite 单写连接（AGENTS.md），这是**优势**：
   动作表可以和业务表同库同事务，不需要分布式 outbox。难点是别让 tick 与 chat 抢同一把写锁。
3. **幂等键的设计**：按「Ticket + 用途 + 目标 id」做唯一约束。没有它，重启重放会发出两条一样的消息 —— 而"发消息"
   是不可撤销的对外动作（原型把它叫 Channel failure，重试到 300s 上限、按 Ticket+会话保序，
   [003](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/003-execution.md)）。
4. **"先存后做"要改的是写法习惯**：现有路径里很多动作是 `await` 完就发。改成"入队 → 由 executor 发"会让
   每个调用点都要拿到队列句柄。这是**一次性的侵入式改动**，但比每条路径各自补重试便宜。

#### 第 3 件：**外部 Watchdog：一个跑在主系统之外的 agent + 一个只读的 `status` 契约**

搬的是 §2.1 第 16/18 项与 §3.4 的三样接口：**退出码 + 结构化健康 + 日志目录**，而**不是**搬"每小时一个提示词"。

**为什么是它**：这是"主动/自愈"的最小闭环，也是最便宜的一件 —— 仓库**已经有第一层**：
`scripts/ruagent-daemon.ps1 watch` + 计划任务 `ruagent-daemon-watchdog`（每 5 分钟、`conhost --headless`、
`watchdog.log` 每次一行）（[async-entry-audit.md](async-entry-audit.md) §2.6 的四套定时器表）。
它和原文的差距不是"有没有定时器"，而是两点：**（a）判据只有 `GET /health` 一个布尔**，
**（b）没有一个模型在读它、也没有说得出原因的通道**。

**落地难点**（这一件的难点不在代码，在边界）：
1. **必须有"任务进展"的信号，否则模型只能读日志猜**：这是原型自己也缺的那一环（§3.5）。最少要三个读数：
   每个 Ticket 的 **stage**、**最后一次 Report/事件的时间**、**它现在在等谁**（人 / 槽位 / 外部）。
   没有这三个字段，Watchdog 只能复述「进程活着」——那正是原文说不够的那一层
   （[§6](https://mp.weixin.qq.com/s/zoS7GlReWPhfCjENGxXCdg)）。
2. **谁来跑**：必须在主系统之外。仓库里最现成的形态就是**计划任务 + 一个 agent 会话**，
   但今天 `Chat::send_prompt` 没有内部调用者（[async-entry-audit.md](async-entry-audit.md) §2.6），
   所以「定时给一个会话发消息」这件事本身要第 2 件的待执行动作表来做，
   或者退一步：用一次性的独立会话（蒸馏器 `ask_agent_with_usage` 是现成先例：
   自己 `start_chat`、自己发一轮、有超时与权限处置，同上一节）。
3. **要不要给它写权限**：原型的答案是"给"（管理 API 能改 settings/prompts/policy，且**比主 Session 工具的权限更大**，
   [006](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/006-deployment.md)）。
   我的建议是**第一版只给读 + 一个白名单动作（restart / pause / retry）**，理由：原型的写面之所以安全，靠的是
   第 7 条必要机制（版本 + 回滚），而本仓库的提示词还没有版本化；在没有 undo 的系统上开写权限，
   等于把 Watchdog 变成新的故障源（二手侧对同一风险的说法见 [腾讯云解读](https://developer.cloud.tencent.com/article/2774420)）。
4. **成本与"少说话"的冲突**：每小时一次模型调用是**固定开销**；而 §6.1 第 8 条要求它"没事就别说话"。
   原型的折中是「exit 0 时什么都不说 + 同类错误只说一次」。要照抄这条，先得有"同类"的判定键（错误码 + 组件 id）。

### 7.2 明确**不**搬的（都属于实现选择）

- 存算分离 / 端侧 daemon / BYOM（原文 §5，原型未实现）。我们是单机单用户，这是负收益。
- 投机执行（§3.4）：没有"先跑起来再说"的算力预算时，它只是花钱。
- 链式召回（§3.3）的完整形态：卡片 + 截断指针 + 搜索已经够用，等到"跨月记忆"成问题时再说。
- 直通（§3.3/§3.2）：单用户并发低，先走 Router 全量分派；**等 Router 的 token 成为可测量成本时再上**（§6.1 第 9 条）。
- GitHub/飞书 Source、worktree prepare、tmux 托管：换 Source 就是另一套实现（[001](https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/001-domain.md)）。
- 四值 Decision / 六值 Report / 七行 stage 的**具体取值**：抄"结构"（枚举 + 一个 stage 函数），不抄"词表"。

### 7.3 交付顺序与验收（建议）

1. 先做第 1 件的**只读一半**：把 stage / 等待对象 / 最后活动时间**算出来并显示在面板与 `GET /tasks` 上**。
   验收：一个失败或卡住的 run，面板上不再显示"进行中"。
2. 再做第 3 件的**只读一半**：一个外部定时任务，读一个新加的 `status` 命令（退出码 + errors/setup/load/stalled），
   用自然语言汇报"哪件活卡住了"。验收：人为让一个 run 停住，Watchdog 能指名它、且不改任何东西。
3. 最后做第 2 件：tick + 动作表 + 自动重试，并把「完成」回流给会话（这一步才真正消灭"切窗口"）。
   验收：重启后待发消息不丢、不重、不早于它的前置消息。

---

## 8 来源全表

| # | 来源 | 类型 | URL |
|---|---|---|---|
| S1 | 《Super-agent 设计探索：迈向更自主的 Agent 系统》，DeepSeek 产品部 & DeepSeek Harness 团队，2026-10-31 发布（公众号原文，含 §1~§9 全文、实验数据、作者与致谢） | **一手** | https://mp.weixin.qq.com/s/zoS7GlReWPhfCjENGxXCdg |
| S2 | 官方开源原型仓库 `deepseek-ai/dsh-superagent-prototype`（README 声明：研究原型、非正式产品、与文章所述实现不完全相同，部分设计是未来方向；单提交 `8c32a9d`，2026-10-31） | **一手** | https://github.com/deepseek-ai/dsh-superagent-prototype |
| S3 | `docs/000-philosophy.md`（注意力预算原则） | 一手 | https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/000-philosophy.md |
| S4 | `docs/001-domain.md`（Source/Trigger/Thread/Correlation/Channel/Policy/Eval/Mandate/Ticket/Assignment/Subscription + 被拒概念 + 中文词表） | 一手 | https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/001-domain.md |
| S5 | `docs/002-authorization.md`（Gate/Assessment/Decision/准入表/accept mode/交接与释放/脱手/Eval 未完成） | 一手 | https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/002-authorization.md |
| S6 | `docs/003-execution.md`（stage 表/Executor/后台与槽位/并发/失败与恢复/存储工作/暂停） | 一手 | https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/003-execution.md |
| S7 | `docs/004-prompts.md`（团队段落/内置 part 与覆盖键/组装优先级/「Separate roles」等原则/版本与生效） | 一手 | https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/004-prompts.md |
| S8 | `docs/005-panel.md`（三个问题/六个 tab/七条面板原则） | 一手 | https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/005-panel.md |
| S9 | `docs/006-deployment.md`（部署形态/权限与隔离/`status` 四退出码/Hourly check 提示词原文/管理 API 表/健康条目/setup 检查/升级与预设） | 一手 | https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/006-deployment.md |
| S10 | `docs/007-architecture.md`（Runtime/Engine 五 part/Store/Outbox/Passes/Notices/角色化引擎接口/域事件/策略/版本） | 一手 | https://raw.githubusercontent.com/deepseek-ai/dsh-superagent-prototype/main/docs/007-architecture.md |
| S11 | 官方 X 账号 `@DeepSeekHarness` 的 article 页（英文版入口，本次抓取被数据源拒绝，**未读**） | 一手（未读） | https://x.com/DeepSeekHarness/article/2116857257245987236 |
| S12 | 知乎全文转载（正文与 S1 一致，用于交叉核对被截断的段落） | 二手 | https://zhuanlan.zhihu.com/p/2099987320443237060 |
| S13 | offloop《Exploring DeepSeek Super Agent Design — Full English Translation》 | 二手（英译全文） | https://offloop.org/learn/exploring-deepseek-super-agent-design |
| S14 | offloop 分析文：指出原型与原文的差异（Router 保留 Bash 等） | 二手 | https://offloop.org/learn/deepseek-super-agent-design |
| S15 | 腾讯云开发者《DeepSeek 开源 Super-agent…》：把「Watchdog 有权改主系统提示词、这是一个新的故障点」写明 | 二手 | https://developer.cloud.tencent.com/article/2774420 |
| S16 | 开源中国《DeepSeek 团队把 Agent 设计成"操作系统"》 | 二手（摘要） | https://www.oschina.net/news/503338/deepseek-harness-superagent |
| S17 | 36 氪《详解 Super Agent：会主动干活，更懂何时闭嘴》 | 二手（侧记） | https://m.36kr.com/p/4050650194630536 |
| S18 | 本仓库：`docs/design/async-entry-audit.md`（仓库侧六项读数：异步已有/交付靠拉/无法唤醒/四套定时器不通向 agent） | 本仓库一手 | [async-entry-audit.md](async-entry-audit.md) |
| S19 | 本仓库：`crates/core/src/task.rs`、`crates/orchestrator/src/lib.rs`、`crates/daemon/src/api.rs`（Task/Run/Topology/路由级联与现有端点） | 本仓库一手 | [task.rs](crates/core/src/task.rs)、[orchestrator/lib.rs](crates/orchestrator/src/lib.rs) |
| S20 | 本仓库：`docs/design/primary-agent.md`（前门不变量：一个可解析的名字、一个答案、一次写入） | 本仓库一手 | [primary-agent.md](primary-agent.md) |

---

## 9 我没能确认的

1. **原文没有给出 Watchdog 的定时消息内容、巡检周期、健康判据。** §3.4 引用的提示词来自**原型的部署文档**
   （S9），它是"为公开发布做过适配"的版本；内部实例是不是也这么写、周期是不是每小时，**查不到**。
2. **发布时间有两个读数**：公众号页面抓取到的元数据是 `Oct 31, 2026, 8:29 AM`，而一篇二手解读写"2026-10-31 20:29 发布"。
   两者差 12 小时，**大概率是时区（UTC vs 北京时间）**，但我没有独立证据，照实写。
3. **原文 §3.3 的「链式召回」在原型里实现到什么程度，我没能确认。** 可确认的只有：卡片内联 + 超长留指针（S7）、
   只读查询与索引（S10）、`001` 域模型里**没有**"链式召回/引用图"这个概念（S4）。完整形态可能未实现
   （README 只说"部分设计未实现"，没列清单）。
4. **`007-architecture.md` 我只读到约 5 万字符（工具截断）**，被截掉的是后半部分（Executor/Sessions/Effects 适配器、
   `src/` 与测试索引）。因此「Effects 的实现细节（表结构、调度器）」的表述来自 `003` 与 `007` 前半，**没有逐行核实**。
5. **原型有没有"选 worker"的逻辑**：我按 `003`「每 Ticket 一个 `worker-<id>` Session + 一个目录」与 `006` 的
   `maxWorkers` 判定为**没有选择器**；但我没有穷举 `src/executor/dsh.ts` 的源码（未下载仓库）。
6. **Eval 超时的默认秒数、`maxWorkers` 的默认值、面板上 stage 的排序细节**：`006` 只给出可配键名，
   **默认值查不到**。
7. **实验数字没有独立复现**：p50 12.4 / p95 15.7 分钟、token 2323 万→3177 万、batch 1.0→2.8 与 6.4s→1.6s，
   全部只来自 S1 的自述；其负载是"约 300 条消息、约 20 条工作请求、编码工作用占位工具代替"（S1 §7.1），
   **不能外推到真实编码质量**（原文自己也这么限定）。
8. **官方 X article（S11）未读**：抓取工具对该 URL 返回 `THIRD_PARTY_DATA_UNSUPPORTED_URL`。英文表述以 S13（二手英译）为准。
9. **"Watchdog 用管理 API 改主系统"是我的推断**：S9 只说明管理 API 的存在、鉴权头与权限范围，
   **没有**说明 Watchdog 通过它写。原文只说"可以让它在线修正"，通道未写明。
10. **本仓库侧我只读了审计文档与少量源码符号**（`task.rs`、`orchestrator/lib.rs`、API 路由表、`scripts/ruagent-daemon.ps1`
    的 `watch`/`install-task`、以及 `grep` 空结果：无 outbox/effects、无 webhook/GitHub Source、`TaskCreator::Schedule`
    无构造点）。**没有起私有守护进程做运行时观测**，与 S18 的纪律一致。
