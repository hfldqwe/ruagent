// App shell: antd Layout sidebar + hash routing + theme/lang toggles.

import {
  Suspense,
  lazy,
  startTransition,
  useEffect,
  useState,
  type ReactNode,
} from "react";
import {
  Alert,
  Badge,
  Layout,
  Menu,
  Button,
  Tooltip,
} from "antd";
import { api } from "./api";
import { Icon } from "./icons";
import { useI18n } from "./i18n";
import { ThemeProvider, useThemeMode } from "./theme";
import { ResizeHandle, Spinner, ToastBridge, useSidebarResize } from "./ui";

// Route-level code splitting (row 27): every view is its own chunk, fetched
// the first time its hash is visited. The shell — sider, brand, palette,
// toasts, offline banner — stays in the entry chunk, so the layout and the
// frozen .app-sider / .kbd-hint / .brand-* selectors are never behind the
// fallback. Board and Agents export more than one route component; both
// lazy() calls resolve the same chunk, so nothing is fetched twice.
// t169: the same factories, kept as a MAP so the shell can WARM a module before
// it is rendered. Measured on this machine: the FIRST import of a route module
// costs a ~320ms Suspense fallback even though its own fetch is ~2ms and the
// view's first API call is ~2ms; the SECOND visit to the same route is ~30ms.
// That 320ms is what "页面切换有明显的加载" is. Warming fetches exactly the same
// chunks, just earlier (nav hover/focus, then idle) — the entry size and the
// per-route split (row 27) are unchanged, only the click path stops paying.
const load = {
  board: () => import("./views/Board").then((m) => ({ default: m.Board })),
  createTask: () => import("./views/Board").then((m) => ({ default: m.CreateTaskModal })),
  home: () => import("./views/Home").then((m) => ({ default: m.Home })),
  task: () => import("./views/TaskDetail").then((m) => ({ default: m.TaskDetail })),
  memory: () => import("./views/Memory").then((m) => ({ default: m.Memory })),
  knowledge: () => import("./views/Knowledge").then((m) => ({ default: m.Knowledge })),
  graph: () => import("./views/Graph").then((m) => ({ default: m.Graph })),
  agents: () => import("./views/Agents").then((m) => ({ default: m.Agents })),
  inbox: () => import("./views/Agents").then((m) => ({ default: m.Inbox })),
  stats: () => import("./views/Agents").then((m) => ({ default: m.Stats })),
  runtimes: () => import("./views/Runtimes").then((m) => ({ default: m.Runtimes })),
  settings: () => import("./views/Settings").then((m) => ({ default: m.Settings })),
  chat: () => import("./views/Chat").then((m) => ({ default: m.Chat })),
  // Warm-only, and deliberately NOT used by a lazy() above: #sessions' chunk
  // sits one level deeper (App imports Sessions statically, and views/Sessions
  // itself lazy-loads SessionsView), so the shell warms that module directly.
  // Fetching it is not extra work — it is exactly the chunk #sessions needs.
  sessions: () => import("./views/SessionsView"),
};
/** Warm a route's chunk without rendering it. Idempotent and free when the
 *  module is already registered, so calling it on every hover is safe. */
const warm = (key: keyof typeof load) => {
  const start = load[key];
  if (start) void start();
};
const Board = lazy(load.board);
const CreateTaskModal = lazy(load.createTask);
const Home = lazy(load.home);
const TaskDetail = lazy(load.task);
const Memory = lazy(load.memory);
const Knowledge = lazy(load.knowledge);
const Graph = lazy(load.graph);
const Agents = lazy(load.agents);
const Inbox = lazy(load.inbox);
const Stats = lazy(load.stats);
const Runtimes = lazy(load.runtimes);
const Settings = lazy(load.settings);
const Chat = lazy(load.chat);
// The palette stays in the entry ON PURPOSE: it is a keyboard-first surface,
// and every attempt to make it a chunk was measured worse — the chunk was not
// ready when Ctrl+K arrived, which loses the keystrokes typed straight after
// it and breaks e2e/palette.spec.ts:65 ("Enter executes a navigation
// command", which does not wait for .cmdk before typing). 5.2KB is not worth
// a hotkey that races the network.
import { CommandPalette } from "./CommandPalette";
// Sessions is split differently: its module is also imported for the pure
// SOURCE_LABEL / sourceHue / msToIso helpers (CommandPalette, Home, Chat), so
// the module itself exports a lazy route component and stays tiny.
import { Sessions } from "./views/Sessions";

const { Sider, Content } = Layout;

type View =
  | { kind: "home" }
  | { kind: "chat"; agent?: string }
  | { kind: "sessions" }
  | { kind: "board" }
  | { kind: "task"; id: string }
  | { kind: "memory" }
  | { kind: "knowledge" }
  | { kind: "graph" }
  | { kind: "agents" }
  | { kind: "runtimes" }
  | { kind: "stats" }
  | { kind: "settings" }
  | { kind: "inbox" };

// MASTER §12 row 56: every route's <title> must identify that route and the 13
// titles must be pairwise distinct. The identifier is the route's i18n display
// name — the same string the nav shows — never a hardcoded literal (so the
// title follows the language toggle like every other label) and never the bare
// product name, which is what all 13 routes used to share.
// Keyed by View["kind"], so adding a route without a title key is a type error.
const ROUTE_TITLE_KEY: Record<View["kind"], string> = {
  home: "nav.home",
  chat: "chat.title",
  sessions: "sessions.title",
  board: "nav.board",
  memory: "nav.memory",
  knowledge: "nav.knowledge",
  graph: "nav.graph",
  agents: "nav.agents",
  runtimes: "nav.runtimes",
  stats: "nav.stats",
  settings: "nav.settings",
  inbox: "nav.inbox",
  task: "nav.task",
};

// ---------------------------------------------------------------------------
// Sidebar geometry (S1/S2). The dragged WIDTH is ui.tsx's business (the hook
// owns "ruagent.sidebar.<id>"); what the shell owns is the collapse preference.
// ---------------------------------------------------------------------------

/** The other resident column's default width — the second term of the S2 max
 *  formula (max = min(2 x def, viewport - other - 390)). t134 swaps this
 *  constant for the live chat-rail width once that rail is resizable. */
const CHAT_RAIL_DEFAULT = 248;

/** The nav rail's explicit collapse choice, or null when the user never made
 *  one. Visiting must never write this (S1): the default comes from the frozen
 *  lg breakpoint, so "just looking at the app" cannot rewrite a default. */
const COLLAPSE_KEY = "ruagent.sidebar.collapsed";
function readStoredCollapsed(): boolean | null {
  try {
    const raw = window.localStorage.getItem(COLLAPSE_KEY);
    return raw === null ? null : raw === "true";
  } catch {
    return null; // private mode: the preference is a nicety, never a gate
  }
}
function writeStoredCollapsed(v: boolean) {
  try {
    window.localStorage.setItem(COLLAPSE_KEY, String(v));
  } catch {
    /* see readStoredCollapsed */
  }
}

// MASTER §12 row 58: the URL carries the view state, and this app routes on the
// hash — so the hash is `route[?query]` (e.g. "#sessions?src=dsh&sys=only").
// Splitting the query off BEFORE routing is what makes that reachable: every
// arm below matches the PATH only, so a suffix can no longer fall through to
// the fallback view (which is exactly what "#sessions?src=dsh" used to do, and
// why the sessions filter had to be smuggled into the real query instead).
// The fallback still owns every unknown PATH — appending a query never turns an
// unknown route into a valid one.
export type Route = { view: View; query: URLSearchParams };

/** Parse `route[?query]` out of a hash. Pure: it reads no history and writes
 *  none, so it can never add or rewrite an entry (a cleanup that rewrites the
 *  entry the browser just created is what breaks Back: a traversal fires
 *  popstate WITHOUT hashchange, so the URL moves and the page does not). */
export function parseHash(raw: string = window.location.hash): Route {
  const h = raw.replace(/^#/, "");
  const q = h.indexOf("?");
  const path = q === -1 ? h : h.slice(0, q);
  const query = new URLSearchParams(q === -1 ? "" : h.slice(q + 1));
  return { view: viewOf(path, query), query };
}

function viewOf(path: string, query: URLSearchParams): View {
  const mTask = path.match(/^task\/([\w-]+)/);
  if (mTask) return { kind: "task", id: mTask[1] };
  // Same prefix semantics as before (the old regex was unanchored, so "chatty"
  // also matched "chat"); the `agent` value now comes from the parsed query,
  // which is where it always belonged — but its GRAMMAR is preserved verbatim.
  // The old parser matched /[\w-]+/ against the raw hash, so it stopped at the
  // first non-word character and reported an empty value as "absent". Both
  // quirks stay: this change's contract is bit-identical parsing of the forms
  // that already existed (verified over a 32-hash table), and widening the
  // agent-value grammar is a separate decision. It is never hit in practice —
  // every agent id in this repo is [\w-]+.
  if (path.startsWith("chat")) return { kind: "chat", agent: (query.get("agent") ?? "").match(/^[\w-]+/)?.[0] };
  if (path === "sessions") return { kind: "sessions" };
  switch (path) {
    case "board":
      return { kind: "board" };
    case "memory":
      return { kind: "memory" };
    case "knowledge":
      return { kind: "knowledge" };
    case "graph":
      return { kind: "graph" };
    case "agents":
      return { kind: "agents" };
    case "runtimes":
      return { kind: "runtimes" };
    case "stats":
      return { kind: "stats" };
    case "inbox":
      return { kind: "inbox" };
    case "settings":
      return { kind: "settings" };
    default:
      return { kind: "home" };
  }
}

export default function App() {
  return (
    <ThemeProvider>
      <Shell />
    </ThemeProvider>
  );
}

function Shell() {
  const { lang, setLang, t } = useI18n();
  const { mode, toggle } = useThemeMode();
  const [route, setRoute] = useState<Route>(() => parseHash());
  const view = route.view;
  // `null` = the queue has not been read yet. The count lives HERE and only
  // here (t50): Home consumes it as a prop instead of polling the same
  // endpoint a second time. `null` vs `0` stays distinguishable, because
  // "no pending permissions" and "cannot reach the daemon" are different
  // facts (MASTER §12 row 20).
  const [inboxCount, setInboxCount] = useState<number | null>(null);
  const [daemonUp, setDaemonUp] = useState(true);
  const [cmdk, setCmdk] = useState(false);
  const [creating, setCreating] = useState(false);
  // Auto-collapse below lg (frozen breakpoint); the footer toggle keeps manual
  // control on desktop too. MatchMedia drives the automatic path, so resizing
  // the window adapts live; an EXPLICIT choice is persisted (S1) and therefore
  // survives a reload. The automatic path deliberately does not write: only the
  // user may overwrite the default.
  const [collapsed, setCollapsed] = useState(
    () => readStoredCollapsed() ?? window.matchMedia("(max-width: 992px)").matches,
  );
  const toggleCollapsed = () => {
    const next = !collapsed;
    setCollapsed(next);
    writeStoredCollapsed(next);
  };

  // S2: the nav rail is the resizable column. Dragging is off while the rail is
  // collapsed (there is nothing to widen) and below 1024 (the hook's single
  // viewport gate, mirrored by CSS through the same aria-disabled attribute).
  const navRail = useSidebarResize({
    id: "nav",
    def: 228,
    other: CHAT_RAIL_DEFAULT,
    enabled: !collapsed,
  });

  // MASTER §12 row 56: before this every route inherited the static
  // <title>ruagent</title> from index.html, so a tab or a bookmark said nothing
  // about where you were. The title is derived from the route's display name, so
  // switching the language re-titles the tab too. The dependency is `lang` and
  // not `t`: `t` is rebuilt on every render and would re-run this each time.
  useEffect(() => {
    document.title = `${t(ROUTE_TITLE_KEY[view.kind])} · ruagent`;
  }, [view.kind, lang]);

  // t169: a route change is not urgent, and as a TRANSITION a view that is
  // still loading keeps the previous view on screen instead of blanking the
  // content area to the fallback. Measured with a 400ms chunk delay: the
  // fallback used to be visible for the whole delay; after this it never
  // appears (the old view stays until the new one is ready). The URL still
  // updates immediately — the hash is the browser's, not ours.
  useEffect(() => {
    const apply = () => startTransition(() => setRoute(parseHash()));
    apply();
    window.addEventListener("hashchange", apply);
    return () => window.removeEventListener("hashchange", apply);
  }, []);

  // t169: once the app has painted, warm every route chunk at idle, so the
  // FIRST click on a route costs the same as a revisit (measured 320ms -> ~30ms
  // on the hover/focus path, and the idle pass covers a touch user, who never
  // hovers). requestIdleCallback is not universal, hence the timer fallback.
  useEffect(() => {
    const keys = Object.keys(load) as (keyof typeof load)[];
    const idle: (fn: () => void) => number =
      typeof window.requestIdleCallback === "function"
        ? (fn) => window.requestIdleCallback(fn)
        : (fn) => window.setTimeout(fn, 300);
    const cancel: (h: number) => void =
      typeof window.cancelIdleCallback === "function"
        ? (h) => window.cancelIdleCallback(h)
        : (h) => window.clearTimeout(h);
    const h = idle(() => keys.forEach((k) => warm(k)));
    return () => cancel(h);
  }, []);

  // Row 58's other half: the query is route state, and a view is a separate
  // chunk that must not import the shell (that would pull the shell into the
  // view's chunk and undo the route-level splitting). The shell therefore
  // publishes the parsed query on <html>, next to the `lang` the i18n provider
  // already writes there; a view reads
  // `document.documentElement.dataset.routeQuery` and parses it with
  // URLSearchParams. Absent = "no query", so the default URL stays clean and a
  // reader can never pick up a stale value from the previous route. This writes
  // no history entry — see parseHash's note on Back.
  useEffect(() => {
    const root = document.documentElement;
    const q = route.query.toString();
    if (q) root.dataset.routeQuery = q;
    else delete root.dataset.routeQuery;
  }, [route]);

  useEffect(() => {
    const mq = window.matchMedia("(max-width: 992px)");
    const onChange = () => setCollapsed(mq.matches);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);

  // The permission queue has exactly ONE 2s poller at a time (MASTER §12 行 28,
  // judged cap = 7 per 11.5s = one 2s poller; the shell's copy used to run
  // alongside the view's and produced 14). While #inbox is mounted the view
  // owns the request — it renders the same list the badge counts, so the badge
  // has nothing to add; leaving #inbox re-runs this effect and polls at once.
  //
  // This is also the ONLY reader of the queue: #home renders the count it
  // receives as a prop, so the sider badge and the home todo card can never
  // disagree, and the route spends 6 of the 7 judged requests instead of 7.
  const inboxRoute = view.kind === "inbox";
  useEffect(() => {
    if (inboxRoute) return;
    let alive = true;
    const poll = () =>
      api
        .pendingPermissions()
        .then((p) => {
          if (!alive) return;
          setInboxCount(p.length);
          setDaemonUp(true);
        })
        .catch(() => alive && setDaemonUp(false));
    poll();
    const i = setInterval(() => {
      // A hidden tab asks for nothing (行 28 同源).
      if (!document.hidden) poll();
    }, 2000);
    const onVisible = () => {
      if (!document.hidden) poll();
    };
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      alive = false;
      clearInterval(i);
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, [inboxRoute]);

  const nav = (hash: string) => {
    window.location.hash = hash;
  };

  const selected =
    view.kind === "task" ? "board" : view.kind === "home" ? "home" : view.kind;

  // MASTER §12 row 57: a nav item must be a real anchor carrying the route's
  // hash. Before this the item was a <li> with an onClick, so Cmd+click,
  // middle-click and "open in new tab" did nothing at all. The anchor wraps the
  // label; the item keeps its own onClick, so keyboard activation and clicks on
  // the item's padding still navigate, while the browser — not React — owns the
  // new-tab path. An anchor takes the shared focus ring from the
  // :where(a[href], …):focus-visible rule (row 16 unaffected), and an inline
  // anchor is exempt from row 18's hit-target floor.
  // t169: hovering or focusing a nav item warms its chunk. The anchor covers
  // the whole item (antd gives a menu link a full-bleed ::before), so hovering
  // anywhere in the row — not just the 28px label — is an intent signal.
  const link = (key: string, text: ReactNode) => (
    <a
      className="nav-link"
      href={`#${key}`}
      onMouseEnter={() => warm(key as keyof typeof load)}
      onFocus={() => warm(key as keyof typeof load)}
    >
      {text}
    </a>
  );

  const navItems = [
    { key: "home", icon: <Icon name="home" size={16} />, label: link("home", t("nav.home")) },
    { key: "chat", icon: <Icon name="chat" size={16} />, label: link("chat", t("chat.title")) },
    { key: "sessions", icon: <Icon name="history" size={16} />, label: link("sessions", t("sessions.title")) },
    { key: "board", icon: <Icon name="grid" size={16} />, label: link("board", t("nav.board")) },
    { key: "memory", icon: <Icon name="cloud" size={16} />, label: link("memory", t("nav.memory")) },
    { key: "knowledge", icon: <Icon name="book" size={16} />, label: link("knowledge", t("nav.knowledge")) },
    { key: "graph", icon: <Icon name="graph" size={16} />, label: link("graph", t("nav.graph")) },
    { key: "agents", icon: <Icon name="bot" size={16} />, label: link("agents", t("nav.agents")) },
    { key: "runtimes", icon: <Icon name="layers" size={16} />, label: link("runtimes", t("nav.runtimes")) },
    { key: "stats", icon: <Icon name="stats" size={16} />, label: link("stats", t("nav.stats")) },
    { key: "settings", icon: <Icon name="settings" size={16} />, label: link("settings", t("nav.settings")) },
    {
      key: "inbox",
      icon: collapsed ? (
        <Badge count={inboxCount ?? 0} size="small" offset={[4, -4]}>
          <Icon name="inbox" size={16} />
        </Badge>
      ) : (
        <Icon name="inbox" size={16} />
      ),
      label: link(
        "inbox",
        <span style={{ display: "inline-flex", alignItems: "center", gap: 8 }}>
          {t("nav.inbox")}
          {inboxCount !== null && inboxCount > 0 && (
            <span className="nav-badge">{inboxCount}</span>
          )}
        </span>,
      ),
    },
  ];

  // Collapsed rail: flat list (group titles have nowhere to live at 72px),
  // antd shows the label as a hover tooltip automatically.
  const items = collapsed
    ? navItems
    : [
        {
          type: "group" as const,
          label: t("nav.group.work"),
          children: navItems.slice(0, 4),
        },
        {
          type: "group" as const,
          label: t("nav.group.knowledge"),
          children: navItems.slice(4, 7),
        },
        {
          type: "group" as const,
          label: t("nav.group.system"),
          children: navItems.slice(7),
        },
      ];

  return (
    <Layout style={{ minHeight: "100vh" }}>
      <ToastBridge />
      <CommandPalette open={cmdk} onOpenChange={setCmdk} nav={nav} onNewTask={() => setCreating(true)} />
      <Sider
        width={navRail.width ?? 228}
        collapsedWidth={72}
        collapsed={collapsed}
        trigger={null}
        className="app-sider"
      >
        <div className="sider-inner">
        {/* Row 57: the brand IS a navigation entry point (it goes home), so it
            is a real <a href> like every nav item — Cmd/middle click and "open
            in new tab" work, the browser (not React) owns the new-tab path, and
            it takes the shared focus ring from the
            :where(a[href], …):focus-visible rule (row 16). The markup inside is
            deliberately untouched: .brand-mark and .brand-name are frozen e2e
            selectors. role="button", tabIndex and onKeyDown are gone because a
            real link is already focusable, Enter-activatable and announced as a
            link — keeping role="button" on an anchor would have been a lie.
            The href is "#home" and not "" so the URL says where you are (row
            58); parseHash maps both "#" and "#home" to home, so the destination
            is bit-identical. */}
        <a className="brand" href="#home" title="ruagent">
          <span className="brand-mark">ru</span>
          {!collapsed && <span className="brand-name">ruagent</span>}
        </a>
        <div className="sider-nav">
          <Menu
            mode="inline"
            items={items}
            selectedKeys={[selected]}
            onClick={({ key }) => nav(String(key))}
            style={{ borderInlineEnd: "none", paddingBlock: 4 }}
          />
        </div>
        <div className="sidebar-foot">
          <Tooltip
            title={`${daemonUp ? t("common.online") : t("common.offline")} · ${t("common.buildId", { id: __BUILD_ID__ })}`}
          >
            <span className={`conn ${daemonUp ? "ok" : "err"}`}>
              {collapsed ? "●" : `● ${daemonUp ? t("common.online") : t("common.offline")}`}
            </span>
          </Tooltip>
          {/* The compact "b <id>" stays (footer width is a hard budget:
              seven items in a 203px column), but the bare prefix must not be
              the only name this element has — screen readers and tooltips get
              the spelled-out, localized one. */}
          {!collapsed && (
            <span
              className="build-id"
              title={t("common.buildId", { id: __BUILD_ID__ })}
              aria-label={t("common.buildId", { id: __BUILD_ID__ })}
            >
              b {__BUILD_ID__}
            </span>
          )}
          {!collapsed && <span className="grow" />}
          {!collapsed && (
            <button className="kbd-hint" onClick={() => setCmdk(true)} title={t("cmd.placeholder")}>
              Ctrl K ⌘K
            </button>
          )}
          {/* S1 unified collapse control: same icon, same 32x32 box, same
              aria naming pattern as the chat rail's (t134). The name states the
              ACTION for the current state — "expand" while collapsed, "collapse"
              while expanded. */}
          <Tooltip title={t("common.toggleSidebar")}>
            <Button
              size="small"
              type="text"
              onClick={toggleCollapsed}
              aria-label={collapsed ? t("sider.expand") : t("sider.collapse")}
            >
              <Icon name={collapsed ? "panelLeftOpen" : "panelLeftClose"} size={14} />
            </Button>
          </Tooltip>
          <Tooltip title={mode === "dark" ? t("theme.light") : t("theme.dark")}>
            <Button
              size="small"
              type="text"
              onClick={toggle}
              aria-label={t("theme.toggle")}
            >
              {mode === "dark" ? <Icon name="sun" size={14} /> : <Icon name="moon" size={14} />}
            </Button>
          </Tooltip>
          <Button size="small" type="text" onClick={() => setLang(lang === "zh" ? "en" : "zh")} title="中文 / EN">
            {lang === "zh" ? "EN" : "中"}
          </Button>
        </div>
        </div>
      </Sider>
      {/* S2: the splitter is a flex SIBLING of the column it resizes. The outer
          antd Layout is already a flex row and .resize-handle stretches itself,
          so this needs no wrapper and no inline geometry. The hook hides it
          (aria-disabled -> display:none) below 1024 and while collapsed. */}
      <ResizeHandle label={t("sider.resizeNav")} {...navRail.handleProps} />
      {creating && (
        <Suspense fallback={null}>
          <CreateTaskModal
            onClose={() => setCreating(false)}
            onCreated={(id) => {
              setCreating(false);
              nav(`task/${id}`);
            }}
          />
        </Suspense>
      )}
      <Layout>
        <Content className="content">
          {!daemonUp && (
            <Alert
              type="error"
              showIcon
              banner
              className="offline-banner"
              message={t("common.offlineBanner")}
            />
          )}
          <Suspense fallback={<Spinner label={t("common.loading")} />}>
          {view.kind === "home" && (
            <Home
              inbox={inboxCount}
              onOpenTask={(id) => nav(`task/${id}`)}
              onNav={(hash) => nav(hash || "board")}
            />
          )}
          {view.kind === "chat" && <Chat initialAgent={view.agent} />}
          {view.kind === "sessions" && <Sessions />}
          {view.kind === "board" && <Board onOpen={(id) => nav(`task/${id}`)} />}
          {view.kind === "task" && (
            <TaskDetail key={view.id} id={view.id} onBack={() => nav("board")} />
          )}
          {view.kind === "memory" && <Memory />}
          {view.kind === "knowledge" && <Knowledge />}
          {view.kind === "graph" && <Graph />}
          {view.kind === "agents" && <Agents />}
          {view.kind === "runtimes" && <Runtimes />}
          {view.kind === "stats" && <Stats />}
          {view.kind === "settings" && <Settings />}
          {view.kind === "inbox" && <Inbox />}
          </Suspense>
        </Content>
      </Layout>
    </Layout>
  );
}
