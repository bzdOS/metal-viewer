# bsdOS Display Stream Viewer

Real-time Zenoh display frame viewer for macOS and Linux. Connects to a running bsdOS broker and streams display frames to your development machine.

## Features

- **Live streaming**: Real-time PPM/PNG frame display via Zenoh
- **SSH tunnel support**: Secure remote viewing through SSH
- **Multi-format**: Automatic PNG conversion for QuickLook on macOS
- **Zero-copy**: Direct Cap'n Proto frame handling
- **Lightweight**: Python-only, no heavy dependencies

## Prerequisites

### macOS (recommended)

```bash
# Install Python and FFmpeg
brew install python3 ffmpeg

# Install Python dependencies
pip3 install eclipse-zenoh pillow
```

### Linux

```bash
# Ubuntu/Debian
sudo apt-get install python3-pip
pip3 install eclipse-zenoh pillow

# Fedora
sudo dnf install python3-pip
pip3 install eclipse-zenoh pillow
```

## Quick Start

### Option 1: Local Connection (Zenoh broker on localhost:7447)

```bash
# Terminal 1: Start bsdOS broker (on FreeBSD guest)
make vm-start
make vm-wait
ssh -p 2222 freebsd@localhost
su -m root -c '/opt/proto-src/broker/target/release/broker'

# Terminal 2 (Mac): Run viewer
cd /srv/bsdos/mac-companion
python3 display-viewer.py
```

**Expected output:**
```
[viewer] Save directory: /Users/user/Library/Caches/bsdOS
[viewer] Connecting to tcp/localhost:7447...
[viewer] Subscribing to 'bsdos/display/frame'...
[viewer] Waiting for frames... (Ctrl+C to stop)
[viewer] frames=30 size=2048KB fps=30.0 path=frame-000030-20260606-120530.png
```

Frames are saved to `~/Library/Caches/bsdOS/` on macOS or `/tmp/bsdos-viewer/` on Linux.

### Option 2: SSH Tunnel (Remote FreeBSD via SSH)

```bash
# Single command: setup tunnel and run viewer
./display-stream.sh --tunnel

# Or manually setup tunnel + viewer in separate terminals
# Terminal 1: SSH tunnel (port forward 7447)
ssh -L 7447:localhost:7447 \
    -p 2222 \
    -i bsdos-key \
    -N freebsd@localhost

# Terminal 2: Run viewer
./display-stream.sh
```

### Option 3: Remote Zenoh Broker (direct IP)

```bash
# Connect directly to remote broker
./display-stream.sh --peer tcp/192.168.1.100:7447

# Or with environment variable
export ZENOH_PEER=tcp/192.168.1.100:7447
python3 display-viewer.py
```

## Command-Line Options

### `display-viewer.py` (Python script)

Environment variables:
- `ZENOH_PEER` - Zenoh broker endpoint (default: `tcp/localhost:7447`)

Example:
```bash
ZENOH_PEER=tcp/remote:7447 python3 display-viewer.py
```

### `display-stream.sh` (Wrapper script)

```bash
# Show help
./display-stream.sh --help

# Local peer
./display-stream.sh --peer tcp/localhost:7447

# Remote peer
./display-stream.sh --peer tcp/freebsd.local:7447

# Auto SSH tunnel
./display-stream.sh --tunnel

# Custom SSH settings
SSH_USER=admin SSH_HOST=freebsd.example.com SSH_PORT=2222 ./display-stream.sh --tunnel
```

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│ macOS Development Machine                                   │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  display-viewer.py                                           │
│  ├─ Zenoh session (peer mode)                               │
│  ├─ Subscribe: bsdos/display/frame                          │
│  └─ Display: ~/Library/Caches/bsdOS/latest.png              │
│       (or QuickLook preview)                                 │
│                                                               │
└─────────────────────────────────────────────────────────────┘
                              △
                              │ TCP 7447
                              │ (via SSH tunnel if --tunnel)
                              │
┌─────────────────────────────────────────────────────────────┐
│ FreeBSD 14.4 Guest (QEMU)                                   │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  Zenoh Broker (peer mode)                                    │
│  ├─ Key: bsdos/display/frame                                │
│  └─ Value: Cap'n Proto FrameData (PPM binary)               │
│                                                               │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ HAL / Display Driver                                │   │
│  │ ├─ /dev/fb0 framebuffer capture                     │   │
│  │ └─ Publish → Zenoh [30 FPS target]                 │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                               │
└─────────────────────────────────────────────────────────────┘
```

## Output Formats

### macOS QuickLook

When the first frame arrives, the viewer automatically opens it in macOS QuickLook. Subsequent frames are continuously saved.

Files saved to: `~/Library/Caches/bsdOS/`

```
latest.ppm                      # Raw PPM (always saved)
frame-000030-20260606-120530.png  # Timestamped PNG (for preview)
```

Press spacebar in QuickLook to auto-advance through frames.

### Linux / Custom Viewers

For Linux, frames are saved to `/tmp/bsdos-viewer/`:

```bash
# Manual preview with ImageMagick
display /tmp/bsdos-viewer/latest.ppm

# Or ffplay
ffplay -r 15 /tmp/bsdos-viewer/latest.ppm

# Or watch with auto-refresh
watch -n 0.1 'file /tmp/bsdos-viewer/latest.ppm'
```

## Troubleshooting

### Connection Refused: `tcp/localhost:7447`

**Cause:** Broker not running or tunnel not established.

**Fix:**
```bash
# Check broker is running (on guest)
ps aux | grep broker

# Or start broker
make vm-ssh
su -m root -c '/opt/proto-src/broker/target/release/broker'

# Verify tunnel is active (on Mac)
nc -z localhost 7447 && echo "OK" || echo "FAILED"
```

### No Frames Received

**Cause:** Display driver not publishing frames, or wrong topic.

**Fix:**
```bash
# Check Zenoh topic (on guest)
/opt/proto-src/broker/target/release/broker --inspect

# Verify subscription (on Mac)
# Viewer logs should show "Subscribing to 'bsdos/display/frame'"
# If no frames after 10s, check guest logs
```

### SSH Tunnel Still Running After Exit

```bash
# Kill lingering tunnel processes
pkill -f "ssh -L 7447"

# Or use --tunnel flag which manages cleanup
./display-stream.sh --tunnel
```

### Python Module Not Found

```bash
# Ensure pip is for Python 3
python3 -m pip install --upgrade eclipse-zenoh pillow

# Check installation
python3 -c "import zenoh; print(zenoh.__version__)"
```

### Slow Frame Rate / Dropped Frames

**Cause:** Network latency, compression overhead, or slow disk (for PNG conversion).

**Fix:**
- Use local TCP connection instead of SSH tunnel (lower latency)
- Disable PNG conversion if not needed (edit `display-viewer.py`, comment `ppm_to_png()`)
- Check available disk space: `df -h ~/Library/Caches/bsdOS/`

## Development

### Zenoh Topic Schema

Published to: `bsdos/display/frame`

Frame format: Cap'n Proto binary (see `/srv/bsdos/schema.capnp`)

```capnp
struct FrameData {
  timestamp @0 :UInt64;  # nanoseconds since epoch
  width @1 :UInt16;
  height @2 :UInt16;
  format @3 :UInt8;      # 0=PPM, 1=RGBA32, etc.
  payload @4 :Data;      # raw frame bytes
}
```

### Customizing Frame Handler

Edit `display-viewer.py` and modify the `on_frame()` callback:

```python
def on_frame(sample):
    data = bytes(sample.payload)
    
    # Your custom logic here
    # e.g., decode Cap'n Proto, extract metadata, custom filters
    
    save_frame(data, frame_count[0])
```

## Performance Notes

- **PPM Format**: Raw uncompressed; ~2-8 MB per 1080p frame at 30 FPS
- **PNG Conversion**: ~50ms overhead per frame (optional, for preview)
- **Zenoh Throughput**: Can handle 30+ FPS over local TCP
- **Disk I/O**: Sustained 60-100 MB/s write for continuous 30 FPS

## License

Part of bsdOS project. See `/srv/bsdos/LICENSE` for details.

## Support

For issues or questions:
1. Check guest broker logs: `tail -f /var/log/broker.log` (on FreeBSD)
2. Enable debug in viewer: Modify `display-viewer.py` logging
3. Verify Zenoh connectivity: `./display-stream.sh --help`
