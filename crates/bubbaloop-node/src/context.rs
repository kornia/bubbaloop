use std::sync::Arc;

use crate::error::Result;

/// Context provided to nodes by the SDK runtime.
pub struct NodeContext {
    pub session: Arc<zenoh::Session>,
    pub scope: String,
    pub machine_id: String,
    /// Per-instance name (from config `name` field, or the node type name).
    /// Ensures multi-instance deployments don't collide on health/schema topics.
    pub instance_name: String,
    pub shutdown_rx: tokio::sync::watch::Receiver<()>,
}

impl NodeContext {
    /// Build a global scoped topic: `bubbaloop/{scope}/{machine_id}/{suffix}`
    pub fn topic(&self, suffix: &str) -> String {
        format!("bubbaloop/{}/{}/{}", self.scope, self.machine_id, suffix)
    }

    /// Build a machine-local topic: `local/{machine_id}/{suffix}`
    ///
    /// Local topics never cross the WebSocket bridge — use for SHM-only data
    /// (e.g. raw RGBA frames from a camera node to a detector on the same machine).
    pub fn local_topic(&self, suffix: &str) -> String {
        format!("local/{}/{}", self.machine_id, suffix)
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

    /// Create a raw publisher that sends [`ZBytes`](zenoh::bytes::ZBytes) with no encoding.
    ///
    /// When `local = true`, publishes to `local/{machine_id}/{suffix}` — SHM zero-copy,
    /// never crosses the WebSocket bridge. Use this for large binary payloads (e.g. RGBA
    /// frames) that only need to reach a consumer on the same machine.
    ///
    /// When `local = false` (default), publishes to `bubbaloop/{scope}/{machine_id}/{suffix}`.
    pub async fn publisher_raw(
        &self,
        suffix: &str,
        local: bool,
    ) -> Result<crate::publisher::RawPublisher> {
        crate::publisher::RawPublisher::new(&self.session, &self.resolve_topic(suffix, local), local)
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
    /// When `local = false` (default), subscribes to `bubbaloop/{scope}/{machine_id}/{suffix}`.
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
    fn topic_format() {
        let (scope, machine_id, suffix) = ("prod", "jetson_01", "camera/front/compressed");
        assert_eq!(
            format!("bubbaloop/{}/{}/{}", scope, machine_id, suffix),
            "bubbaloop/prod/jetson_01/camera/front/compressed"
        );
    }

    #[test]
    fn local_topic_format() {
        assert_eq!(
            format!("local/{}/{}", "jetson_01", "tapo_terrace/raw"),
            "local/jetson_01/tapo_terrace/raw"
        );
    }
}
