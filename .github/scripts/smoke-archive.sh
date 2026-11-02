#!/usr/bin/env bash
# Prove a release archive RUNS, on a machine that never saw this repo (Linux/glibc AND macOS).
#
# WHY (ruagent-issues-56-59 t43) -- one platform over from smoke-archive.ps1: the archive used to
# ship the panel as panel-dist/, a sibling of the binary, while the daemon resolves <exe
# dir>/panel/dist among four candidates (crates/daemon/src/api.rs). A user who unpacked the
# release got an API and a 404 on /. CI could not see it because the release job never ran what it
# had just packaged. macOS joined the matrix later (t44) and now runs THIS script, which is why
# the platform assumptions below are explicit rather than inherited from a Linux runner.
#
# WHAT IT CHECKS (the same contract as the PowerShell leg)
#   1. layout: <exe name> at the archive root, panel/dist/index.html beside it;
#   2. ./ruagent --version succeeds;
#   3. the archive's own SHA256SUMS matches the files it shipped;
#   4. a REAL daemon on a free loopback port with a private --root answers /api/v1/health 200,
#      AND / returns 200 with the PANEL body (not just an API);
#   5. /api/v1/stats names the embedder the daemon ended up with (the ONNX path's outcome -- see
#      the evidence block in .github/workflows/release.yml);
#   6. a clean stop through <root>/data/daemon.stop, forced kill only as a reported fallback, and
#      the daemon's own log tail printed either way.
#
# PLATFORM DIFFERENCES, ITEM BY ITEM (t44). The item first, then what this script does about it.
# "Not used" is a decision that has to survive an edit, so it is written down:
#   * sha256sum -- macOS has no GNU coreutils; the system tool is "shasum -a 256" (Perl). We
#     DETECT it (sha256sum, else shasum) and FAIL LOUDLY when neither exists: a skipped checksum
#     check is the exact failure mode SHA256SUMS exists to prevent, so it is never skipped.
#   * timeout -- NOT USED. The boot wait is a deadline computed with date +%s (portable) and the
#     curl calls carry --max-time (portable), so macOS's missing coreutils timeout cannot bite,
#     and a hung daemon cannot hang the step.
#   * stat -- NOT USED. Nothing here reads sizes or modes: the log tail comes from tail, the
#     panel body length from "wc -c <file", liveness from kill -0. BSD stat's -f/--format versus
#     GNU -c is therefore not a risk to this script.
#   * free port -- python3 (stdlib) first, then perl (present on macOS and Linux). Both bind port
#     0 and read the kernel's assignment. If NEITHER exists we FAIL and say so: a random guess
#     could collide with a listener and turn a real packaging bug into a flaky red. No
#     netstat/lsof/nc parsing, whose flags differ across the two platforms.
#   * sed -i -- NOT USED. Every sed here is a stream filter (sed -n, sed s///), which behaves
#     identically in BSD and GNU sed; in-place editing is where they differ, so it is avoided.
#   * seq, head -c, xargs -r, mktemp -d, tail -20, sleep 0.25, LC_ALL=C sort -- checked one by
#     one: seq is replaced by a POSIX counter loop (BSD seq is not universal), xargs -r is not
#     used at all (BSD xargs has no -r), and head -c/mktemp -d/tail -20/sleep 0.25/LC_ALL=C all
#     exist on macOS. The script is bash 3.2 compatible on purpose: no arrays, no lowercase
#     parameter expansion, no mapfile.
#
# usage: smoke-archive.sh <archive.tar.gz> [--exe ruagent] [--boot-timeout 120]
# exit: 0 the archive runs and serves the panel | 1 it does not | 2 usage
set -uo pipefail

archive=""
exe_name="ruagent"
boot_timeout=240
while [ $# -gt 0 ]; do
  case "$1" in
    --archive) archive="${2:-}"; shift 2 ;;
    --exe) exe_name="${2:-}"; shift 2 ;;
    --boot-timeout) boot_timeout="${2:-}"; shift 2 ;;
    -h|--help)
      # A here-doc rather than "sed -n a,bp $0": a line range silently drifts the moment someone
      # edits the header, and a usage message that prints the wrong lines is worse than none.
      cat <<'USAGE'
usage: smoke-archive.sh <archive.tar.gz> [--exe ruagent] [--boot-timeout 120]
  <archive.tar.gz>   the release archive this run produced (it is unpacked and executed)
  --exe              binary name inside the archive (default: ruagent; the Windows leg uses .exe)
  --boot-timeout     seconds to wait for /api/v1/health (default: 120)
exit: 0 the archive runs and serves the panel | 1 it does not | 2 usage
USAGE
      exit 0 ;;
    *) archive="$1"; shift ;;
  esac
done
if [ -z "$archive" ]; then
  echo "usage: smoke-archive.sh <archive.tar.gz> [--exe ruagent] [--boot-timeout 120]" >&2
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
  echo "  1. <exe dir>/panel/dist        = $stage/panel/dist  <-- ABSENT IN THIS ARCHIVE"
  echo "  2. <build tree>/panel/dist     = (not present in a packaged archive)"
  echo "  3. <cwd>/panel/dist            = $(pwd)/panel/dist"
  echo "  4. RUAGENT_PANEL_DIST env      = ${RUAGENT_PANEL_DIST:-<unset>}"
  found="$(cd "$stage" && find . -name index.html -print | tr '\n' ' ')"
  echo "index.html files anywhere in the archive: ${found:-<none>}"
  fail "the archive carries no panel/dist/index.html at its root, so EVERY candidate above misses and / would answer 404 (the t188 shape)"
fi

# 1. THE BINARY
if ! version="$("$stage/$exe_name" --version 2>&1)"; then
  fail "$exe_name --version failed: $version"
fi
echo "version: $version"

# 2. THE ARCHIVE'S OWN CHECKSUMS. The sha256 tool is DETECTED, never assumed, and its absence is a
#    failure: macOS has shasum, Linux has sha256sum, and a platform with neither must say so
#    instead of quietly verifying nothing.
sha_tool=""
if command -v sha256sum >/dev/null 2>&1; then
  sha_tool="sha256sum"
elif command -v shasum >/dev/null 2>&1; then
  sha_tool="shasum"
else
  fail "no sha256 tool on PATH: neither sha256sum (GNU coreutils) nor shasum (Perl, the macOS default) exists, so the archive SHA256SUMS cannot be verified. Refusing to pass a release whose checksums nobody checked."
fi
if [ ! -f "$stage/SHA256SUMS" ]; then
  fail "the archive carries no SHA256SUMS, so a downloader cannot verify it (the packaging step writes one; see release.yml)"
fi
if [ "$sha_tool" = "sha256sum" ]; then
  sums_out="$(cd "$stage" && sha256sum -c SHA256SUMS 2>&1)"
  sums_rc=$?
else
  sums_out="$(cd "$stage" && shasum -a 256 -c SHA256SUMS 2>&1)"
  sums_rc=$?
fi
if [ "$sums_rc" -ne 0 ]; then
  printf '%s\n' "$sums_out" | sed -n '1,20p'
  fail "the archive SHA256SUMS does not match the files it shipped (checked with $sha_tool)"
fi
echo "checksums: $sha_tool verified $(grep -c . "$stage/SHA256SUMS") entries"

# 3. A REAL DAEMON on a free loopback port, private root (never the runner's ~/.ruagent).
#    Port 0 plus the kernel's choice, via python3 then perl; neither present is a loud failure.
port=""
if command -v python3 >/dev/null 2>&1; then
  port="$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1]);s.close()' 2>/dev/null)"
elif command -v perl >/dev/null 2>&1; then
  port="$(perl -MSocket -e 'socket(S, PF_INET, SOCK_STREAM, 0) or exit 1; bind(S, sockaddr_in(0, inet_aton("127.0.0.1"))) or exit 1; print sockport(S);' 2>/dev/null)"
fi
case "$port" in
  ''|*[!0-9]*) fail "cannot obtain a free loopback port: neither python3 nor perl is on PATH. This script refuses to guess a port, because a collision would turn a packaging bug into a flaky red." ;;
esac
base="http://127.0.0.1:$port"
# Boot the daemon and wait up to $1 seconds for /api/v1/health. Returns 0 when it answered.
boot_and_wait() {
  "$stage/$exe_name" serve --addr "127.0.0.1:$port" --root "$root" >"$stage/daemon.out.log" 2>"$stage/daemon.err.log" &
  daemon_pid=$!
  code=""
  deadline=$(( $(date +%s) + $1 ))
  while [ "$(date +%s)" -lt "$deadline" ]; do
    if ! kill -0 "$daemon_pid" 2>/dev/null; then
      echo 'daemon stdout:'; tail -20 "$stage/daemon.out.log"
      echo 'daemon stderr:'; tail -20 "$stage/daemon.err.log"
      fail "the daemon exited before answering /api/v1/health"
    fi
    code="$(curl -s -o /dev/null -w '%{http_code}' --max-time 5 "$base/api/v1/health" 2>/dev/null)"
    [ "$code" = "200" ] && return 0
    sleep 0.5
  done
  return 1
}

echo "starting: $exe_name serve --addr 127.0.0.1:$port --root $root"
if ! boot_and_wait "$boot_timeout"; then
  # The boot path tries the ONNX-backed FastEmbedder, which DOWNLOADS its model on first use
  # (crates/daemon/src/lib.rs). A cold runner whose model host is slow or blocked can spend the
  # whole budget downloading -- a fact about the network, not about the archive this script is
  # here to vet. So: say the reading was lost, retry with the offline embedder, and fail only if
  # the daemon still cannot boot.
  echo "WARN: no 200 from $base/api/v1/health within ${boot_timeout}s (last: ${code:-<none>})."
  echo "WARN: retrying with RUAGENT_EMBEDDER=hash -- a model download is not part of what this archive promises."
  kill "$daemon_pid" 2>/dev/null || true
  wait "$daemon_pid" 2>/dev/null || true
  export RUAGENT_EMBEDDER=hash
  boot_and_wait 120 || {
    echo 'daemon stdout:'; tail -20 "$stage/daemon.out.log"
    echo 'daemon stderr:'; tail -20 "$stage/daemon.err.log"
    fail "no 200 from $base/api/v1/health within ${boot_timeout}s, nor within 120s with RUAGENT_EMBEDDER=hash: the archive does not boot"
  }
  echo "WARN: it booted with the offline hash embedder; the fastembed reading was lost to this attempt, not to the archive."
fi
echo "GET /api/v1/health -> 200 $(curl -s --max-time 5 "$base/api/v1/health" | tr -d '\n')"

# 4. THE POINT OF THIS SCRIPT: the PANEL, not merely the API.
page_code="$(curl -s -o "$stage/index.html" -w '%{http_code}' --max-time 10 "$base/" 2>/dev/null)"
[ "$page_code" = "200" ] || fail "GET / -> ${page_code:-<none>}: the API answered but the panel was not served"
if ! grep -q '<title>ruagent</title>' "$stage/index.html"; then
  echo "first 200 chars: $(head -c 200 "$stage/index.html")"
  fail "GET / answered 200 but the body is not the panel (no <title>ruagent</title>)"
fi
echo "GET / -> 200 with the panel marker (<title>ruagent</title>), $(wc -c <"$stage/index.html") bytes"

# 5. WHICH EMBEDDER? The daemon tries the ONNX-backed FastEmbedder at boot and falls back to the
#    offline hash embedder (crates/daemon/src/lib.rs); /api/v1/stats reports the outcome, so this
#    is a live reading on a machine with no shipped onnxruntime library.
stats="$(curl -s --max-time 10 "$base/api/v1/stats" 2>/dev/null)"
# A literal "| head -1" closed the pipe on grep here, and under set -o pipefail a SIGPIPE from that
# side becomes the step's exit code -- the same class as the "tar | head" that failed the first
# real release packaging. grep -m1 stops itself rather than being killed.
embedder="$(printf '%s' "$stats" | tr ',' '\n' | grep -m1 '"embedder"' | sed 's/.*: *"\([^"]*\)".*/\1/')"
echo "embedder in use: ${embedder:-<unreadable>}"

# 6. CLEAN STOP through the daemon's own channel, forced kill only as a reported fallback.
#    A POSIX counter loop, not seq: BSD/macOS seq is not universal.
mkdir -p "$root/data"
printf 'smoke-archive: stop requested' >"$root/data/daemon.stop"
stopped=0
i=0
while [ "$i" -lt 40 ]; do
  i=$(( i + 1 ))
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
