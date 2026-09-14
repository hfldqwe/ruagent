// Home: the daily dashboard — recent sessions, platform status, memory
// and inbox at a glance, with the two actions people actually open the
// panel for. Falls back to the intro guide on first run (no agents or
// no sessions yet), so an empty install still explains itself.

import { useEffect, useState } from "react";
import { Button, Card, Statistic } from "antd";
import { api, type MemoryRow, type SessionRecord } from "../api";
import { useI18n } from "../i18n";
import { RelTime, Spinner } from "../ui";
import { Icon, type IconName } from "../icons";
import { SOURCE_LABEL, msToIso, sourceHue } from "./Sessions";
import { CreateTaskModal } from "./Board";

interface Dash {
  agents: number;
  tasks: number;
  runs: number;
  docs: number;
  sessions: SessionRecord[];
  memories: MemoryRow[];
  storeCounts: Record<string, number>;
  embedder: string;
  inbox: number;
}

const EMPTY: Dash = {
  agents: 0,
  tasks: 0,
  runs: 0,
  docs: 0,
  sessions: [],
  memories: [],
  storeCounts: {},
  embedder: "",
  inbox: 0,
};

const STORES = ["profile", "observation", "procedure", "lesson"] as const;

export function Home({ onOpenTask, onNav }: { onOpenTask: (id: string) => void; onNav: (hash: string) => void }) {
  const { t } = useI18n();
  const [dash, setDash] = useState<Dash | null>(null);
  const [down, setDown] = useState(false);
  const [creating, setCreating] = useState(false);

  useEffect(() => {
    const load = async () => {
      try {
        const [agents, sessions, tasks, stats, mem, docs, pending] = await Promise.all([
          api.agents(),
          api.sessions(),
          api.tasks(),
          api.stats(),
          api.memoryList("observation", "user").catch(() => ({
            memories: [] as MemoryRow[],
            counts: [] as [string, string, number][],
          })),
          api.knowledgeDocs().catch(() => ({ documents: [], embedder: "" })),
          api.pendingPermissions().catch(() => []),
        ]);
        const storeCounts: Record<string, number> = {};
        for (const [store, , n] of mem.counts) {
          storeCounts[store] = (storeCounts[store] ?? 0) + n;
        }
        setDown(false);
        setDash({
          agents: agents.filter((a) => a.enabled).length,
          tasks: tasks.length,
          runs: stats.reduce((s, x) => s + x.runs, 0),
          docs: docs.documents.length,
          sessions,
          memories: mem.memories,
          storeCounts,
          embedder: docs.embedder,
          inbox: pending.length,
        });
      } catch {
        setDown(true);
        setDash((prev) => prev ?? EMPTY);
      }
    };
    load();
    const i = setInterval(load, 10000);
    return () => clearInterval(i);
  }, []);

  if (!dash) return <Spinner label={t("common.loading")} />;

  const firstRun = dash.agents === 0 || dash.sessions.length === 0;
  if (firstRun) {
    return <FirstRun dash={dash} creating={creating} setCreating={setCreating} onOpenTask={onOpenTask} onNav={onNav} />;
  }

  const memoriesTotal = Object.values(dash.storeCounts).reduce((s, n) => s + n, 0);
  const embedderLabel = !dash.embedder
    ? t("home.embedder.unknown")
    : dash.embedder.includes("fastembed")
      ? t("home.embedder.semantic")
      : t("home.embedder.fallback");
  const embedderHue = !dash.embedder
    ? "var(--status-idle)"
    : dash.embedder.includes("fastembed")
      ? "var(--status-ok)"
      : "var(--status-warn)";

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

      <div className="dash-grid">
        <Card size="small" className="dash-main">
          <div className="dash-head">
            {t("home.recent.title")}
            <span className="muted">{dash.sessions.length}</span>
            <span className="grow" />
            <Button type="link" size="small" onClick={() => onNav("sessions")}>
              {t("home.recent.viewAll")}
            </Button>
          </div>
          {dash.sessions.slice(0, 8).map((s) => (
            <button key={s.key} className="row-btn" onClick={() => onNav("sessions")}>
              <span className="tag" style={{ display: "inline-flex", alignItems: "center", gap: 6 }}>
                <span className="dot" style={{ background: sourceHue(s.source), width: 6, height: 6 }} />
                {SOURCE_LABEL[s.source] ?? s.source}
              </span>
              <span className="title">
                <strong>{s.title || s.preview || t("sessions.untitled")}</strong>
              </span>
              <span className="muted mono">
                {s.message_count} {t("sessions.messages")}
              </span>
              <span className="grow" />
              <span className="time">
                <RelTime iso={msToIso(s.updated_at)} />
              </span>
            </button>
          ))}
        </Card>

        <div className="dash-side">
          <Card size="small">
            <div className="dash-head">{t("home.panel.platform")}</div>
            <div className="status-row">
              <span className="dot" style={{ background: down ? "var(--status-err)" : "var(--status-ok)" }} />
              {down ? t("common.offline") : t("common.online")}
            </div>
            <div className="status-row" title={dash.embedder || undefined}>
              <span className="dot" style={{ background: embedderHue }} />
              {embedderLabel}
              <span className="muted mono truncated">{dash.embedder}</span>
            </div>
            <div className="mini-stats">
              <div className="stat-cell">
                <span className="stat-num">{dash.agents}</span>
                <span className="muted">{t("home.cell.agents")}</span>
              </div>
              <div className="stat-cell">
                <span className="stat-num">{dash.tasks}</span>
                <span className="muted">{t("home.cell.tasks")}</span>
              </div>
              <div className="stat-cell">
                <span className="stat-num">{dash.runs}</span>
                <span className="muted">{t("home.cell.runs")}</span>
              </div>
            </div>
          </Card>

          <Card size="small">
            <div className="dash-head">{t("home.panel.memory")}</div>
            <div className="mem-cells">
              {STORES.map((st) => (
                <div key={st} className="stat-cell">
                  <span className="stat-num">{dash.storeCounts[st] ?? 0}</span>
                  <span className="muted">{t(`memory.store.${st}`)}</span>
                </div>
              ))}
            </div>
            <div className="mem-sub">{t("home.memory.recent")} · {memoriesTotal}</div>
            {dash.memories.length === 0 ? (
              <p className="muted">{t("home.memory.empty")}</p>
            ) : (
              dash.memories.slice(0, 3).map((m) => (
                <button key={m.id} className="mem-preview" onClick={() => onNav("memory")}>
                  <span className="mp-text">{m.content}</span>
                  <span className="time">
                    <RelTime iso={m.updated_at} />
                  </span>
                </button>
              ))
            )}
          </Card>

          <Card size="small">
            <div className="dash-head">{t("home.panel.todo")}</div>
            <button className="row-btn" onClick={() => onNav("inbox")}>
              <Icon name="inbox" size={15} />
              <span className="grow">{t("home.todo.pending")}</span>
              {dash.inbox > 0 ? (
                <span className="nav-badge">{dash.inbox}</span>
              ) : (
                <span className="muted">{t("home.todo.empty")}</span>
              )}
            </button>
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
  const stats: { key: string; value: number }[] = [
    { key: "home.stat.agents", value: dash.agents },
    { key: "home.stat.tasks", value: dash.tasks },
    { key: "home.stat.runs", value: dash.runs },
    { key: "home.stat.memories", value: memoriesTotal },
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
          <Card key={s.key} size="small">
            <Statistic value={s.value} title={<span className="muted">{t(s.key)}</span>} />
          </Card>
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
