
<#
.SYNOPSIS
  Start a throwaway daemon instance on a spare port + temp root, so a freshly built
  binary can be exercised before the real one is swapped.

  Why a separate script: the real daemon is started by scripts/ruagent-daemon.ps1 and
  holds D:\rust_cache\debug\ruagent.exe open, so the binary cannot be replaced while it
  runs. This script never touches the real instance: it uses its own port, its own root
  and its own pid file, and -Stop kills only the pid it recorded itself.
#>
param(
  [ValidateSet('start','stop','status')] [string]$Action = 'start',
  [string]$Exe  = 'D:\rust_cache\debug\ruagent.exe',
  [string]$Addr = '127.0.0.1:8791',
  [string]$Root = (Join-Path $env:TEMP 'ruagent-canary'),
  [string]$Log  = (Join-Path $env:TEMP 'ruagent-canary.log'),
  [int]$TimeoutSec = 90
)
$ErrorActionPreference = 'Stop'
$pidFile = Join-Path $env:TEMP 'ruagent-canary.pid'

function Get-CanaryPid {
  if (-not (Test-Path $pidFile)) { return $null }
  $raw = (Get-Content $pidFile -Raw).Trim()
  $p = 0
  if ([int]::TryParse($raw, [ref]$p)) { return $p }
  return $null
}

if ($Action -eq 'stop') {
  $p = Get-CanaryPid
  if (-not $p) { Write-Host 'canary: no recorded pid'; exit 0 }
  $proc = Get-Process -Id $p -ErrorAction SilentlyContinue
  if ($proc) { Stop-Process -Id $p -Force; Write-Host "canary: stopped pid $p" }
  else { Write-Host "canary: pid $p already gone" }
  Remove-Item $pidFile -Force -ErrorAction SilentlyContinue
  exit 0
}

if ($Action -eq 'status') {
  $p = Get-CanaryPid
  Write-Host "canary pid: $(if ($p) { $p } else { 'none' })"
  try {
    $r = Invoke-WebRequest -Uri "http://$Addr/" -UseBasicParsing -TimeoutSec 5
    Write-Host "canary health: $($r.StatusCode)"
  } catch { Write-Host "canary health: unreachable" }
  exit 0
}

if (-not (Test-Path $Exe)) { throw "exe not found: $Exe" }
New-Item -ItemType Directory -Force -Path $Root | Out-Null

# ShowWindow = 0: a WMI launch hands its child a console unless told otherwise.
$cmd = 'cmd.exe /c ""' + $Exe + '" serve --addr ' + $Addr + ' --root "' + $Root + '" >> "' + $Log + '" 2>&1"'
$startup = ([wmiclass]'Win32_ProcessStartup').CreateInstance()
$startup.ShowWindow = 0
$res = ([wmiclass]'Win32_Process').Create($cmd, $null, $startup)
if ($res.ReturnValue -ne 0) { throw "Win32_Process.Create failed: $($res.ReturnValue)" }
Set-Content -Path $pidFile -Value $res.ProcessId
Write-Host "canary: started pid $($res.ProcessId) on $Addr root $Root"

$deadline = (Get-Date).AddSeconds($TimeoutSec)
while ((Get-Date) -lt $deadline) {
  Start-Sleep -Milliseconds 750
  try {
    $r = Invoke-WebRequest -Uri "http://$Addr/" -UseBasicParsing -TimeoutSec 5
    if ($r.StatusCode -eq 200) { Write-Host "canary: healthy (200) after $([int]((Get-Date) - $deadline).TotalSeconds + $TimeoutSec)s"; exit 0 }
  } catch { }
}
Write-Host 'canary: NOT healthy within timeout'
exit 1
