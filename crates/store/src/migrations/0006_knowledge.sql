-- M3 knowledge: documents, chunks, FTS over chunks (design SS6/SS19).

CREATE TABLE documents (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL,
    source      TEXT,
    content_hash TEXT NOT NULL,
    chunk_count INTEGER NOT NULL,
    created_at  TEXT NOT NULL
);

CREATE TABLE chunks (
    id          INTEGER PRIMARY KEY,
    document_id INTEGER NOT NULL REFERENCES documents(id),
    idx         INTEGER NOT NULL,
    content     TEXT NOT NULL
);

CREATE VIRTUAL TABLE chunks_fts USING fts5(
    content,
    content='chunks',
    content_rowid='id',
    tokenize='unicode61'
);
CREATE TRIGGER chunks_ai AFTER INSERT ON chunks BEGIN
    INSERT INTO chunks_fts(rowid, content) VALUES (new.id, new.content);
END;
CREATE TRIGGER chunks_ad AFTER DELETE ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, content) VALUES ('delete', old.id, old.content);
END;

-- Embedding metadata: the model id + dims backing the LanceDB table
-- (design SS6.6 trap #6: never mix embeddings across models).
CREATE TABLE knowledge_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
