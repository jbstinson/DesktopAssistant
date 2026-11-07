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

#[derive(Clone, PartialEq, Eq)]
pub struct Rule {
    pub tool: String,
    pub content: String,
    pub destination: String,
}

/// The content comes from the request's input, so Debug output leaves it out.
impl std::fmt::Debug for Rule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rule").field("tool", &self.tool).finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Deny,
    AllowOnce,
    AllowSession,
    AlwaysAllow,
    Release,
}

impl Choice {
    /// The panel's names for the five answers. Anything else is not an answer.
    pub fn parse(name: &str) -> Option<Choice> {
        Some(match name {
            "deny" => Choice::Deny,
            "allow_once" => Choice::AllowOnce,
            "allow_session" => Choice::AllowSession,
            "always_allow" => Choice::AlwaysAllow,
            "release" => Choice::Release,
            _ => return None,
        })
    }
}

pub fn classify(tool: &str) -> Kind {
    match tool {
        "Bash" | "PowerShell" => Kind::Command,
        "Edit" | "Write" | "NotebookEdit" => Kind::Edit,
        "WebFetch" | "WebSearch" => Kind::Web,
        t if t.strip_prefix("mcp__").is_some_and(|r| !r.is_empty()) => Kind::Mcp,
        _ => Kind::HandBack,
    }
}

/// Accepts exactly one `addRules` suggestion holding exactly one allow rule with real content.
/// True when the rule has a tool and content with at least one character that is not a wildcard,
/// path or punctuation filler. Shared by vetting and by `reply`.
fn rule_is_acceptable(tool: &str, content: &str) -> bool {
    const FILLER: &[char] = &['*', ':', '/', '\\', '.', '?', '(', ')', '-', '~'];
    !tool.trim().is_empty() && content.chars().any(|c| !(FILLER.contains(&c) || c.is_whitespace()))
}

pub fn vet_suggestion(suggestions: &Value, request_tool: &str) -> Option<Rule> {
    let list = suggestions.as_array()?;
    let [entry] = list.as_slice() else { return None };
    if entry.get("type")?.as_str()? != "addRules" || entry.get("behavior")?.as_str()? != "allow" {
        return None;
    }
    let rules = entry.get("rules")?.as_array()?;
    let [rule] = rules.as_slice() else { return None };
    let tool = rule.get("toolName")?.as_str()?;
    let content = rule.get("ruleContent")?.as_str()?;
    if tool != request_tool || !rule_is_acceptable(tool, content) {
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
        Choice::AllowSession => rule.filter(|r| rule_is_acceptable(&r.tool, &r.content)).map(|r| with_rule(r, "session")),
        Choice::AlwaysAllow => rule.filter(|r| rule_is_acceptable(&r.tool, &r.content)).map(|r| with_rule(r, "localSettings")),
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

    fn vet(s: &Value) -> Option<Rule> { vet_suggestion(s, "Bash") }

    fn sugg(content: Value) -> Value {
        json!([{"type":"addRules","rules":[{"toolName":"Bash","ruleContent":content}],"behavior":"allow","destination":"localSettings"}])
    }

    #[test]
    fn choices_parse_from_exact_names_only() {
        assert_eq!(Choice::parse("deny"), Some(Choice::Deny));
        assert_eq!(Choice::parse("allow_once"), Some(Choice::AllowOnce));
        assert_eq!(Choice::parse("allow_session"), Some(Choice::AllowSession));
        assert_eq!(Choice::parse("always_allow"), Some(Choice::AlwaysAllow));
        assert_eq!(Choice::parse("release"), Some(Choice::Release));
        for bad in ["", "allow", "Deny", "ALLOW_ONCE", " deny", "deny ", "approve", "always", "allow-once"] {
            assert_eq!(Choice::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn classifies_tools() {
        for t in ["Bash", "PowerShell"] { assert_eq!(classify(t), Kind::Command, "{t}"); }
        for t in ["Edit", "Write", "NotebookEdit"] { assert_eq!(classify(t), Kind::Edit, "{t}"); }
        for t in ["WebFetch", "WebSearch"] { assert_eq!(classify(t), Kind::Web, "{t}"); }
        assert_eq!(classify("mcp__x__y"), Kind::Mcp);
        assert_eq!(classify("mcp__"), Kind::HandBack);
        for t in ["AskUserQuestion", "ExitPlanMode", "Mystery", ""] { assert_eq!(classify(t), Kind::HandBack, "{t}"); }
    }

    #[test]
    fn vet_accepts_one_allow_rule() {
        let r = vet(&sugg(json!("node -e \"x\""))).unwrap();
        assert_eq!(r, Rule { tool: "Bash".into(), content: "node -e \"x\"".into(), destination: "localSettings".into() });
        assert!(vet(&sugg(json!("node *"))).is_some());
    }

    #[test]
    fn vet_rejects_bad_shapes() {
        assert!(vet(&json!(null)).is_none());
        assert!(vet(&json!({})).is_none());
        assert!(vet(&json!("x")).is_none());
        assert!(vet(&json!([])).is_none());
        let one = sugg(json!("ls"))[0].clone();
        assert!(vet(&json!([one.clone(), one.clone()])).is_none());
        let mut wrong = one.clone(); wrong["type"] = json!("setMode");
        assert!(vet(&json!([wrong])).is_none());
        let mut deny = one.clone(); deny["behavior"] = json!("deny");
        assert!(vet(&json!([deny])).is_none());
        let mut two = one.clone(); two["rules"] = json!([{"toolName":"Bash","ruleContent":"a"},{"toolName":"Bash","ruleContent":"b"}]);
        assert!(vet(&json!([two])).is_none());
        let mut norules = one.clone(); norules["rules"] = json!([]);
        assert!(vet(&json!([norules])).is_none());
        let mut nocontent = one.clone(); nocontent["rules"] = json!([{"toolName":"Bash"}]);
        assert!(vet(&json!([nocontent])).is_none());
        let mut notool = one.clone(); notool["rules"] = json!([{"ruleContent":"ls"}]);
        assert!(vet(&json!([notool])).is_none());
    }

    #[test]
    fn vet_rejects_empty_and_wildcard_only_content() {
        for c in ["", " ", "*", "**", ":*", " * ", "* *", "*:*", "\t:*\n", "**/*", "/**", ".*", "?*", "(*)", "-*", "~/*", "\\*"] {
            assert!(vet(&sugg(json!(c))).is_none(), "{c:?}");
        }
        assert!(vet(&sugg(json!(5))).is_none());
    }

    #[test]
    fn vet_keeps_real_commands() {
        for c in ["node *", "npm run test:*", "git status", "./run.sh *"] {
            assert!(vet(&sugg(json!(c))).is_some(), "{c:?}");
        }
    }

    #[test]
    fn vet_requires_tool_to_match_request() {
        let s = sugg(json!("node *"));
        assert!(vet_suggestion(&s, "Bash").is_some());
        assert!(vet_suggestion(&s, "PowerShell").is_none());
        assert!(vet_suggestion(&s, "bash").is_none());
        assert!(vet_suggestion(&s, "").is_none());
    }

    #[test]
    fn reply_rejects_unvetted_rule_content() {
        for c in ["*", "**/*", ""] {
            let bad = rule(c);
            assert!(reply(Choice::AllowSession, Some(&bad)).is_none(), "{c:?}");
            assert!(reply(Choice::AlwaysAllow, Some(&bad)).is_none(), "{c:?}");
        }
        let mut notool = rule("node *"); notool.tool = " ".into();
        assert!(reply(Choice::AllowSession, Some(&notool)).is_none());
    }

    #[test]
    fn debug_output_hides_rule_content() {
        let text = format!("{:?}", rule("node secret.js"));
        assert!(text.contains("Bash"), "{text}");
        assert!(!text.contains("secret"), "{text}");
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
        let files: [(&str, &str); 20] = [
            ("app.rs", include_str!("app.rs")),
            ("approvals.rs", include_str!("approvals.rs")),
            ("delivery.rs", include_str!("delivery.rs")),
            ("events.rs", include_str!("events.rs")),
            ("hats.rs", include_str!("hats.rs")),
            ("installer.rs", include_str!("installer.rs")),
            ("jump.rs", include_str!("jump.rs")),
            ("listener.rs", include_str!("listener.rs")),
            ("main.rs", include_str!("main.rs")),
            ("overlay.rs", include_str!("overlay.rs")),
            ("paths.rs", include_str!("paths.rs")),
            ("pending.rs", include_str!("pending.rs")),
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
