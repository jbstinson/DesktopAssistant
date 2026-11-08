//! Session state machine driven by hook events. Pure logic: no Tauri, no clock.
//! All times are u64 seconds supplied by the caller.

use crate::events::HookEvent;
use std::collections::HashMap;

/// A session silent for this many seconds is dropped by `expire`.
const EXPIRE_SECONDS: u64 = 1800;
/// Sessions waiting on the user (NeedsYou or Error), or in the middle of a turn (a tool can run
/// for hours with no hook in between), stay until handled, so they get a day.
const EXPIRE_BLOCKED_SECONDS: u64 = 86400;

/// A turn must last at least this many seconds before Stop shows Done; shorter turns go Idle.
pub const DONE_MIN_SECONDS: u64 = 60;

const READING_TOOLS: &[&str] = &["Read", "Grep", "Glob", "WebFetch", "WebSearch"];
const TYPING_TOOLS: &[&str] = &["Bash", "PowerShell", "Edit", "Write", "NotebookEdit"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activity {
    Thinking,
    Reading,
    Typing,
    NeedsYou,
    Done,
    Error,
    Idle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    pub turn_seconds: Option<u64>,
    pub newly_blocked: bool,
    pub session_known: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionView {
    pub id: String,
    pub project: String,
    pub source: String,
    pub activity: Activity,
    pub since: u64,
    pub terminal_pid: Option<u32>,
}

struct Session {
    project: String,
    source: String,
    activity: Activity,
    since: u64,
    last_seen: u64,
    turn_start: Option<u64>,
    terminal_pid: Option<u32>,
    /// Which agent the current NeedsYou is waiting on: None is the main session.
    blocked_agent: Option<String>,
}

#[derive(Default)]
pub struct Sessions {
    map: HashMap<String, Session>,
}

/// Last non-empty path segment; handles `/` and `\` and trailing separators.
fn project_name(cwd: &str) -> String {
    cwd.split(['/', '\\'])
        .filter(|s| !s.is_empty())
        .last()
        .unwrap_or("")
        .to_string()
}

/// Notification types that mean Claude is waiting for the user. An absent type counts.
fn is_blocking_notification(kind: Option<&str>) -> bool {
    matches!(kind, None | Some("permission_prompt") | Some("elicitation_dialog") | Some("elicitation_url_dialog"))
}

fn waiting_on_another(s: &Session, e: &HookEvent) -> bool {
    s.activity == Activity::NeedsYou && s.blocked_agent != e.agent_id
}

fn tool_activity(tool: Option<&str>) -> Activity {
    match tool {
        Some(t) if READING_TOOLS.contains(&t) => Activity::Reading,
        Some(t) if TYPING_TOOLS.contains(&t) => Activity::Typing,
        _ => Activity::Thinking,
    }
}

impl Sessions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn apply(&mut self, e: &HookEvent, now: u64) -> Applied {
        if e.name == "SessionEnd" {
            let known = self.map.remove(&e.session_id).is_some();
            return Applied { turn_seconds: None, newly_blocked: false, session_known: known };
        }

        let session_known = self.map.contains_key(&e.session_id);
        let s = self.map.entry(e.session_id.clone()).or_insert_with(|| Session {
            project: String::new(),
            source: String::new(),
            activity: Activity::Idle,
            since: now,
            last_seen: now,
            turn_start: None,
            terminal_pid: None,
            blocked_agent: None,
        });

        s.last_seen = now;
        if s.project.is_empty() {
            s.project = project_name(&e.cwd);
        }
        if !e.source.is_empty() {
            s.source = e.source.clone();
        }
        if e.terminal_pid.is_some() {
            s.terminal_pid = e.terminal_pid;
        }

        let mut turn_seconds = None;
        let mut newly_blocked = false;
        let new_activity = match e.name.as_str() {
            "UserPromptSubmit" => {
                s.turn_start = Some(now);
                Some(Activity::Thinking)
            }
            // Only the agent we are waiting on can say the wait is over; a sibling subagent
            // working on something else must not clear it.
            "PreToolUse" if waiting_on_another(s, e) => None,
            "PostToolUseFailure" if s.activity == Activity::NeedsYou => None,
            "PreToolUse" => Some(tool_activity(e.tool.as_deref())),
            // A failed tool call means the session is running again.
            "PostToolUseFailure" => Some(Activity::Thinking),
            // Only notifications that ask the user for something block. Others (idle_prompt,
            // auth_success, types not known yet) change nothing.
            "Notification" if !is_blocking_notification(e.notification_type.as_deref()) => None,
            "PermissionRequest" | "Notification" => {
                newly_blocked = s.activity != Activity::NeedsYou;
                if newly_blocked {
                    s.blocked_agent = e.agent_id.clone();
                }
                Some(Activity::NeedsYou)
            }
            // The user answered an MCP server's question: the agent that asked runs again.
            "ElicitationResult" if s.activity == Activity::NeedsYou && !waiting_on_another(s, e) => {
                Some(Activity::Thinking)
            }
            // PermissionDenied (auto mode refused a call) and an ElicitationResult nobody was
            // waiting on leave the session as it is; Claude carries on by itself.
            "StopFailure" => {
                newly_blocked = s.activity != Activity::Error;
                s.turn_start = None;
                Some(Activity::Error)
            }
            "Stop" => {
                turn_seconds = s.turn_start.take().map(|t| now.saturating_sub(t));
                // Done is for long turns only; a quick reply goes straight back to Idle.
                match turn_seconds {
                    Some(t) if t >= DONE_MIN_SECONDS => Some(Activity::Done),
                    _ => Some(Activity::Idle),
                }
            }
            // SessionStart and anything unrecognised: register the session, change nothing.
            _ => None,
        };

        if let Some(a) = new_activity {
            if a != s.activity {
                s.activity = a;
                s.since = now;
            }
        }

        Applied { turn_seconds, newly_blocked, session_known }
    }

    pub fn project_of(&self, id: &str) -> Option<String> {
        self.map.get(id).map(|s| s.project.clone())
    }

    pub fn dismiss(&mut self, id: &str) {
        self.map.remove(id);
    }

    pub fn expire(&mut self, now: u64) {
        self.map.retain(|_, s| {
            let limit = match s.activity {
                Activity::NeedsYou | Activity::Error => EXPIRE_BLOCKED_SECONDS,
                _ if s.turn_start.is_some() => EXPIRE_BLOCKED_SECONDS,
                _ => EXPIRE_SECONDS,
            };
            now.saturating_sub(s.last_seen) < limit
        });
    }

    /// Highest-priority state across sessions. Idle sessions do not count as active.
    pub fn character_state(&self) -> &'static str {
        let has = |a: Activity| self.map.values().any(|s| s.activity == a);
        if has(Activity::NeedsYou) {
            "needs-you"
        } else if has(Activity::Error) {
            "error"
        } else if has(Activity::Typing) {
            "typing"
        } else if has(Activity::Reading) {
            "reading"
        } else if has(Activity::Thinking) {
            "thinking"
        } else if has(Activity::Done) {
            "done"
        } else {
            "asleep"
        }
    }

    /// Sessions ordered by when they entered their current state, then id.
    pub fn list(&self) -> Vec<SessionView> {
        let mut v: Vec<SessionView> = self
            .map
            .iter()
            .map(|(id, s)| SessionView {
                id: id.clone(),
                project: s.project.clone(),
                source: s.source.clone(),
                activity: s.activity,
                since: s.since,
                terminal_pid: s.terminal_pid,
            })
            .collect();
        v.sort_by(|a, b| a.since.cmp(&b.since).then_with(|| a.id.cmp(&b.id)));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(name: &str, sid: &str, tool: Option<&str>) -> HookEvent {
        HookEvent {
            name: name.into(),
            session_id: sid.into(),
            cwd: "C:\\git\\proj".into(),
            tool: tool.map(String::from),
            notification_type: None,
            source: "terminal".into(),
            terminal_pid: None,
            agent_id: None,
        }
    }

    fn act(s: &Sessions, id: &str) -> Activity {
        s.list().into_iter().find(|v| v.id == id).unwrap().activity
    }

    #[test]
    fn tools_map_to_activity() {
        let mut s = Sessions::new();
        s.apply(&ev("PreToolUse", "a", Some("Read")), 0);
        assert_eq!(act(&s, "a"), Activity::Reading);
        s.apply(&ev("PreToolUse", "a", Some("Bash")), 1);
        assert_eq!(act(&s, "a"), Activity::Typing);
        s.apply(&ev("PreToolUse", "a", Some("SomethingNew")), 2);
        assert_eq!(act(&s, "a"), Activity::Thinking);
        for t in ["Grep", "Glob", "WebFetch", "WebSearch"] {
            s.apply(&ev("PreToolUse", "a", Some(t)), 3);
            assert_eq!(act(&s, "a"), Activity::Reading, "{t}");
        }
        for t in ["PowerShell", "Edit", "Write", "NotebookEdit"] {
            s.apply(&ev("PreToolUse", "a", Some(t)), 3);
            assert_eq!(act(&s, "a"), Activity::Typing, "{t}");
        }
    }

    #[test]
    fn prompt_submit_is_thinking() {
        let mut s = Sessions::new();
        s.apply(&ev("UserPromptSubmit", "a", None), 10);
        assert_eq!(act(&s, "a"), Activity::Thinking);
    }

    #[test]
    fn stop_reports_turn_seconds() {
        let mut s = Sessions::new();
        s.apply(&ev("UserPromptSubmit", "a", None), 100);
        let r = s.apply(&ev("Stop", "a", None), 175);
        assert_eq!(r.turn_seconds, Some(75));
        assert_eq!(act(&s, "a"), Activity::Done);
        // A second Stop has no turn clock running.
        let r = s.apply(&ev("Stop", "a", None), 180);
        assert_eq!(r.turn_seconds, None);
    }

    #[test]
    fn stop_below_limit_is_idle_and_at_limit_is_done() {
        let mut s = Sessions::new();
        s.apply(&ev("UserPromptSubmit", "a", None), 100);
        let r = s.apply(&ev("Stop", "a", None), 100 + DONE_MIN_SECONDS - 1);
        assert_eq!(r.turn_seconds, Some(DONE_MIN_SECONDS - 1));
        assert_eq!(act(&s, "a"), Activity::Idle);
        s.apply(&ev("UserPromptSubmit", "a", None), 200);
        s.apply(&ev("Stop", "a", None), 200 + DONE_MIN_SECONDS);
        assert_eq!(act(&s, "a"), Activity::Done);
    }

    #[test]
    fn stop_without_turn_clock_is_idle() {
        let mut s = Sessions::new();
        s.apply(&ev("Stop", "a", None), 5);
        assert_eq!(act(&s, "a"), Activity::Idle);
    }

    #[test]
    fn short_stop_clears_needs_you_to_idle() {
        let mut s = Sessions::new();
        s.apply(&ev("UserPromptSubmit", "a", None), 1);
        s.apply(&ev("PermissionRequest", "a", Some("Bash")), 2);
        s.apply(&ev("Stop", "a", None), 3);
        assert_eq!(act(&s, "a"), Activity::Idle);
    }

    #[test]
    fn only_idle_sessions_stay_asleep() {
        let mut s = Sessions::new();
        s.apply(&ev("UserPromptSubmit", "a", None), 1);
        s.apply(&ev("Stop", "a", None), 5);
        assert_eq!(s.character_state(), "asleep");
    }

    #[test]
    fn stop_without_prompt_has_no_turn_seconds() {
        let mut s = Sessions::new();
        let r = s.apply(&ev("Stop", "a", None), 5);
        assert_eq!(r.turn_seconds, None);
    }

    #[test]
    fn newly_blocked_only_first_time() {
        let mut s = Sessions::new();
        let r = s.apply(&ev("PermissionRequest", "a", Some("Bash")), 1);
        assert!(r.newly_blocked);
        assert_eq!(act(&s, "a"), Activity::NeedsYou);
        let r = s.apply(&ev("Notification", "a", None), 2);
        assert!(!r.newly_blocked);
        assert_eq!(act(&s, "a"), Activity::NeedsYou);
    }

    #[test]
    fn tool_use_clears_needs_you() {
        let mut s = Sessions::new();
        s.apply(&ev("PermissionRequest", "a", Some("Bash")), 1);
        s.apply(&ev("PreToolUse", "a", Some("Bash")), 2);
        assert_eq!(act(&s, "a"), Activity::Typing);
        // Blocking again counts as newly blocked.
        let r = s.apply(&ev("Notification", "a", None), 3);
        assert!(r.newly_blocked);
    }

    #[test]
    fn a_url_elicitation_blocks() {
        let mut s = Sessions::new();
        let mut e = ev("Notification", "a", None);
        e.notification_type = Some("elicitation_url_dialog".into());
        assert!(s.apply(&e, 1).newly_blocked);
        assert_eq!(act(&s, "a"), Activity::NeedsYou);
    }

    #[test]
    fn answering_an_elicitation_clears_needs_you() {
        let mut s = Sessions::new();
        let mut e = ev_agent("Notification", "a", None, Some("sub"));
        e.notification_type = Some("elicitation_dialog".into());
        s.apply(&e, 1);
        // Another agent's answer does not end this wait; the waiting agent's does.
        s.apply(&ev_agent("ElicitationResult", "a", None, None), 2);
        assert_eq!(act(&s, "a"), Activity::NeedsYou);
        let r = s.apply(&ev_agent("ElicitationResult", "a", None, Some("sub")), 3);
        assert!(!r.newly_blocked);
        assert_eq!(act(&s, "a"), Activity::Thinking);
    }

    #[test]
    fn an_elicitation_result_while_not_waiting_changes_nothing() {
        let mut s = Sessions::new();
        s.apply(&ev("PreToolUse", "a", Some("Read")), 1);
        s.apply(&ev("ElicitationResult", "a", None), 2);
        assert_eq!(act(&s, "a"), Activity::Reading);
    }

    #[test]
    fn a_permission_denial_does_not_block() {
        let mut s = Sessions::new();
        s.apply(&ev("PreToolUse", "a", Some("Bash")), 1);
        let r = s.apply(&ev("PermissionDenied", "a", Some("Bash")), 2);
        assert!(!r.newly_blocked);
        assert_eq!(act(&s, "a"), Activity::Typing);
    }

    #[test]
    fn stop_failure_is_error_and_newly_blocked_once() {
        let mut s = Sessions::new();
        s.apply(&ev("UserPromptSubmit", "a", None), 1);
        let r = s.apply(&ev("StopFailure", "a", None), 2);
        assert!(r.newly_blocked);
        assert_eq!(act(&s, "a"), Activity::Error);
        let r = s.apply(&ev("StopFailure", "a", None), 3);
        assert!(!r.newly_blocked);
        assert_eq!(act(&s, "a"), Activity::Error);
    }

    #[test]
    fn stop_failure_after_needs_you_is_newly_blocked() {
        let mut s = Sessions::new();
        s.apply(&ev("PermissionRequest", "a", None), 1);
        let r = s.apply(&ev("StopFailure", "a", None), 2);
        assert!(r.newly_blocked);
    }

    #[test]
    fn session_end_and_dismiss_remove() {
        let mut s = Sessions::new();
        s.apply(&ev("PreToolUse", "a", Some("Read")), 1);
        s.apply(&ev("PreToolUse", "b", Some("Read")), 1);
        s.apply(&ev("SessionEnd", "a", None), 2);
        assert_eq!(s.list().len(), 1);
        s.dismiss("b");
        assert!(s.list().is_empty());
        s.dismiss("nope"); // no panic
    }

    #[test]
    fn expire_removes_at_1800_keeps_1799() {
        let mut s = Sessions::new();
        s.apply(&ev("PreToolUse", "old", Some("Read")), 0);
        s.apply(&ev("PreToolUse", "new", Some("Read")), 1);
        s.expire(1799);
        assert_eq!(s.list().len(), 2); // old silent 1799, kept
        s.expire(1800);
        let ids: Vec<_> = s.list().into_iter().map(|v| v.id).collect();
        assert_eq!(ids, vec!["new".to_string()]); // old silent 1800, new silent 1799
    }

    #[test]
    fn blocked_sessions_outlive_the_normal_expiry() {
        let mut s = Sessions::new();
        s.apply(&ev("PermissionRequest", "wait", None), 0);
        s.apply(&ev("StopFailure", "err", None), 0);
        s.apply(&ev("PreToolUse", "quiet", Some("Read")), 0);
        s.expire(1800);
        let mut ids: Vec<_> = s.list().into_iter().map(|v| v.id).collect();
        ids.sort();
        assert_eq!(ids, vec!["err".to_string(), "wait".to_string()]);
        s.expire(86399);
        assert_eq!(s.list().len(), 2);
        s.expire(86400);
        assert!(s.list().is_empty());
    }

    #[test]
    fn dismiss_clears_a_blocked_session_immediately() {
        let mut s = Sessions::new();
        s.apply(&ev("PermissionRequest", "wait", None), 0);
        s.dismiss("wait");
        assert!(s.list().is_empty());
    }

    #[test]
    fn unseen_session_is_created() {
        let mut s = Sessions::new();
        let r = s.apply(&ev("PreToolUse", "x", Some("Edit")), 50);
        assert!(!r.session_known);
        assert_eq!(act(&s, "x"), Activity::Typing);
        let r = s.apply(&ev("PreToolUse", "x", Some("Edit")), 51);
        assert!(r.session_known);
    }

    #[test]
    fn character_state_priority() {
        let mut s = Sessions::new();
        assert_eq!(s.character_state(), "asleep");
        s.apply(&ev("UserPromptSubmit", "d", None), 0);
        s.apply(&ev("Stop", "d", None), 100);
        assert_eq!(s.character_state(), "done");
        s.apply(&ev("UserPromptSubmit", "t", None), 1);
        assert_eq!(s.character_state(), "thinking");
        s.apply(&ev("PreToolUse", "r", Some("Read")), 1);
        assert_eq!(s.character_state(), "reading");
        s.apply(&ev("PreToolUse", "w", Some("Bash")), 1);
        assert_eq!(s.character_state(), "typing");
        s.apply(&ev("StopFailure", "e", None), 1);
        assert_eq!(s.character_state(), "error");
        s.apply(&ev("PermissionRequest", "n", None), 1);
        assert_eq!(s.character_state(), "needs-you");
        s.dismiss("n");
        assert_eq!(s.character_state(), "error");
    }

    #[test]
    fn since_changes_only_on_activity_change() {
        let mut s = Sessions::new();
        s.apply(&ev("PreToolUse", "a", Some("Read")), 10);
        s.apply(&ev("PreToolUse", "a", Some("Grep")), 20);
        assert_eq!(s.list()[0].since, 10);
        s.apply(&ev("PreToolUse", "a", Some("Bash")), 30);
        assert_eq!(s.list()[0].since, 30);
    }

    #[test]
    fn project_name_handles_separators() {
        assert_eq!(project_name("C:\\git\\Proj"), "Proj");
        assert_eq!(project_name("C:\\git\\Proj\\"), "Proj");
        assert_eq!(project_name("/home/me/proj/"), "proj");
        assert_eq!(project_name("C:\\"), "C:");
        assert_eq!(project_name(""), "");
    }

    #[test]
    fn view_carries_source_project_pid() {
        let mut s = Sessions::new();
        let mut e = ev("PreToolUse", "a", Some("Read"));
        e.terminal_pid = Some(7);
        s.apply(&e, 1);
        let v = &s.list()[0];
        assert_eq!(v.project, "proj");
        assert_eq!(v.source, "terminal");
        assert_eq!(v.terminal_pid, Some(7));
        // Later event with empty source / no pid keeps earlier values.
        let mut e2 = ev("PreToolUse", "a", Some("Read"));
        e2.source = String::new();
        s.apply(&e2, 2);
        let v = &s.list()[0];
        assert_eq!(v.source, "terminal");
        assert_eq!(v.terminal_pid, Some(7));
    }

    fn ev_agent(name: &str, sid: &str, tool: Option<&str>, agent: Option<&str>) -> HookEvent {
        let mut e = ev(name, sid, tool);
        e.agent_id = agent.map(String::from);
        e
    }

    #[test]
    fn long_running_tool_is_not_expired_mid_turn() {
        let mut s = Sessions::new();
        s.apply(&ev("UserPromptSubmit", "a", None), 0);
        s.apply(&ev("PreToolUse", "a", Some("Bash")), 1);
        s.expire(1 + 1799);
        assert_eq!(s.list().len(), 1);
        s.expire(1 + 1800); // old limit would drop it here
        assert_eq!(s.list().len(), 1);
        s.expire(1 + 86399);
        assert_eq!(s.list().len(), 1);
        let r = s.apply(&ev("Stop", "a", None), 2000);
        assert_eq!(r.turn_seconds, Some(2000));
        // Turn over: the normal limit applies again.
        s.expire(2000 + 1799);
        assert_eq!(s.list().len(), 1);
        s.expire(2000 + 1800);
        assert!(s.list().is_empty());
    }

    #[test]
    fn turn_in_progress_expires_after_a_day() {
        let mut s = Sessions::new();
        s.apply(&ev("UserPromptSubmit", "a", None), 0);
        s.expire(86399);
        assert_eq!(s.list().len(), 1);
        s.expire(86400);
        assert!(s.list().is_empty());
    }

    #[test]
    fn sibling_pre_tool_use_does_not_clear_needs_you() {
        let mut s = Sessions::new();
        s.apply(&ev_agent("PermissionRequest", "a", Some("Bash"), Some("sub1")), 1);
        assert_eq!(act(&s, "a"), Activity::NeedsYou);
        s.apply(&ev_agent("PreToolUse", "a", Some("Read"), Some("sub2")), 2);
        assert_eq!(act(&s, "a"), Activity::NeedsYou);
        assert_eq!(s.list()[0].since, 1);
        s.apply(&ev_agent("PreToolUse", "a", Some("Read"), None), 3);
        assert_eq!(act(&s, "a"), Activity::NeedsYou);
        // The blocked agent itself resuming clears it.
        s.apply(&ev_agent("PreToolUse", "a", Some("Bash"), Some("sub1")), 4);
        assert_eq!(act(&s, "a"), Activity::Typing);
    }

    #[test]
    fn main_session_block_is_cleared_only_by_main_session() {
        let mut s = Sessions::new();
        s.apply(&ev("Notification", "a", None), 1);
        s.apply(&ev_agent("PreToolUse", "a", Some("Read"), Some("sub1")), 2);
        assert_eq!(act(&s, "a"), Activity::NeedsYou);
        s.apply(&ev("PreToolUse", "a", Some("Read")), 3);
        assert_eq!(act(&s, "a"), Activity::Reading);
    }

    #[test]
    fn prompt_stop_and_end_clear_needs_you_whatever_the_agent() {
        for name in ["UserPromptSubmit", "Stop"] {
            let mut s = Sessions::new();
            s.apply(&ev_agent("PermissionRequest", "a", None, Some("sub1")), 1);
            s.apply(&ev(name, "a", None), 2);
            assert_ne!(act(&s, "a"), Activity::NeedsYou, "{name}");
        }
        let mut s = Sessions::new();
        s.apply(&ev_agent("PermissionRequest", "a", None, Some("sub1")), 1);
        s.apply(&ev("SessionEnd", "a", None), 2);
        assert!(s.list().is_empty());
    }

    #[test]
    fn sibling_event_then_notification_does_not_realert() {
        let mut s = Sessions::new();
        assert!(s.apply(&ev_agent("PermissionRequest", "a", None, Some("sub1")), 1).newly_blocked);
        s.apply(&ev_agent("PreToolUse", "a", Some("Read"), Some("sub2")), 2);
        let r = s.apply(&ev_agent("Notification", "a", None, Some("sub1")), 3);
        assert!(!r.newly_blocked);
    }

    #[test]
    fn only_prompt_notification_types_block() {
        for (ty, blocks) in [
            (Some("permission_prompt"), true),
            (Some("elicitation_dialog"), true),
            (None, true),
            (Some("auth_success"), false),
            (Some("idle_prompt"), false),
            (Some("something_new"), false),
        ] {
            let mut s = Sessions::new();
            s.apply(&ev("UserPromptSubmit", "a", None), 1);
            s.apply(&ev("Stop", "a", None), 100);
            let mut e = ev("Notification", "a", None);
            e.notification_type = ty.map(String::from);
            let r = s.apply(&e, 110);
            assert_eq!(r.newly_blocked, blocks, "{ty:?}");
            let want = if blocks { Activity::NeedsYou } else { Activity::Done };
            assert_eq!(act(&s, "a"), want, "{ty:?}");
            if !blocks {
                assert_eq!(s.list()[0].since, 100, "{ty:?}");
            }
        }
    }

    #[test]
    fn first_non_empty_project_is_kept() {
        let mut s = Sessions::new();
        let mut e = ev("SessionStart", "a", None);
        e.cwd = String::new();
        s.apply(&e, 1);
        assert_eq!(s.list()[0].project, "");
        s.apply(&ev("PreToolUse", "a", Some("Read")), 2); // cwd C:\git\proj
        assert_eq!(s.list()[0].project, "proj");
        let mut e = ev("PreToolUse", "a", Some("Read"));
        e.cwd = r"C:\git\other".into();
        s.apply(&e, 3);
        assert_eq!(s.list()[0].project, "proj");
        let mut e = ev("PreToolUse", "a", Some("Read"));
        e.cwd = String::new();
        s.apply(&e, 4);
        assert_eq!(s.list()[0].project, "proj");
    }

    #[test]
    fn post_tool_use_is_not_a_known_event() {
        // Nothing installed sends it, so it must not change state.
        let mut s = Sessions::new();
        s.apply(&ev("PermissionRequest", "a", None), 1);
        s.apply(&ev("PostToolUse", "a", Some("Bash")), 2);
        assert_eq!(act(&s, "a"), Activity::NeedsYou);
    }

    #[test]
    fn idle_prompt_changes_nothing() {
        let mut s = Sessions::new();
        s.apply(&ev("UserPromptSubmit", "a", None), 1);
        s.apply(&ev("Stop", "a", None), 100);
        let mut e = ev("Notification", "a", None);
        e.notification_type = Some("idle_prompt".into());
        let r = s.apply(&e, 110);
        assert!(!r.newly_blocked);
        assert_eq!(act(&s, "a"), Activity::Done);
        assert_eq!(s.list()[0].since, 100);
        // Other notification types still block.
        let mut e = ev("Notification", "a", None);
        e.notification_type = Some("permission_prompt".into());
        assert!(s.apply(&e, 111).newly_blocked);
    }
}
