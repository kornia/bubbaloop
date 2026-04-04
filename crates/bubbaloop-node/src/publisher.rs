use std::sync::Arc;
use zenoh::bytes::Encoding;

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
}
