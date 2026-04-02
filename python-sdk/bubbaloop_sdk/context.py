"""NodeContext — entry point for Bubbaloop nodes.

Mirrors the Rust NodeContext: Zenoh session, scope, machine_id, topic helper,
and factory methods for declared publishers and subscribers.

Note on async: zenoh-python's open() and declare_* methods are synchronous.
connect() runs session creation in a thread executor so callers can use
`await NodeContext.connect()` without blocking the event loop.
"""

import asyncio
import os
import signal
import socket

import zenoh


def _get_hostname() -> str:
    """Return the local hostname sanitized for use in Zenoh topic paths.

    Hyphens are replaced with underscores to match the convention used
    by the Rust daemon (machine_id sanitization).
    """
    return socket.gethostname().replace("-", "_")


class NodeContext:
    """Context provided to nodes by the SDK runtime.

    Mirrors the Rust NodeContext: session, scope, machine_id, topic helper,
    and declared publisher/subscriber factories.

    Typical usage::

        ctx = await NodeContext.connect()
        pub = await ctx.publisher_proto("camera/front/compressed", CompressedImage)

        while not ctx.is_shutdown():
            await pub.put(frame_msg)

        ctx.close()
    """

    def __init__(self, session: zenoh.Session, scope: str, machine_id: str):
        self.session = session
        self.scope = scope
        self.machine_id = machine_id
        self._shutdown_event = asyncio.Event()

        # Register SIGINT/SIGTERM to set the shutdown event.
        # add_signal_handler requires the calling thread to own the event loop
        # (i.e. must be called from the main thread).
        loop = asyncio.get_event_loop()
        for sig in (signal.SIGINT, signal.SIGTERM):
            try:
                loop.add_signal_handler(sig, self._shutdown_event.set)
            except (NotImplementedError, RuntimeError):
                # Windows or non-main-thread — fall back to signal.signal
                signal.signal(sig, lambda s, f: self._shutdown_event.set())

    # ------------------------------------------------------------------
    # Construction
    # ------------------------------------------------------------------

    @classmethod
    async def connect(cls, endpoint: str | None = None) -> "NodeContext":
        """Connect to a Zenoh router and return a ready NodeContext.

        Endpoint resolution order:
        1. ``endpoint`` parameter
        2. ``BUBBALOOP_ZENOH_ENDPOINT`` environment variable
        3. Default ``tcp/127.0.0.1:7447``

        Scope and machine_id are read from ``BUBBALOOP_SCOPE`` and
        ``BUBBALOOP_MACHINE_ID`` environment variables (defaulting to
        ``"local"`` and the sanitized hostname).
        """
        scope = os.environ.get("BUBBALOOP_SCOPE", "local")
        machine_id = os.environ.get("BUBBALOOP_MACHINE_ID", _get_hostname())
        ep = endpoint or os.environ.get(
            "BUBBALOOP_ZENOH_ENDPOINT", "tcp/127.0.0.1:7447"
        )

        # zenoh.open() is synchronous — run in executor to avoid blocking.
        loop = asyncio.get_event_loop()
        session = await loop.run_in_executor(None, lambda: _open_session(ep))
        return cls(session, scope, machine_id)

    # ------------------------------------------------------------------
    # Topic helpers
    # ------------------------------------------------------------------

    def topic(self, suffix: str) -> str:
        """Build a fully-qualified Zenoh topic for this node.

        Pattern: ``bubbaloop/{scope}/{machine_id}/{suffix}``
        """
        return f"bubbaloop/{self.scope}/{self.machine_id}/{suffix}"

    # ------------------------------------------------------------------
    # Shutdown
    # ------------------------------------------------------------------

    def is_shutdown(self) -> bool:
        """Return True if a shutdown signal has been received."""
        return self._shutdown_event.is_set()

    async def wait_shutdown(self) -> None:
        """Suspend until SIGINT/SIGTERM is received."""
        await self._shutdown_event.wait()

    # ------------------------------------------------------------------
    # Publisher factories
    # ------------------------------------------------------------------

    async def publisher_proto(
        self, suffix: str, msg_class=None
    ) -> "ProtoPublisher":
        """Create a declared protobuf publisher.

        Sets ``Encoding.APPLICATION_PROTOBUF`` (with the message type name
        as schema suffix when ``msg_class`` is provided) on every sample so
        the dashboard can decode without schema sniffing.

        ``msg_class`` must be a generated protobuf message class (has a
        ``DESCRIPTOR.full_name`` attribute).
        """
        from .publisher import ProtoPublisher

        topic = self.topic(suffix)
        type_name = (
            msg_class.DESCRIPTOR.full_name
            if msg_class is not None
            else None
        )
        return ProtoPublisher.create(self.session, topic, type_name)

    async def publisher_json(self, suffix: str) -> "JsonPublisher":
        """Create a declared JSON publisher.

        Sets ``Encoding.APPLICATION_JSON`` on every sample.
        """
        from .publisher import JsonPublisher

        topic = self.topic(suffix)
        return JsonPublisher.create(self.session, topic)

    # ------------------------------------------------------------------
    # Subscriber factories
    # ------------------------------------------------------------------

    async def subscriber(
        self, suffix: str, msg_class=None
    ) -> "TypedSubscriber":
        """Create a typed subscriber.

        If ``msg_class`` is provided, each received payload is decoded as an
        instance of that protobuf message class via ``FromString()``.
        The subscriber supports async-for iteration (blocking recv runs in
        a thread executor to avoid blocking the event loop).

        ``suffix`` may contain wildcards (e.g. ``"weather/+/current"``).
        """
        from .subscriber import TypedSubscriber

        topic = self.topic(suffix)
        return TypedSubscriber(self.session, topic, msg_class)

    async def subscriber_raw(self, key_expr: str) -> "RawSubscriber":
        """Create a raw subscriber for the given key expression.

        Exposes zenoh ``Sample`` objects directly — encoding, payload, and
        all metadata are available on each sample.  The key expression is
        used verbatim (no ``topic()`` prefix is applied).
        """
        from .subscriber import RawSubscriber

        return RawSubscriber(self.session, key_expr)

    # ------------------------------------------------------------------
    # Cleanup
    # ------------------------------------------------------------------

    def close(self) -> None:
        """Close the Zenoh session."""
        self.session.close()


# ------------------------------------------------------------------
# Internal helpers
# ------------------------------------------------------------------


def _open_session(endpoint: str) -> zenoh.Session:
    """Open a client-mode Zenoh session connected to ``endpoint``."""
    conf = zenoh.Config()
    # MUST use client mode so messages route through the zenohd router.
    conf.insert_json5("mode", '"client"')
    conf.insert_json5("connect/endpoints", f'["{endpoint}"]')
    # Disable scouting — connect only to the explicit endpoint.
    conf.insert_json5("scouting/multicast/enabled", "false")
    conf.insert_json5("scouting/gossip/enabled", "false")
    return zenoh.open(conf)
