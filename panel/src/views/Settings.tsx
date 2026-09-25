// Settings: platform behavior the user owns. First resident: the
// distillation policy (auto / graph / agent / language / prompt) —
// pulled out of the Memory page where it never belonged (a settings
// concern is not a memory-browsing concern).

import { Button, Input, Select, Switch } from "antd";
import { useEffect, useState } from "react";
import { api, type DistillPolicy } from "../api";
import { ErrorState, Spinner } from "../ui";
import { useI18n } from "../i18n";

export function Settings() {
  const { t } = useI18n();

  return (
    <div>
      <h1 className="sr-only">{t("settings.title")}</h1>
      <div className="view-bar">
        <h2>{t("settings.title")}</h2>
        <span className="muted">{t("settings.subtitle")}</span>
      </div>
      <DistillSettings />
    </div>
  );
}

// Distillation policy (the [distill] table of policy.toml): auto, the graph
// flag, the extraction agent, the output language, a full prompt override.
// Live — saving swaps the running daemon's policy, no restart.
function DistillSettings() {
  const { t } = useI18n();
  const [policy, setPolicy] = useState<DistillPolicy | null>(null);
  const [err, setErr] = useState<unknown>(null);
  const [agents, setAgents] = useState<{ name: string }[] | null>(null);
  const [saving, setSaving] = useState(false);
  const [savedAt, setSavedAt] = useState<string | null>(null);
  const [saveErr, setSaveErr] = useState<string | null>(null);

  // 行 20: a failed read used to land in the same `null` as "still loading",
  // so a broken daemon showed a spinner forever.
  const load = () => {
    api
      .distillPolicy()
      .then((p) => {
        setPolicy(p);
        setErr(null);
      })
      .catch((e) => setErr(e));
    api
      .agents()
      .then((a) => setAgents(a.filter((x) => x.enabled).map((x) => ({ name: x.name }))))
      .catch(() => setAgents([]));
  };
  useEffect(() => {
    load();
  }, []);

  if (!policy) {
    if (err) {
      return (
        <>
          <h1 className="sr-only">{t("settings.title")}</h1>
          <ErrorState
            title={t("distill.err")}
            hint={t("distill.err.hint")}
            onRetry={load}
            retryLabel={t("common.retry")}
          />
        </>
      );
    }
    return <Spinner label={t("common.loading")} />;
  }

  const set = (patch: Partial<DistillPolicy>) => setPolicy({ ...policy, ...patch });

  /** The textarea is pre-filled with the EFFECTIVE prompt (the override
   * when set, otherwise the built-in) — the user edits from the real
   * thing instead of writing a replacement blind (user request
   * 2026-09-20). Saving an unmodified/equal-to-builtin text clears the
   * override; Reset restores the built-in in one click. */
  const isCustom = !!policy.prompt && policy.prompt.trim() !== policy.builtin_prompt.trim();
  const effectivePrompt = policy.prompt || policy.builtin_prompt;

  // §4: `graph` is three-valued. null means "no key in policy.toml — the
  // daemon's own default (true) applies"; it must be visible AND reachable,
  // so an untouched null is sent as an absent key rather than being written
  // over as `true` on every save (which is what the old body did).
  const graphLabel =
    policy.graph == null
      ? t("distill.graphFollow")
      : policy.graph
        ? t("distill.on")
        : t("distill.off");

  const save = async () => {
    setSaving(true);
    setSaveErr(null);
    try {
      await api.setDistillPolicy({
        auto: policy.auto,
        // undefined drops the key from the JSON body -> the daemon clears
        // `[distill] graph` and falls back to its default.
        graph: policy.graph ?? undefined,
        agent: policy.agent ?? "",
        language: policy.language ?? "",
        prompt: isCustom ? (policy.prompt ?? "") : "",
      });
      // G13: the confirmation is the line that stays (「已保存 hh:mm:ss」).
      // There is deliberately NO toast here — G13 asks for a persistent
      // line INSTEAD of a 2s toast, and keeping both left the old comment
      // contradicting the code (t13 F5).
      setSavedAt(new Date().toLocaleTimeString());
    } catch (e) {
      // G13: a failed save keeps every edit on screen and says so.
      setSaveErr(String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="card distill-settings">
      {/* §3: the first question on a settings page is "what is it set to
          right now". Three real fields at the SECONDARY readout grade
          (18/24, .readout.s) — not a 32px dashboard: this page has no
          instrumentation. .readout-strip gives the dividing rules without
          the .panel box (MASTER P4). */}
      <div className="readout-strip">
        {[
          {
            k: "auto",
            label: t("distill.title"),
            value: policy.auto ? t("distill.autoOn") : t("distill.autoOff"),
          },
          { k: "graph", label: t("distill.graph"), value: graphLabel },
          {
            k: "agent",
            label: t("distill.agent"),
            value: policy.agent || t("distill.agentUnset"),
          },
        ].map((r) => (
          <div key={r.k} className="readout-cell">
            <span className="readout s">{r.value}</span>
            <span className="readout-label">{r.label}</span>
          </div>
        ))}
      </div>

      <div className="zone-head">
        <div className="zone-title">{t("distill.title")}</div>
      </div>

      <div className="row" style={{ justifyContent: "space-between" }}>
        <div style={{ minWidth: 0 }}>
          <strong>{t("distill.auto")}</strong>
          <p className="muted" style={{ margin: 0 }}>
            {t("distill.autoHint")}
          </p>
        </div>
        <Switch
          aria-label={t("distill.auto")}
          checked={policy.auto}
          onChange={(v) => set({ auto: v })}
        />
      </div>

      <div className="row" style={{ justifyContent: "space-between" }}>
        <div style={{ minWidth: 0 }}>
          <strong>{t("distill.graph")}</strong>
          <p className="muted" style={{ margin: 0 }}>
            {t("distill.graphHint")}
          </p>
        </div>
        <span className="row tight">
          {/* G11: "follow the default" is a state of its own — shown while it
              is active, and one click away while it is not. */}
          {policy.graph == null ? (
            <span className="tag">{t("distill.graphFollow")}</span>
          ) : (
            <Button type="link" onClick={() => set({ graph: null })}>
              {t("distill.graphFollow")}
            </Button>
          )}
          <Switch
            aria-label={t("distill.graph")}
            checked={policy.graph ?? true}
            onChange={(v) => set({ graph: v })}
          />
        </span>
      </div>

      <div className="row wrap">
        <label className="chat-field" style={{ flex: "1 1 220px" }}>
          <span>{t("distill.agent")}</span>
          <Select
            aria-label={t("distill.agent")}
            value={policy.agent ?? ""}
            onChange={(v) => set({ agent: v || null })}
            style={{ width: "100%" }}
            options={[
              { value: "", label: t("distill.agentDefault") },
              ...(agents ?? []).map((a) => ({ value: a.name, label: a.name })),
            ]}
            // §5 empty: no enabled role must say so, not open an empty list.
            // The string belongs to THIS view's domain: a settings-only
            // message keyed under distill.* is copy that outlives its owner.
            notFoundContent={t("settings.noRoles")}
          />
        </label>
        <label className="chat-field" style={{ flex: "1 1 220px" }}>
          <span>{t("distill.language")}</span>
          <Input
            aria-label={t("distill.language")}
            value={policy.language ?? ""}
            placeholder={t("distill.languagePh")}
            onChange={(e) => set({ language: e.target.value })}
          />
        </label>
      </div>

      <label className="chat-field">
        <span>
          {t("distill.prompt")}
          {/* §4: which of the two fields is in force must be readable. */}
          <span className="tag" style={{ marginLeft: 8 }}>
            {isCustom ? t("distill.customized") : t("distill.builtinTag")}
          </span>
        </span>
        <Input.TextArea
          aria-label={t("distill.prompt")}
          rows={7}
          value={effectivePrompt}
          onChange={(e) => set({ prompt: e.target.value })}
        />
        <span className="field-hint">{t("distill.promptHint")}</span>
      </label>

      {/* G12: builtin_prompt was invisible — the user could not see what the
          "built-in" they fall back to actually says. Read-only disclosure. */}
      <details className="prompt-view">
        <summary>{t("distill.builtin")}</summary>
        <pre>{policy.builtin_prompt}</pre>
      </details>

      <div className="row wrap end">
        {savedAt ? (
          <span className="muted">
            {t("distill.saved")} · {savedAt}
          </span>
        ) : null}
        {isCustom ? (
          <Button onClick={() => set({ prompt: "" })}>{t("distill.promptReset")}</Button>
        ) : null}
        <Button type="primary" loading={saving} onClick={save}>
          {t("distill.save")}
        </Button>
      </div>

      {saveErr ? <ErrorState title={t("distill.saveFailed")} hint={saveErr} /> : null}
    </div>
  );
}
