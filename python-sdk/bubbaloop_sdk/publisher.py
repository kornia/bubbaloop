"""Declared publishers for JSON and protobuf messages.

Each publisher also registers a Zenoh queryable at the same key expression so that
``session.get(topic)`` returns the last published payload. This allows agents to pull
the current value on demand without subscribing to the continuous stream.
"""

import json
import threading

import zenoh


class JsonPublisher:
    """Declared publisher that sets APPLICATION_JSON encoding on every sample.

    Also registers a queryable at the same key so agents can ``get()`` the last value.
    """

    def __init__(
        self,
        declared_publisher: zenoh.Publisher,
        queryable: zenoh.Queryable,
        lock: threading.Lock,
        cache: list,
    ):
        self._pub = declared_publisher
        self._queryable = queryable  # kept alive — undeclared on GC or explicit undeclare()
        self._lock = lock
        self._cache = cache  # list[bytes | None], shared with queryable handler closure

    @classmethod
    def _declare(cls, session: zenoh.Session, topic: str) -> "JsonPublisher":
        pub = session.declare_publisher(topic, encoding=zenoh.Encoding.APPLICATION_JSON)
        lock = threading.Lock()
        cache = [None]  # mutable container shared with the closure below

        def _handler(query: zenoh.Query) -> None:
            with lock:
                data = cache[0]
            if data is not None:
                # query.key_expr is a property, NOT a method call
                query.reply(query.key_expr, data)

        queryable = session.declare_queryable(topic, _handler)
        return cls(pub, queryable, lock, cache)

    def put(self, value) -> None:
        """Publish a JSON-serializable value (dict, list, str, …) and cache it."""
        if isinstance(value, (bytes, bytearray)):
            data = bytes(value)
        elif isinstance(value, str):
            data = value.encode()
        else:
            data = json.dumps(value).encode()
        with self._lock:
            self._cache[0] = data
        self._pub.put(data)

    def undeclare(self) -> None:
        self._pub.undeclare()
        self._queryable.undeclare()


class ProtoPublisher:
    """Declared publisher that sets APPLICATION_PROTOBUF encoding on every sample.

    Also registers a queryable at the same key so agents can ``get()`` the last value.
    """

    def __init__(
        self,
        declared_publisher: zenoh.Publisher,
        queryable: zenoh.Queryable,
        lock: threading.Lock,
        cache: list,
        type_name: str | None,
    ):
        self._pub = declared_publisher
        self._queryable = queryable
        self._lock = lock
        self._cache = cache
        self._type_name = type_name

    @classmethod
    def _declare(cls, session: zenoh.Session, topic: str, type_name: str | None) -> "ProtoPublisher":
        encoding = zenoh.Encoding.APPLICATION_PROTOBUF
        if type_name:
            encoding = encoding.with_schema(type_name)
        pub = session.declare_publisher(topic, encoding=encoding)
        lock = threading.Lock()
        cache = [None]

        def _handler(query: zenoh.Query) -> None:
            with lock:
                data = cache[0]
            if data is not None:
                query.reply(query.key_expr, data)

        queryable = session.declare_queryable(topic, _handler)
        return cls(pub, queryable, lock, cache, type_name)

    def put(self, msg) -> None:
        """Publish a protobuf message or raw bytes and cache it."""
        if hasattr(msg, "SerializeToString"):
            data = msg.SerializeToString()
        elif isinstance(msg, (bytes, bytearray)):
            data = bytes(msg)
        else:
            raise TypeError(f"Expected protobuf message or bytes, got {type(msg).__name__}")
        with self._lock:
            self._cache[0] = data
        self._pub.put(data)

    def undeclare(self) -> None:
        self._pub.undeclare()
        self._queryable.undeclare()
