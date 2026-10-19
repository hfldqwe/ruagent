// Settings: platform behavior the user owns. First resident: the
// distillation policy (auto / graph / agent / language / prompt) —
// pulled out of the Memory page where it never belonged (a settings
// concern is not a memory-browsing concern). Second resident (t7): the
// capability plane, so a pipeline is switched here instead of by
// hand-editing the [capabilities] table of policy.toml.

import { Alert, Button, Input, Select, Switch, Table, Tooltip } from "antd";
import { useCallback, useEffect, useState } from "react";
import {
  api,
  type CapabilitiesResponse,
  type CapabilityRow,
  type DistillLedgerEntry,
  type DistillLogPage,
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
import {
  ErrorState,
  Modal,
  ReadoutStrip,
  RelTime,
  Spinner,
  StaleNotice,
  fmtTokens,
  fmtUsd,
  readFailed,
  readLoading,
  readReady,
  type LoadState,
} from "../ui";
import { useI18n } from "../i18n";

export function Settings() {
  const { t } = useI18n();
  // t151 (audit row 20, `#settings`): this route has TWO independent loaders, each
  // with its own error card and its own retry. Measured on the committed bytes: a
  // 500 on the route's own endpoints paints two visible `role=alert` errors, and a
  // REAL click on the first retry clears only that card -- the other error stays on
  // screen, so "重试" does not restore the page (audit row 20: 恢复 ✗). The retry is
  // therefore page-scoped: bumping `epoch` re-runs EVERY loader on this route, so one
  // click re-reads all of it and both errors go away. Each card still paints its OWN
  // visible error while its data is missing (row 20 ①), and nothing is rendered as an
  // empty or successful state in the meantime (row 20 ③).
  const [epoch, setEpoch] = useState(0);
  const reload = useCallback(() => setEpoch((e) => e + 1), []);

  return (
    <div>
      <h1 className="sr-only">{t("settings.title")}</h1>
      <div className="view-bar">
        <h2>{t("settings.title")}</h2>
        <span className="muted">{t("settings.subtitle")}</span>
      </div>
      <DistillSettings epoch={epoch} onReload={reload} />
      <DistillLedger epoch={epoch} onReload={reload} />
      <CapabilitiesSettings epoch={epoch} onReload={reload} />
    </div>
  );
}

// Distillation policy (the [distill] table of policy.toml): auto, the graph
// flag, the extraction agent, the output language, a full prompt override.
// Live — saving swaps the running daemon's policy, no restart.
function DistillSettings({ epoch, onReload }: { epoch: number; onReload: () => void }) {
  const { t } = useI18n();
  const [policy, setPolicy] = useState<DistillPolicy | null>(null);
  const [err, setErr] = useState<unknown>(null);
  /** The role list, as a three-state (#53): the Select below has a
   *  `notFoundContent` that claims "没有可选角色", and the old
   *  `.catch(() => setAgents([]))` made a FAILED read say exactly that. */
  const [agents, setAgents] = useState<LoadState<{ name: string }[]>>(readLoading());

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
      .then((a) =>
        setAgents(readReady(a.filter((x) => x.enabled).map((x) => ({ name: x.name })))),
      )
      .catch((e) => setAgents((prev) => readFailed(prev, e)));
  };
  useEffect(() => {
    load();
    // t151: `epoch` is the page-scoped retry (see Settings()). The first run is the
    // mount load; every bump re-reads this card.
  }, [epoch]);

  if (!policy) {
    if (err) {
      return (
        <>
          <h1 className="sr-only">{t("settings.title")}</h1>
          <ErrorState
            title={t("distill.err")}
            hint={t("distill.err.hint")}
            onRetry={onReload}
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

      <div className="row between">
        <div className="minw0">
          <strong>{t("distill.auto")}</strong>
          <p className="muted flush">
            {t("distill.autoHint")}
          </p>
        </div>
        <Switch
          aria-label={t("distill.auto")}
          checked={policy.auto}
          onChange={(v) => set({ auto: v })}
        />
      </div>

      <div className="row between">
        <div className="minw0">
          <strong>{t("distill.graph")}</strong>
          <p className="muted flush">
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
        <label className="chat-field half">
          <span>{t("distill.agent")}</span>
          <Select
            aria-label={t("distill.agent")}
            value={policy.agent ?? ""}
            onChange={(v) => set({ agent: v || null })}
            className="w-full"
            options={[
              { value: "", label: t("distill.agentDefault") },
              ...(agents.data ?? []).map((a) => ({ value: a.name, label: a.name })),
            ]}
            // §5 empty: no enabled role must say so, not open an empty list.
            // The string belongs to THIS view's domain: a settings-only
            // message keyed under distill.* is copy that outlives its owner.
            // ... and a FAILED read must not borrow that sentence: the
            // dropdown then says what actually happened.
            notFoundContent={
              agents.kind === "error" ? t("settings.agentsFailed") : t("settings.noRoles")
            }
          />
        </label>
        <label className="chat-field half">
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
          <span className="tag ml8">
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

      {agents.kind === "error" && agents.data === null ? (
        // Nothing was ever read: the agent picker is not empty, it is UNKNOWN.
        <ErrorState
          title={t("settings.agentsErr")}
          hint={`${t("settings.agentsErr.hint")} — ${String(agents.err).slice(0, 160)}`}
          onRetry={load}
          retryLabel={t("common.retry")}
        />
      ) : null}
      {agents.kind === "error" && agents.data !== null ? (
        // A later failure keeps the list it already read, and says so.
        <StaleNotice err={agents.err} onRetry={load} />
      ) : null}

      {saveErr ? <ErrorState title={t("distill.saveFailed")} hint={saveErr} /> : null}
    </div>
  );
}

// ---------------------------------------------------------------------------
// The distillation LEDGER (t8 / issues #46 + #55)
//
// The policy card above says WHAT distillation is set to do; this card says what
// it actually DID — one row per attempt, failures included, which is the only
// place a failure REASON exists at all (#46's user-visible half).
//
// THREE STATES, NOT TWO. Every measurement 0028 added is nullable on purpose:
// `null` = nobody recorded it (rows written before the migration), `0` = a
// measured zero, a number = the reading. The card prints 未记录 for the first and
// never folds it into 0 — "this cost nothing" and "nobody wrote it down" are
// different readings, and a view that conflates them is the failure the NULL
// discipline exists to prevent.
//
// AND ONE MEASUREMENT THAT IS A GROUP (#58, t5). `from_seq` / `to_seq` /
// `total_messages` answer "which messages did this attempt read"; the writer
// produces them from ONE Option (0030), so they exist together or not at all.
// The card therefore judges the three as a unit (`coverageOf` below): a row
// that carries only part of the triple is NOT a range, and the missing part is
// never filled with a 0 to make it look like one. The daemon counts those rows
// for us in `totals.coverage_not_recorded`.
// ---------------------------------------------------------------------------

/** How many attempts the card lists. ONE page of the daemon's own envelope (its
 *  default is 50, its ceiling 500); `total` is shown beside the rows so a reader
 *  can see that more exists rather than assuming this is all of it. */
const LEDGER_ROWS = 20;

/** The ledger's four buckets, in the daemon's own vocabulary. `not_recorded` is
 *  a NULL status (the pre-0024 rows), NOT a fourth outcome — the buckets are
 *  shown side by side precisely so that distinction stays visible. */
const LEDGER_BUCKETS = ["ok", "empty", "failed", "not_recorded"] as const;

/** ONE ATTEMPT'S COVERAGE, judged as a GROUP (#58, t5; the read side is t4's).
 *
 *  The three columns come from a single `Option<TranscriptCoverage>` on the
 *  write side, so they are recorded together or not at all. Two consequences
 *  the card has to honour, and this helper is where they live so that no cell
 *  can forget them:
 *
 *   * `recorded` requires ALL THREE. A row carrying part of the triple is NOT a
 *     range: rendering "覆盖 0..0" for it would invent a reading out of a null,
 *     which is exactly what the NULL discipline forbids.
 *   * the pieces that DO exist are still reported (`parts`) instead of being
 *     masked into a blank 未记录 — the reader can see that the write was
 *     incomplete rather than the panel quietly tidying it up. */
type Coverage =
  | { recorded: true; from: number; to: number; total: number }
  | { recorded: false; parts: string[] };

function coverageOf(e: DistillLedgerEntry): Coverage {
  // The GROUP test, spelled out as one condition: all three, or not recorded.
  // A union rather than three optional fields, so the `recorded: false` branch
  // has NO numbers to print even if a caller wanted one.
  if (e.from_seq != null && e.to_seq != null && e.total_messages != null) {
    return { recorded: true, from: e.from_seq, to: e.to_seq, total: e.total_messages };
  }
  const parts: string[] = [];
  if (e.from_seq != null) parts.push("from");
  if (e.to_seq != null) parts.push("to");
  if (e.total_messages != null) parts.push("total");
  return { recorded: false, parts };
}

function DistillLedger({ epoch, onReload }: { epoch: number; onReload: () => void }) {
  const { t } = useI18n();
  // The same two-state shape DistillSettings above uses: `data` is what a
  // successful read produced, `err` is the last failure. A failed refresh keeps
  // the rows on screen and says they may be stale (audit row 20) instead of
  // blanking a card that was showing real numbers a second ago.
  const [page, setPage] = useState<DistillLogPage | null>(null);
  const [lastFailure, setLastFailure] = useState<DistillLedgerEntry | null>(null);
  const [err, setErr] = useState<unknown>(null);

  const load = useCallback(() => {
    Promise.all([
      api.distillLog({ limit: LEDGER_ROWS }),
      // The MOST RECENT failure, asked of the whole ledger rather than found
      // inside the page above: "why did the last run fail" must not be answered
      // by "the newest 20 attempts happened to succeed".
      api.distillLog({ status: "failed", limit: 1 }),
    ])
      .then(([p, failures]) => {
        setPage(p);
        setLastFailure(failures.entries[0] ?? null);
        setErr(null);
      })
      .catch((e) => setErr(e));
  }, []);

  useEffect(() => {
    load();
    // t151: `epoch` is the page-scoped retry, so one click on any error card on
    // #settings re-reads every loader on the route.
  }, [load, epoch]);

  if (!page) {
    return err ? (
      <ErrorState
        title={t("distill.ledger.err")}
        hint={`${t("distill.ledger.err.hint")} — ${String(err).slice(0, 200)}`}
        onRetry={onReload}
        retryLabel={t("common.retry")}
      />
    ) : (
      <Spinner label={t("distill.ledger")} />
    );
  }

  const totals = page.totals;
  // A daemon that PREDATES t4 (the resident one on 8787 today is one) sends no
  // `coverage_not_recorded` at all. It is not 0 and it is not a number, and the
  // card says so instead of letting `t()` leave the literal `{n}` standing in
  // the sentence -- a placeholder leak reads as a typo, "0" would read as a
  // measurement, and neither is what happened. The ROW cells need no such
  // branch: a missing column and a null column are the same non-reading, and
  // `coverageOf` tests `!= null`, which catches both.
  const coverageNotRecorded =
    typeof totals.coverage_not_recorded === "number" ? totals.coverage_not_recorded : null;
  /** A recorded number, or the ledger's own word for "nobody wrote it down". ONE
   *  helper, so no cell of this card can accidentally print a null as 0. */
  const measured = (v: number | null, render: (n: number) => string) =>
    v == null ? t("distill.notRecorded") : render(v);

  return (
    <div className="card distill-ledger">
      <div className="zone-head">
        <div className="zone-title">{t("distill.ledger")}</div>
        <span className="zone-note">{t("distill.ledger.rows", { n: page.total })}</span>
        <span className="grow" />
        <span className="muted micro">{t("distill.ledger.windowsNote")}</span>
        <Button size="small" onClick={load}>
          {t("distill.ledger.refresh")}
        </Button>
      </div>

      {err ? (
        // A refresh failed while rows are on screen: say so, and keep them.
        <ErrorState
          title={t("distill.ledger.err")}
          hint={`${t("distill.ledger.err.hint")} — ${String(err).slice(0, 200)}`}
          onRetry={onReload}
          retryLabel={t("common.retry")}
        />
      ) : null}

      {page.total === 0 ? (
        <p className="muted pad">{t("distill.ledger.empty")}</p>
      ) : (
        <>
          {/* #46: the REASON of the most recent failure, not just a count of
              failures. `data-last-failure` carries that row's id (or `none`) and
              `data-failure-reason` the text, so a test reads the reason itself
              rather than the label beside it. */}
          <div
            className="row wrap"
            data-last-failure={lastFailure ? String(lastFailure.id) : "none"}
          >
            <span className="muted micro">{t("distill.ledger.lastFailure")}</span>
            {lastFailure ? (
              <>
                <span className="tag err">{t("distill.ledger.status.failed")}</span>
                <span className="mono micro">{lastFailure.session_key}</span>
                <span className="muted micro" title={lastFailure.distilled_at}>
                  <RelTime iso={lastFailure.distilled_at} />
                </span>
                <span data-failure-reason="">
                  {lastFailure.failure_reason ?? t("distill.ledger.noReason")}
                </span>
              </>
            ) : (
              <span className="muted micro">{t("distill.ledger.noneFailed")}</span>
            )}
          </div>

          <ReadoutStrip
            grid
            items={[
              { key: "rows", label: t("distill.ledger.totalLabel"), value: page.total },
              {
                key: "tokens",
                label: t("distill.ledger.tokens"),
                value: measured(totals.tokens_used, fmtTokens),
              },
              {
                key: "cost",
                label: t("distill.ledger.cost"),
                value: measured(totals.cost_usd, fmtUsd),
              },
            ]}
          />
          <p className="muted micro">
            {t("distill.ledger.totalsNote", {
              t: totals.tokens_used_not_recorded,
              c: totals.cost_usd_not_recorded,
            })}
          </p>
          {/* #58: the same "counted, never summed" discipline for the coverage
              reading. The number is how many attempts could not answer what
              they read; a 0 in it is a MEASURED zero (every row answered), not
              a missing measurement folded flat. */}
          <p
            className="muted micro"
            data-coverage-not-recorded={coverageNotRecorded ?? "not_recorded"}
          >
            {coverageNotRecorded == null
              ? t("distill.ledger.coverageNoteUnknown")
              : t("distill.ledger.coverageNote", { n: coverageNotRecorded })}
          </p>

          {/* The four buckets, each a MEASURED count — the daemon counts them
              over the same filter as `total`, so a 0 here is a real zero and
              the four add up to the number above. */}
          <div className="row wrap">
            {LEDGER_BUCKETS.map((b) => (
              <span key={b} className={b === "failed" ? "tag err" : "tag"} data-bucket={b}>
                {t(`distill.ledger.status.${b}`)} {page.by_status[b] ?? 0}
              </span>
            ))}
          </div>

          <Table
            className="facts ledger"
            size="small"
            dataSource={page.entries}
            rowKey="id"
            pagination={false}
            rowClassName={(e) => (e.status == null ? "row-old" : "")}
            columns={[
              {
                title: t("distill.ledger.col.when"),
                dataIndex: "distilled_at",
                render: (v: string) => (
                  <span className="muted micro" title={v}>
                    <RelTime iso={v} />
                  </span>
                ),
              },
              {
                title: t("distill.ledger.col.session"),
                dataIndex: "session_key",
                render: (v: string) => (
                  <span className="mono micro" title={v}>
                    {v}
                  </span>
                ),
              },
              {
                title: t("distill.ledger.col.status"),
                dataIndex: "status",
                render: (v: string | null, e: DistillLedgerEntry) => (
                  <span className="row tight">
                    <span
                      className={v === "failed" ? "tag err" : v === "ok" ? "tag ok" : "tag"}
                      data-status={v ?? "not_recorded"}
                    >
                      {t(`distill.ledger.status.${v ?? "not_recorded"}`)}
                    </span>
                    {/* #62: "ok" AND "0 written" is the shape a governed refusal
                        leaves in the ledger (the pass ran and was recorded; the
                        write vocabulary refused every candidate). Marked here so an
                        ok/0 row cannot be read as an ok/3 row — and never as the
                        "empty" outcome, which is the other reading entirely. The
                        cause is not guessed: it is in the Reason column, where the
                        daemon writes the refused pairs. */}
                    {v === "ok" && e.memories_written === 0 ? (
                      <span
                        className="tag warn"
                        data-wrote-nothing="1"
                        title={t("distill.ledger.wroteNothingHint")}
                      >
                        {t("distill.ledger.wroteNothing")}
                      </span>
                    ) : null}
                  </span>
                ),
              },
              {
                title: t("distill.ledger.col.reason"),
                dataIndex: "failure_reason",
                render: (v: string | null, e: DistillLedgerEntry) =>
                  v != null ? (
                    <span>{v}</span>
                  ) : e.status === "failed" ? (
                    <span className="muted micro">{t("distill.ledger.noReason")}</span>
                  ) : (
                    <span className="muted micro">—</span>
                  ),
              },
              {
                title: t("distill.ledger.col.usage"),
                key: "usage",
                render: (_: unknown, e: DistillLedgerEntry) => (
                  <span className="mono micro">
                    {measured(e.tokens_used, fmtTokens)}
                    {" · "}
                    {measured(e.cost_usd, fmtUsd)}
                    {" · "}
                    {measured(e.windows, (n) => t("distill.ledger.calls", { n }))}
                    {e.truncated === true ? ` · ${t("distill.ledger.truncated")}` : ""}
                  </span>
                ),
              },
              {
                // Beside the model-call count on purpose: "1 次调用" and "覆盖
                // 1..40 / 共 120 条" are the two halves of one question — how
                // much of the session did that call actually see — and a reader
                // who has one without the other cannot tell a truncated pass
                // from a small session.
                title: t("distill.ledger.col.coverage"),
                key: "coverage",
                render: (_: unknown, e: DistillLedgerEntry) => {
                  const cov = coverageOf(e);
                  const hint = t("distill.ledger.coverage.hint");
                  // `data-coverage` carries the GROUP VERDICT, so a test asserts
                  // the judgment itself rather than re-deriving it from rendered
                  // text; the raw triple rides along for the partial case, where
                  // the verdict alone would hide which half was missing.
                  return cov.recorded ? (
                    <span
                      className="mono micro"
                      data-coverage="recorded"
                      data-coverage-from={cov.from}
                      data-coverage-to={cov.to}
                      data-coverage-total={cov.total}
                      title={hint}
                    >
                      {t("distill.ledger.coverage.range", { from: cov.from, to: cov.to })}
                      {/* " / " inside one reading, " · " between readings: the
                          usage column next door already separates its three
                          measurements with " · ", so the range and its
                          denominator must not blur into that list. */}
                      {" / "}
                      {t("distill.ledger.coverage.total", { n: cov.total })}
                    </span>
                  ) : (
                    <span
                      className="muted micro"
                      data-coverage="not_recorded"
                      data-coverage-parts={cov.parts.length ? cov.parts.join(",") : "none"}
                      title={hint}
                    >
                      {cov.parts.length === 0
                        ? t("distill.notRecorded")
                        : t("distill.ledger.coverage.partial", {
                            from: measured(e.from_seq, String),
                            to: measured(e.to_seq, String),
                            total: measured(e.total_messages, String),
                          })}
                    </span>
                  );
                },
              },
              {
                title: t("distill.ledger.col.relations"),
                key: "relations",
                render: (_: unknown, e: DistillLedgerEntry) => (
                  <span className="mono micro">
                    {t("distill.ledger.relations", {
                      w: e.relations_written,
                      d: measured(e.relations_duplicate, String),
                      r: measured(e.relations_refused, String),
                    })}
                  </span>
                ),
              },
              {
                title: t("distill.ledger.col.retries"),
                dataIndex: "retries",
                render: (v: number | null) => <span className="mono micro">{measured(v, String)}</span>,
              },
            ]}
          />
          {page.truncated ? (
            <p className="muted micro pad">
              {t("distill.ledger.rows", { n: page.total })}
            </p>
          ) : null}
        </>
      )}
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
function CapabilitiesSettings({ epoch, onReload }: { epoch: number; onReload: () => void }) {
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
    // t151: the same page-scoped retry as DistillSettings (see Settings()).
  }, [epoch]);

  if (!data) {
    if (err) {
      return (
        <ErrorState
          title={t("settings.capabilities.err")}
          hint={`${t("settings.capabilities.errHint")} (${
            err instanceof Error ? err.message : String(err)
          })`}
          onRetry={onReload}
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

      <p className="muted micro flush">
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
            <ul className="conflict-list">
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
            <div className="row between">
              <div className="minw0">
                <div className="row tight pad0">
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
                <p className="muted flush">
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
              <p className="cost-note">
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
                <div className="row tight pad0">
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
        <p className="muted micro flush">
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
          <p className="muted micro mb0">
            {armed.description}
          </p>
        </Modal>
      ) : null}
    </div>
  );
}
