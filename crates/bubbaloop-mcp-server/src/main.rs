//! Bubbaloop MCP Server - Entry Point
//!
//! Runs the MCP server over stdio for integration with Claude Desktop.

use anyhow::Result;
use argh::FromArgs;
use bubbaloop_mcp_server::BubbaloopServer;

/// Bubbaloop MCP Server - Expose Bubbaloop to AI assistants
#[derive(FromArgs)]
struct Args {
    /// zenoh endpoint to connect to (default: tcp/127.0.0.1:7447)
    #[argh(option, default = "String::from(\"tcp/127.0.0.1:7447\")")]
    zenoh_endpoint: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse CLI arguments
    let args: Args = argh::from_env();

    // Initialize logging to stderr (stdout is used for MCP protocol)
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .target(env_logger::Target::Stderr)
        .init();

    log::info!("Starting Bubbaloop MCP server");
    log::info!("Zenoh endpoint: {}", args.zenoh_endpoint);

    // Create the Bubbaloop server
    let server = BubbaloopServer::new(&args.zenoh_endpoint).await?;

    // Run over stdio
    server.run_stdio().await?;

    Ok(())
}
