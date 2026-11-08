//! Which character is on screen, saved as `avatar.txt` (the avatar's id) in the data folder.

use std::path::Path;

/// Id and tray label of every avatar, in menu order. The first is the default.
pub const AVATARS: [(&str, &str); 5] = [("robot", "Robot"), ("dog", "Dog"), ("cat", "Cat"), ("paperclip", "Paperclip"), ("skeleton", "Skeleton")];

/// The avatar with this id, or the robot for anything unknown.
pub fn known(id: &str) -> &'static str {
    AVATARS.iter().find(|(a, _)| *a == id).map_or(AVATARS[0].0, |(a, _)| a)
}

/// Reads the saved avatar. A missing, empty or unknown one means the robot.
pub fn load(path: &Path) -> &'static str {
    known(std::fs::read_to_string(path).unwrap_or_default().trim())
}

/// Writes the avatar id atomically: temp file, then rename.
pub fn save(path: &Path, id: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("txt.tmp");
    std::fs::write(&tmp, id)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("da-avatar-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn known_ids_pass_and_anything_else_is_the_robot() {
        assert_eq!(known("dog"), "dog");
        assert_eq!(known("paperclip"), "paperclip");
        for id in ["", "Dog", "ultron", "robot "] {
            assert_eq!(known(id), "robot", "{id:?}");
        }
    }

    #[test]
    fn missing_file_is_the_robot() {
        assert_eq!(load(&temp_dir("missing").join("avatar.txt")), "robot");
    }

    #[test]
    fn saved_ids_are_trimmed_and_unknown_ones_are_the_robot() {
        let p = temp_dir("trim").join("avatar.txt");
        for (text, want) in [("cat\n", "cat"), ("  paperclip  ", "paperclip"), ("giant", "robot"), ("", "robot")] {
            std::fs::write(&p, text).unwrap();
            assert_eq!(load(&p), want, "{text:?}");
        }
    }

    #[test]
    fn saves_and_loads_including_a_new_folder() {
        let p = temp_dir("save").join("nested").join("avatar.txt");
        save(&p, "dog").unwrap();
        assert_eq!(load(&p), "dog");
        assert!(!p.with_extension("txt.tmp").exists());
    }
}
