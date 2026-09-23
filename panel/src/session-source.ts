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

/** Session origin hygiene (contract S7): the default list must not mix in the
 *  sessions the platform generated for itself, and they must stay reachable
 *  through an explicit control — the shape the archived marker already has.
 *
 *  Both rules read a literal the PLATFORM wrote, never a guess about what the
 *  user typed:
 *
 *  · **temporary workspace** — the session's `project` (the workspace the row
 *    shows and the workspace filter lists) contains a path segment that IS
 *    `temp` or `tmp`. The vision bridge files its throwaway sessions under
 *    `…\AppData\Local\Temp\modlens-work-XXXX`, and that is all this catches:
 *    a segment comparison rather than a substring, so `…\projects\template-editor`
 *    (which merely contains "temp") is NOT a temporary workspace.
 *    `preview` is deliberately not consulted — it is the user's own message
 *    text, which merely *quotes* the temp path, so judging by it would be
 *    guessing at user content.
 *  · **memory distillation** — the title starts with the distillation prompt
 *    (`You are a memory distillation…`), which ruagent writes itself when it
 *    distils a session into memories. A title that only mentions distillation
 *    (an agent's own `… [distilled]` wording) does not match.
 *
 *  Both are pure and cheap, so a caller can filter with them on every render. */
export function isTempWorkspace(project: string | null | undefined): boolean {
  return String(project ?? "")
    .split(/[\\/]+/)
    .some((seg) => /^(temp|tmp)$/i.test(seg));
}

/** The prompt ruagent's distillation pipeline puts in the title it writes. */
const DISTILL_TITLE = "you are a memory distillation";

/** The second literal the platform writes for the same activity: the scratch
 *  workspace its distillation runs execute in. Matched as a normalised path
 *  suffix, so it survives a different home directory and either separator,
 *  while a user's own `C:\src\distill` is not it. */
const DISTILL_WORKSPACE = "/.ruagent/workspaces/distill";

export function isDistillationTitle(title: string | null | undefined): boolean {
  return String(title ?? "")
    .trim()
    .toLowerCase()
    .startsWith(DISTILL_TITLE);
}

export function isDistillWorkspace(project: string | null | undefined): boolean {
  return String(project ?? "")
    .trim()
    .replace(/\\/g, "/")
    .toLowerCase()
    .endsWith(DISTILL_WORKSPACE);
}

/** True for a session the platform generated for itself (see above). Both
 *  distillation literals are checked — they agree on today's data (29 rows,
 *  `source=dsh`), and taking either match means a change to one of them does
 *  not quietly let those sessions back into the default list. */
export function isSystemSession(s: {
  project?: string | null;
  title?: string | null;
}): boolean {
  return (
    isTempWorkspace(s.project) ||
    isDistillationTitle(s.title) ||
    isDistillWorkspace(s.project)
  );
}
