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
#           -- and only if it IS that root daemon: see Get-DaemonIdentity below,
#           which status/watch/stop all share (t340, discipline 7.92)
#   status  health check + pid + log tail
#   watch   start it only if the health check fails (this is what the task runs)
#   install-task  register a scheduled task: at logon, and every 5 minutes
#
# Logging: <root>/logs/daemon.log, appended, rotated to daemon.log.1 at 5 MB.
# Stop:    refuses to run without -Force (a script cannot post to the team channel, so
#          it refuses to be silent instead); with -Force it first APPENDS a notice to
#          <root>/logs/daemon-stop-notice.log (rotated to .1 at 256 KB) and then stops
#          the recorded pid.
param(
  [Parameter(Position=0)][ValidateSet('start','stop','status','watch','install-task')][string]$Action = 'status',
  [string]$Root = (Join-Path $env:USERPROFILE '.ruagent'),
  [string]$Exe  = 'D:\rust_cache\debug\ruagent.exe',
  [string]$Addr = '127.0.0.1:8787',
  [int]$MaxLogBytes = 5MB,
  [int]$MaxNoticeBytes = 256KB,
  [string]$Reason = '',
  [switch]$Force
)
$ErrorActionPreference = 'Stop'
$logDir = Join-Path $Root 'logs'
$log    = Join-Path $logDir 'daemon.log'
$pidRec = Join-Path $Root 'data\daemon.pid'
$notice = Join-Path $logDir 'daemon-stop-notice.log'
$health = "http://$Addr/api/v1/health"

function Get-LogPath {
  New-Item -ItemType Directory -Force -Path $logDir | Out-Null
  if ((Test-Path $log) -and (Get-Item $log).Length -gt $MaxLogBytes) {
    Move-Item -Force $log (Join-Path $logDir 'daemon.log.1')
  }
  return $log
}

# Same shape as Get-LogPath: APPEND, rotate at a size cap. Appending is what
# keeps an earlier stop record alive (t188 lost a death record to a '>' that
# truncated the log); the cap is what keeps the record from becoming
# unreadable - neither alone is enough.
function Get-NoticePath {
  New-Item -ItemType Directory -Force -Path $logDir | Out-Null
  if ((Test-Path $notice) -and (Get-Item $notice).Length -gt $MaxNoticeBytes) {
    Move-Item -Force $notice (Join-Path $logDir 'daemon-stop-notice.log.1')
  }
  return $notice
}

function Test-Health {
  try { $r = Invoke-RestMethod -TimeoutSec 5 -Uri $health; return ($r.status -eq 'ok') } catch { return $false }
}

# ── ONE identity definition, shared by status / watch / stop (7.92) ─────────
# Until t340 this function asked only "does the process named in the pid file
# exist, and is it called ruagent?". On a PERSISTENT root that is a false
# positive: the daemon dies, the pid file stays, and if Windows recycles that
# pid to ANY other ruagent process (another root temp daemon, for instance)
# the answer is "alive" -- so status prints a pid that is not ours, and
# stop -Force would kill an innocent process. 7.86 needs the same identity to
# decide what to clean up, so both read it from here instead of each keeping
# its own spelling.
function Get-NormalizedPath {
  param([string]$Path)
  if (-not $Path) { return '' }
  # / and \ are the same separator to Windows, a trailing one is noise,
  # and the comparison must not be case sensitive.
  return $Path.Trim().Replace('/', '\').TrimEnd('\').ToLowerInvariant()
}

function Get-DaemonIdentity {
  param([string]$Root, [string]$PidFile)
  $id = [pscustomobject]@{
    Pid = $null; ProcessExists = $false; NameMatches = $false; RootMatches = $false; Ours = $false
  }
  if (-not (Test-Path $PidFile)) { return $id }
  # The file holds "pid timestamp" (two values): the first token is the pid.
  $first = (Get-Content $PidFile -TotalCount 1).Trim().Split(' ')[0]
  $p = 0
  if (-not [int]::TryParse($first, [ref]$p)) { return $id }
  $id.Pid = $p
  $proc = Get-CimInstance Win32_Process -Filter "ProcessId=$p" -ErrorAction SilentlyContinue
  if (-not $proc) { return $id }
  $id.ProcessExists = $true
  $id.NameMatches = ($proc.Name -eq 'ruagent.exe')
  $cmd = ''; if ($proc.CommandLine) { $cmd = $proc.CommandLine }
  # BOTH sides are normalized: the daemon was launched with the root as the
  # caller spelled it (C:/... in this session, C:\... in another), and a
  # one-sided comparison reads a real daemon as not-ours -- measured, and it
  # is the false NEGATIVE direction of this check.
  $id.RootMatches = (Get-NormalizedPath $cmd).Contains((Get-NormalizedPath $Root))
  $id.Ours = $id.NameMatches -and $id.RootMatches
  return $id
}

function Get-DaemonPid {
  $id = Get-DaemonIdentity -Root $Root -PidFile $pidRec
  if ($id.Ours) { return $id.Pid }
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
    # 7.92: a stale pid file can name a LIVE process that is not ours. Stopping
    # on the pid alone would kill an innocent process -- so the identity decides,
    # and a refusal says which half failed.
    $id = Get-DaemonIdentity -Root $Root -PidFile $pidRec
    if (-not $id.Ours) {
      if ($id.ProcessExists) {
        Write-Output ("REFUSING to stop pid $($id.Pid): it is alive but not this root's daemon (name-match=$($id.NameMatches) root-match=$($id.RootMatches)); nothing was stopped.")
      } else {
        Write-Output 'no recorded daemon pid (nothing to stop)'
      }
      break
    }
    $p = $id.Pid
    # Posting to the team channel is the one step a script cannot do. What it CAN
    # do is refuse to be silent: without -Force it stops nothing and prints what
    # stopping would break, plus the text a human should send. -Force is what the
    # unattended paths use, so they are unaffected.
    $announce = 'daemon stop: root=' + $Root + ' pid=' + $p + ' addr=' + $Addr + ' - verification against ' + $Addr + ' fails until it is back'
    if (-not $Force) {
      Write-Output 'REFUSING to stop without -Force.'
      Write-Output ('impact:   stopping it interrupts anyone verifying against ' + $Addr + ' for the length of the restart')
      Write-Output ('announce: ' + $announce)
      Write-Output 'nothing was stopped: the process and the pid file are untouched.'
      break
    }
    $noticePath = Get-NoticePath
    $stamp = (Get-Date).ToString('s')
    $reasonText = 'unspecified'
    if ($Reason) { $reasonText = $Reason }
    Add-Content -Path $noticePath -Value ('[' + $stamp + '] pid=' + $p + ' root=' + $Root + ' reason=' + $reasonText + ' announce=' + $announce)
    Stop-Process -Id $p -ErrorAction SilentlyContinue
    Write-Output "stopped pid=$p"
    Write-Output ('notice appended: ' + $noticePath)
  }
  'status' {
    # The recorded pid is reported with its IDENTITY, not just as a number: a
    # recycled pid is the case this line exists to make visible (t340).
    $id = Get-DaemonIdentity -Root $Root -PidFile $pidRec
    $healthText = 'DOWN'
    if (Test-Health) { $healthText = 'ok' }
    $pidText = 'none'
    if ($id.Ours) {
      $pidText = "$($id.Pid) (ours)"
    } elseif ($id.ProcessExists) {
      $pidText = "$($id.Pid) NOT OURS (alive, but name-match=$($id.NameMatches) root-match=$($id.RootMatches): the pid was recycled or the file is stale)"
    }
    Write-Output "health: $healthText"
    Write-Output "pid:    $pidText"
    if (Test-Path $log) { Write-Output 'log tail:'; Get-Content $log -Tail 8 }
  }
  'watch' {
    # This path never calls 'stop': it only starts. So the -Force gate above
    # cannot block the unattended restart. (If it ever needs to stop first, it
    # must pass -Force - that is the point of the gate.)
    # Every decision is appended to watchdog.log: that file plus the daemon
    # log's start banner is how a death becomes visible after the fact. The
    # daemon's own log cannot record a hard kill, so the *absence* of lines
    # (a gap) plus a new banner plus this record is the trace.
    New-Item -ItemType Directory -Force -Path $logDir | Out-Null
    $stamp = (Get-Date).ToString('s')
    if (Test-Health) {
      $id = Get-DaemonIdentity -Root $Root -PidFile $pidRec
      $ours = 'none'
      if ($id.Ours) { $ours = "$($id.Pid)" } elseif ($id.ProcessExists) { $ours = "$($id.Pid)!not-ours" }
      Add-Content -Path (Join-Path $logDir 'watchdog.log') -Value "$stamp ok pid=$ours"
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