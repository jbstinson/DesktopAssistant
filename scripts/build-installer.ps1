# Builds the per-user Windows installer: da-hook.exe first (the installer ships it beside the
# app), then the app and its NSIS setup with the installer settings from tauri.bundle.conf.json.
# Prints the path of the setup .exe. Normal builds and tests never use the bundle settings.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    cargo build --release --locked -p da-hook
    if ($LASTEXITCODE -ne 0) { throw "da-hook build failed" }
    npx tauri build --config src-tauri/tauri.bundle.conf.json
    if ($LASTEXITCODE -ne 0) { throw "installer build failed" }
    $setup = Get-ChildItem "$root\target\release\bundle\nsis\*-setup.exe" |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if (-not $setup) { throw "no setup .exe was produced" }
    $setup.FullName
} finally {
    Pop-Location
}
