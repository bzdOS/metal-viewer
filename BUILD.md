# Build Instructions for bsdos-metal-viewer

Полные инструкции по сборке и развертыванию.

## Prerequisites

### macOS (build machine)

- macOS 11.0+ (Big Sur, Monterey, Ventura, Sonoma)
- Apple Silicon (M1, M2, M3) or Intel CPU
- Metal GPU (all modern Macs have this)

### Build Tools

- Rust 1.70+ (from https://rustup.rs/)
- Xcode Command Line Tools:
  ```bash
  xcode-select --install
  ```

- Verify:
  ```bash
  rustc --version     # Should be >= 1.70
  xcrun --version     # Should work
  clang --version     # Should work
  ```

## Build Steps

### 1. Clone bsdOS repo (if needed)

```bash
git clone <repo-url> bsdOS
cd bsdOS/mac-companion/metal-viewer
```

### 2. One-time Setup

```bash
./setup-mac.sh
```

This:
- Checks Rust & Xcode
- Checks Metal support
- Builds release binary
- Creates helper scripts

### 3. Build Binary (if not using setup-mac.sh)

```bash
cargo build --release

# Binary location:
ls -lh target/release/bsdos-metal-viewer

# Should be ~15 MB
```

### 4. Optimize Binary Size (optional)

```bash
# Strip debug symbols
strip target/release/bsdos-metal-viewer

# New size: ~10 MB
ls -lh target/release/bsdos-metal-viewer
```

## Build Variants

### Release (recommended)

```bash
cargo build --release
# Optimized, ~15 MB binary
```

### Debug (for development)

```bash
cargo build
# Unoptimized, ~50 MB binary
# Includes debug symbols
```

### Custom optimization

```bash
RUSTFLAGS="-C opt-level=3 -C lto" cargo build --release
# Maximum optimization, slower build time
```

## Troubleshooting Build

### "error: no Metal SDK found"

**Solution:** Install Xcode Command Line Tools:
```bash
xcode-select --install
```

### "error: cannot find -lobjc"

**Likely:** Xcode tools not installed
```bash
xcode-select --install
# Then try again
```

### "error[E0514]: found two `zenoh` crates with the same name"

**Solution:** Clean build:
```bash
cargo clean
cargo build --release
```

### "Build hangs on dependencies"

**Solution:** Check network, try downloading again:
```bash
cargo update
cargo build --release
```

## Binary Verification

After building:

```bash
# Check it exists
ls -lh target/release/bsdos-metal-viewer

# Check it's executable
file target/release/bsdos-metal-viewer
# Should output: Mach-O 64-bit executable arm64 (or x86_64)

# Check it runs (without connecting)
./target/release/bsdos-metal-viewer --help 2>&1 | head -5
# Should error about Zenoh connection (expected)
```

## Cross-compilation

This project targets native macOS only. If building on Linux:

```bash
# NOT SUPPORTED — use macOS host
# Metal (objc2) requires Objective-C runtime at build time
```

For Linux developers: Consider using macOS VM or CI/CD.

## Installation (optional)

To install globally on macOS:

```bash
cp target/release/bsdos-metal-viewer /usr/local/bin/
chmod +x /usr/local/bin/bsdos-metal-viewer

# Run from anywhere
bsdos-metal-viewer
```

Or keep in project directory and use `./run.sh` helper.

## Build Caching

### Clean build (if needed)

```bash
cargo clean
cargo build --release
```

### Incremental rebuild

```bash
# After code changes
cargo build --release
# Only recompiles changed files
```

## Build Performance

| Action | Time |
|--------|------|
| Fresh build | 60-120 seconds |
| Clean build | 60-120 seconds |
| Incremental rebuild | 5-15 seconds |

**Factors affecting speed:**
- First-time: downloads dependencies (~20 sec)
- Network: slower download = slower build
- CPU: M1/M2 faster than Intel
- Disk: SSD faster than HDD

## Dependencies

### Rust crates

- `objc2` 0.5 — Objective-C runtime
- `objc2-app-kit` 0.2 — NSApplication, NSWindow
- `objc2-metal` 0.2 — Metal GPU
- `objc2-metal-kit` 0.2 — MTKView
- `objc2-foundation` 0.2 — NSString, NSRect
- `zenoh` 0.11 — Pub/sub messaging
- `async-std` 1.x — Async runtime
- `tokio` 1.x — Async tasks
- `tracing` 0.1 — Logging

All specified in `Cargo.toml` — no manual installation needed.

## CI/CD Integration

If building in GitHub Actions:

```yaml
name: Build Metal Viewer

on: [push, pull_request]

jobs:
  build:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - name: Build
        run: |
          cd mac-companion/metal-viewer
          cargo build --release
      - name: Upload artifact
        uses: actions/upload-artifact@v3
        with:
          name: bsdos-metal-viewer
          path: mac-companion/metal-viewer/target/release/bsdos-metal-viewer
```

## Next Steps After Building

1. **Test locally (no VM):**
   ```bash
   ZENOH_PEER=tcp/invalid-host:7447 ./target/release/bsdos-metal-viewer
   # Should fail with: "Failed to open Zenoh session"
   # This is OK — means binary is working
   ```

2. **Setup SSH tunnel:**
   ```bash
   ssh -L 7447:127.0.0.1:7447 freebsd@vm-host
   ```

3. **Run viewer:**
   ```bash
   ZENOH_PEER=tcp/localhost:7447 ./target/release/bsdos-metal-viewer
   ```

## Detailed Setup

See: [QUICKSTART.md](./QUICKSTART.md)
See: [../METAL-VIEWER-SETUP.md](../METAL-VIEWER-SETUP.md)

## References

- Rust Book: https://doc.rust-lang.org/book/
- Cargo: https://doc.rust-lang.org/cargo/
- objc2: https://github.com/madsmtm/objc2
- Metal: https://developer.apple.com/metal/
