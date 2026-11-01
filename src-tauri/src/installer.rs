//! Adds and removes this app's hook entries in Claude Code's settings.json.
//!
//! Pure string-in/string-out logic (`add`, `remove`, `is_installed`) plus `apply`,
//! which does the guarded file work (backup, temp file, rename).

use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

pub const EVENTS: [&str; 9] = [
    "SessionStart",
    "SessionEnd",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUseFailure",
    "PermissionRequest",
    "Notification",
    "Stop",
    "StopFailure",
];

pub enum Change {
    Add(String),
    Remove(String),
}

/// The exact command string that identifies our hook.
fn command_for(hook_path: &str) -> String {
    format!("\"{}\"", hook_path.replace('\\', "/"))
}

fn parse(settings: &str) -> Result<Map<String, Value>, String> {
    let text = settings.trim_start_matches('\u{feff}');
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(m)) => Ok(m),
        Ok(_) => Err("settings root is not a JSON object".to_string()),
        Err(e) => Err(format!("invalid settings JSON: {e}")),
    }
}

fn render(root: Map<String, Value>, original: &str) -> Result<String, String> {
    let mut out = serde_json::to_string_pretty(&Value::Object(root)).map_err(|e| e.to_string())?;
    if original.trim().is_empty() || original.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}

fn is_our_hook(hook: &Value, command: &str) -> bool {
    hook.get("command").and_then(Value::as_str) == Some(command)
}

fn group_has_command(group: &Value, command: &str) -> bool {
    group
        .get("hooks")
        .and_then(Value::as_array)
        .map(|hs| hs.iter().any(|h| is_our_hook(h, command)))
        .unwrap_or(false)
}

/// Checks that `hooks` (if present) is an object and each of our events (if present) is an array.
fn validate(root: &Map<String, Value>) -> Result<(), String> {
    match root.get("hooks") {
        None => Ok(()),
        Some(Value::Object(hooks)) => {
            for ev in EVENTS {
                if hooks.get(ev).is_some_and(|v| !v.is_array()) {
                    return Err(format!("hooks.{ev} is not an array"));
                }
            }
            Ok(())
        }
        Some(_) => Err("\"hooks\" is not an object".to_string()),
    }
}

pub fn add(settings: &str, hook_path: &str) -> Result<String, String> {
    let mut root = parse(settings)?;
    validate(&root)?;
    let command = command_for(hook_path);
    let hooks = root
        .entry("hooks".to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or("\"hooks\" is not an object")?;
    for ev in EVENTS {
        let arr = hooks
            .entry(ev.to_string())
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or_else(|| format!("hooks.{ev} is not an array"))?;
        if !arr.iter().any(|g| group_has_command(g, &command)) {
            arr.push(json!({"hooks": [{"type": "command", "command": command, "timeout": 5}]}));
        }
    }
    render(root, settings)
}

pub fn remove(settings: &str, hook_path: &str) -> Result<String, String> {
    let mut root = parse(settings)?;
    validate(&root)?;
    let command = command_for(hook_path);
    let mut changed = false;
    if let Some(hooks) = root.get_mut("hooks").and_then(Value::as_object_mut) {
        for ev in EVENTS {
            let Some(arr) = hooks.get_mut(ev).and_then(Value::as_array_mut) else {
                continue;
            };
            let mut event_changed = false;
            arr.retain_mut(|group| {
                if !group_has_command(group, &command) {
                    return true;
                }
                let Some(inner) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
                    return true;
                };
                inner.retain(|h| !is_our_hook(h, &command));
                event_changed = true;
                !inner.is_empty()
            });
            if event_changed {
                changed = true;
                if arr.is_empty() {
                    hooks.shift_remove(ev);
                }
            }
        }
        if changed && hooks.is_empty() {
            root.shift_remove("hooks");
        }
    }
    render(root, settings)
}

/// True when our hook is present on all nine events.
pub fn is_installed(settings: &str, hook_path: &str) -> bool {
    let Ok(root) = parse(settings) else { return false };
    let command = command_for(hook_path);
    let Some(hooks) = root.get("hooks").and_then(Value::as_object) else {
        return false;
    };
    EVENTS.iter().all(|ev| {
        hooks
            .get(*ev)
            .and_then(Value::as_array)
            .is_some_and(|a| a.iter().any(|g| group_has_command(g, &command)))
    })
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(suffix);
    path.with_file_name(name)
}

pub fn apply(path: &Path, change: Change) -> Result<(), String> {
    let (original, existed) = match std::fs::read(path) {
        Ok(bytes) => (
            String::from_utf8(bytes).map_err(|_| "settings file is not valid UTF-8".to_string())?,
            true,
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (String::new(), false),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    let updated = match &change {
        Change::Add(p) => add(&original, p)?,
        Change::Remove(p) => remove(&original, p)?,
    };
    // Unchanged in meaning: do not touch the file (or create one).
    if parse(&original)? == parse(&updated)? {
        return Ok(());
    }
    if existed {
        // The first backup is the user's true original; later ones would contain our entries.
        let backup = sibling(path, ".desktop-assistant.bak");
        if !backup.exists() {
            std::fs::copy(path, &backup).map_err(|e| format!("cannot write backup: {e}"))?;
        }
    } else if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = sibling(path, ".desktop-assistant.tmp");
    std::fs::write(&tmp, updated).map_err(|e| format!("cannot write temp file: {e}"))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("cannot replace settings file: {e}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HP: &str = r"C:\Users\x\AppData\Roaming\DesktopAssistant\da-hook.exe";
    const CMD: &str = "\"C:/Users/x/AppData/Roaming/DesktopAssistant/da-hook.exe\"";

    fn v(s: &str) -> Value {
        serde_json::from_str(s).unwrap()
    }

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("da-installer-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn add_on_empty_creates_all_nine_events() {
        for input in ["", "  \n ", "{}"] {
            let out = v(&add(input, HP).unwrap());
            for ev in EVENTS {
                let arr = out["hooks"][ev].as_array().unwrap();
                assert_eq!(arr.len(), 1);
                assert_eq!(
                    arr[0],
                    json!({"hooks":[{"type":"command","command":CMD,"timeout":5}]})
                );
            }
            assert_eq!(out["hooks"].as_object().unwrap().len(), 9);
        }
    }

    #[test]
    fn add_preserves_existing_entries_and_keys() {
        let input = r#"{
  "model": "opus",
  "hooks": {
    "SessionStart": [{"matcher":"startup","hooks":[{"type":"command","command":"echo hi"}]}],
    "PostToolUse": [{"matcher":"Edit","hooks":[{"type":"command","command":"fmt"}]}]
  },
  "env": {"A":"1"}
}"#;
        let out = v(&add(input, HP).unwrap());
        assert_eq!(out["model"], "opus");
        assert_eq!(out["env"], json!({"A":"1"}));
        assert_eq!(out["hooks"]["PostToolUse"], v(input)["hooks"]["PostToolUse"]);
        let ss = out["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(ss.len(), 2);
        assert_eq!(ss[0], v(input)["hooks"]["SessionStart"][0]);
        assert_eq!(ss[1]["hooks"][0]["command"], CMD);
        let keys: Vec<_> = out.as_object().unwrap().keys().cloned().collect();
        assert_eq!(keys, ["model", "hooks", "env"]);
    }

    #[test]
    fn add_twice_equals_add_once() {
        let once = add("{\"a\":1}", HP).unwrap();
        assert_eq!(add(&once, HP).unwrap(), once);
    }

    #[test]
    fn remove_restores_original_semantically() {
        let input = r#"{"model":"opus","hooks":{"Stop":[{"hooks":[{"type":"command","command":"x"}]}],"PostToolUse":[]}}"#;
        let added = add(input, HP).unwrap();
        assert!(is_installed(&added, HP));
        let back = remove(&added, HP).unwrap();
        assert_eq!(v(&back), v(input));
        assert!(!is_installed(&back, HP));
    }

    #[test]
    fn remove_from_empty_add_leaves_empty_object() {
        let back = remove(&add("", HP).unwrap(), HP).unwrap();
        assert_eq!(v(&back), json!({}));
    }

    #[test]
    fn remove_only_our_hook_inside_shared_group() {
        let input = json!({"hooks":{"Stop":[{"matcher":"m","hooks":[
            {"type":"command","command":"other"},
            {"type":"command","command":CMD,"timeout":5}]}]}})
        .to_string();
        let out = v(&remove(&input, HP).unwrap());
        assert_eq!(
            out["hooks"]["Stop"],
            json!([{"matcher":"m","hooks":[{"type":"command","command":"other"}]}])
        );
    }

    #[test]
    fn remove_without_our_entries_is_noop() {
        let input = r#"{"hooks":{"Stop":[{"hooks":[]}]},"x":[1,2]}"#;
        assert_eq!(v(&remove(input, HP).unwrap()), v(input));
        assert_eq!(v(&remove("", HP).unwrap()), json!({}));
    }

    #[test]
    fn other_path_is_not_ours() {
        let added = add("{}", HP).unwrap();
        assert!(!is_installed(&added, r"C:\other\da-hook.exe"));
        assert_eq!(v(&remove(&added, r"C:\other\da-hook.exe").unwrap()), v(&added));
    }

    #[test]
    fn invalid_input_is_err() {
        assert!(add("{not json", HP).is_err());
        assert!(remove("{not json", HP).is_err());
        assert!(add("[]", HP).is_err());
        assert!(add("3", HP).is_err());
        assert!(add(r#"{"hooks":[]}"#, HP).is_err());
        assert!(remove(r#"{"hooks":"x"}"#, HP).is_err());
        assert!(add(r#"{"hooks":{"Stop":{}}}"#, HP).is_err());
        assert!(remove(r#"{"hooks":{"Stop":"x"}}"#, HP).is_err());
        assert!(!is_installed("{not json", HP));
    }

    #[test]
    fn output_is_two_space_pretty() {
        let out = add("{}", HP).unwrap();
        assert!(out.starts_with("{\n  \"hooks\": {\n    \"SessionStart\": ["));
    }

    #[test]
    fn apply_writes_backup_then_changes_file() {
        let d = temp_dir("backup");
        let f = d.join("settings.json");
        let original = "{\n  \"model\": \"opus\"\n}\n";
        std::fs::write(&f, original).unwrap();
        apply(&f, Change::Add(HP.into())).unwrap();
        let bak = d.join("settings.json.desktop-assistant.bak");
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), original);
        assert!(is_installed(&std::fs::read_to_string(&f).unwrap(), HP));
        assert!(!d.join("settings.json.desktop-assistant.tmp").exists());
        // Second change keeps the first backup.
        apply(&f, Change::Remove(HP.into())).unwrap();
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), original);
        assert_eq!(
            v(&std::fs::read_to_string(&f).unwrap()),
            json!({"model":"opus"})
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn apply_leaves_file_untouched_on_err() {
        let d = temp_dir("err");
        let f = d.join("settings.json");
        std::fs::write(&f, "{broken").unwrap();
        assert!(apply(&f, Change::Add(HP.into())).is_err());
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "{broken");
        assert!(!d.join("settings.json.desktop-assistant.bak").exists());
        assert!(!d.join("settings.json.desktop-assistant.tmp").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn apply_missing_file_creates_without_backup() {
        let d = temp_dir("missing");
        let f = d.join("settings.json");
        apply(&f, Change::Remove(HP.into())).unwrap();
        assert!(!f.exists());
        apply(&f, Change::Add(HP.into())).unwrap();
        assert!(is_installed(&std::fs::read_to_string(&f).unwrap(), HP));
        assert!(!d.join("settings.json.desktop-assistant.bak").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn apply_skips_write_when_unchanged() {
        let d = temp_dir("unchanged");
        let f = d.join("settings.json");
        let text = "{ \"model\":   \"opus\" }";
        std::fs::write(&f, text).unwrap();
        apply(&f, Change::Remove(HP.into())).unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), text);
        assert!(!d.join("settings.json.desktop-assistant.bak").exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}
