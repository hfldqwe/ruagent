// Runtimes view — the execution backends (design §4.1 two-layer model,
// user ruling 2026-09-17: claude-code / dsh / opencode are RUNTIMES, a
// sibling page to the Agents view; agents are portable roles). This is
// the connection-management surface: list, create, edit and delete
// backends — every edit goes through the daemon, which round-trips
// agents.toml (comments preserved) and hot-reloads the registry.

import { useEffect, useState } from "react";
import { Button, Card, Input, Popconfirm, Select } from "antd";
import { api, isRoleAgent, type AgentInfo, type SessionOptionInfo } from "../api";
import { brandClass, BrandMark } from "../brand";
import { Empty, ErrorState, Modal, ReadoutStrip, Spinner, useToast } from "../ui";
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
  // Returns whether the probe succeeded: R11 wants a visible failure, not a
  // silent no-op (this catch used to swallow it).
  const sync = async (name: string) => {
    setSyncing(name);
    try {
      const o = await api.agentOptions(name, true);
      setCounts((c) => ({ ...c, [name]: modelOption(o.options)?.choices.length ?? 0 }));
      return true;
    } catch {
      return false;
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
  const [err, setErr] = useState<unknown>(null);
  const [probeErr, setProbeErr] = useState<Record<string, boolean>>({});
  const [editing, setEditing] = useState<AgentInfo | null>(null);
  const [creating, setCreating] = useState(false);
  const toast = useToast();

  // 行 20: a failed read is not "no runtimes registered". The previous
  // `catch(() => setAgents([]))` rendered the empty state on a broken daemon.
  const load = () =>
    api
      .agents()
      .then((v) => {
        setAgents(v);
        setErr(null);
      })
      .catch((e) => setErr(e));
  // The registry only changes when this page writes to it, and every write
  // reloads explicitly — a 5s poll here was pure traffic (3 GET /agents per
  // 11.5s window, F2's class of defect). Returning to the tab re-reads.
  useEffect(() => {
    load();
    const onVisible = () => {
      if (!document.hidden) load();
    };
    document.addEventListener("visibilitychange", onVisible);
    return () => document.removeEventListener("visibilitychange", onVisible);
  }, []);

  // Model-count chips per runtime, from the daemon's cached catalog.
  // Runs before the early return (hook order), names derived from state.
  const { counts, syncing, sync } = useModelCounts(
    (agents ?? []).filter((a) => !isRoleAgent(a) && a.enabled).map((a) => a.name),
  );

  if (agents === null) {
    if (err) {
      return (
        <>
          <h1 className="sr-only">{t("runtimes.title")}</h1>
          <ErrorState
            title={t("runtimes.err")}
            hint={t("runtimes.err.hint")}
            onRetry={load}
            retryLabel={t("common.retry")}
          />
        </>
      );
    }
    return <Spinner label={`${t("runtimes.title")}…`} />;
  }

  const roles = agents.filter(isRoleAgent);
  // Disabled runtimes are configured off — the mock agent template for
  // instance never belongs on the user surface.
  const runtimes = agents
    .filter((a) => !isRoleAgent(a) && a.enabled)
    // Stable order across the 5s poll (§5 密集): harness, then name.
    .sort(
      (a, b) =>
        (a.harness ?? "").localeCompare(b.harness ?? "") || a.name.localeCompare(b.name),
    );

  const remove = async (name: string) => {
    try {
      await api.deleteRuntime(name);
      toast("ok", t("runtimes.deleted", { name }));
      load();
    } catch (e) {
      toast("err", String(e));
    }
  };

  const probe = async (name: string) => {
    const ok = await sync(name);
    setProbeErr((m) => ({ ...m, [name]: !ok }));
  };

  return (
    <div>
      <h1 className="sr-only">{t("runtimes.title")}</h1>
      <div className="view-bar">
        <h2>{t("runtimes.title")}</h2>
        <span className="muted">{t("runtimes.subtitle")}</span>
        <span className="grow" />
        <Button type="primary" onClick={() => setCreating(true)}>
          + {t("runtimes.create")}
        </Button>
      </div>

      {err && runtimes.length > 0 ? (
        <ErrorState
          title={t("common.stale")}
          hint={t("runtimes.stale.hint")}
          onRetry={load}
          retryLabel={t("common.retry")}
        />
      ) : null}

      {runtimes.length > 0 && (
        // `grid` -> .readout-strip.grid (README §3.4 X4). The rule lives in
        // index.css; the view only asks for the variant.
        <ReadoutStrip
          grid
          items={[
            { key: "runtimes", label: t("runtimes.title"), value: runtimes.length },
            { key: "roles", label: t("agents.title"), value: roles.length },
            {
              key: "models",
              label: t("metric.models"),
              value: Object.values(counts).reduce((s, n) => s + n, 0),
            },
          ]}
        />
      )}
      {runtimes.length === 0 ? (
        <Empty
          icon="layers"
          title={t("agents.runtimes.empty")}
          hint={t("agents.runtimes.hint")}
          action={
            <Button type="primary" onClick={() => setCreating(true)}>
              + {t("runtimes.create")}
            </Button>
          }
        />
      ) : (
        <div className="agent-grid">
          {runtimes.map((r) => {
            // §4: derived, no endpoint — a role reaches a runtime either
            // through `runtime` or through the `runtimes` list.
            const usedBy = roles.filter(
              (a) => a.runtime === r.name || a.runtimes?.includes(r.name),
            );
            return (
              <Card key={r.name} className="agent-card" size="small">
                <div className="row">
                  {/* Brand logo tile — the runtime's own mark on a tint of
                      its brand color (mock and unknowns stay neutral).
                      The mark must NOT be `mono` for a branded harness:
                      `mono` forces it into the inherited text color, which
                      is why every tile collapsed into one grade and lost the
                      separation the tint exists for (t13 F1). The tint and its
                      measured contrast belong to the palette tokens and to
                      MASTER §12's contrast rows, which is where the audit
                      prints them, so this comment does not carry a copy.
                      `mono` now means "no brand" only. */}
                  <span
                    className={`runtime-logo${brandClass(r.harness) ? " " + brandClass(r.harness) : ""}`}
                  >
                    <BrandMark
                      harness={r.harness}
                      size={20}
                      mono={!brandClass(r.harness)}
                      fallback="bot"
                    />
                  </span>
                  <div>
                    {/* §4: the harness is the identity here; `name` is the
                        local alias and only shows when the two differ. */}
                    <strong>{r.harness}</strong>
                    {r.name !== r.harness ? (
                      <div className="muted mono">{r.name}</div>
                    ) : null}
                  </div>
                  <span className="grow" />
                  {r.enabled ? (
                    <span className="tag ok">{t("agents.enabled")}</span>
                  ) : (
                    <span className="tag">{t("agents.disabled")}</span>
                  )}
                </div>
                <p className="muted" style={{ margin: "10px 0 6px" }}>
                  {r.description}
                </p>
                {r.command ? (
                  <div className="row">
                    <span className="doc-icon">
                      <Icon name="zap" size={13} />
                    </span>
                    {/* Full value on hover; 12/16 comes from .mono itself
                        (the inline font-size:12 that used to sit here was
                        both redundant and a 行 11 violation). */}
                    <span className="muted mono truncated" title={r.command}>
                      {r.command}
                    </span>
                  </div>
                ) : null}
                <div className="row">
                  {counts[r.name] ? (
                    <span className="tag">{t("runtimes.models", { n: counts[r.name] })}</span>
                  ) : (
                    // "not probed" is not "0 models" (§5 empty ②).
                    <span className="muted">{t("runtimes.notProbed")}</span>
                  )}
                  <Button
                    loading={syncing === r.name}
                    onClick={() => probe(r.name)}
                    aria-label={t("chat.sync")}
                    title={t("chat.sync")}
                  >
                    <Icon name="sync" size={13} />
                  </Button>
                  {probeErr[r.name] ? (
                    <span className="tag err" role="alert">
                      {t("runtimes.probeFailed")}
                    </span>
                  ) : null}
                  <span className="grow" />
                </div>
                <div className="row wrap">
                  {usedBy.length > 0 ? (
                    <>
                      {/* Capped: a runtime referenced by 20 roles must not
                          stretch the card (§4 密度上限). */}
                      {usedBy.slice(0, 5).map((a) => (
                        <span key={a.name} className="tag">
                          {a.name}
                        </span>
                      ))}
                      {usedBy.length > 5 ? (
                        <span className="tag">+{usedBy.length - 5}</span>
                      ) : null}
                    </>
                  ) : (
                    <span className="muted">{t("agents.runtimes.noRoles")}</span>
                  )}
                  <span className="grow" />
                </div>
                <div className="row end">
                  <Button
                    onClick={() => {
                      window.location.hash = `chat?agent=${encodeURIComponent(r.name)}`;
                    }}
                  >
                    {t("agents.runtimes.direct")}
                  </Button>
                  <Button onClick={() => setEditing(r)}>{t("common.edit")}</Button>
                  <Popconfirm
                    title={t("runtimes.deleteConfirm.title", { name: r.name })}
                    description={
                      // R9: deleting a runtime breaks every role that names
                      // it — the confirm says which ones.
                      usedBy.length > 0
                        ? `${t("runtimes.deleteConfirm.body")} ${t("runtimes.usedBy", {
                            names: usedBy.map((a) => a.name).join(", "),
                          })}`
                        : t("runtimes.deleteConfirm.body")
                    }
                    okText={t("common.delete")}
                    cancelText={t("common.keep")}
                    okButtonProps={{ danger: true }}
                    onConfirm={() => remove(r.name)}
                  >
                    <Button danger>{t("common.delete")}</Button>
                  </Popconfirm>
                </div>
              </Card>
            );
          })}
        </div>
      )}
      <p className="muted pad" style={{ marginTop: 16 }}>{t("runtimes.hint")}</p>
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
