// i18n domain: mcp.*
//
// Copy for this domain only. New strings for a view belong in THAT view domain
// file (see panel/src/i18n/README.md and the header of panel/src/i18n.tsx): the point
// of the split is that two tasks editing two different views never touch one file.
// Keep both languages in step: a key added here must be added to zh AND en.

export const zh: Record<string, string> = {
  "mcp.registry": "MCP 注册表",
  "mcp.servers": "{n} 个服务器",
  "mcp.empty.title": "没有注册 MCP 服务器",
  "mcp.empty.hint": "在 ~/.ruagent/config/mcp.toml 添加。",
  "mcp.overlay": "注入是叠加层——各 CLI 自己的 MCP 配置不会被改动。",
  "mcp.up": "在线",
  "mcp.down": "已下线",
  "mcp.tools": "个工具",
  "mcp.profiles": "配置档",
};

export const en: Record<string, string> = {
  "mcp.registry": "MCP Registry",
  "mcp.servers": "{n} servers",
  "mcp.empty.title": "No MCP servers registered",
  "mcp.empty.hint": "Add them to ~/.ruagent/config/mcp.toml.",
  "mcp.overlay": "Injection is an overlay — each CLI's own MCP config is never touched.",
  "mcp.up": "up",
  "mcp.down": "down",
  "mcp.tools": "tools",
  "mcp.profiles": "Profiles",
};
