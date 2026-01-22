//! Open-Meteo weather node binary

use bubbaloop::prelude::*;
use openmeteo_node::OpenMeteoNode;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    run_node::<OpenMeteoNode>().await
}
