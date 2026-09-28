#!/usr/bin/env bash
# Every path a workflow EXECUTES must be tracked, in the same commit (t65, F1).
#
# WHY: `ci.yml` references `.github/workflows/scripts/test-evidence.sh` from five
# steps. If that file is not committed together with the workflows, the pushed
# pipeline does not merely get weaker -- every one of those steps fails, which is
# a red main on the first push. `git status` answers this for a human who happens
# to look; this step answers it for the pipeline.
#
# WHAT COUNTS AS A REFERENCE (and what does not). The scan is deliberately an
# UNDER-approximation, because a gate that cries wolf gets switched off:
#   * only lines inside `jobs:` (header prose is out);
#   * comments are stripped; `echo`/`printf` lines are prose, never references;
#   * only UNQUOTED tokens count -- `foo "node e2e/run-e2e.mjs"` compares strings,
#     it does not execute that path;
#   * tokens containing `$`, `*`, `{`, `}` are runtime values/globs, not repo paths;
#   * `working-directory:` values are checked as directories (they must exist and
#     hold at least one tracked file), and they RESET at every `- name:`/`- uses:`
#     boundary, so a later step cannot silently inherit an earlier one's directory.
#
# RESOLUTION RULE: a relative reference is resolved against the `working-directory`
# of ITS OWN step, not the repository root -- otherwise `npx tsc -b` under
# `working-directory: panel` looks like a missing root path.
#
# MEASURED WHY (the two false positives a naive version produced here):
#   `scripts/build-panel.mjs` (:308) and `tools/design-audit.mjs` (:326) are really
#   `panel/scripts/...` / `panel/tools/...`; and
#   `docs/design/reviews/gen2-ci-hardening.md` (header comment) is prose.
#
# usage: check-workflow-refs.sh [workflow.yml ...]      (default: all of them)
set -uo pipefail

files=("$@")
if [ "${#files[@]}" -eq 0 ]; then
  # shellcheck disable=SC2206
  files=(.github/workflows/*.yml)
fi

path_like() {
  case "$1" in
    ./*) return 0 ;;
    */*) case "$1" in *.sh|*.mjs|*.js|*.ts|*.mts|*.yml|*.yaml|*.json|*.toml|*.sql|*.ps1) return 0 ;; esac ;;
  esac
  return 1
}

refs=0
bad=0
in_jobs=0
workdir=""
step="<top>"

report() { printf '  %-24s %-30s %-58s %s\n' "$1" "$2" "$3" "$4"; }

for f in "${files[@]}"; do
  if [ ! -f "$f" ]; then
    echo "::error::check-workflow-refs: no such workflow '$f'"
    exit 1
  fi
  echo "== $f"
  in_jobs=0
  workdir=""
  step="<top>"
  lineno=0
  while IFS= read -r line; do
    lineno=$((lineno + 1))
    case "$line" in
      jobs:*) in_jobs=1; continue ;;
    esac
    [ "$in_jobs" -eq 1 ] || continue
    stripped="${line%%#*}"
    trimmed="$(printf '%s' "$stripped" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')"
    [ -n "$trimmed" ] || continue

    # ---- step boundary: a new step owns its own working directory ----
    case "$trimmed" in
      -[[:space:]]name:*|-name:*)
        step="$(printf '%s' "${trimmed#-*name:}" | sed -e 's/^[[:space:]]*//')"
        workdir=""
        continue
        ;;
      -[[:space:]]uses:*|-uses:*|-*)
        case "$trimmed" in
          -*[a-z]*) workdir=""; step="<uses>" ;;
        esac
        continue
        ;;
    esac

    if [[ "$trimmed" =~ ^working-directory:[[:space:]]*(.+)$ ]]; then
      workdir="${BASH_REMATCH[1]}"
      refs=$((refs + 1))
      if [ -d "$workdir" ] && [ -n "$(git ls-files "$workdir" | head -1)" ]; then
        report "$f:$lineno" "$step" "$workdir/" "tracked (working-directory)"
      else
        report "$f:$lineno" "$step" "$workdir/" "MISSING/UNTRACKED"
        bad=$((bad + 1))
      fi
      continue
    fi

    # ---- prose lines are never references ----
    case "$trimmed" in
      echo[[:space:]]*|echo|printf[[:space:]]*|printf|'}'*|'{'*|'|'*|'>'*) continue ;;
    esac

    # ---- quote-aware token scan: unquoted path-like tokens only ----
    tok=""
    quoted=0
    scan() {
      local t="$1" q="$2"
      [ -n "$t" ] || return 0
      case "$t" in
        *'$'*|*'*'*|*'{'*|*'}'*|*=*) return 0 ;;
      esac
      path_like "$t" || return 0
      local resolved="$t"
      case "$t" in
        /*|~*) return 0 ;;
        ./*) resolved="${t#./}" ;;
        *) [ -n "$workdir" ] && resolved="$workdir/$t" ;;
      esac
      refs=$((refs + 1))
      if git ls-files --error-unmatch "$resolved" >/dev/null 2>&1; then
        report "$f:$lineno" "$step" "$resolved" "tracked"
      else
        if [ -e "$resolved" ]; then
          report "$f:$lineno" "$step" "$resolved" "UNTRACKED (commit it with this workflow)"
        else
          report "$f:$lineno" "$step" "$resolved" "MISSING"
        fi
        bad=$((bad + 1))
      fi
    }

    i=0
    n=${#trimmed}
    while [ "$i" -lt "$n" ]; do
      c="${trimmed:$i:1}"
      case "$c" in
        '"') if [ "$quoted" -eq 2 ]; then quoted=0; else quoted=2; fi ;;
        "'") if [ "$quoted" -eq 1 ]; then quoted=0; else quoted=1; fi ;;
        ' '|$'\t'|';'|'('|')'|','|'=')
          [ "$quoted" -eq 0 ] && scan "$tok" 0
          tok=""
          ;;
        *)
          if [ "$quoted" -eq 0 ]; then tok="$tok$c"; else tok=""; fi
          ;;
      esac
      i=$((i + 1))
    done
    [ "$quoted" -eq 0 ] && scan "$tok" 0
  done <"$f"
done

echo ""
echo "workflow path references checked: $refs, not tracked/missing: $bad"

# ---------------------------------------------------------------------------
# PARSEABILITY (t66, found the hard way). The step this script backs was itself
# named `Guard: workflow-referenced paths must be tracked` -- an unquoted `: `
# inside a `- name:` value is not valid YAML, so GitHub rejects the WHOLE file
# before running a single step: a guard against "CI points at something that is
# not there" that made the pipeline unparseable. Dependency-free check: any
# `name:`/`uses:` value containing an unquoted `: ` is invalid. If PyYAML is
# present the files are fully parsed as well.
# ---------------------------------------------------------------------------
unquoted=0
for f in "${files[@]}"; do
  lineno=0
  while IFS= read -r line; do
    lineno=$((lineno + 1))
    value=""
    case "$line" in
      *'- name: '*) value="${line#*- name: }" ;;
      *'- uses: '*) value="${line#*- uses: }" ;;
      *) continue ;;
    esac
    case "$value" in
      \"*|\'*) continue ;;                     # already quoted: fine
    esac
    case "$value" in
      *': '*)
        echo "  $f:$lineno UNQUOTED ': ' in a plain scalar -> invalid YAML:|$line|"
        unquoted=$((unquoted + 1))
        ;;
    esac
  done <"$f"
done
if [ "$unquoted" -gt 0 ]; then
  echo "::error::$unquoted workflow line(s) put an unquoted ': ' inside a name/uses value. GitHub rejects the whole file at parse time and the job never starts. Quote the value (t66)."
  bad=$((bad + unquoted))
fi

if command -v python3 >/dev/null 2>&1 && python3 -c 'import yaml' >/dev/null 2>&1; then
  for f in "${files[@]}"; do
    if python3 -c "import sys,yaml; yaml.safe_load(open(sys.argv[1], encoding='utf-8'))" "$f" 2>/dev/null; then
      echo "  $f: parses as YAML (PyYAML)"
    else
      echo "::error::$f does not parse as YAML (PyYAML)"
      bad=$((bad + 1))
    fi
  done
else
  # t104: an unstated skip reads as a pass. Say which half of the check did not
  # run, and why; the unquoted-`:` scan above always ran (no dependency).
  if command -v python3 >/dev/null 2>&1; then
    reason="python3 is present but has no PyYAML module"
  else
    reason="no python3 on PATH"
  fi
  echo "  PARSE CHECK SKIPPED ($reason): the full-YAML half of this guard did NOT run."
  echo "  It does run in CI (ubuntu-latest has PyYAML). Locally: \`pip install pyyaml\`."
  echo "  The unquoted-':' scan above is not affected -- it needs no dependency."
fi

if [ "$bad" -gt 0 ]; then
  echo "::error::$bad workflow path reference(s) are not tracked, or a workflow does not parse. Commit the missing files in the SAME commit as the workflow that executes them (t65/F1: an untracked step script turns the pipeline red, not merely weaker)."
  exit 1
fi
echo "every executed path a workflow references is tracked"
echo "PRE-SUBMIT COMMAND (t104): bash .github/workflows/scripts/check-workflow-refs.sh   # refs tracked + workflows parse"
