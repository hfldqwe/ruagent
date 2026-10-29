<#
.SYNOPSIS
  Prove a release archive RUNS, on a machine that never saw this repo.

.WHY (ruagent-issues-56-59 t43)
  The archive used to ship the panel as panel-dist/, a sibling of the binary. The daemon
  resolves the panel as `$env:RUAGENT_PANEL_DIST`, then `<exe dir>/panel/dist`, then the build
  tree, then `<cwd>/panel/dist` (crates/daemon/src/api.rs) -- so `panel-dist/` matched NOTHING,
  and a user who unpacked the release got an API that answered and a `/` that returned 404.
  That is the t188 failure shape, and nothing in CI could see it: the release job never ran
  what it had just packaged. This script is that missing reading.

.WHAT IT CHECKS
  1. the archive's own layout: `<exe name>` at the root, `panel/dist/index.html` beside it;
  2. `ruagent --version` succeeds (the binary is not a stub / a wrong-arch build);
  3. a REAL daemon on a free loopback port with a private `--root` answers
     /api/v1/health 200, AND `/` returns 200 with the PANEL in the body (not just an API);
  4. /api/v1/stats reports which embedder the daemon ended up with (the ONNX path's outcome,
     see the evidence block in .github/workflows/release.yml);
  5. a clean stop through `<root>/data/daemon.stop`, with a forced Stop-Process only as a
     fallback -- and the daemon's own log tail is printed either way.

  Any failure prints what it saw and exits 1: a release that cannot run must not be published.

.EXAMPLE
  pwsh -File .github/scripts/smoke-archive.ps1 -Archive ruagent-v1.2.3-x86_64-windows.zip
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Archive,
    [string]$Exe = 'ruagent.exe',
    [int]$BootTimeoutSeconds = 120
)

$ErrorActionPreference = 'Stop'

# Loopback through a possibly-configured system proxy. This repo's machine carries a DEAD
# 127.0.0.1:7890 in the Windows settings, which makes every loopback probe fail with a
# connect error that names a port the code never bound; a CI runner has no proxy at all, so
# setting this is harmless there and decisive here.
$env:NO_PROXY = '127.0.0.1,localhost,::1'

function Show-PanelCandidates([string]$stage) {
    # THE DIAGNOSIS, not the symptom. The candidate list is the daemon's own order
    # (crates/daemon/src/api.rs); printing it is what turns "404 on /" into an answer.
    Write-Output 'the daemon looks for the panel in this order (crates/daemon/src/api.rs):'
    $envCandidate = '<unset>'
    if ($env:RUAGENT_PANEL_DIST) { $envCandidate = $env:RUAGENT_PANEL_DIST }
    Write-Output ('  1. ' + [char]36 + "env:RUAGENT_PANEL_DIST = $envCandidate")
    Write-Output ('  2. <exe dir>/panel/dist        = ' + (Join-Path $stage 'panel/dist') + '  <-- the one a package can satisfy')
    Write-Output '  3. <build tree>/panel/dist     = (not present in a packaged archive)'
    Write-Output ('  4. <cwd>/panel/dist            = ' + (Join-Path (Get-Location).Path 'panel/dist'))
    foreach ($want in @('index.html', 'ruagent.exe')) {
        $hits = @(Get-ChildItem -LiteralPath $stage -Recurse -Filter $want -ErrorAction SilentlyContinue |
            ForEach-Object { $_.FullName.Substring($stage.Length).TrimStart([char]92, [char]47) })
        $where = '<none>'
        if ($hits.Count -gt 0) { $where = $hits -join ', ' }
        Write-Output ("$want files anywhere in the archive: $where")
    }
}

function Stop-With([string]$why) {
    Write-Output "SMOKE FAILED: $why"
    exit 1
}

$stage = Join-Path ([IO.Path]::GetTempPath()) ('ruagent-smoke-' + [Guid]::NewGuid().ToString('N').Substring(0, 8))
New-Item -ItemType Directory -Force $stage | Out-Null
$root = Join-Path $stage 'root'
$daemon = $null
$ok = $false

try {
    if (-not (Test-Path -LiteralPath $Archive)) { Stop-With "no archive at $Archive" }
    Expand-Archive -Path $Archive -DestinationPath $stage -Force

    # WHAT THE ARCHIVE ACTUALLY HOLDS. Printed always and first: an unreadable archive and an
    # archive with the wrong layout produce the same downstream symptom, and this line is what
    # separates them for the next reader.
    $top = @(Get-ChildItem -LiteralPath $stage -Force | ForEach-Object { $_.Name })
    Write-Output ("archive root entries: " + ($top -join ', '))

    $exePath = Join-Path $stage $Exe
    $index = Join-Path $stage 'panel/dist/index.html'

    if (-not (Test-Path -LiteralPath $exePath)) {
        Show-PanelCandidates $stage
        Stop-With "the archive has no $Exe at its root; it holds [$($top -join ', ')]"
    }

    if (-not (Test-Path -LiteralPath $index)) {
        Show-PanelCandidates $stage
        Stop-With 'the archive carries no panel/dist/index.html at its root, so EVERY candidate above misses and / would answer 404 (the t188 shape)'
    }

    # 1. THE BINARY
    $version = (& $exePath --version 2>&1 | Out-String).Trim()
    if ($LASTEXITCODE -ne 0) { Stop-With "$Exe --version exited $LASTEXITCODE : $version" }
    Write-Output "version: $version"

    # 2. A REAL DAEMON on a free loopback port, private root (never the runner's ~/.ruagent)
    $listener = New-Object System.Net.Sockets.TcpListener([System.Net.IPAddress]::Loopback, 0)
    $listener.Start()
    $port = ([System.Net.IPEndPoint]$listener.LocalEndpoint).Port
    $listener.Stop()
    $base = "http://127.0.0.1:$port"
    $outLog = Join-Path $stage 'daemon.out.log'
    $errLog = Join-Path $stage 'daemon.err.log'
    Write-Output "starting: $Exe serve --addr 127.0.0.1:$port --root $root"
    $daemon = Start-Process -FilePath $exePath -ArgumentList @('serve', '--addr', "127.0.0.1:$port", '--root', $root) -PassThru -WindowStyle Hidden -RedirectStandardOutput $outLog -RedirectStandardError $errLog

    $health = $null
    $deadline = (Get-Date).AddSeconds($BootTimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        if ($daemon.HasExited) {
            if (Test-Path $outLog) { Write-Output 'daemon stdout:'; Get-Content $outLog -Tail 20 }
            if (Test-Path $errLog) { Write-Output 'daemon stderr:'; Get-Content $errLog -Tail 20 }
            Stop-With "the daemon exited (code $($daemon.ExitCode)) before answering /api/v1/health"
        }
        try { $health = Invoke-WebRequest -Uri "$base/api/v1/health" -UseBasicParsing -TimeoutSec 5 } catch { $health = $null }
        if ($health -and ($health.StatusCode -eq 200)) { break }
        Start-Sleep -Milliseconds 500
    }
    if (-not ($health -and ($health.StatusCode -eq 200))) {
        if (Test-Path $outLog) { Write-Output 'daemon stdout:'; Get-Content $outLog -Tail 20 }
        if (Test-Path $errLog) { Write-Output 'daemon stderr:'; Get-Content $errLog -Tail 20 }
        Stop-With "no 200 from $base/api/v1/health within $BootTimeoutSeconds s"
    }
    Write-Output ("GET /api/v1/health -> 200 " + $health.Content)

    # 3. THE POINT OF THIS SCRIPT: the PANEL, not merely the API.
    try {
        $page = Invoke-WebRequest -Uri "$base/" -UseBasicParsing -TimeoutSec 10
    } catch {
        Stop-With ("GET / failed: " + $_.Exception.Message + ' -- the API is up, so this is the panel not being found, not a dead daemon')
    }
    if ($page.StatusCode -ne 200) { Stop-With "GET / -> $($page.StatusCode): the API answered but the panel was not served" }
    if ($page.Content -notmatch '<title>ruagent</title>') {
        $head = $page.Content
        if ($head.Length -gt 200) { $head = $head.Substring(0, 200) }
        Stop-With "GET / answered 200 but the body is not the panel (no <title>ruagent</title>); first 200 chars: $head"
    }
    Write-Output ("GET / -> 200 with the panel marker (<title>ruagent</title>), " + $page.RawContentLength + ' bytes')

    # 4. WHICH EMBEDDER? The daemon tries the ONNX-backed FastEmbedder at boot and falls back
    #    to the offline hash embedder (crates/daemon/src/lib.rs). /api/v1/stats reports the
    #    result, so this line is a live reading of what the ONNX path did on a machine that has
    #    no shipped onnxruntime library -- see the evidence block in release.yml.
    try {
        $stats = Invoke-WebRequest -Uri "$base/api/v1/stats" -UseBasicParsing -TimeoutSec 10
        $obj = $stats.Content | ConvertFrom-Json
        Write-Output ("embedder in use: " + $obj.embedder)
    } catch {
        Write-Output ('embedder reading unavailable: ' + $_.Exception.Message)
    }

    # 5. CLEAN STOP: the request file is the daemon's own channel (a console-less process has
    #    no Ctrl-C). A forced stop is the fallback and is reported, never silent.
    $stopFile = Join-Path $root 'data/daemon.stop'
    New-Item -ItemType Directory -Force (Split-Path $stopFile) | Out-Null
    Set-Content -Path $stopFile -Value 'smoke-archive: stop requested'
    $stopped = $false
    for ($i = 0; $i -lt 40; $i++) {
        Start-Sleep -Milliseconds 250
        if ($daemon.HasExited) { $stopped = $true; break }
    }
    if ($stopped) {
        Write-Output 'daemon stopped through data/daemon.stop (clean exit)'
    } else {
        Stop-Process -Id $daemon.Id -Force -ErrorAction SilentlyContinue
        Start-Sleep -Milliseconds 500
        Write-Output 'daemon ignored the stop request for 10 s -> forced (the smoke does not fail for that, but a published build should be clean)'
    }

    if (Test-Path $outLog) { Write-Output 'daemon log tail:'; Get-Content $outLog -Tail 12 }
    if (Test-Path (Join-Path $root 'data/daemon.exit')) {
        Write-Output ('exit trace: ' + (Get-Content (Join-Path $root 'data/daemon.exit') -TotalCount 1))
    }
    $ok = $true
} finally {
    if ($daemon -and -not $daemon.HasExited) {
        Stop-Process -Id $daemon.Id -Force -ErrorAction SilentlyContinue
    }
    if ($ok) {
        Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction SilentlyContinue
    } else {
        Write-Output "kept for inspection: $stage"
    }
}

Write-Output "SMOKE OK: $Archive runs from its own contents and serves the panel"
exit 0
