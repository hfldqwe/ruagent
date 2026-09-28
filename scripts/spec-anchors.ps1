# spec-anchors.ps1 -- coordinate trust for the gen2 specs (t85, from the t80 D-2 audit).
#
# WHY: the four gen2 specs cite code as `crates/.../file.rs:NNN`. Those line numbers
# drift as the code moves; a drifted coordinate is worse than no coordinate, because
# the next reader opens the cited line, reads unrelated code, and concludes from it.
# This checker turns "the coordinates are believable" into a property: every cited
# line must carry an ANCHOR WORD taken from the citing sentence.
#
# WHAT COUNTS AS A REFERENCE
#   * full path      `crates/knowledge/src/store.rs:610`  (and `:659` following it on
#                    the same line -- a table row often cites one file, then bare lines)
#   * bare basename  `api.rs:3730`  -- resolved against the repo's own file index;
#                    an ambiguous basename is reported UNRESOLVED, never guessed
#   * a range        `...rs:85-150`  -- the START line is checked (the second number is
#                    a span, not a claim about one line)
#
# WHAT COUNTS AS AN ANCHOR -- and why the first version of this script was wrong.
#   A candidate anchor is an identifier-ish token from the citing line (backticked
#   spans first, then plain identifiers >= 3 chars, minus stopwords). A candidate is
#   STRONG only if it occurs on at most MaxRarity lines of the TARGET file. Without
#   that rarity rule the checker reported 122 "drifts": words like `knowledge`, `store`,
#   `wiki`, `SELECT` occur everywhere, so "the anchor lives elsewhere in the file" said
#   nothing. A gate that cries wolf gets switched off -- so generic words are not
#   anchors, and a reference with no strong anchor is UNVERIFIED (printed and counted),
#   never silently passed and never called a drift.
#   * PASS when a strong anchor occurs in [N-Window, N+Window]
#   * DRIFT when strong anchors exist but none is in the window (prints the nearest
#     line that carries one -- the suggested new coordinate)
#   * UNVERIFIED when no candidate is strong (the sentence cites a concept, or every
#     token it names is generic)
#
# exit code: 1 on any DRIFT or OUT-OF-RANGE. UNVERIFIED / UNRESOLVED are printed and
# counted but do not fail: they need a human ruling (an ambiguous basename is a spec
# defect, but not a coordinate that lies).
#
# usage:
#   scripts/spec-anchors.ps1                 # check the four specs
#   scripts/spec-anchors.ps1 -Suggest        # only the old -> new mapping (for errata)
#   scripts/spec-anchors.ps1 -Spec <path>    # check other spec files too
#   scripts/spec-anchors.ps1 -Window 4       # widen the pass window (default 2)
#   scripts/spec-anchors.ps1 -MaxRarity 6    # anchors may occur on up to 6 lines
[CmdletBinding()]
param(
  [string[]]$Spec,
  [switch]$Suggest,
  [int]$Window = 3,
  [int]$MaxRarity = 3
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
if ([string]::IsNullOrEmpty($root)) { $root = (Get-Location).Path }

$specDir = Join-Path $root 'docs/design/reviews'
if (-not $Spec -or $Spec.Count -eq 0) {
  $Spec = @('gen2-recall-spec.md', 'gen2-memory-spec.md', 'gen2-graph-spec.md', 'gen2-wiki-spec.md') |
    ForEach-Object { Join-Path $specDir $_ }
}

# ---- the repo's .rs index, for bare-basename references ---------------------------
$rsIndex = @{}
$rsFiles = Get-ChildItem -LiteralPath $root -Recurse -File -Filter *.rs -ErrorAction SilentlyContinue |
  Where-Object { $_.FullName -notmatch '\\target\\|\\.git\\' }
foreach ($f in $rsFiles) {
  $k = $f.Name.ToLowerInvariant()
  if (-not $rsIndex.ContainsKey($k)) { $rsIndex[$k] = @() }
  $rsIndex[$k] += $f.FullName
}

$stop = @{}
'crates src tests test docs doc file files line lines the and for with from that this true false
none some self impl pub fn let use mod struct enum trait const static mut ref where async await
return match type new default json http https rust rustfmt clippy cargo usize isize string vec
option result error ok err daemon api spec row rows table hit hits gold id ids ts ms db sql
md css html node npm git main lib rs not all any one two three four five six seven eight nine ten
must shall will should can may only same each other when then than there here where which who
what why how does did was were are is be been being have has had doing
limit offset order desc asc select from where join add off on check int integer text blob null
key primary group having union values set index table alter create insert update delete into
begin commit rollback true false number bool float double char varchar real
lancedb graphiti hipporag graphrag mcp rrf tauri react vite antd jsonl toml yaml csv
and or if else loop while do end start now utc json ok fail pass note todo fixme
row col cell true false
debug_assert to_lowercase unwrap_or unwrap_or_default to_string as_str into_iter map_err
expect assert println eprintln format write read open close flush clone default
some none ok_err map_or filter_map collect insert update delete select_ok' -split '\s+' |
  ForEach-Object { if ($_) { $stop[$_.ToLowerInvariant()] = $true } }

function Test-CodeShaped([string]$t) {
  if ($t.Length -lt 3) { return $false }
  if ($t.Contains('_')) { return $true }              # snake_case / CONST_CASE
  if ($t -cmatch '^[A-Z]{3,}$') { return $true }      # ALLCAPS
  if ($t -cmatch '^[A-Z][a-z0-9]+[A-Z]') { return $true }   # CamelCase, >= 2 humps
  if ($t -cmatch '^[A-Z][a-z]+$') { return $true }    # Pascal, 1 hump (type names)
  if ($t -cmatch '^[a-z]+[A-Z]') { return $true }     # camelCase
  return $false
}

function Get-Candidates([string]$line) {
  # Two candidate families, strongest first:
  #   Quoted -- a literal in double quotes (`"score_kind"`): an exact string, so if it
  #             occurs in the target file it is almost certainly the line meant.
  #   Tokens -- code-shaped identifiers (snake_case / CONST_CASE / CamelCase). An
  #             English word ("knowledge", "store", "inject", "chunk") is NOT a
  #             candidate: it occurs all over the file, so "the anchor is elsewhere"
  #             would be noise, and this gate must not cry wolf.
  $quoted = New-Object System.Collections.Generic.HashSet[string]
  foreach ($m in ($line | Select-String -Pattern '"([^"\r\n]{4,})"' -AllMatches).Matches) {
    # only identifier-shaped literals ("score_kind"); a format placeholder such as
    # "{stage:?}" or a prose fragment is not an anchor.
    $v = $m.Groups[1].Value
    if ($v.Length -ge 8 -and $v -match '^[A-Za-z_][A-Za-z0-9_]*$') { [void]$quoted.Add($v) }
  }
  $set = New-Object System.Collections.Generic.HashSet[string]
  $backticked = New-Object System.Collections.Generic.HashSet[string]
  foreach ($m in ($line | Select-String -Pattern '`([^`]+)`' -AllMatches).Matches) {
    foreach ($t in ($m.Groups[1].Value | Select-String -Pattern '[A-Za-z_][A-Za-z0-9_]{2,}' -AllMatches).Matches) { [void]$set.Add($t.Value); [void]$backticked.Add($t.Value) }
  }
  foreach ($t in ($line | Select-String -Pattern '[A-Za-z_][A-Za-z0-9_]{2,}' -AllMatches).Matches) { [void]$set.Add($t.Value) }
  $out = @()
  foreach ($a in $set) {
    if ($a.Length -lt 4) { continue }
    if (-not (Test-CodeShaped $a)) { continue }
    if ($stop.ContainsKey($a.ToLowerInvariant())) { continue }
    if ($a -match '^(crates|gen2|t\d+)') { continue }
    $out += $a
  }
  return [pscustomobject]@{ Quoted = @($quoted); Tokens = $out; Backticked = @($backticked) }
}

# token -> line numbers, per target file (built once); plus the DEFINITION index
$tokIdx = @{}
$defIdx = @{}
$litIdx = @{}
function Get-TokenIndex([string]$path) {
  if ($tokIdx.ContainsKey($path)) { return $tokIdx[$path] }
  $idx = @{}
  $defs = @{}
  $lits = @{}
  $ls = @(Get-Content -LiteralPath $path -ErrorAction SilentlyContinue)
  for ($i = 0; $i -lt $ls.Count; $i++) {
    $l = $ls[$i]
    foreach ($t in ($l | Select-String -Pattern '[A-Za-z_][A-Za-z0-9_]{2,}' -AllMatches).Matches) {
      $k = $t.Value
      if (-not $idx.ContainsKey($k)) { $idx[$k] = New-Object System.Collections.Generic.List[int] }
      $idx[$k].Add($i + 1)
    }
    foreach ($m in ($l | Select-String -Pattern '"([^"\r\n]{4,})"' -AllMatches).Matches) {
      $k = $m.Groups[1].Value
      if (-not $lits.ContainsKey($k)) { $lits[$k] = New-Object System.Collections.Generic.List[int] }
      $lits[$k].Add($i + 1)
    }
    # a DEFINITION of a symbol is the strongest possible anchor: "the spec cites
    # recall_stubs" is judged by where `fn recall_stubs` actually is.
    if ($l -match '(?:^|[\s;(])(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?(?:fn|struct|enum|const|static|trait|type|union|mod)\s+([A-Za-z_][A-Za-z0-9_]*)') {
      $n = $Matches[1]
      if (-not $defs.ContainsKey($n)) { $defs[$n] = New-Object System.Collections.Generic.List[int] }
      $defs[$n].Add($i + 1)
    }
  }
  $tokIdx[$path] = $idx
  $defIdx[$path] = $defs
  $litIdx[$path] = $lits
  return $idx
}

$refRe = 'crates[/\\][A-Za-z0-9_\-./]+\.rs:\d+(?:-\d+)?'
$bareRe = '(?<![A-Za-z0-9_/\\.\-])[A-Za-z0-9_\-]+\.rs:\d+(?:-\d+)?'

$pass = 0; $drift = 0; $suspect = 0; $unver = 0; $unres = 0
$report = New-Object System.Collections.Generic.List[string]
$suggestions = New-Object System.Collections.Generic.List[string]

foreach ($specPath in $Spec) {
  if (-not (Test-Path -LiteralPath $specPath)) { Write-Host "spec-anchors: MISSING SPEC $specPath"; exit 2 }
  $lines = @(Get-Content -LiteralPath $specPath)
  $specName = Split-Path -Leaf $specPath
  # An errata section QUOTES old coordinates on purpose (that is the point of keeping
  # the old value). Its references are skipped and counted, never judged, never silently.
  $inErrata = $false
  $errataQuotes = 0
  for ($i = 0; $i -lt $lines.Count; $i++) {
    $line = $lines[$i]
    if ($line -match '^##\s') { $inErrata = ($line -match '^##\s+.*勘误') }
    if ($inErrata) {
      if ($line -match '\.rs:\d') { $errataQuotes += ($line | Select-String -Pattern 'crates[/\\][A-Za-z0-9_\-./]+\.rs:\d+' -AllMatches).Matches.Count }
      continue
    }
    if ($line -notmatch '\.rs:\d') { continue }

    $refs = @()
    foreach ($m in ($line | Select-String -Pattern $refRe -AllMatches).Matches) { $refs += $m.Value }
    foreach ($m in ($line | Select-String -Pattern $bareRe -AllMatches).Matches) { $refs += $m.Value }
    $seen = @{}
    $refs = $refs | Where-Object { -not $seen.ContainsKey($_) -and ($seen[$_] = $true) }

    $cand = Get-Candidates $line
    foreach ($r in $refs) {
      $idx = $r.LastIndexOf(':')
      $pathPart = $r.Substring(0, $idx)
      $n = [int](($r.Substring($idx + 1)) -split '-')[0]

      $target = $null
      if ($pathPart -match '[/\\]') {
        $cand = Join-Path $root ($pathPart -replace '/', '\')
        if (Test-Path -LiteralPath $cand) { $target = $cand } else { $cand2 = Join-Path $root $pathPart; if (Test-Path -LiteralPath $cand2) { $target = $cand2 } }
      } else {
        $cands = $rsIndex[$pathPart.ToLowerInvariant()]
        if ($cands -and $cands.Count -eq 1) { $target = $cands[0] }
        elseif ($cands -and $cands.Count -gt 1) {
          $unres++; $report.Add("UNRESOLVED $specName`:$($i+1)  $r  (basename matches $($cands.Count) files -- cite the crate)"); continue
        }
      }
      if (-not $target) { $unres++; $report.Add("UNRESOLVED $specName`:$($i+1)  $r  (no such file under the repo root)"); continue }

      $ti = Get-TokenIndex $target
      $fileLines = @(Get-Content -LiteralPath $target -ErrorAction SilentlyContinue)
      if ($fileLines.Count -eq 0) { $unres++; $report.Add("UNRESOLVED $specName`:$($i+1)  $r  (unreadable)"); continue }

      if ($n -lt 1 -or $n -gt $fileLines.Count) {
        $drift++
        $report.Add("DRIFT  $specName`:$($i+1)  $r  line $n OUT OF RANGE (file has $($fileLines.Count) lines)")
        $suggestions.Add("$specName`:$($i+1)|$r|out-of-range")
        continue
      }

      # STRONG anchors: a quoted literal that occurs in the file, or an identifier the
      # file DEFINES. Both are unambiguous, so a miss is a real drift.
      # WEAK anchors: a rare code-shaped identifier used (not defined) in the file.
      # A weak miss is SUSPECT -- reported for a human, never used to fail the gate.
      $defs = $defIdx[$target]
      $lits = $litIdx[$target]
      $strong = @()
      foreach ($q in $cand.Quoted) {
        if ($lits.ContainsKey($q)) { if ($lits[$q].Count -le $MaxRarity) { $strong += [pscustomobject]@{ Anchor = $q; Lines = $lits[$q]; Kind = 'literal' } } }
      }
      $weak = @()
      foreach ($a in $cand.Tokens) {
        $quotedByAuthor = ($cand.Backticked -contains $a)
        if ($quotedByAuthor -and $defs.ContainsKey($a) -and $ti[$a].Count -le $MaxRarity) {
          # The spec put the symbol in backticks next to the coordinate AND the target
          # file DEFINES that symbol: that combination is unambiguous -- the coordinate
          # is a claim about where that definition is. A miss here is a real drift.
          $strong += [pscustomobject]@{ Anchor = $a; Lines = $ti[$a]; Kind = 'definition' }
          continue
        }
        if ($ti.ContainsKey($a) -and $ti[$a].Count -le $MaxRarity) { $weak += [pscustomobject]@{ Anchor = $a; Lines = $ti[$a]; Kind = 'use' } }
      }

      if ($strong.Count -eq 0 -and $weak.Count -eq 0) {
        $unver++
        $sample = (($cand.Quoted + $cand.Tokens) | Select-Object -First 4) -join ', '
        $report.Add("UNVERIFIED $specName`:$($i+1)  $r  (no distinctive anchor in the target; candidates: $sample)")
        continue
      }

      $lo = [Math]::Max(1, $n - $Window); $hi = [Math]::Min($fileLines.Count, $n + $Window)
      $hit = $null
      foreach ($s in ($strong + $weak)) { foreach ($l in $s.Lines) { if ($l -ge $lo -and $l -le $hi) { $hit = $s; break } }; if ($hit) { break } }
      if ($hit) { $pass++; continue }

      $pick = if ($strong.Count -gt 0) { $strong } else { $weak }
      $bestA = $null; $bestL = 0; $bestD = [int]::MaxValue; $bestK = ''
      foreach ($s in $pick) {
        foreach ($l in $s.Lines) {
          $d = [Math]::Abs($l - $n)
          if ($d -lt $bestD) { $bestD = $d; $bestA = $s.Anchor; $bestL = $l; $bestK = $s.Kind }
        }
      }
      if ($strong.Count -gt 0) {
        $drift++
        $report.Add("DRIFT  $specName`:$($i+1)  $r  -> $bestK '$bestA' lives at :$bestL")
        $suggestions.Add("$specName`:$($i+1)|$r|$pathPart`:$bestL|$bestK $bestA")
      } else {
        $suspect++
        $report.Add("SUSPECT $specName`:$($i+1)  $r  -> only a use-site anchor; '$bestA' lives at :$bestL (human ruling needed)")
      }
    }
  }
}

$total = $pass + $drift + $unver
if ($Suggest) {
  $suggestions | ForEach-Object { $_ }
  Write-Host ""
  Write-Host "spec-anchors: refs=$total drift=$drift unverified=$unver unresolved=$unres (suggest only)"
  if ($drift -gt 0) { exit 1 }
  exit 0
}

$report | ForEach-Object { Write-Host $_ }
Write-Host ""
Write-Host "spec-anchors: refs=$total  pass=$pass  drift=$drift  suspect=$suspect  unverified=$unver  unresolved=$unres  window=+-$Window  max-rarity=$MaxRarity"
if ($drift -gt 0) {
  Write-Host "spec-anchors: FAIL -- $drift coordinate(s) point at a line that does not carry the cited thing (strong anchor: a quoted literal, or a symbol the target defines)."
  exit 1
}
Write-Host "spec-anchors: OK -- no coordinate contradicts a strong anchor from its citing sentence. SUSPECT/UNVERIFIED rows still need a human ruling; they are listed above and counted, never silently skipped."
exit 0
