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
    ///
    /// Use for data that is visible network-wide (dashboards, other machines).
    pub fn topic(&self, suffix: &str) -> String {
        format!("bubbaloop/{}/{}/{}", self.scope, self.machine_id, suffix)
    }

    /// Build a machine-local topic: `local/{machine_id}/{suffix}`
    ///
    /// Use for data that must stay on this machine — e.g. raw RGBA frames passed
    /// from a camera node to a detector via SHM. These topics are outside
    /// `bubbaloop/**` and never cross the WebSocket bridge.
    pub fn local_topic(&self, suffix: &str) -> String {
        format!("local/{}/{}", self.machine_id, suffix)
    }

    // ── Global publishers (network-visible, bubbaloop/** key space) ──────────

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

    // ── Local publisher/subscriber (SHM, machine-local key space) ───────────

    /// Create a local publisher that sends raw [`ZBytes`](zenoh::bytes::ZBytes) to
    /// `local/{machine_id}/{suffix}` with SHM zero-copy.
    ///
    /// Payload never leaves the machine. Counterpart: [`subscriber_local`](Self::subscriber_local).
    pub async fn publisher_local(&self, suffix: &str) -> Result<crate::publisher::RawPublisher> {
        crate::publisher::RawPublisher::new(&self.session, &self.local_topic(suffix)).await
    }

    /// Create a local subscriber that receives raw [`ZBytes`](zenoh::bytes::ZBytes) from
    /// `local/{machine_id}/{suffix}` with SHM zero-copy.
    ///
    /// Uses a small FIFO (4 slots) — older frames are dropped when the consumer is slow.
    /// Counterpart: [`publisher_local`](Self::publisher_local).
    pub async fn subscriber_local(&self, suffix: &str) -> Result<crate::subscriber::RawSubscriber> {
        crate::subscriber::RawSubscriber::new(&self.session, &self.local_topic(suffix)).await
    }

    // ── Global subscribers (network-visible, bubbaloop/** key space) ─────────

    /// Create a typed subscriber that auto-decodes protobuf messages.
    pub async fn subscriber<T>(&self, suffix: &str) -> Result<crate::subscriber::TypedSubscriber<T>>
    where
        T: prost::Message + Default,
    {
        crate::subscriber::TypedSubscriber::new(&self.session, &self.topic(suffix)).await
    }

    /// Create a raw subscriber that yields [`ZBytes`](zenoh::bytes::ZBytes) with no decoding.
    ///
    /// Uses a small FIFO (4 slots) — older frames are dropped when the consumer is slow.
    pub async fn subscriber_raw(&self, suffix: &str) -> Result<crate::subscriber::RawSubscriber> {
        crate::subscriber::RawSubscriber::new(&self.session, &self.topic(suffix)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ctx(scope: &str, machine_id: &str) -> String {
        format!("bubbaloop/{}/{}/sensor/data", scope, machine_id)
    }

    #[test]
    fn topic_format() {
        assert_eq!(
            make_ctx("prod", "jetson_01"),
            "bubbaloop/prod/jetson_01/sensor/data"
        );
    }

    #[test]
    fn local_topic_format() {
        let machine_id = "jetson_01";
        let result = format!("local/{}/tapo_terrace/raw", machine_id);
        assert_eq!(result, "local/jetson_01/tapo_terrace/raw");
    }
}
