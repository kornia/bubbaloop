use std::sync::Arc;
use zenoh::{pubsub::Subscriber, sample::Sample};
use zenoh::handlers::FifoChannel;

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
    pub(crate) async fn new(
        session: &Arc<zenoh::Session>,
        key_expr: &str,
    ) -> anyhow::Result<Self> {
        let subscriber = session
            .declare_subscriber(key_expr.to_string())
            .with(FifoChannel::new(256))
            .await
            .map_err(|e| {
                anyhow::anyhow!("Failed to declare typed subscriber on '{}': {}", key_expr, e)
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

/// An untyped (raw) subscriber that exposes [`Sample`] values directly.
///
/// Created via [`NodeContext::subscriber_raw`](crate::NodeContext::subscriber_raw).
/// Useful for dashboard-style dynamic decoding where the type is not known at compile time.
/// The caller reads `sample.encoding()` to decide how to decode the payload.
pub struct RawSubscriber {
    inner: Subscriber<zenoh::handlers::FifoChannelHandler<Sample>>,
}

impl RawSubscriber {
    pub(crate) async fn new(
        session: &Arc<zenoh::Session>,
        key_expr: &str,
    ) -> anyhow::Result<Self> {
        let subscriber = session
            .declare_subscriber(key_expr.to_string())
            .with(FifoChannel::new(256))
            .await
            .map_err(|e| {
                anyhow::anyhow!("Failed to declare raw subscriber on '{}': {}", key_expr, e)
            })?;

        log::debug!("RawSubscriber declared on '{}'", key_expr);
        Ok(Self { inner: subscriber })
    }

    /// Receive the next raw [`Sample`], or `None` if the subscriber was undeclared.
    pub async fn recv(&self) -> Option<Sample> {
        self.inner.handler().recv_async().await.ok()
    }

    /// Try to receive a raw [`Sample`] without blocking.
    pub fn try_recv(&self) -> Option<Sample> {
        self.inner.handler().try_recv().ok().flatten()
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
