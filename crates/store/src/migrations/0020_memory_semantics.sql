-- 0020: memory semantics (R-B / gen2-memory-spec E.3, E.4, E.7; t6 = I-SCHEMA).
--
-- THE RULE THIS FILE APPLIES, stated once because three columns below deviate
-- from the literal type a spec asked for:
--
--   A column added to a table that already has rows is NULLABLE WITH NO DEFAULT
--   unless the rows' value for it is genuinely knowable. NULL means "unknown,
--   because this row predates this column" -- the rule 0018 set for
--   recall_log.source ("NULL is left NULL: it means UNKNOWN, not 'user'").
--
-- `NOT NULL DEFAULT x` on ALTER TABLE does not just constrain future writes: it
-- BACKFILLS every existing row with `x`, which is an assertion about the past
-- that the migration cannot support.
--   * gen2-memory-spec:843 asks for `access_count INTEGER NOT NULL DEFAULT 0`.
--     That would stamp "used 0 times" onto 163 historical rows -- readable as
--     evidence they were never used, when the truth is that no counter existed.
--   * gen2-graph-spec:361 asks for `status TEXT NOT NULL DEFAULT 'ok'` on
--     distill_log. Measured live: 18 of 33 distill_log rows have all three
--     counters = 0 (mem-core's reading, 2026-09-27), i.e. rows whose outcome is
--     exactly what this column is meant to distinguish. Defaulting them all to
--     'ok' would assert 33 successes out of 33 -- the opposite of the finding.
-- A NULLABLE column keeps the distinction available; a reader that needs a
-- number uses COALESCE(access_count, 0), and a reader that needs to know
-- whether the number is knowledge or a convention can ask.
--
-- The read-side contract for every column below is therefore three-state:
--   NULL  = unknown (row predates the column)
--   value = recorded by the writer that owns the column
-- and nothing may collapse NULL into the value.

-- ---------------------------------------------------------------------------
-- memories: usage state (C3) and validity window (B.2, adopted).
-- ---------------------------------------------------------------------------

-- Injection/recall hits write these back so the decay ordering
-- (score = base + lambda * exp(-delta_t / (1 + access_count))) has an input.
-- Historical rows: NULL = no counter existed, NOT "zero uses".
ALTER TABLE memories ADD COLUMN access_count INTEGER;
-- When this memory was last returned to an agent. NULL = never recorded.
ALTER TABLE memories ADD COLUMN last_used_at TEXT;

-- The validity window (valid time = when the fact was true in the world), as
-- opposed to created_at/updated_at (when the row was written).
--
-- DELIBERATELY NOT BACKFILLED, and this is a deviation from B.2's cost note
-- ("迁移只能回填 created_at（这是回填，不是事实）"). Filling them with
-- created_at would put a number in the row that measures something else
-- entirely. Leaving them NULL lets a reader fall back EXPLICITLY
-- (COALESCE(valid_from, created_at), COALESCE(valid_to, superseded_at)) and
-- keeps "this row has an explicit validity" distinguishable from "this row is
-- being approximated".
ALTER TABLE memories ADD COLUMN valid_from TEXT;
ALTER TABLE memories ADD COLUMN valid_to   TEXT;

-- ---------------------------------------------------------------------------
-- memory_sources: where a consolidated memory came from (E.4).
-- ---------------------------------------------------------------------------

-- A consolidated memory must be able to name its sources (memory ids or episode
-- ids). This is also the reverse index the residue scan needs: "given an
-- episode, which memories were derived from it" had no answer before.
--
-- New table: no history, so the NOT NULLs are honest.
CREATE TABLE IF NOT EXISTS memory_sources (
    memory_id   INTEGER NOT NULL REFERENCES memories(id),
    source_kind TEXT NOT NULL,   -- 'memory' | 'episode'
    source_id   INTEGER NOT NULL,
    created_at  TEXT NOT NULL,
    PRIMARY KEY (memory_id, source_kind, source_id)
);
-- The residue-scan direction: given a source, which memories claim it.
CREATE INDEX IF NOT EXISTS idx_memory_sources_source ON memory_sources(source_kind, source_id);

-- ---------------------------------------------------------------------------
-- distill_log: three-state outcome + prompt attribution (E.7; asked by BOTH
-- gen2-memory-spec:883 and gen2-graph-spec:361).
-- ---------------------------------------------------------------------------

-- ONE COLUMN, ONE NAME. memory's prose called the fingerprint
-- `prompt_fingerprint`, graph's DDL called it `prompt_hash`. The schema owner
-- picks one: `prompt_hash` (graph's DDL is the one written as DDL; memory's is
-- prose). Registered in docs/design/reviews/gen2-store-impl.md §2 so neither
-- owner has to guess.
--
-- `status` is NULLable (see the rule at the top). The intended vocabulary is
-- ok | empty | failed, applied by the writer from this migration on; a NULL
-- row is a pre-0020 row whose outcome was never recorded and must NOT be
-- rendered as ok.
ALTER TABLE distill_log ADD COLUMN status         TEXT;
ALTER TABLE distill_log ADD COLUMN failure_reason TEXT;
ALTER TABLE distill_log ADD COLUMN prompt_hash    TEXT;

-- ---------------------------------------------------------------------------
-- NO SCHEMA CHANGE: the two asks whose answer is "the schema already allows it".
-- ---------------------------------------------------------------------------

-- 1. memory_spec:833 asks whether a new `memory_diffs.op` value
--    ('merge_judged', 'consolidate') needs a migration, "若 I-SCHEMA 的 op
--    白名单是硬约束则登记". Verified: there is NO CHECK on memory_diffs.op
--    (0004_memory.sql:50-59 declares it plain `TEXT NOT NULL`), and the live
--    table already holds SEVEN distinct ops (insert, purge, reject,
--    skip_dedupe, supersede, delete, restore) while the column comment lists
--    four. A new op value needs no migration. The stale comment cannot be fixed
--    here: 0004 is a historical migration file and is frozen.
-- 2. `episodes.kind` is likewise plain TEXT with no CHECK (0004_memory.sql:5-14),
--    so a new kind needs no migration.
