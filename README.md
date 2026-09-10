# ruagent

Local-first, single-user **agent engineering platform**: orchestrate multiple coding-agent CLIs (Claude Code, OpenCode, DeepSeek Harness, …) over the [Agent Client Protocol](https://agentclientprotocol.com), with a unified knowledge base, layered memory, centralized MCP & skill management, and full run observability.

> Status: pre-MVP, under active development. Single daemon, local-first, zero external services.

## What it is

- **ACP-native orchestration** — one resident Rust daemon drives every connected coding-agent CLI through the standard Agent Client Protocol: direct runs, fan-out-and-compare, and review pipelines.
- **Unified memory & knowledge** — a six-store memory model with namespace governance, bounded context injection, and a vector-backed knowledge base; exposed to *every* agent as an MCP server.
- **Central MCP & skill management** — one registry, injected per-run via ACP, exportable to each CLI's native config; SKILL.md-standard skills synced across harnesses.
- **Observability** — replayable run transcripts, per-agent/task cost accounting, and context-level telemetry (see exactly what each agent was injected with).

## Stack

Rust (daemon + CLI, tokio/axum) · TypeScript/React (web panel) · SQLite (WAL) + LanceDB (vectors) + append-only JSONL transcripts.

## Documentation

- Design: [`docs/plans/2026-09-11-ruagent-design.md`](docs/plans/2026-09-11-ruagent-design.md)
- Research notes: [`docs/research/`](docs/research/)

## License

MIT OR Apache-2.0, at your option.
