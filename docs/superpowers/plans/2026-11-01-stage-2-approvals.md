# DesktopAssistant Stage 2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Answer Claude Code permission requests from the floating window, with every failure ending in the normal prompt.

**Architecture:** For `PermissionRequest`, `da-hook` posts the request to the app, then polls for a decision. Polling keeps the serial listener free and tells the app a session has gone: a request with no poll for 3 seconds is dropped. Pure modules classify requests, vet rules and build the hook's reply. A click in the panel is the only source of an allow.

**Tech Stack:** Existing Stage 1 stack. No new runtime dependencies.

**Spec:** `docs/superpowers/specs/2026-11-01-stage-2-approvals.md`

## Global Constraints

- All Stage 1 constraints still hold, except the two below.
- `da-hook` limit changes for `PermissionRequest` only: it may run for the hold time plus 5 seconds. All other events keep the 2 second limit.
- `tool_input` and `permission_suggestions` may be forwarded only on the permission route, are held in memory only, and are never logged or written to disk.
- `da-hook` writes stdout only after parsing a decision of exactly `allow` or `deny` from the app. Any other result: exit 0 with empty stdout.
- Rules, voice and speech can never approve. Allow comes only from `answer_permission`, reached by a panel click.
- The window keeps `WS_EX_NOACTIVATE`. The panel has buttons only.
- Hold default 120 seconds. Lock 1000 ms. Poll 250 ms. Lost-poll limit 3 seconds.

## Review Focus

- A click after timeout, after the session ended, or after the next request has moved into place: it must affect nothing and never answer a different request. Tests key answers to a request id.
- A second request arriving while the first is locked: the visible request must not change under the cursor.
- Malformed, multi-rule or non-`addRules` suggestions: no session or always button, never a guessed rule.
- Hooks connected by Stage 1 with a 5 second timeout: the normal prompt appears, nothing is approved, and the app offers Connect to repair.
- Content over the 1 MiB limit: handed back, never shortened.

---

### Task 1: Carry-over: done pose and GPU

**Files:**
- Modify: `src-tauri/src/sessions.rs`, `src-tauri/src/app.rs`, `scripts/measure.ps1` only if its output needs it, `README.md`
- Test: inline

**Interfaces:**
- Produces: `Applied.turn_seconds` unchanged. `Sessions::apply` sets Done only when `turn_seconds >= DONE_MIN_SECONDS`; shorter or unknown turns become Idle. `pub const DONE_MIN_SECONDS: u64 = 60`.

- [ ] **Step 1: Write failing tests:** `Stop` at 59 seconds gives Idle; at 60 gives Done; with no turn clock gives Idle; `character_state` for only-Idle sessions stays `asleep`.
- [ ] **Step 2:** Run `cargo test sessions`. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run `cargo test`. Expected: PASS.
- [ ] **Step 5:** Run `scripts/measure.ps1`. Record CPU, memory and GPU in `README.md`. Expected: GPU under 2% animating, reported honestly if not.
- [ ] **Step 6:** Commit `fix: show done only after a long turn`.

### Task 2: Request model

**Files:**
- Create: `src-tauri/src/permission.rs`
- Test: inline

**Interfaces:**
- Produces: `Kind` enum `{ Command, Edit, Web, Mcp, HandBack }`. `classify(tool: &str) -> Kind`. `Rule { tool: String, content: String, destination: String }`. `vet_suggestion(suggestions: &Value) -> Option<Rule>`. `is_broad(rule: &Rule) -> bool`. `Choice` enum `{ Deny, AllowOnce, AllowSession, AlwaysAllow, Release }`. `reply(choice: Choice, rule: Option<&Rule>) -> Option<Value>`; `Release` gives None. `describe_rule(rule: &Rule) -> String`.

- [ ] **Step 1: Write failing tests:**
  - `Bash`, `PowerShell` are Command. `Edit`, `Write`, `NotebookEdit` are Edit. `WebFetch`, `WebSearch` are Web. `mcp__x__y` is Mcp. `AskUserQuestion`, `ExitPlanMode`, unknown names and empty are HandBack.
  - `vet_suggestion` accepts one `addRules` with one `allow` rule that has non-empty `ruleContent`. It rejects none, several, wrong type, `deny`, empty content, `*`, `:*` alone, and non-arrays.
  - `is_broad` is true for content ending in `*`.
  - `reply` for AllowOnce and Deny matches the tested shape exactly. AllowSession carries destination `session`. AlwaysAllow carries `localSettings`. Both of those return None without a rule.
  - A test asserts `reply` is the only function in the crate building a `behavior` value.
- [ ] **Step 2:** Run `cargo test permission`. Expected: FAIL.
- [ ] **Step 3:** Implement. Use `spikes/permission/hook.mjs` for the reply shapes.
- [ ] **Step 4:** Run `cargo test permission`. Expected: PASS.
- [ ] **Step 5:** Commit `feat: classify requests and vet permission rules`.

### Task 3: Pending requests

**Files:**
- Create: `src-tauri/src/pending.rs`
- Test: inline

**Interfaces:**
- Consumes: `permission::{Kind, Choice, Rule}`.
- Produces: `Request { id: String, session_id: String, project: String, tool: String, kind: Kind, input: Value, description: Option<String>, rule: Option<Rule> }`. `Pending::new(hold_secs: u64)`. `open(&mut self, r: Request, now_ms: u64)`. `poll(&mut self, id: &str, now_ms: u64) -> Poll`, with `Poll` enum `{ Waiting, Decided(Choice), Gone }`. `answer(&mut self, id: &str, c: Choice, now_ms: u64) -> Result<(), String>`. `tick(&mut self, now_ms: u64) -> bool`. `current(&self) -> Option<&Request>`, `count(&self) -> usize`.

- [ ] **Step 1: Write failing tests:**
  - FIFO; a new request never replaces `current`.
  - `answer` inside 1000 ms of a request becoming current is `Err`; at 1000 ms is `Ok`; for the Release choice there is no lock.
  - Moving to the next request restarts its lock.
  - `answer` with an old, unknown or non-current id is `Err` and changes nothing.
  - A decided request reports `Decided` once, then `Gone`.
  - Hold expiry: `poll` gives `Gone` at the limit. No poll for 3000 ms removes it, at 2999 ms keeps it.
- [ ] **Step 2:** Run `cargo test pending`. Expected: FAIL.
- [ ] **Step 3:** Implement with no clock reads.
- [ ] **Step 4:** Run `cargo test pending`. Expected: PASS.
- [ ] **Step 5:** Commit `feat: hold permission requests until answered or expired`.

### Task 4: Hook and listener routes

**Files:**
- Modify: `hook/src/main.rs`, `hook/tests/bridge.rs`, `src-tauri/src/listener.rs`, `src-tauri/src/installer.rs`, `src-tauri/src/app.rs`, `src-tauri/src/main.rs`
- Test: bridge and inline

**Interfaces:**
- Consumes: `Pending`, `permission::{classify, vet_suggestion, reply}`.
- Produces: `POST /permission` returns `{ "id": "...", "hold": true|false }`. `GET /permission/<id>` returns 204 while waiting, 200 with the reply JSON when decided, 410 when gone or released. Installer: `PermissionRequest` entry uses timeout 130, others 5; `is_installed` requires it.

- [ ] **Step 1: Write failing tests:**
  - Bridge, against a fake server: `hold: false` exits 0, empty, under 2 seconds. A decided allow prints exactly the server's reply. 410 prints nothing. Reply text that isn't a valid allow or deny decision prints nothing. A dead server mid-hold exits 0, empty. Non-permission events still omit `tool_input`.
  - Listener: both routes need the token. A body over 1 MiB gets 413, so the hook falls through.
  - `Core`: a HandBack tool, hidden window, quiet, or switch off returns `hold: false`. A request also updates the session to NeedsYou once, without doubling the `PermissionRequest` alert.
  - Installer: 130 on `PermissionRequest`; an old 5 second entry reads as stale; `add` repairs it.
- [ ] **Step 2:** Run `cargo test --workspace`. Expected: FAIL.
- [ ] **Step 3:** Implement. `da-hook` keeps the worker-thread limit, using hold plus 5 seconds for this route only, and polls every 250 ms.
- [ ] **Step 4:** Run `cargo test --workspace`. Expected: PASS.
- [ ] **Step 5:** Commit `feat: permission route between hook and app`.

### Task 5: Approval panel

**Files:**
- Create: `ui/approval.js`, `ui/approval.css`
- Modify: `ui/index.html`, `ui/main.js`, `ui/panel.js`, `src-tauri/src/main.rs`, `src-tauri/src/app.rs`
- Test: `scripts/approval.test.mjs`

**Interfaces:**
- Consumes: `Pending`, `Choice`.
- Produces: command `answer_permission(id: String, choice: String) -> Result<(), String>`. `Snapshot.request: Option<RequestView>`, `Snapshot.request_count: usize`. Rust opens the panel without focus when a request appears.

- [ ] **Step 1: Write failing tests:** pure helpers for the view of each Kind, button order, lock states including cursor-already-inside, and the two-step Always allow. All text goes through `textContent`.
- [ ] **Step 2:** Run `node --test scripts/approval.test.mjs`. Expected: FAIL.
- [ ] **Step 3:** Implement. Buttons appear only when their rule exists. The lock is enforced in Rust too. Answers send the request id. Changes render without replacing nodes under the cursor.
- [ ] **Step 4:** Run the tests, then extend `scripts/overlay-check.ps1` to confirm focus stays put through an auto-open, a locked click, and an accepted click. Expected: all True.
- [ ] **Step 5:** Screenshot each Kind and a warning rule in a sandbox, and read the PNGs.
- [ ] **Step 6:** Commit `feat: approve requests from the panel`.

### Task 6: Approve in window switch

**Files:**
- Modify: `src-tauri/src/tray.rs`, `src-tauri/src/app.rs`, `src-tauri/src/main.rs`, `README.md`
- Test: inline

**Interfaces:**
- Consumes: `Core`.
- Produces: `Core::set_approve_in_window(bool)`. Saved as `%APPDATA%\DesktopAssistant\approvals.txt`, default on. `Core::set_visible(bool)`. Turning any of the three off releases pending requests at once.

- [ ] **Step 1: Write failing tests:** default on; saved value restored; malformed file means on; hiding, quiet and the switch each release held requests and refuse new ones.
- [ ] **Step 2:** Run `cargo test`. Expected: FAIL.
- [ ] **Step 3:** Implement the tray check item. Add the README section, including the auto mode limit: approved-by-auto-mode commands never reach the window.
- [ ] **Step 4:** Run `cargo test`. Expected: PASS.
- [ ] **Step 5:** Commit `feat: Approve in window switch`.

### Task 7: End-to-end verification

**Files:**
- Modify: `README.md`
- Test: this task is the test

- [ ] **Step 1:** In a temp project with hooks in its own settings and a sandboxed app, run interactive default-mode sessions. Click Deny, Allow once, Allow this session and Always allow. Confirm each result and that repeat prompts behave as specified, and that `.claude/settings.local.json` holds only the shown rule.
- [ ] **Step 2:** Test timeout, Release, session killed while waiting, app quit while waiting, and app killed while waiting. Expected: normal prompt or clean removal, never an allow.
- [ ] **Step 3:** Test hidden, Do not disturb and switch off. Expected: normal prompt at once.
- [ ] **Step 4:** Record results and remaining user checks in `README.md`. User checks: real Connect repair, the desktop app's Code tab including fallback, subagent sessions, the Stage 1 tray and jump list.
- [ ] **Step 5:** Commit `docs: Stage 2 verification results`.
