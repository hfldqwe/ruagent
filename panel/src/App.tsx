// App shell: antd Layout sidebar + hash routing + theme/lang toggles.

import { useEffect, useState } from "react";
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
import { CommandPalette } from "./CommandPalette";
import { ToastBridge } from "./ui";
import { Board } from "./views/Board";
import { Home } from "./views/Home";
import { TaskDetail } from "./views/TaskDetail";
import { Memory } from "./views/Memory";
import { Knowledge } from "./views/Knowledge";
import { Graph } from "./views/Graph";
import { Agents, Inbox, Stats } from "./views/Agents";
import { Runtimes } from "./views/Runtimes";
import { Settings } from "./views/Settings";
import { Chat } from "./views/Chat";
import { Sessions } from "./views/Sessions";
import { CreateTaskModal } from "./views/Board";

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

function parseHash(): View {
  const h = window.location.hash.replace(/^#/, "");
  const mTask = h.match(/^task\/([\w-]+)/);
  if (mTask) return { kind: "task", id: mTask[1] };
  const mChat = h.match(/^chat(?:\?agent=([\w-]+))?/);
  if (mChat) return { kind: "chat", agent: mChat[1] };
  if (h === "sessions") return { kind: "sessions" };
  switch (h) {
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
  const [view, setView] = useState<View>(() => parseHash());
  const [inboxCount, setInboxCount] = useState(0);
  const [daemonUp, setDaemonUp] = useState(true);
  const [cmdk, setCmdk] = useState(false);
  const [creating, setCreating] = useState(false);
  // Auto-collapse below lg; the footer toggle keeps manual control on
  // desktop too. MatchMedia drives it so resizing the window adapts live.
  const [collapsed, setCollapsed] = useState(
    () => window.matchMedia("(max-width: 992px)").matches,
  );

  useEffect(() => {
    const apply = () => setView(parseHash());
    apply();
    window.addEventListener("hashchange", apply);
    return () => window.removeEventListener("hashchange", apply);
  }, []);

  useEffect(() => {
    const mq = window.matchMedia("(max-width: 992px)");
    const onChange = () => setCollapsed(mq.matches);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);

  useEffect(() => {
    const poll = () =>
      api
        .pendingPermissions()
        .then((p) => {
          setInboxCount(p.length);
          setDaemonUp(true);
        })
        .catch(() => setDaemonUp(false));
    poll();
    const i = setInterval(poll, 2000);
    return () => clearInterval(i);
  }, []);

  const nav = (hash: string) => {
    window.location.hash = hash;
  };

  const selected =
    view.kind === "task" ? "board" : view.kind === "home" ? "home" : view.kind;

  const navItems = [
    { key: "home", icon: <Icon name="home" size={16} />, label: t("nav.home") },
    { key: "chat", icon: <Icon name="chat" size={16} />, label: t("chat.title") },
    { key: "sessions", icon: <Icon name="history" size={16} />, label: t("sessions.title") },
    { key: "board", icon: <Icon name="grid" size={16} />, label: t("nav.board") },
    { key: "memory", icon: <Icon name="cloud" size={16} />, label: t("nav.memory") },
    { key: "knowledge", icon: <Icon name="book" size={16} />, label: t("nav.knowledge") },
    { key: "graph", icon: <Icon name="graph" size={16} />, label: t("nav.graph") },
    { key: "agents", icon: <Icon name="bot" size={16} />, label: t("nav.agents") },
    { key: "runtimes", icon: <Icon name="layers" size={16} />, label: t("nav.runtimes") },
    { key: "stats", icon: <Icon name="stats" size={16} />, label: t("nav.stats") },
    { key: "settings", icon: <Icon name="settings" size={16} />, label: t("nav.settings") },
    {
      key: "inbox",
      icon: collapsed ? (
        <Badge count={inboxCount} size="small" offset={[4, -4]}>
          <Icon name="inbox" size={16} />
        </Badge>
      ) : (
        <Icon name="inbox" size={16} />
      ),
      label: (
        <span style={{ display: "inline-flex", alignItems: "center", gap: 8 }}>
          {t("nav.inbox")}
          {inboxCount > 0 && <span className="nav-badge">{inboxCount}</span>}
        </span>
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
        width={228}
        collapsedWidth={72}
        collapsed={collapsed}
        trigger={null}
        className="app-sider"
      >
        <div className="sider-inner">
        <div
          className="brand"
          onClick={() => nav("")}
          role="button"
          tabIndex={0}
          onKeyDown={(e) => e.key === "Enter" && nav("")}
          title="ruagent"
        >
          <span className="brand-mark">ru</span>
          {!collapsed && <span className="brand-name">ruagent</span>}
        </div>
        <div className="sider-nav">
          <Menu
            mode="inline"
            items={items}
            selectedKeys={[selected]}
            onClick={({ key }) => nav(key === "home" ? "" : key)}
            style={{ borderInlineEnd: "none", paddingBlock: 4 }}
          />
        </div>
        <div className="sidebar-foot">
          <Tooltip title={`${daemonUp ? t("common.online") : t("common.offline")} · b ${__BUILD_ID__}`}>
            <span className={`conn ${daemonUp ? "ok" : "err"}`}>
              {collapsed ? "●" : `● ${daemonUp ? t("common.online") : t("common.offline")}`}
            </span>
          </Tooltip>
          {!collapsed && <span className="build-id" title="panel build">b {__BUILD_ID__}</span>}
          {!collapsed && <span className="grow" />}
          {!collapsed && (
            <button className="kbd-hint" onClick={() => setCmdk(true)} title={t("cmd.placeholder")}>
              Ctrl K ⌘K
            </button>
          )}
          <Tooltip title={t("common.toggleSidebar")}>
            <Button
              size="small"
              type="text"
              onClick={() => setCollapsed((c) => !c)}
              aria-label={t("common.toggleSidebar")}
            >
              <Icon name={collapsed ? "panelLeftOpen" : "panelLeftClose"} size={14} />
            </Button>
          </Tooltip>
          <Tooltip title={mode === "dark" ? t("theme.light") : t("theme.dark")}>
            <Button
              size="small"
              type="text"
              onClick={toggle}
              aria-label="toggle theme"
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
      {creating && (
        <CreateTaskModal
          onClose={() => setCreating(false)}
          onCreated={(id) => {
            setCreating(false);
            nav(`task/${id}`);
          }}
        />
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
          {view.kind === "home" && (
            <Home
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
        </Content>
      </Layout>
    </Layout>
  );
}
