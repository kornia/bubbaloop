//! Multi-camera RTSP streaming node for Bubbaloop.
//!
//! This node captures H264 streams from RTSP cameras and publishes:
//! - Compressed H264 frames via protobuf
//! - Decoded raw frames via Zenoh shared memory (SHM)

mod config;
mod h264_capture;
mod h264_decode;
mod node;

pub use config::{CameraConfig, Config, ConfigError, DecoderBackend};
pub use h264_capture::{H264CaptureError, H264Frame, H264StreamCapture};
pub use h264_decode::{DecoderBackend as DecoderBackendInternal, H264DecodeError, RawFrame, VideoH264Decoder};
pub use node::{CamerasNode, RtspCameraNode};
