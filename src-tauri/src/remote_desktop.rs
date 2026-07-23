//! Detection of native remote-desktop / screen-sharing clients.
//!
//! Handy runs on the local machine, but when a remote-desktop client window is
//! frontmost the keystrokes and clipboard it sees belong to the *remote* host.
//! Direct synthetic typing (CGEventKeyboardSetUnicodeString) is not forwarded
//! reliably, and a plain clipboard paste races the client's shared-clipboard
//! sync. Detecting these clients lets `clipboard::paste` switch to a paste
//! profile that survives the round-trip (see `clipboard.rs`).
//!
//! macOS-only: on other platforms the detector is a no-op returning `false`.

#[cfg(target_os = "macos")]
use log::{debug, warn};

/// Known bundle identifiers of native macOS remote-desktop / screen-sharing
/// clients. Matched case-insensitively and exactly.
#[cfg(target_os = "macos")]
const REMOTE_DESKTOP_BUNDLE_IDS: &[&str] = &[
    "com.apple.ScreenSharing",    // Screen Sharing.app (built-in)
    "com.apple.RemoteDesktop",    // Apple Remote Desktop
    "com.microsoft.rdc.macos",    // Microsoft Remote Desktop
    "com.microsoft.rdc.osx.beta", // Microsoft Remote Desktop (beta)
    "com.realvnc.vncviewer",      // RealVNC Viewer
    "com.teamviewer.TeamViewer",  // TeamViewer
    "com.anydesk.anydesk",        // AnyDesk
    "com.philandro.anydesk",      // AnyDesk (older bundle id)
    "com.parsecgaming.parsec",    // Parsec
    "com.p5sys.jump.mac.viewer",  // Jump Desktop
    "com.nulana.remotixmac",      // Remotix
    "com.splashtop.business-mac", // Splashtop Business
];

/// Substring heuristics for clients whose exact bundle id may vary by
/// version/channel. Matched case-insensitively against the lowercased id.
#[cfg(target_os = "macos")]
const REMOTE_DESKTOP_ID_SUBSTRINGS: &[&str] = &[
    "screensharing",
    "remotedesktop",
    "vncviewer",
    "teamviewer",
    "anydesk",
    "parsec",
    "jump.mac",
    "splashtop",
];

/// Returns `true` when the frontmost application is a known native
/// remote-desktop / screen-sharing client.
///
/// This shells out to `osascript` (like `correction_tracking`) and is only
/// called from the paste path when the user has enabled the optimization, so
/// the per-paste cost is paid only by remote-desktop users.
#[cfg(target_os = "macos")]
pub fn frontmost_app_is_remote_desktop() -> bool {
    match frontmost_bundle_id() {
        Some(id) => {
            let is_remote = is_remote_desktop_bundle_id(&id);
            debug!(
                "Frontmost app bundle id '{}' remote-desktop match: {}",
                id, is_remote
            );
            is_remote
        }
        None => false,
    }
}

#[cfg(target_os = "macos")]
fn frontmost_bundle_id() -> Option<String> {
    let script = [
        "tell application \"System Events\"",
        "try",
        "return bundle identifier of (first application process whose frontmost is true)",
        "on error",
        "return \"\"",
        "end try",
        "end tell",
    ];

    let mut command = std::process::Command::new("osascript");
    for line in script {
        command.arg("-e").arg(line);
    }

    let output = match command.output() {
        Ok(output) => output,
        Err(error) => {
            warn!(
                "Failed to run osascript for remote-desktop detection: {}",
                error
            );
            return None;
        }
    };

    if !output.status.success() {
        return None;
    }

    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

/// Pure matcher, kept separate so it can be unit-tested without a running WM.
#[cfg(target_os = "macos")]
fn is_remote_desktop_bundle_id(id: &str) -> bool {
    if REMOTE_DESKTOP_BUNDLE_IDS
        .iter()
        .any(|known| known.eq_ignore_ascii_case(id))
    {
        return true;
    }

    let lowered = id.to_ascii_lowercase();
    REMOTE_DESKTOP_ID_SUBSTRINGS
        .iter()
        .any(|needle| lowered.contains(needle))
}

/// Non-macOS platforms have no supported detection path yet.
#[cfg(not(target_os = "macos"))]
pub fn frontmost_app_is_remote_desktop() -> bool {
    false
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn matches_known_bundle_ids_case_insensitively() {
        assert!(is_remote_desktop_bundle_id("com.apple.ScreenSharing"));
        assert!(is_remote_desktop_bundle_id("COM.APPLE.SCREENSHARING"));
        assert!(is_remote_desktop_bundle_id("com.microsoft.rdc.macos"));
    }

    #[test]
    fn matches_substring_heuristics() {
        assert!(is_remote_desktop_bundle_id("com.example.AnyDesk.helper"));
        assert!(is_remote_desktop_bundle_id("org.vendor.parsec-preview"));
    }

    #[test]
    fn rejects_regular_apps() {
        assert!(!is_remote_desktop_bundle_id("com.apple.Safari"));
        assert!(!is_remote_desktop_bundle_id("com.microsoft.VSCode"));
        assert!(!is_remote_desktop_bundle_id(""));
    }
}
