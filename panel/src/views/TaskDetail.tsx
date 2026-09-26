// Task detail: launcher (single/fanout/pipeline), run list, comparison,
// execution log, task status controls. Fully bilingual.
//
// view-task-detail.md is the P0 page of the panel: the *structure* problems
// (26k event rows, 152k DOM nodes) live in RunTimeline.tsx and are owned by
// the chat/timeline task; what this file owes the spec is the shell around
// it — a real error state instead of a permanent spinner (D6 / MASTER 行 20),
// a hierarchy centre that is not decoration (D8), a visible Run.error (D16)
// and a confirmed cancel (D17).

import { useEffect, useState } from "react";
import { Button, Checkbox, Input, Popconfirm, Segmented, Select } from "antd";
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
  ErrorState,
  RelTime,
  Spinner,
  StatusDot,
  StatusPill,
  Zone,
  fmtUsd,
  useToast,
} from "../ui";
import { Markdown } from "./lazy-markdown";
import { RunTimeline } from "./RunTimeline";

type Mode = "single" | "fanout" | "pipeline";

const ACTIVE_STATUSES = ["queued", "spawning", "running", "waiting_permission"];

/** The newest run by created_at. `runs[]` arrives oldest-first, so this is
 *  deliberately NOT runs[0] — see the default-selection note in `refresh`. */
function newestRunId(runs: Run[]): string | null {
  let best: Run | null = null;
  for (const r of runs) if (!best || r.created_at > best.created_at) best = r;
  return best?.id ?? null;
}

export function TaskDetail({ id, onBack }: { id: string; onBack: () => void }) {
  const { t } = useI18n();
  const [task, setTask] = useState<Task | null>(null);
  const [runs, setRuns] = useState<Run[]>([]);
  const [winner, setWinner] = useState<string | null>(null);
  const [selectionBy, setSelectionBy] = useState<string | null>(null);
  const [judgement, setJudgement] = useState<Judgement | null>(null);
  const [judgeAgent, setJudgeAgent] = useState("");
  const [judging, setJudging] = useState(false);
  const [retrying, setRetrying] = useState<string | null>(null);
  const [landable, setLandable] = useState(false);
  const [landing, setLanding] = useState(false);
  const [selectedRun, setSelectedRun] = useState<string | null>(null);
  const [agents, setAgents] = useState<AgentInfo[]>([]);
  const [launching, setLaunching] = useState(false);
  const [err, setErr] = useState<unknown>(null);
  const toast = useToast();

  // A swallowed error used to leave `task === null` forever (a permanent
  // spinner that reads as "still loading"). The failure is kept instead.
  const refresh = () =>
    api
      .task(id)
      .then((r) => {
        setTask(r.task);
        setRuns(r.runs);
        setWinner(r.selected_run_id);
        setSelectionBy(r.selected_by);
        setJudgement(r.judgement);
        setLandable(r.landable);
        // Default selection: the explicit choice, else the NEWEST run —
        // never runs[0]. runs[] is oldest-first, so runs[0] is the OLDEST run:
        // for the task in the 2026-09-22 report (status done, runs = [failed
        // 09-12, completed 09-22]) the page opened on a stale ACP failure, and
        // the 580px execution log — the page's dominant block — read 「失败」
        // while the header read 「已完成」. That is the "a failed run means the
        // task failed" misreading, and it is real (measured on
        // 01a095f6-0aa1…). The two facts are now separate: the header is the
        // TASK, the run list and the log are RUNS.
        setSelectedRun((prev) => prev ?? r.selected_run_id ?? newestRunId(r.runs));
        setErr(null);
        return r;
      })
      .catch((e) => {
        setErr(e);
        return null;
      });

  useEffect(() => {
    api.agents().then(setAgents).catch(() => {});
    refresh();
    const i = setInterval(refresh, 2000);
    return () => clearInterval(i);
  }, [id]);

  if (!task) {
    if (err) {
      return (
        <>
          <h1 className="sr-only micro">{t("task.err")}</h1>
          <ErrorState
            title={t("task.err")}
            hint={t("task.err.hint")}
            onRetry={refresh}
            retryLabel={t("common.retry")}
          />
          <div className="row end">
            <Button type="link" onClick={onBack}>
              ← {t("task.back")}
            </Button>
          </div>
        </>
      );
    }
    return <Spinner label={t("task.loading")} />;
  }

  const stale = !!err;
  const totalCost = runs.reduce((s, r) => s + (r.cost_usd ?? 0), 0);
  /** Runs that ended in failure. The task's own status is a different fact:
   *  a task can be 「已完成」 while this is non-zero, and the page must say so
   *  rather than let one failed run speak for the whole task. */
  const failedRuns = runs.filter((r) => r.status === "failed").length;
  const selected = runs.find((r) => r.id === selectedRun) ?? null;
  const agentName = (r: Run) =>
    agents.find((a) => a.id === r.params.agent)?.name ?? r.params.agent.slice(0, 8);

  return (
    <div>
      <h1 className="sr-only micro">{task.title}</h1>
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

      {stale && (
        <ErrorState
          title={t("common.stale")}
          hint={t("task.stale.hint")}
          onRetry={refresh}
          retryLabel={t("common.retry")}
        />
      )}

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

      {runs.length > 1 && (
        <>
          <Zone title={t("task.fanoutCompare")}>
            <div className="compare">
              {runs.map((r) => (
                <div
                  key={r.id}
                  className={
                    winner === r.id ? "card compare-card selected" : "card compare-card"
                  }
                >
                  <div className="row">
                    <StatusDot status={r.status} />
                    <strong>{agentName(r)}</strong>
                    {r.cost_usd != null ? (
                      <span className="readout s">{fmtUsd(r.cost_usd)}</span>
                    ) : null}
                    {winner === r.id ? (
                      <>
                        <span className="tag ok">
                          {t("task.winner")}
                          {selectionBy?.startsWith("agent:") ? ` · ${selectionBy.slice(6)}` : ""}
                        </span>
                        {landable ? (
                          <Button
                            loading={landing}
                            onClick={async () => {
                              setLanding(true);
                              try {
                                const { landed } = await api.landTask(task.id);
                                toast("ok", t("toast.landed", { hash: landed }));
                              } catch (e) {
                                toast("err", String(e));
                              } finally {
                                setLanding(false);
                              }
                              refresh();
                            }}
                          >
                            {t("task.land")}
                          </Button>
                        ) : null}
                      </>
                    ) : null}
                  </div>
                  {/* D16: Run.error was never rendered anywhere — a failed run
                      looked like a run with no result. It leads the card. */}
                  <div className="compare-result">
                    {r.error ? (
                      <p className="muted">
                        <span className="tag err">{t("task.runError")}</span> {r.error}
                      </p>
                    ) : null}
                    {r.result ? (
                      <Markdown>{r.result}</Markdown>
                    ) : (
                      <span className="muted">{t("task.noResult")}</span>
                    )}
                  </div>
                  <div className="row">
                    <Button
                      type="link"
                      style={{ paddingLeft: 0 }}
                      onClick={() => setSelectedRun(r.id)}
                    >
                      {t("task.viewLog")}
                    </Button>
                    {r.status === "completed" && winner !== r.id ? (
                      <Button
                        type="primary"
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
          </Zone>
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

      {/* §3: the runs section keeps its numbers next to the thing they
          describe. Two real fields move up to the 18/24 readout grade —
          the run count and Σ cost_usd — which is what puts #task above the
          ≥3 threshold of 行 6 without inventing a dashboard it does not want. */}
      <Zone
        title={t("task.runs")}
        note={
          <>
            <span className="readout s">{runs.length}</span>
            {failedRuns > 0 ? (
              <span className="tag err">
                {failedRuns} {t("status.failed")}
              </span>
            ) : null}
          </>
        }
        actions={<span className="readout s">{t("task.totalCost", { cost: fmtUsd(totalCost) })}</span>}
      >
        {runs.length === 0 ? (
          <p className="muted pad">{t("task.noRuns")}</p>
        ) : (
          <div className="card">
            {runs.map((r) => (
              // One wrapper per run so the row can be followed by its own
              // error block. The card's separator rule is
              // `.card > div + div > .row-btn`, so it still lands on rows 2..N
              // and the first row still has no top rule.
              <div key={r.id}>
                <div
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
                  {/* A run stuck on a permission is stuck on a human: the title
                      is shown inline and links to the queue that can clear it. */}
                  {r.waiting_permission ? (
                    <a
                      className="tag warn"
                      href="#inbox"
                      title={r.waiting_permission.title}
                      onClick={(e) => e.stopPropagation()}
                    >
                      <Icon name="lock" size={11} /> {r.waiting_permission.title}
                    </a>
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
                  {ACTIVE_STATUSES.includes(r.status) ? (
                    // D17: cancelling kills work in flight — it asks first.
                    <Popconfirm
                      title={t("task.cancelConfirm.title")}
                      description={t("task.cancelConfirm.body")}
                      okText={t("task.cancelRun")}
                      cancelText={t("common.keep")}
                      okButtonProps={{ danger: true }}
                      onConfirm={async () => {
                        try {
                          await api.cancelRun(r.id);
                          toast("ok", t("toast.cancelling"));
                        } catch (err) {
                          toast("err", String(err));
                        }
                        refresh();
                      }}
                    >
                      <Button danger onClick={(e) => e.stopPropagation()}>
                        {t("task.cancelRun")}
                      </Button>
                    </Popconfirm>
                  ) : null}
                  {["failed", "interrupted", "cancelled"].includes(r.status) ? (
                    <Button
                      loading={retrying === r.id}
                      onClick={async (e) => {
                        e.stopPropagation();
                        setRetrying(r.id);
                        try {
                          await api.retryRun(r.id);
                          toast("ok", t("toast.retrying"));
                        } catch (err) {
                          toast("err", String(err));
                        }
                        setRetrying(null);
                        refresh();
                      }}
                    >
                      {t("task.retryRun")}
                    </Button>
                  ) : null}
                </div>
              {/* D16 / the 2026-09-22 report: Run.error is the diagnosis. A
                  failed run that only shows a red 「失败」 pill tells the user
                  nothing about whether a retry is worth trying. */}
              {r.error ? <pre className="raw">{r.error}</pre> : null}
              </div>
            ))}
          </div>
        )}
      </Zone>

      {selected ? (
        <Zone
          title={t("task.log")}
          note={<span className="muted mono">{selected.id.slice(0, 13)}</span>}
        >
          <RunTimeline
            key={selected.id}
            run={selected}
            live={ACTIVE_STATUSES.includes(selected.status)}
          />
        </Zone>
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
  // t331 (MASTER row 58): the launcher mode changes WHAT you are looking at
  // (one run vs a fan-out comparison vs a pipeline), so it belongs in the URL.
  // Read once on mount, then keep the hash in step with the state.
  useEffect(() => {
    const h = window.location.hash.replace(/^#/, "");
    const q = new URLSearchParams(h.includes("?") ? h.slice(h.indexOf("?") + 1) : "");
    const m = q.get("mode");
    if (m === "single" || m === "fanout" || m === "pipeline") setMode(m);
  }, []);
  useEffect(() => {
    const h = window.location.hash.replace(/^#/, "");
    const path = h.split("?")[0];
    const q = new URLSearchParams(h.includes("?") ? h.slice(h.indexOf("?") + 1) : "");
    if ((q.get("mode") ?? "single") === mode) return;
    if (mode === "single") q.delete("mode");
    else q.set("mode", mode);
    const qs = q.toString();
    window.location.hash = `${path}${qs ? `?${qs}` : ""}`;
  }, [mode]);
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
        o.category === "thought_level" || o.id === "effort" || o.id === "reasoning_effort",
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
        const { runs } = await api.fanout(task.id, picked, p, repo.trim() || undefined, runOptions);
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
      <div className="row">
        <Segmented
          value={mode}
          onChange={(v) => setMode(v as Mode)}
          options={[
            { value: "single", label: t("launcher.run") },
            { value: "fanout", label: t("launcher.fanout") },
            { value: "pipeline", label: t("launcher.pipeline") },
          ]}
        />
      </div>

      {mode === "single" && (
        <div className="row">
          <Select
            aria-label={t("chat.agent")}
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
        <div className="row">
          {modeChoices.length > 0 ? (
            <Select
              aria-label={t("agents.f.mode")}
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
              aria-label={t("agents.f.effort")}
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
                aria-label={t("launcher.chooseAgent")}
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
        {/* 行 18 wants this 1109x16 disclosure to be >=24px tall. Measured
            attempts from inside the view all fail: a block child (.row) pushes
            the native ▸ marker onto its own line — summary 50px = a 22px dead
            band (marker line box) above the 28px row — and every inline class
            on the ladder tops out at 22px (.zone-title). The fix is
            `.hint summary { padding-block: 4px }` in index.css, not here. */}
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
    <div className="card judge-bar">
      <div className="row">
        <span className="doc-icon">
          <Icon name="thought" size={15} />
        </span>
        <strong>{t("task.judge")}</strong>
        <Select
          aria-label={t("task.judgeAgent")}
          size="small"
          value={judgeAgent || undefined}
          onChange={setJudgeAgent}
          placeholder={t("task.judgeAgent")}
          style={{ minWidth: 150 }}
          options={agents.map((a) => ({ value: a.name, label: a.name }))}
        />
        <Button type="primary" disabled={!canJudge} loading={judging} onClick={go}>
          {t("task.judgeGo")}
        </Button>
        <span className="grow" />
        {verdictTag()}
        {judgement ? <StatusPill status={judgement.judge_run_status} /> : null}
      </div>
      {judgement?.rationale ? <Markdown>{judgement.rationale}</Markdown> : null}
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
        aria-label={t("task.status")}
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
        <Button danger>{t("common.delete")}</Button>
      </Popconfirm>
    </span>
  );
}
