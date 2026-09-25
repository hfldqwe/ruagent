// Zero-dependency i18n: dictionary lookup + language toggle, persisted to
// localStorage, default from navigator.language. UI is fully bilingual
// (zh / en).

import { createContext, useContext, useEffect, useState, type ReactNode } from "react";

export type Lang = "zh" | "en";

import * as navDomain from "./i18n/nav";
import * as themeDomain from "./i18n/theme";
import * as settingsDomain from "./i18n/settings";
import * as commonDomain from "./i18n/common";
import * as siderDomain from "./i18n/sider";
import * as cmdDomain from "./i18n/cmd";
import * as agentsDomain from "./i18n/agents";
import * as runtimesDomain from "./i18n/runtimes";
import * as homeDomain from "./i18n/home";
import * as boardDomain from "./i18n/board";
import * as newtaskDomain from "./i18n/newtask";
import * as taskDomain from "./i18n/task";
import * as toastDomain from "./i18n/toast";
import * as launcherDomain from "./i18n/launcher";
import * as distillDomain from "./i18n/distill";
import * as memoryDomain from "./i18n/memory";
import * as graphDomain from "./i18n/graph";
import * as knowledgeDomain from "./i18n/knowledge";
import * as wikiDomain from "./i18n/wiki";
import * as mcpDomain from "./i18n/mcp";
import * as statsDomain from "./i18n/stats";
import * as inboxDomain from "./i18n/inbox";
import * as timelineDomain from "./i18n/timeline";
import * as chatDomain from "./i18n/chat";
import * as sessionsDomain from "./i18n/sessions";
import * as statusDomain from "./i18n/status";
import * as metricDomain from "./i18n/metric";
// ---------------------------------------------------------------------------
// WHERE NEW COPY GOES (read this before adding a string)
//
// The dictionaries used to live in this one file, which made every view task that
// needed a string edit the same file - so those tasks could only run one at a time.
// They now live in panel/src/i18n/<domain>.ts, one file per key prefix, and this
// file only assembles them.
//
//   * New copy for a view goes in THAT view domain file (chat.* -> i18n/chat.ts,
//     sessions.* -> i18n/sessions.ts, ...). Two views = two files = no shared file
//     to serialise on.
//   * Every key must be added to BOTH zh and en in the same domain file.
//   * Cross-view copy (a button or empty state several views share) belongs in
//     i18n/common.ts - that file is shared on purpose, so keep it small.
//   * t(...) call sites never change: the dictionaries are merged back into the
//     same shape here.
// ---------------------------------------------------------------------------
const zh: Record<string, string> = {
  ...navDomain.zh,
  ...themeDomain.zh,
  ...settingsDomain.zh,
  ...commonDomain.zh,
  ...siderDomain.zh,
  ...cmdDomain.zh,
  ...agentsDomain.zh,
  ...runtimesDomain.zh,
  ...homeDomain.zh,
  ...boardDomain.zh,
  ...newtaskDomain.zh,
  ...taskDomain.zh,
  ...toastDomain.zh,
  ...launcherDomain.zh,
  ...distillDomain.zh,
  ...memoryDomain.zh,
  ...graphDomain.zh,
  ...knowledgeDomain.zh,
  ...wikiDomain.zh,
  ...mcpDomain.zh,
  ...statsDomain.zh,
  ...inboxDomain.zh,
  ...timelineDomain.zh,
  ...chatDomain.zh,
  ...sessionsDomain.zh,
  ...statusDomain.zh,
  ...metricDomain.zh,
};
const en: Record<string, string> = {
  ...navDomain.en,
  ...themeDomain.en,
  ...settingsDomain.en,
  ...commonDomain.en,
  ...siderDomain.en,
  ...cmdDomain.en,
  ...agentsDomain.en,
  ...runtimesDomain.en,
  ...homeDomain.en,
  ...boardDomain.en,
  ...newtaskDomain.en,
  ...taskDomain.en,
  ...toastDomain.en,
  ...launcherDomain.en,
  ...distillDomain.en,
  ...memoryDomain.en,
  ...graphDomain.en,
  ...knowledgeDomain.en,
  ...wikiDomain.en,
  ...mcpDomain.en,
  ...statsDomain.en,
  ...inboxDomain.en,
  ...timelineDomain.en,
  ...chatDomain.en,
  ...sessionsDomain.en,
  ...statusDomain.en,
  ...metricDomain.en,
};

const dicts: Record<Lang, Record<string, string>> = { zh, en };

interface I18n {
  lang: Lang;
  setLang: (l: Lang) => void;
  t: (key: string, params?: Record<string, string | number>) => string;
}

const Ctx = createContext<I18n | null>(null);

export function I18nProvider({ children }: { children: ReactNode }) {
  const [lang, setLang] = useState<Lang>(() => {
    try {
      const saved = localStorage.getItem("ruagent.lang");
      if (saved === "zh" || saved === "en") return saved;
    } catch {
      /* private mode */
    }
    return navigator.language.toLowerCase().startsWith("zh") ? "zh" : "en";
  });

  useEffect(() => {
    try {
      localStorage.setItem("ruagent.lang", lang);
    } catch {
      /* ignore */
    }
    document.documentElement.lang = lang === "zh" ? "zh-CN" : "en";
  }, [lang]);

  const t = (key: string, params?: Record<string, string | number>) => {
    let s = dicts[lang][key] ?? dicts.en[key] ?? key;
    if (params) {
      for (const [k, v] of Object.entries(params)) {
        // A missing param must never render as the literal string
        // "undefined": leave the placeholder standing instead, so the leak
        // is visible as {n} (a bug) rather than as plausible-looking copy.
        if (v === undefined || v === null) continue;
        s = s.replaceAll(`{${k}}`, String(v));
      }
    }
    return s;
  };

  return <Ctx.Provider value={{ lang, setLang, t }}>{children}</Ctx.Provider>;
}

export function useI18n(): I18n {
  const v = useContext(Ctx);
  if (!v) throw new Error("useI18n must be used inside I18nProvider");
  return v;
}

// ---------------------------------------------------------------------------
// Locale-aware relative time
// ---------------------------------------------------------------------------

export function relTime(iso: string | null | undefined, lang: Lang): string {
  if (!iso) return "—";
  const then = new Date(iso.endsWith("Z") || iso.includes("+") ? iso : `${iso}Z`).getTime();
  if (Number.isNaN(then)) return iso;
  const diff = (Date.now() - then) / 1000;
  if (lang === "zh") {
    if (diff < 10) return "刚刚";
    if (diff < 60) return `${Math.floor(diff)} 秒前`;
    if (diff < 3600) return `${Math.floor(diff / 60)} 分钟前`;
    if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`;
    if (diff < 86400 * 7) return `${Math.floor(diff / 86400)} 天前`;
    return new Date(then).toLocaleDateString("zh-CN");
  }
  if (diff < 10) return "just now";
  if (diff < 60) return `${Math.floor(diff)}s ago`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  if (diff < 86400 * 7) return `${Math.floor(diff / 86400)}d ago`;
  return new Date(then).toLocaleDateString("en-US");
}

export function dateOf(iso: string): string {
  return (iso.split("T")[0] || "").replace(/Z$/, "");
}
