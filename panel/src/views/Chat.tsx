// Chat: terminal-like conversations — pick agent, pick model, talk
// multi-turn on one persistent session. Models / permission modes /
// thinking levels come from the daemon's cached catalog (instant, with
// a manual sync); the model defaults to the agent's configured one so
// nobody is asked to choose every time. The history drawer reopens
// past conversations (live ones reattach and stream).

import { Fragment, useEffect, useRef, useState } from "react";
import { Button, Input, Select, Tooltip } from "antd";
import {
  api,
  isRoleAgent,
  type AgentInfo,
  type ChatHistoryEntry,
  type OptionChoice,
  type SessionOptionInfo,
} from "../api";
import { BrandMark } from "../brand";
import { Icon, type IconName } from "../icons";
import { useI18n } from "../i18n";
import { Markdown, RelTime, Spinner, useToast } from "../ui";
import { msToIso } from "./Sessions";

/** Deterministic per-workspace hue from a curated set — the workspace
 * tree reads at a glance (each project keeps its color across visits). */
const WS_HUES = [
  "#7c86f0", "#e0916a", "#56b4a4", "#d97f8e", "#78b874",
  "#a887e8", "#5fa8dc", "#d8ac5c", "#9aa4b8", "#c8877a",
];
const wsColor = (ws: string) =>
  WS_HUES[
    [...ws].reduce((a, c) => (a * 31 + c.charCodeAt(0)) >>> 0, 7) % WS_HUES.length
  ];

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

/** One compact select over a set of agent-advertised choices (grouped
 * when the agent groups them); config fallback / free text when it
 * doesn't advertise any. These are the composer's borderless mini
 * controls, not form fields — the label carries meaning via aria. */
function OptionPicker({
  field,
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
  field: string;
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
  const common = {
    className: `ctl-select ctl-${field}`,
    "aria-label": label,
    size: "small" as const,
    variant: "borderless" as const,
    popupMatchSelectWidth: false,
    disabled,
  };
  if (loading) {
    return <Select {...common} loading style={{ minWidth: 56 }} />;
  }
  if (choices.length > 0) {
    return (
      <Select
        {...common}
        value={current || choices[0]?.value}
        onChange={onPick}
        style={{ minWidth: 56 }}
        options={buildOptions(choices)}
      />
    );
  }
  if (fallback && fallback.length > 0 && onFallbackPick) {
    return (
      <Select
        {...common}
        value={current}
        onChange={onFallbackPick}
        style={{ minWidth: 56 }}
        options={fallback.map((m) => ({ value: m, label: m }))}
      />
    );
  }
  if (freeText && onFreeText) {
    return (
      <Select
        {...common}
        mode="tags"
        maxCount={1}
        value={current ? [current] : []}
        onChange={(v) => onFreeText(v[v.length - 1] ?? "")}
        style={{ minWidth: 56 }}
        placeholder={freeTextPh}
      />
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
  /** collapsible block: platform context injection (inspectable, not
   * hidden) or the agent's thought stream (open while streaming,
   * collapses when the turn ends, reopenable). */
  kind?: "injection" | "thought";
}

interface ChatEvent {
  type: string;
  [k: string]: unknown;
}

export function Chat({ initialAgent }: { initialAgent?: string }) {
  const { t } = useI18n();
  const toast = useToast();
  const [agents, setAgents] = useState<AgentInfo[] | null>(null);
  const [agent, setAgent] = useState(initialAgent ?? "");
  const [model, setModel] = useState("");
  /** The engine the chat currently runs on (roles can switch). */
  const [runtime, setRuntime] = useState("");
  /** Live session options advertised by the agent (model, reasoning
   * effort, permission mode, …; ACP session config). Null = loading. */
  const [options, setOptions] = useState<SessionOptionInfo[] | null>(null);
  /** Fallback model list from config (agents.toml `models`). */
  const [configModels, setConfigModels] = useState<string[]>([]);
  const [chatId, setChatId] = useState<string | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState("");
  const [streaming, setStreaming] = useState(false);
  const [starting, setStarting] = useState(false);
  const [sideOpen, setSideOpen] = useState(false);
  /** History rail collapsed (desktop) — persisted so it survives
   * reloads; the conversation owns the width it frees. */
  const [railCollapsed, setRailCollapsed] = useState(
    () => localStorage.getItem("chat.railCollapsed") === "1",
  );
  useEffect(() => {
    localStorage.setItem("chat.railCollapsed", railCollapsed ? "1" : "0");
  }, [railCollapsed]);
  const [history, setHistory] = useState<ChatHistoryEntry[] | null>(null);
  /** Working directory for new chats — which project the agent works
   * in (persisted; the daemon spawns the session there). */
  const [project, setProject] = useState(
    () => localStorage.getItem("chat.cwd") ?? "",
  );
  const [sessionQuery, setSessionQuery] = useState("");
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());
  /** Explicitly added workspaces (the folder picker) — persisted; the
   * rendered groups are these UNION the cwd of recorded chats. */
  const [workspaces, setWorkspaces] = useState<string[]>(() => {
    try {
      return JSON.parse(localStorage.getItem("chat.workspaces") ?? "[]") as string[];
    } catch {
      return [];
    }
  });
  useEffect(() => {
    localStorage.setItem("chat.workspaces", JSON.stringify(workspaces));
  }, [workspaces]);
  useEffect(() => {
    localStorage.setItem("chat.cwd", project);
  }, [project]);
  const [viewing, setViewing] = useState<ChatHistoryEntry | null>(null);
  const bottomRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const streamRef = useRef<(() => void) | null>(null);
  /** Which chat id the SSE is attached to — reattaching replays the
   * whole transcript, so the same chat must only attach once (the
   * duplicate-append bug on multi-turn conversations). */
  const attachedRef = useRef<string | null>(null);
  /** SSE reconnection: a drop mid-reply reattaches with backoff — the
   * transcript replay rebuilds state including any Stopped we missed —
   * capped at five tries. A chat killed by a daemon restart 404s, which
   * burns the tries and ends in an honest "connection lost" notice
   * instead of a half-finished reply silently marked done. */
  const streamingRef = useRef(false);
  const reconnectRef = useRef<{ tries: number; timer: number | null }>({
    tries: 0,
    timer: null,
  });

  useEffect(() => {
    streamingRef.current = streaming;
  }, [streaming]);

  // Never leak a pending reconnect timer past the component.
  useEffect(
    () => () => {
      const st = reconnectRef.current;
      if (st.timer !== null) clearTimeout(st.timer);
    },
    [],
  );

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
      const chat = await api.chatStart(
        agent,
        model.trim() || null,
        project.trim() || undefined,
      );
      setChatId(chat.id);
      setModel(chat.model ?? "");
      setRuntime(chat.runtime ?? "");
      return chat.id;
    } finally {
      setStarting(false);
    }
  };

  const scheduleReconnect = (id: string) => {
    const st = reconnectRef.current;
    if (st.timer !== null) return;
    if (st.tries >= 5) {
      setStreaming(false);
      setMessages((prev) => [
        ...prev,
        {
          role: "assistant",
          text: t("chat.connectionLost"),
          done: true,
          notice: true,
          icon: "warn",
        },
      ]);
      return;
    }
    const delay = 1000 * 2 ** st.tries; // 1s → 16s
    st.tries += 1;
    st.timer = window.setTimeout(() => {
      st.timer = null;
      setMessages([]); // the replay rebuilds the transcript
      attachStream(id, true);
    }, delay);
  };

  const attachStream = (id: string, force = false) => {
    if (!force && attachedRef.current === id && streamRef.current) return;
    attachedRef.current = id;
    if (streamRef.current) streamRef.current();
    const es = new EventSource(`/api/v1/chat/${id}/events`);
    const close = () => es.close();
    streamRef.current = close;
    es.onmessage = (e) => {
      reconnectRef.current.tries = 0; // liveness
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
      const st = reconnectRef.current;
      if (st.timer !== null) {
        clearTimeout(st.timer);
        st.timer = null;
      }
      st.tries = 0;
      es.close();
      setStreaming(false);
    });
    es.onerror = () => {
      es.close();
      // The server closes the stream after each prompt's `end` — an
      // error with no reply in flight is that close racing us (or a
      // dead daemon nobody is talking to). Only a drop mid-reply is
      // worth reconnecting.
      if (!streamingRef.current) return;
      scheduleReconnect(id);
    };
  };

  const handleEvent = (ev: ChatEvent) => {
    switch (ev.type) {
      case "user_message": {
        // Transcript replay (history reattach) re-adds user turns; the
        // tail dedupe keeps the optimistic copy from doubling live.
        // Injection/notice rows ride BETWEEN the two — scan past them,
        // or every first prompt doubles (the opencode "你好" bug: the
        // optimistic copy, the injection chip, then the live echo).
        const text = String(ev.text ?? "");
        if (!text) return;
        setMessages((prev) => {
          for (let i = prev.length - 1; i >= 0; i--) {
            const m = prev[i];
            if (m.notice || m.kind) continue;
            if (m.role === "user" && m.text === text) return prev;
            break;
          }
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
          if (
            last &&
            last.role === "assistant" &&
            !last.done &&
            !last.kind
          ) {
            next[next.length - 1] = { ...last, text: last.text + text };
          } else {
            next.push({ role: "assistant", text, done: false });
          }
          return next;
        });
        break;
      }
      case "agent_thought_chunk": {
        const content = ev.content as { text?: string }[];
        const text = content.map((c) => c.text ?? "").join("");
        if (!text) return;
        // Accumulates like a reply, but renders as an open-while-
        // streaming collapsible: the tail stays visible live (the page
        // auto-scrolls on every message change), the full history is
        // one click away after the turn ends.
        setMessages((prev) => {
          const next = [...prev];
          const last = next[next.length - 1];
          if (
            last &&
            last.role === "assistant" &&
            last.kind === "thought" &&
            !last.done
          ) {
            next[next.length - 1] = { ...last, text: last.text + text };
          } else {
            next.push({
              role: "assistant",
              text,
              done: false,
              kind: "thought",
            });
          }
          return next;
        });
        break;
      }
      case "context_injected": {
        // Show WHAT was injected, not that something was: the full
        // render behind a collapsed chip, so recall relevance is
        // checkable by the human instead of hidden.
        const render = String(ev.render ?? "");
        if (!render) return;
        setMessages((prev) => [
          ...prev,
          { role: "assistant", text: render, done: true, kind: "injection" },
        ]);
        break;
      }
      case "stopped": {
        setMessages((prev) => {
          // Close every open thought stream first — they may sit
          // anywhere in the turn (before/between/after the reply).
          const next = prev.map((m) =>
            m.kind === "thought" && !m.done ? { ...m, done: true } : m,
          );
          // Then: did anything actually reply to the last ask?
          let sawReply = false;
          for (let i = next.length - 1; i >= 0; i--) {
            const m = next[i];
            if (m.kind) continue; // chips: not replies
            if (m.notice) {
              if (m.text === t("chat.emptyReply")) sawReply = true; // replay guard
              continue;
            }
            if (m.role === "assistant") sawReply = true;
            break;
          }
          if (!sawReply) {
            // The agent ended its turn silently — say so instead of
            // looking broken (opencode does this on bare greetings).
            next.push({
              role: "assistant",
              text: t("chat.emptyReply"),
              done: true,
              notice: true,
              icon: "warn",
            });
          } else {
            const last = next[next.length - 1];
            if (last && last.role === "assistant") {
              next[next.length - 1] = { ...last, done: true };
            }
          }
          return next;
        });
        setStreaming(false);
        // The reply bumped this chat up the history rail — resync it.
        refreshHistory();
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
    setViewing(null);
    setChatId(null);
    setMessages([]);
    setStreaming(false);
  };

  /** Switch agents mid-conversation: the session restarts on the
   * target with the prior turns handed over as context — the
   * conversation continues across engines (claude-code ↔ dsh ↔ …).
   * Without a live chat it is just a different default pick. */
  const switchAgent = async (name: string) => {
    if (chatId && !streaming) {
      try {
        const r = await api.chatHandoff(chatId, name);
        if (streamRef.current) streamRef.current();
        setAgent(name);
        setChatId(r.id);
        setModel(r.model ?? "");
        setRuntime(r.runtime ?? "");
        attachStream(r.id, true);
        setMessages((prev) => [
          ...prev,
          {
            role: "assistant",
            text: t("chat.handoffNotice", { name }),
            done: true,
            notice: true,
            icon: "settings",
          },
        ]);
        refreshHistory();
        return;
      } catch (e) {
        toast("err", String(e));
        return;
      }
    }
    setAgent(name);
  };

  /** New session bound to a specific workspace (the group-head +). */
  const newSessionIn = (ws: string) => {
    setProject(ws);
    newChat();
    setSideOpen(false);
  };

  /** The folder button: system directory picker via the daemon (the
   * browser cannot read absolute local paths — a local process can),
   * the picked directory becomes a workspace and the active project. */
  const pickWorkspace = async () => {
    try {
      const r = await api.pickDirectory();
      if (r.path) {
        setWorkspaces((w) => (w.includes(r.path!) ? w : [...w, r.path!]));
        setProject(r.path);
        toast("ok", t("chat.workspaceAdded", { name: r.path.replace(/.*[/]/, "") }));
      }
    } catch (e) {
      toast("err", String(e));
    }
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
  // History: past conversations with the current agent, always visible
  // in the left rail. Live ones reattach (the SSE replays the
  // transcript, then streams); closed ones open the shared read-only
  // viewer. Refreshed when the agent changes and after each reply.
  // ------------------------------------------------------------------
  const refreshHistory = () => {
    if (!agent) return;
    api.chatsHistory(agent).then(setHistory).catch(() => setHistory([]));
  };
  useEffect(() => {
    refreshHistory();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [agent]);

  /** Session switching: clicking a past conversation RESUMES it — the
   * session restarts under the same id (transcript appends, one
   * history thread) with the prior turns handed over. Only when the
   * agent no longer exists does it fall back to the read-only view. */
  const openPast = async (h: ChatHistoryEntry) => {
    setSideOpen(false);
    if (h.active) {
      setViewing(null);
      if (streamRef.current) streamRef.current();
      setAgent(h.agent);
      setChatId(h.id);
      setModel(h.model ?? "");
      setRuntime(h.runtime ?? "");
      setMessages([]);
      setStreaming(false);
      attachStream(h.id, true);
      return;
    }
    try {
      const r = await api.chatResume(h.id);
      setViewing(null);
      if (streamRef.current) streamRef.current();
      setAgent(r.agent);
      setChatId(r.id);
      setModel(r.model ?? "");
      setRuntime(r.runtime ?? "");
      setMessages([]);
      setStreaming(false);
      attachStream(r.id, true); // replay rebuilds the whole thread
      refreshHistory();
    } catch {
      setViewing(h); // agent deleted / daemon refuses: read-only
    }
  };

  /** Sessions grouped by project (the workspace tree, dsh-web style):
   * the current project's group first, then others, then ungrouped. */
  const historyGroups: [string, ChatHistoryEntry[]][] = (() => {
    const q = sessionQuery.trim().toLowerCase();
    const filtered = (history ?? []).filter((h) =>
      !q ||
      (h.title ?? "").toLowerCase().includes(q) ||
      (h.preview ?? "").toLowerCase().includes(q) ||
      (h.agent ?? "").toLowerCase().includes(q),
    );
    const by = new Map<string, ChatHistoryEntry[]>();
    // Explicit workspaces exist even with no sessions yet (暂无会话).
    for (const ws of workspaces) {
      if (!by.has(ws)) by.set(ws, []);
    }
    for (const h of filtered) {
      const key = h.cwd?.trim() || "";
      const list = by.get(key) ?? [];
      list.push(h);
      by.set(key, list);
    }
    const cur = project.trim().toLowerCase();
    // A search filters visible rows; workspaces without hits keep

    return [...by.entries()].sort((a, b) => {
      const rank = (k: string) =>
        k.toLowerCase() === cur ? 0 : k === "" ? 2 : 1;
      return rank(a[0]) - rank(b[0]) || a[0].localeCompare(b[0]);
    });
  })();

  if (!agents) return <Spinner label={`${t("chat.title")}…`} />;

  const currentAgent = agents.find((a) => a.name === agent);

  /** The composer unit: input row + hairline-divided controls row (agent,
   * model, mode/effort, project). Rendered centered when the conversation
   * is empty, sticky at the bottom once it has messages. */
  const composerBlock = (
    <div className="chat-bottom">
      <div className="composer">
        <div className="composer-row">
          <textarea
            ref={inputRef}
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
          {streaming && chatId ? (
            <Button
              className="send-btn stop-btn"
              onClick={() => {
                api.chatStop(chatId).catch(() => {
                  /* the Stopped event still lands via the stream */
                });
              }}
              title={t("chat.stop")}
            >
              <Icon name="stop" size={14} />
            </Button>
          ) : (
            <Button
              type="primary"
              className="send-btn"
              disabled={streaming || starting || !input.trim()}
              onClick={send}
              title={t("chat.send")}
            >
              {starting ? "…" : <Icon name="arrowUp" size={16} />}
            </Button>
          )}
        </div>
        <div className="composer-controls">
          <Select
            className="ctl-select ctl-agent"
            aria-label={t("chat.agent")}
            variant="borderless"
            size="small"
            popupMatchSelectWidth={false}
            value={agent || undefined}
            onChange={(v) => {
              void switchAgent(v);
            }}
            disabled={streaming}
            options={[
              ...agents
                .filter((a) => isRoleAgent(a))
                .map((a) => ({
                  value: a.name,
                  label: (
                    <span className="sel-opt">
                      <BrandMark harness={a.harness} size={13} mono fallback="bot" />
                      {a.name}
                    </span>
                  ),
                })),
              {
                label: t("chat.runtimeGroup"),
                options: agents
                  .filter((a) => !isRoleAgent(a))
                  .map((a) => ({
                    value: a.name,
                    label: (
                      <span className="sel-opt">
                        <BrandMark harness={a.harness} size={13} mono fallback="bot" />
                        {a.name}
                      </span>
                    ),
                  })),
              },
            ]}
          />
          <span className="ctl-sep" />
          <OptionPicker
            field="model"
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
              <>
                <span className="ctl-sep" />
                <OptionPicker
                  field="runtime"
                  label={t("chat.runtime")}
                  loading={false}
                  choices={rt.map((r) => ({ value: r, name: r }))}
                  current={runtime || currentAgent?.runtime || rt[0]}
                  onPick={switchRuntime}
                  disabled={streaming}
                />
              </>
            );
          })()}
          {extraPickers.map((opt) => (
            <Fragment key={opt.id}>
              <span className="ctl-sep" />
              <OptionPicker
                field={opt.id}
                label={optionLabel(opt, t)}
                loading={false}
                choices={opt.choices}
                current={opt.current ?? ""}
                onPick={(v) => setOption(opt, v)}
                disabled={streaming}
              />
            </Fragment>
          ))}
          <span className="grow" />
          {project.trim() ? (
            <Tooltip title={project}>
              <span className="ws-chip">
                <Icon name="folderOpen" size={12} />
                <span className="ws-chip-name">
                  {project.trim().replace(/.*[\\/]/, "")}
                </span>
              </span>
            </Tooltip>
          ) : null}
        </div>
      </div>
    </div>
  );

  return (
    <div className={`chat-layout${railCollapsed ? " rail-collapsed" : ""}`}>
      <aside className={`chat-side${sideOpen ? " open" : ""}`}>
        <div className="chat-side-head">
          <Button
            type="primary"
            size="small"
            style={{ flex: 1 }}
            onClick={() => {
              newChat();
              setSideOpen(false);
            }}
          >
            <Icon name="squarePen" size={13} /> {t("chat.new")}
          </Button>
          <Tooltip title={t("chat.addWorkspace")}>
            <Button
              size="small"
              type="text"
              className="icon-btn"
              onClick={pickWorkspace}
              aria-label={t("chat.addWorkspace")}
            >
              <Icon name="folderPlus" size={15} />
            </Button>
          </Tooltip>
        </div>
        <Input
          allowClear
          variant="filled"
          value={sessionQuery}
          onChange={(e) => setSessionQuery(e.target.value)}
          placeholder={t("chat.searchSessions")}
          size="small"
          prefix={<Icon name="search" size={12} />}
        />
        <div className="chat-side-list">
          <div className="chat-ws-title">{t("chat.workspaces")}</div>
          {history === null ? (
            <Spinner />
          ) : historyGroups.length === 0 ? (
            <p className="muted">{sessionQuery ? t("chat.noResults") : t("chat.historyEmpty")}</p>
          ) : (
            historyGroups.map(([cwd, items]) => {
              const key = cwd || "";
              const shown = expandedGroups.has(key) ? items : items.slice(0, 5);
              return (
                <div key={key || "none"} className="chat-group">
                  <div className="chat-group-head" title={cwd || undefined}>
                    <Icon
                      name="folderOpen"
                      size={14}
                      className="ws-folder"
                      style={{ color: key ? wsColor(key) : undefined, flex: "none" }}
                    />
                    <span className="chat-group-name">
                      {cwd ? cwd.replace(/.*[\\/]/, "") : t("chat.noProject")}
                    </span>
                    <span className="ws-count">{items.length}</span>
                    <span className="grow" />
                    <Tooltip title={t("chat.newSessionHere")}>
                      <button
                        className="ws-new-btn"
                        aria-label={t("chat.newSessionHere")}
                        onClick={() => newSessionIn(cwd)}
                      >
                        <Icon name="plus" size={13} />
                      </button>
                    </Tooltip>
                  </div>
                  {shown.map((h) => {
                    const harness = agents?.find((a) => a.name === h.runtime)?.harness;
                    return (
                      <button
                        key={h.id}
                        className={`row-btn chat-session${h.id === chatId ? " selected" : ""}`}
                        onClick={() => openPast(h)}
                        title={h.title || h.preview || t("sessions.untitled")}
                      >
                        {h.active ? <span className="live-dot" /> : null}
                        <span className="title">
                          {h.title || h.preview || t("sessions.untitled")}
                        </span>
                        {harness ? (
                          <BrandMark harness={harness} size={12} mono className="muted" />
                        ) : null}
                        <span className="time">
                          <RelTime iso={msToIso(h.updated_at)} />
                        </span>
                      </button>
                    );
                  })}
                  {shown.length === 0 ? (
                    <div className="chat-group-empty muted">{t("chat.noSessions")}</div>
                  ) : null}
                  {items.length > shown.length ? (
                    <button
                      className="chat-group-more"
                      onClick={() =>
                        setExpandedGroups((g) => new Set(g).add(key))
                      }
                    >
                      {t("chat.showAll", { n: items.length })}
                    </button>
                  ) : null}
                </div>
              );
            })
          )}
        </div>
      </aside>
      {sideOpen && <div className="chat-side-backdrop" onClick={() => setSideOpen(false)} />}

      <div
        className={`chat-wrap${!viewing && messages.length === 0 ? " is-empty" : ""}`}
      >
      <div className="view-bar">
        <Button
          size="small"
          type="text"
          className="chat-rail-toggle"
          onClick={() => setRailCollapsed((c) => !c)}
          title={t("chat.toggleRail")}
          aria-label={t("chat.toggleRail")}
        >
          <Icon name={railCollapsed ? "panelLeftOpen" : "panelLeftClose"} size={14} />
        </Button>
        <h2>{t("chat.title")}</h2>
        <span className="grow" />
        <Button
          size="small"
          className="chat-side-toggle"
          onClick={() => setSideOpen(true)}
          title={t("chat.history")}
        >
          <Icon name="history" size={13} />
        </Button>
      </div>

      {viewing ? (
        <PastConversation entry={viewing} onNew={newChat} />
      ) : messages.length === 0 ? (
        <>
          {/* Empty conversation: the composer centers as the invitation —
             who you're talking to, the input, and three starters. */}
          <div className="chat-hero">
            <span className="chat-hero-mark">
              <BrandMark harness={currentAgent?.harness} size={24} fallback="bot" />
            </span>
            <h3>{agent}</h3>
            <p>{currentAgent?.description || t("chat.subtitle")}</p>
          </div>
          {composerBlock}
          <div className="chat-suggest">
            {[t("chat.suggest.1"), t("chat.suggest.2"), t("chat.suggest.3")].map((s) => (
              <button
                key={s}
                className="suggest-chip"
                onClick={() => {
                  setInput(s);
                  inputRef.current?.focus();
                }}
              >
                <Icon name="sparkles" size={12} />
                {s}
              </button>
            ))}
          </div>
        </>
      ) : (
        <>
          <div className="chat-log grow">
          {messages.map((m, i) => {
            if (m.kind === "injection") {
              return (
                <details key={i} className="chat-injection">
                  <summary>
                    <Icon name="brain" size={12} />{" "}
                    {t("chat.injection")} ·{" "}
                    {t("chat.chars", { n: m.text.length })}
                  </summary>
                  <pre>{m.text}</pre>
                </details>
              );
            }
            if (m.kind === "thought") {
              // Open while streaming (the tail stays live — the page
              // auto-scrolls on every message change); collapses when
              // the turn ends, reopenable in full.
              return (
                <details
                  key={i}
                  className="chat-thought"
                  open={!m.done ? true : undefined}
                >
                  <summary>
                    <Icon name="thought" size={12} />{" "}
                    {t("chat.thought")} ·{" "}
                    {t("chat.chars", { n: m.text.length })}
                  </summary>
                  <div className="thought-body">{m.text}</div>
                </details>
              );
            }
            return (
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
            );
          })}
          {streaming && (
            <div className="chat-typing">
              <i /> <i /> <i />
            </div>
          )}
          <div ref={bottomRef} />
          </div>
          {composerBlock}
        </>
      )}

      </div>

    </div>
  );
}


/** A closed conversation, rendered inline in the chat main area —
 * clicking history enters the conversation (ChatGPT-style), it does
 * not pop a modal over the page. Read-only: the agent process is gone;
 * a new conversation is one click away. */
function PastConversation({
  entry,
  onNew,
}: {
  entry: ChatHistoryEntry;
  onNew: () => void;
}) {
  const { t } = useI18n();
  const [messages, setMessages] = useState<
    { role: string; text: string }[] | null
  >(null);

  useEffect(() => {
    if (entry.session_key) {
      api
        .sessionMessages(entry.session_key)
        .then(setMessages)
        .catch(() => setMessages([]));
    } else {
      setMessages([]);
    }
  }, [entry.session_key]);

  return (
    <div className="chat-log grow">
      <div className="row wrap" style={{ gap: 8, marginBottom: 10 }}>
        <span className="tag warn">{t("chat.readonlyHistory")}</span>
        <span className="muted">
          {entry.title || entry.agent}
          {entry.runtime ? ` · ${entry.runtime}` : ""}
        </span>
        <span className="grow" />
        <Button size="small" onClick={onNew}>
          + {t("chat.new")}
        </Button>
      </div>
      {messages === null ? (
        <Spinner />
      ) : messages.length === 0 ? (
        <div className="state empty">
          <p>{t("chat.empty")}</p>
        </div>
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
      <div style={{ height: 40 }} />
    </div>
  );
}
