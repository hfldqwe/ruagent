-- 0033: THE CHAT REPORT OUTBOX (P4, docs/plans/2026-11-23-reports-never-lost.md §1).
--
-- WHAT WAS WRONG: a platform event (the sentence the platform sends into the conversation that
-- dispatched work) only existed as a PROMPT. When the chat had no live session — after a daemon
-- restart, or after the user closed/idle-reaped the conversation — `ChatManager::notify` could only
-- log `platform event dropped` and return. The work HAD finished; the report that says so was gone.
-- The user's own words for the requirement are 「如果是最终结果，还是需要汇报的」, so a dropped report
-- is a broken promise, not a lost log line.
--
-- WHAT THIS IS: one row per report the platform owes a conversation. Written BEFORE any attempt to
-- deliver it (store first, deliver second: a failed delivery leaves the row PENDING, and a failed
-- write is the only failure the caller is told about). Delivered on resume, one per resume.
--
-- TWO RULES THIS TABLE ENCODES, both load-bearing:
--
--   * `delivered_at` is the ONLY delivered/not-delivered criterion. `attempts` is DIAGNOSTIC — how
--     many times the sentence was handed to the session, successful attempts included (a DELIVERED
--     row therefore reads `attempts >= 1`; a row that never got through reads NULL + attempts > 0)
--     — and is deliberately NOT a retry budget: the budget is the per-chat cap (see
--     `CHAT_REPORTS_CAP` in `lib.rs`). A predicate like `attempts < 3` would be a second truth
--     about whether a report is still owed.
--   * `text` carries the report body WITHOUT the platform marker (`chat::USER_TEXT_SENTINEL`). The
--     marker is applied at delivery time by the one function that builds the prompt, so what is
--     stored is the READABLE body: a row is "what the platform wants to say", never "a prompt
--     fragment with a daemon-specific sentinel baked in".
--
-- `chat_id` is not a foreign key: `chats` has been rebuilt/migrated repeatedly and a FK here would
-- make a delete order mandatory at every site. The delete path removes these rows EXPLICITLY
-- (`Db::delete_chat`, one closure with the `chats` DELETE) — the same shape as `DELETE /tasks/{id}`
-- removing the task's runs.
CREATE TABLE IF NOT EXISTS chat_reports (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  chat_id      TEXT NOT NULL,              -- which conversation is owed this report
  run_id       TEXT,                       -- which run prompted it (NULL: not every report is a run's)
  text         TEXT NOT NULL,              -- the body, WITHOUT the platform marker
  created_at   TEXT NOT NULL,              -- RFC3339, this repo's format
  delivered_at TEXT,                       -- NULL = still owed. THE ONLY delivered criterion.
  attempts     INTEGER NOT NULL DEFAULT 0  -- failed delivery attempts, for diagnosis only
);

-- The pending read is `WHERE chat_id = ? AND delivered_at IS NULL ORDER BY id`: the resume path,
-- the per-chat count on GET /api/v1/chats, and the /chat/{id}/reports?pending=1 read all take it.
CREATE INDEX IF NOT EXISTS idx_chat_reports_pending ON chat_reports(chat_id, delivered_at);
