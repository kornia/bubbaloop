//! Modbus TCP driver — read registers on a fixed interval and publish to Zenoh.

use super::{spawn_health_loop, BuiltinDriver, DriverConfig, DriverError, Result};
use std::net::SocketAddr;
use std::time::Duration;
use tokio_modbus::prelude::*;

pub struct ModbusDriver;

#[async_trait::async_trait]
impl BuiltinDriver for ModbusDriver {
    fn name(&self) -> &'static str {
        "modbus"
    }

    async fn run(&self, config: DriverConfig) -> Result<()> {
        let host = config.require_str("host")?;
        let port = config.u16_or("port", 502);
        let register = config.u64_or("register", 0) as u16;
        let register_type = config.str_or("type", "u16");
        let interval_secs = config.u64_or("interval_secs", 2);
        let count = match register_type.as_str() {
            "float32" | "f32" => 2u16,
            "u32" | "i32" => 2,
            _ => 1, // u16, i16
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

        let addr: SocketAddr = format!("{}:{}", host, port)
            .parse()
            .map_err(|e| DriverError::ConfigError(format!("Invalid address: {}", e)))?;

        let mut ctx = tcp::connect(addr)
            .await
            .map_err(|e| DriverError::StartFailed(format!("Modbus connect to {}: {}", addr, e)))?;

        let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));

        log::info!(
            "[modbus] skill='{}' host='{}:{}' register={} type={} interval={}s",
            config.skill_name,
            host,
            port,
            register,
            register_type,
            interval_secs
        );

        loop {
            tokio::select! {
                biased;
                _ = shutdown_rx.changed() => {
                    log::info!("[modbus] '{}' shutting down", config.skill_name);
                    break;
                }
                _ = interval.tick() => {
                    match ctx.read_holding_registers(register, count).await {
                        Ok(Ok(registers)) => {
                            let value = decode_registers(&registers, &register_type);
                            let payload = serde_json::json!({
                                "register": register,
                                "type": register_type,
                                "value": value,
                                "raw": registers,
                            });
                            if let Err(e) = session.put(&data_topic, payload.to_string()).await {
                                log::warn!("[modbus] publish failed: {}", e);
                            }
                        }
                        Ok(Err(exception)) => log::warn!("[modbus] register {} exception: {:?}", register, exception),
                        Err(e) => log::warn!("[modbus] read register {} failed: {}", register, e),
                    }
                }
            }
        }
        Ok(())
    }
}

fn decode_registers(regs: &[u16], register_type: &str) -> serde_json::Value {
    match register_type {
        "u16" => serde_json::json!(regs.first().copied().unwrap_or(0)),
        "i16" => serde_json::json!(regs.first().copied().unwrap_or(0) as i16),
        "u32" if regs.len() >= 2 => {
            let val = ((regs[0] as u32) << 16) | (regs[1] as u32);
            serde_json::json!(val)
        }
        "i32" if regs.len() >= 2 => {
            let val = (((regs[0] as u32) << 16) | (regs[1] as u32)) as i32;
            serde_json::json!(val)
        }
        "float32" | "f32" if regs.len() >= 2 => {
            let bits = ((regs[0] as u32) << 16) | (regs[1] as u32);
            let val = f32::from_bits(bits);
            serde_json::json!(val)
        }
        _ => serde_json::json!(regs),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn driver_name() {
        assert_eq!(ModbusDriver.name(), "modbus");
    }

    #[test]
    fn decode_u16() {
        assert_eq!(decode_registers(&[42], "u16"), serde_json::json!(42));
    }

    #[test]
    fn decode_i16() {
        // 0xFFFF = 65535 unsigned, -1 as i16
        assert_eq!(decode_registers(&[0xFFFF], "i16"), serde_json::json!(-1));
    }

    #[test]
    fn decode_u32() {
        // 0x0001_0000 = 65536
        assert_eq!(decode_registers(&[1, 0], "u32"), serde_json::json!(65536));
    }

    #[test]
    fn decode_float32() {
        // IEEE 754: 42.0 = 0x42280000
        let bits: u32 = 42.0f32.to_bits();
        let hi = (bits >> 16) as u16;
        let lo = (bits & 0xFFFF) as u16;
        let result = decode_registers(&[hi, lo], "float32");
        assert_eq!(result, serde_json::json!(42.0));
    }
}
