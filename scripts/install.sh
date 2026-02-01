#!/bin/bash
# Bubbaloop Installer
# Usage: curl -sSL https://github.com/kornia/bubbaloop/releases/latest/download/install.sh | bash
#
# This script installs:
# - Zenoh router (zenohd)
# - Zenoh WebSocket bridge (zenoh-bridge-remote-api)
# - Bubbaloop daemon
# - Bubbaloop TUI
# - Bubbaloop dashboard server
# - Systemd user services for zenohd, zenoh-bridge, and bubbaloop-daemon

set -euo pipefail

REPO="kornia/bubbaloop"
ZENOH_VERSION="1.7.1"
INSTALL_DIR="$HOME/.bubbaloop"
BIN_DIR="$INSTALL_DIR/bin"
SERVICE_DIR="$HOME/.config/systemd/user"

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

# Detect architecture
detect_arch() {
    local arch
    arch=$(uname -m)
    case "$arch" in
        x86_64)  echo "x86_64-unknown-linux-gnu" ;;
        aarch64) echo "aarch64-unknown-linux-gnu" ;;
        arm64)   echo "aarch64-unknown-linux-gnu" ;;
        *)       error "Unsupported architecture: $arch" ;;
    esac
}

# Get short arch for bubbaloop releases
get_short_arch() {
    local arch
    arch=$(uname -m)
    case "$arch" in
        x86_64)  echo "amd64" ;;
        aarch64) echo "arm64" ;;
        arm64)   echo "arm64" ;;
        *)       error "Unsupported architecture: $arch" ;;
    esac
}

# Detect OS
detect_os() {
    local os
    os=$(uname -s)
    case "$os" in
        Linux)  echo "linux" ;;
        Darwin) error "macOS is not yet supported. Please build from source." ;;
        *)      error "Unsupported OS: $os" ;;
    esac
}

# Check for required dependencies
check_deps() {
    local missing=()

    if ! command -v curl &> /dev/null; then
        missing+=("curl")
    fi

    if ! command -v unzip &> /dev/null; then
        missing+=("unzip")
    fi

    if [ ${#missing[@]} -gt 0 ]; then
        error "Missing required dependencies: ${missing[*]}"
    fi
}

# Get latest release version
get_latest_version() {
    curl -sS "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/'
}

# Get latest dashboard release version (tags starting with dash-)
get_latest_dash_version() {
    curl -sS "https://api.github.com/repos/$REPO/releases" \
        | grep '"tag_name":' \
        | grep 'dash-' \
        | head -1 \
        | sed -E 's/.*"([^"]+)".*/\1/'
}

# Download file
download() {
    local url="$1"
    local dest="$2"
    info "Downloading $(basename "$dest")..."
    curl -sSL "$url" -o "$dest" || error "Failed to download: $url"
}

# Install Zenoh
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

    # Download zenohd
    local zenoh_tarball="/tmp/zenoh.tar.gz"
    local zenoh_url="https://github.com/eclipse-zenoh/zenoh/releases/download/${ZENOH_VERSION}/zenoh-${ZENOH_VERSION}-${arch}-standalone.zip"

    info "Downloading Zenoh..."
    curl -sSL "$zenoh_url" -o "/tmp/zenoh.zip" || error "Failed to download Zenoh"

    info "Extracting Zenoh..."
    unzip -o -q "/tmp/zenoh.zip" -d "$zenoh_dir"
    rm "/tmp/zenoh.zip"

    # Copy binaries
    cp "$zenoh_dir/zenohd" "$BIN_DIR/"
    chmod +x "$BIN_DIR/zenohd"

    info "Zenoh installed: $("$BIN_DIR/zenohd" --version 2>/dev/null | head -1 || echo "$ZENOH_VERSION")"
}

# Install Zenoh Bridge
install_bridge() {
    local arch="$1"
    local bridge_dir="$INSTALL_DIR/zenoh-bridge"

    step "Installing zenoh-bridge-remote-api $ZENOH_VERSION..."

    mkdir -p "$bridge_dir"

    # Download zenoh-bridge-remote-api from zenoh-ts repo
    local bridge_url="https://github.com/eclipse-zenoh/zenoh-ts/releases/download/${ZENOH_VERSION}/zenoh-ts-${ZENOH_VERSION}-${arch}-standalone.zip"

    info "Downloading zenoh-bridge-remote-api..."
    curl -sSL "$bridge_url" -o "/tmp/zenoh-bridge.zip" || error "Failed to download zenoh-bridge-remote-api"

    info "Extracting zenoh-bridge-remote-api..."
    unzip -o -q "/tmp/zenoh-bridge.zip" -d "$bridge_dir"
    rm "/tmp/zenoh-bridge.zip"

    # Copy binary
    cp "$bridge_dir/zenoh-bridge-remote-api" "$BIN_DIR/"
    chmod +x "$BIN_DIR/zenoh-bridge-remote-api"

    info "zenoh-bridge-remote-api installed"
}

# Install Bubbaloop binaries
install_bubbaloop() {
    local version="$1"
    local os="$2"
    local arch="$3"

    step "Installing Bubbaloop $version..."

    local base_url="https://github.com/$REPO/releases/download/$version"

    # Download bubbaloop (includes CLI, TUI, and daemon)
    download "$base_url/bubbaloop-$os-$arch" "$BIN_DIR/bubbaloop"
    chmod +x "$BIN_DIR/bubbaloop"

    # Save version
    echo "$version" > "$INSTALL_DIR/version"

    info "Bubbaloop $version installed"
}

# Install Bubbaloop Dashboard server (from separate dash-* release)
install_dashboard() {
    local os="$1"
    local arch="$2"

    local dash_version
    dash_version=$(get_latest_dash_version)

    if [ -z "$dash_version" ]; then
        warn "No dashboard release found, skipping bubbaloop-dash"
        return 0
    fi

    step "Installing Bubbaloop Dashboard $dash_version..."

    local base_url="https://github.com/$REPO/releases/download/$dash_version"

    # Download bubbaloop-dash (embedded dashboard server)
    if ! download "$base_url/bubbaloop-dash-$os-$arch" "$BIN_DIR/bubbaloop-dash" 2>/dev/null; then
        warn "Dashboard binary not available for $os-$arch, skipping"
        return 0
    fi
    chmod +x "$BIN_DIR/bubbaloop-dash"

    info "Bubbaloop Dashboard $dash_version installed"
}

# Setup systemd services
setup_systemd() {
    step "Setting up systemd services..."

    mkdir -p "$SERVICE_DIR"
    mkdir -p "$INSTALL_DIR/configs"

    # Create Zenoh config (compatible with Zenoh 1.6.x and 1.7.x)
    cat > "$INSTALL_DIR/zenoh.json5" << 'CONFIG'
{
  mode: "router",
  listen: {
    endpoints: ["tcp/0.0.0.0:7447"]
  }
}
CONFIG

    # Zenoh router service
    cat > "$SERVICE_DIR/bubbaloop-zenohd.service" << EOF
[Unit]
Description=Zenoh Router for Bubbaloop
After=network.target

[Service]
Type=simple
ExecStart=$BIN_DIR/zenohd -c $INSTALL_DIR/zenoh.json5
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
EOF

    # Zenoh WebSocket bridge service
    cat > "$SERVICE_DIR/bubbaloop-bridge.service" << EOF
[Unit]
Description=Zenoh WebSocket Bridge for Bubbaloop
After=bubbaloop-zenohd.service
Requires=bubbaloop-zenohd.service

[Service]
Type=simple
ExecStart=$BIN_DIR/zenoh-bridge-remote-api --ws-port 10001 -e tcp/127.0.0.1:7447
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
EOF

    # Bubbaloop daemon service
    cat > "$SERVICE_DIR/bubbaloop-daemon.service" << EOF
[Unit]
Description=Bubbaloop Daemon
After=bubbaloop-zenohd.service
Requires=bubbaloop-zenohd.service

[Service]
Type=simple
Environment="RUST_LOG=info"
ExecStart=$BIN_DIR/bubbaloop daemon
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
EOF

    # Bubbaloop dashboard service
    cat > "$SERVICE_DIR/bubbaloop-dashboard.service" << EOF
[Unit]
Description=Bubbaloop Dashboard
After=bubbaloop-bridge.service
Requires=bubbaloop-bridge.service

[Service]
Type=simple
ExecStart=$BIN_DIR/bubbaloop-dash --port 8080
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
EOF

    # Reload systemd
    systemctl --user daemon-reload

    # Enable services
    systemctl --user enable bubbaloop-zenohd.service
    systemctl --user enable bubbaloop-bridge.service
    systemctl --user enable bubbaloop-daemon.service
    systemctl --user enable bubbaloop-dashboard.service

    info "Systemd services configured"
}

# Stop existing services before upgrade (must run before copying binaries)
stop_services() {
    if systemctl --user is-active --quiet bubbaloop-daemon.service 2>/dev/null || \
       systemctl --user is-active --quiet bubbaloop-bridge.service 2>/dev/null || \
       systemctl --user is-active --quiet bubbaloop-zenohd.service 2>/dev/null || \
       systemctl --user is-active --quiet bubbaloop-dashboard.service 2>/dev/null; then
        step "Stopping existing services for upgrade..."
        systemctl --user stop bubbaloop-daemon.service 2>/dev/null || true
        systemctl --user stop bubbaloop-bridge.service 2>/dev/null || true
        systemctl --user stop bubbaloop-zenohd.service 2>/dev/null || true
        systemctl --user stop bubbaloop-dashboard.service 2>/dev/null || true
    fi
    # Clean up legacy separate daemon binary
    rm -f "$BIN_DIR/bubbaloop-daemon" 2>/dev/null || true
    # Clean up legacy non-prefixed services if present
    systemctl --user stop zenoh-bridge.service 2>/dev/null || true
    systemctl --user stop zenohd.service 2>/dev/null || true
    systemctl --user disable zenoh-bridge.service 2>/dev/null || true
    systemctl --user disable zenohd.service 2>/dev/null || true
    rm -f "$SERVICE_DIR/zenohd.service" "$SERVICE_DIR/zenoh-bridge.service" 2>/dev/null || true
}

# Start services
start_services() {
    step "Starting services..."

    # Start services
    systemctl --user start bubbaloop-zenohd.service
    sleep 1
    systemctl --user start bubbaloop-bridge.service
    systemctl --user start bubbaloop-daemon.service

    # Check status
    if systemctl --user is-active --quiet bubbaloop-zenohd.service; then
        info "bubbaloop-zenohd: running"
    else
        warn "bubbaloop-zenohd: failed to start"
    fi

    if systemctl --user is-active --quiet bubbaloop-bridge.service; then
        info "bubbaloop-bridge: running"
    else
        warn "bubbaloop-bridge: failed to start"
    fi

    if systemctl --user is-active --quiet bubbaloop-daemon.service; then
        info "bubbaloop-daemon: running"
    else
        warn "bubbaloop-daemon: failed to start"
    fi

    systemctl --user start bubbaloop-dashboard.service

    if systemctl --user is-active --quiet bubbaloop-dashboard.service; then
        info "bubbaloop-dashboard: running (http://localhost:8080)"
    else
        warn "bubbaloop-dashboard: failed to start"
    fi
}

# Add to PATH
setup_path() {
    local shell_rc=""
    if [ -f "$HOME/.bashrc" ]; then
        shell_rc="$HOME/.bashrc"
    elif [ -f "$HOME/.zshrc" ]; then
        shell_rc="$HOME/.zshrc"
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

# Enable lingering for user services to run without login
enable_linger() {
    if command -v loginctl &> /dev/null; then
        loginctl enable-linger "$USER" 2>/dev/null || true
    fi
}

main() {
    echo
    echo -e "${GREEN}╔══════════════════════════════════════╗${NC}"
    echo -e "${GREEN}║       Bubbaloop Installer            ║${NC}"
    echo -e "${GREEN}╚══════════════════════════════════════╝${NC}"
    echo

    local os arch short_arch version zenoh_arch
    os=$(detect_os)
    arch=$(detect_arch)
    short_arch=$(get_short_arch)

    info "Platform: $os ($arch)"

    check_deps

    # Get version (from arg or latest)
    version="${1:-$(get_latest_version)}"
    if [ -z "$version" ]; then
        error "Could not determine version. Check https://github.com/$REPO/releases"
    fi

    # Check for upgrade
    if [ -f "$INSTALL_DIR/version" ]; then
        local current_version
        current_version=$(cat "$INSTALL_DIR/version")
        if [ "$current_version" = "$version" ]; then
            info "Bubbaloop $version already installed. Reinstalling..."
        else
            info "Upgrading from $current_version to $version..."
        fi
    fi

    # Create directories
    mkdir -p "$BIN_DIR"
    mkdir -p "$INSTALL_DIR/configs"

    # Stop running services before overwriting binaries (upgrade path)
    stop_services

    # Install components
    install_zenoh "$arch"
    install_bridge "$arch"
    install_bubbaloop "$version" "$os" "$short_arch"
    install_dashboard "$os" "$short_arch"
    setup_systemd
    enable_linger
    start_services

    local shell_rc
    shell_rc=$(setup_path)

    echo
    echo -e "${GREEN}╔══════════════════════════════════════╗${NC}"
    echo -e "${GREEN}║         Bubbaloop is ready!          ║${NC}"
    echo -e "${GREEN}╚══════════════════════════════════════╝${NC}"
    echo

    # Auto-verify installation
    if [ -x "$BIN_DIR/bubbaloop" ]; then
        echo -e "${BLUE}Verifying installation...${NC}"
        echo
        "$BIN_DIR/bubbaloop" status 2>&1 || true
        echo
    fi

    echo -e "${YELLOW}To get started:${NC}"
    echo
    echo "  source $shell_rc   # Add bubbaloop to PATH (or open new terminal)"
    echo "  bubbaloop          # Launch the TUI"
    echo "  Open http://localhost:8080 for the dashboard"
    echo
}

main "$@"
