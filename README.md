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
- Cmd+C → release Cmd → press J/K/L/U/I/O → `Saved → Slot J`
- Cmd+V → press J/K/L/U/I/O → `Pasted ← Slot J` or `Slot J is empty`
- Esc / timeout / invalid key → cancel logs

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

## Milestone 3 Verification Checklist

```bash
RUST_LOG=debug cargo run -p clip-agent
```

- [ ] **Copy + slot:** Cmd+C, release Cmd, press J within 0.8s → logs `Saved → Slot J`
- [ ] **Copy timeout:** Cmd+C, wait >0.8s → logs `Copy select cancelled (timeout)`
- [ ] **Copy esc:** Cmd+C, press Esc → logs `Copy select cancelled (esc)`
- [ ] **Paste + slot:** Cmd+V, press J within 0.8s → logs `Pasted ← Slot J` or `Slot J is empty`
- [ ] **Invalid key:** During select window, press non-slot key → logs `... cancelled (invalid key: ...)`
