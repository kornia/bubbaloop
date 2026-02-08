#!/bin/bash
set -e

# Core protos from bubbaloop-schemas (header + daemon only)
# Node-specific protos (camera, weather, system_telemetry, network_monitor)
# are decoded dynamically via SchemaRegistry at runtime.
CORE_PROTO_DIR="../crates/bubbaloop-schemas/protos"

cd "$(dirname "$0")/.."

# Generate core proto types (header + daemon) into static module
npx pbjs -t static-module -w es6 \
  -p "$CORE_PROTO_DIR" \
  -o src/proto/messages.pb.js \
  "$CORE_PROTO_DIR/header.proto" \
  "$CORE_PROTO_DIR/daemon.proto"
npx pbts -o src/proto/messages.pb.d.ts src/proto/messages.pb.js

# Fix protobufjs 7.x bug that adds incorrect 'error' parameter to decode functions
sed -i 's/function decode(reader, length, error)/function decode(reader, length)/g' src/proto/messages.pb.js
sed -i 's/if (tag === error)/if (false)/g' src/proto/messages.pb.js
