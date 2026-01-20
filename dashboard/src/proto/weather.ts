// Re-export compiled protobuf types and provide helper functions
import { bubbaloop } from './messages.pb.js';
import Long from 'long';

// Re-export the proto types
export const CurrentWeatherProto = bubbaloop.weather.v1.CurrentWeather;
export const HourlyForecastProto = bubbaloop.weather.v1.HourlyForecast;
export const DailyForecastProto = bubbaloop.weather.v1.DailyForecast;
export const HourlyForecastEntryProto = bubbaloop.weather.v1.HourlyForecastEntry;
export const DailyForecastEntryProto = bubbaloop.weather.v1.DailyForecastEntry;
export const LocationConfigProto = bubbaloop.weather.v1.LocationConfig;

// TypeScript interfaces for convenience
export interface Header {
  acqTime: bigint;
  pubTime: bigint;
  sequence: number;
  frameId: string;
}

export interface CurrentWeather {
  header?: Header;
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

export interface HourlyForecastEntry {
  time: bigint;
  temperature_2m: number;
  relativeHumidity_2m: number;
  precipitationProbability: number;
  precipitation: number;
  weatherCode: number;
  windSpeed_10m: number;
  windDirection_10m: number;
  cloudCover: number;
}

export interface HourlyForecast {
  header?: Header;
  latitude: number;
  longitude: number;
  timezone: string;
  entries: HourlyForecastEntry[];
}

export interface DailyForecastEntry {
  time: bigint;
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

export interface DailyForecast {
  header?: Header;
  latitude: number;
  longitude: number;
  timezone: string;
  entries: DailyForecastEntry[];
}

export interface LocationConfig {
  latitude: number;
  longitude: number;
  timezone: string;
}

// Convert protobufjs Long to BigInt
function toLongBigInt(value: Long | number | undefined | null): bigint {
  if (value === undefined || value === null) {
    return 0n;
  }
  if (typeof value === 'number') {
    return BigInt(value);
  }
  if (Long.isLong(value)) {
    return BigInt(value.toString());
  }
  return 0n;
}

// Decode Header
function decodeHeader(header: unknown): Header | undefined {
  if (!header || typeof header !== 'object') return undefined;
  const h = header as Record<string, unknown>;
  return {
    acqTime: toLongBigInt(h.acqTime as Long | number),
    pubTime: toLongBigInt(h.pubTime as Long | number),
    sequence: (h.sequence as number) ?? 0,
    frameId: (h.frameId as string) ?? '',
  };
}

// Decode CurrentWeather from Uint8Array
export function decodeCurrentWeather(data: Uint8Array): CurrentWeather | null {
  try {
    const message = CurrentWeatherProto.decode(data);
    return {
      header: decodeHeader(message.header),
      latitude: message.latitude ?? 0,
      longitude: message.longitude ?? 0,
      timezone: message.timezone ?? '',
      temperature_2m: message.temperature_2m ?? 0,
      relativeHumidity_2m: message.relativeHumidity_2m ?? 0,
      apparentTemperature: message.apparentTemperature ?? 0,
      precipitation: message.precipitation ?? 0,
      rain: message.rain ?? 0,
      windSpeed_10m: message.windSpeed_10m ?? 0,
      windDirection_10m: message.windDirection_10m ?? 0,
      windGusts_10m: message.windGusts_10m ?? 0,
      weatherCode: message.weatherCode ?? 0,
      cloudCover: message.cloudCover ?? 0,
      pressureMsl: message.pressureMsl ?? 0,
      surfacePressure: message.surfacePressure ?? 0,
      isDay: message.isDay ?? 0,
    };
  } catch (error) {
    console.error('[Proto] Failed to decode CurrentWeather:', error);
    return null;
  }
}

// Decode HourlyForecast from Uint8Array
export function decodeHourlyForecast(data: Uint8Array): HourlyForecast | null {
  try {
    const message = HourlyForecastProto.decode(data);
    const entries: HourlyForecastEntry[] = (message.entries ?? []).map((e) => ({
      time: toLongBigInt(e.time as Long | number),
      temperature_2m: e.temperature_2m ?? 0,
      relativeHumidity_2m: e.relativeHumidity_2m ?? 0,
      precipitationProbability: e.precipitationProbability ?? 0,
      precipitation: e.precipitation ?? 0,
      weatherCode: e.weatherCode ?? 0,
      windSpeed_10m: e.windSpeed_10m ?? 0,
      windDirection_10m: e.windDirection_10m ?? 0,
      cloudCover: e.cloudCover ?? 0,
    }));
    return {
      header: decodeHeader(message.header),
      latitude: message.latitude ?? 0,
      longitude: message.longitude ?? 0,
      timezone: message.timezone ?? '',
      entries,
    };
  } catch (error) {
    console.error('[Proto] Failed to decode HourlyForecast:', error);
    return null;
  }
}

// Decode DailyForecast from Uint8Array
export function decodeDailyForecast(data: Uint8Array): DailyForecast | null {
  try {
    const message = DailyForecastProto.decode(data);
    const entries: DailyForecastEntry[] = (message.entries ?? []).map((e) => ({
      time: toLongBigInt(e.time as Long | number),
      temperature_2mMax: e.temperature_2mMax ?? 0,
      temperature_2mMin: e.temperature_2mMin ?? 0,
      precipitationSum: e.precipitationSum ?? 0,
      precipitationProbabilityMax: e.precipitationProbabilityMax ?? 0,
      weatherCode: e.weatherCode ?? 0,
      windSpeed_10mMax: e.windSpeed_10mMax ?? 0,
      windGusts_10mMax: e.windGusts_10mMax ?? 0,
      sunrise: e.sunrise ?? '',
      sunset: e.sunset ?? '',
    }));
    return {
      header: decodeHeader(message.header),
      latitude: message.latitude ?? 0,
      longitude: message.longitude ?? 0,
      timezone: message.timezone ?? '',
      entries,
    };
  } catch (error) {
    console.error('[Proto] Failed to decode DailyForecast:', error);
    return null;
  }
}

// Weather code to description and emoji
export function getWeatherDescription(code: number): { text: string; emoji: string } {
  if (code === 0) return { text: 'Clear sky', emoji: '☀️' };
  if (code >= 1 && code <= 3) return { text: 'Partly cloudy', emoji: '⛅' };
  if (code >= 45 && code <= 48) return { text: 'Fog', emoji: '🌫️' };
  if (code >= 51 && code <= 57) return { text: 'Drizzle', emoji: '🌧️' };
  if (code >= 61 && code <= 67) return { text: 'Rain', emoji: '🌧️' };
  if (code >= 71 && code <= 77) return { text: 'Snow', emoji: '❄️' };
  if (code >= 80 && code <= 82) return { text: 'Rain showers', emoji: '🌦️' };
  if (code >= 85 && code <= 86) return { text: 'Snow showers', emoji: '🌨️' };
  if (code >= 95 && code <= 99) return { text: 'Thunderstorm', emoji: '⛈️' };
  return { text: 'Unknown', emoji: '❓' };
}

// Wind direction to compass
export function getWindDirection(degrees: number): string {
  const directions = ['N', 'NE', 'E', 'SE', 'S', 'SW', 'W', 'NW'];
  const index = Math.round(degrees / 45) % 8;
  return directions[index];
}

// Encode LocationConfig to Uint8Array for publishing
export function encodeLocationConfig(config: LocationConfig): Uint8Array {
  const message = LocationConfigProto.create({
    latitude: config.latitude,
    longitude: config.longitude,
    timezone: config.timezone || '',
  });
  return LocationConfigProto.encode(message).finish();
}
