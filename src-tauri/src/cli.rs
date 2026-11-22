//! Command-line modes that run instead of the character. `--disconnect` removes this app's hook
//! entries from Claude Code's settings and exits; the uninstaller runs it so an uninstall never
//! leaves Claude Code pointing at a deleted da-hook.exe.

use std::path::Path;

/// True when the command line asks for `--disconnect`.
pub fn wants_disconnect(args: impl IntoIterator<Item = String>) -> bool {
    args.into_iter().any(|a| a == "--disconnect")
}

/// Removes only this app's hook entries from `settings` (Claude Code's settings.json), the same
/// as unticking Connect. No settings path (no USERPROFILE) or no file means nothing to do; a
/// missing file is never created.
pub fn disconnect(settings: Option<&Path>) -> Result<(), String> {
    match settings {
        Some(path) => crate::installer::apply(path, crate::installer::Change::Remove),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("da-cli-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn wants_disconnect_only_for_the_flag() {
        assert!(wants_disconnect(args(&["app.exe", "--disconnect"])));
        assert!(!wants_disconnect(args(&["app.exe"])));
        assert!(!wants_disconnect(args(&["app.exe", "--disconnected"])));
        assert!(!wants_disconnect(args(&["app.exe", "disconnect"])));
    }

    #[test]
    fn disconnect_removes_only_our_entries() {
        let path = temp_dir("ours").join("settings.json");
        let start = crate::installer::add("{}", r"C:\x\da-hook.exe").unwrap();
        let mut v: serde_json::Value = serde_json::from_str(&start).unwrap();
        v["hooks"]["PreToolUse"].as_array_mut().unwrap().push(serde_json::json!({
            "hooks": [{ "type": "command", "command": "\"C:/other/tool.exe\"" }]
        }));
        std::fs::write(&path, v.to_string()).unwrap();

        disconnect(Some(&path)).unwrap();
        let after = std::fs::read_to_string(&path).unwrap();
        assert!(!after.contains("da-hook.exe"), "{after}");
        assert!(after.contains("tool.exe"), "{after}");
    }

    #[test]
    fn disconnect_without_a_settings_file_creates_nothing() {
        let path = temp_dir("missing").join(".claude").join("settings.json");
        disconnect(Some(&path)).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn disconnect_without_userprofile_is_ok() {
        assert_eq!(disconnect(None), Ok(()));
    }
}
