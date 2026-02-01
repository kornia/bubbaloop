//! Bubbaloop Storage Service
//!
//! A generic, message-type agnostic storage service that records Zenoh messages
//! to LanceDB backed by GCP Cloud Storage. Extracts Header metadata from any
//! protobuf message following the bubbaloop convention (Header at field 1).
//!
//! # Architecture
//!
//! ```text
//! Zenoh topics ──► Recorder ──► StorageClient ──► LanceDB (local or gs://)
//!                    │                │
//!                    │ extract_header │ messages / sessions / schemas tables
//!                    │                │
//!                    └─ batch buffer ─┘
//! ```
//!
//! # Modules
//!
//! - [`config`] — YAML-based configuration with topic patterns and schema hints.
//! - [`error`] — Unified error type for storage operations.
//! - [`header`] — Generic protobuf Header extraction from raw bytes.
//! - [`lancedb_client`] — LanceDB table operations (messages, sessions, schemas).
//! - [`recorder`] — Recording engine bridging Zenoh subscriptions to LanceDB.

pub mod config;
pub mod error;
pub mod header;
pub mod lancedb_client;
pub mod recorder;

/// Current wall-clock time in nanoseconds since Unix epoch.
pub(crate) fn now_nanos() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as i64
}
