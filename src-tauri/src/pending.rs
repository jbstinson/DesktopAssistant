//! Holds Claude Code permission requests until they are answered, released or expired.
//! Pure: every method takes `now_ms` from the caller and nothing here reads a clock.

use crate::permission::{Choice, Kind, Rule};
use serde_json::Value;

/// Buttons stay locked this long after a request becomes current.
const LOCK_MS: u64 = 1000;
/// A request nobody polls for this long is dropped (the hook has gone away).
const LOST_POLL_MS: u64 = 3000;

/// Holds the tool's input, so this type deliberately has no Debug.
#[derive(Clone)]
pub struct Request {
    pub id: String,
    pub session_id: String,
    pub project: String,
    pub tool: String,
    pub kind: Kind,
    pub input: Value,
    pub description: Option<String>,
    pub rule: Option<Rule>,
}

/// What the panel needs to know about the current request's clocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    /// Milliseconds of click lock left.
    pub locked_ms: u64,
    /// Whole seconds of hold left, rounded up.
    pub remaining_secs: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Poll {
    Waiting,
    Decided(Choice),
    Gone,
}

struct Entry {
    req: Request,
    opened_ms: u64,
    hold_ms: u64,
    last_poll_ms: u64,
    /// Set when this request first becomes the current one.
    current_since_ms: Option<u64>,
    decided: Option<Choice>,
}

pub struct Pending {
    hold_ms: u64,
    entries: Vec<Entry>,
}

impl Entry {
    fn lost(&self, now_ms: u64) -> bool {
        now_ms.saturating_sub(self.last_poll_ms) >= LOST_POLL_MS
    }

    /// Only a request still waiting can run out of hold time.
    fn expired(&self, now_ms: u64) -> bool {
        self.decided.is_none() && now_ms.saturating_sub(self.opened_ms) >= self.hold_ms
    }

    fn dead(&self, now_ms: u64) -> bool {
        self.lost(now_ms) || self.expired(now_ms)
    }
}

impl Pending {
    pub fn new(hold_secs: u64) -> Self {
        Pending { hold_ms: hold_secs.saturating_mul(1000), entries: Vec::new() }
    }

    /// Applies to requests opened afterwards.
    pub fn set_hold(&mut self, secs: u64) {
        self.hold_ms = secs.saturating_mul(1000);
    }

    /// Stores the request. A duplicate id is rejected (false) and the original kept.
    pub fn open(&mut self, r: Request, now_ms: u64) -> bool {
        if self.entries.iter().any(|e| e.req.id == r.id) {
            return false;
        }
        self.entries.push(Entry {
            req: r,
            opened_ms: now_ms,
            hold_ms: self.hold_ms,
            last_poll_ms: now_ms,
            current_since_ms: None,
            decided: None,
        });
        self.settle(now_ms);
        true
    }

    pub fn poll(&mut self, id: &str, now_ms: u64) -> Poll {
        let Some(i) = self.entries.iter().position(|e| e.req.id == id) else { return Poll::Gone };
        let e = &self.entries[i];
        if e.lost(now_ms) || e.expired(now_ms) {
            self.entries.remove(i);
            self.settle(now_ms);
            return Poll::Gone;
        }
        if let Some(c) = e.decided {
            self.entries.remove(i);
            return Poll::Decided(c);
        }
        self.entries[i].last_poll_ms = now_ms;
        Poll::Waiting
    }

    /// Only the current request can be answered, and only after the click lock (not for Release).
    pub fn answer(&mut self, id: &str, c: Choice, now_ms: u64) -> Result<(), String> {
        let Some(i) = self.entries.iter().position(|e| e.decided.is_none()) else {
            return Err("no request is waiting".into());
        };
        let e = &self.entries[i];
        if e.req.id != id {
            return Err("not the current request".into());
        }
        if e.dead(now_ms) {
            return Err("request has expired".into());
        }
        if matches!(c, Choice::AllowSession | Choice::AlwaysAllow) && e.req.rule.is_none() {
            return Err("no rule to save for this request".into());
        }
        if c != Choice::Release {
            let since = e.current_since_ms.unwrap_or(now_ms);
            if now_ms.saturating_sub(since) < LOCK_MS {
                return Err("buttons are still locked".into());
            }
        }
        self.entries[i].decided = Some(c);
        self.settle(now_ms);
        Ok(())
    }

    /// Every request still waiting becomes decided as Release.
    pub fn release_all(&mut self) {
        for e in self.entries.iter_mut().filter(|e| e.decided.is_none()) {
            e.decided = Some(Choice::Release);
        }
    }

    /// Drops lost and expired requests. True when anything was removed or the current request changed.
    pub fn tick(&mut self, now_ms: u64) -> bool {
        let before = self.current_id();
        let n = self.entries.len();
        self.entries.retain(|e| !e.dead(now_ms));
        self.settle(now_ms);
        self.entries.len() != n || self.current_id() != before
    }

    pub fn current(&self) -> Option<&Request> {
        self.entries.iter().find(|e| e.decided.is_none()).map(|e| &e.req)
    }

    pub fn current_timing(&self, now_ms: u64) -> Option<Timing> {
        let e = self.entries.iter().find(|e| e.decided.is_none())?;
        let since = e.current_since_ms.unwrap_or(now_ms);
        let locked_ms = LOCK_MS.saturating_sub(now_ms.saturating_sub(since));
        let left_ms = e.hold_ms.saturating_sub(now_ms.saturating_sub(e.opened_ms));
        Some(Timing { locked_ms, remaining_secs: left_ms.div_ceil(1000) })
    }

    /// Requests still waiting for an answer.
    pub fn waiting_count(&self) -> usize {
        self.entries.iter().filter(|e| e.decided.is_none()).count()
    }

    /// The stored rule of a request, decided or not.
    pub fn rule_of(&self, id: &str) -> Option<&Rule> {
        self.entries.iter().find(|e| e.req.id == id).and_then(|e| e.req.rule.as_ref())
    }

    pub fn count(&self) -> usize {
        self.entries.len()
    }

    fn current_id(&self) -> Option<String> {
        self.current().map(|r| r.id.clone())
    }

    /// Starts the lock for whichever request is now current, measured from this moment.
    fn settle(&mut self, now_ms: u64) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.decided.is_none()) {
            e.current_since_ms.get_or_insert(now_ms);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(id: &str) -> Request {
        Request {
            id: id.into(), session_id: "s".into(), project: "p".into(), tool: "Bash".into(),
            kind: Kind::Command, input: Value::Null, description: None, rule: None,
        }
    }

    fn with_rule(id: &str) -> Request {
        let mut r = req(id);
        r.rule = Some(Rule { tool: "Bash".into(), content: "node *".into(), destination: "localSettings".into() });
        r
    }

    fn cur(p: &Pending) -> Option<&str> { p.current().map(|r| r.id.as_str()) }

    #[test]
    fn timing_of_the_current_request() {
        let mut p = Pending::new(120);
        assert!(p.current_timing(0).is_none());
        p.open(req("a"), 5000);
        let t = p.current_timing(5400).unwrap();
        assert_eq!((t.locked_ms, t.remaining_secs), (600, 120));
        let t = p.current_timing(6000).unwrap();
        assert_eq!((t.locked_ms, t.remaining_secs), (0, 119));
        assert_eq!(p.current_timing(5000 + 119_001).unwrap().remaining_secs, 1);
        assert_eq!(p.current_timing(5000 + 125_000).unwrap().remaining_secs, 0);
    }

    #[test]
    fn waiting_count_leaves_out_decided_requests() {
        let mut p = Pending::new(120);
        p.open(req("a"), 0);
        p.open(req("b"), 0);
        assert_eq!(p.waiting_count(), 2);
        p.answer("a", Choice::Deny, 1000).unwrap();
        assert_eq!((p.count(), p.waiting_count()), (2, 1));
    }

    #[test]
    fn fifo_and_new_request_never_replaces_current() {
        let mut p = Pending::new(120);
        assert!(p.current().is_none());
        assert_eq!(p.count(), 0);
        p.open(req("a"), 0);
        p.open(req("b"), 10);
        p.open(req("c"), 20);
        assert_eq!(cur(&p), Some("a"));
        assert_eq!(p.count(), 3);
        for id in ["a", "b", "c"] { p.poll(id, 1900); }
        p.answer("a", Choice::Deny, 2000).unwrap();
        assert_eq!(cur(&p), Some("b"));
        for id in ["b", "c"] { p.poll(id, 3900); }
        p.answer("b", Choice::Deny, 4000).unwrap();
        assert_eq!(cur(&p), Some("c"));
    }

    #[test]
    fn duplicate_id_is_rejected_and_original_kept() {
        let mut p = Pending::new(120);
        assert!(p.open(req("a"), 0));
        let mut dup = req("a");
        dup.tool = "Other".into();
        assert!(!p.open(dup, 500));
        assert_eq!(p.count(), 1);
        assert_eq!(p.current().unwrap().tool, "Bash");
        // still rejected while decided and uncollected
        p.answer("a", Choice::Deny, 1000).unwrap();
        assert!(!p.open(req("a"), 1100));
    }

    #[test]
    fn answer_locked_for_first_second() {
        let mut p = Pending::new(120);
        p.open(req("a"), 5000);
        assert!(p.answer("a", Choice::AllowOnce, 5000).is_err());
        assert!(p.answer("a", Choice::Deny, 5999).is_err());
        assert_eq!(cur(&p), Some("a"));
        assert!(p.answer("a", Choice::AllowOnce, 6000).is_ok());
        assert_eq!(p.poll("a", 6001), Poll::Decided(Choice::AllowOnce));
    }

    #[test]
    fn release_has_no_lock() {
        let mut p = Pending::new(120);
        p.open(req("a"), 5000);
        assert!(p.answer("a", Choice::Release, 5000).is_ok());
        assert_eq!(p.poll("a", 5001), Poll::Decided(Choice::Release));
    }

    #[test]
    fn next_request_lock_starts_when_it_becomes_current() {
        let mut p = Pending::new(120);
        p.open(req("a"), 0);
        p.open(req("b"), 0);
        for t in [2000, 4000] { p.poll("a", t); p.poll("b", t); }
        p.answer("a", Choice::Deny, 5000).unwrap();
        assert_eq!(cur(&p), Some("b"));
        p.poll("b", 5500);
        assert!(p.answer("b", Choice::Deny, 5999).is_err());
        assert!(p.answer("b", Choice::Deny, 6000).is_ok());
    }

    #[test]
    fn next_lock_starts_when_current_is_removed_by_poll_expiry() {
        let mut p = Pending::new(10);
        p.open(req("a"), 0);
        p.open(req("b"), 5000);
        p.poll("b", 7000);
        p.poll("b", 9000);
        assert_eq!(p.poll("a", 10_000), Poll::Gone);
        assert_eq!(cur(&p), Some("b"));
        assert!(p.answer("b", Choice::Deny, 10_999).is_err());
        assert!(p.answer("b", Choice::Deny, 11_000).is_ok());
    }

    #[test]
    fn bad_ids_are_err_and_change_nothing() {
        let mut p = Pending::new(120);
        p.open(req("a"), 0);
        p.open(req("b"), 0);
        p.poll("a", 2000);
        p.poll("b", 2000);
        assert!(p.answer("b", Choice::Deny, 2000).is_err()); // queued, not current
        assert!(p.answer("zzz", Choice::Deny, 2000).is_err());
        assert!(p.answer("", Choice::Release, 2000).is_err());
        assert_eq!(cur(&p), Some("a"));
        assert_eq!(p.count(), 2);
        assert_eq!(p.poll("a", 2100), Poll::Waiting);
        assert_eq!(p.poll("b", 2100), Poll::Waiting);
        p.answer("a", Choice::Deny, 2200).unwrap();
        // old id
        assert!(p.answer("a", Choice::AllowOnce, 2300).is_err());
        assert_eq!(p.poll("a", 2400), Poll::Decided(Choice::Deny)); // original decision intact
    }

    #[test]
    fn decided_reports_once_then_gone() {
        let mut p = Pending::new(120);
        p.open(req("a"), 0);
        p.answer("a", Choice::AllowOnce, 1000).unwrap();
        assert_eq!(p.count(), 1);
        assert!(p.current().is_none());
        assert_eq!(p.poll("a", 1100), Poll::Decided(Choice::AllowOnce));
        assert_eq!(p.count(), 0);
        assert_eq!(p.poll("a", 1200), Poll::Gone);
        assert_eq!(p.poll("never", 1200), Poll::Gone);
    }

    #[test]
    fn answered_request_not_current_so_panel_moves_on() {
        let mut p = Pending::new(120);
        p.open(req("a"), 0);
        p.open(req("b"), 0);
        p.answer("a", Choice::Deny, 1000).unwrap();
        assert_eq!(cur(&p), Some("b"));
        assert!(p.answer("a", Choice::Deny, 1500).is_err());
    }

    #[test]
    fn hold_expiry_is_gone_at_the_limit() {
        let mut p = Pending::new(2);
        p.open(req("a"), 100);
        assert_eq!(p.poll("a", 2099), Poll::Waiting);
        assert_eq!(p.poll("a", 2100), Poll::Gone);
        assert_eq!(p.count(), 0);
        assert!(p.current().is_none());
    }

    #[test]
    fn expired_request_cannot_be_answered() {
        let mut p = Pending::new(2);
        p.open(req("a"), 0);
        p.poll("a", 1900);
        assert!(p.answer("a", Choice::AllowOnce, 2000).is_err());
        assert!(p.answer("a", Choice::Release, 2000).is_err());
    }

    #[test]
    fn queued_request_expires_while_waiting() {
        let mut p = Pending::new(5);
        p.open(req("a"), 0);
        p.open(req("b"), 1000);
        for t in [2000, 4000, 4900] { p.poll("a", t); p.poll("b", t); }
        // a expires at 5000; b is queued behind it and expires at 6000
        assert!(p.tick(5000));
        assert_eq!(p.count(), 1);
        assert_eq!(cur(&p), Some("b"));
        p.poll("b", 5500);
        assert!(p.tick(6000));
        assert_eq!(p.count(), 0);
    }

    #[test]
    fn lost_poll_removes_at_3000_not_2999() {
        let mut p = Pending::new(120);
        p.open(req("a"), 0);
        assert!(!p.tick(2999));
        assert_eq!(p.count(), 1);
        assert!(p.tick(3000));
        assert_eq!(p.count(), 0);
        assert_eq!(p.poll("a", 3001), Poll::Gone);
    }

    #[test]
    fn poll_refreshes_last_seen() {
        let mut p = Pending::new(120);
        p.open(req("a"), 0);
        assert_eq!(p.poll("a", 2500), Poll::Waiting);
        assert!(!p.tick(5499));
        assert!(p.tick(5500));
    }

    #[test]
    fn poll_after_lost_limit_is_gone() {
        let mut p = Pending::new(120);
        p.open(req("a"), 0);
        assert_eq!(p.poll("a", 3000), Poll::Gone);
        assert_eq!(p.count(), 0);
    }

    #[test]
    fn decided_request_lost_if_never_collected() {
        let mut p = Pending::new(120);
        p.open(req("a"), 0);
        p.poll("a", 1000);
        p.answer("a", Choice::Deny, 1000).unwrap();
        assert!(!p.tick(3999));
        assert!(p.tick(4000));
        assert_eq!(p.poll("a", 4001), Poll::Gone);
    }

    #[test]
    fn decided_request_ignores_hold_expiry() {
        let mut p = Pending::new(2);
        p.open(req("a"), 0);
        p.poll("a", 1000);
        p.answer("a", Choice::Deny, 1500).unwrap();
        // hold ran out at 2000, but a decision already exists; only the lost-poll limit applies
        assert!(!p.tick(2500));
        assert_eq!(p.poll("a", 2900), Poll::Decided(Choice::Deny));
    }

    #[test]
    fn tick_reports_removal_even_when_current_unchanged() {
        let mut p = Pending::new(120);
        assert!(!p.tick(0));
        p.open(req("a"), 0);
        p.open(req("b"), 0);
        p.poll("a", 1000);
        p.poll("b", 1000);
        assert!(!p.tick(1500));
        p.answer("a", Choice::Deny, 1500).unwrap();
        p.poll("b", 2500);
        assert!(!p.tick(2600));
        assert!(p.tick(4000)); // decided a was never collected: removed, b still current
        assert_eq!(p.count(), 1);
        assert_eq!(cur(&p), Some("b"));
    }

    #[test]
    fn tick_reports_when_current_moves_on() {
        let mut p = Pending::new(120);
        p.open(req("a"), 0);
        p.open(req("b"), 0);
        p.poll("b", 2500);
        assert!(p.tick(3000)); // a lost, b becomes current
        assert_eq!(cur(&p), Some("b"));
        assert!(p.answer("b", Choice::Deny, 3999).is_err());
        assert!(p.answer("b", Choice::Deny, 4000).is_ok());
    }

    #[test]
    fn release_all_decides_waiting_as_release() {
        let mut p = Pending::new(120);
        p.open(req("a"), 0);
        p.open(req("b"), 0);
        p.open(req("c"), 0);
        p.answer("a", Choice::AllowOnce, 1000).unwrap();
        p.release_all();
        assert!(p.current().is_none());
        assert_eq!(p.count(), 3);
        assert_eq!(p.poll("a", 1100), Poll::Decided(Choice::AllowOnce)); // already decided stays
        assert_eq!(p.poll("b", 1100), Poll::Decided(Choice::Release));
        assert_eq!(p.poll("c", 1100), Poll::Decided(Choice::Release));
    }

    #[test]
    fn set_hold_applies_to_later_requests_only() {
        let mut p = Pending::new(10);
        p.open(req("a"), 0);
        p.set_hold(2);
        p.open(req("b"), 0);
        assert_eq!(p.poll("b", 2000), Poll::Gone);
        assert_eq!(p.poll("a", 2000), Poll::Waiting);
    }

    #[test]
    fn rule_choices_need_a_rule() {
        let mut p = Pending::new(120);
        p.open(req("a"), 0);
        assert!(p.answer("a", Choice::AlwaysAllow, 2000).is_err());
        assert!(p.answer("a", Choice::AllowSession, 2000).is_err());
        assert_eq!(cur(&p), Some("a"));
        assert!(p.answer("a", Choice::AllowOnce, 2000).is_ok());

        p.open(with_rule("b"), 2000);
        assert!(p.answer("b", Choice::AlwaysAllow, 3000).is_ok());
        p.open(with_rule("c"), 3000);
        assert!(p.answer("c", Choice::AllowSession, 4000).is_ok());
    }

    #[test]
    fn rule_of_finds_decided_requests_too() {
        let mut p = Pending::new(120);
        p.open(with_rule("a"), 0);
        p.open(req("b"), 0);
        assert_eq!(p.rule_of("a").unwrap().content, "node *");
        assert!(p.rule_of("b").is_none());
        assert!(p.rule_of("zzz").is_none());
        p.answer("a", Choice::AlwaysAllow, 1000).unwrap();
        assert_eq!(p.rule_of("a").unwrap().content, "node *");
    }

    #[test]
    fn id_mixup_cannot_approve_the_wrong_request() {
        let mut p = Pending::new(120);
        p.open(req("a"), 0);
        p.open(req("b"), 0);
        p.answer("a", Choice::AllowOnce, 1000).unwrap();
        assert_eq!(p.poll("b", 1001), Poll::Waiting);
        assert_eq!(p.poll("a", 1001), Poll::Decided(Choice::AllowOnce));
    }
}
