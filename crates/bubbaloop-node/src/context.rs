use std::sync::Arc;

use crate::error::Result;

/// Context provided to nodes by the SDK runtime.
pub struct NodeContext {
    pub session: Arc<zenoh::Session>,
    pub machine_id: String,
    /// Per-instance name (from config `name` field, or the node type name).
    /// Ensures multi-instance deployments don't collide on health/schema topics.
    pub instance_name: String,
    pub shutdown_rx: tokio::sync::watch::Receiver<()>,
}

impl NodeContext {
    /// Build a global topic: `bubbaloop/global/{machine_id}/{suffix}`
    ///
    /// Visible across the network — subscribed to by the dashboard and other machines.
    pub fn topic(&self, suffix: &str) -> String {
        format!("bubbaloop/global/{}/{}", self.machine_id, suffix)
    }

    /// Build a machine-local topic: `bubbaloop/local/{machine_id}/{suffix}`
    ///
    /// SHM-only, never crosses the WebSocket bridge. Use for large binary payloads
    /// (e.g. raw RGBA frames) consumed only by processes on the same machine.
    pub fn local_topic(&self, suffix: &str) -> String {
        format!("bubbaloop/local/{}/{}", self.machine_id, suffix)
    }

    fn resolve_topic(&self, suffix: &str, local: bool) -> String {
        if local {
            self.local_topic(suffix)
        } else {
            self.topic(suffix)
        }
    }

    // ── Publishers ───────────────────────────────────────────────────────────

    /// Create a protobuf publisher with `APPLICATION_PROTOBUF` encoding.
    pub async fn publisher_proto<T>(
        &self,
        suffix: &str,
    ) -> Result<crate::publisher::ProtoPublisher<T>>
    where
        T: prost::Message + Default + crate::MessageTypeName,
    {
        crate::publisher::ProtoPublisher::new(&self.session, &self.topic(suffix)).await
    }

    /// Create a JSON publisher with `APPLICATION_JSON` encoding.
    pub async fn publisher_json(&self, suffix: &str) -> Result<crate::publisher::JsonPublisher> {
        crate::publisher::JsonPublisher::new(&self.session, &self.topic(suffix)).await
    }

    /// Create a CBOR publisher with `APPLICATION_CBOR` encoding.
    ///
    /// Serializes values via `ciborium` into a heap `Vec<u8>` before publishing.
    /// Suitable for small structured messages (telemetry, config, events) on
    /// global (network-visible) topics.
    ///
    /// For large, hot-path binary payloads on the same machine, use
    /// [`publisher_cbor_shm`](Self::publisher_cbor_shm) instead — it encodes
    /// directly into shared memory without a heap allocation per message.
    pub async fn publisher_cbor(&self, suffix: &str) -> Result<crate::publisher::CborPublisher> {
        crate::publisher::CborPublisher::new(&self.session, &self.topic(suffix)).await
    }

    /// Create a CBOR publisher backed by a pre-allocated shared-memory pool.
    ///
    /// Each [`put`](crate::publisher::CborPublisherShm::put) allocates a slot,
    /// encodes the value directly into the mmap'd page via `ciborium`, and
    /// ships the SHM handle as a `ZBytes` payload — no heap `Vec`, no
    /// intermediate copy.
    ///
    /// The topic is always machine-local (`bubbaloop/local/{machine_id}/{suffix}`)
    /// because Zenoh SHM cannot cross machines. Congestion control is `Block`,
    /// so the publisher waits for a consumer to free a slot instead of dropping
    /// messages.
    ///
    /// **Parameters:**
    /// - `slot_count`: number of in-flight slots. 4 is a reasonable default for
    ///   single-consumer topics; raise if the consumer is slow.
    /// - `slot_size`: bytes per slot — must fit the largest single CBOR-encoded
    ///   message. Undersize → `put()` returns an error; oversize → wasted RAM.
    pub async fn publisher_cbor_shm(
        &self,
        suffix: &str,
        slot_count: usize,
        slot_size: usize,
    ) -> Result<crate::publisher::CborPublisherShm> {
        crate::publisher::CborPublisherShm::new(
            &self.session,
            &self.local_topic(suffix),
            slot_count,
            slot_size,
        )
        .await
    }

    /// Create a raw publisher that sends [`ZBytes`](zenoh::bytes::ZBytes) with no encoding.
    ///
    /// When `local = true`, publishes to `local/{machine_id}/{suffix}` — SHM zero-copy,
    /// never crosses the WebSocket bridge. Use this for large binary payloads (e.g. RGBA
    /// frames) that only need to reach a consumer on the same machine.
    ///
    /// When `local = false` (default), publishes to `bubbaloop/global/{machine_id}/{suffix}`.
    pub async fn publisher_raw(
        &self,
        suffix: &str,
        local: bool,
    ) -> Result<crate::publisher::RawPublisher> {
        crate::publisher::RawPublisher::new(&self.session, &self.resolve_topic(suffix, local), local)
            .await
    }

    /// Create a raw SHM publisher that tags payloads with `APPLICATION_PROTOBUF` encoding.
    ///
    /// Like [`publisher_raw`](Self::publisher_raw) but sets the protobuf encoding header
    /// so subscribers can auto-decode the payload by type name. Use this when you manually
    /// serialize a proto into an SHM buffer and want schema-aware subscribers to decode it.
    ///
    /// Always local (`local/{machine_id}/{suffix}`) with `CongestionControl::Block`.
    pub async fn publisher_raw_proto<T>(
        &self,
        suffix: &str,
    ) -> Result<crate::publisher::RawPublisher>
    where
        T: prost::Message + Default + crate::MessageTypeName,
    {
        let encoding =
            zenoh::bytes::Encoding::APPLICATION_PROTOBUF.with_schema(T::type_name());
        crate::publisher::RawPublisher::with_encoding(
            &self.session,
            &self.local_topic(suffix),
            true,
            Some(encoding),
        )
        .await
    }

    // ── Subscribers ──────────────────────────────────────────────────────────

    /// Create a typed subscriber that auto-decodes protobuf messages.
    pub async fn subscriber<T>(&self, suffix: &str) -> Result<crate::subscriber::TypedSubscriber<T>>
    where
        T: prost::Message + Default,
    {
        crate::subscriber::TypedSubscriber::new(&self.session, &self.topic(suffix)).await
    }

    /// Create a raw subscriber that yields [`ZBytes`](zenoh::bytes::ZBytes) with no decoding.
    ///
    /// When `local = true`, subscribes to `local/{machine_id}/{suffix}` — SHM zero-copy,
    /// machine-local only. Counterpart to `publisher_raw(suffix, true)`.
    ///
    /// When `local = false` (default), subscribes to `bubbaloop/global/{machine_id}/{suffix}`.
    ///
    /// Uses a small FIFO (4 slots) — older frames are dropped when the consumer is slow.
    pub async fn subscriber_raw(
        &self,
        suffix: &str,
        local: bool,
    ) -> Result<crate::subscriber::RawSubscriber> {
        crate::subscriber::RawSubscriber::new(&self.session, &self.resolve_topic(suffix, local))
            .await
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn topic_builds_global_key() {
        let machine_id = "jetson_01";
        let suffix = "camera/front/compressed";
        assert_eq!(
            format!("bubbaloop/global/{}/{}", machine_id, suffix),
            "bubbaloop/global/jetson_01/camera/front/compressed"
        );
    }

    #[test]
    fn local_topic_builds_local_key() {
        let machine_id = "jetson_01";
        let suffix = "tapo_terrace/raw";
        assert_eq!(
            format!("bubbaloop/local/{}/{}", machine_id, suffix),
            "bubbaloop/local/jetson_01/tapo_terrace/raw"
        );
    }

    #[test]
    fn global_and_local_share_machine_id() {
        let machine_id = "edge_42";
        let suffix = "sensor/data";
        let global = format!("bubbaloop/global/{}/{}", machine_id, suffix);
        let local = format!("bubbaloop/local/{}/{}", machine_id, suffix);
        assert!(global.starts_with("bubbaloop/global/"));
        assert!(local.starts_with("bubbaloop/local/"));
        assert_eq!(
            global.strip_prefix("bubbaloop/global/"),
            local.strip_prefix("bubbaloop/local/"),
        );
    }
}
