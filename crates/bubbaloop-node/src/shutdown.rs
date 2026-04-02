use tokio::sync::watch;

/// Set up a shutdown channel triggered by SIGINT/SIGTERM.
///
/// Returns the sender (for the signal handler) and a receiver (for the node).
pub fn setup_shutdown() -> anyhow::Result<(watch::Sender<()>, watch::Receiver<()>)> {
    let (tx, rx) = watch::channel(());
    let shutdown_tx = tx.clone();
    ctrlc::set_handler(move || {
        log::info!("Shutdown signal received");
        let _ = shutdown_tx.send(());
    })?;
    Ok((tx, rx))
}
