/*eslint-disable block-scoped-var, id-length, no-control-regex, no-magic-numbers, no-prototype-builtins, no-redeclare, no-shadow, no-var, sort-vars*/
import * as $protobuf from "protobufjs/minimal";

// Common aliases
const $Reader = $protobuf.Reader, $Writer = $protobuf.Writer, $util = $protobuf.util;

// Exported root namespace
const $root = $protobuf.roots["default"] || ($protobuf.roots["default"] = {});

export const bubbaloop = $root.bubbaloop = (() => {

    /**
     * Namespace bubbaloop.
     * @exports bubbaloop
     * @namespace
     */
    const bubbaloop = {};

    bubbaloop.header = (function() {

        /**
         * Namespace header.
         * @memberof bubbaloop
         * @namespace
         */
        const header = {};

        header.v1 = (function() {

            /**
             * Namespace v1.
             * @memberof bubbaloop.header
             * @namespace
             */
            const v1 = {};

            v1.Header = (function() {

                /**
                 * Properties of a Header.
                 * @memberof bubbaloop.header.v1
                 * @interface IHeader
                 * @property {number|Long|null} [acqTime] Header acqTime
                 * @property {number|Long|null} [pubTime] Header pubTime
                 * @property {number|null} [sequence] Header sequence
                 * @property {string|null} [frameId] Header frameId
                 */

                /**
                 * Constructs a new Header.
                 * @memberof bubbaloop.header.v1
                 * @classdesc Represents a Header.
                 * @implements IHeader
                 * @constructor
                 * @param {bubbaloop.header.v1.IHeader=} [properties] Properties to set
                 */
                function Header(properties) {
                    if (properties)
                        for (let keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * Header acqTime.
                 * @member {number|Long} acqTime
                 * @memberof bubbaloop.header.v1.Header
                 * @instance
                 */
                Header.prototype.acqTime = $util.Long ? $util.Long.fromBits(0,0,true) : 0;

                /**
                 * Header pubTime.
                 * @member {number|Long} pubTime
                 * @memberof bubbaloop.header.v1.Header
                 * @instance
                 */
                Header.prototype.pubTime = $util.Long ? $util.Long.fromBits(0,0,true) : 0;

                /**
                 * Header sequence.
                 * @member {number} sequence
                 * @memberof bubbaloop.header.v1.Header
                 * @instance
                 */
                Header.prototype.sequence = 0;

                /**
                 * Header frameId.
                 * @member {string} frameId
                 * @memberof bubbaloop.header.v1.Header
                 * @instance
                 */
                Header.prototype.frameId = "";

                /**
                 * Creates a new Header instance using the specified properties.
                 * @function create
                 * @memberof bubbaloop.header.v1.Header
                 * @static
                 * @param {bubbaloop.header.v1.IHeader=} [properties] Properties to set
                 * @returns {bubbaloop.header.v1.Header} Header instance
                 */
                Header.create = function create(properties) {
                    return new Header(properties);
                };

                /**
                 * Encodes the specified Header message. Does not implicitly {@link bubbaloop.header.v1.Header.verify|verify} messages.
                 * @function encode
                 * @memberof bubbaloop.header.v1.Header
                 * @static
                 * @param {bubbaloop.header.v1.IHeader} message Header message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                Header.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.acqTime != null && Object.hasOwnProperty.call(message, "acqTime"))
                        writer.uint32(/* id 1, wireType 0 =*/8).uint64(message.acqTime);
                    if (message.pubTime != null && Object.hasOwnProperty.call(message, "pubTime"))
                        writer.uint32(/* id 2, wireType 0 =*/16).uint64(message.pubTime);
                    if (message.sequence != null && Object.hasOwnProperty.call(message, "sequence"))
                        writer.uint32(/* id 3, wireType 0 =*/24).uint32(message.sequence);
                    if (message.frameId != null && Object.hasOwnProperty.call(message, "frameId"))
                        writer.uint32(/* id 4, wireType 2 =*/34).string(message.frameId);
                    return writer;
                };

                /**
                 * Encodes the specified Header message, length delimited. Does not implicitly {@link bubbaloop.header.v1.Header.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof bubbaloop.header.v1.Header
                 * @static
                 * @param {bubbaloop.header.v1.IHeader} message Header message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                Header.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a Header message from the specified reader or buffer.
                 * @function decode
                 * @memberof bubbaloop.header.v1.Header
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {bubbaloop.header.v1.Header} Header
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                Header.decode = function decode(reader, length) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    let end = length === undefined ? reader.len : reader.pos + length, message = new $root.bubbaloop.header.v1.Header();
                    while (reader.pos < end) {
                        let tag = reader.uint32();
                        if (false)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.acqTime = reader.uint64();
                                break;
                            }
                        case 2: {
                                message.pubTime = reader.uint64();
                                break;
                            }
                        case 3: {
                                message.sequence = reader.uint32();
                                break;
                            }
                        case 4: {
                                message.frameId = reader.string();
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a Header message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof bubbaloop.header.v1.Header
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {bubbaloop.header.v1.Header} Header
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                Header.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a Header message.
                 * @function verify
                 * @memberof bubbaloop.header.v1.Header
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                Header.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.acqTime != null && message.hasOwnProperty("acqTime"))
                        if (!$util.isInteger(message.acqTime) && !(message.acqTime && $util.isInteger(message.acqTime.low) && $util.isInteger(message.acqTime.high)))
                            return "acqTime: integer|Long expected";
                    if (message.pubTime != null && message.hasOwnProperty("pubTime"))
                        if (!$util.isInteger(message.pubTime) && !(message.pubTime && $util.isInteger(message.pubTime.low) && $util.isInteger(message.pubTime.high)))
                            return "pubTime: integer|Long expected";
                    if (message.sequence != null && message.hasOwnProperty("sequence"))
                        if (!$util.isInteger(message.sequence))
                            return "sequence: integer expected";
                    if (message.frameId != null && message.hasOwnProperty("frameId"))
                        if (!$util.isString(message.frameId))
                            return "frameId: string expected";
                    return null;
                };

                /**
                 * Creates a Header message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof bubbaloop.header.v1.Header
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {bubbaloop.header.v1.Header} Header
                 */
                Header.fromObject = function fromObject(object) {
                    if (object instanceof $root.bubbaloop.header.v1.Header)
                        return object;
                    let message = new $root.bubbaloop.header.v1.Header();
                    if (object.acqTime != null)
                        if ($util.Long)
                            (message.acqTime = $util.Long.fromValue(object.acqTime)).unsigned = true;
                        else if (typeof object.acqTime === "string")
                            message.acqTime = parseInt(object.acqTime, 10);
                        else if (typeof object.acqTime === "number")
                            message.acqTime = object.acqTime;
                        else if (typeof object.acqTime === "object")
                            message.acqTime = new $util.LongBits(object.acqTime.low >>> 0, object.acqTime.high >>> 0).toNumber(true);
                    if (object.pubTime != null)
                        if ($util.Long)
                            (message.pubTime = $util.Long.fromValue(object.pubTime)).unsigned = true;
                        else if (typeof object.pubTime === "string")
                            message.pubTime = parseInt(object.pubTime, 10);
                        else if (typeof object.pubTime === "number")
                            message.pubTime = object.pubTime;
                        else if (typeof object.pubTime === "object")
                            message.pubTime = new $util.LongBits(object.pubTime.low >>> 0, object.pubTime.high >>> 0).toNumber(true);
                    if (object.sequence != null)
                        message.sequence = object.sequence >>> 0;
                    if (object.frameId != null)
                        message.frameId = String(object.frameId);
                    return message;
                };

                /**
                 * Creates a plain object from a Header message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof bubbaloop.header.v1.Header
                 * @static
                 * @param {bubbaloop.header.v1.Header} message Header
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                Header.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    let object = {};
                    if (options.defaults) {
                        if ($util.Long) {
                            let long = new $util.Long(0, 0, true);
                            object.acqTime = options.longs === String ? long.toString() : options.longs === Number ? long.toNumber() : long;
                        } else
                            object.acqTime = options.longs === String ? "0" : 0;
                        if ($util.Long) {
                            let long = new $util.Long(0, 0, true);
                            object.pubTime = options.longs === String ? long.toString() : options.longs === Number ? long.toNumber() : long;
                        } else
                            object.pubTime = options.longs === String ? "0" : 0;
                        object.sequence = 0;
                        object.frameId = "";
                    }
                    if (message.acqTime != null && message.hasOwnProperty("acqTime"))
                        if (typeof message.acqTime === "number")
                            object.acqTime = options.longs === String ? String(message.acqTime) : message.acqTime;
                        else
                            object.acqTime = options.longs === String ? $util.Long.prototype.toString.call(message.acqTime) : options.longs === Number ? new $util.LongBits(message.acqTime.low >>> 0, message.acqTime.high >>> 0).toNumber(true) : message.acqTime;
                    if (message.pubTime != null && message.hasOwnProperty("pubTime"))
                        if (typeof message.pubTime === "number")
                            object.pubTime = options.longs === String ? String(message.pubTime) : message.pubTime;
                        else
                            object.pubTime = options.longs === String ? $util.Long.prototype.toString.call(message.pubTime) : options.longs === Number ? new $util.LongBits(message.pubTime.low >>> 0, message.pubTime.high >>> 0).toNumber(true) : message.pubTime;
                    if (message.sequence != null && message.hasOwnProperty("sequence"))
                        object.sequence = message.sequence;
                    if (message.frameId != null && message.hasOwnProperty("frameId"))
                        object.frameId = message.frameId;
                    return object;
                };

                /**
                 * Converts this Header to JSON.
                 * @function toJSON
                 * @memberof bubbaloop.header.v1.Header
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                Header.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for Header
                 * @function getTypeUrl
                 * @memberof bubbaloop.header.v1.Header
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                Header.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/bubbaloop.header.v1.Header";
                };

                return Header;
            })();

            return v1;
        })();

        return header;
    })();

    bubbaloop.camera = (function() {

        /**
         * Namespace camera.
         * @memberof bubbaloop
         * @namespace
         */
        const camera = {};

        camera.v1 = (function() {

            /**
             * Namespace v1.
             * @memberof bubbaloop.camera
             * @namespace
             */
            const v1 = {};

            v1.CompressedImage = (function() {

                /**
                 * Properties of a CompressedImage.
                 * @memberof bubbaloop.camera.v1
                 * @interface ICompressedImage
                 * @property {bubbaloop.header.v1.IHeader|null} [header] CompressedImage header
                 * @property {string|null} [format] CompressedImage format
                 * @property {Uint8Array|null} [data] CompressedImage data
                 */

                /**
                 * Constructs a new CompressedImage.
                 * @memberof bubbaloop.camera.v1
                 * @classdesc Represents a CompressedImage.
                 * @implements ICompressedImage
                 * @constructor
                 * @param {bubbaloop.camera.v1.ICompressedImage=} [properties] Properties to set
                 */
                function CompressedImage(properties) {
                    if (properties)
                        for (let keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * CompressedImage header.
                 * @member {bubbaloop.header.v1.IHeader|null|undefined} header
                 * @memberof bubbaloop.camera.v1.CompressedImage
                 * @instance
                 */
                CompressedImage.prototype.header = null;

                /**
                 * CompressedImage format.
                 * @member {string} format
                 * @memberof bubbaloop.camera.v1.CompressedImage
                 * @instance
                 */
                CompressedImage.prototype.format = "";

                /**
                 * CompressedImage data.
                 * @member {Uint8Array} data
                 * @memberof bubbaloop.camera.v1.CompressedImage
                 * @instance
                 */
                CompressedImage.prototype.data = $util.newBuffer([]);

                /**
                 * Creates a new CompressedImage instance using the specified properties.
                 * @function create
                 * @memberof bubbaloop.camera.v1.CompressedImage
                 * @static
                 * @param {bubbaloop.camera.v1.ICompressedImage=} [properties] Properties to set
                 * @returns {bubbaloop.camera.v1.CompressedImage} CompressedImage instance
                 */
                CompressedImage.create = function create(properties) {
                    return new CompressedImage(properties);
                };

                /**
                 * Encodes the specified CompressedImage message. Does not implicitly {@link bubbaloop.camera.v1.CompressedImage.verify|verify} messages.
                 * @function encode
                 * @memberof bubbaloop.camera.v1.CompressedImage
                 * @static
                 * @param {bubbaloop.camera.v1.ICompressedImage} message CompressedImage message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                CompressedImage.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.header != null && Object.hasOwnProperty.call(message, "header"))
                        $root.bubbaloop.header.v1.Header.encode(message.header, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    if (message.format != null && Object.hasOwnProperty.call(message, "format"))
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.format);
                    if (message.data != null && Object.hasOwnProperty.call(message, "data"))
                        writer.uint32(/* id 3, wireType 2 =*/26).bytes(message.data);
                    return writer;
                };

                /**
                 * Encodes the specified CompressedImage message, length delimited. Does not implicitly {@link bubbaloop.camera.v1.CompressedImage.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof bubbaloop.camera.v1.CompressedImage
                 * @static
                 * @param {bubbaloop.camera.v1.ICompressedImage} message CompressedImage message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                CompressedImage.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a CompressedImage message from the specified reader or buffer.
                 * @function decode
                 * @memberof bubbaloop.camera.v1.CompressedImage
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {bubbaloop.camera.v1.CompressedImage} CompressedImage
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                CompressedImage.decode = function decode(reader, length) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    let end = length === undefined ? reader.len : reader.pos + length, message = new $root.bubbaloop.camera.v1.CompressedImage();
                    while (reader.pos < end) {
                        let tag = reader.uint32();
                        if (false)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.header = $root.bubbaloop.header.v1.Header.decode(reader, reader.uint32());
                                break;
                            }
                        case 2: {
                                message.format = reader.string();
                                break;
                            }
                        case 3: {
                                message.data = reader.bytes();
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a CompressedImage message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof bubbaloop.camera.v1.CompressedImage
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {bubbaloop.camera.v1.CompressedImage} CompressedImage
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                CompressedImage.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a CompressedImage message.
                 * @function verify
                 * @memberof bubbaloop.camera.v1.CompressedImage
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                CompressedImage.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.header != null && message.hasOwnProperty("header")) {
                        let error = $root.bubbaloop.header.v1.Header.verify(message.header);
                        if (error)
                            return "header." + error;
                    }
                    if (message.format != null && message.hasOwnProperty("format"))
                        if (!$util.isString(message.format))
                            return "format: string expected";
                    if (message.data != null && message.hasOwnProperty("data"))
                        if (!(message.data && typeof message.data.length === "number" || $util.isString(message.data)))
                            return "data: buffer expected";
                    return null;
                };

                /**
                 * Creates a CompressedImage message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof bubbaloop.camera.v1.CompressedImage
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {bubbaloop.camera.v1.CompressedImage} CompressedImage
                 */
                CompressedImage.fromObject = function fromObject(object) {
                    if (object instanceof $root.bubbaloop.camera.v1.CompressedImage)
                        return object;
                    let message = new $root.bubbaloop.camera.v1.CompressedImage();
                    if (object.header != null) {
                        if (typeof object.header !== "object")
                            throw TypeError(".bubbaloop.camera.v1.CompressedImage.header: object expected");
                        message.header = $root.bubbaloop.header.v1.Header.fromObject(object.header);
                    }
                    if (object.format != null)
                        message.format = String(object.format);
                    if (object.data != null)
                        if (typeof object.data === "string")
                            $util.base64.decode(object.data, message.data = $util.newBuffer($util.base64.length(object.data)), 0);
                        else if (object.data.length >= 0)
                            message.data = object.data;
                    return message;
                };

                /**
                 * Creates a plain object from a CompressedImage message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof bubbaloop.camera.v1.CompressedImage
                 * @static
                 * @param {bubbaloop.camera.v1.CompressedImage} message CompressedImage
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                CompressedImage.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    let object = {};
                    if (options.defaults) {
                        object.header = null;
                        object.format = "";
                        if (options.bytes === String)
                            object.data = "";
                        else {
                            object.data = [];
                            if (options.bytes !== Array)
                                object.data = $util.newBuffer(object.data);
                        }
                    }
                    if (message.header != null && message.hasOwnProperty("header"))
                        object.header = $root.bubbaloop.header.v1.Header.toObject(message.header, options);
                    if (message.format != null && message.hasOwnProperty("format"))
                        object.format = message.format;
                    if (message.data != null && message.hasOwnProperty("data"))
                        object.data = options.bytes === String ? $util.base64.encode(message.data, 0, message.data.length) : options.bytes === Array ? Array.prototype.slice.call(message.data) : message.data;
                    return object;
                };

                /**
                 * Converts this CompressedImage to JSON.
                 * @function toJSON
                 * @memberof bubbaloop.camera.v1.CompressedImage
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                CompressedImage.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for CompressedImage
                 * @function getTypeUrl
                 * @memberof bubbaloop.camera.v1.CompressedImage
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                CompressedImage.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/bubbaloop.camera.v1.CompressedImage";
                };

                return CompressedImage;
            })();

            v1.RawImage = (function() {

                /**
                 * Properties of a RawImage.
                 * @memberof bubbaloop.camera.v1
                 * @interface IRawImage
                 * @property {bubbaloop.header.v1.IHeader|null} [header] RawImage header
                 * @property {number|null} [width] RawImage width
                 * @property {number|null} [height] RawImage height
                 * @property {string|null} [encoding] RawImage encoding
                 * @property {number|null} [step] RawImage step
                 * @property {Uint8Array|null} [data] RawImage data
                 */

                /**
                 * Constructs a new RawImage.
                 * @memberof bubbaloop.camera.v1
                 * @classdesc Represents a RawImage.
                 * @implements IRawImage
                 * @constructor
                 * @param {bubbaloop.camera.v1.IRawImage=} [properties] Properties to set
                 */
                function RawImage(properties) {
                    if (properties)
                        for (let keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * RawImage header.
                 * @member {bubbaloop.header.v1.IHeader|null|undefined} header
                 * @memberof bubbaloop.camera.v1.RawImage
                 * @instance
                 */
                RawImage.prototype.header = null;

                /**
                 * RawImage width.
                 * @member {number} width
                 * @memberof bubbaloop.camera.v1.RawImage
                 * @instance
                 */
                RawImage.prototype.width = 0;

                /**
                 * RawImage height.
                 * @member {number} height
                 * @memberof bubbaloop.camera.v1.RawImage
                 * @instance
                 */
                RawImage.prototype.height = 0;

                /**
                 * RawImage encoding.
                 * @member {string} encoding
                 * @memberof bubbaloop.camera.v1.RawImage
                 * @instance
                 */
                RawImage.prototype.encoding = "";

                /**
                 * RawImage step.
                 * @member {number} step
                 * @memberof bubbaloop.camera.v1.RawImage
                 * @instance
                 */
                RawImage.prototype.step = 0;

                /**
                 * RawImage data.
                 * @member {Uint8Array} data
                 * @memberof bubbaloop.camera.v1.RawImage
                 * @instance
                 */
                RawImage.prototype.data = $util.newBuffer([]);

                /**
                 * Creates a new RawImage instance using the specified properties.
                 * @function create
                 * @memberof bubbaloop.camera.v1.RawImage
                 * @static
                 * @param {bubbaloop.camera.v1.IRawImage=} [properties] Properties to set
                 * @returns {bubbaloop.camera.v1.RawImage} RawImage instance
                 */
                RawImage.create = function create(properties) {
                    return new RawImage(properties);
                };

                /**
                 * Encodes the specified RawImage message. Does not implicitly {@link bubbaloop.camera.v1.RawImage.verify|verify} messages.
                 * @function encode
                 * @memberof bubbaloop.camera.v1.RawImage
                 * @static
                 * @param {bubbaloop.camera.v1.IRawImage} message RawImage message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                RawImage.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.header != null && Object.hasOwnProperty.call(message, "header"))
                        $root.bubbaloop.header.v1.Header.encode(message.header, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    if (message.width != null && Object.hasOwnProperty.call(message, "width"))
                        writer.uint32(/* id 2, wireType 0 =*/16).uint32(message.width);
                    if (message.height != null && Object.hasOwnProperty.call(message, "height"))
                        writer.uint32(/* id 3, wireType 0 =*/24).uint32(message.height);
                    if (message.encoding != null && Object.hasOwnProperty.call(message, "encoding"))
                        writer.uint32(/* id 4, wireType 2 =*/34).string(message.encoding);
                    if (message.step != null && Object.hasOwnProperty.call(message, "step"))
                        writer.uint32(/* id 5, wireType 0 =*/40).uint32(message.step);
                    if (message.data != null && Object.hasOwnProperty.call(message, "data"))
                        writer.uint32(/* id 6, wireType 2 =*/50).bytes(message.data);
                    return writer;
                };

                /**
                 * Encodes the specified RawImage message, length delimited. Does not implicitly {@link bubbaloop.camera.v1.RawImage.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof bubbaloop.camera.v1.RawImage
                 * @static
                 * @param {bubbaloop.camera.v1.IRawImage} message RawImage message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                RawImage.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a RawImage message from the specified reader or buffer.
                 * @function decode
                 * @memberof bubbaloop.camera.v1.RawImage
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {bubbaloop.camera.v1.RawImage} RawImage
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                RawImage.decode = function decode(reader, length) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    let end = length === undefined ? reader.len : reader.pos + length, message = new $root.bubbaloop.camera.v1.RawImage();
                    while (reader.pos < end) {
                        let tag = reader.uint32();
                        if (false)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.header = $root.bubbaloop.header.v1.Header.decode(reader, reader.uint32());
                                break;
                            }
                        case 2: {
                                message.width = reader.uint32();
                                break;
                            }
                        case 3: {
                                message.height = reader.uint32();
                                break;
                            }
                        case 4: {
                                message.encoding = reader.string();
                                break;
                            }
                        case 5: {
                                message.step = reader.uint32();
                                break;
                            }
                        case 6: {
                                message.data = reader.bytes();
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a RawImage message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof bubbaloop.camera.v1.RawImage
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {bubbaloop.camera.v1.RawImage} RawImage
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                RawImage.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a RawImage message.
                 * @function verify
                 * @memberof bubbaloop.camera.v1.RawImage
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                RawImage.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.header != null && message.hasOwnProperty("header")) {
                        let error = $root.bubbaloop.header.v1.Header.verify(message.header);
                        if (error)
                            return "header." + error;
                    }
                    if (message.width != null && message.hasOwnProperty("width"))
                        if (!$util.isInteger(message.width))
                            return "width: integer expected";
                    if (message.height != null && message.hasOwnProperty("height"))
                        if (!$util.isInteger(message.height))
                            return "height: integer expected";
                    if (message.encoding != null && message.hasOwnProperty("encoding"))
                        if (!$util.isString(message.encoding))
                            return "encoding: string expected";
                    if (message.step != null && message.hasOwnProperty("step"))
                        if (!$util.isInteger(message.step))
                            return "step: integer expected";
                    if (message.data != null && message.hasOwnProperty("data"))
                        if (!(message.data && typeof message.data.length === "number" || $util.isString(message.data)))
                            return "data: buffer expected";
                    return null;
                };

                /**
                 * Creates a RawImage message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof bubbaloop.camera.v1.RawImage
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {bubbaloop.camera.v1.RawImage} RawImage
                 */
                RawImage.fromObject = function fromObject(object) {
                    if (object instanceof $root.bubbaloop.camera.v1.RawImage)
                        return object;
                    let message = new $root.bubbaloop.camera.v1.RawImage();
                    if (object.header != null) {
                        if (typeof object.header !== "object")
                            throw TypeError(".bubbaloop.camera.v1.RawImage.header: object expected");
                        message.header = $root.bubbaloop.header.v1.Header.fromObject(object.header);
                    }
                    if (object.width != null)
                        message.width = object.width >>> 0;
                    if (object.height != null)
                        message.height = object.height >>> 0;
                    if (object.encoding != null)
                        message.encoding = String(object.encoding);
                    if (object.step != null)
                        message.step = object.step >>> 0;
                    if (object.data != null)
                        if (typeof object.data === "string")
                            $util.base64.decode(object.data, message.data = $util.newBuffer($util.base64.length(object.data)), 0);
                        else if (object.data.length >= 0)
                            message.data = object.data;
                    return message;
                };

                /**
                 * Creates a plain object from a RawImage message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof bubbaloop.camera.v1.RawImage
                 * @static
                 * @param {bubbaloop.camera.v1.RawImage} message RawImage
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                RawImage.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    let object = {};
                    if (options.defaults) {
                        object.header = null;
                        object.width = 0;
                        object.height = 0;
                        object.encoding = "";
                        object.step = 0;
                        if (options.bytes === String)
                            object.data = "";
                        else {
                            object.data = [];
                            if (options.bytes !== Array)
                                object.data = $util.newBuffer(object.data);
                        }
                    }
                    if (message.header != null && message.hasOwnProperty("header"))
                        object.header = $root.bubbaloop.header.v1.Header.toObject(message.header, options);
                    if (message.width != null && message.hasOwnProperty("width"))
                        object.width = message.width;
                    if (message.height != null && message.hasOwnProperty("height"))
                        object.height = message.height;
                    if (message.encoding != null && message.hasOwnProperty("encoding"))
                        object.encoding = message.encoding;
                    if (message.step != null && message.hasOwnProperty("step"))
                        object.step = message.step;
                    if (message.data != null && message.hasOwnProperty("data"))
                        object.data = options.bytes === String ? $util.base64.encode(message.data, 0, message.data.length) : options.bytes === Array ? Array.prototype.slice.call(message.data) : message.data;
                    return object;
                };

                /**
                 * Converts this RawImage to JSON.
                 * @function toJSON
                 * @memberof bubbaloop.camera.v1.RawImage
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                RawImage.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for RawImage
                 * @function getTypeUrl
                 * @memberof bubbaloop.camera.v1.RawImage
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                RawImage.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/bubbaloop.camera.v1.RawImage";
                };

                return RawImage;
            })();

            return v1;
        })();

        return camera;
    })();

    bubbaloop.weather = (function() {

        /**
         * Namespace weather.
         * @memberof bubbaloop
         * @namespace
         */
        const weather = {};

        weather.v1 = (function() {

            /**
             * Namespace v1.
             * @memberof bubbaloop.weather
             * @namespace
             */
            const v1 = {};

            v1.CurrentWeather = (function() {

                /**
                 * Properties of a CurrentWeather.
                 * @memberof bubbaloop.weather.v1
                 * @interface ICurrentWeather
                 * @property {bubbaloop.header.v1.IHeader|null} [header] CurrentWeather header
                 * @property {number|null} [latitude] CurrentWeather latitude
                 * @property {number|null} [longitude] CurrentWeather longitude
                 * @property {string|null} [timezone] CurrentWeather timezone
                 * @property {number|null} [temperature_2m] CurrentWeather temperature_2m
                 * @property {number|null} [relativeHumidity_2m] CurrentWeather relativeHumidity_2m
                 * @property {number|null} [apparentTemperature] CurrentWeather apparentTemperature
                 * @property {number|null} [precipitation] CurrentWeather precipitation
                 * @property {number|null} [rain] CurrentWeather rain
                 * @property {number|null} [windSpeed_10m] CurrentWeather windSpeed_10m
                 * @property {number|null} [windDirection_10m] CurrentWeather windDirection_10m
                 * @property {number|null} [windGusts_10m] CurrentWeather windGusts_10m
                 * @property {number|null} [weatherCode] CurrentWeather weatherCode
                 * @property {number|null} [cloudCover] CurrentWeather cloudCover
                 * @property {number|null} [pressureMsl] CurrentWeather pressureMsl
                 * @property {number|null} [surfacePressure] CurrentWeather surfacePressure
                 * @property {number|null} [isDay] CurrentWeather isDay
                 */

                /**
                 * Constructs a new CurrentWeather.
                 * @memberof bubbaloop.weather.v1
                 * @classdesc Represents a CurrentWeather.
                 * @implements ICurrentWeather
                 * @constructor
                 * @param {bubbaloop.weather.v1.ICurrentWeather=} [properties] Properties to set
                 */
                function CurrentWeather(properties) {
                    if (properties)
                        for (let keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * CurrentWeather header.
                 * @member {bubbaloop.header.v1.IHeader|null|undefined} header
                 * @memberof bubbaloop.weather.v1.CurrentWeather
                 * @instance
                 */
                CurrentWeather.prototype.header = null;

                /**
                 * CurrentWeather latitude.
                 * @member {number} latitude
                 * @memberof bubbaloop.weather.v1.CurrentWeather
                 * @instance
                 */
                CurrentWeather.prototype.latitude = 0;

                /**
                 * CurrentWeather longitude.
                 * @member {number} longitude
                 * @memberof bubbaloop.weather.v1.CurrentWeather
                 * @instance
                 */
                CurrentWeather.prototype.longitude = 0;

                /**
                 * CurrentWeather timezone.
                 * @member {string} timezone
                 * @memberof bubbaloop.weather.v1.CurrentWeather
                 * @instance
                 */
                CurrentWeather.prototype.timezone = "";

                /**
                 * CurrentWeather temperature_2m.
                 * @member {number} temperature_2m
                 * @memberof bubbaloop.weather.v1.CurrentWeather
                 * @instance
                 */
                CurrentWeather.prototype.temperature_2m = 0;

                /**
                 * CurrentWeather relativeHumidity_2m.
                 * @member {number} relativeHumidity_2m
                 * @memberof bubbaloop.weather.v1.CurrentWeather
                 * @instance
                 */
                CurrentWeather.prototype.relativeHumidity_2m = 0;

                /**
                 * CurrentWeather apparentTemperature.
                 * @member {number} apparentTemperature
                 * @memberof bubbaloop.weather.v1.CurrentWeather
                 * @instance
                 */
                CurrentWeather.prototype.apparentTemperature = 0;

                /**
                 * CurrentWeather precipitation.
                 * @member {number} precipitation
                 * @memberof bubbaloop.weather.v1.CurrentWeather
                 * @instance
                 */
                CurrentWeather.prototype.precipitation = 0;

                /**
                 * CurrentWeather rain.
                 * @member {number} rain
                 * @memberof bubbaloop.weather.v1.CurrentWeather
                 * @instance
                 */
                CurrentWeather.prototype.rain = 0;

                /**
                 * CurrentWeather windSpeed_10m.
                 * @member {number} windSpeed_10m
                 * @memberof bubbaloop.weather.v1.CurrentWeather
                 * @instance
                 */
                CurrentWeather.prototype.windSpeed_10m = 0;

                /**
                 * CurrentWeather windDirection_10m.
                 * @member {number} windDirection_10m
                 * @memberof bubbaloop.weather.v1.CurrentWeather
                 * @instance
                 */
                CurrentWeather.prototype.windDirection_10m = 0;

                /**
                 * CurrentWeather windGusts_10m.
                 * @member {number} windGusts_10m
                 * @memberof bubbaloop.weather.v1.CurrentWeather
                 * @instance
                 */
                CurrentWeather.prototype.windGusts_10m = 0;

                /**
                 * CurrentWeather weatherCode.
                 * @member {number} weatherCode
                 * @memberof bubbaloop.weather.v1.CurrentWeather
                 * @instance
                 */
                CurrentWeather.prototype.weatherCode = 0;

                /**
                 * CurrentWeather cloudCover.
                 * @member {number} cloudCover
                 * @memberof bubbaloop.weather.v1.CurrentWeather
                 * @instance
                 */
                CurrentWeather.prototype.cloudCover = 0;

                /**
                 * CurrentWeather pressureMsl.
                 * @member {number} pressureMsl
                 * @memberof bubbaloop.weather.v1.CurrentWeather
                 * @instance
                 */
                CurrentWeather.prototype.pressureMsl = 0;

                /**
                 * CurrentWeather surfacePressure.
                 * @member {number} surfacePressure
                 * @memberof bubbaloop.weather.v1.CurrentWeather
                 * @instance
                 */
                CurrentWeather.prototype.surfacePressure = 0;

                /**
                 * CurrentWeather isDay.
                 * @member {number} isDay
                 * @memberof bubbaloop.weather.v1.CurrentWeather
                 * @instance
                 */
                CurrentWeather.prototype.isDay = 0;

                /**
                 * Creates a new CurrentWeather instance using the specified properties.
                 * @function create
                 * @memberof bubbaloop.weather.v1.CurrentWeather
                 * @static
                 * @param {bubbaloop.weather.v1.ICurrentWeather=} [properties] Properties to set
                 * @returns {bubbaloop.weather.v1.CurrentWeather} CurrentWeather instance
                 */
                CurrentWeather.create = function create(properties) {
                    return new CurrentWeather(properties);
                };

                /**
                 * Encodes the specified CurrentWeather message. Does not implicitly {@link bubbaloop.weather.v1.CurrentWeather.verify|verify} messages.
                 * @function encode
                 * @memberof bubbaloop.weather.v1.CurrentWeather
                 * @static
                 * @param {bubbaloop.weather.v1.ICurrentWeather} message CurrentWeather message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                CurrentWeather.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.header != null && Object.hasOwnProperty.call(message, "header"))
                        $root.bubbaloop.header.v1.Header.encode(message.header, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    if (message.latitude != null && Object.hasOwnProperty.call(message, "latitude"))
                        writer.uint32(/* id 2, wireType 1 =*/17).double(message.latitude);
                    if (message.longitude != null && Object.hasOwnProperty.call(message, "longitude"))
                        writer.uint32(/* id 3, wireType 1 =*/25).double(message.longitude);
                    if (message.timezone != null && Object.hasOwnProperty.call(message, "timezone"))
                        writer.uint32(/* id 4, wireType 2 =*/34).string(message.timezone);
                    if (message.temperature_2m != null && Object.hasOwnProperty.call(message, "temperature_2m"))
                        writer.uint32(/* id 5, wireType 1 =*/41).double(message.temperature_2m);
                    if (message.relativeHumidity_2m != null && Object.hasOwnProperty.call(message, "relativeHumidity_2m"))
                        writer.uint32(/* id 6, wireType 1 =*/49).double(message.relativeHumidity_2m);
                    if (message.apparentTemperature != null && Object.hasOwnProperty.call(message, "apparentTemperature"))
                        writer.uint32(/* id 7, wireType 1 =*/57).double(message.apparentTemperature);
                    if (message.precipitation != null && Object.hasOwnProperty.call(message, "precipitation"))
                        writer.uint32(/* id 8, wireType 1 =*/65).double(message.precipitation);
                    if (message.rain != null && Object.hasOwnProperty.call(message, "rain"))
                        writer.uint32(/* id 9, wireType 1 =*/73).double(message.rain);
                    if (message.windSpeed_10m != null && Object.hasOwnProperty.call(message, "windSpeed_10m"))
                        writer.uint32(/* id 10, wireType 1 =*/81).double(message.windSpeed_10m);
                    if (message.windDirection_10m != null && Object.hasOwnProperty.call(message, "windDirection_10m"))
                        writer.uint32(/* id 11, wireType 1 =*/89).double(message.windDirection_10m);
                    if (message.windGusts_10m != null && Object.hasOwnProperty.call(message, "windGusts_10m"))
                        writer.uint32(/* id 12, wireType 1 =*/97).double(message.windGusts_10m);
                    if (message.weatherCode != null && Object.hasOwnProperty.call(message, "weatherCode"))
                        writer.uint32(/* id 13, wireType 0 =*/104).uint32(message.weatherCode);
                    if (message.cloudCover != null && Object.hasOwnProperty.call(message, "cloudCover"))
                        writer.uint32(/* id 14, wireType 1 =*/113).double(message.cloudCover);
                    if (message.pressureMsl != null && Object.hasOwnProperty.call(message, "pressureMsl"))
                        writer.uint32(/* id 15, wireType 1 =*/121).double(message.pressureMsl);
                    if (message.surfacePressure != null && Object.hasOwnProperty.call(message, "surfacePressure"))
                        writer.uint32(/* id 16, wireType 1 =*/129).double(message.surfacePressure);
                    if (message.isDay != null && Object.hasOwnProperty.call(message, "isDay"))
                        writer.uint32(/* id 17, wireType 0 =*/136).uint32(message.isDay);
                    return writer;
                };

                /**
                 * Encodes the specified CurrentWeather message, length delimited. Does not implicitly {@link bubbaloop.weather.v1.CurrentWeather.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof bubbaloop.weather.v1.CurrentWeather
                 * @static
                 * @param {bubbaloop.weather.v1.ICurrentWeather} message CurrentWeather message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                CurrentWeather.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a CurrentWeather message from the specified reader or buffer.
                 * @function decode
                 * @memberof bubbaloop.weather.v1.CurrentWeather
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {bubbaloop.weather.v1.CurrentWeather} CurrentWeather
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                CurrentWeather.decode = function decode(reader, length) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    let end = length === undefined ? reader.len : reader.pos + length, message = new $root.bubbaloop.weather.v1.CurrentWeather();
                    while (reader.pos < end) {
                        let tag = reader.uint32();
                        if (false)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.header = $root.bubbaloop.header.v1.Header.decode(reader, reader.uint32());
                                break;
                            }
                        case 2: {
                                message.latitude = reader.double();
                                break;
                            }
                        case 3: {
                                message.longitude = reader.double();
                                break;
                            }
                        case 4: {
                                message.timezone = reader.string();
                                break;
                            }
                        case 5: {
                                message.temperature_2m = reader.double();
                                break;
                            }
                        case 6: {
                                message.relativeHumidity_2m = reader.double();
                                break;
                            }
                        case 7: {
                                message.apparentTemperature = reader.double();
                                break;
                            }
                        case 8: {
                                message.precipitation = reader.double();
                                break;
                            }
                        case 9: {
                                message.rain = reader.double();
                                break;
                            }
                        case 10: {
                                message.windSpeed_10m = reader.double();
                                break;
                            }
                        case 11: {
                                message.windDirection_10m = reader.double();
                                break;
                            }
                        case 12: {
                                message.windGusts_10m = reader.double();
                                break;
                            }
                        case 13: {
                                message.weatherCode = reader.uint32();
                                break;
                            }
                        case 14: {
                                message.cloudCover = reader.double();
                                break;
                            }
                        case 15: {
                                message.pressureMsl = reader.double();
                                break;
                            }
                        case 16: {
                                message.surfacePressure = reader.double();
                                break;
                            }
                        case 17: {
                                message.isDay = reader.uint32();
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a CurrentWeather message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof bubbaloop.weather.v1.CurrentWeather
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {bubbaloop.weather.v1.CurrentWeather} CurrentWeather
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                CurrentWeather.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a CurrentWeather message.
                 * @function verify
                 * @memberof bubbaloop.weather.v1.CurrentWeather
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                CurrentWeather.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.header != null && message.hasOwnProperty("header")) {
                        let error = $root.bubbaloop.header.v1.Header.verify(message.header);
                        if (error)
                            return "header." + error;
                    }
                    if (message.latitude != null && message.hasOwnProperty("latitude"))
                        if (typeof message.latitude !== "number")
                            return "latitude: number expected";
                    if (message.longitude != null && message.hasOwnProperty("longitude"))
                        if (typeof message.longitude !== "number")
                            return "longitude: number expected";
                    if (message.timezone != null && message.hasOwnProperty("timezone"))
                        if (!$util.isString(message.timezone))
                            return "timezone: string expected";
                    if (message.temperature_2m != null && message.hasOwnProperty("temperature_2m"))
                        if (typeof message.temperature_2m !== "number")
                            return "temperature_2m: number expected";
                    if (message.relativeHumidity_2m != null && message.hasOwnProperty("relativeHumidity_2m"))
                        if (typeof message.relativeHumidity_2m !== "number")
                            return "relativeHumidity_2m: number expected";
                    if (message.apparentTemperature != null && message.hasOwnProperty("apparentTemperature"))
                        if (typeof message.apparentTemperature !== "number")
                            return "apparentTemperature: number expected";
                    if (message.precipitation != null && message.hasOwnProperty("precipitation"))
                        if (typeof message.precipitation !== "number")
                            return "precipitation: number expected";
                    if (message.rain != null && message.hasOwnProperty("rain"))
                        if (typeof message.rain !== "number")
                            return "rain: number expected";
                    if (message.windSpeed_10m != null && message.hasOwnProperty("windSpeed_10m"))
                        if (typeof message.windSpeed_10m !== "number")
                            return "windSpeed_10m: number expected";
                    if (message.windDirection_10m != null && message.hasOwnProperty("windDirection_10m"))
                        if (typeof message.windDirection_10m !== "number")
                            return "windDirection_10m: number expected";
                    if (message.windGusts_10m != null && message.hasOwnProperty("windGusts_10m"))
                        if (typeof message.windGusts_10m !== "number")
                            return "windGusts_10m: number expected";
                    if (message.weatherCode != null && message.hasOwnProperty("weatherCode"))
                        if (!$util.isInteger(message.weatherCode))
                            return "weatherCode: integer expected";
                    if (message.cloudCover != null && message.hasOwnProperty("cloudCover"))
                        if (typeof message.cloudCover !== "number")
                            return "cloudCover: number expected";
                    if (message.pressureMsl != null && message.hasOwnProperty("pressureMsl"))
                        if (typeof message.pressureMsl !== "number")
                            return "pressureMsl: number expected";
                    if (message.surfacePressure != null && message.hasOwnProperty("surfacePressure"))
                        if (typeof message.surfacePressure !== "number")
                            return "surfacePressure: number expected";
                    if (message.isDay != null && message.hasOwnProperty("isDay"))
                        if (!$util.isInteger(message.isDay))
                            return "isDay: integer expected";
                    return null;
                };

                /**
                 * Creates a CurrentWeather message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof bubbaloop.weather.v1.CurrentWeather
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {bubbaloop.weather.v1.CurrentWeather} CurrentWeather
                 */
                CurrentWeather.fromObject = function fromObject(object) {
                    if (object instanceof $root.bubbaloop.weather.v1.CurrentWeather)
                        return object;
                    let message = new $root.bubbaloop.weather.v1.CurrentWeather();
                    if (object.header != null) {
                        if (typeof object.header !== "object")
                            throw TypeError(".bubbaloop.weather.v1.CurrentWeather.header: object expected");
                        message.header = $root.bubbaloop.header.v1.Header.fromObject(object.header);
                    }
                    if (object.latitude != null)
                        message.latitude = Number(object.latitude);
                    if (object.longitude != null)
                        message.longitude = Number(object.longitude);
                    if (object.timezone != null)
                        message.timezone = String(object.timezone);
                    if (object.temperature_2m != null)
                        message.temperature_2m = Number(object.temperature_2m);
                    if (object.relativeHumidity_2m != null)
                        message.relativeHumidity_2m = Number(object.relativeHumidity_2m);
                    if (object.apparentTemperature != null)
                        message.apparentTemperature = Number(object.apparentTemperature);
                    if (object.precipitation != null)
                        message.precipitation = Number(object.precipitation);
                    if (object.rain != null)
                        message.rain = Number(object.rain);
                    if (object.windSpeed_10m != null)
                        message.windSpeed_10m = Number(object.windSpeed_10m);
                    if (object.windDirection_10m != null)
                        message.windDirection_10m = Number(object.windDirection_10m);
                    if (object.windGusts_10m != null)
                        message.windGusts_10m = Number(object.windGusts_10m);
                    if (object.weatherCode != null)
                        message.weatherCode = object.weatherCode >>> 0;
                    if (object.cloudCover != null)
                        message.cloudCover = Number(object.cloudCover);
                    if (object.pressureMsl != null)
                        message.pressureMsl = Number(object.pressureMsl);
                    if (object.surfacePressure != null)
                        message.surfacePressure = Number(object.surfacePressure);
                    if (object.isDay != null)
                        message.isDay = object.isDay >>> 0;
                    return message;
                };

                /**
                 * Creates a plain object from a CurrentWeather message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof bubbaloop.weather.v1.CurrentWeather
                 * @static
                 * @param {bubbaloop.weather.v1.CurrentWeather} message CurrentWeather
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                CurrentWeather.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    let object = {};
                    if (options.defaults) {
                        object.header = null;
                        object.latitude = 0;
                        object.longitude = 0;
                        object.timezone = "";
                        object.temperature_2m = 0;
                        object.relativeHumidity_2m = 0;
                        object.apparentTemperature = 0;
                        object.precipitation = 0;
                        object.rain = 0;
                        object.windSpeed_10m = 0;
                        object.windDirection_10m = 0;
                        object.windGusts_10m = 0;
                        object.weatherCode = 0;
                        object.cloudCover = 0;
                        object.pressureMsl = 0;
                        object.surfacePressure = 0;
                        object.isDay = 0;
                    }
                    if (message.header != null && message.hasOwnProperty("header"))
                        object.header = $root.bubbaloop.header.v1.Header.toObject(message.header, options);
                    if (message.latitude != null && message.hasOwnProperty("latitude"))
                        object.latitude = options.json && !isFinite(message.latitude) ? String(message.latitude) : message.latitude;
                    if (message.longitude != null && message.hasOwnProperty("longitude"))
                        object.longitude = options.json && !isFinite(message.longitude) ? String(message.longitude) : message.longitude;
                    if (message.timezone != null && message.hasOwnProperty("timezone"))
                        object.timezone = message.timezone;
                    if (message.temperature_2m != null && message.hasOwnProperty("temperature_2m"))
                        object.temperature_2m = options.json && !isFinite(message.temperature_2m) ? String(message.temperature_2m) : message.temperature_2m;
                    if (message.relativeHumidity_2m != null && message.hasOwnProperty("relativeHumidity_2m"))
                        object.relativeHumidity_2m = options.json && !isFinite(message.relativeHumidity_2m) ? String(message.relativeHumidity_2m) : message.relativeHumidity_2m;
                    if (message.apparentTemperature != null && message.hasOwnProperty("apparentTemperature"))
                        object.apparentTemperature = options.json && !isFinite(message.apparentTemperature) ? String(message.apparentTemperature) : message.apparentTemperature;
                    if (message.precipitation != null && message.hasOwnProperty("precipitation"))
                        object.precipitation = options.json && !isFinite(message.precipitation) ? String(message.precipitation) : message.precipitation;
                    if (message.rain != null && message.hasOwnProperty("rain"))
                        object.rain = options.json && !isFinite(message.rain) ? String(message.rain) : message.rain;
                    if (message.windSpeed_10m != null && message.hasOwnProperty("windSpeed_10m"))
                        object.windSpeed_10m = options.json && !isFinite(message.windSpeed_10m) ? String(message.windSpeed_10m) : message.windSpeed_10m;
                    if (message.windDirection_10m != null && message.hasOwnProperty("windDirection_10m"))
                        object.windDirection_10m = options.json && !isFinite(message.windDirection_10m) ? String(message.windDirection_10m) : message.windDirection_10m;
                    if (message.windGusts_10m != null && message.hasOwnProperty("windGusts_10m"))
                        object.windGusts_10m = options.json && !isFinite(message.windGusts_10m) ? String(message.windGusts_10m) : message.windGusts_10m;
                    if (message.weatherCode != null && message.hasOwnProperty("weatherCode"))
                        object.weatherCode = message.weatherCode;
                    if (message.cloudCover != null && message.hasOwnProperty("cloudCover"))
                        object.cloudCover = options.json && !isFinite(message.cloudCover) ? String(message.cloudCover) : message.cloudCover;
                    if (message.pressureMsl != null && message.hasOwnProperty("pressureMsl"))
                        object.pressureMsl = options.json && !isFinite(message.pressureMsl) ? String(message.pressureMsl) : message.pressureMsl;
                    if (message.surfacePressure != null && message.hasOwnProperty("surfacePressure"))
                        object.surfacePressure = options.json && !isFinite(message.surfacePressure) ? String(message.surfacePressure) : message.surfacePressure;
                    if (message.isDay != null && message.hasOwnProperty("isDay"))
                        object.isDay = message.isDay;
                    return object;
                };

                /**
                 * Converts this CurrentWeather to JSON.
                 * @function toJSON
                 * @memberof bubbaloop.weather.v1.CurrentWeather
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                CurrentWeather.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for CurrentWeather
                 * @function getTypeUrl
                 * @memberof bubbaloop.weather.v1.CurrentWeather
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                CurrentWeather.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/bubbaloop.weather.v1.CurrentWeather";
                };

                return CurrentWeather;
            })();

            v1.HourlyForecastEntry = (function() {

                /**
                 * Properties of an HourlyForecastEntry.
                 * @memberof bubbaloop.weather.v1
                 * @interface IHourlyForecastEntry
                 * @property {number|Long|null} [time] HourlyForecastEntry time
                 * @property {number|null} [temperature_2m] HourlyForecastEntry temperature_2m
                 * @property {number|null} [relativeHumidity_2m] HourlyForecastEntry relativeHumidity_2m
                 * @property {number|null} [precipitationProbability] HourlyForecastEntry precipitationProbability
                 * @property {number|null} [precipitation] HourlyForecastEntry precipitation
                 * @property {number|null} [weatherCode] HourlyForecastEntry weatherCode
                 * @property {number|null} [windSpeed_10m] HourlyForecastEntry windSpeed_10m
                 * @property {number|null} [windDirection_10m] HourlyForecastEntry windDirection_10m
                 * @property {number|null} [cloudCover] HourlyForecastEntry cloudCover
                 */

                /**
                 * Constructs a new HourlyForecastEntry.
                 * @memberof bubbaloop.weather.v1
                 * @classdesc Represents an HourlyForecastEntry.
                 * @implements IHourlyForecastEntry
                 * @constructor
                 * @param {bubbaloop.weather.v1.IHourlyForecastEntry=} [properties] Properties to set
                 */
                function HourlyForecastEntry(properties) {
                    if (properties)
                        for (let keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * HourlyForecastEntry time.
                 * @member {number|Long} time
                 * @memberof bubbaloop.weather.v1.HourlyForecastEntry
                 * @instance
                 */
                HourlyForecastEntry.prototype.time = $util.Long ? $util.Long.fromBits(0,0,true) : 0;

                /**
                 * HourlyForecastEntry temperature_2m.
                 * @member {number} temperature_2m
                 * @memberof bubbaloop.weather.v1.HourlyForecastEntry
                 * @instance
                 */
                HourlyForecastEntry.prototype.temperature_2m = 0;

                /**
                 * HourlyForecastEntry relativeHumidity_2m.
                 * @member {number} relativeHumidity_2m
                 * @memberof bubbaloop.weather.v1.HourlyForecastEntry
                 * @instance
                 */
                HourlyForecastEntry.prototype.relativeHumidity_2m = 0;

                /**
                 * HourlyForecastEntry precipitationProbability.
                 * @member {number} precipitationProbability
                 * @memberof bubbaloop.weather.v1.HourlyForecastEntry
                 * @instance
                 */
                HourlyForecastEntry.prototype.precipitationProbability = 0;

                /**
                 * HourlyForecastEntry precipitation.
                 * @member {number} precipitation
                 * @memberof bubbaloop.weather.v1.HourlyForecastEntry
                 * @instance
                 */
                HourlyForecastEntry.prototype.precipitation = 0;

                /**
                 * HourlyForecastEntry weatherCode.
                 * @member {number} weatherCode
                 * @memberof bubbaloop.weather.v1.HourlyForecastEntry
                 * @instance
                 */
                HourlyForecastEntry.prototype.weatherCode = 0;

                /**
                 * HourlyForecastEntry windSpeed_10m.
                 * @member {number} windSpeed_10m
                 * @memberof bubbaloop.weather.v1.HourlyForecastEntry
                 * @instance
                 */
                HourlyForecastEntry.prototype.windSpeed_10m = 0;

                /**
                 * HourlyForecastEntry windDirection_10m.
                 * @member {number} windDirection_10m
                 * @memberof bubbaloop.weather.v1.HourlyForecastEntry
                 * @instance
                 */
                HourlyForecastEntry.prototype.windDirection_10m = 0;

                /**
                 * HourlyForecastEntry cloudCover.
                 * @member {number} cloudCover
                 * @memberof bubbaloop.weather.v1.HourlyForecastEntry
                 * @instance
                 */
                HourlyForecastEntry.prototype.cloudCover = 0;

                /**
                 * Creates a new HourlyForecastEntry instance using the specified properties.
                 * @function create
                 * @memberof bubbaloop.weather.v1.HourlyForecastEntry
                 * @static
                 * @param {bubbaloop.weather.v1.IHourlyForecastEntry=} [properties] Properties to set
                 * @returns {bubbaloop.weather.v1.HourlyForecastEntry} HourlyForecastEntry instance
                 */
                HourlyForecastEntry.create = function create(properties) {
                    return new HourlyForecastEntry(properties);
                };

                /**
                 * Encodes the specified HourlyForecastEntry message. Does not implicitly {@link bubbaloop.weather.v1.HourlyForecastEntry.verify|verify} messages.
                 * @function encode
                 * @memberof bubbaloop.weather.v1.HourlyForecastEntry
                 * @static
                 * @param {bubbaloop.weather.v1.IHourlyForecastEntry} message HourlyForecastEntry message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                HourlyForecastEntry.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.time != null && Object.hasOwnProperty.call(message, "time"))
                        writer.uint32(/* id 1, wireType 0 =*/8).uint64(message.time);
                    if (message.temperature_2m != null && Object.hasOwnProperty.call(message, "temperature_2m"))
                        writer.uint32(/* id 2, wireType 1 =*/17).double(message.temperature_2m);
                    if (message.relativeHumidity_2m != null && Object.hasOwnProperty.call(message, "relativeHumidity_2m"))
                        writer.uint32(/* id 3, wireType 1 =*/25).double(message.relativeHumidity_2m);
                    if (message.precipitationProbability != null && Object.hasOwnProperty.call(message, "precipitationProbability"))
                        writer.uint32(/* id 4, wireType 1 =*/33).double(message.precipitationProbability);
                    if (message.precipitation != null && Object.hasOwnProperty.call(message, "precipitation"))
                        writer.uint32(/* id 5, wireType 1 =*/41).double(message.precipitation);
                    if (message.weatherCode != null && Object.hasOwnProperty.call(message, "weatherCode"))
                        writer.uint32(/* id 6, wireType 0 =*/48).uint32(message.weatherCode);
                    if (message.windSpeed_10m != null && Object.hasOwnProperty.call(message, "windSpeed_10m"))
                        writer.uint32(/* id 7, wireType 1 =*/57).double(message.windSpeed_10m);
                    if (message.windDirection_10m != null && Object.hasOwnProperty.call(message, "windDirection_10m"))
                        writer.uint32(/* id 8, wireType 1 =*/65).double(message.windDirection_10m);
                    if (message.cloudCover != null && Object.hasOwnProperty.call(message, "cloudCover"))
                        writer.uint32(/* id 9, wireType 1 =*/73).double(message.cloudCover);
                    return writer;
                };

                /**
                 * Encodes the specified HourlyForecastEntry message, length delimited. Does not implicitly {@link bubbaloop.weather.v1.HourlyForecastEntry.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof bubbaloop.weather.v1.HourlyForecastEntry
                 * @static
                 * @param {bubbaloop.weather.v1.IHourlyForecastEntry} message HourlyForecastEntry message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                HourlyForecastEntry.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes an HourlyForecastEntry message from the specified reader or buffer.
                 * @function decode
                 * @memberof bubbaloop.weather.v1.HourlyForecastEntry
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {bubbaloop.weather.v1.HourlyForecastEntry} HourlyForecastEntry
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                HourlyForecastEntry.decode = function decode(reader, length) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    let end = length === undefined ? reader.len : reader.pos + length, message = new $root.bubbaloop.weather.v1.HourlyForecastEntry();
                    while (reader.pos < end) {
                        let tag = reader.uint32();
                        if (false)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.time = reader.uint64();
                                break;
                            }
                        case 2: {
                                message.temperature_2m = reader.double();
                                break;
                            }
                        case 3: {
                                message.relativeHumidity_2m = reader.double();
                                break;
                            }
                        case 4: {
                                message.precipitationProbability = reader.double();
                                break;
                            }
                        case 5: {
                                message.precipitation = reader.double();
                                break;
                            }
                        case 6: {
                                message.weatherCode = reader.uint32();
                                break;
                            }
                        case 7: {
                                message.windSpeed_10m = reader.double();
                                break;
                            }
                        case 8: {
                                message.windDirection_10m = reader.double();
                                break;
                            }
                        case 9: {
                                message.cloudCover = reader.double();
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes an HourlyForecastEntry message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof bubbaloop.weather.v1.HourlyForecastEntry
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {bubbaloop.weather.v1.HourlyForecastEntry} HourlyForecastEntry
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                HourlyForecastEntry.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies an HourlyForecastEntry message.
                 * @function verify
                 * @memberof bubbaloop.weather.v1.HourlyForecastEntry
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                HourlyForecastEntry.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.time != null && message.hasOwnProperty("time"))
                        if (!$util.isInteger(message.time) && !(message.time && $util.isInteger(message.time.low) && $util.isInteger(message.time.high)))
                            return "time: integer|Long expected";
                    if (message.temperature_2m != null && message.hasOwnProperty("temperature_2m"))
                        if (typeof message.temperature_2m !== "number")
                            return "temperature_2m: number expected";
                    if (message.relativeHumidity_2m != null && message.hasOwnProperty("relativeHumidity_2m"))
                        if (typeof message.relativeHumidity_2m !== "number")
                            return "relativeHumidity_2m: number expected";
                    if (message.precipitationProbability != null && message.hasOwnProperty("precipitationProbability"))
                        if (typeof message.precipitationProbability !== "number")
                            return "precipitationProbability: number expected";
                    if (message.precipitation != null && message.hasOwnProperty("precipitation"))
                        if (typeof message.precipitation !== "number")
                            return "precipitation: number expected";
                    if (message.weatherCode != null && message.hasOwnProperty("weatherCode"))
                        if (!$util.isInteger(message.weatherCode))
                            return "weatherCode: integer expected";
                    if (message.windSpeed_10m != null && message.hasOwnProperty("windSpeed_10m"))
                        if (typeof message.windSpeed_10m !== "number")
                            return "windSpeed_10m: number expected";
                    if (message.windDirection_10m != null && message.hasOwnProperty("windDirection_10m"))
                        if (typeof message.windDirection_10m !== "number")
                            return "windDirection_10m: number expected";
                    if (message.cloudCover != null && message.hasOwnProperty("cloudCover"))
                        if (typeof message.cloudCover !== "number")
                            return "cloudCover: number expected";
                    return null;
                };

                /**
                 * Creates an HourlyForecastEntry message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof bubbaloop.weather.v1.HourlyForecastEntry
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {bubbaloop.weather.v1.HourlyForecastEntry} HourlyForecastEntry
                 */
                HourlyForecastEntry.fromObject = function fromObject(object) {
                    if (object instanceof $root.bubbaloop.weather.v1.HourlyForecastEntry)
                        return object;
                    let message = new $root.bubbaloop.weather.v1.HourlyForecastEntry();
                    if (object.time != null)
                        if ($util.Long)
                            (message.time = $util.Long.fromValue(object.time)).unsigned = true;
                        else if (typeof object.time === "string")
                            message.time = parseInt(object.time, 10);
                        else if (typeof object.time === "number")
                            message.time = object.time;
                        else if (typeof object.time === "object")
                            message.time = new $util.LongBits(object.time.low >>> 0, object.time.high >>> 0).toNumber(true);
                    if (object.temperature_2m != null)
                        message.temperature_2m = Number(object.temperature_2m);
                    if (object.relativeHumidity_2m != null)
                        message.relativeHumidity_2m = Number(object.relativeHumidity_2m);
                    if (object.precipitationProbability != null)
                        message.precipitationProbability = Number(object.precipitationProbability);
                    if (object.precipitation != null)
                        message.precipitation = Number(object.precipitation);
                    if (object.weatherCode != null)
                        message.weatherCode = object.weatherCode >>> 0;
                    if (object.windSpeed_10m != null)
                        message.windSpeed_10m = Number(object.windSpeed_10m);
                    if (object.windDirection_10m != null)
                        message.windDirection_10m = Number(object.windDirection_10m);
                    if (object.cloudCover != null)
                        message.cloudCover = Number(object.cloudCover);
                    return message;
                };

                /**
                 * Creates a plain object from an HourlyForecastEntry message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof bubbaloop.weather.v1.HourlyForecastEntry
                 * @static
                 * @param {bubbaloop.weather.v1.HourlyForecastEntry} message HourlyForecastEntry
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                HourlyForecastEntry.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    let object = {};
                    if (options.defaults) {
                        if ($util.Long) {
                            let long = new $util.Long(0, 0, true);
                            object.time = options.longs === String ? long.toString() : options.longs === Number ? long.toNumber() : long;
                        } else
                            object.time = options.longs === String ? "0" : 0;
                        object.temperature_2m = 0;
                        object.relativeHumidity_2m = 0;
                        object.precipitationProbability = 0;
                        object.precipitation = 0;
                        object.weatherCode = 0;
                        object.windSpeed_10m = 0;
                        object.windDirection_10m = 0;
                        object.cloudCover = 0;
                    }
                    if (message.time != null && message.hasOwnProperty("time"))
                        if (typeof message.time === "number")
                            object.time = options.longs === String ? String(message.time) : message.time;
                        else
                            object.time = options.longs === String ? $util.Long.prototype.toString.call(message.time) : options.longs === Number ? new $util.LongBits(message.time.low >>> 0, message.time.high >>> 0).toNumber(true) : message.time;
                    if (message.temperature_2m != null && message.hasOwnProperty("temperature_2m"))
                        object.temperature_2m = options.json && !isFinite(message.temperature_2m) ? String(message.temperature_2m) : message.temperature_2m;
                    if (message.relativeHumidity_2m != null && message.hasOwnProperty("relativeHumidity_2m"))
                        object.relativeHumidity_2m = options.json && !isFinite(message.relativeHumidity_2m) ? String(message.relativeHumidity_2m) : message.relativeHumidity_2m;
                    if (message.precipitationProbability != null && message.hasOwnProperty("precipitationProbability"))
                        object.precipitationProbability = options.json && !isFinite(message.precipitationProbability) ? String(message.precipitationProbability) : message.precipitationProbability;
                    if (message.precipitation != null && message.hasOwnProperty("precipitation"))
                        object.precipitation = options.json && !isFinite(message.precipitation) ? String(message.precipitation) : message.precipitation;
                    if (message.weatherCode != null && message.hasOwnProperty("weatherCode"))
                        object.weatherCode = message.weatherCode;
                    if (message.windSpeed_10m != null && message.hasOwnProperty("windSpeed_10m"))
                        object.windSpeed_10m = options.json && !isFinite(message.windSpeed_10m) ? String(message.windSpeed_10m) : message.windSpeed_10m;
                    if (message.windDirection_10m != null && message.hasOwnProperty("windDirection_10m"))
                        object.windDirection_10m = options.json && !isFinite(message.windDirection_10m) ? String(message.windDirection_10m) : message.windDirection_10m;
                    if (message.cloudCover != null && message.hasOwnProperty("cloudCover"))
                        object.cloudCover = options.json && !isFinite(message.cloudCover) ? String(message.cloudCover) : message.cloudCover;
                    return object;
                };

                /**
                 * Converts this HourlyForecastEntry to JSON.
                 * @function toJSON
                 * @memberof bubbaloop.weather.v1.HourlyForecastEntry
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                HourlyForecastEntry.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for HourlyForecastEntry
                 * @function getTypeUrl
                 * @memberof bubbaloop.weather.v1.HourlyForecastEntry
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                HourlyForecastEntry.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/bubbaloop.weather.v1.HourlyForecastEntry";
                };

                return HourlyForecastEntry;
            })();

            v1.HourlyForecast = (function() {

                /**
                 * Properties of an HourlyForecast.
                 * @memberof bubbaloop.weather.v1
                 * @interface IHourlyForecast
                 * @property {bubbaloop.header.v1.IHeader|null} [header] HourlyForecast header
                 * @property {number|null} [latitude] HourlyForecast latitude
                 * @property {number|null} [longitude] HourlyForecast longitude
                 * @property {string|null} [timezone] HourlyForecast timezone
                 * @property {Array.<bubbaloop.weather.v1.IHourlyForecastEntry>|null} [entries] HourlyForecast entries
                 */

                /**
                 * Constructs a new HourlyForecast.
                 * @memberof bubbaloop.weather.v1
                 * @classdesc Represents an HourlyForecast.
                 * @implements IHourlyForecast
                 * @constructor
                 * @param {bubbaloop.weather.v1.IHourlyForecast=} [properties] Properties to set
                 */
                function HourlyForecast(properties) {
                    this.entries = [];
                    if (properties)
                        for (let keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * HourlyForecast header.
                 * @member {bubbaloop.header.v1.IHeader|null|undefined} header
                 * @memberof bubbaloop.weather.v1.HourlyForecast
                 * @instance
                 */
                HourlyForecast.prototype.header = null;

                /**
                 * HourlyForecast latitude.
                 * @member {number} latitude
                 * @memberof bubbaloop.weather.v1.HourlyForecast
                 * @instance
                 */
                HourlyForecast.prototype.latitude = 0;

                /**
                 * HourlyForecast longitude.
                 * @member {number} longitude
                 * @memberof bubbaloop.weather.v1.HourlyForecast
                 * @instance
                 */
                HourlyForecast.prototype.longitude = 0;

                /**
                 * HourlyForecast timezone.
                 * @member {string} timezone
                 * @memberof bubbaloop.weather.v1.HourlyForecast
                 * @instance
                 */
                HourlyForecast.prototype.timezone = "";

                /**
                 * HourlyForecast entries.
                 * @member {Array.<bubbaloop.weather.v1.IHourlyForecastEntry>} entries
                 * @memberof bubbaloop.weather.v1.HourlyForecast
                 * @instance
                 */
                HourlyForecast.prototype.entries = $util.emptyArray;

                /**
                 * Creates a new HourlyForecast instance using the specified properties.
                 * @function create
                 * @memberof bubbaloop.weather.v1.HourlyForecast
                 * @static
                 * @param {bubbaloop.weather.v1.IHourlyForecast=} [properties] Properties to set
                 * @returns {bubbaloop.weather.v1.HourlyForecast} HourlyForecast instance
                 */
                HourlyForecast.create = function create(properties) {
                    return new HourlyForecast(properties);
                };

                /**
                 * Encodes the specified HourlyForecast message. Does not implicitly {@link bubbaloop.weather.v1.HourlyForecast.verify|verify} messages.
                 * @function encode
                 * @memberof bubbaloop.weather.v1.HourlyForecast
                 * @static
                 * @param {bubbaloop.weather.v1.IHourlyForecast} message HourlyForecast message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                HourlyForecast.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.header != null && Object.hasOwnProperty.call(message, "header"))
                        $root.bubbaloop.header.v1.Header.encode(message.header, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    if (message.latitude != null && Object.hasOwnProperty.call(message, "latitude"))
                        writer.uint32(/* id 2, wireType 1 =*/17).double(message.latitude);
                    if (message.longitude != null && Object.hasOwnProperty.call(message, "longitude"))
                        writer.uint32(/* id 3, wireType 1 =*/25).double(message.longitude);
                    if (message.timezone != null && Object.hasOwnProperty.call(message, "timezone"))
                        writer.uint32(/* id 4, wireType 2 =*/34).string(message.timezone);
                    if (message.entries != null && message.entries.length)
                        for (let i = 0; i < message.entries.length; ++i)
                            $root.bubbaloop.weather.v1.HourlyForecastEntry.encode(message.entries[i], writer.uint32(/* id 5, wireType 2 =*/42).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified HourlyForecast message, length delimited. Does not implicitly {@link bubbaloop.weather.v1.HourlyForecast.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof bubbaloop.weather.v1.HourlyForecast
                 * @static
                 * @param {bubbaloop.weather.v1.IHourlyForecast} message HourlyForecast message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                HourlyForecast.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes an HourlyForecast message from the specified reader or buffer.
                 * @function decode
                 * @memberof bubbaloop.weather.v1.HourlyForecast
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {bubbaloop.weather.v1.HourlyForecast} HourlyForecast
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                HourlyForecast.decode = function decode(reader, length) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    let end = length === undefined ? reader.len : reader.pos + length, message = new $root.bubbaloop.weather.v1.HourlyForecast();
                    while (reader.pos < end) {
                        let tag = reader.uint32();
                        if (false)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.header = $root.bubbaloop.header.v1.Header.decode(reader, reader.uint32());
                                break;
                            }
                        case 2: {
                                message.latitude = reader.double();
                                break;
                            }
                        case 3: {
                                message.longitude = reader.double();
                                break;
                            }
                        case 4: {
                                message.timezone = reader.string();
                                break;
                            }
                        case 5: {
                                if (!(message.entries && message.entries.length))
                                    message.entries = [];
                                message.entries.push($root.bubbaloop.weather.v1.HourlyForecastEntry.decode(reader, reader.uint32()));
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes an HourlyForecast message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof bubbaloop.weather.v1.HourlyForecast
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {bubbaloop.weather.v1.HourlyForecast} HourlyForecast
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                HourlyForecast.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies an HourlyForecast message.
                 * @function verify
                 * @memberof bubbaloop.weather.v1.HourlyForecast
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                HourlyForecast.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.header != null && message.hasOwnProperty("header")) {
                        let error = $root.bubbaloop.header.v1.Header.verify(message.header);
                        if (error)
                            return "header." + error;
                    }
                    if (message.latitude != null && message.hasOwnProperty("latitude"))
                        if (typeof message.latitude !== "number")
                            return "latitude: number expected";
                    if (message.longitude != null && message.hasOwnProperty("longitude"))
                        if (typeof message.longitude !== "number")
                            return "longitude: number expected";
                    if (message.timezone != null && message.hasOwnProperty("timezone"))
                        if (!$util.isString(message.timezone))
                            return "timezone: string expected";
                    if (message.entries != null && message.hasOwnProperty("entries")) {
                        if (!Array.isArray(message.entries))
                            return "entries: array expected";
                        for (let i = 0; i < message.entries.length; ++i) {
                            let error = $root.bubbaloop.weather.v1.HourlyForecastEntry.verify(message.entries[i]);
                            if (error)
                                return "entries." + error;
                        }
                    }
                    return null;
                };

                /**
                 * Creates an HourlyForecast message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof bubbaloop.weather.v1.HourlyForecast
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {bubbaloop.weather.v1.HourlyForecast} HourlyForecast
                 */
                HourlyForecast.fromObject = function fromObject(object) {
                    if (object instanceof $root.bubbaloop.weather.v1.HourlyForecast)
                        return object;
                    let message = new $root.bubbaloop.weather.v1.HourlyForecast();
                    if (object.header != null) {
                        if (typeof object.header !== "object")
                            throw TypeError(".bubbaloop.weather.v1.HourlyForecast.header: object expected");
                        message.header = $root.bubbaloop.header.v1.Header.fromObject(object.header);
                    }
                    if (object.latitude != null)
                        message.latitude = Number(object.latitude);
                    if (object.longitude != null)
                        message.longitude = Number(object.longitude);
                    if (object.timezone != null)
                        message.timezone = String(object.timezone);
                    if (object.entries) {
                        if (!Array.isArray(object.entries))
                            throw TypeError(".bubbaloop.weather.v1.HourlyForecast.entries: array expected");
                        message.entries = [];
                        for (let i = 0; i < object.entries.length; ++i) {
                            if (typeof object.entries[i] !== "object")
                                throw TypeError(".bubbaloop.weather.v1.HourlyForecast.entries: object expected");
                            message.entries[i] = $root.bubbaloop.weather.v1.HourlyForecastEntry.fromObject(object.entries[i]);
                        }
                    }
                    return message;
                };

                /**
                 * Creates a plain object from an HourlyForecast message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof bubbaloop.weather.v1.HourlyForecast
                 * @static
                 * @param {bubbaloop.weather.v1.HourlyForecast} message HourlyForecast
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                HourlyForecast.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    let object = {};
                    if (options.arrays || options.defaults)
                        object.entries = [];
                    if (options.defaults) {
                        object.header = null;
                        object.latitude = 0;
                        object.longitude = 0;
                        object.timezone = "";
                    }
                    if (message.header != null && message.hasOwnProperty("header"))
                        object.header = $root.bubbaloop.header.v1.Header.toObject(message.header, options);
                    if (message.latitude != null && message.hasOwnProperty("latitude"))
                        object.latitude = options.json && !isFinite(message.latitude) ? String(message.latitude) : message.latitude;
                    if (message.longitude != null && message.hasOwnProperty("longitude"))
                        object.longitude = options.json && !isFinite(message.longitude) ? String(message.longitude) : message.longitude;
                    if (message.timezone != null && message.hasOwnProperty("timezone"))
                        object.timezone = message.timezone;
                    if (message.entries && message.entries.length) {
                        object.entries = [];
                        for (let j = 0; j < message.entries.length; ++j)
                            object.entries[j] = $root.bubbaloop.weather.v1.HourlyForecastEntry.toObject(message.entries[j], options);
                    }
                    return object;
                };

                /**
                 * Converts this HourlyForecast to JSON.
                 * @function toJSON
                 * @memberof bubbaloop.weather.v1.HourlyForecast
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                HourlyForecast.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for HourlyForecast
                 * @function getTypeUrl
                 * @memberof bubbaloop.weather.v1.HourlyForecast
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                HourlyForecast.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/bubbaloop.weather.v1.HourlyForecast";
                };

                return HourlyForecast;
            })();

            v1.DailyForecastEntry = (function() {

                /**
                 * Properties of a DailyForecastEntry.
                 * @memberof bubbaloop.weather.v1
                 * @interface IDailyForecastEntry
                 * @property {number|Long|null} [time] DailyForecastEntry time
                 * @property {number|null} [temperature_2mMax] DailyForecastEntry temperature_2mMax
                 * @property {number|null} [temperature_2mMin] DailyForecastEntry temperature_2mMin
                 * @property {number|null} [precipitationSum] DailyForecastEntry precipitationSum
                 * @property {number|null} [precipitationProbabilityMax] DailyForecastEntry precipitationProbabilityMax
                 * @property {number|null} [weatherCode] DailyForecastEntry weatherCode
                 * @property {number|null} [windSpeed_10mMax] DailyForecastEntry windSpeed_10mMax
                 * @property {number|null} [windGusts_10mMax] DailyForecastEntry windGusts_10mMax
                 * @property {string|null} [sunrise] DailyForecastEntry sunrise
                 * @property {string|null} [sunset] DailyForecastEntry sunset
                 */

                /**
                 * Constructs a new DailyForecastEntry.
                 * @memberof bubbaloop.weather.v1
                 * @classdesc Represents a DailyForecastEntry.
                 * @implements IDailyForecastEntry
                 * @constructor
                 * @param {bubbaloop.weather.v1.IDailyForecastEntry=} [properties] Properties to set
                 */
                function DailyForecastEntry(properties) {
                    if (properties)
                        for (let keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * DailyForecastEntry time.
                 * @member {number|Long} time
                 * @memberof bubbaloop.weather.v1.DailyForecastEntry
                 * @instance
                 */
                DailyForecastEntry.prototype.time = $util.Long ? $util.Long.fromBits(0,0,true) : 0;

                /**
                 * DailyForecastEntry temperature_2mMax.
                 * @member {number} temperature_2mMax
                 * @memberof bubbaloop.weather.v1.DailyForecastEntry
                 * @instance
                 */
                DailyForecastEntry.prototype.temperature_2mMax = 0;

                /**
                 * DailyForecastEntry temperature_2mMin.
                 * @member {number} temperature_2mMin
                 * @memberof bubbaloop.weather.v1.DailyForecastEntry
                 * @instance
                 */
                DailyForecastEntry.prototype.temperature_2mMin = 0;

                /**
                 * DailyForecastEntry precipitationSum.
                 * @member {number} precipitationSum
                 * @memberof bubbaloop.weather.v1.DailyForecastEntry
                 * @instance
                 */
                DailyForecastEntry.prototype.precipitationSum = 0;

                /**
                 * DailyForecastEntry precipitationProbabilityMax.
                 * @member {number} precipitationProbabilityMax
                 * @memberof bubbaloop.weather.v1.DailyForecastEntry
                 * @instance
                 */
                DailyForecastEntry.prototype.precipitationProbabilityMax = 0;

                /**
                 * DailyForecastEntry weatherCode.
                 * @member {number} weatherCode
                 * @memberof bubbaloop.weather.v1.DailyForecastEntry
                 * @instance
                 */
                DailyForecastEntry.prototype.weatherCode = 0;

                /**
                 * DailyForecastEntry windSpeed_10mMax.
                 * @member {number} windSpeed_10mMax
                 * @memberof bubbaloop.weather.v1.DailyForecastEntry
                 * @instance
                 */
                DailyForecastEntry.prototype.windSpeed_10mMax = 0;

                /**
                 * DailyForecastEntry windGusts_10mMax.
                 * @member {number} windGusts_10mMax
                 * @memberof bubbaloop.weather.v1.DailyForecastEntry
                 * @instance
                 */
                DailyForecastEntry.prototype.windGusts_10mMax = 0;

                /**
                 * DailyForecastEntry sunrise.
                 * @member {string} sunrise
                 * @memberof bubbaloop.weather.v1.DailyForecastEntry
                 * @instance
                 */
                DailyForecastEntry.prototype.sunrise = "";

                /**
                 * DailyForecastEntry sunset.
                 * @member {string} sunset
                 * @memberof bubbaloop.weather.v1.DailyForecastEntry
                 * @instance
                 */
                DailyForecastEntry.prototype.sunset = "";

                /**
                 * Creates a new DailyForecastEntry instance using the specified properties.
                 * @function create
                 * @memberof bubbaloop.weather.v1.DailyForecastEntry
                 * @static
                 * @param {bubbaloop.weather.v1.IDailyForecastEntry=} [properties] Properties to set
                 * @returns {bubbaloop.weather.v1.DailyForecastEntry} DailyForecastEntry instance
                 */
                DailyForecastEntry.create = function create(properties) {
                    return new DailyForecastEntry(properties);
                };

                /**
                 * Encodes the specified DailyForecastEntry message. Does not implicitly {@link bubbaloop.weather.v1.DailyForecastEntry.verify|verify} messages.
                 * @function encode
                 * @memberof bubbaloop.weather.v1.DailyForecastEntry
                 * @static
                 * @param {bubbaloop.weather.v1.IDailyForecastEntry} message DailyForecastEntry message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                DailyForecastEntry.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.time != null && Object.hasOwnProperty.call(message, "time"))
                        writer.uint32(/* id 1, wireType 0 =*/8).uint64(message.time);
                    if (message.temperature_2mMax != null && Object.hasOwnProperty.call(message, "temperature_2mMax"))
                        writer.uint32(/* id 2, wireType 1 =*/17).double(message.temperature_2mMax);
                    if (message.temperature_2mMin != null && Object.hasOwnProperty.call(message, "temperature_2mMin"))
                        writer.uint32(/* id 3, wireType 1 =*/25).double(message.temperature_2mMin);
                    if (message.precipitationSum != null && Object.hasOwnProperty.call(message, "precipitationSum"))
                        writer.uint32(/* id 4, wireType 1 =*/33).double(message.precipitationSum);
                    if (message.precipitationProbabilityMax != null && Object.hasOwnProperty.call(message, "precipitationProbabilityMax"))
                        writer.uint32(/* id 5, wireType 1 =*/41).double(message.precipitationProbabilityMax);
                    if (message.weatherCode != null && Object.hasOwnProperty.call(message, "weatherCode"))
                        writer.uint32(/* id 6, wireType 0 =*/48).uint32(message.weatherCode);
                    if (message.windSpeed_10mMax != null && Object.hasOwnProperty.call(message, "windSpeed_10mMax"))
                        writer.uint32(/* id 7, wireType 1 =*/57).double(message.windSpeed_10mMax);
                    if (message.windGusts_10mMax != null && Object.hasOwnProperty.call(message, "windGusts_10mMax"))
                        writer.uint32(/* id 8, wireType 1 =*/65).double(message.windGusts_10mMax);
                    if (message.sunrise != null && Object.hasOwnProperty.call(message, "sunrise"))
                        writer.uint32(/* id 9, wireType 2 =*/74).string(message.sunrise);
                    if (message.sunset != null && Object.hasOwnProperty.call(message, "sunset"))
                        writer.uint32(/* id 10, wireType 2 =*/82).string(message.sunset);
                    return writer;
                };

                /**
                 * Encodes the specified DailyForecastEntry message, length delimited. Does not implicitly {@link bubbaloop.weather.v1.DailyForecastEntry.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof bubbaloop.weather.v1.DailyForecastEntry
                 * @static
                 * @param {bubbaloop.weather.v1.IDailyForecastEntry} message DailyForecastEntry message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                DailyForecastEntry.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a DailyForecastEntry message from the specified reader or buffer.
                 * @function decode
                 * @memberof bubbaloop.weather.v1.DailyForecastEntry
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {bubbaloop.weather.v1.DailyForecastEntry} DailyForecastEntry
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                DailyForecastEntry.decode = function decode(reader, length) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    let end = length === undefined ? reader.len : reader.pos + length, message = new $root.bubbaloop.weather.v1.DailyForecastEntry();
                    while (reader.pos < end) {
                        let tag = reader.uint32();
                        if (false)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.time = reader.uint64();
                                break;
                            }
                        case 2: {
                                message.temperature_2mMax = reader.double();
                                break;
                            }
                        case 3: {
                                message.temperature_2mMin = reader.double();
                                break;
                            }
                        case 4: {
                                message.precipitationSum = reader.double();
                                break;
                            }
                        case 5: {
                                message.precipitationProbabilityMax = reader.double();
                                break;
                            }
                        case 6: {
                                message.weatherCode = reader.uint32();
                                break;
                            }
                        case 7: {
                                message.windSpeed_10mMax = reader.double();
                                break;
                            }
                        case 8: {
                                message.windGusts_10mMax = reader.double();
                                break;
                            }
                        case 9: {
                                message.sunrise = reader.string();
                                break;
                            }
                        case 10: {
                                message.sunset = reader.string();
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a DailyForecastEntry message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof bubbaloop.weather.v1.DailyForecastEntry
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {bubbaloop.weather.v1.DailyForecastEntry} DailyForecastEntry
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                DailyForecastEntry.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a DailyForecastEntry message.
                 * @function verify
                 * @memberof bubbaloop.weather.v1.DailyForecastEntry
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                DailyForecastEntry.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.time != null && message.hasOwnProperty("time"))
                        if (!$util.isInteger(message.time) && !(message.time && $util.isInteger(message.time.low) && $util.isInteger(message.time.high)))
                            return "time: integer|Long expected";
                    if (message.temperature_2mMax != null && message.hasOwnProperty("temperature_2mMax"))
                        if (typeof message.temperature_2mMax !== "number")
                            return "temperature_2mMax: number expected";
                    if (message.temperature_2mMin != null && message.hasOwnProperty("temperature_2mMin"))
                        if (typeof message.temperature_2mMin !== "number")
                            return "temperature_2mMin: number expected";
                    if (message.precipitationSum != null && message.hasOwnProperty("precipitationSum"))
                        if (typeof message.precipitationSum !== "number")
                            return "precipitationSum: number expected";
                    if (message.precipitationProbabilityMax != null && message.hasOwnProperty("precipitationProbabilityMax"))
                        if (typeof message.precipitationProbabilityMax !== "number")
                            return "precipitationProbabilityMax: number expected";
                    if (message.weatherCode != null && message.hasOwnProperty("weatherCode"))
                        if (!$util.isInteger(message.weatherCode))
                            return "weatherCode: integer expected";
                    if (message.windSpeed_10mMax != null && message.hasOwnProperty("windSpeed_10mMax"))
                        if (typeof message.windSpeed_10mMax !== "number")
                            return "windSpeed_10mMax: number expected";
                    if (message.windGusts_10mMax != null && message.hasOwnProperty("windGusts_10mMax"))
                        if (typeof message.windGusts_10mMax !== "number")
                            return "windGusts_10mMax: number expected";
                    if (message.sunrise != null && message.hasOwnProperty("sunrise"))
                        if (!$util.isString(message.sunrise))
                            return "sunrise: string expected";
                    if (message.sunset != null && message.hasOwnProperty("sunset"))
                        if (!$util.isString(message.sunset))
                            return "sunset: string expected";
                    return null;
                };

                /**
                 * Creates a DailyForecastEntry message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof bubbaloop.weather.v1.DailyForecastEntry
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {bubbaloop.weather.v1.DailyForecastEntry} DailyForecastEntry
                 */
                DailyForecastEntry.fromObject = function fromObject(object) {
                    if (object instanceof $root.bubbaloop.weather.v1.DailyForecastEntry)
                        return object;
                    let message = new $root.bubbaloop.weather.v1.DailyForecastEntry();
                    if (object.time != null)
                        if ($util.Long)
                            (message.time = $util.Long.fromValue(object.time)).unsigned = true;
                        else if (typeof object.time === "string")
                            message.time = parseInt(object.time, 10);
                        else if (typeof object.time === "number")
                            message.time = object.time;
                        else if (typeof object.time === "object")
                            message.time = new $util.LongBits(object.time.low >>> 0, object.time.high >>> 0).toNumber(true);
                    if (object.temperature_2mMax != null)
                        message.temperature_2mMax = Number(object.temperature_2mMax);
                    if (object.temperature_2mMin != null)
                        message.temperature_2mMin = Number(object.temperature_2mMin);
                    if (object.precipitationSum != null)
                        message.precipitationSum = Number(object.precipitationSum);
                    if (object.precipitationProbabilityMax != null)
                        message.precipitationProbabilityMax = Number(object.precipitationProbabilityMax);
                    if (object.weatherCode != null)
                        message.weatherCode = object.weatherCode >>> 0;
                    if (object.windSpeed_10mMax != null)
                        message.windSpeed_10mMax = Number(object.windSpeed_10mMax);
                    if (object.windGusts_10mMax != null)
                        message.windGusts_10mMax = Number(object.windGusts_10mMax);
                    if (object.sunrise != null)
                        message.sunrise = String(object.sunrise);
                    if (object.sunset != null)
                        message.sunset = String(object.sunset);
                    return message;
                };

                /**
                 * Creates a plain object from a DailyForecastEntry message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof bubbaloop.weather.v1.DailyForecastEntry
                 * @static
                 * @param {bubbaloop.weather.v1.DailyForecastEntry} message DailyForecastEntry
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                DailyForecastEntry.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    let object = {};
                    if (options.defaults) {
                        if ($util.Long) {
                            let long = new $util.Long(0, 0, true);
                            object.time = options.longs === String ? long.toString() : options.longs === Number ? long.toNumber() : long;
                        } else
                            object.time = options.longs === String ? "0" : 0;
                        object.temperature_2mMax = 0;
                        object.temperature_2mMin = 0;
                        object.precipitationSum = 0;
                        object.precipitationProbabilityMax = 0;
                        object.weatherCode = 0;
                        object.windSpeed_10mMax = 0;
                        object.windGusts_10mMax = 0;
                        object.sunrise = "";
                        object.sunset = "";
                    }
                    if (message.time != null && message.hasOwnProperty("time"))
                        if (typeof message.time === "number")
                            object.time = options.longs === String ? String(message.time) : message.time;
                        else
                            object.time = options.longs === String ? $util.Long.prototype.toString.call(message.time) : options.longs === Number ? new $util.LongBits(message.time.low >>> 0, message.time.high >>> 0).toNumber(true) : message.time;
                    if (message.temperature_2mMax != null && message.hasOwnProperty("temperature_2mMax"))
                        object.temperature_2mMax = options.json && !isFinite(message.temperature_2mMax) ? String(message.temperature_2mMax) : message.temperature_2mMax;
                    if (message.temperature_2mMin != null && message.hasOwnProperty("temperature_2mMin"))
                        object.temperature_2mMin = options.json && !isFinite(message.temperature_2mMin) ? String(message.temperature_2mMin) : message.temperature_2mMin;
                    if (message.precipitationSum != null && message.hasOwnProperty("precipitationSum"))
                        object.precipitationSum = options.json && !isFinite(message.precipitationSum) ? String(message.precipitationSum) : message.precipitationSum;
                    if (message.precipitationProbabilityMax != null && message.hasOwnProperty("precipitationProbabilityMax"))
                        object.precipitationProbabilityMax = options.json && !isFinite(message.precipitationProbabilityMax) ? String(message.precipitationProbabilityMax) : message.precipitationProbabilityMax;
                    if (message.weatherCode != null && message.hasOwnProperty("weatherCode"))
                        object.weatherCode = message.weatherCode;
                    if (message.windSpeed_10mMax != null && message.hasOwnProperty("windSpeed_10mMax"))
                        object.windSpeed_10mMax = options.json && !isFinite(message.windSpeed_10mMax) ? String(message.windSpeed_10mMax) : message.windSpeed_10mMax;
                    if (message.windGusts_10mMax != null && message.hasOwnProperty("windGusts_10mMax"))
                        object.windGusts_10mMax = options.json && !isFinite(message.windGusts_10mMax) ? String(message.windGusts_10mMax) : message.windGusts_10mMax;
                    if (message.sunrise != null && message.hasOwnProperty("sunrise"))
                        object.sunrise = message.sunrise;
                    if (message.sunset != null && message.hasOwnProperty("sunset"))
                        object.sunset = message.sunset;
                    return object;
                };

                /**
                 * Converts this DailyForecastEntry to JSON.
                 * @function toJSON
                 * @memberof bubbaloop.weather.v1.DailyForecastEntry
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                DailyForecastEntry.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for DailyForecastEntry
                 * @function getTypeUrl
                 * @memberof bubbaloop.weather.v1.DailyForecastEntry
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                DailyForecastEntry.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/bubbaloop.weather.v1.DailyForecastEntry";
                };

                return DailyForecastEntry;
            })();

            v1.DailyForecast = (function() {

                /**
                 * Properties of a DailyForecast.
                 * @memberof bubbaloop.weather.v1
                 * @interface IDailyForecast
                 * @property {bubbaloop.header.v1.IHeader|null} [header] DailyForecast header
                 * @property {number|null} [latitude] DailyForecast latitude
                 * @property {number|null} [longitude] DailyForecast longitude
                 * @property {string|null} [timezone] DailyForecast timezone
                 * @property {Array.<bubbaloop.weather.v1.IDailyForecastEntry>|null} [entries] DailyForecast entries
                 */

                /**
                 * Constructs a new DailyForecast.
                 * @memberof bubbaloop.weather.v1
                 * @classdesc Represents a DailyForecast.
                 * @implements IDailyForecast
                 * @constructor
                 * @param {bubbaloop.weather.v1.IDailyForecast=} [properties] Properties to set
                 */
                function DailyForecast(properties) {
                    this.entries = [];
                    if (properties)
                        for (let keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * DailyForecast header.
                 * @member {bubbaloop.header.v1.IHeader|null|undefined} header
                 * @memberof bubbaloop.weather.v1.DailyForecast
                 * @instance
                 */
                DailyForecast.prototype.header = null;

                /**
                 * DailyForecast latitude.
                 * @member {number} latitude
                 * @memberof bubbaloop.weather.v1.DailyForecast
                 * @instance
                 */
                DailyForecast.prototype.latitude = 0;

                /**
                 * DailyForecast longitude.
                 * @member {number} longitude
                 * @memberof bubbaloop.weather.v1.DailyForecast
                 * @instance
                 */
                DailyForecast.prototype.longitude = 0;

                /**
                 * DailyForecast timezone.
                 * @member {string} timezone
                 * @memberof bubbaloop.weather.v1.DailyForecast
                 * @instance
                 */
                DailyForecast.prototype.timezone = "";

                /**
                 * DailyForecast entries.
                 * @member {Array.<bubbaloop.weather.v1.IDailyForecastEntry>} entries
                 * @memberof bubbaloop.weather.v1.DailyForecast
                 * @instance
                 */
                DailyForecast.prototype.entries = $util.emptyArray;

                /**
                 * Creates a new DailyForecast instance using the specified properties.
                 * @function create
                 * @memberof bubbaloop.weather.v1.DailyForecast
                 * @static
                 * @param {bubbaloop.weather.v1.IDailyForecast=} [properties] Properties to set
                 * @returns {bubbaloop.weather.v1.DailyForecast} DailyForecast instance
                 */
                DailyForecast.create = function create(properties) {
                    return new DailyForecast(properties);
                };

                /**
                 * Encodes the specified DailyForecast message. Does not implicitly {@link bubbaloop.weather.v1.DailyForecast.verify|verify} messages.
                 * @function encode
                 * @memberof bubbaloop.weather.v1.DailyForecast
                 * @static
                 * @param {bubbaloop.weather.v1.IDailyForecast} message DailyForecast message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                DailyForecast.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.header != null && Object.hasOwnProperty.call(message, "header"))
                        $root.bubbaloop.header.v1.Header.encode(message.header, writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    if (message.latitude != null && Object.hasOwnProperty.call(message, "latitude"))
                        writer.uint32(/* id 2, wireType 1 =*/17).double(message.latitude);
                    if (message.longitude != null && Object.hasOwnProperty.call(message, "longitude"))
                        writer.uint32(/* id 3, wireType 1 =*/25).double(message.longitude);
                    if (message.timezone != null && Object.hasOwnProperty.call(message, "timezone"))
                        writer.uint32(/* id 4, wireType 2 =*/34).string(message.timezone);
                    if (message.entries != null && message.entries.length)
                        for (let i = 0; i < message.entries.length; ++i)
                            $root.bubbaloop.weather.v1.DailyForecastEntry.encode(message.entries[i], writer.uint32(/* id 5, wireType 2 =*/42).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified DailyForecast message, length delimited. Does not implicitly {@link bubbaloop.weather.v1.DailyForecast.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof bubbaloop.weather.v1.DailyForecast
                 * @static
                 * @param {bubbaloop.weather.v1.IDailyForecast} message DailyForecast message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                DailyForecast.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a DailyForecast message from the specified reader or buffer.
                 * @function decode
                 * @memberof bubbaloop.weather.v1.DailyForecast
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {bubbaloop.weather.v1.DailyForecast} DailyForecast
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                DailyForecast.decode = function decode(reader, length) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    let end = length === undefined ? reader.len : reader.pos + length, message = new $root.bubbaloop.weather.v1.DailyForecast();
                    while (reader.pos < end) {
                        let tag = reader.uint32();
                        if (false)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.header = $root.bubbaloop.header.v1.Header.decode(reader, reader.uint32());
                                break;
                            }
                        case 2: {
                                message.latitude = reader.double();
                                break;
                            }
                        case 3: {
                                message.longitude = reader.double();
                                break;
                            }
                        case 4: {
                                message.timezone = reader.string();
                                break;
                            }
                        case 5: {
                                if (!(message.entries && message.entries.length))
                                    message.entries = [];
                                message.entries.push($root.bubbaloop.weather.v1.DailyForecastEntry.decode(reader, reader.uint32()));
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a DailyForecast message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof bubbaloop.weather.v1.DailyForecast
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {bubbaloop.weather.v1.DailyForecast} DailyForecast
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                DailyForecast.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a DailyForecast message.
                 * @function verify
                 * @memberof bubbaloop.weather.v1.DailyForecast
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                DailyForecast.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.header != null && message.hasOwnProperty("header")) {
                        let error = $root.bubbaloop.header.v1.Header.verify(message.header);
                        if (error)
                            return "header." + error;
                    }
                    if (message.latitude != null && message.hasOwnProperty("latitude"))
                        if (typeof message.latitude !== "number")
                            return "latitude: number expected";
                    if (message.longitude != null && message.hasOwnProperty("longitude"))
                        if (typeof message.longitude !== "number")
                            return "longitude: number expected";
                    if (message.timezone != null && message.hasOwnProperty("timezone"))
                        if (!$util.isString(message.timezone))
                            return "timezone: string expected";
                    if (message.entries != null && message.hasOwnProperty("entries")) {
                        if (!Array.isArray(message.entries))
                            return "entries: array expected";
                        for (let i = 0; i < message.entries.length; ++i) {
                            let error = $root.bubbaloop.weather.v1.DailyForecastEntry.verify(message.entries[i]);
                            if (error)
                                return "entries." + error;
                        }
                    }
                    return null;
                };

                /**
                 * Creates a DailyForecast message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof bubbaloop.weather.v1.DailyForecast
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {bubbaloop.weather.v1.DailyForecast} DailyForecast
                 */
                DailyForecast.fromObject = function fromObject(object) {
                    if (object instanceof $root.bubbaloop.weather.v1.DailyForecast)
                        return object;
                    let message = new $root.bubbaloop.weather.v1.DailyForecast();
                    if (object.header != null) {
                        if (typeof object.header !== "object")
                            throw TypeError(".bubbaloop.weather.v1.DailyForecast.header: object expected");
                        message.header = $root.bubbaloop.header.v1.Header.fromObject(object.header);
                    }
                    if (object.latitude != null)
                        message.latitude = Number(object.latitude);
                    if (object.longitude != null)
                        message.longitude = Number(object.longitude);
                    if (object.timezone != null)
                        message.timezone = String(object.timezone);
                    if (object.entries) {
                        if (!Array.isArray(object.entries))
                            throw TypeError(".bubbaloop.weather.v1.DailyForecast.entries: array expected");
                        message.entries = [];
                        for (let i = 0; i < object.entries.length; ++i) {
                            if (typeof object.entries[i] !== "object")
                                throw TypeError(".bubbaloop.weather.v1.DailyForecast.entries: object expected");
                            message.entries[i] = $root.bubbaloop.weather.v1.DailyForecastEntry.fromObject(object.entries[i]);
                        }
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a DailyForecast message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof bubbaloop.weather.v1.DailyForecast
                 * @static
                 * @param {bubbaloop.weather.v1.DailyForecast} message DailyForecast
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                DailyForecast.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    let object = {};
                    if (options.arrays || options.defaults)
                        object.entries = [];
                    if (options.defaults) {
                        object.header = null;
                        object.latitude = 0;
                        object.longitude = 0;
                        object.timezone = "";
                    }
                    if (message.header != null && message.hasOwnProperty("header"))
                        object.header = $root.bubbaloop.header.v1.Header.toObject(message.header, options);
                    if (message.latitude != null && message.hasOwnProperty("latitude"))
                        object.latitude = options.json && !isFinite(message.latitude) ? String(message.latitude) : message.latitude;
                    if (message.longitude != null && message.hasOwnProperty("longitude"))
                        object.longitude = options.json && !isFinite(message.longitude) ? String(message.longitude) : message.longitude;
                    if (message.timezone != null && message.hasOwnProperty("timezone"))
                        object.timezone = message.timezone;
                    if (message.entries && message.entries.length) {
                        object.entries = [];
                        for (let j = 0; j < message.entries.length; ++j)
                            object.entries[j] = $root.bubbaloop.weather.v1.DailyForecastEntry.toObject(message.entries[j], options);
                    }
                    return object;
                };

                /**
                 * Converts this DailyForecast to JSON.
                 * @function toJSON
                 * @memberof bubbaloop.weather.v1.DailyForecast
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                DailyForecast.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for DailyForecast
                 * @function getTypeUrl
                 * @memberof bubbaloop.weather.v1.DailyForecast
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                DailyForecast.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/bubbaloop.weather.v1.DailyForecast";
                };

                return DailyForecast;
            })();

            v1.LocationConfig = (function() {

                /**
                 * Properties of a LocationConfig.
                 * @memberof bubbaloop.weather.v1
                 * @interface ILocationConfig
                 * @property {number|null} [latitude] LocationConfig latitude
                 * @property {number|null} [longitude] LocationConfig longitude
                 * @property {string|null} [timezone] LocationConfig timezone
                 */

                /**
                 * Constructs a new LocationConfig.
                 * @memberof bubbaloop.weather.v1
                 * @classdesc Represents a LocationConfig.
                 * @implements ILocationConfig
                 * @constructor
                 * @param {bubbaloop.weather.v1.ILocationConfig=} [properties] Properties to set
                 */
                function LocationConfig(properties) {
                    if (properties)
                        for (let keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * LocationConfig latitude.
                 * @member {number} latitude
                 * @memberof bubbaloop.weather.v1.LocationConfig
                 * @instance
                 */
                LocationConfig.prototype.latitude = 0;

                /**
                 * LocationConfig longitude.
                 * @member {number} longitude
                 * @memberof bubbaloop.weather.v1.LocationConfig
                 * @instance
                 */
                LocationConfig.prototype.longitude = 0;

                /**
                 * LocationConfig timezone.
                 * @member {string} timezone
                 * @memberof bubbaloop.weather.v1.LocationConfig
                 * @instance
                 */
                LocationConfig.prototype.timezone = "";

                /**
                 * Creates a new LocationConfig instance using the specified properties.
                 * @function create
                 * @memberof bubbaloop.weather.v1.LocationConfig
                 * @static
                 * @param {bubbaloop.weather.v1.ILocationConfig=} [properties] Properties to set
                 * @returns {bubbaloop.weather.v1.LocationConfig} LocationConfig instance
                 */
                LocationConfig.create = function create(properties) {
                    return new LocationConfig(properties);
                };

                /**
                 * Encodes the specified LocationConfig message. Does not implicitly {@link bubbaloop.weather.v1.LocationConfig.verify|verify} messages.
                 * @function encode
                 * @memberof bubbaloop.weather.v1.LocationConfig
                 * @static
                 * @param {bubbaloop.weather.v1.ILocationConfig} message LocationConfig message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                LocationConfig.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.latitude != null && Object.hasOwnProperty.call(message, "latitude"))
                        writer.uint32(/* id 1, wireType 1 =*/9).double(message.latitude);
                    if (message.longitude != null && Object.hasOwnProperty.call(message, "longitude"))
                        writer.uint32(/* id 2, wireType 1 =*/17).double(message.longitude);
                    if (message.timezone != null && Object.hasOwnProperty.call(message, "timezone"))
                        writer.uint32(/* id 3, wireType 2 =*/26).string(message.timezone);
                    return writer;
                };

                /**
                 * Encodes the specified LocationConfig message, length delimited. Does not implicitly {@link bubbaloop.weather.v1.LocationConfig.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof bubbaloop.weather.v1.LocationConfig
                 * @static
                 * @param {bubbaloop.weather.v1.ILocationConfig} message LocationConfig message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                LocationConfig.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a LocationConfig message from the specified reader or buffer.
                 * @function decode
                 * @memberof bubbaloop.weather.v1.LocationConfig
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {bubbaloop.weather.v1.LocationConfig} LocationConfig
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                LocationConfig.decode = function decode(reader, length) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    let end = length === undefined ? reader.len : reader.pos + length, message = new $root.bubbaloop.weather.v1.LocationConfig();
                    while (reader.pos < end) {
                        let tag = reader.uint32();
                        if (false)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.latitude = reader.double();
                                break;
                            }
                        case 2: {
                                message.longitude = reader.double();
                                break;
                            }
                        case 3: {
                                message.timezone = reader.string();
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a LocationConfig message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof bubbaloop.weather.v1.LocationConfig
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {bubbaloop.weather.v1.LocationConfig} LocationConfig
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                LocationConfig.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a LocationConfig message.
                 * @function verify
                 * @memberof bubbaloop.weather.v1.LocationConfig
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                LocationConfig.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.latitude != null && message.hasOwnProperty("latitude"))
                        if (typeof message.latitude !== "number")
                            return "latitude: number expected";
                    if (message.longitude != null && message.hasOwnProperty("longitude"))
                        if (typeof message.longitude !== "number")
                            return "longitude: number expected";
                    if (message.timezone != null && message.hasOwnProperty("timezone"))
                        if (!$util.isString(message.timezone))
                            return "timezone: string expected";
                    return null;
                };

                /**
                 * Creates a LocationConfig message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof bubbaloop.weather.v1.LocationConfig
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {bubbaloop.weather.v1.LocationConfig} LocationConfig
                 */
                LocationConfig.fromObject = function fromObject(object) {
                    if (object instanceof $root.bubbaloop.weather.v1.LocationConfig)
                        return object;
                    let message = new $root.bubbaloop.weather.v1.LocationConfig();
                    if (object.latitude != null)
                        message.latitude = Number(object.latitude);
                    if (object.longitude != null)
                        message.longitude = Number(object.longitude);
                    if (object.timezone != null)
                        message.timezone = String(object.timezone);
                    return message;
                };

                /**
                 * Creates a plain object from a LocationConfig message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof bubbaloop.weather.v1.LocationConfig
                 * @static
                 * @param {bubbaloop.weather.v1.LocationConfig} message LocationConfig
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                LocationConfig.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    let object = {};
                    if (options.defaults) {
                        object.latitude = 0;
                        object.longitude = 0;
                        object.timezone = "";
                    }
                    if (message.latitude != null && message.hasOwnProperty("latitude"))
                        object.latitude = options.json && !isFinite(message.latitude) ? String(message.latitude) : message.latitude;
                    if (message.longitude != null && message.hasOwnProperty("longitude"))
                        object.longitude = options.json && !isFinite(message.longitude) ? String(message.longitude) : message.longitude;
                    if (message.timezone != null && message.hasOwnProperty("timezone"))
                        object.timezone = message.timezone;
                    return object;
                };

                /**
                 * Converts this LocationConfig to JSON.
                 * @function toJSON
                 * @memberof bubbaloop.weather.v1.LocationConfig
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                LocationConfig.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for LocationConfig
                 * @function getTypeUrl
                 * @memberof bubbaloop.weather.v1.LocationConfig
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                LocationConfig.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/bubbaloop.weather.v1.LocationConfig";
                };

                return LocationConfig;
            })();

            return v1;
        })();

        return weather;
    })();

    bubbaloop.daemon = (function() {

        /**
         * Namespace daemon.
         * @memberof bubbaloop
         * @namespace
         */
        const daemon = {};

        daemon.v1 = (function() {

            /**
             * Namespace v1.
             * @memberof bubbaloop.daemon
             * @namespace
             */
            const v1 = {};

            /**
             * NodeStatus enum.
             * @name bubbaloop.daemon.v1.NodeStatus
             * @enum {number}
             * @property {number} NODE_STATUS_UNKNOWN=0 NODE_STATUS_UNKNOWN value
             * @property {number} NODE_STATUS_STOPPED=1 NODE_STATUS_STOPPED value
             * @property {number} NODE_STATUS_RUNNING=2 NODE_STATUS_RUNNING value
             * @property {number} NODE_STATUS_FAILED=3 NODE_STATUS_FAILED value
             * @property {number} NODE_STATUS_INSTALLING=4 NODE_STATUS_INSTALLING value
             * @property {number} NODE_STATUS_BUILDING=5 NODE_STATUS_BUILDING value
             * @property {number} NODE_STATUS_NOT_INSTALLED=6 NODE_STATUS_NOT_INSTALLED value
             */
            v1.NodeStatus = (function() {
                const valuesById = {}, values = Object.create(valuesById);
                values[valuesById[0] = "NODE_STATUS_UNKNOWN"] = 0;
                values[valuesById[1] = "NODE_STATUS_STOPPED"] = 1;
                values[valuesById[2] = "NODE_STATUS_RUNNING"] = 2;
                values[valuesById[3] = "NODE_STATUS_FAILED"] = 3;
                values[valuesById[4] = "NODE_STATUS_INSTALLING"] = 4;
                values[valuesById[5] = "NODE_STATUS_BUILDING"] = 5;
                values[valuesById[6] = "NODE_STATUS_NOT_INSTALLED"] = 6;
                return values;
            })();

            v1.NodeState = (function() {

                /**
                 * Properties of a NodeState.
                 * @memberof bubbaloop.daemon.v1
                 * @interface INodeState
                 * @property {string|null} [name] NodeState name
                 * @property {string|null} [path] NodeState path
                 * @property {bubbaloop.daemon.v1.NodeStatus|null} [status] NodeState status
                 * @property {boolean|null} [installed] NodeState installed
                 * @property {boolean|null} [autostartEnabled] NodeState autostartEnabled
                 * @property {string|null} [version] NodeState version
                 * @property {string|null} [description] NodeState description
                 * @property {string|null} [nodeType] NodeState nodeType
                 * @property {boolean|null} [isBuilt] NodeState isBuilt
                 * @property {number|Long|null} [lastUpdatedMs] NodeState lastUpdatedMs
                 * @property {Array.<string>|null} [buildOutput] NodeState buildOutput
                 */

                /**
                 * Constructs a new NodeState.
                 * @memberof bubbaloop.daemon.v1
                 * @classdesc Represents a NodeState.
                 * @implements INodeState
                 * @constructor
                 * @param {bubbaloop.daemon.v1.INodeState=} [properties] Properties to set
                 */
                function NodeState(properties) {
                    this.buildOutput = [];
                    if (properties)
                        for (let keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * NodeState name.
                 * @member {string} name
                 * @memberof bubbaloop.daemon.v1.NodeState
                 * @instance
                 */
                NodeState.prototype.name = "";

                /**
                 * NodeState path.
                 * @member {string} path
                 * @memberof bubbaloop.daemon.v1.NodeState
                 * @instance
                 */
                NodeState.prototype.path = "";

                /**
                 * NodeState status.
                 * @member {bubbaloop.daemon.v1.NodeStatus} status
                 * @memberof bubbaloop.daemon.v1.NodeState
                 * @instance
                 */
                NodeState.prototype.status = 0;

                /**
                 * NodeState installed.
                 * @member {boolean} installed
                 * @memberof bubbaloop.daemon.v1.NodeState
                 * @instance
                 */
                NodeState.prototype.installed = false;

                /**
                 * NodeState autostartEnabled.
                 * @member {boolean} autostartEnabled
                 * @memberof bubbaloop.daemon.v1.NodeState
                 * @instance
                 */
                NodeState.prototype.autostartEnabled = false;

                /**
                 * NodeState version.
                 * @member {string} version
                 * @memberof bubbaloop.daemon.v1.NodeState
                 * @instance
                 */
                NodeState.prototype.version = "";

                /**
                 * NodeState description.
                 * @member {string} description
                 * @memberof bubbaloop.daemon.v1.NodeState
                 * @instance
                 */
                NodeState.prototype.description = "";

                /**
                 * NodeState nodeType.
                 * @member {string} nodeType
                 * @memberof bubbaloop.daemon.v1.NodeState
                 * @instance
                 */
                NodeState.prototype.nodeType = "";

                /**
                 * NodeState isBuilt.
                 * @member {boolean} isBuilt
                 * @memberof bubbaloop.daemon.v1.NodeState
                 * @instance
                 */
                NodeState.prototype.isBuilt = false;

                /**
                 * NodeState lastUpdatedMs.
                 * @member {number|Long} lastUpdatedMs
                 * @memberof bubbaloop.daemon.v1.NodeState
                 * @instance
                 */
                NodeState.prototype.lastUpdatedMs = $util.Long ? $util.Long.fromBits(0,0,false) : 0;

                /**
                 * NodeState buildOutput.
                 * @member {Array.<string>} buildOutput
                 * @memberof bubbaloop.daemon.v1.NodeState
                 * @instance
                 */
                NodeState.prototype.buildOutput = $util.emptyArray;

                /**
                 * Creates a new NodeState instance using the specified properties.
                 * @function create
                 * @memberof bubbaloop.daemon.v1.NodeState
                 * @static
                 * @param {bubbaloop.daemon.v1.INodeState=} [properties] Properties to set
                 * @returns {bubbaloop.daemon.v1.NodeState} NodeState instance
                 */
                NodeState.create = function create(properties) {
                    return new NodeState(properties);
                };

                /**
                 * Encodes the specified NodeState message. Does not implicitly {@link bubbaloop.daemon.v1.NodeState.verify|verify} messages.
                 * @function encode
                 * @memberof bubbaloop.daemon.v1.NodeState
                 * @static
                 * @param {bubbaloop.daemon.v1.INodeState} message NodeState message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                NodeState.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.name != null && Object.hasOwnProperty.call(message, "name"))
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.name);
                    if (message.path != null && Object.hasOwnProperty.call(message, "path"))
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.path);
                    if (message.status != null && Object.hasOwnProperty.call(message, "status"))
                        writer.uint32(/* id 3, wireType 0 =*/24).int32(message.status);
                    if (message.installed != null && Object.hasOwnProperty.call(message, "installed"))
                        writer.uint32(/* id 4, wireType 0 =*/32).bool(message.installed);
                    if (message.autostartEnabled != null && Object.hasOwnProperty.call(message, "autostartEnabled"))
                        writer.uint32(/* id 5, wireType 0 =*/40).bool(message.autostartEnabled);
                    if (message.version != null && Object.hasOwnProperty.call(message, "version"))
                        writer.uint32(/* id 6, wireType 2 =*/50).string(message.version);
                    if (message.description != null && Object.hasOwnProperty.call(message, "description"))
                        writer.uint32(/* id 7, wireType 2 =*/58).string(message.description);
                    if (message.nodeType != null && Object.hasOwnProperty.call(message, "nodeType"))
                        writer.uint32(/* id 8, wireType 2 =*/66).string(message.nodeType);
                    if (message.isBuilt != null && Object.hasOwnProperty.call(message, "isBuilt"))
                        writer.uint32(/* id 9, wireType 0 =*/72).bool(message.isBuilt);
                    if (message.lastUpdatedMs != null && Object.hasOwnProperty.call(message, "lastUpdatedMs"))
                        writer.uint32(/* id 10, wireType 0 =*/80).int64(message.lastUpdatedMs);
                    if (message.buildOutput != null && message.buildOutput.length)
                        for (let i = 0; i < message.buildOutput.length; ++i)
                            writer.uint32(/* id 11, wireType 2 =*/90).string(message.buildOutput[i]);
                    return writer;
                };

                /**
                 * Encodes the specified NodeState message, length delimited. Does not implicitly {@link bubbaloop.daemon.v1.NodeState.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof bubbaloop.daemon.v1.NodeState
                 * @static
                 * @param {bubbaloop.daemon.v1.INodeState} message NodeState message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                NodeState.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a NodeState message from the specified reader or buffer.
                 * @function decode
                 * @memberof bubbaloop.daemon.v1.NodeState
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {bubbaloop.daemon.v1.NodeState} NodeState
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                NodeState.decode = function decode(reader, length) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    let end = length === undefined ? reader.len : reader.pos + length, message = new $root.bubbaloop.daemon.v1.NodeState();
                    while (reader.pos < end) {
                        let tag = reader.uint32();
                        if (false)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.name = reader.string();
                                break;
                            }
                        case 2: {
                                message.path = reader.string();
                                break;
                            }
                        case 3: {
                                message.status = reader.int32();
                                break;
                            }
                        case 4: {
                                message.installed = reader.bool();
                                break;
                            }
                        case 5: {
                                message.autostartEnabled = reader.bool();
                                break;
                            }
                        case 6: {
                                message.version = reader.string();
                                break;
                            }
                        case 7: {
                                message.description = reader.string();
                                break;
                            }
                        case 8: {
                                message.nodeType = reader.string();
                                break;
                            }
                        case 9: {
                                message.isBuilt = reader.bool();
                                break;
                            }
                        case 10: {
                                message.lastUpdatedMs = reader.int64();
                                break;
                            }
                        case 11: {
                                if (!(message.buildOutput && message.buildOutput.length))
                                    message.buildOutput = [];
                                message.buildOutput.push(reader.string());
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a NodeState message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof bubbaloop.daemon.v1.NodeState
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {bubbaloop.daemon.v1.NodeState} NodeState
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                NodeState.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a NodeState message.
                 * @function verify
                 * @memberof bubbaloop.daemon.v1.NodeState
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                NodeState.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.name != null && message.hasOwnProperty("name"))
                        if (!$util.isString(message.name))
                            return "name: string expected";
                    if (message.path != null && message.hasOwnProperty("path"))
                        if (!$util.isString(message.path))
                            return "path: string expected";
                    if (message.status != null && message.hasOwnProperty("status"))
                        switch (message.status) {
                        default:
                            return "status: enum value expected";
                        case 0:
                        case 1:
                        case 2:
                        case 3:
                        case 4:
                        case 5:
                        case 6:
                            break;
                        }
                    if (message.installed != null && message.hasOwnProperty("installed"))
                        if (typeof message.installed !== "boolean")
                            return "installed: boolean expected";
                    if (message.autostartEnabled != null && message.hasOwnProperty("autostartEnabled"))
                        if (typeof message.autostartEnabled !== "boolean")
                            return "autostartEnabled: boolean expected";
                    if (message.version != null && message.hasOwnProperty("version"))
                        if (!$util.isString(message.version))
                            return "version: string expected";
                    if (message.description != null && message.hasOwnProperty("description"))
                        if (!$util.isString(message.description))
                            return "description: string expected";
                    if (message.nodeType != null && message.hasOwnProperty("nodeType"))
                        if (!$util.isString(message.nodeType))
                            return "nodeType: string expected";
                    if (message.isBuilt != null && message.hasOwnProperty("isBuilt"))
                        if (typeof message.isBuilt !== "boolean")
                            return "isBuilt: boolean expected";
                    if (message.lastUpdatedMs != null && message.hasOwnProperty("lastUpdatedMs"))
                        if (!$util.isInteger(message.lastUpdatedMs) && !(message.lastUpdatedMs && $util.isInteger(message.lastUpdatedMs.low) && $util.isInteger(message.lastUpdatedMs.high)))
                            return "lastUpdatedMs: integer|Long expected";
                    if (message.buildOutput != null && message.hasOwnProperty("buildOutput")) {
                        if (!Array.isArray(message.buildOutput))
                            return "buildOutput: array expected";
                        for (let i = 0; i < message.buildOutput.length; ++i)
                            if (!$util.isString(message.buildOutput[i]))
                                return "buildOutput: string[] expected";
                    }
                    return null;
                };

                /**
                 * Creates a NodeState message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof bubbaloop.daemon.v1.NodeState
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {bubbaloop.daemon.v1.NodeState} NodeState
                 */
                NodeState.fromObject = function fromObject(object) {
                    if (object instanceof $root.bubbaloop.daemon.v1.NodeState)
                        return object;
                    let message = new $root.bubbaloop.daemon.v1.NodeState();
                    if (object.name != null)
                        message.name = String(object.name);
                    if (object.path != null)
                        message.path = String(object.path);
                    switch (object.status) {
                    default:
                        if (typeof object.status === "number") {
                            message.status = object.status;
                            break;
                        }
                        break;
                    case "NODE_STATUS_UNKNOWN":
                    case 0:
                        message.status = 0;
                        break;
                    case "NODE_STATUS_STOPPED":
                    case 1:
                        message.status = 1;
                        break;
                    case "NODE_STATUS_RUNNING":
                    case 2:
                        message.status = 2;
                        break;
                    case "NODE_STATUS_FAILED":
                    case 3:
                        message.status = 3;
                        break;
                    case "NODE_STATUS_INSTALLING":
                    case 4:
                        message.status = 4;
                        break;
                    case "NODE_STATUS_BUILDING":
                    case 5:
                        message.status = 5;
                        break;
                    case "NODE_STATUS_NOT_INSTALLED":
                    case 6:
                        message.status = 6;
                        break;
                    }
                    if (object.installed != null)
                        message.installed = Boolean(object.installed);
                    if (object.autostartEnabled != null)
                        message.autostartEnabled = Boolean(object.autostartEnabled);
                    if (object.version != null)
                        message.version = String(object.version);
                    if (object.description != null)
                        message.description = String(object.description);
                    if (object.nodeType != null)
                        message.nodeType = String(object.nodeType);
                    if (object.isBuilt != null)
                        message.isBuilt = Boolean(object.isBuilt);
                    if (object.lastUpdatedMs != null)
                        if ($util.Long)
                            (message.lastUpdatedMs = $util.Long.fromValue(object.lastUpdatedMs)).unsigned = false;
                        else if (typeof object.lastUpdatedMs === "string")
                            message.lastUpdatedMs = parseInt(object.lastUpdatedMs, 10);
                        else if (typeof object.lastUpdatedMs === "number")
                            message.lastUpdatedMs = object.lastUpdatedMs;
                        else if (typeof object.lastUpdatedMs === "object")
                            message.lastUpdatedMs = new $util.LongBits(object.lastUpdatedMs.low >>> 0, object.lastUpdatedMs.high >>> 0).toNumber();
                    if (object.buildOutput) {
                        if (!Array.isArray(object.buildOutput))
                            throw TypeError(".bubbaloop.daemon.v1.NodeState.buildOutput: array expected");
                        message.buildOutput = [];
                        for (let i = 0; i < object.buildOutput.length; ++i)
                            message.buildOutput[i] = String(object.buildOutput[i]);
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a NodeState message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof bubbaloop.daemon.v1.NodeState
                 * @static
                 * @param {bubbaloop.daemon.v1.NodeState} message NodeState
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                NodeState.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    let object = {};
                    if (options.arrays || options.defaults)
                        object.buildOutput = [];
                    if (options.defaults) {
                        object.name = "";
                        object.path = "";
                        object.status = options.enums === String ? "NODE_STATUS_UNKNOWN" : 0;
                        object.installed = false;
                        object.autostartEnabled = false;
                        object.version = "";
                        object.description = "";
                        object.nodeType = "";
                        object.isBuilt = false;
                        if ($util.Long) {
                            let long = new $util.Long(0, 0, false);
                            object.lastUpdatedMs = options.longs === String ? long.toString() : options.longs === Number ? long.toNumber() : long;
                        } else
                            object.lastUpdatedMs = options.longs === String ? "0" : 0;
                    }
                    if (message.name != null && message.hasOwnProperty("name"))
                        object.name = message.name;
                    if (message.path != null && message.hasOwnProperty("path"))
                        object.path = message.path;
                    if (message.status != null && message.hasOwnProperty("status"))
                        object.status = options.enums === String ? $root.bubbaloop.daemon.v1.NodeStatus[message.status] === undefined ? message.status : $root.bubbaloop.daemon.v1.NodeStatus[message.status] : message.status;
                    if (message.installed != null && message.hasOwnProperty("installed"))
                        object.installed = message.installed;
                    if (message.autostartEnabled != null && message.hasOwnProperty("autostartEnabled"))
                        object.autostartEnabled = message.autostartEnabled;
                    if (message.version != null && message.hasOwnProperty("version"))
                        object.version = message.version;
                    if (message.description != null && message.hasOwnProperty("description"))
                        object.description = message.description;
                    if (message.nodeType != null && message.hasOwnProperty("nodeType"))
                        object.nodeType = message.nodeType;
                    if (message.isBuilt != null && message.hasOwnProperty("isBuilt"))
                        object.isBuilt = message.isBuilt;
                    if (message.lastUpdatedMs != null && message.hasOwnProperty("lastUpdatedMs"))
                        if (typeof message.lastUpdatedMs === "number")
                            object.lastUpdatedMs = options.longs === String ? String(message.lastUpdatedMs) : message.lastUpdatedMs;
                        else
                            object.lastUpdatedMs = options.longs === String ? $util.Long.prototype.toString.call(message.lastUpdatedMs) : options.longs === Number ? new $util.LongBits(message.lastUpdatedMs.low >>> 0, message.lastUpdatedMs.high >>> 0).toNumber() : message.lastUpdatedMs;
                    if (message.buildOutput && message.buildOutput.length) {
                        object.buildOutput = [];
                        for (let j = 0; j < message.buildOutput.length; ++j)
                            object.buildOutput[j] = message.buildOutput[j];
                    }
                    return object;
                };

                /**
                 * Converts this NodeState to JSON.
                 * @function toJSON
                 * @memberof bubbaloop.daemon.v1.NodeState
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                NodeState.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for NodeState
                 * @function getTypeUrl
                 * @memberof bubbaloop.daemon.v1.NodeState
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                NodeState.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/bubbaloop.daemon.v1.NodeState";
                };

                return NodeState;
            })();

            v1.NodeList = (function() {

                /**
                 * Properties of a NodeList.
                 * @memberof bubbaloop.daemon.v1
                 * @interface INodeList
                 * @property {Array.<bubbaloop.daemon.v1.INodeState>|null} [nodes] NodeList nodes
                 * @property {number|Long|null} [timestampMs] NodeList timestampMs
                 */

                /**
                 * Constructs a new NodeList.
                 * @memberof bubbaloop.daemon.v1
                 * @classdesc Represents a NodeList.
                 * @implements INodeList
                 * @constructor
                 * @param {bubbaloop.daemon.v1.INodeList=} [properties] Properties to set
                 */
                function NodeList(properties) {
                    this.nodes = [];
                    if (properties)
                        for (let keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * NodeList nodes.
                 * @member {Array.<bubbaloop.daemon.v1.INodeState>} nodes
                 * @memberof bubbaloop.daemon.v1.NodeList
                 * @instance
                 */
                NodeList.prototype.nodes = $util.emptyArray;

                /**
                 * NodeList timestampMs.
                 * @member {number|Long} timestampMs
                 * @memberof bubbaloop.daemon.v1.NodeList
                 * @instance
                 */
                NodeList.prototype.timestampMs = $util.Long ? $util.Long.fromBits(0,0,false) : 0;

                /**
                 * Creates a new NodeList instance using the specified properties.
                 * @function create
                 * @memberof bubbaloop.daemon.v1.NodeList
                 * @static
                 * @param {bubbaloop.daemon.v1.INodeList=} [properties] Properties to set
                 * @returns {bubbaloop.daemon.v1.NodeList} NodeList instance
                 */
                NodeList.create = function create(properties) {
                    return new NodeList(properties);
                };

                /**
                 * Encodes the specified NodeList message. Does not implicitly {@link bubbaloop.daemon.v1.NodeList.verify|verify} messages.
                 * @function encode
                 * @memberof bubbaloop.daemon.v1.NodeList
                 * @static
                 * @param {bubbaloop.daemon.v1.INodeList} message NodeList message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                NodeList.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.nodes != null && message.nodes.length)
                        for (let i = 0; i < message.nodes.length; ++i)
                            $root.bubbaloop.daemon.v1.NodeState.encode(message.nodes[i], writer.uint32(/* id 1, wireType 2 =*/10).fork()).ldelim();
                    if (message.timestampMs != null && Object.hasOwnProperty.call(message, "timestampMs"))
                        writer.uint32(/* id 2, wireType 0 =*/16).int64(message.timestampMs);
                    return writer;
                };

                /**
                 * Encodes the specified NodeList message, length delimited. Does not implicitly {@link bubbaloop.daemon.v1.NodeList.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof bubbaloop.daemon.v1.NodeList
                 * @static
                 * @param {bubbaloop.daemon.v1.INodeList} message NodeList message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                NodeList.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a NodeList message from the specified reader or buffer.
                 * @function decode
                 * @memberof bubbaloop.daemon.v1.NodeList
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {bubbaloop.daemon.v1.NodeList} NodeList
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                NodeList.decode = function decode(reader, length) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    let end = length === undefined ? reader.len : reader.pos + length, message = new $root.bubbaloop.daemon.v1.NodeList();
                    while (reader.pos < end) {
                        let tag = reader.uint32();
                        if (false)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                if (!(message.nodes && message.nodes.length))
                                    message.nodes = [];
                                message.nodes.push($root.bubbaloop.daemon.v1.NodeState.decode(reader, reader.uint32()));
                                break;
                            }
                        case 2: {
                                message.timestampMs = reader.int64();
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a NodeList message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof bubbaloop.daemon.v1.NodeList
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {bubbaloop.daemon.v1.NodeList} NodeList
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                NodeList.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a NodeList message.
                 * @function verify
                 * @memberof bubbaloop.daemon.v1.NodeList
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                NodeList.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.nodes != null && message.hasOwnProperty("nodes")) {
                        if (!Array.isArray(message.nodes))
                            return "nodes: array expected";
                        for (let i = 0; i < message.nodes.length; ++i) {
                            let error = $root.bubbaloop.daemon.v1.NodeState.verify(message.nodes[i]);
                            if (error)
                                return "nodes." + error;
                        }
                    }
                    if (message.timestampMs != null && message.hasOwnProperty("timestampMs"))
                        if (!$util.isInteger(message.timestampMs) && !(message.timestampMs && $util.isInteger(message.timestampMs.low) && $util.isInteger(message.timestampMs.high)))
                            return "timestampMs: integer|Long expected";
                    return null;
                };

                /**
                 * Creates a NodeList message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof bubbaloop.daemon.v1.NodeList
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {bubbaloop.daemon.v1.NodeList} NodeList
                 */
                NodeList.fromObject = function fromObject(object) {
                    if (object instanceof $root.bubbaloop.daemon.v1.NodeList)
                        return object;
                    let message = new $root.bubbaloop.daemon.v1.NodeList();
                    if (object.nodes) {
                        if (!Array.isArray(object.nodes))
                            throw TypeError(".bubbaloop.daemon.v1.NodeList.nodes: array expected");
                        message.nodes = [];
                        for (let i = 0; i < object.nodes.length; ++i) {
                            if (typeof object.nodes[i] !== "object")
                                throw TypeError(".bubbaloop.daemon.v1.NodeList.nodes: object expected");
                            message.nodes[i] = $root.bubbaloop.daemon.v1.NodeState.fromObject(object.nodes[i]);
                        }
                    }
                    if (object.timestampMs != null)
                        if ($util.Long)
                            (message.timestampMs = $util.Long.fromValue(object.timestampMs)).unsigned = false;
                        else if (typeof object.timestampMs === "string")
                            message.timestampMs = parseInt(object.timestampMs, 10);
                        else if (typeof object.timestampMs === "number")
                            message.timestampMs = object.timestampMs;
                        else if (typeof object.timestampMs === "object")
                            message.timestampMs = new $util.LongBits(object.timestampMs.low >>> 0, object.timestampMs.high >>> 0).toNumber();
                    return message;
                };

                /**
                 * Creates a plain object from a NodeList message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof bubbaloop.daemon.v1.NodeList
                 * @static
                 * @param {bubbaloop.daemon.v1.NodeList} message NodeList
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                NodeList.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    let object = {};
                    if (options.arrays || options.defaults)
                        object.nodes = [];
                    if (options.defaults)
                        if ($util.Long) {
                            let long = new $util.Long(0, 0, false);
                            object.timestampMs = options.longs === String ? long.toString() : options.longs === Number ? long.toNumber() : long;
                        } else
                            object.timestampMs = options.longs === String ? "0" : 0;
                    if (message.nodes && message.nodes.length) {
                        object.nodes = [];
                        for (let j = 0; j < message.nodes.length; ++j)
                            object.nodes[j] = $root.bubbaloop.daemon.v1.NodeState.toObject(message.nodes[j], options);
                    }
                    if (message.timestampMs != null && message.hasOwnProperty("timestampMs"))
                        if (typeof message.timestampMs === "number")
                            object.timestampMs = options.longs === String ? String(message.timestampMs) : message.timestampMs;
                        else
                            object.timestampMs = options.longs === String ? $util.Long.prototype.toString.call(message.timestampMs) : options.longs === Number ? new $util.LongBits(message.timestampMs.low >>> 0, message.timestampMs.high >>> 0).toNumber() : message.timestampMs;
                    return object;
                };

                /**
                 * Converts this NodeList to JSON.
                 * @function toJSON
                 * @memberof bubbaloop.daemon.v1.NodeList
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                NodeList.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for NodeList
                 * @function getTypeUrl
                 * @memberof bubbaloop.daemon.v1.NodeList
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                NodeList.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/bubbaloop.daemon.v1.NodeList";
                };

                return NodeList;
            })();

            /**
             * CommandType enum.
             * @name bubbaloop.daemon.v1.CommandType
             * @enum {number}
             * @property {number} COMMAND_TYPE_START=0 COMMAND_TYPE_START value
             * @property {number} COMMAND_TYPE_STOP=1 COMMAND_TYPE_STOP value
             * @property {number} COMMAND_TYPE_RESTART=2 COMMAND_TYPE_RESTART value
             * @property {number} COMMAND_TYPE_INSTALL=3 COMMAND_TYPE_INSTALL value
             * @property {number} COMMAND_TYPE_UNINSTALL=4 COMMAND_TYPE_UNINSTALL value
             * @property {number} COMMAND_TYPE_BUILD=5 COMMAND_TYPE_BUILD value
             * @property {number} COMMAND_TYPE_CLEAN=6 COMMAND_TYPE_CLEAN value
             * @property {number} COMMAND_TYPE_ENABLE_AUTOSTART=7 COMMAND_TYPE_ENABLE_AUTOSTART value
             * @property {number} COMMAND_TYPE_DISABLE_AUTOSTART=8 COMMAND_TYPE_DISABLE_AUTOSTART value
             * @property {number} COMMAND_TYPE_ADD_NODE=9 COMMAND_TYPE_ADD_NODE value
             * @property {number} COMMAND_TYPE_REMOVE_NODE=10 COMMAND_TYPE_REMOVE_NODE value
             * @property {number} COMMAND_TYPE_REFRESH=11 COMMAND_TYPE_REFRESH value
             * @property {number} COMMAND_TYPE_GET_LOGS=12 COMMAND_TYPE_GET_LOGS value
             */
            v1.CommandType = (function() {
                const valuesById = {}, values = Object.create(valuesById);
                values[valuesById[0] = "COMMAND_TYPE_START"] = 0;
                values[valuesById[1] = "COMMAND_TYPE_STOP"] = 1;
                values[valuesById[2] = "COMMAND_TYPE_RESTART"] = 2;
                values[valuesById[3] = "COMMAND_TYPE_INSTALL"] = 3;
                values[valuesById[4] = "COMMAND_TYPE_UNINSTALL"] = 4;
                values[valuesById[5] = "COMMAND_TYPE_BUILD"] = 5;
                values[valuesById[6] = "COMMAND_TYPE_CLEAN"] = 6;
                values[valuesById[7] = "COMMAND_TYPE_ENABLE_AUTOSTART"] = 7;
                values[valuesById[8] = "COMMAND_TYPE_DISABLE_AUTOSTART"] = 8;
                values[valuesById[9] = "COMMAND_TYPE_ADD_NODE"] = 9;
                values[valuesById[10] = "COMMAND_TYPE_REMOVE_NODE"] = 10;
                values[valuesById[11] = "COMMAND_TYPE_REFRESH"] = 11;
                values[valuesById[12] = "COMMAND_TYPE_GET_LOGS"] = 12;
                return values;
            })();

            v1.NodeCommand = (function() {

                /**
                 * Properties of a NodeCommand.
                 * @memberof bubbaloop.daemon.v1
                 * @interface INodeCommand
                 * @property {bubbaloop.daemon.v1.CommandType|null} [command] NodeCommand command
                 * @property {string|null} [nodeName] NodeCommand nodeName
                 * @property {string|null} [nodePath] NodeCommand nodePath
                 * @property {string|null} [requestId] NodeCommand requestId
                 */

                /**
                 * Constructs a new NodeCommand.
                 * @memberof bubbaloop.daemon.v1
                 * @classdesc Represents a NodeCommand.
                 * @implements INodeCommand
                 * @constructor
                 * @param {bubbaloop.daemon.v1.INodeCommand=} [properties] Properties to set
                 */
                function NodeCommand(properties) {
                    if (properties)
                        for (let keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * NodeCommand command.
                 * @member {bubbaloop.daemon.v1.CommandType} command
                 * @memberof bubbaloop.daemon.v1.NodeCommand
                 * @instance
                 */
                NodeCommand.prototype.command = 0;

                /**
                 * NodeCommand nodeName.
                 * @member {string} nodeName
                 * @memberof bubbaloop.daemon.v1.NodeCommand
                 * @instance
                 */
                NodeCommand.prototype.nodeName = "";

                /**
                 * NodeCommand nodePath.
                 * @member {string} nodePath
                 * @memberof bubbaloop.daemon.v1.NodeCommand
                 * @instance
                 */
                NodeCommand.prototype.nodePath = "";

                /**
                 * NodeCommand requestId.
                 * @member {string} requestId
                 * @memberof bubbaloop.daemon.v1.NodeCommand
                 * @instance
                 */
                NodeCommand.prototype.requestId = "";

                /**
                 * Creates a new NodeCommand instance using the specified properties.
                 * @function create
                 * @memberof bubbaloop.daemon.v1.NodeCommand
                 * @static
                 * @param {bubbaloop.daemon.v1.INodeCommand=} [properties] Properties to set
                 * @returns {bubbaloop.daemon.v1.NodeCommand} NodeCommand instance
                 */
                NodeCommand.create = function create(properties) {
                    return new NodeCommand(properties);
                };

                /**
                 * Encodes the specified NodeCommand message. Does not implicitly {@link bubbaloop.daemon.v1.NodeCommand.verify|verify} messages.
                 * @function encode
                 * @memberof bubbaloop.daemon.v1.NodeCommand
                 * @static
                 * @param {bubbaloop.daemon.v1.INodeCommand} message NodeCommand message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                NodeCommand.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.command != null && Object.hasOwnProperty.call(message, "command"))
                        writer.uint32(/* id 1, wireType 0 =*/8).int32(message.command);
                    if (message.nodeName != null && Object.hasOwnProperty.call(message, "nodeName"))
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.nodeName);
                    if (message.nodePath != null && Object.hasOwnProperty.call(message, "nodePath"))
                        writer.uint32(/* id 3, wireType 2 =*/26).string(message.nodePath);
                    if (message.requestId != null && Object.hasOwnProperty.call(message, "requestId"))
                        writer.uint32(/* id 4, wireType 2 =*/34).string(message.requestId);
                    return writer;
                };

                /**
                 * Encodes the specified NodeCommand message, length delimited. Does not implicitly {@link bubbaloop.daemon.v1.NodeCommand.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof bubbaloop.daemon.v1.NodeCommand
                 * @static
                 * @param {bubbaloop.daemon.v1.INodeCommand} message NodeCommand message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                NodeCommand.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a NodeCommand message from the specified reader or buffer.
                 * @function decode
                 * @memberof bubbaloop.daemon.v1.NodeCommand
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {bubbaloop.daemon.v1.NodeCommand} NodeCommand
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                NodeCommand.decode = function decode(reader, length) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    let end = length === undefined ? reader.len : reader.pos + length, message = new $root.bubbaloop.daemon.v1.NodeCommand();
                    while (reader.pos < end) {
                        let tag = reader.uint32();
                        if (false)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.command = reader.int32();
                                break;
                            }
                        case 2: {
                                message.nodeName = reader.string();
                                break;
                            }
                        case 3: {
                                message.nodePath = reader.string();
                                break;
                            }
                        case 4: {
                                message.requestId = reader.string();
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a NodeCommand message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof bubbaloop.daemon.v1.NodeCommand
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {bubbaloop.daemon.v1.NodeCommand} NodeCommand
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                NodeCommand.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a NodeCommand message.
                 * @function verify
                 * @memberof bubbaloop.daemon.v1.NodeCommand
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                NodeCommand.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.command != null && message.hasOwnProperty("command"))
                        switch (message.command) {
                        default:
                            return "command: enum value expected";
                        case 0:
                        case 1:
                        case 2:
                        case 3:
                        case 4:
                        case 5:
                        case 6:
                        case 7:
                        case 8:
                        case 9:
                        case 10:
                        case 11:
                        case 12:
                            break;
                        }
                    if (message.nodeName != null && message.hasOwnProperty("nodeName"))
                        if (!$util.isString(message.nodeName))
                            return "nodeName: string expected";
                    if (message.nodePath != null && message.hasOwnProperty("nodePath"))
                        if (!$util.isString(message.nodePath))
                            return "nodePath: string expected";
                    if (message.requestId != null && message.hasOwnProperty("requestId"))
                        if (!$util.isString(message.requestId))
                            return "requestId: string expected";
                    return null;
                };

                /**
                 * Creates a NodeCommand message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof bubbaloop.daemon.v1.NodeCommand
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {bubbaloop.daemon.v1.NodeCommand} NodeCommand
                 */
                NodeCommand.fromObject = function fromObject(object) {
                    if (object instanceof $root.bubbaloop.daemon.v1.NodeCommand)
                        return object;
                    let message = new $root.bubbaloop.daemon.v1.NodeCommand();
                    switch (object.command) {
                    default:
                        if (typeof object.command === "number") {
                            message.command = object.command;
                            break;
                        }
                        break;
                    case "COMMAND_TYPE_START":
                    case 0:
                        message.command = 0;
                        break;
                    case "COMMAND_TYPE_STOP":
                    case 1:
                        message.command = 1;
                        break;
                    case "COMMAND_TYPE_RESTART":
                    case 2:
                        message.command = 2;
                        break;
                    case "COMMAND_TYPE_INSTALL":
                    case 3:
                        message.command = 3;
                        break;
                    case "COMMAND_TYPE_UNINSTALL":
                    case 4:
                        message.command = 4;
                        break;
                    case "COMMAND_TYPE_BUILD":
                    case 5:
                        message.command = 5;
                        break;
                    case "COMMAND_TYPE_CLEAN":
                    case 6:
                        message.command = 6;
                        break;
                    case "COMMAND_TYPE_ENABLE_AUTOSTART":
                    case 7:
                        message.command = 7;
                        break;
                    case "COMMAND_TYPE_DISABLE_AUTOSTART":
                    case 8:
                        message.command = 8;
                        break;
                    case "COMMAND_TYPE_ADD_NODE":
                    case 9:
                        message.command = 9;
                        break;
                    case "COMMAND_TYPE_REMOVE_NODE":
                    case 10:
                        message.command = 10;
                        break;
                    case "COMMAND_TYPE_REFRESH":
                    case 11:
                        message.command = 11;
                        break;
                    case "COMMAND_TYPE_GET_LOGS":
                    case 12:
                        message.command = 12;
                        break;
                    }
                    if (object.nodeName != null)
                        message.nodeName = String(object.nodeName);
                    if (object.nodePath != null)
                        message.nodePath = String(object.nodePath);
                    if (object.requestId != null)
                        message.requestId = String(object.requestId);
                    return message;
                };

                /**
                 * Creates a plain object from a NodeCommand message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof bubbaloop.daemon.v1.NodeCommand
                 * @static
                 * @param {bubbaloop.daemon.v1.NodeCommand} message NodeCommand
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                NodeCommand.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    let object = {};
                    if (options.defaults) {
                        object.command = options.enums === String ? "COMMAND_TYPE_START" : 0;
                        object.nodeName = "";
                        object.nodePath = "";
                        object.requestId = "";
                    }
                    if (message.command != null && message.hasOwnProperty("command"))
                        object.command = options.enums === String ? $root.bubbaloop.daemon.v1.CommandType[message.command] === undefined ? message.command : $root.bubbaloop.daemon.v1.CommandType[message.command] : message.command;
                    if (message.nodeName != null && message.hasOwnProperty("nodeName"))
                        object.nodeName = message.nodeName;
                    if (message.nodePath != null && message.hasOwnProperty("nodePath"))
                        object.nodePath = message.nodePath;
                    if (message.requestId != null && message.hasOwnProperty("requestId"))
                        object.requestId = message.requestId;
                    return object;
                };

                /**
                 * Converts this NodeCommand to JSON.
                 * @function toJSON
                 * @memberof bubbaloop.daemon.v1.NodeCommand
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                NodeCommand.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for NodeCommand
                 * @function getTypeUrl
                 * @memberof bubbaloop.daemon.v1.NodeCommand
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                NodeCommand.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/bubbaloop.daemon.v1.NodeCommand";
                };

                return NodeCommand;
            })();

            v1.CommandResult = (function() {

                /**
                 * Properties of a CommandResult.
                 * @memberof bubbaloop.daemon.v1
                 * @interface ICommandResult
                 * @property {string|null} [requestId] CommandResult requestId
                 * @property {boolean|null} [success] CommandResult success
                 * @property {string|null} [message] CommandResult message
                 * @property {string|null} [output] CommandResult output
                 * @property {bubbaloop.daemon.v1.INodeState|null} [nodeState] CommandResult nodeState
                 */

                /**
                 * Constructs a new CommandResult.
                 * @memberof bubbaloop.daemon.v1
                 * @classdesc Represents a CommandResult.
                 * @implements ICommandResult
                 * @constructor
                 * @param {bubbaloop.daemon.v1.ICommandResult=} [properties] Properties to set
                 */
                function CommandResult(properties) {
                    if (properties)
                        for (let keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * CommandResult requestId.
                 * @member {string} requestId
                 * @memberof bubbaloop.daemon.v1.CommandResult
                 * @instance
                 */
                CommandResult.prototype.requestId = "";

                /**
                 * CommandResult success.
                 * @member {boolean} success
                 * @memberof bubbaloop.daemon.v1.CommandResult
                 * @instance
                 */
                CommandResult.prototype.success = false;

                /**
                 * CommandResult message.
                 * @member {string} message
                 * @memberof bubbaloop.daemon.v1.CommandResult
                 * @instance
                 */
                CommandResult.prototype.message = "";

                /**
                 * CommandResult output.
                 * @member {string} output
                 * @memberof bubbaloop.daemon.v1.CommandResult
                 * @instance
                 */
                CommandResult.prototype.output = "";

                /**
                 * CommandResult nodeState.
                 * @member {bubbaloop.daemon.v1.INodeState|null|undefined} nodeState
                 * @memberof bubbaloop.daemon.v1.CommandResult
                 * @instance
                 */
                CommandResult.prototype.nodeState = null;

                /**
                 * Creates a new CommandResult instance using the specified properties.
                 * @function create
                 * @memberof bubbaloop.daemon.v1.CommandResult
                 * @static
                 * @param {bubbaloop.daemon.v1.ICommandResult=} [properties] Properties to set
                 * @returns {bubbaloop.daemon.v1.CommandResult} CommandResult instance
                 */
                CommandResult.create = function create(properties) {
                    return new CommandResult(properties);
                };

                /**
                 * Encodes the specified CommandResult message. Does not implicitly {@link bubbaloop.daemon.v1.CommandResult.verify|verify} messages.
                 * @function encode
                 * @memberof bubbaloop.daemon.v1.CommandResult
                 * @static
                 * @param {bubbaloop.daemon.v1.ICommandResult} message CommandResult message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                CommandResult.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.requestId != null && Object.hasOwnProperty.call(message, "requestId"))
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.requestId);
                    if (message.success != null && Object.hasOwnProperty.call(message, "success"))
                        writer.uint32(/* id 2, wireType 0 =*/16).bool(message.success);
                    if (message.message != null && Object.hasOwnProperty.call(message, "message"))
                        writer.uint32(/* id 3, wireType 2 =*/26).string(message.message);
                    if (message.output != null && Object.hasOwnProperty.call(message, "output"))
                        writer.uint32(/* id 4, wireType 2 =*/34).string(message.output);
                    if (message.nodeState != null && Object.hasOwnProperty.call(message, "nodeState"))
                        $root.bubbaloop.daemon.v1.NodeState.encode(message.nodeState, writer.uint32(/* id 5, wireType 2 =*/42).fork()).ldelim();
                    return writer;
                };

                /**
                 * Encodes the specified CommandResult message, length delimited. Does not implicitly {@link bubbaloop.daemon.v1.CommandResult.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof bubbaloop.daemon.v1.CommandResult
                 * @static
                 * @param {bubbaloop.daemon.v1.ICommandResult} message CommandResult message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                CommandResult.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a CommandResult message from the specified reader or buffer.
                 * @function decode
                 * @memberof bubbaloop.daemon.v1.CommandResult
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {bubbaloop.daemon.v1.CommandResult} CommandResult
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                CommandResult.decode = function decode(reader, length) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    let end = length === undefined ? reader.len : reader.pos + length, message = new $root.bubbaloop.daemon.v1.CommandResult();
                    while (reader.pos < end) {
                        let tag = reader.uint32();
                        if (false)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.requestId = reader.string();
                                break;
                            }
                        case 2: {
                                message.success = reader.bool();
                                break;
                            }
                        case 3: {
                                message.message = reader.string();
                                break;
                            }
                        case 4: {
                                message.output = reader.string();
                                break;
                            }
                        case 5: {
                                message.nodeState = $root.bubbaloop.daemon.v1.NodeState.decode(reader, reader.uint32());
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a CommandResult message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof bubbaloop.daemon.v1.CommandResult
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {bubbaloop.daemon.v1.CommandResult} CommandResult
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                CommandResult.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a CommandResult message.
                 * @function verify
                 * @memberof bubbaloop.daemon.v1.CommandResult
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                CommandResult.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.requestId != null && message.hasOwnProperty("requestId"))
                        if (!$util.isString(message.requestId))
                            return "requestId: string expected";
                    if (message.success != null && message.hasOwnProperty("success"))
                        if (typeof message.success !== "boolean")
                            return "success: boolean expected";
                    if (message.message != null && message.hasOwnProperty("message"))
                        if (!$util.isString(message.message))
                            return "message: string expected";
                    if (message.output != null && message.hasOwnProperty("output"))
                        if (!$util.isString(message.output))
                            return "output: string expected";
                    if (message.nodeState != null && message.hasOwnProperty("nodeState")) {
                        let error = $root.bubbaloop.daemon.v1.NodeState.verify(message.nodeState);
                        if (error)
                            return "nodeState." + error;
                    }
                    return null;
                };

                /**
                 * Creates a CommandResult message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof bubbaloop.daemon.v1.CommandResult
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {bubbaloop.daemon.v1.CommandResult} CommandResult
                 */
                CommandResult.fromObject = function fromObject(object) {
                    if (object instanceof $root.bubbaloop.daemon.v1.CommandResult)
                        return object;
                    let message = new $root.bubbaloop.daemon.v1.CommandResult();
                    if (object.requestId != null)
                        message.requestId = String(object.requestId);
                    if (object.success != null)
                        message.success = Boolean(object.success);
                    if (object.message != null)
                        message.message = String(object.message);
                    if (object.output != null)
                        message.output = String(object.output);
                    if (object.nodeState != null) {
                        if (typeof object.nodeState !== "object")
                            throw TypeError(".bubbaloop.daemon.v1.CommandResult.nodeState: object expected");
                        message.nodeState = $root.bubbaloop.daemon.v1.NodeState.fromObject(object.nodeState);
                    }
                    return message;
                };

                /**
                 * Creates a plain object from a CommandResult message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof bubbaloop.daemon.v1.CommandResult
                 * @static
                 * @param {bubbaloop.daemon.v1.CommandResult} message CommandResult
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                CommandResult.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    let object = {};
                    if (options.defaults) {
                        object.requestId = "";
                        object.success = false;
                        object.message = "";
                        object.output = "";
                        object.nodeState = null;
                    }
                    if (message.requestId != null && message.hasOwnProperty("requestId"))
                        object.requestId = message.requestId;
                    if (message.success != null && message.hasOwnProperty("success"))
                        object.success = message.success;
                    if (message.message != null && message.hasOwnProperty("message"))
                        object.message = message.message;
                    if (message.output != null && message.hasOwnProperty("output"))
                        object.output = message.output;
                    if (message.nodeState != null && message.hasOwnProperty("nodeState"))
                        object.nodeState = $root.bubbaloop.daemon.v1.NodeState.toObject(message.nodeState, options);
                    return object;
                };

                /**
                 * Converts this CommandResult to JSON.
                 * @function toJSON
                 * @memberof bubbaloop.daemon.v1.CommandResult
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                CommandResult.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for CommandResult
                 * @function getTypeUrl
                 * @memberof bubbaloop.daemon.v1.CommandResult
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                CommandResult.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/bubbaloop.daemon.v1.CommandResult";
                };

                return CommandResult;
            })();

            v1.NodeEvent = (function() {

                /**
                 * Properties of a NodeEvent.
                 * @memberof bubbaloop.daemon.v1
                 * @interface INodeEvent
                 * @property {string|null} [eventType] NodeEvent eventType
                 * @property {string|null} [nodeName] NodeEvent nodeName
                 * @property {bubbaloop.daemon.v1.INodeState|null} [state] NodeEvent state
                 * @property {number|Long|null} [timestampMs] NodeEvent timestampMs
                 */

                /**
                 * Constructs a new NodeEvent.
                 * @memberof bubbaloop.daemon.v1
                 * @classdesc Represents a NodeEvent.
                 * @implements INodeEvent
                 * @constructor
                 * @param {bubbaloop.daemon.v1.INodeEvent=} [properties] Properties to set
                 */
                function NodeEvent(properties) {
                    if (properties)
                        for (let keys = Object.keys(properties), i = 0; i < keys.length; ++i)
                            if (properties[keys[i]] != null)
                                this[keys[i]] = properties[keys[i]];
                }

                /**
                 * NodeEvent eventType.
                 * @member {string} eventType
                 * @memberof bubbaloop.daemon.v1.NodeEvent
                 * @instance
                 */
                NodeEvent.prototype.eventType = "";

                /**
                 * NodeEvent nodeName.
                 * @member {string} nodeName
                 * @memberof bubbaloop.daemon.v1.NodeEvent
                 * @instance
                 */
                NodeEvent.prototype.nodeName = "";

                /**
                 * NodeEvent state.
                 * @member {bubbaloop.daemon.v1.INodeState|null|undefined} state
                 * @memberof bubbaloop.daemon.v1.NodeEvent
                 * @instance
                 */
                NodeEvent.prototype.state = null;

                /**
                 * NodeEvent timestampMs.
                 * @member {number|Long} timestampMs
                 * @memberof bubbaloop.daemon.v1.NodeEvent
                 * @instance
                 */
                NodeEvent.prototype.timestampMs = $util.Long ? $util.Long.fromBits(0,0,false) : 0;

                /**
                 * Creates a new NodeEvent instance using the specified properties.
                 * @function create
                 * @memberof bubbaloop.daemon.v1.NodeEvent
                 * @static
                 * @param {bubbaloop.daemon.v1.INodeEvent=} [properties] Properties to set
                 * @returns {bubbaloop.daemon.v1.NodeEvent} NodeEvent instance
                 */
                NodeEvent.create = function create(properties) {
                    return new NodeEvent(properties);
                };

                /**
                 * Encodes the specified NodeEvent message. Does not implicitly {@link bubbaloop.daemon.v1.NodeEvent.verify|verify} messages.
                 * @function encode
                 * @memberof bubbaloop.daemon.v1.NodeEvent
                 * @static
                 * @param {bubbaloop.daemon.v1.INodeEvent} message NodeEvent message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                NodeEvent.encode = function encode(message, writer) {
                    if (!writer)
                        writer = $Writer.create();
                    if (message.eventType != null && Object.hasOwnProperty.call(message, "eventType"))
                        writer.uint32(/* id 1, wireType 2 =*/10).string(message.eventType);
                    if (message.nodeName != null && Object.hasOwnProperty.call(message, "nodeName"))
                        writer.uint32(/* id 2, wireType 2 =*/18).string(message.nodeName);
                    if (message.state != null && Object.hasOwnProperty.call(message, "state"))
                        $root.bubbaloop.daemon.v1.NodeState.encode(message.state, writer.uint32(/* id 3, wireType 2 =*/26).fork()).ldelim();
                    if (message.timestampMs != null && Object.hasOwnProperty.call(message, "timestampMs"))
                        writer.uint32(/* id 4, wireType 0 =*/32).int64(message.timestampMs);
                    return writer;
                };

                /**
                 * Encodes the specified NodeEvent message, length delimited. Does not implicitly {@link bubbaloop.daemon.v1.NodeEvent.verify|verify} messages.
                 * @function encodeDelimited
                 * @memberof bubbaloop.daemon.v1.NodeEvent
                 * @static
                 * @param {bubbaloop.daemon.v1.INodeEvent} message NodeEvent message or plain object to encode
                 * @param {$protobuf.Writer} [writer] Writer to encode to
                 * @returns {$protobuf.Writer} Writer
                 */
                NodeEvent.encodeDelimited = function encodeDelimited(message, writer) {
                    return this.encode(message, writer).ldelim();
                };

                /**
                 * Decodes a NodeEvent message from the specified reader or buffer.
                 * @function decode
                 * @memberof bubbaloop.daemon.v1.NodeEvent
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @param {number} [length] Message length if known beforehand
                 * @returns {bubbaloop.daemon.v1.NodeEvent} NodeEvent
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                NodeEvent.decode = function decode(reader, length) {
                    if (!(reader instanceof $Reader))
                        reader = $Reader.create(reader);
                    let end = length === undefined ? reader.len : reader.pos + length, message = new $root.bubbaloop.daemon.v1.NodeEvent();
                    while (reader.pos < end) {
                        let tag = reader.uint32();
                        if (false)
                            break;
                        switch (tag >>> 3) {
                        case 1: {
                                message.eventType = reader.string();
                                break;
                            }
                        case 2: {
                                message.nodeName = reader.string();
                                break;
                            }
                        case 3: {
                                message.state = $root.bubbaloop.daemon.v1.NodeState.decode(reader, reader.uint32());
                                break;
                            }
                        case 4: {
                                message.timestampMs = reader.int64();
                                break;
                            }
                        default:
                            reader.skipType(tag & 7);
                            break;
                        }
                    }
                    return message;
                };

                /**
                 * Decodes a NodeEvent message from the specified reader or buffer, length delimited.
                 * @function decodeDelimited
                 * @memberof bubbaloop.daemon.v1.NodeEvent
                 * @static
                 * @param {$protobuf.Reader|Uint8Array} reader Reader or buffer to decode from
                 * @returns {bubbaloop.daemon.v1.NodeEvent} NodeEvent
                 * @throws {Error} If the payload is not a reader or valid buffer
                 * @throws {$protobuf.util.ProtocolError} If required fields are missing
                 */
                NodeEvent.decodeDelimited = function decodeDelimited(reader) {
                    if (!(reader instanceof $Reader))
                        reader = new $Reader(reader);
                    return this.decode(reader, reader.uint32());
                };

                /**
                 * Verifies a NodeEvent message.
                 * @function verify
                 * @memberof bubbaloop.daemon.v1.NodeEvent
                 * @static
                 * @param {Object.<string,*>} message Plain object to verify
                 * @returns {string|null} `null` if valid, otherwise the reason why it is not
                 */
                NodeEvent.verify = function verify(message) {
                    if (typeof message !== "object" || message === null)
                        return "object expected";
                    if (message.eventType != null && message.hasOwnProperty("eventType"))
                        if (!$util.isString(message.eventType))
                            return "eventType: string expected";
                    if (message.nodeName != null && message.hasOwnProperty("nodeName"))
                        if (!$util.isString(message.nodeName))
                            return "nodeName: string expected";
                    if (message.state != null && message.hasOwnProperty("state")) {
                        let error = $root.bubbaloop.daemon.v1.NodeState.verify(message.state);
                        if (error)
                            return "state." + error;
                    }
                    if (message.timestampMs != null && message.hasOwnProperty("timestampMs"))
                        if (!$util.isInteger(message.timestampMs) && !(message.timestampMs && $util.isInteger(message.timestampMs.low) && $util.isInteger(message.timestampMs.high)))
                            return "timestampMs: integer|Long expected";
                    return null;
                };

                /**
                 * Creates a NodeEvent message from a plain object. Also converts values to their respective internal types.
                 * @function fromObject
                 * @memberof bubbaloop.daemon.v1.NodeEvent
                 * @static
                 * @param {Object.<string,*>} object Plain object
                 * @returns {bubbaloop.daemon.v1.NodeEvent} NodeEvent
                 */
                NodeEvent.fromObject = function fromObject(object) {
                    if (object instanceof $root.bubbaloop.daemon.v1.NodeEvent)
                        return object;
                    let message = new $root.bubbaloop.daemon.v1.NodeEvent();
                    if (object.eventType != null)
                        message.eventType = String(object.eventType);
                    if (object.nodeName != null)
                        message.nodeName = String(object.nodeName);
                    if (object.state != null) {
                        if (typeof object.state !== "object")
                            throw TypeError(".bubbaloop.daemon.v1.NodeEvent.state: object expected");
                        message.state = $root.bubbaloop.daemon.v1.NodeState.fromObject(object.state);
                    }
                    if (object.timestampMs != null)
                        if ($util.Long)
                            (message.timestampMs = $util.Long.fromValue(object.timestampMs)).unsigned = false;
                        else if (typeof object.timestampMs === "string")
                            message.timestampMs = parseInt(object.timestampMs, 10);
                        else if (typeof object.timestampMs === "number")
                            message.timestampMs = object.timestampMs;
                        else if (typeof object.timestampMs === "object")
                            message.timestampMs = new $util.LongBits(object.timestampMs.low >>> 0, object.timestampMs.high >>> 0).toNumber();
                    return message;
                };

                /**
                 * Creates a plain object from a NodeEvent message. Also converts values to other types if specified.
                 * @function toObject
                 * @memberof bubbaloop.daemon.v1.NodeEvent
                 * @static
                 * @param {bubbaloop.daemon.v1.NodeEvent} message NodeEvent
                 * @param {$protobuf.IConversionOptions} [options] Conversion options
                 * @returns {Object.<string,*>} Plain object
                 */
                NodeEvent.toObject = function toObject(message, options) {
                    if (!options)
                        options = {};
                    let object = {};
                    if (options.defaults) {
                        object.eventType = "";
                        object.nodeName = "";
                        object.state = null;
                        if ($util.Long) {
                            let long = new $util.Long(0, 0, false);
                            object.timestampMs = options.longs === String ? long.toString() : options.longs === Number ? long.toNumber() : long;
                        } else
                            object.timestampMs = options.longs === String ? "0" : 0;
                    }
                    if (message.eventType != null && message.hasOwnProperty("eventType"))
                        object.eventType = message.eventType;
                    if (message.nodeName != null && message.hasOwnProperty("nodeName"))
                        object.nodeName = message.nodeName;
                    if (message.state != null && message.hasOwnProperty("state"))
                        object.state = $root.bubbaloop.daemon.v1.NodeState.toObject(message.state, options);
                    if (message.timestampMs != null && message.hasOwnProperty("timestampMs"))
                        if (typeof message.timestampMs === "number")
                            object.timestampMs = options.longs === String ? String(message.timestampMs) : message.timestampMs;
                        else
                            object.timestampMs = options.longs === String ? $util.Long.prototype.toString.call(message.timestampMs) : options.longs === Number ? new $util.LongBits(message.timestampMs.low >>> 0, message.timestampMs.high >>> 0).toNumber() : message.timestampMs;
                    return object;
                };

                /**
                 * Converts this NodeEvent to JSON.
                 * @function toJSON
                 * @memberof bubbaloop.daemon.v1.NodeEvent
                 * @instance
                 * @returns {Object.<string,*>} JSON object
                 */
                NodeEvent.prototype.toJSON = function toJSON() {
                    return this.constructor.toObject(this, $protobuf.util.toJSONOptions);
                };

                /**
                 * Gets the default type url for NodeEvent
                 * @function getTypeUrl
                 * @memberof bubbaloop.daemon.v1.NodeEvent
                 * @static
                 * @param {string} [typeUrlPrefix] your custom typeUrlPrefix(default "type.googleapis.com")
                 * @returns {string} The default type url
                 */
                NodeEvent.getTypeUrl = function getTypeUrl(typeUrlPrefix) {
                    if (typeUrlPrefix === undefined) {
                        typeUrlPrefix = "type.googleapis.com";
                    }
                    return typeUrlPrefix + "/bubbaloop.daemon.v1.NodeEvent";
                };

                return NodeEvent;
            })();

            return v1;
        })();

        return daemon;
    })();

    return bubbaloop;
})();

export { $root as default };
