
<#
.SYNOPSIS
  Start a throwaway daemon instance on a spare port + temp root, so a freshly built
  binary can be exercised before the real one is swapped.

  The real daemon holds D:\rust_cache\debug\ruagent.exe open, so the binary cannot be
  replaced while it runs. This script never touches the real instance: own port, own
  root, own pid file, and -Stop kills only the daemon this script started.

  WHY THE PID COMES FROM THE ROOT, NOT FROM Win32_Process.Create: Create returns the
  pid of the cmd.exe wrapper, and that wrapper exits as soon as it has spawned the
  daemon. Recording it means -Stop reports "already gone" while the daemon keeps
  listening (measured 2026-09-26: port 8791 still answered 200 after "stopped", and the
  orphan had to be found by its command line). The daemon writes its own pid to
  <root>/data/daemon.pid, so that file is the truth. Before killing anything we also
  require the target's command line to mention this root -- never kill by name or port.
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
$rootPidFile = Join-Path $Root 'data\daemon.pid'
$wrapperPidFile = Join-Path $env:TEMP 'ruagent-canary.wrapper.pid'

# The identity of "this root's daemon" comes from the shared lib, not from a
# local spelling: discipline 7.86 needs the name AND the root, 7.92 needs both
# sides normalized, and the daemon script reads the same fields.
. (Join-Path $PSScriptRoot 'lib\daemon-identity.ps1')

function Read-Pid($path) {
  if (-not (Test-Path $path)) { return $null }
  $raw = (Get-Content $path -Raw).Trim() -split '\s+' | Select-Object -First 1
  $p = 0
  if ([int]::TryParse($raw, [ref]$p)) { return $p }
  return $null
}

# The only process this script may kill: the pid file first token, and only if
# that process IS this root's daemon -- name AND command line, both sides
# normalized. Measured before t342: the bare substring test this replaces matched
# any process whose command line MENTIONED the root, including a shell that had
# merely received it as an argument, and -Stop then killed it.
function Get-CanaryProcess {
  $id = Get-DaemonIdentity -Root $Root -PidFile $rootPidFile
  if (-not $id.Ours) { return $null }
  return $id.Process
}

if ($Action -eq 'stop') {
  $id = Get-DaemonIdentity -Root $Root -PidFile $rootPidFile
  if ($id.Ours) {
    Stop-Process -Id $id.Pid -Force
    Write-Host "canary: stopped pid $($id.Pid)"
  } elseif ($id.ProcessExists) {
    Write-Host "canary: REFUSING to stop pid $($id.Pid): it is alive but not this root's daemon (name-match=$($id.NameMatches) root-match=$($id.RootMatches)); nothing was stopped."
  } else {
    Write-Host 'canary: no live canary daemon for this root'
  }
  Remove-Item $wrapperPidFile -Force -ErrorAction SilentlyContinue
  exit 0
}

if ($Action -eq 'status') {
  $proc = Get-CanaryProcess
  $sid = Get-DaemonIdentity -Root $Root -PidFile $rootPidFile
  $shown = 'none'
  if ($sid.Ours) { $shown = "$($sid.Pid) (ours)" }
  elseif ($sid.ProcessExists) { $shown = "$($sid.Pid) NOT OURS (name-match=$($sid.NameMatches) root-match=$($sid.RootMatches))" }
  Write-Host "canary daemon pid: $shown"
  try {
    $r = Invoke-WebRequest -Uri "http://$Addr/" -UseBasicParsing -TimeoutSec 5
    Write-Host "canary health: $($r.StatusCode)"
  } catch { Write-Host 'canary health: unreachable' }
  exit 0
}

if (-not (Test-Path $Exe)) { throw "exe not found: $Exe" }
New-Item -ItemType Directory -Force -Path $Root | Out-Null
Remove-Item $rootPidFile -Force -ErrorAction SilentlyContinue

# ShowWindow = 0: a WMI launch hands its child a console unless told otherwise.
$cmd = 'cmd.exe /c ""' + $Exe + '" serve --addr ' + $Addr + ' --root "' + $Root + '" >> "' + $Log + '" 2>&1"'
$startup = ([wmiclass]'Win32_ProcessStartup').CreateInstance()
$startup.ShowWindow = 0
$res = ([wmiclass]'Win32_Process').Create($cmd, $null, $startup)
if ($res.ReturnValue -ne 0) { throw "Win32_Process.Create failed: $($res.ReturnValue)" }
Set-Content -Path $wrapperPidFile -Value $res.ProcessId

$deadline = (Get-Date).AddSeconds($TimeoutSec)
$healthy = $false
while ((Get-Date) -lt $deadline) {
  Start-Sleep -Milliseconds 750
  try {
    $r = Invoke-WebRequest -Uri "http://$Addr/" -UseBasicParsing -TimeoutSec 5
    if ($r.StatusCode -eq 200) { $healthy = $true; break }
  } catch { }
}
$proc = Get-CanaryProcess
$daemonPid = if ($proc) { $proc.ProcessId } else { 'unknown' }
if ($healthy) {
  Write-Host "canary: healthy (200) on $Addr, daemon pid $daemonPid, root $Root"
  exit 0
}
Write-Host ("canary: NOT healthy within " + $TimeoutSec + "s (daemon pid " + $daemonPid + ")")
exit 1
