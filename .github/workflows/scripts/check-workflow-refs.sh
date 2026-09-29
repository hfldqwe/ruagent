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
#
# EXIT CODE (t105, the first independent audit of this script): 1 when any
# reference is not tracked, any file does not parse, a named file does not exist,
# OR **nothing was scanned at all** (`checked: 0`) -- an empty scan used to print
# the same success line as a fully tracked tree. The under-approximation above is
# unchanged (deliberately); what changed is that it may no longer be SILENT.
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
jobs_seen=0
empty_scan=0
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
      jobs:*) in_jobs=1; jobs_seen=$((jobs_seen + 1)); continue ;;
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
      # t105 F-A3 (measured false red): `working-directory: "ok"` is valid YAML and
      # its VALUE is `ok`; the quotes are YAML syntax, not part of the path. Tokens
      # are scanned quote-aware, so the working-directory value must be too.
      case "$workdir" in
        \"*\"|\'*\')
          workdir="${workdir#?}"
          workdir="${workdir%?}"
          ;;
      esac
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
# AN EMPTY SCAN IS NOT A GREEN (t105 F-A2, the first independent audit). Six
# measured inputs made this script exit 0 while it had scanned NOTHING -- a
# capitalised `Jobs:`, a `jobs :` with a space before the colon, a reference that
# is only ever written in quotes, `python3 tools/gen.py` (.py is not in
# `path_like`), a root-level `bash rootscript.sh` (no slash) and a file with no
# `jobs:` at all. "Everything is tracked" and "I did not look" printed the same
# line. The count now has a LOWER BOUND; the two reasons are named separately so
# the reader knows which one happened.
# ---------------------------------------------------------------------------
if [ "$refs" -eq 0 ]; then
  if [ "$jobs_seen" -eq 0 ]; then
    echo "::error::check-workflow-refs: no 'jobs:' key was found in ${#files[@]} workflow file(s), so nothing was scanned -- an empty scan is not a green (t105 F-A2). Is the key indented, capitalised ('Jobs:') or written 'jobs :'?"
  else
    echo "::error::check-workflow-refs: 'jobs:' was found but NO executable path reference was recognised in ${#files[@]} workflow file(s) -- an empty scan is not a green (t105 F-A2). Quoted, variable-carrying and extension-less references are deliberately not scanned; if this file truly executes nothing path-like, run the check over the DEFAULT set (.github/workflows/*.yml) instead of one hand-picked file."
  fi
  empty_scan=1
fi

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

# ---------------------------------------------------------------------------
# EXPRESSION SYNTAX (t106, from a real incident -- and the layer PyYAML cannot
# see). GitHub interpolates `${{ ... }}` over the WHOLE `run:` string BEFORE the
# runner starts, shell comments included. An illegal expression therefore rejects
# the ENTIRE workflow file: `gh run view` said "This run likely failed because of
# a workflow file issue", `run: 0s`, no job, and the run's name degraded to the
# file path instead of `E2E` -- while `python3 -c yaml.safe_load` called that same
# file perfectly valid. Measured shape: a literal `${{ ... }}` inside a `run: |`
# block (a shell comment).
#
# WHAT THIS CHECKS: the SHAPE of every expression -- complete `${{`/`}}` pairing,
# at least one operand, characters inside GitHub's expression alphabet, balanced
# quotes/parentheses/brackets. WHAT IT CANNOT CHECK: whether a LEGAL expression is
# semantically right (`success && 0 || 1` parses, and is always 1 -- that is a job
# structure problem, fixed by taking the status from the command itself, not by a
# parser). Under-approximation kept on purpose: the function set is not validated,
# so an unknown-but-well-shaped function is not flagged.
# ---------------------------------------------------------------------------
bad_expr=0

balanced_expr() {
  local s="$1" i=0 n=${#1} c paren=0 brack=0 inq=0
  while [ "$i" -lt "$n" ]; do
    c="${s:$i:1}"
    if [ "$inq" -eq 1 ]; then
      [ "$c" = "'" ] && inq=0
    else
      case "$c" in
        "'") inq=1 ;;
        '(') paren=$((paren + 1)) ;;
        ')') paren=$((paren - 1)) ;;
        '[') brack=$((brack + 1)) ;;
        ']') brack=$((brack - 1)) ;;
      esac
    fi
    [ "$paren" -lt 0 ] && return 1
    [ "$brack" -lt 0 ] && return 1
    i=$((i + 1))
  done
  [ "$inq" -eq 0 ] && [ "$paren" -eq 0 ] && [ "$brack" -eq 0 ]
}

validate_expr() { # file lineno body whole-line
  local f="$1" n="$2" body="$3" whole="$4" why="" code=""
  body="$(printf '%s' "$body" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')"
  # String literals are LEGAL carriers of anything GitHub's expression alphabet
  # does not have: measured false positive while writing this check --
  # `format('{0}-{1}', github.sha, env.FOO)` was flagged for its braces. The
  # alphabet rule therefore applies to the code AROUND the quoted strings.
  code="$(printf '%s' "$body" | sed -E "s/'[^']*'//g")"
  if [ -z "$body" ]; then
    why="empty expression"
  elif ! printf '%s' "$body" | grep -qE "[A-Za-z_][A-Za-z0-9_]*|'[^']*'|[0-9]"; then
    why="no operand (only operators/dots -- e.g. a bare '...')"
  elif printf '%s' "$code" | grep -qE "[^]A-Za-z0-9_ .(),[*|!<>=&/'-]"; then
    why="characters outside the expression alphabet"
  elif ! balanced_expr "$body"; then
    why="unbalanced quotes, parentheses or brackets"
  fi
  if [ -n "$why" ]; then
    echo "  $f:$n BAD EXPRESSION: $why:|$body| in |$whole|"
    bad_expr=$((bad_expr + 1))
  fi
}

for f in "${files[@]}"; do
  lineno=0
  while IFS= read -r line; do
    lineno=$((lineno + 1))
    rest="$line"
    while true; do
      case "$rest" in
        *'${{'*) ;;
        *) break ;;
      esac
      rest="${rest#*\${{}"
      case "$rest" in
        *'}}'*) validate_expr "$f" "$lineno" "${rest%%\}\}*}" "$line"; rest="${rest#*\}\}}" ;;
        *)
          echo "  $f:$lineno BAD EXPRESSION: unterminated \${{ ... }}:|$line|"
          bad_expr=$((bad_expr + 1))
          break
          ;;
      esac
    done
  done <"$f"
done

if [ "$bad_expr" -gt 0 ]; then
  echo "::error::$bad_expr workflow expression(s) do not parse. GitHub substitutes \${{ ... }} over the whole run: string (shell comments included) BEFORE the runner starts, so an illegal expression rejects the ENTIRE file: \"This run likely failed because of a workflow file issue\", run 0s, no job, and the run name degrades to the file path (t106). PyYAML cannot see this layer."
  bad=$((bad + bad_expr))
fi

# ---------------------------------------------------------------------------
# CONTEXT AVAILABILITY (t115) -- the THIRD "valid YAML, GitHub still rejects it"
# shape, and the second one this generation paid for in a pushed run.
#
# MEASURED INCIDENT: e2e.yml at 02dea44 put
#     PLAYWRIGHT_JSON_OUTPUT_FILE: ${{ runner.temp }}/t65/playwright.json
# in a JOB-LEVEL `env:`. `runner` is not available in `jobs.<job_id>.env`, so
# GitHub's static validation rejected the WHOLE FILE: run 36510848293 finished in
# 0s with 0 jobs, `name` degraded from `E2E` to `.github/workflows/e2e.yml`, and
# `gh run view` said "This run likely failed because of a workflow file issue".
# Both existing guards passed that file: PyYAML parsed it, and the shape check
# above found nothing wrong with the expression. `22255c4` changed the value to
# `${{ github.workspace }}` and the next run loaded again (36511070463, name E2E).
#
# RULE SOURCE (not a guess): the "Context availability" table in
# https://docs.github.com/en/actions/reference/workflows-and-actions/contexts#context-availability
# ("The listed contexts are only available for the given workflow key, and may not
# be used anywhere else."). Implemented rows -- deliberately ONLY these two, since
# each further row needs its own false-positive control and the scopes where
# `runner` IS legal (steps.*, container.env, services.*.env) must not be caught:
#   `env`               -> github, secrets, inputs, vars
#   `jobs.<job_id>.env` -> github, needs, strategy, matrix, vars, secrets, inputs
# Everything else is declared out of scope in the t115 report section; a checker
# that cries wolf is worse than no checker (t92/t105), so this one only fires
# where the scope is unambiguous and the doc row is explicit.
# ---------------------------------------------------------------------------
ALLOWED_ROOT_ENV=" github secrets inputs vars "
ALLOWED_JOB_ENV=" github needs strategy matrix vars secrets inputs "
KNOWN_CTX=" github env vars job jobs steps runner secrets strategy matrix needs inputs "
bad_ctx=0

ctx_allowed() { case "$1" in *" $2 "*) return 0 ;; *) return 1 ;; esac; }

check_ctx_line() { # allowed-set keypath file lineno line
  local allowed="$1" keypath="$2" f="$3" n="$4" line="$5"
  local rest="$line" body name
  while true; do
    case "$rest" in *'${{'*) ;; *) break ;; esac
    rest="${rest#*\${{}"
    case "$rest" in
      *'}}'*) body="${rest%%\}\}*}"; rest="${rest#*\}\}}" ;;
      *) break ;;
    esac
    # Only the FIRST identifier of a top-level token can be a context name: the
    # `[^A-Za-z0-9_.]` guard keeps `needs.build.runner` from reading as `runner`.
    for name in $(printf '%s' "$body" | grep -oE '(^|[^A-Za-z0-9_.])[a-z][a-z0-9_-]*\.' | sed -e 's/^[^a-z]*//' -e 's/\.$//' | sort -u); do
      ctx_allowed "$allowed" "$name" && continue
      case "$KNOWN_CTX" in *" $name "*) ;; *) continue ;; esac
      echo "  $f:$n CONTEXT NOT AVAILABLE under $keypath: '$name' is not allowed there (allowed:$allowed) -- GitHub validates context availability statically and rejects the WHOLE file (t115; docs: context availability table)"
      bad_ctx=$((bad_ctx + 1))
    done
  done
}

for f in "${files[@]}"; do
  lineno=0
  jobs_indent=-1
  jobid_indent=-1
  jobkey_indent=-1
  in_steps=0
  steps_indent=-1
  env_scope=""
  env_allowed=""
  env_indent=-1
  while IFS= read -r line; do
    lineno=$((lineno + 1))
    stripped="${line%%#*}"
    indent="${stripped%%[! ]*}"
    indent=${#indent}
    trimmed="$(printf '%s' "$stripped" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')"

    if [ -n "$env_scope" ]; then
      if [ -n "$trimmed" ] && [ "$indent" -gt "$env_indent" ]; then
        check_ctx_line "$env_allowed" "$env_scope" "$f" "$lineno" "$line"
        continue
      fi
      env_scope=""
      env_allowed=""
      env_indent=-1
    fi
    [ -n "$trimmed" ] || continue
    case "$trimmed" in '#'*) continue ;; esac

    # ---- structural state (exactly what the two rows need) ----
    case "$trimmed" in
      jobs:) jobs_indent="$indent"; jobid_indent=-1; jobkey_indent=-1; in_steps=0; steps_indent=-1; continue ;;
      -*) continue ;;                     # a list item is never a job id / job key
    esac
    key="${trimmed%%:*}"
    case "$key" in ''|*[!A-Za-z0-9_-]*) continue ;; esac
    case "$trimmed" in *:*) ;; *) continue ;; esac   # `key:` or `key: value`
    [ "$jobs_indent" -ge 0 ] || continue
    [ "$indent" -gt "$jobs_indent" ] || { jobid_indent=-1; jobkey_indent=-1; in_steps=0; continue; }

    if [ "$jobid_indent" -lt 0 ]; then
      jobid_indent="$indent"              # first key under `jobs:` is a job id
      continue
    fi
    if [ "$indent" -eq "$jobid_indent" ]; then
      jobid_indent="$indent"; jobkey_indent=-1; in_steps=0; steps_indent=-1
      continue
    fi
    if [ "$in_steps" -eq 1 ]; then
      # still inside the step list (its items are list items, handled above);
      # a plain key at the job-key indent ends the list and is a job key
      [ "$indent" -gt "$steps_indent" ] && continue
      in_steps=0
    fi
    if [ "$jobkey_indent" -lt 0 ]; then
      jobkey_indent="$indent"
    fi
    [ "$indent" -eq "$jobkey_indent" ] || continue    # deeper keys are container/service/step keys

    case "$key" in
      steps) in_steps=1; steps_indent="$indent"; continue ;;
      env) env_scope="jobs.<job_id>.env"; env_allowed="$ALLOWED_JOB_ENV"; env_indent="$indent"; continue ;;
    esac
  done <"$f"
done

# workflow-level `env:` (indent 0) -- same table, row `env`
for f in "${files[@]}"; do
  lineno=0
  env_scope=""
  env_indent=-1
  while IFS= read -r line; do
    lineno=$((lineno + 1))
    stripped="${line%%#*}"
    indent="${stripped%%[! ]*}"
    indent=${#indent}
    trimmed="$(printf '%s' "$stripped" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')"
    if [ -n "$env_scope" ]; then
      if [ -n "$trimmed" ] && [ "$indent" -gt "$env_indent" ]; then
        check_ctx_line "$ALLOWED_ROOT_ENV" "env (workflow level)" "$f" "$lineno" "$line"
        continue
      fi
      env_scope=""
      continue
    fi
    [ -n "$trimmed" ] || continue
    case "$trimmed" in '#'*) continue ;; esac
    # workflow-level only: `env:` at column 0. A deeper `env:` belongs to a job,
    # a container/service or a step -- and the first two of those DO allow
    # `runner`, so treating them as root env would be a false red.
    [ "$indent" -eq 0 ] || continue
    case "$trimmed" in env:) env_scope="env (workflow level)"; env_indent="$indent" ;; esac
  done <"$f"
done

if [ "$bad_ctx" -gt 0 ]; then
  echo "::error::$bad_ctx workflow expression(s) use a context that is NOT available under the key they sit under. GitHub validates context availability statically and rejects the ENTIRE workflow file: run 0s, no job, and the run name degrades to the file path (measured: run 36510848293 / commit 02dea44, t115). PyYAML parses that file and the shape check above passes it -- this layer is the only one that sees it. Source: the context availability table in the GitHub Actions contexts reference."
  bad=$((bad + bad_ctx))
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
  # t105 F-A9: the notice alone still exited 0, so a machine without PyYAML got a
  # guard that had silently halved itself. In CI PyYAML IS present (ubuntu-latest),
  # so its absence there is an environment defect, not a pass -- fail. Locally the
  # half-check stays skipped, but the wording may not read as "the files parse".
  if [ -n "${CI:-}" ]; then
    echo "::error::check-workflow-refs: PARSE CHECK SKIPPED ($reason) while CI is set -- the full-YAML half of this guard did NOT run, and in CI that is an environment defect, not a pass (t105 F-A9)."
    bad=$((bad + 1))
  else
    echo "  It does run in CI (ubuntu-latest has PyYAML). Locally: \`pip install pyyaml\`."
    echo "  The unquoted-':' scan above is not affected -- it needs no dependency."
    echo "  NOT a green for 'the workflows parse': this half is UNMEASURED here, and the files were NOT parsed."
  fi
fi

if [ "$bad" -gt 0 ]; then
  echo "::error::$bad workflow path reference(s) are not tracked, or a workflow does not parse. Commit the missing files in the SAME commit as the workflow that executes them (t65/F1: an untracked step script turns the pipeline red, not merely weaker)."
  exit 1
fi
if [ "$empty_scan" -eq 1 ]; then
  # the ::error:: naming the reason was printed above
  exit 1
fi
echo "every executed path a workflow references is tracked"
echo "PRE-SUBMIT COMMAND (t104): bash .github/workflows/scripts/check-workflow-refs.sh   # refs tracked + workflows parse"
