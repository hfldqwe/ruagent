-- 0027: entity aliases get their own FTS index (ruagent-close-the-gaps t25).
--
-- WHY AN INDEX AT ALL. Measured on the ingested corpus copy (t18): of 257
-- distinct full-width-parenthetical names, 116 are an `entities.name` and 141
-- live ONLY in `entity_aliases`. The strict search pass quotes the query into
-- FTS5 phrases over `entities_fts(name, summary)`, so for the 141 it answers
-- either `match="candidate"` (103: the strict pass is empty, the loose pass
-- widens to a list the reader cannot verify) or a false-positive `match="exact"`
-- from an unrelated entity's summary (38); for 32 of the 103 the alias-owning
-- entity is not even IN the candidate list. The term is not lost -- it is in
-- `entity_aliases` -- it was simply unsearchable.
--
-- WHY ITS OWN INDEX, AND NOT A COLUMN ON `entities_fts`. `entities_fts` is
-- `fts5(name, summary, content='entities', content_rowid='id')`: an EXTERNAL
-- CONTENT index, one row per entity, and FTS5 resolves an external content
-- table's columns BY NAME. A third column would need a matching column on
-- `entities`, so the alias text would have to be duplicated into the entity row
-- -- which is exactly the flattening this task must not do -- and
-- `INSERT INTO entities_fts(entities_fts) VALUES('rebuild')` would no longer be
-- able to reproduce the index from its content table. Aliases therefore get an
-- index keyed by the ALIAS ROW id: `entity_aliases` stays the single source of
-- truth for which names became one entity, an alias hit is distinguishable from
-- a name hit (the join through `entity_aliases` is what says so), and
-- `entities_fts` keeps its one-row-per-entity invariant untouched.
--
-- WHAT IT BUYS OVER AN EXACT `norm_alias` PROBE, measured. A probe on
-- `norm_alias` (UNIQUE, so one row) answers a query that IS the whole alias; the
-- index additionally answers a query that is a PART of it:
--
--   query set                            before  exact probe  +this index
--   base term of the 141 alias-only names    55        57          138  (of 140)
--   base term of every alias                 2761      3819         3927  (of 3929)
--   the alias text itself                    2727      3929         3929  (of 3929)
--
-- (base term = the text before the first `（`; counted on the t18 corpus copy,
-- live_db entity_ids. The "before" column is the strict phrase pass over
-- `entities_fts` as it stands at schema 26.)
--
-- ADDITIVE, AND IDEMPOTENT. Three `IF NOT EXISTS` objects plus one `rebuild` of
-- an index DERIVED from `entity_aliases`: no column is added to any table, no
-- existing row is rewritten, and no alias text is copied anywhere. `rebuild`
-- discards the index content and re-reads the alias table, so it is safe to run
-- again (which is what the t98 re-application path would do if the ledger row
-- for 0027 were lost while its objects remained). A database already at
-- version 27 never reaches this file: `apply` skips versions <= the recorded
-- one. The whole migration is unnecessary for a database whose `entity_aliases`
-- is empty -- the index is simply empty with it.

CREATE VIRTUAL TABLE IF NOT EXISTS entity_aliases_fts USING fts5(
    alias,
    content='entity_aliases',
    content_rowid='id',
    tokenize='unicode61'
);

-- Keep entity_aliases_fts in sync with the alias table (same shape as 0005's
-- entities_ai/au/ad, plus a WHEN filter the entities trigger does not have: a
-- merge rewrites `entity_id` on every absorbed alias row and leaves the TEXT
-- alone -- `UPDATE entity_aliases SET entity_id = ? WHERE entity_id = ?` in
-- graph's `merge_entities_in` -- so an unfiltered trigger would re-index
-- unchanged text once per absorbed row).
CREATE TRIGGER IF NOT EXISTS entity_aliases_ai AFTER INSERT ON entity_aliases BEGIN
    INSERT INTO entity_aliases_fts(rowid, alias) VALUES (new.id, new.alias);
END;
CREATE TRIGGER IF NOT EXISTS entity_aliases_ad AFTER DELETE ON entity_aliases BEGIN
    INSERT INTO entity_aliases_fts(entity_aliases_fts, rowid, alias) VALUES ('delete', old.id, old.alias);
END;
CREATE TRIGGER IF NOT EXISTS entity_aliases_au AFTER UPDATE ON entity_aliases
WHEN old.alias IS NOT new.alias BEGIN
    INSERT INTO entity_aliases_fts(entity_aliases_fts, rowid, alias) VALUES ('delete', old.id, old.alias);
    INSERT INTO entity_aliases_fts(rowid, alias) VALUES (new.id, new.alias);
END;

-- An existing database already holds alias rows: build the index from them.
INSERT INTO entity_aliases_fts(entity_aliases_fts) VALUES('rebuild');
