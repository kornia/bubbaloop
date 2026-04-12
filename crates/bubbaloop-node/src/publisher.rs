use std::io::Cursor;
use std::num::NonZeroUsize;
use std::sync::Arc;
use zenoh::bytes::{Encoding, ZBytes};
use zenoh::qos::CongestionControl;
use zenoh::shm::{
    BlockOn, GarbageCollect, OwnedShmBuf, PosixShmProviderBackend, ShmProvider,
    ShmProviderBuilder,
};
use zenoh::Wait;

use crate::error::{NodeError, Result};

/// A declared protobuf publisher that sets `Encoding::APPLICATION_PROTOBUF` automatically.
///
/// Created via [`NodeContext::publisher_proto`](crate::NodeContext::publisher_proto).
/// The encoding is declared once at construction and reused for every [`put`](ProtoPublisher::put).
pub struct ProtoPublisher<T: prost::Message + Default> {
    publisher: zenoh::pubsub::Publisher<'static>,
    _marker: std::marker::PhantomData<T>,
}

impl<T: prost::Message + Default + crate::MessageTypeName> ProtoPublisher<T> {
    pub(crate) async fn new(session: &Arc<zenoh::Session>, key_expr: &str) -> Result<Self> {
        let encoding = Encoding::APPLICATION_PROTOBUF.with_schema(T::type_name());
        let publisher = session
            .declare_publisher(key_expr.to_string())
            .encoding(encoding)
            .await
            .map_err(|e| NodeError::PublisherDeclare {
                topic: key_expr.to_string(),
                source: e,
            })?;

        log::debug!(
            "ProtoPublisher declared on '{}' (type={})",
            key_expr,
            T::type_name()
        );
        Ok(Self {
            publisher,
            _marker: std::marker::PhantomData,
        })
    }

    /// Encode `msg` as protobuf bytes and publish it.
    pub async fn put(&self, msg: &T) -> Result<()> {
        self.publisher
            .put(msg.encode_to_vec())
            .await
            .map_err(NodeError::Publish)
    }
}

/// A declared JSON publisher that sets `Encoding::APPLICATION_JSON` automatically.
///
/// Created via [`NodeContext::publisher_json`](crate::NodeContext::publisher_json).
/// The encoding is declared once at construction and reused for every [`put`](JsonPublisher::put).
pub struct JsonPublisher {
    publisher: zenoh::pubsub::Publisher<'static>,
}

impl JsonPublisher {
    pub(crate) async fn new(session: &Arc<zenoh::Session>, key_expr: &str) -> Result<Self> {
        let publisher = session
            .declare_publisher(key_expr.to_string())
            .encoding(Encoding::APPLICATION_JSON)
            .await
            .map_err(|e| NodeError::PublisherDeclare {
                topic: key_expr.to_string(),
                source: e,
            })?;

        log::debug!("JsonPublisher declared on '{}'", key_expr);
        Ok(Self { publisher })
    }

    /// Serialize any `Serialize` value as JSON bytes and publish it.
    pub async fn put<S: serde::Serialize>(&self, value: &S) -> Result<()> {
        let bytes = serde_json::to_vec(value)?;
        self.publisher.put(bytes).await.map_err(NodeError::Publish)
    }
}

/// A declared raw-bytes publisher for pre-built [`ZBytes`] payloads (e.g. SHM buffers).
///
/// Created via [`NodeContext::publisher_raw`](crate::NodeContext::publisher_raw).
///
/// When `local = true`, the publisher targets a machine-local topic
/// (`local/{machine_id}/suffix`) and uses `CongestionControl::Block` — required for
/// SHM so the publisher waits for the subscriber to read the SHM buffer instead of
/// silently dropping frames.
///
/// An optional encoding can be set so subscribers can auto-decode the payload
/// (e.g. `APPLICATION_PROTOBUF;TypeName` for proto-serialized SHM buffers).
pub struct RawPublisher {
    publisher: zenoh::pubsub::Publisher<'static>,
}

impl RawPublisher {
    pub(crate) async fn new(
        session: &Arc<zenoh::Session>,
        key_expr: &str,
        local: bool,
    ) -> Result<Self> {
        Self::with_encoding(session, key_expr, local, None).await
    }

    pub(crate) async fn with_encoding(
        session: &Arc<zenoh::Session>,
        key_expr: &str,
        local: bool,
        encoding: Option<Encoding>,
    ) -> Result<Self> {
        let mut builder = session.declare_publisher(key_expr.to_string());
        if local {
            builder = builder.congestion_control(CongestionControl::Block);
        }
        if let Some(enc) = encoding {
            builder = builder.encoding(enc);
        }
        let publisher = builder.await.map_err(|e| NodeError::PublisherDeclare {
            topic: key_expr.to_string(),
            source: e,
        })?;

        log::debug!("RawPublisher declared on '{}' (local={})", key_expr, local);
        Ok(Self { publisher })
    }

    /// Publish a raw [`ZBytes`] payload.
    pub async fn put(&self, payload: zenoh::bytes::ZBytes) -> Result<()> {
        self.publisher
            .put(payload)
            .await
            .map_err(NodeError::Publish)
    }
}

/// A declared CBOR publisher that sets `Encoding::APPLICATION_CBOR` automatically.
///
/// Created via [`NodeContext::publisher_cbor`](crate::NodeContext::publisher_cbor).
/// Serializes values via `ciborium` into a heap `Vec<u8>` before publishing —
/// suitable for small structured messages (telemetry, config, events).
///
/// For large, hot-path payloads (camera frames, sensor buffers) use
/// [`CborPublisherShm`] instead, which encodes directly into a pre-allocated
/// shared-memory slot without a heap allocation per message.
pub struct CborPublisher {
    publisher: zenoh::pubsub::Publisher<'static>,
}

impl CborPublisher {
    pub(crate) async fn new(session: &Arc<zenoh::Session>, key_expr: &str) -> Result<Self> {
        let publisher = session
            .declare_publisher(key_expr.to_string())
            .encoding(Encoding::APPLICATION_CBOR)
            .await
            .map_err(|e| NodeError::PublisherDeclare {
                topic: key_expr.to_string(),
                source: e,
            })?;

        log::debug!("CborPublisher declared on '{}'", key_expr);
        Ok(Self { publisher })
    }

    /// Serialize `value` as CBOR bytes and publish it.
    pub async fn put<S: serde::Serialize>(&self, value: &S) -> Result<()> {
        let mut bytes = Vec::new();
        ciborium::into_writer(value, &mut bytes)
            .map_err(|e| NodeError::CborEncode(e.to_string()))?;
        self.publisher.put(bytes).await.map_err(NodeError::Publish)
    }
}

/// A declared CBOR publisher backed by a pre-allocated POSIX shared-memory pool.
///
/// Created via [`NodeContext::publisher_cbor_shm`](crate::NodeContext::publisher_cbor_shm).
///
/// On every [`put`](CborPublisherShm::put), the publisher:
/// 1. Allocates a slot from the SHM pool (blocks under backpressure via
///    `BlockOn<GarbageCollect>` — the publisher waits for the subscriber to
///    consume a slot instead of silently dropping the message).
/// 2. Serializes the value with `ciborium` **directly into the mmap'd page**,
///    no heap `Vec`, no intermediate copy.
/// 3. Trims the SHM buffer to the exact CBOR length.
/// 4. Ships the SHM handle as a `ZBytes` payload — the subscriber maps the
///    same physical memory on the same machine.
///
/// The topic is always machine-local (`bubbaloop/local/{machine_id}/{suffix}`)
/// because Zenoh shared-memory transport cannot cross machines. Congestion
/// control is set to `Block` to pair correctly with `BlockOn<GarbageCollect>`.
///
/// **Picking `slot_count` and `slot_size`:**
/// - `slot_size` must be >= the largest single CBOR-encoded message your node
///   will publish. Undersize and `put()` returns an error; oversize and you
///   waste resident memory.
/// - `slot_count` controls how many in-flight messages can overlap before the
///   publisher blocks. 4 slots is a reasonable default for single-consumer
///   topics; raise it if you have slow subscribers that need more headroom.
pub struct CborPublisherShm {
    publisher: zenoh::pubsub::Publisher<'static>,
    shm_provider: ShmProvider<PosixShmProviderBackend>,
    slot_size: usize,
}

impl CborPublisherShm {
    pub(crate) async fn new(
        session: &Arc<zenoh::Session>,
        key_expr: &str,
        slot_count: usize,
        slot_size: usize,
    ) -> Result<Self> {
        let pool_size = slot_count
            .checked_mul(slot_size)
            .ok_or_else(|| NodeError::Shm("slot_count * slot_size overflow".to_string()))?;
        let backend = PosixShmProviderBackend::builder(pool_size)
            .wait()
            .map_err(|e| NodeError::Shm(format!("backend: {e:?}")))?;
        let shm_provider = ShmProviderBuilder::backend(backend).wait();

        let publisher = session
            .declare_publisher(key_expr.to_string())
            .encoding(Encoding::APPLICATION_CBOR)
            .congestion_control(CongestionControl::Block)
            .await
            .map_err(|e| NodeError::PublisherDeclare {
                topic: key_expr.to_string(),
                source: e,
            })?;

        log::debug!(
            "CborPublisherShm declared on '{}' (slots={}, slot_size={} B, pool={} B)",
            key_expr,
            slot_count,
            slot_size,
            pool_size
        );
        Ok(Self {
            publisher,
            shm_provider,
            slot_size,
        })
    }

    /// Size of each SHM slot in bytes. Messages larger than this fail to publish.
    pub fn slot_size(&self) -> usize {
        self.slot_size
    }

    /// Serialize `value` directly into a shared-memory slot and publish it.
    ///
    /// Blocks on allocation pressure — if all slots are in use, waits for a
    /// consumer to free one instead of dropping the message.
    pub async fn put<S: serde::Serialize>(&self, value: &S) -> Result<()> {
        let mut sbuf = self
            .shm_provider
            .alloc(self.slot_size)
            .with_policy::<BlockOn<GarbageCollect>>()
            .await
            .map_err(|e| NodeError::ShmAlloc(format!("{e:?}")))?;

        let written = {
            let mut cursor = Cursor::new(&mut sbuf[..]);
            ciborium::into_writer(value, &mut cursor)
                .map_err(|e| NodeError::CborEncode(e.to_string()))?;
            cursor.position() as usize
        };

        let new_len = NonZeroUsize::new(written)
            .ok_or_else(|| NodeError::CborEncode("CBOR encoded zero bytes".to_string()))?;
        sbuf.try_resize(new_len)
            .ok_or_else(|| NodeError::Shm("try_resize failed after encode".to_string()))?;

        self.publisher
            .put(ZBytes::from(sbuf))
            .await
            .map_err(NodeError::Publish)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proto_encoding_string() {
        let encoding =
            Encoding::APPLICATION_PROTOBUF.with_schema("bubbaloop.camera.v1.CompressedImage");
        assert_eq!(
            encoding.to_string(),
            "application/protobuf;bubbaloop.camera.v1.CompressedImage"
        );
    }

    #[test]
    fn json_encoding_string() {
        let encoding = Encoding::APPLICATION_JSON;
        assert_eq!(encoding.to_string(), "application/json");
    }

    #[test]
    fn json_serialization() {
        let value = serde_json::json!({"temperature": 22.5, "unit": "celsius"});
        let bytes = serde_json::to_vec(&value).unwrap();
        let back: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(back["temperature"], 22.5);
    }

    #[test]
    fn cbor_encoding_string() {
        assert_eq!(Encoding::APPLICATION_CBOR.to_string(), "application/cbor");
    }

    #[test]
    fn cbor_roundtrip_via_ciborium() {
        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
        struct Sample {
            n: u32,
            label: String,
        }
        let value = Sample { n: 42, label: "hi".to_string() };
        let mut bytes = Vec::new();
        ciborium::into_writer(&value, &mut bytes).unwrap();
        let back: Sample = ciborium::from_reader(&bytes[..]).unwrap();
        assert_eq!(back, value);
    }
}
