-- M6 knowledge files: markdown documents as the source of truth
-- (design-study: memsearch / EverOS), parent sections + chunk spans
-- (WeKnora parent-child chunking), and chunk revision history
-- (WeKnora chunk editing).

-- A section = one heading + its paragraphs; the retrieval unit stays the
-- chunk, the RETURN unit becomes the section (parent block).
CREATE TABLE chunk_sections (
    id          INTEGER PRIMARY KEY,
    document_id INTEGER NOT NULL REFERENCES documents(id),
    idx         INTEGER NOT NULL,
    content     TEXT NOT NULL
);

-- Byte spans of each chunk in the source document: file-backed chunk
-- edits rewrite the .md file through them. Nullable for rows written
-- before this migration (re-ingest fills them in).
ALTER TABLE chunks ADD COLUMN span_start INTEGER;
ALTER TABLE chunks ADD COLUMN span_end INTEGER;
ALTER TABLE chunks ADD COLUMN section_id INTEGER REFERENCES chunk_sections(id);

-- Edit history. No foreign keys on purpose: revisions outlive the
-- chunk (a reindex replaces chunk ids) and the document (rollback may
-- happen after a delete + re-save); `document_name` is denormalized so
-- history stays readable either way.
CREATE TABLE chunk_revisions (
    id            INTEGER PRIMARY KEY,
    chunk_id      INTEGER NOT NULL,
    document_id   INTEGER NOT NULL,
    document_name TEXT NOT NULL,
    old_content   TEXT NOT NULL,
    new_content   TEXT NOT NULL,
    edited_at     TEXT NOT NULL
);
