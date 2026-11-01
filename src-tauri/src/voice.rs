//! Which installed Windows voice speaks alerts, and remembering that choice.

use std::path::Path;
use std::sync::{Arc, Mutex};

/// The chosen voice's name, shared with the speaking thread. None means the Windows default.
pub type Choice = Arc<Mutex<Option<String>>>;

/// The saved choice if that voice is still installed, otherwise None for the Windows default.
pub fn resolve(saved: Option<String>, installed: &[String]) -> Option<String> {
    saved.filter(|name| installed.contains(name))
}

/// What the tray ticks: the chosen voice, or the Windows default when there is no choice.
pub fn selected<'a>(choice: Option<&'a str>, default: Option<&'a str>) -> Option<&'a str> {
    choice.or(default)
}

/// The sample spoken when a voice is picked.
pub fn sample_text(name: &str) -> String {
    format!("Hello, this is {}.", name.strip_prefix("Microsoft ").unwrap_or(name))
}

/// Reads the saved voice name. A missing or empty file counts as None.
pub fn load(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    Some(text.trim().to_string()).filter(|name| !name.is_empty())
}

/// Writes the voice name atomically: temp file, then rename.
pub fn save(path: &Path, name: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("txt.tmp");
    std::fs::write(&tmp, name)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// Installed voice names and the Windows default. Runs on its own thread because the speech
/// engine is tied to the thread that creates it. Empty if speech is unavailable.
pub fn installed() -> (Vec<String>, Option<String>) {
    let listed = std::thread::spawn(|| {
        let t = tts::Tts::default().ok()?;
        let names = t.voices().ok()?.iter().map(|v| v.name()).collect::<Vec<_>>();
        let default = t.voice().ok().flatten().map(|v| v.name());
        Some((names, default))
    })
    .join();
    listed.ok().flatten().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn voices() -> Vec<String> {
        vec!["Microsoft David".to_string(), "Microsoft Zira".to_string()]
    }

    #[test]
    fn saved_voice_is_kept_while_installed() {
        assert_eq!(resolve(Some("Microsoft Zira".to_string()), &voices()).as_deref(), Some("Microsoft Zira"));
    }

    #[test]
    fn uninstalled_voice_falls_back_to_the_default() {
        assert_eq!(resolve(Some("Microsoft Mark".to_string()), &voices()), None);
        assert_eq!(resolve(None, &voices()), None);
    }

    #[test]
    fn tray_ticks_the_choice_or_else_the_default() {
        assert_eq!(selected(Some("Microsoft Zira"), Some("Microsoft David")), Some("Microsoft Zira"));
        assert_eq!(selected(None, Some("Microsoft David")), Some("Microsoft David"));
        assert_eq!(selected(None, None), None);
    }

    #[test]
    fn sample_uses_the_short_name() {
        assert_eq!(sample_text("Microsoft Zira"), "Hello, this is Zira.");
        assert_eq!(sample_text("Piper Amy"), "Hello, this is Piper Amy.");
    }

    #[test]
    fn saves_and_loads_including_a_new_folder() {
        let dir = std::env::temp_dir().join(format!("da-voice-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("nested").join("voice.txt");
        save(&path, "Microsoft Zira").unwrap();
        assert_eq!(load(&path).as_deref(), Some("Microsoft Zira"));
        assert!(!path.with_extension("txt.tmp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_or_blank_file_loads_as_none() {
        let dir = std::env::temp_dir().join(format!("da-voice-blank-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("voice.txt");
        assert_eq!(load(&path), None);
        std::fs::write(&path, "  \n").unwrap();
        assert_eq!(load(&path), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[ignore = "needs the Windows speech engine"]
    fn lists_installed_voices() {
        let (names, default) = installed();
        assert!(!names.is_empty());
        assert!(names.contains(&default.unwrap()));
    }
}
