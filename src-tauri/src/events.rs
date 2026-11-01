//! Hook event payload as forwarded by da-hook. Pure data, no Tauri.

use serde::Deserialize;

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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_event() {
        let e: HookEvent = serde_json::from_str(
            r#"{"name":"PreToolUse","session_id":"s1","cwd":"C:\\git\\x","tool":"Read",
                "notification_type":"idle_prompt","source":"terminal","terminal_pid":42}"#,
        )
        .unwrap();
        assert_eq!(e.name, "PreToolUse");
        assert_eq!(e.session_id, "s1");
        assert_eq!(e.cwd, "C:\\git\\x");
        assert_eq!(e.tool.as_deref(), Some("Read"));
        assert_eq!(e.notification_type.as_deref(), Some("idle_prompt"));
        assert_eq!(e.source, "terminal");
        assert_eq!(e.terminal_pid, Some(42));
    }

    #[test]
    fn missing_fields_default() {
        let e: HookEvent =
            serde_json::from_str(r#"{"name":"Stop","session_id":"s1","cwd":"/a"}"#).unwrap();
        assert_eq!(e.tool, None);
        assert_eq!(e.notification_type, None);
        assert_eq!(e.source, "");
        assert_eq!(e.terminal_pid, None);
    }
}
