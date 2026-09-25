// i18n domain: settings.*
//
// Copy for this domain only. New strings for a view belong in THAT view domain
// file (see panel/src/i18n/README.md and the header of panel/src/i18n.tsx): the point
// of the split is that two tasks editing two different views never touch one file.
// Keep both languages in step: a key added here must be added to zh AND en.

export const zh: Record<string, string> = {
  "settings.title": "设置",
  "settings.subtitle": "蒸馏策略",
  "settings.noRoles": "没有可选角色",
};

export const en: Record<string, string> = {
  "settings.title": "Settings",
  "settings.subtitle": "distillation policy",
  "settings.noRoles": "No roles available",
};
