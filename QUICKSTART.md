# bsdOS Metal Viewer — Quick Start

Быстрый старт для разработчиков.

## Prerequisites

✓ macOS 11.0+, Rust, Xcode CLI  
✓ SSH access to bsdOS VM  
✓ bsdOS with Wayland/cage stack  

## Installation (macOS)

```bash
cd /path/to/mac-companion/metal-viewer

# One-time setup
./setup-mac.sh

# Should output:
# [✓] Rust: rustc 1.75.0
# [✓] Xcode CLI: ...
# [✓] Metal GPU: Metal Family 3
# [✓] Binary built: target/release/bsdos-metal-viewer (15MB)
# [setup] ✓ Ready!
```

## Running

**Terminal 1 (SSH tunnel — keep running):**

```bash
cd mac-companion/metal-viewer
./setup-tunnel.sh <vm-host>

# Keeps port 7447 forwarded
# Press Ctrl+C to close (when done)
```

**Terminal 2 (Linux host — start VM stack):**

```bash
cd /root/bsdOS
make vm-start-wayland           # Start cage, seatd, Zenoh broker
make vm-start-screencopy        # Start wf-recorder + sender
```

**Terminal 3 (macOS — run viewer):**

```bash
cd mac-companion/metal-viewer
./run.sh

# Window appears → live video from VM
```

## Troubleshooting

### "Connection refused" on Mac

```bash
# Check tunnel is running
lsof -i :7447
# Should list: ssh (LISTEN on ::1 port 7447)

# If not, re-run: ./setup-tunnel.sh <vm-host>
```

### "Subscribed but no frames"

```bash
# On Linux host, check screencopy sender
make vm-ssh
ps aux | grep wf-recorder
tail -f /tmp/screencopy.log
```

### Black window on Mac

1. Check cage is running: `make vm-cage-log`
2. Check wf-recorder: `tail -f /tmp/screencopy-wf.log`
3. Verbose logging: `RUST_LOG=debug ./run.sh`

## Development

**Rebuild after code changes:**

```bash
# Mac side (viewer code)
cargo build --release
./run.sh

# VM side (screencopy sender, cage config)
# Edit code → make vm-start-screencopy (restart)
```

**Debugging:**

```bash
# Full Zenoh debug output
RUST_LOG=zenoh=debug,bsdos=debug ./target/release/bsdos-metal-viewer

# Check frame sizes and counts
# Logs every 3 seconds: "[update] Frames: 30 | 1280x720"
```

## Architecture

```
VM wf-recorder              Mac bsdos-metal-viewer
        ↓                           ↑
  Zenoh sender          Zenoh subscriber
        ↓                           ↑
  bsdos/display/frame (binary packets)
        ↓                           ↑
     Zenoh broker (localhost:7447 via SSH tunnel)
```

Format: `[width u32 BE][height u32 BE][stride u32 BE][RGBA pixels...]`

## Files

- `src/main.rs` — Main loop, Zenoh receiver, frame buffer
- `src/metal_view.rs` — Metal GPU, NSWindow, MTKView
- `src/lib.rs` — Module exports
- `Cargo.toml` — Dependencies (objc2, zenoh, tokio)
- `setup-mac.sh` — One-time build + helpers
- `setup-tunnel.sh` — SSH tunnel helper (created by setup-mac.sh)
- `run.sh` — Run viewer (created by setup-mac.sh)

## Next Steps

1. ✓ Build on macOS
2. ✓ Start Wayland on VM
3. ✓ Run screencopy sender
4. ✓ View live video on Mac

For details, see: [METAL-VIEWER-SETUP.md](../METAL-VIEWER-SETUP.md)
