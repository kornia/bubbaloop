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
    /// Build a fully-qualified scoped topic: `bubbaloop/{scope}/{machine_id}/{suffix}`
    pub fn topic(&self, suffix: &str) -> String {
        format!("bubbaloop/{}/{}/{}", self.scope, self.machine_id, suffix)
    }

    /// Build a machine-local topic: `local/{machine_id}/{suffix}`
    ///
    /// Use this for data that must stay on the same machine (e.g. SHM raw frames).
    /// These topics are NOT under `bubbaloop/**` and will never cross the WebSocket bridge.
    pub fn local_topic(&self, suffix: &str) -> String {
        format!("local/{}/{}", self.machine_id, suffix)
    }

    /// Create a protobuf publisher with `APPLICATION_PROTOBUF` encoding and schema suffix.
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
    /// The caller owns the byte layout. SHM zero-copy is used automatically
    /// when the session has it enabled and the subscriber is on the same machine.
    pub async fn publisher_raw(&self, suffix: &str) -> Result<crate::publisher::RawPublisher> {
        crate::publisher::RawPublisher::new(&self.session, &self.topic(suffix)).await
    }

    /// Create a raw publisher on a machine-local topic (`local/{machine_id}/{suffix}`).
    ///
    /// Identical to [`publisher_raw`](Self::publisher_raw) but uses [`local_topic`](Self::local_topic).
    /// Use this for SHM frame data that must never cross the WebSocket bridge.
    pub async fn publisher_raw_local(
        &self,
        suffix: &str,
    ) -> Result<crate::publisher::RawPublisher> {
        crate::publisher::RawPublisher::new(&self.session, &self.local_topic(suffix)).await
    }

    /// Create a typed subscriber that auto-decodes protobuf messages.
    ///
    /// `suffix` is appended to the scoped base topic.
    pub async fn subscriber<T>(&self, suffix: &str) -> Result<crate::subscriber::TypedSubscriber<T>>
    where
        T: prost::Message + Default,
    {
        crate::subscriber::TypedSubscriber::new(&self.session, &self.topic(suffix)).await
    }

    /// Create a raw subscriber that yields [`ZBytes`](zenoh::bytes::ZBytes) with no decoding.
    ///
    /// Counterpart to [`publisher_raw`](Self::publisher_raw). The caller decodes the bytes.
    /// Uses a small FIFO (4 slots) — older frames are dropped when the consumer is slow.
    pub async fn subscriber_raw(&self, suffix: &str) -> Result<crate::subscriber::RawSubscriber> {
        crate::subscriber::RawSubscriber::new(&self.session, &self.topic(suffix)).await
    }

    /// Create a raw subscriber on a machine-local topic (`local/{machine_id}/{suffix}`).
    ///
    /// Counterpart to [`publisher_raw_local`](Self::publisher_raw_local).
    pub async fn subscriber_raw_local(
        &self,
        suffix: &str,
    ) -> Result<crate::subscriber::RawSubscriber> {
        crate::subscriber::RawSubscriber::new(&self.session, &self.local_topic(suffix)).await
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn topic_format() {
        let scope = "prod";
        let machine_id = "jetson_01";
        let suffix = "camera/front/compressed";
        let result = format!("bubbaloop/{}/{}/{}", scope, machine_id, suffix);
        assert_eq!(result, "bubbaloop/prod/jetson_01/camera/front/compressed");
    }

    #[test]
    fn topic_format_local_scope() {
        let result = format!("bubbaloop/{}/{}/{}", "local", "my_host", "sensor/data");
        assert_eq!(result, "bubbaloop/local/my_host/sensor/data");
    }
}
