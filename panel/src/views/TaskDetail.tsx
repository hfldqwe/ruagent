// Task detail: launcher (single/fanout/pipeline), run list, comparison,
// execution log, task status controls. Fully bilingual.

import { useEffect, useState } from "react";
import { Button, Checkbox, Input, Popconfirm, Segmented, Select, Tooltip } from "antd";
import { Icon } from "../icons";
import {
  api,
  type AgentInfo,
  type Judgement,
  type Run,
  type SessionOptionInfo,
  type Task,
} from "../api";
import { useI18n } from "../i18n";
import {
  Markdown,
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
  const [selectionBy, setSelectionBy] = useState<string | null>(null);
  const [judgement, setJudgement] = useState<Judgement | null>(null);
  const [judgeAgent, setJudgeAgent] = useState("");
  const [judging, setJudging] = useState(false);
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
        setSelectionBy(r.selected_by);
        setJudgement(r.judgement);
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
        <Button type="link" onClick={onBack} style={{ paddingLeft: 0 }}>
          ← {t("task.back")}
        </Button>
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
          <h3 className="sec">{t("task.comparison")}</h3>
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
                  {winner === r.id ? (
                    <span className="tag ok">
                      {t("task.winner")}
                      {selectionBy?.startsWith("agent:") ? ` · ${selectionBy.slice(6)}` : ""}
                    </span>
                  ) : null}
                </div>
                <div className="compare-result">
                  {r.result ? (
                    <Markdown>{r.result}</Markdown>
                  ) : (
                    <span className="muted">{t("task.noResult")}</span>
                  )}
                </div>
                <div className="row">
                  <Button type="link" size="small" style={{ paddingLeft: 0 }} onClick={() => setSelectedRun(r.id)}>
                    {t("task.viewLog")}
                  </Button>
                  {r.status === "completed" && winner !== r.id ? (
                    <Button
                      type="primary"
                      size="small"
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
                    </Button>
                  ) : null}
                </div>
              </div>
            ))}
          </div>
          <JudgeBar
            task={task}
            agents={agents.filter((a) => a.enabled)}
            runs={runs}
            judgement={judgement}
            judgeAgent={judgeAgent}
            setJudgeAgent={setJudgeAgent}
            judging={judging}
            setJudging={setJudging}
            onStarted={refresh}
            agentName={agentName}
          />
        </>
      )}

      <h3 className="sec">
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
              {r.waiting_permission ? (
                <Tooltip title={r.waiting_permission.title}>
                  <span className="tag warn">
                    <Icon name="lock" size={11} /> {t("task.waitingPerm")}
                  </span>
                </Tooltip>
              ) : null}
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
              {r.waiting_permission ? (
                <Button
                  type="primary"
                  size="small"
                  onClick={async (e) => {
                    e.stopPropagation();
                    try {
                      await api.resolvePermission(
                        r.id,
                        r.waiting_permission!.tool_call_id,
                        { action: "allow" },
                      );
                      toast("ok", t("inbox.allowed"));
                    } catch (err) {
                      toast("err", String(err));
                    }
                    refresh();
                  }}
                >
                  {t("inbox.allow")}
                </Button>
              ) : null}
              {activeStatuses.includes(r.status) ? (
                <Button
                  danger
                  size="small"
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
                </Button>
              ) : null}
            </div>
          ))}
        </div>
      )}

      {selectedRun && runs.some((r) => r.id === selectedRun) ? (
        <>
          <h3 className="sec">{t("task.log")}</h3>
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
  // Canonical run options (issue #36): permission mode / thinking
  // effort, applied after session/new. The choices come from the
  // selected agent's runtime catalog (cached daemon-side).
  const [optMode, setOptMode] = useState("");
  const [optEffort, setOptEffort] = useState("");
  const [catalog, setCatalog] = useState<SessionOptionInfo[] | null>(null);
  const optAgent = mode === "fanout" ? (picked[0] ?? "") : agent;
  useEffect(() => {
    if (!optAgent) {
      setCatalog(null);
      setOptMode("");
      setOptEffort("");
      return;
    }
    let alive = true;
    setCatalog(null);
    setOptMode("");
    setOptEffort("");
    api
      .agentOptions(optAgent)
      .then((r) => alive && setCatalog(r.options))
      .catch(() => alive && setCatalog([]));
    return () => {
      alive = false;
    };
  }, [optAgent]);
  const modeChoices =
    (catalog ?? []).find((o) => o.category === "mode" || o.id === "mode")?.choices ?? [];
  const effortChoices =
    (catalog ?? []).find(
      (o) =>
        o.category === "thought_level" ||
        o.id === "effort" ||
        o.id === "reasoning_effort",
    )?.choices ?? [];

  useEffect(() => {
    if (agents.length && !agents.some((a) => a.name === agent)) setAgent(agents[0].name);
  }, [agents, agent]);

  const go = async () => {
    setBusy(true);
    try {
      const p = prompt.trim() || task.intent;
      const runOptions: Record<string, string> = {};
      if (optMode) runOptions.mode = optMode;
      if (optEffort) runOptions.effort = optEffort;
      if (mode === "single") {
        const run = await api.startRun(task.id, agent, p, repo.trim() || undefined, runOptions);
        onLaunched(run.id);
      } else if (mode === "fanout") {
        if (picked.length < 2) throw new Error(t("launcher.needs2"));
        const { runs } = await api.fanout(
          task.id,
          picked,
          p,
          repo.trim() || undefined,
          runOptions,
        );
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
      <Segmented
        value={mode}
        onChange={(v) => setMode(v as Mode)}
        options={[
          { value: "single", label: t("launcher.run") },
          { value: "fanout", label: t("launcher.fanout") },
          { value: "pipeline", label: t("launcher.pipeline") },
        ]}
        style={{ marginBottom: 10 }}
      />

      {mode === "single" && (
        <div className="row">
          <Select
            value={agent}
            onChange={setAgent}
            style={{ minWidth: 180 }}
            options={agents.map((a) => ({ value: a.name, label: a.name }))}
          />
        </div>
      )}
      {mode === "fanout" && (
        <div className="row wrap">
          {agents.map((a) => (
            <Checkbox
              key={a.name}
              checked={picked.includes(a.name)}
              onChange={() =>
                setPicked((list) =>
                  list.includes(a.name) ? list.filter((n) => n !== a.name) : [...list, a.name],
                )
              }
            >
              {a.name}
            </Checkbox>
          ))}
          <span className="muted">{t("launcher.selected", { n: picked.length })}</span>
        </div>
      )}
      {mode !== "pipeline" && (modeChoices.length > 0 || effortChoices.length > 0) ? (
        // Canonical run options (issue #36) — the displayed choices are
        // the selected agent's catalog; on mixed fan-out members the
        // daemon applies each value best-effort.
        <div className="row" style={{ marginBottom: 10 }}>
          {modeChoices.length > 0 ? (
            <Select
              allowClear
              value={optMode || undefined}
              onChange={(v: string) => setOptMode(v ?? "")}
              placeholder={t("agents.f.mode")}
              style={{ minWidth: 170 }}
              options={modeChoices.map((c) => ({ value: c.value, label: c.name }))}
            />
          ) : null}
          {effortChoices.length > 0 ? (
            <Select
              allowClear
              value={optEffort || undefined}
              onChange={(v: string) => setOptEffort(v ?? "")}
              placeholder={t("agents.f.effort")}
              style={{ minWidth: 150 }}
              options={effortChoices.map((c) => ({ value: c.value, label: c.name }))}
            />
          ) : null}
        </div>
      ) : null}
      {mode === "pipeline" && (
        <div className="pipeline-builder">
          {steps.map((s, i) => (
            <div key={i} className="row">
              <span className="step-n">{i + 1}</span>
              <Select
                value={s || undefined}
                placeholder={t("launcher.chooseAgent")}
                style={{ minWidth: 180 }}
                onChange={(v) => setSteps(steps.map((x, j) => (j === i ? v : x)))}
                options={agents.map((a) => ({ value: a.name, label: a.name }))}
              />
              <span className="muted">→</span>
              <Button
                size="small"
                icon={<Icon name="x" size={14} />}
                onClick={() => setSteps(steps.filter((_, j) => j !== i))}
                title={t("launcher.removeStep")}
              />
            </div>
          ))}
          <Button size="small" onClick={() => setSteps([...steps, ""])} disabled={steps.length >= 5}>
            {t("launcher.addStep")}
          </Button>
        </div>
      )}

      {mode !== "pipeline" && (
        <Input
          className="grow"
          placeholder={t("launcher.prompt")}
          value={prompt}
          onChange={(e) => setPrompt(e.target.value)}
          onPressEnter={() => !busy && go()}
        />
      )}
      <Input
        className="grow mono"
        placeholder={t("launcher.repo")}
        value={repo}
        onChange={(e) => setRepo(e.target.value)}
        style={{ marginTop: 8 }}
      />
      <div className="row end">
        {mode === "pipeline" ? (
          <Button
            type="primary"
            loading={busy}
            disabled={steps.some((s) => !s) || steps.length < 2}
            onClick={go}
          >
            {t("launcher.startPipeline")}
          </Button>
        ) : (
          <Button
            type="primary"
            loading={busy}
            disabled={mode === "fanout" && picked.length < 2}
            onClick={go}
          >
            {mode === "single" ? t("launcher.run") : `${t("launcher.fanout")} (${picked.length})`}
          </Button>
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
// Judge bar (design §5.2/§5.3): launch an AI judge over the fan-out
// results, then show its verdict. The human pick always overrides.
// ---------------------------------------------------------------------------

const ACTIVE_STATUSES = ["queued", "spawning", "running", "waiting_permission"];

function JudgeBar({
  task,
  agents,
  runs,
  judgement,
  judgeAgent,
  setJudgeAgent,
  judging,
  setJudging,
  onStarted,
  agentName,
}: {
  task: Task;
  agents: AgentInfo[];
  runs: Run[];
  judgement: Judgement | null;
  judgeAgent: string;
  setJudgeAgent: (a: string) => void;
  judging: boolean;
  setJudging: (b: boolean) => void;
  onStarted: () => void;
  agentName: (r: Run) => string;
}) {
  const { t } = useI18n();
  const toast = useToast();

  useEffect(() => {
    if (agents.length && !agents.some((a) => a.name === judgeAgent)) setJudgeAgent(agents[0].name);
  }, [agents, judgeAgent]);

  const completedWithResult = runs.filter(
    (r) => r.status === "completed" && (r.result ?? "").trim().length > 0,
  );
  const live = !!judgement && ACTIVE_STATUSES.includes(judgement.judge_run_status);
  const canJudge = completedWithResult.length >= 2 && !live && !judging;

  const go = async () => {
    if (!judgeAgent) return;
    setJudging(true);
    try {
      await api.judgeTask(task.id, judgeAgent);
      toast("ok", t("toast.judgeStarted"));
      onStarted();
    } catch (e) {
      toast("err", String(e));
    } finally {
      setJudging(false);
    }
  };

  const verdictTag = () => {
    if (!judgement) return null;
    if (live) return <span className="tag">{t("task.judging")}</span>;
    if (judgement.judge_run_status === "completed") {
      if (judgement.winner_run_id) {
        const run = runs.find((r) => r.id === judgement.winner_run_id);
        return (
          <span className="tag ok">
            {t("task.judgePicked")}
            {run ? `: ${agentName(run)}` : ""}
          </span>
        );
      }
      return <span className="tag warn">{t("task.judgeNoVerdict")}</span>;
    }
    return <span className="tag err">{t("task.judgeFailed")}</span>;
  };

  return (
    <div className="card judge-bar" style={{ marginTop: 10 }}>
      <div className="row">
        <span className="doc-icon">
          <Icon name="thought" size={15} />
        </span>
        <strong>{t("task.judge")}</strong>
        <Select
          size="small"
          value={judgeAgent || undefined}
          onChange={setJudgeAgent}
          placeholder={t("task.judgeAgent")}
          style={{ minWidth: 150 }}
          options={agents.map((a) => ({ value: a.name, label: a.name }))}
        />
        <Button size="small" type="primary" disabled={!canJudge} loading={judging} onClick={go}>
          {t("task.judgeGo")}
        </Button>
        <span className="grow" />
        {verdictTag()}
      </div>
      {judgement?.rationale && (
        <p className="muted" style={{ margin: "8px 0 0" }}>{judgement.rationale}</p>
      )}
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
      <Select
        value={task.status}
        onChange={set}
        style={{ minWidth: 120 }}
        title={t("task.status")}
        options={["pending", "in_progress", "blocked", "done", "cancelled"].map((s) => ({
          value: s,
          label: t(`status.${s}`),
        }))}
      />
      <Popconfirm
        title={t("task.deleteConfirm.title")}
        description={t("task.deleteConfirm.body")}
        okText={t("common.delete")}
        cancelText={t("common.keep")}
        okButtonProps={{ danger: true }}
        onConfirm={async () => {
          try {
            await api.deleteTask(task.id);
            toast("ok", t("toast.taskDeleted"));
            onDeleted();
          } catch (e) {
            toast("err", String(e));
          }
        }}
      >
        <Button danger size="small">
          {t("common.delete")}
        </Button>
      </Popconfirm>
    </span>
  );
}
