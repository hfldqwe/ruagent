// Knowledge manager: documents, ingest, hybrid search with chunk preview.

import { useEffect, useState } from "react";
import { api, type KnowledgeDocument, type SearchHit } from "../api";
import { Empty, Modal, RelTime, Spinner, useToast } from "../ui";
import { useI18n } from "../i18n";

export function Knowledge() {
  const { t } = useI18n();
  const [docs, setDocs] = useState<KnowledgeDocument[] | null>(null);
  const [embedder, setEmbedder] = useState("");
  const [ingesting, setIngesting] = useState(false);
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<SearchHit[] | null>(null);
  const [expanded, setExpanded] = useState<number | null>(null);
  const [chunks, setChunks] = useState<[number, string][] | null>(null);
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

  const openChunks = async (id: number) => {
    if (expanded === id) {
      setExpanded(null);
      setChunks(null);
      return;
    }
    setExpanded(id);
    setChunks(null);
    try {
      setChunks(await api.knowledgeChunks(id));
    } catch (e) {
      toast("err", String(e));
      setChunks([]);
    }
  };

  return (
    <div>
      <div className="view-bar">
        <h2>{t("knowledge.title")}</h2>
        <span className="muted">{t("knowledge.subtitle")} ({embedder || "…"})</span>
        <span className="grow" />
        <button className="primary" onClick={() => setIngesting(true)}>
          + {t("knowledge.ingest")}
        </button>
      </div>

      <div className="search-bar">
        <input
          autoFocus
          className="grow"
          placeholder={t("knowledge.search")}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && search()}
        />
        <button className="primary" onClick={search}>
          {t("common.search")}
        </button>
        {hits ? (
          <button className="link" onClick={() => { setHits(null); setQuery(""); }}>
            {t("common.clear")}
          </button>
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
                <span className="doc-icon">📄</span>
                <strong>{d.name}</strong>
                <span className="muted">{t("knowledge.chunks", { n: d.chunk_count })}</span>
                <span className="grow" />
                <span className="time">{<RelTime iso={d.created_at} />}</span>
                <button
                  className="danger sm"
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
                  Delete
                </button>
              </div>
              {expanded === d.id && (
                <div className="chunks">
                  {chunks === null ? (
                    <Spinner />
                  ) : (
                    chunks.map(([id, text]) => (
                      <pre key={id} className="raw">
                        {text}
                      </pre>
                    ))
                  )}
                </div>
              )}
            </div>
          ))}
        </div>
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
    </div>
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
        <input
          autoFocus
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder={t("knowledge.ingest.namePh")}
        />
      </label>
      <label className="field">
        <span>{t("knowledge.ingest.content")}</span>
        <textarea
          rows={12}
          value={content}
          onChange={(e) => setContent(e.target.value)}
          placeholder="# Deploy runbook&#10;&#10;1. Run scripts/release.sh from the repo root…"
        />
      </label>
      <div className="row end">
        <button
          className="primary"
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
        </button>
      </div>
    </Modal>
  );
}
