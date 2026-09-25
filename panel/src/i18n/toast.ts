// i18n domain: toast.*
//
// Copy for this domain only. New strings for a view belong in THAT view domain
// file (see panel/src/i18n/README.md and the header of panel/src/i18n.tsx): the point
// of the split is that two tasks editing two different views never touch one file.
// Keep both languages in step: a key added here must be added to zh AND en.

export const zh: Record<string, string> = {
  "toast.retrying": "已重新发起运行",
  "toast.taskDeleted": "任务已删除",
  "toast.launched": "已启动",
  "toast.winnerSelected": "已选为胜出",
  "toast.landed": "已落盘：{hash}",
  "toast.judgeStarted": "评审已启动",
  "toast.cancelling": "正在取消…",
  "toast.entityCreated": "实体已创建",
  "toast.factAdded": "事实已添加",
  "toast.memoryWritten": "记忆已{outcome}",
  "toast.written.inserted": "写入",
  "toast.written.superseded": "替代",
};

export const en: Record<string, string> = {
  "toast.retrying": "Run re-launched",
  "toast.taskDeleted": "task deleted",
  "toast.launched": "launched",
  "toast.winnerSelected": "winner selected",
  "toast.landed": "landed at {hash}",
  "toast.judgeStarted": "judge started",
  "toast.cancelling": "cancelling…",
  "toast.entityCreated": "entity created",
  "toast.factAdded": "fact added",
  "toast.memoryWritten": "memory {outcome}",
  "toast.written.inserted": "written",
  "toast.written.superseded": "superseded",
};
