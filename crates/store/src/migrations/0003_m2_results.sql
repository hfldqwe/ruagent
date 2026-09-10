-- M2: run results (fan-out compare / pipeline handoff unit) and the
-- selected winner of a task's runs.

ALTER TABLE runs ADD COLUMN result TEXT;
ALTER TABLE tasks ADD COLUMN selected_run_id TEXT;
