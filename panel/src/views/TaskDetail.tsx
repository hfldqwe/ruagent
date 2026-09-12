// Task detail: launcher (single/fanout/pipeline), run list with costs,
// comparison with select, rich run timeline, task status controls.

import { useEffect, useState } from "react";
import { api, type AgentInfo, type Run, type Task } from "../api";
import { Markdown, Modal, Spinner, StatusDot, StatusPill, fmtUsd, relTime, useToast } from "../ui";
import { RunTimeline } from "./RunTimeline";

type Mode = "single" | "fanout" | "pipeline";

export function TaskDetail({ id, onBack }: { id: string; onBack: () => void }) {
  const [task, setTask] = useState<Task | null>(null);
  const [runs, setRuns] = useState<Run[]>([]);
  const [winner, setWinner] = useState<string | null>(null);
  const [selectedRun, setSelectedRun] = useState<string | null>(null);
  const [agents, setAgents] = useState<AgentInfo[]>([]);
  const [launching, setLaunching] = useState(false);
  const toast = useToast();

  const refresh = () =>
    api
      .task(id)
      .then((r) => {
        setTask(r.task);
        setRuns(r.runs);
        setWinner(r.selected_run_id);
        // Deep-link ergonomics: show the latest run's log without a click.
        setSelectedRun((prev) => prev ?? (r.runs[0]?.id ?? null));
        return r;
      })
      .catch(() => {});

  useEffect(() => {
    api.agents().then(setAgents).catch(() => {});
    refresh();
    const t = setInterval(refresh, 2000);
    return () => clearInterval(t);
  }, [id]);

  if (!task) return <Spinner label="loading task…" />;
  const activeStatuses = ["queued", "spawning", "running", "waiting_permission"];
  const completed = runs.filter((r) => r.status === "completed");
  const totalCost = runs.reduce((s, r) => s + (r.cost_usd ?? 0), 0);

  return (
    <div>
      <div className="view-bar">
        <button className="link" onClick={onBack}>
          ← board
        </button>
        <h2>{task.title}</h2>
        <StatusPill status={task.status} />
        {task.project ? <span className="tag">{task.project}</span> : null}
        <span className="grow" />
        <TaskStatusMenu task={task} onChanged={refresh} onDeleted={onBack} />
      </div>
      <p className="muted intent">{task.intent}</p>

      <Launcher
        task={task}
        agents={agents.filter((a) => a.enabled)}
        onLaunched={(runId) => {
          if (runId) setSelectedRun(runId);
          refresh();
        }}
        busy={launching}
        setBusy={setLaunching}
      />

      {runs.length > 1 && completed.length > 1 && (
        <>
          <h3>Comparison</h3>
          <div className="compare">
            {runs.map((r) => (
              <div
                key={r.id}
                className={winner === r.id ? "card compare-card selected" : "card compare-card"}
              >
                <div className="row">
                  <StatusDot status={r.status} />
                  <strong>
                    {agents.find((a) => a.id === r.params.agent)?.name ?? r.params.agent.slice(0, 8)}
                  </strong>
                  {r.cost_usd != null ? <span className="muted">{fmtUsd(r.cost_usd)}</span> : null}
                  {winner === r.id ? <span className="tag">winner</span> : null}
                </div>
                <div className="compare-result">
                  {r.result ? (
                    <Markdown>{r.result}</Markdown>
                  ) : (
                    <span className="muted">(no result)</span>
                  )}
                </div>
                <div className="row">
                  <button className="link" onClick={() => setSelectedRun(r.id)}>
                    view log
                  </button>
                  {r.status === "completed" && winner !== r.id ? (
                    <button
                      className="primary sm"
                      onClick={async () => {
                        try {
                          await api.selectRun(r.id);
                          toast("ok", "winner selected");
                          refresh();
                        } catch (e) {
                          toast("err", String(e));
                        }
                      }}
                    >
                      Select as winner
                    </button>
                  ) : null}
                </div>
              </div>
            ))}
          </div>
        </>
      )}

      <h3>
        Runs <span className="muted">({runs.length})</span>
        {totalCost > 0 ? <span className="muted"> · {fmtUsd(totalCost)} total</span> : null}
      </h3>
      {runs.length === 0 ? (
        <p className="muted pad">No runs yet — launch one above.</p>
      ) : (
        <div className="card">
          {runs.map((r) => (
            <div
              key={r.id}
              className={selectedRun === r.id ? "row clickable selected" : "row clickable"}
              onClick={() => setSelectedRun(r.id)}
            >
              <StatusDot status={r.status} />
              <span className="mono muted">{r.id.slice(0, 8)}</span>
              <span className="tag">
                {agents.find((a) => a.id === r.params.agent)?.name ?? r.params.agent.slice(0, 8)}
              </span>
              <StatusPill status={r.status} />
              {r.context_usage ? (
                <span className="muted">
                  {Math.round((r.context_usage.used / r.context_usage.size) * 100)}% ctx
                </span>
              ) : null}
              {r.cost_usd != null ? <span className="muted">{fmtUsd(r.cost_usd)}</span> : null}
              {r.stop_reason ? (
                <span className="muted">{r.stop_reason.replaceAll("_", " ")}</span>
              ) : null}
              <span className="grow" />
              <span className="muted time">{relTime(r.created_at)}</span>
              {activeStatuses.includes(r.status) ? (
                <button
                  className="danger sm"
                  onClick={async (e) => {
                    e.stopPropagation();
                    try {
                      await api.cancelRun(r.id);
                      toast("ok", "cancelling…");
                    } catch (err) {
                      toast("err", String(err));
                    }
                    refresh();
                  }}
                >
                  cancel
                </button>
              ) : null}
            </div>
          ))}
        </div>
      )}

      {selectedRun && runs.some((r) => r.id === selectedRun) ? (
        <>
          <h3>Execution log</h3>
          <RunTimeline
            key={selectedRun}
            run={runs.find((r) => r.id === selectedRun)!}
            live={activeStatuses.includes(runs.find((r) => r.id === selectedRun)!.status)}
          />
        </>
      ) : null}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Launcher
// ---------------------------------------------------------------------------

function Launcher({
  task,
  agents,
  onLaunched,
  busy,
  setBusy,
}: {
  task: Task;
  agents: AgentInfo[];
  onLaunched: (runId: string | null) => void;
  busy: boolean;
  setBusy: (b: boolean) => void;
}) {
  const [mode, setMode] = useState<Mode>("single");
  const [agent, setAgent] = useState(agents[0]?.name ?? "");
  const [prompt, setPrompt] = useState("");
  const [picked, setPicked] = useState<string[]>([]);
  const [steps, setSteps] = useState<string[]>([]);
  const [repo, setRepo] = useState("");
  const toast = useToast();

  useEffect(() => {
    if (agents.length && !agents.some((a) => a.name === agent)) setAgent(agents[0].name);
  }, [agents, agent]);

  const toggle = (name: string) =>
    setPicked((list) => (list.includes(name) ? list.filter((n) => n !== name) : [...list, name]));

  const go = async () => {
    setBusy(true);
    try {
      const p = prompt.trim() || task.intent;
      if (mode === "single") {
        const run = await api.startRun(task.id, agent, p, repo.trim() || undefined);
        onLaunched(run.id);
      } else if (mode === "fanout") {
        if (picked.length < 2) throw new Error("fan-out needs at least 2 agents");
        const { runs } = await api.fanout(task.id, picked, p, repo.trim() || undefined);
        onLaunched(runs[0]?.id ?? null);
      } else {
        if (steps.length < 2) throw new Error("pipeline needs at least 2 steps");
        await api.pipeline(
          task.id,
          steps.map((s) => ({ agent: s })),
        );
        onLaunched(null);
      }
      setPrompt("");
      toast("ok", `${mode} launched`);
    } catch (e) {
      toast("err", String(e));
    } finally {
      setBusy(false);
    }
  };

  if (agents.length === 0)
    return <p className="muted pad">No enabled agents — edit ~/.ruagent/config/agents.toml.</p>;

  return (
    <div className="card launcher">
      <div className="seg">
        <button className={mode === "single" ? "on" : ""} onClick={() => setMode("single")}>
          Run
        </button>
        <button className={mode === "fanout" ? "on" : ""} onClick={() => setMode("fanout")}>
          Fan out
        </button>
        <button className={mode === "pipeline" ? "on" : ""} onClick={() => setMode("pipeline")}>
          Pipeline
        </button>
      </div>

      {mode === "single" && (
        <div className="row">
          <select value={agent} onChange={(e) => setAgent(e.target.value)}>
            {agents.map((a) => (
              <option key={a.name} value={a.name}>
                {a.name}
              </option>
            ))}
          </select>
        </div>
      )}
      {mode === "fanout" && (
        <div className="row wrap">
          {agents.map((a) => (
            <label key={a.name} className="check">
              <input type="checkbox" checked={picked.includes(a.name)} onChange={() => toggle(a.name)} />
              {a.name}
            </label>
          ))}
          <span className="muted">{picked.length} selected</span>
        </div>
      )}
      {mode === "pipeline" && (
        <div className="pipeline-builder">
          {steps.map((s, i) => (
            <div key={i} className="row">
              <span className="step-n">{i + 1}</span>
              <select
                value={s}
                onChange={(e) => setSteps(steps.map((x, j) => (j === i ? e.target.value : x)))}
              >
                <option value="">choose agent…</option>
                {agents.map((a) => (
                  <option key={a.name} value={a.name}>
                    {a.name}
                  </option>
                ))}
              </select>
              <span className="muted">→</span>
              <button
                className="sm"
                onClick={() => setSteps(steps.filter((_, j) => j !== i))}
                title="remove step"
              >
                ✕
              </button>
            </div>
          ))}
          <button className="sm" onClick={() => setSteps([...steps, ""])} disabled={steps.length >= 5}>
            + step
          </button>
        </div>
      )}

      {mode !== "pipeline" && (
        <input
          className="grow"
          placeholder="Prompt (defaults to the task intent)"
          value={prompt}
          onChange={(e) => setPrompt(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && !busy && go()}
        />
      )}
      <input
        className="grow mono"
        placeholder="git repo for worktree isolation (optional, e.g. C:/src/myproject)"
        value={repo}
        onChange={(e) => setRepo(e.target.value)}
      />
      <div className="row end">
        {mode === "pipeline" ? (
          <button
            className="primary"
            disabled={busy || steps.some((s) => !s) || steps.length < 2}
            onClick={go}
          >
            Start pipeline
          </button>
        ) : (
          <button
            className="primary"
            disabled={busy || (mode === "fanout" && picked.length < 2)}
            onClick={go}
          >
            {mode === "single" ? "Run" : `Fan out (${picked.length})`}
          </button>
        )}
      </div>
      <details className="hint">
        <summary className="muted">how do these differ?</summary>
        <p className="muted">
          <strong>Run</strong> sends the prompt to one agent. <strong>Fan out</strong> sends it to
          several in parallel (each in its own worktree when a repo is given) and shows results
          side by side for you to pick a winner. <strong>Pipeline</strong> chains agents — each
          step receives the previous step's result as handoff context.
        </p>
      </details>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Task status menu
// ---------------------------------------------------------------------------

function TaskStatusMenu({
  task,
  onChanged,
  onDeleted,
}: {
  task: Task;
  onChanged: () => void;
  onDeleted: () => void;
}) {
  const [confirming, setConfirming] = useState(false);
  const toast = useToast();
  const set = async (status: string) => {
    try {
      await api.updateTaskStatus(task.id, status);
      toast("ok", `task → ${status}`);
      onChanged();
    } catch (e) {
      toast("err", String(e));
    }
  };
  return (
    <span className="row tight">
      <select value={task.status} onChange={(e) => set(e.target.value)} title="change task status">
        {["pending", "in_progress", "blocked", "done", "cancelled"].map((s) => (
          <option key={s} value={s}>
            {s.replaceAll("_", " ")}
          </option>
        ))}
      </select>
      <button className="danger sm" onClick={() => setConfirming(true)}>
        delete
      </button>
      {confirming && (
        <Modal title="Delete task?" onClose={() => setConfirming(false)}>
          <p>
            Deletes the task and its run records. Transcript files stay on disk as evidence. This
            cannot be undone.
          </p>
          <div className="row end">
            <button onClick={() => setConfirming(false)}>Keep</button>
            <button
              className="danger"
              onClick={async () => {
                try {
                  await api.deleteTask(task.id);
                  toast("ok", "task deleted");
                  onDeleted();
                } catch (e) {
                  toast("err", String(e));
                }
              }}
            >
              Delete
            </button>
          </div>
        </Modal>
      )}
    </span>
  );
}
