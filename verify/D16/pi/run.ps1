# D16 / Pi - install into a private folder, self-update off, side by side, rollback.
#   powershell -NoProfile -ExecutionPolicy Bypass -File verify\D16\pi\run.ps1 [-KeepInstalls]
# Raw output: %LOCALAPPDATA%\shinatama-verify\D16\pi\  (versions\<ver> = npm --prefix folders)
# Network: npm registry ONLY, during the two installs. npm runs with a decoy profile
# (USERPROFILE/APPDATA/LOCALAPPDATA/TEMP) so whatever it writes outside --prefix is visible.
# Every Pi run talks only to the mock; the update-check probe uses logproxy.mjs (refuses all).
param([switch]$KeepInstalls)
$ErrorActionPreference = 'Stop'
$BenchRow = 'D16'
. (Join-Path $PSScriptRoot '..\..\D15\pi\bench.ps1')

$Pkg      = '@earendil-works/pi-coding-agent'   # what the user's global install is (PI10)
$New      = '1.4.0'; $Old = '1.2.0'
$Base     = Join-Path $BenchRealLocalAppData 'shinatama-verify\D16\pi'
$VerRoot  = Join-Path $Base 'versions'
$NpmDecoy = Join-Path $Base 'npm-decoy'
$PiHome   = Join-Path $Base 'home'
$Work     = Join-Path $Base 'work'
$Out      = Join-Path $Base 'out'
$Log      = Join-Path $Out 'run.log.txt'
$Port     = 4163; $ProxyPort = 4169
if ($KeepInstalls) { foreach ($d in $NpmDecoy, $PiHome, $Work, $Out) { Reset-BenchDir $d } } else { Reset-BenchDir $Base }
foreach ($d in $VerRoot, $PiHome, $Work, $Out, (Join-Path $NpmDecoy 'home'), (Join-Path $NpmDecoy 'appdata'), (Join-Path $NpmDecoy 'localappdata\Temp')) { New-Item -ItemType Directory -Force -Path $d | Out-Null }
Set-BenchPi -Root $Base -PiHome $PiHome -Port $Port -Mock (Join-Path $BenchH27 'server.local.mjs')
Initialize-H27PiHome

function Get-VerCli([string]$Ver) { Join-Path $VerRoot "$Ver\node_modules\@earendil-works\pi-coding-agent\dist\bundle\cli.js" }

function Install-PiVersion([string]$Ver) {
  $prefix = Join-Path $VerRoot $Ver
  if ($KeepInstalls -and (Test-Path (Get-VerCli $Ver))) { Write-BenchLog $Log "install $Ver skipped (-KeepInstalls)"; return }
  New-Item -ItemType Directory -Force -Path $prefix | Out-Null
  $names = 'USERPROFILE','HOME','APPDATA','LOCALAPPDATA','TEMP','TMP','HTTP_PROXY','HTTPS_PROXY','NO_PROXY','no_proxy'
  $save = @{}; foreach ($n in $names) { $save[$n] = [Environment]::GetEnvironmentVariable($n) }
  $env:USERPROFILE = Join-Path $NpmDecoy 'home'; $env:HOME = $env:USERPROFILE
  $env:APPDATA = Join-Path $NpmDecoy 'appdata'; $env:LOCALAPPDATA = Join-Path $NpmDecoy 'localappdata'
  $env:TEMP = Join-Path $NpmDecoy 'localappdata\Temp'; $env:TMP = $env:TEMP
  foreach ($n in 'HTTP_PROXY','HTTPS_PROXY','NO_PROXY','no_proxy') { Remove-Item "Env:$n" -ErrorAction SilentlyContinue }
  $sw = [Diagnostics.Stopwatch]::StartNew()
  $eap = $ErrorActionPreference; $ErrorActionPreference = 'Continue'   # npm warnings go to stderr
  try {
    $o = & npm.cmd install --prefix $prefix "$Pkg@$Ver" --no-fund --no-audit 2>&1 | ForEach-Object { "$_" } | Out-String
    $code = $LASTEXITCODE
  } finally { $ErrorActionPreference = $eap; foreach ($n in $names) { [Environment]::SetEnvironmentVariable($n, $save[$n]) } }
  Write-H27Utf8 (Join-Path $Out "npm-install-$Ver.txt") $o
  Write-BenchLog $Log ("npm install --prefix versions\{0} {1}@{0}: exit={2} {3}s :: {4}" -f $Ver, $Pkg, $code, [math]::Round($sw.Elapsed.TotalSeconds,1), ($o.Trim() -replace '\s+', ' '))
  $prefixFiles = Get-BenchTree $prefix
  $top = $prefixFiles | ForEach-Object { ($_.Rel -split '\\')[0] } | Sort-Object -Unique
  Write-BenchLog $Log ("  prefix {0}: {1} files, top level: {2}" -f $Ver, $prefixFiles.Count, ($top -join ', '))
  $native = @($prefixFiles | Where-Object { $_.Rel -match '\.(node|dll|exe)$' })
  Write-BenchLog $Log ("  native binaries in prefix (lockable while running): {0}" -f (($native | ForEach-Object Rel) -join ', '))
  $pj = Get-Content (Join-Path $prefix 'node_modules\@earendil-works\pi-coding-agent\package.json') -Raw | ConvertFrom-Json
  Write-BenchLog $Log ("  package {0} {1} engines={2} bin={3}" -f $pj.name, $pj.version, ($pj.engines | ConvertTo-Json -Compress), ($pj.bin | ConvertTo-Json -Compress))
}

function Get-CmdLine([int]$ProcessId) { (Get-CimInstance Win32_Process -Filter "ProcessId=$ProcessId").CommandLine }

# Send one RPC prompt to a running Pi and read stdout until the agent ends.
function Send-RpcPrompt($Proc, [string]$Id, [string]$Text, [int]$TimeoutSec = 120) {
  $utf8 = New-Object Text.UTF8Encoding $false
  $json = '{"id":"' + $Id + '","type":"prompt","message":"' + $Text + '"}'
  $b = $utf8.GetBytes($json + "`n"); $Proc.StandardInput.BaseStream.Write($b, 0, $b.Length); $Proc.StandardInput.BaseStream.Flush()
  $deadline = (Get-Date).AddSeconds($TimeoutSec); $lines = New-Object Collections.Generic.List[string]
  while ((Get-Date) -lt $deadline) {
    $t = $Proc.StandardOutput.ReadLineAsync()
    while (-not $t.Wait(250)) { if ((Get-Date) -gt $deadline) { break } }
    if (-not $t.IsCompleted -or $null -eq $t.Result) { break }
    $lines.Add($t.Result)
    if ($t.Result -match '"type":"agent_end"') { break }
  }
  $reply = ($lines | Where-Object { $_ -match 'H27-REPLY: [^"]*' } | ForEach-Object { $Matches[0] } | Select-Object -Last 1)
  [pscustomobject]@{ Lines = $lines; Reply = $reply }
}

Write-BenchLog $Log ("node {0} at {1}" -f (& $H27NodeExe --version), $H27NodeExe)
Write-BenchLog $Log ("real ~\.pi\agent newest before: {0}" -f (Get-BenchRealPiStamp))

# 1. Newer version installed and started (RPC, stays running).
Install-PiVersion $New
$mockProc = Start-H27Mock (Join-Path $Out 'capture.jsonl')
try {
  $script:H27PiCli = Get-VerCli $New
  $r = Invoke-H27Pi @('--version') $Work 60
  Write-BenchLog $Log ("{0} --version via node.exe <cli.js>: '{1}{2}' exit={3}" -f $New, $r.Stdout.Trim(), $r.Stderr.Trim(), $r.ExitCode)
  $running = New-H27PiProcess (@('--mode', 'rpc') + $H27PiFlags) $Work -Stdin
  $a1 = Send-RpcPrompt $running 'n1' 'D16-N1 first turn on the newer version'
  Write-BenchLog $Log ("{0} RPC pid={1} reply='{2}' cmdline: {3}" -f $New, $running.Id, $a1.Reply, (Get-CmdLine $running.Id))

  # 2. Older version installed WHILE the newer one runs, then started.
  Install-PiVersion $Old
  $script:H27PiCli = Get-VerCli $Old
  $r = Invoke-H27Pi @('--version') $Work 60
  Write-BenchLog $Log ("{0} --version via node.exe <cli.js>: '{1}{2}' exit={3}" -f $Old, $r.Stdout.Trim(), $r.Stderr.Trim(), $r.ExitCode)
  $r = Invoke-H27Pi (@('-p') + $H27PiFlags + @('D16-O1 older version, new session, while newer runs')) $Work 300; Save-H27Run $r (Join-Path $Out 'old-run1')
  Write-BenchLog $Log ("{0} print run while {1} pid {2} runs (alive={3}): exit={4} out='{5}' err='{6}'" -f $Old, $New, $running.Id, (-not $running.HasExited), $r.ExitCode, $r.Stdout.Trim(), $r.Stderr.Trim())

  # 3. The newer instance is still serving.
  $a2 = Send-RpcPrompt $running 'n2' 'D16-N2 second turn on the newer version after the install'
  Write-BenchLog $Log ("{0} RPC pid={1} still alive, second reply='{2}'" -f $New, $running.Id, $a2.Reply)
  $running.StandardInput.Close(); if (-not $running.WaitForExit(30000)) { Stop-H27Tree $running.Id }
  Write-H27Utf8 (Join-Path $Out 'new-rpc.stdout.txt') (($a1.Lines + $a2.Lines) -join "`n")

  # 4. Rollback: the older version started again after the newer one, resuming the newest
  #    session of the shared private home (written by the newer version).
  $script:H27PiCli = Get-VerCli $Old
  $r = Invoke-H27Pi (@('-p') + $H27PiFlags + @('-c', 'D16-O2 rollback, older resumes')) $Work 300; Save-H27Run $r (Join-Path $Out 'old-run2-rollback')
  $s = Get-BenchNewestSession $PiHome
  Write-BenchLog $Log ("rollback {0} -c: exit={1} out='{2}' err='{3}' session={4}" -f $Old, $r.ExitCode, $r.Stdout.Trim(), $r.Stderr.Trim(), $s.Id)
} finally { Stop-H27Mock $mockProc }

# 5. Update check / telemetry probe: which hosts does each version try to reach in print mode,
#    online (no --offline, no PI_OFFLINE, no PI_SKIP_VERSION_CHECK) vs offline? Proxy refuses all.
$proxyLog = Join-Path $Out 'proxy.jsonl'
$proxy = Start-Process -FilePath $H27NodeExe -ArgumentList @((ConvertTo-H27Arg (Join-Path $PSScriptRoot 'logproxy.mjs')), '--port', $ProxyPort, '--log', (ConvertTo-H27Arg $proxyLog)) -WindowStyle Hidden -PassThru
for ($i = 0; $i -lt 50 -and -not (Test-H27Port $ProxyPort); $i++) { Start-Sleep -Milliseconds 200 }
$mockProc = Start-H27Mock (Join-Path $Out 'capture-probe.jsonl')
$env:HTTP_PROXY = "http://127.0.0.1:$ProxyPort"; $env:HTTPS_PROXY = $env:HTTP_PROXY
try {
  foreach ($v in $New, $Old) {
    $script:H27PiCli = Get-VerCli $v
    foreach ($mode in 'online', 'offline') {
      if ($mode -eq 'online') { Remove-Item Env:PI_OFFLINE, Env:PI_SKIP_VERSION_CHECK -ErrorAction SilentlyContinue; $flags = @('--provider', $H27PiProvider, '--model', $H27PiModel, '--no-skills') }
      else { $env:PI_OFFLINE = '1'; $env:PI_SKIP_VERSION_CHECK = '1'; $flags = $H27PiFlags }
      $n0 = @(Get-Content $proxyLog -ErrorAction SilentlyContinue).Count
      $r = Invoke-H27Pi (@('-p') + $flags + @("D16-P $v $mode")) $Work 120; Save-H27Run $r (Join-Path $Out "probe-$v-$mode")
      Start-Sleep -Seconds 2
      $hits = @(Get-Content $proxyLog -ErrorAction SilentlyContinue | Select-Object -Skip $n0 | ForEach-Object { $j = $_ | ConvertFrom-Json; if ($j.target) { $j.target } else { $j.url } })
      Write-BenchLog $Log ("probe {0} print {1}: exit={2} out='{3}' proxy attempts: {4}" -f $v, $mode, $r.ExitCode, $r.Stdout.Trim(), ($(if ($hits) { $hits -join ', ' } else { 'none' })))
    }
    # Interactive mode (the only mode that calls the version check, per source), online, no TTY:
    # started with redirected stdio, killed after 12 s; only the proxy log matters.
    Remove-Item Env:PI_OFFLINE, Env:PI_SKIP_VERSION_CHECK -ErrorAction SilentlyContinue
    $n0 = @(Get-Content $proxyLog -ErrorAction SilentlyContinue).Count
    $ip = New-H27PiProcess @('--provider', $H27PiProvider, '--model', $H27PiModel, '--no-skills') $Work -Stdin
    $errT = $ip.StandardError.ReadToEndAsync(); $outT = $ip.StandardOutput.ReadToEndAsync()
    [void]$ip.WaitForExit(12000); $alive = -not $ip.HasExited; if ($alive) { Stop-H27Tree $ip.Id }
    [void]$errT.Wait(5000); [void]$outT.Wait(5000)
    $hits = @(Get-Content $proxyLog -ErrorAction SilentlyContinue | Select-Object -Skip $n0 | ForEach-Object { $j = $_ | ConvertFrom-Json; if ($j.target) { $j.target } else { $j.url } })
    $err1 = ([string]$errT.Result).Trim() -replace '\s+', ' '
    Write-BenchLog $Log ("probe {0} interactive online (no TTY): alive after 12s={1} stderr='{2}' proxy attempts: {3}" -f $v, $alive, $err1.Substring(0, [Math]::Min(200, $err1.Length)), ($(if ($hits) { $hits -join ', ' } else { 'none' })))
  }
} finally {
  $env:HTTP_PROXY = 'http://127.0.0.1:9'; $env:HTTPS_PROXY = $env:HTTP_PROXY; $env:PI_OFFLINE = '1'; $env:PI_SKIP_VERSION_CHECK = '1'
  Stop-H27Mock $mockProc; Stop-H27Tree $proxy.Id
}

# 6. What npm wrote outside --prefix (decoy profile), and the real store.
$npmTree = Get-BenchTree $NpmDecoy
$byTop = $npmTree | ForEach-Object { (($_.Rel -split '\\') | Select-Object -First 3) -join '\' } | Group-Object | ForEach-Object { "{0} files, {1}" -f $_.Count, $_.Name }
Write-H27Utf8 (Join-Path $Out 'npm-decoy-files.txt') (Format-BenchTree $npmTree)
Write-BenchLog $Log ("npm wrote outside --prefix (decoy profile): {0} files; {1}" -f $npmTree.Count, ($byTop -join '; '))
Write-H27Utf8 (Join-Path $Out 'home-files.txt') (Format-BenchTree (Get-BenchTree $PiHome))
Write-BenchLog $Log ("real ~\.pi\agent newest after: {0}" -f (Get-BenchRealPiStamp))
Write-BenchLog $Log ("raw output: {0}" -f $Out)
