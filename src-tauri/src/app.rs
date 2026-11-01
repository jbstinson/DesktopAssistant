//! The app's core: sessions plus rules, with no Tauri types. `main.rs` wraps it in a
//! mutex and feeds it events, rule reloads and timer ticks.

use crate::events::{HookEvent, PermissionAsk, Polled};
use crate::jump;
use crate::pending::{Pending, Poll, Request};
use crate::permission::{self, Choice, Kind};
use crate::rules::{Alert, RuleSet, Tier};
use crate::sessions::{Activity, Sessions};
use serde::Serialize;
use serde_json::Value;

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

/// The request the panel is asking about. It holds the tool's input, so this type has no
/// Debug, and it reaches the webview only through the snapshot. Never logged or saved.
#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct RequestView {
    pub id: String,
    pub project: String,
    pub source: String,
    pub tool: String,
    /// "command", "edit", "web" or "mcp".
    pub kind: &'static str,
    /// What the panel displays; the page picks the fields for the kind.
    pub input: Value,
    pub description: Option<String>,
    /// The rule Allow this session and Always allow would save, if there is one.
    pub rule_text: Option<String>,
    pub rule_broad: bool,
    pub locked_ms: u64,
    pub remaining_secs: u64,
}

#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct Snapshot {
    pub state: String,
    /// Sessions currently waiting on the user (needs-you).
    pub waiting: usize,
    pub sessions: Vec<SessionView>,
    pub warning: Option<String>,
    pub quiet: bool,
    pub hooks_on: bool,
    pub request: Option<RequestView>,
    /// Requests still waiting, the current one included.
    pub request_count: usize,
}

/// Leaves the request out, so a debug print can never show a command.
impl std::fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Snapshot")
            .field("state", &self.state)
            .field("waiting", &self.waiting)
            .field("sessions", &self.sessions)
            .field("warning", &self.warning)
            .field("quiet", &self.quiet)
            .field("hooks_on", &self.hooks_on)
            .field("request_count", &self.request_count)
            .finish_non_exhaustive()
    }
}

/// Hold time in seconds.
const HOLD_DEFAULT_SECS: u64 = 120;
const HOLD_MIN_SECS: u64 = 10;
/// Must stay below `PERMISSION_LIMIT` in hook/src/main.rs (128), which must stay below the
/// PermissionRequest timeout in `installer::timeout_for` (130). An installer test checks this.
pub const HOLD_MAX_SECS: u64 = 125;
/// Requests past this many are handed straight back.
const MAX_WAITING: usize = 64;

/// The result of opening a permission request.
pub struct Opened {
    pub id: String,
    /// False hands the request straight back to Claude Code.
    pub hold: bool,
    pub alert: Option<Alert>,
}

pub struct Core {
    sessions: Sessions,
    pending: Pending,
    visible: bool,
    approve_in_window: bool,
    rules: RuleSet,
    rules_warning: Option<String>,
    connection_warning: Option<String>,
    hotkey_warning: Option<String>,
    quiet: bool,
    hooks_on: bool,
    hooks_stale: bool,
}

/// Shown when our hook entries exist but are not current: a different da-hook.exe or an old timeout.
pub const HOOKS_STALE_WARNING: &str = "Hooks need updating. Use Connect in the tray to repair them.";

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

/// 16 random bytes as hex, or None if the system cannot supply them.
fn new_id() -> Option<String> {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).ok()?;
    Some(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

impl Core {
    pub fn new(rules: RuleSet, rules_warning: Option<String>) -> Core {
        Core {
            sessions: Sessions::new(),
            pending: Pending::new(HOLD_DEFAULT_SECS),
            visible: true,
            approve_in_window: true,
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

    /// Applies the session event and alert for a permission request, then holds the request
    /// if it can be answered here. Anything else hands it straight back.
    /// `now` is the session clock in seconds; `now_ms` is the permission clock.
    pub fn open_permission(&mut self, ask: PermissionAsk, now: u64, now_ms: u64) -> Opened {
        let alert = self.handle(ask.to_event(), now);
        self.pending.tick(now_ms);
        let kind = permission::classify(&ask.tool_name);
        let can_hold = kind != Kind::HandBack
            && self.visible
            && self.approve_in_window
            && !self.quiet
            && self.pending.count() < MAX_WAITING;
        let id = new_id();
        let hold = match &id {
            Some(id) if can_hold => {
                let rule = permission::vet_suggestion(&ask.permission_suggestions, &ask.tool_name);
                let project = self.sessions.project_of(&ask.session_id).unwrap_or_default();
                let request = Request {
                    id: id.clone(),
                    session_id: ask.session_id,
                    project,
                    tool: ask.tool_name,
                    kind,
                    input: ask.tool_input,
                    description: ask.description,
                    rule,
                };
                self.pending.open(request, now_ms)
            }
            _ => false,
        };
        Opened { id: id.unwrap_or_default(), hold, alert }
    }

    /// What the hook should do next, and whether anything changed for the snapshot.
    pub fn poll_permission(&mut self, id: &str, now_ms: u64) -> (Polled, bool) {
        let before = self.pending.count();
        let ticked = self.pending.tick(now_ms);
        // Polling a decided request removes it, so its rule is read first.
        let rule = self.pending.rule_of(id).cloned();
        let polled = match self.pending.poll(id, now_ms) {
            Poll::Waiting => Polled::Waiting,
            Poll::Gone => Polled::Gone,
            Poll::Decided(choice) => permission::reply(choice, rule.as_ref())
                .map_or(Polled::Gone, |reply| Polled::Reply(reply.to_string())),
        };
        (polled, ticked || self.pending.count() != before)
    }

    /// The only way to a decision. Reached by a panel click and nothing else.
    pub fn answer_permission(&mut self, id: &str, choice: Choice, now_ms: u64) -> Result<(), String> {
        self.pending.answer(id, choice, now_ms)
    }

    /// Drops lost and expired requests. Returns whether anything changed.
    pub fn tick_permissions(&mut self, now_ms: u64) -> bool {
        self.pending.tick(now_ms)
    }

    /// Hiding hands every held request back.
    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
        if !visible {
            self.pending.release_all();
        }
    }

    /// Turning the switch off hands every held request back.
    // The tray switch calling this arrives in Task 6.
    #[allow(dead_code)]
    pub fn set_approve_in_window(&mut self, on: bool) {
        self.approve_in_window = on;
        if !on {
            self.pending.release_all();
        }
    }

    /// Applies to requests opened afterwards.
    // The saved setting calling this arrives in Task 6.
    #[allow(dead_code)]
    pub fn set_hold(&mut self, secs: u64) {
        self.pending.set_hold(secs.clamp(HOLD_MIN_SECS, HOLD_MAX_SECS));
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

    /// Do-not-disturb. The sound queue is muted separately. Turning it on hands every held
    /// request back.
    pub fn set_quiet(&mut self, quiet: bool) {
        self.quiet = quiet;
        if quiet {
            self.pending.release_all();
        }
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

    fn request_view(&self, sessions: &[SessionView], now_ms: u64) -> Option<RequestView> {
        let r = self.pending.current()?;
        let timing = self.pending.current_timing(now_ms)?;
        let source = sessions.iter().find(|s| s.id == r.session_id).map(|s| s.source.clone());
        Some(RequestView {
            id: r.id.clone(),
            project: r.project.clone(),
            source: source.unwrap_or_default(),
            tool: r.tool.clone(),
            kind: match r.kind {
                Kind::Command => "command",
                Kind::Edit => "edit",
                Kind::Web => "web",
                Kind::Mcp => "mcp",
                Kind::HandBack => "other",
            },
            input: r.input.clone(),
            description: r.description.clone(),
            rule_text: r.rule.as_ref().map(permission::describe_rule),
            rule_broad: r.rule.as_ref().is_some_and(permission::is_broad),
            locked_ms: timing.locked_ms,
            remaining_secs: timing.remaining_secs,
        })
    }

    /// `now_ms` is the permission clock, for the request's lock and countdown.
    pub fn snapshot(&self, now_ms: u64) -> Snapshot {
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
        let request = self.request_view(&sessions, now_ms);
        Snapshot {
            state: self.sessions.character_state().to_string(),
            waiting: sessions.iter().filter(|s| s.state == "needs-you").count(),
            sessions,
            warning: (!warnings.is_empty()).then(|| warnings.join("; ")),
            quiet: self.quiet,
            hooks_on: self.hooks_on,
            request,
            request_count: self.pending.waiting_count(),
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
        let s = core.snapshot(0);
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
        let s = core.snapshot(0);
        let jumpable = |id: &str| s.sessions.iter().find(|v| v.id == id).unwrap().jumpable;
        assert!(jumpable("a"));
        assert!(!jumpable("b"));
    }

    #[test]
    fn event_updates_snapshot() {
        let mut core = Core::new(defaults(), None);
        let alert = core.handle(ev("PreToolUse", "a", Some("Read")), 10);
        assert_eq!(alert, None);
        let s = core.snapshot(0);
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
        assert_eq!(core.snapshot(0).state, "needs-you");
        assert_eq!(core.snapshot(0).waiting, 1);
        core.handle(ev("PermissionRequest", "b", None), 2);
        assert_eq!(core.snapshot(0).waiting, 2);
        // A tool resuming clears one session's wait.
        core.handle(ev("PreToolUse", "a", Some("Bash")), 3);
        assert_eq!(core.snapshot(0).waiting, 1);
    }

    #[test]
    fn long_turn_stop_returns_bell_alert() {
        let mut core = Core::new(defaults(), None);
        core.handle(ev("UserPromptSubmit", "a", None), 0);
        let alert = core.handle(ev("Stop", "a", None), 90).unwrap();
        assert_eq!(alert.tier, Tier::GoodToKnow);
        assert_eq!(alert.sound.as_deref(), Some("bell"));
        assert_eq!(core.snapshot(0).state, "done");
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
        assert_eq!(core.snapshot(0).waiting, 2);
    }

    #[test]
    fn rejected_rules_keep_old_rules_and_set_warning() {
        let mut core = Core::new(defaults(), None);
        core.replace_rules(Err("line 3: bad".into()));
        assert_eq!(core.snapshot(0).warning.as_deref(), Some("line 3: bad"));
        assert!(core.handle(ev("PermissionRequest", "a", None), 1).is_some());
        core.replace_rules(Ok(defaults()));
        assert_eq!(core.snapshot(0).warning, None);
    }

    #[test]
    fn startup_rules_warning_is_shown() {
        let core = Core::new(defaults(), Some("using default rules".into()));
        assert_eq!(core.snapshot(0).warning.as_deref(), Some("using default rules"));
    }

    #[test]
    fn warnings_are_joined() {
        let mut core = Core::new(defaults(), Some("rules bad".into()));
        core.set_connection_warning(Some("hooks cannot connect".into()));
        assert_eq!(
            core.snapshot(0).warning.as_deref(),
            Some("rules bad; hooks cannot connect")
        );
        core.replace_rules(Ok(defaults()));
        assert_eq!(core.snapshot(0).warning.as_deref(), Some("hooks cannot connect"));
        core.set_connection_warning(None);
        assert_eq!(core.snapshot(0).warning, None);
    }

    #[test]
    fn tick_expires_silent_sessions_and_reports_change() {
        let mut core = Core::new(defaults(), None);
        core.handle(ev("PreToolUse", "a", Some("Read")), 100);
        assert!(!core.tick(100 + 1799));
        assert_eq!(core.snapshot(0).state, "reading");
        assert!(core.tick(100 + 1800));
        assert_eq!(core.snapshot(0).state, "asleep");
        assert!(!core.tick(100 + 1801));
    }

    #[test]
    fn tick_keeps_waiting_sessions_past_thirty_minutes() {
        let mut core = Core::new(defaults(), None);
        core.handle(ev("PermissionRequest", "a", None), 100);
        assert!(!core.tick(100 + 1800));
        assert_eq!(core.snapshot(0).waiting, 1);
        assert!(!core.tick(100 + 86399));
        assert!(core.tick(100 + 86400));
        assert_eq!(core.snapshot(0).waiting, 0);
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
        assert_eq!(core.snapshot(0).state, "asleep");
        core.dismiss("missing");
    }

    #[test]
    fn session_end_removes_session() {
        let mut core = Core::new(defaults(), None);
        core.handle(ev("PreToolUse", "a", Some("Read")), 1);
        core.handle(ev("SessionEnd", "a", None), 2);
        assert_eq!(core.snapshot(0).state, "asleep");
    }

    #[test]
    fn quiet_and_hooks_on_show_in_snapshot() {
        let mut core = Core::new(defaults(), None);
        core.set_quiet(true);
        core.set_hooks_status(crate::tray::HookStatus { on: true, stale: false });
        let s = core.snapshot(0);
        assert!(s.quiet && s.hooks_on);
        core.set_quiet(false);
        core.set_hooks_status(crate::tray::HookStatus { on: false, stale: false });
        let s = core.snapshot(0);
        assert!(!s.quiet && !s.hooks_on);
    }

    #[test]
    fn quiet_still_shows_needs_you() {
        let mut core = Core::new(defaults(), None);
        core.set_quiet(true);
        core.handle(ev("PermissionRequest", "a", None), 1);
        let s = core.snapshot(0);
        assert_eq!((s.state.as_str(), s.waiting), ("needs-you", 1));
        assert!(s.quiet);
    }

    #[test]
    fn stale_hook_path_shows_a_warning_until_repaired() {
        use crate::tray::HookStatus;
        let mut core = Core::new(defaults(), None);
        core.set_hooks_status(HookStatus { on: false, stale: true });
        let s = core.snapshot(0);
        assert!(!s.hooks_on);
        let w = s.warning.unwrap();
        assert_eq!(w, HOOKS_STALE_WARNING);
        assert!(w.contains("need updating") && w.contains("tray"), "{w}");
        core.set_hooks_status(HookStatus { on: true, stale: false });
        let s = core.snapshot(0);
        assert!(s.hooks_on);
        assert_eq!(s.warning, None);
    }

    use crate::events::{PermissionAsk, Polled};
    use crate::permission::{self, Choice, Rule};

    fn ask(sid: &str, tool: &str) -> PermissionAsk {
        PermissionAsk {
            session_id: sid.into(),
            cwd: r"C:\git\proj".into(),
            source: "cli".into(),
            agent_id: None,
            terminal_pid: Some(7),
            tool_name: tool.into(),
            tool_input: serde_json::json!({"command": "node -v"}),
            permission_suggestions: serde_json::Value::Null,
            description: Some("Show version".into()),
        }
    }

    fn with_rule(sid: &str) -> PermissionAsk {
        let mut a = ask(sid, "Bash");
        // The same shape as a suggestion, from the one place allowed to spell it out.
        let rule = Rule { tool: "Bash".into(), content: "node *".into(), destination: String::new() };
        let reply = permission::reply(Choice::AlwaysAllow, Some(&rule)).unwrap();
        a.permission_suggestions = reply["hookSpecificOutput"]["decision"]["updatedPermissions"].clone();
        a
    }

    /// Polls every 2 seconds from `from` up to and not including `to`, all still waiting.
    fn keep_polling(core: &mut Core, id: &str, from: u64, to: u64) {
        for t in (from..to).step_by(2000) {
            assert_eq!(core.poll_permission(id, t).0, Polled::Waiting, "at {t}");
        }
    }

    #[test]
    fn hold_is_on_by_default_and_ids_are_random_hex() {
        let mut core = Core::new(defaults(), None);
        let a = core.open_permission(ask("a", "Bash"), 0 / 1000, 0);
        let b = core.open_permission(ask("b", "Bash"), 0 / 1000, 0);
        assert!(a.hold && b.hold);
        assert_ne!(a.id, b.id);
        for o in [&a, &b] {
            assert_eq!(o.id.len(), 32);
            assert!(o.id.bytes().all(|c| c.is_ascii_hexdigit()), "{}", o.id);
        }
        assert_eq!(core.poll_permission(&a.id, 100).0, Polled::Waiting);
    }

    #[test]
    fn tools_handled_elsewhere_are_handed_back() {
        let mut core = Core::new(defaults(), None);
        for tool in ["AskUserQuestion", "ExitPlanMode", "Mystery", "", "mcp__"] {
            let o = core.open_permission(ask("a", tool), 0 / 1000, 0);
            assert!(!o.hold, "{tool}");
            assert_eq!(core.poll_permission(&o.id, 1).0, Polled::Gone, "{tool}");
        }
        for tool in ["Bash", "PowerShell", "Edit", "Write", "NotebookEdit", "WebFetch", "WebSearch", "mcp__x__y"] {
            assert!(core.open_permission(ask("a", tool), 0 / 1000, 0).hold, "{tool}");
        }
    }

    #[test]
    fn hidden_quiet_or_switch_off_hand_back_but_still_alert_and_wait() {
        let setups: [fn(&mut Core); 3] =
            [|c| c.set_visible(false), |c| c.set_quiet(true), |c| c.set_approve_in_window(false)];
        for setup in setups {
            let mut core = Core::new(defaults(), None);
            setup(&mut core);
            let o = core.open_permission(ask("a", "Bash"), 0 / 1000, 0);
            assert!(!o.hold);
            assert_eq!(core.poll_permission(&o.id, 1).0, Polled::Gone);
            assert_eq!(o.alert.unwrap().speak.as_deref(), Some("proj needs you"));
            assert_eq!(core.snapshot(0).waiting, 1);
        }
    }

    #[test]
    fn turning_any_of_the_three_off_releases_held_requests() {
        let turn_off: [fn(&mut Core); 3] =
            [|c| c.set_visible(false), |c| c.set_quiet(true), |c| c.set_approve_in_window(false)];
        let turn_on: [fn(&mut Core); 3] =
            [|c| c.set_visible(true), |c| c.set_quiet(false), |c| c.set_approve_in_window(true)];
        for (off, on) in turn_off.into_iter().zip(turn_on) {
            let mut core = Core::new(defaults(), None);
            let a = core.open_permission(ask("a", "Bash"), 0 / 1000, 0);
            let b = core.open_permission(ask("b", "Bash"), 0 / 1000, 0);
            off(&mut core);
            on(&mut core);
            // Coming back on approves nothing and holds nothing old.
            assert_eq!(core.poll_permission(&a.id, 100).0, Polled::Gone);
            assert_eq!(core.poll_permission(&b.id, 100).0, Polled::Gone);
            assert!(core.answer_permission(&a.id, Choice::AllowOnce, 5000).is_err());
            assert!(core.open_permission(ask("c", "Bash"), 200 / 1000, 200).hold);
        }
    }

    #[test]
    fn turning_quiet_off_or_showing_releases_nothing() {
        let mut core = Core::new(defaults(), None);
        let a = core.open_permission(ask("a", "Bash"), 0 / 1000, 0);
        core.set_quiet(false);
        core.set_visible(true);
        core.set_approve_in_window(true);
        assert_eq!(core.poll_permission(&a.id, 100).0, Polled::Waiting);
    }

    #[test]
    fn more_than_64_waiting_are_handed_back() {
        let mut core = Core::new(defaults(), None);
        for i in 0..64 {
            assert!(core.open_permission(ask("a", "Bash"), 0 / 1000, 0).hold, "{i}");
        }
        assert!(!core.open_permission(ask("a", "Bash"), 0 / 1000, 0).hold);
        // Once the old ones are lost there is room again.
        assert!(core.open_permission(ask("a", "Bash"), 3000 / 1000, 3000).hold);
    }

    #[test]
    fn a_request_alerts_once_and_the_later_notification_does_not_double_it() {
        let mut core = Core::new(defaults(), None);
        let first = core.open_permission(ask("a", "Bash"), 5000 / 1000, 5000).alert.unwrap();
        assert_eq!(first.tier, Tier::NeedsYou);
        assert_eq!(first.sound.as_deref(), Some("alert"));
        assert_eq!(core.snapshot(0).sessions[0].terminal_pid, Some(7));
        assert_eq!(core.snapshot(0).sessions[0].since, 5);
        assert_eq!(core.snapshot(0).waiting, 1);

        let mut note = ev("Notification", "a", None);
        note.notification_type = Some("permission_prompt".into());
        let later = core.handle(note, 6).unwrap();
        assert_eq!((later.sound, later.speak), (None, None));
        let again = core.open_permission(ask("a", "Bash"), 7000 / 1000, 7000).alert.unwrap();
        assert_eq!((again.sound, again.speak), (None, None));
        assert_eq!(core.snapshot(0).waiting, 1);
    }

    #[test]
    fn a_request_uses_the_subagent_that_asked() {
        let mut core = Core::new(defaults(), None);
        let mut sub = ask("a", "Bash");
        sub.agent_id = Some("ag".into());
        core.open_permission(sub, 1000 / 1000, 1000);
        // A sibling working on something else must not clear the wait.
        core.handle(ev("PreToolUse", "a", Some("Read")), 2);
        assert_eq!(core.snapshot(0).waiting, 1);
    }

    #[test]
    fn a_decision_comes_back_once_as_the_exact_reply() {
        let mut core = Core::new(defaults(), None);
        let o = core.open_permission(ask("a", "Bash"), 0 / 1000, 0);
        assert!(core.answer_permission(&o.id, Choice::AllowOnce, 999).is_err());
        assert_eq!(core.poll_permission(&o.id, 999).0, Polled::Waiting);
        core.answer_permission(&o.id, Choice::AllowOnce, 1000).unwrap();
        let want = permission::reply(Choice::AllowOnce, None).unwrap().to_string();
        assert_eq!(core.poll_permission(&o.id, 1100).0, Polled::Reply(want));
        assert_eq!(core.poll_permission(&o.id, 1200).0, Polled::Gone);
    }

    #[test]
    fn deny_replies_and_release_is_gone() {
        let mut core = Core::new(defaults(), None);
        let o = core.open_permission(ask("a", "Bash"), 0 / 1000, 0);
        core.answer_permission(&o.id, Choice::Deny, 1000).unwrap();
        let want = permission::reply(Choice::Deny, None).unwrap().to_string();
        assert_eq!(core.poll_permission(&o.id, 1100).0, Polled::Reply(want));

        let o = core.open_permission(ask("a", "Bash"), 2000 / 1000, 2000);
        core.answer_permission(&o.id, Choice::Release, 2000).unwrap();
        assert_eq!(core.poll_permission(&o.id, 2100).0, Polled::Gone);
    }

    #[test]
    fn rule_choices_use_the_rule_vetted_at_open() {
        let rule = Rule { tool: "Bash".into(), content: "node *".into(), destination: "localSettings".into() };
        for choice in [Choice::AllowSession, Choice::AlwaysAllow] {
            let mut core = Core::new(defaults(), None);
            let o = core.open_permission(with_rule("a"), 0 / 1000, 0);
            core.answer_permission(&o.id, choice, 1000).unwrap();
            let want = permission::reply(choice, Some(&rule)).unwrap().to_string();
            assert_eq!(core.poll_permission(&o.id, 1100).0, Polled::Reply(want));
        }
    }

    #[test]
    fn a_suggestion_that_fails_vetting_offers_no_rule_choice() {
        let mut core = Core::new(defaults(), None);
        let mut other_tool = with_rule("a");
        other_tool.tool_name = "PowerShell".into();
        let o = core.open_permission(other_tool, 0 / 1000, 0);
        assert!(o.hold);
        assert!(core.answer_permission(&o.id, Choice::AlwaysAllow, 1000).is_err());
        assert!(core.answer_permission(&o.id, Choice::AllowSession, 1000).is_err());
        assert_eq!(core.poll_permission(&o.id, 1100).0, Polled::Waiting);
    }

    #[test]
    fn a_decided_allow_leaves_needs_you_until_the_next_tool_or_stop() {
        for next in ["PreToolUse", "Stop"] {
            let mut core = Core::new(defaults(), None);
            let o = core.open_permission(ask("a", "Bash"), 0 / 1000, 0);
            core.answer_permission(&o.id, Choice::AllowOnce, 1000).unwrap();
            core.poll_permission(&o.id, 1100);
            assert_eq!(core.snapshot(0).waiting, 1);
            core.handle(ev(next, "a", Some("Bash")), 2);
            assert_eq!(core.snapshot(0).waiting, 0, "{next}");
        }
    }

    #[test]
    fn hold_defaults_to_120_seconds() {
        let mut core = Core::new(defaults(), None);
        let o = core.open_permission(ask("a", "Bash"), 0 / 1000, 0);
        keep_polling(&mut core, &o.id, 2000, 120_000);
        assert_eq!(core.poll_permission(&o.id, 120_000).0, Polled::Gone);
    }

    #[test]
    fn set_hold_clamps_to_10_and_125() {
        for (set, limit) in [(1, 10_000), (10, 10_000), (60, 60_000), (125, 125_000), (500, 125_000)] {
            let mut core = Core::new(defaults(), None);
            core.set_hold(set);
            let o = core.open_permission(ask("a", "Bash"), 0 / 1000, 0);
            keep_polling(&mut core, &o.id, 2000, limit);
            assert_eq!(core.poll_permission(&o.id, limit - 1).0, Polled::Waiting, "{set}");
            assert_eq!(core.poll_permission(&o.id, limit).0, Polled::Gone, "{set}");
        }
    }

    #[test]
    fn tick_permissions_reports_only_changes() {
        let mut core = Core::new(defaults(), None);
        assert!(!core.tick_permissions(0));
        core.open_permission(ask("a", "Bash"), 0 / 1000, 0);
        assert!(!core.tick_permissions(2999));
        assert!(core.tick_permissions(3000));
        assert!(!core.tick_permissions(3001));
    }

    #[test]
    fn polling_one_request_clears_out_a_lost_one_and_says_so() {
        let mut core = Core::new(defaults(), None);
        let lost = core.open_permission(ask("a", "Bash"), 0 / 1000, 0);
        let live = core.open_permission(ask("b", "Bash"), 0 / 1000, 0);
        assert!(!core.poll_permission(&live.id, 2000).1);
        let (polled, changed) = core.poll_permission(&live.id, 3000);
        assert_eq!(polled, Polled::Waiting);
        assert!(changed);
        assert_eq!(core.poll_permission(&lost.id, 3100).0, Polled::Gone);
    }

    #[test]
    fn collecting_a_decision_counts_as_a_change() {
        let mut core = Core::new(defaults(), None);
        let o = core.open_permission(ask("a", "Bash"), 0 / 1000, 0);
        core.answer_permission(&o.id, Choice::Deny, 1000).unwrap();
        assert!(core.poll_permission(&o.id, 1100).1);
    }

    #[test]
    fn the_input_never_reaches_a_snapshot() {
        let mut core = Core::new(defaults(), None);
        core.open_permission(with_rule("a"), 0 / 1000, 0);
        let text = format!("{:?}", core.snapshot(0));
        assert!(!text.contains("node") && !text.contains("Show version"), "{text}");
    }

    #[test]
    fn hotkey_warning_is_joined_with_the_others() {
        let mut core = Core::new(defaults(), Some("rules bad".into()));
        core.set_hotkey_warning(Some("no hotkey".into()));
        assert_eq!(core.snapshot(0).warning.as_deref(), Some("rules bad; no hotkey"));
        core.set_hotkey_warning(None);
        assert_eq!(core.snapshot(0).warning.as_deref(), Some("rules bad"));
    }

    #[test]
    fn snapshot_carries_the_current_request_for_the_panel() {
        let mut core = Core::new(defaults(), None);
        assert!(core.snapshot(0).request.is_none());
        assert_eq!(core.snapshot(0).request_count, 0);
        let o = core.open_permission(with_rule("a"), 0, 5_000);
        core.open_permission(ask("b", "Bash"), 0, 5_000);
        let s = core.snapshot(5_400);
        let r = s.request.unwrap();
        assert_eq!(s.request_count, 2);
        assert_eq!((r.id.as_str(), r.project.as_str(), r.source.as_str()), (o.id.as_str(), "proj", "cli"));
        assert_eq!((r.tool.as_str(), r.kind), ("Bash", "command"));
        assert_eq!(r.input["command"], "node -v");
        assert_eq!(r.description.as_deref(), Some("Show version"));
        assert_eq!(r.rule_text.as_deref(), Some("Bash: node * (allows any command starting with node)"));
        assert!(r.rule_broad);
        assert_eq!(r.locked_ms, 600);
        assert_eq!(r.remaining_secs, 120);
        // Seconds are rounded up, so a request never reads 0 while it still has time.
        assert_eq!(core.snapshot(5_000 + 119_500).request.unwrap().remaining_secs, 1);
        assert_eq!(core.snapshot(5_000 + 1_000).request.unwrap().locked_ms, 0);
    }

    #[test]
    fn a_request_without_a_rule_has_no_rule_text() {
        let mut core = Core::new(defaults(), None);
        core.open_permission(ask("a", "Bash"), 0, 0);
        let r = core.snapshot(0).request.unwrap();
        assert_eq!((r.rule_text, r.rule_broad), (None, false));
    }

    #[test]
    fn the_next_request_shows_once_the_current_is_answered() {
        let mut core = Core::new(defaults(), None);
        let a = core.open_permission(ask("a", "Bash"), 0, 0);
        let b = core.open_permission(ask("b", "Edit"), 0, 0);
        core.answer_permission(&a.id, Choice::Deny, 1000).unwrap();
        let s = core.snapshot(1000);
        assert_eq!(s.request.as_ref().unwrap().id, b.id);
        assert_eq!(s.request_count, 1);
        // Its own lock starts when it becomes current.
        assert_eq!(s.request.unwrap().locked_ms, 1000);
    }

    #[test]
    fn answers_for_unknown_or_locked_requests_are_errors() {
        let mut core = Core::new(defaults(), None);
        let o = core.open_permission(ask("a", "Bash"), 0, 0);
        assert!(core.answer_permission("nope", Choice::Deny, 5000).is_err());
        assert!(core.answer_permission(&o.id, Choice::Deny, 999).is_err());
        assert!(core.snapshot(999).request.is_some());
    }

    #[test]
    fn hiding_releases_requests_and_the_view_ends() {
        let mut core = Core::new(defaults(), None);
        core.open_permission(ask("a", "Bash"), 0, 0);
        core.set_visible(false);
        assert!(core.snapshot(10).request.is_none());
        assert_eq!(core.snapshot(10).request_count, 0);
    }

    #[test]
    fn after_the_hold_runs_out_the_view_ends_but_the_session_keeps_waiting() {
        let mut core = Core::new(defaults(), None);
        let o = core.open_permission(ask("a", "Bash"), 0, 0);
        keep_polling(&mut core, &o.id, 0, 120_000);
        assert!(core.tick_permissions(120_000));
        let s = core.snapshot(120_000);
        assert!(s.request.is_none());
        assert_eq!((s.state.as_str(), s.waiting), ("needs-you", 1));
    }
}
