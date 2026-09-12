// Zero-dependency i18n: dictionary lookup + language toggle, persisted to
// localStorage, default from navigator.language. UI is fully bilingual
// (zh / en).

import { createContext, useContext, useEffect, useState, type ReactNode } from "react";

export type Lang = "zh" | "en";

const zh: Record<string, string> = {
  // nav
  "nav.board": "看板",
  "nav.memory": "记忆",
  "nav.knowledge": "知识库",
  "nav.graph": "图谱",
  "nav.agents": "智能体",
  "nav.stats": "统计",
  "nav.inbox": "收件箱",

  // common
  "common.online": "daemon 在线",
  "common.offline": "daemon 不可达",
  "common.delete": "删除",
  "common.keep": "保留",
  "common.cancel": "取消",
  "common.search": "搜索",
  "common.clear": "清除",
  "common.close": "关闭",
  "common.create": "创建",

  // home
  "home.greeting": "欢迎回来",
  "home.subtitle": "本地优先的智能体工程平台：编排多个编码智能体，统一记忆与知识，全程可观测。",
  "home.stat.agents": "已接入智能体",
  "home.stat.tasks": "任务总数",
  "home.stat.runs": "累计运行",
  "home.stat.memories": "记忆条目",
  "home.stat.docs": "知识文档",
  "home.start.title": "从这里开始",
  "home.step1.title": "创建任务",
  "home.step1.desc": "写下你想完成的事——它会成为运行的提示词。项目字段可以限定记忆范围。",
  "home.step1.cta": "创建第一个任务",
  "home.step2.title": "选择运行方式",
  "home.step2.desc": "单个智能体运行；或扇出到多个智能体并行、对比结果选优；或组成流水线接力。",
  "home.step3.title": "结果与沉淀",
  "home.step3.desc": "执行日志逐事件回放（工具调用、权限裁决、上下文注入）；智能体会通过 MCP 读写平台记忆，跨会话记住你的偏好。",
  "home.guide.title": "各区块的作用",
  "home.guide.board": "任务看板——创建、运行、扇出对比、流水线",
  "home.guide.memory": "平台长期记忆——智能体跨会话记住的东西，可审计",
  "home.guide.knowledge": "知识库——摄取文档，所有智能体可语义检索",
  "home.guide.graph": "实体图谱——带时间线的事实，支持“当时为真”查询",
  "home.guide.agents": "已注册的智能体及其实时表现与成本",
  "home.guide.stats": "按智能体统计的运行量、成功率、成本",
  "home.guide.inbox": "等待你批准的权限请求（配置审批智能体可无人值守）",

  // board
  "board.title": "任务",
  "board.total": "共 {n} 个",
  "board.new": "新建任务",
  "board.empty.title": "还没有任务",
  "board.empty.hint": "创建一个任务，然后在任意智能体上运行——或扇出到多个并行对比。",
  "board.col.pending": "待处理",
  "board.col.in_progress": "进行中",
  "board.col.blocked": "阻塞",
  "board.col.done": "已完成",
  "board.list": "列表",
  "board.board": "看板",

  // new task dialog
  "newtask.title": "新建任务",
  "newtask.titleLabel": "标题",
  "newtask.intentLabel": "意图（要做什么——会成为运行提示词）",
  "newtask.projectLabel": "项目（可选——限定项目记忆范围）",
  "newtask.created": "任务已创建",

  // task detail
  "task.back": "返回看板",
  "task.deleteConfirm.title": "删除任务？",
  "task.deleteConfirm.body": "将删除任务及其运行记录（转录文件保留作为凭证）。此操作不可撤销。",
  "task.runs": "运行",
  "task.runsCount": "（{n}）",
  "task.totalCost": "共 {cost}",
  "task.noRuns": "还没有运行——在上方选择智能体并启动。",
  "task.log": "执行日志",
  "task.comparison": "结果对比",
  "task.winner": "胜出",
  "task.selectWinner": "选为胜出",
  "task.viewLog": "查看日志",
  "task.cancelRun": "取消",
  "task.cancelling": "正在取消…",
  "task.noResult": "（无结果）",
  "task.status": "任务状态",

  // launcher
  "launcher.run": "运行",
  "launcher.fanout": "扇出对比",
  "launcher.pipeline": "流水线",
  "launcher.prompt": "提示词（默认为任务意图）…",
  "launcher.repo": "Git 仓库（可选——每个运行独享 worktree 隔离）…",
  "launcher.selected": "已选 {n} 个",
  "launcher.startPipeline": "启动流水线",
  "launcher.needs2": "扇出至少需要选择 2 个智能体",
  "launcher.chooseAgent": "选择智能体…",
  "launcher.addStep": "+ 添加步骤",
  "launcher.removeStep": "移除此步骤",
  "launcher.noAgents": "没有启用的智能体——编辑 ~/.ruagent/config/agents.toml。",
  "launcher.hint.title": "三种模式有什么区别？",
  "launcher.hint.body": "**运行**把提示词发给一个智能体。**扇出对比**并行发给多个（填了仓库时各自独享 worktree），结果并排展示，由你选优。**流水线**按顺序接力，每一步收到上一步的结果作为交接上下文。",

  // memory
  "memory.title": "记忆",
  "memory.subtitle": "平台记住的东西——每个智能体都通过 MCP 读写",
  "memory.browse": "浏览",
  "memory.audit": "审计日志",
  "memory.write": "写入记忆",
  "memory.current": "当前",
  "memory.superseded": "已替代",
  "memory.replaces": "替代 #{id}",
  "memory.empty.title": "{store} / {ns} 暂无记忆",
  "memory.empty.hint": "智能体会通过 memory_write 工具写入；你也可以手动写入。",
  "memory.supersede": "替代",
  "memory.supersede.title": "替代记忆 #{id}",
  "memory.supersede.desc": "旧内容保留在历史中（审计日志可追溯），新内容成为当前版本。",
  "memory.supersede.btn": "替代",
  "memory.write.title": "写入记忆",
  "memory.write.desc": "每条写一个简明事实。这条记忆会被有界注入后续运行，并可通过 memory_search 被所有智能体检索。",
  "memory.write.govern": "存储 / 命名空间（治理：profile 仅限 user；procedure / lesson 仅限 project / global）",
  "memory.write.content": "内容",
  "memory.write.placeholder": "部署脚本在 scripts/release.sh",
  "memory.write.btn": "写入",
  "memory.audit.empty": "还没有写入记录——每次写入决策都会记在这里。",
  "memory.nsNew": "新建命名空间",

  // knowledge
  "knowledge.title": "知识库",
  "knowledge.subtitle": "所有智能体都可检索的文档",
  "knowledge.ingest": "摄取文档",
  "knowledge.search": "搜索文档（语义 + 关键词）…",
  "knowledge.noResults": "没有结果——换个关键词，或摄取更多文档。",
  "knowledge.empty.title": "还没有文档",
  "knowledge.empty.hint": "粘贴 Markdown 或笔记——分块、向量化后即可被所有智能体通过 knowledge_search 检索。",
  "knowledge.chunks": "{n} 块",
  "knowledge.deleted": "已删除 {name}",
  "knowledge.ingested": "已摄取 {n} 块",
  "knowledge.ingest.title": "摄取文档",
  "knowledge.ingest.name": "名称",
  "knowledge.ingest.namePh": "deploy-runbook…",
  "knowledge.ingest.content": "内容（Markdown / 笔记 / 代码——约 800 字符分块，带重叠）",
  "knowledge.ingest.btn": "摄取",
  "knowledge.score": "相关度 {s}",

  // graph
  "graph.title": "实体图谱",
  "graph.subtitle": "带双时间线的事实——被替代而非删除",
  "graph.newEntity": "新建实体",
  "graph.search": "搜索实体（名称、摘要）…",
  "graph.empty.title": "还没有实体",
  "graph.empty.hint": "创建实体和带日期的事实——支撑多跳推理，智能体可查询。",
  "graph.facts": "事实",
  "graph.asOf": "时间点",
  "graph.asOfPh": "日期，如 2026-01-15…",
  "graph.backToNow": "回到现在",
  "graph.neighbors": "邻域（2 跳）",
  "graph.noFacts": "该时间点暂无事实——可添加，或时间线已前移。",
  "graph.noNeighbors": "还没有连接。",
  "graph.addFact": "添加事实",
  "graph.addFact.title": "为 {name} 添加事实",
  "graph.addFact.desc": "事实是带日期的边。同一方向同一关系的新事实会把旧事实**在此日期失效**——从不删除，“当时为真”永远可查。",
  "graph.addFact.target": "目标实体（名称——不存在则创建）",
  "graph.addFact.targetPh": "Acme…",
  "graph.addFact.relation": "关系",
  "graph.addFact.relationPh": "works_at",
  "graph.addFact.text": "事实内容",
  "graph.addFact.textPh": "Alice works at Acme",
  "graph.addFact.valid": "生效日期（可选——YYYY-MM-DD；留空表示现在）",
  "graph.addFact.btn": "添加事实",
  "graph.newEntity.title": "新建实体",
  "graph.newEntity.name": "名称",
  "graph.newEntity.namePh": "Alice…",
  "graph.newEntity.kind": "类型（可选）",
  "graph.newEntity.kindPh": "person / project / tool…",
  "graph.newEntity.summary": "摘要（可选）",
  "graph.newEntity.btn": "创建",
  "graph.valid": "生效",
  "graph.until": "失效",
  "graph.relation": "关系",
  "graph.fact": "事实",
  "graph.factCount": "{n} 条事实",

  // agents
  "agents.title": "智能体",
  "agents.subtitle": "已注册的执行器与实时表现",
  "agents.empty.title": "没有已注册的智能体",
  "agents.empty.hint": "在 ~/.ruagent/config/agents.toml 添加后重启 daemon。",
  "agents.enabled": "已启用",
  "agents.disabled": "已停用",
  "agents.runs": "运行",
  "agents.success": "成功率",
  "agents.cost": "成本",
  "agents.lastRun": "最近运行",
  "agents.noRuns": "还没有运行",
  "mcp.registry": "MCP 注册表",
  "mcp.empty.title": "没有注册 MCP 服务器",
  "mcp.empty.hint": "在 ~/.ruagent/config/mcp.toml 添加。",
  "mcp.overlay": "注入是叠加层——各 CLI 自己的 MCP 配置不会被改动。",

  // stats
  "stats.title": "统计",
  "stats.subtitle": "各智能体的表现与成本",
  "stats.empty": "还没有运行记录",
  "stats.agent": "智能体",
  "stats.runs": "运行",
  "stats.completed": "成功",
  "stats.failed": "失败",
  "stats.cost": "成本",
  "stats.lastRun": "最近运行",

  // inbox
  "inbox.title": "收件箱",
  "inbox.subtitle": "等待你决定的权限请求",
  "inbox.empty.title": "收件箱是空的",
  "inbox.empty.hint": "当智能体要做规则未覆盖的操作时会出现在这里。配置审批智能体可实现无人值守。",
  "inbox.viewRaw": "查看原始输入",
  "inbox.allow": "允许",
  "inbox.reject": "拒绝",
  "inbox.allowed": "已允许",
  "inbox.rejected": "已拒绝",

  // timeline
  "timeline.live": "实时",
  "timeline.latest": "↓ 最新",
  "timeline.noEvents": "没有事件记录。",
  "timeline.injected": "已注入上下文",
  "timeline.injectedBlocks": "{blocks} · {n} 字符",
  "timeline.routed": "路由：{level}",
  "timeline.permReq": "权限请求：{title}",
  "timeline.permRes": "{outcome} · 由 {who} 裁决",
  "timeline.by.human": "人工",
  "timeline.by.approver": "审批智能体",
  "timeline.by.rule": "规则",
  "timeline.stopped": "已停止：{reason}",
  "timeline.plan": "计划",
  "timeline.toolOutput": "{id} 的输出",

  // statuses
  "status.pending": "待处理",
  "status.in_progress": "进行中",
  "status.blocked": "阻塞",
  "status.done": "已完成",
  "status.cancelled": "已取消",
  "status.completed": "已完成",
  "status.failed": "失败",
  "status.interrupted": "已中断",
  "status.queued": "排队中",
  "status.running": "运行中",
  "status.spawning": "启动中",
  "status.waiting_permission": "等待权限",

  // toasts
  "toast.taskDeleted": "任务已删除",
  "toast.launched": "已启动",
  "toast.winnerSelected": "已选为胜出",
  "toast.cancelling": "正在取消…",
  "toast.entityCreated": "实体已创建",
  "toast.factAdded": "事实已添加",
  "toast.memoryWritten": "记忆已{outcome}",
  "toast.written.inserted": "写入",
  "toast.written.superseded": "替代",
};

const en: Record<string, string> = {
  "nav.board": "Board",
  "nav.memory": "Memory",
  "nav.knowledge": "Knowledge",
  "nav.graph": "Graph",
  "nav.agents": "Agents",
  "nav.stats": "Stats",
  "nav.inbox": "Inbox",

  "common.online": "daemon online",
  "common.offline": "daemon unreachable",
  "common.delete": "Delete",
  "common.keep": "Keep",
  "common.cancel": "Cancel",
  "common.search": "Search",
  "common.clear": "Clear",
  "common.close": "Close",
  "common.create": "Create",

  "home.greeting": "Welcome back",
  "home.subtitle":
    "A local-first agent engineering platform: orchestrate multiple coding agents, unify memory & knowledge, observe everything.",
  "home.stat.agents": "Connected agents",
  "home.stat.tasks": "Total tasks",
  "home.stat.runs": "Total runs",
  "home.stat.memories": "Memories",
  "home.stat.docs": "Knowledge docs",
  "home.start.title": "Get started",
  "home.step1.title": "Create a task",
  "home.step1.desc":
    "Describe what you want done — it becomes the run prompt. The project field scopes project memories.",
  "home.step1.cta": "Create your first task",
  "home.step2.title": "Choose how to run",
  "home.step2.desc":
    "Run on one agent; fan out to several and compare results; or chain a pipeline with handoff.",
  "home.step3.title": "Results & memory",
  "home.step3.desc":
    "Execution logs replay every event (tool calls, permission verdicts, context injection). Agents read/write platform memory via MCP — they remember across sessions.",
  "home.guide.title": "What each section does",
  "home.guide.board": "Task board — create, run, fan-out compare, pipelines",
  "home.guide.memory": "Long-term memory — what agents remember across sessions, auditable",
  "home.guide.knowledge": "Knowledge base — ingest documents, semantically searchable by every agent",
  "home.guide.graph": "Entity graph — dated facts with as-of queries",
  "home.guide.agents": "Registered agents with live performance and cost",
  "home.guide.stats": "Per-agent runs, success rate and cost",
  "home.guide.inbox": "Permission requests awaiting you (configure an approver agent for unattended runs)",

  "board.title": "Tasks",
  "board.total": "{n} total",
  "board.new": "New Task",
  "board.empty.title": "No tasks yet",
  "board.empty.hint": "Create a task, then run it on any agent — or fan out to several and compare.",
  "board.col.pending": "Pending",
  "board.col.in_progress": "In progress",
  "board.col.blocked": "Blocked",
  "board.col.done": "Done",
  "board.list": "List",
  "board.board": "Board",

  "newtask.title": "New Task",
  "newtask.titleLabel": "Title",
  "newtask.intentLabel": "Intent (what should be done — becomes the run prompt)",
  "newtask.projectLabel": "Project (optional — scopes project memories)",
  "newtask.created": "Task created",

  "task.back": "Back to board",
  "task.deleteConfirm.title": "Delete this task?",
  "task.deleteConfirm.body":
    "Deletes the task and its run records (transcripts stay on disk as evidence). This cannot be undone.",
  "task.runs": "Runs",
  "task.runsCount": "({n})",
  "task.totalCost": "{cost} total",
  "task.noRuns": "No runs yet — pick an agent above and launch.",
  "task.log": "Execution Log",
  "task.comparison": "Comparison",
  "task.winner": "winner",
  "task.selectWinner": "Select as Winner",
  "task.viewLog": "View Log",
  "task.cancelRun": "Cancel",
  "task.cancelling": "Cancelling…",
  "task.noResult": "(no result)",
  "task.status": "Task status",

  "launcher.run": "Run",
  "launcher.fanout": "Fan Out",
  "launcher.pipeline": "Pipeline",
  "launcher.prompt": "Prompt (defaults to the task intent)…",
  "launcher.repo": "Git repo (optional — each run gets its own worktree)…",
  "launcher.selected": "{n} selected",
  "launcher.startPipeline": "Start Pipeline",
  "launcher.needs2": "Fan-out needs at least 2 agents",
  "launcher.chooseAgent": "Choose agent…",
  "launcher.addStep": "+ Add step",
  "launcher.removeStep": "Remove this step",
  "launcher.noAgents": "No enabled agents — edit ~/.ruagent/config/agents.toml.",
  "launcher.hint.title": "How do these modes differ?",
  "launcher.hint.body":
    "**Run** sends the prompt to one agent. **Fan Out** sends it to several in parallel (each in its own worktree when a repo is given) and shows results side by side for you to pick a winner. **Pipeline** chains agents — each step receives the previous step's result as handoff context.",

  "memory.title": "Memory",
  "memory.subtitle": "What the platform remembers — every agent reads & writes via MCP",
  "memory.browse": "Browse",
  "memory.audit": "Audit Log",
  "memory.write": "Write Memory",
  "memory.current": "current",
  "memory.superseded": "superseded",
  "memory.replaces": "replaces #{id}",
  "memory.empty.title": "No {store} memories in {ns}",
  "memory.empty.hint": "Agents write via the memory_write tool; you can also write manually.",
  "memory.supersede": "Supersede",
  "memory.supersede.title": "Supersede memory #{id}",
  "memory.supersede.desc":
    "The old content stays in history (the audit log keeps the trail); the new content becomes current.",
  "memory.supersede.btn": "Supersede",
  "memory.write.title": "Write memory",
  "memory.write.desc":
    "One concise fact per write. It will be injected into future runs (bounded) and is searchable by every agent via memory_search.",
  "memory.write.govern":
    "Store / namespace (governed: profile is user-only; procedure/lesson are project/global-only)",
  "memory.write.content": "Content",
  "memory.write.placeholder": "the deploy script lives in scripts/release.sh",
  "memory.write.btn": "Write",
  "memory.audit.empty": "No memory writes yet — every write decision lands here.",
  "memory.nsNew": "new namespace",

  "knowledge.title": "Knowledge",
  "knowledge.subtitle": "Documents every agent can search",
  "knowledge.ingest": "Ingest Document",
  "knowledge.search": "Search documents (semantic + keyword)…",
  "knowledge.noResults": "No results — try different keywords or ingest more.",
  "knowledge.empty.title": "No documents ingested",
  "knowledge.empty.hint":
    "Paste markdown or notes — chunked and embedded, then searchable by every agent via knowledge_search.",
  "knowledge.chunks": "{n} chunks",
  "knowledge.deleted": "deleted {name}",
  "knowledge.ingested": "ingested {n} chunks",
  "knowledge.ingest.title": "Ingest document",
  "knowledge.ingest.name": "Name",
  "knowledge.ingest.namePh": "deploy-runbook…",
  "knowledge.ingest.content": "Content (markdown / notes / code — chunked at ~800 chars with overlap)",
  "knowledge.ingest.btn": "Ingest",
  "knowledge.score": "score {s}",

  "graph.title": "Entity graph",
  "graph.subtitle": "Facts with two timelines — invalidated, never deleted",
  "graph.newEntity": "New Entity",
  "graph.search": "Search entities (name, summary)…",
  "graph.empty.title": "No entities yet",
  "graph.empty.hint": "Create entities and dated facts — they power multi-hop reasoning.",
  "graph.facts": "Facts",
  "graph.asOf": "as of",
  "graph.asOfPh": "date, e.g. 2026-01-15…",
  "graph.backToNow": "back to now",
  "graph.neighbors": "Neighborhood (2 hops)",
  "graph.noFacts": "No facts at this point in time — add one, or the timeline has moved on.",
  "graph.noNeighbors": "Nothing connected yet.",
  "graph.addFact": "Add Fact",
  "graph.addFact.title": "Add fact about {name}",
  "graph.addFact.desc":
    "A fact is a dated edge. A new fact with the same source → target + relation **invalidates the old one at this fact's date** — nothing is ever deleted, so “what was true as of X” always stays answerable.",
  "graph.addFact.target": "Target entity (name — created if new)",
  "graph.addFact.targetPh": "Acme…",
  "graph.addFact.relation": "Relation",
  "graph.addFact.relationPh": "works_at",
  "graph.addFact.text": "Fact text",
  "graph.addFact.textPh": "Alice works at Acme",
  "graph.addFact.valid": "Valid from (optional — YYYY-MM-DD; empty = now)",
  "graph.addFact.btn": "Add fact",
  "graph.newEntity.title": "New entity",
  "graph.newEntity.name": "Name",
  "graph.newEntity.namePh": "Alice…",
  "graph.newEntity.kind": "Kind (optional)",
  "graph.newEntity.kindPh": "person / project / tool…",
  "graph.newEntity.summary": "Summary (optional)",
  "graph.newEntity.btn": "Create",
  "graph.valid": "valid",
  "graph.until": "until",
  "graph.relation": "relation",
  "graph.fact": "fact",
  "graph.factCount": "{n} facts",

  "agents.title": "Agents",
  "agents.subtitle": "Registered harnesses with live performance",
  "agents.empty.title": "No agents registered",
  "agents.empty.hint": "Add them to ~/.ruagent/config/agents.toml and restart the daemon.",
  "agents.enabled": "enabled",
  "agents.disabled": "disabled",
  "agents.runs": "runs",
  "agents.success": "success",
  "agents.cost": "cost",
  "agents.lastRun": "last run",
  "agents.noRuns": "no runs yet",
  "mcp.registry": "MCP Registry",
  "mcp.empty.title": "No MCP servers registered",
  "mcp.empty.hint": "Add them to ~/.ruagent/config/mcp.toml.",
  "mcp.overlay": "Injection is an overlay — each CLI's own MCP config is never touched.",

  "stats.title": "Stats",
  "stats.subtitle": "Per-agent performance and cost",
  "stats.empty": "No runs recorded yet",
  "stats.agent": "Agent",
  "stats.runs": "Runs",
  "stats.completed": "Completed",
  "stats.failed": "Failed",
  "stats.cost": "Cost",
  "stats.lastRun": "Last Run",

  "inbox.title": "Inbox",
  "inbox.subtitle": "Permission requests waiting on you",
  "inbox.empty.title": "Inbox empty",
  "inbox.empty.hint":
    "When an agent wants something the rules don't cover, it lands here. Configure an approver agent for unattended operation.",
  "inbox.viewRaw": "View Raw Input",
  "inbox.allow": "Allow",
  "inbox.reject": "Reject",
  "inbox.allowed": "allowed",
  "inbox.rejected": "rejected",

  "timeline.live": "live",
  "timeline.latest": "↓ latest",
  "timeline.noEvents": "No events recorded.",
  "timeline.injected": "context injected",
  "timeline.injectedBlocks": "{blocks} · {n} chars",
  "timeline.routed": "routed via {level}",
  "timeline.permReq": "permission requested: {title}",
  "timeline.permRes": "{outcome} · by {who}",
  "timeline.by.human": "human",
  "timeline.by.approver": "approver agent",
  "timeline.by.rule": "rule",
  "timeline.stopped": "stopped: {reason}",
  "timeline.plan": "plan",
  "timeline.toolOutput": "output of {id}",

  "status.pending": "pending",
  "status.in_progress": "in progress",
  "status.blocked": "blocked",
  "status.done": "done",
  "status.cancelled": "cancelled",
  "status.completed": "completed",
  "status.failed": "failed",
  "status.interrupted": "interrupted",
  "status.queued": "queued",
  "status.running": "running",
  "status.spawning": "spawning",
  "status.waiting_permission": "waiting for permission",

  "toast.taskDeleted": "task deleted",
  "toast.launched": "launched",
  "toast.winnerSelected": "winner selected",
  "toast.cancelling": "cancelling…",
  "toast.entityCreated": "entity created",
  "toast.factAdded": "fact added",
  "toast.memoryWritten": "memory {outcome}",
  "toast.written.inserted": "written",
  "toast.written.superseded": "superseded",
};

const dicts: Record<Lang, Record<string, string>> = { zh, en };

interface I18n {
  lang: Lang;
  setLang: (l: Lang) => void;
  t: (key: string, params?: Record<string, string | number>) => string;
}

const Ctx = createContext<I18n | null>(null);

export function I18nProvider({ children }: { children: ReactNode }) {
  const [lang, setLang] = useState<Lang>(() => {
    try {
      const saved = localStorage.getItem("ruagent.lang");
      if (saved === "zh" || saved === "en") return saved;
    } catch {
      /* private mode */
    }
    return navigator.language.toLowerCase().startsWith("zh") ? "zh" : "en";
  });

  useEffect(() => {
    try {
      localStorage.setItem("ruagent.lang", lang);
    } catch {
      /* ignore */
    }
    document.documentElement.lang = lang === "zh" ? "zh-CN" : "en";
  }, [lang]);

  const t = (key: string, params?: Record<string, string | number>) => {
    let s = dicts[lang][key] ?? dicts.en[key] ?? key;
    if (params) {
      for (const [k, v] of Object.entries(params)) s = s.replaceAll(`{${k}}`, String(v));
    }
    return s;
  };

  return <Ctx.Provider value={{ lang, setLang, t }}>{children}</Ctx.Provider>;
}

export function useI18n(): I18n {
  const v = useContext(Ctx);
  if (!v) throw new Error("useI18n must be used inside I18nProvider");
  return v;
}

// ---------------------------------------------------------------------------
// Locale-aware relative time
// ---------------------------------------------------------------------------

export function relTime(iso: string | null | undefined, lang: Lang): string {
  if (!iso) return "—";
  const then = new Date(iso.endsWith("Z") || iso.includes("+") ? iso : `${iso}Z`).getTime();
  if (Number.isNaN(then)) return iso;
  const diff = (Date.now() - then) / 1000;
  if (lang === "zh") {
    if (diff < 10) return "刚刚";
    if (diff < 60) return `${Math.floor(diff)} 秒前`;
    if (diff < 3600) return `${Math.floor(diff / 60)} 分钟前`;
    if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`;
    if (diff < 86400 * 7) return `${Math.floor(diff / 86400)} 天前`;
    return new Date(then).toLocaleDateString("zh-CN");
  }
  if (diff < 10) return "just now";
  if (diff < 60) return `${Math.floor(diff)}s ago`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  if (diff < 86400 * 7) return `${Math.floor(diff / 86400)}d ago`;
  return new Date(then).toLocaleDateString("en-US");
}

export function dateOf(iso: string): string {
  return (iso.split("T")[0] || "").replace(/Z$/, "");
}
