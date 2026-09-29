-- 0021: graph evidence (R-C / gen2-graph-spec E3, E4, E7(shared), E8, E10 + DEP-3;
--       t6 = I-SCHEMA).
--
-- The graph's problem is not missing algorithms: 67 edges exist, 20/63 entities
-- are orphans, and the multi-hop capability is present but has no production
-- call site. What the schema was missing is PROVENANCE and RECALL POSITIONS:
--   * 0 of 67 edges carry a source_episode (against 162/163 memories);
--   * |valid_at - created_at| <= 1s on 66/67 edges, so the event-time axis is
--     the write clock and there is no column saying where valid_at came from;
--   * multi-hop retrieval has NO column in recall_log to record its result, so
--     "did the graph leg ever fire" is unanswerable from the log.
--
-- Same NULL rule as 0020: a column added to a table that already has rows is
-- NULLABLE WITH NO DEFAULT, because `DEFAULT x` backfills every historical row
-- with an assertion the migration cannot support.
--   * gen2-graph-spec:357 asks for
--     `event_time_source TEXT NOT NULL DEFAULT 'recorded'`. Measured: 66/67
--     edges do have valid_at == created_at, so 'recorded' would be CORRECT for
--     them and WRONG for the 1 outlier -- a column whose single value is right
--     for 98.5% of rows and silently wrong for the rest is worse than a NULL
--     that says "the extraction path that wrote this edge did not record where
--     valid_at came from". The graph spec's own TemporalStatus keeps a
--     RecordedAtOnly state, and a reader can still DERIVE it by measuring
--     |valid_at - created_at| -- the column records what the writer KNEW, which
--     is the thing measurement cannot reconstruct.
--   * Same reasoning for `distill_log.status` (0020) and `entities.embedder`.

-- ---------------------------------------------------------------------------
-- entity_edges: where the timestamps came from, and a stable fact identity.
-- ---------------------------------------------------------------------------

-- 'extracted' = the extraction returned an event time.
-- 'recorded'  = valid_at was stamped with the write clock (no event time).
-- 'backfilled'= valid_at was derived during a migration.
-- NULL        = pre-0021 row: the writer did not record it.
ALTER TABLE entity_edges ADD COLUMN event_time_source TEXT;
-- A stable identity for the fact (normalised text hash) so the same fact
-- re-extracted in a later session can be recognised instead of duplicated.
ALTER TABLE entity_edges ADD COLUMN fact_hash TEXT;

-- ---------------------------------------------------------------------------
-- Alias resolution (E4).
-- ---------------------------------------------------------------------------

-- New tables: no history, so the NOT NULLs here are honest.
CREATE TABLE IF NOT EXISTS entity_aliases (
    id         INTEGER PRIMARY KEY,
    entity_id  INTEGER NOT NULL REFERENCES entities(id),
    alias      TEXT NOT NULL,
    norm_alias TEXT NOT NULL UNIQUE,
    source     TEXT NOT NULL,   -- where the alias came from (extraction | human | ...)
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_entity_aliases_entity ON entity_aliases(entity_id);

-- The undecided queue: a pair the mechanical judge would not merge, kept so a
-- human/LLM pass can decide later. Keeping the pair is what makes "we did not
-- merge these two" a recorded decision instead of a silent one.
CREATE TABLE IF NOT EXISTS resolution_pending (
    entity_a   INTEGER NOT NULL REFERENCES entities(id),
    entity_b   INTEGER NOT NULL REFERENCES entities(id),
    reason     TEXT,
    created_at TEXT NOT NULL,
    PRIMARY KEY (entity_a, entity_b)
);

-- ---------------------------------------------------------------------------
-- Community summaries (E8). Built lazily; a community with no summary row yet
-- is not an error state.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS communities (
    id         INTEGER PRIMARY KEY,
    level      INTEGER NOT NULL,
    parent_id  INTEGER REFERENCES communities(id),
    summary    TEXT,
    built_at   TEXT
);
CREATE INDEX IF NOT EXISTS idx_communities_level ON communities(level);

CREATE TABLE IF NOT EXISTS community_entities (
    community_id INTEGER NOT NULL REFERENCES communities(id),
    entity_id    INTEGER NOT NULL REFERENCES entities(id),
    weight       REAL NOT NULL DEFAULT 1.0,
    PRIMARY KEY (community_id, entity_id)
);
-- The direction retrieval needs: given a seed entity, its communities.
CREATE INDEX IF NOT EXISTS idx_community_entities_entity ON community_entities(entity_id);

-- ---------------------------------------------------------------------------
-- entities: the vector seed leg (E10, P2).
-- ---------------------------------------------------------------------------
-- `embedder` is the model identity and must be written WITH the vector: 0021's
-- sibling, knowledge_meta, exists for the same reason ("never mix embeddings
-- across models"). NULL = not embedded.
--
-- COST NOTE for I-C (registered, not a defect of this migration):
-- `entities_au` (0005_graph.sql:43) is `AFTER UPDATE ON entities` with NO
-- column filter, so writing `embedding` re-writes the entities_fts row
-- (delete + insert) for every embedded entity. That trigger is in a frozen
-- historical migration and cannot be narrowed here.
ALTER TABLE entities ADD COLUMN embedding BLOB;
ALTER TABLE entities ADD COLUMN embedder  TEXT;

-- ---------------------------------------------------------------------------
-- recall_log: a position for the graph legs (DEP-3).
-- ---------------------------------------------------------------------------
-- A13 could only read the entity leg out of the `entities` column; multi-hop
-- retrieval had NO place to record its result, so "the graph leg returned
-- nothing" and "the graph leg was never attempted" were the same reading.
-- NULL on every historical row = not attempted (pre-0021).
ALTER TABLE recall_log ADD COLUMN graph_entities INTEGER;
ALTER TABLE recall_log ADD COLUMN graph_paths    INTEGER;

-- ---------------------------------------------------------------------------
-- NO SCHEMA CHANGE: asks whose answer is "already available".
-- ---------------------------------------------------------------------------

-- 1. E5's optional `entity_sources(entity_id, episode_id)` is NOT created.
--    It is marked 可选, no consumer names it, and the target it would serve
--    ("every edge carries a source") is about EDGES, which already have
--    `source_episode` (0005_graph.sql:34). An unread table is not schema, it is
--    a claim that something is covered. If I-C lands a real consumer, the table
--    is one additive migration away.
-- 2. E2's "可选索引 entity_edges(src, invalid_at)": `idx_edges_src
--    ON entity_edges(src, relation, invalid_at)` already exists
--    (0005_graph.sql:36) and `idx_edges_dst` covers the other direction. A
--    second index on (src, invalid_at) would be a prefix of the existing one.
