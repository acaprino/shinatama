# OC2 - OpenCode session interchange: export a synthetic session, import it into a second scratch DB,
# compare the round trip, check the 409 on an existing id, exercise the CLI import/export surface, and
# import a rich crafted transcript (user, assistant+tool, compaction, system, synthetic) to see what
# survives a round trip. Scratch only (OPENCODE_DB + XDG_* under LOCALAPPDATA, local scratch password);
# no login; no model calls (synthetic messages stored with resume=false). Rerunnable without edits.
param([string]$Date = (Get-Date -Format 'yyyy-MM-dd'))
$ErrorActionPreference = 'Continue'
$out = Join-Path $PSScriptRoot "out\$Date"
New-Item -ItemType Directory -Force -Path $out | Out-Null
$s = Join-Path $env:LOCALAPPDATA 'shinatama-verify\OC2'
Remove-Item $s -Recurse -Force -ErrorAction SilentlyContinue
$projA = Join-Path $s 'projA'
$projC = Join-Path $s 'projC'
$dbA = Join-Path $s 'A'
$dbB = Join-Path $s 'B'
$dbC = Join-Path $s 'C'
foreach ($d in @($projA, $projC, $dbA, $dbB, $dbC)) { New-Item -ItemType Directory -Force -Path $d | Out-Null }
$utf8 = New-Object System.Text.UTF8Encoding($false)
$pw = 'stx-oc2-local-test'
$authArgs = @('-H', ('Authorization: Basic ' + [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes("opencode:$pw"))))
function Write-Body($path, $text) { [IO.File]::WriteAllText($path, $text, $utf8) }
function EnvFor($base) {
  @{ OPENCODE_DB = (Join-Path $base 'opencode.db'); XDG_DATA_HOME = $base; XDG_CONFIG_HOME = (Join-Path $base 'config'); XDG_STATE_HOME = (Join-Path $base 'state'); OPENCODE_PASSWORD = $pw }
}
function PortFree($p) { -not (Get-NetTCPConnection -LocalPort $p -State Listen -ErrorAction SilentlyContinue) }
$portA = 43117; $portB = 43118
if (-not (PortFree $portA)) { $portA = 43127 }
if (-not (PortFree $portB)) { $portB = 43128 }

function Wait-Http($url, $seconds) {
  $deadline = (Get-Date).AddSeconds($seconds)
  $code = ''
  while ((Get-Date) -lt $deadline) {
    $code = (curl.exe -s -o NUL -w "%{http_code}" @authArgs $url) 2>$null
    if ($code -eq '200') { return '200' }
    Start-Sleep -Milliseconds 700
  }
  return "timeout($code)"
}
function Post-Json($url, $json, $respFile) {
  $bf = Join-Path $env:TEMP ('oc2-' + [guid]::NewGuid().ToString('N') + '.json')
  Write-Body $bf $json
  $code = (curl.exe -s -o $respFile -w "%{http_code}" @authArgs -H "Content-Type: application/json" --data-binary "@$bf" $url) 2>$null
  Remove-Item $bf -Force -ErrorAction SilentlyContinue
  return "$code"
}
function Get-Json($url, $respFile) {
  (curl.exe -s -o $respFile -w "%{http_code}" @authArgs $url) 2>$null
}
function Invoke-Cli($label, $argsArr, $envs, $cwd, $seconds, $logFile) {
  $job = Start-Job -ScriptBlock {
    param($a, $e, $c)
    foreach ($k in $e.Keys) { Set-Item -Path ("env:" + $k) -Value $e[$k] }
    Set-Location $c
    ((& opencode @a 2>&1 | Out-String) + "`nexit=$LASTEXITCODE")
  } -ArgumentList $argsArr, $envs, $cwd
  if (Wait-Job $job -Timeout $seconds) {
    $o = Receive-Job $job
    Set-Content -Path $logFile -Value $o -Encoding utf8
    "$label : ok"
  } else {
    Stop-Job $job
    Set-Content -Path $logFile -Value "TIMEOUT >$seconds s" -Encoding utf8
    "$label : TIMEOUT"
  }
  Remove-Job $job -Force
}
function Headline($file, $chars) {
  if (-not (Test-Path $file)) { return '<missing>' }
  $raw = (Get-Content $file -Raw); if ($null -eq $raw) { return '<empty>' }
  $t = ($raw -replace '\s+',' ')
  if ($t.Length -gt $chars) { $t = $t.Substring(0, $chars) }
  $t
}
function MessageSummary($file) {
  if (-not (Test-Path $file)) { return '<missing>' }
  $raw = Get-Content $file -Raw
  if (-not $raw) { return '<empty>' }
  try { $j = $raw | ConvertFrom-Json } catch { return '<not json>' }
  $msgs = $j.data.messages
  if ($null -eq $msgs) { $msgs = $j.messages }
  if ($null -eq $msgs) { return '<no messages>' }
  $types = @($msgs | ForEach-Object { $_.type }) -join ','
  "messages=$(@($msgs).Count) types=[$types]"
}

$sid = 'ses_oc2' + (Get-Date -Format 'HHmmss')
"session id (chosen): $sid"
"ports: A=$portA B=$portB"

# ---------- Phase 1: server A (scratch DB A) ----------
$jobA = Start-Job -ScriptBlock {
  param($envs, $cwd, $port)
  foreach ($k in $envs.Keys) { Set-Item -Path ("env:" + $k) -Value $envs[$k] }
  Set-Location $cwd
  & opencode serve --hostname 127.0.0.1 --port $port
} -ArgumentList (EnvFor $dbA), $projA, $portA
"phase1: server A ready = " + (Wait-Http "http://127.0.0.1:$portA/api/session" 60)

$body = @{ id = $sid; title = 'OC2 synthetic session' } | ConvertTo-Json -Compress
$createCode = Post-Json "http://127.0.0.1:$portA/api/session" $body (Join-Path $out 'create-a.json')
"phase1: create session -> $createCode"
if ($createCode -ne '200') {
  "phase1: retrying without a chosen id"
  $createCode = Post-Json "http://127.0.0.1:$portA/api/session" (@{ title = 'OC2 synthetic session' } | ConvertTo-Json -Compress) (Join-Path $out 'create-a.json')
  $sid = (Get-Content (Join-Path $out 'create-a.json') -Raw | ConvertFrom-Json).data.id
  "phase1: create session (server id) -> $createCode, id=$sid"
}

$syn1 = @{ text = 'STX-OC2-MARKER turn one: user text'; resume = $false } | ConvertTo-Json -Compress
$syn2 = @{ text = 'STX-OC2-MARKER turn two: second text'; resume = $false } | ConvertTo-Json -Compress
"phase1: synthetic 1 (resume=false) -> " + (Post-Json "http://127.0.0.1:$portA/api/session/$sid/synthetic" $syn1 (Join-Path $out 'synthetic-1.json'))
"phase1: synthetic 2 (resume=false) -> " + (Post-Json "http://127.0.0.1:$portA/api/session/$sid/synthetic" $syn2 (Join-Path $out 'synthetic-2.json'))

$shell = @{ command = 'echo STX-OC2-MARKER shell' } | ConvertTo-Json -Compress
"phase1: shell                     -> " + (Post-Json "http://127.0.0.1:$portA/api/session/$sid/shell" $shell (Join-Path $out 'shell.json'))

"phase1: export A (http) -> " + (Get-Json "http://127.0.0.1:$portA/api/experimental/session/$sid/export" (Join-Path $out 'export-a.json'))

Stop-Job $jobA; Receive-Job $jobA | ForEach-Object { $_ -replace [regex]::Escape($pw), '<scratch-password>' } | Set-Content -Path (Join-Path $out 'server-a.log') -Encoding utf8; Remove-Job $jobA -Force

# ---------- Phase 2: CLI export from A, CLI import into C ----------
Invoke-Cli 'phase2: cli export A' @('session','export',$sid,'--standalone') (EnvFor $dbA) $projA 150 (Join-Path $out 'export-a-cli.out.txt')
$cliFile = Join-Path $out 'export-a-cli.json'
$rawCli = Get-Content (Join-Path $out 'export-a-cli.out.txt') -Raw
$start = $rawCli.IndexOf('{'); $end = $rawCli.LastIndexOf('}')
if ($start -ge 0 -and $end -gt $start) { [IO.File]::WriteAllText($cliFile, $rawCli.Substring($start, $end - $start + 1), $utf8); 'phase2: extracted JSON from CLI output to export-a-cli.json' } else { 'phase2: CLI export produced no JSON' }

Invoke-Cli 'phase2: cli import C' @('session','import',$cliFile,'--directory',$projC,'--standalone') (EnvFor $dbC) $projC 150 (Join-Path $out 'import-c-cli.out.txt')
Invoke-Cli 'phase2: cli export C' @('session','export',$sid,'--standalone') (EnvFor $dbC) $projC 150 (Join-Path $out 'export-c-cli.out.txt')

# ---------- Phase 3: server B (scratch DB B): import -> export -> compare -> 409 ----------
$jobB = Start-Job -ScriptBlock {
  param($envs, $cwd, $port)
  foreach ($k in $envs.Keys) { Set-Item -Path ("env:" + $k) -Value $envs[$k] }
  Set-Location $cwd
  & opencode serve --hostname 127.0.0.1 --port $port
} -ArgumentList (EnvFor $dbB), $projA, $portB
"phase3: server B ready = " + (Wait-Http "http://127.0.0.1:$portB/api/session" 60)

$expA = Get-Content (Join-Path $out 'export-a.json') -Raw | ConvertFrom-Json
$importBody = ($expA.data | ConvertTo-Json -Depth 100 -Compress)
$impCode = Post-Json "http://127.0.0.1:$portB/api/experimental/session/import" $importBody (Join-Path $out 'import-b.json')
"phase3: import A -> B -> $impCode"

"phase3: export B (http) -> " + (Get-Json "http://127.0.0.1:$portB/api/experimental/session/$sid/export" (Join-Path $out 'export-b.json'))

$imp2Code = Post-Json "http://127.0.0.1:$portB/api/experimental/session/import" $importBody (Join-Path $out 'import-b-again-409.json')
"phase3: import A -> B again -> $imp2Code (expect 409)"

# ---------- Phase 4: rich crafted fixture -> import into B -> export and compare ----------
$richId = 'ses_oc2rich' + (Get-Date -Format 'HHmmss')
$now = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
$infoRich = @{
  id = $richId
  projectID = $expA.data.info.projectID
  cost = 0
  tokens = @{ input = 0; output = 0; reasoning = 0; cache = @{ read = 0; write = 0 } }
  time = @{ created = $now; updated = $now }
  title = 'OC2 rich fixture'
  location = @{ directory = $projA }
}
$messagesRich = @(
  @{ id = ('msg_ru' + $now); time = @{ created = $now }; type = 'user'; text = 'STX-OC2-MARKER-RICH user text' },
  @{ id = ('msg_ra' + $now); time = @{ created = $now + 10; completed = $now + 20 }; type = 'assistant'; agent = 'build'; model = @{ providerID = 'stub'; id = 'stub-model' }; content = @(
      @{ type = 'text'; text = 'STX-OC2-MARKER-RICH assistant text' },
      @{ type = 'tool'; id = 'call_rich_1'; name = 'read'; state = @{ status = 'completed'; input = @{ filePath = 'notes.txt' }; content = @( @{ type = 'text'; text = 'STX-OC2-MARKER-RICH tool output' } ) }; time = @{ created = $now + 11; completed = $now + 12 } }
    ) },
  @{ id = ('msg_rc' + $now); time = @{ created = $now + 30 }; type = 'compaction'; status = 'completed'; reason = 'manual'; summary = 'STX-OC2-MARKER-RICH compaction summary' },
  @{ id = ('msg_rs' + $now); time = @{ created = $now + 40 }; type = 'system'; text = 'STX-OC2-MARKER-RICH system text'; description = 'rich fixture system' },
  @{ id = ('msg_ry' + $now); time = @{ created = $now + 50 }; type = 'synthetic'; text = 'STX-OC2-MARKER-RICH synthetic text' }
)
$rich = @{ info = $infoRich; messages = $messagesRich }
Write-Body (Join-Path $out 'rich-fixture.json') ($rich | ConvertTo-Json -Depth 30 -Compress)
$richCode = Post-Json "http://127.0.0.1:$portB/api/experimental/session/import" ($rich | ConvertTo-Json -Depth 30 -Compress) (Join-Path $out 'import-rich-b.json')
"phase4: import rich fixture -> $richCode"
"phase4: export rich (http) -> " + (Get-Json "http://127.0.0.1:$portB/api/experimental/session/$richId/export" (Join-Path $out 'export-rich-b.json'))

Stop-Job $jobB; Receive-Job $jobB | ForEach-Object { $_ -replace [regex]::Escape($pw), '<scratch-password>' } | Set-Content -Path (Join-Path $out 'server-b.log') -Encoding utf8; Remove-Job $jobB -Force

# ---------- Comparison ----------
"compare export-a    : " + (MessageSummary (Join-Path $out 'export-a.json'))
"compare export-b    : " + (MessageSummary (Join-Path $out 'export-b.json'))

$a = (Get-Content (Join-Path $out 'export-a.json') -Raw | ConvertFrom-Json).data.messages | ConvertTo-Json -Depth 50 -Compress
$b = (Get-Content (Join-Path $out 'export-b.json') -Raw | ConvertFrom-Json).data.messages | ConvertTo-Json -Depth 50 -Compress
$na = $a -replace '[0-9a-f]{8}-[0-9a-f-]{27}','<uuid>' -replace '"time":\{[^}]*\}','"time":{}'
$nb = $b -replace '[0-9a-f]{8}-[0-9a-f-]{27}','<uuid>' -replace '"time":\{[^}]*\}','"time":{}'
"round-trip A->B messages normalized equal: " + ($na -eq $nb)

$richSent = Get-Content (Join-Path $out 'rich-fixture.json') -Raw
$richBack = ''
if (Test-Path (Join-Path $out 'export-rich-b.json')) { $richBack = Get-Content (Join-Path $out 'export-rich-b.json') -Raw }
"rich: sent types  = " + ((($messagesRich | ForEach-Object { $_.type }) -join ','))
"rich: back types  = " + (MessageSummary (Join-Path $out 'export-rich-b.json'))
foreach ($kind in 'user','assistant','compaction','system','synthetic') {
  "rich: type $kind sent=" + (([regex]::Matches($richSent, '"type":"' + $kind + '"')).Count) + " back=" + (([regex]::Matches($richBack, '"type":"' + $kind + '"')).Count)
}
foreach ($needle in 'STX-OC2-MARKER-RICH user text','STX-OC2-MARKER-RICH assistant text','STX-OC2-MARKER-RICH tool output','STX-OC2-MARKER-RICH compaction summary','STX-OC2-MARKER-RICH system text','STX-OC2-MARKER-RICH synthetic text','"name":"read"','"status":"completed"','"filePath":"notes.txt"') {
  "rich: kept [" + $needle + "] = " + $richBack.Contains($needle)
}
"rich: import response   : " + (Headline (Join-Path $out 'import-rich-b.json') 300)
"409 body head    : " + (Headline (Join-Path $out 'import-b-again-409.json') 300)
"CLI export A tail: " + (Headline (Join-Path $out 'export-a-cli.out.txt') 200)
"CLI import C tail: " + (Headline (Join-Path $out 'import-c-cli.out.txt') 200)
"CLI export C tail: " + (Headline (Join-Path $out 'export-c-cli.out.txt') 200)
"done. artifacts in $out"
