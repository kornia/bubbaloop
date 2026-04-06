use std::sync::Arc;
use zenoh::handlers::FifoChannel;
use zenoh::{pubsub::Subscriber, sample::Sample};

use crate::error::{NodeError, Result};

/// A typed protobuf subscriber that decodes incoming messages automatically.
///
/// Created via [`NodeContext::subscriber`](crate::NodeContext::subscriber).
/// Samples are decoded using [`prost::Message::decode`]. Decoding errors are
/// logged and the sample is dropped — the subscriber never panics.
pub struct TypedSubscriber<T: prost::Message + Default> {
    inner: Subscriber<zenoh::handlers::FifoChannelHandler<Sample>>,
    _marker: std::marker::PhantomData<T>,
}

impl<T: prost::Message + Default> TypedSubscriber<T> {
    pub(crate) async fn new(session: &Arc<zenoh::Session>, key_expr: &str) -> Result<Self> {
        let subscriber = session
            .declare_subscriber(key_expr.to_string())
            .with(FifoChannel::new(256))
            .await
            .map_err(|e| NodeError::SubscriberDeclare {
                topic: key_expr.to_string(),
                source: e,
            })?;

        log::debug!("TypedSubscriber declared on '{}'", key_expr);
        Ok(Self {
            inner: subscriber,
            _marker: std::marker::PhantomData,
        })
    }

    /// Receive the next decoded message, or `None` if the subscriber was undeclared.
    ///
    /// Decoding errors are logged and skipped; the method loops until a valid message
    /// arrives or the subscriber is closed.
    pub async fn recv(&self) -> Option<T> {
        loop {
            match self.inner.handler().recv_async().await {
                Ok(sample) => {
                    let payload = sample.payload().to_bytes();
                    match T::decode(payload.as_ref()) {
                        Ok(msg) => return Some(msg),
                        Err(e) => {
                            log::warn!("TypedSubscriber: failed to decode protobuf message: {}", e);
                            // skip and try next sample
                        }
                    }
                }
                Err(_) => return None, // channel closed
            }
        }
    }

    /// Try to receive a message without blocking, returns `None` if no message is available.
    pub fn try_recv(&self) -> Option<T> {
        match self.inner.handler().try_recv() {
            Ok(Some(sample)) => {
                let payload = sample.payload().to_bytes();
                match T::decode(payload.as_ref()) {
                    Ok(msg) => Some(msg),
                    Err(e) => {
                        log::warn!("TypedSubscriber: failed to decode protobuf message: {}", e);
                        None
                    }
                }
            }
            _ => None,
        }
    }
}

/// Subscriber for raw byte payloads, counterpart to [`RawPublisher`](crate::RawPublisher).
///
/// Created via [`NodeContext::subscriber_raw`](crate::NodeContext::subscriber_raw).
/// Receives payloads as [`ZBytes`](zenoh::bytes::ZBytes) with no decoding — ideal for
/// SHM frames or any binary data where the caller handles the decode itself.
///
/// Uses a small FIFO (4 slots) — older frames are dropped when the consumer is slow.
pub struct RawSubscriber {
    inner: Subscriber<zenoh::handlers::FifoChannelHandler<Sample>>,
}

impl RawSubscriber {
    pub(crate) async fn new(session: &Arc<zenoh::Session>, key_expr: &str) -> Result<Self> {
        let subscriber = session
            .declare_subscriber(key_expr.to_string())
            .with(FifoChannel::new(4))
            .await
            .map_err(|e| NodeError::SubscriberDeclare {
                topic: key_expr.to_string(),
                source: e,
            })?;

        log::debug!("RawSubscriber declared on '{}'", key_expr);
        Ok(Self { inner: subscriber })
    }

    /// Receive the next payload as [`ZBytes`](zenoh::bytes::ZBytes), or `None` if closed.
    pub async fn recv(&self) -> Option<zenoh::bytes::ZBytes> {
        self.inner
            .handler()
            .recv_async()
            .await
            .ok()
            .map(|s| s.payload().clone())
    }

    /// Try to receive a payload without blocking.
    pub fn try_recv(&self) -> Option<zenoh::bytes::ZBytes> {
        self.inner
            .handler()
            .try_recv()
            .ok()
            .flatten()
            .map(|s| s.payload().clone())
    }
}

#[cfg(test)]
mod tests {
    use prost::Message as _;

    /// Minimal protobuf message for testing: a single uint32 field.
    #[derive(Clone, PartialEq, prost::Message)]
    struct TestMsg {
        #[prost(uint32, tag = "1")]
        value: u32,
    }

    /// Verify round-trip: encode → decode produces the same message.
    #[test]
    fn proto_round_trip() {
        let original = TestMsg { value: 42 };
        let bytes = original.encode_to_vec();
        let decoded = TestMsg::decode(bytes.as_slice()).unwrap();
        assert_eq!(decoded.value, 42);
    }

    /// Verify that decoding garbage bytes returns an error (not a panic).
    #[test]
    fn proto_decode_garbage() {
        let garbage = b"\xFF\xFF\xFF\xFF not valid protobuf";
        let result = TestMsg::decode(garbage.as_ref());
        assert!(result.is_err());
    }
}
