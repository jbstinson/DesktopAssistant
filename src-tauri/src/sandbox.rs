//! Detects being started inside another app's MSIX (Store) container, for example from a
//! Claude desktop app session. Such a process inherits the package's AppData virtualization,
//! so endpoint.json lands in the package's private copy of APPDATA and hooks from VS Code or a
//! terminal never find this app.

use windows_sys::Win32::Foundation::{APPMODEL_ERROR_NO_PACKAGE, WIN32_ERROR};
use windows_sys::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;

pub const SANDBOX_WARNING: &str = "Started from inside another app's sandbox: sessions outside it \
can't reach the character. Quit and start DesktopAssistant from the Start menu or Explorer.";

/// The warning for a GetCurrentPackageFullName result. Only "no package" is plainly outside a
/// container; any other code (a buffer-size error included) means the process has a package
/// identity.
pub fn warning_for(code: WIN32_ERROR) -> Option<String> {
    (code != APPMODEL_ERROR_NO_PACKAGE).then(|| SANDBOX_WARNING.to_string())
}

/// Asks Windows whether this process runs with a package identity.
pub fn current_warning() -> Option<String> {
    let mut len: u32 = 0;
    // A zero-length buffer is enough: the error code alone says whether there is a package.
    let code = unsafe { GetCurrentPackageFullName(&mut len, std::ptr::null_mut()) };
    warning_for(code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS};

    #[test]
    fn no_package_means_no_warning() {
        assert_eq!(warning_for(APPMODEL_ERROR_NO_PACKAGE), None);
    }

    #[test]
    fn a_package_identity_shows_the_warning() {
        assert_eq!(warning_for(ERROR_INSUFFICIENT_BUFFER).as_deref(), Some(SANDBOX_WARNING));
        assert_eq!(warning_for(ERROR_SUCCESS).as_deref(), Some(SANDBOX_WARNING));
    }
}
