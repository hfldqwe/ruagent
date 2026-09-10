-- Agent registry with stable ids: config files define agents by name,
-- the daemon upserts them here so RunParams.agent survives restarts.

CREATE TABLE agents (
    name    TEXT PRIMARY KEY,
    id      TEXT NOT NULL UNIQUE,
    harness TEXT NOT NULL,
    card    TEXT NOT NULL           -- JSON-encoded AgentCard
);
