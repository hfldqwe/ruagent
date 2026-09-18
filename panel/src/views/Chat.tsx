// Chat: terminal-like conversations — pick agent, pick model, talk
// multi-turn on one persistent session. Models / permission modes /
// thinking levels come from the daemon's cached catalog (instant, with
// a manual sync); the model defaults to the agent's configured one so
// nobody is asked to choose every time. The history drawer reopens
// past conversations (live ones reattach and stream).

import { useEffect, useRef, useState } from "react";
import { Button, Drawer, Select, Tooltip } from "antd";
import {
  api,
  type AgentInfo,
  type ChatHistoryEntry,
  type OptionChoice,
  type SessionOptionInfo,
} from "../api";
import { Icon, type IconName } from "../icons";
import { useI18n } from "../i18n";
import { Markdown, RelTime, Spinner } from "../ui";
import { msToIso, SessionDetail } from "./Sessions";

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

/** Canonical picker order: model → permission mode → thinking → extras. */
function categoryRank(o: SessionOptionInfo): number {
  if (o.category === "model" || o.id === "model") return 0;
  if (o.category === "mode" || o.id === "mode") return 1;
  if (o.category === "thought_level") return 2;
  return 3;
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
  /** The engine the chat currently runs on (roles can switch). */
  const [runtime, setRuntime] = useState("");
  /** Live session options advertised by the agent (model, reasoning
   * effort, permission mode, …; ACP session config). Null = loading. */
  const [options, setOptions] = useState<SessionOptionInfo[] | null>(null);
  /** When the daemon's cached catalog was last refreshed. */
  const [optionsAt, setOptionsAt] = useState<number | null>(null);
  const [syncing, setSyncing] = useState(false);
  /** Fallback model list from config (agents.toml `models`). */
  const [configModels, setConfigModels] = useState<string[]>([]);
  const [chatId, setChatId] = useState<string | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState("");
  const [streaming, setStreaming] = useState(false);
  const [starting, setStarting] = useState(false);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [history, setHistory] = useState<ChatHistoryEntry[] | null>(null);
  const [viewing, setViewing] = useState<ChatHistoryEntry | null>(null);
  const bottomRef = useRef<HTMLDivElement>(null);
  const streamRef = useRef<(() => void) | null>(null);
  /** Which chat id the SSE is attached to — reattaching replays the
   * whole transcript, so the same chat must only attach once (the
   * duplicate-append bug on multi-turn conversations). */
  const attachedRef = useRef<string | null>(null);

  useEffect(() => {
    api.agents().then((a) => {
      const enabled = a.filter((x) => x.enabled);
      setAgents(enabled);
      setAgent((cur) =>
        cur && enabled.some((a) => a.name === cur) ? cur : (enabled[0]?.name ?? ""),
      );
    }).catch(() => setAgents([]));
  }, []);

  // Session options for the selected agent: the daemon's cached catalog
  // (instant after restart), read for the CURRENT engine — model
  // catalogs are engine-specific, so a runtime switch refetches and
  // resets the pick (issue #37). The model defaults to the card's
  // configured model, then the agent's advertised current.
  const lastAgentRef = useRef("");
  const lastEngineRef = useRef("");
  useEffect(() => {
    if (!agent) return;
    const a = agents?.find((x) => x.name === agent);
    const engine = runtime || a?.runtime || "";
    const agentChanged = lastAgentRef.current !== agent;
    const engineChanged = lastEngineRef.current !== engine;
    lastAgentRef.current = agent;
    lastEngineRef.current = engine;
    setConfigModels(a?.models ?? []);
    setOptions(null);
    setOptionsAt(null);
    if (agentChanged) {
      setModel(a?.model ?? "");
      setRuntime(a?.runtime ?? "");
    } else if (engineChanged) {
      // Same role, other engine: the old model id is not portable.
      setModel("");
    }
    let alive = true;
    api
      .agentOptions(agent, false, engine || undefined)
      .then((r) => {
        if (!alive) return;
        setOptions(r.options);
        setOptionsAt(r.updated_at ?? null);
        const m = r.options.find(
          (o) => o.category === "model" || o.id === "model",
        );
        // Configured default wins; otherwise the advertised current.
        setModel((cur) => cur || m?.current || "");
      })
      .catch(() => {
        if (alive) setOptions([]);
      });
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [agent, agents, runtime]);

  /** Manual catalog sync (the sync button) — for the current engine. */
  const syncOptions = async () => {
    if (!agent || syncing) return;
    setSyncing(true);
    try {
      const r = await api.agentOptions(agent, true, runtime || undefined);
      setOptions(r.options);
      setOptionsAt(r.updated_at ?? null);
    } catch {
      /* the pickers keep the cached list */
    } finally {
      setSyncing(false);
    }
  };

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
      setRuntime(chat.runtime ?? "");
      return chat.id;
    } finally {
      setStarting(false);
    }
  };

  const attachStream = (id: string, force = false) => {
    if (!force && attachedRef.current === id && streamRef.current) return;
    attachedRef.current = id;
    if (streamRef.current) streamRef.current();
    const es = new EventSource(`/api/v1/chat/${id}/events`);
    const close = () => es.close();
    streamRef.current = close;
    es.onmessage = (e) => {
      try {
        const parsed = JSON.parse(e.data) as { event?: ChatEvent } & ChatEvent;
        // Transcript replay lines wrap the event ({ts, seq, event});
        // live events arrive flat. Handle both.
        handleEvent(parsed.event ?? parsed);
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
      case "user_message": {
        // Transcript replay (history reattach) re-adds user turns; the
        // tail dedupe keeps the optimistic copy from doubling live.
        const text = String(ev.text ?? "");
        if (!text) return;
        setMessages((prev) => {
          const last = prev[prev.length - 1];
          if (last && last.role === "user" && last.text === text) return prev;
          return [...prev, { role: "user", text, done: true }];
        });
        break;
      }
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
    attachedRef.current = null;
    setChatId(null);
    setMessages([]);
    setStreaming(false);
  };

  const modelChoices =
    options?.find((o) => o.category === "model" || o.id === "model")?.choices ??
    [];

  /** Non-model pickers in canonical order: permission mode → thinking. */
  const extraPickers = (options ?? [])
    .filter((o) => o.category !== "model" && o.id !== "model")
    .sort((a, b) => categoryRank(a) - categoryRank(b));

  const switchRuntime = async (r: string) => {
    if (!chatId) return;
    // Runtime switch restarts the engine; the role prompt travels.
    if (streamRef.current) streamRef.current();
    try {
      const chat = await api.chatModel(chatId, null, r);
      setChatId(chat.id);
      setRuntime(chat.runtime ?? r);
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
        setRuntime(chat.runtime ?? runtime);
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

  // ------------------------------------------------------------------
  // History: past conversations with the current agent. Live ones
  // reattach (the SSE replays the transcript, then streams); closed
  // ones open the shared read-only viewer.
  // ------------------------------------------------------------------
  const openHistory = () => {
    setHistoryOpen(true);
    setHistory(null);
    api.chatsHistory(agent).then(setHistory).catch(() => setHistory([]));
  };

  const openPast = (h: ChatHistoryEntry) => {
    if (h.active) {
      setHistoryOpen(false);
      if (streamRef.current) streamRef.current();
      setAgent(h.agent);
      setChatId(h.id);
      setModel(h.model ?? "");
      setRuntime(h.runtime ?? "");
      setMessages([]);
      setStreaming(false);
      attachStream(h.id, true);
    } else {
      setViewing(h);
    }
  };

  if (!agents) return <Spinner label={`${t("chat.title")}…`} />;

  const currentAgent = agents.find((a) => a.name === agent);

  return (
    <div className="chat-wrap">
      <div className="view-bar">
        <h2>{t("chat.title")}</h2>
        <span className="muted">{t("chat.subtitle")}</span>
        <span className="grow" />
        {optionsAt ? (
          <Tooltip title={`${t("chat.syncedAt")} ${relTimeText(optionsAt)}`}>
            <Button size="small" loading={syncing} onClick={syncOptions} title={t("chat.sync")}>
              <Icon name="sync" size={13} />
            </Button>
          </Tooltip>
        ) : null}
        <Button size="small" onClick={openHistory}>
          <Icon name="history" size={13} /> {t("chat.history")}
        </Button>
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
          const rt = currentAgent?.runtimes ?? [];
          if (rt.length <= 1) return null;
          return (
            <OptionPicker
              label={t("chat.runtime")}
              loading={false}
              choices={rt.map((r) => ({ value: r, name: r }))}
              current={runtime || currentAgent?.runtime || rt[0]}
              onPick={switchRuntime}
              disabled={streaming}
            />
          );
        })()}
        {extraPickers.map((opt) => (
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

      <Drawer
        title={`${t("chat.history")} · ${agent}`}
        placement="right"
        width={420}
        open={historyOpen}
        onClose={() => setHistoryOpen(false)}
      >
        {history === null ? (
          <Spinner />
        ) : history.length === 0 ? (
          <p className="muted">{t("chat.historyEmpty")}</p>
        ) : (
          <div className="card">
            {history.map((h) => (
              <button key={h.id} className="row-btn" onClick={() => openPast(h)}>
                <span className="dot" style={{ width: 6, height: 6,
                  background: h.active ? "var(--ant-color-success)" : "var(--ant-color-text-quaternary)" }} />
                <span className="title">
                  <strong>{h.title || h.preview || t("sessions.untitled")}</strong>
                  {h.runtime ? (
                    <span className="muted" style={{ marginLeft: 8, fontSize: 12 }}>
                      {h.runtime}
                    </span>
                  ) : null}
                </span>
                {h.message_count != null ? (
                  <span className="muted" style={{ fontSize: 11 }}>
                    {h.message_count} {t("sessions.messages")}
                  </span>
                ) : null}
                <span className="time">
                  <RelTime iso={msToIso(h.updated_at)} />
                </span>
              </button>
            ))}
          </div>
        )}
      </Drawer>

      {viewing && (
        <SessionDetail
          session={{
            key: viewing.session_key ?? "",
            source: "ruagent",
            title: viewing.title,
            project: null,
            ref_path: "",
            started_at: viewing.created_at,
            updated_at: viewing.updated_at,
            message_count: viewing.message_count ?? 0,
            preview: viewing.preview,
            agent: viewing.agent,
          }}
          onClose={() => setViewing(null)}
        />
      )}
    </div>
  );
}

/** Compact relative time for tooltips ("3 小时前" style, no dependency
 * on the i18n plumbing — Intl handles the locale). */
function relTimeText(epochMs: number): string {
  const diff = Date.now() - epochMs;
  const mins = Math.round(diff / 60000);
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins}m`;
  const hours = Math.round(mins / 60);
  if (hours < 24) return `${hours}h`;
  return `${Math.round(hours / 24)}d`;
}
