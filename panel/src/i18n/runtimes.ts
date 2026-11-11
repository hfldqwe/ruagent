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
  // t76: the page-level verdict -- the answer a fresh machine needs. Every state
  // names what to DO next, because "3 red badges and no summary" was the report.
  "runtimes.fleet.ok.title": "{total} 个运行时全部可启动",
  "runtimes.fleet.ok.detail":
    "每个运行时的命令都能在这台机器上解析到。要让 daemon 真正启动一次做端到端确认，点卡片上的「测试连接」。",
  "runtimes.fleet.partial.title": "{total} 个运行时里有 {unresolved} 个找不到命令",
  "runtimes.fleet.partial.detail":
    "其余 {ok} 个可以启动。红标签卡片上的命令在这台机器上解析不到：装上对应的命令行工具，或到「编辑」里把 command 改成二进制全路径。",
  "runtimes.fleet.none.title": "这台机器上没有任何运行时能启动（0/{total}）",
  "runtimes.fleet.none.detail":
    "全部 {total} 个运行时的 command 都解析不到。下一步：按卡片上的提示安装对应的命令行工具，或到「编辑」里把该运行时的 command 改成二进制全路径（用 npx 装的要给完整路径）。装好后点卡片上的「测试连接」确认。",
  "runtimes.fleet.unknown.title": "{total} 个运行时有 {unknown} 个未报告命令是否可解析",
  "runtimes.fleet.unknown.detail":
    "这个 daemon 不报告 command_resolved，所以这里既不写「可用」也不写「找不到」。点卡片上的「测试连接」可以直接问它。",
  "runtimes.cmd.resolved": "命令可解析",
  "runtimes.cmd.path": "解析到：{path}",
  "runtimes.cmd.notFound": "找不到命令",
  "runtimes.cmd.notFound.hint": "agents.toml 里保存的 command 在这台机器上解析不到。点「测试连接」看 daemon 的原始原因；到「编辑」里改 command，或先把它装上。",
  "runtimes.cmd.unknown": "未知 · daemon 未报告命令是否可解析",
  "runtimes.cmd.unknown.hint": "这个 daemon 不报告 command_resolved，所以这里既不写「可用」也不写「找不到」。点「测试连接」可以直接问它。",
  "runtimes.test": "测试连接",
  "runtimes.testing": "测试中…",
  "runtimes.probe.ok": "接得通",
  "runtimes.probe.bad": "接不通",
  "runtimes.probe.stage": "阶段：",
  "runtimes.probe.detail": "原因：",
  "runtimes.probe.noDetail": "（daemon 没有给出 detail）",
  "runtimes.probe.noRoute": "这个 daemon 没有探测接口（HTTP {status}）——这不等于接得通。",
  "runtimes.probe.requestFailed": "探测请求失败：{err}",
  "runtimes.saved.butNotFound": "已保存，但命令找不到。",
  "runtimes.saved.butNotFound.hint": "保存只写入 agents.toml；接不通要「测试连接」或第一次对话才会暴露。",
  "runtimes.saved.unknownResolve": "已保存（这个 daemon 未报告命令是否可解析）。",
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
  // t76: the page-level verdict -- what a fresh machine needs to be told.
  "runtimes.fleet.ok.title": "all {total} runtimes can start",
  "runtimes.fleet.ok.detail":
    "Every runtime's command resolves on this machine. To have the daemon really start one end to end, press \"Test connection\" on its card.",
  "runtimes.fleet.partial.title": "{unresolved} of {total} runtimes cannot find their command",
  "runtimes.fleet.partial.detail":
    "The other {ok} can start. On the red cards the command does not resolve on this machine: install the matching command-line tool, or edit the runtime and give the full path of the binary.",
  "runtimes.fleet.none.title": "no runtime can start on this machine (0/{total})",
  "runtimes.fleet.none.detail":
    "None of the {total} runtimes' commands resolve. Next step: install the command-line tool each card names, or edit the runtime and give the full path of the binary (an npx install needs its full path). Then press \"Test connection\" on the card to confirm.",
  "runtimes.fleet.unknown.title": "{unknown} of {total} runtimes did not report whether their command resolves",
  "runtimes.fleet.unknown.detail":
    "This daemon does not report command_resolved, so neither \"available\" nor \"not found\" is written here. \"Test connection\" on a card asks it directly.",
  "runtimes.cmd.resolved": "command resolves",
  "runtimes.cmd.path": "resolves to: {path}",
  "runtimes.cmd.notFound": "command not found",
  "runtimes.cmd.notFound.hint": "The command saved in agents.toml does not resolve on this machine. \"Test connection\" shows the daemon's own reason; fix the command under Edit, or install it first.",
  "runtimes.cmd.unknown": "unknown · the daemon did not report whether the command resolves",
  "runtimes.cmd.unknown.hint": "This daemon does not report command_resolved, so this card says neither \"available\" nor \"not found\". \"Test connection\" asks it directly.",
  "runtimes.test": "Test connection",
  "runtimes.testing": "testing…",
  "runtimes.probe.ok": "reachable",
  "runtimes.probe.bad": "not reachable",
  "runtimes.probe.stage": "stage: ",
  "runtimes.probe.detail": "detail: ",

  "runtimes.probe.noDetail": "(the daemon gave no detail)",
  "runtimes.probe.noRoute": "this daemon has no probe route (HTTP {status}) — which is not the same as reachable.",
  "runtimes.probe.requestFailed": "probe request failed: {err}",
  "runtimes.saved.butNotFound": "Saved — but the command cannot be found.",
  "runtimes.saved.butNotFound.hint": "Saving only writes agents.toml; that it cannot start shows up in \"Test connection\" or in the first chat.",
  "runtimes.saved.unknownResolve": "Saved (this daemon does not report whether the command resolves).",
  "runtimes.usedBy": "Affected roles: {names}",
};
