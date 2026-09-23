-- Session lifecycle: ruagent-side "deleted" tombstone.
--
-- Same reasoning as 0016, one step stronger. The indexer writes `sessions`
-- with INSERT OR REPLACE on every 60s rescan, so a deletion recorded only as a
-- missing row is undone as soon as the source file is walked again. Measured
-- before this table existed: a deleted ruagent session was back in the list
-- within 70s — the button said "delete" and the row came back on its own.
-- A tombstone in a side table survives the rescan, and the list filter (which
-- already runs in the daemon) honours it in every mode.
--
-- Deleting is still ruagent-side only: the transcript file is left exactly as
-- it was (for ruagent's own sessions it is also the run's event log), so
-- nothing on this path touches the filesystem.

CREATE TABLE IF NOT EXISTS session_deletions (
    key        TEXT PRIMARY KEY,   -- sessions.key ('<source>:<hash of ref_path>')
    deleted_at INTEGER NOT NULL    -- epoch ms
);
