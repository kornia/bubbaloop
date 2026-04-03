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
        scope = os.environ.get("BUBBALOOP_SCOPE", "local")
        machine_id = os.environ.get("BUBBALOOP_MACHINE_ID", _hostname())
        ep = endpoint or os.environ.get("BUBBALOOP_ZENOH_ENDPOINT", "tcp/127.0.0.1:7447")
        name = instance_name or machine_id

        conf = zenoh.Config()
        conf.insert_json5("mode", '"client"')
        conf.insert_json5("connect/endpoints", f'["{ep}"]')
        conf.insert_json5("scouting/multicast/enabled", "false")
        conf.insert_json5("scouting/gossip/enabled", "false")
        session = zenoh.open(conf)

        return cls(session, scope, machine_id, name)

    # ------------------------------------------------------------------
    # Topic helper
    # ------------------------------------------------------------------

    def topic(self, suffix: str) -> str:
        """Return ``bubbaloop/{scope}/{machine_id}/{suffix}``."""
        return f"bubbaloop/{self.scope}/{self.machine_id}/{suffix}"

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

    # ------------------------------------------------------------------
    # Subscribers
    # ------------------------------------------------------------------

    def subscriber(self, suffix: str, msg_class=None) -> "TypedSubscriber":
        """Declare a typed subscriber. Blocks on ``recv()``."""
        from .subscriber import TypedSubscriber
        return TypedSubscriber(self.session, self.topic(suffix), msg_class)

    def subscriber_raw(self, key_expr: str) -> "RawSubscriber":
        """Declare a raw subscriber with a literal key expression."""
        from .subscriber import RawSubscriber
        return RawSubscriber(self.session, key_expr)

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
