# Stage 2.5 Avatars Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The character can be the Robot, a Dog, a Cat or a Paperclip, picked from the tray, each acting out every state and wearing the monthly hats.

**Architecture:** Rust owns the choice: `avatar.rs` loads and saves it, `Core` puts it in the snapshot as an id, and the tray's Avatar submenu changes it. The page builds the character SVG from one art module per avatar plus a shared fragment of marks (thinking dots, "zzz", alarm, badge), and swaps the SVG when the id changes. Pose CSS is scoped per avatar with `[data-avatar="…"]`; the rules for marks and props stay shared.

**Tech Stack:** Existing stack. No new dependencies.

**Spec:** `docs/superpowers/specs/2026-11-07-stage-2-5-avatars-design.md`

**Approved sketches:** `.superpowers/brainstorm/1377-1794086840/content/avatars.html` (git-ignored, on disk). Start each avatar's idle art from its sketch.

## Global Constraints

- Avatar ids and tray labels, in this order: `robot` Robot, `dog` Dog, `cat` Cat, `paperclip` Paperclip.
- Default `robot`. Saved as `%APPDATA%\DesktopAssistant\avatar.txt` holding the id; missing, empty or unknown means `robot`.
- Every avatar fits the 140×140 viewBox, head top at about y=22, and has the ids `head` (containing an empty `<g id="hat">` as its last child), `keys`, `book`, `book-page`, `bell` and `eyes`. Anything a hat would clash with is inside `<g id="antenna">`.
- The shared marks (`think`, `think-1..3`, `zzz`, `alarm`, `badge`, `badge-count`) come from one fragment, drawn after the avatar, at today's positions.
- The robot must look and move exactly as today.
- Animation: transform and opacity only, `steps()` or `step-end` timing, nothing new while `body.paused`.
- Window size, hit circle, panel, focus and approvals unchanged.

## Review Focus

- Switching avatar while a hat is showing or a state is active: the new avatar shows the same hat and the same state at once. (Task 2 check in the preview and the app.)
- Switching avatar while the warning marker or the panel is showing: `#warn` and the panel survive the swap. (Task 2: remount replaces only the SVG.)
- An `avatar.txt` with junk, an old id or trailing whitespace: Robot, or the trimmed id. (Task 1 test.)
- Each avatar in needs-you with `data-multi="true"`, error and done: the badge, alarm and bell are visible and not hidden behind the art or the hat. (Tasks 3–5 visual checks.)
- The paperclip's small hats sit on the clip and tilt with it. (Task 5 visual check.)

---

### Task 1: Avatar choice in Rust and the tray

**Files:**
- Create: `src-tauri/src/avatar.rs`
- Modify: `src-tauri/src/app.rs`, `src-tauri/src/tray.rs`, `src-tauri/src/main.rs`, `src-tauri/src/permission.rs` (the source-file guard list), `README.md`
- Test: inline

**Interfaces:**
- Produces in `avatar.rs`: `pub const AVATARS: [(&str, &str); 4]` (id, label). `pub fn known(id: &str) -> &'static str` (the matching id, else `"robot"`). `pub fn load(path: &Path) -> &'static str`. `pub fn save(path: &Path, id: &str) -> std::io::Result<()>` (atomic, like `voice::save`).
- Produces in `app.rs`: `Core::set_avatar(&mut self, id: &'static str)`, `Core::avatar(&self) -> &'static str` (default `"robot"`). `Snapshot.avatar: &'static str`, serialized as a string.
- Produces in `tray.rs`: `pub struct AvatarMenu { pub path: PathBuf }` passed to `install`; an **Avatar** submenu after Voice and before Holidays, items `avatar:<id>`, one ticked.

- [ ] **Step 1: Write failing tests:**
  - `known("dog") == "dog"`; `known("")`, `known("Dog")`, `known("ultron")` are `"robot"`.
  - Missing file loads `"robot"`. `"cat\n"` and `"  paperclip  "` load the trimmed id. `"giant"` loads `"robot"`.
  - Save then load round-trips into a new nested folder and leaves no `.txt.tmp`.
  - Core: a new Core's snapshot has `avatar == "robot"`; after `set_avatar("cat")` it is `"cat"`; `serde_json::to_value(snapshot)["avatar"] == "cat"`.
- [ ] **Step 2:** Run `cargo test -p desktop-assistant avatar` and `cargo test -p desktop-assistant app`. Expected: FAIL.
- [ ] **Step 3:** Implement. Load the saved avatar in `main.rs` setup before the tray. A tray pick sets Core, ticks only the picked item, saves (logging a save error), then publishes, with the core lock released before publishing. Add `avatar.rs` to the guard list in `permission.rs`.
- [ ] **Step 4:** README: an **Avatar** line in the tray section, `avatar.txt` in "Where files live".
- [ ] **Step 5:** Run `cargo test --workspace`. Expected: PASS.
- [ ] **Step 6:** Commit `feat: avatar choice in the tray`.

### Task 2: Character built per avatar, robot unchanged

**Files:**
- Create: `ui/avatars/index.js`, `ui/avatars/robot.js`, `scripts/avatars.test.mjs`
- Modify: `ui/character.svg.js`, `ui/character.css`, `ui/main.js`, `scripts/hats-preview.html`
- Test: `scripts/avatars.test.mjs`, visual check

**Interfaces:**
- Consumes: `snap.avatar`.
- Produces in `ui/avatars/index.js`: `export const AVATARS` (object keyed by id, each `{ id, art, hatTransform }`, where `art` is the SVG markup for that avatar and `hatTransform` is `""` or an SVG transform string applied to `#hat`). `export function avatarFor(id)` (the entry, or the robot's for anything unknown).
- Produces in `ui/character.svg.js`: `export const MARKS` (the shared marks fragment). `export function mountCharacter(container, id)`: replaces only an existing `svg#character` in `container` (inserting it first if none), sets `#hat`'s `transform` from `hatTransform`, and returns `{ badgeCount, hat }`.
- Produces in CSS: robot pose rules scoped to `[data-avatar="robot"]`; rules that only show or hide marks and props stay unscoped.

- [ ] **Step 1: Write failing tests** in `scripts/avatars.test.mjs`, parsing markup as text: `avatarFor("nope") === avatarFor("robot")`. For each avatar in `AVATARS`: `art` contains each required id (`head`, `hat`, `keys`, `book`, `book-page`, `bell`, `eyes`); `#hat` is an empty `<g id="hat"></g>` inside `#head` (its index falls between `id="head"` and the head group's end, checked by the order of the `hat` id after `head`); `art` plus `MARKS` define no id twice; `MARKS` defines `think`, `think-1`, `think-2`, `think-3`, `zzz`, `alarm`, `badge`, `badge-count`. The keys of `AVATARS` are `robot`, `dog`, `cat`, `paperclip` in that order (the later tasks' avatars start as copies of the robot's art so this passes).
- [ ] **Step 2:** Run `node --test scripts/avatars.test.mjs`. Expected: FAIL.
- [ ] **Step 3:** Implement. Move the robot's art into `ui/avatars/robot.js` and the marks into `MARKS`, keeping every robot id and coordinate. Dog, cat and paperclip start as placeholders that reuse the robot art. In `main.js`, remount when `snap.avatar` changes, set `body.dataset.avatar`, and re-apply the current hat and badge after a remount. `#warn` stays outside the SVG and survives.
- [ ] **Step 4:** Run `node --test scripts/*.test.mjs`. Expected: PASS.
- [ ] **Step 5:** Extend `scripts/hats-preview.html` with an avatar picker (a `?avatar=` URL parameter is enough) and set `data-avatar` on each cell. Serve with `node scripts/serve-ui.mjs`, open the robot grid, and compare against the merged `main` build's preview: the robot must look the same in every state and month.
- [ ] **Step 6:** Commit `refactor: build the character from per-avatar art`.

### Task 3: Dog

**Files:**
- Create: `ui/avatars/dog.js`, `ui/avatars/dog.css`
- Modify: `ui/avatars/index.js`, `ui/index.html`, `scripts/hats-preview.html`
- Test: `scripts/avatars.test.mjs`, visual check

- [ ] **Step 1:** Run `node --test scripts/avatars.test.mjs` after swapping the placeholder for the sketch art with the required ids. Expected: PASS (the shared test covers the ids).
- [ ] **Step 2:** Pose CSS under `[data-avatar="dog"]` for every state, following the spec's suggestions: paws on the keys, paws on the book, raised waving paw, bell in a paw, startled ears and tail, curled up asleep, tail swish. Same timing style as the robot.
- [ ] **Step 3:** Preview every state and month for the dog; screenshot and read the PNG. Check that the hat sits on the head and tilts with it, and that the badge, alarm, bell and thinking dots show. Fix the art until they do.
- [ ] **Step 4:** Commit `feat: dog avatar`.

### Task 4: Cat

Same as Task 3 with `cat.js` and `cat.css`, and the cat's look from the spec. The ears go under the hat, not inside `#antenna`. Commit `feat: cat avatar`.

### Task 5: Paperclip

Same as Task 3 with `paperclip.js` and `paperclip.css`, plus a `hatTransform` that scales the hats down (about half size) and centres them on the top of the clip. Check every month's hat on the paperclip in the asleep and thinking tilts. Commit `feat: paperclip avatar`.

### Task 6: Verification

**Files:**
- Modify: `README.md`
- Test: this task is the test

- [ ] **Step 1:** Release build into the scratch target. Run `scripts/overlay-check.ps1` with `DA_EXE` set to it. Expected: everything True except the hotkey and drag checks while another copy of the app owns Ctrl+Alt+D (report that honestly).
- [ ] **Step 2:** In a sandbox, start with `avatar.txt` set to each id in turn and screenshot the idle character with the month's hat. Then open the panel and check that it still fits.
- [ ] **Step 3:** Run `scripts/measure.ps1 -TargetDir` with `avatar.txt` set to `cat`. Expected: within 10% of the README's current figures.
- [ ] **Step 4:** Record results in a README "Verification results (Stage 2.5)" section. User checks: switching avatars from the tray while the app runs, and each avatar in real sessions.
- [ ] **Step 5:** Commit `docs: Stage 2.5 verification`.
