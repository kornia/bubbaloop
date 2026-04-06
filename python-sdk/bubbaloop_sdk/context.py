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

For nodes that publish/subscribe via SHM, use the builder::

    ctx = NodeContext.builder().with_shm().connect(endpoint=ep, instance_name=name)
"""

import os
import signal
import socket
import threading

import zenoh


def _hostname() -> str:
    return socket.gethostname().replace("-", "_")


class NodeContextBuilder:
    """Fluent builder for NodeContext.

    Usage::

        ctx = NodeContext.builder().with_shm().connect(endpoint=ep, instance_name=name)
    """

    def __init__(self) -> None:
        self._shm = False

    def with_shm(self) -> "NodeContextBuilder":
        """Enable Zenoh SHM transport for zero-copy same-machine delivery.

        When enabled, Zenoh automatically uses shared memory for any publisher/subscriber
        pair running on the same machine — this applies to JSON, protobuf, and raw
        publishers alike. Falls back to normal transport for cross-machine communication.
        """
        self._shm = True
        return self

    def connect(
        self,
        endpoint: str | None = None,
        instance_name: str | None = None,
    ) -> "NodeContext":
        """Build and connect the NodeContext with the configured options."""
        return NodeContext.connect(
            endpoint=endpoint,
            instance_name=instance_name,
            shm=self._shm,
        )


class NodeContext:
    """Zenoh session + scope/machine_id + shutdown signal for a bubbaloop node.

    Create with :meth:`connect` or :meth:`builder`. Cleanup with :meth:`close`
    (or use as a context manager).
    """

    def __init__(self, session: zenoh.Session, scope: str, machine_id: str, instance_name: str):
        self.session = session
        self.scope = scope
        self.machine_id = machine_id
        self.instance_name = instance_name
        self._shutdown = threading.Event()
        for sig in (signal.SIGINT, signal.SIGTERM):
            signal.signal(sig, lambda s, f: self._shutdown.set())

    @classmethod
    def builder(cls) -> NodeContextBuilder:
        """Return a fluent builder for creating a NodeContext with custom options."""
        return NodeContextBuilder()

    @classmethod
    def connect(
        cls,
        endpoint: str | None = None,
        instance_name: str | None = None,
        shm: bool = False,
    ) -> "NodeContext":
        """Connect to a Zenoh router and return a ready NodeContext.

        Endpoint resolution: ``endpoint`` arg → ``BUBBALOOP_ZENOH_ENDPOINT`` env
        → ``tcp/127.0.0.1:7447``.

        ``instance_name`` is used for health and schema topics. Pass the ``name``
        field from your config so multi-instance deployments don't collide.
        Falls back to the hostname.

        Prefer :meth:`builder` when SHM transport is needed::

            ctx = NodeContext.builder().with_shm().connect(...)
        """
        scope = os.environ.get("BUBBALOOP_SCOPE", "local")
        machine_id = os.environ.get("BUBBALOOP_MACHINE_ID", _hostname())
        ep = endpoint or os.environ.get("BUBBALOOP_ZENOH_ENDPOINT", "tcp/127.0.0.1:7447")
        name = instance_name or machine_id

        conf = zenoh.Config()
        conf.insert_json5("mode", '"client"')
        conf.insert_json5("connect/endpoints", f'["{ep}"]')
        conf.insert_json5("scouting/multicast/enabled", "false")
        conf.insert_json5("scouting/gossip/enabled", "false")
        if shm:
            conf.insert_json5("transport/shared_memory/enabled", "true")
        session = zenoh.open(conf)

        return cls(session, scope, machine_id, name)

    # ------------------------------------------------------------------
    # Topic helper
    # ------------------------------------------------------------------

    def topic(self, suffix: str) -> str:
        """Return ``bubbaloop/{scope}/{machine_id}/{suffix}``."""
        return f"bubbaloop/{self.scope}/{self.machine_id}/{suffix}"

    def local_topic(self, suffix: str) -> str:
        """Return ``local/{machine_id}/{suffix}``.

        Use this for data that must stay on the same machine (e.g. SHM raw frames).
        These topics are NOT under ``bubbaloop/**`` and will never cross the WebSocket bridge.
        """
        return f"local/{self.machine_id}/{suffix}"

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

    def publisher_raw(self, suffix: str) -> "RawPublisher":
        """Declare a raw publisher at ``topic(suffix)`` that sends bytes with no encoding.

        The caller owns the byte layout. SHM zero-copy is used automatically
        when the session has it enabled and the subscriber is on the same machine.
        """
        from .publisher import RawPublisher
        return RawPublisher._declare(self.session, self.topic(suffix))

    def publisher_raw_local(self, suffix: str) -> "RawPublisher":
        """Declare a raw publisher at ``local_topic(suffix)``.

        Identical to :meth:`publisher_raw` but uses :meth:`local_topic`.
        Use this for SHM frame data that must never cross the WebSocket bridge.
        """
        from .publisher import RawPublisher
        return RawPublisher._declare(self.session, self.local_topic(suffix))

    # ------------------------------------------------------------------
    # Subscribers
    # ------------------------------------------------------------------

    def subscriber(self, suffix: str, msg_class=None) -> "TypedSubscriber":
        """Declare a typed subscriber. Blocks on ``recv()``."""
        from .subscriber import TypedSubscriber
        return TypedSubscriber(self.session, self.topic(suffix), msg_class)

    def subscriber_raw(self, suffix: str) -> "RawSubscriber":
        """Declare a raw subscriber at ``topic(suffix)`` that yields ``bytes`` with no decoding.

        Counterpart to :meth:`publisher_raw`. The caller decodes the bytes.
        SHM zero-copy is used automatically when the session has it enabled.
        """
        from .subscriber import RawSubscriber
        return RawSubscriber(self.session, self.topic(suffix))

    def subscriber_raw_local(self, suffix: str) -> "RawSubscriber":
        """Declare a raw subscriber at ``local_topic(suffix)``.

        Counterpart to :meth:`publisher_raw_local`. Use for SHM frame data
        that never crosses the WebSocket bridge.
        """
        from .subscriber import RawSubscriber
        return RawSubscriber(self.session, self.local_topic(suffix))

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
