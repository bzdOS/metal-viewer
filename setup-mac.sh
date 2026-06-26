#!/bin/bash
# setup-mac.sh — One-time setup for macOS development

set -eu

echo "[setup] bsdOS Metal Viewer — macOS setup"

# Check Rust
if ! command -v cargo &> /dev/null; then
    echo "[error] Rust not found. Install from: https://rustup.rs/"
    exit 1
fi

echo "[✓] Rust: $(rustc --version)"

# Check Xcode CLI
if ! command -v xcrun &> /dev/null; then
    echo "[error] Xcode Command Line Tools not found"
    echo "        Run: xcode-select --install"
    exit 1
fi

echo "[✓] Xcode CLI: $(xcrun --version | head -1)"

# Check Metal support
if system_profiler SPDisplaysDataType | grep -q "Metal Family"; then
    METAL_VER=$(system_profiler SPDisplaysDataType | grep "Metal Family" | head -1)
    echo "[✓] Metal GPU: $METAL_VER"
else
    echo "[warning] Metal GPU not detected (very old Mac?)"
fi

# Build release binary
echo ""
echo "[build] Compiling bsdos-metal-viewer..."
cargo build --release 2>&1 | tail -20

BINARY="target/release/bsdos-metal-viewer"
if [ -f "$BINARY" ]; then
    SIZE=$(du -h "$BINARY" | cut -f1)
    echo "[✓] Binary built: $BINARY ($SIZE)"
else
    echo "[error] Build failed"
    exit 1
fi

# Create SSH tunnel helper
cat > setup-tunnel.sh << 'EOF'
#!/bin/bash
# Quick SSH tunnel setup

VM_HOST="${1:?Usage: ./setup-tunnel.sh <vm-host> [--user <user>]}"
VM_USER="${3:-freebsd}"

echo "[tunnel] Connecting to $VM_USER@$VM_HOST"
echo "[tunnel] Forwarding localhost:7447 ← $VM_HOST:7447"
echo "[tunnel] Keep this terminal open while using metal-viewer"
echo ""

ssh -L 7447:127.0.0.1:7447 "$VM_USER@$VM_HOST"
EOF

chmod +x setup-tunnel.sh
echo "[✓] SSH tunnel helper: setup-tunnel.sh"

# Create run helper
cat > run.sh << 'EOF'
#!/bin/bash

ZENOH_PEER="${ZENOH_PEER:-tcp/localhost:7447}"

echo "[viewer] bsdOS Metal Viewer"
echo "[viewer] Zenoh peer: $ZENOH_PEER"
echo "[viewer] Make sure:"
echo "  1. SSH tunnel running: ssh -L 7447:... vm-host"
echo "  2. cage WM running on VM: make vm-start-wayland"
echo "  3. screencopy sender running: make vm-start-screencopy"
echo ""

exec ./target/release/bsdos-metal-viewer
EOF

chmod +x run.sh
echo "[✓] Run helper: run.sh"

echo ""
echo "[setup] ✓ Ready!"
echo ""
echo "Next steps:"
echo "  1. In Terminal 1: ./setup-tunnel.sh <vm-host>"
echo "  2. In Terminal 2: make vm-start-wayland (on Linux host)"
echo "  3. In Terminal 3: make vm-start-screencopy (on Linux host)"
echo "  4. In Terminal 4: ./run.sh (on macOS)"
