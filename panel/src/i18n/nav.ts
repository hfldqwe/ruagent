// i18n domain: nav.*
//
// Copy for this domain only. New strings for a view belong in THAT view domain
// file (see panel/src/i18n/README.md and the header of panel/src/i18n.tsx): the point
// of the split is that two tasks editing two different views never touch one file.
// Keep both languages in step: a key added here must be added to zh AND en.

export const zh: Record<string, string> = {
  "nav.home": "首页",
  "nav.task": "任务详情",
  "nav.group.work": "工作区",
  "nav.group.knowledge": "知识与记忆",
  "nav.group.system": "系统",
  "nav.board": "看板",
  "nav.memory": "记忆",
  "nav.knowledge": "知识库",
  "nav.graph": "图谱",
  "nav.agents": "智能体",
  "nav.stats": "统计",
  "nav.settings": "设置",
  "nav.inbox": "收件箱",
  "nav.runtimes": "运行时",
};

export const en: Record<string, string> = {
  "nav.home": "Home",
  "nav.task": "Task detail",
  "nav.group.work": "Workspace",
  "nav.group.knowledge": "Knowledge",
  "nav.group.system": "System",
  "nav.board": "Board",
  "nav.memory": "Memory",
  "nav.knowledge": "Knowledge",
  "nav.graph": "Graph",
  "nav.agents": "Agents",
  "nav.stats": "Stats",
  "nav.settings": "Settings",
  "nav.inbox": "Inbox",
  "nav.runtimes": "Runtimes",
};
