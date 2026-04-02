"""Declared publishers for protobuf and JSON messages.

Both classes wrap a zenoh.Publisher declared once at creation time with the
appropriate Encoding.  Encoding is carried on every sample so the dashboard
(and any subscriber) can decode without sniffing.
"""

import json

import zenoh


class ProtoPublisher:
    """Declared publisher for protobuf messages.

    Encoding: ``APPLICATION_PROTOBUF`` (with schema suffix = message type name
    when the type name is known).

    Do not construct directly — use ``NodeContext.publisher_proto()``.
    """

    def __init__(self, publisher: zenoh.Publisher, type_name: str | None):
        self._publisher = publisher
        self._type_name = type_name

    @classmethod
    def create(
        cls,
        session: zenoh.Session,
        topic: str,
        type_name: str | None,
    ) -> "ProtoPublisher":
        """Declare a publisher with protobuf encoding.

        ``type_name`` is appended as the encoding schema suffix
        (e.g. ``"application/protobuf;bubbaloop.camera.v1.CompressedImage"``).
        """
        encoding = zenoh.Encoding.APPLICATION_PROTOBUF
        if type_name:
            encoding = encoding.with_schema(type_name)

        publisher = session.declare_publisher(topic, encoding=encoding)
        return cls(publisher, type_name)

    async def put(self, msg) -> None:
        """Publish a protobuf message or raw bytes.

        Accepts a protobuf message instance (calls ``SerializeToString()``)
        or raw ``bytes``.
        """
        if hasattr(msg, "SerializeToString"):
            data = msg.SerializeToString()
        elif isinstance(msg, (bytes, bytearray)):
            data = bytes(msg)
        else:
            raise TypeError(
                f"Expected a protobuf message or bytes, got {type(msg).__name__}"
            )
        self._publisher.put(data)

    def undeclare(self) -> None:
        """Undeclare the underlying Zenoh publisher."""
        self._publisher.undeclare()


class JsonPublisher:
    """Declared publisher for JSON-serializable values.

    Encoding: ``APPLICATION_JSON``.

    Do not construct directly — use ``NodeContext.publisher_json()``.
    """

    def __init__(self, publisher: zenoh.Publisher):
        self._publisher = publisher

    @classmethod
    def create(cls, session: zenoh.Session, topic: str) -> "JsonPublisher":
        """Declare a publisher with JSON encoding."""
        publisher = session.declare_publisher(
            topic, encoding=zenoh.Encoding.APPLICATION_JSON
        )
        return cls(publisher)

    async def put(self, value) -> None:
        """Publish a JSON-serializable value.

        Accepts dicts, lists, strings, or anything JSON-serializable.
        Raw ``bytes`` are published as-is.
        """
        if isinstance(value, (bytes, bytearray)):
            data = bytes(value)
        elif isinstance(value, str):
            data = value.encode()
        else:
            data = json.dumps(value).encode()
        self._publisher.put(data)

    def undeclare(self) -> None:
        """Undeclare the underlying Zenoh publisher."""
        self._publisher.undeclare()
