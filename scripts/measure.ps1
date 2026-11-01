# Measures the app's resource use across its whole process tree (the app plus its WebView2
# processes): process count, CPU percent, private memory, working set and GPU use.
# Reports two cases: asleep (no sessions) and animating (a session waiting on you, so the
# robot waves). Based on Measure-Tree in spikes/window/test.ps1.
# The app runs with APPDATA and USERPROFILE pointed at a temp folder, so it never touches the
# real state.json, endpoint.json or Claude Code settings. It is stopped and the folder removed.
# Build first: npx tauri build --no-bundle
param([int]$Seconds = 20)

function Tree($rootId) {
  $all = Get-CimInstance Win32_Process
  $ids = @($rootId); $grew = $true
  while ($grew) {
    $next = $all | Where-Object { $ids -contains $_.ParentProcessId -and $ids -notcontains $_.ProcessId } | ForEach-Object ProcessId
    $grew = [bool]$next; if ($next) { $ids += $next }
  }
  $ids | ForEach-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue }
}

# Sum of GPU engine utilisation for the given pids over a few samples, or $null if the counter is missing.
function Gpu($pids, $samples) {
  try {
    $data = Get-Counter '\GPU Engine(*)\Utilization Percentage' -SampleInterval 1 -MaxSamples $samples -ErrorAction Stop
  } catch { return $null }
  $max = 0.0; $total = 0.0
  foreach ($s in $data) {
    $sum = 0.0
    foreach ($c in $s.CounterSamples) {
      if ($c.InstanceName -match 'pid_(\d+)_' -and $pids -contains [int]$Matches[1]) { $sum += $c.CookedValue }
    }
    $total += $sum; if ($sum -gt $max) { $max = $sum }
  }
  [pscustomobject]@{ Average = $total / $samples; Peak = $max }
}

function Measure-Tree($rootId, $seconds) {
  $before = (Tree $rootId | Measure-Object CPU -Sum).Sum
  $gpu = Gpu @((Tree $rootId).Id) $seconds     # takes about $seconds seconds
  $tree = Tree $rootId
  $cpu = (($tree | Measure-Object CPU -Sum).Sum - $before) / $seconds / [Environment]::ProcessorCount * 100
  $private = ($tree | Measure-Object PrivateMemorySize64 -Sum).Sum / 1MB
  $working = ($tree | Measure-Object WorkingSet64 -Sum).Sum / 1MB
  $g = if ($null -eq $gpu) { "gpu=unavailable" } else { "gpu avg={0:N2}% peak={1:N2}%" -f $gpu.Average, $gpu.Peak }
  "processes={0} cpu={1:N2}% private={2:N0}MB workingSet={3:N0}MB {4}" -f $tree.Count, $cpu, $private, $working, $g
}

$exe = (Resolve-Path (Join-Path $PSScriptRoot "..\target\release\desktop-assistant.exe")).Path
$oldAppData = $env:APPDATA; $oldProfile = $env:USERPROFILE
$sandbox = Join-Path ([System.IO.Path]::GetTempPath()) ("da-measure-" + $PID)
$data = Join-Path $sandbox "appdata\DesktopAssistant"
New-Item -ItemType Directory -Force $data, (Join-Path $sandbox "profile") | Out-Null
$env:APPDATA = Join-Path $sandbox "appdata"; $env:USERPROFILE = Join-Path $sandbox "profile"
$app = $null
# If Windows animation effects are off (as in some remote sessions), the page treats that as
# "reduce motion" and nothing animates. This test-only browser flag makes the page animate anyway,
# so the animating numbers are real. It changes no system setting.
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--blink-settings=prefersReducedMotion=0"
try {
  $app = Start-Process $exe -PassThru
  Start-Sleep -Seconds 8
  "asleep:    " + (Measure-Tree $app.Id $Seconds)

  # One session waiting on the user makes the robot wave until handled.
  $ep = Get-Content (Join-Path $data "endpoint.json") | ConvertFrom-Json
  $body = @{ name = "PermissionRequest"; session_id = "measure"; cwd = "C:\measure\project"; tool = "Bash"; source = "cli" } | ConvertTo-Json
  Invoke-WebRequest -Uri "http://127.0.0.1:$($ep.port)/event" -Method Post -Body $body `
    -Headers @{ Authorization = "Bearer $($ep.token)" } -UseBasicParsing | Out-Null
  Start-Sleep -Seconds 10   # lets the alert sound and speech finish
  "animating: " + (Measure-Tree $app.Id $Seconds)
}
finally {
  if ($app) { Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue }
  Start-Sleep -Milliseconds 500
  Remove-Item Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS -ErrorAction SilentlyContinue
  $env:APPDATA = $oldAppData; $env:USERPROFILE = $oldProfile
  Remove-Item $sandbox -Recurse -Force -ErrorAction SilentlyContinue
}
