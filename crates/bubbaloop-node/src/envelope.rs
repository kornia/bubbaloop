//! Provenance envelope for CBOR payloads.
//!
//! Every CBOR payload published via [`CborPublisher`](crate::CborPublisher)
//! and [`CborPublisherShm`](crate::CborPublisherShm) is wrapped in a
//! `{header, body}` map before serialization. The header carries enough
//! context that an LLM (or any consumer) can identify what the message is
//! and where it came from from the message alone — no side-channel
//! discovery needed.
//!
//! ## Wire shape
//!
//! ```text
//! {
//!   "header": {
//!     "schema_uri":      str,   // optional caller-provided identifier (default "")
//!     "source_instance": str,   // node's instance_name, filled by SDK
//!     "monotonic_seq":   u64,   // per-publisher counter, starts at 0
//!     "ts_ns":           u64,   // wall-clock ns since unix epoch
//!   },
//!   "body": <user payload>
//! }
//! ```

use serde::{Deserialize, Serialize};

/// Provenance header attached to every CBOR payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Header {
    /// Optional caller-provided schema identifier (e.g. `"bubbaloop://embedder/v1"`).
    /// The SDK never fabricates this; it stays empty unless the caller passes one.
    pub schema_uri: String,
    /// The publishing node's `instance_name`, filled by the SDK.
    pub source_instance: String,
    /// Per-publisher counter, starts at 0, increments on every successful `put`.
    pub monotonic_seq: u64,
    /// Wall-clock nanoseconds since the unix epoch at the time of `put`.
    pub ts_ns: u64,
}

/// `{header, body}` envelope wrapping a CBOR payload.
///
/// The body is a borrowed reference at publish time (no clone) and an owned
/// value at decode time, so we keep this generic over `T`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Envelope<T> {
    pub header: Header,
    pub body: T,
}

/// Internal serialize-side envelope that borrows the body.
///
/// Lets `CborPublisher::put` wrap a `&S` without cloning it: the borrowed body
/// is serialized directly via ciborium.
#[derive(Serialize)]
pub(crate) struct EnvelopeRef<'a, S: Serialize + ?Sized> {
    pub header: Header,
    pub body: &'a S,
}

/// Wall-clock nanoseconds since the unix epoch, or `0` on clock failure.
pub(crate) fn now_ns() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}
