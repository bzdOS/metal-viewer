# bsdOS Mac Companion — Project Index

Complete guide to all files and components for Wayland display streaming to macOS Metal.

## Quick Links

**Start here:**
- [QUICKSTART.md](metal-viewer/QUICKSTART.md) — 5-minute setup for developers
- [METAL-VIEWER-SETUP.md](METAL-VIEWER-SETUP.md) — Full integration guide with troubleshooting

**Architecture & Design:**
- [ARCHITECTURE.txt](ARCHITECTURE.txt) — System diagram, latency breakdown, dependencies
- [IMPLEMENTATION-STATUS.md](IMPLEMENTATION-STATUS.md) — Project status, checklist, timeline

## Directory Structure

```
/srv/bsdos/mac-companion/
├── metal-viewer/                    ← macOS Metal viewer (Rust + objc2)
│   ├── Cargo.toml                   [7 KB] Rust project manifest
│   ├── src/
│   │   ├── main.rs                  [14 KB] Main loop + Zenoh receiver
│   │   ├── metal_view.rs            [9 KB] Metal GPU + NSWindow
│   │   └── lib.rs                   [0.3 KB] Module exports
│   ├── README.md                    [6 KB] Setup & features
│   ├── QUICKSTART.md                [3 KB] Quick reference
│   ├── setup-mac.sh                 [2.4 KB] Build automation
│   └── .gitignore                   [0.2 KB]
│
├── ARCHITECTURE.txt                 [8 KB] System overview & details
├── METAL-VIEWER-SETUP.md            [12 KB] Full integration + troubleshooting
├── IMPLEMENTATION-STATUS.md         [10 KB] Status & project overview
├── INDEX.md                         [THIS FILE]
└── README.md                        [Existing overview]

/srv/bsdos/infra/
├── scripts/
│   └── vm-start-screencopy.sh       [2.4 KB] VM-side startup script
│
└── docs/
    ├── SCREENCOPY-PROTOCOL.md       [5 KB] Packet format & bandwidth
    └── SCREENCOPY-SENDER-SPEC.md    [7 KB] Sender implementation spec
```

## Components Overview

### 1. macOS Metal Viewer ✅ READY

**Purpose:** Display live Wayland frames on macOS using Metal GPU

**Location:** `/srv/bsdos/mac-companion/metal-viewer/`

**Status:** MVP complete (Zenoh subscriber + Metal texture working)

**Key Files:**
- `src/main.rs` — Zenoh subscriber + frame buffer
- `src/metal_view.rs` — Metal GPU + NSWindow initialization
- `setup-mac.sh` — Build + helper scripts

**Building:**
```bash
cd metal-viewer
./setup-mac.sh  # One-time: build + create helpers
```

**Running:**
```bash
./setup-tunnel.sh <vm-host>    # Terminal 1 (SSH tunnel)
./run.sh                        # Terminal 2 (start viewer)
```

### 2. VM-side Screencopy Sender ⏳ SPEC COMPLETE

**Purpose:** Capture Wayland frames, pack with header, publish via Zenoh

**Specification:** `/srv/bsdos/infra/docs/SCREENCOPY-SENDER-SPEC.md`

**Status:** Specification complete, awaiting implementation

**To implement:**
1. Create `/srv/bsdos/proto/screencopy-sender/` directory
2. Copy Cargo.toml template from spec
3. Implement src/main.rs (read stdin, pack, publish)
4. Build: `cargo build --release`
5. Deploy: `/opt/proto-src/target/release/bsdos-screencopy-sender`

### 3. VM Startup Script ✅ READY

**Purpose:** Launch wf-recorder + screencopy-sender pipeline

**Location:** `/srv/bsdos/infra/scripts/vm-start-screencopy.sh`

**Integration:**
```bash
make vm-start-screencopy  # Launches full pipeline
```

## Protocol Specification

### Topic: `bsdos/display/frame`

**Format:** Binary packets (big-endian)

```
Offset  Size    Field
0-3     4 B     width (u32)
4-7     4 B     height (u32)
8-11    4 B     stride (u32, typically width × 4)
12+     N B     RGBA32 pixel data (row-major)
```

**Example (1280×720):**
- Packet size: 12 + (1280 × 720 × 4) = 3,686,412 bytes
- At 30fps: ~110 MB/s uncompressed

**Full spec:** `/srv/bsdos/infra/docs/SCREENCOPY-PROTOCOL.md`

## Getting Started

### Prerequisites

**On macOS:**
- macOS 11.0+ (Big Sur or newer)
- Rust 1.70+
- Xcode Command Line Tools
- Metal GPU (all modern Macs)

**On VM (FreeBSD):**
- FreeBSD 14.4+
- Wayland + cage WM
- wf-recorder
- Zenoh broker

**On Linux host:**
- SSH server
- Make
- bsdOS project files

### Step 1: Build on macOS

```bash
cd /srv/bsdos/mac-companion/metal-viewer
./setup-mac.sh

# Output:
# [✓] Rust: rustc 1.75.0
# [✓] Xcode CLI: ...
# [✓] Metal GPU: Metal Family 3
# [✓] Binary built: target/release/bsdos-metal-viewer (15MB)
```

### Step 2: SSH Tunnel (macOS, Terminal 1)

```bash
cd /srv/bsdos/mac-companion/metal-viewer
./setup-tunnel.sh <vm-host>

# Keep this open for the duration
```

### Step 3: Start VM Stack (Linux, Terminal 2)

```bash
cd /srv/bsdos
make vm-start-wayland      # Starts cage + Zenoh broker
sleep 5
make vm-start-screencopy   # Starts wf-recorder + sender
```

### Step 4: Run Viewer (macOS, Terminal 3)

```bash
cd /srv/bsdos/mac-companion/metal-viewer
./run.sh

# Window appears with live video from VM
```

## Documentation Map

| Document | Purpose | Audience |
|----------|---------|----------|
| **QUICKSTART.md** | 5-min setup | Developers |
| **METAL-VIEWER-SETUP.md** | Full guide + troubleshooting | All users |
| **ARCHITECTURE.txt** | System diagram + latency | System designers |
| **IMPLEMENTATION-STATUS.md** | Project overview + checklist | Project managers |
| **metal-viewer/README.md** | Feature details | Users |
| **SCREENCOPY-PROTOCOL.md** | Packet format | Protocol developers |
| **SCREENCOPY-SENDER-SPEC.md** | Implementation details | Rust developers |

## Troubleshooting Quick Links

**"Connection refused":**
→ See METAL-VIEWER-SETUP.md § Troubleshooting

**"No frames received":**
→ See METAL-VIEWER-SETUP.md § Troubleshooting

**"Black window":**
→ See METAL-VIEWER-SETUP.md § Troubleshooting

**Want to implement screencopy-sender?**
→ See SCREENCOPY-SENDER-SPEC.md

**Want to understand the architecture?**
→ See ARCHITECTURE.txt

## File Sizes & Build Info

| File | Size | Purpose |
|------|------|---------|
| metal-viewer/Cargo.toml | 0.7 KB | Rust manifest |
| metal-viewer/src/main.rs | 14 KB | Main code |
| metal-viewer/src/metal_view.rs | 9 KB | Metal GPU |
| setup-mac.sh | 2.4 KB | Build script |
| Binary (release) | ~15 MB | Compiled executable |

**Build time:** ~30-60 seconds (depends on machine)

**Binary size:** ~15 MB (can be stripped to ~10 MB)

## Integration with bsdOS

### Makefile targets

```bash
# On Linux host, inside /srv/bsdos
make vm-start-wayland       # Start cage + Zenoh
make vm-start-screencopy    # Start wf-recorder + sender
make vm-cage-log            # Tail cage logs
make vm-ssh                 # SSH to VM
```

### Key directories

```
/srv/bsdos/
├── mac-companion/           ← Mac-side tools (this directory)
├── infra/
│   ├── scripts/
│   │   └── vm-start-screencopy.sh
│   └── docs/
│       ├── SCREENCOPY-PROTOCOL.md
│       └── SCREENCOPY-SENDER-SPEC.md
└── Makefile                 ← Integration targets
```

## Performance Targets

| Metric | Target | Notes |
|--------|--------|-------|
| Latency | <50ms | Capture to display |
| Framerate | 30fps | Limited by wf-recorder |
| CPU (VM) | <10% | wf-recorder + sender |
| CPU (Mac) | <20% | Metal render |
| Bandwidth | ~110 MB/s | Uncompressed @ 1280×720 |

## Next Steps

1. **Immediate:** Implement screencopy-sender (see SCREENCOPY-SENDER-SPEC.md)
2. **Short-term:** Complete Metal render pass (CAMetalLayer)
3. **Medium-term:** Add window resizing, optimize latency
4. **Future:** Input feedback, audio, HDR10

## References

- Metal (objc2): https://github.com/madsmtm/objc2
- Zenoh: https://zenoh.io/
- wf-recorder: https://github.com/gmacd/wf-recorder
- bsdOS rules: /srv/bsdos/CLAUDE.md

## Questions?

See the detailed documentation for your question:
- **Setup?** → QUICKSTART.md or METAL-VIEWER-SETUP.md
- **Architecture?** → ARCHITECTURE.txt
- **Implementing sender?** → SCREENCOPY-SENDER-SPEC.md
- **Protocol details?** → SCREENCOPY-PROTOCOL.md
- **Project status?** → IMPLEMENTATION-STATUS.md
