// Run timeline: the execution log — every event rendered richly, bilingual.

import { useEffect, useRef, useState } from "react";
import type { Run } from "../api";
import { useI18n } from "../i18n";
import { Markdown, StatusPill, UsageMeter, fmtUsd } from "../ui";
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

export function RunTimeline({ run, live }: { run: Run; live: boolean }) {
  const { t } = useI18n();
  const [lines, setLines] = useState<EventLine[]>([]);
  const [ended, setEnded] = useState<string | null>(null);
  const [autoScroll, setAutoScroll] = useState(true);
  const bottomRef = useRef<HTMLDivElement>(null);
  const boxRef = useRef<HTMLDivElement>(null);

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
          <span className="live-flag">● {t("timeline.live")}</span>
        ) : (
          <span className="time">{dateOf(run.updated_at)}</span>
        )}
      </div>
      <div className="timeline" ref={boxRef} onScroll={onScroll}>
        {lines.length === 0 && !live ? <p className="muted pad">{t("timeline.noEvents")}</p> : null}
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
          {t("timeline.latest")}
        </button>
      )}
    </div>
  );
}

function EventRow({ line }: { line: EventLine }) {
  const { t } = useI18n();
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
          <span className="ev-icon"><Icon name="thought" size={13} /></span>
          <span className="thought-text">{text}</span>
        </div>
      );
    }
    case "tool_call":
      return (
        <ToolRow id={String(e.tool_call_id)} title={String(e.title)} input={e.raw_input} />
      );
    case "tool_call_update":
      return <ToolUpdateRow id={String(e.tool_call_id)} output={e.raw_output} />;
    case "plan": {
      const entries = e.entries as { content: string; status: string }[];
      const done = entries.filter((x) => x.status === "completed").length;
      return (
        <div className="ev plan">
          <span className="ev-icon"><Icon name="plan" size={13} /></span>
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
          <span className="ev-icon"><Icon name="compass" size={13} /></span>
          <span className="muted">{t("timeline.routed", { level })}{why}</span>
        </div>
      );
    }
    case "user_message": {
      // The original ask (injection rides separately as context_injected).
      const text = String(e.text ?? "");
      return (
        <details className="ev inject">
          <summary>
            <span className="ev-icon"><Icon name="user" size={13} /></span> {t("timeline.asked")}
          </summary>
          <pre className="raw">{text}</pre>
        </details>
      );
    }
    case "context_injected": {
      const render = String(e.render);
      const blocks = (render.match(/<(\w+)>/g) ?? []).map((b) => b.slice(1, -1));
      return (
        <details className="ev inject">
          <summary>
            <span className="ev-icon"><Icon name="brain" size={13} /></span> {t("timeline.injected")}
            <span className="muted">
              {" "}
              {t("timeline.injectedBlocks", { blocks: blocks.join(", "), n: render.length })}
            </span>
          </summary>
          <pre className="raw">{render}</pre>
        </details>
      );
    }
    case "permission_requested":
      return (
        <div className="ev perm">
          <span className="ev-icon"><Icon name="lock" size={13} /></span>
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
          <span className="ev-icon"><Icon name="unlock" size={13} /></span>
          <span>{t("timeline.permRes", { outcome, who: t(whoKey) })}</span>
        </div>
      );
    }
    case "state_changed":
      return (
        <div className="ev sys">
          <span className="ev-icon"><Icon name="play" size={13} /></span>
          <StatusPill status={String(e.status)} />
        </div>
      );
    case "stopped":
      return (
        <div className="ev sys">
          <span className="ev-icon"><Icon name="stop" size={13} /></span>
          <span className="muted">
            {t("timeline.stopped", { reason: t(`status.${String(e.stop_reason)}`) })}
          </span>
        </div>
      );
    case "error":
      return (
        <div className="ev err">
          <span className="ev-icon"><Icon name="error" size={13} /></span>
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

function ToolRow({ id, title, input }: { id: string; title: string; input: unknown }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="ev tool">
      <button className="tool-head" onClick={() => setOpen((v) => !v)}>
        <span className="ev-icon"><Icon name="tool" size={13} /></span>
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
    <details className="ev tool-update">
      <summary className="muted">{t("timeline.toolOutput", { id })}</summary>
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
