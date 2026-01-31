//! Global keyboard event capture via CGEventTap.
//!
//! Uses active tap (Default): events can be swallowed so Cmd+V does not paste
//! until a slot is selected. Requires Accessibility permission.
//! Converts CG events to internal Event enum and sends over channel.

use crate::keys::{keycode_to_key, Key};
use crate::state_machine::{Event, MODE_COPY_SELECTING, MODE_IDLE, MODE_PASTE_SELECTING};
use core_foundation::runloop::CFRunLoop;
use core_graphics::event::{
    CGEvent, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
    CallbackResult,
};
use foreign_types::ForeignType;
use macos_accessibility_client::accessibility::application_is_trusted;
use std::sync::atomic::Ordering;
use std::sync::{atomic::AtomicU8, mpsc::Sender, Arc};
use tracing::{debug, error, info, warn};

// Minimal FFI for event field access (not fully exposed in core-graphics)
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventGetIntegerValueField(event: *const std::ffi::c_void, field: u32) -> i64;
    fn CGEventGetFlags(event: *const std::ffi::c_void) -> u64;
}

const K_CG_KEYBOARD_EVENT_KEYCODE: u32 = 9;
const CMD_MASK: u64 = 1 << 20;

/// Returns true if Accessibility permission is granted.
pub fn has_accessibility_permission() -> bool {
    application_is_trusted()
}

/// Runs the event tap and CFRunLoop. Sends events to `tx`. Blocks until the run loop stops.
/// `mode`: shared atomic 0=Idle, 1=CopyArmed, 2=CopySelecting, 3=PasteSelecting; used to decide swallow.
pub fn run_event_tap_with_sender(tx: Sender<Event>, mode: Arc<AtomicU8>) -> Result<(), String> {
    if !has_accessibility_permission() {
        error!(
            "Accessibility permission required for keyboard capture. Run `clip doctor` to fix."
        );
        return Err(
            "Accessibility permission not granted. Run `clip doctor` to open System Settings."
                .to_string(),
        );
    }

    let events_of_interest = vec![
        CGEventType::KeyDown,
        CGEventType::KeyUp,
        CGEventType::FlagsChanged,
    ];

    let tx = std::sync::Arc::new(tx);

    info!("Keyboard event tap active (can swallow events)");

    CGEventTap::with_enabled(
        CGEventTapLocation::HID,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::Default,
        events_of_interest,
        {
            let tx = tx.clone();
            let mode = mode.clone();
            move |_proxy, event_type, event| {
                let m = mode.load(Ordering::Acquire);
                if let Some((ev_opt, swallow)) = convert_event_and_swallow(event_type, event, m) {
                    if swallow {
                        if let Some(ev) = ev_opt {
                            let _ = tx.send(ev);
                        }
                        return CallbackResult::Drop;
                    }
                    if let Some(ev) = ev_opt {
                        let _ = tx.send(ev);
                    }
                }
                CallbackResult::Keep
            }
        },
        || CFRunLoop::run_current(),
    )
    .map_err(|_| {
        error!(
            "Failed to create event tap. Is Accessibility permission granted? Run `clip doctor`."
        );
        "Failed to create CGEventTap".to_string()
    })
}

/// Returns (event to send if any, true if should swallow). When swallow is true, callback returns Drop.
fn convert_event_and_swallow(
    event_type: CGEventType,
    event: &CGEvent,
    mode: u8,
) -> Option<(Option<Event>, bool)> {
    if matches!(event_type, CGEventType::TapDisabledByTimeout) {
        warn!("Event tap disabled by timeout; re-enabling");
        return None;
    }
    if matches!(event_type, CGEventType::TapDisabledByUserInput) {
        warn!("Event tap disabled by user input");
        return None;
    }

    let keycode = unsafe {
        CGEventGetIntegerValueField(event.as_ptr() as *const _, K_CG_KEYBOARD_EVENT_KEYCODE)
    };
    let flags = unsafe { CGEventGetFlags(event.as_ptr() as *const _) };

    let key = keycode_to_key(keycode);

    debug!("event_type={:?} keycode={} flags=0x{:x} mode={}", event_type, keycode, flags, mode);

    match event_type {
        CGEventType::KeyDown => {
            // Cmd+V in Idle: swallow and send CmdVTrigger so state machine enters PasteSelecting.
            if mode == MODE_IDLE && key == Key::V && (flags & CMD_MASK) != 0 {
                return Some((Some(Event::CmdVTrigger), true));
            }
            // CopySelecting or PasteSelecting: swallow slot keys and Esc so they don't type in app.
            if mode == MODE_COPY_SELECTING || mode == MODE_PASTE_SELECTING {
                if matches!(key, Key::Slot(_)) || key == Key::Escape {
                    if let Some(ev) = convert_event(event_type, event) {
                        return Some((Some(ev), true));
                    }
                }
            }
            if let Some(ev) = convert_event(event_type, event) {
                return Some((Some(ev), false));
            }
        }
        CGEventType::KeyUp => {
            // Swallow Cmd+V keyUp when in PasteSelecting for cleanliness (matching keyDown was swallowed).
            if mode == MODE_PASTE_SELECTING && key == Key::V && (flags & CMD_MASK) != 0 {
                return Some((None, true));
            }
            // Swallow keyUp for slot keys and Esc when selecting, so app doesn't see stray keyUp.
            if mode == MODE_COPY_SELECTING || mode == MODE_PASTE_SELECTING {
                if matches!(key, Key::Slot(_)) || key == Key::Escape {
                    return Some((None, true));
                }
            }
            if let Some(ev) = convert_event(event_type, event) {
                return Some((Some(ev), false));
            }
        }
        CGEventType::FlagsChanged => {
            if let Some(ev) = convert_event(event_type, event) {
                return Some((Some(ev), false));
            }
        }
        _ => {}
    }
    None
}

fn convert_event(event_type: CGEventType, event: &CGEvent) -> Option<Event> {
    let keycode = unsafe {
        CGEventGetIntegerValueField(event.as_ptr() as *const _, K_CG_KEYBOARD_EVENT_KEYCODE)
    };
    let flags = unsafe { CGEventGetFlags(event.as_ptr() as *const _) };
    let key = keycode_to_key(keycode);
    match event_type {
        CGEventType::KeyDown => Some(Event::KeyDown(key, flags)),
        CGEventType::KeyUp => Some(Event::KeyUp(key, flags)),
        CGEventType::FlagsChanged => Some(Event::FlagsChanged(flags)),
        _ => None,
    }
}
