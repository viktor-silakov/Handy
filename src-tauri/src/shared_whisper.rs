//! Bootstrap and constants for the shared local whisper server backing the
//! synthetic "shared-whisper" model.
//!
//! The server is an external component with a fixed contract:
//! - base URL `http://127.0.0.1:8737` — never user-configurable, no auth token
//!   (the user's `remote_server_url`/`remote_server_token` settings are ignored)
//! - `POST /transcribe` with a raw 16 kHz mono WAV body returns `{"text": "..."}`
//!   (exactly what [`crate::managers::transcription::RemoteEngine`] speaks)
//! - `GET /health` answers 2xx JSON once the server is ready
//! - `npx -y shared-whisper-server ensure` idempotently installs (if missing)
//!   and starts it via a launchd agent; a first install can take minutes
//!   because it downloads a ~1.6 GB model.

use log::{info, warn};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// Status of the shared whisper server.
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct SharedWhisperStatusInfo {
    /// One of "ready", "installing", "uninstalled", "error"
    pub status: String,
    pub error: Option<String>,
}

/// Id of the synthetic "Shared Whisper Server" model in the ModelManager
/// catalog. Selecting it routes transcription to the fixed local server.
pub const SHARED_WHISPER_MODEL_ID: &str = "shared-whisper";

/// Fixed base URL of the shared server. Part of the external contract;
/// deliberately not read from settings.
pub const SHARED_WHISPER_SERVER_URL: &str = "http://127.0.0.1:8737";

/// Quick health-check timeout — short so a bootstrap probe never stalls
/// anything user-visible for long.
const HEALTH_CHECK_TIMEOUT: Duration = Duration::from_millis(1500);

static BOOTSTRAP_IN_FLIGHT: AtomicBool = AtomicBool::new(false);
static LAST_BOOTSTRAP_ERROR: Mutex<Option<String>> = Mutex::new(None);

/// Returns true if the background server installation/bootstrap is currently running.
pub fn is_bootstrap_in_flight() -> bool {
    BOOTSTRAP_IN_FLIGHT.load(Ordering::SeqCst)
}

/// Returns the current health and installation status of the shared whisper server.
pub fn get_status() -> SharedWhisperStatusInfo {
    if is_server_healthy(SHARED_WHISPER_SERVER_URL, HEALTH_CHECK_TIMEOUT) {
        SharedWhisperStatusInfo {
            status: "ready".to_string(),
            error: None,
        }
    } else if BOOTSTRAP_IN_FLIGHT.load(Ordering::SeqCst) {
        SharedWhisperStatusInfo {
            status: "installing".to_string(),
            error: None,
        }
    } else if let Some(err) = LAST_BOOTSTRAP_ERROR.lock().unwrap().clone() {
        SharedWhisperStatusInfo {
            status: "error".to_string(),
            error: Some(err),
        }
    } else {
        SharedWhisperStatusInfo {
            status: "uninstalled".to_string(),
            error: None,
        }
    }
}

/// True when `GET {base_url}/health` answers a 2xx status within `timeout`.
pub fn is_server_healthy(base_url: &str, timeout: Duration) -> bool {
    let url = format!("{}/health", base_url.trim_end_matches('/'));
    tauri::async_runtime::block_on(async move {
        let client = match reqwest::Client::builder().timeout(timeout).build() {
            Ok(client) => client,
            Err(_) => return false,
        };
        match client.get(&url).send().await {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    })
}

/// Fire-and-forget model preload on the shared server.
///
/// Called the moment recording starts so the server's ~10 s cold model load
/// overlaps with the user speaking instead of delaying the transcription
/// after they release the key. No-op for other models; errors are ignored —
/// the transcription request itself will surface them.
pub fn warmup_async(model_id: &str) {
    if model_id != SHARED_WHISPER_MODEL_ID {
        return;
    }
    std::thread::spawn(|| {
        let url = format!("{}/warmup", SHARED_WHISPER_SERVER_URL);
        let _ = tauri::async_runtime::block_on(async move {
            let client = reqwest::Client::builder()
                .timeout(HEALTH_CHECK_TIMEOUT)
                .build()
                .ok()?;
            client.post(&url).send().await.ok()
        });
    });
}

/// Ensure the shared server is up without blocking the caller.
///
/// Spawns a background thread that health-checks the server and, when it does
/// not respond, runs `npx -y shared-whisper-server ensure` (idempotent
/// install + start; the first run can take minutes). Emits status events
/// to the frontend when an `AppHandle` is provided.
pub fn ensure_server_running(app_handle: Option<AppHandle>) {
    if is_server_healthy(SHARED_WHISPER_SERVER_URL, HEALTH_CHECK_TIMEOUT) {
        info!(
            "Shared whisper server is already healthy at {}",
            SHARED_WHISPER_SERVER_URL
        );
        if let Some(ref app) = app_handle {
            let _ = app.emit(
                "shared-whisper-status",
                SharedWhisperStatusInfo {
                    status: "ready".to_string(),
                    error: None,
                },
            );
        }
        return;
    }

    if BOOTSTRAP_IN_FLIGHT.swap(true, Ordering::SeqCst) {
        info!("Shared whisper server bootstrap already in flight; skipping");
        return;
    }

    if let Some(ref app) = app_handle {
        let _ = app.emit(
            "shared-whisper-status",
            SharedWhisperStatusInfo {
                status: "installing".to_string(),
                error: None,
            },
        );
    }

    std::thread::spawn(move || {
        // Reset the single-flight flag even if the bootstrap panics.
        struct FlagReset;
        impl Drop for FlagReset {
            fn drop(&mut self) {
                BOOTSTRAP_IN_FLIGHT.store(false, Ordering::SeqCst);
            }
        }
        let _reset = FlagReset;

        bootstrap_server(app_handle);
    });
}

fn bootstrap_server(app_handle: Option<AppHandle>) {
    if is_server_healthy(SHARED_WHISPER_SERVER_URL, HEALTH_CHECK_TIMEOUT) {
        info!(
            "Shared whisper server is already healthy at {}",
            SHARED_WHISPER_SERVER_URL
        );
        if let Some(ref app) = app_handle {
            let _ = app.emit(
                "shared-whisper-status",
                SharedWhisperStatusInfo {
                    status: "ready".to_string(),
                    error: None,
                },
            );
        }
        return;
    }

    let Some(npx) = find_npx() else {
        let err_msg = "Shared whisper server is not running and npx was not found \
             (checked PATH, /usr/local/bin, /opt/homebrew/bin, \
             ~/.nvm/versions/node/*/bin). Install Node.js or start the server \
             manually with `npx -y shared-whisper-server ensure`."
            .to_string();
        warn!("{}", err_msg);
        *LAST_BOOTSTRAP_ERROR.lock().unwrap() = Some(err_msg.clone());
        if let Some(ref app) = app_handle {
            let _ = app.emit(
                "shared-whisper-status",
                SharedWhisperStatusInfo {
                    status: "error".to_string(),
                    error: Some(err_msg),
                },
            );
        }
        return;
    };

    info!(
        "Shared whisper server not responding; running `{} -y shared-whisper-server ensure` \
         (a first install downloads a ~1.6 GB model and can take minutes)",
        npx.display()
    );

    let mut command = Command::new(&npx);
    command.args(["-y", "shared-whisper-server", "ensure"]);

    // npx needs `node` next to it on PATH; prepend its own directory so a
    // binary found outside the (GUI-minimal) inherited PATH still works.
    if let Some(bin_dir) = npx.parent() {
        let mut paths: Vec<PathBuf> = vec![bin_dir.to_path_buf()];
        if let Some(path_var) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&path_var));
        }
        if let Ok(joined) = std::env::join_paths(paths) {
            command.env("PATH", joined);
        }
    }

    match command.output() {
        Ok(output) if output.status.success() => {
            info!("Shared whisper server bootstrap completed successfully");
            *LAST_BOOTSTRAP_ERROR.lock().unwrap() = None;
            if let Some(ref app) = app_handle {
                let _ = app.emit(
                    "shared-whisper-status",
                    SharedWhisperStatusInfo {
                        status: "ready".to_string(),
                        error: None,
                    },
                );
                let _ = app.emit("model-download-complete", SHARED_WHISPER_MODEL_ID);
            }
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let err_msg = format!(
                "Shared whisper server bootstrap exited with {}: {}",
                output.status, stderr
            );
            warn!("{}", err_msg);
            *LAST_BOOTSTRAP_ERROR.lock().unwrap() = Some(err_msg.clone());
            if let Some(ref app) = app_handle {
                let _ = app.emit(
                    "shared-whisper-status",
                    SharedWhisperStatusInfo {
                        status: "error".to_string(),
                        error: Some(err_msg),
                    },
                );
            }
        }
        Err(e) => {
            let err_msg = format!("Failed to run shared whisper server bootstrap: {}", e);
            warn!("{}", err_msg);
            *LAST_BOOTSTRAP_ERROR.lock().unwrap() = Some(err_msg.clone());
            if let Some(ref app) = app_handle {
                let _ = app.emit(
                    "shared-whisper-status",
                    SharedWhisperStatusInfo {
                        status: "error".to_string(),
                        error: Some(err_msg),
                    },
                );
            }
        }
    }
}

fn npx_binary_names() -> &'static [&'static str] {
    if cfg!(windows) {
        &["npx.cmd", "npx.exe", "npx"]
    } else {
        &["npx"]
    }
}

/// Directories to probe for npx, in priority order: the inherited PATH first,
/// then the usual macOS install locations — GUI apps on macOS get a minimal
/// PATH (`/usr/bin:/bin:/usr/sbin:/sbin`) that misses Homebrew and nvm.
fn candidate_npx_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();

    if let Some(path_var) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path_var));
    }

    dirs.push(PathBuf::from("/usr/local/bin"));
    dirs.push(PathBuf::from("/opt/homebrew/bin"));

    // nvm installs: ~/.nvm/versions/node/<version>/bin. Any working npx is
    // fine, so a simple descending lexicographic sort (newest-ish first) is
    // good enough — no need for semver-exact ordering.
    if let Some(home) = std::env::var_os("HOME") {
        let nvm_versions = PathBuf::from(home).join(".nvm/versions/node");
        if let Ok(entries) = std::fs::read_dir(&nvm_versions) {
            let mut versions: Vec<PathBuf> =
                entries.flatten().map(|e| e.path().join("bin")).collect();
            versions.sort();
            versions.reverse();
            dirs.extend(versions);
        }
    }

    dirs
}

/// Find an npx executable, tolerating the minimal PATH of macOS GUI apps.
fn find_npx() -> Option<PathBuf> {
    for dir in candidate_npx_dirs() {
        for name in npx_binary_names() {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// Answer one HTTP request on an ephemeral port with the given status
    /// line, then close. Returns the base URL to hit.
    fn spawn_one_shot_http_server(status_line: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let addr = listener.local_addr().expect("mock server addr");
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf);
                let body = r#"{"status":"ok"}"#;
                let response = format!(
                    "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    status_line,
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        format!("http://{}", addr)
    }

    #[test]
    fn health_check_accepts_2xx() {
        let url = spawn_one_shot_http_server("200 OK");
        assert!(is_server_healthy(&url, Duration::from_secs(2)));
    }

    #[test]
    fn health_check_rejects_server_error() {
        let url = spawn_one_shot_http_server("500 Internal Server Error");
        assert!(!is_server_healthy(&url, Duration::from_secs(2)));
    }

    #[test]
    fn health_check_fails_fast_when_nothing_listens() {
        // Bind then drop to get a port that is almost certainly closed.
        let port = {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap().port()
        };
        let url = format!("http://127.0.0.1:{}", port);
        assert!(!is_server_healthy(&url, Duration::from_millis(500)));
    }

    #[test]
    fn candidate_dirs_include_common_macos_locations() {
        let dirs = candidate_npx_dirs();
        assert!(dirs.contains(&PathBuf::from("/usr/local/bin")));
        assert!(dirs.contains(&PathBuf::from("/opt/homebrew/bin")));
    }
}
