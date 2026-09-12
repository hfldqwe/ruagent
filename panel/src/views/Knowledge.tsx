// Knowledge manager: documents, ingest, hybrid search with chunk preview.

import { useEffect, useState } from "react";
import { api, type KnowledgeDocument, type SearchHit } from "../api";
import { Empty, Modal, Spinner, relTime, useToast } from "../ui";

export function Knowledge() {
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
        <h2>Knowledge</h2>
        <span className="muted">
          documents every agent can search ({embedder || "…"})
        </span>
        <span className="grow" />
        <button className="primary" onClick={() => setIngesting(true)}>
          + Ingest Document
        </button>
      </div>

      <div className="search-bar">
        <input
          autoFocus
          className="grow"
          placeholder="Search documents (semantic + keyword)…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && search()}
        />
        <button className="primary" onClick={search}>
          Search
        </button>
        {hits ? (
          <button className="link" onClick={() => { setHits(null); setQuery(""); }}>
            clear
          </button>
        ) : null}
      </div>

      {hits ? (
        hits.length === 0 ? (
          <Empty icon="🔍" title="No results" hint="Try different keywords, or ingest more documents." />
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
        <Spinner label="Loading documents…" />
      ) : docs.length === 0 ? (
        <Empty
          icon="📚"
          title="No documents ingested"
          hint="Paste markdown or notes — they get chunked, embedded, and become searchable by every agent (knowledge_search MCP tool)."
        />
      ) : (
        <div className="card">
          {docs.map((d) => (
            <div key={d.id}>
              <div
                role="button"
                tabIndex={0}
                aria-label={`Toggle chunks of ${d.name}`}
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
                <span className="muted">{d.chunk_count} chunks</span>
                <span className="grow" />
                <span className="time">{relTime(d.created_at)}</span>
                <button
                  className="danger sm"
                  onClick={async (e) => {
                    e.stopPropagation();
                    try {
                      await api.knowledgeDelete(d.id);
                      toast("ok", `deleted ${d.name}`);
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
  const toast = useToast();
  return (
    <Modal title="Ingest document" onClose={onClose} wide>
      <label className="field">
        <span>Name</span>
        <input
          autoFocus
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="deploy-runbook"
        />
      </label>
      <label className="field">
        <span>Content (markdown / notes / code — chunked at ~800 chars with overlap)</span>
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
              toast("ok", `ingested ${r.chunks} chunks`);
              onIngested();
            } catch (e) {
              toast("err", String(e));
            }
          }}
        >
          Ingest
        </button>
      </div>
    </Modal>
  );
}
