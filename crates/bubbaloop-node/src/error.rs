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

    #[error("failed to declare schema queryable: {0}")]
    SchemaQueryable(#[source] zenoh::Error),

    #[error("get_sample timed out waiting for a message on '{topic}'")]
    GetSampleTimeout { topic: String },

    #[error("protobuf decode failed: {0}")]
    Decode(String),

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
