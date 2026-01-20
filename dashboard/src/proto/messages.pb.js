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

    return bubbaloop;
})();

export { $root as default };
