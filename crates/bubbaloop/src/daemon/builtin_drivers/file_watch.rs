//! File watch driver — watch file/directory for changes and publish events to Zenoh.

use super::{spawn_health_loop, BuiltinDriver, DriverConfig, DriverError, Result};
use notify::{Event, RecursiveMode, Watcher};
use std::path::PathBuf;

pub struct FileWatchDriver;

#[async_trait::async_trait]
impl BuiltinDriver for FileWatchDriver {
    fn name(&self) -> &'static str {
        "file-watch"
    }

    async fn run(&self, config: DriverConfig) -> Result<()> {
        let path_str = config.require_str("path")?;
        let recursive = config.bool_or("recursive", false);

        let watch_path = PathBuf::from(&path_str);
        if !watch_path.exists() {
            return Err(DriverError::ConfigError(format!(
                "Watch path does not exist: {}",
                path_str
            )));
        }

        let data_topic = config.data_topic();
        let health_topic = config.health_topic();
        let session = config.session.clone();
        let mut shutdown_rx = config.shutdown_rx.clone();

        // Spawn health heartbeat
        let health_session = session.clone();
        let health_shutdown = config.shutdown_rx.clone();
        tokio::spawn(spawn_health_loop(
            health_session,
            health_topic,
            health_shutdown,
        ));

        let (tx, mut rx) = tokio::sync::mpsc::channel::<Event>(256);

        let mode = if recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };

        let mut watcher =
            notify::recommended_watcher(move |res: std::result::Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    let _ = tx.blocking_send(event);
                }
            })
            .map_err(|e| DriverError::StartFailed(format!("watcher init: {}", e)))?;

        watcher
            .watch(&watch_path, mode)
            .map_err(|e| DriverError::StartFailed(format!("watch '{}': {}", path_str, e)))?;

        log::info!(
            "[file-watch] skill='{}' watching '{}' recursive={}",
            config.skill_name,
            path_str,
            recursive
        );

        loop {
            tokio::select! {
                biased;
                _ = shutdown_rx.changed() => {
                    log::info!("[file-watch] '{}' shutting down", config.skill_name);
                    break;
                }
                event = rx.recv() => {
                    match event {
                        Some(evt) => {
                            let paths: Vec<String> = evt.paths.iter().map(|p| p.display().to_string()).collect();
                            let payload = serde_json::json!({
                                "kind": format!("{:?}", evt.kind),
                                "paths": paths,
                            });
                            if let Err(e) = session.put(&data_topic, payload.to_string()).await {
                                log::warn!("[file-watch] publish failed: {}", e);
                            }
                        }
                        None => break,
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn driver_name() {
        assert_eq!(FileWatchDriver.name(), "file-watch");
    }
}
