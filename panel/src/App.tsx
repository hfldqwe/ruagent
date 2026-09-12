// App shell: sidebar navigation + language toggle + hash routing.

import { useEffect, useState } from "react";
import { api } from "./api";
import { useI18n } from "./i18n";
import { ToastHost } from "./ui";
import { Board } from "./views/Board";
import { Home } from "./views/Home";
import { TaskDetail } from "./views/TaskDetail";
import { Memory } from "./views/Memory";
import { Knowledge } from "./views/Knowledge";
import { Graph } from "./views/Graph";
import { Agents, Inbox, Stats } from "./views/Agents";

type View =
  | { kind: "home" }
  | { kind: "board" }
  | { kind: "task"; id: string }
  | { kind: "memory" }
  | { kind: "knowledge" }
  | { kind: "graph" }
  | { kind: "agents" }
  | { kind: "stats" }
  | { kind: "inbox" };

function parseHash(): View {
  const h = window.location.hash.replace(/^#/, "");
  const mTask = h.match(/^task\/([\w-]+)/);
  if (mTask) return { kind: "task", id: mTask[1] };
  switch (h) {
    case "board":
      return { kind: "board" };
    case "memory":
      return { kind: "memory" };
    case "knowledge":
      return { kind: "knowledge" };
    case "graph":
      return { kind: "graph" };
    case "agents":
      return { kind: "agents" };
    case "stats":
      return { kind: "stats" };
    case "inbox":
      return { kind: "inbox" };
    default:
      return { kind: "home" };
  }
}

export default function App() {
  const { lang, setLang, t } = useI18n();
  const [view, setView] = useState<View>(() => parseHash());
  const [inboxCount, setInboxCount] = useState(0);
  const [daemonUp, setDaemonUp] = useState(true);

  useEffect(() => {
    const apply = () => setView(parseHash());
    apply();
    window.addEventListener("hashchange", apply);
    return () => window.removeEventListener("hashchange", apply);
  }, []);

  useEffect(() => {
    const poll = () =>
      api
        .pendingPermissions()
        .then((p) => {
          setInboxCount(p.length);
          setDaemonUp(true);
        })
        .catch(() => setDaemonUp(false));
    poll();
    const i = setInterval(poll, 2000);
    return () => clearInterval(i);
  }, []);

  const nav = (hash: string) => {
    window.location.hash = hash;
  };

  return (
    <ToastHost>
      <div className="shell">
        <aside className="sidebar">
          <div className="brand" onClick={() => nav("")}>
            <span className="brand-mark">ru</span>
            <span className="brand-name">ruagent</span>
          </div>
          <nav>
            <NavItem icon="🏠" label={t("home.greeting") === "欢迎回来" ? "首页" : "Home"} active={view.kind === "home"} onClick={() => nav("home")} />
            <NavItem
              icon="🗂️"
              label={t("nav.board")}
              active={view.kind === "board" || view.kind === "task"}
              onClick={() => nav("board")}
            />
            <NavItem icon="🧠" label={t("nav.memory")} active={view.kind === "memory"} onClick={() => nav("memory")} />
            <NavItem icon="📚" label={t("nav.knowledge")} active={view.kind === "knowledge"} onClick={() => nav("knowledge")} />
            <NavItem icon="🕸️" label={t("nav.graph")} active={view.kind === "graph"} onClick={() => nav("graph")} />
            <NavItem icon="🤖" label={t("nav.agents")} active={view.kind === "agents"} onClick={() => nav("agents")} />
            <NavItem icon="📊" label={t("nav.stats")} active={view.kind === "stats"} onClick={() => nav("stats")} />
            <NavItem
              icon="📥"
              label={t("nav.inbox")}
              badge={inboxCount || undefined}
              active={view.kind === "inbox"}
              onClick={() => nav("inbox")}
            />
          </nav>
          <div className="sidebar-foot">
            {daemonUp ? (
              <span className="conn ok">● {t("common.online")}</span>
            ) : (
              <span className="conn err">● {t("common.offline")}</span>
            )}
            <button
              className="lang-toggle"
              onClick={() => setLang(lang === "zh" ? "en" : "zh")}
              title="中文 / EN"
            >
              {lang === "zh" ? "EN" : "中"}
            </button>
          </div>
        </aside>
        <main className="content">
          {view.kind === "home" && (
            <Home
              onOpenTask={(id) => nav(`task/${id}`)}
              onNav={(hash) => nav(hash || "board")}
            />
          )}
          {view.kind === "board" && <Board onOpen={(id) => nav(`task/${id}`)} />}
          {view.kind === "task" && (
            <TaskDetail key={view.id} id={view.id} onBack={() => nav("board")} />
          )}
          {view.kind === "memory" && <Memory />}
          {view.kind === "knowledge" && <Knowledge />}
          {view.kind === "graph" && <Graph />}
          {view.kind === "agents" && <Agents />}
          {view.kind === "stats" && <Stats />}
          {view.kind === "inbox" && <Inbox />}
        </main>
      </div>
    </ToastHost>
  );
}

function NavItem({
  icon,
  label,
  active,
  badge,
  onClick,
}: {
  icon: string;
  label: string;
  active: boolean;
  badge?: number;
  onClick: () => void;
}) {
  return (
    <button className={active ? "nav-item active" : "nav-item"} onClick={onClick}>
      <span className="nav-icon">{icon}</span>
      <span>{label}</span>
      {badge ? <span className="nav-badge">{badge}</span> : null}
    </button>
  );
}
