# clip

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
cargo run -p clip-agent
```

### Other commands

```bash
# Check system configuration
cargo run -p clip -- doctor

# Install system hooks
cargo run -p clip -- install
```

## Development

The workspace contains:
- `crates/clip` - CLI application
- `crates/clip-agent` - Background agent/daemon
- `apps/clip-ui` - Tauri UI (placeholder, to be implemented)
