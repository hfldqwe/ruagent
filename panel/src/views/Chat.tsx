// Chat: terminal-like conversations — pick agent, pick model, talk
// multi-turn on one persistent session. Models / permission modes /
// thinking levels come from the daemon's cached catalog (instant, with
// a manual sync); the model defaults to the agent's configured one so
// nobody is asked to choose every time. The history drawer reopens
// past conversations (live ones reattach and stream).

import { Fragment, memo, useCallback, useEffect, useRef, useState } from "react";
import { Button, Input, Popconfirm, Select, Tooltip } from "antd";
import {
  api,
  isRoleAgent,
  type AgentInfo,
  type ChatHistoryEntry,
  type OptionChoice,
  type SessionOptionInfo,
  HttpError,
} from "../api";
import { BrandMark } from "../brand";
import { Icon, type IconName } from "../icons";
import { useI18n } from "../i18n";
import { ErrorState, RelTime, Spinner, useToast } from "../ui";
import { Markdown } from "./lazy-markdown";
import { msToIso } from "./Sessions";

/** Deterministic per-workspace hue — the workspace tree reads at a glance
 * (each project keeps its colour across visits).
 *
 * The ten hues are tokens, not hexes: `--ws-1 … --ws-10` (primitives.md §7.1
 * R7). The view owns only the deterministic index; the light-mode
 * compensation lives in the token, so `getComputedStyle` can read the final
 * colour instead of a `filter` nobody can audit through. */
const WS_HUES = 10;
const wsColor = (ws: string) => {
  const i =
    [...ws].reduce((a, c) => (a * 31 + c.charCodeAt(0)) >>> 0, 7) % WS_HUES;
  return `var(--ws-${i + 1})`;
};

/** The workspace grouping key — view-chat.md §9.1, six ordered rules. Only
 *  the KEY is normalized; the group title still shows the raw cwd, so
 *  `C:\x` and `C:/x` land in one group without rewriting what the user
 *  typed. Rule 5 is Windows-only on purpose: on POSIX `Foo` and `foo` are
 *  different directories and lowercasing them would merge two real ones. */
export const normCwd = (raw: string): string => {
  let s = (raw ?? "").trim();
  if (!s) return "";
  // 1. strip the \?\ prefix (\?UNCsrvshare -> \srvshare)
  if (s.startsWith("\\\\?\\UNC\\")) s = "\\\\" + s.slice(8);
  else if (s.startsWith("\\\\?\\")) s = s.slice(4);
  // 2. one separator
  s = s.replace(/\\/g, "/");
  // 3. fold repeats, but keep a leading // (UNC) — 6. UNC stays distinct
  const unc = s.startsWith("//");
  s = s.replace(/\/{2,}/g, "/");
  if (unc) s = "/" + s;
  // 4. drop the trailing slash, except for roots (C:/ and /)
  if (s !== "/" && !/^[A-Za-z]:\/$/.test(s)) s = s.replace(/\/+$/, "");
  // 5. drive letter up, the rest down — Windows form only
  if (/^[A-Za-z]:(\/|$)/.test(s)) s = s[0].toUpperCase() + s.slice(1).toLowerCase();
  return s;
};

/** view-chat.md §C9 already pins the number: a group shows five rows by
 *  default and the rest sit behind "show more". Five — not a new value — and
 *  the cap is PER GROUP, so expanding one workspace never moves another. */
const GROUP_CAP = 5;

/** Density caps (view-chat.md §4.2). `#task` measured 153k nodes from an
 * unbounded log (MASTER §12 row 25); a full session is the same risk surface,
 * so the same two bounds apply here: at most 400 mounted turns, and a single
 * body over 2,000 characters collapses behind its own summary. */
const MSG_CAP = 400;
const LONG_MSG = 2000;

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
  /** A failed agent load is an ERROR, not an empty list (MASTER §12 row 20). */
  const [agentsError, setAgentsError] = useState<unknown>(null);
  /** A failed send: shown persistently above the composer, with a retry. */
  const [sendError, setSendError] = useState<unknown>(null);
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
  /** "Show earlier": the log mounts the last MSG_CAP turns by default. */
  const [showAll, setShowAll] = useState(false);
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
  /** §9 C4: an OPTIONAL agent filter — view-level and never persisted (no
   *  localStorage, no URL), so "default off" cannot be broken by a deep link.
   *  Filtering is not isolation: the rail lists every chat by default. */
  /** Groups expanded past GROUP_CAP — per group and IN MEMORY ONLY. Not
   *  persisted on purpose: one visit must not rewrite the default view, the
   *  same reason C4 gives for keeping the agent filter unpersisted. */
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());
  const [agentFilter, setAgentFilter] = useState<string | null>(null);
  /** §9.4 D4: a row that is pending must not accept a second delete.
   *  D3: after a failure it must NOT stay pending, and the error must be
   *  readable on screen (a toast alone disappears). */
  const [deleting, setDeleting] = useState<string | null>(null);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  /** The filter's own disclosure state — also view-level, also not persisted. */
  const [filterOpen, setFilterOpen] = useState(false);
  /** Collapsed workspaces. Persisted under `chat.collapsedGroups` — the same
   *  `chat.*` namespace as `chat.cwd` / `chat.workspaces` /
   *  `chat.railCollapsed`; we store the collapsed (usually few) keys rather
   *  than every expanded one, so a fresh install starts fully expanded. */
  const [collapsedGroups, setCollapsedGroups] = useState<Set<string>>(() => {
    try {
      return new Set(
        JSON.parse(localStorage.getItem("chat.collapsedGroups") ?? "[]") as string[],
      );
    } catch {
      return new Set();
    }
  });
  useEffect(() => {
    localStorage.setItem("chat.collapsedGroups", JSON.stringify([...collapsedGroups]));
  }, [collapsedGroups]);
  const toggleGroup = (key: string) =>
    setCollapsedGroups((g) => {
      const next = new Set(g);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  /** The sessions index joined by `session_key`. `archived` is a ruagent-side
   *  marker and `deletable` is false for every source whose transcript file
   *  ruagent only indexes — the chat history payload carries neither, so the
   *  rail cannot tell an archived row from a live one without this join. */
  const [index, setIndex] = useState<
    Record<string, { archived: boolean; deletable: boolean }>
  >({});
  const [showArchived, setShowArchived] = useState(false);
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

  const loadAgents = useCallback(() => {
    setAgentsError(null);
    setAgents(null);
    api
      .agents()
      .then((a) => {
        const enabled = a.filter((x) => x.enabled);
        setAgents(enabled);
        setAgent((cur) =>
          cur && enabled.some((a) => a.name === cur) ? cur : (enabled[0]?.name ?? ""),
        );
      })
      .catch((e) => {
        // Swallowing this into `[]` used to render a picker with no options
        // and no explanation, forever. Say what happened and offer a retry.
        setAgentsError(e);
        setAgents([]);
      });
  }, []);
  useEffect(() => {
    loadAgents();
  }, [loadAgents]);

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
    const apply = (list: SessionOptionInfo[]) => {
      setOptions(list);
      const m = list.find((o) => o.category === "model" || o.id === "model");
      // Configured default wins; otherwise the advertised current; otherwise
      // the FIRST advertised choice — a catalog that names no current model
      // must still leave the picker usable instead of blank forever.
      setModel((cur) => cur || m?.current || m?.choices?.[0]?.value || "");
    };
    api
      .agentOptions(agent, false, engine || undefined)
      .then((r) => {
        if (!alive) return;
        // An EMPTY catalog is not an answer. The daemon persists the result
        // of the first probe, and on a fresh data root that persisted copy is
        // {"options":[],"cached":true} — so the picker stayed blank forever
        // (CI: e2e/chat.spec.ts:34 expected "mock-pro", received ""). An empty
        // cached catalog therefore triggers exactly one real probe.
        if (r.options.length === 0) {
          api
            .agentOptions(agent, true, engine || undefined)
            .then((r2) => {
              if (alive) apply(r2.options);
            })
            .catch(() => {
              if (alive) apply([]);
            });
          return;
        }
        apply(r.options);
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

  /** The last prompt we handed to the daemon — what a retry re-delivers. */
  const lastSendRef = useRef<string | null>(null);

  const send = async (override?: string) => {
    const text = (override ?? input).trim();
    if (!text || streaming || starting || !agent) return;
    setInput("");
    setSendError(null);
    lastSendRef.current = text;
    setMessages((prev) => [...prev, { role: "user", text, done: true }]);
    setStreaming(true);
    try {
      const id = await ensureChat();
      attachStream(id);
      await api.chatMessage(id, text);
      // This chat may be brand new and is now generating: re-read the list so
      // its row appears with the "running" marker (and the poll below starts).
      refreshHistory();
    } catch (e) {
      // A failed send must not cost the user their prompt, and it must not
      // disappear into a 2s toast: the text goes back in the box and the
      // failure stays on screen until it is retried (view-chat.md §5).
      setInput((cur) => (cur ? cur : text));
      setSendError(e);
      setStreaming(false);
    }
  };

  /** Re-deliver the last prompt. It is already in the log, so this does not
   * append a second copy — it only retries the transport. */
  const retryLastSend = async () => {
    const text = lastSendRef.current;
    if (!text) return;
    setSendError(null);
    setStreaming(true);
    try {
      const id = await ensureChat();
      attachStream(id);
      await api.chatMessage(id, text);
    } catch (e) {
      setSendError(e);
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
  /** Merge a poll result into the rail BY ID (§9.5 L5/L6). Unchanged rows
   *  keep their OBJECT identity — the previous code replaced the whole array
   *  every 3–5s, re-rendering every row, which is what made the rail feel
   *  un-smooth. When nothing at all changed we return the PREVIOUS array, so
   *  React does not even re-render: no data change ⇒ no movement, no scroll
   *  jump, no focus loss. A genuinely new chat enters at the position the
   *  daemon's order gives it; rows the daemon dropped go away. */
  const mergeHistory = (next: ChatHistoryEntry[]) =>
    setHistory((cur) => {
      const prev = cur ?? [];
      if (!prev.length) return next;
      const byId = new Map(prev.map((h) => [h.id, h]));
      let changed = prev.length !== next.length;
      const merged = next.map((h) => {
        const old = byId.get(h.id);
        if (!old) return h;
        const same = (Object.keys(h) as (keyof ChatHistoryEntry)[]).every(
          (k) => old[k] === h[k],
        );
        if (same) return old;
        changed = true;
        return { ...old, ...h };
      });
      if (!changed) {
        for (let i = 0; i < prev.length; i += 1) {
          if (prev[i].id !== merged[i].id) {
            changed = true;
            break;
          }
        }
      }
      return changed ? merged : prev;
    });

  const refreshHistory = () => {
    // §9 C1: the rail lists EVERY chat — a session is found by looking for
    // it, not by first guessing which agent owns it. The agent deep link
    // (`#chat?agent=<id>`) no longer filters this list: it is the default
    // for the NEXT new session (C6).
    api.chatsHistory().then(mergeHistory).catch(() => setHistory([]));
    api
      .sessionsList({ archived: "include" })
      .then((r) => {
        const next = Object.fromEntries(
          r.sessions.map((s) => [
            s.key,
            { archived: !!s.archived, deletable: !!s.deletable },
          ]),
        );
        // Same discipline for the index map: an unchanged index must not
        // re-render the rail every few seconds either.
        setIndex((cur) =>
          JSON.stringify(cur) === JSON.stringify(next) ? cur : next,
        );
      })
      .catch(() => setIndex({}));
  };

  /** Archive is a ruagent-side marker on any source; delete is refused by the
   *  daemon (403) for every source whose file ruagent does not own, which is
   *  why the row only renders the delete affordance for `deletable` rows. */
  const toggleArchive = async (key: string, archived: boolean) => {
    try {
      if (archived) await api.sessionUnarchive(key);
      else await api.sessionArchive(key);
      refreshHistory();
      toast("ok", archived ? t("chat.unarchived") : t("chat.archived"));
    } catch (e) {
      toast("err", String(e));
    }
  };
  /** C8 / §9.4 D1–D6: delete the chat RECORD — the row itself. The old
   *  button only removed the sessions-index entry, which is exactly why the
   *  user could not clear a row from this list (the rail is fed by
   *  /api/v1/chats, not by the index).
   *
   *  Optimistic, and D3 makes the failure path explicit: the row comes BACK
   *  at its old position, an error is readable, and nothing is left pending.
   *  "Row gone but not deleted" is the worst outcome — the user believes it
   *  worked and the row returns on the next refresh. */
  const removeChat = async (h: ChatHistoryEntry) => {
    if (deleting === h.id) return; // D4: no second delete while pending
    const at = (history ?? []).findIndex((x) => x.id === h.id);
    setDeleting(h.id);
    setDeleteError(null);
    setHistory((cur) => (cur ?? []).filter((x) => x.id !== h.id));
    if (chatId === h.id) {
      // The chat being viewed is going away: drop it instead of leaving a
      // composer pointed at a deleted id (D2: no spinner, no stop button).
      if (streamRef.current) streamRef.current();
      setChatId(null);
      setStreaming(false);
      setMessages([]);
      setViewing(null);
    }
    try {
      await api.chatDelete(h.id);
      refreshHistory();
    } catch (e) {
      // D4: a 404 means the row is already gone — which IS the state we
      // wanted, so it must not surface as an error and must not roll back.
      // The branch reads the HTTP STATUS via HttpError: matching the message
      // text never worked, because the daemon's 404 body is "chat not found"
      // and carries no "404".
      const alreadyGone = e instanceof HttpError && e.status === 404;
      if (!alreadyGone) {
        setHistory((cur) => {
          const next = [...(cur ?? [])];
          if (!next.some((x) => x.id === h.id)) {
            next.splice(at < 0 ? 0 : Math.min(at, next.length), 0, h);
          }
          return next;
        });
        setDeleteError(String(e));
        toast("err", t("chat.deleteFailed"));
      }
    } finally {
      setDeleting(null);
    }
  };
  useEffect(() => {
    refreshHistory();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /** view-chat.md §9.3: `generating` is LIVE state while `history` is a
   *  snapshot, so the rail must re-read it. One adaptive interval, always on:
   *  **3s while a reply is being generated**, **5s otherwise** (one
   *  GET /api/v1/chats per tick). The slow baseline is not decoration — a
   *  generation can start outside this panel (another tab, an agent-driven
   *  chat), and a poll that only runs *while already generating* could never
   *  notice it starting. Cost: ≤12 requests/min idle, ≤20 while generating;
   *  in the audit's 11.5s window that is ≤3, well under the row-28 cap of 7. */
  const anyGenerating = (history ?? []).some((h) => h.generating);
  useEffect(() => {
    const i = setInterval(refreshHistory, anyGenerating ? 3000 : 5000);
    return () => clearInterval(i);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [anyGenerating]);

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
      // §9 C3: identity comes from the ENTRY, never from the selector state.
      setAgent(r.agent || h.agent);
      setChatId(r.id);
      setModel(r.model ?? h.model ?? "");
      setRuntime(r.runtime ?? h.runtime ?? "");
      setMessages([]);
      setStreaming(false);
      attachStream(r.id, true); // replay rebuilds the whole thread
      refreshHistory();
    } catch {
      setViewing(h); // agent deleted / daemon refuses: read-only
    }
  };

  /** Sessions grouped by WORKSPACE (§9 C1/C7) — the group key is
   * `normCwd(cwd)`, the title keeps the first raw spelling seen, and the
   * current project's group sorts first, then named ones, then ungrouped. */
  const {
    groups: historyGroups,
    labels: groupLabels,
    visible: visibleRows,
    filtering: filterActive,
  } = (() => {
    const q = sessionQuery.trim().toLowerCase();
    const filtering = !!agentFilter || !!q;
    const filtered = (history ?? []).filter((h) => {
      // A chat with no messages is not a session. The panel only ever creates
      // one when a message is sent (ensureChat has exactly two call sites:
      // send and retry), so a message-less chat can only come from calling the
      // API directly — a test or script residue. Hidden BY RULE, so the next
      // residue never reaches the user instead of relying on someone
      // remembering to clean up.
      if (!h.message_count) return false;
      // Archived rows are hidden by default — that is what archiving means.
      if (!showArchived && h.session_key && index[h.session_key]?.archived) {
        return false;
      }
      if (agentFilter && h.agent !== agentFilter) return false;
      return (
        !q ||
        (h.title ?? "").toLowerCase().includes(q) ||
        (h.preview ?? "").toLowerCase().includes(q) ||
        (h.agent ?? "").toLowerCase().includes(q)
      );
    });
    const by = new Map<string, ChatHistoryEntry[]>();
    const labels = new Map<string, string>();
    // Explicit workspaces exist even with no sessions yet (暂无会话).
    for (const ws of workspaces) {
      const k = normCwd(ws);
      if (!by.has(k)) by.set(k, []);
      if (!labels.has(k)) labels.set(k, ws);
    }
    for (const h of filtered) {
      const raw = (h.cwd ?? "").trim();
      const k = normCwd(raw);
      if (!labels.has(k)) labels.set(k, raw);
      const list = by.get(k) ?? [];
      list.push(h);
      by.set(k, list);
    }
    const cur = normCwd(project);
    const groups = [...by.entries()].sort((a, b) => {
      const rank = (k: string) => (k === cur ? 0 : k === "" ? 2 : 1);
      return rank(a[0]) - rank(b[0]) || a[0].localeCompare(b[0]);
    });
    return { groups, labels, visible: filtered.length, filtering };
  })();

  const archivedCount = (history ?? []).filter(
    (h) => h.session_key && index[h.session_key]?.archived,
  ).length;

  // A failed agent load used to fall through to a spinner or an empty
  // picker. It is a page-level error with a retry (MASTER §12 row 20).
  if (agentsError) {
    return (
      <ErrorState
        title={t("common.offline")}
        hint={String(agentsError)}
        onRetry={loadAgents}
        retryLabel={t("task.retryRun")}
      />
    );
  }
  if (!agents) return <Spinner label={`${t("chat.title")}…`} />;

  const currentAgent = agents.find((a) => a.name === agent);

  /** The conversation the user is actually in: its recorded title (real
   * data, `ChatHistoryEntry.title`), falling back to the first prompt. The
   * view-bar used to say only "对话", so the page never told you WHICH
   * conversation you were reading (view-chat.md §3). */
  const currentSession = history?.find((h) => h.id === chatId) ?? null;
  const sessionTitle =
    currentSession?.title?.trim() ||
    messages.find((m) => m.role === "user" && !m.notice && !m.kind)?.text.slice(0, 80) ||
    agent;
  const active = !viewing && messages.length > 0;
  const windowOffset = showAll ? 0 : Math.max(0, messages.length - MSG_CAP);
  const shownMessages = showAll ? messages : messages.slice(-MSG_CAP);

  /** The composer unit: input row + hairline-divided controls row (agent,
   * model, mode/effort, project). Rendered centered when the conversation
   * is empty, sticky at the bottom once it has messages. */
  const composerBlock = (
    <div className="chat-bottom">
      {sendError ? (
        <ErrorState
          title={t("common.offline")}
          hint={String(sendError)}
          onRetry={() => {
            void retryLastSend();
          }}
          retryLabel={t("task.retryRun")}
        />
      ) : null}
      <div className="composer">
        <div className="composer-row">
          <textarea
            ref={inputRef}
            rows={2}
            aria-label={t("chat.inputPh")}
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
              onClick={() => {
                void send();
              }}
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
        <div className="chat-side-head zone-head">
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
        {deleteError ? (
          <p className="muted truncated" role="alert">
            {t("chat.deleteFailed")}
          </p>
        ) : null}
        <Input
          allowClear
          variant="filled"
          value={sessionQuery}
          onChange={(e) => setSessionQuery(e.target.value)}
          placeholder={t("chat.searchSessions")}
          size="small"
          prefix={<Icon name="search" size={12} />}
        />
        {/* §9 C4 — an optional filter, deliberately built from plain buttons:
            an antd Select adds two inline-styled nodes to the route (row 12's
            budget is per row), and this needs aria-expanded + a pressed state
            anyway. It never persists. */}
        <div className="chat-filter">
          <button
            className="chat-group-more"
            aria-expanded={filterOpen}
            onClick={() => setFilterOpen((v) => !v)}
          >
            {agentFilter
              ? t("chat.filteringBy", { name: agentFilter })
              : t("chat.filterAgent")}
          </button>
          {filterOpen ? (
            <div className="chat-filter-list">
              <button
                className="chat-group-more"
                aria-pressed={agentFilter === null}
                onClick={() => {
                  setAgentFilter(null);
                  setFilterOpen(false);
                }}
              >
                {t("chat.allAgents")}
              </button>
              {agents.filter(isRoleAgent).map((a) => (
                <button
                  key={a.name}
                  className="chat-group-more"
                  aria-pressed={agentFilter === a.name}
                  onClick={() => {
                    setAgentFilter(a.name);
                    setFilterOpen(false);
                  }}
                >
                  {a.name}
                </button>
              ))}
            </div>
          ) : null}
        </div>
        <div className="chat-side-list">
          <div className="chat-ws-title zone-title">{t("chat.workspaces")}</div>
          {history === null ? (
            <Spinner />
          ) : visibleRows === 0 && filterActive ? (
            /* §9.2: a filter that matches nothing must SAY so — falling back
               to "all sessions" would read as "the filter did nothing". */
            <div className="chat-filter-empty">
              <p className="muted">
                {agentFilter
                  ? t("chat.noAgentSessions", { name: agentFilter })
                  : t("chat.noResults")}
              </p>
              <button className="chat-group-more" onClick={() => setAgentFilter(null)}>
                {t("chat.clearFilter")}
              </button>
            </div>
          ) : historyGroups.length === 0 ? (
            <p className="muted">{t("chat.historyEmpty")}</p>
          ) : (
            historyGroups.map(([key, items], gi) => {
              const cwd = groupLabels.get(key) ?? "";
              // While filtering, an empty group is noise — the explicit empty
              // state below owns that case (§9.2).
              if (filterActive && items.length === 0) return null;
              // §C9: five rows per group by default, the rest behind this
              // group's own "show more" — one workspace never expands another.
              const shown = expandedGroups.has(key) ? items : items.slice(0, GROUP_CAP);
              const collapsed = collapsedGroups.has(key);
              const wsLabel = cwd ? cwd.replace(/.*[\\/]/, "") : t("chat.noProject");
              const rowsId = `chat-ws-rows-${gi}`;
              const foldLabel = collapsed
                ? t("chat.expandGroup", { name: wsLabel })
                : t("chat.collapseGroup", { name: wsLabel });
              return (
                <div key={key || "none"} className="chat-group">
                  {/* Clicking the head folds the group; the folder icon is the
                      keyboard-reachable control (row 17/18: named + 24x24). */}
                  <div
                    className="chat-group-head"
                    title={cwd || undefined}
                    onClick={() => toggleGroup(key)}
                  >
                    <button
                      className="icon-btn chat-group-toggle"
                      aria-expanded={!collapsed}
                      aria-controls={rowsId}
                      aria-label={foldLabel}
                      title={foldLabel}
                      onClick={(e) => {
                        e.stopPropagation();
                        toggleGroup(key);
                      }}
                    >
                      <Icon
                        name={collapsed ? "folder" : "folderOpen"}
                        size={14}
                        className="ws-folder"
                        style={{ color: key ? wsColor(key) : undefined, flex: "none" }}
                      />
                    </button>
                    <span className="chat-group-name zone-title">{wsLabel}</span>
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
                  {/* The rows sit one indent level in (32px — the same step
                      .chat-group-empty / .chat-group-more already use), so a
                      session reads as belonging to its workspace instead of
                      sitting flush with the group head. */}
                  <div
                    id={rowsId}
                    className="chat-session-list"
                    style={{ paddingLeft: 32, marginLeft: 8 }}
                  >
                    {collapsed
                      ? null
                      : shown.map((h) => {
                    const harness = agents?.find((a) => a.name === h.runtime)?.harness;
                    const meta = h.session_key ? index[h.session_key] : undefined;
                    const label = h.title || h.preview || t("sessions.untitled");
                    return (
                      <div key={h.id} className="row chat-session-row">
                        <button
                          className={`row-btn chat-session grow${h.id === chatId ? " selected" : ""}`}
                          onClick={() => openPast(h)}
                          title={label}
                        >
                          {h.generating ? (
                            <span className="live-dot" title={t("chat.generating")} />
                          ) : null}
                          <span className="title muted">{label}</span>
                          {harness ? (
                            <BrandMark harness={harness} size={12} mono className="muted" />
                          ) : null}
                          {/* Row 40: a 197px row holds title + agent + brand
                              + time + two 32px actions, so the short labels
                              must never wrap — .truncated (pre-existing) is
                              nowrap + ellipsis, no new CSS. */}
                          <span className="micro muted truncated chat-session-agent">
                            {h.agent}
                          </span>
                          <span className="time truncated">
                            <RelTime iso={msToIso(h.updated_at)} />
                          </span>
                        </button>
                        {h.session_key ? (
                          <button
                            className="icon-btn icon-btn-lg"
                            aria-label={meta?.archived ? t("chat.unarchive") : t("chat.archive")}
                            title={meta?.archived ? t("chat.unarchive") : t("chat.archive")}
                            onClick={() =>
                              void toggleArchive(h.session_key as string, !!meta?.archived)
                            }
                          >
                            <Icon name={meta?.archived ? "unarchive" : "archive"} size={14} />
                          </button>
                        ) : null}
                        {/* D5: two-step, because this is irreversible and it
                            is user data. The confirm states the identity of
                            what is about to go (title + agent + messages), and
                            the wording says what actually happens: the row
                            leaves the list — the transcript and any distilled
                            memory are NOT erased. Claiming it erases the
                            conversation would be the UI lying. */}
                        <Popconfirm
                          title={t("chat.deleteChatConfirm", { name: label })}
                          description={
                            <span className="muted">
                              {t("chat.deleteChatMeta", {
                                agent: h.agent ?? "",
                                n: h.message_count ?? 0,
                              })}
                              {" · "}
                              {t("chat.deleteChatBody")}
                            </span>
                          }
                          okText={t("chat.deleteChat")}
                          cancelText={t("chat.cancel")}
                          okButtonProps={{ danger: true, disabled: deleting === h.id }}
                          onConfirm={() => void removeChat(h)}
                        >
                          <button
                            className="icon-btn icon-btn-lg"
                            aria-label={t("chat.deleteChat")}
                            title={t("chat.deleteChat")}
                            disabled={deleting === h.id}
                          >
                            <Icon name="trash" size={14} />
                          </button>
                        </Popconfirm>
                      </div>
                    );
                  })}
                  </div>
                  {shown.length === 0 ? (
                    <div className="chat-group-empty muted">{t("chat.noSessions")}</div>
                  ) : null}
                  {!collapsed && items.length > GROUP_CAP ? (
                    <button
                      className="chat-group-more truncated"
                      aria-expanded={expandedGroups.has(key)}
                      onClick={() =>
                        setExpandedGroups((g) => {
                          const next = new Set(g);
                          if (next.has(key)) next.delete(key);
                          else next.add(key);
                          return next;
                        })
                      }
                    >
                      {expandedGroups.has(key)
                        ? t("chat.showLess")
                        : t("chat.showMore", { n: items.length - GROUP_CAP })}
                    </button>
                  ) : null}
                </div>
              );
            })
          )}
          {archivedCount > 0 ? (
            <button
              className="chat-group-more"
              aria-pressed={showArchived}
              onClick={() => setShowArchived((v) => !v)}
            >
              {showArchived
                ? t("chat.hideArchived")
                : t("chat.showArchived", { n: archivedCount })}
            </button>
          ) : null}
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
        {/* Row 24: every route owes the document outline exactly one h1.
            The visible 20px title stays `.view-bar h2` (frozen selector). */}
        {/* `.micro` on purpose: `.sr-only` does not reset font-size, and a
            hidden h1 inheriting the UA 2em would be an off-ladder 28px that
            MASTER §12 row 8 counts. */}
        <h1 className="sr-only micro">{t("chat.title")}</h1>
        <h2>{t("chat.title")}</h2>
        {active && currentSession?.message_count != null ? (
          <span className="readout s" title={t("sessions.messages")}>
            {currentSession.message_count} {t("sessions.messages")}
          </span>
        ) : null}
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
        /* Empty conversation: the composer centers in the leftover space
           below the header — the canvas is the invitation, not a void
           (the header itself stays pinned top-left of the column). */
        <div className="chat-empty-zone">
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
        </div>
      ) : (
        <>
          {/* The conversation's own title, at title grade (20/26): the page's
              hierarchy centre when a conversation is open (C2's upgrade path:
              h2 + this h3 + the turn-count readout = 3 nodes ≥18px). It
              reuses `.chat-hero`, the page's existing title-grade primitive;
              the spec asked for it inside `.view-bar`, which has no
              subtitle slot at this grade (see output notes). */}
          <div className="chat-hero chat-session-head">
            <h3>{sessionTitle}</h3>
            <p>
              {agent}
              {model ? ` · ${model}` : ""}
            </p>
          </div>
          <div className="chat-log grow">
          {windowOffset > 0 ? (
            <button
              className="row-btn chat-earlier"
              onClick={() => setShowAll(true)}
            >
              {t("chat.showAll", { n: windowOffset })}
            </button>
          ) : null}
          {shownMessages.map((m, i) => (
            <ChatRow key={windowOffset + i} m={m} />
          ))}
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


/** One turn of the log.
 *
 * Memoized on the `Message` object: the stream appends a new object for the
 * tail only, so the turns above it keep their identity and stop re-parsing
 * their markdown on every chunk (view-chat.md §4.2 — the old inline map
 * rebuilt the whole list, and `Markdown`, per chunk). */
const ChatRow = memo(function ChatRow({ m }: { m: Message }) {
  const { t } = useI18n();
  if (m.kind === "injection") {
    return (
      <details className="chat-injection">
        <summary>
          <Icon name="brain" size={12} /> {t("chat.injection")} ·{" "}
          {t("chat.chars", { n: m.text.length })}
        </summary>
        <pre>{m.text}</pre>
      </details>
    );
  }
  if (m.kind === "thought") {
    // Open while streaming (the tail stays live — the page auto-scrolls on
    // every message change); collapses when the turn ends, reopenable.
    return (
      <details className="chat-thought" open={!m.done ? true : undefined}>
        <summary>
          <Icon name="thought" size={12} /> {t("chat.thought")} ·{" "}
          {t("chat.chars", { n: m.text.length })}
        </summary>
        <div className="thought-body">{m.text}</div>
      </details>
    );
  }
  const plain = m.role === "user" || m.notice;
  const body =
    m.text.length > LONG_MSG ? (
      <details className="chat-long">
        <summary className="muted micro">
          {t("chat.chars", { n: m.text.length })}
        </summary>
        {plain ? <pre className="raw">{m.text}</pre> : <Markdown>{m.text}</Markdown>}
      </details>
    ) : plain ? (
      m.text
    ) : (
      <Markdown>{m.text}</Markdown>
    );
  return (
    <div
      className={
        m.notice ? "chat-msg notice" : m.role === "user" ? "chat-msg user" : "chat-msg agent"
      }
    >
      {m.notice && m.icon ? <Icon name={m.icon} size={12} /> : null}
      {body}
    </div>
  );
});

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
