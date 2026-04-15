use std::collections::BTreeMap;
use std::io::Cursor;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use zenoh::bytes::{Encoding, ZBytes};

use crate::envelope::{now_ns, EnvelopeRef, Header};
use crate::manifest::Liveness;
use zenoh::qos::CongestionControl;
use zenoh::shm::{
    BlockOn, GarbageCollect, OwnedShmBuf, PosixShmProviderBackend, ShmProvider, ShmProviderBuilder,
};
use zenoh::Wait;

use crate::error::{NodeError, Result};

/// Shared manifest-liveness hook carried by every publisher. On first
/// successful `put`, flips `ever_fired=true`; on drop, flips
/// `still_live=false`. `None` when the publisher was declared with a key
/// outside this node's `machine_id` prefix (never happens in practice).
struct ManifestHook {
    map: Arc<Mutex<BTreeMap<String, Liveness>>>,
    suffix: Option<String>,
    ever_fired: AtomicBool,
}

impl ManifestHook {
    fn new(map: Arc<Mutex<BTreeMap<String, Liveness>>>, suffix: Option<String>) -> Self {
        Self {
            map,
            suffix,
            ever_fired: AtomicBool::new(false),
        }
    }

    fn mark_fired(&self) {
        if self.ever_fired.swap(true, Ordering::Relaxed) {
            return;
        }
        if let Some(sfx) = self.suffix.as_deref() {
            let mut guard = self.map.lock().expect("liveness mutex poisoned");
            if let Some(l) = guard.get_mut(sfx) {
                l.ever_fired = true;
            }
        }
    }
}

impl Drop for ManifestHook {
    fn drop(&mut self) {
        if let Some(sfx) = self.suffix.as_deref() {
            let mut guard = self.map.lock().expect("liveness mutex poisoned");
            if let Some(l) = guard.get_mut(sfx) {
                l.still_live = false;
            }
        }
    }
}

/// A declared JSON publisher that sets `Encoding::APPLICATION_JSON` automatically.
///
/// Wraps every payload in the SDK's `{header, body}` provenance envelope
/// (identical shape to [`CborPublisher`]) so JSON edges carry the same
/// lineage information as CBOR edges.
pub struct JsonPublisher {
    publisher: zenoh::pubsub::Publisher<'static>,
    source_instance: String,
    schema_uri: String,
    seq: AtomicU64,
    hook: ManifestHook,
}

#[derive(serde::Serialize)]
struct JsonEnvelope<'a, S: serde::Serialize + ?Sized> {
    header: Header,
    body: &'a S,
}

impl JsonPublisher {
    pub(crate) async fn new(
        session: &Arc<zenoh::Session>,
        key_expr: &str,
        source_instance: String,
        schema_uri: String,
        outputs: Arc<Mutex<BTreeMap<String, Liveness>>>,
        suffix: Option<String>,
    ) -> Result<Self> {
        let publisher = session
            .declare_publisher(key_expr.to_string())
            .encoding(Encoding::APPLICATION_JSON)
            .await
            .map_err(|e| NodeError::PublisherDeclare {
                topic: key_expr.to_string(),
                source: e,
            })?;

        log::debug!(
            "JsonPublisher declared on '{}' (source_instance='{}', schema_uri='{}')",
            key_expr,
            source_instance,
            schema_uri
        );
        Ok(Self {
            publisher,
            source_instance,
            schema_uri,
            seq: AtomicU64::new(0),
            hook: ManifestHook::new(outputs, suffix),
        })
    }

    fn next_header(&self) -> Header {
        Header {
            schema_uri: self.schema_uri.clone(),
            source_instance: self.source_instance.clone(),
            monotonic_seq: self.seq.fetch_add(1, Ordering::Relaxed),
            ts_ns: now_ns(),
        }
    }

    /// Wrap `value` in the provenance envelope, serialize as JSON, and publish.
    pub async fn put<S: serde::Serialize + ?Sized>(&self, value: &S) -> Result<()> {
        let env = JsonEnvelope {
            header: self.next_header(),
            body: value,
        };
        let bytes = serde_json::to_vec(&env)?;
        let res = self.publisher.put(bytes).await.map_err(NodeError::Publish);
        if res.is_ok() {
            self.hook.mark_fired();
        }
        res
    }
}

/// A declared raw-bytes publisher for pre-built [`ZBytes`] payloads (e.g. SHM buffers).
pub struct RawPublisher {
    publisher: zenoh::pubsub::Publisher<'static>,
    hook: ManifestHook,
}

impl RawPublisher {
    pub(crate) async fn new(
        session: &Arc<zenoh::Session>,
        key_expr: &str,
        local: bool,
        outputs: Arc<Mutex<BTreeMap<String, Liveness>>>,
        suffix: Option<String>,
    ) -> Result<Self> {
        Self::with_encoding(session, key_expr, local, None, outputs, suffix).await
    }

    pub(crate) async fn with_encoding(
        session: &Arc<zenoh::Session>,
        key_expr: &str,
        local: bool,
        encoding: Option<Encoding>,
        outputs: Arc<Mutex<BTreeMap<String, Liveness>>>,
        suffix: Option<String>,
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
        Ok(Self {
            publisher,
            hook: ManifestHook::new(outputs, suffix),
        })
    }

    /// Publish a raw [`ZBytes`] payload.
    pub async fn put(&self, payload: zenoh::bytes::ZBytes) -> Result<()> {
        let res = self
            .publisher
            .put(payload)
            .await
            .map_err(NodeError::Publish);
        if res.is_ok() {
            self.hook.mark_fired();
        }
        res
    }
}

/// A declared CBOR publisher that sets `Encoding::APPLICATION_CBOR` automatically.
pub struct CborPublisher {
    publisher: zenoh::pubsub::Publisher<'static>,
    source_instance: String,
    schema_uri: String,
    seq: AtomicU64,
    hook: ManifestHook,
}

impl CborPublisher {
    pub(crate) async fn new(
        session: &Arc<zenoh::Session>,
        key_expr: &str,
        source_instance: String,
        schema_uri: String,
        outputs: Arc<Mutex<BTreeMap<String, Liveness>>>,
        suffix: Option<String>,
    ) -> Result<Self> {
        let publisher = session
            .declare_publisher(key_expr.to_string())
            .encoding(Encoding::APPLICATION_CBOR)
            .await
            .map_err(|e| NodeError::PublisherDeclare {
                topic: key_expr.to_string(),
                source: e,
            })?;

        log::debug!(
            "CborPublisher declared on '{}' (source_instance='{}', schema_uri='{}')",
            key_expr,
            source_instance,
            schema_uri
        );
        Ok(Self {
            publisher,
            source_instance,
            schema_uri,
            seq: AtomicU64::new(0),
            hook: ManifestHook::new(outputs, suffix),
        })
    }

    fn next_header(&self) -> Header {
        Header {
            schema_uri: self.schema_uri.clone(),
            source_instance: self.source_instance.clone(),
            monotonic_seq: self.seq.fetch_add(1, Ordering::Relaxed),
            ts_ns: now_ns(),
        }
    }

    /// Wrap `value` in the provenance envelope, serialize as CBOR, and publish.
    pub async fn put<S: serde::Serialize + ?Sized>(&self, value: &S) -> Result<()> {
        let envelope = EnvelopeRef {
            header: self.next_header(),
            body: value,
        };
        let mut bytes = Vec::new();
        ciborium::into_writer(&envelope, &mut bytes)
            .map_err(|e| NodeError::CborEncode(e.to_string()))?;
        let res = self.publisher.put(bytes).await.map_err(NodeError::Publish);
        if res.is_ok() {
            self.hook.mark_fired();
        }
        res
    }
}

/// A declared CBOR publisher backed by a pre-allocated POSIX shared-memory pool.
pub struct CborPublisherShm {
    publisher: zenoh::pubsub::Publisher<'static>,
    shm_provider: ShmProvider<PosixShmProviderBackend>,
    slot_size: usize,
    source_instance: String,
    schema_uri: String,
    seq: AtomicU64,
    hook: ManifestHook,
}

impl CborPublisherShm {
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn new(
        session: &Arc<zenoh::Session>,
        key_expr: &str,
        slot_count: usize,
        slot_size: usize,
        source_instance: String,
        schema_uri: String,
        outputs: Arc<Mutex<BTreeMap<String, Liveness>>>,
        suffix: Option<String>,
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
            source_instance,
            schema_uri,
            seq: AtomicU64::new(0),
            hook: ManifestHook::new(outputs, suffix),
        })
    }

    pub fn slot_size(&self) -> usize {
        self.slot_size
    }

    fn next_header(&self) -> Header {
        Header {
            schema_uri: self.schema_uri.clone(),
            source_instance: self.source_instance.clone(),
            monotonic_seq: self.seq.fetch_add(1, Ordering::Relaxed),
            ts_ns: now_ns(),
        }
    }

    pub async fn put<S: serde::Serialize + ?Sized>(&self, value: &S) -> Result<()> {
        let mut sbuf = self
            .shm_provider
            .alloc(self.slot_size)
            .with_policy::<BlockOn<GarbageCollect>>()
            .await
            .map_err(|e| NodeError::ShmAlloc(format!("{e:?}")))?;

        let envelope = EnvelopeRef {
            header: self.next_header(),
            body: value,
        };

        let written = {
            let mut cursor = Cursor::new(&mut sbuf[..]);
            ciborium::into_writer(&envelope, &mut cursor)
                .map_err(|e| NodeError::CborEncode(e.to_string()))?;
            cursor.position() as usize
        };

        let new_len = NonZeroUsize::new(written)
            .ok_or_else(|| NodeError::CborEncode("CBOR encoded zero bytes".to_string()))?;
        sbuf.try_resize(new_len)
            .ok_or_else(|| NodeError::Shm("try_resize failed after encode".to_string()))?;

        let res = self
            .publisher
            .put(ZBytes::from(sbuf))
            .await
            .map_err(NodeError::Publish);
        if res.is_ok() {
            self.hook.mark_fired();
        }
        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_encoding_string() {
        let encoding = Encoding::APPLICATION_JSON;
        assert_eq!(encoding.to_string(), "application/json");
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
        let value = Sample {
            n: 42,
            label: "hi".to_string(),
        };
        let mut bytes = Vec::new();
        ciborium::into_writer(&value, &mut bytes).unwrap();
        let back: Sample = ciborium::from_reader(&bytes[..]).unwrap();
        assert_eq!(back, value);
    }

    #[test]
    fn json_envelope_shape_roundtrip() {
        // Build a JsonEnvelope manually and round-trip via serde_json.
        let env = JsonEnvelope {
            header: Header {
                schema_uri: "bubbaloop://probe/v1".into(),
                source_instance: "probe".into(),
                monotonic_seq: 3,
                ts_ns: 7,
            },
            body: &serde_json::json!({"temp": 22.5}),
        };
        let wire = serde_json::to_vec(&env).unwrap();
        let back: serde_json::Value = serde_json::from_slice(&wire).unwrap();
        assert_eq!(back["header"]["schema_uri"], "bubbaloop://probe/v1");
        assert_eq!(back["header"]["monotonic_seq"], 3);
        assert_eq!(back["body"]["temp"], 22.5);
    }

    #[test]
    fn manifest_hook_marks_still_live_false_on_drop() {
        let map = Arc::new(Mutex::new(BTreeMap::new()));
        map.lock().unwrap().insert("t".into(), Liveness::new(0));
        let hook = ManifestHook::new(map.clone(), Some("t".into()));
        drop(hook);
        let guard = map.lock().unwrap();
        assert!(!guard.get("t").unwrap().still_live);
    }

    #[test]
    fn manifest_hook_marks_ever_fired_once() {
        let map = Arc::new(Mutex::new(BTreeMap::new()));
        map.lock().unwrap().insert("t".into(), Liveness::new(0));
        let hook = ManifestHook::new(map.clone(), Some("t".into()));
        hook.mark_fired();
        hook.mark_fired();
        let guard = map.lock().unwrap();
        assert!(guard.get("t").unwrap().ever_fired);
        // Drop hook to flip still_live.
        drop(hook);
    }
}
