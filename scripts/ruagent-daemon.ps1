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
#   install-task  register a scheduled task (-TaskName, default ruagent-daemon-watchdog):
#                 at logon, and every 5 minutes, FOR THE
#                 CURRENT USER (a per-user principal needs no elevation, where an
#                 all-users one is denied in an ordinary shell). The success line is
#                 printed only after reading the task back and checking its shape
#                 (increment 7, t1: it used to claim 'registered' for a denied run).
#                 Its action launches `conhost.exe --headless`, NOT powershell.exe:
#                 a console window in the user's own session was this task's defect
#                 (reported 2026-10-09) -- see the launcher comment in install-task.
#
# Logging: <root>/logs/daemon.log, appended, rotated to daemon.log.1 at 5 MB.
# Stop:    refuses to run without -Force (a script cannot post to the team channel, so
#          it refuses to be silent instead); with -Force it first APPENDS a notice to
#          <root>/logs/daemon-stop-notice.log (rotated to .1 at 256 KB), then ASKS for a
#          clean exit (<root>/data/daemon.stop, 10s) and only then forces the pid.
#
# WHY A DAEMON THAT VANISHES NOW LEAVES EVIDENCE (issue #64, 2026-10-17).
#   The resident daemon disappeared with no panic, no error and no shutdown line: the
#   log ended on an ordinary INFO (chat idle, closing + mcp health ok), the process was
#   gone and the port refused -- and every member's private daemon died with it. From
#   outside, "a job object killed it" and "it exited cleanly" looked IDENTICAL, because
#   neither wrote anything. Now every daemon writes <root>/data/daemon.exit at boot
#   ("running pid=... since=...") and rewrites it with a reason on every path that knows
#   why it is leaving ("exit reason=stop-request|signal:console|panic|error ..."). A
#   KILLED daemon is the case that cannot rewrite it -- so the trace it leaves is its
#   boot line, and "status" reads that line WITH the liveness of its pid and reports
#   "DIED WITHOUT A TRACE". The next boot also logs a WARN about it.
#
# START METHOD, MEASURED (2026-10-19, this machine). "start" uses WMI
# (Win32_Process.Create) because the child is owned by the WMI service rather than by
# the calling shell: measured, the daemon is still alive after the calling shell's call
# returns. A plain background start from that same shell does not survive it (t188), and
# Start-Process -RedirectStandardOutput truncated the previous log, which destroyed the
# evidence of the previous death. The scheduled task ("install-task") is the unattended
# path and launches conhost.exe --headless -- see that comment for why a console
# application there puts a window on screen.
#   Whether to request job-object BREAKAWAY was left OPEN BY MEASUREMENT, not by
#   preference: the daemon now logs, at every boot, whether it was already inside
#   somebody else's job ("startup job-object measurement"), measured BEFORE the orphan
#   sweep creates the daemon's own job. If the next disappearance shows a foreign job
#   there, breakaway becomes the evidenced fix; requesting it now would also let the
#   daemon escape a job that is deliberately containing it, on no measurement at all.
param(
  [Parameter(Position=0)][ValidateSet('start','stop','status','watch','install-task')][string]$Action = 'status',
  [string]$Root = (Join-Path $env:USERPROFILE '.ruagent'),
  [string]$Exe  = 'D:\rust_cache\debug\ruagent.exe',
  [string]$Addr = '127.0.0.1:8787',
  [int]$MaxLogBytes = 5MB,
  [int]$MaxNoticeBytes = 256KB,
  [string]$Reason = '',
  # The task install-task registers. A parameter, not a constant, so the installer
  # can be verified against a THROWAWAY task name: registering one under a test name
  # exercises the whole path (pre-flight, registration, read-back) and unregistering
  # it restores the machine, where an installer that can only ever touch the ONE
  # machine-wide name cannot be tested without changing what the machine runs.
  [string]$TaskName = 'ruagent-daemon-watchdog',
  [switch]$Force
)
$ErrorActionPreference = 'Stop'
$logDir = Join-Path $Root 'logs'
$log    = Join-Path $logDir 'daemon.log'
$pidRec = Join-Path $Root 'data\daemon.pid'
$notice = Join-Path $logDir 'daemon-stop-notice.log'
$trace  = Join-Path $Root 'data\daemon.exit'    # issue #64: WHY the last daemon left
$stopReq = Join-Path $Root 'data\daemon.stop'   # issue #64: the graceful-stop request
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

# ── ONE identity definition, shared with the canary (7.86 / 7.92 / t342) ────
# The helpers moved to scripts/lib/daemon-identity.ps1 so that
# scripts/ruagent-canary.ps1 can use the SAME definition instead of keeping its
# own spelling of "ours". This file cannot be the shared home: it ends in a
# switch ($Action), so sourcing it would execute an action (discipline 7.111).
. (Join-Path $PSScriptRoot 'lib\daemon-identity.ps1')
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
    # t164: the health poll is the ONLY evidence that the daemon actually came up,
    # and its outcome used to be DISCARDED -- an unhealthy start still printed
    # `started ...` and exited 0, so a caller (or a person) reading the green line
    # concluded "ready" while nothing was listening. Form (b): the word "started"
    # is printed only once health has answered, because "started" is a claim about
    # the daemon, not about the WMI call. The identity of the launched process is
    # printed in BOTH paths on purpose: pid and log path are the WMI call's only
    # return value, and dropping them from the failure path would be losing
    # information, not adding rigour.
    $healthy = $false
    $healthStart = Get-Date
    for ($i = 0; $i -lt 20; $i++) { Start-Sleep -Milliseconds 500; if (Test-Health) { $healthy = $true; break } }
    if (-not $healthy) {
      # Elapsed is MEASURED, not assumed: Test-Health has a 5s ceiling of its own,
      # so a port held by a listener that accepts but never answers stretches this
      # loop past the 20x500ms the sleeps alone would suggest.
      $waited = [int]((Get-Date) - $healthStart).TotalSeconds
      Write-Output "NOT healthy after ${waited}s (pid=$($res.ProcessId) log=$logPath) -- nothing answered $health. The process may have exited (read the log) or another listener may hold $Addr."
      exit 1
    }
    Write-Output "started pid=$($res.ProcessId) log=$logPath"
    Write-Output 'health ok'
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
    # Issue #64: ask for a CLEAN exit first, and force only what ignores the ask.
    # A daemon started through WMI has NO console, so there is no Ctrl-C to send it:
    # the request file under the root it already owns is the only channel it has, and
    # the daemon polls it and leaves through its own shutdown path -- which is what
    # writes "exit reason=stop-request ..." into data/daemon.exit. The forced path is
    # kept, because a daemon that is wedged must still be stoppable -- and it is
    # exactly the case that leaves NO exit record, which is how status tells them apart.
    New-Item -ItemType Directory -Force -Path (Split-Path $stopReq) | Out-Null
    Set-Content -Path $stopReq -Value ('stop requested by ' + $env:USERNAME + ' at ' + $stamp + ' reason=' + $reasonText)
    $graceful = $false
    for ($i = 0; $i -lt 40; $i++) {
      Start-Sleep -Milliseconds 250
      if (-not (Get-Process -Id $p -ErrorAction SilentlyContinue)) { $graceful = $true; break }
    }
    if ($graceful) {
      Write-Output "stopped pid=$p (GRACEFUL: it honoured the stop request and wrote its exit trace)"
    } else {
      Stop-Process -Id $p -ErrorAction SilentlyContinue
      Write-Output "stopped pid=$p (FORCED after 10s: a forced stop cannot write an exit trace)"
    }
    if (Test-Path $stopReq) { Remove-Item -Force $stopReq }
    if (Test-Path $trace) { Write-Output ('exit trace: ' + (Get-Content -Path $trace -TotalCount 1)) }
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
    # Issue #64: WHY the last daemon left -- and the one case that has no record.
    # A killed daemon could not write anything, so the trace it left is the line it
    # wrote at BOOT; read together with the liveness of that pid, that line is the
    # diagnosis instead of a stale line. This is the whole difference the trace buys:
    # "shut down" and "was killed" stop looking the same from outside.
    if (Test-Path $trace) {
      $line = (Get-Content -Path $trace -TotalCount 1)
      Write-Output ("exit trace: " + $line)
      $m = [regex]::Match($line, '^running\s')
      if ($m.Success) {
        $tp = [regex]::Match($line, 'pid=(\d+)')
        if ($tp.Success) {
          $tpid = [int]$tp.Groups[1].Value
          $alive = Get-Process -Id $tpid -ErrorAction SilentlyContinue
          if ($alive) {
            Write-Output ("exit trace: pid $tpid is ALIVE, so that line is this daemon's boot record (it has not left)")
          } else {
            Write-Output ("exit trace: DIED WITHOUT A TRACE -- pid $tpid is gone and NO exit record was written: killed (job object / TerminateProcess / power loss), not shut down")
          }
        }
      }
    } else {
      Write-Output 'exit trace: none (no data/daemon.exit on this root: no daemon has ever recorded one here)'
    }
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
    # ── TWO SEPARATE DEFECTS used to live in this block (increment 7, t1) ───────
    # (a) SHAPE. It registered with no principal -- an ALL-USERS task at logon --
    #     which needs elevation, so an ordinary shell was denied. Measured on this
    #     machine from a non-elevated shell: Register-ScheduledTask -> '拒绝访问。'
    #     / 'Access is denied.', HRESULT 0x80070005, with Get-ScheduledTask finding
    #     nothing afterwards.
    # (b) REPORTING. The success line was printed unconditionally, so that same
    #     denied run still read as 'registered'. $ErrorActionPreference='Stop' does
    #     NOT cover this path, and that too is measured rather than assumed: the
    #     denial is terminating only when this file is pwsh's TOP-LEVEL command
    #     (`pwsh -File scripts/ruagent-daemon.ps1 install-task`); invoked the way a
    #     human in a session invokes it (`& scripts/ruagent-daemon.ps1 install-task`)
    #     the same denial left the script running to the success line, exit 0, with
    #     the error on the error stream. The guard was an accident of invocation
    #     shape, so the message below is tied to a READ-BACK instead -- the standard
    #     `stop` already follows by refusing to be silent.
    #
    # The shape is the one verified by hand on this machine: state=Ready, an explicit
    # principal for the CURRENT user (LogonType Interactive, RunLevel Limited), a
    # logon trigger scoped to that user, and a 5-minute repetition trigger. None of
    # that needs elevation. -Force keeps it idempotent: a second run updates the same
    # task instead of leaving two. The action keeps the script's OWN -Root/-Exe/-Addr
    # (dropping them would make `install-task -Exe <other>` register a watchdog that
    # ignores where it was pointed).
    $taskName = $TaskName
    $userId = "$env:USERDOMAIN\$env:USERNAME"
    $scriptPath = $PSCommandPath
    # ── WHAT the action launches: a WINDOWLESS console host, not powershell.exe ────
    # THE DEFECT THIS REPLACES (reported by this machine's user: 'a scheduled task keeps
    # popping a window up, it makes the machine unpleasant to use'). The action used to be
    # `powershell.exe -NoProfile -ExecutionPolicy Bypass -File … watch …`: a console
    # application started in the user's INTERACTIVE session, which Windows hands to the
    # machine's DEFAULT TERMINAL -- here Windows Terminal, because `HKCU:\Console\%%Startup`
    # holds the all-zero 'let Windows decide' GUIDs for DelegationConsole and
    # DelegationTerminal and wt.exe is installed. A VISIBLE terminal window therefore
    # appeared in front of whoever was using the machine, every 5 minutes, for as long as
    # the task stayed registered. What attributed it is the cadence plus the shape:
    # watchdog.log carries one line per run at :00:xx and :05:xx all day, and the launch
    # creates a console host in the interactive session.
    #
    # WHY NOT `-WindowStyle Hidden`: MEASURED, it is not enough. A powershell started with
    # that flag still owns a VISIBLE (WS_VISIBLE) window -- class `PseudoConsoleWindow`,
    # IsWindowVisible true -- because the window that ends up on screen belongs to the
    # DELEGATED terminal, not to powershell: hiding powershell's own console cannot hide
    # another process's window. `conhost.exe --headless` (Windows 11 22000+) runs the child
    # under a console that has NO window and does not consult the default-terminal setting.
    #
    # MEASURED vs NOT MEASURED, because the difference decides what this comment is worth:
    # measured = the 5-minute cadence, that this launch creates a console host in the
    # interactive session, that `-WindowStyle Hidden` still leaves a visible window, and
    # that `--headless` leaves none (the variants were driven through real scheduled tasks
    # and enumerated with EnumWindows/IsWindowVisible); NOT measured = an enumeration of
    # the Windows Terminal window itself, which the probe's process filter did not include.
    # The attribution rests on the registry setting plus the observed console host; the
    # remedy rests on --headless bypassing that delegation by design.
    $consoleHost = Join-Path $env:SystemRoot 'System32\conhost.exe'
    # Windows 10 has no `--headless` console host. Rather than register an action that
    # cannot start at all there, the launcher degrades to powershell + -WindowStyle
    # Hidden AND SAYS SO: on such a machine the window this task shows is reduced, not
    # provably gone, and the success line below reports which launcher was registered.
    $useHeadless = (Test-Path -LiteralPath $consoleHost -PathType Leaf) -and ([Environment]::OSVersion.Version.Build -ge 22000)
    $inner = "powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File `"$scriptPath`" watch -Root `"$Root`" -Exe `"$Exe`" -Addr `"$Addr`""
    if ($useHeadless) {
      $actionExec = $consoleHost
      $actionArgs = "--headless $inner"
    } else {
      $actionExec = 'powershell.exe'
      $actionArgs = "-WindowStyle Hidden $inner"
    }
    $a = New-ScheduledTaskAction -Execute $actionExec -Argument $actionArgs
    $t1 = New-ScheduledTaskTrigger -AtLogOn -User $userId
    $t2 = New-ScheduledTaskTrigger -Once -At (Get-Date) -RepetitionInterval (New-TimeSpan -Minutes 5)
    $principal = New-ScheduledTaskPrincipal -UserId $userId -LogonType Interactive -RunLevel Limited

    # Account comparison that survives the domain-spelling difference: the request
    # says 'HFLD\19410' while Get-ScheduledTask reports the principal as '19410' and
    # the XML stores a SID. Resolve to a SID, and fall back to the bare account name
    # when the account cannot be resolved at all.
    function Get-AccountKey {
      param([string]$Account)
      if (-not $Account) { return '' }
      try { return ([System.Security.Principal.NTAccount]$Account).Translate([System.Security.Principal.SecurityIdentifier]).Value } catch { return $Account.Split('\')[-1] }
    }

    # ── The ACTION helpers (t12). The action is the one field that decides WHAT
    # runs, and it was the one field this read-back never looked at: during t4's
    # verification an instrumented COPY of this script ran install-task and, because
    # the action is built from $PSCommandPath, re-pointed the machine's watchdog at
    # `%TEMP%\t4-scripts\ruagent-daemon.ps1` -- the copy's `watch` then fired from a
    # temp file -- and this read-back's success line blessed it. WHO (principal) and
    # WHEN (triggers) were right; WHAT was another file.

    # The script a registered action would run, pulled out of its `-File "..."`.
    function Get-ActionScriptPath {
      param([string]$Arguments)
      if ($Arguments -match '-File\s+"([^"]+)"') { return $Matches[1] }
      if ($Arguments -match '-File\s+(\S+)') { return $Matches[1] }
      return ''
    }

    # Is this file the REPOSITORY's copy of the script (as opposed to a copy someone
    # dropped in a temp directory)? A checkout of this repository has Cargo.toml next
    # to a crates\daemon directory. Deliberately structural rather than
    # content-based: the incident's copy was instrumented, so comparing bytes would
    # have flagged the instrumentation and missed the point -- what matters is that
    # the file is not the repo's script at all.
    function Test-RepoScript {
      param([string]$Script)
      if (-not $Script) { return $false }
      $root = Split-Path (Split-Path $Script -Parent) -Parent
      if (-not $root) { return $false }
      return ((Test-Path (Join-Path $root 'Cargo.toml')) -and (Test-Path (Join-Path $root 'crates\daemon')))
    }

    # The same invocation written with different whitespace/case is the same
    # invocation: the comparison is about what would run, not about spelling.
    function Get-ArgShape {
      param([string]$Arguments)
      if (-not $Arguments) { return '' }
      return (($Arguments -replace '\s+', ' ').Trim().ToLowerInvariant())
    }

    # ONE formatter for both sides (t12/V4): same keys, same order, same rendering,
    # so `observed:` and `wanted:` differ ONLY where a value differs.
    function Format-TaskShape {
      param([System.Collections.IDictionary]$Shape)
      $parts = foreach ($k in $Shape.Keys) { "$k=$($Shape[$k])" }
      return ($parts -join ' ')
    }

    # V3 (t12): a failure must be non-zero HOWEVER the caller invoked this file.
    # `exit 1` is not: measured through a wrapper (`wrapper.ps1` -> `& this.ps1`) it
    # leaves the wrapper's process at exit 0, while -File/-Command report 1 -- which
    # is what t4's verifier measured and what a CI wrapper would silently swallow.
    # A TERMINATING ERROR is invocation-shape independent; measured 1 via -File,
    # -Command, a wrapper WITHOUT $LASTEXITCODE propagation, a wrapper that
    # propagates, and a wrapper reached through -Command; and 0 for the ok path.
    function Fail-InstallTask {
      param([string]$Summary)
      throw [System.Management.Automation.RuntimeException]::new($Summary)
    }

    # ── PRE-FLIGHT: the action this run is about to register must be able to RUN (t35) ─
    # MEASURED BEFORE THIS EXISTED, on the DOCUMENTED command with one mistyped `-Exe`
    # (no copy involved): exit 0, `scheduled task ... registered`, and
    # `verified by read-back: ... -Exe "...\no-such-ruagent.exe" ...` -- leaving an ENABLED
    # task whose action can never start the daemon. The read-back CANNOT catch this: every
    # field it compares is built from this run's own `$Exe`, so it agrees with itself while
    # the machine is left with a watchdog that fails every five minutes and says nothing.
    #
    # WHAT IS CHECKED: that the path the action will name EXISTS and is a FILE, i.e. that
    # `watch` has something to start at all. WHAT IS DELIBERATELY NOT CHECKED: that the file
    # is the RIGHT binary -- the right build, the right architecture, or even a ruagent
    # binary. A pre-flight that answered "the Exe is usable" from a path test would be the
    # third edition of this script's historical defect (a check claiming more than it
    # verified: the all-users registration that could never succeed, and the success line
    # that depended on nothing). A stronger check would have to RUN the path (`--version`)
    # or hash it, making install-task's outcome depend on code execution and on a file a
    # rebuild can replace under the same path. The boundary is deliberate: install-task
    # refuses a path that cannot run AT ALL; a wrong-but-existing binary is a runtime
    # failure the start/watch path reports.
    #
    # PLACED HERE, AFTER `Fail-InstallTask` IS DEFINED, because PowerShell resolves a
    # function when the line RUNS: an earlier draft sat next to the task shape, called
    # `Fail-InstallTask` before it existed, and failed with `The term 'Fail-InstallTask' is
    # not recognized` -- which is non-zero under -File/-Command but measured **0 through a
    # wrapper**, exactly the invocation-shape hole the V3 comment above closed. It is still
    # before the registration, which is what the pre-flight is for.
    if (-not (Test-Path -LiteralPath $Exe -PathType Leaf)) {
      Write-Output "FAILED: -Exe '$Exe' does not exist (or is not a file), so the watchdog this would register could never start the daemon -- it would be a task that fails silently every five minutes."
      Write-Output "what to do: pass the path to a built ruagent binary, e.g. -Exe `"D:\rust_cache\debug\ruagent.exe`" -- build it with 'cargo build -p ruagent --bin ruagent' if it is missing. NOTHING was registered: the machine's scheduled task (if any) is exactly as it was, so this run cannot have broken a working watchdog."
      Fail-InstallTask "install-task FAILED: -Exe '$Exe' does not exist, so the watchdog's action could never run (nothing was registered)"
    }

    # WHAT THE MACHINE RUNS RIGHT NOW, read BEFORE the attempt (t12): this run may be
    # about to change it, and an action that changes target silently is the incident
    # this read-back exists to catch.
    $prevTask = Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
    $prevArgs = ''
    if ($prevTask) { $prevArgs = [string]@($prevTask.Actions)[0].Arguments }

    # The call below is only an ATTEMPT; the read-back is what decides the message.
    $errorsBefore = $Error.Count
    $registerError = $null
    try {
      Register-ScheduledTask -TaskName $taskName -Action $a -Trigger @($t1, $t2) -Principal $principal -Force -ErrorAction Stop | Out-Null
    } catch {
      $registerError = $_.Exception.Message
    }
    # Explicit -ErrorAction Stop is not always enough for this cmdlet (measured:
    # under `&` invocation the error stayed non-terminating), so take the engine's
    # own record of the failure rather than inventing a reason.
    if (-not $registerError -and $Error.Count -gt $errorsBefore) {
      $registerError = $Error[0].Exception.Message
      if (-not $registerError) { $registerError = $Error[0].ToString() }
    }

    $task = Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
    if (-not $task) {
      Write-Output "FAILED: scheduled task '$taskName' was NOT registered -- Get-ScheduledTask cannot find it, so nothing here protects this machine across a reboot."
      if ($registerError) { Write-Output ('reason: ' + $registerError) }
      Write-Output "what to do: re-run this from your own (non-elevated) session -- the task it installs is for $userId only, which needs no elevation, so a denial means the registration itself failed and should be reported rather than worked around by registering for all users. Act on the reason above."
      Fail-InstallTask "install-task FAILED: '$taskName' was NOT registered (Get-ScheduledTask cannot find it)$(if ($registerError) { ' -- ' + $registerError } else { '' })"
    }

    # ── The read-back: WHAT it compares, and what it deliberately does not ──────
    # COMPARED (11 checks): state; principal account (by SID key); LogonType;
    # RunLevel; trigger count; logon-trigger user (by SID key); repeat interval; and
    # -- t12 -- the ACTION: Execute, Arguments, and that the action's script is inside
    # a checkout of this repository (a copy must never become the machine's watchdog).
    #
    # THE WANTED ACTION IS ANCHORED, NOT copied from this run. Normally the wanted
    # script is this file ($PSCommandPath). But when this file is NOT the repo's copy
    # -- a temp-dir copy, which is the incident's shape -- the wanted target is the
    # script the machine's watchdog ALREADY runs (when that one is the repo's), because
    # a copy running install-task must not be able to certify that it should replace
    # the machine's watchdog: the copy's own read-back is otherwise SELF-CONSISTENT
    # (it registers the copy, then agrees with itself), which is exactly how t4's
    # verification blessed `%TEMP%\t4-scripts\ruagent-daemon.ps1`.
    #
    # NOT COMPARED, and why. An unstated omission is how this defect happened:
    #  * the action's WorkingDirectory -- this script registers none, and the action is
    #    a -File invocation of an absolute path, so a cwd cannot change what runs;
    #  * the arguments as raw bytes -- compared NORMALIZED (trim, collapse whitespace,
    #    case-insensitive): quote/spacing spelling is not behaviour, and byte equality
    #    would report a healthy task as broken after a hand-edit that means the same;
    #  * the raw account spelling ('HFLD\19410' vs '19410') -- the comparison is by SID
    #    (Get-AccountKey) and the maps print the CANONICAL spelling of the compared
    #    value, so the human diff is the comparison rather than a passing difference;
    #  * the time trigger's StartBoundary -- it is built from `Get-Date`, so it differs
    #    on every run by construction; the interval is the load-bearing half;
    #  * a script installed from a DIFFERENT legitimate checkout of this repository: it
    #    IS a repo script, so this read-back accepts it and the machine's watchdog moves
    #    to that checkout. Deliberate: refusing it would make a moved or renamed
    #    checkout uninstallable, and the target is a repo script either way. STATED
    #    rather than silent -- the case that was ever observed is the temp COPY, which
    #    is not a checkout and IS refused below;
    #  * Description/Author/Version/XML encoding -- nothing reads them and they do not
    #    change what runs.
    function Get-AccountDisplay {
      param([string]$Account)
      $key = Get-AccountKey $Account
      if (-not $key) { return '' }
      try { return ([System.Security.Principal.SecurityIdentifier]$key).Translate([System.Security.Principal.NTAccount]).Value } catch { return $key }
    }
    $p = $task.Principal
    # NB: not `$action` -- PowerShell variable names are case-insensitive, so that
    # would bind to this script's own validated `$Action` parameter and the read-back
    # would die with "MSFT_TaskExecAction is not a valid value for the Action variable"
    # AFTER registering (measured, then renamed).
    $taskAction = @($task.Actions)[0]
    $obsExec = ''
    $obsArgs = ''
    if ($taskAction) {
      $obsExec = [string]$taskAction.Execute
      $obsArgs = [string]$taskAction.Arguments
    }
    $wantExec = $actionExec
    $wantedScript = $scriptPath
    if (-not (Test-RepoScript $scriptPath)) {
      $prevScript = Get-ActionScriptPath $prevArgs
      if ($prevScript -and (Test-RepoScript $prevScript)) { $wantedScript = $prevScript }
    }
    # Built from the SAME ($actionExec, $actionArgs) pair the registration used, with
    # only the script path re-anchored (t12): a wanted shape computed independently of
    # what was registered is a second spelling that can disagree with it.
    $wantedInner = "powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File `"$wantedScript`" watch -Root `"$Root`" -Exe `"$Exe`" -Addr `"$Addr`""
    if ($useHeadless) { $wantedArg = "--headless $wantedInner" } else { $wantedArg = "-WindowStyle Hidden $wantedInner" }
    $logonTriggers = @($task.Triggers | Where-Object { $_.CimClass.CimClassName -eq 'MSFT_TaskLogonTrigger' })
    $repeatTriggers = @($task.Triggers | Where-Object { $_.CimClass.CimClassName -eq 'MSFT_TaskTimeTrigger' -and $_.Repetition.Interval })
    $logonUser = 'none'
    if ($logonTriggers.Count -gt 0) { $logonUser = $logonTriggers[0].UserId }
    $every = 'none'
    if ($repeatTriggers.Count -gt 0) { $every = $repeatTriggers[0].Repetition.Interval }
    $accountKey = Get-AccountKey $userId
    $triggerCount = @($task.Triggers).Count
    $observedMap = [ordered]@{
      state     = [string]$task.State
      principal = (Get-AccountDisplay $p.UserId)
      LogonType = [string]$p.LogonType
      RunLevel  = [string]$p.RunLevel
      triggers  = "$triggerCount"
      logon     = (Get-AccountDisplay $logonUser)
      repeat    = "$every"
      execute   = $obsExec
      arguments = $obsArgs
    }
    $wantedMap = [ordered]@{
      state     = 'Ready'
      principal = (Get-AccountDisplay $userId)
      LogonType = 'Interactive'
      RunLevel  = 'Limited'
      triggers  = '2'
      logon     = (Get-AccountDisplay $userId)
      repeat    = 'PT5M'
      execute   = $wantExec
      arguments = $wantedArg
    }
    $observed = Format-TaskShape $observedMap
    $wanted = Format-TaskShape $wantedMap

    $problems = @()
    if ($task.State -ne 'Ready' -and $task.State -ne 'Running') { $problems += "state=$($task.State)" }
    if ((Get-AccountKey $p.UserId) -ne $accountKey) { $problems += "principal=$($p.UserId)" }
    if ($p.LogonType -notin @('Interactive', 'InteractiveToken')) { $problems += "LogonType=$($p.LogonType)" }
    if ($p.RunLevel -ne 'Limited') { $problems += "RunLevel=$($p.RunLevel)" }
    if ($logonTriggers.Count -eq 0) { $problems += 'no logon trigger' }
    elseif ((Get-AccountKey $logonUser) -ne $accountKey) { $problems += "logon trigger is for $logonUser" }
    if ($every -ne 'PT5M') { $problems += "repetition=$every" }
    if ($triggerCount -ne 2) { $problems += "triggers=$triggerCount" }
    if ($obsExec -ne $wantExec) { $problems += "action execute=$obsExec" }
    $obsScript = Get-ActionScriptPath $obsArgs
    if ((Get-ArgShape $obsArgs) -ne (Get-ArgShape $wantedArg)) {
      if ($obsScript -and $wantedScript -and ($obsScript -ne $wantedScript)) {
        # The incident's shape: the action would run a DIFFERENT script from the one
        # the machine's watchdog is supposed to run.
        $problems += "action runs a DIFFERENT script: $obsScript (the watchdog should run $wantedScript)"
      } else {
        $problems += 'action arguments'
      }
    }
    # A COPY MUST NOT BECOME THE MACHINE'S WATCHDOG, whatever the task pointed at
    # before. The anchor above can only name the repo's script while the task still
    # runs it; after one copy run the task points at that copy, and without this check
    # a second copy run would agree with itself and bless it -- measured: the first
    # copy run reported the mismatch, the following ones printed `verified by read-back`
    # until this check existed.
    if ($obsScript -and -not (Test-RepoScript $obsScript)) {
      $problems += "action runs a COPY of this script, from outside any repo checkout: $obsScript"
    }

    if ($problems.Count -gt 0) {
      Write-Output "FAILED: scheduled task '$taskName' exists but is NOT the shape this script installs -- it would not keep the daemon alive as advertised."
      Write-Output ('problems: ' + ($problems -join '; '))
      Write-Output ('observed: ' + $observed)
      Write-Output ('wanted:   ' + $wanted)
      if ($registerError) { Write-Output ("reason from this run's registration attempt: " + $registerError) }
      Write-Output "what to do: inspect it (Get-ScheduledTask -TaskName $taskName), then Unregister-ScheduledTask -TaskName $taskName -Confirm:`$false and re-run install-task FROM THE REPO CHECKOUT ($wantedScript) -- the per-user shape it installs needs no elevation. If the problems line names another script, this copy must not stay the machine's watchdog."
      Fail-InstallTask "install-task FAILED: '$taskName' is not the shape this script installs ($($problems -join '; '))"
    }
    if ($registerError) {
      Write-Output "FAILED to (re)register scheduled task '$taskName': this run did NOT update it."
      Write-Output ('reason: ' + $registerError)
      Write-Output ('observed (what is there, and it is in the wanted shape): ' + $observed)
      Write-Output 'what to do: the machine is still protected by the task that is already there, but this run did not do what it was asked; fix the reason above and re-run.'
      Fail-InstallTask "install-task FAILED to (re)register '$taskName': $registerError"
    }
    $launcherText = 'powershell + -WindowStyle Hidden (Windows 10 fallback: reduced, not provably gone)'
    if ($useHeadless) { $launcherText = 'conhost --headless (no console window)' }
    Write-Output "scheduled task $taskName registered for $userId (at logon + every 5 min)"
    Write-Output "launcher: $launcherText"
    Write-Output "verified by read-back: $observed"
    # The pre-flight's own boundary, stated where the claim is made: it established that the
    # action's -Exe EXISTS, not that it is the right binary. Saying nothing would let the
    # success line read as more than it is (t35).
    Write-Output "pre-flight: the action's -Exe exists ($Exe) -- a path check only, NOT a check that it is the right binary"
  }
}