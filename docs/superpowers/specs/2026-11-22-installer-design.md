# Installer and release builds

## Intent

Anyone who finds the GitHub repo should be able to download one `Setup.exe`, install
DesktopAssistant, and later uninstall it cleanly. They shouldn't need Rust, Node or a build.
Uninstalling must never leave Claude Code pointing at a deleted hook program.

## Goals

- A per-user Windows installer built by GitHub, attached to a GitHub Release.
- `da-hook.exe` is installed in the same folder as `desktop-assistant.exe`, because the app looks for it there and Connect writes that path into Claude settings.
- Uninstalling removes our hook entries from the user's Claude `settings.json` first, the same as unticking **Connect to Claude Code**. Then it removes the files.

## Non-goals

- Paid code signing. Windows SmartScreen will show "Windows protected your PC", and the README tells people to click **More info → Run anyway**.
- Auto-update.
- MSI, a portable zip, or other platforms.
- Connecting automatically on install. Connect stays the user's own choice in the tray, as today.

## Anti-goals

- An uninstall that leaves hook entries behind, or that touches any hook entries other than ours.
- An installer that needs admin rights.

## Approaches considered

- **A. Tauri's NSIS installer (chosen by the user).** Per-user, no admin. Tauri supports bundling an extra program and adding custom install and uninstall steps, which is what the disconnect needs.
- **B. MSI (WiX).** Custom uninstall actions are clumsier, and per-user MSIs are awkward. No benefit for GitHub downloaders.
- **C. Portable zip.** Has no uninstaller, so it can't disconnect.

## Design

1. **Bundling.** Turn on bundling with the NSIS target only, installing per user (`installMode: currentUser`, so under `%LOCALAPPDATA%\Programs\DesktopAssistant`). Ship `da-hook.exe` with the app as a bundled extra program, so it ends up beside `desktop-assistant.exe` under that exact name.
2. **Version.** Start at `0.1.0` in `tauri.conf.json` and both `Cargo.toml` files. The release workflow refuses to build if the tag doesn't match.
3. **`desktop-assistant.exe --disconnect`.** A command-line mode that removes only our hook entries, using the same code and backup as unticking Connect, then exits. It shows no window and doesn't take the single-instance lock, so it works while the app is running. It does nothing if our entries aren't there or the settings file is missing, and it always exits 0, so the uninstall carries on whatever happens.
4. **Uninstall step.** A pre-uninstall step closes a running app, then runs `--disconnect`, then lets the normal uninstall remove the files. User data in `%APPDATA%\DesktopAssistant` (rules, avatar and other choices) is left behind, as uninstallers usually do. The README says where it is.
5. **Upgrades.** Installing a newer `Setup.exe` over an older one keeps the same folder, so the hook path written by Connect stays valid. The installer closes a running app before replacing files.
6. **Release workflow.** `.github/workflows/release.yml` runs on a pushed tag `v*`, on `windows-latest`. It runs the tests, builds the hook and the installer, and creates a **draft** GitHub Release with the `Setup.exe` attached. The owner reviews the draft and publishes it.
7. **README.** An "Install" section covering the download from Releases, the SmartScreen click-through, ticking Connect, and what uninstalling does. The current build-from-source steps stay for developers.

## Left to the builder

- The exact Tauri config keys and NSIS hook macro names for the Tauri 2 version in `Cargo.lock`.
- Unit tests for `--disconnect`: our entries are removed and others kept; a missing file is handled; it's a no-op when not connected. Reuse the existing `installer` tests' fixtures.
- Verifying a real install and uninstall locally against a sandboxed USERPROFILE, never the real settings.
