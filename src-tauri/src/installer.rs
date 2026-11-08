//! Adds and removes this app's hook entries in Claude Code's settings.json.
//!
//! Pure string-in/string-out logic (`add`, `remove`, `is_installed`) plus `apply`,
//! which does the guarded file work (backup, temp file, rename).

use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

pub const EVENTS: [&str; 12] = [
    "SessionStart",
    "SessionEnd",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUseFailure",
    "PermissionRequest",
    "Notification",
    "Stop",
    "StopFailure",
    "Elicitation",
    "ElicitationResult",
    "PermissionDenied",
];

pub enum Change {
    Add(String),
    Remove,
}

/// How long Claude Code lets our hook run, in seconds. For PermissionRequest this must stay above
/// `PERMISSION_LIMIT` in hook/src/main.rs (128), itself above `app::HOLD_MAX_SECS` (125).
fn timeout_for(event: &str) -> u64 {
    if event == "PermissionRequest" {
        130
    } else {
        5
    }
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

const HOOK_EXE: &str = "da-hook.exe";

/// Whether a hook command is one of ours, wherever the exe used to live: a quoted path whose
/// file name is exactly da-hook.exe (either slash style, any case). Anything else, including
/// look-alikes such as my-da-hook.exe or da-hook.exe.bak, or the same exe with arguments, is
/// the user's.
fn is_our_command(command: &str) -> bool {
    let Some(path) = command.trim().strip_prefix('"').and_then(|c| c.strip_suffix('"')) else {
        return false;
    };
    if path.contains('"') {
        return false;
    }
    let name = path.rsplit(['/', '\\']).next().unwrap_or("");
    name.eq_ignore_ascii_case(HOOK_EXE)
}

fn is_our_hook(hook: &Value) -> bool {
    hook.get("command").and_then(Value::as_str).is_some_and(is_our_command)
}

/// Our hook at exactly this path with exactly this timeout.
fn is_current_hook(hook: &Value, command: &str, timeout: u64) -> bool {
    hook.get("command").and_then(Value::as_str) == Some(command)
        && hook.get("timeout").and_then(Value::as_u64) == Some(timeout)
}

fn group_matches(group: &Value, pred: impl Fn(&Value) -> bool) -> bool {
    group
        .get("hooks")
        .and_then(Value::as_array)
        .map(|hs| hs.iter().any(pred))
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
        let timeout = timeout_for(ev);
        let arr = hooks
            .entry(ev.to_string())
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or_else(|| format!("hooks.{ev} is not an array"))?;
        // The first of our entries, current or stale, becomes the current one in place (a
        // current one is kept as written); any further ones are dropped.
        let mut placed = false;
        arr.retain_mut(|group| {
            if !group_matches(group, is_our_hook) {
                return true;
            }
            let Some(inner) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
                return true;
            };
            inner.retain_mut(|h| {
                if !is_our_hook(h) {
                    return true;
                }
                if placed {
                    return false;
                }
                placed = true;
                if !is_current_hook(h, &command, timeout) {
                    *h = json!({"type": "command", "command": command, "timeout": timeout});
                }
                true
            });
            !inner.is_empty()
        });
        if !placed {
            arr.push(json!({"hooks": [{"type": "command", "command": command, "timeout": timeout}]}));
        }
    }
    render(root, settings)
}

/// Drops every hook of ours, whatever path it points at.
pub fn remove(settings: &str) -> Result<String, String> {
    let mut root = parse(settings)?;
    validate(&root)?;
    let mut changed = false;
    if let Some(hooks) = root.get_mut("hooks").and_then(Value::as_object_mut) {
        for ev in EVENTS {
            let Some(arr) = hooks.get_mut(ev).and_then(Value::as_array_mut) else {
                continue;
            };
            let mut event_changed = false;
            arr.retain_mut(|group| {
                if !group_matches(group, is_our_hook) {
                    return true;
                }
                let Some(inner) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
                    return true;
                };
                inner.retain(|h| !is_our_hook(h));
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

/// True when any of our hook entries is present, at whatever path.
pub fn has_any(settings: &str) -> bool {
    let Ok(root) = parse(settings) else { return false };
    let Some(hooks) = root.get("hooks").and_then(Value::as_object) else {
        return false;
    };
    EVENTS.iter().any(|ev| {
        hooks
            .get(*ev)
            .and_then(Value::as_array)
            .is_some_and(|a| a.iter().any(|g| group_matches(g, is_our_hook)))
    })
}

/// True when our hook, at exactly this path and with exactly its timeout, is on all nine events.
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
            .is_some_and(|a| a.iter().any(|g| group_matches(g, |h| is_current_hook(h, &command, timeout_for(ev)))))
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
        Change::Remove => remove(&original)?,
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
                let timeout = if ev == "PermissionRequest" { 130 } else { 5 };
                assert_eq!(
                    arr[0],
                    json!({"hooks":[{"type":"command","command":CMD,"timeout":timeout}]})
                );
            }
            assert_eq!(out["hooks"].as_object().unwrap().len(), 12);
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
    fn an_install_without_the_newer_events_is_stale_until_connect_adds_them() {
        let mut old = v(&add("", HP).unwrap());
        for ev in ["Elicitation", "ElicitationResult", "PermissionDenied"] {
            old["hooks"].as_object_mut().unwrap().remove(ev);
        }
        let old = old.to_string();
        assert!(!is_installed(&old, HP));
        assert!(has_any(&old));
        let repaired = add(&old, HP).unwrap();
        assert!(is_installed(&repaired, HP));
        for ev in ["Elicitation", "ElicitationResult", "PermissionDenied"] {
            assert_eq!(v(&repaired)["hooks"][ev][0]["hooks"][0]["timeout"], 5, "{ev}");
        }
    }

    #[test]
    fn remove_restores_original_semantically() {
        let input = r#"{"model":"opus","hooks":{"Stop":[{"hooks":[{"type":"command","command":"x"}]}],"PostToolUse":[]}}"#;
        let added = add(input, HP).unwrap();
        assert!(is_installed(&added, HP));
        let back = remove(&added).unwrap();
        assert_eq!(v(&back), v(input));
        assert!(!is_installed(&back, HP));
    }

    #[test]
    fn remove_from_empty_add_leaves_empty_object() {
        let back = remove(&add("", HP).unwrap()).unwrap();
        assert_eq!(v(&back), json!({}));
    }

    #[test]
    fn remove_only_our_hook_inside_shared_group() {
        let input = json!({"hooks":{"Stop":[{"matcher":"m","hooks":[
            {"type":"command","command":"other"},
            {"type":"command","command":CMD,"timeout":5}]}]}})
        .to_string();
        let out = v(&remove(&input).unwrap());
        assert_eq!(
            out["hooks"]["Stop"],
            json!([{"matcher":"m","hooks":[{"type":"command","command":"other"}]}])
        );
    }

    #[test]
    fn remove_without_our_entries_is_noop() {
        let input = r#"{"hooks":{"Stop":[{"hooks":[]}]},"x":[1,2]}"#;
        assert_eq!(v(&remove(input).unwrap()), v(input));
        assert_eq!(v(&remove("").unwrap()), json!({}));
    }

    const OLD: &str = "\"D:/old/place/da-hook.exe\"";

    fn stale_everywhere(command: &str) -> String {
        let mut hooks = Map::new();
        for ev in EVENTS {
            hooks.insert(ev.to_string(), json!([{"hooks":[{"type":"command","command":command,"timeout":5}]}]));
        }
        json!({ "hooks": hooks }).to_string()
    }

    #[test]
    fn moved_path_is_not_installed_but_has_any() {
        let stale = stale_everywhere(OLD);
        assert!(!is_installed(&stale, HP));
        assert!(has_any(&stale));
        assert!(!has_any("{}"));
        assert!(!has_any(""));
        assert!(!has_any("{not json"));
        assert!(has_any(&add("{}", HP).unwrap()));
        // One event is enough.
        assert!(has_any(&json!({"hooks":{"Stop":[{"hooks":[{"command":OLD}]}]}}).to_string()));
    }

    #[test]
    fn add_repairs_a_moved_path_without_duplicates() {
        let out = add(&stale_everywhere(OLD), HP).unwrap();
        assert!(is_installed(&out, HP));
        assert!(!out.contains("old/place"));
        let out = v(&out);
        for ev in EVENTS {
            let arr = out["hooks"][ev].as_array().unwrap();
            assert_eq!(arr.len(), 1, "{ev}");
            assert_eq!(arr[0]["hooks"][0]["command"], CMD);
        }
    }

    #[test]
    fn timeout_outlasts_the_hook_limit_which_outlasts_the_longest_hold() {
        // The hook is a separate crate, so its limit is read from its source.
        const HOOK_LIMIT: u64 = 128;
        let hook = include_str!("../../hook/src/main.rs");
        assert!(hook.contains(&format!("PERMISSION_LIMIT: Duration = Duration::from_secs({HOOK_LIMIT});")));
        assert_eq!(crate::app::HOLD_MAX_SECS, 125);
        assert!(crate::app::HOLD_MAX_SECS < HOOK_LIMIT);
        assert!(HOOK_LIMIT < timeout_for("PermissionRequest"));
        assert_eq!(timeout_for("PermissionRequest"), 130);
    }

    #[test]
    fn permission_request_entry_with_the_old_timeout_is_stale_and_add_repairs_it() {
        let old = stale_everywhere(CMD);
        assert!(!is_installed(&old, HP));
        assert!(has_any(&old));
        let out = add(&old, HP).unwrap();
        assert!(is_installed(&out, HP));
        let out = v(&out);
        for ev in EVENTS {
            let arr = out["hooks"][ev].as_array().unwrap();
            assert_eq!(arr.len(), 1, "{ev}");
            let want = if ev == "PermissionRequest" { 130 } else { 5 };
            assert_eq!(arr[0]["hooks"][0]["timeout"], want, "{ev}");
            assert_eq!(arr[0]["hooks"][0]["command"], CMD, "{ev}");
        }
    }

    #[test]
    fn timeout_repair_stays_in_place_inside_a_shared_group() {
        let input = json!({"hooks":{"PermissionRequest":[{"matcher":"m","hooks":[
            {"type":"command","command":"mine-first"},
            {"type":"command","command":CMD,"timeout":5},
            {"type":"command","command":"mine-last"}]}]}})
        .to_string();
        let out = v(&add(&input, HP).unwrap());
        assert_eq!(
            out["hooks"]["PermissionRequest"],
            json!([{"matcher":"m","hooks":[
                {"type":"command","command":"mine-first"},
                {"type":"command","command":CMD,"timeout":130},
                {"type":"command","command":"mine-last"}]}])
        );
    }

    #[test]
    fn any_timeout_other_than_the_exact_one_is_stale() {
        let good = add("{}", HP).unwrap();
        for (ev, bad) in [("Stop", json!(6)), ("Stop", json!(null)), ("PermissionRequest", json!(129)), ("PermissionRequest", json!("130"))] {
            let mut root = v(&good);
            root["hooks"][ev][0]["hooks"][0]["timeout"] = bad.clone();
            assert!(!is_installed(&root.to_string(), HP), "{ev} {bad}");
        }
        let mut root = v(&good);
        root["hooks"]["Stop"][0]["hooks"][0].as_object_mut().unwrap().shift_remove("timeout");
        assert!(!is_installed(&root.to_string(), HP));
        assert!(is_installed(&add(&root.to_string(), HP).unwrap(), HP));
    }

    #[test]
    fn apply_repairs_an_old_timeout_in_a_temp_file() {
        let d = temp_dir("timeout");
        let f = d.join("settings.json");
        std::fs::write(&f, stale_everywhere(CMD)).unwrap();
        apply(&f, Change::Add(HP.into())).unwrap();
        assert!(is_installed(&std::fs::read_to_string(&f).unwrap(), HP));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn add_collapses_several_stale_and_current_entries_to_one() {
        let input = json!({"hooks":{"Stop":[
            {"hooks":[{"type":"command","command":OLD}]},
            {"hooks":[{"type":"command","command":"\"C:\\older\\DA-HOOK.EXE\""}]},
            {"hooks":[{"type":"command","command":CMD,"timeout":5}]}
        ]}})
        .to_string();
        let out = v(&add(&input, HP).unwrap());
        let arr = out["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["hooks"][0]["command"], CMD);
    }

    #[test]
    fn repair_keeps_other_hooks_in_mixed_groups() {
        let input = json!({"hooks":{"Stop":[
            {"matcher":"m","hooks":[
                {"type":"command","command":"other-first"},
                {"type":"command","command":OLD,"timeout":5},
                {"type":"command","command":"other-last"}]}
        ]}})
        .to_string();
        let out = v(&add(&input, HP).unwrap());
        let stop = out["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 1);
        assert_eq!(stop[0]["matcher"], "m");
        let cmds: Vec<_> = stop[0]["hooks"].as_array().unwrap().iter().map(|h| h["command"].clone()).collect();
        assert_eq!(cmds, vec![json!("other-first"), json!(CMD), json!("other-last")]);
        // And removal leaves just the user's hooks.
        let back = v(&remove(&out.to_string()).unwrap());
        assert_eq!(
            back["hooks"]["Stop"],
            json!([{"matcher":"m","hooks":[
                {"type":"command","command":"other-first"},
                {"type":"command","command":"other-last"}]}])
        );
    }

    #[test]
    fn remove_drops_a_stale_path() {
        let back = v(&remove(&stale_everywhere(OLD)).unwrap());
        assert_eq!(back, json!({}));
        let slash = stale_everywhere("\"C:\\x\\da-hook.exe\"");
        assert_eq!(v(&remove(&slash).unwrap()), json!({}));
    }

    #[test]
    fn lookalikes_are_not_ours() {
        let others = [
            "\"C:/x/my-da-hook.exe-wrapper\"",
            "\"C:/x/da-hook.exe.bak\"",
            "\"C:/x/my-da-hook.exe\"",
            "C:/x/da-hook.exe",
            "\"C:/x/da-hook.exe\" --flag",
            "echo da-hook.exe",
            "\"\"",
            "\"da-hook.exe",
        ];
        for c in others {
            let input = json!({"hooks":{"Stop":[{"hooks":[{"type":"command","command":c}]}]}}).to_string();
            assert!(!has_any(&input), "{c}");
            assert_eq!(v(&remove(&input).unwrap()), v(&input), "{c}");
            let added = v(&add(&input, HP).unwrap());
            assert_eq!(added["hooks"]["Stop"][0]["hooks"][0]["command"], c, "{c}");
            assert_eq!(added["hooks"]["Stop"].as_array().unwrap().len(), 2, "{c}");
        }
    }

    #[test]
    fn filename_match_ignores_case_and_slash_style() {
        for c in ["\"C:\\x\\Da-Hook.EXE\"", "\"//server/share/da-hook.exe\"", "\"da-hook.exe\""] {
            let input = json!({"hooks":{"Stop":[{"hooks":[{"command":c}]}]}}).to_string();
            assert!(has_any(&input), "{c}");
        }
    }

    #[test]
    fn invalid_input_is_err() {
        assert!(add("{not json", HP).is_err());
        assert!(remove("{not json").is_err());
        assert!(add("[]", HP).is_err());
        assert!(add("3", HP).is_err());
        assert!(add(r#"{"hooks":[]}"#, HP).is_err());
        assert!(remove(r#"{"hooks":"x"}"#).is_err());
        assert!(add(r#"{"hooks":{"Stop":{}}}"#, HP).is_err());
        assert!(remove(r#"{"hooks":{"Stop":"x"}}"#).is_err());
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
        apply(&f, Change::Remove).unwrap();
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
        apply(&f, Change::Remove).unwrap();
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
        apply(&f, Change::Remove).unwrap();
        assert_eq!(std::fs::read_to_string(&f).unwrap(), text);
        assert!(!d.join("settings.json.desktop-assistant.bak").exists());
        let _ = std::fs::remove_dir_all(&d);
    }
}
