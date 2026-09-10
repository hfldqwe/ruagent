-- M3 memory schema (design SS6, blueprint SS6.6).

-- Episodes: the non-lossy base layer. Raw turns/documents land here first,
-- content-hash deduped; everything derived references back to an episode.
CREATE TABLE episodes (
    id           INTEGER PRIMARY KEY,
    kind         TEXT NOT NULL,          -- run_turn | document | manual | mcp_write
    content      TEXT NOT NULL,
    content_hash TEXT NOT NULL UNIQUE,
    ref_time     TEXT NOT NULL,          -- event time (T)
    ingested_at  TEXT NOT NULL,          -- transaction time (T')
    source_run   TEXT,                   -- runs.id when from a run
    meta         TEXT
);

-- Memories: typed, namespaced, supersessible (design SS6.1 six stores;
-- entities/edges live in the graph tables, decisions below).
CREATE TABLE memories (
    id             INTEGER PRIMARY KEY,
    store          TEXT NOT NULL,        -- profile | observation | procedure | lesson
    namespace      TEXT NOT NULL,         -- user | global | project:<x> | agent:<x>
    content        TEXT NOT NULL,
    content_hash   TEXT NOT NULL,
    confidence     REAL NOT NULL DEFAULT 0.5,
    source_episode INTEGER REFERENCES episodes(id),
    supersedes     INTEGER REFERENCES memories(id),
    superseded_at  TEXT,                  -- NULL = current
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL,
    UNIQUE (store, namespace, content_hash)
);
CREATE INDEX idx_memories_current ON memories(store, namespace, superseded_at);

-- Full-text search over current memories (hybrid retrieval leg 1).
CREATE VIRTUAL TABLE memories_fts USING fts5(
    content,
    content='memories',
    content_rowid='id',
    tokenize='unicode61'
);
-- keep the index in sync
CREATE TRIGGER memories_ai AFTER INSERT ON memories BEGIN
    INSERT INTO memories_fts(rowid, content) VALUES (new.id, new.content);
END;
CREATE TRIGGER memories_ad AFTER DELETE ON memories BEGIN
    INSERT INTO memories_fts(memories_fts, rowid, content) VALUES ('delete', old.id, old.content);
END;

-- Audit log: every write path decision is recorded (design SS6.6 #7).
CREATE TABLE memory_diffs (
    id        INTEGER PRIMARY KEY,
    ts        TEXT NOT NULL,
    op        TEXT NOT NULL,              -- insert | supersede | skip_dedupe | reject
    mem_store TEXT,
    namespace TEXT,
    before    TEXT,
    after     TEXT,
    reason    TEXT
);

-- Session context snapshots: replaced, never appended (design SS6.1).
CREATE TABLE session_contexts (
    run_id     TEXT PRIMARY KEY,
    goal       TEXT,
    plan       TEXT,
    progress   TEXT,
    updated_at TEXT NOT NULL
);

-- Decision log with outcomes (design SS6.1; feeds capability stats/evals).
CREATE TABLE decisions (
    id              INTEGER PRIMARY KEY,
    run_id          TEXT,
    decision        TEXT NOT NULL,
    reasoning       TEXT,
    outcome         TEXT,
    outcome_quality REAL,
    created_at      TEXT NOT NULL
);
