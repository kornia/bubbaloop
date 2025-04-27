#!/bin/bash

# Colors for better readability
GREEN='\033[0;32m'
NC='\033[0m' # No Color
RED='\033[0;31m'

echo -e "${GREEN}Installing dependencies...${NC}"

# Use the existing install_deps.sh script
./scripts/install_deps.sh

# Install Rust if not already installed
if ! command -v rustc &> /dev/null; then
    echo -e "${GREEN}Installing Rust...${NC}"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source $HOME/.cargo/env
else
    echo -e "${GREEN}Rust is already installed.${NC}"
fi

# Install Just if not already installed
if ! command -v just &> /dev/null; then
    echo -e "${GREEN}Installing Just...${NC}"
    cargo install just
else
    echo -e "${GREEN}Just is already installed.${NC}"
fi

# Assume we're already in the project directory
REPO_DIR=$(pwd)
echo -e "${GREEN}Building the project...${NC}"
if ! just build; then
    echo -e "${RED}Build failed! Aborting installation.${NC}"
    exit 1
fi

# Remove previous service if it exists
SERVICE_FILE="/etc/systemd/system/bubbaloop.service"
if [ -f "$SERVICE_FILE" ]; then
    echo -e "${GREEN}Stopping and removing previous service...${NC}"
    sudo systemctl stop bubbaloop.service
    sudo systemctl disable bubbaloop.service
    sudo rm $SERVICE_FILE
    sudo systemctl daemon-reload
fi

# Install binaries in a loop
BUBBALOOP_INSTALL_DIR=/usr/local/bin
BINARIES=("serve" "bubbaloop")

for binary in "${BINARIES[@]}"; do
    echo -e "${GREEN}Installing $binary binary to $BUBBALOOP_INSTALL_DIR...${NC}"
    if [ -f "$REPO_DIR/target/release/$binary" ]; then
        sudo cp "$REPO_DIR/target/release/$binary" "$BUBBALOOP_INSTALL_DIR/"
        sudo chmod +x "$BUBBALOOP_INSTALL_DIR/$binary"
        echo -e "${GREEN}$binary installed successfully.${NC}"
    else
        echo -e "${RED}$binary not found in target/release! Aborting installation.${NC}"
        exit 1
    fi
done

# Create a systemd service file
echo -e "${GREEN}Creating systemd service...${NC}"
sudo tee $SERVICE_FILE > /dev/null << EOL
[Unit]
Description=Bubbaloop - AI & Robotics Service
After=network.target

[Service]
ExecStart=$BUBBALOOP_INSTALL_DIR/serve
WorkingDirectory=$REPO_DIR
User=$USER
Restart=on-failure
RestartSec=5
Environment=RUST_LOG=debug

# Ensure access to /tmp directory
PrivateTmp=false
ReadWritePaths=/tmp
ProtectSystem=false

[Install]
WantedBy=default.target
EOL

# Enable and start the service
echo -e "${GREEN}Enabling and starting the service...${NC}"
sudo systemctl daemon-reload
sudo systemctl enable bubbaloop.service
sudo systemctl start bubbaloop.service

echo -e "${GREEN}Installation complete!${NC}"
echo -e "${GREEN}Service status:${NC}"
sudo systemctl status bubbaloop.service --no-pager

echo -e "${GREEN}You can check the logs with: sudo journalctl -u bubbaloop.service -f${NC}"