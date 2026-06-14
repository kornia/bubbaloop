//! Ring-buffer capture + the circular MCAP writer (spec §3.3.5, §3.2).
//!
//! Ring-buffer mode is the v1 differentiator: the recorder keeps the most recent
//! samples in a bounded in-memory ring and, on `flush`, seals the current
//! contents into a brand-new finalized recording (chunks + `manifest.json`) while
//! the ring keeps running. This module provides two cooperating pieces:
//!
//! - [`RingBuffer`] — a pure, IO-free sliding window bounded by **both** a time
//!   window (`window_secs`) and a byte cap (`ring_max_bytes`, the OOM safety
//!   net, §3.3.5). Eviction is FIFO by arrival (`log_time`).
//! - [`seal_samples`] — the circular MCAP writer: takes a snapshot of samples and
//!   writes them to `{dir}/chunks/chunk-{idx:06}-{prefix8}.mcap` with rosbag2 wire
//!   defaults (§3.3.7), computing the SHA-256 streaming during the write and
//!   embedding the prefix in the filename (§3.3.6), then returns the assembled
//!   [`Recording`] manifest for the caller to persist.
//!
//! Both halves are deliberately clock-free and filesystem-narrow so they unit
//! test without Zenoh or a live recorder; the daemon-side `record` path (a later
//! PR) wires sample ingestion and the `{name}-flush-{ISO}` directory naming on
//! top.

use std::collections::{BTreeMap, VecDeque};
use std::io::Cursor;
use std::path::Path;

use mcap::{Channel, Compression, Message, WriteOptions};

use crate::storage::integrity;
use crate::storage::profile::{CompressionKind, CompressionLevel, DEFAULT_CHUNK_SIZE_BYTES};
use crate::storage::recording::{
    Channel as ManifestChannel, Chunk, Recording, RecordingMode, Selection,
};

/// Default ring byte cap (§3.3.5): 256 MiB. The window is the primary bound; this
/// is the OOM safety net that evicts even inside the window if memory balloons.
pub const DEFAULT_RING_MAX_BYTES: u64 = 256 * 1024 * 1024;

/// MCAP channel metadata keys carrying replay fidelity (spec §4.5).
pub const META_ZENOH_ENCODING: &str = "zenoh.encoding";
pub const META_ZENOH_TOPIC: &str = "zenoh.topic";
pub const META_SCHEMA_NAME: &str = "bubbaloop.schema_name";

/// One captured sample destined for an MCAP recording.
///
/// Carries both timestamps (§3.3.7 dual timestamps) and the original Zenoh
/// encoding so [`replay`](crate::storage::replay) can be byte-identical (§4.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sample {
    /// Full Zenoh topic key the sample was published on.
    pub topic: String,
    /// MCAP `message_encoding` (`cbor` / `protobuf` / `raw`).
    pub message_encoding: String,
    /// Original Zenoh sample encoding (e.g. `application/cbor`), stored in
    /// channel metadata for byte-identical replay.
    pub zenoh_encoding: String,
    /// Fully-qualified protobuf schema name, when known.
    pub schema_name: Option<String>,
    /// Publish-time from the Zenoh sample timestamp / message header (ns).
    pub publish_time_ns: u64,
    /// Recorder local-clock receipt time (ns) — drives windowing and ordering.
    pub log_time_ns: u64,
    /// The raw payload bytes (recorded verbatim).
    pub data: Vec<u8>,
}

impl Sample {
    /// Bytes this sample contributes to the ring's accounting. The payload
    /// dominates; the small per-sample overhead is ignored (the byte cap is a
    /// safety net, not an exact allocator accounting).
    fn ring_bytes(&self) -> u64 {
        self.data.len() as u64
    }
}

/// A bounded in-memory sliding window of the most recent samples (§3.3.5).
///
/// Bounded by **both** a time window and a byte cap: a sample is evicted (FIFO)
/// when it is older than `window_secs` relative to the newest sample **or** when
/// the total retained bytes exceed `ring_max_bytes`. Purely in-memory — there is
/// no disk spillover in v1 (§3.3.5).
#[derive(Debug)]
pub struct RingBuffer {
    window_ns: u64,
    max_bytes: u64,
    samples: VecDeque<Sample>,
    cur_bytes: u64,
}

impl RingBuffer {
    /// Create a ring with a time window (seconds) and a byte cap.
    ///
    /// `window_secs == 0` means "byte-cap only" (no time eviction); `max_bytes`
    /// of 0 is clamped to [`DEFAULT_RING_MAX_BYTES`] so the ring can never be
    /// configured to drop every sample immediately.
    pub fn new(window_secs: u64, max_bytes: u64) -> Self {
        Self {
            window_ns: window_secs.saturating_mul(1_000_000_000),
            max_bytes: if max_bytes == 0 {
                DEFAULT_RING_MAX_BYTES
            } else {
                max_bytes
            },
            samples: VecDeque::new(),
            cur_bytes: 0,
        }
    }

    /// Append a sample and evict anything that falls outside the window or pushes
    /// the ring over its byte cap.
    pub fn push(&mut self, sample: Sample) {
        let newest = sample.log_time_ns;
        self.cur_bytes += sample.ring_bytes();
        self.samples.push_back(sample);
        self.evict(newest);
    }

    /// Evict from the front while the oldest sample is older than the window
    /// (relative to `newest_ns`) or while the ring is over its byte cap. Always
    /// keeps at least the newest sample (a single sample larger than the cap is
    /// retained rather than dropping the very thing just captured).
    fn evict(&mut self, newest_ns: u64) {
        while self.samples.len() > 1 {
            let front = &self.samples[0];
            let too_old =
                self.window_ns != 0 && newest_ns.saturating_sub(front.log_time_ns) > self.window_ns;
            let over_bytes = self.cur_bytes > self.max_bytes;
            if !(too_old || over_bytes) {
                break;
            }
            let evicted = self.samples.pop_front().expect("len > 1");
            self.cur_bytes -= evicted.ring_bytes();
        }
    }

    /// Number of samples currently retained.
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Whether the ring holds no samples.
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Total retained payload bytes.
    pub fn byte_len(&self) -> u64 {
        self.cur_bytes
    }

    /// Snapshot the current contents without disturbing the ring (§3.3.5 flush
    /// step a — retained samples may belong to the next window too).
    pub fn snapshot(&self) -> Vec<Sample> {
        self.samples.iter().cloned().collect()
    }

    /// Snapshot the current contents and clear the ring. Use this for a flush
    /// that should *not* retain samples into the next window.
    pub fn drain(&mut self) -> Vec<Sample> {
        self.cur_bytes = 0;
        self.samples.drain(..).collect()
    }
}

/// MCAP wire configuration for a sealed recording (spec §3.3.7 — defaults match
/// rosbag2 verbatim: 786 432-byte chunks, zstd, fast level, CRC on).
#[derive(Debug, Clone, Copy)]
pub struct McapWriteConfig {
    /// Target size of one on-disk chunk file before rolling to the next.
    pub chunk_size_bytes: u32,
    /// Chunk compression codec.
    pub compression: CompressionKind,
    /// Compression level.
    pub compression_level: CompressionLevel,
    /// Whether to compute the per-chunk CRC inside the MCAP container.
    pub chunk_crc: bool,
}

impl Default for McapWriteConfig {
    fn default() -> Self {
        Self {
            chunk_size_bytes: DEFAULT_CHUNK_SIZE_BYTES,
            compression: CompressionKind::Zstd,
            compression_level: CompressionLevel::Fast,
            chunk_crc: true,
        }
    }
}

impl McapWriteConfig {
    /// The mcap-crate compression codec for this config (`None` = uncompressed).
    fn mcap_compression(&self) -> Option<Compression> {
        match self.compression {
            CompressionKind::Zstd => Some(Compression::Zstd),
            CompressionKind::Lz4 => Some(Compression::Lz4),
            CompressionKind::None => None,
        }
    }

    /// The mcap-crate numeric compression level for this config.
    fn mcap_level(&self) -> u32 {
        match self.compression_level {
            CompressionLevel::Fast => 1,
            CompressionLevel::Default => 3,
            CompressionLevel::High => 19,
        }
    }
}

/// Errors from the circular MCAP writer.
#[derive(Debug, thiserror::Error)]
pub enum RingBufferError {
    /// Filesystem error creating directories or writing a chunk file.
    #[error("io error at {path}: {detail}")]
    Io { path: String, detail: String },
    /// The underlying MCAP writer failed.
    #[error("mcap write error: {0}")]
    Mcap(String),
    /// The assembled manifest failed validation before save.
    #[error("manifest error: {0}")]
    Manifest(#[from] crate::storage::manifest::ManifestError),
}

/// A chunk file being assembled in memory: the MCAP writer over an in-memory
/// buffer, the accumulated uncompressed payload bytes (for size-based rolling),
/// and the running min/max `log_time` of messages written to it.
type PendingChunk = (mcap::Writer<Cursor<Vec<u8>>>, u64, Option<u64>, Option<u64>);

/// In-progress accounting for one channel while sealing.
struct ChannelAccum {
    channel: std::sync::Arc<Channel<'static>>,
    sequence: u32,
    message_count: u64,
    publish_first: Option<u64>,
    publish_last: Option<u64>,
}

/// Seal a snapshot of samples into a finalized recording at `recording_dir`
/// (spec §3.3.5 flush step b; §3.3.6/§3.3.7 for the wire format).
///
/// Writes one or more `chunks/chunk-{idx:06}-{prefix8}.mcap` files (rolling to a
/// new file once the accumulated payload exceeds `cfg.chunk_size_bytes`),
/// computes each chunk's SHA-256 during the write, persists `manifest.json`
/// atomically, and returns the assembled [`Recording`]. Samples are written in
/// the order given (the ring already holds them in arrival order); MCAP message
/// headers carry both `publish_time` and `log_time` (§3.3.7).
///
/// `now_ns` is the caller's clock reading used as `ended_at_ns` when the snapshot
/// is empty of timestamps; with samples present the recorder-clock bounds come
/// from the samples themselves.
#[allow(clippy::too_many_arguments)]
pub fn seal_samples(
    recording_dir: &Path,
    name: &str,
    machine_id: &str,
    mode: RecordingMode,
    window_secs: Option<u64>,
    selection: Selection,
    samples: &[Sample],
    cfg: &McapWriteConfig,
    now_ns: u64,
) -> Result<Recording, RingBufferError> {
    let chunks_dir = recording_dir.join("chunks");
    std::fs::create_dir_all(&chunks_dir).map_err(|e| RingBufferError::Io {
        path: chunks_dir.display().to_string(),
        detail: e.to_string(),
    })?;

    // Stable channel ids in first-seen order, shared across every chunk file.
    let mut channel_ids: BTreeMap<String, u16> = BTreeMap::new();
    let mut channels: Vec<ChannelAccum> = Vec::new();

    // started_at = earliest log_time across the snapshot (or the caller's clock
    // when the snapshot is empty).
    let started_at = samples
        .iter()
        .map(|s| s.log_time_ns)
        .min()
        .unwrap_or(now_ns);
    let mut manifest = Recording::new(name, machine_id, started_at);
    manifest.mode = mode;
    manifest.window_secs = window_secs;
    manifest.selection = selection;

    let mut chunk_entries: Vec<Chunk> = Vec::new();
    let mut total_bytes: u64 = 0;

    // A pending chunk writer over an in-memory buffer; flushed to disk on roll.
    let mut writer_state: Option<PendingChunk> = None;

    let target = cfg.chunk_size_bytes as u64;

    // Helper to finalize the active chunk writer to a canonical on-disk file.
    let finalize_chunk = |writer_state: &mut Option<PendingChunk>,
                          chunk_entries: &mut Vec<Chunk>,
                          total_bytes: &mut u64|
     -> Result<(), RingBufferError> {
        let Some((mut writer, _accum, log_first, log_last)) = writer_state.take() else {
            return Ok(());
        };
        writer
            .finish()
            .map_err(|e| RingBufferError::Mcap(e.to_string()))?;
        let buf = writer.into_inner().into_inner();
        let digest = integrity::sha256(&buf);
        let hex = integrity::to_hex(&digest);
        let index = chunk_entries.len() as u32;
        let fname = Chunk::canonical_name(index, &hex);
        let path = chunks_dir.join(&fname);
        atomic_write(&path, &buf)?;
        *total_bytes += buf.len() as u64;
        chunk_entries.push(Chunk {
            name: fname,
            index,
            size_bytes: buf.len() as u64,
            sha256: hex,
            log_time_first_ns: log_first,
            log_time_last_ns: log_last,
            uploaded_at_ns: None,
            remote_etag: None,
            extra: Default::default(),
        });
        Ok(())
    };

    for sample in samples {
        // Register the channel on first sight (stable id across chunk files).
        if !channel_ids.contains_key(&sample.topic) {
            let id = channels.len() as u16;
            channel_ids.insert(sample.topic.clone(), id);
            let mut metadata = BTreeMap::new();
            metadata.insert(
                META_ZENOH_ENCODING.to_string(),
                sample.zenoh_encoding.clone(),
            );
            metadata.insert(META_ZENOH_TOPIC.to_string(), sample.topic.clone());
            if let Some(schema) = &sample.schema_name {
                metadata.insert(META_SCHEMA_NAME.to_string(), schema.clone());
            }
            channels.push(ChannelAccum {
                channel: std::sync::Arc::new(Channel {
                    id,
                    topic: sample.topic.clone(),
                    schema: None,
                    message_encoding: sample.message_encoding.clone(),
                    metadata,
                }),
                sequence: 0,
                message_count: 0,
                publish_first: None,
                publish_last: None,
            });
        }
        let cid = channel_ids[&sample.topic];
        let accum = &mut channels[cid as usize];

        // Roll to a new chunk file once the accumulated payload exceeds target.
        if let Some((_, ref bytes, _, _)) = writer_state {
            if *bytes >= target && target > 0 {
                finalize_chunk(&mut writer_state, &mut chunk_entries, &mut total_bytes)?;
            }
        }
        if writer_state.is_none() {
            let mut opts = WriteOptions::new()
                .chunk_size(Some(target.max(1)))
                .compression(cfg.mcap_compression())
                .calculate_chunk_crcs(cfg.chunk_crc);
            if cfg.mcap_compression().is_some() {
                opts = opts.compression_level(cfg.mcap_level());
            }
            let writer = opts
                .create(Cursor::new(Vec::new()))
                .map_err(|e| RingBufferError::Mcap(e.to_string()))?;
            writer_state = Some((writer, 0, None, None));
        }

        let (writer, acc_bytes, log_first, log_last) = writer_state.as_mut().expect("just set");
        let msg = Message {
            channel: accum.channel.clone(),
            sequence: accum.sequence,
            log_time: sample.log_time_ns,
            publish_time: sample.publish_time_ns,
            data: std::borrow::Cow::Borrowed(&sample.data),
        };
        writer
            .write(&msg)
            .map_err(|e| RingBufferError::Mcap(e.to_string()))?;

        accum.sequence = accum.sequence.wrapping_add(1);
        accum.message_count += 1;
        accum.publish_first = Some(
            accum
                .publish_first
                .map_or(sample.publish_time_ns, |v| v.min(sample.publish_time_ns)),
        );
        accum.publish_last = Some(
            accum
                .publish_last
                .map_or(sample.publish_time_ns, |v| v.max(sample.publish_time_ns)),
        );
        *acc_bytes += sample.data.len() as u64;
        *log_first = Some(log_first.map_or(sample.log_time_ns, |v| v.min(sample.log_time_ns)));
        *log_last = Some(log_last.map_or(sample.log_time_ns, |v| v.max(sample.log_time_ns)));
    }

    finalize_chunk(&mut writer_state, &mut chunk_entries, &mut total_bytes)?;

    // Assemble the manifest channels in id order.
    manifest.channels = channels
        .iter()
        .enumerate()
        .map(|(id, acc)| ManifestChannel {
            topic: acc.channel.topic.clone(),
            channel_id: id as u16,
            message_count: acc.message_count,
            message_encoding: acc.channel.message_encoding.clone(),
            zenoh_encoding: acc
                .channel
                .metadata
                .get(META_ZENOH_ENCODING)
                .cloned()
                .unwrap_or_default(),
            schema_name: acc.channel.metadata.get(META_SCHEMA_NAME).cloned(),
            publish_time_first_ns: acc.publish_first,
            publish_time_last_ns: acc.publish_last,
            extra: Default::default(),
        })
        .collect();
    manifest.chunks = chunk_entries;
    manifest.size_bytes = total_bytes;

    let ended = samples
        .iter()
        .map(|s| s.log_time_ns)
        .max()
        .unwrap_or(now_ns);
    manifest.ended_at_ns = Some(ended);
    manifest.duration_ns = Some(ended.saturating_sub(manifest.started_at_ns));

    crate::storage::manifest::save_dir(recording_dir, &manifest)?;
    Ok(manifest)
}

/// Atomically write `bytes` to `path` via a sibling temp file + rename.
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), RingBufferError> {
    use std::io::Write;
    let tmp = path.with_extension("mcap.tmp");
    {
        let mut f = std::fs::File::create(&tmp).map_err(|e| RingBufferError::Io {
            path: tmp.display().to_string(),
            detail: e.to_string(),
        })?;
        f.write_all(bytes).map_err(|e| RingBufferError::Io {
            path: tmp.display().to_string(),
            detail: e.to_string(),
        })?;
        f.sync_all().map_err(|e| RingBufferError::Io {
            path: tmp.display().to_string(),
            detail: e.to_string(),
        })?;
    }
    std::fs::rename(&tmp, path).map_err(|e| RingBufferError::Io {
        path: path.display().to_string(),
        detail: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(topic: &str, log_ns: u64, bytes: usize) -> Sample {
        Sample {
            topic: topic.to_string(),
            message_encoding: "cbor".to_string(),
            zenoh_encoding: "application/cbor".to_string(),
            schema_name: None,
            publish_time_ns: log_ns,
            log_time_ns: log_ns,
            data: vec![0xAB; bytes],
        }
    }

    const S: u64 = 1_000_000_000;

    #[test]
    fn window_evicts_old_samples() {
        // 5-second window, generous byte cap.
        let mut ring = RingBuffer::new(5, 10_000_000);
        for i in 0..=10u64 {
            ring.push(sample("t", i * S, 10)); // one per second, t=0..10s
        }
        // Newest is 10s; window is 5s → keep samples with log_time > 5s, i.e.
        // 6,7,8,9,10 plus the boundary handling (strictly older-than-window
        // evicted). 5s is exactly window away (10-5=5, not > 5) so 5s is kept.
        let kept: Vec<u64> = ring.snapshot().iter().map(|s| s.log_time_ns / S).collect();
        assert_eq!(kept, vec![5, 6, 7, 8, 9, 10]);
    }

    #[test]
    fn byte_cap_evicts_even_inside_window() {
        // Huge time window, tight byte cap (100 bytes), samples of 30 bytes.
        let mut ring = RingBuffer::new(3600, 100);
        for i in 0..10u64 {
            ring.push(sample("t", i * S, 30));
        }
        // 100 / 30 → at most 3 samples fit (90 bytes); a 4th would be 120 > 100.
        assert!(ring.byte_len() <= 100);
        assert_eq!(ring.len(), 3);
        let kept: Vec<u64> = ring.snapshot().iter().map(|s| s.log_time_ns / S).collect();
        assert_eq!(kept, vec![7, 8, 9]);
    }

    #[test]
    fn never_drops_the_only_sample_even_if_oversized() {
        let mut ring = RingBuffer::new(1, 10); // 10-byte cap
        ring.push(sample("t", 0, 1000)); // single 1000-byte sample
        assert_eq!(ring.len(), 1);
        assert!(!ring.is_empty());
    }

    #[test]
    fn snapshot_preserves_ring_drain_clears_it() {
        let mut ring = RingBuffer::new(3600, 1_000_000);
        ring.push(sample("t", 0, 10));
        ring.push(sample("t", S, 10));
        assert_eq!(ring.snapshot().len(), 2);
        assert_eq!(ring.len(), 2); // snapshot is non-destructive
        let drained = ring.drain();
        assert_eq!(drained.len(), 2);
        assert!(ring.is_empty());
        assert_eq!(ring.byte_len(), 0);
    }

    #[test]
    fn seal_writes_valid_mcap_and_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let rec_dir = dir.path().join("rec");
        let samples = vec![
            Sample {
                topic: "bubbaloop/global/m/cam/compressed".to_string(),
                message_encoding: "protobuf".to_string(),
                zenoh_encoding: "application/protobuf".to_string(),
                schema_name: Some("bubbaloop.camera.v1.CompressedImage".to_string()),
                publish_time_ns: 100,
                log_time_ns: 110,
                data: vec![1, 2, 3, 4],
            },
            Sample {
                topic: "bubbaloop/global/m/imu/data".to_string(),
                message_encoding: "cbor".to_string(),
                zenoh_encoding: "application/cbor".to_string(),
                schema_name: None,
                publish_time_ns: 200,
                log_time_ns: 210,
                data: vec![5, 6, 7],
            },
        ];
        let manifest = seal_samples(
            &rec_dir,
            "rec",
            "m",
            RecordingMode::RingBuffer,
            Some(60),
            Selection::default(),
            &samples,
            &McapWriteConfig::default(),
            999,
        )
        .unwrap();

        // Manifest is internally consistent and passes the strict validator.
        crate::storage::manifest::validate(&manifest).unwrap();
        assert_eq!(manifest.channels.len(), 2);
        assert_eq!(manifest.chunks.len(), 1);
        assert!(manifest.size_bytes > 0);
        assert_eq!(manifest.started_at_ns, 110);
        assert_eq!(manifest.ended_at_ns, Some(210));

        // Channel metadata carried the zenoh encoding and schema name.
        let cam = manifest
            .channels
            .iter()
            .find(|c| c.topic.ends_with("/compressed"))
            .unwrap();
        assert_eq!(cam.zenoh_encoding, "application/protobuf");
        assert_eq!(
            cam.schema_name.as_deref(),
            Some("bubbaloop.camera.v1.CompressedImage")
        );

        // The chunk file exists, hashes to the recorded digest, and parses as MCAP
        // with both timestamps preserved.
        let chunk = &manifest.chunks[0];
        let path = rec_dir.join("chunks").join(&chunk.name);
        assert!(path.exists());
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(integrity::to_hex(&integrity::sha256(&bytes)), chunk.sha256);
        assert!(chunk.name_is_canonical());

        let mut seen = 0;
        for msg in mcap::MessageStream::new(&bytes).unwrap() {
            let msg = msg.unwrap();
            seen += 1;
            if msg.channel.topic.ends_with("/compressed") {
                assert_eq!(msg.publish_time, 100);
                assert_eq!(msg.log_time, 110);
                assert_eq!(&*msg.data, &[1, 2, 3, 4]);
                assert_eq!(
                    msg.channel
                        .metadata
                        .get(META_ZENOH_ENCODING)
                        .map(|s| s.as_str()),
                    Some("application/protobuf")
                );
            }
        }
        assert_eq!(seen, 2);
    }

    #[test]
    fn seal_rolls_multiple_chunks_by_size() {
        let dir = tempfile::tempdir().unwrap();
        let rec_dir = dir.path().join("rec");
        // Tiny target so each ~500-byte sample forces a new chunk file.
        let cfg = McapWriteConfig {
            chunk_size_bytes: 400,
            ..McapWriteConfig::default()
        };
        let samples: Vec<Sample> = (0..4)
            .map(|i| Sample {
                topic: "t".to_string(),
                message_encoding: "raw".to_string(),
                zenoh_encoding: "zenoh/bytes".to_string(),
                schema_name: None,
                publish_time_ns: i,
                log_time_ns: i,
                data: vec![0u8; 500],
            })
            .collect();
        let manifest = seal_samples(
            &rec_dir,
            "rec",
            "m",
            RecordingMode::RingBuffer,
            None,
            Selection::default(),
            &samples,
            &cfg,
            0,
        )
        .unwrap();
        // Each sample exceeds the 400-byte target → one chunk per sample.
        assert_eq!(manifest.chunks.len(), 4);
        crate::storage::manifest::validate(&manifest).unwrap();
        for (i, c) in manifest.chunks.iter().enumerate() {
            assert_eq!(c.index, i as u32);
            assert!(rec_dir.join("chunks").join(&c.name).exists());
        }
    }

    #[test]
    fn seal_empty_snapshot_produces_open_free_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let rec_dir = dir.path().join("rec");
        let manifest = seal_samples(
            &rec_dir,
            "rec",
            "m",
            RecordingMode::RingBuffer,
            Some(5),
            Selection::default(),
            &[],
            &McapWriteConfig::default(),
            4242,
        )
        .unwrap();
        assert!(manifest.chunks.is_empty());
        assert!(manifest.channels.is_empty());
        assert_eq!(manifest.started_at_ns, 4242);
        assert_eq!(manifest.ended_at_ns, Some(4242));
        crate::storage::manifest::validate(&manifest).unwrap();
    }
}
