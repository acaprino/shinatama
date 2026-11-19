#Requires -Version 7.0
$ErrorActionPreference = 'Stop'
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$logDirectory = Join-Path $workspaceRoot '.verification/runner-fixtures'
[void](New-Item -ItemType Directory -Path $logDirectory -Force)
$checks = [Collections.Generic.List[object]]::new()
$deadline = [DateTime]::UtcNow.AddMinutes(1)
. (Join-Path $PSScriptRoot 'check-process.ps1')

Invoke-Check 'success' 'python' @('-c', 'print("VERIFICATION_CHILD_OK")')
if ($checks[-1].status -ne 'passed' -or -not (Get-Content (Join-Path $logDirectory 'success.stdout.log') -Raw).Contains('VERIFICATION_CHILD_OK')) {
    throw 'The runner did not retain successful child output'
}
$failed = $false
try { Invoke-Check 'nonzero' 'python' @('-c', 'raise SystemExit(7)') } catch { $failed = $true }
if (-not $failed -or $checks[-1].status -ne 'failed' -or $checks[-1].exit_code -ne 7) {
    throw 'The runner did not propagate a child failure'
}
$timedOut = $false
try { Invoke-Check 'deadline' 'python' @('-c', 'import time; time.sleep(30)') -TimeoutSeconds 1 } catch { $timedOut = $_.Exception.Message.Contains('timeout') }
if (-not $timedOut -or $checks[-1].seconds -gt 8) { throw 'Per-check timeout did not terminate the owned child promptly' }
$deadline = [DateTime]::UtcNow.AddSeconds(1)
$timedOut = $false
try { Invoke-Check 'whole-run-deadline' 'python' @('-c', 'import time; time.sleep(30)') } catch { $timedOut = $_.Exception.Message.Contains('timeout') }
if (-not $timedOut -or $checks[-1].seconds -gt 8) { throw 'Whole-run deadline was not enforced' }
$deadline = [DateTime]::UtcNow.AddMinutes(1)
$limited = $false
try { Invoke-Check 'log-budget' 'python' @('-c', 'import sys; sys.stdout.write("x" * (33 * 1024 * 1024))') } catch { $limited = $_.Exception.Message.Contains('log budget') }
if (-not $limited) { throw 'A fast child bypassed the log budget' }
Write-Host 'PASS: runner success, exit failure, per-check/whole-run timeout and output-budget fault probes'
exit 0
