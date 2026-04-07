"""NodeContext — entry point for Bubbaloop Python nodes.

Synchronous API — no asyncio required. zenoh-python is blocking by design;
this SDK wraps it without adding async complexity.

Usage::

    ctx = NodeContext.connect()
    pub = ctx.publisher_json("weather/current")
    sub = ctx.subscribe("other_node/data")
    while not ctx.is_shutdown():
        pub.put({"temperature": 22.5})
        msg = sub.recv()   # auto-decoded: dict, proto, or bytes
"""

from __future__ import annotations

import os
import signal
import socket
import threading
from typing import TYPE_CHECKING

import zenoh

if TYPE_CHECKING:
    from .publisher import JsonPublisher, ProtoPublisher, RawPublisher
    from .subscriber import (
        AsyncQueryable,
        CallbackSubscriber,
        CallbackSubscriberAsync,
        ProtoSubscriber,
        RawCallbackSubscriber,
        RawCallbackSubscriberAsync,
        RawSubscriber,
        TypedSubscriber,
    )


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
    ) -> NodeContext:
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

        SHM-only — never crosses the WebSocket bridge. Use for large binary
        payloads consumed only by processes on the same machine (e.g. raw RGBA
        camera frames).
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

    def publisher_json(self, suffix: str) -> JsonPublisher:
        """Declare a JSON publisher at ``topic(suffix)``."""
        from .publisher import JsonPublisher

        return JsonPublisher._declare(self.session, self.topic(suffix))

    def publisher_proto(self, suffix: str, msg_class=None) -> ProtoPublisher:
        """Declare a protobuf publisher at ``topic(suffix)``."""
        from .publisher import ProtoPublisher

        type_name = msg_class.DESCRIPTOR.full_name if msg_class is not None else None
        return ProtoPublisher._declare(self.session, self.topic(suffix), type_name)

    def publisher_raw(self, suffix: str, local: bool = False) -> RawPublisher:
        """Declare a raw publisher with no encoding.

        When ``local=True``, publishes to ``local/{machine_id}/{suffix}`` with
        ``congestion_control=Block`` — waits for the subscriber to release the
        SHM buffer instead of dropping frames. Never crosses the bridge.
        """
        from .publisher import RawPublisher

        key = self.local_topic(suffix) if local else self.topic(suffix)
        return RawPublisher._declare(self.session, key, local=local)

    # ------------------------------------------------------------------
    # Subscribers
    # ------------------------------------------------------------------

    def subscriber(self, suffix: str, msg_class=None) -> TypedSubscriber:
        """Declare a typed subscriber. Blocks on ``recv()``."""
        from .subscriber import TypedSubscriber

        return TypedSubscriber(self.session, self.topic(suffix), msg_class)

    def subscriber_raw(self, key_expr: str) -> RawSubscriber:
        """Declare a raw subscriber with a literal key expression."""
        from .subscriber import RawSubscriber

        return RawSubscriber(self.session, key_expr)

    # NOTE: next 2 functions are new. Talk to Edgar to understand them.

    def subscribe(self, suffix: str, local: bool = False) -> ProtoSubscriber:
        """Declare a subscriber that auto-decodes every message by its encoding.

        - ``application/protobuf;<TypeName>`` → decoded proto object (schema fetched on demand)
        - ``application/json``               → parsed ``dict``
        - anything else                      → raw ``bytes``

        When ``local=True``, subscribes to the SHM-only local topic
        (``bubbaloop/local/{machine_id}/{suffix}``) — use this to receive frames
        from the camera node without crossing the WebSocket bridge.

        Usage::

            sub = ctx.subscribe("tapo_terrace/raw", local=True)
            for msg in sub:   # RawImage decoded automatically
                tensor = torch.frombuffer(msg.data, dtype=torch.uint8)

            sub = ctx.subscribe("openmeteo/weather")
            for msg in sub:   # dict
                print(msg["temperature"])
        """
        from .schema_registry import SchemaRegistry
        from .subscriber import ProtoSubscriber

        if not hasattr(self, "_schema_registry"):
            self._schema_registry = SchemaRegistry(self.session)
        key = self.local_topic(suffix) if local else self.topic(suffix)
        return ProtoSubscriber(self.session, key, self._schema_registry)

    def subscribe_raw(self, suffix: str, local: bool = False) -> RawSubscriber:
        """Declare a subscriber that yields raw ``bytes`` with no decoding.

        Use when you need direct access to the payload — e.g. to pass to
        ``torch.frombuffer`` without an intermediate proto decode.

        When ``local=True``, subscribes to the SHM-only local topic.
        """
        from .subscriber import RawSubscriber

        key = self.local_topic(suffix) if local else self.topic(suffix)
        return RawSubscriber(self.session, key)

    # ------------------------------------------------------------------
    # Callback Subscribers
    # ------------------------------------------------------------------

    def subscriber_callback(self, suffix: str, handler, msg_class=None) -> CallbackSubscriber:
        """Callback subscriber at ``topic(suffix)``.

        ``handler`` is called from Zenoh's internal thread each time a sample
        arrives. For slow handlers (I/O, DB), use ``subscriber_callback_async()``.
        """
        from .subscriber import CallbackSubscriber

        return CallbackSubscriber(self.session, self.topic(suffix), handler, msg_class)

    def subscriber_raw_callback(self, key_expr: str, handler) -> RawCallbackSubscriber:
        """Callback subscriber at a literal key expression.

        ``handler`` receives raw ``zenoh.Sample`` objects from Zenoh's internal thread.
        """
        from .subscriber import RawCallbackSubscriber

        return RawCallbackSubscriber(self.session, key_expr, handler)

    def subscriber_callback_async(
        self, suffix: str, handler, msg_class=None, max_workers: int = 4
    ) -> CallbackSubscriberAsync:
        """Callback subscriber at ``topic(suffix)`` with handler in a thread pool.

        Use when ``handler`` does slow work (database writes, hardware I/O, network
        calls). Zenoh's internal thread is freed immediately; the handler runs in a
        ``ThreadPoolExecutor`` with ``max_workers`` threads.
        """
        from .subscriber import CallbackSubscriberAsync

        return CallbackSubscriberAsync(self.session, self.topic(suffix), handler, msg_class, max_workers)

    def subscriber_raw_callback_async(self, key_expr: str, handler, max_workers: int = 4) -> RawCallbackSubscriberAsync:
        """Raw callback subscriber at a literal key expression with handler in a thread pool."""
        from .subscriber import RawCallbackSubscriberAsync

        return RawCallbackSubscriberAsync(self.session, key_expr, handler, max_workers)

    # ------------------------------------------------------------------
    # Queryables
    # ------------------------------------------------------------------

    def queryable(self, suffix: str, handler) -> zenoh.Queryable:
        """Declare a queryable at ``topic(suffix)``.

        ``handler`` receives a ``zenoh.Query``. Use the standard zenoh API to reply::

            def on_command(query: zenoh.Query) -> None:
                result = process(query.payload.to_string())
                query.reply(query.key_expr, json.dumps(result).encode())

            qbl = ctx.queryable("command", on_command)

        **Important:** do NOT pass ``complete=True`` — it blocks wildcard queries
        like ``bubbaloop/**/schema`` used by the dashboard.

        For slow handlers, use ``queryable_async()``.

        Call ``undeclare()`` on the returned queryable when done.
        """
        return self.session.declare_queryable(self.topic(suffix), handler)

    def queryable_raw(self, key_expr: str, handler) -> zenoh.Queryable:
        """Declare a queryable at a literal key expression (no topic prefix).

        Use for wildcard queryables or when the ``bubbaloop/{scope}/{machine_id}/``
        prefix does not apply (e.g. ``bubbaloop/**/schema`` for multi-schema serving).

        Call ``undeclare()`` on the returned queryable when done.
        """
        return self.session.declare_queryable(key_expr, handler)

    def queryable_async(self, suffix: str, handler, max_workers: int = 4) -> AsyncQueryable:
        """Declare a queryable at ``topic(suffix)`` with handler in a thread pool.

        Use when the handler does slow work. Zenoh's internal thread is freed
        immediately; the handler runs in a ``ThreadPoolExecutor``::

            def on_db_query(query: zenoh.Query) -> None:
                rows = db.fetch(query.payload.to_string())  # slow
                query.reply(query.key_expr, json.dumps(rows).encode())

            qbl = ctx.queryable_async("device_data", on_db_query)
            # call qbl.undeclare() when done to release threads

        **Threading contract:** multiple invocations may run concurrently.
        Protect shared state with locks.
        """
        from .subscriber import AsyncQueryable

        return AsyncQueryable(self.session, self.topic(suffix), handler, max_workers)

    def queryable_raw_async(self, key_expr: str, handler, max_workers: int = 4) -> AsyncQueryable:
        """Declare a queryable at a literal key expression with handler in a thread pool.

        Same as ``queryable_async()`` but uses a literal key expression without the
        ``bubbaloop/{scope}/{machine_id}/`` prefix. Use for wildcard queryables.
        Call ``undeclare()`` on the returned object when done to release threads.
        """
        from .subscriber import AsyncQueryable

        return AsyncQueryable(self.session, key_expr, handler, max_workers)

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
