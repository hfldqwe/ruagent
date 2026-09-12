// Board: kanban by status + list toggle + task creation (Multica-parity).

import { useEffect, useState } from "react";
import { api, type Task } from "../api";
import { Empty, Modal, Spinner, StatusDot, relTime, useToast } from "../ui";

const COLUMNS: { id: string; label: string }[] = [
  { id: "pending", label: "Pending" },
  { id: "in_progress", label: "In progress" },
  { id: "blocked", label: "Blocked" },
  { id: "done", label: "Done" },
];

export function Board({ onOpen }: { onOpen: (id: string) => void }) {
  const [tasks, setTasks] = useState<Task[] | null>(null);
  const [view, setView] = useState<"board" | "list">("board");
  const [creating, setCreating] = useState(false);

  const refresh = () => api.tasks().then(setTasks).catch(() => setTasks([]));
  useEffect(() => {
    refresh();
    const t = setInterval(refresh, 3000);
    return () => clearInterval(t);
  }, []);

  if (!tasks) return <Spinner label="Loading tasks…" />;
  return (
    <div>
      <div className="view-bar">
        <h2>Tasks</h2>
        <span className="muted">{tasks.length} total</span>
        <span className="grow" />
        <div className="seg">
          <button className={view === "board" ? "on" : ""} onClick={() => setView("board")}>
            Board
          </button>
          <button className={view === "list" ? "on" : ""} onClick={() => setView("list")}>
            List
          </button>
        </div>
        <button className="primary" onClick={() => setCreating(true)}>
          + New task
        </button>
      </div>

      {tasks.length === 0 ? (
        <Empty
          icon="🗂️"
          title="No tasks yet"
          hint="Create a task, then run it on any registered agent — or fan it out to several and compare."
        />
      ) : view === "board" ? (
        <div className="kanban">
          {COLUMNS.map((col) => (
            <div key={col.id} className="kanban-col">
              <div className="kanban-head">
                {col.label}
                <span className="count">{tasks.filter((t) => t.status === col.id).length}</span>
              </div>
              {tasks
                .filter((t) => t.status === col.id)
                .map((t) => (
                  <TaskCard key={t.id} task={t} onOpen={onOpen} />
                ))}
            </div>
          ))}
        </div>
      ) : (
        <div className="card">
          {tasks.map((t) => (
            <button key={t.id} className="row-btn" onClick={() => onOpen(t.id)}>
              <StatusDot status={t.status} />
              <span className="title">{t.title}</span>
              {t.project ? <span className="tag">{t.project}</span> : null}
              <span className="muted">{t.status.replaceAll("_", " ")}</span>
              <span className="grow" />
              <span className="time">{relTime(t.created_at)}</span>
            </button>
          ))}
        </div>
      )}

      {creating && <CreateTaskModal onClose={() => setCreating(false)} onCreated={onOpen} />}
    </div>
  );
}

function TaskCard({ task, onOpen }: { task: Task; onOpen: (id: string) => void }) {
  return (
    <button className="kanban-card" onClick={() => onOpen(task.id)}>
      <div className="row tight">
        <StatusDot status={task.status} />
        <strong>{task.title}</strong>
      </div>
      {task.project ? <span className="tag">{task.project}</span> : null}
      <div className="time">{relTime(task.updated_at)}</div>
    </button>
  );
}

function CreateTaskModal({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated: (id: string) => void;
}) {
  const [title, setTitle] = useState("");
  const [intent, setIntent] = useState("");
  const [project, setProject] = useState("");
  const toast = useToast();
  const create = async () => {
    if (!title.trim()) return;
    try {
      const t = await api.createTask(title.trim(), intent.trim() || title.trim(), project.trim() || undefined);
      toast("ok", "task created");
      onCreated(t.id);
    } catch (e) {
      toast("err", String(e));
    }
  };
  return (
    <Modal title="New task" onClose={onClose}>
      <label className="field">
        <span>Title</span>
        <input
          autoFocus
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && create()}
          placeholder="Fix the login bug…"
        />
      </label>
      <label className="field">
        <span>Intent (what should be done — becomes the run prompt)</span>
        <textarea
          rows={4}
          value={intent}
          onChange={(e) => setIntent(e.target.value)}
          placeholder="The login flow 500s when the session cookie expires…"
        />
      </label>
      <label className="field">
        <span>Project (optional — scopes project memories)</span>
        <input value={project} onChange={(e) => setProject(e.target.value)} placeholder="ruagent…" />
      </label>
      <div className="row end">
        <button className="primary" onClick={create}>
          Create
        </button>
      </div>
    </Modal>
  );
}
