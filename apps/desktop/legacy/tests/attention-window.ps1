param([Parameter(Mandatory)][int]$AppProcessId)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Text;
using System.Runtime.InteropServices;
public static class AttentionWindowProbe {
  public delegate bool Callback(IntPtr window, IntPtr param);
  [DllImport("user32.dll")] public static extern bool EnumWindows(Callback callback, IntPtr param);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint owner);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr window, StringBuilder text, int count);
  [DllImport("user32.dll", EntryPoint="GetWindowLongPtrW")] public static extern IntPtr GetWindowLongPtr(IntPtr window, int index);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  public static object Read(int process) {
    var windows = new List<object>();
    EnumWindows((window, param) => {
      uint owner; GetWindowThreadProcessId(window, out owner);
      if (owner == process) {
        var text = new StringBuilder(512); GetWindowText(window, text, 512);
        long style = GetWindowLongPtr(window, -20).ToInt64();
        windows.Add(new { handle=window.ToInt64(), title=text.ToString(), visible=IsWindowVisible(window), noActivate=(style & 0x08000000)!=0, topmost=(style & 8)!=0 });
      }
      return true;
    }, IntPtr.Zero);
    return new { foreground=GetForegroundWindow().ToInt64(), windows=windows };
  }
}
'@
$owned = Get-Process -Id $AppProcessId
if ($owned.ProcessName -ne 'shinatama-desktop') { throw 'Expected owned Shinatama test process.' }
[AttentionWindowProbe]::Read($AppProcessId) | ConvertTo-Json -Compress -Depth 4
