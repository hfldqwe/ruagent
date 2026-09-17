// Agents (two layers on separate tabs, design §4.1: roles with live
// stats + MCP registry / runtimes — the execution backends), Stats, Inbox.

import { useEffect, useState } from "react";
import { Button, Card, Progress, Segmented, Table, Tooltip } from "antd";
import {
  api,
  type AgentInfo,
  type AgentStats,
  type McpRegistry,
  type PendingPermission,
  type RecallLogRow,
  isRoleAgent,
} from "../api";
import { Empty, Modal, RelTime, Spinner, fmtUsd, useToast } from "../ui";
import { useI18n } from "../i18n";
import { Icon } from "../icons";

// ---------------------------------------------------------------------------
// Agents
// ---------------------------------------------------------------------------

export function Agents() {
  const { t } = useI18n();
  const [tab, setTab] = useState<"agents" | "runtimes">("agents");
  const [agents, setAgents] = useState<AgentInfo[] | null>(null);
  const [stats, setStats] = useState<AgentStats[]>([]);
  const [mcp, setMcp] = useState<McpRegistry | null>(null);
  useEffect(() => {
    const load = () => {
      api.agents().then(setAgents).catch(() => setAgents([]));
      api.stats().then(setStats).catch(() => {});
    };
    load();
    api.mcp().then(setMcp).catch(() => {});
    const t = setInterval(load, 5000);
    return () => clearInterval(t);
  }, []);

  if (!agents) return <Spinner label={`${t("agents.title")}…`} />;
  const roles = agents.filter(isRoleAgent);
  const runtimes = agents.filter((a) => !isRoleAgent(a));

  return (
    <div>
      <div className="view-bar">
        <h2>{t("agents.title")}</h2>
        <span className="muted">{t("agents.subtitle")}</span>
        <Segmented
          value={tab}
          onChange={(v) => setTab(v as "agents" | "runtimes")}
          options={[
            { value: "agents", label: t("agents.tab.agents") },
            { value: "runtimes", label: t("agents.tab.runtimes") },
          ]}
        />
      </div>

      {tab === "agents" ? (
        <>
          {roles.length === 0 ? (
            <Empty icon="bot" title={t("agents.empty.title")} hint={t("agents.empty.hint")} />
          ) : (
            <div className="agent-grid">
              {roles.map((a) => {
                const s = stats.find((x) => x.agent === a.name);
                const success =
                  s && s.runs > 0 ? Math.round((s.completed / s.runs) * 100) : null;
                return (
                  <Card
                    key={a.name}
                    className={a.enabled ? "agent-card" : "agent-card disabled"}
                    size="small"
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
                        <span className="tag">
                          {t("agents.portable", { n: a.runtimes!.length })}
                        </span>
                      ) : null}
                      {a.enabled ? (
                        <span className="tag ok">{t("agents.enabled")}</span>
                      ) : (
                        <span className="tag">{t("agents.disabled")}</span>
                      )}
                    </div>
                    <p className="muted" style={{ margin: "10px 0 6px" }}>{a.description}</p>
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
                          <span className="stat-num"><RelTime iso={s.last_run_at} /></span>
                          <span className="muted">{t("agents.lastRun")}</span>
                        </div>
                      </div>
                    ) : (
                      <p className="muted" style={{ margin: "10px 0 0" }}>{t("agents.noRuns")}</p>
                    )}
                  </Card>
                );
              })}
            </div>
          )}

          <h3 className="sec">{t("mcp.registry")}</h3>
          {mcp && mcp.servers.length > 0 ? (
            <Card size="small">
              {mcp.servers.map((s) => (
                <div key={s.name} className="row">
                  <span className="doc-icon">
                    <Icon name="plug" size={15} />
                  </span>
                  <strong>{s.name}</strong>
                  {s.inject_for ? <span className="tag">{s.inject_for.join(", ")}</span> : null}
                  <span className="grow" />
                  <span className="muted mono truncated">{s.url ?? s.command}</span>
                </div>
              ))}
              <p className="muted pad">{t("mcp.overlay")}</p>
            </Card>
          ) : (
            <Empty icon="plug" title={t("mcp.empty.title")} hint={t("mcp.empty.hint")} />
          )}
        </>
      ) : (
        <RuntimeGrid runtimes={runtimes} roles={roles} />
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Runtimes tab — the execution backends (claude-code / dsh / opencode / …)
// ---------------------------------------------------------------------------

function RuntimeGrid({ runtimes, roles }: { runtimes: AgentInfo[]; roles: AgentInfo[] }) {
  const { t } = useI18n();
  if (runtimes.length === 0)
    return (
      <Empty icon="tool" title={t("agents.runtimes.empty")} hint={t("agents.runtimes.hint")} />
    );
  return (
    <div className="agent-grid">
      {runtimes.map((r) => {
        const usedBy = roles.filter((a) => a.runtimes?.includes(r.name));
        return (
          <Card
            key={r.name}
            className={r.enabled ? "agent-card" : "agent-card disabled"}
            size="small"
          >
            <div className="row">
              <span className="agent-avatar">{r.name.slice(0, 2).toUpperCase()}</span>
              <div>
                <strong>{r.name}</strong>
                <div className="muted">{r.harness}</div>
              </div>
              <span className="grow" />
              {r.enabled ? (
                <span className="tag ok">{t("agents.enabled")}</span>
              ) : (
                <span className="tag">{t("agents.disabled")}</span>
              )}
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
              {usedBy.length > 0 ? (
                usedBy.map((a) => <span key={a.name} className="tag">{a.name}</span>)
              ) : (
                <span className="muted" style={{ fontSize: 12 }}>
                  {t("agents.runtimes.noRoles")}
                </span>
              )}
              <span className="grow" />
              {r.enabled ? (
                <Button
                  size="small"
                  onClick={() => {
                    window.location.hash = `chat?agent=${encodeURIComponent(r.name)}`;
                  }}
                >
                  {t("agents.runtimes.direct")}
                </Button>
              ) : null}
            </div>
          </Card>
        );
      })}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Stats
// ---------------------------------------------------------------------------

export function Stats() {
  const { t } = useI18n();
  const [stats, setStats] = useState<AgentStats[] | null>(null);
  const [recallLog, setRecallLog] = useState<RecallLogRow[] | null>(null);
  useEffect(() => {
    api.stats().then(setStats).catch(() => setStats([]));
    api.recallLog(20).then(setRecallLog).catch(() => setRecallLog([]));
    const t = setInterval(() => api.stats().then(setStats).catch(() => {}), 5000);
    return () => clearInterval(t);
  }, []);
  if (!stats) return <Spinner label={`${t("stats.title")}…`} />;
  if (stats.length === 0)
    return (
      <div>
        <div className="view-bar">
          <h2>{t("stats.title")}</h2>
          <span className="muted">{t("stats.subtitle")}</span>
        </div>
        <Empty icon="stats" title={t("stats.empty")} />
      </div>
    );
  const maxCost = Math.max(...stats.map((s) => s.total_cost_usd), 0.0001);
  return (
    <div>
      <div className="view-bar">
        <h2>{t("stats.title")}</h2>
        <span className="muted">{t("stats.subtitle")}</span>
      </div>
      <Card size="small">
        <Table
          size="small"
          dataSource={stats}
          rowKey="agent"
          pagination={false}
          columns={[
            { title: t("stats.agent"), dataIndex: "agent" },
            { title: t("stats.runs"), dataIndex: "runs", align: "right" },
            { title: t("stats.completed"), dataIndex: "completed", align: "right" },
            { title: t("stats.failed"), dataIndex: "failed", align: "right" },
            {
              title: t("stats.cost"),
              dataIndex: "total_cost_usd",
              align: "right",
              render: (v: number) => (
                <Tooltip title={fmtUsd(v)}>
                  <span className="mono" style={{ display: "inline-flex", alignItems: "center", gap: 8 }}>
                    <Progress
                      percent={Math.round((v / maxCost) * 100)}
                      showInfo={false}
                      size={{ width: 72, height: 5 }}
                      style={{ margin: 0 }}
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

      {/* M6: the recall tuning dataset — what each call returned and
          the raw top scores the filters kept/dropped */}
      {recallLog && recallLog.length > 0 && (
        <>
          <div className="dash-head" style={{ marginTop: 18 }}>{t("stats.recallLog")}</div>
          <div className="card">
            {recallLog.map((r, i) => (
              <div key={i} className="row-btn" style={{ cursor: "default" }}>
                <span className="mono" style={{ fontSize: 12 }}>{r.query}</span>
                <span className="tag">{r.strategy === "conservative" ? "保守" : "激进"}</span>
                <span className="muted" style={{ fontSize: 12 }}>
                  mem {r.memories} · know {r.knowledge} · wiki {r.wiki} · ent {r.entities}
                </span>
                <span className="grow" />
                {r.top_memory_score != null && (
                  <span className="muted mono" style={{ fontSize: 11 }}>
                    m {r.top_memory_score.toFixed(2)}
                  </span>
                )}
                {r.top_knowledge_score != null && (
                  <span className="muted mono" style={{ fontSize: 11 }}>
                    k {r.top_knowledge_score.toFixed(2)}
                  </span>
                )}
                <span className="time"><RelTime iso={r.ts} /></span>
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Inbox
// ---------------------------------------------------------------------------

export function Inbox() {
  const { t } = useI18n();
  const [pending, setPending] = useState<PendingPermission[] | null>(null);
  const [detail, setDetail] = useState<PendingPermission | null>(null);
  const toast = useToast();

  const refresh = () =>
    api
      .pendingPermissions()
      .then(setPending)
      .catch(() => setPending([]));

  useEffect(() => {
    refresh();
    const t = setInterval(refresh, 2000);
    return () => clearInterval(t);
  }, []);

  const resolve = async (p: PendingPermission, action: string) => {
    try {
      await api.resolvePermission(p.run_id, p.tool_call_id, action);
      toast("ok", action === "allow" ? t("inbox.allowed") : t("inbox.rejected"));
      refresh();
    } catch (e) {
      toast("err", String(e));
    }
  };

  if (!pending) return <Spinner label={`${t("inbox.title")}…`} />;
  return (
    <div>
      <div className="view-bar">
        <h2>{t("inbox.title")}</h2>
        <span className="muted">{t("inbox.subtitle")}</span>
      </div>
      {pending.length === 0 ? (
        <Empty
          icon="check"
          title={t("inbox.empty.title")}
          hint={t("inbox.empty.hint")}
        />
      ) : (
        <div>
          {pending.map((p) => (
            <Card key={`${p.run_id}:${p.tool_call_id}`} size="small" className="inbox-card">
              <div className="row">
                <span className="ev-icon"><Icon name="lock" size={13} /></span>
                <strong>{p.title}</strong>
                <span className="grow" />
                <span className="muted mono">{p.run_id.slice(0, 8)}</span>
              </div>
              <Button type="link" size="small" style={{ paddingLeft: 0 }} onClick={() => setDetail(p)}>
                {t("inbox.viewRaw")}
              </Button>
              <div className="row">
                <Button type="primary" onClick={() => resolve(p, "allow")}>
                  {t("inbox.allow")}
                </Button>
                <Button danger onClick={() => resolve(p, "reject")}>
                  {t("inbox.reject")}
                </Button>
              </div>
            </Card>
          ))}
        </div>
      )}
      {detail && (
        <Modal title={detail.title} onClose={() => setDetail(null)} wide>
          <pre className="raw">{JSON.stringify(detail.raw_input, null, 2)}</pre>
          <div className="row end">
            <Button
              type="primary"
              onClick={async () => {
                await resolve(detail, "allow");
                setDetail(null);
              }}
            >
              {t("common.allow")}
            </Button>
            <Button
              danger
              onClick={async () => {
                await resolve(detail, "reject");
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
