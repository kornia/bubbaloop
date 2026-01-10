use ros_z::{MessageTypeInfo, TypeHash, WithTypeInfo};

/// Protobuf schemas for bubbaloop
pub mod schemas {
    pub mod camera {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/bubbaloop.camera.v1.rs"));
        }
    }
    pub mod header {
        pub mod v1 {
            include!(concat!(env!("OUT_DIR"), "/bubbaloop.header.v1.rs"));
        }
    }

    // Re-export commonly used types
    pub use camera::v1::{CompressedImage, RawImage};
    pub use header::v1::Header;
}

// Re-export commonly used types at crate root
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
