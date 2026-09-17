# ruagent

[English](README.md) · **中文**

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

## 快速开始

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
claude-code——同一个角色、同一份记忆。面板为两层各设独立页面：
**智能体**（#agents，角色 + 实时表现）和**运行时**（#runtimes，
执行后端）。旧的单层配置继续兼容。

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

## 记忆层

| 层 | 内容 | Token 成本 |
|---|---|---|
| **L0 情景** | 逐字会话转录——用户 + 智能体，零判断，永不销毁 | 只占磁盘 |
| **L1 语义** | 带质量信号的蒸馏记忆（用户纠正时抽取*纠正后*的事实；确认提升置信度） | 召回时 |
| **L2 结构** | 知识图谱——实体、关系、双时间线事实、确定性替代 | 召回时 |
| **L3 摘要** | 会话首条提示词注入：画像 + 基于开场白的语义召回 | ~1k，仅一次 |

来自 **claude-code、dsh、opencode、ruagent** 的会话自动汇入同一段
历史（60 秒增量扫描）。`policy.toml` 里 `[distill] auto = true` 时，
会话关闭即在后台蒸馏成记忆 + 图谱。

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
原始 top 分数）驱动调优。

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
