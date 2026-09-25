// i18n domain: board.*
//
// Copy for this domain only. New strings for a view belong in THAT view domain
// file (see panel/src/i18n/README.md and the header of panel/src/i18n.tsx): the point
// of the split is that two tasks editing two different views never touch one file.
// Keep both languages in step: a key added here must be added to zh AND en.

export const zh: Record<string, string> = {
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
  "board.err": "无法加载任务列表",
  "board.err.hint": "daemon 不可达或接口出错。这不等于「还没有任务」。",
  "board.stale.hint": "显示的是上一次成功读取的结果，3 秒轮询正在重试。",
  "board.unknown": "{n} 个未知状态",
  "board.more": "还有 {n} 个",
};

export const en: Record<string, string> = {
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
  "board.err": "Cannot load the task list",
  "board.err.hint": "The daemon is unreachable or the endpoint errored. That is not the same as “no tasks yet”.",
  "board.stale.hint": "Showing the last successful read; the 3s poll is retrying.",
  "board.unknown": "{n} unknown status",
  "board.more": "{n} more",
};
