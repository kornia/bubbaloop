/**
 * Mock for @eclipse-zenoh/zenoh-ts — avoids WASM loading in Node.js test environment.
 *
 * This module is aliased via vitest.config.ts so all imports of
 * '@eclipse-zenoh/zenoh-ts' resolve here during tests.
 */

/** Mirror of the real EncodingPredefined enum — numeric values must match exactly */
export enum EncodingPredefined {
  ZENOH_BYTES = 0,
  ZENOH_STRING = 1,
  ZENOH_SERIALIZED = 2,
  APPLICATION_OCTET_STREAM = 3,
  TEXT_PLAIN = 4,
  APPLICATION_JSON = 5,
  TEXT_JSON = 6,
  APPLICATION_CDR = 7,
  APPLICATION_CBOR = 8,
  APPLICATION_YAML = 9,
  TEXT_YAML = 10,
  TEXT_JSON5 = 11,
  APPLICATION_PYTHON_SERIALIZED_OBJECT = 12,
  APPLICATION_PROTOBUF = 13,
  APPLICATION_JAVA_SERIALIZED_OBJECT = 14,
  APPLICATION_OPENMETRICS_TEXT = 15,
  IMAGE_PNG = 16,
  IMAGE_JPEG = 17,
  IMAGE_GIF = 18,
  IMAGE_BMP = 19,
  IMAGE_WEBP = 20,
  CUSTOM = 65535,
}

/** Minimal Encoding mock with toIdSchema() and withSchema() */
export class Encoding {
  private _id: EncodingPredefined;
  private _schema?: string;

  constructor(id: EncodingPredefined, schema?: string) {
    this._id = id;
    this._schema = schema;
  }

  toIdSchema(): [EncodingPredefined, string?] {
    return [this._id, this._schema];
  }

  withSchema(schema: string): Encoding {
    return new Encoding(this._id, schema);
  }

  toString(): string {
    return `encoding(${this._id}${this._schema ? ';' + this._schema : ''})`;
  }

  static readonly ZENOH_BYTES = new Encoding(EncodingPredefined.ZENOH_BYTES);
  static readonly ZENOH_STRING = new Encoding(EncodingPredefined.ZENOH_STRING);
  static readonly APPLICATION_JSON = new Encoding(EncodingPredefined.APPLICATION_JSON);
  static readonly TEXT_JSON = new Encoding(EncodingPredefined.TEXT_JSON);
  static readonly TEXT_JSON5 = new Encoding(EncodingPredefined.TEXT_JSON5);
  static readonly APPLICATION_PROTOBUF = new Encoding(EncodingPredefined.APPLICATION_PROTOBUF);
  static readonly APPLICATION_OCTET_STREAM = new Encoding(EncodingPredefined.APPLICATION_OCTET_STREAM);
  static default(): Encoding { return Encoding.ZENOH_BYTES; }
  static fromString(_s: string): Encoding { return Encoding.ZENOH_BYTES; }
  static from(input: Encoding | string): Encoding {
    if (typeof input === 'string') return Encoding.fromString(input);
    return input;
  }
}

/** Mock KeyExpr */
export class KeyExpr {
  private _expr: string;
  constructor(expr: string) {
    this._expr = expr;
  }
  toString(): string {
    return this._expr;
  }
}

/** Mock Sample */
export class Sample {
  private _keyexpr: KeyExpr;
  private _payload: Uint8Array;
  private _encoding: Encoding;

  constructor(keyexpr?: string, payload?: Uint8Array, encoding?: Encoding) {
    this._keyexpr = new KeyExpr(keyexpr ?? '');
    this._payload = payload ?? new Uint8Array();
    this._encoding = encoding ?? Encoding.ZENOH_BYTES;
  }

  keyexpr(): KeyExpr {
    return this._keyexpr;
  }

  payload() {
    const data = this._payload;
    return {
      toBytes: () => data,
      to_bytes: () => data,
      deserialize: (s: string) => {
        if (s === 'string') {
          return new TextDecoder().decode(data);
        }
        return data;
      },
    };
  }

  encoding(): Encoding {
    return this._encoding;
  }
}

/** Mock Reply */
export class Reply {
  private _sample: Sample | undefined;
  private _error: ReplyError | undefined;

  constructor(sample?: Sample, error?: ReplyError) {
    this._sample = sample;
    this._error = error;
  }

  result(): Sample {
    if (this._error) throw this._error;
    return this._sample!;
  }
}

/** Mock ReplyError */
export class ReplyError extends Error {
  constructor(message?: string) {
    super(message ?? 'ReplyError');
    this.name = 'ReplyError';
  }
}

/** Mock Subscriber */
export class Subscriber {
  async undeclare(): Promise<void> {}
}

/** Mock IntoZBytes */
export class IntoZBytes {
  private _data: Uint8Array;
  constructor(data?: Uint8Array) {
    this._data = data ?? new Uint8Array();
  }
  to_bytes(): Uint8Array {
    return this._data;
  }
}

/** Mock Config */
export class Config {
  constructor(_endpoint?: string) {}
}

/** Mock Session */
export class Session {
  async declareSubscriber(
    _topic: string,
    _options?: unknown
  ): Promise<Subscriber> {
    return new Subscriber();
  }

  async get(
    _keyexpr: string,
    _options?: unknown
  ): Promise<AsyncIterableIterator<Reply>> {
    return (async function* () {})();
  }

  async put(_keyexpr: string, _payload: unknown): Promise<void> {}

  async close(): Promise<void> {}

  liveliness() {
    return {
      declare_subscriber: async (_topic: string, _options?: unknown) => {
        return new Subscriber();
      },
      declare_token: async (_topic: string) => {
        return { undeclare: async () => {} };
      },
      get: async (_topic: string) => {
        return (async function* () {})();
      },
    };
  }
}

/** Mock open function */
export async function open(_config?: Config): Promise<Session> {
  return new Session();
}
