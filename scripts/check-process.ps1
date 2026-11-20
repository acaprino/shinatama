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

# A Windows Server runner signs in an administrator, and what an administrator creates there belongs to the
# Administrators group. Shinatama refuses a data folder that does not belong to the user, so 15 test targets
# failed on that alone in every CI run up to 2026-11-20. What this process and its children create belongs to
# the user from here on, as on a desktop; nothing is changed where it already does.
function Set-NewFileOwnerToUser {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    if ($identity.Owner.Equals($identity.User)) { return }
    $native = Add-Type -Namespace ShinatamaVerify -Name ProcessToken -PassThru -MemberDefinition @'
[DllImport("kernel32.dll")] public static extern IntPtr GetCurrentProcess();
[DllImport("kernel32.dll")] public static extern bool CloseHandle(IntPtr handle);
[DllImport("advapi32.dll", SetLastError = true)] public static extern bool OpenProcessToken(IntPtr process, uint access, out IntPtr token);
[DllImport("advapi32.dll", SetLastError = true)] public static extern bool SetTokenInformation(IntPtr token, int kind, IntPtr information, int length);
'@
    $token = [IntPtr]::Zero
    # TOKEN_ADJUST_DEFAULT
    if (-not $native::OpenProcessToken($native::GetCurrentProcess(), 0x0080, [ref]$token)) {
        throw "The process token could not be opened: error $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
    }
    $sid = [byte[]]::new($identity.User.BinaryLength)
    $identity.User.GetBinaryForm($sid, 0)
    # TOKEN_OWNER: one pointer to the SID, which follows it here.
    $memory = [Runtime.InteropServices.Marshal]::AllocHGlobal([IntPtr]::Size + $sid.Length)
    try {
        $sidAddress = [IntPtr]::Add($memory, [IntPtr]::Size)
        [Runtime.InteropServices.Marshal]::Copy($sid, 0, $sidAddress, $sid.Length)
        [Runtime.InteropServices.Marshal]::WriteIntPtr($memory, $sidAddress)
        # 4 = TokenOwner
        if (-not $native::SetTokenInformation($token, 4, $memory, [IntPtr]::Size)) {
            throw "The owner of new files could not be set to the user: error $([Runtime.InteropServices.Marshal]::GetLastWin32Error())"
        }
    } finally {
        [Runtime.InteropServices.Marshal]::FreeHGlobal($memory)
        [void]$native::CloseHandle($token)
    }
    $after = [Security.Principal.WindowsIdentity]::GetCurrent()
    if (-not $after.Owner.Equals($after.User)) { throw 'New files still do not belong to the user' }
    Write-Host "New files belong to the user ($($identity.Owner.Value) before)."
}
