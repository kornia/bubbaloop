//! Bubbaloop MCP Server
//!
//! Exposes Bubbaloop functionality to AI assistants via Model Context Protocol.
//! Implements MCP JSON-RPC protocol over stdio.

use std::sync::Arc;
use std::time::Duration;

use log::{info, warn, error, debug};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

/// MCP Protocol version
pub const PROTOCOL_VERSION: &str = "2024-11-05";

/// Server name
pub const SERVER_NAME: &str = "bubbaloop-mcp-server";

/// Server version
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Error, Debug)]
pub enum McpError {
    #[error("JSON-RPC error: {0}")]
    JsonRpc(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Zenoh error: {0}")]
    Zenoh(String),
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
}

/// JSON-RPC Request
#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

/// JSON-RPC Response
#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcResponse {
    pub fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Option<Value>, code: i32, message: &str) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.to_string(),
                data: None,
            }),
        }
    }
}

/// Tool definition for MCP
#[derive(Debug, Serialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

/// Content in tool result
#[derive(Debug, Serialize)]
pub struct ToolContent {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: String,
}

/// Bubbaloop MCP Server
pub struct BubbaloopServer {
    zenoh_session: Arc<zenoh::Session>,
    /// Cache of last weather data
    last_weather: Arc<Mutex<Option<Value>>>,
    last_forecast: Arc<Mutex<Option<Value>>>,
}

impl BubbaloopServer {
    /// Create a new Bubbaloop MCP server connected to the given Zenoh endpoint
    pub async fn new(zenoh_endpoint: &str) -> anyhow::Result<Self> {
        info!("Connecting to Zenoh at {}", zenoh_endpoint);

        let mut config = zenoh::Config::default();
        config
            .connect
            .endpoints
            .set(vec![zenoh_endpoint.parse().unwrap()])
            .map_err(|e| anyhow::anyhow!("Failed to set Zenoh endpoint: {:?}", e))?;

        let session = zenoh::open(config).await
            .map_err(|e| anyhow::anyhow!("Failed to open Zenoh session: {}", e))?;

        info!("Connected to Zenoh successfully");

        Ok(Self {
            zenoh_session: Arc::new(session),
            last_weather: Arc::new(Mutex::new(None)),
            last_forecast: Arc::new(Mutex::new(None)),
        })
    }

    /// Get list of available tools
    fn get_tools(&self) -> Vec<Tool> {
        vec![
            Tool {
                name: "get_weather".to_string(),
                description: "Get the current weather data from Bubbaloop's weather service".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
            Tool {
                name: "get_forecast".to_string(),
                description: "Get the hourly weather forecast from Bubbaloop".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
            Tool {
                name: "update_location".to_string(),
                description: "Update the location for weather data (latitude, longitude)".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "latitude": {
                            "type": "number",
                            "description": "Latitude coordinate"
                        },
                        "longitude": {
                            "type": "number",
                            "description": "Longitude coordinate"
                        },
                        "timezone": {
                            "type": "string",
                            "description": "Optional timezone (e.g., 'America/New_York')"
                        }
                    },
                    "required": ["latitude", "longitude"]
                }),
            },
            Tool {
                name: "list_topics".to_string(),
                description: "List available Bubbaloop Zenoh topics (debug tool)".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
        ]
    }

    /// Handle initialize request
    fn handle_initialize(&self, _params: Value) -> Value {
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": SERVER_NAME,
                "version": SERVER_VERSION
            },
            "instructions": "Bubbaloop MCP Server - Control cameras, weather, and recording via Zenoh pub/sub.\n\nAvailable tools:\n- get_weather: Get current weather data\n- get_forecast: Get hourly weather forecast\n- update_location: Change the weather location\n- list_topics: Debug tool to see available Zenoh topics"
        })
    }

    /// Handle tools/list request
    fn handle_tools_list(&self) -> Value {
        json!({
            "tools": self.get_tools()
        })
    }

    /// Handle tools/call request
    async fn handle_tools_call(&self, params: Value) -> Result<Value, McpError> {
        let name = params.get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidRequest("Missing tool name".to_string()))?;

        let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

        debug!("Calling tool: {} with args: {}", name, arguments);

        let result = match name {
            "get_weather" => self.tool_get_weather().await?,
            "get_forecast" => self.tool_get_forecast().await?,
            "update_location" => self.tool_update_location(arguments).await?,
            "list_topics" => self.tool_list_topics().await?,
            _ => return Err(McpError::InvalidRequest(format!("Unknown tool: {}", name))),
        };

        Ok(json!({
            "content": [{
                "type": "text",
                "text": result
            }]
        }))
    }

    /// Get current weather data
    async fn tool_get_weather(&self) -> Result<String, McpError> {
        info!("Tool called: get_weather");

        // Subscribe to weather topic and wait for a message
        // ROS-Z transforms topics: /weather/current -> 0/weather%current/bubbaloop.weather.v1.CurrentWeather/...
        let subscriber = self.zenoh_session
            .declare_subscriber("**/weather%current/**")
            .await
            .map_err(|e| McpError::Zenoh(format!("Zenoh subscribe failed: {}", e)))?;

        // Wait for a sample with timeout
        let sample = tokio::time::timeout(
            Duration::from_secs(5),
            subscriber.recv_async()
        ).await;

        match sample {
            Ok(Ok(sample)) => {
                let payload = sample.payload().to_bytes();

                // Try to decode as protobuf CurrentWeather
                use bubbaloop::schemas::weather::v1::CurrentWeather;
                use prost::Message;

                if let Ok(weather) = CurrentWeather::decode(payload.as_ref()) {
                    let weather_json = json!({
                        "temperature_2m": weather.temperature_2m,
                        "apparent_temperature": weather.apparent_temperature,
                        "relative_humidity_2m": weather.relative_humidity_2m,
                        "wind_speed_10m": weather.wind_speed_10m,
                        "wind_direction_10m": weather.wind_direction_10m,
                        "wind_gusts_10m": weather.wind_gusts_10m,
                        "weather_code": weather.weather_code,
                        "is_day": weather.is_day,
                        "precipitation": weather.precipitation,
                        "rain": weather.rain,
                        "cloud_cover": weather.cloud_cover,
                        "pressure_msl": weather.pressure_msl,
                        "surface_pressure": weather.surface_pressure,
                        "latitude": weather.latitude,
                        "longitude": weather.longitude,
                        "timezone": weather.timezone,
                    });
                    *self.last_weather.lock().await = Some(weather_json.clone());
                    Ok(serde_json::to_string_pretty(&weather_json).unwrap())
                } else {
                    // Try as JSON fallback
                    if let Ok(json) = serde_json::from_slice::<Value>(&payload) {
                        *self.last_weather.lock().await = Some(json.clone());
                        Ok(serde_json::to_string_pretty(&json).unwrap())
                    } else {
                        Ok(format!("Received data but couldn't decode: {} bytes", payload.len()))
                    }
                }
            }
            Ok(Err(e)) => {
                warn!("Subscriber error: {}", e);
                let cached = self.last_weather.lock().await;
                if let Some(data) = cached.as_ref() {
                    Ok(format!("(cached) {}", serde_json::to_string_pretty(data).unwrap()))
                } else {
                    Ok("No weather data available. Make sure the weather service is running.".to_string())
                }
            }
            Err(_) => {
                // Timeout - return cached data if available
                let cached = self.last_weather.lock().await;
                if let Some(data) = cached.as_ref() {
                    Ok(format!("(cached) {}", serde_json::to_string_pretty(data).unwrap()))
                } else {
                    Ok("Timeout waiting for weather data. Make sure the weather service is running.".to_string())
                }
            }
        }
    }

    /// Get weather forecast
    async fn tool_get_forecast(&self) -> Result<String, McpError> {
        info!("Tool called: get_forecast");

        // Subscribe to hourly forecast topic
        // ROS-Z transforms topics: /weather/hourly -> 0/weather%hourly/bubbaloop.weather.v1.HourlyForecast/...
        let subscriber = self.zenoh_session
            .declare_subscriber("**/weather%hourly/**")
            .await
            .map_err(|e| McpError::Zenoh(format!("Zenoh subscribe failed: {}", e)))?;

        // Wait for a sample with timeout
        let sample = tokio::time::timeout(
            Duration::from_secs(5),
            subscriber.recv_async()
        ).await;

        match sample {
            Ok(Ok(sample)) => {
                let payload = sample.payload().to_bytes();

                // Try to decode as protobuf HourlyForecast
                use bubbaloop::schemas::weather::v1::HourlyForecast;
                use prost::Message;

                if let Ok(forecast) = HourlyForecast::decode(payload.as_ref()) {
                    let entries: Vec<Value> = forecast.entries.iter().map(|e| {
                        json!({
                            "time": e.time,
                            "temperature_2m": e.temperature_2m,
                            "relative_humidity_2m": e.relative_humidity_2m,
                            "precipitation_probability": e.precipitation_probability,
                            "precipitation": e.precipitation,
                            "weather_code": e.weather_code,
                            "wind_speed_10m": e.wind_speed_10m,
                            "wind_direction_10m": e.wind_direction_10m,
                            "cloud_cover": e.cloud_cover,
                        })
                    }).collect();

                    let forecast_json = json!({
                        "entries": entries,
                        "count": forecast.entries.len()
                    });
                    *self.last_forecast.lock().await = Some(forecast_json.clone());
                    Ok(serde_json::to_string_pretty(&forecast_json).unwrap())
                } else {
                    // Try as JSON fallback
                    if let Ok(json) = serde_json::from_slice::<Value>(&payload) {
                        *self.last_forecast.lock().await = Some(json.clone());
                        Ok(serde_json::to_string_pretty(&json).unwrap())
                    } else {
                        Ok(format!("Received data but couldn't decode: {} bytes", payload.len()))
                    }
                }
            }
            Ok(Err(e)) => {
                warn!("Subscriber error: {}", e);
                let cached = self.last_forecast.lock().await;
                if let Some(data) = cached.as_ref() {
                    Ok(format!("(cached) {}", serde_json::to_string_pretty(data).unwrap()))
                } else {
                    Ok("No forecast data available. Make sure the weather service is running.".to_string())
                }
            }
            Err(_) => {
                let cached = self.last_forecast.lock().await;
                if let Some(data) = cached.as_ref() {
                    Ok(format!("(cached) {}", serde_json::to_string_pretty(data).unwrap()))
                } else {
                    Ok("Timeout waiting for forecast data. Make sure the weather service is running.".to_string())
                }
            }
        }
    }

    /// Update weather location
    async fn tool_update_location(&self, params: Value) -> Result<String, McpError> {
        let latitude = params.get("latitude")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| McpError::InvalidRequest("Missing latitude".to_string()))?;

        let longitude = params.get("longitude")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| McpError::InvalidRequest("Missing longitude".to_string()))?;

        let timezone = params.get("timezone")
            .and_then(|v| v.as_str())
            .unwrap_or("auto");

        info!("Tool called: update_location lat={}, lon={}", latitude, longitude);

        let location_config = json!({
            "latitude": latitude,
            "longitude": longitude,
            "timezone": timezone,
        });

        let payload = serde_json::to_vec(&location_config)
            .map_err(|e| McpError::Json(e))?;

        self.zenoh_session
            .put("config/location", payload)
            .await
            .map_err(|e| McpError::Zenoh(format!("Zenoh put failed: {}", e)))?;

        Ok(format!("Location updated to ({}, {})", latitude, longitude))
    }

    /// List available Zenoh topics
    async fn tool_list_topics(&self) -> Result<String, McpError> {
        info!("Tool called: list_topics");

        // Known Bubbaloop topics
        let known_topics = vec![
            "/weather/current - Current weather data (protobuf)",
            "/weather/hourly - Hourly forecast (protobuf)",
            "/weather/daily - Daily forecast (protobuf)",
            "/weather/config/location - Location config (subscribe to update)",
            "/camera/*/frames/compressed - Camera frames (protobuf)",
            "/camera/*/frames/raw - Raw camera frames (protobuf)",
        ];

        Ok(format!(
            "Known Bubbaloop topics:\n{}\n\nUse get_weather or get_forecast to fetch data.",
            known_topics.join("\n")
        ))
    }

    /// Handle a single JSON-RPC request
    pub async fn handle_request(&self, request: JsonRpcRequest) -> Option<JsonRpcResponse> {
        debug!("Handling request: {}", request.method);

        match request.method.as_str() {
            "initialize" => {
                let result = self.handle_initialize(request.params);
                Some(JsonRpcResponse::success(request.id, result))
            }
            "initialized" => {
                // Notification, no response needed
                None
            }
            "tools/list" => {
                let result = self.handle_tools_list();
                Some(JsonRpcResponse::success(request.id, result))
            }
            "tools/call" => {
                match self.handle_tools_call(request.params).await {
                    Ok(result) => Some(JsonRpcResponse::success(request.id, result)),
                    Err(e) => Some(JsonRpcResponse::error(request.id, -32603, &e.to_string())),
                }
            }
            "ping" => {
                Some(JsonRpcResponse::success(request.id, json!({})))
            }
            _ => {
                warn!("Unknown method: {}", request.method);
                Some(JsonRpcResponse::error(
                    request.id,
                    -32601,
                    &format!("Method not found: {}", request.method),
                ))
            }
        }
    }

    /// Run the MCP server over stdio
    pub async fn run_stdio(&self) -> anyhow::Result<()> {
        let stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let reader = BufReader::new(stdin);
        let mut lines = reader.lines();

        info!("MCP server ready, listening on stdio...");

        while let Some(line) = lines.next_line().await? {
            if line.is_empty() {
                continue;
            }

            debug!("Received: {}", line);

            let request: JsonRpcRequest = match serde_json::from_str(&line) {
                Ok(req) => req,
                Err(e) => {
                    error!("Failed to parse request: {}", e);
                    let response = JsonRpcResponse::error(None, -32700, "Parse error");
                    let response_json = serde_json::to_string(&response)?;
                    stdout.write_all(response_json.as_bytes()).await?;
                    stdout.write_all(b"\n").await?;
                    stdout.flush().await?;
                    continue;
                }
            };

            if let Some(response) = self.handle_request(request).await {
                let response_json = serde_json::to_string(&response)?;
                debug!("Sending: {}", response_json);
                stdout.write_all(response_json.as_bytes()).await?;
                stdout.write_all(b"\n").await?;
                stdout.flush().await?;
            }
        }

        Ok(())
    }
}
