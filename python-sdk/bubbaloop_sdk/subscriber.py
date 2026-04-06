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
    """Blocking subscriber that yields raw zenoh ``Sample`` objects."""

    def __init__(self, session: zenoh.Session, key_expr: str):
        self._sub = session.declare_subscriber(key_expr)

    def recv(self):
        """Block until the next sample and return it."""
        return self._sub.recv()

    def __iter__(self):
        return self

    def __next__(self):
        try:
            return self.recv()
        except Exception as exc:
            raise StopIteration from exc

    def undeclare(self) -> None:
        self._sub.undeclare()


class ShmSubscriber:
    """Blocking subscriber that yields raw ``bytes`` from Zenoh SHM payloads.

    Counterpart to :class:`ShmPublisher`. The session must have SHM enabled
    (use ``NodeContext.builder().with_shm()``).

    Each call to :meth:`recv` blocks until the next frame arrives and returns
    the raw bytes directly — no encoding inspection, no protobuf decode.

    Usage::

        ctx = NodeContext.builder().with_shm().connect()
        sub = ctx.subscriber_shm("camera/raw")
        for raw_bytes in sub:
            frame = np.frombuffer(raw_bytes, dtype=np.uint8).reshape(h, w, 4)
    """

    def __init__(self, session: zenoh.Session, topic: str):
        self._sub = session.declare_subscriber(topic)

    def recv(self) -> bytes:
        """Block until the next SHM frame arrives and return the raw bytes."""
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
