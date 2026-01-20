# Topics

Bubbaloop uses ROS-Z topic conventions for all message routing.

## Topic Naming

### Format

Topics follow a hierarchical naming pattern:

```
/{category}/{name}/{type}
```

**Examples:**

| Topic | Description |
|-------|-------------|
| `/camera/front/compressed` | Front camera compressed images |
| `/camera/back/compressed` | Back camera compressed images |
| `/weather/current` | Current weather conditions |
| `/weather/hourly` | Hourly weather forecast |
| `/weather/daily` | Daily weather forecast |

### Zenoh Key Expression

ROS topics are converted to Zenoh key expressions:

| ROS Topic | Zenoh Key |
|-----------|-----------|
| `/camera/front/compressed` | `0/camera%front%compressed/**` |
| `/weather/current` | `0/weather%current/**` |

**Conversion:**

1. Remove leading `/`
2. Replace `/` with `%`
3. Add prefix `0/` (namespace)
4. Add suffix `/**` (wildcard)

## Camera Topics

Each camera publishes to its own topic based on the `name` field in the configuration.

### Published Topics

| Topic | Message Type | Description |
|-------|--------------|-------------|
| `/camera/{name}/compressed` | `CompressedImage` | H264 compressed frames |

### Topic Pattern

```
/camera/{camera_name}/compressed
```

**Examples:**

| Config Name | ROS Topic | Zenoh Key |
|-------------|-----------|-----------|
| `front_door` | `/camera/front_door/compressed` | `0/camera%front_door%compressed/**` |
| `backyard` | `/camera/backyard/compressed` | `0/camera%backyard%compressed/**` |
| `garage` | `/camera/garage/compressed` | `0/camera%garage%compressed/**` |

### Message Format

See [Camera Messages](../api/camera.md) for the `CompressedImage` definition.

## Weather Topics

The OpenMeteo service publishes weather data to multiple topics.

### Published Topics

| Topic | Message Type | Description |
|-------|--------------|-------------|
| `/weather/current` | `CurrentWeather` | Current conditions |
| `/weather/hourly` | `HourlyForecast` | Hourly forecast (48h) |
| `/weather/daily` | `DailyForecast` | Daily forecast (7 days) |

### Subscribed Topics

| Topic | Message Type | Description |
|-------|--------------|-------------|
| `/weather/location` | `LocationConfig` | Update location dynamically |

### Message Format

See [Weather Messages](../api/weather.md) for message definitions.

## Topic Discovery

### In the Dashboard

The dashboard automatically discovers available topics:

1. Click **Add Panel** or **Add Camera**
2. Click the edit icon
3. Select from discovered topics
4. Or enter a custom topic pattern

### In the TUI

Use the `/topics` command to list active topics:

```
/topics
```

This shows:

- Topic name
- Message frequency (Hz)
- Message count

## Wildcard Subscriptions

Zenoh supports wildcard subscriptions for monitoring multiple topics:

| Pattern | Matches |
|---------|---------|
| `0/camera%**` | All camera topics |
| `0/weather%**` | All weather topics |
| `0/**` | All topics |

**Example: Subscribe to all cameras**

```typescript
const subscriber = session.declareSubscriber("0/camera%**");
```

## Topic Namespacing

The `0/` prefix is the ROS-Z namespace. Future versions may support:

| Prefix | Purpose |
|--------|---------|
| `0/` | Default namespace |
| `robot1/` | Robot-specific namespace |
| `sim/` | Simulation namespace |

## Custom Topics

When creating custom components, follow these conventions:

### Naming Guidelines

1. Use lowercase with underscores
2. Be descriptive but concise
3. Include component category
4. Include data type

**Good examples:**

- `/sensor/imu/data`
- `/actuator/motor/velocity`
- `/service/detector/results`

**Avoid:**

- `/MyCamera` (mixed case)
- `/cam1` (not descriptive)
- `/data` (too generic)

### Publishing Custom Topics

```rust
// Rust example
let topic = format!("/sensor/{}/data", sensor_name);
let key = topic_to_zenoh_key(&topic);
let publisher = session.declare_publisher(key).await?;
```

## Next Steps

- [API Reference](../api/index.md) — Message type definitions
- [Camera Messages](../api/camera.md) — Camera message format
- [Weather Messages](../api/weather.md) — Weather message format
