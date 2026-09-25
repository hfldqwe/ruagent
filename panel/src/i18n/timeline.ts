// i18n domain: timeline.*
//
// Copy for this domain only. New strings for a view belong in THAT view domain
// file (see panel/src/i18n/README.md and the header of panel/src/i18n.tsx): the point
// of the split is that two tasks editing two different views never touch one file.
// Keep both languages in step: a key added here must be added to zh AND en.

export const zh: Record<string, string> = {
  "timeline.live": "实时",
  "timeline.latest": "↓ 最新",
  "timeline.noEvents": "没有事件记录。",
  "timeline.injected": "已注入上下文",
  "timeline.asked": "原始指令",
  "timeline.injectedBlocks": "{blocks} · {n} 字符",
  "timeline.routed": "路由：{level}",
  "timeline.permReq": "权限请求：{title}",
  "timeline.permRes": "{outcome} · 由 {who} 裁决",
  "timeline.by.human": "人工",
  "timeline.by.approver": "审批智能体",
  "timeline.by.rule": "规则",
  "timeline.stopped": "已停止：{reason}",
  "timeline.plan": "计划",
  "timeline.toolOutput": "{id} 的输出",
};

export const en: Record<string, string> = {
  "timeline.live": "live",
  "timeline.latest": "↓ latest",
  "timeline.noEvents": "No events recorded.",
  "timeline.injected": "context injected",
  "timeline.asked": "the ask",
  "timeline.injectedBlocks": "{blocks} · {n} chars",
  "timeline.routed": "routed via {level}",
  "timeline.permReq": "permission requested: {title}",
  "timeline.permRes": "{outcome} · by {who}",
  "timeline.by.human": "human",
  "timeline.by.approver": "approver agent",
  "timeline.by.rule": "rule",
  "timeline.stopped": "stopped: {reason}",
  "timeline.plan": "plan",
  "timeline.toolOutput": "output of {id}",
};
