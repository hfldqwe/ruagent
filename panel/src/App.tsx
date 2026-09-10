import { useEffect, useState } from "react";
import { api, type AgentInfo, type PendingPermission, type Task } from "./api";
import { TaskDetail } from "./views/TaskDetail";

type Tab = "tasks" | "agents" | "permissions";

export default function App() {
  const [tab, setTab] = useState<Tab>("tasks");
  const [tasks, setTasks] = useState<Task[]>([]);
  const [agents, setAgents] = useState<AgentInfo[]>([]);
  const [pending, setPending] = useState<PendingPermission[]>([]);
  const [selectedTask, setSelectedTask] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = () => {
    api.tasks().then(setTasks).catch((e) => setError(String(e)));
    api.agents().then(setAgents).catch(() => {});
    api.pendingPermissions().then(setPending).catch(() => {});
  };

  useEffect(() => {
    refresh();
    const t = setInterval(refresh, 3000);
    return () => clearInterval(t);
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
        {tabButton("permissions", "Permissions", pending.length)}
      </header>
      {error && <div className="error">{error}</div>}
      <main>
        {tab === "tasks" &&
          (selectedTask ? (
            <TaskDetail id={selectedTask} onBack={() => setSelectedTask(null)} />
          ) : (
            <TaskList tasks={tasks} onSelect={setSelectedTask} />
          ))}
        {tab === "agents" && <AgentsView agents={agents} />}
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

function AgentsView({ agents }: { agents: AgentInfo[] }) {
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
