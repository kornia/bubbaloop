//! MQTT driver — subscribe to MQTT topics and publish messages to Zenoh.

use super::{spawn_health_loop, BuiltinDriver, DriverConfig, DriverError, Result};
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use std::time::Duration;

pub struct MqttDriver;

#[async_trait::async_trait]
impl BuiltinDriver for MqttDriver {
    fn name(&self) -> &'static str {
        "mqtt"
    }

    async fn run(&self, config: DriverConfig) -> Result<()> {
        let broker = config.require_str("broker")?;
        let mqtt_topic = config.require_str("topic")?;
        let qos_val = config.u64_or("qos", 1);
        let qos = match qos_val {
            0 => QoS::AtMostOnce,
            1 => QoS::AtLeastOnce,
            _ => QoS::ExactlyOnce,
        };

        // Parse broker URL: mqtt://host:port
        let broker_clean = broker
            .trim_start_matches("mqtt://")
            .trim_start_matches("tcp://");
        let (host, port) = if let Some((h, p)) = broker_clean.rsplit_once(':') {
            (h.to_string(), p.parse::<u16>().unwrap_or(1883))
        } else {
            (broker_clean.to_string(), 1883)
        };

        let data_topic = config.data_topic();
        let health_topic = config.health_topic();
        let session = config.session.clone();
        let mut shutdown_rx = config.shutdown_rx.clone();

        // Spawn health heartbeat
        let health_session = session.clone();
        let health_shutdown = config.shutdown_rx.clone();
        tokio::spawn(spawn_health_loop(
            health_session,
            health_topic,
            health_shutdown,
        ));

        let client_id = format!("bubbaloop-{}", config.skill_name);
        let mut mqttoptions = MqttOptions::new(&client_id, &host, port);
        mqttoptions.set_keep_alive(Duration::from_secs(30));

        let (client, mut eventloop) = AsyncClient::new(mqttoptions, 100);

        client
            .subscribe(&mqtt_topic, qos)
            .await
            .map_err(|e| DriverError::StartFailed(format!("MQTT subscribe: {}", e)))?;

        log::info!(
            "[mqtt] skill='{}' broker='{}:{}' topic='{}' qos={}",
            config.skill_name,
            host,
            port,
            mqtt_topic,
            qos_val
        );

        loop {
            tokio::select! {
                biased;
                _ = shutdown_rx.changed() => {
                    log::info!("[mqtt] '{}' shutting down", config.skill_name);
                    client.disconnect().await.ok();
                    break;
                }
                notification = eventloop.poll() => {
                    match notification {
                        Ok(Event::Incoming(Packet::Publish(publish))) => {
                            let payload_str = String::from_utf8_lossy(&publish.payload);
                            let payload = serde_json::json!({
                                "topic": publish.topic,
                                "payload": payload_str.as_ref(),
                                "qos": publish.qos as u8,
                            });
                            if let Err(e) = session.put(&data_topic, payload.to_string()).await {
                                log::warn!("[mqtt] publish to Zenoh failed: {}", e);
                            }
                        }
                        Ok(_) => {} // Ignore non-publish events (ConnAck, SubAck, etc.)
                        Err(e) => {
                            log::warn!("[mqtt] eventloop error: {}", e);
                            // Brief backoff before reconnect attempt
                            tokio::time::sleep(Duration::from_secs(5)).await;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn driver_name() {
        assert_eq!(MqttDriver.name(), "mqtt");
    }
}
