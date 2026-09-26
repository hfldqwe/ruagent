-- Memory lifecycle (t251): soft delete on the row, and recall-log provenance.
--
-- WHY A COLUMN, NOT A SIDE TABLE. 0017_session_deletions used a tombstone
-- table because the session indexer re-writes `sessions` with INSERT OR
-- REPLACE every 60s -- a flag on that row would be undone by the next scan.
-- Nothing rescans `memories`: every writer goes through the governed
-- pipeline (write_memory / delete_memory), so the flag survives by
-- construction and "what was deleted" stays answerable from the row itself.
--
-- WHY NOT REUSE superseded_at. Supersession is a correction ("this was
-- replaced by that"); deletion is a retraction. Collapsing them into one
-- column makes "was this replaced, or removed?" undecidable from the data.
--
-- NULL on every row written before this migration: NULL means LIVE, which is
-- what those rows are.

ALTER TABLE memories ADD COLUMN deleted_at TEXT;

-- Recall provenance. The caller DECLARES its own source at the write point
-- (?source= or the x-ruagent-recall-source header); the daemon never infers
-- it from the request. NULL is left NULL: it means UNKNOWN, not "user".
--
-- Every one of the 574 rows written before this migration has source = NULL.
-- They must be rendered as "unknown (pre-0018)" -- never defaulted to a real
-- caller, never guessed from the query text, and never filtered out of the
-- listing (dropping them would turn 574 rows of history into 0 and look
-- tidier while silently losing the sample).
ALTER TABLE recall_log ADD COLUMN source TEXT;
