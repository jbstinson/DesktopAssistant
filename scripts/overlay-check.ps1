# Checks the overlay window: no focus steal, robot takes clicks, empty areas pass clicks through,
# and the session panel (open by clicking the robot) takes clicks without taking focus.
# Also checks the Ctrl+Alt+D hotkey (hide, then show, without moving focus), dragging, and that
# the dragged position is remembered across a restart.
# Briefly moves the mouse; restores the cursor and stops the app afterwards.
# The app runs with APPDATA and USERPROFILE pointed at a temp folder, so it never touches the
# real state.json, endpoint.json or Claude Code settings.
# Build first: npx tauri build --no-bundle
Add-Type -AssemblyName System.Windows.Forms
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class U {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetClipCursor(out RECT r);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
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
$oldAppData = $env:APPDATA; $oldProfile = $env:USERPROFILE
$sandbox = Join-Path ([System.IO.Path]::GetTempPath()) ("da-overlay-check-" + $PID)
New-Item -ItemType Directory -Force (Join-Path $sandbox "appdata\DesktopAssistant"), (Join-Path $sandbox "profile") | Out-Null
$env:APPDATA = Join-Path $sandbox "appdata"; $env:USERPROFILE = Join-Path $sandbox "profile"
# Where the test can click. Normally the primary monitor, but if something confines the mouse to
# one monitor (as some remote-control tools do), use that area so the cursor can reach the windows.
$vs = [System.Windows.Forms.SystemInformation]::VirtualScreen
$clip = New-Object U+RECT; [U]::GetClipCursor([ref]$clip) | Out-Null
if ($clip.L -ne $vs.Left -or $clip.T -ne $vs.Top -or $clip.R -ne $vs.Right -or $clip.B -ne $vs.Bottom) {
  $areaL = $clip.L; $areaT = $clip.T + 100
} else {
  $p0 = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds; $areaL = $p0.Left; $areaT = $p0.Top
}
# Start the overlay at a known spot in that area through the saved position.
$overlayX = $areaL + 100; $overlayY = $areaT + 300
[System.IO.File]::WriteAllText((Join-Path $env:APPDATA "DesktopAssistant\state.json"), "{`"x`":$overlayX,`"y`":$overlayY}")
function Hotkey() {
  foreach ($k in 0x11, 0x12, 0x44) { [U]::keybd_event($k, 0, 0, [UIntPtr]::Zero) }
  foreach ($k in 0x44, 0x12, 0x11) { [U]::keybd_event($k, 0, 2, [UIntPtr]::Zero) }
  Pump 600
}
function Launch() {
  $script:app = Start-Process $exe -PassThru
  $h = [IntPtr]::Zero
  for ($i = 0; $i -lt 40 -and $h -eq [IntPtr]::Zero; $i++) { Pump 250; $h = [U]::FindWindow([NullString]::Value, "DesktopAssistant") }
  if ($h -eq [IntPtr]::Zero) { throw "overlay window not found" }
  Pump 2000
  $h
}
try {
  # The overlay starts at 100,300 inside the area (200x200); Under covers it, Typing is clear of it.
  $a = New-Object System.Windows.Forms.Form -Property @{ Text = "Under"; TopMost = $true; StartPosition = "Manual"; Left = $areaL; Top = $areaT + 200; Width = 500; Height = 500 }
  $b = New-Object System.Windows.Forms.Form -Property @{ Text = "Typing"; TopMost = $true; StartPosition = "Manual"; Left = $areaL + 600; Top = $areaT + 200; Width = 400; Height = 300 }
  $box = New-Object System.Windows.Forms.TextBox -Property @{ Left = 10; Top = 10; Width = 300 }
  $b.Controls.Add($box)
  $script:underClicks = 0
  $a.add_MouseDown({ $script:underClicks++ })
  $a.Show(); $b.Show(); Pump 300

  $hwnd = Launch
  $r = New-Object U+RECT
  [U]::GetWindowRect($hwnd, [ref]$r) | Out-Null
  $x = $r.L; $y = $r.T; $w = $r.R - $r.L; $h = $r.B - $r.T
  "window: pos=$x,$y size=${w}x$h"
  $style = [U]::GetExStyle($hwnd, -20).ToInt64()
  "topmost={0} noactivate={1}" -f [bool]($style -band 0x8), [bool]($style -band 0x8000000)

  $typing = $b.Handle
  "launch did not steal focus: " + ([U]::GetForegroundWindow() -ne $hwnd)
  $boxPoint = $box.PointToScreen((New-Object System.Drawing.Point 5, 5))
  for ($i = 0; $i -lt 3 -and [U]::GetForegroundWindow() -ne $typing; $i++) { Click $boxPoint.X $boxPoint.Y }
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

  # 5. The hotkey hides and shows the overlay without ever taking focus.
  Click $boxPoint.X $boxPoint.Y
  "precondition, typing window focused: " + ([U]::GetForegroundWindow() -eq $typing)
  $box.SelectionStart = $box.Text.Length   # the click put the caret mid-text
  $textBefore = $box.Text
  Hotkey
  "hotkey hid the window: " + (-not [U]::IsWindowVisible($hwnd))
  "focus kept after hide: " + ([U]::GetForegroundWindow() -eq $typing) + " (foreground is $(Fg))"
  [System.Windows.Forms.SendKeys]::SendWait("q"); Pump 200
  "typing lands while hidden: " + ($box.Text -eq $textBefore + "q") + " (text='$($box.Text)')"
  Hotkey
  "hotkey showed the window: " + [U]::IsWindowVisible($hwnd)
  "focus kept after show: " + ([U]::GetForegroundWindow() -eq $typing) + " (foreground is $(Fg))"
  $style = [U]::GetExStyle($hwnd, -20).ToInt64()
  "noactivate kept after show: " + [bool]($style -band 0x8000000)
  "topmost kept after show: " + [bool]($style -band 0x8)
  "same rect after show: " + ((Rect) -eq "$x,$y ${w}x$h") + " (rect=$(Rect))"
  [System.Windows.Forms.SendKeys]::SendWait("r"); Pump 200
  "typing lands after show: " + ($box.Text -eq $textBefore + "qr") + " (text='$($box.Text)')"

  # 6. Holding the mouse on the robot drags the window; focus stays put and the spot is saved.
  # (The click-through poll wakes up to half a second after the window is shown again.)
  Pump 800
  [U]::SetCursorPos($cx, $cy) | Out-Null; Pump 300
  [U]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero); Pump 400
  for ($i = 1; $i -le 10; $i++) { [U]::SetCursorPos($cx + 15 * $i, $cy + 10 * $i) | Out-Null; Pump 40 }
  Pump 200
  [U]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero); Pump 800
  $d = New-Object U+RECT
  [U]::GetWindowRect($hwnd, [ref]$d) | Out-Null
  "dragged by (150,100): " + (($d.L - $x) -eq 150 -and ($d.T - $y) -eq 100) + " (rect=$(Rect))"
  "panel stayed closed after drag: " + ((($d.R - $d.L) -eq $w) -and (($d.B - $d.T) -eq $h))
  "focus kept after drag: " + ([U]::GetForegroundWindow() -eq $typing) + " (foreground is $(Fg))"
  Pump 1000
  $stateFile = Join-Path $env:APPDATA "DesktopAssistant\state.json"
  $saved = if (Test-Path $stateFile) { Get-Content $stateFile -Raw } else { "" }
  "position saved: " + ($saved -eq "{`"x`":$($d.L),`"y`":$($d.T)}") + " ($saved)"

  # 7. A restart puts the window back where it was dragged.
  Stop-Process -Id $app.Id -Force; Pump 800
  $hwnd = Launch
  "restart restored the position: " + ((Rect) -eq "$($d.L),$($d.T) ${w}x$h") + " (rect=$(Rect))"
  "launch did not steal focus: " + ([U]::GetForegroundWindow() -ne $hwnd)

  # 8. A damaged state.json is ignored: the window lands in the primary monitor's corner.
  Stop-Process -Id $app.Id -Force; Pump 800
  Set-Content $stateFile "{ broken"
  $hwnd = Launch
  $m = [System.Windows.Forms.Screen]::PrimaryScreen.WorkingArea
  "malformed state.json put the window bottom-right on the primary: " + ((Rect) -eq "$($m.Right - $w),$($m.Bottom - $h) ${w}x$h") + " (rect=$(Rect))"
}
finally {
  [System.Windows.Forms.Cursor]::Position = $start
  if ($a) { $a.Close() }
  if ($b) { $b.Close() }
  if ($app) { Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue }
  $env:APPDATA = $oldAppData; $env:USERPROFILE = $oldProfile
  Remove-Item $sandbox -Recurse -Force -ErrorAction SilentlyContinue
}
