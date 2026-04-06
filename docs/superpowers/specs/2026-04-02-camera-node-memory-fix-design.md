# Camera Node Memory Fix & Dev Workflow

**Date:** 2026-04-02
**Status:** Approved
**Author:** edgarriba + Claude

## Problem

The `rtsp-camera` node in `kornia/bubbaloop-nodes-official` has a memory leak that causes RAM to blow up. Root cause: an unused `VideoH264Decoder` creates an **unbounded flume channel** for decoded RGBA frames. The GStreamer decoder callback pushes every decoded frame (~8.3MB at 1920x1080 RGBA) into this channel, but no consumer ever reads from it. At 30fps, this leaks ~250MB/sec.

## Architecture Decision

**Remove the decoder entirely from the camera node.** The camera node is a Layer 1 (Streaming) source — its job is to capture RTSP and publish compressed H264 via Zenoh. Decoding belongs in downstream processing nodes (Layer 2) that each decode to their own resolution/format requirements. The dashboard already decodes H264 client-side via WebCodecs.

### Why this is the right boundary

- **Single responsibility:** capture + publish compressed. No decoding work the node doesn't own.
- **Zero-copy on Jetson:** H264 passthrough = near-zero CPU. The RGBA decode was wasted.
- **Fan-out:** dashboard, recording, processing nodes all subscribe to compressed topic independently.
- **Consumer decides decode params:** processing node wants 320x240, dashboard wants 1080p — each decodes locally.

## Fix Scope

Changes to `kornia/bubbaloop-nodes-official/rtsp-camera/`:

1. **Delete `src/h264_decode.rs`** — entire module (decoder pipeline, `RawFrame`, `DecoderBackend`)
2. **Update `src/lib.rs`** — remove `h264_decode` module and `DecoderBackend` re-export
3. **Simplify `src/rtsp_camera_node.rs`**:
   - Remove `VideoH264Decoder` and `DecoderBackend` imports
   - Remove `decoder` creation in `run()`
   - Remove `decoder.push()` call in `compressed_task`
   - Remove `decoder` parameter from `compressed_task` signature
   - Remove `decoder.close()` from shutdown cleanup
4. **Simplify `src/config.rs`**:
   - Remove `DecoderBackend` enum and its `From` impl
   - Remove `decoder` field from `Config` struct
5. **Update `Cargo.toml`** — remove `gstreamer-video` dependency (only needed for decoded frame caps)
6. **Update `config.yaml`** — remove `decoder` field if present

### Files untouched
- `src/main.rs` — no decoder references
- `src/h264_capture.rs` — capture pipeline is correct (bounded appsink: `max-buffers=2 drop=true`)
- `src/proto.rs` — protobuf types unchanged

## Dev Workflow Setup

1. **Clone:** `git clone git@github.com:kornia/bubbaloop-nodes-official.git ~/bubbaloop-nodes-official`
2. **Re-register:** `bubbaloop node add ~/bubbaloop-nodes-official/rtsp-camera` (replaces precompiled binary registration)
3. **Build:** `cd ~/bubbaloop-nodes-official/rtsp-camera && cargo build --release`
4. **Iterate:** stop node -> edit -> rebuild -> start node -> verify

## Verification Plan

1. Daemon already running (PID 1864)
2. Start camera node: `bubbaloop node start rtsp-camera`
3. Start dashboard: `cd ~/bubbaloop/dashboard && npm run dev`
4. Monitor memory: `watch -n 1 'ps -o rss,vsz,comm -p $(pgrep rtsp_camera)'`
5. **Pass criteria:** Camera feed visible in dashboard AND RSS stays flat (< 50MB) over 60 seconds
6. Compare with pre-fix behavior (old binary leaks ~250MB/sec)

## Deliverable

PR to `kornia/bubbaloop-nodes-official` with the fix.
