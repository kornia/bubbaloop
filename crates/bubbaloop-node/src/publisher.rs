use std::sync::{Arc, Mutex};
use zenoh::bytes::Encoding;
use zenoh::Wait as _;

/// A declared protobuf publisher that sets `Encoding::APPLICATION_PROTOBUF` automatically.
///
/// Created via [`NodeContext::publisher_proto`](crate::NodeContext::publisher_proto).
/// The encoding is declared once at construction and reused for every [`put`](ProtoPublisher::put).
///
/// Also registers a Zenoh queryable at the same key expression so that `session.get(topic)`
/// returns the last published payload. This allows agents to pull the current value on demand
/// without subscribing to the continuous stream.
pub struct ProtoPublisher<T: prost::Message + Default> {
    publisher: zenoh::pubsub::Publisher<'static>,
    last_bytes: Arc<Mutex<Option<Vec<u8>>>>,
    _queryable: zenoh::query::Queryable<()>,
    _marker: std::marker::PhantomData<T>,
}

impl<T: prost::Message + Default + crate::MessageTypeName> ProtoPublisher<T> {
    /// Declare a new protobuf publisher on `key_expr`.
    ///
    /// Sets encoding to `application/protobuf;<type_name>` where `<type_name>` is the fully
    /// qualified protobuf type name provided by [`MessageTypeName::type_name`].
    ///
    /// Also declares a queryable at the same key so agents can `get()` the last value.
    pub(crate) async fn new(
        session: &Arc<zenoh::Session>,
        key_expr: &str,
    ) -> anyhow::Result<Self> {
        let encoding = Encoding::APPLICATION_PROTOBUF.with_schema(T::type_name());
        let publisher = session
            .declare_publisher(key_expr.to_string())
            .encoding(encoding)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to declare protobuf publisher on '{}': {}", key_expr, e))?;

        let last_bytes: Arc<Mutex<Option<Vec<u8>>>> = Arc::new(Mutex::new(None));
        let buf = last_bytes.clone();

        let queryable = session
            .declare_queryable(key_expr)
            .callback(move |query| {
                let guard = buf.lock().unwrap();
                if let Some(bytes) = guard.as_ref() {
                    let key = query.key_expr().clone();
                    if let Err(e) = query.reply(key, bytes.as_slice()).wait() {
                        log::warn!("ProtoPublisher queryable reply failed: {}", e);
                    }
                }
            })
            .await
            .map_err(|e| anyhow::anyhow!("Failed to declare publisher queryable on '{}': {}", key_expr, e))?;

        log::debug!("ProtoPublisher declared on '{}' (type={})", key_expr, T::type_name());
        Ok(Self {
            publisher,
            last_bytes,
            _queryable: queryable,
            _marker: std::marker::PhantomData,
        })
    }

    /// Encode `msg` as protobuf bytes, cache it, and publish it.
    pub async fn put(&self, msg: &T) -> anyhow::Result<()> {
        let bytes = msg.encode_to_vec();
        *self.last_bytes.lock().unwrap() = Some(bytes.clone());
        self.publisher
            .put(bytes)
            .await
            .map_err(|e| anyhow::anyhow!("ProtoPublisher put failed: {}", e))
    }
}

/// A declared JSON publisher that sets `Encoding::APPLICATION_JSON` automatically.
///
/// Created via [`NodeContext::publisher_json`](crate::NodeContext::publisher_json).
/// The encoding is declared once at construction and reused for every [`put`](JsonPublisher::put).
///
/// Also registers a Zenoh queryable at the same key expression so that `session.get(topic)`
/// returns the last published payload. This allows agents to pull the current value on demand
/// without subscribing to the continuous stream.
pub struct JsonPublisher {
    publisher: zenoh::pubsub::Publisher<'static>,
    last_bytes: Arc<Mutex<Option<Vec<u8>>>>,
    _queryable: zenoh::query::Queryable<()>,
}

impl JsonPublisher {
    /// Declare a new JSON publisher on `key_expr`.
    ///
    /// Also declares a queryable at the same key so agents can `get()` the last value.
    pub(crate) async fn new(
        session: &Arc<zenoh::Session>,
        key_expr: &str,
    ) -> anyhow::Result<Self> {
        let publisher = session
            .declare_publisher(key_expr.to_string())
            .encoding(Encoding::APPLICATION_JSON)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to declare JSON publisher on '{}': {}", key_expr, e))?;

        let last_bytes: Arc<Mutex<Option<Vec<u8>>>> = Arc::new(Mutex::new(None));
        let buf = last_bytes.clone();

        let queryable = session
            .declare_queryable(key_expr)
            .callback(move |query| {
                let guard = buf.lock().unwrap();
                if let Some(bytes) = guard.as_ref() {
                    let key = query.key_expr().clone();
                    if let Err(e) = query.reply(key, bytes.as_slice()).wait() {
                        log::warn!("JsonPublisher queryable reply failed: {}", e);
                    }
                }
            })
            .await
            .map_err(|e| anyhow::anyhow!("Failed to declare publisher queryable on '{}': {}", key_expr, e))?;

        log::debug!("JsonPublisher declared on '{}'", key_expr);
        Ok(Self { publisher, last_bytes, _queryable: queryable })
    }

    /// Serialize any `Serialize` value as JSON bytes, cache it, and publish it.
    pub async fn put<S: serde::Serialize>(&self, value: &S) -> anyhow::Result<()> {
        let bytes =
            serde_json::to_vec(value).map_err(|e| anyhow::anyhow!("JSON serialization failed: {}", e))?;
        *self.last_bytes.lock().unwrap() = Some(bytes.clone());
        self.publisher
            .put(bytes)
            .await
            .map_err(|e| anyhow::anyhow!("JsonPublisher put failed: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify encoding string format: APPLICATION_PROTOBUF + type name suffix.
    #[test]
    fn proto_encoding_string() {
        let encoding = Encoding::APPLICATION_PROTOBUF.with_schema("bubbaloop.camera.v1.CompressedImage");
        assert_eq!(encoding.to_string(), "application/protobuf;bubbaloop.camera.v1.CompressedImage");
    }

    /// Verify JSON encoding string.
    #[test]
    fn json_encoding_string() {
        let encoding = Encoding::APPLICATION_JSON;
        assert_eq!(encoding.to_string(), "application/json");
    }

    /// Verify that serde_json::to_vec produces valid UTF-8 JSON bytes.
    #[test]
    fn json_serialization() {
        let value = serde_json::json!({"temperature": 22.5, "unit": "celsius"});
        let bytes = serde_json::to_vec(&value).unwrap();
        let back: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(back["temperature"], 22.5);
    }
}
