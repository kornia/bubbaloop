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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_header() -> Header {
        Header {
            schema_uri: "bubbaloop://test/v1".to_string(),
            source_instance: "tapo_terrace".to_string(),
            monotonic_seq: 42,
            ts_ns: 1_700_000_000_000_000_000,
        }
    }

    #[test]
    fn header_roundtrips_through_cbor() {
        let h = sample_header();
        let mut buf = Vec::new();
        ciborium::ser::into_writer(&h, &mut buf).unwrap();
        let decoded: Header = ciborium::de::from_reader(&buf[..]).unwrap();
        assert_eq!(decoded, h);
    }

    #[test]
    fn envelope_roundtrips_with_string_body() {
        let env = Envelope {
            header: sample_header(),
            body: "hello".to_string(),
        };
        let mut buf = Vec::new();
        ciborium::ser::into_writer(&env, &mut buf).unwrap();
        let decoded: Envelope<String> = ciborium::de::from_reader(&buf[..]).unwrap();
        assert_eq!(decoded.body, "hello");
        assert_eq!(decoded.header.source_instance, "tapo_terrace");
        assert_eq!(decoded.header.monotonic_seq, 42);
    }

    #[test]
    fn envelope_roundtrips_with_struct_body() {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        struct Payload {
            temperature_c: i32,
            label: String,
        }

        let body = Payload {
            temperature_c: 23,
            label: "kitchen".into(),
        };
        let env = Envelope {
            header: sample_header(),
            body: body.clone(),
        };
        let mut buf = Vec::new();
        ciborium::ser::into_writer(&env, &mut buf).unwrap();
        let decoded: Envelope<Payload> = ciborium::de::from_reader(&buf[..]).unwrap();
        assert_eq!(decoded.body, body);
    }

    #[test]
    fn envelope_ref_serializes_identically_to_owned() {
        // EnvelopeRef must produce the same wire bytes as Envelope — otherwise
        // borrow-on-publish would be a silent wire-format break.
        let header = sample_header();
        let body = "payload".to_string();

        let owned = Envelope {
            header: header.clone(),
            body: body.clone(),
        };
        let borrowed = EnvelopeRef {
            header: header.clone(),
            body: &body,
        };

        let mut owned_buf = Vec::new();
        let mut borrowed_buf = Vec::new();
        ciborium::ser::into_writer(&owned, &mut owned_buf).unwrap();
        ciborium::ser::into_writer(&borrowed, &mut borrowed_buf).unwrap();
        assert_eq!(owned_buf, borrowed_buf);
    }

    #[test]
    fn now_ns_is_post_2020() {
        // Sanity: clock should be > Jan 1 2020 in ns. Catches the unwrap_or(0)
        // fallback firing on a misconfigured environment.
        let jan_2020_ns: u64 = 1_577_836_800_000_000_000;
        assert!(now_ns() > jan_2020_ns);
    }
}
