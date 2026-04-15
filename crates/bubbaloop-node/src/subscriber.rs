use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use zenoh::handlers::FifoChannel;
use zenoh::{pubsub::Subscriber, sample::Sample};

use crate::envelope::{Envelope, Header};
use crate::error::{NodeError, Result};
use crate::manifest::Liveness;

/// Shared manifest-liveness hook for subscribers. On first delivered sample,
/// flips `ever_fired=true`; on drop, flips `still_live=false`.
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

/// Subscriber for raw byte payloads, counterpart to [`RawPublisher`](crate::RawPublisher).
pub struct RawSubscriber {
    inner: Subscriber<zenoh::handlers::FifoChannelHandler<Sample>>,
    hook: ManifestHook,
}

impl RawSubscriber {
    pub(crate) async fn new(
        session: &Arc<zenoh::Session>,
        key_expr: &str,
        inputs: Arc<Mutex<BTreeMap<String, Liveness>>>,
        suffix: Option<String>,
    ) -> Result<Self> {
        let subscriber = session
            .declare_subscriber(key_expr.to_string())
            .with(FifoChannel::new(4))
            .await
            .map_err(|e| NodeError::SubscriberDeclare {
                topic: key_expr.to_string(),
                source: e,
            })?;

        log::debug!("RawSubscriber declared on '{}'", key_expr);
        Ok(Self {
            inner: subscriber,
            hook: ManifestHook::new(inputs, suffix),
        })
    }

    /// Receive the next payload as [`ZBytes`](zenoh::bytes::ZBytes), or `None` if closed.
    pub async fn recv(&self) -> Option<zenoh::bytes::ZBytes> {
        let payload = self
            .inner
            .handler()
            .recv_async()
            .await
            .ok()
            .map(|s| s.payload().clone());
        if payload.is_some() {
            self.hook.mark_fired();
        }
        payload
    }

    /// Try to receive a payload without blocking.
    pub fn try_recv(&self) -> Option<zenoh::bytes::ZBytes> {
        let payload = self
            .inner
            .handler()
            .try_recv()
            .ok()
            .flatten()
            .map(|s| s.payload().clone());
        if payload.is_some() {
            self.hook.mark_fired();
        }
        payload
    }
}

/// Typed CBOR subscriber that transparently unwraps the SDK's
/// `{header, body}` provenance envelope.
pub struct CborSubscriber<T> {
    inner: Subscriber<zenoh::handlers::FifoChannelHandler<Sample>>,
    hook: ManifestHook,
    _phantom: PhantomData<T>,
}

impl<T: serde::de::DeserializeOwned> CborSubscriber<T> {
    pub(crate) async fn new(
        session: &Arc<zenoh::Session>,
        key_expr: &str,
        inputs: Arc<Mutex<BTreeMap<String, Liveness>>>,
        suffix: Option<String>,
    ) -> Result<Self> {
        let subscriber = session
            .declare_subscriber(key_expr.to_string())
            .with(FifoChannel::new(256))
            .await
            .map_err(|e| NodeError::SubscriberDeclare {
                topic: key_expr.to_string(),
                source: e,
            })?;

        log::debug!("CborSubscriber declared on '{}'", key_expr);
        Ok(Self {
            inner: subscriber,
            hook: ManifestHook::new(inputs, suffix),
            _phantom: PhantomData,
        })
    }

    /// Block until the next message and return the decoded envelope.
    pub async fn recv(&self) -> Option<Result<Envelope<T>>> {
        let sample = self.inner.handler().recv_async().await.ok()?;
        self.hook.mark_fired();
        Some(decode_envelope_bytes::<T>(&sample.payload().to_bytes()))
    }
}

/// Decode CBOR bytes into `Envelope<T>`.
pub fn decode_envelope_bytes<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<Envelope<T>> {
    if let Ok(env) = ciborium::from_reader::<Envelope<T>, _>(bytes) {
        return Ok(env);
    }
    let body: T = ciborium::from_reader(bytes)
        .map_err(|e| NodeError::CborEncode(format!("decode body: {e}")))?;
    Ok(Envelope {
        header: Header {
            schema_uri: String::new(),
            source_instance: String::new(),
            monotonic_seq: 0,
            ts_ns: 0,
        },
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::EnvelopeRef;

    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
    struct Sample {
        n: u32,
        label: String,
    }

    #[test]
    fn envelope_roundtrip_publisher_to_subscriber_decode() {
        let body = Sample {
            n: 42,
            label: "hi".into(),
        };
        let env = EnvelopeRef {
            header: Header {
                schema_uri: "bubbaloop://probe/v1".into(),
                source_instance: "probe".into(),
                monotonic_seq: 5,
                ts_ns: 1234,
            },
            body: &body,
        };
        let mut buf = Vec::new();
        ciborium::into_writer(&env, &mut buf).unwrap();

        let decoded: Envelope<Sample> = decode_envelope_bytes(&buf).unwrap();
        assert_eq!(decoded.body, body);
        assert_eq!(decoded.header.schema_uri, "bubbaloop://probe/v1");
        assert_eq!(decoded.header.source_instance, "probe");
        assert_eq!(decoded.header.monotonic_seq, 5);
        assert_eq!(decoded.header.ts_ns, 1234);
    }

    #[test]
    fn decode_falls_back_to_bare_body_for_non_enveloped_payload() {
        let body = Sample {
            n: 7,
            label: "raw".into(),
        };
        let mut buf = Vec::new();
        ciborium::into_writer(&body, &mut buf).unwrap();

        let decoded: Envelope<Sample> = decode_envelope_bytes(&buf).unwrap();
        assert_eq!(decoded.body, body);
        assert_eq!(decoded.header.source_instance, "");
        assert_eq!(decoded.header.monotonic_seq, 0);
    }

    #[test]
    fn subscriber_manifest_hook_marks_still_live_false_on_drop() {
        let map = Arc::new(Mutex::new(BTreeMap::new()));
        map.lock().unwrap().insert("in".into(), Liveness::new(0));
        let hook = ManifestHook::new(map.clone(), Some("in".into()));
        drop(hook);
        assert!(!map.lock().unwrap().get("in").unwrap().still_live);
    }
}
