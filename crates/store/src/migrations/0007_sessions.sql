-- M7: unified session history index — auto-synced from every agent
-- CLI's local store (claude-code / dsh / ruagent). Files are indexed
-- by (path, mtime); message bodies stay in the source files and are
-- parsed on demand.

CREATE TABLE IF NOT EXISTS sessions (
    key           TEXT PRIMARY KEY,   -- '<source>:<hash of ref_path>'
    source        TEXT NOT NULL,      -- claude-code | dsh | ruagent
    title         TEXT,
    project       TEXT,               -- decoded working dir
    ref_path      TEXT NOT NULL,      -- source file (parsed lazily)
    started_at    INTEGER NOT NULL,   -- epoch ms
    updated_at    INTEGER NOT NULL,
    mtime_ms      INTEGER NOT NULL,   -- incremental-scan marker
    size_bytes    INTEGER NOT NULL,
    message_count INTEGER NOT NULL DEFAULT 0,
    preview       TEXT
);

CREATE INDEX IF NOT EXISTS sessions_updated ON sessions (updated_at DESC);
CREATE INDEX IF NOT EXISTS sessions_source ON sessions (source);
