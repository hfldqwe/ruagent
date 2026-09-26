# daemon-identity.ps1 -- ONE definition of "this root's daemon".
#
# WHY A LIBRARY AND NOT A DOT-SOURCE OF THE SCRIPT: scripts/ruagent-daemon.ps1
# ends in a switch ($Action), so sourcing IT would execute an action (status by
# default, and start/stop if arguments were passed). A helper that must be
# shared cannot live inside a script that runs on load.
#
# SOURCING THIS FILE HAS NO SIDE EFFECTS: it declares two functions and nothing
# else -- no file writes, no process queries, no output.
#
# WHO READS IT: scripts/ruagent-daemon.ps1 (status/watch/stop) and
# scripts/ruagent-canary.ps1 (-Stop). Disciplines 7.86 and 7.92 need the same
# identity for cleanup and for liveness; keeping two spellings of "ours" is how
# one of them drifts.

function Get-NormalizedPath {
  param([string]$Path)
  if (-not $Path) { return '' }
  # / and \ are the same separator to Windows, a trailing one is noise,
  # and the comparison must not be case sensitive. BOTH sides of every
  # comparison go through here: a one-sided comparison reads a live daemon as
  # not-ours when the caller spells the root differently (measured, t340).
  return $Path.Trim().Replace('/', '\').TrimEnd('\').ToLowerInvariant()
}

# The identity of the process recorded in a pid file:
#   Pid, ProcessExists, NameMatches, RootMatches, Ours, Process
# Ours is the only field callers should act on. NameMatches/RootMatches are
# reported so a refusal can say WHICH half failed -- that pair is what
# separates "the pid was recycled" from "there is no process at all".
function Get-DaemonIdentity {
  param([string]$Root, [string]$PidFile)
  $id = [pscustomobject]@{
    Pid = $null; ProcessExists = $false; NameMatches = $false; RootMatches = $false; Ours = $false; Process = $null
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
  $id.Process = $proc
  $id.NameMatches = ($proc.Name -eq 'ruagent.exe')
  $cmd = ''; if ($proc.CommandLine) { $cmd = $proc.CommandLine }
  $id.RootMatches = (Get-NormalizedPath $cmd).Contains((Get-NormalizedPath $Root))
  $id.Ours = $id.NameMatches -and $id.RootMatches
  return $id
}
