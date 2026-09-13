// Session history: every agent CLI's conversations, auto-synced by the
// daemon (claude-code / dsh / ruagent). Index + on-demand viewer.

import { useEffect, useState } from "react";
import { Button, Segmented, Tag, Tooltip } from "antd";
import { ThunderboltOutlined } from "@ant-design/icons";
import { api, type SessionRecord } from "../api";
import { useI18n } from "../i18n";
import { Empty, Markdown, RelTime, Spinner, useToast } from "../ui";

const SOURCE_LABEL: Record<string, string> = {
  "claude-code": "Claude Code",
  dsh: "dsh",
  ruagent: "ruagent",
};

function sourceColor(s: string): string {
  switch (s) {
    case "claude-code":
      return "orange";
    case "dsh":
      return "blue";
    default:
      return "green";
  }
}

export function Sessions() {
  const { t } = useI18n();
  const [sessions, setSessions] = useState<SessionRecord[] | null>(null);
  const [filter, setFilter] = useState<string>("all");
  const [open, setOpen] = useState<SessionRecord | null>(null);
  const [distilling, setDistilling] = useState<string | null>(null);
  const toast = useToast();

  const distill = async (s: SessionRecord) => {
    setDistilling(s.key);
    try {
      const r = await api.sessionDistill(s.key);
      const d = r.distilled;
      toast(
        "ok",
        `${d.memories_written} 记忆 · ${d.entities_written} 实体 · ${d.relations_written} 关系（${d.memories_skipped} 跳过）`,
      );
    } catch (e) {
      toast("err", String(e));
    } finally {
      setDistilling(null);
    }
  };

  useEffect(() => {
    const load = () =>
      api
        .sessions()
        .then(setSessions)
        .catch(() => setSessions([]));
    load();
    const i = setInterval(load, 10000);
    return () => clearInterval(i);
  }, []);

  if (!sessions) return <Spinner label={`${t("sessions.title")}…`} />;

  const filtered =
    filter === "all" ? sessions : sessions.filter((s) => s.source === filter);
  const sources = [...new Set(sessions.map((s) => s.source))];

  return (
    <div>
      <div className="view-bar">
        <h2>{t("sessions.title")}</h2>
        <span className="muted">
          {t("sessions.subtitle", { n: sessions.length })}
        </span>
        <span className="grow" />
        <Segmented
          value={filter}
          onChange={(v) => setFilter(v as string)}
          options={[
            { value: "all", label: t("sessions.all") },
            ...sources.map((s) => ({
              value: s,
              label: SOURCE_LABEL[s] ?? s,
            })),
          ]}
        />
      </div>

      {filtered.length === 0 ? (
        <Empty icon="chat" title={t("sessions.empty")} hint={t("sessions.emptyHint")} />
      ) : (
        <div className="card">
          {filtered.map((s) => (
            <button
              key={s.key}
              className="row-btn"
              onClick={() => setOpen(s)}
            >
              <Tag color={sourceColor(s.source)} style={{ marginInlineEnd: 0 }}>
                {SOURCE_LABEL[s.source] ?? s.source}
              </Tag>
              <span className="title">
                <strong>{s.title || s.preview || t("sessions.untitled")}</strong>
                {s.preview && s.title ? (
                  <span className="muted" style={{ marginLeft: 8, fontSize: 12 }}>
                    {s.preview.slice(0, 60)}
                  </span>
                ) : null}
              </span>
              <span className="muted mono" style={{ fontSize: 11 }}>
                {s.message_count} {t("sessions.messages")}
              </span>
              {s.project ? (
                <span className="tag">{shortProject(s.project)}</span>
              ) : null}
              <span className="time">
                <RelTime iso={msToIso(s.updated_at)} />
              </span>
              <Tooltip title={t("sessions.distillHint")}>
                <Button
                  size="small"
                  type="text"
                  icon={<ThunderboltOutlined />}
                  loading={distilling === s.key}
                  onClick={(e) => {
                    e.stopPropagation();
                    distill(s);
                  }}
                />
              </Tooltip>
            </button>
          ))}
        </div>
      )}

      {open && <SessionDetail session={open} onClose={() => setOpen(null)} />}
    </div>
  );
}

function SessionDetail({
  session,
  onClose,
}: {
  session: SessionRecord;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const [messages, setMessages] = useState<{ role: string; text: string }[] | null>(null);

  useEffect(() => {
    api
      .sessionMessages(session.key)
      .then(setMessages)
      .catch(() => setMessages([]));
  }, [session.key]);

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal wide session-viewer"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="modal-head">
          <h3>
            {session.title || t("sessions.untitled")}
            <span className="muted" style={{ marginLeft: 10, fontSize: 12, fontWeight: 400 }}>
              {SOURCE_LABEL[session.source] ?? session.source}
              {session.project ? ` · ${session.project}` : ""}
            </span>
          </h3>
          <Button type="text" size="small" onClick={onClose} aria-label="close">
            ✕
          </Button>
        </div>
        <div className="modal-body session-log">
          {messages === null ? (
            <Spinner />
          ) : messages.length === 0 ? (
            <Empty icon="chat" title={t("sessions.noMessages")} />
          ) : (
            messages.map((m, i) => (
              <div
                key={i}
                className={
                  m.role === "user" ? "chat-msg user" : "chat-msg agent"
                }
              >
                {m.role === "user" ? m.text : <Markdown>{m.text}</Markdown>}
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
}

function shortProject(p: string): string {
  // ruagent-spawned agent workspaces collapse to a readable label.
  const m = p.match(/[\/]workspaces[\/](chat|run)-([0-9a-f]{8})/);
  if (m) return `ruagent ${m[1]} ·${m[2]}`;
  const parts = p.split(/[\/]/);
  return parts.slice(-2).join("/");
}

function msToIso(ms: number): string {
  return ms > 0 ? new Date(ms).toISOString() : "";
}
