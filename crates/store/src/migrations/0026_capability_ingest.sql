-- 0026: the knowledge -> graph ingestion ledger (t4,
-- docs/plans/capability-plugins-design.md §13.3).
--
-- WHY A LEDGER AT ALL: the ingestion (crates/daemon/src/knowledge_graph.rs) is
-- driven by the EXISTING 60 s knowledge scan loop, so without a record of what
-- has already been written it would re-extract every document on every pass.
-- The graph cannot answer "was this document already ingested": it cannot
-- distinguish "this document has nothing to extract" from "this document was
-- extracted and produced nothing", and re-running extraction on unchanged text
-- is exactly the waste the ledger exists to prevent.
--
-- ONE ROW PER DOCUMENT NAME, NOT PER (document_id, content_hash). The design
-- document's DDL declares UNIQUE(document_id, content_hash); this migration
-- deliberately does not, because `Knowledge::index_doc` DELETES the documents
-- row and INSERTs a new one on every content change
-- (crates/knowledge/src/store.rs:652-654 and 672-683), so `documents.id` is
-- reassigned on every revision:
--
--   * keyed on document_id, "the same document, new revision" and "a new
--     document" are indistinguishable, and the revision rule below (a changed
--     document retracts what its previous revision contributed) would have no
--     key to hang on;
--   * `documents.name` IS stable: it is the upsert key of index_doc
--     (store.rs:638, "SELECT id, content_hash FROM documents WHERE name = ?1")
--     and the argument of `Knowledge::read_raw` (files.rs:239).
--
-- So the ledger holds the CURRENT revision of each document. `document_id` is
-- kept as an informational column (the id the current revision happens to
-- have); nothing keys on it.
--
-- WHAT THE LEDGER MEMORISES BEYOND THE COUNT: `entities_json` and `facts_json`
-- are the entity names and fact hashes THIS revision wrote. They are what makes
-- the revision rule implementable at all: when a document changes, the previous
-- revision's contributions must be retracted, and the only record of them is
-- this row (the new revision's text no longer mentions them, and re-deriving
-- them from the graph is impossible -- the graph does not record which document
-- a fact came from). Both are JSON arrays of strings.
--
-- IDEMPOTENT: the table is created with IF NOT EXISTS and nothing is backfilled,
-- so a database that already ran it is unaffected.

CREATE TABLE IF NOT EXISTS knowledge_graph_ingest (
    id            INTEGER PRIMARY KEY,
    document_name TEXT    NOT NULL UNIQUE,
    -- Informational: the `documents.id` of the revision recorded here. Reassigned
    -- by index_doc on the next revision; never a key.
    document_id   INTEGER NOT NULL,
    -- The sha256 (ruagent_knowledge::sha256_hex) of the markdown THIS revision
    -- was extracted from. A row with this hash means "this exact text is already
    -- in the graph".
    content_hash  TEXT    NOT NULL,
    -- What the write did, for the status route ("is my knowledge base in the
    -- graph?").
    entities      INTEGER NOT NULL DEFAULT 0,
    relations     INTEGER NOT NULL DEFAULT 0,
    candidates    INTEGER NOT NULL DEFAULT 0,
    -- The entity names this revision wrote (JSON array of strings).
    entities_json TEXT    NOT NULL DEFAULT '[]',
    -- The `ruagent_graph::fact_hash` of every fact this revision stated
    -- (JSON array of strings).
    facts_json    TEXT    NOT NULL DEFAULT '[]',
    ingested_at   TEXT    NOT NULL
);
