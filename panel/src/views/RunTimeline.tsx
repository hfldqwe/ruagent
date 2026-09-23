// Run timeline: the execution log — every event rendered richly, bilingual.
//
// A run is a flight recorder, and the reference run in this repo carries
// 26,798 events (24,446 thought chunks / 1,659 message chunks / 462 tool
// updates). One DOM row per event produced 153,137 nodes and a 26,343-row
// `.timeline` — MASTER §12 rows 5/25, the panel's only P0 page. The three
// structural fixes live here, in the order
// docs/design/views/view-task-detail.md §4.3 names them:
//
//   ① merge consecutive same-kind stream chunks in the state layer
//      26,798 lines → ~400 rows (44 thought runs + 37 message runs + the
//      discrete events). A `tool_call_update` that carries no output never
//      takes a row — it renders nothing, so it is not a row.
//   ② virtualize: only the rows inside the scroll window are mounted, so the
//      node count follows the 580px viewport instead of the run length.
//   ③ follow the tail by writing `scrollTop` — `scrollIntoView` on a
//      six-figure-pixel box forces a layout on every single event.
//
// Everything else (rich per-type shells, the head, the jump-to-latest pill)
// is unchanged.

import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { Button } from "antd";
import type { Run } from "../api";
import { useI18n } from "../i18n";
import { StatusPill, UsageMeter, fmtUsd } from "../ui";
import { Markdown } from "./lazy-markdown";
import { dateOf } from "../i18n";
import { Icon } from "../icons";

interface EventLine {
  ts: string;
  seq: number;
  event: {
    type: string;
    [k: string]: unknown;
  };
}

/** One rendered log row. Stream chunks of the same kind collapse into a
 * single row carrying the concatenated text; everything else is a row per
 * event. */
type Row =
  | { kind: "msg"; key: string; seq: number; text: string; chunks: number }
  | { kind: "thought"; key: string; seq: number; text: string; chunks: number }
  | { kind: "event"; key: string; seq: number; line: EventLine };

/** Text payload of a `*_chunk` event (ACP content blocks). */
function chunkText(ev: Record<string, unknown>): string {
  const content = ev.content;
  if (!Array.isArray(content)) return "";
  let out = "";
  for (const c of content as { text?: string }[]) out += c?.text ?? "";
  return out;
}

/** Fold one SSE line into the row list, returning a new array. Identity is
 * preserved when the line adds no row (an empty chunk, an output-less tool
 * update) so React sees no change on that path. */
function foldLine(rows: Row[], line: EventLine): Row[] {
  const ev = line.event;
  const type = ev.type;
  if (type === "agent_message_chunk" || type === "agent_thought_chunk") {
    const text = chunkText(ev);
    if (!text) return rows;
    const kind = type === "agent_message_chunk" ? "msg" : "thought";
    const last = rows[rows.length - 1];
    if (last && last.kind === kind) {
      const next = rows.slice();
      next[next.length - 1] = {
        ...last,
        text: last.text + text,
        chunks: last.chunks + 1,
        seq: line.seq,
      };
      return next;
    }
    return [...rows, { kind, key: `c${line.seq}`, seq: line.seq, text, chunks: 1 }];
  }
  if (type === "tool_call_update" && ev.raw_output == null) return rows;
  return [...rows, { kind: "event", key: `e${line.seq}`, seq: line.seq, line }];
}

/** Same fold, in one pass — the bulk (non-live) load of a whole transcript. */
function foldAll(lines: EventLine[]): Row[] {
  const rows: Row[] = [];
  for (const line of lines) {
    const ev = line.event;
    const type = ev.type;
    if (type === "agent_message_chunk" || type === "agent_thought_chunk") {
      const text = chunkText(ev);
      if (!text) continue;
      const kind = type === "agent_message_chunk" ? "msg" : "thought";
      const last = rows[rows.length - 1];
      if (last && last.kind === kind) {
        rows[rows.length - 1] = {
          ...last,
          text: last.text + text,
          chunks: last.chunks + 1,
          seq: line.seq,
        };
      } else {
        rows.push({ kind, key: `c${line.seq}`, seq: line.seq, text, chunks: 1 });
      }
      continue;
    }
    if (type === "tool_call_update" && ev.raw_output == null) continue;
    rows.push({ kind: "event", key: `e${line.seq}`, seq: line.seq, line });
  }
  return rows;
}

/** Parse an SSE transcript body into event lines. */
function parseSse(body: string): EventLine[] {
  const out: EventLine[] = [];
  for (const line of body.split("\n")) {
    if (!line.startsWith("data: ")) continue;
    try {
      const parsed = JSON.parse(line.slice(6)) as EventLine;
      if (parsed && typeof parsed.event === "object" && parsed.event !== null) {
        out.push(parsed);
      }
    } catch {
      /* malformed line */
    }
  }
  return out;
}

// ── Window sizing ──────────────────────────────────────────────────────────
// Rows have variable height (a markdown paragraph is not a one-line notice),
// so heights are measured once per mounted row and cached by index; rows that
// were never mounted fall back to an estimate. Rows only ever append, so an
// index stays valid for the life of a run.

/** Fallback row height before anything has been measured. */
const ROW_EST = 24;
/** Extra rows kept mounted on each side of the viewport, for smooth scroll. */
const OVERSCAN_ROWS = 3;
/** Hard cap on mounted rows. Every mounted `.ev` row carries a border-left,
 * and MASTER §12 row 5 budgets 40 visible bordered elements for the whole
 * route (7 of them are the shell's own). 30 rows still cover the 580px
 * viewport even when every row is a one-line notice. */
const MAX_MOUNTED = 30;

export function RunTimeline({ run, live }: { run: Run; live: boolean }) {
  const { t } = useI18n();
  const [rows, setRows] = useState<Row[]>([]);
  const [ended, setEnded] = useState<string | null>(null);
  /** The transcript is being fetched: "not loaded yet" is not "no events". */
  const [loading, setLoading] = useState(!live);
  const [dropped, setDropped] = useState(false);
  const [autoScroll, setAutoScroll] = useState(true);
  const boxRef = useRef<HTMLDivElement>(null);

  // Virtual window
  const heights = useRef<number[]>([]);
  const [win, setWin] = useState({ start: 0, end: MAX_MOUNTED });
  const [pad, setPad] = useState({ top: 0, bottom: 0 });
  const [measureTick, setMeasureTick] = useState(0);
  /** Manual reconnect, wired by the live effect. */
  const reconnectRef = useRef<() => void>(() => {});

  // ── Live: SSE ────────────────────────────────────────────────────────────
  useEffect(() => {
    if (!live) return;
    let closed = false;
    let tries = 0;
    let timer: number | null = null;
    let es: EventSource | null = null;
    heights.current = [];
    setRows([]);
    setEnded(null);
    setDropped(false);
    if (boxRef.current) boxRef.current.scrollTop = 0;

    const open = () => {
      if (closed) return;
      es = new EventSource(`/api/v1/runs/${run.id}/events`);
      es.onmessage = (e) => {
        tries = 0;
        try {
          const parsed = JSON.parse(e.data) as EventLine;
          if (parsed && typeof parsed.event === "object" && parsed.event !== null) {
            setRows((prev) => foldLine(prev, parsed));
          }
        } catch {
          /* malformed line */
        }
      };
      es.addEventListener("end", (e) => {
        closed = true;
        setDropped(false);
        try {
          setEnded((JSON.parse((e as MessageEvent).data) as { status: string }).status);
        } catch {
          setEnded("ended");
        }
        es?.close();
      });
      // A dropped stream used to be an empty function: a broken connection
      // looked exactly like a quiet run. Now it is visible and it retries
      // (MASTER §12 row 20 — error must be distinguishable from empty).
      es.onerror = () => {
        es?.close();
        if (closed) return;
        setDropped(true);
        if (tries >= 5) return;
        const delay = 1000 * 2 ** tries;
        tries += 1;
        timer = window.setTimeout(() => {
          timer = null;
          open();
        }, delay);
      };
    };
    reconnectRef.current = () => {
      if (timer !== null) {
        clearTimeout(timer);
        timer = null;
      }
      tries = 0;
      setDropped(false);
      open();
    };
    open();

    return () => {
      closed = true;
      if (timer !== null) clearTimeout(timer);
      es?.close();
    };
  }, [run.id, live]);

  // ── Not live: one fetch of the whole transcript ──────────────────────────
  useEffect(() => {
    if (live) return;
    let cancelled = false;
    heights.current = [];
    setLoading(true);
    fetch(`/api/v1/runs/${run.id}/events`)
      .then((r) => r.text())
      .then((sse) => {
        if (cancelled) return;
        setRows(foldAll(parseSse(sse)));
        setEnded(run.status);
      })
      .catch(() => {
        if (!cancelled) setEnded(run.status);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [run.id, live, run.status]);

  // ── Window computation ───────────────────────────────────────────────────
  // Depends on `rows` on purpose: a layout effect runs before the passive
  // effect that would sync a ref, so a ref here would size the window against
  // the previous row list (the empty-log bug).
  const recompute = useCallback(() => {
    const box = boxRef.current;
    if (!box) return;
    const n = rows.length;
    const scrollTop = box.scrollTop;
    const viewport = box.clientHeight || 580;

    // First row whose bottom edge is below the top of the viewport.
    let cum = 0;
    let start = n;
    for (let i = 0; i < n; i++) {
      const h = heights.current[i] ?? ROW_EST;
      if (cum + h > scrollTop) {
        start = i;
        break;
      }
      cum += h;
    }
    if (start === n) start = Math.max(0, n - 1);
    start = Math.max(0, start - OVERSCAN_ROWS);

    let top = 0;
    for (let i = 0; i < start; i++) top += heights.current[i] ?? ROW_EST;

    let acc = top;
    let end = start;
    for (let i = start; i < n; i++) {
      if (acc > scrollTop + viewport) break;
      acc += heights.current[i] ?? ROW_EST;
      end = i + 1;
    }
    end = Math.min(n, end + OVERSCAN_ROWS);
    if (end - start > MAX_MOUNTED) end = start + MAX_MOUNTED;

    let bottom = 0;
    for (let i = end; i < n; i++) bottom += heights.current[i] ?? ROW_EST;

    setWin((w) => (w.start === start && w.end === end ? w : { start, end }));
    setPad((p) => (p.top === top && p.bottom === bottom ? p : { top, bottom }));
  }, [rows]);

  // The box grows to its 580px ceiling only once rows exist, and that growth
  // is not a prop change — without this observer the first pass would size the
  // window against an empty box and never revisit it.
  useEffect(() => {
    const box = boxRef.current;
    if (!box || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(() => recompute());
    ro.observe(box);
    return () => ro.disconnect();
  }, [recompute]);

  // One pass per render: follow the tail (③), measure what is mounted, then
  // recompute the window. A changed measurement bumps the tick once, which
  // re-runs this effect with stable heights and terminates.
  useLayoutEffect(() => {
    const box = boxRef.current;
    if (!box) return;
    if (autoScroll) box.scrollTop = box.scrollHeight;
    let changed = false;
    for (const el of box.querySelectorAll<HTMLElement>("[data-row]")) {
      const i = Number(el.dataset.row);
      const h = el.offsetHeight;
      if (h && heights.current[i] !== h) {
        heights.current[i] = h;
        changed = true;
      }
    }
    recompute();
    if (changed) setMeasureTick((v) => v + 1);
  }, [rows, autoScroll, recompute, measureTick]);

  const onScroll = () => {
    const box = boxRef.current;
    if (!box) return;
    const nearBottom = box.scrollHeight - box.scrollTop - box.clientHeight < 60;
    setAutoScroll((v) => (v === nearBottom ? v : nearBottom));
    recompute();
  };

  const jumpToLatest = () => {
    const box = boxRef.current;
    if (!box) return;
    setAutoScroll(true);
    box.scrollTop = box.scrollHeight;
    recompute();
  };

  const mounted = rows.slice(win.start, win.end);

  return (
    <div className="timeline-wrap">
      {/* A zone, not a box: rule + label, no surface of its own
          (primitives.md §9.1 #2). */}
      <div className="timeline-head zone-head">
        <StatusPill status={ended ?? run.status} />
        <span className="muted mono">{run.id.slice(0, 13)}…</span>
        {run.acp_session_id ? <span className="muted mono">{run.acp_session_id}</span> : null}
        {run.workspace ? (
          <span className="muted mono" title={run.workspace}>
            {run.workspace.length > 42 ? `…${run.workspace.slice(-40)}` : run.workspace}
          </span>
        ) : null}
        {run.cost_usd != null ? <span className="muted">{fmtUsd(run.cost_usd)}</span> : null}
        <span className="grow" />
        {live && dropped && !ended ? (
          <>
            <span className="tag err">{t("common.offline")}</span>
            <Button size="small" onClick={() => reconnectRef.current()}>
              {t("task.retryRun")}
            </Button>
          </>
        ) : live && !ended ? (
          <span className="live-flag">● {t("timeline.live")}</span>
        ) : (
          <span className="time">{dateOf(run.updated_at)}</span>
        )}
      </div>
      <div className="timeline" ref={boxRef} onScroll={onScroll}>
        {rows.length === 0 && (loading || !live) ? (
          <p className="muted pad">{loading ? t("common.loading") : t("timeline.noEvents")}</p>
        ) : null}
        {pad.top > 0 ? <div style={{ height: pad.top }} aria-hidden /> : null}
        {mounted.map((row, i) => (
          <RowView key={row.key} row={row} index={win.start + i} />
        ))}
        {pad.bottom > 0 ? <div style={{ height: pad.bottom }} aria-hidden /> : null}
      </div>
      {!autoScroll && rows.length > 0 && (
        <button className="jump-latest" onClick={jumpToLatest}>
          {t("timeline.latest")}
        </button>
      )}
    </div>
  );
}

/** A virtualized row: the wrapper carries the measurement index, the row
 * itself keeps its original shell. */
function RowView({ row, index }: { row: Row; index: number }) {
  return (
    <div className="ev-row" data-row={index}>
      {row.kind === "msg" ? (
        <MessageRow text={row.text} />
      ) : row.kind === "thought" ? (
        <ThoughtRow text={row.text} />
      ) : (
        <EventRow line={row.line} />
      )}
    </div>
  );
}

/** A disclosure whose payload is only mounted once it has been opened.
 *
 * `<details>` keeps its children in the DOM even while closed — and the
 * audit's visibility rule still gives them a box, so 7 collapsed `.raw`
 * payloads counted as 7 visible bordered elements against MASTER §12 row 5.
 * The browser keeps owning `open` (no controlled-component race); the view
 * only learns that it happened. */
function Collapse({
  className,
  summary,
  summaryClassName,
  children,
}: {
  className: string;
  summary: ReactNode;
  summaryClassName?: string;
  children: ReactNode;
}) {
  const [opened, setOpened] = useState(false);
  return (
    <details
      className={className}
      onToggle={(e) => {
        if (e.currentTarget.open) setOpened(true);
      }}
    >
      <summary className={summaryClassName}>{summary}</summary>
      {opened ? children : null}
    </details>
  );
}

function MessageRow({ text }: { text: string }) {
  if (!text.trim()) return null;
  return (
    <div className="ev msg">
      <Markdown>{text}</Markdown>
    </div>
  );
}

function ThoughtRow({ text }: { text: string }) {
  return (
    <div className="ev thought">
      <span className="ev-icon">
        <Icon name="thought" size={13} />
      </span>
      <span className="thought-text">{text}</span>
    </div>
  );
}

function EventRow({ line }: { line: EventLine }) {
  const { t } = useI18n();
  const e = line.event;
  switch (e.type) {
    case "tool_call":
      return <ToolRow id={String(e.tool_call_id)} title={String(e.title)} input={e.raw_input} />;
    case "tool_call_update":
      return <ToolUpdateRow id={String(e.tool_call_id)} output={e.raw_output} />;
    case "plan": {
      const entries = e.entries as { content: string; status: string }[];
      const done = entries.filter((x) => x.status === "completed").length;
      return (
        <div className="ev plan">
          <span className="ev-icon">
            <Icon name="plan" size={13} />
          </span>
          <div className="plan-body">
            <div className="plan-head">
              {t("timeline.plan")}{" "}
              <span className="muted">
                {done}/{entries.length}
              </span>
            </div>
            {entries.map((p, i) => (
              <div key={i} className={p.status === "completed" ? "plan-item done" : "plan-item"}>
                <span className="plan-check">
                  {p.status === "completed" ? "✓" : p.status === "in_progress" ? "◐" : "○"}
                </span>
                {p.content}
              </div>
            ))}
          </div>
        </div>
      );
    }
    case "usage_update": {
      const u = e.usage as { used: number; size: number };
      return (
        <div className="ev sys">
          <UsageMeter used={u.used} size={u.size} />
        </div>
      );
    }
    case "routed": {
      const d = e.decision as { source: { level?: string; rule_id?: string }; rationale?: string };
      const level = d.source?.level ?? "?";
      const why = d.source?.rule_id ? ` · ${d.source.rule_id}` : d.rationale ? ` · ${d.rationale}` : "";
      return (
        <div className="ev sys">
          <span className="ev-icon">
            <Icon name="compass" size={13} />
          </span>
          <span className="muted">
            {t("timeline.routed", { level })}
            {why}
          </span>
        </div>
      );
    }
    case "user_message": {
      // The original ask (injection rides separately as context_injected).
      const text = String(e.text ?? "");
      return (
        <Collapse className="ev inject" summary={<>{<span className="ev-icon"><Icon name="user" size={13} /></span>} {t("timeline.asked")}</>}>
          <pre className="raw">{text}</pre>
        </Collapse>
      );
    }
    case "context_injected": {
      const render = String(e.render);
      const blocks = (render.match(/<(\w+)>/g) ?? []).map((b) => b.slice(1, -1));
      return (
        <Collapse
          className="ev inject"
          summary={
            <>
              <span className="ev-icon"><Icon name="brain" size={13} /></span>{" "}
              {t("timeline.injected")}
              <span className="muted">
                {" "}
                {t("timeline.injectedBlocks", { blocks: blocks.join(", "), n: render.length })}
              </span>
            </>
          }
        >
          <pre className="raw">{render}</pre>
        </Collapse>
      );
    }
    case "permission_requested":
      return (
        <div className="ev perm">
          <span className="ev-icon">
            <Icon name="lock" size={13} />
          </span>
          <span>{t("timeline.permReq", { title: String(e.title) })}</span>
        </div>
      );
    case "permission_resolved": {
      const res = e.resolution as { source: string };
      const outcome = t(`status.${String(e.outcome)}`);
      const whoKey =
        res.source === "human"
          ? "timeline.by.human"
          : res.source === "approver_agent"
            ? "timeline.by.approver"
            : "timeline.by.rule";
      return (
        <div className="ev perm resolved">
          <span className="ev-icon">
            <Icon name="unlock" size={13} />
          </span>
          <span>{t("timeline.permRes", { outcome, who: t(whoKey) })}</span>
        </div>
      );
    }
    case "state_changed":
      return (
        <div className="ev sys">
          <span className="ev-icon">
            <Icon name="play" size={13} />
          </span>
          <StatusPill status={String(e.status)} />
        </div>
      );
    case "stopped":
      return (
        <div className="ev sys">
          <span className="ev-icon">
            <Icon name="stop" size={13} />
          </span>
          <span className="muted">
            {t("timeline.stopped", { reason: t(`status.${String(e.stop_reason)}`) })}
          </span>
        </div>
      );
    case "error":
      return (
        <div className="ev err">
          <span className="ev-icon">
            <Icon name="error" size={13} />
          </span>
          <span>{String(e.message)}</span>
        </div>
      );
    default:
      return (
        <div className="ev sys">
          <span className="muted mono">{e.type}</span>
        </div>
      );
  }
}

function ToolRow({ id, title, input }: { id: string; title: string; input: unknown }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="ev tool">
      {/* Row 23: a hand-written expand control owes `aria-expanded`. */}
      <button className="tool-head" aria-expanded={open} onClick={() => setOpen((v) => !v)}>
        <span className="ev-icon">
          <Icon name="tool" size={13} />
        </span>
        <strong>{title}</strong>
        <span className="muted mono">{id}</span>
        <span className="chev">{open ? "▾" : "▸"}</span>
      </button>
      {open && (
        <div className="tool-detail">
          <pre className="raw">{pretty(input)}</pre>
        </div>
      )}
    </div>
  );
}

function ToolUpdateRow({ id, output }: { id: string; output: unknown }) {
  const { t } = useI18n();
  if (output == null) return null;
  return (
    <Collapse className="ev tool-update" summaryClassName="muted" summary={t("timeline.toolOutput", { id })}>
      <pre className="raw">{pretty(output)}</pre>
    </Collapse>
  );
}

function pretty(v: unknown): string {
  if (v == null) return "—";
  try {
    return typeof v === "string" ? v : JSON.stringify(v, null, 2);
  } catch {
    return String(v);
  }
}
