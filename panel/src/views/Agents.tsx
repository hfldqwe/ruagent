// Agents (roles with live stats + MCP registry — runtimes have their own
// view, design §4.1 two-layer ruling 2026-09-17), Stats, Inbox.
//
// Three routes live here; all three share one contract from view-*.md §5:
// a failed read must never be rendered as an empty one.
//   #agents  — a missing statistic (—) and "ran 0 times" are different things,
//              and a failed stats read says so out loud (A9 / 行 20).
//   #stats   — the ledger is never zeroed by a poll failure (S12 / 行 20),
//              and the recall log is read-only data, not 20 dead buttons (S9).
//   #inbox   — the queue that blocks runs is the most dangerous thing to
//              mis-render; a failed read is an alert, not "nothing pending"
//              (I4 / 行 20).

import { useEffect, useMemo, useState, type ReactNode } from "react";
import { Button, Card, Input, Popconfirm, Progress, Select, Table, Tooltip } from "antd";
import {
  api,
  type AgentInfo,
  type AgentStats,
  type McpRegistry,
  type PendingPermission,
  type PrimaryInfo,
  type SessionOptionInfo,
  isRoleAgent,
} from "../api";
import {
  Empty,
  ErrorState,
  Modal,
  ReadoutStrip,
  RelTime,
  Spinner,
  StaleNotice,
  Zone,
  fmtUsd,
  readFailed,
  readLoading,
  readReady,
  useToast,
  type LoadState,
} from "../ui";
import { useI18n } from "../i18n";
import { Icon } from "../icons";

/** One poll for the whole page (§5 密集 / A10, 行 28): a hidden tab stops
 *  asking. The shell's permission poll is a separate, App-level concern. */
function usePoll(fn: () => void, ms: number, deps: unknown[] = []) {
  useEffect(() => {
    fn();
    const i = setInterval(() => {
      if (!document.hidden) fn();
    }, ms);
    const onVisible = () => {
      if (!document.hidden) fn();
    };
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      clearInterval(i);
      document.removeEventListener("visibilitychange", onVisible);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
}

// ---------------------------------------------------------------------------
// Agents
// ---------------------------------------------------------------------------

export function Agents() {
  const { t } = useI18n();
  const [agents, setAgents] = useState<AgentInfo[] | null>(null);
  const [stats, setStats] = useState<AgentStats[]>([]);
  const [mcp, setMcp] = useState<McpRegistry | null>(null);
  const [err, setErr] = useState<unknown>(null);
  const [statsErr, setStatsErr] = useState<unknown>(null);
  const [mcpErr, setMcpErr] = useState<unknown>(null);
  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState<AgentInfo | null>(null);
  /** The agent whose prompt is open in the dialog (null = closed). */
  const [promptOf, setPromptOf] = useState<AgentInfo | null>(null);
  /** WHO ANSWERS A NEW CHAT (docs/design/primary-agent.md, GET /api/v1/primary).
   *  A `LoadState`, not `PrimaryInfo | null`: this file's own rule is that a
   *  failed read is never rendered as an empty one (行 20), and here the empty
   *  reading would be "there is no primary" — a fact this view would be making
   *  up. `data` survives a later failure, so a blip does not blank the badges. */
  const [frontDoor, setFrontDoor] = useState<LoadState<PrimaryInfo>>(readLoading<PrimaryInfo>());
  /** The last primary WRITE that failed, kept on screen with the op it tried:
   *  a toast disappears, and a refusal that disappears leaves the row looking
   *  like the user's click simply did nothing. */
  const [primaryErr, setPrimaryErr] = useState<{ err: unknown; name: string | null } | null>(null);
  const [primaryBusy, setPrimaryBusy] = useState(false);
  const toast = useToast();

  // One request window for agents + stats + mcp + primary instead of staggered
  // timers (A10). Each leg keeps the last good value on failure. The front door
  // joins THIS window rather than getting its own timer: one cadence per page
  // (行 28), and it is the same fact — a registry edit can move the answer, so a
  // separate schedule would let the rows and the answer disagree.
  const load = () => {
    api
      .agents()
      .then((v) => {
        setAgents(v);
        setErr(null);
      })
      .catch((e) => setErr(e));
    api
      .stats()
      .then((v) => {
        setStats(v);
        setStatsErr(null);
      })
      .catch((e) => setStatsErr(e));
    api
      .mcp()
      .then((v) => {
        setMcp(v);
        setMcpErr(null);
      })
      .catch((e) => setMcpErr(e));
    api
      .primary()
      .then((fd) => setFrontDoor(readReady(fd)))
      // A failure keeps the previous answer (and flags itself); it is NEVER
      // turned into "there is no primary" — see `frontDoor`'s declaration.
      .catch((e) => setFrontDoor((prev) => readFailed(prev, e)));
  };
  // 行 39: the window's budget is `2K + 2` per endpoint, so it is 8 for the
  // three it had and 10 for the four it has now. A 5s timer spends 3 per
  // endpoint (mount + 2 ticks) = 9 — measured 262 / 5276 / 10275, evenly spaced:
  // cadence, not duplicate fetches. 10s leaves mount + one tick = 6 ≤ 10, and
  // that per-endpoint count did not grow when the front door joined the window.
  // Registry edits still refresh immediately (create/edit call load() themselves).
  usePoll(load, 10000);

  const roles = useMemo(
    () =>
      (agents ?? [])
        .filter(isRoleAgent)
        // Stable order across polls (§5 密集): the API order is not a contract.
        .sort((a, b) => a.name.localeCompare(b.name)),
    [agents],
  );
  const runtimeList = (agents ?? []).filter((a) => !isRoleAgent(a));

  const removeRole = async (name: string) => {
    try {
      await api.deleteAgent(name);
      toast("ok", t("agents.roleDeleted", { name }));
      load();
    } catch (e) {
      toast("err", String(e));
    }
  };

  const setEnabled = async (name: string, enabled: boolean) => {
    try {
      await api.updateAgent(name, { enabled });
      toast("ok", enabled ? t("agents.enabled") : t("agents.disabled"));
      load();
    } catch (e) {
      toast("err", String(e));
    }
  };

  /** The front door as far as this view KNOWS it. `data` outlives a later
   *  failure on purpose (that is what LoadState is for): a 10s blip must not
   *  blank the badges and thereby claim the platform lost its primary. */
  const fdv = frontDoor.data;
  const fvName = fdv?.name ?? null;
  const fvWhy = (fdv?.why ?? "").trim();
  const fvSource = fdv?.source ?? null;
  /** The read failed and nothing was ever read: the band says so, and NO row is
   *  allowed to claim it is (or is not) the primary. */
  const fvUnknown = frontDoor.kind === "error" && !fdv;
  /** Read failed, but an earlier answer is on screen: show it and say it may be
   *  out of date (StaleNotice), rather than treating the blip as an answer. */
  const fvStale = frontDoor.kind === "error" && !!fdv;
  /** The daemon's answer names something that is NOT a role card (a runtime card,
   *  or an agent gone from the registry). Every badge below lives on a role card,
   *  so the band has to say why none of them carries one — naming a front door
   *  with no visible row reads as a broken page. (A DISABLED role is still a role
   *  card here, filtered by nothing, and is badged like any other.) */
  const fvNotARole = !!fvName && !roles.some((a) => a.name === fvName);

  /** SET or CLEAR the front door — the ONE operation clients should call.
   *  `PUT /api/v1/primary` writes both places (the `primary` flag in agents.toml
   *  and routing.toml's `default`) in one step; this view must not try to do it
   *  in two requests, because the failure mode of a half-write ("I set the
   *  primary and nothing changed") is the one thing the endpoint exists to
   *  prevent. The reply IS the resulting answer, so the state is updated from the
   *  write instead of refetching the page. */
  const applyPrimary = async (name: string | null) => {
    setPrimaryErr(null);
    setPrimaryBusy(true);
    try {
      const fd = await api.setPrimary(name);
      setFrontDoor(readReady(fd));
      // The rows' `primary` flags follow the same single write (exactly one flag,
      // or none), so they are updated locally instead of by a refetch. The 10s
      // poll re-reads the registry anyway — that is what would correct this line
      // if a daemon ever answered with something other than what it just wrote.
      setAgents((rows) =>
        rows ? rows.map((a) => ({ ...a, primary: a.name === (name ?? "") })) : rows,
      );
      toast("ok", name ? t("agents.primary.done", { name }) : t("agents.primary.cleared"));
    } catch (e) {
      // Rows are NOT touched here. A refused write is not a cancel: flipping a
      // row to "no longer primary" on a 4xx would be inventing a success.
      setPrimaryErr({ err: e, name });
      toast("err", String(e));
    } finally {
      setPrimaryBusy(false);
    }
  };

  /** The row badge — WHICH agent answers a new chat, and why. Three different
   *  facts, deliberately not collapsed into one label:
   *   · it answers, because it carries the `primary` flag (the user's own pick);
   *   · it answers, because routing.toml's `default` outranks the flag (someone
   *     who marked a DIFFERENT agent must be able to see this, not just the
   *     winner's name);
   *   · it carries the flag and does NOT answer — or we cannot say, because the
   *     front-door read failed (then only what the row itself claims is shown). */
  const primaryBadge = (a: AgentInfo): { text: string; cls: string; title?: string } | null => {
    if (fvName && fvName === a.name) {
      const routing = fvSource === "routing-default";
      return {
        text: routing
          ? t("agents.primary.effectiveRouting")
          : fvSource === "primary-flag"
            ? t("agents.primary.effectiveFlag")
            : t("agents.primary.effective"),
        cls: routing ? "tag" : "tag ok",
        title: fvWhy || undefined,
      };
    }
    if (a.primary) {
      if (fvName) {
        return {
          text: t("agents.primary.overridden"),
          cls: "tag warn",
          title: t("agents.primary.overriddenHint", { name: fvName }),
        };
      }
      // The answer is unknown (the read failed), so the row says only the one
      // thing it knows: this card is flagged in agents.toml.
      return {
        text: t("agents.primary.flagged"),
        cls: "tag",
        title: t("agents.primary.flaggedHint"),
      };
    }
    return null;
  };

  if (agents === null) {
    if (err) {
      return (
        <>
          <h1 className="sr-only micro">{t("agents.title")}</h1>
          <ErrorState
            title={t("agents.err")}
            hint={t("agents.err.hint")}
            onRetry={load}
            retryLabel={t("common.retry")}
          />
        </>
      );
    }
    return <Spinner label={`${t("agents.title")}…`} />;
  }

  const mcpServers = mcp?.servers ?? [];

  return (
    <div>
      <h1 className="sr-only micro">{t("agents.title")}</h1>
      <div className="view-bar">
        <h2>{t("agents.title")}</h2>
        <span className="muted">{t("agents.subtitle")}</span>
        <span className="grow" />
        <Button type="primary" onClick={() => setCreating(true)}>
          + {t("agents.create")}
        </Button>
      </div>

      {err && roles.length > 0 ? (
        <ErrorState
          title={t("common.stale")}
          hint={t("agents.stale.hint")}
          onRetry={load}
          retryLabel={t("common.retry")}
        />
      ) : null}
      {/* A9: a failed stats read is announced — it is not the same as
          "this role has never run", which the cards render as — . */}
      {statsErr && roles.length > 0 ? (
        <ErrorState
          title={t("agents.err")}
          hint={t("agents.err.hint")}
          onRetry={load}
          retryLabel={t("common.retry")}
        />
      ) : null}

      {/* WHO ANSWERS A NEW CHAT (docs/design/primary-agent.md). The band states
          the ONE answer and its SOURCE, because the two sources can disagree and
          the difference is invisible from the picker: routing.toml's `default`
          outranks the `primary` flag, so a user who marked one agent can be
          answered by another. The rows carry the same information as badges; this
          band is where the reason is readable in words. */}
      <Zone title={t("agents.primary.title")} note={t("agents.primary.subtitle")}>
        {fvUnknown ? (
          /* A FAILED READ IS NOT "no primary" (行 20). Say what happened, offer
             the retry, and leave every row without a badge: not knowing is the
             state, and faking either answer is the bug this avoids. */
          <ErrorState
            title={t("agents.primary.readErr")}
            hint={`${t("agents.primary.readErr.hint")}${
              String(frontDoor.err ?? "").trim()
                ? ` — ${String(frontDoor.err).trim().slice(0, 200)}`
                : ""
            }`}
            onRetry={load}
            retryLabel={t("common.retry")}
          />
        ) : (
          <>
            {fvStale ? (
              <StaleNotice
                err={frontDoor.err}
                hint={t("agents.primary.stale.hint")}
                onRetry={load}
              />
            ) : null}
            {/* The last failed WRITE, with the operation that failed, so a
                refusal is readable instead of a toast that already vanished. */}
            {primaryErr ? (
              <ErrorState
                title={
                  primaryErr.name
                    ? t("agents.primary.writeErr", { name: primaryErr.name })
                    : t("agents.primary.writeErrClear")
                }
                hint={`${String(primaryErr.err ?? "").trim().slice(0, 300) || t(
                  "agents.primary.writeErr.hint",
                )}`}
                onRetry={() => {
                  void applyPrimary(primaryErr.name);
                }}
                retryLabel={t("common.retry")}
              />
            ) : null}
            <div className="card">
              {frontDoor.kind === "loading" && !fdv ? (
                <p className="muted pad">{t("agents.primary.loading")}</p>
              ) : fvName ? (
                <div className="row">
                  <span className="doc-icon">
                    <Icon name="bot" size={15} />
                  </span>
                  <strong>{fvName}</strong>
                  <span
                    className={fvSource === "routing-default" ? "tag" : "tag ok"}
                    title={fvWhy || undefined}
                  >
                    {fvSource === "routing-default"
                      ? t("agents.primary.srcRouting")
                      : fvSource === "primary-flag"
                        ? t("agents.primary.srcFlag")
                        : t("agents.primary.srcUnknown")}
                  </span>
                  <span className="grow" />
                  {fvNotARole ? (
                    <span
                      className="tag warn"
                      title={t("agents.primary.notARole.hint", { name: fvName ?? "" })}
                    >
                      {t("agents.primary.notARole")}
                    </span>
                  ) : null}
                </div>
              ) : (
                /* A DEFINITE answer: the daemon has no front door. Say what to do
                   about it — the picker step on the chat page is the thing this
                   removes. */
                <p className="muted pad">{t("agents.primary.none")}</p>
              )}
              {fvWhy ? <p className="muted pad">{t("agents.primary.why", { why: fvWhy })}</p> : null}
            </div>
          </>
        )}
      </Zone>

      {roles.length === 0 ? (
        <Empty icon="bot" title={t("agents.empty.title")} hint={t("agents.empty.hint")} />
      ) : (
        <div className="agent-grid">
          {roles.map((a) => {
            const s = stats.find((x) => x.agent === a.name);
            const success = s && s.runs > 0 ? Math.round((s.completed / s.runs) * 100) : null;
            return (
              <Card
                key={a.name}
                className={a.enabled ? "agent-card" : "agent-card disabled"}
                size="small"
                aria-disabled={!a.enabled}
              >
                <div className="row">
                  <span className="agent-avatar">{a.name.slice(0, 2).toUpperCase()}</span>
                  <div>
                    <strong>{a.name}</strong>
                    <div className="muted">
                      {t("agents.runtime")} · {a.harness}
                    </div>
                  </div>
                </div>
                {/* t180 (attempt 2): the chips used to end the header row behind
                    a .grow spacer, so their left edge was the SUM of their own
                    text widths and the wrap point moved with the copy. Measured
                    pre-fix, card-relative: the runtime chip's x was 196.3 / 216
                    / 249 / 249.1 / 255.2 (58.9px spread) and the state chip fell
                    to line 2 on architect and impl (y 127.9 -> 168.9, 41px line
                    pitch) while the other five stayed on line 1 - so every block
                    below the band started 41px lower on those two cards. That is
                    the "labels differ per agent => the layout is not uniform" the
                    user reported.

                    The band is now its own row starting at the card's left edge
                    and reserving two lines (.agent-chips), which is what makes
                    the fields below it line up; the CHIP ORDER IS UNCHANGED
                    (runtime -> portable -> state) because e2e/registry.spec.ts:46
                    asserts a role card's first .tag is the runtime name, and the
                    order is information, not decoration. Every chip still renders
                    in full: a fixed-track grid was rejected because it would
                    ellipsize a long runtime name, which is the t71 regression. */}
                <div className="agent-chips">
                  {a.runtime ? <span className="tag">{a.runtime}</span> : null}
                  {(a.runtimes?.length ?? 0) > 1 ? (
                    <span className="tag">{t("agents.portable", { n: a.runtimes!.length })}</span>
                  ) : null}
                  {a.enabled ? (
                    <span className="tag ok">{t("agents.enabled")}</span>
                  ) : (
                    <span className="tag">{t("agents.disabled")}</span>
                  )}
                  {/* Appended LAST on purpose: the band's existing order is
                      information and e2e/registry.spec.ts:46 asserts a role
                      card's first .tag is the runtime name. */}
                  {(() => {
                    const b = primaryBadge(a);
                    return b ? (
                      <span className={b.cls} title={b.title}>
                        {b.text}
                      </span>
                    ) : null;
                  })()}
                </div>
                <p className="muted" style={{ margin: "10px 0 6px" }}>
                  {a.description}
                </p>
                {a.prompt ? (
                  <button
                    type="button"
                    className="prompt-open"
                    onClick={() => setPromptOf(a)}
                  >
                    {/* t243: the prompt opens in a DIALOG, not by unfolding in
                       the card: the cards share a grid row, so an in-card unfold
                       stretched the whole row (measured: one card expanded took
                       all three cards of the row from 339px to 600px, and pushed
                       the next row down). The dialog leaves the row's heights
                       alone and shows the whole prompt. */}
                    {t("agents.promptView")} · {a.prompt.length}{" "}
                    {t("chat.chars", { n: a.prompt.length }).split(" ")[1] ?? ""}
                  </button>
                ) : null}
                {a.model ? (
                  <span className="tag mono">{a.model}</span>
                ) : (
                  // t243: an agent with no model of its own used to render
                  // NOTHING here, which read as a missing field. Say it
                  // instead of leaving a hole. Deliberately NOT a runtime
                  // default: the runtime's current model is only knowable
                  // per agent (GET /agents/{name}/options probes the
                  // harness), so printing one would mean fabricating it or
                  // one request per card.
                  <span className="tag">{t("agents.modelUnset")}</span>
                )}
                {s && s.runs > 0 ? (
                  <div className="agent-stats">
                    <div className="stat-cell">
                      <span className="stat-num">{s.runs}</span>
                      <span className="muted">{t("agents.runs")}</span>
                    </div>
                    <div className="stat-cell">
                      <span className="stat-num">{success}%</span>
                      <span className="muted">{t("agents.success")}</span>
                    </div>
                    <div className="stat-cell">
                      <span className="stat-num">{fmtUsd(s.total_cost_usd)}</span>
                      <span className="muted">{t("agents.cost")}</span>
                    </div>
                    <div className="stat-cell">
                      <span className="stat-num">
                        <RelTime iso={s.last_run_at} />
                      </span>
                      <span className="muted">{t("agents.lastRun")}</span>
                    </div>
                  </div>
                ) : (
                  /* Same block, same shape — a dash over the label keeps
                     the divider at the same height as run stats (a bare
                     sentence made it 29px shorter and the card's hairline
                     visibly lower than its row-mates, user report
                     2026-09-20). */
                  <div className="agent-stats">
                    <div className="stat-cell">
                      <span className="stat-num">—</span>
                      <span className="muted">{t("agents.noRuns")}</span>
                    </div>
                  </div>
                )}
                <div className="row end">
                  {/* Set / clear the front door. The consequence is stated in the
                      popconfirm BEFORE the click does anything — both places it
                      writes, in one sentence — because this changes who answers
                      the user, and that is not a thing to discover afterwards.
                      A disabled role gets a disabled button: it cannot answer, so
                      "make it the primary" would be a promise the cascade breaks
                      (it skips disabled cards). */}
                  {!a.enabled ? (
                    <Button disabled title={t("agents.primary.setDisabled")}>
                      {t("agents.primary.set")}
                    </Button>
                  ) : fvName === a.name ? (
                    <Popconfirm
                      title={t("agents.primary.confirmClear.title")}
                      description={t("agents.primary.confirmClear.body")}
                      okText={t("agents.primary.clear")}
                      cancelText={t("common.keep")}
                      okButtonProps={{ danger: true }}
                      onConfirm={() => applyPrimary(null)}
                    >
                      <Button loading={primaryBusy}>{t("agents.primary.clear")}</Button>
                    </Popconfirm>
                  ) : (
                    <Popconfirm
                      title={t("agents.primary.confirmSet.title", { name: a.name })}
                      description={t("agents.primary.confirmSet.body", { name: a.name })}
                      okText={t("agents.primary.set")}
                      cancelText={t("common.keep")}
                      onConfirm={() => applyPrimary(a.name)}
                    >
                      <Button loading={primaryBusy}>{t("agents.primary.set")}</Button>
                    </Popconfirm>
                  )}
                  {/* A12: a disabled role used to be a dead end — .5 opacity
                      and no way back. The card can re-enable itself. */}
                  {!a.enabled ? (
                    <Button onClick={() => setEnabled(a.name, true)}>{t("agents.enable")}</Button>
                  ) : null}
                  <Button onClick={() => setEditing(a)}>{t("common.edit")}</Button>
                  <Popconfirm
                    title={t("agents.roleDeleteConfirm.title")}
                    okText={t("common.delete")}
                    cancelText={t("common.keep")}
                    okButtonProps={{ danger: true }}
                    onConfirm={() => removeRole(a.name)}
                  >
                    <Button danger>{t("common.delete")}</Button>
                  </Popconfirm>
                </div>
              </Card>
            );
          })}
        </div>
      )}

      <Zone
        title={t("mcp.registry")}
        note={mcpServers.length > 0 ? t("mcp.servers", { n: mcpServers.length }) : null}
      >
        {mcpErr && mcp === null ? (
          <ErrorState
            title={t("agents.err")}
            hint={t("agents.err.hint")}
            onRetry={load}
            retryLabel={t("common.retry")}
          />
        ) : mcpServers.length > 0 ? (
          <>
            <div className="card">
              {mcpServers.map((s) => (
                <div key={s.name} className="row">
                  <span className="doc-icon">
                    <Icon name="plug" size={15} />
                  </span>
                  <strong>{s.name}</strong>
                  {s.health ? (
                    s.health.state === "ok" ? (
                      // t128 (t123 §3 F1 family): `tools == null` is the THIRD
                      // state, and it is not a zero.
                      //
                      // `tools` is `number | null` (api.ts:261) and the key can
                      // also be ABSENT, which reads as `undefined` here: `?? 0`
                      // collapsed both into a confident "0 tools", i.e. "this
                      // server exposes nothing" when the truth was "the daemon
                      // did not tell us". Same rule as `runtimes.probeFailed`
                      // (Runtimes.tsx:213-218: a failed probe must not print a
                      // measured 0) and as `knowledgeSearchLegs`'s "a missing leg
                      // is null, never 0" (api.ts:1341-1346). Loose `==` is
                      // deliberate: it catches `null` AND a deleted key.
                      s.health.tools == null ? (
                        // The label promises a retry, so it has to BE one -- the
                        // shape Runtimes.tsx:305-310 already established for its
                        // third state (neutral `.tag`, not `.ok`, not `.warn`:
                        // `warn` is "down", which IS a measurement).
                        <Tooltip title={t("agents.mcp.toolsUnknown.hint")}>
                          <button type="button" className="tag" onClick={load}>
                            {t("mcp.up")} · {t("agents.mcp.toolsUnknown")}
                          </button>
                        </Tooltip>
                      ) : (
                        <Tooltip
                          title={`${s.health.tools} ${t("mcp.tools")} · ${s.health.latency_ms}ms`}
                        >
                          <span className="tag ok">
                            {t("mcp.up")} · {s.health.tools}
                          </span>
                        </Tooltip>
                      )
                    ) : (
                      // "down" is a state, not an error: .tag.warn, never .tag.err
                      <Tooltip title={s.health.error ?? t("mcp.down")}>
                        <span className="tag warn">{t("mcp.down")}</span>
                      </Tooltip>
                    )
                  ) : null}
                  {s.inject_for ? <span className="tag">{s.inject_for.join(", ")}</span> : null}
                  <span className="grow" />
                  <span className="muted mono truncated">{s.url ?? s.command}</span>
                </div>
              ))}
            </div>
            <p className="muted pad">{t("mcp.overlay")}</p>
          </>
        ) : (
          <p className="muted pad">{t("mcp.empty.title")}</p>
        )}
      </Zone>

      {/* profiles had no surface at all (§4): the registry's second segment —
          which servers a named profile injects. */}
      {mcp && mcp.profiles.length > 0 ? (
        <Zone title={t("mcp.profiles")} note={t("stats.rows", { n: mcp.profiles.length })}>
          <div className="card">
            {mcp.profiles.map((p) => (
              <div key={p.name} className="row">
                <span className="doc-icon">
                  <Icon name="layers" size={15} />
                </span>
                <strong>{p.name}</strong>
                <span className="grow" />
                {p.servers.map((n) => (
                  <span key={n} className="tag">
                    {n}
                  </span>
                ))}
              </div>
            ))}
          </div>
        </Zone>
      ) : null}

      {promptOf ? (
        <Modal
          title={`${t("agents.promptView")} · ${promptOf.name}`}
          onClose={() => setPromptOf(null)}
          wide
        >
          <div className="prompt-view">
            <pre>{promptOf.prompt}</pre>
          </div>
        </Modal>
      ) : null}

      {(creating || editing) && (
        <RoleModal
          runtimes={runtimeList}
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

// ---------------------------------------------------------------------------
// Role create/edit modal (two-layer form: prompt + runtime refs)
// ---------------------------------------------------------------------------

function RoleModal({
  runtimes,
  editing,
  onClose,
  onSaved,
}: {
  runtimes: AgentInfo[];
  editing: AgentInfo | null;
  onClose: () => void;
  onSaved: () => void;
}) {
  const { t } = useI18n();
  const toast = useToast();
  const [form, setForm] = useState(() =>
    editing
      ? {
          name: editing.name,
          prompt: editing.prompt ?? "",
          description: editing.description ?? "",
          model: editing.model ?? "",
          runtimes: editing.runtimes ?? [],
          runtime: editing.runtime ?? editing.runtimes?.[0] ?? "",
          mode: editing.options?.mode ?? "",
          effort: editing.options?.effort ?? "",
        }
      : {
          name: "",
          prompt: "",
          description: "",
          model: "",
          runtimes: [] as string[],
          runtime: "",
          mode: "",
          effort: "",
        },
  );
  const [busy, setBusy] = useState(false);
  const options = runtimes.map((r) => ({ value: r.name, label: r.name }));

  // The default runtime's option catalog (models / permission modes /
  // thinking levels) — cached daemon-side, so this is instant.
  const catalogRuntime = form.runtime || form.runtimes[0] || "";
  const [catalog, setCatalog] = useState<SessionOptionInfo[] | null>(null);
  useEffect(() => {
    if (!catalogRuntime) {
      setCatalog(null);
      return;
    }
    let alive = true;
    setCatalog(null);
    api
      .agentOptions(catalogRuntime)
      .then((r) => alive && setCatalog(r.options))
      .catch(() => alive && setCatalog([]));
    return () => {
      alive = false;
    };
  }, [catalogRuntime]);

  const find = (pred: (o: SessionOptionInfo) => boolean) =>
    (catalog ?? []).find(pred)?.choices ?? [];
  const isModel = (o: SessionOptionInfo) => o.category === "model" || o.id === "model";
  const isMode = (o: SessionOptionInfo) => o.category === "mode" || o.id === "mode";
  const isEffort = (o: SessionOptionInfo) =>
    o.category === "thought_level" || o.id === "effort" || o.id === "reasoning_effort";
  const modelChoices = find(isModel);
  const modeChoices = find(isMode);
  const effortChoices = find(isEffort);
  // Runtime without an advertisement: the config models list, else free text.
  const fallbackModels = runtimes.find((r) => r.name === catalogRuntime)?.models ?? [];

  const go = async () => {
    setBusy(true);
    try {
      const opts: Record<string, string> = {};
      if (form.mode) opts.mode = form.mode;
      if (form.effort) opts.effort = form.effort;
      const body = {
        prompt: form.prompt,
        description: form.description.trim() || undefined,
        model: form.model.trim() || undefined,
        runtimes: form.runtimes.length ? form.runtimes : undefined,
        runtime: form.runtime || form.runtimes[0] || undefined,
        // canonical defaults applied at chat start; `{}` clears on edit
        options: Object.keys(opts).length ? opts : editing ? {} : undefined,
      };
      if (editing) {
        await api.updateAgent(editing.name, body);
        toast("ok", t("agents.roleUpdated", { name: editing.name }));
      } else {
        await api.createAgent({ name: form.name.trim(), ...body });
        toast("ok", t("agents.roleCreated", { name: form.name.trim() }));
      }
      onSaved();
    } catch (e) {
      toast("err", String(e));
    } finally {
      setBusy(false);
    }
  };

  const hasRuntimeRef = form.runtimes.length > 0 || !!form.runtime || !!editing?.runtimes?.length;
  const ok = (editing || form.name.trim()) && form.prompt.trim() && hasRuntimeRef;

  return (
    <Modal
      title={editing ? t("agents.editTitle", { name: editing.name }) : t("agents.create")}
      onClose={onClose}
      wide
      footer={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button type="primary" loading={busy} disabled={!ok} onClick={go}>
            {editing ? t("common.save") : t("agents.create")}
          </Button>
        </>
      }
    >
      <label className="muted">{t("agents.f.name")}</label>
      <Input
        value={form.name}
        onChange={(e) => setForm((f) => ({ ...f, name: e.target.value }))}
        disabled={!!editing}
        placeholder="architect"
        style={{ marginBottom: 10 }}
      />
      <label className="muted">{t("agents.f.runtimes")}</label>
      <Select
        aria-label={t("agents.f.runtimes")}
        mode="multiple"
        value={form.runtimes}
        onChange={(v: string[]) => setForm((f) => ({ ...f, runtimes: v }))}
        options={options}
        placeholder={t("agents.f.runtimesPh")}
        style={{ width: "100%", marginBottom: 10 }}
      />
      <label className="muted">{t("agents.f.runtime")}</label>
      <Select
        aria-label={t("agents.f.runtime")}
        value={form.runtime || undefined}
        onChange={(v: string) => setForm((f) => ({ ...f, runtime: v }))}
        options={options}
        placeholder={t("agents.f.runtimePh")}
        style={{ width: "100%", marginBottom: 10 }}
      />
      <label className="muted">{t("agents.f.prompt")}</label>
      <Input.TextArea
        value={form.prompt}
        onChange={(e) => setForm((f) => ({ ...f, prompt: e.target.value }))}
        rows={6}
        placeholder="You are the architecture reviewer. …"
        style={{ marginBottom: 10 }}
      />
      <label className="muted">{t("agents.f.description")}</label>
      <Input
        value={form.description}
        onChange={(e) => setForm((f) => ({ ...f, description: e.target.value }))}
        style={{ marginBottom: 10 }}
      />
      <label className="muted">{t("agents.f.model")}</label>
      <Select
        aria-label={t("agents.f.model")}
        mode="tags"
        maxCount={1}
        value={form.model ? [form.model] : []}
        onChange={(v: string[]) => setForm((f) => ({ ...f, model: v[v.length - 1] ?? "" }))}
        loading={catalog === null}
        placeholder={t("chat.modelPh")}
        style={{ width: "100%", marginBottom: 10 }}
        options={
          modelChoices.length
            ? modelChoices.map((c) => ({ value: c.value, label: c.name }))
            : fallbackModels.map((m) => ({ value: m, label: m }))
        }
      />
      {modeChoices.length > 0 ? (
        <>
          <label className="muted">{t("agents.f.mode")}</label>
          <Select
            aria-label={t("agents.f.mode")}
            allowClear
            value={form.mode || undefined}
            onChange={(v: string) => setForm((f) => ({ ...f, mode: v ?? "" }))}
            options={modeChoices.map((c) => ({ value: c.value, label: c.name }))}
            placeholder="—"
            style={{ width: "100%", marginBottom: 10 }}
          />
        </>
      ) : null}
      {effortChoices.length > 0 ? (
        <>
          <label className="muted">{t("agents.f.effort")}</label>
          <Select
            aria-label={t("agents.f.effort")}
            allowClear
            value={form.effort || undefined}
            onChange={(v: string) => setForm((f) => ({ ...f, effort: v ?? "" }))}
            options={effortChoices.map((c) => ({ value: c.value, label: c.name }))}
            placeholder="—"
            style={{ width: "100%" }}
          />
        </>
      ) : null}
    </Modal>
  );
}

// ---------------------------------------------------------------------------
// Stats
// ---------------------------------------------------------------------------

/** A numeric cell: right-aligned through .row.end, not through antd's
 *  `align` (which writes an inline style into every td/th). */
const num = (v: ReactNode) => <div className="row tight end">{v}</div>;

export function Stats() {
  const { t } = useI18n();
  const [stats, setStats] = useState<AgentStats[] | null>(null);
  const [err, setErr] = useState<unknown>(null);
  // The bar length must not breathe: a shrinking max makes every bar jump
  // at once on each poll (§5 密集), so the scale is monotonic.
  const [maxCost, setMaxCost] = useState(0.0001);

  const load = () => {
    api
      .stats()
      .then((v) => {
        setStats(v);
        setErr(null);
        setMaxCost((prev) => Math.max(prev, ...v.map((s) => s.total_cost_usd), 0.0001));
      })
      // 行 20 / S12: a failed poll keeps the ledger on screen.
      .catch((e) => setErr(e));

  };
  // 行 39: #stats loads ONE endpoint now (stats) — t253 moved the recall log
  // to #memory, the page that produces it. K = 1, budget `2K + 2 = 4`; a 5s
  // timer spends 3 (mount + 2 ticks x 1 endpoint), so 10s keeps a margin of 2.
  // spends exactly 6 (mount + 2 ticks × 2 endpoints), i.e. zero margin. 10s
  // leaves 4.
  usePoll(load, 10000);

  const sums = useMemo(
    () => ({
      runs: (stats ?? []).reduce((s, x) => s + x.runs, 0),
      completed: (stats ?? []).reduce((s, x) => s + x.completed, 0),
      failed: (stats ?? []).reduce((s, x) => s + x.failed, 0),
      cost: (stats ?? []).reduce((s, x) => s + x.total_cost_usd, 0),
    }),
    [stats],
  );

  if (stats === null) {
    if (err) {
      return (
        <>
          <h1 className="sr-only micro">{t("stats.title")}</h1>
          <ErrorState
            title={t("stats.err")}
            hint={t("stats.err.hint")}
            onRetry={load}
            retryLabel={t("common.retry")}
          />
        </>
      );
    }
    return <Spinner label={`${t("stats.title")}…`} />;
  }

  return (
    <div>
      <h1 className="sr-only micro">{t("stats.title")}</h1>
      <div className="view-bar">
        <h2>{t("stats.title")}</h2>
        <span className="muted">{t("stats.subtitle")}</span>
      </div>

      {err && stats.length > 0 ? (
        <ErrorState
          title={t("common.stale")}
          hint={t("stats.stale.hint")}
          onRetry={load}
          retryLabel={t("common.retry")}
        />
      ) : null}

      {stats.length === 0 ? (
        <Empty icon="stats" title={t("stats.empty")} />
      ) : (
        <>
          {/* The four gauges are the four table columns summed — the reader
              can check the identity (S8). */}
          <ReadoutStrip
            grid
            items={[
              { key: "runs", label: t("stats.runs"), value: sums.runs },
              { key: "ok", label: t("stats.completed"), value: sums.completed },
              { key: "fail", label: t("stats.failed"), value: sums.failed },
              { key: "cost", label: t("stats.cost"), value: fmtUsd(sums.cost) },
            ]}
          />
          <Card size="small">
            <Table
              size="small"
              dataSource={stats}
              rowKey="agent"
              pagination={stats.length > 60 ? { pageSize: 60 } : false}
              // S1: six columns cannot fit 294px of content — the table
              // scrolls inside its own box instead of pushing the page.
              scroll={{ x: 560 }}
              columns={[
                { title: t("stats.agent"), dataIndex: "agent" },
                // Right alignment without antd's `align`: that prop is emitted
                // as an inline `text-align` on every cell (40 td + 4 th here),
                // and 行 12 caps a route at 50 inline styles. The shared .row
                // primitive right-aligns the same content with zero inline CSS.
                { title: num(t("stats.runs")), dataIndex: "runs", render: num },
                { title: num(t("stats.completed")), dataIndex: "completed", render: num },
                { title: num(t("stats.failed")), dataIndex: "failed", render: num },
                {
                  title: num(t("stats.cost")),
                  dataIndex: "total_cost_usd",
                  render: (v: number) => (
                    <Tooltip title={fmtUsd(v)}>
                      <span className="mono">
                        <Progress
                          // S3: the bar was antd's derived primary = the signal
                          // colour, which means "now" everywhere else. A cost
                          // bar is not "now"; it is a neutral magnitude.
                          percent={Math.round((v / maxCost) * 100)}
                          showInfo={false}
                          strokeColor="var(--ant-color-text-secondary)"
                          size={{ width: 72, height: 5 }}
                        />
                        {fmtUsd(v)}
                      </span>
                    </Tooltip>
                  ),
                },
                {
                  title: t("stats.lastRun"),
                  dataIndex: "last_run_at",
                  render: (v: string | null) => <RelTime iso={v} />,
                },
              ]}
            />
          </Card>
        </>
      )}

      {/* M6: the recall tuning dataset — what each call returned and
          the raw top scores the filters kept/dropped. Read-only: these are
          rows of a log, not 20 buttons that do nothing when pressed (S9). */}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Inbox
// ---------------------------------------------------------------------------

/** allow-kind first, reject-kind next, cancel last (§1) — the order must not
 *  depend on what the agent happened to send, or the wrong button gets hit. */
const kindRank = (kind: string) => {
  if (kind.startsWith("allow")) return 0;
  if (kind.startsWith("reject")) return 1;
  if (kind === "cancel") return 3;
  return 2;
};

const MAX_CHOICES = 6;
const MAX_PENDING = 30;
const RAW_LIMIT = 64 * 1024;

/** raw_input is `unknown`: a string must not render as a quoted JSON blob. */
function rawText(raw: unknown): string {
  if (raw == null) return "—";
  if (typeof raw === "string") return raw;
  try {
    return JSON.stringify(raw, null, 2) ?? String(raw);
  } catch {
    return String(raw);
  }
}

function rawSize(raw: unknown): number {
  if (raw == null) return 0;
  if (typeof raw === "string") return raw.length;
  try {
    return JSON.stringify(raw)?.length ?? 0;
  } catch {
    return 0;
  }
}

export function Inbox() {
  const { t } = useI18n();
  const [pending, setPending] = useState<PendingPermission[] | null>(null);
  const [err, setErr] = useState<unknown>(null);
  const [detail, setDetail] = useState<PendingPermission | null>(null);
  const [more, setMore] = useState<Record<string, boolean>>({});
  const [resolveErr, setResolveErr] = useState<string | null>(null);
  const toast = useToast();

  const load = () =>
    api
      .pendingPermissions()
      .then((p) => {
        setPending(p);
        setErr(null);
      })
      // I4: this catch used to render "the inbox is empty" while a run was
      // blocked on the very queue that failed to load.
      .catch((e) => setErr(e));
  usePoll(load, 2000);

  const resolve = async (
    p: PendingPermission,
    answer: { action: "allow" | "reject" | "cancel" } | { option_id: string },
    okLabel: string,
  ) => {
    try {
      await api.resolvePermission(p.run_id, p.tool_call_id, answer);
      setResolveErr(null);
      toast("ok", okLabel);
      load();
    } catch (e) {
      // I7: no optimistic removal — a failed decision must not look decided.
      setResolveErr(String(e));
      toast("err", String(e));
    }
  };

  // I6: a stable order. With a 2s poll and an API-ordered list, a card could
  // move between the moment you read it and the moment you click.
  const queue = useMemo(
    () =>
      [...(pending ?? [])].sort((a, b) =>
        `${a.run_id}:${a.tool_call_id}`.localeCompare(`${b.run_id}:${b.tool_call_id}`),
      ),
    [pending],
  );

  if (pending === null) {
    if (err) {
      return (
        <>
          <h1 className="sr-only micro">{t("inbox.title")}</h1>
          <ErrorState
            title={t("inbox.err")}
            hint={t("inbox.err.hint")}
            onRetry={load}
            retryLabel={t("common.retry")}
          />
        </>
      );
    }
    // I5: the first load shows a spinner instead of an empty inbox.
    return <Spinner label={`${t("inbox.title")}…`} />;
  }

  const visible = queue.slice(0, MAX_PENDING);

  return (
    <div>
      <h1 className="sr-only micro">{t("inbox.title")}</h1>
      <div className="view-bar">
        <h2>{t("inbox.title")}</h2>
        <span className="muted">{t("inbox.subtitle")}</span>
      </div>

      {err ? (
        <ErrorState
          title={t("inbox.err")}
          hint={t("inbox.stale.hint")}
          onRetry={load}
          retryLabel={t("common.retry")}
        />
      ) : null}
      {resolveErr ? <ErrorState title={t("inbox.resolveErr")} /> : null}

      {queue.length > MAX_PENDING ? (
        <p className="muted pad">{t("inbox.tooMany", { n: queue.length - MAX_PENDING })}</p>
      ) : null}

      {visible.length === 0 && !err ? (
        <Empty icon="inbox" title={t("inbox.empty.title")} hint={t("inbox.empty.hint")} />
      ) : (
        <div>
          {visible.map((p) => {
            const ranked = [...p.choices].sort((a, b) => kindRank(a.kind) - kindRank(b.kind));
            const key = `${p.run_id}:${p.tool_call_id}`;
            const expanded = more[key] ?? false;
            const shown = expanded ? ranked : ranked.slice(0, MAX_CHOICES);
            return (
              <Card key={key} size="small" className="inbox-card">
                <div className="row">
                  <span className="ev-icon">
                    <Icon name="lock" size={13} />
                  </span>
                  <span className="readout s truncated" title={p.title}>
                    {p.title}
                  </span>
                  <span className="grow" />
                  {/* run_id is not a task id — it cannot be linked anywhere. */}
                  <span className="muted mono">{p.run_id.slice(0, 8)}</span>
                </div>
                <Button
                  type="link"
                  style={{ paddingLeft: 0 }}
                  onClick={() => setDetail(p)}
                >
                  {t("inbox.viewRaw")}
                </Button>
                <div className="row wrap">
                  {ranked.length > 0 ? (
                    // The agent's own options (e.g. "Yes, allow reading
                    // during this session") — the exact option ids go to
                    // the API, so allow-always style grants are reachable
                    // (issue #35).
                    <>
                      {shown.map((c) => (
                        <Button
                          key={c.option_id}
                          type={c.kind.startsWith("allow") ? "primary" : undefined}
                          danger={c.kind.startsWith("reject")}
                          onClick={() => resolve(p, { option_id: c.option_id }, c.name)}
                        >
                          {c.name}
                        </Button>
                      ))}
                      {ranked.length > shown.length ? (
                        <Button onClick={() => setMore((m) => ({ ...m, [key]: true }))}>
                          {t("inbox.moreOptions")} (+{ranked.length - shown.length})
                        </Button>
                      ) : null}
                    </>
                  ) : (
                    <>
                      <Button
                        type="primary"
                        onClick={() => resolve(p, { action: "allow" }, t("inbox.allowed"))}
                      >
                        {t("inbox.allow")}
                      </Button>
                      <Button
                        danger
                        onClick={() => resolve(p, { action: "reject" }, t("inbox.rejected"))}
                      >
                        {t("inbox.reject")}
                      </Button>
                      <Button onClick={() => resolve(p, { action: "cancel" }, t("common.cancel"))}>
                        {t("common.cancel")}
                      </Button>
                    </>
                  )}
                </div>
              </Card>
            );
          })}
        </div>
      )}

      {detail && (
        <Modal title={detail.title} onClose={() => setDetail(null)} wide>
          {rawSize(detail.raw_input) > RAW_LIMIT ? (
            <p className="muted">
              {t("inbox.rawTooBig", { kb: Math.round(rawSize(detail.raw_input) / 1024) })}
            </p>
          ) : (
            <pre className="raw">{rawText(detail.raw_input)}</pre>
          )}
          <div className="row end">
            <Button
              type="primary"
              onClick={async () => {
                await resolve(detail, { action: "allow" }, t("inbox.allowed"));
                setDetail(null);
              }}
            >
              {t("common.allow")}
            </Button>
            <Button
              danger
              onClick={async () => {
                await resolve(detail, { action: "reject" }, t("inbox.rejected"));
                setDetail(null);
              }}
            >
              {t("common.reject")}
            </Button>
          </div>
        </Modal>
      )}
    </div>
  );
}
