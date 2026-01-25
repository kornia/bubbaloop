#!/bin/bash
# Bubbaloop Installer
# Usage: curl -sSL https://github.com/kornia/bubbaloop/releases/latest/download/install.sh | bash
#
# This script installs:
# - Zenoh router (zenohd)
# - Zenoh WebSocket bridge (zenoh-bridge-remote-api)
# - Bubbaloop daemon
# - Bubbaloop TUI
# - Systemd user services for zenohd, zenoh-bridge, and bubbaloop-daemon

set -euo pipefail

REPO="kornia/bubbaloop"
ZENOH_VERSION="1.2.1"
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
    cp "$zenoh_dir/zenoh-bridge-remote-api" "$BIN_DIR/" 2>/dev/null || true
    chmod +x "$BIN_DIR/zenohd"
    chmod +x "$BIN_DIR/zenoh-bridge-remote-api" 2>/dev/null || true

    info "Zenoh installed: $("$BIN_DIR/zenohd" --version 2>/dev/null | head -1 || echo "$ZENOH_VERSION")"
}

# Install Bubbaloop binaries
install_bubbaloop() {
    local version="$1"
    local os="$2"
    local arch="$3"

    step "Installing Bubbaloop $version..."

    local base_url="https://github.com/$REPO/releases/download/$version"

    # Download daemon
    download "$base_url/bubbaloop-daemon-$os-$arch" "$BIN_DIR/bubbaloop-daemon"
    chmod +x "$BIN_DIR/bubbaloop-daemon"

    # Download TUI
    download "$base_url/bubbaloop-$os-$arch" "$BIN_DIR/bubbaloop"
    chmod +x "$BIN_DIR/bubbaloop"

    # Save version
    echo "$version" > "$INSTALL_DIR/version"

    info "Bubbaloop $version installed"
}

# Setup systemd services
setup_systemd() {
    step "Setting up systemd services..."

    mkdir -p "$SERVICE_DIR"
    mkdir -p "$INSTALL_DIR/configs"

    # Create Zenoh config
    cat > "$INSTALL_DIR/zenoh.json5" << 'CONFIG'
{
  mode: "router",
  listen: {
    endpoints: ["tcp/0.0.0.0:7447"]
  },
  plugins_search_dirs: []
}
CONFIG

    # Zenoh router service
    cat > "$SERVICE_DIR/zenohd.service" << EOF
[Unit]
Description=Zenoh Router
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
    cat > "$SERVICE_DIR/zenoh-bridge.service" << EOF
[Unit]
Description=Zenoh WebSocket Bridge
After=zenohd.service
Requires=zenohd.service

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
After=zenohd.service
Requires=zenohd.service

[Service]
Type=simple
Environment="RUST_LOG=info"
ExecStart=$BIN_DIR/bubbaloop-daemon
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
EOF

    # Reload systemd
    systemctl --user daemon-reload

    # Enable services
    systemctl --user enable zenohd.service
    systemctl --user enable zenoh-bridge.service
    systemctl --user enable bubbaloop-daemon.service

    info "Systemd services configured"
}

# Start services
start_services() {
    step "Starting services..."

    # Stop existing services first (for upgrade)
    systemctl --user stop bubbaloop-daemon.service 2>/dev/null || true
    systemctl --user stop zenoh-bridge.service 2>/dev/null || true
    systemctl --user stop zenohd.service 2>/dev/null || true

    # Start services
    systemctl --user start zenohd.service
    sleep 1
    systemctl --user start zenoh-bridge.service
    systemctl --user start bubbaloop-daemon.service

    # Check status
    if systemctl --user is-active --quiet zenohd.service; then
        info "zenohd: running"
    else
        warn "zenohd: failed to start"
    fi

    if systemctl --user is-active --quiet zenoh-bridge.service; then
        info "zenoh-bridge: running"
    else
        warn "zenoh-bridge: failed to start"
    fi

    if systemctl --user is-active --quiet bubbaloop-daemon.service; then
        info "bubbaloop-daemon: running"
    else
        warn "bubbaloop-daemon: failed to start"
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

    # Install components
    install_zenoh "$arch"
    install_bubbaloop "$version" "$os" "$short_arch"
    setup_systemd
    enable_linger
    start_services

    local shell_rc
    shell_rc=$(setup_path)

    echo
    echo -e "${GREEN}╔══════════════════════════════════════╗${NC}"
    echo -e "${GREEN}║       Installation Complete!         ║${NC}"
    echo -e "${GREEN}╚══════════════════════════════════════╝${NC}"
    echo
    echo "Installed to: $INSTALL_DIR"
    echo
    echo "Services running:"
    echo "  - zenohd (Zenoh router)"
    echo "  - zenoh-bridge (WebSocket bridge)"
    echo "  - bubbaloop-daemon (Node manager)"
    echo
    echo "Service management:"
    echo "  systemctl --user status zenohd"
    echo "  systemctl --user status bubbaloop-daemon"
    echo "  systemctl --user restart bubbaloop-daemon"
    echo
    echo -e "${YELLOW}To start the TUI:${NC}"
    echo "  source $shell_rc  # or open a new terminal"
    echo "  bubbaloop"
    echo
}

main "$@"
