// The Sessions route body, split out of Sessions.tsx so that the module
// CommandPalette/Home/Chat import (SOURCE_LABEL, sourceHue, msToIso — a
// few pure helpers) stays in the entry chunk while this one is a route
// chunk, fetched on first visit (MASTER §12 row 27).
//
// The page's own notes live at the top of Sessions.tsx.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Alert, Button, Dropdown, Input, Popconfirm, Segmented, Select, Tooltip } from "antd";
import {
  api,
  DistillRefusedError,
  type DistillBatchOutcome,
  type DistillRefusedPayload,
  type DistillScope,
  type DistillTier,
  type SessionRecord,
} from "../api";
import { dateOf, useI18n } from "../i18n";
import { Icon } from "../icons";
import {
  Empty,
  ErrorState,
  IconButton,
  Modal,
  ReadoutStrip,
  RelTime,
  Spinner,
  fmtTokens,
  fmtUsd,
  useToast,
} from "../ui";
import { Markdown } from "./lazy-markdown";
import { SOURCE_LABEL, sourceHue, msToIso } from "./Sessions";
// Contract S7 — read straight from the pure module rather than through the
// `./Sessions` re-export: the re-export exists so the ENTRY chunk
// (CommandPalette) does not drag a route module in, and this view is already
// that route chunk, so importing the source of truth directly costs nothing.
import {
  isPlatformInjectedName,
  isSystemSession,
  isTempWorkspace,
} from "../session-source";

const ROW_H = 40;
/** Rows rendered beyond the viewport, so a fast scroll never shows a gap.
 *  Kept small on purpose: each mounted row carries one inline style (the
 *  source dot's colour), and row 12 caps inline styles at 50 per route. */
const OVERSCAN = 2;
/** The viewer is capped like the chat log: newest 400 messages. */
const MSG_CAP = 400;
/** The batch entry's page size. The daemon clamps to [1, 200] and defaults to
 *  20, so this asks for exactly one daemon-default page of work instead of
 *  inventing a second default that could drift from the route's. */
const DISTILL_BATCH_LIMIT = 20;

/** The tier this page's distillation actions START on (issue #57).
 *
 *  DEFAULT `rules`, and the reason is the user's money. The batch entry says
 *  "批量蒸馏未处理 N 条" and one click runs the pass over up to
 *  DISTILL_BATCH_LIMIT sessions; under the ROUTE's own default (`acp`, because
 *  the panel used to post an empty body) that click buys N sessions times their
 *  window counts in model calls, with nothing on screen saying so. `rules` is
 *  the zero-token tier: the default action costs nothing and cannot surprise
 *  anyone, and the paid tier is one deliberate click away with its price shown
 *  before the run (the estimate's window count IS the call count).
 *
 *  PAGE state rather than modal state, because ONE choice has to govern three
 *  requests that must not disagree: the planner (asked on page load and on every
 *  change), the batch run, and the per-row distill. */
const DEFAULT_DISTILL_TIER: DistillTier = "rules";

/** The name a session wears on this page (S9.4).
 *
 *  When the stored name is one of OUR injection headers, the harness took the
 *  first message it was handed — our memory-context block — as the title, so
 *  the name is our prompt and not something the user wrote. It is replaced by
 *  an honest placeholder that says what the row is and when it happened; the
 *  name never pretends to be the user's words, and the session's content is
 *  untouched (it still opens).
 *
 *  The date is the session's own start day (`dateOf` — the same absolute-date
 *  helper Graph/Memory/Knowledge use), not a relative time: a name has to stay
 *  stable in the list, and §9.5 L6 asks for a stable identity. */
/** t199 — a compact local stamp (date + time) for rows that have no name of
 *  their own. The day alone is not enough: measured on the live index, the
 *  title-less rows cover 11 distinct days but 46 rows, so a day-granularity
 *  suffix would leave ten rows showing the same string. The stamp is built
 *  from the session's OWN started_at, so it never moves. */
function stampOf(ms: number): string {
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, "0");
  return dateOf(msToIso(ms)) + " " + p(d.getHours()) + ":" + p(d.getMinutes());
}

function sessionName(
  s: { title: string | null; started_at: number; preview?: string | null },
  t: (key: string, params?: Record<string, string | number>) => string,
): string {
  if (isPlatformInjectedName(s.title)) {
    return t("sessions.systemName", { d: dateOf(msToIso(s.started_at)) });
  }
  // A real title is returned verbatim — not trimmed, not decorated: the
  // rows that already have a name must not change by one character.
  const title = s.title ?? "";
  if (title.trim()) return title;
  // t199 / S9.4: no usable title. Derive from the preview when there is one;
  // otherwise the row still has to be distinguishable from its neighbours,
  // so it carries the session's own start stamp. Measured: every title-less
  // row on the live index has an empty preview, so the stamp is what they
  // all show today — 46 rows that used to render one identical string.
  const preview = (s.preview ?? "").trim();
  if (preview) return preview.slice(0, 80);
  return t("sessions.untitled") + " · " + stampOf(s.started_at);
}

/** S7 — which platform-generated sessions the list shows. */
type SystemMode = "exclude" | "include" | "only";

type ListState =
  | { kind: "loading" }
  | { kind: "ready"; rows: SessionRecord[]; at: number }
  | { kind: "error"; err: unknown; rows: SessionRecord[] | null; at: number | null };

/* MASTER §12 row 58 — the part of this page's state that changes WHAT YOU ARE
 * LOOKING AT (source tab, S7 system tri-state, workspace, time window) lives in
 * the URL, so the URL replays the view. Row 58's letter asks for the HASH to
 * change, and since App.parseHash() learned the route[?query] form the state
 * rides there: "#sessions?src=dsh&sys=only". Only non-default values are
 * written, so the default view's URL is still exactly "#sessions". Two other
 * rules:
 *   · A tab switch must not become a back-button step, so every write goes
 *     through replaceState, never pushState.
 *   · The archived mode is NOT encoded: it is not one of the list's switchable
 *     states (see the note at its Select).
 * The search box is deliberately not here either: it is an uncommitted input,
 * which row 58 excludes by name. */
const SYS_MODES: SystemMode[] = ["exclude", "include", "only"];

/** This route's query, read straight off the hash. The shell publishes the same
 *  string on <html> (dataset.routeQuery), but this runs during the FIRST RENDER
 *  — before any effect, and React runs child effects before the parent's — so
 *  the dataset is not written yet, and being written from route state it also
 *  cannot follow a replaceState. The hash is the source of truth and needs no
 *  timing assumptions. */
/** t179 — how the list is ordered. "updated" is what the daemon already does
 *  (ORDER BY updated_at DESC), so it is the default and the URL stays clean;
 *  "created" is the user-asked-for alternative. */
type SortKey = "updated" | "created";

function routeQuery(): URLSearchParams {
  const h = window.location.hash.replace(/^#/, "");
  const i = h.indexOf("?");
  return new URLSearchParams(i >= 0 ? h.slice(i + 1) : "");
}

/** #47's distillation filter, as THIS page carries it: the daemon's two
 *  scopes (`?only=`) plus the panel's own "no scope" default. The value is
 *  sent straight to the daemon — the standing is decided there, against the
 *  ledger, because this page holds one window of the index and cannot answer
 *  "was this ever distilled" from what it happens to be showing. */
type DistillFilter = "all" | DistillScope;
const DISTILL_FILTERS: DistillFilter[] = ["all", "undistilled", "distilled"];

/** The view state as the URL carries it. An unknown enum value falls back to
 *  the default instead of reaching a Select, which would render empty. */
function readViewState(): {
  filter: string;
  ws: string;
  when: string;
  sort: SortKey;
  systemMode: SystemMode;
  distill: DistillFilter;
} {
  const p = routeQuery();
  const sys = p.get("sys");
  const dis = p.get("distill");
  return {
    filter: p.get("src") ?? "all",
    ws: p.get("ws") ?? "all",
    when: p.get("when") ?? "all",
    sort: p.get("sort") === "created" ? "created" : "updated",
    systemMode: SYS_MODES.includes(sys as SystemMode)
      ? (sys as SystemMode)
      : "exclude",
    distill: DISTILL_FILTERS.includes(dis as DistillFilter)
      ? (dis as DistillFilter)
      : "all",
  };
}

export function Sessions() {
  const { t } = useI18n();
  const [state, setState] = useState<ListState>({ kind: "loading" });
  // Row 58: the first render takes its view state from the URL.
  const [fromUrl] = useState(readViewState);
  const [filter, setFilter] = useState<string>(fromUrl.filter); // source
  const [ws, setWs] = useState<string>(fromUrl.ws); // workspace (= project)
  const [when, setWhen] = useState<string>(fromUrl.when); // time window
  const [q, setQ] = useState(""); // keyword
  /* t170: archived sessions have their own place and never appear in this list,
     so the list always asks the daemon for the non-archived set. The count is
     what the 归档 entry shows. */
  const [archivedCount, setArchivedCount] = useState(0);
  /** The separate archive destination. It does its own read-only fetch, so
   *  opening it never changes what the list is showing. */
  const [archiveOpen, setArchiveOpen] = useState(false);
  const [archiveRows, setArchiveRows] = useState<SessionRecord[] | null>(null);
  /** S7 — which platform-generated sessions the list shows. The counterpart of
   *  archivedMode, and local (not a daemon query parameter) because the page
   *  already holds every row: the origin rules are pure predicates over the
   *  fields. Row 58 puts it in the URL all the same: it changes what you see. */
  const [systemMode, setSystemMode] = useState<SystemMode>(fromUrl.systemMode);
  /** t179 — the order the user picked (row 58: it lives in the URL). */
  const [sort, setSort] = useState<SortKey>(fromUrl.sort);
  /** #47 — which distillation standing to show. It rides the URL with the rest
   *  of the view state (row 58), and it is a SERVER filter: see DistillFilter. */
  const [distillScope, setDistillScope] = useState<DistillFilter>(fromUrl.distill);
  /** The batch entry's own question, asked of the daemon's planner: how many
   *  sessions are undistilled, and what a pass would cost. `null` until the
   *  first answer — and it stays null on a failed read, so the button says
   *  "batch distillation…" instead of inventing a backlog of 0. */
  const [plan, setPlan] = useState<DistillBatchOutcome | null>(null);
  const [planErr, setPlanErr] = useState<unknown>(null);
  const [batchOpen, setBatchOpen] = useState(false);
  /** issue #57: the extractor tier this page's distillation actions use. See
   *  DEFAULT_DISTILL_TIER for why it starts on the free one. */
  const [tier, setTier] = useState<DistillTier>(DEFAULT_DISTILL_TIER);
  /** #62: the last distillation this page was REFUSED, kept as its own fact
   *  until dismissed. A governed refusal (409 + refused[]) is not a failed
   *  request and not an empty session, and a two-second toast plus
   *  "409 Conflict: {json}" would say neither which pairs were refused nor
   *  that NOTHING was written — so it gets a surface of its own. */
  const [refusal, setRefusal] = useState<{ sessionKey: string; payload: DistillRefusedPayload } | null>(null);
  /** Key of the row whose archive/delete request is in flight. */
  const [busy, setBusy] = useState<string | null>(null);
  const [open, setOpen] = useState<SessionRecord | null>(null);
  const [distilling, setDistilling] = useState<string | null>(null);
  // At <=520 a row has 318px of content box (measured) and the shared layer
  // hides the count and the time outright (index.css:616) while capping every
  // tag at 30% (617). The row then spends its width on chrome and the title
  // is left with 4px. The narrow composition below is the captain's ruling for
  // this width: keep the title and the time, drop the path tag, and let the
  // count ride along with the time.
  const narrow = useNarrowRow();
  // A skeleton appears only once the request is demonstrably slow, so a
  // normal load never flashes grey bars.
  const [slow, setSlow] = useState(false);
  const [, forceTick] = useState(0);
  const toast = useToast();
  const trigger = useRef<HTMLElement | null>(null);

  /* Row 58, the other direction: keep the URL in step with the state. The
     comparison keeps this idempotent — the first render of an already-correct
     URL (including a pasted one) writes nothing, so no needless history churn. */
  useEffect(() => {
    const loc = window.location;
    const h = loc.hash.replace(/^#/, "");
    // Never rewrite another route's hash: while this page is leaving, the hash
    // already points elsewhere and a write here would drag it back (that is the
    // exact shape of the bug the removed cleanup caused).
    if (h !== "sessions" && !h.startsWith("sessions?")) return;
    const p = new URLSearchParams();
    if (filter !== "all") p.set("src", filter);
    if (ws !== "all") p.set("ws", ws);
    if (when !== "all") p.set("when", when);
    if (systemMode !== "exclude") p.set("sys", systemMode);
    if (sort !== "updated") p.set("sort", sort);
    if (distillScope !== "all") p.set("distill", distillScope);
    const qs = p.toString();
    const next = loc.pathname + loc.search + "#sessions" + (qs ? "?" + qs : "");
    if (loc.pathname + loc.search + loc.hash !== next) {
      window.history.replaceState(window.history.state, "", next);
    }
  }, [filter, ws, when, systemMode, sort, distillScope]);

  /* The reverse direction while this page stays mounted: a hash edited in the
     address bar, or a back/forward to another "#sessions?…", changes the route
     query without unmounting the view, because the route KIND is unchanged.
     Writes above use replaceState, which fires no hashchange, so this cannot
     loop with the effect above. The route guard keeps a navigation away from
     being read as a state change. */
  useEffect(() => {
    const apply = () => {
      const h = window.location.hash.replace(/^#/, "");
      if (h !== "sessions" && !h.startsWith("sessions?")) return;
      const v = readViewState();
      setFilter(v.filter);
      setWs(v.ws);
      setWhen(v.when);
      setSystemMode(v.systemMode);
      setSort(v.sort);
      setDistillScope(v.distill);
    };
    window.addEventListener("hashchange", apply);
    return () => window.removeEventListener("hashchange", apply);
  }, []);

  /* There is deliberately NO unmount cleanup. One was written while the state
     lived in the real query and it was measured to break the back button: a
     back traversal then fired popstate WITHOUT hashchange, so the URL changed
     while the board stayed rendered. With the state in the hash it is not
     needed at all — the sider's items are fragment-only anchors (row 57), so
     leaving this page replaces the whole fragment and the state goes with it,
     leaving no query behind. */

  /** The batch planner's answer, re-asked whenever the backlog could have
   *  changed (a single distill, a batch run, a filter change). `dry_run` on the
   *  daemon is a PLAN: no extraction, no model call, no write — which is what
   *  makes it safe to ask from a page load, and it is the only source of the
   *  global backlog count (the list holds a window, and its rows carry a
   *  standing, not a total).
   *
   *  IT IS ASKED FOR A TIER (issue #57). The estimate's `windows` is the PAID
   *  tier's model-call count, and the tokens/cost come from the ledger's means
   *  over historical (ACP) attempts, so an answer priced for one tier is the
   *  wrong price for the other — the "estimated ACP, ran rules" mismatch this
   *  issue is about. */
  const refreshPlan = useCallback((which: DistillTier) => {
    api
      .distillBatch({ extractor: which, dryRun: true, limit: DISTILL_BATCH_LIMIT })
      .then((p) => {
        setPlan(p);
        setPlanErr(null);
      })
      .catch((e) => setPlanErr(e));
  }, []);

  const load = useCallback(async () => {
    try {
      const r = await api.sessionsList({
        archived: "exclude",
        // `all` means "no scope": sending the word would be a second spelling
        // of the default the daemon already has.
        only: distillScope === "all" ? undefined : distillScope,
      });
      setState({ kind: "ready", rows: r.sessions, at: Date.now() });
      // `?? 0` is not defensive noise: an older daemon without the field
      // answered undefined, and the label rendered the literal string
      // "undefined". The formatter guard below is the systemic half.
      setArchivedCount(r.archived_count ?? 0);
    } catch (e) {
      // A failed poll must not read as "no sessions": keep the last good
      // rows and say they may be stale (MASTER §12 row 20 / P6).
      setState((prev) =>
        prev.kind === "ready"
          ? { kind: "error", err: e, rows: prev.rows, at: prev.at }
          : {
              kind: "error",
              err: e,
              rows: prev.kind === "error" ? prev.rows : null,
              at: prev.kind === "error" ? prev.at : null,
            },
      );
    }
    // `distillScope` is the request's own parameter, so a filter change must rebuild
    // this callback — otherwise the poll would keep answering the previous scope.
  }, [distillScope]);

  useEffect(() => {
    const timer = setTimeout(() => setSlow(true), 200);
    void load();
    const i = setInterval(() => void load(), 10000);
    return () => {
      clearTimeout(timer);
      clearInterval(i);
    };
  }, [load]);

  // One plan read on mount, plus one on every TIER change (issue #57); the
  // mutation paths below re-ask it. Deliberately NOT polled with the list: it is
  // a costing question, and the answer only moves when something distils — which
  // this page already reacts to. The tier is a dependency because the same
  // sessions cost a different number of MODEL CALLS per tier, so a kept estimate
  // would be a price tag on a pass the user is not about to run.
  useEffect(() => {
    refreshPlan(tier);
  }, [refreshPlan, tier]);

  // "data may be stale · N seconds ago" has to actually tick.
  useEffect(() => {
    if (state.kind !== "error") return;
    const i = setInterval(() => forceTick((n) => n + 1), 1000);
    return () => clearInterval(i);
  }, [state.kind]);

  const rows = state.kind === "loading" ? null : state.rows;
  const failed = state.kind === "error";
  const staleSec = useMemo(
    () =>
      state.kind === "error" && state.at ? Math.round((Date.now() - state.at) / 1000) : 0,
    [state],
  );

  const sources = useMemo(() => [...new Set((rows ?? []).map((s) => s.source))], [rows]);
  /** How many rows in this window the platform generated for itself (S7). */
  const systemCount = useMemo(
    () => (rows ?? []).filter(isSystemSession).length,
    [rows],
  );
  /** Whether any row of this page answers the distillation STANDING at all.
   *  `distilled` is additive on the wire, so a daemon older than it omits the
   *  field, and `undefined` is then the honest reading of "this build cannot
   *  tell you". The distill filter is disabled in that case rather than
   *  silently showing every row under a scope nothing can apply. The test is
   *  over the rows themselves: a page of rows that all lack the field, and a
   *  page with no rows at all, are different situations. */
  const hasStanding = (rows ?? []).some((s) => s.distilled !== undefined);
  /** Workspaces present in the current page of results, most used first.
   *  Capped: this is a filter, not an inventory.
   *  Temporary workspaces are left out (S7): the vision bridge alone files 46
   *  throwaway `modlens-work-XXXX` workspaces here, which is half the menu and
   *  no way to find anything. The rows are still reachable through the system
   *  control below — this only keeps the MENU honest. */
  const workspaces = useMemo(() => {
    const counts = new Map<string, number>();
    for (const s of rows ?? []) {
      if (s.project && !isTempWorkspace(s.project)) {
        counts.set(s.project, (counts.get(s.project) ?? 0) + 1);
      }
    }
    return [...counts.entries()]
      .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
      .slice(0, 20)
      .map(([p]) => p);
  }, [rows]);

  /** 200 rows, so every leg of the filter runs on the client: typing in the
   *  search box must not wait for a round trip. */
  const filtered = useMemo(() => {
    if (rows === null) return [];
    const needle = q.trim().toLowerCase();
    const days = when === "7d" ? 7 : when === "30d" ? 30 : 0;
    const since = days ? Date.now() - days * 86_400_000 : 0;
    const out = rows.filter((s) => {
      // S7: the platform's own sessions stay out of the default list, and the
      // system control is the explicit way back to them.
      const system = isSystemSession(s);
      if (systemMode === "exclude" && system) return false;
      if (systemMode === "only" && !system) return false;
      if (filter !== "all" && s.source !== filter) return false;
      if (ws !== "all" && s.project !== ws) return false;
      if (since && s.updated_at < since) return false;
      if (needle) {
        const hay = `${s.title ?? ""} ${s.project ?? ""} ${s.preview ?? ""} ${
          s.agent ?? ""
        } ${SOURCE_LABEL[s.source] ?? s.source}`.toLowerCase();
        if (!hay.includes(needle)) return false;
      }
      return true;
    });
    /* t179 — the order is the user's choice. "updated" is deliberately NOT
       re-sorted: that is exactly what the daemon already returns
       (ORDER BY updated_at DESC), so the default path is bit-identical.
       "created" re-orders the same set by started_at. Caveat, measured: the
       SET is the daemon's window (the most recently ACTIVE sessions), so
       "created" orders that window — a true whole-index created-order needs a
       daemon-side sort parameter (out of this task's scope). */
    if (sort === "created") {
      out.sort((a, b) => b.started_at - a.started_at || b.updated_at - a.updated_at);
    }
    return out;
  }, [rows, filter, ws, when, q, systemMode, sort]);

  const anyFilter = filter !== "all" || ws !== "all" || when !== "all" || q.trim() !== "";
  const clearFilters = () => {
    setFilter("all");
    setWs("all");
    setWhen("all");
    setQ("");
  };

  /** Archive = hide from this list. It writes one row in the daemon's own
   *  table and touches no file, so it is offered for every source. */
  const setArchived = async (s: SessionRecord, on: boolean) => {
    setBusy(s.key);
    try {
      if (on) await api.sessionArchive(s.key);
      else await api.sessionUnarchive(s.key);
      toast("ok", t(on ? "sessions.archivedToast" : "sessions.unarchivedToast"));
      await load();
    } catch (e) {
      toast("err", String(e));
    } finally {
      setBusy(null);
    }
  };

  /** Delete is offered only for sessions ruagent produced itself — the row
   *  renders no control for anything else (see `deletable` from the API). */
  const remove = async (s: SessionRecord) => {
    setBusy(s.key);
    try {
      await api.sessionDelete(s.key);
      toast("ok", t("sessions.deletedToast"));
      await load();
    } catch (e) {
      toast("err", String(e));
    } finally {
      setBusy(null);
    }
  };

  /* t170 — the archive destination reads the archived set itself. Read-only:
     it never changes what the list is showing. */
  const loadArchived = useCallback(async () => {
    try {
      const r = await api.sessionsList({ archived: "only" });
      setArchiveRows(r.sessions);
      setArchivedCount(r.archived_count ?? 0);
    } catch {
      // An empty destination is the honest reading of a failed fetch here; the
      // list itself keeps its own error state.
      setArchiveRows([]);
    }
  }, []);
  useEffect(() => {
    if (archiveOpen) void loadArchived();
  }, [archiveOpen, loadArchived]);

  const openRow = (s: SessionRecord, el: HTMLElement | null) => {
    trigger.current = el;
    setOpen(s);
  };
  /** Close returns focus to the row that opened the viewer (row 22). */
  const closeRow = useCallback(() => {
    setOpen(null);
    const el = trigger.current;
    trigger.current = null;
    // After React removes the overlay; the row itself is still mounted.
    requestAnimationFrame(() => el?.focus());
  }, []);

  const distill = async (s: SessionRecord) => {
    setDistilling(s.key);
    // A new attempt replaces the previous refusal: the card is about THIS page's
    // latest distillation, not a log.
    setRefusal(null);
    try {
      // issue #57: the row action runs the SAME tier as the batch control above
      // — one page-level choice, so a single distill cannot quietly buy an agent
      // turn while the page reads "rules（免费）".
      const r = await api.sessionDistill(s.key, tier);
      const d = r.distilled;
      toast(
        "ok",
        // The toast IS this action's receipt, so it names the tier and whether
        // that tier calls the model (issue #57) — without it, "记忆 3 写入" reads
        // the same whether it cost 0 tokens or three agent turns.
        t("sessions.distilled", {
          m: d.memories_written,
          k: d.memories_skipped,
          e: d.entities_written,
          r: d.relations_written,
          a: d.agent,
        }) +
          " · " +
          (tier === "rules" ? t("sessions.tier.note.rules") : t("sessions.tier.note.acp")),
      );
      // This session's standing just changed, and the backlog shrank: re-read
      // both rather than leaving a badge (or a count) describing the previous
      // state. `void` because the toast above is already the user's answer.
      void load();
      // Same tier it was priced with: the row action ran THAT one.
      refreshPlan(tier);
    } catch (e) {
      // #62: a governed refusal is a FACT about the write vocabulary, not a
      // failed request. It gets the card (the pairs, what each store accepts,
      // and "nothing was written") plus a short toast, because the click that
      // caused it was on a row.
      if (e instanceof DistillRefusedError) {
        setRefusal({ sessionKey: s.key, payload: e.payload });
        toast("err", t("sessions.refused.toast", { n: e.payload.refused.length }));
        // The attempt DID write a ledger row (status ok, 0 written) and left the
        // backlog, so both readers are re-asked rather than left describing the
        // state before the click.
        void load();
        refreshPlan(tier);
      } else {
        toast("err", String(e));
      }
    } finally {
      setDistilling(null);
    }
  };

  return (
    <div>
      <h1 className="sr-only micro">{t("sessions.title")}</h1>
      <div className="view-bar">
        <h2>{t("sessions.title")}</h2>
        <span className="muted">
          {rows === null ? "" : t("sessions.subtitle", { n: rows.length })}
        </span>
        <span className="grow" />
        {sources.length > 0 && (
          <Segmented
            value={filter}
            onChange={(v) => setFilter(v as string)}
            options={[
              { value: "all", label: t("sessions.all") },
              ...sources.map((s) => ({ value: s, label: SOURCE_LABEL[s] ?? s })),
            ]}
          />
        )}
      </div>

      {/* 200 rows, so filtering is instant and local; only the archived
          dimension is a query parameter, because the hide marker lives in
          the daemon (session_archives). */}
      <div className="filter-bar">
        <Input
          allowClear
          value={q}
          onChange={(e) => setQ(e.target.value)}
          placeholder={t("sessions.search")}
          aria-label={t("sessions.search")}
          prefix={<Icon name="search" size={14} />}
          style={{ maxWidth: 280 }}
        />
        <Select
          value={ws}
          onChange={setWs}
          aria-label={t("sessions.filter.workspace")}
          style={{ minWidth: 170 }}
          options={[
            { value: "all", label: t("sessions.filter.workspaceAll") },
            ...workspaces.map((p) => ({ value: p, label: shortProject(p) })),
          ]}
        />
        <Select
          value={when}
          onChange={setWhen}
          aria-label={t("sessions.filter.time")}
          style={{ minWidth: 130 }}
          options={[
            { value: "all", label: t("sessions.filter.timeAll") },
            { value: "7d", label: t("sessions.filter.time7d") },
            { value: "30d", label: t("sessions.filter.time30d") },
          ]}
        />
        {/* #47: the distillation axis. It is a SERVER filter (`?only=`), because
            the standing is the ledger's — a client-side filter over this page
            would answer "undistilled among the rows I happen to hold", which is
            a different and wrong question. Disabled when no row answers the
            field at all: that is a daemon older than the field, and offering a
            filter it cannot honour would silently show everything. */}
        <Tooltip
          title={hasStanding ? undefined : t("sessions.distill.unsupported")}
        >
          <Select
            className="sessions-distill"
            value={distillScope}
            onChange={(v) => setDistillScope(v as DistillFilter)}
            disabled={!hasStanding}
            aria-label={t("sessions.filter.distill")}
            options={[
              { value: "all", label: t("sessions.filter.distillAll") },
              { value: "undistilled", label: t("sessions.filter.distillNone") },
              { value: "distilled", label: t("sessions.filter.distillOk") },
            ]}
          />
        </Tooltip>
        <span className="grow" />
        {/* t170 — the user's ruling: archived sessions are stored SEPARATELY
            and must not be shown back in this list. So there is no "show
            archived" control any more; this is an ENTRY to their own place, and
            the list never contains them. (The three-state Select that used to
            sit here is gone. Its i18n keys stay: this file may only add keys,
            and removing them is a separate call.) The count is in the label so
            the entry still says how much is over there. */}
        <Button
          className="sessions-archive-entry"
          onClick={() => setArchiveOpen(true)}
          aria-label={t("sessions.archivedEntry", { n: archivedCount })}
        >
          <Icon name="archive" size={14} />
          <span className="muted">
            {t("sessions.archivedEntry", { n: archivedCount })}
          </span>
        </Button>
        {/* S7: the sessions the platform generated for itself (throwaway
            vision-bridge workspaces, memory-distillation runs) are out of the
            default list, and this is the explicit way back to them — the same
            three-state shape as the archived control next to it. Deliberately
            NOT folded into the source tabs: that axis is the `source` field,
            and these sessions' source is dsh/claude-code like everyone else's,
            so a tab there would make the filter lie about its own axis.
            No inline style: 行 12 bills them per route and the Selects above
            already spend one each. */}
        <Select
          className="sessions-system"
          value={systemMode}
          onChange={(v) => setSystemMode(v as SystemMode)}
          aria-label={t("sessions.system")}
          options={[
            { value: "exclude", label: t("sessions.hideSystem") },
            { value: "include", label: t("sessions.showSystem", { n: systemCount }) },
            { value: "only", label: t("sessions.onlySystem") },
          ]}
        />
        {/* t179 — the user asked for a selectable order. 最近对话 is the
            daemon's own ORDER BY updated_at DESC, so it is the default and the
            URL stays clean; 最近创建 orders by started_at. Row 58: it lives in
            the URL with the rest of the view state. No inline style (row 12
            bills them per route). */}
        <Select
          className="sessions-sort"
          value={sort}
          onChange={(v) => setSort(v as SortKey)}
          aria-label={t("sessions.sort")}
          options={[
            { value: "updated", label: t("sessions.sortUpdated") },
            { value: "created", label: t("sessions.sortCreated") },
          ]}
        />
        {/* issue #57: the tier stands BESIDE the entry, not only inside the
            modal, because the planner runs on page load — before any modal
            exists — and because the same choice governs the row action below.
            A native `title` carries the explanation: a Tooltip would wrap a
            controlled control for no gain, and the labels already say which tier
            costs money. */}
        <span className="sessions-tier" title={t("sessions.tier.hint")}>
          <Segmented
            value={tier}
            onChange={(v) => setTier(v as DistillTier)}
            aria-label={t("sessions.tier.label")}
            options={[
              { value: "rules", label: t("sessions.tier.rules") },
              { value: "acp", label: t("sessions.tier.acp") },
            ]}
          />
        </span>
        {/* #47: the backlog pass. The count on the label is the daemon's own
            `undistilled_total` (see refreshPlan), so the button states the size
            of the job BEFORE it is opened — and says nothing rather than 0 when
            the planner could not be read. */}
        <Button
          type="primary"
          className="sessions-batch"
          onClick={() => setBatchOpen(true)}
          aria-label={t("distill.batch.title")}
        >
          {plan ? t("distill.batch.open", { n: plan.undistilled_total }) : t("distill.batch.openPlain")}
        </Button>
      </div>

      {/* #62: A REFUSED PASS KEEPS ITS OWN SURFACE. Not the page's error
          state (nothing failed to load), not the empty state (this is the
          opposite reading: the pass ran and proposed), and not a transient
          toast. It states what was refused, what each store DOES accept, and
          that nothing was written. */}
      {refusal ? (
        <Alert
          className="distill-refused"
          type="warning"
          showIcon
          data-refused-pairs={refusal.payload.refused.length}
          data-refused-written={typeof refusal.payload.memories_written === "number" ? refusal.payload.memories_written : "not_recorded"}
          message={
            <span>
              {t("sessions.refused.title")}
              {" · "}
              <span className="mono micro">{refusal.sessionKey}</span>
            </span>
          }
          description={
            <div>
              <p className="micro flush">{t("sessions.refused.hint")}</p>
              <p className="micro" data-refused-error="">
                {refusal.payload.error}
              </p>
              <p className="micro flush">{t("sessions.refused.pairs")}</p>
              {refusal.payload.refused.map((r) => (
                <div
                  className="row tight"
                  key={r.store + ":" + r.namespace}
                  data-refused-pair={r.store + " × " + r.namespace}
                >
                  <span className="mono micro">
                    {r.store} × {r.namespace}
                  </span>
                  <span className="grow" />
                  <span
                    className="muted micro"
                    data-allowed={r.allowed_namespaces.join(",")}
                  >
                    {t("sessions.refused.allowed", {
                      list: r.allowed_namespaces.length
                        ? r.allowed_namespaces.join(" / ")
                        : t("sessions.refused.allowedEmpty"),
                    })}
                  </span>
                </div>
              ))}
              <p className="micro">
                {t("sessions.refused.measure", {
                  r: typeof refusal.payload.memories_refused === "number" ? refusal.payload.memories_refused : t("distill.notRecorded"),
                  w: typeof refusal.payload.memories_written === "number" ? refusal.payload.memories_written : t("distill.notRecorded"),
                  s: typeof refusal.payload.memories_skipped === "number" ? refusal.payload.memories_skipped : t("distill.notRecorded"),
                })}
              </p>
              <div className="row end">
                <Button size="small" onClick={() => setRefusal(null)}>
                  {t("common.close")}
                </Button>
              </div>
            </div>
          }
        />
      ) : null}

      {failed && (
        <ErrorState
          title={t("sessions.err")}
          hint={
            state.kind === "error" && state.at
              ? `${t("common.stale")} · ${staleSec}s — ${t("sessions.stale.hint")}`
              : t("sessions.err.hint")
          }
          onRetry={() => void load()}
          retryLabel={t("common.retry")}
        />
      )}

      {rows === null ? (
        slow ? (
          <CardSkeleton />
        ) : (
          <Spinner label={`${t("sessions.title")}…`} />
        )
      ) : (
        <>
          <ReadoutStrip
            grid
            items={[
              { key: "sessions", label: t("sessions.title"), value: filtered.length },
              {
                key: "messages",
                label: t("metric.messages"),
                value: filtered.reduce((s, x) => s + x.message_count, 0),
              },
            ]}
          />

          {/* error and empty are mutually exclusive (primitives §3.5): a
              failed load is not "no sessions yet". */}
          {rows.length === 0 || filtered.length === 0 ? (
            failed ? null : (
              <Empty
                icon="chat"
                title={rows.length === 0 ? t("sessions.empty") : t("sessions.filteredEmpty")}
                hint={rows.length === 0 ? t("sessions.emptyHint") : t("sessions.filterEmpty")}
                action={
                  anyFilter && rows.length > 0 ? (
                    <Button size="small" onClick={clearFilters}>
                      {t("sessions.filterClear")}
                    </Button>
                  ) : undefined
                }
              />
            )
          ) : (
            <RowList
              rows={filtered}
              distilling={distilling}
              busy={busy}
              narrow={narrow}
              onOpen={openRow}
              onDistill={distill}
              onArchive={setArchived}
              onDelete={remove}
            />
          )}
        </>
      )}

      {/* t170 — the archive destination. Archived sessions are stored and
          listed HERE and nowhere else; the list above always asks the daemon
          for the non-archived set, so they can never be mixed back in. Undoing
          an archive uses the same daemon call the row button makes. */}
      {archiveOpen && (
        <Modal
          title={t("sessions.archivedTitle")}
          onClose={() => setArchiveOpen(false)}
        >
          <p className="muted micro">{t("sessions.archivedNote")}</p>
          {archiveRows === null ? (
            <Spinner label={t("common.loading")} />
          ) : archiveRows.length === 0 ? (
            <p className="muted">{t("sessions.archivedEmpty")}</p>
          ) : (
            <div className="card">
              {archiveRows.map((s) => (
                <div key={s.key} className="row tight">
                  <span className="title">{sessionName(s, t)}</span>
                  <span className="muted micro">
                    {SOURCE_LABEL[s.source] ?? s.source}
                    {s.project ? " · " + s.project : ""} · {s.message_count}{" "}
                    {t("sessions.messages")}
                  </span>
                  <span className="grow" />
                  <Button
                    size="small"
                    disabled={busy === s.key}
                    onClick={async () => {
                      await setArchived(s, false);
                      await loadArchived();
                    }}
                  >
                    {t("sessions.unarchive")}
                  </Button>
                </div>
              ))}
            </div>
          )}
        </Modal>
      )}

      {open && <SessionDetail session={open} onClose={closeRow} />}

      {batchOpen && (
        <BatchDistillModal
          plan={plan}
          planErr={planErr}
          tier={tier}
          onTierChange={setTier}
          // `refreshPlan` takes the tier it should price, so it cannot be passed
          // bare: a bare reference would hand it `undefined` as the tier.
          onReloadPlan={() => refreshPlan(tier)}
          onClose={() => setBatchOpen(false)}
          onDone={() => {
            // The pass changed standings and the backlog; both readers are
            // re-asked here rather than by a poll that may be 10s away.
            void load();
            refreshPlan(tier);
          }}
        />
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// The list itself. 200 rows is past the point where mounting all of them is
// free, so only the visible window is mounted and two spacers hold the scroll
// height (S10: nodes 2607 → ~600, inline styles 731 → <50, page height kept).
// ---------------------------------------------------------------------------

/** True at the one width where a session row runs out of room. Same
 *  matchMedia pattern as the shell's 992px collapse (App.tsx), so a resize
 *  adapts live instead of only on mount. */
function useNarrowRow(): boolean {
  const [narrow, setNarrow] = useState(
    () => window.matchMedia("(max-width: 520px)").matches,
  );
  useEffect(() => {
    const mq = window.matchMedia("(max-width: 520px)");
    const onChange = () => setNarrow(mq.matches);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);
  return narrow;
}

function RowList({
  rows,
  distilling,
  busy,
  narrow,
  onOpen,
  onDistill,
  onArchive,
  onDelete,
}: {
  rows: SessionRecord[];
  distilling: string | null;
  busy: string | null;
  narrow: boolean;
  onOpen: (s: SessionRecord, el: HTMLElement | null) => void;
  onDistill: (s: SessionRecord) => void;
  onArchive: (s: SessionRecord, on: boolean) => void;
  onDelete: (s: SessionRecord) => void;
}) {
  const { t } = useI18n();
  /* Row 18 (hit targets) wants >=32x32 and row 12 (inline styles) has only 9
     units of headroom, so the size must come from the shared modifier rather
     than an inline width. Measured cost/benefit: 24x24 -> 32x32 turns the
     content-area sub-32px count from 41 to 0 while the row stays 40px
     (.card > .row-btn:has(.icon-btn-lg) drops the row's block padding 8 -> 4).
     NOT applied at <=520: there the row has 230px of content box, and the
     8px-per-button growth would take a 3-action row's title from 41px to 17px
     — trading a hit-target win for a worse reading win. */
  const actionCls = narrow ? undefined : "icon-btn-lg";
  /** Key of the row whose narrow-screen "more" menu is open (S10). Tracked so
   *  the trigger can carry aria-expanded and so only one menu is open at once. */
  const [moreOpen, setMoreOpen] = useState<string | null>(null);
  /** The row a narrow-screen delete is waiting on. The wide row confirms with
   *  an inline Popconfirm; a menu item cannot host one (the menu closes on
   *  click), so the narrow path confirms in the page's own Modal instead —
   *  same copy, same two-step discipline, and it still touches no file. */
  const [confirmDelete, setConfirmDelete] = useState<SessionRecord | null>(null);
  /** The "more" trigger that opened the current menu. antd hands focus back to
   *  the trigger only when the trigger kept it; once the menu takes focus (see
   *  below) Esc drops focus to <body> instead, so the page has to return it —
   *  view-sessions S10 ④ and the house rule for overlays (row 22) both ask for
   *  the trigger. One menu is open at a time, so one ref is enough. */
  const lastTrigger = useRef<HTMLElement | null>(null);
  /** Which row's wide delete confirm is open (t130 rev=1). antd's Popconfirm
   *  ignores Escape, and MASTER:767 ("键盘：Escape 关闭浮层") does not exempt
   *  it: a keyboard user could open a destructive confirm and be stuck inside
   *  it. Scope is deliberately that one behaviour — Escape closes it and hands
   *  focus back to the button that opened it (row 22 family); nothing else
   *  about the Popconfirm changes. */
  const [confirmPop, setConfirmPop] = useState<string | null>(null);
  const popTrigger = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (!confirmPop) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      setConfirmPop(null);
      requestAnimationFrame(() => popTrigger.current?.focus());
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [confirmPop]);
  const card = useRef<HTMLDivElement>(null);
  const [range, setRange] = useState<[number, number]>(() => [0, Math.min(rows.length, 40)]);

  useEffect(() => {
    let raf = 0;
    const compute = () => {
      const el = card.current;
      if (!el) return;
      const top = el.getBoundingClientRect().top + window.scrollY;
      const y = window.scrollY;
      const first = Math.max(0, Math.floor((y - top) / ROW_H) - OVERSCAN);
      const last = Math.min(
        rows.length,
        Math.ceil((y + window.innerHeight - top) / ROW_H) + OVERSCAN,
      );
      setRange((prev) => (prev[0] === first && prev[1] === last ? prev : [first, last]));
    };
    const onScroll = () => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(compute);
    };
    compute();
    window.addEventListener("scroll", onScroll, { passive: true });
    window.addEventListener("resize", onScroll);
    return () => {
      cancelAnimationFrame(raf);
      window.removeEventListener("scroll", onScroll);
      window.removeEventListener("resize", onScroll);
    };
  }, [rows.length]);

  const [start, end] = range;
  const slice = rows.slice(start, end);

  return (
    <div className="card" ref={card}>
      {start > 0 && <div style={{ height: start * ROW_H }} aria-hidden="true" />}
      {slice.map((s) => (
        /* role=button, not a real <button>: the row carries actions inside
           it — nested buttons are invalid HTML and give screen readers two
           activation targets for one row. */
        <div
          key={s.key}
          className="row-btn"
          role="button"
          tabIndex={0}
          onClick={(e) => {
            // Two guards, both measured. (1) The narrow "more" menu lives in a
            // portal, so its clicks bubble through the REACT tree and land here
            // even though the item is not a DOM descendant of this row —
            // activating 归档 from the menu also opened the viewer. Only react
            // to clicks inside the row's own subtree. (2) An action button owns
            // its click; without this the row opened the viewer on every
            // archive/delete/distill press too.
            const el = e.target as HTMLElement;
            if (!e.currentTarget.contains(el)) return;
            if (el.closest(".icon-btn")) return;
            onOpen(s, e.currentTarget);
          }}
          onKeyDown={(e) => {
            // The same two guards the click has: a focused row control owns its
            // own Enter/Space, and a key pressed inside the portal menu is not
            // this row's. Measured without the first one — Enter on the narrow
            // "more" trigger bubbled here and opened the viewer instead of the
            // menu, so the menu was mouse-only. It was already true of the
            // distill and archive buttons.
            const el = e.target as HTMLElement;
            if (!e.currentTarget.contains(el)) return;
            if (el.closest(".icon-btn")) return;
            if (e.key === "Enter" || e.key === " ") {
              e.preventDefault();
              onOpen(s, e.currentTarget);
            }
          }}
        >
          {/* At <=520 a row is 254px wide with a 230px content box (measured)
              and the shared layer already hides the count and the time here
              (index.css:616) while capping every tag at 30% (617) — the tags
              then spend the row's width and leave the title 4px. The captain's
              ruling for this width is "keep the title and the time, drop the
              path tag"; the measurements below add the source and state tags
              to that list, because each one costs the title its full width:
              a 5-item row (source tag + title + meta + 2 actions) leaves the
              title 64px against the 70px it needs, and a 3-action row is over
              budget outright. So the narrow row is title + meta + actions. */}
          {!narrow ? (
            <span className="tag">
              <span className="dot" style={{ background: sourceHue(s.source) }} />{" "}
              {SOURCE_LABEL[s.source] ?? s.source}
            </span>
          ) : null}
          <span className="title">{sessionName(s, t)}</span>
          {/* #47: the row's distillation standing, from the daemon's own ledger
              reading. Shown in BOTH widths (it is the page's answer to "which of
              these still needs distilling"), but the SECOND chip — the newest
              attempt's outcome, which is only interesting when it is not ok — is
              dropped at <=520 where three chips leave the title its 5em floor and
              nothing else. `data-distilled` carries the primary state so a test
              reads the state, not the copy. */}
          {s.distilled !== undefined ? (
            <Tooltip title={standingHint(s, t)}>
              <span
                className={s.distilled ? "tag ok" : "tag"}
                data-distilled={s.distilled ? "true" : "false"}
              >
                {t(s.distilled ? "sessions.distill.state.ok" : "sessions.distill.state.none")}
              </span>
            </Tooltip>
          ) : null}
          {!narrow && lastAttemptChip(s) ? (
            <Tooltip title={standingHint(s, t)}>
              <span
                className={lastAttemptChip(s)!.cls}
                data-distill-status={s.distill_status ?? "not_recorded"}
              >
                {t(lastAttemptChip(s)!.labelKey)}
              </span>
            </Tooltip>
          ) : null}
          {!narrow && s.archived ? (
            <span className="tag">{t("sessions.archived")}</span>
          ) : null}
          {!narrow && s.agent ? <span className="tag">{s.agent}</span> : null}
          {narrow ? (
            /* The count and the time ride together as one meta cluster.
               .row.tight is the existing 8px-gap flex utility (padding 4px 0,
               so the 11/15 line stays inside the row's 24px content box and
               the row does not grow past its 40px pitch), and .micro/.muted are
               the size grade and the tertiary colour (primitives §2 — "size is
               a role, colour is a meaning"). Being flex items is also what
               makes clientWidth meaningful: an inline span measures 0. */
            <span className="row tight">
              <span className="muted micro">
                {s.message_count} {t("sessions.messages")}
              </span>
              <span className="muted micro">
                <RelTime iso={msToIso(s.updated_at)} />
              </span>
            </span>
          ) : (
            <>
              <span className="muted mono micro">
                {s.message_count} {t("sessions.messages")}
              </span>
              <span className="time">
                <RelTime iso={msToIso(s.updated_at)} />
              </span>
            </>
          )}
          {/* The path tag is the first thing to go when the row runs out of
              room: the page's job is to find and open a session, so the title
              identifies it and the time locates it, while the working
              directory is secondary (captain's ruling for <=520). */}
          {!narrow && s.project ? (
            <span className="tag">{shortProject(s.project)}</span>
          ) : null}
          {/* Row actions are ALWAYS visible, not hover-gated. The user
              reported "there is no delete or archive button" after looking at
              this page: `.row-action` is opacity 0 until hover (primitives
              §3.3 C10, view-sessions.md §4), which reads as "the feature does
              not exist" on a screenshot, on a touch device without
              `hover: none`, and to anyone who does not sweep the mouse over a
              row. Measured cost of dropping it: ZERO — opacity does not
              affect layout, and the icons already occupied their 24px in both
              states (title width 441px @1440 either way). */}
          <IconButton
            label={t("sessions.distillHint")}
            title={t("sessions.distillHint")}
            icon="zap"
            size={14}
            className={actionCls}
            disabled={distilling === s.key}
            onClick={() => void onDistill(s)}
          />
          {narrow ? (
            /* S10 (ii) — a narrow row carries exactly two controls: the primary
               action above, and this menu. At <=520 the shared layer raises
               every button to the 44px touch target (index.css, the t108
               block), and three of them wrapped the row to 114px, i.e. three
               sessions per screen. Both halves of that conflict are real
               requirements — the 44px target (95/97 elements were under it) and
               the list's scanability — so the secondary actions move one tap
               deeper instead of either one giving way. Each menu item still
               clears 44px (a two-line label), because shrinking them would
               just move the touch defect inside the menu. */
            <Dropdown
              trigger={["click"]}
              open={moreOpen === s.key}
              onOpenChange={(o) => {
                setMoreOpen(o ? s.key : null);
                if (!o) {
                  lastTrigger.current?.focus();
                  lastTrigger.current = null;
                  return;
                }
                lastTrigger.current = document.activeElement as HTMLElement | null;
                /* antd renders the menu with tabindex="0" but does not move
                   focus into it when the trigger is activated by Enter, so the
                   arrow keys went nowhere (measured: ArrowDown left the active
                   item on the first entry, i.e. the second action could not be
                   reached without a mouse). Only one menu is open at a time
                   (moreOpen), so this query is unambiguous; Esc hands focus
                   back to the trigger on its own. */
                requestAnimationFrame(() =>
                  requestAnimationFrame(() =>
                    document.querySelector<HTMLElement>(".ant-dropdown-menu")?.focus(),
                  ),
                );
              }}
              menu={{
                /* Focus the menu when it opens, so the keyboard path works
                   end to end: Enter opens it, the arrow keys move between
                   items, Esc closes it and hands focus back to the trigger
                   (measured without this: Enter opened the menu but focus
                   stayed on the trigger and the arrows did nothing). */
                autoFocus: true,
                items: [
                  {
                    key: "archive",
                    label: (
                      <span>
                        {s.archived ? t("sessions.unarchive") : t("sessions.archive")}
                        <br />
                        <span className="micro muted">
                          {s.archived
                            ? t("sessions.unarchiveHint")
                            : t("sessions.archiveHint")}
                        </span>
                      </span>
                    ),
                    onClick: () => void onArchive(s, !s.archived),
                  },
                  ...(s.deletable
                    ? [
                        {
                          key: "delete",
                          danger: true,
                          label: (
                            <span>
                              {t("sessions.delete")}
                              <br />
                              <span className="micro muted">
                                {t("sessions.deleteHint")}
                              </span>
                            </span>
                          ),
                          onClick: () => setConfirmDelete(s),
                        },
                      ]
                    : []),
                ],
              }}
            >
              {/* A host <button>, not IconButton: antd's Dropdown clones its
                  child to inject aria-haspopup / aria-expanded, and IconButton
                  takes a fixed prop list, so the injected attributes would be
                  dropped. Same .icon-btn class, so the row keeps one shape. */}
              <button
                type="button"
                className="icon-btn"
                aria-label={t("sessions.more", { name: sessionName(s, t) })}
                aria-haspopup="menu"
                aria-expanded={moreOpen === s.key}
                title={t("sessions.more", { name: sessionName(s, t) })}
              >
                ⋯
              </button>
            </Dropdown>
          ) : (
            <>
              {/* Archive is a ruagent-side hide and works for every source: it
                  writes one row in the daemon's own table and touches no file. */}
              <IconButton
                label={s.archived ? t("sessions.unarchiveHint") : t("sessions.archiveHint")}
                title={s.archived ? t("sessions.unarchiveHint") : t("sessions.archiveHint")}
                icon={s.archived ? "unarchive" : "archive"}
                size={14}
                className={actionCls}
                disabled={busy === s.key}
                onClick={() => void onArchive(s, !s.archived)}
              />
              {/* No delete control at all for the other tools' histories: the
                  row is an index of a file ruagent must not remove, so offering
                  the button (even disabled) would misdescribe what ruagent can
                  do. */}
              {s.deletable ? (
                <Popconfirm
                  title={t("sessions.deleteConfirm")}
                  description={t("sessions.deleteConfirmHint")}
                  okText={t("sessions.delete")}
                  cancelText={t("common.cancel")}
                  okButtonProps={{ danger: true }}
                  /* Controlled so the Escape handler above can close it. */
                  open={confirmPop === s.key}
                  onOpenChange={(o) => setConfirmPop(o ? s.key : null)}
                  onConfirm={() => void onDelete(s)}
                >
                  <button
                    type="button"
                    className={actionCls ? "icon-btn " + actionCls : "icon-btn"}
                    aria-label={t("sessions.deleteHint")}
                    title={t("sessions.deleteHint")}
                    disabled={busy === s.key}
                    /* Remember the trigger for the Escape hand-back; antd
                       chains this with its own click handler. */
                    onClick={(e) => {
                      popTrigger.current = e.currentTarget;
                    }}
                  >
                    <Icon name="trash" size={14} />
                  </button>
                </Popconfirm>
              ) : null}
            </>
          )}
        </div>
      ))}
      {end < rows.length && (
        <div style={{ height: (rows.length - end) * ROW_H }} aria-hidden="true" />
      )}
      {/* The narrow "more" menu's delete confirm (S10): the wide row confirms
          in an inline Popconfirm, but a menu item cannot host one — the menu
          closes on click. Same copy and the same two-step discipline, in the
          page's own Modal. */}
      {confirmDelete ? (
        <Modal
          title={t("sessions.deleteConfirm")}
          onClose={() => setConfirmDelete(null)}
          footer={
            <span className="row tight">
              <span className="grow" />
              <Button onClick={() => setConfirmDelete(null)}>
                {t("common.cancel")}
              </Button>
              <Button
                danger
                onClick={() => {
                  const s = confirmDelete;
                  setConfirmDelete(null);
                  void onDelete(s);
                }}
              >
                {t("sessions.delete")}
              </Button>
            </span>
          }
        >
          <p className="muted">{t("sessions.deleteConfirmHint")}</p>
        </Modal>
      ) : null}
    </div>
  );
}

/** Shown only when the first response is slower than 200ms, so the page is
 *  never a blank screen (view-sessions.md §5, loading). */
/** The ledger statuses this build knows how to name. Anything else is printed
 *  VERBATIM: a status a future daemon invents must not be silently mapped onto
 *  one of these words, and must never reach `t()` as a missing key (which would
 *  render the raw key string). */
const LEDGER_STATUSES = ["ok", "empty", "failed", "not_recorded"];

/** The chip for the NEWEST attempt, or null when there is nothing to say.
 *
 *  `distilled` answers "has a successful attempt covered this session" — the
 *  ledger's criterion, and the axis the filter queries — while `distill_status`
 *  is what happened LAST. The two can disagree (distilled last week, a failing
 *  attempt today), and the page needs both: the first chip mirrors the filter,
 *  the second reports the newest attempt without contradicting it. */
function lastAttemptChip(s: SessionRecord): { labelKey: string; cls: string } | null {
  if (s.distill_status === "ok") return null;
  if (s.distill_status === "failed") {
    return { labelKey: "sessions.distill.state.failed", cls: "tag err" };
  }
  if (s.distill_status === "empty") {
    return { labelKey: "sessions.distill.state.empty", cls: "tag warn" };
  }
  if (s.distill_status == null && s.distilled_at) {
    // A ledger row exists but recorded no status (the fourth state). Only then:
    // with no row at all there is no attempt to report.
    return { labelKey: "sessions.distill.state.norecord", cls: "tag" };
  }
  return null;
}

/** The badge's tooltip: the standing AND when the newest attempt ran — the date
 *  the daemon answered for that row, or "never attempted". It is the same
 *  sentence for both chips, because they are two halves of one reading. */
function standingHint(
  s: SessionRecord,
  t: (key: string, params?: Record<string, string | number>) => string,
): string {
  return t("sessions.distill.hint", {
    state: t(s.distilled ? "sessions.distill.state.ok" : "sessions.distill.state.none"),
    when: s.distilled_at ? dateOf(s.distilled_at) : t("sessions.distill.never"),
  });
}

/** #47's batch entry: the PLAN first, then the RUN.
 *
 *  The two halves come from the same endpoint shape (a dry run answers
 *  `actual: null` and `attempted: 0`), which is what lets the estimate and the
 *  reading be shown side by side without translating between two vocabularies.
 *  Every unrecorded measurement prints 未记录 and never 0: a batch whose rows
 *  recorded no cost must not read as a free batch. */
function BatchDistillModal({
  plan,
  planErr,
  tier,
  onTierChange,
  onReloadPlan,
  onClose,
  onDone,
}: {
  plan: DistillBatchOutcome | null;
  planErr: unknown;
  /** issue #57: the tier this pass will run, owned by the PAGE (the planner and
   *  the row action use the same value, so they cannot drift apart). */
  tier: DistillTier;
  onTierChange: (next: DistillTier) => void;
  onReloadPlan: () => void;
  onClose: () => void;
  onDone: () => void;
}) {
  const { t } = useI18n();
  const [busy, setBusy] = useState(false);
  const [outcome, setOutcome] = useState<DistillBatchOutcome | null>(null);
  const [err, setErr] = useState<unknown>(null);
  /** The tier the RUN was started with, captured at click time: the Segmented
   *  stays live while the receipt is on screen, so reading `tier` when the
   *  receipt renders would credit this pass to whatever tier the user moved to
   *  afterwards. */
  const [sentTier, setSentTier] = useState<DistillTier | null>(null);

  /** A recorded number, or the ledger's own word for "nobody wrote it down". */
  const measured = (v: number | null, render: (n: number) => string) =>
    v == null ? t("distill.notRecorded") : render(v);
  const statusLabel = (s: string) =>
    LEDGER_STATUSES.includes(s) ? t(`distill.ledger.status.${s}`) : s;
  const statusCls = (s: string) =>
    s === "ok" ? "tag ok" : s === "failed" ? "tag err" : s === "empty" ? "tag warn" : "tag";

  const run = async () => {
    setBusy(true);
    setErr(null);
    const sent = tier;
    try {
      const res = await api.distillBatch({ extractor: sent, limit: DISTILL_BATCH_LIMIT });
      setSentTier(sent);
      setOutcome(res);
      // No success toast: the block below IS the confirmation, and it stays on
      // screen (the page's own rule for a mutation's answer — a 2s notice would
      // be gone before the numbers were read).
      onDone();
    } catch (e) {
      // A failed MUTATION still toasts, and the line below keeps it readable.
      setErr(e);
    } finally {
      setBusy(false);
    }
  };

  const est = plan?.estimated;
  const act = outcome?.actual;
  /** Whether the tier that RAN calls the model. The daemon's own label decides:
   *  `rules` is the free one, `acp` and `rules+acp` are not. */
  const ranFree = outcome != null && outcome.extractor === "rules";
  /** #62: the daemon's own count of attempts that answered ok and wrote NOTHING,
   *  or null when this daemon does not report it at all. "Not reported" and
   *  "measured 0" are different readings and the panel must not merge them. */
  const refusedCount =
    outcome && typeof outcome.refused === "number" ? outcome.refused : null;
  /** `rules` is free, so the estimate's windows/tokens/cost — priced from the
   *  ledger's means over historical attempts, i.e. from ACP runs — describe the
   *  OTHER tier. The suffix names which numbers those are; the tier note above
   *  says what THIS pass spends. Without it, "模型调用 12 次" would sit on a pass
   *  that will make none. */
  const acpBasis = tier === "rules" ? t("sessions.tier.acpBasis") : "";
  /** The estimate is only usable for the tier it was priced for. Until the
   *  planner's answer carries the selected tier, the estimate on screen belongs
   *  to a different pass and starting is refused (issue #57: one tier may never
   *  be estimated and another run). */
  const planTierStale = plan != null && plan.extractor !== tier;

  return (
    <Modal title={t("distill.batch.title")} onClose={onClose}>
      {/* issue #57: the tier, and what it COSTS, before anything else in this
          modal. The button at the bottom is one click over up to twenty
          sessions, so "which tier is this, and does it call the model" must not
          have to be inferred from a label or remembered from the page bar. */}
      <div className="row wrap" data-tier={tier}>
        <span className="muted micro">{t("sessions.tier.label")}</span>
        <Segmented
          value={tier}
          onChange={(v) => onTierChange(v as DistillTier)}
          aria-label={t("sessions.tier.label")}
          options={[
            { value: "rules", label: t("sessions.tier.rules") },
            { value: "acp", label: t("sessions.tier.acp") },
          ]}
        />
        <span className="micro" data-tier-note={tier}>
          {tier === "rules" ? t("sessions.tier.note.rules") : t("sessions.tier.note.acp")}
        </span>
      </div>
      {plan ? (
        <p className="muted micro" data-plan-extractor={plan.extractor}>
          {t("sessions.tier.plan", { x: plan.extractor })}
        </p>
      ) : null}
      {planTierStale ? (
        <p className="muted micro" data-plan-stale="1">
          {t("sessions.tier.planStale")}
        </p>
      ) : null}
      {planErr && !plan ? (
        <ErrorState
          title={t("distill.batch.err")}
          hint={`${t("distill.batch.err.hint")} — ${String(planErr).slice(0, 200)}`}
          onRetry={onReloadPlan}
          retryLabel={t("common.retry")}
        />
      ) : null}
      {!plan && !planErr ? <Spinner /> : null}

      {plan && est && !outcome ? (
        <>
          <p className="muted micro">{t("distill.batch.dryNote")}</p>
          <p>
            {t("distill.batch.planned", { n: plan.planned })}
            {" · "}
            {t("distill.batch.backlog", { n: plan.undistilled_total })}
          </p>
          <ReadoutStrip
            grid
            items={[
              { key: "sessions", label: t("distill.batch.estSessions"), value: est.sessions },
              {
                key: "windows",
                label: t("distill.batch.estWindows") + acpBasis,
                value: est.windows,
              },
              {
                key: "tokens",
                label: t("distill.batch.estTokens") + acpBasis,
                value: measured(est.tokens_used, fmtTokens),
              },
              {
                key: "cost",
                label: t("distill.batch.estCost") + acpBasis,
                value: measured(est.cost_usd, fmtUsd),
              },
            ]}
          />
          <p className="muted micro">
            {t("distill.batch.budget", { n: est.budget_bytes })}
            {" · "}
            {t("distill.batch.basis", {
              ok: est.basis.ok_attempts,
              tr: est.basis.tokens_recorded,
              cr: est.basis.cost_recorded,
              wr: est.basis.windows_recorded,
            })}
          </p>
          {/* The daemon's own wording of the method, verbatim: a second
              paraphrase in the panel is a copy that drifts from the formula. */}
          <details className="prompt-view">
            <summary>{t("distill.batch.method")}</summary>
            <p className="muted micro">{est.method}</p>
          </details>
          {plan.planned === 0 ? <p className="muted">{t("distill.batch.nothing")}</p> : null}
        </>
      ) : null}

      {outcome ? (
        <>
          {/* issue #57: WHICH TIER RAN, AND WHETHER IT CALLED THE MODEL. Two
              tiers are named on purpose: `sentTier` is what this request asked
              for, `outcome.extractor` is what the daemon says it ran. Normally
              they are the same string; if they ever differ the receipt says so
              rather than choosing one, because a receipt that quietly credits a
              pass to the wrong tier is the defect #57 is about. */}
          <div
            className="row wrap"
            data-run-tier={sentTier ?? ""}
            data-run-extractor={outcome.extractor}
          >
            <span className="micro">{t("sessions.tier.run", { x: outcome.extractor })}</span>
            {sentTier ? (
              <span className="muted micro">{t("sessions.tier.asked", { x: sentTier })}</span>
            ) : null}
          </div>
          <p className="micro" data-run-calls={ranFree ? "none" : "model"}>
            {ranFree
              ? t("sessions.tier.run.calls.rules", {
                  n: measured(act ? act.windows : null, String),
                })
              : t("sessions.tier.run.calls.acp", {
                  n: measured(act ? act.windows : null, String),
                })}
          </p>
          {sentTier && sentTier !== outcome.extractor ? (
            <p className="muted micro" data-run-mismatch="1">
              {t("sessions.tier.run.mismatch", { x: outcome.extractor, y: sentTier })}
            </p>
          ) : null}
          <p>
            <strong>
              {t("distill.batch.counted", {
                n: outcome.attempted,
                ok: outcome.ok,
                empty: outcome.empty,
                failed: outcome.failed,
              })}
            </strong>
          </p>
          {/* #62: THE REFUSED CLASS, beside the ledger's three words rather than
              as a fourth. "refused" is a subset of "ok" (an attempt that ran,
              was recorded, and wrote nothing because the vocabulary refused every
              candidate), so the sum below is the reading that says the three
              states still add up. An older daemon omits the key and this says so
              instead of printing a 0 it never measured. */}
          <p
            className="muted micro"
            data-batch-refused={refusedCount == null ? "not_recorded" : String(refusedCount)}
          >
            {refusedCount == null
              ? t("sessions.refused.batchNotReported")
              : t("sessions.refused.batch", { n: refusedCount })}
          </p>
          <p
            className="muted micro"
            data-batch-sum={
              String(outcome.ok + outcome.empty + outcome.failed) + "/" + String(outcome.attempted)
            }
          >
            {t("sessions.refused.batchSum", {
              ok: outcome.ok,
              empty: outcome.empty,
              failed: outcome.failed,
              sum: outcome.ok + outcome.empty + outcome.failed,
              attempted: outcome.attempted,
            })}
          </p>
          {act ? (
            <>
              <div className="zone-head">
                <div className="zone-title">{t("distill.batch.actual")}</div>
              </div>
              <ReadoutStrip
                grid
                items={[
                  { key: "rows", label: t("distill.batch.actualRows"), value: act.rows },
                  {
                    key: "windows",
                    // On the free tier the ledger's `windows` count is the number
                    // of WINDOWS the pass read, not model calls: the label must not
                    // turn a free pass into "1 次调用" (issue #57).
                    label: ranFree
                      ? t("sessions.tier.windows.rules")
                      : t("distill.batch.actualWindows"),
                    value: measured(act.windows, (n) => t("distill.ledger.calls", { n })),
                  },
                  {
                    key: "tokens",
                    label: t("distill.batch.actualTokens"),
                    value: measured(act.tokens_used, fmtTokens),
                  },
                  {
                    key: "cost",
                    label: t("distill.batch.actualCost"),
                    value: measured(act.cost_usd, fmtUsd),
                  },
                ]}
              />
              <p className="muted micro">{t("distill.batch.actualNote")}</p>
            </>
          ) : null}
          <div className="zone-head">
            <div className="zone-title">{t("distill.batch.results")}</div>
            <span className="zone-note">{outcome.results.length}</span>
          </div>
          <div className="card">
            {outcome.results.map((r) => (
              <div
                key={r.session_key}
                className="row tight"
                data-result-status={r.status}
              >
                <span className="mono micro truncated" title={r.session_key}>
                  {r.session_key}
                </span>
                <span className={statusCls(r.status)}>{statusLabel(r.status)}</span>
                {/* #62: the per-session subset marker. "ok" with 0 written is
                    the shape a reader would otherwise take for a normal success. */}
                {typeof r.memories_refused === "number" &&
                r.memories_refused > 0 &&
                (r.memories_written ?? 0) === 0 ? (
                  <span className="tag warn" data-result-refused={r.memories_refused}>
                    {t("sessions.refused.result", { n: r.memories_refused })}
                  </span>
                ) : null}
                <span className="grow" />
                {Array.isArray(r.refusals) && r.refusals.length ? (
                  <span
                    className="muted micro"
                    data-result-pairs={r.refusals
                      .map((x) => x.store + " × " + x.namespace)
                      .join(", ")}
                  >
                    {r.refusals
                      .map(
                        (x) =>
                          x.store +
                          " × " +
                          x.namespace +
                          " → " +
                          (x.allowed_namespaces.length
                            ? x.allowed_namespaces.join(" / ")
                            : t("sessions.refused.allowedEmpty")),
                      )
                      .join(" · ")}
                  </span>
                ) : null}
                {r.reason ? <span className="muted micro">{r.reason}</span> : null}
              </div>
            ))}
          </div>
        </>
      ) : null}

      {err ? (
        <ErrorState
          title={t("distill.batch.err")}
          hint={`${t("distill.batch.err.hint")} — ${String(err).slice(0, 200)}`}
        />
      ) : null}

      <div className="row end">
        {outcome ? (
          <Button onClick={onClose}>{t("common.close")}</Button>
        ) : (
          <>
            <Button onClick={onClose}>{t("common.cancel")}</Button>
            <Button
              type="primary"
              loading={busy}
              disabled={busy || !plan || plan.planned === 0 || planTierStale}
              onClick={() => void run()}
            >
              {busy ? t("distill.batch.running") : t("distill.batch.start")}
            </Button>
          </>
        )}
      </div>
    </Modal>
  );
}
function CardSkeleton() {
  return (
    <div className="card" aria-busy="true" aria-hidden="true">
      {Array.from({ length: 10 }).map((_, i) => (
        <div key={i} className="row tight">
          <span className="tag">&nbsp;</span>
          <span className="title">&nbsp;</span>
        </div>
      ))}
    </div>
  );
}


export function SessionDetail({
  session,
  onClose,
}: {
  session: SessionRecord;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const [messages, setMessages] = useState<{ role: string; text: string }[] | null>(null);
  const [failed, setFailed] = useState(false);
  const dialog = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let alive = true;
    api
      .sessionMessages(session.key)
      .then((m) => {
        if (alive) setMessages(m);
      })
      .catch(() => {
        if (alive) {
          setFailed(true);
          setMessages([]);
        }
      });
    return () => {
      alive = false;
    };
  }, [session.key]);

  // Escape closes (rows 21/22); the dialog takes focus on open so the tab
  // order starts inside the overlay, and close returns it to the row.
  useEffect(() => {
    dialog.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  const shown = messages && messages.length > MSG_CAP ? messages.slice(-MSG_CAP) : messages;
  const title = sessionName(session, t);

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal wide session-viewer"
        role="dialog"
        aria-modal="true"
        aria-label={title}
        tabIndex={-1}
        ref={dialog}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="modal-head">
          <h3>
            {title}
            <span className="muted micro">
              {" "}
              {SOURCE_LABEL[session.source] ?? session.source}
              {session.project ? ` · ${session.project}` : ""} · {session.message_count}{" "}
              {t("sessions.messages")} · <RelTime iso={msToIso(session.updated_at)} />
            </span>
          </h3>
          <Button type="text" size="small" onClick={onClose} aria-label={t("common.close")}>
            ✕
          </Button>
        </div>
        {/* Why the row had no delete control. Stated here rather than as a
            disabled button, so the row never advertises a capability ruagent
            does not have. */}
        {session.deletable === false ? (
          <p className="muted micro">
            {t("sessions.deleteNotOurs", {
              s: SOURCE_LABEL[session.source] ?? session.source,
            })}
          </p>
        ) : null}
        <div className="modal-body session-log">
          {shown === null ? (
            <Spinner />
          ) : shown.length === 0 ? (
            failed ? (
              <ErrorState
                title={t("sessions.err")}
                onRetry={() => window.location.reload()}
                retryLabel={t("common.retry")}
              />
            ) : (
              <Empty icon="chat" title={t("sessions.noMessages")} />
            )
          ) : (
            <>
              {messages && messages.length > MSG_CAP ? (
                <p className="muted micro">{t("sessions.msgCap", { n: MSG_CAP })}</p>
              ) : null}
              {shown.map((m, i) => (
                <div key={i} className={m.role === "user" ? "chat-msg user" : "chat-msg agent"}>
                  {m.role === "user" ? m.text : <Markdown>{m.text}</Markdown>}
                </div>
              ))}
            </>
          )}
        </div>
      </div>
    </div>
  );
}

function shortProject(p: string): string {
  // ruagent-spawned agent workspaces collapse to a readable label.
  const m = p.match(/[\/]workspaces[\/](chat|run)-([0-9a-f]{8})/);
  if (m) return `ruagent ${m[1]} ·${m[2]}`;
  const parts = p.split(/[\/]/);
  return parts.slice(-2).join("/");
}
