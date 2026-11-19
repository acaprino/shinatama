param([Parameter(Mandatory)][int]$AppProcessId, [long]$WindowHandle = 0, [switch]$Restore)
$ErrorActionPreference = 'Stop'
# Test-only inspection of the window owned by the process spawned by desktop-smoke.
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class PrototypeWindowProbe {
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr window);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr window, ref Point point);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr window, int command);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);
}
'@
$appProcess = Get-Process -Id $AppProcessId
if ($appProcess.ProcessName -ne 'shinatama-desktop') { throw 'Expected the Shinatama test process' }
$nativeHandle = if ($WindowHandle) { [IntPtr]$WindowHandle } else { $appProcess.MainWindowHandle }
if ($nativeHandle -eq 0) { throw 'The test window has no native handle' }
[uint32]$ownerProcessId = 0
[void][PrototypeWindowProbe]::GetWindowThreadProcessId($nativeHandle, [ref]$ownerProcessId)
if ($ownerProcessId -ne $AppProcessId) { throw 'Window does not belong to the test process' }
if ($Restore) { [void][PrototypeWindowProbe]::ShowWindow($nativeHandle, 9) }
$bounds = New-Object PrototypeWindowProbe+Rect
$clientOrigin = New-Object PrototypeWindowProbe+Point
[void][PrototypeWindowProbe]::GetWindowRect($nativeHandle, [ref]$bounds)
[void][PrototypeWindowProbe]::ClientToScreen($nativeHandle, [ref]$clientOrigin)
@{
    minimized = [PrototypeWindowProbe]::IsIconic($nativeHandle)
    # Tao can retain WS_CAPTION for OS behavior while removing its non-client rendering.
    # Measure the actual client inset instead of treating style bits as visible chrome.
    topInset = $clientOrigin.Y - $bounds.Top
    title = $appProcess.MainWindowTitle
    handle = $nativeHandle.ToInt64()
} | ConvertTo-Json -Compress
