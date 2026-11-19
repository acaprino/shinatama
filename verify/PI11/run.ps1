# PI11 - Pi start-up timing probes (read-only, no login, no model use).
# Rerunnable without edits. Records seconds and output length per command.
param([string]$Date = (Get-Date -Format 'yyyy-MM-dd'))
$out = Join-Path $PSScriptRoot "out\$Date"
New-Item -ItemType Directory -Force -Path $out | Out-Null

function Time-Cap($label, $exe, $argsArr, $seconds, $path) {
  $sw = [System.Diagnostics.Stopwatch]::StartNew()
  $job = Start-Job { param($e,$a) (& $e @a 2>&1 | Out-String) } -ArgumentList $exe,$argsArr
  if (Wait-Job $job -Timeout $seconds) {
    $sw.Stop(); $o = Receive-Job $job
    $msg = "$label : completed in $([math]::Round($sw.Elapsed.TotalSeconds,1)) s, output chars: $($o.Length)"
  } else {
    Stop-Job $job
    $msg = "$label : TIMEOUT >$seconds s, no output"
  }
  Remove-Job $job -Force
  Set-Content -Path $path -Value $msg -Encoding utf8
  $msg
}

Time-Cap 'pi --version' 'pi' @('--version') 60 (Join-Path $out 'pi-version.txt')
Time-Cap 'pi --help' 'pi' @('--help') 150 (Join-Path $out 'pi-help-timing.txt')
