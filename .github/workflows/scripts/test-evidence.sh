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
# t105 (the first independent audit of this script) added two more shapes to the
# same family, and they are now checked in code:
#   * an rc that is not an exit code (or is empty) used to read as "not non-zero"
#     -- the exit-code half of the gate passed while the table looked green;
#   * CR and ANSI escapes in the log are properties of the FILE, not of the run:
#     they turned a healthy run red ("0 targets", or a named test "not run").
#     Counting therefore reads a `tr -d '\r'` + ANSI-stripped copy.
#
# usage: test-evidence.sh <label> <log> <exit-code> [required-test-name ...]
set -u

label="${1:?label}"; log="${2:?log}"; rc="${3-}"; [ -n "$rc" ] || rc=""
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

# AN UNVALIDATED rc IS NOT A READING (t105 F-A1, measured on this script). A
# non-numeric rc made `[ "$rc" -ne 0 ]` print "integer expression expected" and
# evaluate FALSE: the exit-code half of the gate silently PASSED while the log
# looked green (measured: `test-evidence.sh strrc healthy.log boom` -> exit 0 with
# a full green table). Same family as the t65 bug above, one argument over: the
# shape is validated before the value is used, and an empty rc (the caller's
# rcfile was not written) is NAMED instead of surfacing as a bare bash error.
rc_bad=0
if [ -z "$rc" ]; then
  echo "::error::$label: no exit code was given (empty rc argument) -- a run whose exit code we cannot read is not a green (t65/t105)"
  rc_bad=1
elif ! printf '%s' "$rc" | grep -qE '^[0-9]+$'; then
  echo "::error::$label: '$rc' is not an exit code (expected a non-negative integer) -- an unvalidated exit code is not a green (t105 F-A1)"
  rc_bad=1
fi

# READ A NORMALISED COPY (t105 F-A5/F-A6, both measured): a CRLF line ends in `\r`,
# so `^test <name> ... ok$` misses it while every `^test result:` counter still
# matches (false RED); an ANSI-coloured `^test result:` line is invisible to the
# anchored patterns, so a healthy run reports "0 targets" (false RED). Neither is a
# property of the run, so counting reads a stripped copy while the ORIGINAL log is
# what the failing-line echo quotes. If normalising is impossible the gate still
# runs on the raw log (a guard must never disappear because of its own plumbing).
norm="$log"
stripped=""
if command -v mktemp >/dev/null 2>&1; then
  stripped="$(mktemp 2>/dev/null || true)"
  if [ -n "$stripped" ]; then
    if tr -d '\r' <"$log" 2>/dev/null | sed -e 's/\x1b\[[0-9;]*m//g' >"$stripped" 2>/dev/null; then
      norm="$stripped"
      trap 'rm -f "$stripped"' EXIT
    else
      echo "  NOTE: could not normalise the log (CR/ANSI); counts read the raw log"
    fi
  fi
fi

total=$(grep -c '^test result:' "$norm" || true); total=${total:-0}
# Both shapes a target announces itself with: `     Running unittests ...` and
# `   Doc-tests <crate>`. Counting only `Running` made the two rows disagree
# (measured: 45 launched vs 56 reporting, the gap being exactly the doc-test
# targets) -- a reader would have to re-derive that, so the pattern covers both.
launched=$(grep -cE '^ *(Running|Doc-tests) ' "$norm" || true); launched=${launched:-0}
ok=$(grep -c '^test result: ok' "$norm" || true); ok=${ok:-0}
failed=$(grep -c '^test result: FAILED' "$norm" || true); failed=${failed:-0}
panics=$(grep -c 'panicked at' "$norm" || true); panics=${panics:-0}

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
  grep -E '^test result: FAILED|panicked at' "$norm" || true
fi

status=0
if [ "$total" -eq 0 ]; then
  echo "::error::$label reported 0 targets -- an empty run is not a green (t65)"
  status=1
fi
if [ "$rc_bad" -ne 0 ]; then
  # the ::error:: was already printed when the shape was checked
  status=1
elif [ "$rc" -ne 0 ] || [ "$failed" -gt 0 ]; then
  echo "::error::$label: exit code $rc with $failed failing target(s)"
  status=1
fi

for name in "$@"; do
  # `\r?` before the end anchor: a CRLF log is still a log whose named test ran
  # (t105 F-A5 -- the anchored pattern missed it and the gate went red for a
  # property of the file, not of the run).
  if ! grep -qE "^test ${name}( .*)? \.\.\. ok\r?$" "$norm"; then
    echo "::error::$label did not run ${name} -- a name that must be measured never reported ok"
    status=1
  fi
done

if [ "$status" -eq 0 ]; then
  echo "$label: $ok target(s) ok, 0 failed, $panics panic site(s), exit 0"
  [ "$#" -gt 0 ] && echo "and all $# required test name(s) reported ok"
fi
exit "$status"
