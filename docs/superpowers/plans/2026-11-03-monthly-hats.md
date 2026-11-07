# Monthly Hats Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The robot wears the current month's hat, each month's hat can be turned off from the tray, and nothing else about the robot changes.

**Architecture:** Rust owns the truth: `hats.rs` loads the on/off choices and reads the local month, `Core` puts the hat to show (`Option<u8>`, month 1–12) into the snapshot, and a thread checks the month once a minute. The UI draws the hat inside `#head` from art in `ui/hats.js`. The closed window already has 30 CSS px of room above the 140 px robot, so the SVG draws outside its viewBox there; only the open panel needs 30 px more.

**Tech Stack:** Existing stack. No new dependencies. One added `windows-sys` feature: `Win32_System_SystemInformation`.

**Spec:** `docs/superpowers/specs/2026-11-03-monthly-hats-design.md`

**Approved art:** `.superpowers/brainstorm/1729-1793704216/content/monthly-hats.html` (git-ignored, on disk). Port each month's hat markup from it as is.

## Global Constraints

- Twelve hats, January (1) to December (12), drawn as in the approved mockup.
- All twelve on by default. A missing or unreadable `hats.json` means all on.
- Saved in `%APPDATA%\DesktopAssistant\hats.json` as `{"off": [<month numbers>]}`; other entries are ignored.
- A month whose hat is off looks exactly as today, antenna included. While a hat shows, the antenna is hidden.
- The hat lives inside `#head` and moves with it. Badge, alarm, thinking dots, "zzz", bell and `#warn` draw above the hat.
- The robot keeps its size and screen position. The closed window stays 200×200 CSS px; `ROBOT_HIT_RADIUS` and `closed_region` don't change.
- Month check at most once a minute. No added CSS animation. Idle CPU and GPU no higher than now.
- `WS_EX_NOACTIVATE` and click-through behaviour unchanged.

## Review Focus

- Panel opened with the robot at the top of the window: the hat must not be cut off by the window edge. (Task 4 test.)
- `hats.json` with junk entries such as `{"off": [0, 13, "x", 10.5, 10]}`: only October is off. (Task 1 test.)
- The current month's hat turned off in the tray: the hat goes and the antenna returns at once, with no restart. (Task 1 Core test, Task 3 visual check.)
- The month changing while the robot is hidden or the panel is open: the right hat shows when it reappears. (Task 1 Core test: the hat in the snapshot doesn't depend on visibility or quiet.)
- The tallest hats (October, January, April) at the head's widest tilt (thinking −6°, asleep 5°) and during startle stay inside the window. (Task 3 visual check.)

---

### Task 1: Hat settings, month and snapshot

**Files:**
- Create: `src-tauri/src/hats.rs`
- Modify: `src-tauri/src/app.rs`, `src-tauri/src/main.rs`, `src-tauri/Cargo.toml`
- Test: inline

**Interfaces:**
- Produces in `hats.rs`: `pub type Months = [bool; 12]` (index 0 = January). `pub const ALL_ON: Months`. `pub const NAMES: [&str; 12]` ("January" … "December"). `pub fn load(path: &Path) -> Months`. `pub fn save(path: &Path, on: &Months) -> std::io::Result<()>` (atomic, like `approvals::save`, writing months that are off in ascending order). `pub fn shown(month: u8, on: &Months) -> Option<u8>`. `pub fn local_month() -> u8` (Windows `GetLocalTime`, `wMonth`).
- Produces in `app.rs`: `Core::set_hats(&mut self, on: Months)`, `Core::hats(&self) -> Months`, `Core::set_month(&mut self, month: u8) -> bool` (true when the hat to show changed). `Snapshot.hat: Option<u8>`, serialized as a number or `null`.
- Produces in `main.rs`: `hats_path()` returning `paths::data_dir().join("hats.json")`; the month thread.

- [ ] **Step 1: Write failing tests** in `hats.rs`:
  - `shown(10, &ALL_ON) == Some(10)`; with October off, `shown(10, ..) == None` and `shown(11, ..) == Some(11)`; `shown(0, ..)` and `shown(13, ..)` are `None`.
  - Missing file loads `ALL_ON`. `""`, `"{ bad"`, `"[]"`, `"null"`, `{"off": "10"}` all load `ALL_ON`.
  - `{"off": [0, 13, "x", 10.5, 10]}` loads with only index 9 false.
  - Save then load round-trips, including into a new nested folder, and leaves no `.json.tmp`. Saving with February and October off writes `{"off":[2,10]}`.
  - `#[ignore = "reads the system clock"]` test: `local_month()` is in `1..=12`.
- [ ] **Step 2: Write failing tests** in `app.rs`: a new Core's snapshot has `hat == None` until `set_month` is called. `set_month(10)` → `true`, snapshot `hat == Some(10)`. Same month again → `false`. `set_hats` with October off → snapshot `hat == None`; turning it back on → `Some(10)`. With the window hidden or quiet on, the snapshot's `hat` is unchanged. `serde_json::to_value(snapshot)["hat"]` is `10`, or `null` when off.
- [ ] **Step 3:** Run `cargo test -p desktop-assistant hats` and `cargo test -p desktop-assistant app`. Expected: FAIL.
- [ ] **Step 4:** Implement. Add `Win32_System_SystemInformation` to the `windows-sys` features. In `main.rs` setup, before the tray is installed: load `hats.json` into Core, call `set_month(hats::local_month())`, then spawn a thread that sleeps 60 seconds, calls `set_month(local_month())`, and runs `publish()` only when it returns true. The core lock is released before publishing, like every other route.
- [ ] **Step 5:** Run `cargo test --workspace`. Expected: PASS.
- [ ] **Step 6:** Commit `feat: hat to show from the month and saved choices`.

### Task 2: Hats tray submenu

**Files:**
- Modify: `src-tauri/src/tray.rs`, `src-tauri/src/main.rs`, `README.md`
- Test: inline

**Interfaces:**
- Consumes: `hats::{Months, NAMES, save}`, `Core::{hats, set_hats}`, `hats_path()`.
- Produces: `pub struct HatMenu { pub path: PathBuf }` passed to `tray::install`. A **Hats** submenu placed right after Voice, items with ids `hat:1` … `hat:12`, labels from `NAMES`, ticked from `Core::hats()`. `pub fn toggled(on: Months, month: u8) -> Months` (pure: flips one month; anything outside 1–12 returns `on` unchanged).

- [ ] **Step 1: Write failing tests:** `toggled(ALL_ON, 10)` has only index 9 false; toggling twice restores; `toggled(on, 0)` and `toggled(on, 13)` return `on` unchanged.
- [ ] **Step 2:** Run `cargo test -p desktop-assistant tray`. Expected: FAIL.
- [ ] **Step 3:** Implement. A click flips that month in Core, sets the item's tick from the new value, saves `hats.json` (logging a save error with `eprintln!` and keeping the change in memory, like `pick_voice`), then publishes. Release the core lock before publishing.
- [ ] **Step 4:** Add a README "Hats" section: what each month wears, the tray submenu, `hats.json` and its format.
- [ ] **Step 5:** Run `cargo test --workspace`. Expected: PASS.
- [ ] **Step 6:** Commit `feat: Hats tray submenu`.

### Task 3: Hat art on the robot

**Files:**
- Create: `ui/hats.js`, `scripts/hats.test.mjs`, `scripts/hats-preview.html`, `scripts/serve-ui.mjs`
- Modify: `ui/character.svg.js`, `ui/character.css`, `ui/main.js`
- Test: `scripts/hats.test.mjs`, visual check

**Interfaces:**
- Consumes: `snap.hat` (number 1–12 or null).
- Produces in `ui/hats.js`: `export const HAT_DEFS` (an SVG `<defs>` fragment holding the shared shapes, ids `hat-star` and `hat-heart`), `export function hatMarkup(month)` (SVG fragment string; `""` for null, 0, 13 or anything else that isn't 1–12).
- Produces in `character.svg.js`: the antenna's `<line>` and `<circle>` wrapped in `<g id="antenna">`; `<g id="hat"></g>` as the last child of `#head`; `HAT_DEFS` inside the SVG; `mountCharacter` also returns `hat` (the `#hat` element).

- [ ] **Step 1: Write failing tests** in `scripts/hats.test.mjs`: `hatMarkup(null)`, `hatMarkup(0)`, `hatMarkup(13)`, `hatMarkup("10")` are `""`. For 1–12, the markup is non-empty, the twelve are distinct, and none contains `id=`. Every `href="#…"` in a hat points at an id defined in `HAT_DEFS`. Every id in `HAT_DEFS` starts with `hat-`.
- [ ] **Step 2:** Run `node --test scripts/hats.test.mjs`. Expected: FAIL.
- [ ] **Step 3:** Implement. Port each hat from the approved mockup, renaming `#star`/`#heart` to `#hat-star`/`#hat-heart`. In `character.css`, add `#character { overflow: visible; }` and `body[data-hat="on"] #antenna { opacity: 0; }`. In `main.js` `apply`, replace `#hat`'s contents only when `snap.hat` differs from the last value, and set `body.dataset.hat` to `"on"` or `"off"`. The art is the app's own constant text, so `innerHTML` is acceptable here.
- [ ] **Step 4:** Run `node --test scripts/*.test.mjs`. Expected: PASS.
- [ ] **Step 5:** Write `scripts/serve-ui.mjs`: a static file server on `127.0.0.1` with a port from the command line, serving the repo root, with no dependencies. Write `scripts/hats-preview.html`: a grid of 200×200 cells, each with a 140×140 robot centred like the closed window, one row per month plus a no-hat row, and one column per state (`asleep`, `idle`, `typing`, `reading`, `thinking`, `done`, `needs-you` with `data-multi="true"`, `error`). Each cell sets `data-state` and `data-hat` on its own wrapper; adjust `character.css` selectors only if they need to match a wrapper as well as `body`. Animations play.
- [ ] **Step 6:** Serve, open the preview in the browser, and screenshot it. Check by eye: every hat sits on the head and tilts with it; no hat crosses its cell's edge; the badge, alarm, thinking dots and "zzz" draw over the hat; the antenna shows only in the no-hat row. Fix the art where a mark is mostly hidden. Note the result in the commit body.
- [ ] **Step 7:** Commit `feat: draw the month's hat on the robot`.

### Task 4: Room for the hat with the panel open

**Files:**
- Modify: `src-tauri/src/overlay.rs`, `src-tauri/src/main.rs`, `ui/main.js`
- Test: inline

**Interfaces:**
- Produces in `overlay.rs`: `pub struct PanelPlace { pub x: i32, pub y: i32, pub below: bool }`. `pub fn panel_place(robot: (i32, i32), monitor: (i32, i32, i32, i32), panel: (i32, i32), robot_size: (i32, i32), room: i32) -> PanelPlace`. It calls `panel_rect` with the robot box grown upward by `room` (`(robot.0, robot.1 - room)`, size `(robot_size.0, robot_size.1 + room)`). `below` is true when the robot's top half is in the window's top half.
- Produces in `main.rs`: `const HAT_ROOM: f64 = 30.0;`, `PANEL_H` raised from 480 to 510, and `PanelLayout.panel_below: bool`.

- [ ] **Step 1: Write failing tests** for `panel_place` with a 1920×1080 monitor at (0, 0), panel 360×510, robot 140×140, room 30:
  - Robot at (10, 100): `x == 10`, `y == 70`, `below`, and the robot sits 30 px below the window top.
  - Robot at (10, 900): the window grows up, `y == 900 + 140 - 510`, `!below`.
  - Robot at (10, 30), with the box top on the monitor edge: `y == 0`, and the robot is 30 px below the window top.
  - Robot at (1770, 100): `x == 1770 + 140 - 360`.
  - The existing `panel_rect` tests are unchanged and still pass.
- [ ] **Step 2:** Run `cargo test -p desktop-assistant overlay`. Expected: FAIL.
- [ ] **Step 3:** Implement. `set_panel` uses `panel_place` with `px(HAT_ROOM)` and reports `panel_below`. In `main.js`, the panel's top is `robot_y + 140 + 8` when `panel_below`, otherwise `8px`. Panel height stays 332.
- [ ] **Step 4:** Run `cargo test --workspace` and `node --test scripts/*.test.mjs`. Expected: PASS.
- [ ] **Step 5:** Commit `feat: room for the hat when the panel opens`.

### Task 5: Verification

**Files:**
- Modify: `README.md`
- Test: this task is the test

- [ ] **Step 1:** Build into the scratch target (`CARGO_TARGET_DIR`), run `scripts/overlay-check.ps1` with `DA_EXE` set to that build and a sandboxed `APPDATA` and `USERPROFILE`. Expected: all True.
- [ ] **Step 2:** In the sandbox, write `hats.json` with the current month off, start the app, and screenshot: no hat, antenna showing. Remove the month from `off` through the tray, or restart with the file deleted, and screenshot: hat showing, antenna hidden.
- [ ] **Step 3:** Open the panel with the robot near the top of the screen and near the bottom, and screenshot both. Expected: the hat is whole and the panel doesn't cover it.
- [ ] **Step 4:** Run `scripts/measure.ps1 -TargetDir` on the build, idle and animating, with a hat showing. Expected: CPU, memory and GPU within 10% of the README's current figures; report honestly if not.
- [ ] **Step 5:** Record the results in `README.md`. User checks: the hat on a real 1 November change (or a changed clock), 125% scaling.
- [ ] **Step 6:** Commit `docs: monthly hats verification`.
