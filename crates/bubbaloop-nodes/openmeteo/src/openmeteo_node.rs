use crate::api::{
    fetch_location_from_ip, CurrentWeatherResponse, DailyForecastResponse, HourlyForecastResponse,
    OpenMeteoClient,
};
use crate::config::{FetchConfig, LocationConfig};
use bubbaloop_schemas::{
    CurrentWeather, DailyForecast, DailyForecastEntry, Header, HourlyForecast, HourlyForecastEntry,
    LocationConfig as LocationConfigProto,
};
use ros_z::{context::ZContext, msg::ProtobufSerdes, pubsub::ZPub, Builder, Result as ZResult};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};

/// Resolved location with coordinates
#[derive(Debug, Clone)]
pub struct ResolvedLocation {
    pub latitude: f64,
    pub longitude: f64,
    pub timezone: Option<String>,
    pub city: Option<String>,
}

fn get_pub_time() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

fn convert_current_weather(
    response: CurrentWeatherResponse,
    location_name: &str,
    sequence: u32,
    machine_id: &str,
) -> CurrentWeather {
    let now = get_pub_time();
    CurrentWeather {
        header: Some(Header {
            acq_time: now,
            pub_time: now,
            sequence,
            frame_id: location_name.to_string(),
            machine_id: machine_id.to_string(),
            ..Default::default()
        }),
        latitude: response.latitude,
        longitude: response.longitude,
        timezone: response.timezone,
        temperature_2m: response.current.temperature_2m,
        relative_humidity_2m: response.current.relative_humidity_2m,
        apparent_temperature: response.current.apparent_temperature,
        precipitation: response.current.precipitation,
        rain: response.current.rain,
        wind_speed_10m: response.current.wind_speed_10m,
        wind_direction_10m: response.current.wind_direction_10m,
        wind_gusts_10m: response.current.wind_gusts_10m,
        weather_code: response.current.weather_code,
        cloud_cover: response.current.cloud_cover,
        pressure_msl: response.current.pressure_msl,
        surface_pressure: response.current.surface_pressure,
        is_day: response.current.is_day,
    }
}

fn convert_hourly_forecast(
    response: HourlyForecastResponse,
    location_name: &str,
    sequence: u32,
    machine_id: &str,
) -> HourlyForecast {
    let now = get_pub_time();
    let entries: Vec<HourlyForecastEntry> = response
        .hourly
        .time
        .iter()
        .enumerate()
        .map(|(i, &time)| HourlyForecastEntry {
            time: time as u64,
            temperature_2m: response
                .hourly
                .temperature_2m
                .get(i)
                .copied()
                .unwrap_or(0.0),
            relative_humidity_2m: response
                .hourly
                .relative_humidity_2m
                .get(i)
                .copied()
                .unwrap_or(0.0),
            precipitation_probability: response
                .hourly
                .precipitation_probability
                .get(i)
                .copied()
                .unwrap_or(0.0),
            precipitation: response.hourly.precipitation.get(i).copied().unwrap_or(0.0),
            weather_code: response.hourly.weather_code.get(i).copied().unwrap_or(0),
            wind_speed_10m: response
                .hourly
                .wind_speed_10m
                .get(i)
                .copied()
                .unwrap_or(0.0),
            wind_direction_10m: response
                .hourly
                .wind_direction_10m
                .get(i)
                .copied()
                .unwrap_or(0.0),
            cloud_cover: response.hourly.cloud_cover.get(i).copied().unwrap_or(0.0),
        })
        .collect();

    HourlyForecast {
        header: Some(Header {
            acq_time: now,
            pub_time: now,
            sequence,
            frame_id: location_name.to_string(),
            machine_id: machine_id.to_string(),
            ..Default::default()
        }),
        latitude: response.latitude,
        longitude: response.longitude,
        timezone: response.timezone,
        entries,
    }
}

fn parse_date_to_unix(date_str: &str) -> u64 {
    // Parse YYYY-MM-DD format to approximate unix timestamp (midnight UTC)
    let parts: Vec<&str> = date_str.split('-').collect();
    if parts.len() != 3 {
        return 0;
    }
    let year: i32 = parts[0].parse().unwrap_or(1970);
    let month: u32 = parts[1].parse().unwrap_or(1);
    let day: u32 = parts[2].parse().unwrap_or(1);

    // Simple calculation for unix timestamp (approximate)
    let days_since_epoch = (year - 1970) as i64 * 365
        + (year - 1969) as i64 / 4 // leap years
        + days_before_month(month, is_leap_year(year))
        + (day - 1) as i64;

    (days_since_epoch * 86400) as u64
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn days_before_month(month: u32, leap: bool) -> i64 {
    const DAYS: [i64; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let d = DAYS.get((month - 1) as usize).copied().unwrap_or(0);
    if leap && month > 2 {
        d + 1
    } else {
        d
    }
}

fn convert_daily_forecast(
    response: DailyForecastResponse,
    location_name: &str,
    sequence: u32,
    machine_id: &str,
) -> DailyForecast {
    let now = get_pub_time();
    let entries: Vec<DailyForecastEntry> = response
        .daily
        .time
        .iter()
        .enumerate()
        .map(|(i, time)| DailyForecastEntry {
            time: parse_date_to_unix(time),
            temperature_2m_max: response
                .daily
                .temperature_2m_max
                .get(i)
                .copied()
                .unwrap_or(0.0),
            temperature_2m_min: response
                .daily
                .temperature_2m_min
                .get(i)
                .copied()
                .unwrap_or(0.0),
            precipitation_sum: response
                .daily
                .precipitation_sum
                .get(i)
                .copied()
                .unwrap_or(0.0),
            precipitation_probability_max: response
                .daily
                .precipitation_probability_max
                .get(i)
                .copied()
                .unwrap_or(0.0),
            weather_code: response.daily.weather_code.get(i).copied().unwrap_or(0),
            wind_speed_10m_max: response
                .daily
                .wind_speed_10m_max
                .get(i)
                .copied()
                .unwrap_or(0.0),
            wind_gusts_10m_max: response
                .daily
                .wind_gusts_10m_max
                .get(i)
                .copied()
                .unwrap_or(0.0),
            sunrise: response.daily.sunrise.get(i).cloned().unwrap_or_default(),
            sunset: response.daily.sunset.get(i).cloned().unwrap_or_default(),
        })
        .collect();

    DailyForecast {
        header: Some(Header {
            acq_time: now,
            pub_time: now,
            sequence,
            frame_id: location_name.to_string(),
            machine_id: machine_id.to_string(),
            ..Default::default()
        }),
        latitude: response.latitude,
        longitude: response.longitude,
        timezone: response.timezone,
        entries,
    }
}

/// Resolve location from config, with auto-discovery support
pub async fn resolve_location(config: &LocationConfig) -> Result<ResolvedLocation, String> {
    if let (Some(lat), Some(lon)) = (config.latitude, config.longitude) {
        // Use explicit coordinates
        Ok(ResolvedLocation {
            latitude: lat,
            longitude: lon,
            timezone: config.timezone.clone(),
            city: None,
        })
    } else if config.auto_discover {
        // Auto-discover from IP
        log::info!("Auto-discovering location from IP address...");
        match fetch_location_from_ip().await {
            Ok(geo) => {
                if let Some((lat, lon)) = geo.coordinates() {
                    log::info!(
                        "Discovered location: {}, {} ({}, {})",
                        geo.city,
                        geo.country,
                        lat,
                        lon
                    );
                    Ok(ResolvedLocation {
                        latitude: lat,
                        longitude: lon,
                        timezone: Some(geo.timezone),
                        city: Some(geo.city),
                    })
                } else {
                    Err("Failed to parse coordinates from IP geolocation".to_string())
                }
            }
            Err(e) => Err(format!("Failed to fetch location from IP: {}", e)),
        }
    } else {
        Err("No location specified and auto_discover is disabled".to_string())
    }
}

/// Open-Meteo weather node
pub struct OpenMeteoNode {
    ctx: Arc<ZContext>,
    location: ResolvedLocation,
    fetch_config: FetchConfig,
    client: OpenMeteoClient,
    machine_id: String,
}

impl OpenMeteoNode {
    pub fn new(
        ctx: Arc<ZContext>,
        location: ResolvedLocation,
        fetch_config: FetchConfig,
        machine_id: String,
    ) -> ZResult<Self> {
        Ok(Self {
            ctx,
            location,
            fetch_config,
            client: OpenMeteoClient::new(),
            machine_id,
        })
    }

    pub async fn run(
        self,
        shutdown_tx: tokio::sync::watch::Sender<()>,
        zenoh_session: zenoh::Session,
        scope: String,
        machine_id: String,
    ) -> ZResult<()> {
        let mut shutdown_rx = shutdown_tx.subscribe();

        // Mutable location that can be updated via Zenoh
        let mut location = self.location.clone();
        let mut location_label = location
            .city
            .clone()
            .unwrap_or_else(|| format!("{:.2},{:.2}", location.latitude, location.longitude));

        // Create health heartbeat publisher
        let health_topic = format!("bubbaloop/{}/{}/health/openmeteo", scope, machine_id);
        let health_publisher = zenoh_session
            .declare_publisher(health_topic.clone())
            .await
            .map_err(|e| {
                Box::<dyn std::error::Error + Send + Sync>::from(format!(
                    "Health publisher error: {}",
                    e
                ))
            })?;
        log::info!("Health heartbeat topic: {}", health_topic);

        // Create ROS-Z node
        let node = Arc::new(self.ctx.create_node("weather").build()?);

        // Create publishers with simple topic names
        let current_topic = "/weather/current";
        let hourly_topic = "/weather/hourly";
        let daily_topic = "/weather/daily";
        let config_topic = "/weather/config/location";

        let current_pub: ZPub<CurrentWeather, ProtobufSerdes<CurrentWeather>> = node
            .create_pub::<CurrentWeather>(current_topic)
            .with_serdes::<ProtobufSerdes<CurrentWeather>>()
            .build()?;

        let hourly_pub: ZPub<HourlyForecast, ProtobufSerdes<HourlyForecast>> = node
            .create_pub::<HourlyForecast>(hourly_topic)
            .with_serdes::<ProtobufSerdes<HourlyForecast>>()
            .build()?;

        let daily_pub: ZPub<DailyForecast, ProtobufSerdes<DailyForecast>> = node
            .create_pub::<DailyForecast>(daily_topic)
            .with_serdes::<ProtobufSerdes<DailyForecast>>()
            .build()?;

        // Create subscriber for location updates
        let location_sub = node
            .create_sub::<LocationConfigProto>(config_topic)
            .with_serdes::<ProtobufSerdes<LocationConfigProto>>()
            .build()?;

        // Channel for location updates from subscriber task
        let (location_tx, mut location_rx) = mpsc::channel::<LocationConfigProto>(10);

        // Spawn location subscriber task
        let mut shutdown_rx_loc = shutdown_tx.subscribe();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_rx_loc.changed() => break,
                    result = location_sub.async_recv() => {
                        match result {
                            Ok(config) => {
                                if let Err(e) = location_tx.send(config).await {
                                    log::error!("Failed to send location update: {}", e);
                                    break;
                                }
                            }
                            Err(e) => {
                                log::warn!("Error receiving location config: {}", e);
                            }
                        }
                    }
                }
            }
            log::info!("Location subscriber task shutting down");
        });

        log::info!(
            "[{}] Weather node started - current: {}s, hourly: {}s, daily: {}s",
            location_label,
            self.fetch_config.current_interval_secs,
            self.fetch_config.hourly_interval_secs,
            self.fetch_config.daily_interval_secs,
        );
        log::info!(
            "Topics: {}, {}, {}",
            current_topic,
            hourly_topic,
            daily_topic
        );
        log::info!(
            "Location config topic: {} (publish to change location)",
            config_topic
        );

        // Create intervals for each data type
        let mut current_interval =
            interval(Duration::from_secs(self.fetch_config.current_interval_secs));
        let mut hourly_interval =
            interval(Duration::from_secs(self.fetch_config.hourly_interval_secs));
        let mut daily_interval =
            interval(Duration::from_secs(self.fetch_config.daily_interval_secs));

        let mut current_seq: u32 = 0;
        let mut hourly_seq: u32 = 0;
        let mut daily_seq: u32 = 0;
        let mut health_interval = interval(Duration::from_secs(5));

        loop {
            tokio::select! {
                biased;

                _ = shutdown_rx.changed() => {
                    log::info!("[{}] Weather node received shutdown", location_label);
                    break;
                }

                // Health heartbeat
                _ = health_interval.tick() => {
                    if let Err(e) = health_publisher.put("ok").await {
                        log::warn!("[{}] Failed to publish health heartbeat: {}", location_label, e);
                    }
                }

                // Handle location updates
                Some(new_config) = location_rx.recv() => {
                    log::info!(
                        "Received location update: ({:.4}, {:.4}) timezone={}",
                        new_config.latitude,
                        new_config.longitude,
                        if new_config.timezone.is_empty() { "auto" } else { &new_config.timezone }
                    );

                    // Update location
                    location = ResolvedLocation {
                        latitude: new_config.latitude,
                        longitude: new_config.longitude,
                        timezone: if new_config.timezone.is_empty() {
                            None
                        } else {
                            Some(new_config.timezone)
                        },
                        city: None,
                    };
                    location_label = format!("{:.2},{:.2}", location.latitude, location.longitude);

                    // Reset intervals to trigger immediate fetch with new location
                    current_interval.reset();
                    hourly_interval.reset();
                    daily_interval.reset();

                    log::info!("[{}] Location updated, fetching weather data...", location_label);
                }

                _ = current_interval.tick() => {
                    match self.client.fetch_current(
                        location.latitude,
                        location.longitude,
                        location.timezone.as_deref(),
                    ).await {
                        Ok(response) => {
                            let temp = response.current.temperature_2m;
                            let msg = convert_current_weather(response, &location_label, current_seq, &self.machine_id);
                            if current_pub.async_publish(&msg).await.is_ok() {
                                log::info!("[{}] Current weather: {:.1}°C (seq={})", location_label, temp, current_seq);
                                current_seq = current_seq.wrapping_add(1);
                            }
                        }
                        Err(e) => {
                            log::warn!("[{}] Failed to fetch current weather: {}", location_label, e);
                        }
                    }
                }

                _ = hourly_interval.tick() => {
                    match self.client.fetch_hourly(
                        location.latitude,
                        location.longitude,
                        location.timezone.as_deref(),
                        self.fetch_config.hourly_forecast_hours,
                    ).await {
                        Ok(response) => {
                            let count = response.hourly.time.len();
                            let msg = convert_hourly_forecast(response, &location_label, hourly_seq, &self.machine_id);
                            if hourly_pub.async_publish(&msg).await.is_ok() {
                                log::info!("[{}] Hourly forecast: {} entries (seq={})", location_label, count, hourly_seq);
                                hourly_seq = hourly_seq.wrapping_add(1);
                            }
                        }
                        Err(e) => {
                            log::warn!("[{}] Failed to fetch hourly forecast: {}", location_label, e);
                        }
                    }
                }

                _ = daily_interval.tick() => {
                    match self.client.fetch_daily(
                        location.latitude,
                        location.longitude,
                        location.timezone.as_deref(),
                        self.fetch_config.daily_forecast_days,
                    ).await {
                        Ok(response) => {
                            let count = response.daily.time.len();
                            let msg = convert_daily_forecast(response, &location_label, daily_seq, &self.machine_id);
                            if daily_pub.async_publish(&msg).await.is_ok() {
                                log::info!("[{}] Daily forecast: {} days (seq={})", location_label, count, daily_seq);
                                daily_seq = daily_seq.wrapping_add(1);
                            }
                        }
                        Err(e) => {
                            log::warn!("[{}] Failed to fetch daily forecast: {}", location_label, e);
                        }
                    }
                }
            }
        }

        log::info!("[{}] Weather node shutdown complete", location_label);
        Ok(())
    }
}
