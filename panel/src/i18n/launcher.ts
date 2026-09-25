// i18n domain: launcher.*
//
// Copy for this domain only. New strings for a view belong in THAT view domain
// file (see panel/src/i18n/README.md and the header of panel/src/i18n.tsx): the point
// of the split is that two tasks editing two different views never touch one file.
// Keep both languages in step: a key added here must be added to zh AND en.

export const zh: Record<string, string> = {
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
};

export const en: Record<string, string> = {
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
  "launcher.hint.body": "**Run** sends the prompt to one agent. **Fan Out** sends it to several in parallel (each in its own worktree when a repo is given) and shows results side by side for you to pick a winner. **Pipeline** chains agents — each step receives the previous step's result as handoff context.",
};
