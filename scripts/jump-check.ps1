# Checks click-to-jump against a real terminal window: opens a new PowerShell window, tells a
# sandboxed app instance about a "cli" session whose terminal_pid is that window's process,
# focuses a different window, clicks the session's row in the panel, and reports whether the
# terminal came forward or only flashed. Closes only the window it opened.
# The app runs with APPDATA and USERPROFILE pointed at a temp folder, so it never touches the
# real state.json, endpoint.json or Claude Code settings.
# Build first: npx tauri build --no-bundle
# -Shot <png> saves a picture of the open panel (to find the row) and skips the click.
# -Desktop instead reports a claude-desktop session and checks the Claude desktop app comes forward.
param([string]$Shot = "", [double]$RowX = 0.5, [double]$RowY = 0.5, [switch]$Desktop)
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class J {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint x, uint y, uint d, UIntPtr e);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string cls, string title);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, System.Text.StringBuilder s, int n);
}
'@
[J]::SetProcessDPIAware() | Out-Null
function Pump($ms) {
  $end = (Get-Date).AddMilliseconds($ms)
  while ((Get-Date) -lt $end) { [System.Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 15 }
}
function Click($x, $y) {
  [J]::SetCursorPos($x, $y) | Out-Null; Pump 400
  [J]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero); [J]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero); Pump 600
}
function FgText() {
  $h = [J]::GetForegroundWindow(); $sb = New-Object System.Text.StringBuilder 100
  [J]::GetWindowText($h, $sb, 100) | Out-Null; $p = 0; [J]::GetWindowThreadProcessId($h, [ref]$p) | Out-Null
  "'" + $sb.ToString() + "' (pid $p)"
}

$exe = (Resolve-Path (Join-Path $PSScriptRoot "..\target\release\desktop-assistant.exe")).Path
$start = [System.Windows.Forms.Cursor]::Position
$app = $null; $term = $null; $other = $null
$oldAppData = $env:APPDATA; $oldProfile = $env:USERPROFILE
$sandbox = Join-Path ([System.IO.Path]::GetTempPath()) ("da-jump-check-" + $PID)
New-Item -ItemType Directory -Force (Join-Path $sandbox "appdata\DesktopAssistant"), (Join-Path $sandbox "profile") | Out-Null
$env:APPDATA = Join-Path $sandbox "appdata"; $env:USERPROFILE = Join-Path $sandbox "profile"
$errFile = Join-Path $sandbox "stderr.txt"
$vs = [System.Windows.Forms.SystemInformation]::VirtualScreen
$p0 = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$overlayX = $p0.Left + 100; $overlayY = $p0.Top + 300
[System.IO.File]::WriteAllText((Join-Path $env:APPDATA "DesktopAssistant\state.json"), "{`"x`":$overlayX,`"y`":$overlayY}")
try {
  # The terminal under test: a plain console host so the window belongs to the process we started.
  if (-not $Desktop) {
  $term = Start-Process conhost.exe -ArgumentList "powershell.exe -NoExit -Command `$Host.UI.RawUI.WindowTitle='jump-check terminal'" -PassThru
  Pump 2500
  $termWin = [IntPtr]::Zero
  for ($i = 0; $i -lt 20 -and $termWin -eq [IntPtr]::Zero; $i++) { Pump 250; $termWin = [J]::FindWindow([NullString]::Value, "jump-check terminal") }
  if ($termWin -eq [IntPtr]::Zero) { throw "terminal window not found" }
  $owner = 0; [J]::GetWindowThreadProcessId($termWin, [ref]$owner) | Out-Null
  "terminal window owned by pid $owner (started pid $($term.Id))"
  }

  $app = Start-Process $exe -PassThru -RedirectStandardError $errFile
  $h = [IntPtr]::Zero
  for ($i = 0; $i -lt 40 -and $h -eq [IntPtr]::Zero; $i++) { Pump 250; $h = [J]::FindWindow([NullString]::Value, "DesktopAssistant") }
  if ($h -eq [IntPtr]::Zero) { throw "overlay window not found" }
  Pump 2000
  $ep = Get-Content (Join-Path $env:APPDATA "DesktopAssistant\endpoint.json") -Raw | ConvertFrom-Json
  $post = {
    param($body)
    Invoke-WebRequest -UseBasicParsing -Method Post -Uri "http://127.0.0.1:$($ep.port)/event" `
      -Headers @{ Authorization = "Bearer $($ep.token)" } -ContentType "application/json" -Body $body | Out-Null
  }
  if ($Desktop) {
    & $post (@{ name = "PreToolUse"; session_id = "desk-1"; cwd = "C:\demo\desktop"; tool = "Read"; source = "claude-desktop" } | ConvertTo-Json)
  } else {
    & $post (@{ name = "PreToolUse"; session_id = "jump-1"; cwd = "C:\demo\jumper"; tool = "Read"; source = "cli"; terminal_pid = [int]$owner } | ConvertTo-Json)
    & $post (@{ name = "PreToolUse"; session_id = "bg-1"; cwd = "C:\demo\background"; tool = "Read"; source = "sdk-cli"; terminal_pid = [int]$owner } | ConvertTo-Json)
  }
  Pump 500

  # Something else to hold the foreground, so the click has to move it.
  $other = New-Object System.Windows.Forms.Form -Property @{ Text = "jump-check other"; TopMost = $false; StartPosition = "Manual"; Left = $p0.Left + 700; Top = $p0.Top + 300; Width = 400; Height = 300 }
  $other.Show(); $other.Activate(); Pump 500

  $r = New-Object J+RECT; [J]::GetWindowRect($h, [ref]$r) | Out-Null
  $cx = ($r.L + $r.R) / 2; $cy = ($r.T + $r.B) / 2
  Click $cx $cy   # open the panel
  [J]::GetWindowRect($h, [ref]$r) | Out-Null
  $w = $r.R - $r.L; $ht = $r.B - $r.T
  "panel window: $($r.L),$($r.T) ${w}x$ht"
  if ($Shot) {
    $bmp = New-Object System.Drawing.Bitmap $w, $ht
    $g = [System.Drawing.Graphics]::FromImage($bmp); $g.CopyFromScreen($r.L, $r.T, 0, 0, $bmp.Size); $g.Dispose()
    $bmp.Save($Shot); $bmp.Dispose(); "saved $Shot"
    return
  }
  $other.Activate(); Pump 400
  "before click, foreground: $(FgText)"
  Click ($r.L + [int]($w * $RowX)) ($r.T + [int]($ht * $RowY))
  "after click, foreground: $(FgText)"
  if ($Desktop) { "foreground is the Claude desktop app: " + ((FgText) -like "'Claude'*") }
  else { "terminal came forward: " + ([J]::GetForegroundWindow() -eq $termWin) }
  Pump 300
  "app log:"
  Get-Content $errFile
}
finally {
  [System.Windows.Forms.Cursor]::Position = $start
  if ($other) { $other.Close() }
  if ($app) { Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue }
  if ($term) { Stop-Process -Id $term.Id -Force -ErrorAction SilentlyContinue }
  $env:APPDATA = $oldAppData; $env:USERPROFILE = $oldProfile
  Remove-Item $sandbox -Recurse -Force -ErrorAction SilentlyContinue
}
