#!/bin/bash
set -e

# Create a temporary directory with the proto files
TMP_DIR=$(mktemp -d)
trap "rm -rf $TMP_DIR" EXIT

# Copy proto files to temp directory
cp ../protos/bubbaloop/header.proto "$TMP_DIR/"
cp ../protos/bubbaloop/camera.proto "$TMP_DIR/"
cp ../protos/bubbaloop/weather.proto "$TMP_DIR/"
cp ../protos/bubbaloop/daemon.proto "$TMP_DIR/"

# Fix the import path in the temp proto files
sed -i 's|import "bubbaloop/header.proto";|import "header.proto";|' "$TMP_DIR/camera.proto"
sed -i 's|import "bubbaloop/header.proto";|import "header.proto";|' "$TMP_DIR/weather.proto"

# Generate the proto files
cd "$(dirname "$0")/.."

# Generate ALL protos into a single file to avoid $root.bubbaloop being overwritten
# Order: header first (dependency), then camera, then weather, then daemon
npx pbjs -t static-module -w es6 -o src/proto/messages.pb.js \
  "$TMP_DIR/header.proto" \
  "$TMP_DIR/camera.proto" \
  "$TMP_DIR/weather.proto" \
  "$TMP_DIR/daemon.proto"
npx pbts -o src/proto/messages.pb.d.ts src/proto/messages.pb.js

# Fix protobufjs 7.x bug that adds incorrect 'error' parameter to decode functions
sed -i 's/function decode(reader, length, error)/function decode(reader, length)/g' src/proto/messages.pb.js
sed -i 's/if (tag === error)/if (false)/g' src/proto/messages.pb.js
