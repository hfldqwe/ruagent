// i18n domain: sider.*
//
// Copy for this domain only. New strings for a view belong in THAT view domain
// file (see panel/src/i18n/README.md and the header of panel/src/i18n.tsx): the point
// of the split is that two tasks editing two different views never touch one file.
// Keep both languages in step: a key added here must be added to zh AND en.

export const zh: Record<string, string> = {
  "sider.collapse": "收起侧栏",
  "sider.expand": "展开侧栏",
  "sider.resizeNav": "拖动调整导航栏宽度",
  "sider.resizeChat": "拖动调整会话栏宽度",
};

export const en: Record<string, string> = {
  "sider.collapse": "Collapse sidebar",
  "sider.expand": "Expand sidebar",
  "sider.resizeNav": "Resize the navigation rail",
  "sider.resizeChat": "Resize the session rail",
};
