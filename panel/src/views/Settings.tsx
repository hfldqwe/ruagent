// Settings: platform behavior the user owns. First resident: the
// distillation policy (auto / mode / agent / language / prompt) —
// pulled out of the Memory page where it never belonged (a settings
// concern is not a memory-browsing concern).

import { Button, Input, Select, Switch } from "antd";
import { useEffect, useState } from "react";
import { api, type DistillPolicy } from "../api";
import { Spinner, useToast } from "../ui";
import { useI18n } from "../i18n";

export function Settings() {
  const { t } = useI18n();

  return (
    <div>
      <div className="view-bar">
        <h2>{t("settings.title")}</h2>
        <span className="muted">{t("settings.subtitle")}</span>
      </div>
      <DistillSettings />
    </div>
  );
}

// Distillation policy (the [distill] table of policy.toml): auto, the
// extraction agent, the output language, a full prompt override. Live —
// saving swaps the running daemon's policy, no restart.
function DistillSettings() {
  const { t } = useI18n();
  const toast = useToast();
  const [policy, setPolicy] = useState<DistillPolicy | null>(null);
  const [agents, setAgents] = useState<{ name: string }[]>([]);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    api.distillPolicy().then(setPolicy).catch(() => setPolicy(null));
    api
      .agents()
      .then((a) =>
        setAgents(a.filter((x) => x.enabled).map((x) => ({ name: x.name }))),
      )
      .catch(() => setAgents([]));
  }, []);

  if (!policy) return <Spinner label={t("common.loading")} />;
  const set = (patch: Partial<DistillPolicy>) =>
    setPolicy({ ...policy, ...patch });

  /** The textarea is pre-filled with the EFFECTIVE prompt (the override
   * when set, otherwise the built-in) — the user edits from the real
   * thing instead of writing a replacement blind (user request
   * 2026-09-20). Saving an unmodified/equal-to-builtin text clears the
   * override; Reset restores the built-in in one click. */
  const isCustom =
    !!policy.prompt && policy.prompt.trim() !== policy.builtin_prompt.trim();
  const effectivePrompt = policy.prompt || policy.builtin_prompt;

  const save = async () => {
    setSaving(true);
    try {
      await api.setDistillPolicy({
        auto: policy.auto,
        graph: policy.graph ?? true,
        agent: policy.agent ?? "",
        language: policy.language ?? "",
        prompt: isCustom ? policy.prompt ?? "" : "",
      });
      toast("ok", t("distill.saved"));
    } catch (e) {
      toast("err", String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="card distill-settings">
      <h3 className="sec">{t("distill.title")}</h3>
      <div className="row" style={{ justifyContent: "space-between" }}>
        <div style={{ minWidth: 0 }}>
          <strong>{t("distill.auto")}</strong>
          <p className="muted" style={{ margin: 0 }}>
            {t("distill.autoHint")}
          </p>
        </div>
        <Switch checked={policy.auto} onChange={(v) => set({ auto: v })} />
      </div>

      <div className="row" style={{ justifyContent: "space-between" }}>
        <div style={{ minWidth: 0 }}>
          <strong>{t("distill.graph")}</strong>
          <p className="muted" style={{ margin: 0 }}>
            {t("distill.graphHint")}
          </p>
        </div>
        <Switch
          checked={policy.graph ?? true}
          onChange={(v) => set({ graph: v })}
        />
      </div>

      <div className="row wrap">
        <label className="chat-field" style={{ flex: "1 1 220px" }}>
          <span>{t("distill.agent")}</span>
          <Select
            value={policy.agent ?? ""}
            onChange={(v) => set({ agent: v || null })}
            style={{ width: "100%" }}
            options={[
              { value: "", label: t("distill.agentDefault") },
              ...agents.map((a) => ({ value: a.name, label: a.name })),
            ]}
          />
        </label>
        <label className="chat-field" style={{ flex: "1 1 220px" }}>
          <span>{t("distill.language")}</span>
          <Input
            value={policy.language ?? ""}
            placeholder={t("distill.languagePh")}
            onChange={(e) => set({ language: e.target.value })}
          />
        </label>
      </div>

      <label className="chat-field">
        <span>
          {t("distill.prompt")}
          {isCustom ? (
            <span className="tag" style={{ marginLeft: 8 }}>
              {t("distill.customized")}
            </span>
          ) : null}
        </span>
        <Input.TextArea
          rows={7}
          value={effectivePrompt}
          onChange={(e) => set({ prompt: e.target.value })}
        />
        <span className="field-hint">{t("distill.promptHint")}</span>
      </label>

      <div className="row end">
        {isCustom ? (
          <Button onClick={() => set({ prompt: "" })}>
            {t("distill.promptReset")}
          </Button>
        ) : null}
        <Button type="primary" loading={saving} onClick={save}>
          {t("distill.save")}
        </Button>
      </div>
    </div>
  );
}
