// i18n domain: metric.*
//
// Copy for this domain only. New strings for a view belong in THAT view domain
// file (see panel/src/i18n/README.md and the header of panel/src/i18n.tsx): the point
// of the split is that two tasks editing two different views never touch one file.
// Keep both languages in step: a key added here must be added to zh AND en.

export const zh: Record<string, string> = {
  "metric.chunks": "分块",
  "metric.entities": "实体",
  "metric.messages": "消息",
  "metric.models": "模型",
};

export const en: Record<string, string> = {
  "metric.chunks": "Chunks",
  "metric.entities": "Entities",
  "metric.messages": "Messages",
  "metric.models": "Models",
};
