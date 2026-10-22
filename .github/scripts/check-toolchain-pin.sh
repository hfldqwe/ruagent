#!/usr/bin/env bash
# ONE AUTHORITY FOR THE TOOLCHAIN PIN (t40 / audit F6).
#
# WHY: the pinned toolchain has several CARRIERS, and only one of them is the authority:
#   * rust-toolchain.toml -- what every cargo/rustc invocation in this repo RUNS
#     (rustup show active-toolchain prints: ... (overridden by '.../rust-toolchain.toml'));
#   * the `with: toolchain:` input of each dtolnay/rust-toolchain step -- what the RUNNER
#     INSTALLS. That action does not read the toml file (its README says the toolchain comes
#     from the action's @rev), so a step that OMITS the input installs the action's default
#     (stable) and FLOATS: a zero-diff commit can then go red on a new lint under
#     `-D warnings`, which is exactly what the pin exists to prevent.
#
# WHAT IT DOES: reads the authority, parses EVERY workflow's dtolnay/rust-toolchain steps and
# prints one line per step (file:line toolchain=...). It FAILS when
#   * a step of an ENFORCED workflow omits the input (it would float), or
#   * ANY step's value disagrees with the authority, or
#   * nothing was parsed at all -- an empty scan must not print the same success as a clean
#     one, which is the lesson of the t29 audit (a guard blind exactly where its parser was).
#
# ENFORCED vs REPORTED. ci.yml and audit.yml are ENFORCED. e2e.yml:69 and release.yml:132
# also omit the input; they are outside this change's scope, so they are REPORTED as WARN on
# every run (never silently dropped) and become enforced the moment they are pinned -- add
# them to ENFORCED below and nothing else needs to change.
#
# usage: check-toolchain-pin.sh [repo-root]
#   the optional root exists so the negative control can run against a COPY of the tree
#   instead of mutating the shared one.
# exit: 0 clean (warnings allowed) | 1 drift or an empty scan | 2 no authority found
set -uo pipefail

root="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
authority="$root/rust-toolchain.toml"
ENFORCED="ci.yml audit.yml"

want="$(awk -F'"' '/^[[:space:]]*channel[[:space:]]*=/ { print $2; exit }' "$authority")"
if [ -z "$want" ]; then
  echo "FAIL: no channel = ... line in $authority -- nothing to be authoritative about" >&2
  exit 2
fi
echo "authority: rust-toolchain.toml channel=$want"
echo "enforced: $ENFORCED"

shopt -s nullglob
wfs=("$root"/.github/workflows/*.yml)
if [ "${#wfs[@]}" -eq 0 ]; then
  echo "FAIL: no workflow files under $root/.github/workflows -- nothing was parsed, and that is not the same as clean" >&2
  exit 1
fi

awk -v want="$want" -v enforced="$ENFORCED" ' 
  function emit() {
    n = FILENAME
    sub(/.*\//, "", n)
    parsed++
    if (val == "") {
      if (index(" " enforced " ", " " n " ") > 0) {
        printf "FAIL %s:%d: no toolchain: input -- this step installs the action default (stable) and floats\n", n, step
        bad = 1
      } else {
        printf "WARN %s:%d: no toolchain: input -- not pinned yet (outside this change scope; reported, not failed)\n", n, step
      }
    } else if (val != want) {
      printf "FAIL %s:%d: toolchain=%s but the authority (rust-toolchain.toml) says %s\n", n, step, val, want
      bad = 1
    } else {
      printf "ok   %s:%d: toolchain=%s\n", n, step, val
    }
  }
  FNR == 1 && step { emit(); step = 0; inuse = 0 }
  /uses:[[:space:]]*dtolnay\/rust-toolchain/ {
    if (step) emit()
    step = FNR; val = ""; inuse = 1; next
  }
  inuse && /^[[:space:]]*-[[:space:]]*(uses|name|run):/ { emit(); inuse = 0; step = 0; next }
  inuse && /toolchain:/ {
    v = $0
    sub(/^[^:]*:[[:space:]]*/, "", v)
    gsub(/"/, "", v)
    val = v
  }
  END {
    if (step) emit()
    printf "dtolnay/rust-toolchain steps parsed: %d\n", parsed
    if (parsed == 0) { print "FAIL: nothing was parsed -- the parser is blind, and that is not the same as clean"; exit 1 }
    if (bad) { print "FAIL: a toolchain carrier disagrees with rust-toolchain.toml -- raise them together"; exit 1 }
    printf "every enforced carrier agrees with the authority (%s)\n", want
  }
' "${wfs[@]}"