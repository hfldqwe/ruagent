// Runtimes view — the execution backends (design §4.1 two-layer model,
// user ruling 2026-09-17: claude-code / dsh / opencode are RUNTIMES, a
// sibling page to the Agents view; agents are portable roles). This is
// the connection-management surface: what backends exist, how they spawn,
// which roles run on them, and a direct-chat entry.

import { useEffect, useState } from "react";
import { Button, Card } from "antd";
import { api, isRoleAgent, type AgentInfo } from "../api";
import { Empty, Spinner } from "../ui";
import { useI18n } from "../i18n";
import { Icon } from "../icons";

export function Runtimes() {
  const { t } = useI18n();
  const [agents, setAgents] = useState<AgentInfo[] | null>(null);
  useEffect(() => {
    const load = () => api.agents().then(setAgents).catch(() => setAgents([]));
    load();
    const t = setInterval(load, 5000);
    return () => clearInterval(t);
  }, []);

  if (!agents) return <Spinner label={`${t("runtimes.title")}…`} />;
  const roles = agents.filter(isRoleAgent);
  // Disabled runtimes are configured off — the mock agent template for
  // instance never belongs on the user surface.
  const runtimes = agents.filter((a) => !isRoleAgent(a) && a.enabled);

  return (
    <div>
      <div className="view-bar">
        <h2>{t("runtimes.title")}</h2>
        <span className="muted">{t("runtimes.subtitle")}</span>
      </div>
      {runtimes.length === 0 ? (
        <Empty icon="layers" title={t("agents.runtimes.empty")} hint={t("agents.runtimes.hint")} />
      ) : (
        <div className="agent-grid">
          {runtimes.map((r) => {
            const usedBy = roles.filter((a) => a.runtimes?.includes(r.name));
            return (
              <Card key={r.name} className="agent-card" size="small">
                <div className="row">
                  <span className="agent-avatar">{r.name.slice(0, 2).toUpperCase()}</span>
                  <div>
                    <strong>{r.name}</strong>
                    <div className="muted">{r.harness}</div>
                  </div>
                  <span className="grow" />
                  <span className="tag ok">{t("agents.enabled")}</span>
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
                  <Button
                    size="small"
                    onClick={() => {
                      window.location.hash = `chat?agent=${encodeURIComponent(r.name)}`;
                    }}
                  >
                    {t("agents.runtimes.direct")}
                  </Button>
                </div>
              </Card>
            );
          })}
        </div>
      )}
      <p className="muted pad" style={{ marginTop: 14 }}>{t("runtimes.hint")}</p>
    </div>
  );
}
