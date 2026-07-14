use crate::settings::get_settings;
use log::{debug, warn};
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

static TRACKING_SESSION_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Serialize)]
struct CorrectionSuggestionEvent {
    wrong: String,
    correct: String,
}

pub fn maybe_track_post_paste_edits(app: AppHandle, pasted_text: String, auto_submit: bool) {
    #[cfg(target_os = "macos")]
    {
        let settings = get_settings(&app);
        if auto_submit || !settings.track_input_correction_suggestions {
            return;
        }

        let pasted_text = pasted_text.trim().to_string();
        if pasted_text.is_empty() {
            return;
        }

        let session_id = TRACKING_SESSION_ID.fetch_add(1, Ordering::Relaxed) + 1;
        thread::spawn(move || track_post_paste_edits(app, session_id, pasted_text));
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, pasted_text, auto_submit);
    }
}

#[cfg(target_os = "macos")]
fn track_post_paste_edits(app: AppHandle, session_id: u64, pasted_text: String) {
    thread::sleep(Duration::from_millis(250));

    let Some(initial_value) = read_focused_input_value() else {
        return;
    };

    if !matches_pasted_snapshot(&initial_value, &pasted_text) {
        debug!("Skipping correction tracking because focused field did not match the pasted transcript");
        return;
    }

    for _ in 0..10 {
        if TRACKING_SESSION_ID.load(Ordering::Relaxed) != session_id {
            return;
        }

        thread::sleep(Duration::from_millis(500));

        let Some(current_value) = read_focused_input_value() else {
            continue;
        };

        if current_value.trim() == initial_value.trim() {
            continue;
        }

        let Some((wrong, correct)) =
            detect_single_word_replacement(&pasted_text, current_value.trim())
        else {
            continue;
        };

        let settings = get_settings(&app);
        if settings.correction_dictionary.iter().any(|entry| {
            entry.wrong.eq_ignore_ascii_case(&wrong) && entry.correct.eq_ignore_ascii_case(&correct)
        }) {
            return;
        }

        let _ = app.emit(
            "correction-suggestion",
            CorrectionSuggestionEvent { wrong, correct },
        );
        return;
    }
}

#[cfg(target_os = "macos")]
fn matches_pasted_snapshot(snapshot: &str, pasted_text: &str) -> bool {
    let snapshot = snapshot.trim();
    let pasted_text = pasted_text.trim();
    snapshot == pasted_text || snapshot.trim_end_matches(' ') == pasted_text
}

#[cfg(target_os = "macos")]
fn detect_single_word_replacement(original: &str, updated: &str) -> Option<(String, String)> {
    let original_words: Vec<&str> = original.split_whitespace().collect();
    let updated_words: Vec<&str> = updated.split_whitespace().collect();

    if original_words.len() != updated_words.len() || original_words.is_empty() {
        return None;
    }

    let mismatches: Vec<(String, String)> = original_words
        .iter()
        .zip(updated_words.iter())
        .filter_map(|(before, after)| {
            let before_normalized = normalize_observed_word(before);
            let after_normalized = normalize_observed_word(after);
            if before_normalized.is_empty()
                || after_normalized.is_empty()
                || before_normalized == after_normalized
            {
                None
            } else {
                Some((before_normalized, after_normalized))
            }
        })
        .collect();

    if mismatches.len() != 1 {
        return None;
    }

    let (wrong, correct) = mismatches.into_iter().next()?;
    if wrong == correct {
        return None;
    }

    Some((wrong, correct))
}

#[cfg(target_os = "macos")]
fn normalize_observed_word(word: &str) -> String {
    word.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'' && c != '-')
        .to_lowercase()
}

#[cfg(target_os = "macos")]
fn read_focused_input_value() -> Option<String> {
    let script = [
        "tell application \"System Events\"",
        "tell (first application process whose frontmost is true)",
        "try",
        "set focusedElement to value of attribute \"AXFocusedUIElement\"",
        "set elementValue to value of attribute \"AXValue\" of focusedElement",
        "if class of elementValue is list then return \"\"",
        "return elementValue as text",
        "on error",
        "return \"\"",
        "end try",
        "end tell",
        "end tell",
    ];

    let mut command = std::process::Command::new("osascript");
    for line in script {
        command.arg("-e").arg(line);
    }

    let output = match command.output() {
        Ok(output) => output,
        Err(error) => {
            warn!("Failed to run osascript for correction tracking: {}", error);
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
