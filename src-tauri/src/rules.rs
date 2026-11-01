//! Alert rules: decide how the user is told about an event. Pure logic plus one
//! small file loader. A rule can only pick a tier, a sound and some speech; there
//! is deliberately no action that could approve or deny anything.

use crate::events::HookEvent;
use crate::sessions::Applied;
use serde::Deserialize;
use std::path::Path;

pub const DEFAULT_RULES: &str = include_str!("default_rules.toml");

/// The only sounds delivery knows how to play.
const SOUNDS: &[&str] = &["alert", "bell"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    NeedsYou,
    GoodToKnow,
    Silent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    pub tier: Tier,
    pub sound: Option<String>,
    pub speak: Option<String>,
}

impl Alert {
    fn silent() -> Self {
        Alert { tier: Tier::Silent, sound: None, speak: None }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl OneOrMany {
    fn matches(&self, value: Option<&str>, ignore_case: bool) -> bool {
        let Some(v) = value else { return false };
        let eq = |c: &str| if ignore_case { c.to_lowercase() == v.to_lowercase() } else { c == v };
        match self {
            OneOrMany::One(c) => eq(c),
            OneOrMany::Many(cs) => cs.iter().any(|c| eq(c)),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct When {
    event: Option<OneOrMany>,
    project: Option<OneOrMany>,
    tool: Option<OneOrMany>,
    source: Option<OneOrMany>,
    notification_type: Option<OneOrMany>,
    min_turn_seconds: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Then {
    tier: Tier,
    sound: Option<String>,
    speak: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Rule {
    #[serde(default)]
    when: When,
    then: Then,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    rule: Vec<Rule>,
}

#[derive(Debug, Clone)]
pub struct RuleSet {
    rules: Vec<Rule>,
}

impl When {
    fn matches(&self, e: &HookEvent, applied: &Applied, project: &str) -> bool {
        let ok = |c: &Option<OneOrMany>, v: Option<&str>, ci: bool| {
            c.as_ref().map_or(true, |c| c.matches(v, ci))
        };
        ok(&self.event, Some(&e.name), false)
            && ok(&self.project, Some(project), true)
            && ok(&self.tool, e.tool.as_deref(), false)
            && ok(&self.source, Some(&e.source), false)
            && ok(&self.notification_type, e.notification_type.as_deref(), false)
            && self
                .min_turn_seconds
                .map_or(true, |min| applied.turn_seconds.is_some_and(|t| t >= min))
    }
}

impl RuleSet {
    pub fn parse(text: &str) -> Result<RuleSet, String> {
        let file: File = toml::from_str(text).map_err(|e| e.to_string())?;
        for r in &file.rule {
            if let Some(s) = &r.then.sound {
                if !SOUNDS.contains(&s.as_str()) {
                    return Err(format!("unknown sound \"{s}\" (use \"alert\" or \"bell\")"));
                }
            }
        }
        Ok(RuleSet { rules: file.rule })
    }

    pub fn evaluate(&self, e: &HookEvent, applied: &Applied, project: &str) -> Alert {
        let Some(rule) = self.rules.iter().find(|r| r.when.matches(e, applied, project)) else {
            return Alert::silent();
        };
        let t = &rule.then;
        if t.tier == Tier::Silent {
            return Alert::silent();
        }
        // A prompt that already alerted must not alert again.
        if t.tier == Tier::NeedsYou && !applied.newly_blocked {
            return Alert { tier: t.tier, sound: None, speak: None };
        }
        Alert {
            tier: t.tier,
            sound: t.sound.clone(),
            speak: t.speak.as_ref().map(|s| s.replace("{project}", project)),
        }
    }
}

/// Load the rules file, creating it with defaults if missing. An invalid file is
/// never overwritten: the defaults are used and a warning says why.
pub fn load_or_create(path: &Path) -> (RuleSet, Option<String>) {
    let defaults = || RuleSet::parse(DEFAULT_RULES).expect("built-in rules are valid");
    match std::fs::read_to_string(path) {
        Ok(text) => match RuleSet::parse(&text) {
            Ok(r) => (r, None),
            Err(e) => (defaults(), Some(format!("{}: {e}; using default rules", path.display()))),
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            let written = path
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|_| std::fs::write(path, DEFAULT_RULES));
            let warn = written
                .err()
                .map(|e| format!("could not write {}: {e}; using default rules", path.display()));
            (defaults(), warn)
        }
        Err(err) => (
            defaults(),
            Some(format!("could not read {}: {err}; using default rules", path.display())),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::HookEvent;
    use crate::sessions::Applied;

    fn ev(name: &str) -> HookEvent {
        HookEvent {
            name: name.into(),
            session_id: "s".into(),
            cwd: "C:\\git\\proj".into(),
            tool: None,
            notification_type: None,
            source: "terminal".into(),
            terminal_pid: None,
        }
    }
    fn ap(turn: Option<u64>, blocked: bool) -> Applied {
        Applied { turn_seconds: turn, newly_blocked: blocked, session_known: true }
    }
    fn defaults() -> RuleSet {
        RuleSet::parse(DEFAULT_RULES).unwrap()
    }
    fn s(x: &str) -> Option<String> {
        Some(x.to_string())
    }

    #[test]
    fn stop_threshold() {
        let r = defaults();
        let a = r.evaluate(&ev("Stop"), &ap(Some(60), false), "proj");
        assert_eq!(a, Alert { tier: Tier::GoodToKnow, sound: s("bell"), speak: s("proj finished") });
        assert_eq!(r.evaluate(&ev("Stop"), &ap(Some(59), false), "proj").tier, Tier::Silent);
        assert_eq!(r.evaluate(&ev("Stop"), &ap(None, false), "proj").tier, Tier::Silent);
        assert_eq!(
            r.evaluate(&ev("PostToolUseFailure"), &ap(None, false), "proj"),
            Alert { tier: Tier::Silent, sound: None, speak: None }
        );
    }

    #[test]
    fn notification_needs_you_with_project() {
        let r = defaults();
        let a = r.evaluate(&ev("Notification"), &ap(None, true), "proj");
        assert_eq!(a, Alert { tier: Tier::NeedsYou, sound: s("alert"), speak: s("proj needs you") });
        let a = r.evaluate(&ev("StopFailure"), &ap(None, true), "proj");
        assert_eq!(a.speak, s("proj failed"));
    }

    #[test]
    fn idle_prompt_is_silent() {
        let mut e = ev("Notification");
        e.notification_type = Some("idle_prompt".into());
        let a = defaults().evaluate(&e, &ap(None, true), "proj");
        assert_eq!(a, Alert { tier: Tier::Silent, sound: None, speak: None });
    }

    #[test]
    fn not_newly_blocked_keeps_tier_without_sound() {
        let a = defaults().evaluate(&ev("PermissionRequest"), &ap(None, false), "proj");
        assert_eq!(a, Alert { tier: Tier::NeedsYou, sound: None, speak: None });
    }

    #[test]
    fn conditions_match() {
        let r = RuleSet::parse(
            r#"
[[rule]]
when = { project = "MyProj", tool = ["Bash", "Edit"], source = "vscode" }
then = { tier = "good_to_know", speak = "hit {project} {other}" }
"#,
        )
        .unwrap();
        let mut e = ev("PreToolUse");
        e.tool = s("Bash");
        e.source = "vscode".into();
        let a = r.evaluate(&e, &ap(None, false), "myproj");
        assert_eq!(a.tier, Tier::GoodToKnow);
        assert_eq!(a.speak, s("hit myproj {other}"));
        assert_eq!(a.sound, None);
        e.tool = s("Edit");
        assert_eq!(r.evaluate(&e, &ap(None, false), "MYPROJ").tier, Tier::GoodToKnow);
        e.tool = s("bash"); // tool is case-sensitive
        assert_eq!(r.evaluate(&e, &ap(None, false), "myproj").tier, Tier::Silent);
        e.tool = s("Bash");
        assert_eq!(r.evaluate(&e, &ap(None, false), "other").tier, Tier::Silent);
        e.source = "terminal".into();
        assert_eq!(r.evaluate(&e, &ap(None, false), "myproj").tier, Tier::Silent);
        e.source = "vscode".into();
        e.tool = None;
        assert_eq!(r.evaluate(&e, &ap(None, false), "myproj").tier, Tier::Silent);
    }

    #[test]
    fn event_string_or_list() {
        let one = RuleSet::parse("[[rule]]\nwhen = { event = \"Stop\" }\nthen = { tier = \"good_to_know\" }").unwrap();
        let many = RuleSet::parse("[[rule]]\nwhen = { event = [\"Stop\", \"SessionEnd\"] }\nthen = { tier = \"good_to_know\" }").unwrap();
        assert_eq!(one.evaluate(&ev("Stop"), &ap(None, false), "p").tier, Tier::GoodToKnow);
        assert_eq!(one.evaluate(&ev("SessionEnd"), &ap(None, false), "p").tier, Tier::Silent);
        assert_eq!(many.evaluate(&ev("SessionEnd"), &ap(None, false), "p").tier, Tier::GoodToKnow);
        assert_eq!(many.evaluate(&ev("stop"), &ap(None, false), "p").tier, Tier::Silent);
    }

    #[test]
    fn first_match_wins() {
        let r = RuleSet::parse(
            "[[rule]]\nwhen = { event = \"Stop\" }\nthen = { tier = \"silent\" }\n\
             [[rule]]\nwhen = { event = \"Stop\" }\nthen = { tier = \"needs_you\" }",
        )
        .unwrap();
        assert_eq!(r.evaluate(&ev("Stop"), &ap(None, true), "p").tier, Tier::Silent);
    }

    #[test]
    fn silent_alert_never_has_sound_or_speech() {
        let r = RuleSet::parse(
            "[[rule]]\nwhen = { event = \"Stop\" }\nthen = { tier = \"silent\", sound = \"bell\", speak = \"x\" }",
        )
        .unwrap();
        assert_eq!(
            r.evaluate(&ev("Stop"), &ap(None, true), "p"),
            Alert { tier: Tier::Silent, sound: None, speak: None }
        );
    }

    #[test]
    fn bad_input_errors_name_the_problem() {
        let e = RuleSet::parse("[[rule]]\nwhen = { event = \"Stop\" }\nthen = { tier = \"loud\" }").unwrap_err();
        assert!(e.contains("loud"), "{e}");
        let e = RuleSet::parse("[[rule]]\nwhen = { evnt = \"Stop\" }\nthen = { tier = \"silent\" }").unwrap_err();
        assert!(e.contains("evnt"), "{e}");
        let e = RuleSet::parse("[[rule]]\nwhen = {}\nthen = { tier = \"silent\", approve = true }").unwrap_err();
        assert!(e.contains("approve"), "{e}");
        let e = RuleSet::parse("[[rule]\nbroken").unwrap_err();
        assert!(!e.is_empty());
        let e = RuleSet::parse("[[rule]]\nwhen = {}\nthen = { tier = \"needs_you\", sound = \"gong\" }").unwrap_err();
        assert!(e.contains("gong"), "{e}");
        assert!(RuleSet::parse("").is_ok());
    }

    #[test]
    fn load_or_create_writes_defaults_when_missing() {
        let dir = std::env::temp_dir().join(format!("da-rules-missing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("nested").join("rules.toml");
        let (r, warn) = load_or_create(&path);
        assert!(warn.is_none());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), DEFAULT_RULES);
        assert_eq!(r.evaluate(&ev("Notification"), &ap(None, true), "p").tier, Tier::NeedsYou);
        // An existing valid file is loaded, not replaced.
        std::fs::write(&path, "[[rule]]\nwhen = { event = \"Stop\" }\nthen = { tier = \"silent\" }").unwrap();
        let (r, warn) = load_or_create(&path);
        assert!(warn.is_none());
        assert_eq!(r.evaluate(&ev("Notification"), &ap(None, true), "p").tier, Tier::Silent);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_or_create_keeps_invalid_file() {
        let dir = std::env::temp_dir().join(format!("da-rules-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rules.toml");
        std::fs::write(&path, "[[rule]]\nwhen = { evnt = 1 }").unwrap();
        let (r, warn) = load_or_create(&path);
        assert!(warn.is_some());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[[rule]]\nwhen = { evnt = 1 }");
        assert_eq!(r.evaluate(&ev("Notification"), &ap(None, true), "p").tier, Tier::NeedsYou);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
