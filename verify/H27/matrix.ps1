# H27 matrix: every ordered pair of claude_code, codex, opencode, pi.
# For each source session (made by verify/H27/<h>/gen-source.ps1, ids in sources.json):
#   1. txcript reads it into Common          (h27-driver dump, in the source's scratch env)
#   2. each policy variant is derived         (transform.py: full | portable | tail)
#   3. txcript writes it into the destination (h27-driver write, in the destination's scratch env)
#   4. the destination resumes it with a prompt against the mock (verify/H27/<d>/resume-capture.ps1)
#   5. analyze.py reports what the destination model received
# Rerunnable without edits. Writes verify/H27/out/2026-10-27/matrix/.
param(
  [string[]]$Variants = @('full', 'portable', 'tail'),
  [string[]]$Only = @()
)
$ErrorActionPreference = 'Stop'
# With powershell -File, "-Only a,b" arrives as one string: split it.
$Only = @($Only | ForEach-Object { $_ -split ',' } | Where-Object { $_ })
$Variants = @($Variants | ForEach-Object { $_ -split ',' } | Where-Object { $_ })
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$out = Join-Path $root 'out\2026-10-27\matrix'
$driver = if ($env:H27_DRIVER) { $env:H27_DRIVER } else { 'D:\t\h27\release\h27-driver.exe' }
New-Item -ItemType Directory -Force -Path (Join-Path $out 'common') | Out-Null
$sources = Get-Content -Raw (Join-Path $root 'out\2026-10-27\sources.json') | ConvertFrom-Json
$harnesses = @('claude_code', 'codex', 'opencode', 'pi')
$results = Join-Path $out 'results.jsonl'
if (-not $Only) { Remove-Item -ErrorAction SilentlyContinue $results }

function In-Env([string]$h, [string]$cmd) {
  # Each step in a child PowerShell so one harness's scratch variables never leak into another's.
  $envScript = Join-Path $root "$h\env.ps1"
  $full = ". '$envScript' | Out-Null; $cmd"
  if ($h -eq 'opencode') {
    # txcript spawns `opencode session import` without --standalone, which OpenCode 2.1.4 hands to
    # its background service (real home). The shim (verify/H27/shim) appends --standalone.
    $shimBin = Join-Path $env:LOCALAPPDATA 'shinatama-verify\H27\shim-bin'
    $shimLog = Join-Path $out 'opencode-shim.log'
    $full = ". '$envScript' | Out-Null; `$env:H27_OPENCODE_EXE = `$H27Exe; `$env:H27_SHIM_LOG = '$shimLog'; `$env:PATH = '$shimBin;' + `$env:PATH; $cmd"
  }
  $o = & powershell -NoProfile -ExecutionPolicy Bypass -Command $full 2>&1
  return ($o | Out-String)
}

foreach ($s in $harnesses) {
  $sid = $sources.$s
  if (-not $sid) { Write-Host "no source id for $s"; continue }
  $commonFile = Join-Path $out "common\$s.json"
  $txt = In-Env $s "& '$driver' dump $s $sid"
  [IO.File]::WriteAllText($commonFile, $txt)
}

foreach ($s in $harnesses) {
  foreach ($d in $harnesses) {
    if ($s -eq $d) { continue }
    if ($Only -and -not ($Only -contains "$s>$d")) { continue }
    foreach ($v in $Variants) {
      $tag = "$v-from-$s"
      $doc = Join-Path $out "common\$s.$v.to-$d.json"
      $dwork = Join-Path $env:LOCALAPPDATA "shinatama-verify\H27\$d\work"
      & python -I (Join-Path $root 'transform.py') (Join-Path $out "common\$s.json") $v $doc --cwd $dwork | Out-Null
      $w = In-Env $d "& '$driver' write $d '$doc'"
      $line = ($w -split "`n" | Where-Object { $_ -match '^\{"id"' } | Select-Object -First 1)
      $rec = @{ source = $s; dest = $d; variant = $v; write = $w.Trim() }
      if ($line) {
        $id = ($line | ConvertFrom-Json).id
        $rec.id = $id
        $prompt = "H27-RESUME [$tag] what did we do so far?"
        $rc = In-Env $d "& '$(Join-Path $root "$d\resume-capture.ps1")' -SessionId '$id' -Prompt '$prompt' -Tag '$tag'"
        $rec.resume = ($rc.Trim() -split "`n" | Select-Object -Last 6) -join "`n"
        $cap = Join-Path $root "out\2026-10-27\$d\resume\$tag.capture.jsonl"
        if (Test-Path $cap) {
          $a = & python -I (Join-Path $root 'analyze.py') $cap --json
          $rec.analysis = ($a | Out-String).Trim()
        }
      }
      ($rec | ConvertTo-Json -Compress -Depth 4) | Add-Content -Encoding UTF8 $results
      Write-Host "$s -> $d [$v]: $($rec.id)"
    }
  }
}
Write-Host "results: $results"
