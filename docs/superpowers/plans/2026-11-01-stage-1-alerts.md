# DesktopAssistant Stage 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** An always-on-top character that shows what Claude Code sessions are doing and alerts the user when one needs them.

**Architecture:** A small `da-hook` binary is the Claude Code hook. It forwards each event over localhost to the Tauri app. In the app, Rust core modules turn events into session state and alerts through a rules file, then the plain HTML/CSS/JS UI renders one `snapshot` event. Pure logic stays free of Tauri so it can be unit tested.

**Tech Stack:** Tauri 2 (`tray-icon`), Rust, `tauri-plugin-global-shortcut`, `tiny_http`, `ureq`, `serde`, `serde_json`, `toml`, `notify`, `tts`, `windows-sys`, plain HTML/CSS/JS with no bundler or framework.

**Spec:** `docs/superpowers/specs/2026-11-01-stage-1-alerts.md`

## Global Constraints

- Windows 11 only, x86_64-pc-windows-msvc.
- All user data lives in `%APPDATA%\DesktopAssistant\`. No hardcoded paths.
- `da-hook` always exits 0, within 2 seconds at worst, with empty stdout. It must never affect a session.
- The listener binds to `127.0.0.1` only and rejects requests without the current token.
- The window keeps `WS_EX_NOACTIVATE` at all times. Never call Tauri's `set_ignore_cursor_events`, which removes it. Toggle `WS_EX_TRANSPARENT | WS_EX_LAYERED` directly (proved in `spikes/window`).
- Rules can never approve or deny anything. Nothing in Stage 1 writes to a session.
- No network use beyond localhost. No Tiny Engineer names, art or sounds.
- Idle target: near-zero CPU, no GPU work, about 200 MB private memory or less across the process tree.
- No UI framework or bundler.

## Review Focus

- A session closed or killed while waiting never sends another event: it must go stale after 30 minutes of silence, and the panel can dismiss it sooner.
- One permission prompt fires both `PermissionRequest` and `Notification`: the user must get one alert, not two.
- The app is closed while Claude Code runs: `da-hook` exits 0 fast. A leftover `endpoint.json` with a dead port is treated the same way.
- `~/.claude/settings.json` holds hooks, invalid JSON or nothing: existing hooks must survive, invalid JSON must abort without writing, and a backup is written first.
- The saved window position is on a monitor that is gone: the character must reappear on the primary monitor.

---

### Task 1: App scaffold and overlay window

**Files:**
- Create: `package.json`, `.gitignore`, `Cargo.toml` (workspace: `src-tauri`, `hook`), `src-tauri/Cargo.toml`, `src-tauri/build.rs`, `src-tauri/tauri.conf.json`, `src-tauri/icons/icon.ico`, `src-tauri/src/main.rs`, `src-tauri/src/overlay.rs`, `ui/index.html`
- Test: inline `#[cfg(test)]` in `overlay.rs`

**Interfaces:**
- Produces: `overlay::HitRegion` enum `{ Circle { cx: f64, cy: f64, r: f64 }, Full }`; `overlay::is_inside(region: &HitRegion, x: f64, y: f64) -> bool`; `overlay::start(window: WebviewWindow, region: Arc<Mutex<HitRegion>>)`; `overlay::pause(paused: bool)`.

- [ ] **Step 1:** Run `git init` and add a `.gitignore` covering `node_modules`, `target`, `spikes/**/target`, `spikes/**/node_modules`, `spikes/hooks/events.jsonl`.
- [ ] **Step 2: Write failing tests** `is_inside`: circle center true, edge true, just outside false, corner false, `Full` always true. Coordinates are window-relative.
- [ ] **Step 3:** Run `cargo test -p desktop-assistant overlay`. Expected: FAIL, not defined.
- [ ] **Step 4:** Implement `overlay.rs`, porting `spikes/window/src-tauri/src/main.rs`. Read the window rect with `GetWindowRect` on each poll, because the window moves. Poll every 100 ms; while paused, the thread parks.
- [ ] **Step 5:** Run `cargo test`. Expected: PASS.
- [ ] **Step 6:** Point `spikes/window/test.ps1` at the new exe and run it. Expected: every check prints True.
- [ ] **Step 7:** Commit `feat: overlay window that never takes focus`.

### Task 2: Events and session state

**Files:**
- Create: `src-tauri/src/events.rs`, `src-tauri/src/sessions.rs`
- Test: inline

**Interfaces:**
- Produces: `events::HookEvent { name: String, session_id: String, cwd: String, tool: Option<String>, notification_type: Option<String>, source: String, terminal_pid: Option<u32> }`, deserialized from JSON. `sessions::Activity` enum `{ Thinking, Reading, Typing, NeedsYou, Done, Error, Idle }`. `sessions::Sessions` with `apply(&mut self, e: &HookEvent, now: u64) -> Applied`, `dismiss(&mut self, id: &str)`, `expire(&mut self, now: u64)`, `character_state(&self) -> &'static str`, `list(&self) -> Vec<SessionView>`. `Applied { turn_seconds: Option<u64>, newly_blocked: bool, session_known: bool }`. `SessionView { id: String, project: String, source: String, activity: Activity, since: u64, terminal_pid: Option<u32> }`.

- [ ] **Step 1: Write failing tests:**
  - `Read` gives Reading, `Bash` gives Typing, unknown tool gives Thinking, `UserPromptSubmit` gives Thinking and starts the turn clock.
  - `Stop` 75 seconds after `UserPromptSubmit` returns `turn_seconds == Some(75)` and activity Done.
  - `PermissionRequest` then `Notification` on one session: `newly_blocked` true then false.
  - A later `PreToolUse` clears NeedsYou.
  - `StopFailure` gives Error.
  - `SessionEnd` removes the session, `dismiss` removes it, `expire` removes one silent for 1800 seconds and keeps one silent for 1799.
  - An event for an unseen session creates it, because the app may start after the session does.
  - `character_state` priority: `needs-you` over `error` over `typing`/`reading`/`thinking` over `done`; no sessions gives `asleep`.
- [ ] **Step 2:** Run `cargo test sessions`. Expected: FAIL.
- [ ] **Step 3:** Implement. Project name is the last path segment of `cwd`.
- [ ] **Step 4:** Run `cargo test sessions`. Expected: PASS.
- [ ] **Step 5:** Commit `feat: track session state from hook events`.

### Task 3: Hook binary and local listener

**Files:**
- Create: `hook/Cargo.toml`, `hook/src/main.rs`, `src-tauri/src/listener.rs`, `src-tauri/src/paths.rs`
- Test: inline, and `hook/tests/bridge.rs`

**Interfaces:**
- Consumes: `events::HookEvent`.
- Produces: `paths::data_dir() -> PathBuf`. `listener::start(on_event: impl Fn(HookEvent) + Send + 'static) -> Endpoint`, which picks a free port, generates a random token, writes `endpoint.json` as `{ "port": u16, "token": String }`, and accepts `POST /event` with `Authorization: Bearer <token>`. `da-hook` takes no arguments and reads the event from stdin.

- [ ] **Step 1: Write failing tests:**
  - Listener: valid token returns 204 and calls `on_event` once; wrong or missing token returns 401 with no call; malformed JSON returns 400; `GET` returns 405.
  - Bridge: run `da-hook` with no `endpoint.json`, a dead port and empty stdin. Each exits 0 with empty stdout in under 2 seconds. Set `APPDATA` to a temp folder.
- [ ] **Step 2:** Run `cargo test`. Expected: FAIL.
- [ ] **Step 3:** Implement. `da-hook` uses a 1-second `ureq` timeout, adds `source` from `CLAUDE_CODE_ENTRYPOINT`, and adds `terminal_pid` for `cli` sessions: walk parent processes with `CreateToolhelp32Snapshot` to the first ancestor owning a visible top-level window. Forward only fields `HookEvent` needs. Never read or forward `tool_input`.
- [ ] **Step 4:** Run `cargo test`. Expected: PASS.
- [ ] **Step 5:** Commit `feat: forward hook events to the app over localhost`.

### Task 4: Hook installer

**Files:**
- Create: `src-tauri/src/installer.rs`
- Test: inline

**Interfaces:**
- Produces: `installer::add(settings: &str, hook_path: &str) -> Result<String, String>`, `installer::remove(settings: &str, hook_path: &str) -> Result<String, String>`, `installer::is_installed(settings: &str, hook_path: &str) -> bool`, `installer::apply(path: &Path, change: Change) -> Result<(), String>`, where `Change` is `Add(String)` or `Remove(String)`.

Hook events: `SessionStart`, `SessionEnd`, `UserPromptSubmit`, `PreToolUse`, `PostToolUseFailure`, `PermissionRequest`, `Notification`, `Stop`, `StopFailure`.

- [ ] **Step 1: Write failing tests:**
  - `add` on empty input creates all nine events.
  - Existing `SessionStart` and `PostToolUse` entries and non-hook keys survive `add`.
  - `add` twice equals `add` once.
  - `remove` restores the previous content semantically and removes only our entry.
  - Invalid JSON returns `Err`.
  - `apply` writes `settings.json.desktop-assistant.bak` before changing anything, writes through a temp file and rename, and leaves the file untouched on `Err`.
- [ ] **Step 2:** Run `cargo test installer`. Expected: FAIL.
- [ ] **Step 3:** Implement with `serde_json`, preserving key order (`preserve_order` feature).
- [ ] **Step 4:** Run `cargo test installer`. Expected: PASS.
- [ ] **Step 5:** Commit `feat: add and remove hooks safely`.

### Task 5: Rules engine

**Files:**
- Create: `src-tauri/src/rules.rs`, `src-tauri/src/default_rules.toml`
- Test: inline

**Interfaces:**
- Consumes: `events::HookEvent`, `sessions::Applied`.
- Produces: `rules::Tier` enum `{ NeedsYou, GoodToKnow, Silent }`. `rules::Alert { tier: Tier, sound: Option<String>, speak: Option<String> }`. `rules::RuleSet::parse(text: &str) -> Result<RuleSet, String>`, `RuleSet::evaluate(&self, e: &HookEvent, applied: &Applied, project: &str) -> Alert`. `rules::load_or_create(path: &Path) -> (RuleSet, Option<String>)`, where the string is a warning.

Default file, first match wins, no match is silent:

```toml
[[rule]]
when = { event = "StopFailure" }
then = { tier = "needs_you", sound = "alert", speak = "{project} failed" }

[[rule]]
when = { event = ["PermissionRequest", "Notification"] }
then = { tier = "needs_you", sound = "alert", speak = "{project} needs you" }

[[rule]]
when = { event = "Stop", min_turn_seconds = 60 }
then = { tier = "good_to_know", sound = "bell", speak = "{project} finished" }
```

- [ ] **Step 1: Write failing tests:**
  - Defaults: `Stop` at 60 is good_to_know, at 59 is silent, `PostToolUseFailure` is silent.
  - `Notification` is needs_you, with `{project}` substituted in `speak`.
  - `newly_blocked == false` on a needs_you rule returns an Alert with no sound and no speech, so one prompt alerts once.
  - `project`, `tool` and `source` conditions match; `event` takes a string or list.
  - First match wins.
  - Unknown tier, unknown condition key and bad TOML return `Err` with a message naming the problem.
  - `load_or_create` writes the defaults when the file is missing.
- [ ] **Step 2:** Run `cargo test rules`. Expected: FAIL.
- [ ] **Step 3:** Implement with `#[serde(deny_unknown_fields)]`, so a typo is an error, not a rule that silently never fires.
- [ ] **Step 4:** Run `cargo test rules`. Expected: PASS.
- [ ] **Step 5:** Commit `feat: alert rules with safe defaults`.

### Task 6: Rules hot reload

**Files:**
- Create: `src-tauri/src/rules_watch.rs`
- Test: `src-tauri/tests/rules_watch.rs`

**Interfaces:**
- Consumes: `rules::RuleSet`.
- Produces: `rules_watch::watch(path: PathBuf, on_change: impl Fn(Result<RuleSet, String>) + Send + 'static) -> Watcher`. The caller keeps the last good set on `Err` and shows the message.

- [ ] **Step 1: Write failing tests,** using a temp file: saving valid rules gives `Ok` within 2 seconds; saving invalid rules gives `Err`; a rename-over save, which is how many editors save, is detected; a burst of three writes gives one callback.
- [ ] **Step 2:** Run `cargo test --test rules_watch`. Expected: FAIL.
- [ ] **Step 3:** Implement with `notify`, watching the parent folder, filtered to the file, debounced 200 ms.
- [ ] **Step 4:** Run the same command. Expected: PASS.
- [ ] **Step 5:** Commit `feat: reload rules on save`.

### Task 7: Sound and speech

**Files:**
- Create: `src-tauri/src/delivery.rs`, `scripts/gen-sounds.mjs`, `src-tauri/sounds/alert.wav`, `src-tauri/sounds/bell.wav`
- Test: inline

**Interfaces:**
- Consumes: `rules::Alert`.
- Produces: `delivery::Output` trait `{ fn play(&self, sound: &str); fn say(&self, text: &str); }`. `delivery::Queue::new(output: impl Output + Send + 'static) -> Queue`, `Queue::push(&self, alert: Alert)`, `Queue::set_quiet(&self, quiet: bool)`. `delivery::WindowsOutput`.

- [ ] **Step 1: Write failing tests** with a fake `Output` recording calls: sound plays before speech; two alerts play in order without overlap; quiet drops both, and dropped alerts do not play when quiet ends; a silent Alert does nothing; five queued alerts for the same text collapse to one.
- [ ] **Step 2:** Run `cargo test delivery`. Expected: FAIL.
- [ ] **Step 3:** Implement. One worker thread. `WindowsOutput` uses `PlaySoundW` with `include_bytes!` WAVs and the `tts` crate. Generate both WAVs from sine tones in `gen-sounds.mjs`, so they are original.
- [ ] **Step 4:** Run `cargo test delivery`. Expected: PASS.
- [ ] **Step 5:** Commit `feat: queued sound and spoken alerts`.

### Task 8: Wire the core and draw the character

**Files:**
- Create: `src-tauri/src/app.rs`, `ui/character.svg.js`, `ui/character.css`, `ui/main.js`
- Modify: `src-tauri/src/main.rs`, `ui/index.html`
- Test: inline in `app.rs`

**Interfaces:**
- Consumes: everything from Tasks 2 to 7.
- Produces: `app::Core::handle(&mut self, e: HookEvent, now: u64) -> Option<Alert>`, `app::Snapshot { state: String, waiting: usize, sessions: Vec<SessionView>, warning: Option<String>, quiet: bool, hooks_on: bool }`. Rust emits a `snapshot` event on every change and once per 30 seconds for expiry. UI sets `document.body.dataset.state` from it, and each state's animation lives only in CSS under `[data-state="…"]`.

- [ ] **Step 1: Write failing tests** for `Core`: event updates the snapshot; needs_you returns an Alert; rules replacement applies to the next event; a rejected rule file leaves old rules active and sets `warning`.
- [ ] **Step 2:** Run `cargo test app`. Expected: FAIL.
- [ ] **Step 3:** Implement `Core` and connect listener, watcher, delivery and overlay in `main.rs`.
- [ ] **Step 4:** Draw the robot as inline SVG with named groups (`head`, `eyes`, `arm-left`, `arm-right`, `bell`). Animate only `transform` and `opacity`. `asleep` and `done` after settling have no running animation. The waiting count is a badge on the character.
- [ ] **Step 5:** Run `cargo test`. Expected: PASS.
- [ ] **Step 6:** Send events with `curl` using `endpoint.json`, and confirm each state renders by screenshot.
- [ ] **Step 7:** Commit `feat: character reacts to session events`.

### Task 9: Session panel

**Files:**
- Create: `ui/panel.js`, `ui/panel.css`
- Modify: `ui/index.html`, `ui/main.js`, `src-tauri/src/main.rs`
- Test: manual, plus the `spikes/window/test.ps1` focus checks

**Interfaces:**
- Consumes: `Snapshot`, `overlay::HitRegion`, `Sessions::dismiss`.
- Produces: commands `set_panel(open: bool)`, `dismiss_session(id: String)`, `jump_to(id: String)` (body in Task 11).

- [ ] **Step 1:** Implement `set_panel`: resize the single window to 360×480, keeping the character's screen position and expanding toward whichever side has room, and set `HitRegion::Full`. Closing restores 200×200 and the circle.
- [ ] **Step 2:** Implement the list with project, source, state and elapsed time, plus a dismiss button on each row and the rules warning at the top. Escape all text with `textContent`, since project names come from folder names.
- [ ] **Step 3:** Rerun `spikes/window/test.ps1` with the panel open. Expected: focus checks print True.
- [ ] **Step 4:** Commit `feat: session panel`.

### Task 10: Tray, hotkey, position and do-not-disturb

**Files:**
- Create: `src-tauri/src/tray.rs`, `src-tauri/src/position.rs`
- Modify: `src-tauri/src/main.rs`
- Test: inline in `position.rs`

**Interfaces:**
- Consumes: `installer::apply`, `installer::is_installed`, `Queue::set_quiet`, `overlay::pause`.
- Produces: `position::Rect { x: i32, y: i32, w: i32, h: i32 }`, `position::restore(saved: Option<(i32, i32)>, monitors: &[Rect], size: (i32, i32)) -> (i32, i32)`.

- [ ] **Step 1: Write failing tests** for `restore`: valid position kept, including negative y on the portrait monitor (`3840,-139`); position on a missing monitor moves to the primary, which is `monitors[0]`; `None` gives the primary's bottom-right corner; a partly off-screen position is pulled fully on.
- [ ] **Step 2:** Run `cargo test position`. Expected: FAIL.
- [ ] **Step 3:** Implement. Save position to `state.json` after a drag ends. Use `startDragging()` from a mousedown held over 150 ms, so a click still opens the panel.
- [ ] **Step 4:** Implement the tray menu: Show/Hide, Do not disturb, Connect to Claude Code (toggle, showing what will change, using `installer::apply`), Open rules file, Quit. Hotkey `Ctrl+Alt+D` toggles visibility. If registration fails, show a warning instead of crashing. Hiding pauses the overlay and animation.
- [ ] **Step 5:** Run `cargo test`. Expected: PASS.
- [ ] **Step 6:** Commit `feat: tray, hotkey and remembered position`.

### Task 11: Click-to-jump

**Files:**
- Create: `src-tauri/src/jump.rs`
- Modify: `src-tauri/src/main.rs`
- Test: inline for the choice logic, manual for Windows behavior

**Interfaces:**
- Consumes: `SessionView.source`, `SessionView.terminal_pid`.
- Produces: `jump::Target` enum `{ Terminal(u32), DesktopApp, None }`, `jump::target_for(source: &str, terminal_pid: Option<u32>) -> Target`, `jump::focus(target: Target) -> Result<(), String>`.

- [ ] **Step 1: Write failing tests:** `cli` with a pid gives Terminal; `claude-desktop` gives DesktopApp; `sdk-cli` gives None; `cli` with no pid gives None.
- [ ] **Step 2:** Run `cargo test jump`. Expected: FAIL.
- [ ] **Step 3:** Implement with `EnumWindows`, `ShowWindow(SW_RESTORE)` when minimized, and `SetForegroundWindow`. A no-activate window may be refused foreground rights: if so, call `AllowSetForegroundWindow` first and fall back to flashing the target with `FlashWindowEx`. Record which one was needed.
- [ ] **Step 4:** Check whether the desktop app has a documented link to one session. If not, leave DesktopApp as bringing the app forward; the panel already names the session.
- [ ] **Step 5:** Run `cargo test`, then try a real terminal and the desktop app. Expected: PASS, and each comes forward or flashes.
- [ ] **Step 6:** Commit `feat: jump to a session's window`.

### Task 12: End-to-end verification

**Files:**
- Create: `README.md`, `scripts/measure.ps1`
- Modify: `.gitignore`
- Test: this task is the test

- [ ] **Step 1:** Build with `npx tauri build --no-bundle` and copy `da-hook.exe` next to the app.
- [ ] **Step 2:** Connect from the tray. Confirm a diff of `~/.claude/settings.json` adds only our entries and the user's `SessionStart` and `PostToolUse` hooks remain.
- [ ] **Step 3:** Run real sessions in the desktop app and a terminal. Confirm a permission prompt alerts once, a turn of 60 seconds or more rings the bell, and a quick turn stays silent.
- [ ] **Step 4:** Quit the app and run a session. Expected: no delay or error in Claude Code.
- [ ] **Step 5:** Save a broken rule, confirm the warning, and confirm alerts still work.
- [ ] **Step 6:** Check the portrait monitor, display scaling at 125%, and a full-screen app. Record any failures in `README.md`.
- [ ] **Step 7:** Run `scripts/measure.ps1`, based on `Measure-Tree` in `spikes/window/test.ps1`, asleep and animating, and check GPU use in Task Manager. Expected: meets the idle target.
- [ ] **Step 8:** Remove the hooks from the tray, confirm `settings.json` matches the original, then reconnect.
- [ ] **Step 9:** Write `README.md`: what it does, setup, rules file reference, known limits.
- [ ] **Step 10:** Commit `docs: Stage 1 usage and verification results`.
