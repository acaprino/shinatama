param(
  [Parameter(Mandatory)][int]$AppProcessId,
  [Parameter(Mandatory)][ValidateSet('probe', 'press-maximize', 'hover-maximize', 'reload-menu', 'close-dialog', 'key')][string]$Action,
  [int]$VirtualKey = 0, [switch]$Control
)
$ErrorActionPreference = 'Stop'
# Test-only Win32 view of the main window of the process native-frame-smoke.mjs started: the
# maximize overlay (native_frame.rs), the window menu, owned message boxes, posted keys.
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class NativeFrameProbe {
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    public delegate bool EnumProc(IntPtr window, IntPtr data);
    [DllImport("user32.dll")] public static extern IntPtr FindWindowExW(IntPtr parent, IntPtr after, [MarshalAs(UnmanagedType.LPWStr)] string cls, [MarshalAs(UnmanagedType.LPWStr)] string name);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] public static extern bool ScreenToClient(IntPtr window, ref Point point);
    [DllImport("user32.dll")] public static extern IntPtr ChildWindowFromPointEx(IntPtr parent, Point point, uint flags);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassNameW(IntPtr window, StringBuilder name, int size);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowTextW(IntPtr window, StringBuilder text, int size);
    [DllImport("user32.dll")] public static extern IntPtr SendMessageW(IntPtr window, uint msg, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr window, uint msg, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern bool IsZoomed(IntPtr window);
    [DllImport("user32.dll")] public static extern bool IsWindowEnabled(IntPtr window);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] public static extern IntPtr GetSystemMenu(IntPtr window, bool revert);
    [DllImport("user32.dll")] public static extern int GetMenuItemCount(IntPtr menu);
    [DllImport("user32.dll")] public static extern uint GetMenuItemID(IntPtr menu, int position);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetMenuStringW(IntPtr menu, uint item, StringBuilder text, int size, uint flags);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc proc, IntPtr data);
    [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr parent, EnumProc proc, IntPtr data);
    [DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr window, uint command);
    public static string ClassOf(IntPtr window) { var name = new StringBuilder(256); GetClassNameW(window, name, 256); return name.ToString(); }
    public static string TextOf(IntPtr window) { var text = new StringBuilder(1024); GetWindowTextW(window, text, 1024); return text.ToString(); }
    public static List<IntPtr> Owned(IntPtr owner) {
        var found = new List<IntPtr>();
        EnumWindows((window, data) => { if (GetWindow(window, 4) == owner && ClassOf(window) == "#32770" && IsWindowVisible(window)) found.Add(window); return true; }, IntPtr.Zero);
        return found;
    }
    public static List<string> Texts(IntPtr parent) {
        var texts = new List<string>();
        EnumChildWindows(parent, (window, data) => { var text = TextOf(window); if (text.Length > 0) texts.Add(text); return true; }, IntPtr.Zero);
        return texts;
    }
    public static IntPtr Descendant(IntPtr parent, string cls) {
        IntPtr found = IntPtr.Zero;
        EnumChildWindows(parent, (window, data) => { if (ClassOf(window) == cls) { found = window; return false; } return true; }, IntPtr.Zero);
        return found;
    }
}
'@
$process = Get-Process -Id $AppProcessId
if ($process.ProcessName -ne 'shinatama-desktop') { throw 'Expected the Shinatama test process' }
$main = $process.MainWindowHandle
if ($main -eq 0) { throw 'The test window has no native handle' }
$P = [NativeFrameProbe]
$overlay = $P::FindWindowExW($main, [IntPtr]::Zero, 'ImagoMaximizeOverlay', [NullString]::Value)
function Center {
  $r = New-Object NativeFrameProbe+Rect; [void]$P::GetWindowRect($overlay, [ref]$r)
  [IntPtr](((([int](($r.Top + $r.Bottom) / 2)) -band 0xFFFF) -shl 16) -bor (([int](($r.Left + $r.Right) / 2)) -band 0xFFFF))
}
switch ($Action) {
  'probe' {
    $result = [ordered]@{ zoomed = $P::IsZoomed($main); enabled = $P::IsWindowEnabled($main); overlay = $overlay -ne 0 }
    if ($overlay -ne 0) {
      $r = New-Object NativeFrameProbe+Rect; [void]$P::GetWindowRect($overlay, [ref]$r)
      $topLeft = New-Object NativeFrameProbe+Point; $topLeft.X = $r.Left; $topLeft.Y = $r.Top; [void]$P::ScreenToClient($main, [ref]$topLeft)
      $center = New-Object NativeFrameProbe+Point; $center.X = $topLeft.X + [int](($r.Right - $r.Left) / 2); $center.Y = $topLeft.Y + [int](($r.Bottom - $r.Top) / 2)
      $result.overlayRect = @{ x = $topLeft.X; y = $topLeft.Y; width = $r.Right - $r.Left; height = $r.Bottom - $r.Top }
      $result.overlayVisible = $P::IsWindowVisible($overlay)
      $result.topChildAtCenter = $P::ClassOf($P::ChildWindowFromPointEx($main, $center, 1))
      $result.hitTest = $P::SendMessageW($overlay, 0x84, [IntPtr]::Zero, (Center)).ToInt64()
      $result.overlayName = $P::TextOf($overlay)
    }
    $menu = $P::GetSystemMenu($main, $false); $count = $P::GetMenuItemCount($menu)
    if ($count -gt 0) {
      $text = New-Object System.Text.StringBuilder 256; [void]$P::GetMenuStringW($menu, [uint32]($count - 1), $text, 256, 0x400)
      $result.lastMenuItem = @{ id = $P::GetMenuItemID($menu, $count - 1); text = $text.ToString() }
    }
    $result.dialogs = @($P::Owned($main) | ForEach-Object { , @($P::Texts($_)) })
    $result | ConvertTo-Json -Compress -Depth 5
  }
  'hover-maximize' { [void]$P::SendMessageW($overlay, 0x84, [IntPtr]::Zero, (Center)) }
  'press-maximize' {
    # WM_NCLBUTTONDOWN then WM_NCLBUTTONUP with HTMAXBUTTON, as a click on the button.
    [void]$P::SendMessageW($overlay, 0xA1, [IntPtr]9, (Center)); [void]$P::SendMessageW($overlay, 0xA2, [IntPtr]9, (Center))
  }
  'reload-menu' { [void]$P::PostMessageW($main, 0x112, [IntPtr]0x5350, [IntPtr]::Zero) }
  'close-dialog' { foreach ($dialog in $P::Owned($main)) { [void]$P::PostMessageW($dialog, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) } }
  'key' {
    $target = $P::Descendant($main, 'Chrome_RenderWidgetHostHWND')
    if ($target -eq 0) { throw 'No WebView2 input window' }
    if ($Control) { [void]$P::PostMessageW($target, 0x100, [IntPtr]0x11, [IntPtr]0x001D0001) }
    [void]$P::PostMessageW($target, 0x100, [IntPtr]$VirtualKey, [IntPtr]1); [void]$P::PostMessageW($target, 0x101, [IntPtr]$VirtualKey, [IntPtr]0xC0000001)
    if ($Control) { [void]$P::PostMessageW($target, 0x101, [IntPtr]0x11, [IntPtr]0xC01D0001) }
  }
}
