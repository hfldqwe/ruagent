-- 0022: wiki integrity (R-D / gen2-wiki-spec E.2 DDL-1..7; t6 = I-SCHEMA).
--
-- The wiki's pages are markdown files on disk; everything here is DERIVED state
-- that can be rebuilt, so a lost row is a lost reading, never lost content.
-- The spec's DDL is taken almost verbatim; the two places this file departs
-- from it are marked DEVIATION and justified with a measurement, not a taste.

-- ---------------------------------------------------------------------------
-- DDL-1: derived page state.
-- ---------------------------------------------------------------------------
-- New table, no history: the NOT NULLs are honest.
CREATE TABLE IF NOT EXISTS wiki_pages (
    slug              TEXT PRIMARY KEY,
    title             TEXT NOT NULL DEFAULT '',
    summary           TEXT NOT NULL DEFAULT '',
    sources_json      TEXT NOT NULL DEFAULT '[]',
    citations_json    TEXT NOT NULL DEFAULT '[]',
    sections          INTEGER NOT NULL DEFAULT 0,
    cited_sections    INTEGER NOT NULL DEFAULT 0,
    cite_coverage     REAL NOT NULL DEFAULT 0.0,
    verified          TEXT NOT NULL DEFAULT 'unverified',
    built_at          TEXT,
    build_id          INTEGER,
    stale             INTEGER NOT NULL DEFAULT 0,
    -- The observation time at which drift was FIRST seen. Written once: a later
    -- scan that still sees drift must not move it forward, or "how long has this
    -- page been stale" becomes "when did we last look".
    stale_since       TEXT,
    stale_sources_json TEXT NOT NULL DEFAULT '[]',
    edited            INTEGER NOT NULL DEFAULT 0,
    links_in          INTEGER NOT NULL DEFAULT 0,
    links_out         INTEGER NOT NULL DEFAULT 0,
    links_out_broken  INTEGER NOT NULL DEFAULT 0,
    self_links        INTEGER NOT NULL DEFAULT 0,
    uncertain_markers INTEGER NOT NULL DEFAULT 0,
    content_hash      TEXT,
    updated_at        TEXT NOT NULL
);

-- ---------------------------------------------------------------------------
-- DDL-2: citation anchors, so drift can propagate page <- chunk.
-- ---------------------------------------------------------------------------
-- DELIBERATELY NO FOREIGN KEY to wiki_pages(slug): a citation is written while
-- a page is being built, and forcing the page row to exist first would impose a
-- write order on I-D that its own code does not have. The spec's DDL has no FK
-- either; this note records that it was a decision, not an oversight.
CREATE TABLE IF NOT EXISTS wiki_citations (
    page_slug  TEXT NOT NULL,
    section    TEXT NOT NULL,
    document   TEXT NOT NULL,
    chunk_id   INTEGER NOT NULL,
    chunk_hash TEXT NOT NULL,
    build_id   INTEGER,
    PRIMARY KEY (page_slug, section, chunk_id)
);
CREATE INDEX IF NOT EXISTS idx_wiki_citations_chunk ON wiki_citations(chunk_id);
CREATE INDEX IF NOT EXISTS idx_wiki_citations_doc   ON wiki_citations(document);

-- ---------------------------------------------------------------------------
-- DDL-3 + DDL-4 (route B): one vocabulary, and a constraint that holds.
-- ---------------------------------------------------------------------------
--
-- PRE-BACKFILL READING (live DB, read-only, 2026-09-27T22:3x+08:00), recorded
-- here because the backfill CHANGES these values and the record must survive
-- the change (§7.77: do not overwrite a source with a better-looking value):
--   wiki_builds rows: 8.
--     id=1,4  status='planned',      dry_run=1, finished_at=NULL
--     id=7,8  status='planned_only', dry_run=1, finished_at=<set>
--     id=2,3,5,6 status='done',      dry_run=0, finished_at=<set>
--   violations of `(status='planned') = (dry_run=1)`      : 2 rows (id 7,8)
--   violations of `status='planned' => finished_at NOT NULL`: 2 rows (id 1,4)
-- The two statements below are exactly what closes both.
--
-- The SECOND update is a BACKFILL, NOT A FACT: it stamps started_at as
-- finished_at for dry runs that ended before the product began setting it.
-- A dry run IS terminal the moment its plan exists (wiki.rs:767-773), so the
-- claim is true, but the timestamp is inherited, not observed. It is recorded
-- here for that reason.
UPDATE wiki_builds SET status = 'planned'
 WHERE dry_run = 1 AND status = 'planned_only';
UPDATE wiki_builds SET finished_at = COALESCE(finished_at, started_at)
 WHERE dry_run = 1 AND finished_at IS NULL;

-- DEVIATION 1 OF 2: DDL-4's CHECKs are NOT implemented as CHECK constraints,
-- and no table rebuild is performed. Two independent reasons, both verified:
--
-- (a) THE REBUILD CANNOT RUN SAFELY IN THIS RUNNER. `wiki_builds` is the parent
--     of `wiki_build_pages(build_id REFERENCES wiki_builds(id))` and holds 41
--     live child rows. `Db::configure_and_spawn` sets `foreign_keys = ON`
--     (sqlite.rs:49), so `DROP TABLE wiki_builds` performs an implicit DELETE
--     that violates the immediate FK and aborts; the standard 12-step rebuild
--     requires `PRAGMA foreign_keys=OFF`, which is a NO-OP INSIDE A TRANSACTION
--     and `migrations::apply` was not transactional. A migration that cannot be
--     re-run after a partial failure is not worth a constraint we can get
--     another way. (This file also makes `apply` transactional, which is the
--     prerequisite for the rebuild ever becoming viable -- see migrations.rs.)
--
-- (b) THE SECOND CHECK IS NOT EXPRESSIBLE AS A CONSTRAINT AT ALL, by the
--     product's own write order: `insert_build` (wiki.rs:1642) writes
--     status='planned_only' with finished_at UNSET, and `finish_dry_run`
--     (wiki.rs:1845) stamps finished_at in a SEPARATE statement. At the instant
--     of the INSERT the invariant is legitimately false. A CHECK would reject
--     every dry-run build; a trigger could not do better, because a trigger
--     cannot see the future. The invariant is therefore delivered as a READING
--     (the view below) whose required value is 0 rows -- a form a verifier can
--     falsify -- instead of a constraint that would have to break the product.
--
-- What IS enforceable is the defect the spec actually names ("同一事实两份、
-- 可能不一致"): the plan-only vocabulary and the dry_run flag must agree. It is
-- enforced by triggers on the EXISTING table, which needs no rebuild.
--
-- DEVIATION 2 OF 2: the plan-only vocabulary accepted here is BOTH 'planned'
-- and 'planned_only'. The live writer still emits 'planned_only'
-- (DRY_RUN_STATUS, wiki.rs:1838) and I-D's E3 changes it to 'planned'. Accepting
-- both is what makes this migration landable BEFORE that code change: a strict
-- `status='planned'` trigger would abort every dry-run build between this
-- migration and t10. The narrowing to one value is a follow-up migration, to be
-- taken with I-D's change in hand.
CREATE TRIGGER IF NOT EXISTS wiki_builds_plan_pairing_ins BEFORE INSERT ON wiki_builds
WHEN (new.status IN ('planned', 'planned_only')) <> (new.dry_run = 1)
BEGIN
    SELECT RAISE(ABORT, 'wiki_builds: plan-only status and dry_run must agree');
END;

-- Only a CONFORMING row is protected from becoming non-conforming. A row that
-- is already non-conforming (a legacy row this migration did not normalise) must
-- stay repairable: an unconditional trigger would make it impossible to update
-- such a row back into conformance, i.e. it would freeze the very defect.
CREATE TRIGGER IF NOT EXISTS wiki_builds_plan_pairing_upd BEFORE UPDATE ON wiki_builds
WHEN ((old.status IN ('planned', 'planned_only')) = (old.dry_run = 1))
 AND ((new.status IN ('planned', 'planned_only')) <> (new.dry_run = 1))
BEGIN
    SELECT RAISE(ABORT, 'wiki_builds: plan-only status and dry_run must agree');
END;

-- The reading that stands in for the unimplementable second CHECK. Required
-- value on a healthy database: 0 rows.
CREATE VIEW IF NOT EXISTS wiki_builds_unfinished_plans AS
SELECT id, scope, status, dry_run, started_at, finished_at
  FROM wiki_builds
 WHERE dry_run = 1 AND finished_at IS NULL;

-- ---------------------------------------------------------------------------
-- DDL-6: the link-graph reading of one build.
-- ---------------------------------------------------------------------------
-- Without this row, "did this build make the graph better or worse" had no
-- answer: the numbers were computed and thrown away. One row per completed
-- build, keyed by it, so the reading is bounded by the number of builds.
CREATE TABLE IF NOT EXISTS wiki_graph_readings (
    build_id    INTEGER PRIMARY KEY REFERENCES wiki_builds(id),
    nodes       INTEGER NOT NULL,
    edges       INTEGER NOT NULL,
    broken      INTEGER NOT NULL,
    orphans     INTEGER NOT NULL,
    unreachable INTEGER NOT NULL,
    self_links  INTEGER NOT NULL,
    read_at     TEXT NOT NULL
);

-- ---------------------------------------------------------------------------
-- DDL-7: the correction loop.
-- ---------------------------------------------------------------------------
-- `reason` and `author` are NOT NULL with a non-empty CHECK: this is the one
-- place in the wiki pipeline where a human action gets recorded, and an empty
-- reason would make "记下为什么改" optional in the only table meant to enforce it.
CREATE TABLE IF NOT EXISTS wiki_corrections (
    id     INTEGER PRIMARY KEY,
    slug   TEXT NOT NULL,
    kind   TEXT NOT NULL CHECK (kind IN ('pin', 'release', 'note')),
    reason TEXT NOT NULL CHECK (length(trim(reason)) > 0),
    author TEXT NOT NULL CHECK (length(trim(author)) > 0),
    at     TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_wiki_corrections_slug ON wiki_corrections(slug, at);

-- ---------------------------------------------------------------------------
-- NO SCHEMA CHANGE: asks whose answer is "already allowed".
-- ---------------------------------------------------------------------------

-- 1. DDL-5 (route B): I-D will write wiki_build_pages rows with a status value
--    distinguishing a plan-only page from a pending one. `wiki_build_pages.status`
--    is plain `TEXT NOT NULL` (0010_wiki.sql:25) with no CHECK, so a new value
--    needs no migration. The 22 live `pending` rows under dry-run builds are
--    NOT cleaned up here: the spec places that cleanup on the product path
--    ("现场 22 行 pending 的清理由产品路径完成，不在迁移里"), and a migration
--    that guesses which of them are obsolete would be inventing data.
-- 2. Route A (drop `dry_run`) is explicitly P1-later in the spec; not taken.
