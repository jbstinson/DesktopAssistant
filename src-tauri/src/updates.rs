//! The update choices kept in `updates.json` in the data folder: whether to check
//! automatically (off by default) and when the last check finished. Pure rules only; the
//! network side is in updater.rs.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// How often an automatic check runs.
pub const DAY_SECS: u64 = 86_400;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateSettings {
    /// "Check automatically". Off unless the user ticks it.
    #[serde(default)]
    pub auto: bool,
    /// Unix seconds when the last check succeeded.
    #[serde(default)]
    pub last_check: Option<u64>,
}

/// Reads the saved choices. A missing, empty or broken file means off and never checked.
pub fn load(path: &Path) -> UpdateSettings {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Writes the choices atomically: temp file, then rename.
pub fn save(path: &Path, s: &UpdateSettings) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string(s)?)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// True when an automatic check should run now: auto is on, and there was no check in the last
/// day. A last check in the future (a clock moved back) counts as due, so checks can't stop.
pub fn is_due(s: &UpdateSettings, now: u64) -> bool {
    s.auto
        && match s.last_check {
            None => true,
            Some(t) => t > now || now - t >= DAY_SECS,
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("da-updates-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn missing_empty_or_broken_file_is_off() {
        let p = temp_dir("broken").join("updates.json");
        assert_eq!(load(&p), UpdateSettings::default());
        for text in ["", "not json", "[]", r#"{"auto":"yes"}"#] {
            std::fs::write(&p, text).unwrap();
            assert_eq!(load(&p), UpdateSettings::default(), "{text:?}");
        }
        assert!(!UpdateSettings::default().auto);
    }

    #[test]
    fn saves_and_loads() {
        let p = temp_dir("save").join("nested").join("updates.json");
        let s = UpdateSettings { auto: true, last_check: Some(1_700_000_000) };
        save(&p, &s).unwrap();
        assert_eq!(load(&p), s);
        assert!(!p.with_extension("json.tmp").exists());
    }

    #[test]
    fn not_due_when_auto_is_off() {
        assert!(!is_due(&UpdateSettings { auto: false, last_check: None }, 1_000_000));
        assert!(!is_due(&UpdateSettings { auto: false, last_check: Some(0) }, 1_000_000));
    }

    #[test]
    fn due_when_never_checked() {
        assert!(is_due(&UpdateSettings { auto: true, last_check: None }, 1_000_000));
    }

    #[test]
    fn due_after_a_day_not_before() {
        let now = 1_000_000;
        assert!(!is_due(&UpdateSettings { auto: true, last_check: Some(now - DAY_SECS + 1) }, now));
        assert!(is_due(&UpdateSettings { auto: true, last_check: Some(now - DAY_SECS) }, now));
    }

    #[test]
    fn due_when_last_check_is_in_the_future() {
        assert!(is_due(&UpdateSettings { auto: true, last_check: Some(2_000_000) }, 1_000_000));
    }
}
