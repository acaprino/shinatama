#Requires -Version 7.0
<#
Run the same verification locally and in Windows CI. Synthetic UI tests run in
a kill-on-close test job; no agent, account or model request is started.
The production application's P14 containment gate remains open.
#>
[CmdletBinding()]
param([ValidateRange(1, 180)][int]$TimeoutMinutes = 25)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$reportDirectory = Join-Path $workspaceRoot '.verification'
$logDirectory = Join-Path $reportDirectory 'logs'
[void](New-Item -ItemType Directory -Path $logDirectory -Force)
$checks = [Collections.Generic.List[object]]::new()
$deadline = [DateTime]::UtcNow.AddMinutes($TimeoutMinutes)
$previousBuildJobs = $env:CARGO_BUILD_JOBS
# One compile job on a developer's machine, which is in use meanwhile; a CI runner (CI is set) uses all its cores.
if (-not $env:CARGO_BUILD_JOBS -and -not $env:CI) { $env:CARGO_BUILD_JOBS = '1' }
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
Set-NewFileOwnerToUser

# Every check after the dependency install runs even when an earlier one failed, and the run fails at the
# end with all of them: a failing documentation check stopped every run in its first minute, so no run said
# whether the code builds and its tests pass. A check that needs the output of one that did not pass is
# reported as skipped.
$notPassed = [Collections.Generic.HashSet[string]]::new()
function Invoke-Verification {
    param([string]$Name, [string]$Executable, [string[]]$Arguments, [string[]]$Needs = @(), [int]$TimeoutSeconds = 900)
    $missing = @($Needs | Where-Object { $notPassed.Contains($_) })
    if ($missing.Count) {
        Write-Host "[$Name] skipped: needs $($missing -join ', ')"
        [void]$checks.Add([ordered]@{ name = $Name; status = 'skipped'; exit_code = $null; seconds = 0 })
        [void]$notPassed.Add($Name)
        return
    }
    try { Invoke-Check $Name $Executable $Arguments $TimeoutSeconds }
    catch {
        [void]$notPassed.Add($Name)
        Write-Host "FAIL: $($_.Exception.Message)"
    }
}

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
    # Without the dependencies nothing else can run: this one still stops the run.
    Invoke-Check 'npm-install' 'npm.cmd' @('ci', '--no-audit', '--no-fund')
    Invoke-Verification 'plan' 'python' @('scripts/implementation.py', 'validate')
    Invoke-Verification 'runner-tests' 'pwsh' @('-NoProfile', '-File', 'scripts/test_verify.ps1')
    Invoke-Verification 'python-tests' 'python' @('-m', 'unittest', 'discover', '-s', 'scripts', '-p', 'test_*.py')
    Invoke-Verification 'frontend-tests' 'npm.cmd' @('test')
    Invoke-Verification 'frontend-build' 'npm.cmd' @('run', 'build')
    # Imago's canon: the names, values, keys and files the design file asks of an app (the "imago" block of
    # package.json says where they are). A finding fails the run. Then Sigrid's, with the rule own-class
    # (sigrid-check.config.json): no class, style or stylesheet of the app's own.
    Invoke-Verification 'imago-check' 'node' @('node_modules/@acaprino/imago/scripts/check/cli.mjs')
    Invoke-Verification 'sigrid-check' 'node' @('node_modules/@acaprino/sigrid/bin/sigrid-check.mjs', 'apps/desktop/src')
    # The smokes of the page: the window's page on Vite with the commands of Rust answered by src/mock, driven by roles and names (no class) at
    # the widths where the frame changes, in both schemes. The browser is the runner's Edge (tests/smoke-lib.mjs reads this variable): no step
    # downloads one. They need no build, only the installed packages; each fails on any console error but the pending-Sigrid warning it names.
    $env:SHINATAMA_BROWSER_CHANNEL = 'msedge'
    foreach ($area in @('workspace', 'inspector', 'sessions', 'team', 'settings', 'dialogs', 'widget', 'browser', 'app')) {
        Invoke-Verification "smoke-$area" 'npm.cmd' @('run', "test:$area") -TimeoutSeconds 600
    }
    Invoke-Verification 'dependencies' 'python' @('scripts/dependencies.py', 'check')
    Invoke-Verification 'rust-format' 'cargo' @('fmt', '--all', '--check')
    # A whole-workspace compile from nothing takes longer than the 15 minutes of the other checks.
    Invoke-Verification 'rust-clippy' 'cargo' @('clippy', '--locked', '--workspace', '--all-targets', '--features', 'custom-protocol', '--', '-D', 'warnings') -TimeoutSeconds 3600
    # Every test binary runs, so one failing target does not hide the others. The whole output of each check is
    # in .verification/logs, which the workflow uploads: the run's log shows only the end of a failed one.
    Invoke-Verification 'rust-tests' 'cargo' @('test', '--locked', '--workspace', '--all-targets', '--features', 'custom-protocol', '--no-fail-fast') -TimeoutSeconds 3600
    Invoke-Verification 'test-job-build' 'cargo' @('build', '--locked', '--manifest-path', 'verify/P14/Cargo.toml')
    # The desktop embeds the frontend that `frontend-build` writes.
    # The engine's program is the contained session reader (shinatama-session-helper), which reads the saved sessions.
    Invoke-Verification 'desktop-build' 'cargo' @('build', '--locked', '-p', 'shinatama-desktop', '-p', 'shinatama-cli', '-p', 'shinatama-terminal', '-p', 'shinatama-engine', '--bins', '--features', 'custom-protocol') -Needs @('frontend-build') -TimeoutSeconds 3600
    # The contained session reader as it is released (build-release.bat): its reads have a time limit and a tenth
    # of the processor, which an unoptimised reader does not meet on a real home. Uploaded, to be tried on one.
    Invoke-Verification 'session-helper-release' 'cargo' @('build', '--release', '--locked', '-p', 'shinatama-engine', '--features', 'session-reader', '--bin', 'shinatama-session-helper') -TimeoutSeconds 3600
    # The native probes (apps/desktop/tests/native): the built exe on a private data folder, driven over the WebView2 debugging port by roles and
    # names, inside the contained test job (P14). They prove what the mock smokes cannot: the caption buttons, the zoom and the glass of a window of
    # Shinatama's own, the threads and the files through the real Rust, the project catalog, the saved settings, the terminals of ConPTY and the
    # layout of a killed window. They use free ports, so a failing one leaves nothing for the next. The probes that start a real agent or open a
    # native dialog (claude-*, codex-*, forced-crash-recovery, picker) or need the contained session reader built with its feature (sessions) are run by hand.
    $probe = Join-Path $workspaceRoot 'verify/P14/target/debug/p14-probe.exe'
    $app = @('desktop-build', 'test-job-build')
    foreach ($native in @('window', 'workspace', 'threads', 'files', 'projects', 'settings')) {
        Invoke-Verification "native-$native" $probe @('wrap-run', 'node', "apps/desktop/tests/native/$native.mjs") -Needs $app
    }
    if (-not $notPassed.Contains('desktop-build')) {
        $report.executable = [ordered]@{
            path = 'target/debug/shinatama-desktop.exe'
            sha256 = (Get-FileHash -LiteralPath 'target/debug/shinatama-desktop.exe' -Algorithm SHA256).Hash.ToLowerInvariant()
            bytes = (Get-Item -LiteralPath 'target/debug/shinatama-desktop.exe').Length
            kind = 'embedded debug prototype; unsigned'
        }
    }
    if ($notPassed.Count) {
        throw "Checks that did not pass: $(@($checks | Where-Object { $_.status -ne 'passed' } | ForEach-Object { "$($_.name) ($($_.status))" }) -join ', ')"
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
