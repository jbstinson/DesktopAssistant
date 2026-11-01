//! Classifies Claude Code permission requests, vets suggested rules and builds the hook reply.
//! `reply` is the only place a `behavior` value is ever constructed.

use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Command,
    Edit,
    Web,
    Mcp,
    HandBack,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub tool: String,
    pub content: String,
    pub destination: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Deny,
    AllowOnce,
    AllowSession,
    AlwaysAllow,
    Release,
}

pub fn classify(tool: &str) -> Kind {
    match tool {
        "Bash" | "PowerShell" => Kind::Command,
        "Edit" | "Write" | "NotebookEdit" => Kind::Edit,
        "WebFetch" | "WebSearch" => Kind::Web,
        t if t.starts_with("mcp__") => Kind::Mcp,
        _ => Kind::HandBack,
    }
}

/// Accepts exactly one `addRules` suggestion holding exactly one allow rule with real content.
pub fn vet_suggestion(suggestions: &Value) -> Option<Rule> {
    let list = suggestions.as_array()?;
    let [entry] = list.as_slice() else { return None };
    if entry.get("type")?.as_str()? != "addRules" || entry.get("behavior")?.as_str()? != "allow" {
        return None;
    }
    let rules = entry.get("rules")?.as_array()?;
    let [rule] = rules.as_slice() else { return None };
    let tool = rule.get("toolName")?.as_str()?;
    let content = rule.get("ruleContent")?.as_str()?;
    if tool.trim().is_empty() || !content.chars().any(|c| !(c == '*' || c == ':' || c.is_whitespace())) {
        return None;
    }
    let destination = entry.get("destination").and_then(Value::as_str).unwrap_or("");
    Some(Rule { tool: tool.to_string(), content: content.to_string(), destination: destination.to_string() })
}

pub fn is_broad(rule: &Rule) -> bool {
    rule.content.trim_end().ends_with('*')
}

/// The only builder of a `behavior` value. Allow is constructed here and nowhere else.
pub fn reply(choice: Choice, rule: Option<&Rule>) -> Option<Value> {
    let wrap = |decision: Value| json!({"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":decision}});
    let with_rule = |rule: &Rule, destination: &str| {
        wrap(json!({"behavior":"allow","updatedPermissions":[
            {"type":"addRules","rules":[{"toolName":rule.tool,"ruleContent":rule.content}],"behavior":"allow","destination":destination}]}))
    };
    match choice {
        Choice::Release => None,
        Choice::Deny => Some(wrap(json!({"behavior":"deny","message":"Denied in DesktopAssistant"}))),
        Choice::AllowOnce => Some(wrap(json!({"behavior":"allow"}))),
        Choice::AllowSession => rule.map(|r| with_rule(r, "session")),
        Choice::AlwaysAllow => rule.map(|r| with_rule(r, "localSettings")),
    }
}

pub fn describe_rule(rule: &Rule) -> String {
    let base = format!("{}: {}", rule.tool, rule.content);
    if !is_broad(rule) {
        return base;
    }
    let prefix = rule.content.trim_end().trim_end_matches('*').trim_end().trim_end_matches(':').trim();
    let what = if classify(&rule.tool) == Kind::Command { "any command" } else { "anything" };
    format!("{base} (allows {what} starting with {prefix})")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rule(content: &str) -> Rule {
        Rule { tool: "Bash".into(), content: content.into(), destination: "localSettings".into() }
    }

    fn sugg(content: Value) -> Value {
        json!([{"type":"addRules","rules":[{"toolName":"Bash","ruleContent":content}],"behavior":"allow","destination":"localSettings"}])
    }

    #[test]
    fn classifies_tools() {
        for t in ["Bash", "PowerShell"] { assert_eq!(classify(t), Kind::Command, "{t}"); }
        for t in ["Edit", "Write", "NotebookEdit"] { assert_eq!(classify(t), Kind::Edit, "{t}"); }
        for t in ["WebFetch", "WebSearch"] { assert_eq!(classify(t), Kind::Web, "{t}"); }
        assert_eq!(classify("mcp__x__y"), Kind::Mcp);
        for t in ["AskUserQuestion", "ExitPlanMode", "Mystery", ""] { assert_eq!(classify(t), Kind::HandBack, "{t}"); }
    }

    #[test]
    fn vet_accepts_one_allow_rule() {
        let r = vet_suggestion(&sugg(json!("node -e \"x\""))).unwrap();
        assert_eq!(r, Rule { tool: "Bash".into(), content: "node -e \"x\"".into(), destination: "localSettings".into() });
        assert!(vet_suggestion(&sugg(json!("node *"))).is_some());
    }

    #[test]
    fn vet_rejects_bad_shapes() {
        assert!(vet_suggestion(&json!(null)).is_none());
        assert!(vet_suggestion(&json!({})).is_none());
        assert!(vet_suggestion(&json!("x")).is_none());
        assert!(vet_suggestion(&json!([])).is_none());
        let one = sugg(json!("ls"))[0].clone();
        assert!(vet_suggestion(&json!([one.clone(), one.clone()])).is_none());
        let mut wrong = one.clone(); wrong["type"] = json!("setMode");
        assert!(vet_suggestion(&json!([wrong])).is_none());
        let mut deny = one.clone(); deny["behavior"] = json!("deny");
        assert!(vet_suggestion(&json!([deny])).is_none());
        let mut two = one.clone(); two["rules"] = json!([{"toolName":"Bash","ruleContent":"a"},{"toolName":"Bash","ruleContent":"b"}]);
        assert!(vet_suggestion(&json!([two])).is_none());
        let mut norules = one.clone(); norules["rules"] = json!([]);
        assert!(vet_suggestion(&json!([norules])).is_none());
        let mut nocontent = one.clone(); nocontent["rules"] = json!([{"toolName":"Bash"}]);
        assert!(vet_suggestion(&json!([nocontent])).is_none());
        let mut notool = one.clone(); notool["rules"] = json!([{"ruleContent":"ls"}]);
        assert!(vet_suggestion(&json!([notool])).is_none());
    }

    #[test]
    fn vet_rejects_empty_and_wildcard_only_content() {
        for c in ["", " ", "*", "**", ":*", " * ", "* *", "*:*", "\t:*\n"] {
            assert!(vet_suggestion(&sugg(json!(c))).is_none(), "{c:?}");
        }
        assert!(vet_suggestion(&sugg(json!(5))).is_none());
    }

    #[test]
    fn broad_when_ending_in_star() {
        assert!(is_broad(&rule("node *")));
        assert!(is_broad(&rule("node:*")));
        assert!(is_broad(&rule("node * ")));
        assert!(!is_broad(&rule("node -v")));
    }

    #[test]
    fn reply_allow_once_and_deny_match_tested_shape() {
        assert_eq!(
            reply(Choice::AllowOnce, None).unwrap(),
            json!({"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}})
        );
        assert_eq!(
            reply(Choice::Deny, None).unwrap(),
            json!({"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"deny","message":"Denied in DesktopAssistant"}}})
        );
    }

    #[test]
    fn reply_session_and_always_force_destination() {
        let r = rule("node *"); // suggestion said localSettings
        let s = reply(Choice::AllowSession, Some(&r)).unwrap();
        assert_eq!(
            s,
            json!({"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow","updatedPermissions":[
                {"type":"addRules","rules":[{"toolName":"Bash","ruleContent":"node *"}],"behavior":"allow","destination":"session"}]}}})
        );
        let mut r2 = rule("node *"); r2.destination = "session".into();
        let a = reply(Choice::AlwaysAllow, Some(&r2)).unwrap();
        assert_eq!(a["hookSpecificOutput"]["decision"]["updatedPermissions"][0]["destination"], "localSettings");
        assert_eq!(a["hookSpecificOutput"]["decision"]["behavior"], "allow");
    }

    #[test]
    fn reply_none_cases() {
        assert!(reply(Choice::AllowSession, None).is_none());
        assert!(reply(Choice::AlwaysAllow, None).is_none());
        assert!(reply(Choice::Release, None).is_none());
        assert!(reply(Choice::Release, Some(&rule("ls"))).is_none());
    }

    #[test]
    fn describe_rule_text() {
        assert_eq!(describe_rule(&rule("node -v")), "Bash: node -v");
        assert_eq!(describe_rule(&rule("node *")), "Bash: node * (allows any command starting with node)");
        assert_eq!(describe_rule(&rule("node:*")), "Bash: node:* (allows any command starting with node)");
        let mut m = rule("foo*"); m.tool = "WebFetch".into();
        assert_eq!(describe_rule(&m), "WebFetch: foo* (allows anything starting with foo)");
    }

    #[test]
    fn reply_is_the_only_behavior_builder() {
        let files: [(&str, &str); 17] = [
            ("app.rs", include_str!("app.rs")),
            ("delivery.rs", include_str!("delivery.rs")),
            ("events.rs", include_str!("events.rs")),
            ("installer.rs", include_str!("installer.rs")),
            ("jump.rs", include_str!("jump.rs")),
            ("listener.rs", include_str!("listener.rs")),
            ("main.rs", include_str!("main.rs")),
            ("overlay.rs", include_str!("overlay.rs")),
            ("paths.rs", include_str!("paths.rs")),
            ("position.rs", include_str!("position.rs")),
            ("rules.rs", include_str!("rules.rs")),
            ("rules_watch.rs", include_str!("rules_watch.rs")),
            ("sessions.rs", include_str!("sessions.rs")),
            ("single.rs", include_str!("single.rs")),
            ("tray.rs", include_str!("tray.rs")),
            ("voice.rs", include_str!("voice.rs")),
            ("default_rules.toml", include_str!("default_rules.toml")),
        ];
        for (name, text) in files {
            assert!(!text.contains("\"behavior\""), "{name} builds or mentions a behavior value");
        }
        // Guard: the list above must cover every source file.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let on_disk = std::fs::read_dir(dir).unwrap().filter_map(|e| e.ok())
            .filter(|e| e.file_name() != "permission.rs").count();
        assert_eq!(on_disk, files.len(), "new src file: add it to this list");
    }
}
