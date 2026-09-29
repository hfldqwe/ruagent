-- 0024: `distill_log` becomes ONE ROW PER ATTEMPT (I-SCHEMA-2 / R-C DEP-3).
--
-- THE DEFECT THIS FIXES, measured: `distill_log` had 33 rows (2026-09-13..09-26)
-- and 18 of them have all three counters = 0, while the daemon's log over a
-- nearby window shows 22 attempts with 21 failures (95.5%) -- and only ONE row
-- reaches the database for that window. The failure rate therefore lives in the
-- log and nowhere else, because the primary key was `session_key`: every attempt
-- for a session hit the same key, and `INSERT OR REPLACE` deleted the previous
-- attempt's row. "How often does distillation fail?" was unanswerable from the
-- database, which is the whole point of keeping a log.
--
-- WHY A REBUILD RATHER THAN AN ALTER: SQLite cannot drop a PRIMARY KEY, and the
-- primary key IS the defect. There is no additive form of "this key is no longer
-- unique". Three preconditions were MEASURED on the live database before writing
-- this file (2026-09-28T00:58:47+08:00, read-only):
--   * nothing references it: 0 foreign keys, 0 views, 0 triggers mention
--     `distill_log` (so `DROP TABLE` cannot abort on a dependent object);
--   * the data copies losslessly: 33 rows, `session_key` NULL in 0 of them,
--     33 distinct keys -- so no row can collide under the new shape and the
--     `NOT NULL` added below cannot reject existing data;
--   * `migrations::apply` runs each migration inside ONE transaction (since
--     I-SCHEMA/t6), so a rebuild cannot leave a half-migrated table behind.
--
-- ⚠ WHAT THIS BREAKS, AND WHO MUST CHANGE IT (registered, not hidden):
-- with `session_key` no longer unique, a writer that uses
--   INSERT ... ON CONFLICT(session_key) DO UPDATE ...
-- now fails at PREPARE time with
--   "ON CONFLICT clause does not match any PRIMARY KEY or UNIQUE constraint".
-- The one such statement is `crates/daemon/src/distill.rs:327-343`
-- (`log_outcome`), which belongs to the graph/I-C task ("every attempt writes a
-- row; a failure must not overwrite a success"). It must become a plain INSERT
-- in the same integration window. Failing loudly is the intended behaviour here:
-- a stale writer that silently collapsed six attempts into one row is exactly
-- the defect this migration exists to end.

CREATE TABLE IF NOT EXISTS distill_log_attempts (
    -- Per-ATTEMPT identity. `INTEGER PRIMARY KEY` == rowid, so an insert that
    -- does not name it still gets one, in insertion order.
    id                INTEGER PRIMARY KEY,
    -- Which session the attempt was for. NOT a key any more: that is the fix.
    -- NOT NULL: every attempt belongs to a session, and all 33 existing rows
    -- satisfy it (measured above).
    session_key       TEXT    NOT NULL,
    distilled_at      TEXT    NOT NULL,
    memories_written  INTEGER NOT NULL DEFAULT 0,
    entities_written  INTEGER NOT NULL DEFAULT 0,
    relations_written INTEGER NOT NULL DEFAULT 0,
    agent             TEXT,
    -- The three-state outcome (added to the old table by 0020 and carried over
    -- here verbatim). NULL keeps its meaning: NOBODY RECORDED AN OUTCOME for
    -- this row -- do not read it as a success, and do not guess it from the
    -- counters. A pre-0024 row is exactly that case.
    status            TEXT,
    failure_reason    TEXT,
    prompt_hash       TEXT
);

-- Every historical row, NULLs included, in chronological order so the assigned
-- ids are reproducible and the oldest attempt has the smallest id.
INSERT INTO distill_log_attempts
    (session_key, distilled_at, memories_written, entities_written,
     relations_written, agent, status, failure_reason, prompt_hash)
SELECT session_key, distilled_at, memories_written, entities_written,
       relations_written, agent, status, failure_reason, prompt_hash
FROM distill_log
ORDER BY distilled_at, session_key;

DROP TABLE IF EXISTS distill_log;
ALTER TABLE distill_log_attempts RENAME TO distill_log;

-- "This session's attempts, newest last" -- the read that the old key made
-- impossible.
CREATE INDEX IF NOT EXISTS idx_distill_log_session ON distill_log(session_key, id);

-- The failure rate's index. Partial on purpose: rows whose outcome nobody
-- recorded (the pre-0024 history) are NOT part of a rate's denominator, and an
-- index that included them would invite counting them.
CREATE INDEX IF NOT EXISTS idx_distill_log_status ON distill_log(status) WHERE status IS NOT NULL;

-- The reading rule, in one place, so no reader has to remember it:
-- the denominator of a distillation success/failure rate is
-- `distill_recorded_outcomes`, NOT `distill_log`. A row with a NULL status is
-- an attempt whose outcome was never recorded (all 33 pre-0024 rows), and
-- folding it into "ok" would assert successes nobody observed -- the same
-- mistake `0020` refused to make with `NOT NULL DEFAULT 'ok'`.
CREATE VIEW IF NOT EXISTS distill_recorded_outcomes AS
    SELECT * FROM distill_log WHERE status IS NOT NULL;
