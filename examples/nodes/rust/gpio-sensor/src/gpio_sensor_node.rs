//! GPIO sensor node implementation.
//!
//! Publishes simulated pin readings over Zenoh as JSON. In production, swap
//! the `simulate_read()` call for actual GPIO reads — see the comment block below.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use rand::Rng;
use serde_json::json;
use tokio::sync::watch;
use zenoh::Session;

use crate::config::{Config, SensorType};

/// How often to publish a health heartbeat (independent of sensor rate).
const HEALTH_INTERVAL_SECS: u64 = 5;

pub struct GpioSensorNode {
    config: Config,
    session: Arc<Session>,
    /// Fully-qualified Zenoh topic for sensor readings.
    reading_topic: String,
    /// Fully-qualified Zenoh topic for health heartbeats.
    health_topic: String,
}

impl GpioSensorNode {
    pub fn new(
        session: Arc<Session>,
        config: Config,
        scope: &str,
        machine_id: &str,
    ) -> Result<Self> {
        let reading_topic = format!(
            "bubbaloop/{}/{}/{}",
            scope, machine_id, config.publish_topic
        );
        let health_topic = format!(
            "bubbaloop/{}/{}/gpio-sensor/{}/health",
            scope, machine_id, config.name
        );
        log::info!("Reading topic : {}", reading_topic);
        log::info!("Health topic  : {}", health_topic);

        Ok(Self {
            config,
            session,
            reading_topic,
            health_topic,
        })
    }

    /// Main sensor loop. Runs until the shutdown signal fires.
    pub async fn run(self, mut shutdown_rx: watch::Receiver<()>) -> Result<()> {
        let mut sample_interval =
            tokio::time::interval(Duration::from_secs_f64(self.config.interval_secs));
        let mut health_interval = tokio::time::interval(Duration::from_secs(HEALTH_INTERVAL_SECS));

        let reading_publisher = self
            .session
            .declare_publisher(&self.reading_topic)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to declare publisher: {}", e))?;

        let health_publisher = self
            .session
            .declare_publisher(&self.health_topic)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to declare health publisher: {}", e))?;

        log::info!(
            "gpio-sensor '{}' running (pin={}, type={}, interval={}s)",
            self.config.name,
            self.config.pin,
            self.config.sensor_type,
            self.config.interval_secs
        );

        loop {
            tokio::select! {
                _ = shutdown_rx.changed() => {
                    log::info!("gpio-sensor '{}': shutdown signal received", self.config.name);
                    break;
                }

                _ = sample_interval.tick() => {
                    let value = simulate_read(self.config.sensor_type, self.config.active_high);
                    let timestamp_ms = now_ms();

                    let payload = json!({
                        "pin": self.config.pin,
                        "sensor_type": self.config.sensor_type.to_string(),
                        "value": value,
                        "unit": self.config.sensor_type.unit(),
                        "active_high": self.config.active_high,
                        "timestamp_ms": timestamp_ms,
                    });

                    match serde_json::to_vec(&payload) {
                        Ok(bytes) => {
                            if let Err(e) = reading_publisher.put(bytes).await {
                                log::warn!("gpio-sensor '{}': publish failed: {}", self.config.name, e);
                            } else {
                                log::debug!(
                                    "gpio-sensor '{}': pin={} value={:.3} unit={}",
                                    self.config.name,
                                    self.config.pin,
                                    value,
                                    self.config.sensor_type.unit()
                                );
                            }
                        }
                        Err(e) => {
                            log::error!("gpio-sensor '{}': JSON serialization failed: {}", self.config.name, e);
                        }
                    }
                }

                _ = health_interval.tick() => {
                    let health = json!({
                        "status": "healthy",
                        "node": "gpio-sensor",
                        "instance": self.config.name,
                        "pin": self.config.pin,
                        "timestamp_ms": now_ms(),
                    });
                    if let Ok(bytes) = serde_json::to_vec(&health) {
                        let _ = health_publisher.put(bytes).await;
                    }
                    log::debug!("gpio-sensor '{}': health heartbeat published", self.config.name);
                }
            }
        }

        log::info!("gpio-sensor '{}': stopped", self.config.name);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Simulation layer — replace this with real GPIO calls on hardware
// ---------------------------------------------------------------------------

/// Simulate a pin reading based on sensor type.
///
/// **To use real GPIO on Raspberry Pi**, replace this function body with:
/// ```ignore
/// use rppal::gpio::{Gpio, Level};
/// let gpio = Gpio::new()?;
/// let pin = gpio.get(pin_number)?.into_input();
/// match sensor_type {
///     SensorType::Digital | SensorType::Motion => {
///         let level = pin.read();
///         if active_high { (level == Level::High) as u8 as f64 }
///         else           { (level == Level::Low)  as u8 as f64 }
///     }
///     SensorType::Analog => {
///         // Requires an ADC (MCP3008 etc). Read normalized voltage 0.0–1.0.
///         read_adc(pin_number)
///     }
///     SensorType::Temperature => {
///         // One-wire (DS18B20) or ADC-attached NTC thermistor.
///         read_temperature_celsius(pin_number)
///     }
/// }
/// ```
///
/// **To use Linux gpio-cdev (kernel character device API)**:
/// ```ignore
/// use gpio_cdev::{Chip, LineRequestFlags};
/// let mut chip = Chip::new("/dev/gpiochip0")?;
/// let line = chip.get_line(pin_number)?;
/// let handle = line.request(LineRequestFlags::INPUT, 0, "gpio-sensor")?;
/// let value = handle.get_value()? as f64;
/// ```
fn simulate_read(sensor_type: SensorType, _active_high: bool) -> f64 {
    let mut rng = rand::thread_rng();
    match sensor_type {
        SensorType::Digital => {
            // Simulate a binary switch: ~30% chance of HIGH
            if rng.gen::<f64>() < 0.3 { 1.0 } else { 0.0 }
        }
        SensorType::Analog => {
            // Simulate a noisy analog signal (0.0–1.0)
            rng.gen::<f64>()
        }
        SensorType::Temperature => {
            // Simulate temperature around 22°C with ±2°C Gaussian noise
            let base = 22.0_f64;
            let noise: f64 = (rng.gen::<f64>() - 0.5) * 4.0; // ±2°C
            (base + noise * 100.0).round() / 100.0
        }
        SensorType::Motion => {
            // Simulate a PIR sensor: ~10% chance of motion detection per tick
            if rng.gen::<f64>() < 0.1 { 1.0 } else { 0.0 }
        }
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simulate_digital_bounds() {
        for _ in 0..100 {
            let v = simulate_read(SensorType::Digital, true);
            assert!(v == 0.0 || v == 1.0, "digital must be 0 or 1, got {}", v);
        }
    }

    #[test]
    fn test_simulate_analog_bounds() {
        for _ in 0..100 {
            let v = simulate_read(SensorType::Analog, true);
            assert!((0.0..=1.0).contains(&v), "analog must be 0–1, got {}", v);
        }
    }

    #[test]
    fn test_simulate_temperature_reasonable() {
        for _ in 0..100 {
            let v = simulate_read(SensorType::Temperature, true);
            // Simulated Gaussian around 22°C should stay within 0–50°C
            assert!(v > 0.0 && v < 50.0, "temperature out of expected range: {}", v);
        }
    }

    #[test]
    fn test_simulate_motion_bounds() {
        for _ in 0..100 {
            let v = simulate_read(SensorType::Motion, true);
            assert!(v == 0.0 || v == 1.0, "motion must be 0 or 1, got {}", v);
        }
    }
}
