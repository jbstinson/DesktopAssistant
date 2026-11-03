//! Monthly hats: which months have their hat on, saved as `hats.json` in the data folder,
//! and the local month.
//!
//! `{"off": [2, 10]}` lists the months (1-12) whose hat is off. A missing or malformed file
//! means every hat is on; entries that are not whole numbers from 1 to 12 are ignored.

use std::path::Path;

/// Whether each month's hat is on, January first.
pub type Months = [bool; 12];

pub const ALL_ON: Months = [true; 12];

pub const NAMES: [&str; 12] = [
    "January", "February", "March", "April", "May", "June",
    "July", "August", "September", "October", "November", "December",
];

/// Reads the saved choices. Never fails: anything unreadable means all on.
pub fn load(path: &Path) -> Months {
    let value = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok());
    let Some(off) = value.as_ref().and_then(|v| v.get("off")).and_then(|v| v.as_array()) else {
        return ALL_ON;
    };
    let mut on = ALL_ON;
    for m in off.iter().filter_map(|v| v.as_u64()).filter(|m| (1..=12).contains(m)) {
        on[m as usize - 1] = false;
    }
    on
}

/// Writes the choices atomically: temp file, then rename.
pub fn save(path: &Path, on: &Months) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let off: Vec<u8> = (1..=12).filter(|m| !on[*m as usize - 1]).collect();
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::json!({ "off": off }).to_string())?;
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

    #[test]
    fn missing_file_is_all_on() {
        assert_eq!(load(&temp_dir("missing").join("hats.json")), ALL_ON);
    }

    #[test]
    fn malformed_file_is_all_on() {
        let p = temp_dir("bad").join("hats.json");
        for text in ["", "{ bad", "[]", "null", r#"{"off": "10"}"#] {
            std::fs::write(&p, text).unwrap();
            assert_eq!(load(&p), ALL_ON, "{text:?}");
        }
    }

    #[test]
    fn junk_entries_are_ignored() {
        let p = temp_dir("junk").join("hats.json");
        std::fs::write(&p, r#"{"off": [0, 13, "x", 10.5, 10]}"#).unwrap();
        assert_eq!(load(&p), with_off(&[10]));
    }

    #[test]
    fn saves_and_loads_including_a_new_folder() {
        let p = temp_dir("save").join("nested").join("hats.json");
        let on = with_off(&[10, 2]);
        save(&p, &on).unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), r#"{"off":[2,10]}"#);
        assert_eq!(load(&p), on);
        assert!(!p.with_extension("json.tmp").exists());
    }

    #[test]
    #[ignore = "reads the system clock"]
    fn local_month_is_a_real_month() {
        assert!((1..=12).contains(&local_month()));
    }
}
