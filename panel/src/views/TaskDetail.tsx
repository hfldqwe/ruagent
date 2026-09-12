// Task detail: launcher (single/fanout/pipeline), run list, comparison,
// execution log, task status controls. Fully bilingual.

import { useEffect, useState } from "react";
import { api, type AgentInfo, type Run, type Task } from "../api";
import { useI18n } from "../i18n";
import {
  Markdown,
  Modal,
  RelTime,
  Spinner,
  StatusDot,
  StatusPill,
  fmtUsd,
  useToast,
} from "../ui";
import { RunTimeline } from "./RunTimeline";

type Mode = "single" | "fanout" | "pipeline";

export function TaskDetail({ id, onBack }: { id: string; onBack: () => void }) {
  const { t } = useI18n();
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
        setSelectedRun((prev) => prev ?? (r.runs[0]?.id ?? null));
        return r;
      })
      .catch(() => {});

  useEffect(() => {
    api.agents().then(setAgents).catch(() => {});
    refresh();
    const i = setInterval(refresh, 2000);
    return () => clearInterval(i);
  }, [id]);

  if (!task) return <Spinner label={`${t("board.title")}…`} />;
  const activeStatuses = ["queued", "spawning", "running", "waiting_permission"];
  const completed = runs.filter((r) => r.status === "completed");
  const totalCost = runs.reduce((s, r) => s + (r.cost_usd ?? 0), 0);
  const agentName = (r: Run) =>
    agents.find((a) => a.id === r.params.agent)?.name ?? r.params.agent.slice(0, 8);

  return (
    <div>
      <div className="view-bar">
        <button className="link" onClick={onBack}>
          ← {t("task.back")}
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
          <h3>{t("task.comparison")}</h3>
          <div className="compare">
            {runs.map((r) => (
              <div
                key={r.id}
                className={winner === r.id ? "card compare-card selected" : "card compare-card"}
              >
                <div className="row">
                  <StatusDot status={r.status} />
                  <strong>{agentName(r)}</strong>
                  {r.cost_usd != null ? <span className="muted">{fmtUsd(r.cost_usd)}</span> : null}
                  {winner === r.id ? <span className="tag ok">{t("task.winner")}</span> : null}
                </div>
                <div className="compare-result">
                  {r.result ? (
                    <Markdown>{r.result}</Markdown>
                  ) : (
                    <span className="muted">{t("task.noResult")}</span>
                  )}
                </div>
                <div className="row">
                  <button className="link" onClick={() => setSelectedRun(r.id)}>
                    {t("task.viewLog")}
                  </button>
                  {r.status === "completed" && winner !== r.id ? (
                    <button
                      className="primary sm"
                      onClick={async () => {
                        try {
                          await api.selectRun(r.id);
                          toast("ok", t("toast.winnerSelected"));
                          refresh();
                        } catch (e) {
                          toast("err", String(e));
                        }
                      }}
                    >
                      {t("task.selectWinner")}
                    </button>
                  ) : null}
                </div>
              </div>
            ))}
          </div>
        </>
      )}

      <h3>
        {t("task.runs")}{" "}
        <span className="muted">
          {t("task.runsCount", { n: runs.length })}
          {totalCost > 0 ? ` · ${t("task.totalCost", { cost: fmtUsd(totalCost) })}` : ""}
        </span>
      </h3>
      {runs.length === 0 ? (
        <p className="muted pad">{t("task.noRuns")}</p>
      ) : (
        <div className="card">
          {runs.map((r) => (
            <div
              key={r.id}
              role="button"
              tabIndex={0}
              aria-label={agentName(r)}
              className={selectedRun === r.id ? "row-btn selected" : "row-btn"}
              onClick={() => setSelectedRun(r.id)}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  setSelectedRun(r.id);
                }
              }}
            >
              <StatusDot status={r.status} />
              <span className="mono muted">{r.id.slice(0, 8)}</span>
              <span className="tag">{agentName(r)}</span>
              <StatusPill status={r.status} />
              {r.context_usage ? (
                <span className="muted">
                  {Math.round((r.context_usage.used / r.context_usage.size) * 100)}%
                </span>
              ) : null}
              {r.cost_usd != null ? <span className="muted">{fmtUsd(r.cost_usd)}</span> : null}
              <span className="grow" />
              <span className="time">
                <RelTime iso={r.created_at} />
              </span>
              {activeStatuses.includes(r.status) ? (
                <button
                  className="danger sm"
                  onClick={async (e) => {
                    e.stopPropagation();
                    try {
                      await api.cancelRun(r.id);
                      toast("ok", t("toast.cancelling"));
                    } catch (err) {
                      toast("err", String(err));
                    }
                    refresh();
                  }}
                >
                  {t("task.cancelRun")}
                </button>
              ) : null}
            </div>
          ))}
        </div>
      )}

      {selectedRun && runs.some((r) => r.id === selectedRun) ? (
        <>
          <h3>{t("task.log")}</h3>
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
  const { t } = useI18n();
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
        if (picked.length < 2) throw new Error(t("launcher.needs2"));
        const { runs } = await api.fanout(task.id, picked, p, repo.trim() || undefined);
        onLaunched(runs[0]?.id ?? null);
      } else {
        if (steps.length < 2 || steps.some((s) => !s)) throw new Error(t("launcher.needs2"));
        await api.pipeline(
          task.id,
          steps.map((s) => ({ agent: s })),
        );
        onLaunched(null);
      }
      setPrompt("");
      toast("ok", t("toast.launched"));
    } catch (e) {
      toast("err", String(e));
    } finally {
      setBusy(false);
    }
  };

  if (agents.length === 0) return <p className="muted pad">{t("launcher.noAgents")}</p>;

  return (
    <div className="card launcher">
      <div className="seg">
        <button className={mode === "single" ? "on" : ""} onClick={() => setMode("single")}>
          {t("launcher.run")}
        </button>
        <button className={mode === "fanout" ? "on" : ""} onClick={() => setMode("fanout")}>
          {t("launcher.fanout")}
        </button>
        <button className={mode === "pipeline" ? "on" : ""} onClick={() => setMode("pipeline")}>
          {t("launcher.pipeline")}
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
          <span className="muted">{t("launcher.selected", { n: picked.length })}</span>
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
                <option value="">{t("launcher.chooseAgent")}</option>
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
                title={t("launcher.removeStep")}
              >
                ✕
              </button>
            </div>
          ))}
          <button className="sm" onClick={() => setSteps([...steps, ""])} disabled={steps.length >= 5}>
            {t("launcher.addStep")}
          </button>
        </div>
      )}

      {mode !== "pipeline" && (
        <input
          className="grow"
          placeholder={t("launcher.prompt")}
          value={prompt}
          onChange={(e) => setPrompt(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && !busy && go()}
        />
      )}
      <input
        className="grow mono"
        placeholder={t("launcher.repo")}
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
            {t("launcher.startPipeline")}
          </button>
        ) : (
          <button
            className="primary"
            disabled={busy || (mode === "fanout" && picked.length < 2)}
            onClick={go}
          >
            {mode === "single" ? t("launcher.run") : `${t("launcher.fanout")} (${picked.length})`}
          </button>
        )}
      </div>
      <details className="hint">
        <summary className="muted">{t("launcher.hint.title")}</summary>
        <div className="muted">
          <Markdown>{t("launcher.hint.body")}</Markdown>
        </div>
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
  const { t } = useI18n();
  const [confirming, setConfirming] = useState(false);
  const toast = useToast();
  const set = async (status: string) => {
    try {
      await api.updateTaskStatus(task.id, status);
      onChanged();
    } catch (e) {
      toast("err", String(e));
    }
  };
  return (
    <span className="row tight">
      <select value={task.status} onChange={(e) => set(e.target.value)} title={t("task.status")}>
        {["pending", "in_progress", "blocked", "done", "cancelled"].map((s) => (
          <option key={s} value={s}>
            {t(`status.${s}`)}
          </option>
        ))}
      </select>
      <button className="danger sm" onClick={() => setConfirming(true)}>
        {t("common.delete")}
      </button>
      {confirming && (
        <Modal title={t("task.deleteConfirm.title")} onClose={() => setConfirming(false)}>
          <p>{t("task.deleteConfirm.body")}</p>
          <div className="row end">
            <button onClick={() => setConfirming(false)}>{t("common.keep")}</button>
            <button
              className="danger"
              onClick={async () => {
                try {
                  await api.deleteTask(task.id);
                  toast("ok", t("toast.taskDeleted"));
                  onDeleted();
                } catch (e) {
                  toast("err", String(e));
                }
              }}
            >
              {t("common.delete")}
            </button>
          </div>
        </Modal>
      )}
    </span>
  );
}
