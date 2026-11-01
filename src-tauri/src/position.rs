//! Remembered window position: where to put the window at startup, and the small
//! state.json that stores it.

use std::path::Path;

/// A monitor (or its work area) in physical pixels on the virtual desktop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Top-left for the window at startup. `monitors[0]` is the primary. A saved position
/// stays where it is when the window mostly sits on a monitor, pulled fully onto that
/// monitor if it hangs over an edge; with no saved position, or none on any monitor, the
/// window goes to the primary's bottom-right corner.
pub fn restore(saved: Option<(i32, i32)>, monitors: &[Rect], size: (i32, i32)) -> (i32, i32) {
    let Some(primary) = monitors.first() else {
        return saved.unwrap_or((0, 0));
    };
    let corner = (primary.x + primary.w - size.0, primary.y + primary.h - size.1);
    let Some((x, y)) = saved else { return corner };
    // The monitor holding most of the window; the first wins a tie.
    let mut best: Option<(&Rect, i64)> = None;
    for m in monitors {
        let area = overlap(m, x, y, size);
        if area > 0 && best.is_none_or(|(_, a)| area > a) {
            best = Some((m, area));
        }
    }
    let Some((m, _)) = best else { return corner };
    // If the window is bigger than the monitor, the top-left edge wins.
    (x.min(m.x + m.w - size.0).max(m.x), y.min(m.y + m.h - size.1).max(m.y))
}

fn overlap(m: &Rect, x: i32, y: i32, size: (i32, i32)) -> i64 {
    let w = (x as i64 + size.0 as i64).min(m.x as i64 + m.w as i64) - (x as i64).max(m.x as i64);
    let h = (y as i64 + size.1 as i64).min(m.y as i64 + m.h as i64) - (y as i64).max(m.y as i64);
    if w > 0 && h > 0 { w * h } else { 0 }
}

/// Reads the saved position. A missing or malformed file counts as None.
pub fn load(path: &Path) -> Option<(i32, i32)> {
    let text = std::fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let coord = |key: &str| v.get(key)?.as_i64().and_then(|n| i32::try_from(n).ok());
    Some((coord("x")?, coord("y")?))
}

/// Writes the position atomically: temp file, then rename.
pub fn save(path: &Path, pos: (i32, i32)) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::json!({ "x": pos.0, "y": pos.1 }).to_string())?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: (i32, i32) = (200, 200);

    fn primary() -> Rect {
        Rect { x: 0, y: 0, w: 3840, h: 2160 }
    }
    /// Portrait monitor to the right of the primary, with its top above the primary's.
    fn portrait() -> Rect {
        Rect { x: 3840, y: -139, w: 1440, h: 2560 }
    }

    #[test]
    fn valid_position_is_kept() {
        assert_eq!(restore(Some((500, 400)), &[primary(), portrait()], SIZE), (500, 400));
    }

    #[test]
    fn negative_y_on_the_portrait_monitor_is_kept() {
        assert_eq!(restore(Some((3840, -139)), &[primary(), portrait()], SIZE), (3840, -139));
        assert_eq!(restore(Some((4000, -100)), &[primary(), portrait()], SIZE), (4000, -100));
    }

    #[test]
    fn missing_monitor_moves_to_the_primary() {
        // Saved on the portrait monitor, which is now unplugged.
        assert_eq!(restore(Some((4000, -100)), &[primary()], SIZE), (3840 - 200, 2160 - 200));
    }

    #[test]
    fn none_gives_the_primarys_bottom_right_corner() {
        assert_eq!(restore(None, &[primary(), portrait()], SIZE), (3840 - 200, 2160 - 200));
        let offset = Rect { x: -1920, y: 0, w: 1920, h: 1080 };
        assert_eq!(restore(None, &[offset], SIZE), (-200, 1080 - 200));
    }

    #[test]
    fn partly_off_screen_is_pulled_fully_on() {
        let m = [primary()];
        assert_eq!(restore(Some((-50, 100)), &m, SIZE), (0, 100));
        assert_eq!(restore(Some((100, -80)), &m, SIZE), (100, 0));
        assert_eq!(restore(Some((3800, 2100)), &m, SIZE), (3840 - 200, 2160 - 200));
    }

    #[test]
    fn straddling_two_monitors_goes_to_the_one_with_more_overlap() {
        let m = [primary(), portrait()];
        // 150 px on the primary, 50 px on the portrait monitor.
        assert_eq!(restore(Some((3690, 500)), &m, SIZE), (3640, 500));
        // 50 px on the primary, 150 px on the portrait monitor.
        assert_eq!(restore(Some((3790, 500)), &m, SIZE), (3840, 500));
    }

    #[test]
    fn no_monitors_keeps_the_saved_position() {
        assert_eq!(restore(Some((7, 9)), &[], SIZE), (7, 9));
        assert_eq!(restore(None, &[], SIZE), (0, 0));
    }

    #[test]
    fn window_bigger_than_the_monitor_lands_top_left() {
        let tiny = Rect { x: 10, y: 20, w: 100, h: 100 };
        assert_eq!(restore(Some((50, 50)), &[tiny], SIZE), (10, 20));
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("da-position-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn save_then_load_round_trips_including_negatives() {
        let dir = temp_dir("roundtrip");
        let path = dir.join("state.json");
        save(&path, (3840, -139)).unwrap();
        assert_eq!(load(&path), Some((3840, -139)));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), r#"{"x":3840,"y":-139}"#);
        assert!(!dir.join("state.json.tmp").exists());
        save(&path, (1, 2)).unwrap();
        assert_eq!(load(&path), Some((1, 2)));
    }

    #[test]
    fn save_creates_the_directory() {
        let dir = temp_dir("mkdir");
        let path = dir.join("nested").join("state.json");
        save(&path, (5, 6)).unwrap();
        assert_eq!(load(&path), Some((5, 6)));
    }

    #[test]
    fn missing_or_malformed_state_is_none() {
        let dir = temp_dir("bad");
        let path = dir.join("state.json");
        assert_eq!(load(&path), None);
        for bad in ["", "not json", "[]", r#"{"x":1}"#, r#"{"x":"a","y":2}"#, r#"{"x":1.5,"y":2}"#, r#"{"x":99999999999,"y":2}"#] {
            std::fs::write(&path, bad).unwrap();
            assert_eq!(load(&path), None, "input {bad:?}");
        }
    }
}
