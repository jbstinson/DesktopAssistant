//! Detects being started inside another app's MSIX (Store) container, for example from a
//! Claude desktop app session. Such a process can inherit the package's AppData
//! virtualization, so endpoint.json lands in the package's private copy of APPDATA and hooks
//! from VS Code or a terminal never find this app.
//!
//! Two checks, either of which warns:
//! - the process has a package identity of its own (GetCurrentPackageFullName), and
//! - the endpoint.json this app just wrote turns up in a package's private copy of APPDATA.
//!   Processes started from a Claude desktop Code session have no package identity, yet
//!   their writes are still redirected, so this second check is the one that catches them.

use std::path::Path;
use windows_sys::Win32::Foundation::{APPMODEL_ERROR_NO_PACKAGE, WIN32_ERROR};
use windows_sys::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;

pub const SANDBOX_WARNING: &str = "Started from inside another app's sandbox: sessions outside it \
can't reach the character. Quit and start DesktopAssistant from the Start menu or Explorer.";

/// True if a GetCurrentPackageFullName result means the process has a package identity. Only
/// "no package" is plainly outside a container; any other code (a buffer-size error included)
/// means there is one.
pub fn has_package(code: WIN32_ERROR) -> bool {
    code != APPMODEL_ERROR_NO_PACKAGE
}

/// True if some package's private copy of APPDATA under `local_appdata` holds an endpoint.json
/// with this app's `port` and `pid`: the app's own write was redirected there. A copy left by an
/// earlier run has another port or pid and does not count.
pub fn endpoint_redirected(local_appdata: &Path, port: u16, pid: u32) -> bool {
    let Ok(packages) = std::fs::read_dir(local_appdata.join("Packages")) else { return false };
    packages.flatten().any(|package| {
        let file = package.path().join(r"LocalCache\Roaming\DesktopAssistant\endpoint.json");
        let Ok(text) = std::fs::read_to_string(file) else { return false };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return false };
        v.get("port").and_then(|p| p.as_u64()) == Some(u64::from(port))
            && v.get("pid").and_then(|p| p.as_u64()) == Some(u64::from(pid))
    })
}

/// Asks Windows whether this process runs with a package identity, and, once the listener has
/// written endpoint.json on `port`, whether that write was redirected into a package.
pub fn current_warning(port: Option<u16>) -> Option<String> {
    let mut len: u32 = 0;
    // A zero-length buffer is enough: the error code alone says whether there is a package.
    let code = unsafe { GetCurrentPackageFullName(&mut len, std::ptr::null_mut()) };
    let redirected = port.zip(std::env::var_os("LOCALAPPDATA")).is_some_and(|(port, local)| {
        endpoint_redirected(Path::new(&local), port, std::process::id())
    });
    (has_package(code) || redirected).then(|| SANDBOX_WARNING.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS};

    #[test]
    fn no_package_means_no_identity() {
        assert!(!has_package(APPMODEL_ERROR_NO_PACKAGE));
    }

    #[test]
    fn any_other_code_means_a_package_identity() {
        assert!(has_package(ERROR_INSUFFICIENT_BUFFER));
        assert!(has_package(ERROR_SUCCESS));
    }

    fn local_appdata(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("da-sandbox-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("Packages")).unwrap();
        d
    }

    fn redirected_endpoint(local: &Path, package: &str, text: &str) {
        let dir = local.join("Packages").join(package).join(r"LocalCache\Roaming\DesktopAssistant");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("endpoint.json"), text).unwrap();
    }

    #[test]
    fn our_endpoint_in_a_package_copy_is_redirected() {
        let local = local_appdata("ours");
        redirected_endpoint(&local, "Other_abc", r#"{"port":1,"token":"x","pid":2}"#);
        redirected_endpoint(&local, "Claude_pzs8sxrjxfjjc", r#"{"port":5000,"token":"t","pid":42}"#);
        assert!(endpoint_redirected(&local, 5000, 42));
    }

    #[test]
    fn a_stale_copy_from_another_run_is_not_redirected() {
        let local = local_appdata("stale");
        redirected_endpoint(&local, "Claude_pzs8sxrjxfjjc", r#"{"port":5000,"token":"t","pid":41}"#);
        assert!(!endpoint_redirected(&local, 5000, 42));
        assert!(!endpoint_redirected(&local, 5001, 41));
    }

    #[test]
    fn missing_or_unreadable_copies_are_not_redirected() {
        let local = local_appdata("missing");
        assert!(!endpoint_redirected(&local, 5000, 42));
        std::fs::create_dir_all(local.join(r"Packages\Empty_x\LocalCache")).unwrap();
        redirected_endpoint(&local, "Broken_y", "not json");
        assert!(!endpoint_redirected(&local, 5000, 42));
        assert!(!endpoint_redirected(&local.join("nowhere"), 5000, 42));
    }
}
