"""Declared publishers for JSON, protobuf, and SHM messages."""

import json

import zenoh


class JsonPublisher:
    """Declared publisher that sets APPLICATION_JSON encoding on every sample."""

    def __init__(self, declared_publisher: zenoh.Publisher):
        self._pub = declared_publisher

    @classmethod
    def _declare(cls, session: zenoh.Session, topic: str) -> "JsonPublisher":
        pub = session.declare_publisher(topic, encoding=zenoh.Encoding.APPLICATION_JSON)
        return cls(pub)

    def put(self, value) -> None:
        """Publish a JSON-serializable value (dict, list, str, …)."""
        if isinstance(value, (bytes, bytearray)):
            data = bytes(value)
        elif isinstance(value, str):
            data = value.encode()
        else:
            data = json.dumps(value).encode()
        self._pub.put(data)

    def undeclare(self) -> None:
        self._pub.undeclare()


class ProtoPublisher:
    """Declared publisher that sets APPLICATION_PROTOBUF encoding on every sample."""

    def __init__(self, declared_publisher: zenoh.Publisher, type_name: str | None):
        self._pub = declared_publisher
        self._type_name = type_name

    @classmethod
    def _declare(cls, session: zenoh.Session, topic: str, type_name: str | None) -> "ProtoPublisher":
        encoding = zenoh.Encoding.APPLICATION_PROTOBUF
        if type_name:
            encoding = encoding.with_schema(type_name)
        pub = session.declare_publisher(topic, encoding=encoding)
        return cls(pub, type_name)

    def put(self, msg) -> None:
        """Publish a protobuf message or raw bytes."""
        if hasattr(msg, "SerializeToString"):
            data = msg.SerializeToString()
        elif isinstance(msg, (bytes, bytearray)):
            data = bytes(msg)
        else:
            raise TypeError(f"Expected protobuf message or bytes, got {type(msg).__name__}")
        self._pub.put(data)

    def undeclare(self) -> None:
        self._pub.undeclare()


class RawPublisher:
    """Declared publisher for zero-copy same-machine delivery via Zenoh SHM.

    Publishes raw ``bytes`` or ``bytearray`` payloads with no encoding overhead.
    The session must have SHM enabled (use ``NodeContext.builder().with_shm()``).

    Usage::

        ctx = NodeContext.builder().with_shm().connect()
        pub = ctx.publisher_raw("camera/raw")
        pub.put(rgba_bytes)  # delivered zero-copy to same-machine subscribers
    """

    def __init__(self, declared_publisher: zenoh.Publisher):
        self._pub = declared_publisher

    @classmethod
    def _declare(cls, session: zenoh.Session, topic: str) -> "RawPublisher":
        pub = session.declare_publisher(topic)
        return cls(pub)

    def put(self, data: bytes | bytearray) -> None:
        """Publish raw bytes over Zenoh SHM."""
        self._pub.put(bytes(data))

    def undeclare(self) -> None:
        self._pub.undeclare()
