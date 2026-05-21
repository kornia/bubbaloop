/// Typed errors for the bubbaloop-node SDK.
///
/// All SDK functions return `Result<T, NodeError>` so callers can match on
/// specific failure modes. The `Node::init` and `Node::run` trait methods
/// keep `anyhow::Result` because node authors return arbitrary application errors;
/// `NodeError` converts to `anyhow::Error` automatically via the `?` operator.
#[derive(thiserror::Error, Debug)]
pub enum NodeError {
    #[error("failed to declare publisher on '{topic}': {source}")]
    PublisherDeclare {
        topic: String,
        #[source]
        source: zenoh::Error,
    },

    #[error("publish failed: {0}")]
    Publish(#[source] zenoh::Error),

    #[error("failed to declare subscriber on '{topic}': {source}")]
    SubscriberDeclare {
        topic: String,
        #[source]
        source: zenoh::Error,
    },

    #[error("JSON serialization failed: {0}")]
    Json(#[from] serde_json::Error),

    #[error("failed to read config '{path}': {source}")]
    ConfigRead {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse config '{path}': {source}")]
    ConfigParse {
        path: String,
        #[source]
        source: serde_yaml::Error,
    },

    #[error("failed to configure Zenoh '{key}': {source}")]
    ZenohConfig {
        key: &'static str,
        #[source]
        source: zenoh::Error,
    },

    #[error("failed to open Zenoh session: {0}")]
    ZenohSession(#[source] zenoh::Error),

    #[error("get_sample timed out waiting for a message on '{topic}'")]
    GetSampleTimeout { topic: String },

    #[error("CBOR encode failed: {0}")]
    CborEncode(String),

    #[error("SHM pool setup failed: {0}")]
    Shm(String),

    #[error("SHM alloc failed: {0}")]
    ShmAlloc(String),

    #[error("failed to create health publisher: {0}")]
    HealthPublisher(#[source] zenoh::Error),

    #[error("failed to set up signal handler: {0}")]
    Signal(#[from] ctrlc::Error),
}

/// Convenience alias used throughout the SDK internals.
pub type Result<T> = std::result::Result<T, NodeError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_error_converts_via_from() {
        // ?-operator usage relies on the `From<serde_json::Error>` impl.
        let bad: serde_json::Result<serde_json::Value> = serde_json::from_str("{not json");
        let err: NodeError = bad.unwrap_err().into();
        assert!(matches!(err, NodeError::Json(_)));
    }

    #[test]
    fn get_sample_timeout_display_contains_topic() {
        let err = NodeError::GetSampleTimeout {
            topic: "bubbaloop/global/host/node/data".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("bubbaloop/global/host/node/data"));
        assert!(msg.contains("timed out"));
    }

    #[test]
    fn config_read_display_contains_path() {
        let err = NodeError::ConfigRead {
            path: "/tmp/missing.yaml".to_string(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "no such file"),
        };
        let msg = err.to_string();
        assert!(msg.contains("/tmp/missing.yaml"));
        assert!(msg.contains("no such file"));
    }

    #[test]
    fn cbor_encode_display_includes_inner_message() {
        let err = NodeError::CborEncode("buffer overflow".into());
        assert!(err.to_string().contains("buffer overflow"));
    }

    #[test]
    fn result_alias_compiles_with_error_variant() {
        fn returns_err() -> Result<u32> {
            Err(NodeError::CborEncode("test".into()))
        }
        assert!(returns_err().is_err());
    }
}
