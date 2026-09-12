// App shell: sidebar navigation + hash routing + permission badge polling.

import { useEffect, useState } from "react";
import { api } from "./api";
import { ToastHost } from "./ui";
import { Board } from "./views/Board";
import { TaskDetail } from "./views/TaskDetail";
import { Memory } from "./views/Memory";
import { Knowledge } from "./views/Knowledge";
import { Graph } from "./views/Graph";
import { Agents, Inbox, Stats } from "./views/Agents";

type View =
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
      return { kind: "board" };
  }
}

export default function App() {
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
    const t = setInterval(poll, 2000);
    return () => clearInterval(t);
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
            <NavItem
              icon="🗂️"
              label="Board"
              active={view.kind === "board" || view.kind === "task"}
              onClick={() => nav("")}
            />
            <NavItem icon="🧠" label="Memory" active={view.kind === "memory"} onClick={() => nav("memory")} />
            <NavItem
              icon="📚"
              label="Knowledge"
              active={view.kind === "knowledge"}
              onClick={() => nav("knowledge")}
            />
            <NavItem icon="🕸️" label="Graph" active={view.kind === "graph"} onClick={() => nav("graph")} />
            <NavItem icon="🤖" label="Agents" active={view.kind === "agents"} onClick={() => nav("agents")} />
            <NavItem icon="📊" label="Stats" active={view.kind === "stats"} onClick={() => nav("stats")} />
            <NavItem
              icon="📥"
              label="Inbox"
              badge={inboxCount || undefined}
              active={view.kind === "inbox"}
              onClick={() => nav("inbox")}
            />
          </nav>
          <div className="sidebar-foot">
            {daemonUp ? (
              <span className="conn ok">● daemon online</span>
            ) : (
              <span className="conn err">● daemon unreachable</span>
            )}
          </div>
        </aside>
        <main className="content">
          {view.kind === "board" && <Board onOpen={(id) => nav(`task/${id}`)} />}
          {view.kind === "task" && (
            <TaskDetail key={view.id} id={view.id} onBack={() => nav("")} />
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
