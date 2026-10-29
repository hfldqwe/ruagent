#!/usr/bin/env bash
# Prove a release archive RUNS, on a machine that never saw this repo (Linux/glibc leg).
#
# .WHY (ruagent-issues-56-59 t43) -- the same reason as smoke-archive.ps1, one platform over:
# the archive used to ship the panel as panel-dist/, a sibling of the binary, while the daemon
# resolves <exe dir>/panel/dist among four candidates (crates/daemon/src/api.rs). A user who
# unpacked the release got an API and a 404 on /. CI could not see it because the release job
# never ran what it had just packaged.
#
# WHAT IT CHECKS (identical to the PowerShell leg so the two archives are held to one contract)
#   1. layout: <exe name> at the archive root, panel/dist/index.html beside it;
#   2. ./ruagent --version succeeds;
#   3. a REAL daemon on a free loopback port with a private --root answers /api/v1/health 200,
#      AND / returns 200 with the PANEL body (not just an API);
#   4. /api/v1/stats names the embedder the daemon ended up with (the ONNX path's outcome --
#      see the evidence block in .github/workflows/release.yml);
#   5. a clean stop through <root>/data/daemon.stop, forced kill only as a reported fallback,
#      and the daemon's own log tail printed either way.
#
# usage: smoke-archive.sh <archive.tar.gz> [--exe ruagent] [--boot-timeout 120]
# exit: 0 the archive runs and serves the panel | 1 it does not | 2 usage
set -uo pipefail

archive=""
exe_name="ruagent"
boot_timeout=120
while [ $# -gt 0 ]; do
  case "$1" in
    --archive) archive="${2:-}"; shift 2 ;;
    --exe) exe_name="${2:-}"; shift 2 ;;
    --boot-timeout) boot_timeout="${2:-}"; shift 2 ;;
    -h|--help) sed -n '2,22p' "$0"; exit 0 ;;
    *) archive="$1"; shift ;;
  esac
done
if [ -z "$archive" ]; then
  echo "usage: $0 <archive.tar.gz> [--exe ruagent] [--boot-timeout 120]" >&2
  exit 2
fi

# A loopback probe must not travel through a configured proxy (this repo's machine has a dead
# 127.0.0.1:7890 in its Windows settings; GitHub runners have none). Harmless when unset.
NO_PROXY="127.0.0.1,localhost,::1"
no_proxy="$NO_PROXY"
export NO_PROXY no_proxy

stage="$(mktemp -d)"
root="$stage/root"
daemon_pid=""

fail() {
  echo "SMOKE FAILED: $*"
  if [ -n "$daemon_pid" ] && kill -0 "$daemon_pid" 2>/dev/null; then
    kill -9 "$daemon_pid" 2>/dev/null
  fi
  echo "kept for inspection: $stage"
  exit 1
}

if [ ! -f "$archive" ]; then
  echo "SMOKE FAILED: no archive at $archive" >&2
  exit 1
fi

tar -xzf "$archive" -C "$stage" || fail "tar could not read $archive"
top="$(cd "$stage" && ls -A | tr '\n' ' ')"
echo "archive root entries: $top"
[ -f "$stage/$exe_name" ] || fail "the archive has no $exe_name at its root; it holds [$top]"

if [ ! -f "$stage/panel/dist/index.html" ]; then
  # THE DIAGNOSIS. The candidate order is the daemon's own (crates/daemon/src/api.rs).
  echo "the daemon looks for the panel in this order (crates/daemon/src/api.rs):"
  echo "  1. \$env:RUAGENT_PANEL_DIST       = ${RUAGENT_PANEL_DIST:-<unset>}"
  echo "  2. <exe dir>/panel/dist        = $stage/panel/dist  <-- ABSENT IN THIS ARCHIVE"
  echo "  3. <build tree>/panel/dist     = (not present in a packaged archive)"
  echo "  4. <cwd>/panel/dist            = $(pwd)/panel/dist"
  found="$(cd "$stage" && find . -name index.html -print | tr '\n' ' ')"
  echo "index.html files anywhere in the archive: ${found:-<none>}"
  fail "the archive carries no panel/dist/index.html at its root, so EVERY candidate above misses and / would answer 404 (the t188 shape)"
fi

# 1. THE BINARY
if ! version="$("$stage/$exe_name" --version 2>&1)"; then
  fail "$exe_name --version failed: $version"
fi
echo "version: $version"

# 2. A REAL DAEMON on a free loopback port, private root (never the runner's ~/.ruagent)
port="$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1]);s.close()' 2>/dev/null || echo 0)"
if [ "$port" = "0" ]; then
  port="$(( (RANDOM % 20000) + 20000 ))"
fi
base="http://127.0.0.1:$port"
echo "starting: $exe_name serve --addr 127.0.0.1:$port --root $root"
"$stage/$exe_name" serve --addr "127.0.0.1:$port" --root "$root" >"$stage/daemon.out.log" 2>"$stage/daemon.err.log" &
daemon_pid=$!

code=""
deadline=$(( $(date +%s) + boot_timeout ))
while [ "$(date +%s)" -lt "$deadline" ]; do
  if ! kill -0 "$daemon_pid" 2>/dev/null; then
    echo 'daemon stdout:'; tail -20 "$stage/daemon.out.log"
    echo 'daemon stderr:'; tail -20 "$stage/daemon.err.log"
    fail "the daemon exited before answering /api/v1/health"
  fi
  code="$(curl -s -o /dev/null -w '%{http_code}' --max-time 5 "$base/api/v1/health" 2>/dev/null)"
  [ "$code" = "200" ] && break
  sleep 0.5
done
if [ "$code" != "200" ]; then
  echo 'daemon stdout:'; tail -20 "$stage/daemon.out.log"
  echo 'daemon stderr:'; tail -20 "$stage/daemon.err.log"
  fail "no 200 from $base/api/v1/health within ${boot_timeout}s (last: ${code:-<none>})"
fi
echo "GET /api/v1/health -> 200 $(curl -s --max-time 5 "$base/api/v1/health" | tr -d '\n')"

# 3. THE POINT OF THIS SCRIPT: the PANEL, not merely the API.
page_code="$(curl -s -o "$stage/index.html" -w '%{http_code}' --max-time 10 "$base/" 2>/dev/null)"
[ "$page_code" = "200" ] || fail "GET / -> ${page_code:-<none>}: the API answered but the panel was not served"
if ! grep -q '<title>ruagent</title>' "$stage/index.html"; then
  echo "first 200 chars: $(head -c 200 "$stage/index.html")"
  fail "GET / answered 200 but the body is not the panel (no <title>ruagent</title>)"
fi
echo "GET / -> 200 with the panel marker (<title>ruagent</title>), $(wc -c <"$stage/index.html") bytes"

# 4. WHICH EMBEDDER? The daemon tries the ONNX-backed FastEmbedder at boot and falls back to
#    the offline hash embedder (crates/daemon/src/lib.rs); /api/v1/stats reports the outcome,
#    so this is a live reading on a machine with no shipped onnxruntime library.
stats="$(curl -s --max-time 10 "$base/api/v1/stats" 2>/dev/null)"
# `| head -1` closed the pipe on grep, and under `set -o pipefail` a SIGPIPE from that side becomes the
# step's exit code -- the same class as the `tar | head` that failed the first real release packaging.
# `grep -m1` stops itself rather than being killed.
embedder="$(printf '%s' "$stats" | tr ',' '\n' | grep -m1 '"embedder"' | sed 's/.*: *"\([^"]*\)".*/\1/')"
echo "embedder in use: ${embedder:-<unreadable>}"

# 5. CLEAN STOP through the daemon's own channel, forced kill only as a reported fallback.
mkdir -p "$root/data"
printf 'smoke-archive: stop requested' >"$root/data/daemon.stop"
stopped=0
for _ in $(seq 1 40); do
  sleep 0.25
  if ! kill -0 "$daemon_pid" 2>/dev/null; then stopped=1; break; fi
done
if [ "$stopped" = "1" ]; then
  echo 'daemon stopped through data/daemon.stop (clean exit)'
else
  kill -9 "$daemon_pid" 2>/dev/null
  wait "$daemon_pid" 2>/dev/null
  echo 'daemon ignored the stop request for 10 s -> forced (the smoke does not fail for that, but a published build should be clean)'
fi
wait "$daemon_pid" 2>/dev/null

echo 'daemon log tail:'; tail -12 "$stage/daemon.out.log"
if [ -f "$root/data/daemon.exit" ]; then
  echo "exit trace: $(head -1 "$root/data/daemon.exit")"
fi
rm -rf "$stage"
echo "SMOKE OK: $archive runs from its own contents and serves the panel"
exit 0
