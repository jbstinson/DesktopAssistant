# Installer and Release Builds Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** GitHub builds a per-user NSIS `Setup.exe` for DesktopAssistant on a version tag. Uninstalling it removes our Claude Code hook entries before removing files.

**Architecture:** A new `--disconnect` command-line mode in the app reuses the existing hook-removal code. Installer settings live in a separate `src-tauri/tauri.bundle.conf.json`, merged in with `tauri build --config`, so normal builds and `cargo test` are unchanged. An NSIS pre-uninstall hook runs `--disconnect`. `.github/workflows/release.yml` builds on a `v*` tag and makes a draft Release.

**Tech Stack:** Tauri 2.12 (NSIS bundler), Rust, NSIS hook macros, GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-11-22-installer-design.md`

## Global Constraints

- NSIS target only, `installMode: "currentUser"`, no admin prompt.
- `da-hook.exe` is installed in the same folder as `desktop-assistant.exe`, under exactly that name.
- Version is `0.1.0` in `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, `hook/Cargo.toml` and `package.json`.
- Uninstall removes only our hook entries (existing `installer::apply(path, Change::Remove)`), always lets the uninstall continue, and never creates a settings file.
- Releases are drafts. No code signing.
- Never touch the real `%USERPROFILE%\.claude\settings.json` when testing. Use a sandboxed USERPROFILE.
- Every new `src-tauri/src/*.rs` file must be added to the source-file list in `src-tauri/src/permission.rs` (an existing test enforces it).

## Review Focus

- Uninstall while the app is running: the uninstall must still finish and the hooks still be removed.
- Uninstall with no Claude settings file: no file is created and the uninstall carries on (test in Task 1).
- Settings that hold other hooks alongside ours: only ours go (covered by existing `installer` tests; Task 1 adds one through `disconnect`).
- A tag that doesn't match the app version: the release job fails before building (Task 3).
- Installing a newer setup over an older one: same folder, the app is closed first, the hook path stays valid (manual check, Task 2).

---

### Task 1: `--disconnect` mode

**Files:**
- Create: `src-tauri/src/cli.rs`
- Modify: `src-tauri/src/main.rs` (start of `fn main`, `mod cli;`), `src-tauri/src/permission.rs` (source list)

**Interfaces:**
- Produces: `cli::wants_disconnect(args: impl IntoIterator<Item = String>) -> bool` (true when any argument equals `--disconnect`), and `cli::disconnect(settings: Option<&Path>) -> Result<(), String>` (`None` means no USERPROFILE: do nothing and return Ok; otherwise `tray::set_hooks(settings, <unused>, false)` or `installer::apply(settings, Change::Remove)` directly).
- Task 2's NSIS hook runs `"$INSTDIR\desktop-assistant.exe" --disconnect`.

- [ ] **Step 1: Write failing tests in `cli.rs`**
  - `wants_disconnect_only_for_the_flag`: `["app.exe", "--disconnect"]` gives true; `["app.exe"]`, `["app.exe", "--disconnected"]` and `["app.exe", "disconnect"]` give false.
  - `disconnect_removes_only_our_entries`: a temp settings file holding our hook for `PreToolUse` (command `"C:/x/da-hook.exe"`) and another hook (`"C:/other/tool.exe"`). After `disconnect(Some(path))`, the file has no `da-hook.exe` and still has `tool.exe`.
  - `disconnect_without_a_settings_file_creates_nothing`: the path doesn't exist; it returns Ok and the file still doesn't exist.
  - `disconnect_without_userprofile_is_ok`: `disconnect(None)` returns Ok.
- [ ] **Step 2:** `cargo test -p desktop-assistant cli::` fails (module missing).
- [ ] **Step 3: Implement both functions.** In `main()`, before `single::acquire`: if `cli::wants_disconnect(std::env::args())`, run `cli::disconnect(tray::claude_settings_path().as_deref())`, ignore the result except for an `eprintln!`, and `return`. Add `mod cli;` and the file to permission.rs's list.
- [ ] **Step 4:** `cargo test --workspace --locked` passes.
- [ ] **Step 5:** Commit `feat: desktop-assistant --disconnect removes our hooks and exits`.

### Task 2: Installer bundle

**Files:**
- Create: `src-tauri/tauri.bundle.conf.json`, `src-tauri/windows/hooks.nsh`, `scripts/build-installer.ps1`
- Modify: versions in `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, `hook/Cargo.toml`, `package.json` (and `Cargo.lock` via a build)

**Interfaces:**
- Consumes: `--disconnect` from Task 1.
- Produces: `scripts/build-installer.ps1` (no parameters). It builds `da-hook` in release, then runs `npx tauri build --config src-tauri/tauri.bundle.conf.json`, and prints the path of the produced `*-setup.exe` (under `target/release/bundle/nsis/`). Task 3 calls it.

- [ ] **Step 1: Bundle config.** `tauri.bundle.conf.json`: `bundle.active: true`, `targets: ["nsis"]`, `windows.nsis.installMode: "currentUser"`, `windows.nsis.installerHooks: "./windows/hooks.nsh"`, and `resources: { "../target/release/da-hook.exe": "da-hook.exe" }` (on Windows NSIS, resources land in `$INSTDIR`). Also set `publisher`, `shortDescription` and `copyright` to "DesktopAssistant contributors".
- [ ] **Step 2: `hooks.nsh`.** `NSIS_HOOK_PREUNINSTALL` runs `ExecWait '"$INSTDIR\desktop-assistant.exe" --disconnect'` and ignores the exit code. Leave the other macros empty. Check in Tauri's NSIS template for 2.12 whether the "app is running" check comes before this macro. If it doesn't, close the app first. `--disconnect` itself works while the app runs.
- [ ] **Step 3: Version to `0.1.0`** in the four files. Run `cargo build --workspace` so `Cargo.lock` updates.
- [ ] **Step 4: Build locally.** Run `scripts/build-installer.ps1`. It should print a path ending in `DesktopAssistant_0.1.0_x64-setup.exe`.
- [ ] **Step 5: Install and uninstall check, sandboxed.** Use the real user's install location only through `/D=<scratch dir>`, and set `USERPROFILE` to a scratch folder holding a `.claude\settings.json` with our hook pointing at the installed `da-hook.exe`, plus one foreign hook.
  - Run `Setup.exe /S /D=<scratch>\app`. Both `.exe` files are present in `<scratch>\app`.
  - Run `<scratch>\app\uninstall.exe /S` with the same USERPROFILE. The `da-hook.exe` entries are gone, the foreign hook remains, a `.desktop-assistant.bak` exists, and the app folder is removed.
  - Repeat the uninstall with the app running (started via `explorer.exe`). It still finishes.
  - Run the setup twice: it upgrades in place.
- [ ] **Step 6:** `cargo test --workspace --locked` and `node --test scripts/*.test.mjs` still pass without the bundle config.
- [ ] **Step 7:** Commit `feat: per-user NSIS installer that disconnects the hooks on uninstall`.

### Task 3: Release workflow

**Files:**
- Create: `.github/workflows/release.yml`

**Interfaces:**
- Consumes: `scripts/build-installer.ps1` (Task 2).

- [ ] **Step 1: Workflow.**
  - Trigger: `on: push: tags: ['v*']`. Permissions: `contents: write`. `runs-on: windows-latest`, timeout 45 minutes.
  - Steps: checkout, setup-node 22, `npm ci`, rust-toolchain stable, rust-cache, UI tests, `cargo test --workspace --locked`.
  - A version check: the tag minus its `v` must equal `version` in `src-tauri/tauri.conf.json`, or the job fails with a message naming both.
  - Then `scripts/build-installer.ps1`, then `gh release create <tag> <setup.exe> --draft --title "DesktopAssistant <version>" --notes-file <generated notes>`, using `GH_TOKEN: ${{ github.token }}`.
  - The notes say the installer is unsigned, how to get past SmartScreen, and to tick Connect after installing.
- [ ] **Step 2:** Validate with `actionlint` if it's available; otherwise review the YAML by hand. Commit `ci: build the installer and draft a GitHub Release on a version tag`.

### Task 4: README

**Files:**
- Modify: `README.md`

- [ ] **Step 1:** Add an "Install" section near the top:
  - download `DesktopAssistant_<version>_x64-setup.exe` from Releases
  - SmartScreen: **More info → Run anyway**, because it's unsigned
  - per-user, no admin
  - tick **Connect to Claude Code** in the tray
  - uninstall from Windows Settings → Apps: it disconnects the hooks first, and leaves `%APPDATA%\DesktopAssistant` behind for you to delete

  Keep "Build" for developers and add "Making a release": bump the version in the four files, tag `vX.Y.Z`, push the tag, then publish the draft.
- [ ] **Step 2:** Commit `docs: install, uninstall and release steps`.
