-- M9 wiki mode (design docs/plans/2026-09-15-wiki-mode-design.md
-- §6.6 + §13-3): build bookkeeping. The wiki pages themselves are
-- markdown files under knowledge/wiki/ (source of truth, indexed by
-- the scanner); these tables are derived state.

CREATE TABLE wiki_builds (
    id            INTEGER PRIMARY KEY,
    scope         TEXT NOT NULL,          -- "all" | "changed" | comma-separated names
    status        TEXT NOT NULL,          -- planned|running|done|failed
    dry_run       INTEGER NOT NULL DEFAULT 0,
    agent         TEXT NOT NULL,
    pages_planned INTEGER NOT NULL DEFAULT 0,
    pages_written INTEGER NOT NULL DEFAULT 0,
    pages_failed  INTEGER NOT NULL DEFAULT 0,
    plan_json     TEXT,                   -- the planner output (dry-run review + confirm)
    error         TEXT,
    started_at    TEXT NOT NULL,
    finished_at   TEXT
);

CREATE TABLE wiki_build_pages (
    build_id INTEGER NOT NULL REFERENCES wiki_builds(id),
    slug     TEXT NOT NULL,
    action   TEXT NOT NULL,               -- create|update|delete|keep
    status   TEXT NOT NULL,               -- pending|written|failed|skipped
    error    TEXT,
    PRIMARY KEY (build_id, slug)
);

-- §13-3 ruling: the content hash each page had when a build wrote it.
-- A page whose on-disk hash differs was hand-edited and is skipped by
-- later builds unless the plan explicitly says otherwise.
CREATE TABLE wiki_page_hashes (
    slug     TEXT PRIMARY KEY,
    hash     TEXT NOT NULL,
    build_id INTEGER NOT NULL
);
