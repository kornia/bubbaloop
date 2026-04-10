"""Blocking Zenoh subscribers."""

import zenoh


class _BaseSubscriber:
    """Shared iterator protocol and cleanup for all subscriber types."""

    def __init__(self, session: zenoh.Session, topic: str):
        self._sub = session.declare_subscriber(topic)
        self._undeclared = False

    def recv(self):
        raise NotImplementedError

    def __iter__(self):
        return self

    def __next__(self):
        try:
            return self.recv()
        except Exception as exc:
            raise StopIteration from exc

    def undeclare(self) -> None:
        """Release the underlying Zenoh subscriber. Idempotent.

        Double-close is common during shutdown (explicit call + context-manager
        exit, or test teardown racing with ``__del__``). We guard with a flag so
        a legitimate first-call ``RuntimeError`` from Zenoh still propagates —
        only repeat calls become no-ops.
        """
        if self._undeclared:
            return
        self._undeclared = True
        self._sub.undeclare()


class ProtoSubscriber(_BaseSubscriber):
    """Blocking subscriber that decodes protobuf automatically from the encoding header.

    No ``_pb2`` imports needed. On each message the encoding string
    (``application/protobuf;<TypeName>``) is used to look up the message class
    in the shared :class:`~bubbaloop_sdk.schema_registry.SchemaRegistry`, which
    fetches ``FileDescriptorSet`` from the publishing node's ``/schema`` queryable
    on first encounter and caches the result.

    Falls back to raw ``bytes`` if the encoding is not protobuf or the schema
    cannot be resolved within the timeout (default 2s).

    Usage::

        sub = ctx.subscribe("tapo_terrace/raw", local=True)
        for msg in sub:   # decoded RawImage — no _pb2 imports needed
            tensor = torch.frombuffer(msg.data, dtype=torch.uint8)
    """

    def __init__(self, session: zenoh.Session, topic: str, registry):
        super().__init__(session, topic)
        self._registry = registry

    def recv(self):
        """Block until next message and return the decoded proto object."""
        sample = self._sub.recv()
        return self._registry.decode(sample)


class RawSubscriber(_BaseSubscriber):
    """Blocking subscriber that yields raw ``bytes``, counterpart to :class:`RawPublisher`.

    No decoding is applied — the caller owns the byte layout entirely.
    SHM zero-copy delivery is used automatically when both sides have the session
    SHM transport enabled, but the subscriber works over any Zenoh transport.

    Usage::

        sub = ctx.subscribe_raw("camera/raw", local=True)
        for raw_bytes in sub:
            tensor = torch.frombuffer(raw_bytes, dtype=torch.uint8)
    """

    def recv(self) -> bytes:
        """Block until the next frame arrives and return the raw bytes."""
        sample = self._sub.recv()
        return bytes(sample.payload)
