// App shell: antd Layout sidebar + hash routing + theme/lang toggles.

import { useEffect, useState } from "react";
import {
  Layout,
  Menu,
  Button,
  Tooltip,
} from "antd";
import {
  HomeOutlined,
  MessageOutlined,
  AppstoreOutlined,
  CloudOutlined,
  BookOutlined,
  ShareAltOutlined,
  RobotOutlined,
  BarChartOutlined,
  InboxOutlined,
} from "@ant-design/icons";
import { api } from "./api";
import { useI18n } from "./i18n";
import { ThemeProvider, useThemeMode } from "./theme";
import { ToastBridge } from "./ui";
import { Board } from "./views/Board";
import { Home } from "./views/Home";
import { TaskDetail } from "./views/TaskDetail";
import { Memory } from "./views/Memory";
import { Knowledge } from "./views/Knowledge";
import { Graph } from "./views/Graph";
import { Agents, Inbox, Stats } from "./views/Agents";
import { Chat } from "./views/Chat";

const { Sider, Content } = Layout;

type View =
  | { kind: "home" }
  | { kind: "chat"; agent?: string }
  | { kind: "board" }
  | { kind: "task"; id: string }
  | { kind: "memory" }
  | { kind: "knowledge" }
  | { kind: "graph" }
  | { kind: "agents" }
  | { kind: "stats" }
  | { kind: "inbox" };

function parseHash(): View {
  const h = window.location.hash.replace(/^#/, "");
  const mTask = h.match(/^task\/([\w-]+)/);
  if (mTask) return { kind: "task", id: mTask[1] };
  const mChat = h.match(/^chat(?:\?agent=([\w-]+))?/);
  if (mChat) return { kind: "chat", agent: mChat[1] };
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
    case "stats":
      return { kind: "stats" };
    case "inbox":
      return { kind: "inbox" };
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

  useEffect(() => {
    const apply = () => setView(parseHash());
    apply();
    window.addEventListener("hashchange", apply);
    return () => window.removeEventListener("hashchange", apply);
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

  const items = [
    {
      type: "group" as const,
      label: t("nav.group.work"),
      children: [
        { key: "home", icon: <HomeOutlined />, label: t("nav.home") },
        { key: "chat", icon: <MessageOutlined />, label: t("chat.title") },
        { key: "board", icon: <AppstoreOutlined />, label: t("nav.board") },
      ],
    },
    {
      type: "group" as const,
      label: t("nav.group.knowledge"),
      children: [
        { key: "memory", icon: <CloudOutlined />, label: t("nav.memory") },
        { key: "knowledge", icon: <BookOutlined />, label: t("nav.knowledge") },
        { key: "graph", icon: <ShareAltOutlined />, label: t("nav.graph") },
      ],
    },
    {
      type: "group" as const,
      label: t("nav.group.system"),
      children: [
        { key: "agents", icon: <RobotOutlined />, label: t("nav.agents") },
        { key: "stats", icon: <BarChartOutlined />, label: t("nav.stats") },
        {
          key: "inbox",
          icon: <InboxOutlined />,
          label: (
            <span style={{ display: "inline-flex", alignItems: "center", gap: 8 }}>
              {t("nav.inbox")}
              {inboxCount > 0 && (
                <span className="nav-badge">{inboxCount}</span>
              )}
            </span>
          ),
        },
      ],
    },
  ];

  return (
    <Layout style={{ minHeight: "100vh" }}>
      <ToastBridge />
      <Sider width={228} style={{ position: "sticky", top: 0, height: "100vh", overflow: "auto" }}>
        <div
          className="brand"
          onClick={() => nav("")}
          role="button"
          tabIndex={0}
          onKeyDown={(e) => e.key === "Enter" && nav("")}
        >
          <span className="brand-mark">ru</span>
          <span className="brand-name">ruagent</span>
        </div>
        <Menu
          mode="inline"
          items={items}
          selectedKeys={[selected]}
          onClick={({ key }) => nav(key === "home" ? "" : key)}
          style={{ borderInlineEnd: "none", paddingBlock: 4 }}
        />
        <div className="sidebar-foot">
          <span className={`conn ${daemonUp ? "ok" : "err"}`}>
            ● {daemonUp ? t("common.online") : t("common.offline")}
          </span>
          <span className="build-id" title="panel build">b {__BUILD_ID__}</span>
          <span className="grow" />
          <Tooltip title={mode === "dark" ? t("theme.light") : t("theme.dark")}>
            <Button
              size="small"
              type="text"
              onClick={toggle}
              aria-label="toggle theme"
            >
              {mode === "dark" ? "☀" : "☾"}
            </Button>
          </Tooltip>
          <Button size="small" type="text" onClick={() => setLang(lang === "zh" ? "en" : "zh")} title="中文 / EN">
            {lang === "zh" ? "EN" : "中"}
          </Button>
        </div>
      </Sider>
      <Layout>
        <Content className="content">
          {view.kind === "home" && (
            <Home
              onOpenTask={(id) => nav(`task/${id}`)}
              onNav={(hash) => nav(hash || "board")}
            />
          )}
          {view.kind === "chat" && <Chat initialAgent={view.agent} />}
          {view.kind === "board" && <Board onOpen={(id) => nav(`task/${id}`)} />}
          {view.kind === "task" && (
            <TaskDetail key={view.id} id={view.id} onBack={() => nav("board")} />
          )}
          {view.kind === "memory" && <Memory />}
          {view.kind === "knowledge" && <Knowledge />}
          {view.kind === "graph" && <Graph />}
          {view.kind === "agents" && <Agents />}
          {view.kind === "stats" && <Stats />}
          {view.kind === "inbox" && <Inbox />}
        </Content>
      </Layout>
    </Layout>
  );
}
