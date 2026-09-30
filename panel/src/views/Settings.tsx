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
  type CapabilityRow,
  type DistillPolicy,
  type OptionSchemaEntry,
} from "../api";
import {
  capabilitiesBody,
  editedOptions,
  fileCarries,
  fileOptions,
  optionValue,
  preservedEnabled,
  RESET_OPTIONS,
  validateOptionValue,
  zeroWeightEnabled,
  type OptionRefusal,
  type RowEdit,
} from "../capability-options";
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

/** The localised NAME of one capability, or the raw id for an id this build has
 *  never heard of (a daemon that grew a capability after this panel was built):
 *  `t` falls back to the key itself, which is right for a missing string and
 *  wrong for a name — the id is at least a name the daemon answers to. */
function capabilityName(t: (key: string) => string, id: string): string {
  const name = t(`settings.capabilities.name.${id}`);
  return name.startsWith("settings.capabilities.name.") ? id : name;
}

/** The state key of one input: the row id and the option key, separated by a
 *  character no capability id contains, so "drop this row's drafts" is a prefix
 *  test rather than a second nested map. The key is a STRING (the daemon's own
 *  key name from `options_schema`), not a union: the panel does not own that set. */
function draftKey(rowId: string, key: string): string {
  return `${rowId}\u0000${key}`;
}

/** A draft/refusal map without one row's entries. After a write the inputs must
 *  render the daemon's ANSWER; after a FAILED write nothing is dropped, so a
 *  typo the daemon refused is still in front of the user. */
function dropRow<V>(map: Record<string, V>, rowId: string): Record<string, V> {
  const prefix = `${rowId}\u0000`;
  const out: Record<string, V> = {};
  for (const [k, v] of Object.entries(map)) if (!k.startsWith(prefix)) out[k] = v;
  return out;
}

/** The daemon's OWN message, verbatim: `String(err)` would prefix "Error: ",
 *  which is the panel's word, not the daemon's. A `PUT` 400 names the
 *  capability, the key and the range it wanted, and that sentence is the one
 *  worth reading — it is not summarised or reworded anywhere in this card. */
function daemonMessage(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/** Why one input was refused, in the reader's language. The RANGE in every one of
 *  these messages is the daemon's OWN `expectation` phrase, taken from the schema
 *  entry and never re-worded — so the sentence a user reads here is the sentence a
 *  400 would carry, and the two cannot drift. The single exception is `zeroWeight`,
 *  the cross-field rule (see `zeroWeightEnabled`), which has no per-key phrase. */
function refusalText(
  t: (key: string, params?: Record<string, string | number>) => string,
  entry: OptionSchemaEntry,
  refusal: OptionRefusal,
): string {
  if (refusal === "zeroWeight") return t("settings.capabilities.options.zeroWeight");
  if (refusal === "notFinite") {
    return t("settings.capabilities.options.notFinite", { expectation: entry.expectation });
  }
  if (refusal === "notInteger") {
    return t("settings.capabilities.options.notInteger", { expectation: entry.expectation });
  }
  return t("settings.capabilities.options.outOfRange", { expectation: entry.expectation });
}

/** One row per capability: the enable switch, the regression warning, and an
 *  editor for every option value the API reports for it.
 *
 *  There is no Save button for the SWITCHES, unlike the distill card above: a
 *  capability is one independent boolean and the daemon's cost gate is per
 *  capability (enabling an llm-tier one has to be confirmed on its own). Each
 *  flip is ONE `PUT`, and the switches render the daemon's ANSWER — never a
 *  local guess — so a refused write leaves the switch where the config file is
 *  and shows the daemon's own message.
 *
 *  The OPTION inputs work the same way, with one addition: Apply validates the
 *  touched fields against the daemon's own SCHEMA (`options_schema`: `kind`, `min`,
 *  `max`, and the daemon's own `expectation` phrase, which is what a refusal
 *  prints) BEFORE the request, so a value that could not be stored is never sent,
 *  and a 400 that happens anyway is printed verbatim. The editor renders one input
 *  per DECLARED key — including a key with no registry default, which the resolved
 *  `options` alone can never reach — and every write is a whole-table
 *  read-modify-write that re-emits exactly the keys `options_set` says the FILE
 *  carries (see `capabilitiesBody`), because the daemon's editor DELETES what a
 *  body omits. */
function CapabilitiesSettings() {
  const { t } = useI18n();
  const [data, setData] = useState<CapabilitiesResponse | null>(null);
  const [err, setErr] = useState<unknown>(null);
  /** The row whose PUT is in flight; all switches AND all option editors are
   *  locked meanwhile, so two whole-table writes can never interleave and lose
   *  one another's change. */
  const [pending, setPending] = useState<string | null>(null);
  /** The llm-tier row waiting for the cost confirmation. Not applied yet. */
  const [armed, setArmed] = useState<CapabilityRow | null>(null);
  /** Draft TEXT per input (`draftKey`), for fields the user has touched but not
   *  applied. Text, not numbers: a half-typed "0." must not be normalised under
   *  the cursor, and `type="number"` would turn an unrepresentable literal into
   *  an EMPTY value — which this card reads as "unset", i.e. "remove the key". */
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  /** The refusal per input, filled by Apply BEFORE any request: a value the
   *  daemon would reject never reaches the wire, so there is no HTTP error to
   *  show for it — the field itself carries the reason. */
  const [refusals, setRefusals] = useState<Record<string, OptionRefusal>>({});
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

  const apply = async (row: CapabilityRow, edit: RowEdit, confirmCost: boolean) => {
    setPending(row.id);
    setSaveErr(null);
    try {
      const next = await api.setCapabilities({
        confirm_cost: confirmCost,
        capabilities: capabilitiesBody(data, row.id, edit),
      });
      // The daemon answers a PUT with the same payload as the GET, so the
      // response — not an optimistic local edit — is what the card shows.
      setData(next);
      // Only a SUCCESSFUL write drops this row's drafts: the fields then show
      // the values policy.toml really holds. A refused write keeps them, so
      // nothing the user typed is lost by the refusal.
      setDrafts((d) => dropRow(d, row.id));
      setRefusals((r) => dropRow(r, row.id));
      setSavedAt(new Date().toLocaleTimeString());
    } catch (e) {
      // The daemon's message, verbatim: 400 for an id/key/value it refuses,
      // 409 for an llm-tier enable without the cost confirmation, and the
      // write error otherwise. The row keeps the server's values.
      setSaveErr(daemonMessage(e));
    } finally {
      setPending(null);
    }
  };

  const flip = (row: CapabilityRow, next: boolean) => {
    // Spending tokens is a deliberate act: an llm-tier row goes through the
    // cost confirmation (and the daemon refuses the PUT without confirm_cost
    // anyway), while every free-tier flip is immediate. The row's option keys are
    // re-emitted EXACTLY as the file carries them: a flip is not a reason to reset
    // them, and it is not a reason to write a registry default in either.
    if (next && row.tier === "llm") {
      setSaveErr(null);
      setArmed(row);
      return;
    }
    void apply(row, { enabled: next, options: fileOptions(row) }, false);
  };

  /** Apply the touched inputs of ONE row as one write.
   *
   *  Validation runs here, before the request, and comes FROM the daemon's schema
   *  (`validateOptionValue` reads `kind`/`min`/`max` off `options_schema`): every
   *  touched field must pass, plus the cross-field weight rule, or the whole row
   *  is refused locally and NOTHING is sent — a partial write would be worse than
   *  no write, because the daemon's body is the whole table. */
  const applyOptions = async (row: CapabilityRow) => {
    const values: Record<string, number | null> = {};
    const refused: Record<string, OptionRefusal> = {};
    let touched = false;
    for (const entry of row.options_schema ?? []) {
      const text = drafts[draftKey(row.id, entry.key)];
      if (text === undefined) continue;
      touched = true;
      const checked = validateOptionValue(entry, text);
      if (checked.ok) values[entry.key] = checked.value;
      else refused[draftKey(row.id, entry.key)] = checked.refusal;
    }
    if (Object.keys(refused).length) {
      setRefusals(refused);
      setSaveErr(null);
      return;
    }
    if (!touched) return;
    // An OPTION edit re-states the row's flag only when the file already carries
    // it; otherwise it leaves `enabled` alone rather than persisting a default.
    const edit: RowEdit = { enabled: preservedEnabled(row), options: editedOptions(row, values) };
    // The one rule the per-key schema cannot express (see `zeroWeightEnabled`),
    // reported on the weight field it is about. The daemon refuses this state at
    // its own write door too, so this is the earlier of two doors, not the only
    // one.
    if (zeroWeightEnabled(row, edit)) {
      setRefusals({ [draftKey(row.id, "weight")]: "zeroWeight" });
      setSaveErr(null);
      return;
    }
    await apply(row, edit, false);
  };

  /** Record one keystroke. The refusal for THAT field is cleared — it was about
   *  a value that is now gone — while every other field keeps its message, so
   *  fixing one number does not hide the reason another was refused. */
  const editDraft = (row: CapabilityRow, key: string, text: string) => {
    const dk = draftKey(row.id, key);
    setDrafts((d) => ({ ...d, [dk]: text }));
    setRefusals((r) => {
      if (r[dk] === undefined) return r;
      const next = { ...r };
      delete next[dk];
      return next;
    });
  };

  /** Restore the registry defaults for one row.
   *
   *  It does NOT write the default VALUES in: it sends the row with no option
   *  keys at all, and the daemon's `set_or_remove_*` REMOVES each one from
   *  policy.toml, so the file says "unset" rather than stating the default as an
   *  opinion of the user's. A row the file does not configure has no key to
   *  remove: the values the inputs show already ARE the registry defaults, so
   *  there is nothing to write and no row is created.
   *
   *  R2, ADJUDICATED (increment-3 review, finding on these lines): the local drafts
   *  are cleared BEFORE the `configured !== "file"` early return, and that order is
   *  deliberate — the finding was filed against exactly this and the judgement is
   *  that it is NOT a defect. Clearing the row's drafts IS what Reset means: move
   *  the clears below the return and a value the user typed stays in the input
   *  after pressing Reset, so Reset looks like it did nothing. The early return
   *  skips only the PUT — a row the file does not configure has no key to remove —
   *  never the local half of the same gesture. Confirmed, not changed. */
  const resetOptions = async (row: CapabilityRow) => {
    setDrafts((d) => dropRow(d, row.id));
    setRefusals((r) => dropRow(r, row.id));
    if (row.configured !== "file") return;
    await apply(row, { enabled: preservedEnabled(row), options: RESET_OPTIONS }, false);
  };

  /** The inputs of one row whose text differs from the value the daemon reports
   *  (the file's value, or the declared default where the file carries none) —
   *  what there is to apply. */
  const dirtyKeys = (row: CapabilityRow): string[] =>
    (row.options_schema ?? []).filter((entry) => {
      const text = drafts[draftKey(row.id, entry.key)];
      return text !== undefined && text !== String(optionValue(row, entry) ?? "");
    }).map((entry) => entry.key);

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
        /** The knobs this row really has: one input per key the REGISTRY
         *  DECLARES (`options_schema`), not per key that happens to resolve to a
         *  value — a declared key with no registry default (`min_score` on the
         *  semantic recall leg) is reachable only through the schema. A
         *  capability that declares nothing gets no editor at all, rather than an
         *  empty one claiming a knob that does not exist. */
        const keys = row.options_schema ?? [];
        const dirty = dirtyKeys(row);
        const locked = pending !== null;
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
            {/* The option editor, where the values used to be DISPLAYED. One
                numeric input per key the REGISTRY DECLARES (`options_schema`),
                labelled with the key's own name (`weight`, `min_score`, … — the
                exact names the user would otherwise be typing into policy.toml)
                and with the capability's declared description and gates in the
                tooltip. A key the FILE carries shows its own value; one it does
                not shows the registry default (tagged) or an empty field where the
                registry declares none — an empty field means UNSET, never "write
                the default in".

                Apply validates first and writes ONCE for the whole row; Reset
                removes the row's keys instead of writing defaults in. Both go
                through `capabilitiesBody`, which re-emits every other configured
                row with exactly the keys ITS file entry carries — the daemon's PUT
                replaces the whole table and its editor removes what an entry
                omits. */}
            {keys.length ? (
              <div className="capability-options" data-options-for={row.id}>
                <span className="muted micro">{t("settings.capabilities.options.title")}</span>
                {keys.map((entry) => {
                  const dk = draftKey(row.id, entry.key);
                  const refusal = refusals[dk];
                  const fromFile = fileCarries(row, entry);
                  return (
                    <label key={entry.key} className="capability-option" data-option={entry.key}>
                      <span className="mono micro">{entry.key}</span>
                      <Tooltip
                        title={
                          <>
                            <div className="mono">{entry.key}</div>
                            <div>{row.description}</div>
                            <div className="muted micro">{row.gates}</div>
                          </>
                        }
                      >
                        <Input
                          size="small"
                          className="capability-option-input"
                          aria-label={`${name} ${entry.key}`}
                          value={drafts[dk] ?? String(optionValue(row, entry) ?? "")}
                          disabled={locked}
                          status={refusal ? "error" : undefined}
                          onChange={(e) => editDraft(row, entry.key, e.target.value)}
                          onPressEnter={() => void applyOptions(row)}
                        />
                      </Tooltip>
                      <span className="muted micro">{entry.expectation}</span>
                      {/* Where the shown value comes from: the file (nothing) or
                          the registry's declaration. Without this the reader
                          cannot tell "1 because policy.toml says 1" from "1 by
                          default", which is the distinction a write must respect. */}
                      {fromFile ? null : (
                        <span className="tag" data-default-of={entry.key}>
                          {t("settings.capabilities.options.defaultTag")}
                        </span>
                      )}
                      {refusal ? (
                        <span className="tag err" data-refusal={refusal}>
                          {refusalText(t, entry, refusal)}
                        </span>
                      ) : null}
                    </label>
                  );
                })}
                <div className="row tight" style={{ padding: 0 }}>
                  <span data-apply-options={row.id}>
                    <Button
                      size="small"
                      type="primary"
                      loading={pending === row.id}
                      disabled={locked || !dirty.length}
                      onClick={() => void applyOptions(row)}
                    >
                      {t("settings.capabilities.options.apply")}
                    </Button>
                  </span>
                  <Tooltip title={t("settings.capabilities.options.resetHint")}>
                    <span data-reset-options={row.id}>
                      <Button size="small" disabled={locked} onClick={() => void resetOptions(row)}>
                        {t("settings.capabilities.options.reset")}
                      </Button>
                    </span>
                  </Tooltip>
                  {dirty.length ? (
                    <span className="tag">{t("settings.capabilities.options.unapplied")}</span>
                  ) : null}
                </div>
              </div>
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

      {/* A refusal is local, so the daemon never saw it: say so, next to the
          field that carries the reason. */}
      {Object.keys(refusals).length > 0 && !saveErr ? (
        <p className="muted micro" style={{ margin: 0 }}>
          {t("settings.capabilities.options.refused")}
        </p>
      ) : null}

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
                  // The row's flag flips on; its option keys are re-emitted exactly
                  // as the file carries them.
                  void apply(row, { enabled: true, options: fileOptions(row) }, true);
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
