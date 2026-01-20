# OpenMeteo Weather Service

The OpenMeteo service fetches weather data from the [Open-Meteo API](https://open-meteo.com/) and publishes it to the Zenoh message bus.

## Overview

| Property | Value |
|----------|-------|
| Binary | `openmeteo_node` |
| Config File | `crates/openmeteo/configs/config.yaml` |
| Output | `CurrentWeather`, `HourlyForecast`, `DailyForecast` |
| Input | `LocationConfig` (optional) |

## Features

- **Current conditions** — Temperature, humidity, wind, precipitation
- **Hourly forecast** — Up to 48 hours ahead
- **Daily forecast** — Up to 7 days ahead
- **Auto-discovery** — Location from IP address
- **Dynamic location** — Update location via topic
- **Free API** — No API key required

## Architecture

```mermaid
flowchart LR
    subgraph OpenMeteo["Open-Meteo API"]
        api[Weather API]
    end

    subgraph Node["OpenMeteo Node"]
        fetch[Fetch Service]
        parse[Parser]
        pub[Publishers]
    end

    subgraph Topics["Output Topics"]
        current[/weather/current]
        hourly[/weather/hourly]
        daily[/weather/daily]
    end

    subgraph Input["Input Topic"]
        location[/weather/location]
    end

    api -->|JSON| fetch
    fetch --> parse
    parse --> pub
    pub --> current
    pub --> hourly
    pub --> daily
    location -.-> fetch
```

## Configuration

### Basic Configuration

```yaml
location:
  auto_discover: true
```

### Full Configuration

```yaml
# Location configuration
location:
  auto_discover: false       # Set to true for IP-based location
  latitude: 41.4167
  longitude: 1.9667
  timezone: "Europe/Madrid"

# Fetch intervals (optional)
fetch:
  current_interval_secs: 30      # Current weather poll interval
  hourly_interval_secs: 1800     # Hourly forecast poll interval (30 min)
  daily_interval_secs: 10800     # Daily forecast poll interval (3 hours)
  hourly_forecast_hours: 48      # Hours of hourly forecast to fetch
  daily_forecast_days: 7         # Days of daily forecast to fetch
```

### Configuration Fields

#### Location

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `auto_discover` | boolean | No | `true` | Auto-detect location from IP |
| `latitude` | float | If not auto | — | Latitude in decimal degrees |
| `longitude` | float | If not auto | — | Longitude in decimal degrees |
| `timezone` | string | No | Auto | IANA timezone (e.g., "America/New_York") |

#### Fetch Intervals

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `current_interval_secs` | integer | `30` | Current weather poll interval |
| `hourly_interval_secs` | integer | `1800` | Hourly forecast poll interval |
| `daily_interval_secs` | integer | `10800` | Daily forecast poll interval |
| `hourly_forecast_hours` | integer | `48` | Hours of hourly forecast |
| `daily_forecast_days` | integer | `7` | Days of daily forecast |

## Running

### Start Weather Service

```bash
# Default config
pixi run weather

# Custom config file
pixi run weather -- -c /path/to/config.yaml

# Custom Zenoh endpoint
pixi run weather -- -z tcp/192.168.1.50:7447
```

### CLI Options

| Option | Description |
|--------|-------------|
| `-c, --config` | Path to configuration file |
| `-z, --zenoh-endpoint` | Zenoh router endpoint |

## Topics

### Published Topics

| Topic | Type | Interval | Description |
|-------|------|----------|-------------|
| `/weather/current` | `CurrentWeather` | 30s | Current conditions |
| `/weather/hourly` | `HourlyForecast` | 30m | Hourly forecast |
| `/weather/daily` | `DailyForecast` | 3h | Daily forecast |

### Subscribed Topics

| Topic | Type | Description |
|-------|------|-------------|
| `/weather/location` | `LocationConfig` | Update location dynamically |

### Dynamic Location Update

Send a `LocationConfig` message to change the weather location at runtime:

```protobuf
message LocationConfig {
    double latitude = 1;
    double longitude = 2;
    string timezone = 3;
}
```

## Message Formats

### CurrentWeather

Current weather conditions including:

| Field | Unit | Description |
|-------|------|-------------|
| `temperature_2m` | °C | Temperature at 2m height |
| `relative_humidity_2m` | % | Relative humidity |
| `apparent_temperature` | °C | Feels-like temperature |
| `precipitation` | mm | Precipitation |
| `wind_speed_10m` | km/h | Wind speed at 10m |
| `wind_direction_10m` | ° | Wind direction |
| `weather_code` | WMO | Weather condition code |
| `cloud_cover` | % | Cloud coverage |
| `is_day` | 0/1 | Day/night indicator |

### HourlyForecast

Hourly forecast entries containing:

| Field | Unit | Description |
|-------|------|-------------|
| `time` | Unix timestamp | Forecast time |
| `temperature_2m` | °C | Temperature |
| `relative_humidity_2m` | % | Humidity |
| `precipitation_probability` | % | Rain probability |
| `precipitation` | mm | Expected precipitation |
| `weather_code` | WMO | Weather condition |
| `wind_speed_10m` | km/h | Wind speed |

### DailyForecast

Daily forecast entries containing:

| Field | Unit | Description |
|-------|------|-------------|
| `time` | Unix timestamp | Day start |
| `temperature_2m_max` | °C | Maximum temperature |
| `temperature_2m_min` | °C | Minimum temperature |
| `precipitation_sum` | mm | Total precipitation |
| `weather_code` | WMO | Dominant weather |
| `sunrise` | ISO 8601 | Sunrise time |
| `sunset` | ISO 8601 | Sunset time |

See [Weather API](../../api/weather.md) for full protobuf definitions.

## WMO Weather Codes

| Code | Description |
|------|-------------|
| 0 | Clear sky |
| 1-3 | Mainly clear, partly cloudy, overcast |
| 45, 48 | Fog |
| 51-55 | Drizzle |
| 61-65 | Rain |
| 71-75 | Snow fall |
| 80-82 | Rain showers |
| 95 | Thunderstorm |

## Dashboard Integration

The [Weather Panel](../../dashboard/panels/weather.md) displays OpenMeteo data with:

- Current temperature and conditions
- Weather icons based on WMO codes
- Hourly forecast chart
- Daily forecast summary

## Troubleshooting

### No weather data

1. Check internet connectivity
2. Verify Open-Meteo API is accessible: `curl https://api.open-meteo.com/v1/forecast`
3. Check `pixi run weather` logs for errors

### Wrong location

1. Set explicit coordinates in config
2. Disable `auto_discover` if IP geolocation is inaccurate
3. Send `LocationConfig` message to update dynamically

### Stale data

1. Check fetch intervals in configuration
2. Verify service is running: `/topics` in TUI
3. Check for API rate limiting

## Next Steps

- [Weather Panel](../../dashboard/panels/weather.md) — Dashboard visualization
- [Weather API](../../api/weather.md) — Message format details
- [Configuration](../../getting-started/configuration.md) — Full configuration reference
