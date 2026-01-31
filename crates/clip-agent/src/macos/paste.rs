//! Paste from slot: set clipboard to slot text, synthesize Cmd+V, restore clipboard.

use core_graphics::event::{CGEvent, CGEventTapLocation};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use std::thread;
use std::time::Duration;
use tracing::warn;

use super::clipboard;

const RESTORE_DELAY_MS: u64 = 80;
const V_KEYCODE: u16 = 9; // ANSI_V

/// Pastes slot text: backup clipboard, set to slot text, post Cmd+V down/up, restore after delay.
/// Text-only; no UI. On restore failure logs "(clipboard replaced)" once.
pub fn paste_from_slot(slot_text: &str) {
    let backup = clipboard::read_text();

    if let Err(e) = clipboard::write_text(slot_text) {
        warn!("paste_from_slot: failed to set clipboard: {}", e);
        return;
    }

    let source = match CGEventSource::new(CGEventSourceStateID::CombinedSessionState) {
        Ok(s) => s,
        Err(()) => {
            warn!("paste_from_slot: CGEventSource create failed");
            return;
        }
    };

    let flags = core_graphics::event::CGEventFlags::CGEventFlagCommand;

    if let Ok(key_down) = CGEvent::new_keyboard_event(source.clone(), V_KEYCODE, true) {
        key_down.set_flags(flags);
        key_down.post(CGEventTapLocation::HID);
    }
    if let Ok(key_up) = CGEvent::new_keyboard_event(source, V_KEYCODE, false) {
        key_up.set_flags(flags);
        key_up.post(CGEventTapLocation::HID);
    }

    let backup_owned = backup.map(|s| s.to_string());
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(RESTORE_DELAY_MS));
        if let Some(prev) = backup_owned {
            if clipboard::write_text(&prev).is_err() {
                warn!("(clipboard replaced)");
            }
        }
    });
}
