-- Selection provenance (design §5.2/§5.4): who picked the winning run of a
-- fan-out — the human ('human') or a judge agent ('agent:<name>'). Existing
-- selections were all made by the human select button.
ALTER TABLE tasks ADD COLUMN selected_by TEXT;
UPDATE tasks SET selected_by = 'human' WHERE selected_run_id IS NOT NULL;
