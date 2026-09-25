// i18n domain: runtimes.*
//
// Copy for this domain only. New strings for a view belong in THAT view domain
// file (see panel/src/i18n/README.md and the header of panel/src/i18n.tsx): the point
// of the split is that two tasks editing two different views never touch one file.
// Keep both languages in step: a key added here must be added to zh AND en.

export const zh: Record<string, string> = {
  "runtimes.create": "新建运行时",
  "runtimes.editTitle": "编辑运行时 {name}",
  "runtimes.created": "运行时 {name} 已创建",
  "runtimes.updated": "运行时 {name} 已更新",
  "runtimes.deleted": "运行时 {name} 已删除",
  "runtimes.deleteConfirm.title": "删除运行时 {name}？",
  "runtimes.deleteConfirm.body": "将从 agents.toml 移除此执行后端（有角色使用时会被拒绝）。",
  "runtimes.f.name": "名称",
  "runtimes.f.harness": "引擎类型",
  "runtimes.f.command": "启动命令",
  "runtimes.f.description": "描述",
  "runtimes.f.mcp": "MCP profile",
  "runtimes.f.models": "模型列表（逗号分隔，可选）",
  "runtimes.models": "{n} 个模型",
  "runtimes.title": "运行时",
  "runtimes.subtitle": "执行后端——角色在其上运行，可随时切换",
  "runtimes.hint": "运行时在 ~/.ruagent/config/agents.toml 的 [runtime.*] 段定义；角色通过 runtimes = […] 声明可用的后端。",
  "runtimes.err": "无法读取运行时",
  "runtimes.err.hint": "daemon 不可达或接口出错。这不等于「没有注册的运行时」。",
  "runtimes.stale.hint": "显示的是上一次成功读取的结果，5 秒轮询正在重试。",
  "runtimes.notProbed": "未探测",
  "runtimes.probeFailed": "探测失败 · 重试",
  "runtimes.usedBy": "受影响角色：{names}",
};

export const en: Record<string, string> = {
  "runtimes.create": "New Runtime",
  "runtimes.editTitle": "Edit runtime {name}",
  "runtimes.created": "runtime {name} created",
  "runtimes.updated": "runtime {name} updated",
  "runtimes.deleted": "runtime {name} deleted",
  "runtimes.deleteConfirm.title": "Delete runtime {name}?",
  "runtimes.deleteConfirm.body": "Removes this execution backend from agents.toml (refused while a role uses it).",
  "runtimes.f.name": "Name",
  "runtimes.f.harness": "Engine type",
  "runtimes.f.command": "Spawn command",
  "runtimes.f.description": "Description",
  "runtimes.f.mcp": "MCP profile",
  "runtimes.f.models": "Models (comma-separated, optional)",
  "runtimes.models": "{n} models",
  "runtimes.title": "Runtimes",
  "runtimes.subtitle": "Execution backends — roles run on them, swappable any time",
  "runtimes.hint": "Runtimes are defined in the [runtime.*] sections of ~/.ruagent/config/agents.toml; roles declare the backends they can run on via runtimes = […].",
  "runtimes.err": "Cannot read runtimes",
  "runtimes.err.hint": "The daemon is unreachable or the endpoint errored. That is not the same as “no runtimes registered”.",
  "runtimes.stale.hint": "Showing the last successful read; the 5s poll is retrying.",
  "runtimes.notProbed": "not probed",
  "runtimes.probeFailed": "probe failed · retry",
  "runtimes.usedBy": "Affected roles: {names}",
};
