# Checks the overlay window: no focus steal, robot takes clicks, empty areas pass clicks through,
# and the session panel (open by clicking the robot) takes clicks without taking focus.
# Briefly moves the mouse; restores the cursor and stops the app afterwards.
# Build first: npx tauri build --no-bundle
Add-Type -AssemblyName System.Windows.Forms
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class U {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint x, uint y, uint d, UIntPtr e);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string cls, string title);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, System.Text.StringBuilder s, int n);
  [DllImport("user32.dll", EntryPoint="GetWindowLongPtrW")] public static extern IntPtr GetExStyle(IntPtr h, int i);
}
'@
[U]::SetProcessDPIAware() | Out-Null

function Pump($ms) {
  $end = (Get-Date).AddMilliseconds($ms)
  while ((Get-Date) -lt $end) { [System.Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 15 }
}
function Fg() {
  $sb = New-Object System.Text.StringBuilder 100
  [U]::GetWindowText([U]::GetForegroundWindow(), $sb, 100) | Out-Null
  "'" + $sb.ToString() + "'"
}
function Click($x, $y) {
  [U]::SetCursorPos($x, $y) | Out-Null
  Pump 400
  [U]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)
  [U]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
  Pump 500
}

$exe = (Resolve-Path (Join-Path $PSScriptRoot "..\target\release\desktop-assistant.exe")).Path
$start = [System.Windows.Forms.Cursor]::Position
$app = $null; $a = $null; $b = $null
try {
  # The overlay opens at 400,300 (200x200), so park the test windows clear of it.
  $a = New-Object System.Windows.Forms.Form -Property @{ Text = "Under"; TopMost = $true; StartPosition = "Manual"; Left = 300; Top = 200; Width = 500; Height = 500 }
  $b = New-Object System.Windows.Forms.Form -Property @{ Text = "Typing"; TopMost = $true; StartPosition = "Manual"; Left = 1000; Top = 200; Width = 400; Height = 300 }
  $box = New-Object System.Windows.Forms.TextBox -Property @{ Left = 10; Top = 10; Width = 300 }
  $b.Controls.Add($box)
  $script:underClicks = 0
  $a.add_MouseDown({ $script:underClicks++ })
  $a.Show(); $b.Show(); Pump 300

  $app = Start-Process $exe -PassThru
  $hwnd = [IntPtr]::Zero
  for ($i = 0; $i -lt 40 -and $hwnd -eq [IntPtr]::Zero; $i++) { Pump 250; $hwnd = [U]::FindWindow([NullString]::Value, "DesktopAssistant") }
  if ($hwnd -eq [IntPtr]::Zero) { throw "overlay window not found" }
  Pump 2000
  $r = New-Object U+RECT
  [U]::GetWindowRect($hwnd, [ref]$r) | Out-Null
  $x = $r.L; $y = $r.T; $w = $r.R - $r.L; $h = $r.B - $r.T
  "window: pos=$x,$y size=${w}x$h"
  $style = [U]::GetExStyle($hwnd, -20).ToInt64()
  "topmost={0} noactivate={1}" -f [bool]($style -band 0x8), [bool]($style -band 0x8000000)

  $typing = $b.Handle
  "launch did not steal focus: " + ([U]::GetForegroundWindow() -ne $hwnd)
  $boxPoint = $box.PointToScreen((New-Object System.Drawing.Point 5, 5))
  Click $boxPoint.X $boxPoint.Y
  "precondition, typing window focused: " + ([U]::GetForegroundWindow() -eq $typing)

  function Rect() { $q = New-Object U+RECT; [U]::GetWindowRect($hwnd, [ref]$q) | Out-Null; "$($q.L),$($q.T) $($q.R - $q.L)x$($q.B - $q.T)" }
  $cx = $x + [int]($w / 2); $cy = $y + [int]($h / 2)
  $s = $w / 200.0

  # 1. Clicking the robot opens the panel without stealing focus.
  $before = $script:underClicks
  Click $cx $cy
  "robot took the click (under-window count unchanged): " + ($script:underClicks -eq $before)
  "focus kept after robot click: " + ([U]::GetForegroundWindow() -eq $typing) + " (foreground is $(Fg))"
  $p = New-Object U+RECT
  [U]::GetWindowRect($hwnd, [ref]$p) | Out-Null
  $pw = $p.R - $p.L; $ph = $p.B - $p.T
  "panel opened: rect=$(Rect)"
  "panel window is 360x480 css px: " + (($pw -eq [int](360 * $s)) -and ($ph -eq [int](480 * $s)))
  $style = [U]::GetExStyle($hwnd, -20).ToInt64()
  "noactivate kept with panel open: " + [bool]($style -band 0x8000000)
  [System.Windows.Forms.SendKeys]::SendWait("x"); Pump 200
  "typing still lands in app: " + ($box.Text -eq "x")

  # 2. Clicking inside the panel (the part away from the robot) is taken by the panel.
  $robotTop = $y + [int](30 * $s)
  if ($p.T -eq $robotTop) { $py = $p.T + [int]((140 * $s + $ph) / 2) } else { $py = $p.T + [int](($ph - 140 * $s) / 2) }
  $before = $script:underClicks
  Click ($p.L + [int]($pw / 2)) $py
  "panel took the click (under-window count unchanged): " + ($script:underClicks -eq $before)
  "focus kept after panel click: " + ([U]::GetForegroundWindow() -eq $typing) + " (foreground is $(Fg))"
  [System.Windows.Forms.SendKeys]::SendWait("y"); Pump 200
  "typing still lands in app with panel open: " + ($box.Text -eq "xy")

  # 3. Clicking the robot again closes it and restores the original window.
  Click $cx $cy
  "panel closed at the original rect: " + ((Rect) -eq "$x,$y ${w}x$h") + " (rect=$(Rect))"
  "focus kept after closing: " + ([U]::GetForegroundWindow() -eq $typing)

  # 4. With the panel closed, an empty corner of the small window passes clicks through.
  $before = $script:underClicks
  Click ($x + 12) ($y + 12)
  "empty corner passed click through: " + ($script:underClicks -eq $before + 1)
}
finally {
  [System.Windows.Forms.Cursor]::Position = $start
  if ($a) { $a.Close() }
  if ($b) { $b.Close() }
  if ($app) { Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue }
}
