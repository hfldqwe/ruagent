# ruagent

Local-first, single-user **agent engineering platform**: orchestrate multiple coding-agent CLIs (Claude Code, OpenCode, DeepSeek Harness, Codex, …) over the [Agent Client Protocol](https://agentclientprotocol.com), with a unified memory layer, knowledge graph + hybrid retrieval, session distillation, and full observability.

**中文简介**：本地优先的多智能体工程平台——统一运行时（任意 CLI 引擎可切换）、统一记忆（跨 CLI/跨会话自动同步 + 蒸馏）、知识图谱 + 语义召回、双策略检索、面板与 CLI 双入口。

```
┌───────────────────────────── ruagent ─────────────────────────────┐
│  roles (portable prompts)      architect · plugin-dev · …        │
│    └ runs on any runtime       dsh · claude-code · opencode      │
│  unified memory               L0 transcripts → L3 injected digest│
│    └ distills automatically   sessions → memories + graph        │
│  unified knowledge            vector + FTS hybrid + graph-guided │
│  every agent gets             memory_recall · memory_get · … MCP │
└───────────────────────────────────────────────────────────────────┘
```

## Quick start

```bash
# Prerequisites: protoc on PATH (lancedb needs it at build time)
#   Windows: choco install protobuf   (or extract under %USERPROFILE%/.protoc)
#   Linux:   sudo apt-get install protobuf-compiler
cargo build --release

cargo run --release -p ruagent -- serve     # daemon + panel on :8787
cargo run --release -p ruagent -- doctor    # 7-check end-to-end self-test
```

Open **http://127.0.0.1:8787** — chat with any agent, browse unified session
history, recall across memories/knowledge/graph, watch runs live.

## The two-layer agent model

`~/.ruagent/config/agents.toml` separates **what an agent is** from **which
engine runs it**:

```toml
[runtime.dsh]                    # engines: how they spawn
command = "dsh --profile acp"

[agent.architect]                # roles: portable prompts
prompt = "You are the architecture reviewer. …"
runtimes = ["dsh", "claude-code", "opencode"]
runtime = "dsh"                  # default engine
```

The role prompt travels across runtimes: chat with the architect on dsh,
switch the runtime to claude-code mid-conversation, same role. Legacy
single-layer configs keep working.

## The memory layer

| Layer | What | Token cost |
|---|---|---|
| **L0 episodic** | verbatim session transcripts — user + agent, zero judgment, never destroyed | disk only |
| **L1 semantic** | distilled memories with quality signals (user corrections extract the *corrected* fact; confirmations raise confidence) | on recall |
| **L2 structural** | knowledge graph — entities, relations, bi-temporal facts, deterministic supersession | on recall |
| **L3 digest** | injected at a chat's first prompt: profile + semantic recall on the user's opening message | ~1k, once |

Sessions from **claude-code, dsh, opencode and ruagent** auto-sync into one
history (60s incremental scan). With `[distill] auto = true` in `policy.toml`,
closing a session distills it into memories + graph in the background.

**Recall, two strategies** (`GET /api/v1/recall` / the `memory_recall` MCP tool):
- **aggressive** — full content above a relevance threshold, RRF-fused
  (semantic + keyword), ready to use
- **conservative** — stubs only: entity names + live relations + chunk ids;
  the agent pulls details via `memory_get` / `graph_entity` on demand

## Agents get platform memory for free

Every spawned agent receives the platform MCP server with
`memory_search` · `memory_recall` · `memory_write` · `memory_get` ·
`knowledge_search` · `knowledge_ingest` · `list_tasks`.

## More

- **Runs**: direct, fan-out-and-compare (pick a winner), review pipelines;
  routing cascade with provenance; permission cascade (rules > approver agent
  > human inbox, fail-closed).
- **Knowledge**: LanceDB vectors + SQLite FTS5 + RRF, zero LLM at query
  time; fastembed (bge-small-en-v1.5) by default with an offline hash
  fallback; heading-aware chunking (topics never mix).
- **Skills**: SKILL.md library synced into every harness's skill dirs.
- **Observability**: replayable JSONL transcripts, SSE live streams,
  per-agent cost stats, context injection rendered on every timeline.
- **CLI**: `serve · doctor · agents · status · run · skills · skills-sync · mcp-serve`.

## Documentation

- Design: [`docs/plans/2026-09-11-ruagent-design.md`](docs/plans/2026-09-11-ruagent-design.md)
- Memory layer: [`docs/plans/2026-09-14-unified-memory.md`](docs/plans/2026-09-14-unified-memory.md)
- Distillation: [`docs/plans/2026-09-14-distillation-design.md`](docs/plans/2026-09-14-distillation-design.md)
- Roadmap & status: [`docs/plans/`](docs/plans/) · research notes in [`docs/research/`](docs/research/)

## Status

M0–M4 complete (memory layer, two-layer agents, graph-guided recall,
auto-distillation, four-CLI session sync, doctor). CI: rust (ubuntu+windows),
panel (node). License: see [LICENSE](LICENSE).
