-- 0028: the distillation ledger learns WHAT AN ATTEMPT COST and WHAT IT COULD
-- NOT WRITE (ruagent-issues-45-55 t3/t4/t5; issues #45, #48, #55).
--
-- WHY COLUMNS RATHER THAN A SECOND TABLE. `distill_log` is already ONE ROW PER
-- ATTEMPT (0024) and every new reading is a property OF that attempt: how many
-- times it was retried, how many windows its transcript was cut into, whether
-- that cut lost text, what the model call consumed, and how many extracted
-- relations the graph refused or found already present. A side table would have
-- to key on `distill_log.id` and would turn "read one attempt" into a join --
-- and the point of the ledger is that ONE row answers the question the operator
-- has after a failure.
--
-- ⚠ EVERY COLUMN IS NULLABLE WITH **NO DEFAULT**, and that is the whole reading
-- rule: NULL means NOBODY RECORDED THIS, never 0. `ALTER TABLE` with
-- `DEFAULT 0` would stamp a fabricated zero onto every historical row --
-- "this attempt used no tokens", "no window was lost" -- which is exactly the
-- assertion 0020 refused to make with `NOT NULL DEFAULT 'ok'` and the same
-- mistake 0024 refused to make with `status`. A reader must therefore not write
-- `COALESCE(tokens_used, 0)` when it means "the attempt consumed nothing"; it
-- may do so only when it means "spend of unknown attempts counts as zero", and
-- then it has to say so. Pre-0028 rows are the common case of this: they were
-- written by a daemon that had nowhere to record any of it.
--
-- THE TWO SHAPES OF NULL ARE DISTINGUISHABLE, and that is why no sentinel is
-- used. `truncated IS NULL` = the pre-0028 history, plus any attempt on a path
-- that never measured; `truncated = 0` = a measured, lossless single-window
-- attempt. A `-1` sentinel would have made those two indistinguishable.
--
-- IDEMPOTENT BY THE RUNNER, not by SQL: SQLite has NO `ADD COLUMN IF NOT
-- EXISTS`, so an `ALTER` cannot guard itself. `migrations::apply_script` probes
-- `pragma_table_info(<table>)` for every `ALTER TABLE ... ADD COLUMN` statement
-- and SKIPS the ones whose column is already there, which is what makes a lost
-- `schema_migrations` row for this version harmless. The statements below must
-- therefore stay in the exact shape that guard parses -- `ALTER TABLE <table>
-- ADD COLUMN <column> <type>;`, one statement per line -- or the probe misses
-- them and a re-apply fails with `duplicate column name`.
--
-- RELATION: the guard recognised this shape before this file existed
-- (`the_guard_parses_the_shapes_it_claims_to`, and the ~35 ALTERs in
-- 0003..0023); nothing new is added to the runner by this migration.

-- --------------------------------------------------------------------------
-- The graph write report, per attempt (issue #55, t5).
-- --------------------------------------------------------------------------
-- Relations the extractor produced that the graph already held, keyed on
-- (src, dst, relation, fact_hash). Recorded rather than discarded silently:
-- a high number is how a reader tells "the extractor is re-stating known
-- facts" (normal) from "extraction is not converging" (a prompt problem).
ALTER TABLE distill_log ADD COLUMN relations_duplicate INTEGER;

-- Relations the graph REFUSED to write -- a dangling endpoint, an empty
-- relation name, a fact-body mismatch. Non-zero means content was extracted
-- and then dropped, which before this column was invisible: the attempt's
-- `relations_written` simply counted lower.
ALTER TABLE distill_log ADD COLUMN relations_refused INTEGER;

-- --------------------------------------------------------------------------
-- What the model call consumed (issue #55, t5).
-- --------------------------------------------------------------------------
-- `ContextUsage.used` of the attempt's LAST RunEvent::UsageUpdate, parsed by
-- the same function `runs.rs` uses -- one parse, not two, so the ledger and the
-- run timeline cannot disagree about the same event.
ALTER TABLE distill_log ADD COLUMN tokens_used INTEGER;

-- `ContextUsage.size`: the context window the model was configured with. Kept
-- beside `tokens_used` because the pair is what makes the number interpretable
-- ("91k of 200k" vs "91k of 91k"), and because the ratio is what an operator
-- reads when an attempt starts failing on long sessions.
ALTER TABLE distill_log ADD COLUMN context_size INTEGER;

-- `cost_usd` as reported for that attempt. REAL, and NULL for providers that
-- do not price their usage -- not 0.0, which would be a claim that the attempt
-- was free.
ALTER TABLE distill_log ADD COLUMN cost_usd REAL;

-- --------------------------------------------------------------------------
-- How the attempt got there (issues #45 and #48, t4/t5).
-- --------------------------------------------------------------------------
-- The number of RETRIES this attempt needed (0 = first try succeeded or first
-- try's outcome is final). Bounded backoff at the distillation entry, and the
-- count is the only evidence that the retry path actually ran -- a green
-- outcome with `retries = 3` is a different story from one with `retries = 0`.
-- A row with `retries IS NULL` predates the retry path; do not read it as 0.
ALTER TABLE distill_log ADD COLUMN retries INTEGER;

-- How many windows the transcript was cut into (issue #48). 1 = it fit; a
-- single NULL column cannot say both "one window" and "not measured", which is
-- why this is not folded into `truncated`.
ALTER TABLE distill_log ADD COLUMN windows INTEGER;

-- Whether the byte budget forced a LOSSY cut -- text dropped rather than
-- windowed. 0 = nothing was lost, 1 = something was, NULL = unknown/not
-- measured. `windows` beside it says which case a 1 is: a truncated single
-- window lost the tail, several windows lost nothing.
ALTER TABLE distill_log ADD COLUMN truncated INTEGER;
