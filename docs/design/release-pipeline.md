# The release pipeline

How a downloadable build is produced, what it is allowed to contain, and what had to be measured to make
`download, unpack, run` actually work. Written after `v0.1.0` shipped one Windows zip that could not serve
its own UI, and after `v0.1.1` became the first archive CI started before publishing.

## Cutting a release

```bash
git tag -a vX.Y.Z -m '...'
git push git@github.com:<owner>/ruagent.git vX.Y.Z    # HTTPS may be blocked; SSH is the fallback
```

`release.yml` triggers on `push: tags: ["v*"]` and runs three jobs.

1. **Gate** -- asks the Actions API for the CI verdict of the tagged commit and refuses to build unless the
   newest completed run for it succeeded. No CI run at all, a red one, or one still running past the wait
   is `fail`. A tag is a promise; this job is what makes the promise checkable.
2. **Build** (matrix: `windows-latest` / `ubuntu-latest` / `macos-latest`) -- builds `ruagent` in release,
   builds the panel, packages the archive, **smoke-tests that archive**, and only then uploads the artifact.
3. **Release** -- downloads the artifacts and creates the GitHub release (`contents: write` is granted to
   THIS job only; the workflow default is `contents: read`).

### Proving a new target WITHOUT publishing it

`workflow_dispatch` takes a `dry_run` input (boolean, default `true`) and runs the same gate + build +
package + smoke + artifact upload. Two guards keep it from publishing, and both are keyed to the EVENT
rather than to the input: the tag/version step is `if: github.event_name == 'push'`, and so is the release
job. A dispatch that passes `dry_run=false` therefore still publishes nothing -- the input exists to make
the intent readable, not to be a switch a typo can flip. The green CI-verdict gate still applies on a
dispatch, because it asks about the COMMIT, which a branch push also has.

This is how macOS arrived. `aarch64-apple-darwin` was not added to the matrix and left for the next tag to
discover: a tag publishes, so the first evidence for a new target would have been a broken download. The
dry-run dispatch built, packaged and started the macOS archive on a real macOS runner --
`SMOKE OK: ruagent-main-aarch64-macos.tar.gz runs from its own contents and serves the panel` -- and only
then did macOS join the matrix, shipping from `v0.1.2`.

## What an archive must contain, and why the paths are a contract

```
ruagent.exe | ruagent        the CLI, which is also the daemon (`ruagent serve`)
panel/dist/...              the web panel, BESIDE the executable
README.md  LICENSE*         what the download tells you and what it is licensed under
SHA256SUMS                  per-file digests, with the archive's own sum published beside the asset
```

`panel/dist` is not a stylistic choice. `crates/daemon/src/api.rs` resolves the panel in this order:
`RUAGENT_PANEL_DIST`, **`<exe dir>/panel/dist`**, `<build tree>/panel/dist`, `./panel/dist`. A release therefore
has exactly one candidate it can satisfy -- the second -- which means the archive must place the panel at
`panel/dist` relative to its root. `v0.1.0` packaged it as `panel-dist/` inside a `staging/` wrapper, so every
candidate missed and a user who unpacked it got the API and a 404 on `/`. The fix's proof is a log line, not
an inspection: `INFO ruagent_daemon::api: serving the web panel dist=...\panel\dist`.

## Nothing is published that CI has not started

`.github/scripts/smoke-archive.ps1` (Windows) and `smoke-archive.sh` (Linux) take the archive the job just
built and treat it like a user would: unpack it, run `--version`, start a real daemon on a private root and a
free loopback port, require **both** `/api/v1/health` = 200 **and** `/` = 200 carrying the panel marker, print
the embedder from `/api/v1/stats`, then stop through `data/daemon.stop` (forcing only after 10 s, and saying
so when it has to).

It refuses on the failure that matters, and it says which one it found: with only the panel-path bug present
the script prints all four candidates -- marking the one a package can satisfy -- lists where `index.html`
actually sits, and ends `every candidate above misses and / would answer 404 (the t188 shape)`. A test that
cannot fail is not a check, so that red reading was produced on purpose before the script was trusted.

It also verifies the archive's own `SHA256SUMS` before trusting anything inside it. That check did not exist
in the first version: the smoke test unpacked the archive and ran whatever was in it, which would have
passed a tampered or half-written file just as happily. Writing one wrong digest into the manifest turns
the run into `panel/dist/index.html: FAILED` + `SMOKE FAILED: the archive SHA256SUMS does not match the
files it shipped`, exit 1.


**A model download must not read as a broken archive** (v0.1.3's Windows leg). The daemon's boot path tries
the ONNX-backed FastEmbedder, which downloads its model on first use (`crates/daemon/src/lib.rs`), so a cold
runner whose model host is slow can spend the whole health budget downloading. On v0.1.3 that produced
`no 200 from .../api/v1/health within 120 s` on Windows while the SAME bytes passed on Linux and macOS in the
same run, with the daemon log stopping right after `agent registry loaded` -- i.e. a red that named a network
fact, in the one script whose job is to judge the archive. Re-running the failed job on the same tree passed,
which is what makes this a transient rather than a defect. The budget is now 240 s, and a timeout retries once
with `RUAGENT_EMBEDDER=hash`, printing a WARN that names exactly which reading was lost; only a daemon that
cannot boot either way fails the gate.

`smoke-archive.sh` runs on both Linux and macOS, which is not free: `sha256sum` is GNU (macOS has
`shasum -a 256`), `xargs -r` is GNU (BSD xargs does not have it), `timeout` is absent on macOS, `seq` is not
guaranteed, and bash there is 3.2 (no arrays, no `${x,,}`, no `mapfile`). Each is either probed with a
working alternative or **fails loudly naming what is missing** -- a checksum step that silently skips is
worse than no checksum step, because the run still says OK.

## The ONNX question, answered with evidence

`ort 2.0.0-rc.13` reaches both `ruagent` and `ruagent-daemon` through `fastembed` -> `ruagent-knowledge`, so
the obvious worry is that a downloaded binary needs `onnxruntime.dll`/`.so` that the archive does not ship.
It does not, and this was established four ways rather than assumed:

* `cargo tree -e features -i ort` -- `download-binaries` is enabled, `load-dynamic` is not: the runtime is
  linked in, not dlopen'd at run time.
* The product's PE import table lists 35 DLLs and no `onnx*`; it does import `directml.dll`, `d3d12.dll` and
  `dxgi.dll`, and DirectML ships with Windows.
* The embedding path was actually exercised from a packaged build: it fails to *download* `onnx/model.onnx`
  while offline and falls back to the hash embedder, which `/api/v1/stats` confirms.
* Therefore a shipped runtime library would have changed nothing.

The trigger to revisit is recorded in the workflow: if `ort` ever enables `load-dynamic`, the smoke test's
embedder line goes red first.

## Verifying a published asset by hand

```bash
gh release download vX.Y.Z --pattern '*windows.zip' --dir /tmp/rel
cd /tmp/rel && sha256sum -c <(grep ruagent.exe SHA256SUMS)   # after unpacking
unzip -q ruagent-vX.Y.Z-x86_64-windows.zip -d unpacked && cd unpacked
./ruagent.exe --version
./ruagent.exe serve --addr 127.0.0.1:8790   # then: curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:8790/
```

`GET /` must be 200 with the panel markup -- not merely a live API. `doctor` runs the platform's own
seven-check self-test and is the fastest way to tell a broken download from a broken machine.

## Version and tags

`[workspace.package] version` in the root `Cargo.toml` is the single authority: every crate inherits it, so
`ruagent --version` reports it. The release tag must be `v` + that version, and the gate job now enforces
it with `check-release-version.sh` before anything is built.

That guard exists because of a real miss. `v0.1.1` was cut from a commit whose version had not been bumped,
so its archives self-report `ruagent 0.1.0`. Nothing caught it: the CI gate was green (it was), and the
smoke test proved the archive runs and serves its panel (it does). Neither of those looks at the version
string, and the disagreement only surfaces when a user asks which build they have -- so it is cheaper to
refuse the tag than to explain the release. The three readings that prove the guard works: the published
`v0.1.1` manifest fails it (`tag 'v0.1.1' does not match the workspace version '0.1.0'`), a manifest with
no readable version fails it (`the guard is blind, which is a failure, not a pass`), and the current tree
against `v0.1.1` passes.

## Known gaps

* **Intel Macs are not published.** macOS ships as `aarch64-apple-darwin` (Apple silicon) from `v0.1.2`; the
  x86_64 macOS target is absent because no run has produced one, and an untested target is not a download to
  offer. Adding it means a matrix entry plus its packaging and smoke steps, proven first through the dry-run
  dispatch above.
* **`v0.1.1` archives report `ruagent 0.1.0`** in `--version`, per the miss described above; the version was
  bumped to `0.1.1` afterwards, so every release from now on matches its tag.
* **`e2e.yml` still takes the action's default toolchain.** `ci.yml`, `audit.yml` and `release.yml` pin
  `1.95.0` and `check-toolchain-pin.sh` fails on a mismatch; `e2e.yml` is outside that list and is reported
  as a WARN on every run rather than failing silently.
* **The download itself can be truncated on this network.** A `gh release download` of the 97 MB `v0.1.0`
  asset produced a 3.2 MB file that was not a zip, while the API reported the full 97,274,294 bytes -- a
  transport truncation, NOT a corrupt asset. Check `sha256sum` against `SHA256SUMS` before concluding that a
  published archive is broken.