// i18n domain: status.*
//
// Copy for this domain only. New strings for a view belong in THAT view domain
// file (see panel/src/i18n/README.md and the header of panel/src/i18n.tsx): the point
// of the split is that two tasks editing two different views never touch one file.
// Keep both languages in step: a key added here must be added to zh AND en.

export const zh: Record<string, string> = {
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
};

export const en: Record<string, string> = {
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
};
