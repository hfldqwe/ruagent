-- Chat history (design §5.1 "everything is a session"): every chat the
-- daemon ever started, so the panel can list past conversations per
-- agent/runtime and reopen their transcripts. Written by ChatManager on
-- start (row) and first prompt (title); never deleted — closed chats are
-- the point.
CREATE TABLE IF NOT EXISTS chats (
    id TEXT PRIMARY KEY,
    -- The card the user picked (role or runtime name); survives runtime
    -- switches mid-chat.
    agent TEXT NOT NULL,
    -- The [runtime.X] engine this chat runs on right now.
    runtime TEXT,
    model TEXT,
    title TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
