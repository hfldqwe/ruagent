// Home: the daily dashboard — recent sessions, platform status, memory
// and inbox at a glance, with the two actions people actually open the
// panel for. Falls back to the intro guide on first run (no agents or
// no sessions yet), so an empty install still explains itself.
//
// The rule this page is built around: a failed read is NOT a zero. Every
// gauge renders an em dash when its endpoint did not answer, because
// "0 agents" and "cannot reach the daemon" are different facts and the
// dashboard is exactly where confusing them does the most damage
// (MASTER §12 row 20 / view-home.md §5, F10).

import { useCallback, useEffect, useState, type ReactNode } from "react";
import { Button, Card } from "antd";
import {
  api,
  isRoleAgent,
  type MemoryRow,
  type PrimaryInfo,
  type RunReceipt,
  type SessionRecord,
} from "../api";
import { useI18n } from "../i18n";
import { ErrorState, ReadoutStrip, RelTime, Spinner, StatusDot } from "../ui";
import { Icon, type IconName } from "../icons";
import { SOURCE_LABEL, msToIso, sourceHue } from "./Sessions";
import { CreateTaskModal } from "./CreateTaskModal";

/** `null` = the endpoint did not answer. It is never coerced to 0. */
type Maybe<T> = T | null;

interface Dash {
  agents: Maybe<number>;
  tasks: Maybe<number>;
  runs: Maybe<number>;
  docs: Maybe<number>;
  sessions: Maybe<SessionRecord[]>;
  memories: Maybe<MemoryRow[]>;
  storeCounts: Record<string, number>;
  embedder: Maybe<string>;
  /** WHO ANSWERS A NEW CHAT — `GET /api/v1/primary`
   *  (docs/design/primary-agent.md). `null` means the read has not answered
   *  yet; "answered: the daemon has no front door" is `{ name: null, why }`,
   *  i.e. a real object, because those are two different facts and 行 20 exists
   *  precisely to keep them apart. */
  frontDoor: Maybe<PrimaryInfo>;
  /** The last front-door read that FAILED (`null` = the last one landed).
   *  Separate from `frontDoor`, which cannot say "failed" on its own — this
   *  pair is what makes a failure render as a failure instead of as the empty
   *  answer. Deliberately NOT a member of `failed`: that list drives the
   *  platform dot and the "cannot read platform data" banner, so a daemon that
   *  answers the other six endpoints but not this one would be reported
   *  OFFLINE while it is plainly online — the same class of lie as "0 agents". */
  frontDoorErr: unknown;
  /** THE RECEIPTS (P1 of docs/plans/2026-11-20-primary-supervisor-design.md §3):
   *  the runs that finished / failed most recently, anywhere in the platform —
   *  `GET /api/v1/runs?status=completed|failed&limit=…`.
   *
   *  `null` = not answered yet (or the read failed, which `failed` names). An
   *  EMPTY ARRAY is a real, definite answer: "nothing has finished yet" — and
   *  that is what the blocks say, in words. They never render the number 0 in
   *  place of it, for the same reason the gauges render an em dash: "0" is a
   *  reading, and there is no reading here to give. */
  recentDone: Maybe<RunReceipt[]>;
  recentFailed: Maybe<RunReceipt[]>;
  /** endpoints that did not answer; their gauges read em dash, never 0. */
  failed: string[];
}

/** Every field unknown. The first paint is this, not a spinner: the page
 *  renders as soon as the first endpoint lands (view-home.md §5). */
const BLANK: Dash = {
  agents: null,
  tasks: null,
  runs: null,
  docs: null,
  sessions: null,
  memories: null,
  storeCounts: {},
  embedder: null,
  frontDoor: null,
  frontDoorErr: null,
  recentDone: null,
  recentFailed: null,
  failed: [],
};

const STORES = ["profile", "observation", "procedure", "lesson"] as const;
/** `.dash-main` never grows past this — "view all" is the way to more
 *  (view-home.md §4 density cap). */
const RECENT_CAP = 8;
const PREVIEW_CAP = 3;
/** How many receipts each block shows. Smaller than `RECENT_CAP`: a receipt row is
 *  a pointer to a task, and the newest few are the whole point — the full list is
 *  one route (`GET /api/v1/runs`) and one click (`查看全部`) away. */
const RECEIPTS_CAP = 5;

/** The dash a gauge shows when its number could not be read. */
const NA = "—";

/** The section head. `.dash-head` is a frozen e2e selector, so it is written
 *  alongside `.zone-head` instead of replacing it (primitives §9.1). It must
 *  NOT be put on the `<section>`: `.dash-head` is a flex row, and a flex
 *  section lays every following row out sideways (measured: +1107px of
 *  horizontal overflow at 1440). */
function DashHead({
  title,
  note,
  actions,
}: {
  title: string;
  note?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <div className="dash-head zone-head">
      <span className="zone-title">{title}</span>
      {note !== undefined ? <span className="zone-note">{note}</span> : null}
      <span className="grow" />
      {actions}
    </div>
  );
}

/**
 * THE RECEIPTS — "what finished / what failed, just now, anywhere in the platform".
 *
 * WHY HERE (P1 of docs/plans/2026-11-20-primary-supervisor-design.md §3): the user's
 * own complaint was "switching between windows and several agents is exhausting", and
 * before this the platform could not answer "who is done" without opening each task in
 * turn — every run was a window you had to go back to. This is the dashboard saying it
 * once, for all of them.
 *
 * IT IS ONE COMPONENT, RENDERED TWICE (finished / failed), because the two blocks must
 * not drift: same three states, same row anatomy, same empty wording. The difference is
 * the `status` filter the caller passed to `GET /api/v1/runs` and the empty sentence.
 *
 * THE THREE STATES ARE NOT INTERCHANGEABLE:
 *   unreadable (`null`) -> `home.runs.unreadable`: this read did not answer. NOT
 *                          "nothing yet" — an unread fact is not an empty one
 *                          (MASTER §12 row 20, the rule the gauges follow).
 *   empty ([])          -> the block SAYS "还没有…" in words. It never renders the
 *                          number 0 in its place: 0 is a reading, and a reader who
 *                          sees 0 has been told something nobody measured.
 *   rows                -> task title, agent NAME, when, and — for a failure — the
 *                          cause. The row is a button back to the task, because a
 *                          receipt that cannot be opened is a dead end.
 */
function ReceiptZone({
  title,
  runs,
  empty,
  onOpenTask,
  onNav,
}: {
  title: string;
  runs: Maybe<RunReceipt[]>;
  /** The DEFINITE empty sentence for this block. Passed as TEXT (not as a key) so the
   *  translation call that resolves it is a LITERAL at the call site: that is what
   *  `panel/e2e/i18n-check.mjs` can verify, and a key handed through a variable is a
   *  translation nothing checks. (A literal spelled inside a comment is scanned too —
   *  that is how this sentence came to be written without one.) */
  empty: string;
  onOpenTask: (id: string) => void;
  onNav: (hash: string) => void;
}) {
  const { t } = useI18n();
  return (
    <section className="zone">
      <DashHead
        title={title}
        actions={
          <Button type="text" size="small" onClick={() => onNav("board")}>
            {t("home.recent.viewAll")}
          </Button>
        }
      />
      {runs === null ? (
        <p className="muted micro">{t("home.runs.unreadable")}</p>
      ) : runs.length === 0 ? (
        <p className="muted">{empty}</p>
      ) : (
        runs.map((r) => (
          <button
            key={r.id}
            className="row-btn"
            onClick={() => onOpenTask(r.task_id)}
            title={r.error}
          >
            <StatusDot status={r.status} />
            <span className="title">{r.task_title}</span>
            <span className="tag">{r.agent}</span>
            {r.error ? (
              <span className="muted micro truncated">{r.error}</span>
            ) : null}
            <span className="grow" />
            <span className="time">
              <RelTime iso={r.updated_at} />
            </span>
          </button>
        ))
      )}
    </section>
  );
}

/**
 * WHO ANSWERS YOU -- one readout, rendered on BOTH landing branches.
 *
 * It is a component because the same answer is needed twice: the dashboard shows it
 * beside the day's work, and the FIRST-RUN guide shows it because a brand-new install
 * is exactly when "who will reply if I type right now" matters most -- and, before
 * this, that page did not answer it at all (the guide renders when
 * agents === 0 or sessions.length === 0, so a user who had not run anything yet never
 * saw the card; measured in e2e/front-door.spec.ts, where three readings had to skip
 * on a fresh daemon). Two copies would drift; one component cannot.
 *
 * IT IS A READOUT, NOT A SWITCH: the choice is one write against two config files and
 * it lives on #agents, so the only actions here are the jump and -- when the read
 * failed -- the retry.
 *
 * THE THREE STATES ARE NOT INTERCHANGEABLE, and the e2e spec pins each one:
 *   failed    -> home.frontDoor.err: "we could not read it". NOT "there is none"; the
 *                daemon's own reason stands in for the answer and a retry is offered.
 *   no answer -> home.frontDoor.none: a DEFINITE answer, with the reason and the next
 *                step (routing.toml's default names nobody usable, so the next run
 *                silently takes the first enabled card).
 *   named     -> the name plus WHICH carrier produced it: routing.toml's default
 *                outranks the primary flag, so "it is marked primary" and "it answers
 *                you" are two different claims and this page makes the second one.
 * frontDoor === null (the endpoint has not answered yet) is a fourth, transient state:
 * an em dash, the same placeholder the gauges use -- never the empty state.
 */
function FrontDoorReadout({
  dash,
  onNav,
  onRetry,
}: {
  dash: Dash;
  onNav: (hash: string) => void;
  onRetry: () => void;
}) {
  const { t } = useI18n();
  const fd = dash.frontDoor;
  const fdWhy = (fd?.why ?? "").trim();
  const fdErrText = dash.frontDoorErr ? String(dash.frontDoorErr).trim().slice(0, 200) : "";
  const fdSource =
    fd?.source === "primary-flag"
      ? t("home.frontDoor.source.flag")
      : fd?.source === "routing-default"
        ? t("home.frontDoor.source.routing")
        : t("home.frontDoor.source.unknown");

  return (
    <section className="zone" data-readout="front-door">
      <DashHead
        title={t("home.panel.frontDoor")}
        actions={
          <Button type="text" size="small" onClick={() => onNav("agents")}>
            {t("home.frontDoor.change")}
          </Button>
        }
      />
      {dash.frontDoorErr ? (
        /* FAILED, not empty: the daemon's own reason stands in for the answer, with a
           retry. This branch is why frontDoorErr exists -- see Dash. */
        <ErrorState
          title={t("home.frontDoor.err")}
          hint={t("home.frontDoor.err.hint") + (fdErrText ? " — " + fdErrText : "")}
          onRetry={onRetry}
          retryLabel={t("common.retry")}
        />
      ) : fd === null ? (
        /* Not answered yet. An em dash, the same placeholder the gauges use -- never
           the empty state. */
        <div className="status-row">
          <span className="dot" style={{ background: "var(--status-idle)" }} />
          <span className="muted">{NA}</span>
        </div>
      ) : fd.name === null ? (
        /* A DEFINITE answer: there is no front door. Say why and what to do -- the
           fallback is silent otherwise (the next run takes the first enabled card). */
        <>
          <div className="status-row">
            <span className="dot" style={{ background: "var(--status-warn)" }} />
            <span>{t("home.frontDoor.none")}</span>
          </div>
          {fdWhy ? (
            <p className="muted micro truncated" title={fdWhy}>
              {t("home.frontDoor.why", { why: fdWhy })}
            </p>
          ) : null}
          <button className="row-btn" onClick={() => onNav("agents")}>
            <Icon name="bot" size={15} />
            <span className="grow">{t("home.frontDoor.noneCta")}</span>
          </button>
        </>
      ) : (
        <>
          <div className="status-row" title={fdWhy || undefined}>
            <span className="dot" style={{ background: "var(--status-ok)" }} />
            <strong className="truncated">{fd.name}</strong>
            <span className={fd.source === "primary-flag" ? "tag ok" : "tag"}>{fdSource}</span>
          </div>
          {/* The daemon's why, elided by CSS and complete on hover: the short forms
              above do not explain a conflict, and a conflict is exactly when the
              reader needs the sentence. */}
          {fdWhy ? (
            <p className="muted micro truncated" title={fdWhy}>
              {t("home.frontDoor.why", { why: fdWhy })}
            </p>
          ) : null}
        </>
      )}
    </section>
  );
}

export function Home({
  inbox,
  onOpenTask,
  onNav,
}: {
  /** The pending-permission count, owned and polled by the shell (App.tsx,
   *  2s). `null` until the shell's first answer — rendered as an em dash,
   *  never as 0 (MASTER §12 row 20). This view does not ask for it: one
   *  reader for one number is what keeps the sider badge and the todo card
   *  from disagreeing, and it is what leaves #home inside row 28's cap. */
  inbox: number | null;
  onOpenTask: (id: string) => void;
  onNav: (hash: string) => void;
}) {
  const { t } = useI18n();
  const [dash, setDash] = useState<Dash | null>(null);
  const [settled, setSettled] = useState(false);
  const [creating, setCreating] = useState(false);

  /** Merge one endpoint's result, clearing its failure flag. Every endpoint
   *  in `load` reports through this pair, so a retry clears exactly the
   *  endpoints that failed and leaves the rest of the page standing. */
  const patch = useCallback((p: Partial<Dash>, key: string) => {
    setDash((prev) => {
      const base = prev ?? BLANK;
      return { ...base, ...p, failed: base.failed.filter((k) => k !== key) };
    });
  }, []);
  const markFailed = useCallback((key: string) => {
    setDash((prev) => {
      const base = prev ?? BLANK;
      return { ...base, failed: base.failed.includes(key) ? base.failed : [...base.failed, key] };
    });
  }, []);

  const load = useCallback(async () => {
    // Six endpoints, six independent settles. Each one patches its own
    // field the moment it lands, so a slow `memoryList` cannot hold the
    // first paint, and a broken one turns into an em dash instead of a 0
    // (view-home.md §5 / F10).
    //
    // The pending-permission count is deliberately absent: the shell polls
    // that queue and passes the result down as a prop, so this view never
    // asks for it (one reader per number — MASTER §12 row 28).
    await Promise.all([
      api
        .agents()
        .then((a) =>
          // Roles only (user ruling 2026-09-17): runtimes are execution
          // backends, not 智能体.
          patch({ agents: a.filter((x) => x.enabled && isRoleAgent(x)).length }, "agents"),
        )
        .catch(() => markFailed("agents")),
      api.sessions().then((s) => patch({ sessions: s }, "sessions")).catch(() => markFailed("sessions")),
      api.tasks().then((t) => patch({ tasks: t.length }, "tasks")).catch(() => markFailed("tasks")),
      api
        .stats()
        .then((st) => patch({ runs: st.reduce((s, x) => s + x.runs, 0) }, "stats"))
        .catch(() => markFailed("stats")),
      api
        .memoryList("observation", "user")
        .then((m) => {
          const storeCounts: Record<string, number> = {};
          for (const [store, , n] of m.counts) storeCounts[store] = (storeCounts[store] ?? 0) + n;
          patch({ memories: m.memories, storeCounts }, "memory");
        })
        .catch(() => markFailed("memory")),
      api
        .knowledgeDocs()
        .then((d) => patch({ docs: d.documents.length, embedder: d.embedder }, "knowledge"))
        .catch(() => markFailed("knowledge")),
      // THE RECEIPTS, through the SAME pair. They are two reads of ONE endpoint
      // (the same route, two `status` values), each settling on its own: a failed
      // read of "recently failed" must not take the "recently finished" block down
      // with it, and `patch`/`markFailed` is already the mechanism that guarantees
      // that (one endpoint's failure leaves the rest of the page standing).
      api
        .runs({ status: "completed", limit: RECEIPTS_CAP })
        .then((p) => patch({ recentDone: p.runs }, "runs-done"))
        .catch(() => markFailed("runs-done")),
      api
        .runs({ status: "failed", limit: RECEIPTS_CAP })
        .then((p) => patch({ recentFailed: p.runs }, "runs-failed"))
        .catch(() => markFailed("runs-failed")),
      // The front door rides the SAME window as the six above (one cadence per
      // page) but settles itself: it has its own failure text and must not move
      // the platform dot, so it does not go through patch/markFailed — see
      // `frontDoorErr` in `Dash`.
      api
        .primary()
        .then((fd) =>
          setDash((prev) => ({ ...(prev ?? BLANK), frontDoor: fd, frontDoorErr: null })),
        )
        // A FAILED READ IS NOT "THERE IS NO PRIMARY" (行 20). The answer is
        // blanked and the error is kept, so the card can only render the
        // failure — the empty state's text would be a fact nobody read.
        .catch((e) =>
          setDash((prev) => ({
            ...(prev ?? BLANK),
            frontDoor: null,
            frontDoorErr: e ?? new Error("GET /api/v1/primary failed"),
          })),
        ),
    ]);
    setSettled(true);
  }, [patch, markFailed]);

  useEffect(() => {
    void load();
    const i = setInterval(() => void load(), 10000);
    return () => clearInterval(i);
  }, [load]);

  if (!dash) return <Spinner label={t("common.loading")} />;

  // First run only once every endpoint has settled AND we know there is
  // nothing: an unreadable install must not be presented as a fresh one,
  // and a half-loaded one must not flip the page under the reader.
  const firstRun =
    settled &&
    dash.failed.length === 0 &&
    (dash.agents === 0 || (dash.sessions !== null && dash.sessions.length === 0));
  if (firstRun) {
    return (
      <FirstRun
        dash={dash}
        creating={creating}
        setCreating={setCreating}
        onOpenTask={onOpenTask}
        onNav={onNav}
        onRetry={() => void load()}
      />
    );
  }

  const memoriesTotal = Object.values(dash.storeCounts).reduce((s, n) => s + n, 0);
  const memoriesKnown = dash.memories !== null;
  const embedderLabel =
    dash.embedder === null
      ? NA
      : !dash.embedder
        ? t("home.embedder.unknown")
        : dash.embedder.includes("fastembed")
          ? t("home.embedder.semantic")
          : t("home.embedder.fallback");
  const embedderHue =
    dash.embedder === null || !dash.embedder
      ? "var(--status-idle)"
      : dash.embedder.includes("fastembed")
        ? "var(--status-ok)"
        : "var(--status-warn)";
  const recent = (dash.sessions ?? []).slice(0, RECENT_CAP);
  const memories = dash.memories ?? [];

  return (
    <div className="home dash">
      <div className="home-hero dash-hero">
        <div>
          <h1>{t("home.greeting")}</h1>
          <p className="muted">{t("home.subtitle")}</p>
        </div>
        <div className="row">
          <Button type="primary" onClick={() => onNav("chat")}>
            <Icon name="chat" size={15} /> {t("home.action.chat")}
          </Button>
          <Button onClick={() => setCreating(true)}>{t("home.action.task")}</Button>
        </div>
      </div>

      {dash.failed.length > 0 && (
        <ErrorState
          title={t("home.err")}
          hint={t("home.err.hint")}
          onRetry={() => void load()}
          retryLabel={t("common.retry")}
        />
      )}

      {/* `grid` -> .readout-strip.grid (README §3.4 X4). The rule lives in
          index.css and is width-agnostic (auto-fit tracks), so the view only
          asks for the grid variant; it writes no CSS of its own. */}
      <ReadoutStrip
        grid
        items={[
          {
            key: "agents",
            label: t("home.cell.agents"),
            value: dash.agents ?? NA,
            onOpen: () => onNav("agents"),
          },
          {
            key: "tasks",
            label: t("home.cell.tasks"),
            value: dash.tasks ?? NA,
            onOpen: () => onNav("board"),
          },
          {
            key: "runs",
            label: t("home.cell.runs"),
            value: dash.runs ?? NA,
            onOpen: () => onNav("board"),
          },
          {
            key: "memory",
            label: t("home.stat.memories"),
            value: memoriesKnown ? memoriesTotal : NA,
            onOpen: () => onNav("memory"),
          },
        ]}
      />

      <div className="dash-grid">
        <Card size="small" className="dash-main">
          {/* The receipts sit FIRST in the main column: "what just happened" is the
              question this page exists to answer, and before P1 it had no answer at
              all outside of opening each task. */}
          <ReceiptZone
            title={t("home.runs.done.title")}
            runs={dash.recentDone}
            empty={t("home.runs.done.empty")}
            onOpenTask={onOpenTask}
            onNav={onNav}
          />
          <ReceiptZone
            title={t("home.runs.failed.title")}
            runs={dash.recentFailed}
            empty={t("home.runs.failed.empty")}
            onOpenTask={onOpenTask}
            onNav={onNav}
          />
          <section className="zone">
            <DashHead
              title={t("home.recent.title")}
              note={dash.sessions === null ? NA : dash.sessions.length}
              actions={
                <Button type="text" size="small" onClick={() => onNav("sessions")}>
                  {t("home.recent.viewAll")}
                </Button>
              }
            />
            {dash.sessions !== null && recent.length === 0 ? (
              <p className="muted">{t("sessions.empty")}</p>
            ) : (
              recent.map((s) => (
                <button key={s.key} className="row-btn" onClick={() => onNav("sessions")}>
                  <span className="tag">
                    <span className="dot" style={{ background: sourceHue(s.source) }} />{" "}
                    {SOURCE_LABEL[s.source] ?? s.source}
                  </span>
                  <span className="title">{s.title || t("sessions.untitled")}</span>
                  <span className="muted mono micro">
                    {s.message_count} {t("sessions.messages")}
                  </span>
                  <span className="grow" />
                  <span className="time">
                    <RelTime iso={msToIso(s.updated_at)} />
                  </span>
                </button>
              ))
            )}
          </section>
        </Card>

        <div className="dash-side">
          {/* The landing page's most important answer, in the dashboard's side
              column. The card itself is FrontDoorReadout -- the FIRST-RUN guide
              renders the same component, so a brand-new install answers the same
              question with the same sentences and the same three states. */}
          <Card size="small">
            <FrontDoorReadout dash={dash} onNav={onNav} onRetry={() => void load()} />
          </Card>

          <Card size="small">
            <section className="zone">
              <DashHead title={t("home.panel.platform")} />
              <div className="status-row">
                <span
                  className="dot"
                  style={{
                    background:
                      dash.failed.length > 0 ? "var(--status-err)" : "var(--status-ok)",
                  }}
                />
                {dash.failed.length > 0 ? t("common.offline") : t("common.online")}
              </div>
              <div className="status-row" title={dash.embedder || undefined}>
                <span className="dot" style={{ background: embedderHue }} />
                {embedderLabel}
                <span className="muted mono truncated">{dash.embedder ?? ""}</span>
              </div>
            </section>
          </Card>

          <Card size="small">
            <section className="zone">
              <DashHead
                title={t("home.panel.memory")}
                note={memoriesKnown ? memoriesTotal : NA}
              />
              <div className="mem-cells">
                {STORES.map((st) => (
                  <div key={st} className="stat-cell">
                    <span className="stat-num">
                      {memoriesKnown ? (dash.storeCounts[st] ?? 0) : NA}
                    </span>
                    <span className="muted">{t(`memory.store.${st}`)}</span>
                  </div>
                ))}
              </div>
              <div className="mem-sub">
                {t("home.memory.recent")} · {memoriesKnown ? memoriesTotal : NA}
              </div>
              {!memoriesKnown ? (
                <p className="muted micro">{t("home.err")}</p>
              ) : memories.length === 0 ? (
                <p className="muted">{t("home.memory.empty")}</p>
              ) : (
                memories.slice(0, PREVIEW_CAP).map((m) => (
                  <button key={m.id} className="mem-preview" onClick={() => onNav("memory")}>
                    <span className="mp-text">{m.content}</span>
                    <span className="time">
                      <RelTime iso={m.updated_at} />
                    </span>
                  </button>
                ))
              )}
            </section>
          </Card>

          <Card size="small">
            <section className="zone">
              <DashHead title={t("home.panel.todo")} />
              <button className="row-btn" onClick={() => onNav("inbox")}>
                <Icon name="inbox" size={15} />
                <span className="grow">{t("home.todo.pending")}</span>
                {inbox === null ? (
                  <span className="muted">{NA}</span>
                ) : inbox > 0 ? (
                  <span className="nav-badge">{inbox}</span>
                ) : (
                  <span className="muted">{t("home.todo.empty")}</span>
                )}
              </button>
            </section>
          </Card>
        </div>
      </div>

      {creating && (
        <CreateTaskModal
          onClose={() => setCreating(false)}
          onCreated={(id) => {
            setCreating(false);
            onOpenTask(id);
          }}
        />
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// First run: no agents or no sessions yet — the intro guide explains the
// platform instead of an empty dashboard pretending to be useful.
// ---------------------------------------------------------------------------

function FirstRun({
  dash,
  creating,
  setCreating,
  onOpenTask,
  onNav,
  onRetry,
}: {
  dash: Dash;
  creating: boolean;
  setCreating: (v: boolean) => void;
  onOpenTask: (id: string) => void;
  onNav: (hash: string) => void;
  /** Re-read the dashboard (the readout's retry). Passed down rather than
   *  re-implemented: a guide that asked the daemon on its own would be a second
   *  reader of the same answer -- exactly the drift this extraction prevents. */
  onRetry: () => void;
}) {
  const { t } = useI18n();
  const memoriesTotal = Object.values(dash.storeCounts).reduce((s, n) => s + n, 0);
  /* `.stat-cell` / `.stat-num` is the sanctioned KPI grid (primitives §7 R8):
     antd `Statistic` would open a second, off-ladder readout grade
     (view-home.md §3). */
  const stats: { key: string; value: Maybe<number> }[] = [
    { key: "home.stat.agents", value: dash.agents },
    { key: "home.stat.tasks", value: dash.tasks },
    { key: "home.stat.runs", value: dash.runs },
    { key: "home.stat.memories", value: dash.memories === null ? null : memoriesTotal },
    { key: "home.stat.docs", value: dash.docs },
  ];

  const guide: { icon: IconName; hash: string; label: string; desc: string }[] = [
    { icon: "layers", hash: "", label: t("nav.board"), desc: t("home.guide.board") },
    { icon: "brain", hash: "memory", label: t("nav.memory"), desc: t("home.guide.memory") },
    { icon: "book", hash: "knowledge", label: t("nav.knowledge"), desc: t("home.guide.knowledge") },
    { icon: "graph", hash: "graph", label: t("nav.graph"), desc: t("home.guide.graph") },
    { icon: "bot", hash: "agents", label: t("nav.agents"), desc: t("home.guide.agents") },
    { icon: "stats", hash: "stats", label: t("nav.stats"), desc: t("home.guide.stats") },
    { icon: "inbox", hash: "inbox", label: t("nav.inbox"), desc: t("home.guide.inbox") },
  ];

  return (
    <div className="home">
      <div className="home-hero">
        <h1>{t("home.greeting")}</h1>
        <p className="muted">{t("home.subtitle")}</p>
      </div>

      {/* WHO ANSWERS YOU, on the page a brand-new install lands on. Same component as
          the dashboard's card, same three states, same i18n keys: this is the one
          question a first-run user has, and until now this page did not answer it.
          Placement follows the existing rhythm -- a small card above the "start here"
          steps, like the dashboard's own small cards; no new visual grade. */}
      <Card size="small">
        <FrontDoorReadout dash={dash} onNav={onNav} onRetry={onRetry} />
      </Card>

      <div className="stat-strip">
        {stats.map((s) => (
          <div key={s.key} className="stat-cell">
            <span className="stat-num">{s.value ?? NA}</span>
            <span className="muted">{t(s.key)}</span>
          </div>
        ))}
      </div>

      <h2 className="sec">{t("home.start.title")}</h2>
      <div className="steps">
        <Card size="small">
          <div className="step-num">1</div>
          <strong>{t("home.step1.title")}</strong>
          <p className="muted">{t("home.step1.desc")}</p>
          <Button type="primary" size="small" onClick={() => setCreating(true)}>
            {dash.tasks === 0 ? t("home.step1.cta") : t("board.new")}
          </Button>
        </Card>
        <Card size="small">
          <div className="step-num">2</div>
          <strong>{t("home.step2.title")}</strong>
          <p className="muted">{t("home.step2.desc")}</p>
          <Button size="small" onClick={() => onNav("board")} disabled={!dash.tasks}>
            {t("nav.board")}
          </Button>
        </Card>
        <Card size="small">
          <div className="step-num">3</div>
          <strong>{t("home.step3.title")}</strong>
          <p className="muted">{t("home.step3.desc")}</p>
          <Button size="small" onClick={() => onNav("memory")}>
            {t("nav.memory")}
          </Button>
        </Card>
      </div>

      <h2 className="sec">{t("home.guide.title")}</h2>
      <div className="guide-grid">
        {guide.map((g) => (
          <button key={g.hash || "board"} className="guide-card" onClick={() => onNav(g.hash)}>
            <span className="guide-icon">
              <Icon name={g.icon} size={18} />
            </span>
            <div>
              <strong>{g.label}</strong>
              <p>{g.desc}</p>
            </div>
          </button>
        ))}
      </div>

      {creating && (
        <CreateTaskModal
          onClose={() => setCreating(false)}
          onCreated={(id) => {
            setCreating(false);
            onOpenTask(id);
          }}
        />
      )}
    </div>
  );
}
