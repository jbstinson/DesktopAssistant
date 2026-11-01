//! Queued sound and spoken alerts. One worker thread plays them one at a time.

use crate::rules::Alert;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{mpsc, Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

pub trait Output {
    /// Play a named sound ("alert" or "bell"); blocks until finished.
    fn play(&self, sound: &str);
    /// Speak a line; blocks until finished.
    fn say(&self, text: &str);
}

/// Most alerts kept waiting; beyond this the oldest is dropped.
const MAX_QUEUED: usize = 8;

#[derive(Default)]
struct State {
    queue: VecDeque<Alert>,
    quiet: bool,
    closed: bool,
}

type Shared = Arc<(Mutex<State>, Condvar)>;

pub struct Queue {
    shared: Shared,
}

fn lock(shared: &Shared) -> MutexGuard<'_, State> {
    shared.0.lock().unwrap_or_else(|e| e.into_inner())
}

impl Queue {
    pub fn new(output: impl Output + Send + 'static) -> Queue {
        let shared: Shared = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let worker = shared.clone();
        std::thread::spawn(move || run_worker(worker, output));
        Queue { shared }
    }

    /// Queue an alert. Silent alerts and anything pushed while quiet are ignored.
    pub fn push(&self, alert: Alert) {
        if alert.sound.is_none() && alert.speak.is_none() {
            return;
        }
        let mut st = lock(&self.shared);
        if st.quiet {
            return;
        }
        st.queue.push_back(alert);
        while st.queue.len() > MAX_QUEUED {
            st.queue.pop_front();
        }
        self.shared.1.notify_one();
    }

    /// Quiet drops everything still queued; an alert already playing finishes.
    // Task 9 wires do-not-disturb from the tray.
    #[allow(dead_code)]
    pub fn set_quiet(&self, quiet: bool) {
        let mut st = lock(&self.shared);
        st.quiet = quiet;
        if quiet {
            st.queue.clear();
        }
    }
}

impl Drop for Queue {
    fn drop(&mut self) {
        lock(&self.shared).closed = true;
        self.shared.1.notify_one();
    }
}

fn run_worker(shared: Shared, output: impl Output) {
    loop {
        let alert = {
            let mut st = lock(&shared);
            loop {
                if st.closed {
                    return;
                }
                if let Some(a) = st.queue.pop_front() {
                    // Collapse: drop queued repeats of what we are about to deliver.
                    st.queue.retain(|q| q.speak != a.speak || q.sound != a.sound);
                    break a;
                }
                st = shared.1.wait(st).unwrap_or_else(|e| e.into_inner());
            }
        };
        // Failures are logged and ignored; a bad sound must not stop the speech.
        if let Some(sound) = &alert.sound {
            let r = catch_unwind(AssertUnwindSafe(|| output.play(sound)));
            if r.is_err() {
                eprintln!("delivery: play panicked");
            }
        }
        if let Some(text) = &alert.speak {
            let r = catch_unwind(AssertUnwindSafe(|| output.say(text)));
            if r.is_err() {
                eprintln!("delivery: say panicked");
            }
        }
    }
}

static ALERT_WAV: &[u8] = include_bytes!("../sounds/alert.wav");
static BELL_WAV: &[u8] = include_bytes!("../sounds/bell.wav");

/// Longest we wait for one spoken line before giving up on it.
const SAY_TIMEOUT: Duration = Duration::from_secs(15);

/// Plays embedded WAVs with PlaySoundW and speaks through the Windows voices.
pub struct WindowsOutput;

thread_local! {
    // Created on first use, on the worker thread that speaks.
    static TTS: RefCell<Option<tts::Tts>> = const { RefCell::new(None) };
}

impl Output for WindowsOutput {
    fn play(&self, sound: &str) {
        use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_MEMORY, SND_NODEFAULT, SND_SYNC};
        let wav = match sound {
            "alert" => ALERT_WAV,
            "bell" => BELL_WAV,
            other => {
                eprintln!("delivery: unknown sound {other:?}");
                return;
            }
        };
        // SND_MEMORY reads from the static buffer; SND_SYNC blocks until done.
        let ok = unsafe {
            PlaySoundW(wav.as_ptr() as *const u16, std::ptr::null_mut(), SND_MEMORY | SND_SYNC | SND_NODEFAULT)
        };
        if ok == 0 {
            eprintln!("delivery: PlaySoundW failed for {sound:?}");
        }
    }

    fn say(&self, text: &str) {
        TTS.with(|cell| {
            let mut slot = cell.borrow_mut();
            if slot.is_none() {
                match tts::Tts::default() {
                    Ok(t) => *slot = Some(t),
                    Err(e) => {
                        eprintln!("delivery: speech unavailable: {e}");
                        return;
                    }
                }
            }
            let t = slot.as_mut().unwrap();
            let (tx, rx) = mpsc::channel::<()>();
            let tx_end = Mutex::new(tx.clone());
            let tx_stop = Mutex::new(tx);
            let _ = t.on_utterance_end(Some(Box::new(move |_| {
                let _ = tx_end.lock().map(|s| s.send(()));
            })));
            let _ = t.on_utterance_stop(Some(Box::new(move |_| {
                let _ = tx_stop.lock().map(|s| s.send(()));
            })));
            if let Err(e) = t.speak(text, false) {
                eprintln!("delivery: speak failed: {e}");
                return;
            }
            if rx.recv_timeout(SAY_TIMEOUT).is_err() {
                eprintln!("delivery: speech timed out, stopping it");
                let _ = t.stop();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::Tier;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    #[derive(Clone, Default)]
    struct Fake {
        log: Arc<Mutex<Vec<String>>>,
        delay_ms: u64,
    }
    impl Output for Fake {
        fn play(&self, sound: &str) {
            self.log.lock().unwrap().push(format!("play:{sound}:start"));
            std::thread::sleep(Duration::from_millis(self.delay_ms));
            self.log.lock().unwrap().push(format!("play:{sound}:end"));
        }
        fn say(&self, text: &str) {
            self.log.lock().unwrap().push(format!("say:{text}:start"));
            std::thread::sleep(Duration::from_millis(self.delay_ms));
            self.log.lock().unwrap().push(format!("say:{text}:end"));
        }
    }

    fn alert(sound: Option<&str>, speak: Option<&str>) -> Alert {
        Alert {
            tier: Tier::NeedsYou,
            sound: sound.map(String::from),
            speak: speak.map(String::from),
        }
    }

    fn wait_for(log: &Arc<Mutex<Vec<String>>>, n: usize) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while log.lock().unwrap().len() < n && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn settle() {
        std::thread::sleep(Duration::from_millis(300));
    }

    #[test]
    fn sound_plays_before_speech() {
        let fake = Fake::default();
        let log = fake.log.clone();
        let q = Queue::new(fake);
        q.push(alert(Some("alert"), Some("hello")));
        wait_for(&log, 4);
        assert_eq!(
            *log.lock().unwrap(),
            ["play:alert:start", "play:alert:end", "say:hello:start", "say:hello:end"]
        );
    }

    #[test]
    fn two_alerts_play_in_order_without_overlap() {
        let fake = Fake { delay_ms: 20, ..Default::default() };
        let log = fake.log.clone();
        let q = Queue::new(fake);
        q.push(alert(Some("alert"), Some("one")));
        q.push(alert(Some("bell"), Some("two")));
        wait_for(&log, 8);
        assert_eq!(
            *log.lock().unwrap(),
            [
                "play:alert:start", "play:alert:end", "say:one:start", "say:one:end",
                "play:bell:start", "play:bell:end", "say:two:start", "say:two:end",
            ]
        );
    }

    #[test]
    fn quiet_drops_queued_and_new_alerts_and_they_stay_dropped() {
        let fake = Fake { delay_ms: 100, ..Default::default() };
        let log = fake.log.clone();
        let q = Queue::new(fake);
        q.push(alert(None, Some("playing")));
        wait_for(&log, 1); // worker is mid-speech
        q.push(alert(None, Some("queued")));
        q.set_quiet(true);
        q.push(alert(None, Some("while quiet")));
        wait_for(&log, 2); // the one already playing finishes
        q.set_quiet(false);
        settle();
        assert_eq!(*log.lock().unwrap(), ["say:playing:start", "say:playing:end"]);
        // Works again after quiet ends.
        q.push(alert(None, Some("after")));
        wait_for(&log, 4);
        assert_eq!(log.lock().unwrap()[2], "say:after:start");
    }

    #[test]
    fn silent_alert_does_nothing() {
        let fake = Fake::default();
        let log = fake.log.clone();
        let q = Queue::new(fake);
        q.push(alert(None, None));
        settle();
        assert!(log.lock().unwrap().is_empty());
    }

    #[test]
    fn five_alerts_for_same_text_collapse_to_one() {
        let fake = Fake { delay_ms: 100, ..Default::default() };
        let log = fake.log.clone();
        let q = Queue::new(fake);
        q.push(alert(None, Some("blocker"))); // occupies the worker
        wait_for(&log, 1);
        for _ in 0..5 {
            q.push(alert(Some("alert"), Some("same")));
        }
        wait_for(&log, 6);
        settle();
        let said = log.lock().unwrap().iter().filter(|l| *l == "say:same:start").count();
        assert_eq!(said, 1);
    }

    #[test]
    fn queue_is_capped_dropping_oldest() {
        let fake = Fake { delay_ms: 100, ..Default::default() };
        let log = fake.log.clone();
        let q = Queue::new(fake);
        q.push(alert(None, Some("blocker")));
        wait_for(&log, 1);
        for i in 0..12 {
            q.push(alert(None, Some(&format!("m{i}"))));
        }
        wait_for(&log, 2 + 16);
        settle();
        let starts: Vec<String> = log
            .lock()
            .unwrap()
            .iter()
            .filter(|l| l.ends_with(":start"))
            .cloned()
            .collect();
        assert_eq!(starts.len(), 9); // blocker + the newest 8
        assert_eq!(starts[1], "say:m4:start");
        assert_eq!(starts[8], "say:m11:start");
    }

    /// Manual smoke check; makes audible sound. Run with:
    /// cargo test windows_output_speaks -- --ignored
    #[test]
    #[ignore]
    fn windows_output_speaks() {
        let out = WindowsOutput;
        out.play("alert");
        out.play("bell");
        out.say("Desktop assistant sound check.");
    }
}
