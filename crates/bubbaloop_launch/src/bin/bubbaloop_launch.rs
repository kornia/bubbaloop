//! Bubbaloop Launch CLI
//!
//! Usage:
//!   bubbaloop_launch launch/default.launch.yaml
//!   bubbaloop_launch launch/default.launch.yaml -a camera_config:=configs/jetson.yaml
//!   bubbaloop_launch launch/default.launch.yaml --dry-run

use bubbaloop_launch::{Executor, ExecutorConfig, LaunchArgs, LaunchFile};
use std::collections::HashSet;
use tokio::sync::watch;

#[tokio::main]
async fn main() {
    let args: LaunchArgs = argh::from_env();

    // Initialize logging
    let log_level = match args.log_level.to_lowercase().as_str() {
        "error" => "error",
        "warn" => "warn",
        "info" => "info",
        "debug" => "debug",
        "trace" => "trace",
        _ => "info",
    };
    let env = env_logger::Env::default().default_filter_or(log_level);
    env_logger::init_from_env(env);

    // Load launch file
    log::info!("Loading launch file: {}", args.launch_file);
    let launch_file = match LaunchFile::from_file(&args.launch_file) {
        Ok(lf) => lf,
        Err(e) => {
            log::error!("Failed to load launch file: {}", e);
            std::process::exit(1);
        }
    };

    // Validate only mode
    if args.validate {
        println!("Launch file '{}' is valid", args.launch_file);
        println!("  Version: {}", launch_file.version);
        println!("  Args: {}", launch_file.args.len());
        println!("  Nodes: {}", launch_file.nodes.len());
        println!("  Groups: {}", launch_file.groups().join(", "));
        return;
    }

    // Build executor config
    let project_root = std::env::current_dir().expect("Failed to get current directory");

    // Get arg overrides before moving other fields
    let arg_overrides = args.arg_overrides();

    let include_groups = args.groups.map(|g| g.into_iter().collect::<HashSet<_>>());
    let enable_nodes = args
        .enable
        .map(|n| n.into_iter().collect::<HashSet<_>>())
        .unwrap_or_default();
    let disable_nodes = args
        .disable
        .map(|n| n.into_iter().collect::<HashSet<_>>())
        .unwrap_or_default();

    let executor_config = ExecutorConfig {
        project_root,
        include_groups,
        enable_nodes,
        disable_nodes,
        ..Default::default()
    };

    // Create executor
    let mut executor = match Executor::new(launch_file, executor_config, arg_overrides) {
        Ok(e) => e,
        Err(e) => {
            log::error!("Failed to create executor: {}", e);
            std::process::exit(1);
        }
    };

    // Dry run mode
    if args.dry_run {
        match executor.plan() {
            Ok(plan) => {
                println!("{}", plan);
            }
            Err(e) => {
                log::error!("Failed to generate launch plan: {}", e);
                std::process::exit(1);
            }
        }
        return;
    }

    // Create shutdown channel
    let (shutdown_tx, shutdown_rx) = watch::channel(());

    // Set up Ctrl+C handler
    {
        let shutdown_tx = shutdown_tx.clone();
        ctrlc::set_handler(move || {
            log::info!("Received Ctrl+C, initiating shutdown...");
            let _ = shutdown_tx.send(());
        })
        .expect("Error setting Ctrl+C handler");
    }

    // Launch all nodes
    if let Err(e) = executor.launch(shutdown_rx.clone()).await {
        log::error!("Launch failed: {}", e);
        executor.shutdown().await;
        std::process::exit(1);
    }

    // Wait for shutdown signal or all processes to exit
    executor.wait(shutdown_rx).await;

    // Shutdown all processes
    executor.shutdown().await;

    log::info!("Bubbaloop launcher exiting");
}
