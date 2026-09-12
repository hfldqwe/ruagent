// Agents (cards with live stats + MCP registry), Stats, Inbox.

import { useEffect, useState } from "react";
import { api, type AgentInfo, type AgentStats, type McpRegistry, type PendingPermission } from "../api";
import { Empty, Modal, Spinner, fmtUsd, relTime, useToast } from "../ui";

// ---------------------------------------------------------------------------
// Agents
// ---------------------------------------------------------------------------

export function Agents() {
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

  if (!agents) return <Spinner label="Loading agents…" />;
  return (
    <div>
      <div className="view-bar">
        <h2>Agents</h2>
        <span className="muted">registered harnesses with live performance</span>
      </div>
      {agents.length === 0 ? (
        <Empty icon="🤖" title="No agents registered" hint="Add them to ~/.ruagent/config/agents.toml and restart the daemon." />
      ) : (
        <div className="agent-grid">
          {agents.map((a) => {
            const s = stats.find((x) => x.agent === a.name);
            const success = s && s.runs > 0 ? Math.round((s.completed / s.runs) * 100) : null;
            return (
              <div key={a.name} className={a.enabled ? "card agent-card" : "card agent-card disabled"}>
                <div className="row">
                  <span className="agent-avatar">{a.name.slice(0, 2).toUpperCase()}</span>
                  <div>
                    <strong>{a.name}</strong>
                    <div className="muted">{a.harness}</div>
                  </div>
                  <span className="grow" />
                  {a.enabled ? <span className="tag ok">enabled</span> : <span className="tag">disabled</span>}
                </div>
                <p className="muted">{a.description}</p>
                {a.model ? <span className="tag mono">{a.model}</span> : null}
                {s && s.runs > 0 ? (
                  <div className="agent-stats">
                    <div className="stat-cell">
                      <span className="stat-num">{s.runs}</span>
                      <span className="muted">runs</span>
                    </div>
                    <div className="stat-cell">
                      <span className="stat-num">{success}%</span>
                      <span className="muted">success</span>
                    </div>
                    <div className="stat-cell">
                      <span className="stat-num">{fmtUsd(s.total_cost_usd)}</span>
                      <span className="muted">cost</span>
                    </div>
                    <div className="stat-cell">
                      <span className="stat-num">{relTime(s.last_run_at)}</span>
                      <span className="muted">last run</span>
                    </div>
                  </div>
                ) : (
                  <p className="muted">no runs yet</p>
                )}
              </div>
            );
          })}
        </div>
      )}

      <h3>MCP Registry</h3>
      {mcp && mcp.servers.length > 0 ? (
        <div className="card">
          {mcp.servers.map((s) => (
            <div key={s.name} className="row">
              <span className="doc-icon">🔌</span>
              <strong>{s.name}</strong>
              {s.inject_for ? <span className="tag">{s.inject_for.join(", ")}</span> : null}
              <span className="grow" />
              <span className="muted mono truncated">{s.url ?? s.command}</span>
            </div>
          ))}
          <p className="muted pad">
            Injection is an overlay — each CLI's own MCP config is never touched.
          </p>
        </div>
      ) : (
        <Empty icon="🔌" title="No MCP servers registered" hint="Add them to ~/.ruagent/config/mcp.toml (design §7.1)." />
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Stats
// ---------------------------------------------------------------------------

export function Stats() {
  const [stats, setStats] = useState<AgentStats[] | null>(null);
  useEffect(() => {
    api.stats().then(setStats).catch(() => setStats([]));
    const t = setInterval(() => api.stats().then(setStats).catch(() => {}), 5000);
    return () => clearInterval(t);
  }, []);
  if (!stats) return <Spinner label="Loading stats…" />;
  if (stats.length === 0) return <Empty icon="📊" title="No runs recorded yet" />;
  const maxCost = Math.max(...stats.map((s) => s.total_cost_usd), 0.0001);
  return (
    <div>
      <div className="view-bar">
        <h2>Stats</h2>
        <span className="muted">per-agent performance and cost</span>
      </div>
      <div className="card">
        <table className="stats">
          <thead>
            <tr>
              <th>Agent</th>
              <th>Runs</th>
              <th>Completed</th>
              <th>Failed</th>
              <th>Cost</th>
              <th>Last Run</th>
            </tr>
          </thead>
          <tbody>
            {stats.map((s) => (
              <tr key={s.agent}>
                <td>{s.agent}</td>
                <td>{s.runs}</td>
                <td>{s.completed}</td>
                <td>{s.failed}</td>
                <td>
                  <div className="cost-cell">
                    <span className="cost-bar">
                      <span
                        className="cost-fill"
                        style={{ width: `${(s.total_cost_usd / maxCost) * 100}%` }}
                      />
                    </span>
                    <span className="mono">{fmtUsd(s.total_cost_usd)}</span>
                  </div>
                </td>
                <td className="muted">{relTime(s.last_run_at)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Inbox
// ---------------------------------------------------------------------------

export function Inbox() {
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
      toast("ok", `${action}ed`);
      refresh();
    } catch (e) {
      toast("err", String(e));
    }
  };

  if (!pending) return <Spinner label="Loading inbox…" />;
  return (
    <div>
      <div className="view-bar">
        <h2>Inbox</h2>
        <span className="muted">permission requests waiting on you</span>
      </div>
      {pending.length === 0 ? (
        <Empty
          icon="✅"
          title="Inbox empty"
          hint="When an agent wants to do something the rules don't cover, it lands here. Configure an approver agent for unattended operation."
        />
      ) : (
        <div>
          {pending.map((p) => (
            <div key={`${p.run_id}:${p.tool_call_id}`} className="card inbox-card">
              <div className="row">
                <span className="ev-icon">🔐</span>
                <strong>{p.title}</strong>
                <span className="grow" />
                <span className="muted mono">{p.run_id.slice(0, 8)}</span>
              </div>
              <button className="link" onClick={() => setDetail(p)}>
                View Raw Input
              </button>
              <div className="row">
                <button className="primary" onClick={() => resolve(p, "allow")}>
                  Allow
                </button>
                <button className="danger" onClick={() => resolve(p, "reject")}>
                  Reject
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
      {detail && (
        <Modal title={detail.title} onClose={() => setDetail(null)} wide>
          <pre className="raw">{JSON.stringify(detail.raw_input, null, 2)}</pre>
          <div className="row end">
            <button
              className="primary"
              onClick={async () => {
                await resolve(detail, "allow");
                setDetail(null);
              }}
            >
              Allow
            </button>
            <button
              className="danger"
              onClick={async () => {
                await resolve(detail, "reject");
                setDetail(null);
              }}
            >
              Reject
            </button>
          </div>
        </Modal>
      )}
    </div>
  );
}
