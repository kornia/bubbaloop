#!/bin/bash
# Bubbaloop Installer
# Usage: curl -sSL https://github.com/kornia/bubbaloop/releases/latest/download/install.sh | bash

set -euo pipefail

REPO="kornia/bubbaloop"
INSTALL_DIR="$HOME/.bubbaloop"
BIN_DIR="$INSTALL_DIR/bin"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

info() { echo -e "${GREEN}[INFO]${NC} $1"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
error() { echo -e "${RED}[ERROR]${NC} $1"; exit 1; }

# Detect architecture
detect_arch() {
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
    if ! command -v node &> /dev/null; then
        warn "Node.js not found. The TUI requires Node.js 20+."
        warn "Install with: curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash - && sudo apt-get install -y nodejs"
    else
        local node_version
        node_version=$(node --version | cut -d'v' -f2 | cut -d'.' -f1)
        if [ "$node_version" -lt 20 ]; then
            warn "Node.js version $node_version detected. Version 20+ recommended."
        fi
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

main() {
    info "Bubbaloop Installer"
    echo

    local os arch version
    os=$(detect_os)
    arch=$(detect_arch)

    info "Detected: $os-$arch"

    check_deps

    # Get version (from arg or latest)
    version="${1:-$(get_latest_version)}"
    if [ -z "$version" ]; then
        error "Could not determine version. Check https://github.com/$REPO/releases"
    fi
    info "Version: $version"

    # Create directories
    mkdir -p "$BIN_DIR"

    # Download assets
    local base_url="https://github.com/$REPO/releases/download/$version"

    # Download daemon
    download "$base_url/bubbaloop-daemon-$os-$arch" "$BIN_DIR/bubbaloop-daemon"
    chmod +x "$BIN_DIR/bubbaloop-daemon"

    # Download and extract TUI
    local tui_tarball="/tmp/bubbaloop.tar.gz"
    download "$base_url/bubbaloop.tar.gz" "$tui_tarball"

    info "Extracting TUI..."
    mkdir -p "$INSTALL_DIR/tui"
    tar -xzf "$tui_tarball" -C "$INSTALL_DIR/tui"
    rm "$tui_tarball"

    # Install TUI dependencies
    info "Installing TUI dependencies..."
    cd "$INSTALL_DIR/tui"
    npm install --production --silent 2>/dev/null || warn "npm install failed. Run manually: cd $INSTALL_DIR/tui && npm install"

    # Create wrapper script
    cat > "$BIN_DIR/bubbaloop" << 'WRAPPER'
#!/bin/bash
cd "$HOME/.bubbaloop/tui"
exec node --experimental-wasm-modules dist/cli.js "$@"
WRAPPER
    chmod +x "$BIN_DIR/bubbaloop"

    # Add to PATH
    local shell_rc=""
    if [ -n "${BASH_VERSION:-}" ]; then
        shell_rc="$HOME/.bashrc"
    elif [ -n "${ZSH_VERSION:-}" ]; then
        shell_rc="$HOME/.zshrc"
    else
        shell_rc="$HOME/.profile"
    fi

    local path_line="export PATH=\"$BIN_DIR:\$PATH\""
    if ! grep -q "$BIN_DIR" "$shell_rc" 2>/dev/null; then
        echo "" >> "$shell_rc"
        echo "# Bubbaloop" >> "$shell_rc"
        echo "$path_line" >> "$shell_rc"
        info "Added $BIN_DIR to PATH in $shell_rc"
    fi

    echo
    info "Installation complete!"
    echo
    echo "Installed to: $INSTALL_DIR"
    echo "  - bubbaloop        (TUI)"
    echo "  - bubbaloop-daemon (Node manager)"
    echo
    echo "To start using Bubbaloop:"
    echo "  1. Restart your terminal or run: source $shell_rc"
    echo "  2. Start Zenoh router: zenohd &"
    echo "  3. Run: bubbaloop"
    echo
    warn "Note: Zenoh (zenohd) must be installed separately."
    echo "      See: https://zenoh.io/docs/getting-started/installation/"
}

main "$@"
