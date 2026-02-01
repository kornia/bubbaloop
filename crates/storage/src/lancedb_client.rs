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

impl StorageClient {
    /// Connect to a LanceDB instance at the given URI.
    /// Supports local paths and `gs://bucket/path` for GCS.
    pub async fn connect(uri: &str) -> Result<Self> {
        let connection = lancedb::connect(uri).execute().await?;
        Ok(Self { connection })
    }

    /// Create tables if they don't exist.
    pub async fn ensure_tables(&self) -> Result<()> {
        let existing = self.connection.table_names().execute().await?;

        if !existing.contains(&"messages".to_string()) {
            let schema = messages_schema();
            let empty = empty_batch(schema.clone());
            let batches = RecordBatchIterator::new(vec![Ok(empty)].into_iter(), schema);
            self.connection
                .create_table("messages", Box::new(batches))
                .mode(CreateTableMode::Create)
                .execute()
                .await?;
            log::info!("Created 'messages' table");
        }

        if !existing.contains(&"sessions".to_string()) {
            let schema = sessions_schema();
            let empty = empty_batch(schema.clone());
            let batches = RecordBatchIterator::new(vec![Ok(empty)].into_iter(), schema);
            self.connection
                .create_table("sessions", Box::new(batches))
                .mode(CreateTableMode::Create)
                .execute()
                .await?;
            log::info!("Created 'sessions' table");
        }

        if !existing.contains(&"schemas".to_string()) {
            let schema = schemas_schema();
            let empty = empty_batch(schema.clone());
            let batches = RecordBatchIterator::new(vec![Ok(empty)].into_iter(), schema);
            self.connection
                .create_table("schemas", Box::new(batches))
                .mode(CreateTableMode::Create)
                .execute()
                .await?;
            log::info!("Created 'schemas' table");
        }

        Ok(())
    }

    // --- Messages ---

    /// Insert a batch of messages into the messages table.
    /// Returns the number of messages inserted.
    pub async fn insert_messages(&self, messages: &[StoredMessage]) -> Result<usize> {
        if messages.is_empty() {
            return Ok(0);
        }
        let count = messages.len();
        let schema = messages_schema();

        let timestamp_ns: Int64Array = messages.iter().map(|m| m.meta.timestamp_ns).collect();
        let pub_time_ns: Int64Array = messages.iter().map(|m| m.meta.pub_time_ns).collect();
        let sequence: UInt32Array = messages.iter().map(|m| m.meta.sequence).collect();
        let topic = StringArray::from_iter_values(messages.iter().map(|m| m.meta.topic.as_str()));
        let frame_id =
            StringArray::from_iter_values(messages.iter().map(|m| m.meta.frame_id.as_str()));
        let machine_id =
            StringArray::from_iter_values(messages.iter().map(|m| m.meta.machine_id.as_str()));
        let message_type =
            StringArray::from_iter_values(messages.iter().map(|m| m.meta.message_type.as_str()));
        let data_size: UInt64Array = messages.iter().map(|m| m.meta.data_size).collect();
        let raw_data = LargeBinaryArray::from_iter_values(
            messages.iter().map(|m| m.raw_data.as_slice()),
        );
        let session_id =
            StringArray::from_iter_values(messages.iter().map(|m| m.session_id.as_str()));

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(timestamp_ns),
                Arc::new(pub_time_ns),
                Arc::new(sequence),
                Arc::new(topic),
                Arc::new(frame_id),
                Arc::new(machine_id),
                Arc::new(message_type),
                Arc::new(data_size),
                Arc::new(raw_data),
                Arc::new(session_id),
            ],
        )
        .map_err(|e| StorageError::Arrow(e))?;

        let batches = RecordBatchIterator::new(vec![Ok(batch)].into_iter(), schema);
        let table = self.connection.open_table("messages").execute().await?;
        table.add(Box::new(batches)).execute().await?;

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

        let stream = table
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
            .await?;

        let batches: Vec<RecordBatch> = stream.try_collect().await?;
        let mut results = Vec::new();
        let mut skipped = 0;

        for batch in &batches {
            let ts = batch
                .column_by_name("timestamp_ns")
                .unwrap()
                .as_any()
                .downcast_ref::<Int64Array>()
                .unwrap();
            let pt = batch
                .column_by_name("pub_time_ns")
                .unwrap()
                .as_any()
                .downcast_ref::<Int64Array>()
                .unwrap();
            let seq = batch
                .column_by_name("sequence")
                .unwrap()
                .as_any()
                .downcast_ref::<UInt32Array>()
                .unwrap();
            let tp = batch
                .column_by_name("topic")
                .unwrap()
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            let fi = batch
                .column_by_name("frame_id")
                .unwrap()
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            let mi = batch
                .column_by_name("machine_id")
                .unwrap()
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            let mt = batch
                .column_by_name("message_type")
                .unwrap()
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            let ds = batch
                .column_by_name("data_size")
                .unwrap()
                .as_any()
                .downcast_ref::<UInt64Array>()
                .unwrap();

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

        let stream = table
            .query()
            .select(lancedb::query::Select::Columns(vec!["raw_data".into()]))
            .only_if(filter)
            .limit(1)
            .execute()
            .await?;

        let batches: Vec<RecordBatch> = stream.try_collect().await?;
        for batch in &batches {
            if batch.num_rows() > 0 {
                let col = batch
                    .column_by_name("raw_data")
                    .unwrap()
                    .as_any()
                    .downcast_ref::<LargeBinaryArray>()
                    .unwrap();
                return Ok(Some(col.value(0).to_vec()));
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
        .map_err(|e| StorageError::Arrow(e))?;

        let batches = RecordBatchIterator::new(vec![Ok(batch)].into_iter(), schema);
        let table = self.connection.open_table("sessions").execute().await?;
        table.add(Box::new(batches)).execute().await?;

        Ok(())
    }

    /// Update a session's end time, message count, and status.
    pub async fn update_session(
        &self,
        session_id: &str,
        end_time_ns: i64,
        message_count: u64,
        status: &str,
    ) -> Result<()> {
        // LanceDB doesn't support UPDATE natively. We read, delete, and re-insert.
        let table = self.connection.open_table("sessions").execute().await?;

        let filter = format!("session_id = '{session_id}'");
        let stream = table
            .query()
            .only_if(filter.clone())
            .limit(1)
            .execute()
            .await?;
        let batches: Vec<RecordBatch> = stream.try_collect().await?;

        let mut found = false;
        for batch in &batches {
            if batch.num_rows() > 0 {
                found = true;
                break;
            }
        }

        if !found {
            return Err(StorageError::Session(format!(
                "Session '{session_id}' not found"
            )));
        }

        // Delete old record
        table.delete(&filter).await?;

        // Read original fields from the first matching batch
        let batch = &batches[0];
        let names = batch
            .column_by_name("session_name")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let starts = batch
            .column_by_name("start_time_ns")
            .unwrap()
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap();
        let topics = batch
            .column_by_name("topics")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        let machines = batch
            .column_by_name("machine_id")
            .unwrap()
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();

        let updated = SessionRecord {
            session_id: session_id.to_string(),
            session_name: names.value(0).to_string(),
            start_time_ns: starts.value(0),
            end_time_ns,
            topics: topics.value(0).to_string(),
            message_count,
            machine_id: machines.value(0).to_string(),
            status: status.to_string(),
        };

        self.create_session(&updated).await
    }

    /// List all sessions, most recent first.
    pub async fn list_sessions(&self) -> Result<Vec<SessionRecord>> {
        let table = self.connection.open_table("sessions").execute().await?;

        let stream = table.query().execute().await?;
        let batches: Vec<RecordBatch> = stream.try_collect().await?;

        let mut sessions = Vec::new();
        for batch in &batches {
            let ids = batch
                .column_by_name("session_id")
                .unwrap()
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            let names = batch
                .column_by_name("session_name")
                .unwrap()
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            let starts = batch
                .column_by_name("start_time_ns")
                .unwrap()
                .as_any()
                .downcast_ref::<Int64Array>()
                .unwrap();
            let ends = batch
                .column_by_name("end_time_ns")
                .unwrap()
                .as_any()
                .downcast_ref::<Int64Array>()
                .unwrap();
            let topics = batch
                .column_by_name("topics")
                .unwrap()
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            let counts = batch
                .column_by_name("message_count")
                .unwrap()
                .as_any()
                .downcast_ref::<UInt64Array>()
                .unwrap();
            let machines = batch
                .column_by_name("machine_id")
                .unwrap()
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            let statuses = batch
                .column_by_name("status")
                .unwrap()
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();

            for i in 0..batch.num_rows() {
                sessions.push(SessionRecord {
                    session_id: ids.value(i).to_string(),
                    session_name: names.value(i).to_string(),
                    start_time_ns: starts.value(i),
                    end_time_ns: ends.value(i),
                    topics: topics.value(i).to_string(),
                    message_count: counts.value(i),
                    machine_id: machines.value(i).to_string(),
                    status: statuses.value(i).to_string(),
                });
            }
        }

        // Most recent first
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
        let now_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as i64;

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
        .map_err(|e| StorageError::Arrow(e))?;

        let batches = RecordBatchIterator::new(vec![Ok(batch)].into_iter(), schema);
        let table = self.connection.open_table("schemas").execute().await?;
        table.add(Box::new(batches)).execute().await?;

        Ok(())
    }

    /// Get the schema descriptor for a topic.
    /// Returns (message_type, descriptor_bytes) if found.
    pub async fn get_schema(&self, topic: &str) -> Result<Option<(String, Vec<u8>)>> {
        let table = self.connection.open_table("schemas").execute().await?;

        let filter = format!("topic = '{topic}'");
        let stream = table
            .query()
            .select(lancedb::query::Select::Columns(vec![
                "message_type".into(),
                "descriptor".into(),
            ]))
            .only_if(filter)
            .limit(1)
            .execute()
            .await?;

        let batches: Vec<RecordBatch> = stream.try_collect().await?;
        for batch in &batches {
            if batch.num_rows() > 0 {
                let types = batch
                    .column_by_name("message_type")
                    .unwrap()
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .unwrap();
                let descs = batch
                    .column_by_name("descriptor")
                    .unwrap()
                    .as_any()
                    .downcast_ref::<LargeBinaryArray>()
                    .unwrap();
                return Ok(Some((
                    types.value(0).to_string(),
                    descs.value(0).to_vec(),
                )));
            }
        }

        Ok(None)
    }
}

/// Create an empty RecordBatch with the given schema (zero rows).
fn empty_batch(schema: Arc<Schema>) -> RecordBatch {
    RecordBatch::new_empty(schema)
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
        // Second call should not error
        client.ensure_tables().await.unwrap();
    }

    #[tokio::test]
    async fn test_insert_and_query_messages() {
        let (client, _dir) = test_client().await;

        let messages = vec![
            StoredMessage {
                meta: MessageMeta {
                    timestamp_ns: 1_700_000_000_000_000_000,
                    pub_time_ns: 1_700_000_000_100_000_000,
                    sequence: 1,
                    topic: "/camera/entrance/compressed".into(),
                    frame_id: "cam0".into(),
                    machine_id: "jetson1".into(),
                    message_type: "bubbaloop.camera.v1.CompressedImage".into(),
                    data_size: 4,
                },
                raw_data: vec![0xDE, 0xAD, 0xBE, 0xEF],
                session_id: "session-1".into(),
            },
            StoredMessage {
                meta: MessageMeta {
                    timestamp_ns: 1_700_000_000_200_000_000,
                    pub_time_ns: 1_700_000_000_300_000_000,
                    sequence: 2,
                    topic: "/weather/current".into(),
                    frame_id: "weather".into(),
                    machine_id: "central".into(),
                    message_type: "bubbaloop.weather.v1.CurrentWeather".into(),
                    data_size: 8,
                },
                raw_data: vec![1, 2, 3, 4, 5, 6, 7, 8],
                session_id: "session-1".into(),
            },
        ];

        let count = client.insert_messages(&messages).await.unwrap();
        assert_eq!(count, 2);

        // Query all messages in session
        let results = client
            .query_messages("session-1", None, 0, 100)
            .await
            .unwrap();
        assert_eq!(results.len(), 2);

        // Query filtered by topic
        let results = client
            .query_messages("session-1", Some("/camera/entrance/compressed"), 0, 100)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].sequence, 1);
        assert_eq!(results[0].frame_id, "cam0");
    }

    #[tokio::test]
    async fn test_get_message_raw_bytes() {
        let (client, _dir) = test_client().await;

        let messages = vec![StoredMessage {
            meta: MessageMeta {
                timestamp_ns: 1_700_000_000_000_000_000,
                pub_time_ns: 0,
                sequence: 42,
                topic: "/camera/entrance/compressed".into(),
                frame_id: "cam0".into(),
                machine_id: "jetson1".into(),
                message_type: "bubbaloop.camera.v1.CompressedImage".into(),
                data_size: 4,
            },
            raw_data: vec![0xCA, 0xFE, 0xBA, 0xBE],
            session_id: "session-1".into(),
        }];

        client.insert_messages(&messages).await.unwrap();

        let raw = client
            .get_message("session-1", "/camera/entrance/compressed", 42)
            .await
            .unwrap();
        assert_eq!(raw, Some(vec![0xCA, 0xFE, 0xBA, 0xBE]));

        // Non-existent message
        let raw = client
            .get_message("session-1", "/camera/entrance/compressed", 999)
            .await
            .unwrap();
        assert_eq!(raw, None);
    }

    #[tokio::test]
    async fn test_insert_empty_messages() {
        let (client, _dir) = test_client().await;
        let count = client.insert_messages(&[]).await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_session_lifecycle() {
        let (client, _dir) = test_client().await;

        let session = SessionRecord {
            session_id: "sess-abc".into(),
            session_name: "Test Recording".into(),
            start_time_ns: 1_700_000_000_000_000_000,
            end_time_ns: 0,
            topics: "[\"/camera/**\", \"/weather/**\"]".into(),
            message_count: 0,
            machine_id: "central".into(),
            status: "recording".into(),
        };

        client.create_session(&session).await.unwrap();

        let sessions = client.list_sessions().await.unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "sess-abc");
        assert_eq!(sessions[0].status, "recording");

        // Update session
        client
            .update_session(
                "sess-abc",
                1_700_000_060_000_000_000,
                150,
                "completed",
            )
            .await
            .unwrap();

        let sessions = client.list_sessions().await.unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].status, "completed");
        assert_eq!(sessions[0].message_count, 150);
        assert_eq!(sessions[0].end_time_ns, 1_700_000_060_000_000_000);
    }

    #[tokio::test]
    async fn test_schema_registry() {
        let (client, _dir) = test_client().await;

        let descriptor = vec![0x0A, 0x0B, 0x0C]; // fake descriptor bytes
        client
            .register_schema(
                "/camera/entrance/compressed",
                "bubbaloop.camera.v1.CompressedImage",
                &descriptor,
                "session-1",
            )
            .await
            .unwrap();

        let result = client
            .get_schema("/camera/entrance/compressed")
            .await
            .unwrap();
        assert!(result.is_some());
        let (type_name, desc_bytes) = result.unwrap();
        assert_eq!(type_name, "bubbaloop.camera.v1.CompressedImage");
        assert_eq!(desc_bytes, vec![0x0A, 0x0B, 0x0C]);

        // Non-existent schema
        let result = client.get_schema("/lidar/front").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_update_nonexistent_session_errors() {
        let (client, _dir) = test_client().await;

        let result = client
            .update_session("nonexistent", 0, 0, "completed")
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sessions_sorted_most_recent_first() {
        let (client, _dir) = test_client().await;

        for (i, ts) in [1_000_000_000i64, 3_000_000_000, 2_000_000_000]
            .iter()
            .enumerate()
        {
            client
                .create_session(&SessionRecord {
                    session_id: format!("sess-{i}"),
                    session_name: format!("Session {i}"),
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
        assert_eq!(sessions.len(), 3);
        assert_eq!(sessions[0].start_time_ns, 3_000_000_000);
        assert_eq!(sessions[1].start_time_ns, 2_000_000_000);
        assert_eq!(sessions[2].start_time_ns, 1_000_000_000);
    }
}
