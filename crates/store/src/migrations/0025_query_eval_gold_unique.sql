-- 0025: `query_eval_gold` gets the uniqueness its seed already assumed.
--
-- THE DEFECT (V-A F-1, reproduced 2026-09-28T01:2x+08:00 with the real seed code
-- on a scratch database, six consecutive runs):
--
--   run 1: COUNT(*)=22  SUM(answerable)=15   distinct(set_id,query)=22
--   run 2: COUNT(*)=44  SUM(answerable)=30   distinct(set_id,query)=22
--   run 3: COUNT(*)=66  SUM(answerable)=45   distinct(set_id,query)=22
--   run 4: COUNT(*)=88  SUM(answerable)=60   distinct(set_id,query)=22
--   run 5: COUNT(*)=110 SUM(answerable)=75   distinct(set_id,query)=22
--   run 6: COUNT(*)=132 SUM(answerable)=90   distinct(set_id,query)=22
--
-- `crates/knowledge/tests/common/mod.rs` seeds with `INSERT OR IGNORE` and its
-- doc comment called that idempotent. `INSERT OR IGNORE` only ignores a conflict
-- that a UNIQUE constraint defines, and 0019 declared none on `(set_id, query)`
-- -- so every run appended the whole 22-query set again. The set never grew; the
-- TABLE did. Nothing about today's metrics changes (they read the frozen `GOLD`
-- constant), and that is exactly why this is dangerous: the size of this table
-- is the premise a future target is read from ("at least 60 queries, at least 20
-- unanswerable" -- R-A C3), and reading it off `COUNT(*)` on an inflated table
-- awards a target nobody met.
--
-- WHAT THIS MIGRATION DOES
--   1. collapses duplicate `(set_id, query)` rows, keeping the EARLIEST id per
--      pair (the earliest is the row the first seed wrote; later copies carry
--      nothing the first does not -- the seed writes the same values every run),
--   2. then creates the unique index that makes `INSERT OR IGNORE` mean what its
--      name says.
--
-- SAFE ON THE REAL PATH: the live database is at version 18, where this table
-- does not exist yet, so 0019 creates it empty and step 1 deletes nothing. On a
-- development copy that ran the buggy seed (measured: one copy held 308 rows =
-- 22 x 14) step 1 collapses it to the 22 rows that were actually frozen.
--
-- `id INTEGER PRIMARY KEY` is kept: changing a row's label later (for example a
-- human re-judging a query, `judged_by` -> 'human') is an UPDATE of that row, not
-- a second INSERT. One judged query is one row; the provenance lives in a column
-- (0019's own comment on `judged_by`), so `(set_id, query)` is the natural key.

DELETE FROM query_eval_gold
WHERE id NOT IN (SELECT MIN(id) FROM query_eval_gold GROUP BY set_id, query);

CREATE UNIQUE INDEX IF NOT EXISTS ux_query_eval_gold_set_query
    ON query_eval_gold(set_id, query);
