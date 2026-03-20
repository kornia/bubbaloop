#!/bin/bash
# Bubbaloop macOS Installer
# Usage: ./scripts/install-macos.sh [--skip-build]
#
# This script installs:
# - Zenoh router (zenohd)
# - Zenoh WebSocket bridge (zenoh-bridge-remote-api)
# - Bubbaloop daemon + CLI (built from source)
# - Bubbaloop dashboard server (built from source, optional)
# - launchd user agents to manage the above
#
# No prebuilt macOS binaries exist yet — this script builds from source
# using a macOS-compatible release profile (no linker-plugin-lto).

set -euo pipefail

REPO="kornia/bubbaloop"
ZENOH_VERSION="1.7.1"
INSTALL_DIR="$HOME/.bubbaloop"
BIN_DIR="$INSTALL_DIR/bin"
LAUNCH_AGENTS_DIR="$HOME/Library/LaunchAgents"

PLIST_ZENOH="$LAUNCH_AGENTS_DIR/com.bubbaloop.zenohd.plist"
PLIST_BRIDGE="$LAUNCH_AGENTS_DIR/com.bubbaloop.zenoh-bridge-remote-api.plist"
PLIST_DAEMON="$LAUNCH_AGENTS_DIR/com.bubbaloop.daemon.plist"
PLIST_DASH="$LAUNCH_AGENTS_DIR/com.bubbaloop.dashboard.plist"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

info() { echo -e "${GREEN}[INFO]${NC} $1"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
error() { echo -e "${RED}[ERROR]${NC} $1"; exit 1; }
step() { echo -e "${BLUE}[STEP]${NC} $1"; }

detect_os() {
    local os
    os=$(uname -s)
    case "$os" in
        Darwin) echo "darwin" ;;
        *)      error "This installer is for macOS only. For Linux, use scripts/install.sh." ;;
    esac
}

detect_arch() {
    local arch
    arch=$(uname -m)
    case "$arch" in
        x86_64)  echo "x86_64-apple-darwin" ;;
        arm64)   echo "aarch64-apple-darwin" ;;
        aarch64) echo "aarch64-apple-darwin" ;;
        *)       error "Unsupported architecture: $arch" ;;
    esac
}

check_deps() {
    local missing=()

    if ! command -v curl >/dev/null 2>&1; then
        missing+=("curl")
    fi
    if ! command -v unzip >/dev/null 2>&1; then
        missing+=("unzip")
    fi
    if ! command -v cargo >/dev/null 2>&1; then
        missing+=("cargo (install via https://rustup.rs)")
    fi

    if [ ${#missing[@]} -gt 0 ]; then
        error "Missing required dependencies: ${missing[*]}"
    fi
}

download() {
    local url="$1"
    local dest="$2"
    info "Downloading $(basename "$dest")..."
    curl -sSL "$url" -o "$dest" || error "Failed to download: $url"
}

# ── Zenoh ────────────────────────────────────────────────────────────

install_zenoh() {
    local arch="$1"
    local zenoh_dir="$INSTALL_DIR/zenoh"

    if [ -f "$BIN_DIR/zenohd" ]; then
        local current_version
        current_version=$("$BIN_DIR/zenohd" --version 2>/dev/null | head -1 | awk '{print $2}' || echo "unknown")
        info "Zenoh $current_version already installed, upgrading to $ZENOH_VERSION..."
    fi

    step "Installing Zenoh $ZENOH_VERSION..."
    mkdir -p "$zenoh_dir"

    local zip="/tmp/zenoh.zip"
    local url="https://github.com/eclipse-zenoh/zenoh/releases/download/${ZENOH_VERSION}/zenoh-${ZENOH_VERSION}-${arch}-standalone.zip"

    info "Downloading Zenoh..."
    curl -sSL "$url" -o "$zip" || error "Failed to download Zenoh"

    info "Extracting Zenoh..."
    unzip -o -q "$zip" -d "$zenoh_dir"
    rm -f "$zip"

    cp "$zenoh_dir/zenohd" "$BIN_DIR/"
    chmod +x "$BIN_DIR/zenohd"

    info "Zenoh installed: $("$BIN_DIR/zenohd" --version 2>/dev/null | head -1 || echo "$ZENOH_VERSION")"
}

install_bridge() {
    local arch="$1"
    local bridge_dir="$INSTALL_DIR/zenoh-bridge"

    step "Installing zenoh-bridge-remote-api $ZENOH_VERSION..."
    mkdir -p "$bridge_dir"

    local zip="/tmp/zenoh-bridge.zip"
    local url="https://github.com/eclipse-zenoh/zenoh-ts/releases/download/${ZENOH_VERSION}/zenoh-ts-${ZENOH_VERSION}-${arch}-standalone.zip"

    info "Downloading zenoh-bridge-remote-api..."
    curl -sSL "$url" -o "$zip" || error "Failed to download zenoh-bridge-remote-api"

    info "Extracting zenoh-bridge-remote-api..."
    unzip -o -q "$zip" -d "$bridge_dir"
    rm -f "$zip"

    cp "$bridge_dir/zenoh-bridge-remote-api" "$BIN_DIR/"
    chmod +x "$BIN_DIR/zenoh-bridge-remote-api"

    info "zenoh-bridge-remote-api installed"
}

# ── Build from source ────────────────────────────────────────────────

find_repo_root() {
    local dir
    dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
    if [ -f "$dir/Cargo.toml" ]; then
        echo "$dir"
    else
        error "Cannot find repo root (expected Cargo.toml in $dir). Run this script from the bubbaloop repo."
    fi
}

build_bubbaloop() {
    local repo_root
    repo_root=$(find_repo_root)

    step "Building Bubbaloop from source (macOS-compatible profile)..."
    info "Repo root: $repo_root"

    # The workspace Cargo.toml uses [profile.release] with lto=true and
    # opt-level="z" which triggers linker-plugin-lto — unsupported by the
    # default macOS linker. Override with CARGO_PROFILE_RELEASE_LTO=false.
    (
        cd "$repo_root"
        CARGO_PROFILE_RELEASE_LTO=false \
        cargo build --release -p bubbaloop 2>&1
    ) || error "cargo build failed. Check the output above."

    cp "$repo_root/target/release/bubbaloop" "$BIN_DIR/bubbaloop"
    chmod +x "$BIN_DIR/bubbaloop"

    if [ -f "$repo_root/target/release/bubbaloop-dash" ]; then
        cp "$repo_root/target/release/bubbaloop-dash" "$BIN_DIR/bubbaloop-dash"
        chmod +x "$BIN_DIR/bubbaloop-dash"
        info "bubbaloop-dash installed"
    else
        warn "bubbaloop-dash not found in build output (dashboard feature may be missing), skipping"
    fi

    local version
    version=$("$BIN_DIR/bubbaloop" --version 2>/dev/null || echo "dev")
    echo "$version" > "$INSTALL_DIR/version"
    info "Bubbaloop built and installed ($version)"
}

# ── Configuration ────────────────────────────────────────────────────

setup_zenoh_config() {
    step "Setting up Zenoh configuration..."

    mkdir -p "$INSTALL_DIR/zenoh"
    mkdir -p "$INSTALL_DIR/configs"
    mkdir -p "$INSTALL_DIR/nodes"

    cat > "$INSTALL_DIR/zenoh/zenohd.json5" << 'CONFIG'
{
  mode: "router",
  listen: {
    endpoints: ["tcp/127.0.0.1:7447"]
  },
  scouting: {
    multicast: {
      enabled: false
    },
    gossip: {
      enabled: false
    }
  }
}
CONFIG

    cp "$INSTALL_DIR/zenoh/zenohd.json5" "$INSTALL_DIR/zenoh.json5"
    info "Zenoh config created at $INSTALL_DIR/zenoh/zenohd.json5"
}

setup_marketplace() {
    step "Setting up marketplace..."

    cat > "$INSTALL_DIR/sources.json" << 'EOF'
{
  "sources": [
    {
      "name": "Official Nodes",
      "path": "kornia/bubbaloop-nodes-official",
      "type": "builtin",
      "enabled": true
    }
  ]
}
EOF

    info "Marketplace configured with official nodes registry"
}

# ── launchd services ─────────────────────────────────────────────────

stop_services() {
    step "Stopping existing launchd agents (if any)..."

    for plist in "$PLIST_DASH" "$PLIST_DAEMON" "$PLIST_BRIDGE" "$PLIST_ZENOH"; do
        if [ -f "$plist" ]; then
            launchctl unload -w "$plist" 2>/dev/null || true
        fi
    done
}

setup_launchd() {
    step "Configuring launchd agents..."
    mkdir -p "$LAUNCH_AGENTS_DIR"
    mkdir -p "$INSTALL_DIR/logs"

    cat > "$PLIST_ZENOH" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
  <dict>
    <key>Label</key>
    <string>com.bubbaloop.zenohd</string>
    <key>ProgramArguments</key>
    <array>
      <string>$BIN_DIR/zenohd</string>
      <string>-c</string>
      <string>$INSTALL_DIR/zenoh/zenohd.json5</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>$INSTALL_DIR/logs/zenohd.log</string>
    <key>StandardErrorPath</key>
    <string>$INSTALL_DIR/logs/zenohd.err.log</string>
  </dict>
</plist>
EOF

    cat > "$PLIST_BRIDGE" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
  <dict>
    <key>Label</key>
    <string>com.bubbaloop.zenoh-bridge-remote-api</string>
    <key>ProgramArguments</key>
    <array>
      <string>$BIN_DIR/zenoh-bridge-remote-api</string>
      <string>--ws-port</string>
      <string>10001</string>
      <string>-e</string>
      <string>tcp/127.0.0.1:7447</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>$INSTALL_DIR/logs/zenoh-bridge.log</string>
    <key>StandardErrorPath</key>
    <string>$INSTALL_DIR/logs/zenoh-bridge.err.log</string>
  </dict>
</plist>
EOF

    cat > "$PLIST_DAEMON" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
  <dict>
    <key>Label</key>
    <string>com.bubbaloop.daemon</string>
    <key>ProgramArguments</key>
    <array>
      <string>$BIN_DIR/bubbaloop</string>
      <string>daemon</string>
    </array>
    <key>EnvironmentVariables</key>
    <dict>
      <key>RUST_LOG</key>
      <string>info</string>
    </dict>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>$INSTALL_DIR/logs/bubbaloop-daemon.log</string>
    <key>StandardErrorPath</key>
    <string>$INSTALL_DIR/logs/bubbaloop-daemon.err.log</string>
  </dict>
</plist>
EOF

    cat > "$PLIST_DASH" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
  <dict>
    <key>Label</key>
    <string>com.bubbaloop.dashboard</string>
    <key>ProgramArguments</key>
    <array>
      <string>$BIN_DIR/bubbaloop-dash</string>
      <string>--port</string>
      <string>8080</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>$INSTALL_DIR/logs/bubbaloop-dashboard.log</string>
    <key>StandardErrorPath</key>
    <string>$INSTALL_DIR/logs/bubbaloop-dashboard.err.log</string>
  </dict>
</plist>
EOF

    info "launchd agents written to $LAUNCH_AGENTS_DIR"
}

start_services() {
    step "Loading launchd agents..."

    launchctl load -w "$PLIST_ZENOH" 2>/dev/null || true
    sleep 1
    launchctl load -w "$PLIST_BRIDGE" 2>/dev/null || true
    launchctl load -w "$PLIST_DAEMON" 2>/dev/null || true

    if [ -x "$BIN_DIR/bubbaloop-dash" ]; then
        launchctl load -w "$PLIST_DASH" 2>/dev/null || true
    fi

    info "Services started. Logs: $INSTALL_DIR/logs/"
}

setup_path() {
    local shell_rc=""
    if [ -f "$HOME/.zshrc" ]; then
        shell_rc="$HOME/.zshrc"
    elif [ -f "$HOME/.bashrc" ]; then
        shell_rc="$HOME/.bashrc"
    else
        shell_rc="$HOME/.profile"
    fi

    if ! grep -q "$BIN_DIR" "$shell_rc" 2>/dev/null; then
        echo "" >> "$shell_rc"
        echo "# Bubbaloop" >> "$shell_rc"
        echo "export PATH=\"$BIN_DIR:\$PATH\"" >> "$shell_rc"
        info "Added $BIN_DIR to PATH in $shell_rc"
    fi

    echo "$shell_rc"
}

# ── Main ─────────────────────────────────────────────────────────────

usage() {
    echo "Usage: $0 [--skip-build]"
    echo
    echo "Options:"
    echo "  --skip-build   Skip cargo build (use if you already built manually)"
    exit 0
}

main() {
    local skip_build=false
    for arg in "$@"; do
        case "$arg" in
            --skip-build) skip_build=true ;;
            --help|-h)    usage ;;
        esac
    done

    echo
    echo -e "${GREEN}╔══════════════════════════════════════╗${NC}"
    echo -e "${GREEN}║      Bubbaloop macOS Installer       ║${NC}"
    echo -e "${GREEN}╚══════════════════════════════════════╝${NC}"
    echo

    local os arch
    os=$(detect_os)
    arch=$(detect_arch)

    info "Platform: $os ($arch)"

    check_deps

    mkdir -p "$BIN_DIR"
    mkdir -p "$INSTALL_DIR/configs"

    stop_services

    install_zenoh "$arch"
    install_bridge "$arch"

    if [ "$skip_build" = true ]; then
        if [ ! -x "$BIN_DIR/bubbaloop" ]; then
            error "No bubbaloop binary found at $BIN_DIR/bubbaloop. Remove --skip-build to build from source."
        fi
        info "Skipping build (--skip-build)"
    else
        build_bubbaloop
    fi

    setup_zenoh_config
    setup_marketplace
    setup_launchd
    start_services

    local shell_rc
    shell_rc=$(setup_path)

    echo
    echo -e "${GREEN}╔══════════════════════════════════════╗${NC}"
    echo -e "${GREEN}║         Bubbaloop is ready!          ║${NC}"
    echo -e "${GREEN}╚══════════════════════════════════════╝${NC}"
    echo

    if [ -x "$BIN_DIR/bubbaloop" ]; then
        echo -e "${BLUE}Verifying installation...${NC}"
        echo
        "$BIN_DIR/bubbaloop" status 2>&1 || true
        echo
    fi

    echo -e "${YELLOW}To get started:${NC}"
    echo
    echo "  source $shell_rc   # Add bubbaloop to PATH (or open new terminal)"
    echo "  bubbaloop status   # Check services"
    echo "  bubbaloop daemon   # Run daemon directly"
    echo "  Open http://localhost:8080 for the dashboard"
    echo
}

main "$@"
