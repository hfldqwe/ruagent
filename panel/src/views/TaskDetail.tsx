import { useEffect, useRef, useState } from "react";
import { api, type AgentInfo, type Run, type Task } from "../api";
import { StatusDot } from "../App";

interface TranscriptLine {
  ts: string;
  seq: number;
  event: {
    type: string;
    [k: string]: unknown;
  };
}

export function TaskDetail({ id, onBack }: { id: string; onBack: () => void }) {
  const [task, setTask] = useState<Task | null>(null);
  const [runs, setRuns] = useState<Run[]>([]);
  const [agents, setAgents] = useState<AgentInfo[]>([]);
  const [agent, setAgent] = useState<string>("");
  const [prompt, setPrompt] = useState("");
  const [streamRun, setStreamRun] = useState<string | null>(null);
  const [selectedRun, setSelectedRun] = useState<string | null>(null);
  const [fanoutOpen, setFanoutOpen] = useState(false);
  const [fanoutAgents, setFanoutAgents] = useState<string[]>([]);
  const [lines, setLines] = useState<TranscriptLine[]>([]);
  const [liveStatus, setLiveStatus] = useState<string | null>(null);
  const bottomRef = useRef<HTMLDivElement>(null);

  const refresh = () => {
    api
      .task(id)
      .then((r) => {
        setTask(r.task);
        setRuns(r.runs);
        setSelectedRun((r as { selected_run_id?: string | null }).selected_run_id ?? null);
      })
      .catch(() => {});
  };

  useEffect(() => {
    api.agents().then((a) => {
      setAgents(a);
      const first = a.find((x) => x.enabled);
      if (first) setAgent(first.name);
    });
    refresh();
    const t = setInterval(refresh, 2000);
    return () => clearInterval(t);
  }, [id]);

  // Live event stream for the selected run (SSE with replay).
  useEffect(() => {
    if (!streamRun) return;
    setLines([]);
    setLiveStatus(null);
    const es = new EventSource(`/api/v1/runs/${streamRun}/events`);
    es.onmessage = (e) => {
      try {
        setLines((prev) => [...prev, JSON.parse(e.data) as TranscriptLine]);
      } catch {
        /* malformed line */
      }
    };
    es.addEventListener("end", (e) => {
      try {
        setLiveStatus((JSON.parse((e as MessageEvent).data) as { status: string }).status);
      } catch {
        setLiveStatus("ended");
      }
      es.close();
    });
    es.onerror = () => {
      /* keep-alive or shutdown; end event closes explicitly */
    };
    return () => es.close();
  }, [streamRun]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ block: "end" });
  }, [lines]);

  const start = async () => {
    if (!prompt.trim()) return;
    const run = await api.startRun(id, agent || null, prompt.trim());
    setStreamRun(run.id);
    setPrompt("");
    refresh();
  };

  return (
    <div>
      <button className="link" onClick={onBack}>
        ← tasks
      </button>
      {task && (
        <div className="card">
          <div className="row">
            <StatusDot status={task.status} />
            <strong>{task.title}</strong>
            <span className="muted">{task.status}</span>
          </div>
          <p className="muted">{task.intent}</p>
          <div className="row">
            <select value={agent} onChange={(e) => setAgent(e.target.value)}>
              {agents.map((a) => (
                <option key={a.name} value={a.name} disabled={!a.enabled}>
                  {a.name}
                </option>
              ))}
            </select>
            <input
              className="grow"
              placeholder="Prompt (defaults to the task intent)"
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && start()}
            />
            <button className="primary" onClick={start}>
              Run
            </button>
            <button onClick={() => setFanoutOpen((v) => !v)}>Fan out…</button>
          </div>
          {fanoutOpen && (
            <div className="fanout">
              <p className="muted">
                Same prompt to several agents in parallel; compare the results and pick one
                (design &sect;5.2).
              </p>
              <div className="row">
                {agents
                  .filter((a) => a.enabled)
                  .map((a) => (
                    <label key={a.name} className="check">
                      <input
                        type="checkbox"
                        checked={fanoutAgents.includes(a.name)}
                        onChange={(e) =>
                          setFanoutAgents((prev) =>
                            e.target.checked
                              ? [...prev, a.name]
                              : prev.filter((n) => n !== a.name),
                          )
                        }
                      />
                      {a.name}
                    </label>
                  ))}
                <button
                  className="primary"
                  disabled={fanoutAgents.length < 2}
                  onClick={async () => {
                    const { runs } = await api.fanout(
                      id,
                      fanoutAgents,
                      prompt.trim() || task!.intent,
                    );
                    setFanoutOpen(false);
                    setFanoutAgents([]);
                    if (runs[0]) setStreamRun(runs[0].id);
                    refresh();
                  }}
                >
                  Fan out ({fanoutAgents.length})
                </button>
              </div>
            </div>
          )}
        </div>
      )}

      {runs.length > 1 && (
        <>
          <h3>Comparison</h3>
          <div className="compare">
            {runs.map((r) => (
              <div
                key={r.id}
                className={selectedRun === r.id ? "card compare-card selected" : "card compare-card"}
              >
                <div className="row">
                  <StatusDot status={r.status} />
                  <strong>{agentName(agents, r)}</strong>
                  {r.cost_usd != null && <span className="muted">${r.cost_usd.toFixed(4)}</span>}
                  {selectedRun === r.id && <span className="tag">selected</span>}
                </div>
                <pre className="raw">
                  {r.result ?? "(no result yet)"}
                </pre>
                <div className="row">
                  <button className="link" onClick={() => setStreamRun(r.id)}>
                    view stream
                  </button>
                  {r.status === "completed" && selectedRun !== r.id && (
                    <button
                      className="primary"
                      onClick={async () => {
                        await api.selectRun(r.id);
                        refresh();
                      }}
                    >
                      Select
                    </button>
                  )}
                </div>
              </div>
            ))}
          </div>
        </>
      )}

      <h3>Runs</h3>
      {runs.length === 0 && <p className="muted">No runs yet.</p>}
      {runs.map((r) => (
        <div
          key={r.id}
          className={streamRun === r.id ? "row clickable selected" : "row clickable"}
          onClick={() => setStreamRun(r.id)}
        >
          <StatusDot status={r.status} />
          <span className="muted">{r.id.slice(0, 13)}…</span>
          <span className="muted">{r.status}</span>
          {r.context_usage && (
            <span className="muted">
              {fmtTokens(r.context_usage.used)}/{fmtTokens(r.context_usage.size)} ctx
            </span>
          )}
          {r.cost_usd != null && <span className="muted">${r.cost_usd.toFixed(4)}</span>}
          <span className="muted time">{new Date(r.created_at).toLocaleTimeString()}</span>
        </div>
      ))}

      {streamRun && (
        <>
          <h3>
            Event stream
            {liveStatus && <span className="tag"> {liveStatus}</span>}
          </h3>
          <div className="stream">
            {lines.map((l) => (
              <EventLine key={l.seq} line={l} />
            ))}
            <div ref={bottomRef} />
          </div>
        </>
      )}
    </div>
  );
}

function agentName(agents: AgentInfo[], run: Run): string {
  // Runs store agent ids; the compare view wants names.
  return agents.find((a) => a.id === run.params.agent)?.name ?? run.params.agent.slice(0, 8);
}

function fmtTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return String(n);
}

function EventLine({ line }: { line: TranscriptLine }) {
  const e = line.event;
  const time = new Date(line.ts).toLocaleTimeString();
  switch (e.type) {
    case "agent_message_chunk": {
      const content = e.content as { text?: string }[];
      return (
        <div className="ev msg">
          <span className="muted time">{time}</span>
          <span>{content.map((c) => c.text ?? "").join("")}</span>
        </div>
      );
    }
    case "tool_call":
      return (
        <div className="ev tool">
          <span className="muted time">{time}</span>
          <span>
            🔧 {String(e.title)}{" "}
            <code>{JSON.stringify(e.raw_input).slice(0, 120)}</code>
          </span>
        </div>
      );
    case "usage_update": {
      const u = e.usage as { used: number; size: number; cost_usd: number | null };
      return (
        <div className="ev sys">
          <span className="muted time">{time}</span>
          <span className="muted">
            usage {fmtTokens(u.used)}/{fmtTokens(u.size)}
            {u.cost_usd != null ? ` · $${u.cost_usd.toFixed(4)}` : ""}
          </span>
        </div>
      );
    }
    case "permission_requested":
      return (
        <div className="ev perm">
          <span className="muted time">{time}</span>
          <span>🔐 permission requested: {String(e.title)}</span>
        </div>
      );
    case "permission_resolved": {
      const res = e.resolution as { source: string };
      return (
        <div className="ev perm">
          <span className="muted time">{time}</span>
          <span className="muted">
            🔓 resolved ({String(e.outcome)}) by {res.source}
          </span>
        </div>
      );
    }
    case "state_changed":
      return (
        <div className="ev sys">
          <span className="muted time">{time}</span>
          <span className="muted">→ {String(e.status)}</span>
        </div>
      );
    case "stopped":
      return (
        <div className="ev sys">
          <span className="muted time">{time}</span>
          <span className="muted">■ stopped: {String(e.stop_reason)}</span>
        </div>
      );
    case "error":
      return (
        <div className="ev err">
          <span className="muted time">{time}</span>
          <span>✖ {String(e.message)}</span>
        </div>
      );
    default:
      return (
        <div className="ev sys">
          <span className="muted time">{time}</span>
          <span className="muted">{e.type}</span>
        </div>
      );
  }
}
