# Slotpaste

macOS clipboard manager MVP

## Building

```bash
cargo build
```

## Running

### Start the agent (foreground)

```bash
cargo run -p clip -- start
```

This will start the agent in the foreground. Press Ctrl+C to stop.

### Run agent directly

```bash
RUST_LOG=debug cargo run -p clip-agent
```

**Note:** Keyboard capture requires Accessibility permission. Run `cargo run -p clip -- doctor` first to verify or grant permission.

**Milestone 3 (Slot Select mode):** 0.8s window after Cmd+C or Cmd+V:
- Cmd+C → release Cmd → press J/K/L/U/I/O → `Saved → Slot J` (with clipboard preview)
- Cmd+V → press J/K/L/U/I/O → `Pasted ← Slot J` or `Slot J is empty`
- Esc / timeout / invalid key → cancel logs

**Milestone 4 (Clipboard read):** On save, reads actual clipboard text with ~300ms retry; logs preview.

### Doctor (system checks)

```bash
cargo run -p clip -- doctor
```

Checks system configuration, including Accessibility permission (required for global hotkeys on macOS).

**Expected output examples:**

- When Accessibility is granted:
  ```
  Accessibility: ✅ granted
  ```

- When Accessibility is not granted:
  ```
  Accessibility: ❌ not granted

  We need Accessibility permission to register global hotkeys / event tap for Slotpaste.
  No keystrokes are logged or sent anywhere; text stays local.

  Would you like to open System Settings to grant permission? [Y/n]
  ```

  After granting permission and pressing Enter to re-check:
  ```
  Accessibility: ✅ granted
  ```

### Other commands

```bash
# Install system hooks
cargo run -p clip -- install
```

## Development

The workspace contains:
- `crates/clip` - CLI application
- `crates/clip-agent` - Background agent/daemon
- `apps/clip-ui` - Tauri UI (placeholder, to be implemented)

## Milestone 4 Verification Checklist

```bash
RUST_LOG=info cargo run -p clip-agent
```

- [ ] **Copy + slot with text:** Copy text in any app (Cmd+C), release Cmd, hit J quickly → logs `Saved → Slot J: "..."` with actual copied preview
- [ ] **Copy non-text:** Copy image or other non-text content, then Cmd+C → J → logs `Nothing to save (clipboard has no text)`
- [ ] **Paste flow unchanged:** Cmd+V then J → logs `Pasted ← Slot J` if slot has text, or `Slot J is empty` otherwise

## Milestone 5 Verification (paste from slot + Cmd+V suppression)

- [ ] **Cmd+V does not paste immediately:** With focus in any app (e.g. TextEdit), press Cmd+V once. The target app must **not** paste; nothing is pasted until a slot is chosen.
- [ ] **Cmd+V then J pastes Slot J:** Copy some text, save to Slot J (Cmd+C → release Cmd → J). Focus elsewhere, press Cmd+V then J. The **Slot J** text is pasted into the app (clipboard is restored after a short delay).
- [ ] **Empty slot:** Cmd+V then J when Slot J has no text → logs `Slot J is empty`, no paste.

## Milestone 6 Verification (SQLite persistence)

- [ ] **Save text into Slot J:** Copy text, Cmd+C → release Cmd → J. Log shows `Saved → Slot J: "..."`.
- [ ] **Stop agent:** Press Ctrl+C.
- [ ] **Restart agent:** `RUST_LOG=info cargo run -p clip-agent`.
- [ ] **Paste after restart:** Cmd+V then J. The previously saved Slot J text is pasted (proves persistence).
- [ ] **DB file exists:** On macOS, `~/Library/Application Support/Slotpaste/slotpaste.db` exists; on other platforms, `~/.slotpaste/slotpaste.db`.
