import * as $protobuf from "protobufjs";
import Long = require("long");
/** Namespace bubbaloop. */
export namespace bubbaloop {

    /** Namespace header. */
    namespace header {

        /** Namespace v1. */
        namespace v1 {

            /** Properties of a Header. */
            interface IHeader {

                /** Header acqTime */
                acqTime?: (number|Long|null);

                /** Header pubTime */
                pubTime?: (number|Long|null);

                /** Header sequence */
                sequence?: (number|null);

                /** Header frameId */
                frameId?: (string|null);
            }

            /** Represents a Header. */
            class Header implements IHeader {

                /**
                 * Constructs a new Header.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: bubbaloop.header.v1.IHeader);

                /** Header acqTime. */
                public acqTime: (number|Long);

                /** Header pubTime. */
                public pubTime: (number|Long);

                /** Header sequence. */
                public sequence: number;

                /** Header frameId. */
                public frameId: string;

                /**
                 * Creates a new Header instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns Header instance
                 */
                public static create(properties?: bubbaloop.header.v1.IHeader): bubbaloop.header.v1.Header;

                /**
                 * Encodes the specified Header message. Does not implicitly {@link bubbaloop.header.v1.Header.verify|verify} messages.
                 * @param message Header message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: bubbaloop.header.v1.IHeader, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified Header message, length delimited. Does not implicitly {@link bubbaloop.header.v1.Header.verify|verify} messages.
                 * @param message Header message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: bubbaloop.header.v1.IHeader, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a Header message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns Header
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): bubbaloop.header.v1.Header;

                /**
                 * Decodes a Header message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns Header
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): bubbaloop.header.v1.Header;

                /**
                 * Verifies a Header message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a Header message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns Header
                 */
                public static fromObject(object: { [k: string]: any }): bubbaloop.header.v1.Header;

                /**
                 * Creates a plain object from a Header message. Also converts values to other types if specified.
                 * @param message Header
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: bubbaloop.header.v1.Header, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this Header to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for Header
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }
        }
    }

    /** Namespace camera. */
    namespace camera {

        /** Namespace v1. */
        namespace v1 {

            /** Properties of a CompressedImage. */
            interface ICompressedImage {

                /** CompressedImage header */
                header?: (bubbaloop.header.v1.IHeader|null);

                /** CompressedImage format */
                format?: (string|null);

                /** CompressedImage data */
                data?: (Uint8Array|null);
            }

            /** Represents a CompressedImage. */
            class CompressedImage implements ICompressedImage {

                /**
                 * Constructs a new CompressedImage.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: bubbaloop.camera.v1.ICompressedImage);

                /** CompressedImage header. */
                public header?: (bubbaloop.header.v1.IHeader|null);

                /** CompressedImage format. */
                public format: string;

                /** CompressedImage data. */
                public data: Uint8Array;

                /**
                 * Creates a new CompressedImage instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns CompressedImage instance
                 */
                public static create(properties?: bubbaloop.camera.v1.ICompressedImage): bubbaloop.camera.v1.CompressedImage;

                /**
                 * Encodes the specified CompressedImage message. Does not implicitly {@link bubbaloop.camera.v1.CompressedImage.verify|verify} messages.
                 * @param message CompressedImage message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: bubbaloop.camera.v1.ICompressedImage, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified CompressedImage message, length delimited. Does not implicitly {@link bubbaloop.camera.v1.CompressedImage.verify|verify} messages.
                 * @param message CompressedImage message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: bubbaloop.camera.v1.ICompressedImage, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a CompressedImage message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns CompressedImage
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): bubbaloop.camera.v1.CompressedImage;

                /**
                 * Decodes a CompressedImage message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns CompressedImage
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): bubbaloop.camera.v1.CompressedImage;

                /**
                 * Verifies a CompressedImage message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a CompressedImage message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns CompressedImage
                 */
                public static fromObject(object: { [k: string]: any }): bubbaloop.camera.v1.CompressedImage;

                /**
                 * Creates a plain object from a CompressedImage message. Also converts values to other types if specified.
                 * @param message CompressedImage
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: bubbaloop.camera.v1.CompressedImage, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this CompressedImage to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for CompressedImage
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a RawImage. */
            interface IRawImage {

                /** RawImage header */
                header?: (bubbaloop.header.v1.IHeader|null);

                /** RawImage width */
                width?: (number|null);

                /** RawImage height */
                height?: (number|null);

                /** RawImage encoding */
                encoding?: (string|null);

                /** RawImage step */
                step?: (number|null);

                /** RawImage data */
                data?: (Uint8Array|null);
            }

            /** Represents a RawImage. */
            class RawImage implements IRawImage {

                /**
                 * Constructs a new RawImage.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: bubbaloop.camera.v1.IRawImage);

                /** RawImage header. */
                public header?: (bubbaloop.header.v1.IHeader|null);

                /** RawImage width. */
                public width: number;

                /** RawImage height. */
                public height: number;

                /** RawImage encoding. */
                public encoding: string;

                /** RawImage step. */
                public step: number;

                /** RawImage data. */
                public data: Uint8Array;

                /**
                 * Creates a new RawImage instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns RawImage instance
                 */
                public static create(properties?: bubbaloop.camera.v1.IRawImage): bubbaloop.camera.v1.RawImage;

                /**
                 * Encodes the specified RawImage message. Does not implicitly {@link bubbaloop.camera.v1.RawImage.verify|verify} messages.
                 * @param message RawImage message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: bubbaloop.camera.v1.IRawImage, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified RawImage message, length delimited. Does not implicitly {@link bubbaloop.camera.v1.RawImage.verify|verify} messages.
                 * @param message RawImage message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: bubbaloop.camera.v1.IRawImage, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a RawImage message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns RawImage
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): bubbaloop.camera.v1.RawImage;

                /**
                 * Decodes a RawImage message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns RawImage
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): bubbaloop.camera.v1.RawImage;

                /**
                 * Verifies a RawImage message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a RawImage message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns RawImage
                 */
                public static fromObject(object: { [k: string]: any }): bubbaloop.camera.v1.RawImage;

                /**
                 * Creates a plain object from a RawImage message. Also converts values to other types if specified.
                 * @param message RawImage
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: bubbaloop.camera.v1.RawImage, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this RawImage to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for RawImage
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }
        }
    }

    /** Namespace weather. */
    namespace weather {

        /** Namespace v1. */
        namespace v1 {

            /** Properties of a CurrentWeather. */
            interface ICurrentWeather {

                /** CurrentWeather header */
                header?: (bubbaloop.header.v1.IHeader|null);

                /** CurrentWeather latitude */
                latitude?: (number|null);

                /** CurrentWeather longitude */
                longitude?: (number|null);

                /** CurrentWeather timezone */
                timezone?: (string|null);

                /** CurrentWeather temperature_2m */
                temperature_2m?: (number|null);

                /** CurrentWeather relativeHumidity_2m */
                relativeHumidity_2m?: (number|null);

                /** CurrentWeather apparentTemperature */
                apparentTemperature?: (number|null);

                /** CurrentWeather precipitation */
                precipitation?: (number|null);

                /** CurrentWeather rain */
                rain?: (number|null);

                /** CurrentWeather windSpeed_10m */
                windSpeed_10m?: (number|null);

                /** CurrentWeather windDirection_10m */
                windDirection_10m?: (number|null);

                /** CurrentWeather windGusts_10m */
                windGusts_10m?: (number|null);

                /** CurrentWeather weatherCode */
                weatherCode?: (number|null);

                /** CurrentWeather cloudCover */
                cloudCover?: (number|null);

                /** CurrentWeather pressureMsl */
                pressureMsl?: (number|null);

                /** CurrentWeather surfacePressure */
                surfacePressure?: (number|null);

                /** CurrentWeather isDay */
                isDay?: (number|null);
            }

            /** Represents a CurrentWeather. */
            class CurrentWeather implements ICurrentWeather {

                /**
                 * Constructs a new CurrentWeather.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: bubbaloop.weather.v1.ICurrentWeather);

                /** CurrentWeather header. */
                public header?: (bubbaloop.header.v1.IHeader|null);

                /** CurrentWeather latitude. */
                public latitude: number;

                /** CurrentWeather longitude. */
                public longitude: number;

                /** CurrentWeather timezone. */
                public timezone: string;

                /** CurrentWeather temperature_2m. */
                public temperature_2m: number;

                /** CurrentWeather relativeHumidity_2m. */
                public relativeHumidity_2m: number;

                /** CurrentWeather apparentTemperature. */
                public apparentTemperature: number;

                /** CurrentWeather precipitation. */
                public precipitation: number;

                /** CurrentWeather rain. */
                public rain: number;

                /** CurrentWeather windSpeed_10m. */
                public windSpeed_10m: number;

                /** CurrentWeather windDirection_10m. */
                public windDirection_10m: number;

                /** CurrentWeather windGusts_10m. */
                public windGusts_10m: number;

                /** CurrentWeather weatherCode. */
                public weatherCode: number;

                /** CurrentWeather cloudCover. */
                public cloudCover: number;

                /** CurrentWeather pressureMsl. */
                public pressureMsl: number;

                /** CurrentWeather surfacePressure. */
                public surfacePressure: number;

                /** CurrentWeather isDay. */
                public isDay: number;

                /**
                 * Creates a new CurrentWeather instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns CurrentWeather instance
                 */
                public static create(properties?: bubbaloop.weather.v1.ICurrentWeather): bubbaloop.weather.v1.CurrentWeather;

                /**
                 * Encodes the specified CurrentWeather message. Does not implicitly {@link bubbaloop.weather.v1.CurrentWeather.verify|verify} messages.
                 * @param message CurrentWeather message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: bubbaloop.weather.v1.ICurrentWeather, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified CurrentWeather message, length delimited. Does not implicitly {@link bubbaloop.weather.v1.CurrentWeather.verify|verify} messages.
                 * @param message CurrentWeather message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: bubbaloop.weather.v1.ICurrentWeather, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a CurrentWeather message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns CurrentWeather
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): bubbaloop.weather.v1.CurrentWeather;

                /**
                 * Decodes a CurrentWeather message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns CurrentWeather
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): bubbaloop.weather.v1.CurrentWeather;

                /**
                 * Verifies a CurrentWeather message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a CurrentWeather message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns CurrentWeather
                 */
                public static fromObject(object: { [k: string]: any }): bubbaloop.weather.v1.CurrentWeather;

                /**
                 * Creates a plain object from a CurrentWeather message. Also converts values to other types if specified.
                 * @param message CurrentWeather
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: bubbaloop.weather.v1.CurrentWeather, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this CurrentWeather to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for CurrentWeather
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of an HourlyForecastEntry. */
            interface IHourlyForecastEntry {

                /** HourlyForecastEntry time */
                time?: (number|Long|null);

                /** HourlyForecastEntry temperature_2m */
                temperature_2m?: (number|null);

                /** HourlyForecastEntry relativeHumidity_2m */
                relativeHumidity_2m?: (number|null);

                /** HourlyForecastEntry precipitationProbability */
                precipitationProbability?: (number|null);

                /** HourlyForecastEntry precipitation */
                precipitation?: (number|null);

                /** HourlyForecastEntry weatherCode */
                weatherCode?: (number|null);

                /** HourlyForecastEntry windSpeed_10m */
                windSpeed_10m?: (number|null);

                /** HourlyForecastEntry windDirection_10m */
                windDirection_10m?: (number|null);

                /** HourlyForecastEntry cloudCover */
                cloudCover?: (number|null);
            }

            /** Represents an HourlyForecastEntry. */
            class HourlyForecastEntry implements IHourlyForecastEntry {

                /**
                 * Constructs a new HourlyForecastEntry.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: bubbaloop.weather.v1.IHourlyForecastEntry);

                /** HourlyForecastEntry time. */
                public time: (number|Long);

                /** HourlyForecastEntry temperature_2m. */
                public temperature_2m: number;

                /** HourlyForecastEntry relativeHumidity_2m. */
                public relativeHumidity_2m: number;

                /** HourlyForecastEntry precipitationProbability. */
                public precipitationProbability: number;

                /** HourlyForecastEntry precipitation. */
                public precipitation: number;

                /** HourlyForecastEntry weatherCode. */
                public weatherCode: number;

                /** HourlyForecastEntry windSpeed_10m. */
                public windSpeed_10m: number;

                /** HourlyForecastEntry windDirection_10m. */
                public windDirection_10m: number;

                /** HourlyForecastEntry cloudCover. */
                public cloudCover: number;

                /**
                 * Creates a new HourlyForecastEntry instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns HourlyForecastEntry instance
                 */
                public static create(properties?: bubbaloop.weather.v1.IHourlyForecastEntry): bubbaloop.weather.v1.HourlyForecastEntry;

                /**
                 * Encodes the specified HourlyForecastEntry message. Does not implicitly {@link bubbaloop.weather.v1.HourlyForecastEntry.verify|verify} messages.
                 * @param message HourlyForecastEntry message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: bubbaloop.weather.v1.IHourlyForecastEntry, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified HourlyForecastEntry message, length delimited. Does not implicitly {@link bubbaloop.weather.v1.HourlyForecastEntry.verify|verify} messages.
                 * @param message HourlyForecastEntry message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: bubbaloop.weather.v1.IHourlyForecastEntry, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an HourlyForecastEntry message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns HourlyForecastEntry
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): bubbaloop.weather.v1.HourlyForecastEntry;

                /**
                 * Decodes an HourlyForecastEntry message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns HourlyForecastEntry
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): bubbaloop.weather.v1.HourlyForecastEntry;

                /**
                 * Verifies an HourlyForecastEntry message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates an HourlyForecastEntry message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns HourlyForecastEntry
                 */
                public static fromObject(object: { [k: string]: any }): bubbaloop.weather.v1.HourlyForecastEntry;

                /**
                 * Creates a plain object from an HourlyForecastEntry message. Also converts values to other types if specified.
                 * @param message HourlyForecastEntry
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: bubbaloop.weather.v1.HourlyForecastEntry, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this HourlyForecastEntry to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for HourlyForecastEntry
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of an HourlyForecast. */
            interface IHourlyForecast {

                /** HourlyForecast header */
                header?: (bubbaloop.header.v1.IHeader|null);

                /** HourlyForecast latitude */
                latitude?: (number|null);

                /** HourlyForecast longitude */
                longitude?: (number|null);

                /** HourlyForecast timezone */
                timezone?: (string|null);

                /** HourlyForecast entries */
                entries?: (bubbaloop.weather.v1.IHourlyForecastEntry[]|null);
            }

            /** Represents an HourlyForecast. */
            class HourlyForecast implements IHourlyForecast {

                /**
                 * Constructs a new HourlyForecast.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: bubbaloop.weather.v1.IHourlyForecast);

                /** HourlyForecast header. */
                public header?: (bubbaloop.header.v1.IHeader|null);

                /** HourlyForecast latitude. */
                public latitude: number;

                /** HourlyForecast longitude. */
                public longitude: number;

                /** HourlyForecast timezone. */
                public timezone: string;

                /** HourlyForecast entries. */
                public entries: bubbaloop.weather.v1.IHourlyForecastEntry[];

                /**
                 * Creates a new HourlyForecast instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns HourlyForecast instance
                 */
                public static create(properties?: bubbaloop.weather.v1.IHourlyForecast): bubbaloop.weather.v1.HourlyForecast;

                /**
                 * Encodes the specified HourlyForecast message. Does not implicitly {@link bubbaloop.weather.v1.HourlyForecast.verify|verify} messages.
                 * @param message HourlyForecast message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: bubbaloop.weather.v1.IHourlyForecast, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified HourlyForecast message, length delimited. Does not implicitly {@link bubbaloop.weather.v1.HourlyForecast.verify|verify} messages.
                 * @param message HourlyForecast message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: bubbaloop.weather.v1.IHourlyForecast, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes an HourlyForecast message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns HourlyForecast
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): bubbaloop.weather.v1.HourlyForecast;

                /**
                 * Decodes an HourlyForecast message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns HourlyForecast
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): bubbaloop.weather.v1.HourlyForecast;

                /**
                 * Verifies an HourlyForecast message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates an HourlyForecast message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns HourlyForecast
                 */
                public static fromObject(object: { [k: string]: any }): bubbaloop.weather.v1.HourlyForecast;

                /**
                 * Creates a plain object from an HourlyForecast message. Also converts values to other types if specified.
                 * @param message HourlyForecast
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: bubbaloop.weather.v1.HourlyForecast, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this HourlyForecast to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for HourlyForecast
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a DailyForecastEntry. */
            interface IDailyForecastEntry {

                /** DailyForecastEntry time */
                time?: (number|Long|null);

                /** DailyForecastEntry temperature_2mMax */
                temperature_2mMax?: (number|null);

                /** DailyForecastEntry temperature_2mMin */
                temperature_2mMin?: (number|null);

                /** DailyForecastEntry precipitationSum */
                precipitationSum?: (number|null);

                /** DailyForecastEntry precipitationProbabilityMax */
                precipitationProbabilityMax?: (number|null);

                /** DailyForecastEntry weatherCode */
                weatherCode?: (number|null);

                /** DailyForecastEntry windSpeed_10mMax */
                windSpeed_10mMax?: (number|null);

                /** DailyForecastEntry windGusts_10mMax */
                windGusts_10mMax?: (number|null);

                /** DailyForecastEntry sunrise */
                sunrise?: (string|null);

                /** DailyForecastEntry sunset */
                sunset?: (string|null);
            }

            /** Represents a DailyForecastEntry. */
            class DailyForecastEntry implements IDailyForecastEntry {

                /**
                 * Constructs a new DailyForecastEntry.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: bubbaloop.weather.v1.IDailyForecastEntry);

                /** DailyForecastEntry time. */
                public time: (number|Long);

                /** DailyForecastEntry temperature_2mMax. */
                public temperature_2mMax: number;

                /** DailyForecastEntry temperature_2mMin. */
                public temperature_2mMin: number;

                /** DailyForecastEntry precipitationSum. */
                public precipitationSum: number;

                /** DailyForecastEntry precipitationProbabilityMax. */
                public precipitationProbabilityMax: number;

                /** DailyForecastEntry weatherCode. */
                public weatherCode: number;

                /** DailyForecastEntry windSpeed_10mMax. */
                public windSpeed_10mMax: number;

                /** DailyForecastEntry windGusts_10mMax. */
                public windGusts_10mMax: number;

                /** DailyForecastEntry sunrise. */
                public sunrise: string;

                /** DailyForecastEntry sunset. */
                public sunset: string;

                /**
                 * Creates a new DailyForecastEntry instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns DailyForecastEntry instance
                 */
                public static create(properties?: bubbaloop.weather.v1.IDailyForecastEntry): bubbaloop.weather.v1.DailyForecastEntry;

                /**
                 * Encodes the specified DailyForecastEntry message. Does not implicitly {@link bubbaloop.weather.v1.DailyForecastEntry.verify|verify} messages.
                 * @param message DailyForecastEntry message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: bubbaloop.weather.v1.IDailyForecastEntry, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified DailyForecastEntry message, length delimited. Does not implicitly {@link bubbaloop.weather.v1.DailyForecastEntry.verify|verify} messages.
                 * @param message DailyForecastEntry message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: bubbaloop.weather.v1.IDailyForecastEntry, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a DailyForecastEntry message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns DailyForecastEntry
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): bubbaloop.weather.v1.DailyForecastEntry;

                /**
                 * Decodes a DailyForecastEntry message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns DailyForecastEntry
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): bubbaloop.weather.v1.DailyForecastEntry;

                /**
                 * Verifies a DailyForecastEntry message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a DailyForecastEntry message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns DailyForecastEntry
                 */
                public static fromObject(object: { [k: string]: any }): bubbaloop.weather.v1.DailyForecastEntry;

                /**
                 * Creates a plain object from a DailyForecastEntry message. Also converts values to other types if specified.
                 * @param message DailyForecastEntry
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: bubbaloop.weather.v1.DailyForecastEntry, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this DailyForecastEntry to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for DailyForecastEntry
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a DailyForecast. */
            interface IDailyForecast {

                /** DailyForecast header */
                header?: (bubbaloop.header.v1.IHeader|null);

                /** DailyForecast latitude */
                latitude?: (number|null);

                /** DailyForecast longitude */
                longitude?: (number|null);

                /** DailyForecast timezone */
                timezone?: (string|null);

                /** DailyForecast entries */
                entries?: (bubbaloop.weather.v1.IDailyForecastEntry[]|null);
            }

            /** Represents a DailyForecast. */
            class DailyForecast implements IDailyForecast {

                /**
                 * Constructs a new DailyForecast.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: bubbaloop.weather.v1.IDailyForecast);

                /** DailyForecast header. */
                public header?: (bubbaloop.header.v1.IHeader|null);

                /** DailyForecast latitude. */
                public latitude: number;

                /** DailyForecast longitude. */
                public longitude: number;

                /** DailyForecast timezone. */
                public timezone: string;

                /** DailyForecast entries. */
                public entries: bubbaloop.weather.v1.IDailyForecastEntry[];

                /**
                 * Creates a new DailyForecast instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns DailyForecast instance
                 */
                public static create(properties?: bubbaloop.weather.v1.IDailyForecast): bubbaloop.weather.v1.DailyForecast;

                /**
                 * Encodes the specified DailyForecast message. Does not implicitly {@link bubbaloop.weather.v1.DailyForecast.verify|verify} messages.
                 * @param message DailyForecast message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: bubbaloop.weather.v1.IDailyForecast, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified DailyForecast message, length delimited. Does not implicitly {@link bubbaloop.weather.v1.DailyForecast.verify|verify} messages.
                 * @param message DailyForecast message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: bubbaloop.weather.v1.IDailyForecast, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a DailyForecast message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns DailyForecast
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): bubbaloop.weather.v1.DailyForecast;

                /**
                 * Decodes a DailyForecast message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns DailyForecast
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): bubbaloop.weather.v1.DailyForecast;

                /**
                 * Verifies a DailyForecast message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a DailyForecast message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns DailyForecast
                 */
                public static fromObject(object: { [k: string]: any }): bubbaloop.weather.v1.DailyForecast;

                /**
                 * Creates a plain object from a DailyForecast message. Also converts values to other types if specified.
                 * @param message DailyForecast
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: bubbaloop.weather.v1.DailyForecast, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this DailyForecast to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for DailyForecast
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }

            /** Properties of a LocationConfig. */
            interface ILocationConfig {

                /** LocationConfig latitude */
                latitude?: (number|null);

                /** LocationConfig longitude */
                longitude?: (number|null);

                /** LocationConfig timezone */
                timezone?: (string|null);
            }

            /** Represents a LocationConfig. */
            class LocationConfig implements ILocationConfig {

                /**
                 * Constructs a new LocationConfig.
                 * @param [properties] Properties to set
                 */
                constructor(properties?: bubbaloop.weather.v1.ILocationConfig);

                /** LocationConfig latitude. */
                public latitude: number;

                /** LocationConfig longitude. */
                public longitude: number;

                /** LocationConfig timezone. */
                public timezone: string;

                /**
                 * Creates a new LocationConfig instance using the specified properties.
                 * @param [properties] Properties to set
                 * @returns LocationConfig instance
                 */
                public static create(properties?: bubbaloop.weather.v1.ILocationConfig): bubbaloop.weather.v1.LocationConfig;

                /**
                 * Encodes the specified LocationConfig message. Does not implicitly {@link bubbaloop.weather.v1.LocationConfig.verify|verify} messages.
                 * @param message LocationConfig message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encode(message: bubbaloop.weather.v1.ILocationConfig, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Encodes the specified LocationConfig message, length delimited. Does not implicitly {@link bubbaloop.weather.v1.LocationConfig.verify|verify} messages.
                 * @param message LocationConfig message or plain object to encode
                 * @param [writer] Writer to encode to
                 * @returns Writer
                 */
                public static encodeDelimited(message: bubbaloop.weather.v1.ILocationConfig, writer?: $protobuf.Writer): $protobuf.Writer;

                /**
                 * Decodes a LocationConfig message from the specified reader or buffer.
                 * @param reader Reader or buffer to decode from
                 * @param [length] Message length if known beforehand
                 * @returns LocationConfig
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decode(reader: ($protobuf.Reader|Uint8Array), length?: number): bubbaloop.weather.v1.LocationConfig;

                /**
                 * Decodes a LocationConfig message from the specified reader or buffer, length delimited.
                 * @param reader Reader or buffer to decode from
                 * @returns LocationConfig
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                public static decodeDelimited(reader: ($protobuf.Reader|Uint8Array)): bubbaloop.weather.v1.LocationConfig;

                /**
                 * Verifies a LocationConfig message.
                 * @param message Plain object to verify
                 * @returns `null` if valid, otherwise the reason why it is not
                 */
                public static verify(message: { [k: string]: any }): (string|null);

                /**
                 * Creates a LocationConfig message from a plain object. Also converts values to their respective internal types.
                 * @param object Plain object
                 * @returns LocationConfig
                 */
                public static fromObject(object: { [k: string]: any }): bubbaloop.weather.v1.LocationConfig;

                /**
                 * Creates a plain object from a LocationConfig message. Also converts values to other types if specified.
                 * @param message LocationConfig
                 * @param [options] Conversion options
                 * @returns Plain object
                 */
                public static toObject(message: bubbaloop.weather.v1.LocationConfig, options?: $protobuf.IConversionOptions): { [k: string]: any };

                /**
                 * Converts this LocationConfig to JSON.
                 * @returns JSON object
                 */
                public toJSON(): { [k: string]: any };

                /**
                 * Gets the default type url for LocationConfig
                 * @param [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns The default type url
                 */
                public static getTypeUrl(typeUrlPrefix?: string): string;
            }
        }
    }
}
