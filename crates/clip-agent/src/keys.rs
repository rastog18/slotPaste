//! Key and slot definitions.

/// Logical key from a keyboard event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Slot(SlotId),
    Escape,
    C,
    V,
    Other(u16),
}

/// Slot identifiers: J, K, L, U, I, O.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SlotId {
    J,
    K,
    L,
    U,
    I,
    O,
}

impl SlotId {
    /// Human-readable label for logs.
    pub fn label(self) -> &'static str {
        match self {
            SlotId::J => "J",
            SlotId::K => "K",
            SlotId::L => "L",
            SlotId::U => "U",
            SlotId::I => "I",
            SlotId::O => "O",
        }
    }
}

/// macOS virtual key codes (Carbon-style).
const VK_ANSI_C: i64 = 8;
const VK_ANSI_V: i64 = 9;
const VK_ANSI_J: i64 = 38;
const VK_ANSI_K: i64 = 40;
const VK_ANSI_L: i64 = 37;
const VK_ANSI_U: i64 = 32;
const VK_ANSI_I: i64 = 34;
const VK_ANSI_O: i64 = 31;
const VK_ESCAPE: i64 = 53;

/// Converts a raw keycode to a Key. Used by the event tap.
pub fn keycode_to_key(keycode: i64) -> Key {
    match keycode {
        VK_ANSI_J => Key::Slot(SlotId::J),
        VK_ANSI_K => Key::Slot(SlotId::K),
        VK_ANSI_L => Key::Slot(SlotId::L),
        VK_ANSI_U => Key::Slot(SlotId::U),
        VK_ANSI_I => Key::Slot(SlotId::I),
        VK_ANSI_O => Key::Slot(SlotId::O),
        VK_ESCAPE => Key::Escape,
        VK_ANSI_C => Key::C,
        VK_ANSI_V => Key::V,
        _ => Key::Other(keycode as u16),
    }
}
