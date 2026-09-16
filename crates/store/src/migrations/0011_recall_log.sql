-- Recall usage log (M6: threshold tuning with real usage data).
-- One row per recall call: what each section returned, and the RAW
-- top scores BEFORE the relevance filters — the pair answers "are the
-- thresholds dropping near-misses" offline.

CREATE TABLE recall_log (
    id                  INTEGER PRIMARY KEY,
    ts                  TEXT NOT NULL,
    query               TEXT NOT NULL,          -- capped at 200 chars by the writer
    strategy            TEXT NOT NULL,          -- aggressive|conservative
    top_n               INTEGER NOT NULL,
    memories            INTEGER NOT NULL,       -- returned stubs/hits per section
    knowledge           INTEGER NOT NULL,
    wiki                INTEGER NOT NULL,
    entities            INTEGER NOT NULL,
    top_memory_score    REAL,                   -- raw best semantic score (pre-filter)
    top_knowledge_score REAL                    -- raw best hybrid score (pre-filter)
);
