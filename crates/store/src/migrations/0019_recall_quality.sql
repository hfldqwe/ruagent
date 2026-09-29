-- 0019: recall quality (R-A / gen2-recall-spec §E10, t6 = I-SCHEMA).
--
-- Three things land here, all of them ADDITIVE:
--   1. The frozen evaluation gold set. The platform had NO labelled query set,
--      so "is retrieval good?" had no answer that a reader could falsify.
--      A3's 15 title-derived gold queries are the first version (see
--      `judged_by`).
--   2. recall_log scoring provenance. The 651 historical rows' score is an RRF
--      RANK score whose upper bound is 2/61; the columns added here carry
--      whatever the new scorer measures, and they stay NULL on every row
--      written before this migration -- NULL means UNKNOWN (pre-0019), exactly
--      the rule 0018 established for recall_log.source. They are NEVER
--      defaulted to 0: 0 is a legal score and would read as "measured, and it
--      was zero".
--   3. The CJK bigram shadow index. unicode61 makes a whole Han run ONE term,
--      so 63.20% of 2-character Han queries (6922 sampled) are reachable only
--      by a LIKE scan (measured 10.47 ms / 6.80 ms per query on 10765 chunks).
--      SQLite's `trigram` tokenizer was measured at 0/40 on those queries --
--      consistent with its own documentation ("Substrings consisting of fewer
--      than 3 unicode characters do not match any rows"). A Han-bigram index
--      answered 40/40 at 0.08 ms.
--
-- WHY `chunks.grams` IS NULL UNTIL BACKFILLED: computing bigrams needs Rust
-- (SQL cannot), so the column is added empty and filled by the write path plus
-- one idempotent pass (crates/store/src/lib.rs::Db::backfill_chunk_grams).
-- A row whose `grams` is NULL is simply not reachable by the bigram leg; it is
-- not a lie about that row's content. The FTS table is external-content over
-- `chunks.grams`, so the index and the column cannot disagree.

-- ---------------------------------------------------------------------------
-- 1. The frozen evaluation gold set.
-- ---------------------------------------------------------------------------

-- A named, versioned query set. `frozen_at` is the single flag that decides
-- whether the harness may still edit it: NULL = still being assembled,
-- non-NULL = read-only, and a metric is only comparable to another metric taken
-- over the same (set name, version).
CREATE TABLE IF NOT EXISTS query_eval_sets (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL UNIQUE,
    version    TEXT NOT NULL,
    created_at TEXT NOT NULL,
    frozen_at  TEXT
);

-- One judged query.
--
-- `judged_by` is the provenance of the LABEL, and it is not decoration:
-- 'title-derived' means "the gold document's name contains the query verbatim"
-- (cheap, reproducible, and structurally biased toward literal/title matches),
-- 'human' means a person read the corpus and graded it. A metric computed over
-- a title-derived set may NOT be reported as if it were graded relevance
-- (TREC-DL-style qrels). Collapsing the two is the defect this column exists to
-- prevent.
--
-- `answerable = 0` is a first-class row, not a missing one: an unanswerable
-- query belongs in the denominator, or the score inflates (the harness's own
-- metrics() already holds this rule, retrieval-quality.rs:280-295).
CREATE TABLE IF NOT EXISTS query_eval_gold (
    id               INTEGER PRIMARY KEY,
    set_id           INTEGER NOT NULL REFERENCES query_eval_sets(id),
    query            TEXT NOT NULL,
    -- exact-ascii | cjk-2char | cjk-run | mixed | punctuated | no-answer
    class            TEXT NOT NULL,
    gold_document_id INTEGER REFERENCES documents(id),
    answerable       INTEGER NOT NULL,
    judged_by        TEXT NOT NULL,
    note             TEXT
);
CREATE INDEX IF NOT EXISTS idx_query_eval_gold_set ON query_eval_gold(set_id, class);

-- One run of the harness over a set. Every field that makes two readings
-- INCOMPARABLE is stored: the embedder (a model switch moves every distance),
-- the leg window (widening it re-orders the fusion), the fusion expression, and
-- the raw artifact path.
CREATE TABLE IF NOT EXISTS query_eval_runs (
    id           INTEGER PRIMARY KEY,
    ts           TEXT NOT NULL,
    set_id       INTEGER NOT NULL REFERENCES query_eval_sets(id),
    embedder     TEXT NOT NULL,
    leg_window   INTEGER NOT NULL,
    fusion       TEXT NOT NULL,
    metrics_json TEXT NOT NULL,
    artifact     TEXT
);

-- ---------------------------------------------------------------------------
-- 2. recall_log: scoring provenance (NULL = unknown, pre-0019).
-- ---------------------------------------------------------------------------

-- The new scorer's value. NOT a reinterpretation of top_knowledge_score: that
-- column keeps its historical meaning (an RRF rank score, upper bound 2/61,
-- observed max exactly 2/61 = the bound) and is left untouched.
ALTER TABLE recall_log ADD COLUMN top_knowledge_relevance      REAL;
-- Which scale top_knowledge_relevance is on ('calibrated'). NULL on history.
ALTER TABLE recall_log ADD COLUMN top_knowledge_relevance_kind TEXT;
-- The leg window this row was produced with. Rows before this migration are
-- NULL: their window was `limit.max(10)`, so they are NOT comparable to a row
-- produced with a constant window (measured: 2 of 13 queries change their top-1
-- document when the window goes from 10 to 20).
ALTER TABLE recall_log ADD COLUMN knowledge_leg_window         INTEGER;
-- The scorer/calibration version. A model switch or a weight change bumps it;
-- two rows with different versions must never be plotted on one curve.
ALTER TABLE recall_log ADD COLUMN scoring_version              INTEGER;

-- ---------------------------------------------------------------------------
-- 3. CJK bigram shadow index.
-- ---------------------------------------------------------------------------

-- The bigram text for this chunk. Written by the chunk write path
-- (fts::han_bigrams) and by Db::backfill_chunk_grams.
ALTER TABLE chunks ADD COLUMN grams TEXT;

CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts_cjk USING fts5(
    grams,
    content='chunks',
    content_rowid='id',
    tokenize='unicode61'
);

-- FOUR triggers, not the three a first draft would write, and the extra one is
-- load-bearing. The obvious shape --
--   AFTER UPDATE OF grams ... BEGIN delete(old); insert(new); END
-- -- has no NULL guard, and the ONLY way this column is ever populated is a
-- NULL -> value update (the backfill). Its delete-half would therefore run the
-- FTS5 'delete' command for a row that was never inserted. Splitting the update
-- path by whether `old.grams` existed keeps every 'delete' paired with a real
-- earlier insert.
CREATE TRIGGER IF NOT EXISTS chunks_cjk_ai AFTER INSERT ON chunks
WHEN new.grams IS NOT NULL
BEGIN
    INSERT INTO chunks_fts_cjk(rowid, grams) VALUES (new.id, new.grams);
END;

-- The backfill case: nothing was indexed before, so there is nothing to delete.
CREATE TRIGGER IF NOT EXISTS chunks_cjk_au_fill AFTER UPDATE OF grams ON chunks
WHEN old.grams IS NULL AND new.grams IS NOT NULL
BEGIN
    INSERT INTO chunks_fts_cjk(rowid, grams) VALUES (new.id, new.grams);
END;

-- A real change of an already-indexed value.
CREATE TRIGGER IF NOT EXISTS chunks_cjk_au_change AFTER UPDATE OF grams ON chunks
WHEN old.grams IS NOT NULL AND new.grams IS NOT NULL AND new.grams <> old.grams
BEGIN
    INSERT INTO chunks_fts_cjk(chunks_fts_cjk, rowid, grams)
        VALUES ('delete', old.id, old.grams);
    INSERT INTO chunks_fts_cjk(rowid, grams) VALUES (new.id, new.grams);
END;

CREATE TRIGGER IF NOT EXISTS chunks_cjk_ad AFTER DELETE ON chunks
WHEN old.grams IS NOT NULL
BEGIN
    INSERT INTO chunks_fts_cjk(chunks_fts_cjk, rowid, grams)
        VALUES ('delete', old.id, old.grams);
END;
