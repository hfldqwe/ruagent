---
name: ruagent-operator
description: Operate the ruagent platform — search the user's long-term memory, ingest knowledge, and read platform tasks through MCP tools. Use before asking the user for facts they may have already shared, or when asked to remember something for future sessions.
---

# ruagent operator manual

You are running inside the ruagent platform. It gives you three durable
capabilities that outlive your session — use them.

## Memory (facts about the user, learned across sessions)

- **Before asking the user anything they might have told you before**,
  call `memory_search` with the relevant keywords.
- **When the user asks you to remember something** (or shares a durable
  preference/decision), call `memory_write`:
  - `store`: `profile` for facts about the user themselves; `observation`
    for general observations; `procedure` for how-to knowledge scoped to
    a project (`namespace: project:<name>`) or `global`; `lesson` for
    mistakes-and-fixes worth keeping.
  - `namespace`: `user`, `global`, `project:<name>`, or `agent:<name>`.
  - One concise fact per write. Pass `supersedes` to replace an outdated
    memory instead of duplicating it.

## Knowledge (searchable documents)

- `knowledge_search` for anything the user may have ingested (docs,
  notes, code). Combine with `memory_search`: memory holds *facts*,
  knowledge holds *documents*.
- `knowledge_ingest` when the user wants a document to become
  searchable.

## Tasks (the platform's durable work items)

- `list_tasks` (optionally filter by status) to see the board you and
  the other agents share.

## Rules

1. Never invent memory content: write what was actually said or decided.
2. Prefer `memory_search` over asking the user again.
3. Keep writes atomic — one fact, one write.
4. When unsure whether something belongs in memory, ask the user.
