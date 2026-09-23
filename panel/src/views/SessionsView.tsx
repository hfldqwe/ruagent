// The Sessions route body, split out of Sessions.tsx so that the module
// CommandPalette/Home/Chat import (SOURCE_LABEL, sourceHue, msToIso — a
// few pure helpers) stays in the entry chunk while this one is a route
// chunk, fetched on first visit (MASTER §12 row 27).
//
// The page's own notes live at the top of Sessions.tsx.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Button, Input, Popconfirm, Segmented, Select } from "antd";
import { api, type ArchivedMode, type SessionRecord } from "../api";
import { useI18n } from "../i18n";
import { Icon } from "../icons";
import {
  Empty,
  ErrorState,
  IconButton,
  ReadoutStrip,
  RelTime,
  Spinner,
  useToast,
} from "../ui";
import { Markdown } from "./lazy-markdown";
import { SOURCE_LABEL, sourceHue, msToIso } from "./Sessions";
// Contract S7 — read straight from the pure module rather than through the
// `./Sessions` re-export: the re-export exists so the ENTRY chunk
// (CommandPalette) does not drag a route module in, and this view is already
// that route chunk, so importing the source of truth directly costs nothing.
import { isSystemSession, isTempWorkspace } from "../session-source";

const ROW_H = 40;
/** Rows rendered beyond the viewport, so a fast scroll never shows a gap.
 *  Kept small on purpose: each mounted row carries one inline style (the
 *  source dot's colour), and row 12 caps inline styles at 50 per route. */
const OVERSCAN = 2;
/** The viewer is capped like the chat log: newest 400 messages. */
const MSG_CAP = 400;

/** S7 — which platform-generated sessions the list shows. */
type SystemMode = "exclude" | "include" | "only";

type ListState =
  | { kind: "loading" }
  | { kind: "ready"; rows: SessionRecord[]; at: number }
  | { kind: "error"; err: unknown; rows: SessionRecord[] | null; at: number | null };

export function Sessions() {
  const { t } = useI18n();
  const [state, setState] = useState<ListState>({ kind: "loading" });
  const [filter, setFilter] = useState<string>("all"); // source
  const [ws, setWs] = useState<string>("all"); // workspace (= project)
  const [when, setWhen] = useState<string>("all"); // time window
  const [q, setQ] = useState(""); // keyword
  // Which archived sessions the daemon should return. The hide marker lives
  // server-side, so this is a real query parameter and not a local flag.
  const [archivedMode, setArchivedMode] = useState<ArchivedMode>("exclude");
  const [archivedCount, setArchivedCount] = useState(0);
  /** S7 — which platform-generated sessions the list shows. The counterpart of
   *  archivedMode, and local (not a query parameter) because the page already
   *  holds every row: the origin rules are pure predicates over the fields. */
  const [systemMode, setSystemMode] = useState<SystemMode>("exclude");
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

  const load = useCallback(async () => {
    try {
      const r = await api.sessionsList({ archived: archivedMode });
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
  }, [archivedMode]);

  useEffect(() => {
    const timer = setTimeout(() => setSlow(true), 200);
    void load();
    const i = setInterval(() => void load(), 10000);
    return () => {
      clearTimeout(timer);
      clearInterval(i);
    };
  }, [load]);

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
    return rows.filter((s) => {
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
  }, [rows, filter, ws, when, q, systemMode]);

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
    try {
      const r = await api.sessionDistill(s.key);
      const d = r.distilled;
      toast(
        "ok",
        t("sessions.distilled", {
          m: d.memories_written,
          k: d.memories_skipped,
          e: d.entities_written,
          r: d.relations_written,
          a: d.agent,
        }),
      );
    } catch (e) {
      toast("err", String(e));
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
        <span className="grow" />
        {/* A Select, not a Segmented: the three labels are long enough that a
            segmented control is 377px wide, and a 390px phone only has 294px
            of column (measured: +71px of horizontal overflow). A Select
            ellipsises instead of pushing the page sideways. */}
        <Select
          value={archivedMode}
          onChange={(v) => setArchivedMode(v as ArchivedMode)}
          aria-label={t("sessions.archived")}
          style={{ minWidth: 160 }}
          options={[
            { value: "exclude", label: t("sessions.hideArchived") },
            { value: "include", label: t("sessions.showArchived", { n: archivedCount }) },
            { value: "only", label: t("sessions.onlyArchived") },
          ]}
        />
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
      </div>

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

      {open && <SessionDetail session={open} onClose={closeRow} />}
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
            // An action button owns its click; without this the row opened
            // the viewer on every archive/delete/distill press too.
            if ((e.target as HTMLElement).closest(".icon-btn")) return;
            onOpen(s, e.currentTarget);
          }}
          onKeyDown={(e) => {
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
          <span className="title">{s.title || t("sessions.untitled")}</span>
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
          {/* No delete control at all for the other tools' histories: the row
              is an index of a file ruagent must not remove, so offering the
              button (even disabled) would misdescribe what ruagent can do. */}
          {s.deletable ? (
            <Popconfirm
              title={t("sessions.deleteConfirm")}
              description={t("sessions.deleteConfirmHint")}
              okText={t("sessions.delete")}
              cancelText={t("common.cancel")}
              okButtonProps={{ danger: true }}
              onConfirm={() => void onDelete(s)}
            >
              <button
                type="button"
                className={actionCls ? "icon-btn " + actionCls : "icon-btn"}
                aria-label={t("sessions.deleteHint")}
                title={t("sessions.deleteHint")}
                disabled={busy === s.key}
              >
                <Icon name="trash" size={14} />
              </button>
            </Popconfirm>
          ) : null}
        </div>
      ))}
      {end < rows.length && (
        <div style={{ height: (rows.length - end) * ROW_H }} aria-hidden="true" />
      )}
    </div>
  );
}

/** Shown only when the first response is slower than 200ms, so the page is
 *  never a blank screen (view-sessions.md §5, loading). */
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
  const title = session.title || t("sessions.untitled");

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
