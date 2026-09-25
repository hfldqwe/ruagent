// i18n domain: newtask.*
//
// Copy for this domain only. New strings for a view belong in THAT view domain
// file (see panel/src/i18n/README.md and the header of panel/src/i18n.tsx): the point
// of the split is that two tasks editing two different views never touch one file.
// Keep both languages in step: a key added here must be added to zh AND en.

export const zh: Record<string, string> = {
  "newtask.title": "新建任务",
  "newtask.titleLabel": "标题",
  "newtask.titlePh": "修复登录 bug…",
  "newtask.intentLabel": "意图（要做什么——会成为运行提示词）",
  "newtask.intentPh": "会话 cookie 过期时登录流程 500…",
  "newtask.projectLabel": "项目（可选——限定项目记忆范围）",
  "newtask.created": "任务已创建",
};

export const en: Record<string, string> = {
  "newtask.title": "New Task",
  "newtask.titleLabel": "Title",
  "newtask.titlePh": "Fix the login bug…",
  "newtask.intentLabel": "Intent (what should be done — becomes the run prompt)",
  "newtask.intentPh": "The login flow 500s when the session cookie expires…",
  "newtask.projectLabel": "Project (optional — scopes project memories)",
  "newtask.created": "Task created",
};
