// t219: this component used to live in Board.tsx. Home renders it too (its own
// "new task" buttons), and Home's static import of Board meant #home fetched
// Board's whole module — measured 7KB fetched, 96% of it unused on that route
// (the board view's own functions never run there). The modal shares nothing
// with the board view except the module it happened to be written in, so it
// gets its own module: Home imports this directly, Board re-exports it (App's
// lazy createTask import reads m.CreateTaskModal), and the board view stops
// riding along on #home. Body is verbatim from Board.tsx.
import { useState } from "react";
import { Button, Input } from "antd";
import { api } from "../api";
import { useI18n } from "../i18n";
import { ErrorState, Modal, useToast } from "../ui";

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
  // t229: the failure used to appear ONLY as a toast — measured at [720,28],
  // 637px from the trigger and outside the dialog, while the dialog itself
  // stayed open and silent. The toast stays; the dialog now also shows the
  // line, which is the shape Memory's write dialog already had.
  const [err, setErr] = useState<string | null>(null);
  const toast = useToast();
  const create = async () => {
    if (!title.trim() || busy) return;
    setBusy(true);
    setErr(null);
    try {
      const task = await api.createTask(
        title.trim(),
        intent.trim() || title.trim(),
        project.trim() || undefined,
      );
      toast("ok", t("newtask.created"));
      onCreated(task.id);
    } catch (e) {
      setErr(String(e));
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
      {err ? <ErrorState title={err} /> : null}
      <div className="row end">
        <Button type="primary" loading={busy} disabled={busy || !title.trim()} onClick={create}>
          {t("common.create")}
        </Button>
      </div>
    </Modal>
  );
}
