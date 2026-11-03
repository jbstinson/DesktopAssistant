# Checks the overlay window: no focus steal, robot takes clicks, empty areas pass clicks through,
# and the session panel (open by clicking the robot) takes clicks without taking focus.
# Also checks the Ctrl+Alt+D hotkey (hide, then show, without moving focus), dragging, and that
# the dragged position is remembered across a restart.
# Briefly moves the mouse; restores the cursor and stops the app afterwards.
# The app runs with APPDATA and USERPROFILE pointed at a temp folder, so it never touches the
# real state.json, endpoint.json or Claude Code settings.
# Also checks approval requests: the panel opens by itself without taking focus, a click while
# the buttons are locked is ignored, and an accepted click answers the request.
# Build first: npx tauri build --no-bundle. Set DA_EXE to use an exe built somewhere else
# (for example under a custom CARGO_TARGET_DIR).
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

# Requests posted to the app must be polled, as the hook does, or the app drops them after 3 seconds.
$script:ids = @{}; $script:results = @{}; $script:nextPoll = Get-Date; $script:hdr = $null; $script:port = 0
function PollAll {
  foreach ($id in @($script:ids.Keys)) {
    $r = Invoke-WebRequest -Uri "http://127.0.0.1:$($script:port)/permission/$id" -Headers $script:hdr -UseBasicParsing -SkipHttpErrorCheck
    if ([int]$r.StatusCode -ne 204) { $script:results[$id] = "$([int]$r.StatusCode) $($r.Content)"; $script:ids.Remove($id) }
  }
}
function Pump($ms) {
  $end = (Get-Date).AddMilliseconds($ms)
  while ((Get-Date) -lt $end) {
    [System.Windows.Forms.Application]::DoEvents()
    if ($script:ids.Count -gt 0 -and (Get-Date) -ge $script:nextPoll) { PollAll; $script:nextPoll = (Get-Date).AddMilliseconds(250) }
    Start-Sleep -Milliseconds 15
  }
}
function Ask($cmd) {
  $b = [ordered]@{ session_id = "s1"; cwd = "C:\git\myproject"; source = "cli"; tool_name = "Bash"; tool_input = @{ command = $cmd }
    permission_suggestions = @(@{ type = "addRules"; rules = @(@{ toolName = "Bash"; ruleContent = $cmd }); behavior = "allow"; destination = "localSettings" }) }
  $r = Invoke-RestMethod -Method Post -Uri "http://127.0.0.1:$($script:port)/permission" -Headers $script:hdr -Body ($b | ConvertTo-Json -Depth 10 -Compress) -ContentType "application/json"
  $script:ids[$r.id] = $true
  $r.id
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

$exe = if ($env:DA_EXE) { $env:DA_EXE } else { (Resolve-Path (Join-Path $PSScriptRoot "..\target\release\desktop-assistant.exe")).Path }
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
  "panel window is 360x510 css px: " + (($pw -eq [int](360 * $s)) -and ($ph -eq [int](510 * $s)))
  $style = [U]::GetExStyle($hwnd, -20).ToInt64()
  "noactivate kept with panel open: " + [bool]($style -band 0x8000000)
  [System.Windows.Forms.SendKeys]::SendWait("x"); Pump 200
  "typing still lands in app: " + ($box.Text -eq "x")

  # 2. Clicking inside the panel (the part away from the robot) is taken by the panel.
  # The robot keeps 30 css px of hat room above it, so with the panel below, the window top is that far above the robot.
  $robotTop = $y + [int](30 * $s)
  $robotAndRoom = [int](170 * $s)
  if ($p.T -eq $robotTop - [int](30 * $s)) { $py = $p.T + [int](($robotAndRoom + $ph) / 2) } else { $py = $p.T + [int](($ph - $robotAndRoom) / 2) }
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

  # 9. An approval request opens the panel by itself without taking focus; a click while the
  # buttons are locked is ignored; a click after the lock answers the request.
  Stop-Process -Id $app.Id -Force; Pump 800
  [System.IO.File]::WriteAllText($stateFile, "{`"x`":$overlayX,`"y`":$overlayY}")
  $hwnd = Launch
  $ep = Get-Content (Join-Path $env:APPDATA "DesktopAssistant\endpoint.json") -Raw | ConvertFrom-Json
  $script:port = $ep.port; $script:hdr = @{ Authorization = "Bearer $($ep.token)" }
  $r = New-Object U+RECT; [U]::GetWindowRect($hwnd, [ref]$r) | Out-Null
  $x = $r.L; $y = $r.T; $w = $r.R - $r.L; $h = $r.B - $r.T; $s = $w / 200.0
  Click $boxPoint.X $boxPoint.Y
  $box.Text = ""; $box.SelectionStart = 0
  "approval precondition, typing window focused: " + ([U]::GetForegroundWindow() -eq $typing)
  $first = Ask "node first.js"
  $second = Ask "node second.js"
  Pump 2000
  [U]::GetWindowRect($hwnd, [ref]$r) | Out-Null
  $pw = $r.R - $r.L; $ph = $r.B - $r.T
  "request opened the panel by itself: " + (($pw -eq [int](360 * $s)) -and ($ph -eq [int](510 * $s))) + " (rect=$(Rect))"
  "auto-open kept focus: " + ([U]::GetForegroundWindow() -eq $typing) + " (foreground is $(Fg))"
  [System.Windows.Forms.SendKeys]::SendWait("a"); Pump 200
  "typing still lands after auto-open: " + ($box.Text -eq "a")
  $style = [U]::GetExStyle($hwnd, -20).ToInt64()
  "noactivate kept with a request showing: " + [bool]($style -band 0x8000000)
  # Deny sits at the far left of the button row, 50 px above the window's bottom when the panel
  # is below the robot (the window then starts at the hat room above the robot), otherwise
  # 225 px above it.
  $denyX = $r.L + [int](38 * $s)
  $denyY = if ($r.T -gt $y - [int](170 * $s)) { $r.T + [int](460 * $s) } else { $r.T + [int](285 * $s) }
  # Always allow asks twice. A double-click on it must answer nothing: the check that replaces the
  # row keeps Confirm at the far left, clear of where Always allow was, and Cancel is harmless.
  $aaX = $r.L + [int](299 * $s)
  Click $aaX $denyY
  [U]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero); [U]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero); Pump 1500
  "double-click on Always allow answered nothing: " + (-not $script:results.ContainsKey($first))
  Click ($r.L + [int](316 * $s)) ($denyY + [int](25 * $s))
  Pump 1300
  "Cancel went back to the buttons, nothing answered: " + (-not $script:results.ContainsKey($first))
  "focus kept through Always allow and Cancel: " + ([U]::GetForegroundWindow() -eq $typing) + " (foreground is $(Fg))"
  # The cursor came from outside the panel, so after the lock a click is accepted.
  Click $denyX $denyY
  "accepted click answered the first request: " + ($script:results[$first] -like '200*deny*') + " ($($script:results[$first]))"
  "focus kept after accepted click: " + ([U]::GetForegroundWindow() -eq $typing) + " (foreground is $(Fg))"
  [System.Windows.Forms.SendKeys]::SendWait("b"); Pump 200
  "typing still lands after accepted click: " + ($box.Text -eq "ab")
  # The second request became current with the cursor already over the panel: still locked.
  Pump 2500
  Click $denyX $denyY
  "locked click ignored (second request still waiting): " + (-not $script:results.ContainsKey($second))
  "focus kept after locked click: " + ([U]::GetForegroundWindow() -eq $typing) + " (foreground is $(Fg))"
  # Leaving the panel and coming back unlocks it.
  [U]::SetCursorPos($areaL + 900, $areaT + 100) | Out-Null; Pump 600
  Click $denyX $denyY
  "click after leaving and coming back answered it: " + ($script:results[$second] -like '200*deny*')
  Pump 800
  [U]::GetWindowRect($hwnd, [ref]$r) | Out-Null
  "panel closed itself when no requests remained: " + ((Rect) -eq "$x,$y ${w}x$h") + " (rect=$(Rect))"
  "focus kept at the end: " + ([U]::GetForegroundWindow() -eq $typing) + " (foreground is $(Fg))"
}
finally {
  [System.Windows.Forms.Cursor]::Position = $start
  if ($a) { $a.Close() }
  if ($b) { $b.Close() }
  if ($app) { Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue }
  $env:APPDATA = $oldAppData; $env:USERPROFILE = $oldProfile
  Remove-Item $sandbox -Recurse -Force -ErrorAction SilentlyContinue
}
