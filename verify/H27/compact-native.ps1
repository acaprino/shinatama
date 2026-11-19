# H27: the destination's OWN compaction on a session txcript wrote into it ("compact between hops",
# native route), then a resume with a prompt. Proves that an imported session can be compacted by the
# destination harness and what the model receives afterwards.
#   powershell -NoProfile -ExecutionPolicy Bypass -File verify\H27\compact-native.ps1 -Harness <h> -SessionId <id> -Tag <name>
# Writes verify/H27/out/2026-10-27/<h>/compact/<Tag>.* and the resume capture <h>/resume/<Tag>-after.capture.jsonl.
param(
  [Parameter(Mandatory = $true)][ValidateSet('claude_code', 'codex', 'opencode', 'pi')][string]$Harness,
  [Parameter(Mandatory = $true)][string]$SessionId,
  [Parameter(Mandatory = $true)][string]$Tag
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
. (Join-Path $root "$Harness\env.ps1") | Out-Null
if ($Harness -eq 'pi') { . (Join-Path $root 'pi\lib.ps1') | Out-Null }
$outDir = Join-Path $root "out\2026-10-27\$Harness\compact"
New-Item -ItemType Directory -Force -Path $outDir | Out-Null
$base = Join-Path $outDir $Tag
$capture = "$base.capture.jsonl"
Remove-Item "$base.*" -Force -ErrorAction SilentlyContinue

switch ($Harness) {
  'claude_code' {
    $srv = Start-H27Servers $capture "$base.trap.jsonl"
    try { $r = Invoke-H27Claude @('-p', '--resume', $SessionId, '/compact', '--output-format', 'json', '--dangerously-skip-permissions') $H27Work $base 300 }
    finally { Stop-H27Servers $srv }
    "compact: exit=$($r.exit)"
  }
  'codex' {
    Start-H27Mock -Capture $capture -EgressLog "$base.egress.jsonl" | Out-Null
    try {
      $ErrorActionPreference = 'Continue'
      & node (Join-Path $root 'codex\app-server-drive.mjs') --codex $H27Codex --thread $SessionId --cwd $H27Work --log "$base.app-server.jsonl" --action compact 2>&1 | ForEach-Object { "$_" } | Set-Content -Encoding UTF8 "$base.out.txt"
      "compact: exit=$LASTEXITCODE"
      $ErrorActionPreference = 'Stop'
    } finally { Stop-H27Mock }
  }
  'opencode' {
    $mock = Start-H27Mock $capture
    try {
      $serve = Start-H27Serve "$base.serve"
      "compact: $(Invoke-H27Http -Method POST -Path "/api/session/$SessionId/compact" -Body '{}' -OutFile "$base.compact.json")"
      "wait   : $(Invoke-H27Http -Method POST -Path "/api/experimental/session/$SessionId/wait" -OutFile "$base.wait.json" -TimeoutSec 180)"
      "export : $(Invoke-H27Http -Method GET -Path "/api/experimental/session/$SessionId/export" -OutFile "$base.export.json")"
      Stop-H27Process $serve
      Start-Sleep -Seconds 1
    } finally { Stop-H27Process $mock }
  }
  'pi' {
    $mock = Start-H27Mock $capture
    try {
      $r = Invoke-H27PiRpc ($H27PiFlags + @('--session', $SessionId)) $H27PiWork @(
        @{ Send = '{"id":"c1","type":"compact"}'; WaitFor = '"id":"c1".*"type":"response"|"type":"response".*"id":"c1"' })
      Save-H27Run $r $base
      "compact: exit=$($r.ExitCode) completed=$($r.Completed)"
    } finally { Stop-H27Mock $mock }
  }
}
"compaction requests: $((Get-Content $capture -ErrorAction SilentlyContinue | Measure-Object).Count)"

# Resume with a prompt, in a fresh process with the harness's own resume script.
$prompt = "H27-RESUME [$Tag-after] what did we do so far?"
& powershell -NoProfile -ExecutionPolicy Bypass -Command ". '$root\$Harness\env.ps1' | Out-Null; & '$root\$Harness\resume-capture.ps1' -SessionId '$SessionId' -Prompt '$prompt' -Tag '$Tag-after'" 2>&1 | Select-Object -Last 4
$cap = Join-Path $root "out\2026-10-27\$Harness\resume\$Tag-after.capture.jsonl"
& python -I (Join-Path $root 'analyze.py') $cap --json
