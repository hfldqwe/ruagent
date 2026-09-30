// Settings: platform behavior the user owns. First resident: the
// distillation policy (auto / graph / agent / language / prompt) —
// pulled out of the Memory page where it never belonged (a settings
// concern is not a memory-browsing concern). Second resident (t7): the
// capability plane, so a pipeline is switched here instead of by
// hand-editing the [capabilities] table of policy.toml.

import { Alert, Button, Input, Select, Switch, Tooltip } from "antd";
import { useEffect, useState } from "react";
import {
  api,
  type CapabilitiesResponse,
  type CapabilityFileEntry,
  type CapabilityOptions,
  type CapabilityRow,
  type DistillPolicy,
} from "../api";
import { ErrorState, Modal, Spinner } from "../ui";
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
      <CapabilitiesSettings />
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

// ---------------------------------------------------------------------------
// The capability plane (the [capabilities] table of policy.toml)
// ---------------------------------------------------------------------------

/** The option keys a capability may declare, in the registry's order (design
 *  §4.2). A key a capability does not declare never appears in its `options` —
 *  the daemon refuses it as an unknown key rather than ignoring it. */
const OPTION_KEYS: (keyof CapabilityOptions)[] = [
  "weight",
  "min_score",
  "max_per_input",
  "min_confidence",
  "max_docs_per_pass",
];

/** The localised NAME of one capability, or the raw id for an id this build has
 *  never heard of (a daemon that grew a capability after this panel was built):
 *  `t` falls back to the key itself, which is right for a missing string and
 *  wrong for a name — the id is at least a name the daemon answers to. */
function capabilityName(t: (key: string) => string, id: string): string {
  const name = t(`settings.capabilities.name.${id}`);
  return name.startsWith("settings.capabilities.name.") ? id : name;
}

/** A row's resolved options, as the `[capabilities.<id>]` keys it declared.
 *  Only non-null ones: `null` means "not set", and re-writing it as an explicit
 *  value would turn a default into an opinion. */
function declaredOptions(row: CapabilityRow): CapabilityFileEntry {
  const out: CapabilityFileEntry = {};
  const o = row.options;
  if (!o) return out;
  for (const k of OPTION_KEYS) {
    const v = o[k];
    if (v != null) out[k] = v;
  }
  return out;
}

/** The same values as one line — "weight 1.5 · min_score 0.25". The keys are
 *  the API's own names, shown verbatim: they are exactly what the user would
 *  otherwise be editing in policy.toml. */
function optionsLine(row: CapabilityRow): string {
  const o = row.options;
  if (!o) return "";
  return OPTION_KEYS.filter((k) => o[k] != null)
    .map((k) => `${k} ${o[k]}`)
    .join(" · ");
}

/** The `PUT /api/v1/capabilities` body for ONE switch flip.
 *
 *  READ-MODIFY-WRITE, because the daemon's `CapabilitiesEditor::update`
 *  (crates/daemon/src/config.rs:1079-1114) REPLACES the whole `[capabilities]`
 *  table: a row absent from this body is DELETED from policy.toml, and inside a
 *  row `set_or_remove_*` (:1100-1105) removes every key the body omits. A body
 *  carrying only the flipped row would therefore reset every other configured
 *  row's `weight`/`min_score` and drop its `enabled` key — one switch flip, a
 *  silently edited config file.
 *
 *  So every row the file already configures (`configured === "file"`) is
 *  re-emitted with its resolved options, and only the flipped row changes
 *  value. A row nobody has written (`configured !== "file"`) is left OUT on
 *  purpose: it resolves to its registry default, which is the value `GET`
 *  reported and the value the switch shows. */
function capabilitiesBody(
  data: CapabilitiesResponse,
  id: string,
  enabled: boolean,
): Record<string, CapabilityFileEntry> {
  const out: Record<string, CapabilityFileEntry> = {};
  for (const row of data.capabilities) {
    const changing = row.id === id;
    if (!changing && row.configured !== "file") continue;
    out[row.id] = { enabled: changing ? enabled : row.enabled, ...declaredOptions(row) };
  }
  return out;
}

/** One switch per capability.
 *
 *  There is no Save button here, unlike the distill card above: a capability is
 *  one independent boolean and the daemon's cost gate is per capability
 *  (enabling an llm-tier one has to be confirmed on its own). Each flip is ONE
 *  `PUT`, and the switches render the daemon's ANSWER — never a local guess —
 *  so a refused write leaves the switch where the config file is and shows the
 *  daemon's own message. */
function CapabilitiesSettings() {
  const { t } = useI18n();
  const [data, setData] = useState<CapabilitiesResponse | null>(null);
  const [err, setErr] = useState<unknown>(null);
  /** The row whose PUT is in flight; all switches are locked meanwhile, so two
   *  whole-table writes can never interleave and lose one another's change. */
  const [pending, setPending] = useState<string | null>(null);
  /** The llm-tier row waiting for the cost confirmation. Not applied yet. */
  const [armed, setArmed] = useState<CapabilityRow | null>(null);
  const [savedAt, setSavedAt] = useState<string | null>(null);
  const [saveErr, setSaveErr] = useState<string | null>(null);

  const load = () => {
    api
      .capabilities()
      .then((r) => {
        setData(r);
        setErr(null);
      })
      .catch((e) => setErr(e));
  };
  useEffect(() => {
    load();
  }, []);

  if (!data) {
    if (err) {
      return (
        <ErrorState
          title={t("settings.capabilities.err")}
          hint={`${t("settings.capabilities.errHint")} (${
            err instanceof Error ? err.message : String(err)
          })`}
          onRetry={load}
          retryLabel={t("common.retry")}
        />
      );
    }
    return <Spinner label={t("common.loading")} />;
  }

  const apply = async (row: CapabilityRow, enabled: boolean, confirmCost: boolean) => {
    setPending(row.id);
    setSaveErr(null);
    try {
      const next = await api.setCapabilities({
        confirm_cost: confirmCost,
        capabilities: capabilitiesBody(data, row.id, enabled),
      });
      // The daemon answers a PUT with the same payload as the GET, so the
      // response — not an optimistic local edit — is what the switches show.
      setData(next);
      setSavedAt(new Date().toLocaleTimeString());
    } catch (e) {
      // The daemon's message, verbatim: 400 for an id/key/value it refuses,
      // 409 for an llm-tier enable without the cost confirmation, and the
      // write error otherwise. The switch keeps the server's value.
      setSaveErr(String(e));
    } finally {
      setPending(null);
    }
  };

  const flip = (row: CapabilityRow, next: boolean) => {
    // Spending tokens is a deliberate act: an llm-tier row goes through the
    // cost confirmation (and the daemon refuses the PUT without confirm_cost
    // anyway), while every free-tier flip is immediate.
    if (next && row.tier === "llm") {
      setSaveErr(null);
      setArmed(row);
      return;
    }
    void apply(row, next, false);
  };

  return (
    <div className="card capabilities-settings">
      <div className="zone-head">
        <div className="zone-title">{t("settings.capabilities.title")}</div>
        {/* Where the current values come from. "Nothing written to policy.toml"
            is the default configuration, i.e. exactly today's behaviour (L1) —
            the one fact a reader of this card must not have to guess. */}
        <span className="zone-note">
          {data.table_present
            ? t("settings.capabilities.sourceFile")
            : t("settings.capabilities.sourceDefault")}
        </span>
      </div>

      <p className="muted micro" style={{ margin: 0 }}>
        {t("settings.capabilities.hint")}
      </p>

      {/* What this configuration SUPPRESSES. The daemon's `reason` is its own
          sentence and is shown verbatim, like every other string it owns. */}
      {data.conflicts.length ? (
        <Alert
          type="warning"
          showIcon
          message={t("settings.capabilities.conflictTitle")}
          description={
            <ul style={{ margin: 0, paddingInlineStart: 18 }}>
              {data.conflicts.map((c) => (
                <li key={`${c.id}:${c.legacy_key}`}>
                  <span className="mono">{c.id}</span>{" "}
                  <span className="muted">({c.legacy_key})</span> — {c.reason}
                </li>
              ))}
            </ul>
          }
        />
      ) : null}

      {data.capabilities.map((row) => {
        const name = capabilityName(t, row.id);
        const differs = row.enabled !== row.default_enabled;
        const options = optionsLine(row);
        return (
          <div key={row.id} data-capability={row.id}>
            <div className="row" style={{ justifyContent: "space-between" }}>
              <div style={{ minWidth: 0 }}>
                <div className="row tight" style={{ padding: 0 }}>
                  <strong>{name}</strong>
                  <Tooltip
                    title={
                      row.tier === "llm"
                        ? t("settings.capabilities.tier.llmHint")
                        : t("settings.capabilities.tier.freeHint")
                    }
                  >
                    <span className={row.tier === "llm" ? "tag warn" : "tag"}>
                      {row.tier === "llm"
                        ? t("settings.capabilities.tier.llm")
                        : t("settings.capabilities.tier.free")}
                    </span>
                  </Tooltip>
                  {differs ? (
                    <span className="tag">{t("settings.capabilities.notDefault")}</span>
                  ) : null}
                  {row.new ? <span className="tag">{t("settings.capabilities.newBadge")}</span> : null}
                </div>
                <p className="muted" style={{ margin: 0 }}>
                  {row.description}
                </p>
                <span className="muted micro">{row.gates}</span>
              </div>
              <Switch
                aria-label={name}
                checked={row.enabled}
                loading={pending === row.id || armed?.id === row.id}
                disabled={pending !== null}
                onChange={(v) => flip(row, v)}
              />
            </div>
            {/* The cost warning is visible BEFORE the switch is touched — the
                confirmation dialog is the second gate, not the only one. */}
            {row.tier === "llm" ? (
              <p style={{ margin: "0 4px 6px" }}>
                <span className="tag warn">{t("settings.capabilities.costWarning")}</span>
              </p>
            ) : null}
            {options ? (
              <p className="muted micro" style={{ margin: "0 4px 6px" }}>
                {options}
              </p>
            ) : null}
          </div>
        );
      })}

      <div className="row end">
        {savedAt ? (
          <span className="muted micro">
            {t("settings.capabilities.saved")} · {savedAt}
          </span>
        ) : null}
      </div>

      {saveErr ? (
        <ErrorState title={t("settings.capabilities.saveFailed")} hint={saveErr} />
      ) : null}

      {armed ? (
        <Modal
          title={t("settings.capabilities.costTitle")}
          onClose={() => setArmed(null)}
          footer={
            <>
              <Button onClick={() => setArmed(null)}>{t("common.cancel")}</Button>
              <Button
                type="primary"
                danger
                onClick={() => {
                  const row = armed;
                  setArmed(null);
                  void apply(row, true, true);
                }}
              >
                {t("settings.capabilities.costConfirm")}
              </Button>
            </>
          }
        >
          <p>{t("settings.capabilities.costBody", { name: capabilityName(t, armed.id) })}</p>
          <p className="muted micro" style={{ marginBottom: 0 }}>
            {armed.description}
          </p>
        </Modal>
      ) : null}
    </div>
  );
}
