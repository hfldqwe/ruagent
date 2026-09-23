// Board: kanban by status + list toggle + task creation.
//
// view-board.md §1: the page exists to answer "what is stuck, and how do I
// get into it". Four status lanes, one readout per lane, a card that only
// navigates. Two correctness rules the spec calls out explicitly:
//   * an unknown Task.status must NOT vanish — it is filed under pending and
//     the lane head says how many were filed there (§4 / B6);
//   * a failed poll must NOT read as "no tasks" — the previous list stays and
//     a role=alert banner says the numbers may be stale (§5 / B11, MASTER 行 20).
//
// Two more rules, from the 2026-09-22 user report ("有一个在运行中的任务一直
// 在失败中…对于失败的任务没有重试机制"):
//   * a card carries the TASK's status; run outcomes are a separate, explicitly
//     labelled layer underneath it (RunPeek). "the task is done" and "one of its
//     runs failed" are two facts and must not be collapsed into one reading;
//   * a failed run is retryable from the board and its `error` is shown — a bare
//     「失败」 with no reason is not a diagnosis.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Button, Input, Segmented } from "antd";
import { api, type Run, type Task } from "../api";
import { Icon } from "../icons";
import { useI18n } from "../i18n";
import {
  Empty,
  ErrorState,
  Modal,
  ReadoutStrip,
  RelTime,
  Spinner,
  StatusDot,
  StatusPill,
  useToast,
} from "../ui";

const COLUMNS = ["pending", "in_progress", "blocked", "done"] as const;
type Column = (typeof COLUMNS)[number];
const isColumn = (s: string): s is Column => (COLUMNS as readonly string[]).includes(s);

/** §4 density ceiling: past 200 tasks each lane folds to 20 + a jump to the list. */
const CARD_CAP = 200;
const LANE_CAP = 20;

export function Board({ onOpen }: { onOpen: (id: string) => void }) {
  const { t } = useI18n();
  const [tasks, setTasks] = useState<Task[] | null>(null);
  const [err, setErr] = useState<unknown>(null);
  const [view, setView] = useState<string>("board");
  const [creating, setCreating] = useState(false);
  const lanes = useRef<Record<string, HTMLDivElement | null>>({});

  // A failed poll keeps the last good list (a transient 500 must not turn 21
  // tasks into "no tasks yet"); `err` is what the view renders from.
  const refresh = () =>
    api
      .tasks()
      .then((next) => {
        setTasks(next);
        setErr(null);
      })
      .catch((e) => setErr(e));

  // 行 39: this page has ONE endpoint, so the window's whole budget is
  // `2K + 2 = 4` requests. A 3s timer spends all four (mount + 3 ticks) and
  // the audit measured 5 — the timestamps (261 / 3270 / 6274 / 9268) are
  // evenly spaced, so this is a cadence cost, not a duplicate fetch. 5s
  // leaves mount + two ticks, and a hidden tab stops asking entirely.
  useEffect(() => {
    refresh();
    const i = setInterval(() => {
      if (!document.hidden) refresh();
    }, 5000);
    const onVisible = () => {
      if (!document.hidden) refresh();
    };
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      clearInterval(i);
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, []);

  // Grouping + ordering. `updated_at` desc inside a lane: without a stable
  // order the 3s poll re-shuffles the cards (§5 dense / B7).
  const grouped = useMemo(() => {
    const m: Record<Column, Task[]> = { pending: [], in_progress: [], blocked: [], done: [] };
    for (const x of tasks ?? []) m[isColumn(x.status) ? x.status : "pending"].push(x);
    for (const c of COLUMNS) {
      m[c].sort((a, b) => (b.updated_at ?? "").localeCompare(a.updated_at ?? ""));
    }
    return m;
  }, [tasks]);

  const total = tasks?.length ?? 0;
  const unknown = useMemo(() => (tasks ?? []).filter((x) => !isColumn(x.status)).length, [tasks]);
  const folded = total > CARD_CAP;

  if (tasks === null) {
    if (err) {
      return (
        <>
          <h1 className="sr-only micro">{t("board.title")}</h1>
          <ErrorState
            title={t("board.err")}
            hint={t("board.err.hint")}
            onRetry={refresh}
            retryLabel={t("common.retry")}
          />
        </>
      );
    }
    return <Spinner label={`${t("board.title")}…`} />;
  }

  // X5 three-state exclusivity: an error never coexists with `.ant-empty`.
  const stale = !!err && total > 0;

  return (
    <div>
      <h1 className="sr-only micro">{t("board.title")}</h1>
      <div className="view-bar">
        <h2>{t("board.title")}</h2>
        <span className="muted">{t("board.total", { n: total })}</span>
        <span className="grow" />
        <Segmented
          value={view}
          onChange={(v) => setView(v as string)}
          options={[
            { value: "board", label: t("board.board") },
            { value: "list", label: t("board.list") },
          ]}
        />
        <Button type="primary" onClick={() => setCreating(true)}>
          + {t("board.new")}
        </Button>
      </div>

      {stale && (
        <ErrorState
          title={t("common.stale")}
          hint={t("board.stale.hint")}
          onRetry={refresh}
          retryLabel={t("common.retry")}
        />
      )}

      {total > 0 && (
        /* `grid` -> .readout-strip.grid (README §3.4 X4): >520 the auto-fit
           tracks land on the same rects as the flex strip; <=520 the shared
           520 branch turns the four lane gauges into a 2x2. */
        <ReadoutStrip
          grid
          items={COLUMNS.map((col) => {
            const n = grouped[col].length;
            return {
              key: col,
              label: t(`board.col.${col}`),
              value: n,
              // W5: only "in progress and non-zero" earns the signal colour.
              signal: col === "in_progress" && n > 0,
              onOpen: () => lanes.current[col]?.scrollIntoView({ block: "start" }),
            };
          })}
        />
      )}

      {total === 0 ? (
        <Empty
          icon="layers"
          title={t("board.empty.title")}
          hint={t("board.empty.hint")}
          action={
            <Button type="primary" onClick={() => setCreating(true)}>
              + {t("board.new")}
            </Button>
          }
        />
      ) : view === "board" ? (
        <div className="kanban">
          {COLUMNS.map((col) => {
            const all = grouped[col];
            const shown = folded ? all.slice(0, LANE_CAP) : all;
            // Hand-written zone rather than <Zone>: §7 keeps the frozen
            // .kanban-head name on the head element itself, alongside the
            // zone roles (the component hard-codes .zone-head alone).
            return (
              <section key={col} className="zone kanban-col">
                <div className="kanban-head zone-head">
                  <div className="zone-title">{t(`board.col.${col}`)}</div>
                  {col === "pending" && unknown > 0 ? (
                    <span className="zone-note micro">{t("board.unknown", { n: unknown })}</span>
                  ) : null}
                  <span className="grow" />
                  <span className="count">{all.length}</span>
                </div>
                <div
                  ref={(el) => {
                    lanes.current[col] = el;
                  }}
                >
                  {shown.map((x) => (
                    <div key={x.id}>
                      <TaskCard task={x} onOpen={onOpen} />
                      <RunPeek task={x} />
                    </div>
                  ))}
                  {folded && all.length > shown.length ? (
                    <Button type="link" block onClick={() => setView("list")}>
                      {t("board.more", { n: all.length - shown.length })} →
                    </Button>
                  ) : null}
                </div>
              </section>
            );
          })}
        </div>
      ) : (
        <div className="card">
          {COLUMNS.flatMap((col) => grouped[col]).map((x) => (
            <button key={x.id} className="row-btn" onClick={() => onOpen(x.id)}>
              <StatusDot status={x.status} />
              <span className="title">{x.title}</span>
              {x.project ? <span className="tag">{x.project}</span> : null}
              <span className="muted">{t(`status.${x.status}`)}</span>
              <span className="grow" />
              <span className="time">
                <RelTime iso={x.created_at} />
              </span>
            </button>
          ))}
        </div>
      )}

      {creating && <CreateTaskModal onClose={() => setCreating(false)} onCreated={onOpen} />}
    </div>
  );
}

function TaskCard({ task, onOpen }: { task: Task; onOpen: (id: string) => void }) {
  const { t } = useI18n();
  return (
    <button className="kanban-card" onClick={() => onOpen(task.id)}>
      <div className="row tight">
        <StatusDot status={task.status} />
        <strong>{task.title}</strong>
      </div>
      {task.project ? <span className="tag">{task.project}</span> : null}
      <div className="time">
        <RelTime iso={task.updated_at} />
      </div>
      <span className="kanban-open muted">{t("task.viewLog")} →</span>
    </button>
  );
}

/** Run statuses a retry may be offered for — the same set TaskDetail uses. */
const RETRYABLE = ["failed", "interrupted", "cancelled"];

/** The lanes stack into one column at <=768 (index.css:721), and that is where
 *  the run toggle below the last card stops reading as a card control: it is a
 *  full-lane-width text button sitting at the column's bottom edge, and the
 *  next column's head follows right after it, so it reads as a stray tab that
 *  got lost between two columns (captain's screenshot, 390px). Same matchMedia
 *  pattern as the shell's 992px collapse, so a resize adapts live. */
function useStackedLanes(): boolean {
  const [stacked, setStacked] = useState(
    () => window.matchMedia("(max-width: 768px)").matches,
  );
  useEffect(() => {
    const mq = window.matchMedia("(max-width: 768px)");
    const onChange = () => setStacked(mq.matches);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);
  return stacked;
}

/** A task's runs, on demand.
 *
 * Why on demand rather than on the card face: the board's only list endpoint
 * is /api/v1/tasks, which carries no run data, and there is no cross-task run
 * endpoint. Filling the face would cost one /api/v1/tasks/{id} per card — an
 * N+1 the request budget forbids (MASTER §12 行 39: this page has K = 1 data
 * surface, so its whole window is 2K + 2 = 4 requests; the audit measured 5
 * requests at a 3s poll and 4 at 5s). So the board learns a task's runs when
 * the user asks, and only then.
 *
 * What this is NOT: the card's identity. The card face keeps the TASK status
 * (lane + StatusDot); this strip is titled 「运行（N）」 and every row inside it
 * is a RUN with its own status, id, error and retry. A done task whose first
 * run failed reads 「已完成」 on the card and 「运行（2） · 1 失败」 here — the
 * two facts stay separate instead of collapsing into "the task failed". */
function RunPeek({ task }: { task: Task }) {
  const { t } = useI18n();
  const toast = useToast();
  const stacked = useStackedLanes();
  const [open, setOpen] = useState(false);
  const [runs, setRuns] = useState<Run[] | null>(null);
  const [err, setErr] = useState<unknown>(null);
  const [retrying, setRetrying] = useState<string | null>(null);

  const load = useCallback(
    () =>
      api
        .task(task.id)
        .then((r) => {
          setRuns(r.runs);
          setErr(null);
        })
        .catch((e) => setErr(e)),
    [task.id],
  );

  // Polls only while open, and only this one endpoint; closing stops the timer
  // so a board left open costs nothing.
  useEffect(() => {
    if (!open) return;
    load();
    const i = setInterval(load, 3000);
    return () => clearInterval(i);
  }, [open, load]);

  // The badge says 「失败」, so it counts failures — not every retryable
  // status. An interrupted run still shows its own pill inside the strip.
  const failed = (runs ?? []).filter((r) => r.status === "failed").length;

  return (
    <div>
      {/* A text Button, not .readout-btn: 行 18 budgets the content area at
          ≤10 elements under 32px, and .readout-btn is 29px — 21 cards would
          spend 21 of it (measured: 内容区<32px 21/≤10 ✗). antd's control
          height is 32, so the toggle stays inside the budget. .kanban-runs is
          the semantic hook — this page already has .readout-btn elements (the
          lane gauges), so the toggle needs its own name to be addressable. */}
      <Button
        type="text"
        /* Stacked lanes: hug the label and lead with an icon, so the control
           reads as this card's action instead of a full-lane tab. The width is
           the whole difference — a 294px bar at the column's bottom edge reads
           as a lost tab, a 100px icon+label directly under its card does not.
           The count still appears once the strip has been opened: the board has
           no cross-task run endpoint and Task carries no run data, so N is not
           knowable before the user asks (see the note above RunPeek). */
        block={!stacked}
        icon={stacked ? <Icon name="history" size={13} /> : undefined}
        className="kanban-runs"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        {t("task.runs")}
        {runs ? t("task.runsCount", { n: runs.length }) : ""}
        {failed > 0 ? (
          <>
            {" "}
            <span className="tag err">
              {failed} {t("status.failed")}
            </span>
          </>
        ) : null}
      </Button>
      {open ? (
        <div className="card">
          {err ? (
            // task.err, not board.err: this failure is one task's run fetch,
            // not the board's list.
            <ErrorState
              title={t("task.err")}
              hint={t("task.err.hint")}
              onRetry={load}
              retryLabel={t("common.retry")}
            />
          ) : runs === null ? (
            <Spinner />
          ) : runs.length === 0 ? (
            <p className="muted pad">{t("task.noRuns")}</p>
          ) : (
            runs.map((r) => (
              <div key={r.id}>
                <div className="row">
                  <StatusDot status={r.status} />
                  <span className="mono muted">{r.id.slice(0, 8)}</span>
                  <StatusPill status={r.status} />
                  <span className="grow" />
                  {RETRYABLE.includes(r.status) ? (
                    <Button
                      loading={retrying === r.id}
                      onClick={async () => {
                        setRetrying(r.id);
                        try {
                          await api.retryRun(r.id);
                          toast("ok", t("toast.retrying"));
                        } catch (e) {
                          toast("err", String(e));
                        }
                        setRetrying(null);
                        load();
                      }}
                    >
                      {t("task.retryRun")}
                    </Button>
                  ) : null}
                </div>
                {/* The other half of the report: a failed run must say WHY.
                    A red pill reading 「失败」 is a state, not a diagnosis. */}
                {r.error ? <pre className="raw">{r.error}</pre> : null}
              </div>
            ))
          )}
        </div>
      ) : null}
    </div>
  );
}

export function CreateTaskModal({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated: (id: string) => void;
}) {
  const { t } = useI18n();
  const [title, setTitle] = useState("");
  const [intent, setIntent] = useState("");
  const [project, setProject] = useState("");
  const [busy, setBusy] = useState(false);
  const toast = useToast();
  const create = async () => {
    if (!title.trim() || busy) return;
    setBusy(true);
    try {
      const task = await api.createTask(
        title.trim(),
        intent.trim() || title.trim(),
        project.trim() || undefined,
      );
      toast("ok", t("newtask.created"));
      onCreated(task.id);
    } catch (e) {
      toast("err", String(e));
      setBusy(false);
    }
  };
  return (
    <Modal title={t("newtask.title")} onClose={onClose}>
      <label className="field">
        <span>{t("newtask.titleLabel")}</span>
        <Input
          autoFocus
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          onPressEnter={create}
          placeholder={t("newtask.titlePh")}
        />
      </label>
      <label className="field">
        <span>{t("newtask.intentLabel")}</span>
        <Input.TextArea
          rows={4}
          value={intent}
          onChange={(e) => setIntent(e.target.value)}
          placeholder={t("newtask.intentPh")}
        />
      </label>
      <label className="field">
        <span>{t("newtask.projectLabel")}</span>
        <Input
          value={project}
          onChange={(e) => setProject(e.target.value)}
          placeholder="ruagent…"
        />
      </label>
      <div className="row end">
        <Button type="primary" loading={busy} disabled={!title.trim()} onClick={create}>
          {t("common.create")}
        </Button>
      </div>
    </Modal>
  );
}
