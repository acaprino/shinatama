# PI5 - can the Pi TUI attach to an external backend? Evidence from pi --help and the installed package docs.
# Read-only: no login, no model use. Rerunnable without edits.
param([string]$Date = (Get-Date -Format 'yyyy-MM-dd'))
$out = Join-Path $PSScriptRoot "out\$Date"
New-Item -ItemType Directory -Force -Path $out | Out-Null

# pi --help with a cap; on this machine it once produced no output within 40 s (see the PI11 record).
$job = Start-Job { (pi --help 2>&1 | Out-String) }
if (Wait-Job $job -Timeout 150) {
  (Receive-Job $job) | Set-Content (Join-Path $out 'pi-help.txt') -Encoding utf8
} else {
  Stop-Job $job
  Set-Content (Join-Path $out 'pi-help.txt') -Value 'TIMEOUT >150 s' -Encoding utf8
}
Remove-Job $job -Force

$rootg = (npm root -g 2>&1 | Out-String).Trim()
$pkg = Join-Path $rootg '@earendil-works\pi-coding-agent'
$docs = Get-ChildItem (Join-Path $pkg 'docs') -Recurse -File -ErrorAction SilentlyContinue
if ($docs) {
  $docs | ForEach-Object { $_.FullName.Replace($pkg,'') } | Set-Content (Join-Path $out 'pi-docs-list.txt') -Encoding utf8
  Select-String -Path $docs.FullName -Pattern 'rpc|attach|remote' -List | ForEach-Object { "$($_.Path.Replace($pkg,'')) :: $($_.Line.Trim())" } | Set-Content (Join-Path $out 'pi-docs-attach-hits.txt') -Encoding utf8
  Get-Content (Join-Path $out 'pi-docs-attach-hits.txt')
} else {
  "package docs not found at $pkg"
}
