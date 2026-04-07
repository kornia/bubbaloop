"""Blocking Zenoh subscribers."""

import zenoh


class ProtoSubscriber:
    """Blocking subscriber that decodes protobuf automatically from the encoding header.

    No ``_pb2`` imports needed. On each message the encoding string
    (``application/protobuf;<TypeName>``) is used to look up the message class
    in the shared :class:`~bubbaloop_sdk.schema_registry.SchemaRegistry`, which
    fetches ``FileDescriptorSet`` from the publishing node's ``/schema`` queryable
    on first encounter and caches the result.

    Falls back to raw ``bytes`` if the encoding is not protobuf or the schema
    cannot be resolved within the timeout (default 2s).

    Usage::

        sub = ctx.subscriber_proto("tapo_terrace/raw", local=True)
        for msg in sub:   # decoded RawImage — no _pb2 imports needed
            tensor = torch.frombuffer(msg.data, dtype=torch.uint8)
    """

    def __init__(self, session: zenoh.Session, topic: str, registry):
        self._sub = session.declare_subscriber(topic)
        self._registry = registry

    def recv(self):
        """Block until next message and return the decoded proto object."""
        sample = self._sub.recv()
        return self._registry.decode(sample)

    def __iter__(self):
        return self

    def __next__(self):
        try:
            return self.recv()
        except Exception as exc:
            raise StopIteration from exc

    def undeclare(self) -> None:
        self._sub.undeclare()


class TypedSubscriber:
    """Blocking subscriber with optional explicit proto decoding. Iterates with ``for msg in sub``."""

    def __init__(self, session: zenoh.Session, topic: str, msg_class=None):
        self._sub = session.declare_subscriber(topic)
        self._msg_class = msg_class

    def recv(self):
        """Block until the next sample arrives and return the decoded message."""
        sample = self._sub.recv()
        payload = bytes(sample.payload)
        if self._msg_class is not None and hasattr(self._msg_class, "FromString"):
            return self._msg_class.FromString(payload)
        return payload

    def __iter__(self):
        return self

    def __next__(self):
        try:
            return self.recv()
        except Exception as exc:
            raise StopIteration from exc

    def undeclare(self) -> None:
        self._sub.undeclare()


class RawSubscriber:
    """Blocking subscriber that yields raw ``bytes``, counterpart to :class:`RawPublisher`.

    No decoding is applied — the caller owns the byte layout entirely.
    SHM zero-copy delivery is used automatically when both sides have the session
    SHM transport enabled, but the subscriber works over any Zenoh transport.

    Usage::

        sub = ctx.subscriber_raw("camera/raw", local=True)
        for raw_bytes in sub:
            tensor = torch.frombuffer(raw_bytes, dtype=torch.uint8)
    """

    def __init__(self, session: zenoh.Session, topic: str):
        self._sub = session.declare_subscriber(topic)

    def recv(self) -> bytes:
        """Block until the next frame arrives and return the raw bytes."""
        sample = self._sub.recv()
        return bytes(sample.payload)

    def __iter__(self):
        return self

    def __next__(self):
        try:
            return self.recv()
        except Exception as exc:
            raise StopIteration from exc

    def undeclare(self) -> None:
        self._sub.undeclare()
