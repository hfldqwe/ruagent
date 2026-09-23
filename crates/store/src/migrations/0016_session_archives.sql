-- Session lifecycle: ruagent-side "archived" (hidden) marker.
--
-- Deliberately a SIDE TABLE, not a column on `sessions`: the indexer writes
-- that table with INSERT OR REPLACE (crates/daemon/src/sessions.rs), so any
-- column it does not set is reset to its default on every 60s rescan — a hide
-- stored there would silently undo itself. A side table survives rescans, and
-- it needs no change to the index (which only reads other tools' files).
--
-- Archiving never touches the source file the session points at: this table
-- holds a key and a timestamp, nothing else.

CREATE TABLE IF NOT EXISTS session_archives (
    key         TEXT PRIMARY KEY,   -- sessions.key ('<source>:<hash of ref_path>')
    archived_at INTEGER NOT NULL    -- epoch ms
);
