/**
 * Mock for @eclipse-zenoh/zenoh-ts — avoids WASM loading in Node.js test environment.
 *
 * This module is aliased via vitest.config.ts so all imports of
 * '@eclipse-zenoh/zenoh-ts' resolve here during tests.
 */

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

  constructor(keyexpr?: string, payload?: Uint8Array) {
    this._keyexpr = new KeyExpr(keyexpr ?? '');
    this._payload = payload ?? new Uint8Array();
  }

  keyexpr(): KeyExpr {
    return this._keyexpr;
  }

  payload() {
    const data = this._payload;
    return {
      to_bytes: () => data,
      deserialize: (s: string) => {
        if (s === 'string') {
          return new TextDecoder().decode(data);
        }
        return data;
      },
    };
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
}

/** Mock open function */
export async function open(_config?: Config): Promise<Session> {
  return new Session();
}
