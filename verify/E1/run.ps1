# E1 - re-check of the engine 5.7 CLI-surface facts on the installed agent versions.
# Read-only: writes only into out\<date>\. Rerunnable without edits.
param([string]$Date = (Get-Date -Format 'yyyy-MM-dd'))
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$out = Join-Path $PSScriptRoot "out\$Date"
New-Item -ItemType Directory -Force -Path $out | Out-Null

function Cap($label, $exe, $argsArr, $seconds, $path) {
  $job = Start-Job { param($e,$a,$p) Set-Location $p; (& $e @a 2>&1 | Out-String) } -ArgumentList $exe,$argsArr,$root
  if (Wait-Job $job -Timeout $seconds) {
    $o = Receive-Job $job
    if ($null -eq $o) { $o = '' }
    Set-Content -Path $path -Value $o -Encoding utf8
    "{0}: ok, {1} chars" -f $label, $o.Length
  } else {
    Stop-Job $job
    Set-Content -Path $path -Value ("TIMEOUT: no output within {0} s" -f $seconds) -Encoding utf8
    "{0}: TIMEOUT >{1}s, no output" -f $label, $seconds
  }
  Remove-Job $job -Force
}

foreach ($a in 'claude','codex','opencode','pi') {
  Cap "$a --version" $a @('--version') 60 (Join-Path $out "$a-version.txt")
}
Cap 'claude --help' 'claude' @('--help') 60 (Join-Path $out 'claude-help.txt')
Cap 'codex --help' 'codex' @('--help') 60 (Join-Path $out 'codex-help.txt')
Cap 'codex resume --help' 'codex' @('resume','--help') 60 (Join-Path $out 'codex-resume-help.txt')
Cap 'codex fork --help' 'codex' @('fork','--help') 60 (Join-Path $out 'codex-fork-help.txt')
Cap 'codex queue --help' 'codex' @('queue','--help') 60 (Join-Path $out 'codex-queue-help.txt')
Cap 'codex login --help' 'codex' @('login','--help') 60 (Join-Path $out 'codex-login-help.txt')
Cap 'codex app-server --help' 'codex' @('app-server','--help') 60 (Join-Path $out 'codex-app-server-help.txt')
Cap 'codex exec --help' 'codex' @('exec','--help') 60 (Join-Path $out 'codex-exec-help.txt')
Cap 'opencode --help' 'opencode' @('--help') 60 (Join-Path $out 'opencode-help.txt')
Cap 'opencode run --help' 'opencode' @('run','--help') 60 (Join-Path $out 'opencode-run-help.txt')
Cap 'opencode session --help' 'opencode' @('session','--help') 60 (Join-Path $out 'opencode-session-help.txt')
Cap 'opencode auth --help' 'opencode' @('auth','--help') 60 (Join-Path $out 'opencode-auth-help.txt')
Cap 'pi --help' 'pi' @('--help') 150 (Join-Path $out 'pi-help.txt')
