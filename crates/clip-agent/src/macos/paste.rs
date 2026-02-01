//! Paste from slot: set clipboard to slot text, synthesize Cmd+V (realistic 4-event sequence), restore clipboard.

use core_graphics::event::{CGEvent, CGEventTapLocation};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use std::thread;
use std::time::{Duration, Instant};
use tracing::{debug, warn};

use super::clipboard;

const RESTORE_DELAY_MS: u64 = 250;
const CMD_KEYCODE: u16 = 55;
const V_KEYCODE: u16 = 9; // ANSI_V

/// Pastes slot text: backup clipboard, set to slot text, post CmdDown/VDown/VUp/CmdUp, restore after delay.
/// Timing logged at debug: backup_ms, write_ms, restore_ms.
pub fn paste_from_slot(slot_text: &str) {
    let t0 = Instant::now();
    let backup = clipboard::read_text();
    let backup_ms = t0.elapsed().as_millis();
    debug!("paste_from_slot backup_ms={}", backup_ms);

    let t1 = Instant::now();
    if let Err(e) = clipboard::write_text(slot_text) {
        warn!("paste_from_slot: failed to set clipboard: {}", e);
        return;
    }
    let write_ms = t1.elapsed().as_millis();
    debug!("paste_from_slot write_ms={}", write_ms);

    thread::yield_now();

    if let Err(()) = post_cmd_v_realistic() {
        warn!("paste_from_slot: post_cmd_v_realistic failed");
        return;
    }

    let backup_owned = backup.map(|s| s.to_string());
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(RESTORE_DELAY_MS));
        let tr0 = Instant::now();
        if let Some(prev) = backup_owned {
            if clipboard::write_text(&prev).is_err() {
                warn!("(clipboard replaced)");
            }
        }
        let restore_ms = tr0.elapsed().as_millis();
        debug!("paste_from_slot restore_ms={}", restore_ms);
    });
}

/// Post 4 events: CmdDown, VDown, VUp, CmdUp. Keycodes: Cmd=55, V=9. Targets active session.
fn post_cmd_v_realistic() -> Result<(), ()> {
    let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState).map_err(|_| ())?;
    let cmd_flag = core_graphics::event::CGEventFlags::CGEventFlagCommand;
    let loc = CGEventTapLocation::HID;

    let cmd_down = CGEvent::new_keyboard_event(source.clone(), CMD_KEYCODE, true).map_err(|_| ())?;
    cmd_down.set_flags(cmd_flag);
    cmd_down.post(loc);

    let v_down = CGEvent::new_keyboard_event(source.clone(), V_KEYCODE, true).map_err(|_| ())?;
    v_down.set_flags(cmd_flag);
    v_down.post(loc);

    let v_up = CGEvent::new_keyboard_event(source.clone(), V_KEYCODE, false).map_err(|_| ())?;
    v_up.set_flags(cmd_flag);
    v_up.post(loc);

    let cmd_up = CGEvent::new_keyboard_event(source, CMD_KEYCODE, false).map_err(|_| ())?;
    cmd_up.set_flags(core_graphics::event::CGEventFlags::CGEventFlagNull);
    cmd_up.post(loc);

    Ok(())
}
