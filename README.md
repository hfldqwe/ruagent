# ruagent

**English** · [中文](README.zh-CN.md)

[![CI](https://github.com/hfldqwe/ruagent/actions/workflows/ci.yml/badge.svg)](https://github.com/hfldqwe/ruagent/actions/workflows/ci.yml) [![E2E](https://github.com/hfldqwe/ruagent/actions/workflows/e2e.yml/badge.svg)](https://github.com/hfldqwe/ruagent/actions/workflows/e2e.yml) [![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

Local-first, single-user **agent engineering platform**: orchestrate multiple coding-agent CLIs (Claude Code, OpenCode, DeepSeek Harness, Codex, …) over the [Agent Client Protocol](https://agentclientprotocol.com), with a unified memory layer, knowledge base + wiki compilation, session distillation, and full observability.

```
┌───────────────────────────── ruagent ─────────────────────────────┐
│  roles (portable prompts)      architect · plugin-dev · …        │
│    └ run on any runtime        dsh · claude-code · opencode      │
│  unified memory               L0 transcripts → L3 injected digest│
│    └ distills automatically   sessions → memories + graph        │
│  unified knowledge            vector + FTS hybrid + graph-guided │
│    └ compiles into wiki       sources → cross-linked topic pages │
│  every agent gets             memory_recall · knowledge_search … │
│    via one platform MCP       (no per-harness glue)              │
└───────────────────────────────────────────────────────────────────┘
```

One resident daemon (Rust) serves both the CLI and the web panel — a
thin local API client. Everything runs on your machine; nothing leaves
it.

Agent processes die with the daemon. On Windows the daemon joins a
kill-on-close job object, so the OS terminates every agent, wrapper
launcher and MCP server it spawned the moment the daemon process dies —
crash, kill or clean exit. On every platform the next boot sweeps the
surviving children of a previously recorded daemon that is verifiably
gone.

## Quick start

```bash
# Prerequisites: protoc on PATH (lancedb needs it at build time)
#   Windows: choco install protobuf   (or extract under %USERPROFILE%/.protoc)
#   Linux:   sudo apt-get install protobuf-compiler
cargo build --release

# Web panel (optional but recommended)
cd panel && npm ci && npm run build && cd ..

cargo run --release -p ruagent -- serve     # daemon + panel on :8787
cargo run --release -p ruagent -- doctor    # 7-check end-to-end self-test
```

Open **http://127.0.0.1:8787** — chat with any agent, browse one unified
session history, recall across memories / knowledge / wiki / graph, and
watch runs live with per-event replay.

## The two-layer agent model

Agents are **roles**, not engines. `~/.ruagent/config/agents.toml`
separates *what an agent is* from *which runtime runs it*:

```toml
[runtime.dsh]                    # runtimes: execution backends
command = "dsh --profile acp"
mcp_profile = "default"

[agent.architect]                # roles: portable prompts
prompt = "You are the architecture reviewer. …"
runtimes = ["dsh", "claude-code", "opencode"]
runtime = "dsh"                  # default runtime
```

The role prompt travels across runtimes: chat with the architect on dsh,
switch the runtime to claude-code mid-conversation — same role, same
memory. It rides runs too: the role block leads every run's injected
context, so a delegated run carries the specialist's identity, not just
the runtime behind it. The panel has separate pages for the two layers:
**Agents** (#agents, roles with live stats) and **Runtimes** (#runtimes,
execution backends). Legacy single-layer configs keep working.

## Orchestration

- **Direct** — one run on one agent.
- **Fan-out & compare** — same prompt to N agents in parallel (each in
  its own git worktree), results side by side; pick a winner by hand or
  let an **AI judge run** decide — the judge is itself a normal run
  whose reply is the verdict, recorded with provenance (`agent:<name>`
  vs `human`, human always overrides).
- **Pipeline** — sequential sub-tasks with bounded handoff context.
- **Routing cascade** — explicit pin > rules > default, every decision
  recorded with its source.
- **Permissions** — deterministic rules > approver agent > human inbox,
  fail-closed. High-risk operations always reach a human.
- **Concurrency gates** — a fan-out past a harness's cap queues instead
  of spawning N children at once: the run row lands as `queued` and
  consumes no workspace or child process until a slot frees. Past the
  queue cap the launch is rejected with a saturation error, recorded on
  the run row.
- **Retry a dead run** — a failed, interrupted or cancelled run gets a
  **Retry** button (`POST /api/v1/runs/<id>/retry`): a new run on the
  same task, agent and options, reusing the dead attempt's workspace so
  half-written work survives, with the crash snapshot (its last output,
  last tool call and why it died) prepended as injected context.

Tuning lives in `policy.toml`:

```toml
[concurrency]
per_harness = 2        # simultaneous runs per harness kind
queue_per_harness = 8  # waiting runs before rejection
```

## The memory layer

| Layer | What | Token cost |
|---|---|---|
| **L0 episodic** | verbatim session transcripts — user + agent, zero judgment, never destroyed | disk only |
| **L1 semantic** | distilled memories with quality signals (user corrections extract the *corrected* fact; confirmations raise confidence) | on recall |
| **L2 structural** | knowledge graph — entities, relations, bi-temporal facts, deterministic supersession | on recall |
| **L3 digest** | injected at a chat's first prompt: profile + semantic recall on the user's opening message | ~1k, once |

Sessions from **claude-code, dsh, opencode and ruagent** auto-sync into
one history (60s incremental scan). With `[distill] auto = true` in
`policy.toml`, closing a session distills it into memories + graph in
the background. `[distill] language = "简体中文"` sets the output
language of distilled memories/entities; `[distill] prompt` replaces the
built-in extraction prompt entirely.

**Recall, two strategies** (`GET /api/v1/recall` / the `memory_recall`
MCP tool), each returning four separately-budgeted sections — memories,
knowledge, wiki, entities:
- **aggressive** — full content above a relevance threshold, RRF-fused
  (semantic + keyword), ready to use
- **conservative** — stubs only: titles + live relations + chunk ids;
  the agent pulls details via `memory_get` / `graph_entity` on demand

## Knowledge base + wiki

- **Hybrid retrieval, zero LLM at query time**: LanceDB vectors +
  SQLite FTS5, RRF-fused. Multilingual out of the box
  (`multilingual-e5-small`; an offline hash embedder is the fallback),
  with automatic re-embedding on model switches.
- **Wiki mode**: compile source documents into cross-linked overview
  pages. A three-stage pipeline (plan → write per page → validate/land)
  behind a dry-run human gate; hand-edited pages are never overwritten
  (content-hash tracked), stale pages flagged, broken `[[links]]` and
  wanted pages visible in the panel. Wiki hits in recall are always
  conservative stubs, labeled as generated.

## Agents get platform memory for free

Every spawned agent receives one MCP server with
`memory_search` · `memory_recall` · `memory_write` · `memory_get` ·
`knowledge_search` · `knowledge_ingest` · `list_tasks` — no
per-harness glue. Skills (SKILL.md library) sync into every harness.

## Observability

Replayable JSONL transcripts (SSE live streams), per-agent cost stats,
context-injection rendered on every timeline, and a recall usage log
(every call's section counts + raw top scores) that feeds tuning. The
run timeline shows the original instruction as its own entry, separate
from the injected context.

## CLI

```
ruagent serve · doctor · agents · status · run · skills · skills-sync ·
         mcp-serve · wiki build|list|links|show
```

## Documentation

- Design: [`docs/plans/2026-09-11-ruagent-design.md`](docs/plans/2026-09-11-ruagent-design.md)
- Memory layer: [`docs/plans/2026-09-14-unified-memory.md`](docs/plans/2026-09-14-unified-memory.md)
- Distillation: [`docs/plans/2026-09-14-distillation-design.md`](docs/plans/2026-09-14-distillation-design.md)
- Wiki mode: [`docs/plans/2026-09-15-wiki-mode-design.md`](docs/plans/2026-09-15-wiki-mode-design.md)
- Research notes: [`docs/research/`](docs/research/) · roadmap docs in [`docs/plans/`](docs/plans/)

## Status

Core milestones complete: unified memory (L0–L3 + auto-distillation),
two-layer agents, graph-guided recall, four-CLI session sync, doctor
self-test, wiki mode, fan-out judge, bilingual panel, Playwright E2E
suite (real daemon, zero mocks) and Rust integration tests on a
scriptable mock agent. CI: rust (ubuntu + windows), panel (node), E2E.

## License

[MIT](LICENSE)
