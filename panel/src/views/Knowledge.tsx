// Knowledge manager: documents, ingest, hybrid search with chunk preview,
// and the markdown truth-source editor — doc-level raw editing, chunk-level
// curation with revision history + rollback, and an index rebuild.
//
// The search box sits above the content, not inside a tab: the first move in
// a knowledge base is always "where is the thing I stored". Expanding a row
// shows its chunks in place, because "edit the truth source" and "read the
// index result" belong in one line of sight (view-knowledge.md §1).

import { useCallback, useEffect, useState } from "react";
import { Button, Input, Popconfirm, Segmented } from "antd";
import { Icon } from "../icons";
import { api, type KnowledgeDocument, type KnowledgeRevision, type SearchHit } from "../api";
import {
  Empty,
  ErrorState,
  Modal,
  ReadoutStrip,
  RelTime,
  Spinner,
  Zone,
  useToast,
} from "../ui";
import { dateOf, useI18n } from "../i18n";
import { WikiTab } from "./Wiki";

/** Density caps (view-knowledge.md §4): three documents unfolded at most,
 *  200 chunks per document, 50 hits per search, 50 revisions per chunk. */
const MAX_OPEN_DOCS = 3;
const CHUNK_CAP = 200;
const HIT_CAP = 50;
const REVISION_CAP = 50;

/** Does the source file say anything the document name does not already say?
 *
 *  The index stores `source` as the file a document was ingested from, and for
 *  a plain ingest that is exactly `${name}.md` — so rendering both repeats one
 *  string, and at 390px it wraps the row onto three lines (measured: 90px,
 *  versus 48px with the tag gone). The redundancy is the cause, so the fix is
 *  to drop the repetition rather than to accept the taller row.
 *
 *  "Says the same" is judged on the path, not the raw string:
 *    · one trailing extension is removed from the source — wiki/index.md ≡ wiki/index
 *    · backslashes fold to "/" (Windows paths)             — wiki\index.md ≡ wiki/index
 *    · the comparison trims spaces and ignores case        — Wiki/Index.MD ≡ wiki/index
 *  The extension alone is never the difference that matters: every document
 *  here is markdown, so it is a constant rather than information, and the name
 *  already carries the directory the source lives in.
 *
 *  Everything else keeps both strings visible, e.g.
 *    · notes vs uploads/notes-2026.md   (a directory the name does not carry)
 *    · notes vs notes.md.txt            (only ONE extension is dropped)
 *    · notes vs .notes.md               (the stem is ".notes", not "notes")
 *  A bare trailing dot is NOT such a case: `notes.` folds to `notes` because
 *  the dot carries nothing (Windows strips it anyway). So the rule is a
 *  comparison, not "always show one". */
function sourceEchoesName(name: string, source: string): boolean {
  const norm = (s: string) =>
    s
      .trim()
      .replace(/\\/g, "/")
      .toLowerCase()
      // A leading "./" is the same path spelled longer ("the file in the
      // current directory"), so it folds too. Not seen in this index today
      // (all 5 documents store a bare relative path), but the rule is about
      // what the two strings MEAN, and "./x.md" means x.
      .replace(/^\.\//, "");
  const stem = norm(source).replace(/\.[^./]*$/, "");
  return stem.length > 0 && stem === norm(name);
}

/* MASTER §12 row 58 — the URL carries the view state. This app routes on the
   hash and App.parseHash splits "route[?query]" before matching the path, so a
   suffix reaches this view instead of falling through to the fallback. The view
   owns both directions, exactly as SessionsView does: a first render precedes
   any effect, so the URL has to be the source of truth for the initial state. */
function routeQuery(): URLSearchParams {
  const h = window.location.hash.replace(/^#/, "");
  const i = h.indexOf("?");
  return new URLSearchParams(i >= 0 ? h.slice(i + 1) : "");
}

/** An unknown value falls back to the default instead of reaching the
 *  Segmented, which would then render with nothing selected. */
function readTab(): "docs" | "wiki" {
  return routeQuery().get("tab") === "wiki" ? "wiki" : "docs";
}

export function Knowledge() {
  const { t } = useI18n();
  const [docs, setDocs] = useState<KnowledgeDocument[] | null>(null);
  const [docsFailed, setDocsFailed] = useState(false);
  const [embedder, setEmbedder] = useState("");
  const [ingesting, setIngesting] = useState(false);
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<SearchHit[] | null>(null);
  const [searchFailed, setSearchFailed] = useState(false);
  /** ids of the documents currently unfolded (ordered, <= MAX_OPEN_DOCS). */
  const [open, setOpen] = useState<number[]>([]);
  /** id -> chunks; null means "still loading". */
  const [chunks, setChunks] = useState<Record<number, [number, string][] | null>>({});
  const [capWarn, setCapWarn] = useState(false);
  const [editingDoc, setEditingDoc] = useState<KnowledgeDocument | null>(null);
  const [editingChunk, setEditingChunk] = useState<{
    id: number;
    content: string;
    doc: string;
  } | null>(null);
  /** chunk id whose revision history is unfolded (null = none) */
  const [revChunk, setRevChunk] = useState<number | null>(null);
  const [revisions, setRevisions] = useState<KnowledgeRevision[] | null>(null);
  const [rebuilding, setRebuilding] = useState(false);
  /** The rebuild report stays on screen: four numbers, and `errors` is a
   *  failure, not a footnote (view-knowledge.md §4). */
  const [rebuildReport, setRebuildReport] = useState<
    { indexed: number; unchanged: number; removed: number; errors: number } | null
  >(null);
  /** docs list vs the wiki tab (M2 panel surface). Initialised from the URL:
   *  row 58's replay is what makes a shared or refreshed link land here. */
  const [tab, setTab] = useState<"docs" | "wiki">(readTab);
  const toast = useToast();

  /* Row 58, the other direction: keep the URL in step with the tab. The
     comparison keeps this idempotent — the first render of an already-correct
     URL (including a pasted one) writes nothing, so no needless history churn,
     and "docs" is the default so the plain "#knowledge" stays clean. */
  useEffect(() => {
    const loc = window.location;
    const h = loc.hash.replace(/^#/, "");
    // Never rewrite another route's hash: while this page is leaving, the hash
    // already points elsewhere and a write here would drag it back (that is the
    // exact shape of the bug a removed unmount cleanup caused in the sessions
    // view). Writes use replaceState, which fires no hashchange, so the two
    // effects cannot loop.
    if (h !== "knowledge" && !h.startsWith("knowledge?")) return;
    const p = new URLSearchParams();
    if (tab !== "docs") p.set("tab", tab);
    const qs = p.toString();
    const next = loc.pathname + loc.search + "#knowledge" + (qs ? "?" + qs : "");
    if (loc.pathname + loc.search + loc.hash !== next) {
      window.history.replaceState(window.history.state, "", next);
    }
  }, [tab]);

  /* The reverse direction while this page stays mounted: a hash edited in the
     address bar, or a back/forward to another "#knowledge?…", changes the route
     query without unmounting the view (the route KIND is unchanged). The guard
     keeps a navigation away from being read as a state change. */
  useEffect(() => {
    const apply = () => {
      const h = window.location.hash.replace(/^#/, "");
      if (h !== "knowledge" && !h.startsWith("knowledge?")) return;
      setTab(readTab());
    };
    window.addEventListener("hashchange", apply);
    return () => window.removeEventListener("hashchange", apply);
  }, []);

  const refresh = useCallback(() => {
    api
      .knowledgeDocs()
      .then((r) => {
        setDocs(r.documents);
        setEmbedder(r.embedder);
        setDocsFailed(false);
      })
      .catch(() => {
        // A failed read is not an empty library (MASTER §12 row 20).
        setDocsFailed(true);
      });
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const search = async () => {
    if (!query.trim()) {
      setHits(null);
      return;
    }
    setSearchFailed(false);
    try {
      setHits(await api.knowledgeSearch(query.trim()));
    } catch {
      setSearchFailed(true);
    }
  };

  /** Open the raw editor for a wiki page (name = "wiki/<slug>"). */
  const openWikiEditor = (name: string) => {
    const doc = docs?.find((d) => d.name === name);
    setEditingDoc(
      doc ?? { id: -1, name, source: `${name}.md`, chunk_count: 0, created_at: "" },
    );
  };

  const loadChunks = useCallback(async (id: number) => {
    setChunks((prev) => ({ ...prev, [id]: null }));
    try {
      const c = await api.knowledgeChunks(id);
      setChunks((prev) => ({ ...prev, [id]: c }));
    } catch {
      setChunks((prev) => ({ ...prev, [id]: [] }));
    }
  }, []);

  const toggleDoc = (id: number) => {
    if (open.includes(id)) {
      setOpen(open.filter((x) => x !== id));
      return;
    }
    if (open.length >= MAX_OPEN_DOCS) {
      setCapWarn(true);
      return;
    }
    setCapWarn(false);
    setOpen([...open, id]);
    if (!(id in chunks)) void loadChunks(id);
  };

  const toggleRevisions = async (chunkId: number) => {
    if (revChunk === chunkId) {
      setRevChunk(null);
      setRevisions(null);
      return;
    }
    setRevChunk(chunkId);
    setRevisions(null);
    try {
      setRevisions((await api.knowledgeChunkRevisions(chunkId)).slice(0, REVISION_CAP));
    } catch {
      setRevisions([]);
    }
  };

  const rollback = async (revisionId: number) => {
    try {
      const out = await api.knowledgeRollback(revisionId);
      toast("ok", t("knowledge.rolledBack", { id: out.revision }));
      for (const id of open) await loadChunks(id);
      refresh();
      if (revChunk != null) {
        try {
          setRevisions((await api.knowledgeChunkRevisions(revChunk)).slice(0, REVISION_CAP));
        } catch {
          /* keep the stale list rather than blanking it */
        }
      }
    } catch (e) {
      toast("err", String(e));
    }
  };

  const rebuild = async () => {
    setRebuilding(true);
    try {
      const { rebuild: r } = await api.knowledgeRebuild();
      setRebuildReport(r);
      toast("ok", t("knowledge.rebuilt", { n: r.indexed, u: r.unchanged, r: r.removed }));
      refresh();
    } catch (e) {
      toast("err", String(e));
    } finally {
      setRebuilding(false);
    }
  };

  /** A saved chunk edit reindexes the document — reload the visible text
   * and unfold the history under the edited chunk (ids usually survive). */
  const onChunkSaved = async (chunkId: number) => {
    setEditingChunk(null);
    for (const id of open) await loadChunks(id);
    refresh();
    setRevChunk(chunkId);
    setRevisions(null);
    try {
      setRevisions((await api.knowledgeChunkRevisions(chunkId)).slice(0, REVISION_CAP));
    } catch {
      setRevisions([]);
    }
  };

  const chunkTotal = (docs ?? []).reduce((s, d) => s + d.chunk_count, 0);

  return (
    <div>
      <h1 className="sr-only micro">{t("knowledge.title")}</h1>
      <div className="view-bar">
        <h2>{t("knowledge.title")}</h2>
        <span className="muted">
          {t("knowledge.subtitle")} ({embedder || "…"})
        </span>
        <Segmented
          value={tab}
          onChange={(v) => setTab(v as "docs" | "wiki")}
          options={[
            { value: "docs", label: t("knowledge.tab.docs") },
            { value: "wiki", label: t("knowledge.tab.wiki") },
          ]}
        />
        <span className="grow" />
        {tab === "docs" && (
          <Popconfirm
            title={t("knowledge.rebuild.title")}
            description={t("knowledge.rebuild.body")}
            okText={t("knowledge.rebuild")}
            cancelText={t("common.cancel")}
            okButtonProps={{ danger: true }}
            onConfirm={rebuild}
          >
            <Button loading={rebuilding}>{t("knowledge.rebuild")}</Button>
          </Popconfirm>
        )}
        {tab === "docs" && (
          <Button type="primary" onClick={() => setIngesting(true)}>
            + {t("knowledge.ingest")}
          </Button>
        )}
      </div>

      {tab === "docs" && (
        <ReadoutStrip
          grid
          items={[
            { key: "docs", label: t("knowledge.tab.docs"), value: docs?.length ?? 0 },
            { key: "chunks", label: t("metric.chunks"), value: chunkTotal },
          ]}
        />
      )}

      {rebuildReport && tab === "docs" && (
        <p className="row tight">
          <span className={rebuildReport.errors > 0 ? "tag err" : "tag ok"}>
            {t("knowledge.rebuild")}
          </span>
          <span className="muted mono">
            {t("knowledge.rebuilt", {
              n: rebuildReport.indexed,
              u: rebuildReport.unchanged,
              r: rebuildReport.removed,
            })}
          </span>
          {rebuildReport.errors > 0 && (
            <span className="tag err">
              {t("knowledge.rebuiltErr", { e: rebuildReport.errors })}
            </span>
          )}
        </p>
      )}

      {tab === "wiki" ? (
        <WikiTab openEditor={openWikiEditor} />
      ) : (
        <Zone title={t("knowledge.tab.docs")} note={`${docs?.length ?? 0} · ${chunkTotal}`}>
          <div className="search-bar">
            <Input
              autoFocus
              className="grow"
              aria-label={t("knowledge.search")}
              placeholder={t("knowledge.search")}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onPressEnter={search}
            />
            <Button type="primary" onClick={search}>
              {t("common.search")}
            </Button>
            {hits ? (
              <Button
                type="text"
                onClick={() => {
                  setHits(null);
                  setQuery("");
                }}
              >
                {t("common.clear")}
              </Button>
            ) : null}
          </div>

          {searchFailed || docsFailed ? (
            <ErrorState
              title={t("knowledge.err")}
              hint={t("knowledge.err.hint")}
              onRetry={searchFailed ? search : refresh}
              retryLabel={t("common.retry")}
            />
          ) : hits ? (
            hits.length === 0 ? (
              <Empty icon="search" title={t("knowledge.noResults")} />
            ) : (
              <div className="card">
                {/* t316: the keyword leg's STAGE is a property of the QUERY, not
                    of a hit — measured on the live daemon: a hit whose legs are
                    ["semantic"] still reports query_keyword_stage "precision". It is
                    therefore rendered ONCE, above the hits, and never beside a
                    single hit as if it were that hit's source (that is what
                    「命中来源」/legs is for). The daemon repeats it on every hit;
                    the first one is the response's answer. */}
                {(() => {
                  const st = (hits[0] as { query_keyword_stage?: string })
                    .query_keyword_stage;
                  if (!st) return null;
                  return (
                    <div className="muted micro" title={t("knowledge.stageHint")}>
                      {t("knowledge.stage", { s: t(`knowledge.stage.${st}`) })}
                    </div>
                  );
                })()}
                {hits.slice(0, HIT_CAP).map((h) => {
                  // t263: the search wrapper's SearchHit type predates t251's
                  // leg evidence (api.ts is out of scope for this task), so the
                  // fields are read through a narrow cast: absent on an older
                  // daemon, in which case nothing extra renders.
                  const hit = h as typeof h & {
                    legs?: string[];
                    semantic_score?: number | null;
                    keyword_score?: number | null;
                  };
                  // NOTE: query_keyword_stage is deliberately NOT read here — it is
                  // query-level (see the block above the list), and a missing leg
                  // stays null: the two score lines below render only when the
                  // value is non-null, so "this leg found nothing" can never be
                  // shown as 0 or as any number that looks like a measurement.
                  return (
                  <div key={h.chunk_id} className="search-hit">
                    <div className="row tight">
                      <span className="tag">{h.document}</span>
                      {/* t253: this number is an RRF rank score, not a similarity.
                          It carried a hardcoded English "score" while the i18n key
                          knowledge.score existed and was referenced nowhere (dead
                          key). The label now comes from that key, and its value says
                          what the number really is; the title carries the ceiling. */}
                      <span className="muted mono" title={t("knowledge.scoreHint")}>
                        {t("knowledge.score", { s: h.score.toFixed(3) })
                        }
                      </span>
                    </div>
                    {/* t263: which legs found this hit, and each leg's OWN raw
                        score. They are deliberately NOT shown side by side as two
                        comparable numbers: the semantic leg is a LanceDB distance
                        (lower = closer) and the keyword leg is FTS5 bm25 (more
                        negative = better). Each gets its own line, unit and
                        direction word, so nobody reads them as one kind of number. */}
                    {hit.legs?.length ? (
                      <div className="hit-legs">
                        <span className="muted micro">{t("knowledge.legs")}</span>
                        {hit.legs.map((l) => (
                          <span key={l} className="tag micro">
                            {l === "semantic"
                              ? t("knowledge.legSemantic")
                              : l === "keyword"
                                ? t("knowledge.legKeyword")
                                : l}
                          </span>
                        ))}
                      </div>
                    ) : null}
                    {hit.semantic_score != null ? (
                      <div className="muted mono micro" title={t("knowledge.semanticHint")}>
                        {t("knowledge.semanticScore", {
                          s: hit.semantic_score.toFixed(4),
                        })}
                      </div>
                    ) : null}
                    {hit.keyword_score != null ? (
                      <div className="muted mono micro" title={t("knowledge.keywordHint")}>
                        {t("knowledge.keywordScore", { s: hit.keyword_score.toFixed(4) })}
                      </div>
                    ) : null}
                    <p className="hit-content">{h.content}</p>
                  </div>
                  );
                })}
                {hits.length > HIT_CAP && (
                  <p className="muted micro">{t("knowledge.hitCap", { n: HIT_CAP })}</p>
                )}
              </div>
            )
          ) : docs === null ? (
            <Spinner label={`${t("knowledge.title")}…`} />
          ) : docs.length === 0 ? (
            <Empty
              icon="book"
              title={t("knowledge.empty.title")}
              hint={t("knowledge.empty.hint")}
              action={
                <Button type="primary" onClick={() => setIngesting(true)}>
                  + {t("knowledge.ingest")}
                </Button>
              }
            />
          ) : (
            <div className="card">
              {docs.map((d) => (
                <div key={d.id}>
                  <div
                    role="button"
                    tabIndex={0}
                    aria-label={d.name}
                    aria-expanded={open.includes(d.id)}
                    className="row-btn"
                    onClick={() => toggleDoc(d.id)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" || e.key === " ") {
                        e.preventDefault();
                        toggleDoc(d.id);
                      }
                    }}
                  >
                    <span className="doc-icon">
                      <Icon name="doc" size={15} />
                    </span>
                    {/* The title stays `name`: it is the row's accessible
                        name (aria-label below), it is what every follow-up
                        dialog says (edit title, delete confirm, toast), and
                        WCAG 2.5.3 wants the accessible name to contain the
                        visible label. When the source only repeats it, the tag
                        is the redundant one and it goes. */}
                    <strong className="title">{d.name}</strong>
                    {d.source && !sourceEchoesName(d.name, d.source) ? (
                      <span className="tag">{d.source}</span>
                    ) : null}
                    <span className="muted">{t("knowledge.chunks", { n: d.chunk_count })}</span>
                    <span className="grow" />
                    {/* The timestamp rides a hoverable .row-btn, and the
                        quaternary grade does not clear its floor on that
                        hover surface. It sits at the row's metadata grade
                        instead — the same move the shared layer already makes
                        for .row-btn.selected. The grade itself and how far it
                        misses belong to the token in index.css and to MASTER
                        §12's contrast rows, which is where the audit prints
                        them; a copy of a measurement here goes stale the moment
                        the token changes, which is why this comment carries
                        none. Revert to className="time" once index.css carries
                        the hover companion (rule handed to systems, see
                        report). */}
                    <span className="micro mono muted">
                      <RelTime iso={d.created_at} />
                    </span>
                    {/* Row actions must not re-open the row, and a
                        destructive one confirms first (audit P2). */}
                    <span onClick={(e) => e.stopPropagation()}>
                      {d.source ? (
                        <Button onClick={() => setEditingDoc(d)}>{t("knowledge.edit")}</Button>
                      ) : (
                        <span className="legacy-hint">{t("knowledge.legacyHint")}</span>
                      )}
                    </span>
                    <span onClick={(e) => e.stopPropagation()}>
                      <Popconfirm
                        title={t("knowledge.delete.confirm", { name: d.name })}
                        okText={t("common.delete")}
                        cancelText={t("common.cancel")}
                        okButtonProps={{ danger: true }}
                        onConfirm={async () => {
                          try {
                            await api.knowledgeDelete(d.id);
                            toast("ok", t("knowledge.deleted", { name: d.name }));
                            refresh();
                          } catch (err) {
                            toast("err", String(err));
                          }
                        }}
                      >
                        <Button danger>{t("common.delete")}</Button>
                      </Popconfirm>
                    </span>
                  </div>
                  {open.includes(d.id) && (
                    <div className="chunks">
                      <ChunkList
                        chunks={chunks[d.id] ?? null}
                        revChunk={revChunk}
                        revisions={revisions}
                        onEdit={(id, text, name) =>
                          setEditingChunk({ id, content: text, doc: name })
                        }
                        onToggleRevisions={toggleRevisions}
                        onRollback={rollback}
                      />
                    </div>
                  )}
                </div>
              ))}
            </div>
          )}

          {capWarn && <p className="muted micro">{t("knowledge.expandCap")}</p>}
        </Zone>
      )}

      {ingesting && (
        <IngestModal
          onClose={() => setIngesting(false)}
          onIngested={() => {
            setIngesting(false);
            refresh();
          }}
        />
      )}

      {editingDoc && (
        <RawEditorModal
          doc={editingDoc}
          onClose={() => setEditingDoc(null)}
          onSaved={(saved) => {
            setEditingDoc(null);
            refresh();
            if (open.includes(saved.id)) void loadChunks(saved.id);
          }}
        />
      )}

      {editingChunk && (
        <ChunkEditorModal
          target={editingChunk}
          onClose={() => setEditingChunk(null)}
          onSaved={onChunkSaved}
        />
      )}
    </div>
  );
}

/** The chunk list of one document. Capped so 11 long chunks cannot stretch
 *  the page; the note says so rather than silently truncating. */
function ChunkList({
  chunks,
  revChunk,
  revisions,
  onEdit,
  onToggleRevisions,
  onRollback,
}: {
  chunks: [number, string][] | null;
  revChunk: number | null;
  revisions: KnowledgeRevision[] | null;
  onEdit: (id: number, content: string, doc: string) => void;
  onToggleRevisions: (id: number) => void;
  onRollback: (revisionId: number) => void;
}) {
  const { t } = useI18n();
  if (chunks === null) return <Spinner />;
  return (
    <>
      {chunks.slice(0, CHUNK_CAP).map(([id, text]) => (
        <div key={id} className="chunk-item">
          <pre className="raw">{text}</pre>
          <div className="row tight end chunk-actions">
            <Button type="text" onClick={() => onEdit(id, text, "")}>
              {t("knowledge.editChunk")}
            </Button>
            <Button type="text" onClick={() => onToggleRevisions(id)}>
              {t("knowledge.history")}
            </Button>
          </div>
          {revChunk === id &&
            (revisions === null ? (
              <Spinner />
            ) : revisions.length === 0 ? (
              <p className="muted micro">{t("knowledge.noRevisions")}</p>
            ) : (
              <div className="chunk-revisions">
                {revisions.map((r) => (
                  <div key={r.id} className="revision">
                    <div className="row tight">
                      {/* One timestamp voice per view (see the doc-row
                          timestamp above for why the grade is tertiary). */}
                      <span className="micro mono muted">{dateOf(r.edited_at)}</span>
                      <span className="muted mono">
                        #{r.id} · {r.document_name}
                      </span>
                      <span className="grow" />
                      {/* Rollback rewrites the chunk: confirm + busy. */}
                      <Popconfirm
                        title={t("knowledge.rollback.confirm")}
                        okText={t("knowledge.rollback")}
                        cancelText={t("common.cancel")}
                        okButtonProps={{ danger: true }}
                        onConfirm={() => onRollback(r.id)}
                      >
                        <Button type="text">{t("knowledge.rollback")}</Button>
                      </Popconfirm>
                    </div>
                    <span className="rev-label">{t("knowledge.old")}</span>
                    <pre className="raw rev-old">{r.old_content}</pre>
                    <span className="rev-label">{t("knowledge.new")}</span>
                    <pre className="raw">{r.new_content}</pre>
                  </div>
                ))}
              </div>
            ))}
        </div>
      ))}
      {chunks.length > CHUNK_CAP && (
        <p className="muted micro">
          {t("knowledge.chunkCap", { n: CHUNK_CAP, total: chunks.length })}
        </p>
      )}
    </>
  );
}

// ---------------------------------------------------------------------------
// Doc-level editor: the markdown file is the truth.
// ---------------------------------------------------------------------------

function RawEditorModal({
  doc,
  onClose,
  onSaved,
}: {
  doc: KnowledgeDocument;
  onClose: () => void;
  onSaved: (doc: KnowledgeDocument) => void;
}) {
  const { t } = useI18n();
  const [content, setContent] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const toast = useToast();

  useEffect(() => {
    let alive = true;
    api
      .knowledgeRaw(doc.name)
      .then((c) => {
        if (alive) setContent(c);
      })
      .catch(() => {
        // Keep the editor open with a retry: closing on a failed read loses
        // the user's place for no reason.
        if (alive) setFailed(true);
      });
    return () => {
      alive = false;
    };
  }, [doc.name]);

  const save = async () => {
    if (content === null || busy) return;
    setBusy(true);
    try {
      const r = await api.knowledgeSave(doc.name, content);
      toast("ok", t("knowledge.saved", { file: r.file, n: r.chunks }));
      onSaved(doc);
    } catch (e) {
      // The edited text stays in the textarea (view-knowledge.md §5).
      toast("err", String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal title={t("knowledge.editTitle", { name: doc.name })} onClose={onClose} wide>
      <p className="muted">{t("knowledge.editor")}</p>
      {failed ? (
        <ErrorState title={t("knowledge.err")} hint={t("knowledge.err.hint")} />
      ) : content === null ? (
        <Spinner />
      ) : (
        <Input.TextArea
          className="md-editor"
          rows={20}
          value={content}
          onChange={(e) => setContent(e.target.value)}
          spellCheck={false}
        />
      )}
      <div className="row end">
        <Button onClick={onClose}>{t("common.cancel")}</Button>
        <Button type="primary" loading={busy} disabled={content === null} onClick={save}>
          {t("knowledge.save")}
        </Button>
      </div>
    </Modal>
  );
}

// ---------------------------------------------------------------------------
// Chunk-level editor with the parent section as context.
// ---------------------------------------------------------------------------

function ChunkEditorModal({
  target,
  onClose,
  onSaved,
}: {
  target: { id: number; content: string; doc: string };
  onClose: () => void;
  onSaved: (chunkId: number) => void;
}) {
  const { t } = useI18n();
  const [content, setContent] = useState(target.content);
  const [busy, setBusy] = useState(false);
  const [section, setSection] = useState<string | null>(null);
  const toast = useToast();

  useEffect(() => {
    let alive = true;
    api
      .knowledgeExpand(target.id)
      .then((x) => {
        if (alive) setSection(x.section);
      })
      .catch(() => {
        if (alive) setSection("");
      });
    return () => {
      alive = false;
    };
  }, [target.id]);

  const save = async () => {
    if (busy) return;
    setBusy(true);
    try {
      const out = await api.knowledgeEditChunk(target.id, content);
      toast("ok", t("knowledge.chunkSaved", { id: out.revision, doc: out.document, n: out.chunks }));
      onSaved(target.id);
    } catch (e) {
      toast("err", String(e));
      setBusy(false);
    }
  };

  return (
    <Modal title={`${t("knowledge.editChunk")} #${target.id}`} onClose={onClose} wide>
      <p className="muted">
        {target.doc} · {t("knowledge.editor")}
      </p>
      {/* The warning is permanent, not a tooltip: without it a chunk edit
          reads as durable when the next rebuild overwrites it (K11). */}
      <p className="legacy-hint">{t("knowledge.chunkWarn")}</p>
      <Input.TextArea
        className="md-editor"
        rows={10}
        value={content}
        onChange={(e) => setContent(e.target.value)}
        spellCheck={false}
      />
      <details className="hint">
        <summary className="muted">{t("knowledge.expand")}</summary>
        {section === null ? (
          <p className="muted">…</p>
        ) : section ? (
          <pre className="raw">{section}</pre>
        ) : null}
      </details>
      <div className="row end">
        <Button onClick={onClose}>{t("common.cancel")}</Button>
        <Button type="primary" loading={busy} onClick={save}>
          {t("knowledge.save")}
        </Button>
      </div>
    </Modal>
  );
}

function IngestModal({ onClose, onIngested }: { onClose: () => void; onIngested: () => void }) {
  const [name, setName] = useState("");
  const [content, setContent] = useState("");
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const { t } = useI18n();
  const toast = useToast();
  return (
    <Modal title={t("knowledge.ingest.title")} onClose={onClose} wide>
      <label className="field">
        <span>{t("knowledge.ingest.name")}</span>
        <Input
          autoFocus
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder={t("knowledge.ingest.namePh")}
        />
      </label>
      <label className="field">
        <span>{t("knowledge.ingest.content")}</span>
        <Input.TextArea
          rows={12}
          value={content}
          onChange={(e) => setContent(e.target.value)}
          placeholder={`# Deploy runbook

1. Run scripts/release.sh from the repo root…`}
        />
      </label>
      {failed && (
        <ErrorState
          title={t("knowledge.err")}
          hint={t("knowledge.ingest.failed")}
        />
      )}
      <div className="row end">
        <Button
          type="primary"
          loading={busy}
          disabled={!name.trim() || !content.trim()}
          onClick={async () => {
            setBusy(true);
            try {
              const r = await api.knowledgeIngest(name.trim(), content);
              toast("ok", t("knowledge.ingested", { n: r.chunks }));
              onIngested();
            } catch {
              // The pasted text stays in the editor (view-knowledge.md §5).
              setFailed(true);
            } finally {
              setBusy(false);
            }
          }}
        >
          {t("knowledge.ingest.btn")}
        </Button>
      </div>
    </Modal>
  );
}
