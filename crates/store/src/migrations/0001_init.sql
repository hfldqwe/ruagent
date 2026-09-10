-- M1 schema: tasks, typed edges, runs. Memory/graph/decision tables arrive
-- with M3; claim/lease columns arrive with the v2 task board.

CREATE TABLE tasks (
    id            TEXT PRIMARY KEY,
    title         TEXT NOT NULL,
    intent        TEXT NOT NULL,
    status        TEXT NOT NULL,
    creator       TEXT NOT NULL,          -- JSON-encoded TaskCreator
    project       TEXT,
    pinned_agent  TEXT,
    created_at    TEXT NOT NULL,          -- RFC 3339
    updated_at    TEXT NOT NULL
);
CREATE INDEX idx_tasks_status ON tasks(status);

CREATE TABLE task_edges (
    from_id  TEXT NOT NULL,
    to_id    TEXT NOT NULL,
    kind     TEXT NOT NULL,              -- depends_on | spawned_by | reviews | fanout_of
    PRIMARY KEY (from_id, to_id, kind)
);
CREATE INDEX idx_edges_to ON task_edges(to_id);

CREATE TABLE runs (
    id              TEXT PRIMARY KEY,
    task_id         TEXT NOT NULL,
    agent           TEXT NOT NULL,
    params          TEXT NOT NULL,        -- JSON-encoded RunParams
    status          TEXT NOT NULL,
    acp_session_id  TEXT,
    workspace       TEXT,
    context_used    INTEGER,
    context_size    INTEGER,
    cost_usd        REAL,
    error           TEXT,
    stop_reason     TEXT,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);
CREATE INDEX idx_runs_task ON runs(task_id);
CREATE INDEX idx_runs_status ON runs(status);
