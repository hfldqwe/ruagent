// Chat: terminal-like conversations — pick agent, pick model, talk
// multi-turn on one persistent session.

import { useEffect, useRef, useState } from "react";
import { api, type AgentInfo, type ModelChoice } from "../api";
import { useI18n } from "../i18n";
import { Markdown, Spinner } from "../ui";

interface Message {
  role: "user" | "assistant";
  text: string;
  /** assistant streaming accumulation */
  done: boolean;
}

interface ChatEvent {
  type: string;
  [k: string]: unknown;
}

export function Chat({ initialAgent }: { initialAgent?: string }) {
  const { t } = useI18n();
  const [agents, setAgents] = useState<AgentInfo[] | null>(null);
  const [agent, setAgent] = useState(initialAgent ?? "");
  const [model, setModel] = useState("");
  /** Live catalog advertised by the agent (ACP session config). */
  const [choices, setChoices] = useState<ModelChoice[] | null>(null);
  /** Fallback list from config (agents.toml `models`). */
  const [configModels, setConfigModels] = useState<string[]>([]);
  const [chatId, setChatId] = useState<string | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState("");
  const [streaming, setStreaming] = useState(false);
  const [starting, setStarting] = useState(false);
  const bottomRef = useRef<HTMLDivElement>(null);
  const streamRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    api.agents().then((a) => {
      const enabled = a.filter((x) => x.enabled);
      setAgents(enabled);
      setAgent((cur) =>
        cur && enabled.some((a) => a.name === cur) ? cur : (enabled[0]?.name ?? ""),
      );
    }).catch(() => setAgents([]));
  }, []);

  // Live model catalog for the selected agent: what the agent itself
  // advertises over ACP (may probe-spawn it once, then cached server-side).
  // Falls back to the config list, then to free text.
  useEffect(() => {
    if (!agent) return;
    const a = agents?.find((x) => x.name === agent);
    setConfigModels(a?.models ?? []);
    setChoices(null);
    let alive = true;
    api
      .agentModels(agent)
      .then((r) => {
        if (!alive) return;
        setChoices(r.models);
        if (r.current) setModel(r.current);
      })
      .catch(() => {
        if (alive) setChoices([]);
      });
    return () => {
      alive = false;
    };
  }, [agent, agents]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ block: "end" });
  }, [messages]);

  const ensureChat = async (): Promise<string> => {
    if (chatId) return chatId;
    setStarting(true);
    try {
      const chat = await api.chatStart(agent, model.trim() || null);
      setChatId(chat.id);
      setModel(chat.model ?? "");
      return chat.id;
    } finally {
      setStarting(false);
    }
  };

  const attachStream = (id: string) => {
    if (streamRef.current) streamRef.current();
    const es = new EventSource(`/api/v1/chat/${id}/events`);
    const close = () => es.close();
    streamRef.current = close;
    es.onmessage = (e) => {
      try {
        const ev = JSON.parse(e.data) as ChatEvent;
        handleEvent(ev);
      } catch {
        /* skip */
      }
    };
    es.addEventListener("end", () => {
      es.close();
      setStreaming(false);
    });
    es.onerror = () => {
      es.close();
      setStreaming(false);
    };
  };

  const handleEvent = (ev: ChatEvent) => {
    switch (ev.type) {
      case "agent_message_chunk": {
        const content = ev.content as { text?: string }[];
        const text = content.map((c) => c.text ?? "").join("");
        if (!text) return;
        setMessages((prev) => {
          const next = [...prev];
          const last = next[next.length - 1];
          if (last && last.role === "assistant" && !last.done) {
            next[next.length - 1] = { ...last, text: last.text + text };
          } else {
            next.push({ role: "assistant", text, done: false });
          }
          return next;
        });
        break;
      }
      case "stopped": {
        setMessages((prev) => {
          const next = [...prev];
          const last = next[next.length - 1];
          if (last && last.role === "assistant") {
            next[next.length - 1] = { ...last, done: true };
          }
          return next;
        });
        setStreaming(false);
        break;
      }
      case "error": {
        const msg = String(ev.message);
        setMessages((prev) => [
          ...prev.map((m) => (m.role === "assistant" ? { ...m, done: true } : m)),
          { role: "assistant", text: `⚠️ ${msg}`, done: true },
        ]);
        setStreaming(false);
        break;
      }
      case "permission_requested": {
        setMessages((prev) => [
          ...prev,
          {
            role: "assistant",
            text: `🔐 ${t("timeline.permReq", { title: String(ev.title) })} → ${t("nav.inbox")}`,
            done: true,
          },
        ]);
        break;
      }
      default:
        break;
    }
  };

  const send = async () => {
    const text = input.trim();
    if (!text || streaming || starting || !agent) return;
    setInput("");
    setMessages((prev) => [...prev, { role: "user", text, done: true }]);
    setStreaming(true);
    try {
      const id = await ensureChat();
      attachStream(id);
      await api.chatMessage(id, text);
    } catch (e) {
      setMessages((prev) => [
        ...prev,
        { role: "assistant", text: `⚠️ ${String(e)}`, done: true },
      ]);
      setStreaming(false);
    }
  };

  const newChat = () => {
    if (streamRef.current) streamRef.current();
    streamRef.current = null;
    setChatId(null);
    setMessages([]);
    setStreaming(false);
  };

  const switchModel = async (m: string) => {
    setModel(m);
    if (!chatId) return;
    try {
      const chat = await api.chatModel(chatId, m.trim() || null);
      if (chat.switched === "live") {
        // Same session — context preserved.
        setModel(chat.model ?? m);
        setMessages((prev) => [
          ...prev,
          {
            role: "assistant",
            text: `⚙️ ${t("chat.modelLive")} → \`${chat.model ?? m}\``,
            done: true,
          },
        ]);
      } else {
        // Restarted: new session, context reset; reattach the stream.
        if (streamRef.current) streamRef.current();
        setChatId(chat.id);
        setModel(chat.model ?? m);
        setMessages((prev) => [
          ...prev,
          {
            role: "assistant",
            text: `⚙️ ${t("chat.modelSwitched")} → \`${chat.model ?? m}\``,
            done: true,
          },
        ]);
        attachStream(chat.id);
      }
    } catch (e) {
      setMessages((prev) => [
        ...prev,
        { role: "assistant", text: `⚠️ ${String(e)}`, done: true },
      ]);
    }
  };

  if (!agents) return <Spinner label={`${t("chat.title")}…`} />;

  return (
    <div>
      <div className="view-bar">
        <h2>{t("chat.title")}</h2>
        <span className="muted">{t("chat.subtitle")}</span>
        <span className="grow" />
        {chatId ? (
          <button onClick={newChat}>+ {t("chat.new")}</button>
        ) : null}
      </div>

      <div className="card chat-bar">
        <label className="chat-field">
          <span className="muted">{t("chat.agent")}</span>
          <select
            value={agent}
            onChange={(e) => {
              if (chatId) newChat();
              setAgent(e.target.value);
            }}
            disabled={streaming}
          >
            {agents.map((a) => (
              <option key={a.name} value={a.name}>
                {a.name}
              </option>
            ))}
          </select>
        </label>
        <label className="chat-field">
          <span className="muted">{t("chat.model")}</span>
          {choices === null ? (
            <select disabled>
              <option>{t("common.loading")}…</option>
            </select>
          ) : choices.length > 0 ? (
            <select value={model} onChange={(e) => switchModel(e.target.value)}>
              {Object.entries(
                choices.reduce<Record<string, ModelChoice[]>>((acc, c) => {
                  const g = c.group ?? "";
                  (acc[g] ??= []).push(c);
                  return acc;
                }, {}),
              ).map(([group, list]) =>
                group ? (
                  <optgroup key={group} label={group}>
                    {list.map((c) => (
                      <option key={c.value} value={c.value}>
                        {c.name}
                      </option>
                    ))}
                  </optgroup>
                ) : (
                  list.map((c) => (
                    <option key={c.value} value={c.value}>
                      {c.name}
                    </option>
                  ))
                ),
              )}
            </select>
          ) : configModels.length > 0 ? (
            <select value={model} onChange={(e) => switchModel(e.target.value)}>
              {configModels.map((m) => (
                <option key={m} value={m}>
                  {m}
                </option>
              ))}
            </select>
          ) : (
            <input
              className="mono sm"
              value={model}
              onChange={(e) => setModel(e.target.value)}
              onBlur={() => chatId && switchModel(model)}
              placeholder={t("chat.modelPh")}
            />
          )}
        </label>
      </div>

      <div className="chat-log card">
        {messages.length === 0 ? (
          <p className="muted pad">{t("chat.empty")}</p>
        ) : (
          messages.map((m, i) => (
            <div key={i} className={m.role === "user" ? "chat-msg user" : "chat-msg agent"}>
              {m.role === "user" ? m.text : <Markdown>{m.text}</Markdown>}
            </div>
          ))
        )}
        {streaming && <div className="chat-typing">…</div>}
        <div ref={bottomRef} />
      </div>

      <div className="chat-input-bar">
        <textarea
          rows={2}
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              send();
            }
          }}
          placeholder={t("chat.inputPh")}
        />
        <button className="primary" disabled={streaming || starting || !input.trim()} onClick={send}>
          {starting ? t("common.loading") : t("chat.send")}
        </button>
      </div>
    </div>
  );
}
