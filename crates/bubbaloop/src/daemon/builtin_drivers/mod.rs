//! Built-in driver framework — lightweight drivers that run as async tasks inside the daemon.
//!
//! Built-in drivers handle common protocols (HTTP, TCP, UDP, MQTT, Modbus, cron, sysinfo, file-watch)
//! without needing a separate binary or systemd service. They publish to the same Zenoh topic
//! hierarchy as external nodes, so consumers see a uniform interface.

pub mod cron_task;
pub mod exec;
pub mod file_watch;
pub mod http_poll;
pub mod modbus;
pub mod mqtt;
pub mod system;
pub mod tcp_listen;
pub mod udp_listen;
pub mod webhook;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::watch;

#[derive(Error, Debug)]
pub enum DriverError {
    #[error("Driver start failed: {0}")]
    StartFailed(String),
    #[error("Configuration error: missing required param '{0}'")]
    MissingParam(String),
    #[error("Configuration error: {0}")]
    ConfigError(String),
    #[error("Already running: {0}")]
    AlreadyRunning(String),
    #[error("Not running: {0}")]
    NotRunning(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, DriverError>;

/// Configuration passed to a built-in driver at startup.
pub struct DriverConfig {
    pub session: Arc<zenoh::Session>,
    pub scope: String,
    pub machine_id: String,
    pub skill_name: String,
    pub params: HashMap<String, serde_yaml::Value>,
    pub shutdown_rx: watch::Receiver<()>,
}

impl DriverConfig {
    /// Standard data topic: `bubbaloop/{scope}/{machine_id}/{skill_name}/data`
    pub fn data_topic(&self) -> String {
        format!(
            "bubbaloop/{}/{}/{}/data",
            self.scope, self.machine_id, self.skill_name
        )
    }

    /// Standard health topic: `bubbaloop/{scope}/{machine_id}/health/{skill_name}`
    pub fn health_topic(&self) -> String {
        format!(
            "bubbaloop/{}/{}/health/{}",
            self.scope, self.machine_id, self.skill_name
        )
    }

    /// Extract a required string param from the config map.
    pub fn require_str(&self, key: &str) -> Result<String> {
        self.params
            .get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| DriverError::MissingParam(key.to_string()))
    }

    /// Extract an optional string param with a default.
    pub fn str_or(&self, key: &str, default: &str) -> String {
        self.params
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or(default)
            .to_string()
    }

    /// Extract an optional u64 param with a default.
    pub fn u64_or(&self, key: &str, default: u64) -> u64 {
        self.params
            .get(key)
            .and_then(|v| v.as_u64())
            .unwrap_or(default)
    }

    /// Extract an optional u16 param with a default.
    pub fn u16_or(&self, key: &str, default: u16) -> u16 {
        self.params
            .get(key)
            .and_then(|v| v.as_u64())
            .map(|v| v as u16)
            .unwrap_or(default)
    }

    /// Extract an optional bool param with a default.
    pub fn bool_or(&self, key: &str, default: bool) -> bool {
        self.params
            .get(key)
            .and_then(|v| v.as_bool())
            .unwrap_or(default)
    }
}

/// Trait that all built-in drivers implement.
#[async_trait::async_trait]
pub trait BuiltinDriver: Send + Sync + 'static {
    /// Human-readable driver name (e.g., "http-poll").
    fn name(&self) -> &'static str;

    /// Start the driver. Runs until shutdown_rx fires or an unrecoverable error occurs.
    async fn run(&self, config: DriverConfig) -> Result<()>;
}

/// Handle to a running built-in driver task.
pub struct DriverHandle {
    pub skill_name: String,
    pub driver_name: String,
    task: tokio::task::JoinHandle<()>,
    shutdown_tx: watch::Sender<()>,
}

impl DriverHandle {
    /// Stop the driver gracefully.
    pub async fn stop(self) {
        let _ = self.shutdown_tx.send(());
        let _ = self.task.await;
    }
}

/// Registry of running built-in driver instances.
pub struct DriverRegistry {
    handles: tokio::sync::Mutex<HashMap<String, DriverHandle>>,
}

impl Default for DriverRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl DriverRegistry {
    pub fn new() -> Self {
        Self {
            handles: tokio::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Start a built-in driver for a skill.
    pub async fn start_driver(
        &self,
        driver: Box<dyn BuiltinDriver>,
        session: Arc<zenoh::Session>,
        scope: String,
        machine_id: String,
        skill_name: String,
        params: HashMap<String, serde_yaml::Value>,
    ) -> Result<()> {
        let mut handles = self.handles.lock().await;
        if handles.contains_key(&skill_name) {
            return Err(DriverError::AlreadyRunning(skill_name));
        }

        let (shutdown_tx, shutdown_rx) = watch::channel(());
        let driver_name = driver.name().to_string();
        let driver_name_clone = driver_name.clone();
        let skill = skill_name.clone();

        let config = DriverConfig {
            session,
            scope,
            machine_id,
            skill_name: skill.clone(),
            params,
            shutdown_rx,
        };

        let task = tokio::spawn(async move {
            if let Err(e) = driver.run(config).await {
                log::error!(
                    "Built-in driver '{}' for skill '{}' failed: {}",
                    driver_name_clone,
                    skill,
                    e
                );
            }
        });

        handles.insert(
            skill_name.clone(),
            DriverHandle {
                skill_name,
                driver_name,
                task,
                shutdown_tx,
            },
        );

        Ok(())
    }

    /// Stop a running driver by skill name.
    pub async fn stop_driver(&self, skill_name: &str) -> Result<()> {
        let handle = {
            let mut handles = self.handles.lock().await;
            handles
                .remove(skill_name)
                .ok_or_else(|| DriverError::NotRunning(skill_name.to_string()))?
        };
        handle.stop().await;
        Ok(())
    }

    /// Check if a driver is running.
    pub async fn is_running(&self, skill_name: &str) -> bool {
        self.handles.lock().await.contains_key(skill_name)
    }

    /// List all running drivers as (skill_name, driver_name) pairs.
    pub async fn list_running(&self) -> Vec<(String, String)> {
        self.handles
            .lock()
            .await
            .values()
            .map(|h| (h.skill_name.clone(), h.driver_name.clone()))
            .collect()
    }

    /// Stop all drivers (for daemon shutdown).
    pub async fn stop_all(&self) {
        let handles: Vec<DriverHandle> = {
            let mut map = self.handles.lock().await;
            map.drain().map(|(_, h)| h).collect()
        };
        for handle in handles {
            handle.stop().await;
        }
    }
}

/// Factory function: create a driver instance by name.
pub fn create_driver(driver_name: &str) -> Option<Box<dyn BuiltinDriver>> {
    match driver_name {
        "http-poll" => Some(Box::new(http_poll::HttpPollDriver)),
        "webhook" => Some(Box::new(webhook::WebhookDriver)),
        "exec" => Some(Box::new(exec::ExecDriver)),
        "cron-task" => Some(Box::new(cron_task::CronTaskDriver)),
        "system" => Some(Box::new(system::SystemDriver)),
        "tcp-listen" => Some(Box::new(tcp_listen::TcpListenDriver)),
        "udp-listen" => Some(Box::new(udp_listen::UdpListenDriver)),
        "file-watch" => Some(Box::new(file_watch::FileWatchDriver)),
        "mqtt" => Some(Box::new(mqtt::MqttDriver)),
        "modbus" => Some(Box::new(modbus::ModbusDriver)),
        _ => None,
    }
}

/// Spawn a health heartbeat loop that publishes "ok" every 5 seconds.
pub async fn spawn_health_loop(
    session: Arc<zenoh::Session>,
    topic: String,
    mut shutdown_rx: watch::Receiver<()>,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    loop {
        tokio::select! {
            biased;
            _ = shutdown_rx.changed() => break,
            _ = interval.tick() => {
                if let Err(e) = session.put(&topic, "ok").await {
                    log::warn!("Health publish to '{}' failed: {}", topic, e);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_driver_known_names() {
        let names = [
            "http-poll",
            "webhook",
            "exec",
            "cron-task",
            "system",
            "tcp-listen",
            "udp-listen",
            "file-watch",
            "mqtt",
            "modbus",
        ];
        for name in names {
            assert!(
                create_driver(name).is_some(),
                "create_driver('{}') returned None",
                name
            );
        }
    }

    #[test]
    fn create_driver_unknown_returns_none() {
        assert!(create_driver("not-a-driver").is_none());
        assert!(create_driver("").is_none());
    }

    #[test]
    fn driver_config_param_helpers() {
        let mut params = HashMap::new();
        params.insert(
            "url".to_string(),
            serde_yaml::Value::String("http://example.com".to_string()),
        );
        params.insert(
            "interval_secs".to_string(),
            serde_yaml::Value::Number(serde_yaml::Number::from(30u64)),
        );
        params.insert("recursive".to_string(), serde_yaml::Value::Bool(true));

        // require_str
        assert_eq!(
            params.get("url").and_then(|v| v.as_str()).unwrap(),
            "http://example.com"
        );
        assert!(params.get("missing").and_then(|v| v.as_str()).is_none());

        // u64_or equivalent
        assert_eq!(
            params
                .get("interval_secs")
                .and_then(|v| v.as_u64())
                .unwrap_or(60),
            30
        );
        assert_eq!(
            params.get("missing").and_then(|v| v.as_u64()).unwrap_or(60),
            60
        );

        // bool_or equivalent
        assert!(params
            .get("recursive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false));
    }
}
