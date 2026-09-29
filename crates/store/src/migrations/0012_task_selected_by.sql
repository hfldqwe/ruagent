-- Selection provenance (design §5.2/§5.4): who picked the winning run of a
-- fan-out — the human ('human') or a judge agent ('agent:<name>'). Existing
-- selections were all made by the human select button.
ALTER TABLE tasks ADD COLUMN selected_by TEXT;
-- t98: backfill the LEGACY rows only (`selected_by IS NULL`). Without the NULL
-- guard, a re-apply (a lost version row) would rewrite every later provenance --
-- including a judge's `agent:<name>`, which `store::set_selected_run` writes
-- together with `selected_run_id` -- back to 'human'.
UPDATE tasks SET selected_by = 'human' WHERE selected_by IS NULL AND selected_run_id IS NOT NULL;
