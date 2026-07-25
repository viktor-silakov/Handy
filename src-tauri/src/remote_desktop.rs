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
use log::{info, warn};

/// Which remote-desktop client is frontmost. The paste path treats Apple's
/// Screen Sharing specially because it exposes an explicit "Send Clipboard"
/// command that reliably pushes the local clipboard to the remote host.
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
/// app is not a recognized client.
///
/// This shells out to `osascript` (like `correction_tracking`) and is only
/// called from the paste path when the user has enabled the optimization, so
/// the per-paste cost is paid only by remote-desktop users.
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

/// Sets the local clipboard to `text` and immediately pushes it to the remote
/// host via Screen Sharing's Edit ▸ Send Clipboard command, both in a single
/// AppleScript. Returns `true` on success.
///
/// Doing both in one script is what makes this reliable with the automatic "Use
/// Shared Clipboard" sync ON: that sync continuously mirrors the remote host's
/// clipboard back onto the local one and would revert our write before we could
/// deliver it (the cause of the previous clipboard being pasted). Setting and
/// sending back-to-back leaves no window for the revert, and once the remote has
/// our text both sides match so nothing reverts. The text is passed as an argv
/// item (after `--`), so no escaping is needed and leading dashes are safe.
///
/// The menu item name is English-only here; on a localized system the click
/// fails and the caller falls back to the plain clipboard-delay strategy.
#[cfg(target_os = "macos")]
pub fn set_clipboard_and_send_to_screen_sharing(text: &str) -> bool {
    let script = "on run argv
set the clipboard to (item 1 of argv)
tell application \"System Events\"
tell process \"Screen Sharing\"
try
click menu item \"Send Clipboard\" of menu 1 of menu bar item \"Edit\" of menu bar 1
return \"ok\"
on error errMsg
return \"err:\" & errMsg
end try
end tell
end tell
end run";

    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .arg("--")
        .arg(text)
        .output();

    match output {
        Ok(output) => {
            let out = String::from_utf8_lossy(&output.stdout);
            let ok = output.status.success() && out.trim() == "ok";
            if !ok {
                warn!("Screen Sharing set+send clipboard failed: {}", out.trim());
            }
            ok
        }
        Err(error) => {
            warn!(
                "Failed to run osascript for Screen Sharing set+send clipboard: {}",
                error
            );
            false
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

/// Screen Sharing's "Send Clipboard" command is macOS-only.
#[cfg(not(target_os = "macos"))]
pub fn set_clipboard_and_send_to_screen_sharing(_text: &str) -> bool {
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
