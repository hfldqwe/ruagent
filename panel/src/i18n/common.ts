// i18n domain: common.*
//
// Copy for this domain only. New strings for a view belong in THAT view domain
// file (see panel/src/i18n/README.md and the header of panel/src/i18n.tsx): the point
// of the split is that two tasks editing two different views never touch one file.
// Keep both languages in step: a key added here must be added to zh AND en.

export const zh: Record<string, string> = {
  "common.online": "daemon 在线",
  "common.loading": "加载中…",
  "common.offlineBanner": "daemon 不可达 — 无法加载数据，正在自动重试",
  "common.toggleSidebar": "收起 / 展开侧栏",
  "common.buildId": "面板构建 {id}",
  "common.offline": "daemon 不可达",
  "common.delete": "删除",
  "common.keep": "保留",
  "common.cancel": "取消",
  "common.search": "搜索",
  "common.clear": "清除",
  "common.close": "关闭",
  "common.create": "创建",
  "common.edit": "编辑",
  "common.save": "保存",
  "common.allow": "允许",
  "common.reject": "拒绝",
  "common.retry": "重试",
  "common.stale": "数据可能过期",
};

export const en: Record<string, string> = {
  "common.online": "daemon online",
  "common.loading": "Loading…",
  "common.offlineBanner": "daemon unreachable — data may be stale, retrying automatically",
  "common.toggleSidebar": "Collapse / expand sidebar",
  "common.buildId": "Panel build {id}",
  "common.offline": "daemon unreachable",
  "common.delete": "Delete",
  "common.keep": "Keep",
  "common.cancel": "Cancel",
  "common.search": "Search",
  "common.clear": "Clear",
  "common.close": "Close",
  "common.create": "Create",
  "common.edit": "Edit",
  "common.save": "Save",
  "common.allow": "Allow",
  "common.reject": "Reject",
  "common.retry": "Retry",
  "common.stale": "data may be stale",
};
