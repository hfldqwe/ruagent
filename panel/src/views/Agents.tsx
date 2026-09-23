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
  type RecallLogRow,
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
  Zone,
  fmtUsd,
  useToast,
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
  const toast = useToast();

  // One request window for agents + stats + mcp instead of two staggered
  // 5s timers (A10). Each leg keeps the last good value on failure.
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
  };
  // 行 39: agents + stats + mcp are three endpoints, so the window's budget
  // is `2K + 2 = 8`. A 5s timer spends 3 per endpoint (mount + 2 ticks) = 9
  // — measured 262 / 5276 / 10275, evenly spaced: cadence, not duplicate
  // fetches. 10s leaves mount + one tick = 6. Registry edits still refresh
  // immediately (the create/edit paths call load() themselves).
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
                  <span className="grow" />
                  {a.runtime ? <span className="tag">{a.runtime}</span> : null}
                  {(a.runtimes?.length ?? 0) > 1 ? (
                    <span className="tag">{t("agents.portable", { n: a.runtimes!.length })}</span>
                  ) : null}
                  {a.enabled ? (
                    <span className="tag ok">{t("agents.enabled")}</span>
                  ) : (
                    <span className="tag">{t("agents.disabled")}</span>
                  )}
                </div>
                <p className="muted" style={{ margin: "10px 0 6px" }}>
                  {a.description}
                </p>
                {a.prompt ? (
                  <details className="prompt-view">
                    <summary>
                      {t("agents.promptView")} · {a.prompt.length}{" "}
                      {t("chat.chars", { n: a.prompt.length }).split(" ")[1] ?? ""}
                    </summary>
                    <pre>{a.prompt}</pre>
                  </details>
                ) : null}
                {a.model ? <span className="tag mono">{a.model}</span> : null}
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
                      <Tooltip
                        title={`${s.health.tools ?? 0} ${t("mcp.tools")} · ${s.health.latency_ms}ms`}
                      >
                        <span className="tag ok">
                          {t("mcp.up")} · {s.health.tools ?? 0}
                        </span>
                      </Tooltip>
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
  const [recallLog, setRecallLog] = useState<RecallLogRow[] | null>(null);
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
    api
      .recallLog(20)
      .then(setRecallLog)
      .catch(() => setRecallLog((prev) => prev));
  };
  // 行 39: stats + recall/log is K = 2, budget `2K + 2 = 6`; a 5s timer
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
      {recallLog && recallLog.length > 0 && (
        <Zone title={t("stats.recallLog")} note={t("stats.rows", { n: recallLog.length })}>
          <div className="card">
            {recallLog.map((r, i) => (
              <div key={`${r.ts}-${i}`} className="row">
                <span className="mono truncated">{r.query}</span>
                <span className="tag">
                  {r.strategy === "conservative"
                    ? t("memory.recallConservative")
                    : t("memory.recallAggressive")}
                </span>
                <span className="micro muted">{t("stats.topN", { n: r.top_n })}</span>
                <span className="mono muted">
                  mem {r.memories} · know {r.knowledge} · wiki {r.wiki} · ent {r.entities}
                </span>
                <span className="grow" />
                {r.top_memory_score != null && (
                  <span className="muted mono micro">m {r.top_memory_score.toFixed(2)}</span>
                )}
                {r.top_knowledge_score != null && (
                  <span className="muted mono micro">k {r.top_knowledge_score.toFixed(2)}</span>
                )}
                <span className="time">
                  <RelTime iso={r.ts} />
                </span>
              </div>
            ))}
          </div>
        </Zone>
      )}
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
