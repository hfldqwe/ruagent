-- M3 graph: entity property graph with bi-temporal edges (design SS6.6 #1).
-- T  = valid_at / invalid_at   (when the fact was true in the world)
-- T' = created_at / expired_at (when we recorded/invalidated it)
-- Edges are invalidated, never deleted: "what was true as of X" is always
-- answerable.

CREATE TABLE entities (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL,
    norm_name  TEXT NOT NULL,          -- lowercase-trimmed: cheap resolution
    kind       TEXT,                   -- person | project | repo | tool | concept ...
    summary    TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_entities_norm ON entities(norm_name);
CREATE VIRTUAL TABLE entities_fts USING fts5(
    name, summary,
    content='entities',
    content_rowid='id',
    tokenize='unicode61'
);

CREATE TABLE entity_edges (
    id             INTEGER PRIMARY KEY,
    src            INTEGER NOT NULL REFERENCES entities(id),
    dst            INTEGER NOT NULL REFERENCES entities(id),
    relation       TEXT NOT NULL,
    fact_text      TEXT NOT NULL,
    valid_at       TEXT NOT NULL,      -- T: fact true from
    invalid_at     TEXT,               -- T: fact true until (NULL = current)
    created_at     TEXT NOT NULL,      -- T': recorded at
    expired_at     TEXT,               -- T': invalidated at
    source_episode INTEGER
);
CREATE INDEX idx_edges_src ON entity_edges(src, relation, invalid_at);
CREATE INDEX idx_edges_dst ON entity_edges(dst, relation, invalid_at);

-- Keep entities_fts in sync with the entities table.
CREATE TRIGGER entities_ai AFTER INSERT ON entities BEGIN
    INSERT INTO entities_fts(rowid, name, summary) VALUES (new.id, new.name, COALESCE(new.summary, ''));
END;
CREATE TRIGGER entities_au AFTER UPDATE ON entities BEGIN
    INSERT INTO entities_fts(entities_fts, rowid, name, summary) VALUES ('delete', old.id, old.name, COALESCE(old.summary, ''));
    INSERT INTO entities_fts(rowid, name, summary) VALUES (new.id, new.name, COALESCE(new.summary, ''));
END;
CREATE TRIGGER entities_ad AFTER DELETE ON entities BEGIN
    INSERT INTO entities_fts(entities_fts, rowid, name, summary) VALUES ('delete', old.id, old.name, COALESCE(old.summary, ''));
END;
