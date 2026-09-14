// Command palette (Ctrl/Cmd+K): a quick-action layer over the whole
// panel — view navigation, per-agent chat entry, recent sessions, and
// the small set of global actions. Everything comes from data the panel
// already has; nothing new is fetched beyond a cached agents/sessions
// refresh on open.

import { useEffect, useMemo, useRef, useState } from "react";
import { api, type AgentInfo, type SessionRecord } from "./api";
import { useI18n } from "./i18n";
import { useThemeMode } from "./theme";
import { Icon } from "./icons";
import { SOURCE_LABEL } from "./views/Sessions";

type Group = "nav" | "agents" | "sessions" | "actions";

interface Cmd {
  key: string;
  group: Group;
  label: string;
  hint?: string;
  keywords?: string;
  run: () => void;
}

const NAV: { hash: string; labelKey: string; kw: string }[] = [
  { hash: "", labelKey: "nav.home", kw: "home" },
  { hash: "chat", labelKey: "chat.title", kw: "chat" },
  { hash: "sessions", labelKey: "sessions.title", kw: "sessions history" },
  { hash: "board", labelKey: "nav.board", kw: "board kanban tasks" },
  { hash: "memory", labelKey: "nav.memory", kw: "memory" },
  { hash: "knowledge", labelKey: "nav.knowledge", kw: "knowledge docs" },
  { hash: "graph", labelKey: "nav.graph", kw: "graph entities" },
  { hash: "agents", labelKey: "nav.agents", kw: "agents" },
  { hash: "stats", labelKey: "nav.stats", kw: "stats" },
  { hash: "inbox", labelKey: "nav.inbox", kw: "inbox permissions" },
];

const GROUP_ORDER: { key: Group; labelKey: string }[] = [
  { key: "nav", labelKey: "cmd.group.nav" },
  { key: "agents", labelKey: "cmd.group.agents" },
  { key: "sessions", labelKey: "cmd.group.sessions" },
  { key: "actions", labelKey: "cmd.group.actions" },
];

export function CommandPalette({
  open,
  onOpenChange,
  nav,
  onNewTask,
}: {
  open: boolean;
  onOpenChange: (v: boolean) => void;
  nav: (hash: string) => void;
  onNewTask: () => void;
}) {
  const { t, lang, setLang } = useI18n();
  const { mode, toggle } = useThemeMode();
  const [q, setQ] = useState("");
  const [sel, setSel] = useState(0);
  const [agents, setAgents] = useState<AgentInfo[] | null>(null);
  const [sessions, setSessions] = useState<SessionRecord[] | null>(null);
  /** shown instantly on open, refreshed in the background */
  const cache = useRef<{ agents: AgentInfo[] | null; sessions: SessionRecord[] | null }>({
    agents: null,
    sessions: null,
  });
  const listRef = useRef<HTMLDivElement>(null);
  const close = () => onOpenChange(false);

  // Global hotkey: Ctrl/Cmd+K opens (any page), Escape closes.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setQ("");
        setSel(0);
        onOpenChange(true);
      } else if (e.key === "Escape") {
        onOpenChange(false);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onOpenChange]);

  // On open: show the cache immediately, refresh in the background.
  useEffect(() => {
    if (!open) return;
    setAgents(cache.current.agents);
    setSessions(cache.current.sessions);
    api
      .agents()
      .then((a) => {
        cache.current.agents = a;
        setAgents(a);
      })
      .catch(() => {});
    api
      .sessions()
      .then((s) => {
        cache.current.sessions = s;
        setSessions(s);
      })
      .catch(() => {});
  }, [open]);

  const commands: Cmd[] = useMemo(() => {
    const out: Cmd[] = [];
    for (const n of NAV) {
      out.push({
        key: `nav-${n.hash || "home"}`,
        group: "nav",
        label: t(n.labelKey),
        keywords: n.kw,
        run: () => nav(n.hash),
      });
    }
    for (const a of agents ?? []) {
      if (!a.enabled) continue;
      out.push({
        key: `agent-${a.name}`,
        group: "agents",
        label: t("cmd.talkTo", { name: a.name }),
        hint: a.harness,
        run: () => nav(`chat?agent=${encodeURIComponent(a.name)}`),
      });
    }
    for (const s of (sessions ?? []).slice(0, 8)) {
      const title = s.title || s.preview || t("sessions.untitled");
      out.push({
        key: `session-${s.key}`,
        group: "sessions",
        label: t("cmd.session", { title: title.slice(0, 48) }),
        hint: SOURCE_LABEL[s.source] ?? s.source,
        run: () => nav("sessions"),
      });
    }
    out.push(
      {
        key: "act-chat",
        group: "actions",
        label: t("home.action.chat"),
        keywords: "new chat",
        run: () => nav("chat"),
      },
      {
        key: "act-task",
        group: "actions",
        label: t("home.action.task"),
        keywords: "new task",
        run: onNewTask,
      },
      {
        key: "act-theme",
        group: "actions",
        label: t(mode === "dark" ? "theme.light" : "theme.dark"),
        keywords: "theme dark light",
        run: toggle,
      },
      {
        key: "act-lang",
        group: "actions",
        label: t("cmd.toggleLang"),
        keywords: "language english chinese",
        run: () => setLang(lang === "zh" ? "en" : "zh"),
      },
    );
    return out;
  }, [t, agents, sessions, mode, lang, nav, onNewTask, toggle, setLang]);

  // Plain substring over label + hint + keywords; no fuzzy matching.
  const filtered = useMemo(() => {
    const needle = q.trim().toLowerCase();
    if (!needle) return commands;
    return commands.filter((c) =>
      `${c.label} ${c.hint ?? ""} ${c.keywords ?? ""}`.toLowerCase().includes(needle),
    );
  }, [commands, q]);

  useEffect(() => {
    setSel(0);
  }, [q]);

  // Keyboard navigation keeps the selected item in view.
  useEffect(() => {
    listRef.current
      ?.querySelector(".cmdk-item.selected")
      ?.scrollIntoView({ block: "nearest" });
  }, [sel, filtered]);

  const runIndex = (i: number) => {
    const c = filtered[i];
    if (!c) return;
    close();
    setQ("");
    c.run();
  };

  const onInputKey = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSel((s) => Math.min(s + 1, filtered.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSel((s) => Math.max(s - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      runIndex(sel);
    } else if (e.key === "Escape") {
      e.preventDefault();
      close();
    }
  };

  if (!open) return null;

  // Rendered inline (not portaled) on purpose: the palette consumes
  // antd's --ant-* CSS variables, which are scoped to the app tree — a
  // body-level portal would fall outside that scope and lose them.
  // position: fixed works from anywhere in the tree.
  let idx = -1;
  return (
    <div
      className="cmdk-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) close();
      }}
    >
      <div className="cmdk" role="dialog" aria-label={t("cmd.placeholder")}>
        <div className="cmdk-input-row">
          <Icon name="search" size={15} />
          <input
            autoFocus
            value={q}
            onChange={(e) => setQ(e.target.value)}
            onKeyDown={onInputKey}
            placeholder={t("cmd.placeholder")}
            spellCheck={false}
          />
          <span className="cmdk-esc">Esc</span>
        </div>
        <div className="cmdk-list" ref={listRef}>
          {filtered.length === 0 ? (
            <div className="cmdk-empty">{t("cmd.empty")}</div>
          ) : (
            GROUP_ORDER.map((g) => {
              const items = filtered.filter((c) => c.group === g.key);
              if (items.length === 0) return null;
              return (
                <div key={g.key}>
                  <div className="cmdk-group">{t(g.labelKey)}</div>
                  {items.map((c) => {
                    idx += 1;
                    const myIdx = idx;
                    return (
                      <button
                        key={c.key}
                        className={myIdx === sel ? "cmdk-item selected" : "cmdk-item"}
                        onMouseEnter={() => setSel(myIdx)}
                        onMouseDown={(e) => e.preventDefault()}
                        onClick={() => runIndex(myIdx)}
                      >
                        <span className="cmdk-label">{c.label}</span>
                        {c.hint ? <span className="cmdk-hint">{c.hint}</span> : null}
                      </button>
                    );
                  })}
                </div>
              );
            })
          )}
        </div>
        <div className="cmdk-foot">{t("cmd.foot")}</div>
      </div>
    </div>
  );
}
