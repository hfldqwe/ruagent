// i18n domain: theme.*
//
// Copy for this domain only. New strings for a view belong in THAT view domain
// file (see panel/src/i18n/README.md and the header of panel/src/i18n.tsx): the point
// of the split is that two tasks editing two different views never touch one file.
// Keep both languages in step: a key added here must be added to zh AND en.

export const zh: Record<string, string> = {
  "theme.dark": "切换到夜间",
  "theme.light": "切换到日间",
  "theme.toggle": "切换主题",
};

export const en: Record<string, string> = {
  "theme.dark": "Switch to dark",
  "theme.light": "Switch to light",
  "theme.toggle": "Toggle theme",
};
