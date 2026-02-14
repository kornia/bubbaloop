import { useCallback, useState, useEffect } from 'react';
import { Sample, IntoZBytes } from '@eclipse-zenoh/zenoh-ts';
import { getSamplePayload, extractMachineId } from '../lib/zenoh';
import { useZenohSubscription } from '../hooks/useZenohSubscription';
import { useSchemaReady } from '../hooks/useSchemaReady';
import { useZenohSubscriptionContext } from '../contexts/ZenohSubscriptionContext';
import { useFleetContext } from '../contexts/FleetContext';
import { useSchemaRegistry } from '../contexts/SchemaRegistryContext';
import { MachineBadge } from './MachineBadge';

// Local interfaces matching the protobuf schema shapes (decoded via SchemaRegistry)
interface CurrentWeather {
  latitude: number;
  longitude: number;
  timezone: string;
  temperature_2m: number;
  relativeHumidity_2m: number;
  apparentTemperature: number;
  precipitation: number;
  rain: number;
  windSpeed_10m: number;
  windDirection_10m: number;
  windGusts_10m: number;
  weatherCode: number;
  cloudCover: number;
  pressureMsl: number;
  surfacePressure: number;
  isDay: number;
}

interface HourlyForecastEntry {
  time: string; // String from SchemaRegistry (longs: String)
  temperature_2m: number;
  relativeHumidity_2m: number;
  precipitationProbability: number;
  precipitation: number;
  weatherCode: number;
  windSpeed_10m: number;
  windDirection_10m: number;
  cloudCover: number;
}

interface HourlyForecast {
  latitude: number;
  longitude: number;
  timezone: string;
  entries: HourlyForecastEntry[];
}

interface DailyForecastEntry {
  time: string;
  temperature_2mMax: number;
  temperature_2mMin: number;
  precipitationSum: number;
  precipitationProbabilityMax: number;
  weatherCode: number;
  windSpeed_10mMax: number;
  windGusts_10mMax: number;
  sunrise: string;
  sunset: string;
}

interface DailyForecast {
  latitude: number;
  longitude: number;
  timezone: string;
  entries: DailyForecastEntry[];
}

// Weather code to description and emoji
function getWeatherDescription(code: number): { text: string; emoji: string } {
  if (code === 0) return { text: 'Clear sky', emoji: '\u2600\uFE0F' };
  if (code >= 1 && code <= 3) return { text: 'Partly cloudy', emoji: '\u26C5' };
  if (code >= 45 && code <= 48) return { text: 'Fog', emoji: '\uD83C\uDF2B\uFE0F' };
  if (code >= 51 && code <= 57) return { text: 'Drizzle', emoji: '\uD83C\uDF27\uFE0F' };
  if (code >= 61 && code <= 67) return { text: 'Rain', emoji: '\uD83C\uDF27\uFE0F' };
  if (code >= 71 && code <= 77) return { text: 'Snow', emoji: '\u2744\uFE0F' };
  if (code >= 80 && code <= 82) return { text: 'Rain showers', emoji: '\uD83C\uDF26\uFE0F' };
  if (code >= 85 && code <= 86) return { text: 'Snow showers', emoji: '\uD83C\uDF28\uFE0F' };
  if (code >= 95 && code <= 99) return { text: 'Thunderstorm', emoji: '\u26C8\uFE0F' };
  return { text: 'Unknown', emoji: '\u2753' };
}

// Wind direction to compass
function getWindDirection(degrees: number): string {
  const directions = ['N', 'NE', 'E', 'SE', 'S', 'SW', 'W', 'NW'];
  const index = Math.round(degrees / 45) % 8;
  return directions[index];
}

interface DragHandleProps {
  [key: string]: unknown;
}

interface WeatherViewPanelProps {
  topic?: string; // Not used - weather topics are fixed
  onRemove?: () => void;
  dragHandleProps?: DragHandleProps;
}

// Timestamps come as string nanoseconds from SchemaRegistry (longs: String)
function formatDate(timestamp: string): string {
  const ns = BigInt(timestamp || '0');
  const date = new Date(Number(ns / 1000000n));
  return date.toLocaleDateString('en-US', { weekday: 'short', month: 'short', day: 'numeric' });
}

function formatHour(timestamp: string): string {
  const ns = BigInt(timestamp || '0');
  const date = new Date(Number(ns / 1000000n));
  return date.toLocaleTimeString('en-US', { hour: 'numeric' });
}

export function WeatherViewPanel({
  onRemove,
  dragHandleProps,
}: WeatherViewPanelProps) {
  const { machines, selectedMachineId } = useFleetContext();
  const { registry, discoverForTopic } = useSchemaRegistry();
  const schemaReady = useSchemaReady();
  // Get session from context for publishing
  const { getSession } = useZenohSubscriptionContext();
  // Store weather data per machine with last update timestamp
  const [currentMap, setCurrentMap] = useState<Map<string, { data: CurrentWeather; lastUpdate: number }>>(new Map());
  const [hourlyMap, setHourlyMap] = useState<Map<string, { data: HourlyForecast; lastUpdate: number }>>(new Map());
  const [dailyMap, setDailyMap] = useState<Map<string, { data: DailyForecast; lastUpdate: number }>>(new Map());

  const [isEditing, setIsEditing] = useState(false);

  // Location editing state
  const [editLatitude, setEditLatitude] = useState('');
  const [editLongitude, setEditLongitude] = useState('');
  const [isSendingLocation, setIsSendingLocation] = useState(false);
  const [locationUpdateStatus, setLocationUpdateStatus] = useState<'idle' | 'sent' | 'error'>('idle');

  // Match any machine/scope: */*/TypeName/* (ros-z key format: domain/topic%encoded/type/hash)
  const currentTopic = '*/*/bubbaloop.weather.v1.CurrentWeather/*';
  const hourlyTopic = '*/*/bubbaloop.weather.v1.HourlyForecast/*';
  const dailyTopic = '*/*/bubbaloop.weather.v1.DailyForecast/*';

  // Handle current weather samples
  const handleCurrentSample = useCallback((sample: Sample) => {
    try {
      const payload = getSamplePayload(sample);
      const topic = sample.keyexpr().toString();
      const machineId = extractMachineId(topic) ?? 'unknown';

      const result = registry.decode('bubbaloop.weather.v1.CurrentWeather', payload);
      if (result) {
        const data = result.data as unknown as CurrentWeather;
        setCurrentMap(prev => {
          const next = new Map(prev);
          next.set(machineId, { data, lastUpdate: Date.now() });
          return next;
        });
      } else {
        discoverForTopic(topic);
      }
    } catch (e) {
      console.error('[WeatherView] Failed to decode current weather:', e);
    }
  }, [registry, discoverForTopic]);

  // Handle hourly forecast samples
  const handleHourlySample = useCallback((sample: Sample) => {
    try {
      const payload = getSamplePayload(sample);
      const topic = sample.keyexpr().toString();
      const machineId = extractMachineId(topic) ?? 'unknown';

      const result = registry.decode('bubbaloop.weather.v1.HourlyForecast', payload);
      if (result) {
        const data = result.data as unknown as HourlyForecast;
        setHourlyMap(prev => {
          const next = new Map(prev);
          next.set(machineId, { data, lastUpdate: Date.now() });
          return next;
        });
      } else {
        discoverForTopic(topic);
      }
    } catch (e) {
      console.error('[WeatherView] Failed to decode hourly forecast:', e);
    }
  }, [registry, discoverForTopic]);

  // Handle daily forecast samples
  const handleDailySample = useCallback((sample: Sample) => {
    try {
      const payload = getSamplePayload(sample);
      const topic = sample.keyexpr().toString();
      const machineId = extractMachineId(topic) ?? 'unknown';

      const result = registry.decode('bubbaloop.weather.v1.DailyForecast', payload);
      if (result) {
        const data = result.data as unknown as DailyForecast;
        setDailyMap(prev => {
          const next = new Map(prev);
          next.set(machineId, { data, lastUpdate: Date.now() });
          return next;
        });
      } else {
        discoverForTopic(topic);
      }
    } catch (e) {
      console.error('[WeatherView] Failed to decode daily forecast:', e);
    }
  }, [registry, discoverForTopic]);

  // Subscribe to all three topics — gate callbacks on schema readiness
  const { messageCount: currentCount } = useZenohSubscription(currentTopic, schemaReady ? handleCurrentSample : undefined);
  const { messageCount: hourlyCount } = useZenohSubscription(hourlyTopic, schemaReady ? handleHourlySample : undefined);
  const { messageCount: dailyCount } = useZenohSubscription(dailyTopic, schemaReady ? handleDailySample : undefined);

  const totalMessages = currentCount + hourlyCount + dailyCount;

  const handleCloseEdit = () => {
    setIsEditing(false);
  };

  // Initialize location edit fields from current weather data
  // Use weather from selected machine or first available
  useEffect(() => {
    if (!isEditing) return;

    let currentWeather: CurrentWeather | null = null;

    if (selectedMachineId) {
      const entry = currentMap.get(selectedMachineId);
      if (entry) currentWeather = entry.data;
    } else {
      // Use first available machine
      const firstEntry = currentMap.values().next().value;
      if (firstEntry) currentWeather = firstEntry.data;
    }

    if (currentWeather) {
      setEditLatitude(currentWeather.latitude.toFixed(4));
      setEditLongitude(currentWeather.longitude.toFixed(4));
    }
  }, [currentMap, selectedMachineId, isEditing]);

  // Send location update to server via Zenoh publish
  const handleSendLocation = useCallback(async () => {
    const session = getSession();
    if (!session) {
      console.error('[WeatherView] No session available for publishing');
      setLocationUpdateStatus('error');
      return;
    }

    const lat = parseFloat(editLatitude);
    const lon = parseFloat(editLongitude);

    if (isNaN(lat) || isNaN(lon)) {
      console.error('[WeatherView] Invalid coordinates');
      setLocationUpdateStatus('error');
      return;
    }

    if (lat < -90 || lat > 90) {
      console.error('[WeatherView] Latitude must be between -90 and 90');
      setLocationUpdateStatus('error');
      return;
    }

    if (lon < -180 || lon > 180) {
      console.error('[WeatherView] Longitude must be between -180 and 180');
      setLocationUpdateStatus('error');
      return;
    }

    setIsSendingLocation(true);
    setLocationUpdateStatus('idle');

    try {
      // Encode the location config message using SchemaRegistry
      const locType = registry.lookupType('bubbaloop.weather.v1.LocationConfig');
      if (!locType) {
        console.error('[WeatherView] LocationConfig schema not loaded');
        setLocationUpdateStatus('error');
        setIsSendingLocation(false);
        return;
      }
      const payload = locType.encode(locType.create({
        latitude: lat,
        longitude: lon,
        timezone: '',
      })).finish();

      // Publish to the config topic using ros-z format
      // The topic format is: domain_id/topic_name/schema/hash
      const configKey = '0/weather%config%location/bubbaloop.weather.v1.LocationConfig/RIHS01_0000000000000000000000000000000000000000000000000000000000000000';

      console.log('[WeatherView] Publishing location update to:', configKey);
      console.log('[WeatherView] Location:', { lat, lon });

      // Get publisher and send
      const publisher = await session.declarePublisher(configKey);
      await publisher.put(payload as unknown as IntoZBytes);
      await publisher.undeclare();

      console.log('[WeatherView] Location update sent successfully');
      setLocationUpdateStatus('sent');

      // Reset status after a few seconds
      setTimeout(() => setLocationUpdateStatus('idle'), 3000);
    } catch (error) {
      console.error('[WeatherView] Failed to send location update:', error);
      setLocationUpdateStatus('error');
    } finally {
      setIsSendingLocation(false);
    }
  }, [getSession, editLatitude, editLongitude, registry]);

  const renderCurrentWeather = (weather: CurrentWeather) => {
    const desc = getWeatherDescription(weather.weatherCode);
    return (
      <div className="weather-current">
        <div className="weather-main">
          <div className="weather-temp">
            <span className="weather-emoji">{desc.emoji}</span>
            <span className="temp-value">{weather.temperature_2m.toFixed(1)}°C</span>
          </div>
          <div className="weather-desc">{desc.text}</div>
          {weather.timezone && <div className="weather-location">{weather.timezone}</div>}
          {(weather.latitude !== 0 || weather.longitude !== 0) && (
            <div className="weather-coords">
              {weather.latitude.toFixed(4)}°N, {weather.longitude.toFixed(4)}°E
            </div>
          )}
        </div>
        <div className="weather-details">
          <div className="detail-row">
            <span className="detail-label">Feels like</span>
            <span className="detail-value">{weather.apparentTemperature.toFixed(1)}°C</span>
          </div>
          <div className="detail-row">
            <span className="detail-label">Humidity</span>
            <span className="detail-value">{weather.relativeHumidity_2m}%</span>
          </div>
          <div className="detail-row">
            <span className="detail-label">Wind</span>
            <span className="detail-value">
              {weather.windSpeed_10m.toFixed(1)} km/h {getWindDirection(weather.windDirection_10m)}
            </span>
          </div>
          {weather.windGusts_10m > 0 && (
            <div className="detail-row">
              <span className="detail-label">Gusts</span>
              <span className="detail-value">{weather.windGusts_10m.toFixed(1)} km/h</span>
            </div>
          )}
          <div className="detail-row">
            <span className="detail-label">Pressure</span>
            <span className="detail-value">{weather.pressureMsl.toFixed(0)} hPa</span>
          </div>
          {weather.precipitation > 0 && (
            <div className="detail-row">
              <span className="detail-label">Rain</span>
              <span className="detail-value">{weather.precipitation.toFixed(1)} mm</span>
            </div>
          )}
        </div>
      </div>
    );
  };

  const renderHourlyForecast = (forecast: HourlyForecast) => {
    // Show next 12 hours
    const hours = forecast.entries.slice(0, 12);
    return (
      <div className="weather-hourly">
        <div className="section-title">Hourly Forecast</div>
        <div className="forecast-scroll">
          {hours.map((hour, i) => {
            const desc = getWeatherDescription(hour.weatherCode);
            return (
              <div key={i} className="hour-item">
                <div className="hour-time">{formatHour(hour.time)}</div>
                <div className="hour-emoji">{desc.emoji}</div>
                <div className="hour-temp">{hour.temperature_2m.toFixed(0)}°</div>
                {hour.precipitationProbability > 0 && (
                  <div className="hour-rain">💧{hour.precipitationProbability}%</div>
                )}
              </div>
            );
          })}
        </div>
      </div>
    );
  };

  const renderDailyForecast = (forecast: DailyForecast) => {
    return (
      <div className="weather-daily">
        <div className="section-title">7-Day Forecast</div>
        {forecast.entries.map((day, i) => {
          const desc = getWeatherDescription(day.weatherCode);
          return (
            <div key={i} className="day-item">
              <div className="day-date">{formatDate(day.time)}</div>
              <div className="day-weather">
                <span className="day-emoji">{desc.emoji}</span>
                <span className="day-desc">{desc.text}</span>
              </div>
              <div className="day-temps">
                <span className="temp-max">{day.temperature_2mMax.toFixed(0)}°</span>
                <span className="temp-min">{day.temperature_2mMin.toFixed(0)}°</span>
              </div>
              {day.precipitationSum > 0 && (
                <div className="day-rain">💧 {day.precipitationSum.toFixed(1)}mm</div>
              )}
            </div>
          );
        })}
      </div>
    );
  };

  // Filter maps by selectedMachineId
  const visibleMachineIds = selectedMachineId
    ? [selectedMachineId]
    : Array.from(new Set([...currentMap.keys(), ...hourlyMap.keys(), ...dailyMap.keys()]));

  const hasAnyData = currentMap.size > 0 || hourlyMap.size > 0 || dailyMap.size > 0;

  return (
    <div className="weather-view-panel">
      <div className="panel-header">
        <div className="panel-header-left">
          {dragHandleProps && (
            <button className="drag-handle" title="Drag to reorder" {...dragHandleProps}>
              <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
                <circle cx="5" cy="4" r="1.5" />
                <circle cx="11" cy="4" r="1.5" />
                <circle cx="5" cy="8" r="1.5" />
                <circle cx="11" cy="8" r="1.5" />
                <circle cx="5" cy="12" r="1.5" />
                <circle cx="11" cy="12" r="1.5" />
              </svg>
            </button>
          )}
          <span className="panel-type-badge">WEATHER</span>
          <MachineBadge machines={machines} />
        </div>
        <div className="panel-stats">
          <span className="stat">
            <span className="stat-value mono">{totalMessages.toLocaleString()}</span>
            <span className="stat-label">msgs</span>
          </span>
          <button className="icon-btn" onClick={() => setIsEditing(!isEditing)} title="Edit location">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M21 10c0 7-9 13-9 13s-9-6-9-13a9 9 0 0118 0z" />
              <circle cx="12" cy="10" r="3" />
            </svg>
          </button>
          {onRemove && (
            <button className="icon-btn danger" onClick={onRemove} title="Remove panel">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M18 6L6 18M6 6l12 12" />
              </svg>
            </button>
          )}
        </div>
      </div>

      <div className="weather-content-container">
        {!hasAnyData ? (
          <div className="weather-waiting">
            <div className="spinner" />
            <span>Waiting for weather data...</span>
            <span className="waiting-hint">(Current updates every ~30s, forecasts less frequently)</span>
            {totalMessages > 0 && (
              <span className="waiting-received">Received {totalMessages} messages</span>
            )}
          </div>
        ) : (
          <div className="weather-content">
            {visibleMachineIds.map((machineId) => {
              const current = currentMap.get(machineId)?.data;
              const hourly = hourlyMap.get(machineId)?.data;
              const daily = dailyMap.get(machineId)?.data;

              if (!current && !hourly && !daily) return null;

              return (
                <div key={machineId} className="machine-weather-section">
                  {visibleMachineIds.length > 1 && (
                    <div className="machine-weather-header">
                      <span className="machine-name">{machineId}</span>
                    </div>
                  )}
                  {current && renderCurrentWeather(current)}
                  {hourly && renderHourlyForecast(hourly)}
                  {daily && renderDailyForecast(daily)}
                </div>
              );
            })}
          </div>
        )}
      </div>

      <div className="panel-footer">
        <span className="footer-info">weather/current, weather/hourly, weather/daily</span>
      </div>

      {isEditing && (
        <div className="panel-edit-footer">
          <div className="edit-section">
            <span className="section-label">Update Location</span>
            <div className="location-edit-form">
              <div className="coord-input-row">
                <label htmlFor="lat-input">Latitude:</label>
                <input
                  id="lat-input"
                  type="number"
                  step="0.0001"
                  min="-90"
                  max="90"
                  value={editLatitude}
                  onChange={(e) => setEditLatitude(e.target.value)}
                  className="coord-input"
                  placeholder="41.4167"
                />
              </div>
              <div className="coord-input-row">
                <label htmlFor="lon-input">Longitude:</label>
                <input
                  id="lon-input"
                  type="number"
                  step="0.0001"
                  min="-180"
                  max="180"
                  value={editLongitude}
                  onChange={(e) => setEditLongitude(e.target.value)}
                  className="coord-input"
                  placeholder="1.9667"
                />
              </div>
              <button
                className="btn-update-location"
                onClick={handleSendLocation}
                disabled={isSendingLocation}
              >
                {isSendingLocation ? 'Sending...' : 'Update Location'}
              </button>
              {locationUpdateStatus === 'sent' && (
                <div className="location-status success">Location update sent!</div>
              )}
              {locationUpdateStatus === 'error' && (
                <div className="location-status error">Failed to send location update</div>
              )}
            </div>
            <div className="config-hint">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <circle cx="12" cy="12" r="10"/>
                <line x1="12" y1="16" x2="12" y2="12"/>
                <line x1="12" y1="8" x2="12.01" y2="8"/>
              </svg>
              <span>
                Changes are applied in real-time. Timezone is auto-detected based on coordinates.
              </span>
            </div>
          </div>
          <div className="edit-actions">
            <button className="btn-secondary" onClick={handleCloseEdit}>Close</button>
          </div>
        </div>
      )}

      <style>{`
        .weather-view-panel {
          background: var(--bg-card);
          border: 1px solid var(--border-color);
          border-radius: 12px;
          overflow: hidden;
          display: flex;
          flex-direction: column;
          transition: border-color 0.2s, box-shadow 0.2s;
          min-width: 0;
          max-width: 100%;
        }

        .weather-view-panel:hover {
          border-color: var(--border-glow);
          box-shadow: var(--shadow-glow);
        }

        .weather-view-panel.maximized {
          border-color: var(--accent-primary);
        }

        .panel-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
          padding: 10px 12px;
          background: var(--bg-tertiary);
          border-bottom: 1px solid var(--border-color);
          gap: 8px;
        }

        .panel-header-left {
          display: flex;
          align-items: center;
          gap: 8px;
          min-width: 0;
          flex: 1;
        }

        .drag-handle {
          display: flex;
          align-items: center;
          justify-content: center;
          width: 24px;
          height: 24px;
          background: transparent;
          border: none;
          color: var(--text-muted);
          cursor: grab;
          border-radius: 4px;
          flex-shrink: 0;
        }

        .drag-handle:hover {
          background: var(--bg-primary);
          color: var(--text-secondary);
        }

        .drag-handle:active {
          cursor: grabbing;
        }

        .panel-name {
          font-weight: 600;
          font-size: 14px;
          color: var(--text-primary);
          white-space: nowrap;
          overflow: hidden;
          text-overflow: ellipsis;
        }

        .panel-name-input {
          font-weight: 600;
          font-size: 14px;
          color: var(--text-primary);
          background: var(--bg-primary);
          border: 1px solid var(--accent-primary);
          border-radius: 4px;
          padding: 4px 8px;
          min-width: 100px;
          flex: 1;
        }

        .panel-type-badge {
          padding: 2px 8px;
          border-radius: 4px;
          font-size: 10px;
          font-weight: 600;
          letter-spacing: 0.5px;
          background: rgba(255, 214, 0, 0.15);
          color: #ffd600;
          text-transform: uppercase;
          white-space: nowrap;
          flex-shrink: 0;
        }

        .panel-machine-badge {
          font-size: 10px;
          font-family: 'JetBrains Mono', monospace;
          color: var(--text-muted);
          background: var(--bg-tertiary);
          padding: 2px 6px;
          border-radius: 4px;
          max-width: 120px;
          overflow: hidden;
          text-overflow: ellipsis;
          white-space: nowrap;
        }

        .panel-stats {
          display: flex;
          gap: 8px;
          align-items: center;
          flex-shrink: 0;
        }

        .stat {
          display: flex;
          align-items: baseline;
          gap: 4px;
        }

        .stat-value {
          font-weight: 600;
          font-size: 13px;
          color: var(--accent-secondary);
        }

        .stat-label {
          font-size: 10px;
          color: var(--text-muted);
          text-transform: uppercase;
          letter-spacing: 0.5px;
        }

        .icon-btn {
          display: flex;
          align-items: center;
          justify-content: center;
          width: 32px;
          height: 32px;
          background: transparent;
          border: 1px solid var(--border-color);
          border-radius: 6px;
          color: var(--text-secondary);
          cursor: pointer;
          transition: all 0.15s;
          flex-shrink: 0;
        }

        .icon-btn:hover {
          background: var(--bg-primary);
          border-color: var(--text-muted);
          color: var(--text-primary);
        }

        .icon-btn.danger:hover {
          background: rgba(255, 23, 68, 0.1);
          border-color: var(--error);
          color: var(--error);
        }

        .weather-content-container {
          position: relative;
          aspect-ratio: 16 / 9;
          min-height: 240px;
          overflow-y: auto;
          overflow-x: hidden;
          background: var(--bg-primary);
        }

        .weather-panel.maximized .weather-content-container {
          aspect-ratio: unset;
          flex: 1;
          min-height: 400px;
        }

        .weather-waiting {
          position: absolute;
          inset: 0;
          display: flex;
          flex-direction: column;
          align-items: center;
          justify-content: center;
          color: var(--text-muted);
          gap: 12px;
          padding: 20px;
          text-align: center;
        }

        .weather-waiting .spinner {
          width: 24px;
          height: 24px;
          border: 2px solid var(--border-color);
          border-top-color: var(--accent-primary);
          border-radius: 50%;
          animation: spin 1s linear infinite;
        }

        .waiting-hint {
          font-size: 12px;
          color: var(--text-muted);
        }

        .waiting-received {
          font-size: 11px;
          color: var(--success);
          margin-top: 4px;
        }

        @keyframes spin {
          to { transform: rotate(360deg); }
        }

        .weather-content {
          padding: 16px;
          display: flex;
          flex-direction: column;
          gap: 20px;
        }

        .machine-weather-section {
          display: flex;
          flex-direction: column;
          gap: 20px;
        }

        .machine-weather-header {
          padding: 8px 12px;
          background: var(--bg-tertiary);
          border-radius: 6px;
          border: 1px solid var(--border-color);
          margin-bottom: 8px;
        }

        .machine-name {
          font-size: 13px;
          font-weight: 600;
          color: var(--accent-primary);
          font-family: 'JetBrains Mono', 'Fira Code', monospace;
          text-transform: uppercase;
          letter-spacing: 0.5px;
        }

        .section-title {
          font-size: 12px;
          font-weight: 600;
          color: var(--text-muted);
          text-transform: uppercase;
          letter-spacing: 0.5px;
          margin-bottom: 8px;
        }

        /* Current Weather Styles */
        .weather-current {
          display: flex;
          flex-direction: column;
          gap: 16px;
        }

        .weather-main {
          text-align: center;
        }

        .weather-temp {
          display: flex;
          align-items: center;
          justify-content: center;
          gap: 12px;
          margin-bottom: 4px;
        }

        .weather-emoji {
          font-size: 48px;
        }

        .temp-value {
          font-size: 36px;
          font-weight: 600;
          color: var(--text-primary);
        }

        .weather-desc {
          font-size: 16px;
          color: var(--text-secondary);
        }

        .weather-location {
          font-size: 12px;
          color: var(--text-muted);
          margin-top: 4px;
        }

        .weather-coords {
          font-size: 11px;
          color: var(--text-muted);
          font-family: 'JetBrains Mono', 'Fira Code', monospace;
          margin-top: 2px;
        }

        .weather-details {
          display: grid;
          grid-template-columns: repeat(2, 1fr);
          gap: 8px;
        }

        .detail-row {
          display: flex;
          justify-content: space-between;
          padding: 8px 12px;
          background: var(--bg-tertiary);
          border-radius: 6px;
        }

        .detail-label {
          font-size: 11px;
          color: var(--text-muted);
          text-transform: uppercase;
          letter-spacing: 0.5px;
        }

        .detail-value {
          font-size: 12px;
          font-weight: 600;
          color: var(--text-primary);
        }

        /* Hourly Forecast Styles */
        .weather-hourly {
          border-top: 1px solid var(--border-color);
          padding-top: 16px;
        }

        .forecast-scroll {
          display: flex;
          gap: 8px;
          overflow-x: auto;
          padding-bottom: 8px;
          -webkit-overflow-scrolling: touch;
        }

        .hour-item {
          display: flex;
          flex-direction: column;
          align-items: center;
          gap: 4px;
          padding: 10px 8px;
          background: var(--bg-tertiary);
          border-radius: 8px;
          min-width: 56px;
          flex-shrink: 0;
        }

        .hour-time {
          font-size: 10px;
          color: var(--text-muted);
        }

        .hour-emoji {
          font-size: 20px;
        }

        .hour-temp {
          font-size: 13px;
          font-weight: 600;
          color: var(--text-primary);
        }

        .hour-rain {
          font-size: 9px;
          color: var(--accent-secondary);
        }

        /* Daily Forecast Styles */
        .weather-daily {
          border-top: 1px solid var(--border-color);
          padding-top: 16px;
          display: flex;
          flex-direction: column;
          gap: 6px;
        }

        .day-item {
          display: flex;
          align-items: center;
          padding: 10px 12px;
          background: var(--bg-tertiary);
          border-radius: 8px;
          gap: 12px;
        }

        .day-date {
          font-size: 12px;
          color: var(--text-secondary);
          min-width: 70px;
          flex-shrink: 0;
        }

        .day-weather {
          display: flex;
          align-items: center;
          gap: 8px;
          flex: 1;
          min-width: 0;
        }

        .day-emoji {
          font-size: 18px;
          flex-shrink: 0;
        }

        .day-desc {
          font-size: 11px;
          color: var(--text-muted);
          white-space: nowrap;
          overflow: hidden;
          text-overflow: ellipsis;
        }

        .day-temps {
          display: flex;
          gap: 6px;
          align-items: center;
          flex-shrink: 0;
        }

        .temp-max {
          font-size: 13px;
          font-weight: 600;
          color: var(--text-primary);
        }

        .temp-min {
          font-size: 12px;
          color: var(--text-muted);
        }

        .day-rain {
          font-size: 10px;
          color: var(--accent-secondary);
          flex-shrink: 0;
        }

        .panel-footer {
          padding: 8px 12px;
          background: var(--bg-tertiary);
          border-top: 1px solid var(--border-color);
        }

        .footer-info {
          font-size: 11px;
          color: var(--text-muted);
          font-family: 'JetBrains Mono', monospace;
        }

        /* Edit Footer Styles */
        .panel-edit-footer {
          padding: 16px;
          background: var(--bg-tertiary);
          border-top: 1px solid var(--border-color);
          display: flex;
          flex-direction: column;
          gap: 16px;
        }

        .edit-field {
          display: flex;
          flex-direction: column;
          gap: 6px;
        }

        .edit-field label {
          font-size: 11px;
          text-transform: uppercase;
          letter-spacing: 0.5px;
          color: var(--text-muted);
        }

        .edit-input {
          padding: 12px;
          background: var(--bg-primary);
          border: 1px solid var(--border-color);
          border-radius: 8px;
          color: var(--text-primary);
          font-size: 14px;
          width: 100%;
          box-sizing: border-box;
        }

        .edit-input:focus {
          border-color: var(--accent-primary);
          outline: none;
        }

        .edit-section {
          display: flex;
          flex-direction: column;
          gap: 10px;
        }

        .section-label {
          font-size: 11px;
          text-transform: uppercase;
          letter-spacing: 0.5px;
          color: var(--text-muted);
        }

        .location-display {
          display: flex;
          flex-direction: column;
          gap: 6px;
          padding: 12px;
          background: var(--bg-primary);
          border-radius: 8px;
          border: 1px solid var(--border-color);
        }

        .coord-row {
          display: flex;
          justify-content: space-between;
          align-items: center;
        }

        .coord-label {
          font-size: 12px;
          color: var(--text-muted);
        }

        .coord-value {
          font-size: 13px;
          font-weight: 500;
          color: var(--text-primary);
          font-family: 'JetBrains Mono', 'Fira Code', monospace;
        }

        .no-data {
          font-size: 12px;
          color: var(--text-muted);
          font-style: italic;
          text-align: center;
          padding: 8px;
        }

        .config-hint {
          display: flex;
          gap: 10px;
          padding: 12px;
          background: rgba(99, 102, 241, 0.1);
          border: 1px solid rgba(99, 102, 241, 0.3);
          border-radius: 8px;
          color: var(--text-secondary);
          font-size: 12px;
          line-height: 1.5;
        }

        .config-hint svg {
          flex-shrink: 0;
          color: var(--accent-primary);
          margin-top: 2px;
        }

        .config-hint code {
          font-family: 'JetBrains Mono', 'Fira Code', monospace;
          font-size: 11px;
          background: rgba(0, 0, 0, 0.2);
          padding: 2px 6px;
          border-radius: 4px;
          color: var(--accent-secondary);
        }

        .location-edit-form {
          display: flex;
          flex-direction: column;
          gap: 10px;
          padding: 12px;
          background: var(--bg-primary);
          border-radius: 8px;
          border: 1px solid var(--border-color);
        }

        .coord-input-row {
          display: flex;
          align-items: center;
          gap: 12px;
        }

        .coord-input-row label {
          font-size: 12px;
          color: var(--text-muted);
          min-width: 80px;
        }

        .coord-input {
          flex: 1;
          padding: 8px 12px;
          background: var(--bg-tertiary);
          border: 1px solid var(--border-color);
          border-radius: 6px;
          color: var(--text-primary);
          font-size: 13px;
          font-family: 'JetBrains Mono', 'Fira Code', monospace;
        }

        .coord-input:focus {
          border-color: var(--accent-primary);
          outline: none;
        }

        .coord-input::placeholder {
          color: var(--text-muted);
          opacity: 0.6;
        }

        .btn-update-location {
          margin-top: 8px;
          padding: 10px 16px;
          background: var(--accent-secondary);
          border: none;
          border-radius: 6px;
          color: var(--bg-primary);
          font-size: 13px;
          font-weight: 500;
          cursor: pointer;
          transition: all 0.15s;
        }

        .btn-update-location:hover:not(:disabled) {
          opacity: 0.9;
        }

        .btn-update-location:disabled {
          opacity: 0.5;
          cursor: not-allowed;
        }

        .location-status {
          font-size: 12px;
          padding: 8px;
          border-radius: 6px;
          text-align: center;
        }

        .location-status.success {
          background: rgba(52, 211, 153, 0.1);
          color: var(--success);
          border: 1px solid rgba(52, 211, 153, 0.3);
        }

        .location-status.error {
          background: rgba(255, 23, 68, 0.1);
          color: var(--error);
          border: 1px solid rgba(255, 23, 68, 0.3);
        }

        .edit-info {
          display: flex;
          flex-direction: column;
          gap: 6px;
        }

        .info-label {
          font-size: 11px;
          text-transform: uppercase;
          letter-spacing: 0.5px;
          color: var(--text-muted);
        }

        .topic-list {
          display: flex;
          flex-direction: column;
          gap: 4px;
        }

        .topic-item {
          font-size: 11px;
          font-family: 'JetBrains Mono', 'Fira Code', monospace;
          color: var(--text-secondary);
          padding: 6px 10px;
          background: var(--bg-primary);
          border-radius: 4px;
          word-break: break-all;
        }

        .edit-actions {
          display: flex;
          gap: 12px;
          justify-content: flex-end;
        }

        .btn-primary,
        .btn-secondary {
          padding: 12px 24px;
          border-radius: 8px;
          font-size: 14px;
          font-weight: 500;
          cursor: pointer;
          transition: all 0.15s;
          min-width: 80px;
        }

        .btn-primary {
          background: var(--accent-primary);
          border: none;
          color: white;
        }

        .btn-primary:hover {
          background: #5c7cfa;
        }

        .btn-secondary {
          background: transparent;
          border: 1px solid var(--border-color);
          color: var(--text-secondary);
        }

        .btn-secondary:hover {
          background: var(--bg-primary);
          border-color: var(--text-muted);
        }

        .mono {
          font-family: 'JetBrains Mono', 'Fira Code', monospace;
        }

        /* Mobile responsive styles */
        @media (max-width: 768px) {
          .panel-header {
            padding: 10px 12px;
            flex-wrap: wrap;
            gap: 8px;
          }

          .panel-header-left {
            flex: 1 1 auto;
            min-width: 0;
          }

          .panel-type-badge {
            padding: 3px 8px;
            font-size: 9px;
          }

          .panel-stats {
            gap: 6px;
          }

          /* Hide maximize button on mobile */
          .maximize-btn {
            display: none;
          }

          .stat-value {
            font-size: 12px;
          }

          .stat-label {
            font-size: 9px;
          }

          .icon-btn {
            width: 36px;
            height: 36px;
          }

          .weather-content-container {
            min-height: 180px;
          }

          .weather-content {
            padding: 12px;
            gap: 16px;
          }

          .weather-emoji {
            font-size: 36px;
          }

          .temp-value {
            font-size: 28px;
          }

          .weather-desc {
            font-size: 14px;
          }

          .weather-details {
            grid-template-columns: 1fr;
            gap: 6px;
          }

          .detail-row {
            padding: 10px 12px;
          }

          .detail-label {
            font-size: 11px;
          }

          .detail-value {
            font-size: 12px;
          }

          .hour-item {
            padding: 8px 6px;
            min-width: 52px;
          }

          .hour-emoji {
            font-size: 18px;
          }

          .hour-temp {
            font-size: 12px;
          }

          .day-item {
            padding: 10px;
            flex-wrap: wrap;
            gap: 8px;
          }

          .day-date {
            font-size: 11px;
            min-width: 60px;
          }

          .day-emoji {
            font-size: 16px;
          }

          .day-desc {
            display: none;
          }

          .panel-edit-footer {
            padding: 16px;
            gap: 16px;
          }

          .edit-input {
            padding: 14px;
            font-size: 16px;
          }

          .topic-item {
            font-size: 10px;
            padding: 8px 10px;
          }

          .edit-actions {
            flex-direction: column;
            gap: 10px;
          }

          .btn-primary,
          .btn-secondary {
            width: 100%;
            padding: 14px 24px;
            font-size: 15px;
          }
        }

        @media (max-width: 480px) {
          .panel-header {
            padding: 8px 10px;
          }

          .panel-stats .stat {
            display: none;
          }

          .weather-content {
            padding: 10px;
          }

          .weather-emoji {
            font-size: 32px;
          }

          .temp-value {
            font-size: 24px;
          }

          .section-title {
            font-size: 11px;
          }
        }
      `}</style>
    </div>
  );
}
