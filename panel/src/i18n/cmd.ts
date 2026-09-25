// i18n domain: cmd.*
//
// Copy for this domain only. New strings for a view belong in THAT view domain
// file (see panel/src/i18n/README.md and the header of panel/src/i18n.tsx): the point
// of the split is that two tasks editing two different views never touch one file.
// Keep both languages in step: a key added here must be added to zh AND en.

export const zh: Record<string, string> = {
  "cmd.placeholder": "搜索命令…",
  "cmd.list": "命令",
  "cmd.foot": "↑↓ 选择 · Enter 执行 · Esc 关闭",
  "cmd.empty": "没有匹配的命令",
  "cmd.group.nav": "导航",
  "cmd.group.agents": "智能体",
  "cmd.group.sessions": "会话",
  "cmd.group.actions": "动作",
  "cmd.talkTo": "对话：{name}",
  "cmd.session": "会话：{title}",
  "cmd.toggleLang": "切换语言",
  "cmd.loadFailed": "角色与会话未能载入——列表只剩导航项。",
};

export const en: Record<string, string> = {
  "cmd.placeholder": "Search commands…",
  "cmd.list": "Commands",
  "cmd.foot": "↑↓ select · Enter run · Esc close",
  "cmd.empty": "No matching commands",
  "cmd.group.nav": "Navigation",
  "cmd.group.agents": "Agents",
  "cmd.group.sessions": "Sessions",
  "cmd.group.actions": "Actions",
  "cmd.talkTo": "Chat: {name}",
  "cmd.session": "Session: {title}",
  "cmd.toggleLang": "Switch language",
  "cmd.loadFailed": "Roles and sessions could not be loaded — the list shows navigation only.",
};
