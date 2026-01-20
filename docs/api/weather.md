# Weather Messages

Weather messages define the format for data published by the OpenMeteo weather service.

## Package

```
bubbaloop.weather.v1
```

## Messages

### CurrentWeather

Current weather conditions.

```protobuf
syntax = "proto3";

package bubbaloop.weather.v1;

import "bubbaloop/header.proto";

// Current weather conditions
message CurrentWeather {
    bubbaloop.header.v1.Header header = 1;

    // Location information
    double latitude = 2;
    double longitude = 3;
    string timezone = 4;

    // Weather measurements
    double temperature_2m = 5;          // Temperature at 2m height (Celsius)
    double relative_humidity_2m = 6;    // Relative humidity at 2m (%)
    double apparent_temperature = 7;    // Feels-like temperature (Celsius)
    double precipitation = 8;           // Precipitation (mm)
    double rain = 9;                    // Rain (mm)
    double wind_speed_10m = 10;         // Wind speed at 10m (km/h)
    double wind_direction_10m = 11;     // Wind direction at 10m (degrees)
    double wind_gusts_10m = 12;         // Wind gusts at 10m (km/h)
    uint32 weather_code = 13;           // WMO weather code
    double cloud_cover = 14;            // Cloud cover (%)
    double pressure_msl = 15;           // Mean sea level pressure (hPa)
    double surface_pressure = 16;       // Surface pressure (hPa)
    uint32 is_day = 17;                 // 1 = day, 0 = night
}
```

#### Fields

| Field | Type | Unit | Description |
|-------|------|------|-------------|
| `header` | Header | — | Message metadata |
| `latitude` | double | ° | Location latitude |
| `longitude` | double | ° | Location longitude |
| `timezone` | string | — | IANA timezone |
| `temperature_2m` | double | °C | Temperature at 2m height |
| `relative_humidity_2m` | double | % | Relative humidity |
| `apparent_temperature` | double | °C | Feels-like temperature |
| `precipitation` | double | mm | Total precipitation |
| `rain` | double | mm | Rain amount |
| `wind_speed_10m` | double | km/h | Wind speed at 10m |
| `wind_direction_10m` | double | ° | Wind direction (0-360) |
| `wind_gusts_10m` | double | km/h | Wind gusts |
| `weather_code` | uint32 | WMO | Weather condition code |
| `cloud_cover` | double | % | Cloud coverage |
| `pressure_msl` | double | hPa | Sea level pressure |
| `surface_pressure` | double | hPa | Surface pressure |
| `is_day` | uint32 | — | 1 = day, 0 = night |

### HourlyForecast

Hourly weather forecast data.

```protobuf
// Single hourly forecast entry
message HourlyForecastEntry {
    uint64 time = 1;                    // Unix timestamp (seconds)
    double temperature_2m = 2;
    double relative_humidity_2m = 3;
    double precipitation_probability = 4;
    double precipitation = 5;
    uint32 weather_code = 6;
    double wind_speed_10m = 7;
    double wind_direction_10m = 8;
    double cloud_cover = 9;
}

// Hourly forecast data
message HourlyForecast {
    bubbaloop.header.v1.Header header = 1;

    // Location information
    double latitude = 2;
    double longitude = 3;
    string timezone = 4;

    // Forecast entries (typically 24-168 hours)
    repeated HourlyForecastEntry entries = 5;
}
```

#### HourlyForecastEntry Fields

| Field | Type | Unit | Description |
|-------|------|------|-------------|
| `time` | uint64 | s | Unix timestamp |
| `temperature_2m` | double | °C | Temperature |
| `relative_humidity_2m` | double | % | Humidity |
| `precipitation_probability` | double | % | Rain probability |
| `precipitation` | double | mm | Expected precipitation |
| `weather_code` | uint32 | WMO | Weather condition |
| `wind_speed_10m` | double | km/h | Wind speed |
| `wind_direction_10m` | double | ° | Wind direction |
| `cloud_cover` | double | % | Cloud coverage |

### DailyForecast

Daily weather forecast data.

```protobuf
// Single daily forecast entry
message DailyForecastEntry {
    uint64 time = 1;                    // Unix timestamp (seconds) - start of day
    double temperature_2m_max = 2;
    double temperature_2m_min = 3;
    double precipitation_sum = 4;
    double precipitation_probability_max = 5;
    uint32 weather_code = 6;
    double wind_speed_10m_max = 7;
    double wind_gusts_10m_max = 8;
    string sunrise = 9;                 // ISO 8601 time
    string sunset = 10;                 // ISO 8601 time
}

// Daily forecast data
message DailyForecast {
    bubbaloop.header.v1.Header header = 1;

    // Location information
    double latitude = 2;
    double longitude = 3;
    string timezone = 4;

    // Forecast entries (typically 7-16 days)
    repeated DailyForecastEntry entries = 5;
}
```

#### DailyForecastEntry Fields

| Field | Type | Unit | Description |
|-------|------|------|-------------|
| `time` | uint64 | s | Day start timestamp |
| `temperature_2m_max` | double | °C | Maximum temperature |
| `temperature_2m_min` | double | °C | Minimum temperature |
| `precipitation_sum` | double | mm | Total precipitation |
| `precipitation_probability_max` | double | % | Max rain probability |
| `weather_code` | uint32 | WMO | Dominant weather |
| `wind_speed_10m_max` | double | km/h | Maximum wind speed |
| `wind_gusts_10m_max` | double | km/h | Maximum wind gusts |
| `sunrise` | string | ISO 8601 | Sunrise time |
| `sunset` | string | ISO 8601 | Sunset time |

### LocationConfig

Configuration message for updating location dynamically.

```protobuf
// Location configuration for setting weather location
message LocationConfig {
    double latitude = 1;
    double longitude = 2;
    string timezone = 3;     // Optional timezone (e.g., "Europe/Madrid")
}
```

#### Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `latitude` | double | Yes | Latitude in decimal degrees |
| `longitude` | double | Yes | Longitude in decimal degrees |
| `timezone` | string | No | IANA timezone identifier |

## Topics

### Published Topics

| Topic | Message | Interval |
|-------|---------|----------|
| `/weather/current` | `CurrentWeather` | 30s |
| `/weather/hourly` | `HourlyForecast` | 30min |
| `/weather/daily` | `DailyForecast` | 3h |

### Subscribed Topics

| Topic | Message | Purpose |
|-------|---------|---------|
| `/weather/location` | `LocationConfig` | Update location |

### Zenoh Key Expressions

```
0/weather%current/**    # Current conditions
0/weather%hourly/**     # Hourly forecast
0/weather%daily/**      # Daily forecast
0/weather%location/**   # Location config
```

## WMO Weather Codes

The `weather_code` field uses WMO (World Meteorological Organization) codes:

| Code | Description |
|------|-------------|
| 0 | Clear sky |
| 1 | Mainly clear |
| 2 | Partly cloudy |
| 3 | Overcast |
| 45 | Fog |
| 48 | Depositing rime fog |
| 51 | Light drizzle |
| 53 | Moderate drizzle |
| 55 | Dense drizzle |
| 61 | Slight rain |
| 63 | Moderate rain |
| 65 | Heavy rain |
| 71 | Slight snow |
| 73 | Moderate snow |
| 75 | Heavy snow |
| 77 | Snow grains |
| 80 | Slight rain showers |
| 81 | Moderate rain showers |
| 82 | Violent rain showers |
| 85 | Slight snow showers |
| 86 | Heavy snow showers |
| 95 | Thunderstorm |
| 96 | Thunderstorm with slight hail |
| 99 | Thunderstorm with heavy hail |

## Usage Examples

### Rust - Subscribing

```rust
use bubbaloop_protos::weather::v1::CurrentWeather;
use prost::Message;

async fn handle_weather(data: &[u8]) -> Result<()> {
    let weather = CurrentWeather::decode(data)?;

    println!("Temperature: {:.1}°C", weather.temperature_2m);
    println!("Humidity: {:.0}%", weather.relative_humidity_2m);
    println!("Conditions: {}", weather_code_to_string(weather.weather_code));

    Ok(())
}
```

### TypeScript - Displaying

```typescript
import { CurrentWeather, HourlyForecast } from './proto/weather';

function displayWeather(weather: CurrentWeather) {
    const temp = weather.temperature2m.toFixed(1);
    const humidity = weather.relativeHumidity2m.toFixed(0);
    const isDay = weather.isDay === 1;

    console.log(`${temp}°C, ${humidity}% humidity`);
    console.log(`Time: ${isDay ? 'Day' : 'Night'}`);
}

function displayForecast(forecast: HourlyForecast) {
    for (const entry of forecast.entries) {
        const time = new Date(Number(entry.time) * 1000);
        console.log(`${time.toLocaleTimeString()}: ${entry.temperature2m}°C`);
    }
}
```

### Python - Updating Location

```python
from bubbaloop.weather.v1 import LocationConfig
import zenoh

async def update_location(session, lat: float, lon: float, tz: str):
    config = LocationConfig(
        latitude=lat,
        longitude=lon,
        timezone=tz,
    )

    publisher = await session.declare_publisher("0/weather%location")
    await publisher.put(config.SerializeToString())
```

## Wind Direction

Wind direction is given in degrees from North:

| Degrees | Direction |
|---------|-----------|
| 0 / 360 | North |
| 45 | Northeast |
| 90 | East |
| 135 | Southeast |
| 180 | South |
| 225 | Southwest |
| 270 | West |
| 315 | Northwest |

## Next Steps

- [Header](header.md) — Common header fields
- [Camera Messages](camera.md) — Camera API
- [OpenMeteo Service](../components/services/openmeteo.md) — Service configuration
- [Weather Panel](../dashboard/panels/weather.md) — Dashboard visualization
