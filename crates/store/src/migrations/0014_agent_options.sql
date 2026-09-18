-- Advertised session options (model catalog, reasoning efforts, permission
-- modes, …) per runtime, as reported over ACP. The panel pickers read this
-- instead of probe-spawning every runtime on each cold start; entries are
-- refreshed at daemon boot, every few hours, by live chats, and manually.
CREATE TABLE IF NOT EXISTS agent_options (
    runtime TEXT PRIMARY KEY,
    options TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);
