pub mod config;
pub mod foxglove_node;
pub mod h264_capture;
pub mod rtsp_camera_node;

use ros_z::{MessageTypeInfo, TypeHash, WithTypeInfo};

pub mod protos {
    include!(concat!(env!("OUT_DIR"), "/bubbaloop.camera.v1.rs"));
}

impl MessageTypeInfo for protos::CompressedImage {
    fn type_name() -> &'static str {
        "bubbaloop.camera.v1.CompressedImage"
    }

    fn type_hash() -> TypeHash {
        TypeHash::zero()
    }
}

impl WithTypeInfo for protos::CompressedImage {}

