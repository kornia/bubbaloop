"""Schema-driven protobuf decoder for bubbaloop nodes.

Each bubbaloop node serves a ``FileDescriptorSet`` at ``{instance}/schema`` via
Zenoh queryable.  ``SchemaRegistry`` discovers these on demand and builds dynamic
protobuf message classes so subscribers can decode without importing generated
``_pb2`` files.

Usage::

    registry = SchemaRegistry(session)
    msg = registry.decode(sample)   # returns decoded proto or raw bytes
"""

import json
import logging
import threading

import zenoh
from google.protobuf import descriptor_pb2, descriptor_pool, message_factory

log = logging.getLogger(__name__)

_PROTO_PREFIX = "application/protobuf;"
_JSON_ENCODING = "application/json"


class SchemaRegistry:
    """Discovers node schemas and decodes protobuf samples by encoding type name.

    Lazy: schemas are fetched on first encounter of an unknown type name.
    Thread-safe: a single registry can be shared across subscriber threads.
    """

    def __init__(self, session: zenoh.Session, timeout: float = 2.0):
        self._session = session
        self._timeout = timeout
        self._pool = descriptor_pool.DescriptorPool()
        self._cache: dict[str, type] = {}  # type_name → msg_class
        self._lock = threading.Lock()

    def decode(self, sample: zenoh.Sample) -> object:
        """Decode a sample by its encoding.

        - ``application/protobuf;<TypeName>`` → decoded proto message
        - ``application/json``               → parsed dict
        - anything else                      → raw ``bytes``
        """
        encoding = str(sample.encoding)
        payload = bytes(sample.payload)

        if encoding == _JSON_ENCODING:
            return json.loads(payload)

        if not encoding.startswith(_PROTO_PREFIX):
            return payload

        type_name = encoding[len(_PROTO_PREFIX) :]
        msg_class = self._resolve(type_name)
        if msg_class is None:
            log.debug("SchemaRegistry: no class for %s, returning raw bytes", type_name)
            return payload
        return msg_class.FromString(payload)

    def _resolve(self, type_name: str) -> type | None:
        with self._lock:
            if type_name in self._cache:
                return self._cache[type_name]

        # Schema not cached yet — query all schema queryables.
        self._fetch_all_schemas()

        with self._lock:
            return self._cache.get(type_name)

    def _fetch_all_schemas(self) -> None:
        """Query bubbaloop/**/schema and register every FileDescriptorSet found."""
        try:
            replies = self._session.get("bubbaloop/**/schema", timeout=self._timeout)
        except Exception as e:
            log.warning("SchemaRegistry: schema query failed: %s", e)
            return

        for reply in replies:
            sample = reply.ok
            if sample is None:
                continue
            try:
                self._register(bytes(sample.payload))
            except Exception as e:
                log.debug("SchemaRegistry: failed to register schema: %s", e)

    def _register(self, schema_bytes: bytes) -> None:
        """Parse a FileDescriptorSet and register all message types."""
        fds = descriptor_pb2.FileDescriptorSet.FromString(schema_bytes)

        with self._lock:
            for fd_proto in fds.file:
                try:
                    self._pool.Add(fd_proto)
                except TypeError:
                    pass  # already registered — ignore

            for fd_proto in fds.file:
                pkg = fd_proto.package
                for msg_proto in fd_proto.message_type:
                    full_name = f"{pkg}.{msg_proto.name}" if pkg else msg_proto.name
                    if full_name in self._cache:
                        continue
                    try:
                        desc = self._pool.FindMessageTypeByName(full_name)
                        self._cache[full_name] = _get_proto_class(desc, self._pool)
                        log.debug("SchemaRegistry: registered %s", full_name)
                    except KeyError:
                        pass


def _get_proto_class(descriptor, pool: descriptor_pool.DescriptorPool) -> type:
    """Return a message class for a descriptor, compatible with protobuf 4.x and 5.x."""
    try:
        # protobuf >= 4.21 (upb-based)
        return message_factory.GetMessageClass(descriptor)
    except AttributeError:
        # older protobuf
        factory = message_factory.MessageFactory(pool=pool)  # type: ignore[attr-defined]
        return factory.GetPrototype(descriptor)
