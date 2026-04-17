use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use crate::error::Result;
use crate::manifest::{IoEntry, Liveness};

/// Context provided to nodes by the SDK runtime.
///
/// **Publishers** auto-scope under [`instance_name`](Self::instance_name) by default —
/// `publisher_*(suffix)` publishes on
/// `bubbaloop/{global|local}/{machine_id}/{instance_name}/{suffix}`. This guarantees
/// two nodes on the same machine can never collide in the global keyspace; every
/// node owns its own output namespace.
///
/// **Subscribers** take *absolute* suffixes by default — `subscriber_*(suffix)`
/// subscribes on `bubbaloop/{global|local}/{machine_id}/{suffix}`. The 99% case
/// is reading another node's outputs (e.g. `tapo_entrance/compressed`), so the
/// caller passes the full instance-qualified suffix explicitly.
///
/// For genuinely cross-node *publishing* (shared buses, well-known endpoints),
/// use the `*_absolute` publisher variants to opt out of auto-scoping.
pub struct NodeContext {
    pub session: Arc<zenoh::Session>,
    pub machine_id: String,
    /// Per-instance name (from config `name` field, or the node type name).
    /// Used to scope every data, health, and schema topic this node publishes.
    pub instance_name: String,
    pub shutdown_rx: tokio::sync::watch::Receiver<()>,
    /// Per-topic output liveness: `declared_at_ns`, `ever_fired`, `still_live`.
    /// Surfaced by the manifest queryable so the dataflow graph emerges from
    /// real wire usage rather than config drift. Conditional publishers behind
    /// unfired branches still appear with `ever_fired=false`; undeclared
    /// publishers keep their entry with `still_live=false`.
    pub(crate) outputs: Arc<Mutex<BTreeMap<String, Liveness>>>,
    /// Per-topic input liveness — mirror of [`outputs`](Self::outputs) for
    /// subscribers.
    pub(crate) inputs: Arc<Mutex<BTreeMap<String, Liveness>>>,
}

/// Strip the `bubbaloop/{global|local}/{machine_id}/` prefix from a fully
/// qualified key, leaving the absolute suffix the manifest tracks.
///
/// Returns `None` if the key does not match the expected layout.
fn strip_topic_prefix(key: &str, machine_id: &str) -> Option<String> {
    let global_prefix = format!("bubbaloop/global/{}/", machine_id);
    let local_prefix = format!("bubbaloop/local/{}/", machine_id);
    key.strip_prefix(&global_prefix)
        .or_else(|| key.strip_prefix(&local_prefix))
        .map(|s| s.to_string())
}

pub(crate) fn build_default_schema_uri(
    _instance_name: &str,
    _machine_id: &str,
    topic_suffix: &str,
    schema_version: u32,
) -> String {
    format!("bubbaloop://{}@v{}", topic_suffix, schema_version)
}

fn now_ns() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

impl NodeContext {
    /// Build a global topic auto-scoped under this node's instance name:
    /// `bubbaloop/global/{machine_id}/{instance_name}/{suffix}`.
    pub fn topic(&self, suffix: &str) -> String {
        format!(
            "bubbaloop/global/{}/{}/{}",
            self.machine_id, self.instance_name, suffix
        )
    }

    /// Build a machine-local topic auto-scoped under this node's instance name.
    pub fn local_topic(&self, suffix: &str) -> String {
        format!(
            "bubbaloop/local/{}/{}/{}",
            self.machine_id, self.instance_name, suffix
        )
    }

    /// Build an absolute global topic (no instance-name auto-scoping).
    pub fn absolute_topic(&self, suffix: &str) -> String {
        format!("bubbaloop/global/{}/{}", self.machine_id, suffix)
    }

    /// Build an absolute machine-local topic (no instance-name auto-scoping).
    pub fn absolute_local_topic(&self, suffix: &str) -> String {
        format!("bubbaloop/local/{}/{}", self.machine_id, suffix)
    }

    /// Register `key` as an output. Returns the stripped absolute suffix, or
    /// `None` if the key does not match this node's `machine_id` prefix.
    pub(crate) fn declare_output(&self, key: &str) -> Option<String> {
        let sfx = strip_topic_prefix(key, &self.machine_id)?;
        let mut guard = self.outputs.lock().expect("outputs mutex poisoned");
        guard
            .entry(sfx.clone())
            .and_modify(|l| l.still_live = true)
            .or_insert_with(|| Liveness::new(now_ns()));
        Some(sfx)
    }

    pub(crate) fn declare_input(&self, key: &str) -> Option<String> {
        let sfx = strip_topic_prefix(key, &self.machine_id)?;
        let mut guard = self.inputs.lock().expect("inputs mutex poisoned");
        guard
            .entry(sfx.clone())
            .and_modify(|l| l.still_live = true)
            .or_insert_with(|| Liveness::new(now_ns()));
        Some(sfx)
    }

    /// Snapshot outputs as an ordered list of wire entries.
    pub fn outputs_snapshot(&self) -> Vec<IoEntry> {
        crate::manifest::snapshot_entries(&self.outputs.lock().expect("outputs mutex poisoned"))
    }

    /// Snapshot inputs as an ordered list of wire entries.
    pub fn inputs_snapshot(&self) -> Vec<IoEntry> {
        crate::manifest::snapshot_entries(&self.inputs.lock().expect("inputs mutex poisoned"))
    }

    fn resolve_topic(&self, suffix: &str, local: bool) -> String {
        if local {
            self.local_topic(suffix)
        } else {
            self.topic(suffix)
        }
    }

    fn resolve_absolute_topic(&self, suffix: &str, local: bool) -> String {
        if local {
            self.absolute_local_topic(suffix)
        } else {
            self.absolute_topic(suffix)
        }
    }

    /// Compose `bubbaloop://{topic_suffix}@v{schema_version}` (topic_suffix already
    /// contains the instance prefix via `declare_output`).
    pub(crate) fn default_schema_uri(&self, topic_suffix: &str, schema_version: u32) -> String {
        build_default_schema_uri(
            &self.instance_name,
            &self.machine_id,
            topic_suffix,
            schema_version,
        )
    }

    // ── Publishers (auto-scoped under instance_name) ─────────────────────────

    /// Create a JSON publisher with `APPLICATION_JSON` encoding.
    ///
    /// Wraps every payload in the SDK's `{header, body}` provenance envelope
    /// (identical shape to CBOR). Default `schema_uri` is
    /// `bubbaloop://{instance}/{suffix}@v1`.
    pub async fn publisher_json(&self, suffix: &str) -> Result<crate::publisher::JsonPublisher> {
        self.publisher_json_with_schema(suffix, None, 1).await
    }

    /// Like [`publisher_json`](Self::publisher_json), with caller-controlled
    /// `schema_uri`/`schema_version`. Pass `Some("")` to publish blank URIs
    /// for consumers that rely on the old default.
    pub async fn publisher_json_with_schema(
        &self,
        suffix: &str,
        schema_uri: Option<&str>,
        schema_version: u32,
    ) -> Result<crate::publisher::JsonPublisher> {
        let key = self.topic(suffix);
        let sfx = self.declare_output(&key);
        let topic_hint = sfx.clone().unwrap_or_else(|| suffix.to_string());
        let uri = schema_uri
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.default_schema_uri(&topic_hint, schema_version));
        let pub_ = crate::publisher::JsonPublisher::new(
            &self.session,
            &key,
            self.instance_name.clone(),
            uri,
            self.outputs.clone(),
            sfx,
        )
        .await?;
        Ok(pub_)
    }

    /// Create a CBOR publisher with `APPLICATION_CBOR` encoding.
    pub async fn publisher_cbor(&self, suffix: &str) -> Result<crate::publisher::CborPublisher> {
        self.publisher_cbor_with_schema(suffix, None, 1).await
    }

    /// Like [`publisher_cbor`](Self::publisher_cbor), with caller-controlled
    /// `schema_uri`/`schema_version`. `schema_uri=Some("")` is respected as
    /// an explicit blank override.
    pub async fn publisher_cbor_with_schema(
        &self,
        suffix: &str,
        schema_uri: Option<&str>,
        schema_version: u32,
    ) -> Result<crate::publisher::CborPublisher> {
        let key = self.topic(suffix);
        let sfx = self.declare_output(&key);
        let topic_hint = sfx.clone().unwrap_or_else(|| suffix.to_string());
        let uri = schema_uri
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.default_schema_uri(&topic_hint, schema_version));
        crate::publisher::CborPublisher::new(
            &self.session,
            &key,
            self.instance_name.clone(),
            uri,
            self.outputs.clone(),
            sfx,
        )
        .await
    }

    /// Create a CBOR publisher backed by a pre-allocated shared-memory pool.
    pub async fn publisher_cbor_shm(
        &self,
        suffix: &str,
        slot_count: usize,
        slot_size: usize,
    ) -> Result<crate::publisher::CborPublisherShm> {
        self.publisher_cbor_shm_with_schema(suffix, slot_count, slot_size, None, 1)
            .await
    }

    /// Like [`publisher_cbor_shm`](Self::publisher_cbor_shm), with caller-controlled
    /// `schema_uri`/`schema_version`.
    pub async fn publisher_cbor_shm_with_schema(
        &self,
        suffix: &str,
        slot_count: usize,
        slot_size: usize,
        schema_uri: Option<&str>,
        schema_version: u32,
    ) -> Result<crate::publisher::CborPublisherShm> {
        let key = self.local_topic(suffix);
        let sfx = self.declare_output(&key);
        let topic_hint = sfx.clone().unwrap_or_else(|| suffix.to_string());
        let uri = schema_uri
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.default_schema_uri(&topic_hint, schema_version));
        crate::publisher::CborPublisherShm::new(
            &self.session,
            &key,
            slot_count,
            slot_size,
            self.instance_name.clone(),
            uri,
            self.outputs.clone(),
            sfx,
        )
        .await
    }

    /// Create a raw publisher that sends [`ZBytes`](zenoh::bytes::ZBytes) with no encoding.
    pub async fn publisher_raw(
        &self,
        suffix: &str,
        local: bool,
    ) -> Result<crate::publisher::RawPublisher> {
        let key = self.resolve_topic(suffix, local);
        let sfx = self.declare_output(&key);
        crate::publisher::RawPublisher::new(
            &self.session,
            &key,
            local,
            self.outputs.clone(),
            sfx,
        )
        .await
    }

    // ── Publishers (absolute, NOT auto-scoped) ───────────────────────────────

    /// Escape-hatch JSON publisher that skips `instance_name` scoping.
    pub async fn publisher_json_absolute(
        &self,
        absolute_suffix: &str,
    ) -> Result<crate::publisher::JsonPublisher> {
        let key = self.absolute_topic(absolute_suffix);
        let sfx = self.declare_output(&key);
        let topic_hint = sfx
            .clone()
            .unwrap_or_else(|| absolute_suffix.to_string());
        let uri = self.default_schema_uri(&topic_hint, 1);
        crate::publisher::JsonPublisher::new(
            &self.session,
            &key,
            self.instance_name.clone(),
            uri,
            self.outputs.clone(),
            sfx,
        )
        .await
    }

    /// Escape-hatch CBOR publisher that skips `instance_name` scoping.
    pub async fn publisher_cbor_absolute(
        &self,
        absolute_suffix: &str,
    ) -> Result<crate::publisher::CborPublisher> {
        self.publisher_cbor_absolute_with_schema(absolute_suffix, None, 1)
            .await
    }

    /// Escape-hatch CBOR publisher (absolute key) with caller-controlled schema.
    pub async fn publisher_cbor_absolute_with_schema(
        &self,
        absolute_suffix: &str,
        schema_uri: Option<&str>,
        schema_version: u32,
    ) -> Result<crate::publisher::CborPublisher> {
        let key = self.absolute_topic(absolute_suffix);
        let sfx = self.declare_output(&key);
        let topic_hint = sfx
            .clone()
            .unwrap_or_else(|| absolute_suffix.to_string());
        let uri = schema_uri
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.default_schema_uri(&topic_hint, schema_version));
        crate::publisher::CborPublisher::new(
            &self.session,
            &key,
            self.instance_name.clone(),
            uri,
            self.outputs.clone(),
            sfx,
        )
        .await
    }

    /// Escape-hatch SHM CBOR publisher that skips `instance_name` scoping.
    pub async fn publisher_cbor_shm_absolute(
        &self,
        absolute_suffix: &str,
        slot_count: usize,
        slot_size: usize,
    ) -> Result<crate::publisher::CborPublisherShm> {
        self.publisher_cbor_shm_absolute_with_schema(
            absolute_suffix,
            slot_count,
            slot_size,
            None,
            1,
        )
        .await
    }

    /// Escape-hatch SHM CBOR publisher (absolute key) with caller-controlled schema.
    pub async fn publisher_cbor_shm_absolute_with_schema(
        &self,
        absolute_suffix: &str,
        slot_count: usize,
        slot_size: usize,
        schema_uri: Option<&str>,
        schema_version: u32,
    ) -> Result<crate::publisher::CborPublisherShm> {
        let key = self.absolute_local_topic(absolute_suffix);
        let sfx = self.declare_output(&key);
        let topic_hint = sfx
            .clone()
            .unwrap_or_else(|| absolute_suffix.to_string());
        let uri = schema_uri
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.default_schema_uri(&topic_hint, schema_version));
        crate::publisher::CborPublisherShm::new(
            &self.session,
            &key,
            slot_count,
            slot_size,
            self.instance_name.clone(),
            uri,
            self.outputs.clone(),
            sfx,
        )
        .await
    }

    /// Escape-hatch raw publisher that skips `instance_name` scoping.
    pub async fn publisher_raw_absolute(
        &self,
        absolute_suffix: &str,
        local: bool,
    ) -> Result<crate::publisher::RawPublisher> {
        let key = self.resolve_absolute_topic(absolute_suffix, local);
        let sfx = self.declare_output(&key);
        crate::publisher::RawPublisher::new(
            &self.session,
            &key,
            local,
            self.outputs.clone(),
            sfx,
        )
        .await
    }

    // ── Subscribers (absolute by default) ────────────────────────────────────

    /// Create a raw subscriber that yields [`ZBytes`](zenoh::bytes::ZBytes) with no decoding.
    pub async fn subscriber_raw(
        &self,
        absolute_suffix: &str,
        local: bool,
    ) -> Result<crate::subscriber::RawSubscriber> {
        let key = self.resolve_absolute_topic(absolute_suffix, local);
        let sfx = self.declare_input(&key);
        crate::subscriber::RawSubscriber::new(&self.session, &key, self.inputs.clone(), sfx).await
    }

    /// Create a typed CBOR subscriber that auto-decodes the SDK provenance envelope.
    pub async fn subscriber_cbor<T: serde::de::DeserializeOwned>(
        &self,
        absolute_suffix: &str,
        local: bool,
    ) -> Result<crate::subscriber::CborSubscriber<T>> {
        let key = self.resolve_absolute_topic(absolute_suffix, local);
        let sfx = self.declare_input(&key);
        crate::subscriber::CborSubscriber::<T>::new(
            &self.session,
            &key,
            self.inputs.clone(),
            sfx,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topic_scopes_under_instance_name() {
        let built = format!("bubbaloop/global/{}/{}/{}", "jetson_01", "tapo_entrance", "compressed");
        assert_eq!(built, "bubbaloop/global/jetson_01/tapo_entrance/compressed");
    }

    #[test]
    fn strip_prefix_global() {
        let s = super::strip_topic_prefix("bubbaloop/global/jetson_01/tapo/raw", "jetson_01");
        assert_eq!(s.as_deref(), Some("tapo/raw"));
    }

    #[test]
    fn strip_prefix_local() {
        let s = super::strip_topic_prefix("bubbaloop/local/jetson_01/tapo/raw", "jetson_01");
        assert_eq!(s.as_deref(), Some("tapo/raw"));
    }

    #[test]
    fn strip_prefix_wrong_machine_returns_none() {
        let s = super::strip_topic_prefix("bubbaloop/global/other/x", "jetson_01");
        assert_eq!(s, None);
    }

    #[test]
    fn default_schema_uri_uses_topic_suffix() {
        let uri = super::build_default_schema_uri("emb", "bot", "emb/embeddings", 1);
        assert_eq!(uri, "bubbaloop://emb/embeddings@v1");
    }

    #[test]
    fn default_schema_uri_versioned() {
        let uri = super::build_default_schema_uri("", "bot", "emb/x", 2);
        assert_eq!(uri, "bubbaloop://emb/x@v2");
    }
}
