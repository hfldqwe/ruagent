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

# panel -- this command has ALWAYS covered the panel's type check, inside the
# wrapper: panel/scripts/build-panel.mjs runs `node e2e/i18n-check.mjs` (L33),
# then `npx tsc -b` (L34), then `npx tsc -p e2e/tsconfig.json --noEmit` (L35),
# and only then vite (L44); any non-zero step exits and leaves dist untouched.
# Read the wrapper, do not judge this command by its one-line script name --
# t53 read the name and got the history wrong (t55 corrected it).
cd panel && npm ci && npm run build
```

**What these gates do NOT cover (read this before claiming a green).** The three Rust
commands above (`fmt` / `clippy` / `test`) compile and lint **the Rust workspace only**.
`panel/` is not a Rust package, so a TypeScript type error, a broken antd prop, or a dead
import in `panel/src/**` is invisible to them: the panel's own gate is
`cd panel && npm run build`. (That gate **does** cover the type check, and always has: the
`build-panel.mjs` wrapper runs `i18n-check`, `tsc -b`, `tsc -p e2e/tsconfig.json --noEmit` and
then vite, with a non-zero step aborting before anything writes `dist`. The `check` script in
`panel/package.json` names the same two `tsc` steps, but no verify command ever called it by
name — a statement about that script's callers, not about the gate's coverage.) The panel
e2e suite is a third, separate gate: run it through the repo entry point
(`cd panel && npm run test:e2e`, which is `node e2e/run-e2e.mjs`), never through a bare
`npx playwright test`; specs that write real daemon config must call `writeAccess()` and
`test.skip()` unless `RUAGENT_E2E_ALLOW_WRITES=1`, i.e. unless they came through that entry
point. So: **no Rust gate protects the panel** — a workspace-green run says nothing about
`panel/`, and vice versa.

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
* `stop` **refuses to run without `-Force`**. A script cannot post to the team channel, so
  rather than pretend to announce it refuses to be silent: without `-Force` it stops nothing,
  prints what the stop would break, and prints the text a human should send. With `-Force` it
  first **appends** a notice to `<root>/logs/daemon-stop-notice.log` (rotated to `.1` at 256 KB,
  never overwritten) and then stops the recorded pid; the notice holds the timestamp, the pid,
  the reason (`-Reason`) and that broadcast text. **The notice is for looking back after the
  fact, not for members watching in real time -- and seeing it does not mean anyone knew:** only
  the human step puts it in the team channel. `watch` and `install-task` never stop anything,
  so the unattended path cannot be blocked by that gate.

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
- **Format before you submit** (t66). CI's first gate is `cargo fmt --all --check`; a task that
  leaves unformatted Rust does not fail *itself* — it fails the **next push** (observed: **177
  diff sites** accumulated across the tree, concentrated in files no gate ever reformatted:
  `daemon/src/distill.rs`, `graph/tests/{fixture,extraction-gold,multihop-gold,live-after}.rs`,
  `store/src/migrations.rs`). Run `cargo fmt --all` (or `rustfmt` on the files you touched)
  before you finish, and **do not** reformat files you did not touch — that turns a local diff
  into a repository-wide one that cannot be reviewed. Because `cargo fmt --all` rewrites the
  whole tree, whoever runs it while others are editing must treat it as a race: snapshot
  `git status --porcelain` before and after, and re-run the gates on the new bytes if a file
  moved under you. Never `git stash` a shared tree to get a "before" reading.
- **Mutation negative controls must not redden the shared tree** (t93/t81, adopted from a member's
  self-filed finding). A control that temporarily reverts *shared source* to prove a test can fail
  — `unwrap_or_default()` restored, a transaction degraded to autocommit, an assertion's subject
  renamed — makes **everyone's** `test --workspace` red for that window. It has already caused one
  false alarm (a red at `crates/mcp/src/lib.rs:495` was a control window, not a real failure, and a
  second reader misdiagnosed it as "test written, implementation not landed"). Either run such a
  control in an **isolated checkout / `git worktree`**, or **announce it first** (which files, which
  tests, expected duration), keep it **time-boxed**, and report the **window start/end plus the
  restored-green reading**. A control that is *not* announced is indistinguishable, to every other
  member, from the bug it is meant to catch.
- **An isolated `git worktree` build gets its OWN `CARGO_TARGET_DIR`** (t19, from the increment-2
  verification). A worktree at HEAD carries **pre-change sources**, so a build that shares the main
  target dir (`D:\rust_cache`) leaves artifacts that contradict the main tree -- and that is how
  this phantom was made, not a theory: the worktree baseline builds of this effort are the likely
  origin. The symptom is a binary build failing on symbols the source plainly declares: `cargo rustc
  -p ruagent --bin ruagent -o <private path>` failed with **7 bogus errors** --
  `cannot find type CapabilityFile in crate ruagent_policy`
  (`crates/daemon/src/capability.rs`), `struct PolicyConfig has no field named capabilities` --
  **immediately after a GREEN `cargo clippy`**, because the stale `libruagent_policy-*.rlib` in the
  shared dir had been built from the other tree. Read that as an artifact problem, not a source
  regression: it is the same class as the two surprises this effort already hit -- a compiled
  `ruagent-store` reporting `MIGRATIONS=25` while the source declared 26, and a compiled
  `ruagent-mcp` listing 14 tools while the source declared 18. **The shared target dir is the
  leading explanation for all three**, so clean and rebuild before believing a red, a missing symbol
  or a surprising count.
  * Remedy: `cargo clean -p <package>` (for the policy case above, `ruagent-policy`), or `-p` over
    the workspace packages when several are suspect; measured ~1m24s, after which the same
    `cargo rustc ... -o <private path>` succeeds. `cargo build -p ruagent --bin ruagent` compiles
    the same units and can hit the same artifact; it also cannot replace
    `D:\rust_cache\debug\ruagent.exe` while a resident daemon holds that image open (`failed to
    remove file ... os error 5`), which is why `-o <private path>` is the shape to use on this
    machine.
  * Rule: `$env:CARGO_TARGET_DIR='D:\rust_cache_wt\<name>'` for every worktree build. Never point a
    worktree build at the shared dir.
  * A FRESH target dir also loses the cached build script that was finding `protoc`: the
    prerequisite above assumes the shared dir. Set it explicitly
    (`$env:PROTOC="$env:USERPROFILE\.protoc\bin\protoc.exe"` -- on this machine
    `C:\Users\19410\.protoc\bin\protoc.exe` -- or that `bin` on `PATH`), or the cold build dies on
    prost-build's `Could not find protoc` message. Measured: a cold baseline build of the daemon
    binary in its own target dir, dependencies from scratch, took **9m03s**.
- **Binding is loopback by default** (t76). `ruagent serve` binds `127.0.0.1:8787` unless
  `--addr` says otherwise; a non-loopback address additionally requires **explicit consent**
  (`--allow-remote`, or `RUAGENT_ALLOW_REMOTE=1|true|TRUE|yes`), and without it startup fails
  non-zero with a WARN that names the **actual** bound address and states that there is no
  authentication. `serve(root, addr)` remains a thin wrapper over
  `serve_with_remote(root, addr, allow_remote)`. Do not relax this default when touching `cli/`
  (still unowned) or `daemon`: the API has no auth, and a reachable instance can rewrite memory
  bodies (`POST /api/v1/memory/migrate-distilled-prefix`).
- **`cli/` has no dedicated owner yet** (t45/t54). `cli/src/main.rs:809` therefore carries a
  **scoped** `#[allow(clippy::items_after_test_module)]` on `mod tests` plus a reason comment:
  the test module sits mid-file (nine functions are defined after it), and moving ~500 lines to
  the end is a far larger, review-unfriendly diff in an unowned file than the lint it silences.
  That attribute is **intentional, not leftover debt**: when `cli/` gets an owner, the move is
  the better fix, and the comment is the marker for exactly that. Do not delete the comment and
  do not "fix" the allow by moving code as a drive-by.

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

### Processes this repo starts must not put a window on screen

Two shapes have cost real time, and both are about the window rather than the work.

**A WMI launch hands its child a console unless told otherwise.** The start action builds
the command as `cmd.exe /c ...` (that is the redirection shape that appends to the log
instead of truncating it) and creates it through `Win32_Process.Create`. Pass the startup
information with `ShowWindow = 0`, or every start -- and every restart `watch` performs --
puts a console window in front of whoever is using the machine:

```powershell
$startup = ([wmiclass]'Win32_ProcessStartup').CreateInstance()
$startup.ShowWindow = 0
$res = ([wmiclass]'Win32_Process').Create($cmd, $cwd, $startup)
```

**A killed tool call does not necessarily kill its descendants.** When a long probe is
cancelled at a wall-clock ceiling, the `cmd.exe`, `node` and browser processes it started
can survive it as orphans. So: wrap anything that starts a child in `-WindowStyle Hidden`,
run Playwright through the repo entry point (`node e2e/run-e2e.mjs`) rather than
`npx playwright test` (which goes through a `cmd.exe` shim), have a probe close its own
browser, and at the start of the next call look for orphans **by command line matching
your own temporary path** -- never by process name or by port.
