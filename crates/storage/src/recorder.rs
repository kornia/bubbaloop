use std::sync::Arc;

use tokio::sync::{mpsc, watch};
use uuid::Uuid;

use crate::config::StorageConfig;
use crate::error::{Result, StorageError};
use crate::header::MessageMeta;
use crate::lancedb_client::{SessionRecord, StorageClient, StoredMessage};

/// Recording engine that bridges Zenoh subscriptions to LanceDB storage.
///
/// Subscribes to configured topic patterns, extracts Header metadata from
/// each message, buffers payloads, and flushes batches to LanceDB.
pub struct Recorder {
    client: StorageClient,
    config: StorageConfig,
    session_id: Option<String>,
    message_count: u64,
    buffer: Vec<StoredMessage>,
}

impl Recorder {
    /// Create a new recorder with the given storage client and configuration.
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
    /// Subscribes to all configured topic patterns, receives messages,
    /// buffers them, and flushes to LanceDB based on batch_size and
    /// flush_interval_secs from config.
    pub async fn run(
        &mut self,
        zenoh_session: Arc<zenoh::Session>,
        mut shutdown_rx: watch::Receiver<()>,
    ) -> Result<()> {
        self.client.ensure_tables().await?;

        let session_id = self.start_session().await?;
        log::info!("Started recording session: {session_id}");

        // Channel to merge all subscriber streams into one
        let (tx, mut rx) = mpsc::channel::<(String, Vec<u8>)>(1000);

        // Subscribe to each topic pattern, each feeding into the channel
        let mut subscriber_tasks = Vec::new();
        for pattern in &self.config.topics {
            let sub = zenoh_session
                .declare_subscriber(pattern)
                .await
                .map_err(|e| {
                    StorageError::Zenoh(format!("Subscribe failed for '{pattern}': {e}"))
                })?;
            log::info!("Subscribed to: {pattern}");

            let tx = tx.clone();
            let handle = tokio::spawn(async move {
                loop {
                    match sub.recv_async().await {
                        Ok(sample) => {
                            let topic = sample.key_expr().to_string();
                            let payload = sample.payload().to_bytes().to_vec();
                            if tx.send((topic, payload)).await.is_err() {
                                break; // Receiver dropped
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
            subscriber_tasks.push(handle);
        }
        drop(tx); // Drop original so channel closes when all tasks end

        // Flush timer
        let mut flush_interval = tokio::time::interval(std::time::Duration::from_secs(
            self.config.flush_interval_secs,
        ));
        flush_interval.tick().await; // Consume initial immediate tick

        // Main recording loop
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
                _ = flush_interval.tick() => {
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

        // Final flush and close session
        self.stop_session().await?;

        for handle in subscriber_tasks {
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

    /// Create a new recording session in LanceDB.
    async fn start_session(&mut self) -> Result<String> {
        let session_id = Uuid::new_v4().to_string();
        let now_ns = now_nanos();

        let topics_str = format!(
            "[{}]",
            self.config
                .topics
                .iter()
                .map(|t| format!("\"{t}\""))
                .collect::<Vec<_>>()
                .join(", ")
        );

        let record = SessionRecord {
            session_id: session_id.clone(),
            session_name: format!("rec-{}", &session_id[..8]),
            start_time_ns: now_ns,
            end_time_ns: 0,
            topics: topics_str,
            message_count: 0,
            machine_id: hostname(),
            status: "recording".into(),
        };

        self.client.create_session(&record).await?;
        self.session_id = Some(session_id.clone());
        self.message_count = 0;
        self.buffer.clear();

        Ok(session_id)
    }

    /// Flush remaining messages and mark session as completed.
    async fn stop_session(&mut self) -> Result<()> {
        self.flush_buffer().await?;

        if let Some(session_id) = self.session_id.take() {
            let now_ns = now_nanos();
            self.client
                .update_session(&session_id, now_ns, self.message_count, "completed")
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

    /// Get the current recording session ID, if any.
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// Get the number of messages currently buffered (not yet flushed).
    pub fn buffer_len(&self) -> usize {
        self.buffer.len()
    }

    /// Get the total messages recorded in the current session.
    pub fn message_count(&self) -> u64 {
        self.message_count
    }
}

fn now_nanos() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as i64
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
        let uri = dir.path().to_str().unwrap();
        let client = StorageClient::connect(uri).await.unwrap();
        client.ensure_tables().await.unwrap();
        let config = test_config();
        (Recorder::new(client, config), dir)
    }

    #[tokio::test]
    async fn test_handle_message_buffers_with_metadata() {
        let (mut recorder, _dir) = test_recorder().await;
        recorder.session_id = Some("test-session".into());

        recorder.handle_message("/camera/entrance/compressed", vec![1, 2, 3, 4]);

        assert_eq!(recorder.buffer_len(), 1);
        assert_eq!(recorder.buffer[0].meta.topic, "/camera/entrance/compressed");
        assert_eq!(
            recorder.buffer[0].meta.message_type,
            "bubbaloop.camera.v1.CompressedImage"
        );
        assert_eq!(recorder.buffer[0].raw_data, vec![1, 2, 3, 4]);
        assert_eq!(recorder.buffer[0].session_id, "test-session");
    }

    #[tokio::test]
    async fn test_schema_hints_resolve_type() {
        let (mut recorder, _dir) = test_recorder().await;
        recorder.session_id = Some("test".into());

        recorder.handle_message("/camera/entrance/compressed", vec![1]);
        assert_eq!(
            recorder.buffer[0].meta.message_type,
            "bubbaloop.camera.v1.CompressedImage"
        );

        recorder.handle_message("/weather/current", vec![2]);
        assert_eq!(
            recorder.buffer[1].meta.message_type,
            "bubbaloop.weather.v1.CurrentWeather"
        );

        recorder.handle_message("/lidar/front", vec![3]);
        assert_eq!(recorder.buffer[2].meta.message_type, "unknown");
    }

    #[tokio::test]
    async fn test_flush_buffer_writes_to_lancedb() {
        let (mut recorder, _dir) = test_recorder().await;
        recorder.session_id = Some("test-session".into());

        recorder.handle_message("/camera/a/compressed", vec![1, 2]);
        recorder.handle_message("/weather/current", vec![3, 4]);
        assert_eq!(recorder.buffer_len(), 2);
        assert_eq!(recorder.message_count(), 0);

        let flushed = recorder.flush_buffer().await.unwrap();
        assert_eq!(flushed, 2);
        assert_eq!(recorder.buffer_len(), 0);
        assert_eq!(recorder.message_count(), 2);

        // Verify data persisted in LanceDB
        let results = recorder
            .client()
            .query_messages("test-session", None, 0, 100)
            .await
            .unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_flush_empty_buffer_is_noop() {
        let (mut recorder, _dir) = test_recorder().await;
        let flushed = recorder.flush_buffer().await.unwrap();
        assert_eq!(flushed, 0);
        assert_eq!(recorder.message_count(), 0);
    }

    #[tokio::test]
    async fn test_session_lifecycle() {
        let (mut recorder, _dir) = test_recorder().await;

        let session_id = recorder.start_session().await.unwrap();
        assert!(!session_id.is_empty());
        assert!(recorder.session_id().is_some());
        assert_eq!(recorder.message_count(), 0);

        recorder.handle_message("/camera/entrance/compressed", vec![1, 2, 3]);
        recorder.handle_message("/weather/current", vec![4, 5, 6]);

        recorder.stop_session().await.unwrap();
        assert!(recorder.session_id().is_none());
        assert_eq!(recorder.message_count(), 0); // Reset after stop
        assert_eq!(recorder.buffer_len(), 0);

        // Verify session saved as completed with correct count
        let sessions = recorder.client().list_sessions().await.unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].status, "completed");
        assert_eq!(sessions[0].message_count, 2);
        assert!(sessions[0].end_time_ns > sessions[0].start_time_ns);
    }

    #[tokio::test]
    async fn test_multiple_sessions() {
        let (mut recorder, _dir) = test_recorder().await;

        // First session
        recorder.start_session().await.unwrap();
        recorder.handle_message("/camera/a/compressed", vec![1]);
        recorder.stop_session().await.unwrap();

        // Second session
        recorder.start_session().await.unwrap();
        recorder.handle_message("/camera/b/compressed", vec![2]);
        recorder.handle_message("/camera/c/compressed", vec![3]);
        recorder.stop_session().await.unwrap();

        let sessions = recorder.client().list_sessions().await.unwrap();
        assert_eq!(sessions.len(), 2);
        // Most recent first
        assert_eq!(sessions[0].message_count, 2);
        assert_eq!(sessions[1].message_count, 1);
    }

    #[tokio::test]
    async fn test_stop_without_start_is_safe() {
        let (mut recorder, _dir) = test_recorder().await;
        // Should not error when no session is active
        recorder.stop_session().await.unwrap();
    }

    #[test]
    fn test_now_nanos_reasonable() {
        let ns = now_nanos();
        // After 2020-01-01
        assert!(ns > 1_577_836_800_000_000_000);
    }

    #[test]
    fn test_hostname_not_empty() {
        let h = hostname();
        assert!(!h.is_empty());
    }
}
