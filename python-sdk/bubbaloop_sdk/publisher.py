"""Declared publishers for JSON and protobuf messages."""

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
