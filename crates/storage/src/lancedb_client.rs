//! LanceDB-backed storage for messages, sessions, and schema descriptors.
//!
//! Three tables:
//! - **messages**: Raw protobuf payloads with extracted Header metadata.
//! - **sessions**: Recording session lifecycle (start, stop, counts).
//! - **schemas**: Protobuf `FileDescriptorSet` per topic for downstream decoding.

use std::sync::Arc;

use arrow_array::{
    Int64Array, LargeBinaryArray, RecordBatch, RecordBatchIterator, StringArray, UInt32Array,
    UInt64Array,
};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use lancedb::database::CreateTableMode;
use lancedb::query::{ExecutableQuery, QueryBase};

use crate::error::{Result, StorageError};
use crate::header::MessageMeta;

/// Downcast an Arrow column by name. Panics if column is missing or wrong type —
/// this is safe because we control all schemas and only read tables we created.
macro_rules! col {
    ($batch:expr, $name:expr, $ty:ty) => {
        $batch
            .column_by_name($name)
            .unwrap()
            .as_any()
            .downcast_ref::<$ty>()
            .unwrap()
    };
}

/// A stored message with metadata and raw payload.
#[derive(Debug, Clone)]
pub struct StoredMessage {
    pub meta: MessageMeta,
    pub raw_data: Vec<u8>,
    pub session_id: String,
}

/// A recording session record.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionRecord {
    pub session_id: String,
    pub session_name: String,
    pub start_time_ns: i64,
    pub end_time_ns: i64,
    pub topics: String,
    pub message_count: u64,
    pub machine_id: String,
    pub status: String,
}

/// LanceDB-backed storage client for messages, sessions, and schemas.
pub struct StorageClient {
    connection: lancedb::Connection,
}

fn messages_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("timestamp_ns", DataType::Int64, false),
        Field::new("pub_time_ns", DataType::Int64, false),
        Field::new("sequence", DataType::UInt32, false),
        Field::new("topic", DataType::Utf8, false),
        Field::new("frame_id", DataType::Utf8, false),
        Field::new("machine_id", DataType::Utf8, false),
        Field::new("message_type", DataType::Utf8, false),
        Field::new("data_size", DataType::UInt64, false),
        Field::new("raw_data", DataType::LargeBinary, false),
        Field::new("session_id", DataType::Utf8, false),
    ]))
}

fn sessions_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("session_id", DataType::Utf8, false),
        Field::new("session_name", DataType::Utf8, false),
        Field::new("start_time_ns", DataType::Int64, false),
        Field::new("end_time_ns", DataType::Int64, false),
        Field::new("topics", DataType::Utf8, false),
        Field::new("message_count", DataType::UInt64, false),
        Field::new("machine_id", DataType::Utf8, false),
        Field::new("status", DataType::Utf8, false),
    ]))
}

fn schemas_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("topic", DataType::Utf8, false),
        Field::new("message_type", DataType::Utf8, false),
        Field::new("descriptor", DataType::LargeBinary, false),
        Field::new("first_seen_ns", DataType::Int64, false),
        Field::new("session_id", DataType::Utf8, false),
    ]))
}

/// Extract a `SessionRecord` from row `idx` of a `RecordBatch`.
fn session_from_batch(batch: &RecordBatch, idx: usize) -> SessionRecord {
    SessionRecord {
        session_id: col!(batch, "session_id", StringArray).value(idx).to_string(),
        session_name: col!(batch, "session_name", StringArray)
            .value(idx)
            .to_string(),
        start_time_ns: col!(batch, "start_time_ns", Int64Array).value(idx),
        end_time_ns: col!(batch, "end_time_ns", Int64Array).value(idx),
        topics: col!(batch, "topics", StringArray).value(idx).to_string(),
        message_count: col!(batch, "message_count", UInt64Array).value(idx),
        machine_id: col!(batch, "machine_id", StringArray)
            .value(idx)
            .to_string(),
        status: col!(batch, "status", StringArray).value(idx).to_string(),
    }
}

impl StorageClient {
    /// Connect to a LanceDB instance at the given URI.
    ///
    /// Supports local paths (`./recordings`) and GCS (`gs://bucket/path`).
    pub async fn connect(uri: &str) -> Result<Self> {
        let connection = lancedb::connect(uri).execute().await?;
        Ok(Self { connection })
    }

    /// Create the messages, sessions, and schemas tables if they don't exist.
    pub async fn ensure_tables(&self) -> Result<()> {
        let existing = self.connection.table_names().execute().await?;

        for (name, schema) in [
            ("messages", messages_schema()),
            ("sessions", sessions_schema()),
            ("schemas", schemas_schema()),
        ] {
            if !existing.contains(&name.to_string()) {
                let empty = RecordBatch::new_empty(schema.clone());
                let batches = RecordBatchIterator::new(vec![Ok(empty)].into_iter(), schema);
                self.connection
                    .create_table(name, Box::new(batches))
                    .mode(CreateTableMode::Create)
                    .execute()
                    .await?;
                log::info!("Created '{name}' table");
            }
        }

        Ok(())
    }

    /// Insert a `RecordBatch` into an existing table.
    async fn add_batch(
        &self,
        table_name: &str,
        batch: RecordBatch,
        schema: Arc<Schema>,
    ) -> Result<()> {
        let batches = RecordBatchIterator::new(vec![Ok(batch)].into_iter(), schema);
        let table = self.connection.open_table(table_name).execute().await?;
        table.add(Box::new(batches)).execute().await?;
        Ok(())
    }

    // --- Messages ---

    /// Insert a batch of messages. Returns the number inserted.
    pub async fn insert_messages(&self, messages: &[StoredMessage]) -> Result<usize> {
        if messages.is_empty() {
            return Ok(0);
        }
        let count = messages.len();
        let schema = messages_schema();

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(Int64Array::from_iter_values(
                    messages.iter().map(|m| m.meta.timestamp_ns),
                )),
                Arc::new(Int64Array::from_iter_values(
                    messages.iter().map(|m| m.meta.pub_time_ns),
                )),
                Arc::new(UInt32Array::from_iter_values(
                    messages.iter().map(|m| m.meta.sequence),
                )),
                Arc::new(StringArray::from_iter_values(
                    messages.iter().map(|m| m.meta.topic.as_str()),
                )),
                Arc::new(StringArray::from_iter_values(
                    messages.iter().map(|m| m.meta.frame_id.as_str()),
                )),
                Arc::new(StringArray::from_iter_values(
                    messages.iter().map(|m| m.meta.machine_id.as_str()),
                )),
                Arc::new(StringArray::from_iter_values(
                    messages.iter().map(|m| m.meta.message_type.as_str()),
                )),
                Arc::new(UInt64Array::from_iter_values(
                    messages.iter().map(|m| m.meta.data_size),
                )),
                Arc::new(LargeBinaryArray::from_iter_values(
                    messages.iter().map(|m| m.raw_data.as_slice()),
                )),
                Arc::new(StringArray::from_iter_values(
                    messages.iter().map(|m| m.session_id.as_str()),
                )),
            ],
        )
        .map_err(StorageError::Arrow)?;

        self.add_batch("messages", batch, schema).await?;
        Ok(count)
    }

    /// Query message metadata for a session, optionally filtered by topic.
    pub async fn query_messages(
        &self,
        session_id: &str,
        topic: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<MessageMeta>> {
        let table = self.connection.open_table("messages").execute().await?;

        let filter = match topic {
            Some(t) => format!("session_id = '{session_id}' AND topic = '{t}'"),
            None => format!("session_id = '{session_id}'"),
        };

        let batches: Vec<RecordBatch> = table
            .query()
            .select(lancedb::query::Select::Columns(vec![
                "timestamp_ns".into(),
                "pub_time_ns".into(),
                "sequence".into(),
                "topic".into(),
                "frame_id".into(),
                "machine_id".into(),
                "message_type".into(),
                "data_size".into(),
            ]))
            .only_if(filter)
            .limit(limit + offset)
            .execute()
            .await?
            .try_collect()
            .await?;

        let mut results = Vec::new();
        let mut skipped = 0;

        for batch in &batches {
            let ts = col!(batch, "timestamp_ns", Int64Array);
            let pt = col!(batch, "pub_time_ns", Int64Array);
            let seq = col!(batch, "sequence", UInt32Array);
            let tp = col!(batch, "topic", StringArray);
            let fi = col!(batch, "frame_id", StringArray);
            let mi = col!(batch, "machine_id", StringArray);
            let mt = col!(batch, "message_type", StringArray);
            let ds = col!(batch, "data_size", UInt64Array);

            for i in 0..batch.num_rows() {
                if skipped < offset {
                    skipped += 1;
                    continue;
                }
                if results.len() >= limit {
                    break;
                }
                results.push(MessageMeta {
                    timestamp_ns: ts.value(i),
                    pub_time_ns: pt.value(i),
                    sequence: seq.value(i),
                    topic: tp.value(i).to_string(),
                    frame_id: fi.value(i).to_string(),
                    machine_id: mi.value(i).to_string(),
                    message_type: mt.value(i).to_string(),
                    data_size: ds.value(i),
                });
            }
        }

        Ok(results)
    }

    /// Get the raw bytes of a single message by session, topic, and sequence.
    pub async fn get_message(
        &self,
        session_id: &str,
        topic: &str,
        sequence: u32,
    ) -> Result<Option<Vec<u8>>> {
        let table = self.connection.open_table("messages").execute().await?;

        let filter = format!(
            "session_id = '{session_id}' AND topic = '{topic}' AND sequence = {sequence}"
        );

        let batches: Vec<RecordBatch> = table
            .query()
            .select(lancedb::query::Select::Columns(vec!["raw_data".into()]))
            .only_if(filter)
            .limit(1)
            .execute()
            .await?
            .try_collect()
            .await?;

        for batch in &batches {
            if batch.num_rows() > 0 {
                return Ok(Some(col!(batch, "raw_data", LargeBinaryArray).value(0).to_vec()));
            }
        }

        Ok(None)
    }

    // --- Sessions ---

    /// Create a new recording session.
    pub async fn create_session(&self, session: &SessionRecord) -> Result<()> {
        let schema = sessions_schema();

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(vec![session.session_id.as_str()])),
                Arc::new(StringArray::from(vec![session.session_name.as_str()])),
                Arc::new(Int64Array::from(vec![session.start_time_ns])),
                Arc::new(Int64Array::from(vec![session.end_time_ns])),
                Arc::new(StringArray::from(vec![session.topics.as_str()])),
                Arc::new(UInt64Array::from(vec![session.message_count])),
                Arc::new(StringArray::from(vec![session.machine_id.as_str()])),
                Arc::new(StringArray::from(vec![session.status.as_str()])),
            ],
        )
        .map_err(StorageError::Arrow)?;

        self.add_batch("sessions", batch, schema).await
    }

    /// Update a session's end time, message count, and status.
    ///
    /// Uses read-delete-reinsert since LanceDB has no native UPDATE.
    pub async fn update_session(
        &self,
        session_id: &str,
        end_time_ns: i64,
        message_count: u64,
        status: &str,
    ) -> Result<()> {
        let table = self.connection.open_table("sessions").execute().await?;
        let filter = format!("session_id = '{session_id}'");

        let batches: Vec<RecordBatch> = table
            .query()
            .only_if(filter.clone())
            .limit(1)
            .execute()
            .await?
            .try_collect()
            .await?;

        let batch = batches
            .iter()
            .find(|b| b.num_rows() > 0)
            .ok_or_else(|| StorageError::Session(format!("Session '{session_id}' not found")))?;

        let original = session_from_batch(batch, 0);

        table.delete(&filter).await?;

        self.create_session(&SessionRecord {
            end_time_ns,
            message_count,
            status: status.to_string(),
            ..original
        })
        .await
    }

    /// List all sessions, most recent first.
    pub async fn list_sessions(&self) -> Result<Vec<SessionRecord>> {
        let table = self.connection.open_table("sessions").execute().await?;

        let batches: Vec<RecordBatch> = table.query().execute().await?.try_collect().await?;

        let mut sessions: Vec<SessionRecord> = batches
            .iter()
            .flat_map(|batch| (0..batch.num_rows()).map(|i| session_from_batch(batch, i)))
            .collect();

        sessions.sort_by(|a, b| b.start_time_ns.cmp(&a.start_time_ns));
        Ok(sessions)
    }

    // --- Schema Registry ---

    /// Register a protobuf schema descriptor for a topic.
    pub async fn register_schema(
        &self,
        topic: &str,
        type_name: &str,
        descriptor: &[u8],
        session_id: &str,
    ) -> Result<()> {
        let now_ns = crate::now_nanos();
        let schema = schemas_schema();

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(vec![topic])),
                Arc::new(StringArray::from(vec![type_name])),
                Arc::new(LargeBinaryArray::from_iter_values(
                    std::iter::once(descriptor),
                )),
                Arc::new(Int64Array::from(vec![now_ns])),
                Arc::new(StringArray::from(vec![session_id])),
            ],
        )
        .map_err(StorageError::Arrow)?;

        self.add_batch("schemas", batch, schema).await
    }

    /// Get the schema descriptor for a topic.
    ///
    /// Returns `(message_type, descriptor_bytes)` if found.
    pub async fn get_schema(&self, topic: &str) -> Result<Option<(String, Vec<u8>)>> {
        let table = self.connection.open_table("schemas").execute().await?;

        let batches: Vec<RecordBatch> = table
            .query()
            .select(lancedb::query::Select::Columns(vec![
                "message_type".into(),
                "descriptor".into(),
            ]))
            .only_if(format!("topic = '{topic}'"))
            .limit(1)
            .execute()
            .await?
            .try_collect()
            .await?;

        for batch in &batches {
            if batch.num_rows() > 0 {
                let msg_type = col!(batch, "message_type", StringArray).value(0).to_string();
                let desc = col!(batch, "descriptor", LargeBinaryArray).value(0).to_vec();
                return Ok(Some((msg_type, desc)));
            }
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn test_client() -> (StorageClient, TempDir) {
        let dir = TempDir::new().unwrap();
        let uri = dir.path().to_str().unwrap();
        let client = StorageClient::connect(uri).await.unwrap();
        client.ensure_tables().await.unwrap();
        (client, dir)
    }

    fn msg(topic: &str, seq: u32, data: Vec<u8>, session: &str) -> StoredMessage {
        StoredMessage {
            meta: MessageMeta {
                timestamp_ns: 1_700_000_000_000_000_000 + seq as i64,
                pub_time_ns: 0,
                sequence: seq,
                topic: topic.into(),
                frame_id: "test".into(),
                machine_id: "test".into(),
                message_type: "test.Message".into(),
                data_size: data.len() as u64,
            },
            raw_data: data,
            session_id: session.into(),
        }
    }

    #[tokio::test]
    async fn test_ensure_tables_creates_all_three() {
        let (client, _dir) = test_client().await;
        let tables = client.connection.table_names().execute().await.unwrap();
        assert!(tables.contains(&"messages".to_string()));
        assert!(tables.contains(&"sessions".to_string()));
        assert!(tables.contains(&"schemas".to_string()));
    }

    #[tokio::test]
    async fn test_ensure_tables_idempotent() {
        let (client, _dir) = test_client().await;
        client.ensure_tables().await.unwrap();
    }

    #[tokio::test]
    async fn test_insert_and_query_messages() {
        let (client, _dir) = test_client().await;

        let messages = vec![
            msg("/camera/entrance/compressed", 1, vec![0xDE, 0xAD], "s1"),
            msg("/weather/current", 2, vec![1, 2, 3], "s1"),
        ];

        assert_eq!(client.insert_messages(&messages).await.unwrap(), 2);

        let all = client.query_messages("s1", None, 0, 100).await.unwrap();
        assert_eq!(all.len(), 2);

        let cam = client
            .query_messages("s1", Some("/camera/entrance/compressed"), 0, 100)
            .await
            .unwrap();
        assert_eq!(cam.len(), 1);
        assert_eq!(cam[0].sequence, 1);
    }

    #[tokio::test]
    async fn test_query_messages_with_offset() {
        let (client, _dir) = test_client().await;

        let messages: Vec<_> = (0..5)
            .map(|i| msg("/camera/a/compressed", i, vec![i as u8], "s1"))
            .collect();
        client.insert_messages(&messages).await.unwrap();

        let page = client.query_messages("s1", None, 2, 2).await.unwrap();
        assert_eq!(page.len(), 2);
    }

    #[tokio::test]
    async fn test_query_messages_wrong_session() {
        let (client, _dir) = test_client().await;
        client
            .insert_messages(&[msg("/test", 1, vec![1], "s1")])
            .await
            .unwrap();

        let results = client.query_messages("s2", None, 0, 100).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_get_message_raw_bytes() {
        let (client, _dir) = test_client().await;
        client
            .insert_messages(&[msg("/cam", 42, vec![0xCA, 0xFE], "s1")])
            .await
            .unwrap();

        assert_eq!(
            client.get_message("s1", "/cam", 42).await.unwrap(),
            Some(vec![0xCA, 0xFE])
        );
        assert_eq!(client.get_message("s1", "/cam", 999).await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_insert_empty_messages() {
        let (client, _dir) = test_client().await;
        assert_eq!(client.insert_messages(&[]).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_session_lifecycle() {
        let (client, _dir) = test_client().await;

        client
            .create_session(&SessionRecord {
                session_id: "s1".into(),
                session_name: "Test".into(),
                start_time_ns: 1_000,
                end_time_ns: 0,
                topics: "[]".into(),
                message_count: 0,
                machine_id: "m1".into(),
                status: "recording".into(),
            })
            .await
            .unwrap();

        let sessions = client.list_sessions().await.unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].status, "recording");

        client
            .update_session("s1", 2_000, 150, "completed")
            .await
            .unwrap();

        let sessions = client.list_sessions().await.unwrap();
        assert_eq!(sessions[0].status, "completed");
        assert_eq!(sessions[0].message_count, 150);
        assert_eq!(sessions[0].end_time_ns, 2_000);
        assert_eq!(sessions[0].session_name, "Test"); // preserved
    }

    #[tokio::test]
    async fn test_update_nonexistent_session_errors() {
        let (client, _dir) = test_client().await;
        assert!(client
            .update_session("nonexistent", 0, 0, "completed")
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_sessions_sorted_most_recent_first() {
        let (client, _dir) = test_client().await;

        for (i, ts) in [1_000i64, 3_000, 2_000].iter().enumerate() {
            client
                .create_session(&SessionRecord {
                    session_id: format!("s{i}"),
                    session_name: format!("S{i}"),
                    start_time_ns: *ts,
                    end_time_ns: 0,
                    topics: "[]".into(),
                    message_count: 0,
                    machine_id: "test".into(),
                    status: "completed".into(),
                })
                .await
                .unwrap();
        }

        let sessions = client.list_sessions().await.unwrap();
        assert_eq!(sessions[0].start_time_ns, 3_000);
        assert_eq!(sessions[1].start_time_ns, 2_000);
        assert_eq!(sessions[2].start_time_ns, 1_000);
    }

    #[tokio::test]
    async fn test_schema_registry() {
        let (client, _dir) = test_client().await;

        client
            .register_schema("/camera/a", "cam.v1.Image", &[0x0A, 0x0B], "s1")
            .await
            .unwrap();

        let (type_name, desc) = client.get_schema("/camera/a").await.unwrap().unwrap();
        assert_eq!(type_name, "cam.v1.Image");
        assert_eq!(desc, vec![0x0A, 0x0B]);

        assert!(client.get_schema("/lidar/front").await.unwrap().is_none());
    }
}
