# Spike test: focus, click-through and resource use of the overlay window.
Add-Type -AssemblyName System.Windows.Forms
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class U {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint x, uint y, uint d, UIntPtr e);
  [DllImport("user32.dll", EntryPoint="GetWindowLongPtrW")] public static extern IntPtr GetExStyle(IntPtr h, int i);
}
'@
[U]::SetProcessDPIAware() | Out-Null

function Pump($ms) {
  $end = (Get-Date).AddMilliseconds($ms)
  while ((Get-Date) -lt $end) { [System.Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 15 }
}
function Click($x, $y) {
  [U]::SetCursorPos($x, $y) | Out-Null
  Pump 400
  [U]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)
  [U]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
  Pump 500
}
function Tree($rootId) {
  $all = Get-CimInstance Win32_Process
  $ids = @($rootId); $grew = $true
  while ($grew) {
    $next = $all | Where-Object { $ids -contains $_.ParentProcessId -and $ids -notcontains $_.ProcessId } | ForEach-Object ProcessId
    $grew = [bool]$next; if ($next) { $ids += $next }
  }
  $ids | ForEach-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue }
}
function Measure-Tree($rootId, $seconds) {
  $before = (Tree $rootId | Measure-Object CPU -Sum).Sum
  Start-Sleep -Seconds $seconds
  $tree = Tree $rootId
  $cpu = (($tree | Measure-Object CPU -Sum).Sum - $before) / $seconds / [Environment]::ProcessorCount * 100
  $private = ($tree | Measure-Object PrivateMemorySize64 -Sum).Sum / 1MB
  $working = ($tree | Measure-Object WorkingSet64 -Sum).Sum / 1MB
  "processes={0} cpu={1:N2}% private={2:N0}MB workingSet={3:N0}MB" -f $tree.Count, $cpu, $private, $working
}

$exe = (Resolve-Path "src-tauri/target/release/window-spike.exe").Path
Remove-Item spike.log -ErrorAction SilentlyContinue
$start = [System.Windows.Forms.Cursor]::Position

$a = New-Object System.Windows.Forms.Form -Property @{ Text = "Under"; StartPosition = "Manual"; Left = 300; Top = 200; Width = 500; Height = 500 }
$b = New-Object System.Windows.Forms.Form -Property @{ Text = "Typing"; StartPosition = "Manual"; Left = 1000; Top = 200; Width = 400; Height = 300 }
$box = New-Object System.Windows.Forms.TextBox -Property @{ Left = 10; Top = 10; Width = 300 }
$b.Controls.Add($box)
$script:underClicks = 0
$a.add_MouseDown({ $script:underClicks++ })
$a.Show(); $b.Show(); Pump 300

$app = Start-Process $exe -PassThru -WorkingDirectory $PWD
Pump 3000
$ready = Get-Content spike.log | Where-Object { $_ -like "ready*" } | Select-Object -First 1
"log: $ready"
if ($ready -notmatch 'pos=(-?\d+),(-?\d+) size=(\d+)x(\d+).*hwnd=(-?\d+)') { throw "overlay did not report ready" }
$x, $y, $w, $h, $hwnd = [int]$Matches[1], [int]$Matches[2], [int]$Matches[3], [int]$Matches[4], [long]$Matches[5]
$style = [U]::GetExStyle([IntPtr]$hwnd, -20).ToInt64()
"topmost={0} noactivate={1}" -f [bool]($style -band 0x8), [bool]($style -band 0x8000000)

$typing = $b.Handle
"launch did not steal focus: " + ([U]::GetForegroundWindow() -ne [IntPtr]$hwnd)
$boxPoint = $box.PointToScreen((New-Object System.Drawing.Point 5, 5))
Click $boxPoint.X $boxPoint.Y
"precondition, typing window focused: " + ([U]::GetForegroundWindow() -eq $typing)

Click ($x + [int]($w / 2)) ($y + [int]($h / 2))
"robot received click: " + [bool](Get-Content spike.log | Where-Object { $_ -eq "robot clicked" })
$now = [U]::GetForegroundWindow()
"foreground after robot click: {0} (typing={1}, overlay={2})" -f $now, $typing, $hwnd
"focus kept after robot click: " + ($now -eq $typing)
[System.Windows.Forms.SendKeys]::SendWait("x"); Pump 200
"typing still lands in app: " + ($box.Text -eq "x")

Click ($x + 12) ($y + 12)
"empty corner passed click through: " + ($script:underClicks -eq 1)

[System.Windows.Forms.Cursor]::Position = $start
$a.Close(); $b.Close()

Start-Sleep -Seconds 5
"static: " + (Measure-Tree $app.Id 20)
Stop-Process -Id $app.Id -Force

$env:SPIKE_ANIMATE = "1"
$animated = Start-Process $exe -PassThru -WorkingDirectory $PWD
Start-Sleep -Seconds 8
"animated: " + (Measure-Tree $animated.Id 20)
Stop-Process -Id $animated.Id -Force
Remove-Item env:SPIKE_ANIMATE
