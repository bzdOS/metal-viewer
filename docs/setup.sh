#!/bin/bash
# bsdOS Display Viewer - Quick Setup
# Installs dependencies on macOS or Linux

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

echo "bsdOS Display Viewer Setup"
echo "============================"
echo ""

# Detect OS
if [[ "$OSTYPE" == "darwin"* ]]; then
    echo "[setup] Detected macOS"
    PLATFORM="macos"
elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
    echo "[setup] Detected Linux"
    PLATFORM="linux"
else
    echo "ERROR: Unsupported OS: $OSTYPE"
    exit 1
fi

# Check Python 3
if ! command -v python3 &>/dev/null; then
    echo "ERROR: python3 not found"
    if [[ "$PLATFORM" == "macos" ]]; then
        echo "Install with: brew install python3"
    else
        echo "Install with: sudo apt-get install python3 python3-pip"
    fi
    exit 1
fi

PYTHON_VERSION=$(python3 --version 2>&1 | awk '{print $2}')
echo "[setup] Python $PYTHON_VERSION found"

# Install Python dependencies
echo ""
echo "[setup] Installing Python packages from requirements.txt..."
pip3 install --upgrade pip

if pip3 install -r "$SCRIPT_DIR/requirements.txt"; then
    echo "[setup] ✓ Python packages installed"
else
    echo "ERROR: Failed to install Python packages"
    exit 1
fi

# Optional: Install ffmpeg for advanced viewing
echo ""
echo "[setup] Optional: Install ffmpeg for advanced video playback"
if [[ "$PLATFORM" == "macos" ]]; then
    if ! command -v ffplay &>/dev/null; then
        echo "  Install with: brew install ffmpeg"
    else
        echo "  ✓ ffmpeg already installed"
    fi
elif [[ "$PLATFORM" == "linux" ]]; then
    if ! command -v ffplay &>/dev/null; then
        echo "  Install with: sudo apt-get install ffmpeg"
    else
        echo "  ✓ ffmpeg already installed"
    fi
fi

# Test connection (optional)
echo ""
read -p "[setup] Test Zenoh connection? (requires broker running) (y/n) " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    echo "[setup] Testing connection to tcp/localhost:7447..."
    if timeout 5 python3 -c "import zenoh; s = zenoh.open(zenoh.Config().insert_json5('connect/endpoints', '[\"tcp/localhost:7447\"]')); print('[setup] ✓ Connection successful'); s.close()" 2>/dev/null; then
        :
    else
        echo "[setup] ⚠ Could not connect (broker may not be running)"
        echo "[setup]   Start broker on guest: make vm-start && make vm-ssh && su -m root -c '/opt/proto-src/broker/target/release/broker'"
    fi
fi

echo ""
echo "Setup Complete!"
echo ""
echo "Next steps:"
echo "  1. Start Zenoh broker on FreeBSD guest:"
echo "     make vm-start && make vm-wait"
echo "     ssh -p 2222 freebsd@localhost"
echo "     su -m root -c '/opt/proto-src/broker/target/release/broker'"
echo ""
echo "  2. Run display viewer:"
echo "     cd $SCRIPT_DIR"
echo "     python3 display-viewer.py"
echo ""
echo "  Or use the wrapper script:"
echo "     ./display-stream.sh"
echo "     ./display-stream.sh --tunnel  # with SSH tunnel"
echo ""
