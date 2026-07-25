//! Detection of native remote-desktop / screen-sharing clients.
//!
//! Handy runs on the local machine, but when a remote-desktop client window is
//! frontmost the keystrokes it sees belong to the *remote* host. Detecting these
//! clients lets `clipboard::paste` deliver the transcription by typing real
//! keystrokes (which the client forwards) instead of via the local clipboard,
//! whose shared-clipboard sync makes remote delivery unreliable (see `input.rs`).
//!
//! macOS-only: on other platforms the detector is a no-op returning `None`.

#[cfg(target_os = "macos")]
use log::info;

/// Which remote-desktop client is frontmost. Currently only used as a presence
/// check (`is_some()`); the variant is kept for future client-specific handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteClient {
    /// Apple's built-in Screen Sharing.app (`com.apple.ScreenSharing`).
    ScreenSharing,
    /// Any other detected remote-desktop client (VNC, RDP, AnyDesk, ...).
    Other,
}

#[cfg(target_os = "macos")]
const SCREEN_SHARING_BUNDLE_ID: &str = "com.apple.ScreenSharing";

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

/// Returns which remote-desktop client is frontmost, or `None` if the frontmost
/// app is not a recognized client. Uses a native NSWorkspace lookup, so it's
/// cheap enough to run on every paste when the optimization is enabled.
#[cfg(target_os = "macos")]
pub fn frontmost_remote_client() -> Option<RemoteClient> {
    match frontmost_bundle_id() {
        Some(id) => {
            let client = if id.eq_ignore_ascii_case(SCREEN_SHARING_BUNDLE_ID) {
                Some(RemoteClient::ScreenSharing)
            } else if is_remote_desktop_bundle_id(&id) {
                Some(RemoteClient::Other)
            } else {
                None
            };
            info!(
                "Remote-desktop detection: frontmost bundle id '{}', client: {:?}",
                id, client
            );
            client
        }
        None => {
            info!("Remote-desktop detection: could not read frontmost bundle id");
            None
        }
    }
}

/// Returns the frontmost application's bundle identifier via NSWorkspace.
///
/// This is a native, in-process call (sub-millisecond) rather than spawning
/// `osascript`, so enabling the remote-desktop optimization doesn't add
/// process-spawn latency to every paste — including local (non-remote) ones.
#[cfg(target_os = "macos")]
fn frontmost_bundle_id() -> Option<String> {
    use objc2_app_kit::NSWorkspace;

    let bundle_id = unsafe {
        let workspace = NSWorkspace::sharedWorkspace();
        let app = workspace.frontmostApplication()?;
        app.bundleIdentifier()?
    };
    Some(bundle_id.to_string())
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
pub fn frontmost_remote_client() -> Option<RemoteClient> {
    None
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
