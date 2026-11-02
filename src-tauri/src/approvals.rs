//! The saved "Approve in window" settings: `approvals.json` in the data folder.
//!
//! `{"enabled": true, "hold_seconds": 120}`. A missing or malformed file, or a missing or
//! wrong-typed field, gives the default for that field. The hold is read once at startup.

use crate::app::{HOLD_MAX_SECS, HOLD_MIN_SECS};
use std::path::Path;

pub const DEFAULT_HOLD_SECS: u64 = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Approvals {
    pub enabled: bool,
    pub hold_seconds: u64,
}

impl Default for Approvals {
    fn default() -> Self {
        Approvals { enabled: true, hold_seconds: DEFAULT_HOLD_SECS }
    }
}

/// Reads the settings. Never fails: anything unreadable gives the default for that field.
pub fn load(path: &Path) -> Approvals {
    let default = Approvals::default();
    let Some(value) = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
    else {
        return default;
    };
    Approvals {
        enabled: value.get("enabled").and_then(|v| v.as_bool()).unwrap_or(default.enabled),
        hold_seconds: value
            .get("hold_seconds")
            .and_then(|v| v.as_u64())
            .unwrap_or(default.hold_seconds)
            .clamp(HOLD_MIN_SECS, HOLD_MAX_SECS),
    }
}

/// Writes the settings atomically: temp file, then rename. The hold is written as given.
pub fn save(path: &Path, a: Approvals) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = serde_json::json!({ "enabled": a.enabled, "hold_seconds": a.hold_seconds }).to_string();
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("da-approvals-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn missing_file_is_on_with_120_seconds() {
        let p = temp_dir("missing").join("approvals.json");
        assert_eq!(load(&p), Approvals { enabled: true, hold_seconds: 120 });
    }

    #[test]
    fn saved_off_is_restored_including_a_new_folder() {
        let p = temp_dir("off").join("nested").join("approvals.json");
        save(&p, Approvals { enabled: false, hold_seconds: 60 }).unwrap();
        assert_eq!(load(&p), Approvals { enabled: false, hold_seconds: 60 });
        assert!(!p.with_extension("json.tmp").exists());
    }

    #[test]
    fn malformed_file_means_on() {
        let p = temp_dir("bad").join("approvals.json");
        for text in ["", "{ not json", "[]", "null", "\"off\""] {
            std::fs::write(&p, text).unwrap();
            assert_eq!(load(&p), Approvals::default(), "{text:?}");
        }
    }

    #[test]
    fn each_missing_or_wrong_typed_field_defaults_alone() {
        let p = temp_dir("fields").join("approvals.json");
        let cases = [
            (r#"{"enabled": false}"#, false, 120),
            (r#"{"hold_seconds": 30}"#, true, 30),
            (r#"{"enabled": "no", "hold_seconds": 30}"#, true, 30),
            (r#"{"enabled": false, "hold_seconds": "30"}"#, false, 120),
            (r#"{"enabled": false, "hold_seconds": -5}"#, false, 120),
            (r#"{"enabled": false, "hold_seconds": 1.5}"#, false, 120),
        ];
        for (text, enabled, hold) in cases {
            std::fs::write(&p, text).unwrap();
            assert_eq!(load(&p), Approvals { enabled, hold_seconds: hold }, "{text}");
        }
    }

    #[test]
    fn hold_is_clamped_at_both_ends() {
        let p = temp_dir("clamp").join("approvals.json");
        for (text, want) in [
            (r#"{"hold_seconds": 0}"#, 10),
            (r#"{"hold_seconds": 9}"#, 10),
            (r#"{"hold_seconds": 10}"#, 10),
            (r#"{"hold_seconds": 125}"#, 125),
            (r#"{"hold_seconds": 126}"#, 125),
            (r#"{"hold_seconds": 99999999999}"#, 125),
        ] {
            std::fs::write(&p, text).unwrap();
            assert_eq!(load(&p).hold_seconds, want, "{text}");
        }
    }
}
