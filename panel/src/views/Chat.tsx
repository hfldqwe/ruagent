// Chat: terminal-like conversations — pick agent, pick model, talk
// multi-turn on one persistent session.

import { useEffect, useRef, useState } from "react";
import { Button, Select } from "antd";
import {
  api,
  type AgentInfo,
  type OptionChoice,
  type SessionOptionInfo,
} from "../api";
import { Icon, type IconName } from "../icons";
import { useI18n } from "../i18n";
import { Markdown, Spinner } from "../ui";

/** Known option ids get translated labels; others show the agent's name. */
function optionLabel(opt: SessionOptionInfo, t: (k: string) => string): string {
  const key = `chat.opt.${opt.id}`;
  const viaId = t(key);
  if (viaId !== key) return viaId;
  if (opt.category === "model") return t("chat.model");
  if (opt.category === "mode") return t("chat.opt.mode");
  if (opt.category === "thought_level") return t("chat.opt.thought");
  return opt.name;
}

type PickerOption =
  | { value: string; label: string }
  | { label: string; options: { value: string; label: string }[] };

/** Group choices by the agent's grouping into Select option shape. */
function buildOptions(choices: OptionChoice[]): PickerOption[] {
  const groups = choices.reduce<Record<string, OptionChoice[]>>((acc, c) => {
    const g = c.group ?? "";
    (acc[g] ??= []).push(c);
    return acc;
  }, {});
  return Object.entries(groups).flatMap(([group, list]): PickerOption[] =>
    group
      ? [{ label: group, options: list.map((c) => ({ value: c.value, label: c.name })) }]
      : list.map((c) => ({ value: c.value, label: c.name })),
  );
}

/** One select over a set of agent-advertised choices (grouped when the
 * agent groups them); config fallback / free text when it doesn't
 * advertise any. */
function OptionPicker({
  label,
  loading,
  choices,
  current,
  onPick,
  fallback,
  onFallbackPick,
  freeText,
  freeTextPh,
  onFreeText,
  disabled,
}: {
  label: string;
  loading: boolean;
  choices: OptionChoice[];
  current: string;
  onPick: (value: string) => void;
  fallback?: string[];
  onFallbackPick?: (value: string) => void;
  freeText?: boolean;
  freeTextPh?: string;
  onFreeText?: (value: string) => void;
  disabled?: boolean;
}) {
  if (loading) {
    return (
      <label className="chat-field">
        <span>{label}</span>
        <Select loading disabled style={{ minWidth: 190 }} />
      </label>
    );
  }
  if (choices.length > 0) {
    return (
      <label className="chat-field">
        <span>{label}</span>
        <Select
          value={current || choices[0]?.value}
          onChange={onPick}
          disabled={disabled}
          style={{ minWidth: 190 }}
          options={buildOptions(choices)}
        />
      </label>
    );
  }
  if (fallback && fallback.length > 0 && onFallbackPick) {
    return (
      <label className="chat-field">
        <span>{label}</span>
        <Select
          value={current}
          onChange={onFallbackPick}
          disabled={disabled}
          style={{ minWidth: 190 }}
          options={fallback.map((m) => ({ value: m, label: m }))}
        />
      </label>
    );
  }
  if (freeText && onFreeText) {
    return (
      <label className="chat-field">
        <span>{label}</span>
        <Select
          mode="tags"
          maxCount={1}
          value={current ? [current] : []}
          onChange={(v) => onFreeText(v[v.length - 1] ?? "")}
          disabled={disabled}
          style={{ minWidth: 190 }}
          placeholder={freeTextPh}
        />
      </label>
    );
  }
  return null;
}

interface Message {
  role: "user" | "assistant";
  text: string;
  /** assistant streaming accumulation */
  done: boolean;
  /** system notice (model switched, permissions) — quiet, centered */
  notice?: boolean;
  /** icon shown on notices */
  icon?: IconName;
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
  /** Live session options advertised by the agent (model, reasoning
   * effort, permission mode, …; ACP session config). Null = probing. */
  const [options, setOptions] = useState<SessionOptionInfo[] | null>(null);
  /** Fallback model list from config (agents.toml `models`). */
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

  // Live session options for the selected agent: what the agent itself
  // advertises over ACP (may probe-spawn it once, then cached server-side).
  // The model option falls back to the config list, then to free text.
  useEffect(() => {
    if (!agent) return;
    const a = agents?.find((x) => x.name === agent);
    setConfigModels(a?.models ?? []);
    setOptions(null);
    let alive = true;
    api
      .agentOptions(agent)
      .then((r) => {
        if (!alive) return;
        setOptions(r.options);
        const m = r.options.find(
          (o) => o.category === "model" || o.id === "model",
        );
        if (m?.current) setModel(m.current);
      })
      .catch(() => {
        if (alive) setOptions([]);
      });
    return () => {
      alive = false;
    };
  }, [agent, agents]);

  /** Apply the refreshed option list (after a live set). */
  const applyOptions = (list: SessionOptionInfo[]) => {
    setOptions(list);
    const m = list.find((o) => o.category === "model" || o.id === "model");
    if (m?.current) setModel(m.current);
  };

  /** Non-model options (reasoning effort, permission mode, …) set live. */
  const setOption = async (opt: SessionOptionInfo, value: string) => {
    if (!chatId) return;
    try {
      const r = await api.chatSetOption(chatId, opt.id, value);
      applyOptions(r.options);
      setMessages((prev) => [
        ...prev,
        {
          role: "assistant",
          text: `${optionLabel(opt, t)} → \`${value}\``,
          done: true,
          notice: true,
          icon: "settings",
        },
      ]);
    } catch (e) {
      setMessages((prev) => [
        ...prev,
        { role: "assistant", text: String(e), done: true, notice: true, icon: "warn" },
      ]);
    }
  };

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
      case "context_injected": {
        const render = String(ev.render ?? "");
        setMessages((prev) => [
          ...prev,
          {
            role: "assistant",
            text: render.length > 160 ? `${render.slice(0, 160)}…` : render,
            done: true,
            notice: true,
            icon: "brain",
          },
        ]);
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
          { role: "assistant", text: msg, done: true, notice: true, icon: "warn" },
        ]);
        setStreaming(false);
        break;
      }
      case "permission_requested": {
        setMessages((prev) => [
          ...prev,
          {
            role: "assistant",
            text: `${t("timeline.permReq", { title: String(ev.title) })} → ${t("nav.inbox")}`,
            done: true,
            notice: true,
            icon: "lock",
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
        { role: "assistant", text: String(e), done: true, notice: true, icon: "warn" },
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

  const modelChoices =
    options?.find((o) => o.category === "model" || o.id === "model")?.choices ??
    [];

  const switchRuntime = async (r: string) => {
    if (!chatId) return;
    // Runtime switch restarts the engine; the role prompt travels.
    if (streamRef.current) streamRef.current();
    try {
      const chat = await api.chatModel(chatId, null, r);
      setChatId(chat.id);
      setMessages((prev) => [
        ...prev,
        {
          role: "assistant",
          text: `${t("chat.runtimeSwitched")} → ${r}`,
          done: true,
          notice: true,
        },
      ]);
      attachStream(chat.id);
    } catch (e) {
      setMessages((prev) => [
        ...prev,
        { role: "assistant", text: String(e), done: true, notice: true },
      ]);
    }
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
            text: `${t("chat.modelLive")} → \`${chat.model ?? m}\``,
            done: true,
            notice: true,
            icon: "settings",
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
            text: `${t("chat.modelSwitched")} → \`${chat.model ?? m}\``,
            done: true,
            notice: true,
            icon: "settings",
          },
        ]);
        attachStream(chat.id);
      }
    } catch (e) {
      setMessages((prev) => [
        ...prev,
        { role: "assistant", text: String(e), done: true, notice: true, icon: "warn" },
      ]);
    }
  };

  if (!agents) return <Spinner label={`${t("chat.title")}…`} />;

  return (
    <div className="chat-wrap">
      <div className="view-bar">
        <h2>{t("chat.title")}</h2>
        <span className="muted">{t("chat.subtitle")}</span>
        <span className="grow" />
        {chatId ? (
          <Button size="small" onClick={newChat}>
            + {t("chat.new")}
          </Button>
        ) : null}
      </div>

      <div className="chat-log grow">
        {messages.length === 0 ? (
          <div className="state empty">
            <span className="empty-icon">
              <Icon name="chat" size={30} />
            </span>
            <p>{t("chat.empty")}</p>
          </div>
        ) : (
          messages.map((m, i) => (
            <div
              key={i}
              className={
                m.notice
                  ? "chat-msg notice"
                  : m.role === "user"
                    ? "chat-msg user"
                    : "chat-msg agent"
              }
            >
              {m.notice && m.icon ? <Icon name={m.icon} size={12} /> : null}
              {m.role === "user" || m.notice ? m.text : <Markdown>{m.text}</Markdown>}
            </div>
          ))
        )}
        {streaming && (
          <div className="chat-typing">
            <i /> <i /> <i />
          </div>
        )}
        <div ref={bottomRef} />
      </div>

      <div className="chat-bottom">
      <div className="chat-bar">
        <label className="chat-field">
          <span>{t("chat.agent")}</span>
          <Select
            value={agent || undefined}
            onChange={(v) => {
              if (chatId) newChat();
              setAgent(v);
            }}
            disabled={streaming}
            style={{ minWidth: 150 }}
            options={agents.map((a) => ({ value: a.name, label: a.name }))}
          />
        </label>
        <OptionPicker
          label={t("chat.model")}
          loading={options === null}
          choices={modelChoices}
          current={model}
          onPick={switchModel}
          fallback={configModels}
          onFallbackPick={switchModel}
          freeText
          freeTextPh={t("chat.modelPh")}
          onFreeText={(v) => setModel(v)}
          disabled={streaming}
        />
        {(() => {
          const a = agents?.find((x) => x.name === agent);
          const rt = a?.runtimes ?? [];
          if (rt.length <= 1) return null;
          return (
            <OptionPicker
              label={t("chat.runtime")}
              loading={false}
              choices={rt.map((r) => ({ value: r, name: r }))}
              current={a?.runtime ?? rt[0]}
              onPick={switchRuntime}
              disabled={streaming}
            />
          );
        })()}
        {(options ?? [])
          .filter((o) => o.category !== "model" && o.id !== "model")
          .map((opt) => (
            <OptionPicker
              key={opt.id}
              label={optionLabel(opt, t)}
              loading={false}
              choices={opt.choices}
              current={opt.current ?? ""}
              onPick={(v) => setOption(opt, v)}
              disabled={streaming}
            />
          ))}
      </div>
      <div className="chat-input-bar">
        <div className="composer">
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
          <Button
            type="primary"
            className="send-btn"
            disabled={streaming || starting || !input.trim()}
            onClick={send}
            title={t("chat.send")}
          >
            {starting ? "…" : <Icon name="arrowUp" size={16} />}
          </Button>
        </div>
      </div>
      </div>
    </div>
  );
}
