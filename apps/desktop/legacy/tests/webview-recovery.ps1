# The main window of one Shinatama process: rectangle, visibility and live browser surfaces.
# -Place moves it to a known rectangle first.
param([Parameter(Mandatory)][uint32]$AppProcessId, [switch]$Place)
$ErrorActionPreference = 'Stop'; $ProgressPreference = 'SilentlyContinue'
Add-Type @'
using System; using System.Text; using System.Runtime.InteropServices;
public static class RecoveryProbe {
  public delegate bool P(IntPtr h, IntPtr l); public struct R { public int l, t, r, b; }
  [DllImport("user32.dll")] static extern bool EnumWindows(P p, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr h, P p, IntPtr l);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out R r);
  [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h, int x, int y, int w, int height, bool repaint);
  public static IntPtr Window(uint pid) { IntPtr found = IntPtr.Zero; EnumWindows((h, l) => { uint p; GetWindowThreadProcessId(h, out p); var t = new StringBuilder(128); GetWindowText(h, t, 128);
    if (p == pid && t.ToString().Contains("terminal workspace")) found = h; return true; }, IntPtr.Zero); return found; }
  public static int Drawn(IntPtr h) { int n = 0; EnumChildWindows(h, (c, l) => { var s = new StringBuilder(128); GetClassName(c, s, 128); if (s.ToString().StartsWith("Chrome_WidgetWin")) n++; return true; }, IntPtr.Zero); return n; }
}
'@
$h = [RecoveryProbe]::Window($AppProcessId)
if ($Place) { [void][RecoveryProbe]::MoveWindow($h, 140, 90, 1100, 700, $true) }
$r = New-Object RecoveryProbe+R; [void][RecoveryProbe]::GetWindowRect($h, [ref]$r)
$browser = Get-CimInstance Win32_Process -Filter "Name='msedgewebview2.exe' AND ParentProcessId=$AppProcessId" | Where-Object { $_.CommandLine -notmatch '--type=' } | Select-Object -First 1
@{ found = ($h -ne [IntPtr]::Zero); visible = [RecoveryProbe]::IsWindowVisible($h); drawn = [RecoveryProbe]::Drawn($h)
   x = $r.l; y = $r.t; width = $r.r - $r.l; height = $r.b - $r.t; browser = [int]$browser.ProcessId } | ConvertTo-Json -Compress
