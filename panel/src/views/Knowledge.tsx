// Knowledge manager: documents, ingest, hybrid search with chunk preview,
// and the markdown truth-source editor — doc-level raw editing, chunk-level
// curation with revision history + rollback, and an index rebuild.

import { useEffect, useState } from "react";
import { Button, Input, Popconfirm, Segmented } from "antd";
import { Icon } from "../icons";
import { api, type KnowledgeDocument, type KnowledgeRevision, type SearchHit } from "../api";
import { Empty, Modal, RelTime, Spinner, useToast } from "../ui";
import { dateOf, useI18n } from "../i18n";
import { WikiTab } from "./Wiki";

export function Knowledge() {
  const { t } = useI18n();
  const [docs, setDocs] = useState<KnowledgeDocument[] | null>(null);
  const [embedder, setEmbedder] = useState("");
  const [ingesting, setIngesting] = useState(false);
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<SearchHit[] | null>(null);
  const [expanded, setExpanded] = useState<number | null>(null);
  const [chunks, setChunks] = useState<[number, string][] | null>(null);
  const [editingDoc, setEditingDoc] = useState<KnowledgeDocument | null>(null);
  const [editingChunk, setEditingChunk] = useState<{ id: number; content: string; doc: string } | null>(null);
  /** chunk id whose revision history is unfolded (null = none) */
  const [revChunk, setRevChunk] = useState<number | null>(null);
  const [revisions, setRevisions] = useState<KnowledgeRevision[] | null>(null);
  const [rebuilding, setRebuilding] = useState(false);
  /** docs list vs the wiki tab (M2 panel surface). */
  const [tab, setTab] = useState<"docs" | "wiki">("docs");
  const toast = useToast();

  const refresh = () =>
    api
      .knowledgeDocs()
      .then((r) => {
        setDocs(r.documents);
        setEmbedder(r.embedder);
      })
      .catch((e) => {
        toast("err", String(e));
        setDocs([]);
      });

  useEffect(() => {
    refresh();
  }, []);

  const search = async () => {
    if (!query.trim()) {
      setHits(null);
      return;
    }
    try {
      setHits(await api.knowledgeSearch(query.trim()));
    } catch (e) {
      toast("err", String(e));
      setHits([]);
    }
  };

  /** Open the raw editor for a wiki page (name = "wiki/<slug>"). */
  const openWikiEditor = (name: string) => {
    const doc = docs?.find((d) => d.name === name);
    setEditingDoc(
      doc ?? { id: -1, name, source: `${name}.md`, chunk_count: 0, created_at: "" },
    );
  };

  const reloadChunks = async (id: number) => {
    setChunks(null);
    try {
      setChunks(await api.knowledgeChunks(id));
    } catch {
      setChunks([]);
    }
  };

  const openChunks = (id: number) => {
    if (expanded === id) {
      setExpanded(null);
      setChunks(null);
      setRevChunk(null);
      setRevisions(null);
      return;
    }
    setExpanded(id);
    setRevChunk(null);
    setRevisions(null);
    reloadChunks(id);
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
      setRevisions(await api.knowledgeChunkRevisions(chunkId));
    } catch (e) {
      toast("err", String(e));
      setRevisions([]);
    }
  };

  const rollback = async (revisionId: number) => {
    try {
      const out = await api.knowledgeRollback(revisionId);
      toast("ok", t("knowledge.rolledBack", { id: out.revision }));
      if (expanded != null) await reloadChunks(expanded);
      refresh();
      // The rollback is itself a new revision — refresh the open history.
      if (revChunk != null) {
        try {
          setRevisions(await api.knowledgeChunkRevisions(revChunk));
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
      let msg = t("knowledge.rebuilt", { n: r.indexed, u: r.unchanged, r: r.removed });
      if (r.errors > 0) msg += t("knowledge.rebuiltErr", { e: r.errors });
      toast("ok", msg);
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
    if (expanded != null) await reloadChunks(expanded);
    refresh();
    setRevChunk(chunkId);
    setRevisions(null);
    try {
      setRevisions(await api.knowledgeChunkRevisions(chunkId));
    } catch {
      setRevisions([]);
    }
  };

  return (
    <div>
      <div className="view-bar">
        <h2>{t("knowledge.title")}</h2>
        <span className="muted">{t("knowledge.subtitle")} ({embedder || "…"})</span>
        <Segmented
          value={tab}
          onChange={(v) => setTab(v as "docs" | "wiki")}
          options={[
            { value: "docs", label: t("knowledge.tab.docs") },
            { value: "wiki", label: t("knowledge.tab.wiki") },
          ]}
        />
        <span className="grow" />
        {tab === "docs" && <Popconfirm
          title={t("knowledge.rebuild.title")}
          description={t("knowledge.rebuild.body")}
          okText={t("knowledge.rebuild")}
          cancelText={t("common.cancel")}
          okButtonProps={{ danger: true }}
          onConfirm={rebuild}
        >
          <Button loading={rebuilding}>{t("knowledge.rebuild")}</Button>
        </Popconfirm>}
        {tab === "docs" && (
          <Button type="primary" onClick={() => setIngesting(true)}>
            + {t("knowledge.ingest")}
          </Button>
        )}
      </div>

      {tab === "wiki" ? (
        <WikiTab openEditor={openWikiEditor} />
      ) : (
      <>
      <div className="search-bar">
        <Input
          autoFocus
          className="grow"
          placeholder={t("knowledge.search")}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onPressEnter={search}
        />
        <Button type="primary" onClick={search}>
          {t("common.search")}
        </Button>
        {hits ? (
          <Button type="link" onClick={() => { setHits(null); setQuery(""); }}>
            {t("common.clear")}
          </Button>
        ) : null}
      </div>

      {hits ? (
        hits.length === 0 ? (
          <Empty icon="search" title={t("knowledge.noResults")} />
        ) : (
          <div className="card">
            {hits.map((h) => (
              <div key={h.chunk_id} className="search-hit">
                <div className="row tight">
                  <span className="tag">{h.document}</span>
                  <span className="muted mono">score {h.score.toFixed(3)}</span>
                </div>
                <p className="hit-content">{h.content}</p>
              </div>
            ))}
          </div>
        )
      ) : docs === null ? (
        <Spinner label={`${t("knowledge.title")}…`} />
      ) : docs.length === 0 ? (
        <Empty
          icon="book"
          title={t("knowledge.empty.title")}
          hint={t("knowledge.empty.hint")}
        />
      ) : (
        <div className="card">
          {docs.map((d) => (
            <div key={d.id}>
              <div
                role="button"
                tabIndex={0}
                aria-label={`${d.name}`}
                className="row-btn"
                onClick={() => openChunks(d.id)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    openChunks(d.id);
                  }
                }}
              >
                <span className="doc-icon"><Icon name="doc" size={15} /></span>
                <strong className="title">{d.name}</strong>
                {d.source ? <span className="tag">{d.source}</span> : null}
                <span className="muted">{t("knowledge.chunks", { n: d.chunk_count })}</span>
                <span className="grow" />
                <span className="time">{<RelTime iso={d.created_at} />}</span>
                {d.source ? (
                  <Button
                    size="small"
                    onClick={(e) => {
                      e.stopPropagation();
                      setEditingDoc(d);
                    }}
                  >
                    {t("knowledge.edit")}
                  </Button>
                ) : (
                  <span className="legacy-hint">{t("knowledge.legacyHint")}</span>
                )}
                <Button
                  danger
                  size="small"
                  onClick={async (e) => {
                    e.stopPropagation();
                    try {
                      await api.knowledgeDelete(d.id);
                      toast("ok", t("knowledge.deleted", { name: d.name }));
                      refresh();
                    } catch (err) {
                      toast("err", String(err));
                    }
                  }}
                >
                  {t("common.delete")}
                </Button>
              </div>
              {expanded === d.id && (
                <div className="chunks">
                  {chunks === null ? (
                    <Spinner />
                  ) : (
                    chunks.map(([id, text]) => (
                      <div key={id} className="chunk-item">
                        <pre className="raw">{text}</pre>
                        <div className="row tight end chunk-actions">
                          <Button
                            type="link"
                            size="small"
                            onClick={() => setEditingChunk({ id, content: text, doc: d.name })}
                          >
                            {t("knowledge.editChunk")}
                          </Button>
                          <Button type="link" size="small" onClick={() => toggleRevisions(id)}>
                            {t("knowledge.history")}
                          </Button>
                        </div>
                        {revChunk === id &&
                          (revisions === null ? (
                            <Spinner />
                          ) : revisions.length === 0 ? (
                            <p className="muted">{t("knowledge.noRevisions")}</p>
                          ) : (
                            <div className="chunk-revisions">
                              {revisions.map((r) => (
                                <div key={r.id} className="revision">
                                  <div className="row tight">
                                    <span className="time mono">{dateOf(r.edited_at)}</span>
                                    <span className="muted mono">#{r.id} · {r.document_name}</span>
                                    <span className="grow" />
                                    <Button type="link" size="small" onClick={() => rollback(r.id)}>
                                      {t("knowledge.rollback")}
                                    </Button>
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
                    ))
                  )}
                </div>
              )}
            </div>
          ))}
        </div>
      )}
      </>
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
            if (expanded === saved.id) reloadChunks(saved.id);
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
  const toast = useToast();

  useEffect(() => {
    let alive = true;
    api
      .knowledgeRaw(doc.name)
      .then((c) => {
        if (alive) setContent(c);
      })
      .catch((e) => {
        toast("err", String(e));
        onClose();
      });
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [doc.name]);

  const save = async () => {
    if (content === null || busy) return;
    setBusy(true);
    try {
      const r = await api.knowledgeSave(doc.name, content);
      toast("ok", t("knowledge.saved", { file: r.file, n: r.chunks }));
      onSaved(doc);
    } catch (e) {
      toast("err", String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal title={t("knowledge.editTitle", { name: doc.name })} onClose={onClose} wide>
      <p className="muted">{t("knowledge.editor")}</p>
      {content === null ? (
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
          placeholder={"# Deploy runbook\n\n1. Run scripts/release.sh from the repo root…"}
        />
      </label>
      <div className="row end">
        <Button
          type="primary"
          disabled={!name.trim() || !content.trim()}
          onClick={async () => {
            try {
              const r = await api.knowledgeIngest(name.trim(), content);
              toast("ok", t("knowledge.ingested", { n: r.chunks }));
              onIngested();
            } catch (e) {
              toast("err", String(e));
            }
          }}
        >
          {t("knowledge.ingest.btn")}
        </Button>
      </div>
    </Modal>
  );
}
