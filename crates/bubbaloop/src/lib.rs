use ros_z::{MessageTypeInfo, TypeHash, WithTypeInfo};

/// Protobuf schemas for bubbaloop
pub mod schemas {
    include!(concat!(env!("OUT_DIR"), "/bubbaloop.camera.v1.rs"));
}

// Re-export commonly used types
pub use schemas::{CompressedImage, Header, RawImage};

impl MessageTypeInfo for schemas::CompressedImage {
    fn type_name() -> &'static str {
        "bubbaloop.camera.v1.CompressedImage"
    }

    fn type_hash() -> TypeHash {
        TypeHash::zero()
    }
}

impl WithTypeInfo for schemas::CompressedImage {}

impl MessageTypeInfo for schemas::RawImage {
    fn type_name() -> &'static str {
        "bubbaloop.camera.v1.RawImage"
    }

    fn type_hash() -> TypeHash {
        TypeHash::zero()
    }
}

impl WithTypeInfo for schemas::RawImage {}
