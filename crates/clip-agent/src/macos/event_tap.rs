//! Global keyboard event capture via CGEventTap.
//!
//! Uses listen-only mode: events are observed but not modified or blocked.
//! Requires Accessibility permission.
//! Converts CG events to internal Event enum and sends over channel.

use crate::keys::keycode_to_key;
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

// Minimal FFI for event field access (not fully exposed in core-graphics)
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventGetIntegerValueField(event: *const std::ffi::c_void, field: u32) -> i64;
    fn CGEventGetFlags(event: *const std::ffi::c_void) -> u64;
}

const K_CG_KEYBOARD_EVENT_KEYCODE: u32 = 9;

/// Returns true if Accessibility permission is granted.
pub fn has_accessibility_permission() -> bool {
    application_is_trusted()
}

/// Runs the event tap and CFRunLoop. Sends events to `tx`. Blocks until the run loop stops.
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

    info!("Keyboard event tap active (listen-only)");

    CGEventTap::with_enabled(
        CGEventTapLocation::HID,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::ListenOnly,
        events_of_interest,
        {
            let tx = tx.clone();
            move |_proxy, event_type, event| {
                if let Some(ev) = convert_event(event_type, event) {
                    if tx.send(ev).is_err() {
                        debug!("Channel closed, event tap stopping");
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

fn convert_event(event_type: CGEventType, event: &CGEvent) -> Option<Event> {
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
        CGEventType::KeyDown => Some(Event::KeyDown(key, flags)),
        CGEventType::KeyUp => Some(Event::KeyUp(key, flags)),
        CGEventType::FlagsChanged => Some(Event::FlagsChanged(flags)),
        _ => None,
    }
}
