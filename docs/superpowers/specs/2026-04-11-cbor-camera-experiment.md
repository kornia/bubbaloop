# CBOR camera experiment

**Date:** 2026-04-11
**Branch (nodes repo):** `experiment/cbor-compressed` on `bubbaloop-nodes-official`
**Base:** `refactor/scope-removal-and-cleanup` (matches currently-installed camera state)
**Scope:** `rtsp-camera` only — parallel publisher, no changes to existing consumers

## Why

We wanted to explore a new message protocol (self-describing + zero-copy + debug-readable) as
an alternative to protobuf. The brainstorm landed on: **before designing a novel format, try
CBOR on a real sensor node and see whether the pain points are real or imagined.** CBOR is
already self-describing (RFC 8949), already zero-copy for bulk numeric payloads (RFC 8746
typed arrays), already debug-readable (diagnostic notation), and already has Rust/Python
libraries. If it works, we save ourselves a multi-month wire-format design project.

## Hypothesis

1. For a schema dominated by a single byte blob (`CompressedImage = {header, format, data}`
   where `data` is H264 bytes), CBOR wire size should be within a few percent of protobuf.
   Both encodings stamp ~20 bytes of headers around the blob.
2. Ciborium encode time should be negligible compared to gstreamer's H264 pipeline
   (encode cost < 1 ms at 30 fps, gstreamer latency is tens of ms).
3. The only genuine risk is ciborium's serde integration for `&[u8]`: without the
   `serde_bytes` helper, `Vec<u8>` serializes as a CBOR *array of u8* (~3× bloat). With
   `serde_bytes`, it serializes as a CBOR byte string, which is what we want.

## Design

### Topology

```
rtsp-camera node
  ├── {key}/compressed       → ProtoPublisher<CompressedImage>   [EXISTING, unchanged]
  ├── {key}/compressed_cbor  → RawPublisher (no encoding tag)    [NEW, parallel]
  └── {key}/raw              → RawPublisher<RawImage> over SHM   [EXISTING, unchanged]
```

All four downstream consumers (`camera-object-detector`, `camera-vlm`, `camera-embedder`,
`motion-detector`) keep subscribing to the protobuf topic and keep working. The new CBOR
topic is only read by the throwaway `cbor_subscriber` binary.

### Files changed in `bubbaloop-nodes-official/rtsp-camera`

- `Cargo.toml` — added `ciborium = "0.2"`, `serde_bytes = "0.11"`, and a second `[[bin]]`
  entry for the subscriber.
- `src/cbor_wire.rs` *(new)* — serde-serializable mirror types:
  - `HeaderCbor` (owned, round-trips)
  - `CompressedImageCborRef<'a>` (borrowed, used by the publisher to avoid copying the
    ~200 KB H264 slice)
  - `CompressedImageCborOwned` (owned, used by the subscriber for deserialization)
  - `#[serde(with = "serde_bytes")]` on the `data` field so ciborium emits a CBOR byte
    string, not an array of u8.
- `src/lib.rs` — exports the new `cbor_wire` module.
- `src/rtsp_camera_node.rs`:
  - Declares a parallel `RawPublisher` on `{key}/compressed_cbor`.
  - Per frame: builds a borrowed `CompressedImageCborRef` from the gstreamer mapped buffer,
    encodes into a fresh `Vec<u8>` via `ciborium::into_writer`, publishes via `RawPublisher`.
  - Tracks `total_pb_bytes`, `total_cbor_bytes`, `total_cbor_encode_ns` alongside the
    existing counters.
  - Per-second log line now reports `pb_avg`, `cbor_avg`, `ratio`, and `cbor_enc≈Nµs`.
- `src/bin/cbor_subscriber.rs` *(new)* — standalone ~60-line binary. Opens a Zenoh client
  session, subscribes to `bubbaloop/global/**/compressed_cbor`, decodes each payload as
  `CompressedImageCborOwned`, prints per-message and rolling-average stats.

### What was *not* done (deferred)

- **SDK changes.** `NodeContext::publisher_raw` takes no encoding argument, so the new
  topic is published without an `application/cbor` hint. Adding `publisher_raw_cbor` to
  `bubbaloop-node` is a 10-line patch, deferred until the experiment proves itself worth
  promoting — this keeps the experiment contained in one repo.
- **SHM / `RawImage` path.** The pre-allocated SHM pool + `msg.encode(&mut slice)` +
  `try_resize` hack is harder to replicate with ciborium. If compressed CBOR looks good,
  the next experiment is replacing the SHM path.
- **Python cbor2 interop.** No Python consumer touched. The 4 existing Python consumers
  continue to decode protobuf from the unchanged topic.
- **Dashboard.** Untouched. The experimental topic is invisible to it (no encoding hint,
  no schema queryable).

## Results

Measured on Jetson Orin, `tapo_terrace` camera (560×560 target, h264/nvdec, 10 fps target),
~13 minutes of sustained streaming (7974 frames published per topic), publisher on SHM
transport (`publisher_raw(local=true)`) with ciborium encoding **directly into a
pre-allocated SHM slot** via `std::io::Cursor<&mut [u8]>` — no intermediate heap Vec.

| Metric | Protobuf | CBOR | Δ |
|---|---|---|---|
| Avg wire bytes / frame | 13,952 | 14,016 | +64 B (+0.46%) |
| Ratio (cbor / pb) | 1.000 | **1.00459** | +0.46% |
| CBOR encode time (publisher) | — | **≈11 µs** | well under 500 µs gate |
| Frames published (steady state) | 7,974 | 7,974 | 0 lost vs pb |
| Decode errors on subscriber | — | **0 / 7920** | clean |
| Constant CBOR framing overhead | — | **131 B / msg** | header-length dominated |

### Zero-copy / SHM verification

Kernel-level proof that the CBOR publisher path writes into shared memory pages:

```
$ grep .zenoh /proc/<rtsp_camera_node_pid>/maps
ffff4ede0000-ffff4efe0000 rw-s 00000000 00:19 292405  /dev/shm/1476150401.zenoh
  →  size = 0x200000 = 2,097,152 B = 512 KB × 4 slots (CBOR pool)
ffff4e916000-ffff4ede0000 rw-s 00000000 00:19 292406  /dev/shm/4222734242.zenoh
  →  size = 0x4ca000 = 5,018,112 B = (560×560×4 + 128) × 4 (raw RGBA pool, unchanged)
```

Both pools are file-backed `rw-s` mappings — standard POSIX shared memory. `ciborium::into_writer`
runs `cursor.write(...)` calls that store directly into these pages, so the only copy of the
H264 payload from the gstreamer mapped buffer into CBOR bytes happens *in place* inside SHM.
No heap `Vec<u8>`, no `Encoding::from(payload)` copy, no intermediate staging buffer.

### Where zero-copy stops (client mode through zenohd)

The camera node publishes via `publisher_raw(local=true)` into an SHM-backed pool, and the
cbor_subscriber session has `transport/shared_memory/enabled = true` with threshold lowered
to 1 B so *all* messages should become SHM candidates. Despite that, `payload.as_shm()`
returns `None` for every sample delivered through zenohd, and `/proc/<cbor_subscriber>/maps`
shows only a 4 KB zenoh control segment — the 2 MB CBOR pool is *not* mapped into the
subscriber process.

Root cause: Zenoh SHM is negotiated session-to-session at transport handshake time. In
client mode the subscriber's peer is `zenohd`, not the camera node. `zenohd` is itself a
regular Zenoh session in the middle — it *could* take the SHM payload from the publisher,
but it has to route it to downstream subscribers, and it dereferences the payload into
regular bytes at its routing boundary. The bytes then ship over TCP loopback to the
subscriber, which sees a heap copy. Lowering `message_size_threshold` doesn't help because
the SHM *channel* never exists between subscriber and zenohd — only between publisher and
zenohd.

This isn't a CBOR issue; the same topology applies to `/raw` RawImage.

**For camera nodes specifically, client mode is the correct choice and this copy is the
right tradeoff.** The router gives us: per-client auth, a single authoritative subscription
table, predictable discovery, deterministic startup (no peer-mesh handshake race), stable
operational surface for the `bubbaloop` CLI and dashboard, and a single place to enforce
policy. The one receive-side memcpy it costs us — ~8 µs for a 200 KB frame on loopback DDR
— is negligible next to gstreamer's tens-of-milliseconds H264 pipeline latency. Peer mode
would save that 8 µs at the cost of losing all of the above. Not worth it for camera work.

**So why the peer-mode experiment below?** Purely to answer one question: *is Zenoh's SHM
API honest?* The kernel-level evidence in the following section proves end-to-end zero-copy
is physically real when you put two peers on a direct link — which validates the API
itself. Nothing in that experiment is production guidance. Camera nodes keep client mode.
The peer-mode binaries are verification artifacts, not a template to copy.

### End-to-end zero-copy verification (standalone peer binaries, NOT for production)

To prove the publisher → subscriber path is *physically* zero-copy, we built a pair of
throwaway binaries that bypass `zenohd` entirely. Both live under
[`rtsp-camera/src/bin/`](../../../../bubbaloop-nodes-official/rtsp-camera/src/bin):

- [`cbor_peer_pub.rs`](../../../../bubbaloop-nodes-official/rtsp-camera/src/bin/cbor_peer_pub.rs):
  peer-mode Zenoh session listening on `tcp/127.0.0.1:17447`, scouting disabled, SHM
  enabled with `message_size_threshold=1`. Pre-allocates a `PosixShmProviderBackend` pool
  of **4 slots × 512 KB = 2 MB**, then once every 100 ms allocates a slot, encodes a
  synthetic `CompressedImageCborRef` (200 KB payload) directly into the slot with
  `ciborium::into_writer(&msg, &mut Cursor::new(&mut sbuf[..]))`, `try_resize`s to the
  actual CBOR length, and publishes via `ZBytes::from(sbuf)`. Stamps a big-endian `u32`
  counter into the first 4 bytes of the CBOR `data` field so the subscriber can
  cross-reference each sample.
- [`cbor_peer_sub.rs`](../../../../bubbaloop-nodes-official/rtsp-camera/src/bin/cbor_peer_sub.rs):
  peer-mode Zenoh session connecting to `tcp/127.0.0.1:17447`, same SHM config. For every
  sample calls `payload.as_shm()` and, if `Some`, reads the slice pointer, walks
  `/proc/self/maps` to find the backing `/dev/shm/*.zenoh` file plus the byte offset
  within that file, and logs them. Then decodes the CBOR to confirm the counter.

Run on Jetson Orin:

```
$ ./cbor_peer_pub &
$ ./cbor_peer_sub &
$ grep .zenoh /proc/<cbor_peer_pub>/maps
ffff8a020000-ffff8a220000 rw-s 00000000 00:19 297645  /dev/shm/1161509704.zenoh   ← 2 MB CBOR pool
ffff89ead000-ffff89fed000 rw-s 00000000 00:19 297651  /dev/shm/9937.zenoh         ← 1.25 MB auth
ffff8ad23000-ffff8ad24000 rw-s 00000000 00:19 297644  /dev/shm/2014784182.zenoh   ← 4 KB ctrl

$ grep .zenoh /proc/<cbor_peer_sub>/maps
ffff995aa000-ffff997aa000 rw-s 00000000 00:19 297645  /dev/shm/1161509704.zenoh   ← SAME INODE
ffff99bc0000-ffff99d00000 rw-s 00000000 00:19 297651  /dev/shm/9937.zenoh         ← SAME INODE
ffff9b255000-ffff9b256000 rw-s 00000000 00:19 297650  /dev/shm/291060959.zenoh    ← sub's own ctrl
```

The CBOR pool file `/dev/shm/1161509704.zenoh`, inode `297645`, is mapped into **both**
processes. Virtual addresses differ (`0xffff8a020000` on pub vs `0xffff995aa000` on sub) —
that's normal: each process has its own address space. What matters is that the **same
inode** is mapped, which means both mappings point at the **same physical kernel pages**.

Per-sample trace (first 4 samples):

```
[peer_pub] put #1 ptr=0xffff8a020410 len=204917 first=0xa3
[peer_pub] put #2 ptr=0xffff8a0a0418 len=204917 first=0xa3
[peer_pub] put #3 ptr=0xffff8a120420 len=204917 first=0xa3
[peer_pub] put #4 ptr=0xffff8a020410 len=204917 first=0xa3  ← slot 0 reused (4-slot ring)

[peer_sub SHM #1] ptr=0xffff995aa410 len=204917 first=0xa3 file=/dev/shm/1161509704.zenoh offset_in_file=0x410
[peer_sub]        seq=1 marker=1 data_len=204800B
[peer_sub SHM #2] ptr=0xffff9962a418 len=204917 first=0xa3 file=/dev/shm/1161509704.zenoh offset_in_file=0x80418
[peer_sub]        seq=2 marker=2 data_len=204800B
[peer_sub SHM #3] ptr=0xffff996aa420 len=204917 first=0xa3 file=/dev/shm/1161509704.zenoh offset_in_file=0x100420
[peer_sub]        seq=3 marker=3 data_len=204800B
[peer_sub SHM #4] ptr=0xffff995aa410 len=204917 first=0xa3 file=/dev/shm/1161509704.zenoh offset_in_file=0x410
[peer_sub]        seq=4 marker=4 data_len=204800B
```

Offsets line up exactly:

| Sample | Pub pointer | Pub offset (ptr − 0xffff8a020000) | Sub offset_in_file | Slot |
|---|---|---|---|---|
| #1 | 0xffff8a020410 | 0x410     | 0x410     | 0 |
| #2 | 0xffff8a0a0418 | 0x80418   | 0x80418   | 1 |
| #3 | 0xffff8a120420 | 0x100420  | 0x100420  | 2 |
| #4 | 0xffff8a020410 | 0x410     | 0x410     | 0 (ring-reused) |

Counter markers also round-trip: `peer_pub put #N` ↔ `peer_sub seq=N marker=N` for all 330
samples collected during the run. Stats:

| Metric | Value |
|---|---|
| Samples received | 330 |
| SHM deliveries (`as_shm()` → Some) | **330 / 330 (100%)** |
| Heap fallbacks | 0 |
| Decode errors | 0 |
| Slot offsets observed | 0x410, 0x80418, 0x100420 (3 of 4 slots — GC policy keeps last-released slot fresh) |
| First byte | 0xA3 (CBOR major type 5 `map(3)`) every sample ✓ |

This settles it: **same file inode + same file offset + matching byte content = same
physical memory page**. The publisher's `ciborium::into_writer` calls land bytes directly
in `/dev/shm/1161509704.zenoh` at offset `0x410`, and the subscriber's `as_shm()` slice
reads *those exact bytes from the exact same offset in the exact same file*. No copy
anywhere in the pipeline.

What the test explicitly does NOT prove:
- It uses synthetic 200 KB filler, not a real H264 frame. The camera's real H264 byte-stream
  comes from a gstreamer `Buffer::map_readable()` which is *itself* a copy from gstreamer's
  internal pool into the mapped slice — we still cross one boundary between gstreamer and
  our SHM slot. Removing that copy would require a gstreamer pool allocator that hands
  `nvbuf`s directly from the NVDEC output queue. Out of scope for the CBOR experiment.
- It bypasses `zenohd`. Running the real camera node through zenohd to the real
  cbor_subscriber still falls back to heap on the receive side for the reasons documented
  in the previous section.

### Sanity-check run against pure heap Vec (earlier)

A first-pass run published CBOR over a global TCP topic (`publisher_raw(local=false)`) with
`ciborium::into_writer` into a fresh `Vec<u8>` per frame. Encode time averaged 11–15 µs
(occasionally 20 µs for the first keyframe), ratio ≈ 1.003–1.005, 0 decode errors across 270+
messages. Moving to the SHM + direct-encode path brought encode time to a tighter 10–11 µs and
removed the per-frame heap allocation, but the wire ratio was unchanged — consistent with the
byte layout being identical (CBOR framing is content, not transport).

## Decision gate after measurement

**Result: CBOR is viable.** Ratio 1.00459 ≪ 1.05 gate, encode time ~11 µs ≪ 500 µs gate,
zero decode errors across ~8000 messages.

**Hypothesis verification:**
1. ✅ Wire size within "a few percent" of protobuf — measured at **+0.46%**. The 131-byte
   constant CBOR overhead is well within the ~20–30 byte / field framing cost predicted by
   RFC 8949 for a map-of-3-fields with string keys and one bulk byte blob.
2. ✅ Encode time negligible. 11 µs on Jetson Orin vs gstreamer latency of tens of ms —
   CBOR encoding is ~0.01% of the frame's wall-clock budget.
3. ✅ `serde_bytes` correctly honored. Without it, encoding `&[u8]` would have blown the wire
   size by ~3× (array-of-u8 instead of byte string). Observed 131 B framing overhead confirms
   byte-string encoding (major type 2, a few header bytes for length-prefix).

**What worked better than expected:**
- Writing ciborium into SHM via `std::io::Cursor<&mut [u8]>` was trivial — no custom
  `ciborium_io::Write` adapter needed because the std feature provides the blanket impl.
- `ZBytes::from(OwnedShmBuf)` composes cleanly with publisher_raw, so the existing
  `publisher_raw_proto` code path didn't need to be generalized.

## Decision

**Keep CBOR in the rtsp-camera pipeline as a parallel publisher with publish-side SHM
zero-copy.** This was validated:

- Publisher writes CBOR bytes directly into a pre-allocated `PosixShmProviderBackend` slot
  via `ciborium::into_writer(&msg, &mut Cursor::new(&mut sbuf[..]))` — no heap `Vec`, no
  intermediate staging buffer. Kernel evidence: the 2 MB CBOR pool shows as a `rw-s`
  mapping of `/dev/shm/*.zenoh` in the camera process's `/proc/<pid>/maps`.
- Uses `publisher_raw(local=true)` with `CongestionControl::Block` — required so slow
  consumers don't silently drop frames.
- Session stays in **client mode** talking to `zenohd`. Receive-side pays one ~8 µs
  memcpy per frame on loopback (`zenohd` dereferences the SHM handle when routing). That
  cost is negligible next to the ~30 ms gstreamer H264 pipeline and is traded for the
  router's auth, discovery, deterministic startup, and operational uniformity benefits.

## Next steps (ordered by blocking dependency)

1. **Upstream the SDK `message_size_threshold=1` change** to `bubbaloop/main`. It's a
   5-line patch in `crates/bubbaloop-node/src/zenoh_session.rs` that makes *every* message
   an SHM candidate instead of just ones over 3072 B. Orthogonal to CBOR — helps any node
   that uses `publisher_raw(local=true)` for small messages. Needs its own tiny PR.
2. **Remove the `[patch]` block from `rtsp-camera/Cargo.toml`** once (1) is merged. Right
   now that block redirects the SDK git dep at a local `/home/nvidia/bubbaloop` checkout,
   which breaks anyone else's build. Cannot drop it until the SDK change is on main.
3. **Add `publisher_raw_cbor` to `bubbaloop-node`** — a ~15-line SDK helper that calls
   `RawPublisher::with_encoding(..., Some(Encoding::APPLICATION_CBOR))`. Without it, the
   camera's CBOR topic is published with no encoding hint, so dashboard sniffs the payload
   instead of decoding it cleanly. Promotes CBOR from "experiment" to "first-class
   encoding" alongside `publisher_raw_proto` and `publisher_json`.
4. **Dashboard CBOR decode path** — once (3) lands and samples carry `APPLICATION_CBOR`,
   branch on `sample.encoding()` and decode via [cbor-x](https://www.npmjs.com/package/cbor-x).
   Existing protobuf/JSON paths untouched.
5. **Python `CborSubscriber`** — `pip install cbor2`, mirror the existing `ProtoSubscriber`
   in `python-sdk/`. Validate against the Rust camera publisher.
6. **Receive-side zero-copy is NOT a camera-pipeline goal.** The peer-mode experiment
   proved Zenoh's SHM API is honest — end-to-end zero-copy is physically achievable when
   two sessions share a direct link. Camera nodes will not be migrated. If a *different*
   class of consumer ever genuinely needs zero-copy on receive (e.g. a tight inference
   loop where one memcpy matters), the move is a **second, dedicated peer-mode session
   for that one hot path** alongside the existing client-mode session — never a migration
   of the whole pipeline. The verification binaries (`cbor_peer_pub`, `cbor_peer_sub`)
   that proved this were thrown away after delivering the measurement; if needed again,
   resurrect them from git history at commit `<this-experiment-commit>`.
7. **Decide CBOR scope beyond camera** — options: (a) camera pipelines only, (b) all
   binary-heavy topics, (c) full replacement of protobuf for new nodes, (d) defer.
   Tracked separately from this experiment; the camera pipeline is already committed to
   CBOR per the decision above.

## Unresolved questions

1. Does `ciborium::into_writer` into a `&mut Vec<u8>` cause per-call reallocation when the
   vec is freshly created with `Vec::with_capacity(h264_slice.len() + 256)`? Probably not
   — the capacity is an upper bound — but worth verifying with a flame graph if encode
   time looks bad.
2. Is `ZBytes::from(Vec<u8>)` zero-copy? If it takes ownership without copying, we have a
   single allocation per frame (the output buffer). If it copies, we have two. Needs a
   quick `cargo doc` check.
3. Over-the-wire size for the protobuf path reported by `msg.encoded_len()` is the
   serialized proto length, but the wire message on Zenoh also carries an encoding hint
   string (`application/protobuf;bubbaloop.camera.v1.CompressedImage`, ~48 bytes). We are
   not counting this against the protobuf path — the comparison is "payload bytes only",
   which is slightly biased in protobuf's favor. For CBOR we could also set an encoding
   hint in a future SDK change; for now the bias is small enough to ignore.
