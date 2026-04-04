use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;

use prost_reflect::{DescriptorPool, DynamicMessage};

use crate::error::{NodeError, Result};

/// Dynamic protobuf decoder — fetch a node's schema once, decode any sample on demand.
///
/// Fetches `FileDescriptorSet` bytes from a node's `/schema` Zenoh queryable,
/// builds a [`DescriptorPool`], and decodes protobuf samples into
/// [`serde_json::Value`] without pre-generated Rust types.
///
/// Intended for agent code that needs to inspect node data at runtime.
/// One instance can be shared across many decode calls; schemas are cached.
///
/// # Example
///
/// ```ignore
/// let decoder = ProtoDecoder::new(session.clone());
///
/// // Prefetch all schemas at once (optional, avoids per-call latency)
/// decoder.prefetch("bubbaloop/**/schema").await?;
///
/// // Grab a frame and decode
/// let sample = get_sample(&session, "bubbaloop/local/host/camera/.../compressed",
///                          Duration::from_secs(2)).await?;
/// let value = decoder.decode(&sample).await?;
/// println!("{}", serde_json::to_string_pretty(&value)?);
/// ```
#[derive(Clone)]
pub struct ProtoDecoder {
    session: Arc<zenoh::Session>,
    // type_name → message class (pool already parsed)
    cache: Arc<Mutex<HashMap<String, DescriptorPool>>>,
}

impl ProtoDecoder {
    pub fn new(session: Arc<zenoh::Session>) -> Self {
        Self { session, cache: Arc::new(Mutex::new(HashMap::new())) }
    }

    /// Decode a Zenoh sample whose encoding is `application/protobuf;<TypeName>`.
    ///
    /// Fetches the schema on the first call for a given type; subsequent calls
    /// use the cached [`DescriptorPool`].
    ///
    /// Returns `None` if the encoding is not protobuf or has no type suffix.
    pub async fn decode(
        &self,
        sample: &zenoh::sample::Sample,
    ) -> Result<Option<serde_json::Value>> {
        let type_name = match proto_type_from_encoding(sample.encoding()) {
            Some(t) => t,
            None => return Ok(None),
        };

        let schema_key = schema_key_for(sample);
        let pool = self.get_or_fetch_pool(&type_name, &schema_key).await?;

        let msg_desc = pool
            .get_message_by_name(&type_name)
            .ok_or_else(|| NodeError::GetSampleTimeout { topic: type_name.clone() })?;

        let payload = sample.payload().to_bytes();
        let msg = DynamicMessage::decode(msg_desc, payload.as_ref())
            .map_err(|e| NodeError::Publish(zenoh::Error::from(e.to_string())))?;

        Ok(Some(serde_json::to_value(&msg).map_err(NodeError::Json)?))
    }

    /// Eagerly fetch and cache all schemas matching `key_expr` (e.g. `"bubbaloop/**/schema"`).
    pub async fn prefetch(&self, key_expr: &str) -> Result<usize> {
        let replies = self
            .session
            .get(key_expr)
            .timeout(std::time::Duration::from_secs(3))
            .await
            .map_err(NodeError::SchemaQueryable)?;

        let mut count = 0;
        while let Ok(reply) = replies.recv_async().await {
            if let Ok(sample) = reply.result() {
                let bytes = sample.payload().to_bytes();
                if let Ok(pool) = DescriptorPool::decode(bytes.as_ref()) {
                    self.store_pool(pool).await;
                    count += 1;
                }
            }
        }
        Ok(count)
    }

    // ------------------------------------------------------------------
    // Internals
    // ------------------------------------------------------------------

    async fn get_or_fetch_pool(&self, type_name: &str, schema_key: &str) -> Result<DescriptorPool> {
        {
            let cache = self.cache.lock().await;
            if let Some(pool) = cache.get(type_name) {
                return Ok(pool.clone());
            }
        }

        // Not cached — fetch the schema
        let replies = self
            .session
            .get(schema_key)
            .timeout(std::time::Duration::from_secs(3))
            .await
            .map_err(NodeError::SchemaQueryable)?;

        while let Ok(reply) = replies.recv_async().await {
            if let Ok(sample) = reply.result() {
                let bytes = sample.payload().to_bytes();
                if let Ok(pool) = DescriptorPool::decode(bytes.as_ref()) {
                    self.store_pool(pool).await;
                    // Retry cache lookup
                    let cache = self.cache.lock().await;
                    if let Some(pool) = cache.get(type_name) {
                        return Ok(pool.clone());
                    }
                }
            }
        }

        Err(NodeError::GetSampleTimeout { topic: schema_key.to_string() })
    }

    async fn store_pool(&self, pool: DescriptorPool) {
        let mut cache = self.cache.lock().await;
        for file in pool.files() {
            for msg in file.messages() {
                cache.entry(msg.full_name().to_string()).or_insert_with(|| pool.clone());
            }
        }
    }
}

/// Extract the protobuf type name from a Zenoh encoding.
///
/// `"application/protobuf;bubbaloop.camera.v1.CompressedImage"` → `Some("bubbaloop.camera.v1.CompressedImage")`
pub fn proto_type_from_encoding(encoding: &zenoh::bytes::Encoding) -> Option<String> {
    let s = encoding.to_string();
    if !s.starts_with("application/protobuf;") {
        return None;
    }
    let type_name = s.split_once(';')?.1.trim().to_string();
    if type_name.is_empty() { None } else { Some(type_name) }
}

fn schema_key_for(sample: &zenoh::sample::Sample) -> String {
    let key = sample.key_expr().as_str();
    match key.rfind('/') {
        Some(pos) => format!("{}/schema", &key[..pos]),
        None => format!("{}/schema", key),
    }
}
