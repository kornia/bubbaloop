//! Multi-camera RTSP streaming node binary

use bubbaloop::prelude::*;
use cameras_node::CamerasNode;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    run_node::<CamerasNode>().await
}
