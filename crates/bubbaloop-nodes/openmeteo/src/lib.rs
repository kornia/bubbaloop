//! Open-Meteo weather data publisher node for Bubbaloop.
//!
//! This node fetches weather data from the Open-Meteo API and publishes:
//! - Current weather conditions
//! - Hourly forecast
//! - Daily forecast
//!
//! Supports location auto-discovery from IP address.

mod api;
mod config;
mod node;

pub use api::{ApiError, OpenMeteoClient};
pub use config::{Config, ConfigError, FetchConfig, LocationConfig};
pub use node::{resolve_location, OpenMeteoNode, ResolvedLocation};
