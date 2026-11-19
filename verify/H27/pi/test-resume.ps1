# H27 / Pi - exercise resume-capture.ps1 on the sessions gen-source.ps1 left in the scratch home.
# Run after gen-source.ps1. Rerunnable without edits (each run replaces resume/<tag>.*):
#   powershell -NoProfile -ExecutionPolicy Bypass -File verify/H27/pi/test-resume.ps1
#   source-T5        the T1-T4 source session (compaction, then a killed T4 left a dangling user turn)
#   fork-abort       the abort-variant fork (an assistant entry with stopReason "aborted")
#   source-othercwd  the source session again, Pi started in another folder
#   synthetic        a hand-written 3-line session (header, user, assistant with a foreign-model
#                    thinking block), the shape an importer would write
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\env.ps1"
$resume = Join-Path $PSScriptRoot 'resume-capture.ps1'
$idFile = Join-Path $H27PiOut 'source\session-id.txt'
if (-not (Test-Path $idFile)) { throw "no $idFile; run gen-source.ps1 first" }
$sid = (Get-Content $idFile -TotalCount 1).Trim()
$forkFile = Get-ChildItem (Join-Path $H27PiOut 'source\abort-variant') -Filter '*.jsonl' | Select-Object -First 1
$forkId = (Get-Content $forkFile.FullName -TotalCount 1 | ConvertFrom-Json).id

# Synthetic session written straight into the scratch home's project folder.
$proj = Get-ChildItem (Join-Path $H27PiHome 'sessions') -Directory | Select-Object -First 1
$syn = [ordered]@{ type = 'session'; version = 3; id = 'h27-synthetic-minimal'; timestamp = '2026-10-27T16:00:00.000Z'; cwd = $H27PiWork }
$u = [ordered]@{ type = 'message'; id = 'aaaa0001'; parentId = $null; timestamp = '2026-10-27T16:00:01.000Z'
  message = [ordered]@{ role = 'user'; content = @([ordered]@{ type = 'text'; text = 'H27-SYN first user turn' }); timestamp = 1793116801000 } }
$a = [ordered]@{ type = 'message'; id = 'aaaa0002'; parentId = 'aaaa0001'; timestamp = '2026-10-27T16:00:02.000Z'
  message = [ordered]@{ role = 'assistant'
    content = @([ordered]@{ type = 'thinking'; thinking = 'H27-SYN thinking'; thinkingSignature = 'h27-syn-signature' }, [ordered]@{ type = 'text'; text = 'H27-SYN first answer' })
    api = 'anthropic-messages'; provider = 'anthropic'; model = 'claude-sonnet-4-5'
    usage = [ordered]@{ input = 1; output = 1; cacheRead = 0; cacheWrite = 0; totalTokens = 2; cost = [ordered]@{ input = 0; output = 0; cacheRead = 0; cacheWrite = 0; total = 0 } }
    stopReason = 'stop'; timestamp = 1793116802000 } }
$lines = @($syn, $u, $a) | ForEach-Object { $_ | ConvertTo-Json -Depth 10 -Compress }
Write-H27Utf8 (Join-Path $proj.FullName '2026-10-27T16-00-00-000Z_h27-synthetic-minimal.jsonl') (($lines -join "`n") + "`n")

$other = Join-Path $H27PiRoot 'othercwd'
$runs = @(
  @{ Id = $sid; Prompt = 'H27-RESUME continue please'; Tag = 'source-T5' },
  @{ Id = $forkId; Prompt = 'H27-RESUME after abort'; Tag = 'fork-abort' },
  @{ Id = $sid; Prompt = 'H27-RESUME from another cwd'; Tag = 'source-othercwd'; Cwd = $other },
  @{ Id = 'h27-synthetic-minimal'; Prompt = 'H27-RESUME synthetic minimal'; Tag = 'synthetic' }
)
foreach ($r in $runs) {
  $a = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $resume, '-SessionId', $r.Id, '-Prompt', $r.Prompt, '-Tag', $r.Tag)
  if ($r.Cwd) { $a += @('-Cwd', $r.Cwd) }
  & powershell.exe @a
  Write-Host ("[{0}] exit={1}" -f $r.Tag, $LASTEXITCODE)
}
