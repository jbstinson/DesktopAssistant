//! One app per data folder. A second launch would overwrite endpoint.json and leave the
//! first one deaf, so it must exit before it opens a window or a listener.

use std::path::Path;
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows_sys::Win32::System::Threading::CreateMutexW;

/// Holds the instance mutex; released when dropped or when the process ends.
pub struct Guard(HANDLE);

impl Drop for Guard {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

/// Mutex name for a data folder. Per folder, so a sandboxed copy with its own APPDATA does
/// not collide with the real one (they would not share an endpoint.json anyway).
pub fn mutex_name(data_dir: &Path) -> String {
    // FNV-1a over the lowercased path: stable, and no characters a mutex name dislikes.
    let mut h: u64 = 0xcbf29ce484222325;
    for b in data_dir.to_string_lossy().to_lowercase().replace('/', "\\").trim_end_matches('\\').bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("Local\\DesktopAssistant-{h:016x}")
}

/// Takes the mutex, or returns None when another instance already holds it.
pub fn acquire(name: &str) -> Option<Guard> {
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let h = CreateMutexW(std::ptr::null(), 0, wide.as_ptr());
        if h.is_null() {
            // Held by another user's session or otherwise unreachable: treat as running.
            return None;
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            CloseHandle(h);
            return None;
        }
        Some(Guard(h))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn second_acquire_fails_until_the_first_is_dropped() {
        let name = format!("Local\\DesktopAssistant-test-{}", std::process::id());
        let first = acquire(&name).expect("first instance");
        assert!(acquire(&name).is_none(), "second instance must be refused");
        assert!(acquire(&name).is_none(), "and again");
        drop(first);
        assert!(acquire(&name).is_some(), "free again after the first exits");
    }

    #[test]
    fn name_is_per_data_folder_and_ignores_case_and_slash_style() {
        let a = mutex_name(&PathBuf::from(r"C:\Users\x\AppData\Roaming\DesktopAssistant"));
        let b = mutex_name(&PathBuf::from(r"c:/users/X/appdata/roaming/desktopassistant/"));
        let c = mutex_name(&PathBuf::from(r"C:\Temp\sandbox\DesktopAssistant"));
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert!(a.starts_with("Local\\DesktopAssistant-"));
    }
}
