/// Storage service error types.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("LanceDB error: {0}")]
    Lance(#[from] lancedb::Error),

    #[error("Arrow error: {0}")]
    Arrow(#[from] arrow_schema::ArrowError),

    #[error("Zenoh error: {0}")]
    Zenoh(String),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Session error: {0}")]
    Session(String),

    #[error("Protobuf decode error: {0}")]
    Decode(#[from] prost::DecodeError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, StorageError>;
