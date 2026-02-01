# clip-ui

Tauri overlay for Slotpaste: 6-slot chooser (1–6, labels J/K/L/U/I/O). Listens for show/hide over UDP from the agent; sends chosen/cancel back.

## Run

From this directory:

```bash
npm install
npm run dev
```

Or from repo root: `cd apps/clip-ui && npm run dev`

Requires the agent to be running (`RUST_LOG=info cargo run -p clip-agent`) so it can receive UDP on 45454 and send responses to 45455.

## Build

```bash
npm run build
```

Produces a Tauri app bundle (platform-specific).
