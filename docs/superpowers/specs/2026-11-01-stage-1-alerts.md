# DesktopAssistant Stage 1: Alerts

An always-on-top 2D character for Windows 11 that watches Claude Code sessions and tells the user when one needs them. Later stages add Allow/Deny from the window (Stage 2) and starting tasks by typing or voice (Stage 3). Neither is in scope here.

## Requirements

**Sessions.** Watch sessions in the Claude desktop app, the terminal and `claude -p`, using Claude Code hooks. Several sessions can run at once. Stage 1 only watches; it never sends anything to a session.

**Character.** A 2D vector friendly robot with these states:

| State | Trigger | Animation |
|---|---|---|
| asleep | No active sessions | Still, no animation |
| reading | `Read`, `Grep`, `Glob`, `WebFetch`, `WebSearch` | Looks around |
| typing | `Bash`, `PowerShell`, `Edit`, `Write`, `NotebookEdit` | Types |
| thinking | Prompt submitted, other activity | Looks up |
| needs-you | Needs-you tier alert | Waves until handled, with a count if more than one |
| done | Good-to-know tier alert | Rings a bell, then settles |
| error | Session failed (`StopFailure`) | Looks alarmed until handled |

The art must stay replaceable: code only ever asks for a state by name.

**Panel.** Clicking the character opens a panel listing each session's project, source, state and time in that state. Clicking a session jumps to it. A session can be dismissed by hand.

**Alert tiers.** Defaults:

| Tier | Events | Behavior |
|---|---|---|
| needs_you | `PermissionRequest`, `Notification`, `StopFailure` | Sound and speech; stays until handled |
| good_to_know | `Stop` after a turn of 60 seconds or more | Bell; marked done |
| silent | Quick turns, `PostToolUseFailure`, activity | Animation only |

**Rules.** A readable rules file holds the tiers and is reloaded on save. A broken file keeps the last working rules and shows a warning. Rules decide only how the user is told. Conditions in Stage 1: event, project, tool, source, minimum turn seconds. Designed so new conditions are small additions. A settings screen comes later.

**Speech and sound.** Original sounds. Short spoken alerts through built-in Windows voices. Queued, never overlapping. Do-not-disturb mutes both. Nothing leaves the machine.

**Window behavior.** Never takes focus, including on launch and click. Empty areas pass clicks through. Draggable to either monitor, remembers its position, and recovers if that monitor is gone. A hotkey shows or hides it, and a tray icon stays available.

**Click-to-jump.** Brings the session's terminal window forward, or the Claude desktop app for desktop sessions. Opening one specific desktop session is unconfirmed; if it can't be done, the panel names the session.

**Hook connection.** The user turns it on from the tray. It adds entries to `~/.claude/settings.json` without disturbing existing hooks, and can remove them. The listener binds to 127.0.0.1 and requires a token. Hooks must always exit 0 quickly, so a stopped app never slows or stalls Claude Code.

**Resources.** Idle: near-zero CPU, no GPU work, about 200 MB private memory or less. Animation stops when asleep or hidden.

**Shareable later.** No hardcoded paths, no Tiny Engineer names, art or sounds.

## Out of scope

Allow/Deny, starting tasks, typing to the bot, voice input, a settings screen, signing, auto-update.
