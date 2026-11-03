#Requires -Version 7.0
<#
Run the same verification locally and in Windows CI. Synthetic UI tests run in
a kill-on-close test job; no agent, account or model request is started.
The production application's P14 containment gate remains open.
#>
[CmdletBinding()]
param([ValidateRange(1, 45)][int]$TimeoutMinutes = 25)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$reportDirectory = Join-Path $workspaceRoot '.verification'
$logDirectory = Join-Path $reportDirectory 'logs'
[void](New-Item -ItemType Directory -Path $logDirectory -Force)
$checks = [Collections.Generic.List[object]]::new()
$deadline = [DateTime]::UtcNow.AddMinutes($TimeoutMinutes)
$previousBuildJobs = $env:CARGO_BUILD_JOBS
if (-not $env:CARGO_BUILD_JOBS) { $env:CARGO_BUILD_JOBS = '1' }
$report = [ordered]@{
    schema_version = 1
    scope = 'Windows x64 development verification; not release certification'
    status = 'running'
    started_at = [DateTime]::UtcNow.ToString('o')
    tools = [ordered]@{}
    checks = $checks
    limitations = @(
        'Remote GitHub execution is separate evidence from a local run.'
        'P14, release signing/install, sidecar and complete X3/X4/X5 gates remain open; TL1 has separate real-agent Windows evidence.'
        'No claim is made for macOS or Linux; Daodan remains deferred.'
    )
}

. (Join-Path $PSScriptRoot 'check-process.ps1')

function Find-WebViewVersion {
    $versions = [Collections.Generic.List[string]]::new()
    $roots = @(
        (Join-Path ${env:ProgramFiles(x86)} 'Microsoft/EdgeWebView/Application'),
        (Join-Path $env:LOCALAPPDATA 'Microsoft/EdgeWebView/Application')
    )
    if ($env:WEBVIEW2_BROWSER_EXECUTABLE_FOLDER) {
        $fixedExecutable = Join-Path $env:WEBVIEW2_BROWSER_EXECUTABLE_FOLDER 'msedgewebview2.exe'
        if (Test-Path -LiteralPath $fixedExecutable) { return (Get-Item -LiteralPath $fixedExecutable).VersionInfo.FileVersion }
    }
    foreach ($root in $roots) {
        foreach ($directory in (Get-ChildItem -LiteralPath $root -Directory -ErrorAction SilentlyContinue)) {
            $executable = Join-Path $directory.FullName 'msedgewebview2.exe'
            if (Test-Path -LiteralPath $executable) { [void]$versions.Add((Get-Item -LiteralPath $executable).VersionInfo.FileVersion) }
        }
    }
    if ($versions.Count) { return ($versions | Sort-Object { [version]$_ } -Descending | Select-Object -First 1) }
    throw 'Microsoft Edge WebView2 Runtime is required for desktop tests. Install it before verification.'
}

Push-Location $workspaceRoot
try {
    if (-not $IsWindows -or [Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne 'X64') {
        throw 'The supported verification target is Windows x64.'
    }
    foreach ($tool in @('node', 'npm.cmd', 'cargo', 'rustc', 'python', 'git')) {
        [void](Get-Command $tool -ErrorAction Stop)
    }
    $nodeVersion = (& node --version).Trim()
    $expectedNode = 'v' + (Get-Content -LiteralPath '.node-version' -Raw).Trim()
    if ($nodeVersion -ne $expectedNode) { throw "Expected Node $expectedNode, found $nodeVersion" }
    $rustVersion = (& rustc --version).Trim()
    if ($rustVersion -notmatch '^rustc 1\.99\.1 ') { throw "Expected pinned Rust 1.99.1, found $rustVersion" }
    $report.tools = [ordered]@{
        windows = [Environment]::OSVersion.Version.ToString()
        powershell = $PSVersionTable.PSVersion.ToString()
        node = $nodeVersion
        npm = (& npm.cmd --version).Trim()
        rustc = $rustVersion
        cargo = (& cargo --version).Trim()
        python = (& python --version).Trim()
        webview2_installed_candidate = Find-WebViewVersion
    }
    $report.source_commit = (& git rev-parse HEAD).Trim()
    $report.source_dirty = -not [string]::IsNullOrWhiteSpace((& git status --porcelain | Out-String))
    # Dependency restoration is explicit; no audit/update/catalog call is part of it.
    Invoke-Check 'npm-install' 'npm.cmd' @('ci', '--no-audit', '--no-fund')
    Invoke-Check 'plan' 'python' @('scripts/implementation.py', 'validate')
    Invoke-Check 'runner-tests' 'pwsh' @('-NoProfile', '-File', 'scripts/test_verify.ps1')
    Invoke-Check 'python-tests' 'python' @('-m', 'unittest', 'discover', '-s', 'scripts', '-p', 'test_*.py')
    Invoke-Check 'frontend-tests' 'npm.cmd' @('test')
    Invoke-Check 'frontend-build' 'npm.cmd' @('run', 'build')
    Invoke-Check 'dependencies' 'python' @('scripts/dependencies.py', 'check')
    Invoke-Check 'rust-format' 'cargo' @('fmt', '--all', '--check')
    Invoke-Check 'rust-clippy' 'cargo' @('clippy', '--locked', '--workspace', '--all-targets', '--features', 'custom-protocol', '--', '-D', 'warnings')
    Invoke-Check 'rust-tests' 'cargo' @('test', '--locked', '--workspace', '--all-targets', '--features', 'custom-protocol')
    Invoke-Check 'test-job-build' 'cargo' @('build', '--locked', '--manifest-path', 'verify/P14/Cargo.toml')
    Invoke-Check 'desktop-build' 'cargo' @('build', '--locked', '-p', 'shinatama-desktop', '-p', 'shinatama-cli', '-p', 'shinatama-terminal', '--bins', '--features', 'custom-protocol')
    $probe = Join-Path $workspaceRoot 'verify/P14/target/debug/p14-probe.exe'
    # The real desktop check below covers ConPTY through its admitted helper.
    # A default unconfigured Windows host deliberately cannot spawn a process.
    # These share a checkout and use fixed CDP ports: keep them sequential.
    Invoke-Check 'desktop-smoke' $probe @('wrap-run', 'node', 'apps/desktop/tests/desktop-smoke.mjs')
    Invoke-Check 'projects-smoke' $probe @('wrap-run', 'node', 'apps/desktop/tests/projects-smoke.mjs')
    Invoke-Check 'storage-smoke' $probe @('wrap-run', 'node', 'apps/desktop/tests/storage-smoke.mjs')
    Invoke-Check 'state-smoke' $probe @('wrap-run', 'node', 'apps/desktop/tests/state-smoke.mjs')
    Invoke-Check 'catalog-smoke' $probe @('wrap-run', 'node', 'apps/desktop/tests/catalog-smoke.mjs')
    Invoke-Check 'new-project-smoke' $probe @('wrap-run', 'node', 'apps/desktop/tests/new-project-smoke.mjs')
    Invoke-Check 'launch-preflight-smoke' $probe @('wrap-run', 'node', 'apps/desktop/tests/launch-preflight-smoke.mjs')
    Invoke-Check 'resources-smoke' $probe @('wrap-run', 'node', 'apps/desktop/tests/resources-smoke.mjs')
    Invoke-Check 'maintenance-smoke' $probe @('wrap-run', 'node', 'apps/desktop/tests/maintenance-smoke.mjs')
    $report.executable = [ordered]@{
        path = 'target/debug/shinatama-desktop.exe'
        sha256 = (Get-FileHash -LiteralPath 'target/debug/shinatama-desktop.exe' -Algorithm SHA256).Hash.ToLowerInvariant()
        bytes = (Get-Item -LiteralPath 'target/debug/shinatama-desktop.exe').Length
        kind = 'embedded debug prototype; unsigned'
    }
    $report.status = 'passed'
    Write-Host 'PASS: Windows verification completed. Report: .verification/verification.json'
} catch {
    $report.status = 'failed'
    Write-Host "FAIL: $($_.Exception.Message)"
    exit 1
} finally {
    $report.finished_at = [DateTime]::UtcNow.ToString('o')
    $report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $reportDirectory 'verification.json') -Encoding utf8NoBOM
    $env:CARGO_BUILD_JOBS = $previousBuildJobs
    Pop-Location
}
