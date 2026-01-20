# Weather Panel

The Weather panel displays current conditions and forecasts from the OpenMeteo weather service.

## Overview

| Property | Value |
|----------|-------|
| Input | `CurrentWeather`, `HourlyForecast`, `DailyForecast` |
| Topics | `/weather/current`, `/weather/hourly`, `/weather/daily` |
| Source | OpenMeteo API |

## Features

- **Current conditions** — Temperature, humidity, wind, conditions
- **Weather icons** — Visual representation based on WMO codes
- **Hourly forecast** — Temperature and precipitation chart
- **Daily forecast** — Multi-day outlook
- **Location display** — Coordinates and timezone
- **Day/night indicator** — Sun/moon icon

## Panel Interface

### Current Conditions

The main display shows:

| Element | Description |
|---------|-------------|
| **Temperature** | Current temperature in Celsius |
| **Feels like** | Apparent temperature |
| **Weather icon** | Based on WMO weather code |
| **Conditions** | Text description |
| **Humidity** | Relative humidity percentage |
| **Wind** | Speed and direction |
| **Cloud cover** | Percentage |

### Hourly Forecast

Below the current conditions:

- Temperature trend chart
- Precipitation probability
- Next 12-24 hours preview

### Daily Forecast

Summary of upcoming days:

- High/low temperatures
- Weather condition icons
- Precipitation summary
- Sunrise/sunset times

## Adding a Weather Panel

The Weather panel subscribes to weather topics automatically:

1. Click **Add Panel**
2. Select "Weather" panel type
3. Panel auto-subscribes to weather topics

Or manually configure:

1. Click the edit icon
2. Enter: `0/weather%current/**`
3. Click **Save**

## Weather Icons

Icons are based on WMO weather codes:

| Code | Icon | Description |
|------|------|-------------|
| 0 | ☀️ | Clear sky |
| 1-2 | 🌤️ | Mainly clear |
| 3 | ☁️ | Overcast |
| 45, 48 | 🌫️ | Fog |
| 51-55 | 🌧️ | Drizzle |
| 61-65 | 🌧️ | Rain |
| 71-75 | 🌨️ | Snow |
| 80-82 | 🌦️ | Rain showers |
| 95+ | ⛈️ | Thunderstorm |

### Day/Night

Icons adapt to time of day:

- Day: ☀️ Sun icon
- Night: 🌙 Moon icon

## Data Updates

### Update Intervals

| Data | Interval |
|------|----------|
| Current | Every 30 seconds |
| Hourly | Every 30 minutes |
| Daily | Every 3 hours |

### Real-time Updates

The panel updates automatically when new data arrives from the OpenMeteo service.

## Location Information

The panel displays location metadata:

| Field | Description |
|-------|-------------|
| Coordinates | Latitude, longitude |
| Timezone | IANA timezone identifier |
| Elevation | Above sea level (if available) |

## Configuration

### Topics

The Weather panel subscribes to:

```
0/weather%current/**    # Current conditions
0/weather%hourly/**     # Hourly forecast
0/weather%daily/**      # Daily forecast
```

### Changing Location

To change the weather location:

1. Update the OpenMeteo configuration file
2. Restart the weather service

Or send a `LocationConfig` message to `/weather/location`.

See [OpenMeteo Configuration](../../components/services/openmeteo.md) for details.

## Data Fields

### Current Weather

| Field | Unit | Description |
|-------|------|-------------|
| `temperature_2m` | °C | Temperature |
| `apparent_temperature` | °C | Feels like |
| `relative_humidity_2m` | % | Humidity |
| `wind_speed_10m` | km/h | Wind speed |
| `wind_direction_10m` | ° | Wind direction |
| `precipitation` | mm | Current precipitation |
| `cloud_cover` | % | Cloud coverage |
| `weather_code` | WMO | Condition code |

### Forecast Data

See [Weather API](../../api/weather.md) for complete field documentation.

## Troubleshooting

### No weather data

1. Verify OpenMeteo service is running: `pixi run weather`
2. Check `/topics` in TUI for weather topics
3. Verify internet connectivity

### Wrong location

1. Check OpenMeteo configuration
2. Disable `auto_discover` for explicit coordinates
3. Update location via topic message

### Stale data

1. Check weather service logs
2. Verify fetch intervals in configuration
3. Check for API rate limiting

### Icons not displaying

1. Verify weather code is valid
2. Check browser console for errors
3. Refresh the dashboard

## Next Steps

- [OpenMeteo Service](../../components/services/openmeteo.md) — Weather service configuration
- [Weather API](../../api/weather.md) — Message format details
- [Dashboard Overview](../index.md) — Other panel types
