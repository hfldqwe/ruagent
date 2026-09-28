#!/usr/bin/env bash
# Counting discipline for a test run (t65, report §3).
#
# A run's evidence is the number of TARGETS that reported, the per-target
# `test result:` lines, the `panicked at` lines and the EXIT CODE -- never a sum
# of "(N) passed". The four false-green families this repo paid for are exactly
# the ones where a passed-count is true while nothing was measured:
#
#   * an empty run: `cargo test -- --ignored` with no ignored tests exits 0 and
#     prints "0 passed ... N filtered out" (measured in t36);
#   * a stale binary: the green came from a build that never happened;
#   * a cached green: `-D`-only changes do not invalidate fingerprints;
#   * a criterion weakened to nothing: one assertion that became non-falsifiable.
#
# So: 0 reporting targets is an ERROR here, and "which names actually ran" can be
# demanded explicitly (4th+ argument) -- that is how the `#[ignore]`d instruments
# prove they RAN rather than proving they were skipped.
#
# usage: test-evidence.sh <label> <log> <exit-code> [required-test-name ...]
set -u

label="${1:?label}"; log="${2:?log}"; rc="${3:?exit code}"
shift 3

# AN UNREADABLE RUN IS NOT A GREEN (t65, found by the captain reviewing this very
# script): with a missing log every count became an empty string, `[ "" -eq 0 ]`
# printed "integer expression expected" and evaluated FALSE, so the script
# reported "0 target(s) ok, 0 failed, ... exit 0". The gate that exists to stop
# silent greens was itself silently green.
if [ ! -r "$log" ]; then
  echo "::error::$label: evidence log '$log' is unreadable -- a run whose log we cannot read is not a green (t65)"
  exit 1
fi

total=$(grep -c '^test result:' "$log" || true); total=${total:-0}
# Both shapes a target announces itself with: `     Running unittests ...` and
# `   Doc-tests <crate>`. Counting only `Running` made the two rows disagree
# (measured: 45 launched vs 56 reporting, the gap being exactly the doc-test
# targets) -- a reader would have to re-derive that, so the pattern covers both.
launched=$(grep -cE '^ *(Running|Doc-tests) ' "$log" || true); launched=${launched:-0}
ok=$(grep -c '^test result: ok' "$log" || true); ok=${ok:-0}
failed=$(grep -c '^test result: FAILED' "$log" || true); failed=${failed:-0}
panics=$(grep -c 'panicked at' "$log" || true); panics=${panics:-0}

{
  echo "### $label"
  echo ""
  echo "| line class | count |"
  echo "| --- | --- |"
  echo "| \`Running \` (targets launched) | $launched |"
  echo "| \`test result:\` (targets reporting) | $total |"
  echo "| \`test result: ok\` | $ok |"
  echo "| \`test result: FAILED\` | $failed |"
  echo "| \`panicked at\` | $panics |"
  echo "| exit code | $rc |"
} | tee -a "${GITHUB_STEP_SUMMARY:-/dev/null}"

if [ "$failed" -gt 0 ]; then
  echo "failing targets and panic sites:"
  grep -E '^test result: FAILED|panicked at' "$log" || true
fi

status=0
if [ "$total" -eq 0 ]; then
  echo "::error::$label reported 0 targets -- an empty run is not a green (t65)"
  status=1
fi
if [ "$rc" -ne 0 ] || [ "$failed" -gt 0 ]; then
  echo "::error::$label: exit code $rc with $failed failing target(s)"
  status=1
fi

for name in "$@"; do
  if ! grep -qE "^test ${name}( .*)? \.\.\. ok$" "$log"; then
    echo "::error::$label did not run ${name} -- a name that must be measured never reported ok"
    status=1
  fi
done

if [ "$status" -eq 0 ]; then
  echo "$label: $ok target(s) ok, 0 failed, $panics panic site(s), exit 0"
  [ "$#" -gt 0 ] && echo "and all $# required test name(s) reported ok"
fi
exit "$status"
