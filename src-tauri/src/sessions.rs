//! Session state machine driven by hook events. Pure logic: no Tauri, no clock.
//! All times are u64 seconds supplied by the caller.

use crate::events::HookEvent;
use std::collections::HashMap;

/// A session silent for this many seconds is dropped by `expire`.
const EXPIRE_SECONDS: u64 = 1800;

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
        });

        s.last_seen = now;
        s.project = project_name(&e.cwd);
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
            "PreToolUse" => Some(tool_activity(e.tool.as_deref())),
            // Work resumed after a block or failure; a tool finishing means it is running again.
            "PostToolUse" | "PostToolUseFailure" => Some(Activity::Thinking),
            "PermissionRequest" | "Notification" => {
                newly_blocked = s.activity != Activity::NeedsYou;
                Some(Activity::NeedsYou)
            }
            "StopFailure" => {
                newly_blocked = s.activity != Activity::Error;
                s.turn_start = None;
                Some(Activity::Error)
            }
            "Stop" => {
                turn_seconds = s.turn_start.take().map(|t| now.saturating_sub(t));
                Some(Activity::Done)
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

    pub fn dismiss(&mut self, id: &str) {
        self.map.remove(id);
    }

    pub fn expire(&mut self, now: u64) {
        self.map.retain(|_, s| now.saturating_sub(s.last_seen) < EXPIRE_SECONDS);
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
        s.apply(&ev("Stop", "d", None), 1);
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
}
