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
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Id of the synthetic "Shared Whisper Server" model in the ModelManager
/// catalog. Selecting it routes transcription to the fixed local server.
pub const SHARED_WHISPER_MODEL_ID: &str = "shared-whisper";

/// Fixed base URL of the shared server. Part of the external contract;
/// deliberately not read from settings.
pub const SHARED_WHISPER_SERVER_URL: &str = "http://127.0.0.1:8737";

/// Quick health-check timeout — short so a bootstrap probe never stalls
/// anything user-visible for long.
const HEALTH_CHECK_TIMEOUT: Duration = Duration::from_millis(1500);

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

/// Single-flight guard so overlapping model loads spawn at most one bootstrap.
static BOOTSTRAP_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

/// Ensure the shared server is up without blocking the caller.
///
/// Spawns a background thread that health-checks the server and, when it does
/// not respond, runs `npx -y shared-whisper-server ensure` (idempotent
/// install + start; the first run can take minutes). Never fails the caller:
/// when npx cannot be found this only logs a warning, and transcription then
/// surfaces a clear "server not reachable" error instead.
pub fn ensure_server_running() {
    if BOOTSTRAP_IN_FLIGHT.swap(true, Ordering::SeqCst) {
        info!("Shared whisper server bootstrap already in flight; skipping");
        return;
    }

    std::thread::spawn(|| {
        // Reset the single-flight flag even if the bootstrap panics.
        struct FlagReset;
        impl Drop for FlagReset {
            fn drop(&mut self) {
                BOOTSTRAP_IN_FLIGHT.store(false, Ordering::SeqCst);
            }
        }
        let _reset = FlagReset;

        bootstrap_server();
    });
}

fn bootstrap_server() {
    if is_server_healthy(SHARED_WHISPER_SERVER_URL, HEALTH_CHECK_TIMEOUT) {
        info!(
            "Shared whisper server is already healthy at {}",
            SHARED_WHISPER_SERVER_URL
        );
        return;
    }

    let Some(npx) = find_npx() else {
        warn!(
            "Shared whisper server is not running at {} and npx was not found \
             (checked PATH, /usr/local/bin, /opt/homebrew/bin, \
             ~/.nvm/versions/node/*/bin). Install Node.js or start the server \
             manually with `npx -y shared-whisper-server ensure`.",
            SHARED_WHISPER_SERVER_URL
        );
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
        }
        Ok(output) => {
            warn!(
                "Shared whisper server bootstrap exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Err(e) => {
            warn!("Failed to run shared whisper server bootstrap: {}", e);
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
