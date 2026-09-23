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
import { api, isRoleAgent, type MemoryRow, type SessionRecord } from "../api";
import { useI18n } from "../i18n";
import { ErrorState, ReadoutStrip, RelTime, Spinner } from "../ui";
import { Icon, type IconName } from "../icons";
import { SOURCE_LABEL, msToIso, sourceHue } from "./Sessions";
import { CreateTaskModal } from "./Board";

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
  failed: [],
};

const STORES = ["profile", "observation", "procedure", "lesson"] as const;
/** `.dash-main` never grows past this — "view all" is the way to more
 *  (view-home.md §4 density cap). */
const RECENT_CAP = 8;
const PREVIEW_CAP = 3;

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
}: {
  dash: Dash;
  creating: boolean;
  setCreating: (v: boolean) => void;
  onOpenTask: (id: string) => void;
  onNav: (hash: string) => void;
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
