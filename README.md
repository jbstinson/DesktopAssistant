# DesktopAssistant

A small always-on-top robot for Windows 11 that watches your Claude Code sessions and tells you when one needs you.

Stage 1 only watches. It never approves, denies or types anything in a session. It uses Claude Code hooks, so it sees sessions from the Claude desktop app, the terminal and `claude -p`. Nothing leaves your machine.

## What it does

- Shows a robot that reacts to what your sessions are doing: asleep, reading, typing, thinking, needs-you, done, error.
- Plays a short sound and speaks a short alert (built-in Windows voice) when a session needs you or finishes a long turn.
- Click the robot to open a panel that lists each session: project, source, state and time in that state. Click a session to jump to it. The small x dismisses it.
- Never takes focus. Empty areas of its window let clicks through. Drag the robot to move it, including to another monitor. It remembers where you left it.

## Build

Needs Rust, Node and the Tauri prerequisites.

```
npm install
cargo build --release -p da-hook
npx tauri build --no-bundle
```

This gives `target\release\desktop-assistant.exe` and `target\release\da-hook.exe`. Keep the two files in the same folder: the app looks for `da-hook.exe` beside itself.

## First run

1. Start `desktop-assistant.exe`. The robot appears at the bottom right of the main monitor, asleep.
2. Right-click the tray icon and choose **Connect to Claude Code**. It asks first, makes a backup, then adds hook entries for nine events to `%USERPROFILE%\.claude\settings.json`. Your own hooks are kept.
3. Start a Claude Code session. The robot wakes up.

Running sessions pick up new hooks only when they start, so restart any session that was already open.

## Tray menu

- **Hide / Show**: hides or shows the robot.
- **Do not disturb**: mutes sound and speech. The robot still animates.
- **Connect to Claude Code**: ticked when the hooks are installed. Click again to remove only our entries.
- **Open rules file**: opens `rules.toml` in your default editor.
- **Quit**.

## Hotkey

`Ctrl+Alt+D` hides or shows the robot without taking focus. If another app already owns that key, the robot shows a warning marker and the tray still works.

## Where files live

Everything is in `%APPDATA%\DesktopAssistant\`: `rules.toml`, `state.json` (robot position) and `endpoint.json` (the local port and token that `da-hook` uses).

## Rules file

`rules.toml` decides how you are told, never what happens. It is created with safe defaults on first run and reloaded when you save it. If you save a broken file, the last working rules stay in use and the robot shows a `!` marker (hover it for the reason). Fix the file and the marker clears.

The first matching rule wins. If none matches, the event is silent.

```toml
[[rule]]
when = { event = "Stop", min_turn_seconds = 60 }
then = { tier = "good_to_know", sound = "bell", speak = "{project} finished" }
```

Conditions (every condition you write must match):

| Condition | Meaning |
|---|---|
| `event` | Hook event name, such as `PermissionRequest`, `Notification`, `Stop`, `StopFailure`, `PostToolUseFailure`. |
| `project` | The project folder name. Not case sensitive. |
| `tool` | Tool name, such as `Bash` or `Edit`. |
| `source` | Where the session runs, as reported by Claude Code (for example `cli`). |
| `notification_type` | The notification kind, such as `permission_prompt` or `idle_prompt`. |
| `min_turn_seconds` | The turn lasted at least this many seconds. |

`event`, `project`, `tool`, `source` and `notification_type` accept one string or a list: `event = ["PermissionRequest", "Notification"]`. Unknown condition names are an error.

Result (`then`):

| Field | Meaning |
|---|---|
| `tier` | `needs_you` (sound and speech, stays until handled), `good_to_know` (marked done) or `silent` (animation only). |
| `sound` | `"alert"` or `"bell"`. |
| `speak` | Text to say. `{project}` becomes the project name. |

Examples:

```toml
# Never speak for the scratch project.
[[rule]]
when = { project = "scratch" }
then = { tier = "silent" }

# A failed Bash call is worth a look.
[[rule]]
when = { event = "PostToolUseFailure", tool = "Bash" }
then = { tier = "good_to_know", sound = "bell" }

# Longer threshold for terminal sessions only.
[[rule]]
when = { event = "Stop", source = "cli", min_turn_seconds = 180 }
then = { tier = "good_to_know", sound = "bell", speak = "{project} is done" }
```

## Known limits

- A Stop cannot tell a finished task from a question asked in plain text. Both look like "done".
- For desktop app sessions, jump brings the Claude app forward, not the specific session.
- For Windows Terminal, jump raises the window, not the tab.
- A waiting session that crashed stays in the list for up to 24 hours unless you dismiss it.
- A dismissed session comes back on its next event.
- With the app closed, each hook call spends about 0.2 seconds before giving up (see results).
- The character ignores the Windows reduced-motion (animation effects) setting for now and always animates.
- Windows only (x86_64).

## Verification results (Stage 1)

Run on Windows 11 with Claude Code 2.1.316. The app ran against a temporary APPDATA, never the real settings. Full details are in `.superpowers/sdd/2026-11-01-stage-1-alerts/task-12-report.md`.

Checked and passing:

- Release build of the app and `da-hook` succeeds.
- Real `claude -p` sessions (haiku, with hooks on all nine events in a temp project) reach the app. A quick reply is silent. A turn of about 80 seconds rings the bell and speaks.
- A permission request followed by a notification gives one alert, not two (checked by measuring audio output: same length as a single request).
- A broken `rules.toml` shows the warning marker, a new needs-you event still alerts, and restoring the file clears the marker.
- The robot and panel work on the portrait monitor (second monitor, x 3840, y -139, 1080x1920).
- Resources, whole process tree (7 processes), from `scripts\measure.ps1`:
  - Asleep: 0.00% CPU, 156 MB private, 312 MB working set, 0% GPU.
  - Animating (waving, stepped animation at about 7 updates a second): 0.00% CPU, 161 MB private, 349 MB working set, 0.54% GPU average (0.59% peak). Before the stepped animation the GPU figure was about 9.7%.
- The robot looks distinct in every state (asleep, reading, typing, thinking, needs-you checked by screenshot).
- A stopped app no longer slows Claude Code much: with the app closed, a quick `claude -p` took about 5.4 s with the hooks and about 4.4 s without (three runs each). `da-hook` takes about 0.2 s per event when the endpoint is stale.

Findings:

- A foreground `sleep 70` is blocked by Claude Code and run in the background instead, so the long-turn check used `ping -n 72` to hold a turn open.

Not yet checked (needs you):

- Connect and Disconnect from the tray against your real `%USERPROFILE%\.claude\settings.json`: confirm only our entries are added, your `SessionStart` and `PostToolUse` hooks stay, and Disconnect restores the original.
- Sessions in the Claude desktop app and in Windows Terminal, including jump.
- 125% display scaling.
- A full-screen app in front.
- Hearing the sound and speech (checked only through audio output level, not by ear).
- Mouse input on the portrait monitor (the test mouse was confined to the main monitor; the click was delivered as a window message).
