# ruagent

[English](README.md) · **中文**

[![CI](https://github.com/hfldqwe/ruagent/actions/workflows/ci.yml/badge.svg)](https://github.com/hfldqwe/ruagent/actions/workflows/ci.yml) [![E2E](https://github.com/hfldqwe/ruagent/actions/workflows/e2e.yml/badge.svg)](https://github.com/hfldqwe/ruagent/actions/workflows/e2e.yml) [![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

本地优先、单用户的**智能体工程平台**：通过 [Agent Client Protocol](https://agentclientprotocol.com) 编排多个编码智能体 CLI（Claude Code、OpenCode、DeepSeek Harness、Codex、…），配套统一记忆层、知识库 + Wiki 编译、会话蒸馏和全程可观测。

```
┌───────────────────────────── ruagent ─────────────────────────────┐
│  角色（可移植提示词）           architect · plugin-dev · …        │
│    └ 可跑在任意运行时          dsh · claude-code · opencode      │
│  统一记忆                     L0 转录全文 → L3 首条注入摘要      │
│    └ 自动蒸馏                 会话 → 记忆 + 图谱                 │
│  统一知识                     向量 + 全文检索混合 + 图谱定向     │
│    └ 编译成 Wiki              源文档 → 互相链接的综述页          │
│  每个智能体都获得             memory_recall · knowledge_search … │
│    同一个平台 MCP             （无需为每个引擎写胶水）           │
└───────────────────────────────────────────────────────────────────┘
```

一个常驻 daemon（Rust）同时服务 CLI 和 Web 面板——两者都是同一个
本地 API 的瘦客户端。一切都在你的机器上运行，数据不出本机。

智能体进程随 daemon 一起结束：Windows 上 daemon 加入 kill-on-close
的 Job Object，daemon 进程一死（崩溃、被杀或正常退出），操作系统就
立刻终止它拉起过的所有智能体、包装进程和 MCP 服务器；所有平台上，
下一次启动都会清扫上一个已确认消失的 daemon 遗留的子进程。

## 安装（预编译版）

每个打 tag 的版本都会发布**开箱即用**的归档——不需要 Rust 工具链、不需要
`protoc`、不需要 `npm ci`：

| 平台 | 归档 |
| --- | --- |
| Windows (x86_64) | `ruagent-vX.Y.Z-x86_64-windows.zip` |
| Linux (x86_64) | `ruagent-vX.Y.Z-x86_64-linux.tar.gz` |
| macOS（Apple 芯片） | `ruagent-vX.Y.Z-aarch64-macos.tar.gz` |

到 [Releases](https://github.com/hfldqwe/ruagent/releases) 下载对应平台那一个，
用同页的 `SHA256SUMS` 校验，**整包解压**后运行：

```bash
./ruagent serve      # 守护进程 + Web 面板，127.0.0.1:8787
./ruagent doctor     # 7 项端到端自检
```

然后打开 **http://127.0.0.1:8787**。

> 归档把 Web 面板放在可执行文件旁边（`panel/dist`），因为 `ruagent` 是按**自身
> 所在目录**解析面板的。把可执行文件单独挪走，就只剩 API、`/` 会 404——发布
> 流水线的冒烟测试正是拦这个：CI 会解压自己产出的归档并真的启动一次，只有
> `/` 能出面板且 `/api/v1/health` 有应答才允许发布。

Intel 芯片的 Mac 暂不发布，请用下面的源码构建。

## 从源码构建

```bash
# 前置：protoc 在 PATH 上（lancedb 构建期需要）
#   Windows: choco install protobuf   （或解压到 %USERPROFILE%/.protoc）
#   Linux:   sudo apt-get install protobuf-compiler
cargo build --release

# Web 面板（可选，但推荐）
cd panel && npm ci && npm run build && cd ..

cargo run --release -p ruagent -- serve     # daemon + 面板，端口 :8787
cargo run --release -p ruagent -- doctor    # 7 项端到端自检
```

打开 **http://127.0.0.1:8787** ——与任意智能体对话、浏览统一的会话
历史、跨记忆 / 知识 / Wiki / 图谱召回、逐事件回放运行过程。

## 两层智能体模型

智能体是**角色**，不是引擎。`~/.ruagent/config/agents.toml` 把
「智能体是什么」和「哪个运行时执行它」分开：

```toml
[runtime.dsh]                    # 运行时：执行后端
command = "dsh --profile acp"
mcp_profile = "default"

[agent.architect]                # 角色：可移植的提示词
prompt = "You are the architecture reviewer. …"
runtimes = ["dsh", "claude-code", "opencode"]
runtime = "dsh"                  # 默认运行时
```

角色提示词跨运行时通用：在 dsh 上与架构评审对话，中途切到
claude-code——同一个角色、同一份记忆。角色提示词也同样上运行：角色块
位于每次运行注入上下文的最前面，被委派出去的运行带的是那个专家的身份，
而不只是它背后的运行时。面板为两层各设独立页面：
**智能体**（#agents，角色 + 实时表现）和**运行时**（#runtimes，
执行后端）。旧的单层配置继续兼容。

## 主智能体（谁来回答你）

有一个智能体是**正门**：新对话从它开始，凡是没被 pin、没被规则、也没被
`routing.toml` 的 `default` 选中的运行，都落到它身上。在 `agents.toml` 里标出：

```toml
[agent.ruagent]
primary = true                    # 最多只有一个智能体能带这个标记
prompt = "你是 ruagent，用户唯一的对接人。…"
runtimes = ["claude-code", "dsh", "opencode"]
```

全新安装本来就有——seed 把 `claude` 标成了主智能体，而它正是 `routing.toml`
早就点名的那个——所以在你主动更改之前，行为一点没变。三种改法：

```bash
ruagent primary                     # 这里谁在回答你，以及为什么
ruagent primary set architect       # 换人（一处调用写两处）
ruagent primary none                # 干脆不设主智能体
```

……或者用面板的**智能体**页（首页也会显示当前主智能体），或者直接让智能体
自己改。它有十个工具做这件事：`agents_list` / `agent_set_primary` /
`agent_create` / `agent_update` / `agent_delete` 决定「有谁」和「谁回答你」，
`routing_show` / `routing_set_default` 管路由级联，
`task_create` / `task_dispatch` / `run_get` 把活派给别的智能体并在不阻塞的
前提下跟进。派活才是重点：你只跟一个智能体说话，由它去操作其余的。

`GET /api/v1/primary` 给出的永远是**实际会回答你的那个智能体**，并说明这个
结论来自哪里。这条区分是承重的：`routing.toml` 的 `default` 优先于 `primary`
标记，若两者不一致而接口只报标记，你会设了主智能体却看不到任何变化。写入同理
——一次调用写两处——理由是一样的。

两个智能体同时标 `primary` 是**加载错误**并点名两者，而不是按文件顺序决出
胜负。标了但被禁用的主智能体会**明确声明退出**：路由的 rationale 会写明跳过了
谁、为什么。

### 接入运行时（以及接不通时看哪里）

*运行时*是可被拉起的引擎：平台通过 ACP 跑一条命令跟它说话。种子注册了三个，
而**每一个都要求你正用的这台机器上装了对应的东西**——换一台电脑全新安装，会看到
它们、然后启动失败，这是预期状态，不是装坏了：

| 运行时 | 命令 | 需要什么 |
| --- | --- | --- |
| `claude` | `npx @agentclientprotocol/claude-agent-acp` | Node.js（`npx`）、首次运行要联网、以及适配器要驱动的 `claude` CLI |
| `opencode` | `opencode acp` | `PATH` 上的 `opencode` |
| `dsh` | `dsh --profile acp` | `PATH` 上的 `dsh` |

要在 **daemon 的 `PATH`** 里查（不一定是你的 shell 的）：

```bash
command -v npx opencode dsh      # Windows: where npx opencode dsh
```

然后把你打算用的那个**端到端验一遍**——「注册了」不等于「起得来」：

```bash
curl -s -X POST localhost:8787/api/v1/runtimes/dsh/probe   # 0.1.6 起：stage + detail
curl -s localhost:8787/api/v1/agents/dsh/options           # 真去起一次；400 = 没起来
```

`options` 的 `400` 会原样带出原因——`program \`x\` for agent \`dsh\` not found on
PATH` 是最常见的那条。若这台机器上装的是别的命令，在面板 Runtimes 页编辑它，或：

```bash
curl -s -X PATCH localhost:8787/api/v1/runtimes/dsh \
  -H 'content-type: application/json' -d '{"command":"my-cli --acp"}'
```

### 一台什么都没装的机器：`0/3` 就是起点，不是坏掉

全新电脑上 `ruagent doctor` 会直接这样报（0.1.8 实测，PATH 里没有 `npx`/`dsh`/`opencode`）：

```
  [FAIL] runtimes   0/3 runtime(s) start — NONE could start: install the binary each FAIL row
                    names, or set that runtime's `command` to the binary's full path in
                    ~/.ruagent/config/agents.toml, then re-run `ruagent doctor`
         FAIL claude   [ClaudeCode] command NOT found on PATH · does NOT start (stage: not-found)
                       — `npx` is not on PATH. Install it, or set this runtime's `command` …  ← primary
         FAIL dsh      [Dsh]        command NOT found on PATH · … `dsh` is not on PATH. …
         FAIL opencode [OpenCode]   command NOT found on PATH · … `opencode` is not on PATH. …
```

`← primary` 标出前门（谁来应答你）。**只要装上任一个**——例如装 Node 让 `claude` 的
`npx` 可用——再跑一次 `ruagent doctor`，那一行就会变成 `starts (stage: ok)`，面板同名的
卡片也会从「找不到命令」变成「命令可解析」。你不需要三个都装，也不需要它们都在 PATH 上：
把某个运行时的 `command` 改成二进制全路径同样成立。

运行时还需要一个**已知 harness**（`claude-code`、`opencode`、`dsh`、`mock`），它决定
挂哪个 ACP 适配器。名字不必等于 harness：名字自定义时请显式给 `harness`。

## 编排

- **直连** —— 一个智能体一次运行。
- **扇出对比** —— 同一提示词并行发给 N 个智能体（各占独立 git
  worktree），结果并排展示；人工选优，或交给 **AI 评审运行**自动
  裁决——评审本身就是一个普通运行，回复即裁决，来源可审计
  （`agent:<name>` vs `human`，人工永远可覆盖）。
- **流水线** —— 顺序子任务，有界交接上下文。
- **路由级联** —— 显式指定 > 规则 > 默认，每个决策记录来源。
- **权限级联** —— 确定性规则 > 审批智能体 > 人工收件箱，fail-closed；
  高危操作永远到人。
- **并发闸门** —— 扇出超过某个 harness 上限时进入排队，而不是一次
  拉起 N 个子进程：运行行标记为 `queued`，在让出空位前不占用工作区、
  不产生子进程。排队数也超限时，新启动被拒绝并返回饱和错误，错误
  记录在该运行行上。
- **失败运行重试** —— 失败 / 中断 / 取消的运行在面板上有**重试**
  按钮（`POST /api/v1/runs/<id>/retry`）：同一任务、同一智能体、同一
  选项的新运行，复用上次运行的工作区（写了一半的成果保留），并把
  崩溃快照（上次最后的输出、最后的工具调用和死因）作为注入上下文
  前置，而非写进提示词。

并发上限在 `policy.toml` 中调整：

```toml
[concurrency]
per_harness = 2        # 每个 harness 同时运行数
queue_per_harness = 8  # 排队上限，超过即拒绝
```

## 记忆层

| 层 | 内容 | Token 成本 |
|---|---|---|
| **L0 情景** | 逐字会话转录——用户 + 智能体，零判断，永不销毁 | 只占磁盘 |
| **L1 语义** | 带质量信号的蒸馏记忆（用户纠正时抽取*纠正后*的事实；确认提升置信度） | 召回时 |
| **L2 结构** | 知识图谱——实体、关系、双时间线事实、确定性替代 | 召回时 |
| **L3 摘要** | 会话首条提示词注入：画像 + 基于开场白的语义召回 | ~1k，仅一次 |

来自 **claude-code、dsh、opencode、ruagent** 的会话自动汇入同一段
历史（60 秒增量扫描）。`policy.toml` 里 `[distill] auto = true` 时，
会话关闭即在后台蒸馏成记忆 + 图谱。`[distill] language = "简体中文"`
指定蒸馏产出的书写语言；`[distill] prompt` 可整体替换内置提取提示词。

**召回双策略**（`GET /api/v1/recall` / `memory_recall` MCP 工具），
各自返回四个独立预算的分区——记忆、知识、Wiki、实体：
- **激进** —— 高于相关性阈值即给全文，RRF 融合（语义 + 关键词），
  拿来即用
- **保守** —— 只给桩：标题 + 活跃关系 + chunk id；智能体按需经
  `memory_get` / `graph_entity` 拉取详情

## 知识库 + Wiki

- **混合检索，查询期零 LLM**：LanceDB 向量 + SQLite FTS5，RRF 融合。
  开箱即多语言（`multilingual-e5-small`；离线时回退哈希嵌入器），
  模型切换自动重嵌入。
- **Wiki 模式**：把源文档编译成互相链接的综述页。三段管线
  （计划 → 逐页撰写 → 校验落地）置于 dry-run 人工门之后；手改过的
  页永不被覆盖（内容哈希追踪）、失效页打标、断链 `[[链接]]` 与
  「想要的页」在面板可见。召回中的 Wiki 命中永远是保守桩，且标注
  为生成内容。

## 每个智能体免费获得平台记忆

每个拉起的智能体都会收到同一个 MCP 服务器：
`memory_search` · `memory_recall` · `memory_write` · `memory_get` ·
`knowledge_search` · `knowledge_ingest` · `list_tasks`——无需为每个
引擎写胶水。技能（SKILL.md 库）同步到每个引擎的技能目录。

## 可观测性

可回放的 JSONL 转录（SSE 实时流）、按智能体的成本统计、每次运行的
上下文注入在时间线上渲染、召回使用日志（每次调用的分区计数 + 过滤前
原始 top 分数）驱动调优。运行时间线把原始指令单列为一条记录，与
注入上下文分开显示。

## CLI

```
ruagent serve · doctor · agents · status · run · skills · skills-sync ·
         mcp-serve · wiki build|list|links|show
```

## 文档

- 总体设计：[`docs/plans/2026-09-11-ruagent-design.md`](docs/plans/2026-09-11-ruagent-design.md)
- 记忆层：[`docs/plans/2026-09-14-unified-memory.md`](docs/plans/2026-09-14-unified-memory.md)
- 蒸馏：[`docs/plans/2026-09-14-distillation-design.md`](docs/plans/2026-09-14-distillation-design.md)
- Wiki 模式：[`docs/plans/2026-09-15-wiki-mode-design.md`](docs/plans/2026-09-15-wiki-mode-design.md)
- 调研笔记：[`docs/research/`](docs/research/) · 路线文档见 [`docs/plans/`](docs/plans/)

## 状态

核心里程碑已完成：统一记忆（L0–L3 + 自动蒸馏）、两层智能体、图谱
定向召回、四源会话同步、doctor 自检、Wiki 模式、扇出 AI 评审、双语
面板、Playwright E2E 套件（真实 daemon，零 mock）与基于可脚本 mock
智能体的 Rust 集成测试。CI：rust（ubuntu + windows）、panel（node）、
E2E。

## 许可

[MIT](LICENSE)
