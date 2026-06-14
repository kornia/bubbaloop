//! Replay — read a recording's MCAP chunks and re-publish into Zenoh (spec §4.5,
//! CLI §5 `storage replay`).
//!
//! Replay mirrors `ros2 bag play`: `--rate`, `--loop`, `--start-time`/`--end-time`
//! trim, `--topics`/`--exclude` filtering, `--remap from:=to`, and a choice of
//! timeline (`publish_time` by default for fidelity, `--use-log-time` for
//! recorder-local ordering). The original Zenoh encoding is recovered from the
//! MCAP channel metadata (`zenoh.encoding`, §4.5) so re-published bytes are
//! byte-identical to what was recorded.
//!
//! The module is split so the time-and-filter logic is pure and unit-testable
//! without a live router:
//! - [`read_recording_messages`] turns a recording's chunk files into
//!   [`ReplayMessage`]s (filesystem only, no Zenoh).
//! - [`plan`] applies the [`ReplayOptions`] (trim / filter / remap / rate) and
//!   returns a [`PlannedMessage`] schedule with wall-clock offsets — pure.
//! - [`run_replay`] drives that schedule against any [`ReplaySink`], sleeping
//!   between messages; the CLI supplies a Zenoh-backed sink.

use std::path::Path;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::storage::recording::Recording;
use crate::storage::ring_buffer::{META_ZENOH_ENCODING, META_ZENOH_TOPIC};

/// One message recovered from a recording, ready to be filtered and replayed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayMessage {
    /// The topic to re-publish on (the recorded Zenoh key).
    pub topic: String,
    /// The Zenoh encoding string to set on the re-published sample (§4.5).
    pub zenoh_encoding: String,
    /// Publish-time from the recording (ns).
    pub publish_time_ns: u64,
    /// Recorder-clock log-time from the recording (ns).
    pub log_time_ns: u64,
    /// Raw payload bytes.
    pub data: Vec<u8>,
}

impl ReplayMessage {
    /// The timestamp this message is ordered/scheduled by, per `use_log_time`.
    fn timeline(&self, use_log_time: bool) -> u64 {
        if use_log_time {
            self.log_time_ns
        } else {
            self.publish_time_ns
        }
    }
}

/// A message scheduled for replay: the resolved (possibly remapped) topic, the
/// encoding, the payload, and the wall-clock delay from playback start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedMessage {
    /// Topic to publish on (after `--remap`).
    pub topic: String,
    /// Zenoh encoding to set.
    pub zenoh_encoding: String,
    /// Payload bytes.
    pub data: Vec<u8>,
    /// Delay from playback start, after `--rate` scaling (ns).
    pub offset_ns: u64,
}

/// Options controlling a replay (mirror the CLI flags, §5).
#[derive(Debug, Clone)]
pub struct ReplayOptions {
    /// Playback speed multiplier (`--rate`). `1.0` = original rate; `2.0` = 2×.
    pub rate: f64,
    /// Repeat indefinitely (`--loop`).
    pub loop_playback: bool,
    /// Drop messages with a timeline timestamp below this (`--start-time`, ns).
    pub start_time_ns: Option<u64>,
    /// Drop messages with a timeline timestamp above this (`--end-time`, ns).
    pub end_time_ns: Option<u64>,
    /// Include only topics intersecting one of these key patterns (empty = all).
    pub topics: Vec<String>,
    /// Drop topics intersecting one of these key patterns (`--exclude`).
    pub exclude: Vec<String>,
    /// Topic remaps applied to the published key (`--remap from:=to`).
    pub remap: Vec<(String, String)>,
    /// Order/schedule by `log_time` instead of `publish_time` (`--use-log-time`).
    pub use_log_time: bool,
}

impl Default for ReplayOptions {
    fn default() -> Self {
        Self {
            rate: 1.0,
            loop_playback: false,
            start_time_ns: None,
            end_time_ns: None,
            topics: Vec::new(),
            exclude: Vec::new(),
            remap: Vec::new(),
            use_log_time: false,
        }
    }
}

/// Errors from planning or driving a replay.
#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    /// A `--topics`/`--exclude` pattern or a recorded topic was not a valid key.
    #[error("invalid key expression {pattern:?}: {detail}")]
    InvalidPattern { pattern: String, detail: String },
    /// `--rate` was zero or negative.
    #[error("replay rate must be > 0 (got {0})")]
    InvalidRate(f64),
    /// A `--remap` argument was not of the form `from:=to`.
    #[error("invalid remap {0:?}: expected the form from:=to")]
    InvalidRemap(String),
    /// Reading a chunk file failed.
    #[error("io error reading {path}: {detail}")]
    Io { path: String, detail: String },
    /// An MCAP chunk could not be parsed.
    #[error("mcap read error in {path}: {detail}")]
    Mcap { path: String, detail: String },
    /// The replay sink failed to publish.
    #[error("publish failed for {topic}: {detail}")]
    Publish { topic: String, detail: String },
}

/// Parse a single `from:=to` remap argument (ros2 bag style).
pub fn parse_remap(arg: &str) -> Result<(String, String), ReplayError> {
    let (from, to) = arg
        .split_once(":=")
        .ok_or_else(|| ReplayError::InvalidRemap(arg.to_string()))?;
    if from.is_empty() || to.is_empty() {
        return Err(ReplayError::InvalidRemap(arg.to_string()));
    }
    Ok((from.to_string(), to.to_string()))
}

/// Whether `topic` is selected by `patterns` (any pattern whose key expression
/// intersects the concrete topic key). An empty pattern list matches everything.
fn matches_any(topic: &str, patterns: &[String]) -> Result<bool, ReplayError> {
    if patterns.is_empty() {
        return Ok(true);
    }
    let topic_ke =
        zenoh::key_expr::KeyExpr::try_from(topic).map_err(|e| ReplayError::InvalidPattern {
            pattern: topic.to_string(),
            detail: e.to_string(),
        })?;
    for pat in patterns {
        let pat_ke = zenoh::key_expr::KeyExpr::try_from(pat.as_str()).map_err(|e| {
            ReplayError::InvalidPattern {
                pattern: pat.clone(),
                detail: e.to_string(),
            }
        })?;
        if pat_ke.intersects(&topic_ke) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Apply remaps to a topic (first matching `from` wins). Remaps match the full
/// recorded key exactly; non-matching topics pass through unchanged.
fn apply_remap(topic: &str, remap: &[(String, String)]) -> String {
    for (from, to) in remap {
        if from == topic {
            return to.clone();
        }
    }
    topic.to_string()
}

/// Build the replay schedule from recovered messages and options (pure).
///
/// Steps: filter by `--topics`/`--exclude` and the `--start-time`/`--end-time`
/// trim, sort by the selected timeline, then compute each message's wall-clock
/// offset from the first surviving message scaled by `--rate`, and apply
/// `--remap`. The result is ordered and ready for [`run_replay`].
pub fn plan(
    messages: &[ReplayMessage],
    opts: &ReplayOptions,
) -> Result<Vec<PlannedMessage>, ReplayError> {
    if !opts.rate.is_finite() || opts.rate <= 0.0 {
        return Err(ReplayError::InvalidRate(opts.rate));
    }

    // Filter: topic include/exclude and the time-window trim.
    let mut kept: Vec<&ReplayMessage> = Vec::new();
    for m in messages {
        let ts = m.timeline(opts.use_log_time);
        if let Some(start) = opts.start_time_ns {
            if ts < start {
                continue;
            }
        }
        if let Some(end) = opts.end_time_ns {
            if ts > end {
                continue;
            }
        }
        if !matches_any(&m.topic, &opts.topics)? {
            continue;
        }
        if !opts.exclude.is_empty() && matches_any(&m.topic, &opts.exclude)? {
            continue;
        }
        kept.push(m);
    }

    // Stable sort by the selected timeline so equal timestamps preserve order.
    kept.sort_by_key(|m| m.timeline(opts.use_log_time));

    let Some(base) = kept.first().map(|m| m.timeline(opts.use_log_time)) else {
        return Ok(Vec::new());
    };

    let planned = kept
        .into_iter()
        .map(|m| {
            let raw = m.timeline(opts.use_log_time).saturating_sub(base);
            // Scale by rate: higher rate → smaller delay.
            let offset_ns = (raw as f64 / opts.rate) as u64;
            PlannedMessage {
                topic: apply_remap(&m.topic, &opts.remap),
                zenoh_encoding: m.zenoh_encoding.clone(),
                data: m.data.clone(),
                offset_ns,
            }
        })
        .collect();
    Ok(planned)
}

/// Read all messages from a recording's chunk files, in chunk-index order.
///
/// Topic and encoding are recovered from MCAP channel metadata (`zenoh.topic` /
/// `zenoh.encoding`, §4.5), falling back to the MCAP channel `topic` /
/// `message_encoding` when a recording predates those metadata keys.
pub fn read_recording_messages(
    recording_dir: &Path,
    recording: &Recording,
) -> Result<Vec<ReplayMessage>, ReplayError> {
    let chunks_dir = recording_dir.join("chunks");
    let mut out = Vec::new();
    for chunk in &recording.chunks {
        let path = chunks_dir.join(&chunk.name);
        let bytes = std::fs::read(&path).map_err(|e| ReplayError::Io {
            path: path.display().to_string(),
            detail: e.to_string(),
        })?;
        let stream = mcap::MessageStream::new(&bytes).map_err(|e| ReplayError::Mcap {
            path: path.display().to_string(),
            detail: e.to_string(),
        })?;
        for msg in stream {
            let msg = msg.map_err(|e| ReplayError::Mcap {
                path: path.display().to_string(),
                detail: e.to_string(),
            })?;
            let topic = msg
                .channel
                .metadata
                .get(META_ZENOH_TOPIC)
                .cloned()
                .unwrap_or_else(|| msg.channel.topic.clone());
            let zenoh_encoding = msg
                .channel
                .metadata
                .get(META_ZENOH_ENCODING)
                .cloned()
                .unwrap_or_else(|| msg.channel.message_encoding.clone());
            out.push(ReplayMessage {
                topic,
                zenoh_encoding,
                publish_time_ns: msg.publish_time,
                log_time_ns: msg.log_time,
                data: msg.data.into_owned(),
            });
        }
    }
    Ok(out)
}

/// A destination for replayed samples. The CLI implements this over a Zenoh
/// session; tests implement it over an in-memory buffer.
#[async_trait]
pub trait ReplaySink {
    /// Publish `payload` on `topic` with the given Zenoh `encoding`.
    async fn publish(&self, topic: &str, encoding: &str, payload: &[u8])
        -> Result<(), ReplayError>;
}

/// Outcome of a replay run.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ReplayStats {
    /// Total messages published across all passes.
    pub messages_published: u64,
    /// Number of full passes completed (a cancelled partial pass is not counted).
    pub loops_completed: u64,
}

/// Drive a planned schedule against a sink, sleeping between messages to honor
/// the recorded timing (already rate-scaled in [`plan`]). Runs once, or forever
/// when `opts.loop_playback`, until `cancel` fires. Cancellation is checked
/// before each sleep and after each publish, so a long inter-message gap or an
/// infinite loop both stop promptly.
pub async fn run_replay<S: ReplaySink + Sync>(
    messages: &[ReplayMessage],
    opts: &ReplayOptions,
    sink: &S,
    cancel: CancellationToken,
) -> Result<ReplayStats, ReplayError> {
    let schedule = plan(messages, opts)?;
    let mut stats = ReplayStats::default();
    if schedule.is_empty() {
        return Ok(stats);
    }

    loop {
        let start = tokio::time::Instant::now();
        for pm in &schedule {
            if cancel.is_cancelled() {
                return Ok(stats);
            }
            let deadline = start + std::time::Duration::from_nanos(pm.offset_ns);
            tokio::select! {
                _ = cancel.cancelled() => return Ok(stats),
                _ = tokio::time::sleep_until(deadline) => {}
            }
            sink.publish(&pm.topic, &pm.zenoh_encoding, &pm.data)
                .await?;
            stats.messages_published += 1;
        }
        stats.loops_completed += 1;
        if !opts.loop_playback || cancel.is_cancelled() {
            return Ok(stats);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn msg(topic: &str, pub_ns: u64, log_ns: u64, data: &[u8]) -> ReplayMessage {
        ReplayMessage {
            topic: topic.to_string(),
            zenoh_encoding: "application/cbor".to_string(),
            publish_time_ns: pub_ns,
            log_time_ns: log_ns,
            data: data.to_vec(),
        }
    }

    #[test]
    fn parse_remap_forms() {
        assert_eq!(
            parse_remap("a/b:=c/d").unwrap(),
            ("a/b".to_string(), "c/d".to_string())
        );
        assert!(parse_remap("noseparator").is_err());
        assert!(parse_remap(":=to").is_err());
        assert!(parse_remap("from:=").is_err());
    }

    #[test]
    fn plan_orders_and_offsets_by_publish_time() {
        let msgs = vec![
            msg("t/a", 2000, 9999, b"second"),
            msg("t/a", 1000, 8888, b"first"),
            msg("t/a", 4000, 7777, b"third"),
        ];
        let planned = plan(&msgs, &ReplayOptions::default()).unwrap();
        assert_eq!(planned.len(), 3);
        // Sorted by publish_time; offsets relative to the earliest (1000).
        assert_eq!(&planned[0].data, b"first");
        assert_eq!(planned[0].offset_ns, 0);
        assert_eq!(&planned[1].data, b"second");
        assert_eq!(planned[1].offset_ns, 1000);
        assert_eq!(&planned[2].data, b"third");
        assert_eq!(planned[2].offset_ns, 3000);
    }

    #[test]
    fn rate_scales_offsets() {
        let msgs = vec![msg("t", 0, 0, b"a"), msg("t", 1000, 0, b"b")];
        let opts = ReplayOptions {
            rate: 2.0,
            ..ReplayOptions::default()
        };
        let planned = plan(&msgs, &opts).unwrap();
        assert_eq!(planned[1].offset_ns, 500); // 1000ns / 2x
    }

    #[test]
    fn use_log_time_changes_ordering() {
        // publish_time and log_time disagree on order.
        let msgs = vec![
            msg("t", 100, 200, b"pub_first_log_second"),
            msg("t", 300, 100, b"pub_second_log_first"),
        ];
        let opts = ReplayOptions {
            use_log_time: true,
            ..ReplayOptions::default()
        };
        let planned = plan(&msgs, &opts).unwrap();
        assert_eq!(&planned[0].data, b"pub_second_log_first"); // log_time 100 first
        assert_eq!(&planned[1].data, b"pub_first_log_second");
    }

    #[test]
    fn start_end_trim_by_timeline() {
        let msgs = vec![
            msg("t", 100, 0, b"a"),
            msg("t", 200, 0, b"b"),
            msg("t", 300, 0, b"c"),
            msg("t", 400, 0, b"d"),
        ];
        let opts = ReplayOptions {
            start_time_ns: Some(200),
            end_time_ns: Some(300),
            ..ReplayOptions::default()
        };
        let planned = plan(&msgs, &opts).unwrap();
        let datas: Vec<&[u8]> = planned.iter().map(|p| p.data.as_slice()).collect();
        assert_eq!(datas, vec![b"b".as_slice(), b"c".as_slice()]);
    }

    #[test]
    fn topic_include_and_exclude_filter() {
        let msgs = vec![
            msg("bubbaloop/global/m/cam/compressed", 1, 0, b"cam"),
            msg("bubbaloop/global/m/imu/data", 2, 0, b"imu"),
            msg("bubbaloop/global/m/cam/health", 3, 0, b"health"),
        ];
        let opts = ReplayOptions {
            topics: vec!["bubbaloop/global/m/cam/**".to_string()],
            exclude: vec!["**/health".to_string()],
            ..ReplayOptions::default()
        };
        let planned = plan(&msgs, &opts).unwrap();
        assert_eq!(planned.len(), 1);
        assert_eq!(&planned[0].data, b"cam");
    }

    #[test]
    fn remap_rewrites_topic() {
        let msgs = vec![msg("old/topic", 0, 0, b"x")];
        let opts = ReplayOptions {
            remap: vec![("old/topic".to_string(), "new/topic".to_string())],
            ..ReplayOptions::default()
        };
        let planned = plan(&msgs, &opts).unwrap();
        assert_eq!(planned[0].topic, "new/topic");
    }

    #[test]
    fn zero_rate_is_rejected() {
        let msgs = vec![msg("t", 0, 0, b"x")];
        let opts = ReplayOptions {
            rate: 0.0,
            ..ReplayOptions::default()
        };
        assert!(matches!(
            plan(&msgs, &opts),
            Err(ReplayError::InvalidRate(_))
        ));
    }

    #[derive(Default)]
    struct MockSink {
        published: Mutex<Vec<(String, String, Vec<u8>)>>,
        cancel_after: Option<(usize, CancellationToken)>,
    }

    #[async_trait]
    impl ReplaySink for MockSink {
        async fn publish(
            &self,
            topic: &str,
            encoding: &str,
            payload: &[u8],
        ) -> Result<(), ReplayError> {
            let mut g = self.published.lock().unwrap();
            g.push((topic.to_string(), encoding.to_string(), payload.to_vec()));
            if let Some((n, token)) = &self.cancel_after {
                if g.len() >= *n {
                    token.cancel();
                }
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn run_replay_publishes_in_order() {
        let msgs = vec![
            msg("t/a", 1000, 0, b"first"),
            msg("t/b", 3000, 0, b"second"),
        ];
        let sink = MockSink::default();
        let stats = run_replay(
            &msgs,
            &ReplayOptions::default(),
            &sink,
            CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(stats.messages_published, 2);
        assert_eq!(stats.loops_completed, 1);
        let g = sink.published.lock().unwrap();
        assert_eq!(g[0].0, "t/a");
        assert_eq!(&g[0].2, b"first");
        assert_eq!(g[1].0, "t/b");
        assert_eq!(&g[1].2, b"second");
    }

    #[tokio::test]
    async fn run_replay_loops_until_cancelled() {
        let msgs = vec![msg("t", 0, 0, b"x"), msg("t", 100, 0, b"y")];
        let token = CancellationToken::new();
        let sink = MockSink {
            published: Mutex::new(Vec::new()),
            // Cancel once 5 messages have gone out (mid-third pass).
            cancel_after: Some((5, token.clone())),
        };
        let opts = ReplayOptions {
            loop_playback: true,
            ..ReplayOptions::default()
        };
        let stats = run_replay(&msgs, &opts, &sink, token).await.unwrap();
        assert_eq!(stats.messages_published, 5);
        // Two full passes (4 msgs) completed; the third was cancelled mid-way.
        assert_eq!(stats.loops_completed, 2);
    }

    #[tokio::test]
    async fn run_replay_empty_schedule_is_noop() {
        let sink = MockSink::default();
        let stats = run_replay(
            &[],
            &ReplayOptions::default(),
            &sink,
            CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(stats, ReplayStats::default());
    }

    #[test]
    fn read_recording_roundtrips_through_real_mcap() {
        use crate::storage::recording::{RecordingMode, Selection};
        use crate::storage::ring_buffer::{seal_samples, McapWriteConfig, Sample};

        // Seal real samples into an on-disk MCAP recording, then read them back
        // — proving the §4.5 metadata + dual timestamps survive the container.
        let dir = tempfile::tempdir().unwrap();
        let rec_dir = dir.path().join("rec");
        let samples = vec![
            Sample {
                topic: "bubbaloop/global/m/cam/compressed".to_string(),
                message_encoding: "protobuf".to_string(),
                zenoh_encoding: "application/protobuf".to_string(),
                schema_name: Some("bubbaloop.camera.v1.CompressedImage".to_string()),
                publish_time_ns: 1000,
                log_time_ns: 1100,
                data: vec![9, 8, 7, 6],
            },
            Sample {
                topic: "bubbaloop/global/m/imu/data".to_string(),
                message_encoding: "cbor".to_string(),
                zenoh_encoding: "application/cbor".to_string(),
                schema_name: None,
                publish_time_ns: 2000,
                log_time_ns: 2100,
                data: vec![1, 2],
            },
        ];
        let recording = seal_samples(
            &rec_dir,
            "rec",
            "m",
            RecordingMode::Streaming,
            None,
            Selection::default(),
            &samples,
            &McapWriteConfig::default(),
            0,
        )
        .unwrap();

        let mut recovered = read_recording_messages(&rec_dir, &recording).unwrap();
        recovered.sort_by_key(|m| m.publish_time_ns);
        assert_eq!(recovered.len(), 2);

        assert_eq!(recovered[0].topic, "bubbaloop/global/m/cam/compressed");
        assert_eq!(recovered[0].zenoh_encoding, "application/protobuf");
        assert_eq!(recovered[0].publish_time_ns, 1000);
        assert_eq!(recovered[0].log_time_ns, 1100);
        assert_eq!(&recovered[0].data, &[9, 8, 7, 6]);

        assert_eq!(recovered[1].zenoh_encoding, "application/cbor");
        assert_eq!(recovered[1].log_time_ns, 2100);

        // And the recovered messages plan into a 1000ns-spaced schedule.
        let planned = plan(&recovered, &ReplayOptions::default()).unwrap();
        assert_eq!(planned[0].offset_ns, 0);
        assert_eq!(planned[1].offset_ns, 1000);
    }
}
