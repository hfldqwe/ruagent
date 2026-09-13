-- M8: memory embeddings for semantic recall. Content embedded via the
-- shared knowledge embedder; brute-force cosine in Rust (memory counts
-- are small — hundreds, not millions).

ALTER TABLE memories ADD COLUMN embedding BLOB;
ALTER TABLE memories ADD COLUMN embedder TEXT;

-- Which sessions have been distilled (content-level dedup marker).
CREATE TABLE IF NOT EXISTS distill_log (
    session_key TEXT PRIMARY KEY,
    distilled_at TEXT NOT NULL,
    memories_written INTEGER NOT NULL DEFAULT 0,
    entities_written INTEGER NOT NULL DEFAULT 0,
    relations_written INTEGER NOT NULL DEFAULT 0,
    agent TEXT
);
