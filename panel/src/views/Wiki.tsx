// Wiki tab of the knowledge manager: page inventory with stale/edited
// markers, the link graph's broken (wanted) pages, build history, and
// the compile flow — dry-run plan preview is the human gate; the plan
// is confirmed explicitly before any page is written (design §6.6,
// §13). Generated pages are ordinary markdown docs under wiki/.
//
// A wiki page is a GENERATED artefact. Every surface that lists one says so
// (page rows carry stale/edited/orphan marks; recall labels wiki hits
// "生成内容"), because compiled content and hand-written sources must never
// read as the same kind of thing (view-knowledge.md §1).

import { useEffect, useRef, useState } from "react";
import { Button, Segmented, Select } from "antd";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import {
  api,
  type AgentInfo,
  type WikiBuild,
  type WikiBuildStarted,
  type WikiPageInfo,
} from "../api";
import { Empty, ErrorState, Modal, RelTime, Spinner, Zone, useToast } from "../ui";
import { Icon } from "../icons";
import { useI18n } from "../i18n";

/** action badge vocabulary (planner output). */
const ACTION_CLASS: Record<string, string> = {
  create: "ok",
  update: "",
  delete: "err",
  keep: "",
};

const PAGE_CAP = 300;
const BUILD_CAP = 20;

export function WikiTab({ openEditor }: { openEditor: (name: string) => void }) {
  const { t } = useI18n();
  const toast = useToast();
  const [pages, setPages] = useState<WikiPageInfo[] | null>(null);
  const [builds, setBuilds] = useState<WikiBuild[] | null>(null);
  const [broken, setBroken] = useState<string[]>([]);
  const [orphans, setOrphans] = useState<string[]>([]);
  const [failed, setFailed] = useState(false);
  const [compiling, setCompiling] = useState(false);
  /** slug being viewed in the page viewer (null = closed). */
  const [viewing, setViewing] = useState<string | null>(null);
  /** build id whose per-page detail is unfolded. */
  const [openBuild, setOpenBuild] = useState<number | null>(null);
  const [buildDetail, setBuildDetail] = useState<
    { slug: string; action: string; status: string; error: string | null }[] | null
  >(null);
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  const refresh = () =>
    Promise.all([api.wikiPages(), api.wikiLinks(), api.wikiBuilds()])
      .then(([p, l, b]) => {
        if (!alive.current) return;
        setPages(p);
        setBroken(l.broken);
        setOrphans(l.orphans);
        setBuilds(b);
        setFailed(false);
      })
      .catch(() => {
        // Keep whatever we had: an unreadable wiki is not an empty wiki
        // (MASTER §12 row 20).
        if (alive.current) setFailed(true);
      });

  useEffect(() => {
    refresh();
    const i = setInterval(refresh, 3000);
    return () => clearInterval(i);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const toggleBuild = async (id: number) => {
    if (openBuild === id) {
      setOpenBuild(null);
      return;
    }
    setOpenBuild(id);
    setBuildDetail(null);
    try {
      const d = await api.wikiBuild(id);
      setBuildDetail(d.pages);
    } catch (e) {
      toast("err", String(e));
    }
  };

  if (failed && pages === null) {
    return (
      <ErrorState
        title={t("wiki.err")}
        hint={t("knowledge.err.hint")}
        onRetry={refresh}
        retryLabel={t("common.retry")}
      />
    );
  }
  if (pages === null || builds === null) {
    return <Spinner label={t("knowledge.title")} />;
  }
  const staleCount = pages.filter((p) => p.stale).length;
  const running = builds.find((b) => b.status === "running");
  const shownPages = pages.slice(0, PAGE_CAP);
  const shownBuilds = builds.slice(0, BUILD_CAP);

  return (
    <div>
      {failed && (
        <ErrorState
          title={t("wiki.err")}
          hint={t("knowledge.err.hint")}
          onRetry={refresh}
          retryLabel={t("common.retry")}
        />
      )}

      <Zone
        title={t("knowledge.tab.wiki")}
        note={t("wiki.stat", { n: pages.length, stale: staleCount, broken: broken.length })}
        actions={
          <Button
            type="primary"
            disabled={!!running}
            onClick={() => setCompiling(true)}
            title={running ? t("wiki.runningHint") : undefined}
          >
            {t("wiki.compile")}
          </Button>
        }
      >
        {broken.length > 0 && (
          <div className="row tight wrap">
            <span className="zone-title">{t("wiki.wanted")}</span>
            {broken.map((b) => (
              <span key={b} className="tag err mono">
                {b}?
              </span>
            ))}
          </div>
        )}

        {pages.length === 0 ? (
          <Empty
            icon="book"
            title={t("wiki.empty.title")}
            hint={t("wiki.empty.hint")}
            action={
              <Button type="primary" onClick={() => setCompiling(true)}>
                {t("wiki.compile")}
              </Button>
            }
          />
        ) : (
          <div className="card">
            {shownPages.map((p) => (
              <div
                key={p.slug}
                role="button"
                tabIndex={0}
                aria-label={p.slug}
                className="row-btn"
                onClick={() => setViewing(p.slug)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    setViewing(p.slug);
                  }
                }}
              >
                <span className="doc-icon">
                  <Icon name="doc" size={15} />
                </span>
                <span className="title">{p.title || p.slug}</span>
                {p.stale && <span className="tag warn">{t("wiki.staleTag")}</span>}
                {p.edited && <span className="tag">{t("wiki.editedTag")}</span>}
                {orphans.includes(p.slug) && <span className="tag">{t("wiki.orphanTag")}</span>}
                <span className="muted mono">{p.slug}</span>
                <span className="grow" />
                <span className="muted">
                  {t("wiki.links", { out: p.links_out, in: p.links_in })}
                </span>
                <span className="muted">{p.sources.join(", ")}</span>
              </div>
            ))}
            {pages.length > shownPages.length && (
              <p className="muted micro">{t("wiki.pageCap", { n: PAGE_CAP })}</p>
            )}
          </div>
        )}
      </Zone>

      {shownBuilds.length === 0 ? (
        <Zone title={t("wiki.builds")}>
          <Empty
            icon="scroll"
            title={t("wiki.buildsEmpty")}
            hint={t("wiki.buildsEmptyHint")}
            action={<Button onClick={() => setCompiling(true)}>{t("wiki.compile.preview")}</Button>}
          />
        </Zone>
      ) : (
        <Zone title={t("wiki.builds")} note={builds.length}>
          <div className="card">
            {shownBuilds.map((b) => (
              <div key={b.id}>
                <div
                  role="button"
                  tabIndex={0}
                  aria-label={`build-${b.id}`}
                  aria-expanded={openBuild === b.id}
                  className="row-btn"
                  onClick={() => void toggleBuild(b.id)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.preventDefault();
                      void toggleBuild(b.id);
                    }
                  }}
                >
                  <strong className="mono">#{b.id}</strong>
                  <span
                    className={`tag ${
                      b.status === "done"
                        ? "ok"
                        : b.status === "failed"
                          ? "err"
                          : b.status === "running"
                            ? "warn"
                            : ""
                    }`}
                  >
                    {b.dry_run ? `${b.status} (dry)` : b.status}
                  </span>
                  <span className="tag">{b.scope}</span>
                  <span className="muted mono">{b.agent}</span>
                  <span className="grow" />
                  <span className="muted">
                    {t("wiki.buildPages", { written: b.pages_written, planned: b.pages_planned })}
                  </span>
                  {b.pages_failed > 0 && (
                    <span className="tag err">{t("wiki.buildFailed", { n: b.pages_failed })}</span>
                  )}
                  <span className="time">
                    <RelTime iso={b.started_at} />
                  </span>
                </div>
                {openBuild === b.id && (
                  <div className="chunks">
                    {buildDetail === null ? (
                      <Spinner />
                    ) : (
                      buildDetail.map((p) => (
                        <div key={p.slug} className="chunk-item">
                          <span className={`tag ${ACTION_CLASS[p.action] ?? ""}`}>
                            {t(`wiki.action.${p.action}`)}
                          </span>
                          <span className="mono">{p.slug}</span>
                          <span
                            className={`tag ${
                              p.status === "written" || p.status === "deleted"
                                ? "ok"
                                : p.status === "failed"
                                  ? "err"
                                  : ""
                            }`}
                          >
                            {p.status}
                          </span>
                          {p.error && <span className="muted micro">{p.error}</span>}
                        </div>
                      ))
                    )}
                  </div>
                )}
              </div>
            ))}
          </div>
        </Zone>
      )}

      {viewing && (
        <WikiPageModal
          slug={viewing}
          pages={pages}
          onEdit={(slug) => {
            setViewing(null);
            openEditor(`wiki/${slug}`);
          }}
          onClose={() => setViewing(null)}
        />
      )}

      {compiling && (
        <CompileModal
          onClose={() => setCompiling(false)}
          onStarted={() => {
            setCompiling(false);
            refresh();
          }}
        />
      )}
    </div>
  );
}

/** The compile flow: scope + agent -> dry-run plan (the human gate) ->
 * confirm executes the stored plan. */
function CompileModal({
  onClose,
  onStarted,
}: {
  onClose: () => void;
  onStarted: () => void;
}) {
  const { t } = useI18n();
  const toast = useToast();
  const [scope, setScope] = useState<"all" | "changed">("changed");
  const [agents, setAgents] = useState<AgentInfo[] | null>(null);
  const [agent, setAgent] = useState<string | undefined>(undefined);
  const [preview, setPreview] = useState<WikiBuildStarted | null>(null);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    api
      .agents()
      .then((a) => setAgents(a.filter((x) => x.enabled)))
      .catch(() => setAgents([]));
  }, []);

  const plan = async () => {
    if (busy) return;
    setBusy(true);
    setFailed(false);
    setPreview(null);
    try {
      const r = await api.wikiBuildStart({ scope, dry_run: true, agent });
      setPreview(r);
    } catch {
      setFailed(true);
    } finally {
      setBusy(false);
    }
  };

  const confirm = async () => {
    if (!preview || busy) return;
    setBusy(true);
    try {
      const r = await api.wikiBuildConfirm(preview.build_id, agent);
      toast("ok", t("wiki.compile.started", { id: r.build_id }));
      onStarted();
    } catch (e) {
      toast("err", String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal
      title={t("wiki.compile")}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          {preview ? (
            <>
              <Button onClick={plan} loading={busy}>
                {t("wiki.compile.replan")}
              </Button>
              <Button
                type="primary"
                onClick={confirm}
                loading={busy}
                disabled={preview.pages_planned === 0}
              >
                {t("wiki.compile.confirm")}
              </Button>
            </>
          ) : (
            <Button type="primary" onClick={plan} loading={busy}>
              {t("wiki.compile.preview")}
            </Button>
          )}
        </>
      }
    >
      <div className="row tight">
        <span className="muted">{t("wiki.compile.scope")}</span>
        <Segmented
          value={scope}
          onChange={(v) => setScope(v as "all" | "changed")}
          options={[
            { value: "changed", label: t("wiki.compile.scope.changed") },
            { value: "all", label: t("wiki.compile.scope.all") },
          ]}
        />
        <span className="grow" />
        {agents && agents.length > 0 && (
          <>
            <span className="muted">{t("wiki.compile.agent")}</span>
            <Select
              size="small"
              aria-label={t("wiki.compile.agent")}
              style={{ minWidth: 160 }}
              allowClear
              placeholder={t("wiki.compile.agent.auto")}
              value={agent}
              onChange={setAgent}
              options={agents.map((a) => ({ value: a.name, label: a.name }))}
            />
          </>
        )}
      </div>

      {failed && <ErrorState title={t("wiki.err")} hint={t("knowledge.err.hint")} />}

      {preview && (
        <div>
          <div className="row tight">
            <span>{t("wiki.compile.planned", { n: preview.pages_planned })}</span>
            <span className="muted mono">{preview.agent}</span>
          </div>
          {preview.notes && <p className="muted">{preview.notes}</p>}
          <div className="card" style={{ maxHeight: 320, overflow: "auto" }}>
            {(preview.plan ?? []).map((p) => (
              <div key={p.slug} className="chunk-item">
                <span className={`tag ${ACTION_CLASS[p.action] ?? ""}`}>
                  {t(`wiki.action.${p.action}`)}
                </span>
                <strong className="title">{p.title || p.slug}</strong>
                <span className="mono muted">{p.slug}</span>
                <span className="grow" />
                <span className="muted micro">{p.sources?.join(", ")}</span>
              </div>
            ))}
          </div>
          <p className="muted micro">{t("wiki.compile.gate")}</p>
        </div>
      )}
    </Modal>
  );
}

// ---------------------------------------------------------------------------
// Page viewer: rendered markdown with navigable [[wikilinks]] — the
// Karpathy-wiki core interaction. Broken links render red with "?" —
// they are the wanted pages (MediaWiki convention).
// ---------------------------------------------------------------------------

/** Strip the daemon-serialized frontmatter block (everything between
 * the leading `---` and the closing `---`). */
function stripFrontmatter(text: string): string {
  if (!text.startsWith("---\n")) return text;
  const end = text.indexOf("\n---\n", 3);
  return end < 0 ? text : text.slice(end + 5);
}

/** Mirror of the server's normalize_target (wiki.rs §5.2). */
function normalizeWikiTarget(target: string): string {
  return target.trim().replace(/\.md$/, "").trim().toLowerCase().replace(/ /g, "-");
}

/** [[target|display]] / [[target]] -> markdown links pointing at the
 * viewer's internal scheme (#wiki-<slug>). Code fences are left
 * alone — same discipline as the server's wiki_links parser. */
function linkifyWikilinks(body: string): string {
  const link = (t: string, disp?: string) => {
    const slug = normalizeWikiTarget(t);
    return `[${(disp ?? t).trim()}](#wiki-${slug})`;
  };
  return body
    .split(/(```[\s\S]*?```)/g)
    .map((part, i) =>
      i % 2 === 1
        ? part
        : part.replace(/\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g, (_, t: string, d?: string) =>
            link(t, d),
          ),
    )
    .join("");
}

function WikiPageModal({
  slug,
  pages,
  onEdit,
  onClose,
}: {
  slug: string;
  pages: WikiPageInfo[];
  onEdit: (slug: string) => void;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const [current, setCurrent] = useState(slug);
  const [raw, setRaw] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let alive = true;
    setRaw(null);
    setFailed(false);
    api
      .knowledgeRaw(`wiki/${current}`)
      .then((r) => {
        if (alive) setRaw(r);
      })
      .catch(() => {
        // In-place error, not a silent close: a failed read is not "gone".
        if (alive) setFailed(true);
      });
    return () => {
      alive = false;
    };
  }, [current]);

  const title = pages.find((p) => p.slug === current)?.title || current;
  const exists = (s: string) => pages.some((p) => p.slug === s);
  const body = raw === null ? "" : linkifyWikilinks(stripFrontmatter(raw));

  return (
    <Modal
      wide
      title={title}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>{t("common.close")}</Button>
          <Button type="primary" onClick={() => onEdit(current)}>
            {t("wiki.view.edit")}
          </Button>
        </>
      }
    >
      {failed ? (
        <ErrorState title={t("wiki.err")} hint={t("knowledge.err.hint")} />
      ) : raw === null ? (
        <Spinner />
      ) : (
        <div className="md wiki-view">
          <ReactMarkdown
            remarkPlugins={[remarkGfm]}
            components={{
              a: ({ href, children }) => {
                if (typeof href === "string" && href.startsWith("#wiki-")) {
                  const target = href.slice("#wiki-".length);
                  if (!exists(target)) {
                    // wanted page — red ? marks what a future build
                    // should create
                    return (
                      <span className="wiki-broken">
                        {children}
                        <span className="wiki-q">?</span>
                      </span>
                    );
                  }
                  return (
                    <a
                      href={`#wiki-${target}`}
                      onClick={(e) => {
                        e.preventDefault();
                        setCurrent(target);
                      }}
                    >
                      {children}
                    </a>
                  );
                }
                return (
                  <a href={href} target="_blank" rel="noreferrer">
                    {children}
                  </a>
                );
              },
            }}
          >
            {body}
          </ReactMarkdown>
        </div>
      )}
    </Modal>
  );
}
