//! Where the app keeps its data: %APPDATA%\DesktopAssistant.

use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
    let appdata = std::env::var_os("APPDATA").unwrap_or_default();
    PathBuf::from(appdata).join("DesktopAssistant")
}
