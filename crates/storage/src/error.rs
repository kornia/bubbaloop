//! Unified error type for storage operations.

/// Storage service errors.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("LanceDB error: {0}")]
    Lance(#[from] lancedb::Error),

    #[error("Arrow error: {0}")]
    Arrow(#[from] arrow_schema::ArrowError),

    /// Zenoh uses `Box<dyn Error>` which can't derive `#[from]`.
    #[error("Zenoh error: {0}")]
    Zenoh(String),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Session not found: {0}")]
    Session(String),

    #[error("Protobuf decode error: {0}")]
    Decode(#[from] prost::DecodeError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, StorageError>;
