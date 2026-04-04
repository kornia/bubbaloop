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

    /// Create a protobuf publisher with `APPLICATION_PROTOBUF` encoding and schema suffix.
    pub async fn publisher_proto<T>(&self, suffix: &str) -> Result<crate::publisher::ProtoPublisher<T>>
    where
        T: prost::Message + Default + crate::MessageTypeName,
    {
        crate::publisher::ProtoPublisher::new(&self.session, &self.topic(suffix)).await
    }

    /// Create a JSON publisher with `APPLICATION_JSON` encoding.
    pub async fn publisher_json(&self, suffix: &str) -> Result<crate::publisher::JsonPublisher> {
        crate::publisher::JsonPublisher::new(&self.session, &self.topic(suffix)).await
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

    /// Create a raw subscriber with a literal key expression (no scoped prefix).
    pub async fn subscriber_raw(&self, key_expr: &str) -> Result<crate::subscriber::RawSubscriber> {
        crate::subscriber::RawSubscriber::new(&self.session, key_expr).await
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
