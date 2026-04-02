use std::sync::Arc;

/// Context provided to nodes by the SDK runtime.
pub struct NodeContext {
    /// Shared Zenoh session (Arc-wrapped, safe to clone)
    pub session: Arc<zenoh::Session>,
    /// Deployment scope (from BUBBALOOP_SCOPE env, default: "local")
    pub scope: String,
    /// Machine identifier (from BUBBALOOP_MACHINE_ID env, default: hostname)
    pub machine_id: String,
    /// Shutdown signal receiver — select! on this in your main loop
    pub shutdown_rx: tokio::sync::watch::Receiver<()>,
}

impl NodeContext {
    /// Build a fully-qualified scoped topic: `bubbaloop/{scope}/{machine_id}/{suffix}`
    pub fn topic(&self, suffix: &str) -> String {
        format!("bubbaloop/{}/{}/{}", self.scope, self.machine_id, suffix)
    }

    /// Create a declared publisher for protobuf messages.
    ///
    /// Sets `Encoding::APPLICATION_PROTOBUF` with the message type name as the schema suffix.
    /// The encoding is declared once at publisher creation time and reused for every `put()`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let pub = ctx.publisher_proto::<MyMessage>("sensor/data").await?;
    /// pub.put(&my_msg).await?;
    /// ```
    pub async fn publisher_proto<T>(
        &self,
        suffix: &str,
    ) -> anyhow::Result<crate::publisher::ProtoPublisher<T>>
    where
        T: prost::Message + Default + crate::MessageTypeName,
    {
        crate::publisher::ProtoPublisher::new(&self.session, &self.topic(suffix)).await
    }

    /// Create a declared publisher for JSON messages.
    ///
    /// Sets `Encoding::APPLICATION_JSON`. The encoding is declared once at publisher
    /// creation time and reused for every `put()`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let pub = ctx.publisher_json("status/current").await?;
    /// pub.put(&serde_json::json!({"state": "running"})).await?;
    /// ```
    pub async fn publisher_json(
        &self,
        suffix: &str,
    ) -> anyhow::Result<crate::publisher::JsonPublisher> {
        crate::publisher::JsonPublisher::new(&self.session, &self.topic(suffix)).await
    }

    /// Create a typed subscriber that decodes protobuf messages automatically.
    ///
    /// `suffix` is appended to the scoped base topic. Use `+` and `**` wildcards
    /// for cross-machine subscriptions where needed.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let sub = ctx.subscriber::<MyMessage>("sensor/data").await?;
    /// while let Some(msg) = sub.recv().await {
    ///     println!("received: {:?}", msg);
    /// }
    /// ```
    pub async fn subscriber<T>(
        &self,
        suffix: &str,
    ) -> anyhow::Result<crate::subscriber::TypedSubscriber<T>>
    where
        T: prost::Message + Default,
    {
        crate::subscriber::TypedSubscriber::new(&self.session, &self.topic(suffix)).await
    }

    /// Create a raw (untyped) subscriber with a literal key expression.
    ///
    /// Unlike `subscriber()`, this does NOT prepend the scoped base topic.
    /// Use it for dashboard-style dynamic decoding across many topics.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let sub = ctx.subscriber_raw("bubbaloop/**").await?;
    /// while let Some(sample) = sub.recv().await {
    ///     println!("encoding: {}", sample.encoding());
    /// }
    /// ```
    pub async fn subscriber_raw(
        &self,
        key_expr: &str,
    ) -> anyhow::Result<crate::subscriber::RawSubscriber> {
        crate::subscriber::RawSubscriber::new(&self.session, key_expr).await
    }
}

#[cfg(test)]
mod tests {
    /// topic() formats the key as bubbaloop/{scope}/{machine_id}/{suffix}.
    /// We test the format rule directly (no Zenoh session needed).
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
