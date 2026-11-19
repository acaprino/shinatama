# H27 / OpenCode: isolation check. Runs one `opencode run` (H27-HANG, killed after ~11 s) with the env.ps1
# isolation on a SEPARATE scratch DB (probe-net\opencode.db; the source session is untouched) and lists every
# remote TCP endpoint the CLI and its `serve --stdio` child opened, plus URLs in its debug log.
#   powershell -NoProfile -ExecutionPolicy Bypass -File verify\H27\opencode\net-probe.ps1
$ErrorActionPreference = 'Continue'
. (Join-Path $PSScriptRoot 'env.ps1')
$probe = Join-Path $H27Root 'probe-net'
Remove-Item $probe -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $probe | Out-Null
$env:OPENCODE_DB = Join-Path $probe 'opencode.db'
$cap = Join-Path $probe 'capture.jsonl'
$mock = Start-H27Mock $cap
$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = $H27Exe
$psi.Arguments = 'run --standalone --model anthropic/h27-mock --format json --auto --log-level debug --print-logs'
$psi.WorkingDirectory = $H27Work
$psi.UseShellExecute = $false; $psi.CreateNoWindow = $true
$psi.RedirectStandardInput = $true; $psi.RedirectStandardOutput = $true; $psi.RedirectStandardError = $true
$p = [System.Diagnostics.Process]::Start($psi)
$errTask = $p.StandardError.ReadToEndAsync(); $outTask = $p.StandardOutput.ReadToEndAsync()
$b = [Text.Encoding]::UTF8.GetBytes('H27-HANG'); $p.StandardInput.BaseStream.Write($b, 0, $b.Length); $p.StandardInput.Close()
$seen = @{}
for ($i = 0; $i -lt 16; $i++) {
  Start-Sleep -Milliseconds 700
  $pids = @($p.Id) + @(Get-CimInstance Win32_Process | Where-Object { $_.ParentProcessId -eq $p.Id } | ForEach-Object { $_.ProcessId })
  foreach ($c in @(Get-NetTCPConnection -ErrorAction SilentlyContinue | Where-Object { $pids -contains $_.OwningProcess })) {
    $k = "$($c.RemoteAddress):$($c.RemotePort) $($c.State)"
    if ($c.RemoteAddress -notin @('0.0.0.0', '::')) { $seen[$k] = 1 }
  }
}
& taskkill.exe /PID $p.Id /T /F 2>&1 | Out-Null
Stop-H27Process $mock
$null = $errTask.Wait(5000)
$log = $errTask.Result
[IO.File]::WriteAllText((Join-Path $H27Out 'net-probe.debug-log.txt'), $log, $H27Utf8)
$report = & {
"remote endpoints seen from opencode run + its serve child:"
$seen.Keys | Sort-Object
"debug log lines: " + ($log -split "`n").Count
"URLs in debug log:"
[regex]::Matches($log, '(https?|wss?)://[^\s"''\\]+') | ForEach-Object { $_.Value } | Sort-Object -Unique
"proxy/fetch/network mentions:"
($log -split "`n") | Where-Object { $_ -match 'proxy|ECONNREFUSED|fetch|models\.dev|opencode\.ai|telemetry|share' } | ForEach-Object { $_.Substring(0, [Math]::Min(220, $_.Length)) } | Select-Object -First 15
}
$report
[IO.File]::WriteAllLines((Join-Path $H27Out 'net-probe.txt'), [string[]]$report, $H27Utf8)
"written to  : $(Join-Path $H27Out 'net-probe.txt')"
