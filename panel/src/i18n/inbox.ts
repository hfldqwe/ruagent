// i18n domain: inbox.*
//
// Copy for this domain only. New strings for a view belong in THAT view domain
// file (see panel/src/i18n/README.md and the header of panel/src/i18n.tsx): the point
// of the split is that two tasks editing two different views never touch one file.
// Keep both languages in step: a key added here must be added to zh AND en.

export const zh: Record<string, string> = {
  "inbox.title": "收件箱",
  "inbox.subtitle": "等待你决定的权限请求",
  "inbox.empty.title": "收件箱是空的",
  "inbox.empty.hint": "当智能体要做规则未覆盖的操作时会出现在这里。配置审批智能体可实现无人值守。",
  "inbox.viewRaw": "查看原始输入",
  "inbox.allow": "允许",
  "inbox.reject": "拒绝",
  "inbox.allowed": "已允许",
  "inbox.rejected": "已拒绝",
  "inbox.err": "无法读取权限队列",
  "inbox.err.hint": "运行可能正被阻塞在这上面——不要把读不到当成「没有待裁决项」。",
  "inbox.stale.hint": "显示的是上一次成功读取的队列，2 秒轮询正在重试。",
  "inbox.resolveErr": "裁决失败——卡片保持可见，请重试。",
  "inbox.moreOptions": "更多选项",
  "inbox.tooMany": "还有 {n} 条，请先处理上面的",
  "inbox.rawTooBig": "输入过大（{kb} KB），不在此渲染",
};

export const en: Record<string, string> = {
  "inbox.title": "Inbox",
  "inbox.subtitle": "Permission requests waiting on you",
  "inbox.empty.title": "Inbox empty",
  "inbox.empty.hint": "When an agent wants something the rules don't cover, it lands here. Configure an approver agent for unattended operation.",
  "inbox.viewRaw": "View Raw Input",
  "inbox.allow": "Allow",
  "inbox.reject": "Reject",
  "inbox.allowed": "allowed",
  "inbox.rejected": "rejected",
  "inbox.err": "Cannot read the permission queue",
  "inbox.err.hint": "A run may be blocked on this — do not read a failed fetch as “nothing pending”.",
  "inbox.stale.hint": "Showing the last successful queue; the 2s poll is retrying.",
  "inbox.resolveErr": "The decision failed — the card stays visible, try again.",
  "inbox.moreOptions": "More options",
  "inbox.tooMany": "{n} more — clear the ones above first",
  "inbox.rawTooBig": "Input too large ({kb} KB), not rendered here",
};
