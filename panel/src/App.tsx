import { useEffect, useState } from "react";
import { api, type AgentInfo, type AgentStats, type McpRegistry, type PendingPermission, type Task } from "./api";
import { TaskDetail } from "./views/TaskDetail";

type Tab = "tasks" | "agents" | "stats" | "permissions";

export default function App() {
  const [tab, setTab] = useState<Tab>("tasks");
  const [tasks, setTasks] = useState<Task[]>([]);
  const [agents, setAgents] = useState<AgentInfo[]>([]);
  const [stats, setStats] = useState<AgentStats[]>([]);
  const [mcp, setMcp] = useState<McpRegistry | null>(null);
  const [pending, setPending] = useState<PendingPermission[]>([]);
  const [selectedTask, setSelectedTask] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = () => {
    api.tasks().then(setTasks).catch((e) => setError(String(e)));
    api.agents().then(setAgents).catch(() => {});
    api.pendingPermissions().then(setPending).catch(() => {});
    api.stats().then(setStats).catch(() => {});
  };

  useEffect(() => {
    refresh();
    api.mcp().then(setMcp).catch(() => {});
    const t = setInterval(refresh, 3000);
    return () => clearInterval(t);
  }, []);

  // Deep links: #task/<id> opens that task (also makes the compare view
  // reachable by URL).
  useEffect(() => {
    const apply = () => {
      const m = window.location.hash.match(/^#task\/([\w-]+)/);
      setSelectedTask(m ? m[1] : null);
    };
    apply();
    window.addEventListener("hashchange", apply);
    return () => window.removeEventListener("hashchange", apply);
  }, []);

  const tabButton = (t: Tab, label: string, badge?: number) => (
    <button
      key={t}
      className={tab === t ? "tab active" : "tab"}
      onClick={() => {
        setTab(t);
        setSelectedTask(null);
      }}
    >
      {label}
      {badge ? <span className="badge">{badge}</span> : null}
    </button>
  );

  return (
    <div className="app">
      <header className="topbar">
        <span className="brand">ruagent</span>
        {tabButton("tasks", "Tasks")}
        {tabButton("agents", "Agents")}
        {tabButton("stats", "Stats")}
        {tabButton("permissions", "Permissions", pending.length)}
      </header>
      {error && <div className="error">{error}</div>}
      <main>
        {tab === "tasks" &&
          (selectedTask ? (
            <TaskDetail
              id={selectedTask}
              onBack={() => {
                setSelectedTask(null);
                history.replaceState(null, "", location.pathname);
              }}
            />
          ) : (
            <TaskList
              tasks={tasks}
              onSelect={(id) => {
                setSelectedTask(id);
                location.hash = `task/${id}`;
              }}
            />
          ))}
        {tab === "agents" && <AgentsView agents={agents} mcp={mcp} />}
        {tab === "stats" && <StatsView stats={stats} />}
        {tab === "permissions" && <PermissionsView pending={pending} onChanged={refresh} />}
      </main>
    </div>
  );
}

function TaskList({ tasks, onSelect }: { tasks: Task[]; onSelect: (id: string) => void }) {
  const [title, setTitle] = useState("");
  const [intent, setIntent] = useState("");

  const create = async () => {
    if (!title.trim()) return;
    const t = await api.createTask(title.trim(), intent.trim() || title.trim());
    setTitle("");
    setIntent("");
    onSelect(t.id);
  };

  return (
    <div>
      <div className="card new-task">
        <input
          placeholder="Task title"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && create()}
        />
        <input
          placeholder="Intent (optional, defaults to title)"
          value={intent}
          onChange={(e) => setIntent(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && create()}
        />
        <button onClick={create}>Create</button>
      </div>
      {tasks.length === 0 && <p className="muted">No tasks yet.</p>}
      {tasks.map((t) => (
        <div
          key={t.id}
          className="row clickable"
          onClick={() => onSelect(t.id)}
        >
          <StatusDot status={t.status} />
          <span className="title">{t.title}</span>
          <span className="muted">{t.status}</span>
          <span className="muted time">{new Date(t.created_at).toLocaleTimeString()}</span>
        </div>
      ))}
    </div>
  );
}

function AgentsView({ agents, mcp }: { agents: AgentInfo[]; mcp: McpRegistry | null }) {
  return (
    <div>
      {agents.map((a) => (
        <div key={a.name} className="card">
          <div className="row">
            <StatusDot status={a.enabled ? "in_progress" : "blocked"} />
            <strong>{a.name}</strong>
            <span className="tag">{a.harness}</span>
            <span className="muted">{a.model ?? ""}</span>
          </div>
          <p className="muted">{a.description}</p>
        </div>
      ))}
      <h3>MCP registry</h3>
      {mcp && mcp.servers.length > 0 ? (
        mcp.servers.map((s) => (
          <div key={s.name} className="card">
            <div className="row">
              <strong>{s.name}</strong>
              {s.inject_for && <span className="tag">{s.inject_for.join(", ")}</span>}
            </div>
            <p className="muted">{s.url ?? s.command}</p>
          </div>
        ))
      ) : (
        <p className="muted">
          No MCP servers registered. Add them to mcp.toml (design &sect;7.1: injection is an
          overlay &mdash; each CLI&rsquo;s own config is never touched).
        </p>
      )}
      {mcp && mcp.profiles.length > 0 && (
        <p className="muted">
          Profiles: {mcp.profiles.map((p) => `${p.name} (${p.servers.length})`).join(" · ")}
        </p>
      )}
    </div>
  );
}

function StatsView({ stats }: { stats: AgentStats[] }) {
  if (stats.length === 0) return <p className="muted">No runs recorded yet.</p>;
  return (
    <div className="card">
      <table className="stats">
        <thead>
          <tr>
            <th>agent</th>
            <th>runs</th>
            <th>completed</th>
            <th>failed</th>
            <th>cost</th>
            <th>last run</th>
          </tr>
        </thead>
        <tbody>
          {stats.map((s) => (
            <tr key={s.agent}>
              <td>{s.agent}</td>
              <td>{s.runs}</td>
              <td>{s.completed}</td>
              <td>{s.failed}</td>
              <td>${s.total_cost_usd.toFixed(4)}</td>
              <td className="muted">
                {s.last_run_at ? new Date(s.last_run_at).toLocaleString() : "-"}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function PermissionsView({
  pending,
  onChanged,
}: {
  pending: PendingPermission[];
  onChanged: () => void;
}) {
  const resolve = async (p: PendingPermission, action: string) => {
    await api.resolvePermission(p.run_id, p.tool_call_id, action);
    onChanged();
  };
  if (pending.length === 0) return <p className="muted">Inbox empty — nothing waiting on you.</p>;
  return (
    <div>
      {pending.map((p) => (
        <div key={`${p.run_id}:${p.tool_call_id}`} className="card">
          <div className="row">
            <strong>{p.title}</strong>
            <span className="tag">{p.tool_call_id}</span>
          </div>
          <pre className="raw">{JSON.stringify(p.raw_input, null, 2)}</pre>
          <div className="row">
            <button className="primary" onClick={() => resolve(p, "allow")}>
              Allow
            </button>
            <button className="danger" onClick={() => resolve(p, "reject")}>
              Reject
            </button>
          </div>
        </div>
      ))}
    </div>
  );
}

export function StatusDot({ status }: { status: string }) {
  const color =
    {
      done: "#3fb950",
      completed: "#3fb950",
      failed: "#f85149",
      cancelled: "#f85149",
      interrupted: "#d29922",
      in_progress: "#58a6ff",
      running: "#58a6ff",
      spawning: "#58a6ff",
      pending: "#8b949e",
      waiting_permission: "#d29922",
    }[status] ?? "#8b949e";
  return <span className="dot" style={{ background: color }} />;
}
