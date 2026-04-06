"""Blocking Zenoh subscribers."""

import zenoh


class TypedSubscriber:
    """Blocking subscriber. Iterates with ``for msg in sub`` (blocks on each recv)."""

    def __init__(self, session: zenoh.Session, topic: str, msg_class=None):
        self._sub = session.declare_subscriber(topic)
        self._msg_class = msg_class

    def recv(self):
        """Block until the next sample arrives and return the decoded message."""
        sample = self._sub.recv()
        payload = bytes(sample.payload.to_bytes())
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
    SHM transport enabled (``NodeContext.builder().with_shm().connect()``), but
    the subscriber works over any Zenoh transport.

    Usage::

        ctx = NodeContext.builder().with_shm().connect()
        sub = ctx.subscriber_raw("camera/raw")
        for raw_bytes in sub:
            frame = np.frombuffer(raw_bytes, dtype=np.uint8).reshape(h, w, 4)
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
