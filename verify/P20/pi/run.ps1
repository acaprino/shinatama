# P20 / Pi - inherited agent markers. Rerunnable; resets its scratch.
#   powershell -NoProfile -ExecutionPolicy Bypass -File verify\P20\pi\run.ps1
# Raw output: %LOCALAPPDATA%\shinatama-verify\P20\pi\
# Same mock session (new session: "P20-ENVNAMES" -> one bash tool call printing env NAMES;
# then -c "P20-T2 hello") in three environments, each with its own fresh private home and the
# same cwd so request bodies are comparable:
#   clean  - every marker removed (what Shinatama launches)
#   pi     - what a Pi bash-tool child carries (PI_CODING_AGENT, AI_AGENT=pi, PI_SESSION_ID,
#            PI_SESSION_FILE -> the clean run's session file, PI_PROVIDER, PI_MODEL,
#            PI_REASONING_LEVEL, PATH with the outer home's bin first)
#   claude - Claude Code markers (CLAUDECODE=1, AI_AGENT=claude-code, CLAUDE_CODE_ENTRYPOINT=cli)
# Then compare.mjs diffs captures, stdout/stderr, exit codes and files written.
$ErrorActionPreference = 'Stop'
$BenchRow = 'P20'
. (Join-Path $PSScriptRoot '..\..\D15\pi\bench.ps1')

$Base = Join-Path $BenchRealLocalAppData 'shinatama-verify\P20\pi'
$Work = Join-Path $Base 'work'
$Port = 4162
Reset-BenchDir $Base
New-Item -ItemType Directory -Force -Path $Work | Out-Null
$MockScript = Join-Path $PSScriptRoot 'server.p20.mjs'
$Log = Join-Path $Base 'run.log.txt'
$BasePath = $env:PATH
$CleanSession = $null

foreach ($variant in 'clean', 'pi', 'claude') {
  $vdir = Join-Path $Base $variant
  $home_ = Join-Path $vdir 'home'
  New-Item -ItemType Directory -Force -Path $home_ | Out-Null
  Set-BenchPi -Root $vdir -PiHome $home_ -Port $Port -Mock $MockScript
  Initialize-H27PiHome
  $mockProc = Start-H27Mock (Join-Path $vdir 'capture.jsonl')
  Clear-BenchMarkers
  $env:PATH = $BasePath
  $outerFileHash = $null
  switch ($variant) {
    'pi' {
      $env:PI_CODING_AGENT = 'true'; $env:AI_AGENT = 'pi'
      $env:PI_SESSION_ID = $CleanSession.Id; $env:PI_SESSION_FILE = $CleanSession.File
      $env:PI_PROVIDER = $H27PiProvider; $env:PI_MODEL = $H27PiModel; $env:PI_REASONING_LEVEL = 'medium'
      $env:PATH = (Join-Path (Join-Path $Base 'clean\home') 'bin') + ';' + $BasePath
      $outerFileHash = (Get-FileHash $CleanSession.File -Algorithm SHA256).Hash
    }
    'claude' { $env:CLAUDECODE = '1'; $env:AI_AGENT = 'claude-code'; $env:CLAUDE_CODE_ENTRYPOINT = 'cli' }
  }
  # Names only, never values.
  Write-H27Utf8 (Join-Path $vdir 'parent-env-names.txt') (((Get-ChildItem Env:).Name | Sort-Object) -join "`n")
  try {
    $r1 = Invoke-H27Pi (@('-p') + $H27PiFlags + @('P20-ENVNAMES list the environment')) $Work 300; Save-H27Run $r1 (Join-Path $vdir 'turn1')
    $r2 = Invoke-H27Pi (@('-p') + $H27PiFlags + @('-c', 'P20-T2 hello')) $Work 300; Save-H27Run $r2 (Join-Path $vdir 'turn2')
  } finally { Stop-H27Mock $mockProc; Clear-BenchMarkers; $env:PATH = $BasePath }
  $s = Get-BenchNewestSession $home_
  if ($variant -eq 'clean') { $CleanSession = $s }
  Write-H27Utf8 (Join-Path $vdir 'home-files.txt') (Format-BenchTree (Get-BenchTree $home_))
  Write-BenchLog $Log ("[{0}] turn1 exit={1} out={2} | turn2 exit={3} out={4} | stderr1={5} stderr2={6} | session={7}" -f $variant, $r1.ExitCode, $r1.Stdout.Trim(), $r2.ExitCode, $r2.Stdout.Trim(), $r1.Stderr.Trim().Length, $r2.Stderr.Trim().Length, $s.Id)
  if ($outerFileHash) {
    $now = (Get-FileHash $CleanSession.File -Algorithm SHA256).Hash
    Write-BenchLog $Log ("[pi] outer session file named by PI_SESSION_FILE unchanged: {0}" -f ($now -eq $outerFileHash))
  }
}
Write-H27Utf8 (Join-Path $Base 'work-files.txt') (Format-BenchTree (Get-BenchTree $Work))
& $H27NodeExe (Join-Path $PSScriptRoot 'compare.mjs') $Base 2>&1 | Tee-Object -FilePath (Join-Path $Base 'compare.txt')
Write-BenchLog $Log ("raw output: {0}" -f $Base)
