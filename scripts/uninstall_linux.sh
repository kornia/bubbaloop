#!/bin/bash

# Colors for better readability
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${GREEN}Uninstalling Bubbaloop...${NC}"

# Stop and remove the systemd service
SERVICE_FILE="/etc/systemd/system/bubbaloop.service"
if [ -f "$SERVICE_FILE" ]; then
    echo -e "${GREEN}Stopping and removing service...${NC}"
    sudo systemctl stop bubbaloop.service
    sudo systemctl disable bubbaloop.service
    sudo rm $SERVICE_FILE
    sudo systemctl daemon-reload
    echo -e "${GREEN}Service removed successfully.${NC}"
else
    echo -e "${RED}Service not found, skipping service removal.${NC}"
fi

# Remove binaries using a loop
BUBBALOOP_INSTALL_DIR=/usr/local/bin
BINARIES=("bubbaloop" "serve")

for binary in "${BINARIES[@]}"; do
    if [ -f "$BUBBALOOP_INSTALL_DIR/$binary" ]; then
        echo -e "${GREEN}Removing $binary binary...${NC}"
        sudo rm $BUBBALOOP_INSTALL_DIR/$binary
        echo -e "${GREEN}$binary binary removed successfully.${NC}"
    else
        echo -e "${RED}$binary binary not found, skipping removal.${NC}"
    fi
done

echo -e "${GREEN}Uninstallation complete!${NC}"