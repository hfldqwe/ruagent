-- 0023: recall telemetry (R-E / gen2-integration-contract §3.1 K-2; t6 = I-SCHEMA).
--
-- WHY A SECOND recall_log MIGRATION RATHER THAN MORE COLUMNS IN 0019: the ask
-- arrived after 0019 was written and registered (integration contract,
-- 2026-09-27T22:53:52+08:00), and the two asks are different questions.
-- 0019 answers "how good was the score"; this one answers "which leg found the
-- top hit, how many candidates did each leg offer, and what was dropped, by
-- which rule". Keeping them apart keeps each file's provenance readable, and it
-- keeps the additive rule intact: nothing already written is edited.
--
-- The gap it closes, from the contract: `min_score`'s discard is a silent
-- `continue` in api.rs, so "we found nothing" and "we threw everything away"
-- are the same reading; and multi-hop had no position at all. `graph_entities`
-- / `graph_paths` landed in 0021; these six are the rest.
--
-- SAME NULL RULE AS 0019-0022, and it is the whole point here: every column is
-- NULLABLE WITH NO DEFAULT, so the 651 historical rows read as "not measured
-- (pre-0023)". NOTHING may render a NULL as 0 -- the contract states it for the
-- API (`recall/log` "历史行这些列 = null（不是 0，不给历史行背书）"), and a 0 in
-- `rejected_json`'s counters would be exactly the "we dropped nothing" claim
-- that today's silent `continue` makes indistinguishable from a real zero.

-- The scale literal of `top_knowledge_score`, written as a constant ("rrf_rank")
-- by the writer. Redundant with the column's own name on purpose: a reader of a
-- row must not have to know which generation of the writer produced it, and
-- 0019 already added the sibling `top_knowledge_relevance_kind` for the same
-- reason -- a number without its scale is the defect (R-A C6).
ALTER TABLE recall_log ADD COLUMN score_kind       TEXT;
-- The fusion expression, e.g. "rrf(k=60,w_sem=2,w_kw=1)". Read WITH
-- knowledge_leg_window and scoring_version: three rows must agree on all three
-- before their scores may be compared at all.
ALTER TABLE recall_log ADD COLUMN fusion           TEXT;
-- Per-leg evidence for the top-1 FUSED knowledge hit:
--   [{"leg":"semantic","rank":1,"raw_score":0.1244,"kind":"semantic_l2sq"},
--    {"leg":"keyword","rank":3,"raw_score":-4.75,"kind":"bm25"}]
-- A leg that did NOT find that chunk is ABSENT from the array, never present
-- with a 0: 0 is a legal score and would read as a perfect match on a leg that
-- missed entirely.
ALTER TABLE recall_log ADD COLUMN top_legs_json    TEXT;
-- How many candidates each leg contributed and how big the union was:
--   {"semantic":60,"keyword":60,"fused":84}
ALTER TABLE recall_log ADD COLUMN candidates_json  TEXT;
-- Per-section KEPT counts: {"memories":5,"knowledge":5,"wiki":2,"entities":3,"graph_paths":4}
ALTER TABLE recall_log ADD COLUMN selected_json    TEXT;
-- Per-section DROPPED counts, each named by the rule that dropped it:
--   {"memories_by_top_n":2,"knowledge_by_top_n":3,"knowledge_by_min_score":1,
--    "wiki_by_top_n":0,"graph_no_seed":7}
-- `graph_no_seed` is the field that lets "the graph leg had nothing to start
-- from" be told apart from "the graph leg produced no path" (contract §3.1).
ALTER TABLE recall_log ADD COLUMN rejected_json    TEXT;
