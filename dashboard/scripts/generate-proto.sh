#!/bin/bash
set -e

# Proto source of truth: crates/bubbaloop-schemas/protos/
PROTO_DIR="../crates/bubbaloop-schemas/protos"

# Generate the proto files
cd "$(dirname "$0")/.."

# Generate ALL protos into a single file to avoid $root.bubbaloop being overwritten
# Order: header first (dependency), then camera, then weather, then daemon
npx pbjs -t static-module -w es6 -o src/proto/messages.pb.js \
  "$PROTO_DIR/header.proto" \
  "$PROTO_DIR/camera.proto" \
  "$PROTO_DIR/weather.proto" \
  "$PROTO_DIR/daemon.proto" \
  "$PROTO_DIR/system_telemetry.proto" \
  "$PROTO_DIR/network_monitor.proto"
npx pbts -o src/proto/messages.pb.d.ts src/proto/messages.pb.js

# Fix protobufjs 7.x bug that adds incorrect 'error' parameter to decode functions
sed -i 's/function decode(reader, length, error)/function decode(reader, length)/g' src/proto/messages.pb.js
sed -i 's/if (tag === error)/if (false)/g' src/proto/messages.pb.js
