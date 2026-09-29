# scripts/cargo-team.ps1
#
# WHY THIS EXISTS (2026-09-27, measured):
#   Several team members share one checkout, and every member was given its own
#   CARGO_TARGET_DIR. So each member compiled the whole workspace -- lancedb
#   included -- from scratch, in parallel. Measured: 5 concurrent `cargo`
#   invocations, two `rustc` processes compiling `lancedb` at the same time in
#   two different target dirs, 32.5 GB of RAM with only 3.7 GB free, all 16
#   logical CPUs busy. The caches were also split (ruagent-verify-ra 5.5 GB,
#   ruagent-verify-rd 6.5 GB, ~300 MB per other task).
#
# WHAT IT DOES:
#   1. SINGLE FLIGHT -- an exclusive lock file means at most ONE team build runs
#      at a time. A second caller waits; it does not fail and it does not start
#      a second compiler.
#   2. ONE CACHE -- every caller uses the same CARGO_TARGET_DIR, so the same
#      dependency is compiled once and then reused incrementally instead of N
#      times.
#   3. HEADROOM -- bounded job count, a CPU affinity mask that leaves the
#      highest-numbered cores free for the person using the machine, and
#      below-normal priority.
#
# USAGE (from the repo root). Cargo arguments are forwarded verbatim:
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-memory
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test -p ruagent-memory -Nocapture
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 clippy -p ruagent-graph --all-targets -DenyWarnings
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 check -p ruagent-daemon --all-targets
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/cargo-team.ps1 test --release -p ruagent-store
#
#   Two switches expand to the tails that are easy to get wrong:
#     -Nocapture    appends `-- --nocapture`   (for test/bench only)
#     -DenyWarnings appends `-- -D warnings`   (for clippy only -- see below)
#
#   MEASURED 2026-09-29 (t116/t117): a literal `--` IS forwarded verbatim from the
#   documented invocation -- `... cargo-team.ps1 test -p ruagent-core -- --list`
#   reaches cargo as `cargo test -p ruagent-core -- --list`. The earlier claim here
#   that PowerShell eats the bare `--` was NOT reproduced from this caller, so these
#   two switches are convenience and self-documentation, not a workaround for a
#   demonstrated loss. (A caller that goes through another shell layer may still
#   lose it; this script only knows what reaches it.)
#
#   -DenyWarnings IS COMMAND-AWARE (t117, fixing t116 W-4): `-- -D warnings` is only
#   meaningful for `clippy` (and `cargo rustc`). With `check` cargo rejects the
#   arguments (exit 1); with `test` libtest dies with "Unrecognized option: 'D'"
#   while NO test runs -- a red that reads like a test failure (measured: exit 101
#   with zero `test result:` lines). This script now REFUSES that pairing (exit 2 and
#   the reason) instead of producing the misleading red. Every call site in this repo
#   already uses -DenyWarnings with clippy.
#
#   Add -DryRun to print the exact cargo command line without compiling and
#   without taking the build lock. Use it to check a shape first.
#
#   FORCING A REAL LINT/TEST RE-READING (2026-09-28, measured twice by two
#   members): a `clippy ... -- -D warnings` that hits cargo's cache reports
#   success in ~0.8s without checking anything. Worse, `clean -p <crate>`
#   followed by a separate build is a RACE: a peer's ordinary clippy can slip
#   in between under the same lock and re-warm the cache, so the gate is a
#   false green again (measured: 1.44s). -CleanFirst runs the clean and the
#   gate inside ONE lock window, which is the only shape that cannot be
#   interleaved:
#     ... -File scripts/cargo-team.ps1 clippy -p ruagent-daemon --all-targets -DenyWarnings -CleanFirst ruagent-daemon
#   A green reading is still only valid when the output shows this run's own
#   `Checking <crate>` line -- cargo prints that per PACKAGE, so it proves the
#   package was re-checked and never enumerates targets.
#
#   t117 (fixing t116 W-1, a gate-shaped false green): `-CleanFirst <name>` used to
#   print "forced re-check" even when <name> was NOT a package the gate builds, and
#   the mismatched run then reported a CACHED green with the SAME words as the
#   matched one (measured: matched -> "Checking ruagent-core" + 1.1s; mismatched ->
#   no Checking line + 0.3s; both exit 0). The wording is now honest:
#     * a cleaned name that IS a gated package: "(forced re-check: '<name>' IS a
#       package this gate builds)"
#     * otherwise a NOT VERIFIED banner naming what the gate builds, what this run
#       cleaned, and that the green below is a genuine re-read only if
#       'Checking <gated package>' appears in THIS run's output.
#   Cleaning a DEPENDENCY to force the gated package to rebuild stays allowed (the
#   banner says so): it was the CLAIM that was wrong, not the technique.
#
#   THIS SCRIPT REFUSES (exit 2, with the reason on stdout) rather than guessing:
#     * a nameless -CleanFirst with no `-p <spec>`                (t88/R-5)
#     * -TargetDir with a missing or empty value                   (t116 W-2: it used
#       to fall back to the SHARED target silently, so a two-tree comparison would
#       compile the second tree into the first tree's cache -- closure 7.1)
#     * -Jobs / -Cpus with a missing, non-numeric or < 1 value      (t116 W-3/W-8)
#     * -DenyWarnings with a command other than clippy/rustc        (t116 W-4)
#   Switch names are matched case-insensitively (PowerShell `-eq`), so `-dryrun`
#   works too (t116 W-7); no cargo single-dash flag collides with them today.
#
# COMPARING TWO TREES: a before/after pair (git worktree, git archive export)
# must NOT share a target dir -- cargo would hand the second tree the first
# tree's rlibs (closure plan 7.1). Give the comparison tree a dir of its own:
#   ... -File scripts/cargo-team.ps1 test -p X -TargetDir "$env:TEMP\ruagent-cmp-a"
#
# -NoLock is only for such a deliberate second target dir that you want to run
# in parallel (rare). It does NOT check that your dir differs from the team's
# (t116 W-9): with the shared dir, two -NoLock builds are serialized only by
# cargo's own "Blocking waiting for file lock on build directory", and the real
# cost is CPU/memory headroom. This script prints a NOTE in that case.
#
# WHY THERE IS NO `param()` BLOCK (2026-09-28, found the hard way):
#   With `powershell -File`, ANY declared parameter takes the first bare word:
#   `... cargo-team.ps1 test -p ruagent-memory` bound "test" to the first
#   declared parameter and failed before cargo ever ran, and `$args` could not
#   rescue it because binding happens first. So this script declares NOTHING and
#   parses its own switches out of $args; everything else is handed to cargo
#   untouched -- including `-p`, `-j`, `-v`, `-q`, `-F`, `-r` and a literal `--`.
#   Adding a `param()` block here would reintroduce that bug.

$ErrorActionPreference = 'Stop'

$jobs = 4
$cpus = 12
$targetDir = $null
$noLock = $false
$noCapture = $false
$denyWarnings = $false
$dryRun = $false
$cargoArgs = New-Object 'System.Collections.Generic.List[string]'
$cleanFirst = New-Object 'System.Collections.Generic.List[string]'

$raw = @($args)

function Refuse([string]$msg) {
    Write-Host "[cargo-team] $msg"
    exit 2
}

function PosInt([string]$v, [string]$name) {
    $n = 0
    if (-not [int]::TryParse($v, [ref]$n) -or $n -lt 1) {
        Refuse "$name needs a positive integer (got '$v'). Refusing: 0 or a non-number used to reach cargo (or silently clamp), which is not a build parameter (t116 W-3/W-8)."
    }
    return $n
}

for ($i = 0; $i -lt $raw.Count; $i++) {
    $a = [string]$raw[$i]
    if ($a -eq '-CleanFirst') {
        # A bare -CleanFirst (no package name) used to consume the NEXT flag as the package
        # name and then build `cargo clean -p  (forced re-check)` with an EMPTY SPEC: cargo
        # errored, the wrapper printed "not running the gate", and clippy never ran -- a gate
        # that silently did not run, disguised as "this step failed" (t88/t72, R-5).
        # Now: take the name if it is really there, otherwise resolve it later from `-p <spec>`.
        $nxt = if ($i + 1 -lt $raw.Count) { [string]$raw[$i + 1] } else { '' }
        if ($nxt -and -not $nxt.StartsWith('-')) { $i++; $cleanFirst.Add($nxt) }
        else { $cleanFirst.Add('') }
    }
    elseif ($a -eq '-Jobs') {
        $nxt = if ($i + 1 -lt $raw.Count) { [string]$raw[$i + 1] } else { '' }
        $i++; $jobs = PosInt $nxt '-Jobs'
    }
    elseif ($a -eq '-Cpus') {
        $nxt = if ($i + 1 -lt $raw.Count) { [string]$raw[$i + 1] } else { '' }
        $i++; $cpus = PosInt $nxt '-Cpus'
    }
    elseif ($a -eq '-TargetDir') {
        $nxt = if ($i + 1 -lt $raw.Count) { [string]$raw[$i + 1] } else { '' }
        if (-not $nxt -or $nxt.StartsWith('-')) {
            Refuse "-TargetDir needs a path (got '$nxt'). Refusing: a missing value used to fall back to the SHARED team target silently, so a two-tree before/after comparison would compile the second tree into the first tree's cache and its readings would not be the second tree's (t116 W-2)."
        }
        $i++; $targetDir = $nxt
    }
    elseif ($a -eq '-NoLock') { $noLock = $true }
    elseif ($a -eq '-Nocapture') { $noCapture = $true }
    elseif ($a -eq '-DenyWarnings') { $denyWarnings = $true }
    elseif ($a -eq '-DryRun') { $dryRun = $true }
    else { $cargoArgs.Add($a) }
}

# Resolve any nameless -CleanFirst from the `-p <spec>` arguments, and REFUSE loudly if
# there is nothing to resolve it to. A gate must either really run or clearly refuse;
# never "run" with an empty SPEC.
if ($cleanFirst.Count -gt 0) {
    $pSpecs = New-Object 'System.Collections.Generic.List[string]'
    for ($i = 0; $i -lt $cargoArgs.Count; $i++) {
        if (($cargoArgs[$i] -eq '-p' -or $cargoArgs[$i] -eq '--package') -and $i + 1 -lt $cargoArgs.Count) {
            $pSpecs.Add([string]$cargoArgs[$i + 1])
        }
    }
    $unresolved = @($cleanFirst | Where-Object { -not $_ }).Count
    if ($unresolved -gt 0 -and $pSpecs.Count -eq 0) {
        Write-Host "[cargo-team] -CleanFirst needs a package spec (e.g. -CleanFirst ruagent-core, or a -p <spec> in the cargo args)."
        Write-Host "[cargo-team] Refusing: a nameless -CleanFirst would build an empty 'cargo clean -p' SPEC and the gate would NOT run."
        exit 2
    }
    $resolved = New-Object 'System.Collections.Generic.List[string]'
    foreach ($c in $cleanFirst) {
        if ($c) { $resolved.Add($c) } else { foreach ($s in $pSpecs) { $resolved.Add($s) } }
    }
    $cleanFirst = $resolved
}

if ($cargoArgs.Count -eq 0) {
    Write-Host "usage: cargo-team.ps1 [-Jobs N] [-Cpus N] [-TargetDir PATH] [-NoLock] [-Nocapture] [-DenyWarnings] [-DryRun] <cargo args...>"
    exit 2
}

# W-4 (t117): the `-- -D warnings` tail only exists for clippy/rustc. Refusing here is
# the honest choice -- "supporting" it for check/test would need RUSTFLAGS, which
# changes the fingerprint of every crate and would rebuild/re-cache the whole shared
# target dir (a silent, large side effect on the team cache).
$sub = [string]$cargoArgs[0]
if ($denyWarnings -and ($sub -ne 'clippy' -and $sub -ne 'rustc')) {
    Write-Host "[cargo-team] -DenyWarnings does not apply to 'cargo $sub' -- it appends '-- -D warnings', which only clippy/rustc accept. Measured with the old shape (t116 W-4): 'check' -> cargo 'error: unexpected argument ''-D'' found' (exit 1); 'test' -> libtest 'Unrecognized option: ''D''' + cargo 'error: test failed' (exit 101) with ZERO 'test result:' lines, i.e. a red that reads like a test failure while no test ran."
    Write-Host "[cargo-team] Refusing. Use -DenyWarnings with clippy (every call site in this repo does), or set RUSTFLAGS yourself if you really need rustc-level -D warnings."
    exit 2
}

# the packages this gate actually builds (-p/--package); used by the W-1 honesty check
$gated = New-Object 'System.Collections.Generic.List[string]'
for ($i = 0; $i -lt $cargoArgs.Count; $i++) {
    if (($cargoArgs[$i] -eq '-p' -or $cargoArgs[$i] -eq '--package') -and $i + 1 -lt $cargoArgs.Count) {
        $gated.Add([string]$cargoArgs[$i + 1])
    }
}

if ($noCapture) { $cargoArgs.Add('--'); $cargoArgs.Add('--nocapture') }
if ($denyWarnings) { $cargoArgs.Add('--'); $cargoArgs.Add('-D'); $cargoArgs.Add('warnings') }

# 1. one shared cache for the main tree; a caller-supplied dir for tree comparisons
$shared = if ($targetDir) { $targetDir } else { Join-Path $env:TEMP 'ruagent-team-target' }
$env:CARGO_TARGET_DIR = $shared

# 2. bounded parallelism
$env:CARGO_BUILD_JOBS = "$jobs"

$logical = [Environment]::ProcessorCount
$useCpus = $cpus
if ($useCpus -ge $logical) { $useCpus = $logical - 1 }
if ($useCpus -lt 1) { $useCpus = 1 }

if ($dryRun) {
    Write-Host "[cargo-team] DRY RUN (nothing compiled, no lock taken)"
    Write-Host "[cargo-team] target=$shared jobs=$jobs cpus=0-$($useCpus - 1)/$logical priority=BelowNormal"
    if ($cleanFirst.Count -gt 0) {
        foreach ($pkg in $cleanFirst) {
            if ($gated.Count -gt 0 -and ($gated -contains $pkg)) {
                Write-Host "[cargo-team] (under the same lock) clean -p $pkg (forced re-check: '$pkg' IS a package this gate builds)"
            } else {
                Write-Host "[cargo-team] (under the same lock) clean -p $pkg (NOT a package this gate builds -- the run will print NOT VERIFIED; t116 W-1)"
            }
        }
    }
    Write-Host "[cargo-team] cargo $($cargoArgs -join ' ')"
    exit 0
}

New-Item -ItemType Directory -Force -Path $shared | Out-Null

$lockPath = Join-Path $env:TEMP 'ruagent-team-build.lock'
$lock = $null
if (-not $noLock) {
    $deadline = (Get-Date).AddMinutes(90)
    while ($null -eq $lock) {
        try {
            $lock = [System.IO.File]::Open($lockPath, 'OpenOrCreate', 'ReadWrite', 'None')
        } catch [System.IO.IOException] {
            if ((Get-Date) -gt $deadline) {
                Write-Host "[cargo-team] gave up waiting for the team build lock after 90 min ($lockPath)"
                exit 75
            }
            Write-Host "[cargo-team] another team build is running (one compile at a time); waiting 10s ..."
            Start-Sleep -Seconds 10
        }
    }
}

try {
    # 3. headroom for the person using the machine: leave the top cores free,
    #    run at below-normal priority. Child processes inherit both.
    $mask = [int64]([math]::Pow(2, $useCpus) - 1)
    try {
        (Get-Process -Id $PID).ProcessorAffinity = [IntPtr]$mask
        (Get-Process -Id $PID).PriorityClass = 'BelowNormal'
    } catch {
        Write-Host "[cargo-team] could not set affinity/priority ($($_.Exception.Message)); continuing"
    }

    Write-Host "[cargo-team] target=$shared jobs=$jobs cpus=0-$($useCpus - 1)/$logical priority=BelowNormal"
    if ($noLock -and -not $targetDir) {
        Write-Host "[cargo-team] NOTE: -NoLock with the SHARED target dir. Two such builds are serialized only by cargo's own 'Blocking waiting for file lock on build directory' and otherwise compete for CPU/memory; -NoLock is meant for a deliberate second -TargetDir (t116 W-9)."
    }

    # -CleanFirst: force a real re-reading while still holding the lock, so no peer
    # build can slip in and re-warm the cache against the gate.
    # t117/W-1: say what is cleaned AND whether it is a package this gate builds. The
    # old wording claimed "forced re-check" for any name, so a run that cleaned an
    # unrelated package printed the same words as one that cleaned the gated package,
    # while its green came from cargo's cache.
    $uncovered = New-Object 'System.Collections.Generic.List[string]'
    foreach ($pkg in $cleanFirst) {
        if ($gated.Count -gt 0 -and ($gated -contains $pkg)) {
            Write-Host "[cargo-team] clean -p $pkg (forced re-check: '$pkg' IS a package this gate builds)"
        } elseif ($gated.Count -gt 0) {
            Write-Host "[cargo-team] clean -p $pkg (NOT a package this gate builds: gate builds $($gated -join ', '))"
            $uncovered.Add($pkg)
        } else {
            Write-Host "[cargo-team] clean -p $pkg (the gate has no -p spec: it builds the workspace default set, so '$pkg' is not confirmed as a gated package)"
            $uncovered.Add($pkg)
        }
        & cargo clean -p $pkg
        if ($LASTEXITCODE -ne 0) {
            Write-Host "[cargo-team] cargo clean -p $pkg exited $LASTEXITCODE; not running the gate"
            exit $LASTEXITCODE
        }
    }
    if ($uncovered.Count -gt 0) {
        $gateDesc = if ($gated.Count -gt 0) { "-p " + ($gated -join ' -p ') } else { "the workspace default set (no -p spec)" }
        Write-Host "[cargo-team] ============ NOT VERIFIED BY THE CLEAN ============"
        Write-Host "[cargo-team] This run cleaned package(s) other than the ones it gates, so whether the gate was re-read is NOT established by the clean alone:"
        Write-Host "[cargo-team]   gate builds        : $gateDesc"
        Write-Host "[cargo-team]   this run cleaned   : $($cleanFirst -join ', ')"
        Write-Host "[cargo-team]   cleaned, not gated: $($uncovered -join ', ')"
        Write-Host "[cargo-team] A green below is a genuine re-read ONLY if the clean invalidates a gated package: cleaning a DEPENDENCY of it does, cleaning an unrelated package does NOT. Confirm by looking for 'Checking <gated crate>' in THIS run's output; if no such line appears, the green came from cargo's cache and is NOT a re-read (t116 W-1)."
        Write-Host "[cargo-team] ==================================================="
    }
    # W-5 (t117): the gate's command line is announced AFTER the clean, so the log no
    # longer reads as if the gate ran before anything was removed.
    Write-Host "[cargo-team] cargo $($cargoArgs -join ' ')"
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    & cargo @cargoArgs
    $code = $LASTEXITCODE
    $sw.Stop()
    if ($uncovered.Count -gt 0) {
        Write-Host "[cargo-team] re-read check: this run cleaned $($cleanFirst -join ', '); a re-read of $gateDesc shows up above as 'Checking <crate>'. No such line => cache green, not a gate (t116 W-1)."
    }
    Write-Host ("[cargo-team] exit={0} elapsed={1:N1}s" -f $code, $sw.Elapsed.TotalSeconds)
    exit $code
} finally {
    if ($lock) { $lock.Dispose() }
}
