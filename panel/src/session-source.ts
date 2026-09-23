// Session source taxonomy — the display label a session's `source` wears.
//
// This is a PURE CONSTANT module on purpose (MASTER §12 row 27). It used to
// live in `views/Sessions.tsx`, and CommandPalette — which is part of the
// entry chunk — imported it from there, so a route's module sat in the
// entry's dependency graph: whatever Sessions.tsx grew, the entry grew.
// A three-entry label map has no business dragging a view along.
//
// `views/Sessions.tsx` re-exports it, so Home / SessionsView keep importing
// the same names from the same path (single source of truth, two entry
// points).

export const SOURCE_LABEL: Record<string, string> = {
  "claude-code": "Claude Code",
  dsh: "dsh",
  ruagent: "ruagent",
};
