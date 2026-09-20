// Runtimes view — the execution backends (design §4.1 two-layer model,
// user ruling 2026-09-17: claude-code / dsh / opencode are RUNTIMES, a
// sibling page to the Agents view; agents are portable roles). This is
// the connection-management surface: list, create, edit and delete
// backends — every edit goes through the daemon, which round-trips
// agents.toml (comments preserved) and hot-reloads the registry.

import { useEffect, useState } from "react";
import { Button, Card, Input, Popconfirm, Select, Tooltip } from "antd";
import { api, isRoleAgent, type AgentInfo, type SessionOptionInfo } from "../api";
import { brandClass, BrandMark } from "../brand";
import { Empty, Modal, Spinner, useToast } from "../ui";
import { useI18n } from "../i18n";
import { Icon } from "../icons";

const HARNESSES = ["claude-code", "opencode", "dsh", "mock"];

const modelOption = (options: SessionOptionInfo[]) =>
  options.find((o) => o.category === "model" || o.id === "model");

/** Model-count chip state for the card grid. */
function useModelCounts(names: string[]) {
  const [counts, setCounts] = useState<Record<string, number>>({});
  const [syncing, setSyncing] = useState<string | null>(null);
  const key = names.join(",");
  useEffect(() => {
    if (!key) return;
    let alive = true;
    for (const n of key.split(",")) {
      api
        .agentOptions(n)
        .then((o) => {
          if (!alive) return;
          setCounts((c) => ({ ...c, [n]: modelOption(o.options)?.choices.length ?? 0 }));
        })
        .catch(() => {});
    }
    return () => {
      alive = false;
    };
  }, [key]);
  const sync = async (name: string) => {
    setSyncing(name);
    try {
      const o = await api.agentOptions(name, true);
      setCounts((c) => ({ ...c, [name]: modelOption(o.options)?.choices.length ?? 0 }));
    } catch {
      /* keep the cached count */
    } finally {
      setSyncing(null);
    }
  };
  return { counts, syncing, sync };
}

/** The create/edit form state; `editing` names the runtime being edited. */
type FormState = {
  name: string;
  harness: string;
  command: string;
  description: string;
  mcp_profile: string;
  models: string;
};

const EMPTY_FORM: FormState = {
  name: "",
  harness: "claude-code",
  command: "",
  description: "",
  mcp_profile: "default",
  models: "",
};

export function Runtimes() {
  const { t } = useI18n();
  const [agents, setAgents] = useState<AgentInfo[] | null>(null);
  const [editing, setEditing] = useState<AgentInfo | null>(null);
  const [creating, setCreating] = useState(false);
  const toast = useToast();

  const load = () => api.agents().then(setAgents).catch(() => setAgents([]));
  useEffect(() => {
    load();
    const t = setInterval(load, 5000);
    return () => clearInterval(t);
  }, []);

  // Model-count chips per runtime, from the daemon's cached catalog.
  // Runs before the early return (hook order), names derived from state.
  const { counts, syncing, sync } = useModelCounts(
    (agents ?? []).filter((a) => !isRoleAgent(a) && a.enabled).map((a) => a.name),
  );

  if (!agents) return <Spinner label={`${t("runtimes.title")}…`} />;
  const roles = agents.filter(isRoleAgent);
  // Disabled runtimes are configured off — the mock agent template for
  // instance never belongs on the user surface.
  const runtimes = agents.filter((a) => !isRoleAgent(a) && a.enabled);

  const remove = async (name: string) => {
    try {
      await api.deleteRuntime(name);
      toast("ok", t("runtimes.deleted", { name }));
      load();
    } catch (e) {
      toast("err", String(e));
    }
  };

  return (
    <div>
      <div className="view-bar">
        <h2>{t("runtimes.title")}</h2>
        <span className="muted">{t("runtimes.subtitle")}</span>
        <span className="grow" />
        <Button type="primary" onClick={() => setCreating(true)}>
          + {t("runtimes.create")}
        </Button>
      </div>
      {runtimes.length === 0 ? (
        <Empty icon="layers" title={t("agents.runtimes.empty")} hint={t("agents.runtimes.hint")} />
      ) : (
        <div className="agent-grid">
          {runtimes.map((r) => {
            const usedBy = roles.filter((a) => a.runtimes?.includes(r.name));
            return (
              <Card key={r.name} className="agent-card" size="small">
                <div className="row">
                  {/* Brand logo tile — the runtime's own mark on a tint of
                      its brand color (mock and unknowns stay neutral). */}
                  <span className={`runtime-logo${brandClass(r.harness) ? " " + brandClass(r.harness) : ""}`}>
                    <BrandMark harness={r.harness} size={20} mono fallback="bot" />
                  </span>
                  <div>
                    <strong>{r.name}</strong>
                    <div className="muted">{r.harness}</div>
                  </div>
                  <span className="grow" />
                  <span className="tag ok">{t("agents.enabled")}</span>
                </div>
                <p className="muted" style={{ margin: "10px 0 6px" }}>{r.description}</p>
                {r.command ? (
                  <div className="row">
                    <span className="doc-icon">
                      <Icon name="zap" size={13} />
                    </span>
                    <span className="muted mono truncated" style={{ fontSize: 12 }}>{r.command}</span>
                  </div>
                ) : null}
                <div className="row" style={{ marginTop: 10 }}>
                  {counts[r.name] ? (
                    <Tooltip title={t("chat.sync")}>
                      <span className="tag">
                        {t("runtimes.models", { n: counts[r.name] })}
                      </span>
                    </Tooltip>
                  ) : null}
                  {usedBy.length > 0 ? (
                    usedBy.map((a) => <span key={a.name} className="tag">{a.name}</span>)
                  ) : (
                    <span className="muted" style={{ fontSize: 12 }}>
                      {t("agents.runtimes.noRoles")}
                    </span>
                  )}
                  <span className="grow" />
                </div>
                <div className="row end" style={{ marginTop: 8 }}>
                  <Tooltip title={t("chat.sync")}>
                    <Button
                      size="small"
                      loading={syncing === r.name}
                      onClick={() => sync(r.name)}
                      aria-label={t("chat.sync")}
                    >
                      <Icon name="sync" size={13} />
                    </Button>
                  </Tooltip>
                  <Button
                    size="small"
                    onClick={() => {
                      window.location.hash = `chat?agent=${encodeURIComponent(r.name)}`;
                    }}
                  >
                    {t("agents.runtimes.direct")}
                  </Button>
                  <Button size="small" onClick={() => setEditing(r)}>
                    {t("common.edit")}
                  </Button>
                  <Popconfirm
                    title={t("runtimes.deleteConfirm.title")}
                    description={t("runtimes.deleteConfirm.body")}
                    okText={t("common.delete")}
                    cancelText={t("common.keep")}
                    okButtonProps={{ danger: true }}
                    onConfirm={() => remove(r.name)}
                  >
                    <Button size="small" danger>
                      {t("common.delete")}
                    </Button>
                  </Popconfirm>
                </div>
              </Card>
            );
          })}
        </div>
      )}
      <p className="muted pad" style={{ marginTop: 14 }}>{t("runtimes.hint")}</p>
      {(creating || editing) && (
        <RuntimeModal
          editing={editing}
          onClose={() => {
            setCreating(false);
            setEditing(null);
          }}
          onSaved={() => {
            setCreating(false);
            setEditing(null);
            load();
          }}
        />
      )}
    </div>
  );
}

function RuntimeModal({
  editing,
  onClose,
  onSaved,
}: {
  editing: AgentInfo | null;
  onClose: () => void;
  onSaved: () => void;
}) {
  const { t } = useI18n();
  const toast = useToast();
  const [form, setForm] = useState<FormState>(() =>
    editing
      ? {
          name: editing.name,
          harness: HARNESS_FOR[editing.harness] ?? editing.harness,
          command: editing.command ?? "",
          description: editing.description ?? "",
          mcp_profile: "default",
          models: (editing.models ?? []).join(", "),
        }
      : { ...EMPTY_FORM },
  );
  const [busy, setBusy] = useState(false);
  const set = (k: keyof FormState) => (v: string) => setForm((f) => ({ ...f, [k]: v }));

  const go = async () => {
    setBusy(true);
    try {
      const models = form.models
        .split(",")
        .map((m) => m.trim())
        .filter(Boolean);
      const body = {
        harness: form.harness,
        command: form.command.trim() || undefined,
        description: form.description.trim() || undefined,
        mcp_profile: form.mcp_profile.trim() || undefined,
        models: models.length ? models : undefined,
      };
      if (editing) {
        await api.updateRuntime(editing.name, body);
        toast("ok", t("runtimes.updated", { name: editing.name }));
      } else {
        await api.createRuntime({ name: form.name.trim(), ...body });
        toast("ok", t("runtimes.created", { name: form.name.trim() }));
      }
      onSaved();
    } catch (e) {
      toast("err", String(e));
    } finally {
      setBusy(false);
    }
  };

  const ok = editing || (form.name.trim() && form.command.trim());

  return (
    <Modal
      title={editing ? t("runtimes.editTitle", { name: editing.name }) : t("runtimes.create")}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button type="primary" loading={busy} disabled={!ok} onClick={go}>
            {editing ? t("common.save") : t("runtimes.create")}
          </Button>
        </>
      }
    >
      <label className="muted">{t("runtimes.f.name")}</label>
      <Input
        value={form.name}
        onChange={(e) => set("name")(e.target.value)}
        disabled={!!editing}
        placeholder="codex"
        style={{ marginBottom: 10 }}
      />
      <label className="muted">{t("runtimes.f.harness")}</label>
      <Select
        value={form.harness}
        onChange={set("harness")}
        style={{ width: "100%", marginBottom: 10 }}
        options={HARNESSES.map((h) => ({ value: h, label: h }))}
      />
      <label className="muted">{t("runtimes.f.command")}</label>
      <Input
        value={form.command}
        onChange={(e) => set("command")(e.target.value)}
        placeholder="codex --acp"
        className="mono"
        style={{ marginBottom: 10 }}
      />
      <label className="muted">{t("runtimes.f.description")}</label>
      <Input
        value={form.description}
        onChange={(e) => set("description")(e.target.value)}
        style={{ marginBottom: 10 }}
      />
      <label className="muted">{t("runtimes.f.mcp")}</label>
      <Input
        value={form.mcp_profile}
        onChange={(e) => set("mcp_profile")(e.target.value)}
        placeholder="default"
        style={{ marginBottom: 10 }}
      />
      <label className="muted">{t("runtimes.f.models")}</label>
      <Input
        value={form.models}
        onChange={(e) => set("models")(e.target.value)}
        placeholder="glm-5.1, qwen3.7-max"
        style={{ marginBottom: 10 }}
      />
    </Modal>
  );
}

/** API harness names ("ClaudeCode") → form values ("claude-code"). */
const HARNESS_FOR: Record<string, string> = {
  ClaudeCode: "claude-code",
  OpenCode: "opencode",
  Dsh: "dsh",
  Mock: "mock",
};
