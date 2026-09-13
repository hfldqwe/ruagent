// Board: kanban by status + list toggle + task creation.

import { useEffect, useState } from "react";
import { Button, Input, Segmented } from "antd";
import { api, type Task } from "../api";
import { useI18n } from "../i18n";
import { Empty, Modal, RelTime, Spinner, StatusDot, useToast } from "../ui";

const COLUMNS = ["pending", "in_progress", "blocked", "done"] as const;

export function Board({ onOpen }: { onOpen: (id: string) => void }) {
  const { t } = useI18n();
  const [tasks, setTasks] = useState<Task[] | null>(null);
  const [view, setView] = useState<string>("board");
  const [creating, setCreating] = useState(false);

  const refresh = () => api.tasks().then(setTasks).catch(() => setTasks([]));
  useEffect(() => {
    refresh();
    const i = setInterval(refresh, 3000);
    return () => clearInterval(i);
  }, []);

  if (!tasks)
    return <Spinner label={`${t("board.title")}…`} />;
  return (
    <div>
      <div className="view-bar">
        <h2>{t("board.title")}</h2>
        <span className="muted">{t("board.total", { n: tasks.length })}</span>
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

      {tasks.length === 0 ? (
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
          {COLUMNS.map((col) => (
            <div key={col} className="kanban-col">
              <div className="kanban-head">
                {t(`board.col.${col}`)}
                <span className="count">{tasks.filter((x) => x.status === col).length}</span>
              </div>
              {tasks
                .filter((x) => x.status === col)
                .map((x) => (
                  <TaskCard key={x.id} task={x} onOpen={onOpen} />
                ))}
            </div>
          ))}
        </div>
      ) : (
        <div className="card">
          {tasks.map((x) => (
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

      {creating && (
        <CreateTaskModal onClose={() => setCreating(false)} onCreated={onOpen} />
      )}
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
          placeholder="Fix the login bug…"
        />
      </label>
      <label className="field">
        <span>{t("newtask.intentLabel")}</span>
        <Input.TextArea
          rows={4}
          value={intent}
          onChange={(e) => setIntent(e.target.value)}
          placeholder="The login flow 500s when the session cookie expires…"
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
