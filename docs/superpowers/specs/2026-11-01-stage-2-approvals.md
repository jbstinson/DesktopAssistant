# DesktopAssistant Stage 2: Approvals

Answer Claude Code permission requests from the floating window. Starting tasks by typing or voice is Stage 3 and out of scope.

## Tested behavior (Claude Code 2.1.316, spikes/permission)

- `PermissionRequest` fires in interactive default mode and in `claude -p`. It does not fire when auto mode approves by itself.
- Explicit allow and deny work in both.
- An empty reply, crash, exit 2, bad JSON or hang never approved anything. Interactively, crash, bad JSON and hang all fell back to the normal prompt.
- The normal prompt appears only after the hook ends.
- `updatedPermissions` with destination `session` applied for that session and changed no file. Tested in `claude -p` only.
- `updatedPermissions` with destination `localSettings` wrote `.claude/settings.local.json` and held for a new session. Tested in `claude -p` only.
- Not tested: fallback in the desktop app's Code tab.

## Requirements

**Handling.** The window handles shell commands, file edits and writes, web tools and MCP tools. Claude's questions, plan approvals and any unrecognized tool go straight back to Claude Code with no wait.

**Hold.** A request waits 120 seconds by default, configurable, then goes back to Claude Code's normal prompt. It is handed back at once when the robot is hidden, Do not disturb is on, or the new **Approve in window** tray switch is off. That switch is on by default. If the app is closed, the normal prompt appears at once.

**Panel.** A request opens the panel without taking focus. Requests appear one at a time with a count. It shows session, project and full content: commands in full with Claude's description, edits as file path plus changes, other tools as name plus all inputs. Nothing is shortened; long content scrolls. Content too large to send is handed back.

**Answers.** Deny · Allow once · Allow this session · Always allow, Deny on the left and Allow on the right, plus an **Answer in Claude Code** link that releases the request at once and approves nothing.

**Click safety.** Buttons stay locked for 1 second after a request appears. If the cursor is over the panel when it appears, they stay locked until it leaves and returns. Voice never approves. Rules never approve. Only a click in the panel can.

**Allow this session.** Uses a session-scoped rule that is shown before use. It wears off with the session and changes no file.

**Always allow.** Applies to this project on this PC. The first click shows the exact rule in plain words and the second saves it. Rules allowing a whole tool or every command are never offered. Broad rules ending in `*` show a warning.

**Lifecycle.** A session interrupted or ended while waiting removes its request; late clicks do nothing. On timeout the buttons disappear, and the robot keeps waving until the session moves on.

**Failure.** Every failure path ends in Claude Code's normal prompt. Nothing but an explicit click produces an allow.

**Stage 1 carry-over.** Re-measure GPU use. Show "done" only after a turn long enough to ring the bell. Run the unrun tray and session checks.

## Out of scope

Typing to the bot, starting tasks, voice, answering questions or plans from the window, a settings screen, display scaling, a reduced-motion setting.
