//! Sink trait — data-consuming nodes that subscribe to Zenoh topics and act on data.
//!
//! Implement `Sink` for a data consumer (e.g., actuator, logger, alerter).
//! The SDK handles: session setup, health heartbeat, config loading,
//! schema registration, topic construction, and the subscribe→execute loop.

use crate::NodeContext;
use serde::de::DeserializeOwned;

/// Trait for data-consuming nodes.
///
/// A Sink subscribes to a Zenoh topic and processes each incoming message.
/// The SDK runs the subscriber loop automatically; the node author only
/// implements `init()` and `execute()`.
#[async_trait::async_trait]
pub trait Sink: Send + Sync + 'static {
    /// Node-specific configuration type (deserialized from YAML).
    type Config: DeserializeOwned + Send + Sync + 'static;

    /// Human-readable node name (must match `name` in `node.yaml`).
    fn name() -> &'static str;

    /// Protobuf FileDescriptorSet bytes for schema registration.
    fn descriptor() -> &'static [u8];

    /// Input topic suffix to subscribe to (e.g., `"temp-sensor/data"`).
    /// Full topic: `bubbaloop/{scope}/{machine_id}/{input_topic}`.
    fn input_topic() -> &'static str;

    /// Initialize the sink after Zenoh session is established and config is loaded.
    async fn init(ctx: &NodeContext, config: &Self::Config) -> anyhow::Result<Self>
    where
        Self: Sized;

    /// Process an incoming message. Return `Some(json)` to publish a response,
    /// `None` to consume silently.
    async fn execute(
        &mut self,
        ctx: &NodeContext,
        input: serde_json::Value,
    ) -> anyhow::Result<Option<serde_json::Value>>;
}

/// Run a Sink node with the full SDK runtime.
///
/// Handles: logging, CLI args, config, Zenoh session, schema queryable,
/// health heartbeat, and the `subscribe → execute` loop with graceful shutdown.
pub async fn run_sink<S: Sink>() -> anyhow::Result<()> {
    // 1. Init logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // 2. Parse CLI args
    let args: crate::SdkArgs = argh::from_env();

    // 3. Load config
    let node_config: S::Config = crate::config::load_config(&args.config)?;
    log::info!(
        "{}: config loaded from {}",
        S::name(),
        args.config.display()
    );

    // 4. Resolve scope + machine_id
    let scope = std::env::var("BUBBALOOP_SCOPE").unwrap_or_else(|_| "local".to_string());
    let machine_id = std::env::var("BUBBALOOP_MACHINE_ID")
        .unwrap_or_else(|_| {
            hostname::get()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_else(|_| "unknown".to_string())
        })
        .replace('-', "_");

    // 5. Setup shutdown channel
    let (shutdown_tx, _) = crate::shutdown::setup_shutdown()?;

    // 6. Open Zenoh session
    let session = crate::zenoh_session::open_zenoh_session(&args.endpoint).await?;

    // 7. Declare schema queryable
    let _schema_queryable = crate::schema::declare_schema_queryable(
        &session,
        &scope,
        &machine_id,
        S::name(),
        S::descriptor(),
    )
    .await?;

    // 8. Spawn health heartbeat
    let _health_handle = crate::health::spawn_health_heartbeat(
        session.clone(),
        &scope,
        &machine_id,
        S::name(),
        shutdown_tx.subscribe(),
    )
    .await?;

    // 9. Build context
    let ctx = NodeContext {
        session: session.clone(),
        scope,
        machine_id,
        shutdown_rx: shutdown_tx.subscribe(),
    };

    // 10. Init sink
    let mut sink = S::init(&ctx, &node_config).await?;
    log::info!("{} sink initialized", S::name());

    // 11. Subscribe loop
    let input_topic = ctx.topic(S::input_topic());
    let output_topic = ctx.topic(&format!("{}/data", S::name()));

    let subscriber = session
        .declare_subscriber(&input_topic)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to subscribe to '{}': {}", input_topic, e))?;

    let mut shutdown_rx = ctx.shutdown_rx.clone();

    loop {
        tokio::select! {
            biased;
            _ = shutdown_rx.changed() => {
                log::info!("{} sink shutting down", S::name());
                break;
            }
            sample = subscriber.recv_async() => {
                match sample {
                    Ok(sample) => {
                        let bytes = sample.payload().to_bytes();
                        let input: serde_json::Value = match serde_json::from_slice(&bytes) {
                            Ok(v) => v,
                            Err(_) => {
                                // Fallback: treat as string
                                let text = String::from_utf8_lossy(&bytes);
                                serde_json::Value::String(text.to_string())
                            }
                        };
                        match sink.execute(&ctx, input).await {
                            Ok(Some(output)) => {
                                if let Err(e) = session.put(&output_topic, output.to_string()).await {
                                    log::warn!("{}: publish failed: {}", S::name(), e);
                                }
                            }
                            Ok(None) => {}
                            Err(e) => {
                                log::warn!("{}: execute error: {}", S::name(), e);
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("{}: subscriber error: {}", S::name(), e);
                        break;
                    }
                }
            }
        }
    }

    log::info!("{} sink shut down", S::name());
    Ok(())
}
