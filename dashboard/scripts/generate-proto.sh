#!/bin/bash
set -e

# Create a temporary directory with the proto files
TMP_DIR=$(mktemp -d)
trap "rm -rf $TMP_DIR" EXIT

# Copy proto files to temp directory
cp ../protos/bubbaloop/header.proto "$TMP_DIR/"
cp ../protos/bubbaloop/camera.proto "$TMP_DIR/"

# Fix the import path in the temp camera.proto
sed -i 's|import "bubbaloop/header.proto";|import "header.proto";|' "$TMP_DIR/camera.proto"

# Generate the proto files
cd "$(dirname "$0")/.."
pbjs -t static-module -w es6 -I "$TMP_DIR" -o src/proto/camera.pb.js "$TMP_DIR/camera.proto"
pbts -o src/proto/camera.pb.d.ts src/proto/camera.pb.js
