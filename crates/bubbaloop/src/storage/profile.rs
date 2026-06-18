//! Recording profiles (`~/.bubbaloop/profiles/*.yaml`, spec §4.3).
//!
//! Profiles are small, committable YAML files that capture "the record button"
//! a team uses. Field names borrow from the OpenTelemetry Collector
//! (`sending_queue`, `retry_on_failure`) for forward compatibility with the v2
//! multi-pipeline mode. The top-level `pipelines:` key is **reserved** in v1 —
//! its presence triggers [`ProfileError::RequiresV2`] rather than being silently
//! ignored (spec §4.3.2).
//!
//! The loader is strict (spec §4.3.1): unknown keys (top-level and nested) are
//! rejected as typos, ring-buffer mode requires `window_secs` + `trigger`,
//! `trigger: on-event` is Phase 3, an empty selection is rejected, and
//! `chunk_size_bytes` is range-checked.

use super::recording::{RecordingMode, Trigger};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// rosbag2's validated default chunk size (spec §3.3.7).
pub const DEFAULT_CHUNK_SIZE_BYTES: u32 = 786_432;
/// Minimum accepted chunk size (4 KiB, spec §4.3.1).
pub const MIN_CHUNK_SIZE_BYTES: u32 = 4 * 1024;
/// Maximum accepted chunk size (16 MiB, spec §4.3.1).
pub const MAX_CHUNK_SIZE_BYTES: u32 = 16 * 1024 * 1024;

/// Top-level keys accepted in a v1 profile. Anything else is a typo and fails
/// loud — except `pipelines`, which is the reserved v2 entry point handled
/// separately, and `schema_version`, validated for the v1/v2 boundary.
const ALLOWED_TOP_LEVEL: &[&str] = &[
    "schema_version",
    "name",
    "description",
    "topics",
    "regex",
    "exclude",
    "include_local",
    "chunk_size_bytes",
    "compression",
    "compression_level",
    "chunk_crc",
    "mode",
    "window_secs",
    "trigger",
    "sending_queue",
    "retry_on_failure",
];

/// A parsed, validated v1 profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    /// Profile name.
    pub name: String,
    /// Human description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    // --- Topic selection (additive composition, §4.3) ---
    /// Additive literal/wildcard include patterns.
    #[serde(default)]
    pub topics: Vec<String>,
    /// Additive regex include.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regex: Option<String>,
    /// Subtractive exclude patterns.
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Include SHM-only `bubbaloop/local/**` topics.
    #[serde(default)]
    pub include_local: bool,

    // --- MCAP writer config ---
    /// Target chunk size in bytes. Accepts YAML underscores (`786_432`).
    #[serde(default = "default_chunk_size", deserialize_with = "de_flexible_u32")]
    pub chunk_size_bytes: u32,
    /// Compression codec.
    #[serde(default)]
    pub compression: CompressionKind,
    /// Compression level.
    #[serde(default)]
    pub compression_level: CompressionLevel,
    /// Whether to write per-chunk CRCs.
    #[serde(default = "default_true")]
    pub chunk_crc: bool,

    // --- Session config ---
    /// Capture mode.
    #[serde(default)]
    pub mode: RecordingMode,
    /// Sliding-window length (required when `mode: ring_buffer`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_secs: Option<u64>,
    /// Ring-buffer trigger (required when `mode: ring_buffer`; `manual` only in v1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<Trigger>,

    // --- Upload behavior (OTel-style names) ---
    /// Bounded send queue config.
    #[serde(default)]
    pub sending_queue: SendingQueue,
    /// Retry/backoff config.
    #[serde(default)]
    pub retry_on_failure: RetryOnFailure,
}

/// Compression codec for chunks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompressionKind {
    /// zstd (rosbag2 default).
    #[default]
    Zstd,
    /// lz4.
    Lz4,
    /// No compression.
    None,
}

/// Compression level.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompressionLevel {
    /// Fast (rosbag2 default).
    #[default]
    Fast,
    /// Balanced default.
    Default,
    /// Maximum ratio.
    High,
}

/// Bounded send-queue config (`sending_queue:`). Unknown nested keys are
/// rejected (spec §4.3.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SendingQueue {
    /// Whether background upload is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Jobs in flight before backpressure.
    #[serde(default = "default_queue_size", deserialize_with = "de_flexible_u32")]
    pub queue_size: u32,
    /// Reserved for a future file-backed queue (forward-compat only in v1, §3.4.2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_dir: Option<String>,
}

impl Default for SendingQueue {
    fn default() -> Self {
        Self {
            enabled: true,
            queue_size: default_queue_size(),
            storage_dir: None,
        }
    }
}

/// Retry/backoff config (`retry_on_failure:`). Unknown nested keys are rejected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetryOnFailure {
    /// Whether retries are enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Initial backoff interval.
    #[serde(default = "default_initial_interval")]
    pub initial_interval: HumanDuration,
    /// Maximum backoff interval (clamp).
    #[serde(default = "default_max_interval")]
    pub max_interval: HumanDuration,
    /// Total time before a job is dead-lettered.
    #[serde(default = "default_max_elapsed")]
    pub max_elapsed_time: HumanDuration,
}

impl Default for RetryOnFailure {
    fn default() -> Self {
        Self {
            enabled: true,
            initial_interval: default_initial_interval(),
            max_interval: default_max_interval(),
            max_elapsed_time: default_max_elapsed(),
        }
    }
}

/// A duration written human-style in YAML (`5s`, `60s`, `5m`, `1h`, `500ms`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HumanDuration(pub Duration);

impl HumanDuration {
    /// The wrapped [`Duration`].
    pub fn as_duration(&self) -> Duration {
        self.0
    }
}

impl Serialize for HumanDuration {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&format_duration(self.0))
    }
}

impl<'de> Deserialize<'de> for HumanDuration {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        parse_duration(&s)
            .map(HumanDuration)
            .map_err(serde::de::Error::custom)
    }
}

fn default_chunk_size() -> u32 {
    DEFAULT_CHUNK_SIZE_BYTES
}
fn default_true() -> bool {
    true
}
fn default_queue_size() -> u32 {
    1000
}
fn default_initial_interval() -> HumanDuration {
    HumanDuration(Duration::from_secs(5))
}
fn default_max_interval() -> HumanDuration {
    HumanDuration(Duration::from_secs(60))
}
fn default_max_elapsed() -> HumanDuration {
    HumanDuration(Duration::from_secs(300))
}

/// Deserialize a u32 that may be written with YAML underscores (`786_432`) or as
/// a plain integer. libyaml's core schema treats `786_432` as a string, so we
/// accept both forms and strip underscores (spec §4.3.3 number normalization).
fn de_flexible_u32<'de, D>(d: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = serde_yaml::Value::deserialize(d)?;
    match v {
        serde_yaml::Value::Number(n) => n
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| serde::de::Error::custom(format!("not a valid u32: {n}"))),
        serde_yaml::Value::String(s) => s
            .replace('_', "")
            .parse::<u32>()
            .map_err(|_| serde::de::Error::custom(format!("not a valid u32: {s}"))),
        other => Err(serde::de::Error::custom(format!(
            "expected an integer, found {other:?}"
        ))),
    }
}

/// Parse `5s` / `500ms` / `5m` / `1h` into a [`Duration`].
fn parse_duration(s: &str) -> Result<Duration, String> {
    let s = s.trim();
    let (num, unit) = if let Some(rest) = s.strip_suffix("ms") {
        (rest, "ms")
    } else if let Some(rest) = s.strip_suffix('s') {
        (rest, "s")
    } else if let Some(rest) = s.strip_suffix('m') {
        (rest, "m")
    } else if let Some(rest) = s.strip_suffix('h') {
        (rest, "h")
    } else {
        return Err(format!("duration '{s}' missing a unit (ms/s/m/h)"));
    };
    let value: u64 = num
        .trim()
        .parse()
        .map_err(|_| format!("duration '{s}' has a non-integer value"))?;
    let dur = match unit {
        "ms" => Duration::from_millis(value),
        "s" => Duration::from_secs(value),
        "m" => Duration::from_secs(value * 60),
        "h" => Duration::from_secs(value * 3600),
        _ => unreachable!(),
    };
    Ok(dur)
}

/// Render a [`Duration`] back to the most natural human form.
fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    let millis = d.subsec_millis();
    if secs == 0 && millis > 0 {
        return format!("{millis}ms");
    }
    // Prefer the largest exact unit. Division-then-multiply (rather than `% == 0`)
    // keeps this MSRV-1.75 friendly — `u64::is_multiple_of` is only 1.87+.
    let hours = secs / 3600;
    let mins = secs / 60;
    if secs > 0 && hours * 3600 == secs {
        format!("{hours}h")
    } else if secs > 0 && mins * 60 == secs {
        format!("{mins}m")
    } else {
        format!("{secs}s")
    }
}

/// Default profiles directory: `~/.bubbaloop/profiles`.
pub fn profiles_dir() -> Result<PathBuf, ProfileError> {
    let home = dirs::home_dir().ok_or(ProfileError::NoHomeDir)?;
    Ok(home.join(".bubbaloop").join("profiles"))
}

/// Parse and fully validate a profile from a YAML string.
pub fn parse(yaml: &str) -> Result<Profile, ProfileError> {
    // 1. Inspect the raw mapping for the reserved v2 key and unknown top-level keys.
    let raw: serde_yaml::Value =
        serde_yaml::from_str(yaml).map_err(|e| ProfileError::Parse(e.to_string()))?;
    let map = raw
        .as_mapping()
        .ok_or_else(|| ProfileError::Parse("profile must be a YAML mapping".into()))?;

    if map.contains_key("pipelines") {
        return Err(ProfileError::RequiresV2 {
            reason: "found reserved `pipelines:` key".into(),
        });
    }
    if let Some(sv) = map.get("schema_version") {
        let v = sv.as_u64().unwrap_or(1);
        if v > 1 {
            return Err(ProfileError::RequiresV2 {
                reason: format!("schema_version {v} requires bubbaloop v2"),
            });
        }
    }
    for key in map.keys() {
        if let Some(k) = key.as_str() {
            if !ALLOWED_TOP_LEVEL.contains(&k) {
                return Err(ProfileError::UnknownField {
                    field: k.to_string(),
                });
            }
        }
    }

    // 2. Deserialize into the typed profile (nested deny_unknown_fields catches
    //    typos under sending_queue / retry_on_failure).
    let profile: Profile =
        serde_yaml::from_str(yaml).map_err(|e| ProfileError::Parse(e.to_string()))?;

    // 3. Semantic validation.
    validate(&profile)?;
    Ok(profile)
}

/// Semantic validation rules (§4.3.1), applied after structural parsing.
pub fn validate(p: &Profile) -> Result<(), ProfileError> {
    if p.name.trim().is_empty() {
        return Err(ProfileError::UnknownField {
            field: "name (must be non-empty)".into(),
        });
    }

    if p.mode == RecordingMode::RingBuffer {
        if p.window_secs.is_none() {
            return Err(ProfileError::MissingField {
                field: "window_secs",
                required_when: "mode=ring_buffer",
            });
        }
        match p.trigger {
            None => {
                return Err(ProfileError::MissingField {
                    field: "trigger",
                    required_when: "mode=ring_buffer",
                })
            }
            Some(Trigger::OnEvent) => {
                return Err(ProfileError::NotYetImplemented {
                    feature: "on-event trigger",
                    available_in: "Phase 3",
                })
            }
            Some(Trigger::Manual) => {}
        }
    } else if p.trigger == Some(Trigger::OnEvent) {
        return Err(ProfileError::NotYetImplemented {
            feature: "on-event trigger",
            available_in: "Phase 3",
        });
    }

    if p.topics.is_empty() && p.regex.is_none() {
        return Err(ProfileError::EmptySelection);
    }

    if p.chunk_size_bytes < MIN_CHUNK_SIZE_BYTES || p.chunk_size_bytes > MAX_CHUNK_SIZE_BYTES {
        return Err(ProfileError::OutOfRange {
            field: "chunk_size_bytes",
            value: p.chunk_size_bytes as u64,
            min: MIN_CHUNK_SIZE_BYTES as u64,
            max: MAX_CHUNK_SIZE_BYTES as u64,
        });
    }

    Ok(())
}

/// Load and validate a profile from a file.
pub fn load(path: impl AsRef<Path>) -> Result<Profile, ProfileError> {
    let path = path.as_ref();
    let contents = std::fs::read_to_string(path).map_err(|e| ProfileError::Io {
        path: path.display().to_string(),
        detail: e.to_string(),
    })?;
    parse(&contents)
}

/// Load a profile by name from the default profiles directory.
pub fn load_named(name: &str) -> Result<Profile, ProfileError> {
    load(profiles_dir()?.join(format!("{name}.yaml")))
}

/// Validate and save a profile to a file (atomically via tmp + rename).
pub fn save(path: impl AsRef<Path>, profile: &Profile) -> Result<(), ProfileError> {
    validate(profile)?;
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ProfileError::Io {
            path: parent.display().to_string(),
            detail: e.to_string(),
        })?;
    }
    let yaml =
        serde_yaml::to_string(profile).map_err(|e| ProfileError::Serialize(e.to_string()))?;
    let tmp = path.with_extension("yaml.tmp");
    std::fs::write(&tmp, yaml.as_bytes()).map_err(|e| ProfileError::Io {
        path: tmp.display().to_string(),
        detail: e.to_string(),
    })?;
    std::fs::rename(&tmp, path).map_err(|e| ProfileError::Io {
        path: path.display().to_string(),
        detail: e.to_string(),
    })?;
    Ok(())
}

/// Canonical SHA-256 of a profile for manifest provenance (spec §4.3.3).
///
/// Computed over a canonical JSON form (recursively key-sorted, numbers
/// normalized by the typed round-trip) so whitespace, comment, key-order, and
/// `786_432`-vs-`786432` edits do **not** change the hash, while semantic edits
/// do. Returns the 64-char lowercase hex digest.
pub fn canonical_sha256(p: &Profile) -> String {
    let value = serde_json::to_value(p).expect("profile is always serializable");
    let canonical = canonicalize_json(&value);
    super::integrity::to_hex(&super::integrity::sha256(canonical.as_bytes()))
}

/// Serialize a JSON value with object keys recursively sorted — a deterministic
/// canonical form independent of serde_json's key-order feature flags.
fn canonicalize_json(v: &serde_json::Value) -> String {
    let mut out = String::new();
    write_canonical(v, &mut out);
    out
}

fn write_canonical(v: &serde_json::Value, out: &mut String) {
    match v {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(k).unwrap());
                out.push(':');
                write_canonical(&map[k.as_str()], out);
            }
            out.push('}');
        }
        serde_json::Value::Array(arr) => {
            out.push('[');
            for (i, e) in arr.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(e, out);
            }
            out.push(']');
        }
        other => out.push_str(&serde_json::to_string(other).unwrap()),
    }
}

/// Errors from profile loading and validation.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProfileError {
    /// No home directory could be determined.
    #[error("could not determine home directory")]
    NoHomeDir,
    /// Filesystem error.
    #[error("profile io error at {path}: {detail}")]
    Io { path: String, detail: String },
    /// YAML parse error (includes nested unknown-field rejections).
    #[error("profile parse error: {0}")]
    Parse(String),
    /// Serialization error.
    #[error("profile serialize error: {0}")]
    Serialize(String),
    /// An unknown top-level key was found (typo).
    #[error("unknown profile field: {field}")]
    UnknownField { field: String },
    /// The profile uses the reserved v2 `pipelines:` surface.
    #[error("this profile requires bubbaloop v2: {reason}")]
    RequiresV2 { reason: String },
    /// A field is required given another field's value.
    #[error("missing field `{field}` (required when {required_when})")]
    MissingField {
        field: &'static str,
        required_when: &'static str,
    },
    /// A feature is reserved for a later phase.
    #[error("`{feature}` is not implemented in v1 (available in {available_in})")]
    NotYetImplemented {
        feature: &'static str,
        available_in: &'static str,
    },
    /// No topic selection was provided.
    #[error("profile has an empty selection (set `topics` and/or `regex`)")]
    EmptySelection,
    /// A numeric field is out of its accepted range.
    #[error("`{field}` = {value} is out of range [{min}, {max}]")]
    OutOfRange {
        field: &'static str,
        value: u64,
        min: u64,
        max: u64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_yaml() -> &'static str {
        "name: demo\ntopics:\n  - 'bubbaloop/global/*/cam_*/compressed'\n"
    }

    #[test]
    fn parses_minimal_profile_with_defaults() {
        let p = parse(minimal_yaml()).unwrap();
        assert_eq!(p.name, "demo");
        assert_eq!(p.chunk_size_bytes, DEFAULT_CHUNK_SIZE_BYTES);
        assert_eq!(p.compression, CompressionKind::Zstd);
        assert_eq!(p.compression_level, CompressionLevel::Fast);
        assert!(p.chunk_crc);
        assert_eq!(p.mode, RecordingMode::Streaming);
        assert!(p.sending_queue.enabled);
        assert_eq!(p.sending_queue.queue_size, 1000);
        assert_eq!(
            p.retry_on_failure.initial_interval.as_duration(),
            Duration::from_secs(5)
        );
        assert_eq!(
            p.retry_on_failure.max_elapsed_time.as_duration(),
            Duration::from_secs(300)
        );
    }

    #[test]
    fn parses_full_otel_style_profile() {
        let yaml = r#"
name: demo
description: "Camera + IMU"
topics:
  - 'bubbaloop/global/*/cam_*/compressed'
regex: '^bubbaloop/global/.+/imu/.+'
exclude:
  - '**/health'
chunk_size_bytes: 786_432
compression: zstd
compression_level: fast
chunk_crc: true
mode: streaming
sending_queue:
  enabled: true
  queue_size: 1000
retry_on_failure:
  enabled: true
  initial_interval: 5s
  max_interval: 60s
  max_elapsed_time: 5m
"#;
        let p = parse(yaml).unwrap();
        assert_eq!(p.chunk_size_bytes, 786_432);
        assert_eq!(p.regex.as_deref(), Some("^bubbaloop/global/.+/imu/.+"));
        assert_eq!(
            p.retry_on_failure.max_interval.as_duration(),
            Duration::from_secs(60)
        );
    }

    #[test]
    fn reserved_pipelines_key_requires_v2() {
        let yaml = "name: demo\ntopics: [a]\npipelines:\n  - id: x\n";
        assert!(matches!(parse(yaml), Err(ProfileError::RequiresV2 { .. })));
    }

    #[test]
    fn schema_version_2_requires_v2() {
        let yaml = "schema_version: 2\nname: demo\ntopics: [a]\n";
        assert!(matches!(parse(yaml), Err(ProfileError::RequiresV2 { .. })));
    }

    #[test]
    fn unknown_top_level_key_rejected() {
        let yaml = "name: demo\ntopics: [a]\ncompresssion: zstd\n"; // typo
        assert_eq!(
            parse(yaml),
            Err(ProfileError::UnknownField {
                field: "compresssion".into()
            })
        );
    }

    #[test]
    fn unknown_nested_key_rejected() {
        let yaml = "name: demo\ntopics: [a]\nsending_queue:\n  enabled: true\n  bogus: 1\n";
        assert!(matches!(parse(yaml), Err(ProfileError::Parse(_))));
    }

    #[test]
    fn ring_buffer_requires_window_and_trigger() {
        let no_window = "name: d\ntopics: [a]\nmode: ring_buffer\ntrigger: manual\n";
        assert_eq!(
            parse(no_window),
            Err(ProfileError::MissingField {
                field: "window_secs",
                required_when: "mode=ring_buffer"
            })
        );

        let no_trigger = "name: d\ntopics: [a]\nmode: ring_buffer\nwindow_secs: 60\n";
        assert_eq!(
            parse(no_trigger),
            Err(ProfileError::MissingField {
                field: "trigger",
                required_when: "mode=ring_buffer"
            })
        );
    }

    #[test]
    fn ring_buffer_manual_ok() {
        let yaml = "name: d\ntopics: [a]\nmode: ring_buffer\nwindow_secs: 60\ntrigger: manual\n";
        let p = parse(yaml).unwrap();
        assert_eq!(p.mode, RecordingMode::RingBuffer);
        assert_eq!(p.trigger, Some(Trigger::Manual));
    }

    #[test]
    fn on_event_trigger_not_yet_implemented() {
        let yaml = "name: d\ntopics: [a]\nmode: ring_buffer\nwindow_secs: 60\ntrigger: on-event\n";
        assert!(matches!(
            parse(yaml),
            Err(ProfileError::NotYetImplemented { .. })
        ));
    }

    #[test]
    fn empty_selection_rejected() {
        let yaml = "name: d\n";
        assert_eq!(parse(yaml), Err(ProfileError::EmptySelection));
    }

    #[test]
    fn chunk_size_out_of_range_rejected() {
        let too_small = "name: d\ntopics: [a]\nchunk_size_bytes: 100\n";
        assert!(matches!(
            parse(too_small),
            Err(ProfileError::OutOfRange { .. })
        ));
        let too_big = "name: d\ntopics: [a]\nchunk_size_bytes: 99999999\n";
        assert!(matches!(
            parse(too_big),
            Err(ProfileError::OutOfRange { .. })
        ));
    }

    #[test]
    fn save_then_load_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("demo.yaml");
        let p = parse(minimal_yaml()).unwrap();
        save(&path, &p).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded, p);
    }

    #[test]
    fn canonical_sha256_ignores_cosmetic_edits() {
        // Same semantics, different whitespace / comments / underscores / key order.
        let a = "name: demo\ntopics: [a]\nchunk_size_bytes: 786_432\n";
        let b = "# a comment\nchunk_size_bytes: 786432\nname: demo\ntopics:\n  - a\n";
        let pa = parse(a).unwrap();
        let pb = parse(b).unwrap();
        assert_eq!(canonical_sha256(&pa), canonical_sha256(&pb));
    }

    #[test]
    fn canonical_sha256_changes_on_semantic_edit() {
        let a = parse("name: demo\ntopics: [a]\n").unwrap();
        let b = parse("name: demo\ntopics: [b]\n").unwrap(); // different topic
        assert_ne!(canonical_sha256(&a), canonical_sha256(&b));
    }

    #[test]
    fn duration_parsing_units() {
        assert_eq!(parse_duration("500ms").unwrap(), Duration::from_millis(500));
        assert_eq!(parse_duration("5s").unwrap(), Duration::from_secs(5));
        assert_eq!(parse_duration("5m").unwrap(), Duration::from_secs(300));
        assert_eq!(parse_duration("1h").unwrap(), Duration::from_secs(3600));
        assert!(parse_duration("5").is_err()); // missing unit
        assert!(parse_duration("xs").is_err());
    }

    #[test]
    fn duration_format_roundtrips() {
        for s in ["500ms", "5s", "5m", "1h"] {
            let d = parse_duration(s).unwrap();
            assert_eq!(format_duration(d), s);
        }
    }
}
