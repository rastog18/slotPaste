//! Slot Select mode state machine.

use crate::keys::{Key, SlotId};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{atomic::AtomicU8, mpsc::Receiver, mpsc::Sender, Arc};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// Mode for event tap: 0=Idle, 1=CopyArmed, 2=CopySelecting, 3=PasteSelecting.
pub const MODE_IDLE: u8 = 0;
pub const MODE_COPY_ARMED: u8 = 1;
pub const MODE_COPY_SELECTING: u8 = 2;
pub const MODE_PASTE_SELECTING: u8 = 3;

/// Internal events from event tap or timer.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum Event {
    KeyDown(Key, u64),
    KeyUp(Key, u64),
    FlagsChanged(u64),
    /// Sent by event tap when Cmd+V was swallowed in Idle (paste trigger).
    CmdVTrigger,
    Timeout(u64),
    Quit,
}

const SLOT_SELECT_WINDOW: Duration = Duration::from_millis(800);
const CMD_MASK: u64 = 1 << 20;

/// State machine state.
#[derive(Debug)]
enum State {
    Idle,
    CopyArmed { deadline: Instant, token: u64 },
    CopySelecting { deadline: Instant, token: u64 },
    PasteSelecting { deadline: Instant, token: u64 },
}

/// In-memory slot storage; optionally backed by SQLite (upsert on save, load on start).
pub struct SlotStorage {
    slots: HashMap<SlotId, String>,
    persistence: Option<rusqlite::Connection>,
}

impl SlotStorage {
    pub fn new() -> Self {
        Self {
            slots: HashMap::new(),
            persistence: None,
        }
    }

    /// Pre-populate from DB and use connection for future upserts.
    pub fn with_persistence(conn: rusqlite::Connection, loaded: HashMap<SlotId, String>) -> Self {
        Self {
            slots: loaded,
            persistence: Some(conn),
        }
    }

    pub fn save(&mut self, slot: SlotId, content: String) {
        self.slots.insert(slot, content.clone());
        if let Some(ref conn) = self.persistence {
            if let Err(e) = crate::persistence::sqlite::upsert_slot(conn, slot.label(), &content) {
                warn!("persistence upsert failed: {}", e);
            }
        }
    }

    pub fn get(&self, slot: SlotId) -> Option<&str> {
        self.slots.get(&slot).map(|s| s.as_str())
    }

    pub fn is_empty(&self, slot: SlotId) -> bool {
        self.get(slot).map(|s| s.is_empty()).unwrap_or(true)
    }
}

impl Default for SlotStorage {
    fn default() -> Self {
        Self::new()
    }
}

/// Runs the state machine loop. Returns when Quit is received.
/// Updates `mode` (0=Idle, 1=CopyArmed, 2=CopySelecting, 3=PasteSelecting) on every transition.
/// If `persistence` is Some(conn), loads slots from DB and upserts on save.
pub fn run(
    rx: Receiver<Event>,
    tx: Sender<Event>,
    mode: Arc<AtomicU8>,
    persistence: Option<rusqlite::Connection>,
) {
    let mut slots = match persistence {
        Some(conn) => {
            match crate::persistence::sqlite::load_all(&conn) {
                Ok(rows) => {
                    let loaded: HashMap<SlotId, String> = rows
                        .into_iter()
                        .filter_map(|(k, v)| SlotId::from_label(&k).map(|s| (s, v)))
                    .collect();
                    info!("loaded {} slots from DB", loaded.len());
                    SlotStorage::with_persistence(conn, loaded)
                }
                Err(e) => {
                    warn!("persistence load failed: {}, using in-memory only", e);
                    SlotStorage::new()
                }
            }
        }
        None => SlotStorage::new(),
    };
    let mut state = State::Idle;
    let mut cmd_down = false;
    let mut next_token: u64 = 0;
    mode.store(MODE_IDLE, Ordering::Release);

    loop {
        let event = match rx.recv() {
            Ok(e) => e,
            Err(_) => break,
        };

        let prev_cmd_down = cmd_down;
        match &event {
            Event::Quit => {
                debug!("Received Quit");
                break;
            }
            Event::FlagsChanged(flags) => {
                cmd_down = (flags & CMD_MASK) != 0;
                debug!("flagsChanged cmd_down={}", cmd_down);
            }
            _ => {}
        }

        state = match state {
            State::Idle => handle_idle(state, event, &mut next_token, cmd_down, &tx),
            State::CopyArmed { deadline, token } => {
                handle_copy_armed(event, deadline, token, &mut next_token, prev_cmd_down, &tx, &mut slots)
            }
            State::CopySelecting { deadline, token } => {
                handle_copy_selecting(event, deadline, token, &mut slots)
            }
            State::PasteSelecting { deadline, token } => {
                handle_paste_selecting(event, deadline, token, &slots)
            }
        };
        set_mode_for_state(&state, &mode);
    }
}

fn set_mode_for_state(state: &State, mode: &AtomicU8) {
    let m = match state {
        State::Idle => MODE_IDLE,
        State::CopyArmed { .. } => MODE_COPY_ARMED,
        State::CopySelecting { .. } => MODE_COPY_SELECTING,
        State::PasteSelecting { .. } => MODE_PASTE_SELECTING,
    };
    mode.store(m, Ordering::Release);
}

fn handle_idle(
    _state: State,
    event: Event,
    next_token: &mut u64,
    _cmd_down: bool,
    tx: &Sender<Event>,
) -> State {
    match event {
        Event::KeyDown(Key::C, flags) if (flags & CMD_MASK) != 0 => {
            *next_token += 1;
            let token = *next_token;
            let deadline = Instant::now() + SLOT_SELECT_WINDOW;
            spawn_timeout(deadline, token, tx.clone());
            debug!("CopyArmed deadline={:?} token={}", deadline, token);
            State::CopyArmed { deadline, token }
        }
        Event::CmdVTrigger => {
            *next_token += 1;
            let token = *next_token;
            let deadline = Instant::now() + SLOT_SELECT_WINDOW;
            spawn_timeout(deadline, token, tx.clone());
            debug!("PasteSelecting deadline={:?} token={}", deadline, token);
            State::PasteSelecting { deadline, token }
        }
        Event::KeyDown(Key::V, flags) if (flags & CMD_MASK) != 0 => {
            *next_token += 1;
            let token = *next_token;
            let deadline = Instant::now() + SLOT_SELECT_WINDOW;
            spawn_timeout(deadline, token, tx.clone());
            debug!("PasteSelecting deadline={:?} token={}", deadline, token);
            State::PasteSelecting { deadline, token }
        }
        _ => State::Idle,
    }
}

fn handle_copy_armed(
    event: Event,
    deadline: Instant,
    token: u64,
    _next_token: &mut u64,
    cmd_down: bool,
    _tx: &Sender<Event>,
    slots: &mut SlotStorage,
) -> State {
    match event {
        Event::Timeout(t) if t == token => {
            info!("Copy select cancelled (timeout)");
            State::Idle
        }
        Event::KeyDown(Key::Escape, _) => {
            info!("Copy select cancelled (esc)");
            State::Idle
        }
        Event::KeyDown(key, _) => {
            if !cmd_down {
                // Cmd released: transition to CopySelecting
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    info!("Copy select cancelled (timeout)");
                    State::Idle
                } else {
                    debug!("Cmd released, now CopySelecting");
                    match key {
                        Key::Slot(slot) => {
                            save_slot_from_clipboard(slots, slot);
                            State::Idle
                        }
                        Key::Escape => {
                            info!("Copy select cancelled (esc)");
                            State::Idle
                        }
                        Key::Other(_) | Key::C | Key::V => {
                            info!("Copy select cancelled (invalid key: {})", key_debug(key));
                            State::Idle
                        }
                    }
                }
            } else {
                // Cmd still down: ignore slot keys (must wait for release)
                if matches!(key, Key::Slot(_)) {
                    debug!("Ignoring slot key while Cmd held");
                } else if matches!(key, Key::Escape) {
                    info!("Copy select cancelled (esc)");
                    return State::Idle;
                } else if !matches!(key, Key::C | Key::V) {
                    info!("Copy select cancelled (invalid key: {})", key_debug(key));
                    return State::Idle;
                }
                State::CopyArmed { deadline, token }
            }
        }
        Event::FlagsChanged(flags) => {
            let cmd_now = (flags & CMD_MASK) != 0;
            if !cmd_now && cmd_down {
                // cmd_down = was true before this event, cmd_now = false now => Cmd just released
                // Cmd just released: transition to CopySelecting
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    info!("Copy select cancelled (timeout)");
                    State::Idle
                } else {
                    debug!("Cmd released (via FlagsChanged), now CopySelecting");
                    State::CopySelecting { deadline, token }
                }
            } else {
                State::CopyArmed { deadline, token }
            }
        }
        _ => State::CopyArmed { deadline, token },
    }
}

fn handle_copy_selecting(
    event: Event,
    deadline: Instant,
    token: u64,
    slots: &mut SlotStorage,
) -> State {
    match event {
        Event::Timeout(t) if t == token => {
            info!("Copy select cancelled (timeout)");
            State::Idle
        }
        Event::KeyDown(Key::Escape, _) => {
            info!("Copy select cancelled (esc)");
            State::Idle
        }
        Event::KeyDown(Key::Slot(slot), _) => {
            if Instant::now() < deadline {
                save_slot_from_clipboard(slots, slot);
            } else {
                info!("Copy select cancelled (timeout)");
            }
            State::Idle
        }
        Event::KeyDown(key, _) => {
            if Instant::now() < deadline {
                info!("Copy select cancelled (invalid key: {})", key_debug(key));
            }
            State::Idle
        }
        _ => State::CopySelecting { deadline, token },
    }
}

fn handle_paste_selecting(
    event: Event,
    deadline: Instant,
    token: u64,
    slots: &SlotStorage,
) -> State {
    match event {
        Event::Timeout(t) if t == token => {
            info!("Paste select cancelled (timeout)");
            State::Idle
        }
        Event::KeyDown(Key::Escape, _) => {
            info!("Paste select cancelled (esc)");
            State::Idle
        }
        Event::KeyDown(Key::Slot(slot), _) => {
            if Instant::now() < deadline {
                if slots.is_empty(slot) {
                    info!("Slot {} is empty", slot.label());
                } else if let Some(content) = slots.get(slot).map(|s| s.to_string()) {
                    info!("Pasted ← Slot {}", slot.label());
                    #[cfg(target_os = "macos")]
                    crate::macos::paste::paste_from_slot(&content);
                }
            } else {
                info!("Paste select cancelled (timeout)");
            }
            State::Idle
        }
        Event::KeyDown(key, _) => {
            if Instant::now() < deadline {
                info!("Paste select cancelled (invalid key: {})", key_debug(key));
            }
            State::Idle
        }
        _ => State::PasteSelecting { deadline, token },
    }
}

/// Reads clipboard and saves to slot. Logs result. Does not overwrite slot if no text.
fn save_slot_from_clipboard(slots: &mut SlotStorage, slot: SlotId) {
    #[cfg(target_os = "macos")]
    {
        let text = crate::macos::clipboard::read_text_with_retry(Duration::from_millis(300));
        match text {
            Some(content) => {
                let preview = preview_for_log(&content);
                info!("Saved → Slot {}: \"{}\"", slot.label(), preview);
                slots.save(slot, content);
            }
            None => {
                info!("Nothing to save (clipboard has no text)");
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        info!("Clipboard read not supported on this platform");
    }
}

/// Safe preview for logging: trim, replace newlines, cap at 30 chars.
fn preview_for_log(s: &str) -> String {
    let trimmed: String = s.trim().replace('\n', " ").replace('\r', " ");
    let chars: Vec<_> = trimmed.chars().collect();
    if chars.len() <= 30 {
        trimmed
    } else {
        chars.iter().take(30).collect::<String>() + "..."
    }
}

fn key_debug(key: Key) -> String {
    match key {
        Key::Slot(s) => format!("slot {}", s.label()),
        Key::Escape => "Esc".to_string(),
        Key::C => "C".to_string(),
        Key::V => "V".to_string(),
        Key::Other(c) => format!("keycode {}", c),
    }
}

fn spawn_timeout(deadline: Instant, token: u64, tx: Sender<Event>) {
    std::thread::spawn(move || {
        let now = Instant::now();
        if deadline > now {
            std::thread::sleep(deadline - now);
        }
        let _ = tx.send(Event::Timeout(token));
    });
}
