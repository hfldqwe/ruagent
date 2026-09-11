# ruagent

Local-first, single-user **agent engineering platform**: orchestrate multiple coding-agent CLIs (Claude Code, OpenCode, DeepSeek Harness, …) over the [Agent Client Protocol](https://agentclientprotocol.com), with a unified knowledge base, layered memory, centralized MCP & skill management, and full run observability.

## Quick start

```bash
# Prerequisites: protoc on PATH (lancedb needs it at build time)
#   Windows: choco install protobuf   (or extract under %USERPROFILE%/.protoc and add bin to PATH)
#   Linux:   sudo apt-get install protobuf-compiler
cargo build --release

# 1. Start the daemon (config bootstraps into ~/.ruagent on first run)
cargo run --release -p ruagent -- serve

# 2. In another terminal (or open http://127.0.0.1:8787 in a browser)
cargo run --release -p ruagent -- agents          # list registered harnesses
cargo run --release -p ruagent -- run "hello" --agent claude
cargo run --release -p ruagent -- status
cargo run --release -p ruagent -- skills          # SKILL.md library
cargo run --release -p ruagent -- skills-sync     # distribute to harness dirs
```

Every agent spawned by the daemon gets the platform's own MCP server
(`ruagent mcp-serve`) injected, exposing `memory_search` / `memory_write` /
`knowledge_search` / `knowledge_ingest` / `list_tasks` — agents remember
across sessions out of the box.

## What it does

- **ACP-native orchestration** — direct runs, fan-out-and-compare (side-by-side results, pick a winner), and review pipelines with bounded handoff; routing cascade (explicit > rules > default) with provenance events.
- **Unified memory** — six stores with namespace governance, supersession, and an injection contract that is property-tested to never blow an agent's context; bi-temporal entity graph ("what was true as of X"); every write audited.
- **Knowledge base** — hybrid retrieval (LanceDB vectors + SQLite FTS5, RRF fusion), zero LLM at query time; offline hash embedder by default, real embeddings via `RUAGENT_EMBEDDER=fastembed`.
- **Permission cascade** — deterministic rules > approver agent (unattended operation, no self-approval, high-risk always human) > human inbox; fail-closed.
- **Observability** — replayable JSONL transcripts, SSE live streams, per-agent cost stats, context-injection rendered on every run's timeline.

## Documentation

- Design: [`docs/plans/2026-09-11-ruagent-design.md`](docs/plans/2026-09-11-ruagent-design.md)
- Research notes: [`docs/research/`](docs/research/)
- Working on the code: [`AGENTS.md`](AGENTS.md)

## License

MIT OR Apache-2.0, at your option.
