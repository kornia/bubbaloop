use serde::Deserialize;
use thiserror::Error;

const BASE_URL: &str = "https://api.open-meteo.com/v1/forecast";
const IPINFO_URL: &str = "https://ipinfo.io/json";

/// IP geolocation response
#[derive(Debug, Deserialize)]
pub struct GeoLocationResponse {
    pub city: String,
    pub region: String,
    pub country: String,
    pub loc: String, // "latitude,longitude"
    pub timezone: String,
}

impl GeoLocationResponse {
    /// Parse the loc field into (latitude, longitude)
    pub fn coordinates(&self) -> Option<(f64, f64)> {
        let parts: Vec<&str> = self.loc.split(',').collect();
        if parts.len() == 2 {
            let lat = parts[0].parse().ok()?;
            let lon = parts[1].parse().ok()?;
            Some((lat, lon))
        } else {
            None
        }
    }
}

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("HTTP request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),
    #[error("API error: {0}")]
    ApiError(String),
}

/// Current weather response from Open-Meteo API
#[derive(Debug, Deserialize)]
pub struct CurrentWeatherResponse {
    pub latitude: f64,
    pub longitude: f64,
    #[serde(default)]
    pub timezone: String,
    pub current: CurrentData,
}

#[derive(Debug, Deserialize)]
pub struct CurrentData {
    #[allow(dead_code)]
    pub time: String,
    pub temperature_2m: f64,
    pub relative_humidity_2m: f64,
    pub apparent_temperature: f64,
    pub precipitation: f64,
    pub rain: f64,
    pub weather_code: u32,
    pub cloud_cover: f64,
    pub pressure_msl: f64,
    pub surface_pressure: f64,
    pub wind_speed_10m: f64,
    pub wind_direction_10m: f64,
    pub wind_gusts_10m: f64,
    pub is_day: u32,
}

/// Hourly forecast response from Open-Meteo API
#[derive(Debug, Deserialize)]
pub struct HourlyForecastResponse {
    pub latitude: f64,
    pub longitude: f64,
    #[serde(default)]
    pub timezone: String,
    pub hourly: HourlyData,
}

#[derive(Debug, Deserialize)]
pub struct HourlyData {
    pub time: Vec<i64>,
    pub temperature_2m: Vec<f64>,
    pub relative_humidity_2m: Vec<f64>,
    pub precipitation_probability: Vec<f64>,
    pub precipitation: Vec<f64>,
    pub weather_code: Vec<u32>,
    pub wind_speed_10m: Vec<f64>,
    pub wind_direction_10m: Vec<f64>,
    pub cloud_cover: Vec<f64>,
}

/// Daily forecast response from Open-Meteo API
#[derive(Debug, Deserialize)]
pub struct DailyForecastResponse {
    pub latitude: f64,
    pub longitude: f64,
    #[serde(default)]
    pub timezone: String,
    pub daily: DailyData,
}

#[derive(Debug, Deserialize)]
pub struct DailyData {
    pub time: Vec<String>,
    pub temperature_2m_max: Vec<f64>,
    pub temperature_2m_min: Vec<f64>,
    pub precipitation_sum: Vec<f64>,
    pub precipitation_probability_max: Vec<f64>,
    pub weather_code: Vec<u32>,
    pub wind_speed_10m_max: Vec<f64>,
    pub wind_gusts_10m_max: Vec<f64>,
    pub sunrise: Vec<String>,
    pub sunset: Vec<String>,
}

/// Open-Meteo API client
pub struct OpenMeteoClient {
    client: reqwest::Client,
}

impl OpenMeteoClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    /// Fetch current weather for a location
    pub async fn fetch_current(
        &self,
        latitude: f64,
        longitude: f64,
        timezone: Option<&str>,
    ) -> Result<CurrentWeatherResponse, ApiError> {
        let tz = timezone.unwrap_or("auto");
        let url = format!(
            "{}?latitude={}&longitude={}&timezone={}&current=\
            temperature_2m,relative_humidity_2m,apparent_temperature,\
            precipitation,rain,weather_code,cloud_cover,pressure_msl,\
            surface_pressure,wind_speed_10m,wind_direction_10m,wind_gusts_10m,is_day",
            BASE_URL, latitude, longitude, tz
        );

        let response = self.client.get(&url).send().await?;
        let data = response.json::<CurrentWeatherResponse>().await?;
        Ok(data)
    }

    /// Fetch hourly forecast for a location
    pub async fn fetch_hourly(
        &self,
        latitude: f64,
        longitude: f64,
        timezone: Option<&str>,
        forecast_hours: u32,
    ) -> Result<HourlyForecastResponse, ApiError> {
        let tz = timezone.unwrap_or("auto");
        let url = format!(
            "{}?latitude={}&longitude={}&timezone={}&forecast_hours={}&timeformat=unixtime&hourly=\
            temperature_2m,relative_humidity_2m,precipitation_probability,\
            precipitation,weather_code,wind_speed_10m,wind_direction_10m,cloud_cover",
            BASE_URL, latitude, longitude, tz, forecast_hours
        );

        let response = self.client.get(&url).send().await?;
        let data = response.json::<HourlyForecastResponse>().await?;
        Ok(data)
    }

    /// Fetch daily forecast for a location
    pub async fn fetch_daily(
        &self,
        latitude: f64,
        longitude: f64,
        timezone: Option<&str>,
        forecast_days: u32,
    ) -> Result<DailyForecastResponse, ApiError> {
        let tz = timezone.unwrap_or("auto");
        let url = format!(
            "{}?latitude={}&longitude={}&timezone={}&forecast_days={}&daily=\
            temperature_2m_max,temperature_2m_min,precipitation_sum,\
            precipitation_probability_max,weather_code,wind_speed_10m_max,\
            wind_gusts_10m_max,sunrise,sunset",
            BASE_URL, latitude, longitude, tz, forecast_days
        );

        let response = self.client.get(&url).send().await?;
        let data = response.json::<DailyForecastResponse>().await?;
        Ok(data)
    }
}

impl Default for OpenMeteoClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Fetch location from IP address using ipinfo.io
pub async fn fetch_location_from_ip() -> Result<GeoLocationResponse, ApiError> {
    let client = reqwest::Client::new();
    let response = client.get(IPINFO_URL).send().await?;
    let data = response.json::<GeoLocationResponse>().await?;
    Ok(data)
}
