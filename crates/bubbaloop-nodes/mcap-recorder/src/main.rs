//! MCAP recorder node binary

use bubbaloop::prelude::*;
use mcap_recorder_node::McapRecorderNode;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    run_node::<McapRecorderNode>().await
}
