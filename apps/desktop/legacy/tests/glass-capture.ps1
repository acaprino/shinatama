param([int]$ProcessId, [string]$Out)
# Photographs a window from the screen: the only capture that includes the acrylic backdrop.
# A webview screenshot draws the backdrop as white. Prints the colour of three reference points.
Add-Type @'
using System; using System.Runtime.InteropServices;
public class Cap {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out R r);
  public struct R { public int L, T, Rr, B; }
}
'@
Add-Type -AssemblyName System.Drawing
[Cap]::SetProcessDPIAware() | Out-Null
$h = (Get-Process -Id $ProcessId).MainWindowHandle
[Cap]::SetWindowPos($h, [IntPtr](-1), 0, 0, 0, 0, 0x13) | Out-Null   # topmost, no move, no size, no activate
Start-Sleep -Milliseconds 700
$r = New-Object Cap+R; [Cap]::GetWindowRect($h, [ref]$r) | Out-Null
$w = $r.Rr - $r.L; $hh = $r.B - $r.T
$bmp = New-Object System.Drawing.Bitmap $w, $hh
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($r.L, $r.T, 0, 0, (New-Object System.Drawing.Size $w, $hh))
$bmp.Save($Out)
$mid = $bmp.GetPixel([int]($w * 0.62), [int]($hh * 0.62)); $bar = $bmp.GetPixel([int]($w * 0.5), 8); $side = $bmp.GetPixel(60, [int]($hh * 0.6))
"size ${w}x${hh} terminal $($mid.R),$($mid.G),$($mid.B) titlebar $($bar.R),$($bar.G),$($bar.B) sidebar $($side.R),$($side.G),$($side.B)"
$g.Dispose(); $bmp.Dispose()
[Cap]::SetWindowPos($h, [IntPtr](-2), 0, 0, 0, 0, 0x13) | Out-Null   # not topmost
