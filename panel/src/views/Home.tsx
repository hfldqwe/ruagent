// Home: the discoverability layer — platform overview, getting-started
// steps, and a guide to every section. First thing a new user sees.

import { useEffect, useState } from "react";
import { api } from "../api";
import { useI18n } from "../i18n";
import { Spinner } from "../ui";
import { CreateTaskModal } from "./Board";

interface Overview {
  agents: number;
  tasks: number;
  runs: number;
  memories: number;
  docs: number;
}

export function Home({ onOpenTask, onNav }: { onOpenTask: (id: string) => void; onNav: (hash: string) => void }) {
  const { t } = useI18n();
  const [ov, setOv] = useState<Overview | null>(null);
  const [creating, setCreating] = useState(false);

  useEffect(() => {
    const load = async () => {
      try {
        const [agents, tasks, stats, mem, docs] = await Promise.all([
          api.agents(),
          api.tasks(),
          api.stats(),
          api.memoryList("observation", "user").catch(() => ({ memories: [], counts: [] as [string, string, number][] })),
          api.knowledgeDocs().catch(() => ({ documents: [], embedder: "" })),
        ]);
        setOv({
          agents: agents.filter((a) => a.enabled).length,
          tasks: tasks.length,
          runs: stats.reduce((s, x) => s + x.runs, 0),
          memories: mem.counts.reduce((s, c) => s + c[2], 0),
          docs: docs.documents.length,
        });
      } catch {
        setOv({ agents: 0, tasks: 0, runs: 0, memories: 0, docs: 0 });
      }
    };
    load();
    const i = setInterval(load, 5000);
    return () => clearInterval(i);
  }, []);

  if (!ov) return <Spinner label={t("common.loading")} />;

  const stats: { key: string; value: number }[] = [
    { key: "home.stat.agents", value: ov.agents },
    { key: "home.stat.tasks", value: ov.tasks },
    { key: "home.stat.runs", value: ov.runs },
    { key: "home.stat.memories", value: ov.memories },
    { key: "home.stat.docs", value: ov.docs },
  ];

  const guide: { icon: string; hash: string; label: string; desc: string }[] = [
    { icon: "🗂️", hash: "", label: t("nav.board"), desc: t("home.guide.board") },
    { icon: "🧠", hash: "memory", label: t("nav.memory"), desc: t("home.guide.memory") },
    { icon: "📚", hash: "knowledge", label: t("nav.knowledge"), desc: t("home.guide.knowledge") },
    { icon: "🕸️", hash: "graph", label: t("nav.graph"), desc: t("home.guide.graph") },
    { icon: "🤖", hash: "agents", label: t("nav.agents"), desc: t("home.guide.agents") },
    { icon: "📊", hash: "stats", label: t("nav.stats"), desc: t("home.guide.stats") },
    { icon: "📥", hash: "inbox", label: t("nav.inbox"), desc: t("home.guide.inbox") },
  ];

  return (
    <div className="home">
      <div className="home-hero">
        <h1>{t("home.greeting")}</h1>
        <p className="muted">{t("home.subtitle")}</p>
      </div>

      <div className="stat-strip">
        {stats.map((s) => (
          <div key={s.key} className="stat-card">
            <span className="stat-big nums">{s.value}</span>
            <span className="muted">{t(s.key)}</span>
          </div>
        ))}
      </div>

      <h2>{t("home.start.title")}</h2>
      <div className="steps">
        <div className="step-card">
          <div className="step-num">1</div>
          <div>
            <strong>{t("home.step1.title")}</strong>
            <p className="muted">{t("home.step1.desc")}</p>
            <button className="primary sm" onClick={() => setCreating(true)}>
              {ov.tasks === 0 ? t("home.step1.cta") : t("board.new")}
            </button>
          </div>
        </div>
        <div className="step-card">
          <div className="step-num">2</div>
          <div>
            <strong>{t("home.step2.title")}</strong>
            <p className="muted">{t("home.step2.desc")}</p>
            <button className="sm" onClick={() => onNav(ov.tasks ? `task/${""}` : "")} disabled={!ov.tasks}>
              {t("nav.board")}
            </button>
          </div>
        </div>
        <div className="step-card">
          <div className="step-num">3</div>
          <div>
            <strong>{t("home.step3.title")}</strong>
            <p className="muted">{t("home.step3.desc")}</p>
            <button className="sm" onClick={() => onNav("memory")}>
              {t("nav.memory")}
            </button>
          </div>
        </div>
      </div>

      <h2>{t("home.guide.title")}</h2>
      <div className="guide-grid">
        {guide.map((g) => (
          <button key={g.hash || "board"} className="guide-card" onClick={() => onNav(g.hash)}>
            <span className="guide-icon">{g.icon}</span>
            <div>
              <strong>{g.label}</strong>
              <p className="muted">{g.desc}</p>
            </div>
          </button>
        ))}
      </div>

      {creating && (
        <CreateTaskModal
          onClose={() => setCreating(false)}
          onCreated={(id) => {
            setCreating(false);
            onOpenTask(id);
          }}
        />
      )}
    </div>
  );
}
