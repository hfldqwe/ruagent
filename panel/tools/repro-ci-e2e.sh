#!/usr/bin/env bash
# Reproduce the CI E2E conditions locally (t107).
#
# CI fails the last step of .github/workflows/e2e.yml -- "Panel E2E
# (Playwright)" -- while the same suite is green on this machine. The only
# difference is the ENVIRONMENT: CI boots a brand-new daemon with an empty data
# root and registers the scripted mock agents; here the daemon has been running
# for days over real data. This script rebuilds the CI environment and nothing
# else:
#
#   * its own data root  (RUAGENT_HOME + --root)  -- never ~/.ruagent
#   * its own port       (default 8799)           -- never the user's 8787
#   * its own agents.toml, copied from the workflow step verbatim
#   * E2E_BASE_URL, the escape hatch playwright.config.ts documents
#
# Usage: bash tools/repro-ci-e2e.sh [--keep]
#   --keep   leave the data root in place for inspection (default: remove it)

set -u
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
PORT="${T107_PORT:-8799}"
ROOT="${T107_ROOT:-$(mktemp -d 2>/dev/null || echo "/tmp/t107-$$")}"
BIN="${T107_MOCK_BIN:-D:/rust_cache/debug/ruagent-mock-agent.exe}"
KEEP="${1:-}"

echo "[t107] repo   = $REPO"
echo "[t107] root   = $ROOT"
echo "[t107] port   = $PORT"
echo "[t107] mock   = $BIN"

mkdir -p "$ROOT/config"
# --- the "Register e2e mock agents" step, copied from the workflow ----------
printf '[{"marker":"FANOUT MARKER","reply":"answer from alpha"},{"marker":"CHAT MARKER","reply":"answer from alpha chat"}]' > "$ROOT/config/e2e-alpha.json"
printf '[{"marker":"FANOUT MARKER","reply":"answer from beta"}]' > "$ROOT/config/e2e-beta.json"
cat > "$ROOT/config/agents.toml" <<EOF
[agent.alpha]
harness = "mock"
command = "$BIN --behavior scripted --replies $ROOT/config/e2e-alpha.json"
description = "e2e fan-out member"

[agent.beta]
harness = "mock"
command = "$BIN --behavior scripted --replies $ROOT/config/e2e-beta.json"
description = "e2e fan-out member"

[agent.judge]
harness = "mock"
command = "$BIN --behavior judge"
description = "e2e fan-out judge"
EOF

# --- boot the daemon on its own port over its own root ----------------------
export RUAGENT_HOME="$ROOT"
export RUAGENT_EMBEDDER=hash
cd "$REPO"
"D:/rust_cache/debug/ruagent.exe" serve --addr "127.0.0.1:$PORT" --root "$ROOT" > "$ROOT/daemon.log" 2>&1 &
DAEMON_PID=$!
echo "[t107] daemon pid = $DAEMON_PID"

cleanup() {
  echo "[t107] stopping daemon $DAEMON_PID"
  kill "$DAEMON_PID" 2>/dev/null
  sleep 2
  kill -9 "$DAEMON_PID" 2>/dev/null
  if [ "$KEEP" != "--keep" ]; then
    rm -rf "$ROOT"
    echo "[t107] removed data root $ROOT"
  else
    echo "[t107] kept data root $ROOT"
  fi
}
trap cleanup EXIT

for i in $(seq 1 60); do
  if curl -sf "http://127.0.0.1:$PORT/api/v1/health" > /dev/null; then
    echo "[t107] daemon healthy after ${i}s"
    break
  fi
  sleep 1
done
curl -sf "http://127.0.0.1:$PORT/api/v1/health" || { echo "[t107] DAEMON NEVER CAME UP"; tail -20 "$ROOT/daemon.log"; exit 1; }

# The empty-database readings that the hypotheses turn on.
echo "[t107] chats at boot:    $(curl -s "http://127.0.0.1:$PORT/api/v1/chats" | head -c 120)"
echo "[t107] sessions at boot: $(curl -s "http://127.0.0.1:$PORT/api/v1/sessions" | head -c 120)"

# --- the suite, pointed at this instance ------------------------------------
cd "$REPO/panel"
E2E_BASE_URL="http://127.0.0.1:$PORT" npm run test:e2e 2>&1 | tee /tmp/t107-full.log | tail -60
echo "[t107] e2e exit = ${PIPESTATUS[0]}"
