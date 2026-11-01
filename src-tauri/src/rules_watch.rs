//! Reloads the alert rules file when it is saved.

use crate::rules::RuleSet;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};
use std::ffi::OsStr;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::thread::JoinHandle;
use std::time::Duration;

const DEBOUNCE: Duration = Duration::from_millis(200);
const RETRY_DELAY: Duration = Duration::from_millis(100);

/// Keeps watching while alive; dropping it stops the watch and joins the thread.
pub struct Watcher {
    notify: Option<RecommendedWatcher>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Watcher {
    fn drop(&mut self) {
        // Dropping the notify watcher drops the channel sender, which ends the thread.
        self.notify.take();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn is_relevant(ev: &notify::Result<Event>, name: &OsStr) -> bool {
    let Ok(ev) = ev else { return false };
    if matches!(ev.kind, EventKind::Access(_)) {
        return false;
    }
    ev.paths.iter().any(|p| {
        p.file_name()
            .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(&name.to_string_lossy()))
    })
}

fn load(path: &Path) -> Result<RuleSet, String> {
    let read = || std::fs::read_to_string(path);
    let text = match read() {
        Ok(t) => t,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            return Err(format!("rules file is missing: {}", path.display()))
        }
        Err(_) => {
            // An editor may briefly hold the file.
            std::thread::sleep(RETRY_DELAY);
            match read() {
                Ok(t) => t,
                Err(e) if e.kind() == ErrorKind::NotFound => {
                    return Err(format!("rules file is missing: {}", path.display()))
                }
                Err(e) => return Err(format!("cannot read rules file {}: {e}", path.display())),
            }
        }
    };
    RuleSet::parse(&text)
}

/// Calls `on_change` after the rules file is saved (200 ms debounce).
/// The caller keeps its last good set on `Err` and shows the message.
pub fn watch(
    path: PathBuf,
    on_change: impl Fn(Result<RuleSet, String>) + Send + 'static,
) -> Watcher {
    let dir = match path.parent() {
        Some(d) if !d.as_os_str().is_empty() => d.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let Some(name) = path.file_name().map(|n| n.to_owned()) else {
        on_change(Err(format!("invalid rules path: {}", path.display())));
        return Watcher { notify: None, thread: None };
    };
    let (tx, rx) = channel();
    let started = notify::recommended_watcher(move |ev| {
        let _ = tx.send(ev);
    })
    .and_then(|mut w| w.watch(&dir, RecursiveMode::NonRecursive).map(|_| w));
    let notify = match started {
        Ok(w) => w,
        Err(e) => {
            on_change(Err(format!("cannot watch rules file: {e}")));
            return Watcher { notify: None, thread: None };
        }
    };
    let thread = std::thread::spawn(move || {
        // Blocks with no timeout until an event arrives; exits when the watcher is dropped.
        while let Ok(ev) = rx.recv() {
            if !is_relevant(&ev, &name) {
                continue;
            }
            // Debounce: keep absorbing events until the file is quiet for DEBOUNCE.
            loop {
                match rx.recv_timeout(DEBOUNCE) {
                    Ok(_) => {}
                    Err(RecvTimeoutError::Timeout) => break,
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
            on_change(load(&path));
        }
    });
    Watcher { notify: Some(notify), thread: Some(thread) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::mpsc::{channel, Receiver};
    use std::time::Duration;

    const VALID: &str = include_str!("default_rules.toml");
    const INVALID: &str = "this is [not valid";

    struct Fixture {
        dir: PathBuf,
        file: PathBuf,
        rx: Receiver<Result<RuleSet, String>>,
        _w: Watcher,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    fn fixture(name: &str) -> Fixture {
        let dir = std::env::temp_dir().join(format!("da-rules-watch-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("rules.toml");
        fs::write(&file, VALID).unwrap();
        let (tx, rx) = channel();
        let w = watch(file.clone(), move |r| {
            let _ = tx.send(r);
        });
        // Let the OS watch register before the test touches the file.
        std::thread::sleep(Duration::from_millis(150));
        Fixture { dir, file, rx, _w: w }
    }

    const WAIT: Duration = Duration::from_secs(2);

    #[test]
    fn valid_save_gives_ok() {
        let f = fixture("ok");
        fs::write(&f.file, VALID).unwrap();
        assert!(f.rx.recv_timeout(WAIT).unwrap().is_ok());
    }

    #[test]
    fn invalid_save_gives_err() {
        let f = fixture("err");
        fs::write(&f.file, INVALID).unwrap();
        assert!(f.rx.recv_timeout(WAIT).unwrap().is_err());
    }

    #[test]
    fn rename_over_is_detected() {
        let f = fixture("rename");
        let tmp = f.dir.join("rules.toml.tmp");
        fs::write(&tmp, INVALID).unwrap();
        fs::rename(&tmp, &f.file).unwrap();
        assert!(f.rx.recv_timeout(WAIT).unwrap().is_err());
    }

    #[test]
    fn burst_gives_one_callback() {
        let f = fixture("burst");
        for _ in 0..3 {
            fs::write(&f.file, VALID).unwrap();
        }
        assert!(f.rx.recv_timeout(WAIT).unwrap().is_ok());
        assert!(f.rx.recv_timeout(Duration::from_millis(600)).is_err());
    }

    #[test]
    fn other_files_in_folder_are_ignored() {
        let f = fixture("other");
        fs::write(f.dir.join("other.toml"), VALID).unwrap();
        assert!(f.rx.recv_timeout(Duration::from_millis(600)).is_err());
    }

    #[test]
    fn file_name_match_is_case_insensitive() {
        let f = fixture("case");
        // Same file, addressed through a differently cased name.
        fs::write(f.dir.join("RULES.TOML"), INVALID).unwrap();
        assert!(f.rx.recv_timeout(WAIT).unwrap().is_err());
    }

    #[test]
    fn deleted_file_reports_missing_and_later_save_recovers() {
        let f = fixture("delete");
        fs::remove_file(&f.file).unwrap();
        let err = f.rx.recv_timeout(WAIT).unwrap().unwrap_err();
        assert!(err.contains("missing"), "{err}");
        fs::write(&f.file, VALID).unwrap();
        assert!(f.rx.recv_timeout(WAIT).unwrap().is_ok());
    }

    #[test]
    fn dropping_watcher_stops_callbacks() {
        let dir = std::env::temp_dir().join(format!("da-rules-watch-drop-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("rules.toml");
        fs::write(&file, VALID).unwrap();
        let (tx, rx) = channel();
        let w = watch(file.clone(), move |r| {
            let _ = tx.send(r);
        });
        std::thread::sleep(Duration::from_millis(150));
        drop(w);
        fs::write(&file, VALID).unwrap();
        assert!(rx.recv_timeout(Duration::from_millis(600)).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
