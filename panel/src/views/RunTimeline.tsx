// Run timeline: the execution log (Multica parity) — every event rendered
// richly: markdown messages, expandable tool calls, permission attribution,
// plan progress, usage meters, cancellation.

import { useEffect, useRef, useState } from "react";
import type { Run } from "../api";
import { api } from "../api";
import { Markdown, StatusPill, UsageMeter, dateOf, fmtUsd, relTime } from "../ui";

interface EventLine {
  ts: string;
  seq: number;
  event: {
    type: string;
    [k: string]: unknown;
  };
}

export function RunTimeline({ run, live }: { run: Run; live: boolean }) {
  const [lines, setLines] = useState<EventLine[]>([]);
  const [ended, setEnded] = useState<string | null>(null);
  const [autoScroll, setAutoScroll] = useState(true);
  const bottomRef = useRef<HTMLDivElement>(null);
  const boxRef = useRef<HTMLDivElement>(null);

  // Replay + live tail over SSE.
  useEffect(() => {
    if (!live) return;
    setLines([]);
    setEnded(null);
    const es = new EventSource(`/api/v1/runs/${run.id}/events`);
    es.onmessage = (e) => {
      try {
        const parsed = JSON.parse(e.data) as EventLine;
        if (parsed && typeof parsed.event === "object" && parsed.event !== null) {
          setLines((prev) => [...prev, parsed]);
        }
      } catch {
        /* malformed line */
      }
    };
    es.addEventListener("end", (e) => {
      try {
        setEnded((JSON.parse((e as MessageEvent).data) as { status: string }).status);
      } catch {
        setEnded("ended");
      }
      es.close();
    });
    es.onerror = () => {
      /* end event closes explicitly */
    };
    return () => es.close();
  }, [run.id, live]);

  // For completed runs: fetch the whole transcript at once.
  useEffect(() => {
    if (live) return;
    let cancelled = false;
    fetch(`/api/v1/runs/${run.id}/events`)
      .then((r) => r.text())
      .then((sse) => {
        if (cancelled) return;
        const out: EventLine[] = [];
        for (const line of sse.split("\n")) {
          if (!line.startsWith("data: ")) continue;
          try {
            const parsed = JSON.parse(line.slice(6)) as EventLine;
            // The end marker ({"status": ...}) has no `event` — skip it.
            if (parsed && typeof parsed.event === "object" && parsed.event !== null) {
              out.push(parsed);
            }
          } catch {
            /* skip */
          }
        }
        setLines(out);
        setEnded(run.status);
      })
      .catch(() => setEnded(run.status));
    return () => {
      cancelled = true;
    };
  }, [run.id, live, run.status]);

  useEffect(() => {
    if (autoScroll) bottomRef.current?.scrollIntoView({ block: "end" });
  }, [lines, autoScroll]);

  const onScroll = () => {
    const box = boxRef.current;
    if (!box) return;
    setAutoScroll(box.scrollHeight - box.scrollTop - box.clientHeight < 60);
  };

  return (
    <div className="timeline-wrap">
      <div className="timeline-head">
        <StatusPill status={ended ?? run.status} />
        <span className="muted mono">{run.id.slice(0, 13)}…</span>
        {run.acp_session_id ? <span className="muted mono">{run.acp_session_id}</span> : null}
        {run.workspace ? (
          <span className="muted mono" title={run.workspace}>
            {run.workspace.length > 42 ? `…${run.workspace.slice(-40)}` : run.workspace}
          </span>
        ) : null}
        {run.cost_usd != null ? <span className="muted">{fmtUsd(run.cost_usd)}</span> : null}
        {live && !ended ? (
          <span className="live-flag">● live</span>
        ) : (
          <span className="muted">{relTime(run.updated_at)}</span>
        )}
      </div>
      <div className="timeline" ref={boxRef} onScroll={onScroll}>
        {lines.length === 0 && !live ? (
          <p className="muted pad">No events recorded.</p>
        ) : null}
        {lines.map((l) => (
          <EventRow key={l.seq} line={l} />
        ))}
        <div ref={bottomRef} />
      </div>
      {!autoScroll && (
        <button
          className="jump-latest"
          onClick={() => {
            setAutoScroll(true);
            bottomRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
          }}
        >
          ↓ latest
        </button>
      )}
    </div>
  );
}

function EventRow({ line }: { line: EventLine }) {
  const e = line.event;
  switch (e.type) {
    case "agent_message_chunk": {
      const content = e.content as { text?: string }[];
      const text = content.map((c) => c.text ?? "").join("");
      return <MessageRow text={text} />;
    }
    case "agent_thought_chunk": {
      const content = e.content as { text?: string }[];
      const text = content.map((c) => c.text ?? "").join("");
      if (!text) return null;
      return (
        <div className="ev thought">
          <span className="ev-icon">💭</span>
          <span className="thought-text">{text}</span>
        </div>
      );
    }
    case "tool_call":
      return <ToolRow ts={line.ts} id={String(e.tool_call_id)} title={String(e.title)} input={e.raw_input} />;
    case "tool_call_update":
      return <ToolUpdateRow id={String(e.tool_call_id)} output={e.raw_output} />;
    case "plan": {
      const entries = e.entries as { content: string; status: string }[];
      const done = entries.filter((x) => x.status === "completed").length;
      return (
        <div className="ev plan">
          <span className="ev-icon">🗺️</span>
          <div className="plan-body">
            <div className="plan-head">
              plan <span className="muted">{done}/{entries.length}</span>
            </div>
            {entries.map((p, i) => (
              <div key={i} className={p.status === "completed" ? "plan-item done" : "plan-item"}>
                <span className="plan-check">{p.status === "completed" ? "✓" : p.status === "in_progress" ? "◐" : "○"}</span>
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
          <span className="ev-icon">🧭</span>
          <span className="muted">routed via {level}{why}</span>
        </div>
      );
    }
    case "context_injected": {
      const render = String(e.render);
      const blocks = (render.match(/<(\w+)>/g) ?? []).map((b) => b.slice(1, -1));
      return (
        <details className="ev inject">
          <summary>
            <span className="ev-icon">🧠</span> context injected
            <span className="muted"> {blocks.join(", ")} · {render.length} chars</span>
          </summary>
          <pre className="raw">{render}</pre>
        </details>
      );
    }
    case "permission_requested":
      return (
        <div className="ev perm">
          <span className="ev-icon">🔐</span>
          <span>
            permission requested: <strong>{String(e.title)}</strong>
          </span>
        </div>
      );
    case "permission_resolved": {
      const res = e.resolution as { source: string };
      const outcome = String(e.outcome).replaceAll("_", " ");
      const badge =
        res.source === "human" ? "👤 human" : res.source === "approver_agent" ? "🤖 approver" : "📜 rule";
      return (
        <div className="ev perm resolved">
          <span className="ev-icon">🔓</span>
          <span>
            {outcome} <span className="muted">by</span> {badge}
          </span>
        </div>
      );
    }
    case "state_changed":
      return (
        <div className="ev sys">
          <span className="ev-icon">▶</span>
          <StatusPill status={String(e.status)} />
        </div>
      );
    case "stopped":
      return (
        <div className="ev sys">
          <span className="ev-icon">■</span>
          <span className="muted">stopped: {String(e.stop_reason).replaceAll("_", " ")}</span>
        </div>
      );
    case "error":
      return (
        <div className="ev err">
          <span className="ev-icon">✖</span>
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

function MessageRow({ text }: { text: string }) {
  if (!text.trim()) return null;
  return (
    <div className="ev msg">
      <Markdown>{text}</Markdown>
    </div>
  );
}

function ToolRow({
  ts,
  id,
  title,
  input,
}: {
  ts: string;
  id: string;
  title: string;
  input: unknown;
}) {
  const [open, setOpen] = useState(false);
  return (
    <div className="ev tool">
      <button className="tool-head" onClick={() => setOpen((v) => !v)}>
        <span className="ev-icon">🔧</span>
        <strong>{title}</strong>
        <span className="muted mono">{id}</span>
        <span className="muted time">{dateOf(ts)}</span>
        <span className="chev">{open ? "▾" : "▸"}</span>
      </button>
      {open && (
        <div className="tool-detail">
          <div className="tool-io">
            <span className="io-label">input</span>
            <pre className="raw">{pretty(input)}</pre>
          </div>
        </div>
      )}
    </div>
  );
}

function ToolUpdateRow({ id, output }: { id: string; output: unknown }) {
  // Render updates inline under the last tool row is complex; show a
  // compact expandable for non-empty outputs.
  if (output == null) return null;
  return (
    <details className="ev tool-update">
      <summary className="muted">↳ output of {id}</summary>
      <pre className="raw">{pretty(output)}</pre>
    </details>
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

export function RunActions({ run, onDone }: { run: Run; onDone: () => void }) {
  const busy = useRef(false);
  const active = !["completed", "failed", "cancelled", "interrupted"].includes(run.status);
  return (
    <span className="run-actions">
      {active ? (
        <button
          className="danger sm"
          disabled={busy.current}
          onClick={async () => {
            busy.current = true;
            try {
              await api.cancelRun(run.id);
              onDone();
            } catch (e) {
              onDone();
              // surfaced by the caller's toast
              void e;
            }
          }}
        >
          Cancel
        </button>
      ) : null}
    </span>
  );
}
