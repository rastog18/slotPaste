//! macOS clipboard access via NSPasteboard (using clipboard crate).
//!
//! Clipboard read with retry for "settle" delay after Cmd+C.

use clipboard::{ClipboardContext, ClipboardProvider};
use std::time::Duration;
use tracing::debug;

const RETRY_INTERVAL: Duration = Duration::from_millis(50);
const MAX_TRIES: u32 = 6;

/// Reads plain text from the clipboard with retry.
/// Handles timing: clipboard may update slightly after Cmd+C.
/// Retries up to ~300ms (6 tries * 50ms) before giving up.
///
/// Returns None if clipboard has no text or all retries fail.
pub fn read_text_with_retry(max_wait: Duration) -> Option<String> {
    let total_tries = (max_wait.as_millis() / RETRY_INTERVAL.as_millis()).max(1) as u32;
    let tries = total_tries.min(MAX_TRIES);

    for attempt in 0..tries {
        if attempt > 0 {
            std::thread::sleep(RETRY_INTERVAL);
        }

        if let Ok(mut ctx) = ClipboardContext::new() {
            if let Ok(contents) = ctx.get_contents() {
                let trimmed = contents.trim();
                if !trimmed.is_empty() {
                    debug!("Clipboard read succeeded on attempt {}", attempt + 1);
                    return Some(trimmed.to_string());
                }
            }
        }
    }

    None
}

/// Writes plain text to the clipboard.
/// Returns Ok(()) on success, Err on failure.
pub fn write_text(text: &str) -> Result<(), String> {
    let mut ctx = ClipboardContext::new().map_err(|e| e.to_string())?;
    ctx.set_contents(text.to_owned()).map_err(|e| e.to_string())?;
    Ok(())
}

/// Reads current clipboard as text. Returns None if empty or non-text.
pub fn read_text() -> Option<String> {
    let mut ctx = ClipboardContext::new().ok()?;
    let contents = ctx.get_contents().ok()?;
    let trimmed = contents.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}
