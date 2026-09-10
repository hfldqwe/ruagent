# OpenViking — Research Report

**Date:** 2026-09-10
**Researcher:** research-openviking agent
**Verdict:** Found. The project is real, popular, and matches the user's description exactly: a Chinese-origin (ByteDance / Volcano Engine) open-source "context database" that unifies agent memory, knowledge RAG, and skills behind a single filesystem-style interface.

---

## 1. Identification and disambiguation

**Repo:** [github.com/volcengine/OpenViking](https://github.com/volcengine/OpenViking)
**Homepage:** [openviking.ai](https://openviking.ai/) · Docs: [docs.openviking.ai](https://docs.openviking.ai/) · Blog: [blog.openviking.ai](https://blog.openviking.ai/)
**Self-description:** "Self-evolving Context Database for AI Agents. Unify Agent Memory, Knowledge RAG and Skills."

A GitHub search for "openviking" returns 241 repositories, but only one is AI-agent-related: `volcengine/OpenViking`. Disambiguation is trivial:

- It is by far the most-starred match (36,470 stars vs. single digits for the rest, which are mostly Viking-history/DBSCAN-weather projects).
- It is owned by `volcengine` (Volcano Engine, ByteDance's cloud arm) — consistent with the user's profile (Chinese-speaking developer) and with the ecosystem names inside it (VikingDB, Doubao models, Feishu/Lark imports, WeChat/Lark community groups, README in English/中文/日本語).
- The description — "Unify Agent Memory, Knowledge RAG and Skills" — is a word-for-word match for the user's "unified knowledge-base + memory management for AI agents."

Note: GitHub's own WebSearch/WebFetch was blocked from this environment; all data below was gathered via the GitHub REST API and raw.githubusercontent.com with curl, plus direct fetches of docs.openviking.ai and blog.openviking.ai content.

---

## 2. Positioning and philosophy

OpenViking calls itself a **context database** — not a memory plugin, not a vector DB, not a RAG framework. The March 2026 blog post "[The Database Paradigm for Context Engineering](https://blog.openviking.ai/post/openviking-context-database/)" states the thesis:

> "Context engineering = reliable reasoning constraints + complete information organization + effective context recommendation + full-lifecycle memory + traceable self-evolving learning."

The argument: Prompt, RAG, web search, tools/MCP, skills, and memory are all "context primitives," each solving one slice of the same problem. Agents need a database-shaped layer that makes context **ingestible, addressable, indexable, summarizable, scoped, retrievable, and updatable** — with CLI operations agents can learn. Three comparison axes from the blog:

- vs. **vector database**: a vector DB ranks semantic matches; OpenViking exposes a *data interface* (hierarchy, deterministic paths, tiered content, sessions, transactions).
- vs. **file system**: a filesystem provides traversal but no semantic retrieval or lifecycle; OpenViking adds embeddings, tiered summaries, auto-refresh watches, memory extraction.
- vs. **memory frameworks** (mem0/Letta/Zep class): those are memory layers or agent frameworks; OpenViking is a standalone *database service* that any agent plugs into, treating memory as one of three context types.

The project open-sources a subset of the research system described in **"VikingMem: A Memory Base Management System for Stateful LLM-based Applications"** (Fu et al., arXiv:2605.29640, accepted at VLDB 2026). The paper proposes a "Memory Base" paradigm: selective extraction of high-value memories; stateful evolution (progressive summarization, correction, temporal weighting); event-centric extraction with entities dynamically updated by events; topic-wise timeline temporal compression; time-weighted recall. VikingMem reportedly beats baselines by up to 30% on long-term-memory benchmarks.

**Commercial model:** open-source edition under AGPLv3 with an explicit "not crippled" promise (no feature gates, no account required). Paid editions answer "who operates it": Managed SaaS on Volcano Engine (personal free tier / enterprise), and Self-Managed (your VPC, or fully air-gapped for regulated industries). Partner ecosystem: deer-flow (ByteDance), NoKV, loopx, Hermes Agent (NousResearch).

---

## 3. Architecture overview

```
Client (Python SDK / Go SDK / ov CLI / MCP / HTTP)
        │ delegates
Service Layer  (FSService · SearchService · SessionService · ResourceService · PackService · DebugService)
        │
   ┌────┴────────────────┬──────────────────┐
   ▼                    ▼                  ▼
Retrieve            Session              Parse
(intent analysis,   (add/used/commit,    (doc parsing,
 hierarchical        compression,         tree building,
 retrieval, rerank)  memory commit)       L0/L1/L2 gen)
   │                    │
   │             Compressor (schema-driven
   │             memory extraction + LLM dedup)
   └────────────────────┬─────────────────┘
                        ▼
              Storage Layer
   AGFS (content, Rust "RAGFS") + Vector Index (derived)
```

**Design principles (from docs):**

| Principle | Meaning |
|---|---|
| Pure storage layer | Storage only does AGFS ops + basic vector search; rerank lives in retrieval |
| Three-layer information | L0/L1/L2 progressive loading to save tokens |
| Two-stage retrieval | Vector recall + rerank |
| Single data source | All content read from AGFS; the vector index stores only references (URIs, vectors, metadata) and is rebuildable |

**Deployment:** a standalone HTTP server (`openviking-server`, default port 1933), started after an interactive `init` wizard that configures providers (Volcengine, OpenAI, Codex OAuth, Kimi, GLM, or local Ollama — it can auto-install Ollama and pull hardware-appropriate models). Docker/production guides, multi-write storage, snapshots, `ovpack` export/import, at-rest encryption, Prometheus/Grafana observability, native OAuth 2.1.

---

## 4. Deep dive: knowledge base + memory management

This is the part the user cares about, and it is genuinely the best-engineered piece of the project.

### 4.1 The `viking://` URI — everything is a file

All context lives in one virtual filesystem addressed by `viking://{scope}/{path}`:

```
viking://
├── resources/{project}/…          # shared knowledge (account-global, ACL-able)
├── user/{user_id}/
│   ├── memories/…                 # long-term memory (9 built-in types)
│   ├── resources/…                # private resources
│   ├── skills/{name}/SKILL.md     # user skills
│   ├── peers/{peer_id}/…          # per-peer memories & resources
│   └── sessions/{session_id}/…    # session archives
├── agent/                         # account-global capabilities
│   ├── skills/ · endpoints/ · tools/ · payments/   (last three planned)
└── (internal: temp/ queue/ upload/)
```

- `viking://~` is a **home alias** that server-side expands to the authenticated caller's user root — same string, different directory per user.
- Every file gets a deterministic ID: `md5(f"{account_id}:{uri}")`, which is the vector-record primary key — so you can cross-reference index entries without a lookup.
- The agent (or human) operates on it with **`ls`, `tree`, `find`, `grep`, `glob`, `read`, `write`, `edit`, `mv`, `rm`** via CLI, MCP tools, SDK, or WebDAV. This is the core bet: deterministic navigation *plus* semantic search, instead of a black-box vector store. The README: "an agent browses its own context with `ls`, `tree`, and `find` instead of querying a black-box vector store."

### 4.2 Three context types

| Type | What | Who writes | Path |
|---|---|---|---|
| **Resource** | external knowledge: docs, repos, web pages | user (add_resource) | `viking://resources/…` |
| **Memory** | agent's cognition, learned from interactions | agent (auto-extraction) | `viking://~/memories/…` |
| **Skill** | declarable capabilities (AgentDefinedContextType) | user/system | `viking://~/skills/…` or `viking://agent/skills/…` |

Built-in memory types (`openviking/prompts/templates/memory/*.yaml`): `profile`, `preferences` (per-topic files), `entities`, `events`, `identity` (assistant persona), `soul` (assistant principles/continuity), `cases`, `trajectories`, `experiences` (the last three form the "Agent Evolution" pipeline). Custom types are supported.

### 4.3 L0/L1/L2 tiered content — the token-economics engine

Every **directory** (not file) carries two hidden semantic sidecars:

| Layer | File | Default limit | Purpose |
|---|---|---|---|
| **L0 abstract** | `.abstract.md` | 256 chars | one-sentence summary; the unit of vector retrieval and quick relevance checks |
| **L1 overview** | `.overview.md` | 4000 chars | structure + key points + quick navigation; the unit of rerank and "should I load L2?" decisions |
| **L2 detail** | original files | unlimited | full content, loaded only when needed |

- Generation is **bottom-up**: file summaries → leaf L1 → leaf L0 → parents (child L0 bodies aggregate into parent L1), through a `SemanticQueue` with concurrency limits (10 concurrent LLM calls).
- Sidecars use a strict "OKF" format: YAML frontmatter (`directory`, `source`, `generated_by`, `freshness`) + Markdown body. Schema-validated; unknown fields silently dropped; embedding input is the body + whitelisted `directory` field only, so reindexing never changes retrieval input.
- **Freshness metadata**: `total_entries / sampled_entries / unsampled_entries / pending_child_changes` per directory. Above 32 direct children, deterministic *stable sampling* picks the same sample on every refresh of an unchanged tree — avoiding noisy rewrites and diff churn.
- Write protection: public APIs can update sidecar *bodies* but never the protected metadata.

### 4.4 Storage model

Dual-layer, content separated from index:

- **AGFS (content)** — POSIX-style ops, backends: `localfs`, `s3fs` (S3-compatible), `memory`. Rewritten in Rust ("RAGFS") for performance. Optional **multi-write mode** with primary + backup backends (replica/migration/read-acceleration) tracked by internal `.redirect.json` / `.sync_log.json`.
- **Vector index (derived)** — backends: `local`, `http` (remote service), `volcengine` (VikingDB). Schema per record: `id, uri, parent_uri, context_type, is_leaf, vector (dense), sparse_vector, abstract, name, description, created_at, active_count`. Hybrid index (`flat_hybrid`), cosine distance, int8 quantization.
- **VikingFS** is the unified abstraction that keeps both in sync: `rm` deletes index records by URI prefix first, then files; `mv` rewrites `uri`/`parent_uri` in place. The FS is the source of truth; a lost index is rebuildable, lost content is not.
- **Multi-tenancy**: one server, `account` (outer tenant) → `user` (per-account) boundaries, roles ROOT/ADMIN/USER, `api_key` or trusted-gateway (`X-OpenViking-Account/User`) auth. URIs stay clean; storage is silently prefixed `/local/{account_id}/…`. Shared `resources` scope supports per-directory/per-file ACLs.

### 4.5 Ingestion pipeline (knowledge base side)

```
Input → Parser → TreeBuilder → SemanticQueue → EmbeddingQueue → Vector Index
        (no LLM)  (move to AGFS)  (async L0/L1)   (async)
```

- **Parser** (never calls an LLM): Markdown, plain text, PDF, HTML, code repositories (respects `.gitignore`), image/video/audio (VLM-described). Smart splitting: ≤1024 tokens → single file; split by headers; <512-token sections merge; >1024-token sections become subdirectories.
- **TreeBuilder**: 5-phase move from `viking://temp/…` into the target scope, then enqueues semantics.
- **Code intelligence**: code skeletons extracted with maintained tree-sitter `tags.scm` queries, falling back to `tree-sitter-language-pack`, falling back to LLM code-summary.
- **Watch tasks**: resources added from URLs can auto-refresh on an interval (including Feishu/Lark documents, with user-token or app-token auth) — the knowledge base stays current.

### 4.6 Retrieval — hierarchical, observable, two-stage

Two entry points:

- **`find(query)`** — single query, no session, low latency, plain vector search + optional rerank.
- **`search(query, session_id)`** — the full pipeline:
  1. **Intent analysis** (LLM): from session compression summary + last 5 messages + query, emits 0–5 `TypedQuery`s, each typed MEMORY/RESOURCE/SKILL with rewritten query, intent, priority 1–5. Query styles are prescribed: verb-first for skills ("Create RFC document"), noun phrase for resources ("RFC template"), "User's XX" for memory ("User's code style preferences"). 0 queries = chitchat, retrieval skipped.
  2. **Hierarchical retrieval** (HierarchicalRetriever): root directories per context type → global vector search locates up to 10 starting directories → merge + rerank → **recursive descent with a priority queue**: pop a directory, search its children, blend `final_score = α·child_score + (1−α)·parent_score` (α default 1.0), push non-leaf children above threshold, stop after top-k is unchanged for 3 rounds.
  3. **Rerank** (e.g. `doubao-seed-rerank`) at both starting-point evaluation and each recursion level; falls back to vector scores on failure.

Results are `MatchedContext` (uri, type, is_leaf, L0 abstract, score) grouped into memories/resources/skills. Every query **preserves its directory-browsing trajectory** — when a result looks wrong you can see exactly which path produced it. That observability is a headline feature.

### 4.7 Sessions → memory (the memory-management core)

Session lifecycle: **create → interact → commit**.

- Messages support typed parts: `TextPart`, `ImagePart` (VLM-described during extraction), `ContextPart` (URI + abstract — records *what memory was used*), `ToolPart` (tool calls; outputs >20k chars are externalized to a tool-result store with a synopsis stub + reference).
- **`commit()` is two-phase:**
  - **Phase 1 (synchronous, returns a `task_id` immediately):** increment compression index, write `messages.jsonl` to `history/archive_NNN/`, clear active messages.
  - **Phase 2 (async background, resumable after crash via a persistent `session_commit` queue):** generate a structured summary (one-line overview, analysis, primary intent, key concepts, pending tasks) → write archive `.abstract.md`/`.overview.md` → **extract long-term memories** → write `memory_diff.json` audit log → update `active_count` → `.done` marker.
- **Memory extraction flow:**
  ```
  Messages → LLM extract (schema-driven) → candidate memories
      → vector pre-filter for similar existing memories
      → LLM dedup decision:
            candidate-level: skip | create | none
            per-existing-item: merge | delete
      → write to AGFS → vectorize
  ```
- **Schema-driven memory types** are the standout mechanism. Each type is a YAML template (e.g. `preferences.yaml`) declaring: `directory`, `filename_template` (`{{ user }}/{{ topic }}.md`), `embedding_template`, `overview_template`, and typed `fields` each with a **`merge_op`** (`immutable` identity fields like `user`/`topic`; `patch` for content). The schema encodes what "duplicate" and "merge" mean per type, so LLM dedup decisions are constrained and predictable. Example guidance embedded in the schema: "Merge only true aliases for the same owner's same behavioral choice dimension; preferred movie genres and preferred games are not one preference merely because both are entertainment."
- **`memory_diff.json`** — a per-commit audit log of `adds` (uri, type, after), `updates` (before/after), `deletes` (deleted content), plus `skipped_operations` with stable reason codes. Enables audit and rollback of memory changes. This level of memory-change accounting is rare.
- **Peer memory**: when a conversation involves a stable peer, memories can be written to `viking://user/{uid}/peers/{peer_id}/memories/`. In the Claude Code plugin the peer is derived from the git `origin` URL (e.g. `github.com-volcengine-openviking`), so one project = one memory space across clones/worktrees; configurable via `peer.source` (git | cwd | none | template) or a committable `.openviking/config.json`.

### 4.8 Consistency model

"Better to miss a search result than to return a bad one." Core writes (`rm`, `mv`, `add_resource`, `session.commit`) are protected by **file-based path locks** (EXACT and TREE modes, fencing tokens against TOCTOU, stale-lock cleanup), and only `session_commit` needs crash recovery — a persistent queue resumes Phase 2 after restart. Memory extraction is idempotent. Deletes go index-first-then-file so a failed delete never leaves the index pointing at dead content.

### 4.9 How it hooks into agents

- **MCP (universal):** built-in `/mcp` endpoint, 15 server-defined tools — `find`, `search` (with `mode="context"` that assembles injection-ready, budgeted, deduplicated context — the successor of a former `recall` tool), `read` (returns native MCP image/audio blocks for media), `list`, `tree`, `remember` (store messages → triggers extraction), `write`, `edit`, `add_resource` (with a progressive one-shot-token upload flow for sandboxed clients like Claude web/Manus), `list_watches`, `cancel_watch`, `grep`, `glob`, `forget`, `health`. Native OAuth 2.1 for Claude Desktop/claude.ai.
- **Claude Code (deepest integration):** a plugin (`openviking-memory@openviking` via their marketplace, or a one-line installer with a China-friendly TOS mirror) shipping **9 lifecycle hooks + a stdio MCP proxy + slash command + statusline + a skill**. Hooks: before every user prompt → auto-recall and inject; after each response → capture the turn; session start → profile/memory-index injection; PreCompact/session end → commit pending messages; each subagent → an isolated memory session. All writes async, never blocking. Knobs: `OPENVIKING_AUTO_RECALL`, `RECALL_TOKEN_BUDGET` (2000), `AUTO_CAPTURE`, `BYPASS_SESSION(_PATTERNS)`, sed-style recall/capture filters, and latency fuses — query expansion (5s) and digest compression (30s) can each be disabled independently; compression prefers local `claude -p` and falls back to a server digest. Claude Code <2.0 gets a legacy `claude mcp add` + hooks-merge fallback.
- **Other first-class integrations:** Codex / TraeCode CLI 2.0 (shared installer), OpenClaw (context-engine plugin that *takes over compaction*), Cursor, TRAE/TRAE CN, OpenCode, pi (native extension, compaction takeover), Hermes Agent (built-in provider), DeepSeek Harness `dsh`, Agent Plugins 1.0 portable package, LangChain/LangGraph (retriever/tools/store/middleware), community plugins (ZCode, AstrBot). A **Capability Reference** doc cross-compares all of them (tool surface, recall semantics, commit behavior, degradation) — unusual engineering-documentation discipline.
- **Higher layers on the same substrate:**
  - **`ov compile` (context compilation):** `--from` sources + `--to` target + `--skill` (output-shape spec) + optional `--instruction`. A VikingBot agent loop autonomously reads/distills/organizes/writes pages. Shipped skills: **LLM Wiki** (interlinked Markdown + index), **Knowledge Graph** (`entities/*.md` + `relations.jsonl`), **Daily Report** (per-date reconstruction from sessions), **Knowledge Distillation** (cross-source topic conclusions).
  - **Agent Evolution:** `experiences` memory activates the cases/trajectories pipeline; an API reports which trajectories consumed a given experience and their outcome distribution (success/failure/partial/unknown/unfinished) — a closed feedback loop on whether a distilled experience actually helps.
  - **OpenViking Helper** (beta desktop app): visual agent setup, session-trace inspection (recall/injection/MCP/capture/commit events), local memory/skill management.
  - **VikingBot**: a full agent framework on top (`ov chat`), bundled in the official Docker image.

### 4.10 Benchmark evidence (self-reported, scripts shipped in `./benchmark`)

LoCoMo (long-conversation user memory), with Doubao 2.0 Pro as VLM and Doubao-embedding-vision as embedder:

| Agent | Native memory | + OpenViking |
|---|---|---|
| OpenClaw | 24.20% | **82.08%** |
| Hermes | 33.38% | **82.86%** |
| Claude Code | 57.21% | **80.32%** |

…while input tokens drop 34.3–91.0% and query latency drops 58.45–66.10%. tau2-bench (agent experience memory): +6.87pp (retail) and +11.87pp (airline) task success over the same LLM without memory.

---

## 5. Tech stack, activity, license, maturity

| Dimension | Facts (as of 2026-09-10) |
|---|---|
| **Stars / forks / watchers** | 36,470 / 2,793 / 106 |
| **Created** | 2026-01-05 (8 months old) |
| **Activity** | Extremely high: 300+ commits since 2026-08-27 (2 weeks), last push today; Trendshift badge; the blog noted 4k stars shortly after release |
| **Contributors** | 257 (non-anonymous) — a real team, not a one-person project |
| **Releases** | v0.3.3 → v0.4.19 (main), plus `sdk/go/v0.0.2` today; ~40 tags in 8 months — weekly-plus cadence |
| **Languages** | Python 19.2MB (core), Rust 3.4MB (RAGFS storage + `ov` CLI in `crates/`), TypeScript 2.0MB (plugins/studio), C++ 448KB, Go 160KB, tree-sitter queries |
| **License** | **AGPL-3.0** for the main project; Apache-2.0 for `crates/ov_cli` and `examples/` |
| **Maturity** | v0.4.x, pre-1.0. Polished docs (EN/CN/JA), formal docs for transactions/encryption/ACL/multi-tenant/privacy — but docs also carry honest TODOs (parent-refresh write amplification) and recent breaking changes (URI spellings, tool renames), so expect churn |
| **Ecosystem** | Volcengine SaaS + VikingDB + Doubao models are the home turf, but providers include OpenAI, Kimi, GLM, Codex OAuth, and local Ollama; storage works on localfs/S3 |

---

## 6. What is well designed — worth borrowing for our platform

1. **The `viking://` URI + filesystem metaphor is the single best idea.** Unifying memory, knowledge, and skills behind one addressable namespace with `ls/tree/find/grep/read/write/edit` gives agents *deterministic* context navigation layered *under* semantic search. It kills the "black-box vector store" debugging problem and makes memory inspectable by both agents and humans. If we borrow one thing, borrow this.
2. **L0/L1/L2 tiered directory sidecars.** Per-directory abstract (~256 chars, embedded) / overview (~4k chars, reranked) / full content (on demand) is a clean, general answer to token economics in RAG and memory recall. The details are also well-considered: OKF frontmatter with schema validation, embedding-input whitelisting so reindex is retrieval-stable, deterministic stable sampling over 32+ children, freshness counters.
3. **Schema-driven memory types with field-level merge ops.** Memory extraction is not one magic prompt; each memory type is a YAML schema (fields, `merge_op: immutable|patch`, filename/embedding/overview templates) that constrains the LLM's skip/create/merge/delete dedup decisions. Extensible to custom types. This is the most principled memory-write policy I've seen in this category.
4. **`memory_diff.json` audit per commit.** Every memory add/update/delete recorded with before/after content and reason-coded skips → auditable, rollback-able memory. Great for trust and debugging; trivially cheap to replicate.
5. **Two-phase session commit.** Sync archive + async extraction (summary → memory extraction → vectorize) with a persistent resumable queue and idempotent extraction — the right shape for never-blocking the agent while surviving crashes.
6. **Hierarchical recursive retrieval with score propagation and observable trajectories.** Vector search finds the best *directory*, then drills down; results come with surrounding context, and every query keeps its browsing trace for debugging. Much better than flat chunk retrieval for tree-structured knowledge.
7. **FS as source of truth, index as derived data.** Path locks (EXACT/TREE with fencing tokens), index-first deletes, rebuildable index — "better to miss a search result than return a bad one" is the right consistency posture for this class of system.
8. **Scoping model: account / user / peer / session.** Especially peer-scoped memory keyed by git origin — "one project, one memory across clones" — maps perfectly onto coding agents. ACLs on the shared resource scope; clean multi-tenancy without URI pollution (storage-prefix based).
9. **Integration architecture discipline.** Auto-recall/auto-capture via hooks (no model tool-calls needed), MCP as universal fallback with 15 well-chosen tools (including `search mode="context"` that returns budgeted, deduped, injection-ready context), independent latency fuses per optional stage, local-vs-server compression fallbacks, and a published cross-harness capability matrix. The graceful-degradation engineering is worth copying wholesale.
10. **Context compilation (`ov compile`).** Declarative "from → to → skill" knowledge compilation where an agent loop turns scattered material into wikis/knowledge graphs/daily reports. A compelling product-level abstraction on top of the storage substrate.

## 7. Weaknesses and risks

1. **AGPL-3.0.** For our platform, we can borrow *design*, not code — the copyleft makes direct code reuse viable only if our platform is also AGPL/open-sourced. (The `ov` CLI and examples are Apache-2.0, but the core is not.) This is a deliberate moat for their commercial editions.
2. **LLM-heavy write path.** Every ingested directory costs VLM calls for file summaries + L0/L1; every session commit costs summary + extraction + dedup calls. Cost and latency scale with ingest volume; mitigations exist (`vectors_only` mode, timeout fuses, concurrency limits) but the default path is expensive.
3. **Documented write amplification.** Their own docs flag that parent-summary "bubbling" runs after *every* successful semantic task up to the namespace root, causing repeated refreshes in hot, deeply nested directories — acknowledged as an unsolved TODO.
4. **Pre-1.0 churn.** 8 months old, ~weekly releases, recent breaking changes (removed URI spellings, `recall` → `search` mode="context", legacy aliases). Building directly on the API means tracking a fast-moving target; 683 open issues.
5. **Volcengine gravity.** Benchmarks are self-reported using Doubao models; docs and mirrors assume China-friendly deployment (TOS mirrors, Lark/WeChat-first community). Works fine with OpenAI/Ollama/S3, but the happy path is ByteDance's stack, and the enterprise features funnel to their SaaS.
6. **Operational weight.** Server + CLI + plugins + bot + studio + desktop helper, Python core with Rust storage — a lot of moving parts compared to embedding a library like mem0. It is infrastructure, not a dependency.
7. **Multi-worker gaps.** E.g. the one-shot upload token is held in-process, so `add_resource` + upload must hit the same worker in multi-worker deployments (documented limitation).
8. **Memory quality depends on the extraction model.** The 80%+ LoCoMo numbers come from a strong VLM; with weaker local models the extraction/dedup quality ceiling will be lower.

---

## 8. Sources

- Repo: https://github.com/volcengine/OpenViking (README, docs/en/**, GitHub REST API for stats/tags/contributors)
- Concepts: architecture, context-types, context-layers, viking-uri, storage, extraction, retrieval, session, transaction, multi-tenant — docs/en/concepts/ (served at https://docs.openviking.ai/)
- Integrations: docs/en/agent-integrations/01-overview, 02-claude-code, 06-mcp-clients, 16-capability-reference
- Context compilation: docs/en/context-compilation/01-overview
- Memory schemas: openviking/prompts/templates/memory/ (e.g. preferences.yaml)
- Blog: https://blog.openviking.ai/post/openviking-context-database/ ("The Database Paradigm for Context Engineering", Mar 2026)
- Paper: VikingMem, arXiv:2605.29640 (VLDB 2026)
- Benchmarks: README benchmark section + https://blog.openviking.ai/post/openviking-benchmark-results/
