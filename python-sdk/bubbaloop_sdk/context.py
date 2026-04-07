"""NodeContext — entry point for Bubbaloop Python nodes.

Synchronous API — no asyncio required. zenoh-python is blocking by design;
this SDK wraps it without adding async complexity.

Usage::

    ctx = NodeContext.connect()
    pub = ctx.publisher_json("weather/current")
    while not ctx.is_shutdown():
        pub.put({"temperature": 22.5})
        time.sleep(30)
    ctx.close()

SHM transport is always enabled — all publishers and subscribers on the
session benefit from zero-copy delivery automatically when both sides are on
the same machine. Use ``publisher_raw_local`` / ``subscriber_raw_local`` for
data that must stay machine-local (e.g. raw RGBA frames from camera to
detector) — these topics are outside ``bubbaloop/**`` and never cross the
WebSocket bridge.
"""

import os
import signal
import socket
import threading

import zenoh


def _hostname() -> str:
    return socket.gethostname().replace("-", "_")


class NodeContext:
    """Zenoh session + scope/machine_id + shutdown signal for a bubbaloop node.

    Create with :meth:`connect`. Cleanup with :meth:`close` (or use as a
    context manager).

    SHM transport is always enabled on the session. All publishers and
    subscribers benefit from zero-copy delivery automatically when both
    sides are on the same machine.
    """

    def __init__(self, session: zenoh.Session, machine_id: str, instance_name: str):
        self.session = session
        self.machine_id = machine_id
        self.instance_name = instance_name
        self._shutdown = threading.Event()
        for sig in (signal.SIGINT, signal.SIGTERM):
            signal.signal(sig, lambda s, f: self._shutdown.set())

    @classmethod
    def connect(
        cls,
        endpoint: str | None = None,
        instance_name: str | None = None,
    ) -> "NodeContext":
        """Connect to a Zenoh router and return a ready NodeContext.

        Endpoint resolution: ``endpoint`` arg → ``BUBBALOOP_ZENOH_ENDPOINT`` env
        → ``tcp/127.0.0.1:7447``.

        ``instance_name`` is used for health and schema topics. Pass the ``name``
        field from your config so multi-instance deployments don't collide.
        Falls back to the hostname.
        """
        machine_id = os.environ.get("BUBBALOOP_MACHINE_ID", _hostname())
        ep = endpoint or os.environ.get("BUBBALOOP_ZENOH_ENDPOINT", "tcp/127.0.0.1:7447")
        name = instance_name or machine_id

        conf = zenoh.Config()
        conf.insert_json5("mode", '"client"')
        conf.insert_json5("connect/endpoints", f'["{ep}"]')
        conf.insert_json5("scouting/multicast/enabled", "false")
        conf.insert_json5("scouting/gossip/enabled", "false")
        conf.insert_json5("transport/shared_memory/enabled", "true")
        session = zenoh.open(conf)

        return cls(session, machine_id, name)

    # ------------------------------------------------------------------
    # Topic helpers
    # ------------------------------------------------------------------

    def topic(self, suffix: str) -> str:
        """Return ``bubbaloop/global/{machine_id}/{suffix}``."""
        return f"bubbaloop/global/{self.machine_id}/{suffix}"

    def local_topic(self, suffix: str) -> str:
        """Return ``bubbaloop/local/{machine_id}/{suffix}``.

        SHM-only — never crosses the WebSocket bridge. Use for large binary payloads
        consumed only by processes on the same machine (e.g. raw RGBA camera frames).
        """
        return f"bubbaloop/local/{self.machine_id}/{suffix}"

    # ------------------------------------------------------------------
    # Shutdown
    # ------------------------------------------------------------------

    def is_shutdown(self) -> bool:
        """True if SIGINT/SIGTERM has been received."""
        return self._shutdown.is_set()

    def wait_shutdown(self) -> None:
        """Block until SIGINT/SIGTERM is received."""
        self._shutdown.wait()

    # ------------------------------------------------------------------
    # Publishers
    # ------------------------------------------------------------------

    def publisher_json(self, suffix: str) -> "JsonPublisher":
        """Declare a JSON publisher at ``topic(suffix)``."""
        from .publisher import JsonPublisher
        return JsonPublisher._declare(self.session, self.topic(suffix))

    def publisher_proto(self, suffix: str, msg_class=None) -> "ProtoPublisher":
        """Declare a protobuf publisher at ``topic(suffix)``."""
        from .publisher import ProtoPublisher
        type_name = msg_class.DESCRIPTOR.full_name if msg_class is not None else None
        return ProtoPublisher._declare(self.session, self.topic(suffix), type_name)

    def publisher_raw(self, suffix: str, local: bool = False) -> "RawPublisher":
        """Declare a raw publisher with no encoding.

        When ``local=True``, publishes to ``local/{machine_id}/{suffix}`` with SHM-specific
        settings: ``congestion_control=Block`` so the publisher waits for the subscriber to
        release the SHM buffer instead of silently dropping frames. Never crosses the bridge.

        When ``local=False`` (default), publishes to ``bubbaloop/{scope}/{machine_id}/{suffix}``.
        """
        from .publisher import RawPublisher
        key = self.local_topic(suffix) if local else self.topic(suffix)
        return RawPublisher._declare(self.session, key, local=local)

    # ------------------------------------------------------------------
    # Subscribers
    # ------------------------------------------------------------------

    def subscriber_proto(self, suffix: str, msg_class, local: bool = False) -> "ProtoSubscriber":
        """Declare a protobuf subscriber that deserializes each message automatically.

        When ``local=True``, subscribes to the SHM-only local topic — use this to
        receive ``RawImage`` frames published by the camera node over shared memory.

        Usage::

            from camera_pb2 import RawImage
            sub = ctx.subscriber_proto("tapo_terrace/raw", RawImage, local=True)
            for msg in sub:   # msg is a decoded RawImage
                tensor = torch.frombuffer(msg.data, dtype=torch.uint8)
        """
        from .subscriber import ProtoSubscriber
        key = self.local_topic(suffix) if local else self.topic(suffix)
        return ProtoSubscriber(self.session, key, msg_class)

    def subscriber(self, suffix: str, msg_class=None) -> "TypedSubscriber":
        """Declare a typed subscriber. Blocks on ``recv()``."""
        from .subscriber import TypedSubscriber
        return TypedSubscriber(self.session, self.topic(suffix), msg_class)

    def subscriber_raw(self, suffix: str, local: bool = False) -> "RawSubscriber":
        """Declare a raw subscriber that yields ``bytes`` with no decoding.

        When ``local=True``, subscribes to ``local/{machine_id}/{suffix}`` — SHM zero-copy,
        machine-local only. Counterpart to ``publisher_raw(suffix, local=True)``.

        When ``local=False`` (default), subscribes to ``bubbaloop/{scope}/{machine_id}/{suffix}``.
        """
        from .subscriber import RawSubscriber
        key = self.local_topic(suffix) if local else self.topic(suffix)
        return RawSubscriber(self.session, key)

    # ------------------------------------------------------------------
    # Cleanup
    # ------------------------------------------------------------------

    def close(self) -> None:
        """Close the Zenoh session."""
        self.session.close()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()
