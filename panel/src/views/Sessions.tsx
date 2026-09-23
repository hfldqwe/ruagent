// Session history: every agent CLI's conversations, auto-synced by the
// daemon (claude-code / dsh / ruagent). Index + on-demand viewer.
//
// This is the densest page in the panel: ~200 rows, ~8700px of scroll.
// Two consequences are designed for here, not discovered later:
//   1. the source dot is a CLASSIFICATION mark, so it may only wear the
//      brand domain or the neutral grade — never --signal (MASTER §3.2 V1,
//      §3.1 I hard rule 4);
//   2. 200 rows × 13 nodes is ~2600 nodes and 730 inline styles, so the
//      list mounts a window slice instead of every row (S4/S10).

//
// The two view components live in ./SessionsView so this module — which
// App.tsx imports STATICALLY for the route component — stays tiny.
// Row 27: that keeps the route's code in its own chunk instead of dragging
// it into the entry. The shim cannot be dropped even though the pure
// constants now live in ../session-source: App's static import means
// anything this module imports statically lands in the entry chunk.

import { lazy } from "react";

// Single source of truth lives in ../session-source (CommandPalette, in the
// entry chunk, imports it directly — row 27); re-exported here so Home and
// SessionsView keep the same import path.
export { SOURCE_LABEL } from "../session-source";

/** 会话来源点取色。终稿：只用域②品牌色与中性档（MASTER §3.1 I 域③→②，零新增 hex）。
 *
 *  Hard rules this encodes (view-sessions.md §4.1):
 *   - `--signal` / `--status-*` never colour a *category* (§3.1 I rule 4);
 *   - `--graph-*` / `--ws-*` never leave their view (§3.1 I rules 1–2);
 *   - a source with no brand mark is neutral, not "another hue".
 *  The signature stays `(s: string) => string` — Home.tsx and SessionDetail
 *  both import it, and a narrower type would silently fall to `default`. */
export function sourceHue(s: string): string {
  switch (s) {
    case "claude-code":
      return "var(--brand-claude)";
    case "opencode":
      return "var(--brand-opencode)";
    case "deepseek":
      return "var(--brand-deepseek)";
    default:
      return "var(--ant-color-text-tertiary)";
  }
}

/** Row height is constant by contract (S11): every child is single-line and
 *  the tallest one (the 24px distill button) sets it. The windowed list needs
 *  the number to size its spacers. */

/** Route component; the body arrives on demand. */
export const Sessions = lazy(() =>
  import("./SessionsView").then((m) => ({ default: m.Sessions })),
);

/** The single-session viewer, kept lazy too (Sessions mounts it on click). */
export const SessionDetail = lazy(() =>
  import("./SessionsView").then((m) => ({ default: m.SessionDetail })),
);

export function msToIso(ms: number): string {
  return ms > 0 ? new Date(ms).toISOString() : "";
}
