// i18n domain: stats.*
//
// Copy for this domain only. New strings for a view belong in THAT view domain
// file (see panel/src/i18n/README.md and the header of panel/src/i18n.tsx): the point
// of the split is that two tasks editing two different views never touch one file.
// Keep both languages in step: a key added here must be added to zh AND en.

export const zh: Record<string, string> = {
  "stats.title": "统计",
  "stats.subtitle": "各智能体的表现与成本",
  "stats.empty": "还没有运行记录",
  "stats.recallLog": "最近召回",
  "stats.agent": "智能体",
  "stats.runs": "运行",
  "stats.completed": "成功",
  "stats.failed": "失败",
  "stats.cost": "成本",
  "stats.lastRun": "最近运行",
  "stats.err": "无法读取统计",
  "stats.err.hint": "daemon 不可达。上一次的账本不会被清零。",
  "stats.stale.hint": "显示的是上一次成功读取的账本，5 秒轮询正在重试。",
  "stats.topN": "前 {n}",
  "stats.rows": "{n} 条",
};

export const en: Record<string, string> = {
  "stats.title": "Stats",
  "stats.subtitle": "Per-agent performance and cost",
  "stats.empty": "No runs recorded yet",
  "stats.recallLog": "Recent recalls",
  "stats.agent": "Agent",
  "stats.runs": "Runs",
  "stats.completed": "Completed",
  "stats.failed": "Failed",
  "stats.cost": "Cost",
  "stats.lastRun": "Last Run",
  "stats.err": "Cannot read stats",
  "stats.err.hint": "The daemon is unreachable. The previous ledger is not zeroed out.",
  "stats.stale.hint": "Showing the last successful ledger; the 5s poll is retrying.",
  "stats.topN": "top {n}",
  "stats.rows": "{n} rows",
};
