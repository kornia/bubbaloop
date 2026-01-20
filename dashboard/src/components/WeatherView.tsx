import { useCallback, useState, useRef, useEffect } from 'react';
import { Session, Sample } from '@eclipse-zenoh/zenoh-ts';
import { useZenohSubscriber, getSamplePayload } from '../lib/zenoh';
import {
  decodeCurrentWeather,
  decodeHourlyForecast,
  decodeDailyForecast,
  getWeatherDescription,
  getWindDirection,
  CurrentWeather,
  HourlyForecast,
  DailyForecast,
} from '../proto/weather';

interface DragHandleProps {
  [key: string]: unknown;
}

interface WeatherViewPanelProps {
  session: Session;
  panelName: string;
  topic: string; // Base topic pattern, we'll derive all 3 from it
  isMaximized?: boolean;
  onMaximize?: () => void;
  onTopicChange?: (topic: string) => void;
  onNameChange?: (name: string) => void;
  onRemove?: () => void;
  dragHandleProps?: DragHandleProps;
}

function formatDate(timestamp: bigint): string {
  const date = new Date(Number(timestamp / 1000000n));
  return date.toLocaleDateString('en-US', { weekday: 'short', month: 'short', day: 'numeric' });
}

function formatHour(timestamp: bigint): string {
  const date = new Date(Number(timestamp / 1000000n));
  return date.toLocaleTimeString('en-US', { hour: 'numeric' });
}

export function WeatherViewPanel({
  session,
  panelName,
  isMaximized = false,
  onMaximize,
  onNameChange,
  onRemove,
  dragHandleProps,
}: WeatherViewPanelProps) {
  // Store all three types of weather data
  const [currentWeather, setCurrentWeather] = useState<CurrentWeather | null>(null);
  const [hourlyForecast, setHourlyForecast] = useState<HourlyForecast | null>(null);
  const [dailyForecast, setDailyForecast] = useState<DailyForecast | null>(null);
  const [isEditing, setIsEditing] = useState(false);
  const [editName, setEditName] = useState(panelName);
  const lastUpdateRef = useRef<number>(0);

  // Fixed weather topic patterns
  const currentTopic = '0/weather%current/**';
  const hourlyTopic = '0/weather%hourly/**';
  const dailyTopic = '0/weather%daily/**';

  // Handle current weather samples
  const handleCurrentSample = useCallback((sample: Sample) => {
    try {
      const payload = getSamplePayload(sample);
      const data = decodeCurrentWeather(payload);
      if (data) {
        setCurrentWeather(data);
        lastUpdateRef.current = Date.now();
      }
    } catch (e) {
      console.error('[WeatherView] Failed to decode current weather:', e);
    }
  }, []);

  // Handle hourly forecast samples
  const handleHourlySample = useCallback((sample: Sample) => {
    try {
      const payload = getSamplePayload(sample);
      const data = decodeHourlyForecast(payload);
      if (data) {
        setHourlyForecast(data);
      }
    } catch (e) {
      console.error('[WeatherView] Failed to decode hourly forecast:', e);
    }
  }, []);

  // Handle daily forecast samples
  const handleDailySample = useCallback((sample: Sample) => {
    try {
      const payload = getSamplePayload(sample);
      const data = decodeDailyForecast(payload);
      if (data) {
        setDailyForecast(data);
      }
    } catch (e) {
      console.error('[WeatherView] Failed to decode daily forecast:', e);
    }
  }, []);

  // Subscribe to all three topics
  const { messageCount: currentCount } = useZenohSubscriber(session, currentTopic, handleCurrentSample);
  const { messageCount: hourlyCount } = useZenohSubscriber(session, hourlyTopic, handleHourlySample);
  const { messageCount: dailyCount } = useZenohSubscriber(session, dailyTopic, handleDailySample);

  const totalMessages = currentCount + hourlyCount + dailyCount;

  const handleSaveEdit = () => {
    if (editName !== panelName && onNameChange) {
      onNameChange(editName);
    }
    setIsEditing(false);
  };

  const handleCancelEdit = () => {
    setEditName(panelName);
    setIsEditing(false);
  };

  // Update edit state when props change
  useEffect(() => {
    setEditName(panelName);
  }, [panelName]);

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

  const hasAnyData = currentWeather || hourlyForecast || dailyForecast;

  return (
    <div className={`weather-view-panel ${isMaximized ? 'maximized' : ''}`}>
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
          {isEditing ? (
            <input
              type="text"
              value={editName}
              onChange={(e) => setEditName(e.target.value)}
              className="panel-name-input"
              autoFocus
            />
          ) : (
            <span className="panel-name">{panelName}</span>
          )}
          <span className="panel-type-badge">WEATHER</span>
        </div>
        <div className="panel-stats">
          <span className="stat">
            <span className="stat-value mono">{totalMessages.toLocaleString()}</span>
            <span className="stat-label">msgs</span>
          </span>
          {onMaximize && (
            <button className="icon-btn" onClick={onMaximize} title={isMaximized ? 'Restore' : 'Maximize'}>
              {isMaximized ? (
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M8 3v3a2 2 0 01-2 2H3m18 0h-3a2 2 0 01-2-2V3m0 18v-3a2 2 0 012-2h3M3 16h3a2 2 0 012 2v3" />
                </svg>
              ) : (
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M8 3H5a2 2 0 00-2 2v3m18 0V5a2 2 0 00-2-2h-3m0 18h3a2 2 0 002-2v-3M3 16v3a2 2 0 002 2h3" />
                </svg>
              )}
            </button>
          )}
          <button className="icon-btn" onClick={() => setIsEditing(!isEditing)} title="Edit panel">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M11 4H4a2 2 0 00-2 2v14a2 2 0 002 2h14a2 2 0 002-2v-7" />
              <path d="M18.5 2.5a2.121 2.121 0 013 3L12 15l-4 1 1-4 9.5-9.5z" />
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
            {currentWeather && renderCurrentWeather(currentWeather)}
            {hourlyForecast && renderHourlyForecast(hourlyForecast)}
            {dailyForecast && renderDailyForecast(dailyForecast)}
          </div>
        )}
      </div>

      {isEditing && (
        <div className="panel-edit-footer">
          <div className="edit-field">
            <label>Panel Name:</label>
            <input
              type="text"
              value={editName}
              onChange={(e) => setEditName(e.target.value)}
              className="edit-input"
              placeholder="Enter panel name..."
            />
          </div>
          <div className="edit-info">
            <span className="info-label">Subscribed Topics:</span>
            <div className="topic-list">
              <span className="topic-item">0/weather%current/**</span>
              <span className="topic-item">0/weather%hourly/**</span>
              <span className="topic-item">0/weather%daily/**</span>
            </div>
          </div>
          <div className="edit-actions">
            <button className="btn-secondary" onClick={handleCancelEdit}>Cancel</button>
            <button className="btn-primary" onClick={handleSaveEdit}>Save</button>
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
          flex: 1;
          min-height: 200px;
          max-height: 600px;
          overflow: auto;
          background: var(--bg-primary);
        }

        .weather-waiting {
          display: flex;
          flex-direction: column;
          align-items: center;
          justify-content: center;
          height: 100%;
          min-height: 200px;
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

          .panel-name {
            font-size: 13px;
          }

          .panel-name-input {
            font-size: 14px;
            padding: 8px 12px;
          }

          .panel-type-badge {
            padding: 3px 8px;
            font-size: 9px;
          }

          .panel-stats {
            gap: 6px;
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
            min-height: 150px;
            max-height: none;
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
