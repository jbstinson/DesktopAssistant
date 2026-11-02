//! Hook event payload as forwarded by da-hook. Pure data, no Tauri.

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct HookEvent {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub notification_type: Option<String>,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub terminal_pid: Option<u32>,
    /// Set when the event comes from a subagent rather than the main session.
    #[serde(default)]
    pub agent_id: Option<String>,
}

/// A permission request as sent to the permission route. Holds the tool's input, which stays
/// in memory only, so this type deliberately has no Debug.
#[derive(Deserialize)]
pub struct PermissionAsk {
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub terminal_pid: Option<u32>,
    #[serde(default)]
    pub tool_name: String,
    #[serde(default)]
    pub tool_input: Value,
    #[serde(default)]
    pub permission_suggestions: Value,
    #[serde(default)]
    pub description: Option<String>,
}

impl PermissionAsk {
    /// The session side of the request, without any of its input.
    pub fn to_event(&self) -> HookEvent {
        HookEvent {
            name: "PermissionRequest".to_string(),
            session_id: self.session_id.clone(),
            cwd: self.cwd.clone(),
            tool: Some(self.tool_name.clone()),
            notification_type: None,
            source: self.source.clone(),
            terminal_pid: self.terminal_pid,
            agent_id: self.agent_id.clone(),
        }
    }
}

/// What a poll of a permission request finds.
#[derive(Clone, PartialEq, Eq)]
pub enum Polled {
    Waiting,
    /// The exact reply for Claude Code.
    Reply(String),
    Gone,
}

/// A reply can carry rule content from the request's input, so Debug output leaves it out.
impl std::fmt::Debug for Polled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Polled::Waiting => "Waiting",
            Polled::Reply(_) => "Reply",
            Polled::Gone => "Gone",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_event() {
        let e: HookEvent = serde_json::from_str(
            r#"{"name":"PreToolUse","session_id":"s1","cwd":"C:\\git\\x","tool":"Read",
                "notification_type":"idle_prompt","source":"terminal","terminal_pid":42,"agent_id":"ag1"}"#,
        )
        .unwrap();
        assert_eq!(e.name, "PreToolUse");
        assert_eq!(e.session_id, "s1");
        assert_eq!(e.cwd, "C:\\git\\x");
        assert_eq!(e.tool.as_deref(), Some("Read"));
        assert_eq!(e.notification_type.as_deref(), Some("idle_prompt"));
        assert_eq!(e.source, "terminal");
        assert_eq!(e.terminal_pid, Some(42));
        assert_eq!(e.agent_id.as_deref(), Some("ag1"));
    }

    #[test]
    fn parses_a_permission_ask() {
        let a: PermissionAsk = serde_json::from_str(
            r#"{"session_id":"s1","cwd":"C:/x","source":"cli","agent_id":"ag","terminal_pid":9,
                "tool_name":"Bash","tool_input":{"command":"ls"},
                "permission_suggestions":[1],"description":"List"}"#,
        )
        .unwrap();
        assert_eq!(a.tool_input["command"], "ls");
        assert_eq!(a.permission_suggestions[0], 1);
        assert_eq!(a.description.as_deref(), Some("List"));
        let e = a.to_event();
        assert_eq!(e.name, "PermissionRequest");
        assert_eq!(e.session_id, "s1");
        assert_eq!(e.cwd, "C:/x");
        assert_eq!(e.tool.as_deref(), Some("Bash"));
        assert_eq!(e.notification_type, None);
        assert_eq!(e.source, "cli");
        assert_eq!(e.terminal_pid, Some(9));
        assert_eq!(e.agent_id.as_deref(), Some("ag"));
    }

    #[test]
    fn debug_output_hides_a_reply() {
        assert_eq!(format!("{:?}", Polled::Reply("node secret.js".into())), "Reply");
    }

    #[test]
    fn a_permission_ask_with_missing_fields_defaults() {
        let a: PermissionAsk = serde_json::from_str("{}").unwrap();
        assert_eq!(a.tool_name, "");
        assert!(a.tool_input.is_null() && a.permission_suggestions.is_null());
        assert_eq!(a.description, None);
    }

    #[test]
    fn missing_fields_default() {
        let e: HookEvent =
            serde_json::from_str(r#"{"name":"Stop","session_id":"s1","cwd":"/a"}"#).unwrap();
        assert_eq!(e.tool, None);
        assert_eq!(e.notification_type, None);
        assert_eq!(e.source, "");
        assert_eq!(e.terminal_pid, None);
        assert_eq!(e.agent_id, None);
    }
}
