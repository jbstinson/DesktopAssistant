//! The app's core: sessions plus rules, with no Tauri types. `main.rs` wraps it in a
//! mutex and feeds it events, rule reloads and timer ticks.

use crate::events::HookEvent;
use crate::jump;
use crate::rules::{Alert, RuleSet, Tier};
use crate::sessions::{Activity, Sessions};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionView {
    pub id: String,
    pub project: String,
    pub source: String,
    /// Same names as the character states, e.g. "needs-you".
    pub state: String,
    pub since: u64,
    pub terminal_pid: Option<u32>,
    /// Whether clicking the row can bring a window forward.
    pub jumpable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Snapshot {
    pub state: String,
    /// Sessions currently waiting on the user (needs-you).
    pub waiting: usize,
    pub sessions: Vec<SessionView>,
    pub warning: Option<String>,
    pub quiet: bool,
    pub hooks_on: bool,
}

pub struct Core {
    sessions: Sessions,
    rules: RuleSet,
    rules_warning: Option<String>,
    connection_warning: Option<String>,
    hotkey_warning: Option<String>,
    quiet: bool,
    hooks_on: bool,
    hooks_stale: bool,
}

/// Shown when our hook entries exist but point at a different da-hook.exe than this app's.
pub const HOOKS_STALE_WARNING: &str =
    "The Claude Code hook path is out of date: use the tray menu, Connect to Claude Code, to repair it";

fn state_name(a: Activity) -> &'static str {
    match a {
        Activity::Thinking => "thinking",
        Activity::Reading => "reading",
        Activity::Typing => "typing",
        Activity::NeedsYou => "needs-you",
        Activity::Done => "done",
        Activity::Error => "error",
        Activity::Idle => "asleep",
    }
}

impl Core {
    pub fn new(rules: RuleSet, rules_warning: Option<String>) -> Core {
        Core {
            sessions: Sessions::new(),
            rules,
            rules_warning,
            connection_warning: None,
            hotkey_warning: None,
            quiet: false,
            hooks_on: false,
            hooks_stale: false,
        }
    }

    /// Apply an event. Returns the alert to deliver, or None when the rules say silent.
    pub fn handle(&mut self, e: HookEvent, now: u64) -> Option<Alert> {
        // SessionEnd removes the session, so its project is read before the event is applied.
        let before = self.sessions.project_of(&e.session_id);
        let applied = self.sessions.apply(&e, now);
        let project = self.sessions.project_of(&e.session_id).or(before).unwrap_or_default();
        let alert = self.rules.evaluate(&e, &applied, &project);
        (alert.tier != Tier::Silent).then_some(alert)
    }

    /// `Ok` replaces the rules and clears the rules warning; `Err` keeps the last good
    /// rules and sets the warning.
    pub fn replace_rules(&mut self, result: Result<RuleSet, String>) {
        match result {
            Ok(rules) => {
                self.rules = rules;
                self.rules_warning = None;
            }
            Err(msg) => self.rules_warning = Some(msg),
        }
    }

    pub fn set_connection_warning(&mut self, warning: Option<String>) {
        self.connection_warning = warning;
    }

    pub fn set_hotkey_warning(&mut self, warning: Option<String>) {
        self.hotkey_warning = warning;
    }

    /// Do-not-disturb. Only the mirrored flag lives here; the sound queue is muted separately.
    pub fn set_quiet(&mut self, quiet: bool) {
        self.quiet = quiet;
    }

    /// Whether our hook entries are in Claude Code's settings.
    pub fn set_hooks_on(&mut self, on: bool) {
        self.hooks_on = on;
    }

    pub fn set_hooks_status(&mut self, status: crate::tray::HookStatus) {
        self.hooks_on = status.on;
        self.hooks_stale = status.stale;
    }

    pub fn hooks_status(&self) -> crate::tray::HookStatus {
        crate::tray::HookStatus { on: self.hooks_on, stale: self.hooks_stale }
    }

    /// Remove one session by hand, whatever its state.
    pub fn dismiss(&mut self, id: &str) {
        self.sessions.dismiss(id);
    }

    /// Expire stale sessions: 30 minutes of silence, or 24 hours for ones waiting on the
    /// user (needs-you, error). Returns whether anything changed.
    pub fn tick(&mut self, now: u64) -> bool {
        let before = self.sessions.list().len();
        self.sessions.expire(now);
        self.sessions.list().len() != before
    }

    /// Where the session `id` lives, or None if it is gone.
    pub fn jump_target(&self, id: &str) -> Option<jump::Target> {
        self.sessions
            .list()
            .into_iter()
            .find(|v| v.id == id)
            .map(|v| jump::target_for(&v.source, v.terminal_pid))
    }

    pub fn snapshot(&self) -> Snapshot {
        let sessions: Vec<SessionView> = self
            .sessions
            .list()
            .into_iter()
            .map(|v| SessionView {
                jumpable: jump::target_for(&v.source, v.terminal_pid) != jump::Target::None,
                id: v.id,
                project: v.project,
                source: v.source,
                state: state_name(v.activity).to_string(),
                since: v.since,
                terminal_pid: v.terminal_pid,
            })
            .collect();
        let stale = self.hooks_stale.then(|| HOOKS_STALE_WARNING.to_string());
        let warnings: Vec<&str> =
            [&self.rules_warning, &self.connection_warning, &self.hotkey_warning, &stale]
                .into_iter()
                .flatten()
                .map(String::as_str)
                .collect();
        Snapshot {
            state: self.sessions.character_state().to_string(),
            waiting: sessions.iter().filter(|s| s.state == "needs-you").count(),
            sessions,
            warning: (!warnings.is_empty()).then(|| warnings.join("; ")),
            quiet: self.quiet,
            hooks_on: self.hooks_on,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::DEFAULT_RULES;

    fn defaults() -> RuleSet {
        RuleSet::parse(DEFAULT_RULES).unwrap()
    }

    fn ev(name: &str, sid: &str, tool: Option<&str>) -> HookEvent {
        HookEvent {
            name: name.into(),
            session_id: sid.into(),
            cwd: r"C:\git\proj".into(),
            tool: tool.map(String::from),
            notification_type: None,
            source: "terminal".into(),
            terminal_pid: Some(7),
            agent_id: None,
        }
    }

    #[test]
    fn starts_asleep() {
        let core = Core::new(defaults(), None);
        let s = core.snapshot();
        assert_eq!(s.state, "asleep");
        assert_eq!(s.waiting, 0);
        assert!(s.sessions.is_empty());
        assert_eq!(s.warning, None);
        assert!(!s.quiet);
        assert!(!s.hooks_on);
    }

    #[test]
    fn jump_target_follows_source_and_pid() {
        let mut core = Core::new(defaults(), None);
        let mut cli = ev("PreToolUse", "a", Some("Read"));
        cli.source = "cli".into();
        let mut sdk = ev("PreToolUse", "b", Some("Read"));
        sdk.source = "sdk-cli".into();
        core.handle(cli, 10);
        core.handle(sdk, 10);
        assert_eq!(core.jump_target("a"), Some(jump::Target::Terminal(7)));
        assert_eq!(core.jump_target("b"), Some(jump::Target::None));
        assert_eq!(core.jump_target("gone"), None);
        let s = core.snapshot();
        let jumpable = |id: &str| s.sessions.iter().find(|v| v.id == id).unwrap().jumpable;
        assert!(jumpable("a"));
        assert!(!jumpable("b"));
    }

    #[test]
    fn event_updates_snapshot() {
        let mut core = Core::new(defaults(), None);
        let alert = core.handle(ev("PreToolUse", "a", Some("Read")), 10);
        assert_eq!(alert, None);
        let s = core.snapshot();
        assert_eq!(s.state, "reading");
        assert_eq!(s.sessions.len(), 1);
        let v = &s.sessions[0];
        assert_eq!((v.id.as_str(), v.project.as_str()), ("a", "proj"));
        assert_eq!((v.source.as_str(), v.state.as_str()), ("terminal", "reading"));
        assert_eq!((v.since, v.terminal_pid), (10, Some(7)));
    }

    #[test]
    fn needs_you_returns_alert_and_counts_waiting() {
        let mut core = Core::new(defaults(), None);
        let alert = core.handle(ev("PermissionRequest", "a", Some("Bash")), 1).unwrap();
        assert_eq!(alert.tier, Tier::NeedsYou);
        assert_eq!(alert.speak.as_deref(), Some("proj needs you"));
        assert_eq!(core.snapshot().state, "needs-you");
        assert_eq!(core.snapshot().waiting, 1);
        core.handle(ev("PermissionRequest", "b", None), 2);
        assert_eq!(core.snapshot().waiting, 2);
        // A tool resuming clears one session's wait.
        core.handle(ev("PreToolUse", "a", Some("Bash")), 3);
        assert_eq!(core.snapshot().waiting, 1);
    }

    #[test]
    fn long_turn_stop_returns_bell_alert() {
        let mut core = Core::new(defaults(), None);
        core.handle(ev("UserPromptSubmit", "a", None), 0);
        let alert = core.handle(ev("Stop", "a", None), 90).unwrap();
        assert_eq!(alert.tier, Tier::GoodToKnow);
        assert_eq!(alert.sound.as_deref(), Some("bell"));
        assert_eq!(core.snapshot().state, "done");
    }

    #[test]
    fn rules_replacement_applies_to_next_event() {
        let mut core = Core::new(defaults(), None);
        assert!(core.handle(ev("PermissionRequest", "a", None), 1).is_some());
        let quiet = RuleSet::parse(
            "[[rule]]\nwhen = { event = \"PermissionRequest\" }\nthen = { tier = \"silent\" }\n",
        )
        .unwrap();
        core.replace_rules(Ok(quiet));
        assert_eq!(core.handle(ev("PermissionRequest", "b", None), 2), None);
        // The state still changes; only the alert is silenced.
        assert_eq!(core.snapshot().waiting, 2);
    }

    #[test]
    fn rejected_rules_keep_old_rules_and_set_warning() {
        let mut core = Core::new(defaults(), None);
        core.replace_rules(Err("line 3: bad".into()));
        assert_eq!(core.snapshot().warning.as_deref(), Some("line 3: bad"));
        assert!(core.handle(ev("PermissionRequest", "a", None), 1).is_some());
        core.replace_rules(Ok(defaults()));
        assert_eq!(core.snapshot().warning, None);
    }

    #[test]
    fn startup_rules_warning_is_shown() {
        let core = Core::new(defaults(), Some("using default rules".into()));
        assert_eq!(core.snapshot().warning.as_deref(), Some("using default rules"));
    }

    #[test]
    fn warnings_are_joined() {
        let mut core = Core::new(defaults(), Some("rules bad".into()));
        core.set_connection_warning(Some("hooks cannot connect".into()));
        assert_eq!(
            core.snapshot().warning.as_deref(),
            Some("rules bad; hooks cannot connect")
        );
        core.replace_rules(Ok(defaults()));
        assert_eq!(core.snapshot().warning.as_deref(), Some("hooks cannot connect"));
        core.set_connection_warning(None);
        assert_eq!(core.snapshot().warning, None);
    }

    #[test]
    fn tick_expires_silent_sessions_and_reports_change() {
        let mut core = Core::new(defaults(), None);
        core.handle(ev("PreToolUse", "a", Some("Read")), 100);
        assert!(!core.tick(100 + 1799));
        assert_eq!(core.snapshot().state, "reading");
        assert!(core.tick(100 + 1800));
        assert_eq!(core.snapshot().state, "asleep");
        assert!(!core.tick(100 + 1801));
    }

    #[test]
    fn tick_keeps_waiting_sessions_past_thirty_minutes() {
        let mut core = Core::new(defaults(), None);
        core.handle(ev("PermissionRequest", "a", None), 100);
        assert!(!core.tick(100 + 1800));
        assert_eq!(core.snapshot().waiting, 1);
        assert!(!core.tick(100 + 86399));
        assert!(core.tick(100 + 86400));
        assert_eq!(core.snapshot().waiting, 0);
    }

    #[test]
    fn session_end_rules_see_the_project_name() {
        let rules = RuleSet::parse(
            "[[rule]]
when = { event = \"SessionEnd\", project = \"proj\" }
             then = { tier = \"good_to_know\", sound = \"bell\", speak = \"{project} closed\" }",
        )
        .unwrap();
        let mut core = Core::new(rules, None);
        core.handle(ev("PreToolUse", "a", Some("Read")), 1);
        let alert = core.handle(ev("SessionEnd", "a", None), 2).unwrap();
        assert_eq!(alert.speak.as_deref(), Some("proj closed"));
    }

    #[test]
    fn dismiss_removes_any_session() {
        let mut core = Core::new(defaults(), None);
        core.handle(ev("PermissionRequest", "a", None), 1);
        core.dismiss("a");
        assert_eq!(core.snapshot().state, "asleep");
        core.dismiss("missing");
    }

    #[test]
    fn session_end_removes_session() {
        let mut core = Core::new(defaults(), None);
        core.handle(ev("PreToolUse", "a", Some("Read")), 1);
        core.handle(ev("SessionEnd", "a", None), 2);
        assert_eq!(core.snapshot().state, "asleep");
    }

    #[test]
    fn quiet_and_hooks_on_show_in_snapshot() {
        let mut core = Core::new(defaults(), None);
        core.set_quiet(true);
        core.set_hooks_on(true);
        let s = core.snapshot();
        assert!(s.quiet && s.hooks_on);
        core.set_quiet(false);
        core.set_hooks_on(false);
        let s = core.snapshot();
        assert!(!s.quiet && !s.hooks_on);
    }

    #[test]
    fn quiet_still_shows_needs_you() {
        let mut core = Core::new(defaults(), None);
        core.set_quiet(true);
        core.handle(ev("PermissionRequest", "a", None), 1);
        let s = core.snapshot();
        assert_eq!((s.state.as_str(), s.waiting), ("needs-you", 1));
        assert!(s.quiet);
    }

    #[test]
    fn stale_hook_path_shows_a_warning_until_repaired() {
        use crate::tray::HookStatus;
        let mut core = Core::new(defaults(), None);
        core.set_hooks_status(HookStatus { on: false, stale: true });
        let s = core.snapshot();
        assert!(!s.hooks_on);
        let w = s.warning.unwrap();
        assert!(w.contains("out of date") && w.contains("tray"), "{w}");
        core.set_hooks_status(HookStatus { on: true, stale: false });
        let s = core.snapshot();
        assert!(s.hooks_on);
        assert_eq!(s.warning, None);
    }

    #[test]
    fn hotkey_warning_is_joined_with_the_others() {
        let mut core = Core::new(defaults(), Some("rules bad".into()));
        core.set_hotkey_warning(Some("no hotkey".into()));
        assert_eq!(core.snapshot().warning.as_deref(), Some("rules bad; no hotkey"));
        core.set_hotkey_warning(None);
        assert_eq!(core.snapshot().warning.as_deref(), Some("rules bad"));
    }
}
