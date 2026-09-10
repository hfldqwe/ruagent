# AGENTS.md — working on ruagent

ruagent is a local-first, single-user agent engineering platform: a resident Rust daemon orchestrating coding-agent CLIs over ACP, with unified memory/knowledge, MCP & skill management. Rust core, TypeScript panel.

## Layout

```
crates/
  core/          domain types: Task, Run, events, RoutingDecision (pure, zero I/O)
  acp/           ACP client (JSON-RPC over stdio) + HarnessAdapter trait + adapters
  mock-agent/    scriptable ACP agent used by tests (first-class test asset)
  orchestrator/  Task/Run state machines, topologies, routing cascade
  memory/        six memory stores, injection contract, ingestion pipeline
  graph/         entity property graph (SQLite adjacency + temporal validity)
  mcp/           MCP registry + rmcp-based server exposing the platform
  policy/        permission cascade, routing rules, cost policy
  store/         repository traits + SQLite/LanceDB/JSONL implementations
  daemon/        axum assembly: local HTTP/WS API, static panel serving
cli/             ruagent CLI (clap)
panel/           web panel (TypeScript/React/Vite)
docs/            design + research
```

Dependency direction is strictly downward; `core` must stay I/O-free.

## Commands

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# panel
cd panel && npm ci && npm run build   # tsc + vite build
```

Run the daemon locally: `cargo run -p ruagent-cli -- serve` (serves the panel + API on localhost).

## Conventions

- Edition 2024, MSRV = current stable toolchain.
- No `unwrap()` outside tests; use `anyhow` at bin edges, `thiserror` for library errors.
- All writes to SQLite go through the single-writer actor in `store`; never open a second write connection.
- High-volume run events are append-only JSONL under `transcripts/`; SQLite holds queryable state only.
- New adapters must come with mock-driven tests; real-harness smoke tests are feature-gated (`--features smoke`).
- Windows is a first-class platform: no Unix-only assumptions (paths, process groups, signals).

## Testing philosophy

The mock ACP agent (`crates/mock-agent`) is the backbone of CI: every orchestration/policy/memory behavior must be testable without real harnesses or API keys. Property tests guard the injection contract (bounded, tagged, visible truncation).
