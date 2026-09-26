# ruagent daemon: start / stop / status / watch / install-task
#
# Why this file exists (t188):
#   * `nohup ... &` from a shell dies with that shell's call, and
#     `Start-Process -RedirectStandardOutput` both timed out and *truncated*
#     the previous log - which destroyed the evidence of the previous death.
#   * The daemon therefore logs to a FIXED file that is APPENDED to, and is
#     launched through WMI so it is not a child of the caller.
#
#   start   launch it (WMI; survives the calling shell; appends the log)
#   stop    stop the daemon recorded in <root>/data/daemon.pid (never by name/port)
#   status  health check + pid + log tail
#   watch   start it only if the health check fails (this is what the task runs)
#   install-task  register a scheduled task: at logon, and every 5 minutes
#
# Logging: <root>/logs/daemon.log, appended, rotated to daemon.log.1 at 5 MB.
param(
  [Parameter(Position=0)][ValidateSet('start','stop','status','watch','install-task')][string]$Action = 'status',
  [string]$Root = (Join-Path $env:USERPROFILE '.ruagent'),
  [string]$Exe  = 'D:\rust_cache\debug\ruagent.exe',
  [string]$Addr = '127.0.0.1:8787',
  [int]$MaxLogBytes = 5MB
)
$ErrorActionPreference = 'Stop'
$logDir = Join-Path $Root 'logs'
$log    = Join-Path $logDir 'daemon.log'
$pidRec = Join-Path $Root 'data\daemon.pid'
$health = "http://$Addr/api/v1/health"

function Get-LogPath {
  New-Item -ItemType Directory -Force -Path $logDir | Out-Null
  if ((Test-Path $log) -and (Get-Item $log).Length -gt $MaxLogBytes) {
    Move-Item -Force $log (Join-Path $logDir 'daemon.log.1')
  }
  return $log
}

function Test-Health {
  try { $r = Invoke-RestMethod -TimeoutSec 5 -Uri $health; return ($r.status -eq 'ok') } catch { return $false }
}

function Get-DaemonPid {
  if (-not (Test-Path $pidRec)) { return $null }
  $first = (Get-Content $pidRec -TotalCount 1).Trim().Split(' ')[0]
  $p = $null
  if ([int]::TryParse($first, [ref]$p)) {
    $proc = Get-Process -Id $p -ErrorAction SilentlyContinue
    if ($proc -and $proc.ProcessName -eq 'ruagent') { return $p }
  }
  return $null
}

switch ($Action) {
  'start' {
    if (Test-Health) { Write-Output 'already running (health ok)'; break }
    $logPath = Get-LogPath
    if (-not (Test-Path $Exe)) { throw "exe not found: $Exe" }
    # WMI: the child is owned by the WMI service, not by this shell, so it
    # survives the caller. `cmd /c` does the append redirection (WMI itself
    # cannot redirect - and Start-Process' redirect is the shape that hung).
    $cmd = 'cmd.exe /c ""' + $Exe + '" serve --addr ' + $Addr + ' --root "' + $Root + '" >> "' + $logPath + '" 2>&1"'
    # ShowWindow 0: a daemon that opens a console window is a window the user
    # did not ask for and has to close. WMI hands the child a console unless
    # the startup information says otherwise; `cmd /c` stays because it is the
    # redirection shape that appends instead of truncating the log.
    $startup = ([wmiclass]'Win32_ProcessStartup').CreateInstance()
    $startup.ShowWindow = 0
    $res = ([wmiclass]'Win32_Process').Create($cmd, $null, $startup)
    if ($res.ReturnValue -ne 0) { throw "Win32_Process.Create failed: $($res.ReturnValue)" }
    Write-Output "started pid=$($res.ProcessId) log=$logPath"
    for ($i = 0; $i -lt 20; $i++) { Start-Sleep -Milliseconds 500; if (Test-Health) { Write-Output 'health ok'; break } }
  }
  'stop' {
    # Only the pid recorded by the daemon itself: never a name or port sweep.
    $p = Get-DaemonPid
    if (-not $p) { Write-Output 'no recorded daemon pid (nothing to stop)'; break }
    Stop-Process -Id $p -ErrorAction SilentlyContinue
    Write-Output "stopped pid=$p"
  }
  'status' {
    $p = Get-DaemonPid
    $healthText = 'DOWN'
    if (Test-Health) { $healthText = 'ok' }
    $pidText = 'none'
    if ($p) { $pidText = "$p" }
    Write-Output "health: $healthText"
    Write-Output "pid:    $pidText"
    if (Test-Path $log) { Write-Output 'log tail:'; Get-Content $log -Tail 8 }
  }
  'watch' {
    # Every decision is appended to watchdog.log: that file plus the daemon
    # log's start banner is how a death becomes visible after the fact. The
    # daemon's own log cannot record a hard kill, so the *absence* of lines
    # (a gap) plus a new banner plus this record is the trace.
    New-Item -ItemType Directory -Force -Path $logDir | Out-Null
    $stamp = (Get-Date).ToString('s')
    if (Test-Health) {
      Add-Content -Path (Join-Path $logDir 'watchdog.log') -Value "$stamp ok pid=$(Get-DaemonPid)"
      Write-Output 'ok (no action)'
    } else {
      $dead = Get-DaemonPid
      Add-Content -Path (Join-Path $logDir 'watchdog.log') -Value "$stamp DOWN (recorded pid=$(if ($dead) { $dead } else { 'none' })) - restarting"
      Write-Output 'health DOWN - restarting'
      & $PSCommandPath start -Root $Root -Exe $Exe -Addr $Addr
    }
  }
  'install-task' {
    $scriptPath = $PSCommandPath
    $arg = "-NoProfile -ExecutionPolicy Bypass -File `"$scriptPath`" watch -Root `"$Root`" -Exe `"$Exe`" -Addr `"$Addr`""
    $a = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument $arg
    $t1 = New-ScheduledTaskTrigger -AtLogOn
    $t2 = New-ScheduledTaskTrigger -Once -At (Get-Date) -RepetitionInterval (New-TimeSpan -Minutes 5)
    Register-ScheduledTask -TaskName 'ruagent-daemon-watchdog' -Action $a -Trigger @($t1, $t2) -Force | Out-Null
    Write-Output 'scheduled task ruagent-daemon-watchdog registered (at logon + every 5 min)'
  }
}