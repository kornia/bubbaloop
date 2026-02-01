//! Recording engine that bridges Zenoh subscriptions to LanceDB storage.
//!
//! Subscribes to configured topic patterns, extracts Header metadata from
//! each incoming message, buffers payloads, and flushes batches to LanceDB.
//!
//! # Recording loop
//!
//! ```text
//! ┌──────────────────────────────────────┐
//! │  topic /camera/** ──► subscriber ──┐ │
//! │  topic /weather/** ─► subscriber ──┼─► mpsc channel ──► buffer ──► LanceDB
//! │  topic /lidar/**  ──► subscriber ──┘ │       ▲                      ▲
//! │                                      │  flush timer            batch_size
//! └──────────────────────────────────────┘
//! ```

use std::sync::Arc;

use tokio::sync::{mpsc, watch};
use uuid::Uuid;

use crate::config::StorageConfig;
use crate::error::{Result, StorageError};
use crate::header::MessageMeta;
use crate::lancedb_client::{SessionRecord, StorageClient, StoredMessage};

/// Recording engine that manages Zenoh subscriptions and LanceDB persistence.
pub struct Recorder {
    client: StorageClient,
    config: StorageConfig,
    session_id: Option<String>,
    message_count: u64,
    buffer: Vec<StoredMessage>,
}

impl Recorder {
    pub fn new(client: StorageClient, config: StorageConfig) -> Self {
        Self {
            client,
            config,
            session_id: None,
            message_count: 0,
            buffer: Vec::new(),
        }
    }

    /// Run the recording loop until shutdown.
    ///
    /// 1. Creates a new session in LanceDB.
    /// 2. Subscribes to all configured topic patterns.
    /// 3. Receives messages, buffers, and flushes on `batch_size` or timer.
    /// 4. On shutdown: flushes remaining buffer, marks session completed.
    pub async fn run(
        &mut self,
        zenoh_session: Arc<zenoh::Session>,
        mut shutdown_rx: watch::Receiver<()>,
    ) -> Result<()> {
        self.client.ensure_tables().await?;

        let session_id = self.start_session().await?;
        log::info!("Started recording session: {session_id}");

        // Merge all subscriber streams into one channel
        let (tx, mut rx) = mpsc::channel::<(String, Vec<u8>)>(1000);
        let mut tasks = Vec::new();

        for pattern in &self.config.topics {
            let sub = zenoh_session
                .declare_subscriber(pattern)
                .await
                .map_err(|e| {
                    StorageError::Zenoh(format!("Subscribe failed for '{pattern}': {e}"))
                })?;
            log::info!("Subscribed to: {pattern}");

            let tx = tx.clone();
            tasks.push(tokio::spawn(async move {
                loop {
                    match sub.recv_async().await {
                        Ok(sample) => {
                            let topic = sample.key_expr().to_string();
                            let payload = sample.payload().to_bytes().to_vec();
                            if tx.send((topic, payload)).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            }));
        }
        drop(tx);

        let mut flush_timer = tokio::time::interval(std::time::Duration::from_secs(
            self.config.flush_interval_secs,
        ));
        flush_timer.tick().await; // Consume initial tick

        loop {
            tokio::select! {
                sample = rx.recv() => {
                    match sample {
                        Some((topic, payload)) => {
                            self.handle_message(&topic, payload);
                            if self.buffer.len() >= self.config.batch_size {
                                self.flush_buffer().await?;
                            }
                        }
                        None => {
                            log::warn!("All subscriber channels closed");
                            break;
                        }
                    }
                }
                _ = flush_timer.tick() => {
                    if !self.buffer.is_empty() {
                        self.flush_buffer().await?;
                    }
                }
                _ = shutdown_rx.changed() => {
                    log::info!("Shutdown signal received, stopping recording");
                    break;
                }
            }
        }

        self.stop_session().await?;
        for handle in tasks {
            handle.abort();
        }
        Ok(())
    }

    /// Process a single incoming message into the buffer.
    fn handle_message(&mut self, topic: &str, payload: Vec<u8>) {
        let message_type = self.config.message_type_for_topic(topic);
        let meta = MessageMeta::from_raw(&payload, topic, &message_type);
        let session_id = self.session_id.clone().unwrap_or_default();

        self.buffer.push(StoredMessage {
            meta,
            raw_data: payload,
            session_id,
        });
    }

    /// Flush the message buffer to LanceDB.
    async fn flush_buffer(&mut self) -> Result<usize> {
        if self.buffer.is_empty() {
            return Ok(0);
        }

        let count = self.buffer.len();
        match self.client.insert_messages(&self.buffer).await {
            Ok(inserted) => {
                self.message_count += inserted as u64;
                self.buffer.clear();
                log::debug!(
                    "Flushed {inserted} messages (total: {})",
                    self.message_count
                );
                Ok(inserted)
            }
            Err(e) => {
                log::error!("Failed to flush {count} messages: {e}");
                Err(e)
            }
        }
    }

    async fn start_session(&mut self) -> Result<String> {
        let session_id = Uuid::new_v4().to_string();

        let topics_str = format!(
            "[{}]",
            self.config
                .topics
                .iter()
                .map(|t| format!("\"{t}\""))
                .collect::<Vec<_>>()
                .join(", ")
        );

        self.client
            .create_session(&SessionRecord {
                session_id: session_id.clone(),
                session_name: format!("rec-{}", &session_id[..8]),
                start_time_ns: crate::now_nanos(),
                end_time_ns: 0,
                topics: topics_str,
                message_count: 0,
                machine_id: hostname(),
                status: "recording".into(),
            })
            .await?;

        self.session_id = Some(session_id.clone());
        self.message_count = 0;
        self.buffer.clear();
        Ok(session_id)
    }

    async fn stop_session(&mut self) -> Result<()> {
        self.flush_buffer().await?;

        if let Some(session_id) = self.session_id.take() {
            self.client
                .update_session(
                    &session_id,
                    crate::now_nanos(),
                    self.message_count,
                    "completed",
                )
                .await?;
            log::info!(
                "Session {session_id} completed: {} messages recorded",
                self.message_count
            );
        }

        self.message_count = 0;
        Ok(())
    }

    /// Access the underlying storage client for querying stored data.
    pub fn client(&self) -> &StorageClient {
        &self.client
    }

    /// Current recording session ID, if active.
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// Messages currently buffered (not yet flushed).
    pub fn buffer_len(&self) -> usize {
        self.buffer.len()
    }

    /// Total messages recorded in the current session.
    pub fn message_count(&self) -> u64 {
        self.message_count
    }
}

fn hostname() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("HOST"))
        .unwrap_or_else(|_| "unknown".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn test_config() -> StorageConfig {
        StorageConfig {
            storage_uri: String::new(),
            topics: vec!["/camera/**".into(), "/weather/**".into()],
            schema_hints: HashMap::from([
                (
                    "/camera/*/compressed".into(),
                    "bubbaloop.camera.v1.CompressedImage".into(),
                ),
                (
                    "/weather/current".into(),
                    "bubbaloop.weather.v1.CurrentWeather".into(),
                ),
            ]),
            batch_size: 3,
            flush_interval_secs: 1,
        }
    }

    async fn test_recorder() -> (Recorder, TempDir) {
        let dir = TempDir::new().unwrap();
        let client = StorageClient::connect(dir.path().to_str().unwrap())
            .await
            .unwrap();
        client.ensure_tables().await.unwrap();
        (Recorder::new(client, test_config()), dir)
    }

    #[tokio::test]
    async fn test_handle_message_buffers_with_metadata() {
        let (mut rec, _dir) = test_recorder().await;
        rec.session_id = Some("test-session".into());

        rec.handle_message("/camera/entrance/compressed", vec![1, 2, 3, 4]);

        assert_eq!(rec.buffer_len(), 1);
        assert_eq!(rec.buffer[0].meta.topic, "/camera/entrance/compressed");
        assert_eq!(
            rec.buffer[0].meta.message_type,
            "bubbaloop.camera.v1.CompressedImage"
        );
        assert_eq!(rec.buffer[0].raw_data, vec![1, 2, 3, 4]);
        assert_eq!(rec.buffer[0].session_id, "test-session");
    }

    #[tokio::test]
    async fn test_schema_hints_resolve_type() {
        let (mut rec, _dir) = test_recorder().await;
        rec.session_id = Some("test".into());

        rec.handle_message("/camera/entrance/compressed", vec![1]);
        rec.handle_message("/weather/current", vec![2]);
        rec.handle_message("/lidar/front", vec![3]);

        assert_eq!(
            rec.buffer[0].meta.message_type,
            "bubbaloop.camera.v1.CompressedImage"
        );
        assert_eq!(
            rec.buffer[1].meta.message_type,
            "bubbaloop.weather.v1.CurrentWeather"
        );
        assert_eq!(rec.buffer[2].meta.message_type, "unknown");
    }

    #[tokio::test]
    async fn test_flush_buffer_writes_to_lancedb() {
        let (mut rec, _dir) = test_recorder().await;
        rec.session_id = Some("test-session".into());

        rec.handle_message("/camera/a/compressed", vec![1, 2]);
        rec.handle_message("/weather/current", vec![3, 4]);
        assert_eq!(rec.buffer_len(), 2);
        assert_eq!(rec.message_count(), 0);

        assert_eq!(rec.flush_buffer().await.unwrap(), 2);
        assert_eq!(rec.buffer_len(), 0);
        assert_eq!(rec.message_count(), 2);

        let results = rec
            .client()
            .query_messages("test-session", None, 0, 100)
            .await
            .unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_raw_data_survives_roundtrip() {
        let (mut rec, _dir) = test_recorder().await;
        rec.session_id = Some("s1".into());

        let payload = vec![0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE];
        rec.handle_message("/camera/a/compressed", payload.clone());
        rec.flush_buffer().await.unwrap();

        let raw = rec
            .client()
            .get_message("s1", "/camera/a/compressed", 0)
            .await
            .unwrap();
        assert_eq!(raw, Some(payload));
    }

    #[tokio::test]
    async fn test_flush_empty_buffer_is_noop() {
        let (mut rec, _dir) = test_recorder().await;
        assert_eq!(rec.flush_buffer().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_session_lifecycle() {
        let (mut rec, _dir) = test_recorder().await;

        let session_id = rec.start_session().await.unwrap();
        assert!(!session_id.is_empty());
        assert!(rec.session_id().is_some());

        rec.handle_message("/camera/entrance/compressed", vec![1, 2, 3]);
        rec.handle_message("/weather/current", vec![4, 5, 6]);
        rec.stop_session().await.unwrap();

        assert!(rec.session_id().is_none());
        assert_eq!(rec.message_count(), 0);
        assert_eq!(rec.buffer_len(), 0);

        let sessions = rec.client().list_sessions().await.unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].status, "completed");
        assert_eq!(sessions[0].message_count, 2);
        assert!(sessions[0].end_time_ns > sessions[0].start_time_ns);
    }

    #[tokio::test]
    async fn test_multiple_sessions() {
        let (mut rec, _dir) = test_recorder().await;

        rec.start_session().await.unwrap();
        rec.handle_message("/camera/a/compressed", vec![1]);
        rec.stop_session().await.unwrap();

        rec.start_session().await.unwrap();
        rec.handle_message("/camera/b/compressed", vec![2]);
        rec.handle_message("/camera/c/compressed", vec![3]);
        rec.stop_session().await.unwrap();

        let sessions = rec.client().list_sessions().await.unwrap();
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].message_count, 2); // Most recent first
        assert_eq!(sessions[1].message_count, 1);
    }

    #[tokio::test]
    async fn test_stop_without_start_is_safe() {
        let (mut rec, _dir) = test_recorder().await;
        rec.stop_session().await.unwrap();
    }

    #[test]
    fn test_hostname_not_empty() {
        assert!(!hostname().is_empty());
    }
}
