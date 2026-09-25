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
# Build prerequisite: protoc on PATH (lancedb needs it).
#   Windows: install under %USERPROFILE%/.protoc and add its bin to PATH, or `choco install protobuf`
#   Linux:   `sudo apt-get install protobuf-compiler`
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# panel
cd panel && npm ci && npm run build   # tsc + vite build
```

### Running the daemon (the reliable start)

`ruagent serve` resolves the panel **independently of the working directory** (t188): the
first of `RUAGENT_PANEL_DIST`, `<exe dir>/panel/dist`, `<repo>/panel/dist` (derived from the
build tree the binary came from) and `./panel/dist` that actually holds an `index.html`
wins. Before that fix, starting the daemon from anywhere but the repo root silently served
the API with no panel — a 404 on `/`, indistinguishable from a broken build. Starting it
from the repo root is still the habit to keep: it makes the log say which directory it picked.

Start it through the script rather than a bare background job:

```bash
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/ruagent-daemon.ps1 start
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/ruagent-daemon.ps1 status
```

* `start` launches the daemon through WMI (`Win32_Process.Create`), so the process belongs
  to the WMI service rather than to the calling shell. `nohup ... &` dies with the shell
  call it was started from, and `Start-Process -RedirectStandardOutput` both hung and
  **truncated** the previous log — which is how the evidence of the previous death was lost.
* Output goes to `~/.ruagent/logs/daemon.log`, **appended** (rotated to `daemon.log.1` at
  5 MB). A daemon whose output goes nowhere is undiagnosable: two deaths left no panic, no
  error and no process behind.
* `status` prints the health check, the recorded pid and the log tail. `watch` starts the
  daemon only when the health check fails, and `install-task` registers it as a scheduled
  task (at logon + every 5 minutes) so nobody has to remember to restart it.
* `stop` stops the pid recorded in `~/.ruagent/data/daemon.pid` — never a sweep by process
  name or port.

`cargo run -p ruagent -- serve` still works for a foreground run (it resolves the panel the
same way); it simply dies with the terminal.

The CLI package is named `ruagent` (it lives in `cli/`), not `ruagent-cli`.

## Conventions

- Edition 2024, MSRV = current stable toolchain.
- No `unwrap()` outside tests; use `anyhow` at bin edges, `thiserror` for library errors.
- All writes to SQLite go through the single-writer actor in `store`; never open a second write connection.
- High-volume run events are append-only JSONL under `transcripts/`; SQLite holds queryable state only.
- New adapters must come with mock-driven tests; real-harness smoke tests are feature-gated (`--features smoke`).
- Windows is a first-class platform: no Unix-only assumptions (paths, process groups, signals).

## E2E specs that write the daemon's real config

Most of panel/e2e/ is read-only: it drives the panel and the HTTP API and changes nothing.
**One spec writes real data**, and running it against a daemon whose config matters is a
data-loss-shaped mistake:

| spec | what it writes |
| --- | --- |
| panel/e2e/registry.spec.ts | creates a **runtime** and a **role** in the daemon's own config (agents.toml), through the UI, and deletes them again |

Two protections, because one was not enough:

1. **It cannot run by accident.** panel/e2e/write-guard.ts exposes writeAccess(), true only when
   RUAGENT_E2E_ALLOW_WRITES=1. That flag is set by the suite's single entry point,
   npm run test:e2e -> panel/e2e/run-e2e.mjs, and by nothing else. A bare npx playwright test
   does **not** set it, so the spec skips itself and prints why.
2. **It cannot leave residue.** The cleanup runs from a finally block and calls the API directly
   (DELETE /api/v1/agents/{name}, DELETE /api/v1/runtimes/{name}) instead of clicking through the
   UI. The UI path is what failed in practice: the cleanup lived in a later step, so a failure in
   an earlier step skipped it and left [runtime.e2e-rt] and [agent.e2e-role] behind.

When adding a spec that writes anything, call writeAccess() and test.skip() on it, and add the
spec to the table above.

To run the suite the intended way (private artefacts, write access armed):

```bash
cd panel && npm run test:e2e -- --output=/tmp/pw
```

## Testing philosophy

The mock ACP agent (`crates/mock-agent`) is the backbone of CI: every orchestration/policy/memory behavior must be testable without real harnesses or API keys. Property tests guard the injection contract (bounded, tagged, visible truncation).
