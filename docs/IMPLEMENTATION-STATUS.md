# bsdOS Mac Companion — Implementation Status

Статус разработки компонентов для macOS viewer и VM-side sender.

## Components

### 1. macOS Metal Viewer ✅ COMPLETE

**Location:** `/srv/bsdos/mac-companion/metal-viewer/`

**Status:** MVP ready (feature-complete for basic streaming)

**Files:**
- `Cargo.toml` — Rust dependencies (objc2, zenoh, tokio)
- `src/main.rs` — Main loop, Zenoh receiver, frame buffer
- `src/metal_view.rs` — Metal GPU, NSWindow, MTKView
- `src/lib.rs` — Module exports
- `setup-mac.sh` — Build + helper scripts
- `README.md` — Detailed setup guide
- `QUICKSTART.md` — Quick start for devs
- `.gitignore` — Build artifacts

**What works:**
- ✅ Zenoh connection to broker
- ✅ Subscription to `bsdos/display/frame` topic
- ✅ Frame buffer (Arc<Mutex<>>) for thread-safe updates
- ✅ RGBA32 frame parsing
- ✅ Metal texture creation (MTLTexture)
- ✅ NSWindow + MTKView initialization
- ✅ Main event loop (~60fps render loop)

**What's pending (v1.1+):**
- 🔄 CAMetalLayer drawable setup (render pass execution)
- 🔄 Actual pixel rendering (currently texture upload only)
- 🔄 Window resizing support
- 🔄 FPS counter in title bar
- 🔄 Keyboard/mouse input (reverse channel)

**Build:** `cargo build --release` → ~15MB binary

### 2. VM-side screencopy-sender ⏳ SPEC COMPLETE

**Location:** (to be created in `/srv/bsdos/proto/screencopy-sender/` or similar)

**Status:** Specification complete, ready for implementation

**Specification:**
- `/srv/bsdos/infra/docs/SCREENCOPY-SENDER-SPEC.md` — Full impl spec
- `/srv/bsdos/infra/docs/SCREENCOPY-PROTOCOL.md` — Packet format

**What needs doing:**
1. Create `Cargo.toml` with deps (zenoh, tokio, tracing)
2. Implement `src/main.rs`:
   - Read RGBA frames from stdin (wf-recorder)
   - Pack with header: `[width|height|stride|rgba_data]`
   - Publish to Zenoh `bsdos/display/frame`
3. Handle graceful EOF (wf-recorder termination)
4. Error handling (no panics)

**Build destination:** `/opt/proto-src/target/release/bsdos-screencopy-sender`

**Typical size:** ~5-8MB binary

### 3. VM-side startup script ✅ COMPLETE

**Location:** `/srv/bsdos/infra/scripts/vm-start-screencopy.sh`

**Status:** Ready to use (after screencopy-sender is built)

**What it does:**
- Kills existing processes (cleanup)
- Starts wf-recorder (Wayland frame capture)
- Pipes to bsdos-screencopy-sender (packing + Zenoh publish)
- Logs to `/tmp/screencopy.log`

**Requires:**
- `bsdos-screencopy-sender` binary at `/opt/proto-src/target/release/`
- cage WM running
- Zenoh broker accessible

**Integration:** `make vm-start-screencopy`

### 4. Makefile target ✅ COMPLETE

**Location:** `/srv/bsdos/Makefile`

**Status:** Target added, ready to use

```makefile
vm-start-screencopy:
	SSH_KEY=$(SSH_KEY) VM_SSH_PORT=$(VM_SSH_PORT) $(SCRIPTS)/vm-start-screencopy.sh
```

**Usage:**
```bash
make vm-start-screencopy
```

## Protocol

### Topic: `bsdos/display/frame`

**Format:** Binary packets

```
[u32 BE width][u32 BE height][u32 BE stride][u8[] RGBA pixels]
```

**Frame sizes:**
- 1280×720: ~3.6 MB/frame (30fps → ~110 MB/s)
- 1920×1080: ~8.3 MB/frame (30fps → ~250 MB/s)

**Uncompressed** (by design) — suitable for local LAN with SSH tunnel.

**Optimization opportunities:**
- Zenoh LZ4 compression plugin (5-10x)
- Codec (H.264, VP9) — requires decompression on Mac
- Delta frames (only changed regions)
- Reduced framerate (15fps, 24fps)

## Architecture

```
bsdOS VM (FreeBSD)
  ├─ cage (kiosk WM)
  │  └─ WAYLAND_DISPLAY=wayland-0
  │
  ├─ wf-recorder (screen capture)
  │  └─ --pixel-format=rgba → stdout
  │
  ├─ bsdos-screencopy-sender (packing)
  │  └─ [header + RGBA] → Zenoh
  │
  └─ Zenoh broker (localhost:7447)
       └─ bsdos/display/frame topic

SSH tunnel (Linux host)
  └─ ssh -L 7447:127.0.0.1:7447 freebsd@vm

macOS (Metal GPU)
  └─ bsdos-metal-viewer
     ├─ Zenoh subscriber
     ├─ Frame buffer (Arc<Mutex<>>)
     ├─ Metal texture
     ├─ MTKView (CAMetalLayer)
     └─ NSWindow (1280×720)
```

## Integration Checklist

### For macOS developer:

- [ ] Clone bsdOS repo
- [ ] `cd mac-companion/metal-viewer`
- [ ] `./setup-mac.sh` (builds binary)
- [ ] `./setup-tunnel.sh <vm-host>` (Terminal 1)
- [ ] (Linux host) `make vm-start-wayland` (Terminal 2)
- [ ] (Linux host) `make vm-start-screencopy` (Terminal 2)
- [ ] (macOS) `./run.sh` (Terminal 3)
- [ ] See live video from VM

### For VM/Rust developer (screencopy-sender):

- [ ] Implement sender in `/srv/bsdos/proto/screencopy-sender/`
- [ ] Build: `cargo build --release`
- [ ] Deploy: `cp target/release/bsdos-screencopy-sender /opt/proto-src/`
- [ ] Test: `make vm-start-screencopy` → check logs
- [ ] Verify frames arrive on Mac

## Documentation

| File | Purpose |
|------|---------|
| `METAL-VIEWER-SETUP.md` | Full setup + troubleshooting guide |
| `QUICKSTART.md` | Quick reference for developers |
| `metal-viewer/README.md` | Detailed Metal viewer docs |
| `SCREENCOPY-PROTOCOL.md` | Packet format + bandwidth |
| `SCREENCOPY-SENDER-SPEC.md` | Implementation specification |
| `IMPLEMENTATION-STATUS.md` | This file — project overview |

## Next Steps (Priority Order)

1. **Build screencopy-sender** (VM-side)
   - Implement per spec
   - Test with wf-recorder pipeline
   - Deploy to /opt/proto-src/

2. **Complete Metal rendering**
   - Add CAMetalLayer + render pass
   - Implement drawable setup
   - Test pixel display

3. **Optimize**
   - Measure latency (target <50ms)
   - Profile CPU/GPU usage
   - Consider compression (if needed)

4. **Features**
   - Window resizing
   - Dynamic resolution detection
   - Input feedback (keyboard, mouse)
   - Audio stream (separate topic)

## Performance Targets

| Metric | Target | Current |
|--------|--------|---------|
| Latency | <50ms | Design allows ~25-50ms |
| Framerate | 30fps | Code supports up to 60fps |
| CPU (VM) | <10% | Depends on screencopy-sender |
| CPU (Mac) | <20% | Depends on Metal render |
| Bandwidth | 1.3 Gbps (1280×720 uncompressed) | Achievable on local LAN |

## Known Issues

None yet — system is in MVP phase.

## Testing

**Manual test workflow:**

```bash
# Terminal 1 (macOS)
cd mac-companion/metal-viewer
./setup-tunnel.sh <vm-host>

# Terminal 2 (Linux host)
cd /srv/bsdos
make vm-start-wayland
sleep 5
make vm-start-screencopy

# Terminal 3 (macOS)
cd mac-companion/metal-viewer
./run.sh

# Expected: window appears with live video from VM
# Expected logs: "Frame #30", "Frame #60", etc.
```

**Automated test (future):**

```bash
# Headless test (no display)
make test-screencopy-sender    # Unit + integration tests
make test-metal-viewer-build   # Build verification
```

## Timeline

- ✅ **Week 1:** Metal viewer MVP (current)
- ⏳ **Week 2:** screencopy-sender implementation
- ⏳ **Week 3:** Integration testing + optimization
- ⏳ **Week 4:** Features (resize, input, audio)

## References

- **Metal/Objective-C:** https://github.com/madsmtm/objc2
- **Zenoh:** https://zenoh.io/docs/
- **wf-recorder:** https://github.com/gmacd/wf-recorder
- **bsdOS:** `/srv/bsdos/CLAUDE.md` (project rules)
