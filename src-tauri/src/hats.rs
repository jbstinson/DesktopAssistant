//! Monthly hats: which months have their hat on, saved as `hats.json` in the data folder,
//! and the local month.
//!
//! `{"enabled": true, "off": [2, 10]}`: the Holidays switch, and the months (1-12) whose hat
//! is off. A missing or malformed file means on with every hat; a missing `enabled` (files
//! saved before the switch existed) means on; entries in `off` that are not whole numbers from
//! 1 to 12 are ignored.

use std::path::Path;

/// Whether each month's hat is on, January first.
pub type Months = [bool; 12];

pub const ALL_ON: Months = [true; 12];

/// The Holidays settings: the master switch and each month's tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Holidays {
    pub enabled: bool,
    pub months: Months,
}

impl Default for Holidays {
    fn default() -> Self {
        Holidays { enabled: true, months: ALL_ON }
    }
}

pub const NAMES: [&str; 12] = [
    "January", "February", "March", "April", "May", "June",
    "July", "August", "September", "October", "November", "December",
];

/// Reads the saved choices. Never fails: anything unreadable gives the default for that part.
pub fn load(path: &Path) -> Holidays {
    let value = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok());
    let field = |key: &str| value.as_ref().and_then(|v| v.get(key));
    let mut months = ALL_ON;
    let off = field("off").and_then(|v| v.as_array()).into_iter().flatten();
    for m in off.filter_map(|v| v.as_u64()).filter(|m| (1..=12).contains(m)) {
        months[m as usize - 1] = false;
    }
    Holidays { enabled: field("enabled").and_then(|v| v.as_bool()).unwrap_or(true), months }
}

/// Writes the choices atomically: temp file, then rename.
pub fn save(path: &Path, h: &Holidays) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let off: Vec<u8> = (1..=12).filter(|m| !h.months[*m as usize - 1]).collect();
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::json!({ "enabled": h.enabled, "off": off }).to_string())?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// The hat to show in `month` (1-12), or None when that month's hat is off.
pub fn shown(month: u8, on: &Months) -> Option<u8> {
    (1..=12).contains(&month).then_some(month).filter(|m| on[*m as usize - 1])
}

/// The month on this PC's local clock, 1-12.
pub fn local_month() -> u8 {
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    let mut t = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut t) };
    t.wMonth as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("da-hats-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn with_off(months: &[u8]) -> Months {
        let mut on = ALL_ON;
        for m in months {
            on[*m as usize - 1] = false;
        }
        on
    }

    #[test]
    fn shows_the_month_when_its_hat_is_on() {
        assert_eq!(shown(10, &ALL_ON), Some(10));
        assert_eq!(shown(1, &ALL_ON), Some(1));
        assert_eq!(shown(12, &ALL_ON), Some(12));
    }

    #[test]
    fn a_month_turned_off_shows_no_hat() {
        let on = with_off(&[10]);
        assert_eq!(shown(10, &on), None);
        assert_eq!(shown(11, &on), Some(11));
    }

    #[test]
    fn months_outside_1_to_12_show_no_hat() {
        assert_eq!(shown(0, &ALL_ON), None);
        assert_eq!(shown(13, &ALL_ON), None);
    }

    fn off(months: &[u8]) -> Holidays {
        Holidays { enabled: true, months: with_off(months) }
    }

    #[test]
    fn missing_file_is_all_on() {
        assert_eq!(load(&temp_dir("missing").join("hats.json")), Holidays::default());
        assert_eq!(Holidays::default(), Holidays { enabled: true, months: ALL_ON });
    }

    #[test]
    fn malformed_file_is_all_on() {
        let p = temp_dir("bad").join("hats.json");
        for text in ["", "{ bad", "[]", "null", r#"{"off": "10"}"#, r#"{"enabled": "no"}"#] {
            std::fs::write(&p, text).unwrap();
            assert_eq!(load(&p), Holidays::default(), "{text:?}");
        }
    }

    #[test]
    fn junk_entries_are_ignored() {
        let p = temp_dir("junk").join("hats.json");
        std::fs::write(&p, r#"{"off": [0, 13, "x", 10.5, 10]}"#).unwrap();
        assert_eq!(load(&p), off(&[10]));
    }

    #[test]
    fn a_file_from_before_the_switch_reads_as_enabled() {
        let p = temp_dir("old").join("hats.json");
        std::fs::write(&p, r#"{"off":[2]}"#).unwrap();
        assert_eq!(load(&p), off(&[2]));
    }

    #[test]
    fn disabled_keeps_the_month_choices() {
        let p = temp_dir("disabled").join("hats.json");
        std::fs::write(&p, r#"{"enabled": false, "off": [10]}"#).unwrap();
        assert_eq!(load(&p), Holidays { enabled: false, months: with_off(&[10]) });
    }

    #[test]
    fn saves_and_loads_including_a_new_folder() {
        let p = temp_dir("save").join("nested").join("hats.json");
        let h = Holidays { enabled: false, months: with_off(&[10, 2]) };
        save(&p, &h).unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), r#"{"enabled":false,"off":[2,10]}"#);
        assert_eq!(load(&p), h);
        assert!(!p.with_extension("json.tmp").exists());
    }

    #[test]
    #[ignore = "reads the system clock"]
    fn local_month_is_a_real_month() {
        assert!((1..=12).contains(&local_month()));
    }
}
