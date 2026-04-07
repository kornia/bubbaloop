"""Blocking Zenoh subscribers."""

import zenoh


class ProtoSubscriber:
    """Blocking subscriber that deserializes protobuf messages.

    Handles the ``bytes(sample.payload)`` copy once per message, then calls
    ``msg_class.FromString`` to decode. Supports both global and local (SHM)
    topics via the topic string passed at construction.

    Usage::

        from camera_pb2 import RawImage
        sub = ctx.subscriber_proto("tapo_terrace/raw", RawImage, local=True)
        for msg in sub:          # msg is a decoded RawImage
            process(msg.data, msg.width, msg.height)
    """

    def __init__(self, session: zenoh.Session, topic: str, msg_class):
        self._sub = session.declare_subscriber(topic)
        self._msg_class = msg_class

    def recv(self):
        """Block until next message, return decoded proto object."""
        sample = self._sub.recv()
        return self._msg_class.FromString(bytes(sample.payload))

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
    """Blocking subscriber with optional proto decoding. Iterates with ``for msg in sub``."""

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
