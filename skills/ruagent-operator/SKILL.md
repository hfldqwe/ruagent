---
name: ruagent-operator
description: Operate the ruagent platform — search and write the user's long-term memory, run unified recall, search/ingest knowledge, read platform tasks, and inspect or switch the platform's pipelines (capabilities) through MCP tools. Use before asking the user for facts they may have already shared, when asked to remember something for future sessions, when asked what is switched on or what costs model tokens, or when asked to turn a pipeline on or off.
---

# ruagent operator manual

You are running inside the ruagent platform. It gives you durable
capabilities that outlive your session — use them.

## Cost tiers (read this first)

Every tool below is marked `free` or `llm`.

- `free` = zero model tokens: local index reads, config reads, deterministic
  extraction. Calling one costs nothing, so call it instead of guessing.
- `llm` = spends the user's model tokens (an agent turn is run). Never take an
  llm path without saying so and getting agreement.

When a cheap and an expensive path both exist, take the cheap one first.

## Platform capabilities (what is switched on)

- Call `capabilities_list` **before** you rely on recall, distillation or
  knowledge ingestion, and whenever the user asks what is on/off or why
  something did not happen. It reports, per capability: the id, the token tier
  (`free` / `llm`), whether it is enabled now, whether that value came from
  `policy.toml` or from the registry default, its options, and what it gates.
  `free` = zero tokens, deterministic; `llm` = spends model tokens.
  Optional `tier: "free"` or `tier: "llm"` filters the list.
- Change one with `capability_set` (`id`, `enabled`, plus the options that
  capability declares). Enabling an `llm` capability spends the user's tokens:
  say so, then pass `confirm_cost: true` — without it the daemon refuses the
  write. The tool changes exactly one capability and keeps everything else
  `policy.toml` already configures. Its reply is the full post-change state:
  read `conflicts[]`, which names any capability that is now off while its
  legacy switch is still on, and prints the `[capabilities.<id>]` line that
  restores it.
- Distil one session now with `distill_session`: `extractor: "rules"` is
  `free` — zero model tokens, deterministic; `extractor: "acp"` (the default)
  is `llm` — it runs a full extraction-agent turn; `"both"` runs rules then
  acp. Pass `dry_run: true` to see what would be written without writing.
  A session key looks like `ruagent:<hex>`.
- Put knowledge documents into the knowledge graph with
  `knowledge_graph_ingest`: it is `free` (no model is called) but it WRITES
  entities and relations, and the automatic path is **off by default**.
  Run `dry_run: true` first — it prices the work with counts and always works,
  capability on or off. With `dry_run: false` while the capability is off the
  daemon refuses and names the remedy; enable it with
  `capability_set(id: "knowledge_ingest_graph", enabled: true)`.
  The reply's `ledger_hits` is why a re-run writes nothing.

## Memory (facts about the user, learned across sessions)

- **Before asking the user anything they might have told you before**,
  call `memory_search` with the relevant keywords (`free`).
- **When the user asks you to remember something** (or shares a durable
  preference/decision), call `memory_write` (`free`):
  - `store`: `profile` for facts about the user themselves; `observation`
    for general observations; `procedure` for how-to knowledge scoped to
    a project (`namespace: project:<name>`) or `global`; `lesson` for
    mistakes-and-fixes worth keeping.
  - `namespace`: `user`, `global`, `project:<name>`, or `agent:<name>`.
  - One concise fact per write. Pass `supersedes` to replace an outdated
    memory instead of duplicating it.

## Recall (one pass over memory + knowledge + wiki + graph)

- `memory_recall` (`free`) searches all four stores at once. Start here when a
  question may be answered by facts, documents or relations.
- `strategy: "conservative"` returns stubs only (titles, entity names,
  one-line relations) — cheap to scan; then fetch what you actually need with
  `memory_get`, `knowledge_expand` or `graph_entity`. `strategy: "aggressive"`
  (the default) returns full content above the relevance threshold.
  `conservative: true` is the older spelling; do not pass both with
  disagreeing values.
- Recall *legs* (the six retrieval sources) are switched and weighted through
  the capability plane, not through `memory_recall`: the ids are
  `recall_leg_memory_semantic`, `recall_leg_memory_fts`,
  `recall_leg_knowledge_semantic`, `recall_leg_knowledge_fts`,
  `recall_leg_wiki`, `recall_leg_graph`. Use `capabilities_list` to see which
  are off and `capability_set` to change one. The reply's `legs_disabled`
  tells you which legs a call skipped: if it is non-empty, a thin result is
  the configuration, not an empty corpus — say so instead of concluding "no
  information".
- Generated wiki pages are leads, not ground truth: verify against sources.

## Knowledge (searchable documents)

- `knowledge_search` (`free`) for anything the user may have ingested (docs,
  notes, code). Combine with `memory_search`: memory holds *facts*,
  knowledge holds *documents*.
- `knowledge_ingest` (`free`) when the user wants a document to become
  searchable. It ingests chunks only: entities and relations reach the
  knowledge graph through `knowledge_graph_ingest` (or the automatic sweep),
  and **only when the capability `knowledge_ingest_graph` is enabled** —
  it is off by default, so check with `capabilities_list` before promising
  graph results.

## Tasks (the platform's durable work items)

- `list_tasks` (`free`, optionally filter by status) to see the board you and
  the other agents share.

## Rules

1. Never invent memory content: write what was actually said or decided.
2. Prefer `memory_search` over asking the user again.
3. Keep writes atomic — one fact, one write.
4. When unsure whether something belongs in memory, ask the user.
5. Never enable an `llm`-tier capability without the user's explicit
   agreement: it spends their tokens. State the tier and the expected cost
   before you call `capability_set`.
