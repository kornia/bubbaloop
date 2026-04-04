"""Dynamic protobuf decoding from Zenoh samples.

Workflow:
1. Read the type name from the sample encoding string (``application/protobuf;<TypeName>``).
2. Fetch the node's ``FileDescriptorSet`` from its ``/schema`` queryable (once, then cached).
3. Decode the payload bytes into a Python dict using ``google.protobuf``.

The schema cache is per ``ProtoDecoder`` instance so it can be shared across many
``decode()`` calls without re-fetching or re-parsing the descriptor.
"""

from __future__ import annotations

import json
from typing import Any

import zenoh

try:
    # protobuf >= 4.21
    from google.protobuf.message_factory import GetMessageClass  # type: ignore[attr-defined]

    def _get_proto_class(factory, descriptor):  # type: ignore[no-untyped-def]
        return GetMessageClass(descriptor)

except ImportError:
    # protobuf 3.x
    from google.protobuf import message_factory as _mf

    _shared_factory = _mf.MessageFactory()

    def _get_proto_class(factory, descriptor):  # type: ignore[no-untyped-def]
        return factory.GetPrototype(descriptor)


from google.protobuf import descriptor_pb2, message_factory as _message_factory
from google.protobuf.json_format import MessageToDict


class ProtoDecoder:
    """Fetch schemas on demand and decode protobuf Zenoh samples into dicts.

    Instantiate once per session and reuse across decode calls.

    Example::

        decoder = ProtoDecoder(session)
        sample = await get_sample(session, "bubbaloop/local/host/camera/.../compressed")
        data = decoder.decode(sample)
        # data = {"header": {...}, "format": "h264", "data": "<base64>"}
    """

    def __init__(self, session: zenoh.Session, schema_timeout: float = 3.0) -> None:
        self._session = session
        self._schema_timeout = schema_timeout
        # topic → {type_name: message_class}
        self._class_cache: dict[str, Any] = {}
        # MessageFactory shared across all schemas (pool accumulates all files)
        self._factory = _message_factory.MessageFactory()

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    def decode(self, sample: zenoh.Sample, schema_key: str | None = None) -> dict | None:
        """Decode a protobuf sample into a plain Python dict.

        Args:
            sample: The Zenoh sample with ``application/protobuf;<TypeName>`` encoding.
            schema_key: Override the schema queryable key. If omitted, derived from
                ``sample.key_expr`` by replacing the last segment with ``schema``.

        Returns:
            A dict with snake_case field names, or ``None`` if decoding fails.
        """
        encoding = str(sample.encoding)
        if not encoding.startswith("application/protobuf;"):
            return None

        type_name = encoding.split(";", 1)[1]
        cls = self._get_class(type_name, schema_key or self._schema_key_for(sample))
        if cls is None:
            return None

        msg = cls()
        msg.ParseFromString(bytes(sample.payload))
        return MessageToDict(msg, preserving_proto_field_name=True)

    def prefetch_schema(self, schema_key: str) -> bool:
        """Eagerly fetch and register a schema. Returns True if successful."""
        schema_bytes = self._fetch_schema(schema_key)
        if schema_bytes is None:
            return False
        self._register_schema(schema_bytes)
        return True

    # ------------------------------------------------------------------
    # Internals
    # ------------------------------------------------------------------

    @staticmethod
    def _schema_key_for(sample: zenoh.Sample) -> str:
        """Replace last topic segment with 'schema': a/b/c/metrics → a/b/c/schema."""
        parts = str(sample.key_expr).rsplit("/", 1)
        return parts[0] + "/schema"

    def _get_class(self, type_name: str, schema_key: str) -> Any | None:
        if type_name in self._class_cache:
            return self._class_cache[type_name]

        schema_bytes = self._fetch_schema(schema_key)
        if schema_bytes is None:
            return None

        self._register_schema(schema_bytes)
        return self._class_cache.get(type_name)

    def _fetch_schema(self, schema_key: str) -> bytes | None:
        """Query the schema queryable and return the raw FileDescriptorSet bytes."""
        for reply in self._session.get(schema_key, timeout=self._schema_timeout):
            if reply.ok:
                return bytes(reply.ok.payload)
        return None

    def _register_schema(self, schema_bytes: bytes) -> None:
        """Parse a FileDescriptorSet, add all files to the shared pool, cache classes."""
        fds = descriptor_pb2.FileDescriptorSet()
        fds.ParseFromString(schema_bytes)

        for file_proto in fds.file:
            try:
                self._factory.pool.FindFileByName(file_proto.name)
            except KeyError:
                # Not yet registered — safe to add.
                self._factory.pool.Add(file_proto)

        # Cache message classes for all types in this descriptor set.
        for file_proto in fds.file:
            file_desc = self._factory.pool.FindFileByName(file_proto.name)
            for msg_desc in file_desc.message_types_by_name.values():
                full_name = msg_desc.full_name
                if full_name not in self._class_cache:
                    self._class_cache[full_name] = _get_proto_class(self._factory, msg_desc)
