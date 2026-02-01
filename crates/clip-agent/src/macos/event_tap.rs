//! Global keyboard event capture via CGEventTap.
//!
//! Only Cmd+Option+V is swallowed (paste chooser trigger). Cmd+V and Cmd+C pass through normally.
//! Requires Accessibility permission.

use crate::keys::{keycode_to_key, Key};
use crate::state_machine::Event;
use core_foundation::runloop::CFRunLoop;
use core_graphics::event::{
    CGEvent, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
    CallbackResult,
};
use foreign_types::ForeignType;
use macos_accessibility_client::accessibility::application_is_trusted;
use std::sync::mpsc::Sender;
use tracing::{debug, error, info, warn};

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventGetIntegerValueField(event: *const std::ffi::c_void, field: u32) -> i64;
    fn CGEventGetFlags(event: *const std::ffi::c_void) -> u64;
}

const K_CG_KEYBOARD_EVENT_KEYCODE: u32 = 9;
const CMD_MASK: u64 = 1 << 20;
const OPTION_MASK: u64 = 1 << 19;

pub fn has_accessibility_permission() -> bool {
    application_is_trusted()
}

/// Runs the event tap and CFRunLoop. Sends events to `tx`. Blocks until the run loop stops.
/// Only Cmd+Option+V is swallowed; all other keys pass through.
pub fn run_event_tap_with_sender(tx: Sender<Event>) -> Result<(), String> {
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

    info!("Keyboard event tap active (Cmd+Option+V = paste chooser, Cmd+V/Cmd+C normal)");

    CGEventTap::with_enabled(
        CGEventTapLocation::HID,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::Default,
        events_of_interest,
        {
            let tx = tx.clone();
            move |_proxy, event_type, event| {
                if let Some((ev_opt, swallow)) = convert_event_and_swallow(event_type, event) {
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

/// Returns (event to send if any, true if should swallow). Only Cmd+Option+V is swallowed.
fn convert_event_and_swallow(event_type: CGEventType, event: &CGEvent) -> Option<(Option<Event>, bool)> {
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

    debug!("event_type={:?} keycode={} flags=0x{:x}", event_type, keycode, flags);

    match event_type {
        CGEventType::KeyDown => {
            // Cmd+Option+V only: swallow and send CmdOptionVTrigger (paste chooser). Cmd+V and Cmd+C pass through.
            if key == Key::V && (flags & CMD_MASK) != 0 && (flags & OPTION_MASK) != 0 {
                return Some((Some(Event::CmdOptionVTrigger), true));
            }
            if let Some(ev) = convert_event(event_type, event) {
                return Some((Some(ev), false));
            }
        }
        CGEventType::KeyUp | CGEventType::FlagsChanged => {
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
