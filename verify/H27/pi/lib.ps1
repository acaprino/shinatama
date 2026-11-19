# H27 / Pi - helpers shared by gen-source.ps1 and resume-capture.ps1 (dot-sourced after env.ps1).
# Pi is started as `node <pi cli.js>` (what the npm `pi` shim runs) so a hung run can be killed
# by PID without orphaning node.

function Get-H27PiCli {
  $shim = Get-Command pi.cmd -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
  if ($shim) {
    $cli = Join-Path (Split-Path $shim.Source) 'node_modules\@earendil-works\pi-coding-agent\dist\bundle\cli.js'
    if (Test-Path $cli) { return $cli }
  }
  $root = (& npm root -g 2>$null | Out-String).Trim()
  $cli = Join-Path $root '@earendil-works\pi-coding-agent\dist\bundle\cli.js'
  if (Test-Path $cli) { return $cli }
  throw "pi cli.js not found (looked next to pi.cmd and under npm root -g)"
}
$H27PiCli  = Get-H27PiCli
$H27NodeExe = (Get-Command node -CommandType Application | Select-Object -First 1).Source

# Quote one argument for a Windows command line (MSVC rules: backslashes before a quote double).
function ConvertTo-H27Arg([string]$a) {
  if ($a -ne '' -and $a -notmatch '[\s"]') { return $a }
  $sb = New-Object Text.StringBuilder
  [void]$sb.Append('"')
  $bs = 0
  foreach ($ch in $a.ToCharArray()) {
    if ($ch -eq '\') { $bs++; continue }
    if ($ch -eq '"') { [void]$sb.Append('\' * (2 * $bs + 1)); [void]$sb.Append('"'); $bs = 0; continue }
    if ($bs) { [void]$sb.Append('\' * $bs); $bs = 0 }
    [void]$sb.Append($ch)
  }
  if ($bs) { [void]$sb.Append('\' * (2 * $bs)) }
  [void]$sb.Append('"')
  $sb.ToString()
}

function New-H27PiProcess([string[]]$PiArgs, [string]$Cwd, [switch]$Stdin) {
  $psi = New-Object Diagnostics.ProcessStartInfo
  $psi.FileName = $H27NodeExe
  $psi.Arguments = ((@($H27PiCli) + $PiArgs) | ForEach-Object { ConvertTo-H27Arg $_ }) -join ' '
  $psi.WorkingDirectory = $Cwd
  $psi.UseShellExecute = $false
  $psi.CreateNoWindow = $true
  $psi.RedirectStandardInput = [bool]$Stdin
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  $psi.StandardOutputEncoding = New-Object Text.UTF8Encoding $false
  $psi.StandardErrorEncoding = New-Object Text.UTF8Encoding $false
  # .NET Framework builds the stdin writer from Console.InputEncoding (UTF-8 with BOM here) and
  # flushes the BOM at start; Pi's RPC parser rejects it. Use BOM-less UTF-8 while starting.
  $prev = $null
  if ($Stdin) { try { $prev = [Console]::InputEncoding; [Console]::InputEncoding = New-Object Text.UTF8Encoding $false } catch { $prev = $null } }
  try { [Diagnostics.Process]::Start($psi) } finally { if ($prev) { try { [Console]::InputEncoding = $prev } catch { } } }
}

# Kill a process and its children (taskkill /T), ignoring "not found".
function Stop-H27Tree([int]$ProcessId) {
  & taskkill.exe /PID $ProcessId /T /F 2>&1 | Out-Null
}

# One non-interactive pi run. Waits up to $TimeoutSec, then kills the tree.
# Returns @{ ExitCode; Stdout; Stderr; Killed; Seconds }.
function Invoke-H27Pi([string[]]$PiArgs, [string]$Cwd, [int]$TimeoutSec = 300) {
  $sw = [Diagnostics.Stopwatch]::StartNew()
  $p = New-H27PiProcess $PiArgs $Cwd
  $outTask = $p.StandardOutput.ReadToEndAsync()
  $errTask = $p.StandardError.ReadToEndAsync()
  $killed = $false
  if (-not $p.WaitForExit($TimeoutSec * 1000)) { Stop-H27Tree $p.Id; $killed = $true; [void]$p.WaitForExit(10000) }
  [void]$outTask.Wait(10000); [void]$errTask.Wait(10000)
  $code = if ($p.HasExited) { $p.ExitCode } else { $null }
  [pscustomobject]@{ ExitCode = $code; Stdout = $outTask.Result; Stderr = $errTask.Result; Killed = $killed; Seconds = [math]::Round($sw.Elapsed.TotalSeconds, 1); Args = $PiArgs }
}

# RPC run: send JSON lines one by one; after each, read stdout until a line matches its wait
# regex. Then close stdin and wait for exit. Returns @{ ExitCode; Stdout; Stderr; Killed }.
function Invoke-H27PiRpc([string[]]$PiArgs, [string]$Cwd, [object[]]$Steps, [int]$TimeoutSec = 300) {
  $sw = [Diagnostics.Stopwatch]::StartNew()
  $p = New-H27PiProcess (@('--mode', 'rpc') + $PiArgs) $Cwd -Stdin
  $stdin = $p.StandardInput.BaseStream
  $utf8 = New-Object Text.UTF8Encoding $false
  $errTask = $p.StandardError.ReadToEndAsync()
  $out = New-Object Text.StringBuilder
  $deadline = (Get-Date).AddSeconds($TimeoutSec)
  $pending = $null
  $ok = $true
  foreach ($step in $Steps) {
    if ($step.Delay) { Start-Sleep -Seconds $step.Delay }
    $bytes = $utf8.GetBytes($step.Send + "`n"); $stdin.Write($bytes, 0, $bytes.Length); $stdin.Flush()
    $matched = $false
    while (-not $matched) {
      if (-not $pending) { $pending = $p.StandardOutput.ReadLineAsync() }
      while (-not $pending.Wait(250)) { if ((Get-Date) -gt $deadline) { break } }
      if (-not $pending.IsCompleted) { $ok = $false; break }
      $line = $pending.Result; $pending = $null
      if ($null -eq $line) { $ok = $false; break }
      [void]$out.AppendLine($line)
      if ($line -match $step.WaitFor) { $matched = $true }
      elseif ($line -match '"type":"response"' -and $line -match '"success":false') { $ok = $false; break }
    }
    if (-not $ok) { break }
  }
  $stdin.Close()
  $killed = $false
  if (-not $p.WaitForExit(60000)) { Stop-H27Tree $p.Id; $killed = $true; [void]$p.WaitForExit(10000) }
  if ($pending) { [void]$pending.Wait(5000); if ($pending.IsCompleted -and $null -ne $pending.Result) { [void]$out.AppendLine($pending.Result) } }
  [void]$out.Append($p.StandardOutput.ReadToEnd())
  [void]$errTask.Wait(10000)
  $code = if ($p.HasExited) { $p.ExitCode } else { $null }
  [pscustomobject]@{ ExitCode = $code; Stdout = $out.ToString(); Stderr = $errTask.Result; Killed = $killed; Completed = $ok; Seconds = [math]::Round($sw.Elapsed.TotalSeconds, 1); Args = $PiArgs }
}

function Test-H27Port([int]$Port) {
  $c = New-Object Net.Sockets.TcpClient
  try { $c.Connect('127.0.0.1', $Port); return $true } catch { return $false } finally { $c.Close() }
}

# Start the shared mock (unchanged verify/H27/mock/server.mjs) with a fresh capture file.
function Start-H27Mock([string]$CaptureFile) {
  if (Test-H27Port $H27PiPort) { throw "port $H27PiPort is already in use; stop the other mock first" }
  New-Item -ItemType Directory -Force -Path (Split-Path $CaptureFile) | Out-Null
  if (Test-Path $CaptureFile) { Remove-Item $CaptureFile -Force }
  $log = Join-Path $H27PiRoot ("mock-{0}.log" -f [IO.Path]::GetFileNameWithoutExtension($CaptureFile))
  New-Item -ItemType Directory -Force -Path $H27PiRoot | Out-Null
  $p = Start-Process -FilePath $H27NodeExe -ArgumentList @((ConvertTo-H27Arg $H27PiMock), '--port', $H27PiPort, '--capture', (ConvertTo-H27Arg $CaptureFile)) `
    -WindowStyle Hidden -PassThru -RedirectStandardOutput $log -RedirectStandardError "$log.err"
  for ($i = 0; $i -lt 50 -and -not (Test-H27Port $H27PiPort); $i++) { Start-Sleep -Milliseconds 200 }
  if (-not (Test-H27Port $H27PiPort)) { Stop-H27Tree $p.Id; throw "mock did not start on $H27PiPort (see $log)" }
  $p
}

function Stop-H27Mock($Proc) { if ($Proc -and -not $Proc.HasExited) { Stop-H27Tree $Proc.Id } }

# Session file of a session id anywhere under the scratch home's sessions folder.
function Find-H27PiSession([string]$SessionId) {
  $root = Join-Path $H27PiHome 'sessions'
  Get-ChildItem -Path $root -Recurse -File -Filter "*_$SessionId.jsonl" -ErrorAction SilentlyContinue | Select-Object -First 1
}

# Newest file mtime in the REAL pi store (proves the runs never wrote there).
function Get-H27RealPiStamp {
  $real = Join-Path $env:USERPROFILE '.pi\agent'
  $files = @(Get-ChildItem -Path $real -File -ErrorAction SilentlyContinue) + @(Get-ChildItem -Path (Join-Path $real 'sessions') -Recurse -File -ErrorAction SilentlyContinue)
  $n = $files | Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1
  if ($n) { '{0:o} {1}' -f $n.LastWriteTimeUtc, $n.FullName } else { 'none' }
}

function Save-H27Run($Run, [string]$Base) {
  Write-H27Utf8 "$Base.stdout.txt" ([string]$Run.Stdout)
  Write-H27Utf8 "$Base.stderr.txt" ([string]$Run.Stderr)
  Write-H27Utf8 "$Base.exit.txt" ("exit={0} killed={1} seconds={2}`nargs: {3}`n" -f $Run.ExitCode, $Run.Killed, $Run.Seconds, ($Run.Args -join ' '))
}
