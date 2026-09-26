// Chat: terminal-like conversations — pick agent, pick model, talk
// multi-turn on one persistent session. Models / permission modes /
// thinking levels come from the daemon's cached catalog (instant, with
// a manual sync); the model defaults to the agent's configured one so
// nobody is asked to choose every time. The history drawer reopens
// past conversations (live ones reattach and stream).

import {
  Fragment,
  memo,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import { Button, Input, Modal, Popconfirm, Select, Tooltip } from "antd";
import {
  api,
  isRoleAgent,
  type AgentInfo,
  type ChatHistoryEntry,
  type OptionChoice,
  type SessionOptionInfo,
  type SessionRecord,
  HttpError,
} from "../api";
import { BrandMark } from "../brand";
import { Icon, type IconName } from "../icons";
import { useI18n } from "../i18n";
import {
  ErrorState,
  RelTime,
  ResizeHandle,
  Spinner,
  useSidebarResize,
  useToast,
} from "../ui";
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

  // S2: the chat rail is the resizable column (t132's shared mechanism).
  // The width goes into the CSS variable t144 added (--chat-rail-w): the
  // column is a GRID TRACK, so the track is what must move - writing a width
  // onto .chat-side changed its box but not the layout (F-134a, measured:
  // box 248->300 while grid-template-columns stayed 248px and the content
  // stayed at x=517). `other` is the nav rail's LIVE width, measured from
  // the DOM: the hook uses it only for the max formula, but a stale constant
  // would drift as soon as the user widens the nav rail.
  const [navRailWidth, setNavRailWidth] = useState(228);
  useEffect(() => {
    const el = document.querySelector(".app-sider");
    if (!el) return;
    const measure = () =>
      setNavRailWidth(Math.round(el.getBoundingClientRect().width) || 228);
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  const chatRail = useSidebarResize({
    id: "chat",
    def: 248,
    other: navRailWidth,
    enabled: !railCollapsed,
  });
  const [history, setHistory] = useState<ChatHistoryEntry[] | null>(null);
  /** t200: the rail's row order while the pointer is inside it. A prompt in
   *  ANY chat refreshes that chat's updated_at and the rail is ordered by it
   *  (chat.rs:1157), so the list used to reorder itself under the user's
   *  cursor — measured 10 reorders/minute while another chat was being
   *  prompted, 0 when quiet. Holding the order fixes the experience without
   *  touching the rule: conversations still drive the order, they just stop
   *  doing it while you are reading. */
  const [heldOrder, setHeldOrder] = useState<string[] | null>(null);
  /** t171: the rail's read failed — the rail then shows WHY (with a retry),
   *  instead of an empty list that reads as「你没有会话」. Same rule as the
   *  page-level error: one presentation per failure, and it is never an empty
   *  state. */
  const [historyError, setHistoryError] = useState<unknown>(null);
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
  /** t178 — membership is decided from `index` (below), which carries each
   *  session's archived flag. The previous shape kept every archived row in
   *  `history` and hid it behind a `showArchived` toggle — exactly the behaviour
   *  the user reported (「点击显示归档，然后又回到了原来的位置」). */
  /** Count from the same response (`archived_count`) — the entry's label. */
  const [archivedCount, setArchivedCount] = useState(0);
  /** The archived destination: opened on demand, nothing prefetched. */
  const [archiveOpen, setArchiveOpen] = useState(false);
  const [archivedRows, setArchivedRows] = useState<SessionRecord[] | null>(null);
  const [archivedErr, setArchivedErr] = useState<unknown>(null);
  const [unarchiving, setUnarchiving] = useState<string | null>(null);
  /** A conversation that is open AND archived: a bin is not a place to talk,
   *  so the composer stays disabled until it is restored. */
  const [archivedOpen, setArchivedOpen] = useState<string | null>(null);
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
  /** t183: one SSE per chat, not one SSE per panel. Switching away must not
   * tear down a run's feed — the daemon keeps generating either way (one Chat
   * per id, and send_prompt has no busy gate), but a closed feed means the
   * tokens land nowhere and the user returns to a chat that looks frozen.
   * Keyed by chat id, so a chat still attaches only once (reattaching replays
   * the transcript — the duplicate-append bug on multi-turn conversations). */
  const streamsRef = useRef<Map<string, () => void>>(new Map());
  /** The chat the transcript on screen belongs to. An SSE handler for any
   * other chat must never append into it. */
  const chatIdRef = useRef<string | null>(null);
  /** Chats with a reply in flight, from SSE events plus the rail poll. Per
   * chat on purpose: A generating must not disable B's composer (t183). */
  const [runningIds, setRunningIds] = useState<string[]>([]);
  const runningRef = useRef<string[]>([]);
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

  // The SSE handlers read the open chat id synchronously, so keep the ref in
  // step with the state (openPast/newChat also set it inline, for the window
  // before this effect runs).
  useEffect(() => {
    chatIdRef.current = chatId;
  }, [chatId]);

  // Never leak a pending reconnect timer — or a stream — past the component.
  useEffect(
    () => () => {
      const st = reconnectRef.current;
      if (st.timer !== null) clearTimeout(st.timer);
      for (const close of streamsRef.current.values()) close();
      streamsRef.current.clear();
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
  /** t171 item 3: the (agent, engine) pair we already asked for. This effect
   *  runs TWICE on a normal load — the first run stores the agent's runtime,
   *  which re-runs the effect — and the fetch below was unconditional, so
   *  /api/v1/agents/<id>/options went out twice with the SAME url (measured:
   *  ?runtime=dsh x2). One (agent, engine) pair is one catalog: the second run
   *  has nothing new to ask for. A failed read clears the key so a later run
   *  may retry. */
  const loadedKeyRef = useRef<string | null>(null);
  /** t230: the fetch currently in flight, keyed by the (agent, engine) pair it
   *  answers for. A same-pair re-run reuses it instead of cancelling it; a
   *  different pair retires it. */
  const inflightRef = useRef<{ key: string; alive: boolean } | null>(null);
  useEffect(() => {
    if (!agent) return;
    const a = agents?.find((x) => x.name === agent);
    const engine = runtime || a?.runtime || "";
    const agentChanged = lastAgentRef.current !== agent;
    const engineChanged = lastEngineRef.current !== engine;
    lastAgentRef.current = agent;
    lastEngineRef.current = engine;
    setConfigModels(a?.models ?? []);
    if (agentChanged) {
      setModel(a?.model ?? "");
      setRuntime(a?.runtime ?? "");
    } else if (engineChanged) {
      // Same role, other engine: the old model id is not portable.
      setModel("");
    }
    const key = agent + "|" + engine;
    // t230: one (agent, engine) pair is one catalog, and this effect re-runs on
    // its own setRuntime below. Two things must hold at once:
    //   · a no-op re-run must not ask again (t171: the same URL went out twice)
    //   · and it must not CANCEL the fetch the first run started either
    // A cleanup-based `alive` flag broke the second: React runs the previous
    // cleanup before the next effect body, so the fetch was killed and the
    // re-run then returned early — nothing was left to ask and the picker sat on
    // its loading (empty) Select forever. Measured on the real root: .ctl-model
    // carried `ant-select-loading` with 0 options while
    // GET /agents/approver/options?runtime=dsh had answered with the full
    // 7-model catalog (the mock shape only looked healthy because a local
    // catalog answers before a re-run can cancel it). A ticket keyed by the pair
    // does both: a same-key re-run reuses it, a different key retires it.
    const ticket = inflightRef.current;
    if (ticket && ticket.key === key) return;
    if (ticket) ticket.alive = false;
    const mine = { key, alive: true };
    inflightRef.current = mine;
    loadedKeyRef.current = key;
    // t230: the clear belongs HERE, below the guard — not above it.
    //
    // This effect runs TWICE on a normal load (the first run stores the agent's
    // runtime, which re-runs the effect). The second run is a no-op and returns
    // early at the line above, so a clear placed before it wiped the catalog the
    // first run had just loaded and nothing ever refilled it: the picker stayed
    // on its loading (empty) Select forever. Measured on the real root: the
    // .ctl-model div carried `ant-select-loading`, had 0 options and an empty
    // placeholder, while GET /agents/approver/options?runtime=dsh had returned
    // the full 7-model catalog. Clearing below the guard keeps the loading state
    // for a real refetch (agent/engine change) and leaves a no-op run alone.
    setOptions(null);
    const apply = (list: SessionOptionInfo[]) => {
      if (!mine.alive) return;
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
        if (!mine.alive) return;
        // An EMPTY catalog is not an answer. The daemon persists the result
        // of the first probe, and on a fresh data root that persisted copy is
        // {"options":[],"cached":true} — so the picker stayed blank forever
        // (CI: e2e/chat.spec.ts:34 expected "mock-pro", received ""). An empty
        // cached catalog therefore triggers exactly one real probe.
        if (r.options.length === 0) {
          api
            .agentOptions(agent, true, engine || undefined)
            .then((r2) => {
              if (mine.alive) apply(r2.options);
            })
            .catch(() => {
              if (mine.alive) apply([]);
            });
          return;
        }
        apply(r.options);
      })
      .catch(() => {
        if (mine.alive) {
          // A failed read retires the ticket so a later run may retry.
          mine.alive = false;
          inflightRef.current = null;
          loadedKeyRef.current = null;
          setOptions([]);
        }
      });
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

  /** t183: the running set is the panel's per-chat truth. SSE events feed it
   * (a chunk means alive, stopped/error means finished) and so does the rail
   * poll's own `generating` flag, so a chat started in another tab shows up. */
  const markRunning = (id: string, on: boolean) => {
    const cur = runningRef.current;
    if (on === cur.includes(id)) return;
    const next = on ? [...cur, id] : cur.filter((x) => x !== id);
    runningRef.current = next;
    setRunningIds(next);
  };

  /** Drop one chat's feed. Used when that chat's session is replaced or
   * deleted — never when the user merely looks at a different chat. */
  const closeStream = (id: string | null) => {
    if (!id) return;
    streamsRef.current.get(id)?.();
    streamsRef.current.delete(id);
    markRunning(id, false);
  };

  const scheduleReconnect = (id: string) => {
    const st = reconnectRef.current;
    if (st.timer !== null) return;
    if (st.tries >= 5) {
      markRunning(id, false);
      // Only the chat on screen can carry a notice; a background chat's
      // transcript is rebuilt by the replay when the user returns to it.
      if (chatIdRef.current !== id) return;
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
      // The replay rebuilds the transcript — but only the open one's; a
      // background chat reattaches without touching what is on screen.
      if (chatIdRef.current === id) setMessages([]);
      attachStream(id, true);
    }, delay);
  };

  const attachStream = (id: string, force = false) => {
    // t183: per-chat. Attaching B leaves A's stream open — that is the whole
    // point (parallel conversations), and the map key replaces the old
    // single "attached to" ref.
    if (!force && streamsRef.current.has(id)) return;
    streamsRef.current.get(id)?.();
    const es = new EventSource(`/api/v1/chat/${id}/events`);
    const close = () => es.close();
    streamsRef.current.set(id, close);
    es.onmessage = (e) => {
      reconnectRef.current.tries = 0; // liveness
      try {
        const parsed = JSON.parse(e.data) as { event?: ChatEvent } & ChatEvent;
        // Transcript replay lines wrap the event ({ts, seq, event});
        // live events arrive flat. Handle both.
        handleEvent(parsed.event ?? parsed, id);
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
      streamsRef.current.delete(id);
      markRunning(id, false);
      // Only the chat on screen owns the composer's spinner.
      if (chatIdRef.current === id) setStreaming(false);
    });
    es.onerror = () => {
      es.close();
      streamsRef.current.delete(id);
      // The server closes the stream after each prompt's `end` — an
      // error with no reply in flight is that close racing us (or a
      // dead daemon nobody is talking to). Only a drop mid-reply is
      // worth reconnecting, and "mid-reply" is now per chat: a
      // background conversation deserves its feed back too.
      const inFlight =
        chatIdRef.current === id ? streamingRef.current : runningRef.current.includes(id);
      if (!inFlight) return;
      scheduleReconnect(id);
    };
  };

  const handleEvent = (ev: ChatEvent, id: string) => {
    // t183: this is what makes parallel chats safe. A chunk from a chat the
    // user is not looking at must NOT append into the transcript on screen —
    // it only refreshes that chat's running marker (and the replay rebuilds
    // its transcript when the user switches back).
    markRunning(id, !(ev.type === "stopped" || ev.type === "error"));
    if (id !== chatIdRef.current) return;
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
    // t178: an archived conversation is not a place to talk. The composer is
    // disabled, and the guard lives here too so Enter/retry cannot slip past a
    // disabled control.
    if (archivedOpen || !text || streaming || starting || !agent) return;
    setInput("");
    setSendError(null);
    lastSendRef.current = text;
    setMessages((prev) => [...prev, { role: "user", text, done: true }]);
    setStreaming(true);
    try {
      const id = await ensureChat();
      markRunning(id, true);
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
      markRunning(id, true);
      attachStream(id);
      await api.chatMessage(id, text);
    } catch (e) {
      setSendError(e);
      setStreaming(false);
    }
  };

  const newChat = () => {
    // t183: opening a blank composer is not a reason to stop listening to
    // the chats that are still generating — they keep their feeds.
    setViewing(null);
    setChatId(null);
    chatIdRef.current = null;
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
        closeStream(chatId); // this chat's session is being replaced
        setAgent(name);
        setChatId(r.id);
        chatIdRef.current = r.id;
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
    closeStream(chatId);
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
        closeStream(chatId);
        setChatId(chat.id);
        chatIdRef.current = chat.id;
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

  /** t171 item 3 — one switch, one read.
   *
   *  The rail has three independent triggers that can land inside the same
   *  switch: the resume path, the stream's "done" event, and the 5s poll. Each
   *  one re-read BOTH /api/v1/chats and /api/v1/sessions, and each call was
   *  unconditional — measured on one row click: chats x3 + sessions x3.
   *
   *  Coalescing window: the first trigger in a window does the read, later
   *  triggers inside the same window are DROPPED (not queued). Dropping is safe
   *  because every trigger is a「the rail may have changed」hint, and the poll
   *  re-reads within 5s anyway; nothing is lost, only repeated. Explicit user
   *  actions (the two retry buttons) pass `force` so a click always reads. */
  const REFRESH_WINDOW_MS = 1200;
  const lastRefreshRef = useRef(0);
  const readHistory = () => {
    // §9 C1: the rail lists EVERY chat — a session is found by looking for
    // it, not by first guessing which agent owns it. The agent deep link
    // (`#chat?agent=<id>`) no longer filters this list: it is the default
    // for the NEXT new session (C6).
    api
      .chatsHistory()
      .then((rows) => {
        setHistoryError(null);
        mergeHistory(rows);
      })
      // Measured (t171): this used to swallow into `[]`, so a 500 on
      // /api/v1/chats rendered the rail as「还没有会话」with no explanation —
      // the exact shape row 20 exists to catch.
      .catch((e) => {
        setHistoryError(e);
        setHistory([]);
      });
    // t178 — the rail reads the session index to decide membership, so archiving
    // is a move out of the list rather than a flag the list has to remember to
    // respect. It asks for `include` on purpose: the membership test needs the
    // archived FLAG, and a response that omits archived sessions cannot be told
    // apart from one that omits sessions the indexer has not reached yet. It is
    // still a single request carrying the per-key `deletable` marker and the
    // archived count, and archived rows are still never rendered or tracked.
    api
      .sessionsList({ archived: "include" })
      .then((r) => {
        const next: Record<string, { archived: boolean; deletable: boolean }> = {};
        for (const s of r.sessions) {
          next[s.key] = { archived: !!s.archived, deletable: !!s.deletable };
        }
        // Same discipline for the index map: an unchanged index must not
        // re-render the rail every few seconds either.
        setIndex((cur) =>
          JSON.stringify(cur) === JSON.stringify(next) ? cur : next,
        );
        setArchivedCount(r.archived_count ?? 0);
      })
      // Fail OPEN on purpose: with no index every session is unknown, and an
      // unknown session is shown (see the membership test). Failing closed here
      // is what hid freshly created conversations until the indexer caught up.
      .catch(() => setIndex({}));
  };
  const refreshHistory = (force = false) => {
    const now = Date.now();
    if (!force && now - lastRefreshRef.current < REFRESH_WINDOW_MS) return;
    lastRefreshRef.current = now;
    readHistory();
  };

  /** Archive is a ruagent-side marker on any source; delete is refused by the
   *  daemon (403) for every source whose file ruagent does not own, which is
   *  why the row only renders the delete affordance for `deletable` rows. */
  /** t178 — the archived destination. It reads the archived set ITSELF, only
   *  when it is opened: while a conversation is in the bin, nothing about it is
   *  fetched (no messages, no rail entry, no per-chat stream). */
  const openArchive = () => {
    setArchiveOpen(true);
    setArchivedRows(null);
    setArchivedErr(null);
    api
      .sessionsList({ archived: "only" })
      .then((r) => {
        setArchivedRows(r.sessions);
        setArchivedCount(r.archived_count ?? 0);
      })
      .catch((e) => setArchivedErr(e));
  };
  /** Restoring is the only way back — that is the user's model (「只有还原归档
   *  的时候才需要恢复到原来的位置」). */
  const unarchive = async (key: string) => {
    setUnarchiving(key);
    try {
      await api.sessionUnarchive(key);
      setArchivedOpen((cur) => (cur === key ? null : cur));
      // force: the user acted, so the rail's membership query must re-run now
      // rather than wait out the coalescing window.
      refreshHistory(true);
      api
        .sessionsList({ archived: "only" })
        .then((r) => {
          setArchivedRows(r.sessions);
          setArchivedCount(r.archived_count ?? 0);
        })
        .catch((e) => setArchivedErr(e));
    } catch (e) {
      setArchivedErr(e);
    } finally {
      setUnarchiving(null);
    }
  };
  const toggleArchive = async (key: string, archived: boolean) => {
    try {
      if (archived) await api.sessionUnarchive(key);
      else await api.sessionArchive(key);
      const open = (history ?? []).find((h) => h.id === chatId);
      if (!archived && open?.session_key === key) {
        // t178: archiving the conversation you are looking at moves it out of
        // the rail (membership) and STOPS it being tracked — the stream is
        // detached, so an archived chat no longer reports agent progress.
        // t183: only THIS chat's feed goes; the others keep running.
        closeStream(chatId);
        setStreaming(false);
        setArchivedOpen(key);
      } else if (archived && archivedOpen === key) {
        setArchivedOpen(null);
      }
      refreshHistory(true);
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
      closeStream(h.id);
      setChatId(null);
      chatIdRef.current = null;
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
  // t183: a background chat we hold a stream for counts as generating too, so
  // the rail's marker and the faster poll keep up with it.
  const anyGenerating = (history ?? []).some((h) => h.generating) || runningIds.length > 0;
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
      setAgent(h.agent);
      setChatId(h.id);
      chatIdRef.current = h.id;
      setModel(h.model ?? "");
      setRuntime(h.runtime ?? "");
      setMessages([]);
      // t183: the composer reflects THIS chat's run, not the panel's — a
      // chat that is still generating elsewhere must not grey out this one.
      setStreaming(runningRef.current.includes(h.id));
      attachStream(h.id, true);
      return;
    }
    try {
      const r = await api.chatResume(h.id);
      setViewing(null);
      // §9 C3: identity comes from the ENTRY, never from the selector state.
      setAgent(r.agent || h.agent);
      setChatId(r.id);
      chatIdRef.current = r.id;
      setModel(r.model ?? h.model ?? "");
      setRuntime(r.runtime ?? h.runtime ?? "");
      setMessages([]);
      // Resuming restarts the session, so this chat is generating again.
      setStreaming(true);
      markRunning(r.id, true);
      attachStream(r.id, true); // replay rebuilds the whole thread
      refreshHistory();
    } catch {
      setViewing(h); // agent deleted / daemon refuses: read-only
    }
  };

  /** t200: the rail reads this, not `history` directly. While the pointer is
   *  inside the rail the rows keep their positions (live flags still update —
   *  the running dot is the reason a row climbed, so it must stay truthful);
   *  the pending order lands as soon as the pointer leaves. Rows that appear
   *  while held sort after the held ones, so nothing jumps above the cursor. */
  /** t206: the hold is only worth explaining when something is really waiting.
   *  Showing "the list is holding its order" with nothing to apply would claim
   *  a pending update that does not exist — the one thing the notice must not
   *  do. So it appears exactly when the fresh order differs from the held one. */
  const holdPending = useMemo(() => {
    if (!heldOrder) return false;
    const ids = (history ?? []).map((h) => h.id);
    if (ids.length !== heldOrder.length) return true;
    return ids.some((id, i) => heldOrder[i] !== id);
  }, [history, heldOrder]);

  const railOrdered = useMemo(() => {
    const list = history ?? [];
    if (!heldOrder) return list;
    const rank = new Map(heldOrder.map((id, i) => [id, i]));
    return [...list].sort(
      (a, b) =>
        (rank.get(a.id) ?? Number.MAX_SAFE_INTEGER) - (rank.get(b.id) ?? Number.MAX_SAFE_INTEGER),
    );
  }, [history, heldOrder]);

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
    const filtered = railOrdered.filter((h) => {
      // A chat with no messages is not a session. The panel only ever creates
      // one when a message is sent (ensureChat has exactly two call sites:
      // send and retry), so a message-less chat can only come from calling the
      // API directly — a test or script residue. Hidden BY RULE, so the next
      // residue never reaches the user instead of relying on someone
      // remembering to clean up.
      if (!h.message_count) return false;
      // t178: MEMBERSHIP, not a toggle — a stray flag cannot flip it back on.
      // The test asks whether the session is KNOWN to be archived, not whether
      // it is present in a set. Absence from ?archived=exclude means one of two
      // things — archived, or not yet picked up by the indexer — and reading it
      // as archived hid every freshly created conversation until the index
      // caught up. On a fresh data root that is always the case, which is why
      // the e2e chat round-trip failed deterministically: its own conversation
      // was the one being hidden. An unknown session is therefore shown; only a
      // session the index marks archived is hidden. A chat with no session_key
      // cannot be archived at all, so it stays.
      if (h.session_key && index[h.session_key]?.archived) return false;
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

  // A failed agent load used to fall through to a spinner or an empty
  // picker. It is a page-level error with a retry (MASTER §12 row 20).
  //
  // t171: this branch keeps the route's own heading. The row-20 probe waits for
  // a ready signal of「sider 在 ∧ 本页有 h1」— and row 24 owes every route
  // exactly one h1 — so a branch that returned ONLY an ErrorState left the page
  // with h1 = 0. Measured with this route's endpoints 500ing: h1 = 0, the probe
  // never became ready, and row 20 then reported a white screen it had never
  // measured (the tool's own throw is reported separately). The heading is part
  // of the view in every state, error included.
  if (agentsError) {
    return (
      <div className="chat-wrap">
        <div className="view-bar">
          <h1 className="sr-only micro">{t("chat.title")}</h1>
          <h2>{t("chat.title")}</h2>
        </div>
        <ErrorState
          title={t("common.offline")}
          hint={String(agentsError)}
          onRetry={() => {
            // t171: the page-level retry re-runs EVERY read this view owns, not
            // just the one that failed first. Measured with agents + chats both
            // 500ing: retrying only the agents load brought the rail back with
            // its own error still on screen (1 affordance left), so the page
            // never reached a clean state.
            loadAgents();
            refreshHistory(true);
          }}
          retryLabel={t("task.retryRun")}
        />
      </div>
    );
  }
  if (!agents) {
    return (
      <div className="chat-wrap">
        <div className="view-bar">
          <h1 className="sr-only micro">{t("chat.title")}</h1>
          <h2>{t("chat.title")}</h2>
        </div>
        <Spinner label={`${t("chat.title")}…`} />
      </div>
    );
  }

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
        {archivedOpen ? (
          /* t178: disabled rather than hidden — the reason has to be readable
             next to the control it explains, and the way back is right here. */
          <div className="chat-archived-note">
            <span className="muted">{t("chat.archivedCantChat")}</span>
            <Button
              size="small"
              loading={unarchiving === archivedOpen}
              onClick={() => {
                void unarchive(archivedOpen);
              }}
            >
              {t("sessions.unarchive")}
            </Button>
          </div>
        ) : null}
        <div className="composer-row">
          <textarea
            ref={inputRef}
            rows={2}
            aria-label={t("chat.inputPh")}
            disabled={!!archivedOpen}
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
              disabled={streaming || starting || !input.trim() || !!archivedOpen}
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
    <div
      className={`chat-layout${railCollapsed ? " rail-collapsed" : ""}`}
      /* null until the user drags: an untouched column must keep the CSS
         default byte-for-byte, so no inline style is emitted at rest. */
      style={
        chatRail.width === null
          ? undefined
          : ({ "--chat-rail-w": `${chatRail.width}px` } as unknown as CSSProperties)
      }
    >
      {/* S2: the splitter is a DIRECT child of .chat-layout - that is the
          scope t144's rule targets (.chat-layout > .resize-handle), and an
          absolutely positioned child is not a grid item, so it adds no
          implicit track and eats no gap. */}
      <ResizeHandle label={t("sider.resizeChat")} {...chatRail.handleProps} />
      <aside
        className={`chat-side${sideOpen ? " open" : ""}`}
        // t200: reading the list must not be a race with the list. Held here
        // rather than on the list alone so scrolling the head counts too.
        onPointerEnter={() => setHeldOrder((history ?? []).map((h) => h.id))}
        onPointerLeave={() => setHeldOrder(null)}
      >
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
          {/* S1 (README:400): the chat rail's collapse entry. Before this the
              ONLY entry was .chat-side-toggle, which is 0x0 on desktop and can
              only ever OPEN — the user's report. Same icon pair, same 32x32 box
              and the same aria naming as the nav rail (t133): the name states
              the action for the current state. On narrow the rail is a drawer,
              so the same button also closes it. */}
          <Tooltip title={t("common.toggleSidebar")}>
            <Button
              size="small"
              type="text"
              className="icon-btn icon-btn-lg chat-rail-toggle"
              onClick={() => {
                setRailCollapsed((v) => !v);
                setSideOpen(false);
              }}
              aria-label={railCollapsed ? t("sider.expand") : t("sider.collapse")}
            >
              <Icon name={railCollapsed ? "panelLeftOpen" : "panelLeftClose"} size={14} />
            </Button>
          </Tooltip>
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
              {/* t171 row 23: .chat-group-more is the audit's EXPANDER selector
                  — the class has to mean「这是展开控件」. The panel's entries are
                  not expanders (they dismiss the panel), so they no longer
                  carry it; they are antd text buttons, which is the same
                  borderless look the class used to give them. Measured before:
                  .chat-group-more 2/3 (the two entries matched the expander
                  selector without an aria-expanded). */}
              <Button
                type="text"
                size="small"
                aria-pressed={agentFilter === null}
                onClick={() => {
                  setAgentFilter(null);
                  setFilterOpen(false);
                }}
              >
                {t("chat.allAgents")}
              </Button>
              {agents.filter(isRoleAgent).map((a) => (
                <Button
                  key={a.name}
                  type="text"
                  size="small"
                  aria-pressed={agentFilter === a.name}
                  onClick={() => {
                    setAgentFilter(a.name);
                    setFilterOpen(false);
                  }}
                >
                  {a.name}
                </Button>
              ))}
            </div>
          ) : null}
        </div>
        <div className="chat-side-list">
          {/* t206: while the pointer holds the rail AND the order is actually
              pending, the line the user is already looking at explains why the
              list is not moving. Same element, same 16px line — no new row, no
              shift under the cursor — and `.truncated` keeps the sentence to
              one line (the full text rides in the tooltip). In the normal path
              nothing changes: the class is not added and the label is the label. */}
          <div
            className={`chat-ws-title zone-title${holdPending ? " truncated" : ""}`}
            title={holdPending ? t("chat.railHold") : undefined}
          >
            {holdPending ? t("chat.railHold") : t("chat.workspaces")}
          </div>
          {historyError ? (
            <ErrorState
              title={t("common.offline")}
              hint={String(historyError)}
              onRetry={() => refreshHistory(true)}
              retryLabel={t("task.retryRun")}
            />
          ) : history === null ? (
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
              <Button type="text" size="small" onClick={() => setAgentFilter(null)}>
                {t("chat.clearFilter")}
              </Button>
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
                      <div
                        key={h.id}
                        className="row chat-session-row"
                        data-chat-id={h.id}
                        data-session-key={h.session_key ?? ""}
                      >
                        <button
                          className={`row-btn chat-session grow${h.id === chatId ? " selected" : ""}`}
                          onClick={() => openPast(h)}
                          title={label}
                        >
                          {/* t183: the rail marks EVERY chat with a reply in
                              flight, not just the one on screen — otherwise
                              switching away hides who is still working. */}
                          {h.generating || runningIds.includes(h.id) ? (
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
            /* t178 — an ENTRY to the archived destination, not a disclosure
               that mixes the archived rows back into this list. It opens a
               dialog which reads the archived set itself; the rail's own list
               is `?archived=exclude`. (t171's row-23 lesson still holds: the
               class marks an expander, so it carries aria-expanded — here it
               tracks whether the destination is showing.) */
            <button
              className="chat-group-more"
              aria-expanded={archiveOpen}
              aria-haspopup="dialog"
              aria-label={t("sessions.archivedEntry", { n: archivedCount })}
              onClick={openArchive}
            >
              {t("sessions.archivedEntry", { n: archivedCount })}
            </button>
          ) : null}
          <Modal
            open={archiveOpen}
            onCancel={() => setArchiveOpen(false)}
            footer={null}
            title={t("sessions.archivedTitle")}
            width={520}
          >
            <p className="muted micro">{t("sessions.archivedNote")}</p>
            {archivedErr ? (
              <ErrorState
                title={t("common.offline")}
                hint={String(archivedErr)}
                onRetry={openArchive}
                retryLabel={t("task.retryRun")}
              />
            ) : archivedRows === null ? (
              <Spinner />
            ) : archivedRows.length === 0 ? (
              <p className="muted">{t("sessions.archivedEmpty")}</p>
            ) : (
              <div className="chat-archived-list">
                {archivedRows.map((s) => (
                  /* data-session-key: the restore path is verified BY KEY.
                     t170 lost two rows to title-based location (titles
                     collide), so the archived destination names its rows. */
                  <div
                    key={s.key}
                    className="row chat-session-row"
                    data-session-key={s.key}
                  >
                    <span className="title muted truncated">
                      {s.title || s.preview || t("sessions.untitled")}
                    </span>
                    <span className="grow" />
                    <Button
                      size="small"
                      loading={unarchiving === s.key}
                      title={t("sessions.unarchiveHint")}
                      onClick={() => {
                        void unarchive(s.key);
                      }}
                    >
                      {t("sessions.unarchive")}
                    </Button>
                  </div>
                ))}
              </div>
            )}
          </Modal>
        </div>
      </aside>
      {sideOpen && <div className="chat-side-backdrop" onClick={() => setSideOpen(false)} />}

      <div
        className={`chat-wrap${!viewing && messages.length === 0 ? " is-empty" : ""}`}
      >
      <div className="view-bar">
        {/* t134 item 7: the DUPLICATE collapse entry used to live here
            (.view-bar .chat-rail-toggle, chat.toggleRail). It is removed: the
            chat rail's one and only collapse entry is the head control below
            (32x32, sider.collapse / sider.expand), so at 1440 exactly one
            entry is visible. The narrow-screen frozen .chat-side-toggle stays. */}
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
        {/* The other half of S1: when the desktop rail is collapsed the head
            button is gone with it (the rail is display:none), so the expand
            path has to live outside the rail. This is the frozen
            .chat-side-toggle: it already opens the narrow drawer; it now also
            restores the desktop rail, and it becomes VISIBLE on desktop while
            collapsed (inline display is the mechanism, not decoration — at rest
            no inline style is emitted). */}
        <Button
          size="small"
          className="chat-side-toggle"
          onClick={() => {
            setSideOpen(true);
            setRailCollapsed(false);
          }}
          style={railCollapsed ? { display: "inline-flex" } : undefined}
          title={railCollapsed ? t("sider.expand") : t("chat.history")}
          aria-label={railCollapsed ? t("sider.expand") : t("chat.history")}
        >
          <Icon name={railCollapsed ? "panelLeftOpen" : "history"} size={13} />
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
