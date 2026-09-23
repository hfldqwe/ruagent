// Command palette (Ctrl/Cmd+K): a quick-action layer over the whole
// panel — view navigation, per-agent chat entry, recent sessions, and
// the small set of global actions. Everything comes from data the panel
// already has; nothing new is fetched beyond a cached agents/sessions
// refresh on open.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Button } from "antd";
import { api, type AgentInfo, type SessionRecord } from "./api";
import { useI18n } from "./i18n";
import { useThemeMode } from "./theme";
import { Icon } from "./icons";
// Row 27: a pure constant module, not views/Sessions — importing the view
// from the entry chunk is what forced ui-work to split the Sessions page.
import { SOURCE_LABEL } from "./session-source";

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
  { hash: "agents", labelKey: "nav.agents", kw: "agents roles" },
  { hash: "runtimes", labelKey: "nav.runtimes", kw: "runtimes backends claude-code dsh opencode" },
  { hash: "stats", labelKey: "nav.stats", kw: "stats" },
  { hash: "settings", labelKey: "nav.settings", kw: "settings preferences distill" },
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
  const [loadErr, setLoadErr] = useState<string | null>(null);
  /** shown instantly on open, refreshed in the background */
  const cache = useRef<{ agents: AgentInfo[] | null; sessions: SessionRecord[] | null }>({
    agents: null,
    sessions: null,
  });
  const listRef = useRef<HTMLDivElement>(null);
  const dialogRef = useRef<HTMLDivElement>(null);
  const close = () => onOpenChange(false);
  /** The element that had focus when the palette opened (行 22).
   *  Captured by a focusin listener while closed, because autoFocus moves
   *  focus into the input before any open-time effect can read it. */
  const triggerRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (open) return;
    const remember = () => {
      const el = document.activeElement as HTMLElement | null;
      if (!el || el === document.body) return;
      // The listener is still attached for the commit in which the palette
      // mounts (autoFocus fires focusin before this effect's cleanup runs),
      // so the palette's own input must never become the remembered trigger.
      if (el.closest && el.closest(".cmdk")) return;
      triggerRef.current = el;
    };
    remember();
    document.addEventListener("focusin", remember);
    return () => document.removeEventListener("focusin", remember);
  }, [open]);

  // Escape/run closes the palette; focus must land back on the trigger, not
  // on <body> (行 22 — the input unmounts and takes the focus with it).
  useEffect(() => {
    if (open) return;
    const el = triggerRef.current;
    if (el && document.contains(el)) el.focus();
  }, [open]);

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

  /** F4: these two catches used to be `catch(() => {})`, so a failed
   *  refresh silently degraded the palette to navigation-only with no hint
   *  that roles/sessions were missing. The failure is now state. */
  const refreshList = useCallback(() => {
    setLoadErr(null);
    api
      .agents()
      .then((a) => {
        cache.current.agents = a;
        setAgents(a);
      })
      .catch((e) => setLoadErr(String(e)));
    api
      .sessions()
      .then((s) => {
        cache.current.sessions = s;
        setSessions(s);
      })
      .catch((e) => setLoadErr(String(e)));
  }, []);

  // On open: show the cache immediately, refresh in the background.
  useEffect(() => {
    if (!open) return;
    setAgents(cache.current.agents);
    setSessions(cache.current.sessions);
    refreshList();
  }, [open, refreshList]);

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

  /** F4: aria-modal="true" is a promise that focus cannot leave the
   *  dialog. antd supplies that for its own overlays, but .cmdk is hand
   *  written, so Tab used to walk the 34 options and then escape to the
   *  page behind it. This is the minimal trap: Tab cycles inside .cmdk. */
  const onDialogKey = (e: React.KeyboardEvent) => {
    if (e.key !== "Tab") return;
    const root = dialogRef.current;
    if (!root) return;
    const stops = Array.from(
      root.querySelectorAll<HTMLElement>(
        'input, button, [href], [tabindex]:not([tabindex="-1"])',
      ),
    ).filter((el) => !el.hasAttribute("disabled") && el.tabIndex >= 0);
    if (stops.length === 0) return;
    const first = stops[0];
    const last = stops[stops.length - 1];
    const active = document.activeElement as HTMLElement | null;
    if (!active || !root.contains(active)) {
      e.preventDefault();
      first.focus();
      return;
    }
    if (e.shiftKey && active === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && active === last) {
      e.preventDefault();
      first.focus();
    }
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
      <div
        className="cmdk"
        role="dialog"
        aria-modal="true"
        aria-label={t("cmd.placeholder")}
        ref={dialogRef}
        onKeyDown={onDialogKey}
      >
        <div className="cmdk-input-row">
          <Icon name="search" size={15} />
          <input
            autoFocus
            role="combobox"
            aria-expanded="true"
            aria-controls="cmdk-list"
            aria-autocomplete="list"
            aria-activedescendant={
              filtered[sel] ? `cmdk-opt-${sel}` : undefined
            }
            value={q}
            onChange={(e) => setQ(e.target.value)}
            onKeyDown={onInputKey}
            placeholder={t("cmd.placeholder")}
            spellCheck={false}
          />
          <span className="cmdk-esc">Esc</span>
        </div>
        {/* t43/N1: ARIA lets a listbox own only `option` / `group`, so the
            failure row (F4②) and the empty state are SIBLINGS of the listbox
            rather than its children. Nothing about the failure affordance
            moves off screen — it moves out of the list. */}
        {loadErr ? (
          <div className="row" role="alert">
            <span className="muted">{t("cmd.loadFailed")}</span>
            <span className="grow" />
            <Button size="small" onClick={refreshList}>
              {t("common.retry")}
            </Button>
          </div>
        ) : null}
        {filtered.length === 0 ? (
          <div className="cmdk-empty">{t("cmd.empty")}</div>
        ) : null}
        <div
          className="cmdk-list"
          id="cmdk-list"
          role="listbox"
          aria-label={t("cmd.list")}
          ref={listRef}
        >
          {GROUP_ORDER.map((g) => {
            const items = filtered.filter((c) => c.group === g.key);
            if (items.length === 0) return null;
            return (
              <div key={g.key} role="group" aria-label={t(g.labelKey)}>
                {/* The group already carries aria-label; without this the
                    same text is announced twice (primitives §5.2.1). */}
                <div className="cmdk-group" aria-hidden="true">
                  {t(g.labelKey)}
                </div>
                {items.map((c) => {
                  idx += 1;
                  const myIdx = idx;
                  return (
                    // APG combobox + listbox: the input is the only Tab stop and
                    // the Arrow keys move `aria-activedescendant`. With 34 options
                    // as Tab stops, crossing the list cost 34 presses — a real
                    // usability bug, not a paper one (primitives §5.2.1).
                    <button
                      key={c.key}
                      id={`cmdk-opt-${myIdx}`}
                      role="option"
                      aria-selected={myIdx === sel}
                      tabIndex={-1}
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
          })}
        </div>
        <div className="cmdk-foot">{t("cmd.foot")}</div>
      </div>
    </div>
  );
}
