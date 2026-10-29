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
2. **Build** (matrix: `windows-latest` / `ubuntu-latest`) -- builds `ruagent` in release, builds the panel,
   packages the archive, **smoke-tests that archive**, and only then uploads the artifact.
3. **Release** -- downloads the artifacts and creates the GitHub release (`contents: write` is granted to
   THIS job only; the workflow default is `contents: read`).

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

## Known gaps

* **macOS is not published.** The matrix covers `x86_64-pc-windows-msvc` and `x86_64-unknown-linux-gnu`; the
  macOS entries are deliberately absent rather than untested-and-hoped-for.
* **`e2e.yml` and `release.yml`... `release.yml` is pinned now; `e2e.yml` still takes the action's default
  toolchain**, which `check-toolchain-pin.sh` reports as a WARN on every run rather than failing silently.
* **The download itself can be truncated on this network.** A `gh release download` of the 97 MB `v0.1.0`
  asset produced a 3.2 MB file that was not a zip, while the API reported the full 97,274,294 bytes -- a
  transport truncation, NOT a corrupt asset. Check `sha256sum` against `SHA256SUMS` before concluding that a
  published archive is broken.