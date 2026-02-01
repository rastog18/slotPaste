# Slotpaste

macOS clipboard manager MVP. Saves and pastes from 6 slots (1–6, labeled J/K/L/U/I/O) without interfering with normal copy/paste.

## Building

```bash
cargo build
```

## Running

**You need both the agent and the UI for the chooser overlay.**

### 1. Run the agent

```bash
RUST_LOG=info cargo run -p clip-agent
```

Runs in the foreground. Press Ctrl+C to stop.

- **Accessibility permission** is required for global hotkeys. Run `cargo run -p clip -- doctor` to verify or grant it.
- Normal **Cmd+C** and **Cmd+V** are **not** intercepted. Only **Cmd+Option+V** is captured for Slotpaste paste.

### 2. Run the chooser UI (Tauri)

From the repo root:

```bash
cd apps/clip-ui && npm install && npm run dev
```

Or with pnpm: `cd apps/clip-ui && pnpm install && pnpm dev`

The UI shows a small overlay when you copy (save flow) or press Cmd+Option+V (paste flow). It listens on UDP 45454; the agent sends show/hide and receives chosen/cancel on 45455.

### Doctor (system checks)

```bash
cargo run -p clip -- doctor
```

Checks Accessibility permission (required for the event tap on macOS).

### Other commands

```bash
# Install system hooks (if used by your setup)
cargo run -p clip -- install
```

## Behavior (chooser overlay)

- **Save flow (non-interfering)**  
  - You use normal **Cmd+C** (not swallowed).  
  - A chooser overlay appears for 0.8s.  
  - **Option+1..6** or **mouse click** on a slot saves clipboard to that slot (1=J, 2=K, 3=L, 4=U, 5=I, 6=O).  
  - If no slot is chosen in 0.8s, the chooser closes with no side effects.

- **Paste flow (dedicated shortcut)**  
  - **Cmd+Option+V** is swallowed; a chooser appears (active mode).  
  - **1..6** (no Option), or mouse click, selects a slot to paste from.  
  - **Esc** or 0.8s timeout cancels.  
  - **Cmd+V** is never touched; normal paste stays Cmd+V.

## Development

- `crates/clip` – CLI (doctor, install, etc.)
- `crates/clip-agent` – Agent: event tap, state machine, SQLite slots, IPC to UI
- `apps/clip-ui` – Tauri overlay: 6-slot chooser, UDP listener, mouse + keyboard selection

## Verification

1. **Run both:** agent (`RUST_LOG=info cargo run -p clip-agent`) and UI (`cd apps/clip-ui && npm run dev`).
2. **Cmd+C ⇒ chooser:** Copy text (Cmd+C); chooser appears. Click slot 1 ⇒ saves clipboard into slot J. Log: `Saved → Slot J: "..."`.
3. **Cmd+C ⇒ timeout:** Copy (Cmd+C); do nothing. Chooser disappears in ~0.8s, no save.
4. **Cmd+Option+V ⇒ paste:** Press Cmd+Option+V; chooser appears. Press 1 ⇒ pastes slot J (or logs “Slot J is empty”).
5. **Normal Cmd+V:** In any app, Cmd+V pastes as usual (not intercepted).
6. **Normal Cmd+C:** Cmd+C copies as usual (not intercepted).

## Persistence

Slots are stored in SQLite: on macOS, `~/Library/Application Support/Slotpaste/slotpaste.db`; elsewhere `~/.slotpaste/slotpaste.db`. Slot IDs J/K/L/U/I/O map to chooser numbers 1..6.
