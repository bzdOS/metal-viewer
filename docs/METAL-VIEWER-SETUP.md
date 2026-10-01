# bsdOS Metal Viewer — Full Integration Guide

Полная настройка Wayland display streaming от bsdOS VM на macOS через Metal.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│ bsdOS VM (FreeBSD 14.4 aarch64)                                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  cage (kiosk WM)   ◄─────── Wayland display socket             │
│   │                         (wayland-0)                         │
│   ├─ seatd (session mgr)                                        │
│   └─ app windows (kiosk, web)                                   │
│                                                                  │
│  wf-recorder ────► raw video frames (RGBA32)                    │
│   │                                                              │
│   └─► bsdos-screencopy-sender ────► Zenoh broker (localhost)   │
│        (pack: [w|h|stride|rgba])        │                       │
│                                         │                       │
│  bsdos-core / broker (Zenoh mesh)      │                       │
│    ├─ bsdos/display/frame ◄────────────┘                        │
│    └─ (other: telemetry, control)                               │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
         │
         │ Zenoh mesh (TCP loopback → SSH tunnel)
         ↓
┌─────────────────────────────────────────────────────────────────┐
│ Linux host (SSH tunnel)                                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  $ ssh -L 7447:127.0.0.1:7447 freebsd@vm-host                  │
│     ↑ Forwards VM Zenoh port to Mac localhost                  │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
         │
         │ localhost:7447 (after SSH tunnel)
         ↓
┌─────────────────────────────────────────────────────────────────┐
│ macOS (Metal GPU)                                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  bsdos-metal-viewer (Rust + objc2)                              │
│   ├─ Zenoh subscriber (bsdos/display/frame)                     │
│   ├─ Frame buffer (Arc<Mutex<>>)                                │
│   ├─ Metal device + MTLTexture                                  │
│   ├─ MTKView (CAMetalLayer)                                     │
│   └─ NSWindow (1280×720 initially)                              │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## Prerequisites

### On bsdOS (VM)

- FreeBSD 14.4+ with Wayland stack
- `cage` window manager (kiosk mode, lightweight)
- `seatd` session manager
- `wf-recorder` (screen capture tool)
- Rust binary: `bsdos-screencopy-sender` (TBD — will be built)
- Zenoh broker accessible on localhost:7447

### On Linux host

- SSH access to VM
- Ability to forward ports (`ssh -L`)

### On macOS

- macOS 11.0+ (Big Sur, Monterey, Ventura, Sonoma)
- Rust 1.70+
- Xcode Command Line Tools
- Metal GPU (all modern Macs have this)

## Step 1: Build on macOS

```bash
# Navigate to metal-viewer
cd /path/to/bsdOS/mac-companion/metal-viewer

# Build release binary
cargo build --release

# Binary location
ls -lh target/release/bsdos-metal-viewer
```

Expected size: ~8-15 MB (depending on strip configuration)

## Step 2: Setup SSH Tunnel (persistent)

On macOS, in a dedicated terminal (keep it running):

```bash
# Replace vm-host with actual IP/hostname
ssh -L 7447:127.0.0.1:7447 freebsd@vm-host

# On older SSH, might need:
ssh -L localhost:7447:localhost:7447 freebsd@vm-host

# If key-based auth:
ssh -i /path/to/key -L 7447:127.0.0.1:7447 freebsd@vm-host
```

This forwards the VM's Zenoh port (7447) to macOS localhost:7447.

**Keep this terminal open for the duration of your streaming session.**

## Step 3: Start Wayland Stack on VM

On Linux host, connect to VM and start Wayland:

```bash
# SSH to Linux host, then connect to VM
ssh freebsd@vm-host

# Start full Wayland + cage stack
make vm-start-wayland
# This runs:
#  - seatd
#  - cage (kiosk WM)
#  - bsdos-core / Zenoh broker
```

Or manually:

```bash
# On VM
sudo service seatd start
export XDG_RUNTIME_DIR=/tmp/wayland-run
export WAYLAND_DISPLAY=wayland-0
cage &
# ... start some app ...
```

Check that cage is running:

```bash
freebsd@vm$ ps aux | grep cage
freebsd 12345  0.0  1.2  ... cage
```

## Step 4: Start Screencopy Sender on VM

On Linux host:

```bash
# From bsdOS root directory
make vm-start-screencopy

# Or SSH manually:
make vm-ssh

# Inside VM:
freebsd@vm$ tail -f /tmp/screencopy.log
[screencopy] Starting screencopy sender...
[screencopy] wf-recorder: 1280x720 @ 30fps
[screencopy] Zenoh: publishing to bsdos/display/frame
...frames...
```

## Step 5: Run Metal Viewer on macOS

In a new macOS terminal (after tunnel is up):

```bash
# Ensure SSH tunnel is still running in another terminal
cd /path/to/bsdOS/mac-companion/metal-viewer

# Run with Zenoh peer pointing to localhost
ZENOH_PEER=tcp/localhost:7447 ./target/release/bsdos-metal-viewer

# Expected output:
# [bsdOS Metal Viewer] Starting...
# [bsdOS Metal Viewer] Zenoh peer: tcp/localhost:7447
# [receiver] ✓ Subscribed to bsdos/display/frame
# [receiver] ✓ Listening for frames from tcp/localhost:7447
# [metal] ✓ MTKView initialized (1280x720)
# [update] Frames: 30 | 1280x720
# [update] Frames: 60 | 1280x720
```

Window should appear on screen and display live video from VM.

## Troubleshooting

### "Cannot connect to Zenoh peer"

```
[receiver] Failed to open Zenoh session: Connection refused
```

**Solutions:**
1. Check SSH tunnel is running: `nc -zv localhost 7447`
2. Verify Zenoh broker started on VM: `ps aux | grep zenoh`
3. Re-establish tunnel: `Ctrl+C` and run `ssh -L ...` again

### "Subscribed to bsdos/display/frame" but no frames appear

**On VM side:**
```bash
make vm-ssh
ps aux | grep wf-recorder
ps aux | grep screencopy-sender

# Check logs
tail -30 /tmp/screencopy.log
tail -30 /tmp/screencopy-wf.log

# Verify cage is running
ps aux | grep cage
echo $WAYLAND_DISPLAY
```

**On Mac side:**
```bash
# Check that frames are arriving
RUST_LOG=debug ZENOH_PEER=tcp/localhost:7447 ./target/release/bsdos-metal-viewer
# Should show frame counting every 3 seconds
```

### Metal device error on Mac

```
Failed to create Metal device
```

**Check:**
```bash
# On macOS
system_profiler SPDisplaysDataType | grep Metal

# Should show: "Metal Family" (e.g., Metal Family 3)
# If not found — GPU not supported (very old Mac)
```

### Window appears but stays black

1. Check screencopy sender is actually capturing:
   ```bash
   make vm-ssh
   tail -100 /tmp/screencopy-wf.log | head -20
   ```

2. Check Zenoh payload size is reasonable:
   ```bash
   # On Mac, intercept with tcpdump (requires nettap privilege)
   # Or just check logs for frame size
   ZENOH_PEER=tcp/localhost:7447 ./target/release/bsdos-metal-viewer 2>&1 | grep -i frame
   ```

3. Metal texture upload might fail silently — check Xcode console for Metal errors:
   ```bash
   # On Mac, open Instruments → Metal
   Instruments &
   # and re-run viewer
   ```

### High latency (>100ms)

**VM side:**
- Reduce wf-recorder overhead:
  ```bash
  # Check CPU usage
  top -n 10 | head -20
  # wf-recorder should be <10% CPU
  ```

**Network:**
- Check SSH tunnel latency:
  ```bash
  ssh freebsd@vm-host 'ping -c 3 8.8.8.8'
  # Should be <50ms
  ```

- Reduce framerate (if needed):
  ```bash
  # Edit vm-start-screencopy.sh, add --fps-limit 15
  ```

**Mac side:**
- Check Metal texture upload time (Instruments → System Trace)
- May need to optimize texture format or use async copy

## Workflow

### Daily development

1. **Terminal 1 (persistent SSH tunnel):**
   ```bash
   ssh -L 7447:127.0.0.1:7447 freebsd@vm-host
   # Keep running
   ```

2. **Terminal 2 (VM management):**
   ```bash
   cd /srv/bsdos
   make vm-ssh
   # Or run make targets
   ```

3. **Terminal 3 (macOS viewer):**
   ```bash
   cd mac-companion/metal-viewer
   ZENOH_PEER=tcp/localhost:7447 ./target/release/bsdos-metal-viewer
   ```

### Rebuilding after code changes

**Mac viewer code:**
```bash
cd mac-companion/metal-viewer
cargo build --release
# Re-run Metal viewer in Terminal 3
```

**VM side (wf-recorder, cage, etc.):**
```bash
# From Terminal 2 (on Linux host)
make vm-start-screencopy   # Restarts sender if updated
```

### Debugging

**Verbose logging on Mac:**
```bash
RUST_LOG=debug ZENOH_PEER=tcp/localhost:7447 \
  ./target/release/bsdos-metal-viewer
```

**Verbose logging on VM:**
```bash
make vm-ssh
RUST_LOG=debug /usr/local/bin/bsdos-screencopy-sender
```

## Performance Targets

- **Latency:** <50ms (capture → display)
- **Framerate:** 30fps (1280×720)
- **CPU (VM):** <10% for wf-recorder + sender
- **CPU (Mac):** <20% for viewer (Metal render thread)
- **Bandwidth:** ~1.3 Gbps uncompressed RGBA (can be reduced with codec)

## Future Enhancements

- [ ] **Codec support:** H.264, VP9 (reduce bandwidth 5-10x)
- [ ] **Compression:** LZ4, Zstd (Zenoh plugin)
- [ ] **Input passthrough:** keyboard, mouse → VM (reverse channel)
- [ ] **Audio stream:** separate Zenoh topic for audio
- [ ] **HDR10 support:** for modern Mac displays
- [ ] **Window scaling:** automatic DPI scaling
- [ ] **Multi-monitor:** separate streams per display
- [ ] **Cursor sync:** VM cursor visible on Mac side

## Security Notes

- SSH tunnel is encrypted (SSH protocol)
- Zenoh runs on loopback only (no network exposure)
- No authentication between components yet (TODO)
- Frame data is uncompressed (suitable for local LAN)

## References

- **wf-recorder:** https://github.com/gmacd/wf-recorder
- **Zenoh:** https://zenoh.io/
- **Metal (objc2):** https://github.com/madsmtm/objc2
- **bsdOS CLAUDE.md:** `/srv/bsdos/CLAUDE.md`
