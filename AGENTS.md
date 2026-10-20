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
* `status` prints the health check, the recorded pid, the **exit trace** and the log tail.
  `watch` starts the daemon only when the health check fails, and `install-task` registers
  it as a scheduled task (at logon + every 5 minutes) so nobody has to remember to restart it.
* **A death leaves evidence: `<root>/data/daemon.exit`** (issue #64, ruagent-issues-56-59
  t20). The daemon writes `running pid=... since=...` at boot and overwrites that line with
  a reason on every path that knows why it is leaving (`stop-request`, `signal:console`,
  `panic`, `error`, `server-returned`). A **killed** daemon is the one case that cannot
  overwrite it, so the trace it leaves is its own boot line -- and the fact that its pid is
  gone is what turns that line into a diagnosis: `status` prints the line and then either
  `pid N is ALIVE, so that line is this daemon's boot record (it has not left)` or
  `DIED WITHOUT A TRACE -- pid N is gone and NO exit record was written: killed (job object /
  TerminateProcess / power loss), not shut down`. The next boot logs the same conclusion as a
  WARN. Measured 2026-10-19 on a private root: a hard kill left `running pid=34352 since=...`
  and `status` reported `DIED WITHOUT A TRACE`; a graceful stop left
  `exit reason=stop-request pid=34068 at=2026-10-19T15:22:18.354Z`; a real panic through the
  installed hook left `exit reason=panic ... detail=t20 deliberate panic at
  crates/daemon/src/lib.rs:842:44`. The 2026-10-17 disappearance (the log ending on an
  ordinary INFO line, no panic, no error, the process gone, the port refusing) is exactly the
  shape the kill reading now names.
* **`stop` asks for a clean exit before it forces one.** A WMI-started daemon has no console
  to send Ctrl-C to, so `stop` writes `<root>/data/daemon.stop` and waits up to 10 s; the
  daemon polls that file every 250 ms and leaves through its own shutdown path, writing
  `exit reason=stop-request`. Only if the ask is ignored does `stop` force the process --
  and the forced case is the one with NO exit record, which is how the two become
  distinguishable. A stale request left behind by a crash is deleted at boot, so it can never
  kill the daemon that comes up after it.
* **Start method, measured (2026-10-19): WMI survives the caller; a shell-scoped start does
  not.** A daemon started through `Win32_Process.Create` was still alive (checked with
  `Get-Process` by image path) after the calling shell's call had returned, which is why
  `start` uses it; `nohup ... &` from that same shell does not survive it. The daemon also
  measures, at every boot, whether it was already inside somebody else's **job object** --
  logged as `startup job-object measurement ... in_an_outer_job_object=true|false`, taken
  BEFORE the orphan sweep creates the daemon's own job (after it, the answer is always yes).
  Measured `false` for a WMI start, so **job-object breakaway is left OPEN BY MEASUREMENT,
  not by preference**: requesting it on the strength of the 2026-10-17 symptom alone would also
  let the daemon escape a job that is deliberately containing it. If a future disappearance
  logs `in_an_outer_job_object=true`, breakaway becomes the evidenced fix.
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
  -p ruagent --bin ruagent -- -o <private path>` failed with **7 bogus errors** --
  `cannot find type CapabilityFile in crate ruagent_policy`
  (`crates/daemon/src/capability.rs`), `struct PolicyConfig has no field named capabilities` --
  **immediately after a GREEN `cargo clippy`**, because the stale `libruagent_policy-*.rlib` in the
  shared dir had been built from the other tree. Read that as an artifact problem, not a source
  regression: it is the same class as the two surprises this effort already hit -- a compiled
  `ruagent-store` reporting `MIGRATIONS=25` while the source declared 26, and a compiled
  `ruagent-mcp` listing 14 tools while the source declared 18. **The shared target dir is the
  leading explanation for all three**, so clean and rebuild before believing a red, a missing symbol
  or a surprising count. Measured twice more (ruagent-issues-56-59, in a PRIVATE target dir, so this
  is not only a shared-dir problem): the compiled `ruagent-memory` rlib lacked a type the source
  declared (`WriteError`, while the same tree's `cargo check` was green) and `ruagent-store`'s rlib
  lacked a column (`memories_refused`); `cargo clean -p <package>` turned both green. Same tell, same
  remedy -- clean first rather than investigating that shape as a source regression.
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
- **A red loopback test can be the machine's proxy, not your change** (recall-dev, increment 3).
  The Windows system proxy is enabled on this machine pointing at `127.0.0.1:7890` with nothing
  listening, and `reqwest` honours it because its `ProxyOverride` is not applied to loopback. Every
  in-test loopback call is therefore dialled at the PROXY address, and the panic names a socket the
  test never mentioned: `reqwest::Error { kind: Request, url: "http://127.0.0.1:62523/api/v1/recall?q=..." }`
  whose `source` is `ConnectError("tcp connect error", 127.0.0.1:7890, Os { code: 10061, ... })`.
  Observed instances: `recall_calls_are_logged` failing at `crates/daemon/tests/knowledge_api.rs:720`,
  and (increment-3 verification) 9 failures in `crates/daemon/tests/capabilities.rs` taking
  `cargo test --workspace` to **exit 101** on a tree whose diff touches neither file. Remedy: put the
  loopback hosts in `NO_PROXY` — `NO_PROXY=127.0.0.1,localhost,::1 cargo test --workspace` took the
  same run from exit 101 to exit 0 (66 targets, 587 passed, 0 failed). **The discipline matters more
  than the remedy:** reproduce the red on an UNTOUCHED baseline — a `git worktree` at HEAD with its
  OWN `CARGO_TARGET_DIR` (the worktree/`CARGO_TARGET_DIR` entry above) — before believing it, which is
  how this was diagnosed as the environment rather than reported as a regression. A red that names a
  test your change does not touch, panicking at a connect error to a port you did not bind, is the
  machine until the baseline says otherwise. Do not bypass a gate that is red for a reason that might
  be yours, and never report a gate as green when you ran it only with the bypass (or did not run it):
  say which command produced which reading.
  **The same dead proxy blocks any network client on this machine, not only tests** (ruagent-close-the-gaps
  t36). Pushing this effort's commits failed with `fatal: unable to access 'https://github.com/…':
  Failed to connect to 127.0.0.1 port 7890` — and every obvious place to look was CLEAN, which is what
  made it cost two attempts: `git config --get http.proxy` is EMPTY locally and globally, and
  `HTTP_PROXY`/`HTTPS_PROXY`/`ALL_PROXY` (and their lowercase spellings) are EMPTY. The proxy comes
  from the Windows system settings — `HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet
  Settings`, `ProxyEnable=1`, `ProxyServer=127.0.0.1:7890`, and a `ProxyOverride` list of unrelated
  domains that does NOT include github.com — so any client that reads the system proxy honours a proxy
  nothing is listening on. The remedy is a measured PAIR, and both halves reproduce without pushing
  (`git ls-remote origin HEAD`): `git -c http.proxy= -c https.proxy= …` STILL FAILS — an explicitly
  empty proxy does not disable the system one, curl falls back to it — while `$env:NO_PROXY='*'`
  succeeded on the first attempt (`e24263d..813d2f9 main -> main`, and `ls-remote` exit 0 under it).
  For CURL-BACKED CLIENTS that is the remedy, and it changes nothing on the machine — where the
  alternative is toggling the user's system proxy off and back on, their setting, probably there for
  a reason, changed to work around an environment fault. **But `*` is NOT the remedy for "any network
  client", which is how the first version of this paragraph read, and applying the wrong form
  manufactures the very red this entry warns about** (ruagent-close-the-gaps t39). MEASURED,
  `cargo test --workspace` with `NO_PROXY='*'`: **exit 101**, 9 failures in
  `crates/daemon/tests/capabilities.rs` — including `put_persists_the_table_and_get_reflects_it`,
  `a_file_table_activates_the_plane_and_narrowing_is_reported` and `an_empty_map_restores_legacy_mode`
  — every one an `Err` whose source is
  `ConnectError("tcp connect error", 127.0.0.1:7890, Os { code: 10061, ... })`, because `reqwest` does
  not treat `*` as a wildcard. Cargo stops at that failing target, so the run reports `184 passed / 9
  failed` on one FAILED target instead of the same tree's `73 targets, 660 passed, 0 failed` under the
  host list — most of the suite never runs. **The two clients cannot share one value**, measured three
  ways on `git ls-remote origin HEAD` (no push needed): `NO_PROXY=127.0.0.1,localhost,::1` → **exit
  128** (`Failed to connect to 127.0.0.1 port 7890`; curl never matches github.com against a loopback
  list), `NO_PROXY=127.0.0.1,localhost,::1,*` → **exit 128** as well (curl accepts `*` only as the
  WHOLE value, never as a list item), `NO_PROXY='*'` alone → **exit 0**. So, per real command: the
  three Rust gates (`fmt`/`clippy`/`test`) need the explicit host list, while `git push` and
  `git ls-remote` need `*` alone — do not try to build a combined value. The panel e2e entry point
  (`npm run test:e2e`) starts its own daemon and its proxy needs were NOT measured; assume neither
  value for it. The baseline-diff discipline above is unchanged, and this is that discipline one level
  down: the REMEDY has a scope too.
  **And when the host itself is unreachable, `gh` is not `git`** (ruagent-issues-56-59, measured): during
  that round `curl -4 https://github.com` returned 200 while `git push` failed with `Failed to connect to
  github.com port 443` on six consecutive attempts over ~15 minutes -- and `Test-NetConnection github.com
  -Port 443` was `False` while `api.github.com:443` was `True`, which is exactly why every `gh` command
  kept working. The route that worked is SSH: `ssh -T git@github.com` authenticated (so did
  `-p 443 git@ssh.github.com`), and `git push git@github.com:<owner>/<repo>.git main` succeeded on the
  first attempt. Two consequences: a push over the SSH URL does NOT update `refs/remotes/origin/main`, so
  `git status` keeps reporting 'ahead 1' after a push that landed -- align it with
  `git update-ref refs/remotes/origin/main <sha>`; and the remote's real state is readable through the API
  that still works (`gh api repos/<owner>/<repo>/commits/main -q .sha`).
- **With `CARGO_NET_OFFLINE=true`, this workspace fails to LINK on ONNX — so it reads as a source
  regression in a crate you never touched** (ruagent-close-the-gaps t14, from the increment-7 migration
  work). `cargo test --workspace` dies at link time with `link.exe` **error 1120** and unresolved
  `OrtGetApiBase` inside `ruagent-knowledge`, `ruagent-daemon` and siblings, on a diff that touches
  none of them — the shape a source regression has, not a configuration one. Cause, from `ort-sys`'s own
  build-script output: with the flag set it emits `cargo:rustc-cfg=link_error_generic` and **never emits
  its `rustc-link-search`** for the prebuilt ONNX Runtime, so the linker never sees the library even
  though it is PRESENT at `C:\Users\19410\AppData\Local\ort.pyke.io` (~341 MB `onnxruntime.lib`) — the
  red is not a missing download. Remedy, measured as a before/after pair rather than inferred: **do not
  set `CARGO_NET_OFFLINE` for this workspace** — unset it if a shell or wrapper sets it. The same command
  that died at link with the flag set linked in 14.96 s once it was removed. If you see this link error,
  suspect the environment before the diff: it is the same class as the proxy red above — the red names
  code the change does not touch — and the same discipline applies, but the tell is HERE rather than
  there: a link error in an ONNX-dependent crate outranks a baseline diff. **The obvious next move for
  that red makes it look like a different bug, so it is recorded here too:** pointing `ort-sys` at a
  library yourself with `ORT_LIB_LOCATION` makes it **skip its DirectML link directives**, and the
  link then dies with `LNK2019` on `DMLCreateDevice1` inside onnxruntime's DirectML provider
  factory — a DIFFERENT unresolved symbol, for a DIFFERENT reason, on the same untouched diff.
  The configuration that works is the plain one: **no ort environment override at all, with
  `CARGO_NET_OFFLINE` unset** (measured, not inferred: the same command links in ~16 s).
  Both shapes are environment, not source — a second unresolved symbol is not evidence that the first
  reading was wrong.
- **Cite a task by concept, or qualify the id with its team generation** (recall-dev, increment 3).
  Comments in this repo cite team task ids — `t5:` alone has 23 references in
  `crates/daemon/src/memembed.rs` — but the runtime allocates ids PER GENERATION, so this generation's
  `t5` is a DIFFERENT task from the previous generation's `t5`. A bare `tN` in a comment therefore
  points at an authority a later reader cannot resolve, and two generations of the same number read as
  one task. Convention: name the CONCEPT the comment protects (which behaviour, which invariant) and,
  when the id is worth keeping, qualify it with the team generation —
  `ruagent-tunable-capabilities t5`, the shape the generation ledger uses. Honest note: `AGENTS.md`'s
  own entries above cite bare ids from earlier generations (t66, t93/t81, t76, t45/t54, t188), so this
  is an improvement on current practice rather than a description of it; apply it to new comments and
  do not rewrite the old ones as a drive-by.
- **A green gate certifies the BYTES it ran on, so a post-handover edit must be disclosed and the gates
  re-run** (ruagent-tunable-capabilities t20, from the increment-5 handover). The increment-5
  implementer added two COMMENT-ONLY clarifications to `crates/daemon/src/capability.rs` AFTER handing
  the task over — prompted by a captain question about a sixth leg — while that increment's verification
  had already started: `git diff --numstat` for the file moved from `223/69` to `242/69` mid-run, and
  nothing on disk said why. The failure mode is silent, which is why this is written down: a green
  reading belonging to a previous revision looks EXACTLY like a green reading of the bytes in the tree,
  so the next reader cannot tell them apart, and a verification built on the earlier run certifies bytes
  it never saw. Two obligations follow.
  * **The author of the edit** re-runs the gates on the FINAL bytes and says which reading belongs to
    which revision. Avoiding the edit is not the rule ("never touch it after handover" is not always
    available — a review's question can require a clarification); DISCLOSING it and re-covering it is.
  * **A verifier or reviewer that sees a hash move** re-hashes, establishes the NATURE of the change BY
    READING THE DIFF — never by accepting the author's or the captain's summary of it — and states which
    revision each of its readings came from.
  What the honest path cost, and what the silent one hides: in that round the verifier's own copy of the
  file POSTDATED the write, so the 19 added lines could not be isolated at all; it refused to take
  "comment-only" on trust, re-ran all four gates against the final bytes, and reported the earlier run as
  unpinnable and superseded. One re-run and one sentence bought that. Without the disclosure the same
  moved hash is indistinguishable from a task whose green reading belongs to a different revision. Note
  who else this binds: a captain's follow-up question is one of the things that CAUSES a post-handover
  edit, so the duty to disclose and re-run sits with the author and with the captain who asked — not only
  with the next verifier.

- **A bare hex string in this repo is a `git hash-object` blob id unless the text says md5**
  (ruagent-issues-56-59; recorded after a real misdiagnosis, not a theory). Two 32-character hex
  values that differ are **not** evidence that a file moved: they can be the same bytes under two
  different functions. The captain compared a member's blob id against his own `md5sum` of the same
  file, concluded the file had changed after the reading, and wrote that claim into a closing brief;
  recomputing **both** functions on that one file showed a single content. Discipline: before saying
  "the file changed after that reading", recompute both (`git hash-object f; md5sum f`) and name the
  one you are quoting -- `docs/design/reviews/issues-45-55-closing.md` SS4 keeps a 10-row blob/md5
  table for exactly this reason. Related trap on a CRLF checkout: the working copy may be CRLF while
  the blob is LF, so `md5sum f` and `git show HEAD:f | md5sum` can legitimately differ **while
  `git status` stays clean** (observed on `scripts/ruagent-daemon.ps1`).
- **Never delete a probe artifact a report cites -- preserve it, or record that the citation died
  with it** (ruagent-issues-56-59; this one is not hypothetical, it is what the captain did). Cleaning
  this effort's probe leftovers out of the user's home removed `~/.dsh/sessions/...t8-acp...`, which a
  report cited for its per-turn token counts (input 11313 / output 703 / cacheRead 768). The verifier
  could still confirm the ledger side (`tokens_used=13224`) but had to report those numbers as
  **unreproducible from their cited source** -- and correctly noted the removal was NOT age-based
  cleanup, because its own daemon's directories were still there. The rule costs one `cp`; losing it
  costs a reading nobody can ever re-check. So: before deleting anything under a user home, grep the
  effort's reports for that path; if one cites it, copy it into the effort's OWN evidence root (a
  `%TEMP%\<task>\...` directory that is not cleaned mid-round) and leave the original alone until the
  round closes. The same applies to a task's private daemon roots: a fixture another task verifies
  against is evidence, not clutter.
- **A green Rust gate on this machine is a WINDOWS reading; a unix-sensitive change needs a Linux one**
  (ruagent-issues-56-59, measured twice in one round). `fmt`/`clippy`/`test` all run on Windows here, so
  CI's `Rust (ubuntu)` job is the first place a unix-only path breaks -- and in that round it did: the
  child-attribution work passed every local gate and failed 5 tests there. Two platform semantics, both
  measured rather than inferred: a child killed but not yet reaped still has a `/proc` entry with state
  `Z` and was counted as LIVE (Windows has no zombie state; measured `state=S` -> `Z` -> gone after
  `waitpid`), and `/proc/stat`'s `btime` is WHOLE SECONDS, so a just-spawned child's computed start time
  came out `delta_ms=996` early and the window filter excluded the session's own child. **WSL is NOT the
  loop on this machine**: `wsl -d Ubuntu-22.04` starts but has no `cc/gcc/make/protoc` and `sudo` wants a
  password, so rustc has no linker driver. The loop that worked is the existing podman machine with the
  local `postgres:16` image as a Debian base (apt + rustup inside as root), **tuna mirrors** for
  `static.rust-lang.org`/`index.crates.io` (direct is blocked from the container), the target dir in a
  container VOLUME (never `/mnt`), and tests as a NON-root user (`-u 1000`: as root, the cleanup
  assertions falsely red). The image `localhost/t28-rust:local` and the stopped container `t28b` are kept
  for re-runs; the repeatable setup steps are in that task's output and in the round's closing notes.
  Do not delete an `#[ignore]`d instrument to make a count match, and do not make a test skip on Linux --
  both remove the evidence; CI's ignored-instrument manifest exists to make every `#[ignore]` accounted
  for by name.

## E2E specs that write the daemon's real config

Most of panel/e2e/ is read-only: it drives the panel and the HTTP API and changes nothing.
**Two specs write the daemon's own CONFIG files** (and two more write run-scoped data —
`panel/e2e/write-guard.ts` carries the complete list). Running any of them against a daemon
whose config matters is a data-loss-shaped mistake:

| spec | what it writes |
| --- | --- |
| panel/e2e/registry.spec.ts | creates a **runtime** and a **role** in the daemon's own config (agents.toml), through the UI, and deletes them again |
| panel/e2e/settings-capabilities.spec.ts | edits the `[capabilities]` table of the daemon's own **policy.toml** (a `weight`), through the `#settings` option editor, and restores the table |

Two protections, because one was not enough:

1. **It cannot run by accident.** panel/e2e/write-guard.ts exposes writeAccess(), true only when
   RUAGENT_E2E_ALLOW_WRITES=1. That flag is set by the suite's single entry point,
   npm run test:e2e -> panel/e2e/run-e2e.mjs, and by nothing else. A bare npx playwright test
   does **not** set it, so the spec skips itself and prints why — and each spec passes
   writeAccess() what IT writes, so the skip reason names the right damage.
2. **It cannot leave residue.** The cleanup runs from a finally block and calls the API directly
   instead of clicking through the UI. The UI path is what failed in practice: the cleanup lived
   in a later step, so a failure in an earlier step skipped it and left [runtime.e2e-rt] and
   [agent.e2e-role] behind. registry.spec.ts deletes the runtime and the role;
   settings-capabilities.spec.ts PUTs back the whole `[capabilities]` table it snapshotted
   (restoring only the row it edited would still leave an empty `[capabilities]` table behind,
   and the mere presence of that table changes daemon behaviour).

When adding a spec that writes anything, call writeAccess() and test.skip() on it, and add the
spec to the table above.

To run the suite the intended way (private artefacts, write access armed):

```bash
cd panel && npm run test:e2e -- --output=/tmp/pw
```

- **An intermittent unix test failure needs a CONCURRENT probe, not another run** (ruagent-issues-56-59 t33,
  measured). One ubuntu assertion failed in a run and passed in the PREVIOUS run on identical code, with the
  reading `the snapshot reports the image of a live child: Some("ruagent_daemon-...")` -- the TEST BINARY'S OWN
  NAME, i.e. the parent's image. It is the fork/exec window: on unix `Command::spawn` returns after the fork and
  possibly before the exec, so `/proc/<pid>/exe` still names the parent. Hand reproduction failed for two rounds
  because the window needs CONCURRENCY: measured in the container as uid 1000, six threads spawning `sleep 30`
  gave **7 hits in 600 rounds (1.2%)**, reading the parent image 12-27 us after spawn with the child's own image
  appearing 40-189 us later, while **180 sequential rounds gave zero**. So when a Linux-only test fails
  occasionally: hammer it from several threads and time the first read. A green sequential run is NOT evidence the
  bug is gone -- the author of that repair said so about its own five green runs, and it was right. Fix the TEST's
  determinism (a bounded wait for the child to become itself), never the assertion's criteria, and then READ what
  the product path does inside the window instead of arguing it: here the parent image does not match the
  program's file name, so `attribute_child` declines, and under-attribution is the safe direction.
- **A guard test that parses another carrier is blind exactly where its parser is** (ruagent-issues-56-59 t29).
  The ignored-instrument manifest guard scans CI yml rows for `fn <ident>` names, and one RUN row's token
  contained the words ` names`, so the parser skipped that token -- the row LOOKED accounted for and was never
  statically checked. Give such a name its own row so it becomes checkable, and when you add a guard that reads a
  second carrier (workflow yml, `scripts/*.ps1`, panel TS), first print what the parser extracts and confirm the
  count is what you expect: this class fails SILENTLY, never with an error.

## Testing philosophy

The mock ACP agent (`crates/mock-agent`) is the backbone of CI: every orchestration/policy/memory behavior must be testable without real harnesses or API keys. Property tests guard the injection contract (bounded, tagged, visible truncation).

### Processes this repo starts must not put a window on screen

Three shapes have cost real time, and all three are about the window rather than the work.

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

**A scheduled task's action must not be a CONSOLE application.** `install-task` used to
register `powershell.exe -NoProfile -ExecutionPolicy Bypass -File <script> watch …`. The
principal is `Interactive`, so the task runs in the user's own session, and Windows hands a
console application started there to the machine's **default terminal** — on this machine
Windows Terminal (`HKCU:\Console\%%Startup` holds the all-zero "let Windows decide" GUIDs for
`DelegationConsole`/`DelegationTerminal`, and `wt.exe` is installed). The result was a
*visible* terminal window every 5 minutes; the user reported it as "a scheduled task keeps
popping a window up", and the attribution came from the cadence (`watchdog.log`, one line per
run at `:00:xx` and `:05:xx` all day) plus the console host observed at the launch.
`-WindowStyle Hidden` is **not** the fix: measured through real scheduled tasks and enumerated
with `EnumWindows`/`IsWindowVisible`, a powershell started with that flag still owns a
`visible=True` `PseudoConsoleWindow`, because the window on screen belongs to the delegated
terminal, not to powershell. The action therefore launches `conhost.exe --headless` (Windows
11 22000+, which does not consult the default terminal at all; below that build `install-task`
degrades to `-WindowStyle Hidden` and **says so** in its output). Measured for the headless
shape: no visible window and no `WindowsTerminal`/`OpenConsole` process at all, with the
watchdog still reaching its health check. `install-task -TaskName <throwaway>` exists so the
registration and its read-back can be verified WITHOUT touching the machine's own task.

**A killed tool call does not necessarily kill its descendants.** When a long probe is
cancelled at a wall-clock ceiling, the `cmd.exe`, `node` and browser processes it started
can survive it as orphans. So: wrap anything that starts a child in `-WindowStyle Hidden`,
run Playwright through the repo entry point (`node e2e/run-e2e.mjs`) rather than
`npx playwright test` (which goes through a `cmd.exe` shim), have a probe close its own
browser, and at the start of the next call look for orphans **by command line matching
your own temporary path** -- never by process name or by port.
