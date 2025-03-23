#!/bin/bash

# Stop the script if any command fails
set -e

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"

# Parse command line arguments
while getopts "r:u:" opt; do
  case $opt in
    r) TARGET_IP="$OPTARG"  # Target IP
    ;;
    u) TARGET_USER="$OPTARG"  # Target user
    ;;
  esac
done

# Check if required arguments are provided
if [ -z "$TARGET_IP" ] || [ -z "$TARGET_USER" ]; then
  echo "Usage: $0 -r <target-ip> -u <target-user>"
  exit 1
fi

# Configuration
BINARY_NAME="serve"
DEPLOY_ARCH="aarch64-unknown-linux-gnu"
LOCAL_FOLDER="/tmp/deploy"
TARGET_PATH="/home/$TARGET_USER/deploy"

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Function to print status
print_status() {
    echo -e "${GREEN}==> ${1}${NC}"
}

print_error() {
    echo -e "${RED}==> ERROR: ${1}${NC}"
    exit 1
}

# Check if cross is installed
if ! command -v cross &> /dev/null; then
    print_error "cross is not installed. Install it with: cargo install cross"
fi

rm -rf $LOCAL_FOLDER
mkdir -p $LOCAL_FOLDER

# Build the release binary
print_status "Building release binary for aarch64..."
cross build --target $DEPLOY_ARCH --release -v --bin $BINARY_NAME || print_error "Build failed"
rsync -a target/$DEPLOY_ARCH/release/$BINARY_NAME $LOCAL_FOLDER

# Check if binary exists
if [ ! -f "target/$DEPLOY_ARCH/release/$BINARY_NAME" ]; then
    print_error "Binary not found after build"
fi

# copy useful scripts
cp -p scripts/run_serve.sh $LOCAL_FOLDER

# Copy to remote machine
print_status "Copying to $TARGET_USER@$TARGET_IP:$TARGET_PATH..."

ssh $TARGET_USER@$TARGET_IP "mkdir -p $TARGET_PATH"
rsync -a --delete $LOCAL_FOLDER $TARGET_USER@$TARGET_IP:~/

print_status "Deploy completed successfully!"