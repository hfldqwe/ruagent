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
#   `cargo test ... -- --nocapture` and `clippy ... -- -D warnings` cannot be
#   forwarded verbatim through `powershell -File` (PowerShell consumes the bare
#   `--`). Use the two switches that expand to exactly those tails:
#     -Nocapture   appends `-- --nocapture`
#     -DenyWarnings appends `-- -D warnings`
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
# COMPARING TWO TREES: a before/after pair (git worktree, git archive export)
# must NOT share a target dir -- cargo would hand the second tree the first
# tree's rlibs (closure plan 7.1). Give the comparison tree a dir of its own:
#   ... -File scripts/cargo-team.ps1 test -p X -TargetDir "$env:TEMP\ruagent-cmp-a"
#
# -NoLock is only for such a deliberate second target dir that you want to run
# in parallel (rare).
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
    elseif ($a -eq '-Jobs') { $i++; $jobs = [int]$raw[$i] }
    elseif ($a -eq '-Cpus') { $i++; $cpus = [int]$raw[$i] }
    elseif ($a -eq '-TargetDir') { $i++; $targetDir = [string]$raw[$i] }
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
    if ($cleanFirst.Count -gt 0) { Write-Host "[cargo-team] (under the same lock) cargo clean -p $($cleanFirst -join ', -p ')" }
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
    Write-Host "[cargo-team] cargo $($cargoArgs -join ' ')"
    # -CleanFirst: force a real re-reading while still holding the lock, so no
    # peer build can slip in and re-warm the cache against the gate.
    foreach ($pkg in $cleanFirst) {
        Write-Host "[cargo-team] clean -p $pkg (forced re-check)"
        & cargo clean -p $pkg
        if ($LASTEXITCODE -ne 0) {
            Write-Host "[cargo-team] cargo clean -p $pkg exited $LASTEXITCODE; not running the gate"
            exit $LASTEXITCODE
        }
    }
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    & cargo @cargoArgs
    $code = $LASTEXITCODE
    $sw.Stop()
    Write-Host ("[cargo-team] exit={0} elapsed={1:N1}s" -f $code, $sw.Elapsed.TotalSeconds)
    exit $code
} finally {
    if ($lock) { $lock.Dispose() }
}
