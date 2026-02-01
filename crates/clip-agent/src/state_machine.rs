//! Slotpaste state machine: chooser overlay (save after Cmd+C, paste after Cmd+Option+V).

use crate::keys::{Key, SlotId};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{atomic::AtomicU8, mpsc::Receiver, mpsc::Sender, Arc};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// Mode for event tap: 0=Idle, 1=SaveChooserPending, 2=PasteChooserActive.
pub const MODE_IDLE: u8 = 0;
pub const MODE_SAVE_PENDING: u8 = 1;
pub const MODE_PASTE_ACTIVE: u8 = 2;

/// Internal events from event tap, IPC, or timer.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum Event {
    KeyDown(Key, u64),
    KeyUp(Key, u64),
    FlagsChanged(u64),
    /// Cmd+Option+V swallowed in Idle -> show paste chooser.
    CmdOptionVTrigger,
    /// UI chose slot 1..6.
    ChooserChosen { token: String, slot_num: u8 },
    /// UI cancel or timeout.
    ChooserCancel { token: String, reason: String },
    Quit,
}

const CHOOSER_TIMEOUT_MS: u64 = 800;
const CMD_MASK: u64 = 1 << 20;

/// State machine state.
#[derive(Debug)]
enum State {
    Idle,
    SaveChooserPending { token: String, deadline: Instant },
    PasteChooserActive { token: String, deadline: Instant },
}

/// In-memory slot storage; optionally backed by SQLite.
pub struct SlotStorage {
    slots: HashMap<SlotId, String>,
    persistence: Option<rusqlite::Connection>,
}

impl SlotStorage {
    pub fn new() -> Self {
        Self { slots: HashMap::new(), persistence: None }
    }

    pub fn with_persistence(conn: rusqlite::Connection, loaded: HashMap<SlotId, String>) -> Self {
        Self { slots: loaded, persistence: Some(conn) }
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
    #[allow(unused_assignments)]
    let mut cmd_down = false;
    let mut next_token: u64 = 0;
    mode.store(MODE_IDLE, Ordering::Release);

    loop {
        let event = match rx.recv() {
            Ok(e) => e,
            Err(_) => break,
        };

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
            State::Idle => handle_idle(event, &mut next_token, &tx),
            State::SaveChooserPending { token, deadline } => {
                handle_save_chooser_pending(event, token.clone(), deadline, &tx, &mut slots)
            }
            State::PasteChooserActive { token, deadline } => {
                handle_paste_chooser_active(event, token.clone(), deadline, &tx, &slots)
            }
        };
        set_mode_for_state(&state, &mode);
    }
}

fn set_mode_for_state(state: &State, mode: &AtomicU8) {
    let m = match state {
        State::Idle => MODE_IDLE,
        State::SaveChooserPending { .. } => MODE_SAVE_PENDING,
        State::PasteChooserActive { .. } => MODE_PASTE_ACTIVE,
    };
    mode.store(m, Ordering::Release);
}

fn handle_idle(event: Event, next_token: &mut u64, tx: &Sender<Event>) -> State {
    match event {
        Event::KeyDown(Key::C, flags) if (flags & CMD_MASK) != 0 => {
            info!("Cmd+C detected (KeyDown C with Cmd) -> save chooser flow");
            *next_token += 1;
            let token = next_token.to_string();
            let deadline = Instant::now() + Duration::from_millis(CHOOSER_TIMEOUT_MS);
            info!("send_show(save, token={}) -> UDP 45454", token);
            crate::ipc::udp::send_show("save", &token, CHOOSER_TIMEOUT_MS);
            spawn_chooser_timeout(token.clone(), tx.clone());
            info!("Chooser show (save) token={} -> UI, state=SaveChooserPending", token);
            State::SaveChooserPending { token, deadline }
        }
        Event::CmdOptionVTrigger => {
            info!("Cmd+Option+V detected -> paste chooser flow");
            *next_token += 1;
            let token = next_token.to_string();
            let deadline = Instant::now() + Duration::from_millis(CHOOSER_TIMEOUT_MS);
            info!("send_show(paste, token={}) -> UDP 45454", token);
            crate::ipc::udp::send_show("paste", &token, CHOOSER_TIMEOUT_MS);
            spawn_chooser_timeout(token.clone(), tx.clone());
            info!("Chooser show (paste) token={} -> UI, state=PasteChooserActive", token);
            State::PasteChooserActive { token, deadline }
        }
        _ => State::Idle,
    }
}

fn handle_save_chooser_pending(
    event: Event,
    token: String,
    deadline: Instant,
    _tx: &Sender<Event>,
    slots: &mut SlotStorage,
) -> State {
    match event {
        Event::ChooserChosen { token: t, slot_num } if t == token => {
            info!("Save chooser: user chose slot {} (token={})", slot_num, t);
            if let Some(slot) = SlotId::from_slot_num(slot_num) {
                save_slot_from_clipboard(slots, slot);
            }
            info!("send_hide(token={}) -> UI", token);
            crate::ipc::udp::send_hide(&token);
            State::Idle
        }
        Event::ChooserCancel { token: t, reason } if t == token => {
            info!("Save chooser cancelled: {} (token={})", reason, t);
            info!("send_hide(token={}) -> UI", token);
            crate::ipc::udp::send_hide(&token);
            State::Idle
        }
        _ => State::SaveChooserPending { token, deadline },
    }
}

fn handle_paste_chooser_active(
    event: Event,
    token: String,
    deadline: Instant,
    _tx: &Sender<Event>,
    slots: &SlotStorage,
) -> State {
    match event {
        Event::ChooserChosen { token: t, slot_num } if t == token => {
            info!("Paste chooser: user chose slot {} (token={})", slot_num, t);
            if let Some(slot) = SlotId::from_slot_num(slot_num) {
                if slots.is_empty(slot) {
                    info!("Slot {} is empty", slot.label());
                } else if let Some(content) = slots.get(slot).map(|s| s.to_string()) {
                    info!("Pasted ← Slot {}", slot.label());
                    #[cfg(target_os = "macos")]
                    std::thread::spawn(move || crate::macos::paste::paste_from_slot(&content));
                }
            }
            info!("send_hide(token={}) -> UI", token);
            crate::ipc::udp::send_hide(&token);
            State::Idle
        }
        Event::ChooserCancel { token: t, reason } if t == token => {
            info!("Paste chooser cancelled: {} (token={})", reason, t);
            info!("send_hide(token={}) -> UI", token);
            crate::ipc::udp::send_hide(&token);
            State::Idle
        }
        _ => State::PasteChooserActive { token, deadline },
    }
}

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
            None => info!("Nothing to save (clipboard has no text)"),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        info!("Clipboard read not supported on this platform");
    }
}

fn preview_for_log(s: &str) -> String {
    let trimmed: String = s.trim().replace('\n', " ").replace('\r', " ");
    let chars: Vec<_> = trimmed.chars().collect();
    if chars.len() <= 30 {
        trimmed
    } else {
        chars.iter().take(30).collect::<String>() + "..."
    }
}

fn spawn_chooser_timeout(token: String, tx: Sender<Event>) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(CHOOSER_TIMEOUT_MS));
        info!("chooser timeout fired (token={}), sending ChooserCancel", token);
        let _ = tx.send(Event::ChooserCancel { token, reason: "timeout".to_string() });
    });
}
