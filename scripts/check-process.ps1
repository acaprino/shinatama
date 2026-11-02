# Shared bounded native process runner; dot-sourced by verification and its fault probes.
function Invoke-Check {
    param([string]$Name, [string]$Executable, [string[]]$Arguments, [int]$TimeoutSeconds = 900)
    Write-Host "[$Name]"
    $timer = [Diagnostics.Stopwatch]::StartNew()
    $entry = [ordered]@{ name = $Name; status = 'running'; exit_code = $null; seconds = 0 }
    [void]$checks.Add($entry)
    $process = [Diagnostics.Process]::new()
    $started = $false
    $stdout = $null; $stderr = $null
    $copyCancellation = [Threading.CancellationTokenSource]::new()
    try {
        $info = [Diagnostics.ProcessStartInfo]::new()
        $info.WorkingDirectory = $workspaceRoot
        $info.UseShellExecute = $false
        $info.CreateNoWindow = $true
        $info.RedirectStandardOutput = $true
        $info.RedirectStandardError = $true
        if ($Executable -eq 'npm.cmd') {
            # cmd is used only for npm's Windows shim. Every argument is a fixed,
            # whitespace-free token; never interpolate a path or arbitrary command.
            if (@($Arguments | Where-Object { $_ -notmatch '^[A-Za-z0-9_./:@=-]+$' }).Count) {
                throw 'Unsupported npm argument; do not pass shell expressions to verification'
            }
            $info.FileName = $env:ComSpec
            foreach ($argument in @('/d', '/s', '/c', ('npm.cmd ' + ($Arguments -join ' ')))) {
                $info.ArgumentList.Add($argument)
            }
        } else {
            $info.FileName = (Get-Command $Executable -ErrorAction Stop).Source
            foreach ($argument in $Arguments) { $info.ArgumentList.Add($argument) }
        }
        $process.StartInfo = $info
        [void]$process.Start()
        $started = $true
        $stdout = [IO.File]::Create((Join-Path $logDirectory "$Name.stdout.log"))
        $stderr = [IO.File]::Create((Join-Path $logDirectory "$Name.stderr.log"))
        $stdoutTask = $process.StandardOutput.BaseStream.CopyToAsync($stdout, $copyCancellation.Token)
        $stderrTask = $process.StandardError.BaseStream.CopyToAsync($stderr, $copyCancellation.Token)
        while (-not $process.WaitForExit(250)) {
            if ($timer.Elapsed.TotalSeconds -gt $TimeoutSeconds -or [DateTime]::UtcNow -ge $deadline) {
                $entry.exit_code = 124
                throw "Check '$Name' exceeded its local timeout"
            }
            if ($stdout.Length + $stderr.Length -gt 32MB) {
                $entry.exit_code = 125
                throw "Check '$Name' exceeded the 32 MiB log budget"
            }
        }
        if (-not [Threading.Tasks.Task]::WaitAll(@($stdoutTask, $stderrTask), 5000)) {
            throw "Check '$Name' left output pipes open after exit"
        }
        if ($stdout.Length + $stderr.Length -gt 32MB) {
            $entry.exit_code = 125
            throw "Check '$Name' exceeded the 32 MiB log budget"
        }
        $entry.exit_code = $process.ExitCode
        if ($process.ExitCode -ne 0) { throw "Check '$Name' exited with code $($process.ExitCode)" }
        $entry.status = 'passed'
    } catch {
        $entry.status = 'failed'
        throw
    } finally {
        if ($started -and -not $process.HasExited) {
            $process.Kill($true)
            [void]$process.WaitForExit(5000)
        }
        $copyCancellation.Cancel()
        if ($stdout) { $stdout.Dispose() }
        if ($stderr) { $stderr.Dispose() }
        $copyCancellation.Dispose()
        $process.Dispose()
        $entry.seconds = [Math]::Round($timer.Elapsed.TotalSeconds, 3)
        if ($entry.status -eq 'failed') {
            foreach ($suffix in @('stderr', 'stdout')) {
                $logPath = Join-Path $logDirectory "$Name.$suffix.log"
                if (Test-Path -LiteralPath $logPath) {
                    $reader = [IO.File]::OpenRead($logPath)
                    try {
                        [void]$reader.Seek([Math]::Max(0, $reader.Length - 4096), [IO.SeekOrigin]::Begin)
                        $bytes = [byte[]]::new(4096)
                        $length = $reader.Read($bytes, 0, $bytes.Length)
                        if ($length) { Write-Host ([Text.Encoding]::UTF8.GetString($bytes, 0, $length)) }
                    } finally { $reader.Dispose() }
                }
            }
        }
    }
}
